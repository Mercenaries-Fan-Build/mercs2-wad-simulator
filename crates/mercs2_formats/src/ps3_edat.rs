//! NPDRM EDAT decryptor (port of `edat_decrypt.py`, itself the data-decrypt path
//! of RPCS3 `unedat.cpp`).
//!
//! Handles the retail case for MERCS2 WIF `DLC01.EDAT`: npd version 2, license
//! type 3, ENCRYPTED_KEY (0x08) set, not COMPRESSED (0x01) / FLAG_0x20 (0x20).
//! Per-block HMAC/CMAC verification is skipped (as RPCS3 does at runtime); the
//! correctness oracle is the decrypted WAD magic (`SCFF`). The klicensee is a
//! caller input — for DLC01 it is [`crate::ps3_keys::dlc01_klicensee`], recovered
//! separately by [`crate::ps3_klic`].

use crate::ps3_crypto::{aes128_cbc_decrypt, aes128_ecb_encrypt_block};
use crate::ps3_keys::{edat_key_0, edat_key_1};

fn i32_be(d: &[u8], o: usize) -> i32 {
    i32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn u64_be(d: &[u8], o: usize) -> u64 {
    u64::from_be_bytes([
        d[o], d[o + 1], d[o + 2], d[o + 3], d[o + 4], d[o + 5], d[o + 6], d[o + 7],
    ])
}

/// The NPD header (first 0x80 bytes) of an EDAT/SDAT file.
#[derive(Clone, Debug)]
pub struct NpdHeader {
    pub version: i32,
    pub license: i32,
    pub kind: i32,
    pub digest: [u8; 16],
    pub title_hash: [u8; 16],
    pub dev_hash: [u8; 16],
    pub flags: i32,
    pub block_size: i32,
    pub file_size: u64,
}

impl NpdHeader {
    pub fn total_blocks(&self) -> usize {
        let bs = self.block_size as u64;
        self.file_size.div_ceil(bs) as usize
    }
}

/// Parse the NPD + EDAT header fields.
pub fn parse_headers(blob: &[u8]) -> Result<NpdHeader, String> {
    if blob.len() < 0x90 || &blob[0..4] != b"NPD\0" {
        return Err(format!(
            "not an EDAT/NPD (magic {:02x?})",
            &blob[0..4.min(blob.len())]
        ));
    }
    let mut digest = [0u8; 16];
    let mut title_hash = [0u8; 16];
    let mut dev_hash = [0u8; 16];
    digest.copy_from_slice(&blob[0x40..0x50]);
    title_hash.copy_from_slice(&blob[0x50..0x60]);
    dev_hash.copy_from_slice(&blob[0x60..0x70]);
    Ok(NpdHeader {
        version: i32_be(blob, 4),
        license: i32_be(blob, 8),
        kind: i32_be(blob, 12),
        digest,
        title_hash,
        dev_hash,
        flags: i32_be(blob, 0x80),
        block_size: i32_be(blob, 0x84),
        file_size: u64_be(blob, 0x88),
    })
}

fn block_key(block: usize, npd: &NpdHeader) -> [u8; 16] {
    let mut out = [0u8; 16];
    if npd.version > 1 {
        out[..0xC].copy_from_slice(&npd.dev_hash[..0xC]);
    }
    out[0xC..].copy_from_slice(&(block as u32).to_be_bytes());
    out
}

/// Decrypt a single EDAT block. `klic` is the crypt key (klicensee).
fn decrypt_block(
    blob: &[u8],
    npd: &NpdHeader,
    klic: &[u8; 16],
    block_num: usize,
    total_blocks: usize,
) -> Result<Vec<u8>, String> {
    let flags = npd.flags;
    let bs = npd.block_size as usize;
    let compressed = flags & 0x01 != 0;
    let flag20 = flags & 0x20 != 0;
    if compressed || flag20 {
        return Err(format!(
            "unsupported EDAT flags 0x{:08x} (COMPRESSED/FLAG_0x20 not handled)",
            flags as u32
        ));
    }
    let meta_size = 0x10usize; // simple layout (no COMPRESSED/FLAG_0x20)
    let meta_off = 0x100usize;

    let offset = meta_off + block_num * bs + total_blocks * meta_size;
    let mut length = bs;
    if block_num == total_blocks - 1 && !npd.file_size.is_multiple_of(bs as u64) {
        length = (npd.file_size % bs as u64) as usize;
    }
    let pad_length = length;
    let length = (length + 0xF) & !0xF;
    if offset + length > blob.len() {
        return Err(format!(
            "block {block_num} at 0x{offset:x}+0x{length:x} overruns file 0x{:x}",
            blob.len()
        ));
    }
    let enc = &blob[offset..offset + length];

    // per-block key: ECB-encrypt the block_key with the klicensee
    let key_result = aes128_ecb_encrypt_block(klic, &block_key(block_num, npd));

    // ENCRYPTED_KEY (0x08): key_final = CBC-decrypt(EDAT_KEY, iv=0, key_result)
    let key_final = if flags & 0x08 != 0 {
        let edat_key = if npd.version == 4 { edat_key_1() } else { edat_key_0() };
        let d = aes128_cbc_decrypt(&edat_key, &[0u8; 16], &key_result);
        let mut k = [0u8; 16];
        k.copy_from_slice(&d);
        k
    } else {
        key_result
    };

    let iv_final = if npd.version <= 1 { [0u8; 16] } else { npd.digest };

    let dec = if flags & 0x02 != 0 {
        enc.to_vec() // FLAG_0x02: no algorithm (copy)
    } else {
        aes128_cbc_decrypt(&key_final, &iv_final, enc)
    };
    Ok(dec[..pad_length.min(dec.len())].to_vec())
}

/// Decrypt an entire EDAT into its plaintext payload using `klic`.
pub fn decrypt_edat(blob: &[u8], klic: &[u8; 16]) -> Result<Vec<u8>, String> {
    let npd = parse_headers(blob)?;
    let total = npd.total_blocks();
    let mut out = Vec::with_capacity(npd.file_size as usize);
    for i in 0..total {
        out.extend_from_slice(&decrypt_block(blob, &npd, klic, i, total)?);
    }
    Ok(out)
}

/// Convenience: decrypt just block 0 (16-byte-aligned head) to sniff the payload
/// magic — used to confirm a candidate klicensee produces a real WAD (`SCFF`).
pub fn decrypt_block0(blob: &[u8], klic: &[u8; 16]) -> Result<Vec<u8>, String> {
    let npd = parse_headers(blob)?;
    decrypt_block(blob, &npd, klic, 0, npd.total_blocks())
}
