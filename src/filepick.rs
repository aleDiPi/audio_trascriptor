//! Native "open file" / "save file" dialog. Windows: the system dialog. Linux: zenity or kdialog when installed
//! (the .deb recommends none of them as a hard dependency); without them the caller keeps the typed path.

use std::path::{Path, PathBuf};

/// Arguments for `zenity` (dialog to open or save a file).
pub fn zenity_args(save: bool, suggested: &Path) -> Vec<String> {
    let mut a = vec!["--file-selection".to_string()];
    if save {
        a.push("--save".into());
        a.push("--confirm-overwrite".into());
    }
    a.push(format!("--filename={}", suggested.display()));
    a.push("--file-filter=*.json".into());
    a
}

/// Arguments for `kdialog`.
pub fn kdialog_args(save: bool, suggested: &Path) -> Vec<String> {
    vec![if save { "--getsavefilename" } else { "--getopenfilename" }.into(), suggested.display().to_string(), "*.json".into()]
}

/// Cleans the output of a command-line dialog: one line, empty = cancelled.
pub fn parse_output(out: &str) -> Option<PathBuf> {
    let line = out.trim_end_matches(['\n', '\r']).trim();
    (!line.is_empty()).then(|| PathBuf::from(line))
}

/// Shows the dialog and waits. `None` = cancelled or no dialog available (see `available`).
#[cfg(target_os = "linux")]
pub fn pick(save: bool, suggested: &Path) -> Option<PathBuf> {
    for (prog, args) in [("zenity", zenity_args(save, suggested)), ("kdialog", kdialog_args(save, suggested))] {
        if let Ok(out) = std::process::Command::new(prog).args(&args).output() {
            return parse_output(&String::from_utf8_lossy(&out.stdout));
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub fn available() -> bool {
    ["zenity", "kdialog"].iter().any(|p| std::process::Command::new(p).arg("--version").output().is_ok())
}

#[cfg(windows)]
pub fn pick(save: bool, suggested: &Path) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().add_filter("JSON", &["json"]);
    if let Some(dir) = suggested.parent() {
        d = d.set_directory(dir);
    }
    if save {
        d = d.set_file_name(suggested.file_name()?.to_string_lossy());
        d.save_file()
    } else {
        d.pick_file()
    }
}

#[cfg(windows)]
pub fn available() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zenity_save_dialog_asks_before_overwriting() {
        let a = zenity_args(true, Path::new("/home/a/x.json"));
        assert!(a.contains(&"--save".to_string()) && a.contains(&"--confirm-overwrite".to_string()));
        assert!(a.contains(&"--filename=/home/a/x.json".to_string()));
        assert!(!zenity_args(false, Path::new("/x")).contains(&"--save".to_string()));
    }

    #[test]
    fn kdialog_picks_open_or_save() {
        assert_eq!(kdialog_args(true, Path::new("/x.json"))[0], "--getsavefilename");
        assert_eq!(kdialog_args(false, Path::new("/x.json"))[0], "--getopenfilename");
    }

    #[test]
    fn dialog_output_is_one_path_or_cancelled() {
        assert_eq!(parse_output("/home/a/f.json\n"), Some(PathBuf::from("/home/a/f.json")));
        assert_eq!(parse_output("\n"), None);
        assert_eq!(parse_output(""), None);
    }
}
