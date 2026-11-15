//! The game's display-mode list and the BUG-016 choice of a mode when `Mercs2.ini`'s mode is not in it
//! (`docs/fixpack/bug_register.md`). The list is built by the render state's constructor from
//! `IDirect3D9::EnumAdapterModes` (`D3DFMT_X8R8G8B8` only) and searched by exact equality on width, height,
//! bits per pixel and refresh rate ([`crate::display_mode_sites`]). Retail falls back to 640×480 at 60 Hz
//! when the ini mode is missing and returns an error the caller never checks when that is missing too.
//! The fix picks, from the list the game built:
//!
//! 1. the ini resolution at its refresh rate closest to 60 Hz, else
//! 2. the desktop resolution at its refresh rate closest to 60 Hz,
//!
//! and otherwise reports that nothing was picked so the caller can fail with a message naming both.
//! Between two rates equally far from 60 Hz the lower one wins.

/// One 8-byte record of the game's mode list, as the constructor writes it: `[0]` the
/// `EnumAdapterModes` index (truncated to a byte), `[1]` the refresh rate, `[2..4]` the height, `[4..6]`
/// the width, `[6]` the bits per pixel, `[7]` the widescreen flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeRecord {
    pub index: u8,
    pub refresh: u8,
    pub height: u16,
    pub width: u16,
    pub bpp: u8,
    pub wide: u8,
}

/// The size of one record.
pub const RECORD_BYTES: usize = 8;
/// The refresh rate the fallback aims for.
pub const TARGET_REFRESH: u32 = 60;

impl ModeRecord {
    pub fn from_bytes(b: [u8; RECORD_BYTES]) -> Self {
        ModeRecord {
            index: b[0],
            refresh: b[1],
            height: u16::from_le_bytes([b[2], b[3]]),
            width: u16::from_le_bytes([b[4], b[5]]),
            bpp: b[6],
            wide: b[7],
        }
    }

    pub fn to_bytes(self) -> [u8; RECORD_BYTES] {
        let [h0, h1] = self.height.to_le_bytes();
        let [w0, w1] = self.width.to_le_bytes();
        [self.index, self.refresh, h0, h1, w0, w1, self.bpp, self.wide]
    }
}

/// The records of a list of `count` records at `bytes`; `None` when `bytes` is shorter than that.
pub fn records(bytes: &[u8], count: usize) -> Option<Vec<ModeRecord>> {
    let need = count.checked_mul(RECORD_BYTES)?;
    let b = bytes.get(..need)?;
    Some(b.chunks_exact(RECORD_BYTES).map(|c| ModeRecord::from_bytes(c.try_into().expect("8-byte chunk"))).collect())
}

/// The position in `list` of the `width`×`height` record at 32 bits per pixel whose refresh rate is closest
/// to [`TARGET_REFRESH`] (the lower of two equally close), or `None` when the list has no such resolution.
pub fn closest_to_60(list: &[ModeRecord], width: u32, height: u32) -> Option<usize> {
    list.iter()
        .enumerate()
        .filter(|(_, r)| u32::from(r.width) == width && u32::from(r.height) == height && r.bpp == 32)
        .min_by_key(|(_, r)| (u32::from(r.refresh).abs_diff(TARGET_REFRESH), r.refresh))
        .map(|(i, _)| i)
}

/// Which rule chose the mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// The ini resolution was listed.
    Requested(usize),
    /// The ini resolution was not listed and the desktop resolution was.
    Desktop(usize),
}

impl Pick {
    pub fn position(self) -> usize {
        match self {
            Pick::Requested(i) | Pick::Desktop(i) => i,
        }
    }
}

/// The mode the fix uses in place of retail's 640×480 at 60 Hz: the `requested` (ini) resolution, else the
/// `desktop` resolution, each at its refresh rate closest to 60 Hz. `None` when the list has neither.
pub fn pick(list: &[ModeRecord], requested: (u32, u32), desktop: Option<(u32, u32)>) -> Option<Pick> {
    if let Some(i) = closest_to_60(list, requested.0, requested.1) {
        return Some(Pick::Requested(i));
    }
    let (w, h) = desktop?;
    closest_to_60(list, w, h).map(Pick::Desktop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(width: u16, height: u16, refresh: u8) -> ModeRecord {
        ModeRecord { index: 0, refresh, height, width, bpp: 32, wide: 0 }
    }

    #[test]
    fn a_record_reads_and_writes_the_constructors_byte_layout() {
        // 2560x1440 at 75 Hz, enumeration index 0x2a, widescreen.
        let bytes = [0x2a, 75, 0xa0, 0x05, 0x00, 0x0a, 32, 1];
        let r = ModeRecord::from_bytes(bytes);
        assert_eq!(r, ModeRecord { index: 0x2a, refresh: 75, height: 1440, width: 2560, bpp: 32, wide: 1 });
        assert_eq!(r.to_bytes(), bytes);
    }

    #[test]
    fn records_reads_count_records_and_refuses_a_short_buffer() {
        let mut b = mode(1920, 1080, 60).to_bytes().to_vec();
        b.extend(mode(1920, 1080, 144).to_bytes());
        assert_eq!(records(&b, 2), Some(vec![mode(1920, 1080, 60), mode(1920, 1080, 144)]));
        assert_eq!(records(&b, 0), Some(vec![]));
        assert_eq!(records(&b, 3), None);
    }

    #[test]
    fn the_ini_resolution_wins_at_the_rate_closest_to_60() {
        let list = [mode(2560, 1440, 144), mode(2560, 1440, 75), mode(1920, 1080, 60), mode(2560, 1440, 120)];
        assert_eq!(pick(&list, (2560, 1440), Some((1920, 1080))), Some(Pick::Requested(1)));
    }

    #[test]
    fn the_desktop_resolution_is_used_when_the_ini_resolution_is_not_listed() {
        let list = [mode(2560, 1600, 240), mode(2560, 1600, 75), mode(1280, 800, 165)];
        assert_eq!(pick(&list, (1600, 900), Some((2560, 1600))), Some(Pick::Desktop(1)));
    }

    #[test]
    fn of_two_rates_equally_far_from_60_the_lower_wins() {
        let list = [mode(1920, 1080, 61), mode(1920, 1080, 59)];
        assert_eq!(closest_to_60(&list, 1920, 1080), Some(1));
        let list = [mode(1920, 1080, 59), mode(1920, 1080, 61)];
        assert_eq!(closest_to_60(&list, 1920, 1080), Some(0));
    }

    #[test]
    fn only_32_bit_records_are_candidates() {
        let list = [ModeRecord { bpp: 16, ..mode(1920, 1080, 60) }, mode(1920, 1080, 144)];
        assert_eq!(closest_to_60(&list, 1920, 1080), Some(1));
    }

    #[test]
    fn nothing_is_picked_from_an_empty_list_or_one_without_either_resolution() {
        assert_eq!(pick(&[], (1920, 1080), Some((1920, 1080))), None);
        let list = [mode(1280, 720, 60)];
        assert_eq!(pick(&list, (1920, 1080), Some((2560, 1440))), None);
        assert_eq!(pick(&list, (1920, 1080), None), None);
    }
}
