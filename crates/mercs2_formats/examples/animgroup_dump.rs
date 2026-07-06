//! Parse one real human-character animgroup from retail vz.wad and print:
//! block hash/name, #packfiles, class census, #clips + names/types, skeleton bone
//! count, binding track count, and the HIER-vs-animgroup bone-order correspondence
//! verdict (spec Open Q#1).
//!
//! Not a `#[test]` because it needs the ~2.5 GB retail `vz.wad`, which is absent in
//! CI. Run locally:
//!   cargo run -p mercs2_formats --example animgroup_dump
//!
//! Optional args: `<anim_block_index> <model_block_index>` to force a pairing.
//! Default: auto-discovers the shared human rig (a 60+-track animgroup) and the
//! biggest-HIER character model, then reports their name-hash correspondence.

use std::collections::BTreeSet;
use std::fs::File;

use mercs2_formats::animgroup::{parse_animgroup, AnimBinding};
use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le, FfcsArchive};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_ANIMATION, TYPE_HASH_MODEL};
use mercs2_formats::ucfx::extract_chunk_body;

const WAD: &str = "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad";

fn main() {
    let path = std::env::var("VZ_WAD").unwrap_or_else(|_| WAD.to_string());
    let mut f = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot open vz.wad at {path}: {e}");
            eprintln!("set VZ_WAD=<path> or place the retail WAD at the default location.");
            return;
        }
    };
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).expect("ffcs");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (anim_blk, model_blk) = if args.len() >= 2 {
        (args[0].parse().unwrap(), Some(args[1].parse().unwrap()))
    } else {
        discover(&mut f, &arch)
    };

    println!("================ ANIMGROUP DUMP ================");
    let block_hash = anim_block_hash(&arch, anim_blk);
    println!(
        "animgroup block[{anim_blk}]  (an ASET asset_hash referencing it = {})",
        block_hash.map(|h| format!("0x{h:08X}")).unwrap_or_else(|| "?".into())
    );

    let data = decompress_block(&mut f, &arch.indx, anim_blk).expect("decompress animgroup");
    let ag = parse_animgroup(&data).expect("parse animgroup");

    println!("\n-- embedded Havok packfiles --");
    println!("  packfile_count = {}", ag.packfile_count);
    println!("  class census:");
    for (name, count) in &ag.class_census {
        println!("    {count:6}  {name}");
    }

    println!("\n-- clips ({}) --", ag.clips.len());
    println!("  {:<12} {:<12} {:>6} {:>6} {:>8} {:>7}", "name", "class", "tracks", "float", "duration", "poses");
    for c in ag.clips.iter().take(40) {
        println!(
            "  {:<12} {:<12} {:>6} {:>6} {:>8.3} {:>7}",
            c.name, c.class, c.num_transform_tracks, c.num_float_tracks, c.duration, c.num_poses
        );
    }
    if ag.clips.len() > 40 {
        println!("  … {} more clips", ag.clips.len() - 40);
    }

    let skel_bones = ag.skeleton.as_ref().map(|s| s.bone_name_hashes.len()).unwrap_or(0);
    let track_count = ag.binding.as_ref().map(|b| b.track_to_bone_hash.len()).unwrap_or(0);
    println!("\n-- skeleton / binding --");
    println!("  derived skeleton bone-name-hash count = {skel_bones}");
    println!("  primary binding transform-track count = {track_count}");
    println!(
        "  hkaSkeleton present? {}  |  hkaAnimationBinding present? {}",
        ag.class_census.contains_key("hkaSkeleton"),
        ag.class_census.contains_key("hkaAnimationBinding"),
    );
    if !ag.class_census.contains_key("hkaSkeleton") {
        println!("  (retail data ships NO hkaSkeleton / hkaAnimationBinding instances;");
        println!("   the track→bone binding is the Pandemic 'trnm' name-hash table.)");
    }

    // Open Q#1 — HIER vs animgroup bone correspondence.
    let Some(model_blk) = model_blk else {
        println!("\n(no paired model block found; skipping HIER correspondence.)");
        return;
    };
    let md = decompress_block(&mut f, &arch.indx, model_blk).expect("decompress model");
    let Some(hier) = first_hier(&md) else {
        println!("\n(model block[{model_blk}] has no HIER; skipping correspondence.)");
        return;
    };
    let Some(binding) = &ag.binding else { return };

    println!("\n================ OPEN Q#1: HIER vs animgroup bone order ================");
    println!("  model block[{model_blk}] HIER node count = {}", hier.len());
    println!("  animgroup rig track count             = {}", binding.track_to_bone_hash.len());

    let hset: BTreeSet<u32> = hier.iter().copied().collect();
    let tset: BTreeSet<u32> = binding.track_to_bone_hash.iter().copied().collect();
    let inter = hset.intersection(&tset).count();
    let resolved = binding.resolve_to_hier(&hier);
    let mapped = resolved.iter().filter(|r| r.is_some()).count();

    let n = binding.track_to_bone_hash.len().min(hier.len());
    let same_order = (0..n).filter(|&i| binding.track_to_bone_hash[i] == hier[i]).count();

    println!("  name-hash set intersection            = {inter}");
    println!("  tracks resolvable to a HIER bone       = {mapped}/{}", binding.track_to_bone_hash.len());
    println!("  index-order identical positions        = {same_order}/{n}");

    let verdict = if same_order == n && n == hier.len() {
        "IDENTICAL-ORDER (no runtime remap needed)"
    } else if inter as f64 >= 0.75 * n as f64 {
        "SAME-SET, DIFFERENT-ORDER (name-hash track→HIER remap REQUIRED at runtime)"
    } else {
        "DISJOINT (tracks and HIER do not share bone identities — investigate)"
    };
    println!("\n  VERDICT: {verdict}");
    println!("  Integrator: call AnimBinding::resolve_to_hier(&hier_name_hashes) to get");
    println!("  a Vec<Option<usize>> mapping each animation track to its HIER bone index.");

    // Show the first few mappings as concrete evidence.
    println!("\n  first 10 track→HIER-bone-index (by name-hash):");
    for (t, r) in resolved.iter().take(10).enumerate() {
        let th = binding.track_to_bone_hash[t];
        match r {
            Some(i) => println!("    track[{t:>2}] 0x{th:08X} -> HIER[{i}]"),
            None => println!("    track[{t:>2}] 0x{th:08X} -> (not in this model's HIER)"),
        }
    }
    let _ = AnimBinding { track_to_bone: vec![], track_to_bone_hash: vec![] };
}

/// Auto-discover the widest human animgroup and the best-matching HIER model.
fn discover(f: &mut File, arch: &FfcsArchive) -> (u16, Option<u16>) {
    let mut blocks: Vec<u16> = arch.aset.iter().map(|e| e.block_index()).collect();
    blocks.sort();
    blocks.dedup();

    let mut best_anim: Option<(u16, usize)> = None; // (blk, track count)
    let mut big_hiers: Vec<(u16, Vec<u32>)> = Vec::new();
    for &blk in &blocks {
        let Ok(data) = decompress_block(f, &arch.indx, blk) else { continue };
        let ents = entries(&data);
        let mut pos = 4 + ents.len() * 16;
        let mut has_anim = false;
        for (_, th, _, sz) in &ents {
            let sz = *sz as usize;
            if pos + sz > data.len() { break; }
            let cont = &data[pos..pos + sz];
            pos += sz;
            if *th == TYPE_HASH_ANIMATION { has_anim = true; }
            if *th == TYPE_HASH_MODEL {
                if let Some(h) = hier_hashes(cont) {
                    if h.len() >= 60 { big_hiers.push((blk, h)); }
                }
            }
        }
        if has_anim {
            if let Ok(ag) = parse_animgroup(&data) {
                if let Some(b) = &ag.binding {
                    let n = b.track_to_bone_hash.len();
                    if best_anim.map(|(_, m)| n > m).unwrap_or(true) {
                        best_anim = Some((blk, n));
                    }
                }
            }
        }
    }
    let anim = best_anim.map(|(b, _)| b).unwrap_or(3315);

    // pick the HIER model with the highest name-hash intersection with this rig
    let rig: BTreeSet<u32> = {
        let d = decompress_block(f, &arch.indx, anim).unwrap();
        parse_animgroup(&d).unwrap().binding
            .map(|b| b.track_to_bone_hash.into_iter().collect())
            .unwrap_or_default()
    };
    let model = big_hiers.into_iter()
        .max_by_key(|(_, h)| h.iter().filter(|x| rig.contains(x)).count())
        .map(|(b, _)| b);
    (anim, model)
}

fn anim_block_hash(arch: &FfcsArchive, blk: u16) -> Option<u32> {
    arch.aset.iter()
        .find(|e| e.block_index() == blk && e.type_id == mercs2_formats::types::TYPE_ID_ANIMATION)
        .map(|e| e.asset_hash)
}

fn entries(data: &[u8]) -> Vec<(u32, u32, u32, u32)> {
    if data.len() < 4 { return vec![]; }
    let c = read_u32_le(data, 0) as usize;
    let mx = (data.len() - 4) / 16;
    let c = c.min(mx);
    (0..c).map(|i| {
        let b = 4 + i * 16;
        (read_u32_le(data, b), read_u32_le(data, b + 4), read_u32_le(data, b + 8), read_u32_le(data, b + 12))
    }).collect()
}

fn hier_hashes(c: &[u8]) -> Option<Vec<u32>> {
    let h = extract_chunk_body(c, b"HIER")?;
    if h.len() < 176 || h.len() % 176 != 0 { return None; }
    let n = h.len() / 176;
    Some((0..n).map(|i| read_u32_le(&h, i * 176)).collect())
}

fn first_hier(block: &[u8]) -> Option<Vec<u32>> {
    let ents = entries(block);
    let mut pos = 4 + ents.len() * 16;
    for (_, th, _, sz) in &ents {
        let sz = *sz as usize;
        if pos + sz > block.len() { break; }
        let cont = &block[pos..pos + sz];
        pos += sz;
        if *th == TYPE_HASH_MODEL {
            if let Some(h) = hier_hashes(cont) { return Some(h); }
        }
    }
    None
}
