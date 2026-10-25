//! Finestra impostazioni (Slint, renderer software: nessuna GPU). L'interfaccia e' descritta in ui/settings.slint.
//! Il thread grafico parte al primo clic sull'icona nella tray; la finestra viene creata a ogni apertura e
//! distrutta alla chiusura, quindi da chiusa resta solo un thread in attesa (nessuna CPU).

use crate::providers::{self, Provider};
use crate::{anthropic, config, oai, release, updater, usage};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::sync::{Condvar, Mutex, Once};
use std::time::Duration;

slint::include_modules!();

const GITHUB_TOKEN_URL: &str = "https://github.com/settings/personal-access-tokens/new";
const REFRESH: Duration = Duration::from_secs(3);

/// Servizi nell'ordine mostrato dalle tessere, con il flag "usato".
static LAYOUT: Mutex<Vec<(Provider, bool)>> = Mutex::new(Vec::new());
/// Esito dell'ultima azione, "occupato" e contatore per svuotare il campo chiave, per servizio.
static TILE_UI: Mutex<Vec<(Provider, String, bool, i32)>> = Mutex::new(Vec::new());
/// Titoli e testi dei prompt (stessi indici della lista a discesa).
static CASES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static READY: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

thread_local! {
    static WIN: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

struct Ui {
    win: SettingsWindow,
    _timer: Timer,
}

/// Apre la finestra (o la porta in primo piano se e' gia' aperta).
pub fn show() {
    static START: Once = Once::new();
    START.call_once(|| {
        let _ = std::thread::Builder::new().name("gui".into()).spawn(gui_thread);
        let (lock, cv) = &READY;
        let _ = cv.wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(5), |ready| !*ready);
    });
    if slint::invoke_from_event_loop(open_window).is_err() {
        config::log("interfaccia grafica non disponibile (thread terminato)");
    }
}

fn gui_thread() {
    if let Err(e) = slint::BackendSelector::new().backend_name("winit".into()).renderer_name("software".into()).select() {
        config::log(&format!("impossibile avviare l'interfaccia grafica: {e}"));
        return;
    }
    Timer::single_shot(Duration::ZERO, || {
        let (lock, cv) = &READY;
        *lock.lock().unwrap() = true;
        cv.notify_all();
    });
    // la finestra si crea e si chiude piu' volte: il ciclo non deve terminare con l'ultima finestra
    if let Err(e) = slint::run_event_loop_until_quit() {
        config::log(&format!("interfaccia grafica terminata: {e}"));
    }
}

fn open_window() {
    let existing = WIN.with(|w| {
        if let Some(ui) = w.borrow().as_ref() {
            let _ = ui.win.show();
            ui.win.window().set_minimized(false);
            true
        } else {
            false
        }
    });
    if existing {
        return;
    }
    match create_window() {
        Ok(ui) => WIN.with(|w| *w.borrow_mut() = Some(ui)),
        Err(e) => config::log(&format!("impossibile aprire le impostazioni: {e}")),
    }
}

fn create_window() -> Result<Ui, slint::PlatformError> {
    let win = SettingsWindow::new()?;
    let cfg = config::load();

    *LAYOUT.lock().unwrap() = initial_layout(&cfg.providers);
    *TILE_UI.lock().unwrap() = providers::ALL.iter().map(|p| (*p, String::new(), false, 0)).collect();

    let cases = cfg.prompt_cases();
    win.set_prompt_titles(ModelRc::new(VecModel::from(cases.iter().map(|(t, _)| SharedString::from(t.as_str())).collect::<Vec<_>>())));
    win.set_prompt_text(cases.first().map(|(_, t)| norm(t)).unwrap_or_default().into());
    *CASES.lock().unwrap() = cases;
    win.set_tiles(ModelRc::new(VecModel::<Tile>::default()));
    win.set_ver_info(update_info_initial().into());
    win.set_token_status(token_status().into());
    refresh(&win);
    wire(&win);

    let weak = win.as_weak();
    win.window().on_close_requested(move || {
        let _ = &weak;
        // distrugge la finestra (e il suo timer) appena il ciclo e' libero
        Timer::single_shot(Duration::ZERO, || WIN.with(|w| drop(w.borrow_mut().take())));
        slint::CloseRequestResponse::HideWindow
    });

    let weak = win.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, REFRESH, move || {
        if let Some(w) = weak.upgrade() {
            refresh(&w);
        }
    });
    check_update_in_background(win.as_weak());
    win.show()?;
    Ok(Ui { win, _timer: timer })
}

// ---------- testo e dati (parti pure, con test)

/// Slint va a capo con "\n": i testi dei moduli usano anche "\r\n".
fn norm(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Ordine iniziale delle tessere: i servizi in uso nell'ordine di priorita', poi gli altri.
fn initial_layout(order: &[String]) -> Vec<(Provider, bool)> {
    let enabled = providers::parse_order(order);
    providers::tile_order(&enabled).into_iter().map(|p| (p, enabled.contains(&p))).collect()
}

fn enabled_order(layout: &[(Provider, bool)]) -> Vec<Provider> {
    layout.iter().filter(|(_, on)| *on).map(|(p, _)| *p).collect()
}

struct TileData {
    id: String,
    name: String,
    enabled: bool,
    has_key: bool,
    ready: bool,
    status: String,
    key_note: String,
    stt_model: String,
    llm_model: String,
    models_editable: bool,
    supports_stt: bool,
    info: String,
    result: String,
    busy: bool,
    clear_seq: i32,
}

/// Contenuto della tessera di un servizio. `status` e' (pronto, testo) e `usage` la riga dei contatori.
fn tile_data(p: Provider, cfg: &config::Config, enabled: bool, key: Option<&str>, status: (bool, String), usage: &str) -> TileData {
    let (stt, llm, editable) = match p {
        Provider::Groq => (cfg.stt_models().join(", "), cfg.llm_models().join(", "), false),
        Provider::OpenAi => (cfg.openai_stt_model.clone(), cfg.openai_llm_model.clone(), true),
        Provider::Anthropic => (String::new(), cfg.anthropic_llm_model.clone(), true),
    };
    let mut info = usage.to_string();
    if !p.supports_stt() {
        info.push_str(" Solo correzione del testo: non trascrive l'audio.");
    }
    TileData {
        id: p.id().into(),
        name: p.name().into(),
        enabled,
        has_key: key.is_some(),
        ready: status.0,
        status: status.1,
        key_note: key.map_or(String::new(), |k| format!("Chiave salvata: {}. Per cambiarla incollane una nuova.", config::mask_key(k))),
        stt_model: stt,
        llm_model: llm,
        models_editable: editable,
        supports_stt: p.supports_stt(),
        info,
        result: String::new(),
        busy: false,
        clear_seq: 0,
    }
}

impl From<TileData> for Tile {
    fn from(t: TileData) -> Tile {
        Tile {
            id: t.id.into(),
            name: t.name.into(),
            enabled: t.enabled,
            has_key: t.has_key,
            ready: t.ready,
            status: t.status.into(),
            key_note: t.key_note.into(),
            stt_model: t.stt_model.into(),
            llm_model: t.llm_model.into(),
            models_editable: t.models_editable,
            supports_stt: t.supports_stt,
            info: t.info.into(),
            result: t.result.into(),
            busy: t.busy,
            clear_seq: t.clear_seq,
        }
    }
}

/// Testi della Home.
struct HomeData {
    ai: String,
    stt: String,
    order: String,
    keys: String,
    hotkey: String,
}

fn model_of(cfg: &config::Config, p: Provider, stt: bool) -> String {
    match (p, stt) {
        (Provider::Groq, true) => cfg.stt_model.clone(),
        (Provider::Groq, false) => cfg.llm_model.clone(),
        (Provider::OpenAi, true) => cfg.openai_stt_model.clone(),
        (Provider::OpenAi, false) => cfg.openai_llm_model.clone(),
        (Provider::Anthropic, _) => cfg.anthropic_llm_model.clone(),
    }
}

fn home_data(cfg: &config::Config, order: &[Provider], has_key: impl Fn(Provider) -> bool + Copy) -> HomeData {
    let pick = |stt: bool| match providers::first_active(order, has_key, stt) {
        Some(p) => format!("{}\n{}", model_of(cfg, p, stt), p.name()),
        None => "Nessuna chiave configurata".to_string(),
    };
    HomeData {
        ai: pick(false),
        stt: pick(true),
        order: order.iter().enumerate().map(|(i, p)| format!("{}. {}", i + 1, p.name())).collect::<Vec<_>>().join("   "),
        keys: providers::ALL
            .iter()
            .map(|p| format!("{}: {}", p.name(), if has_key(*p) { "chiave presente" } else { "nessuna chiave" }))
            .collect::<Vec<_>>()
            .join("  |  "),
        hotkey: format!(
            "{}\n{}, lingua {}",
            cfg.hotkey,
            if cfg.is_toggle() { "premi per iniziare e fermare" } else { "tieni premuto" },
            cfg.language
        ),
    }
}

/// Una riga per servizio in uso: stato e contatori dall'avvio (senza la frase sul credito).
fn home_usage(order: &[Provider], status: impl Fn(Provider) -> (bool, String), usage: impl Fn(Provider) -> String) -> String {
    order
        .iter()
        .map(|p| format!("{}: {}  -  {}", p.name(), status(*p).1, usage(*p).split(". ").next().unwrap_or("").trim_end_matches('.')))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Esito di una verifica chiave: per ogni modello atteso dice se l'account lo vede.
fn model_lines(wanted: &[&str], ids: &[String]) -> Vec<String> {
    wanted
        .iter()
        .map(|m| format!("{m}: {}", if ids.iter().any(|i| i == m) { "disponibile" } else { "NON disponibile nel tuo account" }))
        .collect()
}

// ---------- aggiornamento della finestra

fn tile_ui(p: Provider) -> (String, bool, i32) {
    TILE_UI.lock().unwrap().iter().find(|x| x.0 == p).map_or((String::new(), false, 0), |x| (x.1.clone(), x.2, x.3))
}

fn set_tile_ui(p: Provider, f: impl FnOnce(&mut (Provider, String, bool, i32))) {
    if let Some(x) = TILE_UI.lock().unwrap().iter_mut().find(|x| x.0 == p) {
        f(x);
    }
}

fn build_tiles(cfg: &config::Config) -> Vec<Tile> {
    LAYOUT
        .lock()
        .unwrap()
        .iter()
        .map(|(p, on)| {
            let key = config::provider_key(*p);
            let status = providers::status(*p, key.is_some());
            let (result, busy, seq) = tile_ui(*p);
            let mut t = tile_data(*p, cfg, *on, key.as_deref(), status, &providers::usage_line(*p));
            t.result = norm(&result);
            t.busy = busy;
            t.clear_seq = seq;
            t.into()
        })
        .collect()
}

/// Aggiorna le righe del modello una per una: cosi' le tessere gia' create (e il testo che si sta scrivendo
/// nei loro campi) non vengono ricostruite.
fn sync_tiles(win: &SettingsWindow, new: Vec<Tile>) {
    let model = win.get_tiles();
    let Some(vm) = model.as_any().downcast_ref::<VecModel<Tile>>() else { return };
    for (i, t) in new.iter().enumerate() {
        if i < vm.row_count() {
            if vm.row_data(i).as_ref() != Some(t) {
                vm.set_row_data(i, t.clone());
            }
        } else {
            vm.push(t.clone());
        }
    }
    while vm.row_count() > new.len() {
        vm.remove(vm.row_count() - 1);
    }
}

fn refresh(win: &SettingsWindow) {
    let cfg = config::load();
    sync_tiles(win, build_tiles(&cfg));
    let order = enabled_order(&LAYOUT.lock().unwrap());
    let has_key = |p: Provider| config::provider_key(p).is_some();
    let h = home_data(&cfg, &order, has_key);
    win.set_home_ai(h.ai.into());
    win.set_home_stt(h.stt.into());
    win.set_home_order(h.order.clone().into());
    win.set_home_keys(h.keys.into());
    win.set_home_usage(home_usage(&order, |p| providers::status(p, config::provider_key(p).is_some()), providers::usage_line).into());
    win.set_home_hotkey(h.hotkey.into());
    win.set_home_version(version_line().into());
    win.set_home_updates(update_state_line().into());
    win.set_order_note(format!("Priorita' attuale: {}.", order.iter().map(|p| p.name()).collect::<Vec<_>>().join(" > ")).into());
    let usage_text = norm(&usage::report());
    if win.get_usage_text().as_str() != usage_text {
        win.set_usage_text(usage_text.into());
    }
    win.set_overlay(config::overlay_enabled());
    win.set_debug(config::debug_enabled());
    win.set_debug_text(cfg.debug_log_text);
    win.set_auto_update(config::auto_update_enabled());
    win.set_log_line(format!("Log: {}", config::log_path().display()).into());
    let exe = std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default();
    win.set_general_info(
        format!(
            "Programma: {exe}\nConfigurazione e chiavi: {}\nLog: {}\n\nTrascrittore audio registra dal microfono predefinito di Windows mentre usi la scorciatoia, trascrive e corregge il testo con i servizi AI scelti e lo incolla dove hai il cursore. Sta nella tray e usa poche risorse.",
            config::dir().display(),
            config::log_path().display()
        )
        .into(),
    );
}

fn refresh_in_loop(weak: &slint::Weak<SettingsWindow>) {
    let _ = weak.upgrade_in_event_loop(|w| refresh(&w));
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("explorer").arg(url).spawn();
}

fn provider_by_id(id: &str) -> Option<Provider> {
    Provider::parse(id)
}

fn save_layout() {
    config::set_providers(&enabled_order(&LAYOUT.lock().unwrap()));
}

// ---------- collegamento dei pulsanti

fn wire(win: &SettingsWindow) {
    let weak = win.as_weak();

    win.on_open_keys(|id| {
        if let Some(p) = provider_by_id(&id) {
            open_url(p.keys_url());
        }
    });
    win.on_open_billing(|id| {
        if let Some(p) = provider_by_id(&id) {
            open_url(p.billing_url());
        }
    });
    win.on_open_github_link(|| open_url(GITHUB_TOKEN_URL));
    win.on_open_log(config::open_log);
    win.on_open_folder(|| {
        let _ = std::process::Command::new("explorer").arg(config::dir()).spawn();
    });

    win.on_toggle_overlay(config::set_overlay);
    win.on_toggle_debug({
        let weak = weak.clone();
        move |on| {
            config::set_debug(on);
            refresh_in_loop(&weak);
        }
    });
    win.on_toggle_debug_text(config::set_debug_log_text);
    win.on_toggle_auto_update({
        let weak = weak.clone();
        move |on| {
            config::set_auto_update(on);
            if let Some(w) = weak.upgrade() {
                w.set_ver_info(update_info_initial().into());
            }
            check_update_in_background(weak.clone());
        }
    });

    win.on_select_prompt(|i| {
        let text = CASES.lock().unwrap().get(i.max(0) as usize).map(|(_, t)| norm(t));
        if let Some(t) = text {
            WIN.with(|w| {
                if let Some(ui) = w.borrow().as_ref() {
                    ui.win.set_prompt_text(t.into());
                }
            });
        }
    });

    win.on_reorder({
        let weak = weak.clone();
        move |from, to| {
            {
                let mut l = LAYOUT.lock().unwrap();
                providers::move_item(&mut l, from.max(0) as usize, to.max(0) as usize);
            }
            save_layout();
            // un testo scritto in una tessera non deve finire in un'altra dopo lo spostamento
            for p in providers::ALL {
                set_tile_ui(p, |x| x.3 += 1);
            }
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
        }
    });

    win.on_toggle_use({
        let weak = weak.clone();
        move |id, on| {
            let Some(p) = provider_by_id(&id) else { return };
            {
                let mut l = LAYOUT.lock().unwrap();
                if let Some(e) = l.iter_mut().find(|e| e.0 == p) {
                    e.1 = on;
                }
                if !l.iter().any(|e| e.1) {
                    if let Some(e) = l.iter_mut().find(|e| e.0 == p) {
                        e.1 = true; // ne serve almeno uno
                    }
                    drop(l);
                    set_tile_ui(p, |x| x.1 = "Almeno un servizio deve restare in uso.".into());
                }
            }
            save_layout();
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
        }
    });

    win.on_save_models({
        let weak = weak.clone();
        move |id, stt, llm| {
            let Some(p) = provider_by_id(&id) else { return };
            match p {
                Provider::OpenAi => config::set_provider_models(&stt, &llm, ""),
                Provider::Anthropic => config::set_provider_models("", "", &llm),
                Provider::Groq => {}
            }
            set_tile_ui(p, |x| x.1 = "Modelli salvati: valgono dalla prossima dettatura. Campi vuoti o con spazi ignorati.".into());
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
        }
    });

    win.on_remove_key({
        let weak = weak.clone();
        move |id| {
            let Some(p) = provider_by_id(&id) else { return };
            let msg = match config::save_provider_key(p, "") {
                Ok(()) => format!("Chiave {} rimossa.", p.name()),
                Err(e) => format!("Impossibile rimuovere la chiave: {e}"),
            };
            set_tile_ui(p, |x| x.1 = msg);
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
        }
    });

    win.on_save_key({
        let weak = weak.clone();
        move |id, typed| {
            let Some(p) = provider_by_id(&id) else { return };
            set_tile_ui(p, |x| {
                x.1 = "Verifica in corso...".into();
                x.2 = true;
            });
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
            let typed = typed.trim().to_string();
            let weak = weak.clone();
            std::thread::spawn(move || {
                let msg = verify_provider(p, typed);
                let saved = msg.contains("salvata");
                set_tile_ui(p, |x| {
                    x.1 = msg;
                    x.2 = false;
                    if saved {
                        x.3 += 1;
                    }
                });
                refresh_in_loop(&weak);
            });
        }
    });

    win.on_save_token({
        let weak = weak.clone();
        move |typed| {
            let typed = typed.trim().to_string();
            if let Some(w) = weak.upgrade() {
                w.set_token_busy(true);
                w.set_token_status("Verifica in corso...".into());
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let (msg, update) = verify_and_save_token(typed);
                let saved = msg.starts_with("Token valido e salvato");
                let _ = weak.upgrade_in_event_loop(move |w| {
                    w.set_token_status(norm(&msg).into());
                    w.set_token_busy(false);
                    if saved {
                        w.set_token_clear_seq(w.get_token_clear_seq() + 1);
                    }
                    w.set_ver_info(update_info_initial().into());
                });
                if let Some(token) = update {
                    std::thread::sleep(Duration::from_secs(3)); // il tempo di leggere il messaggio
                    let _ = updater::check_and_apply(&token);
                }
            });
        }
    });

    win.on_clear_token({
        let weak = weak.clone();
        move || {
            let msg = match config::clear_update_token() {
                Ok(()) => token_status(),
                Err(e) => format!("Impossibile rimuovere il token: {e}"),
            };
            if let Some(w) = weak.upgrade() {
                w.set_token_status(msg.into());
                w.set_ver_info(update_info_initial().into());
            }
        }
    });

    win.on_check_update({
        let weak = weak.clone();
        move || manual_update(weak.clone(), false)
    });
    win.on_apply_update({
        let weak = weak.clone();
        move || manual_update(weak.clone(), true)
    });
}

// ---------- versione e aggiornamenti

/// "Versione installata: 2026.10.25.3 (build abc1234)" oppure "locale (build locale)".
fn version_line() -> String {
    let build = release::current_commit().map_or("build locale".to_string(), |c| format!("build {}", release::short(c)));
    let version = release::current_version().map_or("locale".to_string(), |v| v.to_string());
    format!("Versione installata: {version} ({build})")
}

/// Esito del controllo online (senza installare nulla), come testo per la finestra.
fn update_report(token: &str) -> String {
    match updater::fetch_latest(token) {
        Ok(r) if release::needs_update(release::current_version(), r.version) => format!(
            "{}\nAggiornamento disponibile: {} (installata {}).",
            version_line(),
            r.version,
            release::current_version().map_or("locale".to_string(), |v| v.to_string())
        ),
        Ok(r) if release::current_version().is_none() => {
            format!("{}\nBuild locale: gli aggiornamenti automatici non si applicano (online c'e' la {}).", version_line(), r.version)
        }
        Ok(r) => format!("{}\nSei aggiornato: l'ultima versione online e' la {}.", version_line(), r.version),
        Err(e) => format!("{}\nControllo aggiornamenti non riuscito: {e}", version_line()),
    }
}

/// "Verifica aggiornamenti" (apply = false) e "Aggiorna ora" (apply = true): lavorano in un thread.
fn manual_update(weak: slint::Weak<SettingsWindow>, apply: bool) {
    let Some(token) = config::update_token() else {
        if let Some(w) = weak.upgrade() {
            w.set_upd_result("Nessun token GitHub: inseriscilo qui sotto, oppure scarica a mano il nuovo installer dalla pagina Release.".into());
        }
        return;
    };
    if let Some(w) = weak.upgrade() {
        w.set_upd_busy(true);
        w.set_upd_result("Controllo in corso...".into());
    }
    std::thread::spawn(move || {
        let post = |text: String, done: bool| {
            let _ = weak.upgrade_in_event_loop(move |w| {
                w.set_upd_result(text.clone().into());
                w.set_ver_info(text.into());
                if done {
                    w.set_upd_busy(false);
                }
            });
        };
        let report = update_report(&token);
        let newer = updater::fetch_latest(&token).map_or(false, |r| release::needs_update(release::current_version(), r.version));
        if apply && newer {
            post(format!("{report}\nScarico e installo: il programma si riavvia tra pochi secondi (Windows puo' chiedere i permessi di amministratore)."), false);
            std::thread::sleep(Duration::from_secs(3)); // il tempo di leggere il messaggio
            match updater::check_and_apply(&token) {
                Ok(_) => post(format!("{report}\nGia' aggiornato."), true),
                Err(e) => post(format!("{report}\nAggiornamento non riuscito: {e}"), true),
            }
        } else {
            post(report, true);
        }
    });
}

/// Stato del token GitHub.
fn token_status() -> String {
    match config::update_token() {
        Some(t) => format!("Token salvato: {}. Per cambiarlo incolla quello nuovo e premi \"Verifica e salva\".", config::mask_key(&t)),
        None => "Nessun token salvato: gli aggiornamenti automatici non sono disponibili. Incolla un token qui sopra (\"Crea token\" apre la pagina), oppure aggiorna a mano scaricando il nuovo installer dalla pagina Release.".to_string(),
    }
}

/// Stato degli aggiornamenti automatici in una riga.
fn update_state_line() -> &'static str {
    match (config::update_token().is_some(), config::auto_update_enabled()) {
        (false, _) => "Aggiornamenti automatici: non disponibili (nessun token GitHub).",
        (true, false) => "Aggiornamenti automatici: disattivati.",
        (true, true) => "Aggiornamenti automatici: attivi.",
    }
}

/// Versione e stato degli aggiornamenti (sezione Info).
fn update_info_initial() -> String {
    let state = match (config::update_token().is_some(), config::auto_update_enabled()) {
        (false, _) => "Aggiornamenti automatici: non disponibili (nessun token GitHub). Per aggiornare scarica a mano il nuovo installer dalla pagina Release.",
        (true, false) => "Aggiornamenti automatici: DISATTIVATI (attivali qui sotto).",
        (true, true) => "Aggiornamenti automatici: attivi. Controllo in corso...",
    };
    format!("{}\n{state}", version_line())
}

/// Se c'e' un token, chiede a GitHub se esiste una versione piu' recente (senza installarla).
fn check_update_in_background(weak: slint::Weak<SettingsWindow>) {
    let Some(token) = config::update_token() else { return };
    if !config::auto_update_enabled() {
        return;
    }
    std::thread::spawn(move || {
        let text = update_report(&token);
        let _ = weak.upgrade_in_event_loop(move |w| w.set_ver_info(text.into()));
    });
}

// ---------- verifica chiavi e token (rete)

/// Prova la chiave di un servizio elencando i modelli e, se funziona e ne e' stata scritta una nuova, la salva.
fn verify_provider(p: Provider, typed: String) -> String {
    let key = if typed.is_empty() {
        match config::provider_key(p) {
            Some(k) => k,
            None => return format!("Inserisci la chiave API {}.", p.name()),
        }
    } else {
        typed.clone()
    };
    if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return "La chiave non deve contenere spazi.".into();
    }
    let listed = match p {
        Provider::Anthropic => anthropic::Anthropic::new(key.clone()).and_then(|a| a.list_models().map_err(|e| e.to_string())),
        Provider::Groq => oai::Oai::groq(key.clone()).and_then(|g| g.list_models().map_err(|e| e.to_string())),
        Provider::OpenAi => oai::Oai::new(p, key.clone()).and_then(|o| o.list_models().map_err(|e| e.to_string())),
    };
    let ids = match listed {
        Ok(v) => v,
        Err(e) => return format!("Chiave {} NON valida o rete non raggiungibile.\n{e}", p.name()),
    };
    let saved = !typed.is_empty();
    if saved {
        if let Err(e) = config::save_provider_key(p, &key) {
            return format!("La chiave funziona ma non e' stato possibile salvarla: {e}");
        }
    }
    let cfg = config::load();
    let wanted: Vec<&str> = match p {
        Provider::Groq => vec![cfg.stt_model.as_str(), cfg.llm_model.as_str()],
        Provider::OpenAi => vec![cfg.openai_llm_model.as_str(), cfg.openai_stt_model.as_str()],
        Provider::Anthropic => vec![cfg.anthropic_llm_model.as_str()],
    };
    format!("Chiave {} valida{}.\n{}", p.name(), if saved { " e salvata" } else { "" }, model_lines(&wanted, &ids).join("\n"))
}

/// Prova il token su GitHub e, se funziona e ne e' stato scritto uno nuovo, lo salva in .env.
/// Il secondo valore e' il token se c'e' un aggiornamento da installare subito.
fn verify_and_save_token(typed: String) -> (String, Option<String>) {
    let token = if typed.is_empty() {
        match config::update_token() {
            Some(t) => t,
            None => return ("Inserisci il token GitHub (vedi README).".into(), None),
        }
    } else {
        typed.clone()
    };
    if token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return ("Il token non deve contenere spazi.".into(), None);
    }
    let rel = match updater::fetch_latest(&token) {
        Ok(r) => r,
        Err(e) => return (format!("Token non utilizzabile.\n{e}"), None),
    };
    let saved = !typed.is_empty();
    if saved {
        if let Err(e) = config::save_update_token(&token) {
            return (format!("Il token funziona ma non e' stato possibile salvarlo: {e}"), None);
        }
    }
    let head = format!("Token valido{}. Aggiornamenti automatici attivi.", if saved { " e salvato" } else { "" });
    let newer = release::needs_update(release::current_version(), rel.version);
    if newer && !config::auto_update_enabled() {
        (format!("{head}\nC'e' una versione piu' recente ({}), ma gli aggiornamenti automatici sono disattivati: spunta la casella qui sopra, oppure usa \"Aggiorna ora\".", rel.version), None)
    } else if newer {
        (format!("{head}\nVersione online {} piu' recente: la scarico e riavvio il programma tra pochi secondi.", rel.version), Some(token))
    } else {
        (format!("{head}\n{}\nSei gia' all'ultima versione.", version_line()), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(l: &[(Provider, bool)]) -> Vec<(&'static str, bool)> {
        l.iter().map(|(p, on)| (p.id(), *on)).collect()
    }

    #[test]
    fn layout_starts_with_enabled_services_in_priority_order() {
        let l = initial_layout(&["openai".into(), "groq".into()]);
        assert_eq!(names(&l), vec![("openai", true), ("groq", true), ("anthropic", false)]);
        assert_eq!(enabled_order(&l), vec![Provider::OpenAi, Provider::Groq]);
        // config vuota o sbagliata: solo Groq in uso
        assert_eq!(names(&initial_layout(&[])), vec![("groq", true), ("openai", false), ("anthropic", false)]);
    }

    #[test]
    fn dragging_a_tile_changes_the_saved_order() {
        let mut l = initial_layout(&["groq".into(), "openai".into(), "anthropic".into()]);
        providers::move_item(&mut l, 2, 0);
        assert_eq!(enabled_order(&l), vec![Provider::Anthropic, Provider::Groq, Provider::OpenAi]);
        l[1].1 = false;
        assert_eq!(enabled_order(&l), vec![Provider::Anthropic, Provider::OpenAi]);
    }

    #[test]
    fn newlines_are_normalized_for_the_ui() {
        assert_eq!(norm("a\r\nb\nc"), "a\nb\nc");
    }

    #[test]
    fn tile_shows_models_key_and_status_per_service() {
        let cfg = config::Config::default();
        let t = tile_data(Provider::Groq, &cfg, true, Some("gsk_abcd1234"), (true, "Pronto".into()), "Dall'avvio: 0 chiamate, 0 token.");
        assert!(t.has_key && t.ready && !t.models_editable && t.supports_stt);
        assert!(t.key_note.contains("...1234"));
        assert!(t.llm_model.contains("openai/gpt-oss-120b") && t.llm_model.contains("gpt-oss-20b"));
        let a = tile_data(Provider::Anthropic, &cfg, false, None, (false, "Nessuna chiave".into()), "x");
        assert!(!a.has_key && a.models_editable && !a.supports_stt && a.stt_model.is_empty());
        assert!(a.info.contains("Solo correzione del testo"));
        assert_eq!(a.llm_model, cfg.anthropic_llm_model);
        assert!(a.key_note.is_empty());
    }

    #[test]
    fn home_shows_the_model_that_will_answer() {
        let cfg = config::Config::default();
        let order = [Provider::Anthropic, Provider::OpenAi, Provider::Groq];
        let h = home_data(&cfg, &order, |p| p != Provider::OpenAi);
        assert!(h.ai.starts_with(&cfg.anthropic_llm_model) && h.ai.contains("Anthropic"));
        // Anthropic non trascrive: la trascrizione passa al primo servizio che la sa fare
        assert!(h.stt.starts_with(&cfg.stt_model) && h.stt.contains("Groq"));
        assert_eq!(h.order, "1. Anthropic   2. OpenAI   3. Groq");
        assert!(h.keys.contains("OpenAI: nessuna chiave") && h.keys.contains("Groq: chiave presente"));
        assert!(h.hotkey.starts_with("ctrl+win"));
        let none = home_data(&cfg, &order, |_| false);
        assert_eq!(none.ai, "Nessuna chiave configurata");
    }

    #[test]
    fn home_usage_has_one_line_per_service() {
        let r = home_usage(
            &[Provider::Groq, Provider::OpenAi],
            |p| (true, if p == Provider::Groq { "Pronto".into() } else { "Nessuna chiave".into() }),
            |_| "Dall'avvio: 2 chiamate, 10 token. Il credito residuo non e' leggibile via API.".into(),
        );
        assert_eq!(r, "Groq: Pronto  -  Dall'avvio: 2 chiamate, 10 token\nOpenAI: Nessuna chiave  -  Dall'avvio: 2 chiamate, 10 token");
    }

    #[test]
    fn model_lines_flag_missing_models() {
        let ids = vec!["a".to_string(), "b".to_string()];
        assert_eq!(model_lines(&["a", "z"], &ids), vec!["a: disponibile", "z: NON disponibile nel tuo account"]);
    }
}
