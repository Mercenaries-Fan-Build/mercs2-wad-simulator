//! Dev bin: list every `global_particle_*` FX placement in the PMC interior state block (667),
//! then dump the effect for each distinct effect name (extract its UCFX container from the effects
//! block and print the typed tree `mercs2_formats::fxdict::parse_effect_container` reads: shapes,
//! emitters with their TRFM channels / PTYP attributes / COLR / TEXT, forces). This pins what the
//! interior loader classifies + which effects are skipped as "unsupported" (godray / lightshaft).
//!   cargo run -p mercs2_probe --bin fx_probe

use mercs2_engine::wad;
use mercs2_engine::worldutil::PMC_INTERIOR_STATE_BLOCK;
use mercs2_formats::fxdict::{attribute_name, parse_effect_container, Atrb, AtrbValue};
use mercs2_formats::hash::{pandemic_hash, pandemic_hash_m2};
use mercs2_formats::placement::load_placements;
use mercs2_formats::types::TYPE_HASH_EFFECT;

fn ru32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn dump_effect(w: &mut wad::Wad, name: &str) {
    println!("\n=== effect template: {name} ===");
    let candidates = [
        ("pandemic_hash_m2", pandemic_hash_m2(name)),
        ("pandemic_hash", pandemic_hash(name)),
    ];
    let mut container: Option<Vec<u8>> = None;
    for (which, h) in candidates {
        match wad::extract_container_typed(w, h, TYPE_HASH_EFFECT) {
            Ok(c) => {
                println!("  resolved via {which}(0x{h:08X}), container {} bytes", c.len());
                container = Some(c);
                break;
            }
            Err(e) => println!("  {which}=0x{h:08X}: {e}"),
        }
    }
    let Some(c) = container else {
        println!("  (could not resolve effect container by name hash) — scanning ASET table:");
        for (which, h) in candidates {
            let hits = wad::aset_types(w, h);
            println!("    {which}=0x{h:08X}: {} ASET hits (type_id,primary,block)={hits:?}", hits.len());
        }
        return;
    };
    dump_typed(&c, name);
}

fn main() {
    let mut w = wad::resolve_vz_wad(None).and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    let dec = wad::decompress_block_index(&mut w, PMC_INTERIOR_STATE_BLOCK)
        .expect("decompress interior state block 667");
    let placements = load_placements(&dec).unwrap_or_default();
    println!("block {PMC_INTERIOR_STATE_BLOCK}: {} placements total", placements.len());

    let mut distinct: Vec<String> = Vec::new();
    let mut n = 0usize;
    for p in &placements {
        let raw = p.name.as_deref().unwrap_or("");
        let name = raw.split(" 0x").next().unwrap_or(raw).trim_start_matches('_');
        if name.starts_with("global_particle") {
            n += 1;
            println!(
                "  [{n:2}] {:<52} pos [{:9.2},{:9.2},{:9.2}] quat [{:+.3},{:+.3},{:+.3},{:+.3}] sub {} key=0x{:08X} raw={:?}",
                name, p.pos[0], p.pos[1], p.pos[2],
                p.quat[0], p.quat[1], p.quat[2], p.quat[3], p.sub_block, p.key, raw
            );
            if !distinct.iter().any(|d| d == name) {
                distinct.push(name.to_string());
            }
        }
    }
    println!("total global_particle_* placements: {n}; distinct effects: {}", distinct.len());
    // The placement entity references its effect via an EffectTemplate COMP (opaque dword = the
    // effect-block name_hash). Dump every COMP in block 667 whose type-name mentions "effect".
    println!("\n[interior EffectTemplate COMPs in block 667]");
    let mut effect_dwords: Vec<u32> = Vec::new();
    for c in mercs2_formats::placement::comp_inventory(&dec) {
        let nm = c.info_name.clone().unwrap_or_default();
        let low = nm.to_ascii_lowercase();
        if low.contains("effect") || low.contains("emitter") || low.contains("redeffect") {
            if let (Some(off), Some(sz)) = (c.data_off, c.data_size) {
                let body = &dec[off..(off + sz).min(dec.len())];
                let words: Vec<u32> = (0..body.len() / 4).map(|i| ru32(body, i * 4)).collect();
                println!("  COMP {nm:<22} stride={:?} sub={} {} bytes words={:08X?}",
                    c.payload_stride, c.sub_block, sz, &words[..words.len().min(16)]);
                for w in words { if w > 0xFFFF { effect_dwords.push(w); } }
            }
        }
    }

    // Find the effects block(s) by path name and scan their UCFX entry table for our effect hashes.
    // The effect template name = placement name with "particle_" removed (verified: placement
    // `global_particle_env_godray2` -> effect `global_env_godray2`, m2=0xDB331999).
    let targets: Vec<(String, u32, u32)> = distinct.iter()
        .flat_map(|n| {
            let alt = n.replace("particle_", "");
            vec![
                (n.clone(), pandemic_hash_m2(n), pandemic_hash(n)),
                (alt.clone(), pandemic_hash_m2(&alt), pandemic_hash(&alt)),
            ]
        })
        .collect();
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    for (blk, path) in paths.iter().enumerate() {
        let lp = path.to_ascii_lowercase();
        if !(lp.contains("effect") || lp.contains("resident")) {
            continue;
        }
        let Ok(dec) = wad::decompress_block_index(&mut w, blk as u16) else { continue };
        let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
        let n_eff = entries.iter().filter(|e| e.type_hash == TYPE_HASH_EFFECT).count();
        if n_eff == 0 { continue; }
        println!("\n[effects-block] blk {blk} path={path:?}: {count} entries, {n_eff} effect chunks");
        let all: Vec<u32> = entries.iter().filter(|e| e.type_hash == TYPE_HASH_EFFECT).map(|e| e.name_hash).collect();
        for (name, h2, h1) in &targets {
            println!("  target {name}: m2=0x{h2:08X} present={} / m1=0x{h1:08X} present={}",
                all.contains(h2), all.contains(h1));
        }
        // Try shortened / alternate name spellings against the 314 effect hashes.
        for v in ["env_godray2", "godray2", "godray", "env_godray", "particle_env_godray2",
                  "global_particle_env_godray", "envgodray2", "global_env_godray2",
                  "global_particle_env_godray2_infinite", "global_particle_godray2"] {
            let (m2, m1) = (pandemic_hash_m2(v), pandemic_hash(v));
            if all.contains(&m2) || all.contains(&m1) {
                println!("  >>> VARIANT HIT '{v}': m2=0x{m2:08X}({}) m1=0x{m1:08X}({})",
                    all.contains(&m2), all.contains(&m1));
            }
        }
        for dw in &effect_dwords {
            if all.contains(dw) {
                println!("  >>> EffectTemplate dword 0x{dw:08X} IS an effect-block entry");
            }
        }
        println!("  first 12 effect name_hashes: {:08X?}", &all[..all.len().min(12)]);
        let mut pos = 4 + count as usize * 16;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if e.type_hash == TYPE_HASH_EFFECT {
                for (name, h2, h1) in &targets {
                    if e.name_hash == *h2 || e.name_hash == *h1 {
                        println!("  >>> MATCH {name}: name_hash=0x{:08X} at blk {blk} ({} bytes)", e.name_hash, e.chunk_size);
                        if end <= dec.len() {
                            dump_typed(&dec[pos..end], name);
                        }
                    }
                }
            }
            pos = end;
        }
    }

    // Characterize the god-ray TEXT texture (0xB73157C0): aspect + luminance/alpha profile.
    probe_texture(&mut w, 0xB73157C0);

    // End-to-end: resolve each god-ray placement to its real, data-driven glow card.
    println!("\n[resolved glow cards for god-ray placements]");
    for p in &placements {
        let raw = p.name.as_deref().unwrap_or("");
        let name = raw.split(" 0x").next().unwrap_or(raw).trim_start_matches('_');
        if name.contains("godray") {
            let g = mercs2_engine::game_world::glow_card_for_effect(&mut w, name, p.pos)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            println!("  {name}: pos {:.1?} size {:.2} color {:.3?}", g.pos, g.size, g.color);
        }
    }

    // Also try the ASET-based resolution (kept for completeness / diagnostics).
    let names: Vec<String> = distinct.clone();
    for name in &names {
        dump_effect(&mut w, name);
    }
}

/// DXT1/DXT5 block color luminance (avg of the two RGB565 endpoints) + DXT5 alpha (block avg).
fn probe_texture(w: &mut wad::Wad, hash: u32) {
    use mercs2_formats::texture::TexFormat;
    println!("\n=== TEXT texture 0x{hash:08X} ===");
    println!("  ASET hits for 0x{hash:08X} (type_id,primary,block): {:?}", wad::aset_types(w, hash));
    let tex = match wad::extract_texture(w, hash) {
        Ok(t) => t,
        Err(e) => { println!("  extract failed: {e}"); return; }
    };
    let (bw, bh) = (tex.width / 4, tex.height / 4);
    let aspect = tex.width as f32 / tex.height.max(1) as f32;
    println!("  {}x{} {:?} mips={} aspect={aspect:.2} ({})",
        tex.width, tex.height, tex.format, tex.mip_count,
        if aspect > 1.6 { "WIDE band" } else if aspect < 0.62 { "TALL shaft" } else { "~square disc/cone" });
    let (block_bytes, color_off, has_alpha) = match tex.format {
        TexFormat::Bc1 => (8usize, 0usize, false),
        TexFormat::Bc3 => (16usize, 8usize, true),
    };
    let data = &tex.mip0;
    let l565 = |c: u16| -> f32 {
        let r = ((c >> 11) & 0x1F) as f32 / 31.0;
        let g = ((c >> 5) & 0x3F) as f32 / 63.0;
        let b = (c & 0x1F) as f32 / 31.0;
        0.299 * r + 0.587 * g + 0.114 * b
    };
    // Coarse GRID x GRID luminance + alpha map.
    const G: usize = 12;
    let mut lum = [[0f32; G]; G];
    let mut alp = [[0f32; G]; G];
    let mut cnt = [[0f32; G]; G];
    for by in 0..bh as usize {
        for bx in 0..bw as usize {
            let o = (by * bw as usize + bx) * block_bytes;
            if o + block_bytes > data.len() { continue; }
            let c0 = u16::from_le_bytes([data[o + color_off], data[o + color_off + 1]]);
            let c1 = u16::from_le_bytes([data[o + color_off + 2], data[o + color_off + 3]]);
            let l = 0.5 * (l565(c0) + l565(c1));
            let a = if has_alpha {
                let mut s = 0u32; for k in 0..8 { s += data[o + k] as u32; } s as f32 / (8.0 * 255.0)
            } else { 1.0 };
            let gx = bx * G / bw.max(1) as usize;
            let gy = by * G / bh.max(1) as usize;
            lum[gy][gx] += l; alp[gy][gx] += a; cnt[gy][gx] += 1.0;
        }
    }
    println!("  luminance grid (0-9, . = ~0), rows top->bottom:");
    for gy in 0..G {
        let row: String = (0..G).map(|gx| {
            let v = if cnt[gy][gx] > 0.0 { lum[gy][gx] / cnt[gy][gx] } else { 0.0 };
            if v < 0.05 { '.' } else { (b'0' + (v * 9.0).min(9.0) as u8) as char }
        }).collect();
        let arow: String = (0..G).map(|gx| {
            let v = if cnt[gy][gx] > 0.0 { alp[gy][gx] / cnt[gy][gx] } else { 0.0 };
            if v < 0.05 { '.' } else { (b'0' + (v * 9.0).min(9.0) as u8) as char }
        }).collect();
        println!("    L|{row}|  A|{arow}|");
    }
}

fn atrb_line(a: &Atrb) -> String {
    let name = attribute_name(a.hash).map(|n| format!(" ({n})")).unwrap_or_default();
    let value = match a.value {
        AtrbValue::F32(v) => format!("{v}"),
        AtrbValue::U32(v) => format!("0x{v:08X}"),
    };
    let curve = match &a.curve {
        None => String::new(),
        Some(keys) => format!(" curve {:?}", keys.iter().map(|k| (k.time, k.value)).collect::<Vec<_>>()),
    };
    format!("0x{:08X}{name} flags=0x{:03X} = {value}{curve}", a.hash, a.flags)
}

/// Print an effect container as the typed tree. A container that does not parse is reported, not
/// half-printed.
fn dump_typed(c: &[u8], name: &str) {
    println!("  --- {name}: {} bytes ---", c.len());
    let fx = match parse_effect_container(c) {
        Ok(fx) => fx,
        Err(e) => {
            println!("  PARSE FAILED: {e}");
            return;
        }
    };
    match fx.efct_words() {
        Ok(w) => println!("  EFCT {w:?}"),
        Err(e) => println!("  EFCT: {e}"),
    }
    for (i, s) in fx.shapes.iter().enumerate() {
        println!("  shape {i}: {} records", s.records.len());
    }
    for (i, e) in fx.emitters.iter().enumerate() {
        println!("  emitter {i}: geom {:?} PTYP flags 0x{:X}", e.geom, e.particle.flags);
        for (r, row) in e.transform.iter().enumerate() {
            println!("    TRFM[{r}]: {row:8.3?}");
        }
        for a in &e.channels {
            println!("    channel {}", atrb_line(a));
        }
        for a in &e.particle.attributes {
            println!("    attr {}", atrb_line(a));
        }
        let colr: Vec<String> = (0..8)
            .map(|k| {
                let key = e.particle.colr.keys[k * 99 / 7];
                format!("{:02X?}/{:04X}", key.rgba, key.half_bits)
            })
            .collect();
        println!("    COLR (8 of 100 keys, rgba/half): {colr:?}");
        println!("    TEXT frames: {:08X?}", e.particle.text.frames);
    }
    for (i, f) in fx.forces.iter().enumerate() {
        println!("  force {i}: {:?}", f.kind);
        for a in &f.attributes {
            println!("    attr {}", atrb_line(a));
        }
    }
}
