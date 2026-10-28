//! Guards on the vendored Lua tree that read `vendor/` alone.
//!
//! `tools/lua51-mercs2/README.md` declares the patches to be the single source of truth, and
//! `vendor/` to be upstream `tools/lua51-src/src` with those patches applied. Nothing enforced
//! that, so the two could drift silently — and a drifted VM is the kind of failure that shows up
//! as a mod that builds fine and crashes the game.
//!
//! Rather than shell out to `patch` (absent on a stock Windows box), these assert the *invariant*
//! the patches exist to produce:
//!
//! * the three files they touch carry each documented change;
//! * `LUA_COMPAT_VARARG` is on.
//!
//! The other half — every file the patches do NOT touch is byte-identical to upstream, modulo line
//! endings — needs the upstream tree, so it lives in `vendor_upstream_retail.rs` (`retail` feature).
//!
//! `LUA_COMPAT_VARARG` is not from a patch — it is stock 5.1 config — but two corpus scripts depend
//! on it and it is exactly the sort of compat define a future tidy-up would switch off. See
//! `sys::tests::implicit_vararg_arg_table_exists`, which proves the runtime behaviour; this proves
//! the intent is written down.

use std::path::{Path, PathBuf};

fn vendor_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor")
}

#[test]
fn patch_01_makes_lua_number_a_float() {
    let src = std::fs::read_to_string(vendor_dir().join("luaconf.h")).expect("read luaconf.h");
    assert!(
        src.contains("#define LUA_NUMBER\tfloat"),
        "lua_Number must be float — this is the header byte the game's VM checks, and mlua-sys's \
         hardcoded c_double is why this crate exists"
    );
    assert!(src.contains("#define LUAI_UACNUMBER\tfloat"));
    // Without the -f variants the C library silently promotes through double, which defeats the
    // point of a single-precision VM.
    assert!(src.contains("strtof"), "lua_str2number must use strtof, not strtod");
    assert!(src.contains("floorf"), "luai_nummod must use floorf");
    assert!(src.contains("powf"), "luai_numpow must use powf");
}

#[test]
fn patch_02_dumps_32_bit_string_lengths() {
    let src = std::fs::read_to_string(vendor_dir().join("ldump.c")).expect("read ldump.c");
    assert!(src.contains("uint32_t size"), "DumpString must size strings as uint32_t");
    assert!(
        !src.contains("size_t size=s->tsv.len+1"),
        "the native-size_t DumpString is back — 64-bit hosts would emit 8-byte lengths"
    );
}

#[test]
fn patch_03_undumps_32_bit_string_lengths_and_forces_size_t_4() {
    let src = std::fs::read_to_string(vendor_dir().join("lundump.c")).expect("read lundump.c");
    assert!(src.contains("uint32_t size32"), "LoadString must read 4-byte lengths");
    assert!(
        src.contains("*h++=(char)4;"),
        "luaU_header must force sizeof(size_t)=4; without it a 64-bit host writes 8 and the \
         game rejects the chunk"
    );
    assert!(
        !src.contains("*h++=(char)sizeof(size_t);"),
        "the native sizeof(size_t) header byte is back"
    );
}

/// Not a patch — stock 5.1 config we must not lose.
///
/// `resident/mrxtaskjobdestroyset.lua` and `resident/mrxtaskjobverifyset.lua` both write
/// `function _AddTarget(self, ...)` then `arg[1]` / `unpack(arg)`. `arg` is a hidden local the VM
/// materialises per call (`lparser.c` sets `VARARG_NEEDSARG`, `ldo.c` builds the table), so it
/// cannot be shimmed from Lua. Turning this define off breaks those two scripts at runtime only.
#[test]
fn lua_compat_vararg_is_enabled() {
    let src = std::fs::read_to_string(vendor_dir().join("luaconf.h")).expect("read luaconf.h");
    let enabled = src
        .lines()
        .any(|l| l.trim_start().starts_with("#define LUA_COMPAT_VARARG"));
    assert!(
        enabled,
        "LUA_COMPAT_VARARG must stay defined — two corpus scripts use the implicit `arg` table, \
         and no Lua-level shim can replace a hidden local"
    );
}
