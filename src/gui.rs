//! Finestra impostazioni (Slint, renderer software: nessuna GPU). L'interfaccia e' descritta in ui/settings.slint.
//! Il thread grafico parte al primo clic sull'icona nella tray; la finestra viene creata a ogni apertura e
//! distrutta alla chiusura, quindi da chiusa resta solo un thread in attesa (nessuna CPU).

use crate::i18n::{self, t, tf};
use crate::providers::{self, Provider};
use crate::{anthropic, config, localstt, oai, platform, release, sounds, updater, usage};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::sync::{Condvar, Mutex, Once};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

slint::include_modules!();
include!(concat!(env!("OUT_DIR"), "/tr_apply.rs"));

const GITHUB_TOKEN_URL: &str = "https://github.com/settings/personal-access-tokens/new";
const REFRESH: Duration = Duration::from_secs(3);

/// Servizi nell'ordine mostrato dalle tessere, con il flag "usato".
static LAYOUT: Mutex<Vec<(Provider, bool)>> = Mutex::new(Vec::new());
/// Esito dell'ultima azione, "occupato" e contatore per svuotare il campo chiave, per servizio.
static TILE_UI: Mutex<Vec<(Provider, String, bool, i32)>> = Mutex::new(Vec::new());
/// Titoli e testi dei prompt (stessi indici della lista a discesa).
static CASES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
/// Download del modello locale in corso: (dimensione, scaricati, totale).
static DL: Mutex<Option<(localstt::Size, u64, Option<u64>)>> = Mutex::new(None);
static CANCEL: AtomicBool = AtomicBool::new(false);
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
        config::log("GUI not available (thread ended)");
    }
}

fn gui_thread() {
    if let Err(e) = slint::BackendSelector::new().backend_name("winit".into()).renderer_name("software".into()).select() {
        config::log(&format!("cannot start the GUI: {e}"));
        return;
    }
    Timer::single_shot(Duration::ZERO, || {
        let (lock, cv) = &READY;
        *lock.lock().unwrap() = true;
        cv.notify_all();
    });
    // la finestra si crea e si chiude piu' volte: il ciclo non deve terminare con l'ultima finestra
    if let Err(e) = slint::run_event_loop_until_quit() {
        config::log(&format!("GUI ended: {e}"));
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
        Err(e) => config::log(&format!("cannot open the settings: {e}")),
    }
}

fn create_window() -> Result<Ui, slint::PlatformError> {
    let win = SettingsWindow::new()?;
    let cfg = config::load();
    apply_ui_texts(&win, &t);
    set_language_list(&win, &cfg.ui_language);

    *LAYOUT.lock().unwrap() = initial_layout(&cfg.providers);
    *TILE_UI.lock().unwrap() = providers::ALL.iter().map(|p| (*p, String::new(), false, 0)).collect();

    let cases = cfg.prompt_cases();
    win.set_prompt_titles(ModelRc::new(VecModel::from(cases.iter().map(|(t, _)| SharedString::from(t.as_str())).collect::<Vec<_>>())));
    win.set_prompt_text(cases.first().map(|(_, t)| norm(t)).unwrap_or_default().into());
    *CASES.lock().unwrap() = cases;
    win.set_tiles(ModelRc::new(VecModel::<Tile>::default()));
    win.set_local_size_labels(ModelRc::new(VecModel::from(localstt::SIZES.iter().map(|z| SharedString::from(z.label())).collect::<Vec<_>>())));
    win.set_ver_info(update_info_initial().into());
    win.set_token_status(token_status().into());
    // i controlli dei suoni si impostano solo qui: aggiornarli col timer sposterebbe il cursore mentre lo si trascina
    win.set_linux(cfg!(target_os = "linux"));
    win.set_autostart(platform::autostart_enabled());
    win.set_sounds(config::sounds_enabled());
    win.set_sound_volume(config::sound_volume() as f32);
    win.set_sound_style_index(style_index(&config::sound_style()));
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

// ---------- language

/// Entries of the language list: automatic first, then every available language by its own name.
fn language_choices() -> Vec<(String, String)> {
    i18n::choices(&i18n::languages(), &t("ui.language_auto"))
}

fn set_language_list(win: &SettingsWindow, setting: &str) {
    let choices = language_choices();
    win.set_lang_index(i18n::choice_index(setting, &choices) as i32);
    win.set_lang_names(ModelRc::new(VecModel::from(choices.into_iter().map(|(_, n)| SharedString::from(n)).collect::<Vec<_>>())));
}

/// After a language change: rewrites every text of the open window.
fn relocalize(win: &SettingsWindow) {
    let cfg = config::load();
    apply_ui_texts(win, &t);
    set_language_list(win, &cfg.ui_language);
    let cases = cfg.prompt_cases();
    win.set_prompt_titles(ModelRc::new(VecModel::from(cases.iter().map(|(t, _)| SharedString::from(t.as_str())).collect::<Vec<_>>())));
    win.set_prompt_text(cases.first().map(|(_, t)| norm(t)).unwrap_or_default().into());
    *CASES.lock().unwrap() = cases;
    win.set_local_size_labels(ModelRc::new(VecModel::from(localstt::SIZES.iter().map(|z| SharedString::from(z.label())).collect::<Vec<_>>())));
    win.set_ver_info(update_info_initial().into());
    win.set_token_status(token_status().into());
    refresh(win);
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
    is_local: bool,
    local_index: i32,
    downloading: bool,
    progress: f32,
    local_detail: String,
}

/// Contenuto della tessera di un servizio. `status` e' (pronto, testo) e `usage` la riga dei contatori.
fn tile_data(p: Provider, cfg: &config::Config, enabled: bool, key: Option<&str>, available: bool, status: (bool, String), usage: &str) -> TileData {
    let (stt, llm, editable) = match p {
        Provider::Local => (format!("whisper {}", cfg.local_model), String::new(), false),
        Provider::Groq => (cfg.stt_models().join(", "), cfg.llm_models().join(", "), false),
        Provider::OpenAi => (cfg.openai_stt_model.clone(), cfg.openai_llm_model.clone(), true),
        Provider::Anthropic => (String::new(), cfg.anthropic_llm_model.clone(), true),
    };
    let mut info = usage.to_string();
    if !p.supports_stt() {
        info.push(' ');
        info.push_str(&t("gui.tile_text_only"));
    }
    if p.is_local() {
        info = tf("gui.tile_local_info", &[("usage", usage)]);
    }
    TileData {
        id: p.id().into(),
        name: p.name().into(),
        enabled,
        has_key: available,
        ready: status.0,
        status: status.1,
        key_note: key.map_or(String::new(), |k| tf("gui.key_saved", &[("key", &config::mask_key(k))])),
        stt_model: stt,
        llm_model: llm,
        models_editable: editable,
        supports_stt: p.supports_stt(),
        info,
        result: String::new(),
        busy: false,
        clear_seq: 0,
        is_local: p.is_local(),
        local_index: localstt::SIZES.iter().position(|z| *z == localstt::Size::parse(&cfg.local_model)).unwrap_or(1) as i32,
        downloading: false,
        progress: -1.0,
        local_detail: String::new(),
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
            is_local: t.is_local,
            local_index: t.local_index,
            downloading: t.downloading,
            progress: t.progress,
            local_detail: t.local_detail.into(),
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
        (Provider::Local, _) => format!("whisper {}", cfg.local_model),
    }
}

fn home_data(cfg: &config::Config, order: &[Provider], ready: impl Fn(Provider) -> bool + Copy) -> HomeData {
    let pick = |stt: bool| match providers::first_active(order, ready, stt) {
        Some(p) => format!("{}\n{}", model_of(cfg, p, stt), p.name()),
        None if !stt && providers::first_active(order, ready, true).is_some() => t("gui.home_no_correction"),
        None => t("gui.home_no_key"),
    };
    HomeData {
        ai: pick(false),
        stt: pick(true),
        order: order.iter().enumerate().map(|(i, p)| format!("{}. {}", i + 1, p.name())).collect::<Vec<_>>().join("   "),
        keys: providers::ALL
            .iter()
            .map(|p| {
                let (yes, no) = if p.is_local() { ("gui.home_model_yes", "gui.home_model_no") } else { ("gui.home_key_yes", "gui.home_key_no") };
                format!("{}: {}", p.name(), t(if ready(*p) { yes } else { no }))
            })
            .collect::<Vec<_>>()
            .join("  |  "),
        hotkey: tf(
            "gui.home_hotkey",
            &[
                ("hotkey", &cfg.hotkey),
                ("mode", &t(if cfg.is_toggle() { "gui.mode_toggle" } else { "gui.mode_hold" })),
                ("lang", if cfg.language.trim().is_empty() { "auto" } else { cfg.language.trim() }),
            ],
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

/// Riga sotto la tendina del modello locale: scaricato o no, e spazio occupato.
fn local_detail(size: localstt::Size, downloaded: bool, disk: u64) -> String {
    let disk_note = if disk > 0 { tf("gui.local_disk", &[("size", &localstt::format_bytes(disk))]) } else { String::new() };
    if downloaded {
        tf("gui.local_ready", &[("id", size.id()), ("disk", &disk_note)])
    } else {
        tf("gui.local_missing", &[("id", size.id()), ("size", &localstt::format_bytes(size.approx_bytes())), ("disk", &disk_note)])
    }
}

/// Posizione di uno stile nell'elenco a discesa dei suoni.
fn style_index(id: &str) -> i32 {
    let style = sounds::Style::parse(id);
    sounds::STYLES.iter().position(|s| *s == style).unwrap_or(0) as i32
}

/// Esito di una verifica chiave: per ogni modello atteso dice se l'account lo vede.
fn model_lines(wanted: &[&str], ids: &[String]) -> Vec<String> {
    wanted
        .iter()
        .map(|m| format!("{m}: {}", t(if ids.iter().any(|i| i == m) { "gui.model_available" } else { "gui.model_unavailable" })))
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
            let available = config::provider_ready(*p);
            let status = providers::status(*p, available);
            let (result, busy, seq) = tile_ui(*p);
            let mut t = tile_data(*p, cfg, *on, key.as_deref(), available, status, &providers::usage_line(*p));
            t.result = norm(&result);
            t.busy = busy;
            t.clear_seq = seq;
            if p.is_local() {
                let size = localstt::Size::parse(&cfg.local_model);
                let dl = *DL.lock().unwrap();
                t.downloading = dl.is_some();
                t.progress = dl.and_then(|(_, done, total)| localstt::fraction(done, total)).unwrap_or(-1.0);
                t.local_detail = match dl {
                    Some((z, done, total)) => tf(
                        "gui.local_downloading",
                        &[
                            ("id", z.id()),
                            ("done", &localstt::format_bytes(done)),
                            ("total", &total.map_or(String::new(), |n| tf("gui.local_of_total", &[("total", &localstt::format_bytes(n))]))),
                        ],
                    ),
                    None => local_detail(size, available, localstt::disk_usage(config::dir())),
                };
            }
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
    let h = home_data(&cfg, &order, config::provider_ready);
    win.set_home_ai(h.ai.into());
    win.set_home_stt(h.stt.into());
    win.set_home_order(h.order.clone().into());
    win.set_home_keys(h.keys.into());
    win.set_home_usage(home_usage(&order, |p| providers::status(p, config::provider_ready(p)), providers::usage_line).into());
    win.set_home_hotkey(h.hotkey.into());
    win.set_home_version(version_line().into());
    win.set_home_updates(update_state_line().into());
    win.set_order_note(tf("gui.order_note", &[("order", &order.iter().map(|p| p.name()).collect::<Vec<_>>().join(" > "))]).into());
    let usage_text = norm(&usage::report());
    if win.get_usage_text().as_str() != usage_text {
        win.set_usage_text(usage_text.into());
    }
    win.set_overlay(config::overlay_enabled());
    win.set_debug(config::debug_enabled());
    win.set_debug_text(cfg.debug_log_text);
    win.set_auto_update(config::auto_update_enabled());
    win.set_log_line(tf("gui.log_line", &[("path", &config::log_path().display().to_string())]).into());
    let exe = std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default();
    win.set_general_info(
        tf(
            "gui.general_info",
            &[("exe", &exe), ("dir", &config::dir().display().to_string()), ("log", &config::log_path().display().to_string())],
        )
        .into(),
    );
}

fn refresh_in_loop(weak: &slint::Weak<SettingsWindow>) {
    let _ = weak.upgrade_in_event_loop(|w| refresh(&w));
}

fn open_url(url: &str) {
    platform::open(url);
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
        if let Some(p) = provider_by_id(&id).filter(|p| !p.is_local()) {
            open_url(p.keys_url());
        }
    });
    win.on_open_billing(|id| {
        if let Some(p) = provider_by_id(&id).filter(|p| !p.is_local()) {
            open_url(p.billing_url());
        }
    });
    win.on_open_github_link(|| open_url(GITHUB_TOKEN_URL));
    win.on_open_log(config::open_log);
    win.on_open_folder(|| {
        platform::open(config::dir());
    });

    win.on_toggle_overlay(config::set_overlay);
    win.on_toggle_sounds(config::set_sounds);
    win.on_toggle_autostart(platform::set_autostart);
    win.on_set_sound_style(|i| {
        let style = sounds::STYLES.get(i.max(0) as usize).copied().unwrap_or(sounds::Style::Soft);
        config::set_sound_style(style.id());
        sounds::play_with(sounds::Kind::Start, style, config::sound_volume());
    });
    win.on_set_sound_volume(|v| {
        config::set_sound_volume(v as i64);
        sounds::play_with(sounds::Kind::Start, sounds::Style::parse(&config::sound_style()), config::sound_volume());
    });
    win.on_preview_sound(|| {
        // inizio e poi fine, con le impostazioni attuali
        let style = sounds::Style::parse(&config::sound_style());
        let vol = config::sound_volume();
        sounds::play_with(sounds::Kind::Start, style, vol);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(sounds::duration_ms(sounds::Kind::Start, style) as u64 + 500));
            sounds::play_with(sounds::Kind::End, style, vol);
        });
    });
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

    win.on_set_language({
        let weak = weak.clone();
        move |i| {
            let choices = language_choices();
            let Some((code, _)) = choices.get(i.max(0) as usize) else { return };
            config::set_ui_language(code);
            i18n::init(code, Some(&config::dir().join("lang")));
            if let Some(w) = weak.upgrade() {
                relocalize(&w);
            }
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
                    set_tile_ui(p, |x| x.1 = t("gui.min_one_service"));
                }
            }
            save_layout();
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
        }
    });

    win.on_set_local_size({
        let weak = weak.clone();
        move |i| {
            if let Some(z) = localstt::SIZES.get(i.max(0) as usize) {
                config::set_local_model(*z);
            }
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
        }
    });

    win.on_delete_local({
        let weak = weak.clone();
        move || {
            let size = config::local_size();
            localstt::delete(config::dir(), size);
            set_tile_ui(Provider::Local, |x| x.1 = tf("gui.model_deleted", &[("id", size.id())]));
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
        }
    });

    win.on_cancel_download(|| CANCEL.store(true, Ordering::Relaxed));

    win.on_download_local({
        let weak = weak.clone();
        move || {
            let size = config::local_size();
            {
                let mut dl = DL.lock().unwrap();
                if dl.is_some() || localstt::is_downloaded(config::dir(), size) {
                    return;
                }
                *dl = Some((size, 0, None));
            }
            CANCEL.store(false, Ordering::Relaxed);
            set_tile_ui(Provider::Local, |x| x.1 = tf("gui.model_downloading", &[("id", size.id())]));
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let mut last = Instant::now();
                let r = localstt::download(config::dir(), size, &CANCEL, |done, total| {
                    *DL.lock().unwrap() = Some((size, done, total));
                    if last.elapsed() >= Duration::from_millis(300) {
                        last = Instant::now();
                        refresh_in_loop(&weak);
                    }
                });
                *DL.lock().unwrap() = None;
                let msg = match r {
                    Ok(()) => tf("gui.model_done", &[("id", size.id())]),
                    Err(e) => tf("gui.download_failed", &[("err", &e)]),
                };
                config::log(&msg);
                set_tile_ui(Provider::Local, |x| x.1 = msg);
                refresh_in_loop(&weak);
            });
        }
    });

    win.on_save_models({
        let weak = weak.clone();
        move |id, stt, llm| {
            let Some(p) = provider_by_id(&id) else { return };
            match p {
                Provider::OpenAi => config::set_provider_models(&stt, &llm, ""),
                Provider::Anthropic => config::set_provider_models("", "", &llm),
                Provider::Groq | Provider::Local => {}
            }
            set_tile_ui(p, |x| x.1 = t("gui.models_saved"));
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
                Ok(()) => tf("gui.key_removed", &[("name", p.name())]),
                Err(e) => tf("gui.key_remove_failed", &[("err", &e.to_string())]),
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
                x.1 = t("gui.verifying");
                x.2 = true;
            });
            if let Some(w) = weak.upgrade() {
                refresh(&w);
            }
            let typed = typed.trim().to_string();
            let weak = weak.clone();
            std::thread::spawn(move || {
                let (msg, saved) = verify_provider(p, typed);
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
                w.set_token_status(t("gui.verifying").into());
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let (msg, update, saved) = verify_and_save_token(typed);
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
                Err(e) => tf("gui.token_remove_failed", &[("err", &e.to_string())]),
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
    let build = release::current_commit().map_or(t("gui.build_local"), |c| tf("gui.build_id", &[("id", release::short(c))]));
    let version = release::current_version().map_or(t("gui.version_local"), |v| v.to_string());
    tf("gui.version_line", &[("version", &version), ("build", &build)])
}

/// Esito del controllo online (senza installare nulla), come testo per la finestra.
fn update_report(token: &str) -> String {
    match updater::fetch_latest(token) {
        Ok(r) if release::needs_update(release::current_version(), r.version) => tf(
            "gui.update_available",
            &[
                ("line", &version_line()),
                ("new", &r.version.to_string()),
                ("current", &release::current_version().map_or(t("gui.version_local"), |v| v.to_string())),
            ],
        ),
        Ok(r) if release::current_version().is_none() => tf("gui.update_local_build", &[("line", &version_line()), ("new", &r.version.to_string())]),
        Ok(r) => tf("gui.update_current", &[("line", &version_line()), ("new", &r.version.to_string())]),
        Err(e) => tf("gui.update_check_failed", &[("line", &version_line()), ("err", &e)]),
    }
}

/// "Verifica aggiornamenti" (apply = false) e "Aggiorna ora" (apply = true): lavorano in un thread.
fn manual_update(weak: slint::Weak<SettingsWindow>, apply: bool) {
    let Some(token) = config::update_token() else {
        if let Some(w) = weak.upgrade() {
            w.set_upd_result(t("gui.upd_no_token").into());
        }
        return;
    };
    if let Some(w) = weak.upgrade() {
        w.set_upd_busy(true);
        w.set_upd_result(t("gui.upd_checking").into());
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
            let how = t(if cfg!(target_os = "linux") { "gui.upd_how_linux" } else { "gui.upd_how_windows" });
            post(tf("gui.upd_installing", &[("report", &report), ("how", &how)]), false);
            std::thread::sleep(Duration::from_secs(3)); // il tempo di leggere il messaggio
            match updater::check_and_apply(&token) {
                Ok(_) => post(tf("gui.upd_already", &[("report", &report)]), true),
                Err(e) => post(tf("gui.upd_failed", &[("report", &report), ("err", &e)]), true),
            }
        } else {
            post(report, true);
        }
    });
}

/// Stato del token GitHub.
fn token_status() -> String {
    match config::update_token() {
        Some(tok) => tf("gui.token_saved", &[("token", &config::mask_key(&tok))]),
        None => t("gui.token_none"),
    }
}

/// Stato degli aggiornamenti automatici in una riga.
fn update_state_line() -> String {
    t(match (config::update_token().is_some(), config::auto_update_enabled()) {
        (false, _) => "gui.upd_state_unavailable",
        (true, false) => "gui.upd_state_off",
        (true, true) => "gui.upd_state_on",
    })
}

/// Versione e stato degli aggiornamenti (sezione Info).
fn update_info_initial() -> String {
    let state = t(match (config::update_token().is_some(), config::auto_update_enabled()) {
        (false, _) => "gui.upd_info_unavailable",
        (true, false) => "gui.upd_info_off",
        (true, true) => "gui.upd_info_on",
    });
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
/// Returns the message and whether a new key was saved.
fn verify_provider(p: Provider, typed: String) -> (String, bool) {
    let key = if typed.is_empty() {
        match config::provider_key(p) {
            Some(k) => k,
            None => return (tf("gui.key_enter", &[("name", p.name())]), false),
        }
    } else {
        typed.clone()
    };
    if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return (t("gui.key_no_spaces"), false);
    }
    let listed = match p {
        Provider::Anthropic => anthropic::Anthropic::new(key.clone()).and_then(|a| a.list_models().map_err(|e| e.to_string())),
        Provider::Groq => oai::Oai::groq(key.clone()).and_then(|g| g.list_models().map_err(|e| e.to_string())),
        Provider::OpenAi => oai::Oai::new(p, key.clone()).and_then(|o| o.list_models().map_err(|e| e.to_string())),
        Provider::Local => return (t("gui.key_local"), false),
    };
    let ids = match listed {
        Ok(v) => v,
        Err(e) => return (tf("gui.key_invalid", &[("name", p.name()), ("err", &e)]), false),
    };
    let saved = !typed.is_empty();
    if saved {
        if let Err(e) = config::save_provider_key(p, &key) {
            return (tf("gui.key_save_failed", &[("err", &e.to_string())]), false);
        }
    }
    let cfg = config::load();
    let wanted: Vec<&str> = match p {
        Provider::Groq => vec![cfg.stt_model.as_str(), cfg.llm_model.as_str()],
        Provider::OpenAi => vec![cfg.openai_llm_model.as_str(), cfg.openai_stt_model.as_str()],
        Provider::Anthropic => vec![cfg.anthropic_llm_model.as_str()],
        Provider::Local => vec![],
    };
    let msg = tf(if saved { "gui.key_valid_saved" } else { "gui.key_valid" }, &[("name", p.name()), ("lines", &model_lines(&wanted, &ids).join("\n"))]);
    (msg, saved)
}

/// Prova il token su GitHub e, se funziona e ne e' stato scritto uno nuovo, lo salva in .env.
/// Returns the message, the token if there is an update to install right now, and whether a new token was saved.
fn verify_and_save_token(typed: String) -> (String, Option<String>, bool) {
    let token = if typed.is_empty() {
        match config::update_token() {
            Some(t) => t,
            None => return (t("gui.token_enter"), None, false),
        }
    } else {
        typed.clone()
    };
    if token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return (t("gui.token_no_spaces"), None, false);
    }
    let rel = match updater::fetch_latest(&token) {
        Ok(r) => r,
        Err(e) => return (tf("gui.token_unusable", &[("err", &e)]), None, false),
    };
    let saved = !typed.is_empty();
    if saved {
        if let Err(e) = config::save_update_token(&token) {
            return (tf("gui.token_save_failed", &[("err", &e.to_string())]), None, false);
        }
    }
    let head = t(if saved { "gui.token_valid_saved" } else { "gui.token_valid" });
    let newer = release::needs_update(release::current_version(), rel.version);
    if newer && !config::auto_update_enabled() {
        (tf("gui.token_newer_off", &[("head", &head), ("new", &rel.version.to_string())]), None, saved)
    } else if newer {
        (tf("gui.token_newer_on", &[("head", &head), ("new", &rel.version.to_string())]), Some(token), saved)
    } else {
        (tf("gui.token_latest", &[("head", &head), ("line", &version_line())]), None, saved)
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
        assert_eq!(names(&l), vec![("openai", true), ("groq", true), ("anthropic", false), ("local", false)]);
        assert_eq!(enabled_order(&l), vec![Provider::OpenAi, Provider::Groq]);
        // config vuota o sbagliata: solo Groq in uso
        assert_eq!(names(&initial_layout(&[])), vec![("groq", true), ("openai", false), ("anthropic", false), ("local", false)]);
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
        let t = tile_data(Provider::Groq, &cfg, true, Some("gsk_abcd1234"), true, (true, "Pronto".into()), "Dall'avvio: 0 chiamate, 0 token.");
        assert!(t.has_key && t.ready && !t.models_editable && t.supports_stt);
        assert!(t.key_note.contains("...1234"));
        assert!(t.llm_model.contains("openai/gpt-oss-120b") && t.llm_model.contains("gpt-oss-20b"));
        let a = tile_data(Provider::Anthropic, &cfg, false, None, false, (false, "No key".into()), "x");
        assert!(!a.has_key && a.models_editable && !a.supports_stt && a.stt_model.is_empty());
        assert!(a.info.contains("Text correction only"));
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
        assert!(h.keys.contains("OpenAI: no key") && h.keys.contains("Groq: key present"));
        assert!(h.hotkey.starts_with("ctrl+win"));
        let none = home_data(&cfg, &order, |_| false);
        assert_eq!(none.ai, "No key configured");
    }

    #[test]
    fn home_shows_local_whisper_when_no_cloud_service_is_ready() {
        let cfg = config::Config::default();
        let order = [Provider::Groq, Provider::OpenAi, Provider::Local];
        let h = home_data(&cfg, &order, |p| p == Provider::Local);
        assert!(h.stt.starts_with("whisper base") && h.stt.contains("Local (Whisper)"));
        assert_eq!(h.ai, "None: text is not corrected", "local does not correct the text");
        assert!(h.keys.contains("Local (Whisper): model downloaded") && h.keys.contains("Groq: no key"));
        let h = home_data(&cfg, &order, |_| false);
        assert!(h.keys.contains("Local (Whisper): model not downloaded"));
    }

    #[test]
    fn local_tile_is_about_the_model_not_a_key() {
        let cfg = config::Config { local_model: "small".into(), ..config::Config::default() };
        let t = tile_data(Provider::Local, &cfg, true, None, true, (true, "Ready".into()), "Since start: 2 transcriptions.");
        assert!(t.is_local && t.has_key && !t.models_editable);
        assert_eq!(t.local_index, 2);
        assert!(t.info.contains("no data leaves it") && t.info.contains("raw transcription"));
        assert!(t.key_note.is_empty());
    }

    #[test]
    fn local_detail_says_what_to_do() {
        let d = local_detail(localstt::Size::Base, false, 0);
        assert!(d.contains("not downloaded yet") && d.contains("142 MB") && d.contains("Download"));
        let d = local_detail(localstt::Size::Small, true, 466_000_000);
        assert!(d.contains("small downloaded and ready") && d.contains("466 MB"));
    }

    #[test]
    fn home_usage_has_one_line_per_service() {
        let r = home_usage(
            &[Provider::Groq, Provider::OpenAi],
            |p| (true, if p == Provider::Groq { "Ready".into() } else { "No key".into() }),
            |_| "Since start: 2 calls, 10 tokens. The remaining credit cannot be read via API.".into(),
        );
        assert_eq!(r, "Groq: Ready  -  Since start: 2 calls, 10 tokens\nOpenAI: No key  -  Since start: 2 calls, 10 tokens");
    }

    #[test]
    fn sound_style_index_matches_the_dropdown_order() {
        assert_eq!((style_index("soft"), style_index("click"), style_index("chime")), (0, 1, 2));
        assert_eq!(style_index("boh"), 0);
    }

    #[test]
    fn model_lines_flag_missing_models() {
        let ids = vec!["a".to_string(), "b".to_string()];
        assert_eq!(model_lines(&["a", "z"], &ids), vec!["a: available", "z: NOT available in your account"]);
    }
}
