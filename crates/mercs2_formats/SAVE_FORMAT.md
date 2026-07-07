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
| `0x00` | u32 | `checksum` | **FACT (algorithm DERIVED)** | **Not a magic** — varies every save. Per-file `ProfileHash` = **CRC-32/BZIP2** over `[4:]` (poly `0x04C11DB7`, init/xorout `0xFFFFFFFF`, **non-reflected**). Verified byte-exact against all 8 retail saves. Validate via `Profile::hash_ok()`; the writer (`save_write.rs`) stamps a real one. The earlier "not crc32" ruling tested only the *reflected* ISO-HDLC model; the non-reflected (MSB-first, BE) variant matches — consistent with the engine's `ntohl` BE SaveData blob. |
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
| `0x4C` | u32 | `flags_0x4c` | superseded | NOT one bitfield — per-byte fields (raw dword kept for reference). Byte `0x4C` = 0 in all saves (meaning unknown). |
| `0x4D` | u8 | `character_index` | **FACT (values) + verified offset** | **HERO** (`Get/SetProfileCharacter` = runtime profile object `+0x61`, decompiled `FUN_005df790/7d0`). Values engine-coded in `FUN_00634810`: 1 → `SHELL.SelectCharacter.MattiasNilsson`, 2 → `.ChrisJacobs`, 3 → `.JenniferMui`, else "Player". Offset verified by save diff (Jen save = 3; the only header byte separating parallel fresh saves). |
| `0x4E` | u8 | — | unknown | 0 in all saves. Candidate for the wardrobe COSTUME byte (object `+0x63`, `Get/SetProfileCostume` `FUN_005df8e0/920`) — every observed save has costume 0 (wardrobe never used), so it cannot be located from this corpus. |
| `0x4F` | u8 | `upgrade_index` | **verified semantics** | Hero UPGRADE tier 0..3 (`Get/SetProfileUpgrade` = object `+0x62`, `FUN_005df830/870`). Drives the spawn TEMPLATE (`mrxplayer.lua:167-168`: `_tCharacterMap.templates[iUpgrade] or base`) — **the hero's look progresses with tier**. User-verified: tier 0 saves show the default skin, the tier-3 endgame save shows Mattias's "MetalHead" (v3) look. |
| `0x20A` | UTF-16LE | `save_name` | FACT | NUL-terminated slot label, e.g. `auto_634304EA`. This is the **autosave/slot name**, *not* the player display name (even `Mattias Nilsson_*.profile` stores `auto_*` here). |
| `0x24A` | u8 | `unlocked_costumes` | INFERRED | UNLOCKED-outfit count (feeds `Player.GetAvailableCostumes`, the wardrobe menu gate): 1 on fresh/mid saves, 5 (= all five base outfits) on the completed saves. NOT the selected outfit — user ground truth disproved that twice (looks differ between saves sharing these bytes; the look is `0x4F` upgrade-tier driven). |
| `0x24B` | u8 | `unknown_0x24b` | unknown | `1` in every observed save. NOT character/costume/upgrade. |
| `0x2F8` | u16 | `fuel_capacity` | INFERRED | Max fuel; ≥ `fuel` (700/5500/300). |
| `0x468` | — | zlib Lua payload | FACT | Header byte `0x78 0xDA`. Inflates to 24.8K–54K of Lua `SaveSingleton` text (mission/faction/economy tables). Rest of file is fixed-size padding the deflate stream ignores. |

## Per-file summary (all six)

| File | contract | hero @0x4D | upgrade @0x4F | unlocked @0x24A | retail look (user-verified) | timestamp | cash | fuel | save_name |
|------|----------|-----------|---------------|-----------------|------------------------------|-----------|------|------|-----------|
| Mattias Nilsson_63430745 | OilCon001 | 1 mattias | 0 | 1 | default skin ✓ | 0x634F59AA | 2,545,064 | 700 | auto_634304EA |
| Mattias Nilsson_6A0E523C | PmcJob001 | 1 mattias | **3** | 5 | "MetalHead" v3 ✓ | 0x6A0E523C | 342,479,104 | 5485 | auto_48E12F36 |
| _______ ________48EFABFB | PmcJob001 | 1 mattias | **3** | 5 | "MetalHead" v3 | 0x48F2C77C | 342,479,104 | 5485 | auto_48E12F36 |
| auto_634304EA | OilCon003 | 1 mattias | 0 | 1 | default skin | 0x634F586D | 2,765,064 | 700 | auto_634304EA |
| auto_6A0BE454 | PmcCon001 | 1 mattias | 0 | 1 | default skin | 0x6A0CFE39 | 50,000 | 0 | auto_6A0BE454 |
| auto_6A447BF8 | PmcCon001 | **3 jen** | 0 | 1 | Jen ✓ | 0x6A45586A | 200,000 | 25 | auto_6A447BF8 |

**Runtime profile object** (pointer at `0x01176054`; the singleton that also carries cash/fuel):
`+0x61` character, `+0x62` upgrade, `+0x63` costume — from the decompiled Lua binders
`FUN_005df790/7d0/830/870/8e0/920` (created + exported via
`scripts/ghidra_scripts/DecompileProfileAccessors.java`; they were missing from the bulk export
because only the binding table at file `0x7992B0` references them).

## SaveSingleton Lua boot-state

`Profile::decompress_lua()` inflates to **readable Lua source** (24.8K–54K) — a
serialized `SaveSingleton` table, *not* bytecode. `parse_save_state(&str)` (and
the convenience `Profile::save_state()`) decode it into a `SaveState` so
`mercs2_game` can restore the real start-state. Extraction mirrors the legacy
regex harvest in `tools/savefile_parser.py` (`harvest_from_lua`); the Lua is
plain text, so a light brace/quote-aware table walker is used (no interpreter).

Observed top-level shape (verified on `auto_6A447BF8.profile`):

```lua
{
  ["vEquippedSupport"] = { [1]="[vehicle.wz10]", ... },   -- ordered tokens (may be empty)
  ["nTimeElapsed"]     = 964.000000,                      -- playtime seconds
  ["tFlowData"] = {                                       -- mission-flow container
    ["tCulledBindings"] = { [1]="Start", [2]="VzaCon001", [3]="PmcCon001" },
    ["tActiveMissions"] = { ["PmcJob001"] = { ["nState"]=1, ["_nTargetsComplete"]=1,
                                              ["tCollected"]={ Sys.StringToGuid('0x0013E2C6') } }, ... },
    ["tMyFlowData"]     = { ["PmcCon001"]=1, ["VzaCon001"]=1 },  -- completed-flow flags
  },
  ["tLayerData"] = { [1]="vz_state_mer_big_lineregion", ... },   -- 238–299 world overlays
}
```

Each key (`tCulledBindings` / `tActiveMissions` / `tMyFlowData` / `tLayerData` /
`nTimeElapsed` / `vEquippedSupport`) appears **exactly once** per file, so they
are located globally by name; per-mission `nState` / `_nTargetsComplete` /
`tCollected` are scoped to their own mission body (avoids colliding with the
same mission ids in `tMyFlowData`). Numbers are Lua floats → `f64`.

| `SaveState` field | Lua source | Status | Drives |
|-------------------|-----------|--------|--------|
| `flow_chain: Vec<String>` | `tFlowData.tCulledBindings` (ordered) | FACT | seeds mission-flow FSM binding chain |
| `active_missions: Vec<ActiveMission>` | `tFlowData.tActiveMissions` | FACT | restores in-progress contracts |
| `ActiveMission.state: f64` | `["nState"]` | FACT key / INFERRED code meaning | mission FSM state |
| `ActiveMission.targets_complete: Option<f64>` | `["_nTargetsComplete"]` | FACT | objective progress |
| `ActiveMission.collected: Vec<u32>` | `["tCollected"]` → `Sys.StringToGuid('0x…')` | FACT | collectible entity guids gathered |
| `completed_flow: BTreeMap<String,f64>` | `tFlowData.tMyFlowData` | FACT key / INFERRED per-value | seen/complete flow flags (`1`=seen, higher=later stage) |
| `layers: Vec<String>` | `tLayerData` (ordered) | FACT | `vz_state_*` world overlays to stream — **`world_streaming_spec.md §5`** (destruction/staging/faction/pristine) |
| `time_elapsed_secs: f64` | `nTimeElapsed` | FACT key / INFERRED unit | playtime (matches header `play_time_seconds`) |
| `equipped_support: Vec<String>` | `vEquippedSupport` (ordered) | FACT | equipped vehicle/support tokens (`[vehicle.*]`, `[support.*]`) |

Per-file decode (all six saves):

| File | flow_chain | active | layers | time (s) | equipped | contract |
|------|-----------:|-------:|-------:|---------:|---------:|----------|
| Mattias Nilsson_63430745 | 15 | — | 299 | 24266 | — | OilCon001 |
| Mattias Nilsson_6A0E523C | 63+ | many | 238 | 246050 | 3 | PmcJob001 |
| _______ ________48EFABFB | 63+ | many | 238 | 246031 | 3 | PmcJob001 |
| auto_634304EA | 15 | many | 295 | 24006 | — | OilCon003 |
| auto_6A0BE454 | 3 | few | 255 | 966 | — | PmcCon001 |
| auto_6A447BF8 | 3 | 3 | 253 | 964 | — | PmcCon001 |

The `tLayerData` sets are genuinely per-save (differ in both membership and
count) — every entry, in every file, begins with `vz_state_`.

## Writing `.profile` files

Writer: [`src/save_write.rs`](src/save_write.rs) — the inverse of `save::parse`.

- `save_write::profile_hash(&data[4:]) -> u32` — the derived CRC-32/BZIP2 `ProfileHash`.
- `save_write::write_profile(&Profile) -> Vec<u8>` — re-stamps every grounded header
  field over the profile's retained raw buffer, recomputes `data_size` + hash, and
  emits the 13,404-byte file. **Round-trips byte-exact**: `write_profile(&parse(x)) == x`.
  Mutate a public field then re-write to get a loadable save with a valid integrity hash.
- `save_write::set_lua_payload(&mut Profile, lua_source)` — re-deflates a
  `return { … }` Lua blob into the fixed payload region (`0x468`, capacity 12,276 B).

The `saveProfile` disk-write *body* is still absent from the Ghidra dump, but every
behaviour it must reproduce is now grounded (LE header, zlib@0x468, CRC-32/BZIP2 hash),
so `write_profile` is a faithful reimplementation, not a stand-in.

## Not yet reversed

- `0x462`–`0x467`: two u16 immediately before the zlib stream; not the
  compressed/uncompressed lengths (verified) — purpose unknown.
- Byte `0xAC`, and the `0x4C`/`0x4F–0x51` flag bit meanings.
- The Lua payload is exposed raw (`Profile::decompress_lua()`) and decoded into
  a `SaveState` (see "SaveSingleton Lua boot-state" above). Tables beyond the
  boot-state set (economy/faction/support catalogs, `_tRequirementsObtained`,
  `tLockedGates`, per-vehicle unlock tables) are present in the Lua but not yet
  decoded.
