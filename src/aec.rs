//! Rimozione dell'eco del computer dalla registrazione del microfono.
//!
//! Durante la dettatura si registra anche cio' che esce dagli altoparlanti (il "riferimento") e,
//! a registrazione finita, lo si sottrae dal microfono con un filtro adattivo (NLMS). Il calcolo e'
//! fatto a posteriori sull'audio gia' registrato, quindi non pesa durante la dettatura.
//! Tutto e' in funzioni pure (senza dispositivi), cosi' si puo' provare con segnali sintetici.

use std::time::Duration;

/// Frequenza di lavoro (la stessa del WAV finale).
pub const RATE: usize = 16_000;
/// Ritardo massimo cercato tra riferimento e microfono (ms): le schede audio possono avere una latenza alta.
const MAX_LAG_MS: usize = 400;
/// Quanto il riferimento puo' arrivare DOPO il microfono (ms): jitter di avvio dei due flussi.
const MIN_LAG_MS: usize = 100;
/// Lunghezza del filtro: 1024 campioni = 64 ms di eco di stanza dopo il ritardo principale.
const TAPS: usize = 1024;
/// Dove cade il ritardo principale dentro il filtro, cosi' ci stanno anche i riflessi anticipati.
const LEAD: usize = 64;
/// Sotto questa correlazione normalizzata si considera che non ci sia eco (es. cuffie): il microfono resta intatto.
const MIN_CORR: f32 = 0.08;
const DECIM: usize = 8;

/// Aggiunge silenzio in coda a `buf` finche' non copre `elapsed` di tempo reale a `rate` Hz.
/// Serve perche' alcuni flussi (WASAPI loopback) non mandano dati quando non c'e' audio: senza
/// riempire i vuoti il riferimento si sposterebbe rispetto al microfono.
pub fn pad_to_elapsed(buf: &mut Vec<f32>, rate: usize, elapsed: Duration) {
    let want = (elapsed.as_secs_f64() * rate as f64) as usize;
    if buf.len() < want {
        buf.resize(want, 0.0);
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Riferimento spostato di `lag` campioni: out[n] = r[n - lag] (zero fuori), lungo `len`.
fn shifted(r: &[f32], lag: i64, len: usize) -> Vec<f32> {
    (0..len as i64)
        .map(|n| {
            let i = n - lag;
            if i >= 0 && (i as usize) < r.len() { r[i as usize] } else { 0.0 }
        })
        .collect()
}

fn decimate(x: &[f32]) -> Vec<f32> {
    x.chunks_exact(DECIM).map(|c| c.iter().sum::<f32>() / DECIM as f32).collect()
}

/// Ritardo (in campioni a 16 kHz) con cui l'eco compare nel microfono rispetto al riferimento,
/// e correlazione normalizzata (0..1) a quel ritardo. Positivo = il microfono sente dopo.
pub fn estimate_delay(mic: &[f32], reference: &[f32]) -> (i64, f32) {
    let n = mic.len().min(reference.len()).min(RATE * 10);
    if n < RATE / 2 {
        return (0, 0.0);
    }
    let (m, r) = (decimate(&mic[..n]), decimate(&reference[..n]));
    let max_l = (MAX_LAG_MS * RATE / 1000 / DECIM) as i64;
    let min_l = -((MIN_LAG_MS * RATE / 1000 / DECIM) as i64);
    let corr_at = |m: &[f32], r: &[f32], l: i64| -> (f32, f32) {
        // somma di m[i] * r[i - l]
        let (a0, b0) = if l >= 0 { (l as usize, 0) } else { (0, (-l) as usize) };
        let len = m.len().saturating_sub(a0).min(r.len().saturating_sub(b0));
        if len == 0 {
            return (0.0, 0.0);
        }
        let (ms, rs) = (&m[a0..a0 + len], &r[b0..b0 + len]);
        (dot(ms, rs), (dot(ms, ms) * dot(rs, rs)).sqrt())
    };
    let mut best = (0i64, f32::MIN);
    for l in min_l..=max_l {
        let (c, _) = corr_at(&m, &r, l);
        if c > best.1 {
            best = (l, c);
        }
    }
    // rifinitura a piena risoluzione attorno al massimo
    let center = best.0 * DECIM as i64;
    let (mf, rf) = (&mic[..n], &reference[..n]);
    let mut fine = (center, f32::MIN, 0f32);
    for l in center - DECIM as i64..=center + DECIM as i64 {
        let (c, norm) = corr_at(mf, rf, l);
        if c > fine.1 {
            fine = (l, c, norm);
        }
    }
    let corr = if fine.2 > 0.0 { (fine.1 / fine.2).clamp(0.0, 1.0) } else { 0.0 };
    (fine.0, corr)
}

/// Toglie dal microfono cio' che arriva dagli altoparlanti. Se non trova eco (cuffie, silenzio)
/// restituisce il microfono invariato.
pub fn cancel(mic: &[f32], reference: &[f32]) -> Vec<f32> {
    if mic.is_empty() || reference.len() < RATE / 2 {
        return mic.to_vec();
    }
    let (lag, corr) = estimate_delay(mic, reference);
    if corr < MIN_CORR {
        return mic.to_vec();
    }
    // allinea: l'eco principale cade a LEAD campioni dentro il filtro
    let aligned = shifted(reference, lag - LEAD as i64, mic.len());
    // guadagno altoparlante->microfono, per portare il riferimento alla stessa scala del microfono
    let exact = shifted(reference, lag, mic.len()); // senza anticipo: l'eco coincide col microfono
    let e = dot(&exact, &exact);
    if e <= 1e-9 {
        return mic.to_vec();
    }
    let gain = (dot(mic, &exact) / e).clamp(0.02, 50.0);
    let x: Vec<f32> = aligned.iter().map(|v| v * gain).collect();
    // passata 1: il filtro impara; passata 2: si applica il filtro finale a tutto, dall'inizio
    let w = train(mic, &x);
    apply(mic, &x, &w)
}

/// NLMS con protezione "voce sopra voce": non adatta quando il microfono e' molto piu' forte del riferimento.
fn train(d: &[f32], x: &[f32]) -> Vec<f32> {
    const MU: f32 = 0.1; // passo piccolo: la voce dell'utente disturba poco il filtro
    const EPOCHS: usize = 3; // si ripassa l'audio piu' volte (e' gia' registrato), cosi' converge comunque
    const EPS: f32 = 1e-3;
    let mut w = vec![0f32; TAPS];
    for _ in 0..EPOCHS {
        adapt(d, x, &mut w, MU, EPS);
    }
    w
}

fn adapt(d: &[f32], x: &[f32], w: &mut [f32], mu: f32, eps: f32) {
    let mut pow = 0f32; // energia delle ultime TAPS campioni di x
    let mut peak = 0f32;
    // scala tipica dell'errore: parte dal livello del microfono e scende quando il filtro converge
    let head = &d[..d.len().min(RATE)];
    let mut sigma = (head.iter().map(|v| v * v).sum::<f32>() / head.len().max(1) as f32).sqrt().max(1e-4);
    for n in 0..d.len() {
        pow += x[n] * x[n];
        if n >= TAPS {
            pow -= x[n - TAPS] * x[n - TAPS];
        }
        peak = peak.max(x[n].abs()).max(peak * 0.9998);
        if n + 1 < TAPS {
            continue;
        }
        // finestra x[n], x[n-1], ... x[n-TAPS+1] contro w[0..TAPS]
        let win = &x[n + 1 - TAPS..=n];
        let y: f32 = win.iter().rev().zip(&*w).map(|(a, b)| a * b).sum();
        let err = d[n] - y;
        if peak < 1e-4 {
            continue; // riferimento muto: niente da imparare
        }
        // voce sopra l'eco: un errore molto piu' grande del solito e' la voce dell'utente, non un difetto del filtro, quindi si limita
        let lim = 2.0 * sigma;
        let err_c = err.clamp(-lim, lim);
        sigma = (sigma * 0.9995 + err_c.abs() * 1.25 * 0.0005).max(1e-4);
        let step = mu * err_c / (pow.max(0.0) + eps);
        for (wi, xi) in w.iter_mut().zip(win.iter().rev()) {
            *wi += step * xi;
        }
    }
}

fn apply(d: &[f32], x: &[f32], w: &[f32]) -> Vec<f32> {
    (0..d.len())
        .map(|n| {
            if n + 1 < TAPS {
                let k = n + 1;
                let y: f32 = x[..k].iter().rev().zip(w).map(|(a, b)| a * b).sum();
                d[n] - y
            } else {
                let y: f32 = x[n + 1 - TAPS..=n].iter().rev().zip(w).map(|(a, b)| a * b).sum();
                d[n] - y
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rumore deterministico (LCG), colorato con un po' di filtro per somigliare a un segnale audio.
    fn noise(len: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        let mut prev = 0f32;
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                let v = (s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                prev = 0.6 * prev + 0.4 * v;
                prev
            })
            .collect()
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32
    }

    /// Eco sintetico: riferimento ritardato, attenuato e con un riflesso.
    fn echo_of(r: &[f32], delay: usize, gain: f32) -> Vec<f32> {
        (0..r.len())
            .map(|n| {
                let a = if n >= delay { r[n - delay] } else { 0.0 };
                let b = if n >= delay + 90 { r[n - delay - 90] } else { 0.0 };
                gain * (a + 0.4 * b)
            })
            .collect()
    }

    #[test]
    fn pads_gaps_with_silence() {
        let mut b = vec![1.0; 100];
        pad_to_elapsed(&mut b, 1000, Duration::from_millis(250));
        assert_eq!(b.len(), 250);
        assert!(b[100..].iter().all(|v| *v == 0.0));
        pad_to_elapsed(&mut b, 1000, Duration::from_millis(100)); // mai accorciare
        assert_eq!(b.len(), 250);
    }

    #[test]
    fn finds_the_delay() {
        let r = noise(RATE * 4, 7);
        let mic = echo_of(&r, 1234, 0.5);
        let (lag, corr) = estimate_delay(&mic, &r);
        assert!((lag - 1234).abs() <= 2, "lag {lag}");
        assert!(corr > 0.5, "corr {corr}");
    }

    #[test]
    fn removes_the_echo() {
        let r = noise(RATE * 6, 11);
        let mic = echo_of(&r, 900, 0.6);
        let out = cancel(&mic, &r);
        let tail = RATE * 2; // dopo la convergenza
        let before = energy(&mic[tail..]);
        let after = energy(&out[tail..]);
        assert!(after < before * 0.03, "eco residuo {after} su {before}"); // almeno -15 dB
    }

    #[test]
    fn keeps_the_voice_while_removing_the_echo() {
        let r = noise(RATE * 6, 3);
        let echo = echo_of(&r, 700, 0.5);
        // "voce": tono che compare solo dopo 3 s
        let voice: Vec<f32> = (0..r.len())
            .map(|n| if n > RATE * 3 { 0.15 * (n as f32 * 0.05).sin() } else { 0.0 })
            .collect();
        let mic: Vec<f32> = echo.iter().zip(&voice).map(|(a, b)| a + b).collect();
        let out = cancel(&mic, &r);
        let (s, e) = (RATE * 4, RATE * 6);
        let residual: Vec<f32> = out[s..e].iter().zip(&voice[s..e]).map(|(a, b)| a - b).collect();
        assert!(energy(&residual) < energy(&echo[s..e]) * 0.2);
        assert!(energy(&out[s..e]) > energy(&voice[s..e]) * 0.5, "la voce non deve sparire");
    }

    #[test]
    fn no_echo_leaves_the_microphone_untouched() {
        let mic = noise(RATE * 3, 5);
        let unrelated = noise(RATE * 3, 99);
        let out = cancel(&mic, &unrelated);
        assert_eq!(out, mic);
        // riferimento muto (cuffie): invariato
        assert_eq!(cancel(&mic, &vec![0.0; RATE * 3]), mic);
        // registrazione troppo corta
        assert_eq!(cancel(&mic[..100], &unrelated[..100]), mic[..100].to_vec());
    }
}
