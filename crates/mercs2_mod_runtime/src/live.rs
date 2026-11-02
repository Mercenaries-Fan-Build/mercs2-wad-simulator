//! The `M2Live` Lua namespace: reads of the running game that Lua has no binding for. It only reads, and
//! the reads themselves are `mercs2_formats::live_reads`.
//!
//! - `M2Live.CameraEye(camera)`, where `camera` is `Player.GetCamera`'s value: `x, y, z`.
//! - `M2Live.Residency()`: registered records, resident blocks.
//! - `M2Live.AssetStatus("name:type,…")`, hexadecimal: `"status:refs,…"` in order, `-` for an absent one.
//! - `M2Live.Ticks()`: the Lua host's ticks with a state, with none, and from the frame.
//!
//! A read that cannot be made raises a Lua error naming why, so a caller's `pcall` sees it. It is never
//! a zero.

use crate::lua_host::{self, LuaReg, m2_lua_add_namespace, m2_lua_push_number, m2_lua_push_string, m2_lua_raise};
use crate::{luastack, rt_log, sites};
use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::Ordering;
use mercs2_formats::exe_image::Image;
use mercs2_formats::live_reads::{self, MANAGER_OPERAND, MANAGER_PATTERN};
use mercs2_formats::lua_sites::Pattern;
use mercs2_formats::lua_stack::{self, LUA_TSTRING, Mem, TSTRING_DATA, TSTRING_LEN, TVALUE_SIZE, TVALUE_TT};
use std::ffi::CString;
use std::sync::OnceLock;

const LUA_TLIGHTUSERDATA: u32 = 2;
const LUA_TUSERDATA: u32 = 7;
/// Where a full userdata holds the value the engine reads (`FUN_0059FF50`).
const USERDATA_VALUE: u32 = 0x18;

/// The most a pair list can hold: a Shipment's assets, read on a game frame.
const MAX_PAIRS: usize = 256;
const MAX_LIST: u32 = 64 * 1024;

fn raise(l: *mut c_void, message: &str) -> ! {
    let text = CString::new(message.replace('\0', " ")).expect("NULs replaced");
    unsafe { m2_lua_raise(l, text.as_ptr()) }
}

/// The address of the manager pointer, found once.
fn manager_slot() -> Result<u32, String> {
    static SLOT: OnceLock<Result<u32, String>> = OnceLock::new();
    SLOT.get_or_init(|| {
        let image: Image<'static> = sites::mapped()?;
        let text = image.section(".text").ok_or("the game has no .text")?;
        let pattern = Pattern::parse(MANAGER_PATTERN).expect("the pattern parses");
        let hits = pattern.find_all(text.bytes);
        let [at] = hits[..] else {
            return Err(format!("the asset manager's signature matched {} times, not once", hits.len()));
        };
        let o = at + MANAGER_OPERAND;
        let slot = u32::from_le_bytes([text.bytes[o], text.bytes[o + 1], text.bytes[o + 2], text.bytes[o + 3]]);
        rt_log::line(&format!("asset manager pointer at {slot:#x}"));
        Ok(slot)
    })
    .clone()
}

/// The frame of the call, with at least `n` arguments.
fn frame(l: *mut c_void, n: u32) -> lua_stack::Frame {
    match lua_stack::resolve(&luastack::Process, l as u32) {
        Some(f) if f.nargs >= n => f,
        _ if n == 0 => lua_stack::Frame { base: 0, nargs: 0 },
        _ => raise(l, &format!("M2Live: expected {n} argument(s)")),
    }
}

unsafe extern "C" fn camera_eye(l: *mut c_void) -> c_int {
    let f = frame(l, 1);
    let m = luastack::Process;
    // As the engine's own argument reader takes one (`FUN_0059FF50`).
    let camera = match m.read_u32(f.base + TVALUE_TT) {
        LUA_TLIGHTUSERDATA => m.read_u32(f.base),
        LUA_TUSERDATA => {
            let u = m.read_u32(f.base);
            if m.readable_span(u + USERDATA_VALUE) < 4 {
                raise(l, "M2Live.CameraEye: the userdata is not readable");
            }
            m.read_u32(u + USERDATA_VALUE)
        }
        _ => raise(l, "M2Live.CameraEye: the argument is not Player.GetCamera's value"),
    };
    match live_reads::camera_eye(&m, camera) {
        Ok(eye) => {
            for c in eye {
                m2_lua_push_number(l, f64::from(c));
            }
            3
        }
        Err(e) => raise(l, &format!("M2Live.CameraEye: {e}")),
    }
}

unsafe extern "C" fn residency(l: *mut c_void) -> c_int {
    let read = manager_slot().and_then(|s| live_reads::stats(&luastack::Process, s));
    match read {
        Ok(s) => {
            m2_lua_push_number(l, f64::from(s.records));
            m2_lua_push_number(l, f64::from(s.blocks));
            2
        }
        Err(e) => raise(l, &format!("M2Live.Residency: {e}")),
    }
}

unsafe extern "C" fn ticks(l: *mut c_void) -> c_int {
    for counter in [&lua_host::TICKS_STATE, &lua_host::TICKS_NULL, &lua_host::TICKS_FRAME] {
        m2_lua_push_number(l, f64::from(counter.load(Ordering::Relaxed)));
    }
    3
}

/// The string argument at slot `i`.
fn string_arg(l: *mut c_void, f: &lua_stack::Frame, i: u32) -> String {
    let m = luastack::Process;
    let slot = f.base + i * TVALUE_SIZE;
    if m.read_u32(slot + TVALUE_TT) != LUA_TSTRING {
        raise(l, "M2Live.AssetStatus: the argument is not a string");
    }
    let s = m.read_u32(slot);
    let len = m.read_u32(s + TSTRING_LEN);
    if len > MAX_LIST || m.readable_span(s + TSTRING_DATA) < len {
        raise(l, "M2Live.AssetStatus: the list is too long or unreadable");
    }
    (0..len).map(|i| char::from(m.read_u8(s + TSTRING_DATA + i))).collect()
}

unsafe extern "C" fn asset_status(l: *mut c_void) -> c_int {
    let f = frame(l, 1);
    let text = string_arg(l, &f, 0);
    let m = luastack::Process;
    let read = manager_slot().and_then(|slot| {
        let pairs = live_reads::parse_pairs(&text)?;
        if pairs.len() > MAX_PAIRS {
            return Err(format!("{} assets asked of one frame, over {MAX_PAIRS}", pairs.len()));
        }
        pairs.iter().map(|&(n, t)| live_reads::asset(&m, slot, n, t)).collect::<Result<Vec<_>, _>>()
    });
    match read {
        Ok(assets) => {
            let out = live_reads::render_assets(&assets);
            unsafe { m2_lua_push_string(l, out.as_ptr().cast::<c_char>(), out.len() as c_int) };
            1
        }
        Err(e) => raise(l, &format!("M2Live.AssetStatus: {e}")),
    }
}

/// Add the namespace. The table lives for the process.
pub(crate) fn register() {
    static NAMES: [&[u8]; 4] = [b"CameraEye\0", b"Residency\0", b"AssetStatus\0", b"Ticks\0"];
    let fns: &'static [LuaReg] = Box::leak(Box::new([
        LuaReg { name: NAMES[0].as_ptr().cast(), func: Some(camera_eye) },
        LuaReg { name: NAMES[1].as_ptr().cast(), func: Some(residency) },
        LuaReg { name: NAMES[2].as_ptr().cast(), func: Some(asset_status) },
        LuaReg { name: NAMES[3].as_ptr().cast(), func: Some(ticks) },
        LuaReg { name: core::ptr::null(), func: None },
    ]));
    if unsafe { m2_lua_add_namespace(c"M2Live".as_ptr(), fns.as_ptr()) } == 0 {
        rt_log::line("ERROR the M2Live namespace was not added");
    }
}
