//! Trascrittore audio: dettatura vocale per Windows con icona nella tray.
//! Scorciatoia -> microfono -> Groq Whisper -> Groq LLM (prompt per app) -> incolla.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("Trascrittore funziona solo su Windows.");
}

#[cfg(windows)]
mod audio;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod combo;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod config;
#[cfg(windows)]
mod groq;
#[cfg(windows)]
mod gui;
#[cfg(windows)]
mod hotkey;
#[cfg(windows)]
mod platform;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod usage;

#[cfg(windows)]
pub enum Event {
    HotkeyDown,
    HotkeyUp,
    Reload,
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq)]
enum Ui {
    Idle,
    Recording,
    Working,
    Error,
}

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(windows)]
mod app {
    use super::*;
    use std::ptr::null_mut;
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::{Arc, Mutex};
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub fn run() {
        if !platform::single_instance() {
            return;
        }
        let cfg = config::load();
        apply_hotkey(&cfg.hotkey);

        let (ev_tx, ev_rx) = channel::<Event>();
        let (ui_tx, ui_rx) = channel::<(Ui, String)>();
        let main_tid = unsafe { GetCurrentThreadId() };
        let notify: Arc<dyn Fn(Ui, String) + Send + Sync> = {
            let ui_tx = Mutex::new(ui_tx);
            Arc::new(move |s, t| {
                let _ = ui_tx.lock().unwrap().send((s, t));
                unsafe { PostThreadMessageW(main_tid, WM_APP, 0, 0) };
            })
        };
        hotkey::install(ev_tx.clone());
        std::thread::spawn({
            let notify = notify.clone();
            move || worker(ev_rx, notify)
        });

        let m_auto = CheckMenuItem::new("Avvia con Windows", true, platform::autostart_enabled(), None);
        let m_set = MenuItem::new("Impostazioni...", true, None);
        let m_cfg = MenuItem::new("Apri cartella configurazione", true, None);
        let m_reload = MenuItem::new("Ricarica configurazione", true, None);
        let m_quit = MenuItem::new("Esci", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&m_set, &m_auto, &m_cfg, &m_reload, &PredefinedMenuItem::separator(), &m_quit]);
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(idle_tip(&cfg.hotkey))
            .with_icon(icon(Ui::Idle))
            .build()
            .expect("impossibile creare l'icona nella tray");

        let menu_rx = MenuEvent::receiver();
        let tray_rx = TrayIconEvent::receiver();
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            let r = unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) };
            if r <= 0 {
                break;
            }
            if msg.hwnd.is_null() && msg.message == WM_APP {
                while let Ok((state, text)) = ui_rx.try_recv() {
                    let _ = tray.set_icon(Some(icon(state)));
                    let tip = if state == Ui::Idle { idle_tip(&config::load().hotkey) } else { text };
                    let _ = tray.set_tooltip(Some(tip));
                }
                continue;
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            while let Ok(e) = tray_rx.try_recv() {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                    gui::show();
                }
            }
            while let Ok(e) = menu_rx.try_recv() {
                if e.id == *m_set.id() {
                    gui::show();
                } else if e.id == *m_quit.id() {
                    unsafe { PostQuitMessage(0) };
                } else if e.id == *m_auto.id() {
                    platform::set_autostart(m_auto.is_checked());
                } else if e.id == *m_cfg.id() {
                    let _ = std::process::Command::new("explorer").arg(config::dir()).spawn();
                } else if e.id == *m_reload.id() {
                    let _ = ev_tx.send(Event::Reload);
                }
            }
        }
    }

    fn idle_tip(hotkey: &str) -> String {
        format!("Trascrittore - pronto ({hotkey})")
    }

    fn apply_hotkey(s: &str) {
        match combo::parse(s) {
            Ok(c) => hotkey::set_combo(c),
            Err(e) => config::log(&format!("hotkey '{s}' non valida ({e}), uso ctrl+win")),
        }
    }

    fn icon(state: Ui) -> Icon {
        let (r, g, b) = match state {
            Ui::Idle => (60, 120, 220),
            Ui::Recording => (220, 40, 40),
            Ui::Working => (240, 160, 30),
            Ui::Error => (140, 140, 140),
        };
        let n = 32i32;
        let mut px = Vec::with_capacity((n * n * 4) as usize);
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x - n / 2, y - n / 2);
                let inside = dx * dx + dy * dy <= 14 * 14;
                px.extend_from_slice(&if inside { [r, g, b, 255] } else { [0, 0, 0, 0] });
            }
        }
        Icon::from_rgba(px, n as u32, n as u32).expect("icona")
    }

    fn worker(rx: Receiver<Event>, notify: Arc<dyn Fn(Ui, String) + Send + Sync>) {
        let mut cfg = config::load();
        let mut rec: Option<(audio::Recorder, String, String)> = None; // recorder, exe, titolo
        while let Ok(ev) = rx.recv() {
            match ev {
                Event::Reload => {
                    cfg = config::load();
                    apply_hotkey(&cfg.hotkey);
                    notify(Ui::Idle, String::new());
                }
                Event::HotkeyDown => {
                    if rec.is_some() {
                        if cfg.is_toggle() {
                            finish(&mut rec, &cfg, &notify);
                        }
                    } else {
                        let (exe, title) = platform::foreground_app();
                        match audio::start(cfg.max_seconds) {
                            Ok(r) => {
                                rec = Some((r, exe, title));
                                notify(Ui::Recording, "Trascrittore - registrazione...".into());
                            }
                            Err(e) => {
                                config::log(&format!("avvio registrazione: {e}"));
                                notify(Ui::Error, format!("Trascrittore - errore microfono: {e}"));
                            }
                        }
                    }
                }
                Event::HotkeyUp => {
                    if !cfg.is_toggle() && rec.is_some() {
                        finish(&mut rec, &cfg, &notify);
                    }
                }
            }
        }
    }

    fn finish(
        rec: &mut Option<(audio::Recorder, String, String)>,
        cfg: &config::Config,
        notify: &Arc<dyn Fn(Ui, String) + Send + Sync>,
    ) {
        let Some((r, exe, title)) = rec.take() else { return };
        let Some(wav) = r.finish() else {
            notify(Ui::Idle, String::new());
            return;
        };
        notify(Ui::Working, "Trascrittore - elaborazione...".into());
        let (cfg, notify) = (cfg.clone(), notify.clone());
        std::thread::spawn(move || match process(wav, &cfg, &exe, &title) {
            Ok(()) => notify(Ui::Idle, String::new()),
            Err(e) => {
                config::log(&e);
                notify(Ui::Error, format!("Trascrittore - {e}"));
            }
        });
    }

    fn process(wav: Vec<u8>, cfg: &config::Config, exe: &str, title: &str) -> Result<(), String> {
        let key = config::groq_key().ok_or("chiave GROQ_API_KEY mancante (file .env)")?;
        let groq = groq::Groq::new(key)?;
        let (text, _) = usage::with_fallback(&cfg.stt_models(), |model| groq.transcribe(wav.clone(), model, &cfg.language))?;
        if text.is_empty() {
            return Ok(());
        }
        let rule = cfg.rule_for(exe, title);
        let raw = rule.map_or(false, |r| r.raw);
        let final_text = if raw {
            text
        } else {
            let prompt = cfg.system_prompt(rule);
            match usage::with_fallback(&cfg.llm_models(), |model| groq.refine(model, &prompt, &text)) {
                Ok((refined, _)) => refined,
                Err(e) => {
                    config::log(&format!("AI non disponibile, incollo la trascrizione grezza: {e}"));
                    text
                }
            }
        };
        let paste = rule.map_or("ctrl+v", |r| if r.paste.is_empty() { "ctrl+v" } else { r.paste.as_str() });
        let send_enter = rule.map_or(false, |r| r.send_enter);
        platform::paste_text(&final_text, paste, cfg.restore_clipboard, send_enter)
    }
}
