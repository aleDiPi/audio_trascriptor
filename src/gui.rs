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
const TIMER_USAGE: usize = 1;
const WM_VERIFIED: u32 = WM_APP + 1;
const WM_TOKEN_DONE: u32 = WM_APP + 2;
const WM_UPDATE_INFO: u32 = WM_APP + 3;

static RESULT: Mutex<Option<String>> = Mutex::new(None);
static TOKEN_RESULT: Mutex<Option<String>> = Mutex::new(None);
static UPDATE_INFO: Mutex<Option<String>> = Mutex::new(None);

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
        let (ww, wh) = (656, 830);
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

unsafe fn child(parent: HWND, class: &str, text: &str, style: u32, rect: (i32, i32, i32, i32), id: i32) -> HWND {
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
    h
}

unsafe fn build(hwnd: HWND) {
    let cfg = config::load();
    child(hwnd, "STATIC", "Chiave API Groq:", 0, (16, 16, 300, 18), 0);
    child(hwnd, "EDIT", "", (ES_PASSWORD | ES_AUTOHSCROLL) as u32 | WS_TABSTOP, (16, 38, 470, 24), ID_KEY);
    child(hwnd, "BUTTON", "Verifica e salva", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 36, 128, 28), ID_SAVE);
    let status = match config::groq_key() {
        Some(k) => format!("Chiave salvata: {}. Per cambiarla scrivi quella nuova e premi il pulsante.", config::mask_key(&k)),
        None => "Nessuna chiave configurata. Incolla la chiave (https://console.groq.com/keys) e premi il pulsante.".into(),
    };
    child(hwnd, "STATIC", &status, 0, (16, 72, 608, 64), ID_STATUS);
    child(hwnd, "STATIC", "Token GitHub per gli aggiornamenti automatici (facoltativo):", 0, (16, 140, 450, 18), 0);
    child(hwnd, "EDIT", "", (ES_PASSWORD | ES_AUTOHSCROLL) as u32 | WS_TABSTOP, (16, 162, 470, 24), ID_TOKEN);
    child(hwnd, "BUTTON", "Verifica e salva", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 160, 128, 28), ID_TOKEN_SAVE);
    child(hwnd, "STATIC", &update_status_initial(), 0, (16, 194, 608, 48), ID_UPDATE);
    let info = format!(
        "Modelli in uso (i predefiniti Groq di OpenWhispr):  trascrizione {}  |  AI {}",
        cfg.stt_model, cfg.llm_model
    );
    child(hwnd, "STATIC", &info, 0, (16, 248, 608, 34), 0);
    child(hwnd, "STATIC", "Utilizzo Groq (si aggiorna da solo):", 0, (16, 288, 400, 18), 0);
    child(
        hwnd,
        "EDIT",
        &usage_text(),
        (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32 | WS_VSCROLL,
        (16, 310, 608, 130),
        ID_USAGE,
    );
    let fb = format!(
        "Se il limite gratuito si esaurisce passo in automatico a: AI {} | trascrizione {}. Si cambiano in config.toml.",
        cfg.fallback_models.join(", "),
        cfg.stt_fallback_models.join(", ")
    );
    child(hwnd, "STATIC", &fb, 0, (16, 448, 470, 34), 0);
    child(hwnd, "BUTTON", "Apri cartella config", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 446, 128, 28), ID_FOLDER);
    child(hwnd, "STATIC", "Prompt inviati all'AI, per ogni caso:", 0, (16, 488, 400, 18), 0);
    let report = cfg.prompt_report().replace("\r\n", "\n").replace('\n', "\r\n");
    child(
        hwnd,
        "EDIT",
        &report,
        (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32 | WS_VSCROLL | WS_TABSTOP,
        (16, 510, 608, 262),
        ID_PROMPTS,
    );
    SetTimer(hwnd, TIMER_USAGE, 3000, None);
    check_update_in_background(hwnd);
}

/// "Versione 0.1.0 (build abc1234)" oppure "(build locale)".
fn version_line() -> String {
    let build = release::current_commit().map_or("build locale".to_string(), |c| format!("build {}", release::short(c)));
    format!("Versione installata: {} ({build})", env!("CARGO_PKG_VERSION"))
}

fn update_status_initial() -> String {
    if config::update_token().is_some() {
        format!("{}\r\nAggiornamenti automatici: attivi. Controllo in corso...", version_line())
    } else {
        format!(
            "{}\r\nAggiornamenti automatici: disattivati (nessun token). Per aggiornare scarica a mano il nuovo installer dalla pagina Release.",
            version_line()
        )
    }
}

/// Se c'e' un token, chiede a GitHub se esiste una versione piu' recente (senza installarla).
unsafe fn check_update_in_background(hwnd: HWND) {
    let Some(token) = config::update_token() else { return };
    let h = hwnd as usize;
    std::thread::spawn(move || {
        let text = match updater::fetch_latest(&token) {
            Ok(r) if release::needs_update(release::current_commit(), &r.commit) => format!(
                "{}\r\nAggiornamento disponibile ({}): verra' installato in automatico entro poche ore, oppure al prossimo avvio.",
                version_line(),
                release::short(&r.commit)
            ),
            Ok(_) if release::current_commit().is_none() => {
                format!("{}\r\nBuild locale: gli aggiornamenti automatici non si applicano.", version_line())
            }
            Ok(_) => format!("{}\r\nSei aggiornato all'ultima versione.", version_line()),
            Err(e) => format!("{}\r\nControllo aggiornamenti non riuscito: {e}", version_line()),
        };
        *UPDATE_INFO.lock().unwrap() = Some(text);
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
        Err(e) => return (format!("Token NON valido o senza accesso alla release.\r\n{e}"), None),
    };
    let saved = !typed.is_empty();
    if saved {
        if let Err(e) = config::save_update_token(&token) {
            return (format!("Il token funziona ma non e' stato possibile salvarlo: {e}"), None);
        }
    }
    let head = format!("Token valido{}. Aggiornamenti automatici attivi.", if saved { " e salvato" } else { "" });
    if release::needs_update(release::current_commit(), &rel.commit) {
        (format!("{head}\r\nVersione online {} piu' recente: la scarico e riavvio il programma tra pochi secondi.", release::short(&rel.commit)), Some(token))
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
                ID_FOLDER => {
                    let _ = std::process::Command::new("explorer").arg(config::dir()).spawn();
                }
                _ => {}
            }
            0
        }
        WM_TIMER => {
            if wp == TIMER_USAGE {
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
                set_text(GetDlgItem(hwnd, ID_UPDATE), &text);
            }
            0
        }
        WM_CLOSE => {
            KillTimer(hwnd, TIMER_USAGE);
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
