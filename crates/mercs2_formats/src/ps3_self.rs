//! PS3 SCE SELF → plain ELF decryptor (port of `unself_decrypt.py`, following
//! RPCS3 `unself.cpp`: LoadHeaders / DecryptNPDRM / LoadMetadata / DecryptData /
//! MakeElf).
//!
//! Two keysets are supported, both key_revision 0x0001:
//! - [`SelfKind::App`] — a disc APP SELF (`self_type = 4`), appldr AES-256 only.
//! - [`SelfKind::Npdrm`] — an NPDRM SELF (`self_type = 8`, the v1.03 patch EBOOT),
//!   which has an extra NPDRM CBC layer (`AES128_ECB_dec(NP_KLIC_KEY, NP_KLIC_FREE)`
//!   → CBC-decrypt the metadata_info) before the appldr step.
//!
//! The recovered plain ELF is a PPC64 (`e_machine = 0x15`) ET_EXEC; that, plus the
//! decrypted klicensee living inside it, is why this path exists (see
//! [`crate::ps3_klic`]).

use crate::ps3_crypto::{
    aes128_cbc_decrypt, aes128_ctr, aes128_ecb_decrypt_block, aes256_cbc_decrypt,
};
use crate::ps3_keys::{appldr_app_keyset, appldr_npdrm_keyset, np_klic_free, np_klic_key};
use std::io::Read;

/// Which appldr keyset / NPDRM path to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfKind {
    App,
    Npdrm,
}

fn u16b(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}
fn u32b(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn u64b(d: &[u8], o: usize) -> u64 {
    u64::from_be_bytes([
        d[o], d[o + 1], d[o + 2], d[o + 3], d[o + 4], d[o + 5], d[o + 6], d[o + 7],
    ])
}
fn slice(d: &[u8], o: usize, n: usize) -> Result<&[u8], String> {
    d.get(o..o + n)
        .ok_or_else(|| format!("SELF truncated: need 0x{n:x} bytes @ 0x{o:x}, file 0x{:x}", d.len()))
}

fn zlib_inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib inflate: {e}"))?;
    Ok(out)
}

#[derive(Clone, Debug)]
struct MetaSection {
    data_offset: u64,
    data_size: u64,
    seg_type: u32,
    program_idx: u32,
    encrypted: u32,
    key_idx: u32,
    iv_idx: u32,
    compressed: u32,
}

/// Decrypt a SCE SELF to a plain ELF image. Returns the ELF bytes.
pub fn decrypt_self(data: &[u8], kind: SelfKind) -> Result<Vec<u8>, String> {
    if slice(data, 0, 4)? != b"SCE\0" {
        return Err(format!("bad SCE magic {:02x?}", &data[0..4.min(data.len())]));
    }
    let se_flags = u16b(data, 0x08); // key_revision
    let se_meta = u32b(data, 0x0C) as usize; // metadata_offset
    let se_hsize = u64b(data, 0x10) as usize; // header_len
    const SCE_HDR: usize = 0x20;

    // ext header @ 0x20
    let ehdr_off = u64b(data, SCE_HDR + 0x10) as usize;
    let phdr_off = u64b(data, SCE_HDR + 0x18) as usize;
    let shdr_off = u64b(data, SCE_HDR + 0x20) as usize;

    // cleartext ELF64 header
    let e = slice(data, ehdr_off, 0x40)?.to_vec();
    if &e[0..4] != b"\x7fELF" || e[4] != 2 {
        return Err("expected ELF64 cleartext header".into());
    }
    let e_phnum = u16b(&e, 0x38) as usize;
    let e_shnum = u16b(&e, 0x3C) as usize;
    let e_phoff = u64b(&e, 0x20) as usize;
    let e_shoff = u64b(&e, 0x28) as usize;

    // program header p_offset table (needed by MakeElf)
    let mut ph_offsets = Vec::with_capacity(e_phnum);
    for i in 0..e_phnum {
        let po = phdr_off + i * 0x38;
        ph_offsets.push(u64b(slice(data, po + 0x08, 8)?, 0) as usize);
    }

    // ---- LoadMetadata ----
    const META_INFO: usize = 0x40;
    let mi_off = se_meta + SCE_HDR;
    let mut metadata_info = slice(data, mi_off, META_INFO)?.to_vec();
    let mh_off = se_meta + SCE_HDR + META_INFO;
    let mh_size = se_hsize
        .checked_sub(SCE_HDR + se_meta + META_INFO)
        .ok_or("bad header_len/metadata_offset")?;
    let metadata_headers = slice(data, mh_off, mh_size)?.to_vec();

    let mi: Vec<u8> = if (se_flags & 0x8000) != 0x8000 {
        if kind == SelfKind::Npdrm {
            // NPDRM pre-decrypt: strip the klicensee CBC layer
            let npdrm_key = aes128_ecb_decrypt_block(&np_klic_key(), &np_klic_free());
            metadata_info = aes128_cbc_decrypt(&npdrm_key, &[0u8; 16], &metadata_info);
        }
        let (erk, riv) = match kind {
            SelfKind::App => appldr_app_keyset(),
            SelfKind::Npdrm => appldr_npdrm_keyset(),
        };
        aes256_cbc_decrypt(&erk, &riv, &metadata_info)
    } else {
        metadata_info
    };

    let mut m_key = [0u8; 16];
    let mut m_iv = [0u8; 16];
    m_key.copy_from_slice(&mi[0x00..0x10]);
    m_iv.copy_from_slice(&mi[0x20..0x30]);
    if mi[0x10] != 0 || mi[0x30] != 0 {
        return Err("metadata_info decrypt failed (pad nonzero) — wrong keyset/kind".into());
    }

    let mhd = aes128_ctr(&m_key, &m_iv, &metadata_headers);
    let section_count = u32b(slice(&mhd, 12, 4)?, 0) as usize;
    let key_count = u32b(slice(&mhd, 16, 4)?, 0) as usize;

    const META_HDR: usize = 0x20;
    const SEC_HDR: usize = 0x30;
    let mut sections = Vec::with_capacity(section_count);
    for i in 0..section_count {
        let o = META_HDR + i * SEC_HDR;
        let s = slice(&mhd, o, SEC_HDR)?;
        sections.push(MetaSection {
            data_offset: u64b(s, 0),
            data_size: u64b(s, 0x08),
            seg_type: u32b(s, 0x10),
            program_idx: u32b(s, 0x14),
            encrypted: u32b(s, 0x20),
            key_idx: u32b(s, 0x24),
            iv_idx: u32b(s, 0x28),
            compressed: u32b(s, 0x2C),
        });
    }
    let keys_off = META_HDR + section_count * SEC_HDR;
    let data_keys = slice(&mhd, keys_off, key_count * 0x10)?.to_vec();

    // ---- DecryptData ----
    let mut data_buf = Vec::new();
    for s in &sections {
        if s.encrypted == 3 && (s.key_idx as usize) < key_count && (s.iv_idx as usize) < key_count {
            let ki = s.key_idx as usize * 0x10;
            let vi = s.iv_idx as usize * 0x10;
            let mut dk = [0u8; 16];
            let mut dv = [0u8; 16];
            dk.copy_from_slice(&data_keys[ki..ki + 16]);
            dv.copy_from_slice(&data_keys[vi..vi + 16]);
            let enc = slice(data, s.data_offset as usize, s.data_size as usize)?;
            data_buf.extend_from_slice(&aes128_ctr(&dk, &dv, enc));
        }
    }

    // ---- MakeElf ----
    let mut out: Vec<u8> = Vec::new();
    let wr = |off: usize, b: &[u8], out: &mut Vec<u8>| {
        if out.len() < off + b.len() {
            out.resize(off + b.len(), 0);
        }
        out[off..off + b.len()].copy_from_slice(b);
    };
    wr(0, &e[0..0x40], &mut out);
    wr(e_phoff, slice(data, phdr_off, e_phnum * 0x38)?, &mut out);
    if e_shnum != 0 {
        wr(e_shoff, slice(data, shdr_off, e_shnum * 0x40)?, &mut out);
    }

    let mut data_buf_offset = 0usize;
    for s in &sections {
        if s.seg_type == 2 {
            let sz = s.data_size as usize;
            let raw = data_buf
                .get(data_buf_offset..data_buf_offset + sz)
                .ok_or("data_buf underrun assembling ELF")?;
            let seg = if s.compressed == 2 {
                zlib_inflate(raw)?
            } else {
                raw.to_vec()
            };
            let poff = *ph_offsets
                .get(s.program_idx as usize)
                .ok_or("section program_idx out of range")?;
            wr(poff, &seg, &mut out);
            data_buf_offset += sz;
        }
    }

    Ok(out)
}

/// Cheap sanity oracle: a decrypted PS3 ELF is a big-endian ELF64 for PPC64.
pub fn is_ppc64_elf(elf: &[u8]) -> bool {
    elf.len() >= 0x14
        && &elf[0..4] == b"\x7fELF"
        && elf[4] == 2 // ELFCLASS64
        && elf[5] == 2 // ELFDATA2MSB (big-endian)
        && u16b(elf, 0x12) == 0x15 // EM_PPC64
}
