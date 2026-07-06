//! action_table_probe — dump the base-game ActionTable (type animationtable
//! 0x207359C7, asset 0x6802C321) and cross-reference its AnimationHandles column
//! against character animgroups, to pin how the shared table resolves to a
//! specific merc's clips (the dynamic per-character animation mechanism).
//!
//! Container layout (verified in ucfx_byteswap::convert + action_table.rs):
//!   block = [u32 count][count×16B: name_hash,type_hash,field_c,size][containers]
//!   ActionTable container = UCFX:
//!     INFO body = [u16 keyDims][u16 totalDims][u16 count]
//!     TYPE body = totalDims × ([ASCII name]\0 [u16 field])   (the column names)
//!     VALU body = count rows × totalDims u32                 (the value matrix)

use std::collections::BTreeSet;

use mercs2_engine::wad;
use mercs2_formats::animgroup::parse_animgroup;

const ACTIONTABLE: u32 = 0x6802_C321;
const TYPE_ANIMTABLE: u32 = 0x2073_59C7;
const NONE_SENTINEL: u32 = 0x27DE_7135;

fn r_u16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn r_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// Find the ActionTable container bytes in a decompressed multi-entry block.
fn find_actiontable(dec: &[u8]) -> Option<(u32, Vec<u8>)> {
    if dec.len() < 4 {
        return None;
    }
    let count = r_u32(dec, 0) as usize;
    let max = dec.len().saturating_sub(4) / 16;
    let count = count.min(max);
    let mut pos = 4 + count * 16;
    for i in 0..count {
        let b = 4 + i * 16;
        let nh = r_u32(dec, b);
        let th = r_u32(dec, b + 4);
        let sz = r_u32(dec, b + 12) as usize;
        if pos + sz > dec.len() {
            break;
        }
        if nh == ACTIONTABLE || th == TYPE_ANIMTABLE {
            println!("  entry[{i}] name=0x{nh:08X} type=0x{th:08X} size={sz}");
        }
        if nh == ACTIONTABLE {
            return Some((th, dec[pos..pos + sz].to_vec()));
        }
        pos += sz;
    }
    None
}

/// Parse a UCFX container's chunks into (tag, body) slices.
fn chunks(cont: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    if cont.len() < 20 || &cont[0..4] != b"UCFX" {
        return out;
    }
    let data_area = r_u32(cont, 4) as usize;
    let ndesc = r_u32(cont, 16) as usize;
    if 20 + ndesc * 20 > cont.len() {
        return out;
    }
    for i in 0..ndesc {
        let off = 20 + i * 20;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&cont[off..off + 4]);
        let body_off = r_u32(cont, off + 4) as usize;
        let body_sz = r_u32(cont, off + 8) as usize;
        let start = data_area + body_off;
        if start + body_sz <= cont.len() {
            out.push((tag, start, body_sz));
        }
    }
    out
}

/// Parse the TYPE chunk: totalDims × ([ASCII name]\0 [u16]). Returns column names.
fn column_names(body: &[u8], total_dims: usize) -> Vec<String> {
    let mut names = Vec::new();
    let mut p = 0;
    for _ in 0..total_dims {
        let start = p;
        while p < body.len() && body[p] != 0 {
            p += 1;
        }
        let name = String::from_utf8_lossy(&body[start..p]).to_string();
        p += 1; // skip NUL
        p += 2; // skip trailing u16
        names.push(name);
        if p > body.len() {
            break;
        }
    }
    names
}

/// Enumerate every animationtable (type 0x207359C7) container in a block.
fn animation_tables(dec: &[u8]) -> Vec<(u32, Vec<u8>)> {
    let mut out = Vec::new();
    if dec.len() < 4 {
        return out;
    }
    let count = r_u32(dec, 0) as usize;
    let max = dec.len().saturating_sub(4) / 16;
    let count = count.min(max);
    let mut pos = 4 + count * 16;
    for i in 0..count {
        let b = 4 + i * 16;
        let nh = r_u32(dec, b);
        let th = r_u32(dec, b + 4);
        let sz = r_u32(dec, b + 12) as usize;
        if pos + sz > dec.len() {
            break;
        }
        if th == TYPE_ANIMTABLE {
            out.push((nh, dec[pos..pos + sz].to_vec()));
        }
        pos += sz;
    }
    out
}

/// Census the entry TYPES in any multi-entry block.
fn entry_type_census(dec: &[u8]) -> Vec<(u32, u32, usize)> {
    let mut out = Vec::new();
    if dec.len() < 4 {
        return out;
    }
    let count = r_u32(dec, 0) as usize;
    let max = dec.len().saturating_sub(4) / 16;
    let count = count.min(max);
    let mut pos = 4 + count * 16;
    for i in 0..count {
        let b = 4 + i * 16;
        let nh = r_u32(dec, b);
        let th = r_u32(dec, b + 4);
        let sz = r_u32(dec, b + 12) as usize;
        if pos + sz > dec.len() {
            break;
        }
        out.push((nh, th, sz));
        pos += sz;
    }
    out
}

fn parse_table(cont: &[u8]) -> Option<(usize, usize, usize, Vec<String>, Option<(usize, usize)>)> {
    let chs = chunks(cont);
    let (mut kd, mut td, mut ct) = (0usize, 0usize, 0usize);
    for (tag, start, sz) in &chs {
        if tag == b"INFO" && *sz >= 6 {
            kd = r_u16(cont, *start) as usize;
            td = r_u16(cont, *start + 2) as usize;
            ct = r_u16(cont, *start + 4) as usize;
        }
    }
    let mut names = Vec::new();
    let mut valu = None;
    for (tag, start, sz) in &chs {
        if tag == b"TYPE" {
            names = column_names(&cont[*start..*start + *sz], td);
        }
        if tag == b"VALU" {
            valu = Some((*start, *sz));
        }
    }
    Some((kd, td, ct, names, valu))
}

fn main() {
    let mut w = wad::registry_vz_wad()
        .and_then(|p| wad::open(&p).ok())
        .expect("open vz.wad");

    // (1) Survey EVERY resident animationtable's columns — which one maps handles->clips?
    let dec3185 = wad::decompress_block_index(&mut w, 3185).expect("decompress 3185");
    println!("== resident animationtables (block 3185) ==");
    for (nh, cont) in animation_tables(&dec3185) {
        if let Some((kd, td, ct, names, _)) = parse_table(&cont) {
            println!("  0x{nh:08X}  {}B  keyDims={kd} totalDims={td} rows={ct}", cont.len());
            println!("      cols: {names:?}");
        } else {
            println!("  0x{nh:08X}  {}B  (no INFO/TYPE — not a dim table)", cont.len());
        }
    }

    // (2) Chris animgroup (3278): what entry TYPES does the block carry besides clips?
    if let Ok(dec) = wad::decompress_block_index(&mut w, 3278) {
        let cen = entry_type_census(&dec);
        let mut by_type: std::collections::BTreeMap<u32, (usize, usize)> = Default::default();
        for (_nh, th, sz) in &cen {
            let e = by_type.entry(*th).or_insert((0, 0));
            e.0 += 1;
            e.1 += *sz;
        }
        println!("== Chris animgroup block 3278: {} entries, types ==", cen.len());
        for (th, (n, bytes)) in &by_type {
            println!("  type 0x{th:08X}: {n} entries, {bytes} bytes");
        }
    }

    // (2b) AnimationLookup 0xE00B080C: validate it resolves (Handle, CharacterName) -> clip.
    let clip_set = |w: &mut _, blk: u16| -> BTreeSet<u32> {
        wad::decompress_block_index(w, blk)
            .ok()
            .and_then(|d| parse_animgroup(&d).ok())
            .map(|g| g.clips.iter().map(|c| c.name_hash).collect())
            .unwrap_or_default()
    };
    let chris_clips = clip_set(&mut w, 3278);
    let mattias_clips = clip_set(&mut w, 3154);
    let jen_clips = clip_set(&mut w, 3362);

    if let Some((_, lk)) = animation_tables(&dec3185).into_iter().find(|(nh, _)| *nh == 0xE00B080C) {
        if let Some((_kd, td, ct, names, Some((vstart, vsz)))) = parse_table(&lk) {
            let col = |n: &str| names.iter().position(|x| x.eq_ignore_ascii_case(n));
            let (ci_h, ci_cn, ci_an) = (col("Handle"), col("CharacterName"), col("Animation"));
            let rows = (vsz / 4) / td;
            let get = |row: usize, ci: usize| -> u32 { r_u32(&lk, vstart + (row * td + ci) * 4) };
            println!("== AnimationLookup 0xE00B080C ({ct} rows): Handle@{ci_h:?} CharacterName@{ci_cn:?} Animation@{ci_an:?} ==");
            // Does the lookup's Handle column match the ActionTable's AnimationHandles?
            let action_handles: BTreeSet<u32> = find_actiontable(&dec3185)
                .and_then(|(_, atc)| parse_table(&atc).map(|(_, atd, _, atn, av)| (atc, atd, atn, av)))
                .map(|(atc, atd, atn, av)| {
                    let ai = atn.iter().position(|x| x.eq_ignore_ascii_case("AnimationHandles")).unwrap();
                    let (vs, vz) = av.unwrap();
                    let rows = (vz / 4) / atd;
                    (0..rows).map(|r| r_u32(&atc, vs + (r * atd + ai) * 4)).filter(|h| *h != 0 && *h != NONE_SENTINEL).collect()
                })
                .unwrap_or_default();
            if let (Some(hh0), Some(cn0)) = (ci_h, ci_cn) {
                let mut lk_handles: BTreeSet<u32> = BTreeSet::new();
                let mut cnames: std::collections::BTreeMap<u32, usize> = Default::default();
                for row in 0..rows {
                    lk_handles.insert(get(row, hh0));
                    *cnames.entry(get(row, cn0)).or_default() += 1;
                }
                let overlap = action_handles.iter().filter(|h| lk_handles.contains(h)).count();
                println!("  ActionTable.AnimationHandles={} distinct; Lookup.Handle={} distinct; overlap={overlap}", action_handles.len(), lk_handles.len());
                println!("  distinct CharacterName ({}):", cnames.len());
                for (v, n) in cnames.iter() {
                    println!("     0x{v:08X} x{n}");
                }
                println!("  sample rows (Handle | Gender | CharacterName | Animation):");
                for row in 0..rows.min(8) {
                    let g = col("Gender").map(|c| get(row, c)).unwrap_or(0);
                    println!("     0x{:08X} | 0x{g:08X} | 0x{:08X} | 0x{:08X}", get(row, hh0), get(row, cn0), ci_an.map(|c| get(row, c)).unwrap_or(0));
                }
            }
            let _ = (&chris_clips, &mattias_clips, &jen_clips);
            if let (Some(ah), Some(cn), Some(hh)) = (ci_an, ci_cn, ci_h) {
                let f32_at = |row: usize, ci: usize| f32::from_bits(get(row, ci));
                // Animation column: small index/enum vs hash-like?
                let (mut small, mut large, mut amax) = (0u32, 0u32, 0u32);
                for row in 0..rows {
                    let a = get(row, ah);
                    if a < 0x1_0000 { small += 1; amax = amax.max(a); } else { large += 1; }
                }
                println!("  Animation column: {small} small(<0x10000, max={amax}), {large} hash-like");
                // Per-merc Animation-index stats: distinct count vs animgroup clip count.
                for (merc, cval, clips) in [("mattias", 0x030E6C38u32, &mattias_clips), ("chris", 0xD64BB122, &chris_clips), ("jennifer", 0xF3144C8E, &jen_clips)] {
                    let mut idxs: BTreeSet<u32> = BTreeSet::new();
                    for row in 0..rows { if get(row, cn) == cval { idxs.insert(get(row, ah)); } }
                    let (mn, mx) = (idxs.iter().next().copied().unwrap_or(0), idxs.iter().next_back().copied().unwrap_or(0));
                    let span = (mx.saturating_sub(mn)) as usize;
                    println!("  [{merc}] rows->{} distinct indices, range 0x{mn:04X}..0x{mx:04X} (span {span}), animgroup clips={}", idxs.len(), clips.len());
                }

                // Hop §4: the Animation index -> clip resolution. Look for a value pool chunk
                // (a chunk that is not INFO/TYPE/VALU) and test several index interpretations.
                let all = chunks(&lk);
                println!("  lk chunks: {:?}", all.iter().map(|(t, _, s)| format!("{}({s}B)", String::from_utf8_lossy(t))).collect::<Vec<_>>());
                let pools: Vec<_> = all.iter().filter(|(t, _, _)| t != b"INFO" && t != b"TYPE" && t != b"VALU").cloned().collect();
                for (ptag, pstart, psz) in &pools {
                    println!("  --- pool {} @byte{pstart} size={psz} ({} u32 / {} u16) ---", String::from_utf8_lossy(ptag), psz / 4, psz / 2);
                    for (merc, cval, clips) in [("mattias", 0x030E6C38u32, &mattias_clips), ("chris", 0xD64BB122, &chris_clips), ("jennifer", 0xF3144C8E, &jen_clips)] {
                        let (mut h_u32i, mut h_u32o, mut h_half, mut total) = (0, 0, 0, 0);
                        let mut idle = (false, false, false);
                        for row in 0..rows {
                            if get(row, cn) != cval { continue; }
                            let idx = get(row, ah) as usize;
                            total += 1;
                            // (a) idx = u32 element index into pool
                            if pstart + idx * 4 + 4 <= pstart + psz {
                                let v = r_u32(&lk, pstart + idx * 4);
                                if clips.contains(&v) { h_u32i += 1; if v == 0xED37BC56 { idle.0 = true; } }
                            }
                            // (b) idx = byte offset into pool, read u32
                            if pstart + idx + 4 <= pstart + psz {
                                let v = r_u32(&lk, pstart + idx);
                                if clips.contains(&v) { h_u32o += 1; if v == 0xED37BC56 { idle.1 = true; } }
                            }
                            // (c) idx/2 = u32 element index
                            if pstart + (idx / 2) * 4 + 4 <= pstart + psz {
                                let v = r_u32(&lk, pstart + (idx / 2) * 4);
                                if clips.contains(&v) { h_half += 1; if v == 0xED37BC56 { idle.2 = true; } }
                            }
                        }
                        println!("     [{merc}] /{total}: u32-idx={h_u32i} byte-off={h_u32o} half-idx={h_half}  chrisIdle(a/b/c)={:?}", idle);
                    }
                }

                // FULL-CHAIN validation: Chris idle clip -> ASTO -> Handle -> ActionTable state row.
                if let Some((_, ps, psz)) = pools.iter().find(|(t, _, _)| t == b"ASTO").cloned() {
                    let mut h_idle = None;
                    for row in 0..rows {
                        if get(row, cn) != 0xD64BB122 { continue; }
                        let idx = get(row, ah) as usize;
                        if ps + idx * 4 + 4 <= ps + psz && r_u32(&lk, ps + idx * 4) == 0xED37BC56 {
                            h_idle = Some(get(row, hh));
                            break;
                        }
                    }
                    if let Some(hid) = h_idle {
                        println!("  ✓ FULL CHAIN: Chris idle 0xED37BC56 <- ASTO[idx] <- Handle 0x{hid:08X} (Handle in ActionTable={})", action_handles.contains(&hid));
                        if let Some((_, atc)) = find_actiontable(&dec3185) {
                            if let Some((_, atd, _, atn, Some((avs, avz)))) = parse_table(&atc) {
                                let ai = atn.iter().position(|x| x.eq_ignore_ascii_case("AnimationHandles")).unwrap();
                                let arows = (avz / 4) / atd;
                                for r in 0..arows {
                                    if r_u32(&atc, avs + (r * atd + ai) * 4) == hid {
                                        let vals: Vec<String> = (0..atd.min(8)).map(|c| format!("{}=0x{:08X}", atn[c], r_u32(&atc, avs + (r * atd + c) * 4))).collect();
                                        println!("     ActionTable row -> {}", vals.join("  "));
                                    }
                                }
                            }
                        }
                    }
                }
                // Dump rows for the 3 most common NAMED characters.
                let mut cn_counts: Vec<(u32, usize)> = {
                    let mut m: std::collections::BTreeMap<u32, usize> = Default::default();
                    for row in 0..rows { *m.entry(get(row, cn)).or_default() += 1; }
                    m.into_iter().filter(|(k, _)| *k != NONE_SENTINEL).collect()
                };
                cn_counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
                for (cval, _) in cn_counts.iter().take(3) {
                    println!("  --- CharacterName 0x{cval:08X} rows (Handle | Animation | Min | Max | PrimEquipName) ---");
                    let mut shown = 0;
                    for row in 0..rows {
                        if get(row, cn) != *cval { continue; }
                        let pe = col("PrimaryEquipmentName").map(|c| get(row, c)).unwrap_or(0);
                        println!("     0x{:08X} | 0x{:08X} | {:.2} | {:.2} | 0x{pe:08X}", get(row, hh), get(row, ah), f32_at(row, 8), f32_at(row, 9));
                        shown += 1;
                        if shown >= 6 { break; }
                    }
                }
            }
        }
    }

    // (3) Locate the ActionTable for the detailed AnimationHandles cross-ref below.
    println!("\n== ActionTable 0x{ACTIONTABLE:08X} detail ==");
    let Some((_, cont)) = find_actiontable(&dec3185) else {
        println!("ActionTable not found");
        return;
    };

    // Parse INFO / TYPE / VALU.
    let chs = chunks(&cont);
    println!("  chunks: {:?}", chs.iter().map(|(t, _, s)| format!("{}({s}B)", String::from_utf8_lossy(t))).collect::<Vec<_>>());
    let mut key_dims = 0usize;
    let mut total_dims = 0usize;
    let mut count = 0usize;
    for (tag, start, sz) in &chs {
        if tag == b"INFO" && *sz >= 6 {
            key_dims = r_u16(&cont, *start) as usize;
            total_dims = r_u16(&cont, *start + 2) as usize;
            count = r_u16(&cont, *start + 4) as usize;
        }
    }
    println!("  INFO: keyDims={key_dims} totalDims={total_dims} count={count} rows");

    let mut names: Vec<String> = Vec::new();
    for (tag, start, sz) in &chs {
        if tag == b"TYPE" {
            names = column_names(&cont[*start..*start + *sz], total_dims);
        }
    }
    println!("  columns: {names:?}");

    let anim_col = names.iter().position(|n| n.eq_ignore_ascii_case("AnimationHandles"));
    println!("  AnimationHandles column index = {anim_col:?}");

    // VALU value matrix: count × total_dims u32.
    let mut valu: Option<(usize, usize)> = None;
    for (tag, start, sz) in &chs {
        if tag == b"VALU" {
            valu = Some((*start, *sz));
        }
    }
    let Some((vstart, vsz)) = valu else {
        println!("  no VALU chunk");
        return;
    };
    let u32s = vsz / 4;
    let derived_rows = if total_dims > 0 { u32s / total_dims } else { 0 };
    println!("  VALU: {vsz} bytes = {u32s} u32 -> {derived_rows} rows (INFO count={count})");

    // Dump the AnimationHandles column values (distinct), if we located it.
    if let Some(ac) = anim_col {
        let rows = derived_rows.max(count);
        let mut handles: BTreeSet<u32> = BTreeSet::new();
        for row in 0..rows {
            let o = vstart + (row * total_dims + ac) * 4;
            if o + 4 > vstart + vsz {
                break;
            }
            let h = r_u32(&cont, o);
            if h != 0 && h != NONE_SENTINEL {
                handles.insert(h);
            }
        }
        println!("  AnimationHandles: {} distinct non-sentinel values", handles.len());
        for (i, h) in handles.iter().take(24).enumerate() {
            println!("     [{i}] 0x{h:08X}");
        }

        // Cross-reference against Chris (3278) & Mattias (3154) animgroups.
        for (merc, blk, anchor) in [("chris", 3278u16, 0xED37BC56u32), ("mattias", 3154u16, 0x24F8C8E6u32)] {
            if let Ok(dec) = wad::decompress_block_index(&mut w, blk) {
                if let Ok(ag) = parse_animgroup(&dec) {
                    let clip_hashes: BTreeSet<u32> = ag.clips.iter().map(|c| c.name_hash).collect();
                    let direct_hits = handles.iter().filter(|h| clip_hashes.contains(h)).count();
                    let anchor_in_handles = handles.contains(&anchor);
                    let anchor_in_clips = clip_hashes.contains(&anchor);
                    println!(
                        "  [{merc} blk{blk}] {} clips | AnimationHandles that are {merc} clip hashes: {direct_hits} | anchor 0x{anchor:08X} in-handles={anchor_in_handles} in-clips={anchor_in_clips}",
                        clip_hashes.len()
                    );
                }
            }
        }
    }

    // Distinct values of the key/context columns — to crack the state vocabulary (idle/walk/run).
    let rows = derived_rows.max(count);
    for want in ["Stance", "Action", "AimState", "ActionDirection"] {
        if let Some(ci) = names.iter().position(|n| n.eq_ignore_ascii_case(want)) {
            let mut vals: std::collections::BTreeMap<u32, usize> = Default::default();
            for row in 0..rows {
                let o = vstart + (row * total_dims + ci) * 4;
                if o + 4 <= vstart + vsz {
                    *vals.entry(r_u32(&cont, o)).or_default() += 1;
                }
            }
            let list: Vec<String> = vals.iter().map(|(v, n)| format!("0x{v:08X}:{n}")).collect();
            println!("  DISTINCT {want} ({}): {}", vals.len(), list.join(" "));
        }
    }

    // === RESOLVER VALIDATION: (Upright, Idle/Walk/Run) -> per-merc clip ===
    const UPRIGHT: u32 = 0x12C07B18;
    let atcol = |n: &str| names.iter().position(|x| x.eq_ignore_ascii_case(n)).unwrap();
    let (c_stance, c_action, c_ah) = (atcol("Stance"), atcol("Action"), atcol("AnimationHandles"));
    // the remaining key columns must be the none-sentinel for a "plain" state row
    let other_keys: Vec<usize> = ["AimState", "Tandem", "Seat", "Target", "ActionDirection", "DamageDirection"].iter().map(|n| atcol(n)).collect();
    let atget = |row: usize, ci: usize| r_u32(&cont, vstart + (row * total_dims + ci) * 4);
    let atrows = (vsz / 4) / total_dims;

    let (_, lk) = animation_tables(&dec3185).into_iter().find(|(nh, _)| *nh == 0xE00B080C).unwrap();
    let (_, ltd, _, lnames, lvalu) = parse_table(&lk).unwrap();
    let (lvs, lvz) = lvalu.unwrap();
    let lcol = |n: &str| lnames.iter().position(|x| x.eq_ignore_ascii_case(n)).unwrap();
    let (l_h, l_cn, l_an) = (lcol("Handle"), lcol("CharacterName"), lcol("Animation"));
    let lget = |row: usize, ci: usize| r_u32(&lk, lvs + (row * ltd + ci) * 4);
    let lrows = (lvz / 4) / ltd;
    let (_, asto_s, asto_z) = chunks(&lk).into_iter().find(|(t, _, _)| t == b"ASTO").unwrap();

    let _ = &other_keys;
    // Build Handle -> set of Actions for Upright-stance rows (keyDims=6, so Action is a key col).
    let mut handle_actions: std::collections::BTreeMap<u32, BTreeSet<u32>> = Default::default();
    for r in 0..atrows {
        if atget(r, c_stance) != UPRIGHT { continue; }
        handle_actions.entry(atget(r, c_ah)).or_default().insert(atget(r, c_action));
    }
    let action_name = |a: u32| -> String {
        match a {
            0xB4DA003B => "idle".into(), 0x5607E14E => "walk".into(), 0x41332AA0 => "run".into(),
            0x84FFBF2B => "jog".into(), 0x0C0A7FA6 => "fidget".into(), 0x9E33AD21 => "die".into(),
            0x5B50C577 => "dead".into(), 0x4565E347 => "dive".into(), 0x6FBE1165 => "jump".into(),
            0x3FB9797E => "fall".into(), 0xA43386B8 => "getup".into(), 0x8602E37D => "pickup".into(),
            other => format!("0x{other:08X}"),
        }
    };
    // Where do Walk/Run/Idle live? Dump every matching row's full key + handle (any stance).
    let stance_name = |s: u32| -> &'static str { match s { 0x12C07B18 => "Upright", 0x5E2CD838 => "InVehicle", 0x614DB965 => "Swim", 0xFC8D859D => "Prone", 0x27DE7135 => "*", _ => "?" } };
    for (nm, action) in [("idle", 0xB4DA003Bu32), ("walk", 0x5607E14E), ("run", 0x41332AA0), ("jog", 0x84FFBF2B)] {
        for r in (0..atrows).filter(|&r| atget(r, c_action) == action) {
            let h = atget(r, c_ah);
            // resolve this handle for each merc + the none default
            let resolve = |cval: u32| (0..lrows).find(|&lr| lget(lr, l_h) == h && lget(lr, l_cn) == cval)
                .and_then(|lr| { let o = asto_s + lget(lr, l_an) as usize * 4; (o + 4 <= asto_s + asto_z).then(|| r_u32(&lk, o)) });
            let dirs: Vec<usize> = ["AimState", "ActionDirection"].iter().map(|n| atcol(n)).collect();
            let ctx: Vec<String> = dirs.iter().map(|&c| format!("0x{:04X}", atget(r, c) & 0xffff)).collect();
            println!("  '{nm}' row: Stance={} Handle=0x{h:08X} [aim,dir]={ctx:?} | mattias={:?} chris={:?} jen={:?} none={:?}",
                stance_name(atget(r, c_stance)),
                resolve(0x030E6C38).map(|c| format!("0x{c:08X}")), resolve(0xD64BB122).map(|c| format!("0x{c:08X}")),
                resolve(0xF3144C8E).map(|c| format!("0x{c:08X}")), resolve(NONE_SENTINEL).map(|c| format!("0x{c:08X}")));
        }
    }
    // Resolve the validated primary-idle handle 0x700D4DE0 for each merc (idle-cluster head).
    println!("\n== primary-idle handle 0x700D4DE0 per merc ==");
    for (merc, cval) in [("mattias", 0x030E6C38u32), ("chris", 0xD64BB122), ("jennifer", 0xF3144C8E)] {
        let clip = (0..lrows).find(|&lr| lget(lr, l_h) == 0x700D4DE0 && lget(lr, l_cn) == cval)
            .and_then(|lr| { let o = asto_s + lget(lr, l_an) as usize * 4; (o + 4 <= asto_s + asto_z).then(|| r_u32(&lk, o)) });
        println!("  [{merc:8}] 0x700D4DE0 -> {:?}", clip.map(|c| format!("0x{c:08X}")));
    }

    // Validate the SHIPPED resolver the engine now uses (mercs2_formats::anim_select).
    println!("\n== mercs2_formats::anim_select::AnimSelector (engine code path) ==");
    match mercs2_formats::anim_select::AnimSelector::from_resident_block(&dec3185) {
        Some(sel) => for merc in ["mattias", "chris", "jennifer"] {
            let cn = mercs2_formats::anim_select::AnimSelector::character_name(merc);
            println!("  {merc:8} (0x{cn:08X}) idle -> {:?}", sel.primary_idle(cn).map(|c| format!("0x{c:08X}")));
        },
        None => println!("  from_resident_block(3185) = None"),
    }

    println!("\n== per-merc Upright action distribution (Handle->Action, clip counts) ==");
    for (merc, cval, blk) in [("mattias", 0x030E6C38u32, 3154u16), ("chris", 0xD64BB122, 3278), ("jennifer", 0xF3144C8E, 3362)] {
        let clips: BTreeSet<u32> = wad::decompress_block_index(&mut w, blk).ok().and_then(|d| parse_animgroup(&d).ok()).map(|g| g.clips.iter().map(|c| c.name_hash).collect()).unwrap_or_default();
        let _ = &clips;
        let mut by_action: std::collections::BTreeMap<u32, BTreeSet<u32>> = Default::default();
        let mut unmatched = 0;
        for lr in 0..lrows {
            if lget(lr, l_cn) != cval { continue; }
            let h = lget(lr, l_h);
            let o = asto_s + lget(lr, l_an) as usize * 4;
            if o + 4 > asto_s + asto_z { continue; }
            let clip = r_u32(&lk, o);
            match handle_actions.get(&h) {
                Some(acts) => for a in acts { by_action.entry(*a).or_default().insert(clip); },
                None => unmatched += 1,
            }
        }
        let mut sorted: Vec<_> = by_action.iter().collect();
        sorted.sort_by_key(|(_, cs)| std::cmp::Reverse(cs.len()));
        let summ: Vec<String> = sorted.iter().take(12).map(|(a, cs)| format!("{}={}", action_name(**a), cs.len())).collect();
        println!("  [{merc:8}] {} actions, {unmatched} rows w/ non-Upright handle | {}", by_action.len(), summ.join(" "));
    }
}
