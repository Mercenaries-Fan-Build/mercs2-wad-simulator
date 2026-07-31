//! Guards on the vendored Lua tree.
//!
//! `tools/lua51-mercs2/README.md` declares the patches to be the single source of truth, and
//! `vendor/` to be upstream `tools/lua51-src/src` with those patches applied. Nothing enforced
//! that, so the two could drift silently — and a drifted VM is the kind of failure that shows up
//! as a mod that builds fine and crashes the game.
//!
//! Rather than shell out to `patch` (absent on a stock Windows box), these assert the *invariant*
//! the patches exist to produce:
//!
//! * every file the patches do NOT touch is byte-identical to upstream, modulo line endings;
//! * the three files they DO touch carry each documented change;
//! * `LUA_COMPAT_VARARG` is on.
//!
//! The last one is not from a patch — it is stock 5.1 config — but two corpus scripts depend on it
//! and it is exactly the sort of compat define a future tidy-up would switch off. See
//! `sys::tests::implicit_vararg_arg_table_exists`, which proves the runtime behaviour; this proves
//! the intent is written down.

use std::path::{Path, PathBuf};

/// Upstream lives outside the crate, so a published `.crate` tarball will not contain it. Tests
/// only ever run in-tree, but skip loudly rather than fail if it is genuinely absent.
fn upstream_dir() -> Option<PathBuf> {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../lua51-src/src");
    d.is_dir().then(|| d)
}

fn vendor_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor")
}

/// Compare ignoring line endings — the vendored copy is CRLF, upstream is LF, and that difference
/// is not a code change.
fn same_ignoring_line_endings(a: &str, b: &str) -> bool {
    a.replace("\r\n", "\n") == b.replace("\r\n", "\n")
}

/// The only files any patch is allowed to touch.
const PATCHED: [&str; 3] = ["luaconf.h", "ldump.c", "lundump.c"];

#[test]
fn vendor_is_upstream_plus_exactly_the_three_patched_files() {
    let Some(upstream) = upstream_dir() else {
        eprintln!("skipping: tools/lua51-src/src not present");
        return;
    };
    let vendor = vendor_dir();

    let mut checked = 0usize;
    let mut unexpected = Vec::new();

    for entry in std::fs::read_dir(&vendor).expect("read vendor/") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !(name.ends_with(".c") || name.ends_with(".h")) || PATCHED.contains(&name.as_str()) {
            continue;
        }
        let up = upstream.join(&name);
        assert!(up.is_file(), "{name} is in vendor/ but not upstream — where did it come from?");

        let a = std::fs::read_to_string(&up).expect("read upstream");
        let b = std::fs::read_to_string(&path).expect("read vendor");
        if !same_ignoring_line_endings(&a, &b) {
            unexpected.push(name);
        }
        checked += 1;
    }

    assert!(checked > 20, "expected the full Lua tree, only compared {checked} files");
    assert!(
        unexpected.is_empty(),
        "these vendored files differ from upstream but no patch claims them: {unexpected:?}. \
         Either the change belongs in tools/lua51-mercs2/patches/ or it should not be there."
    );
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
