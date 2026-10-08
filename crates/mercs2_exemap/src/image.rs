//! The mapped image of a PE32 file, over [`securom_unwrap::Pe`]'s parse, plus the file bytes no
//! header or section covers.
//!
//! The mapped image is defined here, once, and the comparison in [`crate::membership`] reads it:
//!
//! * the headers map RVA `[0, SizeOfHeaders)` to file offset `[0, SizeOfHeaders)`;
//! * a section maps RVA `[rva, rva + align_up(max(VirtualSize, SizeOfRawData), SectionAlignment))`;
//!   offset `k` reads file byte `PointerToRawData + k` while `k < SizeOfRawData`, and `0` past it;
//! * an RVA outside every extent is unmapped.
//!
//! Anything that would make that view ambiguous — headers or a raw range past the end of the file,
//! two extents that overlap, two raw ranges that overlap, a zero section alignment — is an
//! [`Error`], never resolved by a guess.

use std::ops::Range;

use securom_unwrap::Pe;

use crate::Error;

/// One mapped RVA range and what backs it.
#[derive(Clone, Debug)]
struct Extent {
    rva: Range<u32>,
    /// `None` for the headers, which map straight onto the file.
    section: Option<usize>,
}

/// A piece of [`Image::view`]: an RVA range, and its bytes when the image maps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub rva: Range<u32>,
    pub bytes: Option<Vec<u8>>,
}

/// A parsed PE32 file and its mapped image.
pub struct Image<'a> {
    pub pe: Pe<'a>,
    extents: Vec<Extent>,
    slack: Vec<Range<u64>>,
}

fn align_up(x: u64, a: u64) -> u64 {
    x.div_ceil(a) * a
}

impl<'a> Image<'a> {
    /// Parse `bytes` as a PE32 file and lay out its mapped image.
    pub fn parse(bytes: &'a [u8]) -> Result<Image<'a>, Error> {
        let pe = Pe::parse(bytes).map_err(|e| Error::new(format!("PE parse: {e}")))?;
        if pe.section_alignment == 0 {
            return Err(Error::new("PE declares SectionAlignment 0"));
        }
        if pe.size_of_headers as usize > bytes.len() {
            return Err(Error::new(format!(
                "SizeOfHeaders 0x{:x} runs past the end of the {}-byte file",
                pe.size_of_headers,
                bytes.len()
            )));
        }
        for s in &pe.sections {
            if s.raw_ptr as u64 + s.raw_size as u64 > bytes.len() as u64 {
                return Err(Error::new(format!(
                    "section {} raw data 0x{:x}+0x{:x} runs past the end of the {}-byte file",
                    s.name,
                    s.raw_ptr,
                    s.raw_size,
                    bytes.len()
                )));
            }
        }

        let mut extents = vec![Extent { rva: 0..pe.size_of_headers, section: None }];
        for (i, s) in pe.sections.iter().enumerate() {
            let span = align_up(s.vsize.max(s.raw_size) as u64, pe.section_alignment as u64);
            let end = s.vaddr as u64 + span;
            if end > u32::MAX as u64 {
                return Err(Error::new(format!("section {} extent ends past 4 GiB", s.name)));
            }
            extents.push(Extent { rva: s.vaddr..end as u32, section: Some(i) });
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

        let mut covered: Vec<Range<u64>> = std::iter::once(0..pe.size_of_headers as u64)
            .chain(
                pe.sections
                    .iter()
                    .filter(|s| s.raw_size > 0)
                    .map(|s| s.raw_ptr as u64..s.raw_ptr as u64 + s.raw_size as u64),
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

        Ok(Image { pe, extents, slack })
    }

    /// The whole file.
    pub fn bytes(&self) -> &'a [u8] {
        self.pe.data()
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
        let bytes = self.pe.data();
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
                        None => bytes[k..k + n].to_vec(),
                        Some(i) => {
                            let s = &self.pe.sections[i];
                            let avail = (s.raw_size as usize).saturating_sub(k).min(n);
                            let mut v = Vec::with_capacity(n);
                            if avail > 0 {
                                let from = s.raw_ptr as usize + k;
                                v.extend_from_slice(&bytes[from..from + avail]);
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
    fn reads_the_header_fields_and_section_table_from_the_shared_parser() {
        let d = two_sections();
        let img = Image::parse(&d).unwrap();
        assert_eq!(img.pe.timestamp, 0x1234);
        assert_eq!(img.pe.image_base, 0x400000);
        assert_eq!(img.pe.section_alignment, 0x1000);
        assert_eq!(img.pe.size_of_headers, 0x200);
        assert_eq!(img.pe.sections.len(), 2);
        assert_eq!(img.pe.sections[0].name, ".text");
        let s = &img.pe.sections[1];
        assert_eq!((s.vaddr, s.raw_ptr, s.raw_size), (0x3000, 0x300, 0x100));
        assert_eq!(img.section_extent(0), 0x1000..0x2000);
    }

    #[test]
    fn the_view_zero_fills_past_raw_data_and_marks_gaps_unmapped() {
        let d = two_sections();
        let img = Image::parse(&d).unwrap();
        let v = img.view(0x10fe..0x2002);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].rva, 0x10fe..0x2000);
        let b = v[0].bytes.as_ref().unwrap();
        assert_eq!(&b[..2], &[0xaa, 0xaa]);
        assert!(b[2..].iter().all(|&x| x == 0));
        assert_eq!(v[1], Piece { rva: 0x2000..0x2002, bytes: None });
        assert!(img.mapped(0x1ffe..0x2001).is_err());
        assert_eq!(img.mapped(0..2).unwrap(), b"MZ");
    }

    #[test]
    fn slack_is_every_file_byte_outside_headers_and_raw_data() {
        let d = two_sections();
        let img = Image::parse(&d).unwrap();
        assert_eq!(img.slack(), std::slice::from_ref(&(0x400..0x404)));
    }

    #[test]
    fn rejects_what_it_cannot_view_unambiguously() {
        assert!(Image::parse(b"XX").err().unwrap().to_string().contains("not a 32-bit PE"));

        let mut d = two_sections();
        d[0x80] = b'Q';
        assert!(Image::parse(&d).err().unwrap().to_string().contains("not a 32-bit PE"));

        let mut d = two_sections();
        d[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
        assert!(Image::parse(&d).err().unwrap().to_string().contains("not a 32-bit PE"));

        let d = pe(0, &[Sec { name: "a", rva: 0x1000, virtual_size: 0x10, raw_offset: 0x200, raw: vec![1; 0x10] }], b"");
        let mut short = d.clone();
        short.truncate(0x208);
        assert!(Image::parse(&short).err().unwrap().to_string().contains("past the end"));

        let d = pe(
            0,
            &[
                Sec { name: "a", rva: 0x1000, virtual_size: 0x1800, raw_offset: 0x200, raw: vec![1; 0x10] },
                Sec { name: "b", rva: 0x2000, virtual_size: 0x10, raw_offset: 0x300, raw: vec![1; 0x10] },
            ],
            b"",
        );
        assert!(Image::parse(&d).err().unwrap().to_string().contains("extents overlap"));
    }
}
