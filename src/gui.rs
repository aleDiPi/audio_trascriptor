//! Finestra impostazioni (Win32 nativo, nessuna libreria grafica). Viene creata solo quando si clicca
//! l'icona nella tray e distrutta alla chiusura: quando e' chiusa non occupa risorse.

use crate::{config, groq, release, updater, usage};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const ID_KEY: i32 = 101;
const ID_SAVE: i32 = 102;
const ID_STATUS: i32 = 103;
const ID_PROMPTS: i32 = 105;
const ID_USAGE: i32 = 106;
const ID_FOLDER: i32 = 107;
const ID_TOKEN: i32 = 108;
const ID_TOKEN_SAVE: i32 = 109;
const ID_UPDATE: i32 = 110;
const ID_DEBUG: i32 = 111;
const ID_OPENLOG: i32 = 112;
const ID_GROQ_LINK: i32 = 113;
const ID_GITHUB_LINK: i32 = 114;
const ID_AUTOUPD: i32 = 115;
const ID_TOKEN_CLEAR: i32 = 116;
const ID_VERINFO: i32 = 117;
const ID_DEBUG_TEXT: i32 = 118;
const ID_PROMPT_SEL: i32 = 119;
const ID_CHECK_NOW: i32 = 120;
const ID_UPDATE_NOW: i32 = 121;
const ID_UPD_RESULT: i32 = 122;
const ID_TAB0: i32 = 200; // 200..=204: pulsanti delle sezioni
const TABS: [&str; 5] = ["API", "GitHub", "Impostazioni", "Prompt", "Info"];

const GROQ_KEYS_URL: &str = "https://console.groq.com/keys";
const GITHUB_TOKEN_URL: &str = "https://github.com/settings/personal-access-tokens/new";
const TIMER_USAGE: usize = 1;
const WM_VERIFIED: u32 = WM_APP + 1;
const WM_TOKEN_DONE: u32 = WM_APP + 2;
const WM_UPDATE_INFO: u32 = WM_APP + 3;
const WM_UPD_RESULT: u32 = WM_APP + 4;

static RESULT: Mutex<Option<String>> = Mutex::new(None);
static TOKEN_RESULT: Mutex<Option<String>> = Mutex::new(None);
static UPDATE_INFO: Mutex<Option<String>> = Mutex::new(None);
static UPD_RESULT: Mutex<Option<String>> = Mutex::new(None);
/// Controlli di ogni sezione: (finestra, indice sezione). Si mostra solo quella scelta.
static PAGES: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
/// Titoli e testi dei prompt della sezione Prompt (stessi indici della lista a discesa).
static CASES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Apre la finestra (o la porta in primo piano se e' gia' aperta).
pub fn show() {
    unsafe {
        let class = w("TrascrittoreSettings");
        let existing = FindWindowW(class.as_ptr(), null());
        if !existing.is_null() {
            ShowWindow(existing, SW_RESTORE);
            SetForegroundWindow(existing);
            return;
        }
        let hinst = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as usize as HBRUSH,
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc); // 0 se gia' registrata: va bene
        let (ww, wh) = (656, 600);
        let x = (GetSystemMetrics(SM_CXSCREEN) - ww) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - wh) / 2;
        let title = w("Trascrittore audio - Impostazioni");
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
            x.max(0),
            y.max(0),
            ww,
            wh,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        );
        if !hwnd.is_null() {
            SetForegroundWindow(hwnd);
        }
    }
}

unsafe fn child(parent: HWND, page: usize, class: &str, text: &str, style: u32, rect: (i32, i32, i32, i32), id: i32) -> HWND {
    let (c, t) = (w(class), w(text));
    let h = CreateWindowExW(
        if class == "EDIT" { WS_EX_CLIENTEDGE } else { 0 },
        c.as_ptr(),
        t.as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        rect.0,
        rect.1,
        rect.2,
        rect.3,
        parent,
        id as usize as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(h, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as usize, 1);
    PAGES.lock().unwrap().push((h as usize, page));
    h
}

/// Testo informativo selezionabile e copiabile: casella di sola lettura senza bordo.
unsafe fn info(parent: HWND, page: usize, text: &str, rect: (i32, i32, i32, i32), id: i32) -> HWND {
    let (c, t) = (w("EDIT"), w(text));
    let h = CreateWindowExW(
        0,
        c.as_ptr(),
        t.as_ptr(),
        WS_CHILD | WS_VISIBLE | (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32,
        rect.0,
        rect.1,
        rect.2,
        rect.3,
        parent,
        id as usize as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(h, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as usize, 1);
    PAGES.lock().unwrap().push((h as usize, page));
    h
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("explorer").arg(url).spawn();
}

/// Mostra i controlli della sezione scelta e nasconde gli altri.
unsafe fn show_page(hwnd: HWND, page: usize) {
    for (h, p) in PAGES.lock().unwrap().iter() {
        if *p == usize::MAX {
            continue; // i pulsanti delle sezioni restano sempre visibili
        }
        ShowWindow(*h as HWND, if *p == page { SW_SHOW } else { SW_HIDE });
    }
    for i in 0..TABS.len() {
        SendMessageW(GetDlgItem(hwnd, ID_TAB0 + i as i32), BM_SETCHECK, (i == page) as usize, 0);
    }
}

unsafe fn select_prompt(hwnd: HWND) {
    let i = SendMessageW(GetDlgItem(hwnd, ID_PROMPT_SEL), CB_GETCURSEL, 0, 0);
    let text = CASES.lock().unwrap().get(i.max(0) as usize).map(|(_, t)| t.replace("\r\n", "\n").replace('\n', "\r\n"));
    if let Some(t) = text {
        set_text(GetDlgItem(hwnd, ID_PROMPTS), &t);
    }
}

unsafe fn build(hwnd: HWND) {
    let cfg = config::load();
    PAGES.lock().unwrap().clear();
    // pulsanti delle sezioni
    for (i, name) in TABS.iter().enumerate() {
        let style = (BS_AUTORADIOBUTTON | BS_PUSHLIKE) as u32 | WS_TABSTOP | if i == 0 { WS_GROUP } else { 0 };
        let b = child(hwnd, usize::MAX, "BUTTON", name, style, (16 + i as i32 * 122, 8, 118, 30), ID_TAB0 + i as i32);
        let _ = b;
    }

    // --- 0: API
    let p = 0;
    child(hwnd, p, "STATIC", "Groq (trascrizione e correzione del testo) - chiave API:", 0, (16, 52, 470, 18), 0);
    child(hwnd, p, "BUTTON", "Crea chiave Groq", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 48, 128, 24), ID_GROQ_LINK);
    child(hwnd, p, "EDIT", "", (ES_PASSWORD | ES_AUTOHSCROLL) as u32 | WS_TABSTOP, (16, 76, 470, 24), ID_KEY);
    child(hwnd, p, "BUTTON", "Verifica e salva", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 74, 128, 28), ID_SAVE);
    let status = match config::groq_key() {
        Some(k) => format!("Chiave salvata: {}. Per cambiarla scrivi quella nuova e premi il pulsante.", config::mask_key(&k)),
        None => format!("Nessuna chiave configurata. Incolla la chiave ({GROQ_KEYS_URL}) e premi il pulsante."),
    };
    info(hwnd, p, &status, (16, 108, 608, 50), ID_STATUS);
    let models = format!("Modelli in uso:  trascrizione {}  |  AI {}", cfg.stt_model, cfg.llm_model);
    info(hwnd, p, &models, (16, 160, 608, 34), 0);
    child(hwnd, p, "STATIC", "Utilizzo Groq (si aggiorna da solo):", 0, (16, 200, 400, 18), 0);
    child(
        hwnd,
        p,
        "EDIT",
        &usage_text(),
        (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32 | WS_VSCROLL,
        (16, 222, 608, 120),
        ID_USAGE,
    );
    let fb = format!(
        "Se il limite gratuito si esaurisce passo in automatico a: AI {} | trascrizione {} (si cambiano in config.toml).",
        cfg.fallback_models.join(", "),
        cfg.stt_fallback_models.join(", ")
    );
    info(hwnd, p, &fb, (16, 350, 608, 34), 0);
    info(
        hwnd,
        p,
        "Altri servizi di riserva (per esempio OpenAI): non ancora supportati. Dimmi quali ti servono e li aggiungo con la loro chiave in questa sezione.",
        (16, 392, 608, 50),
        0,
    );

    // --- 1: GitHub
    let p = 1;
    child(hwnd, p, "STATIC", "Token GitHub per gli aggiornamenti (incolla qui per inserirlo o cambiarlo):", 0, (16, 52, 470, 18), 0);
    child(hwnd, p, "BUTTON", "Crea token GitHub", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 48, 128, 24), ID_GITHUB_LINK);
    child(hwnd, p, "EDIT", "", (ES_PASSWORD | ES_AUTOHSCROLL) as u32 | WS_TABSTOP, (16, 76, 470, 24), ID_TOKEN);
    child(hwnd, p, "BUTTON", "Verifica e salva", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 74, 128, 28), ID_TOKEN_SAVE);
    child(hwnd, p, "BUTTON", "Rimuovi token", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 108, 128, 28), ID_TOKEN_CLEAR);
    info(hwnd, p, &token_status(), (16, 108, 470, 120), ID_UPDATE);
    info(
        hwnd,
        p,
        "Come crearlo: il pulsante \"Crea token GitHub\" apre la pagina. Scegli Resource owner aleDiPi, Repository access solo audio_trascriptor e Permissions > Contents = Read-only. Copia il token (inizia con github_pat_) e incollalo qui.\r\nL'attivazione degli aggiornamenti automatici e' nella sezione Impostazioni.",
        (16, 240, 608, 100),
        0,
    );

    // --- 2: Impostazioni
    let p = 2;
    let auto = child(hwnd, p, "BUTTON", "Aggiornamenti automatici attivi (serve il token GitHub)", BS_AUTOCHECKBOX as u32 | WS_TABSTOP, (16, 56, 600, 22), ID_AUTOUPD);
    SendMessageW(auto, BM_SETCHECK, config::auto_update_enabled() as usize, 0);
    child(hwnd, p, "BUTTON", "Verifica aggiornamenti", BS_PUSHBUTTON as u32 | WS_TABSTOP, (16, 86, 170, 28), ID_CHECK_NOW);
    child(hwnd, p, "BUTTON", "Aggiorna ora", BS_PUSHBUTTON as u32 | WS_TABSTOP, (196, 86, 150, 28), ID_UPDATE_NOW);
    info(hwnd, p, "Verifica: controlla se esiste una versione piu' recente. Aggiorna ora: la scarica e la installa subito (anche con gli aggiornamenti automatici spenti).", (16, 120, 608, 50), ID_UPD_RESULT);
    let dbg = child(hwnd, p, "BUTTON", "Modalita' debug: scrive nel log ogni passaggio (mai chiavi o token)", BS_AUTOCHECKBOX as u32 | WS_TABSTOP, (16, 184, 600, 22), ID_DEBUG);
    SendMessageW(dbg, BM_SETCHECK, config::debug_enabled() as usize, 0);
    let dtx = child(
        hwnd,
        p,
        "BUTTON",
        "Nel log anche il testo dettato, il prompt e la risposta dell'AI (privato: solo per cercare un problema)",
        BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        (36, 210, 580, 22),
        ID_DEBUG_TEXT,
    );
    SendMessageW(dtx, BM_SETCHECK, cfg.debug_log_text as usize, 0);
    child(hwnd, p, "BUTTON", "Apri log", BS_PUSHBUTTON as u32 | WS_TABSTOP, (16, 246, 128, 28), ID_OPENLOG);
    child(hwnd, p, "BUTTON", "Apri cartella config", BS_PUSHBUTTON as u32 | WS_TABSTOP, (156, 246, 150, 28), ID_FOLDER);
    info(hwnd, p, &format!("Log: {}", config::log_path().display()), (16, 284, 608, 34), 0);
    info(
        hwnd,
        p,
        "Spegnendo la modalita' debug i file di log vengono cancellati. Scorciatoia, lingua, modelli e prompt si cambiano in config.toml (\"Apri cartella config\"), poi \"Ricarica configurazione\" dal menu dell'icona.",
        (16, 326, 608, 70),
        0,
    );

    // --- 3: Prompt
    let p = 3;
    child(hwnd, p, "STATIC", "Prompt inviati all'AI, per tipo di applicazione:", 0, (16, 52, 470, 18), 0);
    let combo = child(hwnd, p, "COMBOBOX", "", (CBS_DROPDOWNLIST | CBS_HASSTRINGS) as u32 | WS_VSCROLL | WS_TABSTOP, (16, 76, 608, 260), ID_PROMPT_SEL);
    let cases = cfg.prompt_cases();
    for (title, _) in &cases {
        SendMessageW(combo, CB_ADDSTRING, 0, w(title).as_ptr() as LPARAM);
    }
    SendMessageW(combo, CB_SETCURSEL, 0, 0);
    let first = cases.first().map(|(_, t)| t.replace("\r\n", "\n").replace('\n', "\r\n")).unwrap_or_default();
    *CASES.lock().unwrap() = cases;
    child(
        hwnd,
        p,
        "EDIT",
        &first,
        (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32 | WS_VSCROLL | WS_TABSTOP,
        (16, 108, 608, 392),
        ID_PROMPTS,
    );

    // --- 4: Info
    let p = 4;
    info(hwnd, p, &update_info_initial(), (16, 56, 608, 110), ID_VERINFO);
    let exe = std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default();
    let general = format!(
        "Programma: {exe}\r\nConfigurazione e chiavi: {}\r\nLog: {}\r\n\r\nScorciatoia: {} ({}), lingua {}\r\nModelli: trascrizione {}, AI {}\r\n\r\nTrascrittore audio registra dal microfono predefinito di Windows mentre tieni premuta la scorciatoia, trascrive e corregge il testo con Groq e lo incolla dove hai il cursore. Sta nella tray e usa poche risorse.",
        config::dir().display(),
        config::log_path().display(),
        cfg.hotkey,
        if cfg.is_toggle() { "premi per iniziare e fermare" } else { "tieni premuto" },
        cfg.language,
        cfg.stt_model,
        cfg.llm_model
    );
    info(hwnd, p, &general, (16, 176, 608, 250), 0);

    show_page(hwnd, 0);
    SetTimer(hwnd, TIMER_USAGE, 3000, None);
    check_update_in_background(hwnd);
}

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
            "{}\r\nAggiornamento disponibile: {} (installata {}).",
            version_line(),
            r.version,
            release::current_version().map_or("locale".to_string(), |v| v.to_string())
        ),
        Ok(r) if release::current_version().is_none() => {
            format!("{}\r\nBuild locale: gli aggiornamenti automatici non si applicano (online c'e' la {}).", version_line(), r.version)
        }
        Ok(r) => format!("{}\r\nSei aggiornato: l'ultima versione online e' la {}.", version_line(), r.version),
        Err(e) => format!("{}\r\nControllo aggiornamenti non riuscito: {e}", version_line()),
    }
}

/// "Verifica aggiornamenti" (apply = false) e "Aggiorna ora" (apply = true): lavorano in un thread e
/// scrivono l'esito nella sezione Impostazioni e in Info.
unsafe fn manual_update(hwnd: HWND, apply: bool) {
    let Some(token) = config::update_token() else {
        set_text(GetDlgItem(hwnd, ID_UPD_RESULT), "Nessun token GitHub: inseriscilo nella sezione GitHub, oppure scarica a mano il nuovo installer dalla pagina Release.");
        return;
    };
    for id in [ID_CHECK_NOW, ID_UPDATE_NOW] {
        EnableWindow(GetDlgItem(hwnd, id), 0);
    }
    set_text(GetDlgItem(hwnd, ID_UPD_RESULT), "Controllo in corso...");
    let h = hwnd as usize;
    std::thread::spawn(move || {
        let post = |text: String| {
            *UPD_RESULT.lock().unwrap() = Some(text);
            unsafe { PostMessageW(h as HWND, WM_UPD_RESULT, 0, 0) };
        };
        let report = update_report(&token);
        let newer = updater::fetch_latest(&token).map_or(false, |r| release::needs_update(release::current_version(), r.version));
        if apply && newer {
            post(format!("{report}\r\nScarico e installo: il programma si riavvia tra pochi secondi (Windows puo' chiedere i permessi di amministratore)."));
            std::thread::sleep(std::time::Duration::from_secs(3)); // il tempo di leggere il messaggio
            match updater::check_and_apply(&token) {
                Ok(_) => post(format!("{report}\r\nGia' aggiornato.")),
                Err(e) => post(format!("{report}\r\nAggiornamento non riuscito: {e}")),
            }
        } else {
            post(report);
        }
    });
}

/// Stato del token GitHub (sezione GitHub).
fn token_status() -> String {
    match config::update_token() {
        Some(t) => format!("Token salvato: {}. Per cambiarlo incolla quello nuovo e premi \"Verifica e salva\".", config::mask_key(&t)),
        None => "Nessun token salvato: gli aggiornamenti automatici non sono disponibili. Incolla un token qui sopra (\"Crea token GitHub\" apre la pagina), oppure aggiorna a mano scaricando il nuovo installer dalla pagina Release.".to_string(),
    }
}

/// Versione e stato degli aggiornamenti (sezione Info).
fn update_info_initial() -> String {
    let state = match (config::update_token().is_some(), config::auto_update_enabled()) {
        (false, _) => "Aggiornamenti automatici: non disponibili (nessun token GitHub). Per aggiornare scarica a mano il nuovo installer dalla pagina Release.",
        (true, false) => "Aggiornamenti automatici: DISATTIVATI (si attivano nella sezione Impostazioni).",
        (true, true) => "Aggiornamenti automatici: attivi. Controllo in corso...",
    };
    format!("{}\r\n{state}", version_line())
}

/// Se c'e' un token, chiede a GitHub se esiste una versione piu' recente (senza installarla).
unsafe fn check_update_in_background(hwnd: HWND) {
    let Some(token) = config::update_token() else { return };
    if !config::auto_update_enabled() {
        return;
    }
    let h = hwnd as usize;
    std::thread::spawn(move || {
        *UPDATE_INFO.lock().unwrap() = Some(update_report(&token));
        unsafe { PostMessageW(h as HWND, WM_UPDATE_INFO, 0, 0) };
    });
}

fn usage_text() -> String {
    usage::report().replace("\r\n", "\n").replace('\n', "\r\n")
}

unsafe fn get_text(h: HWND) -> String {
    let n = GetWindowTextLengthW(h).max(0) as usize;
    let mut buf = vec![0u16; n + 1];
    let got = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buf[..got])
}

unsafe fn set_text(h: HWND, s: &str) {
    SetWindowTextW(h, w(s).as_ptr());
}

unsafe fn on_token_save(hwnd: HWND) {
    let typed = get_text(GetDlgItem(hwnd, ID_TOKEN)).trim().to_string();
    EnableWindow(GetDlgItem(hwnd, ID_TOKEN_SAVE), 0);
    set_text(GetDlgItem(hwnd, ID_UPDATE), "Verifica in corso...");
    let h = hwnd as usize;
    std::thread::spawn(move || {
        let (msg, update) = verify_and_save_token(typed);
        *TOKEN_RESULT.lock().unwrap() = Some(msg);
        unsafe { PostMessageW(h as HWND, WM_TOKEN_DONE, 0, 0) };
        if let Some(token) = update {
            std::thread::sleep(std::time::Duration::from_secs(3)); // il tempo di leggere il messaggio
            let _ = updater::check_and_apply(&token);
        }
    });
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
        Err(e) => return (format!("Token non utilizzabile.\r\n{e}"), None),
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
        (format!("{head}\r\nC'e' una versione piu' recente ({}), ma gli aggiornamenti automatici sono disattivati: spunta la casella nella sezione Impostazioni, oppure usa \"Aggiorna ora\".", rel.version), None)
    } else if newer {
        (format!("{head}\r\nVersione online {} piu' recente: la scarico e riavvio il programma tra pochi secondi.", rel.version), Some(token))
    } else {
        (format!("{head}\r\n{}\r\nSei gia' all'ultima versione.", version_line()), None)
    }
}

unsafe fn on_save(hwnd: HWND) {
    let typed = get_text(GetDlgItem(hwnd, ID_KEY)).trim().to_string();
    EnableWindow(GetDlgItem(hwnd, ID_SAVE), 0);
    set_text(GetDlgItem(hwnd, ID_STATUS), "Verifica in corso...");
    let h = hwnd as usize;
    std::thread::spawn(move || {
        let msg = verify_and_save(typed);
        *RESULT.lock().unwrap() = Some(msg);
        unsafe { PostMessageW(h as HWND, WM_VERIFIED, 0, 0) };
    });
}

/// Prova la chiave su Groq e, se funziona e ne e' stata scritta una nuova, la salva in .env.
fn verify_and_save(typed: String) -> String {
    let cfg = config::load();
    let key = if typed.is_empty() {
        match config::groq_key() {
            Some(k) => k,
            None => return "Inserisci la chiave API Groq.".into(),
        }
    } else {
        typed.clone()
    };
    if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return "La chiave non deve contenere spazi.".into();
    }
    let listed = groq::Groq::new(key.clone()).and_then(|g| g.list_models().map_err(|e| e.to_string()));
    let ids = match listed {
        Ok(v) => v,
        Err(e) => return format!("Chiave NON valida o rete non raggiungibile.\r\n{e}"),
    };
    let saved = !typed.is_empty();
    if saved {
        if let Err(e) = config::save_groq_key(&key) {
            return format!("La chiave funziona ma non e' stato possibile salvarla: {e}");
        }
    }
    let state = |m: &str| if ids.iter().any(|i| i == m) { "disponibile" } else { "NON disponibile nel tuo account" };
    format!(
        "Chiave valida{}.\r\nTrascrizione {}: {}\r\nAI {}: {}",
        if saved { " e salvata" } else { "" },
        cfg.stt_model,
        state(&cfg.stt_model),
        cfg.llm_model,
        state(&cfg.llm_model)
    )
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            build(hwnd);
            0
        }
        WM_COMMAND => {
            match (wp & 0xFFFF) as i32 {
                ID_SAVE => on_save(hwnd),
                ID_TOKEN_SAVE => on_token_save(hwnd),
                ID_OPENLOG => config::open_log(),
                ID_GROQ_LINK => open_url(GROQ_KEYS_URL),
                ID_AUTOUPD => {
                    let on = SendMessageW(GetDlgItem(hwnd, ID_AUTOUPD), BM_GETCHECK, 0, 0) == 1;
                    config::set_auto_update(on);
                    set_text(GetDlgItem(hwnd, ID_VERINFO), &update_info_initial());
                    check_update_in_background(hwnd);
                }
                ID_TOKEN_CLEAR => {
                    let msg = match config::clear_update_token() {
                        Ok(()) => token_status(),
                        Err(e) => format!("Impossibile rimuovere il token: {e}"),
                    };
                    set_text(GetDlgItem(hwnd, ID_UPDATE), &msg);
                    set_text(GetDlgItem(hwnd, ID_VERINFO), &update_info_initial());
                }
                ID_CHECK_NOW => manual_update(hwnd, false),
                ID_UPDATE_NOW => manual_update(hwnd, true),
                ID_DEBUG_TEXT => {
                    let on = SendMessageW(GetDlgItem(hwnd, ID_DEBUG_TEXT), BM_GETCHECK, 0, 0) == 1;
                    config::set_debug_log_text(on);
                }
                ID_PROMPT_SEL if (wp >> 16) as u32 == CBN_SELCHANGE => select_prompt(hwnd),
                id if (ID_TAB0..ID_TAB0 + TABS.len() as i32).contains(&id) => show_page(hwnd, (id - ID_TAB0) as usize),
                ID_GITHUB_LINK => open_url(GITHUB_TOKEN_URL),
                ID_DEBUG => {
                    let on = SendMessageW(GetDlgItem(hwnd, ID_DEBUG), BM_GETCHECK, 0, 0) == 1;
                    config::set_debug(on);
                }
                ID_FOLDER => {
                    let _ = std::process::Command::new("explorer").arg(config::dir()).spawn();
                }
                _ => {}
            }
            0
        }
        WM_TIMER => {
            if wp == TIMER_USAGE {
                // tiene allineata la casella con il menu della tray
                SendMessageW(GetDlgItem(hwnd, ID_DEBUG), BM_SETCHECK, config::debug_enabled() as usize, 0);
                SendMessageW(GetDlgItem(hwnd, ID_AUTOUPD), BM_SETCHECK, config::auto_update_enabled() as usize, 0);
                let h = GetDlgItem(hwnd, ID_USAGE);
                let t = usage_text();
                if get_text(h) != t {
                    set_text(h, &t); // evita di riportare in cima lo scorrimento se non cambia nulla
                }
            }
            0
        }
        WM_VERIFIED => {
            if let Some(text) = RESULT.lock().unwrap().take() {
                set_text(GetDlgItem(hwnd, ID_STATUS), &text);
                if text.starts_with("Chiave valida e salvata") {
                    set_text(GetDlgItem(hwnd, ID_KEY), "");
                }
            }
            EnableWindow(GetDlgItem(hwnd, ID_SAVE), 1);
            0
        }
        WM_TOKEN_DONE => {
            if let Some(text) = TOKEN_RESULT.lock().unwrap().take() {
                set_text(GetDlgItem(hwnd, ID_UPDATE), &text);
                if text.starts_with("Token valido e salvato") {
                    set_text(GetDlgItem(hwnd, ID_TOKEN), "");
                }
            }
            EnableWindow(GetDlgItem(hwnd, ID_TOKEN_SAVE), 1);
            0
        }
        WM_UPDATE_INFO => {
            if let Some(text) = UPDATE_INFO.lock().unwrap().take() {
                set_text(GetDlgItem(hwnd, ID_VERINFO), &text);
            }
            0
        }
        WM_UPD_RESULT => {
            if let Some(text) = UPD_RESULT.lock().unwrap().take() {
                set_text(GetDlgItem(hwnd, ID_UPD_RESULT), &text);
                set_text(GetDlgItem(hwnd, ID_VERINFO), &text);
            }
            for id in [ID_CHECK_NOW, ID_UPDATE_NOW] {
                EnableWindow(GetDlgItem(hwnd, id), 1);
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            // le caselle di sola lettura (testo selezionabile) hanno lo stesso sfondo bianco della finestra
            SetBkColor(wp as HDC, GetSysColor(COLOR_WINDOW));
            GetSysColorBrush(COLOR_WINDOW) as LRESULT
        }
        WM_CLOSE => {
            KillTimer(hwnd, TIMER_USAGE);
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
