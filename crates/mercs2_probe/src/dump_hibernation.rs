//! `dump-hibernation` — the retail hibernation grid a v1.0 capture holds (`docs/reverse_engineer/
//! hibernation_grid.md`): grid header `0x0179C7B0`, viewers `0x0179C800`, cell blocks (0x1F0 bytes,
//! base `[0x01175DD8]`) found through each SceneObject's block index (its `+0x18` low 12 bits).
//! Prints the viewers and a summary; the retail tests check `mercs2_core::hibernation` and the
//! engine's placed-entity catalog against every row.

use mercs2_core::hibernation::{GridEntry, HibernationScale, Quantized, Viewer, MAX_VIEWERS};

use crate::dump::{Capture, Mem};

const GRID: u32 = 0x0179_C7B0;
const VIEWER_POS: u32 = 0x0179_C800;
const BLOCK_BASE: u32 = 0x0117_5DD8;
const BLOCK_SIZE: u32 = 0x1F0;
const NOT_IN_GRID: u16 = 0x0FFF;

/// The grid as captured.
pub struct LiveGrid {
    /// `[grid+0x38]`, `[0xD1BF34]`, `[0x1175DC8]`, `[0x1175DC4]`.
    pub scale: HibernationScale,
    pub viewers: [Option<Viewer>; MAX_VIEWERS],
    /// Every row, with its entry exactly as stored (state included).
    pub rows: Vec<GridEntry>,
}

fn f32_at(mem: &Mem, va: u32) -> Result<f32, String> {
    mem.u32(va).map(f32::from_bits)
}

pub fn live_grid(mem: &Mem) -> Result<LiveGrid, String> {
    let scale = HibernationScale {
        k: f32_at(mem, GRID + 0x38)?,
        inv_k: f32_at(mem, 0x00D1_BF34)?,
        radius_factor: f32_at(mem, 0x0117_5DC8)?,
        min_distance: f32_at(mem, 0x0117_5DC4)?,
    };
    let margin = f32_at(mem, GRID + 0x3C)?;
    if margin != mercs2_core::hibernation::MARGIN {
        return Err(format!("grid margin {margin}, expected {}", mercs2_core::hibernation::MARGIN));
    }
    let count = mem.u16(GRID + 0xA)? as i16;
    let mut viewers = [None; MAX_VIEWERS];
    for i in 0..count.max(0) as u32 {
        let v = mem.u8(GRID + 0x44 + i)? as usize;
        if v >= MAX_VIEWERS {
            return Err(format!("viewer id {v} out of range"));
        }
        let p = VIEWER_POS + 12 * v as u32;
        viewers[v] = Some(Viewer {
            position: [f32_at(mem, p)?, f32_at(mem, p + 4)?, f32_at(mem, p + 8)?],
            local: mem.u8(GRID + 0x40 + v as u32)? != 0,
        });
    }
    let so = mem.container_named("SceneObject")?;
    let base = mem.u32(BLOCK_BASE)?;
    let mut blocks = std::collections::BTreeSet::new();
    for r in mem.records(&so)? {
        let idx = mem.u16(r.va + 0x18)? & 0x0FFF;
        if idx != NOT_IN_GRID {
            blocks.insert(idx);
        }
    }
    let mut rows = Vec::new();
    for idx in blocks {
        let b = base + u32::from(idx) * BLOCK_SIZE;
        let blk = mem.read(b, BLOCK_SIZE as usize)?;
        let rd16 = |o: usize| u16::from_le_bytes([blk[o], blk[o + 1]]);
        let n = rd16(0x1E4) as i16;
        let cell = i32::from(rd16(0x1E6) as i16);
        let statics = u32::from_le_bytes([blk[0x1E0], blk[0x1E1], blk[0x1E2], blk[0x1E3]]);
        for i in 0..n.max(0) as usize {
            let packed = rd16(0xA0 + 4 * i + 2);
            rows.push(GridEntry {
                key: u32::from_le_bytes([blk[4 * i], blk[4 * i + 1], blk[4 * i + 2], blk[4 * i + 3]]),
                q: Quantized { cell, bx: blk[0xA0 + 4 * i], bz: blk[0xA1 + 4 * i], ylo: blk[0x120 + i], yhi: (packed >> 14) as u8 },
                r14: packed & 0x3FFF,
                rings: [blk[0x140 + 3 * i], blk[0x141 + 3 * i], blk[0x142 + 3 * i]],
                base: rd16(0x1A0 + 2 * i),
                is_static: statics >> i & 1 != 0,
                state: blk[0x80 + i],
            });
        }
    }
    Ok(LiveGrid { scale, viewers, rows })
}

pub fn dump_hibernation(capture: &str) -> Result<(), String> {
    let cap = Capture::open(std::path::Path::new(capture))?;
    let mem = cap.mem()?;
    let g = live_grid(&mem)?;
    println!("[dump-hibernation] {} (build {}): {} grid rows, scale {:?}", cap.path.display(), cap.build_id, g.rows.len(), g.scale);
    for (v, vw) in g.viewers.iter().enumerate() {
        if let Some(vw) = vw {
            println!("viewer {v}: ({:.3}, {:.3}, {:.3}) local {}", vw.position[0], vw.position[1], vw.position[2], vw.local);
        }
    }
    let awake = g.rows.iter().filter(|r| r.state & 0x0F != 0).count();
    println!("awake (mask set): {awake}");
    Ok(())
}

#[cfg(all(test, feature = "retail"))]
mod tests {
    use super::*;
    use mercs2_core::hibernation::{evaluate, quantize, HibernationScale, DEFAULT_DISTANCE};
    use std::path::Path;

    /// In every `updated-chris-exports` capture: the scale is single-player; re-running the job on each
    /// row's own inputs reproduces its stored state; each row's radius follows the radius rule from the
    /// live `HibernationControl` (own, else the template's, else 100); and each entity with no render
    /// node quantises to its row exactly.
    #[test]
    fn the_core_reproduces_every_captured_grid_row() {
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dir = mercs2_formats::game_paths::local_config_memory_dumps_dir(start)
            .unwrap_or_else(|e| panic!("{e}"))
            .join("updated-chris-exports");
        let mut total = 0;
        for name in [
            "20261021_chris-jacobs-flying-winch.dmp",
            "20261021_chris-jacobs-mid-mission.dmp",
            "20261021_clean-load-chris-jacobs-7-percent.dmp",
            "20261021_mattias-nilson-jacket-skin.dmp",
        ] {
            let cap = Capture::open(&dir.join(name)).unwrap_or_else(|e| panic!("{e}"));
            let mem = cap.mem().unwrap_or_else(|e| panic!("{e}"));
            let g = live_grid(&mem).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(g.scale, HibernationScale::single_player(), "{name}: single-player scale");
            assert!(g.rows.len() > 70_000, "{name}: {} rows", g.rows.len());

            let so = mem.container_named("SceneObject").unwrap();
            let so_va: std::collections::HashMap<u32, u32> = mem.records(&so).unwrap().into_iter().map(|r| (r.key, r.va)).collect();
            let hc = mem.container_named("HibernationControl").unwrap();
            let hc_va: std::collections::HashMap<u32, u32> = mem.records(&hc).unwrap().into_iter().map(|r| (r.key, r.va)).collect();
            let rso = mem.container_named("RuntimeSceneObject").unwrap();
            let node: std::collections::HashSet<u32> = mem
                .records(&rso)
                .unwrap()
                .into_iter()
                .filter(|r| mem.u32(r.va + 0x34).unwrap() != 0)
                .map(|r| r.key)
                .collect();

            let (mut quantized, mut with_node, mut runtime_moved, mut unvisited_spawns) = (0, 0, 0, 0);
            let mut placed_misses = Vec::new();
            for row in &g.rows {
                // A masked state with no tier only comes from a spawn's insert (`0x516C00` computes the
                // mask alone); the job always adds a tier for a local viewer, so such a row has not been
                // through the job yet.
                if row.state & 0x0F != 0 && row.state & 0xF0 == 0 && g.viewers.iter().flatten().all(|v| v.local) {
                    assert!(row.key >= 0x1000_0000, "{name}: placed key 0x{:08X} in a spawn-only state {row:?}", row.key);
                    unvisited_spawns += 1;
                } else {
                    assert_eq!(evaluate(row, &g.viewers, &g.scale), row.state, "{name}: key 0x{:08X} {row:?}", row.key);
                }
                let so = so_va[&row.key];
                let hc = hc_va.get(&row.key).or_else(|| hc_va.get(&mem.u32(so + 0x14).unwrap()));
                let dist0 = hc.map_or(DEFAULT_DISTANCE, |&a| mem.u16(a).unwrap());
                assert_eq!(row.r14, g.scale.radius(dist0, row.is_static), "{name}: key 0x{:08X} radius of distance {dist0}", row.key);
                if node.contains(&row.key) {
                    with_node += 1;
                    continue;
                }
                let pos = [f32_at(&mem, so).unwrap(), f32_at(&mem, so + 4).unwrap(), f32_at(&mem, so + 8).unwrap()];
                if quantize(pos) == row.q {
                    quantized += 1;
                } else if row.key >= 0x1000_0000 {
                    // A runtime entity moved since its row was last encoded.
                    runtime_moved += 1;
                } else {
                    placed_misses.push((row.key, pos, row.q));
                }
            }
            eprintln!(
                "{name}: {} rows, {quantized} re-quantised, {with_node} with a render node, {runtime_moved} runtime moved, {unvisited_spawns} spawns not yet visited",
                g.rows.len()
            );
            assert!(unvisited_spawns <= 2, "{name}: {unvisited_spawns} spawn-only rows");
            let y_only = placed_misses.iter().filter(|(_, p, q)| {
                let mine = quantize(*p);
                (mine.cell, mine.bx, mine.bz) == (q.cell, q.bx, q.bz)
            }).count();
            let ever_awake = placed_misses.iter().filter(|(k, _, _)| g.rows.iter().any(|r| r.key == *k && r.state != 0)).count();
            eprintln!("{name}: {} placed misses, {y_only} only in y, {ever_awake} currently masked/ringed", placed_misses.len());
            // A row encoded while its entity had a render node holds the node's centre and radius (base
            // ≠ 0), not the SceneObject position. In the two captures of a fresh session every placed
            // entity off its row is such a row; the long sessions also hold rows of entities that were
            // awake (or moved) earlier, which a snapshot cannot replay.
            let base_of: std::collections::HashMap<u32, u16> = g.rows.iter().map(|r| (r.key, r.base)).collect();
            let no_node: Vec<u32> = placed_misses.iter().filter(|(k, _, _)| base_of[k] == 0).map(|(k, _, _)| *k).collect();
            eprintln!("{name}: placed misses with base 0 (no render node at encode): {}", no_node.len());
            if name.contains("clean-load") || name.contains("mattias") {
                assert!(no_node.is_empty(), "{name}: placed entities off their row with no render node: {no_node:08X?}");
            }
            total += g.rows.len();
        }
        assert!(total > 280_000);
    }

    /// Every live posed model node (`0xBAB0F0`) of a placed entity: its node-enable table `+0x2A0`
    /// against `render_node::activation_enable` from the shipped model. Every one of the two fresh
    /// captures matches; the long sessions also hold nodes whose machine moved on since activation.
    #[test]
    fn live_node_enables_follow_the_activation_rule() {
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        let wad_path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        let mut w = mercs2_engine::wad::open(&wad_path.to_string_lossy()).unwrap_or_else(|e| panic!("{e}"));
        let mut tables: std::collections::HashMap<u32, Vec<bool>> = std::collections::HashMap::new();
        let dir = mercs2_formats::game_paths::local_config_memory_dumps_dir(start)
            .unwrap_or_else(|e| panic!("{e}"))
            .join("updated-chris-exports");
        for name in [
            "20261021_chris-jacobs-flying-winch.dmp",
            "20261021_chris-jacobs-mid-mission.dmp",
            "20261021_clean-load-chris-jacobs-7-percent.dmp",
            "20261021_mattias-nilson-jacket-skin.dmp",
        ] {
            let cap = Capture::open(&dir.join(name)).unwrap_or_else(|e| panic!("{e}"));
            let mem = cap.mem().unwrap_or_else(|e| panic!("{e}"));
            let rso = mem.container_named("RuntimeSceneObject").unwrap();
            let (mut ok, mut bad) = (0, Vec::new());
            for r in mem.records(&rso).unwrap().into_iter().filter(|r| r.key < 0x1000_0000) {
                let node = mem.u32(r.va + 0x34).unwrap();
                if node == 0 || mem.u32(node).unwrap() != 0x00BA_B0F0 {
                    continue;
                }
                let model_res = mem.u32(node + 0x1E0).unwrap();
                let (model, n) = (mem.u32(model_res + 4).unwrap(), mem.u32(model_res + 0x5C).unwrap() as usize);
                let table = match tables.get(&model) {
                    Some(t) => t.clone(),
                    None => {
                        let c = mercs2_engine::wad::extract_container(&mut w, model).unwrap_or_else(|e| panic!("{e}"));
                        let t = mercs2_formats::render_node::activation_enable(&c).unwrap_or_else(|e| panic!("model 0x{model:08X}: {e}"));
                        tables.insert(model, t.clone());
                        t
                    }
                };
                let live: Vec<bool> = mem.read(mem.u32(node + 0x2A0).unwrap(), n).unwrap().iter().map(|&b| b != 0).collect();
                if live == table {
                    ok += 1;
                } else {
                    bad.push((r.key, model));
                }
            }
            eprintln!("{name}: {ok} placed posed nodes match the activation enable table, {} do not: {:08X?}", bad.len(), &bad[..bad.len().min(12)]);
            if name.contains("clean-load") || name.contains("mattias") {
                assert!(bad.is_empty(), "{name}: live enable tables off the activation rule: {bad:08X?}");
            }
        }
    }

    /// The engine's placed-entity catalog (`mercs2_engine::worldutil::PlacedCatalog`) over every placement
    /// block, against every placed row of each capture:
    /// - each placed row's key is in the catalog (one catalog holds all 749 blocks: no key is placed
    ///   twice), with the static bit, radius and rings its registration gives;
    /// - a row with base 10000 is a TerrainObject never activated, at its registered position;
    /// - a row with another non-zero base is the re-cell of a completed activation: the catalog's node
    ///   source and `NodeSpheres` reproduce its position and base. Every such row of the two fresh
    ///   captures does; the long sessions also hold rows of entities that moved or changed state since,
    ///   which the shipped data cannot give;
    /// - every live `HibernationControl` of a placed key with a shipped record equals it;
    /// - no name resolves a model the shipped records (own or template ModelName) do not give.
    #[test]
    fn placed_rows_follow_the_shipped_records() {
        use mercs2_engine::worldutil::{NodeSource, NodeSpheres, PlacedCatalog, TemplateRecords};
        use std::collections::{BTreeMap, HashMap};

        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        let wad_path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        let mut assets = mercs2_engine::asset::AssetSource::discover(&wad_path.to_string_lossy(), &[]).unwrap_or_else(|e| panic!("{e}"));
        let templates = TemplateRecords::load(&mut assets).unwrap_or_else(|e| panic!("{e}"));

        let mut w = mercs2_engine::wad::open(&wad_path.to_string_lossy()).unwrap_or_else(|e| panic!("{e}"));
        let mut catalog = PlacedCatalog::default();
        let (mut blocks, mut named, mut name_only) = (0, 0, Vec::new());
        for b in 0..=mercs2_engine::worldutil::MAX_PLACEMENT_BLOCK {
            let dec = mercs2_engine::wad::decompress_block_index(&mut w, b).unwrap_or_else(|e| panic!("block {b}: {e}"));
            if catalog.add_block(&dec, &templates).unwrap_or_else(|e| panic!("block {b}: {e}")).is_empty() {
                continue;
            }
            blocks += 1;
            // The name-hash route the old streamer used: a name whose hash is a model, where the shipped
            // records give none.
            for p in mercs2_formats::placement::load_placements(&dec).unwrap_or_else(|e| panic!("block {b}: {e}")) {
                let Some(n) = p.name else { continue };
                named += 1;
                let h = mercs2_formats::hash::pandemic_hash_m2(n.trim_start_matches('_'));
                if catalog.get(p.key).is_some_and(|o| o.model.is_none()) && mercs2_engine::wad::has_model(&w, h) {
                    name_only.push((p.key, n));
                }
            }
        }
        eprintln!("{blocks} placement blocks, {} placed entities; {named} named, {} whose name hashes to a model the records do not give", catalog.len(), name_only.len());
        assert!(name_only.is_empty(), "names resolving a model the records do not give: {:?}", &name_only[..name_only.len().min(20)]);

        let mut spheres = NodeSpheres::default();
        let dir = mercs2_formats::game_paths::local_config_memory_dumps_dir(start)
            .unwrap_or_else(|e| panic!("{e}"))
            .join("updated-chris-exports");
        for name in [
            "20261021_chris-jacobs-flying-winch.dmp",
            "20261021_chris-jacobs-mid-mission.dmp",
            "20261021_clean-load-chris-jacobs-7-percent.dmp",
            "20261021_mattias-nilson-jacket-skin.dmp",
        ] {
            let cap = Capture::open(&dir.join(name)).unwrap_or_else(|e| panic!("{e}"));
            let mem = cap.mem().unwrap_or_else(|e| panic!("{e}"));
            let g = live_grid(&mem).unwrap_or_else(|e| panic!("{name}: {e}"));
            let hc = mem.container_named("HibernationControl").unwrap();
            let live_hc: HashMap<u32, u32> = mem.records(&hc).unwrap().into_iter().map(|r| (r.key, r.va)).collect();

            let mut node_rows: BTreeMap<String, (usize, Vec<u32>)> = BTreeMap::new();
            let (mut rows, mut terrain_unactivated, mut runtime_hc) = (0, 0, 0);
            for row in g.rows.iter().filter(|r| r.key < 0x1000_0000) {
                rows += 1;
                let o = *catalog.get(row.key).unwrap_or_else(|| panic!("{name}: placed key 0x{:08X} is in no placement block", row.key));
                let reg = o.registration();
                assert!(
                    row.is_static == reg.is_static && row.r14 == g.scale.radius(reg.distance, reg.is_static) && row.rings == reg.rings,
                    "{name}: key 0x{:08X} row {row:?} vs registration {reg:?}",
                    row.key
                );
                if let Some(&va) = live_hc.get(&row.key) {
                    // Byte +5 holds only the two schema bits; the rest of it is not written.
                    let live = (mem.u16(va).unwrap(), [mem.u8(va + 2).unwrap(), mem.u8(va + 3).unwrap(), mem.u8(va + 4).unwrap()], mem.u8(va + 5).unwrap() & 3);
                    match o.own_hibernation {
                        Some(h) => assert_eq!(
                            (h.distance, h.rings(), u8::from(h.flags) | u8::from(h.no_del_on_hibernate_if_preplaced) << 1),
                            live,
                            "{name}: key 0x{:08X} live HibernationControl differs from its shipped record",
                            row.key
                        ),
                        None => runtime_hc += 1,
                    }
                }
                if row.base == 10_000 {
                    assert!(o.terrain_mesh.is_some() && quantize(reg.position) == row.q, "{name}: key 0x{:08X} base 10000 {row:?} {o:?}", row.key);
                    terrain_unactivated += 1;
                    continue;
                }
                if row.base == 0 {
                    continue;
                }
                let kind = match o.node {
                    NodeSource::None => "none".to_string(),
                    NodeSource::Model { tiny_geometry: true, .. } => "tinygeometry".to_string(),
                    NodeSource::Model { tiny_geometry: false, .. } => "posed model".to_string(),
                    NodeSource::LowResTile(_) => "low-res tile".to_string(),
                    NodeSource::Light(l, _) => format!("light type {}", l.light_type),
                };
                let node = spheres.sphere(&mut w, o.node).unwrap_or_else(|e| panic!("{name}: key 0x{:08X}: {e}", row.key));
                let r = o.activated_registration(node);
                let e = node_rows.entry(kind).or_default();
                if node.is_some() && quantize(r.position) == row.q && r.base == row.base {
                    e.0 += 1;
                } else {
                    e.1.push(row.key);
                }
            }
            eprintln!("{name}: {rows} placed rows, {terrain_unactivated} TerrainObjects never activated, {runtime_hc} live HibernationControl records on placed keys that ship none");
            for (kind, (ok, bad)) in &node_rows {
                eprintln!("{name}:   {kind}: {ok} re-celled rows reproduced, {} not: {:08X?}", bad.len(), &bad[..bad.len().min(12)]);
            }
            let missed: usize = node_rows.values().map(|(_, bad)| bad.len()).sum();
            if name.contains("clean-load") || name.contains("mattias") {
                assert_eq!(missed, 0, "{name}: re-celled rows the catalog does not reproduce");
            }
        }
    }
}
