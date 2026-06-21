//! Mercenaries 2 CSUM: CRC-32 init=0, no final XOR (poly 0xEDB88320 reflected).
//!
//! Python equivalent in tools/ucfx_be_to_le.py:
//!   (zlib.crc32(data, 0xFFFFFFFF) ^ 0xFFFFFFFF) & 0xFFFFFFFF
//! which produces the same init=0 / no-final-XOR result (see that file's docstring).

/// Compute Mercenaries 2 CRC-32 checksum.
///
/// Uses init=0, no final XOR, with reflected polynomial 0xEDB88320.
/// This is the canonical checksum for CSUM chunks in FFCS WADs.
///
/// # Example
///
/// ```
/// use mercs2_formats::crc32::crc32_mercs2;
///
/// let data = b"hello";
/// let sum = crc32_mercs2(data);
/// assert_eq!(sum, 0xF032519B);
/// ```
pub fn crc32_mercs2(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB88320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_data() {
        assert_eq!(crc32_mercs2(b""), 0);
    }

    #[test]
    fn single_byte() {
        assert_eq!(crc32_mercs2(b"a"), 0x3AB551CE);
    }

    #[test]
    fn known_vector() {
        assert_eq!(crc32_mercs2(b"hello"), 0xF032519B);
    }

    #[test]
    fn longer_input() {
        let data = b"The quick brown fox jumps over the lazy dog";
        let expected = 0xB9C60808;
        assert_eq!(crc32_mercs2(data), expected);
    }

    #[test]
    fn different_lengths() {
        let s = b"test";
        let crc1 = crc32_mercs2(&s[0..1]);
        let crc2 = crc32_mercs2(&s[0..2]);
        let crc3 = crc32_mercs2(&s[0..3]);
        let crc4 = crc32_mercs2(s);
        // All should be different (collision probability is very low)
        assert_ne!(crc1, crc2);
        assert_ne!(crc2, crc3);
        assert_ne!(crc3, crc4);
    }

    #[test]
    fn zeroed_buffer() {
        let data = vec![0u8; 256];
        let crc = crc32_mercs2(&data);
        assert_eq!(crc, 0);
    }

    #[test]
    fn all_ones_buffer() {
        let data = vec![0xFFu8; 256];
        let crc = crc32_mercs2(&data);
        assert_eq!(crc, 0xF33E2D79);
    }
}
