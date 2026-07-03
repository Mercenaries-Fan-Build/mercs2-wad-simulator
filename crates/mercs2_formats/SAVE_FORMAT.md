# Mercenaries 2 (PC) `.profile` save format

Parser: [`src/save.rs`](src/save.rs) — `mercs2_formats::save::parse(&[u8]) -> Result<Profile, String>`.

A `.profile` is a **fixed 13,404-byte** file: a packed binary header, then a
**zlib** stream at `0x468` that inflates to the game's Lua `SaveSingleton` state.

## Evidence

- **Diff** of the six retail saves in `My Games/Mercenaries 2/SaveGames/*.profile`.
  Classifies every byte as CONSTANT (structure) vs VARYING (mutable state).
- **Corpus / symbols**: `docs/mercs2-pdb-analysis/game-systems.md` (`ProfileHash`,
  `SetLuaSaveVersion`, `SetProfileCostume/Upgrade/Character`, `saveProfile`),
  `docs/format_reference.md`, and the legacy `tools/savefile_parser.py`.

Diff result: `const=4363  vary=9041` bytes. Varying regions (start–end inclusive):

```
0x0000-0x0003  checksum
0x0014-0x001D  play_time / cash / fuel
0x0024-0x0027  timestamp
0x002C-0x0034  active contract id (ASCII)
0x004D / 0x004F-0x0051   flags
0x00AC         (1 byte, unknown)
0x0214-0x0222  save-name UTF-16 chars
0x024A         costume index
0x02F8-0x02F9  fuel capacity
0x0462-0x0467  (pre-zlib, unknown — 2 u16, not lengths)
0x046B-0x2790  zlib deflate payload
```

## Header layout

| Offset | Size | Field | Status | Notes |
|--------|------|-------|--------|-------|
| `0x00` | u32 | `checksum` | FACT (opaque) | **Not a magic** — varies every save. Per-file `ProfileHash`. Algorithm not reversed (not crc32/fnv1a/sum/xor/adler over `[4:]`,`[8:]`,`[4:0x468]`,`[0x468:]`). Stored, **not validated**. |
| `0x04` | u32 | `version` | FACT | Always `4` (`SetLuaSaveVersion`). Validated. |
| `0x08` | u32 | `data_size` | FACT | `= file_len - 4` = `0x3458` (13400) — the range the checksum covers. Validated. |
| `0x0C` | u32 | `unknown_0x0c` | FACT (const) | Constant `3` across all saves. Meaning unknown. |
| `0x10` | u32 | `unknown_0x10` | FACT (const) | Constant `0`. |
| `0x14` | u32 | `play_time_seconds` | INFERRED | Small monotonic seconds (0x3C4 … 0x3C122). |
| `0x18` | u32 | `cash` | INFERRED | 50000 … ~342M, within the 1B economy cap (memory: money datatype). |
| `0x1C` | u32 | `fuel` | INFERRED | 0 … 5485; tracks `fuel_capacity`. |
| `0x20` | u32 | `unknown_0x20` | FACT (const) | Constant `0`. |
| `0x24` | u32 | `timestamp` | FACT | Unix timestamp of the save (redacted 2008 devsave = `0x48F2C77C`; newest = `0x6A45586A`, 2026). |
| `0x2C` | 16B | `active_contract` | FACT | NUL-padded ASCII mission/contract id: `PmcCon001`, `OilCon001`, `OilCon003`, `PmcJob001`. Matches `PmcCon031_x3` placement-binding naming. |
| `0x4C` | u32 | `flags_0x4c` | INFERRED | Bitfield that changes with progress (`0x100`/`0x300`/`0x3000100`). |
| `0x20A` | UTF-16LE | `save_name` | FACT | NUL-terminated slot label, e.g. `auto_634304EA`. This is the **autosave/slot name**, *not* the player display name (even `Mattias Nilsson_*.profile` stores `auto_*` here). |
| `0x24A` | u8 | `costume_index` | INFERRED | Costume/character index (`SetProfileCostume`); values 1 and 5. |
| `0x2F8` | u16 | `fuel_capacity` | INFERRED | Max fuel; ≥ `fuel` (700/5500/300). |
| `0x468` | — | zlib Lua payload | FACT | Header byte `0x78 0xDA`. Inflates to 24.8K–54K of Lua `SaveSingleton` text (mission/faction/economy tables). Rest of file is fixed-size padding the deflate stream ignores. |

## Per-file summary (all six)

| File | contract | costume | timestamp | cash | fuel | save_name |
|------|----------|---------|-----------|------|------|-----------|
| Mattias Nilsson_63430745 | OilCon001 | 1 | 0x634F59AA | 2,545,064 | 700 | auto_634304EA |
| Mattias Nilsson_6A0E523C | PmcJob001 | 5 | 0x6A0E523C | 342,479,104 | 5485 | auto_48E12F36 |
| _______ ________48EFABFB | PmcJob001 | 5 | 0x48F2C77C | 342,479,104 | 5485 | auto_48E12F36 |
| auto_634304EA | OilCon003 | 1 | 0x634F586D | 2,765,064 | 700 | auto_634304EA |
| auto_6A0BE454 | PmcCon001 | 1 | 0x6A0CFE39 | 50,000 | 0 | auto_6A0BE454 |
| auto_6A447BF8 | PmcCon001 | 1 | 0x6A45586A | 200,000 | 25 | auto_6A447BF8 |

## Not yet reversed

- `checksum` algorithm (integrity hash at `0x00`).
- `0x462`–`0x467`: two u16 immediately before the zlib stream; not the
  compressed/uncompressed lengths (verified) — purpose unknown.
- Byte `0xAC`, and the `0x4C`/`0x4F–0x51` flag bit meanings.
- The Lua payload is exposed raw (`Profile::decompress_lua()`); its internal
  table structure is deliberately not parsed here (legacy regex harvest lives in
  `tools/savefile_parser.py`).
