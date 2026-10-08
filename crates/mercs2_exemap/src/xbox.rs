//! Xbox 360 executable intake: XEX2 → PE image, and the PE layout Ghidra's PE loader reads.
//!
//! An XEX2 file carries the game's PE as a "basefile": optionally AES-128-CBC encrypted under a
//! per-file session key, and optionally compressed. [`unpack_xex`] reverses both:
//!
//! 1. The session key sits wrapped at security-info offset `+0x150`; it is AES-128-ECB decrypted
//!    with the retail or devkit key-encryption key ([`XexKey`]).
//! 2. The basefile at the PE data offset is AES-128-CBC decrypted (IV zero).
//! 3. Compression `2` (normal) is a chain of blocks, each `{u32 next_block_size, u8 hash[20],
//!    (u16 chunk_len, chunk)*, u16 0}`; the chunks concatenate into one LZX stream (window from the
//!    base-file-format header), decompressed to the image size from the security info.
//!    Compression `1` (basic) is a list of `{u32 data_size, u32 zero_size}` runs. Compression `0`
//!    is the basefile itself.
//!
//! A wrong key-encryption key shows up as a block chain that does not fit the basefile, which is
//! an error naming the key; nothing is retried with the other key.
//!
//! The recovered PE is laid out by RVA (file offset = RVA) while its section headers keep the
//! original packed `PointerToRawData`. [`ghidra_pe`] rewrites the headers to match the RVA layout.

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes128;
use securom_unwrap::Pe;

use crate::Error;

/// `FileHeader.Machine` of an Xbox 360 executable.
pub const MACHINE_XENON: u16 = 0x1f2;

/// The key-encryption key that wraps an XEX's session key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XexKey {
    Retail,
    Devkit,
}

impl XexKey {
    fn kek(self) -> [u8; 16] {
        match self {
            XexKey::Retail => [
                0x20, 0xB1, 0x85, 0xA5, 0x9D, 0x28, 0xFD, 0xC3, 0x40, 0x58, 0x3F, 0xBB, 0x08, 0x96, 0xBF, 0x91,
            ],
            XexKey::Devkit => [0u8; 16],
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            XexKey::Retail => "retail",
            XexKey::Devkit => "devkit",
        }
    }
}

impl std::str::FromStr for XexKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<XexKey, Error> {
        match s {
            "retail" => Ok(XexKey::Retail),
            "devkit" => Ok(XexKey::Devkit),
            other => Err(Error::new(format!("unknown XEX key {other:?}; expected retail or devkit"))),
        }
    }
}

/// What [`unpack_xex`] found in the headers, for reporting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XexInfo {
    pub encrypted: bool,
    /// Base-file compression: 0 none, 1 basic, 2 normal (LZX).
    pub compression: u16,
    pub image_size: u32,
    /// LZX window size for compression 2, else 0.
    pub window_size: u32,
    /// Compressed blocks for compression 2, else 0.
    pub blocks: usize,
    /// Concatenated LZX stream length for compression 2, else 0.
    pub stream_len: usize,
}

fn be32(d: &[u8], o: usize, what: &str) -> Result<u32, Error> {
    d.get(o..o + 4)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::new(format!("XEX truncated reading {what} at 0x{o:x}")))
}

fn be16(d: &[u8], o: usize, what: &str) -> Result<u16, Error> {
    d.get(o..o + 2)
        .map(|b| u16::from_be_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::new(format!("XEX truncated reading {what} at 0x{o:x}")))
}

/// Optional-header key of the base file format descriptor.
const OPT_BASE_FILE_FORMAT: u32 = 0x0000_03FF;

/// Recover the PE image from an XEX2 file.
pub fn unpack_xex(xex: &[u8], key: XexKey) -> Result<(Vec<u8>, XexInfo), Error> {
    if xex.get(0..4) != Some(b"XEX2") {
        return Err(Error::new("not an XEX2 file (magic is not \"XEX2\")"));
    }
    let pe_off = be32(xex, 0x08, "PE data offset")? as usize;
    let sec_off = be32(xex, 0x10, "security info offset")? as usize;
    let opt_count = be32(xex, 0x14, "optional header count")? as usize;
    let mut bff = None;
    for i in 0..opt_count {
        let k = be32(xex, 0x18 + i * 8, "optional header key")?;
        let v = be32(xex, 0x18 + i * 8 + 4, "optional header value")?;
        if k == OPT_BASE_FILE_FORMAT {
            bff = Some(v as usize);
        }
    }
    let bff = bff.ok_or_else(|| Error::new("XEX has no base file format optional header (0x3FF)"))?;
    let image_size = be32(xex, sec_off + 4, "image size")?;
    let encryption = be16(xex, bff + 4, "encryption type")?;
    let compression = be16(xex, bff + 6, "compression type")?;
    if pe_off > xex.len() {
        return Err(Error::new(format!("XEX PE data offset 0x{pe_off:x} is past the end of the file")));
    }

    let mut image = xex[pe_off..].to_vec();
    match encryption {
        0 => {}
        1 => {
            let wrapped = xex
                .get(sec_off + 0x150..sec_off + 0x160)
                .ok_or_else(|| Error::new("XEX truncated reading the wrapped session key"))?;
            let kek = Aes128::new(GenericArray::from_slice(&key.kek()));
            let mut file_key = GenericArray::clone_from_slice(wrapped);
            kek.decrypt_block(&mut file_key);
            let cipher = Aes128::new(&file_key);
            let n = image.len() & !0xF;
            image.truncate(n);
            let mut prev = [0u8; 16];
            for block in image.chunks_exact_mut(16) {
                let ct: [u8; 16] = block.try_into().unwrap();
                let mut b = GenericArray::clone_from_slice(block);
                cipher.decrypt_block(&mut b);
                for (o, (p, q)) in block.iter_mut().zip(b.iter().zip(prev.iter())) {
                    *o = p ^ q;
                }
                prev = ct;
            }
        }
        other => return Err(Error::new(format!("XEX encryption type {other} is not handled"))),
    }

    let mut info = XexInfo {
        encrypted: encryption == 1,
        compression,
        image_size,
        window_size: 0,
        blocks: 0,
        stream_len: 0,
    };

    let pe = match compression {
        0 => {
            let n = image_size as usize;
            if image.len() < n {
                return Err(Error::new(format!(
                    "XEX basefile is {} bytes, shorter than the {n}-byte image",
                    image.len()
                )));
            }
            image.truncate(n);
            image
        }
        1 => {
            let info_size = be32(xex, bff, "base file format size")? as usize;
            let end = bff + info_size;
            let mut out = Vec::with_capacity(image_size as usize);
            let mut q = bff + 8;
            let mut p = 0usize;
            while q + 8 <= end {
                let data_size = be32(xex, q, "basic block data size")? as usize;
                let zero_size = be32(xex, q + 4, "basic block zero size")? as usize;
                q += 8;
                let src = image.get(p..p + data_size).ok_or_else(|| {
                    Error::new(format!(
                        "XEX basic block of {data_size} bytes at 0x{p:x} runs past the basefile (wrong {} key?)",
                        key.as_str()
                    ))
                })?;
                out.extend_from_slice(src);
                out.resize(out.len() + zero_size, 0);
                p += data_size;
            }
            if out.len() != image_size as usize {
                return Err(Error::new(format!(
                    "XEX basic blocks expand to {} bytes, not the {image_size}-byte image",
                    out.len()
                )));
            }
            out
        }
        2 => {
            let window_size = be32(xex, bff + 8, "LZX window size")?;
            let mut block_size = be32(xex, bff + 12, "first block size")? as usize;
            let mut stream = Vec::new();
            let mut p = 0usize;
            let mut blocks = 0usize;
            while block_size != 0 {
                let pnext = p + block_size;
                if pnext > image.len() {
                    return Err(Error::new(format!(
                        "XEX block {blocks} (0x{block_size:x} bytes at 0x{p:x}) runs past the {}-byte basefile: the {} key does not decrypt this file",
                        image.len(),
                        key.as_str()
                    )));
                }
                let next_size = be32(&image, p, "next block size")? as usize;
                let mut q = p + 4 + 20;
                loop {
                    let chunk = be16(&image, q, "chunk length")? as usize;
                    q += 2;
                    if chunk == 0 {
                        break;
                    }
                    if q + chunk > pnext {
                        return Err(Error::new(format!(
                            "XEX block {blocks}: a {chunk}-byte chunk at 0x{q:x} runs past the block end 0x{pnext:x}: the {} key does not decrypt this file",
                            key.as_str()
                        )));
                    }
                    stream.extend_from_slice(&image[q..q + chunk]);
                    q += chunk;
                }
                p = pnext;
                block_size = next_size;
                blocks += 1;
            }
            info.window_size = window_size;
            info.blocks = blocks;
            info.stream_len = stream.len();
            mercs2_formats::lzx::lzx_decompress(&stream, window_size, image_size as usize)
                .map_err(|e| Error::new(format!("XEX LZX stream: {e}")))?
        }
        other => return Err(Error::new(format!("XEX compression type {other} is not handled"))),
    };

    if pe.get(0..2) != Some(b"MZ") {
        return Err(Error::new(format!(
            "XEX image does not start with \"MZ\": the {} key does not decrypt this file",
            key.as_str()
        )));
    }
    Ok((pe, info))
}

/// Rewrite an Xbox 360 RVA-laid-out PE image so every section's `PointerToRawData` is its
/// `VirtualAddress` and `SizeOfRawData` is the bytes the file holds for it, with `FileAlignment` =
/// `SectionAlignment`.
pub fn ghidra_pe(image: &[u8]) -> Result<Vec<u8>, Error> {
    let pe = Pe::parse(image).map_err(|e| Error::new(format!("PE parse: {e}")))?;
    if pe.machine != MACHINE_XENON {
        return Err(Error::new(format!(
            "PE machine is 0x{:x}, not an Xbox 360 image (0x{MACHINE_XENON:x})",
            pe.machine
        )));
    }
    let mut d = image.to_vec();
    let flen = d.len() as u64;
    let lf = u32::from_le_bytes(d[0x3c..0x40].try_into().unwrap()) as usize;
    let opt = lf + 24;
    let size_opt = u16::from_le_bytes(d[lf + 20..lf + 22].try_into().unwrap()) as usize;
    d[opt + 0x24..opt + 0x28].copy_from_slice(&pe.section_alignment.to_le_bytes());
    let table = opt + size_opt;
    for (i, s) in pe.sections.iter().enumerate() {
        let o = table + i * 40;
        let va = s.vaddr as u64;
        let avail = (va + s.vsize as u64).min(flen).saturating_sub(va) as u32;
        d[o + 16..o + 20].copy_from_slice(&avail.to_le_bytes());
        d[o + 20..o + 24].copy_from_slice(&s.vaddr.to_le_bytes());
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_parse_by_name_only() {
        assert_eq!("retail".parse::<XexKey>().unwrap(), XexKey::Retail);
        assert_eq!("devkit".parse::<XexKey>().unwrap(), XexKey::Devkit);
        assert!("Retail".parse::<XexKey>().is_err());
    }

    #[test]
    fn a_non_xex_file_is_refused() {
        let e = unpack_xex(b"MZ not an xex", XexKey::Retail).unwrap_err().to_string();
        assert!(e.contains("not an XEX2"), "{e}");
    }

    /// An unencrypted, uncompressed XEX2 around `pe`.
    fn plain_xex(pe: &[u8]) -> Vec<u8> {
        let pe_off = 0x200usize;
        let sec_off = 0x100usize;
        let bff = 0x80usize;
        let mut d = vec![0u8; pe_off];
        d[0..4].copy_from_slice(b"XEX2");
        d[0x08..0x0c].copy_from_slice(&(pe_off as u32).to_be_bytes());
        d[0x10..0x14].copy_from_slice(&(sec_off as u32).to_be_bytes());
        d[0x14..0x18].copy_from_slice(&1u32.to_be_bytes());
        d[0x18..0x1c].copy_from_slice(&OPT_BASE_FILE_FORMAT.to_be_bytes());
        d[0x1c..0x20].copy_from_slice(&(bff as u32).to_be_bytes());
        d[sec_off + 4..sec_off + 8].copy_from_slice(&(pe.len() as u32).to_be_bytes());
        d[bff..bff + 4].copy_from_slice(&8u32.to_be_bytes());
        d.extend_from_slice(pe);
        d
    }

    #[test]
    fn an_uncompressed_unencrypted_basefile_is_the_image() {
        let mut pe = vec![0u8; 64];
        pe[0..2].copy_from_slice(b"MZ");
        let (out, info) = unpack_xex(&plain_xex(&pe), XexKey::Retail).unwrap();
        assert_eq!(out, pe);
        assert!(!info.encrypted);
        assert_eq!(info.compression, 0);
    }

    #[test]
    fn an_image_without_mz_is_reported_against_the_key() {
        let e = unpack_xex(&plain_xex(&[0u8; 64]), XexKey::Devkit).unwrap_err().to_string();
        assert!(e.contains("devkit key"), "{e}");
    }

    #[test]
    fn ghidra_pe_points_raw_data_at_the_rva_and_refuses_x86() {
        use crate::starts::tests::pe_image;
        let img = pe_image(MACHINE_XENON, 0x8200_0000, &[(".text", 0x400, 0x100, 0x6000_0020, &[])]);
        let fixed = ghidra_pe(&img).unwrap();
        let pe = Pe::parse(&fixed).unwrap();
        assert_eq!(pe.file_alignment, pe.section_alignment);
        assert_eq!(pe.sections[0].raw_ptr, 0x400);
        assert_eq!(pe.sections[0].raw_size, 0x100);
        let x86 = pe_image(0x14c, 0x40_0000, &[(".text", 0x400, 0x100, 0x6000_0020, &[])]);
        assert!(ghidra_pe(&x86).unwrap_err().to_string().contains("0x14c"));
    }
}
