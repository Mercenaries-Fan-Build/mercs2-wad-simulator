# vz.wad Model Hash -> Name Map

Resolves the model-asset 32-bit hashes listed by the reimplementation engine
(`mercs2_engine --list`) to human-readable names, so model listings are readable
(e.g. `0xA3C1FABC` -> `pmc_hum_mattias_v3`).

## Summary

| metric | value |
|--------|-------|
| model assets measured by engine `--list` | **1566** |
| humanoid-flagged models | **44** |
| hashes with a *real, name-like* candidate (all verified) | **112** |
| humanoid models resolved to a real name | **21 / 44** |
| hashes with only synthetic/block-path collision candidates (not real names) | 783 |
| hashes with *any* rainbow-table candidate | 895 |

Every name in the tables below was confirmed by recomputing
`pandemic_hash_m2(name) == hash` (all `verified = yes`). The hash function is the
FNV-1a variant in [`tools/wad_simulator/crates/mercs2_formats/src/hash.rs`](../../crates/mercs2_formats/src/hash.rs)
(seed `0x811C9DC5`, per-byte `| 0x20` case suppression, final `^0x2A` then `*prime`).

### Note on the 1771 vs 1566

The engine's header prints an estimate of **1771** model assets, but the
detailed listing enumerates **1566** unique model hashes. This map is built
against the 1566 enumerated hashes.

### Note on the "783 junk-only" hashes

783 additional model hashes DO collide with an entry in the rainbow table, but
only against synthetic candidates (block paths like `blocks\VZ\c30846_...block`,
or `..._0xNNNNNNNN` permutation strings), not a genuine asset name. Those are NOT
included below because the string is not a usable name. Real names for those
require additional candidate strings (see "Improving coverage").

## Rainbow table location & format

- **Path:** `tools/rainbow_table.json` (~72 MB)
- **Format:** JSON. Top-level keys `_meta`, `pandemic_hash_m2`, `pandemic_hash`.
  Each hash map is `{ "0xHHHHHHHH": [candidate_string, ...] }` (hex hash key ->
  list of input strings that hash to it).
- **Entry count:** `pandemic_hash_m2` = **733,594** unique hashes
  (candidate_count 64,216 base strings + console/DLC WAD token harvest).
- **Builder:** [`tools/build_rainbow_table.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/build_rainbow_table.py) (uses [`tools/pandemic_hash.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/pandemic_hash.py));
  the meta records additional console/DLC harvest merges on top of the base build.

## Humanoid models (21 resolved)

| hash | name | verified |
|------|------|----------|
| `0xDF3B2FD1` | `global_weapon_tripodmount` | yes |
| `0xEEAABF91` | `gr_hum_starter_1` | yes |
| `0x39F47E08` | `pmc_hum_chris` | yes |
| `0x77E59F1F` | `pmc_hum_chris_chickensuit` | yes |
| `0x98C5EE9B` | `pmc_hum_chris_v2` | yes |
| `0x76C37A7E` | `pmc_hum_chris_v3` | yes |
| `0x80D4D35D` | `pmc_hum_chris_v4` | yes |
| `0x15D89832` | `pmc_hum_jen` | yes |
| `0xD9588A21` | `pmc_hum_jen_chickensuit` | yes |
| `0xD0C6D545` | `pmc_hum_jen_v2` | yes |
| `0xE6C4B950` | `pmc_hum_jen_v3` | yes |
| `0xE8B7F083` | `pmc_hum_jen_v4` | yes |
| `0xC6B57C66` | `pmc_hum_jen_v5` | yes |
| `0x0BBA3066` | `pmc_hum_mattias` | yes |
| `0xDFDF5B5D` | `pmc_hum_mattias_chickensuit` | yes |
| `0x1DC4F961` | `pmc_hum_mattias_v2` | yes |
| `0xA3C1FABC` | `pmc_hum_mattias_v3` | yes |
| `0x25C98327` | `pmc_hum_mattias_v4` | yes |
| `0x0F1C9C51` | `pmc_hum_mechanic` | yes |
| `0xEE23F7E0` | `pr_hum_boss` | yes |
| `0xE7562E13` | `vz_hum_solano` | yes |

## Props / weapons / environment (91 resolved)

| hash | name | verified |
|------|------|----------|
| `0x0CFE172D` | `caracas_walllong_pristine` | yes |
| `0x30AB14E0` | `commercial_wallcorner_pristine` | yes |
| `0x0C336A4B` | `commercial_walllong_pristine` | yes |
| `0x472F6B7F` | `commercial_wallshort_pristine` | yes |
| `0x92894A5C` | `docks_fishcage` | yes |
| `0x1CE1675F` | `global_att_signnouturn` | yes |
| `0x8FAE19F4` | `global_barrela` | yes |
| `0x69AB9F8B` | `global_barrelb` | yes |
| `0x07A8C6AE` | `global_barrelc` | yes |
| `0x410AF029` | `global_barrelorange` | yes |
| `0xF4A846E1` | `global_barricadea` | yes |
| `0x6C2FA30F` | `global_boxa` | yes |
| `0xE2329B68` | `global_boxb` | yes |
| `0xCC34B75D` | `global_boxc` | yes |
| `0x663C3B0C` | `global_bullettracer` | yes |
| `0x516E145E` | `global_chairwooda` | yes |
| `0xD977E4D2` | `global_chairwoode` | yes |
| `0xC7609576` | `global_concretebarrier01` | yes |
| `0xD1B791F9` | `global_drinkingfountain` | yes |
| `0xE43A561C` | `global_env_bush01` | yes |
| `0x20DA5502` | `global_env_rocksbeach02` | yes |
| `0x02F17D0C` | `global_env_rocktall01` | yes |
| `0x5F1CF60F` | `global_env_scrub03` | yes |
| `0x294B9E7E` | `global_fencebarbed` | yes |
| `0xDA6328D9` | `global_fencewoodpanel` | yes |
| `0xC977839E` | `global_firehydrantred02` | yes |
| `0xE700B8EE` | `global_flag_smallal` | yes |
| `0xE2B57CFB` | `global_flag_wallgr` | yes |
| `0x73C6BEB7` | `global_fruitboxa` | yes |
| `0x43552C13` | `global_golfclub` | yes |
| `0xF84AEB3C` | `global_locationmarker` | yes |
| `0x9A0013C3` | `global_locker` | yes |
| `0x40FF196A` | `global_objectivemarker` | yes |
| `0xB6FE9827` | `global_parkinglota` | yes |
| `0xC40DE75F` | `global_signcokea` | yes |
| `0x2653B4DD` | `global_signstopa` | yes |
| `0xEF1018E6` | `global_tablecafe` | yes |
| `0x6670475F` | `global_tablechess` | yes |
| `0x1FB95B74` | `global_tablefolding` | yes |
| `0xD2A84161` | `global_tablemarket` | yes |
| `0xB1EC0672` | `global_targetmarker` | yes |
| `0xE86593FA` | `global_trashbag01` | yes |
| `0x196896DC` | `global_tripwirefinish` | yes |
| `0x7CC413C4` | `global_vendingmachine01` | yes |
| `0x16C1349B` | `global_vendingmachine02` | yes |
| `0x3EA94386` | `global_waterpuddle02` | yes |
| `0x5B3B0BF5` | `global_weapon_amraam` | yes |
| `0x83A15CF9` | `global_weapon_cruisemissile` | yes |
| `0xF67B3FD5` | `global_weapon_daisycutter` | yes |
| `0xBB034506` | `global_weapon_dshk` | yes |
| `0x421B5FA3` | `global_weapon_ffarrocket` | yes |
| `0x0097289B` | `global_weapon_gbu16` | yes |
| `0x528A5A4B` | `global_weapon_grapplinghook` | yes |
| `0xF799E10F` | `global_weapon_hellfire` | yes |
| `0xBE5CA114` | `global_weapon_ied` | yes |
| `0xC68A3A23` | `global_weapon_m21at` | yes |
| `0x5B6F14FF` | `global_weapon_minigun` | yes |
| `0x6FFFC2DF` | `global_weapon_moab` | yes |
| `0x81758ACC` | `global_weapon_quad50` | yes |
| `0xCE2AEFC3` | `global_weapon_recoilessrifle` | yes |
| `0x076BED6B` | `global_weapon_rpg` | yes |
| `0x93533353` | `global_weapon_rpgrocket` | yes |
| `0xD1575ABB` | `global_weapon_sniperdragunov` | yes |
| `0x80725FF4` | `global_weapon_stinger` | yes |
| `0xC3A8A6AC` | `global_weapon_tow` | yes |
| `0x0927AEBC` | `global_weapon_towtripod` | yes |
| `0x64F25712` | `industrial_signgastall` | yes |
| `0xEDF20B2C` | `industrial_trackstop` | yes |
| `0x1DB09757` | `jungle_env_bushsmall03` | yes |
| `0xF405EF93` | `jungle_env_plantsmall02` | yes |
| `0x1203E036` | `jungle_env_plantsmall03` | yes |
| `0x63079DCA` | `margarita_env_trench` | yes |
| `0x5F6F6450` | `merida_bld_plazachurch` | yes |
| `0x37E81895` | `merida_pmcautoshop_sportscar` | yes |
| `0x17128250` | `merida_signa` | yes |
| `0x01102117` | `merida_signb` | yes |
| `0x9F0D483A` | `merida_signc` | yes |
| `0x0C3B85E3` | `merida_universityfence` | yes |
| `0xDF07DBFF` | `mountain_blastdoors_stitcher` | yes |
| `0x75DB90E0` | `ocoutpost_hqbase` | yes |
| `0x75BEFD49` | `ocoutpost_tablearmwrestling` | yes |
| `0x191C53FB` | `pmcoutpost_coverflowerpot` | yes |
| `0x7A4CEDB3` | `pmcoutpost_hq_door_entrance` | yes |
| `0xFA91F7B7` | `pmcoutpost_hq_door_roof` | yes |
| `0x7FD34E23` | `shanty_canvasshort` | yes |
| `0x0FFDE8C8` | `shanty_env_bush01` | yes |
| `0xF7F945D2` | `shanty_env_bush03` | yes |
| `0x058CD8B8` | `shanty_polepower` | yes |
| `0xC6937275` | `village_cota` | yes |
| `0x11EA5D4A` | `village_lampcolemana` | yes |
| `0x7E6BEF2C` | `village_prop_tentsmallpupa` | yes |

## Machine-readable output

[`docs/modernization/model_names.tsv`](model_names.tsv) — `hash<TAB>name<TAB>humanoid<TAB>verified`,
one row per resolved model (112 rows).

## Recommended engine integration

Keep it simple: ship the compact TSV and load it at startup.

1. **Ship `model_names.tsv`** (this file, ~112 rows, a few KB) alongside the
   engine, or embed it with `include_str!`.
2. In `mercs2_engine`, build a `HashMap<u32, String>` (or a `phf` map if you want
   zero-alloc/const) from the TSV at startup. Parse `hash` as `u32::from_str_radix(&s[2..], 16)`.
3. When printing a model line, look up the hash; if present, append the name,
   else print the bare hash. Example:
   `0xA3C1FABC (pmc_hum_mattias_v3) block=... meshes=...`.

Because names are verifiable (`pandemic_hash_m2(name) == hash`), the loader can
optionally assert each row on load in debug builds to catch a corrupted map.

A `phf::phf_map!` generated from the TSV via a `build.rs` is the tidiest
zero-runtime-parse option, but the plain-TSV-at-startup approach is the minimal
change and is recommended first.

## Improving coverage

### The console WADs are the name oracle — the PC bake strips names

The retail PC `vz.wad` does not spell out the names of build-GENERATED assets. Grep it
for `tinygeometry` (the per-region LOD imposters) and you get **zero** hits; grep the PS3
WAD and you get 1,548. The console builds ship an *uncompressed block-path table*, and a
block path spells its asset's name:

```
blocks\vz\vz_state_alljob005_01_destroyed_tinygeometry_tgr12_tgc06_0x001439f9_P000_Q3.block
          `------------------------------ the model's name ---------------------------'
```

Strip the `_P%03d_Q%d` LOD-rung suffix and `.block`, hash the stem, and it lands on the PC
model hash — same assets, same hashes, only the console still names them. Verified: the PC
WAD keeps the *layer's* path (`vz_state_alljob005_01_destroyed_P000_Q3.block`) but drops
the tiny-geometry *model's* path, keeping only its hash.

`aset_external_mine` does this over any binary (console WADs, the Jul-2008 prototype ISO,
wordlists) and emits only verified preimages:

```
cargo run --release -p wad_simulator --bin aset_external_mine -- \
    --names docs/data/aset_names.csv \
    --source game-files/ps3-VZ.WAD --merge docs/data/aset_model_names.json \
    --emit docs/data/aset_external_names.json
```

This took model naming from **1,901/2,964 (64%)** to **2,442/3,007 (81%)**.

### Do NOT brute-force the grammar — the hash is only 32 bits

The advice this section used to give — extend `build_rainbow_table.py`'s candidate
generation with more vocabulary and re-run — is **actively harmful**, and the corpus
carries the scars.

A candidate set of size `S` tested against `T` targets produces `S*T/2^32` preimages
*by chance*. With `T ≈ 13,000` unresolved hashes, anything past `S ≈ 6.7M` candidates is
expected to mint at least one false name that "verifies" perfectly — `pandemic_hash_m2(name)
== hash` is then true of a name the game never used. `aset_expanded_names.json` contained
three such fabrications, caught only because they are *structurally* impossible:

```
vz_state_oiljob007_e_pristine_tinygeometry_6x00022cdb   <- format string emits _0x%08x, not 6x
vz_state_chicon18_pristine_tinygeometry_18x14acc
vz_pmc_tiny_tinygeometry_6x00018e04
```

They have been removed. The ones that happen to *look* plausible are undetectable, so the
rule is: **a hash match alone is not evidence.** Every emitted name needs a second,
independent witness. `aset_external_mine` tracks that as *provenance* and reports a
per-source error bar (`S*T/2^32` for the tokens that source actually contributed):

| channel | witness | expected noise |
|---------|---------|----------------|
| `block-stem` | the token *was* the asset's block path — structure + hash agree | 0.02 over 428 names |
| `tex-stem` | token was `<name>_dm/_nm/_sm`, the texture convention | ~0 |
| `bare` | a raw token in a multi-GB image — **hash is the only witness** | 13.3 over 6 names → junk |

The bare channel over a 6 GB disc image is precisely where the collisions appeared, and
they were all short underscore-less gibberish (`cbjoxg`, `kdwjc`, `qxcvzq`, `rfwuf`) — so
bare tokens are additionally required to contain `_` and be ≥8 chars. Read the `noise`
column as "how many of this row's names are fiction".

### The 565 that remain are not recoverable from anything we hold

They live *inside* shared blocks — `c3xxxx` region model libraries, `resident2`, `effects`
— rather than owning a block of their own, so no block path spells them, and they appear
as a bare string in no artifact we have (PC WAD, PS3 WAD, Xbox WAD, Jul-2008 prototype ISO,
`console-vz-wads.zip` — all mined, all exhausted). Naming them needs a *new source* (a dev
or debug build), not a cleverer search of the old ones.
