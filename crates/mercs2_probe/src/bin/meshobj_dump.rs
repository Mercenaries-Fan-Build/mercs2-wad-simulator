//! `meshobj_dump` — dump every WpMeshShape16 object's raw bytes + ALL local fixups landing inside it,
//! classifying each pointer target as packfile-resident (< pkend) or wrapper (>= pkend). Decisive for the
//! collide hunt: which subpart pointer(s) the engine follows to reach vertices, and what the +40/+44
//! "secondary array" is. Reads a whole PHY2 body file (argv[1]) = [prefix][packfile][wrapper].

use mercs2_formats::havok::{parse_packfile_raw, parse_phy2_body, Shape, HAVOK_MAGIC};
use std::io::Read;

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn f32le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().cloned().expect("usage: meshobj_dump <phy2body> [--reauthor]");
    let reauthor = args.iter().any(|a| a == "--reauthor");
    let mut body = Vec::new();
    std::fs::File::open(&path).unwrap().read_to_end(&mut body).unwrap();

    if reauthor {
        // Decode the input floor's meshes, re-author via build_phy2_multi, replace `body` with the output.
        let pf = parse_phy2_body(&body).expect("parse input phy2");
        let mut soups: Vec<(Vec<[u32; 3]>, Vec<[f32; 3]>)> = Vec::new();
        for sh in &pf.shapes {
            if let Shape::Mesh(m) = sh {
                if m.indices.is_empty() { continue; }
                let tris: Vec<[u32; 3]> = m.indices.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect();
                soups.push((tris, m.vertices.clone()));
            }
        }
        println!("[reauthor] decoded {} mesh soup(s) from input", soups.len());
        body = mercs2_formats::phy2_build::build_phy2_multi("reauthor_floor", &soups).expect("build_phy2_multi");
        println!("[reauthor] authored body {} bytes\n", body.len());
    }
    let off = body.windows(8).position(|w| w == HAVOK_MAGIC).expect("no havok magic");
    let pk = &body[off..];
    let raw = parse_packfile_raw(pk).expect("parse raw");
    let pkend_body = off + raw.size; // body-absolute wrapper start

    // Parse GLOBAL fixups (src, sec, dst) — RawPackfile doesn't expose them. Replicate the header walk.
    let cn = pk.windows(14).position(|w| w == b"__classnames__").expect("classnames");
    let sh = cn; // find_sub returns start of "__classnames__" which is the section-header name field start
    let mut secs = [[0u32; 7]; 3];
    for (s, sec) in secs.iter_mut().enumerate() {
        for (kk, field) in sec.iter_mut().enumerate() {
            *field = u32le(pk, sh + s * 48 + 20 + kk * 4);
        }
    }
    let body0 = sh + 3 * 48;
    let data_pk = body0 + secs[0][6] as usize + secs[1][6] as usize;
    let (d_gf, d_vf) = (secs[2][2] as usize, secs[2][3] as usize);
    let mut gf: Vec<(usize, usize, usize)> = Vec::new();
    let mut kk = data_pk + d_gf;
    while kk + 12 <= data_pk + d_vf {
        let src = u32le(pk, kk);
        if src == 0xFFFF_FFFF { break; }
        gf.push((src as usize, u32le(pk, kk + 4) as usize, u32le(pk, kk + 8) as usize));
        kk += 12;
    }
    println!("global fixups total = {}", gf.len());

    // Dump wrapper EE blocks (0xEEEEEEEE): nverts@+68, pool-ptr@+76 (body-absolute), classify target.
    println!("\n== wrapper EE blocks (pkend_body={pkend_body}) ==");
    let mut i = pkend_body;
    while i + 4 <= body.len() {
        if u32le(&body, i) == 0xEEEE_EEEE {
            let nverts = u32le(&body, i + 68);
            let poolptr = u32le(&body, i + 76) as usize; // body-absolute offset
            let tailptr = u32le(&body, i + 4) as usize;
            let region = if poolptr < pkend_body { format!("PACKFILE(body+{poolptr}, pk+{})", poolptr.saturating_sub(off)) } else { format!("WRAPPER(wrapper+{})", poolptr.saturating_sub(pkend_body)) };
            println!("  EE @wrapper+{:<5} nverts={nverts:<6} +76(pool)->{region}  +4(tail)->body+{tailptr}", i - pkend_body);
        }
        i += 4;
    }
    println!();
    println!("file {path}");
    println!("packfile @body+{off}, raw.size={}, pkend(body-abs)={pkend_body}, body.len={}", raw.size, body.len());
    println!("data_pk(rel to pk)={}", raw.data_pk);

    // classify: pointer target given as pk-relative absolute
    let classify = |abs_in_pk: usize| -> String {
        let body_abs = off + abs_in_pk;
        if body_abs < pkend_body {
            format!("PACKFILE (body+{body_abs}, pk+{abs_in_pk})")
        } else {
            format!("WRAPPER   (body+{body_abs}, wrapper+{})", body_abs - pkend_body)
        }
    };

    // collect mesh objects
    let mut mesh_srcs: Vec<usize> = raw
        .vfixups
        .iter()
        .filter(|(_, n)| n == "WpMeshShape16")
        .map(|(s, _)| *s)
        .collect();
    mesh_srcs.sort();
    println!("\n{} WpMeshShape16 object(s): {:?}\n", mesh_srcs.len(), mesh_srcs);

    for (mi, &src) in mesh_srcs.iter().enumerate() {
        let obj = raw.data_pk + src;
        println!("===== WpMeshShape16 #{mi}  vfixup.src={src}  obj(pk-abs)={obj} =====");
        // dump obj+0..112 as u32 hex + float interpretation
        for row in 0..7 {
            let base = row * 16;
            let mut hexs = String::new();
            let mut fls = String::new();
            for c in 0..4 {
                let o = obj + base + c * 4;
                if o + 4 <= pk.len() {
                    let v = u32le(pk, o);
                    hexs += &format!("{v:08x} ");
                    let f = f32le(pk, o);
                    fls += &format!("{:>12.4} ", if f.is_finite() { f } else { 0.0 });
                }
            }
            println!("  obj+{:<3} [{}]  f=[{}]", base, hexs.trim_end(), fls.trim_end());
        }
        // ALL local fixups whose src is inside [src, src+112)
        println!("  -- local fixups landing in obj [{src}..{}):", src + 112);
        let mut lfs: Vec<(usize, usize)> = raw
            .lf
            .iter()
            .filter(|(s, _)| **s >= src && **s < src + 112)
            .map(|(s, d)| (*s, *d))
            .collect();
        lfs.sort();
        for (s, d) in lfs {
            let field = s - src;
            let tgt_pk = raw.data_pk + d; // pk-relative absolute
            println!("     field obj+{:<3} -> LOCAL {}", field, classify(tgt_pk));
        }
        println!("  -- GLOBAL fixups landing in obj [{src}..{}):", src + 112);
        let mut gfs: Vec<&(usize, usize, usize)> = gf.iter().filter(|(s, _, _)| *s >= src && *s < src + 112).collect();
        gfs.sort();
        for (s, sec, d) in gfs {
            println!("     field obj+{:<3} -> GLOBAL sec{} {}", s - src, sec, classify(raw.data_pk + d));
        }
        // Interpret candidate subpart layouts.
        println!("  -- interpretation:");
        println!("     obj+28 (reader subpart-arr ptr fixup?)  nsub@obj+32 = {}", u32le(pk, obj + 32));
        println!("     obj+52 (ctor subpart-arr ptr?)          cnt@obj+56  = {}", u32le(pk, obj + 56));
        // subpart at obj+48 (reader), dump subpart+0..48
        let sp = obj + 48;
        println!("  -- subpart @obj+48 (reader model):");
        for row in 0..3 {
            let base = row * 16;
            let mut hexs = String::new();
            let mut fls = String::new();
            for c in 0..4 {
                let o = sp + base + c * 4;
                if o + 4 <= pk.len() {
                    hexs += &format!("{:08x} ", u32le(pk, o));
                    let f = f32le(pk, o);
                    fls += &format!("{:>12.4} ", if f.is_finite() { f } else { 0.0 });
                }
            }
            println!("     sp+{:<3} [{}]  f=[{}]", base, hexs.trim_end(), fls.trim_end());
        }
        println!("     sp+36 acnt = {}", u32le(pk, sp + 36));

        // Resolve the three fixups and dump their target regions.
        let idx_ptr = raw.lf.get(&(src + 80)).map(|d| raw.data_pk + d);
        let sec_ptr = raw.lf.get(&(src + 88)).map(|d| raw.data_pk + d);
        let acnt = u32le(pk, sp + 36) as usize;
        let sec_cnt = u32le(pk, obj + 92) as usize;
        if let Some(ap) = idx_ptr {
            println!("  -- INDEX array @pk+{ap} (acnt={acnt}), first 4 tris (u16 a,b,c,pad):");
            for t in 0..4.min(acnt) {
                let o = ap + t * 8;
                println!("       [{} {} {} | {}]", u16::from_le_bytes([pk[o],pk[o+1]]), u16::from_le_bytes([pk[o+2],pk[o+3]]), u16::from_le_bytes([pk[o+4],pk[o+5]]), u16::from_le_bytes([pk[o+6],pk[o+7]]));
            }
            println!("       index array spans pk+{ap}..pk+{} ({} B)", ap + acnt * 8, acnt * 8);
        }
        if let Some(scp) = sec_ptr {
            println!("  -- SECONDARY array @pk+{scp} (count={sec_cnt}), first 128 bytes as u32:");
            for row in 0..8 {
                let mut hexs = String::new();
                for c in 0..4 {
                    let o = scp + row * 16 + c * 4;
                    if o + 4 <= pk.len() { hexs += &format!("{:08x} ", u32le(pk, o)); }
                }
                println!("       +{:<3} {}", row * 16, hexs.trim_end());
            }
            // guess element size
            println!("       sec spans from pk+{scp}; if next obj at pk+? -> element size guess");
        }

        // Locate mesh#mi's vertex pool by scanning the WHOLE body for a base that makes tris sane.
        let min = [f32le(pk, sp), f32le(pk, sp + 4), f32le(pk, sp + 8)];
        let scale = [f32le(pk, sp + 16), f32le(pk, sp + 20), f32le(pk, sp + 24)];
        // gather indices
        let mut tris: Vec<[usize; 3]> = Vec::new();
        if let Some(ap) = idx_ptr {
            for t in 0..acnt {
                let o = ap + t * 8;
                tris.push([
                    u16::from_le_bytes([pk[o], pk[o + 1]]) as usize,
                    u16::from_le_bytes([pk[o + 2], pk[o + 3]]) as usize,
                    u16::from_le_bytes([pk[o + 4], pk[o + 5]]) as usize,
                ]);
            }
        }
        let maxidx = tris.iter().flatten().copied().max().unwrap_or(0);
        let nverts = maxidx + 1;
        let getb = |pool: usize, v: usize| -> [f32; 3] {
            let o = pool + v * 6;
            [
                min[0] + u16::from_le_bytes([pk[o], pk[o + 1]]) as f32 * scale[0],
                min[1] + u16::from_le_bytes([pk[o + 2], pk[o + 3]]) as f32 * scale[1],
                min[2] + u16::from_le_bytes([pk[o + 4], pk[o + 5]]) as f32 * scale[2],
            ]
        };
        let edge = |p: [f32; 3], q: [f32; 3]| ((p[0]-q[0]).powi(2)+(p[1]-q[1]).powi(2)+(p[2]-q[2]).powi(2)).sqrt();
        let score = |pool: usize| -> f64 {
            let mut ok = 0usize;
            for t in &tris {
                let (a,b,c) = (getb(pool,t[0]),getb(pool,t[1]),getb(pool,t[2]));
                if edge(a,b) < 40.0 && edge(b,c) < 40.0 && edge(a,c) < 40.0 { ok += 1; }
            }
            ok as f64 / tris.len().max(1) as f64
        };
        let mut best = (0.0f64, 0usize);
        let end = pk.len().saturating_sub(nverts * 6);
        let mut base = 0usize;
        while base <= end {
            let t0 = tris[0];
            let (a,b,c) = (getb(base,t0[0]),getb(base,t0[1]),getb(base,t0[2]));
            if edge(a,b) > 0.001 && edge(a,b) < 40.0 && edge(b,c) < 40.0 && edge(a,c) < 40.0 {
                let s = score(base);
                if s > best.0 { best = (s, base); if s > 0.999 { break; } }
            }
            base += 2; // pools are u16-aligned
        }
        let region = if off + best.1 < pkend_body { "PACKFILE" } else { "WRAPPER" };
        println!("  -- VERTEX POOL scan: best score {:.3} @pk+{} (body+{}) => {region}  [nverts={nverts}]", best.0, best.1, off + best.1);
        // Forced check: score THIS mesh's tris against the SHARED wrapper pool at wrapper+1232 (body+pkend+1232).
        let shared_body = pkend_body + 1232;
        let shared_pk = shared_body.saturating_sub(off);
        if shared_pk + nverts * 6 <= pk.len() {
            println!("       [forced] score vs shared wrapper pool @wrapper+1232 (body+{shared_body}) = {:.3}", score(shared_pk));
        }
        // distance from index/secondary array ends
        if let Some(ap) = idx_ptr { println!("       pool - (index_end) = {} ", best.1 as i64 - (ap + acnt*8) as i64); }
        if let Some(scp) = sec_ptr { println!("       pool - sec_start = {}", best.1 as i64 - scp as i64); }
        println!();
    }
}
