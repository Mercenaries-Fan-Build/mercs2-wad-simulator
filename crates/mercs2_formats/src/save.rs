//! Mercenaries 2 (PC) `.profile` save-game parser.
//!
//! A `.profile` is a fixed-size **13,404-byte** file: a packed binary header
//! followed by a **zlib** stream (starting at `0x468`) that decompresses to the
//! game's Lua `SaveSingleton` state (cash / fuel / faction / mission tables).
//!
//! This module reverses the header fields that are grounded either in a
//! byte-for-byte diff of the six retail saves under
//! `My Games/Mercenaries 2/SaveGames/*.profile` or in the engine save symbols
//! (`docs/mercs2-pdb-analysis/game-systems.md`: `ProfileHash`, `SetLuaSaveVersion`,
//! `SetProfileCostume`, `saveProfile`, ...). Fields whose *meaning* is not
//! grounded are named `unknown_<offset>` or flagged `INFERRED`.
//!
//! There is **no magic constant** at `0x00`: that u32 varies across every save
//! and is a per-file integrity **checksum/hash** (`ProfileHash`). The stable
//! structural sentinels are `version == 4` (`@0x04`), `data_size == len-4`
//! (`@0x08`), and the zlib header byte `0x78` at `0x468`. See `SAVE_FORMAT.md`.

use std::collections::BTreeMap;
use std::io::Read;

/// Fixed on-disk size of every retail `.profile` (bytes).
pub const PROFILE_SIZE: usize = 13_404;
/// Save-format version this parser understands (`SetLuaSaveVersion`).
pub const VERSION: u32 = 4;
/// Byte offset of the zlib-compressed Lua payload.
pub const ZLIB_OFFSET: usize = 0x468;

// --- header field offsets (FACT: located by cross-file diff) ---
const OFF_CHECKSUM: usize = 0x00; // u32  per-file hash (ProfileHash), opaque
const OFF_VERSION: usize = 0x04; // u32  == VERSION
const OFF_DATA_SIZE: usize = 0x08; // u32  == file_len - 4 (bytes the checksum covers)
const OFF_UNK_0C: usize = 0x0C; // u32  constant 0x3 across all saves
const OFF_UNK_10: usize = 0x10; // u32  constant 0x0
const OFF_PLAY_TIME: usize = 0x14; // u32  play-time seconds (INFERRED)
const OFF_CASH: usize = 0x18; // u32  PMC cash (INFERRED)
const OFF_FUEL: usize = 0x1C; // u32  PMC fuel (INFERRED)
const OFF_UNK_20: usize = 0x20; // u32  constant 0x0
const OFF_TIMESTAMP: usize = 0x24; // u32  unix timestamp of the save
const OFF_CONTRACT: usize = 0x2C; // [16] NUL-padded ASCII active contract id (FACT)
const CONTRACT_LEN: usize = 16;
const OFF_FLAGS_4C: usize = 0x4C; // u32  bitfield (INFERRED)
const OFF_SAVE_NAME: usize = 0x20A; // UTF-16LE NUL-terminated slot name (FACT)
const OFF_FUEL_CAP: usize = 0x2F8; // u16  fuel capacity? tracks fuel (INFERRED)
const OFF_COSTUME: usize = 0x24A; // u8   costume/character index (INFERRED)

/// Decoded Mercenaries 2 `.profile` save.
///
/// Raw header fields are exposed as public members. Grounding for each is noted
/// in the module docs and `SAVE_FORMAT.md` (FACT vs INFERRED).
#[derive(Debug, Clone)]
pub struct Profile {
    /// `@0x00` u32 — per-file integrity checksum (`ProfileHash`). Algorithm not
    /// yet reversed; stored verbatim, **not** validated. Varies every save.
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
    /// `@0x4C` u32 — flag bitfield (changes with progress). INFERRED.
    pub flags_0x4c: u32,
    /// `@0x24A` u8 — costume / character index (`SetProfileCostume`). INFERRED.
    pub costume_index: u8,
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
        costume_index: bytes[OFF_COSTUME],
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

    Ok(SaveState {
        flow_chain,
        active_missions,
        completed_flow,
        layers,
        time_elapsed_secs,
        equipped_support,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const SAVE_DIR: &str = r"C:/Users/Shadow/Documents/My Games/Mercenaries 2/SaveGames";

    const ALL_SAVES: &[&str] = &[
        "Mattias Nilsson_63430745.profile",
        "Mattias Nilsson_6A0E523C.profile",
        "_______ ________48EFABFB.profile",
        "auto_634304EA.profile",
        "auto_6A0BE454.profile",
        "auto_6A447BF8.profile",
    ];

    fn load(name: &str) -> Vec<u8> {
        std::fs::read(Path::new(SAVE_DIR).join(name))
            .unwrap_or_else(|e| panic!("read {name}: {e}"))
    }

    #[test]
    fn all_six_parse_with_invariants() {
        for name in ALL_SAVES {
            let bytes = load(name);
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
        let bytes = load("auto_6A447BF8.profile");
        let p = parse(&bytes).unwrap();
        assert_eq!(p.active_contract(), "PmcCon001");
        assert_eq!(p.checksum, 0xCA2F_06BE); // this file's stored hash
        assert_eq!(p.save_name(), "auto_6A447BF8");
        assert_eq!(p.timestamp, 0x6A45_586A);
    }

    #[test]
    fn contracts_match_expected() {
        let cases = [
            ("Mattias Nilsson_63430745.profile", "OilCon001"),
            ("Mattias Nilsson_6A0E523C.profile", "PmcJob001"),
            ("_______ ________48EFABFB.profile", "PmcJob001"),
            ("auto_634304EA.profile", "OilCon003"),
            ("auto_6A0BE454.profile", "PmcCon001"),
            ("auto_6A447BF8.profile", "PmcCon001"),
        ];
        for (name, contract) in cases {
            let p = parse(&load(name)).unwrap();
            assert_eq!(p.active_contract(), contract, "{name}");
        }
    }

    #[test]
    fn all_six_decode_save_state() {
        for name in ALL_SAVES {
            let p = parse(&load(name)).unwrap();
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
        let p = parse(&load("auto_6A447BF8.profile")).unwrap();
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
        // Cross-file: the overlay sets are genuinely per-save (not a shared
        // constant), and every entry everywhere is a vz_state_* layer.
        let a = parse(&load("auto_6A447BF8.profile"))
            .unwrap()
            .save_state()
            .unwrap()
            .layers;
        let b = parse(&load("Mattias Nilsson_6A0E523C.profile"))
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
        // The high-progress save equips support/vehicle tokens.
        let st = parse(&load("Mattias Nilsson_6A0E523C.profile"))
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
        assert!(parse(&[0u8; 16]).is_err(), "short buffer");
        let mut b = load("auto_6A447BF8.profile");
        b[OFF_VERSION] = 9;
        assert!(parse(&b).is_err(), "bad version");
    }
}
