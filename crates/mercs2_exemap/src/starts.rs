//! Function start addresses read from an executable's own tables.
//!
//! * **Xbox 360 PE** (machine `0x1f2`): the `.pdata` section is a table of 8-byte big-endian
//!   records `{u32 begin_va, u32 packed_info}`, ascending, ended by a zero record or the section
//!   end. Every begin address lies in an executable section (`.text` and the Bink library's `BINK`
//!   both carry code).
//! * **PS3 PPU ELF** (64-bit big-endian, 32-bit pointers): the `.opd` function-descriptor table is
//!   an array of 8-byte records `{u32 code, u32 toc}`. Section names are stripped in the shipped
//!   EBOOT, so `.opd` is found as the section that holds the ELF entry point, which is itself a
//!   descriptor. Every code address lies in an executable (`SHF_EXECINSTR`) section.
//!
//! Anything else — another PE machine, a missing table, an address outside the code sections, a
//! descending or partly-zeroed `.pdata` — is an error.

use securom_unwrap::Pe;

use crate::xbox::MACHINE_XENON;
use crate::Error;

/// Which table the starts came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Xbox 360 `.pdata` at this VA, with this many records.
    Pdata { va: u32, records: usize },
    /// PS3 `.opd` at this VA, with this many descriptors (the TOC of the entry descriptor too).
    Opd { va: u32, descriptors: usize, toc: u32 },
}

/// The function starts of `image`, ascending and unique, and the table they came from.
pub fn function_starts(image: &[u8]) -> Result<(Vec<u32>, Source), Error> {
    if image.starts_with(b"MZ") {
        pdata_starts(image)
    } else if image.starts_with(b"\x7fELF") {
        opd_starts(image)
    } else {
        Err(Error::new("not a PE (MZ) or ELF image"))
    }
}

const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;

fn pdata_starts(image: &[u8]) -> Result<(Vec<u32>, Source), Error> {
    let pe = Pe::parse(image).map_err(|e| Error::new(format!("PE parse: {e}")))?;
    if pe.machine != MACHINE_XENON {
        return Err(Error::new(format!(
            "PE machine 0x{:x} has no .pdata start table this tool reads (Xbox 360 is 0x{MACHINE_XENON:x})",
            pe.machine
        )));
    }
    let pdata = pe
        .sections
        .iter()
        .find(|s| s.name == ".pdata")
        .ok_or_else(|| Error::new("Xbox 360 PE has no .pdata section"))?;
    let off = pe
        .rva_to_off(pdata.vaddr)
        .ok_or_else(|| Error::new(".pdata RVA does not map to the file"))?;
    let end = off + pdata.vsize as usize;
    let table = image
        .get(off..end)
        .ok_or_else(|| Error::new(format!(".pdata (0x{:x} bytes at file 0x{off:x}) runs past the file", pdata.vsize)))?;
    let code: Vec<(u32, u32, &str)> = pe
        .sections
        .iter()
        .filter(|s| s.characteristics & IMAGE_SCN_MEM_EXECUTE != 0)
        .map(|s| (pe.image_base + s.vaddr, pe.image_base + s.vaddr + s.vsize, s.name.as_str()))
        .collect();

    let mut starts = Vec::new();
    let mut ended = false;
    for (i, rec) in table.chunks_exact(8).enumerate() {
        let begin = u32::from_be_bytes(rec[0..4].try_into().unwrap());
        let info = u32::from_be_bytes(rec[4..8].try_into().unwrap());
        if ended {
            if begin != 0 || info != 0 {
                return Err(Error::new(format!(".pdata record {i} follows the zero terminator but is not zero")));
            }
            continue;
        }
        if begin == 0 {
            if info != 0 {
                return Err(Error::new(format!(".pdata record {i} has begin 0 but info 0x{info:08x}")));
            }
            ended = true;
            continue;
        }
        if !code.iter().any(|&(lo, hi, _)| begin >= lo && begin < hi) {
            return Err(Error::new(format!(".pdata record {i}: begin 0x{begin:08x} is outside every executable section")));
        }
        if let Some(&last) = starts.last() {
            if begin <= last {
                return Err(Error::new(format!(
                    ".pdata record {i}: begin 0x{begin:08x} does not ascend past 0x{last:08x}"
                )));
            }
        }
        starts.push(begin);
    }
    if starts.is_empty() {
        return Err(Error::new(".pdata holds no records"));
    }
    let records = starts.len();
    Ok((starts, Source::Pdata { va: pe.image_base + pdata.vaddr, records }))
}

pub(crate) struct ElfSection {
    pub(crate) flags: u64,
    pub(crate) addr: u64,
    pub(crate) offset: u64,
    pub(crate) size: u64,
    pub(crate) kind: u32,
}

pub(crate) const SHF_EXECINSTR: u64 = 0x4;
pub(crate) const SHT_PROGBITS: u32 = 1;

fn be_u16(d: &[u8], o: usize) -> Result<u16, Error> {
    d.get(o..o + 2)
        .map(|b| u16::from_be_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::new(format!("ELF truncated at 0x{o:x}")))
}
fn be_u32(d: &[u8], o: usize) -> Result<u32, Error> {
    d.get(o..o + 4)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::new(format!("ELF truncated at 0x{o:x}")))
}
fn be_u64(d: &[u8], o: usize) -> Result<u64, Error> {
    d.get(o..o + 8)
        .map(|b| u64::from_be_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::new(format!("ELF truncated at 0x{o:x}")))
}

/// The entry point and section headers of a 64-bit big-endian (PS3 PPU) ELF.
pub(crate) fn elf_sections(image: &[u8]) -> Result<(u64, Vec<ElfSection>), Error> {
    // e_ident: class 2 (64-bit), data 2 (big-endian).
    if image.get(4) != Some(&2) || image.get(5) != Some(&2) {
        return Err(Error::new("ELF is not 64-bit big-endian (PS3 PPU)"));
    }
    let entry = be_u64(image, 0x18)?;
    let shoff = be_u64(image, 0x28)? as usize;
    let shentsize = be_u16(image, 0x3a)? as usize;
    let shnum = be_u16(image, 0x3c)? as usize;
    if shoff == 0 || shnum == 0 {
        return Err(Error::new("ELF has no section header table"));
    }
    let mut secs = Vec::with_capacity(shnum);
    for i in 0..shnum {
        let o = shoff + i * shentsize;
        secs.push(ElfSection {
            kind: be_u32(image, o + 4)?,
            flags: be_u64(image, o + 8)?,
            addr: be_u64(image, o + 16)?,
            offset: be_u64(image, o + 24)?,
            size: be_u64(image, o + 32)?,
        });
    }
    Ok((entry, secs))
}

pub(crate) struct ElfSegment {
    pub(crate) kind: u32,
    pub(crate) offset: u64,
    pub(crate) vaddr: u64,
    pub(crate) filesz: u64,
}

/// The program headers of a 64-bit big-endian (PS3 PPU) ELF.
pub(crate) fn elf_segments(image: &[u8]) -> Result<Vec<ElfSegment>, Error> {
    if image.get(4) != Some(&2) || image.get(5) != Some(&2) {
        return Err(Error::new("ELF is not 64-bit big-endian (PS3 PPU)"));
    }
    let phoff = be_u64(image, 0x20)? as usize;
    let phentsize = be_u16(image, 0x36)? as usize;
    let phnum = be_u16(image, 0x38)? as usize;
    if phoff == 0 || phnum == 0 {
        return Err(Error::new("ELF has no program header table"));
    }
    let mut segs = Vec::with_capacity(phnum);
    for i in 0..phnum {
        let o = phoff + i * phentsize;
        segs.push(ElfSegment {
            kind: be_u32(image, o)?,
            offset: be_u64(image, o + 8)?,
            vaddr: be_u64(image, o + 16)?,
            filesz: be_u64(image, o + 32)?,
        });
    }
    Ok(segs)
}

/// The `.opd` section: the PROGBITS, non-executable section that holds the entry descriptor.
pub(crate) fn opd_section(entry: u64, secs: &[ElfSection]) -> Result<&ElfSection, Error> {
    let opd = secs
        .iter()
        .find(|s| s.kind == SHT_PROGBITS && s.size > 0 && entry >= s.addr && entry < s.addr + s.size)
        .ok_or_else(|| Error::new(format!("no PROGBITS section holds the entry descriptor 0x{entry:x}")))?;
    if opd.flags & SHF_EXECINSTR != 0 {
        return Err(Error::new(format!(
            "the entry point 0x{entry:x} lies in an executable section, not an .opd descriptor table"
        )));
    }
    if opd.size % 8 != 0 {
        return Err(Error::new(format!(".opd size 0x{:x} is not a multiple of 8-byte descriptors", opd.size)));
    }
    Ok(opd)
}

fn opd_starts(image: &[u8]) -> Result<(Vec<u32>, Source), Error> {
    let (entry, secs) = elf_sections(image)?;
    let opd = opd_section(entry, &secs)?;
    let table = image
        .get(opd.offset as usize..(opd.offset + opd.size) as usize)
        .ok_or_else(|| Error::new(".opd runs past the file"))?;
    let entry_off = (opd.offset + (entry - opd.addr)) as usize;
    let toc = be_u32(image, entry_off + 4)?;
    let code: Vec<(u64, u64)> = secs
        .iter()
        .filter(|s| s.flags & SHF_EXECINSTR != 0 && s.size > 0)
        .map(|s| (s.addr, s.addr + s.size))
        .collect();
    let mut starts = Vec::with_capacity(table.len() / 8);
    for (i, d) in table.chunks_exact(8).enumerate() {
        let c = u32::from_be_bytes(d[0..4].try_into().unwrap());
        if !code.iter().any(|&(lo, hi)| (c as u64) >= lo && (c as u64) < hi) {
            return Err(Error::new(format!(".opd descriptor {i}: code 0x{c:08x} is outside every executable section")));
        }
        starts.push(c);
    }
    let descriptors = starts.len();
    starts.sort_unstable();
    starts.dedup();
    Ok((starts, Source::Opd { va: opd.addr as u32, descriptors, toc }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal PE32 image: headers at 0, then `sections` as (name, va, vsize, characteristics,
    /// bytes), each placed at file offset = va (the RVA layout of an unpacked Xbox image).
    pub(crate) fn pe_image(machine: u16, base: u32, sections: &[(&str, u32, u32, u32, &[u8])]) -> Vec<u8> {
        let size = sections.iter().map(|s| (s.1 + s.2) as usize).max().unwrap_or(0x400).max(0x400);
        let mut d = vec![0u8; size];
        d[0..2].copy_from_slice(b"MZ");
        let lf = 0x80usize;
        d[0x3c..0x40].copy_from_slice(&(lf as u32).to_le_bytes());
        d[lf..lf + 4].copy_from_slice(b"PE\0\0");
        d[lf + 4..lf + 6].copy_from_slice(&machine.to_le_bytes());
        d[lf + 6..lf + 8].copy_from_slice(&(sections.len() as u16).to_le_bytes());
        let size_opt = 0xE0u16;
        d[lf + 20..lf + 22].copy_from_slice(&size_opt.to_le_bytes());
        let opt = lf + 24;
        d[opt..opt + 2].copy_from_slice(&0x10bu16.to_le_bytes());
        d[opt + 28..opt + 32].copy_from_slice(&base.to_le_bytes());
        d[opt + 32..opt + 36].copy_from_slice(&0x100u32.to_le_bytes());
        d[opt + 36..opt + 40].copy_from_slice(&0x200u32.to_le_bytes());
        d[opt + 60..opt + 64].copy_from_slice(&0x200u32.to_le_bytes());
        let table = opt + size_opt as usize;
        for (i, (name, va, vsize, ch, bytes)) in sections.iter().enumerate() {
            let o = table + i * 40;
            d[o..o + name.len()].copy_from_slice(name.as_bytes());
            d[o + 8..o + 12].copy_from_slice(&vsize.to_le_bytes());
            d[o + 12..o + 16].copy_from_slice(&va.to_le_bytes());
            d[o + 16..o + 20].copy_from_slice(&vsize.to_le_bytes());
            d[o + 20..o + 24].copy_from_slice(&va.to_le_bytes());
            d[o + 36..o + 40].copy_from_slice(&ch.to_le_bytes());
            d[*va as usize..*va as usize + bytes.len()].copy_from_slice(bytes);
        }
        d
    }

    fn pdata(records: &[(u32, u32)], len: usize) -> Vec<u8> {
        let mut v = vec![0u8; len];
        for (i, (b, info)) in records.iter().enumerate() {
            v[i * 8..i * 8 + 4].copy_from_slice(&b.to_be_bytes());
            v[i * 8 + 4..i * 8 + 8].copy_from_slice(&info.to_be_bytes());
        }
        v
    }

    const EXEC: u32 = 0x6000_0020;
    const DATA: u32 = 0x4000_0040;
    const BASE: u32 = 0x8200_0000;

    fn xbox(records: &[(u32, u32)]) -> Vec<u8> {
        let p = pdata(records, 0x40);
        pe_image(
            MACHINE_XENON,
            BASE,
            &[(".pdata", 0x400, 0x40, DATA, &p), (".text", 0x500, 0x100, EXEC, &[]), ("BINK", 0x600, 0x40, EXEC, &[])],
        )
    }

    #[test]
    fn pdata_starts_cover_every_executable_section_up_to_the_terminator() {
        let img = xbox(&[(BASE + 0x500, 1), (BASE + 0x520, 1), (BASE + 0x600, 1)]);
        let (starts, src) = function_starts(&img).unwrap();
        assert_eq!(starts, vec![BASE + 0x500, BASE + 0x520, BASE + 0x600]);
        assert_eq!(src, Source::Pdata { va: BASE + 0x400, records: 3 });
    }

    #[test]
    fn pdata_rejects_a_start_outside_code() {
        let e = function_starts(&xbox(&[(BASE + 0x400, 1)])).unwrap_err().to_string();
        assert!(e.contains("outside every executable section"), "{e}");
    }

    #[test]
    fn pdata_rejects_descending_starts() {
        let e = function_starts(&xbox(&[(BASE + 0x520, 1), (BASE + 0x500, 1)])).unwrap_err().to_string();
        assert!(e.contains("does not ascend"), "{e}");
    }

    #[test]
    fn pdata_rejects_records_after_the_terminator() {
        let e = function_starts(&xbox(&[(BASE + 0x500, 1), (0, 0), (BASE + 0x520, 1)])).unwrap_err().to_string();
        assert!(e.contains("follows the zero terminator"), "{e}");
    }

    #[test]
    fn an_x86_pe_is_refused() {
        let img = pe_image(0x14c, 0x40_0000, &[(".text", 0x400, 0x40, EXEC, &[])]);
        let e = function_starts(&img).unwrap_err().to_string();
        assert!(e.contains("0x14c"), "{e}");
    }

    /// A minimal ELF64 big-endian image with a code section and an .opd table; `entry_index` picks
    /// the descriptor the entry point names.
    fn elf(descriptors: &[(u32, u32)], entry_index: usize) -> Vec<u8> {
        let text = (0x1000u64, 0x200u64, 0x100u64); // addr, offset, size
        let opd_addr = 0x2000u64;
        let opd_off = 0x300u64;
        let opd_size = (descriptors.len() * 8) as u64;
        let shoff = 0x400usize;
        let mut d = vec![0u8; shoff + 3 * 64];
        d[0..4].copy_from_slice(b"\x7fELF");
        d[4] = 2;
        d[5] = 2;
        d[0x18..0x20].copy_from_slice(&(opd_addr + entry_index as u64 * 8).to_be_bytes());
        d[0x28..0x30].copy_from_slice(&(shoff as u64).to_be_bytes());
        d[0x3a..0x3c].copy_from_slice(&64u16.to_be_bytes());
        d[0x3c..0x3e].copy_from_slice(&3u16.to_be_bytes());
        let mut sh = |i: usize, kind: u32, flags: u64, addr: u64, off: u64, size: u64| {
            let o = shoff + i * 64;
            d[o + 4..o + 8].copy_from_slice(&kind.to_be_bytes());
            d[o + 8..o + 16].copy_from_slice(&flags.to_be_bytes());
            d[o + 16..o + 24].copy_from_slice(&addr.to_be_bytes());
            d[o + 24..o + 32].copy_from_slice(&off.to_be_bytes());
            d[o + 32..o + 40].copy_from_slice(&size.to_be_bytes());
        };
        sh(1, SHT_PROGBITS, 0x6, text.0, text.1, text.2);
        sh(2, SHT_PROGBITS, 0x3, opd_addr, opd_off, opd_size);
        for (i, (c, t)) in descriptors.iter().enumerate() {
            let o = opd_off as usize + i * 8;
            d[o..o + 4].copy_from_slice(&c.to_be_bytes());
            d[o + 4..o + 8].copy_from_slice(&t.to_be_bytes());
        }
        d
    }

    #[test]
    fn opd_is_found_from_the_entry_descriptor() {
        let img = elf(&[(0x1080, 0x9000), (0x1000, 0x9000), (0x1080, 0x9000)], 1);
        let (starts, src) = function_starts(&img).unwrap();
        assert_eq!(starts, vec![0x1000, 0x1080]);
        assert_eq!(src, Source::Opd { va: 0x2000, descriptors: 3, toc: 0x9000 });
    }

    #[test]
    fn opd_rejects_code_outside_executable_sections() {
        let e = function_starts(&elf(&[(0x1000, 1), (0x5000, 1)], 0)).unwrap_err().to_string();
        assert!(e.contains("descriptor 1"), "{e}");
    }
}
