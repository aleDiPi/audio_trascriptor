//! Aggiornamenti automatici dalla release "latest" del repository privato (richiede il token in .env).
//! Il token e l'installer scaricato passano solo da api.github.com / dal download firmato da GitHub.

use crate::release::{self, Asset, Release};
use crate::{config, platform};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const API: &str = "https://api.github.com/repos/aleDiPi/audio_trascriptor/releases/tags/latest";

/// true mentre si registra o si elabora un audio: l'aggiornamento aspetta.
pub static BUSY: AtomicBool = AtomicBool::new(false);

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("trascrittore-audio")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

/// Legge la release "latest". Un errore 401/403/404 di solito significa token errato o senza permessi.
pub fn fetch_latest(token: &str) -> Result<Release, String> {
    let resp = client()?
        .get(API)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("rete: {e}"))?;
    let status = resp.status();
    let body = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("GitHub HTTP {status} (token errato, scaduto o senza accesso al repository?)"));
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
        .map_err(|e| format!("rete: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download: HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    if !release::digest_matches(asset.digest.as_deref(), &release::hex(&Sha256::digest(&bytes))) {
        return Err("file scaricato non valido (SHA-256 diverso): aggiornamento annullato".into());
    }
    Ok(bytes.to_vec())
}

/// Aggiornamento in-place: scarica solo l'eseguibile, rinomina quello in uso (Windows lo permette),
/// mette il nuovo al suo posto e riavvia. Se qualcosa va storto il vecchio eseguibile resta al suo posto.
fn swap_in_place(token: &str, rel: &Release) -> Result<(), String> {
    let bytes = download(token, &rel.exe)?;
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    let (new, old) = (cur.with_extension("exe.new"), cur.with_extension("exe.old"));
    std::fs::write(&new, &bytes).map_err(|e| format!("scrittura {}: {e}", new.display()))?;
    let _ = std::fs::remove_file(&old);
    if let Err(e) = std::fs::rename(&cur, &old) {
        let _ = std::fs::remove_file(&new);
        return Err(format!("impossibile sostituire l'eseguibile: {e}"));
    }
    if let Err(e) = std::fs::rename(&new, &cur) {
        let _ = std::fs::rename(&old, &cur); // ripristina
        return Err(format!("impossibile installare il nuovo eseguibile: {e}"));
    }
    std::process::Command::new(&cur)
        .arg("--after-update")
        .spawn()
        .map_err(|e| format!("riavvio: {e}"))?;
    std::process::exit(0);
}

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

/// Controlla subito; se c'e' una versione piu' recente la installa. Ok(false) = gia' aggiornato.
pub fn check_and_apply(token: &str) -> Result<bool, String> {
    let rel = fetch_latest(token)?;
    config::debug(|| format!("aggiornamenti: online {}, installato {}", release::short(&rel.commit), release::current_commit().map_or("build locale", release::short)));
    if !release::needs_update(release::current_commit(), &rel.commit) {
        return Ok(false);
    }
    // non interrompere una dettatura in corso
    for _ in 0..60 {
        if !BUSY.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_secs(10));
    }
    config::log(&format!("aggiornamento alla versione {}", release::short(&rel.commit)));
    let exe_dir_writable = std::env::current_exe().ok().and_then(|p| p.parent().map(release::dir_writable)).unwrap_or(false);
    if !rel.installer_required && !exe_dir_writable {
        config::debug(|| "cartella del programma non scrivibile (Program Files): uso l'installer".into());
    }
    if !rel.installer_required && exe_dir_writable {
        match swap_in_place(token, &rel) {
            Ok(()) => {}
            Err(e) => config::log(&format!("aggiornamento in-place non riuscito, uso l'installer: {e}")),
        }
    }
    run_setup(token, &rel)?;
    Ok(true)
}

/// Thread in background: controlla poco dopo l'avvio e poi ogni 6 ore. Senza token non fa nulla.
pub fn start_background() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(20));
        loop {
            if release::current_commit().is_some() {
                if let Some(token) = config::update_token() {
                    if let Err(e) = check_and_apply(&token) {
                        config::log(&format!("controllo aggiornamenti: {e}"));
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(6 * 3600));
        }
    });
}
