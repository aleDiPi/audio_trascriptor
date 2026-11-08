//! Tastiera a 3 tasti "The Key" di Stack Overflow (logo, C, V): riconoscimento, tasti e azioni associabili.
//! Questo file e' puro (nessuna API di sistema): il rilevamento del dispositivo sta in hotkey_linux.rs e pad_windows.rs.
//! Di fabbrica i tre tasti inviano Ctrl, C e V: il programma li distingue dalla tastiera normale in base al dispositivo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Tasto fisico inviato dalla tastiera (di fabbrica: Ctrl, C, V).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Ctrl,
    C,
    V,
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
        }
    }
}

/// Codice virtuale di Windows -> tasto della tastiera. Ctrl arriva come 0x11 (raw input) o 0xA2/0xA3 (hook).
pub fn key_for_vk(vk: u32) -> Option<Key> {
    match vk {
        0x11 | 0xA2 | 0xA3 => Some(Key::Ctrl),
        0x43 => Some(Key::C),
        0x56 => Some(Key::V),
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

/// Toglie le ripetizioni automatiche: ogni pulsante produce un solo "giu'" e un solo "su'".
#[derive(Default)]
pub struct Edges {
    down: [bool; 3],
}

impl Edges {
    fn pos(slot: Slot) -> usize {
        SLOTS.iter().position(|s| *s == slot).unwrap_or(0)
    }

    /// `Some(true)` = pressione, `Some(false)` = rilascio, `None` = ripetizione da ignorare.
    pub fn feed(&mut self, slot: Slot, down: bool) -> Option<bool> {
        let d = &mut self.down[Self::pos(slot)];
        if *d == down {
            return None;
        }
        *d = down;
        Some(down)
    }

    pub fn clear(&mut self) {
        self.down = [false; 3];
    }
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
    fn key_repeat_is_ignored() {
        let mut e = Edges::default();
        assert_eq!(e.feed(Slot::C, true), Some(true));
        assert_eq!(e.feed(Slot::C, true), None);
        assert_eq!(e.feed(Slot::V, true), Some(true));
        assert_eq!(e.feed(Slot::C, false), Some(false));
        assert_eq!(e.feed(Slot::C, false), None);
    }
}
