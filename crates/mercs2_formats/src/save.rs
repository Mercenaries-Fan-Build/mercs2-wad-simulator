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
    fn rejects_bad_input() {
        assert!(parse(&[0u8; 16]).is_err(), "short buffer");
        let mut b = load("auto_6A447BF8.profile");
        b[OFF_VERSION] = 9;
        assert!(parse(&b).is_err(), "bad version");
    }
}
