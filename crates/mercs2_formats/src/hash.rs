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
    fn case_suppression_makes_upper_lower_equal() {
        assert_eq!(pandemic_hash_m2("TEXTURE"), pandemic_hash_m2("texture"));
        assert_eq!(pandemic_hash("Model"), pandemic_hash("model"));
    }
}
