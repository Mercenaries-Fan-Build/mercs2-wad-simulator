//! `inject_character` — the faithful one-step character import: rigged `.glb` + a real NPC
//! donor block → an injected model block with **shipped-format skinning** (palette-relative
//! BLENDINDICES + `INFO(56)` range table + direction-aligned re-pose).
//!
//! This drives `mercs2_formats::char_skin` (the Rust port of Logan's `mercs2-mesher`, itself
//! byte-exact to the Python that produced two in-game-confirmed characters) and injects via
//! `inject_character_into_donor_block`. The donor supplies BOTH the injection target and the
//! target skeleton (its HIER = the 84/100-bone rig we re-pose onto).
//!
//! Usage:
//!   inject_character --glb <model.glb> --donor <block.bin> --group <ordinal> --out <out.bin>
//!                    [--name <hex>] [--repoint <from_hex>:<to_hex>]... [--container-verts <tsv>]
//!
//! Without `--container-verts` the model→container transform is ESTIMATED from bone
//! correspondences (good for preview). Supply a `dump_group_verts` TSV of the grafted donor
//! group for the EXACT, in-game-proven transform.

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::{build_character, validate, Mode, TargetSkeleton};
use mercs2_formats::model_inject::{inject_character_into_donor_block, ExternalMesh, MtrlRepoint};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

struct Args {
    glb: String,
    donor: String,
    groups: Vec<usize>,
    out: String,
    name: u32,
    repoints: Vec<MtrlRepoint>,
    container_verts: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut glb = None;
    let mut donor = None;
    let mut groups: Vec<usize> = Vec::new();
    let mut out = None;
    let mut name = 0u32;
    let mut repoints = Vec::new();
    let mut container_verts = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--glb" => glb = it.next(),
            "--donor" => donor = it.next(),
            "--group" => {
                let v = it.next().ok_or("--group needs a value")?;
                for part in v.split(',') {
                    groups.push(part.trim().parse().map_err(|_| "--group wants ordinals")?);
                }
            }
            "--out" => out = it.next(),
            "--name" => {
                let s = it.next().ok_or("--name needs a value")?;
                name = u32::from_str_radix(s.trim_start_matches("0x"), 16)
                    .map_err(|_| "--name must be hex")?;
            }
            "--repoint" => {
                let s = it.next().ok_or("--repoint needs from:to")?;
                let (f, t) = s.split_once(':').ok_or("--repoint wants from:to")?;
                repoints.push(MtrlRepoint {
                    from: u32::from_str_radix(f.trim_start_matches("0x"), 16).map_err(|_| "bad from")?,
                    to: u32::from_str_radix(t.trim_start_matches("0x"), 16).map_err(|_| "bad to")?,
                });
            }
            "--container-verts" => container_verts = it.next(),
            "-h" | "--help" => return Err("usage: inject_character --glb <m.glb> --donor <block.bin> --group <n[,n2,...]> --out <out.bin> [--name <hex>] [--repoint <hex>:<hex>] [--container-verts <tsv>]".into()),
            other => return Err(format!("unknown arg {other}")),
        }
    }
    Ok(Args {
        glb: glb.ok_or("--glb required")?,
        donor: donor.ok_or("--donor required")?,
        groups: {
            if groups.is_empty() {
                return Err("--group required".into());
            }
            groups
        },
        out: out.ok_or("--out required")?,
        name,
        repoints,
        container_verts,
    })
}

/// Parse a `dump_group_verts` TSV (`# ...`, header `idx\tx\ty\tz`, then rows) into positions.
fn parse_group_verts(text: &str) -> Result<Vec<[f64; 3]>, String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') || line.starts_with("idx") {
            continue;
        }
        let p: Vec<&str> = line.split('\t').collect();
        if p.len() < 4 {
            continue;
        }
        out.push([
            p[1].trim().parse().map_err(|_| "bad x")?,
            p[2].trim().parse().map_err(|_| "bad y")?,
            p[3].trim().parse().map_err(|_| "bad z")?,
        ]);
    }
    if out.is_empty() {
        return Err("no vertex rows in TSV".into());
    }
    Ok(out)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("inject_character: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let glb = gltf::load_char_glb(&args.glb)?;
    let donor_block = std::fs::read(&args.donor).map_err(|e| format!("read donor: {e}"))?;
    let skel = Skeleton::from_block(&donor_block)?;
    let target = TargetSkeleton::from_skeleton(&skel);
    println!(
        "donor skeleton: {} HIER bones, rest height {:.3} m",
        target.bones.len(),
        target.height
    );

    let container_verts: Option<Vec<[f64; 3]>> = match &args.container_verts {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("read TSV: {e}"))?;
            Some(parse_group_verts(&text)?)
        }
        None => None,
    };

    let inp = glb.build_input(&target, container_verts.as_deref(), HashMap::new(), false);
    let cs = build_character(&inp)?;
    println!(
        "retarget: {} verts, {} tris, palette {} slots over {} runs, mode {:?}",
        cs.stats.verts, cs.stats.tris, cs.palette_slots, cs.stats.range_count, cs.mode
    );
    if cs.mode == Mode::Estimated {
        println!("  NOTE: transform ESTIMATED (no container dump). Supply --container-verts for the proven path.");
    }
    for w in &cs.warnings {
        println!("  WARN: {w}");
    }

    // ---- validation battery ----
    let report = validate::validate(&cs, &glb.vjoints, &glb.vweights, &glb.indices);
    println!("checks:");
    for c in &report.checks {
        println!("  [{:?}] {}: {} ({})", c.status, c.title, c.text, c.reference);
    }
    for l in &report.limits {
        println!("  [{}] {}: {}", if l.ok { "ok" } else { "FAIL" }, l.title, l.text);
    }
    println!("overall: {:?}", report.worst);

    // ---- inject ----
    let mesh = ExternalMesh {
        positions: cs.pos.clone(),
        // CONFORMED normals, not the source glTF's. Conforming re-poses the geometry, so the
        // source field stops describing the surface (measured: mean dot -0.01 against it).
        normals: if cs.nrm.is_empty() { glb.normals.clone() } else { cs.nrm.clone() },
        uvs: glb.uvs.clone(),
        tris: glb.tris.clone(),
        joints: (0..cs.stats.verts)
            .map(|i| {
                [
                    cs.skin_bytes[i * 8],
                    cs.skin_bytes[i * 8 + 1],
                    cs.skin_bytes[i * 8 + 2],
                    cs.skin_bytes[i * 8 + 3],
                ]
            })
            .collect(),
        weights: (0..cs.stats.verts)
            .map(|i| {
                [
                    cs.skin_bytes[i * 8 + 4],
                    cs.skin_bytes[i * 8 + 5],
                    cs.skin_bytes[i * 8 + 6],
                    cs.skin_bytes[i * 8 + 7],
                ]
            })
            .collect(),
    };
    let (block, stats) = if args.groups.len() == 1 {
        inject_character_into_donor_block(
            &donor_block,
            &mesh,
            &cs.ranges,
            args.groups[0],
            &args.repoints,
            args.name,
        )?
    } else {
        // MULTI-GROUP. Each host group gets its OWN palette + INFO(56) table, computed inside
        // the injector from the bones that group actually uses — so `mesh.joints` must carry
        // GLOBAL donor HIER indices here, not the whole-model palette slots the single-group
        // path wants. Expand the slots back through the model palette to recover them.
        let palette = mercs2_formats::char_skin::expand_ranges(&cs.ranges);
        let mut gmesh = mesh;
        for (vi, j) in gmesh.joints.iter_mut().enumerate() {
            for k in 0..4 {
                let slot = cs.skin_bytes[vi * 8 + k] as usize;
                *j.get_mut(k).unwrap() = palette.get(slot).copied().unwrap_or(0) as u8;
            }
        }
        let (b, _audits, s) = mercs2_formats::model_inject::inject_character_multi_into_donor_block(
            &donor_block,
            &gmesh,
            &args.groups,
            &args.repoints,
            args.name,
            true, // grow: the import is denser than the donor; packager recomputes page_count
            // No explicit triangle->group map: this path splits evenly by triangle order. The
            // faithful sub-object partition lives in xfer_apply and has not been promoted yet.
            None,
        )?;
        (b, s)
    };
    std::fs::write(&args.out, &block).map_err(|e| format!("write {}: {e}", args.out))?;
    println!(
        "wrote {} ({} bytes): group {} <- {} verts / {} strip idx, emptied {:?}",
        args.out,
        block.len(),
        stats.target_group,
        stats.vertex_count,
        stats.strip_len,
        stats.emptied_groups
    );
    Ok(())
}
