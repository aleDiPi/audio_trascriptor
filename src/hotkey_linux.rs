//! Scorciatoia globale su X11 tramite gli eventi "raw" di XInput2: sono ricevuti da tutte le applicazioni senza
//! catturare la tastiera, quindi funzionano anche le combinazioni di soli modificatori (predefinita Ctrl+Win)
//! e il rilascio dei tasti per la modalita' "tieni premuto". I tasti non vengono "mangiati": conviene una
//! combinazione di soli modificatori, oppure con un tasto che non scrive nulla (F9, ecc.).

use crate::combo::Combo;
use crate::platform::{self, KS_ALT, KS_CONTROL, KS_SHIFT, KS_SUPER};
use crate::{config, macropad, Event};
use std::sync::Arc;
use x11rb::rust_connection::RustConnection;
use x11rb::protocol::xinput::{DeviceType, GrabOwner, XIGetPropertyItems};
use x11rb::protocol::xproto::{self, ChangeWindowAttributesAux, ConnectionExt as _, GrabMode, GrabStatus, PropMode};
use x11rb::wrapper::ConnectionExt as _;
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

/// Connessione condivisa per svegliare il thread dei dispositivi (da Impostazioni o dal ricaricamento).
static WAKE: OnceLock<(Arc<RustConnection>, u32, u32)> = OnceLock::new();

/// Chiede di riesaminare subito i dispositivi (la tastiera Stack Overflow e' stata attivata o disattivata).
pub fn refresh_pad() {
    if let Some((conn, root, atom)) = WAKE.get() {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u32);
        let _ = conn.change_property32(PropMode::REPLACE, *root, *atom, xproto::AtomEnum::CARDINAL, &[n]);
        let _ = conn.flush();
    }
}

/// Tastiera Stack Overflow collegata: identificativi dei dispositivi X (slave) e nome.
#[derive(Default)]
struct PadDevices {
    ids: HashSet<u16>,
    name: String,
}

fn product_id(conn: &RustConnection, dev: u16) -> Option<(u16, u16)> {
    let atom = conn.intern_atom(true, b"Device Product ID").ok()?.reply().ok()?.atom;
    if atom == 0 {
        return None;
    }
    let r = conn.xinput_xi_get_property(dev, false, atom, 0, 0, 2).ok()?.reply().ok()?;
    match r.items {
        XIGetPropertyItems::Data32(v) if v.len() >= 2 => Some((v[0] as u16, v[1] as u16)),
        _ => None,
    }
}

/// Cerca la tastiera tra i dispositivi e la "cattura" (grab): cosi' i suoi Ctrl/C/V non arrivano alle applicazioni.
fn scan_pad(conn: &RustConnection, root: u32, old: &PadDevices) -> PadDevices {
    let mut found = PadDevices::default();
    if config::macropad_enabled() {
        let extra = config::load().macropad_ids;
        if let Ok(reply) = conn.xinput_xi_query_device(0u16).map_err(|e| e.to_string()).and_then(|c| c.reply().map_err(|e| e.to_string())) {
            for info in reply.infos {
                if info.type_ != DeviceType::SLAVE_KEYBOARD {
                    continue;
                }
                let name = String::from_utf8_lossy(&info.name).to_string();
                let ids = product_id(conn, info.deviceid);
                config::debug(|| format!("keyboard device {}: '{name}', usb id {:?}", info.deviceid, ids.map(|(v, p)| format!("{v:04x}:{p:04x}"))));
                if macropad::is_pad(&name, ids, &extra) {
                    found.ids.insert(info.deviceid);
                    found.name = name;
                }
            }
        }
    }
    for dev in &found.ids {
        if !old.ids.contains(dev) {
            let grab = conn.xinput_xi_grab_device(root, 0u32, 0,  *dev, GrabMode::ASYNC, GrabMode::ASYNC, GrabOwner::NO_OWNER, &[]);
            let ok = grab.map_err(|e| e.to_string()).and_then(|c| c.reply().map_err(|e| e.to_string())).map(|r| r.status == GrabStatus::SUCCESS);
            config::log(&format!("Stack Overflow keyboard (device {dev}): exclusive grab {}", if ok == Ok(true) { "ok".to_string() } else { format!("failed ({ok:?})") }));
        }
    }
    for dev in old.ids.difference(&found.ids) {
        let _ = conn.xinput_xi_ungrab_device(0u32, *dev);
    }
    let _ = conn.flush();
    macropad::set_present(!found.ids.is_empty(), &found.name);
    found
}

fn pad_key(keycode: u8, map: &std::collections::HashMap<u32, Vec<u8>>) -> Option<macropad::Key> {
    let has = |syms: &[u32]| syms.iter().any(|s| map.get(s).map_or(false, |v| v.contains(&keycode)));
    if has(&[KS_CONTROL, 0xffe4]) {
        Some(macropad::Key::Ctrl)
    } else if has(&[0x63, 0x43]) {
        Some(macropad::Key::C)
    } else if has(&[0x76, 0x56]) {
        Some(macropad::Key::V)
    } else {
        None
    }
}

fn listen() -> Result<(), String> {
    let (conn, root) = platform::connect()?;
    let conn = Arc::new(conn);
    conn.xinput_xi_query_version(2, 0).map_err(|e| e.to_string())?.reply().map_err(|e| format!("XInput2 not available: {e}"))?;
    // 1 = XIAllMasterDevices, 0 = XIAllDevices (le modifiche ai dispositivi arrivano solo da qui)
    let mask = EventMask { deviceid: 1, mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE] };
    let hier = EventMask { deviceid: 0, mask: vec![XIEventMask::HIERARCHY] };
    conn.xinput_xi_select_events(root, &[mask, hier]).map_err(|e| e.to_string())?;
    // una proprieta' della finestra radice serve a svegliare questo thread dall'esterno (vedi refresh_pad)
    if let Ok(atom) = conn.intern_atom(false, b"_TRASCRITTORE_WAKE").map_err(|e| e.to_string()).and_then(|c| c.reply().map_err(|e| e.to_string())) {
        let _ = conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(xproto::EventMask::PROPERTY_CHANGE));
        let _ = WAKE.set((conn.clone(), root, atom.atom));
    }
    conn.flush().map_err(|e| e.to_string())?;

    let mut tracker = Tracker::default();
    let mut pad = scan_pad(&conn, root, &PadDevices::default());
    let mut pad_codes = platform::resolve_keycodes(&conn, &[KS_CONTROL, 0xffe4, 0x63, 0x43, 0x76, 0x56]);
    let mut pad_edges = macropad::Edges::default();
    loop {
        let (keycode, down, source) = match conn.wait_for_event().map_err(|e| format!("X11 connection lost: {e}"))? {
            XEvent::XinputRawKeyPress(e) => (e.detail, true, e.sourceid),
            XEvent::XinputRawKeyRelease(e) => (e.detail, false, e.sourceid),
            XEvent::XinputHierarchy(_) | XEvent::PropertyNotify(_) => {
                // dispositivo collegato/scollegato oppure impostazione cambiata
                pad = scan_pad(&conn, root, &pad);
                pad_codes = platform::resolve_keycodes(&conn, &[KS_CONTROL, 0xffe4, 0x63, 0x43, 0x76, 0x56]);
                pad_edges.clear();
                continue;
            }
            _ => continue,
        };
        let Ok(keycode) = u8::try_from(keycode) else { continue };
        if pad.ids.contains(&source) {
            // tasti della tastiera Stack Overflow: non fanno parte della scorciatoia normale
            if let Some(key) = pad_key(keycode, &pad_codes) {
                if let Some(edge) = pad_edges.feed(key.slot(), down) {
                    if let Some(tx) = TX.get() {
                        let _ = tx.send(Event::Pad(key.slot(), edge));
                    }
                }
            }
            continue;
        }
        let renew = {
            let mut s = SHARED.lock().unwrap();
            std::mem::replace(&mut s.dirty, false).then_some(s.combo)
        };
        if let Some(combo) = renew {
            tracker = Tracker::new(combo, codes_for(&combo, &conn));
        }
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
