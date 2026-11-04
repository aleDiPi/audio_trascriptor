//! Aggiornamenti automatici dalla release "latest" del repository privato (richiede il token in .env).
//! Il token e l'installer scaricato passano solo da api.github.com / dal download firmato da GitHub.

use crate::release::{self, Asset, Release};
#[cfg(windows)]
use crate::platform;
use crate::config;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const API: &str = "https://api.github.com/repos/aleDiPi/audio_trascriptor/releases/latest";

/// true mentre si registra o si elabora un audio: l'aggiornamento aspetta.
pub static BUSY: AtomicBool = AtomicBool::new(false);

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("trascrittore-audio")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

/// Esito (HTTP) di GET /repos/aleDiPi/audio_trascriptor con il token: dice se il token vede il repository.
fn repo_status(token: &str) -> Option<u16> {
    let url = API.split("/releases/").next()?;
    let resp = client().ok()?.get(url).bearer_auth(token).header("Accept", "application/vnd.github+json").send().ok()?;
    Some(resp.status().as_u16())
}

/// Legge la release "latest". Se GitHub risponde con un errore dice se dipende dal token o dalla release.
pub fn fetch_latest(token: &str) -> Result<Release, String> {
    let resp = client()?
        .get(API)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| crate::i18n::tf("err.network", &[("msg", &e.to_string())]))?;
    let status = resp.status();
    let body = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        let code = status.as_u16();
        let repo = if matches!(code, 403 | 404) { repo_status(token) } else { None };
        let msg = release::explain_github_failure(code, repo, &release::api_message(&body));
        config::log(&format!("update check: HTTP {code}, repository HTTP {repo:?}"));
        return Err(msg);
    }
    release::parse_release(&body)
}

/// Scarica un asset e ne verifica lo SHA-256 con quello dichiarato da GitHub.
fn download(token: &str, asset: &Asset) -> Result<Vec<u8>, String> {
    let resp = client()?
        .get(&asset.url)
        .bearer_auth(token) // reqwest toglie l'header sul redirect verso un altro host
        .header("Accept", "application/octet-stream")
        .send()
        .map_err(|e| crate::i18n::tf("err.network", &[("msg", &e.to_string())]))?;
    if !resp.status().is_success() {
        return Err(crate::i18n::tf("err.download_http", &[("status", &resp.status().to_string())]));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    if !release::digest_matches(asset.digest.as_deref(), &release::hex(&Sha256::digest(&bytes))) {
        return Err(crate::i18n::t("err.update_sha"));
    }
    Ok(bytes.to_vec())
}

#[cfg(windows)]
/// Aggiornamento in-place: scarica solo l'eseguibile, rinomina quello in uso (Windows lo permette),
/// mette il nuovo al suo posto e riavvia. Se qualcosa va storto il vecchio eseguibile resta al suo posto.
fn swap_in_place(token: &str, rel: &Release) -> Result<(), String> {
    let bytes = download(token, &rel.exe)?;
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    let (new, old) = (cur.with_extension("exe.new"), cur.with_extension("exe.old"));
    std::fs::write(&new, &bytes).map_err(|e| crate::i18n::tf("err.write_file", &[("path", &new.display().to_string()), ("err", &e.to_string())]))?;
    let _ = std::fs::remove_file(&old);
    if let Err(e) = std::fs::rename(&cur, &old) {
        let _ = std::fs::remove_file(&new);
        return Err(crate::i18n::tf("err.replace_exe", &[("err", &e.to_string())]));
    }
    if let Err(e) = std::fs::rename(&new, &cur) {
        let _ = std::fs::rename(&old, &cur); // ripristina
        return Err(crate::i18n::tf("err.install_exe", &[("err", &e.to_string())]));
    }
    std::process::Command::new(&cur)
        .arg("--after-update")
        .spawn()
        .map_err(|e| crate::i18n::tf("err.restart", &[("err", &e.to_string())]))?;
    std::process::exit(0);
}

#[cfg(windows)]
/// Aggiornamento con l'installer completo, in silenzio (modifiche strutturali o scambio in-place fallito).
fn run_setup(token: &str, rel: &Release) -> Result<(), String> {
    let bytes = download(token, &rel.setup)?;
    let dir = std::env::temp_dir().join("Trascrittore-Update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(release::SETUP_NAME);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    platform::run_installer_after_exit(&path)?;
    std::process::exit(0);
}

/// Argomenti per installare un .deb locale con le sue dipendenze (con pkexec: chiede la password in una finestra).
#[cfg(any(target_os = "linux", test))]
pub fn apt_install_args(deb: &std::path::Path) -> Vec<String> {
    vec!["apt-get".into(), "install".into(), "-y".into(), "--allow-downgrades".into(), deb.display().to_string()]
}

/// Aggiornamento su Linux: scarica il .deb della release, ne verifica lo SHA-256 e lo installa con apt (pkexec).
/// Poi riavvia il programma. Se l'utente annulla la richiesta della password non cambia nulla.
#[cfg(target_os = "linux")]
fn install_deb(token: &str, rel: &Release) -> Result<(), String> {
    let asset = rel.deb.as_ref().ok_or_else(|| crate::i18n::t("err.deb_missing"))?;
    let bytes = download(token, asset)?;
    let dir = std::env::temp_dir().join("Trascrittore-Update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(release::DEB_NAME);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("pkexec")
        .args(apt_install_args(&path))
        .status()
        .map_err(|e| crate::i18n::tf("err.pkexec", &[("err", &e.to_string())]))?;
    let _ = std::fs::remove_dir_all(&dir);
    if !status.success() {
        return Err(crate::i18n::t("err.install_cancelled"));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // la nuova copia parte dopo che questa si e' chiusa (istanza unica)
    std::process::Command::new("sh")
        .args(["-c", "sleep 1; exec \"$0\" --after-update", &exe.display().to_string()])
        .spawn()
        .map_err(|e| crate::i18n::tf("err.restart", &[("err", &e.to_string())]))?;
    std::process::exit(0);
}

/// Controlla subito; se c'e' una versione piu' recente la installa. Ok(false) = gia' aggiornato.
pub fn check_and_apply(token: &str) -> Result<bool, String> {
    let rel = fetch_latest(token)?;
    config::debug(|| format!("updates: online {}, installed {}", rel.version, release::current_version().map_or("local build".to_string(), |v| v.to_string())));
    if !release::needs_update(release::current_version(), rel.version) {
        return Ok(false);
    }
    // non interrompere una dettatura in corso
    for _ in 0..60 {
        if !BUSY.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_secs(10));
    }
    config::log(&format!("updating to version {}", rel.version));
    #[cfg(target_os = "linux")]
    {
        install_deb(token, &rel)?;
        Ok(true)
    }
    #[cfg(windows)]
    {
        let exe_dir_writable = std::env::current_exe().ok().and_then(|p| p.parent().map(release::dir_writable)).unwrap_or(false);
        if !rel.installer_required && !exe_dir_writable {
            config::debug(|| "program folder not writable (Program Files): using the installer".into());
        }
        if !rel.installer_required && exe_dir_writable {
            match swap_in_place(token, &rel) {
                Ok(()) => {}
                Err(e) => config::log(&format!("in-place update failed, using the installer: {e}")),
            }
        }
        run_setup(token, &rel)?;
        Ok(true)
    }
}

/// Notifica del desktop (notify-send) per l'aggiornamento disponibile su Linux.
#[cfg(target_os = "linux")]
fn notify_update(version: release::Version) {
    let _ = std::process::Command::new("notify-send")
        .args(["Trascrittore audio", &crate::i18n::tf("tray.update_notice", &[("version", &version.to_string())])])
        .spawn();
}

/// Thread in background: controlla poco dopo l'avvio e poi ogni 6 ore. Senza token non fa nulla.
/// Su Linux l'installazione richiede la password (pkexec): il controllo automatico si limita ad avvisare con una
/// notifica; si installa da Impostazioni > Info > "Aggiorna ora".
pub fn start_background() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(20));
        #[cfg(target_os = "linux")]
        let mut notified: Option<release::Version> = None;
        loop {
            if release::current_version().is_some() && config::auto_update_enabled() {
                if let Some(token) = config::update_token() {
                    #[cfg(windows)]
                    if let Err(e) = check_and_apply(&token) {
                        config::log(&format!("update check: {e}"));
                    }
                    #[cfg(target_os = "linux")]
                    match fetch_latest(&token) {
                        Ok(r) if release::needs_update(release::current_version(), r.version) && notified != Some(r.version) => {
                            notified = Some(r.version);
                            notify_update(r.version);
                        }
                        Ok(_) => {}
                        Err(e) => config::log(&format!("update check: {e}")),
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(6 * 3600));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apt_installs_the_local_deb_with_its_dependencies() {
        let args = apt_install_args(std::path::Path::new("/tmp/Trascrittore-Update/trascrittore.deb"));
        assert_eq!(args[..2], ["apt-get", "install"]);
        assert_eq!(args.last().unwrap(), "/tmp/Trascrittore-Update/trascrittore.deb");
    }
}
