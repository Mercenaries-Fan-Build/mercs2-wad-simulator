//! Dev bin: how many LightObject dynamic lights do we actually harvest, and how many are near the PMC
//! villa spawn? Diagnoses "no interior lights showing".
//!   cargo run -p mercs2_engine --bin light_probe

use mercs2_engine::{wad, worldutil};
use mercs2_formats::placement::light_inventory;

const SPAWN: [f32; 3] = [3794.0427, 450.7505, -3911.0322];

fn d2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y, z) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    x * x + y * y + z * z
}

fn dump(tag: &str, block: &[u8]) {
    let lights = light_inventory(block);
    let near = lights.iter().filter(|l| d2(l.pos, SPAWN) < 200.0 * 200.0).count();
    println!("[{tag}] {} LightObject lights ({near} within 200 m of the villa spawn)", lights.len());
    for l in lights.iter().filter(|l| d2(l.pos, SPAWN) < 200.0 * 200.0).take(10) {
        println!(
            "   {:<28} pos [{:8.1},{:7.1},{:8.1}] type {} color ({:.2},{:.2},{:.2}) params {:?}",
            l.name.clone().unwrap_or_default(),
            l.pos[0], l.pos[1], l.pos[2], l.light.light_type,
            l.light.color[0], l.light.color[1], l.light.color[2],
            &l.light.params[..4]
        );
    }
}

fn main() {
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    {
        // The WAD's OWN name->block index (PTHS): enumerate every interior/hq/vz_state block path so we
        // can drive the interior loader from data instead of hardcoded block numbers.
        let paths = wad::block_paths(&w);
        println!("== interior/hq/vz_state block PATHS ({} total blocks) ==", paths.len());
        for (i, p) in paths.iter().enumerate() {
            let pl = p.to_ascii_lowercase();
            if pl.contains("animgroup") || pl.contains("charactername") {
                println!("   block {i:5}: {p}");
            }
        }
    }
    // Which merc animgroup contains each of the 3 CURRENT player clips? (are they Mattias's or Jenn's?)
    for (label, blk) in [("mattias", 3154u16), ("chris", 3278), ("jennifer", 3362)] {
        if let Ok(dec) = wad::decompress_block_index(&mut w, blk) {
            print!("[animgroup {label:<9} blk{blk}] current player clips present:");
            for (cn, ch) in [("idle", 0x24F8C8E6u32), ("walk", 0x53682784), ("run", 0x867B166D)] {
                let n = ch.to_le_bytes();
                let cnt = dec.windows(4).filter(|w| *w == n).count();
                if cnt > 0 {
                    print!(" {cn}(0x{ch:08X} x{cnt})");
                }
            }
            println!();
        }
    }
    // CHRIS (block 3278) full clip table — for matching the live-captured idle (Chris is loaded in
    // x32dbg). Sorted by poses so a captured numPoses (LtSampleWave [ecx+0x28]) pinpoints the clip.
    if let Ok(cb) = wad::decompress_block_index(&mut w, 3278) {
        if let Ok(cg) = mercs2_formats::animgroup::parse_animgroup(&cb) {
            let mut cc: Vec<&_> = cg.clips.iter().collect();
            // LtSampleWave [ecx+0x58] = 8642 is the playing clip's data base offset in the animgroup.
            // Sort by havok_offset and flag the clip whose packfile owns offset 8642.
            cc.sort_by_key(|c| c.havok_offset);
            println!("[chris] {} clips (hash | tt | poses | dur | havok_offset) — captured base=8642:", cg.clips.len());
            for c in &cc {
                let mark = if c.havok_offset <= 8642 && 8642 < c.havok_offset + 4000 { " <== near 8642" } else { "" };
                println!("   0x{:08X}  {:3}tt  {:3} poses  {:.2}s  @0x{:X} ({}){}", c.name_hash, c.num_transform_tracks, c.num_poses, c.duration, c.havok_offset, c.havok_offset, mark);
            }
            // Match the LIVE-captured quant data (distinctive stretch e00d/e01f/e011) to the block -> clip.
            let needle: [u8; 16] = [0xe0,0x0d,0xe0,0x1d,0xe0,0x05,0xe0,0x1f,0xe0,0x1d,0xe0,0x05,0xe0,0x11,0xe0,0x1d];
            if let Some(pos) = cb.windows(needle.len()).position(|w| w == needle) {
                let mut by: Vec<&_> = cg.clips.iter().collect();
                by.sort_by_key(|c| c.havok_offset);
                let owner = by.iter().rev().find(|c| c.havok_offset <= pos);
                println!("[chris] captured quant data @ block 0x{pos:X}");
                if let Some(c) = owner {
                    println!("[chris] ==> CHRIS IDLE = 0x{:08X}  ({}tt {} poses {:.2}s, packfile @0x{:X})", c.name_hash, c.num_transform_tracks, c.num_poses, c.duration, c.havok_offset);
                }
            } else {
                println!("[chris] captured quant data NOT found in block 3278");
            }
            // Chris WALK/RUN via footstep annotations: bucket each footstep_* string offset to its clip.
            let mut by: Vec<&_> = cg.clips.iter().collect();
            by.sort_by_key(|c| c.havok_offset);
            for (tag, needle) in [("WALK", b"footstep_walk".as_slice()), ("RUN", b"footstep_run".as_slice())] {
                let mut clips: std::collections::BTreeSet<u32> = Default::default();
                let mut start = 0usize;
                while start + needle.len() <= cb.len() {
                    match cb[start..].windows(needle.len()).position(|w| w == needle) {
                        Some(p) => {
                            let off = start + p;
                            if let Some(c) = by.iter().rev().find(|c| c.havok_offset <= off) {
                                clips.insert(c.name_hash);
                            }
                            start = off + needle.len();
                        }
                        None => break,
                    }
                }
                let list: Vec<String> = clips.iter().map(|h| format!("0x{h:08X}")).collect();
                println!("[chris] {tag} clips ({}): {:?}", clips.len(), list);
            }
        }
    }

    // Find MATTIAS's idle: Jennifer's idle is 0x24F8C8E6; match it to Mattias's block 3154 by parallel
    // clip index AND by profile (same transform-track count + duration).
    {
        let jen = wad::decompress_block_index(&mut w, 3362)
            .ok()
            .and_then(|b| mercs2_formats::animgroup::parse_animgroup(&b).ok());
        let mat = wad::decompress_block_index(&mut w, 3154)
            .ok()
            .and_then(|b| mercs2_formats::animgroup::parse_animgroup(&b).ok());
        if let (Some(jen), Some(mat)) = (jen, mat) {
            println!("[animset] jennifer clips={} mattias clips={}", jen.clips.len(), mat.clips.len());
            if let Some((ji, jc)) = jen.clips.iter().enumerate().find(|(_, c)| c.name_hash == 0x24F8C8E6) {
                println!(
                    "[animset] Jennifer idle 0x24F8C8E6 @ index {ji}: {}tt {:.2}s {} poses",
                    jc.num_transform_tracks, jc.duration, jc.num_poses
                );
                if let Some(mc) = mat.clips.get(ji) {
                    println!(
                        "[animset] Mattias clip @ same index {ji}: 0x{:08X} {}tt {:.2}s {} poses (parallel-index idle candidate)",
                        mc.name_hash, mc.num_transform_tracks, mc.duration, mc.num_poses
                    );
                }
                // No role in the entry table (type_hash = Havok class, field_c = 0). The idle is a LONG,
                // low-motion clip — list Mattias's clips by duration; the longest non-64-track-locomotion
                // one is the idle candidate (verify against the live Mattias).
                let mut byd: Vec<&_> = mat.clips.iter().collect();
                byd.sort_by(|a, b| b.duration.total_cmp(&a.duration));
                println!("[animset] Mattias's 12 longest clips (idle candidate = longest):");
                for c in byd.iter().take(12) {
                    println!("   0x{:08X}  {}tt  {:.2}s  {} poses", c.name_hash, c.num_transform_tracks, c.duration, c.num_poses);
                }
            }
        }
    }
    let (_low, ls) = worldutil::find_terrain_blocks(&mut w).expect("layers_static");
    dump("layers_static", &ls);
    for blk in [667u16, 461, 291, 703, 711] {
        match wad::decompress_block_index(&mut w, blk) {
            Ok(dec) => dump(&format!("block {blk}"), &dec),
            Err(e) => println!("[block {blk}] decompress failed: {e}"),
        }
    }

    // Block-667 named furniture: POSITION + distance from the player spawn (which room?) + whether it
    // resolves. Props far from the spawn are in the main hall / recruit bays, not the initial room.
    if let Ok(dec) = wad::decompress_block_index(&mut w, 667) {
        let pls = mercs2_formats::placement::load_placements(&dec).unwrap_or_default();
        println!("== block 667 named placements: pos + dist from spawn {:?} ==", SPAWN);
        let mut rows: Vec<(f32, String)> = Vec::new();
        for p in &pls {
            let Some(raw) = p.name.as_deref() else { continue };
            let base = raw.split(" 0x").next().unwrap_or(raw).trim_start_matches('_');
            if base.is_empty() {
                continue;
            }
            let h = mercs2_formats::hash::pandemic_hash_m2(base);
            let ok = wad::extract_container(&mut w, h).is_ok();
            let dist = d2(p.pos, SPAWN).sqrt();
            rows.push((
                dist,
                format!(
                    "   {base:<30} pos [{:7.1},{:6.1},{:8.1}] dist {dist:6.1}m  {}",
                    p.pos[0], p.pos[1], p.pos[2], if ok { "resolves" } else { "MISS" }
                ),
            ));
        }
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, line) in &rows {
            println!("{line}");
        }
    }

    // REAL interior light params: the placement records carry 0 (values arrive at runtime), so the
    // authoritative color/intensity/radius live in each Light_small_* TEMPLATE asset. Extract them and
    // dump the LightObject values directly — no guessing.
    for (nm, h) in [
        ("Light_small_blue", 0x9DDE617Au32),
        ("Light_small_blue_dim", 0x8E4C0966),
        ("Light_small_darkblue", 0xA86337B4),
        ("Light_small_yellow", 0xD965157E),
        ("Light_small_yellow_dim", 0x1419DF50),
        ("Light_small_white", 0xBE090CAF),
        ("Light_small_orange", 0x192CEFA8),
    ] {
        match wad::extract_container(&mut w, h) {
            Ok(c) => {
                let li = mercs2_formats::placement::light_inventory(&c);
                println!("[template {nm} 0x{h:08X}] {} bytes, {} LightObject(s)", c.len(), li.len());
                for l in &li {
                    println!(
                        "    type {} color ({:.3},{:.3},{:.3}) params {:?}",
                        l.light.light_type, l.light.color[0], l.light.color[1], l.light.color[2], l.light.params
                    );
                }
            }
            Err(e) => println!("[template {nm} 0x{h:08X}] extract FAILED: {e}"),
        }
    }

    // Interior STRUCTURE meshes (stockpile/sickbay/scaffold/money/recruit) — are we missing them?
    // The registry lists 17 _pmcoutpost_interior_*; we only hardcode the hall. Try resolving each the
    // furniture way: pandemic_hash_m2(name without leading '_') -> extract_container -> build.
    for name in [
        "pmcoutpost_interior_stockpile",
        "pmcoutpost_interior_sickbay",
        "pmcoutpost_interior_scaffold",
        "pmcoutpost_interior_money",
        "pmcoutpost_interior_money_a",
        "pmcoutpost_interior_recruitmechanic",
        "pmcoutpost_interior_recruitjet",
        "pmcoutpost_interior_recruitheli",
        "pmcoutpost_interior_hq",
    ] {
        let h = mercs2_formats::hash::pandemic_hash_m2(name);
        let status = match wad::extract_container(&mut w, h) {
            Ok(c) => match mercs2_engine::mesh::build_indexed_from_container(&c) {
                Ok((v, _, d, _)) => format!("OK {} verts / {} draws", v.len(), d.len()),
                Err(e) => format!("BUILD FAIL: {e}"),
            },
            Err(_) => "MISS (no container)".to_string(),
        };
        println!("[interior-mesh] {name:<38} 0x{h:08X} -> {status}");
    }

    // Does the interior TEMPLATE / actor container ENUMERATE its structure meshes (the data-driven part
    // list)? Extract each template and search its bytes for the structure mesh hashes.
    {
        let struct_hashes = [
            ("hall", 0x39AF17DCu32),
            ("sickbay", 0x757EAE95),
            ("scaffold", 0x1FBFBB4B),
            ("stockpile_nameH", 0x2258BD11),
            ("recruitmech", 0xE8EB75D7),
            ("recruitjet", 0x86D7CF92),
            ("recruitheli", 0x634F1F65),
        ];
        for (label, th) in [
            ("HqInterior", mercs2_formats::hash::pandemic_hash_m2("HqInterior")),
            ("_proutpost_interior_job", 0x39394E37u32),
            ("proutpost_interior_job", mercs2_formats::hash::pandemic_hash_m2("proutpost_interior_job")),
            ("AllHq_Interior", 0xC8EF281E),
        ] {
            match wad::extract_container(&mut w, th) {
                Ok(c) => {
                    print!("[template {label:<24} 0x{th:08X}] {} bytes; references:", c.len());
                    let mut any = false;
                    for (nm, sh) in struct_hashes {
                        let n = sh.to_le_bytes();
                        let cnt = c.windows(4).filter(|w| *w == n).count();
                        if cnt > 0 {
                            print!(" {nm}(x{cnt})");
                            any = true;
                        }
                    }
                    println!("{}", if any { "" } else { " NONE" });
                }
                Err(e) => println!("[template {label:<24} 0x{th:08X}] extract FAIL: {e}"),
            }
        }
    }

    // ARCHITECTURE: does the base layer block 667 REFERENCE the structure mesh hashes (data-driven), or
    // are they only in the undecoded HqInterior actor template? Search the decompressed block for each.
    if let Ok(dec) = wad::decompress_block_index(&mut w, 667) {
        println!("== structure mesh hashes referenced in block 667? ==");
        for (nm, h) in [
            ("hall", 0x39AF17DCu32),
            ("sickbay", 0x757EAE95),
            ("scaffold", 0x1FBFBB4B),
            ("recruitmechanic", 0xE8EB75D7),
            ("recruitjet", 0x86D7CF92),
            ("recruitheli", 0x634F1F65),
        ] {
            let needle = h.to_le_bytes();
            let found: Vec<usize> = dec
                .windows(4)
                .enumerate()
                .filter(|(_, w)| *w == needle)
                .map(|(i, _)| i)
                .collect();
            println!("   {nm:<16} 0x{h:08X}: {} occurrence(s) {:?}", found.len(), &found[..found.len().min(4)]);
        }
    }

    // Baked-lighting check: does the PMC hall carry non-white vertex COLORS (static lightmap baked to
    // verts, the Pandemic-era interior lighting) — or flat white (no baked light)?
    for (name, hash) in [
        ("PMC hall", 0x39AF17DCu32),
        ("lamppostmilitary", 0xA6AD0346),
        ("wardrobe", 0x8AAA90D1),
        ("Mattias(player)", 0xA3C1FABC),
    ] {
        if let Some((m, _, _)) = mercs2_engine::game_world::load_model_by_hash_state(&mut w, hash, 0x01) {
            let n = m.verts.len().max(1) as f32;
            let mut cmn = [1.0f32; 3];
            let mut cmx = [0.0f32; 3];
            let mut sum = [0.0f32; 3];
            let mut nonwhite = 0usize;
            // NATIVE model-space bbox (positions are pre-fit): height/footprint in game units.
            let mut pmn = [f32::MAX; 3];
            let mut pmx = [f32::MIN; 3];
            for v in &m.verts {
                for c in 0..3 {
                    cmn[c] = cmn[c].min(v.color[c]);
                    cmx[c] = cmx[c].max(v.color[c]);
                    sum[c] += v.color[c];
                    pmn[c] = pmn[c].min(v.pos[c]);
                    pmx[c] = pmx[c].max(v.pos[c]);
                }
                if v.color.iter().any(|&c| c < 0.95) {
                    nonwhite += 1;
                }
            }
            println!(
                "[{name} 0x{hash:08X}] {} verts  bbox X {:.2} Y {:.2} Z {:.2} (native units)  color mean({:.2},{:.2},{:.2}) {} non-white ({:.0}%)",
                m.verts.len(), pmx[0] - pmn[0], pmx[1] - pmn[1], pmx[2] - pmn[2],
                sum[0] / n, sum[1] / n, sum[2] / n, nonwhite, 100.0 * nonwhite as f32 / n
            );
        }
    }

    // Locomotion: derive the player's walk/run GROUND SPEED from each clip's baked root stride
    // (what world.rs now uses instead of the hardcoded 2.2/6.5 m/s).
    if let Ok(container) = wad::extract_container(&mut w, 0xA3C1FABC) {
        if let Ok((_v, _i, _d, stats)) = mercs2_engine::mesh::build_indexed_from_container(&container) {
            let hier: Vec<u32> = stats.rig.iter().map(|b| b.name_hash).collect();
            let wanted = [0x24F8_C8E6u32, 0x5368_2784, 0x867B_166D];
            let names = ["idle", "walk", "run"];
            for (found, (h, nm)) in mercs2_engine::game_world::load_clips_for_rig(&mut w, &hier, &wanted)
                .into_iter()
                .zip(wanted.iter().zip(names))
            {
                if let Some(ca) = found {
                    let d = ca.clip.duration.max(1e-3);
                    let sp = mercs2_engine::pose::clip_root_speed(
                        &stats.rig,
                        &ca.clip.sample_local(0.0),
                        &ca.clip.sample_local(d * 0.999),
                        &ca.track_to_hier,
                        ca.num_transform_tracks,
                        d * 0.999,
                    );
                    println!("[locomotion] {nm} 0x{h:08X}: dur {d:.2}s -> {sp:.2} m/s (stride {:.2} m)", sp * d);
                }
            }
        }
    }
}
