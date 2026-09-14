//! guidmap_decode — READ-ONLY structural map of the guidmap CHDR body (block 3185, 0x385EA82C).
//! Walks the body, segmenting it into runs by u32 class (handle/zero/small/other) to reveal the
//! array boundaries (key array, parallel value array(s), bucket table), so an append can be made
//! consistent. Usage: guidmap_decode <vz.wad>

use std::fs::File;
use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::sges::decompress_block;

const WORLDENTITY_TYPE: u32 = 0x5647_C35D;
const GUIDMAP_TYPE: u32 = 0x140E_8728;

fn class(w: u32) -> &'static str {
    if w == 0 { "zero" }
    else if (w & 0xFFFF_0000) == 0x8000_0000 { "h8" }
    else if (w & 0xFFFF_0000) == 0x9000_0000 { "h9" }
    else if w < 0x0001_0000 { "small" }
    else { "other" }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: guidmap_decode <vz.wad>");
    let mut f = File::open(&path).unwrap();
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).expect("ffcs");
    let mut gm: Option<Vec<u8>> = None;
    for bi in 0..arch.indx.len() {
        let Ok(dec) = decompress_block(&mut f, &arch.indx, bi as u16) else { continue };
        if dec.len() < 4 { continue; }
        let cnt = read_u32_le(&dec, 0) as usize;
        if cnt == 0 || cnt > 100_000 { continue; }
        let mut has_we = false; let mut pos = 4 + cnt * 16;
        let mut found: Option<Vec<u8>> = None;
        for ei in 0..cnt {
            let base = 4 + ei * 16;
            if base + 16 > dec.len() { break; }
            let th = read_u32_le(&dec, base + 4);
            let sz = read_u32_le(&dec, base + 12) as usize;
            if th == WORLDENTITY_TYPE { has_we = true; }
            if th == GUIDMAP_TYPE { found = Some(dec[pos..pos + sz].to_vec()); }
            pos += sz;
        }
        if has_we { gm = found; break; }
    }
    let gm = gm.expect("guidmap");
    let data_off = read_u32_le(&gm, 4) as usize;
    let ndesc = read_u32_le(&gm, 16) as usize;
    let mut body: &[u8] = &[];
    for i in 0..ndesc {
        let ro = 20 + i * 20;
        if &gm[ro..ro + 4] == b"CHDR" {
            let u0 = read_u32_le(&gm, ro + 4) as usize;
            let sz = read_u32_le(&gm, ro + 8) as usize;
            body = &gm[data_off + u0..data_off + u0 + sz];
        }
    }
    println!("CHDR body {} bytes ({} u32 + {} rem)", body.len(), body.len() / 4, body.len() % 4);
    print!("header u32[0..6]: ");
    for i in 0..6 { print!("0x{:08X} ", read_u32_le(body, i * 4)); }
    println!();
    let count = read_u32_le(body, 4) as usize;
    println!("count(w1)={count}  w2=0x{:X}={}  w3=0x{:X}={}", read_u32_le(body,8), read_u32_le(body,8), read_u32_le(body,12), read_u32_le(body,12));

    // Segment the WHOLE body into class-runs (u32 granularity) from offset 24.
    println!("\n--- class-run segmentation from off 24 (u32 granularity) ---");
    let mut off = 24usize;
    let mut runs: Vec<(usize, usize, &'static str, u32, u32)> = Vec::new(); // start,count,class,first,last
    while off + 4 <= body.len() {
        let c0 = class(read_u32_le(body, off));
        let start = off;
        let first = read_u32_le(body, off);
        let mut last = first;
        while off + 4 <= body.len() && class(read_u32_le(body, off)) == c0 {
            last = read_u32_le(body, off);
            off += 4;
        }
        runs.push((start, (off - start) / 4, c0, first, last));
    }
    for (s, n, c, fst, lst) in &runs {
        if *n >= 4 || *c == "h8" || *c == "h9" {
            println!("  off {s:>7} (0x{s:06X}) x{n:<6} {c:<6} first=0x{fst:08X} last=0x{lst:08X}");
        }
    }
    // Merge tiny runs summary: total by class.
    println!("\n--- coarse: first big handle array + what follows ---");
    // Assume key array = 'count' u32 from off 24.
    let key_start = 24usize;
    let key_end = key_start + count * 4;
    println!("if key array = count({count}) u32: off {key_start}..{key_end} (end 0x{key_end:X}); body ends {} (0x{:X})", body.len(), body.len());
    if key_end + 4 <= body.len() {
        let rem = body.len() - key_end;
        println!("  remaining after key array: {rem} bytes = {} u32 = {} u16", rem / 4, rem / 2);
        for mult in [count, count + 1, read_u32_le(body,8) as usize, read_u32_le(body,12) as usize] {
            if mult > 0 {
                if rem % (mult) == 0 { println!("    rem / {mult} = {} (exact bytes/entry)", rem / mult); }
                if rem % (mult * 2) == 0 { println!("    rem / ({mult}*2 u16) = {} u16/entry", rem / (mult * 2)); }
            }
        }
        // dump 64 bytes at key_end
        println!("  bytes at key_end (0x{key_end:X}):");
        for row in body[key_end..(key_end + 64).min(body.len())].chunks(16) {
            let hex: Vec<String> = row.iter().map(|b| format!("{b:02X}")).collect();
            println!("    {}", hex.join(" "));
        }
    }
    // Where does the u16-pair tail region start? find last long 'other'/'small' region.
    println!("\n--- tail 64 bytes ---");
    let ts = body.len().saturating_sub(64);
    for row in body[ts..].chunks(16) {
        let hex: Vec<String> = row.iter().map(|b| format!("{b:02X}")).collect();
        println!("  0x{:06X}: {}", ts, hex.join(" "));
    }

    // --- CSR decode: is trailing = [N counts][u16 member list] with N+2*sum(counts) fitting? ---
    println!("\n=== CSR structure test (trailing = [N × countwidth][u16 members]) ===");
    for key_end in [24 + 6126 * 4, 24 + 6127 * 4] {
        for &nn in &[6126usize, 6127, 6128] {
            for cw in [1usize, 2] {
                if key_end + nn * cw > body.len() { continue; }
                let counts_end = key_end + nn * cw;
                let mut sum = 0u64;
                let mut mx = 0u32;
                for i in 0..nn {
                    let v = if cw == 1 { body[key_end + i] as u32 } else { u16::from_le_bytes([body[key_end + i*2], body[key_end + i*2+1]]) as u32 };
                    sum += v as u64; mx = mx.max(v);
                }
                let member_bytes = body.len() - counts_end;
                for mw in [2usize, 4] {
                    if member_bytes as u64 == sum * mw as u64 {
                        println!("  MATCH key_end=0x{key_end:X} N={nn} count_width={cw} member_width={mw}: sum={sum} maxcount={mx} member_bytes={member_bytes}");
                        // box = slot 0 count; last slot count
                        let c0 = if cw==1 { body[key_end] as u32 } else { u16::from_le_bytes([body[key_end], body[key_end+1]]) as u32 };
                        let cl = if cw==1 { body[key_end + (nn-1)] as u32 } else { u16::from_le_bytes([body[key_end+(nn-1)*2], body[key_end+(nn-1)*2+1]]) as u32 };
                        println!("     slot0(box) count={c0}; slot{}(last) count={cl}", nn-1);
                    }
                }
            }
        }
    }
    // Raw: first 32 count-bytes at each key_end, and the value at slot0 (box).
    for key_end in [24 + 6126 * 4, 24 + 6127 * 4] {
        let hx: Vec<String> = body[key_end..(key_end+32).min(body.len())].iter().map(|b| format!("{b:02X}")).collect();
        println!("  first 32 bytes at key_end 0x{key_end:X}: {}", hx.join(" "));
    }

    // --- Is header word[4]=0xD27CC641 a checksum? test crc32_mercs2 + sums over candidate ranges ---
    {
        use mercs2_formats::crc32::crc32_mercs2;
        let target = 0xD27CC641u32;
        let key_end_a = 24 + 6127 * 4; // if 6127-slot key array
        let key_end_b = 24 + 6126 * 4;
        let cands: [(&str, &[u8]); 8] = [
            ("body[24..end]", &body[24..]),
            ("body[0..end]", body),
            ("keys 6127 (24..key_end_a)", &body[24..key_end_a]),
            ("keys 6126 (24..key_end_b)", &body[24..key_end_b]),
            ("trailing after 6127 keys", &body[key_end_a..]),
            ("trailing after 6126 keys", &body[key_end_b..]),
            ("body[20..end] (after count/dims)", &body[20..]),
            ("body[24..end-? ] all-but-hash", &body[24..]),
        ];
        for (label, slice) in cands {
            let c = crc32_mercs2(slice);
            let sum = slice.chunks_exact(4).fold(0u32, |a, w| a.wrapping_add(u32::from_le_bytes([w[0],w[1],w[2],w[3]])));
            let xor = slice.chunks_exact(4).fold(0u32, |a, w| a ^ u32::from_le_bytes([w[0],w[1],w[2],w[3]]));
            let hit = if c == target { " <== CRC MATCH" } else if sum == target { " <== SUM MATCH" } else if xor == target { " <== XOR MATCH" } else { "" };
            println!("  chk {label:38}: crc=0x{c:08X} sum=0x{sum:08X} xor=0x{xor:08X}{hit}");
        }
        // also: is 0xD27CC641 present anywhere else (self-reference)?
    }

    // --- Free-slot check: slot[count-1] value + is it the only trailing-adjacent zero? ---
    {
        let slot_last = 24 + (count - 1) * 4;
        println!("\n  key slot[{}] @0x{slot_last:X} = 0x{:08X} (expect 0 if reserved free slot)", count - 1, read_u32_le(body, slot_last));
        println!("  key slot[{}] @0x{:X} = 0x{:08X} (last used handle)", count - 2, 24 + (count-2)*4, read_u32_le(body, 24 + (count - 2) * 4));
        println!("  u32 at 0x{:X}..: {:?}", slot_last, (0..6).map(|i| format!("0x{:08X}", read_u32_le(body, slot_last + i*4))).collect::<Vec<_>>());
    }

    // --- PARALLEL-ARRAY hypothesis: trailing = sequence of length-N arrays (u8/u16/u32) ---
    // Find true key count by walking handles from off 24.
    let mut nkeys = 0usize; let mut p = 24usize;
    while p + 4 <= body.len() {
        let w = read_u32_le(body, p);
        if (w & 0xFFFF_0000) == 0x8000_0000 || (w & 0xFFFF_0000) == 0x9000_0000 { nkeys += 1; p += 4; } else { break; }
    }
    let key_end = 24 + nkeys * 4;
    println!("\n=== PARALLEL-ARRAY segmentation: nkeys(walked)={nkeys}, header count={count}, key_end=0x{key_end:X} ===");
    // fourcc scan across whole body
    let tags: [&[u8;4];6] = [b"info", b"data", b"schm", b"enum", b"flgs", b"COMP"];
    let mut fcc = Vec::new();
    for o in 0..body.len().saturating_sub(4) {
        for t in &tags { if &body[o..o+4] == *t { fcc.push((o, std::str::from_utf8(*t).unwrap())); } }
    }
    println!("fourcc tags in body: {} {:?}", fcc.len(), &fcc[..fcc.len().min(12)]);
    // Try N in {nkeys, count}: greedily consume arrays of stride 1,2,4 starting at key_end, printing each segment.
    for &nn in [nkeys, count].iter() {
        println!("--- assume N={nn}: greedily fit length-N arrays from key_end ---");
        let mut o = key_end;
        let mut seg = 0;
        while o < body.len() && seg < 12 {
            let rem = body.len() - o;
            // classify the next N-element array by looking at value ranges for stride 1/2/4
            let try_stride = |st: usize| -> Option<(u32,u32,usize)> {
                if nn * st > rem { return None; }
                let (mut mn, mut mx) = (u32::MAX, 0u32);
                for i in 0..nn {
                    let v = match st { 1 => body[o+i] as u32, 2 => u16::from_le_bytes([body[o+i*2],body[o+i*2+1]]) as u32, _ => read_u32_le(body, o+i*4) };
                    mn = mn.min(v); mx = mx.max(v);
                }
                Some((mn, mx, nn*st))
            };
            // pick smallest stride whose max is "reasonable" (u8<256 array of small vals, u16 slot idx < 2*count, u32 handle-ish)
            let s1 = try_stride(1); let s2 = try_stride(2); let s4 = try_stride(4);
            let choose = if let Some((mn,mx,_)) = s1 { if mx <= 0x15 { Some((1usize,mn,mx)) } else { None } } else { None }
                .or_else(|| s2.and_then(|(mn,mx,_)| if mx <= (count as u32*2) { Some((2usize,mn,mx)) } else { None }))
                .or_else(|| s4.map(|(mn,mx,_)| (4usize,mn,mx)));
            match choose {
                Some((st,mn,mx)) => {
                    println!("  seg{seg}: off 0x{o:X} stride {st} x{nn} = {} B, val range [{mn},{mx}] (0x{mn:X}..0x{mx:X})", nn*st);
                    o += nn * st;
                }
                None => { println!("  seg{seg}: off 0x{o:X} — no length-N array fits (rem {rem} B); stopping", ); break; }
            }
            seg += 1;
        }
        println!("  consumed up to 0x{o:X} of 0x{:X} ({} B left)", body.len(), body.len().saturating_sub(o));
    }

    // --- Sub-region hunt: does any contiguous slice have exactly count/count+1/w2/w3 elements? ---
    // The 133 h9 handles end the key array; the rest is trailing. Find the u16-pair region boundary:
    // scan from the END backward for a long run where u16[i]==u16[i+1] (the observed duplicated pairs).
    println!("\n--- u16-pair region detection (from tail backward) ---");
    let u16at = |o: usize| u16::from_le_bytes([body[o], body[o + 1]]);
    let mut pair_start = body.len();
    {
        let mut o = body.len() & !1;
        // walk backward in steps of 4 checking [o-4]==[o-2]
        while o >= 4 {
            let a = u16at(o - 4);
            let b = u16at(o - 2);
            if a == b {
                pair_start = o - 4;
                o -= 4;
            } else {
                break;
            }
        }
    }
    println!("  contiguous trailing dup-u16-pair region starts ~0x{pair_start:06X} (len {} bytes = {} pairs)",
        body.len() - pair_start, (body.len() - pair_start) / 4);
    // How many u32 handles precede? and how big is the middle byte-packed region?
    let key_end_guess = 24 + 6126 * 4; // 24528
    println!("  key array end guess 0x{key_end_guess:06X}; middle byte-packed region = 0x{key_end_guess:06X}..0x{pair_start:06X} = {} bytes",
        pair_start.saturating_sub(key_end_guess));
    // Check element-count matches for the pair region and the middle region.
    let pair_pairs = (body.len() - pair_start) / 4;
    let mid_bytes = pair_start.saturating_sub(key_end_guess);
    for (label, val) in [("count", count), ("count+1", count + 1), ("w2", read_u32_le(body,8) as usize), ("w3", read_u32_le(body,12) as usize), ("keys6126", 6126usize)] {
        if val > 0 {
            let a = if pair_pairs == val { " == pair_pairs!" } else { "" };
            let b = if mid_bytes == val { " == mid_bytes!" } else if mid_bytes % val == 0 { " (mid divisible)" } else { "" };
            println!("    {label}={val}{a}{b}");
        }
    }
    // dump a u16 window at the pair-region start
    let maxpairs = ((body.len().saturating_sub(pair_start)) / 2).min(12);
    println!("  u16 at pair_start: {:?}", (0..maxpairs).map(|i| format!("0x{:04X}", u16at(pair_start + i * 2))).collect::<Vec<_>>());
    println!("  bytes just before pair_start (0x{:06X}):", pair_start.saturating_sub(16));
    let s = pair_start.saturating_sub(16);
    let hex: Vec<String> = body[s..pair_start].iter().map(|b| format!("{b:02X}")).collect();
    println!("    {}", hex.join(" "));
}
