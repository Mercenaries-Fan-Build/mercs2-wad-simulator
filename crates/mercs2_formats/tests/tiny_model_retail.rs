//! Retail gate for [`mercs2_formats::tiny_model`]: every TINY model container in `vz.wad` decodes
//! and encodes back to its own bytes, and carries the conventions the module's constants and
//! helpers state.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::tiny_model::{self as tiny, is_tiny_container, TinyModel};
use mercs2_formats::types::TYPE_HASH_MODEL;
use mercs2_formats::ucfx::walk_decompressed_block;

/// Every TINY container of the retail `vz.wad`: (block, name hash, container bytes).
fn retail_tiny_containers() -> Vec<(usize, u32, Vec<u8>)> {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut f = std::fs::File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS tables");
    let mut out = Vec::new();
    for block in 0..archive.indx.len() {
        let dec = decompress_block(&mut f, &archive.indx, block as u16).expect("decompress block");
        let (parsed, _) = walk_decompressed_block(&dec, "block");
        for (entry, c) in parsed.entries.iter().zip(parsed.containers.iter()) {
            if entry.type_hash == TYPE_HASH_MODEL && is_tiny_container(c) {
                out.push((block, entry.name_hash, c.to_vec()));
            }
        }
    }
    out
}

fn f16(h: u16) -> f32 {
    mercs2_formats::model_inject::read_f16_le(&h.to_le_bytes(), 0)
}

/// One f16 step at `x`: the gap between adjacent half floats of `x`'s magnitude.
fn f16_step(x: f32) -> f32 {
    let e = x.abs().max(f32::MIN_POSITIVE).log2().floor().max(-14.0);
    2f32.powf(e - 10.0)
}

#[test]
fn every_retail_tiny_container_round_trips_and_follows_the_conventions() {
    let all = retail_tiny_containers();
    let pristine = pandemic_hash_m2(tiny::NODE_PRISTINE);
    let ruin = pandemic_hash_m2(tiny::NODE_RUIN);
    let pixel = pandemic_hash_m2(tiny::MATERIAL_PIXEL_SHADER);
    let trailing = pandemic_hash_m2(tiny::MATERIAL_TRAILING);
    let intact_vs = pandemic_hash_m2("PgMeshTinyVP");
    let ruin_vs = pandemic_hash_m2("PgMeshTinyVP_Ruin");

    let mut failures = Vec::new();
    for (block, hash, c) in &all {
        let at = format!("block {block} container {hash:#010X}");
        let m = match TinyModel::decode(c) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{at}: {e}"));
                continue;
            }
        };
        if &m.encode() != c {
            failures.push(format!("{at}: encodes to different bytes"));
            continue;
        }

        let mut bad = |what: &str| failures.push(format!("{at}: {what}"));
        if m.info.flags != tiny::INFO_FLAGS || m.info.word_1c != tiny::INFO_WORD_1C || m.info.tail != tiny::INFO_TAIL {
            bad("INFO constants");
        }
        if m.slots.windows(2).any(|w| w[0] >= w[1]) {
            bad("slot list not strictly ascending");
        }
        let n = m.sub_objects.len();
        if m.nodes.len() != n {
            bad("node count differs from the sub-object count");
        }
        let segments: Vec<tiny::SegmRecord> = (0..n)
            .map(|k| tiny::SegmRecord { bone: k as u16, segment: k as u8, state_mask: 1 })
            .collect();
        if m.segments != segments {
            bad("SEGM is not {k, k, 1} per sub-object");
        }
        for (k, node) in m.nodes.iter().enumerate() {
            let want = tiny::hier_node(
                node.name_hash,
                k,
                n,
                [node.bbox_min[0], node.bbox_min[1], node.bbox_min[2]],
                [node.bbox_max[0], node.bbox_max[1], node.bbox_max[2]],
            );
            if format!("{node:?}") != format!("{want:?}") {
                bad("HIER node is not hier_node(..)");
            }
        }
        for mat in &m.materials {
            if mat.textures.len() != 1
                || mat.pixel_shader != pixel
                || mat.trailing != trailing
                || mat.preamble != tiny::MATERIAL_PREAMBLE
                || (mat.flags != tiny::MATERIAL_OPAQUE && mat.flags != tiny::MATERIAL_ALPHATEST)
            {
                bad("material is not one texture, PgDiffFP, m2(ANY), the TINY preamble, opaque or alphatest");
            }
        }
        let roles: Vec<u32> = m.sub_objects.iter().map(|s| m.nodes[s.node as usize].name_hash).collect();
        if roles != [pristine, ruin] && roles != [pristine] && roles != [ruin] {
            bad("sub-objects are not [pristine, ruin], [pristine] or [ruin]");
            continue;
        }
        for (k, s) in m.sub_objects.iter().enumerate() {
            if s.node as usize != k {
                bad("INDX entry is not the sub-object's ordinal");
            }
            let want_vs = if roles[k] == pristine { intact_vs } else { ruin_vs };
            for g in &s.groups {
                if g.header != tiny::GROUP_HEADER || g.stream_flag != tiny::STREAM_FLAG || g.vertex_shader != want_vs {
                    bad("group header / stream flag / vertex shader");
                }
                let mat = g.prims[0].material as usize;
                let Some(material) = m.materials.get(mat) else {
                    bad("PRMT material out of range");
                    continue;
                };
                let alpha = material.flags & 0x0B != 0;
                let want_shadow = match (roles[k] == pristine, alpha) {
                    (true, false) => "PgMeshTinyShadowVP",
                    (true, true) => "PgMeshTinyShadowTexVP",
                    (false, false) => "PgMeshTinyShadowVP_Ruin",
                    (false, true) => "PgMeshTinyShadowTexVP_Ruin",
                };
                if g.shadow_vertex_shader != pandemic_hash_m2(want_shadow) {
                    bad("shadow vertex shader is not the one the role and the material's alpha flag pick");
                }
                if g.prims != tiny::group_prims(mat as u32, g.strip.len(), g.vertices.len()) {
                    bad("PRMT is not group_prims(..)");
                }
                for t in g.strip.windows(3) {
                    if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                        continue;
                    }
                    let w: Vec<f32> = t.iter().map(|&i| f16(g.vertices[i as usize].position[3])).collect();
                    if w[0] != w[1] || w[1] != w[2] {
                        bad("a triangle spans two slots");
                        break;
                    }
                }
                if g.vertices.iter().any(|v| {
                    let w = f16(v.position[3]);
                    w.fract() != 0.0 || w < 0.0 || w as usize >= m.slots.len()
                }) {
                    bad("a vertex slot is not a slot of the list");
                }
                let mid: [f32; 3] = std::array::from_fn(|a| (g.bbox_min[a] + g.bbox_max[a]) * 0.5);
                if mid != g.center {
                    bad("group centre is not the midpoint of its bounds");
                }
                // The bounds come from the positions before they were stored as half floats: each
                // lies within one f16 step of the stored vertices' bounds.
                for a in 0..3 {
                    let lo = g.vertices.iter().map(|v| f16(v.position[a])).fold(f32::INFINITY, f32::min);
                    let hi = g.vertices.iter().map(|v| f16(v.position[a])).fold(f32::NEG_INFINITY, f32::max);
                    if (lo - g.bbox_min[a]).abs() > f16_step(lo) || (hi - g.bbox_max[a]).abs() > f16_step(hi) {
                        bad("group bounds are more than one f16 step from the vertices' bounds");
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    assert_eq!(all.len(), 1208);
}
