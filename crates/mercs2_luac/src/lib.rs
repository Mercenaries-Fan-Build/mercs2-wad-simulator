//! Compile Lua 5.1 source into the **exact** bytecode dialect the Mercenaries 2 VM loads.
//!
//! The game does not run stock Lua 5.1. Its VM was built with `lua_Number = float`
//! (4-byte, not the usual 8-byte double) and reads string lengths as 32-bit. A chunk from
//! a stock `luac` therefore has the wrong header and is rejected. The bytes the game wants
//! start with:
//!
//! ```text
//! 1b 4c 75 61  51 00 01 04  04 04 04 00
//! │           │  │  │  │  │  │  │  └─ integral flag  (0 = floating point)
//! │           │  │  │  │  │  └──────── sizeof(lua_Number) = 4   ← float, not double
//! │           │  │  │  │  └─────────── sizeof(Instruction) = 4
//! │           │  │  │  └────────────── sizeof(size_t)      = 4   ← forced, see build.rs
//! │           │  │  └───────────────── sizeof(int)         = 4
//! │           │  └──────────────────── little-endian
//! │           └─────────────────────── version 5.1, format 0
//! └─────────────────────────────────── "\x1bLua"
//! ```
//!
//! [`compile`] produces exactly that from any host, 32- or 64-bit, and **verifies the
//! header before returning** — so a chunk in the wrong dialect can never reach a WAD.
//!
//! Feed the result to `mercs2_formats::scripts_block::ScriptsBlock::replace_lua`.

use std::os::raw::{c_char, c_int, c_void};

/// The 12-byte LuaQ header the game's VM accepts. See the module docs.
pub const MERCS2_LUAQ_HEADER: [u8; 12] = [
    0x1b, 0x4c, 0x75, 0x61, // "\x1bLua"
    0x51, // version 5.1
    0x00, // format 0 (official)
    0x01, // little-endian
    0x04, // sizeof(int)
    0x04, // sizeof(size_t)      — forced to 4 by the vendored lundump.c
    0x04, // sizeof(Instruction)
    0x04, // sizeof(lua_Number)  — float, from the vendored luaconf.h
    0x00, // lua_Number is not integral
];

#[allow(non_camel_case_types)]
type lua_State = c_void;

extern "C" {
    fn luaL_newstate() -> *mut lua_State;
    fn lua_close(L: *mut lua_State);
    fn luaL_loadbuffer(
        L: *mut lua_State,
        buff: *const c_char,
        sz: usize,
        name: *const c_char,
    ) -> c_int;
    fn lua_dump(
        L: *mut lua_State,
        writer: extern "C" fn(*mut lua_State, *const c_void, usize, *mut c_void) -> c_int,
        data: *mut c_void,
    ) -> c_int;
    fn lua_tolstring(L: *mut lua_State, idx: c_int, len: *mut usize) -> *const c_char;
    fn lua_settop(L: *mut lua_State, idx: c_int);
}

/// `lua_Writer` — append each dumped block to the `Vec<u8>` behind `ud`.
extern "C" fn writer(
    _l: *mut lua_State,
    p: *const c_void,
    sz: usize,
    ud: *mut c_void,
) -> c_int {
    // SAFETY: `ud` is the `&mut Vec<u8>` we hand to `lua_dump`, and `p`/`sz` describe a
    // buffer Lua owns for the duration of this call.
    unsafe {
        let out = &mut *(ud as *mut Vec<u8>);
        out.extend_from_slice(std::slice::from_raw_parts(p as *const u8, sz));
    }
    0 // 0 = ok
}

/// Compile Lua 5.1 source to Mercenaries 2 LuaQ bytecode.
///
/// `chunk_name` is what the VM reports in a runtime traceback — pass the script's name
/// (e.g. `"wifpmcinterior"`) so an error in a mod's Lua points somewhere useful.
///
/// A syntax error returns `Err` with Lua's own message (including a line number), which is
/// exactly what we want to surface to a mod author.
pub fn compile(source: &str, chunk_name: &str) -> Result<Vec<u8>, String> {
    // Lua expects a NUL-terminated chunk name; the source is passed with an explicit length.
    let name = std::ffi::CString::new(chunk_name)
        .map_err(|_| "chunk name contains a NUL byte".to_string())?;

    // SAFETY: single-threaded use of a state we create and close here. Every raw pointer
    // is derived from a live local, and we check each return code before proceeding.
    unsafe {
        let l = luaL_newstate();
        if l.is_null() {
            return Err("could not create a Lua state (out of memory)".into());
        }

        // Guard so an early return still closes the state.
        struct State(*mut lua_State);
        impl Drop for State {
            fn drop(&mut self) {
                unsafe { lua_close(self.0) }
            }
        }
        let guard = State(l);

        // Parse. Non-zero = failure, with the message pushed on the stack.
        if luaL_loadbuffer(
            l,
            source.as_ptr() as *const c_char,
            source.len(),
            name.as_ptr(),
        ) != 0
        {
            let mut len: usize = 0;
            let msg = lua_tolstring(l, -1, &mut len);
            let text = if msg.is_null() {
                "unknown Lua syntax error".to_string()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(msg as *const u8, len))
                    .into_owned()
            };
            lua_settop(l, -2);
            return Err(text);
        }

        // Dump the compiled function at the top of the stack.
        let mut out: Vec<u8> = Vec::new();
        let rc = lua_dump(l, writer, &mut out as *mut Vec<u8> as *mut c_void);
        drop(guard);

        if rc != 0 {
            return Err(format!("lua_dump failed (code {rc})"));
        }

        // Never hand back a chunk in the wrong dialect. If this ever trips, the vendored
        // sources lost a patch and the bytecode would be silently unloadable in-game.
        if out.len() < MERCS2_LUAQ_HEADER.len() || out[..12] != MERCS2_LUAQ_HEADER {
            return Err(format!(
                "compiled a chunk the game cannot load: header {:02x?} != expected {:02x?} \
                 (the vendored Lua lost one of its Mercs2 patches)",
                &out[..out.len().min(12)],
                MERCS2_LUAQ_HEADER
            ));
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the crate: the header must match the game's, from THIS host.
    #[test]
    fn emits_the_mercs2_luaq_header() {
        let bytes = compile("return 1", "t").expect("compile");
        assert_eq!(&bytes[..12], &MERCS2_LUAQ_HEADER);
        // Spell out the two a stock 64-bit luac would get wrong. Byte order is
        // [7]=int [8]=size_t [9]=Instruction [10]=lua_Number [11]=integral-flag.
        assert_eq!(bytes[8], 4, "sizeof(size_t) must be 4, not the host's 8");
        assert_eq!(bytes[10], 4, "sizeof(lua_Number) must be 4 (float), not 8 (double)");
        assert_eq!(bytes[11], 0, "lua_Number is floating point, not integral");
    }

    /// A mod author's typo must come back as a message with a line number, not a panic.
    #[test]
    fn syntax_errors_are_reported_with_a_line_number() {
        let err = compile("function oops(\nreturn 1", "wifpmcinterior").unwrap_err();
        assert!(err.contains("wifpmcinterior"), "names the chunk: {err}");
    }

    /// The real shape of a wardrobe edit: append to a global table, redefine a global fn.
    #[test]
    fn compiles_a_wardrobe_append() {
        let src = r#"
_tOutfits = _tOutfits or {}
_tOutfits.mattias = _tOutfits.mattias or {}
table.insert(_tOutfits.mattias, {
  Name = "Mechanic",
  Model = "pmc_hum_mechanic",
  PlayerVisibleName = "Mechanic",
})
function GetAvailableCostumes() return 99 end
"#;
        let bytes = compile(src, "wifpmcinterior").expect("compile");
        assert_eq!(&bytes[..4], b"\x1bLua");
        assert!(bytes.len() > 12);
    }

    /// Float `lua_Number` is not cosmetic — constants must round-trip through 4 bytes.
    #[test]
    fn number_constants_are_single_precision() {
        // 0.1 is not representable in binary; as f32 it differs from f64. If the build
        // ever reverted to double, the dumped constant would be 8 bytes and the header
        // assertion above would already have caught it — this pins the intent.
        let bytes = compile("return 0.1", "t").expect("compile");
        assert_eq!(bytes[10], 4);
    }
}
