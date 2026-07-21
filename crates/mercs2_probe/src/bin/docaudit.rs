//! THROWAWAY doc-audit probe (2026-07-21): measures the concrete numeric claims in
//! `docs/placement_data_format.md` and `docs/comprehensive_engine_understanding.md`
//! against retail `vz.wad`. Nothing here is engine code — it only counts and dumps.
//!
//! Usage: docaudit <ffcs|layers|census|vzstate|hash> [wadpath]

use mercs2_formats::{ffcs, hash::pandemic_hash_m2, placement, sges, ucfx};
use std::collections::BTreeMap;
use std::fs::File;

const DEFAULT_WAD: &str =
    "C:/Users/Shadow/Desktop/notes-on-the-released-game/game-files/vz.wad";

fn open_archive(path: &str) -> (ffcs::FfcsArchive, File, u64) {
    let mut f = File::open(path).unwrap_or_else(|e| panic!("open {path}: {e}"));
    let size = f.metadata().unwrap().len();
    let a = ffcs::load_ffcs_archive(&mut f, size).unwrap();
    (a, f, size)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "ffcs".into());
    let wadpath = args.next().unwrap_or_else(|| DEFAULT_WAD.into());
    match cmd.as_str() {
        "ffcs" => ffcs_report(&wadpath),
        "layers" => layers_report(&wadpath),
        "census" => census(&wadpath),
        "vzstate" => vzstate(&wadpath),
        "hash" => hashes(),
        "part2" => {
            let which = std::env::args().nth(3).unwrap_or_else(|| "layers".into());
            part2(&wadpath, &which)
        }
        "lrterrain" => lrterrain(&wadpath),
        "part3" => part3(&wadpath),
        "part4" => part4(&wadpath),
        "part5" => part5(&wadpath),
        "part6" => part6(&wadpath),
        other => eprintln!("unknown cmd {other}"),
    }
}

fn hashes() {
    for n in [
        "script", "model", "texture", "layer", "animation", "path", "terrainmesh",
        "lowresterrain", "effect", "wavebank", "soundbank", "binary", "font", "stringdb",
        "materialtable", "watermap", "foliage", "level", "ANY", "Transform", "Name",
    ] {
        println!("  {:<16} 0x{:08X}", n, pandemic_hash_m2(n));
    }
}

fn ffcs_report(wadpath: &str) {
    let (a, mut f, size) = open_archive(wadpath);
    println!("== FFCS {wadpath} ==");
    println!("file size        : {size} bytes ({:.3} GB)", size as f64 / 1e9);
    println!("endian           : {:?}", a.endian);
    println!("chunk rows       : {}", a.chunks.len());
    for r in &a.chunks {
        println!(
            "   {:<4} offset=0x{:08X} meta=0x{:08X} ({})",
            String::from_utf8_lossy(&r.tag),
            r.offset,
            r.meta,
            r.meta
        );
    }
    println!("INDX entries     : {}", a.indx.len());
    println!("ASET rows        : {}", a.aset.len());
    println!("PTHS paths       : {}", a.paths.len());

    // ASET type_id histogram
    let mut h: BTreeMap<u32, usize> = BTreeMap::new();
    for e in &a.aset {
        *h.entry(e.type_id).or_default() += 1;
    }
    println!("ASET distinct type_ids: {}", h.len());
    for (k, v) in &h {
        println!("   type_id {:>3}: {:>6}", k, v);
    }
    // sentinel stats for the +4/+8 claim
    let sec_ffff = a.aset.iter().filter(|e| e.secondary_ref == 0xFFFF_FFFF).count();
    let low_ffff = a.aset.iter().filter(|e| (e.packed_block_ref & 0xFFFF) == 0xFFFF).count();
    let hi_in_range = a
        .aset
        .iter()
        .filter(|e| ((e.packed_block_ref >> 16) as usize) < a.indx.len())
        .count();
    let lo_in_range = a
        .aset
        .iter()
        .filter(|e| {
            let lo = (e.packed_block_ref & 0xFFFF) as usize;
            lo != 0xFFFF && lo < a.indx.len()
        })
        .count();
    println!("ASET secondary_ref == 0xFFFFFFFF : {sec_ffff}");
    println!("ASET packed lo16 == 0xFFFF       : {low_ffff}");
    println!("ASET packed hi16 < INDX count    : {hi_in_range}");
    println!("ASET packed lo16 (non-sentinel) < INDX count : {lo_in_range} of {}", a.aset.len() - low_ffff);

    // sges header of block 0
    let head = sges::decompress_block_head(&mut f, &a.indx, 0, 64).unwrap();
    println!("block0 head bytes: {:02x?}", &head[..32.min(head.len())]);
    let _ = &mut f;
}

fn block_index_by_path(a: &ffcs::FfcsArchive, needle: &str) -> Vec<u16> {
    a.paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.to_ascii_lowercase().contains(needle))
        .map(|(i, _)| i as u16)
        .collect()
}

fn layers_report(wadpath: &str) {
    let (a, mut f, _) = open_archive(wadpath);
    let cands = block_index_by_path(&a, "layers_static");
    println!("layers_static blocks: {:?}", cands);
    for &b in &cands {
        println!("   [{b}] {}", a.paths[b as usize]);
    }
    let b = *cands
        .iter()
        .find(|&&b| a.paths[b as usize].to_ascii_lowercase().contains("p000"))
        .unwrap_or(&cands[0]);
    let dec = sges::decompress_block(&mut f, &a.indx, b).unwrap();
    println!("\n== block {b} decompressed: {} bytes ({:.2} MB) ==", dec.len(), dec.len() as f64 / 1048576.0);

    let (count, entries) = ucfx::parse_block_entry_table(&dec);
    println!("entry table count = {count}, parsed {} rows", entries.len());
    let mut th: BTreeMap<u32, usize> = BTreeMap::new();
    for e in &entries {
        *th.entry(e.type_hash).or_default() += 1;
    }
    println!("entry type_hash histogram: {:?}", th.iter().map(|(k, v)| (format!("0x{k:08X}"), *v)).collect::<Vec<_>>());
    println!("first 4 entries:");
    for e in entries.iter().take(4) {
        println!("   name=0x{:08X} type=0x{:08X} field_c=0x{:08X} size={}", e.name_hash, e.type_hash, e.field_c, e.chunk_size);
    }
    println!("raw first 48 bytes: {:02x?}", &dec[..48]);

    // UCFX sub-block count + does entry chunk_size match UCFX spacing?
    let mut ucfx_pos = Vec::new();
    let mut i = 0;
    while i + 4 <= dec.len() {
        if &dec[i..i + 4] == b"UCFX" {
            ucfx_pos.push(i);
            i += 4;
        } else {
            i += 1;
        }
    }
    println!("UCFX occurrences: {}", ucfx_pos.len());
    let header_end = 4 + count as usize * 16;
    println!("header_end = {header_end}; first UCFX at {}", ucfx_pos.first().copied().unwrap_or(0));
    let mut off = header_end;
    let mut mism = 0;
    for (i, e) in entries.iter().enumerate() {
        if ucfx_pos.get(i).copied() != Some(off) {
            if mism < 5 {
                println!("   MISMATCH entry {i}: expected UCFX at {off}, actual {:?}", ucfx_pos.get(i));
            }
            mism += 1;
        }
        off += e.chunk_size as usize;
    }
    println!("entry chunk_size vs UCFX spacing mismatches: {mism} / {}", entries.len());

    // COMP inventory
    let comps = placement::comp_inventory(&dec);
    println!("\ntotal COMPs: {}", comps.len());
    let mut names: BTreeMap<String, (usize, BTreeMap<u32, usize>, usize)> = BTreeMap::new();
    for c in &comps {
        let n = c.info_name.clone().unwrap_or_else(|| "<none>".into());
        let e = names.entry(n).or_default();
        e.0 += 1;
        if let Some(s) = c.payload_stride {
            *e.1.entry(s).or_default() += 1;
        }
        e.2 += c.data_size.unwrap_or(0);
    }
    println!("distinct COMP info names: {}", names.len());
    println!("{:<32} {:>5} {:>10} {:>12}  strides", "COMP", "n", "dataBytes", "recs@stride");
    let mut total_records = 0usize;
    for (n, (cnt, strides, bytes)) in &names {
        let stride_desc: Vec<String> = strides.iter().map(|(s, c)| format!("{s}x{c}")).collect();
        // record count if stride = 4 + payload_stride
        let rec = strides
            .keys()
            .next()
            .map(|s| {
                let st = 4 + *s as usize;
                if st > 0 { bytes / st } else { 0 }
            })
            .unwrap_or(0);
        if n != "Transform" && n != "Name" {
            total_records += rec;
        }
        println!("{:<32} {:>5} {:>10} {:>12}  {}", n, cnt, bytes, rec, stride_desc.join(","));
    }
    println!("(sum of non-Transform/Name records @ 4+schm stride: {total_records})");

    // Transform stride forensics
    println!("\n== Transform COMP stride forensics ==");
    let tf: Vec<&placement::CompInfo> = comps.iter().filter(|c| c.info_name.as_deref() == Some("Transform")).collect();
    println!("Transform COMPs: {}", tf.len());
    let sizes: Vec<usize> = tf.iter().filter_map(|c| c.data_size).collect();
    let total: usize = sizes.iter().sum();
    println!("total Transform data bytes: {total}");
    println!("schm payload_stride values: {:?}", tf.iter().filter_map(|c| c.payload_stride).collect::<std::collections::BTreeSet<_>>());
    for s in 1..=80usize {
        if sizes.iter().all(|&z| z % s == 0) {
            let n: usize = sizes.iter().map(|z| z / s).sum();
            println!("   stride {s:>3} divides EVERY Transform blob -> {n} records");
        }
    }

    // quaternion norm test at stride 42 layout vs alternatives
    for &(stride, qoff) in &[(42usize, 20usize), (42, 16), (56, 20), (56, 40)] {
        let mut n = 0usize;
        let mut good = 0usize;
        for c in &tf {
            let (o, s) = (c.data_off.unwrap(), c.data_size.unwrap());
            if s % stride != 0 { continue; }
            for r in 0..s / stride {
                let base = o + r * stride;
                if base + qoff + 16 > dec.len() { break; }
                let q: Vec<f32> = (0..4).map(|k| ffcs::read_f32_le(&dec, base + qoff + k * 4)).collect();
                let nrm = q.iter().map(|v| v * v).sum::<f32>();
                n += 1;
                if (nrm - 1.0).abs() < 1e-3 { good += 1; }
            }
        }
        println!("   stride {stride} quat@+{qoff}: {good}/{n} unit-norm ({:.2}%)", 100.0 * good as f64 / n.max(1) as f64);
    }

    // Full placement load
    let ps = placement::load_placements(&dec).unwrap();
    let named = ps.iter().filter(|p| p.name.is_some()).count();
    println!("\nload_placements: {} records, {} named, {} unnamed", ps.len(), named, ps.len() - named);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut tilted = 0usize;
    let mut bad_norm = 0usize;
    let mut pad_nonzero = 0usize;
    for p in &ps {
        for k in 0..3 {
            lo[k] = lo[k].min(p.pos[k]);
            hi[k] = hi[k].max(p.pos[k]);
        }
        let nrm: f32 = p.quat.iter().map(|v| v * v).sum();
        if (nrm - 1.0).abs() >= 1e-3 { bad_norm += 1; }
        if p.quat[0].abs() > 0.01 || p.quat[2].abs() > 0.01 { tilted += 1; }
    }
    // pad at +0x10 check
    for c in &tf {
        let (o, s) = (c.data_off.unwrap(), c.data_size.unwrap());
        for r in 0..s / 42 {
            let base = o + r * 42;
            if base + 20 > dec.len() { break; }
            if ffcs::read_f32_le(&dec, base + 16) != 0.0 { pad_nonzero += 1; }
        }
    }
    println!("pos X {:.1}..{:.1}  Y {:.1}..{:.1}  Z {:.1}..{:.1}", lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]);
    println!("quat non-unit: {bad_norm} / {}", ps.len());
    println!("tilted (|qx|>0.01 or |qz|>0.01): {tilted} ({:.1}%)", 100.0 * tilted as f64 / ps.len() as f64);
    println!("+0x10 pad float nonzero: {pad_nonzero}");
    let mut keys: Vec<u32> = ps.iter().map(|p| p.key).collect();
    keys.sort_unstable();
    let uniq = { keys.dedup(); keys.len() };
    println!("distinct entity keys among transforms: {uniq}");

    // first Transform record hex dump
    if let Some(c) = tf.first() {
        let o = c.data_off.unwrap();
        println!("\nfirst Transform blob @{o} size {} first 3 records:", c.data_size.unwrap());
        for r in 0..3 {
            println!("   {:02x?}", &dec[o + r * 42..o + (r + 1) * 42]);
        }
    }

    // LowResTerrainObject
    println!("\n== LowResTerrainObject ==");
    for c in comps.iter().filter(|c| c.info_name.as_deref() == Some("LowResTerrainObject")) {
        println!("   sub_block {} stride(schm)={:?} data_size={:?} -> {:?} recs @12",
            c.sub_block, c.payload_stride, c.data_size, c.data_size.map(|s| s / 12));
        if let (Some(o), Some(s)) = (c.data_off, c.data_size) {
            for r in 0..3.min(s / 12) {
                println!("      {:02x?}", &dec[o + r * 12..o + (r + 1) * 12]);
            }
        }
    }
    // sub-block COMP count range
    let mut per_sub: BTreeMap<u16, usize> = BTreeMap::new();
    for c in &comps { *per_sub.entry(c.sub_block).or_default() += 1; }
    let mn = per_sub.values().min().copied().unwrap_or(0);
    let mx = per_sub.values().max().copied().unwrap_or(0);
    println!("\nsub-blocks with COMPs: {} ; COMPs per sub-block: {}..{}", per_sub.len(), mn, mx);
}

fn census(wadpath: &str) {
    let (a, mut f, _) = open_archive(wadpath);
    let mut type_hist: BTreeMap<u32, usize> = BTreeMap::new();
    let mut entries_total = 0usize;
    let mut c3_blocks = 0usize;
    let mut c3_type: BTreeMap<u32, usize> = BTreeMap::new();
    let mut vz_state_blocks = 0usize;
    let mut class: BTreeMap<&str, usize> = BTreeMap::new();
    let mut failed = 0usize;
    for b in 0..a.indx.len() as u16 {
        let path = a.paths.get(b as usize).cloned().unwrap_or_default().to_ascii_lowercase();
        let cls = if path.contains("\\c3") || path.contains("/c3") {
            "c3"
        } else if path.contains("vz_state") {
            "vz_state"
        } else if path.contains("layers_static") {
            "layers_static"
        } else if path.contains("resident") {
            "resident"
        } else if path.contains("animgroup") {
            "animgroups"
        } else {
            "other"
        };
        *class.entry(cls).or_default() += 1;
        if cls == "c3" { c3_blocks += 1; }
        if cls == "vz_state" { vz_state_blocks += 1; }
        let head = match sges::decompress_block_head(&mut f, &a.indx, b, 0x20000) {
            Ok(h) => h,
            Err(_) => { failed += 1; continue; }
        };
        let (count, mut ents) = ucfx::parse_block_entry_table(&head);
        if (ents.len() as u32) < count {
            // table spans past the first segment: full decompress
            if let Ok(d) = sges::decompress_block(&mut f, &a.indx, b) {
                let (_, e2) = ucfx::parse_block_entry_table(&d);
                ents = e2;
            }
        }
        entries_total += ents.len();
        for e in &ents {
            *type_hist.entry(e.type_hash).or_default() += 1;
            if cls == "c3" { *c3_type.entry(e.type_hash).or_default() += 1; }
        }
        if b % 1000 == 0 { eprintln!("  .. block {b}"); }
    }
    println!("blocks scanned: {} (failed {failed})", a.indx.len());
    println!("block class counts: {:?}", class);
    println!("c3 blocks: {c3_blocks}  vz_state blocks: {vz_state_blocks}");
    println!("total UCFX entries: {entries_total}, distinct type_hash: {}", type_hist.len());
    let mut v: Vec<(u32, usize)> = type_hist.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (t, c) in &v {
        println!("   0x{:08X} {:>7}", t, c);
    }
    println!("\nc3 block type_hash histogram:");
    let mut v2: Vec<(u32, usize)> = c3_type.into_iter().collect();
    v2.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (t, c) in &v2 {
        println!("   0x{:08X} {:>7}", t, c);
    }
}

fn vzstate(wadpath: &str) {
    let (a, mut f, _) = open_archive(wadpath);
    let blocks = block_index_by_path(&a, "vz_state");
    println!("vz_state blocks: {}", blocks.len());
    // uniquify by asset stem
    let mut with_flgs = 0usize;
    let mut with_transform = 0usize;
    let mut total_tf_records = 0usize;
    let mut sample_done = false;
    let mut comp_names: BTreeMap<String, usize> = BTreeMap::new();
    for &b in &blocks {
        let dec = match sges::decompress_block(&mut f, &a.indx, b) { Ok(d) => d, Err(_) => continue };
        let has_flgs = dec.windows(4).any(|w| w == b"flgs");
        if has_flgs { with_flgs += 1; }
        let comps = placement::comp_inventory(&dec);
        for c in &comps {
            *comp_names.entry(c.info_name.clone().unwrap_or_else(|| "<none>".into())).or_default() += 1;
        }
        let tf: Vec<&placement::CompInfo> = comps.iter().filter(|c| c.info_name.as_deref() == Some("Transform")).collect();
        if !tf.is_empty() {
            with_transform += 1;
            for c in &tf {
                if let Some(s) = c.data_size { total_tf_records += s / 42; }
            }
        }
        if !sample_done && a.paths[b as usize].to_ascii_lowercase().contains("pmccon004") {
            sample_done = true;
            println!("\n== sample {} (block {b}) ==", a.paths[b as usize]);
            println!("decompressed {} bytes; first 32: {:02x?}", dec.len(), &dec[..32.min(dec.len())]);
            let (cnt, ents) = ucfx::parse_block_entry_table(&dec);
            println!("entry count {cnt}");
            for e in ents.iter().take(4) {
                println!("   name=0x{:08X} type=0x{:08X} field_c=0x{:08X} size={}", e.name_hash, e.type_hash, e.field_c, e.chunk_size);
            }
            for c in &comps {
                println!("   COMP {:<28} stride={:?} size={:?}", c.info_name.clone().unwrap_or_default(), c.payload_stride, c.data_size);
            }
            if let Some(c) = tf.first() {
                let (o, s) = (c.data_off.unwrap(), c.data_size.unwrap());
                println!("   Transform blob {} bytes, {} recs@42; first 3:", s, s / 42);
                for r in 0..3.min(s / 42) {
                    println!("      {:02x?}", &dec[o + r * 42..o + (r + 1) * 42]);
                }
            }
            if let Ok(ps) = placement::load_placements(&dec) {
                println!("   load_placements -> {} records", ps.len());
                for p in ps.iter().take(6) {
                    println!("      key=0x{:08X} name={:?} pos={:?}", p.key, p.name, p.pos);
                }
            }
        }
    }
    println!("\nvz_state blocks with a flgs chunk: {with_flgs} / {}", blocks.len());
    println!("vz_state blocks with a Transform COMP: {with_transform}");
    println!("total vz_state Transform records @42: {total_tf_records}");
    println!("\nvz_state COMP name histogram (top 30):");
    let mut v: Vec<(String, usize)> = comp_names.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (n, c) in v.iter().take(30) {
        println!("   {:<32} {}", n, c);
    }
}

// ---------------------------------------------------------------- part 2

/// Exact-divisibility test of every COMP data blob by `4 + schm payload_stride`,
/// ECS coverage of transform keys, Name record shape, and the flgs/flgt/enum rows.
pub fn part2(wadpath: &str, which: &str) {
    let (a, mut f, _) = open_archive(wadpath);
    let blocks: Vec<u16> = if which == "layers" {
        block_index_by_path(&a, "layers_static")
    } else {
        block_index_by_path(&a, which)
    };
    for &b in blocks.iter().take(3) {
        let dec = sges::decompress_block(&mut f, &a.indx, b).unwrap();
        println!("== block {b} {} ({} bytes) ==", a.paths[b as usize], dec.len());
        let comps = placement::comp_inventory(&dec);
        // exact divisibility
        let mut exact = 0usize;
        let mut inexact: Vec<String> = Vec::new();
        for c in &comps {
            let (Some(s), Some(sz)) = (c.payload_stride, c.data_size) else { continue };
            let st = 4 + s as usize;
            if sz % st == 0 { exact += 1; } else {
                inexact.push(format!("{}({} % {} = {})", c.info_name.clone().unwrap_or_default(), sz, st, sz % st));
            }
        }
        println!("COMP blobs exactly divisible by 4+schm_stride: {exact}/{}", comps.len());
        println!("  NOT divisible ({}): {:?}", inexact.len(), &inexact[..inexact.len().min(12)]);

        // ECS coverage: transform keys with >=1 record in another COMP
        let mut tkeys: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for c in comps.iter().filter(|c| c.info_name.as_deref() == Some("Transform")) {
            let (o, sz) = (c.data_off.unwrap(), c.data_size.unwrap());
            for r in 0..sz / 42 { tkeys.insert(ffcs::read_u32_le(&dec, o + r * 42)); }
        }
        let mut covered: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut ecs_records = 0usize;
        for c in &comps {
            let n = c.info_name.clone().unwrap_or_default();
            if n == "Transform" || n == "Name" { continue; }
            let (Some(s), Some(o), Some(sz)) = (c.payload_stride, c.data_off, c.data_size) else { continue };
            let st = 4 + s as usize;
            if sz % st != 0 { continue; }
            for r in 0..sz / st {
                let k = ffcs::read_u32_le(&dec, o + r * st);
                ecs_records += 1;
                if tkeys.contains(&k) { covered.insert(k); }
            }
        }
        println!("transform keys {} ; ECS records {} ; keys with >=1 ECS comp {} ({:.1}%)",
            tkeys.len(), ecs_records, covered.len(), 100.0 * covered.len() as f64 / tkeys.len().max(1) as f64);

        // Name record shape: does embedded 0xHEX equal the u32 key?
        if let Some(c) = comps.iter().find(|c| c.info_name.as_deref() == Some("Name")) {
            let (o, sz) = (c.data_off.unwrap(), c.data_size.unwrap());
            println!("Name blob first 96 bytes: {:02x?}", &dec[o..o + 96.min(sz)]);
            println!("  as text: {:?}", String::from_utf8_lossy(&dec[o..o + 96.min(sz)]));
        }

        // flgs/flgt/enum rows in the first sub-block
        dump_nonComp_rows(&dec);

        // doc §3.3 heuristic: first 0x3f800000 minus 4, relative to the Transform blob start
        if let Some(c) = comps.iter().find(|c| c.info_name.as_deref() == Some("Transform")) {
            let (o, sz) = (c.data_off.unwrap(), c.data_size.unwrap());
            let blob = &dec[o..o + sz];
            if let Some(p) = blob.windows(4).position(|w| w == [0x00, 0x00, 0x80, 0x3f]) {
                let start = p.saturating_sub(4);
                println!("doc §3.3 heuristic (first 1.0f - 4) lands at Transform-blob offset {start} => start mod 42 = {}", start % 42);
                println!("  => doc record start = real record start + {} bytes", start % 42);
            } else {
                println!("doc §3.3 heuristic: no 1.0f in Transform blob");
            }
        }
        println!();
    }
}

fn dump_nonComp_rows(dec: &[u8]) {
    // walk the first UCFX's CHDR table manually and print non-COMP rows
    let Some(u) = dec.windows(4).position(|w| w == b"UCFX") else { return };
    let Some(cp) = dec[u..(u + 4096).min(dec.len())].windows(4).position(|w| w == b"CHDR") else { return };
    let chdr = u + cp;
    let n = ffcs::read_u32_le(dec, chdr + 12) as usize;
    let mut pos = chdr + 20;
    println!("CHDR rows = {n}");
    for _ in 0..n {
        if pos + 20 > dec.len() { break; }
        let tag = String::from_utf8_lossy(&dec[pos..pos + 4]).into_owned();
        let nc = ffcs::read_u32_le(dec, pos + 16) as usize;
        if tag != "COMP" {
            println!("   row {:<6} f4=0x{:08X} f8=0x{:08X} f12=0x{:08X} children={}",
                tag, ffcs::read_u32_le(dec, pos + 4), ffcs::read_u32_le(dec, pos + 8),
                ffcs::read_u32_le(dec, pos + 12), nc);
        }
        pos += 20 + nc * 20;
    }
}

/// lrterrain mesh_hash cross-check against the low_res_terrain block's entry table.
pub fn lrterrain(wadpath: &str) {
    let (a, mut f, _) = open_archive(wadpath);
    let lb = block_index_by_path(&a, "low_res_terrain");
    println!("low_res_terrain blocks: {:?}", lb.iter().map(|&b| (b, a.paths[b as usize].clone())).collect::<Vec<_>>());
    let mut tile_hashes: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut n_entries = 0;
    for &b in &lb {
        let d = sges::decompress_block(&mut f, &a.indx, b).unwrap();
        let (_, e) = ucfx::parse_block_entry_table(&d);
        n_entries += e.len();
        for x in &e { tile_hashes.insert(x.name_hash); }
    }
    println!("low_res_terrain entries: {n_entries}, distinct name_hash: {}", tile_hashes.len());
    let ls = block_index_by_path(&a, "layers_static")[0];
    let dec = sges::decompress_block(&mut f, &a.indx, ls).unwrap();
    let comps = placement::comp_inventory(&dec);
    for c in comps.iter().filter(|c| c.info_name.as_deref() == Some("LowResTerrainObject")) {
        let (o, sz) = (c.data_off.unwrap(), c.data_size.unwrap());
        let mut hit = 0;
        let mut miss = Vec::new();
        let mut ids = Vec::new();
        for r in 0..sz / 12 {
            let mh = ffcs::read_u32_le(&dec, o + r * 12 + 4);
            let id = ffcs::read_u32_le(&dec, o + r * 12 + 8);
            ids.push(id);
            if tile_hashes.contains(&mh) { hit += 1; } else { miss.push((r, mh)); }
        }
        println!("LowResTerrainObject mesh_hash in low_res_terrain TOC: {hit}/{}", sz / 12);
        println!("   misses: {:?}", &miss[..miss.len().min(6)]);
        let seq = ids.windows(2).all(|w| w[1] == w[0] + 1);
        println!("   3rd field strictly sequential (+1): {seq}; first={:?} last={:?}", ids.first(), ids.last());
    }
}

/// c3 block sub-type census + "no placement tags in c3" test + named-block inventory.
pub fn part3(wadpath: &str) {
    let (a, mut f, _) = open_archive(wadpath);
    let mut c3_by_typeset: BTreeMap<String, usize> = BTreeMap::new();
    let mut c3_tiny = 0usize;
    let mut named: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // stem -> (blocks, entries)
    let mut csum_ok = 0usize;
    let mut csum_bad = 0usize;
    let mut csum_chunks = 0usize;
    let mut csum_blocks = 0usize;
    let mut c3_with_comp = 0usize;
    let mut c3_sampled = 0usize;
    for b in 0..a.indx.len() as u16 {
        let path = a.paths.get(b as usize).cloned().unwrap_or_default();
        let low = path.to_ascii_lowercase();
        let stem = low
            .rsplit(|ch| ch == '\u{5c}' || ch == '/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".block")
            .to_string();
        let head = match sges::decompress_block_head(&mut f, &a.indx, b, 0x20000) { Ok(h) => h, Err(_) => continue };
        let (count, mut ents) = ucfx::parse_block_entry_table(&head);
        if (ents.len() as u32) < count {
            if let Ok(d) = sges::decompress_block(&mut f, &a.indx, b) {
                let (_, e2) = ucfx::parse_block_entry_table(&d);
                ents = e2;
            }
        }
        let key: String = {
            let mut ts: Vec<u32> = ents.iter().map(|e| e.type_hash).collect();
            ts.sort_unstable(); ts.dedup();
            ts.iter().map(|t| format!("{t:08X}")).collect::<Vec<_>>().join("+")
        };
        let is_c3 = low.contains("\u{5c}c3") || low.contains("/c3");
        if is_c3 {
            *c3_by_typeset.entry(key.clone()).or_default() += 1;
            if ents.len() <= 1 { c3_tiny += 1; }
            // sample every 200th c3 block for COMP/flgs/enum tags (full decompress)
            if b % 200 == 0 {
                if let Ok(d) = sges::decompress_block(&mut f, &a.indx, b) {
                    c3_sampled += 1;
                    let hit = d.windows(4).any(|w| w == b"COMP" || w == b"flgs" || w == b"enum");
                    if hit { c3_with_comp += 1; }
                }
            }
        } else {
            let e = named.entry(stem.clone()).or_default();
            e.0 += 1;
            e.1 += ents.len();
        }
        // CSUM verify on a sample of blocks
        if b % 37 == 0 {
            if let Ok(d) = sges::decompress_block(&mut f, &a.indx, b) {
                csum_blocks += 1;
                let (pb, _issues) = ucfx::walk_decompressed_block(&d, "audit");
                for cont in &pb.containers {
                    if let Some(p) = cont.windows(4).rposition(|w| w == b"CSUM") {
                        if p + 12 <= cont.len() {
                            let stored = ffcs::read_u32_le(cont, p + 8);
                            let calc = mercs2_formats::crc32::crc32_mercs2(&cont[..p]);
                            csum_chunks += 1;
                            if stored == calc { csum_ok += 1; } else { csum_bad += 1; }
                        }
                    }
                }
            }
        }
    }
    println!("c3 blocks by entry-type-set:");
    let mut v: Vec<(String, usize)> = c3_by_typeset.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (k, c) in v.iter().take(15) { println!("   {:<40} {:>6}", k, c); }
    println!("c3 blocks with <=1 entry: {c3_tiny}");
    println!("c3 sampled for COMP/flgs/enum: {c3_sampled}, hits: {c3_with_comp}");
    println!("\nCSUM sample: {csum_blocks} blocks, {csum_chunks} chunks, ok={csum_ok} bad={csum_bad}");
    println!("\nnon-c3 named blocks (top 40 by entries):");
    let mut n: Vec<(String, (usize, usize))> = named.into_iter().collect();
    n.sort_by_key(|(_, (_, e))| std::cmp::Reverse(*e));
    for (s, (blk, ent)) in n.iter().take(40) { println!("   {:<48} blocks={:<3} entries={}", s, blk, ent); }
    // grouped prefixes
    println!("\nblocks whose stem starts with 'animgroup': {}",
        a.paths.iter().filter(|p| p.to_ascii_lowercase().contains("animgroup")).count());
    println!("blocks whose stem contains 'resident': {}",
        a.paths.iter().filter(|p| p.to_ascii_lowercase().contains("resident")).count());
    println!("blocks whose stem contains 'scripts': {}",
        a.paths.iter().filter(|p| p.to_ascii_lowercase().contains("scripts")).count());
    for p in a.paths.iter().filter(|p| p.to_ascii_lowercase().contains("scripts")) { println!("   {p}"); }
    for p in a.paths.iter().filter(|p| p.to_ascii_lowercase().contains("resident")).take(6) { println!("   R {p}"); }
}

/// CSUM verification across the whole WAD + UCFX/CHDR header shape + FFCS CSUM chunk peek.
pub fn part4(wadpath: &str) {
    let (a, mut f, size) = open_archive(wadpath);
    // UCFX / CHDR header shape from layers_static
    let ls = block_index_by_path(&a, "layers_static")[0];
    let dec = sges::decompress_block(&mut f, &a.indx, ls).unwrap();
    let (cnt, _e) = ucfx::parse_block_entry_table(&dec);
    let u0 = 4 + cnt as usize * 16;
    println!("layers_static first UCFX @{u0}: {:02x?}", &dec[u0..u0 + 64]);
    println!("  tag={:?} u4=0x{:08X} u8=0x{:08X} u12=0x{:08X} u16=0x{:08X}",
        String::from_utf8_lossy(&dec[u0..u0 + 4]),
        ffcs::read_u32_le(&dec, u0 + 4), ffcs::read_u32_le(&dec, u0 + 8),
        ffcs::read_u32_le(&dec, u0 + 12), ffcs::read_u32_le(&dec, u0 + 16));
    println!("  at +20: tag={:?} u4=0x{:08X} u8=0x{:08X} u12=0x{:08X} u16=0x{:08X}",
        String::from_utf8_lossy(&dec[u0 + 20..u0 + 24]),
        ffcs::read_u32_le(&dec, u0 + 24), ffcs::read_u32_le(&dec, u0 + 28),
        ffcs::read_u32_le(&dec, u0 + 32), ffcs::read_u32_le(&dec, u0 + 36));
    // COMP row children
    println!("  first COMP row @{}: {:02x?}", u0 + 40, &dec[u0 + 40..u0 + 40 + 20]);
    println!("  container tail 16: {:02x?}", &dec[u0 + 25202 - 16..u0 + 25202]);

    // FFCS CSUM chunk
    if let Some(row) = a.chunks.iter().find(|r| &r.tag == b"CSUM") {
        use std::io::{Read, Seek, SeekFrom};
        println!("\nFFCS CSUM row: offset=0x{:08X} ({}) meta={} ; file size {}",
            row.offset, row.offset, row.meta, size);
        println!("  offset within file: {}", (row.offset as u64) < size);
        let mut buf = [0u8; 64];
        f.seek(SeekFrom::Start(row.offset as u64)).unwrap();
        f.read_exact(&mut buf).unwrap();
        println!("  bytes @offset: {:02x?}", buf);
    }

    // CSUM verification over every block
    let mut blocks = 0usize;
    let mut chunks = 0usize;
    let mut with_csum = 0usize;
    let mut ok = 0usize;
    let mut bad = 0usize;
    for b in 0..a.indx.len() as u16 {
        let d = match sges::decompress_block(&mut f, &a.indx, b) { Ok(d) => d, Err(_) => continue };
        blocks += 1;
        let (pb, _issues) = ucfx::walk_decompressed_block(&d, "audit");
        for c in &pb.containers {
            chunks += 1;
            if c.len() >= 8 && &c[c.len() - 8..c.len() - 4] == b"CSUM" {
                with_csum += 1;
                let exp = ffcs::read_u32_le(c, c.len() - 4);
                if mercs2_formats::crc32::crc32_mercs2(&c[..c.len() - 8]) == exp { ok += 1; } else { bad += 1; }
            }
        }
        if b % 2000 == 0 { eprintln!("  .. block {b}"); }
    }
    println!("\nCSUM: {blocks} blocks, {chunks} containers, {with_csum} carry a CSUM trailer, ok={ok} bad={bad}");
}

/// lrterrain record-index → `lrterrain_rXX_cYY` name test, and vz_state size range.
pub fn part5(wadpath: &str) {
    let (a, mut f, _) = open_archive(wadpath);
    let ls = block_index_by_path(&a, "layers_static")[0];
    let dec = sges::decompress_block(&mut f, &a.indx, ls).unwrap();
    let ps = placement::load_placements(&dec).unwrap();
    let name_of: std::collections::HashMap<u32, String> =
        ps.iter().filter_map(|p| p.name.clone().map(|n| (p.key, n))).collect();
    let comps = placement::comp_inventory(&dec);
    for c in comps.iter().filter(|c| c.info_name.as_deref() == Some("LowResTerrainObject")) {
        let (o, sz) = (c.data_off.unwrap(), c.data_size.unwrap());
        let n = sz / 12;
        let mut rowmajor_ok = 0usize;
        let mut colmajor_ok = 0usize;
        let mut unnamed = 0usize;
        let mut ids = Vec::new();
        let mut samples = Vec::new();
        for i in 0..n {
            let k = ffcs::read_u32_le(&dec, o + i * 12);
            ids.push(ffcs::read_u32_le(&dec, o + i * 12 + 8));
            match name_of.get(&k) {
                Some(nm) => {
                    if i < 5 || (i > 18 && i < 23) { samples.push((i, nm.clone())); }
                    if *nm == format!("lrterrain_r{:02}_c{:02}", i / 20, i % 20)
                        || *nm == format!("lrterrain_r{}_c{}", i / 20, i % 20) { rowmajor_ok += 1; }
                    if *nm == format!("lrterrain_r{:02}_c{:02}", i % 20, i / 20)
                        || *nm == format!("lrterrain_r{}_c{}", i % 20, i / 20) { colmajor_ok += 1; }
                }
                None => unnamed += 1,
            }
        }
        println!("LowResTerrainObject {n} records; named {} unnamed {unnamed}", n - unnamed);
        println!("   samples: {:?}", samples);
        println!("   index==row-major (r=i/20,c=i%20): {rowmajor_ok}/{n}");
        println!("   index==col-major (r=i%20,c=i/20): {colmajor_ok}/{n}");
        let mono = ids.windows(2).all(|w| w[1] > w[0]);
        let step1 = ids.windows(2).filter(|w| w[1] == w[0] + 1).count();
        println!("   3rd field strictly increasing: {mono}; +1 steps {step1}/{}", n - 1);
    }
    // vz_state decompressed size range
    let vz = block_index_by_path(&a, "vz_state");
    let mut mn = usize::MAX;
    let mut mx = 0usize;
    let mut tot = 0usize;
    for &b in &vz {
        if let Ok(d) = sges::decompress_block(&mut f, &a.indx, b) {
            mn = mn.min(d.len()); mx = mx.max(d.len()); tot += d.len();
        }
    }
    println!("\nvz_state blocks {}: decompressed size {}..{} bytes, total {} ({:.2} MB)",
        vz.len(), mn, mx, tot, tot as f64 / 1048576.0);
}

/// Script-entry block spread, vz_state filename suffix taxonomy, and the five §3.4 example ids.
pub fn part6(wadpath: &str) {
    let (a, mut f, _) = open_archive(wadpath);
    let mut sb: std::collections::BTreeSet<u16> = Default::default();
    for e in a.aset.iter().filter(|e| e.type_id == 35) { sb.insert(e.block_index()); }
    println!("ASET script rows: {} across {} distinct blocks",
        a.aset.iter().filter(|e| e.type_id == 35).count(), sb.len());

    let mut suffix: BTreeMap<&str, usize> = BTreeMap::new();
    for p in a.paths.iter().filter(|p| p.to_ascii_lowercase().contains("vz_state")) {
        let l = p.to_ascii_lowercase();
        for s in ["_pristine", "_destroyed", "_staging", "_defenses", "_captured",
                  "chi", "pir", "gur", "oil", "all", "pmc", "vza"] {
            if l.contains(s) { *suffix.entry(s).or_default() += 1; }
        }
    }
    println!("vz_state filename tokens: {:?}", suffix);

    let want: [u32; 5] = [0x0012b37b, 0x0012d4f1, 0x001313f9, 0x00131407, 0x0013140d];
    let mut found: BTreeMap<u32, (String, String)> = BTreeMap::new();
    let targets: Vec<u16> = block_index_by_path(&a, "vz_state")
        .into_iter().chain(block_index_by_path(&a, "layers_static")).collect();
    for &b in &targets {
        let Ok(d) = sges::decompress_block(&mut f, &a.indx, b) else { continue };
        let Ok(ps) = placement::load_placements(&d) else { continue };
        for p in &ps {
            if want.contains(&p.key) {
                found.entry(p.key).or_insert((a.paths[b as usize].clone(),
                    p.name.clone().unwrap_or_default()));
            }
        }
    }
    println!("doc §3.4 example ids resolved:");
    for k in want { println!("   0x{:08x} -> {:?}", k, found.get(&k)); }
}
