//! Engine-level data-driven input: a reusable action/binding layer configured by an ini. The mapping
//! is generic infrastructure (not game logic) — the game just points it at a config file and queries
//! [`Action`]s; it never names raw keys. Bindings are read from `[Actions1]` + `[Actions2]` (primary +
//! alternate binds) and mouse look from `[Mouse]` Sensitivity/InvertY, matching the retail `Mercs2.ini`
//! format, so the player's own key/mouse map (and their edits) apply. Controller (`[Controller …]`) is
//! a follow-up (needs a gamepad backend); the action layer here is already the seam it will plug into.

use std::collections::{HashMap, HashSet};
use winit::keyboard::KeyCode;

/// A player action the game logic queries (a subset of the ini's actions — the ones we act on today).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    Forward,
    Backward,
    MoveLeft,
    MoveRight,
    Sprint,
    Walk,
    Jump,
    Crouch,
    Use,
    LookUp,
    LookDown,
    LookLeft,
    LookRight,
}

impl Action {
    /// The ini key name in `[Actions1]`/`[Actions2]`.
    fn ini_name(self) -> &'static str {
        match self {
            Action::Forward => "Forward",
            Action::Backward => "Backward",
            Action::MoveLeft => "MoveLeft",
            Action::MoveRight => "MoveRight",
            Action::Sprint => "Sprint",
            Action::Walk => "Walk",
            Action::Jump => "Jump",
            Action::Crouch => "Crouch",
            Action::Use => "Use",
            Action::LookUp => "LookUp",
            Action::LookDown => "LookDown",
            Action::LookLeft => "LookLeft",
            Action::LookRight => "LookRight",
        }
    }
    const ALL: [Action; 13] = [
        Action::Forward, Action::Backward, Action::MoveLeft, Action::MoveRight, Action::Sprint,
        Action::Walk, Action::Jump, Action::Crouch, Action::Use, Action::LookUp, Action::LookDown,
        Action::LookLeft, Action::LookRight,
    ];
}

/// A resolved binding for an action (a keyboard key or a mouse button).
#[derive(Clone, Copy, PartialEq)]
enum Bind {
    Key(KeyCode),
    Mouse(winit::event::MouseButton),
}

/// Parsed `Mercs2.ini` bindings + mouse tuning.
pub struct Bindings {
    binds: HashMap<Action, Vec<Bind>>,
    pub invert_y: bool,
    /// Radians of look per pixel of mouse motion (derived from `[Mouse] Sensitivity`, 1–20).
    pub mouse_rad_per_px: f32,
}

impl Default for Bindings {
    /// Retail defaults (used if `Mercs2.ini` is missing/unreadable).
    fn default() -> Self {
        let mut binds: HashMap<Action, Vec<Bind>> = HashMap::new();
        let mut put = |a: Action, s: &str| {
            if let Some(b) = parse_bind(s) {
                binds.entry(a).or_default().push(b);
            }
        };
        put(Action::Forward, "W");
        put(Action::Backward, "S");
        put(Action::MoveLeft, "A");
        put(Action::MoveRight, "D");
        put(Action::Sprint, "LSHIFT");
        put(Action::Walk, "LCTRL");
        put(Action::Jump, "SPACE");
        put(Action::Crouch, "C");
        put(Action::Use, "E");
        Bindings { binds, invert_y: false, mouse_rad_per_px: sensitivity_to_rad(9.0) }
    }
}

impl Bindings {
    /// Load from a `Mercs2.ini` path; falls back to [`Bindings::default`] on any problem.
    pub fn load(path: &std::path::Path) -> Bindings {
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("[input] Mercs2.ini not found at {} — using retail defaults", path.display());
            return Bindings::default();
        };
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut cur = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                cur = name.to_string();
            } else if let Some((k, v)) = line.split_once('=') {
                sections.entry(cur.clone()).or_default().insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        let mut binds: HashMap<Action, Vec<Bind>> = HashMap::new();
        for sect in ["Actions1", "Actions2"] {
            if let Some(map) = sections.get(sect) {
                for a in Action::ALL {
                    if let Some(v) = map.get(a.ini_name()) {
                        if let Some(b) = parse_bind(v) {
                            binds.entry(a).or_default().push(b);
                        }
                    }
                }
            }
        }
        let mouse = sections.get("Mouse");
        let invert_y = mouse.and_then(|m| m.get("InvertY")).map(|v| v.trim() != "0").unwrap_or(false);
        let sens = mouse.and_then(|m| m.get("Sensitivity")).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(9.0);
        if binds.is_empty() {
            eprintln!("[input] Mercs2.ini had no [Actions*] binds — using retail defaults");
            return Bindings::default();
        }
        println!("[input] loaded Mercs2.ini bindings ({} actions bound, sensitivity {sens}, invertY {invert_y})", binds.len());
        Bindings { binds, invert_y, mouse_rad_per_px: sensitivity_to_rad(sens) }
    }
}

/// Live input snapshot: the held keyboard keys + mouse buttons this frame. `held(action)` resolves
/// through the bindings so game code never names raw keys.
pub struct Input<'a> {
    pub bindings: &'a Bindings,
    pub keys: &'a HashSet<KeyCode>,
    pub mouse: &'a HashSet<winit::event::MouseButton>,
}

impl Input<'_> {
    pub fn held(&self, a: Action) -> bool {
        self.bindings.binds.get(&a).is_some_and(|bs| {
            bs.iter().any(|b| match b {
                Bind::Key(k) => self.keys.contains(k),
                Bind::Mouse(m) => self.mouse.contains(m),
            })
        })
    }
}

/// `[Mouse] Sensitivity` (1–20, default 9) → radians of look per pixel of motion. 9 maps to the
/// engine's prior hardcoded 0.0008 rad/px so default feel is unchanged.
fn sensitivity_to_rad(s: f32) -> f32 {
    0.0008 * (s.clamp(1.0, 20.0) / 9.0)
}

/// Parse an ini binding token (e.g. `W`, `SPACE`, `LSHIFT`, `MOUSE1`) into a [`Bind`]. `NULL`/`EMPTY`
/// and unknown tokens → None (unbound).
fn parse_bind(s: &str) -> Option<Bind> {
    let s = s.trim().to_ascii_uppercase();
    match s.as_str() {
        "NULL" | "EMPTY" | "" => None,
        "MOUSE1" => Some(Bind::Mouse(winit::event::MouseButton::Left)),
        "MOUSE2" => Some(Bind::Mouse(winit::event::MouseButton::Right)),
        "MOUSE3" => Some(Bind::Mouse(winit::event::MouseButton::Middle)),
        _ => key_from_name(&s).map(Bind::Key),
    }
}

/// Map an ini key name to a winit [`KeyCode`].
fn key_from_name(s: &str) -> Option<KeyCode> {
    use KeyCode::*;
    // Single letter A–Z → KeyA..KeyZ.
    if s.len() == 1 {
        let c = s.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Some(match c {
                'A' => KeyA, 'B' => KeyB, 'C' => KeyC, 'D' => KeyD, 'E' => KeyE, 'F' => KeyF,
                'G' => KeyG, 'H' => KeyH, 'I' => KeyI, 'J' => KeyJ, 'K' => KeyK, 'L' => KeyL,
                'M' => KeyM, 'N' => KeyN, 'O' => KeyO, 'P' => KeyP, 'Q' => KeyQ, 'R' => KeyR,
                'S' => KeyS, 'T' => KeyT, 'U' => KeyU, 'V' => KeyV, 'W' => KeyW, 'X' => KeyX,
                'Y' => KeyY, 'Z' => KeyZ, _ => return None,
            });
        }
        if c.is_ascii_digit() {
            return Some(match c {
                '0' => Digit0, '1' => Digit1, '2' => Digit2, '3' => Digit3, '4' => Digit4,
                '5' => Digit5, '6' => Digit6, '7' => Digit7, '8' => Digit8, '9' => Digit9,
                _ => return None,
            });
        }
    }
    Some(match s {
        "SPACE" => Space,
        "TAB" => Tab,
        "LSHIFT" => ShiftLeft,
        "RSHIFT" => ShiftRight,
        "LCTRL" => ControlLeft,
        "RCTRL" => ControlRight,
        "LALT" => AltLeft,
        "RALT" => AltRight,
        "ENTER" | "RETURN" => Enter,
        "ESC" | "ESCAPE" => Escape,
        "UP" => ArrowUp,
        "DOWN" => ArrowDown,
        "LEFT" => ArrowLeft,
        "RIGHT" => ArrowRight,
        _ => return None,
    })
}

/// Locate `Mercs2.ini` next to the game install (sibling of the `data/` dir that holds `vz.wad`).
pub fn find_mercs2_ini() -> Option<std::path::PathBuf> {
    let vz = crate::wad::registry_vz_wad()?;
    // …/<game>/data/vz.wad → …/<game>/Mercs2.ini
    let p = std::path::Path::new(&vz).parent()?.parent()?.join("Mercs2.ini");
    p.exists().then_some(p)
}
