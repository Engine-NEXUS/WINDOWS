//! Layout-independent key chords (shortcut brain, phase E1).
//!
//! A shortcut such as `Ctrl+T` is bound by applications to a **virtual key**, not to the character the
//! user's layout happens to print on that key. The legacy path sent `Key::Unicode('t')`, which on a
//! non-Latin layout or under an IME is not the key the application listens for (research doc 09 §1.4).
//! Chords here are therefore parsed to virtual-key codes, and `key_events` produces the exact
//! press/release sequence that is sent in ONE batched `SendInput` call (no inter-key sleeps).
//!
//! Pure and platform-independent: everything is unit-tested without touching the OS.

use std::fmt;

/// Modifier keys, in canonical send order (also the display order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

impl Modifier {
    /// Virtual-key code and whether the key needs `KEYEVENTF_EXTENDEDKEY`.
    fn vk(self) -> (u16, bool) {
        match self {
            Modifier::Ctrl => (0x11, false),  // VK_CONTROL
            Modifier::Alt => (0x12, false),   // VK_MENU
            Modifier::Shift => (0x10, false), // VK_SHIFT
            Modifier::Win => (0x5B, true),    // VK_LWIN
        }
    }

    fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Alt => "Alt",
            Modifier::Shift => "Shift",
            Modifier::Win => "Win",
        }
    }
}

/// One keyboard shortcut: modifiers held while a single main key is pressed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    pub mods: Vec<Modifier>,
    /// Virtual-key code of the main key.
    pub vk: u16,
    /// Main key needs the extended-key flag (arrows, Home/End, PageUp/Down, Insert/Delete).
    pub extended: bool,
    label: String,
}

impl Chord {
    /// Human-readable form, e.g. `Ctrl+Shift+T` (used in logs and in spoken "what's the shortcut").
    pub fn display(&self) -> String {
        let mut parts: Vec<&str> = self.mods.iter().map(|m| m.name()).collect();
        parts.push(self.label.as_str());
        parts.join("+")
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

/// One low-level key event of a chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub vk: u16,
    pub extended: bool,
    pub up: bool,
}

/// The exact event sequence for a chord: modifiers down (canonical order), key down, key up,
/// modifiers up (reverse order). Sent as one `SendInput` batch.
pub fn key_events(chord: &Chord) -> Vec<KeyEvent> {
    let mut ev = Vec::with_capacity(chord.mods.len() * 2 + 2);
    for m in &chord.mods {
        let (vk, extended) = m.vk();
        ev.push(KeyEvent { vk, extended, up: false });
    }
    ev.push(KeyEvent { vk: chord.vk, extended: chord.extended, up: false });
    ev.push(KeyEvent { vk: chord.vk, extended: chord.extended, up: true });
    for m in chord.mods.iter().rev() {
        let (vk, extended) = m.vk();
        ev.push(KeyEvent { vk, extended, up: true });
    }
    ev
}

/// Parse `Ctrl+Shift+T`, `Alt+Left`, `Ctrl+=`, `F5`, `Ctrl+PageDown`… (case-insensitive).
/// The **last** token is the main key; every earlier token must be a modifier.
pub fn parse_chord(text: &str) -> Result<Chord, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("empty chord".into());
    }
    // A literal '+' main key is written "Ctrl+Plus" (a bare trailing '+' would split ambiguously).
    let tokens: Vec<&str> = text.split('+').map(|t| t.trim()).collect();
    if tokens.iter().any(|t| t.is_empty()) {
        return Err(format!("malformed chord '{text}' (use 'Plus' for the + key)"));
    }
    let (key_tok, mod_toks) = tokens.split_last().ok_or_else(|| "empty chord".to_string())?;

    let mut mods: Vec<Modifier> = Vec::new();
    for t in mod_toks {
        let m = match t.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Modifier::Ctrl,
            "alt" | "option" => Modifier::Alt,
            "shift" => Modifier::Shift,
            "win" | "windows" | "meta" | "super" | "cmd" => Modifier::Win,
            other => return Err(format!("'{other}' is not a modifier in '{text}'")),
        };
        if mods.contains(&m) {
            return Err(format!("duplicate modifier '{}' in '{text}'", m.name()));
        }
        mods.push(m);
    }
    mods.sort();

    let (vk, extended, label) = parse_key(key_tok)
        .ok_or_else(|| format!("unknown key '{key_tok}' in '{text}'"))?;
    Ok(Chord { mods, vk, extended, label })
}

/// Main-key table → (virtual-key code, extended flag, canonical label).
fn parse_key(tok: &str) -> Option<(u16, bool, String)> {
    let t = tok.trim();
    let lower = t.to_ascii_lowercase();

    // Single letter / digit.
    if t.chars().count() == 1 {
        let c = t.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            let up = c.to_ascii_uppercase();
            return Some((up as u16, false, up.to_string())); // VK_A..VK_Z == ASCII
        }
        if c.is_ascii_digit() {
            return Some((c as u16, false, c.to_string())); // VK_0..VK_9 == ASCII
        }
        // Punctuation keys by their unshifted glyph (US-layout label; the VK is what matters).
        let punct = match c {
            '=' => Some(0xBB), // VK_OEM_PLUS (the "=/+" key)
            '-' => Some(0xBD), // VK_OEM_MINUS
            ',' => Some(0xBC), // VK_OEM_COMMA
            '.' => Some(0xBE), // VK_OEM_PERIOD
            '/' => Some(0xBF), // VK_OEM_2
            ';' => Some(0xBA), // VK_OEM_1
            '`' => Some(0xC0), // VK_OEM_3
            '[' => Some(0xDB), // VK_OEM_4
            '\\' => Some(0xDC), // VK_OEM_5
            ']' => Some(0xDD), // VK_OEM_6
            '\'' => Some(0xDE), // VK_OEM_7
            _ => None,
        };
        if let Some(vk) = punct {
            return Some((vk, false, c.to_string()));
        }
    }

    // Function keys F1..F24.
    if let Some(n) = lower.strip_prefix('f').and_then(|r| r.parse::<u16>().ok()) {
        if (1..=24).contains(&n) {
            return Some((0x6F + n, false, format!("F{n}"))); // VK_F1 = 0x70
        }
    }

    // Named keys. (vk, extended, label)
    let named: Option<(u16, bool, &str)> = match lower.as_str() {
        "enter" | "return" => Some((0x0D, false, "Enter")),
        "esc" | "escape" => Some((0x1B, false, "Esc")),
        "tab" => Some((0x09, false, "Tab")),
        "space" | "spacebar" => Some((0x20, false, "Space")),
        "backspace" => Some((0x08, false, "Backspace")),
        "delete" | "del" => Some((0x2E, true, "Delete")),
        "insert" | "ins" => Some((0x2D, true, "Insert")),
        "home" => Some((0x24, true, "Home")),
        "end" => Some((0x23, true, "End")),
        "pageup" | "pgup" => Some((0x21, true, "PageUp")),
        "pagedown" | "pgdn" => Some((0x22, true, "PageDown")),
        "left" => Some((0x25, true, "Left")),
        "up" => Some((0x26, true, "Up")),
        "right" => Some((0x27, true, "Right")),
        "down" => Some((0x28, true, "Down")),
        "plus" => Some((0xBB, false, "Plus")),
        "minus" => Some((0xBD, false, "Minus")),
        "numadd" => Some((0x6B, false, "NumAdd")),
        "numsubtract" | "numsub" => Some((0x6D, false, "NumSubtract")),
        _ => None,
    };
    named.map(|(vk, ext, label)| (vk, ext, label.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_chords_to_virtual_keys() {
        let c = parse_chord("Ctrl+T").unwrap();
        assert_eq!((c.vk, c.extended), (0x54, false));
        assert_eq!(c.display(), "Ctrl+T");

        let c = parse_chord("ctrl+shift+t").unwrap();
        assert_eq!(c.mods, vec![Modifier::Ctrl, Modifier::Shift]);
        assert_eq!(c.display(), "Ctrl+Shift+T");

        let c = parse_chord("Alt+Left").unwrap();
        assert_eq!((c.vk, c.extended), (0x25, true), "arrows are extended keys");

        let c = parse_chord("F5").unwrap();
        assert_eq!((c.vk, c.mods.len()), (0x74, 0));

        let c = parse_chord("Ctrl+PageDown").unwrap();
        assert_eq!((c.vk, c.extended), (0x22, true));

        let c = parse_chord("Ctrl+=").unwrap();
        assert_eq!(c.vk, 0xBB, "zoom-in key is VK_OEM_PLUS, not the '=' character");
        assert_eq!(parse_chord("Ctrl+Plus").unwrap().vk, 0xBB);
        assert_eq!(parse_chord("Ctrl+-").unwrap().vk, 0xBD);
        assert_eq!(parse_chord("Ctrl+9").unwrap().vk, 0x39);
    }

    #[test]
    fn modifier_order_is_canonical_regardless_of_input_order() {
        let a = parse_chord("Shift+Ctrl+T").unwrap();
        let b = parse_chord("Ctrl+Shift+T").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.display(), "Ctrl+Shift+T");
    }

    #[test]
    fn rejects_malformed_chords() {
        for bad in ["", "Ctrl+", "+T", "Ctrl+Ctrl+T", "Foo+T", "Ctrl+Nonsense", "Ctrl++"] {
            assert!(parse_chord(bad).is_err(), "should reject '{bad}'");
        }
        // A modifier is never a main key.
        assert!(parse_chord("Ctrl+Shift").is_err());
    }

    #[test]
    fn event_sequence_presses_mods_then_key_then_releases_in_reverse() {
        let ev = key_events(&parse_chord("Ctrl+Shift+T").unwrap());
        let seq: Vec<(u16, bool)> = ev.iter().map(|e| (e.vk, e.up)).collect();
        assert_eq!(
            seq,
            vec![
                (0x11, false), // Ctrl down
                (0x10, false), // Shift down
                (0x54, false), // T down
                (0x54, true),  // T up
                (0x10, true),  // Shift up
                (0x11, true),  // Ctrl up
            ]
        );
        // A bare key is just down/up.
        assert_eq!(key_events(&parse_chord("F5").unwrap()).len(), 2);
    }

    #[test]
    fn extended_flag_follows_the_main_key_only() {
        let ev = key_events(&parse_chord("Alt+Left").unwrap());
        let by_vk = |vk| ev.iter().find(|e| e.vk == vk).unwrap().extended;
        assert!(by_vk(0x25), "Left is extended");
        assert!(!by_vk(0x12), "Alt is not");
    }

    #[test]
    fn all_f_keys_and_digits_map() {
        for n in 1..=24u16 {
            assert_eq!(parse_chord(&format!("F{n}")).unwrap().vk, 0x6F + n);
        }
        for d in 0..=9u16 {
            assert_eq!(parse_chord(&format!("Ctrl+{d}")).unwrap().vk, 0x30 + d);
        }
    }

    #[test]
    fn modifier_parsing_is_case_insensitive_and_accepts_aliases() {
        // Equivalent casings and spacing for Ctrl+Shift+T.
        let variants = [
            "ctrl+shift+t",
            "Ctrl+Shift+T",
            "CTRL+SHIFT+T",
            "control + shift + t",
            "CONTROL + SHIFT + T",
            "  Ctrl   +   Shift  +  T  ",
        ];
        let expected = Chord {
            mods: vec![Modifier::Ctrl, Modifier::Shift],
            vk: 0x54,
            extended: false,
            label: "T".into(),
        };
        for v in variants {
            let parsed = parse_chord(v).expect(v);
            assert_eq!(parsed, expected, "variant '{v}' should parse identically");
            assert_eq!(parsed.display(), "Ctrl+Shift+T");
        }

        // Alt aliases: "alt" and "option".
        let alt = parse_chord("Alt+Left").unwrap();
        let opt = parse_chord("option+left").unwrap();
        assert_eq!(alt, opt);
        assert_eq!(alt.mods, vec![Modifier::Alt]);

        // Windows key aliases: "win", "windows", "meta", "super", "cmd".
        for alias in ["win", "windows", "meta", "super", "cmd"] {
            let c = parse_chord(&format!("{alias}+r")).unwrap();
            assert_eq!(c.mods, vec![Modifier::Win]);
            assert_eq!(c.vk, 0x52);
            assert_eq!(c.display(), "Win+R");
        }
    }

    #[test]
    fn virtual_key_codes_used_for_letters_and_digits_independent_of_layout() {
        // ASCII letters map to VK_A..VK_Z (0x41..0x5A), not characters or localized keycodes.
        for (idx, ch) in ('a'..='z').enumerate() {
            let upper = ch.to_ascii_uppercase();
            let c_lower = parse_chord(&format!("Ctrl+{ch}")).unwrap();
            let c_upper = parse_chord(&format!("Ctrl+{upper}")).unwrap();
            assert_eq!(c_lower, c_upper, "lower '{ch}' and upper '{upper}' must match");
            let expected_vk = 0x41 + idx as u16;
            assert_eq!(c_lower.vk, expected_vk, "letter '{ch}' VK");
            assert!(!c_lower.extended, "letters are never extended keys");
        }

        // Digits map to VK_0..VK_9 (0x30..0x39).
        for d in 0..=9u16 {
            let c = parse_chord(&format!("Alt+{d}")).unwrap();
            assert_eq!(c.vk, 0x30 + d, "digit {d} VK");
            assert!(!c.extended, "digits are never extended keys");
        }
    }

    #[test]
    fn unknown_key_names_and_invalid_modifiers_rejected() {
        // Unknown key names fail with an informative error rather than being silently ignored.
        for bad_key in ["UnknownKey", "FooBar", "NoSuchKey", "F0", "F25", "F100"] {
            let err = parse_chord(&format!("Ctrl+{bad_key}")).unwrap_err();
            assert!(
                err.contains(&format!("unknown key '{bad_key}'")),
                "expected unknown key error for '{bad_key}', got: {err}"
            );
        }

        // Invalid modifier names fail.
        for bad_mod in ["Bogus", "Fake", "Invalid", "CommandX"] {
            let err = parse_chord(&format!("{bad_mod}+T")).unwrap_err();
            assert!(
                err.contains("is not a modifier"),
                "expected invalid modifier error for '{bad_mod}', got: {err}"
            );
        }

        // Duplicate modifiers are rejected.
        assert!(parse_chord("Ctrl+Control+A").unwrap_err().contains("duplicate modifier 'Ctrl'"));
        assert!(parse_chord("Alt+Option+B").unwrap_err().contains("duplicate modifier 'Alt'"));
        assert!(parse_chord("Win+Super+C").unwrap_err().contains("duplicate modifier 'Win'"));

        // Malformed structural chords are rejected.
        assert!(parse_chord("").is_err());
        assert!(parse_chord("   ").is_err());
        assert!(parse_chord("Ctrl+").is_err());
        assert!(parse_chord("+Enter").is_err());
        assert!(parse_chord("Ctrl+Tab+").is_err());
        assert!(parse_chord("Ctrl++").is_err());
    }

    #[test]
    fn navigation_editing_and_control_keys_map_correct_vks_and_flags() {
        // Extended navigation keys: arrows, Home, End, PageUp, PageDown, Delete, Insert.
        let extended_cases: &[(&str, u16, &str)] = &[
            ("Left", 0x25, "Left"),
            ("Up", 0x26, "Up"),
            ("Right", 0x27, "Right"),
            ("Down", 0x28, "Down"),
            ("Home", 0x24, "Home"),
            ("End", 0x23, "End"),
            ("PageUp", 0x21, "PageUp"),
            ("PgUp", 0x21, "PageUp"),
            ("PageDown", 0x22, "PageDown"),
            ("PgDn", 0x22, "PageDown"),
            ("Delete", 0x2E, "Delete"),
            ("Del", 0x2E, "Delete"),
            ("Insert", 0x2D, "Insert"),
            ("Ins", 0x2D, "Insert"),
        ];
        for &(name, expected_vk, canonical_label) in extended_cases {
            let c = parse_chord(name).unwrap();
            assert_eq!(c.vk, expected_vk, "VK for {name}");
            assert!(c.extended, "{name} must have extended flag true");
            assert_eq!(c.display(), canonical_label);

            // Case-insensitivity.
            let lower = parse_chord(&name.to_ascii_lowercase()).unwrap();
            assert_eq!(lower.vk, expected_vk);
            assert!(lower.extended);
        }

        // Non-extended control keys: Tab, Esc, Enter, Space, Backspace.
        let standard_cases: &[(&str, u16, &str)] = &[
            ("Tab", 0x09, "Tab"),
            ("Esc", 0x1B, "Esc"),
            ("Escape", 0x1B, "Esc"),
            ("Enter", 0x0D, "Enter"),
            ("Return", 0x0D, "Enter"),
            ("Space", 0x20, "Space"),
            ("Spacebar", 0x20, "Space"),
            ("Backspace", 0x08, "Backspace"),
        ];
        for &(name, expected_vk, canonical_label) in standard_cases {
            let c = parse_chord(name).unwrap();
            assert_eq!(c.vk, expected_vk, "VK for {name}");
            assert!(!c.extended, "{name} must have extended flag false");
            assert_eq!(c.display(), canonical_label);

            let with_mod = parse_chord(&format!("Ctrl+{name}")).unwrap();
            assert_eq!(with_mod.vk, expected_vk);
            assert!(!with_mod.extended);
            assert_eq!(with_mod.display(), format!("Ctrl+{canonical_label}"));
        }
    }

    #[test]
    fn function_keys_f1_to_f24_full_range_and_boundaries() {
        for n in 1..=24u16 {
            // Lowercase "f{n}" and uppercase "F{n}".
            let lower = parse_chord(&format!("f{n}")).unwrap();
            let upper = parse_chord(&format!("F{n}")).unwrap();
            assert_eq!(lower, upper);
            assert_eq!(lower.vk, 0x6F + n, "VK for F{n}");
            assert!(!lower.extended, "F{n} is not extended");
            assert_eq!(lower.display(), format!("F{n}"));

            // With modifiers.
            let chord = parse_chord(&format!("Ctrl+Alt+F{n}")).unwrap();
            assert_eq!(chord.vk, 0x6F + n);
            assert_eq!(chord.display(), format!("Ctrl+Alt+F{n}"));
        }

        // Out-of-bounds function keys.
        assert!(parse_chord("F0").is_err());
        assert!(parse_chord("F25").is_err());
        assert!(parse_chord("Ctrl+F99").is_err());
    }

    #[test]
    fn punctuation_keys_and_arithmetic_aliases() {
        // Punctuation keys mapping to VK_OEM_* codes.
        let punct = [
            ("=", 0xBB),
            ("-", 0xBD),
            (",", 0xBC),
            (".", 0xBE),
            ("/", 0xBF),
            (";", 0xBA),
            ("`", 0xC0),
            ("[", 0xDB),
            ("\\", 0xDC),
            ("]", 0xDD),
            ("'", 0xDE),
        ];
        for (symbol, expected_vk) in punct {
            let c = parse_chord(&format!("Ctrl+{symbol}")).unwrap();
            assert_eq!(c.vk, expected_vk, "symbol '{symbol}'");
            assert!(!c.extended);
        }

        // Named arithmetic keys.
        assert_eq!(parse_chord("Plus").unwrap().vk, 0xBB);
        assert_eq!(parse_chord("Minus").unwrap().vk, 0xBD);
        assert_eq!(parse_chord("NumAdd").unwrap().vk, 0x6B);
        assert_eq!(parse_chord("NumSubtract").unwrap().vk, 0x6D);
        assert_eq!(parse_chord("NumSub").unwrap().vk, 0x6D);
    }
}
