//! Riavvio dell'applicazione dal menu della tray: lancia una nuova copia che aspetta la chiusura di questa.

use std::path::{Path, PathBuf};

/// Argomento con cui parte la copia lanciata dal riavvio.
pub const FLAG: &str = "--restart";

/// true se la copia deve attendere che la precedente finisca di chiudersi (dopo un aggiornamento o un riavvio).
pub fn waits_for_old(args: &[String]) -> bool {
    args.iter().any(|a| a == "--after-update" || a == FLAG)
}

/// Su Linux, se il file del programma e' stato sostituito mentre girava, `current_exe` aggiunge " (deleted)".
pub fn clean_exe_path(p: &Path) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_suffix(" (deleted)")) {
        Some(s) => PathBuf::from(s),
        None => p.to_path_buf(),
    }
}

/// Lancia la nuova copia. true se e' partita: allora la copia attuale deve chiudersi subito.
pub fn spawn_new() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    match std::process::Command::new(clean_exe_path(&exe)).arg(FLAG).spawn() {
        Ok(_) => true,
        Err(e) => {
            crate::config::log(&format!("restart failed: {e}"));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn restart_and_update_wait_for_the_old_copy() {
        assert!(waits_for_old(&v(&["exe", "--restart"])));
        assert!(waits_for_old(&v(&["exe", "--after-update"])));
        assert!(!waits_for_old(&v(&["exe"])));
        assert!(!waits_for_old(&v(&["exe", "--autostart"])));
    }

    #[test]
    fn deleted_suffix_is_removed() {
        assert_eq!(clean_exe_path(Path::new("/usr/bin/trascrittore (deleted)")), PathBuf::from("/usr/bin/trascrittore"));
        assert_eq!(clean_exe_path(Path::new("/usr/bin/trascrittore")), PathBuf::from("/usr/bin/trascrittore"));
    }
}
