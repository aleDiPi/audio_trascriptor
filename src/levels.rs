//! Livello del microfono per l'alone dell'icona: calcolo del volume, conversione in raggio e smorzamento.
//! Parte pura (testabile): la finestra che lo disegna e' in overlay.rs.

use std::sync::atomic::{AtomicU32, Ordering};

/// Picco del volume (RMS) visto dall'ultima lettura. Per i numeri f32 non negativi l'ordine dei bit
/// coincide con l'ordine dei valori, quindi `fetch_max` sui bit tiene il massimo.
static PEAK_BITS: AtomicU32 = AtomicU32::new(0);

/// Chiamata dal flusso audio a ogni blocco di campioni.
pub fn report(rms: f32) {
    PEAK_BITS.fetch_max(rms.max(0.0).to_bits(), Ordering::Relaxed);
}

/// Letto dall'icona a ogni fotogramma: restituisce il picco e riparte da zero.
pub fn take_peak() -> f32 {
    f32::from_bits(PEAK_BITS.swap(0, Ordering::Relaxed))
}

#[cfg_attr(not(test), allow(dead_code))] // il flusso audio calcola l'energia da se'
/// Valore efficace (RMS) di un blocco di campioni in [-1, 1].
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Volume percepito da 0 a 1: scala in decibel da -54 dB (silenzio) a -8 dB (voce forte).
pub fn unit(rms: f32) -> f32 {
    let db = 20.0 * rms.max(1e-5).log10();
    ((db + 54.0) / 46.0).clamp(0.0, 1.0)
}

/// Raggio dell'alone: `min` col silenzio, `max` col volume massimo.
pub fn radius(unit: f32, min: f32, max: f32) -> f32 {
    min + (max - min) * unit.clamp(0.0, 1.0)
}

/// Smorzamento: sale in fretta quando il volume aumenta e scende piano, cosi' l'alone pulsa senza tremare.
pub struct Smoother {
    pub value: f32,
}

impl Smoother {
    const ATTACK: f32 = 0.5;
    const RELEASE: f32 = 0.1;

    pub fn new() -> Smoother {
        Smoother { value: 0.0 }
    }

    pub fn step(&mut self, target: f32) -> f32 {
        let k = if target > self.value { Self::ATTACK } else { Self::RELEASE };
        self.value += (target - self.value) * k;
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_blocks() {
        assert_eq!(rms(&[]), 0.0);
        assert!((rms(&[0.5, 0.5, 0.5]) - 0.5).abs() < 1e-6);
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6); // il segno non conta
        assert_eq!(rms(&[0.0; 8]), 0.0);
    }

    #[test]
    fn unit_scale_is_silent_to_loud_and_monotonic() {
        assert_eq!(unit(0.0), 0.0);
        assert_eq!(unit(0.001), 0.0); // -60 dB: rumore di fondo
        assert_eq!(unit(0.5), 1.0); // -6 dB: voce forte
        let (a, b, c) = (unit(0.005), unit(0.02), unit(0.1));
        assert!(0.0 < a && a < b && b < c && c < 1.0, "{a} {b} {c}");
    }

    #[test]
    fn radius_maps_unit_to_range() {
        assert_eq!(radius(0.0, 4.0, 44.0), 4.0);
        assert_eq!(radius(1.0, 4.0, 44.0), 44.0);
        assert_eq!(radius(0.5, 4.0, 44.0), 24.0);
        assert_eq!(radius(2.0, 4.0, 44.0), 44.0); // fuori scala: limitato
        assert_eq!(radius(-1.0, 4.0, 44.0), 4.0);
    }

    #[test]
    fn smoother_rises_fast_and_falls_slowly() {
        let mut s = Smoother::new();
        assert!((s.step(1.0) - 0.5).abs() < 1e-6); // salita rapida
        s.value = 1.0;
        assert!((s.step(0.0) - 0.9).abs() < 1e-6); // discesa lenta
        for _ in 0..200 {
            s.step(0.3);
        }
        assert!((s.value - 0.3).abs() < 1e-3);
        let mut s = Smoother::new();
        for _ in 0..50 {
            assert!(s.step(1.0) <= 1.0); // nessun superamento del bersaglio
        }
    }

    #[test]
    fn peak_is_max_since_last_read() {
        take_peak();
        report(0.1);
        report(0.5);
        report(0.2);
        assert_eq!(take_peak(), 0.5);
        assert_eq!(take_peak(), 0.0); // dopo la lettura riparte da zero
    }
}
