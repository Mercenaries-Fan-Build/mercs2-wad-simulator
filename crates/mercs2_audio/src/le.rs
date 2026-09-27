//! Bounds-checked little-endian field access shared by the three bank codecs
//! ([`crate::wave`], [`crate::soundbank`], [`crate::sounddb`]).
//!
//! Every read returns `None` past the end of the buffer instead of panicking, so each codec can turn a
//! short read into its own `Truncated` error naming the field it was reading. Every table these
//! codecs handle is little-endian on the PC build.

/// Read a `u8` at `off`.
pub(crate) fn u8_at(b: &[u8], off: usize) -> Option<u8> {
    b.get(off).copied()
}

/// Read a little-endian `u16` at `off`.
pub(crate) fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

/// Read a little-endian `u32` at `off`.
pub(crate) fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Read a little-endian `f32` at `off`, bit-exact (a NaN payload survives a read/write round trip).
pub(crate) fn f32_at(b: &[u8], off: usize) -> Option<f32> {
    u32_at(b, off).map(f32::from_bits)
}

/// Append a little-endian `u16`.
pub(crate) fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Append a little-endian `u32`.
pub(crate) fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Append a little-endian `f32`, bit-exact.
pub(crate) fn put_f32(out: &mut Vec<u8>, v: f32) {
    put_u32(out, v.to_bits());
}

/// Round `n` up to the next multiple of 16.
pub(crate) fn align16(n: usize) -> usize {
    (n + 15) & !15
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_little_endian_and_bounds_checked() {
        let b = [0x78, 0x56, 0x34, 0x12, 0xFF];
        assert_eq!(u32_at(&b, 0), Some(0x1234_5678));
        assert_eq!(u16_at(&b, 3), Some(0xFF12));
        assert_eq!(u8_at(&b, 4), Some(0xFF));
        assert_eq!(u32_at(&b, 2), None, "a read running past the end is None, not a panic");
        assert_eq!(u16_at(&b, usize::MAX), None, "offset overflow is None");
    }

    #[test]
    fn f32_round_trip_is_bit_exact() {
        let nan = f32::from_bits(0x7FC0_1234);
        let mut out = Vec::new();
        put_f32(&mut out, nan);
        assert_eq!(f32_at(&out, 0).map(f32::to_bits), Some(0x7FC0_1234));
    }

    #[test]
    fn align16_rounds_up() {
        assert_eq!(align16(0), 0);
        assert_eq!(align16(1), 16);
        assert_eq!(align16(16), 16);
        assert_eq!(align16(17), 32);
    }
}
