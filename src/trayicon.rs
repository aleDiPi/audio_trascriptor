//! Pixel dell'icona nella tray: stesso microfono bianco dell'icona del programma, su un fondo
//! che cambia colore con lo stato (blu pronto, rosso registra, ambra elabora, grigio errore).

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Idle,
    Recording,
    Working,
    Error,
}

pub fn color(tone: Tone) -> [u8; 3] {
    match tone {
        Tone::Idle => [37, 99, 235],
        Tone::Recording => [220, 40, 40],
        Tone::Working => [240, 160, 30],
        Tone::Error => [140, 140, 140],
    }
}

/// RGBA di un'icona quadrata `n` x `n`: disco colorato con il microfono bianco al centro.
pub fn pixels(tone: Tone, n: u32) -> Vec<u8> {
    let c = color(tone);
    let nf = n as f32;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            // coordinate normalizzate in [-1, 1]
            let u = ((x as f32 + 0.5) / nf) * 2.0 - 1.0;
            let v = ((y as f32 + 0.5) / nf) * 2.0 - 1.0;
            let rgba = if u * u + v * v > 0.94 * 0.94 {
                [0, 0, 0, 0]
            } else if mic(u, v) {
                [255, 255, 255, 255]
            } else {
                [c[0], c[1], c[2], 255]
            };
            px.extend_from_slice(&rgba);
        }
    }
    px
}

/// true se il punto (u, v) cade sul microfono: capsula, staffa a U, asta e base.
fn mic(u: f32, v: f32) -> bool {
    let capsule = u.abs() <= 0.22 && {
        let (top, bottom) = (-0.62, 0.12);
        if v < top + 0.22 {
            u * u + (v - (top + 0.22)).powi(2) <= 0.22 * 0.22
        } else if v > bottom - 0.22 {
            u * u + (v - (bottom - 0.22)).powi(2) <= 0.22 * 0.22
        } else {
            true
        }
    };
    let r = (u * u + (v - (-0.05)).powi(2)).sqrt();
    let cradle = v >= -0.05 && (0.34..=0.46).contains(&r);
    let stem = u.abs() <= 0.06 && (0.40..=0.66).contains(&v);
    let base = u.abs() <= 0.26 && (0.62..=0.72).contains(&v);
    capsule || cradle || stem || base
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(px: &[u8], n: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * n + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    #[test]
    fn corners_are_transparent_and_background_is_state_colour() {
        for tone in [Tone::Idle, Tone::Recording, Tone::Working, Tone::Error] {
            let px = pixels(tone, 32);
            assert_eq!(px.len(), 32 * 32 * 4);
            assert_eq!(at(&px, 32, 0, 0)[3], 0);
            let c = color(tone);
            assert_eq!(at(&px, 32, 5, 16), [c[0], c[1], c[2], 255]);
        }
    }

    #[test]
    fn microphone_capsule_is_white_in_the_middle() {
        let px = pixels(Tone::Recording, 32);
        assert_eq!(at(&px, 32, 16, 10), [255, 255, 255, 255]);
    }

    #[test]
    fn states_have_distinct_colours() {
        let all = [Tone::Idle, Tone::Recording, Tone::Working, Tone::Error].map(color);
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                assert_ne!(all[i], all[j]);
            }
        }
    }
}
