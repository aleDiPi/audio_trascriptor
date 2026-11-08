//! Tastiera a 3 tasti di Stack Overflow su Windows: i messaggi "raw input" dicono da quale tastiera arriva ogni tasto.
//! Il dispositivo si riconosce dal nome (stringa USB del prodotto) o dal VID:PID in `macropad_ids`.
//! Il filtro a basso livello (hotkey_windows.rs) chiede qui se un tasto appena premuto viene dalla tastiera Stack
//! Overflow: in tal caso lo "mangia", cosi' Ctrl, C e V non arrivano all'applicazione in primo piano.

use crate::macropad::{self, Key};
use crate::config;
use std::collections::HashMap;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows_sys::Win32::Devices::HumanInterfaceDevice::HidD_GetProductString;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_RESCAN: u32 = WM_APP + 10;
/// Quanto a lungo un tasto della tastiera Stack Overflow resta "in attesa" del filtro a basso livello.
const MATCH_WINDOW: Duration = Duration::from_millis(150);
/// Attesa massima del filtro per sapere da quale tastiera arriva un tasto (solo Ctrl/C/V, solo con la tastiera collegata).
const HOOK_WAIT: Duration = Duration::from_millis(4);

static HWND_SELF: AtomicIsize = AtomicIsize::new(0);
static KNOWN: Mutex<Option<HashMap<isize, (bool, String)>>> = Mutex::new(None);
static LAST: Mutex<Option<(Key, bool, Instant)>> = Mutex::new(None);

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn device_path(h: HANDLE) -> Option<String> {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, null_mut(), &mut len);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        let n = GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, buf.as_mut_ptr().cast(), &mut len);
        if n == u32::MAX {
            return None;
        }
        Some(String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string())
    }
}

/// Nome del prodotto dal dispositivo USB (stringa vuota se non si riesce a leggerlo).
fn product_name(path: &str) -> String {
    unsafe {
        let wide = w(path);
        let h = CreateFileW(wide.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, 0, null_mut());
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            return String::new();
        }
        let mut buf = [0u16; 128];
        let ok = HidD_GetProductString(h, buf.as_mut_ptr().cast(), (buf.len() * 2) as u32);
        CloseHandle(h);
        if ok {
            String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string()
        } else {
            String::new()
        }
    }
}

/// Il dispositivo e' la tastiera Stack Overflow? (la risposta si ricorda finche' non cambiano i dispositivi)
fn is_pad_device(h: HANDLE) -> (bool, String) {
    if let Some(v) = KNOWN.lock().unwrap().as_ref().and_then(|m| m.get(&(h as isize))) {
        return v.clone();
    }
    let result = match device_path(h) {
        Some(path) => {
            let name = product_name(&path);
            let ids = macropad::parse_hid_path(&path);
            let pad = macropad::is_pad(&name, ids, &config::load().macropad_ids);
            config::debug(|| format!("keyboard device '{name}', usb id {:?}, path {path}: {}", ids.map(|(v, p)| format!("{v:04x}:{p:04x}")), if pad { "Stack Overflow keyboard" } else { "ordinary keyboard" }));
            (pad, name)
        }
        None => (false, String::new()),
    };
    KNOWN.lock().unwrap().get_or_insert_with(HashMap::new).insert(h as isize, result.clone());
    result
}

/// Rilegge l'elenco dei dispositivi e aggiorna "collegata / non collegata".
fn rescan() {
    *KNOWN.lock().unwrap() = None;
    let mut found = None;
    if config::macropad_enabled() {
        unsafe {
            let mut count = 0u32;
            let size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
            GetRawInputDeviceList(null_mut(), &mut count, size);
            let mut list = vec![RAWINPUTDEVICELIST { hDevice: null_mut(), dwType: 0 }; count as usize];
            let n = GetRawInputDeviceList(list.as_mut_ptr(), &mut count, size);
            if n != u32::MAX {
                for d in list.iter().take(n as usize).filter(|d| d.dwType == RIM_TYPEKEYBOARD) {
                    let (pad, name) = is_pad_device(d.hDevice);
                    if pad {
                        found = Some(name);
                    }
                }
            }
        }
    }
    macropad::reset_keys();
    macropad::set_present(found.is_some(), found.as_deref().unwrap_or(""));
}

/// Chiede di rileggere i dispositivi (la funzione e' stata attivata o disattivata dalle Impostazioni).
pub fn refresh_pad() {
    let h = HWND_SELF.load(Ordering::Acquire);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, WM_RESCAN, 0, 0) };
    }
}

/// Avvia il thread con la finestra invisibile che riceve i messaggi raw input (a riposo non usa CPU).
pub fn start() {
    std::thread::spawn(|| unsafe {
        let class = w("TrascrittorePad");
        let hinst = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(0, class.as_ptr(), null(), 0, 0, 0, 0, 0, HWND_MESSAGE, null_mut(), hinst, null());
        if hwnd.is_null() {
            config::log("Stack Overflow keyboard: window not created");
            return;
        }
        HWND_SELF.store(hwnd as isize, Ordering::Release);
        // tastiere (pagina 1, uso 6): INPUTSINK = anche quando l'app non e' in primo piano; DEVNOTIFY = collegamenti e scollegamenti
        let rid = RAWINPUTDEVICE { usUsagePage: 1, usUsage: 6, dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY, hwndTarget: hwnd };
        if RegisterRawInputDevices(&rid, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            config::log("Stack Overflow keyboard: raw input not available");
            return;
        }
        rescan();
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe fn on_input(lp: LPARAM) {
    if !macropad::present() || !config::macropad_enabled() {
        return;
    }
    let mut size = 0u32;
    let head = std::mem::size_of::<RAWINPUTHEADER>() as u32;
    GetRawInputData(lp as HRAWINPUT, RID_INPUT, null_mut(), &mut size, head);
    if size == 0 {
        return;
    }
    // buffer allineato a 8 byte (la struttura contiene puntatori)
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    if GetRawInputData(lp as HRAWINPUT, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, head) == u32::MAX {
        return;
    }
    let raw = &*(buf.as_ptr() as *const RAWINPUT);
    if raw.header.dwType != RIM_TYPEKEYBOARD {
        return;
    }
    if !is_pad_device(raw.header.hDevice).0 {
        return;
    }
    let kb = raw.data.keyboard;
    let down = kb.Flags & 1 == 0; // RI_KEY_BREAK = 1
    let Some(key) = macropad::key_for_vk(kb.VKey as u32) else { return };
    *LAST.lock().unwrap() = Some((key, down, Instant::now()));
    macropad::feed_key(key, down);
}

/// Chiamata dal filtro a basso livello per ogni tasto: true = viene dalla tastiera Stack Overflow, va "mangiato".
pub fn swallow(vk: u32, down: bool) -> bool {
    if !macropad::present() {
        return false;
    }
    let Some(key) = macropad::key_for_vk(vk) else { return false };
    let matches = || {
        LAST.lock().unwrap().as_ref().map_or(false, |(k, d, t)| *k == key && *d == down && t.elapsed() < MATCH_WINDOW)
    };
    // il messaggio raw input puo' arrivare un attimo dopo il filtro: si aspetta pochissimo
    let start = Instant::now();
    loop {
        if matches() {
            *LAST.lock().unwrap() = None;
            return true;
        }
        if start.elapsed() >= HOOK_WAIT {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            on_input(lp);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_INPUT_DEVICE_CHANGE | WM_RESCAN => {
            rescan();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
