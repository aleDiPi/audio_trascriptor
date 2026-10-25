//! Scorciatoia da testo ("ctrl+win") a combinazione di tasti. Indipendente dalla piattaforma.

#[derive(Clone, Copy, Default, Debug)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: Option<u32>,
}

pub fn parse(s: &str) -> Result<Combo, String> {
    let mut c = Combo::default();
    for part in s.split('+').map(|p| p.trim().to_lowercase()) {
        match part.as_str() {
            "ctrl" | "control" => c.ctrl = true,
            "alt" => c.alt = true,
            "shift" => c.shift = true,
            "win" | "windows" | "super" => c.win = true,
            "space" => c.key = Some(0x20),
            "tab" => c.key = Some(0x09),
            "enter" | "invio" => c.key = Some(0x0D),
            "esc" => c.key = Some(0x1B),
            p if p.len() == 1 && p.as_bytes()[0].is_ascii_alphanumeric() => {
                c.key = Some(p.as_bytes()[0].to_ascii_uppercase() as u32)
            }
            p if p.starts_with('f') && p[1..].parse::<u32>().map_or(false, |n| (1..=24).contains(&n)) => {
                c.key = Some(0x70 + p[1..].parse::<u32>().unwrap() - 1)
            }
            other => return Err(format!("tasto sconosciuto: '{other}'")),
        }
    }
    if !(c.ctrl || c.alt || c.shift || c.win) && c.key.is_none() {
        return Err("scorciatoia vuota".into());
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ctrl_win() {
        let c = parse("ctrl+win").unwrap();
        assert!(c.ctrl && c.win && !c.alt && !c.shift);
        assert_eq!(c.key, None);
    }

    #[test]
    fn modifiers_and_key_case_and_spaces() {
        let c = parse(" Ctrl + ALT + Space ").unwrap();
        assert!(c.ctrl && c.alt && !c.win);
        assert_eq!(c.key, Some(0x20));
    }

    #[test]
    fn letters_digits_and_function_keys() {
        assert_eq!(parse("ctrl+shift+a").unwrap().key, Some(b'A' as u32));
        assert_eq!(parse("alt+7").unwrap().key, Some(b'7' as u32));
        assert_eq!(parse("f9").unwrap().key, Some(0x78));
        assert_eq!(parse("f24").unwrap().key, Some(0x87));
    }

    #[test]
    fn rejects_invalid() {
        assert!(parse("ctrl+banana").is_err());
        assert!(parse("f25").is_err());
        assert!(parse("").is_err());
    }
}
