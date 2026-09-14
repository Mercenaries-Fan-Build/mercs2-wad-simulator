//! `segm_collision_census` — OFFLINE, WAD-wide census of how a MODEL container's collision shapes
//! bind to its `SEGM` records. Feeds the "collision follows new geometry" (`add_model
//! collision: follow_geometry`) multi-shape design.
//!
//! For every container in `vz.wad` that carries a `PHY2` Havok collider, dump:
//!   * the collision shape census (class + order + per-`WpMeshShape16` AABB / vert-tri range),
//!   * the FULL `SEGM` table (idx, bone, seg_id, state_mask),
//!   * the `INDX` table + the set of `seg_id`s the RENDER path references (`INDX[sub_object]`),
//!   * the COMPLEMENT: SEGM records NOT referenced by any local INDX entry,
//!   * per-bone HIER world positions (for the shape-AABB ↔ bone cross-check).
//!
//! Then test the four candidate collision↔SEGM mappings (H1..H4 — see the module doc block in the
//! deliverable) across the WHOLE collider set and print the cross-model verdict. Writes a full JSON
//! dump for offline analysis.
//!
//! Nothing is mutated; `vz.wad` is opened read-only. Reproduce:
//!   cargo run -p mercs2_probe --bin segm_collision_census -- [--json OUT] [--max-blocks N] [--verbose K]

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::havok::{find_packfiles, Shape};
use mercs2_formats::model_cubeize::{parse_segm, sub_object_count, SegRec};
use mercs2_formats::orchestrator::parse_indx;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::skeleton::Skeleton;
use mercs2_formats::ucfx::walk_decompressed_block;
use std::collections::BTreeMap;
use std::path::Path;

fn arg(a: &[String], n: &str) -> Option<String> {
    a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned()
}
fn has(a: &[String], n: &str) -> bool {
    a.iter().any(|x| x == n)
}

/// Does this UCFX container carry a PHY2 collider descriptor row? (mirror of phy2_census::phy2_span)
fn has_phy2(container: &[u8]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().ok()?) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().ok()?) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > container.len() {
            break;
        }
        if &container[r..r + 4] != b"PHY2" {
            continue;
        }
        let row_u0 = u32::from_le_bytes(container[r + 4..r + 8].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(container[r + 8..r + 12].try_into().ok()?) as usize;
        if row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let start = if data_area_off > 0 { data_area_off + row_u0 } else { 8 + row_u0 };
        if start + size > container.len() {
            return None;
        }
        return Some((start, size));
    }
    None
}

/// AABB accumulator.
#[derive(Clone, Copy)]
struct Aabb {
    lo: [f32; 3],
    hi: [f32; 3],
}
impl Aabb {
    fn new() -> Self {
        Aabb { lo: [f32::MAX; 3], hi: [f32::MIN; 3] }
    }
    fn add(&mut self, p: [f32; 3]) {
        for k in 0..3 {
            self.lo[k] = self.lo[k].min(p[k]);
            self.hi[k] = self.hi[k].max(p[k]);
        }
    }
    fn valid(&self) -> bool {
        self.lo[0] <= self.hi[0]
    }
    fn centroid(&self) -> [f32; 3] {
        [
            0.5 * (self.lo[0] + self.hi[0]),
            0.5 * (self.lo[1] + self.hi[1]),
            0.5 * (self.lo[2] + self.hi[2]),
        ]
    }
}

/// A decoded collision mesh unit for the census.
#[derive(serde::Serialize)]
struct MeshRec {
    verts: usize,
    tris: usize,
    decoded: bool,
    aabb_lo: [f32; 3],
    aabb_hi: [f32; 3],
    centroid: [f32; 3],
    /// nearest HIER bone index to the AABB centroid (world-rest), and its distance.
    nearest_bone: i64,
    nearest_dist: f32,
}

#[derive(serde::Serialize)]
struct SegmRec {
    idx: usize,
    bone: u16,
    seg_id: u8,
    mask: u8,
    self_ok: bool,
    referenced: bool,
    bone_pos: [f32; 3],
}

#[derive(serde::Serialize)]
struct ModelRec {
    block: u16,
    container: usize,
    name_hash: u32,
    n_desc: usize,
    // shape census
    class_counts: BTreeMap<String, u32>,
    mopp_count: usize,
    mesh_count: usize,
    mesh_decoded: usize,
    convex_count: usize,
    box_count: usize,
    capsule_count: usize,
    sphere_count: usize,
    other_shapes: Vec<String>,
    // segm / indx / render
    segm_count: usize,
    sub_objects: usize,
    indx_len: usize,
    referenced_seg_ids: Vec<usize>,
    complement_seg_ids: Vec<usize>,
    hier_nodes: usize,
    segm: Vec<SegmRec>,
    meshes: Vec<MeshRec>,
    // per-model hypothesis flags
    h1_mesh_eq_complement: bool,
    mesh_eq_mopp: bool,
    mesh_eq_segm: bool,
}

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Build a Skeleton from a raw UCFX container (wrap it in the 20-byte block header from_block expects).
fn skeleton_of(container: &[u8]) -> Option<Skeleton> {
    let mut wrapped = vec![0u8; 20];
    wrapped[16..20].copy_from_slice(&(container.len() as u32).to_le_bytes());
    wrapped.extend_from_slice(container);
    Skeleton::from_block(&wrapped).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json_out = arg(&args, "--json");
    let max_blocks: usize = arg(&args, "--max-blocks").and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let verbose: usize = arg(&args, "--verbose").and_then(|s| s.parse().ok()).unwrap_or(12);
    let quiet = has(&args, "--quiet");

    let Some(vz) = game_paths::vz_wad(Path::new(".")) else {
        eprintln!("SKIPPING: vz.wad not found (set MERCS2_GAME_DIR or .mercs2-local.toml)");
        std::process::exit(0);
    };
    let mut f = std::fs::File::open(&vz).expect("open vz.wad");
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).expect("ffcs");
    let nblocks = ar.indx.len().min(max_blocks);
    eprintln!("segm_collision_census: {} blocks in {}", ar.indx.len(), vz.display());

    let mut models: Vec<ModelRec> = Vec::new();
    for block in 0..nblocks as u16 {
        let Ok(dec) = decompress_block(&mut f, &ar.indx, block) else { continue };
        let (parsed, _issues) = walk_decompressed_block(&dec, "census");
        for (ci, c) in parsed.containers.iter().enumerate() {
            let Some((s, sz)) = has_phy2(c) else { continue };
            if s + sz > c.len() {
                continue;
            }
            let name_hash = parsed.entries.get(ci).map(|e| e.name_hash).unwrap_or(0);
            let n_desc = u32::from_le_bytes(c[16..20].try_into().unwrap()) as usize;

            // ---- shapes ----
            let phy2 = &c[s..s + sz];
            let mut class_counts: BTreeMap<String, u32> = BTreeMap::new();
            let mut meshes_raw: Vec<mercs2_formats::havok::MeshShape> = Vec::new();
            let (mut mopp_count, mut convex_count, mut box_count) = (0usize, 0usize, 0usize);
            let (mut capsule_count, mut sphere_count) = (0usize, 0usize);
            let mut other_shapes: Vec<String> = Vec::new();
            for (_off, pf) in find_packfiles(phy2) {
                for (cn, n) in &pf.class_counts {
                    *class_counts.entry(cn.clone()).or_insert(0) += *n;
                }
                for sh in &pf.shapes {
                    match sh {
                        Shape::Mopp => mopp_count += 1,
                        Shape::Mesh(m) => meshes_raw.push(m.clone()),
                        Shape::Convex(_) => convex_count += 1,
                        Shape::Box { .. } => box_count += 1,
                        Shape::Capsule(_) => capsule_count += 1,
                        Shape::Sphere { .. } => sphere_count += 1,
                        Shape::Other(s) => other_shapes.push(s.clone()),
                    }
                }
            }
            let mesh_count = *class_counts.get("WpMeshShape16").unwrap_or(&0) as usize;

            // ---- segm / indx / render binding ----
            let segm: Vec<SegRec> = parse_segm(c);
            let indx = parse_indx(c);
            let subobjs = sub_object_count(c);
            // The render path references SEGM[INDX[sub_object]] for sub_object in 0..subobjs.
            let mut referenced: Vec<usize> = Vec::new();
            for k in 0..subobjs {
                let seg_id = indx.get(k).copied().unwrap_or(k);
                if seg_id < segm.len() && !referenced.contains(&seg_id) {
                    referenced.push(seg_id);
                }
            }
            referenced.sort_unstable();
            let complement: Vec<usize> =
                (0..segm.len()).filter(|i| !referenced.contains(i)).collect();

            // ---- HIER positions ----
            let skel = skeleton_of(c);
            let hier_nodes = skel.as_ref().map(|s| s.bones.len()).unwrap_or(0);
            let bone_pos = |b: u16| -> [f32; 3] {
                skel.as_ref()
                    .and_then(|s| s.bones.get(b as usize))
                    .map(|bo| bo.world_pos())
                    .unwrap_or([0.0; 3])
            };

            // ---- decoded mesh AABBs + nearest bone ----
            let mut mesh_recs: Vec<MeshRec> = Vec::new();
            let mut mesh_decoded = 0usize;
            for m in &meshes_raw {
                let mut bb = Aabb::new();
                for v in &m.vertices {
                    bb.add(*v);
                }
                let decoded = !m.indices.is_empty() && bb.valid();
                if decoded {
                    mesh_decoded += 1;
                }
                let cen = if bb.valid() { bb.centroid() } else { [0.0; 3] };
                // nearest HIER bone to the centroid
                let (mut nb, mut nd) = (-1i64, f32::MAX);
                if decoded {
                    if let Some(sk) = &skel {
                        for bo in &sk.bones {
                            let d = dist3(cen, bo.world_pos());
                            if d < nd {
                                nd = d;
                                nb = bo.index as i64;
                            }
                        }
                    }
                }
                mesh_recs.push(MeshRec {
                    verts: m.vertices.len(),
                    tris: m.indices.len(),
                    decoded,
                    aabb_lo: if bb.valid() { bb.lo } else { [0.0; 3] },
                    aabb_hi: if bb.valid() { bb.hi } else { [0.0; 3] },
                    centroid: cen,
                    nearest_bone: nb,
                    nearest_dist: if nd.is_finite() { nd } else { -1.0 },
                });
            }

            let segm_recs: Vec<SegmRec> = segm
                .iter()
                .enumerate()
                .map(|(i, r)| SegmRec {
                    idx: i,
                    bone: r.bone,
                    seg_id: r.seg_id,
                    mask: r.state_mask,
                    self_ok: r.seg_id as usize == i,
                    referenced: referenced.contains(&i),
                    bone_pos: bone_pos(r.bone),
                })
                .collect();

            models.push(ModelRec {
                block,
                container: ci,
                name_hash,
                n_desc,
                mopp_count,
                mesh_count,
                mesh_decoded,
                convex_count,
                box_count,
                capsule_count,
                sphere_count,
                other_shapes,
                segm_count: segm.len(),
                sub_objects: subobjs,
                indx_len: indx.len(),
                referenced_seg_ids: referenced.clone(),
                complement_seg_ids: complement.clone(),
                hier_nodes,
                h1_mesh_eq_complement: mesh_count == complement.len() && mesh_count > 0,
                mesh_eq_mopp: mesh_count == mopp_count,
                mesh_eq_segm: mesh_count == segm.len() && mesh_count > 0,
                class_counts,
                segm: segm_recs,
                meshes: mesh_recs,
            });
        }
        if block % 200 == 0 {
            eprintln!("  ..block {block}/{nblocks}  collider models so far: {}", models.len());
        }
    }

    // ============ CROSS-MODEL VERDICT ============
    let n = models.len();
    // Only models that actually carry a static mesh collider (WpMeshShape16) are in scope for the
    // SEGM↔mesh binding question. Convex-only PHY2 (destructible break hulls) is a different system.
    let mesh_models: Vec<&ModelRec> = models.iter().filter(|m| m.mesh_count > 0).collect();
    let rich: Vec<&ModelRec> = mesh_models.iter().copied().filter(|m| m.segm_count > 1).collect();

    let count = |f: &dyn Fn(&ModelRec) -> bool, set: &[&ModelRec]| set.iter().filter(|m| f(m)).count();

    println!("\n================ SEGM ↔ COLLISION CENSUS (offline, vz.wad) ================");
    println!("containers with a PHY2 collider ............ {n}");
    println!("  with ≥1 WpMeshShape16 (static mesh) ...... {}", mesh_models.len());
    println!("  of those, rich SEGM (>1 record) .......... {}", rich.len());
    println!();

    // Baseline counts
    let eq_segm = count(&|m| m.mesh_count == m.segm_count, &mesh_models);
    let eq_mopp = count(&|m| m.mesh_count == m.mopp_count, &mesh_models);
    let eq_compl = count(&|m| m.mesh_count == m.complement_seg_ids.len(), &mesh_models);
    println!("mesh_count == segm_count ................... {eq_segm}/{} (memory's single-shape claim)", mesh_models.len());
    println!("mesh_count == mopp_count ................... {eq_mopp}/{} (each WpMeshShape16 wrapped by one MOPP)", mesh_models.len());
    println!("mesh_count == complement_count (H1) ........ {eq_compl}/{}", mesh_models.len());
    println!();

    // Restricted to rich models (the real question)
    if !rich.is_empty() {
        let r_eq_segm = count(&|m| m.mesh_count == m.segm_count, &rich);
        let r_eq_mopp = count(&|m| m.mesh_count == m.mopp_count, &rich);
        let r_eq_compl = count(&|m| m.mesh_count == m.complement_seg_ids.len(), &rich);
        println!("--- RICH models only ({}): ---", rich.len());
        println!("  mesh==segm ...... {r_eq_segm}");
        println!("  mesh==mopp ...... {r_eq_mopp}");
        println!("  mesh==complement (H1) ... {r_eq_compl}");
    }
    println!();

    // H2: are complement records mask/bone-distinguished from referenced records?
    // Test 1: do all mesh models have a state_mask value that appears in complement but NOT referenced?
    let mut h2_mask_sep = 0usize;
    let mut h2_examples: Vec<(u32, Vec<u8>, Vec<u8>)> = Vec::new();
    for m in &mesh_models {
        let ref_masks: std::collections::BTreeSet<u8> =
            m.segm.iter().filter(|r| r.referenced).map(|r| r.mask).collect();
        let compl_masks: std::collections::BTreeSet<u8> =
            m.segm.iter().filter(|r| !r.referenced).map(|r| r.mask).collect();
        let only_compl: Vec<u8> = compl_masks.difference(&ref_masks).copied().collect();
        if !only_compl.is_empty() && !m.complement_seg_ids.is_empty() {
            h2_mask_sep += 1;
            if h2_examples.len() < 8 {
                h2_examples.push((
                    m.name_hash,
                    ref_masks.into_iter().collect(),
                    compl_masks.into_iter().collect(),
                ));
            }
        }
    }
    println!("H2  models where complement carries a state_mask absent from referenced: {h2_mask_sep}/{}", mesh_models.len());

    // H4: for each decoded mesh, is its nearest HIER bone the `bone` of a COMPLEMENT record?
    // (positional/geometric shape→record test)
    let mut h4_hits = 0usize;
    let mut h4_total = 0usize;
    for m in &mesh_models {
        let compl_bones: std::collections::BTreeSet<u16> =
            m.segm.iter().filter(|r| !r.referenced).map(|r| r.bone).collect();
        for mr in &m.meshes {
            if !mr.decoded || mr.nearest_bone < 0 {
                continue;
            }
            h4_total += 1;
            if compl_bones.contains(&(mr.nearest_bone as u16)) {
                h4_hits += 1;
            }
        }
    }
    println!("H4  decoded meshes whose nearest HIER bone is a COMPLEMENT record's bone: {h4_hits}/{h4_total}");

    // Distribution: complement size vs mesh count (does complement OVER-count?)
    println!("\n--- per-rich-model shape/segm shape (first {verbose}) ---");
    println!("{:>5} {:>10} {:>4} {:>4} {:>4} {:>5} {:>5} {:>5} {:>5}  masks(ref|compl)",
        "blk", "name", "segm", "sub", "indx", "mesh", "mopp", "cvx", "compl");
    for m in rich.iter().take(verbose) {
        let ref_masks: Vec<u8> = m.segm.iter().filter(|r| r.referenced).map(|r| r.mask).collect();
        let compl_masks: Vec<u8> = m.segm.iter().filter(|r| !r.referenced).map(|r| r.mask).collect();
        println!(
            "{:>5} 0x{:08X} {:>4} {:>4} {:>4} {:>5} {:>5} {:>5} {:>5}  {:02X?} | {:02X?}",
            m.block, m.name_hash, m.segm_count, m.sub_objects, m.indx_len,
            m.mesh_count, m.mopp_count, m.convex_count, m.complement_seg_ids.len(),
            dedup(&ref_masks), dedup(&compl_masks)
        );
    }

    if !quiet {
        // Deep dump of a few rich models
        for m in rich.iter().take(3) {
            deep_dump(m);
        }
    }

    if let Some(path) = json_out {
        let json = serde_json::to_string_pretty(&models).unwrap();
        std::fs::write(&path, json).expect("write json");
        eprintln!("\nfull JSON dump -> {path}");
    }
}

fn dedup(v: &[u8]) -> Vec<u8> {
    let mut s: Vec<u8> = v.to_vec();
    s.sort_unstable();
    s.dedup();
    s
}

fn deep_dump(m: &ModelRec) {
    println!("\n===== DEEP block {} container {} name 0x{:08X} =====", m.block, m.container, m.name_hash);
    println!("  class_counts: {:?}", m.class_counts);
    println!("  segm={} sub_objects={} indx_len={} hier_nodes={}", m.segm_count, m.sub_objects, m.indx_len, m.hier_nodes);
    println!("  referenced seg_ids: {:?}", m.referenced_seg_ids);
    println!("  complement seg_ids: {:?}", m.complement_seg_ids);
    println!("  SEGM table (idx bone seg_id mask self ref bone_pos):");
    for r in &m.segm {
        println!("    [{:>3}] bone={:>4} seg_id={:>3} mask=0x{:02X} self={} ref={} pos=[{:.2},{:.2},{:.2}]",
            r.idx, r.bone, r.seg_id, r.mask, r.self_ok as u8, r.referenced as u8,
            r.bone_pos[0], r.bone_pos[1], r.bone_pos[2]);
    }
    println!("  MESHES (verts tris decoded centroid -> nearest_bone@dist):");
    for (i, mr) in m.meshes.iter().enumerate() {
        println!("    mesh[{i}] v={} t={} dec={} cen=[{:.2},{:.2},{:.2}] nb={} d={:.2}",
            mr.verts, mr.tris, mr.decoded as u8, mr.centroid[0], mr.centroid[1], mr.centroid[2],
            mr.nearest_bone, mr.nearest_dist);
    }
}
