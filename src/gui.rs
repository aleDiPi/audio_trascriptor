//! Finestra impostazioni (Win32 nativo, nessuna libreria grafica). Viene creata solo quando si clicca
//! l'icona nella tray e distrutta alla chiusura: quando e' chiusa non occupa risorse.

use crate::{config, groq, usage};
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
const TIMER_USAGE: usize = 1;
const WM_VERIFIED: u32 = WM_APP + 1;

static RESULT: Mutex<Option<String>> = Mutex::new(None);

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
        let (ww, wh) = (656, 740);
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
    let info = format!(
        "Modelli in uso (scelti come i predefiniti Groq di OpenWhispr):  trascrizione {}  |  AI {}",
        cfg.stt_model, cfg.llm_model
    );
    child(hwnd, "STATIC", &info, 0, (16, 142, 608, 34), 0);
    child(hwnd, "STATIC", "Utilizzo Groq (si aggiorna da solo):", 0, (16, 184, 400, 18), 0);
    child(
        hwnd,
        "EDIT",
        &usage_text(),
        (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32 | WS_VSCROLL,
        (16, 206, 608, 140),
        ID_USAGE,
    );
    let fb = format!(
        "Se il limite gratuito si esaurisce passo in automatico a: AI {} | trascrizione {}. Si cambiano in config.toml.",
        cfg.fallback_models.join(", "),
        cfg.stt_fallback_models.join(", ")
    );
    child(hwnd, "STATIC", &fb, 0, (16, 354, 470, 34), 0);
    child(hwnd, "BUTTON", "Apri cartella config", BS_PUSHBUTTON as u32 | WS_TABSTOP, (496, 352, 128, 28), ID_FOLDER);
    child(hwnd, "STATIC", "Prompt inviati all'AI, per ogni caso:", 0, (16, 394, 400, 18), 0);
    let report = cfg.prompt_report().replace("\r\n", "\n").replace('\n', "\r\n");
    child(
        hwnd,
        "EDIT",
        &report,
        (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32 | WS_VSCROLL | WS_TABSTOP,
        (16, 416, 608, 272),
        ID_PROMPTS,
    );
    SetTimer(hwnd, TIMER_USAGE, 3000, None);
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
        WM_CLOSE => {
            KillTimer(hwnd, TIMER_USAGE);
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
