//! Dev bin: list every `global_particle_*` FX placement in the PMC interior state block (667),
//! then reverse the effect template for each distinct effect name (extract its UCFX container from
//! the effects block + dump the EFCT/EMTR/EMIT/COLR/FRCE/PTYP/POFF/TRFM/TEXT chunks). This pins what
//! the interior loader classifies + which effects are skipped as "unsupported" (godray / lightshaft).
//!   cargo run -p mercs2_probe --bin fx_probe

use mercs2_engine::wad;
use mercs2_engine::worldutil::PMC_INTERIOR_STATE_BLOCK;
use mercs2_formats::fxdict::EffectTemplate;
use mercs2_formats::hash::{pandemic_hash, pandemic_hash_m2};
use mercs2_formats::placement::load_placements;
use mercs2_formats::types::TYPE_HASH_EFFECT;

fn ru32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn rf32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Walk a UCFX container's descriptor rows -> (tag, body) pairs.
fn ucfx_chunks(c: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut out = Vec::new();
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return out;
    }
    let dao = ru32(c, 4) as usize;
    let n = ru32(c, 16) as usize;
    for i in 0..n {
        let row = 20 + i * 20;
        if row + 20 > c.len() {
            break;
        }
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&c[row..row + 4]);
        let u0 = ru32(c, row + 4);
        if u0 == 0xFFFF_FFFF {
            out.push((tag, Vec::new())); // container sentinel
            continue;
        }
        let size = ru32(c, row + 8) as usize;
        let start = if dao > 0 { dao + u0 as usize } else { 8 + u0 as usize };
        let end = start + size;
        if end <= c.len() {
            out.push((tag, c[start..end].to_vec()));
        }
    }
    out
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
    let chunks = ucfx_chunks(&c);
    println!("  {} chunks: {:?}", chunks.len(),
        chunks.iter().map(|(t, b)| format!("{}({}B)", String::from_utf8_lossy(t), b.len())).collect::<Vec<_>>());
    // Raw dumps of the interesting chunks.
    for (tag, body) in &chunks {
        match tag {
            b"EMIT" => {
                let f: Vec<f32> = (0..body.len() / 4).map(|i| rf32(body, i * 4)).collect();
                println!("    EMIT floats: {f:?}");
            }
            b"POFF" => println!("    POFF: [{:.3},{:.3},{:.3}]", rf32(body, 0), rf32(body, 4), rf32(body, 8)),
            b"TRFM" if body.len() >= 64 => {
                for r in 0..4 {
                    println!("    TRFM[{r}]: [{:8.3},{:8.3},{:8.3},{:8.3}]",
                        rf32(body, (r * 4) * 4), rf32(body, (r * 4 + 1) * 4),
                        rf32(body, (r * 4 + 2) * 4), rf32(body, (r * 4 + 3) * 4));
                }
            }
            b"PTYP" => println!("    PTYP flags: 0x{:02X}", body.first().copied().unwrap_or(0)),
            b"FRCE" => {
                let ih = if body.len() >= 4 { ru32(body, 0) } else { 0 };
                let ps: Vec<f32> = (0..(body.len().saturating_sub(4)) / 4).map(|i| rf32(body, 4 + i * 4)).collect();
                println!("    FRCE inner=0x{ih:08X} ('{}') params={ps:?}", String::from_utf8_lossy(&ih.to_le_bytes()));
            }
            b"TEXT" => {
                let refs: Vec<String> = (0..body.len() / 4).map(|i| format!("0x{:08X}", ru32(body, i * 4))).collect();
                println!("    TEXT words: {refs:?}");
            }
            b"EMTR" => {
                let refs: Vec<String> = if body.len() >= 2 {
                    let cnt = u16::from_le_bytes([body[0], body[1]]) as usize;
                    (0..cnt.min((body.len() - 2) / 4)).map(|i| format!("0x{:08X}", ru32(body, 2 + i * 4))).collect()
                } else { Vec::new() };
                println!("    EMTR refs: {refs:?}");
            }
            b"COLR" if body.len() >= 200 => {
                // Dump 8 evenly-spaced RGBA stops.
                let g = mercs2_formats::fxdict::parse_colr(body).unwrap();
                let s: Vec<String> = (0..8).map(|k| {
                    let c = g.sample(k as f32 / 7.0);
                    format!("[{:.2},{:.2},{:.2},{:.2}]", c[0], c[1], c[2], c[3])
                }).collect();
                println!("    COLR stops(8): {s:?}");
            }
            _ => {}
        }
    }
    // Also dump the structured EffectTemplate view.
    let refs: Vec<(&[u8; 4], &[u8])> = chunks.iter().map(|(t, b)| (t, b.as_slice())).collect();
    let tmpl = EffectTemplate::from_chunks(refs.into_iter());
    println!("  parsed: header={:?} emitters={} forces={} ptype={:?} offset={:?} has_colr={} has_trfm={}",
        tmpl.header, tmpl.emitters.refs.len(), tmpl.forces.len(), tmpl.ptype,
        tmpl.offset, tmpl.gradient.is_some(), tmpl.transform.is_some());
}

fn main() {
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
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
                            dump_container_chunks(&dec[pos..end], name);
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
            let g = mercs2_engine::game_world::glow_card_for_effect(&mut w, name, p.pos);
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

/// Reverse an ATRB parameter name-hash by brute-forcing a candidate word list (fxdict namespace).
fn name_atrb(nh: u32) -> String {
    let words = [
        "red","green","blue","alpha","color","colour","intensity","brightness","scale","size",
        "width","height","length","depth","radius","radiusinner","radiusouter","innerradius",
        "outerradius","angle","cone","fadein","fadeout","fade","opacity","glow","emissive",
        "rotation","rotate","spin","speed","rate","lifetime","life","count","number","offset",
        "offsetx","offsety","offsetz","posx","posy","posz","scalex","scaley","scalez","tint",
        "texture","tex","uv","uvscale","scroll","pulse","flicker","frequency","amplitude","phase",
        "distance","near","far","start","end","top","bottom","taper","falloff","softness","edge",
        "additive","blend","enabled","visible","gamma","exposure","hdr","bloom","raylength",
        "shaftlength","shaftwidth","godray","lightshaft","light","sun","ambient","diffuse","specular",
        "attenuation","range","power","strength","factor","multiplier","min","max","base","value",
    ];
    for w in words {
        if pandemic_hash_m2(w) == nh || pandemic_hash(w) == nh {
            return format!(" ('{w}')");
        }
    }
    String::new()
}

fn dump_container_chunks(c: &[u8], name: &str) {
    println!("  --- chunks for {name} ---");
    let chunks = ucfx_chunks(c);
    println!("  {} chunks: {:?}", chunks.len(),
        chunks.iter().map(|(t, b)| format!("{}({}B)", String::from_utf8_lossy(t), b.len())).collect::<Vec<_>>());
    for (tag, body) in &chunks {
        match tag {
            b"EMIT" => { let f: Vec<f32> = (0..body.len()/4).map(|i| rf32(body, i*4)).collect(); println!("    EMIT: {f:?}"); }
            b"POFF" if body.len() >= 12 => println!("    POFF: [{:.3},{:.3},{:.3}]", rf32(body,0), rf32(body,4), rf32(body,8)),
            b"TRFM" if body.len() >= 64 => for r in 0..4 {
                println!("    TRFM[{r}]: [{:8.3},{:8.3},{:8.3},{:8.3}]",
                    rf32(body,(r*4)*4), rf32(body,(r*4+1)*4), rf32(body,(r*4+2)*4), rf32(body,(r*4+3)*4)); },
            b"PTYP" => println!("    PTYP: 0x{:02X}", body.first().copied().unwrap_or(0)),
            b"FRCE" => { let ih = if body.len()>=4 {ru32(body,0)} else {0};
                let ps: Vec<f32> = (0..(body.len().saturating_sub(4))/4).map(|i| rf32(body,4+i*4)).collect();
                println!("    FRCE: inner=0x{ih:08X} ('{}') params={ps:?}", String::from_utf8_lossy(&ih.to_le_bytes())); }
            b"TEXT" => { let refs: Vec<String> = (0..body.len()/4).map(|i| format!("0x{:08X}", ru32(body,i*4))).collect();
                println!("    TEXT: {refs:?}"); }
            b"EMTR" => { let refs: Vec<String> = if body.len()>=2 {
                    let cnt = u16::from_le_bytes([body[0],body[1]]) as usize;
                    (0..cnt.min((body.len()-2)/4)).map(|i| format!("0x{:08X}", ru32(body,2+i*4))).collect() } else { Vec::new() };
                println!("    EMTR: {refs:?}"); }
            b"COLR" => {
                let nz: Vec<usize> = (0..body.len()).filter(|&i| body[i] != 0).collect();
                println!("    COLR {}B: {} nonzero bytes, first nz @ {:?}, last nz @ {:?}",
                    body.len(), nz.len(), nz.first(), nz.last());
                // Dump 32-byte windows around the nonzero region.
                if let (Some(&f), Some(&l)) = (nz.first(), nz.last()) {
                    let a = f.saturating_sub(4);
                    let b = (l + 4).min(body.len());
                    for row in (a..b).step_by(16) {
                        let end = (row + 16).min(body.len());
                        println!("      @{row:4}: {:02X?}", &body[row..end]);
                    }
                }
            }
            b"ATRB" if body.len() >= 12 => {
                let nh = ru32(body, 0);
                let ty = ru32(body, 4);
                let vf = rf32(body, 8);
                let vi = ru32(body, 8);
                println!("    ATRB name=0x{nh:08X}{} ty={ty} val_f={vf:.4} val_hex=0x{vi:08X}", name_atrb(nh));
            }
            b"GEOM" if body.len() > 64 => {
                let vcount = ru32(body, 0) as usize;
                let stride = (body.len() - 4) / vcount.max(1);
                let nf = stride / 4;
                println!("    GEOM {}B: vcount={vcount} stride={stride}B ({nf} f32/vert)", body.len());
                // Try pos = first 3 f32 per stride; report bbox.
                let mut mn = [f32::MAX; 3];
                let mut mx = [f32::MIN; 3];
                for v in 0..vcount {
                    let o = 4 + v * stride;
                    if o + 12 > body.len() { break; }
                    for c in 0..3 {
                        let val = rf32(body, o + c * 4);
                        mn[c] = mn[c].min(val); mx[c] = mx[c].max(val);
                    }
                }
                println!("      pos bbox min={mn:.3?} max={mx:.3?} size={:.3?}",
                    [mx[0]-mn[0], mx[1]-mn[1], mx[2]-mn[2]]);
                // First 2 verts, all floats.
                for v in 0..vcount.min(2) {
                    let o = 4 + v * stride;
                    let f: Vec<f32> = (0..nf).map(|i| rf32(body, o + i*4)).collect();
                    println!("      v{v}: {f:.3?}");
                }
            }
            _ => { println!("    {} ({}B) head={:02X?}", String::from_utf8_lossy(tag), body.len(), &body[..body.len().min(24)]); }
        }
    }
}
