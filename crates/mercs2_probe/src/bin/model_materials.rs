//! Every draw of one model with the material that draws it: rung, SEGM row, HIER node (with its name
//! when a candidate hashes to it), LOD mask, the material's on-disk flags, blend mode, alpha test,
//! alpha multiplier and pixel shader, and the diffuse texture.
//!
//! `model_materials <name|0xHASH> [names.txt...]` — each `names.txt` lists candidate names, one per
//! line, hashed with `pandemic_hash_m2` to label node hashes and pixel-shader keys. A hash match
//! labels a node; it is not by itself proof that the name is the node's real name.

use std::collections::BTreeMap;

use mercs2_engine::model::Model;
use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::texture::{material_alpha_tested, runtime_material_flags};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let target = args.first().unwrap_or_else(|| panic!("usage: model_materials <name|0xHASH> [names.txt...]"));
    let hash = match target.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16).unwrap_or_else(|e| panic!("{target}: {e}")),
        None => pandemic_hash_m2(target),
    };
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    for p in &args[1..] {
        for l in std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {p}: {e}")).lines() {
            let l = l.trim();
            if !l.is_empty() {
                names.insert(pandemic_hash_m2(l), l.to_string());
            }
        }
    }
    let label = |k: u32| names.get(&k).cloned().unwrap_or_else(|| format!("0x{k:08X}"));

    let start = std::env::current_dir().expect("cwd");
    let vz = mercs2_formats::game_paths::local_config_vz_wad(&start).unwrap_or_else(|e| panic!("{e}"));
    let mut w = wad::open(vz.to_str().expect("UTF-8 vz.wad path")).unwrap_or_else(|e| panic!("{e}"));
    let m = Model::load(&mut w, hash).unwrap_or_else(|e| panic!("0x{hash:08X}: {e}"));

    println!("0x{hash:08X}: {} rungs, {} HIER nodes", m.rungs.len(), m.hier.len());
    // The resident block's materials in full: every slot and the materialData colour words
    // (material_shader_spec.md §1e: diffuse = words 1-3, emissive = words 10-12, alpha = word 13).
    let container = wad::extract_container(&mut w, hash).unwrap_or_else(|e| panic!("0x{hash:08X}: {e}"));
    let mats = mercs2_formats::texture::parse_mtrl(&container, mercs2_formats::texture::MtrlSource::Model)
        .unwrap_or_else(|e| panic!("0x{hash:08X} MTRL: {e:?}"));
    for (i, mt) in mats.iter().enumerate() {
        let p = &mt.preamble;
        println!(
            "mat{i}: flags 0x{:04X} shader {} slots [{}] diffuse ({:.3} {:.3} {:.3}) ambient ({:.3} {:.3} {:.3}) emissive ({:.3} {:.3} {:.3}) alpha {:.3}",
            mt.flags,
            label(mt.shader_key),
            mt.textures.iter().map(|&t| label(t)).collect::<Vec<_>>().join(", "),
            p[1], p[2], p[3], p[7], p[8], p[9], p[10], p[11], p[12], p[13],
        );
    }
    println!("rung row  node  node-name                         lod   flags  blend  test  alpha  shader                        diffuse");
    for (ri, r) in m.rungs.iter().enumerate() {
        for d in &r.draws {
            let node = if d.node >= 0 {
                m.hier.get(d.node as usize).map(|n| label(n.hash)).unwrap_or_else(|| "?".into())
            } else {
                "-".into()
            };
            let rt = runtime_material_flags(d.material_flags);
            let blend = match rt & 0xF {
                0 => "opaque",
                1 => "alpha",
                2 => "add",
                4 => "sub",
                8 => "revsub",
                _ => "?",
            };
            println!(
                "{ri:>4} {:>4} {:>5}  {node:<32} 0x{:02X}  0x{:04X}  {blend:<6} {:<5} {:.2}   {:<29} {}",
                d.segm_row,
                d.node,
                d.lod_mask,
                d.material_flags,
                if material_alpha_tested(d.material_flags) { "yes" } else { "no" },
                d.material_data[5][3],
                label(d.shader_key),
                d.diffuse.map(|h| label(h)).unwrap_or_else(|| "-".into()),
            );
        }
    }
    // The vertex COLOR alpha each blended draw's vertices carry: the `*AmbOcc*` shaders multiply their
    // output alpha by it (material_shader_spec.md §1e).
    let (verts, indices, draws, _) = m.flatten();
    println!("blended draws: lod  flags   vertex alpha min..max");
    for d in draws.iter().filter(|d| mercs2_formats::texture::material_blended(d.material_flags)) {
        let range = &indices[d.index_start as usize..(d.index_start + d.index_count) as usize];
        let (lo, hi) = range.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &i| {
            let a = verts[i as usize].color[3];
            (lo.min(a), hi.max(a))
        });
        println!("  0x{:02X}  0x{:04X}  {lo:.3}..{hi:.3}  {}", d.lod_mask, d.material_flags, label(d.shader_key));
    }
}
