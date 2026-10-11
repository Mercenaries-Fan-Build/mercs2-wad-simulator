//! Build the PROCEDURAL MANNEQUIN model block: a clean primitive humanoid fitted
//! to mattias_v2's REAL 95-bone skeleton, auto-weighted to the owning bone, then
//! injected into the mattias_v2 donor via the shared `model_inject` path.
//!
//! This is a guaranteed-valid, fully-owned control mesh (no foreign import). It
//! reuses M2's weight-carrying injection (`inject_multi_into_donor_block`) and
//! the reusable `skeleton` + `mannequin` modules. Model after `inject_cesium`.
//!
//! Usage: build_mannequin <donor.block.bin> <out_model.bin>
//!
//! DO NOT deploy — build + verify only.

use mercs2_formats::ffcs::read_u32_le;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::mannequin::{build_mannequin, BodyMap};
use mercs2_formats::model_inject::{
    assert_no_empty_drawing_group, inject_multi_into_donor_block, read_f16_le, ExternalMesh,
    MtrlRepoint,
};
use mercs2_formats::skeleton::Skeleton;

// mattias_v2 measured model-space bind height.
const DONOR_HEIGHT: f32 = 1.847;

// mattias_v2 host drawing groups (stride-40 / DECL64, skinned). The mannequin is
// split across these two via the multi-group splitter, same as M2/cesium.
const TARGET_GROUPS: [usize; 2] = [2, 6];

// Reuse the already-resident cesium_skin diffuse (single texture).
const CESIUM_SKIN: u32 = 0xdd4d410d;
// mattias_v2 donor diffuse hashes (grp2 + grp6 materials) -> cesium_skin.
const MATTIAS_DIFFUSE: [u32; 2] = [0xf66b8f19, 0x63c031b5];

// Named bone hashes resolved via the rainbow table (pandemic_hash_m2).
const H_HIPS: u32 = 0x24C5009C;
const H_CHEST: u32 = 0x4C7733ED;
const H_HEAD: u32 = 0x705C4508;
const H_LBICEP: u32 = 0xB2C9CE63;
const H_RBICEP: u32 = 0x20F635D9;
const H_LFOREARM: u32 = 0xBEFC09A2;
const H_RFOREARM: u32 = 0x23F6F598;
const H_LTHIGH: u32 = 0x76853D12;
const H_RTHIGH: u32 = 0xC2299AC4;
const H_LSHIN: u32 = 0xA76C9842;
const H_RSHIN: u32 = 0x0163705C;

/// First child of `bone` (by parent index), or None.
fn first_child(skel: &Skeleton, bone: usize) -> Option<usize> {
    skel.bones.iter().position(|b| b.parent == bone as i32)
}

/// The forearm's child node furthest (in world distance) from the elbow — the
/// wrist/hand attach. Falls back to first child.
fn wrist_of(skel: &Skeleton, forearm: usize) -> usize {
    let elbow = skel.bones[forearm].world_pos();
    let mut best: Option<(usize, f32)> = None;
    for b in &skel.bones {
        if b.parent == forearm as i32 {
            let p = b.world_pos();
            let d = (p[0] - elbow[0]).powi(2) + (p[1] - elbow[1]).powi(2) + (p[2] - elbow[2]).powi(2);
            if best.map_or(true, |(_, bd)| d > bd) {
                best = Some((b.index, d));
            }
        }
    }
    best.map(|(i, _)| i).unwrap_or(forearm)
}

fn resolve(skel: &Skeleton, h: u32, label: &str) -> usize {
    skel.by_hash(h)
        .unwrap_or_else(|| panic!("bone {label} ({h:#010x}) not found in skeleton"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: build_mannequin <donor.block.bin> <out_model.bin>");
        std::process::exit(2);
    }
    // Standalone gate mode: `build_mannequin --gate <model_block.bin>` runs ONLY
    // the empty-drawing-group build gate against an existing model block and
    // reports pass/fail (used to prove the gate flags the broken v1 model).
    if args[1] == "--gate" {
        let path = &args[2];
        let block = std::fs::read(path).expect("read model block");
        match assert_no_empty_drawing_group(&block) {
            Ok(()) => {
                eprintln!("GATE PASS: {path} has no empty-but-drawing group");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("GATE FAIL: {path}: {e}");
                std::process::exit(1);
            }
        }
    }

    let donor_path = &args[1];
    let out_path = &args[2];

    let donor_block = std::fs::read(donor_path).expect("read donor");

    // ---- 1. SKELETON EXTRACTION ----
    let skel = Skeleton::from_block(&donor_block).expect("extract skeleton");
    let n = skel.bones.len();
    let h = skel.height();
    eprintln!("=== skeleton: {n} bones, bind height {h:.4} (donor target {DONOR_HEIGHT}) ===");
    assert!((1.7..1.95).contains(&h), "skeleton height {h} not humanoid ~1.847");

    // named bones
    let pelvis = resolve(&skel, H_HIPS, "pelvis");
    let chest = resolve(&skel, H_CHEST, "chest");
    let head = resolve(&skel, H_HEAD, "head");
    let upperarm_l = resolve(&skel, H_LBICEP, "upperarm_l");
    let upperarm_r = resolve(&skel, H_RBICEP, "upperarm_r");
    let forearm_l = resolve(&skel, H_LFOREARM, "forearm_l");
    let forearm_r = resolve(&skel, H_RFOREARM, "forearm_r");
    let thigh_l = resolve(&skel, H_LTHIGH, "thigh_l");
    let thigh_r = resolve(&skel, H_RTHIGH, "thigh_r");
    let shin_l = resolve(&skel, H_LSHIN, "shin_l");
    let shin_r = resolve(&skel, H_RSHIN, "shin_r");

    // hierarchy-derived bones (no hard-coded indices)
    let neck = skel.bones[head].parent as usize; // parent of head
    let clav_l = skel.bones[upperarm_l].parent as usize; // parent of bicep
    let clav_r = skel.bones[upperarm_r].parent as usize;
    let hand_l = wrist_of(&skel, forearm_l);
    let hand_r = wrist_of(&skel, forearm_r);
    let foot_l = first_child(&skel, shin_l).expect("left foot (shin child)");
    let foot_r = first_child(&skel, shin_r).expect("right foot (shin child)");

    let map = BodyMap {
        pelvis, chest, neck, head,
        clav_l, clav_r,
        upperarm_l, upperarm_r,
        forearm_l, forearm_r,
        hand_l, hand_r,
        thigh_l, thigh_r,
        shin_l, shin_r,
        foot_l, foot_r,
    };

    // sanity print of key bone positions
    let pp = |i: usize| {
        let p = skel.bones[i].world_pos();
        format!("[{:.3},{:.3},{:.3}]", p[0], p[1], p[2])
    };
    eprintln!("  pelvis  idx{pelvis:>2} {}", pp(pelvis));
    eprintln!("  chest   idx{chest:>2} {}", pp(chest));
    eprintln!("  neck    idx{neck:>2} {}", pp(neck));
    eprintln!("  head    idx{head:>2} {}", pp(head));
    eprintln!("  clav  L idx{clav_l:>2} {}  R idx{clav_r:>2} {}", pp(clav_l), pp(clav_r));
    eprintln!("  uarm  L idx{upperarm_l:>2} {}  R idx{upperarm_r:>2} {}", pp(upperarm_l), pp(upperarm_r));
    eprintln!("  farm  L idx{forearm_l:>2} {}  R idx{forearm_r:>2} {}", pp(forearm_l), pp(forearm_r));
    eprintln!("  hand  L idx{hand_l:>2} {}  R idx{hand_r:>2} {}", pp(hand_l), pp(hand_r));
    eprintln!("  thigh L idx{thigh_l:>2} {}  R idx{thigh_r:>2} {}", pp(thigh_l), pp(thigh_r));
    eprintln!("  shin  L idx{shin_l:>2} {}  R idx{shin_r:>2} {}", pp(shin_l), pp(shin_r));
    eprintln!("  foot  L idx{foot_l:>2} {}  R idx{foot_r:>2} {}", pp(foot_l), pp(foot_r));

    // symmetry check (L/R x near-opposite)
    let lr = |l: usize, r: usize| {
        let a = skel.bones[l].world_pos();
        let b = skel.bones[r].world_pos();
        (a[0] + b[0]).abs()
    };
    eprintln!(
        "  symmetry |xL+xR|: uarm={:.3} farm={:.3} thigh={:.3} shin={:.3} (want ~0)",
        lr(upperarm_l, upperarm_r), lr(forearm_l, forearm_r), lr(thigh_l, thigh_r), lr(shin_l, shin_r)
    );

    // ---- 2 + 3. GEOMETRY + AUTO-WEIGHT (nearest-2 blend on limbs) ----
    let (mesh, parts) = build_mannequin(&skel, &map, DONOR_HEIGHT, true);
    eprintln!("=== procedural geometry (per part: vcount / tris) ===");
    let mut tv = 0usize;
    let mut tt = 0usize;
    for (name, vc, tc) in &parts.parts {
        eprintln!("  {name:<8} v{vc:>4} t{tc:>4}");
        tv += vc;
        tt += tc;
    }
    eprintln!("  TOTAL    v{} t{} (mesh verts={}, tris={})", tv, tt, mesh.positions.len(), mesh.tris.len());
    assert!(mesh.positions.len() <= 3500, "mannequin too dense for grp2+grp6");

    // weight-distribution report
    {
        use std::collections::HashMap as HM;
        let mut hist: HM<[u8; 4], usize> = HM::new();
        for j in &mesh.joints {
            *hist.entry(*j).or_insert(0) += 1;
        }
        eprintln!("  distinct BLENDINDICES tuples: {}", hist.len());
        let mut top: Vec<_> = hist.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        for (k, c) in top.iter().take(6) {
            eprintln!("    idx {:?} x{}", k, c);
        }
        for &vi in &[0usize, mesh.positions.len() / 2, mesh.positions.len() - 1] {
            let ws: u32 = mesh.weights[vi].iter().map(|&w| w as u32).sum();
            eprintln!("    sample v{vi}: idx={:?} wgt={:?} (sum={ws})", mesh.joints[vi], mesh.weights[vi]);
        }
    }

    // ---- 4. INJECT via the shared M2 path ----
    let pmc_hum_cesium = pandemic_hash_m2("pmc_hum_cesium"); // reuse cesium slot/name
    let repoints: Vec<MtrlRepoint> = MATTIAS_DIFFUSE
        .iter()
        .map(|&from| MtrlRepoint { from, to: CESIUM_SKIN })
        .collect();
    let (block, audits, stats) =
        inject_multi_into_donor_block(&donor_block, &mesh, &TARGET_GROUPS, &repoints, pmc_hum_cesium)
            .expect("inject multi");

    std::fs::write(out_path, &block).expect("write model block");

    eprintln!("=== injection ===");
    eprintln!("  block name hash pmc_hum_cesium={pmc_hum_cesium:#010x}");
    eprintln!("  total verts={} tris={}", stats.vertex_count, stats.triangle_count);
    eprintln!("  === PER-GROUP BUDGET (injected vs donor original) ===");
    for a in &audits {
        let vok = a.injected_vc <= a.donor_vc;
        let iok = a.injected_ic <= a.donor_ic;
        eprintln!(
            "    grp{:>2}: vc {:>5}/{:<5} ({})  ic {:>6}/{:<6} ({})  tris={}",
            a.group, a.injected_vc, a.donor_vc, if vok { "OK" } else { "FAIL" },
            a.injected_ic, a.donor_ic, if iok { "OK" } else { "FAIL" }, a.triangles
        );
        assert!(vok && iok, "group {} budget violated", a.group);
    }
    eprintln!("  emptied groups: {:?}", stats.emptied_groups);
    for (f, t, c) in &stats.mtrl_repoints {
        eprintln!("  MTRL repoint {f:#010x} -> {t:#010x} x{c}");
    }
    eprintln!(
        "  bbox min={:?} max={:?}",
        stats.bbox_min.map(|v| (v * 1000.0).round() / 1000.0),
        stats.bbox_max.map(|v| (v * 1000.0).round() / 1000.0)
    );
    eprintln!("  avg normal len={:.4} tangent len={:.4}", stats.avg_normal_len, stats.avg_tangent_len);
    eprintln!("  wrote {out_path}: {} bytes", block.len());

    verify_emitted(&block);
}

/// Re-parse emitted block: confirm every INJECTED drawing group carries varied
/// (non-bone-0) BLENDINDICES and stride-40 layout (sanity).
fn verify_emitted(block: &[u8]) {
    // BUILD GATE: no registered drawing group may have a zero-size vbuf/ibuf.
    assert_no_empty_drawing_group(block).expect("empty-drawing-group build gate");
    let _ = ExternalMesh::default();
    let ulen = read_u32_le(block, 16) as usize;
    let ucfx = &block[20..20 + ulen];
    let data_off = read_u32_le(ucfx, 4) as usize;
    let ndesc = read_u32_le(ucfx, 16) as usize;

    let prmg: Vec<usize> = (0..ndesc)
        .filter(|&i| {
            let ro = 20 + i * 20;
            &ucfx[ro..ro + 4] == b"PRMG" && read_u32_le(ucfx, ro + 4) == 0xFFFF_FFFF
        })
        .collect();
    let leaf_at = |i: usize| -> (usize, usize) {
        let ro = 20 + i * 20;
        (data_off + read_u32_le(ucfx, ro + 4) as usize, read_u32_le(ucfx, ro + 8) as usize)
    };

    for (gi, &pr) in prmg.iter().enumerate() {
        let nxt = if gi + 1 < prmg.len() { prmg[gi + 1] } else { ndesc };
        let mut state = 0u8;
        let (mut strm_data, mut strm_stride, mut strm_n) = (None, 0usize, 0usize);
        let mut prmt: Option<usize> = None;
        for i in (pr + 1)..nxt {
            let ro = 20 + i * 20;
            let tag = &ucfx[ro..ro + 4];
            let cm = read_u32_le(ucfx, ro + 4) == 0xFFFF_FFFF;
            if tag == b"STRM" && cm {
                state = 1;
            } else if cm {
                state = 0;
            } else if state == 1 && tag == b"info" {
                let (o, _) = leaf_at(i);
                strm_stride = read_u32_le(ucfx, o + 4) as usize;
                strm_n = read_u32_le(ucfx, o + 8) as usize;
            } else if state == 1 && tag == b"data" {
                strm_data = Some(leaf_at(i));
            } else if tag == b"PRMT" && !cm {
                prmt = Some(i);
            }
        }
        let draws = prmt.map_or(false, |p| {
            let (o, sz) = leaf_at(p);
            (0..sz / 16).any(|r| read_u32_le(ucfx, o + r * 16 + 8) > 0)
        });
        if !draws || strm_stride != 40 {
            continue;
        }
        let Some((s, dsz)) = strm_data else { continue };
        let cnt = strm_n.min(dsz / 40);
        if cnt == 0 {
            continue;
        }
        use std::collections::HashSet;
        let mut distinct: HashSet<[u8; 4]> = HashSet::new();
        let mut all_bone0 = true;
        for v in 0..cnt {
            let o = s + v * 40;
            let bi = [ucfx[o + 16], ucfx[o + 17], ucfx[o + 18], ucfx[o + 19]];
            distinct.insert(bi);
            if bi != [0, 0, 0, 0] {
                all_bone0 = false;
            }
        }
        let pos_w = u16::from_le_bytes([ucfx[s + 6], ucfx[s + 7]]);
        let bi0 = [ucfx[s + 16], ucfx[s + 17], ucfx[s + 18], ucfx[s + 19]];
        let bw0 = [ucfx[s + 20], ucfx[s + 21], ucfx[s + 22], ucfx[s + 23]];
        let _ = read_f16_le(ucfx, s); // touch
        eprintln!(
            "  INJECTED grp(prmg#{gi}) {cnt} verts: POS.w={pos_w:#06x} vtx0 BLENDIDX={bi0:02x?} BLENDWGT={bw0:02x?} distinct_idx={} all_bone0={all_bone0}",
            distinct.len()
        );
        assert_eq!(pos_w, 0x3c00, "POS.w must be 1.0");
        assert!(
            !all_bone0 && distinct.len() > 1,
            "injected group must carry varied BLENDINDICES (all_bone0={all_bone0}, distinct={})",
            distinct.len()
        );
    }
}
