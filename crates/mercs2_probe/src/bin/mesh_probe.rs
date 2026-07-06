//! Dev bin: dump a model container's draw-group STATE machinery (SEGM/SWIT/STAT) so we can see why
//! groups are kept/skipped by `build_indexed_state`, and which state a baked-in prop group needs.
//! Cross-references docs/ucfx_tag_registry.md §3/§6.
//!
//!   cargo run -p mercs2_engine --bin mesh_probe -- 0x39AF17DC

use mercs2_engine::wad;
use std::collections::BTreeMap;

fn u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Walk the UCFX descriptor rows; return `(tag, is_marker, data_start, size)` for each row.
fn rows(c: &[u8]) -> Vec<([u8; 4], bool, usize, usize)> {
    let mut out = Vec::new();
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return out;
    }
    let data_off = u32_le(c, 4) as usize;
    let n_desc = u32_le(c, 16) as usize;
    let max = c.len().saturating_sub(20) / 20;
    for i in 0..n_desc.min(max) {
        let ro = 20 + i * 20;
        let tag = [c[ro], c[ro + 1], c[ro + 2], c[ro + 3]];
        let u0 = u32_le(c, ro + 4);
        let size = u32_le(c, ro + 8) as usize;
        if u0 == 0xFFFF_FFFF {
            out.push((tag, true, 0, 0));
        } else {
            out.push((tag, false, data_off + u0 as usize, size));
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mhash = args.get(1).and_then(|a| a.strip_prefix("0x")).and_then(|h| u32::from_str_radix(h, 16).ok()).unwrap_or(0x39AF17DC);
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    let c = wad::extract_container(&mut w, mhash).expect("extract container");
    println!("model 0x{mhash:08X}: {} bytes", c.len());

    // 1. Chunk tag histogram.
    let rs = rows(&c);
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    for (tag, _m, _s, _sz) in &rs {
        *hist.entry(String::from_utf8_lossy(tag).to_string()).or_default() += 1;
    }
    println!("chunk tags: {:?}", hist);

    // 2. SWIT / STAT raw dumps (state machinery), with rainbow-table reverse of each hash.
    let mut state_hashes: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for want in [b"SWIT", b"STAT"] {
        for (tag, marker, start, size) in &rs {
            if tag == want && !marker && start + size <= c.len() {
                for k in 0..(size / 4) {
                    state_hashes.insert(u32_le(&c, start + k * 4));
                }
            }
        }
    }
    let state_names = mercs2_engine::worldutil::rainbow_names(&state_hashes);
    for want in [b"SWIT", b"STAT"] {
        let mut seen: std::collections::BTreeSet<Vec<u32>> = std::collections::BTreeSet::new();
        for (tag, marker, start, size) in &rs {
            if tag == want && !marker && start + size <= c.len() {
                let vals: Vec<u32> = (0..(size / 4)).map(|k| u32_le(&c, start + k * 4)).collect();
                if !seen.insert(vals.clone()) {
                    continue; // dedup repeated STAT records
                }
                let show: Vec<String> = vals.iter().map(|h| {
                    let n = state_names.get(h).map(|s| s.as_str()).unwrap_or("?");
                    format!("0x{h:08X}({n})")
                }).collect();
                println!("{} : [{}]", String::from_utf8_lossy(want), show.join(", "));
            }
        }
    }

    // 3. SEGM records: {bone, seg_id, state_mask/group}. Distribution of the 4th byte.
    let segs = mercs2_formats::model_cubeize::parse_segm(&c);
    let mut mask_hist: BTreeMap<u8, usize> = BTreeMap::new();
    for s in &segs {
        *mask_hist.entry(s.state_mask).or_default() += 1;
    }
    println!("SEGM: {} records; byte3 (state_mask/group) distribution: {:?}", segs.len(), mask_hist);

    // 4. Per draw group: state_mask + material slot0 texture NAME, so we see which state the props need.
    let meshes = mercs2_formats::model_cubeize::read_model_meshes(&c).unwrap_or_default();
    let group_mat = mercs2_formats::texture::group_material_indices(&c);
    let mats = mercs2_formats::texture::parse_mtrl(&c);

    // MULTI-MATERIAL groups: a PRMG can carry several PRMT records, each a sub-strip with its OWN
    // material. group_material_indices() keeps only the FIRST — so a floor sub-strip sharing a group
    // with e.g. a wall would render with the wall material. Dump every group's FULL material list.
    let group_all = mercs2_formats::texture::group_prmt_material_indices(&c);
    let multi = group_all.iter().filter(|l| l.len() > 1).count();
    println!("== {} materials total; {} of {} groups are MULTI-material ==", mats.len(), multi, group_all.len());
    for (gi, list) in group_all.iter().enumerate() {
        let hashes: Vec<String> = list.iter().map(|&mi|
            mats.get(mi).and_then(|mt| mt.textures.first().copied())
                .map(|h| format!("0x{h:08X}")).unwrap_or_else(|| "-".into())).collect();
        println!("  group {gi}: mat_idx={:?} tex0={:?}", list, hashes);
    }

    // After the multi-material fix: build_indexed_state emits one draw per PRMT sub-strip material.
    // Dump the resulting draw count + the distinct diffuse texture NAMES actually bound (the floor
    // material should now appear).
    if let Ok((_v, _i, draws, s)) = mercs2_engine::mesh::build_indexed_from_container(&c) {
        println!("== prelit (baked vertex lighting): {} ==", s.prelit);
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
        for d in &draws {
            if let Some(h) = d.diffuse {
                *counts.entry(h).or_default() += 1;
            }
        }
        println!(
            "== build_indexed_state: {} draw groups; {} distinct diffuse textures (game loads via extract_texture_hires — FAIL = white/blank draw) ==",
            draws.len(), counts.len()
        );
        for (h, cnt) in &counts {
            let nm = {
                let (ar, fl) = wad::archive_and_file(&mut w);
                mercs2_formats::texture::extract_texture_name(fl, ar, *h).unwrap_or_else(|| format!("0x{h:08X}"))
            };
            let status = match wad::extract_texture_hires(&mut w, *h) {
                Ok(t) => format!("{}x{} {:?}", t.width, t.height, t.format),
                Err(e) => format!("*** HIRES FAIL ({e}) -> WHITE"),
            };
            println!("   {cnt:2} x {nm:<42} {status}");
        }
    }
    println!("== {} draw groups (group: sub-object seg_mask -> material slot0 name) ==", meshes.len());
    let (archive, file) = wad::archive_and_file(&mut w);
    // Cache names to avoid re-extracting.
    let mut name_cache: BTreeMap<u32, String> = BTreeMap::new();
    let mut by_mask: BTreeMap<u8, Vec<String>> = BTreeMap::new();
    for m in &meshes {
        let mi = group_mat.get(m.group_index).copied().unwrap_or(usize::MAX);
        let tex0 = mats.get(mi).and_then(|mt| mt.textures.first().copied());
        let name = tex0
            .map(|h| {
                name_cache
                    .entry(h)
                    .or_insert_with(|| {
                        mercs2_formats::texture::extract_texture_name(file, archive, h)
                            .unwrap_or_else(|| format!("0x{h:08X}"))
                    })
                    .clone()
            })
            .unwrap_or_else(|| "<no-mtrl>".into());
        by_mask.entry(m.state_mask).or_default().push(name);
    }
    // Per-group STAT (state hash) in descriptor order, zipped to group_index.
    let stat_by_order: Vec<u32> = rs.iter()
        .filter(|(t, m, s, sz)| t == b"STAT" && !*m && s + sz <= c.len())
        .map(|(_, _, s, _)| u32_le(&c, *s))
        .collect();
    println!("== per RENDERED group: STAT state | SEGM mask | material ==");
    for m in &meshes {
        if m.state_mask != 0 && (m.state_mask & 0x01) == 0 {
            continue; // not rendered at 0x01
        }
        let mi = group_mat.get(m.group_index).copied().unwrap_or(usize::MAX);
        let name = mats.get(mi).and_then(|mt| mt.textures.first().copied())
            .map(|h| name_cache.get(&h).cloned().unwrap_or_else(|| format!("0x{h:08X}")))
            .unwrap_or_else(|| "<no-mtrl>".into());
        let stat = stat_by_order.get(m.group_index).copied().unwrap_or(0);
        let sname = state_names.get(&stat).map(|s| s.as_str()).unwrap_or("?");
        println!("  group {:2}: STAT 0x{stat:08X}({sname:>14})  mask 0x{:02X}  {name}", m.group_index, m.state_mask);
    }

    for (mask, names) in &by_mask {
        let active = if *mask == 0 || (*mask & 0x01) != 0 { "RENDERED@0x01" } else { "skipped@0x01" };
        println!("  state_mask 0x{mask:02X} [{active}] ({} groups):", names.len());
        // dedup + count names
        let mut counts: BTreeMap<&String, usize> = BTreeMap::new();
        for n in names {
            *counts.entry(n).or_default() += 1;
        }
        for (n, ct) in counts {
            println!("      {ct:2}x  {n}");
        }
    }
}
