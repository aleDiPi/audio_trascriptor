//! Registrazione dal microfono predefinito del sistema al momento dell'avvio.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

const TARGET_RATE: u32 = 16_000;

pub struct Recorder {
    stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    /// Quello che esce dagli altoparlanti, se la rimozione dell'eco e' attiva e disponibile
    reference: Option<reference::Capture>,
    skip_ms: u32,
}

/// True se su questo computer si puo' registrare l'uscita degli altoparlanti (serve per togliere l'eco).
pub fn echo_cancel_available() -> bool {
    reference::available()
}

/// Quanti campioni scartare all'inizio per non registrare il suono d'avvio (piu' un piccolo margine
/// per la latenza degli altoparlanti).
pub fn frames_to_skip(rate: u32, skip_ms: u32) -> usize {
    (rate as u64 * skip_ms as u64 / 1000) as usize
}

/// `skip_ms`: durata iniziale da scartare (il suono d'avvio); 0 per non scartare nulla.
/// `echo_cancel`: registra anche l'uscita degli altoparlanti per toglierla dal microfono a fine registrazione.
pub fn start(max_seconds: u32, skip_ms: u32, echo_cancel: bool) -> Result<Recorder, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| crate::i18n::t("err.no_mic"))?;
    let cfg = device.default_input_config().map_err(|e| e.to_string())?;
    crate::config::debug(|| format!("microphone: {:?}, {} Hz, {} channels, {:?}", device.description().map(|d| d.to_string()).unwrap_or_default(), cfg.sample_rate(), cfg.channels(), cfg.sample_format()));
    let rate: u32 = cfg.sample_rate().into();
    let channels = cfg.channels() as usize;
    let max = rate as usize * max_seconds as usize;
    let skip = frames_to_skip(rate, skip_ms);
    let buf = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 10)));
    let config = cfg.config();
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, &buf, channels, max, skip),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, &buf, channels, max, skip),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, &buf, channels, max, skip),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config, &buf, channels, max, skip),
        f => Err(crate::i18n::tf("err.audio_format", &[("format", &format!("{f:?}"))])),
    }?;
    let reference = if echo_cancel {
        match reference::Capture::start() {
            Ok(c) => Some(c),
            Err(e) => {
                crate::config::log(&format!("echo cancellation unavailable, recording without it: {e}"));
                None
            }
        }
    } else {
        None
    };
    stream.play().map_err(|e| e.to_string())?;
    Ok(Recorder { stream, buf, rate, reference, skip_ms })
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buf: &Arc<Mutex<Vec<f32>>>,
    channels: usize,
    max: usize,
    skip: usize,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let buf = Arc::clone(buf);
    let mut skip_left = skip;
    device
        .build_input_stream(
            config.clone(),
            move |data: &[T], _| {
                let mut b = buf.lock().unwrap();
                let (mut energy, mut n) = (0f32, 0usize);
                for frame in data.chunks(channels) {
                    if skip_left > 0 {
                        skip_left -= 1;
                        continue;
                    }
                    if b.len() >= max {
                        break;
                    }
                    let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                    let v = sum / frame.len() as f32;
                    energy += v * v;
                    n += 1;
                    b.push(v);
                }
                if n > 0 {
                    crate::levels::report((energy / n as f32).sqrt()); // volume per l'alone dell'icona
                }
            },
            |e| crate::config::log(&format!("audio stream error: {e}")),
            None,
        )
        .map_err(|e| e.to_string())
}

impl Recorder {
    /// Ferma la registrazione e restituisce un WAV mono 16 kHz, o None se vuoto/silenzioso.
    pub fn finish(self) -> Option<Vec<u8>> {
        drop(self.stream);
        let samples = std::mem::take(&mut *self.buf.lock().unwrap()); // senza copia: libera subito il buffer
        crate::config::debug(|| format!("recorded {:.1} s of audio", samples.len() as f32 / self.rate as f32));
        if samples.len() < self.rate as usize / 4 {
            crate::config::debug(|| "audio too short (under 0.25 s): discarded".into());
            return None; // meno di 0,25 s
        }
        let mut pcm = resample(&samples, self.rate, TARGET_RATE);
        if let Some(cap) = self.reference {
            let mut r = cap.finish();
            let skip = frames_to_skip(TARGET_RATE, self.skip_ms).min(r.len());
            r.drain(..skip); // come il microfono, senza il suono d'avvio
            crate::config::debug(|| format!("echo cancellation: {:.1} s of speaker audio", r.len() as f32 / TARGET_RATE as f32));
            let (lag, corr) = crate::aec::estimate_delay(&pcm, &r);
            crate::config::debug(|| format!("echo delay {} ms, correlation {corr:.2}", lag * 1000 / TARGET_RATE as i64));
            pcm = crate::aec::cancel(&pcm, &r);
        }
        let peak = pcm.iter().fold(0f32, |m, s| m.max(s.abs()));
        crate::config::debug(|| format!("signal peak {peak:.3}"));
        if peak < 0.005 {
            crate::config::debug(|| "silent audio (peak < 0.005): discarded, check the system default microphone".into());
            return None; // silenzio: evita allucinazioni di Whisper
        }
        let mut cur = Cursor::new(Vec::new());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: TARGET_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::new(&mut cur, spec).ok()?;
        for s in pcm {
            w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).ok()?;
        }
        w.finalize().ok()?;
        Some(cur.into_inner())
    }
}

pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (input.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = input[idx];
            let b = *input.get(idx + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

/// Registrazione di cio' che esce dagli altoparlanti, in mono. Windows: loopback WASAPI (cpal);
/// Linux: `parec` sul "monitor" dell'uscita predefinita (PulseAudio e PipeWire).
mod reference {
    use super::*;
    use std::time::Instant;

    pub struct Capture {
        buf: Arc<Mutex<Vec<f32>>>,
        started: Instant,
        rate: u32,
        #[cfg(windows)]
        _stream: cpal::Stream,
        #[cfg(target_os = "linux")]
        child: std::process::Child,
    }

    #[cfg(target_os = "linux")]
    impl Drop for Capture {
        fn drop(&mut self) {
            let _ = self.child.kill(); // anche se la registrazione viene annullata
            let _ = self.child.wait();
        }
    }

    impl Capture {
        /// Fine: restituisce l'audio a 16 kHz, pareggiato al tempo reale.
        pub fn finish(self) -> Vec<f32> {
            let this = self; // su Linux Drop ferma parec
            let mut b = std::mem::take(&mut *this.buf.lock().unwrap());
            crate::aec::pad_to_elapsed(&mut b, this.rate as usize, this.started.elapsed());
            resample(&b, this.rate, TARGET_RATE)
        }
    }

    fn push(buf: &Mutex<Vec<f32>>, started: Instant, rate: u32, mono: &[f32], max: usize) {
        let mut b = buf.lock().unwrap();
        // il tempo reale attuale meno la durata di questo blocco: i buchi (silenzio) si riempiono prima
        let at = started.elapsed().saturating_sub(std::time::Duration::from_secs_f64(mono.len() as f64 / rate as f64));
        crate::aec::pad_to_elapsed(&mut b, rate as usize, at);
        if b.len() < max {
            b.extend_from_slice(mono);
        }
    }

    #[cfg(target_os = "linux")]
    pub fn available() -> bool {
        std::process::Command::new("parec").arg("--version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
    }

    #[cfg(target_os = "linux")]
    impl Capture {
        pub fn start() -> Result<Capture, String> {
            use std::io::Read;
            use std::process::{Command, Stdio};
            let mut child = Command::new("parec")
                .args(["-d", "@DEFAULT_MONITOR@", "--format=float32le", "--rate=16000", "--channels=1", "--latency-msec=20"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .stdin(Stdio::null())
                .spawn()
                .map_err(|e| format!("parec: {e}"))?;
            let mut out = child.stdout.take().ok_or("parec: no output")?;
            let buf = Arc::new(Mutex::new(Vec::with_capacity(TARGET_RATE as usize * 10)));
            let started = Instant::now();
            let b2 = Arc::clone(&buf);
            std::thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                let mut carry: Vec<u8> = Vec::new();
                while let Ok(n) = out.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    carry.extend_from_slice(&chunk[..n]);
                    let whole = carry.len() / 4 * 4;
                    let samples: Vec<f32> = carry[..whole].chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                    carry.drain(..whole);
                    push(&b2, started, TARGET_RATE, &samples, TARGET_RATE as usize * 3600);
                }
            });
            Ok(Capture { buf, started, rate: TARGET_RATE, child })
        }
    }

    #[cfg(windows)]
    pub fn available() -> bool {
        cpal::default_host().default_output_device().is_some()
    }

    #[cfg(windows)]
    impl Capture {
        pub fn start() -> Result<Capture, String> {
            let host = cpal::default_host();
            let device = host.default_output_device().ok_or("no output device")?;
            let cfg = device.default_output_config().map_err(|e| e.to_string())?;
            let rate: u32 = cfg.sample_rate().into();
            let channels = cfg.channels() as usize;
            let buf = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 10)));
            let started = Instant::now();
            let config = cfg.config();
            let stream = match cfg.sample_format() {
                cpal::SampleFormat::F32 => build_loopback::<f32>(&device, &config, &buf, channels, rate, started),
                cpal::SampleFormat::I16 => build_loopback::<i16>(&device, &config, &buf, channels, rate, started),
                cpal::SampleFormat::U16 => build_loopback::<u16>(&device, &config, &buf, channels, rate, started),
                cpal::SampleFormat::I32 => build_loopback::<i32>(&device, &config, &buf, channels, rate, started),
                f => Err(format!("{f:?}")),
            }?;
            stream.play().map_err(|e| e.to_string())?;
            Ok(Capture { buf, started, rate, _stream: stream })
        }
    }

    /// Su WASAPI, aprire un dispositivo di uscita come ingresso attiva il "loopback".
    #[cfg(windows)]
    fn build_loopback<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buf: &Arc<Mutex<Vec<f32>>>,
        channels: usize,
        rate: u32,
        started: Instant,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let buf = Arc::clone(buf);
        device
            .build_input_stream(
                config.clone(),
                move |data: &[T], _| {
                    let mono: Vec<f32> = data.chunks(channels).map(|f| f.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / f.len() as f32).collect();
                    push(&buf, started, rate, &mono, rate as usize * 3600);
                },
                |e| crate::config::log(&format!("loopback stream error: {e}")),
                None,
            )
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_frames_follow_rate_and_duration() {
        assert_eq!(frames_to_skip(48_000, 200), 9_600);
        assert_eq!(frames_to_skip(16_000, 0), 0);
        assert_eq!(frames_to_skip(44_100, 1000), 44_100);
    }
}
