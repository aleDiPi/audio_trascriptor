//! Trascrizione locale con Whisper (whisper.cpp), ultima risorsa quando nessun servizio in rete risponde.
//! Il modello non e' nell'installer: si scarica a richiesta dalle Impostazioni in `<cartella dati>\models`.
//! Viene caricato solo durante la trascrizione e liberato subito dopo, cosi' a riposo non occupa RAM.

use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Tiny,
    Base,
    Small,
    Medium,
}

pub const SIZES: [Size; 4] = [Size::Tiny, Size::Base, Size::Small, Size::Medium];

impl Size {
    pub fn id(self) -> &'static str {
        match self {
            Size::Tiny => "tiny",
            Size::Base => "base",
            Size::Small => "small",
            Size::Medium => "medium",
        }
    }

    /// Nome sconosciuto o vuoto: `Base` (buon compromesso per l'italiano).
    pub fn parse(s: &str) -> Size {
        SIZES.into_iter().find(|x| x.id() == s.trim().to_ascii_lowercase()).unwrap_or(Size::Base)
    }

    /// Modelli multilingua (non i `.en`): servono per l'italiano.
    pub fn file_name(self) -> String {
        format!("ggml-{}.bin", self.id())
    }

    pub fn url(self) -> String {
        format!("{BASE_URL}/{}", self.file_name())
    }

    /// Dimensione approssimativa del file, in byte.
    pub fn approx_bytes(self) -> u64 {
        match self {
            Size::Tiny => 75_000_000,
            Size::Base => 142_000_000,
            Size::Small => 466_000_000,
            Size::Medium => 1_530_000_000,
        }
    }

    /// Testo dell'elenco a discesa.
    pub fn label(self) -> String {
        let note = match self {
            Size::Tiny => "size.tiny",
            Size::Base => "size.base",
            Size::Small => "size.small",
            Size::Medium => "size.medium",
        };
        let note = crate::i18n::t(note);
        format!("{} - {} ({note})", self.id(), format_bytes(self.approx_bytes()))
    }
}

pub fn models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models")
}

pub fn model_path(data_dir: &Path, size: Size) -> PathBuf {
    models_dir(data_dir).join(size.file_name())
}

/// Il file c'e' ed e' (quasi) completo: un download interrotto resta come `.part` e non conta.
pub fn is_downloaded(data_dir: &Path, size: Size) -> bool {
    std::fs::metadata(model_path(data_dir, size)).map_or(false, |m| m.len() >= size.approx_bytes() / 10 * 9)
}

/// Spazio occupato dai modelli scaricati (anche i download interrotti).
pub fn disk_usage(data_dir: &Path) -> u64 {
    std::fs::read_dir(models_dir(data_dir))
        .map(|d| d.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

/// Cancella il modello (e l'eventuale download interrotto) di quella dimensione.
pub fn delete(data_dir: &Path, size: Size) {
    let p = model_path(data_dir, size);
    let _ = std::fs::remove_file(part_path(&p));
    let _ = std::fs::remove_file(p);
}

fn part_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// "142 MB", "1.5 GB" (the decimal separator comes from the language file: "1,5 GB" in Italian).
pub fn format_bytes(n: u64) -> String {
    const MB: f64 = 1_000_000.0;
    let mb = n as f64 / MB;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0).replace('.', &crate::i18n::t("fmt.decimal_separator"))
    } else if mb >= 1.0 {
        format!("{:.0} MB", mb)
    } else {
        format!("{} KB", n / 1000)
    }
}

/// Avanzamento 0..1 (None se la dimensione totale non si conosce).
pub fn fraction(done: u64, total: Option<u64>) -> Option<f32> {
    total.filter(|t| *t > 0).map(|t| (done as f32 / t as f32).clamp(0.0, 1.0))
}

/// L'ETag di un file LFS di Hugging Face e' il suo SHA-256: `"<64 cifre esadecimali>"`.
pub fn etag_sha256(header: &str) -> Option<String> {
    let h = header.trim().trim_start_matches("W/").trim_matches('"').to_ascii_lowercase();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// I file ggml iniziano con il numero magico "ggml" (little endian: `lmgg`).
pub fn has_ggml_magic(head: &[u8]) -> bool {
    head.starts_with(b"lmgg")
}

/// WAV mono a 16 bit (quello che produce la registrazione) -> campioni f32 per Whisper.
pub fn wav_to_samples(wav: &[u8]) -> Result<Vec<f32>, String> {
    let mut r = hound::WavReader::new(Cursor::new(wav)).map_err(|e| e.to_string())?;
    let spec = r.spec();
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 || spec.channels != 1 || spec.sample_rate != 16_000 {
        return Err(crate::i18n::tf("err.audio_whisper", &[("spec", &format!("{spec:?}"))]));
    }
    r.samples::<i16>().map(|s| s.map(|v| v as f32 / i16::MAX as f32).map_err(|e| e.to_string())).collect()
}

/// Trascrive sul PC. Carica il modello, trascrive e lo libera alla fine. `language`: "it", "en"... oppure "auto".
pub fn transcribe(model: &Path, samples: &[f32], language: &str) -> Result<String, String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
    let path = model.to_str().ok_or_else(|| crate::i18n::t("err.model_path"))?;
    let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default()).map_err(|e| crate::i18n::tf("err.model_load", &[("err", &e.to_string())]))?;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let lang = language.trim().to_ascii_lowercase();
    params.set_language(if lang.is_empty() || lang == "auto" { None } else { Some(lang.as_str()) });
    params.set_n_threads(std::thread::available_parallelism().map_or(2, |n| n.get().min(8)) as i32);
    params.set_translate(false);
    params.set_no_context(true);
    params.set_suppress_blank(true);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    state.full(params, samples).map_err(|e| crate::i18n::tf("err.local_failed", &[("err", &e.to_string())]))?;
    let mut text = String::new();
    for i in 0..state.full_n_segments() {
        if let Some(seg) = state.get_segment(i) {
            text.push_str(&seg.to_str_lossy().map_err(|e| e.to_string())?);
        }
    }
    Ok(text.trim().to_string())
}

/// Scarica il modello in `models/` (file `.part` poi rinominato). `on_progress(scaricati, totale)`.
/// Controlla il SHA-256 dichiarato da Hugging Face e il numero magico; `cancel` interrompe e cancella il parziale.
pub fn download(data_dir: &Path, size: Size, cancel: &AtomicBool, mut on_progress: impl FnMut(u64, Option<u64>)) -> Result<(), String> {
    let dest = model_path(data_dir, size);
    std::fs::create_dir_all(models_dir(data_dir)).map_err(|e| crate::i18n::tf("err.models_dir", &[("err", &e.to_string())]))?;
    let part = part_path(&dest);
    let url = size.url();

    // SHA-256 atteso: sta negli header della prima risposta (prima del reindirizzamento al CDN)
    let expected = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .ok()
        .and_then(|c| c.head(&url).send().ok())
        .and_then(|r| r.headers().get("x-linked-etag").and_then(|v| v.to_str().ok()).and_then(etag_sha256));

    let client = reqwest::blocking::Client::builder().connect_timeout(std::time::Duration::from_secs(30)).build().map_err(|e| e.to_string())?;
    let mut resp = client.get(&url).send().map_err(|e| crate::i18n::tf("err.network", &[("msg", &e.to_string())]))?;
    if !resp.status().is_success() {
        return Err(crate::i18n::tf("err.download_refused", &[("status", &resp.status().as_u16().to_string())]));
    }
    let total = resp.content_length();
    let mut file = std::fs::File::create(&part).map_err(|e| crate::i18n::tf("err.file_create", &[("err", &e.to_string())]))?;
    let (mut hasher, mut done, mut buf) = (Sha256::new(), 0u64, vec![0u8; 64 * 1024]);
    let mut first = true;
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err(crate::i18n::t("err.download_cancelled"));
        }
        let n = resp.read(&mut buf).map_err(|e| crate::i18n::tf("err.network", &[("msg", &e.to_string())]))?;
        if n == 0 {
            break;
        }
        if first {
            first = false;
            if !has_ggml_magic(&buf[..n]) {
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Err(crate::i18n::t("err.not_whisper"));
            }
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| crate::i18n::tf("err.write", &[("err", &e.to_string())]))?;
        done += n as u64;
        on_progress(done, total);
    }
    drop(file);
    if total.map_or(false, |t| t != done) {
        let _ = std::fs::remove_file(&part);
        return Err(crate::i18n::t("err.download_incomplete"));
    }
    if let Some(want) = expected {
        let got = hex(&hasher.finalize());
        if got != want {
            let _ = std::fs::remove_file(&part);
            return Err(crate::i18n::t("err.download_corrupt"));
        }
    }
    std::fs::rename(&part, &dest).map_err(|e| crate::i18n::tf("err.model_finish", &[("err", &e.to_string())]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trascrittore-localstt-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn size_parsing_defaults_to_base_and_files_are_multilingual() {
        assert_eq!(Size::parse(" Small "), Size::Small);
        assert_eq!(Size::parse("medium"), Size::Medium);
        assert_eq!(Size::parse(""), Size::Base);
        assert_eq!(Size::parse("large"), Size::Base);
        for s in SIZES {
            assert_eq!(Size::parse(s.id()), s);
            assert!(!s.file_name().contains(".en"), "serve il modello multilingua per l'italiano");
            assert!(s.url().starts_with("https://huggingface.co/") && s.url().ends_with(&s.file_name()));
            assert!(s.label().starts_with(s.id()));
        }
        assert!(Size::Tiny.approx_bytes() < Size::Base.approx_bytes() && Size::Small.approx_bytes() < Size::Medium.approx_bytes());
    }

    #[test]
    fn bytes_are_formatted_in_italian() {
        assert_eq!(format_bytes(142_000_000), "142 MB");
        assert_eq!(format_bytes(1_530_000_000), "1.5 GB");
        assert_eq!(format_bytes(500_000), "500 KB");
    }

    #[test]
    fn progress_fraction_is_bounded_and_optional() {
        assert_eq!(fraction(50, Some(200)), Some(0.25));
        assert_eq!(fraction(300, Some(200)), Some(1.0));
        assert_eq!(fraction(5, None), None);
        assert_eq!(fraction(5, Some(0)), None);
    }

    #[test]
    fn etag_is_accepted_only_as_sha256() {
        let sha = "a".repeat(64);
        assert_eq!(etag_sha256(&format!("\"{sha}\"")), Some(sha.clone()));
        assert_eq!(etag_sha256(&format!("W/\"{}\"", sha.to_uppercase())), Some(sha));
        assert_eq!(etag_sha256("\"abc123\""), None);
        assert_eq!(etag_sha256(&format!("\"{}\"", "z".repeat(64))), None);
    }

    #[test]
    fn ggml_magic_check() {
        assert!(has_ggml_magic(b"lmgg\x01\x02"));
        assert!(!has_ggml_magic(b"<html>"));
    }

    #[test]
    fn only_a_complete_file_counts_as_downloaded() {
        let d = tmp("dl");
        assert!(!is_downloaded(&d, Size::Tiny));
        std::fs::create_dir_all(models_dir(&d)).unwrap();
        let p = model_path(&d, Size::Tiny);
        std::fs::write(&p, vec![0u8; 1000]).unwrap();
        assert!(!is_downloaded(&d, Size::Tiny), "file troppo piccolo");
        std::fs::write(part_path(&p), vec![0u8; 500]).unwrap();
        assert_eq!(disk_usage(&d), 1500);
        delete(&d, Size::Tiny);
        assert!(!p.exists() && !part_path(&p).exists());
        assert_eq!(disk_usage(&d), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn wav_is_converted_to_float_samples() {
        let mut cur = Cursor::new(Vec::new());
        let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
        for v in [0i16, i16::MAX, i16::MIN + 1] {
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
        let s = wav_to_samples(&cur.into_inner()).unwrap();
        assert_eq!(s.len(), 3);
        assert!(s[0] == 0.0 && (s[1] - 1.0).abs() < 1e-6 && (s[2] + 1.0).abs() < 1e-6);
        // formato diverso: rifiutato
        let mut cur = Cursor::new(Vec::new());
        let bad = hound::WavSpec { sample_rate: 44_100, ..spec };
        hound::WavWriter::new(&mut cur, bad).unwrap().finalize().unwrap();
        assert!(wav_to_samples(&cur.into_inner()).is_err());
        assert!(wav_to_samples(b"non un wav").is_err());
    }
}
