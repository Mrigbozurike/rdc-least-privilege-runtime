//! Key-chord parsing shared by client validation and the daemon's input layer.
//!
//! Grammar: tokens separated by `+`; every token except the last is a modifier.
//! `cmd+shift+4`, `ctrl+c`, `alt+f4`, `enter`, `space`, `+` (a literal plus is written `plus`).

use crate::proto::{RdcError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// Cmd on macOS, Super/Win elsewhere.
    Meta,
    Control,
    Alt,
    Shift,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyName {
    Return,
    Escape,
    Tab,
    Space,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    CapsLock,
    F(u8),
    /// A modifier pressed on its own (e.g. `key "shift"`).
    Mod(Modifier),
    Char(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub mods: Vec<Modifier>,
    pub key: KeyName,
}

pub fn parse_modifier(tok: &str) -> Option<Modifier> {
    Some(match tok {
        "cmd" | "command" | "super" | "win" | "windows" | "meta" => Modifier::Meta,
        "ctrl" | "control" => Modifier::Control,
        "alt" | "option" | "opt" => Modifier::Alt,
        "shift" => Modifier::Shift,
        _ => return None,
    })
}

pub fn parse_key(tok: &str) -> Result<KeyName> {
    let lower = tok.to_ascii_lowercase();
    if let Some(m) = parse_modifier(&lower) {
        return Ok(KeyName::Mod(m));
    }
    let k = match lower.as_str() {
        "enter" | "return" => KeyName::Return,
        "esc" | "escape" => KeyName::Escape,
        "tab" => KeyName::Tab,
        "space" | "spc" => KeyName::Space,
        "backspace" | "bs" => KeyName::Backspace,
        "delete" | "del" => KeyName::Delete,
        "insert" | "ins" => KeyName::Insert,
        "home" => KeyName::Home,
        "end" => KeyName::End,
        "pageup" | "pgup" => KeyName::PageUp,
        "pagedown" | "pgdn" | "pgdown" => KeyName::PageDown,
        "up" | "uparrow" => KeyName::Up,
        "down" | "downarrow" => KeyName::Down,
        "left" | "leftarrow" => KeyName::Left,
        "right" | "rightarrow" => KeyName::Right,
        "capslock" => KeyName::CapsLock,
        "plus" => KeyName::Char('+'),
        "minus" => KeyName::Char('-'),
        "comma" => KeyName::Char(','),
        "period" | "dot" => KeyName::Char('.'),
        "slash" => KeyName::Char('/'),
        "backslash" => KeyName::Char('\\'),
        "semicolon" => KeyName::Char(';'),
        "quote" | "apostrophe" => KeyName::Char('\''),
        "grave" | "backtick" => KeyName::Char('`'),
        "equal" | "equals" => KeyName::Char('='),
        "bracketleft" | "lbracket" => KeyName::Char('['),
        "bracketright" | "rbracket" => KeyName::Char(']'),
        f if f.starts_with('f') && f.len() <= 3 && f[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => {
            KeyName::F(f[1..].parse().unwrap())
        }
        _ => {
            let mut chars = tok.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyName::Char(c),
                _ => return Err(RdcError::BadRequest(format!("unknown key {tok:?}"))),
            }
        }
    };
    Ok(k)
}

pub fn parse_chord(s: &str) -> Result<Chord> {
    let s = s.trim();
    if s.is_empty() {
        return Err(RdcError::BadRequest("empty key chord".into()));
    }
    // A bare "+" means the plus key.
    if s == "+" {
        return Ok(Chord { mods: vec![], key: KeyName::Char('+') });
    }
    let toks: Vec<&str> = s.split('+').map(str::trim).collect();
    let (last, mods) = toks.split_last().unwrap();
    let mut modifiers = Vec::new();
    for m in mods {
        let m = parse_modifier(&m.to_ascii_lowercase())
            .ok_or_else(|| RdcError::BadRequest(format!("{m:?} is not a modifier in {s:?}")))?;
        if !modifiers.contains(&m) {
            modifiers.push(m);
        }
    }
    // Trailing "+" like "ctrl+" → treat as plus key.
    let key = if last.is_empty() { KeyName::Char('+') } else { parse_key(last)? };
    Ok(Chord { mods: modifiers, key })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_chords() {
        let c = parse_chord("cmd+shift+4").unwrap();
        assert_eq!(c.mods, vec![Modifier::Meta, Modifier::Shift]);
        assert_eq!(c.key, KeyName::Char('4'));
        assert_eq!(parse_chord("Enter").unwrap().key, KeyName::Return);
        assert_eq!(parse_chord("ctrl+c").unwrap().key, KeyName::Char('c'));
        assert_eq!(parse_chord("F12").unwrap().key, KeyName::F(12));
        assert_eq!(parse_chord("ctrl+plus").unwrap().key, KeyName::Char('+'));
        assert_eq!(parse_chord("+").unwrap().key, KeyName::Char('+'));
        assert_eq!(parse_chord("shift").unwrap().key, KeyName::Mod(Modifier::Shift));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_chord("").is_err());
        assert!(parse_chord("foo+bar").is_err());
        assert!(parse_chord("ctrl+enterr").is_err());
    }
}
