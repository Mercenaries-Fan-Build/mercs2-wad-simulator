//!  Shared world/asset helpers carved out of `main.rs`.
//!
//!  Render-agnostic constants, spatial helpers, the terrain `HeightMap`, the streaming
//!  DECISION catalog builder, and reverse-hash utilities. Used by BOTH the engine binary's
//!  run modes (`main.rs`) and the headless diagnostics in `crate::diag` (the `mercs2_probe`
//!  binary), so there is exactly one implementation.

#![allow(clippy::all)]
use crate::wad;

/// Default WAD block indices for the two terrain inputs (from the `00029_…` /
/// `03121_…` filenames). Verified/repaired at load time by `find_terrain_blocks`.
pub const LAYERS_STATIC_BLOCK: u16 = 29;
pub const LOW_RES_TERRAIN_BLOCK: u16 = 3121;

/// Decompress the low_res_terrain (3121) + layers_static (29) blocks, verifying the
/// expected signatures. If an index doesn't match, scan a bounded range of block
/// indices for the right one and log which index actually matched.
///
/// low_res_terrain block: `u32[0] == 401` and contains `b"UCFX"`.
/// layers_static block: contains `b"UCFX"` and the ascii `"LowResTerrainObject"`.
pub fn find_terrain_blocks(w: &mut wad::Wad) -> Result<(Vec<u8>, Vec<u8>), String> {
    fn is_low_res(b: &[u8]) -> bool {
        b.len() >= 4
            && u32::from_le_bytes([b[0], b[1], b[2], b[3]]) == 401
            && b.windows(4).any(|w| w == b"UCFX")
    }
    fn is_layers_static(b: &[u8]) -> bool {
        b.windows(4).any(|w| w == b"UCFX")
            && b.windows(19).any(|w| w == b"LowResTerrainObject")
    }

    // low_res_terrain (3121).
    let low = wad::decompress_block_index(w, LOW_RES_TERRAIN_BLOCK).ok().filter(|b| is_low_res(b));
    let (low, low_idx) = match low {
        Some(b) => (b, LOW_RES_TERRAIN_BLOCK),
        None => {
            eprintln!(
                "[world] block {LOW_RES_TERRAIN_BLOCK} is not low_res_terrain (u32[0]!=401 or no UCFX); scanning…"
            );
            let mut found = None;
            for idx in 0..12000u16 {
                if let Ok(b) = wad::decompress_block_index(w, idx) {
                    if is_low_res(&b) {
                        found = Some((b, idx));
                        break;
                    }
                }
            }
            found.ok_or("no block matched low_res_terrain signature (u32[0]==401 + UCFX)")?
        }
    };
    if low_idx != LOW_RES_TERRAIN_BLOCK {
        eprintln!("[world] low_res_terrain actually at block {low_idx} (expected {LOW_RES_TERRAIN_BLOCK})");
    } else {
        eprintln!("[world] low_res_terrain block {low_idx}: OK (u32[0]==401, UCFX present)");
    }

    // layers_static (29).
    let ls = wad::decompress_block_index(w, LAYERS_STATIC_BLOCK).ok().filter(|b| is_layers_static(b));
    let (ls, ls_idx) = match ls {
        Some(b) => (b, LAYERS_STATIC_BLOCK),
        None => {
            eprintln!(
                "[world] block {LAYERS_STATIC_BLOCK} is not layers_static (no UCFX/LowResTerrainObject); scanning…"
            );
            let mut found = None;
            for idx in 0..12000u16 {
                if let Ok(b) = wad::decompress_block_index(w, idx) {
                    if is_layers_static(&b) {
                        found = Some((b, idx));
                        break;
                    }
                }
            }
            found.ok_or("no block matched layers_static signature (UCFX + LowResTerrainObject)")?
        }
    };
    if ls_idx != LAYERS_STATIC_BLOCK {
        eprintln!("[world] layers_static actually at block {ls_idx} (expected {LAYERS_STATIC_BLOCK})");
    } else {
        eprintln!("[world] layers_static block {ls_idx}: OK (UCFX + LowResTerrainObject present)");
    }

    Ok((low, ls))
}

/// Lowest block index whose PTHS path contains `needle` (case-insensitive).
pub fn find_block_by_path(w: &wad::Wad, needle: &str) -> Option<u16> {
    let needle = needle.to_lowercase();
    wad::block_paths(w)
        .iter()
        .position(|p| p.to_lowercase().contains(&needle))
        .map(|i| i as u16)
}

/// Name hashes of every texture asset in a `terraintextures*` block's entry table.
pub fn terraintexture_hashes(w: &mut wad::Wad, needle: &str) -> Vec<u32> {
    let Some(bi) = find_block_by_path(w, needle) else { return Vec::new() };
    let Ok(dec) = wad::decompress_block_index(w, bi) else { return Vec::new() };
    let (_n, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    entries.iter().map(|e| e.name_hash).collect()
}

/// The terrainmesh CHDR class hash (`0x7C569307`, "terrainmesh" — per-cell hi-res terrain geometry;
/// docs/aset_format.md). Distinct from the small building `Model` (`MODEL_TYPE_HASH`).
pub const TERRAINMESH_TYPE_HASH: u32 = 0x7C56_9307;

pub fn parse_hash(s: &str) -> Option<u32> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u32::from_str_radix(s, 16).ok()
}

/// Best-effort bone-name resolution from the repo rainbow table (tools/rainbow_table.json).
/// Returns hash -> first candidate name for exactly the hashes asked for; empty map if the
/// table is absent (the diagnostic still prints hashes).
pub fn rainbow_names(hashes: &std::collections::BTreeSet<u32>) -> std::collections::HashMap<u32, String> {
    let mut out = std::collections::HashMap::new();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../rainbow_table.json");
    let Ok(text) = std::fs::read_to_string(path) else { return out };
    for &h in hashes {
        let key = format!("\"0x{h:08X}\"");
        let Some(p) = text.find(&key) else { continue };
        let rest = &text[p + key.len()..];
        let Some(q0) = rest.find('"') else { continue };
        let Some(q1) = rest[q0 + 1..].find('"') else { continue };
        out.insert(h, rest[q0 + 1..q0 + 1 + q1].to_string());
    }
    out
}

/// The PMC HQ compound, game coords (docs/coordinate_systems.md Example 1).
pub const PMC_HQ: [f32; 2] = [2647.0, -951.0];
pub const PMC_HQ_RADIUS_M: f32 = 150.0;

/// Normal world envelope (docs §5). A placement outside it is an interior-hunt
/// candidate: |x|>4000 OR |z|>4000 OR y<-150 OR y>450.
pub fn is_out_of_bounds(p: &[f32; 3]) -> bool {
    p[0].abs() > 4000.0 || p[2].abs() > 4000.0 || p[1] < -150.0 || p[1] > 450.0
}

/// True if a placement's name flags it as a base/interior of interest.
pub fn name_is_pmc_base(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["pmc", "interior", "hq", "base", "outpost"]
        .iter()
        .any(|k| n.contains(k))
}

/// True if a placement belongs to the PMC-base subset (near the HQ or name-flagged).
pub fn placement_is_pmc_subset(p: &mercs2_formats::placement::Placement) -> bool {
    let dx = p.pos[0] - PMC_HQ[0];
    let dz = p.pos[2] - PMC_HQ[1];
    if (dx * dx + dz * dz).sqrt() <= PMC_HQ_RADIUS_M {
        return true;
    }
    p.name.as_deref().map(name_is_pmc_base).unwrap_or(false)
}

/// Print the full interior-hunt analysis (Task 2): out-of-bounds clusters,
/// pmc/interior/base-named placements, and PMC-subset count. Pure logging.
pub fn report_interior_hunt(placements: &[mercs2_formats::placement::Placement]) {
    // Overall counts + ranges.
    let named = placements.iter().filter(|p| p.name.is_some()).count();
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in placements {
        for k in 0..3 {
            min[k] = min[k].min(p.pos[k]);
            max[k] = max[k].max(p.pos[k]);
        }
    }
    println!(
        "[placements] total = {}, named = {}",
        placements.len(),
        named
    );
    println!(
        "[placements] X range = [{:.1}, {:.1}]  Y range = [{:.1}, {:.1}]  Z range = [{:.1}, {:.1}]",
        min[0], max[0], min[1], max[1], min[2], max[2]
    );

    // Out-of-bounds cluster analysis: bin by ~500 m XZ cell + Y band, print
    // centroids + counts + sample names.
    let oob: Vec<&mercs2_formats::placement::Placement> =
        placements.iter().filter(|p| is_out_of_bounds(&p.pos)).collect();
    println!("[interior-hunt] out-of-bounds placements (|x|>4000 | |z|>4000 | y<-150 | y>450) = {}", oob.len());
    if !oob.is_empty() {
        use std::collections::HashMap;
        let mut clusters: HashMap<(i32, i32, i32), Vec<&mercs2_formats::placement::Placement>> =
            HashMap::new();
        for p in &oob {
            let cx = (p.pos[0] / 500.0).round() as i32;
            let cz = (p.pos[2] / 500.0).round() as i32;
            let cy = (p.pos[1] / 200.0).round() as i32; // 200 m Y band
            clusters.entry((cx, cy, cz)).or_default().push(p);
        }
        let mut ranked: Vec<((i32, i32, i32), Vec<&mercs2_formats::placement::Placement>)> =
            clusters.into_iter().collect();
        ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        for ((_cx, _cy, _cz), members) in ranked.iter().take(20) {
            let n = members.len() as f32;
            let mut c = [0.0f32; 3];
            for m in members {
                for k in 0..3 {
                    c[k] += m.pos[k] / n;
                }
            }
            let samples: Vec<String> = members
                .iter()
                .filter_map(|m| m.name.clone())
                .take(4)
                .collect();
            println!(
                "[interior-hunt]   cluster n={:<5} centroid=({:.0}, {:.0}, {:.0})  samples: {}",
                members.len(),
                c[0],
                c[1],
                c[2],
                if samples.is_empty() { "<unnamed>".to_string() } else { samples.join(", ") }
            );
        }
    }

    // Name-flagged placements (pmc/interior/hq/base/outpost).
    let flagged: Vec<&mercs2_formats::placement::Placement> = placements
        .iter()
        .filter(|p| p.name.as_deref().map(name_is_pmc_base).unwrap_or(false))
        .collect();
    println!("[interior-hunt] name-flagged (pmc/interior/hq/base/outpost) = {}", flagged.len());
    // Group by distinct name for a compact report (name -> count + one sample pos).
    {
        use std::collections::BTreeMap;
        let mut by_name: BTreeMap<String, (usize, [f32; 3])> = BTreeMap::new();
        for p in &flagged {
            let e = by_name.entry(p.name.clone().unwrap()).or_insert((0, p.pos));
            e.0 += 1;
        }
        for (name, (count, pos)) in by_name.iter().take(60) {
            println!(
                "[interior-hunt]   {name:<40} x{count:<4} e.g. ({:.0}, {:.0}, {:.0})",
                pos[0], pos[1], pos[2]
            );
        }
        if by_name.len() > 60 {
            println!("[interior-hunt]   ... {} more distinct names", by_name.len() - 60);
        }
    }

    // Interior locator: the game boots the player into the PMC interior at the SE-corner coord
    // (3794.04, 450.75, -3911.03) (MrxUtil._TeleportHero). Count any layers_static placement within
    // 300 m XZ of it — if none, the interior geometry is NOT in this block (it's a runtime-spawned
    // HqInterior actor / separate cell), which the Z-min below confirms.
    const INT_XZ: [f32; 2] = [3794.0427, -3911.0322];
    let near_int: Vec<&mercs2_formats::placement::Placement> = placements
        .iter()
        .filter(|p| {
            let dx = p.pos[0] - INT_XZ[0];
            let dz = p.pos[2] - INT_XZ[1];
            (dx * dx + dz * dz).sqrt() <= 300.0
        })
        .collect();
    println!(
        "[interior-hunt] placements within 300 m XZ of the interior coord (3794, -3911) = {} (block Z-min was {:.1}; interior Z=-3911 is BEYOND it)",
        near_int.len(),
        min[2]
    );
    for p in near_int.iter().take(10) {
        println!(
            "[interior-hunt]   near-interior: {:<32} ({:.0}, {:.0}, {:.0})",
            p.name.as_deref().unwrap_or("<unnamed>"),
            p.pos[0], p.pos[1], p.pos[2]
        );
    }

    // PMC-subset (near HQ or name-flagged) — the real-geometry render candidates.
    let subset = placements.iter().filter(|p| placement_is_pmc_subset(p)).count();
    let near_hq = placements
        .iter()
        .filter(|p| {
            let dx = p.pos[0] - PMC_HQ[0];
            let dz = p.pos[2] - PMC_HQ[1];
            (dx * dx + dz * dz).sqrt() <= PMC_HQ_RADIUS_M
        })
        .count();
    println!(
        "[interior-hunt] PMC subset (<= {PMC_HQ_RADIUS_M:.0} m of HQ {:?} OR name-flagged) = {} ({} within HQ radius)",
        PMC_HQ, subset, near_hq
    );
}

/// Build the Layer-2 streaming DECISION catalog (spec §10) from a WAD's world index + the
/// decompressed `layers_static` block. Returns the pure `StreamingManager` (blocks + per-entity
/// placements, with each entity's own `HibernationControl` distances — class defaults 100/160/60/20
/// when absent) plus the key->`PropSpawn` map the executor needs to instantiate a prop on WAKE.
///
/// - **Coarse LOAD units:** every c3 cell that carries model-format geometry (buildings are baked
///   into c3 cells — spec §2B), with its grid-square extent. `layers_static` (block 29) is the
///   always-resident base layer; its entities stream PER-ENTITY (below), never by block.
/// - **Per-entity placements:** every `ModelName` prop in `layers_static` (the entity->mesh recipe,
///   spec §2A), each carrying its own hibernation/LOD distances or the class defaults.
pub fn build_streaming_catalog(
    _idx: &mercs2_formats::world_index::WorldIndex,
    layers_static: &[u8],
    cfg: mercs2_core::streaming::StreamingConfig,
) -> (
    mercs2_core::streaming::StreamingManager,
    std::collections::HashMap<u32, PropSpawn>,
    std::collections::HashMap<u32, (u32, [f32; 3])>,
) {
    use mercs2_core::streaming::{EntityUnit, StreamingManager};

    let mut mgr = StreamingManager::new(cfg);
    let default_dist = cfg.default_distances;

    // NOTE (2026-07-02): the c3-block residency path (`load_one_c3_cell` → the small 0x5B724250
    // building `Model`) is DISABLED. That path placed the Model with a SYNTHESIZED position (c3-grid
    // XZ + Y=0), which floated ~80 m off the terrain — the misalignment the user reported. The real
    // per-cell hi-res content is the `0x7C569307` terrainmesh, now streamed correctly via the
    // `TerrainObject`->Transform tiles (below). The building `Model`'s authored transform is a
    // separate unsolved RCA (its position source is not the c3 cell-id); until it's recovered, we do
    // NOT stream it rather than render floating geometry. Re-enable once that placement is known.
    //
    // CONFIRM-LIVE (Wave-0 S5 investigation, 2026-07-06) — WHY it stays disabled + what unblocks it:
    //   Grounded in three independent facts, the c3 building `Model` placement is genuinely
    //   unrecovered from the shipped data, NOT merely un-wired:
    //   (1) `--comp-probe` (d) proved "exterior buildings ARE baked into c3 cell geometry (not placed
    //       via ModelName)" — so, unlike every other prop, these buildings have NO `layers_static`
    //       Transform/ModelName record we could register as a per-entity placement (the loops below).
    //   (2) `WorldIndex`'s extent for a bare c3 cell is `Aabb::from_center_half(cx, cz, C3_CELL_SIZE/2)`
    //       (world_index.rs) — the cell-CENTRE box. It carries an XZ proxy but NO authored Y and NO
    //       per-object footprint offset (a cell holds up to ~39 distinct objects at distinct spots —
    //       world_streaming_spec §10.2b), so it cannot place the Model, only bound it.
    //   (3) The baked geometry is ORIGIN-LOCAL (a model LIBRARY — [[world-placements-no-model-hash]]);
    //       `load_one_c3_cell` already returns identity for the rare cells whose verts test world-space,
    //       and cell-local `[cx,0,cz]` (Y=0) for the rest — the float.
    //   The authored per-cell origin the engine actually uses is built at runtime by
    //   `TerrainObject::Activate FUN_0066cac0` (world_streaming_code_map §3): it stamps ≤16 patch
    //   children under a PARENT record carrying the cell's world AABB. The building-cell analog of that
    //   parent AABB origin is the missing datum. UNBLOCK = recover that authored origin, by either:
    //     (a) x32dbg break on `FUN_0066cac0` / the c3-cell activate path and read the parent-record
    //         AABB origin the engine assigns (read-only while PAUSED — [[x32dbg-mcp-no-resume]]); or
    //     (b) locating a per-cell `Transform`/POFF record analogous to the terrain-tile
    //         `TerrainObject->Transform` composition used for the 0x7C569307 tiles below.
    //   Deriving Y from a terrain-height sample would be a FABRICATION (a building may sit on a roof /
    //   in a pit) and is explicitly declined. Until (a) or (b) lands, do NOT stream it.

    // Per-entity placements: ModelName props in layers_static, keyed by entity key with their own
    // hibernation directive (or the class defaults).
    let mut props: std::collections::HashMap<u32, PropSpawn> = std::collections::HashMap::new();
    for p in mercs2_formats::placement::load_model_placements(layers_static) {
        let dist = p.hibernation.map(|h| h.dist).unwrap_or(default_dist);
        mgr.add_entity(EntityUnit { key: p.key, pos: p.pos, dist });
        props.insert(p.key, PropSpawn { model_hash: p.model_hash, pos: p.pos, quat: p.quat });
    }

    // Hi-res terrain tiles: the 400 `0x7C569307` terrainmesh tiles, placed via TerrainObject->Transform
    // (POFF-composed 400 m tiles). Streamed per-tile with a large stream-out (terrain reads from far).
    // Added BEFORE the named pass so a terrain-tile entity (which also has a Name) is never
    // double-added with a smaller stream-out — that double-add made the manager emit conflicting
    // wake(d<1000)/hibernate(d>400) for the same key each tick, flickering the low-res hide/show.
    let mut terrain_tiles: std::collections::HashMap<u32, (u32, [f32; 3])> = std::collections::HashMap::new();
    for t in mercs2_formats::placement::load_terrain_tiles(layers_static) {
        mgr.add_entity(EntityUnit { key: t.key, pos: t.pos, dist: [1000, 160, 60, 20] });
        terrain_tiles.insert(t.key, (t.terrainmesh_hash, t.pos));
    }

    // Named world content — the INSTANCED trees/rocks/bushes/fences/lamps/props: ~5,000 distinct
    // models referenced 60k+ times (e.g. jungle_env_plantlarge04 ×1912), placed via Name + Transform
    // with the mesh resolved by NAME-HASH (`pandemic_hash_m2`). These have a Name but no ModelName, so
    // they were never loaded before. Add every such entity; the executor resolves the mesh on WAKE
    // (caching non-mesh names like Road/Light/Lane as wake-failures). Instances of the same model
    // share one GPU upload (`scene.has_model`). Env objects get a larger stream-out (visible farther).
    for p in mercs2_formats::placement::load_placements(layers_static).unwrap_or_default() {
        if props.contains_key(&p.key) || terrain_tiles.contains_key(&p.key) {
            continue; // already a ModelName prop or a hi-res terrain tile
        }
        let Some(name) = &p.name else { continue };
        let base = name.trim_start_matches('_');
        let h = mercs2_formats::hash::pandemic_hash_m2(base);
        // Big env props (rocks/plants/trees) read from farther; small props use the class default.
        let lname = base.to_ascii_lowercase();
        let far = lname.contains("env") || lname.contains("rock") || lname.contains("huge")
            || lname.contains("large") || lname.contains("tree") || lname.contains("building");
        let dist = if far { [400, 160, 60, 20] } else { default_dist };
        mgr.add_entity(EntityUnit { key: p.key, pos: p.pos, dist });
        props.insert(p.key, PropSpawn { model_hash: h, pos: p.pos, quat: p.quat });
    }

    // Region cache (PgSysPopulation CacheIn/CacheOut — row 9), SEAM B: the streaming manager carries
    // the decision layer (`mgr.add_region` / `update_regions`); this now POPULATES it from the world's
    // authored `PopulationDensity` COMPs (E1 schema path — `register_population_regions`). See that
    // function for the grounding + the confirm-live note on why the extent is the authored Transform
    // anchor (a point) rather than an on-disk rect (region geometry is NOT authored in the placed
    // COMP; the runtime rect `FUN_004d60e0` +0x10..+0x1c is computed at load).
    let n_regions = register_population_regions(layers_static, &mut mgr);
    if n_regions > 0 {
        println!("[stream] region cache (seam B): {n_regions} PopulationDensity anchors registered");
    }

    (mgr, props, terrain_tiles)
}

/// Fold one `vz_state` OVERLAY block's placements into an existing streaming catalog, using the SAME
/// recipe [`build_streaming_catalog`] applies to `layers_static`: the overlay's `ModelName` props +
/// named-hash props are registered with `mgr` (proximity streaming) and `props` (the wake recipe).
/// Overlays carry no terrain tiles. Returns `(modelname_added, named_added)`. Any key already present
/// (the base layer or a previously-folded overlay) is skipped, so an overlay never double-streams an
/// entity — matching the game's model where vz_state overlays layer ON TOP of the always-loaded base.
pub fn add_overlay_to_catalog(
    block: &[u8],
    default_dist: [u16; 4],
    mgr: &mut mercs2_core::streaming::StreamingManager,
    props: &mut std::collections::HashMap<u32, PropSpawn>,
) -> (usize, usize) {
    use mercs2_core::streaming::EntityUnit;

    let mut n_mn = 0usize;
    for p in mercs2_formats::placement::load_model_placements(block) {
        if props.contains_key(&p.key) {
            continue;
        }
        let dist = p.hibernation.map(|h| h.dist).unwrap_or(default_dist);
        mgr.add_entity(EntityUnit { key: p.key, pos: p.pos, dist });
        props.insert(p.key, PropSpawn { model_hash: p.model_hash, pos: p.pos, quat: p.quat });
        n_mn += 1;
    }

    let mut n_named = 0usize;
    for p in mercs2_formats::placement::load_placements(block).unwrap_or_default() {
        if props.contains_key(&p.key) {
            continue;
        }
        let Some(name) = &p.name else { continue };
        let base = name.trim_start_matches('_');
        let h = mercs2_formats::hash::pandemic_hash_m2(base);
        let lname = base.to_ascii_lowercase();
        let far = lname.contains("env") || lname.contains("rock") || lname.contains("huge")
            || lname.contains("large") || lname.contains("tree") || lname.contains("building");
        let dist = if far { [400, 160, 60, 20] } else { default_dist };
        mgr.add_entity(EntityUnit { key: p.key, pos: p.pos, dist });
        props.insert(p.key, PropSpawn { model_hash: h, pos: p.pos, quat: p.quat });
        n_named += 1;
    }

    // Fold the overlay's population regions into the same region cache (seam B), on top of the base.
    let _ = register_population_regions(block, mgr);

    (n_mn, n_named)
}

// ===========================================================================================
//  E1 schema → world-loader wiring (Wave-1 seam A) + region cache activation (seam B).
//
//  The bespoke `placement::*` loaders remain the correctness ORACLE for the props/terrain/regions
//  the executor instantiates (they still drive `build_streaming_catalog` unchanged). This section
//  adds the schema-DRIVEN path ALONGSIDE them: it walks each streamed block's `COMP` groups through
//  `mercs2_formats::schema` (`parse_comp_groups` → `ComponentSchema` → `deserialize_records`), builds
//  the kernel `ComponentRegistry` (`register_with_fields`), and reads typed field values by
//  name-hash — validating agreement with the oracle where they overlap (HibernationControl dist0,
//  ModelName hash) so drift is caught, and sourcing the population regions the RegionCache needs.
// ===========================================================================================

/// Population-region cache radii (streaming tunables, native game metres) — the proximity band at
/// which a `PopulationDensity` region caches its ambient-population lump IN / OUT around its authored
/// anchor. These are TUNABLES (like `StreamingConfig::tier_stream_out`), not on-disk geometry: the
/// placed region COMP authors only the anchor position (its `Transform`), never an extent. CONFIRM-
/// LIVE: the engine's actual cache radius comes from the runtime region rect built in
/// `FUN_004d60e0`/`PgSysPopulation::Update` (+0x10..+0x1c), read live; these normalized defaults give
/// the decision core real anchors to run against until that rect is captured. `OUT > IN` = hysteresis.
pub const POPULATION_REGION_CACHE_IN: f32 = 250.0;
pub const POPULATION_REGION_CACHE_OUT: f32 = 400.0;

/// Walk every `COMP` group across a decompressed world block's UCFX containers (the same entry-table
/// framing `ucfx::walk_decompressed_block` uses), grouped into `{info, schm, data}` triples by the E1
/// `parse_comp_groups` (the `FUN_00654940` COMP arm). This is the single entry point both the schema
/// registry build and the region registration consume.
fn walk_comp_groups(block: &[u8]) -> Vec<mercs2_formats::schema::CompGroup> {
    let parsed = mercs2_formats::ucfx::walk_decompressed_block(block, "stream-world").0;
    let mut out = Vec::new();
    for c in &parsed.containers {
        out.extend(mercs2_formats::schema::parse_comp_groups(c));
    }
    out
}

/// Map an E1 asset-boundary `SchemaFieldType` to the kernel-side `FieldKind` (identical schm type
/// codes; the mirror is architecturally required — seam F).
fn field_kind_of(t: mercs2_formats::schema::SchemaFieldType) -> mercs2_core::registry::FieldKind {
    use mercs2_core::registry::FieldKind as K;
    use mercs2_formats::schema::SchemaFieldType as T;
    match t {
        T::Bit => K::Bit,
        T::U8 => K::U8,
        T::U16 => K::U16,
        T::F32 => K::F32,
        T::U32 => K::U32,
        T::Ref => K::Ref,
        T::StringRef => K::StringRef,
        T::Flags => K::Flags,
        T::Vec3 => K::Vec3,
        T::Blob32 => K::Blob32,
    }
}

/// Result of the schema-driven load pass over one world block.
#[derive(Debug, Default, Clone)]
pub struct SchemaLoadStats {
    /// Distinct component classes registered into the `ComponentRegistry` (each `schm` → descriptor).
    pub classes: usize,
    /// Generic `COMP` groups whose fixed-stride `data` deserialized (excludes `Transform`, which is
    /// CHDR-stride-gated per E1, and variable-length/`Name` records).
    pub generic_groups: usize,
    /// Total per-entity records deserialized across those generic groups.
    pub generic_records: usize,
    /// HibernationControl dist0 values cross-checked against the `placement` oracle, and how many
    /// agreed (the schema path must match the oracle exactly where they overlap).
    pub hib_checked: usize,
    pub hib_agree: usize,
    /// ModelName hashes cross-checked against the oracle, and how many agreed.
    pub model_checked: usize,
    pub model_agree: usize,
}

/// Build the kernel `ComponentRegistry` from a world block's `schm` field schemas and deserialize its
/// generic `COMP` records through the E1 schema path, cross-checking agreement with the bespoke
/// `placement` oracle. ADDITIVE + read-only: this informs/validates instantiation; it does not change
/// what streams (the oracle-driven catalog is unchanged). Returns the registry + a stats summary.
pub fn load_schema_components(block: &[u8]) -> (mercs2_core::ComponentRegistry, SchemaLoadStats) {
    use mercs2_core::registry::FieldLayout;
    use mercs2_formats::schema::FieldValue;

    let mut reg = mercs2_core::ComponentRegistry::new();
    let mut stats = SchemaLoadStats::default();

    // Oracle references for the agreement cross-check.
    let oracle_hib = mercs2_formats::placement::load_hibernation(block);
    let oracle_models: std::collections::HashMap<u32, u32> =
        mercs2_formats::placement::load_model_placements(block)
            .iter()
            .map(|m| (m.key, m.model_hash))
            .collect();
    let mut schema_hib: std::collections::HashMap<u32, u16> = std::collections::HashMap::new();
    let mut schema_model: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();

    for g in walk_comp_groups(block) {
        let Some(name) = g.name.clone() else { continue };
        let Some(schema) = g.schema() else { continue };

        // Register the class (schm → descriptor). register_with_fields is idempotent per type-hash.
        let type_hash = g
            .type_hash
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(&name));
        let fields: Vec<FieldLayout> = schema
            .fields
            .iter()
            .map(|f| FieldLayout {
                name_hash: f.name_hash,
                byte_offset: f.byte_offset,
                bit_index: f.bit_index,
                kind: field_kind_of(f.field_type),
            })
            .collect();
        reg.register_with_fields(name.clone(), type_hash, Some(schema.payload_stride), fields);

        // Transform's on-disk `data` is written by the CHDR-stride-gated builder (0x0063D7C0), NOT the
        // generic [key][payload] path — keep using the placement parser for it (E1 confirm-live note).
        if name == "Transform" {
            continue;
        }
        let Some(data) = g.data.as_ref() else { continue };
        if data.is_empty() {
            continue;
        }
        let Some(recs) = schema.deserialize_records(data) else { continue };
        stats.generic_groups += 1;
        stats.generic_records += recs.len();

        // Read typed field values by name-hash for the two classes the oracle also decodes.
        match name.as_str() {
            "HibernationControl" => {
                let nh = schema.fields[0].name_hash;
                for r in &recs {
                    if let Some(FieldValue::U16(d0)) = r.get(nh) {
                        schema_hib.entry(r.entity_key).or_insert(d0);
                    }
                }
            }
            "ModelName" => {
                let nh = schema.fields[0].name_hash;
                for r in &recs {
                    if let Some(FieldValue::U32(h)) = r.get(nh) {
                        schema_model.entry(r.entity_key).or_insert(h);
                    }
                }
            }
            _ => {}
        }
    }

    for (k, h) in &oracle_hib {
        stats.hib_checked += 1;
        if schema_hib.get(k) == Some(&h.dist[0]) {
            stats.hib_agree += 1;
        }
    }
    for (k, h) in &oracle_models {
        stats.model_checked += 1;
        if schema_model.get(k) == Some(h) {
            stats.model_agree += 1;
        }
    }
    stats.classes = reg.len();
    (reg, stats)
}

/// Register the world block's authored `PopulationDensity` regions into the streaming manager's
/// RegionCache (seam B). Each placed `PopulationDensity` entity is joined by key to its `Transform`
/// (the oracle center) and registered as a region anchored at that authored world point.
///
/// GROUNDING / FIDELITY (see the retail-vz.wad survey, 2026-07-07): the placed region COMPs do NOT
/// carry a rect/radius extent on disk — `PopulationDensity`'s `schm` is density params + two name
/// refs + a flags word (no min/max); `LineRegion` is a single ref (its points live in a separate
/// `PgLineRegion` structure); and `SphereRegion`/`CircleRegion` (the only region types that author a
/// radius float, `FUN_0065fe40`/`FUN_0065fee0`) have ZERO placed instances joined to a Transform (they
/// exist only as prototypes in the registry block + are created at runtime by `World.CreateRegion`).
/// So the only grounded region datum is the authored ANCHOR (the Transform center); the extent is a
/// point and the cache in/out band is the streaming tunable [`POPULATION_REGION_CACHE_IN`]/`_OUT`.
/// Fabricating a rect from terrain/heuristics is explicitly declined; the real runtime rect
/// (`FUN_004d60e0` +0x10..+0x1c) is a confirm-live capture.
fn register_population_regions(
    block: &[u8],
    mgr: &mut mercs2_core::streaming::StreamingManager,
) -> usize {
    use mercs2_core::streaming::{Extent2, RegionUnit};

    // Authored centers from the block's Transforms (the placement oracle).
    let mut centers: std::collections::HashMap<u32, [f32; 3]> = std::collections::HashMap::new();
    for p in mercs2_formats::placement::load_placements(block).unwrap_or_default() {
        centers.entry(p.key).or_insert(p.pos);
    }

    let mut n = 0usize;
    for g in walk_comp_groups(block) {
        if g.name.as_deref() != Some("PopulationDensity") {
            continue;
        }
        let Some(schema) = g.schema() else { continue };
        let Some(data) = g.data.as_ref() else { continue };
        let Some(recs) = schema.deserialize_records(data) else { continue };
        for r in &recs {
            let Some(c) = centers.get(&r.entity_key) else { continue };
            mgr.add_region(RegionUnit {
                key: r.entity_key,
                extent: Extent2::from_center_half(c[0], c[2], 0.0), // authored anchor (a point)
                priority: 0,
                cache_in: POPULATION_REGION_CACHE_IN,
                cache_out: POPULATION_REGION_CACHE_OUT,
            });
            n += 1;
        }
    }
    n
}

/// Resolve a vz_state overlay LAYER name to its WAD block, matching the PTHS filename
/// `<layer>_P###_Q#.block` — i.e. the layer name immediately followed by the `_P` quality suffix.
/// This avoids the prefix ambiguity a loose substring ([`find_block_by_path`]) has: `vz_state_pmc`
/// must NOT match `vz_state_pmcinterior_...`. Returns the lowest matching block index, or `None`.
pub fn resolve_overlay_block(w: &wad::Wad, layer: &str) -> Option<u16> {
    let needle = format!("{}_p", layer.to_ascii_lowercase());
    wad::block_paths(w)
        .iter()
        .position(|p| p.to_ascii_lowercase().contains(&needle))
        .map(|i| i as u16)
}

/// Keyed by entity key in the map `build_streaming_catalog` returns, so the streaming executor can
/// instantiate the prop on WAKE.
#[derive(Clone, Copy)]
pub struct PropSpawn {
    pub model_hash: u32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
}

/// Ground height lookup for the third-person walk, built from the SAME triangle data the renderer
/// draws. Two layers:
///  1. EXACT: a triangle spatial hash (TRI_N×TRI_N cells over the terrain's [-4000, 4000]² world
///     extent, ~32 m cells); each triangle is inserted into every cell its XZ AABB overlaps, and
///     lookup does a 2D barycentric point-in-XZ-triangle test, interpolating Y barycentrically.
///  2. FALLBACK: the previous coarse grid (max vertex Y per 512×512 cell, neighbour-dilated,
///     bilinear between cell centres) for (x, z) covered by NO triangle (holes/map edge), so the
///     player never falls through the world.
pub struct HeightMap {
    cells: Vec<f32>,          // coarse fallback grid (max vertex Y per cell, dilated)
    positions: Vec<[f32; 3]>, // terrain vertices (copy of the render data)
    indices: Vec<u32>,        // terrain triangle indices (copy of the render data)
    tri_cells: Vec<Vec<u32>>, // per-cell triangle ids (index/3), by XZ AABB overlap
}

impl HeightMap {
    const N: usize = 512;
    const MIN: f32 = -4000.0;
    const MAX: f32 = 4000.0;
    const TRI_N: usize = 250; // 32 m triangle-hash cells over the same extent

    pub fn build(tm: &mercs2_formats::terrain::TerrainMesh) -> HeightMap {
        let t0 = std::time::Instant::now();
        let n = Self::N;
        let scale = n as f32 / (Self::MAX - Self::MIN);
        let mut cells = vec![f32::NEG_INFINITY; n * n];
        for p in &tm.positions {
            let cx = (((p[0] - Self::MIN) * scale) as usize).min(n - 1);
            let cz = (((p[2] - Self::MIN) * scale) as usize).min(n - 1);
            let c = &mut cells[cz * n + cx];
            *c = c.max(p[1]);
        }
        let mut remaining = cells.iter().filter(|c| !c.is_finite()).count();
        if remaining == n * n {
            cells.fill(0.0); // no terrain verts at all: flat ground, don't dilate forever
            remaining = 0;
        }
        while remaining > 0 {
            let prev = cells.clone();
            for cz in 0..n {
                for cx in 0..n {
                    if prev[cz * n + cx].is_finite() {
                        continue;
                    }
                    let mut best = f32::NEG_INFINITY;
                    for dz in cz.saturating_sub(1)..=(cz + 1).min(n - 1) {
                        for dx in cx.saturating_sub(1)..=(cx + 1).min(n - 1) {
                            best = best.max(prev[dz * n + dx]);
                        }
                    }
                    if best.is_finite() {
                        cells[cz * n + cx] = best;
                        remaining -= 1;
                    }
                }
            }
        }
        // Triangle spatial hash: each triangle goes into every cell its XZ AABB overlaps.
        let tn = Self::TRI_N;
        let tscale = tn as f32 / (Self::MAX - Self::MIN);
        let cell_of = |v: f32| (((v - Self::MIN) * tscale) as isize).clamp(0, tn as isize - 1) as usize;
        let mut tri_cells: Vec<Vec<u32>> = vec![Vec::new(); tn * tn];
        let mut entries = 0usize;
        for (t, tri) in tm.indices.chunks_exact(3).enumerate() {
            let a = tm.positions[tri[0] as usize];
            let b = tm.positions[tri[1] as usize];
            let c = tm.positions[tri[2] as usize];
            let (x0, x1) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
            let (z0, z1) = (a[2].min(b[2]).min(c[2]), a[2].max(b[2]).max(c[2]));
            for cz in cell_of(z0)..=cell_of(z1) {
                for cx in cell_of(x0)..=cell_of(x1) {
                    tri_cells[cz * tn + cx].push(t as u32);
                    entries += 1;
                }
            }
        }
        println!(
            "[world] heightmap: {} tris hashed into {tn}x{tn} cells ({entries} entries) + {n}x{n} fallback in {:.0} ms",
            tm.indices.len() / 3,
            t0.elapsed().as_secs_f64() * 1000.0
        );
        HeightMap {
            cells,
            positions: tm.positions.clone(),
            indices: tm.indices.clone(),
            tri_cells,
        }
    }

    /// Highest Y of any rendered triangle covering world (x, z), by 2D barycentric test in XZ
    /// (edges included, weight epsilon 1e-4; math in f64). With `y_max`, prefers the highest hit
    /// at or below it (overhang/bridge disambiguation), falling back to the highest overall.
    /// `None` when no triangle covers the point.
    fn tri_height_at(&self, x: f32, z: f32, y_max: Option<f32>) -> Option<f32> {
        let tn = Self::TRI_N;
        let tscale = tn as f32 / (Self::MAX - Self::MIN);
        let cell = |v: f32| (((v - Self::MIN) * tscale) as isize).clamp(0, tn as isize - 1) as usize;
        let (px, pz) = (x as f64, z as f64);
        let mut best: Option<f64> = None; // highest overall
        let mut best_near: Option<f64> = None; // highest ≤ y_max
        for &t in &self.tri_cells[cell(z) * tn + cell(x)] {
            let i = t as usize * 3;
            let a = self.positions[self.indices[i] as usize];
            let b = self.positions[self.indices[i + 1] as usize];
            let c = self.positions[self.indices[i + 2] as usize];
            let (ax, az) = (a[0] as f64, a[2] as f64);
            let (bx, bz) = (b[0] as f64, b[2] as f64);
            let (cx, cz) = (c[0] as f64, c[2] as f64);
            let denom = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz);
            if denom.abs() < 1e-9 {
                continue; // degenerate in XZ (vertical / zero-area)
            }
            let w0 = ((bz - cz) * (px - cx) + (cx - bx) * (pz - cz)) / denom;
            let w1 = ((cz - az) * (px - cx) + (ax - cx) * (pz - cz)) / denom;
            let w2 = 1.0 - w0 - w1;
            const EPS: f64 = 1e-4;
            if w0 < -EPS || w1 < -EPS || w2 < -EPS {
                continue;
            }
            let y = w0 * a[1] as f64 + w1 * b[1] as f64 + w2 * c[1] as f64;
            if best.map_or(true, |v| y > v) {
                best = Some(y);
            }
            if let Some(limit) = y_max {
                if y <= limit as f64 && best_near.map_or(true, |v| y > v) {
                    best_near = Some(y);
                }
            }
        }
        (if y_max.is_some() { best_near.or(best) } else { best }).map(|y| y as f32)
    }

    /// Ground height at world (x, z): exact triangle sample (highest covering triangle), with the
    /// coarse grid as fallback where no triangle covers the point.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.tri_height_at(x, z, None)
            .unwrap_or_else(|| self.coarse_height_at(x, z))
    }

    /// Like `height_at`, but prefers the highest triangle at or below `y_hint + 2.0` so a player
    /// standing UNDER a bridge/overhang isn't teleported on top of it.
    pub fn height_at_near(&self, x: f32, z: f32, y_hint: f32) -> f32 {
        self.tri_height_at(x, z, Some(y_hint + 2.0))
            .unwrap_or_else(|| self.coarse_height_at(x, z))
    }

    /// Coarse-grid ground height at world (x, z): bilinear blend of the four nearest cell centres.
    fn coarse_height_at(&self, x: f32, z: f32) -> f32 {
        let n = Self::N;
        let scale = n as f32 / (Self::MAX - Self::MIN);
        let fx = ((x - Self::MIN) * scale - 0.5).clamp(0.0, (n - 1) as f32);
        let fz = ((z - Self::MIN) * scale - 0.5).clamp(0.0, (n - 1) as f32);
        let (x0, z0) = (fx as usize, fz as usize);
        let (x1, z1) = ((x0 + 1).min(n - 1), (z0 + 1).min(n - 1));
        let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);
        let h = |cx: usize, cz: usize| self.cells[cz * n + cx];
        let a = h(x0, z0) * (1.0 - tx) + h(x1, z0) * tx;
        let b = h(x0, z1) * (1.0 - tx) + h(x1, z1) * tx;
        a * (1.0 - tz) + b * tz
    }
}

/// MERCS2_HMAP_VERIFY: numeric evidence for the exact triangle sampler.
///  - old-vs-new sweep on a 25 m grid (max |coarse − exact| + 5 worst points),
///  - exactness on 1000 deterministic-random triangle centroids (barycentric hit must reproduce
///    the centroid Y unless a HIGHER overlapping triangle covers it).
pub fn verify_heightmap(hmap: &HeightMap) {
    // Old vs new sweep.
    let mut worst: Vec<(f32, f32, f32, f32, f32)> = Vec::new(); // (|d|, x, z, old, new)
    for iz in 0..=320 {
        for ix in 0..=320 {
            let x = HeightMap::MIN + ix as f32 * 25.0;
            let z = HeightMap::MIN + iz as f32 * 25.0;
            let old = hmap.coarse_height_at(x, z);
            let new = hmap.height_at(x, z);
            let d = (old - new).abs();
            worst.push((d, x, z, old, new));
            worst.sort_by(|a, b| b.0.total_cmp(&a.0));
            worst.truncate(5);
        }
    }
    println!("[hmap-verify] old-vs-new on 321x321 grid (25 m step): max |old-new| = {:.3}", worst[0].0);
    for (d, x, z, old, new) in &worst {
        println!("[hmap-verify]   worst: ({x:.0}, {z:.0}) old={old:.3} new={new:.3} |d|={d:.3}");
    }
    println!(
        "[hmap-verify] h(0,0): old={:.4} new={:.4}",
        hmap.coarse_height_at(0.0, 0.0),
        hmap.height_at(0.0, 0.0)
    );
    // Centroid exactness.
    let ntris = hmap.indices.len() / 3;
    let (mut exact, mut higher, mut miss, mut degen) = (0u32, 0u32, 0u32, 0u32);
    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..1000 {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let i = ((rng >> 33) as usize % ntris) * 3;
        let a = hmap.positions[hmap.indices[i] as usize];
        let b = hmap.positions[hmap.indices[i + 1] as usize];
        let c = hmap.positions[hmap.indices[i + 2] as usize];
        let denom = (b[2] as f64 - c[2] as f64) * (a[0] as f64 - c[0] as f64)
            + (c[0] as f64 - b[0] as f64) * (a[2] as f64 - c[2] as f64);
        if denom.abs() < 1e-9 {
            degen += 1; // XZ-degenerate: sampler skips these by design
            continue;
        }
        let cxz = [(a[0] + b[0] + c[0]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
        let cy = (a[1] as f64 + b[1] as f64 + c[1] as f64) / 3.0;
        let h = hmap.height_at(cxz[0], cxz[1]) as f64;
        if (h - cy).abs() <= 1e-3 {
            exact += 1;
        } else if h > cy + 1e-3 {
            higher += 1;
        } else {
            miss += 1;
            println!(
                "[hmap-verify]   MISS tri {} centroid ({:.2}, {:.2}) cy={cy:.4} h={h:.4}",
                i / 3, cxz[0], cxz[1]
            );
        }
    }
    println!(
        "[hmap-verify] centroids: {exact} within 1e-3, {higher} higher-overlap won, {miss} MISSES, {degen} degenerate-skipped (of 1000)"
    );
}

/// The exterior pool/back-door spawn (the `--props` centre; matches the default player spawn).
pub const EXTERIOR_SPAWN: [f32; 3] = [2560.2646, -13.1779, -926.2511];

/// c3 streaming-cell grid (ported from `game-scripts/mercs2_c3_grid.py`, GRID_LOGIC_VERSION 3):
/// `c3####` names are linear slots (base 30001) in a 100×100 grid over game-world X/Z
/// [-3900, 3850]; cell centre = min + (col|row + 0.5) · (7750 / 100).
pub const C3_CELL_ID_BASE: u32 = 30001;
pub const C3_GRID_COLS: u32 = 100;
pub const C3_WORLD_MIN: f32 = -3900.0;
pub const C3_CELL_SIZE: f32 = (3850.0 - C3_WORLD_MIN) / C3_GRID_COLS as f32; // 77.5 m

/// First `c3` + four digits in a block path → streaming cell id (c30123 ⇒ 30123).
pub fn c3_cell_id_from_path(path: &str) -> Option<u32> {
    let b = path.as_bytes();
    for i in 0..b.len().saturating_sub(5) {
        if (b[i] == b'c' || b[i] == b'C')
            && b[i + 1] == b'3'
            && b[i + 2..i + 6].iter().all(|c| c.is_ascii_digit())
        {
            let slot: u32 = path[i + 2..i + 6].parse().ok()?;
            return Some(C3_CELL_ID_BASE - 1 + slot);
        }
    }
    None
}

/// Game-space (x, z) centre of a streaming cell (metres). Grid carries no height.
pub fn c3_cell_centre(cell_id: u32) -> (f32, f32) {
    let linear = cell_id.saturating_sub(C3_CELL_ID_BASE);
    let (row, col) = (linear / C3_GRID_COLS, linear % C3_GRID_COLS);
    let x = C3_WORLD_MIN + (col as f32 + 0.5) * C3_CELL_SIZE;
    let z = C3_WORLD_MIN + (row as f32 + 0.5) * C3_CELL_SIZE;
    (x, z)
}


/// Interior STATE/placement overlay (`vz_state_pmcinterior_P000_Q3.block`): 104 Transform records,
/// authored around the spawn (floor Y≈450.8), each keying a named interior instance (cots, planters,
/// wardrobe, sickbay, lamps, generator, …) plus the room-shell (`pmcoutpost_bld_*`) meshes.
pub const PMC_INTERIOR_STATE_BLOCK: u16 = 667;

/// The KEYED PMC-interior entities from `docs/mercs2-luacd/src/vz/wifpmcinterior.lua` (`_tBuildings`
/// + the recruit-interior variants): `(entity_key, canonical_name)`. Each entity's AUTHORED world
/// Transform + its `ModelName` mesh live in one of the interior-candidate overlay blocks; the name is
/// the `pandemic_hash_m2` fallback when a key has a Transform but no ModelName record.
pub const PMC_INTERIOR_ENTITIES: &[(u32, &str)] = &[
    (0x000d3c77, "_pmcoutpost_bld_hq_livedin"),
    (0x000d3c78, "_pmcoutpost_bld_hqgarage_livedin"),
    (0x000cf8c2, "_pmcoutpost_bld_hqsuites"),
    (0x000c73ec, "_pmcoutpost_interior_recruitheli"),
    (0x000c740d, "_pmcoutpost_interior_recruitjet"),
    (0x000c73ee, "_pmcoutpost_interior_recruitmechanic"),
];

#[cfg(test)]
mod schema_wire_tests {
    use super::*;

    /// Live end-to-end proof that the E1 schema deserializer is wired into the world-load path and
    /// that the S5 RegionCache is populated (seams A + B). SKIPS (passes) when vz.wad is absent so CI
    /// stays green — matching the other live tests in this workspace.
    ///
    /// Asserts, against the real retail `layers_static` block:
    ///   1. the schema path deserializes many generic COMP records (≥ 2000) across many classes,
    ///   2. every value it reads for HibernationControl dist0 + ModelName hash AGREES with the bespoke
    ///      `placement` oracle (no drift on the overlap),
    ///   3. ≥ 1 population region registers into the streaming manager's RegionCache, and driving
    ///      `update_regions` at a region's anchor caches that region IN.
    #[test]
    fn live_schema_and_region_wire_if_wad_present() {
        let path = std::env::var("VZ_WAD").unwrap_or_else(|_| {
            "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad".into()
        });
        if std::fs::metadata(&path).is_err() {
            eprintln!("skip: vz.wad not present at {path}");
            return;
        }
        let mut w = crate::wad::open(&path).expect("open vz.wad");
        let (_low, ls) = find_terrain_blocks(&mut w).expect("terrain blocks");

        // Seam A — schema-driven generic COMP deserialize + oracle agreement.
        let (reg, stats) = load_schema_components(&ls);
        println!(
            "[schema-test] classes={} generic_groups={} generic_records={} | HibernationControl {}/{} agree, ModelName {}/{} agree | pool_budget_total={}",
            stats.classes, stats.generic_groups, stats.generic_records,
            stats.hib_agree, stats.hib_checked, stats.model_agree, stats.model_checked, reg.total_budget()
        );
        assert!(stats.classes >= 10, "expected many registered classes, got {}", stats.classes);
        assert!(
            stats.generic_records >= 2000,
            "expected ≥2000 generic COMP records deserialized, got {}",
            stats.generic_records
        );
        // The schema path must agree with the oracle exactly where they overlap.
        assert!(stats.hib_checked > 0 && stats.hib_agree == stats.hib_checked,
            "HibernationControl dist0 disagreed with oracle: {}/{}", stats.hib_agree, stats.hib_checked);
        assert!(stats.model_checked > 0 && stats.model_agree == stats.model_checked,
            "ModelName hash disagreed with oracle: {}/{}", stats.model_agree, stats.model_checked);

        // Descriptor lookups resolve real classes registered from schm.
        assert!(reg.get_by_name("HibernationControl").is_some(), "HibernationControl not registered");
        assert!(reg.get_by_name("PopulationDensity").is_some(), "PopulationDensity not registered");

        // Seam B — region cache populated from PopulationDensity anchors, and actually driven.
        let mut mgr = mercs2_core::streaming::StreamingManager::new(
            mercs2_core::streaming::StreamingConfig::default(),
        );
        let n_regions = register_population_regions(&ls, &mut mgr);
        println!("[schema-test] region cache: {n_regions} PopulationDensity anchors registered");
        assert!(n_regions >= 1, "expected ≥1 population region registered");
        assert_eq!(mgr.region_count(), n_regions);

        // Drive the decision layer at an anchor: it must cache that region IN.
        let anchor = first_population_anchor(&ls).expect("a population anchor");
        let diff = mgr.update_regions(anchor);
        assert!(
            !diff.cache_in.is_empty() || mgr.cached_region_count() >= 1,
            "driving update_regions at an anchor should cache ≥1 region in"
        );
        println!("[schema-test] update_regions@anchor -> cached {}/{}", mgr.cached_region_count(), mgr.region_count());
    }

    /// The world position of the first `PopulationDensity` region's authored Transform anchor.
    fn first_population_anchor(block: &[u8]) -> Option<[f32; 3]> {
        let mut centers: std::collections::HashMap<u32, [f32; 3]> = std::collections::HashMap::new();
        for p in mercs2_formats::placement::load_placements(block).unwrap_or_default() {
            centers.entry(p.key).or_insert(p.pos);
        }
        for g in walk_comp_groups(block) {
            if g.name.as_deref() != Some("PopulationDensity") {
                continue;
            }
            let schema = g.schema()?;
            let data = g.data.as_ref()?;
            let recs = schema.deserialize_records(data)?;
            for r in &recs {
                if let Some(c) = centers.get(&r.entity_key) {
                    return Some(*c);
                }
            }
        }
        None
    }
}
