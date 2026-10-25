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
mod oai;
#[cfg(windows)]
mod anthropic;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod providers;
#[cfg(windows)]
mod gui;
#[cfg(windows)]
mod hotkey;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod levels;
#[cfg(windows)]
mod overlay;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod trayicon;
#[cfg(windows)]
mod platform;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod release;
#[cfg(windows)]
mod updater;
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
    use crate::providers::Provider;
    use std::ptr::null_mut;
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::{Arc, Mutex};
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub fn run() {
        let after_update = std::env::args().any(|a| a == "--after-update");
        if !platform::single_instance(after_update) {
            return;
        }
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::fs::remove_file(exe.with_extension("exe.old")); // resto di un aggiornamento
        }
        // installer scaricato da un aggiornamento precedente (se e' ancora in uso resta fino al prossimo avvio)
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("Trascrittore-Update"));
        let cfg = config::load();
        config::log(&format!(
            "avvio: versione {} ({}), cartella {}, hotkey {}, modalita' {}, debug {}",
            release::current_version().map_or("locale".to_string(), |v| v.to_string()),
            release::current_commit().map_or("build locale", release::short),
            config::dir().display(),
            cfg.hotkey,
            cfg.mode,
            if cfg.debug { "attivo" } else { "spento" }
        ));
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
        overlay::start();
        updater::start_background();
        std::thread::spawn({
            let notify = notify.clone();
            move || worker(ev_rx, notify)
        });

        let m_auto = CheckMenuItem::new("Avvia con Windows", true, platform::autostart_enabled(), None);
        let m_set = MenuItem::new("Impostazioni...", true, None);
        let m_cfg = MenuItem::new("Apri cartella configurazione", true, None);
        let m_dbg = CheckMenuItem::new("Modalita' debug (log dettagliato)", true, cfg.debug, None);
        let m_log = MenuItem::new("Apri log", true, None);
        let m_reload = MenuItem::new("Ricarica configurazione", true, None);
        let m_quit = MenuItem::new("Esci", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&m_set, &m_auto, &m_dbg, &m_log, &m_cfg, &m_reload, &PredefinedMenuItem::separator(), &m_quit]);
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
                } else if e.id == *m_dbg.id() {
                    config::set_debug(m_dbg.is_checked());
                } else if e.id == *m_log.id() {
                    config::open_log();
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
        let tone = match state {
            Ui::Idle => trayicon::Tone::Idle,
            Ui::Recording => trayicon::Tone::Recording,
            Ui::Working => trayicon::Tone::Working,
            Ui::Error => trayicon::Tone::Error,
        };
        Icon::from_rgba(trayicon::pixels(tone, 32), 32, 32).expect("icona")
    }

    fn worker(rx: Receiver<Event>, notify: Arc<dyn Fn(Ui, String) + Send + Sync>) {
        let mut cfg = config::load();
        let mut rec: Option<(audio::Recorder, String, String)> = None; // recorder, exe, titolo
        while let Ok(ev) = rx.recv() {
            match ev {
                Event::Reload => {
                    cfg = config::load();
                    config::debug(|| "configurazione ricaricata".into());
                    apply_hotkey(&cfg.hotkey);
                    notify(Ui::Idle, String::new());
                }
                Event::HotkeyDown => {
                    config::debug(|| format!("hotkey premuta (registrazione in corso: {})", rec.is_some()));
                    if rec.is_some() {
                        if cfg.is_toggle() {
                            finish(&mut rec, &cfg, &notify);
                        }
                    } else {
                        let (exe, title) = platform::foreground_app();
                        config::debug(|| format!("app in primo piano: '{exe}', titolo '{title}'"));
                        match audio::start(cfg.max_seconds) {
                            Ok(r) => {
                                rec = Some((r, exe, title));
                                updater::BUSY.store(true, std::sync::atomic::Ordering::Relaxed);
                                overlay::show_recording(platform::foreground_icon());
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
                    config::debug(|| format!("hotkey rilasciata (registrazione in corso: {})", rec.is_some()));
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
            updater::BUSY.store(false, std::sync::atomic::Ordering::Relaxed);
            overlay::hide();
            notify(Ui::Idle, String::new());
            return;
        };
        overlay::show_processing();
        notify(Ui::Working, "Trascrittore - elaborazione...".into());
        let (cfg, notify) = (cfg.clone(), notify.clone());
        std::thread::spawn(move || {
            let result = process(wav, &cfg, &exe, &title);
            updater::BUSY.store(false, std::sync::atomic::Ordering::Relaxed);
            overlay::hide();
            match result {
                Ok(()) => notify(Ui::Idle, String::new()),
                Err(e) => {
                    config::log(&e);
                    notify(Ui::Error, format!("Trascrittore - {e}"));
                }
            }
        });
    }

    fn client_err(p: Provider, msg: String) -> usage::ApiError {
        usage::ApiError { provider: p.name(), status: 0, retry_after: None, msg }
    }

    fn process(wav: Vec<u8>, cfg: &config::Config, exe: &str, title: &str) -> Result<(), String> {
        let order = providers::parse_order(&cfg.providers);
        let stt_order: Vec<Provider> = order.iter().copied().filter(|p| p.supports_stt()).collect();
        config::debug(|| format!("invio audio: {} KB, lingua '{}', servizi in ordine {:?}", wav.len() / 1024, cfg.language, stt_order.iter().map(|p| p.name()).collect::<Vec<_>>()));
        let ((text, stt_model), stt_provider) = providers::run(&stt_order, |p| config::provider_key(p).is_some(), |p| {
            let key = config::provider_key(p).unwrap_or_default();
            match p {
                Provider::Groq => {
                    let c = oai::Oai::groq(key).map_err(|e| client_err(p, e))?;
                    usage::with_fallback(&cfg.stt_models(), |model| c.transcribe(wav.clone(), model, &cfg.language)).map(|(t, m)| (t, m))
                }
                Provider::OpenAi => {
                    let c = oai::Oai::openai(key).map_err(|e| client_err(p, e))?;
                    c.transcribe(wav.clone(), &cfg.openai_stt_model, &cfg.language).map(|t| (t, cfg.openai_stt_model.clone()))
                }
                Provider::Anthropic => Err(client_err(p, "non trascrive l'audio".into())),
            }
        })?;
        let stt_model = format!("{} {}", stt_provider.name(), stt_model);
        config::debug(|| format!("trascrizione ricevuta con {stt_model}: {} caratteri", text.chars().count()));
        config::debug_text("trascrizione", &text);
        if text.is_empty() {
            config::debug(|| "trascrizione vuota: non incollo nulla".into());
            return Ok(());
        }
        let rule = cfg.rule_for(exe, title);
        config::debug(|| match rule {
            Some(r) => format!("regola app: {} (raw: {}, invio: {})", r.exe.join(", "), r.raw, r.send_enter),
            None => "nessuna regola per questa app: uso il prompt di base".into(),
        });
        let raw = rule.map_or(false, |r| r.raw);
        let final_text = if raw {
            text
        } else {
            let prompt = cfg.system_prompt(rule);
            config::debug_text_long("prompt di sistema inviato all'AI", &prompt);
            config::debug_text("messaggio utente inviato all'AI", &format!("<trascrizione>{text}</trascrizione>"));
            config::debug(|| format!("servizi AI in ordine: {:?}, temperatura 0.2", order.iter().map(|p| p.name()).collect::<Vec<_>>()));
            let refined = providers::run(&order, |p| config::provider_key(p).is_some(), |p| {
                let key = config::provider_key(p).unwrap_or_default();
                match p {
                    Provider::Groq => {
                        let c = oai::Oai::groq(key).map_err(|e| client_err(p, e))?;
                        usage::with_fallback(&cfg.llm_models(), |model| c.refine(model, &prompt, &text))
                    }
                    Provider::OpenAi => {
                        let c = oai::Oai::openai(key).map_err(|e| client_err(p, e))?;
                        c.refine(&cfg.openai_llm_model, &prompt, &text).map(|t| (t, cfg.openai_llm_model.clone()))
                    }
                    Provider::Anthropic => {
                        let c = anthropic::Anthropic::new(key).map_err(|e| client_err(p, e))?;
                        c.refine(&cfg.anthropic_llm_model, &prompt, &text).map(|t| (t, cfg.anthropic_llm_model.clone()))
                    }
                }
            });
            match refined {
                Ok(((refined, llm_model), provider)) => {
                    let llm_model = format!("{} {}", provider.name(), llm_model);
                    config::debug(|| format!("testo rifinito con {llm_model}: {} caratteri", refined.chars().count()));
                    config::debug_text("risposta dell'AI (testo rifinito)", &refined);
                    refined
                }
                Err(e) => {
                    config::log(&format!("AI non disponibile, incollo la trascrizione grezza: {e}"));
                    text
                }
            }
        };
        let paste = rule.map_or("ctrl+v", |r| if r.paste.is_empty() { "ctrl+v" } else { r.paste.as_str() });
        let send_enter = rule.map_or(false, |r| r.send_enter);
        config::debug(|| format!("incollo {} caratteri ({paste}, invio: {send_enter})", final_text.chars().count()));
        platform::paste_text(&final_text, paste, cfg.restore_clipboard, send_enter)
    }
}
