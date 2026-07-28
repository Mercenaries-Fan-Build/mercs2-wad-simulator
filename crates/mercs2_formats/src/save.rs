//! Mercenaries 2 (PC) `.profile` save-game parser.
//!
//! A `.profile` is a fixed-size **13,404-byte** file: a packed binary header
//! followed by a **zlib** stream (starting at `0x468`) that decompresses to the
//! game's Lua `SaveSingleton` state (cash / fuel / faction / mission tables).
//!
//! This module reverses the header fields that are grounded either in a
//! byte-for-byte diff of the **eight retail saves vendored at `fixtures/saves`**
//! (see that directory's README; reached via
//! [`crate::game_paths::save_fixtures`], never a hardcoded path) or in the engine save symbols
//! (`docs/mercs2-pdb-analysis/game-systems.md`: `ProfileHash`, `SetLuaSaveVersion`,
//! `SetProfileCostume`, `saveProfile`, ...). Fields whose *meaning* is not
//! grounded are named `unknown_<offset>` or flagged `INFERRED`.
//!
//! There is **no magic constant** at `0x00`: that u32 varies across every save
//! and is a per-file integrity **checksum/hash** (`ProfileHash`). The stable
//! structural sentinels are `version == 4` (`@0x04`), `data_size == len-4`
//! (`@0x08`), and the zlib header byte `0x78` at `0x468`. See `SAVE_FORMAT.md`.
//!
//! The `ProfileHash` algorithm is now **derived**: CRC-32/BZIP2 over `[4:]`
//! (see [`crate::save_write`]). The reader can verify it via
//! [`Profile::hash_ok`]; the writer stamps a real one.

use std::collections::BTreeMap;
use std::io::Read;

/// Fixed on-disk size of every retail `.profile` (bytes).
pub const PROFILE_SIZE: usize = 13_404;
/// Save-format version this parser understands (`SetLuaSaveVersion`).
pub const VERSION: u32 = 4;
/// Byte offset of the zlib-compressed Lua payload.
pub const ZLIB_OFFSET: usize = 0x468;

// --- header field offsets (FACT: located by cross-file diff) ---
// `pub(crate)` so the writer in `save_write.rs` re-stamps the exact same offsets.
pub(crate) const OFF_CHECKSUM: usize = 0x00; // u32  per-file hash (ProfileHash), CRC-32/BZIP2 over [4:]
pub(crate) const OFF_VERSION: usize = 0x04; // u32  == VERSION
pub(crate) const OFF_DATA_SIZE: usize = 0x08; // u32  == file_len - 4 (bytes the checksum covers)
pub(crate) const OFF_UNK_0C: usize = 0x0C; // u32  constant 0x3 across all saves
pub(crate) const OFF_UNK_10: usize = 0x10; // u32  constant 0x0
pub(crate) const OFF_PLAY_TIME: usize = 0x14; // u32  play-time seconds (INFERRED)
pub(crate) const OFF_CASH: usize = 0x18; // u32  PMC cash (INFERRED)
pub(crate) const OFF_FUEL: usize = 0x1C; // u32  PMC fuel (INFERRED)
pub(crate) const OFF_UNK_20: usize = 0x20; // u32  constant 0x0
pub(crate) const OFF_TIMESTAMP: usize = 0x24; // u32  unix timestamp of the save
pub(crate) const OFF_CONTRACT: usize = 0x2C; // [16] NUL-padded ASCII active contract id (FACT)
pub(crate) const CONTRACT_LEN: usize = 16;
pub(crate) const OFF_FLAGS_4C: usize = 0x4C; // u32  raw dword; byte +1 is the hero (see OFF_CHARACTER)
pub(crate) const OFF_CHARACTER: usize = 0x4D; // u8  1-based hero: 1 mattias / 2 chris / 3 jen (INFERRED, strong)
pub(crate) const OFF_SAVE_NAME: usize = 0x20A; // UTF-16LE NUL-terminated slot name (FACT)
pub(crate) const OFF_FUEL_CAP: usize = 0x2F8; // u16  fuel capacity? tracks fuel (INFERRED)
pub(crate) const OFF_UNLOCKED_COSTUMES: usize = 0x24A; // u8  unlocked-costume count (1 fresh, 5 = all base outfits)
pub(crate) const OFF_UNK_24B: usize = 0x24B; // u8  unknown (1 in every observed save)
pub(crate) const OFF_UPGRADE: usize = 0x4F; // u8  hero upgrade tier 0..3 (drives the upgrade TEMPLATE = the look)

/// Decoded Mercenaries 2 `.profile` save.
///
/// Raw header fields are exposed as public members. Grounding for each is noted
/// in the module docs and `SAVE_FORMAT.md` (FACT vs INFERRED).
#[derive(Debug, Clone)]
pub struct Profile {
    /// `@0x00` u32 — per-file integrity checksum (`ProfileHash`): **CRC-32/BZIP2
    /// over bytes `[4:]`** (poly `0x04C11DB7`, init/xorout `0xFFFFFFFF`,
    /// non-reflected). Stored verbatim on read; validate with [`Profile::hash_ok`].
    /// Varies every save. See [`crate::save_write`].
    pub checksum: u32,
    /// `@0x04` u32 — save-format version. Always `4` in retail. Validated.
    pub version: u32,
    /// `@0x08` u32 — size the checksum covers: `file_len - 4` (`0x3458`). Validated.
    pub data_size: u32,
    /// `@0x0C` u32 — constant `3` across all observed saves. Meaning unknown.
    pub unknown_0x0c: u32,
    /// `@0x10` u32 — constant `0`. Meaning unknown.
    pub unknown_0x10: u32,
    /// `@0x14` u32 — play-time in seconds. INFERRED (monotonic, small).
    pub play_time_seconds: u32,
    /// `@0x18` u32 — PMC cash. INFERRED (values 50000..~342M, within the 1B cap).
    pub cash: u32,
    /// `@0x1C` u32 — PMC fuel. INFERRED (values 0..5485, tracks `fuel_capacity`).
    pub fuel: u32,
    /// `@0x20` u32 — constant `0`. Meaning unknown.
    pub unknown_0x20: u32,
    /// `@0x24` u32 — unix timestamp of the save (2008 devsave .. 2026). FACT.
    pub timestamp: u32,
    /// `@0x2C` 16B — active/last mission **contract id**, NUL-padded ASCII
    /// (`PmcCon001`, `OilCon003`, `PmcJob001`, ...). FACT.
    pub active_contract: String,
    /// `@0x4C` u32 — raw dword, kept for reference. NOT one bitfield: byte `@0x4D` is the hero
    /// (exposed as `character_index`); byte `@0x4F` is a progression flag (3 on completed saves).
    pub flags_0x4c: u32,
    /// `@0x4D` u8 — HERO (`Get/SetProfileCharacter` = runtime profile object byte `+0x61`,
    /// `FUN_005df790/7d0`). Values are engine-coded (`FUN_00634810` display-name switch):
    /// 1 = `SHELL.SelectCharacter.MattiasNilsson`, 2 = `.ChrisJacobs`, 3 = `.JenniferMui`,
    /// else "Player". File offset verified by save diff: the Jen save stores 3, all Mattias
    /// saves store 1, and it is the only header byte separating two parallel fresh saves.
    pub character_index: u8,
    /// `@0x24A` u8 — UNLOCKED-costume count (feeds `Player.GetAvailableCostumes`, the wardrobe
    /// menu gate in `_SelectOutfit`): 1 on fresh/mid saves, 5 (= all five base outfits) on the
    /// completed saves. NOT the selected costume (disproved by user ground truth: saves with
    /// different looks share it via upgrade tier, wardrobe untouched).
    pub unlocked_costumes: u8,
    /// `@0x24B` u8 — unknown, `1` in every observed save. NOT the costume/character.
    pub unknown_0x24b: u8,
    /// `@0x4F` u8 — hero UPGRADE tier 0..3 (`Get/SetProfileUpgrade` = runtime profile object
    /// byte `+0x62`, `FUN_005df830/870`). Drives the spawn TEMPLATE via `_tCharacterMap.templates`
    /// (`mrxplayer.lua:167-168`: `tTemplates[iUpgrade] or base`) — the hero's LOOK progresses with
    /// tier, no wardrobe involvement. Observed: 0 on fresh/mid saves (default skin,
    /// user-confirmed), 3 on both completed-save copies (Mattias endgame "MetalHead" look,
    /// user-confirmed).
    pub upgrade_index: u8,
    /// `@0x2F8` u16 — fuel capacity (max fuel); tracks/exceeds `fuel`. INFERRED.
    pub fuel_capacity: u16,
    /// `@0x20A` — save-slot name, UTF-16LE NUL-terminated (e.g. `auto_634304EA`).
    /// This is the autosave/slot label, **not** the player display name. FACT.
    pub save_name: String,
    /// Whole file, retained so the zlib Lua payload can be decompressed on demand.
    raw: Vec<u8>,
}

fn rd_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn rd_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

/// Parse a `.profile` byte buffer.
///
/// Validates the structural sentinels (`version == 4`, `data_size == len-4`, and
/// the zlib header byte at `0x468`). Returns `Err` with a description otherwise.
pub fn parse(bytes: &[u8]) -> Result<Profile, String> {
    if bytes.len() < ZLIB_OFFSET + 2 {
        return Err(format!(
            "file too short: {} bytes (need at least {})",
            bytes.len(),
            ZLIB_OFFSET + 2
        ));
    }

    let version = rd_u32(bytes, OFF_VERSION);
    if version != VERSION {
        return Err(format!("unexpected version {version} (expected {VERSION})"));
    }

    let data_size = rd_u32(bytes, OFF_DATA_SIZE);
    let expected = (bytes.len() as u32).wrapping_sub(4);
    if data_size != expected {
        return Err(format!(
            "data_size 0x{data_size:X} != file_len-4 0x{expected:X}"
        ));
    }

    // Zlib payload sentinel: CMF byte 0x78 (deflate, 32K window).
    if bytes[ZLIB_OFFSET] != 0x78 {
        return Err(format!(
            "no zlib stream at 0x{ZLIB_OFFSET:X}: byte 0x{:02X}",
            bytes[ZLIB_OFFSET]
        ));
    }

    let active_contract = read_cstr_ascii(&bytes[OFF_CONTRACT..OFF_CONTRACT + CONTRACT_LEN]);
    let save_name = read_utf16z(bytes, OFF_SAVE_NAME, 64);

    Ok(Profile {
        checksum: rd_u32(bytes, OFF_CHECKSUM),
        version,
        data_size,
        unknown_0x0c: rd_u32(bytes, OFF_UNK_0C),
        unknown_0x10: rd_u32(bytes, OFF_UNK_10),
        play_time_seconds: rd_u32(bytes, OFF_PLAY_TIME),
        cash: rd_u32(bytes, OFF_CASH),
        fuel: rd_u32(bytes, OFF_FUEL),
        unknown_0x20: rd_u32(bytes, OFF_UNK_20),
        timestamp: rd_u32(bytes, OFF_TIMESTAMP),
        active_contract,
        flags_0x4c: rd_u32(bytes, OFF_FLAGS_4C),
        unlocked_costumes: bytes[OFF_UNLOCKED_COSTUMES],
        unknown_0x24b: bytes[OFF_UNK_24B],
        upgrade_index: bytes[OFF_UPGRADE],
        character_index: bytes[OFF_CHARACTER],
        fuel_capacity: rd_u16(bytes, OFF_FUEL_CAP),
        save_name,
        raw: bytes.to_vec(),
    })
}

impl Profile {
    /// The active-contract mission id (`@0x2C`), e.g. `"PmcCon001"`.
    pub fn active_contract(&self) -> &str {
        &self.active_contract
    }

    /// The save-slot label (`@0x20A`), e.g. `"auto_634304EA"`.
    pub fn save_name(&self) -> &str {
        &self.save_name
    }

    /// Raw zlib-compressed payload (from `0x468` to end of file, incl. trailing
    /// padding that the deflate stream ignores).
    pub fn compressed_payload(&self) -> &[u8] {
        &self.raw[ZLIB_OFFSET..]
    }

    /// The whole on-disk file, retained verbatim from [`parse`].
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }

    /// Mutable access to the retained on-disk buffer (crate-internal — the writer
    /// re-stamps header fields and the payload region in place).
    pub(crate) fn raw_mut(&mut self) -> &mut Vec<u8> {
        &mut self.raw
    }

    /// Recompute the `ProfileHash` (`@0x00`) over the on-disk bytes `[4:]`.
    ///
    /// The algorithm is **CRC-32/BZIP2** (poly `0x04C11DB7`, init/xorout
    /// `0xFFFFFFFF`, non-reflected) — see [`crate::save_write::profile_hash`],
    /// derived and verified byte-exact against every retail `.profile`.
    pub fn computed_hash(&self) -> u32 {
        crate::save_write::profile_hash(&self.raw[4..])
    }

    /// Whether the stored `checksum` (`@0x00`) matches [`Profile::computed_hash`].
    /// True for every uncorrupted retail save (this is the engine's integrity check).
    pub fn hash_ok(&self) -> bool {
        self.checksum == self.computed_hash()
    }

    /// Decompress the Lua `SaveSingleton` payload. This is the authoritative
    /// game-state blob (cash/fuel/faction/mission tables serialized as Lua).
    pub fn decompress_lua(&self) -> Result<Vec<u8>, String> {
        let mut dec = flate2::read::ZlibDecoder::new(self.compressed_payload());
        let mut out = Vec::new();
        dec.read_to_end(&mut out)
            .map_err(|e| format!("zlib decompress failed: {e}"))?;
        Ok(out)
    }
}

/// Read a NUL-terminated (or region-bounded) ASCII string, trimming trailing NULs.
fn read_cstr_ascii(region: &[u8]) -> String {
    let end = region.iter().position(|&b| b == 0).unwrap_or(region.len());
    String::from_utf8_lossy(&region[..end]).into_owned()
}

/// Read a NUL-terminated UTF-16LE string starting at `off`, capped at `max_chars`.
fn read_utf16z(bytes: &[u8], off: usize, max_chars: usize) -> String {
    let mut units = Vec::new();
    let mut i = off;
    while i + 1 < bytes.len() && units.len() < max_chars {
        let c = rd_u16(bytes, i);
        if c == 0 {
            break;
        }
        units.push(c);
        i += 2;
    }
    String::from_utf16_lossy(&units)
}

// ===========================================================================
// SaveSingleton Lua boot-state
// ===========================================================================
//
// `decompress_lua()` yields the serialized `SaveSingleton` table as **readable
// Lua source** (not bytecode): 24.8K–54K of text. This section decodes the
// boot-relevant fields into structured Rust so `mercs2_game` can restore the
// real start-state (mission flow, active missions, world overlay layers, ...).
//
// Grounding: the field set and extraction mirror the legacy regex harvest in
// `tools/savefile_parser.py` (`harvest_from_lua`) and the observed layout of
// the six retail saves. The Lua is plain text — a light brace/quote-aware
// table walker is sufficient; no Lua interpreter is needed.
//
// Observed top-level shape (verified on `auto_6A447BF8.profile`):
// ```text
// {
//   ["vEquippedSupport"] = { [1]="[vehicle.wz10]", ... },   -- ordered tokens
//   ["nTimeElapsed"]     = 964.000000,                      -- playtime seconds
//   ["tFlowData"] = {                                       -- mission-flow container
//     ["tCulledBindings"] = { [1]="Start", [2]="VzaCon001", [3]="PmcCon001" },
//     ["tActiveMissions"] = { ["PmcJob001"] = { ["nState"]=1, ["_nTargetsComplete"]=1,
//                                               ["tCollected"]={ Sys.StringToGuid('0x0013E2C6') } }, ... },
//     ["tMyFlowData"]     = { ["PmcCon001"]=1, ["VzaCon001"]=1 },  -- completed flow flags
//   },
//   ["tLayerData"] = { [1]="vz_state_mer_big_lineregion", ... },   -- ~200-300 world overlays
// }
// ```
// Each of `tCulledBindings` / `tActiveMissions` / `tMyFlowData` / `tLayerData`
// / `nTimeElapsed` / `vEquippedSupport` appears exactly once per file, so they
// are located by key name globally; per-mission fields are scoped to their own
// mission body to avoid colliding with `tMyFlowData` (same mission ids).

/// One entry of `tFlowData.tActiveMissions` — a mission currently in progress.
#[derive(Debug, Clone)]
pub struct ActiveMission {
    /// Mission id / key (e.g. `"PmcJob001"`, `"OilCon020"`). FACT.
    pub id: String,
    /// `["nState"]` — mission state code (0 = queued/available, 1 = active/…).
    /// Stored as `f64` because the Lua serializes every number as a float. FACT
    /// that it is `nState`; the numeric *meaning* of each code is INFERRED.
    pub state: f64,
    /// `["_nTargetsComplete"]` — number of objectives ticked off, when present.
    /// FACT (key name); absent for freshly-queued missions.
    pub targets_complete: Option<f64>,
    /// `["tCollected"]` — GUIDs collected for this mission, decoded from
    /// `Sys.StringToGuid('0x........')`. FACT (these are collectible entity guids).
    pub collected: Vec<u32>,
}

/// Decoded boot-state from the `SaveSingleton` Lua payload.
///
/// Drives `mercs2_game` start-up: `flow_chain` seeds the mission-flow FSM,
/// `active_missions` restores in-progress contracts, `layers` selects the
/// `vz_state_*` world overlays to stream (see
/// `docs/modernization/world_streaming_spec.md §5`).
#[derive(Debug, Clone, Default)]
pub struct SaveState {
    /// `tFlowData.tCulledBindings` — the mission-flow binding chain, **in order**
    /// (e.g. `["Start", "VzaCon001", "PmcCon001"]`). FACT.
    pub flow_chain: Vec<String>,
    /// `tFlowData.tActiveMissions` — in-progress missions. FACT.
    pub active_missions: Vec<ActiveMission>,
    /// `tFlowData.tMyFlowData` — completed / advanced flow flags, mission-id →
    /// flag value (`1` = seen/complete, higher = later stage). FACT (key name);
    /// per-value meaning INFERRED. Sorted by id (`BTreeMap`).
    pub completed_flow: BTreeMap<String, f64>,
    /// `tLayerData` — active `vz_state_*` world-overlay layer names, **order
    /// preserved** (destruction / staging / faction / pristine overlays). This
    /// is the overlay set the streamer must load. FACT.
    pub layers: Vec<String>,
    /// `nTimeElapsed` — total playtime in seconds. FACT (key name); value is a
    /// float in the Lua. INFERRED that the unit is seconds (matches header
    /// `play_time_seconds`).
    pub time_elapsed_secs: f64,
    /// `vEquippedSupport` — ordered equipped support/vehicle tokens
    /// (`"[vehicle.wz10]"`, `"[support.airstrike.fuelairbomb.name]"`, …), may be
    /// empty. FACT (matches `savefile_parser.py` vehicle/support harvest).
    pub equipped_support: Vec<String>,
    /// `tTransitData.bEnabled` — whether the transit (fast-travel) system is switched on at all.
    /// `MrxTransit.LoadSingleton` passes it straight to `SetSystemEnabled`. FACT.
    pub transit_enabled: bool,
    /// `tTransitData[n]` — per-landing-zone transit state, sorted by zone. FACT.
    ///
    /// Retail vz authors 23 zones (`1..8, 12, 15..18, 20..25, 27..30`), which is exactly the set the
    /// `LandingZone` COMP enumerates — see
    /// `mercs2_engine::worldutil`'s `retail_capture_corroborates_the_authored_landing_zone_set`.
    pub transit_zones: Vec<TransitZone>,
    /// `tStarterData` keys — the UNLOCKED starter/recruit ids in this save. `PmcBoss` (Fiona) is the
    /// always-present HQ boss; `HelPmcBoss`/`MecPmcBoss`/`JetPmcBoss` are the Villa recruits (Ewen/Eva/
    /// Misha), each present only after its unlock mission. `MrxStarterManager.SaveSingleton` writes only
    /// unlocked starters, so this is the source of truth for which PMC-interior recruit layers load. FACT.
    pub unlocked_starters: Vec<String>,
}

/// One transit landing zone as a save records it.
///
/// The field set is `MrxTransit.SaveSingleton` (`resident/mrxtransit.lua:362-376`) verbatim — it
/// writes exactly `sFactionAbbrev`, `bHasPlayedFanfare`, `bIsNuked`, `bEnabled` per zone — and
/// `LoadSingleton` (`:378-404`) reads back the same four. A zone the save leaves partly unset (an
/// early save writes only `bEnabled`) comes back with the remaining flags false / `None`, which is
/// what Lua's own nil-for-missing does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransitZone {
    /// Absolute zone number — the table key. NOT a position: the retail set is sparse.
    pub zone: u32,
    /// `sFactionAbbrev` — the faction that owns the zone (`"Pmc"`, `"Oil"`, `"Gur"`, …). `None` on a
    /// zone the player has not taken, and on the zone-6 `bFake` pad, which is never affiliated.
    pub faction: Option<String>,
    /// `bEnabled` — the zone is available for fast travel.
    pub enabled: bool,
    /// `bIsNuked` — a mission has nuked the site (`MrxTransit.SetLocationIsNuked`).
    pub is_nuked: bool,
    /// `bHasPlayedFanfare` — the capture-fanfare has already played, so a resume must not replay it.
    pub played_fanfare: bool,
}

/// Decode a `tTransitData` body into `(bEnabled, zones)`.
///
/// Top-level keys are the zone numbers, written by the Lua serializer as floats (`[1.000000]`), plus
/// the one string key `bEnabled` for the system-wide switch.
fn parse_transit(body: &str) -> (bool, Vec<TransitZone>) {
    let mut enabled = false;
    let mut zones: Vec<TransitZone> = Vec::new();
    for (key, raw) in parse_table(body) {
        if key == "bEnabled" {
            enabled = raw.trim() == "true";
            continue;
        }
        // `[1.000000]` — parse as f64 first; an integer-only parse misses every real save.
        let Ok(n) = key.trim().parse::<f64>() else { continue };
        let v = raw.trim();
        let inner = v.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(v);
        let mut z = TransitZone { zone: n as u32, ..Default::default() };
        for (fk, fv) in parse_table(inner) {
            let is_true = fv.trim() == "true";
            match fk.as_str() {
                "bEnabled" => z.enabled = is_true,
                "bIsNuked" => z.is_nuked = is_true,
                "bHasPlayedFanfare" => z.played_fanfare = is_true,
                "sFactionAbbrev" => z.faction = Some(unquote(&fv)),
                _ => {}
            }
        }
        zones.push(z);
    }
    zones.sort_by_key(|z| z.zone);
    (enabled, zones)
}

impl SaveState {
    /// Total collectibles gathered across all active missions (`tCollected`).
    pub fn collected_count(&self) -> usize {
        self.active_missions.iter().map(|m| m.collected.len()).sum()
    }
}

impl Profile {
    /// Decompress the Lua payload and decode it into structured [`SaveState`].
    pub fn save_state(&self) -> Result<SaveState, String> {
        let lua = self.decompress_lua()?;
        let text = String::from_utf8_lossy(&lua);
        parse_save_state(&text)
    }
}

/// Strip one layer of surrounding `"…"` from a Lua string literal.
fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Return the inner text of the first Lua table keyed by `["key"]` in `s`
/// (between the matching `{`…`}`, exclusive), or `None` if absent.
///
/// Brace matching skips over `"…"` / `'…'` string literals so braces inside a
/// string can never be miscounted (none occur in these saves, but be safe).
fn table_body<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("[\"{key}\"]");
    let start = s.find(&needle)? + needle.len();
    let b = s.as_bytes();
    let mut i = start;
    while i < b.len() && b[i] != b'{' {
        i += 1;
    }
    if i >= b.len() {
        return None;
    }
    let open = i;
    let mut depth = 0usize;
    while i < b.len() {
        match b[i] {
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[open + 1..i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Read the scalar value assigned to `["key"]` in `s` (up to the next comma or
/// newline), trimmed. Used for `nState` / `_nTargetsComplete` / `nTimeElapsed`.
fn scalar_value<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("[\"{key}\"]");
    let start = s.find(&needle)? + needle.len();
    let rest = &s[start..];
    let eq = rest.find('=')?;
    let after = &rest[eq + 1..];
    let end = after.find([',', '\n']).unwrap_or(after.len());
    Some(after[..end].trim())
}

/// Walk one Lua table body (the text *inside* the braces) and return its
/// top-level `(key, raw_value)` entries **in source order**. Keys are unquoted;
/// values are returned verbatim (a `{…}` block keeps its braces, scalars are
/// trimmed). Nested tables are skipped as whole values, so `tActiveMissions`
/// entries do not leak their inner `tCollected` / `tTargets` keys.
fn parse_table(inner: &str) -> Vec<(String, String)> {
    let b = inner.as_bytes();
    let n = b.len();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < n {
        while i < n && (b[i].is_ascii_whitespace() || b[i] == b',') {
            i += 1;
        }
        if i >= n || b[i] != b'[' {
            if i < n {
                i += 1;
            }
            continue;
        }
        i += 1; // past '['
        let key: String;
        if i < n && b[i] == b'"' {
            i += 1;
            let ks = i;
            while i < n && b[i] != b'"' {
                i += 1;
            }
            key = inner[ks..i].to_string();
        } else {
            let ks = i;
            while i < n && b[i] != b']' {
                i += 1;
            }
            key = inner[ks..i].trim().to_string();
        }
        while i < n && b[i] != b']' {
            i += 1;
        }
        if i < n {
            i += 1; // past ']'
        }
        while i < n && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < n && b[i] == b'=' {
            i += 1;
        }
        while i < n && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let vs = i;
        if i < n && b[i] == b'{' {
            let mut depth = 0usize;
            while i < n {
                match b[i] {
                    q @ (b'"' | b'\'') => {
                        i += 1;
                        while i < n && b[i] != q {
                            i += 1;
                        }
                    }
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            out.push((key, inner[vs..i].to_string()));
        } else {
            while i < n && b[i] != b',' && b[i] != b'\n' {
                i += 1;
            }
            out.push((key, inner[vs..i].trim().to_string()));
        }
    }
    out
}

/// Decode every `0x........` hex literal in a `tCollected` block (each is the
/// argument of `Sys.StringToGuid('0x........')`) into a `u32` GUID.
fn extract_guids(block: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut rest = block;
    while let Some(p) = rest.find("0x") {
        let hex = &rest[p + 2..];
        let end = hex
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(hex.len());
        if end > 0 {
            if let Ok(v) = u32::from_str_radix(&hex[..end], 16) {
                out.push(v);
            }
        }
        rest = &hex[end..];
    }
    out
}

/// Decode a decompressed `SaveSingleton` Lua string into a [`SaveState`].
///
/// Errors only if `lua` contains no recognizable `SaveSingleton` table keys.
pub fn parse_save_state(lua: &str) -> Result<SaveState, String> {
    // Sanity: must look like the serialized SaveSingleton table.
    let has_any = ["tLayerData", "tCulledBindings", "tFlowData", "nTimeElapsed"]
        .iter()
        .any(|k| lua.contains(&format!("[\"{k}\"]")));
    if !has_any {
        return Err("not a SaveSingleton Lua table (no known keys found)".into());
    }

    let flow_chain = table_body(lua, "tCulledBindings")
        .map(|b| parse_table(b).into_iter().map(|(_, v)| unquote(&v)).collect())
        .unwrap_or_default();

    let mut active_missions = Vec::new();
    if let Some(am) = table_body(lua, "tActiveMissions") {
        for (id, val) in parse_table(am) {
            // `val` is the mission's `{ … }` table; strip the outer braces.
            let body = val.trim();
            let body = body
                .strip_prefix('{')
                .and_then(|b| b.strip_suffix('}'))
                .unwrap_or(body);
            let state = scalar_value(body, "nState")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let targets_complete =
                scalar_value(body, "_nTargetsComplete").and_then(|s| s.parse::<f64>().ok());
            let collected = table_body(body, "tCollected")
                .map(extract_guids)
                .unwrap_or_default();
            active_missions.push(ActiveMission {
                id,
                state,
                targets_complete,
                collected,
            });
        }
    }

    let completed_flow = table_body(lua, "tMyFlowData")
        .map(|b| {
            parse_table(b)
                .into_iter()
                .filter_map(|(k, v)| v.parse::<f64>().ok().map(|n| (k, n)))
                .collect()
        })
        .unwrap_or_default();

    let layers = table_body(lua, "tLayerData")
        .map(|b| parse_table(b).into_iter().map(|(_, v)| unquote(&v)).collect())
        .unwrap_or_default();

    let time_elapsed_secs = scalar_value(lua, "nTimeElapsed")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let equipped_support = table_body(lua, "vEquippedSupport")
        .map(|b| parse_table(b).into_iter().map(|(_, v)| unquote(&v)).collect())
        .unwrap_or_default();

    // `tStarterData` = { ["PmcBoss"] = {..}, ["MecPmcBoss"] = {..}, … } — its keys are the unlocked
    // starters (recruits appear only once their mission unlocks them).
    let unlocked_starters = table_body(lua, "tStarterData")
        .map(|b| parse_table(b).into_iter().map(|(k, _)| k).collect())
        .unwrap_or_default();

    let (transit_enabled, transit_zones) =
        table_body(lua, "tTransitData").map(parse_transit).unwrap_or((false, Vec::new()));

    Ok(SaveState {
        flow_chain,
        active_missions,
        completed_flow,
        layers,
        time_elapsed_secs,
        equipped_support,
        unlocked_starters,
        transit_enabled,
        transit_zones,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The eight retail saves these tests read, **vendored in-tree** at `fixtures/saves`.
    ///
    /// They used to be read from a hardcoded `C:/Users/Shadow/Documents/...`, so on any other machine
    /// these tests failed for a missing file rather than testing the format. They are committed now
    /// (128 KiB total) and every test below runs unconditionally, everywhere — see
    /// `fixtures/saves/README.md`.
    ///
    /// The set is deliberately varied: three different heroes, upgrade tiers 0 and 3, flow chains from
    /// 2 to 63 entries, and one non-ASCII slot name that exercises the UTF-16LE `save_name` path.
    use crate::game_paths::SAVE_FIXTURES as ALL_SAVES;

    /// The vendored fixtures directory. Never `Option` — these files are committed, so their absence is
    /// a broken checkout and must fail loudly rather than skip into a false green.
    fn save_dir() -> std::path::PathBuf {
        let d = crate::game_paths::save_fixtures();
        assert!(
            d.is_dir(),
            "vendored save fixtures missing at {} — they are committed to the repo; a checkout \
             without them is broken, not a reason to skip",
            d.display()
        );
        d
    }

    fn load_from(dir: &Path, name: &str) -> Vec<u8> {
        std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
    }

    /// `tTransitData` decodes out of the REAL saves, and the three progression points disagree in
    /// exactly the way the game's own logs say they should.
    ///
    /// The zone set is the cross-check that matters: every save authors the same 23 zones
    /// (`1..8, 12, 15..18, 20..25, 27..30`), which is precisely what the `LandingZone` COMP
    /// enumerates and what a live retail run reports. Three independent sources — world data, save
    /// data, and a hooked-log capture of retail play — and they agree.
    #[test]
    fn transit_data_decodes_from_the_retail_saves() {
        const AUTHORED_ZONES: [u32; 23] = [
            1, 2, 3, 4, 5, 6, 7, 8, 12, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 27, 28, 29, 30,
        ];
        let dir = save_dir();
        let state_of = |name: &str| -> SaveState {
            let p = parse(&load_from(&dir, name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            let lua = p.decompress_lua().unwrap_or_else(|e| panic!("{name}: {e}"));
            parse_save_state(&String::from_utf8_lossy(&lua))
                .unwrap_or_else(|e| panic!("{name}: {e}"))
        };

        // Every fixture carries the full authored zone set — the table is written whole, not sparsely.
        for name in ALL_SAVES {
            let s = state_of(name);
            let zones: Vec<u32> = s.transit_zones.iter().map(|z| z.zone).collect();
            assert_eq!(
                zones, AUTHORED_ZONES,
                "{name}: tTransitData must carry all 23 authored landing zones"
            );
        }

        // A 0%-completion save: transit not yet unlocked, so every zone is off and unaffiliated.
        // Matches the capture's `SetSystemEnabled( false, nil, nil  @mrxtransit:418`.
        let chris = state_of("Chris Jacobs_6A499ED6.profile");
        assert!(!chris.transit_enabled, "a pre-PMC-takeover save has transit switched off");
        assert!(
            chris.transit_zones.iter().all(|z| !z.enabled && z.faction.is_none()),
            "no zone is enabled or affiliated before the takeover"
        );

        // A mid-progression save: transit on, but only some zones taken. Proves the parse is reading
        // per-zone state rather than filling a blanket value.
        let mid = state_of("Mattias Nilsson_63430745.profile");
        assert!(mid.transit_enabled, "a late save has transit switched on");
        let taken = mid.transit_zones.iter().filter(|z| z.faction.is_some()).count();
        assert!(
            (1..AUTHORED_ZONES.len()).contains(&taken),
            "a mid save has SOME zones taken, not none and not all; got {taken}"
        );

        // **The end-game save, checked pair-for-pair against a live capture of the game loading it.**
        //
        // `game-files/pmc_blackbox-mattias-save-end-game.log` records `MrxTransit.LoadSingleton`
        // replaying this exact blob, one line per affiliated zone:
        //
        //     [lua] Landing zone 1 affiliated with Pmc (nil)   @mrxtransit:669
        //
        // Those 22 lines and the 22 affiliated zones parsed out of this file agree completely. A save
        // file we decode and the shipped game reading the same save, cross-checked.
        const CAPTURED: [(u32, &str); 22] = [
            (1, "Pmc"), (2, "Oil"), (3, "Oil"), (4, "Gur"), (5, "Gur"), (7, "All"), (8, "Pir"),
            (12, "Chi"), (15, "Oil"), (16, "Oil"), (17, "Gur"), (18, "Gur"), (20, "All"),
            (21, "All"), (22, "All"), (23, "Chi"), (24, "Chi"), (25, "Chi"), (27, "Pir"),
            (28, "Pir"), (29, "Oil"), (30, "Chi"),
        ];
        let end = state_of("Mattias Nilsson_6A0E523C.profile");
        assert!(end.transit_enabled, "the end-game save has transit switched on");
        let parsed: Vec<(u32, &str)> = end
            .transit_zones
            .iter()
            .filter_map(|z| z.faction.as_deref().map(|f| (z.zone, f)))
            .collect();
        assert_eq!(
            parsed,
            CAPTURED.to_vec(),
            "the parsed affiliations must match what the live capture logged, zone for zone"
        );
        assert!(
            end.transit_zones.iter().filter(|z| z.faction.is_some()).all(|z| z.enabled),
            "an affiliated zone is an enabled zone"
        );

        // Zone 6 is the `bFake` pad (`mrxtransit.lua` Reset): present in every table, never affiliated
        // in any of them — which is why the capture logs 22 zones where the world data authors 23.
        for (name, s) in [("mid", &mid), ("end", &end)] {
            let fake = s.transit_zones.iter().find(|z| z.zone == 6).expect("zone 6 present");
            assert_eq!(fake.faction, None, "{name}: the zone-6 fake pad is never affiliated");
        }
    }

    /// Every file named in [`ALL_SAVES`] is really present, and nothing in the fixtures directory is
    /// left out of the list. Without this a fixture could be deleted, or added and never exercised,
    /// and the suite would stay green — the exact failure mode that let the old hardcoded path hide.
    #[test]
    fn the_fixture_set_is_complete_and_fully_covered() {
        let dir = save_dir();
        for name in ALL_SAVES {
            assert!(dir.join(name).is_file(), "fixture {name} is missing from {}", dir.display());
        }
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .expect("fixtures dir readable")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".profile"))
            .collect();
        on_disk.sort();
        let mut listed: Vec<String> = ALL_SAVES.iter().map(|s| s.to_string()).collect();
        listed.sort();
        assert_eq!(on_disk, listed, "every vendored .profile must be listed in ALL_SAVES");
    }

    #[test]
    fn all_six_parse_with_invariants() {
        let dir = save_dir();
        for name in ALL_SAVES {
            let bytes = load_from(&dir, name);
            assert_eq!(bytes.len(), PROFILE_SIZE, "{name} size");
            let p = parse(&bytes).unwrap_or_else(|e| panic!("parse {name}: {e}"));

            // Structural invariants that hold across every retail save.
            assert_eq!(p.version, 4, "{name} version");
            assert_eq!(p.data_size, (PROFILE_SIZE as u32) - 4, "{name} data_size");
            assert_eq!(p.unknown_0x0c, 3, "{name} unk0x0c const");
            assert_eq!(p.unknown_0x10, 0, "{name} unk0x10 const");
            assert_eq!(p.unknown_0x20, 0, "{name} unk0x20 const");

            // Contract id is a printable-ASCII mission tag.
            assert!(!p.active_contract.is_empty(), "{name} contract present");
            assert!(
                p.active_contract.bytes().all(|b| b.is_ascii_graphic()),
                "{name} contract ascii: {:?}",
                p.active_contract
            );

            // Payload decompresses to a non-trivial Lua blob.
            let lua = p.decompress_lua().unwrap_or_else(|e| panic!("lua {name}: {e}"));
            assert!(lua.len() > 10_000, "{name} lua len {}", lua.len());
        }
    }

    #[test]
    fn target_file_contract_is_pmccon001() {
        let dir = save_dir();
        let bytes = load_from(&dir, "auto_6A447BF8.profile");
        let p = parse(&bytes).unwrap();
        assert_eq!(p.active_contract(), "PmcCon001");
        assert_eq!(p.checksum, 0xCA2F_06BE); // this file's stored hash
        assert_eq!(p.save_name(), "auto_6A447BF8");
        assert_eq!(p.timestamp, 0x6A45_586A);
    }

    /// The fixture set spans the game's whole progression, and the parser tracks it monotonically.
    ///
    /// This is what makes the eight files worth keeping as a *set* rather than one sample: the flow
    /// chain grows from 2 entries to 63, so a parser that mis-sized the chain, truncated it, or read a
    /// fixed count would pass on one save and fail here.
    ///
    /// `Chris Jacobs_6A499ED6` is the earliest state we have — **before the player owns the PMC**.
    /// They have beaten the intro and not yet reached the open world, so the chain is exactly
    /// `["Start", "VzaCon001"]`: `VzaCon001` is the intro contract itself. That makes it the fixture
    /// that pins "no progression yet" as distinct from "one contract done", and it is the save state
    /// the boot path's `VzaCon001` asset gate actually corresponds to.
    #[test]
    fn the_set_spans_the_progression_and_flow_chains_grow_with_it() {
        let dir = save_dir();
        let chain = |name: &str| -> Vec<String> {
            parse(&load_from(&dir, name)).unwrap().save_state().unwrap().flow_chain
        };

        // Pre-PMC-ownership: the intro is done and nothing else. Asserted exactly — a chain that grew
        // by even one entry would mean we decoded a later save's state into the earliest one.
        let intro = chain("Chris Jacobs_6A499ED6.profile");
        assert_eq!(intro, ["Start", "VzaCon001"], "the pre-open-world save has only the intro");

        // Then the ladder. Each rung must be strictly longer than the last.
        let rungs = [
            ("Chris Jacobs_6A499ED6.profile", "intro done, PMC not yet owned"),
            ("auto_6A0BE454.profile", "first PMC contract"),
            ("auto_634304EA.profile", "mid-game"),
            ("Mattias Nilsson_6A0E523C.profile", "endgame"),
        ];
        let mut prev = 0usize;
        for (name, stage) in rungs {
            let n = chain(name).len();
            assert!(n > prev, "{name} ({stage}): flow chain {n} should exceed the previous rung {prev}");
            prev = n;
        }

        // Every rung's chain starts from the same root contract — progression appends, it does not
        // rewrite history.
        for (name, _) in rungs {
            assert!(chain(name).contains(&"Start".to_string()), "{name} retains the Start entry");
        }
    }

    /// All three playable heroes appear in the set, so `character_index` is exercised across its whole
    /// meaningful range rather than only the Mattias value.
    #[test]
    fn every_hero_is_represented() {
        let dir = save_dir();
        let heroes: std::collections::BTreeSet<u8> = ALL_SAVES
            .iter()
            .map(|n| parse(&load_from(&dir, n)).unwrap().character_index)
            .collect();
        assert_eq!(
            heroes,
            [1u8, 2, 3].into_iter().collect(),
            "1 = Mattias, 2 = Chris, 3 = Jen — all three must be covered"
        );
        assert_eq!(
            parse(&load_from(&dir, "Chris Jacobs_6A499ED6.profile")).unwrap().character_index,
            2,
            "the Chris slot really stores the Chris hero code"
        );
    }

    /// Every fixture's stored `ProfileHash` validates. This is the read-side half of the CRC-32/BZIP2
    /// derivation that `save_write` claims — a claim that, until these files were vendored, had never
    /// executed outside one developer's machine.
    #[test]
    fn every_fixture_has_a_valid_stored_hash() {
        let dir = save_dir();
        for name in ALL_SAVES {
            let p = parse(&load_from(&dir, name)).unwrap();
            assert!(p.hash_ok(), "{name}: stored ProfileHash does not match CRC-32/BZIP2 over [4:]");
        }
    }

    #[test]
    fn contracts_match_expected() {
        let dir = save_dir();
        let cases = [
            ("Mattias Nilsson_63430745.profile", "OilCon001"),
            ("Mattias Nilsson_6A0E523C.profile", "PmcJob001"),
            ("Chris Jacobs_6A499ED6.profile", "VzaCon001"),
            ("_______ ________48EFABFB.profile", "PmcJob001"),
            ("auto_634304EA.profile", "OilCon003"),
            ("auto_6A0BE454.profile", "PmcCon001"),
            ("auto_6A447BF8.profile", "PmcCon001"),
            ("auto_6A499D08.profile", "VzaCon001"),
        ];
        for (name, contract) in cases {
            let p = parse(&load_from(&dir, name)).unwrap();
            assert_eq!(p.active_contract(), contract, "{name}");
        }
    }

    #[test]
    fn all_six_decode_save_state() {
        let dir = save_dir();
        for name in ALL_SAVES {
            let p = parse(&load_from(&dir, name)).unwrap();
            let st = p.save_state().unwrap_or_else(|e| panic!("save_state {name}: {e}"));

            // Every retail save carries a non-empty world-overlay set, and every
            // entry is a vz_state_* layer (world_streaming_spec §5 overlays).
            assert!(!st.layers.is_empty(), "{name} has layers");
            assert!(
                st.layers.iter().all(|l| l.starts_with("vz_state_")),
                "{name} all layers vz_state_*"
            );
            // Flow chain always begins the mission-flow FSM.
            assert!(!st.flow_chain.is_empty(), "{name} flow_chain non-empty");
            // Playtime is present and non-negative.
            assert!(st.time_elapsed_secs >= 0.0, "{name} time_elapsed");
        }
    }

    #[test]
    fn target_file_save_state_decoded() {
        let dir = save_dir();
        let p = parse(&load_from(&dir, "auto_6A447BF8.profile")).unwrap();
        let st = p.save_state().unwrap();

        // Mission-flow binding chain, in order.
        assert_eq!(st.flow_chain, ["Start", "VzaCon001", "PmcCon001"]);
        assert!(st.flow_chain.contains(&"PmcCon001".to_string()));

        // 253 world-overlay layers, all vz_state_*.
        assert_eq!(st.layers.len(), 253, "layer count");
        assert!(st.layers.iter().all(|l| l.starts_with("vz_state_")));
        assert_eq!(st.layers[0], "vz_state_mer_big_lineregion");

        // Playtime seconds (matches the raw header count).
        assert_eq!(st.time_elapsed_secs, 964.0);
        assert_eq!(st.time_elapsed_secs as u32, p.play_time_seconds);

        // Active missions incl. PmcJob001 with one collected guid.
        let ids: Vec<&str> = st.active_missions.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"PmcJob001"), "active missions: {ids:?}");
        let pmcjob = st
            .active_missions
            .iter()
            .find(|m| m.id == "PmcJob001")
            .unwrap();
        assert_eq!(pmcjob.state, 1.0);
        assert_eq!(pmcjob.targets_complete, Some(1.0));
        assert_eq!(pmcjob.collected, vec![0x0013E2C6]);
        assert_eq!(st.collected_count(), 1);

        // Completed-flow flags.
        assert_eq!(st.completed_flow.get("PmcCon001"), Some(&1.0));
        assert_eq!(st.completed_flow.get("VzaCon001"), Some(&1.0));

        // This save has no equipped support.
        assert!(st.equipped_support.is_empty());
    }

    #[test]
    fn layer_sets_differ_across_files() {
        let dir = save_dir();
        // Cross-file: the overlay sets are genuinely per-save (not a shared
        // constant), and every entry everywhere is a vz_state_* layer.
        let a = parse(&load_from(&dir, "auto_6A447BF8.profile"))
            .unwrap()
            .save_state()
            .unwrap()
            .layers;
        let b = parse(&load_from(&dir, "Mattias Nilsson_6A0E523C.profile"))
            .unwrap()
            .save_state()
            .unwrap()
            .layers;
        assert_ne!(a, b, "layer lists must differ across saves");
        assert_ne!(a.len(), b.len(), "layer counts differ (253 vs 238)");
        for set in [&a, &b] {
            assert!(set.iter().all(|l| l.starts_with("vz_state_")));
        }
    }

    #[test]
    fn equipped_support_harvested_when_present() {
        let dir = save_dir();
        // The high-progress save equips support/vehicle tokens.
        let st = parse(&load_from(&dir, "Mattias Nilsson_6A0E523C.profile"))
            .unwrap()
            .save_state()
            .unwrap();
        assert!(!st.equipped_support.is_empty());
        assert_eq!(st.equipped_support[0], "[vehicle.wz10]");
        // Later-game save advances many flow flags.
        assert!(st.completed_flow.len() > 100, "flow flags: {}", st.completed_flow.len());
    }

    #[test]
    fn rejects_non_savestate_lua() {
        assert!(parse_save_state("print('hello')").is_err());
        assert!(parse_save_state("").is_err());
    }

    #[test]
    fn rejects_bad_input() {
        let dir = save_dir();
        assert!(parse(&[0u8; 16]).is_err(), "short buffer");
        let mut b = load_from(&dir, "auto_6A447BF8.profile");
        b[OFF_VERSION] = 9;
        assert!(parse(&b).is_err(), "bad version");
    }
}
