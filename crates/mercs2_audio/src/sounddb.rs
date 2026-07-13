//! The sound database (`sounddb`) — the cue → (wavebank, wave-index) routing catalog.
//!
//! **Oracle:** `PalGlobalTable::sounddb parser` **`FUN_00835b80`** (audio_code_map.md §3.5, §0). The
//! parser tests `*param_1 == '\x1d'` — the `'\x1d'` node tag cross-verified on two builds
//! (`PalEngine.cpp` `FUN_828ce9b8`). The chain: `Sound.AddPgAsset(pkg,"sounddb")` → `FUN_006025d0`
//! (type `0xE5273C14`) → `FUN_00607c50` → this parser (into `PalGlobalTable DAT_011763fc`), and
//! `Sound.LoadSoundBank` re-requests the per-bank sounddb block.
//!
//! ## Layout — CALIBRATED against shipped per-bank blocks (`veh_support`, 112 B; `Mercs2Globals`, 188 B)
//! The earlier 16-byte-record scheme in this file was a `// CONFIRM-LIVE:` guess and read **0 cues**
//! from every real block. The real per-bank layout, reversed from `veh_support` (7 cues, indices
//! 0..6), is a **28-byte header + 12-byte entries**:
//!
//! ```text
//! +0x00  u32  version tag (0x1D)
//! +0x04  u32  self/package hash (the wavebank this bank's cues play from)
//! +0x08  u32  cue count
//! +0x0C  u32  (0 — reserved / second count)
//! +0x10  u32  entries offset (0x1C)
//! +0x14  u32  table/size marker
//! +0x18  u32  total body size
//! +0x1C  entries[count], 12 bytes each:
//!          +0x00  u32  cue GUID   (m2 name-hash — what `Sound.CueSound("name")` hashes to)
//!          +0x04  u32  bank hash  (the `wavebank` this cue's wave lives in)
//!          +0x08  u32  wave index (index of the clip within that bank)
//! ```
//!
//! There is **no** priority / category / gain / distance in the record — the exe reads those from the
//! wave descriptor at play time. [`CueEntry`] keeps those as fields with faithful defaults so the
//! mixer/spatial path is unchanged; only the three routing fields are read from disk.

/// The `'\x1d'` node/version tag — cross-verified on PC (`FUN_00835b80`) and Xbox (`PalEngine.cpp`).
pub const SOUNDDB_TAG: u8 = 0x1D;

/// `FindCue` direct-index threshold (`FUN_00835a70`): ids below this are a direct cue-map index,
/// ids at/above it are hashed GUIDs matched against the entry table.
pub const FINDCUE_DIRECT_MAX: u32 = 0x401;

/// m2 name-hash of the `sounddb` asset type (audio_code_map.md §7).
pub const ASSET_TYPE_SOUNDDB: u32 = 0xE527_3C14;

/// A cue-routing record. The three disk fields (`guid`, `bank_hash`, `wave_index`) route a cue name to
/// its wave; the rest are play-time parameters the exe pulls from the wave descriptor — defaulted here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueEntry {
    /// Cue name-hash (`Sound.CueSound("name")` hashes the name to this).
    pub guid: u32,
    /// The `wavebank` (hash `0xF753F6D0`) this cue's samples live in.
    pub bank_hash: u32,
    /// Index of the wave within that bank (indexes the bank's clip list).
    pub wave_index: u32,
    /// Steal priority 0..255 — higher wins voice contention. Not in the record; defaults to 128.
    pub priority: u8,
    /// Category id (sfx/vo/music/…). Not in the record; defaults to 0 (sfx).
    pub category: u8,
    /// Bit flags: bit0 = looping, bit1 = 3D-positional, bit2 = streamed. Defaults to 0.
    pub flags: u16,
    /// Default linear gain (0..1) before category/attenuation. Defaults to 1.0.
    pub default_gain: f32,
    /// 3D attenuation: full volume within `min_dist` (from the wave descriptor; 0 = default).
    pub min_dist: f32,
    /// 3D attenuation: silent beyond `max_dist` (from the wave descriptor; 0 = default).
    pub max_dist: f32,
}

impl CueEntry {
    /// A routing entry with faithful play-time defaults (the shape [`SoundDb::parse`] produces).
    pub fn routed(guid: u32, bank_hash: u32, wave_index: u32) -> CueEntry {
        CueEntry {
            guid,
            bank_hash,
            wave_index,
            priority: 128,
            category: 0,
            flags: 0,
            default_gain: 1.0,
            min_dist: 0.0,
            max_dist: 0.0,
        }
    }
    /// bit0 — the cue loops until explicitly stopped.
    pub fn is_looping(&self) -> bool {
        self.flags & 0x1 != 0
    }
    /// bit1 — 3D-positional (attenuated/panned against the closest listener).
    pub fn is_positional(&self) -> bool {
        self.flags & 0x2 != 0
    }
    /// bit2 — streamed from a `.pws` stream file rather than a resident wave bank.
    pub fn is_streamed(&self) -> bool {
        self.flags & 0x4 != 0
    }
}

/// Parse failures for [`SoundDb::parse`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoundDbError {
    /// Buffer too short to even hold the fixed header.
    Truncated,
    /// First byte was not the `'\x1d'` version tag — not a sounddb block (or wrong endian/build).
    BadTag(u8),
    /// The declared entry table runs past the end of the buffer.
    TableOutOfBounds,
}

impl std::fmt::Display for SoundDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SoundDbError::Truncated => write!(f, "sounddb: buffer too short for header"),
            SoundDbError::BadTag(b) => write!(f, "sounddb: bad version tag 0x{b:02x} (expected 0x1D)"),
            SoundDbError::TableOutOfBounds => write!(f, "sounddb: declared table exceeds buffer"),
        }
    }
}

impl std::error::Error for SoundDbError {}

// Header field offsets (calibrated against shipped blocks).
const OFF_TAG: usize = 0x00;
const OFF_SELF: usize = 0x04;
const OFF_COUNT: usize = 0x08;
const OFF_ENTRIES_OFF: usize = 0x10;
/// Nominal header size / default entries offset.
pub const HEADER_SIZE: usize = 0x1C;
/// Per-cue record stride.
pub const CUE_STRIDE: usize = 12;

/// The parsed sound database: the bank's self hash + its cue-routing entries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundDb {
    /// Format/node version (`0x1D` in every shipped build).
    pub version: u8,
    /// The bank/package hash this block belongs to (`+0x04`).
    pub self_hash: u32,
    /// The cue-routing entries, in record order (id `< 0x401` indexes this directly).
    pub cues: Vec<CueEntry>,
}

impl SoundDb {
    /// Parse a `sounddb` block (`FUN_00835b80`). Rejects anything whose first byte is not `0x1D`.
    pub fn parse(bytes: &[u8]) -> Result<SoundDb, SoundDbError> {
        if bytes.len() < HEADER_SIZE {
            return Err(SoundDbError::Truncated);
        }
        let tag = bytes[OFF_TAG];
        if tag != SOUNDDB_TAG {
            return Err(SoundDbError::BadTag(tag));
        }
        let self_hash = rd_u32(bytes, OFF_SELF);
        let count = rd_u32(bytes, OFF_COUNT) as usize;
        let entries_off = rd_u32(bytes, OFF_ENTRIES_OFF) as usize;

        let end = entries_off
            .checked_add(count.checked_mul(CUE_STRIDE).ok_or(SoundDbError::TableOutOfBounds)?)
            .ok_or(SoundDbError::TableOutOfBounds)?;
        if entries_off < HEADER_SIZE || end > bytes.len() {
            return Err(SoundDbError::TableOutOfBounds);
        }

        let mut cues = Vec::with_capacity(count);
        for i in 0..count {
            let o = entries_off + i * CUE_STRIDE;
            cues.push(CueEntry::routed(
                rd_u32(bytes, o),
                rd_u32(bytes, o + 4),
                rd_u32(bytes, o + 8),
            ));
        }

        Ok(SoundDb { version: tag, self_hash, cues })
    }

    /// Serialize back to the on-disk layout — the exact inverse of [`parse`](Self::parse) for the three
    /// routing fields (the defaulted play-time fields are not on disk). Used by the round-trip test.
    pub fn to_bytes(&self) -> Vec<u8> {
        let entries_off = HEADER_SIZE;
        let end = entries_off + self.cues.len() * CUE_STRIDE;
        let mut b = vec![0u8; end];
        b[OFF_TAG] = self.version;
        wr_u32(&mut b, OFF_SELF, self.self_hash);
        wr_u32(&mut b, OFF_COUNT, self.cues.len() as u32);
        wr_u32(&mut b, OFF_ENTRIES_OFF, entries_off as u32);
        wr_u32(&mut b, 0x14, end as u32);
        wr_u32(&mut b, 0x18, end as u32);
        for (i, c) in self.cues.iter().enumerate() {
            let o = entries_off + i * CUE_STRIDE;
            wr_u32(&mut b, o, c.guid);
            wr_u32(&mut b, o + 4, c.bank_hash);
            wr_u32(&mut b, o + 8, c.wave_index);
        }
        b
    }

    /// `PalGlobalTable::FindCue` (`FUN_00835a70`): resolve a cue id to its [`CueEntry`].
    ///
    /// * `id < 0x401` → direct index into the entry table (the exe's fast path).
    /// * `id >= 0x401` → treat `id` as a hashed GUID; match it against the entry table.
    pub fn find_cue(&self, id: u32) -> Option<&CueEntry> {
        if id < FINDCUE_DIRECT_MAX {
            return self.cues.get(id as usize);
        }
        self.cues.iter().find(|c| c.guid == id)
    }

    /// Convenience: resolve a cue by its *name* (hashes with the m2 name-hash then [`find_cue`]).
    pub fn find_cue_by_name(&self, name: &str) -> Option<&CueEntry> {
        self.find_cue(mercs2_formats::hash::pandemic_hash_m2(name))
    }

    /// Build a database from routing entries (tests / synthesized blocks).
    pub fn from_cues(version: u8, cues: Vec<CueEntry>) -> SoundDb {
        let self_hash = cues.first().map(|c| c.bank_hash).unwrap_or(0);
        SoundDb { version, self_hash, cues }
    }

    /// Merge another bank's cue entries into this catalog (the game assembles one catalog from every
    /// resident bank's per-bank sounddb). Later duplicates keep the first mapping, as the exe does.
    pub fn merge(&mut self, other: &SoundDb) {
        for c in &other.cues {
            if !self.cues.iter().any(|e| e.guid == c.guid) {
                self.cues.push(*c);
            }
        }
    }
}

// --- little-endian readers (sounddb is little-endian on PC) ---
fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn wr_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact `veh_support` header + first two entries (from the shipped WAD) must parse to the real
    /// routing: 7 cues, bank hash 0x84701C9A, wave indices in 0..=6.
    #[test]
    fn parses_real_veh_support_header() {
        // 28-byte header + 7×12-byte entries (only the first two entries' bytes are needed to prove the
        // stride/field layout; the rest are zero-padded to the declared size).
        let mut b = vec![0u8; HEADER_SIZE + 7 * CUE_STRIDE];
        b[0] = 0x1D;
        wr_u32(&mut b, OFF_SELF, 0x8470_1C9A);
        wr_u32(&mut b, OFF_COUNT, 7);
        wr_u32(&mut b, OFF_ENTRIES_OFF, 0x1C);
        // entry 0: guid 0x1B2C8599, bank 0x84701C9A, index 5
        wr_u32(&mut b, 0x1C, 0x1B2C_8599);
        wr_u32(&mut b, 0x20, 0x8470_1C9A);
        wr_u32(&mut b, 0x24, 5);
        // entry 1: guid 0x229D0B74, bank 0x84701C9A, index 0
        wr_u32(&mut b, 0x28, 0x229D_0B74);
        wr_u32(&mut b, 0x2C, 0x8470_1C9A);
        wr_u32(&mut b, 0x30, 0);

        let db = SoundDb::parse(&b).expect("real header parses");
        assert_eq!(db.self_hash, 0x8470_1C9A);
        assert_eq!(db.cues.len(), 7);
        let c0 = db.find_cue(0x1B2C_8599).expect("cue by hash");
        assert_eq!(c0.bank_hash, 0x8470_1C9A);
        assert_eq!(c0.wave_index, 5);
        assert_eq!(db.find_cue(0x229D_0B74).unwrap().wave_index, 0);
        // direct-index path (id < 0x401)
        assert_eq!(db.find_cue(0).unwrap().guid, 0x1B2C_8599);
    }

    #[test]
    fn roundtrips_routing_fields() {
        let db = SoundDb::from_cues(
            SOUNDDB_TAG,
            vec![
                CueEntry::routed(0x1111_2222, 0xAABB_CCDD, 3),
                CueEntry::routed(0x3333_4444, 0xAABB_CCDD, 1),
            ],
        );
        let bytes = db.to_bytes();
        assert_eq!(bytes[0], SOUNDDB_TAG);
        let parsed = SoundDb::parse(&bytes).expect("round-trips");
        assert_eq!(parsed, db);

        let bad = {
            let mut x = bytes.clone();
            x[0] = 0x1C;
            x
        };
        assert!(matches!(SoundDb::parse(&bad), Err(SoundDbError::BadTag(0x1C))));
    }

    #[test]
    fn merge_keeps_first_mapping() {
        // Realistic (hashed) guids ≥ 0x401 so find_cue takes the GUID-match path, not direct-index.
        let (g1, g2) = (0x1B2C_8599u32, 0x229D_0B74u32);
        let mut a = SoundDb::from_cues(SOUNDDB_TAG, vec![CueEntry::routed(g1, 0xA, 0)]);
        let b = SoundDb::from_cues(
            SOUNDDB_TAG,
            vec![CueEntry::routed(g1, 0xB, 9), CueEntry::routed(g2, 0xB, 1)],
        );
        a.merge(&b);
        assert_eq!(a.cues.len(), 2);
        assert_eq!(a.find_cue(g1).unwrap().bank_hash, 0xA, "first mapping wins");
        assert_eq!(a.find_cue(g2).unwrap().bank_hash, 0xB);
    }
}
