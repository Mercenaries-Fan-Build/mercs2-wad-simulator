//! Engine-level data-driven input: a reusable action/binding layer configured by an ini, covering
//! keyboard, mouse AND gamepad. The mapping is generic infrastructure (not game logic) — the game just
//! points it at a config file and queries [`Action`]s (or the analog [`Input::move_vec`]/
//! [`Input::look_delta`]); it never names raw keys or buttons.
//!
//! Bindings are read from the retail `Mercs2.ini` format:
//!  - `[Actions1]` + `[Actions2]` — primary + alternate keyboard/mouse binds.
//!  - `[Mouse]` — Sensitivity + InvertY.
//!  - `[Controller (XBOX 360 For Windows)]` — gamepad binds. Its `Button N` indices are the standard
//!    DirectInput enumeration of an Xbox pad (1=A,2=B,3=X,4=Y,5=LB,6=RB,7=Back,8=Start,9=L3,10=R3),
//!    which is exactly the layout `gilrs` normalises every controller to via SDL_GameControllerDB — so
//!    honouring this section works for any pad on Linux/SteamOS/Windows. `Left/Right Stick - Dir` →
//!    stick axes; `DPAD Dir` → d-pad; `Other Left Stick - L/R` → the triggers.
//!
//! Gamepad backend: [`gilrs`] (pure Rust; Linux evdev / Windows XInput+DInput; works on the Steam Deck).

use std::collections::{HashMap, HashSet};
use winit::keyboard::KeyCode;

/// Every player action the game can query (the full retail Mercs2 action set). Movement/look are also
/// available as analog via [`Input::move_vec`]/[`Input::look_delta`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    Forward,
    Backward,
    MoveLeft,
    MoveRight,
    PrimaryAttack,
    PrimarySwitch,
    Reload,
    SecondaryAttack,
    SecondarySwitch,
    MeleeAttack,
    Use,
    Jump,
    Crouch,
    Binoculars,
    Pda,
    LookUp,
    LookDown,
    LookLeft,
    LookRight,
    SelectUp,
    SelectDown,
    SelectRight,
    Walk,
    Sprint,
    Start,
}

impl Action {
    /// The ini key name in `[Actions*]` / `[Controller …]`.
    fn ini_name(self) -> &'static str {
        match self {
            Action::Forward => "Forward",
            Action::Backward => "Backward",
            Action::MoveLeft => "MoveLeft",
            Action::MoveRight => "MoveRight",
            Action::PrimaryAttack => "PrimaryAttack",
            Action::PrimarySwitch => "PrimarySwitch",
            Action::Reload => "Reload",
            Action::SecondaryAttack => "SecondaryAttack",
            Action::SecondarySwitch => "SecondarySwitch",
            Action::MeleeAttack => "MeleeAttack",
            Action::Use => "Use",
            Action::Jump => "Jump",
            Action::Crouch => "Crouch",
            Action::Binoculars => "Binoculars",
            Action::Pda => "PDA",
            Action::LookUp => "LookUp",
            Action::LookDown => "LookDown",
            Action::LookLeft => "LookLeft",
            Action::LookRight => "LookRight",
            Action::SelectUp => "Up",
            Action::SelectDown => "Down",
            Action::SelectRight => "Right",
            Action::Walk => "Walk",
            Action::Sprint => "Sprint",
            Action::Start => "Start",
        }
    }
    const ALL: [Action; 25] = [
        Action::Forward, Action::Backward, Action::MoveLeft, Action::MoveRight, Action::PrimaryAttack,
        Action::PrimarySwitch, Action::Reload, Action::SecondaryAttack, Action::SecondarySwitch,
        Action::MeleeAttack, Action::Use, Action::Jump, Action::Crouch, Action::Binoculars, Action::Pda,
        Action::LookUp, Action::LookDown, Action::LookLeft, Action::LookRight, Action::SelectUp,
        Action::SelectDown, Action::SelectRight, Action::Walk, Action::Sprint, Action::Start,
    ];
}

/// A keyboard/mouse binding.
#[derive(Clone, Copy, PartialEq)]
enum Bind {
    Key(KeyCode),
    Mouse(winit::event::MouseButton),
}

/// A gamepad binding (a button, or a stick axis past a threshold in one direction).
#[derive(Clone, Copy, PartialEq)]
enum GpBind {
    Button(gilrs::Button),
    AxisPos(gilrs::Axis),
    AxisNeg(gilrs::Axis),
}

/// Parsed `Mercs2.ini` bindings + mouse tuning.
pub struct Bindings {
    keys: HashMap<Action, Vec<Bind>>,
    pads: HashMap<Action, Vec<GpBind>>,
    pub invert_y: bool,
    /// Radians of look per pixel of mouse motion (from `[Mouse] Sensitivity`, 1–20).
    pub mouse_rad_per_px: f32,
}

impl Default for Bindings {
    /// Retail defaults (used if `Mercs2.ini` is missing/unreadable): the stock KB/mouse map + a standard
    /// Xbox gamepad layout.
    fn default() -> Self {
        let mut keys: HashMap<Action, Vec<Bind>> = HashMap::new();
        let mut k = |a: Action, s: &str| {
            if let Some(b) = parse_bind(s) {
                keys.entry(a).or_default().push(b);
            }
        };
        k(Action::Forward, "W");
        k(Action::Backward, "S");
        k(Action::MoveLeft, "A");
        k(Action::MoveRight, "D");
        k(Action::PrimaryAttack, "MOUSE1");
        k(Action::SecondaryAttack, "MOUSE2");
        k(Action::Reload, "R");
        k(Action::MeleeAttack, "F");
        k(Action::Use, "E");
        k(Action::Jump, "SPACE");
        k(Action::Crouch, "C");
        k(Action::Sprint, "LSHIFT");
        k(Action::Walk, "LCTRL");
        k(Action::LookUp, "I");
        k(Action::LookDown, "K");
        k(Action::LookLeft, "J");
        k(Action::LookRight, "L");
        Bindings { keys, pads: default_pad_map(), invert_y: false, mouse_rad_per_px: sensitivity_to_rad(9.0) }
    }
}

impl Bindings {
    /// Load from a `Mercs2.ini` path; falls back to [`Bindings::default`] on any problem.
    pub fn load(path: &std::path::Path) -> Bindings {
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("[input] Mercs2.ini not found at {} — using retail defaults", path.display());
            return Bindings::default();
        };
        let sections = parse_ini(&text);
        let mut keys: HashMap<Action, Vec<Bind>> = HashMap::new();
        for sect in ["Actions1", "Actions2"] {
            if let Some(map) = sections.get(sect) {
                for a in Action::ALL {
                    if let Some(v) = map.get(a.ini_name()) {
                        if let Some(b) = parse_bind(v) {
                            keys.entry(a).or_default().push(b);
                        }
                    }
                }
            }
        }
        // Gamepad: honour the XBOX 360 section (DInput indices == gilrs' normalised Xbox layout); else
        // fall back to the standard layout so a pad always works.
        let mut pads: HashMap<Action, Vec<GpBind>> = HashMap::new();
        if let Some(map) = sections.get("Controller (XBOX 360 For Windows)") {
            for a in Action::ALL {
                if let Some(v) = map.get(a.ini_name()) {
                    if let Some(b) = parse_gp_bind(v) {
                        pads.entry(a).or_default().push(b);
                    }
                }
            }
        }
        if pads.is_empty() {
            pads = default_pad_map();
        }
        let mouse = sections.get("Mouse");
        let invert_y = mouse.and_then(|m| m.get("InvertY")).map(|v| v.trim() != "0").unwrap_or(false);
        let sens = mouse.and_then(|m| m.get("Sensitivity")).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(9.0);
        if keys.is_empty() {
            eprintln!("[input] Mercs2.ini had no [Actions*] binds — using retail defaults");
            return Bindings::default();
        }
        println!(
            "[input] Mercs2.ini loaded: {} kb/mouse actions, {} gamepad actions, sensitivity {sens}, invertY {invert_y}",
            keys.len(), pads.len()
        );
        Bindings { keys, pads, invert_y, mouse_rad_per_px: sensitivity_to_rad(sens) }
    }
}

/// Standard Xbox gamepad layout (fallback when the ini has no controller section). Left stick = move,
/// right stick = look, plus a sensible full-action button map.
fn default_pad_map() -> HashMap<Action, Vec<GpBind>> {
    use gilrs::{Axis, Button};
    let mut m: HashMap<Action, Vec<GpBind>> = HashMap::new();
    let mut p = |a: Action, b: GpBind| m.entry(a).or_default().push(b);
    p(Action::Forward, GpBind::AxisPos(Axis::LeftStickY));
    p(Action::Backward, GpBind::AxisNeg(Axis::LeftStickY));
    p(Action::MoveRight, GpBind::AxisPos(Axis::LeftStickX));
    p(Action::MoveLeft, GpBind::AxisNeg(Axis::LeftStickX));
    p(Action::LookUp, GpBind::AxisPos(Axis::RightStickY));
    p(Action::LookDown, GpBind::AxisNeg(Axis::RightStickY));
    p(Action::LookRight, GpBind::AxisPos(Axis::RightStickX));
    p(Action::LookLeft, GpBind::AxisNeg(Axis::RightStickX));
    p(Action::PrimaryAttack, GpBind::Button(Button::RightTrigger2));
    p(Action::SecondaryAttack, GpBind::Button(Button::LeftTrigger2));
    p(Action::PrimarySwitch, GpBind::Button(Button::RightTrigger));
    p(Action::SecondarySwitch, GpBind::Button(Button::LeftTrigger));
    p(Action::Jump, GpBind::Button(Button::South));
    p(Action::MeleeAttack, GpBind::Button(Button::East));
    p(Action::Reload, GpBind::Button(Button::West));
    p(Action::Use, GpBind::Button(Button::North));
    p(Action::Sprint, GpBind::Button(Button::LeftThumb));
    p(Action::Binoculars, GpBind::Button(Button::RightThumb));
    p(Action::Pda, GpBind::Button(Button::Select));
    p(Action::Start, GpBind::Button(Button::Start));
    p(Action::SelectUp, GpBind::Button(Button::DPadUp));
    p(Action::SelectDown, GpBind::Button(Button::DPadDown));
    p(Action::SelectRight, GpBind::Button(Button::DPadRight));
    p(Action::Crouch, GpBind::Button(Button::DPadLeft));
    m
}

/// Live gamepad state via gilrs. Owns the gilrs context; poll [`Gamepad::update`] once per frame.
pub struct Gamepad {
    gilrs: Option<gilrs::Gilrs>,
    active: Option<gilrs::GamepadId>,
}

impl Default for Gamepad {
    fn default() -> Self {
        Self::new()
    }
}

impl Gamepad {
    pub fn new() -> Self {
        match gilrs::Gilrs::new() {
            Ok(g) => {
                let active = g.gamepads().next().map(|(id, _)| id);
                if active.is_some() {
                    println!("[input] gamepad detected");
                }
                Gamepad { gilrs: Some(g), active }
            }
            Err(e) => {
                eprintln!("[input] gamepad backend unavailable: {e:?}");
                Gamepad { gilrs: None, active: None }
            }
        }
    }

    /// Pump gilrs events (connect/disconnect) and keep an active pad selected. Call once per frame.
    pub fn update(&mut self) {
        let Some(g) = self.gilrs.as_mut() else { return };
        let mut active = self.active;
        while let Some(ev) = g.next_event() {
            match ev.event {
                gilrs::EventType::Connected => active = Some(ev.id),
                gilrs::EventType::Disconnected if active == Some(ev.id) => active = None,
                _ => {}
            }
        }
        if active.is_none() {
            active = g.gamepads().next().map(|(id, _)| id);
        }
        self.active = active;
    }

    pub fn connected(&self) -> bool {
        self.active.is_some()
    }

    fn pressed(&self, b: gilrs::Button) -> bool {
        match (self.gilrs.as_ref(), self.active) {
            (Some(g), Some(id)) => g.gamepad(id).is_pressed(b),
            _ => false,
        }
    }

    /// Deadzoned axis value in [-1,1].
    fn axis(&self, a: gilrs::Axis) -> f32 {
        let v = match (self.gilrs.as_ref(), self.active) {
            (Some(g), Some(id)) => g.gamepad(id).value(a),
            _ => 0.0,
        };
        if v.abs() < 0.15 {
            0.0
        } else {
            v
        }
    }
}

/// Live input snapshot: the held keyboard keys + mouse buttons + gamepad this frame. Game code queries
/// actions through this and never names raw keys/buttons.
pub struct Input<'a> {
    pub bindings: &'a Bindings,
    pub keys: &'a HashSet<KeyCode>,
    pub mouse: &'a HashSet<winit::event::MouseButton>,
    pub gamepad: &'a Gamepad,
}

impl Input<'_> {
    /// Keyboard/mouse only (no gamepad) — for digital movement / keyboard look so analog sticks aren't
    /// double-counted.
    pub fn kb_held(&self, a: Action) -> bool {
        self.bindings.keys.get(&a).is_some_and(|bs| {
            bs.iter().any(|b| match b {
                Bind::Key(k) => self.keys.contains(k),
                Bind::Mouse(m) => self.mouse.contains(m),
            })
        })
    }

    fn gp_held(&self, a: Action) -> bool {
        self.bindings.pads.get(&a).is_some_and(|bs| {
            bs.iter().any(|b| match b {
                GpBind::Button(btn) => self.gamepad.pressed(*btn),
                GpBind::AxisPos(ax) => self.gamepad.axis(*ax) > 0.5,
                GpBind::AxisNeg(ax) => self.gamepad.axis(*ax) < -0.5,
            })
        })
    }

    /// Is the action active on any device this frame?
    pub fn held(&self, a: Action) -> bool {
        self.kb_held(a) || self.gp_held(a)
    }

    /// Planar move intent: `.0` = strafe (−left … +right), `.1` = forward (−back … +forward), in a unit
    /// disk. Combines digital WASD (±1) with the analog left stick.
    pub fn move_vec(&self) -> (f32, f32) {
        let mut x = self.kb_held(Action::MoveRight) as i32 as f32 - self.kb_held(Action::MoveLeft) as i32 as f32;
        let mut y = self.kb_held(Action::Forward) as i32 as f32 - self.kb_held(Action::Backward) as i32 as f32;
        x += self.gamepad.axis(gilrs::Axis::LeftStickX);
        y += self.gamepad.axis(gilrs::Axis::LeftStickY);
        let mag = (x * x + y * y).sqrt();
        if mag > 1.0 {
            (x / mag, y / mag)
        } else {
            (x, y)
        }
    }

    /// Right-stick look delta this frame: `(yaw+, pitch+)` in radians (already dt-scaled). Keyboard look
    /// keys and the mouse are handled by the caller separately.
    pub fn look_delta(&self, dt: f32) -> (f32, f32) {
        const GAMEPAD_LOOK: f32 = 2.6; // rad/s at full deflection
        let yaw = self.gamepad.axis(gilrs::Axis::RightStickX) * GAMEPAD_LOOK * dt;
        let pitch = self.gamepad.axis(gilrs::Axis::RightStickY) * GAMEPAD_LOOK * dt;
        (yaw, pitch)
    }
}

// ---------------------------------------------------------------------------
//   Parsing
// ---------------------------------------------------------------------------

/// Parse an ini into `section -> {key -> value}`.
fn parse_ini(text: &str) -> HashMap<String, HashMap<String, String>> {
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
    sections
}

/// `[Mouse] Sensitivity` (1–20, default 9) → radians of look per pixel. 9 maps to the engine's prior
/// hardcoded 0.0008 rad/px so default feel is unchanged.
fn sensitivity_to_rad(s: f32) -> f32 {
    0.0008 * (s.clamp(1.0, 20.0) / 9.0)
}

/// Parse a keyboard/mouse ini token (`W`, `SPACE`, `LSHIFT`, `MOUSE1`). `NULL`/`EMPTY`/unknown → None.
fn parse_bind(s: &str) -> Option<Bind> {
    match s.trim().to_ascii_uppercase().as_str() {
        "NULL" | "EMPTY" | "" | "MWHEEL" => None,
        "MOUSE1" => Some(Bind::Mouse(winit::event::MouseButton::Left)),
        "MOUSE2" => Some(Bind::Mouse(winit::event::MouseButton::Right)),
        "MOUSE3" => Some(Bind::Mouse(winit::event::MouseButton::Middle)),
        other => key_from_name(other).map(Bind::Key),
    }
}

/// Parse a gamepad ini token from the `[Controller (XBOX 360 …)]` section.
fn parse_gp_bind(s: &str) -> Option<GpBind> {
    use gilrs::{Axis, Button};
    let s = s.trim();
    let up = s.to_ascii_uppercase();
    // Sticks: "Left Stick - Up" etc.
    if let Some(dir) = up.strip_prefix("LEFT STICK - ") {
        return match dir {
            "UP" => Some(GpBind::AxisPos(Axis::LeftStickY)),
            "DOWN" => Some(GpBind::AxisNeg(Axis::LeftStickY)),
            "RIGHT" => Some(GpBind::AxisPos(Axis::LeftStickX)),
            "LEFT" => Some(GpBind::AxisNeg(Axis::LeftStickX)),
            _ => None,
        };
    }
    if let Some(dir) = up.strip_prefix("RIGHT STICK - ") {
        return match dir {
            "UP" => Some(GpBind::AxisPos(Axis::RightStickY)),
            "DOWN" => Some(GpBind::AxisNeg(Axis::RightStickY)),
            "RIGHT" => Some(GpBind::AxisPos(Axis::RightStickX)),
            "LEFT" => Some(GpBind::AxisNeg(Axis::RightStickX)),
            _ => None,
        };
    }
    // Triggers are exposed as a shared axis in DInput; the ini calls them "Other Left Stick - L/R".
    if up == "OTHER LEFT STICK - RIGHT" {
        return Some(GpBind::Button(Button::RightTrigger2));
    }
    if up == "OTHER LEFT STICK - LEFT" {
        return Some(GpBind::Button(Button::LeftTrigger2));
    }
    if let Some(dir) = up.strip_prefix("DPAD ") {
        return match dir {
            "UP" => Some(GpBind::Button(Button::DPadUp)),
            "DOWN" => Some(GpBind::Button(Button::DPadDown)),
            "LEFT" => Some(GpBind::Button(Button::DPadLeft)),
            "RIGHT" => Some(GpBind::Button(Button::DPadRight)),
            _ => None,
        };
    }
    if let Some(n) = up.strip_prefix("BUTTON ") {
        // Standard DInput index of an Xbox pad == gilrs' normalised layout.
        return match n.trim().parse::<u8>().ok()? {
            1 => Some(GpBind::Button(Button::South)),
            2 => Some(GpBind::Button(Button::East)),
            3 => Some(GpBind::Button(Button::West)),
            4 => Some(GpBind::Button(Button::North)),
            5 => Some(GpBind::Button(Button::LeftTrigger)),
            6 => Some(GpBind::Button(Button::RightTrigger)),
            7 => Some(GpBind::Button(Button::Select)),
            8 => Some(GpBind::Button(Button::Start)),
            9 => Some(GpBind::Button(Button::LeftThumb)),
            10 => Some(GpBind::Button(Button::RightThumb)),
            11 => Some(GpBind::Button(Button::Mode)),
            _ => None,
        };
    }
    None
}

/// Map an ini key name to a winit [`KeyCode`].
fn key_from_name(s: &str) -> Option<KeyCode> {
    use KeyCode::*;
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
    let p = std::path::Path::new(&vz).parent()?.parent()?.join("Mercs2.ini");
    p.exists().then_some(p)
}
