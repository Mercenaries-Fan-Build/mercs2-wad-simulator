//! The sound database (`sounddb`) — the cue-name → `(soundbank, cue index)` routing catalog, plus the
//! category tree the global catalog carries.
//!
//! **Oracle:** `PalGlobalTable::sounddb parser` **`FUN_00835b80`** (audio_code_map.md §3.5, §0). The
//! parser tests `*param_1 == '\x1d'` — the `'\x1d'` node tag cross-verified on two builds
//! (`PalEngine.cpp` `FUN_828ce9b8`) — and binary-searches its GUID table (`FUN_0083c570`). The chain:
//! `Sound.AddPgAsset(pkg,"sounddb")` → `FUN_006025d0` (type `0xE5273C14`) → `FUN_00607c50` → this
//! parser (into `PalGlobalTable DAT_011763fc`), and `Sound.LoadSoundBank` re-requests the per-bank
//! sounddb block.
//!
//! ## Layout (measured on every sounddb in retail `vz.wad`, `English.wad` and `shell.wad`)
//!
//! ```text
//! +0x00  u32  table version 0x1D
//! +0x04  u32  bank hash (m2 of the bank name; m2("mercs2globals") for the global catalog)
//! +0x08  u16  cue entry count
//! +0x0A  u16  category entry count
//! +0x0C  u32  parameter count
//! +0x10  u32  cue table offset (0x1C)
//! +0x14  u32  category table offset (= 0x1C + 12 × cue count)
//! +0x18  u32  parameter table offset (= category table + 8 × category count); the body ends at
//!             parameter table + 4 × parameter count
//! cue table       {u32 cue guid, u32 soundbank hash, u32 cue index}, strictly ascending by guid
//! category table  {u32 category hash, u32 parent category hash (0 at the root)}, ascending by hash
//! parameter table u32 parameter-name hashes
//! ```
//!
//! A **per-bank** sounddb carries cue entries only: one per cue of the soundbank of the same name,
//! each third field the index of that cue IN THE SOUNDBANK (`sounddb guid == soundbank cue[index]
//! guid` for all 76 per-bank tables in `vz.wad`). It is not a wave index: the wave is reached through
//! the cue's group ([`crate::soundbank`]). The **global** `Mercs2Globals` sounddb carries no cues, the
//! 19-entry category tree and 2 parameter hashes.
//!
//! There is no priority / category / gain / distance in a cue entry; [`CueEntry`] keeps those as
//! play-time fields with defaults so the mixer/spatial path is unchanged, and only the three routing
//! fields are read from disk.

use crate::le::{put_u16, put_u32, u16_at, u32_at};

/// The `'\x1d'` node/version tag — cross-verified on PC (`FUN_00835b80`) and Xbox (`PalEngine.cpp`).
pub const SOUNDDB_TAG: u8 = 0x1D;

/// `FindCue` direct-index threshold (`FUN_00835a70`): ids below this are a direct cue-map index,
/// ids at/above it are hashed GUIDs matched against the entry table.
pub const FINDCUE_DIRECT_MAX: u32 = 0x401;

/// m2 name-hash of the `sounddb` asset type (audio_code_map.md §7).
pub const ASSET_TYPE_SOUNDDB: u32 = 0xE527_3C14;

/// A cue-routing record. The three disk fields (`guid`, `bank_hash`, `cue_index`) route a cue name to
/// a soundbank cue; the rest are play-time parameters with defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueEntry {
    /// Cue name-hash (`Sound.CueSound("name")` hashes the name to this).
    pub guid: u32,
    /// The `soundbank` (hash `0x9F8BCA10`) holding the cue. Retail names each bank's soundbank,
    /// wavebank and sounddb alike, so this is also the bank name hash.
    pub bank_hash: u32,
    /// Index of the cue in that soundbank's cue table.
    pub cue_index: u32,
    /// Steal priority 0..255 — higher wins voice contention. Not in the record; defaults to 128.
    pub priority: u8,
    /// Category id (sfx/vo/music/…). Not in the record; defaults to 0 (sfx).
    pub category: u8,
    /// Bit flags: bit0 = looping, bit1 = 3D-positional, bit2 = streamed. Defaults to 0.
    pub flags: u16,
    /// Default linear gain (0..1) before category/attenuation. Defaults to 1.0.
    pub default_gain: f32,
    /// 3D attenuation: full volume within `min_dist` (0 = default).
    pub min_dist: f32,
    /// 3D attenuation: silent beyond `max_dist` (0 = default).
    pub max_dist: f32,
}

impl CueEntry {
    /// A routing entry with play-time defaults (the shape [`SoundDb::parse`] produces).
    pub fn routed(guid: u32, bank_hash: u32, cue_index: u32) -> CueEntry {
        CueEntry {
            guid,
            bank_hash,
            cue_index,
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

/// One node of the category tree the global catalog carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CategoryEntry {
    /// The category's hash (m2 of its name).
    pub category: u32,
    /// The parent category's hash; 0 at the root.
    pub parent: u32,
}

/// Parse and serialize failures. Each is a hard error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoundDbError {
    /// Buffer too short to even hold the fixed header.
    Truncated,
    /// First byte was not the `'\x1d'` version tag — not a sounddb block (or wrong endian/build).
    BadTag(u8),
    /// First byte was the tag but the whole `u32` version was not 0x1D.
    BadVersion(u32),
    /// The declared tables run past the end of the buffer.
    TableOutOfBounds,
    /// A table offset, or the body length, is not where the contiguous layout puts it.
    BadLayout { field: &'static str, found: u32, expected: u32 },
    /// The cue entries are not strictly ascending by guid (the engine binary-searches them).
    UnsortedCues { index: usize },
    /// The category entries are not strictly ascending by hash.
    UnsortedCategories { index: usize },
    /// More entries than a count field holds.
    TooMany { what: &'static str, count: usize },
}

impl std::fmt::Display for SoundDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SoundDbError::Truncated => write!(f, "sounddb: buffer too short for header"),
            SoundDbError::BadTag(b) => write!(f, "sounddb: bad version tag 0x{b:02x} (expected 0x1D)"),
            SoundDbError::BadVersion(v) => write!(f, "sounddb: version 0x{v:X}, expected 0x1D"),
            SoundDbError::TableOutOfBounds => write!(f, "sounddb: declared table exceeds buffer"),
            SoundDbError::BadLayout { field, found, expected } => write!(
                f,
                "sounddb: {field} = 0x{found:X}, the contiguous layout puts it at 0x{expected:X}"
            ),
            SoundDbError::UnsortedCues { index } => {
                write!(f, "sounddb: cue entry {index} is not above its predecessor's guid")
            }
            SoundDbError::UnsortedCategories { index } => {
                write!(f, "sounddb: category entry {index} is not above its predecessor's hash")
            }
            SoundDbError::TooMany { what, count } => {
                write!(f, "sounddb: {count} {what} exceed the count field")
            }
        }
    }
}

impl std::error::Error for SoundDbError {}

/// Header size; also the cue table offset.
pub const HEADER_SIZE: usize = 0x1C;
/// Per-cue record stride.
pub const CUE_STRIDE: usize = 12;
/// Per-category record stride.
pub const CATEGORY_STRIDE: usize = 8;

/// The parsed sound database.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundDb {
    /// Format/node version (`0x1D` in every shipped build).
    pub version: u8,
    /// The bank/package hash this block belongs to (`+0x04`).
    pub self_hash: u32,
    /// The cue-routing entries, in record order (id `< 0x401` indexes this directly).
    pub cues: Vec<CueEntry>,
    /// The category tree (global catalog only).
    pub categories: Vec<CategoryEntry>,
    /// The parameter-name hashes (global catalog only).
    pub params: Vec<u32>,
}

fn rd32(b: &[u8], off: usize) -> Result<u32, SoundDbError> {
    u32_at(b, off).ok_or(SoundDbError::TableOutOfBounds)
}

fn layout(field: &'static str, found: u32, expected: usize) -> Result<(), SoundDbError> {
    if found as usize != expected {
        return Err(SoundDbError::BadLayout { field, found, expected: expected as u32 });
    }
    Ok(())
}

impl SoundDb {
    /// Parse a `sounddb` block (`FUN_00835b80`). Refuses anything outside the measured layout.
    pub fn parse(bytes: &[u8]) -> Result<SoundDb, SoundDbError> {
        if bytes.len() < HEADER_SIZE {
            return Err(SoundDbError::Truncated);
        }
        let tag = bytes[0];
        if tag != SOUNDDB_TAG {
            return Err(SoundDbError::BadTag(tag));
        }
        let version = rd32(bytes, 0x00)?;
        if version != u32::from(SOUNDDB_TAG) {
            return Err(SoundDbError::BadVersion(version));
        }
        let self_hash = rd32(bytes, 0x04)?;
        let n_cues = u16_at(bytes, 0x08).ok_or(SoundDbError::Truncated)? as usize;
        let n_cats = u16_at(bytes, 0x0A).ok_or(SoundDbError::Truncated)? as usize;
        let n_params = rd32(bytes, 0x0C)? as usize;
        let cue_off = HEADER_SIZE;
        let cat_off = cue_off + n_cues * CUE_STRIDE;
        let param_off = cat_off + n_cats * CATEGORY_STRIDE;
        let end = n_params
            .checked_mul(4)
            .and_then(|p| p.checked_add(param_off))
            .ok_or(SoundDbError::TableOutOfBounds)?;
        layout("+0x10 cue table", rd32(bytes, 0x10)?, cue_off)?;
        layout("+0x14 category table", rd32(bytes, 0x14)?, cat_off)?;
        layout("+0x18 parameter table", rd32(bytes, 0x18)?, param_off)?;
        if end > bytes.len() {
            return Err(SoundDbError::TableOutOfBounds);
        }
        if bytes.len() != end {
            return Err(SoundDbError::BadLayout {
                field: "body length",
                found: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
                expected: end as u32,
            });
        }

        let mut cues = Vec::with_capacity(n_cues);
        for i in 0..n_cues {
            let o = cue_off + i * CUE_STRIDE;
            cues.push(CueEntry::routed(rd32(bytes, o)?, rd32(bytes, o + 4)?, rd32(bytes, o + 8)?));
        }
        let mut categories = Vec::with_capacity(n_cats);
        for i in 0..n_cats {
            let o = cat_off + i * CATEGORY_STRIDE;
            categories.push(CategoryEntry { category: rd32(bytes, o)?, parent: rd32(bytes, o + 4)? });
        }
        let params = (0..n_params).map(|i| rd32(bytes, param_off + 4 * i)).collect::<Result<_, _>>()?;
        let db = SoundDb { version: tag, self_hash, cues, categories, params };
        db.check_order()?;
        Ok(db)
    }

    fn check_order(&self) -> Result<(), SoundDbError> {
        if let Some(i) = (1..self.cues.len()).find(|&i| self.cues[i].guid <= self.cues[i - 1].guid) {
            return Err(SoundDbError::UnsortedCues { index: i });
        }
        if let Some(i) = (1..self.categories.len())
            .find(|&i| self.categories[i].category <= self.categories[i - 1].category)
        {
            return Err(SoundDbError::UnsortedCategories { index: i });
        }
        Ok(())
    }

    /// Serialize to the on-disk layout — the exact inverse of [`parse`](Self::parse) for the routing
    /// fields, the category tree and the parameters (the play-time cue fields are not on disk). Refuses
    /// a table the engine could not binary-search (unsorted or duplicate guids).
    pub fn to_bytes(&self) -> Result<Vec<u8>, SoundDbError> {
        if self.version != SOUNDDB_TAG {
            return Err(SoundDbError::BadTag(self.version));
        }
        for (what, count, max) in [
            ("cue entries", self.cues.len(), u16::MAX as usize),
            ("category entries", self.categories.len(), u16::MAX as usize),
            ("parameters", self.params.len(), u32::MAX as usize),
        ] {
            if count > max {
                return Err(SoundDbError::TooMany { what, count });
            }
        }
        self.check_order()?;
        let cat_off = HEADER_SIZE + self.cues.len() * CUE_STRIDE;
        let param_off = cat_off + self.categories.len() * CATEGORY_STRIDE;
        let end = param_off + 4 * self.params.len();
        let mut b = Vec::with_capacity(end);
        put_u32(&mut b, u32::from(SOUNDDB_TAG));
        put_u32(&mut b, self.self_hash);
        put_u16(&mut b, self.cues.len() as u16);
        put_u16(&mut b, self.categories.len() as u16);
        put_u32(&mut b, self.params.len() as u32);
        put_u32(&mut b, HEADER_SIZE as u32);
        put_u32(&mut b, cat_off as u32);
        put_u32(&mut b, param_off as u32);
        for c in &self.cues {
            put_u32(&mut b, c.guid);
            put_u32(&mut b, c.bank_hash);
            put_u32(&mut b, c.cue_index);
        }
        for c in &self.categories {
            put_u32(&mut b, c.category);
            put_u32(&mut b, c.parent);
        }
        for &p in &self.params {
            put_u32(&mut b, p);
        }
        Ok(b)
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
        SoundDb { version, self_hash, cues, categories: Vec::new(), params: Vec::new() }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wr_u32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// The `veh_support` header + first two entries (from the shipped WAD) must parse to the real
    /// routing: 7 cues, bank hash 0x84701C9A, cue indices in 0..=6.
    #[test]
    fn parses_real_veh_support_header() {
        let mut b = vec![0u8; HEADER_SIZE + 7 * CUE_STRIDE];
        b[0] = 0x1D;
        wr_u32(&mut b, 0x04, 0x8470_1C9A);
        wr_u32(&mut b, 0x08, 7);
        wr_u32(&mut b, 0x10, 0x1C);
        wr_u32(&mut b, 0x14, 0x70);
        wr_u32(&mut b, 0x18, 0x70);
        // entry 0: guid 0x1B2C8599, bank 0x84701C9A, cue 5
        wr_u32(&mut b, 0x1C, 0x1B2C_8599);
        wr_u32(&mut b, 0x20, 0x8470_1C9A);
        wr_u32(&mut b, 0x24, 5);
        // entry 1: guid 0x229D0B74, bank 0x84701C9A, cue 0
        wr_u32(&mut b, 0x28, 0x229D_0B74);
        wr_u32(&mut b, 0x2C, 0x8470_1C9A);
        wr_u32(&mut b, 0x30, 0);
        // entries 2..6: ascending placeholder guids (the table is binary-searched, so sorted).
        for (k, o) in (0x34..0x70).step_by(CUE_STRIDE).enumerate() {
            wr_u32(&mut b, o, 0x3000_0000 + k as u32);
            wr_u32(&mut b, o + 4, 0x8470_1C9A);
            wr_u32(&mut b, o + 8, 1 + k as u32);
        }

        let db = SoundDb::parse(&b).expect("real header parses");
        assert_eq!(db.self_hash, 0x8470_1C9A);
        assert_eq!(db.cues.len(), 7);
        let c0 = db.find_cue(0x1B2C_8599).expect("cue by hash");
        assert_eq!(c0.bank_hash, 0x8470_1C9A);
        assert_eq!(c0.cue_index, 5);
        assert_eq!(db.find_cue(0x229D_0B74).unwrap().cue_index, 0);
        // direct-index path (id < 0x401)
        assert_eq!(db.find_cue(0).unwrap().guid, 0x1B2C_8599);
        assert_eq!(db.to_bytes().expect("encodes"), b, "byte-identical re-encode");
    }

    /// The global catalog's shape: no cues, a category tree and parameter hashes.
    #[test]
    fn global_catalog_shape_round_trips() {
        let db = SoundDb {
            version: SOUNDDB_TAG,
            self_hash: 0x3775_0257,
            cues: vec![],
            categories: vec![
                CategoryEntry { category: 0x6413_FB86, parent: 0 },
                CategoryEntry { category: 0x7674_95E2, parent: 0x6413_FB86 },
                CategoryEntry { category: 0x8EC8_3583, parent: 0x7674_95E2 },
            ],
            params: vec![0xD11A_DEF6, 0xD913_464B],
        };
        let bytes = db.to_bytes().expect("encodes");
        assert_eq!(bytes.len(), HEADER_SIZE + 3 * CATEGORY_STRIDE + 8);
        assert_eq!(
            &bytes[0x08..0x1C],
            &[0, 0, 3, 0, 2, 0, 0, 0, 0x1C, 0, 0, 0, 0x1C, 0, 0, 0, 0x34, 0, 0, 0]
        );
        assert_eq!(SoundDb::parse(&bytes).expect("parses"), db);
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
        let bytes = db.to_bytes().expect("encodes");
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
    fn unsorted_or_misframed_tables_are_refused() {
        let unsorted = SoundDb::from_cues(
            SOUNDDB_TAG,
            vec![CueEntry::routed(0x3333_4444, 1, 0), CueEntry::routed(0x1111_2222, 1, 1)],
        );
        assert_eq!(unsorted.to_bytes(), Err(SoundDbError::UnsortedCues { index: 1 }));

        let good = SoundDb::from_cues(SOUNDDB_TAG, vec![CueEntry::routed(0x1111_2222, 1, 0)])
            .to_bytes()
            .expect("encodes");
        let mut long = good.clone();
        long.extend_from_slice(&[0; 4]);
        assert!(matches!(
            SoundDb::parse(&long),
            Err(SoundDbError::BadLayout { field: "body length", .. })
        ));
        let mut moved = good;
        moved[0x14] = 0x20;
        assert!(matches!(SoundDb::parse(&moved), Err(SoundDbError::BadLayout { .. })));
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
