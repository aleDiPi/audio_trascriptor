//! Scorciatoia globale su X11 tramite gli eventi "raw" di XInput2: sono ricevuti da tutte le applicazioni senza
//! catturare la tastiera, quindi funzionano anche le combinazioni di soli modificatori (predefinita Ctrl+Win)
//! e il rilascio dei tasti per la modalita' "tieni premuto". I tasti non vengono "mangiati": conviene una
//! combinazione di soli modificatori, oppure con un tasto che non scrive nulla (F9, ecc.).

use crate::combo::Combo;
use crate::platform::{self, KS_ALT, KS_CONTROL, KS_SHIFT, KS_SUPER};
use crate::{config, Event};
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{ConnectionExt as _, EventMask, XIEventMask};
use x11rb::protocol::Event as XEvent;

/// Cosa e' successo alla scorciatoia dopo un tasto.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Edge {
    Down,
    Up,
}

/// Keycode associati a ciascun modificatore/tasto della scorciatoia (piu' di uno: lato sinistro e destro).
#[derive(Default, Clone, Debug)]
pub struct Codes {
    pub ctrl: Vec<u8>,
    pub alt: Vec<u8>,
    pub shift: Vec<u8>,
    pub win: Vec<u8>,
    pub key: Vec<u8>,
}

/// Macchina a stati pura: riceve pressioni/rilasci di keycode e dice quando la scorciatoia scatta e si rilascia.
#[derive(Default)]
pub struct Tracker {
    combo: Combo,
    codes: Codes,
    pressed: HashSet<u8>,
    active: bool,
}

impl Tracker {
    pub fn new(combo: Combo, codes: Codes) -> Self {
        Tracker { combo, codes, pressed: HashSet::new(), active: false }
    }

    pub fn feed(&mut self, keycode: u8, down: bool) -> Option<Edge> {
        if down {
            self.pressed.insert(keycode);
        } else {
            self.pressed.remove(&keycode);
        }
        let any = |v: &[u8]| v.iter().any(|c| self.pressed.contains(c));
        let (c, k) = (self.combo, &self.codes);
        let mods = (!c.ctrl || any(&k.ctrl)) && (!c.alt || any(&k.alt)) && (!c.shift || any(&k.shift)) && (!c.win || any(&k.win));
        // una scorciatoia senza codici (tasto non trovato) non scatta mai
        let key_ok = c.key.is_none() || any(&k.key);
        let has_codes = (!c.ctrl || !k.ctrl.is_empty()) && (!c.win || !k.win.is_empty()) && (c.key.is_none() || !k.key.is_empty());
        let satisfied = mods && key_ok && has_codes;
        let was = self.active;
        self.active = satisfied;
        match (satisfied, was) {
            (true, false) => Some(Edge::Down),
            (false, true) => Some(Edge::Up),
            _ => None,
        }
    }
}

struct Shared {
    combo: Combo,
    dirty: bool,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { combo: Combo { ctrl: true, alt: false, shift: false, win: true, key: None }, dirty: true });
static TX: OnceLock<Sender<Event>> = OnceLock::new();

pub fn set_combo(c: Combo) {
    let mut s = SHARED.lock().unwrap();
    s.combo = c;
    s.dirty = true;
}

/// Avvia il thread che ascolta la tastiera (resta bloccato in attesa di eventi: nessuna CPU a riposo).
pub fn install(tx: Sender<Event>) {
    let _ = TX.set(tx);
    if std::env::var("XDG_SESSION_TYPE").map_or(false, |t| t == "wayland") {
        config::log("Wayland session: the global hotkey only works in an X11 session (Linux Mint Cinnamon uses it by default)");
    }
    let _ = std::thread::Builder::new().name("hotkey".into()).spawn(|| {
        if let Err(e) = listen() {
            config::log(&format!("global hotkey not active: {e}"));
        }
    });
}

fn codes_for(combo: &Combo, conn: &x11rb::rust_connection::RustConnection) -> Codes {
    let key_sym = combo.key.and_then(platform::vk_to_keysym);
    let mut wanted = vec![KS_CONTROL, 0xffe4, KS_ALT, 0xffea, KS_SHIFT, 0xffe2, KS_SUPER, 0xffec];
    wanted.extend(key_sym);
    let map = platform::resolve_keycodes(conn, &wanted);
    let get = |syms: &[u32]| syms.iter().flat_map(|s| map.get(s).cloned().unwrap_or_default()).collect::<Vec<u8>>();
    Codes {
        ctrl: get(&[KS_CONTROL, 0xffe4]),
        alt: get(&[KS_ALT, 0xffea]),
        shift: get(&[KS_SHIFT, 0xffe2]),
        win: get(&[KS_SUPER, 0xffec]),
        key: key_sym.map(|s| get(&[s])).unwrap_or_default(),
    }
}

fn listen() -> Result<(), String> {
    let (conn, root) = platform::connect()?;
    conn.xinput_xi_query_version(2, 0).map_err(|e| e.to_string())?.reply().map_err(|e| format!("XInput2 not available: {e}"))?;
    // 1 = XIAllMasterDevices
    let mask = EventMask { deviceid: 1, mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE] };
    conn.xinput_xi_select_events(root, &[mask]).map_err(|e| e.to_string())?;
    conn.flush().map_err(|e| e.to_string())?;

    let mut tracker = Tracker::default();
    loop {
        let (keycode, down) = match conn.wait_for_event().map_err(|e| format!("X11 connection lost: {e}"))? {
            XEvent::XinputRawKeyPress(e) => (e.detail, true),
            XEvent::XinputRawKeyRelease(e) => (e.detail, false),
            _ => continue,
        };
        let renew = {
            let mut s = SHARED.lock().unwrap();
            std::mem::replace(&mut s.dirty, false).then_some(s.combo)
        };
        if let Some(combo) = renew {
            tracker = Tracker::new(combo, codes_for(&combo, &conn));
        }
        let Ok(keycode) = u8::try_from(keycode) else { continue };
        let ev = match tracker.feed(keycode, down) {
            Some(Edge::Down) => Event::HotkeyDown,
            Some(Edge::Up) => Event::HotkeyUp,
            None => continue,
        };
        if let Some(tx) = TX.get() {
            let _ = tx.send(ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL_L: u8 = 37;
    const CTRL_R: u8 = 105;
    const SUPER_L: u8 = 133;
    const SHIFT: u8 = 50;
    const F9: u8 = 75;
    const LETTER: u8 = 40;

    fn codes() -> Codes {
        Codes { ctrl: vec![CTRL_L, CTRL_R], alt: vec![64], shift: vec![SHIFT], win: vec![SUPER_L, 134], key: vec![F9] }
    }

    fn ctrl_win() -> Tracker {
        Tracker::new(crate::combo::parse("ctrl+win").unwrap(), codes())
    }

    #[test]
    fn modifier_only_combo_fires_and_releases() {
        let mut t = ctrl_win();
        assert_eq!(t.feed(CTRL_L, true), None);
        assert_eq!(t.feed(SUPER_L, true), Some(Edge::Down));
        assert_eq!(t.feed(LETTER, true), None); // un altro tasto durante la pressione non cambia nulla
        assert_eq!(t.feed(LETTER, false), None);
        assert_eq!(t.feed(SUPER_L, false), Some(Edge::Up));
        assert_eq!(t.feed(CTRL_L, false), None);
    }

    #[test]
    fn right_modifier_counts_and_order_does_not_matter() {
        let mut t = ctrl_win();
        assert_eq!(t.feed(SUPER_L, true), None);
        assert_eq!(t.feed(CTRL_R, true), Some(Edge::Down));
        assert_eq!(t.feed(CTRL_R, false), Some(Edge::Up));
    }

    #[test]
    fn repeated_presses_fire_once() {
        let mut t = ctrl_win();
        t.feed(CTRL_L, true);
        assert_eq!(t.feed(SUPER_L, true), Some(Edge::Down));
        assert_eq!(t.feed(SUPER_L, true), None); // auto-ripetizione
    }

    #[test]
    fn combo_with_key_needs_the_key() {
        let mut t = Tracker::new(crate::combo::parse("ctrl+f9").unwrap(), codes());
        assert_eq!(t.feed(CTRL_L, true), None);
        assert_eq!(t.feed(F9, true), Some(Edge::Down));
        assert_eq!(t.feed(F9, false), Some(Edge::Up));
    }

    #[test]
    fn missing_keycodes_never_fire() {
        let mut t = Tracker::new(crate::combo::parse("ctrl+win").unwrap(), Codes::default());
        assert_eq!(t.feed(CTRL_L, true), None);
        assert_eq!(t.feed(SUPER_L, true), None);
    }
}
