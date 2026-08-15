//! PS3 crypto key material for the "Blow It Up Again" DLC decrypt chain.
//!
//! Provenance: the retail PSN package key and the appldr / NPDRM keysets are the
//! public constants used by RPCS3 (`Utilities/key_vault.h`, `Crypto/unself.cpp`,
//! `Crypto/unedat.cpp`); this module is the constant half of the Rust port of the
//! four reference scripts (`ps3_pkg_unpack.py`, `edat_decrypt.py`,
//! `unself_decrypt.py`, `klic_scan_*.py`).
//!
//! The one project-specific secret is [`DLC01_KLICENSEE`] — the title-specific
//! EDAT klicensee recovered on 2026-08-01 by sliding-window AES-CMAC over the
//! v1.03 patched EBOOT (see [`crate::ps3_klic`]). It is NOT a public key; it is a
//! recovered fact about this one title, kept here so the decrypt is reproducible.

fn hex16(s: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
    }
    out
}

fn hex32(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
    }
    out
}

/// Retail PSN `.pkg` data-region key (AES-128-CTR, IV = header `riv` @ 0x70).
pub fn ps3_pkg_key() -> [u8; 16] {
    hex16("2e7b71d7c9c9a14ea3221f188828b8f8")
}

/// EDAT key 0 — used for the ENCRYPTED_KEY (0x08) CBC unwrap when npd `version != 4`.
pub fn edat_key_0() -> [u8; 16] {
    hex16("be959ca8308defa2e5e180c63712a9ae")
}

/// EDAT key 1 — used for the ENCRYPTED_KEY unwrap when npd `version == 4`.
pub fn edat_key_1() -> [u8; 16] {
    hex16("4ca9c14b01c95309969bec68aa0bc081")
}

/// `NP_OMAC_KEY_2` — XORed with a candidate window to form the CMAC key in the
/// `validate_dev_klic` klicensee oracle.
pub fn np_omac_key_2() -> [u8; 16] {
    hex16("6ba52976efda16ef3c339fb2971e256b")
}

/// `NP_KLIC_KEY` — ECB-decrypts a klicensee into the NPDRM metadata key.
pub fn np_klic_key() -> [u8; 16] {
    hex16("f2fbca7a75b04edc1390638ccdfdd1ee")
}

/// `NP_KLIC_FREE` — the "free" klicensee (used for the NPDRM SELF pre-decrypt).
pub fn np_klic_free() -> [u8; 16] {
    hex16("72f990788f9cff745725f08e4c128387")
}

/// The title-specific klicensee for MERCS2 WIF DLC01, recovered 2026-08-01.
///
/// Confirmed two ways: the `dev_hash` CMAC oracle, and block 0 of `DLC01.EDAT`
/// decrypting to a big-endian `SCFF` (FFCS) WAD.
pub fn dlc01_klicensee() -> [u8; 16] {
    hex16("1896170d86be49b983b7135c96d6fb79")
}

/// appldr keyset for a disc **APP** SELF (`self_type = 4`), key_revision 0x0001:
/// `(erk[32], riv[16])`, an AES-256-CBC pair.
pub fn appldr_app_keyset() -> ([u8; 32], [u8; 16]) {
    (
        hex32("79481839c406a632bdb4ac093d73d99ae1587f24ce7e69192c1cd0010274a8ab"),
        hex16("6f0f25e1c8c4b7ae70df968b04521dda"),
    )
}

/// appldr keyset for an **NPDRM** SELF (`self_type = 8`), key_revision 0x0001
/// (the v1.03 patch EBOOT): `(erk[32], riv[16])`, AES-256-CBC.
pub fn appldr_npdrm_keyset() -> ([u8; 32], [u8; 16]) {
    (
        hex32("f9edd0301f770fabba8863d9897f0fea6551b09431f61312654e28f43533ea6b"),
        hex16("a551ccb4a42c37a734a2b4f9657d5540"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_widths_and_known_bytes() {
        assert_eq!(ps3_pkg_key()[0], 0x2e);
        assert_eq!(dlc01_klicensee(), super::hex16("1896170d86be49b983b7135c96d6fb79"));
        let (erk, riv) = appldr_npdrm_keyset();
        assert_eq!(erk.len(), 32);
        assert_eq!(riv.len(), 16);
        assert_eq!(erk[0], 0xf9);
    }
}
