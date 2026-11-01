//! `layer` — what a placement layer's entities are, as the WAD stack resolves them.
//!
//! `layer <name|0xH>` prints every entity of the layer: its key, name, template, model and component
//! classes (its own, then its template's). `layer --all` reads all of them and prints how many
//! entities draw a model and which classes the rest carry, and fails when a layer does not read.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;

use mercs2_engine::asset::AssetSource;
use mercs2_engine::worldutil::{PlacedCatalog, TemplateRecords, LAYER_TYPE_HASH};
use mercs2_formats::placement::{entity_classes, placed_names};
use mercs2_formats::types::TYPE_ID_LAYER;
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
        .resolve(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)
        .ok_or("worldentity: in no open wad")?;
    WorldEntity::parse(&c).map_err(|e| format!("worldentity: {e}"))
}

pub fn layer(wadpath: &str, hash: u32) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let we = world(&mut assets)?;
    let templates = TemplateRecords::from_worldentity(&we)?;
    let names: HashMap<u32, &str> = templates.names.iter().map(|(k, n)| (*k, n.as_str())).collect();
    let classes = template_classes(&we);
    let block = assets.resolve(LAYER_TYPE_HASH, hash).ok_or_else(|| format!("0x{hash:08X}: no layer in {:?}", assets.paths()))?;
    let mut catalog = PlacedCatalog::default();
    let keys = catalog.add_block(&block, &templates)?;
    let own = entity_classes(&block)?;
    let entity_names: HashMap<u32, String> = placed_names(&block).into_iter().collect();
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
        let Some(block) = assets.resolve(LAYER_TYPE_HASH, h) else {
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
