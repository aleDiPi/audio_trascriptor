//! Tastiera a 3 tasti "The Key" di Stack Overflow (logo, C, V): riconoscimento, tasti e azioni associabili.
//! Questo file e' puro (nessuna API di sistema): il rilevamento del dispositivo sta in hotkey_linux.rs e pad_windows.rs.
//! Di fabbrica i tre tasti inviano Ctrl, C e V: il programma li distingue dalla tastiera normale in base al dispositivo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Tasto fisico inviato dalla tastiera (di fabbrica: Ctrl, C, V).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Ctrl,
    C,
    V,
    /// Tasto "diretto" (F13, F14, F15 dopo la riprogrammazione): vale direttamente un pulsante
    Pad(Slot),
}

/// Pulsante della tastiera, nell'ordine in cui compare nelle Impostazioni.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    Logo,
    C,
    V,
}

pub const SLOTS: [Slot; 3] = [Slot::Logo, Slot::C, Slot::V];

impl Key {
    pub fn slot(self) -> Slot {
        match self {
            Key::Ctrl => Slot::Logo,
            Key::C => Slot::C,
            Key::V => Slot::V,
            Key::Pad(slot) => slot,
        }
    }
}

/// Codice virtuale di Windows -> tasto della tastiera. Ctrl arriva come 0x11 (raw input) o 0xA2/0xA3 (hook).
pub fn key_for_vk(vk: u32) -> Option<Key> {
    match vk {
        0x11 | 0xA2 | 0xA3 => Some(Key::Ctrl),
        0x43 => Some(Key::C),
        0x56 => Some(Key::V),
        // F13, F14, F15: Windows non li usa, la tastiera riprogrammata li invia al posto di Ctrl, C, V
        0x7C => Some(Key::Pad(Slot::Logo)),
        0x7D => Some(Key::Pad(Slot::C)),
        0x7E => Some(Key::Pad(Slot::V)),
        _ => None,
    }
}

/// Cosa fa un pulsante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Modalita' normale: trascrizione + correzione AI (come prevede l'app in primo piano) + Invio.
    Full,
    /// Solo trascrizione + Invio, senza AI.
    RawSend,
    /// Solo trascrizione, senza AI e senza Invio.
    Raw,
    /// Trascrizione + correzione AI, senza Invio.
    Ai,
    /// Il pulsante non fa nulla.
    Off,
}

/// Ordine delle voci nel menu delle Impostazioni.
pub const ACTIONS: [Action; 5] = [Action::Full, Action::RawSend, Action::Raw, Action::Ai, Action::Off];

impl Action {
    pub fn id(self) -> &'static str {
        match self {
            Action::Full => "full",
            Action::RawSend => "raw_send",
            Action::Raw => "raw",
            Action::Ai => "ai",
            Action::Off => "off",
        }
    }

    /// Testo di config.toml -> azione; se non e' riconosciuto vale `default`.
    pub fn parse(s: &str, default: Action) -> Action {
        let s = s.trim().to_ascii_lowercase();
        ACTIONS.iter().copied().find(|a| a.id() == s).unwrap_or(default)
    }

    pub fn index(self) -> i32 {
        ACTIONS.iter().position(|a| *a == self).unwrap_or(0) as i32
    }

    pub fn from_index(i: i32) -> Action {
        ACTIONS.get(i.max(0) as usize).copied().unwrap_or(Action::Off)
    }

    /// `Some(true/false)` forza l'uso dell'AI, `None` segue la regola dell'app in primo piano.
    pub fn ai(self) -> Option<bool> {
        match self {
            Action::Full | Action::Off => None,
            Action::Ai => Some(true),
            Action::RawSend | Action::Raw => Some(false),
        }
    }

    /// Invio dopo l'incolla; `None` = come prevede la regola dell'app.
    pub fn enter(self) -> Option<bool> {
        match self {
            Action::Full | Action::RawSend => Some(true),
            Action::Raw | Action::Ai => Some(false),
            Action::Off => None,
        }
    }
}

/// Azioni di fabbrica: logo = normale, C = senza AI + Invio, V = solo trascrizione.
pub fn default_action(slot: Slot) -> Action {
    match slot {
        Slot::Logo => Action::Full,
        Slot::C => Action::RawSend,
        Slot::V => Action::Raw,
    }
}

pub fn action_for(slot: Slot, logo: &str, c: &str, v: &str) -> Action {
    let text = match slot {
        Slot::Logo => logo,
        Slot::C => c,
        Slot::V => v,
    };
    Action::parse(text, default_action(slot))
}

/// "16c0:27db" -> (0x16c0, 0x27db)
pub fn parse_id(s: &str) -> Option<(u16, u16)> {
    let (v, p) = s.trim().split_once(':')?;
    Some((u16::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok()?, u16::from_str_radix(p.trim().trim_start_matches("0x"), 16).ok()?))
}

/// Percorso HID di Windows (`\\?\HID#VID_16C0&PID_27DB&MI_00#...`) -> (vid, pid).
pub fn parse_hid_path(path: &str) -> Option<(u16, u16)> {
    let up = path.to_ascii_uppercase();
    let grab = |tag: &str| {
        let i = up.find(tag)? + tag.len();
        u16::from_str_radix(up.get(i..i + 4)?, 16).ok()
    };
    Some((grab("VID_")?, grab("PID_")?))
}

/// Il nome somiglia a quello della tastiera di Stack Overflow? ("The Key" solo come parola intera: non "the keyboard")
pub fn name_matches(name: &str) -> bool {
    let n = name.to_lowercase();
    if n.contains("stack overflow") || n.contains("stackoverflow") {
        return true;
    }
    n.match_indices("the key").any(|(i, m)| n[i + m.len()..].chars().next().map_or(true, |c| !c.is_alphanumeric()))
}

/// Il dispositivo e' la tastiera di Stack Overflow? Per nome, oppure per VID:PID elencati in `macropad_ids`.
pub fn is_pad(name: &str, ids: Option<(u16, u16)>, extra: &[String]) -> bool {
    if name_matches(name) {
        return true;
    }
    ids.map_or(false, |id| extra.iter().filter_map(|s| parse_id(s)).any(|e| e == id))
}

/// Il logo invia Ctrl e C/V inviano Ctrl+C / Ctrl+V in un attimo: dopo un Ctrl si aspetta un momento per capire
/// se arriva una lettera (allora il pulsante e' C o V) oppure no (allora e' il logo, anche tenuto premuto).
pub const CHORD_WINDOW: Duration = Duration::from_millis(60);

/// Tasti appena arrivati dalla tastiera Stack Overflow (Windows): il filtro a basso livello li confronta con i propri
/// per sapere se un Ctrl/C/V viene da lei. Il colpo e' di piu' tasti in un millisecondo, quindi serve una coda.
#[derive(Default)]
pub struct Recent {
    items: std::collections::VecDeque<(Key, bool, Instant)>,
}

impl Recent {
    pub const WINDOW: Duration = Duration::from_millis(200);

    pub fn push(&mut self, key: Key, down: bool, now: Instant) {
        self.items.push_back((key, down, now));
        while self.items.len() > 32 {
            self.items.pop_front();
        }
    }

    /// C'e' un tasto uguale arrivato da poco? Se si' lo toglie dalla coda (ogni tasto si usa una volta sola).
    pub fn take(&mut self, key: Key, down: bool, now: Instant) -> bool {
        self.items.retain(|(_, _, t)| now.duration_since(*t) < Self::WINDOW);
        match self.items.iter().position(|(k, d, _)| *k == key && *d == down) {
            Some(i) => {
                self.items.remove(i);
                true
            }
            None => false,
        }
    }
}

/// Un pulsante rilasciato prima di questo tempo e' un "tocco": la registrazione continua fino al tocco successivo.
pub const TAP_MAX: Duration = Duration::from_millis(350);

/// Il pulsante e' stato solo toccato (rilasciato subito)?
pub fn is_tap(held: Duration) -> bool {
    held < TAP_MAX
}

/// Macchina a stati dei tasti della tastiera: trasforma tasti fisici in pressioni/rilasci dei tre pulsanti.
#[derive(Default)]
pub struct Chords {
    ctrl_down: bool,
    /// Ctrl premuto da poco, ancora senza lettera: istante della pressione
    pending: Option<Instant>,
    logo_active: bool,
    letters: [bool; 2],
    direct: [bool; 3],
}

impl Chords {
    fn letter(slot: Slot) -> Option<usize> {
        match slot {
            Slot::C => Some(0),
            Slot::V => Some(1),
            Slot::Logo => None,
        }
    }

    /// Un tasto fisico premuto o rilasciato; restituisce le pressioni/rilasci dei pulsanti che ne derivano.
    pub fn feed(&mut self, key: Key, down: bool, now: Instant) -> Vec<(Slot, bool)> {
        let mut out = Vec::new();
        match (key, down) {
            (Key::Ctrl, true) => {
                if !self.ctrl_down {
                    self.ctrl_down = true;
                    self.pending = Some(now);
                }
            }
            (Key::Ctrl, false) => {
                self.ctrl_down = false;
                if self.pending.take().is_some() {
                    // tocco brevissimo del logo, senza lettera
                    out.push((Slot::Logo, true));
                    out.push((Slot::Logo, false));
                } else if std::mem::replace(&mut self.logo_active, false) {
                    out.push((Slot::Logo, false));
                }
            }
            (Key::Pad(slot), down) => {
                let i = SLOTS.iter().position(|s| *s == slot).unwrap_or(0);
                if down {
                    if !std::mem::replace(&mut self.direct[i], true) {
                        out.push((slot, true));
                    }
                } else if std::mem::replace(&mut self.direct[i], false) {
                    out.push((slot, false));
                }
            }
            (k, down) => {
                let slot = k.slot();
                let Some(i) = Self::letter(slot) else { return out };
                if down {
                    self.pending = None; // il Ctrl faceva parte del colpo: non e' il logo
                    if !self.letters[i] {
                        self.letters[i] = true;
                        out.push((slot, true));
                    }
                } else if std::mem::replace(&mut self.letters[i], false) {
                    out.push((slot, false));
                }
            }
        }
        out
    }

    /// Da chiamare dopo `CHORD_WINDOW` da un Ctrl: se non e' arrivata nessuna lettera, il logo e' premuto (tenuto).
    pub fn expire(&mut self, now: Instant) -> Vec<(Slot, bool)> {
        match self.pending {
            Some(t) if now.duration_since(t) >= CHORD_WINDOW => {
                self.pending = None;
                self.logo_active = true;
                vec![(Slot::Logo, true)]
            }
            _ => Vec::new(),
        }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

type Sink = Box<dyn Fn(Slot, bool) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();
static CHORDS: Mutex<Option<Chords>> = Mutex::new(None);

/// Dove inviare le pressioni dei pulsanti (il programma principale ne fa eventi).
pub fn set_sink(f: Sink) {
    let _ = SINK.set(f);
}

fn emit(list: Vec<(Slot, bool)>) {
    if let Some(sink) = SINK.get() {
        for (slot, down) in list {
            sink(slot, down);
        }
    }
}

/// Un tasto fisico della tastiera Stack Overflow e' stato premuto o rilasciato.
pub fn feed_key(key: Key, down: bool) {
    let (out, wait) = {
        let mut g = CHORDS.lock().unwrap();
        let c = g.get_or_insert_with(Chords::default);
        let out = c.feed(key, down, Instant::now());
        (out, c.has_pending())
    };
    emit(out);
    if wait && key == Key::Ctrl && down {
        std::thread::spawn(|| {
            std::thread::sleep(CHORD_WINDOW + Duration::from_millis(10));
            let out = CHORDS.lock().unwrap().get_or_insert_with(Chords::default).expire(Instant::now());
            emit(out);
        });
    }
}

/// Dimentica lo stato dei tasti (tastiera scollegata o riesaminata).
pub fn reset_keys() {
    *CHORDS.lock().unwrap() = None;
}

static PRESENT: AtomicBool = AtomicBool::new(false);
static NAME: Mutex<String> = Mutex::new(String::new());

/// La tastiera e' collegata adesso? (aggiornato dal thread che ascolta i dispositivi)
pub fn present() -> bool {
    PRESENT.load(Ordering::Relaxed)
}

pub fn name() -> String {
    NAME.lock().unwrap().clone()
}

pub fn set_present(on: bool, name: &str) {
    PRESENT.store(on, Ordering::Relaxed);
    *NAME.lock().unwrap() = if on { name.to_string() } else { String::new() };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_keys_map_to_the_three_buttons() {
        assert_eq!(Key::Ctrl.slot(), Slot::Logo);
        assert_eq!(Key::C.slot(), Slot::C);
        assert_eq!(Key::V.slot(), Slot::V);
    }

    #[test]
    fn windows_virtual_keys_are_recognised() {
        for vk in [0x11, 0xA2, 0xA3] {
            assert_eq!(key_for_vk(vk), Some(Key::Ctrl));
        }
        assert_eq!(key_for_vk(0x43), Some(Key::C));
        assert_eq!(key_for_vk(0x56), Some(Key::V));
        assert_eq!(key_for_vk(0x41), None);
    }

    #[test]
    fn default_buttons_follow_the_requested_modes() {
        // logo = normale (AI come l'app + Invio), C = senza AI + Invio, V = solo trascrizione
        let (logo, c, v) = (default_action(Slot::Logo), default_action(Slot::C), default_action(Slot::V));
        assert_eq!((logo.ai(), logo.enter()), (None, Some(true)));
        assert_eq!((c.ai(), c.enter()), (Some(false), Some(true)));
        assert_eq!((v.ai(), v.enter()), (Some(false), Some(false)));
    }

    #[test]
    fn actions_round_trip_through_text_and_index() {
        for a in ACTIONS {
            assert_eq!(Action::parse(a.id(), Action::Off), a);
            assert_eq!(Action::from_index(a.index()), a);
        }
        assert_eq!(Action::parse(" RAW ", Action::Full), Action::Raw);
        assert_eq!(Action::parse("boh", Action::Ai), Action::Ai);
        assert_eq!(Action::from_index(-1), Action::Full);
        assert_eq!(Action::from_index(99), Action::Off);
    }

    #[test]
    fn config_text_falls_back_to_the_default_of_each_button() {
        assert_eq!(action_for(Slot::Logo, "", "", ""), Action::Full);
        assert_eq!(action_for(Slot::C, "", "x", ""), Action::RawSend);
        assert_eq!(action_for(Slot::V, "", "", "ai"), Action::Ai);
    }

    #[test]
    fn ids_parse_from_text_and_from_windows_paths() {
        assert_eq!(parse_id("16c0:27DB"), Some((0x16c0, 0x27db)));
        assert_eq!(parse_id("0x1b4f:0x9206"), Some((0x1b4f, 0x9206)));
        assert_eq!(parse_id("nonsense"), None);
        assert_eq!(parse_hid_path(r"\\?\HID#VID_16C0&PID_27DB&MI_00#7&abc#{884b96c3}"), Some((0x16c0, 0x27db)));
        assert_eq!(parse_hid_path(r"\\?\ACPI#PNP0303"), None);
    }

    #[test]
    fn the_pad_is_found_by_name_or_by_configured_id() {
        assert!(name_matches("Stack Overflow The Key"));
        assert!(name_matches("StackOverflow Macropad"));
        assert!(name_matches("Drop The Key"));
        assert!(!name_matches("Generic USB Keyboard"));
        assert!(!name_matches("Logitech the keyboard"));
        let extra = vec!["16c0:27db".to_string(), "junk".to_string()];
        assert!(is_pad("Unknown", Some((0x16c0, 0x27db)), &extra));
        assert!(!is_pad("Unknown", Some((0x046d, 0xc31c)), &extra));
        assert!(!is_pad("Unknown", None, &extra));
    }

    #[test]
    fn f13_f14_f15_are_direct_buttons_that_can_be_held() {
        assert_eq!(key_for_vk(0x7C), Some(Key::Pad(Slot::Logo)));
        assert_eq!(key_for_vk(0x7D), Some(Key::Pad(Slot::C)));
        assert_eq!(key_for_vk(0x7E), Some(Key::Pad(Slot::V)));
        let (mut c, t) = (Chords::default(), t0());
        assert_eq!(c.feed(Key::Pad(Slot::C), true, t), vec![(Slot::C, true)]);
        assert!(c.feed(Key::Pad(Slot::C), true, t).is_empty()); // ripetizione
        assert_eq!(c.feed(Key::Pad(Slot::C), false, t), vec![(Slot::C, false)]);
        assert!(c.feed(Key::Pad(Slot::C), false, t).is_empty());
    }

    #[test]
    fn a_whole_chord_can_be_matched_in_order_even_if_it_arrived_at_once() {
        let (mut r, t) = (Recent::default(), t0());
        for (k, d) in [(Key::Ctrl, true), (Key::V, true), (Key::V, false), (Key::Ctrl, false)] {
            r.push(k, d, t);
        }
        assert!(r.take(Key::Ctrl, true, t));
        assert!(r.take(Key::V, true, t));
        assert!(r.take(Key::V, false, t));
        assert!(r.take(Key::Ctrl, false, t));
        assert!(!r.take(Key::V, true, t)); // ogni tasto vale una volta
    }

    #[test]
    fn old_or_different_keys_are_not_matched() {
        let (mut r, t) = (Recent::default(), t0());
        r.push(Key::C, true, t);
        assert!(!r.take(Key::V, true, t));
        assert!(!r.take(Key::C, false, t));
        assert!(!r.take(Key::C, true, t + Recent::WINDOW * 2));
    }

    #[test]
    fn short_presses_are_taps_and_long_ones_are_holds() {
        assert!(is_tap(Duration::from_millis(5)));
        assert!(!is_tap(Duration::from_millis(800)));
    }

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn ctrl_c_chord_is_the_c_button_and_not_the_logo() {
        let (mut c, t) = (Chords::default(), t0());
        assert!(c.feed(Key::Ctrl, true, t).is_empty());
        assert_eq!(c.feed(Key::C, true, t), vec![(Slot::C, true)]);
        assert_eq!(c.feed(Key::C, false, t), vec![(Slot::C, false)]);
        assert!(c.feed(Key::Ctrl, false, t).is_empty());
        // la scadenza non fa nulla: il Ctrl era parte del colpo
        assert!(c.expire(t + CHORD_WINDOW * 2).is_empty());
    }

    #[test]
    fn ctrl_v_chord_is_the_v_button() {
        let (mut c, t) = (Chords::default(), t0());
        c.feed(Key::Ctrl, true, t);
        assert_eq!(c.feed(Key::V, true, t), vec![(Slot::V, true)]);
        assert_eq!(c.feed(Key::V, false, t), vec![(Slot::V, false)]);
        assert!(c.feed(Key::Ctrl, false, t).is_empty());
    }

    #[test]
    fn held_ctrl_alone_becomes_the_logo_after_the_window() {
        let (mut c, t) = (Chords::default(), t0());
        assert!(c.feed(Key::Ctrl, true, t).is_empty());
        assert!(c.expire(t + CHORD_WINDOW / 2).is_empty());
        assert_eq!(c.expire(t + CHORD_WINDOW), vec![(Slot::Logo, true)]);
        assert!(c.expire(t + CHORD_WINDOW * 3).is_empty());
        assert_eq!(c.feed(Key::Ctrl, false, t), vec![(Slot::Logo, false)]);
    }

    #[test]
    fn quick_tap_of_ctrl_alone_is_a_logo_press_and_release() {
        let (mut c, t) = (Chords::default(), t0());
        c.feed(Key::Ctrl, true, t);
        assert_eq!(c.feed(Key::Ctrl, false, t), vec![(Slot::Logo, true), (Slot::Logo, false)]);
    }

    #[test]
    fn repeats_and_plain_letters_are_handled() {
        let (mut c, t) = (Chords::default(), t0());
        assert_eq!(c.feed(Key::C, true, t), vec![(Slot::C, true)]);
        assert!(c.feed(Key::C, true, t).is_empty()); // ripetizione automatica
        assert_eq!(c.feed(Key::C, false, t), vec![(Slot::C, false)]);
        assert!(c.feed(Key::C, false, t).is_empty());
        c.feed(Key::Ctrl, true, t);
        assert!(c.feed(Key::Ctrl, true, t).is_empty());
    }
}
