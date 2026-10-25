//! Parte pura degli aggiornamenti automatici: lettura della release GitHub e confronto versioni.

use serde_json::Value;
use std::fmt;

/// Prefisso obbligatorio dell'URL dell'asset: il token non viene mai inviato altrove.
pub const ASSET_PREFIX: &str = "https://api.github.com/repos/aleDiPi/audio_trascriptor/releases/assets/";
pub const SETUP_NAME: &str = "Trascrittore-Setup.exe";
pub const EXE_NAME: &str = "trascrittore.exe";
/// Se compare nel testo della release, questo aggiornamento richiede di rieseguire l'installer.
pub const INSTALLER_MARKER: &str = "[installer]";

#[derive(Debug, PartialEq)]
pub struct Asset {
    pub url: String,
    /// "sha256:<hex>" come riportato da GitHub
    pub digest: Option<String>,
}

/// Versione AAAA.MM.GG.N: data della build e numero progressivo del giorno. Si confronta numericamente
/// (2026.10.25.10 viene dopo 2026.10.25.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32, pub u32);

impl Version {
    /// Accetta "2026.10.25.3" e "v2026.10.25.3" (tag di GitHub): esattamente quattro numeri.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
        let mut it = s.split('.').map(|p| if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) { p.parse::<u32>().ok() } else { None });
        let v = Version(it.next()??, it.next()??, it.next()??, it.next()??);
        if it.next().is_some() {
            return None;
        }
        Some(v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}.{:02}.{}", self.0, self.1, self.2, self.3)
    }
}

#[derive(Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    pub commit: String,
    pub installer_required: bool,
    /// solo l'eseguibile: aggiornamento in-place, scarica pochi MB
    pub exe: Asset,
    /// installer completo: usato per le modifiche strutturali o se lo scambio in-place fallisce
    pub setup: Asset,
}

/// Commit da cui e' stato compilato questo eseguibile (impostato dalla GitHub Action).
pub fn current_commit() -> Option<&'static str> {
    option_env!("GITHUB_SHA").filter(|s| s.len() >= 7)
}

/// Versione stampata nell'eseguibile dalla GitHub Action (variabile TRASCRITTORE_VERSION). None = build locale.
pub fn current_version() -> Option<Version> {
    option_env!("TRASCRITTORE_VERSION").and_then(Version::parse)
}

pub fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn find_asset(v: &Value, name: &str) -> Result<Asset, String> {
    let a = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(name)))
        .ok_or_else(|| format!("{name} non presente nella release"))?;
    let url = a["url"].as_str().ok_or("asset senza url")?.to_string();
    if !url.starts_with(ASSET_PREFIX) {
        return Err("url dell'asset non atteso".into());
    }
    Ok(Asset { url, digest: a["digest"].as_str().map(String::from) })
}

pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let commit = v["target_commitish"].as_str().ok_or("release senza commit")?.to_string();
    let tag = v["tag_name"].as_str().unwrap_or("");
    let version = Version::parse(tag).ok_or_else(|| format!("la release \"{tag}\" non ha una versione AAAA.MM.GG.N"))?;
    Ok(Release {
        version,
        commit,
        installer_required: v["body"].as_str().map_or(false, |b| b.contains(INSTALLER_MARKER)),
        exe: find_asset(&v, EXE_NAME)?,
        setup: find_asset(&v, SETUP_NAME)?,
    })
}

/// Messaggio d'errore per una risposta non riuscita di GitHub. Su un repository privato GitHub risponde 404
/// anche quando il token non ha accesso: per distinguere i casi si guarda se il token vede il repository
/// (`repo_status` = esito di GET /repos/aleDiPi/audio_trascriptor con lo stesso token, se e' stato provato).
pub fn explain_github_failure(status: u16, repo_status: Option<u16>, api_message: &str) -> String {
    let detail = if api_message.is_empty() { String::new() } else { format!(" (GitHub: \"{api_message}\")") };
    let no_access = "Il token non vede il repository aleDiPi/audio_trascriptor: nella pagina di creazione controlla \
        Resource owner = aleDiPi, Repository access = solo audio_trascriptor e Permissions > Contents = Read-only.";
    match (status, repo_status) {
        (401, _) | (404 | 403, Some(401)) => format!("GitHub non accetta il token (errato, revocato o scaduto){detail}."),
        (404, Some(200)) => format!(
            "Il token e' valido e vede il repository, ma non c'e' ancora nessuna release pubblicata \
             (la pubblicazione richiede un paio di minuti dopo ogni modifica): riprova piu' tardi{detail}."
        ),
        (404 | 403, Some(404 | 403)) => format!("{no_access}{detail}"),
        (403, _) => format!("GitHub nega l'accesso (permessi insufficienti o troppe richieste){detail}."),
        (404, None) => format!("GitHub HTTP 404: release non trovata oppure token senza accesso al repository{detail}."),
        (s, _) => format!("GitHub HTTP {s}{detail}."),
    }
}

/// Testo del campo "message" di una risposta d'errore di GitHub ("" se assente).
pub fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(|m| m.chars().take(120).collect()))
        .unwrap_or_default()
}

/// Aggiorna solo se la versione online e' piu' recente. Una build locale (senza versione) non si aggiorna mai.
pub fn needs_update(current: Option<Version>, remote: Version) -> bool {
    current.map_or(false, |c| remote > c)
}

/// L'installer scaricato va eseguito solo se il suo SHA-256 coincide con quello dichiarato da GitHub.
/// Vero se si puo' creare e cancellare un file nella cartella (es. non in Program Files senza amministratore).
pub fn dir_writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(format!(".scrittura-{}.tmp", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn digest_matches(expected: Option<&str>, actual_hex: &str) -> bool {
    expected
        .and_then(|d| d.strip_prefix("sha256:"))
        .map_or(false, |d| d.eq_ignore_ascii_case(actual_hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(url: &str, body: &str) -> String {
        format!(
            r#"{{"tag_name":"v2026.10.25.3","target_commitish":"abc1234def","body":"{body}","assets":[
              {{"name":"altro.txt","url":"https://x/1"}},
              {{"name":"trascrittore.exe","url":"{url}1","digest":"sha256:AABB"}},
              {{"name":"Trascrittore-Setup.exe","url":"{url}2","digest":"sha256:CCDD"}}]}}"#
        )
    }

    #[test]
    fn parses_release() {
        let r = parse_release(&json(ASSET_PREFIX, "Build automatica")).unwrap();
        assert_eq!(r.commit, "abc1234def");
        assert_eq!(r.version, Version(2026, 10, 25, 3));
        assert!(!r.installer_required);
        assert_eq!(r.exe.digest.as_deref(), Some("sha256:AABB"));
        assert!(r.exe.url.ends_with("/1") && r.setup.url.ends_with("/2"));
    }

    #[test]
    fn installer_marker_in_body() {
        assert!(parse_release(&json(ASSET_PREFIX, "Build [installer]")).unwrap().installer_required);
    }

    #[test]
    fn refuses_foreign_asset_url() {
        assert!(parse_release(&json("https://evil.example/", "x")).is_err());
    }

    #[test]
    fn refuses_missing_asset_or_commit() {
        assert!(parse_release(r#"{"target_commitish":"a","assets":[]}"#).is_err());
        assert!(parse_release(r#"{"assets":[]}"#).is_err());
        assert!(parse_release("non json").is_err());
    }

    #[test]
    fn update_decision() {
        let v = |s| Version::parse(s).unwrap();
        assert!(needs_update(Some(v("2026.10.25.1")), v("2026.10.25.2")));
        assert!(needs_update(Some(v("2026.10.25.9")), v("2026.10.25.10"))); // numerico, non alfabetico
        assert!(needs_update(Some(v("2026.10.31.7")), v("2026.11.01.1")));
        assert!(!needs_update(Some(v("2026.10.25.2")), v("2026.10.25.2"))); // gia' aggiornato
        assert!(!needs_update(Some(v("2026.10.25.5")), v("2026.10.25.2"))); // mai tornare indietro
        assert!(!needs_update(None, v("2026.10.25.2"))); // build locale
    }

    #[test]
    fn version_parse_and_display() {
        assert_eq!(Version::parse("2026.10.25.3"), Some(Version(2026, 10, 25, 3)));
        assert_eq!(Version::parse("v2026.10.05.12"), Some(Version(2026, 10, 5, 12)));
        assert_eq!(Version::parse(" v2026.1.2.3 "), Some(Version(2026, 1, 2, 3)));
        assert_eq!(Version(2026, 10, 5, 12).to_string(), "2026.10.05.12");
        assert_eq!(Version::parse(&Version(2026, 1, 2, 3).to_string()), Some(Version(2026, 1, 2, 3)));
        // servono esattamente quattro numeri
        for bad in ["", "latest", "2026.10.25", "2026.10.25.3.1", "2026.10.x.3", "2026..25.3", "2026.10.25.-1", "0.1.0"] {
            assert!(Version::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn refuses_release_without_a_version_tag() {
        let j = json(ASSET_PREFIX, "x").replace("v2026.10.25.3", "latest");
        assert!(parse_release(&j).is_err());
    }

    #[test]
    fn digest_check_fails_closed() {
        assert!(digest_matches(Some("sha256:aabb"), "AABB"));
        assert!(!digest_matches(Some("sha256:aabb"), "ccdd"));
        assert!(!digest_matches(None, "aabb"));
        assert!(!digest_matches(Some("md5:aabb"), "aabb"));
    }

    #[test]
    fn short_commit() {
        assert_eq!(short("abcdef1234"), "abcdef1");
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(hex(&[0x00, 0x0a, 0xff]), "000aff");
    }

    #[test]
    fn writable_dir_detection() {
        assert!(dir_writable(&std::env::temp_dir()));
        assert!(!dir_writable(&std::env::temp_dir().join("trascrittore-cartella-inesistente-xyz")));
    }

    #[test]
    fn explains_404_depending_on_repo_access() {
        // il token vede il repository: la release manca (e' in ricostruzione)
        let m = explain_github_failure(404, Some(200), "Not Found");
        assert!(m.contains("valido") && m.contains("riprova"), "{m}");
        // il token non vede il repository: problema di permessi, non di scadenza
        let m = explain_github_failure(404, Some(404), "Not Found");
        assert!(m.contains("Resource owner") && m.contains("Contents") && !m.contains("scaduto"), "{m}");
        // token rifiutato
        assert!(explain_github_failure(401, None, "Bad credentials").contains("scaduto"));
        assert!(explain_github_failure(404, Some(401), "").contains("scaduto"));
        // sonda non eseguita
        assert!(explain_github_failure(404, None, "").contains("404"));
        assert!(explain_github_failure(500, None, "").contains("500"));
    }

    #[test]
    fn extracts_github_message() {
        assert_eq!(api_message(r#"{"message":"Not Found","status":"404"}"#), "Not Found");
        assert_eq!(api_message("non json"), "");
    }
}
