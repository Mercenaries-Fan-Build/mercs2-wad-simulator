//! model_forge — author a static UCFX model container FROM SCRATCH (no donor).
//!
//! Reads a raw mesh blob and emits a complete model container via
//! `mercs2_formats::model_build`. Ship the result with
//! `smuggler --inject-extra 0x<hash>:19:<out>` (new asset, nothing overridden).
//!
//! Raw mesh format (little-endian), produced by `tools/fbx_preprocess.py --mesh`:
//!   "MESH" | u32 nverts | u32 ntris |
//!   pos f32[3*nverts] | nrm f32[3*nverts] | uv f32[2*nverts] | tris u32[3*ntris]
//!
//! Usage: model_forge <mesh.bin> <out.bin> --name <asset_name> --diffuse 0x<hash>

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::model_build::{build_skinned_model, build_static_model, StaticMesh};

fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn rd_f32(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let mut args = std::env::args().skip(1);
    let mut positional: Vec<String> = Vec::new();
    let mut name: Option<String> = None;
    let mut diffuse: Option<u32> = None;
    let mut skinned = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--skinned" => skinned = true,
            "--name" => name = args.next(),
            "--diffuse" => {
                diffuse = args
                    .next()
                    .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            }
            s => positional.push(s.to_string()),
        }
    }
    if positional.len() != 2 {
        eprintln!("usage: model_forge <mesh.bin> <out.bin> --name <asset_name> --diffuse 0x<hash>");
        return 2;
    }
    let (mesh_path, out_path) = (&positional[0], &positional[1]);
    let Some(name) = name else {
        eprintln!("--name is required");
        return 2;
    };
    let model_hash = pandemic_hash_m2(&name);
    // Default diffuse: a global-resident base texture so the model binds even
    // before its own texture ships. Override with --diffuse.
    let diffuse_hash = diffuse.unwrap_or_else(|| pandemic_hash_m2("global_defaultdiffuse"));

    let d = match std::fs::read(mesh_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read {mesh_path}: {e}");
            return 1;
        }
    };
    if d.len() < 12 || &d[0..4] != b"MESH" {
        eprintln!("not a MESH blob");
        return 1;
    }
    let nv = rd_u32(&d, 4) as usize;
    let nt = rd_u32(&d, 8) as usize;
    let mut o = 12;
    let mut positions = Vec::with_capacity(nv);
    for _ in 0..nv {
        positions.push([rd_f32(&d, o), rd_f32(&d, o + 4), rd_f32(&d, o + 8)]);
        o += 12;
    }
    let mut normals = Vec::with_capacity(nv);
    for _ in 0..nv {
        normals.push([rd_f32(&d, o), rd_f32(&d, o + 4), rd_f32(&d, o + 8)]);
        o += 12;
    }
    let mut uvs = Vec::with_capacity(nv);
    for _ in 0..nv {
        uvs.push([rd_f32(&d, o), rd_f32(&d, o + 4)]);
        o += 8;
    }
    let mut tris = Vec::with_capacity(nt);
    for _ in 0..nt {
        tris.push([rd_u32(&d, o), rd_u32(&d, o + 4), rd_u32(&d, o + 8)]);
        o += 12;
    }

    let mesh = StaticMesh { positions, normals, uvs, tris };
    let built = if skinned {
        build_skinned_model(&mesh, model_hash, diffuse_hash)
    } else {
        build_static_model(&mesh, model_hash, diffuse_hash)
    };
    match built {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(out_path, &bytes) {
                eprintln!("write {out_path}: {e}");
                return 1;
            }
            println!(
                "forged '{name}' (hash 0x{model_hash:08X}) diffuse 0x{diffuse_hash:08X}: {} verts, {} tris -> {out_path} ({} bytes)",
                nv,
                nt,
                bytes.len()
            );
            0
        }
        Err(e) => {
            eprintln!("build_static_model: {e}");
            1
        }
    }
}
