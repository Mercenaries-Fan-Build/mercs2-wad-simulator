//! `BinkCopyToBuffer` into an `A8R8G8B8` surface, as the game asks for it (`SPEC-video.md` §7.2): the
//! MMX converter a current CPU is given, for surface type 5.

use crate::video::Frame;

/// `pmulhw`: the high half of a signed 16 × 16 product.
fn mulhi(a: i32, b: i32) -> i32 {
    (a * b) >> 16
}

/// `paddsw` with `k`, `psubusw 0x7000`, `psrlw 4`.
fn channel(a: i32, k: i32) -> u8 {
    (((a + k).min(0x7fff) - 0x7000).max(0) >> 4) as u8
}

/// One pixel: R from V, G from both, B from U.
pub fn pixel(y: u8, u: u8, v: u8) -> [u8; 3] {
    let u = mulhi(u as i32 * 16, 0x6810);
    let v = mulhi(v as i32 * 32, 0x6810);
    let y = mulhi(y as i32 * 64, 0x4a89);
    [channel(2 * v + y, 0x61d0), channel(y - (u + v), 0x78a7), channel(5 * u + y, 0x5e8d)]
}

/// The frame as RGBA bytes, `width * height * 4`, each 2×2 block of pixels sharing its chroma sample;
/// alpha is opaque, the game's surface having no alpha plane to copy.
pub fn to_rgba(frame: &Frame) -> Vec<u8> {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let cw = w.div_ceil(2);
    let mut out = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        for col in 0..w {
            let c = (row / 2) * cw + col / 2;
            let [r, g, b] = pixel(frame.y[row * w + col], frame.u[c], frame.v[c]);
            out.extend_from_slice(&[r, g, b, 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Worked by hand from §7.2: studio black is y = 298, u = 832, v = 1665, so G = (298 − 2497 +
    /// 0x78A7 − 0x7000) >> 4 = 1; studio white is y = 4378, giving R = 4076 >> 4 and B = 4071 >> 4.
    /// The converter is a fixed-point approximation, not an exact studio-to-full-range map.
    #[test]
    fn studio_black_and_white_land_where_the_mmx_arithmetic_puts_them() {
        assert_eq!(pixel(16, 128, 128), [0, 1, 0]);
        assert_eq!(pixel(235, 128, 128), [254, 255, 254]);
    }

    #[test]
    fn red_difference_drives_red_and_blue_difference_drives_blue() {
        let [r, _, b] = pixel(128, 128, 240);
        assert!(r > 200 && b < 140, "{r} {b}");
        let [r, _, b] = pixel(128, 240, 128);
        assert!(b > 200 && r < 140, "{r} {b}");
    }
}
