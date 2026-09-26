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

/// `IMAGE_NT_OPTIONAL_HDR32_MAGIC`. The game is 32-bit, so its optional header is PE32, not PE32+.
const PE32_MAGIC: u16 = 0x010B;

/// The `n` bytes of a PE32 image at absolute virtual address `va` — the address `native_hook`
/// `touches` are written in (image base included) — or `None` when `va` maps into no section, the
/// bytes are not backed by raw file data, or the image is not a readable PE32.
///
/// This is the mapping M0199 uses to compare a declared signature guard against the bytes actually
/// at that address in `Mercenaries2.exe`. PE32 only: a PE32+ (64-bit) optional header is refused
/// rather than misread, because the game is a 32-bit process and its addresses are 32-bit.
pub fn read_at_va(bytes: &[u8], va: u32, n: usize) -> Option<Vec<u8>> {
    if bytes.len() < 0x40 || &bytes[0..2] != b"MZ" {
        return None;
    }
    let pe_at = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    if pe_at + 24 > bytes.len() || &bytes[pe_at..pe_at + 4] != b"PE\0\0" {
        return None;
    }
    let coff = pe_at + 4;
    // COFF header: NumberOfSections @2 (u16), SizeOfOptionalHeader @16 (u16).
    let num_sections = u16::from_le_bytes([bytes[coff + 2], bytes[coff + 3]]) as usize;
    let opt_size = u16::from_le_bytes([bytes[coff + 16], bytes[coff + 17]]) as usize;
    let opt = coff + 20;
    // ImageBase lives at optional-header offset 28 in PE32; guard the read.
    if opt + 32 > bytes.len() {
        return None;
    }
    if u16::from_le_bytes([bytes[opt], bytes[opt + 1]]) != PE32_MAGIC {
        return None;
    }
    let image_base = u32::from_le_bytes([
        bytes[opt + 28],
        bytes[opt + 29],
        bytes[opt + 30],
        bytes[opt + 31],
    ]);
    let rva = va.checked_sub(image_base)?;
    let table = opt + opt_size;
    for i in 0..num_sections {
        let s = table + i * 40;
        if s + 40 > bytes.len() {
            return None;
        }
        let rd = |off: usize| u32::from_le_bytes([bytes[s + off], bytes[s + off + 1], bytes[s + off + 2], bytes[s + off + 3]]);
        let virtual_size = rd(8);
        let virtual_addr = rd(12);
        let raw_size = rd(16);
        let raw_ptr = rd(20);
        // A section maps `VirtualSize` bytes at `VirtualAddress`; some linkers leave VirtualSize 0
        // and the raw size is the only span, so fall back to it.
        let span = if virtual_size == 0 { raw_size } else { virtual_size };
        if rva >= virtual_addr && rva < virtual_addr.saturating_add(span) {
            let into = (rva - virtual_addr) as usize;
            // Only the raw-backed prefix is on disk; a `va` in the zero-filled tail is not readable.
            if into.checked_add(n)? > raw_size as usize {
                return None;
            }
            let start = (raw_ptr as usize).checked_add(into)?;
            let end = start.checked_add(n)?;
            return (end <= bytes.len()).then(|| bytes[start..end].to_vec());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built minimal PE32: one `.text` section mapped at RVA 0x1000 from file offset 0x200,
    /// image base 0x00400000. Proves `read_at_va` maps an absolute VA to the right file bytes
    /// without needing a real `Mercenaries2.exe`.
    #[test]
    fn read_at_va_maps_a_section_address_to_file_bytes() {
        let mut pe = vec![0u8; 0x400];
        pe[0..2].copy_from_slice(b"MZ");
        let e_lfanew: u32 = 0x80;
        pe[0x3C..0x40].copy_from_slice(&e_lfanew.to_le_bytes());
        let pe_at = e_lfanew as usize;
        pe[pe_at..pe_at + 4].copy_from_slice(b"PE\0\0");
        let coff = pe_at + 4;
        pe[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes()); // NumberOfSections
        let opt_size: u16 = 0x60;
        pe[coff + 16..coff + 18].copy_from_slice(&opt_size.to_le_bytes());
        let opt = coff + 20;
        pe[opt..opt + 2].copy_from_slice(&PE32_MAGIC.to_le_bytes());
        pe[opt + 28..opt + 32].copy_from_slice(&0x0040_0000u32.to_le_bytes()); // ImageBase
        let table = opt + opt_size as usize;
        pe[table..table + 5].copy_from_slice(b".text");
        pe[table + 8..table + 12].copy_from_slice(&0x1000u32.to_le_bytes()); // VirtualSize
        pe[table + 12..table + 16].copy_from_slice(&0x1000u32.to_le_bytes()); // VirtualAddress
        pe[table + 16..table + 20].copy_from_slice(&0x0200u32.to_le_bytes()); // SizeOfRawData
        pe[table + 20..table + 24].copy_from_slice(&0x0200u32.to_le_bytes()); // PointerToRawData
        let prologue = [0x55u8, 0x8B, 0xEC, 0x51, 0x53, 0x56, 0x57];
        pe[0x200..0x200 + prologue.len()].copy_from_slice(&prologue);

        // 0x00400000 + 0x1000 → file offset 0x200.
        assert_eq!(read_at_va(&pe, 0x0040_1000, 7).as_deref(), Some(&prologue[..]));
        // A few bytes in, still mapped.
        assert_eq!(read_at_va(&pe, 0x0040_1002, 2).as_deref(), Some(&[0xEC, 0x51][..]));
        // Below the image base, and above every section: unmapped.
        assert_eq!(read_at_va(&pe, 0x0000_0010, 1), None);
        assert_eq!(read_at_va(&pe, 0x0090_0000, 1), None);
        // A PE32+ magic is refused rather than misread.
        let mut pe64 = pe.clone();
        pe64[opt..opt + 2].copy_from_slice(&0x020Bu16.to_le_bytes());
        assert_eq!(read_at_va(&pe64, 0x0040_1000, 1), None);
    }
}
