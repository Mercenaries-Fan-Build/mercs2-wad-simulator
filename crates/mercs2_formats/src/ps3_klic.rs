//! Title-specific EDAT klicensee recovery (port of `klic_scan_cpu.py` /
//! `klic_scan_gpu.py`).
//!
//! Oracle (RPCS3 `validate_dev_klic`): a 16-byte window `w` in some binary is the
//! title klicensee iff
//!   `CMAC(AES128, key = w ^ NP_OMAC_KEY_2, msg = EDAT[0:0x60]) == EDAT[0x60:0x70]`.
//!
//! For MERCS2 WIF the klicensee is not in the disc EBOOT (DLC is post-launch); it
//! is a verbatim 16-byte constant at offset 0x103f498 in the **v1.03 patched**
//! (decrypted) EBOOT. This scanner slides that oracle over an arbitrary binary.
//! Single-threaded, but the `aes` block function is AES-NI accelerated (the
//! workspace builds this crate at opt-level 3 even in dev), so a full pass over a
//! multi-MB EBOOT is a matter of seconds.

use crate::ps3_crypto::aes128_cmac;
use crate::ps3_keys::np_omac_key_2;

/// Split an EDAT header into `(msg[0:0x60], target[0x60:0x70])`.
fn split_head(edat_head: &[u8]) -> Result<(&[u8], [u8; 16]), String> {
    if edat_head.len() < 0x70 {
        return Err(format!(
            "EDAT header too short: need >= 0x70, got 0x{:x}",
            edat_head.len()
        ));
    }
    let mut target = [0u8; 16];
    target.copy_from_slice(&edat_head[0x60..0x70]);
    Ok((&edat_head[0..0x60], target))
}

/// True if `klic` is the klicensee for the EDAT whose first 0x70 header bytes are
/// given (the `validate_dev_klic` check).
pub fn validate_klicensee(edat_head: &[u8], klic: &[u8; 16]) -> bool {
    let (msg, target) = match split_head(edat_head) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut key = [0u8; 16];
    let omac2 = np_omac_key_2();
    for i in 0..16 {
        key[i] = klic[i] ^ omac2[i];
    }
    aes128_cmac(&key, msg) == target
}

/// Slide the klicensee oracle over `haystack`, returning every `(offset, klic)`
/// whose 16-byte window validates against the EDAT header.
pub fn scan_for_klicensee(edat_head: &[u8], haystack: &[u8]) -> Result<Vec<(usize, [u8; 16])>, String> {
    let (msg, target) = split_head(edat_head)?;
    let omac2 = np_omac_key_2();
    let mut hits = Vec::new();
    if haystack.len() < 16 {
        return Ok(hits);
    }
    let mut key = [0u8; 16];
    for off in 0..=haystack.len() - 16 {
        let w = &haystack[off..off + 16];
        for i in 0..16 {
            key[i] = w[i] ^ omac2[i];
        }
        if aes128_cmac(&key, msg) == target {
            let mut klic = [0u8; 16];
            klic.copy_from_slice(w);
            hits.push((off, klic));
        }
    }
    Ok(hits)
}
