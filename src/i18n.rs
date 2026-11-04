//! Interface languages. Every language is a TOML file in `lang/` (see CONTRIBUTING.md): the built-in ones are
//! embedded in the program at build time (build.rs scans the folder), and extra ones can be dropped into the
//! `lang` folder next to config.toml without recompiling. English is the fallback for any missing text.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

include!(concat!(env!("OUT_DIR"), "/lang_files.rs"));

/// Language used when a text is missing from the chosen one (and when the system language is not available).
pub const FALLBACK: &str = "en";

/// Texts of one language: flat keys such as `tray.quit`.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub code: String,
    pub name: String,
    texts: HashMap<String, String>,
}

impl Catalog {
    /// Reads a language file. `code` comes from the file name (`it.toml` -> `it`).
    pub fn parse(code: &str, source: &str) -> Result<Catalog, String> {
        let value = toml::Value::Table(source.parse::<toml::Table>().map_err(|e| format!("{code}: {e}"))?);
        let mut texts = HashMap::new();
        flatten("", &value, &mut texts);
        let name = texts.remove("meta.name").filter(|n| !n.trim().is_empty()).unwrap_or_else(|| code.to_string());
        Ok(Catalog { code: code.to_string(), name, texts })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.texts.get(key).map(String::as_str).filter(|t| !t.is_empty())
    }

    /// True if the file has the key, even with an empty text (a translation not done yet).
    pub fn has_key(&self, key: &str) -> bool {
        self.texts.contains_key(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.texts.keys()
    }
}

fn flatten(prefix: &str, v: &toml::Value, out: &mut HashMap<String, String>) {
    match v {
        toml::Value::Table(t) => {
            for (k, x) in t {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten(&key, x, out);
            }
        }
        toml::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

/// Replaces `{name}` with the matching value of `args`. Unknown placeholders stay as they are.
pub fn fill(text: &str, args: &[(&str, &str)]) -> String {
    let mut out = text.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Text for `key` in `primary`, else in `fallback`, else the key itself (so a gap is visible, never a crash).
pub fn lookup(primary: &Catalog, fallback: &Catalog, key: &str) -> String {
    primary.get(key).or_else(|| fallback.get(key)).map_or_else(|| key.to_string(), str::to_string)
}

/// Language code from a system locale string: `it_IT.UTF-8` -> `it_it`, `en-US` -> `en_us`, `C`/`POSIX` -> "".
pub fn normalize_locale(raw: &str) -> String {
    let first = raw.split(':').next().unwrap_or("");
    let base = first.split(['.', '@']).next().unwrap_or("").trim().replace('-', "_").to_ascii_lowercase();
    if base == "c" || base == "posix" {
        String::new()
    } else {
        base
    }
}

/// Chooses the language: an explicit setting wins if that language exists, otherwise the system locale
/// (full `pt_br` first, then `pt`), otherwise English. `available` holds language codes in lower case.
pub fn resolve(setting: &str, system_locale: &str, available: &[String]) -> String {
    let has = |c: &str| available.iter().any(|a| a.eq_ignore_ascii_case(c));
    let pick = |raw: &str| -> Option<String> {
        let loc = normalize_locale(raw);
        if loc.is_empty() {
            return None;
        }
        let base = loc.split('_').next().unwrap_or("").to_string();
        [loc.clone(), base].into_iter().find(|c| !c.is_empty() && has(c))
    };
    let setting = setting.trim();
    if !setting.is_empty() && !setting.eq_ignore_ascii_case("auto") {
        if let Some(c) = pick(setting) {
            return c;
        }
    }
    pick(system_locale).unwrap_or_else(|| FALLBACK.to_string())
}

/// Locale of the operating system as reported by it (empty if unknown).
pub fn system_locale() -> String {
    #[cfg(windows)]
    {
        let mut buf = [0u16; 85];
        // SAFETY: the buffer is 85 u16 as required for a locale name (LOCALE_NAME_MAX_LENGTH).
        let n = unsafe { windows_sys::Win32::Globalization::GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if n > 1 {
            return String::from_utf16_lossy(&buf[..(n as usize - 1)]);
        }
        String::new()
    }
    #[cfg(not(windows))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"].iter().filter_map(|k| std::env::var(k).ok()).find(|v| !normalize_locale(v).is_empty()).unwrap_or_default()
    }
}

// ---------- languages in use

struct Active {
    primary: Catalog,
    fallback: Catalog,
    all: Vec<Catalog>,
}

static ACTIVE: RwLock<Option<Arc<Active>>> = RwLock::new(None);
static GENERATION: AtomicU32 = AtomicU32::new(0);
type Hook = Arc<dyn Fn() + Send + Sync>;
static ON_CHANGE: Mutex<Option<Hook>> = Mutex::new(None);

/// Grows every time the language is (re)selected: lets the tray menu notice that it must rewrite its labels.
pub fn generation() -> u32 {
    GENERATION.load(Ordering::Relaxed)
}

/// Function called after a language change (the tray uses it to wake its loop).
pub fn set_on_change(f: Hook) {
    *ON_CHANGE.lock().unwrap() = Some(f);
}

/// All languages: the embedded ones, then those found in `extra_dir` (a file there replaces the embedded one).
pub fn load_all(extra_dir: Option<&std::path::Path>) -> Vec<Catalog> {
    let mut all: Vec<Catalog> = EMBEDDED.iter().filter_map(|(code, src)| Catalog::parse(code, src).ok()).collect();
    if let Some(dir) = extra_dir {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let code = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") || code.is_empty() || code.starts_with('_') {
                continue;
            }
            match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| Catalog::parse(&code, &s)) {
                Ok(c) => {
                    all.retain(|x| x.code != code);
                    all.push(c);
                }
                Err(e) => crate::config::log(&format!("language file ignored ({}): {e}", path.display())),
            }
        }
    }
    all.sort_by(|a, b| a.code.cmp(&b.code));
    all
}

/// Picks the language (see `resolve`) and makes it the current one. Returns its code.
pub fn init(setting: &str, extra_dir: Option<&std::path::Path>) -> String {
    let all = load_all(extra_dir);
    let codes: Vec<String> = all.iter().map(|c| c.code.clone()).collect();
    let code = resolve(setting, &system_locale(), &codes);
    let fallback = all.iter().find(|c| c.code == FALLBACK).cloned().unwrap_or_default();
    let primary = all.iter().find(|c| c.code == code).cloned().unwrap_or_else(|| fallback.clone());
    *ACTIVE.write().unwrap() = Some(Arc::new(Active { primary, fallback, all }));
    GENERATION.fetch_add(1, Ordering::Relaxed);
    let hook = ON_CHANGE.lock().unwrap().clone();
    if let Some(f) = hook {
        f();
    }
    code
}

fn active() -> Arc<Active> {
    if let Some(a) = ACTIVE.read().unwrap().as_ref() {
        return a.clone();
    }
    // tests must not depend on the language of the machine that runs them
    init(if cfg!(test) { "en" } else { "auto" }, None);
    ACTIVE.read().unwrap().as_ref().unwrap().clone()
}

/// Text for `key` in the current language.
pub fn t(key: &str) -> String {
    let a = active();
    lookup(&a.primary, &a.fallback, key)
}

/// Text for `key` with `{name}` placeholders filled in.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    fill(&t(key), args)
}

/// (code, native name) of every available language, for the language list in the settings.
pub fn languages() -> Vec<(String, String)> {
    active().all.iter().map(|c| (c.code.clone(), c.name.clone())).collect()
}

/// Code of the language in use.
pub fn current() -> String {
    active().primary.code.clone()
}

/// Entries of the language list in the settings: ("auto", automatic) followed by every language as (code, name).
pub fn choices(langs: &[(String, String)], auto_label: &str) -> Vec<(String, String)> {
    std::iter::once(("auto".to_string(), auto_label.to_string())).chain(langs.iter().cloned()).collect()
}

/// Position in `choices` of the saved setting (0 = automatic, also for an unknown language).
pub fn choice_index(setting: &str, choices: &[(String, String)]) -> usize {
    let s = setting.trim();
    choices.iter().position(|(c, _)| c.eq_ignore_ascii_case(s)).filter(|_| !s.is_empty()).unwrap_or(0)
}

#[cfg(test)]
pub fn embedded() -> Vec<Catalog> {
    EMBEDDED.iter().map(|(c, s)| Catalog::parse(c, s).unwrap()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_flattens_sections_and_reads_the_name() {
        let c = Catalog::parse("xx", "[meta]\nname = \"Xish\"\n[tray]\nquit = \"Bye\"\n[a.b]\nc = \"deep\"").unwrap();
        assert_eq!(c.name, "Xish");
        assert_eq!(c.get("tray.quit"), Some("Bye"));
        assert_eq!(c.get("a.b.c"), Some("deep"));
        assert_eq!(c.get("meta.name"), None, "meta.name is not a text");
    }

    #[test]
    fn name_defaults_to_the_code_and_bad_files_are_errors() {
        assert_eq!(Catalog::parse("xx", "[tray]\nquit = \"Bye\"").unwrap().name, "xx");
        assert!(Catalog::parse("xx", "this is = = not toml").is_err());
    }

    #[test]
    fn empty_texts_count_as_missing_so_a_template_falls_back_to_english() {
        let en = Catalog::parse("en", "[tray]\nquit = \"Quit\"").unwrap();
        let xx = Catalog::parse("xx", "[tray]\nquit = \"\"").unwrap();
        assert_eq!(lookup(&xx, &en, "tray.quit"), "Quit");
        assert_eq!(lookup(&xx, &en, "tray.unknown"), "tray.unknown");
    }

    #[test]
    fn placeholders_are_filled_and_unknown_ones_stay() {
        assert_eq!(fill("Hi {name}, {n} left, {other}", &[("name", "Ann"), ("n", "3")]), "Hi Ann, 3 left, {other}");
    }

    #[test]
    fn locale_strings_are_normalised() {
        assert_eq!(normalize_locale("it_IT.UTF-8"), "it_it");
        assert_eq!(normalize_locale("en-US"), "en_us");
        assert_eq!(normalize_locale("de_DE@euro"), "de_de");
        assert_eq!(normalize_locale("fr_FR:en_US"), "fr_fr");
        assert_eq!(normalize_locale("C"), "");
        assert_eq!(normalize_locale("POSIX"), "");
        assert_eq!(normalize_locale(""), "");
    }

    #[test]
    fn resolve_follows_the_system_unless_the_user_chose() {
        let av = codes(&["en", "it", "pt_br"]);
        assert_eq!(resolve("auto", "it-IT", &av), "it");
        assert_eq!(resolve("", "it_IT.UTF-8", &av), "it");
        assert_eq!(resolve("auto", "pt-BR", &av), "pt_br", "the regional variant wins when it exists");
        assert_eq!(resolve("auto", "pt-PT", &av), "en", "no base 'pt' file: English");
        assert_eq!(resolve("auto", "ja-JP", &av), "en");
        assert_eq!(resolve("auto", "", &av), "en");
        assert_eq!(resolve("en", "it-IT", &av), "en", "an explicit choice beats the system");
        assert_eq!(resolve("IT", "en-US", &av), "it");
        assert_eq!(resolve("xx", "it-IT", &av), "it", "an unknown choice falls back to the system language");
    }

    #[test]
    fn language_list_starts_with_automatic_and_finds_the_saved_choice() {
        let langs = vec![("en".to_string(), "English".to_string()), ("it".to_string(), "Italiano".to_string())];
        let c = choices(&langs, "Automatic");
        assert_eq!(c.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["auto", "en", "it"]);
        assert_eq!(choice_index("auto", &c), 0);
        assert_eq!(choice_index("", &c), 0);
        assert_eq!(choice_index("IT", &c), 2);
        assert_eq!(choice_index("xx", &c), 0);
    }

    #[test]
    fn extra_language_files_are_discovered_and_override_embedded_ones() {
        let dir = std::env::temp_dir().join(format!("trascrittore-lang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("de.toml"), "[meta]\nname = \"Deutsch\"\n[tray]\nquit = \"Beenden\"").unwrap();
        std::fs::write(dir.join("_template.toml"), "[meta]\nname = \"\"").unwrap();
        std::fs::write(dir.join("broken.toml"), "= = =").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let all = load_all(Some(&dir));
        let de = all.iter().find(|c| c.code == "de").expect("de.toml is picked up");
        assert_eq!(de.name, "Deutsch");
        assert!(all.iter().any(|c| c.code == "en") && all.iter().any(|c| c.code == "it"));
        assert!(!all.iter().any(|c| c.code == "_template" || c.code == "broken" || c.code == "notes"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_embedded_language_has_the_same_keys_and_placeholders_as_english() {
        let all = embedded();
        let en = all.iter().find(|c| c.code == FALLBACK).expect("en.toml is embedded");
        assert!(!en.keys().next().is_none());
        let holders = |s: &str| {
            let mut v: Vec<String> = s.split('{').skip(1).filter_map(|r| r.split('}').next()).map(str::to_string).collect();
            v.sort();
            v
        };
        for c in all.iter().filter(|c| c.code != FALLBACK) {
            for k in en.keys() {
                assert!(c.has_key(k), "{}.toml is missing '{k}'", c.code);
                // an empty text is a translation not done yet (it falls back to English); a filled one must keep the placeholders
                if let Some(text) = c.get(k) {
                    assert_eq!(holders(text), holders(en.get(k).unwrap()), "{}.toml: placeholders of '{k}' differ from English", c.code);
                }
            }
            for k in c.keys() {
                assert!(en.has_key(k), "{}.toml has '{k}' which English does not", c.code);
            }
        }
    }

    /// Text keys that appear as string literals in a source file, such as "tray.quit" or "err.network".
    fn keys_in(source: &str) -> Vec<String> {
        const AREAS: [&str; 10] = ["tray", "cfg", "gui", "failure", "status", "usage", "size", "fmt", "err", "gh"];
        let mut found = Vec::new();
        for area in AREAS.iter().chain(["ui"].iter()) {
            let start = format!("\"{area}.");
            for part in source.split(&start).skip(1) {
                if let Some((name, _)) = part.split_once('"') {
                    if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                        found.push(format!("{area}.{name}"));
                    }
                }
            }
        }
        found
    }

    #[test]
    fn every_key_used_in_the_code_exists_and_every_key_is_used() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let en = embedded().into_iter().find(|c| c.code == FALLBACK).unwrap();
        let mut used = std::collections::HashSet::new();
        for entry in std::fs::read_dir(root.join("src")).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") || path.ends_with("i18n.rs") {
                continue;
            }
            for k in keys_in(&std::fs::read_to_string(&path).unwrap()) {
                assert!(en.get(&k).is_some(), "{} uses the text '{k}' which lang/en.toml does not have", path.display());
                used.insert(k);
            }
        }
        // the [ui] texts are used by the window: build.rs sends every one of them to the Slint global Tr
        let slint = std::fs::read_to_string(root.join("ui/settings.slint")).unwrap();
        for k in en.keys() {
            if let Some(name) = k.strip_prefix("ui.") {
                assert!(used.contains(k) || slint.contains(&format!("Tr.{name}")), "'{k}' is in lang/en.toml but nothing uses it");
            } else {
                assert!(used.contains(k), "'{k}' is in lang/en.toml but no code uses it");
            }
        }
    }

    #[test]
    fn the_window_has_no_hard_coded_text_left() {
        let slint = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/settings.slint")).unwrap();
        for (n, line) in slint.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for key in ["text:", "title:", "label:", "placeholder-text:", "placeholder:"] {
                if let Some((_, rest)) = code.split_once(key) {
                    let lit = rest.trim_start();
                    if lit.starts_with('"') && lit[1..].chars().any(|c| c.is_alphabetic()) && !lit.starts_with("\"github_pat") {
                        let allowed = ["\"Trascrittore\"", "\"⋮⋮\"", "\"github_pat_...\""];
                        assert!(allowed.iter().any(|a| lit.starts_with(a)), "settings.slint:{}: hard-coded text: {line}", n + 1);
                    }
                }
            }
        }
    }

    #[test]
    fn the_template_has_every_key_of_english_and_all_of_them_empty() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let template = Catalog::parse("_template", &std::fs::read_to_string(root.join("lang/_template.toml")).unwrap()).unwrap();
        let en = embedded().into_iter().find(|c| c.code == FALLBACK).unwrap();
        for k in en.keys() {
            assert!(template.has_key(k), "lang/_template.toml lacks '{k}': regenerate it from en.toml");
            assert!(template.get(k).is_none(), "'{k}' in the template must be empty");
        }
        for k in template.keys() {
            assert!(en.has_key(k), "lang/_template.toml has '{k}' which English does not");
        }
    }

    #[test]
    fn the_template_works_as_a_translation_that_falls_back_to_english() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let template = Catalog::parse("xx", &std::fs::read_to_string(root.join("lang/_template.toml")).unwrap()).unwrap();
        let en = embedded().into_iter().find(|c| c.code == FALLBACK).unwrap();
        assert_eq!(lookup(&template, &en, "tray.quit"), en.get("tray.quit").unwrap());
    }
}
