//! Scorciatoia globale tramite hook di tastiera a basso livello.
//! Serve (invece di RegisterHotKey) perche' permette combinazioni di soli modificatori,
//! come il predefinito Ctrl+Win, e il rilascio dei tasti per la modalita' "tieni premuto".

use crate::combo::Combo;
use crate::Event;
use std::ptr::null_mut;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

struct State {
    combo: Combo,
    down: [bool; 256],
    active: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    combo: Combo { ctrl: true, alt: false, shift: false, win: true, key: None },
    down: [false; 256],
    active: false,
});
static TX: OnceLock<Sender<Event>> = OnceLock::new();

pub fn set_combo(c: Combo) {
    let mut s = STATE.lock().unwrap();
    s.combo = c;
    s.active = false;
}

/// Da chiamare dal thread che esegue il message loop.
pub fn install(tx: Sender<Event>) {
    let _ = TX.set(tx);
    unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc_), null_mut(), 0);
    }
}

unsafe extern "system" fn proc_(code: i32, w: usize, l: isize) -> isize {
    if code == HC_ACTION as i32 {
        let k = &*(l as *const KBDLLHOOKSTRUCT);
        if k.flags & LLKHF_INJECTED == 0 {
            let w = w as u32;
            let down = w == WM_KEYDOWN || w == WM_SYSKEYDOWN;
            let up = w == WM_KEYUP || w == WM_SYSKEYUP;
            if (down || up) && handle(k.vkCode, down) {
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, w, l)
}

/// Restituisce true se l'evento va "mangiato" (tasto principale della combinazione).
fn handle(vk: u32, down: bool) -> bool {
    let mut s = STATE.lock().unwrap();
    if (vk as usize) < 256 {
        s.down[vk as usize] = down;
    }
    let d = |v: u32| s.down[v as usize];
    let c = s.combo;
    let mods = (!c.ctrl || d(0xA2) || d(0xA3))
        && (!c.alt || d(0xA4) || d(0xA5))
        && (!c.shift || d(0xA0) || d(0xA1))
        && (!c.win || d(0x5B) || d(0x5C));
    let satisfied = mods && c.key.map_or(true, |k| d(k));
    let swallow = c.key == Some(vk) && (mods || s.active);
    let was_active = s.active;
    s.active = satisfied;
    drop(s);

    if satisfied && !was_active {
        if c.win {
            suppress_start_menu();
        }
        send(Event::HotkeyDown);
    } else if !satisfied && was_active {
        send(Event::HotkeyUp);
    }
    swallow
}

fn send(e: Event) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(e);
    }
}

/// Un tasto "neutro" tra pressione e rilascio di Win evita l'apertura del menu Start.
fn suppress_start_menu() {
    unsafe {
        let mk = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: 0xE8, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
            },
        };
        let inputs = [mk(0), mk(KEYEVENTF_KEYUP)];
        SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}
