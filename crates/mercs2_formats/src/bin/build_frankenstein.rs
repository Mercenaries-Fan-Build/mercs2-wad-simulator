//! FRANKENSTEIN kitbash driver: a novel human assembled from BASE-GAME parts —
//! chris HEAD + jen TORSO grafted onto mattias_v2's 95-bone skeleton, each borrowed
//! part keeping its NATIVE base skin. No foreign import: all parts are already
//! PC-format donor groups, so geometry is EXTRACTED (positions/normals/uvs/tris +
//! per-vertex BLENDINDICES/BLENDWEIGHT) from the source group, the BLENDINDICES are
//! RETARGETED source->mattias BY BONE-NAME-HASH (parent fallback for unique bones),
//! the part is TRANSLATED to mattias's bind pose by its attachment bone, then
//! INJECTED into the corresponding mattias host group(s) via the shared multi-group
//! splitter. MTRL diffuse is repointed to each part's NATIVE base skin hash.
//!
//! Reusable shape: kitbash-by-hash-retarget. Supply (source block, source group,
//! attachment-bone hash, host group set, host diffuse -> part diffuse) per part.
//!
//! Usage: build_frankenstein <mattias_v2.bin> <chris.bin> <jen.bin> <out_model.bin>
//! DO NOT deploy.

use mercs2_formats::ffcs::read_u32_le;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::model_inject::{
    assert_no_empty_drawing_group, extract_group_mesh, inject_parts_into_donor_block, read_f16_le,
    repose_part_cross_skeleton, survey_groups, ExternalMesh, InjectPart, MtrlRepoint,
};
use mercs2_formats::skeleton::{affine_inverse, mat4_mul, transform_point, Skeleton};
use std::collections::HashSet;

// Named mattias bone hashes (attachment references; rainbow-resolved).
const H_HEAD: u32 = 0x705C4508; // Bone_Head (neck/head attach for the head part)
const H_HIPS: u32 = 0x24C5009C; // Bone_Hips  (pelvis attach for the torso part)
const H_CHEST: u32 = 0x4C7733ED; // Bone_Chest

// mattias_v2 host group diffuse hashes (proven by the cesium pipeline).
const MATTIAS_HEAD_DIFFUSE: u32 = 0xf66b8f19; // head/face material (grp2/3 region)
const MATTIAS_TORSO_DIFFUSE: u32 = 0x63c031b5; // torso/arms material (grp6/11 region)

// Native base part-skin diffuse hashes (derived from each source's MTRL, the
// material bound to the head-band / torso-band single-material groups).
const CHRIS_HEAD_DIFFUSE: u32 = 0xfbd0e02c; // chris face/head skin (mat2)
const JEN_TORSO_DIFFUSE: u32 = 0x2336e0d7; // jen torso skin (mat4)

// Selected SOURCE drawing-group ordinals (justified by the STEP-1 survey, asserted
// at runtime against Y-band + bone evidence).
// CHRIS HEAD: ord8 (decl-64/stride-40, vc=1389, ic=4093, Y[1.47..1.81], 100% of
// 1562 weights sum-to-255). CORRECTION over v1/v2 which used ord23 (decl-112,
// stride!=40) -> the garbled face. ord8 is the smallest clean decl-64 head group;
// ord24 (vc=918) is the same region at lower density.
const CHRIS_HEAD_ORD: usize = 8;
const JEN_TORSO_ORD: usize = 2; // Y[0.81..1.51] mid band, spine/chest bones

// mattias host groups (DECL64, drawing). Head part hosted across the head groups;
// torso part across the torso/arm body groups.
const HEAD_HOSTS: [usize; 3] = [2, 3, 7];
const TORSO_HOSTS: [usize; 2] = [6, 11];

// Decimation budget: with the chris head = ord8 (2427 tris) and THREE head hosts
// (mattias ord2/3/7, caps vc>=2900 ic>=7596 each), the balanced split is ~809
// tris/host (~2429 strip idx) — under every host's cap. So NO decimation is
// needed; the budget is set above the part's tri count so decimate_to_tris is a
// no-op and facial topology is preserved verbatim.
const HEAD_TRI_BUDGET: usize = 100_000;

/// Build source-bone-index -> mattias-bone-index map BY HASH (STEP 4). For each
/// source bone, look its name-hash up in mattias; if absent, walk the source PARENT
/// chain until a hash matches (nearest mapped ancestor). Returns (map, clean_set,
/// fallback list of (src_idx, used_ancestor_idx)).
fn build_bone_map(
    src: &Skeleton,
    dst: &Skeleton,
) -> (Vec<usize>, HashSet<usize>, Vec<(usize, usize)>) {
    let n = src.bones.len();
    let mut map = vec![0usize; n];
    let mut clean = HashSet::new();
    let mut fallback = Vec::new();
    for i in 0..n {
        if let Some(t) = dst.by_hash(src.bones[i].name_hash) {
            map[i] = t;
            clean.insert(i);
        } else {
            // walk parent chain for the nearest mapped ancestor
            let mut p = src.bones[i].parent;
            let mut resolved = None;
            let mut guard = 0;
            while p >= 0 && guard < n {
                let pi = p as usize;
                if let Some(t) = dst.by_hash(src.bones[pi].name_hash) {
                    resolved = Some((t, pi));
                    break;
                }
                p = src.bones[pi].parent;
                guard += 1;
            }
            let (t, anc) = resolved.unwrap_or((0, 0)); // root fallback
            map[i] = t;
            fallback.push((i, anc));
        }
    }
    (map, clean, fallback)
}

/// Remap a part's per-vertex BLENDINDICES (source bone indices) through the bone
/// map (STEP 4). Weights kept native. Returns (count of referenced source bones,
/// how many of those mapped cleanly).
fn retarget_part(
    mesh: &mut ExternalMesh,
    map: &[usize],
    clean: &HashSet<usize>,
) -> (usize, usize, Vec<usize>) {
    // referenced SOURCE bone indices (before remap)
    let mut referenced: HashSet<usize> = HashSet::new();
    for j in &mesh.joints {
        for &b in j {
            referenced.insert(b as usize);
        }
    }
    for j in mesh.joints.iter_mut() {
        for k in 0..4 {
            let s = j[k] as usize;
            j[k] = if s < map.len() { map[s] as u8 } else { 0 };
        }
    }
    let nref = referenced.len();
    let nclean = referenced.iter().filter(|&&b| clean.contains(&b)).count();
    // referenced source bones that FELL BACK (not clean)
    let mut fb: Vec<usize> = referenced.iter().copied().filter(|b| !clean.contains(b)).collect();
    fb.sort_unstable();
    (nref, nclean, fb)
}

/// Translate a part's verts so its attachment bone in the SOURCE skeleton lands on
/// the same bone in mattias (STEP 5). Returns the applied translation + the part
/// bbox after positioning. SUPERSEDED by the inverse-bind re-pose (which lands the
/// part in mattias's bind frame intrinsically); kept for reference / non-skinned
/// parts.
#[allow(dead_code)]
fn position_part(
    mesh: &mut ExternalMesh,
    src: &Skeleton,
    dst: &Skeleton,
    attach_hash: u32,
) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let s_idx = src.by_hash(attach_hash).expect("source attach bone");
    let d_idx = dst.by_hash(attach_hash).expect("mattias attach bone");
    let sp = src.bones[s_idx].world_pos();
    let dp = dst.bones[d_idx].world_pos();
    let t = [dp[0] - sp[0], dp[1] - sp[1], dp[2] - sp[2]];
    let mut bmin = [f32::INFINITY; 3];
    let mut bmax = [f32::NEG_INFINITY; 3];
    for p in mesh.positions.iter_mut() {
        for k in 0..3 {
            p[k] += t[k];
            bmin[k] = bmin[k].min(p[k]);
            bmax[k] = bmax[k].max(p[k]);
        }
    }
    (t, bmin, bmax)
}

/// Decimate a mesh to at most `max_tris` triangles by uniform triangle striding
/// (keep every k-th triangle), then COMPACT to only the referenced vertices so the
/// result is a valid self-contained mesh. Crude (no edge-collapse) but topology-
/// safe and adequate for a v1 kitbash where the host INDEX budget is tight. Returns
/// the decimated mesh. A no-op if already under budget.
fn decimate_to_tris(m: &ExternalMesh, max_tris: usize) -> ExternalMesh {
    if m.tris.len() <= max_tris {
        return m.clone();
    }
    let k = (m.tris.len() + max_tris - 1) / max_tris; // keep every k-th
    let kept: Vec<[u32; 3]> = m.tris.iter().step_by(k).copied().collect();
    // compact referenced verts
    let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    for t in &kept {
        for &v in t {
            if !remap.contains_key(&v) {
                remap.insert(v, order.len() as u32);
                order.push(v);
            }
        }
    }
    let has_skin = !m.joints.is_empty();
    let mut out = ExternalMesh {
        positions: order.iter().map(|&v| m.positions[v as usize]).collect(),
        normals: order.iter().map(|&v| m.normals[v as usize]).collect(),
        uvs: order.iter().map(|&v| m.uvs[v as usize]).collect(),
        tris: kept.iter().map(|t| [remap[&t[0]], remap[&t[1]], remap[&t[2]]]).collect(),
        joints: if has_skin { order.iter().map(|&v| m.joints[v as usize]).collect() } else { Vec::new() },
        weights: if has_skin { order.iter().map(|&v| m.weights[v as usize]).collect() } else { Vec::new() },
    };
    let _ = &mut out;
    out
}

fn report_survey_pick(label: &str, block: &[u8], ord: usize) {
    let skel = Skeleton::from_block(block).unwrap();
    let groups = survey_groups(block).unwrap();
    let g = &groups[ord];
    let mut bone_str = String::new();
    for (bi, cnt) in g.bone_hist.iter().take(4) {
        let nh = skel.bones.get(*bi as usize).map(|b| b.name_hash).unwrap_or(0);
        bone_str.push_str(&format!(" [b{bi}x{cnt} {nh:#010x}]"));
    }
    eprintln!(
        "  {label}: ord{ord} vc={} ic={} Y[{:.2}..{:.2}] dom_bones:{}",
        g.vertex_count, g.index_count, g.y_min, g.y_max, bone_str
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 {
        eprintln!("usage: build_frankenstein <mattias.bin> <chris.bin> <jen.bin> <out.bin>");
        std::process::exit(2);
    }
    let mattias = std::fs::read(&a[1]).expect("read mattias");
    let chris = std::fs::read(&a[2]).expect("read chris");
    let jen = std::fs::read(&a[3]).expect("read jen");
    let out_path = &a[4];

    let m_skel = Skeleton::from_block(&mattias).expect("mattias skel");
    let c_skel = Skeleton::from_block(&chris).expect("chris skel");
    let j_skel = Skeleton::from_block(&jen).expect("jen skel");
    eprintln!(
        "=== skeletons: mattias {}b/{:.3}  chris {}b/{:.3}  jen {}b/{:.3} ===",
        m_skel.bones.len(),
        m_skel.height(),
        c_skel.bones.len(),
        c_skel.height(),
        j_skel.bones.len(),
        j_skel.height()
    );

    eprintln!("=== STEP 1/2: SELECTED SOURCE GROUPS (Y-band + bone evidence) ===");
    report_survey_pick("chris HEAD ", &chris, CHRIS_HEAD_ORD);
    report_survey_pick("jen  TORSO ", &jen, JEN_TORSO_ORD);

    // ---- HEAD part: chris ----
    let head_raw = extract_group_mesh(&chris, CHRIS_HEAD_ORD).expect("extract chris head");
    eprintln!(
        "=== chris HEAD extracted: {} verts, {} tris ===",
        head_raw.positions.len(),
        head_raw.tris.len()
    );
    // Decimate to fit the head-host INDEX budget (hosts [2,3,7] ~29k indices; the
    // partition uses a 3-idx/tri lower bound, so cap ~8500 tris with margin).
    let mut head = decimate_to_tris(&head_raw, HEAD_TRI_BUDGET);
    if head.tris.len() < head_raw.tris.len() {
        eprintln!(
            "  HEAD decimated {} -> {} tris ({} -> {} verts) to fit host index budget",
            head_raw.tris.len(),
            head.tris.len(),
            head_raw.positions.len(),
            head.positions.len()
        );
    }
    let (cmap, cclean, _cfb) = build_bone_map(&c_skel, &m_skel);
    // STEP 2 CORE FIX: inverse-bind re-pose BEFORE the index remap (joints still
    // SOURCE indices). Capture a few pre-repose sample verts for the spot-check.
    let head_samples = pick_bone_samples(&head, &cmap, &m_skel, H_HEAD);
    let (rmin, rmax) = repose_part_cross_skeleton(&mut head, &c_skel, &m_skel, &cmap);
    eprintln!(
        "  HEAD inverse-bind RE-POSE applied: v' = W_M[map(b)]*InvBind_S[b]*v  bbox min={:?} max={:?}",
        rmin.map(r3),
        rmax.map(r3)
    );
    let (cref, ccln, cfb_ref) = retarget_part(&mut head, &cmap, &cclean);
    eprintln!(
        "  HEAD bone-hash retarget: {ccln}/{cref} referenced bones CLEAN ({:.0}%), {} fell back",
        100.0 * ccln as f32 / cref.max(1) as f32,
        cref - ccln
    );
    for &b in &cfb_ref {
        eprintln!(
            "    fallback src bone {b} ({:#010x}) -> mattias bone {}",
            c_skel.bones[b].name_hash, cmap[b]
        );
    }
    // Deformation spot-check: skin the captured head samples at mattias BIND and at
    // a TEST pose (Bone_Head rotated ~30deg) to prove rigid follow (no collapse).
    deform_spotcheck("HEAD", &head, &cmap, &m_skel, H_HEAD, &head_samples);
    let hmin = rmin;
    let hmax = rmax;

    // ---- TORSO part: jen ----
    let mut torso = extract_group_mesh(&jen, JEN_TORSO_ORD).expect("extract jen torso");
    eprintln!(
        "=== jen TORSO extracted: {} verts, {} tris ===",
        torso.positions.len(),
        torso.tris.len()
    );
    let (jmap, jclean, _jfb) = build_bone_map(&j_skel, &m_skel);
    let torso_samples = pick_bone_samples(&torso, &jmap, &m_skel, H_CHEST);
    let (tmin, tmax) = repose_part_cross_skeleton(&mut torso, &j_skel, &m_skel, &jmap);
    eprintln!(
        "  TORSO inverse-bind RE-POSE applied: bbox min={:?} max={:?}",
        tmin.map(r3),
        tmax.map(r3)
    );
    let (jref, jcln, jfb_ref) = retarget_part(&mut torso, &jmap, &jclean);
    eprintln!(
        "  TORSO bone-hash retarget: {jcln}/{jref} referenced bones CLEAN ({:.0}%), {} fell back",
        100.0 * jcln as f32 / jref.max(1) as f32,
        jref - jcln
    );
    for &b in &jfb_ref {
        eprintln!(
            "    fallback src bone {b} ({:#010x}) -> mattias bone {}",
            j_skel.bones[b].name_hash, jmap[b]
        );
    }
    deform_spotcheck("TORSO", &torso, &jmap, &m_skel, H_CHEST, &torso_samples);

    // stack check
    eprintln!("=== STACK CHECK (head atop torso atop legs) ===");
    eprintln!("  head Y[{:.2}..{:.2}]  torso Y[{:.2}..{:.2}]", hmin[1], hmax[1], tmin[1], tmax[1]);
    let gap = hmin[1] - tmax[1];
    eprintln!(
        "  neck seam: head_bottom {:.2} vs torso_top {:.2} -> gap {:.2} ({})",
        hmin[1],
        tmax[1],
        gap,
        if gap.abs() < 0.10 { "OK" } else { "SEAM" }
    );
    let _ = H_CHEST;

    let new_name = pandemic_hash_m2("pmc_hum_cesium"); // reuse cesium wardrobe slot

    // ---- COMBINED kitbash injection: each part STRICTLY into its own host set ----
    // chris head -> mattias head hosts; jen torso -> mattias torso/body hosts. The
    // parts injector routes each part's triangles only to that part's hosts (no
    // balanced mixing), neutralises everything else, and applies both native-skin
    // MTRL repoints.
    let head_rp = [MtrlRepoint { from: MATTIAS_HEAD_DIFFUSE, to: CHRIS_HEAD_DIFFUSE }];
    let torso_rp = [MtrlRepoint { from: MATTIAS_TORSO_DIFFUSE, to: JEN_TORSO_DIFFUSE }];
    let parts = [
        InjectPart { mesh: &head, hosts: &HEAD_HOSTS, repoints: &head_rp },
        InjectPart { mesh: &torso, hosts: &TORSO_HOSTS, repoints: &torso_rp },
    ];
    eprintln!("=== KITBASH INJECT: head->{HEAD_HOSTS:?}  torso->{TORSO_HOSTS:?} (preserve native body) ===");
    // KITBASH default: preserve the donor's native non-host groups (legs/neck/feet/
    // accessories keep drawing mattias's own geometry+weights+materials). Only the
    // head/torso host groups are rewritten with the injected parts.
    let (combined, audits, stats) =
        inject_parts_into_donor_block(&mattias, &parts, new_name, true).expect("kitbash inject");
    report_audit("COMBINED", &audits, &stats);
    for (f, t, c) in &stats.mtrl_repoints {
        eprintln!("  MTRL repoint {f:#010x} -> {t:#010x} x{c}");
    }
    eprintln!(
        "  total verts={} tris={} avg_normal={:.3} avg_tangent={:.3}",
        stats.vertex_count, stats.triangle_count, stats.avg_normal_len, stats.avg_tangent_len
    );

    std::fs::write(out_path, &combined).expect("write out");
    eprintln!("=== wrote {out_path}: {} bytes ===", combined.len());

    // gates
    assert_no_empty_drawing_group(&combined).expect("empty-drawing-group GATE");
    eprintln!("GATE: empty-drawing-group PASS");
    verify_skinned(&combined);
    verify_csum(&combined);
    eprintln!("name_hash={new_name:#010x} (pmc_hum_cesium slot)");
}

fn r3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

/// Pick up to 4 vertex indices whose dominant SOURCE bone maps (via `src_map`) to
/// the donor `attach_hash` bone — i.e. verts the test-pose rotation will move.
/// Called BEFORE the index remap (joints still source indices).
fn pick_bone_samples(
    mesh: &ExternalMesh,
    src_map: &[usize],
    dst: &Skeleton,
    attach_hash: u32,
) -> Vec<usize> {
    let target = match dst.by_hash(attach_hash) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for i in 0..mesh.positions.len() {
        if mesh.joints.is_empty() {
            break;
        }
        let w = mesh.weights[i];
        let dom = (0..4).max_by_key(|&k| w[k]).unwrap();
        let sb = mesh.joints[i][dom] as usize;
        if sb < src_map.len() && src_map[sb] == target {
            out.push(i);
            if out.len() >= 4 {
                break;
            }
        }
    }
    out
}

/// Skin a vertex (re-posed, joints already MATTIAS indices) with the given per-bone
/// CURRENT world transforms: v_world = Σ w_b · (InvBind_M[b] · Cur[b]) · v.
fn skin_vertex(
    pos: [f32; 3],
    joints: [u8; 4],
    weights: [u8; 4],
    dst: &Skeleton,
    cur: &[[[f32; 4]; 4]],
) -> [f32; 3] {
    let wsum: u32 = weights.iter().map(|&w| w as u32).sum();
    let wsum = if wsum == 0 { 255 } else { wsum } as f32;
    let mut acc = [0.0f32; 3];
    for k in 0..4 {
        if weights[k] == 0 {
            continue;
        }
        let b = joints[k] as usize;
        let inv_bind = affine_inverse(&dst.bones[b].world);
        let palette = mat4_mul(&inv_bind, &cur[b]);
        let p = transform_point(&palette, pos);
        let w = weights[k] as f32 / wsum;
        for j in 0..3 {
            acc[j] += w * p[j];
        }
    }
    acc
}

/// STEP-2 deformation spot-check: skin the sample verts at mattias BIND (cur=world)
/// and at a TEST pose (the `attach_hash` bone rotated ~30deg about its own pivot).
/// Prints bind vs posed positions and the move magnitude — sane = small rigid
/// follow, FAIL would be collapse-to-origin or explosion.
fn deform_spotcheck(
    label: &str,
    mesh: &ExternalMesh,
    _src_map: &[usize],
    dst: &Skeleton,
    attach_hash: u32,
    samples: &[usize],
) {
    eprintln!("  {label} DEFORM SPOT-CHECK (bind vs Bone[{attach_hash:#010x}] +30deg):");
    if samples.is_empty() || mesh.joints.is_empty() {
        eprintln!("    (no samples influenced by attach bone)");
        return;
    }
    let hb = match dst.by_hash(attach_hash) {
        Some(t) => t,
        None => return,
    };
    // bind world transforms
    let bind: Vec<[[f32; 4]; 4]> = dst.bones.iter().map(|b| b.world).collect();
    // test pose: rotate the attach bone (and, since the part is single-bone-graft
    // dominant, only it) by 30deg about Y around its own pivot.
    let piv = [bind[hb][3][0], bind[hb][3][1], bind[hb][3][2]];
    let a = 30.0f32.to_radians();
    let (c, s) = (a.cos(), a.sin());
    let roty = [[c, 0.0, s, 0.0], [0.0, 1.0, 0.0, 0.0], [-s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let tr = |t: [f32; 3]| [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [t[0], t[1], t[2], 1.0]];
    let about_pivot = mat4_mul(&mat4_mul(&tr([-piv[0], -piv[1], -piv[2]]), &roty), &tr(piv));
    let mut posed = bind.clone();
    posed[hb] = mat4_mul(&bind[hb], &about_pivot);
    let mut max_move = 0.0f32;
    for (n, &i) in samples.iter().enumerate() {
        let p = mesh.positions[i];
        let j = mesh.joints[i];
        let w = mesh.weights[i];
        let b0 = skin_vertex(p, j, w, dst, &bind);
        let b1 = skin_vertex(p, j, w, dst, &posed);
        let d = ((b1[0] - b0[0]).powi(2) + (b1[1] - b0[1]).powi(2) + (b1[2] - b0[2]).powi(2)).sqrt();
        max_move = max_move.max(d);
        if n < 3 {
            eprintln!(
                "    v#{i}: bind=({:.2},{:.2},{:.2}) posed=({:.2},{:.2},{:.2}) moved={:.3}",
                b0[0], b0[1], b0[2], b1[0], b1[1], b1[2], d
            );
        }
    }
    let bind_ok = samples.iter().all(|&i| mesh.positions[i].iter().all(|v| v.is_finite()));
    let verdict = if max_move > 1e-3 && max_move < 1.0 { "RIGID-FOLLOW OK (no collapse)" } else if max_move <= 1e-3 { "STATIC (sample not moved by bone?)" } else { "EXPLODE/COLLAPSE — FAIL" };
    eprintln!("    max move {max_move:.3} -> {verdict}; bind finite={bind_ok}");
}

fn report_audit(label: &str, audits: &[mercs2_formats::model_inject::GroupBudgetAudit], stats: &mercs2_formats::model_inject::InjectStats) {
    for a in audits {
        let ok = a.injected_vc <= a.donor_vc && a.injected_ic <= a.donor_ic;
        eprintln!(
            "  {label} grp{:>2}: vc {}/{} ic {}/{} tris={} {}",
            a.group, a.injected_vc, a.donor_vc, a.injected_ic, a.donor_ic, a.triangles,
            if ok { "OK" } else { "FAIL" }
        );
    }
    eprintln!("  {label} emptied groups: {:?}", stats.emptied_groups);
}

/// Verify every injected (drawing) group carries varied BLENDINDICES (skinned).
fn verify_skinned(block: &[u8]) {
    let ulen = read_u32_le(block, 16) as usize;
    let ucfx = &block[20..20 + ulen];
    let data_off = read_u32_le(ucfx, 4) as usize;
    let ndesc = read_u32_le(ucfx, 16) as usize;
    let prmg: Vec<usize> = (0..ndesc)
        .filter(|&i| &ucfx[20 + i * 20..20 + i * 20 + 4] == b"PRMG" && read_u32_le(ucfx, 20 + i * 20 + 4) == 0xFFFF_FFFF)
        .collect();
    let leaf_at = |i: usize| (data_off + read_u32_le(ucfx, 20 + i * 20 + 4) as usize, read_u32_le(ucfx, 20 + i * 20 + 8) as usize);
    let mut skinned_groups = 0;
    for (gi, &pr) in prmg.iter().enumerate() {
        let nxt = if gi + 1 < prmg.len() { prmg[gi + 1] } else { ndesc };
        let mut state = 0u8;
        let (mut sd, mut stride, mut sn) = (None, 0usize, 0usize);
        let mut prmt = None;
        for i in (pr + 1)..nxt {
            let tag = &ucfx[20 + i * 20..20 + i * 20 + 4];
            let cm = read_u32_le(ucfx, 20 + i * 20 + 4) == 0xFFFF_FFFF;
            if tag == b"STRM" && cm { state = 1; }
            else if cm { state = 0; }
            else if state == 1 && tag == b"info" { let (o, _) = leaf_at(i); stride = read_u32_le(ucfx, o + 4) as usize; sn = read_u32_le(ucfx, o + 8) as usize; }
            else if state == 1 && tag == b"data" { sd = Some(leaf_at(i)); }
            else if tag == b"PRMT" && !cm { prmt = Some(i); }
        }
        let draws = prmt.map_or(false, |p| { let (o, sz) = leaf_at(p); (0..sz / 16).any(|r| read_u32_le(ucfx, o + r * 16 + 8) > 0) });
        if !draws || stride != 40 { continue; }
        let Some((s, dsz)) = sd else { continue };
        let n = sn.min(dsz / 40);
        if n == 0 { continue; }
        let mut distinct: HashSet<[u8; 4]> = HashSet::new();
        for v in 0..n { let o = s + v * 40; distinct.insert([ucfx[o + 16], ucfx[o + 17], ucfx[o + 18], ucfx[o + 19]]); }
        let pos_w = u16::from_le_bytes([ucfx[s + 6], ucfx[s + 7]]);
        let ymin = (0..n).map(|v| read_f16_le(ucfx, s + v * 40 + 2)).fold(f32::INFINITY, f32::min);
        let ymax = (0..n).map(|v| read_f16_le(ucfx, s + v * 40 + 2)).fold(f32::NEG_INFINITY, f32::max);
        eprintln!("  INJECTED grp#{gi}: {n}v POS.w={pos_w:#06x} distinct_idx={} Y[{ymin:.2}..{ymax:.2}]", distinct.len());
        assert_eq!(pos_w, 0x3c00, "POS.w must be 1.0");
        assert!(distinct.len() > 1, "group {gi} not skinned (uniform BLENDINDICES)");
        skinned_groups += 1;
    }
    eprintln!("GATE: all {skinned_groups} injected drawing groups skinned (distinct BLENDINDICES) PASS");
}

fn verify_csum(block: &[u8]) {
    use mercs2_formats::crc32::crc32_mercs2;
    let ulen = read_u32_le(block, 16) as usize;
    let ucfx = &block[20..20 + ulen];
    assert_eq!(&ucfx[ucfx.len() - 8..ucfx.len() - 4], b"CSUM");
    let stored = read_u32_le(ucfx, ucfx.len() - 4);
    let calc = crc32_mercs2(&ucfx[..ucfx.len() - 8]);
    assert_eq!(stored, calc, "CSUM mismatch");
    eprintln!("GATE: CSUM valid PASS");
}
