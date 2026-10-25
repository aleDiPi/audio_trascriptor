//! Configurazione (config.toml), segreti (.env) e log.

use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const DEFAULT_CONFIG: &str = include_str!("../config/config.example.toml");
const DEFAULT_ENV: &str = include_str!("../.env.example");

static DIR: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub hotkey: String,
    pub mode: String,
    pub language: String,
    pub stt_model: String,
    pub llm_model: String,
    pub max_seconds: u32,
    pub restore_clipboard: bool,
    pub default_prompt: String,
    pub dictionary: Vec<String>,
    pub apps: Vec<AppRule>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AppRule {
    pub exe: Vec<String>,
    pub title_contains: String,
    pub prompt: String,
    pub raw: bool,
    pub paste: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            hotkey: "ctrl+win".into(),
            mode: "hold".into(),
            language: "it".into(),
            stt_model: "whisper-large-v3-turbo".into(),
            llm_model: "llama-3.3-70b-versatile".into(),
            max_seconds: 120,
            restore_clipboard: true,
            default_prompt: "Correggi punteggiatura ed errori del testo dettato, senza cambiarne il significato.".into(),
            dictionary: Vec::new(),
            apps: Vec::new(),
        }
    }
}

impl Config {
    pub fn is_toggle(&self) -> bool {
        self.mode.eq_ignore_ascii_case("toggle")
    }

    /// Regola da applicare all'app in primo piano (None = prompt predefinito).
    pub fn rule_for(&self, exe: &str, title: &str) -> Option<&AppRule> {
        let exe = exe.to_lowercase();
        let title = title.to_lowercase();
        self.apps.iter().find(|r| {
            r.exe.iter().any(|e| e.eq_ignore_ascii_case(&exe))
                && (r.title_contains.is_empty() || title.contains(&r.title_contains.to_lowercase()))
        })
    }
}

/// Cartella di lavoro: la prima tra cwd, cartella dell'exe, %APPDATA%\Trascrittore
/// che contiene config.toml; altrimenti viene creata %APPDATA%\Trascrittore.
pub fn dir() -> &'static Path {
    DIR.get_or_init(|| {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(d) = std::env::current_dir() {
            candidates.push(d);
        }
        if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            candidates.push(d);
        }
        let appdata = std::env::var_os("APPDATA")
            .map(|a| PathBuf::from(a).join("Trascrittore"))
            .unwrap_or_else(|| PathBuf::from("."));
        candidates.push(appdata.clone());
        for c in &candidates {
            if c.join("config.toml").is_file() {
                return c.clone();
            }
        }
        let _ = std::fs::create_dir_all(&appdata);
        let _ = std::fs::write(appdata.join("config.toml"), DEFAULT_CONFIG);
        if !appdata.join(".env").exists() {
            let _ = std::fs::write(appdata.join(".env"), DEFAULT_ENV);
        }
        appdata
    })
}

pub fn load() -> Config {
    let path = dir().join("config.toml");
    match std::fs::read_to_string(&path) {
        Ok(s) => match toml::from_str::<Config>(&s) {
            Ok(c) => c,
            Err(e) => {
                log(&format!("config.toml non valido, uso i valori predefiniti: {e}"));
                Config::default()
            }
        },
        Err(e) => {
            log(&format!("config.toml non leggibile: {e}"));
            Config::default()
        }
    }
}

/// Chiave Groq: variabile d'ambiente GROQ_API_KEY oppure riga nel file .env.
pub fn groq_key() -> Option<String> {
    if let Ok(k) = std::env::var("GROQ_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    let text = std::fs::read_to_string(dir().join(".env")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "GROQ_API_KEY" {
                let v = v.trim().trim_matches('"').trim_matches('\'').trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Log minimale su trascrittore.log (troncato oltre 256 KB). Mai chiavi o testo dettato.
pub fn log(msg: &str) {
    let path = dir().join("trascrittore.log");
    if std::fs::metadata(&path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}
