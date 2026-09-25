//! The PE header checks for native code a Shipment places in the game folder.
//!
//! Two kinds ship a Windows DLL: `native_hook` (an `.asi` plugin the loader `LoadLibrary`s from
//! `scripts/`) and `add_runtime_dll` (a DLL in the game root that plugins import by name). Both are
//! loaded by `LoadLibrary` into `Mercenaries2.exe`, a 32-bit process, so both must be i386 PE DLLs.
//! One check serves both; only the kind named in the message differs.
//!
//! Nothing here reads an import table. Declaring every dependency is the Shipment author's job.

/// `IMAGE_FILE_MACHINE_I386`. The game is a 32-bit process, so a 64-bit DLL cannot load into it.
const PE_MACHINE_I386: u16 = 0x014C;
/// `IMAGE_FILE_DLL`. `LoadLibrary` will not run an executable image.
const PE_CHARACTERISTICS_DLL: u16 = 0x2000;

/// Why the game could not load `bytes` as a DLL, or `None` when the header says it can.
///
/// `kind` is the contribution kind that ships the file (`native_hook`, `add_runtime_dll`), named in
/// the message so the author knows which contribution to fix.
///
/// A load failure is observable in `pmc_blackbox.log` as `[FAILED] … (error: …)`, but only to
/// someone who knows to look. None of these is recoverable at deploy time, and all are cheap to see
/// here.
pub fn pe_dll_load_blocker(bytes: &[u8], kind: &str) -> Option<String> {
    if bytes.len() < 0x40 || &bytes[0..2] != b"MZ" {
        return Some("it is not a PE image at all (no `MZ` header)".into());
    }
    let pe_at = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    if pe_at + 24 > bytes.len() || &bytes[pe_at..pe_at + 4] != b"PE\0\0" {
        return Some("its `e_lfanew` does not point at a `PE\\0\\0` signature".into());
    }
    let coff = pe_at + 4;
    let machine = u16::from_le_bytes([bytes[coff], bytes[coff + 1]]);
    let characteristics = u16::from_le_bytes([bytes[coff + 18], bytes[coff + 19]]);
    if machine != PE_MACHINE_I386 {
        return Some(format!(
            "it is built for machine 0x{machine:04X}, not i386 (0x{PE_MACHINE_I386:04X}). \
             Mercenaries2.exe is a 32-bit process and `LoadLibrary` refuses a foreign architecture"
        ));
    }
    if characteristics & PE_CHARACTERISTICS_DLL == 0 {
        return Some(format!(
            "its COFF characteristics do not set `IMAGE_FILE_DLL`, so it is an executable image \
             rather than a DLL. A {kind} file is loaded with `LoadLibrary`, which needs a DLL"
        ));
    }
    None
}
