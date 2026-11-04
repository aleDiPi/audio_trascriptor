//! Suoni brevi all'inizio e alla fine della registrazione. Sono toni sintetizzati in memoria (nessun file da
//! distribuire) e riprodotti in modo asincrono (Windows: PlaySound; Linux: paplay/pw-play/aplay). Il suono d'inizio viene scartato dalla registrazione
//! (`audio::start` salta i primi millisecondi) e quello di fine parte quando il microfono e' gia' chiuso.

pub const RATE: u32 = 22_050;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Start,
    End,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// due note morbide, sale all'inizio e scende alla fine
    Soft,
    /// un colpetto brevissimo
    Click,
    /// due note con coda piu' lunga
    Chime,
}

pub const STYLES: [Style; 3] = [Style::Soft, Style::Click, Style::Chime];

impl Style {
    pub fn id(self) -> &'static str {
        match self {
            Style::Soft => "soft",
            Style::Click => "click",
            Style::Chime => "chime",
        }
    }

    /// Nome sconosciuto o vuoto: `Soft`.
    pub fn parse(s: &str) -> Style {
        STYLES.into_iter().find(|x| x.id() == s.trim().to_ascii_lowercase()).unwrap_or(Style::Soft)
    }
}

/// (frequenza in Hz, durata in ms) di ogni nota.
fn notes(kind: Kind, style: Style) -> Vec<(f32, u32)> {
    let (a, b) = match style {
        Style::Soft => ((660.0, 70), (880.0, 90)),
        Style::Chime => ((988.0, 80), (1319.0, 150)),
        Style::Click => return vec![(if kind == Kind::Start { 1400.0 } else { 1000.0 }, 25)],
    };
    if kind == Kind::Start {
        vec![a, b]
    } else {
        vec![(b.0, a.1), (a.0, b.1)]
    }
}

pub fn duration_ms(kind: Kind, style: Style) -> u32 {
    notes(kind, style).iter().map(|n| n.1).sum()
}

/// Campioni a 16 bit. `volume` 0-100 (oltre 100 vale 100); al massimo il segnale arriva a circa il 60% del fondo scala.
pub fn samples(kind: Kind, style: Style, volume: u8) -> Vec<i16> {
    let amp = volume.min(100) as f32 / 100.0 * 0.6 * i16::MAX as f32;
    let mut out = Vec::new();
    for (freq, ms) in notes(kind, style) {
        let n = (RATE as u64 * ms as u64 / 1000) as usize;
        let attack = (RATE as usize * 4 / 1000).max(1);
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let rise = (i as f32 / attack as f32).min(1.0);
            let fall = (1.0 - i as f32 / n as f32).powf(1.5); // a zero alla fine: nessun "clic" tra le note
            out.push((amp * rise * fall * (2.0 * std::f32::consts::PI * freq * t).sin()) as i16);
        }
    }
    out
}

/// File WAV completo (mono, 16 bit) da dare a `PlaySound`.
pub fn wav(kind: Kind, style: Style, volume: u8) -> Vec<u8> {
    let pcm = samples(kind, style, volume);
    let data_len = (pcm.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + pcm.len() * 2);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&RATE.to_le_bytes());
    v.extend_from_slice(&(RATE * 2).to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

#[cfg(windows)]
mod player {
    use super::*;
    use std::ptr::null_mut;
    use std::sync::Mutex;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

    /// Il buffer deve restare valido mentre Windows lo suona: si tiene l'ultimo.
    static CURRENT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

    /// Suona (senza aspettare) con stile e volume dati, a prescindere dall'impostazione "suoni attivi".
    pub fn play_with(kind: Kind, style: Style, volume: u8) {
        if volume == 0 {
            return;
        }
        let buf = wav(kind, style, volume);
        let mut cur = CURRENT.lock().unwrap();
        // PlaySound interrompe il suono precedente prima di partire con il nuovo buffer
        unsafe { PlaySoundW(buf.as_ptr() as *const u16, null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
        *cur = buf;
    }

    /// Suona se i suoni sono attivi nelle impostazioni. Restituisce la durata in ms (0 se non suona).
    pub fn play(kind: Kind) -> u32 {
        if !crate::config::sounds_enabled() || crate::config::sound_volume() == 0 {
            return 0;
        }
        let style = Style::parse(&crate::config::sound_style());
        play_with(kind, style, crate::config::sound_volume());
        duration_ms(kind, style)
    }
}

#[cfg(target_os = "linux")]
mod player {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    /// Programmi per suonare un file WAV, nell'ordine di preferenza, con i loro argomenti.
    const PLAYERS: [(&str, &[&str]); 3] = [("paplay", &[]), ("pw-play", &[]), ("aplay", &["-q"])];

    /// Primo programma dell'elenco presente in una variabile PATH.
    pub fn find_player(path_var: &str) -> Option<(PathBuf, &'static [&'static str])> {
        PLAYERS.iter().find_map(|(name, args)| {
            std::env::split_paths(path_var).map(|d| d.join(name)).find(|p| p.is_file()).map(|p| (p, *args))
        })
    }

    fn wav_path(kind: Kind) -> PathBuf {
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        dir.join(format!("trascrittore-{}.wav", if kind == Kind::Start { "inizio" } else { "fine" }))
    }

    fn spawn(player: &Path, args: &[&str], file: &Path) {
        if let Ok(mut child) = Command::new(player).args(args).arg(file).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }

    /// Suona (senza aspettare) con stile e volume dati, a prescindere dall'impostazione "suoni attivi".
    pub fn play_with(kind: Kind, style: Style, volume: u8) {
        if volume == 0 {
            return;
        }
        let Some((player, args)) = std::env::var("PATH").ok().and_then(|p| find_player(&p)) else { return };
        let file = wav_path(kind);
        if std::fs::write(&file, wav(kind, style, volume)).is_ok() {
            spawn(&player, args, &file);
        }
    }

    /// Suona se i suoni sono attivi nelle impostazioni. Restituisce la durata in ms (0 se non suona).
    pub fn play(kind: Kind) -> u32 {
        if !crate::config::sounds_enabled() || crate::config::sound_volume() == 0 {
            return 0;
        }
        let style = Style::parse(&crate::config::sound_style());
        play_with(kind, style, crate::config::sound_volume());
        duration_ms(kind, style)
    }
}

#[cfg(desktop)]
pub use player::{play, play_with};


#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::player::find_player;

    #[test]
    fn player_is_looked_up_in_path() {
        let dir = std::env::temp_dir().join(format!("trascrittore-test-{}-player", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(find_player(dir.to_str().unwrap()).is_none());
        std::fs::write(dir.join("aplay"), "").unwrap();
        let (p, args) = find_player(dir.to_str().unwrap()).unwrap();
        assert!(p.ends_with("aplay") && args == ["-q"]);
        std::fs::write(dir.join("paplay"), "").unwrap(); // paplay ha la precedenza
        assert!(find_player(dir.to_str().unwrap()).unwrap().0.ends_with("paplay"));
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(s: &[i16]) -> i32 {
        s.iter().map(|x| (*x as i32).abs()).max().unwrap_or(0)
    }

    #[test]
    fn wav_header_and_length_match_the_duration() {
        for style in STYLES {
            for kind in [Kind::Start, Kind::End] {
                let w = wav(kind, style, 50);
                assert_eq!(&w[0..4], b"RIFF");
                assert_eq!(&w[8..16], b"WAVEfmt ");
                assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), RATE);
                let data = u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize;
                assert_eq!(data, w.len() - 44);
                assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize, w.len() - 8);
                let ms = data / 2 * 1000 / RATE as usize;
                assert!((ms as i64 - duration_ms(kind, style) as i64).abs() <= 2, "{style:?} {kind:?}: {ms} ms");
                assert!(duration_ms(kind, style) <= 250, "i suoni devono restare brevi");
            }
        }
    }

    #[test]
    fn volume_scales_the_signal_and_zero_is_silent() {
        let low = peak(&samples(Kind::Start, Style::Soft, 20));
        let high = peak(&samples(Kind::Start, Style::Soft, 100));
        assert!(low > 0 && high > low * 3);
        assert!(high < (i16::MAX as f32 * 0.62) as i32, "mai al fondo scala");
        assert_eq!(peak(&samples(Kind::Start, Style::Soft, 0)), 0);
        assert_eq!(samples(Kind::End, Style::Chime, 250), samples(Kind::End, Style::Chime, 100));
    }

    #[test]
    fn sounds_start_and_end_at_zero_to_avoid_clicks() {
        for style in STYLES {
            let s = samples(Kind::Start, style, 100);
            assert!(s[0].abs() < 300 && s[s.len() - 1].abs() < 300, "{style:?}");
        }
    }

    #[test]
    fn start_and_end_are_different_sounds() {
        assert_ne!(samples(Kind::Start, Style::Soft, 50), samples(Kind::End, Style::Soft, 50));
        assert_ne!(samples(Kind::Start, Style::Click, 50), samples(Kind::End, Style::Click, 50));
        // all'inizio la nota sale, alla fine scende
        let (s, e) = (notes(Kind::Start, Style::Chime), notes(Kind::End, Style::Chime));
        assert!(s[0].0 < s[1].0 && e[0].0 > e[1].0);
    }

    #[test]
    fn style_parsing_falls_back_to_soft() {
        assert_eq!(Style::parse(" Chime "), Style::Chime);
        assert_eq!(Style::parse("click"), Style::Click);
        assert_eq!(Style::parse(""), Style::Soft);
        assert_eq!(Style::parse("boh"), Style::Soft);
        for s in STYLES {
            assert_eq!(Style::parse(s.id()), s);
        }
    }
}
