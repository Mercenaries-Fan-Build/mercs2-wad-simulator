//! Mercenaries 2 CSUM: CRC-32 init=0, no final XOR (poly 0xEDB88320 reflected).
//!
//! Python equivalent in tools/ucfx_be_to_le.py:
//!   (zlib.crc32(data, 0xFFFFFFFF) ^ 0xFFFFFFFF) & 0xFFFFFFFF
//! which produces the same init=0 / no-final-XOR result (see that file's docstring).

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
