//! THROWAWAY audit probe (2026-07-21): verify every numeric claim in
//! docs/terrainmesh_reencode_{implementation,investigation_A,investigation_B}.md
//! against the real Xbox-BE / PC-retail terrainmesh bytes.
//!
//! Usage: cargo run -p ucfx_byteswap --bin terrain_audit -- <fixture_dir> [asset...]

use std::collections::HashSet;
use std::path::PathBuf;

const TERRAIN_TYPE: u32 = 0x7C56_9307;

#[derive(Clone, Debug)]
struct Desc {
    tag: [u8; 4],
    u0: u32,
    size: u32,
    _u3: u32,
    _u4: u32,
}

fn rd(b: &[u8], o: usize, be: bool) -> u32 {
    let v = [b[o], b[o + 1], b[o + 2], b[o + 3]];
    if be { u32::from_be_bytes(v) } else { u32::from_le_bytes(v) }
}

struct Container<'a> {
    buf: &'a [u8],
    be: bool,
    dao: usize,
    descs: Vec<Desc>,
}

fn parse(buf: &[u8]) -> Container<'_> {
    let be = &buf[0..4] == b"XFCU";
    assert!(be || &buf[0..4] == b"UCFX", "bad magic {:?}", &buf[0..4]);
    let dao = rd(buf, 4, be) as usize;
    let n = rd(buf, 16, be) as usize;
    let mut descs = Vec::with_capacity(n);
    for i in 0..n {
        let r = 20 + i * 20;
        let mut tag = [buf[r], buf[r + 1], buf[r + 2], buf[r + 3]];
        if be {
            tag.reverse();
        }
        descs.push(Desc {
            tag,
            u0: rd(buf, r + 4, be),
            size: rd(buf, r + 8, be),
            _u3: rd(buf, r + 12, be),
            _u4: rd(buf, r + 16, be),
        });
    }
    Container { buf, be, dao, descs }
}

impl<'a> Container<'a> {
    fn body(&self, i: usize) -> Option<&'a [u8]> {
        let d = &self.descs[i];
        if d.u0 == 0xFFFF_FFFF {
            return None;
        }
        let s = if self.dao > 0 { self.dao + d.u0 as usize } else { 8 + d.u0 as usize };
        let e = s + d.size as usize;
        if e <= self.buf.len() { Some(&self.buf[s..e]) } else { None }
    }
    fn tag(&self, i: usize) -> String {
        String::from_utf8_lossy(&self.descs[i].tag).to_string()
    }
}

// ---------------------------------------------------------------- half floats
fn f16_to_f32(h: u16) -> f32 {
    let s = ((h >> 15) & 1) as u32;
    let e = ((h >> 10) & 0x1f) as u32;
    let m = (h & 0x3ff) as u32;
    let bits = if e == 0 {
        if m == 0 { s << 31 } else {
            let mut e2: i32 = -1;
            let mut m2 = m;
            while m2 & 0x400 == 0 { m2 <<= 1; e2 -= 1; }
            m2 &= 0x3ff;
            (s << 31) | (((127 - 15 + e2 + 1) as u32) << 23) | (m2 << 13)
        }
    } else if e == 31 {
        (s << 31) | 0x7f80_0000 | (m << 13)
    } else {
        (s << 31) | ((e + 127 - 15) << 23) | (m << 13)
    };
    f32::from_bits(bits)
}

fn f32_to_f16(f: f32) -> u16 {
    // round-to-nearest-even, same as convert.rs::f32_to_f16_bits
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let mut mant = (x & 0x007f_ffff) as i32;
    let exp = ((x >> 23) & 0xff) as i32;
    if exp == 0xff { return sign | 0x7c00 | (if mant != 0 { 0x0200 } else { 0 }); }
    let mut e = exp - 127 + 15;
    if e >= 0x1f { return sign | 0x7c00; }
    if e <= 0 {
        if e < -10 { return sign; }
        mant |= 0x0080_0000;
        let shift = 14 - e;
        let hm0 = mant >> shift;
        let rem = mant & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        let mut hm = hm0 as u16;
        if rem > halfway || (rem == halfway && (hm & 1) == 1) { hm += 1; }
        return sign | hm;
    }
    let mut hm = (mant >> 13) as u16;
    let rem = mant & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && (hm & 1) == 1) {
        hm += 1;
        if hm == 0x0400 { hm = 0; e += 1; if e >= 0x1f { return sign | 0x7c00; } }
    }
    sign | ((e as u16) << 10) | hm
}

fn sx(v: u32, bits: u32) -> i32 {
    let m = (v & ((1u32 << bits) - 1)) as i32;
    let half = 1i32 << (bits - 1);
    if m >= half { m - (1 << bits) } else { m }
}

/// candidate decoders: (name, fn(u32)->(f32,f32,f32))
fn decode_1110(u: u32) -> (f32, f32, f32) {
    (sx(u, 11) as f32 / 1023.0, sx(u >> 11, 11) as f32 / 1023.0, sx(u >> 22, 10) as f32 / 511.0)
}
fn decode_101010(u: u32) -> (f32, f32, f32) {
    (sx(u, 10) as f32 / 511.0, sx(u >> 10, 10) as f32 / 511.0, sx(u >> 20, 10) as f32 / 511.0)
}

fn norm(v: (f32, f32, f32)) -> (f32, f32, f32) {
    let m = (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt();
    if m > 0.0 { (v.0 / m, v.1 / m, v.2 / m) } else { (0.0, 0.0, 0.0) }
}

// ---------------------------------------------------------------- decl parsing
#[derive(Debug, Clone)]
struct Elem { offset: usize, typ: u8, usage: u8 }

fn pc_type_size(t: u8) -> usize {
    match t {
        0 => 4, 1 => 8, 2 => 12, 3 => 16,  // FLOAT1..4
        4 => 4,                            // D3DCOLOR
        5 => 4, 6 => 8,                    // UBYTE4, SHORT2
        7 => 8,                            // SHORT4
        8 => 4, 9 => 8, 10 => 8,           // UBYTE4N, SHORT2N, SHORT4N
        11 => 8, 12 => 8,                  // USHORT2N, USHORT4N
        13 => 4, 14 => 4, 15 => 4,         // UDEC3, DEC3N, FLOAT16_2
        16 => 8,                           // FLOAT16_4
        _ => 0,
    }
}

fn parse_decl(decl: &[u8]) -> (Vec<Elem>, usize) {
    let mut elems = Vec::new();
    let mut stride = 0usize;
    let mut p = 8usize;
    while p + 8 <= decl.len() {
        let stream = u16::from_le_bytes([decl[p], decl[p + 1]]);
        let typ = decl[p + 4];
        if stream == 0x00ff || typ == 17 { break; }
        let offset = u16::from_le_bytes([decl[p + 2], decl[p + 3]]) as usize;
        let usage = decl[p + 6];
        if stream == 0 {
            let end = offset + pc_type_size(typ);
            if end > stride { stride = end; }
            elems.push(Elem { offset, typ, usage });
        }
        p += 8;
    }
    (elems, stride)
}

fn usage_name(u: u8) -> &'static str {
    match u { 0 => "POSITION", 3 => "NORMAL", 5 => "TEXCOORD", 6 => "TANGENT", 7 => "BINORMAL", 10 => "COLOR", _ => "?" }
}
fn type_name(t: u8) -> &'static str {
    match t { 4 => "D3DCOLOR", 5 => "UBYTE4", 15 => "FLOAT16_2", 16 => "FLOAT16_4", 13 => "UDEC3", 14 => "DEC3N", _ => "?" }
}

// ---------------------------------------------------------------- destrip
/// Split on 0xFFFF, emit triangle-list triples with alternating winding, drop degenerates.
fn destrip(idx: &[u16]) -> Vec<[u16; 3]> {
    let mut out = Vec::new();
    let mut run: Vec<u16> = Vec::new();
    let flush = |run: &Vec<u16>, out: &mut Vec<[u16; 3]>| {
        for i in 0..run.len().saturating_sub(2) {
            let (a, b, c) = if i % 2 == 0 { (run[i], run[i + 1], run[i + 2]) } else { (run[i + 1], run[i], run[i + 2]) };
            if a != b && b != c && a != c { out.push([a, b, c]); }
        }
    };
    for &v in idx {
        if v == 0xFFFF { flush(&run, &mut out); run.clear(); } else { run.push(v); }
    }
    flush(&run, &mut out);
    out
}

fn as_list(idx: &[u16]) -> Vec<[u16; 3]> {
    idx.chunks_exact(3).filter(|c| c[0] != c[1] && c[1] != c[2] && c[0] != c[2])
        .map(|c| [c[0], c[1], c[2]]).collect()
}

fn as_single_strip(idx: &[u16]) -> Vec<[u16; 3]> {
    let mut out = Vec::new();
    for i in 0..idx.len().saturating_sub(2) {
        let (a, b, c) = if i % 2 == 0 { (idx[i], idx[i + 1], idx[i + 2]) } else { (idx[i + 1], idx[i], idx[i + 2]) };
        if a != b && b != c && a != c { out.push([a, b, c]); }
    }
    out
}

fn triset(t: &[[u16; 3]]) -> HashSet<[u16; 3]> {
    t.iter().map(|x| { let mut s = *x; s.sort(); s }).collect()
}

// ---------------------------------------------------------------- converter
fn run_converter(be_body_with_csum: &[u8]) -> Vec<u8> {
    // Wrap the single entry body in a minimal 1-entry BE block.
    let mut blk = Vec::new();
    blk.extend_from_slice(&1u32.to_be_bytes());
    blk.extend_from_slice(&0xCA67_E07Bu32.to_be_bytes());
    blk.extend_from_slice(&TERRAIN_TYPE.to_be_bytes());
    blk.extend_from_slice(&0u32.to_be_bytes());
    blk.extend_from_slice(&(be_body_with_csum.len() as u32).to_be_bytes());
    blk.extend_from_slice(be_body_with_csum);
    let out = ucfx_byteswap::convert::convert_block(&blk, false, None).expect("convert failed");
    // strip [u32 n][16B entry] header and the trailing CSUM
    out[20..out.len() - 8].to_vec()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(args.first().cloned().unwrap_or_else(|| ".".into()));
    let assets: Vec<String> = if args.len() > 1 { args[1..].to_vec() } else {
        vec!["CA67E07B".into(), "D0E2F48D".into(), "81348F94".into()]
    };

    // per-usage angular stats for both candidate layouts: usage -> (n, sum1110, max1110, sum1010, max1010)
    let mut g_usage: std::collections::BTreeMap<u8, (usize, f64, f64, f64, f64)> = Default::default();
    let mut g_distinct_norm: HashSet<(u32, [u8; 8])> = HashSet::new();
    let mut g_distinct_all: HashSet<(u32, [u8; 8])> = HashSet::new();
    // newconv STRM data byte agreement, and per-byte-offset diff histogram for the stride-20 class
    let mut g_nc_bytes_eq = 0usize;
    let mut g_nc_bytes_tot = 0usize;
    let mut g_off20: [usize; 20] = [0; 20];
    let mut g_off20_tot = 0usize;
    // global accumulators for the DEC3N survey
    let mut g_norm_pairs = 0usize;
    let mut g_all_pairs = 0usize;
    let mut g_ang_sum_n = 0f64;
    let mut g_ang_max_n = 0f64;
    let mut g_ang_sum_all = 0f64;
    let mut g_ang_max_all = 0f64;
    let mut g_byteexact_1110 = 0usize;
    let mut g_byteexact_best = 0usize;
    let mut g_strm_groups = 0usize;
    let mut g_stride_rule_ok = 0usize;
    let mut g_stride_declend_ok = 0usize;

    for a in &assets {
        let be_raw = std::fs::read(dir.join(format!("{a}_be.bin"))).unwrap();
        let conv_raw = std::fs::read(dir.join(format!("{a}_conv.bin"))).unwrap();
        let pc_raw = std::fs::read(dir.join(format!("{a}_pc.bin"))).unwrap();
        let cut = |v: &Vec<u8>| -> Vec<u8> {
            if v.len() >= 8 && &v[v.len() - 8..v.len() - 4] == b"CSUM" { v[..v.len() - 8].to_vec() } else { v.clone() }
        };
        let be_b = cut(&be_raw);
        let conv_b = cut(&conv_raw);
        let pc_b = cut(&pc_raw);
        let newconv_raw = run_converter(&be_raw);
        let newconv_b = cut(&newconv_raw);

        println!("\n================ ASSET {a} ================");
        println!("file sizes: be={} conv(old)={} pc={} newconv={}  |  pc-be={}",
            be_raw.len(), conv_raw.len(), pc_raw.len(), newconv_raw.len(),
            pc_raw.len() as i64 - be_raw.len() as i64);

        let be = parse(&be_b);
        let cv = parse(&conv_b);
        let pc = parse(&pc_b);
        let nc = parse(&newconv_b);
        println!("headers: be(dao={},n={}) conv(dao={},n={}) pc(dao={},n={}) newconv(dao={},n={})",
            be.dao, be.descs.len(), cv.dao, cv.descs.len(), pc.dao, pc.descs.len(), nc.dao, nc.descs.len());

        let tags_eq = be.descs.len() == pc.descs.len()
            && be.descs.iter().zip(pc.descs.iter()).all(|(x, y)| x.tag == y.tag);
        println!("tag sequence be==pc: {tags_eq}");

        // tag counts
        let mut tagcount: std::collections::BTreeMap<String, usize> = Default::default();
        for i in 0..pc.descs.len() { *tagcount.entry(pc.tag(i)).or_default() += 1; }
        println!("tag counts: {:?}", tagcount);

        // per-tag size delta / byte-eq, old conv and new conv
        for (label, c) in [("OLDCONV", &cv), ("NEWCONV", &nc)] {
            if c.descs.len() != pc.descs.len() { println!("  [{label}] descriptor count differs, skip"); continue; }
            let mut agg: std::collections::BTreeMap<String, (usize, i64, usize, usize)> = Default::default();
            for i in 0..pc.descs.len() {
                let (ob, pb) = (c.body(i), pc.body(i));
                if let (Some(o), Some(p)) = (ob, pb) {
                    let e = agg.entry(pc.tag(i)).or_default();
                    e.0 += 1;
                    e.1 += p.len() as i64 - o.len() as i64;
                    if p.len() == o.len() { e.2 += 1; if p == o { e.3 += 1; } }
                }
            }
            println!("  [{label}] tag  n  sum(pc-ours)  size_eq  byte_eq");
            let mut tot = 0i64;
            for (t, (n, d, se, bee)) in &agg { println!("    {t:6} {n:4} {d:>10}  {se:4} {bee:4}"); tot += d; }
            println!("    NET DELTA = {tot}");
        }

        // ---- walk STRM / IBUF groups
        let mut strm_idx = Vec::new();
        let mut ibuf_idx = Vec::new();
        for i in 0..pc.descs.len() {
            if pc.descs[i].u0 == 0xFFFF_FFFF {
                match &pc.descs[i].tag {
                    b"STRM" => strm_idx.push(i),
                    b"IBUF" => ibuf_idx.push(i),
                    _ => {}
                }
            }
        }
        println!("STRM groups={} IBUF groups={}", strm_idx.len(), ibuf_idx.len());

        // ---------- STRM survey ----------
        let mut stride_rule_ok = 0;
        let mut stride_declend_ok = 0;
        let mut decl_byteeq = 0;
        let mut pos_exact = 0usize; let mut pos_tot = 0usize;
        let mut col_rev = 0usize; let mut col_rot = 0usize; let mut col_tot = 0usize; let mut col_distinct = 0usize;
        let mut f162_exact = 0usize; let mut f162_tot = 0usize;
        let mut printed_sample = false;

        for &gi in &strm_idx {
            // children: info, decl, data
            let (mut ii, mut di, mut ai) = (None, None, None);
            for k in 1..=4 {
                if gi + k >= pc.descs.len() { break; }
                match &pc.descs[gi + k].tag {
                    b"info" => ii = Some(gi + k),
                    b"decl" => di = Some(gi + k),
                    b"data" => ai = Some(gi + k),
                    _ => break,
                }
            }
            let (ii, di, ai) = match (ii, di, ai) { (Some(a), Some(b), Some(c)) => (a, b, c), _ => continue };
            let be_info = be.body(ii).unwrap();
            let pc_info = pc.body(ii).unwrap();
            let be_stride = u32::from_be_bytes(be_info[4..8].try_into().unwrap()) as usize;
            let be_nv = u32::from_be_bytes(be_info[8..12].try_into().unwrap()) as usize;
            let pc_stride = u32::from_le_bytes(pc_info[4..8].try_into().unwrap()) as usize;
            let pc_nv = u32::from_le_bytes(pc_info[8..12].try_into().unwrap()) as usize;

            let pdecl = pc.body(di).unwrap();
            let cdecl = cv.body(di).unwrap();
            if pdecl == cdecl { decl_byteeq += 1; }
            let (elems, declend) = parse_decl(pdecl);
            let n_f164 = elems.iter().filter(|e| e.typ == 16).count();

            if pc_stride == be_stride + 4 * n_f164 { stride_rule_ok += 1; }
            if pc_stride == declend { stride_declend_ok += 1; }

            let be_data = be.body(ai).unwrap();
            let pc_data = pc.body(ai).unwrap();

            if !printed_sample {
                println!("  sample STRM group desc#{gi}: be_info=({}, {}, {}) pc_info=({}, {}, {}) declend={} n_f16_4={}",
                    u32::from_be_bytes(be_info[0..4].try_into().unwrap()), be_stride, be_nv,
                    u32::from_le_bytes(pc_info[0..4].try_into().unwrap()), pc_stride, pc_nv, declend, n_f164);
                for e in &elems { println!("      elem off={} type={}({}) usage={}({})", e.offset, e.typ, type_name(e.typ), e.usage, usage_name(e.usage)); }
                println!("      be_data len={} (== {}*{}? {}) pc_data len={} (== {}*{}? {})",
                    be_data.len(), be_stride, be_nv, be_data.len() == be_stride * be_nv,
                    pc_data.len(), pc_stride, pc_nv, pc_data.len() == pc_stride * pc_nv);
                println!("      be v0 = {}", hex(&be_data[..be_stride.min(be_data.len())]));
                println!("      pc v0 = {}", hex(&pc_data[..pc_stride.min(pc_data.len())]));
                printed_sample = true;
            }

            if be_nv != pc_nv || be_data.len() < be_stride * be_nv || pc_data.len() < pc_stride * pc_nv { continue; }

            // per-vertex element checks
            for v in 0..be_nv {
                let bs = &be_data[v * be_stride..(v + 1) * be_stride];
                let ps = &pc_data[v * pc_stride..(v + 1) * pc_stride];
                // POSITION: implicit 8B at off 0, per-u16 byteswap
                pos_tot += 1;
                let mut want = [0u8; 8];
                for h in 0..4 { want[h * 2] = bs[h * 2 + 1]; want[h * 2 + 1] = bs[h * 2]; }
                if want == ps[0..8] { pos_exact += 1; }

                // BE source offsets: position 8B then declared elements in decl order,
                // each at its BE-packed size (FLOAT16_4 => 4B on Xbox, else pc size).
                let mut bo = 8usize;
                for e in &elems {
                    let bsz = if e.typ == 16 { 4 } else { pc_type_size(e.typ) };
                    let src = &bs[bo..bo + bsz];
                    let dst = &ps[e.offset..e.offset + pc_type_size(e.typ)];
                    match e.typ {
                        4 | 5 => { // D3DCOLOR / UBYTE4
                            col_tot += 1;
                            let rev = [src[3], src[2], src[1], src[0]];
                            let rot = [src[1], src[2], src[3], src[0]];
                            if rev == dst { col_rev += 1; }
                            if rot == dst { col_rot += 1; }
                            if src[0] != src[1] || src[1] != src[2] || src[2] != src[3] { col_distinct += 1; }
                        }
                        15 => { // FLOAT16_2
                            f162_tot += 1;
                            let w = [src[1], src[0], src[3], src[2]];
                            if w == dst { f162_exact += 1; }
                        }
                        16 => { // packed -> FLOAT16_4
                            let u = u32::from_be_bytes(src.try_into().unwrap());
                            let pcv = (
                                f16_to_f32(u16::from_le_bytes([dst[0], dst[1]])),
                                f16_to_f32(u16::from_le_bytes([dst[2], dst[3]])),
                                f16_to_f32(u16::from_le_bytes([dst[4], dst[5]])),
                            );
                            let pcn = norm(pcv);
                            let d1110 = norm(decode_1110(u));
                            let d1010 = norm(decode_101010(u));
                            let ang = |a: (f32, f32, f32), b: (f32, f32, f32)| -> f64 {
                                let d = (a.0 * b.0 + a.1 * b.1 + a.2 * b.2).clamp(-1.0, 1.0) as f64;
                                d.acos().to_degrees()
                            };
                            let a1 = ang(d1110, pcn);
                            let a2 = ang(d1010, pcn);
                            let (best, chosen) = if e.usage == 3 { (a1, d1110) } else { (a2.min(a1), if a2 <= a1 { d1010 } else { d1110 }) };
                            g_all_pairs += 1;
                            g_ang_sum_all += best; if best > g_ang_max_all { g_ang_max_all = best; }
                            if e.usage == 3 {
                                g_norm_pairs += 1;
                                g_ang_sum_n += a1; if a1 > g_ang_max_n { g_ang_max_n = a1; }
                            }
                            // byte-exactness of the re-encode
                            let enc = |n: (f32, f32, f32)| -> [u8; 8] {
                                let mut o = [0u8; 8];
                                o[0..2].copy_from_slice(&f32_to_f16(n.0).to_le_bytes());
                                o[2..4].copy_from_slice(&f32_to_f16(n.1).to_le_bytes());
                                o[4..6].copy_from_slice(&f32_to_f16(n.2).to_le_bytes());
                                o[6..8].copy_from_slice(&f32_to_f16(1.0).to_le_bytes());
                                o
                            };
                            if enc(d1110) == dst { g_byteexact_1110 += 1; }
                            if enc(chosen) == dst { g_byteexact_best += 1; }
                            let u_e = g_usage.entry(e.usage).or_default();
                            u_e.0 += 1;
                            u_e.1 += a1; if a1 > u_e.2 { u_e.2 = a1; }
                            u_e.3 += a2; if a2 > u_e.4 { u_e.4 = a2; }
                            let mut d8 = [0u8; 8]; d8.copy_from_slice(dst);
                            g_distinct_all.insert((u, d8));
                            if e.usage == 3 { g_distinct_norm.insert((u, d8)); }
                        }
                        _ => {}
                    }
                    bo += bsz;
                }
            }
            // NEWCONV byte agreement on this STRM data body (sizes should match pc)
            if let Some(nb) = nc.body(ai) {
                if nb.len() == pc_data.len() {
                    let eq = nb.iter().zip(pc_data.iter()).filter(|(a, b)| a == b).count();
                    g_nc_bytes_eq += eq;
                    g_nc_bytes_tot += nb.len();
                    if pc_stride == 20 {
                        for v in 0..pc_nv {
                            for k in 0..20 {
                                if nb[v * 20 + k] != pc_data[v * 20 + k] { g_off20[k] += 1; }
                            }
                        }
                        g_off20_tot += pc_nv;
                    }
                }
            }
            g_strm_groups += 1;
        }
        g_stride_rule_ok += stride_rule_ok;
        g_stride_declend_ok += stride_declend_ok;
        println!("  STRM: decl byte-eq(conv vs pc) {}/{}; stride rule (be+4*nF16_4) {}/{}; stride==decl_end {}/{}",
            decl_byteeq, strm_idx.len(), stride_rule_ok, strm_idx.len(), stride_declend_ok, strm_idx.len());
        println!("  STRM vertex elements: POSITION per-u16-swap exact {}/{} ; FLOAT16_2 exact {}/{} ; COLOR: u32-reverse {}/{} rotate {}/{} (samples w/ >=2 distinct bytes: {})",
            pos_exact, pos_tot, f162_exact, f162_tot, col_rev, col_tot, col_rot, col_tot, col_distinct);

        // ---------- IBUF survey ----------
        let mut tot_be_idx = 0usize; let mut tot_pc_idx = 0usize;
        let mut n_restart = 0usize;
        let mut pc_has_restart = 0usize;
        let mut pc_div3 = 0usize;
        let mut set_match_list = 0usize; let mut set_match_strip = 0usize;
        let mut ibuf_n = 0usize;
        let mut seq_match = 0usize; let mut vset_match = 0usize;
        let mut first = true;
        for &gi in &ibuf_idx {
            let (mut ii, mut ai) = (None, None);
            for k in 1..=3 {
                if gi + k >= pc.descs.len() { break; }
                match &pc.descs[gi + k].tag { b"info" => ii = Some(gi + k), b"data" => ai = Some(gi + k), _ => break }
            }
            let (ii, ai) = match (ii, ai) { (Some(a), Some(b)) => (a, b), _ => continue };
            let be_info = be.body(ii).unwrap(); let pc_info = pc.body(ii).unwrap();
            if be_info.len() < 4 || pc_info.len() < 4 { continue; }
            let bec = u32::from_be_bytes(be_info[0..4].try_into().unwrap()) as usize;
            let pcc = u32::from_le_bytes(pc_info[0..4].try_into().unwrap()) as usize;
            let bed = be.body(ai).unwrap(); let pcd = pc.body(ai).unwrap();
            if bed.len() != bec * 2 || pcd.len() != pcc * 2 { println!("  IBUF#{gi}: len mismatch be {} vs {}*2, pc {} vs {}*2", bed.len(), bec, pcd.len(), pcc); }
            let bi: Vec<u16> = bed.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            let pi: Vec<u16> = pcd.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let nr = bi.iter().filter(|&&v| v == 0xFFFF).count();
            let pr = pi.iter().filter(|&&v| v == 0xFFFF).count();
            tot_be_idx += bec; tot_pc_idx += pcc; n_restart += nr; if pr > 0 { pc_has_restart += 1; }
            if pcc % 3 == 0 { pc_div3 += 1; }
            let bt = destrip(&bi);
            let sl = triset(&as_list(&pi));
            let ss = triset(&as_single_strip(&pi));
            let bs = triset(&bt);
            if sl == bs { set_match_list += 1; }
            if ss == bs { set_match_strip += 1; }
            // degenerate-window statistics: a stitched strip has degenerate windows,
            // a triangle list has (almost) none at chunk-aligned triples.
            let degen_win = (0..pi.len().saturating_sub(2))
                .filter(|&i| pi[i] == pi[i + 1] || pi[i + 1] == pi[i + 2] || pi[i] == pi[i + 2]).count();
            let degen_chunk = pi.chunks_exact(3)
                .filter(|c| c[0] == c[1] || c[1] == c[2] || c[0] == c[2]).count();
            // "PC re-indexes vertices" test: collapse consecutive duplicates in the PC
            // strip and compare with the Xbox non-restart index sequence.
            let mut pc_collapsed: Vec<u16> = Vec::new();
            for &v in &pi { if pc_collapsed.last() != Some(&v) { pc_collapsed.push(v); } }
            let be_norestart: Vec<u16> = bi.iter().cloned().filter(|&v| v != 0xFFFF).collect();
            let mut be_collapsed: Vec<u16> = Vec::new();
            for &v in &be_norestart { if be_collapsed.last() != Some(&v) { be_collapsed.push(v); } }
            let seq_eq = pc_collapsed == be_collapsed;
            if seq_eq { seq_match += 1; }
            let bset: HashSet<u16> = be_norestart.iter().cloned().collect();
            let pset: HashSet<u16> = pi.iter().cloned().collect();
            if bset == pset { vset_match += 1; }
            if first {
                println!("    collapse-dupes: pc_seq==be_seq? {seq_eq} (pc {} -> {}, be {} -> {}) ; index-VALUE set equal? {}",
                    pi.len(), pc_collapsed.len(), be_norestart.len(), be_collapsed.len(), bset == pset);
                println!("    pc degenerate windows (strip view) = {degen_win}/{} ; degenerate aligned triples (list view) = {degen_chunk}/{}",
                    pi.len().saturating_sub(2), pi.len() / 3);
                println!("  IBUF sample desc#{gi}: be_count={bec} (restarts={nr}) pc_count={pcc} (restarts={pr}) pc%3={} maxpc={:?}",
                    pcc % 3, pi.iter().max());
                println!("    be first16 = {:?}", &bi[..16.min(bi.len())]);
                println!("    pc first16 = {:?}", &pi[..16.min(pi.len())]);
                println!("    destripped xbox tris={} ; naive list indices={} ; pc-as-list tris={} ; pc-as-strip tris={}",
                    bt.len(), bt.len() * 3, sl.len(), ss.len());
                println!("    tri-SET equal? pc-as-list={} pc-as-strip={} | xbox set size={} list set={} strip set={}",
                    sl == bs, ss == bs, bs.len(), sl.len(), ss.len());
                // overlap measures
                let inter_l = sl.intersection(&bs).count();
                let inter_s = ss.intersection(&bs).count();
                println!("    overlap: |pc_list ∩ xbox|={inter_l}/{} |pc_strip ∩ xbox|={inter_s}/{}", bs.len(), bs.len());
                first = false;
            }
            ibuf_n += 1;
        }
        println!("  IBUF: n={ibuf_n} be_idx_total={tot_be_idx} restarts={n_restart} pc_idx_total={tot_pc_idx} delta_bytes={} ; pc bufs with restart={pc_has_restart} ; pc count%3==0 in {}/{}",
            (tot_pc_idx as i64 - tot_be_idx as i64) * 2, pc_div3, ibuf_n);
        println!("  IBUF triangle-SET match: as-list {}/{} ; as-single-strip {}/{}", set_match_list, ibuf_n, set_match_strip, ibuf_n);
        println!("  IBUF collapse-dupes seq match {}/{} ; index-value-SET match {}/{}", seq_match, ibuf_n, vset_match, ibuf_n);

        // ---------- MTRL / PRMT / root INFO ----------
        for i in 0..pc.descs.len() {
            let t = pc.tag(i);
            if t == "MTRL" {
                let (o, p) = (cv.body(i).unwrap(), pc.body(i).unwrap());
                let n = nc.body(i).map(|x| x.len()).unwrap_or(0);
                let fd = o.iter().zip(p.iter()).position(|(a, b)| a != b);
                println!("  MTRL: conv={} pc={} delta={} newconv={} first_diff(oldconv vs pc)={:?}", o.len(), p.len(), p.len() as i64 - o.len() as i64, n, fd);
                if let Some(f) = fd {
                    println!("    @{f}: conv={} pc={}", hex(&o[f..(f + 8).min(o.len())]), hex(&p[f..(f + 8).min(p.len())]));
                }
                if let Some(nb) = nc.body(i) {
                    let fd2 = nb.iter().zip(p.iter()).position(|(a, b)| a != b);
                    println!("    first_diff(newconv vs pc)={:?}", fd2);
                }
            }
        }
        // root INFO (32B) first descriptor
        if let (Some(o), Some(p)) = (cv.body(0), pc.body(0)) {
            println!("  INFO[0] ({}B): conv={} \n                  pc  ={}", o.len(), hex(o), hex(p));
        }
        // PRMT aggregate
        let mut prmt_n = 0; let mut prmt_delta = 0i64; let mut prmt_smaller = 0;
        for i in 0..pc.descs.len() {
            if pc.tag(i) == "PRMT" {
                if let (Some(o), Some(p)) = (cv.body(i), pc.body(i)) {
                    prmt_n += 1; prmt_delta += p.len() as i64 - o.len() as i64;
                    if p.len() < o.len() { prmt_smaller += 1; }
                }
            }
        }
        println!("  PRMT: n={prmt_n} sum(pc-conv)={prmt_delta} smaller_in={prmt_smaller}");
        // PRMT per-record sizes
        let mut pr: Vec<(usize, usize)> = Vec::new();
        for i in 0..pc.descs.len() {
            if pc.tag(i) == "PRMT" { if let (Some(o), Some(p)) = (cv.body(i), pc.body(i)) { pr.push((o.len(), p.len())); } }
        }
        pr.sort(); pr.dedup();
        println!("    PRMT distinct (conv,pc) size pairs: {:?}", pr);
        // MTRL sub-record marker census
        for i in 0..pc.descs.len() {
            if pc.tag(i) != "MTRL" { continue; }
            let (o, p) = (be.body(i).unwrap(), pc.body(i).unwrap());
            let m_le: [u8; 4] = [0xa7, 0x72, 0xcd, 0xa3];
            let m_be: [u8; 4] = [0xa3, 0xcd, 0x72, 0xa7];
            let cnt = |b: &[u8], m: [u8; 4]| b.windows(4).filter(|w| *w == m).count();
            println!("    MTRL marker a772cda3: in PC={} in BE={} | a3cd72a7: in PC={} in BE={}",
                cnt(p, m_le), cnt(o, m_le), cnt(p, m_be), cnt(o, m_be));
        }
        // per-INFO differing u32 word census
        let mut wordhist: std::collections::BTreeMap<usize, usize> = Default::default();
        let mut ndiff_hist: std::collections::BTreeMap<usize, usize> = Default::default();
        for i in 0..pc.descs.len() {
            if pc.tag(i) != "INFO" { continue; }
            if let (Some(o), Some(p)) = (cv.body(i), pc.body(i)) {
                if o.len() != p.len() { continue; }
                let mut nd = 0;
                for w in 0..o.len() / 4 {
                    if o[w * 4..w * 4 + 4] != p[w * 4..w * 4 + 4] { nd += 1; *wordhist.entry(w).or_default() += 1; }
                }
                *ndiff_hist.entry(nd).or_default() += 1;
            }
        }
        println!("    INFO differing-word index histogram: {:?}", wordhist);
        println!("    INFO records by #differing u32 words: {:?}", ndiff_hist);

        // ---------- PC body contiguity (impl doc: "PC bodies are contiguous, zero-gap, no pad") ----------
        for (label, c) in [("PC", &pc), ("BE", &be)] {
            let mut spans: Vec<(usize, usize)> = Vec::new();
            for i in 0..c.descs.len() {
                let d = &c.descs[i];
                if d.u0 == 0xFFFF_FFFF { continue; }
                spans.push((d.u0 as usize, d.u0 as usize + d.size as usize));
            }
            spans.sort();
            let mut gaps = 0usize; let mut gapbytes = 0i64; let mut overlaps = 0usize; let mut cur = 0usize;
            for (s, e) in &spans {
                if *s > cur { gaps += 1; gapbytes += (*s - cur) as i64; }
                else if *s < cur { overlaps += 1; }
                if *e > cur { cur = *e; }
            }
            let tail = c.buf.len() as i64 - (c.dao + cur) as i64;
            println!("  [{label}] body layout: n={} gaps={gaps} gap_bytes={gapbytes} overlaps={overlaps} end={} tail_after_last_body={tail}",
                spans.len(), c.dao + cur);
        }

        // ---------- byte-agreement % on size-equal info/INFO chunks (impl doc 87.8% / 87.7%) ----------
        for tag in ["info", "INFO"] {
            for (label, c) in [("OLDCONV", &cv), ("NEWCONV", &nc)] {
                let (mut eq, mut tot) = (0usize, 0usize);
                for i in 0..pc.descs.len() {
                    if pc.tag(i) != tag { continue; }
                    if let (Some(o), Some(p)) = (c.body(i), pc.body(i)) {
                        if o.len() == p.len() {
                            eq += o.iter().zip(p.iter()).filter(|(a, b)| a == b).count();
                            tot += o.len();
                        }
                    }
                }
                if tot > 0 { println!("  {tag} byte-agreement [{label}] = {:.1}% ({eq}/{tot})", 100.0 * eq as f64 / tot as f64); }
            }
        }
    }

    println!("\n================ GLOBAL DEC3N SURVEY ================");
    println!("STRM groups examined: {g_strm_groups}; stride rule ok {g_stride_rule_ok}; stride==decl_end ok {g_stride_declend_ok}");
    println!("NORMAL-usage pairs: {g_norm_pairs}  mean_ang(11-11-10)={:.4}deg max={:.4}deg", g_ang_sum_n / g_norm_pairs.max(1) as f64, g_ang_max_n);
    println!("ALL packed pairs:   {g_all_pairs}  mean_ang(best-per-usage)={:.4}deg max={:.4}deg", g_ang_sum_all / g_all_pairs.max(1) as f64, g_ang_max_all);
    println!("byte-exact re-encode: 11-11-10-everywhere {g_byteexact_1110}/{g_all_pairs} ({:.2}%) ; per-usage-best {g_byteexact_best}/{g_all_pairs} ({:.2}%)",
        100.0 * g_byteexact_1110 as f64 / g_all_pairs.max(1) as f64,
        100.0 * g_byteexact_best as f64 / g_all_pairs.max(1) as f64);
    println!("distinct (BE u32 -> PC f16x4) pairs: NORMAL={} ALL={}", g_distinct_norm.len(), g_distinct_all.len());
    println!("per-usage angular error, 11-11-10 vs 10-10-10:");
    for (u, (n, s1, m1, s2, m2)) in &g_usage {
        println!("  usage {u:2} ({:9}) n={n:6}  11-11-10 mean={:.4} max={:.4}   |  10-10-10 mean={:.4} max={:.4}",
            usage_name(*u), s1 / *n as f64, m1, s2 / *n as f64, m2);
    }
    println!("NEWCONV STRM data byte agreement vs PC: {:.2}% ({g_nc_bytes_eq}/{g_nc_bytes_tot})",
        100.0 * g_nc_bytes_eq as f64 / g_nc_bytes_tot.max(1) as f64);
    println!("stride-20 class per-byte-offset diff counts over {g_off20_tot} vertices:");
    for k in 0..20 { print!(" [{k}]={}", g_off20[k]); }
    println!();
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join("")
}
