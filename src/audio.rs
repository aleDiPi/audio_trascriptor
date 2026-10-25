//! Registrazione dal microfono predefinito di Windows al momento dell'avvio.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

const TARGET_RATE: u32 = 16_000;

pub struct Recorder {
    stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

pub fn start(max_seconds: u32) -> Result<Recorder, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("nessun microfono predefinito")?;
    let cfg = device.default_input_config().map_err(|e| e.to_string())?;
    let rate: u32 = cfg.sample_rate().into();
    let channels = cfg.channels() as usize;
    let max = rate as usize * max_seconds as usize;
    let buf = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 10)));
    let config = cfg.config();
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, &buf, channels, max),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, &buf, channels, max),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, &buf, channels, max),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config, &buf, channels, max),
        f => Err(format!("formato audio non supportato: {f:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Recorder { stream, buf, rate })
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buf: &Arc<Mutex<Vec<f32>>>,
    channels: usize,
    max: usize,
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
                let mut b = buf.lock().unwrap();
                for frame in data.chunks(channels) {
                    if b.len() >= max {
                        break;
                    }
                    let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                    b.push(sum / frame.len() as f32);
                }
            },
            |e| crate::config::log(&format!("errore stream audio: {e}")),
            None,
        )
        .map_err(|e| e.to_string())
}

impl Recorder {
    /// Ferma la registrazione e restituisce un WAV mono 16 kHz, o None se vuoto/silenzioso.
    pub fn finish(self) -> Option<Vec<u8>> {
        drop(self.stream);
        let samples = self.buf.lock().unwrap().clone();
        if samples.len() < self.rate as usize / 4 {
            return None; // meno di 0,25 s
        }
        let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        if peak < 0.005 {
            return None; // silenzio: evita allucinazioni di Whisper
        }
        let pcm = resample(&samples, self.rate, TARGET_RATE);
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

fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
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
