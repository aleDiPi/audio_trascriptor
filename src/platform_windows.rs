//! Funzioni specifiche di Windows: app in primo piano, appunti/incolla, avvio automatico.

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CF_UNICODETEXT: u32 = 13;

/// Istanza unica: false se un'altra copia e' gia' in esecuzione. Dopo un aggiornamento
/// (`wait_old`) si attende qualche secondo che la vecchia copia finisca di chiudersi.
pub fn single_instance(wait_old: bool) -> bool {
    let name: Vec<u16> = "Local\\TrascrittoreAudioSingleInstance\0".encode_utf16().collect();
    for _ in 0..if wait_old { 40 } else { 1 } {
        unsafe {
            let h = CreateMutexW(null_mut(), 0, name.as_ptr());
            if !h.is_null() && GetLastError() != ERROR_ALREADY_EXISTS {
                return true;
            }
            if !h.is_null() {
                CloseHandle(h);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    false
}

/// Toglie i resti di un aggiornamento precedente.
pub fn cleanup_after_update() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("exe.old")); // resto di un aggiornamento
    }
    // installer scaricato da un aggiornamento precedente (se e' ancora in uso resta fino al prossimo avvio)
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("Trascrittore-Update"));
}

/// Apre un file, una cartella o un indirizzo web con il programma predefinito.
pub fn open(target: impl AsRef<std::ffi::OsStr>) {
    let _ = std::process::Command::new("explorer").arg(target).spawn();
}

/// (nome exe in minuscolo, titolo finestra) dell'app in primo piano.
pub fn foreground_app() -> (String, String) {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return Default::default();
        }
        let mut t = [0u16; 512];
        let n = GetWindowTextW(hwnd, t.as_mut_ptr(), 512).max(0) as usize;
        let title = String::from_utf16_lossy(&t[..n]);

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return (String::new(), title);
        }
        let mut p = [0u16; 1024];
        let mut len = p.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, p.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        let exe = if ok {
            let path = String::from_utf16_lossy(&p[..len as usize]);
            path.rsplit('\\').next().unwrap_or("").to_lowercase()
        } else {
            String::new()
        };
        (exe, title)
    }
}

/// Copia (da distruggere con DestroyIcon) dell'icona dell'app in primo piano: quella della finestra,
/// altrimenti quella dell'eseguibile. None se non se ne trova nessuna.
pub fn foreground_icon() -> Option<isize> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        for kind in [ICON_BIG as usize, ICON_SMALL2 as usize] {
            let mut r: usize = 0;
            let ok = SendMessageTimeoutW(hwnd, WM_GETICON, kind, 0, SMTO_ABORTIFHUNG | SMTO_BLOCK, 80, &mut r);
            if ok != 0 && r != 0 {
                let c = CopyIcon(r as HICON);
                if !c.is_null() {
                    return Some(c as isize);
                }
            }
        }
        let class_icon = GetClassLongPtrW(hwnd, GCLP_HICON);
        if class_icon != 0 {
            let c = CopyIcon(class_icon as HICON);
            if !c.is_null() {
                return Some(c as isize);
            }
        }
        // icona dell'eseguibile
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut p = [0u16; 1024];
        let mut len = p.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, p.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        if !ok {
            return None;
        }
        p[len as usize] = 0;
        let mut large: HICON = null_mut();
        let n = windows_sys::Win32::UI::Shell::ExtractIconExW(p.as_ptr(), 0, &mut large, null_mut(), 1);
        (n > 0 && !large.is_null()).then_some(large as isize)
    }
}

fn open_clipboard() -> bool {
    for _ in 0..10 {
        if unsafe { OpenClipboard(null_mut()) } != 0 {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

fn get_clipboard_text() -> Option<String> {
    if !open_clipboard() {
        return None;
    }
    let mut out = None;
    unsafe {
        let h = GetClipboardData(CF_UNICODETEXT);
        if !h.is_null() {
            let p = GlobalLock(h) as *const u16;
            if !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(h);
            }
        }
        CloseClipboard();
    }
    out
}

fn set_clipboard_text(s: &str) -> bool {
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    if !open_clipboard() {
        return false;
    }
    unsafe {
        EmptyClipboard();
        let h = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2);
        if h.is_null() {
            CloseClipboard();
            return false;
        }
        let p = GlobalLock(h) as *mut u16;
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        GlobalUnlock(h);
        let ok = !SetClipboardData(CF_UNICODETEXT, h).is_null();
        if !ok {
            GlobalFree(h);
        }
        CloseClipboard();
        ok
    }
}

fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 },
        },
    }
}

/// Incolla `text` nell'app in primo piano tramite appunti + scorciatoia di incolla.
pub fn paste_text(text: &str, paste: &str, restore: bool, send_enter: bool) -> Result<(), String> {
    let previous = if restore { get_clipboard_text() } else { None };
    if !set_clipboard_text(text) {
        return Err("appunti non accessibili".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(40));
    let (modifier, vk) = if paste.eq_ignore_ascii_case("shift+insert") {
        (VK_SHIFT, VK_INSERT)
    } else {
        (VK_CONTROL, b'V' as u16)
    };
    let inputs = [key(modifier, false), key(vk, false), key(vk, true), key(modifier, true)];
    unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
    if send_enter {
        // lascia all'app il tempo di completare l'incolla, poi invia
        std::thread::sleep(std::time::Duration::from_millis(400));
        let inputs = [key(VK_RETURN, false), key(VK_RETURN, true)];
        unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
        }
    }
    if let Some(prev) = previous {
        std::thread::sleep(std::time::Duration::from_millis(250));
        set_clipboard_text(&prev);
    }
    Ok(())
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_NAME: &str = "TrascrittoreAudio";

pub fn autostart_enabled() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(RUN_NAME))
        .is_ok()
}

/// Valore della chiave Run: percorso dell'exe tra virgolette (cosi' funziona anche con gli spazi).
fn run_value(exe: &std::path::Path) -> String {
    format!("\"{}\"", exe.display())
}

pub fn set_autostart(on: bool) {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    if let Ok((k, _)) = hkcu.create_subkey(RUN_KEY) {
        if on {
            if let Ok(exe) = std::env::current_exe() {
                let _ = k.set_value(RUN_NAME, &run_value(&exe));
            }
        } else {
            let _ = k.delete_value(RUN_NAME);
        }
    }
}

/// Avvia l'installer in silenzio dopo 2 secondi (il tempo di far chiudere questo programma).
/// L'installer e' per tutto il PC: `-Verb RunAs` mostra la richiesta di amministratore (UAC).
pub fn run_installer_after_exit(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let quoted = path.display().to_string().replace('\'', "''");
    let script = format!(
        "Start-Sleep -Seconds 2; Start-Process -FilePath '{quoted}' -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -Verb RunAs"
    );
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_value_quotes_path_with_spaces() {
        let v = run_value(std::path::Path::new(r"C:\Program Files\Trascrittore\trascrittore.exe"));
        assert_eq!(v, r#""C:\Program Files\Trascrittore\trascrittore.exe""#);
    }

    #[test]
    fn autostart_roundtrip_in_registry() {
        let before = autostart_enabled();
        set_autostart(true);
        assert!(autostart_enabled());
        set_autostart(false);
        assert!(!autostart_enabled());
        if before {
            set_autostart(true);
        }
    }
}
