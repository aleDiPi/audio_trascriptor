//! Disegno del bollino a schermo in memoria (pixel ARGB premoltiplicati): alone che segue il volume, disco scuro e
//! icona dell'app di destinazione. Parte pura e testabile; la finestra X11 che lo mostra e' in overlay_linux.rs.

pub const SIZE: usize = 160;
pub const DISC_R: f32 = 31.0;
pub const HALO_MIN: f32 = 3.0;
pub const HALO_MAX: f32 = 44.0;
pub const ICON_PX: usize = 40;

/// Immagine non premoltiplicata, un pixel per u32 = 0xAARRGGBB.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub w: usize,
    pub h: usize,
    pub argb: Vec<u32>,
}

/// Sceglie fra le immagini di `_NET_WM_ICON` (sequenza di `larghezza, altezza, pixel...`) la piu' piccola che sia
/// almeno `want` pixel (altrimenti la piu' grande). Dati incoerenti o troncati: ignorati.
pub fn pick_icon(data: &[u32], want: usize) -> Option<Icon> {
    let mut best: Option<Icon> = None;
    let mut i = 0;
    while i + 2 <= data.len() {
        let (w, h) = (data[i] as usize, data[i + 1] as usize);
        i += 2;
        let n = w.checked_mul(h)?;
        if w == 0 || h == 0 || w > 1024 || h > 1024 || i + n > data.len() {
            break;
        }
        let side = w.max(h);
        let better = match &best {
            None => true,
            Some(b) => {
                let bs = b.w.max(b.h);
                if bs >= want { side >= want && side < bs } else { side > bs }
            }
        };
        if better {
            best = Some(Icon { w, h, argb: data[i..i + n].to_vec() });
        }
        i += n;
    }
    best
}

/// Ridimensiona (media sui pixel coperti) in un quadrato `px` x `px` mantenendo le proporzioni, centrata.
pub fn fit(icon: &Icon, px: usize) -> Vec<u32> {
    let mut out = vec![0u32; px * px];
    let scale = px as f32 / icon.w.max(icon.h) as f32;
    let (dw, dh) = (((icon.w as f32 * scale).round() as usize).clamp(1, px), ((icon.h as f32 * scale).round() as usize).clamp(1, px));
    let (ox, oy) = ((px - dw) / 2, (px - dh) / 2);
    for y in 0..dh {
        for x in 0..dw {
            let (x0, x1) = (x * icon.w / dw, ((x + 1) * icon.w / dw).max(x * icon.w / dw + 1).min(icon.w));
            let (y0, y1) = (y * icon.h / dh, ((y + 1) * icon.h / dh).max(y * icon.h / dh + 1).min(icon.h));
            let (mut a, mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = icon.argb[sy * icon.w + sx];
                    let pa = p >> 24;
                    a += pa;
                    r += ((p >> 16) & 255) * pa;
                    g += ((p >> 8) & 255) * pa;
                    b += (p & 255) * pa;
                    n += 1;
                }
            }
            if n == 0 || a == 0 {
                continue;
            }
            let (ra, rr, rg, rb) = (a / n, r / a, g / a, b / a);
            out[(y + oy) * px + x + ox] = (ra << 24) | (rr << 16) | (rg << 8) | rb;
        }
    }
    out
}

/// Distanza di ogni pixel dal centro del quadrato `SIZE`.
pub fn distances() -> Vec<f32> {
    let c = SIZE as f32 / 2.0 - 0.5;
    let mut v = Vec::with_capacity(SIZE * SIZE);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            v.push((dx * dx + dy * dy).sqrt());
        }
    }
    v
}

/// Un fotogramma completo in `px` (SIZE*SIZE). `halo` e' il raggio dell'alone, `recording` sceglie rosso o ambra,
/// `icon` e' gia' adattata a ICON_PX (vedi `fit`).
pub fn render(px: &mut [u32], dist: &[f32], halo: f32, recording: bool, icon: Option<&[u32]>) {
    let (r, g, b) = if recording { (235.0, 64.0, 64.0) } else { (240.0, 170.0, 40.0) };
    for (i, d) in dist.iter().enumerate() {
        let d = *d;
        let (mut a, mut cr, mut cg, mut cb) = (0.0f32, r, g, b);
        if d <= DISC_R + halo {
            let k = ((DISC_R + halo - d) / halo.max(1.0)).clamp(0.0, 1.0);
            a = 0.80 * k * k;
        }
        let disc = (DISC_R + 0.5 - d).clamp(0.0, 1.0);
        if disc > 0.0 {
            a = a * (1.0 - disc) + disc;
            cr = cr * (1.0 - disc) + 30.0 * disc;
            cg = cg * (1.0 - disc) + 30.0 * disc;
            cb = cb * (1.0 - disc) + 34.0 * disc;
        }
        let (pr, pg, pb) = ((cr * a) as u32, (cg * a) as u32, (cb * a) as u32);
        px[i] = (((a * 255.0) as u32) << 24) | (pr << 16) | (pg << 8) | pb;
    }
    if let Some(icon) = icon {
        let o = (SIZE - ICON_PX) / 2;
        for y in 0..ICON_PX {
            for x in 0..ICON_PX {
                let s = icon[y * ICON_PX + x];
                let sa = s >> 24;
                if sa == 0 {
                    continue;
                }
                let dst = &mut px[(y + o) * SIZE + x + o];
                let inv = 255 - sa;
                let ch = |sh: u32| (((s >> sh) & 255) * sa / 255) + ((*dst >> sh) & 255) * inv / 255;
                *dst = (((sa + (*dst >> 24) * inv / 255).min(255)) << 24) | (ch(16).min(255) << 16) | (ch(8).min(255) << 8) | ch(0).min(255);
            }
        }
    }
}

/// Posizione in basso a destra di un'area di lavoro (x, y, larghezza, altezza), con un margine.
pub fn corner(area: (i32, i32, i32, i32), margin: i32) -> (i32, i32) {
    let (x, y, w, h) = area;
    ((x + w - SIZE as i32 - margin).max(x), (y + h - SIZE as i32 - margin).max(y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_prefers_smallest_at_least_wanted() {
        let mut d = vec![16, 16];
        d.extend(vec![0xFF00_0001; 256]);
        d.extend([48, 48]);
        d.extend(vec![0xFF00_0002; 48 * 48]);
        d.extend([128, 128]);
        d.extend(vec![0xFF00_0003; 128 * 128]);
        assert_eq!(pick_icon(&d, 40).unwrap().w, 48);
        assert_eq!(pick_icon(&d, 200).unwrap().w, 128); // nessuna abbastanza grande: la maggiore
        assert_eq!(pick_icon(&d, 10).unwrap().w, 16);
    }

    #[test]
    fn pick_ignores_garbage() {
        assert!(pick_icon(&[], 40).is_none());
        assert!(pick_icon(&[4, 4, 1, 2], 40).is_none()); // troncata
        assert!(pick_icon(&[0, 0], 40).is_none());
        assert!(pick_icon(&[u32::MAX, u32::MAX], 40).is_none());
    }

    #[test]
    fn fit_keeps_aspect_and_colour() {
        let icon = Icon { w: 80, h: 40, argb: vec![0xFF10_2030; 80 * 40] };
        let out = fit(&icon, 40);
        assert_eq!(out.len(), 1600);
        assert_eq!(out[0], 0); // sopra l'immagine: vuoto (proporzioni 2:1)
        assert_eq!(out[20 * 40 + 20], 0xFF10_2030);
    }

    #[test]
    fn render_halo_grows_and_icon_is_drawn() {
        let dist = distances();
        let mut small = vec![0u32; SIZE * SIZE];
        let mut big = vec![0u32; SIZE * SIZE];
        render(&mut small, &dist, HALO_MIN, true, None);
        render(&mut big, &dist, HALO_MAX, true, None);
        let lit = |v: &[u32]| v.iter().filter(|p| **p >> 24 != 0).count();
        assert!(lit(&big) > lit(&small));
        assert_eq!(small[0], 0); // gli angoli restano trasparenti
        let c = SIZE / 2 * SIZE + SIZE / 2;
        assert_eq!(small[c] >> 24, 255); // il disco e' opaco
        let icon = vec![0xFFFF_FFFFu32; ICON_PX * ICON_PX];
        render(&mut small, &dist, HALO_MIN, true, Some(&icon));
        assert_eq!(small[c], 0xFFFF_FFFF);
    }

    #[test]
    fn corner_is_bottom_right_and_clamped() {
        assert_eq!(corner((0, 0, 1920, 1040), 12), (1920 - 160 - 12, 1040 - 160 - 12));
        assert_eq!(corner((0, 0, 100, 100), 12), (0, 0));
    }
}
