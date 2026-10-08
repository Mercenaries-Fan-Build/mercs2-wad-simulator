//! A read-only PE32 view: headers, section table, the mapped image, and the file bytes no header or
//! section covers.
//!
//! The mapped image is defined here, once, and the comparison in [`crate::membership`] reads it:
//!
//! * the headers map RVA `[0, SizeOfHeaders)` to file offset `[0, SizeOfHeaders)`;
//! * a section maps RVA `[rva, rva + align_up(max(VirtualSize, SizeOfRawData), SectionAlignment))`;
//!   offset `k` reads file byte `PointerToRawData + k` while `k < SizeOfRawData`, and `0` past it;
//! * an RVA outside every extent is unmapped.
//!
//! Anything that would make that view ambiguous — a raw range past the end of the file, two extents
//! that overlap, two raw ranges that overlap — is an [`Error`], never resolved by a guess.

use std::ops::Range;

use crate::Error;

/// One section header, as the file states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub rva: u32,
    pub virtual_size: u32,
    pub raw_offset: u32,
    pub raw_size: u32,
}

/// One mapped RVA range and what backs it.
#[derive(Clone, Debug)]
struct Extent {
    rva: Range<u32>,
    /// `None` for the headers, which map straight onto the file.
    section: Option<usize>,
}

/// A piece of [`Pe::view`]: an RVA range, and its bytes when the image maps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub rva: Range<u32>,
    pub bytes: Option<Vec<u8>>,
}

/// A parsed PE32 image over borrowed file bytes.
#[derive(Debug)]
pub struct Pe<'a> {
    bytes: &'a [u8],
    pub timestamp: u32,
    pub image_base: u32,
    pub section_alignment: u32,
    pub size_of_headers: u32,
    pub sections: Vec<Section>,
    extents: Vec<Extent>,
    slack: Vec<Range<u64>>,
}

fn rd_u16(d: &[u8], o: usize, what: &str) -> Result<u16, Error> {
    d.get(o..o + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| Error::new(format!("PE truncated reading {what} at file offset 0x{o:x}")))
}

fn rd_u32(d: &[u8], o: usize, what: &str) -> Result<u32, Error> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| Error::new(format!("PE truncated reading {what} at file offset 0x{o:x}")))
}

fn align_up(x: u64, a: u64) -> u64 {
    x.div_ceil(a) * a
}

impl<'a> Pe<'a> {
    /// Parse `bytes` as a PE32 image.
    pub fn parse(bytes: &'a [u8]) -> Result<Pe<'a>, Error> {
        if bytes.get(0..2) != Some(b"MZ") {
            return Err(Error::new("not a PE: no MZ signature at file offset 0"));
        }
        let pe_off = rd_u32(bytes, 0x3c, "e_lfanew")? as usize;
        if bytes.get(pe_off..pe_off + 4) != Some(b"PE\0\0") {
            return Err(Error::new(format!("not a PE: no PE\\0\\0 signature at e_lfanew 0x{pe_off:x}")));
        }
        let num_sections = rd_u16(bytes, pe_off + 6, "NumberOfSections")? as usize;
        let timestamp = rd_u32(bytes, pe_off + 8, "TimeDateStamp")?;
        let size_of_optional = rd_u16(bytes, pe_off + 20, "SizeOfOptionalHeader")? as usize;
        let opt = pe_off + 24;
        let magic = rd_u16(bytes, opt, "OptionalHeader.Magic")?;
        if magic != 0x10b {
            return Err(Error::new(format!("not a PE32 image: OptionalHeader.Magic is 0x{magic:x}, not 0x10b")));
        }
        let image_base = rd_u32(bytes, opt + 28, "ImageBase")?;
        let section_alignment = rd_u32(bytes, opt + 32, "SectionAlignment")?;
        let size_of_headers = rd_u32(bytes, opt + 60, "SizeOfHeaders")?;
        if section_alignment == 0 {
            return Err(Error::new("PE declares SectionAlignment 0"));
        }
        if size_of_headers as usize > bytes.len() {
            return Err(Error::new(format!(
                "SizeOfHeaders 0x{size_of_headers:x} runs past the end of the {}-byte file",
                bytes.len()
            )));
        }

        let table = opt + size_of_optional;
        let mut sections = Vec::with_capacity(num_sections);
        for i in 0..num_sections {
            let o = table + i * 40;
            let raw_name = bytes
                .get(o..o + 8)
                .ok_or_else(|| Error::new(format!("PE truncated reading section header {i}")))?;
            let len = raw_name.iter().position(|&b| b == 0).unwrap_or(8);
            let name = std::str::from_utf8(&raw_name[..len])
                .map_err(|_| Error::new(format!("section header {i} has a non-UTF-8 name {raw_name:02x?}")))?
                .to_string();
            let s = Section {
                name,
                virtual_size: rd_u32(bytes, o + 8, "VirtualSize")?,
                rva: rd_u32(bytes, o + 12, "VirtualAddress")?,
                raw_size: rd_u32(bytes, o + 16, "SizeOfRawData")?,
                raw_offset: rd_u32(bytes, o + 20, "PointerToRawData")?,
            };
            if s.raw_offset as u64 + s.raw_size as u64 > bytes.len() as u64 {
                return Err(Error::new(format!(
                    "section {} raw data 0x{:x}+0x{:x} runs past the end of the {}-byte file",
                    s.name,
                    s.raw_offset,
                    s.raw_size,
                    bytes.len()
                )));
            }
            sections.push(s);
        }

        let mut extents = vec![Extent { rva: 0..size_of_headers, section: None }];
        for (i, s) in sections.iter().enumerate() {
            let span = align_up(s.virtual_size.max(s.raw_size) as u64, section_alignment as u64);
            let end = s.rva as u64 + span;
            if end > u32::MAX as u64 {
                return Err(Error::new(format!("section {} extent ends past 4 GiB", s.name)));
            }
            extents.push(Extent { rva: s.rva..end as u32, section: Some(i) });
        }
        extents.sort_by_key(|e| e.rva.start);
        for w in extents.windows(2) {
            if w[0].rva.end > w[1].rva.start {
                return Err(Error::new(format!(
                    "mapped extents overlap: 0x{:x}..0x{:x} and 0x{:x}..0x{:x}",
                    w[0].rva.start, w[0].rva.end, w[1].rva.start, w[1].rva.end
                )));
            }
        }

        let mut covered: Vec<Range<u64>> = std::iter::once(0..size_of_headers as u64)
            .chain(
                sections
                    .iter()
                    .filter(|s| s.raw_size > 0)
                    .map(|s| s.raw_offset as u64..s.raw_offset as u64 + s.raw_size as u64),
            )
            .collect();
        covered.sort_by_key(|r| r.start);
        let mut slack = Vec::new();
        let mut end = 0u64;
        for r in &covered {
            if r.start < end {
                return Err(Error::new(format!(
                    "file ranges overlap at offset 0x{:x} (previous range ends at 0x{end:x})",
                    r.start
                )));
            }
            if r.start > end {
                slack.push(end..r.start);
            }
            end = r.end;
        }
        if end < bytes.len() as u64 {
            slack.push(end..bytes.len() as u64);
        }

        Ok(Pe {
            bytes,
            timestamp,
            image_base,
            section_alignment,
            size_of_headers,
            sections,
            extents,
            slack,
        })
    }

    /// The whole file.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// File ranges covered by neither the headers nor any section's raw data, ascending.
    pub fn slack(&self) -> &[Range<u64>] {
        &self.slack
    }

    /// The mapped extent of section `i` (an RVA range).
    pub fn section_extent(&self, i: usize) -> Range<u32> {
        self.extents
            .iter()
            .find(|e| e.section == Some(i))
            .map(|e| e.rva.clone())
            .expect("every section has an extent")
    }

    /// The mapped image over RVA `range`, split wherever the backing changes.
    pub fn view(&self, range: Range<u32>) -> Vec<Piece> {
        let mut out = Vec::new();
        let mut rva = range.start;
        while rva < range.end {
            match self.extents.iter().find(|e| e.rva.contains(&rva)) {
                None => {
                    let next = self
                        .extents
                        .iter()
                        .map(|e| e.rva.start)
                        .filter(|&s| s > rva)
                        .min()
                        .unwrap_or(range.end)
                        .min(range.end);
                    out.push(Piece { rva: rva..next, bytes: None });
                    rva = next;
                }
                Some(e) => {
                    let stop = e.rva.end.min(range.end);
                    let k = (rva - e.rva.start) as usize;
                    let n = (stop - rva) as usize;
                    let data = match e.section {
                        None => self.bytes[k..k + n].to_vec(),
                        Some(i) => {
                            let s = &self.sections[i];
                            let avail = (s.raw_size as usize).saturating_sub(k).min(n);
                            let mut v = Vec::with_capacity(n);
                            if avail > 0 {
                                let from = s.raw_offset as usize + k;
                                v.extend_from_slice(&self.bytes[from..from + avail]);
                            }
                            v.resize(n, 0);
                            v
                        }
                    };
                    out.push(Piece { rva: rva..stop, bytes: Some(data) });
                    rva = stop;
                }
            }
        }
        out
    }

    /// The mapped bytes of RVA `range`, or an error when any of it is unmapped.
    pub fn mapped(&self, range: Range<u32>) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity((range.end - range.start) as usize);
        for p in self.view(range) {
            match p.bytes {
                Some(b) => out.extend_from_slice(&b),
                None => {
                    return Err(Error::new(format!(
                        "RVA 0x{:x}..0x{:x} is not mapped by this image",
                        p.rva.start, p.rva.end
                    )))
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod synth {
    //! Synthetic PE32 images for the hermetic tests.

    /// One section to write: name, RVA, virtual size, raw bytes (placed at `raw_offset`).
    pub struct Sec<'a> {
        pub name: &'a str,
        pub rva: u32,
        pub virtual_size: u32,
        pub raw_offset: u32,
        pub raw: Vec<u8>,
    }

    /// A PE32 file: headers in the first 0x200 bytes, `e_lfanew` 0x80, section alignment 0x1000,
    /// image base 0x400000, then each section's raw bytes at its offset, then `tail`.
    pub fn pe(timestamp: u32, sections: &[Sec], tail: &[u8]) -> Vec<u8> {
        let end = sections
            .iter()
            .map(|s| s.raw_offset as usize + s.raw.len())
            .max()
            .unwrap_or(0x200)
            .max(0x200);
        let mut d = vec![0u8; end];
        d[0..2].copy_from_slice(b"MZ");
        let pe = 0x80usize;
        d[0x3c..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
        d[pe..pe + 4].copy_from_slice(b"PE\0\0");
        let p16 = |d: &mut [u8], o: usize, v: u16| d[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let p32 = |d: &mut [u8], o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        p16(&mut d, pe + 4, 0x14c);
        p16(&mut d, pe + 6, sections.len() as u16);
        p32(&mut d, pe + 8, timestamp);
        p16(&mut d, pe + 20, 0xe0);
        let opt = pe + 24;
        p16(&mut d, opt, 0x10b);
        p32(&mut d, opt + 28, 0x400000);
        p32(&mut d, opt + 32, 0x1000);
        p32(&mut d, opt + 36, 0x200);
        p32(&mut d, opt + 60, 0x200);
        let table = opt + 0xe0;
        for (i, s) in sections.iter().enumerate() {
            let o = table + i * 40;
            d[o..o + s.name.len()].copy_from_slice(s.name.as_bytes());
            p32(&mut d, o + 8, s.virtual_size);
            p32(&mut d, o + 12, s.rva);
            p32(&mut d, o + 16, s.raw.len() as u32);
            p32(&mut d, o + 20, s.raw_offset);
            let r = s.raw_offset as usize;
            d[r..r + s.raw.len()].copy_from_slice(&s.raw);
        }
        d.extend_from_slice(tail);
        d
    }
}

#[cfg(test)]
mod tests {
    use super::synth::{pe, Sec};
    use super::*;

    fn two_sections() -> Vec<u8> {
        pe(
            0x1234,
            &[
                Sec { name: ".text", rva: 0x1000, virtual_size: 0x180, raw_offset: 0x200, raw: vec![0xaa; 0x100] },
                Sec { name: ".data", rva: 0x3000, virtual_size: 0x10, raw_offset: 0x300, raw: vec![0xbb; 0x100] },
            ],
            b"tail",
        )
    }

    #[test]
    fn parses_the_header_fields_and_section_table() {
        let d = two_sections();
        let p = Pe::parse(&d).unwrap();
        assert_eq!(p.timestamp, 0x1234);
        assert_eq!(p.image_base, 0x400000);
        assert_eq!(p.section_alignment, 0x1000);
        assert_eq!(p.size_of_headers, 0x200);
        assert_eq!(p.sections.len(), 2);
        assert_eq!(p.sections[0].name, ".text");
        assert_eq!((p.sections[1].rva, p.sections[1].raw_offset, p.sections[1].raw_size), (0x3000, 0x300, 0x100));
        assert_eq!(p.section_extent(0), 0x1000..0x2000);
    }

    #[test]
    fn the_view_zero_fills_past_raw_data_and_marks_gaps_unmapped() {
        let d = two_sections();
        let p = Pe::parse(&d).unwrap();
        let v = p.view(0x10fe..0x2002);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].rva, 0x10fe..0x2000);
        let b = v[0].bytes.as_ref().unwrap();
        assert_eq!(&b[..2], &[0xaa, 0xaa]);
        assert!(b[2..].iter().all(|&x| x == 0));
        assert_eq!(v[1], Piece { rva: 0x2000..0x2002, bytes: None });
        assert!(p.mapped(0x1ffe..0x2001).is_err());
        assert_eq!(p.mapped(0..2).unwrap(), b"MZ");
    }

    #[test]
    fn slack_is_every_file_byte_outside_headers_and_raw_data() {
        let d = two_sections();
        let p = Pe::parse(&d).unwrap();
        assert_eq!(p.slack(), std::slice::from_ref(&(0x400..0x404)));
    }

    #[test]
    fn rejects_what_it_cannot_view_unambiguously() {
        assert!(Pe::parse(b"XX").unwrap_err().to_string().contains("MZ"));

        let mut d = two_sections();
        d[0x80] = b'Q';
        assert!(Pe::parse(&d).unwrap_err().to_string().contains("PE\\0\\0"));

        let mut d = two_sections();
        d[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
        assert!(Pe::parse(&d).unwrap_err().to_string().contains("0x20b"));

        let d = pe(0, &[Sec { name: "a", rva: 0x1000, virtual_size: 0x10, raw_offset: 0x200, raw: vec![1; 0x10] }], b"");
        let mut short = d.clone();
        short.truncate(0x208);
        assert!(Pe::parse(&short).unwrap_err().to_string().contains("past the end"));

        let d = pe(
            0,
            &[
                Sec { name: "a", rva: 0x1000, virtual_size: 0x1800, raw_offset: 0x200, raw: vec![1; 0x10] },
                Sec { name: "b", rva: 0x2000, virtual_size: 0x10, raw_offset: 0x300, raw: vec![1; 0x10] },
            ],
            b"",
        );
        assert!(Pe::parse(&d).unwrap_err().to_string().contains("extents overlap"));
    }
}
