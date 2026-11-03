//! `facefx` — the FaceFX assets (`mercs2_formats::facefx`).
//!
//! `facefx <name|0xH>` prints the actor and the anim set of that name, as the WAD stack
//! (`AssetSource`, patch overlays included) resolves them: an actor's face bones with their
//! reference pose, its face-graph nodes with the bones each pose sets, and a set's anims with their
//! curves. `facefx --all` reads every FaceFX container of every mounted WAD, prints the census (node
//! classes, link functions, curve names, interpolations), how many takes name a voice clip and an
//! `english` string, and each set's speaker ([`facecast`]), and fails when any container fails.

use std::collections::BTreeMap;
use std::fs::File;

use mercs2_engine::asset::AssetSource;
use mercs2_engine::facecast;
use mercs2_formats::facefx::{self, Actor, AnimSet, NodeKind};
use mercs2_formats::types::{TYPE_HASH_FACEFX_ACTOR, TYPE_HASH_FACEFX_ANIMSET, TYPE_HASH_SCRIPT, TYPE_HASH_STRINGDB, TYPE_HASH_WAVEBANK};
use mercs2_formats::worldentity::{WorldEntity, WORLDENTITY_TYPE_HASH};

pub fn facefx(wadpath: &str, name: u32) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let actor = assets.resolve(TYPE_HASH_FACEFX_ACTOR, name);
    let set = assets.resolve(TYPE_HASH_FACEFX_ANIMSET, name);
    if actor.is_none() && set.is_none() {
        return Err(format!("0x{name:08X}: no facefxactor or facefxanimationset in {:?}", assets.paths()));
    }
    if let Some(c) = actor {
        let a = facefx::parse_actor_container(&c).map_err(|e| format!("facefxactor 0x{name:08X}: {e}"))?;
        println!("facefxactor 0x{name:08X}: version {} ({} / {})", a.header.version, a.header.licensee, a.header.project);
        print_actor(&a.root);
        // The model of the same name: each face bone's HIER local beside the actor's reference
        // pose as the solve writes it into the clip's space.
        if let Some(m) = assets.resolve(mercs2_formats::types::TYPE_HASH_MODEL, name) {
            let hier = mercs2_formats::orchestrator::parse_hier(&m);
            let rig = mercs2_engine::anim::facefx::FaceRig::new(&a.root)?;
            println!("model 0x{name:08X}: {} HIER nodes; face bone reference pose vs HIER local", hier.len());
            for p in rig.reference_pose() {
                let h = mercs2_formats::hash::pandemic_hash_m2(&p.name);
                match hier.iter().find(|n| n.hash == h) {
                    Some(n) => {
                        let m: [[f32; 4]; 4] = std::array::from_fn(|r| std::array::from_fn(|c| n.local[r * 4 + c]));
                        let q = mercs2_engine::pose::mat_to_qs(&m);
                        println!("  {:<26} face t {:?} r {:?}", p.name, p.local.translation, p.local.rotation);
                        println!("  {:<26} HIER t {:?} r {:?}", "", q.translation, q.rotation);
                    }
                    None => println!("  {:<26} not in the model's HIER", p.name),
                }
            }
        }
    }
    if let Some(c) = set {
        let s = facefx::parse_animset_container(&c).map_err(|e| format!("facefxanimationset 0x{name:08X}: {e}"))?;
        println!("facefxanimationset 0x{name:08X}: version {}", s.header.version);
        print_set(&s.root);
    }
    Ok(())
}

fn print_actor(a: &Actor) {
    println!("actor {:?}: {} master bones, {} nodes", a.name, a.master_bones.len(), a.nodes.len());
    for b in &a.master_bones {
        println!(
            "  bone {:<26} pos {:?} rot(wxyz) {:?} scale {:?} index {} weight {:?}",
            b.bone.name, b.bone.position, b.bone.rotation, b.bone.scale, b.index, b.weight
        );
    }
    for n in &a.nodes {
        println!(
            "  node {:<24} {} min {} max {} op {} links {} properties {}",
            n.name,
            n.class,
            n.min,
            n.max,
            n.input_operation,
            n.links.len(),
            n.properties.len()
        );
        for l in &n.links {
            println!("    link from {:?} via {} {:?}", l.input, l.function, l.params);
        }
        if let NodeKind::BonePose(bones) = &n.kind {
            for b in bones {
                println!("    sets {:<26} pos {:?} rot(wxyz) {:?} scale {:?}", b.name, b.position, b.rotation, b.scale);
            }
        }
    }
    for g in &a.groups {
        println!("  anim group {:?}: {} anims", g.name, g.anims.len());
    }
    if !a.names.is_empty() {
        println!("  names {:?}", a.names);
    }
    if let Some(m) = &a.phoneme_map {
        println!("  phoneme map: {} mappings, {} targets", m.mappings.len(), m.targets.len());
    }
}

fn print_set(s: &AnimSet) {
    println!("anim set {:?} for actor {:?}: group {:?}, {} anims", s.name, s.actor, s.group.name, s.group.anims.len());
    for a in &s.group.anims {
        println!(
            "  anim {:<40} {} curves, ends {:.3}s, f20 {} f24 {}, bone weights {}",
            a.name,
            a.curves.len(),
            a.bounds().1,
            a.f20,
            a.f24,
            a.bone_weights.len()
        );
        for c in &a.curves {
            let (lo, hi) = c.keys.iter().map(|k| k.value()).fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v), h.max(v)));
            let peak = c.keys.iter().max_by(|a, b| a.value().total_cmp(&b.value())).map_or(0.0, |k| k.time());
            println!(
                "    curve {:<26} interpolation {} keys {:>3} values {:.3}..{:.3} (highest at {peak:.3}s)",
                c.name,
                c.interpolation,
                c.keys.len(),
                if c.keys.is_empty() { 0.0 } else { lo },
                if c.keys.is_empty() { 0.0 } else { hi }
            );
        }
    }
}

/// Every FaceFX container of every mounted WAD, read; the census of what they hold.
pub fn facefx_all(wadpath: &str) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let mut failed = Vec::new();
    let (mut actors, mut sets) = (0usize, 0usize);
    let mut classes: BTreeMap<String, usize> = BTreeMap::new();
    let mut functions: BTreeMap<String, usize> = BTreeMap::new();
    let mut curves: BTreeMap<String, usize> = BTreeMap::new();
    let mut interpolations: BTreeMap<i32, usize> = BTreeMap::new();
    // Keys, and keys whose third or fourth float (the slopes the Hermite reads) is not 0.
    let (mut keys, mut sloped) = (0usize, 0usize);
    // Every wavebank clip hash, and every anim's name: does an anim name a voice clip?
    let mut clips: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    let mut anim_names: Vec<String> = Vec::new();
    // Each set's name, asset name hash and the actor it names; every actor asset's name hash; the
    // world's templates.
    let mut set_actors: Vec<(String, u32, String)> = Vec::new();
    let mut actor_hashes: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut worldentity: Option<Vec<u8>> = None;
    // The `english` string table: is an anim's name also the key of its line's text?
    let mut english: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    for path in assets.paths() {
        let mut f = File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let size = f.metadata().map_err(|e| format!("{path}: {e}"))?.len();
        let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{path}: {e}"))?;
        for bi in 0..arch.indx.len() {
            let block = mercs2_formats::sges::decompress_block(&mut f, &arch.indx, bi as u16)
                .map_err(|e| format!("{path} block {bi}: {e}"))?;
            for (e, span) in mercs2_formats::ucfx::block_chunk_spans(&block).map_err(|e| format!("{path} block {bi}: {e}"))? {
                let what = format!("{path} block {bi} ({}) 0x{:08X}", arch.paths[bi], e.name_hash);
                let c = &block[span];
                if e.type_hash == TYPE_HASH_FACEFX_ACTOR {
                    actors += 1;
                    actor_hashes.insert(e.name_hash);
                    match facefx::parse_actor_container(c) {
                        Ok(a) => {
                            for n in &a.root.nodes {
                                *classes.entry(n.class.clone()).or_default() += 1;
                                for l in &n.links {
                                    *functions.entry(l.function.clone()).or_default() += 1;
                                }
                            }
                        }
                        Err(err) => failed.push(format!("facefxactor {what}: {err}")),
                    }
                } else if e.type_hash == WORLDENTITY_TYPE_HASH {
                    worldentity = Some(c.to_vec());
                } else if e.type_hash == TYPE_HASH_STRINGDB && e.name_hash == mercs2_formats::hash::pandemic_hash_m2("english") {
                    let bodies = (mercs2_formats::ucfx::extract_chunk_body(c, b"KEYS"), mercs2_formats::ucfx::extract_chunk_body(c, b"STRS"));
                    match bodies {
                        (Some(k), Some(s)) => match mercs2_formats::stringdb::parse(&k, &s) {
                            Ok(db) => english.extend(db.entries.into_iter().map(|e| (e.key_hash, e.text))),
                            Err(err) => failed.push(format!("stringdb {what}: {err}")),
                        },
                        _ => failed.push(format!("stringdb {what}: no KEYS or STRS chunk")),
                    }
                } else if e.type_hash == TYPE_HASH_WAVEBANK {
                    let body = mercs2_formats::ucfx::extract_chunk_body(c, b"data");
                    match body.map(|b| mercs2_engine::audio::wave::WavebankFile::parse(&b)) {
                        Some(Ok(w)) => {
                            let bank = w.stream_name.clone().unwrap_or_else(|| format!("embedded 0x{:08X}", e.name_hash));
                            clips.extend(w.records.iter().map(|r| (r.clip_hash, bank.clone())));
                        }
                        Some(Err(err)) => failed.push(format!("wavebank {what}: {err:?}")),
                        None => failed.push(format!("wavebank {what}: no data chunk")),
                    }
                } else if e.type_hash == TYPE_HASH_FACEFX_ANIMSET {
                    sets += 1;
                    match facefx::parse_animset_container(c) {
                        Ok(s) => {
                            set_actors.push((s.root.name.clone(), e.name_hash, s.root.actor.clone()));
                            for a in &s.root.group.anims {
                                anim_names.push(a.name.clone());
                                for c in &a.curves {
                                    *curves.entry(c.name.clone()).or_default() += 1;
                                    *interpolations.entry(c.interpolation).or_default() += 1;
                                    keys += c.keys.len();
                                    sloped += c.keys.iter().filter(|k| k.0[2] != 0.0 || k.0[3] != 0.0).count();
                                }
                            }
                        }
                        Err(err) => failed.push(format!("facefxanimationset {what}: {err}")),
                    }
                }
            }
        }
    }
    println!("{actors} facefxactor and {sets} facefxanimationset containers in {:?}", assets.paths());
    println!("node classes: {classes:?}");
    println!("link functions: {functions:?}");
    println!("curve interpolations: {interpolations:?}");
    println!("curve keys: {keys}, with a non-zero third or fourth float: {sloped}");
    let mut banks: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unmatched: Vec<&String> = Vec::new();
    for n in &anim_names {
        match clips.get(&mercs2_formats::hash::pandemic_hash_m2(n)) {
            Some(b) => *banks.entry(b.as_str()).or_default() += 1,
            None => unmatched.push(n),
        }
    }
    println!(
        "anims whose name hashes to a clip of a wavebank ({} clips): {} of {}, by bank {banks:?}; the others:",
        clips.len(),
        anim_names.len() - unmatched.len(),
        anim_names.len()
    );
    for n in &unmatched {
        println!("  {n}");
    }
    let texted = |n: &&String| {
        english.get(&mercs2_formats::stringdb::key_hash(n)).is_some_and(|t| !t.trim().is_empty())
    };
    let untexted: Vec<&String> = anim_names.iter().filter(|n| !texted(n)).collect();
    println!(
        "anims whose name is a key of the english string table ({} keys): {} of {}; the others:",
        english.len(),
        anim_names.len() - untexted.len(),
        anim_names.len()
    );
    for n in &untexted {
        println!("  {n}");
    }
    println!("curve names ({}):", curves.len());
    for (n, k) in &curves {
        println!("  {n:<30} {k}");
    }
    // Who speaks each set, as the shipped scripts bind it (`mercs2_engine::facecast`).
    set_actors.sort();
    let we = worldentity.ok_or("no worldentity in any mounted WAD")?;
    let we = WorldEntity::parse(&we).map_err(|e| format!("worldentity: {e}"))?;
    let templates = mercs2_formats::mixr::kit_templates(&we)?;
    let mut scripts = std::collections::HashMap::new();
    for m in facecast::MODULES {
        let name = m.to_ascii_lowercase();
        let hash = mercs2_formats::hash::pandemic_hash_m2(&name);
        let c = assets.resolve(TYPE_HASH_SCRIPT, hash).ok_or_else(|| format!("script {name}: not in any mounted WAD"))?;
        scripts.insert(name, facecast::script_luaq(&c)?);
    }
    let fetch = move |name: &str| scripts.get(name).cloned().ok_or_else(|| format!("{name} is not one of facecast::MODULES"));
    let mut wads = assets.paths().iter().map(|p| mercs2_engine::wad::open(p)).collect::<Result<Vec<_>, _>>()?;
    let cast = facecast::Cast::read(fetch, |layer| facecast::layer_block(&mut wads, layer), &templates)?;
    let mut faces = 0;
    for (name, hash, _) in &set_actors {
        match cast.sets.get(hash) {
            Some(Ok(s)) => {
                let asset = actor_hashes.contains(&s.actor);
                faces += usize::from(asset);
                println!(
                    "  {name:<36} {:<24} model 0x{:08X} actor 0x{:08X}{}  {}",
                    s.template,
                    s.model,
                    s.actor,
                    if asset { "" } else { " (NO SUCH ACTOR)" },
                    s.via
                );
            }
            Some(Err(e)) => println!("  {name:<36} NONE: {e}"),
            None => println!("  {name:<36} NONE: no script binds it"),
        }
    }
    println!("sets with a speaker whose actor exists: {faces} of {}", set_actors.len());
    for f in &failed {
        eprintln!("FAILED {f}");
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} of {} containers failed", failed.len(), actors + sets))
    }
}
