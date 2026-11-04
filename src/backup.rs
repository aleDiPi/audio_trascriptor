//! Export and import of the settings (config.toml + the AI service keys of .env) as one file,
//! optionally encrypted with a password.
//!
//! File format: a small JSON document. In clear: `{"format","version","encrypted":false,"payload":{..}}`.
//! Encrypted: the payload is serialized, then sealed with XChaCha20-Poly1305 using a key derived from the
//! password with Argon2id (random salt and nonce per file). The header (format, version, KDF parameters, salt,
//! nonce) is authenticated as associated data, so it cannot be altered without detection.
//! The GitHub update token is never exported nor imported.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{aead::{Aead, KeyInit, Payload as AeadPayload}, XChaCha20Poly1305};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const FORMAT: &str = "trascrittore-settings";
const VERSION: u32 = 1;
/// Variables of .env that travel in the backup (the update token does not).
pub const KEY_NAMES: [&str; 3] = ["GROQ_API_KEY", "OPENAI_API_KEY", "ANTHROPIC_API_KEY"];
// Argon2id costs used when writing: 64 MiB, 3 passes, 1 lane (OWASP-class parameters).
const M_COST: u32 = 64 * 1024;
const T_COST: u32 = 3;
const P_COST: u32 = 1;
// Upper bounds accepted when reading, so a hostile file cannot ask for gigabytes or minutes of work.
const MAX_M_COST: u32 = 256 * 1024;
const MAX_T_COST: u32 = 10;
const MAX_P_COST: u32 = 8;
/// A backup is a few KB; refuse anything absurd before parsing it.
const MAX_FILE: u64 = 4 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum BackupError {
    /// The file is encrypted and no password was given.
    NeedsPassword,
    /// Wrong password, or the file was modified.
    WrongPassword,
    /// Not a backup of this program, or damaged.
    Invalid,
    /// Backup written by a newer version of the program.
    Unsupported,
    /// The destination already exists (export never overwrites).
    Exists,
    Io(String),
}

/// What goes into the file: each part can be left out.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub struct Selection {
    /// Preferences: every config.toml setting except the prompts.
    pub prefs: bool,
    /// Prompts: default prompt, dictionary and per-application rules.
    pub prompts: bool,
    /// Keys of the AI services (.env).
    pub keys: bool,
}

impl Selection {
    pub const ALL: Selection = Selection { prefs: true, prompts: true, keys: true };
    pub fn is_empty(&self) -> bool {
        !(self.prefs || self.prompts || self.keys)
    }
}

/// Keys of config.toml that belong to "Prompts"; all the others are "Preferences".
pub const PROMPT_KEYS: [&str; 3] = ["default_prompt", "dictionary", "apps"];

fn is_prompt_key(k: &str) -> bool {
    PROMPT_KEYS.contains(&k)
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Payload {
    /// Which parts the file contains.
    pub scope: Selection,
    /// config.toml: the whole text when preferences and prompts are both present, otherwise only the selected
    /// settings as TOML. Empty if neither is present.
    pub config: String,
    /// Service keys (only the names in `KEY_NAMES`); empty if keys are not part of the file.
    pub keys: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct Header {
    format: String,
    version: u32,
    encrypted: bool,
}

#[derive(Serialize, Deserialize)]
struct Plain {
    #[serde(flatten)]
    head: Header,
    payload: Payload,
}

#[derive(Serialize, Deserialize, Clone)]
struct Kdf {
    algorithm: String,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    salt: String,
}

#[derive(Serialize, Deserialize)]
struct Sealed {
    #[serde(flatten)]
    head: Header,
    kdf: Kdf,
    cipher: String,
    nonce: String,
    data: String,
}

/// Reads the keys from the text of .env (only the ones that travel, only non-empty values).
pub fn keys_from_env(env: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in env.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"').trim_matches('\'').trim();
            if KEY_NAMES.contains(&k) && valid_value(v) {
                out.insert(k.to_string(), v.to_string());
            }
        }
    }
    out
}

fn valid_value(v: &str) -> bool {
    !v.is_empty() && !v.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn random<const N: usize>() -> Result<[u8; N], BackupError> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| BackupError::Io(e.to_string()))?;
    Ok(b)
}

fn derive(password: &str, kdf: &Kdf, salt: &[u8]) -> Result<[u8; 32], BackupError> {
    if kdf.algorithm != "argon2id" || kdf.m_cost > MAX_M_COST || kdf.t_cost > MAX_T_COST || kdf.p_cost > MAX_P_COST {
        return Err(BackupError::Invalid);
    }
    let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(32)).map_err(|_| BackupError::Invalid)?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| BackupError::Invalid)?;
    Ok(key)
}

/// Associated data: the header fields that decide how the file is read.
fn aad(head: &Header, kdf: &Kdf, nonce: &str) -> Vec<u8> {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        head.format, head.version, head.encrypted, kdf.algorithm, kdf.m_cost, kdf.t_cost, kdf.p_cost, kdf.salt, nonce
    )
    .into_bytes()
}

fn seal_with(payload: &Payload, password: &str, kdf_costs: (u32, u32, u32)) -> Result<String, BackupError> {
    let plain = serde_json::to_vec(payload).map_err(|e| BackupError::Io(e.to_string()))?;
    let salt = random::<16>()?;
    let nonce_bytes = random::<24>()?;
    let kdf = Kdf { algorithm: "argon2id".into(), m_cost: kdf_costs.0, t_cost: kdf_costs.1, p_cost: kdf_costs.2, salt: B64.encode(salt) };
    let head = Header { format: FORMAT.into(), version: VERSION, encrypted: true };
    let nonce = B64.encode(nonce_bytes);
    let key = derive(password, &kdf, &salt)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let data = cipher
        .encrypt((&nonce_bytes).into(), AeadPayload { msg: &plain, aad: &aad(&head, &kdf, &nonce) })
        .map_err(|_| BackupError::Invalid)?;
    let sealed = Sealed { head, kdf, cipher: "xchacha20poly1305".into(), nonce, data: B64.encode(data) };
    serde_json::to_string_pretty(&sealed).map_err(|e| BackupError::Io(e.to_string()))
}

/// Serializes the backup; with a non-empty password the content is encrypted.
pub fn seal(payload: &Payload, password: Option<&str>) -> Result<String, BackupError> {
    match password.filter(|p| !p.is_empty()) {
        Some(p) => seal_with(payload, p, (M_COST, T_COST, P_COST)),
        None => serde_json::to_string_pretty(&Plain {
            head: Header { format: FORMAT.into(), version: VERSION, encrypted: false },
            payload: payload.clone(),
        })
        .map_err(|e| BackupError::Io(e.to_string())),
    }
}

/// Is this backup encrypted? (so the UI can ask for the password). `None` if it is not a backup.
pub fn is_encrypted(text: &str) -> Option<bool> {
    let head: Header = serde_json::from_str(text).ok()?;
    (head.format == FORMAT).then_some(head.encrypted)
}

/// Reads a backup. The returned payload is already sanitized (see `sanitize`).
pub fn open(text: &str, password: Option<&str>) -> Result<Payload, BackupError> {
    let head: Header = serde_json::from_str(text).map_err(|_| BackupError::Invalid)?;
    if head.format != FORMAT {
        return Err(BackupError::Invalid);
    }
    if head.version > VERSION {
        return Err(BackupError::Unsupported);
    }
    let payload = if head.encrypted {
        let s: Sealed = serde_json::from_str(text).map_err(|_| BackupError::Invalid)?;
        let pw = password.filter(|p| !p.is_empty()).ok_or(BackupError::NeedsPassword)?;
        if s.cipher != "xchacha20poly1305" {
            return Err(BackupError::Invalid);
        }
        let salt = B64.decode(&s.kdf.salt).map_err(|_| BackupError::Invalid)?;
        let nonce: [u8; 24] = B64.decode(&s.nonce).map_err(|_| BackupError::Invalid)?.try_into().map_err(|_| BackupError::Invalid)?;
        let data = B64.decode(&s.data).map_err(|_| BackupError::Invalid)?;
        let key = derive(pw, &s.kdf, &salt)?;
        let plain = XChaCha20Poly1305::new((&key).into())
            .decrypt((&nonce).into(), AeadPayload { msg: &data, aad: &aad(&s.head, &s.kdf, &s.nonce) })
            .map_err(|_| BackupError::WrongPassword)?;
        serde_json::from_slice::<Payload>(&plain).map_err(|_| BackupError::Invalid)?
    } else {
        serde_json::from_str::<Plain>(text).map_err(|_| BackupError::Invalid)?.payload
    };
    sanitize(payload)
}

/// Keeps only what the program accepts: valid TOML for config.toml, known key names with plausible values.
pub fn sanitize(mut p: Payload) -> Result<Payload, BackupError> {
    let table = p.config.parse::<toml::Table>().map_err(|_| BackupError::Invalid)?;
    // a part not declared in the scope cannot sneak in
    for k in table.keys() {
        let is_prompt = is_prompt_key(k);
        if (is_prompt && !p.scope.prompts) || (!is_prompt && !p.scope.prefs) {
            return Err(BackupError::Invalid);
        }
    }
    if !p.scope.keys {
        p.keys.clear();
    }
    p.keys.retain(|k, v| KEY_NAMES.contains(&k.as_str()) && valid_value(v));
    Ok(p)
}

/// Writes `text` through a temporary file, so a crash never leaves a half-written file.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Picks the selected parts out of config.toml and .env.
pub fn build_payload(config_text: &str, env_text: &str, sel: Selection) -> Result<Payload, BackupError> {
    if sel.is_empty() {
        return Err(BackupError::Invalid);
    }
    let config = if sel.prefs && sel.prompts {
        config_text.to_string()
    } else if sel.prefs || sel.prompts {
        let mut table: toml::Table = config_text.parse().map_err(|_| BackupError::Invalid)?;
        table.retain(|k, _| is_prompt_key(k) == sel.prompts);
        toml::to_string(&table).map_err(|e| BackupError::Io(e.to_string()))?
    } else {
        String::new()
    };
    let keys = if sel.keys { keys_from_env(env_text) } else { BTreeMap::new() };
    Ok(Payload { scope: sel, config, keys })
}

/// Creates the backup file. Never overwrites an existing file.
pub fn export_to(dir: &Path, dest: &Path, password: Option<&str>, sel: Selection) -> Result<(), BackupError> {
    if dest.exists() {
        return Err(BackupError::Exists);
    }
    let config = std::fs::read_to_string(dir.join("config.toml")).map_err(|e| BackupError::Io(e.to_string()))?;
    let env = std::fs::read_to_string(dir.join(".env")).unwrap_or_default();
    let text = seal(&build_payload(&config, &env, sel)?, password)?;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(dest).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists { BackupError::Exists } else { BackupError::Io(e.to_string()) }
    })?;
    use std::io::Write;
    f.write_all(text.as_bytes()).map_err(|e| BackupError::Io(e.to_string()))?;
    drop(f);
    #[cfg(unix)]
    crate::config::restrict_permissions(dest);
    Ok(())
}

/// Reads a backup file (size-limited).
pub fn read_file(src: &Path) -> Result<String, BackupError> {
    let meta = std::fs::metadata(src).map_err(|e| BackupError::Io(e.to_string()))?;
    if meta.len() > MAX_FILE {
        return Err(BackupError::Invalid);
    }
    std::fs::read_to_string(src).map_err(|_| BackupError::Invalid)
}

/// The new text of config.toml: replaced as a whole when the file holds preferences and prompts,
/// otherwise the imported settings replace those of the same part and the rest stays.
pub fn merge_config(old: &str, payload: &Payload) -> Result<String, BackupError> {
    if payload.scope.prefs && payload.scope.prompts {
        return Ok(payload.config.clone());
    }
    let mut table: toml::Table = old.parse().unwrap_or_default();
    let new: toml::Table = payload.config.parse().map_err(|_| BackupError::Invalid)?;
    table.retain(|k, _| is_prompt_key(k) != payload.scope.prompts);
    for (k, v) in new {
        table.insert(k, v);
    }
    toml::to_string(&table).map_err(|e| BackupError::Io(e.to_string()))
}

/// Applies a backup to the working folder: the old config.toml is kept as config.toml.bak,
/// the keys replace those of the same name in .env, everything else in .env stays as it is.
pub fn apply(dir: &Path, payload: &Payload) -> Result<(), BackupError> {
    let io = |e: std::io::Error| BackupError::Io(e.to_string());
    let cfg_path = dir.join("config.toml");
    let old = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    if payload.scope.prefs || payload.scope.prompts {
        if !old.is_empty() {
            std::fs::write(dir.join("config.toml.bak"), &old).map_err(io)?;
        }
        write_atomic(&cfg_path, &merge_config(&old, payload)?).map_err(io)?;
    }
    if !payload.keys.is_empty() {
        let env_path = dir.join(".env");
        let mut env = std::fs::read_to_string(&env_path).unwrap_or_default();
        for (k, v) in &payload.keys {
            env = crate::config::set_env_value(&env, k, v);
        }
        write_atomic(&env_path, &env).map_err(io)?;
        #[cfg(unix)]
        crate::config::restrict_permissions(&env_path);
    }
    Ok(())
}

/// Suggested file name for the export.
pub const DEFAULT_NAME: &str = "trascrittore-settings.json";

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Payload {
        let mut keys = BTreeMap::new();
        keys.insert("GROQ_API_KEY".to_string(), "gsk_secret123".to_string());
        Payload { scope: Selection::ALL, config: "hotkey = \"ctrl+win\"\nui_language = \"it\"\n".into(), keys }
    }

    // fast KDF for tests (the real costs take about a second)
    fn seal_fast(p: &Payload, pw: &str) -> String {
        seal_with(p, pw, (8, 1, 1)).unwrap()
    }

    #[test]
    fn plain_roundtrip() {
        let text = seal(&sample(), None).unwrap();
        assert_eq!(is_encrypted(&text), Some(false));
        assert_eq!(open(&text, None).unwrap(), sample());
    }

    #[test]
    fn empty_password_means_no_encryption() {
        let text = seal(&sample(), Some("")).unwrap();
        assert_eq!(is_encrypted(&text), Some(false));
    }

    #[test]
    fn encrypted_roundtrip_hides_the_keys() {
        let text = seal_fast(&sample(), "correct horse");
        assert_eq!(is_encrypted(&text), Some(true));
        assert!(!text.contains("gsk_secret123"));
        assert!(!text.contains("ctrl+win"));
        assert_eq!(open(&text, Some("correct horse")).unwrap(), sample());
    }

    #[test]
    fn real_costs_roundtrip() {
        let text = seal(&sample(), Some("pässword ✓")).unwrap();
        assert_eq!(open(&text, Some("pässword ✓")).unwrap(), sample());
    }

    #[test]
    fn wrong_or_missing_password_is_reported() {
        let text = seal_fast(&sample(), "abc");
        assert_eq!(open(&text, Some("abd")), Err(BackupError::WrongPassword));
        assert_eq!(open(&text, None), Err(BackupError::NeedsPassword));
        assert_eq!(open(&text, Some("")), Err(BackupError::NeedsPassword));
    }

    #[test]
    fn two_files_of_the_same_data_differ() {
        assert_ne!(seal_fast(&sample(), "abc"), seal_fast(&sample(), "abc"));
    }

    #[test]
    fn tampering_is_detected() {
        let text = seal_fast(&sample(), "abc");
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        // altered ciphertext
        let mut data = B64.decode(v["data"].as_str().unwrap()).unwrap();
        data[0] ^= 1;
        v["data"] = B64.encode(data).into();
        assert_eq!(open(&v.to_string(), Some("abc")), Err(BackupError::WrongPassword));
        // altered header (the KDF cost is authenticated, so lowering it is detected)
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["kdf"]["t_cost"] = 2.into();
        assert!(open(&v.to_string(), Some("abc")).is_err());
    }

    #[test]
    fn hostile_kdf_parameters_are_refused() {
        let text = seal_fast(&sample(), "abc");
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["kdf"]["m_cost"] = 4_000_000u32.into();
        assert_eq!(open(&v.to_string(), Some("abc")), Err(BackupError::Invalid));
    }

    #[test]
    fn garbage_and_foreign_files_are_invalid() {
        assert_eq!(open("hello", None), Err(BackupError::Invalid));
        assert_eq!(open("{\"format\":\"other\",\"version\":1,\"encrypted\":false}", None), Err(BackupError::Invalid));
        assert_eq!(is_encrypted("hello"), None);
    }

    #[test]
    fn newer_version_is_unsupported() {
        let text = seal(&sample(), None).unwrap().replace("\"version\": 1", "\"version\": 99");
        assert_eq!(open(&text, None), Err(BackupError::Unsupported));
    }

    #[test]
    fn invalid_config_toml_is_refused_and_unknown_keys_dropped() {
        let mut p = sample();
        p.config = "this is = not [ toml".into();
        assert_eq!(open(&seal(&p, None).unwrap(), None), Err(BackupError::Invalid));
        let mut p = sample();
        p.keys.insert("TRASCRITTORE_UPDATE_TOKEN".into(), "github_pat_x".into());
        p.keys.insert("PATH".into(), "/evil".into());
        p.keys.insert("OPENAI_API_KEY".into(), "bad value".into());
        let got = open(&seal(&p, None).unwrap(), None).unwrap();
        assert_eq!(got.keys.keys().collect::<Vec<_>>(), vec!["GROQ_API_KEY"]);
    }

    #[test]
    fn keys_from_env_takes_only_service_keys() {
        let env = "# c\nGROQ_API_KEY=gsk_1\nOPENAI_API_KEY=\nANTHROPIC_API_KEY=\"sk-ant\"\nTRASCRITTORE_UPDATE_TOKEN=github_pat_z\n";
        let k = keys_from_env(env);
        assert_eq!(k.len(), 2);
        assert_eq!(k["GROQ_API_KEY"], "gsk_1");
        assert_eq!(k["ANTHROPIC_API_KEY"], "sk-ant");
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("trascrittore-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn export_then_import_moves_settings_between_folders() {
        let a = tmp("a");
        let b = tmp("b");
        std::fs::write(a.join("config.toml"), "hotkey = \"f9\"\n").unwrap();
        std::fs::write(a.join(".env"), "GROQ_API_KEY=gsk_a\nTRASCRITTORE_UPDATE_TOKEN=github_pat_a\n").unwrap();
        std::fs::write(b.join("config.toml"), "hotkey = \"old\"\n").unwrap();
        std::fs::write(b.join(".env"), "# mine\nOPENAI_API_KEY=sk-b\nTRASCRITTORE_UPDATE_TOKEN=github_pat_b\n").unwrap();
        let file = a.join(DEFAULT_NAME);
        export_to(&a, &file, Some("pw"), Selection::ALL).unwrap();
        // never overwrites
        assert_eq!(export_to(&a, &file, None, Selection::ALL), Err(BackupError::Exists));
        let text = read_file(&file).unwrap();
        assert!(!text.contains("github_pat_a"));
        let payload = open(&text, Some("pw")).unwrap();
        apply(&b, &payload).unwrap();
        assert_eq!(std::fs::read_to_string(b.join("config.toml")).unwrap(), "hotkey = \"f9\"\n");
        assert_eq!(std::fs::read_to_string(b.join("config.toml.bak")).unwrap(), "hotkey = \"old\"\n");
        let env = std::fs::read_to_string(b.join(".env")).unwrap();
        assert!(env.contains("GROQ_API_KEY=gsk_a"));
        assert!(env.contains("OPENAI_API_KEY=sk-b"), "keys not in the backup stay");
        assert!(env.contains("github_pat_b"), "the local update token stays and is not replaced");
        assert!(!env.contains("github_pat_a"));
    }

    #[test]
    fn oversized_file_is_refused() {
        let d = tmp("big");
        let f = d.join("x.json");
        std::fs::write(&f, vec![b' '; (MAX_FILE + 1) as usize]).unwrap();
        assert_eq!(read_file(&f), Err(BackupError::Invalid));
    }

    const CFG: &str = "hotkey = \"f9\"\nsound_volume = 7\ndefault_prompt = \"P\"\ndictionary = [\"x\"]\n\n[[apps]]\nname = \"A\"\n";
    const ENV: &str = "GROQ_API_KEY=gsk_1\nANTHROPIC_API_KEY=sk-a\n";

    #[test]
    fn selection_picks_only_the_chosen_parts() {
        let prefs = build_payload(CFG, ENV, Selection { prefs: true, prompts: false, keys: false }).unwrap();
        assert!(prefs.config.contains("hotkey") && !prefs.config.contains("default_prompt") && !prefs.config.contains("apps"));
        assert!(prefs.keys.is_empty());
        let prompts = build_payload(CFG, ENV, Selection { prefs: false, prompts: true, keys: false }).unwrap();
        assert!(!prompts.config.contains("hotkey") && prompts.config.contains("default_prompt") && prompts.config.contains("[[apps]]"));
        let keys = build_payload(CFG, ENV, Selection { prefs: false, prompts: false, keys: true }).unwrap();
        assert!(keys.config.is_empty());
        assert_eq!(keys.keys.len(), 2);
        let all = build_payload(CFG, ENV, Selection::ALL).unwrap();
        assert_eq!(all.config, CFG, "everything selected keeps config.toml verbatim");
        assert_eq!(build_payload(CFG, ENV, Selection { prefs: false, prompts: false, keys: false }), Err(BackupError::Invalid));
    }

    #[test]
    fn partial_import_keeps_the_rest_of_the_config() {
        let old = "hotkey = \"old\"\nsound_volume = 90\ndefault_prompt = \"mine\"\n";
        // only prompts imported: preferences stay
        let p = build_payload(CFG, ENV, Selection { prefs: false, prompts: true, keys: false }).unwrap();
        let merged: toml::Table = merge_config(old, &p).unwrap().parse().unwrap();
        assert_eq!(merged["hotkey"].as_str(), Some("old"));
        assert_eq!(merged["default_prompt"].as_str(), Some("P"));
        assert!(merged.contains_key("apps"));
        // only preferences imported: prompts stay
        let p = build_payload(CFG, ENV, Selection { prefs: true, prompts: false, keys: false }).unwrap();
        let merged: toml::Table = merge_config(old, &p).unwrap().parse().unwrap();
        assert_eq!(merged["hotkey"].as_str(), Some("f9"));
        assert_eq!(merged["sound_volume"].as_integer(), Some(7));
        assert_eq!(merged["default_prompt"].as_str(), Some("mine"));
    }

    #[test]
    fn keys_only_import_leaves_config_untouched() {
        let d = tmp("keysonly");
        std::fs::write(d.join("config.toml"), "hotkey = \"keep\"\n").unwrap();
        let p = build_payload(CFG, ENV, Selection { prefs: false, prompts: false, keys: true }).unwrap();
        apply(&d, &p).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("config.toml")).unwrap(), "hotkey = \"keep\"\n");
        assert!(!d.join("config.toml.bak").exists());
        assert!(std::fs::read_to_string(d.join(".env")).unwrap().contains("GROQ_API_KEY=gsk_1"));
    }

    #[test]
    fn content_outside_the_declared_scope_is_refused() {
        // a file that declares "keys only" but carries config settings, or vice versa, is not accepted
        let p = Payload { scope: Selection { prefs: false, prompts: false, keys: true }, config: "hotkey = \"x\"\n".into(), keys: BTreeMap::new() };
        assert_eq!(sanitize(p), Err(BackupError::Invalid));
        let p = Payload { scope: Selection { prefs: true, prompts: false, keys: false }, config: "default_prompt = \"x\"\n".into(), keys: BTreeMap::new() };
        assert_eq!(sanitize(p), Err(BackupError::Invalid));
        let mut keys = BTreeMap::new();
        keys.insert("GROQ_API_KEY".to_string(), "k".to_string());
        let p = Payload { scope: Selection { prefs: true, prompts: false, keys: false }, config: String::new(), keys };
        assert!(sanitize(p).unwrap().keys.is_empty());
    }
}
