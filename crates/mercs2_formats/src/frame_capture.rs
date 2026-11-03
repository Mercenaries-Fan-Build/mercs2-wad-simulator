//! A finished frame's render stores as the profiler ASI copies them (`mercs2_profiler_frame_<frame>.bin`): the
//! packets, the display list, the constant sidecar and the world-matrix table the frame filled, each with the
//! address it was read from, its fill and its size (`crate::profiler_sites::FrameStores`).
//!
//! ```text
//! "M2FC", u32 version (1), u32 frame, u32 parity,
//! 4 × store: u32 used, u32 size, u32 address, u32 byte count, then that many bytes
//! ```
//!
//! The stores are in [`Store`] order. `used` and `size` are in the store's own unit (packets, bytes, bytes,
//! matrices); the bytes are `min(used, size)` units of it, so a store its frame overran keeps its size's worth
//! and says how far past it went. Little-endian throughout.

/// The file's magic.
pub const MAGIC: [u8; 4] = *b"M2FC";
pub const VERSION: u32 = 1;
/// A world matrix: 4 × 4 `f32`.
pub const MATRIX_BYTES: usize = 0x40;

/// The four stores, in file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Packets,
    List,
    Sidecar,
    Matrices,
}

impl Store {
    pub const ALL: [Store; 4] = [Store::Packets, Store::List, Store::Sidecar, Store::Matrices];
}

/// One store's copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The frame's fill and the store's size, in the store's unit.
    pub used: u32,
    pub size: u32,
    /// Where the bytes were read from in the game.
    pub address: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCapture {
    pub frame: u32,
    pub parity: u32,
    /// In [`Store::ALL`] order.
    pub sections: [Section; 4],
}

impl FrameCapture {
    pub fn section(&self, s: Store) -> &Section {
        &self.sections[s as usize]
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.sections.iter().map(|s| 16 + s.bytes.len()).sum::<usize>());
        out.extend_from_slice(&MAGIC);
        for w in [VERSION, self.frame, self.parity] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for s in &self.sections {
            for w in [s.used, s.size, s.address, s.bytes.len() as u32] {
                out.extend_from_slice(&w.to_le_bytes());
            }
            out.extend_from_slice(&s.bytes);
        }
        out
    }

    pub fn decode(b: &[u8]) -> Result<FrameCapture, String> {
        let word = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| format!("frame capture: short at +{o:#x}"));
        if b.get(..4) != Some(&MAGIC[..]) {
            return Err("frame capture: no M2FC magic".into());
        }
        if word(4)? != VERSION {
            return Err(format!("frame capture: version {}, this reader knows {VERSION}", word(4)?));
        }
        let (frame, parity) = (word(8)?, word(12)?);
        let mut p = 16;
        let mut sections = Vec::with_capacity(4);
        for store in Store::ALL {
            let (used, size, address, n) = (word(p)?, word(p + 4)?, word(p + 8)?, word(p + 12)? as usize);
            let bytes = b.get(p + 16..p + 16 + n).ok_or_else(|| format!("frame capture: {store:?} holds {n} bytes past the file's end"))?.to_vec();
            p += 16 + n;
            sections.push(Section { used, size, address, bytes });
        }
        if p != b.len() {
            return Err(format!("frame capture: {} bytes after the last store", b.len() - p));
        }
        let sections: [Section; 4] = sections.try_into().map_err(|_| "frame capture: not four stores".to_string())?;
        Ok(FrameCapture { frame, parity, sections })
    }

    /// The world matrices, each as 16 `f32` in memory order.
    pub fn matrices(&self) -> Vec<[f32; 16]> {
        self.section(Store::Matrices)
            .bytes
            .chunks_exact(MATRIX_BYTES)
            .map(|m| std::array::from_fn(|k| f32::from_le_bytes([m[4 * k], m[4 * k + 1], m[4 * k + 2], m[4 * k + 3]])))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FrameCapture {
        let s = |used: u32, size: u32, address: u32, bytes: Vec<u8>| Section { used, size, address, bytes };
        let mut m = Vec::new();
        for k in 0..16u32 {
            m.extend_from_slice(&(k as f32).to_le_bytes());
        }
        FrameCapture {
            frame: 1234,
            parity: 1,
            sections: [s(2, 0x2000, 0x1000, vec![7; 2 * 0x58]), s(8, 0x50000, 0x2000, vec![1; 8]), s(4, 0x20000, 0x3000, vec![2; 4]), s(1, 0x1400, 0x4000, m)],
        }
    }

    #[test]
    fn a_capture_round_trips_and_its_matrices_read_back() {
        let c = sample();
        let d = FrameCapture::decode(&c.encode()).unwrap();
        assert_eq!(d, c);
        assert_eq!(d.matrices(), vec![std::array::from_fn(|k| k as f32)]);
    }

    #[test]
    fn a_damaged_capture_is_refused() {
        let b = sample().encode();
        assert!(FrameCapture::decode(&b[..b.len() - 1]).unwrap_err().contains("past the file's end"));
        let mut long = b.clone();
        long.push(0);
        assert!(FrameCapture::decode(&long).unwrap_err().contains("after the last store"));
        let mut other = b.clone();
        other[4] = 2;
        assert!(FrameCapture::decode(&other).unwrap_err().contains("version 2"));
        assert!(FrameCapture::decode(b"nope").unwrap_err().contains("magic"));
    }
}
