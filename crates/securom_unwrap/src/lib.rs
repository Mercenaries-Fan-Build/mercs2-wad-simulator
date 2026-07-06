//! `securom_unwrap` — turn a SecuROM-cracked-but-**decrypted** Win32 PE into a
//! SecuROM-free executable by restoring the original entry point and a clean
//! import table, without disturbing the (kept) relocated code sections.
//!
//! ## Why this shape
//!
//! SecuROM 7.x on Mercenaries 2 (PC) uses *code-splicing*: chunks of the game's
//! own code are relocated into SecuROM-named sections (`Stext`, `.securom`, …)
//! and reached by `jmp`/`call` from `.text`. A double-blind RE study established
//! that the protection runtime cannot be removed by dropping those sections
//! without devirtualizing ~700 spliced macros (proven not worth it: weeks of
//! work, zero functional gain). The supported, robust transform is the standard
//! "run-to-OEP" endgame applied *statically* to an already-decrypted image:
//!
//! 1. Repoint the PE entry from the SecuROM/loader stub to the real **OEP**
//!    (`WinMainCRTStartup`), so none of the protection trigger code ever runs.
//! 2. Rebuild a clean import directory listing only the game's own imports,
//!    dropping the SecuROM loader's duplicate import descriptors.
//!
//! Every section — including the SecuROM ones that hold relocated game code — is
//! preserved verbatim. The result needs no disc, activation, online check, or
//! crack loader.
//!
//! The base image must already have decrypted code on disk (e.g. a RELOADED-
//! unpacked build); this crate does not decrypt SecuROM sections.

use std::collections::HashSet;

/// Section names treated as SecuROM's by default. An import descriptor whose
/// thunk array lives in one of these is dropped; sections are still kept in the
/// output.
pub const DEFAULT_SECUROM_SECTIONS: &[&str] =
    &["stext", "sitext", "srdata", "sdata", "sidata", ".securom", "reloaded"];

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Not a 32-bit PE (missing `PE\0\0` or wrong optional-header magic).
    NotPe,
    /// A read ran past the end of the buffer.
    Truncated(&'static str),
    /// The image has no import directory (data directory 1 is empty).
    NoImportDir,
    /// Could not statically locate the original entry point; pass one explicitly.
    OepNotFound,
    /// SecuROM import descriptors are interleaved with the game's, so a simple
    /// truncation would drop real imports. Needs a full import-table rebuild.
    ImportsNotPrefix,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotPe => write!(f, "not a 32-bit PE image"),
            Error::Truncated(what) => write!(f, "image truncated while reading {what}"),
            Error::NoImportDir => write!(f, "image has no import directory"),
            Error::OepNotFound => write!(f, "could not derive original entry point (pass --oep)"),
            Error::ImportsNotPrefix => {
                write!(f, "SecuROM imports interleaved with game imports (needs full rebuild)")
            }
        }
    }
}
impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

fn rd_u16(d: &[u8], o: usize) -> Result<u16> {
    d.get(o..o + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(Error::Truncated("u16"))
}
fn rd_u32(d: &[u8], o: usize) -> Result<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Truncated("u32"))
}

/// A parsed PE section header.
#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub vaddr: u32,
    pub vsize: u32,
    pub raw_ptr: u32,
    pub raw_size: u32,
}

/// Minimal read-only PE32 view: just enough to find the entry, sections,
/// data directories, and to map RVAs to file offsets.
pub struct Pe<'a> {
    data: &'a [u8],
    opt_off: usize,
    pub image_base: u32,
    pub entry: u32,
    pub sections: Vec<Section>,
}

impl<'a> Pe<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Pe<'a>> {
        if data.get(0..2) != Some(b"MZ") {
            return Err(Error::NotPe);
        }
        let pe_off = rd_u32(data, 0x3c)? as usize;
        if data.get(pe_off..pe_off + 4) != Some(b"PE\0\0") {
            return Err(Error::NotPe);
        }
        let opt_off = pe_off + 24;
        if rd_u16(data, opt_off)? != 0x10b {
            return Err(Error::NotPe); // not PE32 (this crate targets 32-bit)
        }
        let num_sec = rd_u16(data, pe_off + 6)? as usize;
        let size_opt = rd_u16(data, pe_off + 20)? as usize;
        let entry = rd_u32(data, opt_off + 16)?;
        let image_base = rd_u32(data, opt_off + 28)?;
        let sec_table = pe_off + 24 + size_opt;
        let mut sections = Vec::with_capacity(num_sec);
        for i in 0..num_sec {
            let so = sec_table + i * 40;
            let raw_name = data.get(so..so + 8).ok_or(Error::Truncated("section name"))?;
            let name = String::from_utf8_lossy(raw_name)
                .trim_end_matches('\0')
                .to_string();
            sections.push(Section {
                name,
                vsize: rd_u32(data, so + 8)?,
                vaddr: rd_u32(data, so + 12)?,
                raw_size: rd_u32(data, so + 16)?,
                raw_ptr: rd_u32(data, so + 20)?,
            });
        }
        Ok(Pe { data, opt_off, image_base, entry, sections })
    }

    /// Map an RVA to a file offset (handles both header and section regions).
    pub fn rva_to_off(&self, rva: u32) -> Option<usize> {
        for s in &self.sections {
            let span = s.vsize.max(s.raw_size);
            if rva >= s.vaddr && rva < s.vaddr + span {
                return Some((s.raw_ptr + (rva - s.vaddr)) as usize);
            }
        }
        // Header RVAs (e.g. data directories) map 1:1 below the first section.
        let first = self.sections.iter().map(|s| s.vaddr).min().unwrap_or(0);
        if rva < first {
            Some(rva as usize)
        } else {
            None
        }
    }

    pub fn section_of(&self, rva: u32) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| rva >= s.vaddr && rva < s.vaddr + s.vsize.max(s.raw_size))
    }

    /// Read data directory `i` as `(rva, size)`.
    pub fn data_dir(&self, i: usize) -> Result<(u32, u32)> {
        let o = self.opt_off + 96 + i * 8;
        Ok((rd_u32(self.data, o)?, rd_u32(self.data, o + 4)?))
    }

    fn cstr(&self, rva: u32) -> Result<String> {
        let o = self.rva_to_off(rva).ok_or(Error::Truncated("name rva"))?;
        let end = self.data[o..]
            .iter()
            .position(|&b| b == 0)
            .ok_or(Error::Truncated("name"))?;
        Ok(String::from_utf8_lossy(&self.data[o..o + end]).into_owned())
    }

    /// Count thunks in a null-terminated thunk array (INT or IAT).
    fn count_thunks(&self, thunk_rva: u32) -> Result<usize> {
        let mut o = self.rva_to_off(thunk_rva).ok_or(Error::Truncated("thunks"))?;
        let mut n = 0;
        loop {
            if rd_u32(self.data, o)? == 0 {
                return Ok(n);
            }
            n += 1;
            o += 4;
        }
    }

    fn is_crt_start(&self, rva: u32) -> bool {
        // WinMainCRTStartup signature: `call __security_init_cookie; jmp __tmainCRTStartup`
        match self.rva_to_off(rva) {
            Some(o) => self.data.get(o) == Some(&0xE8) && self.data.get(o + 5) == Some(&0xE9),
            None => false,
        }
    }
}

/// One import directory entry, tagged with whether its thunks live in a
/// SecuROM section (and are therefore dropped).
#[derive(Clone, Debug)]
pub struct ImportEntry {
    pub dll: String,
    pub thunk_count: usize,
    pub first_thunk: u32,
    pub is_securom: bool,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Original entry point RVA; if `None` it is derived from the entry stub.
    pub oep: Option<u32>,
    /// Section names (lowercased) whose imports are SecuROM's and get dropped.
    pub securom_sections: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            oep: None,
            securom_sections: DEFAULT_SECUROM_SECTIONS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// What the transform did, for logging/UI.
#[derive(Clone, Debug)]
pub struct Report {
    pub original_entry: u32,
    pub oep: u32,
    pub kept: Vec<ImportEntry>,
    pub dropped: Vec<ImportEntry>,
    pub iat_rva: u32,
    pub iat_size: u32,
}

/// Derive the original entry point by single-stepping the SecuROM/loader stub
/// with a tiny opcode walker (no disassembler dependency). The stub is a short
/// linear sequence that ends in a `jmp` to `WinMainCRTStartup`; we follow it
/// until we land on the `call;jmp` CRT-start signature.
pub fn derive_oep(pe: &Pe) -> Result<u32> {
    if pe.is_crt_start(pe.entry) {
        return Ok(pe.entry); // already clean
    }
    let d = pe.data;
    let mut pc = pe.entry;
    for _ in 0..64 {
        let off = pe.rva_to_off(pc).ok_or(Error::OepNotFound)?;
        let op = *d.get(off).ok_or(Error::OepNotFound)?;
        match op {
            0xA1 | 0xA3 => pc += 5,              // mov eax,moffs32 / mov moffs32,eax
            0x68 => pc += 5,                     // push imm32
            0xE8 => pc += 5,                     // call rel32 (returns, continue)
            0x90 => pc += 1,                     // nop
            0x50..=0x5F => pc += 1,              // push/pop r32
            0xC7 if d.get(off + 1) == Some(&0x05) => pc += 10, // mov [moffs32],imm32
            0xFF if d.get(off + 1) == Some(&0x15) => pc += 6,  // call [mem]
            0xEB => {
                let rel = *d.get(off + 1).ok_or(Error::OepNotFound)? as i8 as i32;
                pc = (pc as i32 + 2 + rel) as u32; // jmp rel8
            }
            0xE9 => {
                let rel = rd_u32(d, off + 1)? as i32;
                let tgt = (pc as i32 + 5 + rel) as u32; // jmp rel32
                if pe.is_crt_start(tgt) {
                    return Ok(tgt);
                }
                pc = tgt; // intermediate jmp, keep following
            }
            _ => return Err(Error::OepNotFound),
        }
    }
    Err(Error::OepNotFound)
}

/// Produce a SecuROM-free copy of `data`. Returns the new image bytes and a
/// [`Report`]. The transform is purely a set of in-place header/directory edits
/// (entry point, import & IAT data directories, checksum, import-table
/// truncation), so the output keeps the input's section layout byte-for-byte.
pub fn unwrap(data: &[u8], opts: &Options) -> Result<(Vec<u8>, Report)> {
    let pe = Pe::parse(data)?;
    let sr: HashSet<String> = opts.securom_sections.iter().map(|s| s.to_lowercase()).collect();

    let oep = match opts.oep {
        Some(o) => o,
        None => derive_oep(&pe)?,
    };

    let (imp_rva, _) = pe.data_dir(1)?;
    if imp_rva == 0 {
        return Err(Error::NoImportDir);
    }
    let imp_off = pe.rva_to_off(imp_rva).ok_or(Error::NoImportDir)?;

    // Walk import descriptors (20 bytes each), tagging SecuROM ones.
    let mut entries: Vec<ImportEntry> = Vec::new();
    let mut o = imp_off;
    loop {
        // The terminator descriptor can lie in a section's zero-padded tail that
        // is past the file's end (raw_size < virtual_size); treat that as the end.
        if o + 20 > data.len() {
            break;
        }
        let oft = rd_u32(data, o)?;
        let name_rva = rd_u32(data, o + 12)?;
        let ft = rd_u32(data, o + 16)?;
        if oft == 0 && name_rva == 0 && ft == 0 {
            break;
        }
        let dll = pe.cstr(name_rva)?;
        let thunk_count = pe.count_thunks(if oft != 0 { oft } else { ft })?;
        let is_securom = pe
            .section_of(ft)
            .map(|s| sr.contains(&s.name.to_lowercase()))
            .unwrap_or(false);
        entries.push(ImportEntry { dll, thunk_count, first_thunk: ft, is_securom });
        o += 20;
    }

    // Game imports must form a leading prefix; SecuROM's follow. (If they were
    // interleaved, truncation would drop real imports — bail for a full rebuild.)
    let kept_count = entries.iter().position(|e| e.is_securom).unwrap_or(entries.len());
    if entries[kept_count..].iter().any(|e| !e.is_securom) {
        return Err(Error::ImportsNotPrefix);
    }
    let kept = entries[..kept_count].to_vec();
    let dropped = entries[kept_count..].to_vec();

    // IAT directory spans the kept descriptors' thunk arrays.
    let iat_rva = kept.iter().map(|e| e.first_thunk).min().unwrap_or(0);
    let iat_end = kept
        .iter()
        .map(|e| e.first_thunk + (e.thunk_count as u32 + 1) * 4)
        .max()
        .unwrap_or(iat_rva);
    let iat_size = iat_end - iat_rva;

    // Apply the in-place edits.
    let mut out = data.to_vec();
    let put = |out: &mut [u8], off: usize, v: u32| out[off..off + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut out, pe.opt_off + 16, oep); // AddressOfEntryPoint -> OEP
    put(&mut out, pe.opt_off + 64, 0); // CheckSum -> 0 (let the loader skip it)
    put(&mut out, pe.opt_off + 96 + 8 + 4, (kept_count as u32 + 1) * 20); // import dir size
    put(&mut out, pe.opt_off + 96 + 12 * 8, iat_rva); // IAT dir rva
    put(&mut out, pe.opt_off + 96 + 12 * 8 + 4, iat_size); // IAT dir size
    for b in out[imp_off + kept_count * 20..imp_off + kept_count * 20 + 20].iter_mut() {
        *b = 0; // null-terminate the descriptor array after the kept imports
    }

    Ok((out, Report { original_entry: pe.entry, oep, kept, dropped, iat_rva, iat_size }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a tiny synthetic flat PE (raw == virtual) to exercise the parser,
    // the OEP walker, and the import partition without needing a real game exe.
    fn synth() -> Vec<u8> {
        let mut d = vec![0u8; 0x4000];
        d[0..2].copy_from_slice(b"MZ");
        let pe = 0x80usize;
        d[0x3c..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
        d[pe..pe + 4].copy_from_slice(b"PE\0\0");
        let put16 = |d: &mut [u8], o: usize, v: u16| d[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let put32 = |d: &mut [u8], o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put16(&mut d, pe + 6, 2); // 2 sections
        put16(&mut d, pe + 20, 0xE0); // size of optional header
        let opt = pe + 24;
        put16(&mut d, opt, 0x10b); // PE32
        put32(&mut d, opt + 16, 0x1000); // entry = stub at start of .text
        put32(&mut d, opt + 28, 0x400000); // image base
        // data dir 1 (import) -> rva 0x2000
        put32(&mut d, opt + 96 + 8, 0x2000);
        put32(&mut d, opt + 96 + 8 + 4, 0x28);
        // sections: .text @0x1000, .Sdata @0x2000 (flat raw==virtual)
        let st = opt + 0xE0;
        let mksec = |d: &mut [u8], o: usize, name: &str, va: u32, sz: u32| {
            d[o..o + 8.min(name.len())].copy_from_slice(&name.as_bytes()[..name.len().min(8)]);
            put32(d, o + 8, sz);
            put32(d, o + 12, va);
            put32(d, o + 16, sz);
            put32(d, o + 20, va);
        };
        mksec(&mut d, st, ".text", 0x1000, 0x1000);
        mksec(&mut d, st + 40, "Sdata", 0x2000, 0x1000);
        // stub @0x1000: jmp 0x1100 (E9) ; OEP @0x1100: call;jmp (CRT sig)
        d[0x1000] = 0xE9;
        put32(&mut d, 0x1001, 0x1100 - (0x1000 + 5));
        d[0x1100] = 0xE8; // call rel32
        d[0x1105] = 0xE9; // jmp rel32  -> CRT-start signature at 0x1100
        // imports @0x2000: [0] game (INT/FT in .text-region rdata-ish) then [1] SecuROM (FT in Sdata)
        // game descriptor: OFT=0x2200, Name=0x2300, FT=0x2400 (all outside Sdata)
        put32(&mut d, 0x2000, 0x2200);
        put32(&mut d, 0x2000 + 12, 0x2300);
        put32(&mut d, 0x2000 + 16, 0x2400);
        // securom descriptor: FT=0x2080 inside Sdata(0x2000..0x3000)
        put32(&mut d, 0x2014, 0x2210);
        put32(&mut d, 0x2014 + 12, 0x2310);
        put32(&mut d, 0x2014 + 16, 0x2080);
        // terminator
        // (already zero)
        // thunk arrays: game INT @0x2200 = [x, 0]; securom INT @0x2210 = [x, 0]
        put32(&mut d, 0x2200, 0x2500);
        put32(&mut d, 0x2210, 0x2500);
        // names
        d[0x2300..0x2308].copy_from_slice(b"game.dll");
        d[0x2310..0x2316].copy_from_slice(b"sr.dll");
        // FT 0x2400 region is in headers-mapped/.text gap; ensure rva_to_off works: 0x2400 is in Sdata!
        // Move game FT outside Sdata: use 0x1400 (in .text). Fix descriptor.
        put32(&mut d, 0x2000 + 16, 0x1400);
        d
    }

    #[test]
    fn parses_and_partitions() {
        let d = synth();
        let (out, rep) = unwrap(&d, &Options::default()).expect("unwrap");
        assert_eq!(rep.oep, 0x1100, "derived OEP");
        assert_eq!(rd_u32(&out, 0x80 + 24 + 16).unwrap(), 0x1100, "entry patched");
        assert_eq!(rep.kept.len(), 1);
        assert_eq!(rep.kept[0].dll, "game.dll");
        assert_eq!(rep.dropped.len(), 1);
        assert_eq!(rep.dropped[0].dll, "sr.dll");
        assert!(rep.dropped[0].is_securom);
    }

    #[test]
    fn already_clean_entry_is_oep() {
        let mut d = synth();
        // point entry directly at the CRT-start
        d[0x80 + 24 + 16..0x80 + 24 + 20].copy_from_slice(&0x1100u32.to_le_bytes());
        let pe = Pe::parse(&d).unwrap();
        assert_eq!(derive_oep(&pe).unwrap(), 0x1100);
    }
}
