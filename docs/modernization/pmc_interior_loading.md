# PMC HQ Interior — how the game assembles it (data-driven, de-hardcoded)

**Purpose:** locate *where every piece of the PMC interior comes from in the shipped data*, so the
engine's interior loader can be driven from that data instead of the hand-written mesh/block lists in
`crates/mercs2_game/src/pmc.rs`. Nothing about the interior is invented at runtime — it is all present
in the WAD + the live name registry + the save; this document says exactly where.

Compiled 2026-07-04. Verified with `mercs2_probe --bin light_probe` / `--bin mesh_probe` and
`mercs2_game --comps 667`.

## The three sources (none should be hardcoded)

The interior is assembled from **layers** (furniture) + **structure meshes** (the rooms) + **amount-
driven meshes** (money / stockpile). Each has a concrete data source:

### 1. Layers — furniture & placements (`vz_state_pmcinterior*` blocks)

The block numbers are **not** magic constants; they are the WAD's own `PTHS` path index
(`wad::block_paths`). Filtering it for `pmcinterior` gives the complete set:

| Block | Path | When |
|---|---|---|
| 667 | `vz_state_pmcinterior` | base — always |
| 461 | `vz_state_pmcinterior_mec` | Eva (Mec) unlocked |
| 291 | `vz_state_pmcinterior_mecabsent` | Eva not unlocked |
| 703 | `vz_state_pmcinterior_hel` | Ewen (Hel) unlocked |
| 711 | `vz_state_pmcinterior_jet` | Misha (Jet) unlocked |

The gating (base + each recruit's present/absent layer) is `_GetStarterLayers()` +
`MrxLayerManager.Add(...)` in [`docs/mercs2-luacd/src/vz/wifpmcinterior.lua`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifpmcinterior.lua) (`_tStarters`).

**Block 667's data** (`--comps 667`): three COMP types in one sub-block — `Name` (3678 B), `ModelName`
(8 B = **1** record, the Custom Outfit Wardrobe), `Transform` (4368 B = **84** entities). So the
furniture links to its mesh by **`pandemic_hash_m2(entity name, leading `_` stripped)`** — one
`ModelName` record aside. Verified resolving: `electricBoxB/C`, `togoBox`, `icebox`, `generator`,
`printer`, `waterCooler`, `armyCot`, `IV`, `tray`, `plantera`, `planterc`, `radioA`, `global_files`,
`global_lamppostmilitary`. Unnamed Transforms are markers/hardpoints (no mesh).

### 2. Structure meshes — the rooms (the `HqInterior` actor)

**Key finding:** block 667 references **none** of the structure mesh hashes (0 occurrences of the hall,
sickbay, scaffold, or recruit hashes as u32). The rooms are **not** in the layer block — they are the
`HqInterior` actor, spawned by `MrxUtil.SpawnActor("location", …, "HqInterior", …)`
(`wifpmcinterior.lua:980`; HQ config template `_proutpost_interior_job`).

Every interior mesh is **name-addressable** — `pandemic_hash_m2("pmcoutpost_interior_<part>")` — so no
geometric hunts or hardcoded hashes are needed. (The hall we "found geometrically", `0x39AF17DC`, is
simply `pandemic_hash_m2("pmcoutpost_interior_hq")`.) Verified base structure parts:

| Mesh name | hash | verts / draws | note |
|---|---|---|---|
| `pmcoutpost_interior_hq` | 0x39AF17DC | 96784 / 80 | the main ornate hall (prelit) |
| `pmcoutpost_interior_sickbay` | 0x757EAE95 | 5183 / 13 | now loaded (base) |
| `pmcoutpost_interior_scaffold` | 0x1FBFBB4B | 825 / 4 | now loaded (base) |
| `proutpost_interior_job` | 0xCE8165DE | 9977 / 16 | prelit room — now loaded (base) |
| `pmcoutpost_interior_recruitmechanic` | 0xE8EB75D7 | 19197 / 23 | Eva bay (unlock) |
| `pmcoutpost_interior_recruitjet` | 0x86D7CF92 | 8970 / 10 | Misha bay (unlock) |
| `pmcoutpost_interior_recruitheli` | 0x634F1F65 | 10247 / 14 | Ewen bay (unlock) |

The **identities** of all interior parts are in the live name registry
([`docs/data/live_registry_hashes.csv`](../../crates/mercs2_workshop/data/live_registry_hashes.csv)) — 17 `_pmcoutpost_interior_*` entries (dumped from the process at
`0xDF6B88`). That CSV is the authoritative list; the engine just needs to hash the base-part names.

### 3. Amount-driven meshes — money & stockpile

Registry `_pmcoutpost_interior_money`, `_money_a`…`_money_i`, `_pmcoutpost_interior_stockpile`. These
are shown **by the save's amounts** (money by cash tier — see `_tStockpile.money` thresholds in
`wifpmcinterior.lua` / `pmc::Stockpile::tier_visible`; supplies by quantity). Their name→ASET resolution
differs from the base parts (their `pandemic_hash_m2` misses the container), so they resolve through the
stockpile-category path (`pmc::stockpile_mesh`), not the plain name hash.

## The drift (what we hardcode vs. what the game does)

| Base game | Engine today (`pmc.rs`) |
|---|---|
| Layers via `PTHS` + `_GetStarterLayers` gating | **hardcoded** `state_blocks = [667, 461/291, 703, 711]` |
| Structure = `HqInterior` actor's base `pmcoutpost_interior_*` parts | name-resolved base parts (hall + sickbay + scaffold + job) + recruit bays by unlock — **DONE** (was: hall + recruit bays only) |
| Money/stockpile by amount | money tier-gated (partial); **stockpile absent** |

That missing sickbay/scaffold/job (and stockpile) is why the interior reads incomplete — not a render
bug, a **de-hardcoding gap**.

## Data-driven loader design (target)

1. **Layers:** resolve `vz_state_pmcinterior[_<recruit>]` → block index via `wad::block_paths` (no
   literal block numbers); load base always + each recruit's present/absent block by unlock flag.
2. **Structure:** load the base parts by name — `pmcoutpost_interior_{hq,sickbay,scaffold}` +
   `proutpost_interior_job` — via `pandemic_hash_m2` at the actor origin `(3750, 450, -3840)`; add each
   recruit bay by unlock. The base-part *set* is the non-recruit, non-amount `pmcoutpost_interior_*`
   entries in the registry.
3. **Amount-driven:** money + stockpile meshes by the save's cash/supply amounts.
4. All identities come from the registry + name hashing; all gating from save state — **no hand-lists.**

**Open verification:** confirm the base parts assemble correctly at the actor origin (sickbay/scaffold/
job positioned right relative to the hall) once loaded — they share the actor origin like the recruit
bays, so they should drop in place, but it needs an eyes-on check.
