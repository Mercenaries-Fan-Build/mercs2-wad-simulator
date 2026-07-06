//! Headless verification probe for the texture/material extraction module.
//!
//! Dumps, per drawing group of a model, its material index, the material's
//! diffuse texture hash + resolved name, and that texture's width/height/format.
//!
//! Usage:
//!   cargo run -p mercs2_formats --example texture_probe -- <vz.wad> 0xA3C1FABC

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::texture::{
    extract_model, extract_texture, extract_texture_name, group_prmt_material_indices, parse_mtrl,
};
use std::fs::File;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: texture_probe <vz.wad> <model_hash e.g. 0xA3C1FABC>");
        std::process::exit(2);
    }
    let wad = &args[1];
    let hash = u32::from_str_radix(args[2].trim_start_matches("0x"), 16).expect("hex model hash");

    let mut file = File::open(wad).expect("open wad");
    let size = file.metadata().expect("stat").len();
    let archive = load_ffcs_archive(&mut file, size).expect("load ffcs");

    let model = extract_model(&mut file, &archive, hash).expect("extract model container");
    println!(
        "model 0x{hash:08X}: {} byte UCFX container",
        model.len()
    );

    let materials = parse_mtrl(&model);
    println!("\n=== {} materials (MTRL) ===", materials.len());
    for (mi, m) in materials.iter().enumerate() {
        let d = m.diffuse().unwrap_or(0);
        let name = extract_texture_name(&mut file, &archive, d).unwrap_or_else(|| "?".into());
        println!(
            "  mat[{mi:2}] tex_count={} diffuse=0x{d:08X} ({name})  slots={:08X?}",
            m.textures.len(),
            m.textures
        );
    }

    let groups = group_prmt_material_indices(&model);
    println!("\n=== {} drawing groups (PRMG -> material) ===", groups.len());
    for (gi, mats) in groups.iter().enumerate() {
        let mut parts = Vec::new();
        for &mi in mats {
            let diffuse = materials.get(mi).and_then(|m| m.diffuse()).unwrap_or(0);
            let name = extract_texture_name(&mut file, &archive, diffuse)
                .unwrap_or_else(|| "unresolved".into());
            let dims = match extract_texture(&mut file, &archive, diffuse) {
                Ok(t) => format!("{}x{} {:?}", t.width, t.height, t.format),
                Err(_) => "(no texture asset)".into(),
            };
            parts.push(format!(
                "mat{mi}->0x{diffuse:08X} {name} [{dims}]"
            ));
        }
        println!("  G{gi:2}: {}", parts.join("  |  "));
    }

    // Detailed dump of each distinct diffuse texture actually present in the WAD.
    println!("\n=== distinct diffuse textures (dims / format / mip chain) ===");
    let mut seen = std::collections::BTreeSet::new();
    for m in &materials {
        if let Some(d) = m.diffuse() {
            if !seen.insert(d) {
                continue;
            }
            match extract_texture(&mut file, &archive, d) {
                Ok(t) => {
                    let name =
                        extract_texture_name(&mut file, &archive, d).unwrap_or_else(|| "?".into());
                    println!(
                        "  0x{d:08X} {name:<32} {}x{} {:?} mips={} body={} mip0={}",
                        t.width,
                        t.height,
                        t.format,
                        t.mip_count,
                        t.all_mips.len(),
                        t.mip0.len()
                    );
                }
                Err(e) => println!("  0x{d:08X} {e}"),
            }
        }
    }
}
