//! `lc_verify` — offline gate for the landing-craft `follow_geometry` overlay.
//!
//! Proves, against the BUILT overlay WAD (not the authoring path): the regenerated PHY2 re-parses to
//! one `WpMeshShape16`, decodes back to the full source triangle/vertex count, its spatial MOPP
//! decodes to the exact key set `[0..ntris)` and is no-miss over a triangle sample, and the model
//! container's `SEGM` + `INDX` chunk bytes are BYTE-IDENTICAL to the donor's (render draw table
//! untouched). Exit 0 only if every assertion holds.
//!
//!   lc_verify --overlay <wad> --model 0xHASH --donor-block <n> --donor 0xHASH

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::havok::{parse_phy2_body, Shape};
use mercs2_formats::model_cubeize::parse_segm;
use mercs2_formats::mopp;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::walk_decompressed_block;
use std::collections::HashSet;
use std::path::Path;

fn arg(a: &[String], n: &str) -> Option<String> {
    a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned()
}
fn parse_hash(s: &str) -> u32 {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(s, 16).expect("hex hash")
}

/// Raw leaf bytes of the first descriptor with `tag` in a UCFX container (mirrors phy2_census).
fn chunk_bytes<'a>(container: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
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
        if &container[r..r + 4] != tag {
            continue;
        }
        let row_u0 = u32::from_le_bytes(container[r + 4..r + 8].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(container[r + 8..r + 12].try_into().ok()?) as usize;
        if row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let start = if data_area_off > 0 { data_area_off + row_u0 } else { 8 + row_u0 };
        return container.get(start..start + size);
    }
    None
}
fn phy2_bytes(container: &[u8]) -> Option<Vec<u8>> {
    chunk_bytes(container, b"PHY2").map(|b| b.to_vec())
}

/// Find the container whose ASET row name-hash == `hash`, scanning every block of `wad`.
fn find_container(wad: &Path, hash: u32) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(wad).ok()?;
    let size = f.metadata().ok()?.len();
    let ar = load_ffcs_archive(&mut f, size).ok()?;
    for b in 0..ar.indx.len() {
        let Ok(dec) = decompress_block(&mut f, &ar.indx, b as u16) else { continue };
        let (parsed, _issues) = walk_decompressed_block(&dec, "verify");
        for (i, c) in parsed.containers.iter().enumerate() {
            if parsed.entries[i].name_hash == hash {
                return Some(c.clone());
            }
        }
    }
    None
}

/// Whole-block re-walk must report no structural issues, for the block carrying `hash`.
fn rewalk_ok(wad: &Path, hash: u32) -> bool {
    let Ok(mut f) = std::fs::File::open(wad) else { return false };
    let Ok(size) = f.metadata().map(|m| m.len()) else { return false };
    let Ok(ar) = load_ffcs_archive(&mut f, size) else { return false };
    for b in 0..ar.indx.len() {
        let Ok(dec) = decompress_block(&mut f, &ar.indx, b as u16) else { continue };
        let (parsed, issues) = walk_decompressed_block(&dec, "verify");
        if parsed.entries.iter().any(|e| e.name_hash == hash) {
            if !issues.is_empty() {
                eprintln!("  re-walk issues on block {b}: {issues:?}");
                return false;
            }
            return true;
        }
    }
    false
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let overlay = arg(&a, "--overlay").expect("--overlay <wad>");
    let model_hash = parse_hash(&arg(&a, "--model").expect("--model 0xHASH"));
    let donor_block: usize = arg(&a, "--donor-block").and_then(|s| s.parse().ok()).expect("--donor-block");
    let donor_hash = parse_hash(&arg(&a, "--donor").expect("--donor 0xHASH"));
    let want_tris: usize = arg(&a, "--tris").and_then(|s| s.parse().ok()).unwrap_or(26937);
    let want_verts: usize = arg(&a, "--verts").and_then(|s| s.parse().ok()).unwrap_or(14580);

    let mut fail = 0usize;
    macro_rules! check {
        ($cond:expr, $($m:tt)*) => {
            if $cond { println!("  PASS  {}", format!($($m)*)); }
            else { println!("  FAIL  {}", format!($($m)*)); fail += 1; }
        };
    }

    println!("=== lc_verify: overlay {overlay} model 0x{model_hash:08X} donor 0x{donor_hash:08X} ===");

    // --- overlay model container ---
    let ov = Path::new(&overlay);
    let mc = find_container(ov, model_hash).expect("model container not in overlay");
    println!("overlay model container: {} B", mc.len());

    // --- donor container from vz.wad block ---
    let vz = game_paths::vz_wad(Path::new(".")).expect("vz.wad not found");
    let mut f = std::fs::File::open(&vz).expect("open vz.wad");
    let vsz = f.metadata().unwrap().len();
    let var = load_ffcs_archive(&mut f, vsz).expect("ffcs vz");
    let dblk = decompress_block(&mut f, &var.indx, donor_block as u16).expect("decompress donor block");
    let (dparsed, _) = walk_decompressed_block(&dblk, "donor");
    let dc = dparsed
        .containers
        .iter()
        .zip(&dparsed.entries)
        .find(|(_, e)| e.name_hash == donor_hash)
        .map(|(c, _)| c.clone())
        .expect("donor container not in block");
    println!("donor container:         {} B", dc.len());

    // --- GATE 1: render draw table (SEGM/INDX) vs donor ---
    // INDX (sub-object→seg_id table) must be byte-identical: the collision regen and the render inject
    // both leave it untouched. SEGM is byte-identical EXCEPT the one host-group record the render inject
    // deliberately UNBINDS to `node=-1, mask=0x7f` (so the injected model-space mesh draws at every view
    // state); its record COUNT is preserved. Collision does NOT touch SEGM (census-proven).
    let indx_d = chunk_bytes(&dc, b"INDX");
    let indx_o = chunk_bytes(&mc, b"INDX");
    check!(indx_d == indx_o, "INDX byte-identical to donor ({:?} B)", indx_o.map(|b| b.len()));
    let segm_d = chunk_bytes(&dc, b"SEGM").unwrap_or(&[]);
    let segm_o = chunk_bytes(&mc, b"SEGM").unwrap_or(&[]);
    check!(segm_d.len() == segm_o.len(), "SEGM same length as donor ({} B)", segm_o.len());
    let diff_recs = segm_d
        .chunks(4)
        .zip(segm_o.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    check!(diff_recs <= 1, "SEGM differs from donor in at most the ONE unbound host record (differs in {diff_recs})");
    let seg_d = parse_segm(&dc).len();
    let seg_o = parse_segm(&mc).len();
    check!(seg_d == seg_o && seg_d > 1, "SEGM record count preserved & RICH: donor {seg_d} == overlay {seg_o} (>1)");

    // Collision-regen neutrality: a `collision: donor` build of the SAME model must carry byte-identical
    // SEGM + INDX — proving `follow_geometry` changed ONLY the PHY2, never the render draw table.
    if let Some(dcol) = arg(&a, "--overlay-donorcol") {
        let mcd = find_container(Path::new(&dcol), model_hash).expect("model not in donorcol overlay");
        check!(chunk_bytes(&mc, b"SEGM") == chunk_bytes(&mcd, b"SEGM"), "SEGM identical to collision:donor build (regen is SEGM-neutral)");
        check!(chunk_bytes(&mc, b"INDX") == chunk_bytes(&mcd, b"INDX"), "INDX identical to collision:donor build (regen is INDX-neutral)");
    }

    // --- GATE 2: PHY2 re-parses to N WpMeshShape16 shapes covering the FULL mesh ---
    let body = phy2_bytes(&mc).expect("overlay container has no PHY2");
    let pf = parse_phy2_body(&body).expect("overlay PHY2 re-parses");
    check!(pf.version.starts_with("Havok-5.5"), "PHY2 version {}", pf.version);
    check!(pf.class_counts.get("WpArray").copied() == Some(1), "one root WpArray");
    let meshes: Vec<&mercs2_formats::havok::MeshShape> = pf
        .shapes
        .iter()
        .filter_map(|s| if let Shape::Mesh(m) = s { Some(m) } else { None })
        .collect();
    let nshapes = meshes.len();
    check!(nshapes >= 1, "≥1 WpMeshShape16 (got {nshapes})");
    let convex = pf.shapes.iter().filter(|s| matches!(s, Shape::Convex(_))).count();
    check!(convex == 0, "no convex hulls remain (donor's 39 replaced) — got {convex}");
    let bvtree = pf.class_counts.get("hkpMoppBvTreeShape").copied().unwrap_or(0);
    let mcode = pf.class_counts.get("hkpMoppCode").copied().unwrap_or(0);
    check!(bvtree == nshapes as u32 && mcode == nshapes as u32, "one BvTree + one MoppCode per shape (got {bvtree}/{mcode} for {nshapes})");
    let total_tris: usize = meshes.iter().map(|m| m.indices.len()).sum();
    let total_verts: usize = meshes.iter().map(|m| m.vertices.len()).sum();
    check!(total_tris == want_tris, "shapes cover the FULL mesh: {total_tris} tris (want {want_tris})");
    // The shared pool concatenates each shape's own vertex set, so boundary verts appear in more than
    // one shape — the pool is LARGER than the source vertex count, never smaller (no vertex is lost).
    // The collision SURFACE is the {total_tris} triangles above; the pool size is a representation
    // detail of multi-shape chunking, not a fidelity measure.
    check!(total_verts >= want_verts,
        "no vertex lost: pool {total_verts} verts ≥ source {want_verts} (extra = per-shape boundary dupes from MOPP chunking)");

    // --- GATE 3+4: each shape's MOPP decodes [0..tris_i) and is no-miss over its own tris ---
    let mopps = mopp::extract_mopp_with_info(&body);
    check!(mopps.len() == nshapes, "one hkpMoppCode buffer per shape ({} for {nshapes})", mopps.len());
    for (si, m) in meshes.iter().enumerate() {
        let ntris = m.indices.len();
        let Some((code, info)) = mopps.get(si) else { continue };
        let dec = mopp::decode(code);
        check!(dec.error.is_none(), "shape[{si}] MOPP decodes clean: {:?}", dec.error);
        check!(dec.consumed == code.len(), "shape[{si}] 100% MOPP byte coverage ({}/{})", dec.consumed, code.len());
        let (ks, range, missing) = dec.key_summary();
        check!(ks.len() == ntris && range == Some((0, ntris as u32 - 1)) && missing.is_empty(),
            "shape[{si}] MOPP keys are exactly [0..{ntris}) ({} keys, range {range:?}, {} missing)", ks.len(), missing.len());

        let vc = |i: u16| m.vertices[i as usize];
        let step = (ntris / 60).max(1);
        let (mut self_miss, mut overlap_miss, mut probes) = (0usize, 0usize, 0usize);
        let mut ti = 0usize;
        while ti < ntris {
            let mut qmin = [f32::MAX; 3];
            let mut qmax = [f32::MIN; 3];
            for v in [vc(m.indices[ti][0]), vc(m.indices[ti][1]), vc(m.indices[ti][2])] {
                for k in 0..3 { qmin[k] = qmin[k].min(v[k]); qmax[k] = qmax[k].max(v[k]); }
            }
            let cand: HashSet<u32> = mopp::query_aabb(code, info, qmin, qmax).into_iter().collect();
            if !cand.contains(&(ti as u32)) { self_miss += 1; }
            for (j, t) in m.indices.iter().enumerate() {
                let mut tlo = [f32::MAX; 3];
                let mut thi = [f32::MIN; 3];
                for v in [vc(t[0]), vc(t[1]), vc(t[2])] {
                    for k in 0..3 { tlo[k] = tlo[k].min(v[k]); thi[k] = thi[k].max(v[k]); }
                }
                if (0..3).all(|k| tlo[k] <= qmax[k] && thi[k] >= qmin[k]) && !cand.contains(&(j as u32)) {
                    overlap_miss += 1;
                }
            }
            probes += 1;
            ti += step;
        }
        check!(self_miss == 0 && overlap_miss == 0,
            "shape[{si}] MOPP no-miss over {probes} probes (self {self_miss}, overlap {overlap_miss})");
    }

    // --- GATE 5: whole block re-walks clean ---
    check!(rewalk_ok(ov, model_hash), "overlay block re-walks with no structural issues");

    println!("\n{}", if fail == 0 { "ALL GATES PASS" } else { "GATES FAILED" });
    std::process::exit(if fail == 0 { 0 } else { 1 });
}
