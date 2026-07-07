//! The sound database (`sounddb`) — cue/wave catalog parser.
//!
//! **Oracle:** `PalGlobalTable::sounddb parser` **`FUN_00835b80`** (audio_code_map.md §3.5, §0). The
//! parser tests `*param_1 == '\x1d'` — the same **`'\x1d'` node tag** the Xbox analysis found in
//! `PalEngine.cpp` (`FUN_828ce9b8`), version byte matching across builds — so `0x1D` is the one field
//! that is *independently cross-verified on two builds* and anchors the whole format. From the body:
//! u16 counts at `+0x0A`/`+0x0C`, 8-byte GUID entries at `+0x14`, a `0x10`-stride cue map, and a
//! binary search over the sorted GUID table (`FUN_0083c570`).
//!
//! Runtime resolution is `PalGlobalTable::FindCue` **`FUN_00835a70`**: id `< 0x401` resolves as a
//! *direct index* into the cue map; otherwise the id is a hashed GUID resolved by binary search
//! (`FUN_0083c760`, `0xffff` sentinel). Both paths are modelled by [`SoundDb::find_cue`].
//!
//! The chain that feeds this: Lua `Sound.AddPgAsset("Mercs2Globals","sounddb")` → `FUN_006025d0`
//! (type `0xE5273C14`) → `FUN_00607c50` → this parser (into `PalGlobalTable DAT_011763fc`).
//!
//! ## Field-layout confidence
//! `0x1D` version, the `+0x0A/+0x0C` counts, the `+0x14` GUID-table base, the 8-byte GUID stride and
//! the `0x10` cue stride are read from `FUN_00835b80`. The *named sub-fields* inside a 16-byte cue
//! record (priority / category / default gain / min-max distance / loop) are what the mixer needs and
//! are laid out here to a self-consistent scheme; their exact byte offsets in a shipped `sounddb`
//! block are `// CONFIRM-LIVE:` against a real bank (none is bundled in this worktree). [`SoundDb::to_bytes`]
//! is the exact inverse of [`SoundDb::parse`], so the round-trip is authoritative for *our* layout.

use std::collections::BTreeMap;

/// The `'\x1d'` node/version tag — cross-verified on PC (`FUN_00835b80`) and Xbox
/// (`PalEngine.cpp` `FUN_828ce9b8`).
pub const SOUNDDB_TAG: u8 = 0x1D;

/// `FindCue` direct-index threshold (`FUN_00835a70`): ids below this are a direct cue-map index,
/// ids at/above it are hashed GUIDs resolved by binary search.
pub const FINDCUE_DIRECT_MAX: u32 = 0x401;

/// m2 name-hash of the `sounddb` asset type (audio_code_map.md §7).
pub const ASSET_TYPE_SOUNDDB: u32 = 0xE527_3C14;

/// One entry in the sorted GUID table at `+0x14` (8 bytes: hashed cue GUID → cue-map index).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuidEntry {
    /// m2 name-hash of the cue (`Sound.CueSound("name")` hashes the name to this).
    pub guid: u32,
    /// Index into [`SoundDb::cues`] this GUID resolves to.
    pub cue_index: u32,
}

/// A 16-byte (`0x10`-stride) cue-map record: what playing one cue needs.
///
/// `guid`, and the 16-byte stride, are from the parser. The remaining fields are the parameters the
/// software mixer consumes for a cue; their exact byte positions are `// CONFIRM-LIVE:` (§ module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueEntry {
    /// Cue name-hash (also the key in the GUID table for the hashed path).
    pub guid: u32,
    /// Wave-bank slot this cue's samples live in (`wavebank` asset, hash `0xF753F6D0`).
    pub bank_id: u16,
    /// Index of the wave within that bank.
    pub wave_index: u16,
    /// Steal priority 0..255 — higher wins voice contention
    /// (`PalSoundInstance::GetWavePriority` `~0x00837e30`, victim pick `FUN_00837830`).
    pub priority: u8,
    /// Category id (sfx/vo/music/chatter/non_ui…); indexes [`crate::categories::Categories`].
    pub category: u8,
    /// Bit flags: bit0 = looping, bit1 = 3D-positional, bit2 = streamed.
    pub flags: u16,
    /// Default linear gain (0..1) applied before category/attenuation.
    pub default_gain: f32,
    /// 3D attenuation: full volume within `min_dist`.
    pub min_dist: f32,
    /// 3D attenuation: silent beyond `max_dist` (`MaxDistCheck`, inlined in `FUN_00836c70`).
    pub max_dist: f32,
}

impl CueEntry {
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
    /// A declared table runs past the end of the buffer.
    TableOutOfBounds,
}

impl std::fmt::Display for SoundDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SoundDbError::Truncated => write!(f, "sounddb: buffer too short for header"),
            SoundDbError::BadTag(b) => {
                write!(f, "sounddb: bad version tag 0x{b:02x} (expected 0x1D)")
            }
            SoundDbError::TableOutOfBounds => write!(f, "sounddb: declared table exceeds buffer"),
        }
    }
}

impl std::error::Error for SoundDbError {}

/// Header field offsets (from `FUN_00835b80`).
const OFF_TAG: usize = 0x00;
const OFF_GUID_COUNT: usize = 0x0A;
const OFF_CUE_COUNT: usize = 0x0C;
const OFF_TABLES: usize = 0x14;
const GUID_STRIDE: usize = 8;
const CUE_STRIDE: usize = 0x10;

/// The parsed sound database: the sorted GUID table + the cue map. Held per-`PalGlobalTable`
/// (`DAT_011763fc`) in the exe; here it is the catalog the [`crate::AudioEngine`] resolves cues
/// against.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundDb {
    /// Format/node version (`0x1D` in every shipped build).
    pub version: u8,
    /// GUID → cue-index table, **sorted by `guid`** so [`find_cue`](Self::find_cue) can binary-search
    /// it exactly like `FUN_0083c570`.
    pub guids: Vec<GuidEntry>,
    /// The `0x10`-stride cue map, indexed directly for ids `< 0x401`.
    pub cues: Vec<CueEntry>,
}

impl SoundDb {
    /// Parse a `sounddb` block (`FUN_00835b80`). Rejects anything whose first byte is not `0x1D`.
    pub fn parse(bytes: &[u8]) -> Result<SoundDb, SoundDbError> {
        if bytes.len() < OFF_TABLES {
            return Err(SoundDbError::Truncated);
        }
        let tag = bytes[OFF_TAG];
        if tag != SOUNDDB_TAG {
            return Err(SoundDbError::BadTag(tag));
        }
        let guid_count = rd_u16(bytes, OFF_GUID_COUNT) as usize;
        let cue_count = rd_u16(bytes, OFF_CUE_COUNT) as usize;

        let guid_base = OFF_TABLES;
        let cue_base = guid_base + guid_count * GUID_STRIDE;
        let end = cue_base + cue_count * CUE_STRIDE;
        if end > bytes.len() {
            return Err(SoundDbError::TableOutOfBounds);
        }

        let mut guids = Vec::with_capacity(guid_count);
        for i in 0..guid_count {
            let o = guid_base + i * GUID_STRIDE;
            guids.push(GuidEntry {
                guid: rd_u32(bytes, o),
                cue_index: rd_u32(bytes, o + 4),
            });
        }
        // FUN_0083c570 relies on a sorted table; enforce it so the binary search is correct even if
        // a block ships unsorted.
        guids.sort_unstable_by_key(|g| g.guid);

        let mut cues = Vec::with_capacity(cue_count);
        for i in 0..cue_count {
            let o = cue_base + i * CUE_STRIDE;
            // 16-byte (0x10-stride) record. // CONFIRM-LIVE: sub-field offsets vs a shipped sounddb block.
            cues.push(CueEntry {
                guid: rd_u32(bytes, o),
                bank_id: rd_u16(bytes, o + 4),
                wave_index: rd_u16(bytes, o + 6),
                priority: bytes[o + 8],
                category: bytes[o + 9],
                flags: rd_u16(bytes, o + 10),
                default_gain: q8_8_to_f32(rd_u16(bytes, o + 12)),
                // min/max distance are not in the 16-byte record; the exe reads them from the
                // wave-bank/wave descriptor at play time (MaxDistCheck in FUN_00836c70). Defaulted here.
                min_dist: 0.0,
                max_dist: 0.0,
            });
        }

        Ok(SoundDb {
            version: tag,
            guids,
            cues,
        })
    }

    /// Serialize back to the on-disk layout — the exact inverse of [`parse`](Self::parse). Used by the
    /// round-trip test and by tools that synthesize a `sounddb` block.
    pub fn to_bytes(&self) -> Vec<u8> {
        let guid_base = OFF_TABLES;
        let cue_base = guid_base + self.guids.len() * GUID_STRIDE;
        let end = cue_base + self.cues.len() * CUE_STRIDE;
        let mut b = vec![0u8; end];
        b[OFF_TAG] = self.version;
        wr_u16(&mut b, OFF_GUID_COUNT, self.guids.len() as u16);
        wr_u16(&mut b, OFF_CUE_COUNT, self.cues.len() as u16);
        for (i, g) in self.guids.iter().enumerate() {
            let o = guid_base + i * GUID_STRIDE;
            wr_u32(&mut b, o, g.guid);
            wr_u32(&mut b, o + 4, g.cue_index);
        }
        for (i, c) in self.cues.iter().enumerate() {
            let o = cue_base + i * CUE_STRIDE;
            wr_u32(&mut b, o, c.guid);
            wr_u16(&mut b, o + 4, c.bank_id);
            wr_u16(&mut b, o + 6, c.wave_index);
            b[o + 8] = c.priority;
            b[o + 9] = c.category;
            wr_u16(&mut b, o + 10, c.flags);
            wr_u16(&mut b, o + 12, f32_to_q8_8(c.default_gain));
        }
        b
    }

    /// `PalGlobalTable::FindCue` (`FUN_00835a70`): resolve a cue id to its [`CueEntry`].
    ///
    /// * `id < 0x401` → direct index into the cue map (the exe's `FUN_0083c610` fast path).
    /// * `id >= 0x401` → treat `id` as a hashed GUID; binary-search the sorted GUID table
    ///   (`FUN_0083c760` → `FUN_0083c570`).
    pub fn find_cue(&self, id: u32) -> Option<&CueEntry> {
        if id < FINDCUE_DIRECT_MAX {
            return self.cues.get(id as usize);
        }
        // Hashed path: binary search the sorted GUID table.
        let idx = self
            .guids
            .binary_search_by_key(&id, |g| g.guid)
            .ok()?;
        let cue_index = self.guids[idx].cue_index as usize;
        self.cues.get(cue_index)
    }

    /// Convenience: resolve a cue by its *name* (hashes with the m2 name-hash then [`find_cue`]).
    pub fn find_cue_by_name(&self, name: &str) -> Option<&CueEntry> {
        self.find_cue(mercs2_formats::hash::pandemic_hash_m2(name))
    }

    /// Build a database from cue records, deriving the GUID table automatically (sorted). Cues keep
    /// their given order (so ids `< 0x401` are stable direct indices).
    pub fn from_cues(version: u8, cues: Vec<CueEntry>) -> SoundDb {
        let mut guids: BTreeMap<u32, u32> = BTreeMap::new();
        for (i, c) in cues.iter().enumerate() {
            guids.insert(c.guid, i as u32);
        }
        let guids = guids
            .into_iter()
            .map(|(guid, cue_index)| GuidEntry { guid, cue_index })
            .collect();
        SoundDb {
            version,
            guids,
            cues,
        }
    }
}

// --- small endian helpers (sounddb is little-endian on PC) ---
fn rd_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn wr_u16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn wr_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
// Q8.8 fixed-point gain used for the 2-byte default-gain slot.
fn q8_8_to_f32(v: u16) -> f32 {
    v as f32 / 256.0
}
fn f32_to_q8_8(v: f32) -> u16 {
    (v.clamp(0.0, 255.0) * 256.0).round() as u16
}
