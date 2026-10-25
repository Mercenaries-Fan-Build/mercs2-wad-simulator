//! The game executable's sections, read from a file on disk or from the image Windows mapped.
//!
//! Field offsets are the PE/COFF specification's: `e_lfanew` at DOS header `0x3c`; COFF file header after
//! the `PE\0\0` signature (`NumberOfSections` +2, `SizeOfOptionalHeader` +16); PE32 optional header magic
//! `0x10b`, `ImageBase` +28; 40-byte section headers (`VirtualSize` +8, `VirtualAddress` +12,
//! `SizeOfRawData` +16, `PointerToRawData` +20, `Characteristics` +36).
//!
//! A section's bytes here are its first `min(VirtualSize, SizeOfRawData)` bytes: that span holds the same
//! content in the file (from `PointerToRawData`) and in the mapped image (from `VirtualAddress`), so an
//! address resolved from a file on disk is the address the code has once the image is mapped.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExeError {
    Truncated { len: usize, at: usize },
    NoMz,
    NoPe(usize),
    NotPe32(u16),
    SectionOutOfBounds { name: String, start: usize, end: usize, len: usize },
}

impl std::fmt::Display for ExeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExeError::Truncated { len, at } => write!(f, "image is {len} bytes, too small for the header field at {at:#x}"),
            ExeError::NoMz => write!(f, "no MZ signature"),
            ExeError::NoPe(at) => write!(f, "no PE\\0\\0 signature at {at:#x}"),
            ExeError::NotPe32(m) => write!(f, "optional header magic {m:#06x} is not PE32 (0x010b)"),
            ExeError::SectionOutOfBounds { name, start, end, len } => {
                write!(f, "section {name:?} spans {start:#x}..{end:#x}, outside the {len}-byte image")
            }
        }
    }
}

impl std::error::Error for ExeError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionHeader {
    pub name: String,
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub raw_size: u32,
    pub raw_pointer: u32,
    pub characteristics: u32,
}

/// `IMAGE_SCN_CNT_CODE`: the section contains executable code.
pub const SCN_CNT_CODE: u32 = 0x20;

impl SectionHeader {
    /// Whether the section is marked as containing code.
    pub fn is_code(&self) -> bool {
        self.characteristics & SCN_CNT_CODE != 0
    }

    /// Length of the span that exists identically in the file and in the mapped image.
    pub fn common_len(&self) -> usize {
        self.virtual_size.min(self.raw_size) as usize
    }
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, ExeError> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or(ExeError::Truncated { len: b.len(), at })
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ExeError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(ExeError::Truncated { len: b.len(), at })
}

/// `ImageBase` and the section table, from the start of a file or of a mapped image (laid out alike).
pub fn parse_headers(b: &[u8]) -> Result<(u32, Vec<SectionHeader>), ExeError> {
    if b.get(0..2) != Some(b"MZ") {
        return Err(ExeError::NoMz);
    }
    let pe = u32_at(b, 0x3c)? as usize;
    if b.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(ExeError::NoPe(pe));
    }
    let coff = pe + 4;
    let nsec = u16_at(b, coff + 2)? as usize;
    let opt_size = u16_at(b, coff + 16)? as usize;
    let opt = coff + 20;
    let magic = u16_at(b, opt)?;
    if magic != 0x10b {
        return Err(ExeError::NotPe32(magic));
    }
    let image_base = u32_at(b, opt + 28)?;
    let table = opt + opt_size;
    let mut sections = Vec::with_capacity(nsec);
    for i in 0..nsec {
        let h = table + i * 40;
        let raw_name = b.get(h..h + 8).ok_or(ExeError::Truncated { len: b.len(), at: h })?;
        let end = raw_name.iter().position(|&c| c == 0).unwrap_or(8);
        sections.push(SectionHeader {
            name: String::from_utf8_lossy(&raw_name[..end]).into_owned(),
            virtual_size: u32_at(b, h + 8)?,
            virtual_address: u32_at(b, h + 12)?,
            raw_size: u32_at(b, h + 16)?,
            raw_pointer: u32_at(b, h + 20)?,
            characteristics: u32_at(b, h + 36)?,
        });
    }
    Ok((image_base, sections))
}

/// One section's header and bytes.
#[derive(Clone, Debug)]
pub struct Section<'a> {
    pub header: SectionHeader,
    /// The section's first virtual address.
    pub va: u32,
    pub bytes: &'a [u8],
}

impl Section<'_> {
    pub fn contains(&self, va: u32) -> bool {
        va >= self.va && ((va - self.va) as usize) < self.bytes.len()
    }
}

/// The executable's sections with their bytes, addressed by virtual address.
#[derive(Clone, Debug)]
pub struct Image<'a> {
    pub image_base: u32,
    pub sections: Vec<Section<'a>>,
}

impl<'a> Image<'a> {
    /// An executable file's bytes.
    pub fn from_file(file: &'a [u8]) -> Result<Self, ExeError> {
        let (image_base, headers) = parse_headers(file)?;
        let mut sections = Vec::with_capacity(headers.len());
        for h in headers {
            let start = h.raw_pointer as usize;
            let end = start + h.common_len();
            let bytes = file.get(start..end).ok_or_else(|| ExeError::SectionOutOfBounds {
                name: h.name.clone(),
                start,
                end,
                len: file.len(),
            })?;
            sections.push(Section { va: image_base + h.virtual_address, header: h, bytes });
        }
        Ok(Image { image_base, sections })
    }

    /// The image Windows mapped at `base`, given the headers parsed from its first page.
    ///
    /// # Safety
    /// Every section's `[base + VirtualAddress, + common_len)` is mapped and readable for `'a`.
    pub unsafe fn from_mapped(base: *const u8, image_base: u32, headers: Vec<SectionHeader>) -> Self {
        let sections = headers
            .into_iter()
            .map(|h| {
                let bytes = unsafe { core::slice::from_raw_parts(base.add(h.virtual_address as usize), h.common_len()) };
                Section { va: base as u32 + h.virtual_address, header: h, bytes }
            })
            .collect();
        Image { image_base, sections }
    }

    pub fn section(&self, name: &str) -> Option<&Section<'a>> {
        self.sections.iter().find(|s| s.header.name == name)
    }

    /// `[start, end)` of every section marked as code, over its `VirtualSize` (the span mapped), in
    /// section-table order.
    pub fn code_ranges(&self) -> Vec<(u32, u32)> {
        self.sections.iter().filter(|s| s.header.is_code()).map(|s| (s.va, s.va + s.header.virtual_size)).collect()
    }

    /// The bytes at `va` up to the end of its section.
    pub fn bytes_at(&self, va: u32) -> Option<&'a [u8]> {
        self.sections.iter().find(|s| s.contains(va)).map(|s| &s.bytes[(va - s.va) as usize..])
    }

    pub fn u32_at(&self, va: u32) -> Option<u32> {
        self.bytes_at(va).and_then(|b| b.get(..4)).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// The NUL-terminated string at `va`, without its NUL; `None` when unterminated within its section.
    pub fn cstr_at(&self, va: u32) -> Option<&'a [u8]> {
        let b = self.bytes_at(va)?;
        b.iter().position(|&c| c == 0).map(|n| &b[..n])
    }

    /// The name of the section holding `va`.
    pub fn section_of(&self, va: u32) -> Option<&str> {
        self.sections.iter().find(|s| s.contains(va)).map(|s| s.header.name.as_str())
    }
}

#[cfg(test)]
pub(crate) mod synth {
    //! A minimal PE32 file built in memory: headers, then each section's bytes at its file offset.

    /// `sections`: `(name, virtual address, bytes)`. File offsets are 0x200-aligned after a 0x400 header.
    pub fn pe(image_base: u32, sections: &[(&str, u32, Vec<u8>)]) -> Vec<u8> {
        let mut f = vec![0u8; 0x400];
        f[0..2].copy_from_slice(b"MZ");
        f[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        f[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        f[coff + 2..coff + 4].copy_from_slice(&(sections.len() as u16).to_le_bytes());
        f[coff + 16..coff + 18].copy_from_slice(&0xe0u16.to_le_bytes());
        let opt = coff + 20;
        f[opt..opt + 2].copy_from_slice(&0x10bu16.to_le_bytes());
        f[opt + 28..opt + 32].copy_from_slice(&image_base.to_le_bytes());
        let table = opt + 0xe0;
        let mut raw = 0x400u32;
        let mut bodies = Vec::new();
        for (i, (name, va, bytes)) in sections.iter().enumerate() {
            let h = table + i * 40;
            f[h..h + name.len()].copy_from_slice(name.as_bytes());
            f[h + 8..h + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            f[h + 12..h + 16].copy_from_slice(&va.to_le_bytes());
            f[h + 16..h + 20].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            f[h + 20..h + 24].copy_from_slice(&raw.to_le_bytes());
            bodies.push((raw, bytes.clone()));
            raw += (bytes.len() as u32 + 0x1ff) & !0x1ff;
        }
        f.resize(raw as usize, 0);
        for (at, b) in bodies {
            f[at as usize..at as usize + b.len()].copy_from_slice(&b);
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_are_addressed_by_va() {
        let file = synth::pe(0x400000, &[(".text", 0x1000, vec![0x90, 0xc3]), (".rdata", 0x2000, b"abc\0def\0".to_vec())]);
        let img = Image::from_file(&file).unwrap();
        assert_eq!(img.image_base, 0x400000);
        assert_eq!(img.section(".text").unwrap().va, 0x401000);
        assert_eq!(img.bytes_at(0x401000).unwrap(), &[0x90, 0xc3]);
        assert_eq!(img.cstr_at(0x402004).unwrap(), b"def");
        assert_eq!(img.section_of(0x402000), Some(".rdata"));
        assert_eq!(img.section_of(0x403000), None);
        assert_eq!(img.u32_at(0x402000), Some(u32::from_le_bytes(*b"abc\0")));
    }

    #[test]
    fn code_sections_are_read_from_characteristics() {
        let mut file = synth::pe(0x400000, &[(".text", 0x1000, vec![0x90; 0x10]), (".rdata", 0x2000, vec![0; 8]), ("Stext", 0x3000, vec![0xc3; 4])]);
        // Section headers start at 0x178 in the synthetic file (synth::pe); Characteristics is +36.
        for (i, ch) in [(0usize, 0x6000_0020u32), (1, 0x4000_0040), (2, 0xe000_0020)] {
            let at = 0x178 + i * 40 + 36;
            file[at..at + 4].copy_from_slice(&ch.to_le_bytes());
        }
        let img = Image::from_file(&file).unwrap();
        assert!(img.section(".text").unwrap().header.is_code());
        assert!(!img.section(".rdata").unwrap().header.is_code());
        assert_eq!(img.code_ranges(), vec![(0x401000, 0x401010), (0x403000, 0x403004)]);
    }

    #[test]
    fn rejects_non_pe() {
        assert_eq!(parse_headers(b"ZZ").unwrap_err(), ExeError::NoMz);
        let mut b = vec![0u8; 0x80];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3c] = 0x40;
        assert_eq!(parse_headers(&b).unwrap_err(), ExeError::NoPe(0x40));
    }
}
