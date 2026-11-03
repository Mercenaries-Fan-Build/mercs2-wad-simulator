//! Retail's keyboard and mouse vocabulary: one id per key, the DirectInput scan code for a key and
//! `0x100 +` for the mouse, named by the table at `0x00D1E640` (267 entries, ids `0..=0x10A`).
//!
//! `Mercs2.ini`'s `[Actions1]`/`[Actions2]` hold these names, read back with `_stricmp` over the whole
//! table (`FUN_004FB6B0`). The remap page shows a key through `FUN_004FA500`, which picks a table by
//! language (`0x00CF255C`): Spanish `0x00D24A80`, Italian `0x00D23170`, French `0x00D1FF50`, German
//! `0x00D21860`, every other language this one.

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

pub const NULL: u16 = 0x00;
pub const ESCAPE: u16 = 0x01;
pub const ENTER: u16 = 0x1C;
pub const SPACE: u16 = 0x39;
pub const MOUSE1: u16 = 0x100;
pub const MWUP: u16 = 0x108;
pub const MWDOWN: u16 = 0x109;
pub const MWHEEL: u16 = 0x10A;
/// How many ids the table has.
pub const COUNT: u16 = 0x10B;

/// `0x00D1E640`, by id.
pub const NAMES: [&str; COUNT as usize] = [
    "NULL", "ESCAPE", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "BACKSPACE", "TAB",
    "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]", "ENTER", "LCTRL", "A", "S",
    "D", "F", "G", "H", "J", "K", "L", ";", "'", "`", "LSHIFT", "\\", "Z", "X", "C", "V",
    "B", "N", "M", ",", ".", "/", "RSHIFT", "*", "LALT", "SPACE", "CAPSLOCK", "F1", "F2", "F3", "F4", "F5",
    "F6", "F7", "F8", "F9", "F10", "NUMLOCK", "SCROLLLOCK", "NUM7", "NUM8", "NUM9", "NUMMINUS", "NUM4", "NUM5", "NUM6", "NUMPLUS", "NUM1",
    "NUM2", "NUM3", "NUM0", "NUM.", ".54.", ".55", ".56.", "F11", "F12", ".59.", ".5A.", ".5B.", ".5C.", ".5D.", ".5E.", ".5F.",
    ".60.", ".61.", ".62.", ".63.", "F13", "F14", "F15", ".67.", ".68.", ".69.", ".6A.", ".6B.", ".6C.", ".6D.", ".6E.", ".6F.",
    "Kana", ".71.", ".72.", ".73.", ".74.", ".75.", ".76.", ".77.", ".78.", "C(JP)", ".7A.", "NC(JP)", ".7C.", "YEN(KP)", ".7E.", ".7F.",
    ".80.", ".81.", ".82.", ".83.", ".84.", ".85.", ".86.", ".87.", ".88.", ".89.", ".8A.", ".8B.", ".8C.", "NUMEQUAL", ".8E.", ".8F.",
    "PRVTRCK", "@NEC", ":NEC", "_NEC", "KANJI", "STOPNEC", "AX(JPAX)", ".97.", ".98.", "NEXTTRACK", ".9A.", ".9B.", "NUMENTER", "RCTRL", ".9E.", ".9F.",
    "MUTE", "CALC", "PLAY", ".A3.", "MSTOP", ".A5.", ".A6.", ".A7.", ".A8.", ".A9.", ".AA.", ".AB.", ".AC.", ".AD.", "VDOWN", ".AF.",
    "VUP", ".B1.", "WEBHOME", "NUMCOMMA", ".B4.", "NUMDIV", ".B6.", "PRINTSCREEN", "RALT", ".B9.", ".BA.", ".BB.", ".BC.", ".BD.", ".BE.", ".BF.",
    ".C0.", ".C1.", ".C2.", ".C3.", ".C4.", "PAUSE", ".C6.", "HOME", "UP", "PGUP", ".CA.", "LEFT", ".CC.", "RIGHT", ".CE.", "END",
    "DOWN", "PAGEDOWN", "INSERT", "DELETE", ".D4.", ".D5.", ".D6.", ".D7.", ".D8.", ".D9.", ".DA.", "LWIN", "RWIN", "APPMENU", "POWER", "SLEEP",
    ".E0.", ".E1.", ".E2.", "WAKE", ".E4.", "SEARCH", "FAVORITES", "REFRESH", "STOP", "FORWARD", "BACKUP", "MYCOMPUTER", "MAIL", "MEDIA", ".EE.", ".EF.",
    ".F0.", ".F1.", ".F2.", ".F3.", ".F4.", ".F5.", ".F6.", ".F7.", ".F8.", ".F9.", ".FA.", ".FB.", ".FC.", ".FD.", ".FE.", ".FF.",
    "MOUSE1", "MOUSE2", "MOUSE3", "MOUSE4", "MOUSE5", "MOUSE6", "MOUSE7", "MOUSE8", "MWUP", "MWDOWN", "MWHEEL",
];

/// The id an INI value names: the first entry `_stricmp` matches, as `FUN_004FB6B0` searches.
pub fn id(name: &str) -> Option<u16> {
    NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| i as u16)
}

/// `FUN_004FA500`: what the remap page shows for an id, the string key `[0x1033c821]` (`0x00BB30F0`)
/// outside `1..=0x10A`.
pub fn display(id: u16) -> &'static str {
    match id {
        1..=MWHEEL => NAMES[id as usize],
        _ => "[0x1033c821]",
    }
}

/// The id DirectInput reports a key as: its scan code (`dinput.h` `DIK_*`). A key DirectInput has no
/// code for is none.
pub fn of_key(code: KeyCode) -> Option<u16> {
    use KeyCode::*;
    Some(match code {
        Escape => 0x01,
        Digit1 => 0x02,
        Digit2 => 0x03,
        Digit3 => 0x04,
        Digit4 => 0x05,
        Digit5 => 0x06,
        Digit6 => 0x07,
        Digit7 => 0x08,
        Digit8 => 0x09,
        Digit9 => 0x0A,
        Digit0 => 0x0B,
        Minus => 0x0C,
        Equal => 0x0D,
        Backspace => 0x0E,
        Tab => 0x0F,
        KeyQ => 0x10,
        KeyW => 0x11,
        KeyE => 0x12,
        KeyR => 0x13,
        KeyT => 0x14,
        KeyY => 0x15,
        KeyU => 0x16,
        KeyI => 0x17,
        KeyO => 0x18,
        KeyP => 0x19,
        BracketLeft => 0x1A,
        BracketRight => 0x1B,
        Enter => 0x1C,
        ControlLeft => 0x1D,
        KeyA => 0x1E,
        KeyS => 0x1F,
        KeyD => 0x20,
        KeyF => 0x21,
        KeyG => 0x22,
        KeyH => 0x23,
        KeyJ => 0x24,
        KeyK => 0x25,
        KeyL => 0x26,
        Semicolon => 0x27,
        Quote => 0x28,
        Backquote => 0x29,
        ShiftLeft => 0x2A,
        Backslash => 0x2B,
        KeyZ => 0x2C,
        KeyX => 0x2D,
        KeyC => 0x2E,
        KeyV => 0x2F,
        KeyB => 0x30,
        KeyN => 0x31,
        KeyM => 0x32,
        Comma => 0x33,
        Period => 0x34,
        Slash => 0x35,
        ShiftRight => 0x36,
        NumpadMultiply => 0x37,
        AltLeft => 0x38,
        Space => 0x39,
        CapsLock => 0x3A,
        F1 => 0x3B,
        F2 => 0x3C,
        F3 => 0x3D,
        F4 => 0x3E,
        F5 => 0x3F,
        F6 => 0x40,
        F7 => 0x41,
        F8 => 0x42,
        F9 => 0x43,
        F10 => 0x44,
        NumLock => 0x45,
        ScrollLock => 0x46,
        Numpad7 => 0x47,
        Numpad8 => 0x48,
        Numpad9 => 0x49,
        NumpadSubtract => 0x4A,
        Numpad4 => 0x4B,
        Numpad5 => 0x4C,
        Numpad6 => 0x4D,
        NumpadAdd => 0x4E,
        Numpad1 => 0x4F,
        Numpad2 => 0x50,
        Numpad3 => 0x51,
        Numpad0 => 0x52,
        NumpadDecimal => 0x53,
        IntlBackslash => 0x56,
        F11 => 0x57,
        F12 => 0x58,
        F13 => 0x64,
        F14 => 0x65,
        F15 => 0x66,
        KanaMode => 0x70,
        IntlRo => 0x73,
        Convert => 0x79,
        NonConvert => 0x7B,
        IntlYen => 0x7D,
        NumpadEqual => 0x8D,
        MediaTrackPrevious => 0x90,
        MediaTrackNext => 0x99,
        NumpadEnter => 0x9C,
        ControlRight => 0x9D,
        AudioVolumeMute => 0xA0,
        LaunchApp2 => 0xA1,
        MediaPlayPause => 0xA2,
        MediaStop => 0xA4,
        AudioVolumeDown => 0xAE,
        AudioVolumeUp => 0xB0,
        BrowserHome => 0xB2,
        NumpadComma => 0xB3,
        NumpadDivide => 0xB5,
        PrintScreen => 0xB7,
        AltRight => 0xB8,
        Pause => 0xC5,
        Home => 0xC7,
        ArrowUp => 0xC8,
        PageUp => 0xC9,
        ArrowLeft => 0xCB,
        ArrowRight => 0xCD,
        End => 0xCF,
        ArrowDown => 0xD0,
        PageDown => 0xD1,
        Insert => 0xD2,
        Delete => 0xD3,
        SuperLeft => 0xDB,
        SuperRight => 0xDC,
        ContextMenu => 0xDD,
        Power => 0xDE,
        Sleep => 0xDF,
        WakeUp => 0xE3,
        BrowserSearch => 0xE5,
        BrowserFavorites => 0xE6,
        BrowserRefresh => 0xE7,
        BrowserStop => 0xE8,
        BrowserForward => 0xE9,
        BrowserBack => 0xEA,
        LaunchApp1 => 0xEB,
        LaunchMail => 0xEC,
        MediaSelect => 0xED,
        _ => return None,
    })
}

/// The id of a mouse button: DirectInput's buttons 1..8 are `MOUSE1..MOUSE8`.
pub fn of_mouse(button: MouseButton) -> Option<u16> {
    Some(match button {
        MouseButton::Left => MOUSE1,
        MouseButton::Right => MOUSE1 + 1,
        MouseButton::Middle => MOUSE1 + 2,
        MouseButton::Back => MOUSE1 + 3,
        MouseButton::Forward => MOUSE1 + 4,
        MouseButton::Other(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_read_back_case_blind_to_their_ids() {
        assert_eq!((id("w"), id("LShift"), id("mouse1"), id("MWHEEL"), id("nothing")), (Some(0x11), Some(0x2A), Some(MOUSE1), Some(MWHEEL), None));
    }

    #[test]
    fn a_key_and_its_name_agree() {
        assert_eq!(NAMES[of_key(KeyCode::F1).unwrap() as usize], "F1");
        assert_eq!(NAMES[of_key(KeyCode::ArrowDown).unwrap() as usize], "DOWN");
        assert_eq!(NAMES[of_key(KeyCode::Backslash).unwrap() as usize], "\\");
        assert_eq!(NAMES[of_mouse(MouseButton::Middle).unwrap() as usize], "MOUSE3");
    }

    #[test]
    fn null_shows_as_its_string_key() {
        assert_eq!((display(NULL), display(0x11)), ("[0x1033c821]", "W"));
    }
}
