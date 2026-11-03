//! `layer` — what a placement layer's entities are, as the WAD stack resolves them.
//!
//! `layer <name|0xH>` prints every entity of the layer: its key, name, template, model and component
//! classes (its own, then its template's). `layer --all` reads all of them and prints how many
//! entities draw a model and which classes the rest carry, and fails when a layer does not read.
//! `layer --frames <capture>` tests placed `Transform`s against a capture's live positions.

use glam::{Mat4, Quat, Vec3};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::path::Path;

use mercs2_engine::asset::AssetSource;
use mercs2_engine::worldutil::{PlacedCatalog, TemplateRecords, LAYER_TYPE_HASH};
use mercs2_formats::physical_link::PhysicalLink;
use mercs2_formats::placement::{comp_inventory, entity_classes, placed_entities, placed_names, PlacedEntity};
use mercs2_formats::types::{TYPE_HASH_MODEL, TYPE_ID_LAYER};
use mercs2_formats::worldentity::{WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};

/// The classes of every key the world's templates name.
fn template_classes(we: &WorldEntity) -> HashMap<u32, BTreeSet<String>> {
    let mut out: HashMap<u32, BTreeSet<String>> = HashMap::new();
    for c in &we.components {
        for r in &c.records {
            for &k in &r.keys {
                out.entry(k).or_default().insert(c.class.clone());
            }
        }
    }
    out
}

fn world(assets: &mut AssetSource) -> Result<WorldEntity, String> {
    let c = assets
        .ecs_container(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)?
        .ok_or("worldentity: in no open wad")?;
    WorldEntity::parse(&c).map_err(|e| format!("worldentity: {e}"))
}

/// Each placed `LineRegion` record's region: its entity key and the `lineregion` asset hash it names
/// (the component's one u32, which the engine looks up as `(hash, 0x6310807F)`).
fn line_regions(block: &[u8]) -> Result<HashMap<u32, u32>, String> {
    let mut out = HashMap::new();
    for c in comp_inventory(block).into_iter().filter(|c| c.info_name.as_deref() == Some("LineRegion")) {
        let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
        if c.payload_stride != Some(4) {
            return Err(format!("LineRegion with a {:?}-byte record payload, not 4", c.payload_stride));
        }
        let data = block.get(off..off + size).ok_or("LineRegion data past the block")?;
        if data.len() % 8 != 0 {
            return Err(format!("LineRegion data of {} bytes is not whole 8-byte records", data.len()));
        }
        for r in data.chunks_exact(8) {
            let (key, region) = (u32::from_le_bytes([r[0], r[1], r[2], r[3]]), u32::from_le_bytes([r[4], r[5], r[6], r[7]]));
            if out.insert(key, region).is_some_and(|prev| prev != region) {
                return Err(format!("entity 0x{key:08X} has two different LineRegion records"));
            }
        }
    }
    Ok(out)
}

pub fn layer(wadpath: &str, hash: u32, overlays: &[String]) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, overlays)?;
    let we = world(&mut assets)?;
    let templates = TemplateRecords::from_worldentity(&we)?;
    let names: HashMap<u32, &str> = templates.names.iter().map(|(k, n)| (*k, n.as_str())).collect();
    let classes = template_classes(&we);
    let block = assets.ecs_container(LAYER_TYPE_HASH, hash)?.ok_or_else(|| format!("0x{hash:08X}: no layer in {:?}", assets.paths()))?;
    let mut catalog = PlacedCatalog::default();
    let keys = catalog.add_block(&block, &templates)?;
    let own = entity_classes(&block)?;
    let entity_names: HashMap<u32, String> = placed_names(&block).into_iter().collect();
    let regions = line_regions(&block)?;
    println!("layer 0x{hash:08X}: {} entities", keys.len());
    for k in keys {
        let o = catalog.get(k).expect("add_block returned the key");
        println!(
            "0x{k:08X} {:?} template {} model {:?} mesh {:?} pos {:?}",
            entity_names.get(&k).map(String::as_str).unwrap_or("-"),
            names.get(&o.parent).copied().unwrap_or("-"),
            o.model.map(|m| format!("0x{m:08X}")),
            o.terrain_mesh.or(o.low_res_mesh).map(|m| format!("0x{m:08X}")),
            o.pos,
        );
        println!("    own {:?}", own.get(&k));
        println!("    template {:?}", classes.get(&o.parent));
        if let Some(r) = regions.get(&k) {
            println!("    lineregion 0x{r:08X}");
        }
    }
    Ok(())
}

pub fn layer_all(wadpath: &str) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let we = world(&mut assets)?;
    let templates = TemplateRecords::from_worldentity(&we)?;
    let classes = template_classes(&we);
    let mut hashes = BTreeSet::new();
    for path in assets.paths() {
        let mut f = File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let size = f.metadata().map_err(|e| format!("{path}: {e}"))?.len();
        let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{path}: {e}"))?;
        hashes.extend(arch.aset.iter().filter(|e| e.type_id == TYPE_ID_LAYER).map(|e| e.asset_hash));
    }
    let (mut entities, mut parented, mut modelled, mut meshed, mut largest) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut largest_layer = 0u32;
    let (mut own_links, mut child_template, mut child_here, mut child_elsewhere) = (0usize, 0usize, 0usize, 0usize);
    let mut bare: BTreeMap<String, usize> = BTreeMap::new();
    let mut signatures: BTreeMap<String, usize> = BTreeMap::new();
    let mut failed = Vec::new();
    for &h in &hashes {
        let Some(block) = assets.ecs_container(LAYER_TYPE_HASH, h)? else {
            failed.push(format!("0x{h:08X}: no container"));
            continue;
        };
        let read = (|| -> Result<(), String> {
            let mut catalog = PlacedCatalog::default();
            let keys = catalog.add_block(&block, &templates)?;
            let own = entity_classes(&block)?;
            for l in mercs2_formats::physical_link::placed_physical_links(&block)?.values().flatten() {
                own_links += 1;
                if templates.keys.contains(&l.child) {
                    child_template += 1;
                } else if catalog.get(l.child).is_some() {
                    child_here += 1;
                } else {
                    child_elsewhere += 1;
                }
            }
            if keys.len() > largest {
                largest = keys.len();
                largest_layer = h;
            }
            for k in keys {
                let o = catalog.get(k).expect("add_block returned the key");
                entities += 1;
                parented += usize::from(o.parent != 0);
                if o.model.is_some() {
                    modelled += 1;
                    continue;
                }
                if o.terrain_mesh.or(o.low_res_mesh).is_some() {
                    meshed += 1;
                    continue;
                }
                let mut set: BTreeSet<&str> = BTreeSet::new();
                for c in own.get(&k).into_iter().chain(classes.get(&o.parent)).flatten() {
                    set.insert(c);
                }
                for c in &set {
                    *bare.entry((*c).to_string()).or_default() += 1;
                }
                *signatures.entry(set.into_iter().collect::<Vec<_>>().join(" ")).or_default() += 1;
            }
            Ok(())
        })();
        if let Err(e) = read {
            failed.push(format!("0x{h:08X}: {e}"));
        }
    }
    println!("{} layers, {entities} entities, the largest {largest} (0x{largest_layer:08X}); {parented} with a template", hashes.len());
    println!("{modelled} draw a model, {meshed} a terrain mesh, {} neither", entities - modelled - meshed);
    println!("{own_links} placed PhysicalLink records: child a template {child_template}, an entity of the layer {child_here}, neither {child_elsewhere}");
    println!("classes of the entities that draw neither:");
    let mut by_count: Vec<_> = bare.into_iter().collect();
    by_count.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (c, n) in by_count {
        println!("  {n:7} {c}");
    }
    println!("their class sets:");
    let mut by_count: Vec<_> = signatures.into_iter().collect();
    by_count.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (s, n) in by_count {
        println!("  {n:7} {s}");
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} layers did not read:\n{}", failed.len(), failed.join("\n")))
    }
}

/// `layer --near <x> <z> <radius>`: every placed entity of every layer within `radius` metres of the XZ
/// point: the layer, what it draws, and its classes, nearest first. The question "what is under this
/// spot" across all 900-odd layers, which `layer <name>` cannot ask.
pub fn layer_near(wadpath: &str, x: f32, z: f32, radius: f32) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let we = world(&mut assets)?;
    let templates = TemplateRecords::from_worldentity(&we)?;
    let names: HashMap<u32, &str> = templates.names.iter().map(|(k, n)| (*k, n.as_str())).collect();
    let classes = template_classes(&we);
    let mut hashes = BTreeSet::new();
    for path in assets.paths() {
        let mut f = File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let size = f.metadata().map_err(|e| format!("{path}: {e}"))?.len();
        let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{path}: {e}"))?;
        hashes.extend(arch.aset.iter().filter(|e| e.type_id == TYPE_ID_LAYER).map(|e| e.asset_hash));
    }
    let mut rows: Vec<(f32, String)> = Vec::new();
    let mut unread = 0usize;
    for &h in &hashes {
        let Some(block) = assets.ecs_container(LAYER_TYPE_HASH, h)? else {
            return Err(format!("0x{h:08X}: no container"));
        };
        let mut catalog = PlacedCatalog::default();
        let keys = match catalog.add_block(&block, &templates) {
            Ok(keys) => keys,
            Err(e) => {
                println!("0x{h:08X}: unreadable: {e}");
                for path in assets.paths() {
                    let mut w = mercs2_engine::wad::open(path)?;
                    let rows = mercs2_engine::wad::aset_types(&w, h);
                    let Some(c) = mercs2_engine::worldutil::layer_container(&mut w, h)? else { continue };
                    let issues = mercs2_formats::ucfx::verify_ucfx_container(&c, "layer", LAYER_TYPE_HASH);
                    println!("  {path}: rows {rows:?}, {} bytes, issues {issues:?}", c.len());
                }
                unread += 1;
                continue;
            }
        };
        let own = entity_classes(&block).map_err(|e| format!("0x{h:08X}: {e}"))?;
        let entity_names: HashMap<u32, String> = placed_names(&block).into_iter().collect();
        for k in keys {
            let o = catalog.get(k).expect("add_block returned the key");
            let d = (o.pos[0] - x).hypot(o.pos[2] - z);
            if d > radius {
                continue;
            }
            let mut set: BTreeSet<&str> = BTreeSet::new();
            for c in own.get(&k).into_iter().chain(classes.get(&o.parent)).flatten() {
                set.insert(c);
            }
            rows.push((
                d,
                format!(
                    "{d:7.1} layer 0x{h:08X} 0x{k:08X} {:?} template {} model {:?} mesh {:?} pos {:?} {set:?}",
                    entity_names.get(&k).map(String::as_str).unwrap_or("-"),
                    names.get(&o.parent).copied().unwrap_or("-"),
                    o.model.map(|m| format!("0x{m:08X}")),
                    o.terrain_mesh.or(o.low_res_mesh).map(|m| format!("0x{m:08X}")),
                    o.pos,
                ),
            ));
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!("{} entities within {radius} m of {x}, {z}, over {} layers, {unread} unreadable", rows.len(), hashes.len());
    for (_, r) in rows {
        println!("{r}");
    }
    Ok(())
}

/// `layer --loaded <capture>`: which of `wadpath`'s own layers a retail capture has in memory, measured by
/// how many of each layer's placed keys are live `SceneObject` rows. Prints `live/placed` per layer that
/// has any, most complete first. Reads the archive at `wadpath` alone, no overlay.
pub fn layer_loaded(wadpath: &str, capture: &Path) -> Result<(), String> {
    let cap = crate::dump::Capture::open(capture)?;
    let mem = cap.mem()?;
    let table = mem.container_named("SceneObject")?;
    let live: BTreeSet<u32> = mem.records(&table)?.iter().map(|r| r.key).collect();
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let templates = TemplateRecords::from_worldentity(&world(&mut assets)?)?;
    let mut w = mercs2_engine::wad::open(wadpath)?;
    let hashes: BTreeSet<u32> = mercs2_engine::wad::aset_entries(&w).iter().filter(|e| e.type_id == TYPE_ID_LAYER).map(|e| e.asset_hash).collect();
    let mut rows = Vec::new();
    for &h in &hashes {
        let block = mercs2_engine::worldutil::layer_container(&mut w, h)?.ok_or_else(|| format!("0x{h:08X}: no container"))?;
        let mut catalog = PlacedCatalog::default();
        let keys = catalog.add_block(&block, &templates).map_err(|e| format!("0x{h:08X}: {e}"))?;
        let present = keys.iter().filter(|k| live.contains(k)).count();
        if present > 0 {
            rows.push((present as f32 / keys.len() as f32, present, keys.len(), h));
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    println!("{} of {} layers have live keys, {} live keys", rows.len(), hashes.len(), live.len());
    for (_, present, total, h) in rows {
        println!("{present:6}/{total:<6} 0x{h:08X}");
    }
    Ok(())
}

/// A model's HIER nodes at rest, by name hash.
fn node_worlds(container: &[u8]) -> HashMap<u32, Mat4> {
    let hier = mercs2_formats::orchestrator::parse_hier(container);
    let mut worlds: Vec<Mat4> = Vec::with_capacity(hier.len());
    let mut out = HashMap::with_capacity(hier.len());
    for n in &hier {
        let local = Mat4::from_cols_array(&n.local);
        let world = n.parent.and_then(|p| worlds.get(p).copied()).map_or(local, |p| p * local);
        worlds.push(world);
        out.insert(n.hash, world);
    }
    out
}

fn transform(pos: [f32; 3], quat: [f32; 4]) -> Mat4 {
    Mat4::from_rotation_translation(Quat::from_array(quat), Vec3::from(pos))
}

/// `layer --frames <capture>`: which placed entities sit at their `Transform`, tested against a v1.0
/// capture's live SceneObject positions (`+0`). An entity a placed `PhysicalLink` names as its child is
/// compared with its `Transform` and with its parent × hard point × offset, up the chain of links;
/// every other entity with its `Transform`. An entity the capture does not hold is not compared.
pub fn layer_frames(wadpath: &str, capture: &Path) -> Result<(), String> {
    let cap = crate::dump::Capture::open(capture)?;
    let mem = cap.mem()?;
    let scene_objects = mem.container_named("SceneObject")?;
    let mut live: HashMap<u32, Vec3> = HashMap::new();
    let mut awake: BTreeSet<u32> = BTreeSet::new();
    for r in mem.records(&scene_objects)? {
        let b = mem.read(r.va, 12)?;
        let float = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        live.insert(r.key, Vec3::new(float(0), float(4), float(8)));
        if (mem.u16(r.va + 0x18)? >> 12) & 3 == 2 {
            awake.insert(r.key);
        }
    }
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let we = world(&mut assets)?;
    let templates = TemplateRecords::from_worldentity(&we)?;
    let mut hashes = BTreeSet::new();
    for path in assets.paths() {
        let mut f = File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let size = f.metadata().map_err(|e| format!("{path}: {e}"))?.len();
        let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{path}: {e}"))?;
        hashes.extend(arch.aset.iter().filter(|e| e.type_id == TYPE_ID_LAYER).map(|e| e.asset_hash));
    }
    const BUCKETS: [f32; 5] = [0.01, 0.1, 1.0, 10.0, 100.0];
    let bucket = |d: f32| BUCKETS.iter().position(|&b| d <= b).unwrap_or(BUCKETS.len());
    let (mut child_stored, mut child_linked, mut rest_stored) = ([0usize; 6], [0usize; 6], [0usize; 6]);
    let (mut held, mut not_held, mut two_parents) = (0usize, 0usize, 0usize);
    let mut flags: BTreeMap<(u16, bool), usize> = BTreeMap::new();
    let (mut child_misses, mut rest_misses) = (Vec::new(), Vec::new());
    let mut nodes: HashMap<u32, HashMap<u32, Mat4>> = HashMap::new();
    for &h in &hashes {
        let block = assets.ecs_container(LAYER_TYPE_HASH, h)?.ok_or_else(|| format!("0x{h:08X}: no container"))?;
        let mut catalog = PlacedCatalog::default();
        catalog.add_block(&block, &templates)?;
        let entities: HashMap<u32, PlacedEntity> = placed_entities(&block)?.into_iter().map(|e| (e.key, e)).collect();
        let entity_names: HashMap<u32, String> = placed_names(&block).into_iter().collect();
        let links = mercs2_formats::physical_link::placed_links_in_order(&block)?;
        let mut incoming: HashMap<u32, (u32, PhysicalLink)> = HashMap::new();
        for (parent, l) in links.iter().filter(|(_, l)| entities.contains_key(&l.child)) {
            if let Some((was, _)) = incoming.insert(l.child, (*parent, *l)) {
                two_parents += 1;
                let at = live.get(&l.child);
                let parent_at = |k: u32| live.get(&k).copied();
                println!("two parents of 0x{:08X} live {at:?}: first 0x{was:08X} {:?}, last 0x{parent:08X} {:?}", l.child, parent_at(was), parent_at(*parent));
            }
            if l.hard_point != 0 {
                let model = catalog.get(*parent).and_then(|o| o.model).ok_or_else(|| format!("parent 0x{parent:08X}: a hard point and no model"))?;
                if !nodes.contains_key(&model) {
                    let c = assets.resolve(TYPE_HASH_MODEL, model).ok_or_else(|| format!("model 0x{model:08X}: in no open wad"))?;
                    nodes.insert(model, node_worlds(&c));
                }
            }
        }
        let resolved = |key: u32, use_node: &dyn Fn(u32) -> bool| -> Result<Mat4, String> {
            let (mut chain, mut at) = (Vec::new(), key);
            while let Some(&(parent, l)) = incoming.get(&at) {
                chain.push((parent, l));
                at = parent;
                if chain.len() > 8 {
                    return Err(format!("0x{key:08X}: a chain of links deeper than 8"));
                }
            }
            let root = entities.get(&at).ok_or_else(|| format!("0x{at:08X}: a link's parent with no Transform"))?;
            let mut out = transform(root.pos, root.quat);
            for (parent, l) in chain.iter().rev() {
                let node = if l.hard_point == 0 || !use_node(*parent) {
                    Mat4::IDENTITY
                } else {
                    let model = catalog.get(*parent).and_then(|o| o.model).expect("a hard point's model was read above");
                    *nodes[&model].get(&l.hard_point).ok_or_else(|| format!("model 0x{model:08X}: no node 0x{:08X}", l.hard_point))?
                };
                out = out * node * transform(l.pos, l.quat);
            }
            Ok(out)
        };
        for e in entities.values() {
            let child = incoming.get(&e.key);
            *flags.entry((e.flags, child.is_some())).or_default() += 1;
            let Some(&at) = live.get(&e.key) else {
                not_held += 1;
                continue;
            };
            held += 1;
            let stored = (at - Vec3::from(e.pos)).length();
            let name = entity_names.get(&e.key);
            if child.is_some() {
                let (_, _, translation) = resolved(e.key, &|p| awake.contains(&p))?.to_scale_rotation_translation();
                let (_, _, with_nodes) = resolved(e.key, &|_| true)?.to_scale_rotation_translation();
                let linked = (at - translation).length().min((at - with_nodes).length());
                child_stored[bucket(stored)] += 1;
                child_linked[bucket(linked)] += 1;
                if linked > 0.1 {
                    child_misses.push(format!("0x{h:08X} 0x{:08X} {name:?} live {at} stored {:?} linked {translation} off by {linked:.3}", e.key, e.pos));
                }
            } else {
                rest_stored[bucket(stored)] += 1;
                if stored > 0.1 {
                    rest_misses.push(format!("0x{h:08X} 0x{:08X} {name:?} live {at} stored {:?} off by {stored:.3}", e.key, e.pos));
                }
            }
        }
    }
    println!("{held} placed entities held by the capture, {not_held} not; {two_parents} children of two parents");
    println!("bucket upper bounds, metres: {BUCKETS:?} and beyond");
    println!("link children, live against Transform:        {child_stored:?}");
    println!("link children, live against the link's chain: {child_linked:?}");
    println!("every other entity, live against Transform:   {rest_stored:?}");
    println!("flag words (value, is a link child): {flags:04X?}");
    println!("{} link children off the chain by more than 0.1 m:", child_misses.len());
    for l in &child_misses {
        println!("  {l}");
    }
    println!("{} other entities off their Transform by more than 0.1 m:", rest_misses.len());
    for l in &rest_misses {
        println!("  {l}");
    }
    Ok(())
}

/// `layer --catalog <out.tsv> [--also <archive,archive>]`: one row per layer of the mounted stack, with
/// the archive that resolves it, its block's path, and its entities, models, link children and class
/// counts. A layer of an `--also` archive, which may be another platform's and is not read further,
/// gets a row with its archive and path alone.
pub fn layer_catalog(wadpath: &str, out: &Path, also: &[String], overlays: &[String]) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, overlays)?;
    let we = world(&mut assets)?;
    let templates = TemplateRecords::from_worldentity(&we)?;
    let classes = template_classes(&we);
    let mut rows = vec!["hash\tarchive\tblock\tpath\tentities\ttemplated\tmodels\tlink_children\tbounds_xz\ttop_names\tclasses".to_string()];
    let mut homes: BTreeMap<u32, (String, u16, String)> = BTreeMap::new();
    let read_layers = |path: &str| -> Result<Vec<(u32, String, u16, String)>, String> {
        let mut f = File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let size = f.metadata().map_err(|e| format!("{path}: {e}"))?.len();
        let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{path}: {e}"))?;
        let file = Path::new(path).file_name().map_or(path.to_string(), |n| n.to_string_lossy().into_owned());
        Ok(arch
            .aset
            .iter()
            .filter(|e| e.type_id == TYPE_ID_LAYER)
            .map(|e| (e.asset_hash, file.clone(), e.block_index(), arch.paths.get(e.block_index() as usize).cloned().unwrap_or_default()))
            .collect())
    };
    // The stack resolves last-mounted first, so a later archive's row replaces an earlier one's.
    for path in assets.paths().to_vec() {
        for (hash, file, block, block_path) in read_layers(&path)? {
            homes.insert(hash, (file, block, block_path));
        }
    }
    for (&h, (file, block_index, path)) in &homes {
        let block = assets.ecs_container(LAYER_TYPE_HASH, h)?.ok_or_else(|| format!("0x{h:08X}: no container"))?;
        let mut catalog = PlacedCatalog::default();
        let keys = catalog.add_block(&block, &templates)?;
        let own = entity_classes(&block)?;
        let entities: BTreeSet<u32> = keys.iter().copied().collect();
        let linked = mercs2_formats::physical_link::placed_links_in_order(&block)?.into_iter().map(|(_, l)| l.child).filter(|c| entities.contains(c)).collect::<BTreeSet<u32>>().len();
        let children: BTreeSet<u32> = mercs2_formats::physical_link::placed_links_in_order(&block)?.into_iter().map(|(_, l)| l.child).collect();
        let (mut templated, mut models) = (0usize, 0usize);
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        let mut stems: BTreeMap<String, usize> = BTreeMap::new();
        let entity_names: HashMap<u32, String> = placed_names(&block).into_iter().collect();
        for &k in &keys {
            let o = catalog.get(k).expect("add_block returned the key");
            templated += usize::from(o.parent != 0);
            models += usize::from(o.model.is_some());
            if !children.contains(&k) {
                lo = [lo[0].min(o.pos[0]), lo[1].min(o.pos[2])];
                hi = [hi[0].max(o.pos[0]), hi[1].max(o.pos[2])];
            }
            if let Some(n) = entity_names.get(&k) {
                let stem = n.split(" 0x").next().unwrap_or(n).trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == '_');
                *stems.entry(stem.to_string()).or_default() += 1;
            }
            let set: BTreeSet<&str> = own.get(&k).into_iter().chain(classes.get(&o.parent)).flatten().map(String::as_str).collect();
            for c in set {
                *counts.entry(c).or_default() += 1;
            }
        }
        let mut listed: Vec<(&str, usize)> = counts.into_iter().filter(|(c, _)| !matches!(*c, "Name" | "HibernationControl")).collect();
        listed.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let listed: Vec<String> = listed.iter().map(|(c, n)| format!("{c}:{n}")).collect();
        let mut names: Vec<(String, usize)> = stems.into_iter().collect();
        names.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let names: Vec<String> = names.iter().take(6).map(|(s, n)| format!("{s}:{n}")).collect();
        let bounds = if keys.is_empty() || lo[0] > hi[0] { String::new() } else { format!("{:.0},{:.0},{:.0},{:.0}", lo[0], lo[1], hi[0], hi[1]) };
        rows.push(format!("0x{h:08X}\t{file}\t{block_index}\t{path}\t{}\t{templated}\t{models}\t{linked}\t{bounds}\t{}\t{}", keys.len(), names.join(","), listed.join(",")));
    }
    for path in also {
        for (hash, file, block, block_path) in read_layers(path)? {
            rows.push(format!("0x{hash:08X}\t{file}\t{block}\t{block_path}\t\t\t\t\t\t\t"));
        }
    }
    println!("{} rows, {} of the mounted stack", rows.len() - 1, homes.len());
    std::fs::write(out, rows.join("\n") + "\n").map_err(|e| format!("{}: {e}", out.display()))
}
