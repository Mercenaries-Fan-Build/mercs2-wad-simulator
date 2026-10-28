//! Pandemic Studios FNV-1a hashing (port of `tools/pandemic_hash.py`).
//!
//! Two variants across engine generations:
//!   * `pandemic_hash` — Mercs 1: FNV-1a 32-bit with case suppression (each
//!     byte OR'd with 0x20). Verified: `pandemic_hash("registry") == 0x3884598E`.
//!   * `pandemic_hash_m2` — Mercs 2: the above plus a final `^0x2A` then `*prime`.
//!     Verified: `pandemic_hash_m2("texture") == 0xF011157A`,
//!     `pandemic_hash_m2("model") == 0x5B724250` (the ASET type constants).

const FNV1A_OFFSET_BASIS: u32 = 0x811C_9DC5;
const FNV1A_PRIME: u32 = 0x0100_0193;

/// FNV-1a with case suppression (Mercs 1 variant). Empty input hashes to 0.
pub fn pandemic_hash(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let mut h = FNV1A_OFFSET_BASIS;
    for &b in text.as_bytes() {
        h ^= (b | 0x20) as u32;
        h = h.wrapping_mul(FNV1A_PRIME);
    }
    h
}

/// Mercs 2 variant: `pandemic_hash` + finalization (`^0x2A`, `*prime`). Empty input hashes to 0.
pub fn pandemic_hash_m2(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let mut h = FNV1A_OFFSET_BASIS;
    for &b in text.as_bytes() {
        h ^= (b | 0x20) as u32;
        h = h.wrapping_mul(FNV1A_PRIME);
    }
    h ^= 0x2A;
    h.wrapping_mul(FNV1A_PRIME)
}

/// The multiplicative inverse of the FNV prime modulo 2^32: `FNV1A_PRIME × this ≡ 1`.
const FNV1A_PRIME_INVERSE: u32 = 0x359C_449B;

/// `pandemic_hash_m2(s + suffix)` from `pandemic_hash_m2(s)` alone, without the text of `s`.
///
/// The finalization round (`^0x2A`, `×prime`) is invertible — the prime is odd, so it has an
/// inverse modulo 2^32 — which recovers the FNV state after the last byte of `s`; the suffix's
/// bytes are then folded in (case-suppressed) and the finalization applied again. `hash` must be a
/// non-empty string's hash: the empty string's `0` is not a finalized state.
pub fn pandemic_hash_m2_extend(hash: u32, suffix: &str) -> u32 {
    let mut h = hash.wrapping_mul(FNV1A_PRIME_INVERSE) ^ 0x2A;
    for &b in suffix.as_bytes() {
        h ^= (b | 0x20) as u32;
        h = h.wrapping_mul(FNV1A_PRIME);
    }
    h ^= 0x2A;
    h.wrapping_mul(FNV1A_PRIME)
}

/// FUN_00824270 exactly, over any bytes: each byte is sign-extended (`movsx ecx, cl` @0x824280) before
/// the `| 0x20` case fold, then the Mercs 2 finalization. Equal to [`pandemic_hash_m2`] for ASCII
/// names; use this one for names that may hold bytes >= 0x80 or are not UTF-8 (C strings from a mod).
/// Empty input hashes to 0, as the exe returns for an empty or null name.
pub fn pandemic_hash_m2_bytes(name: &[u8]) -> u32 {
    if name.is_empty() {
        return 0;
    }
    let mut h = FNV1A_OFFSET_BASIS;
    for &b in name {
        h ^= (i32::from(b as i8) as u32) | 0x20;
        h = h.wrapping_mul(FNV1A_PRIME);
    }
    h ^= 0x2A;
    h.wrapping_mul(FNV1A_PRIME)
}

/// Whether a name HARVESTED from block bytes (`aset_block_strings.json`) has the shape of an
/// authored identifier: it starts with an ASCII letter and holds only `[A-Za-z0-9_]`.
///
/// A run of bytes that happens to hash to an asset is not evidence of its name. Over the 281 asset
/// names whose only source is that harvest, the 15 this refuses are all collision debris (`6xht`,
/// `-ufzc9`, `.j0x.`, `2v3tx`, `j-cfe` …); what it keeps includes the hijack clip names the
/// decompiled Lua spells (`all_md500_driverdoor_actionhijack_getin_loopf_fb`). Debris with the
/// right shape goes on the workshop crate's `data/rejected_names.json`. Only for harvested names:
/// authored names from other sources may hold spaces or hyphens (`Allied Infantry_Mirron01`).
pub fn is_identifier_shaped(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// FNV-1a over raw bytes, no case suppression. Empty input hashes to 0.
pub fn pandemic_hash_bytes(data: &[u8]) -> u32 {
    if data.is_empty() {
        return 0;
    }
    let mut h = FNV1A_OFFSET_BASIS;
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(FNV1A_PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        // Verified against MERCENAR.EXE call sites and the Python reference.
        assert_eq!(pandemic_hash("registry"), 0x3884_598E);
        assert_eq!(pandemic_hash_m2("texture"), 0xF011_157A);
        assert_eq!(pandemic_hash_m2("model"), 0x5B72_4250);
        assert_eq!(pandemic_hash(""), 0);
        assert_eq!(pandemic_hash_m2(""), 0);
    }

    #[test]
    fn extending_a_hash_equals_hashing_the_joined_text() {
        assert_eq!(FNV1A_PRIME.wrapping_mul(FNV1A_PRIME_INVERSE), 1);
        for (base, suffix) in [("vo_mattias", ".english"), ("vo_stream", ".polski"), ("a", ""), ("ui_hud", "_x")] {
            assert_eq!(
                pandemic_hash_m2_extend(pandemic_hash_m2(base), suffix),
                pandemic_hash_m2(&format!("{base}{suffix}")),
                "{base}{suffix}"
            );
        }
    }

    #[test]
    fn the_byte_variant_sign_extends_like_the_exe() {
        for name in ["texture", "model", "TestGlowFP_pl_sl", "a"] {
            assert_eq!(pandemic_hash_m2_bytes(name.as_bytes()), pandemic_hash_m2(name), "{name}");
        }
        assert_eq!(pandemic_hash_m2_bytes(b""), 0);
        assert_eq!(pandemic_hash_m2_bytes(b"\xE9t\xE9"), 0x1449_B483);
        assert_ne!(pandemic_hash_m2_bytes(b"\xE9t\xE9"), pandemic_hash_m2("\u{e9}t\u{e9}"));
    }

    #[test]
    fn harvested_debris_is_not_identifier_shaped() {
        for junk in ["6xht", "6-ucfx", "5twtt", "-ufzc9", ".j0x.", "2v3tx", "1uud", "j-cfe", ""] {
            assert!(!is_identifier_shaped(junk), "{junk}");
        }
        for real in ["all_md500_driverdoor_actionhijack_getin_loopf_fb", "telephone", "gr_veh_tank_m551sheridan", "Font_glyphs"] {
            assert!(is_identifier_shaped(real), "{real}");
        }
    }

    #[test]
    fn case_suppression_makes_upper_lower_equal() {
        assert_eq!(pandemic_hash_m2("TEXTURE"), pandemic_hash_m2("texture"));
        assert_eq!(pandemic_hash("Model"), pandemic_hash("model"));
    }
}
