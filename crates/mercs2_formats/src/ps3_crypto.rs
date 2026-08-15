//! Minimal AES primitives for the PS3 DLC decrypt chain, built on the `aes`
//! block cipher plus `cmac` for AES-CMAC. CTR / CBC / single-block ECB are
//! implemented here by hand rather than pulling the `ctr`/`cbc`/`ecb` mode
//! crates — the chain only needs these few shapes, and keeping the modes local
//! keeps `mercs2_formats`' dependency surface to `aes` + `cmac`.
//!
//! Ports the `aes_ctr` / `aes_cbc_dec` / `aes_ecb_enc` helpers used across
//! `ps3_pkg_unpack.py`, `edat_decrypt.py` and `unself_decrypt.py` (which call
//! pycryptodome). Verified against FIPS-197, NIST SP 800-38A and RFC 4493
//! known-answer vectors in the tests below.

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::{Aes128, Aes256};
use cmac::{Cmac, Mac};

/// Encrypt one 16-byte block with AES-128 in ECB (i.e. the raw block function).
pub fn aes128_ecb_encrypt_block(key: &[u8; 16], block: &[u8; 16]) -> [u8; 16] {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut b = *GenericArray::from_slice(block);
    cipher.encrypt_block(&mut b);
    b.into()
}

/// Decrypt one 16-byte block with AES-128 in ECB.
pub fn aes128_ecb_decrypt_block(key: &[u8; 16], block: &[u8; 16]) -> [u8; 16] {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut b = *GenericArray::from_slice(block);
    cipher.decrypt_block(&mut b);
    b.into()
}

/// Increment a 128-bit big-endian counter in place (CTR mode).
fn inc_be_128(ctr: &mut [u8; 16]) {
    for byte in ctr.iter_mut().rev() {
        let (v, carry) = byte.overflowing_add(1);
        *byte = v;
        if !carry {
            break;
        }
    }
}

/// AES-128-CTR. `iv` is the full 128-bit big-endian initial counter block.
/// CTR is symmetric, so this is both encrypt and decrypt.
pub fn aes128_ctr(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut counter = *iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut ks = *GenericArray::from_slice(&counter);
        cipher.encrypt_block(&mut ks);
        for (i, &b) in chunk.iter().enumerate() {
            out.push(b ^ ks[i]);
        }
        inc_be_128(&mut counter);
    }
    out
}

/// AES-128-CBC decrypt, no padding. `data.len()` must be a multiple of 16;
/// any trailing partial block is ignored (matches the block-aligned callers).
pub fn aes128_cbc_decrypt(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut prev = *iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks_exact(16) {
        let mut block = *GenericArray::from_slice(chunk);
        cipher.decrypt_block(&mut block);
        for i in 0..16 {
            out.push(block[i] ^ prev[i]);
        }
        prev.copy_from_slice(chunk);
    }
    out
}

/// AES-256-CBC decrypt, no padding (the appldr metadata_info step). `data.len()`
/// must be a multiple of 16.
pub fn aes256_cbc_decrypt(key: &[u8; 32], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let cipher = Aes256::new(GenericArray::from_slice(key));
    let mut prev = *iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks_exact(16) {
        let mut block = *GenericArray::from_slice(chunk);
        cipher.decrypt_block(&mut block);
        for i in 0..16 {
            out.push(block[i] ^ prev[i]);
        }
        prev.copy_from_slice(chunk);
    }
    out
}

/// AES-128-CMAC over `msg` with a 16-byte key.
pub fn aes128_cmac(key: &[u8; 16], msg: &[u8]) -> [u8; 16] {
    let mut mac = <Cmac<Aes128> as Mac>::new_from_slice(key).expect("16-byte key");
    mac.update(msg);
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hx(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }
    fn a16(s: &str) -> [u8; 16] {
        hx(s).try_into().unwrap()
    }
    fn a32(s: &str) -> [u8; 32] {
        hx(s).try_into().unwrap()
    }

    #[test]
    fn fips197_aes128_block() {
        // FIPS-197 Appendix B / C.1
        let key = a16("000102030405060708090a0b0c0d0e0f");
        let pt = a16("00112233445566778899aabbccddeeff");
        let ct = aes128_ecb_encrypt_block(&key, &pt);
        assert_eq!(&ct[..], &hx("69c4e0d86a7b0430d8cdb78070b4c55a")[..]);
        // round-trip
        assert_eq!(aes128_ecb_decrypt_block(&key, &ct), pt);
    }

    #[test]
    fn fips197_aes256_via_cbc_zero_iv() {
        // AES-256 single block (FIPS-197 C.3); drive it through CBC with a zero IV
        // and zero chaining (single block) to exercise aes256_cbc_decrypt's cipher.
        let key = a32("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let ct = a16("8ea2b7ca516745bfeafc49904b496089");
        let iv = [0u8; 16];
        // CBC-decrypt of one block with zero IV == raw block decrypt.
        let pt = aes256_cbc_decrypt(&key, &iv, &ct);
        assert_eq!(&pt[..], &hx("00112233445566778899aabbccddeeff")[..]);
    }

    #[test]
    fn nist_ctr_aes128_f5() {
        // NIST SP 800-38A F.5.1 CTR-AES128.Encrypt
        let key = a16("2b7e151628aed2a6abf7158809cf4f3c");
        let iv = a16("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
        let pt = hx("6bc1bee22e409f96e93d7e117393172a\
                     ae2d8a571e03ac9c9eb76fac45af8e51");
        let ct = hx("874d6191b620e3261bef6864990db6ce\
                     9806f66b7970fdff8617187bb9fffdff");
        assert_eq!(aes128_ctr(&key, &iv, &pt), ct);
        // symmetric
        assert_eq!(aes128_ctr(&key, &iv, &ct), pt);
    }

    #[test]
    fn nist_cbc_aes128_f2() {
        // NIST SP 800-38A F.2.2 CBC-AES128.Decrypt
        let key = a16("2b7e151628aed2a6abf7158809cf4f3c");
        let iv = a16("000102030405060708090a0b0c0d0e0f");
        let ct = hx("7649abac8119b246cee98e9b12e9197d\
                     5086cb9b507219ee95db113a917678b2");
        let pt = hx("6bc1bee22e409f96e93d7e117393172a\
                     ae2d8a571e03ac9c9eb76fac45af8e51");
        assert_eq!(aes128_cbc_decrypt(&key, &iv, &ct), pt);
    }

    #[test]
    fn rfc4493_cmac_aes128() {
        // RFC 4493 test vectors
        let key = a16("2b7e151628aed2a6abf7158809cf4f3c");
        assert_eq!(
            &aes128_cmac(&key, b"")[..],
            &hx("bb1d6929e95937287fa37d129b756746")[..]
        );
        let msg = hx("6bc1bee22e409f96e93d7e117393172a");
        assert_eq!(
            &aes128_cmac(&key, &msg)[..],
            &hx("070a16b46b4d4144f79bdd9dd04a287c")[..]
        );
    }
}
