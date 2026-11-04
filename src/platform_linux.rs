//! Funzioni specifiche di Linux (sessione X11, per esempio Linux Mint Cinnamon): istanza unica, app in primo piano,
//! appunti e incolla, avvio automatico (XDG autostart).

use crate::combo;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, Window};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

// ---------- istanza unica (socket Unix) e "mostra le impostazioni" da una seconda copia

/// Percorso del socket dell'istanza in esecuzione: `$XDG_RUNTIME_DIR/trascrittore.sock`, altrimenti in /tmp per utente.
pub fn socket_path(runtime_dir: Option<&str>, user: Option<&str>) -> PathBuf {
    match runtime_dir.filter(|d| !d.is_empty()) {
        Some(d) => Path::new(d).join("trascrittore.sock"),
        None => PathBuf::from(format!("/tmp/trascrittore-{}.sock", user.filter(|u| !u.is_empty()).unwrap_or("utente"))),
    }
}

fn default_socket_path() -> PathBuf {
    socket_path(std::env::var("XDG_RUNTIME_DIR").ok().as_deref(), std::env::var("USER").ok().as_deref())
}

pub enum Claim {
    /// Questa e' la prima copia: ascolta le richieste delle altre.
    First(UnixListener),
    /// Un'altra copia e' gia' in esecuzione (la connessione per parlarle).
    Running(UnixStream),
    /// Impossibile usare il socket: si prosegue senza controllo di istanza unica.
    Unavailable,
}

pub fn claim(path: &Path) -> Claim {
    if let Ok(l) = UnixListener::bind(path) {
        return Claim::First(l);
    }
    if let Ok(s) = UnixStream::connect(path) {
        return Claim::Running(s);
    }
    // socket rimasto dopo una chiusura anomala: nessuno risponde, lo si rimuove e lo si riprende
    let _ = std::fs::remove_file(path);
    UnixListener::bind(path).map_or(Claim::Unavailable, Claim::First)
}

/// Serve le richieste delle altre copie: ogni riga "show" chiama `on_show`. Non ritorna finche' il socket vive.
pub fn serve(listener: UnixListener, on_show: impl Fn()) {
    for stream in listener.incoming().flatten() {
        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_ok() && line.trim() == "show" {
            on_show();
        }
    }
}

/// Avvio "silenzioso": dall'avvio automatico o dopo un aggiornamento non si apre la finestra delle Impostazioni.
pub fn quiet_start() -> bool {
    std::env::args().any(|a| a == "--autostart" || a == "--after-update")
}

/// Istanza unica: false se un'altra copia e' gia' in esecuzione. In quel caso, se la nuova copia e' stata
/// lanciata dal menu (non dall'avvio automatico), chiede alla prima di aprire le Impostazioni.
pub fn single_instance(wait_old: bool) -> bool {
    let path = default_socket_path();
    // dopo un aggiornamento si attende qualche secondo che la vecchia copia finisca di chiudersi
    for attempt in 0..if wait_old { 40 } else { 1 } {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        match claim(&path) {
            Claim::First(l) => {
                let _ = std::thread::Builder::new().name("istanza".into()).spawn(move || serve(l, crate::gui::show));
                return true;
            }
            Claim::Unavailable => return true,
            Claim::Running(mut s) => {
                if !wait_old && !quiet_start() {
                    let _ = s.write_all(b"show\n");
                }
            }
        }
    }
    false
}

pub fn cleanup_after_update() {}

/// Apre un file, una cartella o un indirizzo web con il programma predefinito (xdg-open).
pub fn open(target: impl AsRef<std::ffi::OsStr>) {
    if let Ok(mut child) = std::process::Command::new("xdg-open")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Argomenti di `zenity` per una domanda con due pulsanti (il primo e' "OK").
pub fn zenity_args(title: &str, text: &str, ok_label: &str, cancel_label: &str) -> Vec<String> {
    vec![
        "--question".into(),
        "--no-markup".into(),
        format!("--title={title}"),
        format!("--text={text}"),
        format!("--ok-label={ok_label}"),
        format!("--cancel-label={cancel_label}"),
        "--width=420".into(),
    ]
}

/// Finestra di avviso con due pulsanti: restituisce true se l'utente sceglie il primo. Usa `zenity` (presente in
/// Linux Mint); se manca ripiega su una notifica `notify-send` e restituisce false. Blocca finche' la finestra
/// e' aperta: va chiamata da un thread a parte.
pub fn ask(title: &str, text: &str, ok_label: &str, cancel_label: &str) -> bool {
    match std::process::Command::new("zenity").args(zenity_args(title, text, ok_label, cancel_label)).status() {
        Ok(status) => status.success(),
        Err(_) => {
            let _ = std::process::Command::new("notify-send").args([title, text]).status();
            false
        }
    }
}

// ---------- X11

pub fn connect() -> Result<(RustConnection, Window), String> {
    let (conn, screen) = RustConnection::connect(None).map_err(|e| format!("X11 display server not reachable: {e}"))?;
    let root = conn.setup().roots[screen].root;
    Ok((conn, root))
}

fn atom(conn: &RustConnection, name: &str) -> Option<Atom> {
    Some(conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom).filter(|a| *a != 0)
}

fn prop32(conn: &RustConnection, win: Window, name: &str, ty: AtomEnum) -> Option<u32> {
    let reply = conn.get_property(false, win, atom(conn, name)?, ty, 0, 1).ok()?.reply().ok()?;
    let v = reply.value32()?.next();
    v
}

fn prop_bytes(conn: &RustConnection, win: Window, name: &str, ty: Option<&str>) -> Option<Vec<u8>> {
    let ty = match ty {
        Some(t) => atom(conn, t)?,
        None => AtomEnum::STRING.into(),
    };
    let reply = conn.get_property(false, win, atom(conn, name)?, ty, 0, 1024).ok()?.reply().ok()?;
    (!reply.value.is_empty()).then_some(reply.value)
}

/// Nome del programma: il file eseguibile del processo della finestra, altrimenti la classe `WM_CLASS`
/// (app in Flatpak/Snap, di cui /proc non e' leggibile). Sempre in minuscolo.
pub fn app_name(exe_link: Option<&str>, wm_class: Option<&str>) -> String {
    if let Some(link) = exe_link {
        let link = link.strip_suffix(" (deleted)").unwrap_or(link);
        if let Some(name) = link.rsplit('/').next().filter(|n| !n.is_empty()) {
            return name.to_lowercase();
        }
    }
    // WM_CLASS = "istanza\0Classe\0": vale la classe
    wm_class.and_then(|c| c.split('\0').filter(|p| !p.is_empty()).last()).map(str::to_lowercase).unwrap_or_default()
}

/// (nome del programma in minuscolo, titolo finestra) dell'app in primo piano.
pub fn foreground_app() -> (String, String) {
    foreground_app_inner().unwrap_or_default()
}

fn foreground_app_inner() -> Option<(String, String)> {
    let (conn, root) = connect().ok()?;
    let win = prop32(&conn, root, "_NET_ACTIVE_WINDOW", AtomEnum::ANY).filter(|w| *w != 0)?;
    let title = prop_bytes(&conn, win, "_NET_WM_NAME", Some("UTF8_STRING"))
        .or_else(|| prop_bytes(&conn, win, "WM_NAME", None))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let exe_link = prop32(&conn, win, "_NET_WM_PID", AtomEnum::CARDINAL)
        .and_then(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).ok())
        .map(|p| p.display().to_string());
    let class = prop_bytes(&conn, win, "WM_CLASS", None).map(|b| String::from_utf8_lossy(&b).into_owned());
    Some((app_name(exe_link.as_deref(), class.as_deref()), title))
}

/// Icona (`_NET_WM_ICON`) dell'app in primo piano, per il bollino a schermo. None se la finestra non la espone.
pub fn foreground_icon() -> Option<crate::overlay_render::Icon> {
    let (conn, root) = connect().ok()?;
    let win = prop32(&conn, root, "_NET_ACTIVE_WINDOW", AtomEnum::ANY).filter(|w| *w != 0)?;
    let reply = conn.get_property(false, win, atom(&conn, "_NET_WM_ICON")?, AtomEnum::CARDINAL, 0, 1 << 20).ok()?.reply().ok()?;
    let data: Vec<u32> = reply.value32()?.collect();
    crate::overlay_render::pick_icon(&data, crate::overlay_render::ICON_PX)
}

// ---------- tasti

pub const KS_SHIFT: u32 = 0xffe1;
pub const KS_CONTROL: u32 = 0xffe3;
pub const KS_ALT: u32 = 0xffe9;
pub const KS_SUPER: u32 = 0xffeb;
const KS_RETURN: u32 = 0xff0d;

/// Tasto della scorciatoia (codice virtuale di Windows, vedi `combo`) -> keysym di X11.
pub fn vk_to_keysym(vk: u32) -> Option<u32> {
    match vk {
        0x30..=0x39 => Some(vk),                  // cifre: '0'..'9'
        0x41..=0x5A => Some(vk + 0x20),           // lettere: 'a'..'z'
        0x20 => Some(0x20),                       // spazio
        0x09 => Some(0xff09),                     // Tab
        0x0D => Some(KS_RETURN),                  // Invio
        0x1B => Some(0xff1b),                     // Esc
        0x2D => Some(0xff63),                     // Ins
        0x70..=0x87 => Some(0xffbe + (vk - 0x70)), // F1..F24
        _ => None,
    }
}

/// Tutti i keycode (uno per layout/lato) che producono `keysym` in una mappa tastiera di X11.
pub fn keycodes_for(keysyms: &[u32], per_keycode: usize, min_keycode: u8, keysym: u32) -> Vec<u8> {
    if per_keycode == 0 {
        return Vec::new();
    }
    keysyms
        .chunks(per_keycode)
        .enumerate()
        .filter(|(_, syms)| syms.iter().take(2).any(|s| *s == keysym))
        .map(|(i, _)| min_keycode.saturating_add(i as u8))
        .collect()
}

/// Keycode attuali per ciascun keysym richiesto (la mappa viene letta una volta sola).
pub fn resolve_keycodes(conn: &RustConnection, wanted: &[u32]) -> HashMap<u32, Vec<u8>> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let Some(map) = conn.get_keyboard_mapping(min, max - min + 1).ok().and_then(|c| c.reply().ok()) else {
        return HashMap::new();
    };
    wanted.iter().map(|k| (*k, keycodes_for(&map.keysyms, map.keysyms_per_keycode as usize, min, *k))).collect()
}

/// Scorciatoia di incolla ("ctrl+v", "shift+insert", "ctrl+shift+v") -> keysym da premere: modificatori e tasto.
pub fn paste_keys(spec: &str) -> Vec<u32> {
    let parsed = combo::parse(spec).ok().filter(|c| c.key.is_some()).unwrap_or_else(|| combo::parse("ctrl+v").unwrap());
    let mut keys = Vec::new();
    for (on, ks) in [(parsed.ctrl, KS_CONTROL), (parsed.alt, KS_ALT), (parsed.shift, KS_SHIFT), (parsed.win, KS_SUPER)] {
        if on {
            keys.push(ks);
        }
    }
    keys.extend(parsed.key.and_then(vk_to_keysym));
    keys
}

/// Preme i tasti insieme (nell'ordine) e li rilascia al contrario, con XTest.
fn send_keys(keysyms: &[u32]) -> Result<(), String> {
    let (conn, root) = connect()?;
    let map = resolve_keycodes(&conn, keysyms);
    let codes: Vec<u8> = keysyms.iter().filter_map(|k| map.get(k).and_then(|v| v.first().copied())).collect();
    if codes.len() != keysyms.len() {
        return Err(crate::i18n::t("err.paste_key"));
    }
    let fake = |ty: u8, code: u8| conn.xtest_fake_input(ty, code, 0, root, 0, 0, 0).map(|_| ()).map_err(|e| e.to_string());
    for c in &codes {
        fake(2, *c)?; // KeyPress
    }
    for c in codes.iter().rev() {
        fake(3, *c)?; // KeyRelease
    }
    // attende che il server abbia eseguito tutto
    let done = conn.get_input_focus().map_err(|e| e.to_string())?.reply().map(|_| ()).map_err(|e| e.to_string());
    done
}

// ---------- appunti e incolla

static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Su X11 chi imposta gli appunti deve restare in vita per consegnarli: l'oggetto resta in questa variabile.
fn with_clipboard<T>(f: impl FnOnce(&mut arboard::Clipboard) -> T) -> Option<T> {
    let mut guard = CLIPBOARD.lock().ok()?;
    if guard.is_none() {
        *guard = arboard::Clipboard::new().ok();
    }
    guard.as_mut().map(f)
}

/// Incolla `text` nell'app in primo piano tramite appunti + scorciatoia di incolla.
pub fn paste_text(text: &str, paste: &str, restore: bool, send_enter: bool) -> Result<(), String> {
    let previous = if restore { with_clipboard(|c| c.get_text().ok()).flatten() } else { None };
    if with_clipboard(|c| c.set_text(text.to_string()).is_ok()) != Some(true) {
        return Err(crate::i18n::t("err.clipboard"));
    }
    std::thread::sleep(std::time::Duration::from_millis(40));
    send_keys(&paste_keys(paste))?;
    if send_enter {
        // lascia all'app il tempo di completare l'incolla, poi invia
        std::thread::sleep(std::time::Duration::from_millis(400));
        send_keys(&[KS_RETURN])?;
    }
    if let Some(prev) = previous {
        std::thread::sleep(std::time::Duration::from_millis(250));
        with_clipboard(|c| c.set_text(prev));
    }
    Ok(())
}

// ---------- avvio automatico (XDG autostart)

const DESKTOP_NAME: &str = "trascrittore.desktop";
/// Voce installata dal pacchetto .deb: avvia il programma al login di tutti gli utenti.
const SYSTEM_AUTOSTART: &str = "/etc/xdg/autostart/trascrittore.desktop";

fn config_home() -> PathBuf {
    crate::config::linux_config_home(std::env::var("XDG_CONFIG_HOME").ok().as_deref(), std::env::var("HOME").ok().as_deref())
}

/// Quota il percorso per il campo `Exec` di un file .desktop (spazi e caratteri speciali).
pub fn desktop_exec_quote(exe: &str) -> String {
    if exe.chars().any(|c| " \t\n\"'\\><~|&;$*?#()`".contains(c)) {
        let escaped: String = exe.chars().flat_map(|c| if "\"`$\\".contains(c) { vec!['\\', c] } else { vec![c] }).collect();
        format!("\"{escaped}\"")
    } else {
        exe.to_string()
    }
}

/// Contenuto del file .desktop dell'avvio automatico per l'utente.
pub fn autostart_entry(exe: &str, hidden: bool) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Trascrittore audio\nComment=Voice dictation in the tray\nComment[it]=Dettatura vocale nella tray\nExec={} --autostart\nIcon=trascrittore\nTerminal=false\nX-GNOME-Autostart-enabled={}\n{}",
        desktop_exec_quote(exe),
        !hidden,
        if hidden { "Hidden=true\n" } else { "" }
    )
}

/// Un file .desktop e' disattivato se contiene `Hidden=true` (regola della specifica XDG).
pub fn entry_hidden(content: &str) -> bool {
    content.lines().any(|l| l.trim().eq_ignore_ascii_case("Hidden=true"))
}

/// Stato dell'avvio automatico: la voce dell'utente vince su quella di sistema.
pub fn autostart_state(user_entry: Option<&str>, system_entry_exists: bool) -> bool {
    match user_entry {
        Some(c) => !entry_hidden(c),
        None => system_entry_exists,
    }
}

pub fn autostart_enabled() -> bool {
    let user = std::fs::read_to_string(config_home().join("autostart").join(DESKTOP_NAME)).ok();
    autostart_state(user.as_deref(), Path::new(SYSTEM_AUTOSTART).is_file())
}

pub fn set_autostart(on: bool) {
    let Ok(exe) = std::env::current_exe() else { return };
    write_autostart(&config_home(), &exe.display().to_string(), on);
}

fn write_autostart(config_home: &Path, exe: &str, on: bool) {
    let dir = config_home.join("autostart");
    if std::fs::create_dir_all(&dir).is_ok() {
        // se la voce e' spenta resta un file "Hidden=true": nasconde anche quella di sistema
        let _ = std::fs::write(dir.join(DESKTOP_NAME), autostart_entry(exe, !on));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trascrittore-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Prova su un vero server X11 (per esempio Xvfb con xterm in primo piano): `cargo test -- --ignored paste_smoke`.
    #[test]
    #[ignore]
    fn paste_smoke() {
        let spec = std::env::var("PASTE_SPEC").unwrap_or_else(|_| "shift+insert".into());
        paste_text("ciao dal test", &spec, false, true).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2)); // il tempo di leggere gli appunti
    }

    /// Con una finestra attiva su un vero server X11: `cargo test -- --ignored --nocapture foreground_smoke`.
    #[test]
    #[ignore]
    fn foreground_smoke() {
        let (exe, title) = foreground_app();
        println!("app in primo piano: '{exe}' - '{title}'");
        assert!(!exe.is_empty());
    }

    #[test]
    fn zenity_question_has_title_text_and_both_buttons() {
        let a = zenity_args("Titolo", "Testo\ncon due righe", "Apri", "Chiudi");
        assert_eq!(a[0], "--question");
        assert!(a.contains(&"--title=Titolo".to_string()));
        assert!(a.contains(&"--text=Testo\ncon due righe".to_string()));
        assert!(a.contains(&"--ok-label=Apri".to_string()) && a.contains(&"--cancel-label=Chiudi".to_string()));
        assert!(a.contains(&"--no-markup".to_string())); // il testo non va interpretato come markup
    }

    #[test]
    fn socket_path_prefers_runtime_dir() {
        assert_eq!(socket_path(Some("/run/user/1000"), Some("ale")), PathBuf::from("/run/user/1000/trascrittore.sock"));
        assert_eq!(socket_path(None, Some("ale")), PathBuf::from("/tmp/trascrittore-ale.sock"));
        assert_eq!(socket_path(Some(""), None), PathBuf::from("/tmp/trascrittore-utente.sock"));
    }

    #[test]
    fn second_copy_is_detected_and_can_ask_to_show() {
        let dir = tmp("istanza");
        let path = dir.join("t.sock");
        let Claim::First(listener) = claim(&path) else { panic!("la prima copia deve vincere") };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || serve(listener, move || tx.send(()).unwrap()));
        let Claim::Running(mut s) = claim(&path) else { panic!("la seconda copia deve vedere la prima") };
        s.write_all(b"show\n").unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(5)).expect("la prima copia deve ricevere 'show'");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_socket_is_taken_over() {
        let dir = tmp("stantio");
        let path = dir.join("t.sock");
        drop(UnixListener::bind(&path).unwrap()); // il file resta, ma nessuno ascolta
        assert!(matches!(claim(&path), Claim::First(_)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn app_name_uses_exe_then_wm_class() {
        assert_eq!(app_name(Some("/usr/bin/Thunderbird"), Some("Mail\0Thunderbird\0")), "thunderbird");
        assert_eq!(app_name(Some("/opt/x/app (deleted)"), None), "app");
        assert_eq!(app_name(None, Some("navigator\0Firefox\0")), "firefox");
        assert_eq!(app_name(None, None), "");
    }

    #[test]
    fn keys_map_to_keysyms() {
        assert_eq!(vk_to_keysym(b'V' as u32), Some(0x76));
        assert_eq!(vk_to_keysym(b'7' as u32), Some(0x37));
        assert_eq!(vk_to_keysym(0x20), Some(0x20));
        assert_eq!(vk_to_keysym(0x70), Some(0xffbe)); // F1
        assert_eq!(vk_to_keysym(0x7B), Some(0xffc9)); // F12
        assert_eq!(vk_to_keysym(0x2D), Some(0xff63));
        assert_eq!(vk_to_keysym(0xFF), None);
    }

    #[test]
    fn keycodes_are_found_in_the_keyboard_map() {
        // 3 keycode a partire dal 8, 2 colonne ciascuno: Control_L, 'v'/'V', Control_L/Control_R
        let map = [KS_CONTROL, 0, 0x76, 0x56, KS_CONTROL, 0xffe4];
        assert_eq!(keycodes_for(&map, 2, 8, KS_CONTROL), vec![8, 10]);
        assert_eq!(keycodes_for(&map, 2, 8, 0x76), vec![9]);
        assert!(keycodes_for(&map, 2, 8, 0x77).is_empty());
        assert!(keycodes_for(&map, 0, 8, 0x76).is_empty());
    }

    #[test]
    fn paste_shortcuts_become_key_sequences() {
        assert_eq!(paste_keys("ctrl+v"), vec![KS_CONTROL, 0x76]);
        assert_eq!(paste_keys("shift+insert"), vec![KS_SHIFT, 0xff63]);
        assert_eq!(paste_keys("ctrl+shift+v"), vec![KS_CONTROL, KS_SHIFT, 0x76]);
        // valore non valido o senza tasto: si ripiega su Ctrl+V
        assert_eq!(paste_keys("boh"), vec![KS_CONTROL, 0x76]);
        assert_eq!(paste_keys("ctrl"), vec![KS_CONTROL, 0x76]);
    }

    #[test]
    fn exec_is_quoted_only_when_needed() {
        assert_eq!(desktop_exec_quote("/usr/bin/trascrittore"), "/usr/bin/trascrittore");
        assert_eq!(desktop_exec_quote("/opt/Mio Programma/t"), "\"/opt/Mio Programma/t\"");
    }

    #[test]
    fn autostart_entry_and_state() {
        let on = autostart_entry("/usr/bin/trascrittore", false);
        assert!(on.contains("Exec=/usr/bin/trascrittore --autostart") && !on.contains("Hidden"));
        let off = autostart_entry("/usr/bin/trascrittore", true);
        assert!(entry_hidden(&off) && !entry_hidden(&on));
        // la voce dell'utente vince su quella di sistema
        assert!(autostart_state(Some(&on), false));
        assert!(!autostart_state(Some(&off), true));
        assert!(autostart_state(None, true));
        assert!(!autostart_state(None, false));
    }

    #[test]
    fn autostart_file_roundtrip() {
        let dir = tmp("autostart");
        write_autostart(&dir, "/usr/bin/trascrittore", true);
        let f = std::fs::read_to_string(dir.join("autostart").join(DESKTOP_NAME)).unwrap();
        assert!(autostart_state(Some(&f), false));
        write_autostart(&dir, "/usr/bin/trascrittore", false);
        let f = std::fs::read_to_string(dir.join("autostart").join(DESKTOP_NAME)).unwrap();
        assert!(!autostart_state(Some(&f), true));
        let _ = std::fs::remove_dir_all(dir);
    }
}
