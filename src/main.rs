//! Trascrittore audio: dettatura vocale per Windows e Linux con icona nella tray.
//! Scorciatoia -> microfono -> Groq Whisper -> Groq LLM (prompt per app) -> incolla.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(desktop))]
fn main() {
    eprintln!("Trascrittore only works on Windows and Linux.");
}

#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod i18n;
#[cfg(desktop)]
mod backup;
#[cfg(desktop)]
mod filepick;
mod aec;
mod audio;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod macropad;
#[cfg(windows)]
mod pad_windows;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod combo;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod config;
#[cfg(desktop)]
mod oai;
#[cfg(desktop)]
mod anthropic;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod providers;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod legal;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod winfit;
#[cfg(desktop)]
mod gui;
#[cfg(windows)]
#[path = "hotkey_windows.rs"]
mod hotkey;
#[cfg(target_os = "linux")]
#[path = "hotkey_linux.rs"]
mod hotkey;
#[cfg(any(desktop, test))]
mod levels;
#[cfg(desktop)]
mod localstt;
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod overlay_render;
#[cfg(windows)]
#[path = "overlay_windows.rs"]
mod overlay;
#[cfg(target_os = "linux")]
#[path = "overlay_linux.rs"]
mod overlay;
#[cfg(desktop)]
mod sounds;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod trayicon;
#[cfg(windows)]
#[path = "platform_windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "platform_linux.rs"]
mod platform;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod release;
#[cfg(desktop)]
mod updater;
#[cfg(any(desktop, test))]
#[cfg_attr(not(desktop), allow(dead_code))]
mod usage;

#[cfg(windows)]
#[path = "shell_windows.rs"]
mod shell;
#[cfg(target_os = "linux")]
#[path = "shell_linux.rs"]
mod shell;

#[cfg(desktop)]
pub enum Event {
    HotkeyDown,
    HotkeyUp,
    /// Pulsante della tastiera Stack Overflow premuto (true) o rilasciato (false)
    Pad(macropad::Slot, bool),
    Reload,
}

#[cfg(desktop)]
#[derive(Clone, Copy, PartialEq)]
pub enum Ui {
    Idle,
    Recording,
    Working,
    Error,
}

/// Rilegge i dispositivi: la tastiera Stack Overflow e' stata attivata o disattivata dalle Impostazioni.
#[cfg(desktop)]
pub fn refresh_pad() {
    #[cfg(windows)]
    pad_windows::refresh_pad();
    #[cfg(target_os = "linux")]
    hotkey::refresh_pad();
}

#[cfg(desktop)]
fn main() {
    // the installer tells the program the language chosen in its wizard, then the program exits
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--set-language") {
        if let Some(code) = args.get(i + 1) {
            config::adopt_language(code);
        }
        return;
    }
    app::run();
}

#[cfg(desktop)]
mod app {
    use super::*;
    use crate::providers::Provider;
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::{Arc, Mutex};
    use tray_icon::Icon;

    pub fn run() {
        let after_update = std::env::args().any(|a| a == "--after-update");
        if !platform::single_instance(after_update) {
            return;
        }
        platform::cleanup_after_update();
        let cfg = config::load();
        let ui_lang = i18n::init(&cfg.ui_language, Some(&config::dir().join("lang")));
        config::log(&format!(
            "start: version {} ({}), language {ui_lang}, folder {}, hotkey {}, mode {}, debug {}",
            release::current_version().map_or("local".to_string(), |v| v.to_string()),
            release::current_commit().map_or("local build", release::short),
            config::dir().display(),
            cfg.hotkey,
            cfg.mode,
            if cfg.debug { "on" } else { "off" }
        ));
        apply_hotkey(&cfg.hotkey);

        let (ev_tx, ev_rx) = channel::<Event>();
        let (ui_tx, ui_rx) = channel::<(Ui, String)>();
        let wake = shell::waker();
        i18n::set_on_change(wake.clone());
        let notify: Arc<dyn Fn(Ui, String) + Send + Sync> = {
            let ui_tx = Mutex::new(ui_tx);
            Arc::new(move |s, t| {
                let _ = ui_tx.lock().unwrap().send((s, t));
                wake();
            })
        };
        hotkey::install(ev_tx.clone());
        macropad::set_sink({
            let tx = Mutex::new(ev_tx.clone());
            Box::new(move |slot, down| {
                let _ = tx.lock().unwrap().send(Event::Pad(slot, down));
            })
        });
        #[cfg(windows)]
        pad_windows::start();
        overlay::start();
        updater::start_background();
        std::thread::spawn({
            let notify = notify.clone();
            move || worker(ev_rx, notify)
        });
        shell::run(&cfg, ev_tx, ui_rx);
    }

    pub fn idle_tip(hotkey: &str) -> String {
        crate::i18n::tf("tray.tip_ready", &[("hotkey", hotkey)])
    }

    fn apply_hotkey(s: &str) {
        match combo::parse(s) {
            Ok(c) => hotkey::set_combo(c),
            Err(e) => config::log(&format!("hotkey '{s}' not valid ({e}), using ctrl+win")),
        }
    }

    pub fn icon(state: Ui) -> Icon {
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
        let mut rec: Option<Rec> = None;
        let mut pad_slot: Option<macropad::Slot> = None; // pulsante che ha avviato la registrazione
        let mut pad_action: Option<macropad::Action> = None;
        let mut pad_down_at: Option<std::time::Instant> = None;
        let mut pad_sticky = false; // tocco breve: la registrazione continua fino al tocco successivo
        while let Ok(ev) = rx.recv() {
            match ev {
                Event::Reload => {
                    cfg = config::load();
                    config::debug(|| "configuration reloaded".into());
                    apply_hotkey(&cfg.hotkey);
                    notify(Ui::Idle, String::new());
                }
                Event::Pad(slot, down) => {
                    // i pulsanti si cambiano dalle Impostazioni senza ricaricare: si rilegge il file
                    let fresh = config::load();
                    if !fresh.macropad {
                        continue;
                    }
                    let action = macropad::action_for(slot, &fresh.macropad_logo, &fresh.macropad_c, &fresh.macropad_v);
                    config::debug(|| format!("pad button {slot:?} {} (action {}, recording: {})", if down { "down" } else { "up" }, action.id(), rec.is_some()));
                    if down {
                        if rec.is_some() {
                            // un secondo tocco ferma la registrazione (modalita' "tocco" o "alterna")
                            if pad_sticky || cfg.is_toggle() {
                                finish(&mut rec, &cfg, &notify, pad_action.take());
                                pad_slot = None;
                                pad_sticky = false;
                            }
                        } else if action != macropad::Action::Off && start_recording(&mut rec, &notify, &cfg) {
                            pad_slot = Some(slot);
                            pad_action = Some(action);
                            pad_down_at = Some(std::time::Instant::now());
                            pad_sticky = false;
                        }
                    } else if !cfg.is_toggle() && rec.is_some() && pad_slot == Some(slot) && !pad_sticky {
                        // i tasti della tastiera inviano un colpo istantaneo: un tocco breve avvia e il successivo ferma,
                        // tenendo premuto (logo) si registra finche' si tiene premuto
                        if pad_down_at.map_or(false, |t| macropad::is_tap(t.elapsed())) {
                            pad_sticky = true;
                        } else {
                            finish(&mut rec, &cfg, &notify, pad_action.take());
                            pad_slot = None;
                        }
                    }
                }
                Event::HotkeyDown => {
                    config::debug(|| format!("hotkey pressed (recording in progress: {})", rec.is_some()));
                    if rec.is_some() {
                        if cfg.is_toggle() {
                            finish(&mut rec, &cfg, &notify, pad_action.take());
                            pad_slot = None;
                        }
                    } else if start_recording(&mut rec, &notify, &cfg) {
                        pad_slot = None;
                        pad_action = None;
                        pad_sticky = false;
                    }
                }
                Event::HotkeyUp => {
                    config::debug(|| format!("hotkey released (recording in progress: {})", rec.is_some()));
                    if !cfg.is_toggle() && rec.is_some() && pad_slot.is_none() {
                        finish(&mut rec, &cfg, &notify, None);
                    }
                }
            }
        }
    }

    /// Avvia la registrazione (scorciatoia o pulsante della tastiera). Restituisce true se e' partita.
    fn start_recording(rec: &mut Option<Rec>, notify: &Arc<dyn Fn(Ui, String) + Send + Sync>, cfg: &config::Config) -> bool {
        if !providers::stt_available(&providers::parse_order(&cfg.providers), config::provider_ready) {
            // nessuna chiave e nessun modello locale: parlare sarebbe inutile, si avvisa e si apre la sezione AI
            config::log("no AI service ready (no API key and no local model): not recording");
            no_ai_notice();
            return false;
        }
        let (exe, title) = platform::foreground_app();
        config::debug(|| format!("foreground app: '{exe}', title '{title}'"));
        // il suono d'avvio non deve finire nella registrazione: si scartano i suoi millisecondi
        let played = sounds::play(sounds::Kind::Start);
        let skip_ms = if played > 0 { played + 120 } else { 0 };
        match audio::start(cfg.max_seconds, skip_ms, cfg.echo_cancel) {
            Ok(r) => {
                *rec = Some((r, exe, title));
                updater::BUSY.store(true, std::sync::atomic::Ordering::Relaxed);
                overlay::show_recording(platform::foreground_icon());
                notify(Ui::Recording, crate::i18n::t("tray.tip_recording"));
                true
            }
            Err(e) => {
                config::log(&format!("cannot start recording: {e}"));
                notify(Ui::Error, crate::i18n::tf("tray.tip_mic_error", &[("err", &e)]));
                false
            }
        }
    }

    /// Finestra "nessun servizio AI pronto" con il pulsante che apre Impostazioni > AI. Al massimo una ogni 30 secondi.
    fn no_ai_notice() {
        static LAST: Mutex<Option<u64>> = Mutex::new(None);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        {
            let mut last = LAST.lock().unwrap();
            if !providers::notice_due(*last, now, 30) {
                return;
            }
            *last = Some(now);
        }
        std::thread::spawn(|| {
            let (title, mut text) = (crate::i18n::t("popup.no_ai_title"), crate::i18n::t("popup.no_ai_text"));
            if cfg!(windows) {
                // i pulsanti di Windows sono sempre OK/Annulla: il testo dice cosa fa OK
                text = format!("{text}\n\n{}", crate::i18n::t("popup.ok_hint"));
            }
            let (open, close) = (crate::i18n::t("popup.open_settings"), crate::i18n::t("popup.close"));
            if platform::ask(&title, &text, &open, &close) {
                gui::show_page(gui::PAGE_AI);
            }
        });
    }

    type Rec = (audio::Recorder, String, String); // recorder, exe, titolo

    fn finish(rec: &mut Option<Rec>, cfg: &config::Config, notify: &Arc<dyn Fn(Ui, String) + Send + Sync>, pad: Option<macropad::Action>) {
        let Some((r, exe, title)) = rec.take() else { return };
        let wav = r.finish(); // il microfono e' gia' chiuso: il suono di fine non viene registrato
        sounds::play(sounds::Kind::End);
        let Some(wav) = wav else {
            updater::BUSY.store(false, std::sync::atomic::Ordering::Relaxed);
            overlay::hide();
            notify(Ui::Idle, String::new());
            return;
        };
        overlay::show_processing();
        notify(Ui::Working, crate::i18n::t("tray.tip_working"));
        let (cfg, notify) = (cfg.clone(), notify.clone());
        std::thread::spawn(move || {
            let result = process(wav, &cfg, &exe, &title, pad);
            updater::BUSY.store(false, std::sync::atomic::Ordering::Relaxed);
            overlay::hide();
            match result {
                Ok(()) => notify(Ui::Idle, String::new()),
                Err(e) => {
                    config::log(&e);
                    notify(Ui::Error, crate::i18n::tf("tray.tip_error", &[("err", &e)]));
                }
            }
        });
    }

    fn client_err(p: Provider, msg: String) -> usage::ApiError {
        usage::ApiError { provider: p.name(), status: 0, retry_after: None, msg }
    }

    fn process(wav: Vec<u8>, cfg: &config::Config, exe: &str, title: &str, pad: Option<macropad::Action>) -> Result<(), String> {
        let order = providers::parse_order(&cfg.providers);
        let stt_order = providers::stt_order(&order);
        config::debug(|| format!("sending audio: {} KB, language '{}', services in order {:?}", wav.len() / 1024, cfg.language, stt_order.iter().map(|p| p.name()).collect::<Vec<_>>()));
        let ((text, stt_model), stt_provider) = providers::run(&stt_order, config::provider_ready, |p| {
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
                Provider::Anthropic => Err(client_err(p, i18n::t("err.no_audio"))),
                Provider::Local => {
                    let size = config::local_size();
                    let t = std::time::Instant::now();
                    let samples = localstt::wav_to_samples(&wav).map_err(|e| client_err(p, e))?;
                    let text = localstt::transcribe(&localstt::model_path(config::dir(), size), &samples, &cfg.language).map_err(|e| client_err(p, e))?;
                    config::debug(|| format!("local transcription ({}) in {:.1} s", size.id(), t.elapsed().as_secs_f32()));
                    providers::record_use(p, 0);
                    Ok((text, format!("whisper {}", size.id())))
                }
            }
        })?;
        let stt_model = format!("{} {}", stt_provider.name(), stt_model);
        config::debug(|| format!("transcription received with {stt_model}: {} characters", text.chars().count()));
        config::debug_text("transcription", &text);
        if text.is_empty() {
            config::debug(|| "empty transcription: nothing to paste".into());
            return Ok(());
        }
        let rule = cfg.rule_for(exe, title);
        config::debug(|| match rule {
            Some(r) => format!("app rule: {} (raw: {}, enter: {})", r.exe.join(", "), r.raw, r.send_enter),
            None => "no rule for this app: using the base prompt".into(),
        });
        // un pulsante della tastiera Stack Overflow puo' forzare l'uso (o il non uso) dell'AI
        let raw = pad.and_then(|a| a.ai()).map_or_else(|| rule.map_or(false, |r| r.raw), |ai| !ai);
        let final_text = if raw {
            text
        } else {
            let prompt = cfg.system_prompt(rule);
            config::debug_text_long("system prompt sent to the AI", &prompt);
            config::debug_text("user message sent to the AI", &format!("<transcription>{text}</transcription>"));
            let llm_order = providers::llm_order(&order);
            config::debug(|| format!("AI services in order: {:?}, temperature 0.2", llm_order.iter().map(|p| p.name()).collect::<Vec<_>>()));
            let refined = providers::run(&llm_order, config::provider_ready, |p| {
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
                    Provider::Local => Err(client_err(p, i18n::t("err.no_correction"))),
                }
            });
            match refined {
                Ok(((refined, llm_model), provider)) => {
                    let llm_model = format!("{} {}", provider.name(), llm_model);
                    config::debug(|| format!("text refined with {llm_model}: {} characters", refined.chars().count()));
                    config::debug_text("AI reply (refined text)", &refined);
                    refined
                }
                Err(e) => {
                    config::log(&format!("AI not available, pasting the raw transcription: {e}"));
                    text
                }
            }
        };
        let paste = rule.map_or("ctrl+v", |r| if r.paste.is_empty() { "ctrl+v" } else { r.paste.as_str() });
        let send_enter = pad.and_then(|a| a.enter()).unwrap_or_else(|| rule.map_or(false, |r| r.send_enter));
        config::debug(|| format!("pasting {} characters ({paste}, enter: {send_enter})", final_text.chars().count()));
        platform::paste_text(&final_text, paste, cfg.restore_clipboard, send_enter)
    }
}
