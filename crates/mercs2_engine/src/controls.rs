//! The control option pages: the general page (`LTIInputGeneral*`), the keyboard and mouse remap
//! page (`LTIInputKM*`), and the joystick remap page (`LTIInputJoystick*`), as retail's `LTILibName`
//! bindings run them.
//!
//! Retail keeps the keyboard binds in the input config (`[0x00ED3B1C]`) at `+0xEA0`: 35 rows of a
//! primary and an alternate [`crate::keys`] id. Rows 0..24 are the actions `Mercs2.ini`'s
//! `[Actions1]`/`[Actions2]` name; rows 24..35 are the menu's own and always the built-in ones. The
//! remap page edits a staging copy (`0x00EDAE60`) that Apply commits and saves.

use std::collections::HashMap;
use std::rc::Rc;

use mercs2_ui::FlashArg;

use crate::keys;
use crate::options::{atoi, Effect, Record, Reply};

/// `0x00D1E440`: the INI key of each row, and `Start`, which only the joystick sections carry.
pub const ACTIONS: [&str; 25] = [
    "Forward", "Backward", "MoveLeft", "MoveRight", "PrimaryAttack", "PrimarySwitch", "Reload", "SecondaryAttack",
    "SecondarySwitch", "MeleeAttack", "Use", "Jump", "Crouch", "Binoculars", "PDA", "LookUp", "LookDown", "LookLeft",
    "LookRight", "Up", "Down", "Right", "Walk", "Sprint", "Start",
];

/// The rows the remap pages list (`0x18`); the joystick page has `Start` too.
pub const ROWS: usize = 24;

/// `0x00D1E5D8`: the name the remap pages give each row (`LTIInputKMSetActionName`).
const ACTION_TEXT: [&str; 25] = [
    "[0xa4bcfdd8]", "[0x36865a62]", "[0x2bbfb180]", "[0xa738a167]", "[0x29aface6]", "[0xa777dad2]", "[0x741cbf9d]",
    "[0x49716b9a]", "[0xfbaa9286]", "[0x719fe83c]", "[0x5644564b]", "[0xfda32c92]", "[0xfc4fa8b4]", "[0x3d94c67c]",
    "[0x2da77f77]", "[0xdaaa3cb2]", "[0x21cb0267]", "[0x83999416]", "[0xa0199cff]", "[0xf98f60b1]", "[0xd7c932d9]",
    "[0x768777b1]", "[0xea5788f1]", "[0xe97b0bd7]", "[0x72abfa75]",
];

/// `0x00D1E578`: the name the already-bound question gives each row.
const CONFLICT_TEXT: [&str; ROWS] = [
    "[0x841fc6c5]", "[0x7e6e8759]", "[0x4a8c7ed9]", "[0x8b4c536e]", "[0xc972a1a0]", "[0x25a96ad4]", "[0x741cbf9d]",
    "[0xf418e84c]", "[0x9eda2940]", "[0x3e119916]", "[0x5644564b]", "[0xfda32c92]", "[0xfc4fa8b4]", "[0x3d94c67c]",
    "[0x2da77f77]", "[0x8baa2db4]", "[0x3451dd0f]", "[0x4cba8e10]", "[0x2b99bdb7]", "[0xddbbf03a]", "[0x2b0e79f1]",
    "[0x3cac5639]", "[0xea5788f1]", "[0xe97b0bd7]",
];

/// `0x00B9BB98`: the built-in binds, `[primary, alternate]` per row.
pub const DEFAULT_KEYS: [[u16; 2]; 35] = [
    [0x11, 0],
    [0x1F, 0],
    [0x1E, 0],
    [0x20, 0],
    [0x100, 0],
    [0x0F, 0x10A],
    [0x13, 0],
    [0x101, 0x22],
    [0x2F, 0],
    [0x21, 0x102],
    [0x12, 0],
    [0x39, 0],
    [0x2E, 0],
    [0x30, 0],
    [0x3B, 0],
    [0x17, 0],
    [0x25, 0],
    [0x24, 0],
    [0x26, 0],
    [0x02, 0xC8],
    [0x03, 0xD0],
    [0x05, 0xCD],
    [0x1D, 0],
    [0x2A, 0],
    [0x01, 0],
    [0xC8, 0],
    [0xD0, 0],
    [0xCB, 0],
    [0xCD, 0],
    [0x1C, 0x39],
    [0x01, 0x101],
    [0x3B, 0],
    [0x108, 0xC9],
    [0x109, 0xD1],
    [0x0F, 0],
];

/// The row of the menu's own binds that copies the PDA's (`FUN_004FB6B0`, `0x1F`).
const PDA_ROW: usize = 14;
const PDA_COPY_ROW: usize = 31;

pub type Keys = [[u16; 2]; 35];

/// Rows 24..35 back to the built-in binds, the PDA's copy following row 14 (`FUN_004FB6B0`,
/// `FUN_004FB890`).
fn force_menu_rows(keys: &mut Keys) {
    for row in ROWS..keys.len() {
        keys[row] = if row == PDA_COPY_ROW { keys[PDA_ROW] } else { DEFAULT_KEYS[row] };
    }
}

/// `FUN_004FB6B0` for `[Actions1]` (column 0) and `[Actions2]` (column 1): each key's name read
/// back to its id, the built-in one when the key is absent and NULL when the table has no such name.
pub fn load_keys(vz_wad: &str) -> Result<Keys, String> {
    let mut keys = DEFAULT_KEYS;
    for column in 0..2 {
        let section = format!("Actions{}", column + 1);
        for (row, action) in ACTIONS[..ROWS].iter().enumerate() {
            if let Some(name) = crate::ini::string(vz_wad, &section, action)? {
                keys[row][column] = keys::id(&name).unwrap_or(keys::NULL);
            }
        }
    }
    force_menu_rows(&mut keys);
    Ok(keys)
}

/// `FUN_004FB890`, before the binds are saved: ESCAPE comes off every action, and an id bound twice
/// keeps the earlier of the two in column order then row order (`MWHEEL` counting as bound wherever
/// `MWUP` or `MWDOWN` is). Whether anything changed.
pub fn resolve_conflicts(keys: &mut Keys) -> bool {
    let mut changed = false;
    for column in 0..2 {
        for row in 0..ROWS {
            if keys[row][column] == keys::NULL {
                continue;
            }
            if keys[row][column] == keys::ESCAPE {
                keys[row][column] = keys::NULL;
                changed = true;
                continue;
            }
            for other_column in 0..2 {
                for other_row in 0..ROWS {
                    if (other_column, other_row) == (column, row) {
                        continue;
                    }
                    let mine = keys[row][column];
                    let theirs = keys[other_row][other_column];
                    if mine == theirs || (mine == keys::MWHEEL && (theirs == keys::MWUP || theirs == keys::MWDOWN)) {
                        changed = true;
                        if column <= other_column {
                            keys[other_row][other_column] = keys::NULL;
                        } else {
                            keys[row][column] = keys::NULL;
                        }
                    }
                }
            }
        }
    }
    if changed {
        force_menu_rows(keys);
    }
    changed
}

/// The `[Actions1]` (column 0) or `[Actions2]` entries, as the writers save them.
fn key_entries(keys: &Keys, column: usize) -> Vec<(&'static str, &'static str, String)> {
    let section = ["Actions1", "Actions2"][column];
    ACTIONS[..ROWS].iter().enumerate().map(|(row, action)| (section, *action, keys::NAMES[keys[row][column] as usize].to_string())).collect()
}

/// `0x00CF2560`: the joystick inputs as the device sections name them; the last is "no input".
pub const PAD_NAMES: [&str; 54] = [
    "Left Stick - Left", "Left Stick - Right", "Left Stick - Down", "Left Stick - Up",
    "Right Stick - Left", "Right Stick - Right", "Right Stick - Down", "Right Stick - Up",
    "Other Left Stick - Left", "Other Left Stick - Right", "Other Left Stick - Down", "Other Left Stick - Up",
    "Other Right Stick - Left", "Other Right Stick - Right", "Other Right Stick - Down", "Other Right Stick - Up",
    "DPAD Left", "DPAD Right", "DPAD Down", "DPAD Up",
    "Button 1", "Button 2", "Button 3", "Button 4", "Button 5", "Button 6", "Button 7", "Button 8", "Button 9", "Button 10",
    "Button 11", "Button 12", "Button 13", "Button 14", "Button 15", "Button 16", "Button 17", "Button 18", "Button 19", "Button 20",
    "Button 21", "Button 22", "Button 23", "Button 24", "Button 25", "Button 26", "Button 27", "Button 28", "Button 29", "Button 30",
    "Button 31", "Button 32", "Button 33", "EMPTY",
];
pub const PAD_EMPTY: u8 = 0x35;
/// The pad inputs that are the triggers on an XInput pad (`Other Left Stick - Left`/`Right`).
pub const PAD_LEFT_TRIGGER: u8 = 8;
pub const PAD_RIGHT_TRIGGER: u8 = 9;

/// `0x00CF2638`: what the joystick page shows for each input (index 14 reuses 10's key in retail).
const PAD_TEXT: [&str; 54] = [
    "[0xd9c96e50]", "[0x170b90f7]", "[0xc160bd4f]", "[0x34544cf4]", "[0xcfa34807]", "[0xde168f06]", "[0x52868cc8]", "[0x4d44eb87]",
    "[0xd6476d7a]", "[0xa6f1b2d1]", "[0x3f022b62]", "[0x9e5c1482]", "[0x1cd00741]", "[0xf1cf8428]", "[0x3f022b62]", "[0x77117035]",
    "[0xdc04b6b5]", "[0xfc84790c]", "[0xa8db2654]", "[0x08a26dfd]", "[0xe3c445cf]", "[0x59c73e28]", "[0x43c95a1d]", "[0x39b8013e]",
    "[0x5bba755b]", "[0xc1bd5484]", "[0xdbbfbc09]", "[0x41d622ea]", "[0x63d89707]", "[0x92eb2f55]", "[0x68e8aea0]", "[0x12e5e8a7]",
    "[0xf0e3748a]", "[0x0ae15ee1]", "[0x90de603c]", "[0x6adbe5d3]", "[0x88d9d676]", "[0x72d7753d]", "[0x08d48fc8]", "[0x5e708fc4]",
    "[0x7872f749]", "[0xd66b3c7e]", "[0xf86db09b]", "[0xf67a7968]", "[0xe07c955d]", "[0x5e750cf2]", "[0x8077810f]", "[0x7e8449dc]",
    "[0x78867f01]", "[0xcd07dcbf]", "[0x6b0503e2]", "[0xed0c8c4d]", "[0xc30a0b98]", "[0x1033c821]",
];
/// `0x00CF2718`, `0x00CF271C`: an XInput pad's triggers, `Left Trigger` and `Right Trigger`.
const LEFT_TRIGGER_TEXT: &str = "[0x83ecba77]";
const RIGHT_TRIGGER_TEXT: &str = "[0x32694dd8]";

/// `0x00B9BCB0`: each row's action id, and `0x00B92A10`: each action id's slot in a pad's binds.
const ROW_ACTION: [usize; 25] = [0x0B, 0x0C, 0x09, 0x0A, 0x14, 0x15, 0x07, 0x12, 0x03, 0x08, 0x05, 0x06, 0x11, 0x16, 0x18, 0x0F, 0x10, 0x0D, 0x0E, 0x01, 0x02, 0x04, 0x19, 0x13, 0x17];
const ACTION_SLOT: [usize; 26] = [0x19, 0x0C, 0x0E, 0x0F, 0x0D, 0x00, 0x02, 0x03, 0x01, 0x10, 0x11, 0x13, 0x12, 0x14, 0x15, 0x17, 0x16, 0x06, 0x04, 0x09, 0x07, 0x05, 0x0A, 0x0B, 0x08, 0x18];
/// The slot the Jump row binds, whose input is the pad's confirm (`0x014A3846`).
const CONFIRM_SLOT: usize = 2;

fn pad_slot(row: usize) -> usize {
    ACTION_SLOT[ROW_ACTION[row]]
}

/// `0x00CF2720`: the built-in binds of each pad layout, by slot.
const PAD_LAYOUTS: [[u8; 25]; 10] = [
    [0x17, 0x16, 0x15, 0x14, 0x1a, 0x1b, 0x18, 0x19, 0x1c, 0x1e, 0x1f, 0x1d, 0x13, 0x11, 0x12, 0x10, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x35],
    [0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x00, 0x01, 0x02, 0x03, 0x07, 0x06, 0x05, 0x04, 0x35],
    [0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1d, 0x1e, 0x1f, 0x1c, 0x20, 0x21, 0x22, 0x23, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x35],
    [0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x00, 0x01, 0x02, 0x03, 0x07, 0x06, 0x09, 0x08, 0x35],
    [0x17, 0x15, 0x14, 0x16, 0x18, 0x19, 0x08, 0x09, 0x1a, 0x1c, 0x1d, 0x1b, 0x13, 0x11, 0x12, 0x10, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x35],
    [0x17, 0x16, 0x15, 0x14, 0x1a, 0x1b, 0x18, 0x19, 0x1c, 0x1e, 0x1f, 0x1d, 0x13, 0x11, 0x12, 0x10, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x35],
    [0x17, 0x16, 0x14, 0x15, 0x19, 0x1b, 0x18, 0x1a, 0x1c, 0x1f, 0x20, 0x35, 0x13, 0x11, 0x12, 0x10, 0x00, 0x01, 0x02, 0x03, 0x07, 0x06, 0x09, 0x08, 0x35],
    [0x17, 0x16, 0x15, 0x14, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x0c, 0x09, 0x0d, 0x08, 0x00, 0x01, 0x02, 0x03, 0x07, 0x06, 0x05, 0x04, 0x35],
    [0x18, 0x15, 0x14, 0x17, 0x19, 0x16, 0x1a, 0x1b, 0x1d, 0x1e, 0x1f, 0x1c, 0x20, 0x21, 0x22, 0x23, 0x00, 0x01, 0x02, 0x03, 0x07, 0x06, 0x09, 0x08, 0x35],
    [0x17, 0x15, 0x14, 0x16, 0x18, 0x19, 0x08, 0x09, 0x1a, 0x1c, 0x1d, 0x1b, 0x13, 0x11, 0x12, 0x10, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x35],
];

/// `0x00CF2840`: the pads with a layout of their own, `(vendor, product)` → layout (`FUN_008283F0`).
const PAD_PRODUCTS: [(u16, u16); 9] =
    [(0x6666, 0x0667), (0x0B43, 0x0003), (0x0925, 0x0005), (0x044F, 0xB303), (0x045E, 0x028E), (0x046D, 0xC216), (0x044F, 0xB300), (0x047D, 0x4006), (0x046D, 0xC20A)];
/// The layout of an XInput pad the table does not list.
const XINPUT_LAYOUT: usize = 9;

/// A pad the game reads, as it attaches it. Every pad is read in XInput's shape (the engine's pad
/// library normalises them to it), so each takes retail's XInput treatment: its triggers named as
/// triggers and, unlisted, the XInput layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Pad {
    /// Its name, which is its section in `Mercs2.ini`.
    pub name: String,
    pub vendor: u16,
    pub product: u16,
}

/// One pad's record (`cfg + 0x5C + k × 0x390`): its layout (`+0x340`) and binds by slot (`+0x374`).
#[derive(Clone, Debug)]
struct Device {
    name: String,
    layout: usize,
    binds: [u8; 25],
}

/// Inputs bound twice keep the first by slot; the rest become EMPTY (`FUN_004FBD40`, `0x02478750`).
fn dedupe_pad(binds: &mut [u8; 25]) {
    for i in 0..binds.len() {
        for k in i + 1..binds.len() {
            if binds[i] != PAD_EMPTY && binds[k] == binds[i] {
                binds[k] = PAD_EMPTY;
            }
        }
    }
}

/// `FUN_004FBD40` for one pad: each row's input by name from the pad's section, the layout's when the
/// key is absent; then the doubled ones cleared.
fn load_device(vz_wad: &str, pad: &Pad) -> Result<Device, String> {
    let layout = PAD_PRODUCTS.iter().position(|&p| p == (pad.vendor, pad.product)).unwrap_or(XINPUT_LAYOUT);
    let mut binds = PAD_LAYOUTS[layout];
    for (row, action) in ACTIONS.iter().enumerate() {
        if let Some(name) = crate::ini::string(vz_wad, &pad.name, action)? {
            let input = PAD_NAMES.iter().position(|n| n.eq_ignore_ascii_case(&name)).ok_or_else(|| format!("[{}] {action}={name} names no joystick input", pad.name))?;
            binds[pad_slot(row)] = input as u8;
        }
    }
    dedupe_pad(&mut binds);
    Ok(Device { name: pad.name.clone(), layout, binds })
}

/// `FUN_0082A900`/`0x004D4640`: what the joystick page shows for an input.
fn pad_display(input: u8) -> Result<&'static str, String> {
    match input {
        PAD_LEFT_TRIGGER => Ok(LEFT_TRIGGER_TEXT),
        PAD_RIGHT_TRIGGER => Ok(RIGHT_TRIGGER_TEXT),
        _ => PAD_TEXT.get(input as usize).copied().ok_or_else(|| format!("joystick input {input:#x} has no name")),
    }
}

/// The joystick page's titles and buttons: `Joystick Key Over Bound`, `Too many buttons`, `Too many
/// buttons were pressed.`, `Ok`.
const PAD_OVER_BOUND_TITLE: &str = "[0x764469fd]";
const PAD_TOO_MANY_TITLE: &str = "[0x045ff684]";
const PAD_TOO_MANY_TEXT: &str = "[0x645dd0c7]";
const OK: &str = "[0x96673ef8]";

/// The string key of the already-bound question's text: `The key %s is currently bound to %s, would
/// you like to bind it to %s instead?`, and of its title, `Key Over Bound`.
const OVER_BOUND_TEXT: u32 = 0x603E1C7D;
const OVER_BOUND_TITLE: &str = "[0x23641453]";
/// `YES` and `NO`, as `sprintf("[0x%08X]")` writes them.
const YES: &str = "[0xAF4B6E11]";
const NO: &str = "[0xC9A7EABD]";

/// The rebind waiting on the already-bound question (`0x00D1467C..0x00D1468C`, each -1 when unset):
/// the row and column edited, the key, and the row and column that already had it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct OverBound {
    row: i32,
    column: i32,
    key: i32,
    other_row: i32,
    other_column: i32,
}

impl OverBound {
    const UNSET: OverBound = OverBound { row: -1, column: -1, key: -1, other_row: -1, other_column: -1 };

    fn set(&self) -> bool {
        [self.row, self.column, self.key, self.other_row, self.other_column].iter().all(|&v| v != -1)
    }
}

/// The control pages' state: the input config's keyboard binds and the remap page's copies, the mouse
/// (`[0x00ED4574]`), and the joystick sensitivity.
pub struct Controls {
    vz_wad: String,
    /// The shell's string table, for the already-bound question.
    strings: Rc<HashMap<u32, String>>,
    /// The committed binds (`+0xEA0`), the page's staging copy (`0x00EDAE60`) and the copy taken before
    /// a rebind (`0x00EDAD48`).
    keys: Keys,
    staging: Keys,
    backup: Keys,
    /// The slot a rebind fills (`0x00EDAF78`): the row, plus 100 for the alternate column.
    editing: i32,
    /// A rebind waits for its answer (`0x01175F2E`), and the next key down gives it (`+0xFC4`).
    waiting: bool,
    armed: bool,
    /// The remap page is open (`+0xFC0`).
    km_page: bool,
    over_bound: OverBound,
    /// `[Mouse] InvertY` as read (`+0x10`): 1 leaves the mouse's Y as it comes (`0x004FA6D5`).
    mouse_invert_y: bool,
    /// `[Mouse] Sensitivity` n, kept as n × 0.02 (`+0x14`).
    mouse_sensitivity: f32,
    /// `[Joystick] Sensitivity` n, kept as n × 0.05 + 0.5 (`+0x50`).
    joy_sensitivity: f32,
    /// The pads attached (`+0x58` of them); the pages edit the first.
    devices: Vec<Device>,
    /// The joystick page is open (`+0xFC8`), its staging binds by row (`0x00EDAD28`), the copy taken
    /// before a rebind (`0x00EDAD0C`), and the doubled inputs a rebind left (`0x00EDAD08`):
    /// `(input, row, row)`.
    joystick_page: bool,
    pad_staging: [u8; 25],
    pad_backup: [u8; 25],
    pad_doubles: Vec<(u8, usize, usize)>,
}

/// `0x00BBB9D0`, `0x00B984AC`, `0x00B9854C`: the mouse sensitivity's scale, its inverse, and the
/// slider's (0..100 to 1..20).
const MOUSE_SCALE: f32 = 0.02;
const MOUSE_UNSCALE: f32 = 50.0;
const SLIDER_TO_STEP: f32 = 0.2;
/// `0x00B98748`, `0x00BBB99C`, `0x00B9B980`: the joystick sensitivity's scale, offset and inverse.
const JOY_SCALE: f32 = 0.05;
const JOY_OFFSET: f32 = 0.5;
const JOY_UNSCALE: f32 = 20.0;
/// `0x00B9B700`: a 1..20 step to the slider's 0..100.
const STEP_TO_SLIDER: f32 = 5.0;

impl Controls {
    /// The binds, the mouse and the joystick sensitivity from the install's `Mercs2.ini`: `FUN_004FB6B0`,
    /// `FUN_004FD740` (`InvertY` 0, `Sensitivity` 10 in 1..20) and `0x00828A85` (`Sensitivity` 10 in 1..20).
    pub fn load(vz_wad: &str, strings: Rc<HashMap<u32, String>>) -> Result<Controls, String> {
        let keys = load_keys(vz_wad)?;
        let int = |section: &str, key: &str, default: i64| -> Result<i64, String> { Ok(crate::ini::int(vz_wad, section, key)?.unwrap_or(default)) };
        let mouse_step = int("Mouse", "Sensitivity", 10)?.clamp(1, 20);
        let joy_step = int("Joystick", "Sensitivity", 10)?.clamp(1, 20);
        Ok(Controls {
            vz_wad: vz_wad.to_string(),
            strings,
            keys,
            staging: keys,
            backup: keys,
            editing: 0,
            waiting: false,
            armed: false,
            km_page: false,
            over_bound: OverBound::UNSET,
            mouse_invert_y: int("Mouse", "InvertY", 0)? != 0,
            mouse_sensitivity: mouse_step as f32 * MOUSE_SCALE,
            joy_sensitivity: joy_step as f32 * JOY_SCALE + JOY_OFFSET,
            devices: Vec::new(),
            joystick_page: false,
            pad_staging: [PAD_EMPTY; 25],
            pad_backup: [PAD_EMPTY; 25],
            pad_doubles: Vec::new(),
        })
    }

    /// The pads, first the one the pages edit, each with its section read (`FUN_004FBD40`).
    pub fn attach_pads(&mut self, pads: &[Pad]) -> Result<(), String> {
        self.devices = pads.iter().map(|p| load_device(&self.vz_wad, p)).collect::<Result<_, _>>()?;
        Ok(())
    }

    fn device(&self) -> Result<&Device, String> {
        self.devices.first().ok_or_else(|| "the joystick page needs a pad attached".to_string())
    }

    /// One frame of the first pad's inputs held (`FUN_004FB270`), as retail lists them (at most 6): while
    /// the joystick page waits for an input, the held ones answer it (`FUN_005C2E80`); while the page is
    /// open, they are shown (`LTIInputSeeButtonsPushed`).
    pub fn pad_frame(&mut self, held: &[u8]) -> Result<Reply, String> {
        let mut reply = Reply::default();
        if !self.joystick_page {
            return Ok(reply);
        }
        if self.waiting && !held.is_empty() {
            let confirm = self.device().map(|d| held.contains(&d.binds[CONFIRM_SLOT]))?;
            self.pad_rebind(&mut reply, held, confirm)?;
        }
        let shown = held.iter().map(|&i| pad_display(i)).collect::<Result<Vec<_>, _>>()?.join(", ");
        reply.invoke("LTIInputSeeButtonsPushed", vec![FlashArg::Text(shown)]);
        Ok(reply)
    }

    /// `FUN_005C2E80`: one input fills the row being edited, unless another row has it, which asks; more
    /// than one at once is refused.
    fn pad_rebind(&mut self, reply: &mut Reply, held: &[u8], confirm: bool) -> Result<(), String> {
        self.pad_backup = self.pad_staging;
        self.waiting = false;
        let row = usize::try_from(self.editing).ok().filter(|&r| r < 25).ok_or_else(|| format!("joystick rebind of row {}: no such row", self.editing))?;
        if held.len() != 1 {
            reply.invoke(
                "LTIonlineMsgBox",
                vec![
                    FlashArg::Text(PAD_TOO_MANY_TITLE.into()),
                    FlashArg::Text(PAD_TOO_MANY_TEXT.into()),
                    FlashArg::Number(1.0),
                    FlashArg::Text(OK.into()),
                    FlashArg::Text(String::new()),
                    FlashArg::Number(0.0),
                ],
            );
            self.pad_staging = self.pad_backup;
            reply.invoke("LTIFinishedGettingInput", vec![FlashArg::Bool(false)]);
            return Ok(());
        }
        let input = held[0];
        self.pad_staging[row] = input;
        // `FUN_0082A820`: every pair of rows holding the same input.
        self.pad_doubles.clear();
        for i in 0..25 {
            for j in i + 1..25 {
                if self.pad_staging[i] != PAD_EMPTY && self.pad_staging[i] == self.pad_staging[j] {
                    self.pad_doubles.push((self.pad_staging[i], i, j));
                }
            }
        }
        match self.pad_doubles.first() {
            None => {
                reply.invoke("LTIInputJoystickMap", vec![FlashArg::Number(row as f64), FlashArg::Text(pad_display(input)?.into())]);
                reply.invoke("LTIFinishedGettingJoystickInput", vec![FlashArg::Bool(confirm)]);
                self.editing = 0;
            }
            Some(&(value, i, j)) => {
                // The row that kept its input owns it.
                let (owner, edited) = if self.pad_staging[i] == self.pad_backup[i] { (i, j) } else { (j, i) };
                let text = format!("[0x{OVER_BOUND_TEXT:08x}:{}:{}:{}]", pad_display(value)?, ACTION_TEXT[owner], ACTION_TEXT[edited]);
                reply.invoke(
                    "LTIonlineMsgBox",
                    vec![
                        FlashArg::Text(PAD_OVER_BOUND_TITLE.into()),
                        FlashArg::Text(text),
                        FlashArg::Number(2.0),
                        FlashArg::Text(YES.into()),
                        FlashArg::Text(NO.into()),
                        FlashArg::Number(2.0),
                    ],
                );
            }
        }
        Ok(())
    }

    /// `LTIInputJoystickMap` for every row of the staging binds.
    fn joystick_map(&self, reply: &mut Reply) -> Result<(), String> {
        for row in 0..25 {
            reply.invoke("LTIInputJoystickMap", vec![FlashArg::Number(row as f64), FlashArg::Text(pad_display(self.pad_staging[row])?.into())]);
        }
        Ok(())
    }

    /// `FUN_005C3500`: the staging binds back to the pad's layout.
    fn joystick_defaults(&mut self, reply: &mut Reply) -> Result<(), String> {
        let layout = PAD_LAYOUTS[self.device()?.layout];
        self.pad_staging = std::array::from_fn(|row| layout[pad_slot(row)]);
        self.joystick_map(reply)?;
        reply.invoke("LTIInputJoystickUpdateTable", vec![]);
        Ok(())
    }

    /// The writer behind `FUN_004FBC60` (its body at `0x02478750`): the first pad's section, every row's
    /// input by name after the doubled ones are cleared.
    fn save_pad(&mut self) -> Result<(), String> {
        let vz_wad = self.vz_wad.clone();
        let device = self.devices.first_mut().ok_or("the joystick page needs a pad attached")?;
        dedupe_pad(&mut device.binds);
        let entries: Vec<(&str, &str, String)> =
            ACTIONS.iter().enumerate().map(|(row, action)| (device.name.as_str(), *action, PAD_NAMES[device.binds[pad_slot(row)] as usize].to_string())).collect();
        crate::ini::write(&vz_wad, &entries)
    }

    /// The binds in force.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// Controls with the built-in binds and readers' defaults, saving nowhere, for the option pages'
    /// tests.
    #[cfg(test)]
    pub(crate) fn detached(strings: HashMap<u32, String>) -> Controls {
        Controls {
            vz_wad: String::new(),
            strings: Rc::new(strings),
            keys: DEFAULT_KEYS,
            staging: DEFAULT_KEYS,
            backup: DEFAULT_KEYS,
            editing: 0,
            waiting: false,
            armed: false,
            km_page: false,
            over_bound: OverBound::UNSET,
            mouse_invert_y: false,
            mouse_sensitivity: 10.0 * MOUSE_SCALE,
            joy_sensitivity: 10.0 * JOY_SCALE + JOY_OFFSET,
            devices: Vec::new(),
            joystick_page: false,
            pad_staging: [PAD_EMPTY; 25],
            pad_backup: [PAD_EMPTY; 25],
            pad_doubles: Vec::new(),
        }
    }

    /// `[Mouse]`'s step, as `FUN_004FD930` saves it.
    fn mouse_step(&self) -> i32 {
        (self.mouse_sensitivity * MOUSE_UNSCALE) as i32
    }

    /// `[Joystick] Sensitivity`'s step, as `FUN_004FBF20` saves it.
    fn joy_step(&self) -> i32 {
        (((self.joy_sensitivity - JOY_OFFSET) * JOY_UNSCALE) as i32).clamp(1, 20)
    }

    /// `FUN_004FD930`.
    fn save_mouse(&self) -> Result<(), String> {
        crate::ini::write(
            &self.vz_wad,
            &[("Mouse", "InvertY", (self.mouse_invert_y as i32).to_string()), ("Mouse", "Sensitivity", self.mouse_step().to_string())],
        )
    }

    /// The keyboard binds' writer (`0x006D4890`): the conflicts settled (`FUN_004FB890`), then both
    /// columns saved.
    fn save_keys(&mut self) -> Result<(), String> {
        resolve_conflicts(&mut self.keys);
        let mut entries = key_entries(&self.keys, 0);
        entries.extend(key_entries(&self.keys, 1));
        crate::ini::write(&self.vz_wad, &entries)
    }

    /// Whether a key down now answers a rebind: then [`Controls::key_down`] takes it and nothing else
    /// should see it.
    pub fn capturing(&self) -> bool {
        self.armed
    }

    /// `FUN_004FA570` while a rebind waits: the first key down (`0..0x10B`) goes to `FUN_005C2110`.
    pub fn key_down(&mut self, key: u16) -> Result<Reply, String> {
        if !self.armed {
            return Err(format!("key {key:#x} went to a rebind nobody asked for"));
        }
        self.armed = false;
        self.rebind(key as i32)
    }

    /// One control command: `arg` is its single argument as text when it takes one. `None` when the
    /// command is not a control one.
    pub fn call(&mut self, name: &str, arg: Option<&str>, record: &mut Record) -> Result<Option<Reply>, String> {
        let mut reply = Reply::default();
        let text = || arg.ok_or_else(|| format!("{name} takes an argument"));
        // `FUN_0059D850`: a number, or a string holding one, cut to an integer.
        let int = || -> Result<i32, String> { text()?.trim().parse::<f64>().map(|v| v as f32 as i32).map_err(|e| format!("{name}({arg:?}): {e}")) };
        match name {
            // `FUN_005C1470`.
            "LTIInputGeneralEnter" => {
                let mouse = ((self.mouse_step() as f32) * STEP_TO_SLIDER) as i32;
                reply.invoke("LTIInputSetMouseSense", vec![FlashArg::Number(mouse as f64)]);
                reply.invoke("LTIInputSetJoySense", vec![FlashArg::Number((self.joy_step() as f32 * STEP_TO_SLIDER) as f64)]);
                reply.invoke("LTIInputSetInvertJoystick", vec![FlashArg::Bool(record.joy_invert)]);
                reply.invoke("LTIInputSetInvertMouse", vec![FlashArg::Bool(self.mouse_invert_y)]);
                reply.invoke("LTIInputSetRumble", vec![FlashArg::Bool(record.rumble)]);
                reply.invoke("LTIInputNumberControllers", vec![FlashArg::Number(self.devices.len() as f64)]);
            }
            // `FUN_005C1600`: `"<case>&<value>"`, the case read with `atoi`.
            "LTIInputGeneralOptions" => {
                let s = text()?;
                let (case, value) = match s.split_once('&') {
                    Some((case, value)) => (case, value),
                    None => return Err(format!("{name}({s:?}) has no value after its case")),
                };
                match atoi(case) {
                    1 => {
                        let step = ((atoi(value) as f32 * SLIDER_TO_STEP) as i32).clamp(1, 20);
                        self.mouse_sensitivity = step as f32 * MOUSE_SCALE;
                        self.save_mouse()?;
                    }
                    2 => {
                        match atoi(value) {
                            1 => self.mouse_invert_y = true,
                            2 => self.mouse_invert_y = false,
                            _ => {}
                        }
                        self.save_mouse()?;
                    }
                    // `FUN_0082A960` then `FUN_004FBF20`, which also saves `[Joystick] Invert` from
                    // `[cfg+0x1EC9]`, a byte nothing read here sets; the movie's next case, 5, saves
                    // `Invert` from the record (`ControlsApply` sends 1..5 in order).
                    3 => {
                        let step = (atoi(value) / 5).clamp(1, 20);
                        self.joy_sensitivity = step as f32 * JOY_SCALE + JOY_OFFSET;
                        let mut entries = vec![("Joystick", "Sensitivity", self.joy_step().to_string())];
                        entries.extend(key_entries(&self.keys, 0));
                        crate::ini::write(&self.vz_wad, &entries)?;
                    }
                    // `rep cmpsb` against `"true"`, so the case matters.
                    4 => {
                        record.rumble = value == "true";
                        crate::ini::write(&self.vz_wad, &[("Joystick", "Rumble", (record.rumble as i32).to_string())])?;
                    }
                    _ => {
                        record.joy_invert = value == "true";
                        crate::ini::write(&self.vz_wad, &[("Joystick", "Invert", (record.joy_invert as i32).to_string())])?;
                    }
                }
            }
            // `FUN_005C17B0`.
            "LTIInputGeneralInvertMouse" => {
                self.mouse_invert_y = int()? == 1;
                self.save_mouse()?;
            }
            // `FUN_005C17F0`.
            "LTIInputGeneralMouseSense" => {
                let v: f32 = text()?.trim().parse().map_err(|e| format!("{name}({arg:?}): {e}"))?;
                self.mouse_sensitivity = ((v * SLIDER_TO_STEP) as i32).clamp(1, 20) as f32 * MOUSE_SCALE;
                self.save_mouse()?;
            }
            // `FUN_005C18B0`.
            "LTIInputGeneralRumble" => {
                record.rumble = text()?.trim().eq_ignore_ascii_case("true");
                crate::ini::write(&self.vz_wad, &[("Joystick", "Rumble", (record.rumble as i32).to_string())])?;
            }
            // `FUN_005C1900`.
            "LTIInputKMEnter" => {
                reply.invoke("LTIInputMaxActionBind", vec![FlashArg::Number(ROWS as f64), FlashArg::Nil]);
                for row in 0..ROWS {
                    reply.invoke("LTIInputKMSetActionName", vec![FlashArg::Number(row as f64), FlashArg::Text(ACTION_TEXT[row].into())]);
                    self.staging[row] = self.keys[row];
                    self.key_map(&mut reply, row);
                }
                reply.invoke("LTIInputKMUpdateTable", vec![]);
                self.km_page = true;
            }
            // `FUN_005C1CC0`.
            "LTIInputKMChangeInput" => {
                if !self.waiting {
                    self.editing = int()?;
                    self.armed = true;
                    self.waiting = true;
                }
            }
            // `FUN_005C1D00`, then the writer (`FUN_004FBBA0`, run from its copy at `0x006D4890`).
            "LTIInputKMApplyChanges" => {
                self.keys[..ROWS].copy_from_slice(&self.staging[..ROWS]);
                self.keys[PDA_COPY_ROW] = self.staging[PDA_ROW];
                self.save_keys()?;
                reply.effects.push(Effect::Bindings);
            }
            // `FUN_005C1ED0`.
            "LTIInputKMDefault" => {
                for row in 0..ROWS {
                    self.staging[row] = DEFAULT_KEYS[row];
                    self.key_map(&mut reply, row);
                }
                reply.invoke("LTIInputKMUpdateTable", vec![]);
            }
            // `FUN_005C2720`.
            "LTIInputKMCancelInput" => {
                self.waiting = false;
                self.editing = 0;
            }
            // `FUN_005C2950`.
            "LTIInputKMExit" => self.km_page = false,
            // `FUN_005C2730`.
            "LTIOverBoundResponse" => {
                if !self.over_bound.set() {
                    return Ok(Some(reply));
                }
                let o = self.over_bound;
                let (row, column) = (o.row as usize, o.column as usize);
                if int()? == 1 {
                    self.staging[row][column] = o.key as u16;
                    self.staging[o.other_row as usize][o.other_column as usize] = keys::NULL;
                } else {
                    self.staging[row][column] = self.backup[row][column];
                }
                for column in 0..2 {
                    for row in 0..ROWS {
                        self.key_map_column(&mut reply, row, column);
                    }
                }
                reply.invoke("LTIFinishedGettingInput", vec![FlashArg::Bool(false)]);
                self.waiting = false;
                self.over_bound = OverBound::UNSET;
            }
            // `FUN_005C2970`.
            "LTIInputJoystickEnter" => {
                reply.invoke("LTIInputMaxActionBind", vec![FlashArg::Number(25.0), FlashArg::Nil]);
                self.joystick_page = true;
                reply.invoke("LTIInputNumberControllers", vec![FlashArg::Number(self.devices.len() as f64)]);
                if self.devices.is_empty() {
                    self.joystick_defaults(&mut reply)?;
                } else {
                    for (k, device) in self.devices.iter().enumerate() {
                        reply.invoke("LTIInputControllerNames", vec![FlashArg::Number(k as f64), FlashArg::Text(device.name.clone())]);
                    }
                    let binds = self.device()?.binds;
                    for row in 0..25 {
                        reply.invoke("LTIInputKMSetActionName", vec![FlashArg::Number(row as f64), FlashArg::Text(ACTION_TEXT[row].into())]);
                        self.pad_staging[row] = binds[pad_slot(row)];
                        reply.invoke("LTIInputJoystickMap", vec![FlashArg::Number(row as f64), FlashArg::Text(pad_display(self.pad_staging[row])?.into())]);
                    }
                    reply.invoke("LTIInputJoystickUpdateTable", vec![]);
                }
            }
            // `FUN_005C2CB0`: the pad picked becomes the one edited (`0x005C04C0`, run in the VM and not
            // read), then its binds are shown again, edits not applied dropped.
            "LTIInputJoystickChangePrimary" => {
                let index = int()?;
                if index != 0 {
                    return Err(format!("{name}({index}): making another pad the first (0x005C04C0) is not read"));
                }
                let binds = self.device()?.binds;
                self.pad_staging = std::array::from_fn(|row| binds[pad_slot(row)]);
                self.joystick_map(&mut reply)?;
                reply.invoke("LTIInputJoystickUpdateTable", vec![]);
            }
            // `FUN_005C2E50`: the row; the pad's next inputs answer it (`FUN_004FB270`).
            "LTIInputJoystickChangeInput" => {
                if !self.waiting {
                    self.editing = int()?;
                    self.waiting = true;
                }
            }
            // `FUN_005C2720`, the keyboard page's cancel.
            "LTIInputJoystickCancel" => {
                self.waiting = false;
                self.editing = 0;
            }
            // `FUN_005C33F0`, then the save (`FUN_004FBC60`).
            "LTIInputJoystickApplyChanges" => {
                let staging = self.pad_staging;
                let device = self.devices.first_mut().ok_or("the joystick page needs a pad attached")?;
                for (row, &input) in staging.iter().enumerate() {
                    if input <= PAD_EMPTY {
                        device.binds[pad_slot(row)] = input;
                    }
                }
                self.save_pad()?;
                self.joystick_page = false;
                reply.effects.push(Effect::Bindings);
            }
            // `FUN_005C3500`.
            "LTIInputJoystickDefault" => self.joystick_defaults(&mut reply)?,
            // `FUN_005C3690`, `FUN_005C36B0`.
            "LTIInputJoystickExit" => self.joystick_page = false,
            "LTIInputJoystickReEnter" => self.joystick_page = true,
            // `FUN_005C3200`: yes clears the row that already had the input; anything else puts the
            // staging binds back.
            "LTIJoystickOverBoundResponse" => {
                let answer = match text().and_then(|_| int()) {
                    Ok(answer) => answer,
                    Err(_) => {
                        self.pad_staging = self.pad_backup;
                        return Ok(Some(reply));
                    }
                };
                if answer == 1 {
                    if let Some(&(_, i, j)) = self.pad_doubles.first() {
                        let owner = if self.pad_backup[i] == self.pad_staging[i] { i } else { j };
                        self.pad_staging[owner] = PAD_EMPTY;
                    }
                    self.joystick_map(&mut reply)?;
                } else {
                    self.pad_staging = self.pad_backup;
                }
                self.pad_doubles.clear();
                reply.invoke("LTIFinishedGettingJoystickInput", vec![FlashArg::Bool(false)]);
            }
            _ => return Ok(None),
        }
        Ok(Some(reply))
    }

    /// `LTIInputKMKeyMap` for both columns of `row` of the staging binds.
    fn key_map(&self, reply: &mut Reply, row: usize) {
        self.key_map_column(reply, row, 0);
        self.key_map_column(reply, row, 1);
    }

    fn key_map_column(&self, reply: &mut Reply, row: usize, column: usize) {
        let name = keys::display(self.staging[row][column]);
        reply.invoke("LTIInputKMKeyMap", vec![FlashArg::Number(row as f64), FlashArg::Bool(column == 1), FlashArg::Text(name.into())]);
    }

    /// `FUN_005C2110`: `key` fills the slot being edited, unless it is ESCAPE, which gives up. A key
    /// another row has asks whether to move it there; a key in both columns of a row stays only in the
    /// one being edited.
    fn rebind(&mut self, key: i32) -> Result<Reply, String> {
        let mut reply = Reply::default();
        let row = if self.editing >= 100 { self.editing - 100 } else { self.editing };
        if key == keys::ESCAPE as i32 {
            reply.invoke("LTIFinishedGettingInput", vec![FlashArg::Bool(false)]);
            self.waiting = false;
            return Ok(reply);
        }
        let row = usize::try_from(row).ok().filter(|&r| r < ROWS).ok_or_else(|| format!("rebind of slot {}: no such row", self.editing))?;
        let alternate = self.editing >= 100;
        self.backup = self.staging;
        self.staging[row][alternate as usize] = key as u16;
        let mut found = 0;
        for i in 0..ROWS {
            let mut primary = false;
            if self.staging[i][0] as i32 == key {
                if i != row {
                    self.over_bound.other_row = i as i32;
                    self.over_bound.other_column = 0;
                }
                found += 1;
                primary = true;
            }
            if self.staging[i][1] as i32 == key {
                if primary {
                    self.staging[i][if alternate { 0 } else { 1 }] = keys::NULL;
                } else {
                    if i != row {
                        self.over_bound.other_row = i as i32;
                        self.over_bound.other_column = 1;
                    }
                    found += 1;
                }
            }
        }
        if found == 2 {
            self.over_bound.row = row as i32;
            self.over_bound.column = alternate as i32;
            self.over_bound.key = key;
            let format = self.strings.get(&OVER_BOUND_TEXT).ok_or_else(|| format!("the string table has no [0x{OVER_BOUND_TEXT:08x}]"))?;
            let owner = CONFLICT_TEXT[self.over_bound.other_row as usize];
            let text = substitute(format, &[keys::display(key as u16), owner, CONFLICT_TEXT[row]]);
            reply.invoke(
                "LTIonlineMsgBox",
                vec![
                    FlashArg::Text(OVER_BOUND_TITLE.into()),
                    FlashArg::Text(text),
                    FlashArg::Number(2.0),
                    FlashArg::Text(YES.into()),
                    FlashArg::Text(NO.into()),
                    FlashArg::Number(1.0),
                ],
            );
        } else {
            self.key_map(&mut reply, row);
        }
        reply.invoke("LTIFinishedGettingInput", vec![FlashArg::Bool(key == keys::ENTER as i32 || key == keys::SPACE as i32)]);
        self.waiting = false;
        Ok(reply)
    }
}

/// `format` with each `%s` in turn taking the next of `args`, as the wide `sprintf` behind `0x005C5950`
/// fills it.
fn substitute(format: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = format;
    while let Some(at) = rest.find("%s") {
        out.push_str(&rest[..at]);
        match args.next() {
            Some(arg) => out.push_str(arg),
            None => out.push_str("%s"),
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doubled_key_stays_on_the_earlier_bind() {
        let mut keys = DEFAULT_KEYS;
        keys[6][0] = 0x11; // Reload on W, which Forward has
        keys[2][1] = keys::ESCAPE;
        assert!(resolve_conflicts(&mut keys));
        assert_eq!((keys[0][0], keys[6][0], keys[2][1]), (0x11, keys::NULL, keys::NULL));
    }

    #[test]
    fn the_wheel_gives_way_to_a_wheel_direction() {
        let mut keys = DEFAULT_KEYS;
        keys[10][0] = keys::MWUP; // Use on the wheel turning up; PrimarySwitch's alternate is MWHEEL
        assert!(resolve_conflicts(&mut keys));
        assert_eq!((keys[10][0], keys[5][1]), (keys::MWUP, keys::NULL));
    }

    #[test]
    fn the_built_in_binds_have_no_conflict() {
        let mut keys = DEFAULT_KEYS;
        assert!(!resolve_conflicts(&mut keys));
    }

    fn controls() -> (Controls, Record) {
        let strings = [(OVER_BOUND_TEXT, "The key %s is currently bound to %s, would you like to bind it to %s instead?".to_string())].into();
        let mut c = Controls::detached(strings);
        let mut record = crate::options::tests::record();
        c.call("LTIInputKMEnter", None, &mut record).unwrap();
        (c, record)
    }

    fn names(reply: &Reply) -> Vec<&'static str> {
        reply.invokes.iter().map(|(n, _)| *n).collect()
    }

    #[test]
    fn a_free_key_fills_the_slot() {
        let (mut c, mut record) = controls();
        c.call("LTIInputKMChangeInput", Some("6"), &mut record).unwrap();
        assert!(c.capturing());
        let reply = c.key_down(0x10).unwrap();
        assert!(!c.capturing());
        assert_eq!(c.staging[6], [0x10, 0]);
        assert_eq!(names(&reply), ["LTIInputKMKeyMap", "LTIInputKMKeyMap", "LTIFinishedGettingInput"]);
        assert_eq!(reply.invokes[0].1[2], FlashArg::Text("Q".into()));
        assert_eq!(reply.invokes[2].1, vec![FlashArg::Bool(false)]);
    }

    #[test]
    fn a_bound_key_asks_and_no_puts_the_slot_back() {
        let (mut c, mut record) = controls();
        c.call("LTIInputKMChangeInput", Some("6"), &mut record).unwrap();
        let reply = c.key_down(0x22).unwrap();
        assert_eq!(names(&reply), ["LTIonlineMsgBox", "LTIFinishedGettingInput"]);
        assert_eq!(reply.invokes[0].1[1], FlashArg::Text("The key G is currently bound to [0xf418e84c], would you like to bind it to [0x741cbf9d] instead?".into()));
        c.call("LTIOverBoundResponse", Some("0"), &mut record).unwrap();
        assert_eq!((c.staging[6], c.staging[7]), (DEFAULT_KEYS[6], DEFAULT_KEYS[7]));
        assert_eq!(c.over_bound, OverBound::UNSET);
    }

    #[test]
    fn yes_moves_the_key() {
        let (mut c, mut record) = controls();
        c.call("LTIInputKMChangeInput", Some("6"), &mut record).unwrap();
        c.key_down(0x22).unwrap();
        c.call("LTIOverBoundResponse", Some("1"), &mut record).unwrap();
        assert_eq!((c.staging[6][0], c.staging[7][1]), (0x22, keys::NULL));
    }

    #[test]
    fn escape_gives_up_and_enter_tells_the_movie() {
        let (mut c, mut record) = controls();
        c.call("LTIInputKMChangeInput", Some("100"), &mut record).unwrap();
        let reply = c.key_down(keys::ESCAPE).unwrap();
        assert_eq!(reply.invokes, vec![("LTIFinishedGettingInput", vec![FlashArg::Bool(false)])]);
        assert_eq!(c.staging[0], DEFAULT_KEYS[0]);
        c.call("LTIInputKMChangeInput", Some("110"), &mut record).unwrap();
        let reply = c.key_down(keys::ENTER).unwrap();
        assert_eq!(c.staging[10][1], keys::ENTER);
        assert_eq!(reply.invokes.last().unwrap().1, vec![FlashArg::Bool(true)]);
    }

    #[test]
    fn a_key_in_both_columns_stays_in_the_one_edited() {
        let (mut c, mut record) = controls();
        c.call("LTIInputKMChangeInput", Some("100"), &mut record).unwrap();
        c.key_down(0x11).unwrap();
        assert_eq!(c.staging[0], [keys::NULL, 0x11]);
    }

    /// 9 × 0.02 × 50 comes back 9 (the install's `Sensitivity=9` saves unchanged), but 5 × 0.02 is a
    /// rounding tie that lands on 0.099999994, and × 50 cuts to 4.
    #[test]
    fn mouse_steps_save_through_float() {
        let mut c = Controls::detached(HashMap::new());
        let saved = |c: &mut Controls, step: i32| {
            c.mouse_sensitivity = step as f32 * MOUSE_SCALE;
            c.mouse_step()
        };
        assert_eq!((saved(&mut c, 9), saved(&mut c, 5)), (9, 4));
    }

    /// Controls with an Xbox 360 pad (layout 4) and the joystick page open.
    fn joystick() -> (Controls, Record) {
        let mut c = Controls::detached(HashMap::new());
        c.devices = vec![Device { name: "Controller (XBOX 360 For Windows)".into(), layout: 4, binds: PAD_LAYOUTS[4] }];
        let mut record = crate::options::tests::record();
        let reply = c.call("LTIInputJoystickEnter", None, &mut record).unwrap().unwrap();
        // Jump (row 11) is on Button 1 in the Xbox 360 layout, as the installed INI has it.
        assert!(reply.invokes.contains(&("LTIInputJoystickMap", vec![FlashArg::Number(11.0), FlashArg::Text("[0xe3c445cf]".into())])));
        (c, record)
    }

    #[test]
    fn the_xbox_360_layout_is_the_installed_sections() {
        let names: Vec<&str> = (0..25).map(|row| PAD_NAMES[PAD_LAYOUTS[4][pad_slot(row)] as usize]).collect();
        assert_eq!(names[4], "Other Left Stick - Right"); // PrimaryAttack
        assert_eq!(names[11], "Button 1"); // Jump
        assert_eq!(names[22], "EMPTY"); // Walk
        assert_eq!(names[24], "Button 8"); // Start
    }

    #[test]
    fn a_free_pad_input_fills_the_row() {
        let (mut c, mut record) = joystick();
        c.call("LTIInputJoystickChangeInput", Some("22"), &mut record).unwrap();
        let reply = c.pad_frame(&[0x1F]).unwrap();
        assert_eq!(names(&reply), ["LTIInputJoystickMap", "LTIFinishedGettingJoystickInput", "LTIInputSeeButtonsPushed"]);
        assert_eq!(c.pad_staging[22], 0x1F);
    }

    #[test]
    fn a_bound_pad_input_asks_and_yes_clears_the_owner() {
        let (mut c, mut record) = joystick();
        c.call("LTIInputJoystickChangeInput", Some("22"), &mut record).unwrap();
        let reply = c.pad_frame(&[0x14]).unwrap();
        assert_eq!(reply.invokes[0].0, "LTIonlineMsgBox");
        assert_eq!(reply.invokes[0].1[1], FlashArg::Text("[0x603e1c7d:[0xe3c445cf]:[0xfda32c92]:[0xea5788f1]]".into()));
        c.call("LTIJoystickOverBoundResponse", Some("1"), &mut record).unwrap();
        assert_eq!((c.pad_staging[11], c.pad_staging[22]), (PAD_EMPTY, 0x14));
    }

    #[test]
    fn two_pad_inputs_at_once_are_refused() {
        let (mut c, mut record) = joystick();
        c.call("LTIInputJoystickChangeInput", Some("22"), &mut record).unwrap();
        let reply = c.pad_frame(&[0x14, 0x15]).unwrap();
        assert_eq!(reply.invokes[0].1[0], FlashArg::Text(PAD_TOO_MANY_TITLE.into()));
        assert_eq!(c.pad_staging[22], PAD_EMPTY);
        assert!(!c.waiting);
    }

    #[test]
    fn held_pad_inputs_show_while_the_page_is_open() {
        let (mut c, _) = joystick();
        let reply = c.pad_frame(&[PAD_LEFT_TRIGGER, 0x14]).unwrap();
        assert_eq!(reply.invokes, vec![("LTIInputSeeButtonsPushed", vec![FlashArg::Text("[0x83ecba77], [0xe3c445cf]".into())])]);
    }

    #[test]
    fn a_doubled_pad_input_keeps_its_first_slot() {
        let mut binds = PAD_LAYOUTS[4];
        binds[20] = binds[2];
        dedupe_pad(&mut binds);
        assert_eq!((binds[2], binds[20]), (PAD_LAYOUTS[4][2], PAD_EMPTY));
    }

    /// 3 × 0.05 + 0.5 is 0.64999998 as a float, and (0.64999998 − 0.5) × 20 cuts to 2: retail's own
    /// arithmetic saves joystick step 3 as 2.
    #[test]
    fn joystick_step_3_saves_as_2() {
        let mut c = Controls::detached(HashMap::new());
        c.joy_sensitivity = 3.0 * JOY_SCALE + JOY_OFFSET;
        assert_eq!(c.joy_step(), 2);
    }
}
