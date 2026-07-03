//!  Headless diagnostic / export subcommands carved out of `main.rs`.
//!
//!  These are consumed by the `mercs2_probe` binary (one subcommand each). They are
//!  render-agnostic: they read the WAD and print/emit analysis, never opening a window.
//!  Shared helpers live in `crate::worldutil`; the wgpu run modes stay in the engine binary.

#![allow(clippy::all)]
use crate::worldutil::*;
use crate::{mesh, pose, wad};

/// The ECS `Model` component m2 hash (`pandemic_hash_m2("model")`), stride 4 = one u32 mesh
/// handle. Same value as `wad::MODEL_TYPE_HASH` (the MESH-block "Model" CHDR class hash).
const MODEL_COMP_HASH: u32 = 0x5B72_4250;

/// Export the c3 building `Model`s (`0x5B724250`) from building-only c3 cells (those WITHOUT a
/// terrainmesh) to Wavefront OBJ, in the viewer's review-tree layout `<outdir>/c3build/<cell>/mesh.obj`
/// (auto-scanned by the viewer's vite plugin). Geometry is LOCAL (unplaced) — the point is to VISUALLY
/// inspect what these unplaced Models are, to crack their placement. No coordinate flips.
pub fn export_c3_obj(wadpath: &str, outdir: &str) -> Result<(), String> {
    use mercs2_formats::ucfx::parse_block_entry_table;
    let mut w = wad::open(wadpath)?;
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    let mut exported = 0usize;
    for (bi, path) in paths.iter().enumerate() {
        let lname = path.to_lowercase();
        // Bare c3 cell blocks only (the P000_Q3 tier).
        let is_c3 = lname.contains("\\c3") && lname.contains("_p000_q3") && !lname.contains('-');
        if !is_c3 {
            continue;
        }
        let Ok(dec) = wad::decompress_block_index(&mut w, bi as u16) else { continue };
        let (count, entries) = parse_block_entry_table(&dec);
        // Building-only: has a Model but NO terrainmesh (terrain cells are placed separately).
        if entries.iter().any(|e| e.type_hash == TERRAINMESH_TYPE_HASH) {
            continue;
        }
        let mut pos = 4 + count as usize * 16;
        let mut model: Option<(usize, usize)> = None;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if e.type_hash == wad::MODEL_TYPE_HASH && end <= dec.len() {
                model = Some((pos, end));
                break;
            }
            pos = end;
        }
        let Some((s0, s1)) = model else { continue };
        let Ok((verts, indices, _draws, _stats)) = mesh::build_indexed_from_container(&dec[s0..s1]) else { continue };
        if verts.is_empty() || indices.len() < 3 {
            continue;
        }
        // Cell name for the stem (e.g. c30140).
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.split("_P000").next().unwrap_or(s).rsplit(['\\', '/']).next().unwrap_or(s).to_string())
            .unwrap_or_else(|| format!("block{bi}"));
        let dir = format!("{outdir}/c3build/{stem}");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut obj = String::with_capacity(verts.len() * 48);
        obj.push_str(&format!("# c3 building model {stem} (block {bi}) — LOCAL/unplaced geometry\n"));
        for v in &verts {
            obj.push_str(&format!("v {} {} {}\n", v.pos[0], v.pos[1], v.pos[2]));
        }
        for v in &verts {
            obj.push_str(&format!("vn {} {} {}\n", v.normal[0], v.normal[1], v.normal[2]));
        }
        for t in indices.chunks_exact(3) {
            let (a, b, c) = (t[0] + 1, t[1] + 1, t[2] + 1);
            obj.push_str(&format!("f {a}//{a} {b}//{b} {c}//{c}\n"));
        }
        std::fs::write(format!("{dir}/mesh.obj"), obj).map_err(|e| e.to_string())?;
        exported += 1;
    }
    println!("[export-c3-obj] wrote {exported} building-model OBJs to {outdir}/c3build/  (pack='c3build' in the viewer)");
    Ok(())
}

/// Scan every c3 building model and report the FLAT, floor-sized ones — candidates for the PMC
/// interior floor (a nearly-flat mesh whose XZ footprint could be a room floor). Sorted by footprint.
pub fn c3_flat_report(wadpath: &str) -> Result<(), String> {
    use mercs2_formats::ucfx::parse_block_entry_table;
    let mut w = wad::open(wadpath)?;
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    let mut hits: Vec<(String, usize, f32, f32, f32, [f32; 3], [f32; 3], usize)> = Vec::new();
    for (bi, path) in paths.iter().enumerate() {
        let lname = path.to_lowercase();
        if !(lname.contains("\\c3") && lname.contains("_p000_q3") && !lname.contains('-')) {
            continue;
        }
        let Ok(dec) = wad::decompress_block_index(&mut w, bi as u16) else { continue };
        let (count, entries) = parse_block_entry_table(&dec);
        if entries.iter().any(|e| e.type_hash == TERRAINMESH_TYPE_HASH) {
            continue;
        }
        let mut pos = 4 + count as usize * 16;
        let mut model: Option<(usize, usize)> = None;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if e.type_hash == wad::MODEL_TYPE_HASH && end <= dec.len() {
                model = Some((pos, end));
                break;
            }
            pos = end;
        }
        let Some((s0, s1)) = model else { continue };
        let Ok((verts, _i, _d, _s)) = mesh::build_indexed_from_container(&dec[s0..s1]) else { continue };
        if verts.is_empty() {
            continue;
        }
        let mut mn = [f32::MAX; 3];
        let mut mx = [f32::MIN; 3];
        for v in &verts {
            for c in 0..3 {
                mn[c] = mn[c].min(v.pos[c]);
                mx[c] = mx[c].max(v.pos[c]);
            }
        }
        let (dx, dy, dz) = (mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2]);
        // FLAT = thin in Y; floor-sized = a real XZ footprint.
        if dy < 2.0 && (dx > 15.0 || dz > 15.0) {
            let stem = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.split("_P000").next().unwrap_or(s).rsplit(['\\', '/']).next().unwrap_or(s).to_string())
                .unwrap_or_else(|| format!("block{bi}"));
            hits.push((stem, bi, dx, dy, dz, mn, mx, verts.len()));
        }
    }
    hits.sort_by(|a, b| (b.2 * b.4).partial_cmp(&(a.2 * a.4)).unwrap_or(std::cmp::Ordering::Equal));
    println!("[c3-flat] {} flat floor-sized c3 model(s) (dy<2m, XZ>15m), largest footprint first:", hits.len());
    for (stem, bi, dx, dy, dz, mn, mx, nv) in &hits {
        println!(
            "  {stem} (blk {bi}) {nv}v dims=({dx:.1},{dy:.1},{dz:.1}) bbox x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}]",
            mn[0], mx[0], mn[1], mx[1], mn[2], mx[2]
        );
    }
    Ok(())
}

/// Dump the placement-side reference hashes from layers_static: every `ModelName` COMP `model_hash`
/// (with reuse count) and every named-placement base-name hash (`pandemic_hash_m2`, with reuse
/// count). Intersecting these with the c3 blocks' model-chunk name_hashes tests whether c3 model
/// blocks are placed upstream by reference (and how heavily reused). Emits JSON.
pub fn placement_hashes(wadpath: &str, outfile: &str) -> Result<(), String> {
    use std::collections::HashMap;
    let mut w = wad::open(wadpath)?;
    let (_low, ls) = find_terrain_blocks(&mut w)?;
    let mut mh: HashMap<u32, usize> = HashMap::new();
    for p in mercs2_formats::placement::load_model_placements(&ls) {
        *mh.entry(p.model_hash).or_default() += 1;
    }
    let mut nh: HashMap<u32, usize> = HashMap::new();
    for p in mercs2_formats::placement::load_placements(&ls)? {
        if let Some(name) = &p.name {
            let h = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
            *nh.entry(h).or_default() += 1;
        }
    }
    let mut s = String::from("{\"model_hashes\":{");
    for (i, (h, c)) in mh.iter().enumerate() {
        if i > 0 { s.push(','); }
        s.push_str(&format!("\"0x{h:08X}\":{c}"));
    }
    s.push_str("},\"name_hashes\":{");
    for (i, (h, c)) in nh.iter().enumerate() {
        if i > 0 { s.push(','); }
        s.push_str(&format!("\"0x{h:08X}\":{c}"));
    }
    s.push_str("}}");
    std::fs::write(outfile, &s).map_err(|e| e.to_string())?;
    println!(
        "[placement-hashes] ModelName: {} distinct model_hashes ({} total refs); named: {} distinct name_hashes ({} total refs) -> {outfile}",
        mh.len(), mh.values().sum::<usize>(), nh.len(), nh.values().sum::<usize>()
    );
    Ok(())
}

/// Dump FULL metadata for every c3 model block (same population --export-c3-obj rendered): the
/// block's PTHS name, its computed c3 grid centre (name→world formula), the Model chunk's own
/// name_hash (the object's identity hash — resolve via the rainbow table), and its ENTIRE chunk
/// entry table. Point: reveal any field beyond the Model geometry that fingerpoints what the object
/// is or where it goes. NDJSON, one line per block; prints a chunk type_hash histogram at the end.
pub fn c3_meta(wadpath: &str, outfile: &str) -> Result<(), String> {
    use mercs2_formats::ucfx::parse_block_entry_table;
    let mut w = wad::open(wadpath)?;
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    let mut out = String::new();
    let mut n = 0usize;
    let mut type_hist: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (bi, path) in paths.iter().enumerate() {
        let lname = path.to_lowercase();
        let is_c3 = lname.contains("\\c3") && lname.contains("_p000_q3") && !lname.contains('-');
        if !is_c3 {
            continue;
        }
        let Ok(dec) = wad::decompress_block_index(&mut w, bi as u16) else { continue };
        let (_count, entries) = parse_block_entry_table(&dec);
        let has_tm = entries.iter().any(|e| e.type_hash == TERRAINMESH_TYPE_HASH);
        let has_model = entries.iter().any(|e| e.type_hash == wad::MODEL_TYPE_HASH);
        if !has_model || has_tm {
            continue; // same population as --export-c3-obj (Model, no terrainmesh)
        }
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.split("_P000").next().unwrap_or(s).rsplit(['\\', '/']).next().unwrap_or(s).to_string())
            .unwrap_or_default();
        // c3 cell number from "c3NNNN" -> world grid centre (name→placement formula).
        let cell = stem.strip_prefix('c').and_then(|d| d.parse::<u32>().ok());
        let centre = cell.map(mercs2_formats::world_index::c3_cell_centre);
        let model_name = entries
            .iter()
            .find(|e| e.type_hash == wad::MODEL_TYPE_HASH)
            .map(|e| e.name_hash)
            .unwrap_or(0);
        // Union AABB over ALL model chunks in the block (decisive for placement: cell-local-offset
        // vs origin-centred). Walk the entry table by offset to slice each model chunk.
        let mut pos = 4 + entries.len() * 16;
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        let mut n_models = 0usize;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if e.type_hash == wad::MODEL_TYPE_HASH && end <= dec.len() {
                if let Ok((verts, _idx, _d, _s)) = mesh::build_indexed_from_container(&dec[pos..end]) {
                    for v in &verts {
                        for k in 0..3 {
                            lo[k] = lo[k].min(v.pos[k]);
                            hi[k] = hi[k].max(v.pos[k]);
                        }
                    }
                    if !verts.is_empty() {
                        n_models += 1;
                    }
                }
            }
            pos = end;
        }
        let aabb = if n_models > 0 {
            format!("[{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}]", lo[0], lo[1], lo[2], hi[0], hi[1], hi[2])
        } else {
            "null".to_string()
        };
        let mut chunks = String::new();
        for (i, e) in entries.iter().enumerate() {
            *type_hist.entry(e.type_hash).or_default() += 1;
            if i > 0 {
                chunks.push(',');
            }
            chunks.push_str(&format!(
                "[\"0x{:08X}\",\"0x{:08X}\",{}]",
                e.type_hash, e.name_hash, e.chunk_size
            ));
        }
        let (cx, cz) = centre.unwrap_or((f32::NAN, f32::NAN));
        out.push_str(&format!(
            "{{\"stem\":\"{stem}\",\"block\":{bi},\"path\":\"{}\",\"cell\":{},\"centre\":[{:.2},{:.2}],\"model_name\":\"0x{:08X}\",\"n_models\":{n_models},\"aabb\":{aabb},\"n_chunks\":{},\"chunks\":[{chunks}]}}\n",
            path.replace('\\', "\\\\"),
            cell.map(|c| c as i64).unwrap_or(-1),
            cx, cz, model_name, entries.len(),
        ));
        n += 1;
    }
    std::fs::write(outfile, &out).map_err(|e| e.to_string())?;
    let mut hist: Vec<(u32, usize)> = type_hist.into_iter().collect();
    hist.sort_by(|a, b| b.1.cmp(&a.1));
    println!("[c3-meta] wrote {n} c3 model blocks -> {outfile}");
    println!("[c3-meta] chunk type_hash histogram (across all {n} blocks):");
    for (t, c) in hist.iter().take(30) {
        println!("[c3-meta]   0x{t:08X}  x{c}");
    }
    Ok(())
}

/// Extract + build the `0x7C569307` terrainmesh from a c3 cell block (or auto-find one), reporting
/// vertex bounds (WORLD vs cell-local — the real placement question) and draw/material count (the
/// splat). This is the geometry `load_one_c3_cell` should be loading for terrain, not the small Model.
pub fn terrainmesh_probe(wadpath: &str, block: Option<u16>) -> Result<(), String> {
    use mercs2_formats::ucfx::parse_block_entry_table;
    let mut w = wad::open(wadpath)?;

    // Find candidate blocks: any with a terrainmesh entry. Auto-pick the first few if none given.
    let candidates: Vec<u16> = match block {
        Some(b) => vec![b],
        None => {
            let mut v = Vec::new();
            for i in 0..wad::block_paths(&w).len() as u16 {
                if v.len() >= 6 {
                    break;
                }
                if let Ok(dec) = wad::decompress_block_index(&mut w, i) {
                    let (_n, entries) = parse_block_entry_table(&dec);
                    if entries.iter().any(|e| e.type_hash == TERRAINMESH_TYPE_HASH) {
                        v.push(i);
                    }
                }
            }
            v
        }
    };
    if candidates.is_empty() {
        return Err("no block with a 0x7C569307 terrainmesh found".into());
    }

    // Low-res terrain heightmap (the VERIFIED-correct ground) + the TerrainObject->Transform tile
    // placement map (terrainmesh_hash -> world pos), so we test placement against the REAL position.
    let (hmap, tile_pos) = {
        let (low, ls) = find_terrain_blocks(&mut w)?;
        let tm = mercs2_formats::terrain::load_terrain(&low, &ls)?;
        let tiles = mercs2_formats::placement::load_terrain_tiles(&ls);
        let map: std::collections::HashMap<u32, [f32; 3]> =
            tiles.iter().map(|t| (t.terrainmesh_hash, t.pos)).collect();
        println!("[terrainmesh] TerrainObject tiles parsed: {} (distinct meshes {})", tiles.len(), map.len());
        (HeightMap::build(&tm), map)
    };

    for bi in candidates {
        let path = wad::block_paths(&w).get(bi as usize).cloned().unwrap_or_default();
        let dec = match wad::decompress_block_index(&mut w, bi) {
            Ok(d) => d,
            Err(e) => {
                println!("[terrainmesh] block={bi} {path}: decompress failed: {e}");
                continue;
            }
        };
        let (count, entries) = parse_block_entry_table(&dec);
        // Locate the terrainmesh chunk span (mirrors load_one_c3_cell's model walk).
        let mut pos = 4 + count as usize * 16;
        let mut span: Option<(usize, usize)> = None;
        let mut tm_hash = 0u32;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if e.type_hash == TERRAINMESH_TYPE_HASH && end <= dec.len() {
                span = Some((pos, end));
                tm_hash = e.name_hash;
                break;
            }
            pos = end;
        }
        let Some((s0, s1)) = span else {
            println!("[terrainmesh] block={bi} {path}: no terrainmesh chunk located");
            continue;
        };
        // Terrain material RCA: parse_mtrl + group indices, and scan the MTRL body for terraintextures
        // hashes (the ~30 material set) to see how the terrainmesh binds them.
        {
            let container = &dec[s0..s1];
            let mats = mercs2_formats::texture::parse_mtrl(container);
            let gmi = mercs2_formats::texture::group_material_indices(container);
            println!("[terrainmesh]   MTRL: parse_mtrl -> {} materials; group_material_indices (per draw) {:?}", mats.len(), &gmi.iter().take(12).collect::<Vec<_>>());
            for (mi, m) in mats.iter().enumerate().take(6) {
                println!("[terrainmesh]     material[{mi}] textures={:08X?}", m.textures);
            }
            let tt: std::collections::HashSet<u32> = terraintexture_hashes(&mut w, "terraintextures_P002_Q1").into_iter().collect();
            // RAW MTRL body dump (annotated) to reverse the terrainmesh record layout.
            if std::env::var("MERCS2_MTRL_DUMP").is_ok() {
                // Locate the MTRL chunk data span in the container.
                let dao = u32::from_le_bytes([container[4], container[5], container[6], container[7]]) as usize;
                let ndesc = u32::from_le_bytes([container[16], container[17], container[18], container[19]]) as usize;
                let mut mtrl: Option<(usize, usize)> = None;
                for d in 0..ndesc.min(6000) {
                    let dp = 20 + d * 20;
                    if dp + 20 > container.len() { break; }
                    if &container[dp..dp + 4] == b"MTRL" {
                        let off = u32::from_le_bytes([container[dp + 4], container[dp + 5], container[dp + 6], container[dp + 7]]);
                        let sz = u32::from_le_bytes([container[dp + 8], container[dp + 9], container[dp + 10], container[dp + 11]]) as usize;
                        if off != 0xFFFF_FFFF {
                            let base = if dao > 0 { dao + off as usize } else { 8 + off as usize };
                            mtrl = Some((base, sz));
                        }
                        break;
                    }
                }
                if let Some((mb, ms)) = mtrl {
                    println!("[mtrl-dump] MTRL body @{mb} size {ms}: annotated u32s (TT=terraintextures, M=A3CD72A7 marker, f=float):");
                    let end = (mb + ms).min(container.len());
                    let mut o = mb;
                    let mut idx = 0;
                    while o + 4 <= end && idx < 120 {
                        let u = u32::from_le_bytes([container[o], container[o + 1], container[o + 2], container[o + 3]]);
                        let f = f32::from_le_bytes([container[o], container[o + 1], container[o + 2], container[o + 3]]);
                        let tag = if u == 0xA3CD72A7 { "  <M".to_string() }
                            else if tt.contains(&u) { "  <TT".to_string() }
                            else if u <= 16 { format!("  int={u}") }
                            else if f.abs() > 1e-6 && f.abs() < 1e6 { format!("  f={f:.3}") }
                            else { String::new() };
                        println!("[mtrl-dump]   +{:04}: 0x{u:08X}{tag}", o - mb);
                        o += 4;
                        idx += 1;
                    }
                }
            }
            // Re-locate the MTRL chunk raw bytes and scan for tt hashes.
            let dec2 = wad::decompress_block_index(&mut w, bi).unwrap_or_default();
            let hits: Vec<(usize, u32)> = {
                let mut v = Vec::new();
                let mut i = s0;
                while i + 4 <= (s1).min(dec2.len()) {
                    let h = u32::from_le_bytes([dec2[i], dec2[i + 1], dec2[i + 2], dec2[i + 3]]);
                    if tt.contains(&h) { v.push((i - s0, h)); }
                    i += 1;
                }
                v
            };
            println!("[terrainmesh]   terraintextures hashes present in this terrainmesh chunk: {} {:08X?}", hits.len(), hits.iter().take(8).map(|(_, h)| h).collect::<Vec<_>>());
            // Reverse draw->material: collect each MTRL record's ID (first u32) + its layer textures
            // via the standard stride (116 + tex_count*4), then check the PRMT refs against those IDs.
            {
                let dao = u32::from_le_bytes([container[4], container[5], container[6], container[7]]) as usize;
                let ndesc = u32::from_le_bytes([container[16], container[17], container[18], container[19]]) as usize;
                let mut mbody: Option<(usize, usize)> = None;
                for d in 0..ndesc.min(6000) {
                    let dp = 20 + d * 20;
                    if dp + 4 > container.len() { break; }
                    if &container[dp..dp + 4] == b"MTRL" {
                        let off = u32::from_le_bytes([container[dp + 4], container[dp + 5], container[dp + 6], container[dp + 7]]);
                        let sz = u32::from_le_bytes([container[dp + 8], container[dp + 9], container[dp + 10], container[dp + 11]]) as usize;
                        if off != 0xFFFF_FFFF { mbody = Some((if dao > 0 { dao + off as usize } else { 8 + off as usize }, sz)); }
                        break;
                    }
                }
                if let Some((mb, ms)) = mbody {
                    let g = |o: usize| u32::from_le_bytes([container[o], container[o + 1], container[o + 2], container[o + 3]]);
                    let mut ids: Vec<u32> = Vec::new();
                    let mut p = mb;
                    let end = (mb + ms).min(container.len());
                    while p + 108 <= end {
                        let id = g(p);
                        let cnt = u16::from_le_bytes([container[p + 106], container[p + 107]]) as usize;
                        if cnt == 0 || cnt > 12 { break; }
                        if p + 108 + cnt * 4 > end { break; }
                        ids.push(id);
                        p += 116 + cnt * 4;
                    }
                    let id_set: std::collections::HashSet<u32> = ids.iter().copied().collect();
                    let gmi = mercs2_formats::texture::group_material_indices(container);
                    let refs: Vec<u32> = gmi.iter().map(|&r| r as u32).collect();
                    let matches = refs.iter().filter(|r| id_set.contains(r)).count();
                    let matches_rev = refs.iter().filter(|r| id_set.contains(&r.swap_bytes())).count();
                    let as_index = refs.iter().filter(|&&r| (r as usize) < ids.len()).count();
                    println!("[terrainmesh]   REVERSE: {} MTRL records parsed (ids), {} draws; PRMT-ref matches material id: {}/{}, byte-rev: {}/{}, valid-index(<{}): {}", ids.len(), refs.len(), matches, refs.len(), matches_rev, refs.len(), ids.len(), as_index);
                    println!("[terrainmesh]     first material ids: {:08X?}", &ids.iter().take(6).collect::<Vec<_>>());
                    println!("[terrainmesh]     first PRMT refs:    {:08X?}", &refs.iter().take(6).collect::<Vec<_>>());
                    // PRMG group INFO leaf -> material binding? Dump each group's first INFO (before
                    // STRM) as u32s + flag any field that is a valid material index (<n_records) or id.
                    if std::env::var("MERCS2_MTRL_DUMP").is_ok() {
                        let nrec = ids.len();
                        let marker = |dp: usize| g(dp + 4) == 0xFFFF_FFFF;
                        let mut gi = 0;
                        let mut d = 0usize;
                        while d < ndesc && gi < 6 {
                            let dp = 20 + d * 20;
                            if dp + 20 > container.len() { break; }
                            if &container[dp..dp + 4] == b"PRMG" && marker(dp) {
                                // find first INFO leaf before a STRM/IBUF marker in this group
                                let mut j = d + 1;
                                let mut info: Option<usize> = None;
                                while j < ndesc {
                                    let jp = 20 + j * 20;
                                    if jp + 20 > container.len() { break; }
                                    let t = &container[jp..jp + 4];
                                    if (t == b"PRMG") && marker(jp) { break; }
                                    if (t == b"STRM" || t == b"IBUF") && marker(jp) { break; }
                                    if t == b"INFO" && !marker(jp) { info = Some(jp); break; }
                                    j += 1;
                                }
                                if let Some(jp) = info {
                                    let off = g(jp + 4) as usize;
                                    let sz = g(jp + 8) as usize;
                                    let base = if dao > 0 { dao + off } else { 8 + off };
                                    let n = (sz / 4).min(12);
                                    let mut fields = Vec::new();
                                    for r in 0..n {
                                        let o = base + r * 4;
                                        if o + 4 > container.len() { break; }
                                        let val = g(o);
                                        let asidx = (val as usize) < nrec;
                                        let asid = ids.iter().position(|&x| x == val).is_some();
                                        fields.push(format!("{val:08X}{}{}", if asidx { "<idx" } else { "" }, if asid { "<ID" } else { "" }));
                                    }
                                    println!("[terrainmesh]     grp{gi} INFO({sz}B): {}", fields.join(" "));
                                }
                                gi += 1;
                            }
                            d += 1;
                        }
                    }
                    // Where do the recurring PRMT hashes live? (Are they material NAME fields in MTRL?)
                    if std::env::var("MERCS2_MTRL_DUMP").is_ok() {
                        for want in [0x16E4944Bu32, 0xDC351FCB, 0x1E3E7DD4] {
                            let mut locs = Vec::new();
                            let mut o = 0usize;
                            while o + 4 <= container.len() {
                                if g(o) == want {
                                    let in_mtrl = o >= mb && o < mb + ms;
                                    // offset within the nearest preceding material record start
                                    let rel = if in_mtrl {
                                        let mut rp = mb; let mut prev = mb;
                                        while rp + 108 <= o { prev = rp; let c = u16::from_le_bytes([container[rp+106],container[rp+107]]) as usize; if c==0||c>12 {break;} rp += 116 + c*4; }
                                        Some(o - prev)
                                    } else { None };
                                    locs.push((o, in_mtrl, rel));
                                }
                                o += 4;
                            }
                            println!("[terrainmesh]     hash 0x{want:08X}: {} occurrences; first {:?}", locs.len(), locs.iter().take(4).map(|(o,m,r)| (o,m,r)).collect::<Vec<_>>());
                        }
                    }
                    // Dump full 16-byte PRMT records for the first 3 PRMG groups (all 4 u32 fields).
                    if std::env::var("MERCS2_MTRL_DUMP").is_ok() {
                        let mut gi = 0;
                        for d in 0..ndesc.min(6000) {
                            let dp = 20 + d * 20;
                            if dp + 20 > container.len() { break; }
                            let is_marker = u32::from_le_bytes([container[dp + 4], container[dp + 5], container[dp + 6], container[dp + 7]]) == 0xFFFF_FFFF;
                            if &container[dp..dp + 4] == b"PRMT" && !is_marker {
                                let off = u32::from_le_bytes([container[dp + 4], container[dp + 5], container[dp + 6], container[dp + 7]]) as usize;
                                let sz = u32::from_le_bytes([container[dp + 8], container[dp + 9], container[dp + 10], container[dp + 11]]) as usize;
                                let base = if dao > 0 { dao + off } else { 8 + off };
                                let nrec = sz / 16;
                                for r in 0..nrec.min(3) {
                                    let o = base + r * 16;
                                    if o + 16 > container.len() { break; }
                                    let idx_of = |h: u32| ids.iter().position(|&x| x == h);
                                    println!("[terrainmesh]     PRMT[grp?]: [{:08X} {:08X} {:08X} {:08X}] (field->matID idx: {:?} {:?} {:?} {:?})",
                                        g(o), g(o+4), g(o+8), g(o+12), idx_of(g(o)), idx_of(g(o+4)), idx_of(g(o+8)), idx_of(g(o+12)));
                                }
                                gi += 1;
                                if gi >= 4 { break; }
                            }
                        }
                    }
                }
            }
            // Per-draw splat layers (the reversed model): each group's material -> detail layers.
            {
                let layers = mercs2_formats::texture::terrain_group_layers(container);
                let midx = mercs2_formats::texture::terrain_group_material_index(container);
                let all: std::collections::HashSet<u32> = layers.iter().flatten().copied().collect();
                let mut resolvable = 0usize;
                for &h in &all {
                    if wad::extract_texture(&mut w, h).is_ok() {
                        resolvable += 1;
                    }
                }
                let counts: Vec<usize> = layers.iter().map(|l| l.len()).collect();
                println!(
                    "[terrainmesh]   SPLAT LAYERS: {} draws, material idx (first 8) {:?}; layers/draw (first 8) {:?}; {} distinct layer textures, {resolvable} resolve",
                    layers.len(), &midx.iter().take(8).collect::<Vec<_>>(), &counts.iter().take(8).collect::<Vec<_>>(), all.len()
                );
                for l in layers.iter().take(4) {
                    println!("[terrainmesh]     draw layers: {:08X?}", l);
                }
            }
            // Vertex COLOR (splat weights)?
            if let Ok(meshes) = mercs2_formats::model_cubeize::read_model_meshes(container) {
                let with_col = meshes.iter().filter(|m| !m.colors.is_empty()).count();
                let mut distinct: std::collections::HashSet<[u8; 4]> = std::collections::HashSet::new();
                let mut sample = Vec::new();
                for m in &meshes {
                    for c in &m.colors {
                        distinct.insert(*c);
                        if sample.len() < 8 { sample.push(*c); }
                    }
                }
                println!("[terrainmesh]   vertex COLOR (splat weights): {}/{} groups carry it; {} distinct values; sample {:?}", with_col, meshes.len(), distinct.len(), sample);
            }
        }
        match mesh::build_indexed_from_container(&dec[s0..s1]) {
            Ok((verts, indices, draws, stats)) => {
                let cell = c3_cell_id_from_path(&path);
                let cc = cell.map(c3_cell_centre);
                println!(
                    "[terrainmesh] block={bi} {path}: {} verts / {} tris / {} draws | bbox X[{:.1},{:.1}] Y[{:.1},{:.1}] Z[{:.1},{:.1}]",
                    verts.len(), indices.len() / 3, draws.len(),
                    stats.bbox_min[0], stats.bbox_max[0], stats.bbox_min[1], stats.bbox_max[1], stats.bbox_min[2], stats.bbox_max[2]
                );
                let _ = cc;
                // REAL placement from the TerrainObject->Transform map (not the c3 cell-id).
                match tile_pos.get(&tm_hash) {
                    Some(&p) => {
                        // Terrainmesh verts are local (POFF-collapsed here); Transform gives world XZ.
                        // Test Y: low-res ground at the tile position should fall within the
                        // terrainmesh Y range (Y is world-absolute) once placed at (pos.x, pos.z).
                        let lo = hmap.height_at(p[0], p[2]);
                        let straddles = lo.is_finite() && lo >= stats.bbox_min[1] - 8.0 && lo <= stats.bbox_max[1] + 8.0;
                        println!(
                            "[terrainmesh]   TerrainObject pos=({:.0},{:.0},{:.0}); low-res ground there={lo:.1}; terrainmesh Y[{:.1},{:.1}] -> {}",
                            p[0], p[1], p[2], stats.bbox_min[1], stats.bbox_max[1],
                            if straddles { "MATCH (placement correct, Y world-absolute)" } else { "MISMATCH" }
                        );
                    }
                    None => println!("[terrainmesh]   mesh 0x{tm_hash:08X} not in TerrainObject map"),
                }
                // Material/diffuse hashes = the splat set.
                let diffuse: Vec<u32> = draws.iter().filter_map(|d| d.diffuse).collect();
                println!("[terrainmesh]   {} draws carry a diffuse texture (multi-material = the splat)", diffuse.len());
                // POFF (Position OFFset) chunks — the suspected per-GEOM world anchor the builder
                // ignores. Walk the UCFX descriptor table of the terrainmesh chunk and read each
                // POFF's 3 floats; compare to the grid cell centre.
                let c = &dec[s0..s1];
                if c.len() > 20 && &c[0..4] == b"UCFX" {
                    let dao = u32::from_le_bytes([c[4], c[5], c[6], c[7]]) as usize;
                    let ndesc = u32::from_le_bytes([c[16], c[17], c[18], c[19]]) as usize;
                    let mut poffs: Vec<[f32; 3]> = Vec::new();
                    for d in 0..ndesc.min(4000) {
                        let dp = 20 + d * 20;
                        if dp + 20 > c.len() {
                            break;
                        }
                        if &c[dp..dp + 4] == b"POFF" {
                            let off = u32::from_le_bytes([c[dp + 4], c[dp + 5], c[dp + 6], c[dp + 7]]) as usize;
                            let base = dao + off;
                            if base + 12 <= c.len() {
                                let f = |o: usize| f32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]);
                                poffs.push([f(base), f(base + 4), f(base + 8)]);
                            }
                        }
                    }
                    let uniq: std::collections::HashSet<[u32; 3]> =
                        poffs.iter().map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]).collect();
                    println!("[terrainmesh]   POFF chunks: {} ({} distinct)", poffs.len(), uniq.len());
                    for p in poffs.iter().take(4) {
                        println!("[terrainmesh]     POFF = ({:.2}, {:.2}, {:.2})", p[0], p[1], p[2]);
                    }
                }
            }
            Err(e) => println!("[terrainmesh] block={bi} {path}: build failed: {e}"),
        }
    }
    Ok(())
}

/// Terrain-consumer hunt: which blocks reference the 30 `terraintextures` material hashes? (The
/// hi-res terrain path — proven absent from low_res_terrain/layers_static.) Scans every C3Cell
/// geometry block by raw LE-u32 hash match and reports hits (block name + which materials).
pub fn terrain_consumer_scan(wadpath: &str) -> Result<(), String> {
    use mercs2_formats::world_index::BlockClass;
    let mut w = wad::open(wadpath)?;
    let tt: Vec<u32> = terraintexture_hashes(&mut w, "terraintextures_P002_Q1");
    let tt_set: std::collections::HashSet<u32> = tt.iter().copied().collect();
    if tt_set.is_empty() {
        return Err("no terraintextures hashes found".into());
    }
    println!("[terrain-consumer] scanning for {} terraintextures material hashes", tt_set.len());

    let idx = {
        let (archive, file) = wad::archive_and_file(&mut w);
        mercs2_formats::world_index::WorldIndex::build(archive, file)
    };
    let c3_geom: Vec<u16> = idx
        .by_class(BlockClass::C3Cell)
        .filter(|b| b.has_model_geometry)
        .map(|b| b.block_index)
        .collect();
    println!("[terrain-consumer] candidate C3Cell geometry blocks: {}", c3_geom.len());

    let mut hit_blocks = 0usize;
    let mut total_refs = 0usize;
    for (n, &bi) in c3_geom.iter().enumerate() {
        let Ok(dec) = wad::decompress_block_index(&mut w, bi) else { continue };
        let mut found: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut i = 0usize;
        while i + 4 <= dec.len() {
            let v = u32::from_le_bytes([dec[i], dec[i + 1], dec[i + 2], dec[i + 3]]);
            if tt_set.contains(&v) {
                found.insert(v);
            }
            i += 1;
        }
        if !found.is_empty() {
            hit_blocks += 1;
            total_refs += found.len();
            if hit_blocks <= 20 {
                let path = wad::block_paths(&w).get(bi as usize).cloned().unwrap_or_default();
                println!("[terrain-consumer]   HIT block={bi} {path} — {} materials", found.len());
            }
        }
        if n % 400 == 399 {
            eprintln!("[terrain-consumer]   ...scanned {}/{}", n + 1, c3_geom.len());
        }
    }
    println!(
        "[terrain-consumer] RESULT: {hit_blocks}/{} c3 geometry blocks reference terraintextures ({total_refs} total refs)",
        c3_geom.len()
    );
    if hit_blocks == 0 {
        println!("[terrain-consumer] -> terraintextures NOT consumed by c3 geometry blocks; the hi-res terrain path is elsewhere (candidates: a resident material-def block, or TerrainKey/FUN_004a88a0 runtime).");
    }
    Ok(())
}

/// Stage-1 terrain splat/LOD RCA: per-tile MTRL materials + the `@12` per-vertex scalar,
/// cross-checked against the `terraintextures` material set. Headless; prints verifiable numbers.
pub fn terrain_probe(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let (low, ls) = find_terrain_blocks(&mut w)?;

    // The terrain-material set = terraintextures_P002_Q1 (the finest resident rung, 30 materials).
    let tt_set: std::collections::HashSet<u32> =
        terraintexture_hashes(&mut w, "terraintextures_P002_Q1").into_iter().collect();
    let detail = [
        ("mountain01", terraintexture_hashes(&mut w, "tt_mountain01_P003_Q0")),
        ("rock", terraintexture_hashes(&mut w, "tt_rock_P003_Q0")),
        ("pmcgrass02", terraintexture_hashes(&mut w, "tt_pmcgrass02_P003_Q0")),
    ];
    println!("[terrain-probe] terraintextures_P002_Q1 material set: {} hashes", tt_set.len());
    for (name, hs) in &detail {
        let in_set = hs.iter().all(|h| tt_set.contains(h));
        println!(
            "[terrain-probe]   detail tt_{name}: {} hash(es) {:08X?} -> in P002 set: {in_set}",
            hs.len(), hs
        );
    }

    // Where else might the terraintextures splat be authored? Scan the terrain blocks' raw bytes
    // for any of the 30 material hashes (LE u32), and surface splat/control-like block names.
    let scan_hashes = |buf: &[u8], set: &std::collections::HashSet<u32>| -> usize {
        let mut hits = std::collections::HashSet::new();
        let mut i = 0;
        while i + 4 <= buf.len() {
            let h = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
            if set.contains(&h) {
                hits.insert(h);
            }
            i += 1;
        }
        hits.len()
    };
    println!(
        "[terrain-probe] terraintextures-hash presence (raw LE u32 scan): low_res_terrain={} / {} distinct, layers_static={} / {}",
        scan_hashes(&low, &tt_set), tt_set.len(), scan_hashes(&ls, &tt_set), tt_set.len()
    );
    let splat_names = ["splat", "blend", "control", "heightfield", "hfield", "tt_", "terrainkey"];
    let mut cand = Vec::new();
    for (i, p) in wad::block_paths(&w).iter().enumerate() {
        let pl = p.to_lowercase();
        if splat_names.iter().any(|n| pl.contains(n)) {
            cand.push((i, p.clone()));
        }
    }
    println!("[terrain-probe] splat/control-like block names ({}):", cand.len());
    for (i, p) in cand.iter().take(20) {
        println!("[terrain-probe]     block={i} {p}");
    }

    let probes = mercs2_formats::terrain::probe_terrain(&low);
    let tiles = probes.len();
    let with_mtrl = probes.iter().filter(|p| !p.materials.is_empty()).count();

    // Materials-per-tile histogram + membership in the terraintextures set.
    let mut per_tile_hist: std::collections::BTreeMap<usize, usize> = Default::default();
    let mut distinct: std::collections::HashSet<u32> = Default::default();
    let mut refs_total = 0usize;
    let mut refs_in_set = 0usize;
    for p in &probes {
        *per_tile_hist.entry(p.materials.len()).or_default() += 1;
        for &h in &p.materials {
            distinct.insert(h);
            refs_total += 1;
            if tt_set.contains(&h) {
                refs_in_set += 1;
            }
        }
    }
    let distinct_in_set = distinct.iter().filter(|h| tt_set.contains(h)).count();

    println!("[terrain-probe] tiles decoded: {tiles}; tiles with a parsed MTRL: {with_mtrl}");
    println!("[terrain-probe] materials-per-tile histogram (mat_count -> tiles):");
    for (k, v) in &per_tile_hist {
        println!("[terrain-probe]     {k} materials -> {v} tiles");
    }
    println!(
        "[terrain-probe] material refs: {refs_total} total, {refs_in_set} in terraintextures set \
         ({} distinct hashes, {distinct_in_set} of them in-set)",
        distinct.len()
    );
    // List distinct referenced hashes NOT in the terraintextures set (should be none if the
    // hypothesis holds).
    let strays: Vec<u32> = distinct.iter().copied().filter(|h| !tt_set.contains(h)).collect();
    if strays.is_empty() {
        println!("[terrain-probe]   -> ALL referenced material hashes are members of the terraintextures set");
    } else {
        println!("[terrain-probe]   -> {} referenced hashes NOT in set: {:08X?}", strays.len(), strays);
    }
    // Is the single referenced hash actually the baked composite atlas (vz_lrterrain)?
    let atlas_hash = mercs2_formats::hash::pandemic_hash_m2("vz_lrterrain");
    println!(
        "[terrain-probe]   pandemic_hash_m2(\"vz_lrterrain\") = 0x{atlas_hash:08X}; \
         referenced-by-tiles = {:08X?} (match: {})",
        distinct, distinct.contains(&atlas_hash)
    );

    // The @12 per-vertex scalar.
    let (mut gmin, mut gmax, mut gsum, mut gn) = (f32::INFINITY, f32::NEG_INFINITY, 0.0f64, 0usize);
    let mut in01_verts = 0usize;
    let mut total_verts = 0usize;
    let mut lane6_ok = 0usize;
    let mut lane14_ok = 0usize;
    let mut const_tiles = 0usize; // tiles whose @12 min==max (constant per tile)
    let mut unit_normal_verts = 0usize;
    for p in &probes {
        gmin = gmin.min(p.w12.0);
        gmax = gmax.max(p.w12.1);
        gsum += p.w12.2 as f64 * p.verts as f64;
        gn += p.verts;
        in01_verts += p.w12_in01;
        total_verts += p.verts;
        unit_normal_verts += p.unit_normal_verts;
        if p.lane6_all_one {
            lane6_ok += 1;
        }
        if p.lane14_all_one {
            lane14_ok += 1;
        }
        if (p.w12.1 - p.w12.0).abs() < 1e-4 {
            const_tiles += 1;
        }
    }
    println!("[terrain-probe] @12 scalar across {total_verts} verts / {tiles} tiles:");
    println!(
        "[terrain-probe]     range [{gmin:.5}, {gmax:.5}] mean {:.5}; in [0,1]: {in01_verts}/{total_verts} ({:.1}%)",
        if gn > 0 { gsum / gn as f64 } else { 0.0 },
        100.0 * in01_verts as f32 / total_verts.max(1) as f32
    );
    println!(
        "[terrain-probe]     constant-per-tile (@12 min==max): {const_tiles}/{tiles} tiles; \
         lane@6==1.0 all: {lane6_ok}/{tiles}; lane@14==1.0 all: {lane14_ok}/{tiles}"
    );
    println!(
        "[terrain-probe] NORMAL test: vertices where (f16@8,@10,@12) is unit-length (|len-1|<0.03): \
         {unit_normal_verts}/{total_verts} ({:.1}%)",
        100.0 * unit_normal_verts as f32 / total_verts.max(1) as f32
    );
    Ok(())
}

/// Enumerate + measure every model in the WAD (headless); flag likely humanoids by bbox.
pub fn wad_list(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    eprintln!("{} model assets in {wadpath}", models.len());
    let (mut ok, mut human) = (0u32, 0u32);
    for (hash, block) in &models {
        let container = match wad::extract_container(&mut w, *hash) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok((_v, s)) = mesh::build_from_container(&container) {
            let yh = s.bbox_max[1] - s.bbox_min[1];
            let xw = s.bbox_max[0] - s.bbox_min[0];
            let humanoid = (1.4..2.3).contains(&yh) && xw < 1.6 && s.vertices > 800;
            println!(
                "0x{hash:08X} block={block:<5} meshes={:<3} verts={:<6} yheight={yh:6.2} xwidth={xw:6.2}{}",
                s.meshes,
                s.vertices,
                if humanoid { "  <-- humanoid?" } else { "" }
            );
            ok += 1;
            if humanoid {
                human += 1;
            }
        }
    }
    eprintln!("measured {ok} models; {human} look humanoid");
    Ok(())
}

/// Skinning-convention diagnostic (headless). CPU-skins the mesh at frame 0 under several bone-matrix
/// variants and reports each resulting bbox vs the known-good BIND pose. The variant whose extent +
/// centroid match the bind pose reveals the correct rotation/root convention — measured, not guessed.
pub fn animdiag(wadpath: &str, model: Option<String>, index: Option<String>) -> Result<(), String> {
    use mercs2_formats::skeleton::transform_point;

    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, _i, _d, s) = mesh::build_indexed_from_container(&container)?;
    if s.rig.is_empty() {
        return Err("model has no skeleton".into());
    }
    let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();
    // Find the animgroup whose clips best cover this HIER, and inspect EVERY clip — to reveal whether
    // the spikes are clip-specific (e.g. the full-body/additive clip) or universal.
    use mercs2_formats::animgroup::parse_animgroup;
    let mut best_blk: Option<(u16, usize)> = None;
    for blk in wad::animgroup_blocks(&w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        let total: usize = ag
            .clips
            .iter()
            .map(|c| c.binding.resolve_to_hier(&hier).iter().filter(|r| r.is_some()).count())
            .sum();
        if best_blk.map_or(true, |(_, t)| total > t) {
            best_blk = Some((blk, total));
        }
    }
    let (blk, _) = best_blk.ok_or("no animgroup matched this model")?;
    let data = wad::decompress_block_index(&mut w, blk)?;
    let ag = parse_animgroup(&data).map_err(|e| format!("parse animgroup: {e}"))?;

    // CPU-skin bbox extent for a palette (mirrors the shader LBS).
    let extent_of = |pal: &[[[f32; 4]; 4]]| -> f32 {
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for v in &verts {
            let wsum: f32 = v.weights.iter().map(|&x| x as f32).sum::<f32>().max(1.0);
            let mut acc = [0.0f32; 3];
            for k in 0..4 {
                let wk = v.weights[k] as f32 / wsum;
                if wk <= 0.0 { continue; }
                let b = v.joints[k] as usize;
                if b >= pal.len() { continue; }
                let p = transform_point(&pal[b], v.pos);
                for j in 0..3 { acc[j] += wk * p[j]; }
            }
            for j in 0..3 { lo[j] = lo[j].min(acc[j]); hi[j] = hi[j].max(acc[j]); }
        }
        (0..3).map(|j| hi[j] - lo[j]).fold(0.0f32, f32::max)
    };
    let bind_extent = extent_of(&pose::palette(&s.rig, &pose::bind_locals(&s.rig)));
    println!(
        "model 0x{hash:08X}: {} bones, {} verts; animgroup block[{blk}], {} clips; BIND extent {bind_extent:.3}",
        s.rig.len(), verts.len(), ag.clips.len()
    );

    // Rotation-driven locals for a clip sample (matches shipping animate_locals): xyzw absolute
    // rotation, rigid bind offset, root at bind.
    let times = [0.0f32, 0.12, 0.25, 0.37, 0.5, 0.62, 0.75, 0.87];
    let locals_at = |ac: &mercs2_formats::anim::AnimClip, tth: &[Option<usize>], t: f32| -> Vec<[[f32; 4]; 4]> {
        let sample = ac.sample_local(t * ac.duration.max(1e-3));
        let mut locals = pose::bind_locals(&s.rig);
        for (track, bone) in tth.iter().enumerate() {
            if let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) {
                if b >= locals.len() || s.rig[b].parent < 0 { continue; }
                let mut m = pose::qs_to_local(qs);
                let lb = s.rig[b].local_bind;
                m[3] = [lb[3][0], lb[3][1], lb[3][2], 1.0];
                locals[b] = m;
            }
        }
        locals
    };

    println!("  per-clip max bbox extent (want ~{bind_extent:.2}; >1.4x = spikes):");
    for c in &ag.clips {
        let Ok(ac) = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]) else { continue };
        if !ac.decoded { continue; }
        let tth = c.binding.resolve_to_hier(&hier);
        let resolved = tth.iter().filter(|r| r.is_some()).count();
        let mut max_e = 0.0f32;
        for &t in &times {
            let e = extent_of(&pose::palette(&s.rig, &locals_at(&ac, &tth, t)));
            if e > max_e { max_e = e; }
        }
        let tag = if max_e < 1.4 * bind_extent { "  <- CLEAN" } else { "" };
        println!(
            "    clip 0x{:08X}  {:>3}t {:>3}res  {:>5.2}s  max extent={max_e:.3}{tag}",
            c.name_hash, ac.num_tracks, resolved, ac.duration
        );
    }
    Ok(())
}

/// Animation coordinate-consistency gate (headless). Retail ships no referencePose, so clip local
/// transforms must be authored in the SAME frame as the mesh HIER bind locals. Decisive check: for
/// every bone a track drives, the animated LOCAL translation (bone offset from parent) must equal the
/// HIER bind-local translation — bones rotate but don't stretch. A near-zero delta proves the clip
/// data drops straight into `pose::palette` with no coordinate conversion; a large/negated delta
/// would reveal a handedness fix is needed. Finds the animgroup whose binding best matches this model.
pub fn animcheck(wadpath: &str, model: Option<String>, index: Option<String>) -> Result<(), String> {
    use mercs2_formats::animgroup::parse_animgroup;

    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (_v, _i, _d, s) = mesh::build_indexed_from_container(&container)?;
    if s.rig.is_empty() {
        return Err("model has no skeleton (rig empty)".into());
    }
    let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();

    // Pass 1: pick the animgroup + clip whose binding resolves the most tracks onto this HIER.
    let mut best: Option<(u16, u32, usize)> = None; // (block, clip name_hash, resolved count)
    for blk in wad::animgroup_blocks(&w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        for c in &ag.clips {
            let resolved = c
                .binding
                .resolve_to_hier(&hier)
                .iter()
                .filter(|r| r.is_some())
                .count();
            if best.map_or(true, |(_, _, r)| resolved > r) {
                best = Some((blk, c.name_hash, resolved));
            }
        }
    }
    let (blk, clip_hash, resolved) = best.ok_or("no animgroup binding matched this model")?;
    println!("model 0x{hash:08X}: {} bones; best animgroup block[{blk}] clip 0x{clip_hash:08X} ({resolved} tracks resolve to HIER)", s.rig.len());

    // Pass 2: decode that clip and compare its frame-0 local translations to the HIER bind locals.
    let data = wad::decompress_block_index(&mut w, blk)?;
    let ag = parse_animgroup(&data).map_err(|e| format!("parse animgroup: {e}"))?;
    let clip = ag
        .clips
        .iter()
        .find(|c| c.name_hash == clip_hash)
        .ok_or("clip vanished on re-parse")?;
    let pk = &data[clip.havok_offset..];
    let ac = mercs2_formats::anim::parse_anim(pk).map_err(|e| format!("parse_anim: {e}"))?;
    println!(
        "clip: class={} decoded={} tracks={} frames={} duration={:.3}",
        clip.class, ac.decoded, ac.num_tracks, ac.num_frames, ac.duration
    );
    if !ac.decoded {
        return Err(format!("clip not decoded (class {}) — cannot check", clip.class));
    }

    let track_to_hier = clip.binding.resolve_to_hier(&hier);
    let sample = ac.sample_local(0.0);

    let (mut n, mut sum_d, mut max_d, mut sum_off) = (0u32, 0.0f32, 0.0f32, 0.0f32);
    let mut worst: Vec<(usize, f32)> = Vec::new();
    for (track, bone) in track_to_hier.iter().enumerate() {
        let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) else { continue };
        if s.rig[b].parent < 0 {
            continue; // root translation is motion, not a fixed bone offset
        }
        let at = qs.translation;
        let bt = [s.rig[b].local_bind[3][0], s.rig[b].local_bind[3][1], s.rig[b].local_bind[3][2]];
        let d = ((at[0] - bt[0]).powi(2) + (at[1] - bt[1]).powi(2) + (at[2] - bt[2]).powi(2)).sqrt();
        let off = (bt[0] * bt[0] + bt[1] * bt[1] + bt[2] * bt[2]).sqrt();
        n += 1;
        sum_d += d;
        sum_off += off;
        if d > max_d {
            max_d = d;
        }
        worst.push((b, d));
    }
    if n == 0 {
        return Err("no non-root driven bones to compare".into());
    }
    // Hypothesis test: the decoded wavelet tracks may be offset by one relative to the trnm binding.
    // Compare sample[track] against the bone that binding[track+1] names, and against [track-1].
    let mut shift_next = (0u32, 0.0f32); // sample[N] vs bone(binding[N+1])
    let mut shift_prev = (0u32, 0.0f32); // sample[N] vs bone(binding[N-1])
    for (track, qs) in sample.iter().enumerate() {
        for (delta, acc) in [(1i32, &mut shift_next), (-1i32, &mut shift_prev)] {
            let j = track as i32 + delta;
            if j < 0 || j as usize >= track_to_hier.len() {
                continue;
            }
            let Some(&b) = track_to_hier[j as usize].as_ref() else { continue };
            if s.rig[b].parent < 0 {
                continue;
            }
            let bt = [s.rig[b].local_bind[3][0], s.rig[b].local_bind[3][1], s.rig[b].local_bind[3][2]];
            let d = ((qs.translation[0] - bt[0]).powi(2)
                + (qs.translation[1] - bt[1]).powi(2)
                + (qs.translation[2] - bt[2]).powi(2))
            .sqrt();
            acc.0 += 1;
            acc.1 += d;
        }
    }

    let mean_d = sum_d / n as f32;
    let mean_off = sum_off / n as f32;
    if shift_next.0 > 0 {
        println!(
            "  SHIFT TEST: aligned mean|Δ|={mean_d:.6}  |  sample[N]vs binding[N+1] mean|Δ|={:.6}  |  vs binding[N-1] mean|Δ|={:.6}",
            shift_next.1 / shift_next.0 as f32,
            shift_prev.1 / shift_prev.0.max(1) as f32
        );
    }
    worst.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!(
        "translation delta (anim local vs HIER bind local), {n} non-root driven bones:"
    );
    println!("  mean |Δ| = {mean_d:.6}   max |Δ| = {max_d:.6}   (mean bone offset = {mean_off:.4})");
    // Correctness gate = BINDING ALIGNMENT, not bind-equality: the animation is authored in the
    // HIER frame iff the aligned translation delta is clearly the smallest of {N-1, N, N+1}. (A
    // straight rel<threshold gate is confounded — a clip genuinely moves some bones in frame 0, so
    // aligned |Δ| is never zero; but a one-track misbinding makes a neighbour shift fit better.)
    let d_next = shift_next.1 / shift_next.0.max(1) as f32;
    let d_prev = shift_prev.1 / shift_prev.0.max(1) as f32;
    let aligned_best = mean_d < 0.7 * d_next && mean_d < 0.7 * d_prev;
    println!(
        "  aligned mean Δ = {mean_d:.4} vs shift±1 [{d_prev:.4}, {d_next:.4}]  ->  {}",
        if aligned_best {
            "GATE PASS — track↔bone binding is aligned (clip authored in HIER frame)"
        } else {
            "GATE FAIL — a neighbouring shift fits better; binding is misaligned"
        }
    );
    print!("  worst bones (Δ):");
    for (b, d) in worst.iter().take(4) {
        print!(" bone{b}={d:.4}");
    }
    println!();

    // Raw side-by-side dump (anim frame-0 local T/R vs HIER bind-local T) to reveal the relationship
    // (rotation-only? scaled? negated component? mapping off?) without guessing.
    println!("  --- raw anim-vs-bind for first 6 driven non-root bones ---");
    let mut shown = 0;
    for (track, bone) in track_to_hier.iter().enumerate() {
        let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) else { continue };
        if s.rig[b].parent < 0 {
            continue;
        }
        let bt = [s.rig[b].local_bind[3][0], s.rig[b].local_bind[3][1], s.rig[b].local_bind[3][2]];
        println!(
            "    track{track:>3}->bone{b:<3} animT=[{:+.4},{:+.4},{:+.4}] bindT=[{:+.4},{:+.4},{:+.4}] animR=[{:+.3},{:+.3},{:+.3},{:+.3}]",
            qs.translation[0], qs.translation[1], qs.translation[2],
            bt[0], bt[1], bt[2],
            qs.rotation[0], qs.rotation[1], qs.rotation[2], qs.rotation[3]
        );
        shown += 1;
        if shown >= 6 {
            break;
        }
    }

    // Full render-path sanity: build the animated palette at mid-clip and confirm every Skin matrix
    // is finite and bounded (Skin translation = per-bone displacement from bind; a blow-up = NaN or
    // huge values). This exercises sample_local -> animate_locals -> palette exactly as render() does.
    let sample_mid = ac.sample_local(ac.duration * 0.5);
    let locals = pose::animate_locals(&s.rig, &sample_mid, &track_to_hier);
    let pal = pose::palette(&s.rig, &locals);
    let mut finite = true;
    let mut max_t = 0.0f32;
    for m in &pal {
        for row in m {
            for &v in row {
                if !v.is_finite() {
                    finite = false;
                }
            }
        }
        let t = (m[3][0].powi(2) + m[3][1].powi(2) + m[3][2].powi(2)).sqrt();
        max_t = max_t.max(t);
    }
    let extent = (0..3).map(|k| s.bbox_max[k] - s.bbox_min[k]).fold(0.0f32, f32::max);
    println!(
        "animated palette @{:.2}s: finite={finite}  max|Skin T|={max_t:.3}  (model extent ~{extent:.2})  ->  {}",
        ac.duration * 0.5,
        if finite && max_t < 4.0 * extent.max(0.25) {
            "SANE (render path bounded)"
        } else {
            "UNSTABLE — investigate before rendering"
        }
    );
    Ok(())
}

/// Per-track binding audit (headless). For `--clip <hash>` on this model, prints — for EVERY
/// animgroup block containing that clip — the raw `trnm` words read back from the block bytes
/// (count, leading word, size check), the Havok header track counts, and a per-track table:
/// track index, raw binding name-hash, resolved HIER bone index (+ name/parent/bind position),
/// or UNRESOLVED. Also lists HIER bones driven by no track and bones driven by more than one.
pub fn trackmap(wadpath: &str, model: Option<String>, index: Option<String>, want: Option<u32>) -> Result<(), String> {
    use mercs2_formats::animgroup::parse_animgroup;
    use mercs2_formats::skeleton::mat4_mul;
    use mercs2_formats::ucfx::parse_block_entry_table;

    let clip_hash = want.ok_or("--trackmap requires --clip <hash>")?;
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, _i, _d, s) = mesh::build_indexed_from_container(&container)?;
    if s.rig.is_empty() {
        return Err("model has no skeleton".into());
    }
    let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();

    // Bind-pose world position per bone (world = local · world_parent, row-vector).
    let mut world = vec![[[0.0f32; 4]; 4]; s.rig.len()];
    for b in 0..s.rig.len() {
        world[b] = if s.rig[b].parent < 0 {
            s.rig[b].local_bind
        } else {
            mat4_mul(&s.rig[b].local_bind, &world[s.rig[b].parent as usize])
        };
    }

    // Names for every HIER hash + every trnm hash we encounter (collected below in pass 1).
    let mut wanted: std::collections::BTreeSet<u32> = hier.iter().copied().collect();
    let mut hits: Vec<(u16, Vec<u32>)> = Vec::new(); // (block, trnm hashes)
    for blk in wad::animgroup_blocks(&w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        if let Some(c) = ag.clips.iter().find(|c| c.name_hash == clip_hash) {
            wanted.extend(c.binding.track_to_bone_hash.iter().copied());
            hits.push((blk, c.binding.track_to_bone_hash.clone()));
        }
    }
    if hits.is_empty() {
        return Err(format!("clip 0x{clip_hash:08X} not found in any animgroup"));
    }
    let names = rainbow_names(&wanted);
    let nm = |h: u32| names.get(&h).map(String::as_str).unwrap_or("?");

    println!("model 0x{hash:08X}: {} HIER bones", s.rig.len());
    for (b, bone) in s.rig.iter().enumerate() {
        println!(
            "  bone{b:<3} hash=0x{:08X} parent={:<3} bindpos=[{:+7.3},{:+7.3},{:+7.3}]  {}",
            bone.name_hash, bone.parent, world[b][3][0], world[b][3][1], world[b][3][2], nm(bone.name_hash)
        );
    }

    // QS bind-identity gate: recompose the palette through the hkQsTransform path with NO
    // tracks driven (exactly what havok_palette does to undriven bones). Every Skin matrix
    // must be identity; a deviation marks a local_bind that does not survive the
    // mat_to_qs -> qs_mul -> qs_to_local roundtrip (mirror/shear/non-TRS local).
    {
        let qs_pal = pose::havok_palette(&s.rig, &[], &[], 0);
        let mut bad: Vec<(usize, f32, f32)> = Vec::new(); // (bone, max|Skin-I|, det3)
        for (b, m) in qs_pal.iter().enumerate() {
            let mut dev = 0.0f32;
            for r in 0..4 {
                for c in 0..4 {
                    let ident = if r == c { 1.0 } else { 0.0 };
                    dev = dev.max((m[r][c] - ident).abs());
                }
            }
            if dev > 1e-3 {
                let lb = s.rig[b].local_bind;
                let det = lb[0][0] * (lb[1][1] * lb[2][2] - lb[1][2] * lb[2][1])
                    - lb[0][1] * (lb[1][0] * lb[2][2] - lb[1][2] * lb[2][0])
                    + lb[0][2] * (lb[1][0] * lb[2][1] - lb[1][1] * lb[2][0]);
                bad.push((b, dev, det));
            }
        }
        println!("QS bind-identity gate: {} / {} bones deviate > 1e-3 through the undriven-bone (bind_qs) path", bad.len(), s.rig.len());
        for (b, dev, det) in &bad {
            println!(
                "    bone{b:<3} hash=0x{:08X} parent={:<3} |Skin-I|={dev:.4} det(local_bind)={det:+.4}  {}",
                s.rig[*b].name_hash, s.rig[*b].parent, nm(s.rig[*b].name_hash)
            );
        }
    }

    // Vertex->joint plausibility per drawing group: a skinned vertex should sit near its dominant
    // bone's bind position. If a group's BLENDINDICES are NOT global HIER indices (e.g. a
    // per-group palette), its verts land far from the bones they claim — invisible at bind
    // (identity palette) but exploding under animation.
    {
        let meshes = mercs2_formats::model_cubeize::read_model_meshes(&container)
            .map_err(|e| format!("read_model_meshes: {e}"))?;
        let dist = |p: &[f32; 3], j: usize| -> f32 {
            let w = &world[j.min(world.len() - 1)];
            ((p[0] - w[3][0]).powi(2) + (p[1] - w[3][1]).powi(2) + (p[2] - w[3][2]).powi(2)).sqrt()
        };
        println!("vertex->joint distance per skinned group (dom = weight-dominant joint; min = best of the 4):");
        for m in &meshes {
            if m.rigid || m.joints.is_empty() {
                continue;
            }
            let (mut jmin, mut jmax) = (255u8, 0u8);
            let (mut sum_dom, mut sum_min, mut mx_min, mut nfar, mut n) = (0.0f32, 0.0f32, 0.0f32, 0usize, 0usize);
            for (vi, p) in m.positions.iter().enumerate() {
                let (Some(j4), Some(w4)) = (m.joints.get(vi), m.weights.get(vi)) else { continue };
                let wi = w4.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
                for k in 0..4 {
                    if w4[k] > 0 {
                        jmin = jmin.min(j4[k]);
                        jmax = jmax.max(j4[k]);
                    }
                }
                let d_dom = dist(p, j4[wi] as usize);
                let d_min = (0..4)
                    .filter(|&k| w4[k] > 0)
                    .map(|k| dist(p, j4[k] as usize))
                    .fold(f32::INFINITY, f32::min);
                sum_dom += d_dom;
                sum_min += d_min;
                mx_min = mx_min.max(d_min);
                if d_min > 0.5 {
                    nfar += 1;
                }
                n += 1;
            }
            println!(
                "  group{:<3} sub{:<2} verts={:<6} joints[{jmin}..{jmax}] mean_dom={:.3} mean_min={:.3} max_min={:.3} far={}",
                m.group_index, m.sub_object, n,
                sum_dom / n.max(1) as f32, sum_min / n.max(1) as f32, mx_min, nfar
            );
        }
        // Sample verts from the face region (y > 1.6): their 4 joints AS-IF-global vs their position.
        println!("face-region vertex samples (pos, joints, weights):");
        let mut shown = 0;
        for m in &meshes {
            if m.rigid || m.joints.is_empty() {
                continue;
            }
            for (vi, p) in m.positions.iter().enumerate() {
                if p[1] > 1.65 && shown < 10 {
                    let j4 = m.joints[vi];
                    let w4 = m.weights[vi];
                    println!(
                        "  group{:<3} v{vi:<5} pos=[{:+6.3},{:+6.3},{:+6.3}] joints={:?} weights={:?}",
                        m.group_index, p[0], p[1], p[2], j4, w4
                    );
                    shown += 1;
                }
            }
        }
        // Descriptor-tag census of the model container: reveals any candidate per-group
        // bone-palette chunk the reader currently ignores.
        {
            let n_desc = mercs2_formats::ffcs::read_u32_le(&container, 16) as usize;
            let mut tags: Vec<String> = Vec::new();
            for i in 0..n_desc.min((container.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                let t = &container[row..row + 4];
                let u0 = mercs2_formats::ffcs::read_u32_le(&container, row + 4);
                let sz = mercs2_formats::ffcs::read_u32_le(&container, row + 8);
                let marker = if u0 == 0xFFFF_FFFF { "*" } else { "" };
                tags.push(format!("{}{}({sz})", String::from_utf8_lossy(t), marker));
            }
            println!("container descriptor rows ({n_desc}): {}", tags.join(" "));
        }
        // Hexdump candidate palette carriers: the GEOM INDX, each PRMG INFO(56), each PRMT body,
        // and each SKIN INFO(4) — one of these must carry the per-group bone palette.
        {
            let data_off = mercs2_formats::ffcs::read_u32_le(&container, 4) as usize;
            let n_desc = mercs2_formats::ffcs::read_u32_le(&container, 16) as usize;
            let mut dumped_info = 0;
            for i in 0..n_desc.min((container.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                let t: [u8; 4] = container[row..row + 4].try_into().unwrap();
                let u0 = mercs2_formats::ffcs::read_u32_le(&container, row + 4);
                let sz = mercs2_formats::ffcs::read_u32_le(&container, row + 8) as usize;
                if u0 == 0xFFFF_FFFF {
                    continue;
                }
                let start = data_off + u0 as usize;
                let hex = |n: usize| -> String {
                    container[start..(start + n.min(sz)).min(container.len())]
                        .chunks(4)
                        .map(|c| c.iter().map(|b| format!("{b:02x}")).collect::<String>())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                match &t {
                    b"INDX" => println!("  INDX({sz}) @0x{start:x}: {}", hex(sz)),
                    b"INFO" if sz >= 56 && sz <= 60 && dumped_info < 6 => {
                        println!("  groupINFO({sz}) @0x{start:x}: {}", hex(sz));
                        dumped_info += 1;
                    }
                    b"PRMT" if dumped_info <= 8 => println!("  PRMT({sz}) @0x{start:x}: {}", hex(sz)),
                    _ => {}
                }
            }
        }
        // Base hypothesis check: BLENDINDICES look BASE-RELATIVE (global = slot + base), base =
        // u16 at the group's PRMG INFO(56/60) offset +24, count = u16 at +26. Verify per group:
        // read that field, brute-force the base that minimizes the mean vertex->bone distance,
        // and print both plus the distance at each.
        {
            let data_off = mercs2_formats::ffcs::read_u32_le(&container, 4) as usize;
            let n_desc = mercs2_formats::ffcs::read_u32_le(&container, 16) as usize;
            // group_index (PRMG ordinal) -> (info_base, info_count) from the INFO row after PRMG.
            let mut info_bases: Vec<(u16, u16)> = Vec::new();
            let mut want_info = false;
            for i in 0..n_desc.min((container.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                let t = &container[row..row + 4];
                let u0 = mercs2_formats::ffcs::read_u32_le(&container, row + 4);
                if t == b"PRMG" && u0 == 0xFFFF_FFFF {
                    want_info = true;
                    continue;
                }
                if want_info && t == b"INFO" && u0 != 0xFFFF_FFFF {
                    let start = data_off + u0 as usize;
                    let base = u16::from_le_bytes([container[start + 24], container[start + 25]]);
                    let cnt = u16::from_le_bytes([container[start + 26], container[start + 27]]);
                    // Full range table: +20 u32 range_count, +24 pairs (u16 base, u16 count).
                    let rc = mercs2_formats::ffcs::read_u32_le(&container, start + 20) as usize;
                    let sz = mercs2_formats::ffcs::read_u32_le(&container, row + 8) as usize;
                    let mut pairs: Vec<(u16, u16)> = Vec::new();
                    for r in 0..rc.min((sz.saturating_sub(24)) / 4) {
                        let o = start + 24 + r * 4;
                        pairs.push((
                            u16::from_le_bytes([container[o], container[o + 1]]),
                            u16::from_le_bytes([container[o + 2], container[o + 3]]),
                        ));
                    }
                    let total: u32 = pairs.iter().map(|&(_, c)| c as u32).sum();
                    println!(
                        "  PRMG#{} INFO range table: rc={rc} pairs={pairs:?} total_slots={total}",
                        info_bases.len()
                    );
                    info_bases.push((base, cnt));
                    want_info = false;
                }
            }
            println!("per-group INFO(+24) base/count vs brute-force best base:");
            for m in &meshes {
                if m.rigid || m.joints.is_empty() {
                    continue;
                }
                let (info_base, info_cnt) = info_bases.get(m.group_index).copied().unwrap_or((0xFFFF, 0));
                let jmax = m
                    .joints
                    .iter()
                    .zip(&m.weights)
                    .flat_map(|(j4, w4)| (0..4).filter(|&k| w4[k] > 0).map(|k| j4[k]))
                    .max()
                    .unwrap_or(0) as usize;
                let mean_at = |base: usize| -> f32 {
                    let (mut sum, mut n) = (0.0f32, 0usize);
                    for (vi, p) in m.positions.iter().enumerate() {
                        let (Some(j4), Some(w4)) = (m.joints.get(vi), m.weights.get(vi)) else { continue };
                        let wi = w4.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
                        let g = j4[wi] as usize + base;
                        if g >= world.len() {
                            return f32::INFINITY;
                        }
                        sum += dist(p, g);
                        n += 1;
                    }
                    sum / n.max(1) as f32
                };
                let mut best = (0usize, f32::INFINITY);
                for base in 0..world.len().saturating_sub(jmax) {
                    let d = mean_at(base);
                    if d < best.1 {
                        best = (base, d);
                    }
                }
                println!(
                    "  group{:<3} sub{:<2} jmax={jmax:<3} INFO base={info_base:<3} count={info_cnt:<3} d(info)={:.3} | best base={} d={:.3}",
                    m.group_index, m.sub_object,
                    if (info_base as usize) < world.len() { mean_at(info_base as usize) } else { f32::INFINITY },
                    best.0, best.1
                );
            }
        }
        // SEGM records + per-group PRMT primitive records (seg ref @0, vertex range @12), then a
        // per-PRIMITIVE base solve: primitives partition the vertex buffer and may carry their
        // own bone-window base via the SEGM they reference.
        {
            let segm = mercs2_formats::model_cubeize::parse_segm(&container);
            println!("SEGM records ({}):", segm.len());
            for (i, r) in segm.iter().enumerate() {
                println!("  seg{i:<2} bone={:<3} seg_id={} state=0x{:02x}  ({})", r.bone, r.seg_id, r.state_mask, nm(s.rig.get(r.bone as usize).map(|b| b.name_hash).unwrap_or(0)));
            }
            let data_off = mercs2_formats::ffcs::read_u32_le(&container, 4) as usize;
            let n_desc = mercs2_formats::ffcs::read_u32_le(&container, 16) as usize;
            // Collect PRMT bodies in PRMG order (one PRMT row per group, after the IBUF).
            let mut prmts: Vec<Vec<[u32; 4]>> = Vec::new();
            for i in 0..n_desc.min((container.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                let t = &container[row..row + 4];
                let u0 = mercs2_formats::ffcs::read_u32_le(&container, row + 4);
                if t == b"PRMT" && u0 != 0xFFFF_FFFF {
                    let start = data_off + u0 as usize;
                    let sz = mercs2_formats::ffcs::read_u32_le(&container, row + 8) as usize;
                    let recs: Vec<[u32; 4]> = (0..sz / 16)
                        .map(|k| {
                            let o = start + k * 16;
                            [
                                mercs2_formats::ffcs::read_u32_le(&container, o),
                                mercs2_formats::ffcs::read_u32_le(&container, o + 4),
                                mercs2_formats::ffcs::read_u32_le(&container, o + 8),
                                mercs2_formats::ffcs::read_u32_le(&container, o + 12),
                            ]
                        })
                        .collect();
                    prmts.push(recs);
                }
            }
            println!("per-group per-PRIMITIVE base solve (seg ref, vert range, best base, d):");
            for m in &meshes {
                if m.rigid || m.joints.is_empty() {
                    continue;
                }
                let Some(recs) = prmts.get(m.group_index) else { continue };
                println!("  group{} sub{} ({} prims):", m.group_index, m.sub_object, recs.len());
                for r in recs {
                    let seg = r[0] as usize;
                    let vmax = (r[3] & 0xFFFF) as usize;
                    let vnum = (r[3] >> 16) as usize;
                    let v0 = (vmax + 1).saturating_sub(vnum);
                    let mut jmax = 0usize;
                    let mean_at = |base: usize, jmax: &mut usize| -> f32 {
                        let (mut sum, mut n) = (0.0f32, 0usize);
                        for vi in v0..=vmax.min(m.positions.len().saturating_sub(1)) {
                            let (Some(j4), Some(w4)) = (m.joints.get(vi), m.weights.get(vi)) else { continue };
                            let wi = w4.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
                            let sl = j4[wi] as usize;
                            *jmax = (*jmax).max(sl);
                            let g = sl + base;
                            if g >= world.len() {
                                return f32::INFINITY;
                            }
                            sum += dist(&m.positions[vi], g);
                            n += 1;
                        }
                        sum / n.max(1) as f32
                    };
                    let mut best = (0usize, f32::INFINITY);
                    for base in 0..world.len() {
                        let mut jm = 0usize;
                        let d = mean_at(base, &mut jm);
                        jmax = jm;
                        if d < best.1 {
                            best = (base, d);
                        }
                    }
                    let seg_bone = segm.get(seg).map(|r| r.bone).unwrap_or(0xFFFF);
                    println!(
                        "    prim seg={seg}(bone {seg_bone}) verts {v0}..={vmax} jmax={jmax} best base={} d={:.3} d(segbone)={:.3}",
                        best.0,
                        best.1,
                        {
                            let mut jm = 0usize;
                            if (seg_bone as usize) < world.len() { mean_at(seg_bone as usize, &mut jm) } else { f32::INFINITY }
                        }
                    );
                }
            }
        }
        // Empirical palette solve: for each (group, joint-slot), the centroid of the verts
        // dominantly bound to that slot, and the nearest HIER bones to that centroid. This is
        // the mapping the data IMPLIES, independent of where it is encoded on disk.
        for m in &meshes {
            if m.rigid || m.joints.is_empty() {
                continue;
            }
            let jmax = m
                .joints
                .iter()
                .zip(&m.weights)
                .flat_map(|(j4, w4)| (0..4).filter(|&k| w4[k] > 0).map(|k| j4[k]))
                .max()
                .unwrap_or(0) as usize;
            let mut acc = vec![([0.0f32; 3], 0usize); jmax + 1];
            for (vi, p) in m.positions.iter().enumerate() {
                let (Some(j4), Some(w4)) = (m.joints.get(vi), m.weights.get(vi)) else { continue };
                let wi = w4.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
                let slot = j4[wi] as usize;
                let a = &mut acc[slot];
                for k in 0..3 {
                    a.0[k] += p[k];
                }
                a.1 += 1;
            }
            println!("  group{} sub{} slot->nearest-bone solve ({} slots):", m.group_index, m.sub_object, jmax + 1);
            for (slot, (sumc, n)) in acc.iter().enumerate() {
                if *n == 0 {
                    println!("    slot{slot:<3} (no dominant verts)");
                    continue;
                }
                let c = [sumc[0] / *n as f32, sumc[1] / *n as f32, sumc[2] / *n as f32];
                let mut cand: Vec<(usize, f32)> = (0..s.rig.len())
                    .map(|b| {
                        let w = &world[b];
                        let d = ((c[0] - w[3][0]).powi(2) + (c[1] - w[3][1]).powi(2) + (c[2] - w[3][2]).powi(2)).sqrt();
                        (b, d)
                    })
                    .collect();
                cand.sort_by(|a, b| a.1.total_cmp(&b.1));
                println!(
                    "    slot{slot:<3} n={n:<5} c=[{:+6.3},{:+6.3},{:+6.3}] near: {}",
                    c[0], c[1], c[2],
                    cand[..3]
                        .iter()
                        .map(|(b, d)| format!("bone{b}({}, {d:.3})", nm(s.rig[*b].name_hash)))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
        }
    }

    for (blk, _) in &hits {
        let data = wad::decompress_block_index(&mut w, *blk)?;
        let ag = parse_animgroup(&data).map_err(|e| format!("parse animgroup {blk}: {e}"))?;
        let c = ag.clips.iter().find(|c| c.name_hash == clip_hash).ok_or("clip vanished")?;

        // Raw trnm read-back straight from the block bytes (independent of read_trnm).
        let (count, entries) = parse_block_entry_table(&data);
        let mut pos = 4 + count as usize * 16;
        let mut raw: Option<(u32, u32, u32, Vec<u32>)> = None; // (size, count_word, lead_word, hashes)
        for e in &entries {
            let cont = &data[pos..(pos + e.chunk_size as usize).min(data.len())];
            pos += e.chunk_size as usize;
            if e.name_hash != clip_hash || cont.len() < 20 || &cont[0..4] != b"UCFX" {
                continue;
            }
            let dao = u32::from_le_bytes(cont[4..8].try_into().unwrap()) as usize;
            let nd = u32::from_le_bytes(cont[16..20].try_into().unwrap()) as usize;
            for i in 0..nd.min((cont.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                if &cont[row..row + 4] != b"trnm" {
                    continue;
                }
                let u0 = u32::from_le_bytes(cont[row + 4..row + 8].try_into().unwrap());
                let size = u32::from_le_bytes(cont[row + 8..row + 12].try_into().unwrap());
                let start = if dao > 0 { dao + u0 as usize } else { 8 + u0 as usize };
                let t = &cont[start..start + size as usize];
                let rd = |o: usize| u32::from_le_bytes(t[o..o + 4].try_into().unwrap());
                let cw = rd(0);
                let all: Vec<u32> = (1..(size as usize / 4)).map(|k| rd(k * 4)).collect();
                raw = Some((size, cw, all[0], all));
                break;
            }
            break;
        }

        let ac = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]).ok();
        println!("\n== block {blk}: clip 0x{clip_hash:08X} class={} ==", c.class);
        println!(
            "  header: numTransformTracks={} numFloatTracks={} duration={:.4}s poses={}",
            c.num_transform_tracks, c.num_float_tracks, c.duration, c.num_poses
        );
        if let Some(ac) = &ac {
            println!(
                "  decoder: decoded={} num_tracks={} num_frames={} duration={:.4}",
                ac.decoded, ac.num_tracks, ac.num_frames, ac.duration
            );
        }
        if let Some((size, cw, lead, all)) = &raw {
            println!(
                "  raw trnm: size={size} count_word={cw} lead_word=0x{lead:08X} ({})  size==8+count*4: {}  words_after_count={}",
                nm(*lead), *size as usize == 8 + *cw as usize * 4, all.len()
            );
        }
        let tth = c.binding.resolve_to_hier(&hier);
        println!("  binding: {} trnm hashes, {} resolve to HIER", tth.len(), tth.iter().filter(|r| r.is_some()).count());
        // Per-track decoded-data stats across every frame: max |T_anim - T_bind| (bind = the
        // bone's HIER local translation), scale min/max, worst |q|-1. Garbage on a track shows
        // up here as a huge T delta / non-unit scale even when the binding itself is correct.
        let frames: Vec<Vec<mercs2_formats::anim::QsTransform>> = match &ac {
            Some(ac) if ac.decoded && ac.num_frames > 0 => (0..ac.num_frames)
                .map(|f| ac.sample_local(ac.duration * f as f32 / (ac.num_frames.max(2) - 1) as f32))
                .collect(),
            _ => Vec::new(),
        };
        for (t, h) in c.binding.track_to_bone_hash.iter().enumerate() {
            let stats = if frames.is_empty() {
                String::new()
            } else {
                let bind_t: Option<[f32; 3]> = tth[t].map(|b| {
                    let lb = s.rig[b].local_bind;
                    [lb[3][0], lb[3][1], lb[3][2]]
                });
                let (mut max_dt, mut smin, mut smax, mut qerr) = (0.0f32, f32::INFINITY, f32::NEG_INFINITY, 0.0f32);
                for fr in &frames {
                    let Some(qs) = fr.get(t) else { continue };
                    if let Some(bt) = bind_t {
                        let d = ((qs.translation[0] - bt[0]).powi(2)
                            + (qs.translation[1] - bt[1]).powi(2)
                            + (qs.translation[2] - bt[2]).powi(2))
                        .sqrt();
                        max_dt = max_dt.max(d);
                    }
                    for &sc in &qs.scale {
                        smin = smin.min(sc);
                        smax = smax.max(sc);
                    }
                    let qn = qs.rotation.iter().map(|c| c * c).sum::<f32>().sqrt();
                    qerr = qerr.max((qn - 1.0).abs());
                }
                format!("  max|dT|={max_dt:7.4} scale=[{smin:+.3},{smax:+.3}] |q|err={qerr:.4}")
            };
            match tth[t] {
                Some(b) => println!(
                    "    track{t:<3} 0x{h:08X} -> bone{b:<3} parent={:<3} bindpos=[{:+7.3},{:+7.3},{:+7.3}]  {}{stats}",
                    s.rig[b].parent, world[b][3][0], world[b][3][1], world[b][3][2], nm(*h)
                ),
                None => println!("    track{t:<3} 0x{h:08X} -> UNRESOLVED  {}{stats}", nm(*h)),
            }
        }
        // Coverage: undriven bones + multiply-driven bones.
        let mut drive = vec![0u32; s.rig.len()];
        for r in tth.iter().flatten() {
            drive[*r] += 1;
        }
        let undriven: Vec<usize> = (0..s.rig.len()).filter(|&b| drive[b] == 0).collect();
        let multi: Vec<usize> = (0..s.rig.len()).filter(|&b| drive[b] > 1).collect();
        println!("  undriven bones: {undriven:?}");
        println!("  multiply-driven bones: {multi:?}");

        // Render-path replica: compute the EXACT palette render() computes (sample_local at
        // continuous times -> havok_palette) and CPU-skin every vert by its dominant joint.
        // Reports each bone's worst vertex displacement from its bind position across the
        // sweep — the numeric fingerprint of on-screen spikes.
        if let Some(ac) = &ac {
            if ac.decoded {
                let _ = &verts;
                let ntt = c.num_transform_tracks as usize;
                // Bone-length stretch: |modelpos[b] - modelpos[parent]| vs the bind bone
                // length, over the same locals havok_palette builds. Root motion cancels
                // out, so a ratio >> 1 IS a spike (a stretched bone), not locomotion.
                let bind_len: Vec<f32> = (0..s.rig.len())
                    .map(|b| {
                        if s.rig[b].parent < 0 {
                            0.0
                        } else {
                            let p = s.rig[b].parent as usize;
                            ((world[b][3][0] - world[p][3][0]).powi(2)
                                + (world[b][3][1] - world[p][3][1]).powi(2)
                                + (world[b][3][2] - world[p][3][2]).powi(2))
                            .sqrt()
                        }
                    })
                    .collect();
                let mut per_bone_max_len = vec![0.0f32; s.rig.len()];
                let steps = 101usize;
                for k in 0..steps {
                    let t = ac.duration * k as f32 / (steps - 1) as f32;
                    let sample = ac.sample_local(t);
                    let mut local = pose::bind_qs(&s.rig);
                    for (track, bone) in tth.iter().enumerate() {
                        if track >= ntt {
                            break;
                        }
                        if let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) {
                            if b < local.len() {
                                local[b] = *qs;
                            }
                        }
                    }
                    let model = pose::model_poses(&s.rig, &local);
                    for b in 0..s.rig.len() {
                        if s.rig[b].parent < 0 {
                            continue;
                        }
                        let p = s.rig[b].parent as usize;
                        let l = ((model[b].translation[0] - model[p].translation[0]).powi(2)
                            + (model[b].translation[1] - model[p].translation[1]).powi(2)
                            + (model[b].translation[2] - model[p].translation[2]).powi(2))
                        .sqrt();
                        if l > per_bone_max_len[b] {
                            per_bone_max_len[b] = l;
                        }
                    }
                }
                let mut ranked: Vec<(usize, f32)> = (0..s.rig.len())
                    .map(|b| (b, per_bone_max_len[b] - bind_len[b]))
                    .collect();
                ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
                let bone_track: std::collections::HashMap<usize, usize> =
                    tth.iter().enumerate().filter_map(|(t, b)| b.map(|bb| (bb, t))).collect();
                println!("  render-path replica, worst bone-length stretch (bone  anim_len vs bind_len  track  name):");
                for (b, ex) in ranked.iter().take(14) {
                    println!(
                        "    bone{b:<3} stretch={ex:+7.3} (max {:.3} vs bind {:.3}) parent={:<3} track={:<4} {}",
                        per_bone_max_len[*b],
                        bind_len[*b],
                        s.rig[*b].parent,
                        bone_track.get(b).map(|t| t.to_string()).unwrap_or_else(|| "-".into()),
                        nm(s.rig[*b].name_hash)
                    );
                }
            }
        }
    }
    Ok(())
}

/// Bind-pose skinning gate (headless): the palette `Skin[b] = InvBind[b]·Pose[b]` must be identity
/// at bind pose. Reports the worst per-bone deviation from I, the fit transform, and blend coverage.
/// A near-zero max deviation means the LBS palette reproduces the un-skinned render exactly.
pub fn skincheck(wadpath: &str, model: Option<String>, index: Option<String>) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, _indices, _draws, s) = mesh::build_indexed_from_container(&container)?;

    let mut worst = 0.0f32;
    let mut worst_bone = 0usize;
    for (b, m) in s.bones.iter().enumerate() {
        for r in 0..4 {
            for c in 0..4 {
                let ident = if r == c { 1.0 } else { 0.0 };
                let d = (m[r][c] - ident).abs();
                if d > worst {
                    worst = d;
                    worst_bone = b;
                }
            }
        }
    }
    // Recompose gate: rebuild the palette from the rig's bind-pose LOCAL transforms (local->world
    // ->skin chain, the animation path). Must also be identity, proving the hierarchy recompose.
    let recomposed = pose::palette(&s.rig, &pose::bind_locals(&s.rig));
    let mut worst_r = 0.0f32;
    for m in &recomposed {
        for r in 0..4 {
            for c in 0..4 {
                let ident = if r == c { 1.0 } else { 0.0 };
                worst_r = worst_r.max((m[r][c] - ident).abs());
            }
        }
    }

    let skinned = verts.iter().filter(|v| v.weights != [255, 0, 0, 0]).count();
    println!("model 0x{hash:08X}: {} bones, {} verts", s.bones.len(), verts.len());
    println!("fit: center={:?} scale={:.5}", s.fit_center, s.fit_scale);
    println!(
        "bind-pose palette   max |Skin - I| = {worst:.6} (bone {worst_bone})  ->  {}",
        if worst < 1e-3 { "GATE PASS (identity)" } else { "GATE FAIL — convention bug" }
    );
    println!(
        "recomposed palette  max |Skin - I| = {worst_r:.6}                 ->  {}",
        if worst_r < 1e-3 { "GATE PASS (local->world->skin)" } else { "GATE FAIL — recompose bug" }
    );
    println!(
        "blend coverage: {skinned}/{} verts skinned ({} rigid/pass-through)",
        verts.len(),
        verts.len() - skinned
    );
    Ok(())
}

/// Per-STRM diagnostic for one model: stride, vcount, decl, POSITION element, and bbox — to
/// pinpoint a mis-positioned submesh (e.g. a floating accessory).
pub fn wad_meshes(wadpath: &str, model: Option<String>) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = match model {
        Some(m) => parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?,
        None => models.first().map(|&(h, _)| h).ok_or("no models in WAD")?,
    };
    let container = wad::extract_container(&mut w, hash)?;
    let strms = mercs2_formats::model_cubeize::describe_model_strms(&container)?;
    println!("model 0x{hash:08X}: {} STRM groups", strms.len());
    for (i, s) in strms.iter().enumerate() {
        let pos = match s.pos {
            Some((st, off, ty)) => format!("pos[stream={st} off={off} type={ty}]"),
            None => "pos[NONE]".to_string(),
        };
        let bbox = match s.bbox {
            Some((lo, hi)) => format!(
                "y[{:6.2},{:6.2}] x[{:6.2},{:6.2}] z[{:6.2},{:6.2}]",
                lo[1], hi[1], lo[0], hi[0], lo[2], hi[2]
            ),
            None => "bbox[-]".to_string(),
        };
        println!(
            "  [{i:2}] stride={:<3} vcount={:<6} decl={:<2} {pos:<28} {bbox}",
            s.stride, s.vcount, s.decl_elems
        );
    }

    // UV/normal extraction coverage (1e reader check): per group, how many verts got UVs/normals
    // + the UV range (expect roughly [0,1]).
    let meshes = mercs2_formats::model_cubeize::read_model_meshes(&container)?;

    // Winding check: fraction of triangles whose geometric winding (cross of edges) agrees with the
    // vertex normal. ~1.0 => tri order a,b,c is CCW-when-viewed-from-outside (front_face Ccw);
    // ~0.0 => CW. Tells us the correct cull front_face without a GPU trial.
    let (mut agree, mut total) = (0u64, 0u64);
    for m in &meshes {
        for t in &m.tris {
            if m.normals.is_empty() {
                continue;
            }
            let (p0, p1, p2) = (
                m.positions[t[0] as usize],
                m.positions[t[1] as usize],
                m.positions[t[2] as usize],
            );
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let gn = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let n = m.normals[t[0] as usize];
            let d = gn[0] * n[0] + gn[1] * n[1] + gn[2] * n[2];
            if d > 0.0 {
                agree += 1;
            }
            total += 1;
        }
    }
    if total > 0 {
        println!(
            "-- winding: {:.1}% of tris wind CCW-outward (>~90% => front_face Ccw; <~10% => Cw) --",
            100.0 * agree as f64 / total as f64
        );
    }
    println!("-- geometry read: {} drawing groups --", meshes.len());
    for (i, m) in meshes.iter().enumerate() {
        let (mut u0, mut u1, mut v0, mut v1) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
        for uv in &m.uvs {
            u0 = u0.min(uv[0]);
            u1 = u1.max(uv[0]);
            v0 = v0.min(uv[1]);
            v1 = v1.max(uv[1]);
        }
        let uvr = if m.uvs.is_empty() {
            "uv[none]".to_string()
        } else {
            format!("u[{u0:5.2},{u1:5.2}] v[{v0:5.2},{v1:5.2}]")
        };
        // Per-group winding agreement (CCW-outward %).
        let (mut ga, mut gt) = (0u64, 0u64);
        for t in &m.tris {
            if m.normals.is_empty() {
                break;
            }
            let (p0, p1, p2) = (
                m.positions[t[0] as usize],
                m.positions[t[1] as usize],
                m.positions[t[2] as usize],
            );
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let gn = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let n = m.normals[t[0] as usize];
            if gn[0] * n[0] + gn[1] * n[1] + gn[2] * n[2] > 0.0 {
                ga += 1;
            }
            gt += 1;
        }
        let wind = if gt > 0 {
            format!("wind={:.0}%", 100.0 * ga as f64 / gt as f64)
        } else {
            "wind=-".to_string()
        };
        let kind = if m.rigid { "MESH" } else { "SKIN" };
        println!(
            "  [{i:2}] verts={:<6} tris={:<6} so={:<2} {kind} bone={:<3} mask={:#04x} {wind:<9} {uvr}",
            m.positions.len(),
            m.tris.len(),
            m.sub_object,
            m.bone,
            m.state_mask
        );
    }
    Ok(())
}

/// Headless placement probe (VERIFIABLE proof): parse block 29, load all
/// placements, and print counts, ranges, the interior hunt, and whether the
/// records carry a model-asset hash (they key by entity — see report).
pub fn placement_probe(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let (_low, ls) = find_terrain_blocks(&mut w)?;
    println!("[placement-probe] layers_static block = {} bytes", ls.len());
    let placements = mercs2_formats::placement::load_placements(&ls)?;
    report_interior_hunt(&placements);
    // Quat unit-length sanity across all records.
    let mut nonunit = 0usize;
    for p in &placements {
        let m = p.quat[0] * p.quat[0] + p.quat[1] * p.quat[1] + p.quat[2] * p.quat[2] + p.quat[3] * p.quat[3];
        if !(0.81..=1.21).contains(&m) {
            nonunit += 1;
        }
    }
    println!(
        "[placement-probe] quaternion unit-length: {} of {} outside [0.9,1.1]^2",
        nonunit,
        placements.len()
    );

    // Where does the interior geometry actually live? Scan the WAD's block-path table (PTHS) for
    // interior/hqinterior/pmcinterior block names — the interior cell is a separate block, not in
    // layers_static.
    let hits: Vec<(usize, &String)> = wad::block_paths(&w)
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            let l = p.to_ascii_lowercase();
            l.contains("interior") || l.contains("hqint") || l.contains("pmcint") || l.contains("briefing")
        })
        .collect();
    println!("[placement-probe] WAD block paths matching interior/briefing = {}", hits.len());
    for (i, p) in hits.iter().take(30) {
        println!("[placement-probe]   block {i}: {p}");
    }
    Ok(())
}

/// Headless Layer-1 World Block Index probe (VERIFIABLE proof, spec §10): build the full
/// `WorldIndex`, print total blocks, a per-class histogram, the proven verification counts
/// (models / c3-mesh / lrterrain / placements / grid anchor), and sample `blocks_near` +
/// `lod_chain` queries. No rendering, no streaming loop — index only.
pub fn world_index_probe(wadpath: &str) -> Result<(), String> {
    use mercs2_formats::world_index::{BlockClass, WorldIndex};
    use std::time::Instant;

    let mut w = wad::open(wadpath)?;

    // Build the index (times the full scan; placement AABBs are lazy so this is the eager cost).
    let t0 = Instant::now();
    let idx = {
        let (archive, file) = wad::archive_and_file(&mut w);
        WorldIndex::build(archive, file)
    };
    let build_ms = t0.elapsed().as_secs_f64() * 1000.0;

    println!("[world-index] total blocks = {}", idx.len());

    // Histogram by class.
    let classes = [
        BlockClass::Model,
        BlockClass::C3Cell,
        BlockClass::LayersStatic,
        BlockClass::VzStateOverlay,
        BlockClass::LowResTerrain,
        BlockClass::Texture,
        BlockClass::Animation,
        BlockClass::Other,
    ];
    println!("[world-index] class histogram:");
    for c in classes {
        let n = idx.by_class(c).count();
        println!("[world-index]   {:<16} {}", c.name(), n);
    }

    // --- Verification counts vs the proven totals ---
    // 1,771 primary model ASETs (from the ASET table directly).
    let model_asets = wad::model_list(&w).len();
    // c3 blocks carrying model-format geometry (~1,849).
    let c3_mesh = idx
        .by_class(BlockClass::C3Cell)
        .filter(|b| b.has_model_geometry)
        .count();
    let c3_total = idx.by_class(BlockClass::C3Cell).count();

    println!("[world-index] --- verification ---");
    println!(
        "[world-index] primary model ASETs      = {model_asets}  (expect 1771) {}",
        if model_asets == 1771 { "MATCH" } else { "DIFF" }
    );
    println!(
        "[world-index] c3 blocks (total)        = {c3_total}"
    );
    println!(
        "[world-index] c3 blocks w/ model geom  = {c3_mesh}  (expect ~1849)"
    );

    // 400 lrterrain tiles + 62,624 placements — via the format loaders on blocks 29/3121.
    let (low, ls) = find_terrain_blocks(&mut w)?;
    let tm = mercs2_formats::terrain::load_terrain(&low, &ls)?;
    println!(
        "[world-index] lrterrain tiles placed   = {}  (expect 400) {}",
        tm.tiles_placed,
        if tm.tiles_placed == 400 { "MATCH" } else { "DIFF" }
    );
    let placements = mercs2_formats::placement::load_placements(&ls)?;
    println!(
        "[world-index] layers_static placements = {}  (expect 62624) {}",
        placements.len(),
        if placements.len() == 62624 { "MATCH" } else { "DIFF" }
    );

    // c3 grid anchor: c30123 -> (-2156.25, -3783.75).
    let (ax, az) = mercs2_formats::world_index::c3_cell_centre(30123);
    let anchor_ok = (ax - (-2156.25)).abs() < 0.01 && (az - (-3783.75)).abs() < 0.01;
    println!(
        "[world-index] c3 anchor c30123         = ({ax:.2}, {az:.2})  (expect -2156.25,-3783.75) {}",
        if anchor_ok { "MATCH" } else { "DIFF" }
    );

    // --- Sample proximity queries (blocks_near) ---
    for (qx, qz, r) in [(2560.0f32, -926.0f32, 300.0f32), (0.0, 0.0, 500.0)] {
        let (archive, file) = wad::archive_and_file(&mut w);
        // A fresh index for the lazy-extent query so we don't hold a mutable borrow across w reuse.
        let mut idx2 = WorldIndex::build(archive, file);
        let (archive, file) = wad::archive_and_file(&mut w);
        let hits = idx2.blocks_near(qx, qz, r, archive, file);
        println!(
            "[world-index] blocks_near({qx:.0},{qz:.0}, r={r:.0}) = {} blocks",
            hits.len()
        );
        for bi in hits.iter().take(8) {
            if let Some(b) = idx2.block(*bi) {
                let e = b.extent.map(|a| {
                    format!(
                        "X[{:.0},{:.0}] Z[{:.0},{:.0}]",
                        a.min[0], a.max[0], a.min[2], a.max[2]
                    )
                });
                println!(
                    "[world-index]   blk {:<5} {:<14} {:<28} {}",
                    bi,
                    b.class.name(),
                    b.name,
                    e.unwrap_or_else(|| "(no extent)".into())
                );
            }
        }
    }

    // Leading-tier histogram across c3-class blocks (how the name's FIRST cell token distributes).
    let mut lead_tier: [usize; 4] = [0; 4];
    let mut chain_blocks = 0usize;
    for b in idx.by_class(BlockClass::C3Cell) {
        if let Some(t) = b.lod.tier {
            if (t as usize) < 4 {
                lead_tier[t as usize] += 1;
            }
        }
        if b.lod.chain.len() > 1 {
            chain_blocks += 1;
        }
    }
    println!(
        "[world-index] c3 leading-tier hist    = c0:{} c1:{} c2:{} c3:{}  ({} chain-named)",
        lead_tier[0], lead_tier[1], lead_tier[2], lead_tier[3], chain_blocks
    );

    // --- Does a cell carry GEOMETRY at more than one tier? (Decides whether a coarse<->fine LOD
    // swap even exists in the data, or whether geometry is authored at a single granularity per
    // region.) Group geometry-bearing c3 blocks by base cell; count distinct geometry tiers each.
    let mut geom_tiers_per_cell: std::collections::HashMap<u32, std::collections::HashSet<u8>> =
        std::collections::HashMap::new();
    for b in idx.by_class(BlockClass::C3Cell) {
        if !b.has_model_geometry {
            continue;
        }
        if let (Some(cid), Some(t)) = (b.lod.base_cell_id, b.lod.tier) {
            geom_tiers_per_cell.entry(cid).or_default().insert(t);
        }
    }
    let cells_geom = geom_tiers_per_cell.len();
    let multi_tier = geom_tiers_per_cell.values().filter(|s| s.len() > 1).count();
    let mut per_cell_geom_block_count: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    for b in idx.by_class(BlockClass::C3Cell) {
        if b.has_model_geometry {
            if let Some(cid) = b.lod.base_cell_id {
                *per_cell_geom_block_count.entry(cid).or_insert(0) += 1;
            }
        }
    }
    let multi_block = per_cell_geom_block_count.values().filter(|n| **n > 1).count();
    let max_geom_blocks = per_cell_geom_block_count.values().copied().max().unwrap_or(0);
    println!(
        "[world-index] geometry-bearing base cells = {cells_geom}; with geom at >1 TIER = {multi_tier}; \
         with >1 geom BLOCK = {multi_block} (max {max_geom_blocks} geom blocks/cell)"
    );

    // --- Sample LOD chain for one c3 cell: prefer a cell that actually ships multiple tiers
    // (a chain-named block), else fall back to the first c3 cell.
    let mut tier_count: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for b in idx.by_class(BlockClass::C3Cell) {
        if let Some(cid) = b.lod.base_cell_id {
            *tier_count.entry(cid).or_insert(0) += 1;
        }
    }
    let sample_cell = tier_count
        .iter()
        .filter(|(_, n)| **n > 1)
        .max_by_key(|(_, n)| **n)
        .map(|(cid, _)| *cid)
        .or_else(|| {
            idx.by_class(BlockClass::C3Cell)
                .find_map(|b| b.lod.base_cell_id)
        });
    if let Some(cid) = sample_cell {
        println!("[world-index] lod_chain(base_cell {cid}):");
        let chain = idx.lod_chain(cid);
        for (tier, slot) in chain.iter().enumerate() {
            match slot {
                Some(b) => println!(
                    "[world-index]   tier c{tier}: blk {} {} (P{:?} Q{:?})",
                    b.block_index, b.name, b.lod.p, b.lod.q
                ),
                None => println!("[world-index]   tier c{tier}: (none)"),
            }
        }
    }

    println!("[world-index] build time = {build_ms:.1} ms (eager scan; placement AABBs lazy)");
    Ok(())
}

/// Headless streaming-runtime probe (spec §10 verification): build the Layer-2 decision core over a
/// scripted camera path from the PMC exterior spawn outward, and log per-step resident-block count,
/// awake/hibernated entity counts, and the awake LOD-tier distribution — WITHOUT opening a window.
/// This proves the control-driven runtime independently of the GPU executor.
pub fn stream_probe(wadpath: &str) -> Result<(), String> {
    use mercs2_core::streaming::StreamingConfig;
    use std::time::Instant;

    let mut w = wad::open(wadpath)?;
    let t0 = Instant::now();
    let idx = {
        let (archive, file) = wad::archive_and_file(&mut w);
        mercs2_formats::world_index::WorldIndex::build(archive, file)
    };
    // layers_static (block 29) — the always-loaded base placement layer.
    let (_low, ls) = find_terrain_blocks(&mut w)?;
    let cfg = StreamingConfig::default();
    let (mut mgr, props, _terrain_tiles) = build_streaming_catalog(&idx, &ls, cfg);
    let build_ms = t0.elapsed().as_secs_f64() * 1000.0;

    println!("[stream-probe] catalog: {} geometry blocks (per-object, tier-scaled distance), {} per-entity props (of {} keyed spawns)", mgr.block_count(), mgr.entity_count(), props.len());
    println!(
        "[stream-probe] config: tier_stream_out(c0..c3)={:?} unload_margin={:.0} block_budget={} entity_budget={} scan_cap={:.0} hysteresis={:.0}",
        cfg.tier_stream_out, cfg.block_unload_margin, cfg.block_budget, cfg.entity_budget, cfg.entity_scan_cap, cfg.entity_hysteresis
    );
    println!("[stream-probe] built in {build_ms:.1} ms");

    // Scripted camera path: start at the PMC exterior/pool spawn, sweep outward (roughly NE across
    // the map) so blocks/entities load then hibernate as the camera passes. Each waypoint is settled
    // over several ticks so the throttled load budget can catch up before we read counts.
    let start = EXTERIOR_SPAWN; // (2560.26, -13.18, -926.25)
    let path: [[f32; 3]; 8] = [
        [start[0], start[1], start[2]],
        [start[0] + 200.0, start[1], start[2] + 200.0],
        [start[0] + 600.0, start[1], start[2] + 600.0],
        [start[0] + 1200.0, start[1], start[2] + 1200.0],
        [1000.0, 0.0, 1000.0],
        [0.0, 0.0, 0.0],
        [-1500.0, 0.0, -1500.0],
        [-3000.0, 50.0, -3000.0],
    ];
    const SETTLE_TICKS: u32 = 12; // enough for the per-frame budgets to converge at each waypoint

    for (wi, p) in path.iter().enumerate() {
        let mut last = mercs2_core::streaming::StreamDiff::default();
        let (mut loaded, mut unloaded, mut woke, mut hib) = (0usize, 0usize, 0usize, 0usize);
        for _ in 0..SETTLE_TICKS {
            let d = mgr.update(*p);
            loaded += d.load_blocks.len();
            unloaded += d.unload_blocks.len();
            woke += d.wake.len();
            hib += d.hibernate.len();
            last = d;
        }
        println!(
            "[stream-probe] wp{} ({:>7.0},{:>7.0}): resident={:<4} awake={:<5} | +load {:<3} -load {:<3} +wake {:<4} -hib {:<4} | settled_diff_empty={}",
            wi, p[0], p[2],
            mgr.resident_count(), mgr.awake_count(),
            loaded, unloaded, woke, hib,
            last.is_empty()
        );
    }
    println!(
        "[stream-probe] final: resident={} awake={} (of {} props / {} blocks)",
        mgr.resident_count(), mgr.awake_count(), mgr.entity_count(), mgr.block_count()
    );
    Ok(())
}

/// Task 1: for each of the PMC-interior keys, scan the candidate overlay blocks for its Transform
/// (→ pos+quat) and ModelName (→ model_hash), test `extract_container(model_hash)`, and — when a
/// key has a Transform but no ModelName — try the `pandemic_hash_m2(name)` mesh fallback. Prints a
/// per-key table; the same resolution is what `load_pmc_interior` consumes. `extra_keys` (if given)
/// REPLACES the default 6 keys, using the key itself as the name (no name-hash fallback then).
pub fn entity_find(wadpath: &str, extra_keys: &[u32]) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;

    // Key -> canonical name (owned Strings so ad-hoc keys work). Default = the documented 6.
    let entities: Vec<(u32, String)> = if extra_keys.is_empty() {
        PMC_INTERIOR_ENTITIES.iter().map(|(k, n)| (*k, n.to_string())).collect()
    } else {
        extra_keys.iter().map(|k| (*k, format!("0x{k:08X}"))).collect()
    };

    // Decompress each candidate block once and parse its placements + model placements.
    struct BlockData {
        block: u16,
        // key -> (pos, quat) from load_placements (Transform+Name join).
        xform: std::collections::HashMap<u32, ([f32; 3], [f32; 4])>,
        // key -> model_hash from load_model_placements (ModelName COMP).
        models: std::collections::HashMap<u32, u32>,
    }
    // Which blocks to scan: the fixed candidates, or (MERCS2_SCANALL=1) every UCFX overlay block
    // — a WAD-wide hunt for a key's owning block.
    let scan_blocks: Vec<u16> = if std::env::var_os("MERCS2_SCANALL").is_some() {
        (0..wad::block_paths(&w).len() as u16).collect()
    } else {
        INTERIOR_CANDIDATE_BLOCKS.to_vec()
    };
    let want: std::collections::HashSet<u32> = entities.iter().map(|(k, _)| *k).collect();

    let mut blocks: Vec<BlockData> = Vec::new();
    for &blk in &scan_blocks {
        let dec = match wad::decompress_block_index(&mut w, blk) {
            Ok(d) => d,
            Err(e) => {
                println!("[entity-find] block {blk}: decompress failed: {e}");
                continue;
            }
        };
        let mut xform = std::collections::HashMap::new();
        if let Ok(pl) = mercs2_formats::placement::load_placements(&dec) {
            for p in &pl {
                xform.entry(p.key).or_insert((p.pos, p.quat));
            }
        }
        let mut models = std::collections::HashMap::new();
        for mp in mercs2_formats::placement::load_model_placements(&dec) {
            models.entry(mp.key).or_insert(mp.model_hash);
        }
        // In scan-all mode only log blocks that actually key one of the wanted entities.
        let hits: Vec<u32> = want.iter().filter(|k| xform.contains_key(k) || models.contains_key(k)).copied().collect();
        if !std::env::var_os("MERCS2_SCANALL").is_some() {
            println!(
                "[entity-find] block {blk}: {} Transform keys, {} ModelName keys",
                xform.len(), models.len()
            );
        } else if !hits.is_empty() {
            let path = wad::block_paths(&w).get(blk as usize).cloned().unwrap_or_default();
            println!(
                "[entity-find] block {blk} '{path}': keys {}",
                hits.iter().map(|k| format!("0x{k:08X}")).collect::<Vec<_>>().join(",")
            );
        }
        blocks.push(BlockData { block: blk, xform, models });
    }

    let mut found: Vec<FoundEntity> = Vec::new();
    for (key, name) in &entities {
        // First Transform across candidate blocks (interior overlays don't duplicate an entity).
        let mut transform = None;
        for b in &blocks {
            if let Some(&(pos, quat)) = b.xform.get(key) {
                transform = Some((b.block, pos, quat));
                break;
            }
        }
        // First ModelName across candidate blocks.
        let mut model: Option<(u32, String, u16)> = None;
        for b in &blocks {
            if let Some(&h) = b.models.get(key) {
                model = Some((h, "ModelName".to_string(), b.block));
                break;
            }
        }
        // Fallback: no ModelName but a canonical name — try pandemic_hash_m2(name) as the mesh hash.
        if model.is_none() && extra_keys.is_empty() {
            // Try both the raw name and the name with a leading underscore stripped (the Lua
            // building keys are written with a leading '_').
            let cands = [name.as_str(), name.trim_start_matches('_')];
            for cand in cands {
                let h = mercs2_formats::hash::pandemic_hash_m2(cand);
                if wad::extract_container(&mut w, h).is_ok() {
                    model = Some((h, format!("name-hash '{cand}'"), 0));
                    break;
                }
            }
        }
        // Resolve container geometry for whichever mesh hash we have.
        let container = model.as_ref().and_then(|(h, _, _)| {
            let h = *h;
            wad::extract_container(&mut w, h).ok().and_then(|c| {
                mesh::build_indexed_from_container(&c)
                    .ok()
                    .map(|(v, idx, _d, s)| (v.len(), idx.len() / 3, s.bbox_min, s.bbox_max))
            })
        });
        found.push(FoundEntity {
            key: *key,
            name: PMC_INTERIOR_ENTITIES.iter().find(|(k, _)| k == key).map(|(_, n)| *n).unwrap_or("<adhoc>"),
            transform,
            model,
            container,
        });
    }

    // Report table.
    println!("\n[entity-find] ===== PMC INTERIOR ENTITY TABLE ({} keys) =====", found.len());
    for f in &found {
        println!("\n  key 0x{:08X}  {}", f.key, f.name);
        match f.transform {
            Some((blk, pos, quat)) => println!(
                "    Transform : block {blk}  pos=({:.3},{:.3},{:.3})  quat=({:+.4},{:+.4},{:+.4},{:+.4})  yaw={:.3}rad",
                pos[0], pos[1], pos[2], quat[0], quat[1], quat[2], quat[3],
                mercs2_formats::placement::yaw_from_quat(&quat)
            ),
            None => println!("    Transform : MISS (no Transform record for this key in any candidate block)"),
        }
        match &f.model {
            Some((h, src, blk)) => {
                let where_ = if *blk == 0 { String::new() } else { format!(" (block {blk})") };
                println!("    ModelName : 0x{h:08X}  via {src}{where_}");
            }
            None => println!("    ModelName : MISS (no ModelName COMP, and name-hash mesh not in WAD)"),
        }
        match f.container {
            Some((v, t, bmin, bmax)) => println!(
                "    Mesh      : extract_container OK — {v} verts / {t} tris  local-bbox x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}]",
                bmin[0], bmax[0], bmin[1], bmax[1], bmin[2], bmax[2]
            ),
            None => match &f.model {
                Some((h, _, _)) => println!("    Mesh      : MISS — model 0x{h:08X} has no primary ASET / container build failed"),
                None => println!("    Mesh      : MISS — no model hash to resolve"),
            },
        }
    }
    let resolved = found.iter().filter(|f| f.container.is_some()).count();
    println!(
        "\n[entity-find] summary: {}/{} keys resolve to a real mesh; {} have a Transform.",
        resolved,
        found.len(),
        found.iter().filter(|f| f.transform.is_some()).count()
    );
    Ok(())
}

/// Headless COMP probe (RESEARCH BRICK deliverable a–e):
///  1. Enumerate every COMP in layers_static (block 29) AND the PMC interior state block (667).
///  2. Reverse-scan every COMP's data blob for the anchor interior/model hashes and c3 model hashes.
///  3. When an anchor is found: report COMP type, byte offset in the record, and the owning entity.
///  4. Cross-check the winning COMP against the ECS `Model` class (0x5b724250, stride 4).
///  5. Prove one entity end-to-end: key -> Model COMP -> mesh hash -> extract_container -> verts/tris.
/// Hex-dump the data blobs of a named COMP across `layers_static` (block 29), alongside the owning
/// sub-block's Transform keys, so the on-disk record stride can be reversed empirically (the `schm`
/// payload_stride is the in-memory footprint, not the on-disk stride — Transform is 42 on disk vs
/// schm 52). Prints, per COMP occurrence: sub_block, data span, hex of the first bytes, and the
/// set of entity keys present in the same sub-block (Transform records) so leading-u32 keys in the
/// blob can be recognised.
pub fn comp_dump(wadpath: &str, target: &str) -> Result<(), String> {
    use mercs2_formats::placement::{comp_inventory, load_placements};
    let mut w = wad::open(wadpath)?;
    let (_low, ls) = find_terrain_blocks(&mut w)?;

    // Per sub-block key set (from Transform/Name records) to recognise keys inside the target blob.
    let placements = load_placements(&ls).unwrap_or_default();
    let mut keys_by_sub: std::collections::HashMap<u16, std::collections::HashSet<u32>> =
        std::collections::HashMap::new();
    let mut name_by_key: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    for p in &placements {
        keys_by_sub.entry(p.sub_block).or_default().insert(p.key);
        if let Some(n) = &p.name {
            name_by_key.entry(p.key).or_insert_with(|| n.clone());
        }
    }

    let inv = comp_inventory(&ls);
    let mut shown = 0usize;
    for c in &inv {
        if c.info_name.as_deref() != Some(target) {
            continue;
        }
        let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
        if off + size > ls.len() {
            continue;
        }
        let blob = &ls[off..off + size];
        let known = keys_by_sub.get(&c.sub_block);
        println!(
            "[comp-dump] {target} sub_block={} data_off={off} size={size} schm_stride={:?}",
            c.sub_block, c.payload_stride
        );
        // Hex dump in 16-byte rows.
        for (row, chunk) in blob.chunks(16).enumerate().take(8) {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            println!("[comp-dump]   +{:04x}: {}", row * 16, hex.join(" "));
        }
        // Try to recognise entity keys at every 4-byte-aligned offset that match this sub-block.
        if let Some(known) = known {
            let mut hits: Vec<(usize, u32)> = Vec::new();
            let mut i = 0usize;
            while i + 4 <= blob.len() {
                let v = u32::from_le_bytes([blob[i], blob[i + 1], blob[i + 2], blob[i + 3]]);
                if known.contains(&v) {
                    hits.push((i, v));
                }
                i += 1;
            }
            print!("[comp-dump]   key-hits (off:key):");
            for (o, k) in hits.iter().take(24) {
                print!(" {o}:0x{k:08x}");
            }
            println!();
            // Infer stride = gap between the first two key hits (if regular).
            if hits.len() >= 2 {
                let strides: Vec<usize> = hits.windows(2).map(|w| w[1].0 - w[0].0).collect();
                println!("[comp-dump]   key-hit gaps: {strides:?}");
            }
        }
        shown += 1;
        if shown >= 6 {
            break;
        }
    }
    if shown == 0 {
        println!("[comp-dump] no '{target}' COMP found in layers_static(29)");
    }

    // World-wide summary for HibernationControl: the per-entity distance distribution + how many
    // props (ModelName placements) actually carry one vs fall back to class defaults.
    if target == "HibernationControl" {
        use mercs2_formats::placement::{load_hibernation, load_model_placements};
        let hib = load_hibernation(&ls);
        let mut d0: Vec<u16> = hib.values().map(|h| h.dist[0]).collect();
        d0.sort_unstable();
        let n = d0.len();
        if n > 0 {
            let min = d0[0];
            let max = d0[n - 1];
            let med = d0[n / 2];
            let over400 = d0.iter().filter(|&&v| v > 400).count();
            // Confirm dist[1..4] are the constant class defaults across every record.
            let non_default = hib
                .values()
                .filter(|h| h.dist[1] != 160 || h.dist[2] != 60 || h.dist[3] != 20)
                .count();
            let flagged = hib.values().filter(|h| h.flag != 0).count();
            println!(
                "[comp-dump] --- HibernationControl world summary (layers_static) ---\n\
                 [comp-dump]   entities with directive: {n}\n\
                 [comp-dump]   dist0 (hibernation): min={min} median={med} max={max}  (>400: {over400})\n\
                 [comp-dump]   dist1..3 != default(160/60/20): {non_default} entities\n\
                 [comp-dump]   flag != 0: {flagged} entities"
            );
        }
        let props = load_model_placements(&ls);
        let with = props.iter().filter(|p| p.hibernation.is_some()).count();
        println!(
            "[comp-dump]   ModelName props: {} total, {with} carry a HibernationControl \
             (rest use class defaults)",
            props.len()
        );
    }
    Ok(())
}

pub fn comp_probe(wadpath: &str) -> Result<(), String> {
    use mercs2_formats::placement::{comp_inventory, load_placements, yaw_from_quat, CompInfo};
    let mut w = wad::open(wadpath)?;

    // The anchor model hashes (verified to load via wad::extract_container this session).
    let anchors: &[(u32, &str)] = &[
        (0x50AA_CA22, "pmcoutpost_bld_hq"),
        (0xC087_777D, "pmcoutpost_bld_pool"),
        (0xD5D6_5249, "pmcoutpost_bld_hqsuites"),
    ];

    // Resolve the two target blocks by the same live index the rest of the engine uses.
    let (_low, ls) = find_terrain_blocks(&mut w)?;
    let state = wad::decompress_block_index(&mut w, PMC_INTERIOR_STATE_BLOCK)
        .map_err(|e| format!("interior state block {PMC_INTERIOR_STATE_BLOCK} decompress: {e}"))?;

    // ---- (a) COMP inventory for both blocks -------------------------------------------------
    for (label, blk) in [("layers_static(29)", &ls), ("interior_state(667)", &state)] {
        let inv = comp_inventory(blk);
        let mut by_name: std::collections::BTreeMap<String, (usize, Option<u32>)> =
            std::collections::BTreeMap::new();
        for c in &inv {
            let name = c.info_name.clone().unwrap_or_else(|| "<no-info>".into());
            let e = by_name.entry(name).or_insert((0, c.payload_stride));
            e.0 += 1;
        }
        println!(
            "[comp-probe] === {label}: {} COMPs across sub-blocks, {} distinct types ===",
            inv.len(),
            by_name.len()
        );
        for (name, (count, stride)) in &by_name {
            println!(
                "[comp-probe]   {name:<32} x{count:<5} schm payload_stride={}",
                stride.map(|s| s.to_string()).unwrap_or_else(|| "?".into())
            );
        }
    }

    // ---- (b) Reverse-anchor search: scan EVERY COMP data blob for the anchor hashes ----------
    // A "hit" = an anchor u32 appears as a little-endian dword at any byte offset inside a COMP
    // data blob. Report the COMP type, the byte offset within the (4 + payload_stride) record,
    // and the record's leading u32 entity key.
    let anchor_set: std::collections::HashMap<u32, &str> =
        anchors.iter().map(|(h, n)| (*h, *n)).collect();

    for (label, blk) in [("layers_static(29)", &ls), ("interior_state(667)", &state)] {
        // Build a key->name / key->transform map for this block so we can name the owning entity.
        let placements = load_placements(blk).unwrap_or_default();
        let name_by_key: std::collections::HashMap<u32, String> = placements
            .iter()
            .filter_map(|p| p.name.clone().map(|n| (p.key, n)))
            .collect();
        let xform_by_key: std::collections::HashMap<u32, ([f32; 3], [f32; 4])> =
            placements.iter().map(|p| (p.key, (p.pos, p.quat))).collect();

        let inv: Vec<CompInfo> = comp_inventory(blk);
        let mut total_hits = 0usize;
        for c in &inv {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            if off + size > blk.len() {
                continue;
            }
            let blob = &blk[off..off + size];
            let stride = c.payload_stride.map(|s| s as usize + 4).unwrap_or(0);
            let mut i = 0usize;
            while i + 4 <= blob.len() {
                let v = u32::from_le_bytes([blob[i], blob[i + 1], blob[i + 2], blob[i + 3]]);
                if let Some(model_name) = anchor_set.get(&v) {
                    total_hits += 1;
                    let (rec_idx, field_off, key) = if stride > 0 {
                        let ri = i / stride;
                        let fo = i % stride;
                        let k = u32::from_le_bytes([
                            blob[ri * stride],
                            blob[ri * stride + 1],
                            blob[ri * stride + 2],
                            blob[ri * stride + 3],
                        ]);
                        (ri as isize, fo as isize, k)
                    } else {
                        (-1, -1, 0)
                    };
                    let ename = name_by_key.get(&key).cloned().unwrap_or_else(|| "<unknown>".into());
                    println!(
                        "[comp-probe] ANCHOR HIT in {label}: COMP='{}' hash=0x{v:08X} ({model_name}) \
                         at data+{i} (record {rec_idx}, field_off={field_off}, stride={stride}) \
                         entity_key=0x{key:08X} name='{ename}'",
                        c.info_name.as_deref().unwrap_or("<no-info>")
                    );
                }
                i += 4;
            }
        }
        if total_hits == 0 {
            println!("[comp-probe] {label}: NO anchor model hash found verbatim in any COMP data blob");
        }

        // ---- The name->mesh link: the "ModelName" COMP (stride-4 u32 = pandemic_hash_m2(model
        // name string), which equals the model ASET asset_hash). Dump records + resolve each. ----
        for c in inv.iter().filter(|c| c.info_name.as_deref() == Some("ModelName")) {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            if off + size > blk.len() {
                continue;
            }
            let stride = c.payload_stride.map(|s| s as usize + 4).unwrap_or(8);
            let blob = &blk[off..off + size];
            let n = blob.len() / stride.max(1);
            let mut resolved = 0usize;
            for r in 0..n {
                let base = r * stride;
                if base + 8 > blob.len() {
                    break;
                }
                let mesh = u32::from_le_bytes([blob[base + 4], blob[base + 5], blob[base + 6], blob[base + 7]]);
                if wad::extract_container(&mut w, mesh).is_ok() {
                    resolved += 1;
                }
            }
            println!(
                "[comp-probe] {label}: ModelName COMP (sub_block {}) — {n} records stride={stride}, \
                 {resolved} resolve via extract_container (val == pandemic_hash_m2(model-name) == model ASET hash)",
                c.sub_block
            );
            for r in 0..n.min(6) {
                let base = r * stride;
                if base + 8 > blob.len() {
                    break;
                }
                let key = u32::from_le_bytes([blob[base], blob[base + 1], blob[base + 2], blob[base + 3]]);
                let mesh = u32::from_le_bytes([blob[base + 4], blob[base + 5], blob[base + 6], blob[base + 7]]);
                let loads = wad::extract_container(&mut w, mesh).is_ok();
                let ename = name_by_key.get(&key).cloned().unwrap_or_else(|| "<unknown>".into());
                println!(
                    "[comp-probe]     rec[{r}] key=0x{key:08X} modelhash=0x{mesh:08X} \
                     placement_name='{ename}' extract_container={}",
                    if loads { "OK" } else { "miss" }
                );
            }
        }

        // ---- Direct Model-COMP dump: any COMP whose info name is "Model" (stride-4 u32 handles) ----
        for c in inv.iter().filter(|c| c.info_name.as_deref() == Some("Model")) {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            if off + size > blk.len() {
                continue;
            }
            let stride = c.payload_stride.map(|s| s as usize + 4).unwrap_or(8);
            let blob = &blk[off..off + size];
            let n = blob.len() / stride.max(1);
            println!(
                "[comp-probe] {label}: Model COMP (sub_block {}) — {n} records, stride={stride}",
                c.sub_block
            );
            for r in 0..n.min(8) {
                let base = r * stride;
                if base + 8 > blob.len() {
                    break;
                }
                let key = u32::from_le_bytes([blob[base], blob[base + 1], blob[base + 2], blob[base + 3]]);
                let mesh = u32::from_le_bytes([blob[base + 4], blob[base + 5], blob[base + 6], blob[base + 7]]);
                let loads = wad::extract_container(&mut w, mesh).is_ok();
                let ename = name_by_key.get(&key).cloned().unwrap_or_else(|| "<unknown>".into());
                println!(
                    "[comp-probe]     rec[{r}] key=0x{key:08X} mesh=0x{mesh:08X} \
                     name='{ename}' extract_container={}",
                    if loads { "OK" } else { "miss" }
                );
            }
        }

        // ---- (e) end-to-end proof: pick the first ModelName record whose mesh loads, resolve fully ----
        'proof: for c in inv.iter().filter(|c| c.info_name.as_deref() == Some("ModelName")) {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            if off + size > blk.len() {
                continue;
            }
            let stride = c.payload_stride.map(|s| s as usize + 4).unwrap_or(8);
            let blob = &blk[off..off + size];
            let n = blob.len() / stride.max(1);
            for r in 0..n {
                let base = r * stride;
                if base + 8 > blob.len() {
                    break;
                }
                let key = u32::from_le_bytes([blob[base], blob[base + 1], blob[base + 2], blob[base + 3]]);
                let mesh = u32::from_le_bytes([blob[base + 4], blob[base + 5], blob[base + 6], blob[base + 7]]);
                let Ok(container) = wad::extract_container(&mut w, mesh) else { continue };
                let Ok((verts, indices, draws, _stats)) =
                    mesh::build_indexed_from_container(&container)
                else {
                    continue;
                };
                let ename = name_by_key.get(&key).cloned().unwrap_or_else(|| "<unknown>".into());
                let (pos, quat) = xform_by_key.get(&key).cloned().unwrap_or(([0.0; 3], [0.0, 0.0, 0.0, 1.0]));
                println!(
                    "[comp-probe] *** END-TO-END PROOF ({label}) ***\n\
                     [comp-probe]   entity_key = 0x{key:08X}\n\
                     [comp-probe]   name       = '{ename}'\n\
                     [comp-probe]   Model.mesh = 0x{mesh:08X}\n\
                     [comp-probe]   loaded     = {} verts / {} tris / {} draw groups\n\
                     [comp-probe]   transform  = pos ({:.2},{:.2},{:.2}) yaw {:.3} rad",
                    verts.len(),
                    indices.len() / 3,
                    draws.len(),
                    pos[0], pos[1], pos[2],
                    yaw_from_quat(&quat)
                );
                break 'proof;
            }
        }
    }

    // ---- (d) c3-vs-placement for exterior buildings -----------------------------------------
    // Take a named exterior building placement, find the c3 cell covering its XZ, load that cell's
    // mesh, and report vert/tri counts — i.e. is the building baked into the c3 cell geometry?
    let placements = load_placements(&ls).unwrap_or_default();
    let bld = placements.iter().find(|p| {
        p.name.as_deref().map(|n| {
            let l = n.to_ascii_lowercase();
            l.contains("_bld_") && !l.contains("pmcoutpost")
        }).unwrap_or(false)
    });
    match bld {
        Some(p) => {
            println!(
                "[comp-probe] (d) exterior building sample: name='{}' key=0x{:08X} pos=({:.1},{:.1},{:.1})",
                p.name.as_deref().unwrap_or(""), p.key, p.pos[0], p.pos[1], p.pos[2]
            );
            // Is this building's name resolvable to a model ASET by hash (placement path)?
            let h = mercs2_formats::hash::pandemic_hash_m2(p.name.as_deref().unwrap_or(""));
            let by_name = wad::extract_container(&mut w, h).is_ok();
            println!(
                "[comp-probe] (d)   pandemic_hash_m2(name)=0x{h:08X} extract_container(name)={}",
                if by_name { "OK" } else { "miss (name is NOT a model hash)" }
            );
            // Load the c3 mesh cell nearest the building's XZ and report its geometry: if the
            // building is baked into the cell, that cell carries substantial vert/tri geometry.
            use mercs2_formats::ucfx::parse_block_entry_table;
            let c3: Vec<(u16, u32)> = wad::block_paths(&w)
                .iter()
                .enumerate()
                .filter_map(|(i, p)| c3_cell_id_from_path(p).map(|cid| (i as u16, cid)))
                .collect();
            let target = [p.pos[0], p.pos[2]];
            let mut best: Option<(f32, u16, u32, f32, f32)> = None;
            for &(blk, cid) in &c3 {
                let (cx, cz) = c3_cell_centre(cid);
                let d2 = (cx - target[0]).powi(2) + (cz - target[1]).powi(2);
                if best.map_or(true, |b| d2 < b.0) {
                    best = Some((d2, blk, cid, cx, cz));
                }
            }
            if let Some((d2, blk, cid, cx, cz)) = best {
                if let Ok(dec) = wad::decompress_block_index(&mut w, blk) {
                    let (count, entries) = parse_block_entry_table(&dec);
                    let mut pos = 4 + count as usize * 16;
                    let mut vt = 0usize;
                    let mut tt = 0usize;
                    let mut has_model = false;
                    for e in &entries {
                        let end = pos + e.chunk_size as usize;
                        if e.type_hash == wad::MODEL_TYPE_HASH && end <= dec.len() {
                            has_model = true;
                            if let Ok((v, i, _d, _s)) = mesh::build_indexed_from_container(&dec[pos..end]) {
                                vt += v.len();
                                tt += i.len() / 3;
                            }
                        }
                        pos = end;
                    }
                    println!(
                        "[comp-probe] (d)   nearest c3 cell {cid} (block {blk}) centre=({cx:.0},{cz:.0}) \
                         dist={:.0}m: has_model={has_model}, {vt} verts / {tt} tris \
                         => exterior buildings ARE baked into c3 cell geometry (not placed via ModelName)",
                        d2.sqrt()
                    );
                }
            }
        }
        None => println!("[comp-probe] (d) no non-PMC *_bld_* placement found in layers_static"),
    }
    // Report how many distinct model-format c3 blocks exist (the baked-geometry cells, format 0x5B724250).
    let model_paths: Vec<(usize, &String)> = wad::block_paths(&w)
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            let l = p.to_ascii_lowercase();
            l.contains("\\c3") || l.contains("/c3") || l.starts_with("c3")
        })
        .collect();
    println!(
        "[comp-probe] (d) c3 cell blocks in WAD path table = {} (exterior world geometry is baked per-cell)",
        model_paths.len()
    );

    let _ = MODEL_COMP_HASH; // documented constant (== wad::MODEL_TYPE_HASH); layers_static COMPs key by info-name string.
    Ok(())
}


/// Print `block_index` + path for every WAD block whose PTHS path contains `needle`
/// (case-insensitive). Was the inline `--block-grep` dispatch probe.
pub fn block_grep(wadpath: &str, needle: &str) {
    let needle = needle.to_lowercase();
    match wad::open(wadpath) {
        Ok(w) => {
            let mut n = 0;
            for (i, p) in wad::block_paths(&w).iter().enumerate() {
                if needle.is_empty() || p.to_lowercase().contains(&needle) {
                    println!("block={i:<5} {p}");
                    n += 1;
                }
            }
            println!("[block-grep] {n} blocks match '{needle}'");
        }
        Err(e) => eprintln!("--block-grep failed: {e}"),
    }
}

/// Report where given `hashes` appear (LE u32) in low_res_terrain (3121) + layers_static (29),
/// annotating the owning COMP + surrounding words in layers_static. Was the inline `--scan-hash`.
pub fn scan_hash(wadpath: &str, hashes: &[u32]) {
    if let Ok(mut w) = wad::open(wadpath) {
        if let Ok((low, ls)) = find_terrain_blocks(&mut w) {
            for (label, blk) in [("low_res_terrain(3121)", &low), ("layers_static(29)", &ls)] {
                for &want in hashes {
                    let mut hits = Vec::new();
                    let mut i = 0usize;
                    while i + 4 <= blk.len() {
                        if u32::from_le_bytes([blk[i], blk[i + 1], blk[i + 2], blk[i + 3]]) == want {
                            hits.push(i);
                        }
                        i += 1;
                    }
                    println!("[scan-hash] {label}: 0x{want:08X} -> {} hits {:?}", hits.len(), &hits.iter().take(6).collect::<Vec<_>>());
                    if label.starts_with("layers") && !hits.is_empty() {
                        // Which COMP owns the first hit?
                        for c in mercs2_formats::placement::comp_inventory(blk) {
                            if let (Some(o), Some(s)) = (c.data_off, c.data_size) {
                                if hits[0] >= o && hits[0] < o + s {
                                    println!("[scan-hash]     -> owning COMP: {:?} (sub_block {}, data@{o}+{s}, schm_stride={:?})", c.info_name, c.sub_block, c.payload_stride);
                                }
                            }
                        }
                    }
                    if label.starts_with("layers") {
                        for &h in hits.iter().take(1) {
                            let lo = h.saturating_sub(16);
                            for j in 0..12 {
                                let o = lo + j * 4;
                                if o + 4 <= blk.len() {
                                    let u = u32::from_le_bytes([blk[o], blk[o + 1], blk[o + 2], blk[o + 3]]);
                                    let f = f32::from_le_bytes([blk[o], blk[o + 1], blk[o + 2], blk[o + 3]]);
                                    let mark = if o == h { " <<< hash" } else { "" };
                                    println!("[scan-hash]     @{o}: u32=0x{u:08X} f32={f:.2}{mark}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Scan EVERY block for a given hex hash (LE u32) and report which blocks reference it (skips the
/// huge terrainmesh/geom blocks to bound cost). Was the inline `--find-ref` dispatch probe.
pub fn find_ref(wadpath: &str, wants: &[u32]) {
    if let Ok(mut w) = wad::open(wadpath) {
        let nblocks = wad::block_paths(&w).len();
        for &want in wants {
            let mut hits = 0;
            for bi in 0..nblocks as u16 {
                let Ok(dec) = wad::decompress_block_index(&mut w, bi) else { continue };
                if dec.len() > 6_000_000 { continue; } // skip huge terrainmesh/geom blocks
                let mut i = 0usize;
                let mut found = false;
                while i + 4 <= dec.len() {
                    if u32::from_le_bytes([dec[i], dec[i + 1], dec[i + 2], dec[i + 3]]) == want {
                        found = true;
                        break;
                    }
                    i += 1;
                }
                if found {
                    let path = wad::block_paths(&w).get(bi as usize).cloned().unwrap_or_default();
                    println!("[find-ref] 0x{want:08X} in block={bi} {path}");
                    hits += 1;
                    if hits >= 10 { break; }
                }
            }
            println!("[find-ref] 0x{want:08X}: {hits} block(s) reference it");
        }
    }
}

/// Decompress a block and list its chunk-entry table (type_hash, name_hash, size) + any textures'
/// dimensions. Was the inline `--block-probe <index>` dispatch probe.
pub fn block_probe(wadpath: &str, bi: u16) {
    if let Ok(mut w) = wad::open(wadpath) {
        match wad::decompress_block_index(&mut w, bi) {
            Ok(dec) => {
                let path = wad::block_paths(&w).get(bi as usize).cloned().unwrap_or_default();
                println!("[block-probe] block={bi} {path} ({} B decompressed)", dec.len());
                let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
                println!("[block-probe] {count} entries:");
                for (i, e) in entries.iter().enumerate() {
                    let tex = wad::extract_texture(&mut w, e.name_hash).ok();
                    let tinfo = tex
                        .map(|t| format!("  TEX {}x{} fmt={:?} mips={}", t.width, t.height, t.format, t.mip_count))
                        .unwrap_or_default();
                    println!(
                        "[block-probe]   [{i}] type=0x{:08X} name=0x{:08X} size={}{tinfo}",
                        e.type_hash, e.name_hash, e.chunk_size
                    );
                }
            }
            Err(e) => eprintln!("[block-probe] decompress failed: {e}"),
        }
    }
}

/// The ~62k layers_static placements have a Name but mostly no ModelName. Count name frequency
/// (instancing) + check which resolve to a real model via the name-hash recipe. Was `--placement-names`.
pub fn placement_names(wadpath: &str) {
    if let Ok(mut w) = wad::open(wadpath) {
        if let Ok((_low, ls)) = find_terrain_blocks(&mut w) {
            let places = mercs2_formats::placement::load_placements(&ls).unwrap_or_default();
            let mn: std::collections::HashSet<u32> = mercs2_formats::placement::load_model_placements(&ls).iter().map(|p| p.key).collect();
            let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            let mut no_mn_with_name = 0usize;
            for p in &places {
                if let Some(n) = &p.name {
                    let base = n.trim_start_matches('_').to_string();
                    *freq.entry(base).or_insert(0) += 1;
                    if !mn.contains(&p.key) { no_mn_with_name += 1; }
                }
            }
            println!("[pnames] {} placements, {} distinct base names; {} have a Name but NO ModelName", places.len(), freq.len(), no_mn_with_name);
            // Full coverage: distinct names + total placements whose name-hash resolves.
            let (mut names_ok, mut places_ok) = (0usize, 0usize);
            for (name, count) in &freq {
                let h = mercs2_formats::hash::pandemic_hash_m2(name);
                if wad::extract_container(&mut w, h).is_ok() {
                    names_ok += 1;
                    places_ok += count;
                }
            }
            let total_named: usize = freq.values().sum();
            println!("[pnames] RESOLVE via name-hash: {names_ok}/{} distinct names; {places_ok}/{total_named} placements ({:.0}%)", freq.len(), 100.0 * places_ok as f32 / total_named.max(1) as f32);
            // Variant test: why don't the big instances (plantlarge/rockhuge) resolve? Try
            // hash-fn + name-form variants for a few non-resolving high-count names.
            let probes = ["jungle_env_plantlarge04", "Jungle_env_rockhuge01", "global_env_rocksbeach03", "global_lamppostA"];
            for name in probes {
                let lc = name.to_ascii_lowercase();
                let variants: [(&str, u32); 5] = [
                    ("m2(name)", mercs2_formats::hash::pandemic_hash_m2(name)),
                    ("hash(name)", mercs2_formats::hash::pandemic_hash(name)),
                    ("m2(lower)", mercs2_formats::hash::pandemic_hash_m2(&lc)),
                    ("hash(lower)", mercs2_formats::hash::pandemic_hash(&lc)),
                    ("m2(name.mesh)", mercs2_formats::hash::pandemic_hash_m2(&format!("{name}.mesh"))),
                ];
                let hit: Vec<&str> = variants.iter().filter(|(_, h)| wad::extract_container(&mut w, *h).is_ok()).map(|(l, _)| *l).collect();
                let aset = wad::aset_types(&w, mercs2_formats::hash::pandemic_hash_m2(name));
                println!("[pnames]   variant {name}: resolves via {hit:?}; ASET(m2) types={aset:?}");
            }
            let mut top: Vec<(String, usize)> = freq.into_iter().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            for (name, count) in top.iter().take(30) {
                let h = mercs2_formats::hash::pandemic_hash_m2(name);
                let resolves = wad::extract_container(&mut w, h).is_ok();
                println!("[pnames]   x{count:<5} {name:<40} 0x{h:08X} -> {}", if resolves { "MODEL" } else { "-" });
            }
        }
    }
}

/// UCFX overlay blocks that may carry the interior entities' Transform / ModelName COMPs:
/// 29 (layers_static), 667 (vz_state_pmcinterior), and the state variants 703/711/461/291
/// (`_hel/_jet/_mec/_mecabsent`). Used by `entity_find`.
const INTERIOR_CANDIDATE_BLOCKS: &[u16] = &[29, 667, 703, 711, 461, 291];

/// One resolved interior entity: its authored Transform (from the block that keyed it), the mesh
/// hash + which source (ModelName COMP vs name→hash fallback), and the container geometry stats.
struct FoundEntity {
    key: u32,
    name: &'static str,
    /// (block, pos, quat) of the winning Transform record (first block that carried one).
    transform: Option<(u16, [f32; 3], [f32; 4])>,
    /// (model_hash, source, block) of the resolved mesh. Source "ModelName" = keyed COMP record;
    /// "name-hash" = `pandemic_hash_m2(name)` fallback.
    model: Option<(u32, String, u16)>,
    /// (verts, tris, local bbox min, local bbox max) if `extract_container` + build succeeded.
    container: Option<(usize, usize, [f32; 3], [f32; 3])>,
}
