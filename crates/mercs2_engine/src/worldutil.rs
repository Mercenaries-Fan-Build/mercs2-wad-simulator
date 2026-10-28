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

/// Hash-name resolution from the rainbow table at `table` (dev callers pass the `rainbow_table` key
/// of `.mercs2-local.toml`). Returns hash -> first candidate name for the asked-for hashes the table
/// names; a hash the table does not name is absent from the map. An unreadable table is an error
/// naming its path.
pub fn rainbow_names(
    table: &std::path::Path,
    hashes: &std::collections::BTreeSet<u32>,
) -> Result<std::collections::HashMap<u32, String>, String> {
    let mut out = std::collections::HashMap::new();
    let text = std::fs::read_to_string(table)
        .map_err(|e| format!("read rainbow table {}: {e}", table.display()))?;
    for &h in hashes {
        let key = format!("\"0x{h:08X}\"");
        let Some(p) = text.find(&key) else { continue };
        let rest = &text[p + key.len()..];
        let Some(q0) = rest.find('"') else { continue };
        let Some(q1) = rest[q0 + 1..].find('"') else { continue };
        out.insert(h, rest[q0 + 1..q0 + 1 + q1].to_string());
    }
    Ok(out)
}

/// An asset named on a command line: `0x`-prefixed hex is the hash itself, anything else is a name
/// and is hashed with `pandemic_hash_m2`. Unlike [`parse_hash`], a bare word is never read as hex —
/// `dead` is a name.
pub fn hash_or_name(s: &str) -> Result<u32, String> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).map_err(|e| format!("{s:?} is not a hex hash: {e}")),
        None => Ok(mercs2_formats::hash::pandemic_hash_m2(s)),
    }
}

/// hash -> name from the ASET export (`docs/data/aset_names.csv`, written by `aset_export`): its
/// first column is the hash, its second the name, empty for a hash nobody has named — those are
/// absent from the map. Every row after the header must start with a `0x` hash; one that does not
/// is an error naming its line.
pub fn aset_names(csv: &std::path::Path) -> Result<std::collections::HashMap<u32, String>, String> {
    let text = std::fs::read_to_string(csv).map_err(|e| format!("read {}: {e}", csv.display()))?;
    let mut out = std::collections::HashMap::new();
    for (i, line) in text.lines().enumerate().skip(1) {
        let mut f = line.split(',');
        let col0 = f.next().unwrap_or("");
        let h = col0
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| format!("{}:{}: {col0:?} is not a 0x hash", csv.display(), i + 1))?;
        if let Some(n) = f.next().filter(|n| !n.is_empty()) {
            out.insert(h, n.to_string());
        }
    }
    Ok(out)
}

/// [`aset_names`] for a **dev tool**: the CSV under `data/` of the `notes_docs_dir` named by the
/// nearest `.mercs2-local.toml` above `start`.
pub fn dev_aset_names(start: &std::path::Path) -> Result<std::collections::HashMap<u32, String>, String> {
    let docs = mercs2_formats::game_paths::local_config_notes_docs_dir(start)?;
    aset_names(&docs.join("data").join("aset_names.csv"))
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

/// The worldentity template records a placed entity falls back to through its `Transform.Parent`, and
/// a runtime spawn is made from.
pub struct TemplateRecords {
    /// Every key a worldentity record names.
    pub keys: std::collections::BTreeSet<u32>,
    /// The `Name` records (6,126 in retail, one key each), in data order: what the name index holds
    /// before any layer loads.
    pub names: Vec<(u32, String)>,
    pub hibernation: std::collections::BTreeMap<u32, mercs2_formats::hibernation_control::HibernationControl>,
    pub models: std::collections::BTreeMap<u32, u32>,
    /// Templates with a `TinyGeometryObject` record.
    pub tiny_geometry: std::collections::BTreeSet<u32>,
    pub lights: std::collections::BTreeMap<u32, mercs2_formats::placement::LightObject>,
    pub light_scales: std::collections::BTreeMap<u32, mercs2_formats::render_node::LightScales>,
    /// `NetCategoryInfo` (the packed u16): `Pg.Spawn` reads its low nibble and bit 9.
    pub net_category: std::collections::BTreeMap<u32, u16>,
    /// The archetype each template's physics component gives it (templates without one are props).
    pub archetypes: std::collections::BTreeMap<u32, crate::spawn::Archetype>,
    /// `Association`: the faction template.
    pub factions: std::collections::BTreeMap<u32, u32>,
    /// Templates with a `TerrainObject` or `LowResTerrainObject` record.
    pub terrain: std::collections::BTreeSet<u32>,
}

impl TemplateRecords {
    /// From the `worldentity` the asset stack resolves.
    pub fn load(assets: &mut crate::asset::AssetSource) -> Result<TemplateRecords, String> {
        use mercs2_formats::worldentity::{WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};
        let we = assets
            .resolve(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)
            .ok_or("worldentity 0x50075B3B: in no open wad")?;
        Self::from_worldentity(&WorldEntity::parse(&we).map_err(|e| format!("worldentity: {e}"))?)
    }

    /// An error when a key carries two `Name`, `NetCategoryInfo` or `Association` records, a payload
    /// of another shape, or several archetype physics components.
    pub fn from_worldentity(we: &mercs2_formats::worldentity::WorldEntity) -> Result<TemplateRecords, String> {
        use mercs2_formats::worldentity::Payload;
        use std::collections::{BTreeMap, BTreeSet};
        let keys_of = |class: &str| -> BTreeSet<u32> { we.groups_of(class).flat_map(|c| c.records.iter().flat_map(|r| r.keys.iter().copied())).collect() };
        let mut names = Vec::new();
        let mut net_category = BTreeMap::new();
        let mut factions = BTreeMap::new();
        let mut classes: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
        for c in &we.components {
            for r in &c.records {
                for &k in &r.keys {
                    classes.entry(k).or_default().push(c.class.as_str());
                    let dup = match (c.class.as_str(), &r.payload) {
                        ("Name", Payload::Name { name, .. }) => {
                            names.push((k, name.clone()));
                            false
                        }
                        ("NetCategoryInfo", Payload::PackedU16(v)) => net_category.insert(k, *v).is_some(),
                        ("Association", Payload::Fixed(b)) if b.len() == 4 => factions.insert(k, u32::from_le_bytes([b[0], b[1], b[2], b[3]])).is_some(),
                        ("Name" | "NetCategoryInfo" | "Association", p) => return Err(format!("template 0x{k:08X}: {} payload {p:?} is not the native layout", c.class)),
                        _ => false,
                    };
                    if dup {
                        return Err(format!("template 0x{k:08X}: two {} records", c.class));
                    }
                }
            }
        }
        let mut archetypes = BTreeMap::new();
        for (&k, cls) in &classes {
            let a = crate::game_world::unit_archetype(cls).map_err(|e| format!("template 0x{k:08X}: {e}"))?;
            if a != crate::spawn::Archetype::Prop {
                archetypes.insert(k, a);
            }
        }
        let mut terrain = keys_of("TerrainObject");
        terrain.extend(keys_of("LowResTerrainObject"));
        Ok(TemplateRecords {
            keys: classes.keys().copied().collect(),
            names,
            hibernation: mercs2_formats::hibernation_control::template_hibernation(we)?,
            models: mercs2_formats::model_name::template_model_names(we)?,
            tiny_geometry: keys_of("TinyGeometryObject"),
            lights: mercs2_formats::render_node::template_lights(we)?,
            light_scales: mercs2_formats::render_node::template_light_scales(we)?,
            net_category,
            archetypes,
            factions,
            terrain,
        })
    }
}

/// What makes a placed entity's render node when its activation completes — the source of the sphere
/// its grid entry is re-celled with (`docs/reverse_engineer/hibernation_grid.md` §2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeSource {
    /// No node: the re-cell writes the bare position and base 0 (TerrainObject and every entity
    /// without one of the components below).
    None,
    /// The ModelName listener's node: the `INFO` sphere with `TinyGeometryObject`, else the posed box.
    Model { model: u32, tiny_geometry: bool },
    /// The LowResTerrainObject listener's node: the tile's `BNDS` sphere.
    LowResTile(u32),
    /// The LightObject listener's node.
    Light(mercs2_formats::placement::LightObject, mercs2_formats::render_node::LightScales),
}

/// One placed entity — a `Transform` record of a placement block — as the hibernation grid and the
/// activation executor see it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedObject {
    pub key: u32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    /// `Transform.Parent`, 0 for none.
    pub parent: u32,
    /// Its own `HibernationControl`.
    pub own_hibernation: Option<mercs2_formats::hibernation_control::HibernationControl>,
    /// Its own, else its template's.
    pub hibernation: Option<mercs2_formats::hibernation_control::HibernationControl>,
    /// Its `TerrainObject` terrainmesh.
    pub terrain_mesh: Option<u32>,
    /// It has a `TerrainObject` or `LowResTerrainObject` record.
    pub terrain: bool,
    /// Its `LowResTerrainObject` tile mesh.
    pub low_res_mesh: Option<u32>,
    /// Registered static ([`mercs2_formats::placement::PlacedEntity::is_static`]).
    pub is_static: bool,
    /// Its own `ModelName`, else its template's.
    pub model: Option<u32>,
    /// What makes its render node.
    pub node: NodeSource,
    /// What a runtime spawn's activation builds; `None` for a placed entity.
    pub runtime: Option<RuntimeSpawn>,
}

/// The gameplay side of a runtime spawn, from its template.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeSpawn {
    pub archetype: crate::spawn::Archetype,
    /// `Association`, 0 for none.
    pub faction: u32,
}

/// The one render-node source an entity has (`docs/reverse_engineer/hibernation_grid.md` §2): its
/// model, its low-res tile or its light; a terrain tile has none here. More than one is an error.
fn node_source(
    key: u32,
    model: Option<u32>,
    tiny_geometry: bool,
    low_res: Option<u32>,
    light: Option<(mercs2_formats::placement::LightObject, mercs2_formats::render_node::LightScales)>,
    terrain: Option<u32>,
) -> Result<NodeSource, String> {
    let mut sources = Vec::new();
    if let Some(m) = model {
        sources.push(NodeSource::Model { model: m, tiny_geometry });
    }
    if let Some(t) = low_res {
        sources.push(NodeSource::LowResTile(t));
    }
    if let Some((l, s)) = light {
        sources.push(NodeSource::Light(l, s));
    }
    if sources.len() > 1 || (terrain.is_some() && !sources.is_empty()) {
        return Err(format!("entity 0x{key:08X}: more than one render-node source ({sources:?}, terrainmesh {terrain:?})"));
    }
    Ok(sources.pop().unwrap_or(NodeSource::None))
}

impl PlacedObject {
    /// Its grid registration (`FUN_00516280`): `HibernationControl` own → template → defaults; static
    /// unless terrain or Transform flag bit 9; a `TerrainObject` registers with base 10000.
    pub fn registration(&self) -> mercs2_core::hibernation::Registration {
        use mercs2_core::hibernation::{Registration, DEFAULT_DISTANCE, DEFAULT_RINGS};
        let (distance, rings) = self.hibernation.map_or((DEFAULT_DISTANCE, DEFAULT_RINGS), |h| (h.distance, h.rings()));
        Registration {
            key: self.key,
            position: self.pos,
            distance,
            rings,
            base: if self.terrain_mesh.is_some() { 10_000 } else { 0 },
            is_static: self.is_static,
        }
    }
}

impl PlacedObject {
    /// Its registration after an activation completes: the SetTransform re-cell (`0x518140` →
    /// `FUN_00515F60` → `FUN_00516F00`) moves the quantiser's input to pos + R(q)·centre (the
    /// SceneObject's i16 quaternion) and sets `base` = cvtt(radius) when `node` is its render node's
    /// sphere; without a node, the bare position and `base` 0.
    pub fn activated_registration(&self, node: Option<mercs2_formats::render_node::Sphere>) -> mercs2_core::hibernation::Registration {
        use mercs2_core::hibernation::{node_input, scene_object_quat};
        let mut r = self.registration();
        r.base = 0;
        if let Some((centre, radius)) = node {
            r.position = node_input(self.pos, scene_object_quat(self.quat), centre);
            r.base = radius as i32 as u16;
        }
        r
    }
}

/// Render-node spheres of placed entities, computed from the shipped assets and cached by source.
#[derive(Default)]
pub struct NodeSpheres {
    cache: std::collections::HashMap<(u32, u32, bool), mercs2_formats::render_node::Sphere>,
}

impl NodeSpheres {
    /// The sphere of the node `source` makes; `None` for no node.
    pub fn sphere(&mut self, w: &mut wad::Wad, source: NodeSource) -> Result<Option<mercs2_formats::render_node::Sphere>, String> {
        use mercs2_formats::render_node::{activation_enable, info_sphere, light_sphere, posed_sphere};
        let k = match source {
            NodeSource::None => return Ok(None),
            NodeSource::Light(light, scales) => return light_sphere(&light, scales).map(Some),
            NodeSource::Model { model, tiny_geometry } => (wad::MODEL_TYPE_HASH, model, tiny_geometry),
            NodeSource::LowResTile(mesh) => (mercs2_formats::types::TYPE_HASH_LOWRES_TERRAIN, mesh, false),
        };
        if let Some(&s) = self.cache.get(&k) {
            return Ok(Some(s));
        }
        let (ty, hash, tiny) = k;
        let s = if ty == wad::MODEL_TYPE_HASH {
            let c = wad::extract_container(w, hash)?;
            if tiny { info_sphere(&c) } else { activation_enable(&c).and_then(|en| posed_sphere(&c, &en)) }.map_err(|e| format!("model 0x{hash:08X}: {e}"))?
        } else {
            let c = wad::extract_container_typed(w, hash, ty)?;
            mercs2_formats::terrain::lowres_tile_sphere(&c).map_err(|e| format!("low-res tile 0x{hash:08X}: {e}"))?
        };
        self.cache.insert(k, s);
        Ok(Some(s))
    }
}

/// Every placed entity of the loaded layers, by key, in load order.
#[derive(Default)]
pub struct PlacedCatalog {
    objects: std::collections::HashMap<u32, PlacedObject>,
    order: Vec<u32>,
}

impl PlacedCatalog {
    /// Add every placed entity of one layer container (or a whole placement block) and return their
    /// keys in data order. A key already in the catalog is overwritten in place, as retail's container
    /// add is set-or-overwrite (`FUN_0064A600`; PROVEN for RuntimeLayerId, INFERRED for the rest —
    /// `docs/reverse_engineer/layer_load_code_map.md` §2). An entity with two node sources, or a placed
    /// `LightAnimation` (only templates carry one), is an error.
    pub fn add_block(&mut self, block: &[u8], templates: &TemplateRecords) -> Result<Vec<u32>, String> {
        use mercs2_formats::placement::{component_keys, load_terrain_tiles, placed_entities, placed_light_objects};
        let own_hib = mercs2_formats::hibernation_control::placed_hibernation(block)?;
        let own_model = mercs2_formats::model_name::placed_model_names(block)?;
        let own_tiny = component_keys(block, "TinyGeometryObject")?;
        let own_light = placed_light_objects(block)?;
        if let Some(k) = component_keys(block, "LightAnimation")?.into_iter().next() {
            return Err(format!("placed entity 0x{k:08X} has its own LightAnimation, which is not handled"));
        }
        let terrain_mesh: std::collections::HashMap<u32, u32> =
            load_terrain_tiles(block)?.into_iter().map(|t| (t.key, t.terrainmesh_hash)).collect();
        let ents = placed_entities(block)?;
        for e in &ents {
            let own_hibernation = own_hib.get(&e.key).copied();
            let model = own_model.get(&e.key).or_else(|| templates.models.get(&e.parent)).copied();
            let light = own_light.get(&e.key).or_else(|| templates.lights.get(&e.parent)).copied();
            let terrain = terrain_mesh.get(&e.key).copied();
            let tiny = own_tiny.contains(&e.key) || templates.tiny_geometry.contains(&e.parent);
            let node = node_source(e.key, model, tiny, e.low_res_mesh, light.map(|l| (l, templates.light_scales.get(&e.parent).copied().unwrap_or_default())), terrain)?;
            let replaced = self.objects.insert(e.key, PlacedObject {
                key: e.key,
                pos: e.pos,
                quat: e.quat,
                parent: e.parent,
                own_hibernation,
                hibernation: own_hibernation.or_else(|| templates.hibernation.get(&e.parent).copied()),
                terrain_mesh: terrain,
                terrain: e.terrain,
                low_res_mesh: e.low_res_mesh,
                is_static: e.is_static(),
                model,
                node,
                runtime: None,
            });
            if replaced.is_none() {
                self.order.push(e.key);
            }
        }
        Ok(ents.iter().map(|e| e.key).collect())
    }

    /// Add a runtime spawn of `template` as key `key` (the spawn worker `FUN_00673070`): its template's
    /// model, light, `HibernationControl`, archetype and faction. An error for a terrain template, whose
    /// spawn is not modelled.
    pub fn add_spawn(&mut self, key: u32, template: u32, pos: [f32; 3], quat: [f32; 4], is_static: bool, templates: &TemplateRecords) -> Result<PlacedObject, String> {
        if templates.terrain.contains(&template) {
            return Err(format!("spawn of template 0x{template:08X}: it carries a TerrainObject or LowResTerrainObject, whose spawn is not modelled"));
        }
        let model = templates.models.get(&template).copied();
        let light = templates.lights.get(&template).map(|&l| (l, templates.light_scales.get(&template).copied().unwrap_or_default()));
        let o = PlacedObject {
            key,
            pos,
            quat,
            parent: template,
            own_hibernation: None,
            hibernation: templates.hibernation.get(&template).copied(),
            terrain_mesh: None,
            terrain: false,
            low_res_mesh: None,
            is_static,
            model,
            node: node_source(key, model, templates.tiny_geometry.contains(&template), None, light, None)?,
            runtime: Some(RuntimeSpawn {
                archetype: templates.archetypes.get(&template).copied().unwrap_or(crate::spawn::Archetype::Prop),
                faction: templates.factions.get(&template).copied().unwrap_or(0),
            }),
        };
        if self.objects.insert(key, o).is_some() {
            return Err(format!("spawn key 0x{key:08X} is already catalogued"));
        }
        self.order.push(key);
        Ok(o)
    }

    /// Set a catalogued entity's transform (the SceneObject row a SetTransform wrote).
    pub fn set_transform(&mut self, key: u32, pos: [f32; 3], quat: [f32; 4]) {
        if let Some(o) = self.objects.get_mut(&key) {
            o.pos = pos;
            o.quat = quat;
        }
    }

    /// Remove placed entities.
    pub fn remove(&mut self, keys: &[u32]) {
        let gone: std::collections::HashSet<u32> = keys.iter().copied().filter(|k| self.objects.remove(k).is_some()).collect();
        self.order.retain(|k| !gone.contains(k));
    }

    pub fn get(&self, key: u32) -> Option<&PlacedObject> {
        self.objects.get(&key)
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// In load order.
    pub fn iter(&self) -> impl Iterator<Item = &PlacedObject> + '_ {
        self.order.iter().map(|k| &self.objects[k])
    }
}

/// The asset type of a layer (`pandemic_hash_m2("layer")`).
pub const LAYER_TYPE_HASH: u32 = 0xE6B8_1A54;

/// A layer's container — its placed entities — by layer name hash; `None` when the archive carries no
/// such layer (retail's load then fails, status 3).
pub fn layer_container(w: &mut wad::Wad, layer: u32) -> Result<Option<Vec<u8>>, String> {
    if !wad::aset_types(w, layer).iter().any(|(ty, _, _)| *ty == 9) {
        return Ok(None);
    }
    wad::extract_container_typed(w, layer, LAYER_TYPE_HASH).map(Some)
}

/// The layers whose containers a decompressed block carries (entries of type [`LAYER_TYPE_HASH`]), in
/// block order.
pub fn block_layers(block: &[u8]) -> Vec<u32> {
    mercs2_formats::ucfx::parse_block_entry_table(block)
        .1
        .iter()
        .filter(|e| e.type_hash == LAYER_TYPE_HASH)
        .map(|e| e.name_hash)
        .collect()
}

/// The world-streaming decision manager (geometry blocks and the population region cache). Placed
/// entities arrive by layer loads (`StreamingWorld`), not here.
pub fn build_streaming_catalog(
    _idx: &mercs2_formats::world_index::WorldIndex,
    cfg: mercs2_core::streaming::StreamingConfig,
) -> mercs2_core::streaming::StreamingManager {
    // NOTE (2026-07-02): the c3-block residency path (`load_one_c3_cell` → the small 0x5B724250
    // building `Model`) is DISABLED. That path placed the Model with a SYNTHESIZED position (c3-grid
    // XZ + Y=0), which floated ~80 m off the terrain — the misalignment the user reported. The real
    // per-cell hi-res content is the `0x7C569307` terrainmesh, now streamed correctly via the
    // `TerrainObject`->Transform tiles (below). The building `Model`'s authored transform is a
    // separate unsolved RCA (its position source is not the c3 cell-id); until it's recovered, we do
    // NOT stream it rather than render floating geometry. Re-enable once that placement is known.
    //
    // CONFIRM-LIVE (investigation 2026-07-06) — WHY it stays disabled + what unblocks it:
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
    mercs2_core::streaming::StreamingManager::new(cfg)
}

// ===========================================================================================
//  `schm` schema → world-loader wiring + region cache activation.
//
//  The bespoke `placement::*` loaders remain the correctness ORACLE for the props/terrain/regions
//  the executor instantiates (they still drive `build_streaming_catalog` unchanged). This section
//  adds the schema-DRIVEN path ALONGSIDE them: it walks each streamed block's `COMP` groups through
//  `mercs2_formats::schema` (`parse_comp_groups` → `ComponentSchema` → `deserialize_records`), builds
//  the kernel `ComponentRegistry` (`register_with_fields`), and reads typed field values by
//  name-hash — validating agreement with the oracle where they overlap (ModelName hash) so drift is
//  caught, and sourcing the population regions the RegionCache needs.
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

/// Walk every `COMP` group across the UCFX containers of a decompressed world block or a layer
/// container (`ucfx::placement_containers`), grouped into `{info, schm, data}` triples by the E1
/// `parse_comp_groups` (the `FUN_00654940` COMP arm). This is the single entry point both the schema
/// registry build and the region registration consume.
fn walk_comp_groups(block: &[u8]) -> Vec<mercs2_formats::schema::CompGroup> {
    let containers = mercs2_formats::ucfx::placement_containers(block, "stream-world").0;
    let mut out = Vec::new();
    for c in &containers {
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
        T::Byte => K::Byte,
        T::U8 => K::U8,
        T::Short => K::Short,
        T::U16 => K::U16,
        T::Int => K::Int,
        T::Hash => K::Hash,
        T::F32 => K::F32,
        T::StringRef => K::StringRef,
        T::Enum => K::Enum,
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
    let oracle_models: std::collections::HashMap<u32, u32> =
        mercs2_formats::placement::load_model_placements(block)
            .iter()
            .map(|m| (m.key, m.model_hash))
            .collect();
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
                bit_start: f.bit_start,
                bit_width: f.bit_width,
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

        // Read typed field values by name-hash for the class the oracle also decodes.
        match name.as_str() {
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
pub(crate) fn register_population_regions(
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

// ---- PopulationSimpleSpawner recovered on-disk schema (offline COMP-probe, retail vz.wad) ----
//
// PROBE RESULT (this session): `PopulationSimpleSpawner` IS A PLAIN KEYED COMP, not a bucketed
// hashmap. Confirmed over BOTH layers_static block 29 (6 sub-block groups, 29 records) and multiple
// `vz_state_*` overlay blocks (e.g. `vz_state_gurcon002` 14 records, `vz_state_pmccon004` 1): every
// group carries a `schm` declaring `payload_stride = 112` (0x70) with 27 fields, `is_variable_length
// = false`, and `schema.deserialize_records` divides the `data` blob exactly (`4 + 112 = 116`-byte
// records) and yields records cleanly. So it reads through the SAME generic keyed-COMP path as
// `PopulationDensity`/`ModelName` (`register_population_regions` above) — NOT the fixed-stride trap
// `jc2-arc-recon-detour` warns about. (The code map's "class-manager, not a flat descriptor" note is
// about the RUNTIME reflection *registry* — why it is absent from the 231-class TSVs — not about the
// on-disk serialization, which is an ordinary `schm`-declared COMP.)
//
// RECOVERED FIELD NAME-HASHES (`pandemic_hash_m2` of the field name, cracked by matching candidate
// names against the on-disk `schm` name-hashes — the schema is the recovered field layout, NOT the
// runtime `+0x20..+0x8c` instance offsets the task warns against):
//   * `Position`          0x05D7FC60  Blob32 @ payload+0   — ALWAYS identity `[0,0,0, 0,0,0,0, 1]`
//                                                            on disk (29/29 records); the spawner's
//                                                            real world anchor is the entity's joined
//                                                            `Transform` COMP (join hit 29/29, 0 miss).
//   * `SpawnerType`       0x6FF203C7  Flags  @ payload+84  — `SimpleSpawnerTypeEnum` discriminator
//                                                            (observed values {0, 2}).
//   * `SpawnerGroup`      0xD1A03B34  Flags  @ payload+88  — group index (observed 0..3, all < 8 ✓,
//                                                            matches `SPAWNER_GROUP_COUNT`).
//   * `SpawnerRadiusType` 0xC377497D  Flags  @ payload+76  — `SpawnerRadiusTypeEnum` (radius mode).
//   * `Hardpoint`         0xC780F3A8  U32    @ payload+36  — hardpoint id/ref.
//   * `UnitList`          0xB63E903F  Hash   @ payload+44  — the `PopulationList` the spawner's units
//                                                            come from.
//   * `UnitListWeighting` 0x87F967E5  Enum   @ payload+72  — `SpawnerWeightTypeEnum`, the choice rule
//                                                            (`mercs2_population::units`).
// (Field names: `mercs2_probe we-template --class PopulationSimpleSpawner`; the runtime spawner these
// records become is the table in code map §6.)

/// Recovered `PopulationSimpleSpawner` field name-hashes (see the block comment above).
pub const PSS_FIELD_POSITION: u32 = 0x05D7_FC60;
pub const PSS_FIELD_SPAWNER_TYPE: u32 = 0x6FF2_03C7;
pub const PSS_FIELD_SPAWNER_GROUP: u32 = 0xD1A0_3B34;
pub const PSS_FIELD_SPAWNER_RADIUS_TYPE: u32 = 0xC377_497D;
pub const PSS_FIELD_UNIT_LIST: u32 = 0xB63E_903F;
pub const PSS_FIELD_UNIT_LIST_WEIGHTING: u32 = 0x87F9_67E5;

/// Read the world block's authored `PopulationSimpleSpawner` COMP and register each spawner instance
/// into `mgr` (the ambient-population content feed — Task B). Mirrors [`register_population_regions`]:
/// walks the block's `COMP` groups, deserializes each `PopulationSimpleSpawner` record through the
/// recovered `schm` (plain keyed, stride 112 — see the block comment above), joins each record by
/// entity key to the `Transform` oracle for its world anchor, and reads the grounded `SpawnerGroup` /
/// `SpawnerType` / `UnitList` / `UnitListWeighting` fields by name-hash; the spawner's owner is the
/// entity key. Returns how many spawners were registered (bounded by the 768-cap pool — a full pool
/// refuses further registers, exactly as `cdbsizes.ini` bounds it); an error when a spawner's
/// `UnitList` is no `PopulationList` of `pop.units` or its `UnitListWeighting` is no
/// `SpawnerWeightTypeEnum` value.
///
/// GROUNDING / FIDELITY: the transform anchor, the group/type discriminators and the unit choice are
/// recovered from the shipped block. Two things are deliberately left at their defaults with a
/// **CONFIRM-LIVE** marker (code map §10.2):
///   1. **family** (Window/NoModel/Hardpoint/Path) — the `SpawnerType`→family fold is unproven
///      (§10.2, the four updaters are structurally interchangeable); registered as `NoModel` (the
///      neutral ambient/off-screen family) pending a live capture of the fold.
///   2. **interval/countdown/reload** — kept at the [`SimpleSpawner`] defaults.
/// Fabricating these from heuristics is declined, exactly as `register_population_regions` declines to
/// fabricate a region rect.
pub fn register_population_spawners(
    block: &[u8],
    pop: &mut crate::population::PopulationWorld,
) -> Result<usize, String> {
    use crate::population::{SimpleSpawner, SpawnerFamily, UnitChoice};
    use mercs2_core::glam::{Quat, Vec3};
    use mercs2_core::Transform;
    use mercs2_formats::schema::FieldValue;

    // Authored world anchors from the block's Transforms (the placement oracle) — the same join
    // register_population_regions uses. The inline `Position` field is identity on disk, so the real
    // world position comes from here.
    let mut centers: std::collections::HashMap<u32, [f32; 3]> = std::collections::HashMap::new();
    let mut yaws: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
    for p in mercs2_formats::placement::load_placements(block).unwrap_or_default() {
        centers.entry(p.key).or_insert(p.pos);
        yaws.entry(p.key).or_insert_with(|| mercs2_formats::placement::yaw_from_quat(&p.quat));
    }

    let mut n = 0usize;
    for g in walk_comp_groups(block) {
        if g.name.as_deref() != Some("PopulationSimpleSpawner") {
            continue;
        }
        let Some(schema) = g.schema() else { continue };
        let Some(data) = g.data.as_ref() else { continue };
        let Some(recs) = schema.deserialize_records(data) else { continue };
        for r in &recs {
            // World anchor: the joined Transform (the inline Position blob is identity on disk).
            let Some(c) = centers.get(&r.entity_key) else { continue };
            let yaw = yaws.get(&r.entity_key).copied().unwrap_or(0.0);
            let mut transform = Transform::from_translation(Vec3::from(*c));
            transform.rotation = Quat::from_rotation_y(yaw);

            let word = |field: u32, name: &str| match r.get(field) {
                Some(FieldValue::U32(v)) => Ok(v),
                other => Err(format!("PopulationSimpleSpawner 0x{:08X}: {name} is {other:?}", r.entity_key)),
            };
            // `SpawnerGroup` lands byte-for-byte in the runtime `+0x8b` (`FUN_004E2840` `0x004E289A`).
            let group = word(PSS_FIELD_SPAWNER_GROUP, "SpawnerGroup")?;
            if group >= u32::from(crate::population::SPAWNER_GROUP_ENUM_COUNT) {
                return Err(format!("PopulationSimpleSpawner 0x{:08X}: SpawnerGroup {group} is no SimpleSpawnerGroupEnum value", r.entity_key));
            }
            let spawner_type = word(PSS_FIELD_SPAWNER_TYPE, "SpawnerType")?;
            let spawner_type = u8::try_from(spawner_type).map_err(|_| {
                format!("PopulationSimpleSpawner 0x{:08X}: SpawnerType {spawner_type} is past a byte", r.entity_key)
            })?;
            let choice = UnitChoice {
                owner: r.entity_key,
                unit_list: word(PSS_FIELD_UNIT_LIST, "UnitList")?,
                weighting: word(PSS_FIELD_UNIT_LIST_WEIGHTING, "UnitListWeighting")?,
            };
            pop.units
                .check(&choice)
                .map_err(|e| format!("PopulationSimpleSpawner 0x{:08X}: {e}", r.entity_key))?;

            let spawner = SimpleSpawner {
                group: group as u8,
                // CONFIRM-LIVE: SpawnerType→family fold unproven (§10.2). Kept as the raw type
                // discriminator; family defaulted to the neutral NoModel ambient updater.
                type_disc: [spawner_type, 0, 0],
                family: SpawnerFamily::NoModel,
                choice,
                transform,
                // CONFIRM-LIVE: interval/reload kept at the SimpleSpawner defaults until the type-default
                // table or a live capture supplies them.
                ..SimpleSpawner::default()
            };
            if pop.spawners.register(spawner).is_some() {
                n += 1;
            } else {
                break; // 768-cap pool full — stop, matching cdbsizes.ini bound
            }
        }
    }
    Ok(n)
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

/// The highest WAD block index that carries placed entities.
///
/// Measured, not guessed: a census over `0..4000` finds **749** placement-bearing blocks and every one
/// of them lands in `0..=750`. Bounding the scan here keeps the boot cost at a few seconds instead of
/// the ~14 s a full 12k-block sweep takes; raise it if a future archive places entities higher.
pub const MAX_PLACEMENT_BLOCK: u16 = 750;

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
pub(crate) mod schema_wire_tests {
    use super::*;

    /// The game-gated tests: built by the `retail` feature, they read the retail `vz.wad` named by the
    /// repo-root `.mercs2-local.toml` and fail when it is absent. `pub(crate)` because
    /// `script_host`'s game-gated tests share its helpers.
    #[cfg(feature = "retail")]
    pub(crate) mod retail {
        use super::*;

        /// The retail `vz.wad` path, from the repo-root `.mercs2-local.toml` and nowhere else. Panics
        /// with the resolver's message when the file, its `vz_wad` key, or the archive is missing, and
        /// when the path is not UTF-8 (`wad::open` takes `&str`).
        pub(crate) fn vz_wad_path() -> String {
            let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let path =
                mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
            path.to_str()
                .unwrap_or_else(|| panic!("vz.wad path is not UTF-8: {}", path.display()))
                .to_string()
        }

        /// The retail `vz.wad`, opened. Panics when it cannot be found or opened.
        pub(crate) fn open_vz_wad() -> crate::wad::Wad {
            let path = vz_wad_path();
            crate::wad::open(&path).unwrap_or_else(|e| panic!("open {path}: {e}"))
        }

        /// The retail `layers_static` block. Shared by the game-gated tests here and in `script_host`.
        pub(crate) fn retail_layers_static() -> Vec<u8> {
            let mut w = open_vz_wad();
            find_terrain_blocks(&mut w)
                .unwrap_or_else(|e| panic!("the retail vz.wad's terrain blocks: {e}"))
                .1
        }

        /// Loading the `layers_static` block's 173 layers one container at a time places exactly what
        /// reading the block whole places: the same entities with the same node sources, terrain
        /// tiles, lights, landing pads, names and population regions. Every placement reader must
        /// accept a lone layer container (`ucfx::placement_containers`); one that read nothing from a
        /// container would surface here as a missing record, not as an error.
        #[test]
        fn static_layer_containers_place_what_the_whole_block_places() {
            use mercs2_formats::placement::{light_inventory, load_landing_zones, load_terrain_tiles, placed_names};
            let mut w = open_vz_wad();
            let ls = retail_layers_static();
            let templates = TemplateRecords::load(&mut crate::asset::AssetSource::discover(&vz_wad_path(), &[]).unwrap())
                .unwrap_or_else(|e| panic!("{e}"));
            let layers = block_layers(&ls);
            assert_eq!(layers.len(), 173, "layers_static carries 173 layer containers");

            let mut whole = PlacedCatalog::default();
            whole.add_block(&ls, &templates).unwrap_or_else(|e| panic!("whole block: {e}"));
            let mut per = PlacedCatalog::default();
            let (mut tiles, mut lights, mut pads, mut names) = (0usize, 0usize, 0usize, 0usize);
            let mut regions = StreamingManagerCounter::default();
            for &l in &layers {
                let c = layer_container(&mut w, l)
                    .unwrap_or_else(|e| panic!("layer 0x{l:08X}: {e}"))
                    .unwrap_or_else(|| panic!("layer 0x{l:08X} has no type-9 asset"));
                assert!(c.starts_with(b"UCFX"), "a layer asset is one UCFX container");
                per.add_block(&c, &templates).unwrap_or_else(|e| panic!("layer 0x{l:08X}: {e}"));
                tiles += load_terrain_tiles(&c).unwrap_or_else(|e| panic!("{e}")).len();
                lights += light_inventory(&c).len();
                pads += load_landing_zones(&c).len();
                names += placed_names(&c).len();
                regions.add(&c);
            }
            assert_eq!(per.len(), whole.len(), "the same number of placed entities");
            for o in whole.iter() {
                assert_eq!(per.get(o.key), Some(o), "entity 0x{:08X} differs between the two reads", o.key);
            }
            assert_eq!(tiles, load_terrain_tiles(&ls).unwrap_or_else(|e| panic!("{e}")).len(), "terrain tiles");
            assert_eq!(lights, light_inventory(&ls).len(), "lights");
            assert_eq!(pads, load_landing_zones(&ls).len(), "landing pads");
            assert_eq!(names, placed_names(&ls).len(), "names");
            let mut whole_regions = StreamingManagerCounter::default();
            whole_regions.add(&ls);
            assert_eq!(regions.0, whole_regions.0, "population regions");
            assert!(tiles > 0 && lights > 0 && pads > 0 && names > 0 && regions.0 > 0, "every record kind is present");
        }

        /// Population regions a block or container registers.
        #[derive(Default)]
        struct StreamingManagerCounter(usize);

        impl StreamingManagerCounter {
            fn add(&mut self, block: &[u8]) {
                let mut m = mercs2_core::streaming::StreamingManager::new(mercs2_core::streaming::StreamingConfig::default());
                self.0 += register_population_regions(block, &mut m);
            }
        }

        /// TASK-B confirm: `PopulationSimpleSpawner` is a plain keyed COMP in the retail `layers_static`
        /// block and the content loader feeds real spawner instances (at joined-Transform anchors, with the
        /// recovered SpawnerGroup) into an empty `SimpleSpawnerManager`.
        #[test]
        fn live_register_population_spawners_from_layers_static() {
            let ls = retail_layers_static();
            // The COMP must parse as plain-keyed (stride 4+112) and produce records — the probe result.
            let mut pss_groups = 0usize;
            for g in walk_comp_groups(&ls) {
                if g.name.as_deref() != Some("PopulationSimpleSpawner") {
                    continue;
                }
                pss_groups += 1;
                let schema = g.schema().expect("PopulationSimpleSpawner carries a schm");
                assert_eq!(schema.payload_stride, 112, "recovered PSS stride is 112 (0x70)");
                assert!(!schema.is_variable_length(), "PSS is fixed-stride, not variable/bucketed");
                if let Some(data) = g.data.as_ref() {
                    assert_eq!(data.len() % (4 + 112), 0, "PSS data divides exactly by the 116-B record");
                    assert!(schema.deserialize_records(data).is_some(), "PSS deserializes as plain keyed");
                }
            }
            assert!(pss_groups > 0, "layers_static must carry PopulationSimpleSpawner COMPs");

            // The loader feeds real spawners into a fresh (empty) population, checked against the shipped
            // unit lists.
            let mut assets = crate::asset::AssetSource::discover(&vz_wad_path(), &[]).expect("asset source");
            let data = crate::game_world::PopulationData::load(&mut assets).unwrap_or_else(|e| panic!("{e}"));
            let mut pop = crate::population::PopulationWorld::new();
            pop.units = data.catalog;
            assert_eq!(pop.spawners.spawners().len(), 0, "starts empty (the bug this fixes)");
            let n = register_population_spawners(&ls, &mut pop).unwrap_or_else(|e| panic!("{e}"));
            assert!(n >= 1, "must register at least one spawner from world data");
            assert_eq!(pop.spawners.spawners().len(), n, "every registered spawner lands in the pool");
            // Every registered spawner sits at a real (non-origin) world anchor and a valid group, and
            // its units come from a shipped list whose every unit is a loaded template.
            for sp in pop.spawners.spawners() {
                assert!(sp.group < crate::population::SPAWNER_GROUP_ENUM_COUNT, "a SimpleSpawnerGroupEnum value");
                assert!(
                    sp.transform.translation != mercs2_core::glam::Vec3::ZERO,
                    "spawner anchored at its joined Transform, not the identity Position blob"
                );
                for u in pop.units.units_of(sp.choice.unit_list).filter(|&u| u != 0) {
                    assert!(data.units.contains_key(&u), "unit 0x{u:08X} of list 0x{:08X} has a template", sp.choice.unit_list);
                }
            }
            eprintln!("[task-b] registered {n} PopulationSimpleSpawner instances from layers_static");
        }

        /// The transit landing pads read out of the REAL retail `LandingZone` COMP.
        ///
        /// Every number here is measured from the shipped block, and each is separately corroborated by the
        /// vendored Lua, which is what makes them assertable rather than merely observed:
        /// - 46 records = 23 zones × 2 co-op player slots.
        /// - The zone set `1..8, 12, 15..18, 20..25, 27..30` is exactly the set `vz/wifhqdata.lua`'s
        ///   `nLandingZone`/`nAltLandingZone` fields reference (`docs/mercs2-luacd/04_tutorials_wifdata.md`
        ///   §2.5); the seven gaps appear nowhere in the corpus.
        /// - All 46 pads carry a `Name` of the form `<zone>_<faction>_<site>_lz_player{one,two}` — the
        ///   names the shipped Lua also resolves by hand (`resident/mrxsupport.lua:606-610`,
        ///   `vz/wifpmcinterior.lua:2108`, `vz/wifhqdata.lua:128` for zone 12 slot 1).
        /// One `LandingZone` record joined by key to its entity's `Transform` and `Name`.
        #[derive(Debug)]
        struct Pad {
            zone: u32,
            slot: u32,
            name: Option<String>,
            pos: [f32; 3],
        }

        /// Every landing pad of a placement block, sorted by `(zone, slot)`. A record whose key has no
        /// `Transform` is an error: every pad is an addressable world object.
        fn landing_zone_pads(block: &[u8]) -> Vec<Pad> {
            let mut xf: std::collections::HashMap<u32, ([f32; 3], Option<String>)> = std::collections::HashMap::new();
            for p in mercs2_formats::placement::load_placements(block).unwrap_or_else(|e| panic!("placements: {e}")) {
                xf.entry(p.key).or_insert((p.pos, p.name.map(|n| n.to_ascii_lowercase())));
            }
            let mut out: Vec<Pad> = mercs2_formats::placement::load_landing_zones(block)
                .into_iter()
                .map(|r| {
                    let (pos, name) = xf.get(&r.key).cloned().unwrap_or_else(|| panic!("pad 0x{:08X} has no Transform", r.key));
                    Pad { zone: r.zone, slot: r.slot, name, pos }
                })
                .collect();
            out.sort_by_key(|p| (p.zone, p.slot));
            out
        }

        #[test]
        fn live_landing_zone_pads_if_wad_present() {
            let ls = retail_layers_static();
            let pads = landing_zone_pads(&ls);
            assert_eq!(pads.len(), 46, "retail vz authors 23 landing zones × 2 player slots");

            let zones: Vec<u32> = {
                let mut z: Vec<u32> = pads.iter().map(|p| p.zone).collect();
                z.dedup();
                z
            };
            assert_eq!(
                zones,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 12, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 27, 28, 29, 30],
                "the sparse zone set the shipped Lua indexes by absolute number"
            );

            // Exactly one pad per (zone, slot); slots are only ever 1 or 2.
            for z in &zones {
                let mut slots: Vec<u32> = pads.iter().filter(|p| p.zone == *z).map(|p| p.slot).collect();
                slots.sort_unstable();
                assert_eq!(slots, vec![1, 2], "zone {z} must have one pad per player slot");
            }

            // The name convention, where a name exists: `<zone>_…_lz_player{one,two}`.
            let named = pads.iter().filter(|p| p.name.is_some()).count();
            assert_eq!(named, 46, "all 46 retail pads carry a Name COMP");
            for p in pads.iter().filter(|p| p.name.is_some()) {
                let n = p.name.as_deref().unwrap();
                let want_slot = if p.slot == 1 { "_playerone" } else { "_playertwo" };
                assert!(
                    n.starts_with(&format!("{:02}_", p.zone)),
                    "pad name {n:?} must be prefixed with its zone number ({})",
                    p.zone
                );
                // Only the `player{one,two}` tail is asserted: zone 3 slot 1 ships as
                // `03_mar_airport_llz_playerone` — a typo in the retail data, reproduced not repaired.
                assert!(n.ends_with(want_slot), "pad name {n:?} must end with {want_slot}");
            }

            // Every pad has a distinct authored position (they are physical touchdown spots).
            let mut seen = std::collections::HashSet::new();
            for p in &pads {
                let key = (p.pos[0].to_bits(), p.pos[1].to_bits(), p.pos[2].to_bits());
                assert!(seen.insert(key), "two pads share a position: {p:?}");
            }
        }

        /// The landing zones a LIVE RETAIL RUN reports, transcribed from the PMC Blackbox capture
        /// `game-files/pmc_blackbox-mattias-save-end-game.log` (an ASI loader hooking the shipped build's
        /// stripped `Debug.Printf`). Each line is `MrxTransit.LoadSingleton` replaying the SAVE's transit
        /// blob — `mrxtransit.lua:399` in the vendored corpus:
        ///
        /// ```text
        /// [lua] Landing zone 28 affiliated with Pir (nil)  @mrxtransit:669
        /// ```
        ///
        /// so this is the end-game save's faction ownership, not authored world data. It is transcribed
        /// rather than parsed at test time because the capture is a 500 KB machine-local artifact; the
        /// zone/faction pairs are the whole of what it contributes.
        const RETAIL_CAPTURE_AFFILIATIONS: [(u32, &str); 22] = [
            (1, "Pmc"), (2, "Oil"), (3, "Oil"), (4, "Gur"), (5, "Gur"), (7, "All"), (8, "Pir"),
            (12, "Chi"), (15, "Oil"), (16, "Oil"), (17, "Gur"), (18, "Gur"), (20, "All"), (21, "All"),
            (22, "All"), (23, "Chi"), (24, "Chi"), (25, "Chi"), (27, "Pir"), (28, "Pir"), (29, "Oil"),
            (30, "Chi"),
        ];

        /// **Cross-check of the `LandingZone` COMP against a live retail run.**
        ///
        /// `live_landing_zone_pads_if_wad_present` above proves what the shipped WORLD DATA authors. This
        /// proves the same set is what the shipped GAME actually enumerates at runtime, from a completely
        /// independent source: a hooked-log capture of retail play. Two sources that could disagree and
        /// do not.
        ///
        /// The one difference is the interesting part. The capture affiliates 22 zones; the COMP authors
        /// 23. The missing one is zone **6**, and `mrxtransit.lua`'s `Reset` singles out exactly that zone:
        ///
        /// ```lua
        /// if _tLandingZones[6] then
        ///   _tLandingZones[6].bFake = true
        /// end
        /// ```
        ///
        /// A fake pad is never faction-affiliated, so it never reaches the `LoadSingleton` print. The
        /// capture and the shipped script therefore corroborate each other on the one zone where the two
        /// enumerations differ, which is what makes 23-vs-22 evidence rather than a discrepancy.
        #[test]
        fn retail_capture_corroborates_the_authored_landing_zone_set() {
            let ls = retail_layers_static();
            let authored: std::collections::BTreeSet<u32> =
                landing_zone_pads(&ls).iter().map(|p| p.zone).collect();
            let captured: std::collections::BTreeSet<u32> =
                RETAIL_CAPTURE_AFFILIATIONS.iter().map(|(z, _)| *z).collect();

            const FAKE_ZONE: u32 = 6; // mrxtransit.lua Reset(): `_tLandingZones[6].bFake = true`
            assert!(
                !captured.contains(&FAKE_ZONE),
                "zone {FAKE_ZONE} is the bFake pad; a live run must never affiliate it"
            );
            assert_eq!(
                authored.difference(&captured).copied().collect::<Vec<u32>>(),
                vec![FAKE_ZONE],
                "the ONLY authored zone a live run does not affiliate is the fake one"
            );
            assert!(
                captured.is_subset(&authored),
                "every zone the live game enumerated must exist in the authored COMP; strays: {:?}",
                captured.difference(&authored).collect::<Vec<_>>()
            );

            // The affiliations name real factions — the abbreviations `MrxFactionManager.GetFactionAbbrevs`
            // returns, which the same capture lists as it arms each one's attitude events.
            const FACTIONS: [&str; 8] = ["All", "Chi", "Civ", "Gur", "Oil", "Pir", "Pmc", "Vza"];
            for (zone, faction) in RETAIL_CAPTURE_AFFILIATIONS {
                assert!(FACTIONS.contains(&faction), "zone {zone}: unknown faction {faction:?}");
            }
        }

        /// Live end-to-end proof that the E1 schema deserializer is wired into the world-load path and
        /// that the RegionCache is populated.
        ///
        /// Asserts, against the real retail `layers_static` block:
        ///   1. the schema path deserializes many generic COMP records (≥ 2000) across many classes,
        ///   2. every ModelName hash it reads AGREES with the bespoke `placement` oracle (no drift on the
        ///      overlap),
        ///   3. ≥ 1 population region registers into the streaming manager's RegionCache, and driving
        ///      `update_regions` at a region's anchor caches that region IN.
        #[test]
        fn live_schema_and_region_wire_if_wad_present() {
            let ls = retail_layers_static();

            // Seam A — schema-driven generic COMP deserialize + oracle agreement.
            let (reg, stats) = load_schema_components(&ls);
            println!(
                "[schema-test] classes={} generic_groups={} generic_records={} | ModelName {}/{} agree | pool_budget_total={}",
                stats.classes, stats.generic_groups, stats.generic_records,
                stats.model_agree, stats.model_checked, reg.total_budget()
            );
            assert!(stats.classes >= 10, "expected many registered classes, got {}", stats.classes);
            assert!(
                stats.generic_records >= 2000,
                "expected ≥2000 generic COMP records deserialized, got {}",
                stats.generic_records
            );
            // The schema path must agree with the oracle exactly where they overlap.
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
}

// ── W7 relocations: boot mechanism moved out of the game's `world.rs` ─────────────────────────────
//   These were hand-rolled in `mercs2_game::world` (recovered from the deleted engine bin); they are
//   engine world/terrain utilities, so they live next to the `HeightMap` + placement helpers they use.

/// Build the DEBUG placement-marker mesh (`--markers`): one upright tetra per placement, colour-coded
/// by class (PMC-subset warm orange / off-map yellow / ordinary cool blue). Vertex-colour only.
/// Relocated verbatim from `mercs2_game::world::build_placement_markers` — pure geometry over the
/// placement records, using this module's own `placement_is_pmc_subset` / `is_out_of_bounds`.
pub fn build_placement_markers(
    placements: &[mercs2_formats::placement::Placement],
) -> (Vec<crate::mesh::Vertex>, Vec<u32>, Vec<crate::mesh::DrawGroup>) {
    const H: f32 = 3.0; // marker height (m)
    const R: f32 = 0.9; // marker base half-width (m)
    // Upright tetra: apex above, 3 base corners. (LH +Y up.)
    let local: [[f32; 3]; 4] =
        [[0.0, H, 0.0], [-R, 0.0, -R], [R, 0.0, -R], [0.0, 0.0, R]];
    let faces: [[u32; 3]; 4] = [[0, 2, 1], [0, 3, 2], [0, 1, 3], [1, 2, 3]];
    let mut verts: Vec<crate::mesh::Vertex> = Vec::with_capacity(placements.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(placements.len() * 12);
    for p in placements {
        let color = if placement_is_pmc_subset(p) {
            [0.95, 0.35, 0.10] // PMC/base subset: warm orange
        } else if is_out_of_bounds(&p.pos) {
            [0.95, 0.90, 0.15] // off-map candidate: yellow
        } else {
            [0.20, 0.55, 0.90] // ordinary placement: cool blue
        };
        let base = verts.len() as u32;
        for l in &local {
            verts.push(crate::mesh::Vertex {
                pos: [p.pos[0] + l[0], p.pos[1] + l[1], p.pos[2] + l[2]],
                color,
                uv: [0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                joints: [0, 0, 0, 0],
                weights: [255, 0, 0, 0],
            });
        }
        for f in &faces {
            indices.push(base + f[0]);
            indices.push(base + f[1]);
            indices.push(base + f[2]);
        }
    }
    let draws = vec![crate::mesh::DrawGroup {
        index_start: 0,
        index_count: indices.len() as u32,
        diffuse: None, // vertex-color only (white fallback texture)
        ..Default::default()
    }];
    (verts, indices, draws)
}

/// World-space collision triangles of a placed renderable mesh: rotate each vertex by `quat` and
/// translate by `pos`, emitting one `[Vec3; 3]` per index triple. The single builder behind the
/// game's per-cell / per-interior-shell / per-prop collision loops (they differ only in whether they
/// pass a rotation). Pass the identity quat `[0,0,0,1]` for translate-only placement.
pub fn mesh_collision_tris(
    verts: &[crate::mesh::Vertex],
    indices: &[u32],
    pos: [f32; 3],
    quat: [f32; 4],
) -> Vec<[mercs2_core::glam::Vec3; 3]> {
    use mercs2_core::glam::{Quat, Vec3};
    let tr = Vec3::from(pos);
    let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
    indices
        .chunks_exact(3)
        .map(|idx| {
            let w = |i: u32| q * Vec3::from(verts[i as usize].pos) + tr;
            [w(idx[0]), w(idx[1]), w(idx[2])]
        })
        .collect()
}

impl HeightMap {
    /// Sample this render `HeightMap` onto a regular grid → a `mercs2_physics::Heightmap` the fleet
    /// physics can raycast (K2 S3). The terrain extent is the engine's ±4000 m world square; a
    /// 257×257 grid (~31 m cells) bilinearly interpolates smoothly enough for vehicle ground contact.
    /// The render `HeightMap` keeps the exact triangles for the player walk; this is the physics-side
    /// heightfield. Relocated from `mercs2_game::world::heightmap_to_physics`.
    pub fn to_physics_heightmap(&self) -> crate::physics::Heightmap {
        const W: usize = 257;
        const MIN: f32 = -4000.0;
        const MAX: f32 = 4000.0;
        let cell = (MAX - MIN) / (W as f32 - 1.0);
        let mut heights = Vec::with_capacity(W * W);
        for iz in 0..W {
            let z = MIN + iz as f32 * cell;
            for ix in 0..W {
                let x = MIN + ix as f32 * cell;
                heights.push(self.height_at(x, z));
            }
        }
        crate::physics::Heightmap::new(MIN, MIN, cell, W, W, heights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_or_name_reads_hex_only_after_0x() {
        assert_eq!(hash_or_name("0x4D4F42B5"), Ok(0x4D4F_42B5));
        assert_eq!(hash_or_name("0X4d4f42b5"), Ok(0x4D4F_42B5));
        assert_eq!(hash_or_name("dead"), Ok(mercs2_formats::hash::pandemic_hash_m2("dead")));
        assert_eq!(hash_or_name("al_hum_soldier_elitea_ub"), Ok(0x037C_DC9B));
        assert!(hash_or_name("0xnothex").is_err());
    }

    #[test]
    fn aset_names_keeps_named_rows_and_rejects_a_row_without_a_hash() {
        let dir = std::env::temp_dir().join("mercs2_engine_aset_names_test");
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.csv");
        std::fs::write(&good, "asset_hash,name,rest\n0x00000001,one,x\n0x00000002,,y\n").unwrap();
        let names = aset_names(&good).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names.get(&1).map(String::as_str), Some("one"));

        let bad = dir.join("bad.csv");
        std::fs::write(&bad, "asset_hash,name\n0x00000001,one\nnope,two\n").unwrap();
        let e = aset_names(&bad).unwrap_err();
        assert!(e.ends_with("bad.csv:3: \"nope\" is not a 0x hash"), "{e}");
    }
}

