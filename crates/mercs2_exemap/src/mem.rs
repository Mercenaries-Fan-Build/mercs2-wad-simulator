//! A read-only view of an image's mapped memory: the ranges its sections occupy, which of them hold
//! code, and its byte order. Shared by the readers that scan an image for pointer structures
//! ([`crate::rtti`], [`crate::luareg`]).

use securom_unwrap::Pe;

use crate::starts::{elf_sections, opd_section, SHF_EXECINSTR, SHT_PROGBITS};
use crate::xbox::MACHINE_XENON;
use crate::Error;

/// One mapped section.
pub(crate) struct Range {
    pub(crate) lo: u32,
    pub(crate) hi: u32,
    pub(crate) off: usize,
    pub(crate) exec: bool,
}

/// The image's mapped ranges, byte order, and (for a PS3 ELF) its `.opd` descriptor table.
pub(crate) struct Mem<'a> {
    pub(crate) d: &'a [u8],
    pub(crate) big: bool,
    pub(crate) ranges: Vec<Range>,
    /// `[lo, hi)` of the `.opd` table, where function pointers are descriptor addresses.
    pub(crate) opd: Option<(u32, u32)>,
}

impl<'a> Mem<'a> {
    /// The view of a PE (x86 little-endian or Xbox 360 big-endian) or PS3 ELF image.
    pub(crate) fn of(image: &'a [u8]) -> Result<Mem<'a>, Error> {
        if image.starts_with(b"MZ") {
            Mem::pe(image)
        } else if image.starts_with(b"\x7fELF") {
            Mem::elf(image)
        } else {
            Err(Error::new("not a PE (MZ) or ELF image"))
        }
    }

    fn pe(image: &'a [u8]) -> Result<Mem<'a>, Error> {
        let pe = Pe::parse(image).map_err(|e| Error::new(format!("PE parse: {e}")))?;
        let big = match pe.machine {
            0x14c => false,
            MACHINE_XENON => true,
            m => return Err(Error::new(format!("PE machine 0x{m:x} is neither x86 (0x14c) nor Xbox 360 (0x1f2)"))),
        };
        let mut ranges = Vec::new();
        for s in &pe.sections {
            let off = s.raw_ptr as usize;
            if off >= image.len() || s.raw_size == 0 {
                continue;
            }
            let len = (s.raw_size as usize).min(image.len() - off);
            let lo = pe.image_base + s.vaddr;
            ranges.push(Range { lo, hi: lo + len as u32, off, exec: s.characteristics & 0x2000_0000 != 0 });
        }
        Ok(Mem { d: image, big, ranges, opd: None })
    }

    fn elf(image: &'a [u8]) -> Result<Mem<'a>, Error> {
        let (entry, secs) = elf_sections(image)?;
        let opd = opd_section(entry, &secs)?;
        let opd = Some((opd.addr as u32, (opd.addr + opd.size) as u32));
        let ranges = secs
            .iter()
            .filter(|s| s.kind == SHT_PROGBITS && s.size > 0 && s.addr != 0)
            .map(|s| Range {
                lo: s.addr as u32,
                hi: (s.addr + s.size) as u32,
                off: s.offset as usize,
                exec: s.flags & SHF_EXECINSTR != 0,
            })
            .collect();
        Ok(Mem { d: image, big: true, ranges, opd })
    }

    pub(crate) fn off(&self, va: u32) -> Option<usize> {
        self.ranges.iter().find(|r| va >= r.lo && va < r.hi).map(|r| r.off + (va - r.lo) as usize)
    }

    pub(crate) fn word_at(&self, off: usize) -> Option<u32> {
        let b: [u8; 4] = self.d.get(off..off + 4)?.try_into().ok()?;
        Some(if self.big { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    }

    pub(crate) fn word(&self, va: u32) -> Option<u32> {
        self.word_at(self.off(va)?)
    }

    pub(crate) fn exec(&self, va: u32) -> bool {
        self.ranges.iter().any(|r| r.exec && va >= r.lo && va < r.hi)
    }

    /// The code address a function pointer names: the descriptor's code word for a pointer into
    /// `.opd`, the pointer itself when it points into code, `None` otherwise.
    pub(crate) fn function(&self, p: u32) -> Option<u32> {
        if let Some((lo, hi)) = self.opd {
            if p >= lo && p < hi && (p - lo) % 8 == 0 {
                let c = self.word(p)?;
                return self.exec(c).then_some(c);
            }
        }
        self.exec(p).then_some(p)
    }

    /// The NUL-terminated bytes at `va`, when the terminator comes within `max` bytes.
    pub(crate) fn cstr(&self, va: u32, max: usize) -> Option<&'a [u8]> {
        let o = self.off(va)?;
        let tail = self.d.get(o..(o + max + 1).min(self.d.len()))?;
        let end = tail.iter().position(|&b| b == 0)?;
        Some(&tail[..end])
    }

    /// Every 4-aligned word of the non-executable ranges, as (va, value).
    pub(crate) fn data_words(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.ranges.iter().filter(|r| !r.exec).flat_map(move |r| {
            let n = (r.hi - r.lo) as usize / 4;
            (0..n).filter_map(move |i| {
                let va = r.lo + (i * 4) as u32;
                self.word_at(r.off + i * 4).map(|w| (va, w))
            })
        })
    }
}
