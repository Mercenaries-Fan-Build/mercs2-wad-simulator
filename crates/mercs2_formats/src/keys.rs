//! Key names and binding specs, shared by the mods that read a key from a config file (the Lua bridge's
//! `[OnKey]` bindings, the profiler's record hotkey).
//!
//! A binding spec is a key name with optional leading modifiers (`F5`, `ctrl+F5`, `shift+alt+k`,
//! `numpad3`). Names resolve to Windows virtual-key codes; the table below is plain data so it is testable
//! on any host, and on Windows every code is checked at compile time against `windows-sys`.

/// Windows virtual-key codes the name table resolves to.
pub mod vk {
    pub const XBUTTON1: u8 = 0x05;
    pub const XBUTTON2: u8 = 0x06;
    pub const BACK: u8 = 0x08;
    pub const TAB: u8 = 0x09;
    pub const RETURN: u8 = 0x0D;
    pub const SHIFT: u8 = 0x10;
    pub const CONTROL: u8 = 0x11;
    pub const MENU: u8 = 0x12;
    pub const PAUSE: u8 = 0x13;
    pub const CAPITAL: u8 = 0x14;
    pub const ESCAPE: u8 = 0x1B;
    pub const SPACE: u8 = 0x20;
    pub const PRIOR: u8 = 0x21;
    pub const NEXT: u8 = 0x22;
    pub const END: u8 = 0x23;
    pub const HOME: u8 = 0x24;
    pub const LEFT: u8 = 0x25;
    pub const UP: u8 = 0x26;
    pub const RIGHT: u8 = 0x27;
    pub const DOWN: u8 = 0x28;
    pub const SNAPSHOT: u8 = 0x2C;
    pub const INSERT: u8 = 0x2D;
    pub const DELETE: u8 = 0x2E;
    /// `'0'`; digits `0`..`9` are consecutive from here.
    pub const KEY_0: u8 = 0x30;
    /// `'A'`; letters `A`..`Z` are consecutive from here.
    pub const KEY_A: u8 = 0x41;
    /// Numpad digits `0`..`9` are consecutive from here.
    pub const NUMPAD0: u8 = 0x60;
    pub const MULTIPLY: u8 = 0x6A;
    pub const ADD: u8 = 0x6B;
    pub const SUBTRACT: u8 = 0x6D;
    pub const DECIMAL: u8 = 0x6E;
    pub const DIVIDE: u8 = 0x6F;
    /// F1; F1..F24 are consecutive from here.
    pub const F1: u8 = 0x70;
    pub const NUMLOCK: u8 = 0x90;
    pub const SCROLL: u8 = 0x91;
    pub const LSHIFT: u8 = 0xA0;
    pub const RSHIFT: u8 = 0xA1;
    pub const LCONTROL: u8 = 0xA2;
    pub const RCONTROL: u8 = 0xA3;
    pub const LMENU: u8 = 0xA4;
    pub const RMENU: u8 = 0xA5;
    pub const OEM_1: u8 = 0xBA;
    pub const OEM_PLUS: u8 = 0xBB;
    pub const OEM_COMMA: u8 = 0xBC;
    pub const OEM_MINUS: u8 = 0xBD;
    pub const OEM_PERIOD: u8 = 0xBE;
    pub const OEM_2: u8 = 0xBF;
    pub const OEM_3: u8 = 0xC0;
    pub const OEM_4: u8 = 0xDB;
    pub const OEM_5: u8 = 0xDC;
    pub const OEM_6: u8 = 0xDD;
    pub const OEM_7: u8 = 0xDE;
}

#[cfg(windows)]
const _: () = {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse as km;
    assert!(vk::XBUTTON1 as u16 == km::VK_XBUTTON1);
    assert!(vk::XBUTTON2 as u16 == km::VK_XBUTTON2);
    assert!(vk::BACK as u16 == km::VK_BACK);
    assert!(vk::TAB as u16 == km::VK_TAB);
    assert!(vk::RETURN as u16 == km::VK_RETURN);
    assert!(vk::SHIFT as u16 == km::VK_SHIFT);
    assert!(vk::CONTROL as u16 == km::VK_CONTROL);
    assert!(vk::MENU as u16 == km::VK_MENU);
    assert!(vk::PAUSE as u16 == km::VK_PAUSE);
    assert!(vk::CAPITAL as u16 == km::VK_CAPITAL);
    assert!(vk::ESCAPE as u16 == km::VK_ESCAPE);
    assert!(vk::SPACE as u16 == km::VK_SPACE);
    assert!(vk::PRIOR as u16 == km::VK_PRIOR);
    assert!(vk::NEXT as u16 == km::VK_NEXT);
    assert!(vk::END as u16 == km::VK_END);
    assert!(vk::HOME as u16 == km::VK_HOME);
    assert!(vk::LEFT as u16 == km::VK_LEFT);
    assert!(vk::UP as u16 == km::VK_UP);
    assert!(vk::RIGHT as u16 == km::VK_RIGHT);
    assert!(vk::DOWN as u16 == km::VK_DOWN);
    assert!(vk::SNAPSHOT as u16 == km::VK_SNAPSHOT);
    assert!(vk::INSERT as u16 == km::VK_INSERT);
    assert!(vk::DELETE as u16 == km::VK_DELETE);
    assert!(vk::KEY_0 as u16 == km::VK_0);
    assert!(vk::KEY_A as u16 == km::VK_A);
    assert!(vk::NUMPAD0 as u16 == km::VK_NUMPAD0);
    assert!(vk::MULTIPLY as u16 == km::VK_MULTIPLY);
    assert!(vk::ADD as u16 == km::VK_ADD);
    assert!(vk::SUBTRACT as u16 == km::VK_SUBTRACT);
    assert!(vk::DECIMAL as u16 == km::VK_DECIMAL);
    assert!(vk::DIVIDE as u16 == km::VK_DIVIDE);
    assert!(vk::F1 as u16 == km::VK_F1);
    assert!(vk::F1 as u16 + 23 == km::VK_F24);
    assert!(vk::NUMLOCK as u16 == km::VK_NUMLOCK);
    assert!(vk::SCROLL as u16 == km::VK_SCROLL);
    assert!(vk::LSHIFT as u16 == km::VK_LSHIFT);
    assert!(vk::RSHIFT as u16 == km::VK_RSHIFT);
    assert!(vk::LCONTROL as u16 == km::VK_LCONTROL);
    assert!(vk::RCONTROL as u16 == km::VK_RCONTROL);
    assert!(vk::LMENU as u16 == km::VK_LMENU);
    assert!(vk::RMENU as u16 == km::VK_RMENU);
    assert!(vk::OEM_1 as u16 == km::VK_OEM_1);
    assert!(vk::OEM_PLUS as u16 == km::VK_OEM_PLUS);
    assert!(vk::OEM_COMMA as u16 == km::VK_OEM_COMMA);
    assert!(vk::OEM_MINUS as u16 == km::VK_OEM_MINUS);
    assert!(vk::OEM_PERIOD as u16 == km::VK_OEM_PERIOD);
    assert!(vk::OEM_2 as u16 == km::VK_OEM_2);
    assert!(vk::OEM_3 as u16 == km::VK_OEM_3);
    assert!(vk::OEM_4 as u16 == km::VK_OEM_4);
    assert!(vk::OEM_5 as u16 == km::VK_OEM_5);
    assert!(vk::OEM_6 as u16 == km::VK_OEM_6);
    assert!(vk::OEM_7 as u16 == km::VK_OEM_7);
};

/// Whole-name entries, matched ASCII-case-insensitively after the letter/digit, numpad and
/// F-key rules in [`resolve_key_name`].
pub const NAMED_KEYS: &[(&str, u8)] = &[
    ("insert", vk::INSERT),
    ("delete", vk::DELETE),
    ("home", vk::HOME),
    ("end", vk::END),
    ("pageup", vk::PRIOR),
    ("pagedown", vk::NEXT),
    ("space", vk::SPACE),
    ("enter", vk::RETURN),
    ("return", vk::RETURN),
    ("escape", vk::ESCAPE),
    ("esc", vk::ESCAPE),
    ("backspace", vk::BACK),
    ("tab", vk::TAB),
    ("shift", vk::SHIFT),
    ("ctrl", vk::CONTROL),
    ("control", vk::CONTROL),
    ("alt", vk::MENU),
    ("left", vk::LEFT),
    ("up", vk::UP),
    ("right", vk::RIGHT),
    ("down", vk::DOWN),
    ("lshift", vk::LSHIFT),
    ("rshift", vk::RSHIFT),
    ("lctrl", vk::LCONTROL),
    ("rctrl", vk::RCONTROL),
    ("lalt", vk::LMENU),
    ("ralt", vk::RMENU),
    ("capslock", vk::CAPITAL),
    ("numlock", vk::NUMLOCK),
    ("scrolllock", vk::SCROLL),
    ("pause", vk::PAUSE),
    ("break", vk::PAUSE),
    ("printscreen", vk::SNAPSHOT),
    ("prtsc", vk::SNAPSHOT),
    ("tilde", vk::OEM_3),
    ("grave", vk::OEM_3),
    ("`", vk::OEM_3),
    ("minus", vk::OEM_MINUS),
    ("-", vk::OEM_MINUS),
    ("plus", vk::OEM_PLUS),
    ("=", vk::OEM_PLUS),
    ("equals", vk::OEM_PLUS),
    ("lbracket", vk::OEM_4),
    ("[", vk::OEM_4),
    ("rbracket", vk::OEM_6),
    ("]", vk::OEM_6),
    ("backslash", vk::OEM_5),
    ("\\", vk::OEM_5),
    ("semicolon", vk::OEM_1),
    (";", vk::OEM_1),
    ("quote", vk::OEM_7),
    ("'", vk::OEM_7),
    ("comma", vk::OEM_COMMA),
    (",", vk::OEM_COMMA),
    ("period", vk::OEM_PERIOD),
    (".", vk::OEM_PERIOD),
    ("slash", vk::OEM_2),
    ("/", vk::OEM_2),
    ("mouse4", vk::XBUTTON1),
    ("mouse5", vk::XBUTTON2),
];

/// Suffixes after `numpad` that name a non-digit numpad key.
pub const NUMPAD_NAMED_KEYS: &[(&str, u8)] = &[
    ("plus", vk::ADD),
    ("add", vk::ADD),
    ("minus", vk::SUBTRACT),
    ("subtract", vk::SUBTRACT),
    ("multiply", vk::MULTIPLY),
    ("star", vk::MULTIPLY),
    ("divide", vk::DIVIDE),
    ("slash", vk::DIVIDE),
    ("decimal", vk::DECIMAL),
    ("dot", vk::DECIMAL),
];

/// The name that means "bound to nothing" without being an error.
pub const UNASSIGNED: &str = "unassigned";

/// Resolve a bare key name to a virtual-key code. `None` for [`UNASSIGNED`] and for any name
/// the table does not know.
///
/// Rule order matters: a single letter or digit first, then anything starting with `numpad`
/// (which never falls through to the table), then `F` + 1..=24 written as one or two digits with
/// no leading zero and nothing after them, then [`NAMED_KEYS`]. Anything else is unrecognized.
pub fn resolve_key_name(name: &str) -> Option<u8> {
    if name.eq_ignore_ascii_case(UNASSIGNED) {
        return None;
    }
    let b = name.as_bytes();
    if b.len() == 1 {
        let c = b[0];
        if c.is_ascii_alphabetic() {
            return Some(vk::KEY_A + (c.to_ascii_uppercase() - b'A'));
        }
        if c.is_ascii_digit() {
            return Some(vk::KEY_0 + (c - b'0'));
        }
    }
    if b.len() >= 6 && b[..6].eq_ignore_ascii_case(b"numpad") {
        let s = &name[6..];
        let sb = s.as_bytes();
        if sb.len() == 1 && sb[0].is_ascii_digit() {
            return Some(vk::NUMPAD0 + (sb[0] - b'0'));
        }
        return lookup(NUMPAD_NAMED_KEYS, s);
    }
    if (b.len() == 2 || b.len() == 3)
        && (b[0] == b'f' || b[0] == b'F')
        && b[1] != b'0'
        && b[1..].iter().all(u8::is_ascii_digit)
    {
        let num: u8 = name[1..].parse().expect("one or two ASCII digits");
        return (1..=24).contains(&num).then(|| vk::F1 + (num - 1));
    }
    lookup(NAMED_KEYS, name)
}

fn lookup(table: &[(&str, u8)], name: &str) -> Option<u8> {
    table.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, v)| v)
}

/// A set of held modifiers, or the set a binding requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const CTRL: Mods = Mods(0x01);
    pub const SHIFT: Mods = Mods(0x02);
    pub const ALT: Mods = Mods(0x04);

    pub const fn bits(self) -> u8 {
        self.0
    }
    pub const fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }
    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn without(self, other: Mods) -> Mods {
        Mods(self.0 & !other.0)
    }

    /// The modifier a virtual key is itself part of (either-side, left or right variants), or
    /// [`Mods::NONE`] for an ordinary key.
    pub const fn of_key(code: u8) -> Mods {
        match code {
            vk::SHIFT | vk::LSHIFT | vk::RSHIFT => Mods::SHIFT,
            vk::CONTROL | vk::LCONTROL | vk::RCONTROL => Mods::CTRL,
            vk::MENU | vk::LMENU | vk::RMENU => Mods::ALT,
            _ => Mods::NONE,
        }
    }
}

/// A parsed binding: the base key (`None` when unassigned or unrecognized) and the modifiers
/// it requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySpec {
    pub vk: Option<u8>,
    pub mods: Mods,
}

/// Parse a binding spec. Surrounding spaces and tabs are trimmed (and trailing CR/LF). Leading
/// `ctrl+`/`control+`/`shift+`/`alt+` tokens, in any order and case, set modifiers; spaces
/// after each `+` are skipped. Parsing stops at the first token that is empty or not a
/// modifier, and the rest resolves with [`resolve_key_name`]. A bare modifier name is a base
/// key (`shift` is VK_SHIFT with no required modifiers).
pub fn resolve_key_spec(spec: &str) -> KeySpec {
    let mut p = spec.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t', '\r', '\n']);
    let mut mods = Mods::NONE;
    while let Some(plus) = p.find('+') {
        if plus == 0 {
            break;
        }
        let tok = &p[..plus];
        let m = if tok.eq_ignore_ascii_case("ctrl") || tok.eq_ignore_ascii_case("control") {
            Mods::CTRL
        } else if tok.eq_ignore_ascii_case("shift") {
            Mods::SHIFT
        } else if tok.eq_ignore_ascii_case("alt") {
            Mods::ALT
        } else {
            break;
        };
        mods = mods.union(m);
        p = p[plus + 1..].trim_start_matches(' ');
    }
    KeySpec { vk: resolve_key_name(p), mods }
}

/// Whether a `GetAsyncKeyState` result says the key is down now (high bit).
pub const fn async_state_is_down(state: i16) -> bool {
    (state as u16) & 0x8000 != 0
}

/// Whether the foreground window belongs to this process. `None` when there is no
/// foreground window.
pub fn focus_matches(foreground_pid: Option<u32>, own_pid: u32) -> bool {
    foreground_pid == Some(own_pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_letters_and_digits() {
        assert_eq!(resolve_key_name("a"), Some(0x41));
        assert_eq!(resolve_key_name("Z"), Some(0x5A));
        assert_eq!(resolve_key_name("0"), Some(0x30));
        assert_eq!(resolve_key_name("9"), Some(0x39));
    }

    #[test]
    fn unassigned_and_unknown_resolve_to_none() {
        assert_eq!(resolve_key_name("unassigned"), None);
        assert_eq!(resolve_key_name("UNASSIGNED"), None);
        assert_eq!(resolve_key_name(""), None);
        assert_eq!(resolve_key_name("pgup"), None);
        assert_eq!(resolve_key_name("+"), None);
    }

    #[test]
    fn numpad_rules_never_fall_through() {
        assert_eq!(resolve_key_name("numpad0"), Some(0x60));
        assert_eq!(resolve_key_name("NumPad9"), Some(0x69));
        assert_eq!(resolve_key_name("numpadplus"), Some(vk::ADD));
        assert_eq!(resolve_key_name("numpadADD"), Some(vk::ADD));
        assert_eq!(resolve_key_name("numpadminus"), Some(vk::SUBTRACT));
        assert_eq!(resolve_key_name("numpadstar"), Some(vk::MULTIPLY));
        assert_eq!(resolve_key_name("numpadslash"), Some(vk::DIVIDE));
        assert_eq!(resolve_key_name("numpaddot"), Some(vk::DECIMAL));
        assert_eq!(resolve_key_name("numpad"), None);
        assert_eq!(resolve_key_name("numpad10"), None);
        assert_eq!(resolve_key_name("numpadtab"), None);
    }

    #[test]
    fn function_keys_one_to_twenty_four() {
        assert_eq!(resolve_key_name("F1"), Some(0x70));
        assert_eq!(resolve_key_name("f12"), Some(0x7B));
        assert_eq!(resolve_key_name("F24"), Some(0x87));
        assert_eq!(resolve_key_name("F25"), None);
        assert_eq!(resolve_key_name("F0"), None);
        assert_eq!(resolve_key_name("F99999999999999999999"), None);
    }

    #[test]
    fn function_key_names_are_strict() {
        for name in ["F5x", "f1abc", "F05", "F0", "F00", "F25", "F99", "F123", "F-1", "F+5", "5a", "a5", "F 5"] {
            assert_eq!(resolve_key_name(name), None, "{name}");
        }
        assert_eq!(resolve_key_name("F"), Some(0x46), "a lone F is the letter key");
        assert_eq!(resolve_key_spec("ctrl+F5x"), KeySpec { vk: None, mods: Mods::CTRL });
    }

    #[test]
    fn named_keys_case_insensitive() {
        assert_eq!(resolve_key_name("Insert"), Some(vk::INSERT));
        assert_eq!(resolve_key_name("PAGEDOWN"), Some(vk::NEXT));
        assert_eq!(resolve_key_name("esc"), Some(vk::ESCAPE));
        assert_eq!(resolve_key_name("Tilde"), Some(vk::OEM_3));
        assert_eq!(resolve_key_name("`"), Some(vk::OEM_3));
        assert_eq!(resolve_key_name("-"), Some(vk::OEM_MINUS));
        assert_eq!(resolve_key_name("="), Some(vk::OEM_PLUS));
        assert_eq!(resolve_key_name("\\"), Some(vk::OEM_5));
        assert_eq!(resolve_key_name("mouse4"), Some(vk::XBUTTON1));
        assert_eq!(resolve_key_name("rctrl"), Some(vk::RCONTROL));
    }

    #[test]
    fn every_table_entry_resolves_to_itself() {
        for &(name, code) in NAMED_KEYS {
            assert_eq!(resolve_key_name(name), Some(code), "{name}");
        }
        for &(suffix, code) in NUMPAD_NAMED_KEYS {
            assert_eq!(resolve_key_name(&format!("numpad{suffix}")), Some(code), "{suffix}");
        }
    }

    #[test]
    fn table_sizes() {
        assert_eq!(NAMED_KEYS.len(), 60);
        assert_eq!(NUMPAD_NAMED_KEYS.len(), 10);
    }

    #[test]
    fn spec_modifiers_any_order_and_case() {
        assert_eq!(resolve_key_spec("ctrl+F5"), KeySpec { vk: Some(0x74), mods: Mods::CTRL });
        assert_eq!(
            resolve_key_spec("Shift+ALT+k"),
            KeySpec { vk: Some(0x4B), mods: Mods::SHIFT.union(Mods::ALT) }
        );
        assert_eq!(
            resolve_key_spec("alt+control+shift+1"),
            KeySpec { vk: Some(0x31), mods: Mods::CTRL.union(Mods::SHIFT).union(Mods::ALT) }
        );
    }

    #[test]
    fn spec_trimming_and_spaces_after_plus() {
        assert_eq!(resolve_key_spec(" \tctrl+  F5 \r\n"), KeySpec { vk: Some(0x74), mods: Mods::CTRL });
        // A space before the plus makes the token "ctrl " which is not a modifier.
        assert_eq!(resolve_key_spec("ctrl +F5"), KeySpec { vk: None, mods: Mods::NONE });
    }

    #[test]
    fn spec_bare_modifier_is_a_base_key() {
        assert_eq!(resolve_key_spec("shift"), KeySpec { vk: Some(vk::SHIFT), mods: Mods::NONE });
        assert_eq!(resolve_key_spec("ctrl+shift"), KeySpec { vk: Some(vk::SHIFT), mods: Mods::CTRL });
    }

    #[test]
    fn spec_empty_token_stops_parsing() {
        assert_eq!(resolve_key_spec("ctrl++"), KeySpec { vk: None, mods: Mods::CTRL });
        assert_eq!(resolve_key_spec("ctrl+"), KeySpec { vk: None, mods: Mods::CTRL });
        assert_eq!(resolve_key_spec("+F5"), KeySpec { vk: None, mods: Mods::NONE });
    }

    #[test]
    fn mods_of_modifier_keys() {
        assert_eq!(Mods::of_key(vk::LSHIFT), Mods::SHIFT);
        assert_eq!(Mods::of_key(vk::RCONTROL), Mods::CTRL);
        assert_eq!(Mods::of_key(vk::MENU), Mods::ALT);
        assert_eq!(Mods::of_key(vk::INSERT), Mods::NONE);
    }

    #[test]
    fn async_state_high_bit() {
        assert!(async_state_is_down(i16::MIN));
        assert!(async_state_is_down(-32767));
        assert!(!async_state_is_down(1));
        assert!(!async_state_is_down(0));
    }

    #[test]
    fn focus_is_a_pid_match() {
        assert!(focus_matches(Some(42), 42));
        assert!(!focus_matches(Some(7), 42));
        assert!(!focus_matches(None, 42));
    }
}
