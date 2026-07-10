//! inject_static — conform a novel rigid mesh into a REAL static/vehicle template
//! container, instead of authoring a UCFX model from scratch (which the engine
//! rejects → 0x004CC064). Replaces one drawing group's geometry with the mesh,
//! encoded into the TEMPLATE's own vertex decl, and neutralises the other groups;
//! decl/material/shader/chunk-layout are preserved. See
//! docs/reverse_engineer/valid_model_structure_map.md.
//!
//! Input template is a RAW UCFX container (as produced by `smuggler
//! --dump-container`). Output is a RAW UCFX container ready for `smuggler
//! --inject-container`.
//!
//! Usage:
//!   inject_static <template.ucfx> <mesh.blob> <out.ucfx> --name-hash 0xHASH
//!       [--group N] [--diffuse-from 0xA --diffuse-to 0xB]

use mercs2_formats::model_inject::{inject_static_into_donor_block, ExternalMesh, MtrlRepoint};

const MODEL_TYPE_HASH: u32 = 0x5B72_4250; // pandemic_hash_m2("model")

fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn rd_f32(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn parse_hash(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut pos: Vec<String> = Vec::new();
    let (mut name_hash, mut group, mut df, mut dt) = (0u32, 0usize, None::<u32>, None::<u32>);
    let mut fit = true; // auto-fit to template size by default (--natural-scale to disable)
    let mut flip = true; // flip winding by default (fbx RH->engine LH); --no-flip to disable
    let mut keep = false; // --keep-groups: don't neutralise other template groups (diagnostic)
    let mut all = false; // --all-groups: inject the mesh into EVERY drawing group (guarantees visibility)
    let mut raw_targets: Vec<usize> = Vec::new(); // --raw-groups N,M,...: inject into these RAW group ordinals (the engine's rendered set)
    let mut scale_mult = 1.0f32; // --scale S: multiply the auto-fit scale (1.0 = exact fit to real body envelope)
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--name-hash" => name_hash = it.next().and_then(|s| parse_hash(s)).unwrap_or(0),
            "--group" => {
                group = match it.next().map(|s| s.as_str()) {
                    Some("largest") => usize::MAX,
                    Some(s) => s.parse().unwrap_or(0),
                    None => 0,
                }
            }
            // Raw group ordinal (index into the container's PRMG groups, not the
            // "has-geometry" drawing list) — targets the specific state-machine
            // RENDERED body group, e.g. UH1 group 14 (full-res huey body).
            "--raw-group" => {
                group = 0x1000_0000 + it.next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0)
            }
            "--diffuse-from" => df = it.next().and_then(|s| parse_hash(s)),
            "--diffuse-to" => dt = it.next().and_then(|s| parse_hash(s)),
            "--natural-scale" => fit = false,
            "--scale" => scale_mult = it.next().and_then(|s| s.parse().ok()).unwrap_or(1.0),
            "--no-flip" => flip = false,
            "--keep-groups" => keep = true,
            "--all-groups" => all = true,
            "--raw-groups" => {
                raw_targets = it
                    .next()
                    .map(|s| s.split(',').filter_map(|x| x.trim().parse::<usize>().ok()).collect())
                    .unwrap_or_default()
            }
            s => pos.push(s.to_string()),
        }
    }
    if pos.len() != 3 || name_hash == 0 {
        eprintln!("usage: inject_static <template.ucfx> <mesh.blob> <out.ucfx> --name-hash 0xHASH [--group N] [--diffuse-from 0xA --diffuse-to 0xB]");
        return 2;
    }

    // ---- template: raw UCFX -> wrap in a 20-byte WAD block for the injector ----
    let ucfx = match std::fs::read(&pos[0]) {
        Ok(b) => b,
        Err(e) => { eprintln!("read {}: {e}", pos[0]); return 1; }
    };
    if ucfx.len() < 8 || &ucfx[0..4] != b"UCFX" {
        eprintln!("{} is not a raw UCFX container", pos[0]);
        return 1;
    }
    let mut block = Vec::with_capacity(20 + ucfx.len());
    block.extend_from_slice(&1u32.to_le_bytes());
    block.extend_from_slice(&name_hash.to_le_bytes());
    block.extend_from_slice(&MODEL_TYPE_HASH.to_le_bytes());
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&(ucfx.len() as u32).to_le_bytes());
    block.extend_from_slice(&ucfx);

    // ---- mesh blob -> ExternalMesh ----
    let d = match std::fs::read(&pos[1]) {
        Ok(b) => b,
        Err(e) => { eprintln!("read {}: {e}", pos[1]); return 1; }
    };
    if d.len() < 12 || &d[0..4] != b"MESH" {
        eprintln!("{} is not a MESH blob", pos[1]);
        return 1;
    }
    let nv = rd_u32(&d, 4) as usize;
    let nt = rd_u32(&d, 8) as usize;
    let mut o = 12;
    let mut positions = Vec::with_capacity(nv);
    for _ in 0..nv { positions.push([rd_f32(&d, o), rd_f32(&d, o + 4), rd_f32(&d, o + 8)]); o += 12; }
    let mut normals = Vec::with_capacity(nv);
    for _ in 0..nv { normals.push([rd_f32(&d, o), rd_f32(&d, o + 4), rd_f32(&d, o + 8)]); o += 12; }
    let mut uvs = Vec::with_capacity(nv);
    for _ in 0..nv { uvs.push([rd_f32(&d, o), rd_f32(&d, o + 4)]); o += 8; }
    let mut tris = Vec::with_capacity(nt);
    for _ in 0..nt { tris.push([rd_u32(&d, o), rd_u32(&d, o + 4), rd_u32(&d, o + 8)]); o += 12; }

    let mesh = ExternalMesh { positions, normals, uvs, tris, joints: vec![], weights: vec![] };
    let repoints: Vec<MtrlRepoint> = match (df, dt) {
        (Some(from), Some(to)) => vec![MtrlRepoint { from, to }],
        _ => vec![],
    };

    // ---- inject, then unwrap the block back to raw UCFX ----
    let (out_block, stats) = match inject_static_into_donor_block(&block, &mesh, group, &repoints, name_hash, fit, flip, keep, all, &raw_targets, scale_mult) {
        Ok(v) => v,
        Err(e) => { eprintln!("inject_static: {e}"); return 1; }
    };
    let out_ucfx_len = rd_u32(&out_block, 16) as usize;
    let out_ucfx = &out_block[20..20 + out_ucfx_len];
    if let Err(e) = std::fs::write(&pos[2], out_ucfx) {
        eprintln!("write {}: {e}", pos[2]); return 1;
    }
    println!(
        "injected {} verts / {} tris into group {} (neutralised {} groups); bbox=[{:.2},{:.2},{:.2}]..[{:.2},{:.2},{:.2}]; MTRL repoints={:?} -> {} ({} UCFX bytes)",
        stats.vertex_count, stats.triangle_count, stats.target_group, stats.emptied_groups.len(),
        stats.bbox_min[0], stats.bbox_min[1], stats.bbox_min[2],
        stats.bbox_max[0], stats.bbox_max[1], stats.bbox_max[2],
        stats.mtrl_repoints, pos[2], out_ucfx.len()
    );
    0
}
