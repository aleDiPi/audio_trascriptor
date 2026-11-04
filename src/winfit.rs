//! Fits the settings window to the available screen area (logical pixels, i.e. already divided by the DPI scale).

/// Space taken by title bar, panel/taskbar and a safety margin, which the monitor size does not include.
const RESERVE_W: f64 = 24.0;
const RESERVE_H: f64 = 110.0;

/// Size the window should open at: the desired size, shrunk to fit the screen, never below `min`
/// (unless the screen itself is smaller, in which case the screen wins and the content scrolls).
pub fn fit(desired: (f64, f64), min: (f64, f64), screen: (f64, f64)) -> (f64, f64) {
    let avail = ((screen.0 - RESERVE_W).max(1.0), (screen.1 - RESERVE_H).max(1.0));
    let one = |d: f64, m: f64, a: f64| d.min(a).max(m.min(a));
    (one(desired.0, min.0, avail.0), one(desired.1, min.1, avail.1))
}

/// Comfortable size for a screen: a share of it (60% x 75%), between the default size and a cap,
/// so big screens do not get a huge window and small ones are handled by `fit`.
pub fn preferred(screen: (f64, f64)) -> (f64, f64) {
    ((screen.0 * 0.6).clamp(900.0, 1150.0), (screen.1 * 0.75).clamp(640.0, 820.0))
}

/// Monitor size in logical pixels from the physical size and the DPI scale (invalid scale counts as 1).
pub fn logical(physical: (u32, u32), scale: f64) -> (f64, f64) {
    let s = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    (physical.0 as f64 / s, physical.1 as f64 / s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESIRED: (f64, f64) = (900.0, 640.0);
    const MIN: (f64, f64) = (560.0, 380.0);

    #[test]
    fn big_screen_keeps_desired_size() {
        assert_eq!(fit(DESIRED, MIN, (1920.0, 1080.0)), DESIRED);
    }

    #[test]
    fn small_screen_shrinks_window() {
        let (w, h) = fit(DESIRED, MIN, (1280.0, 720.0));
        assert_eq!(w, 900.0);
        assert!(h <= 720.0 - RESERVE_H && h >= MIN.1);
    }

    #[test]
    fn tiny_screen_goes_below_min() {
        let (w, h) = fit(DESIRED, MIN, (500.0, 400.0));
        assert!(w <= 500.0 && h <= 400.0);
    }

    #[test]
    fn scaling_reduces_logical_screen() {
        // 1920x1080 at 200% is 960x540 logical
        assert_eq!(logical((1920, 1080), 2.0), (960.0, 540.0));
        let (_, h) = fit(DESIRED, MIN, logical((1920, 1080), 2.0));
        assert!(h < 540.0);
    }

    #[test]
    fn preferred_is_proportional_and_capped() {
        assert_eq!(preferred((1920.0, 1080.0)), (1150.0, 810.0));
        assert_eq!(preferred((1366.0, 768.0)), (900.0, 640.0));
        assert_eq!(preferred((3840.0, 2160.0)), (1150.0, 820.0));
    }

    #[test]
    fn preferred_then_fit_never_fills_screen() {
        for s in [(1366.0, 768.0), (1920.0, 1080.0), (1280.0, 720.0), (2560.0, 1440.0)] {
            let (w, h) = fit(preferred(s), MIN, s);
            assert!(w < s.0 && h < s.1, "{s:?} -> {w}x{h}");
        }
    }

    #[test]
    fn invalid_scale_is_one() {
        assert_eq!(logical((800, 600), 0.0), (800.0, 600.0));
        assert_eq!(logical((800, 600), f64::NAN), (800.0, 600.0));
    }
}
