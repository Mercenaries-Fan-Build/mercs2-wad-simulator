//! The PE header check shared by `native_hook` plugins and `add_runtime_dll` DLLs.
//!
//! The passing image is the committed fixture DLL (`fixtures/load_plan/.../lua_bridge.asi`, a
//! header-only i386 PE32 DLL whose generator `tests/compat.rs` checks). The failing ones are that
//! same image with one field changed, so each case differs from a known-good DLL in exactly the
//! field under test.

use mercs2_quartermaster::pe::pe_dll_load_blocker;
use std::path::Path;

fn fixture_dll() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/load_plan/shipments/lua-bridge/src/lua_bridge.asi");
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The COFF header's offset: `e_lfanew` plus the 4-byte `PE\0\0` signature.
fn coff(bytes: &[u8]) -> usize {
    u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize + 4
}

#[test]
fn i386_dll_passes() {
    assert_eq!(pe_dll_load_blocker(&fixture_dll(), "add_runtime_dll"), None);
}

#[test]
fn amd64_refused() {
    let mut bytes = fixture_dll();
    let c = coff(&bytes);
    bytes[c..c + 2].copy_from_slice(&0x8664u16.to_le_bytes());
    let why = pe_dll_load_blocker(&bytes, "add_runtime_dll").expect("amd64 must be refused");
    assert!(why.contains("0x8664") && why.contains("32-bit process"), "{why}");
}

/// An executable image (no `IMAGE_FILE_DLL`). The message names the kind that ships it.
#[test]
fn exe_image_refused() {
    let mut bytes = fixture_dll();
    let c = coff(&bytes);
    let characteristics = u16::from_le_bytes([bytes[c + 18], bytes[c + 19]]) & !0x2000;
    bytes[c + 18..c + 20].copy_from_slice(&characteristics.to_le_bytes());
    for kind in ["native_hook", "add_runtime_dll"] {
        let why = pe_dll_load_blocker(&bytes, kind).expect("an exe image must be refused");
        assert!(why.contains("IMAGE_FILE_DLL"), "{why}");
        assert!(why.contains(kind), "names the kind: {why}");
    }
}

#[test]
fn not_a_pe_image_refused() {
    let why = pe_dll_load_blocker(b"plain text, no MZ header", "native_hook").expect("refused");
    assert!(why.contains("not a PE image"), "{why}");
}
