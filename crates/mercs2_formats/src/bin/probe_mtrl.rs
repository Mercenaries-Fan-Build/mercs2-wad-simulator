//! Probe MTRL materials + per-group material binding. Read-only.
//! Usage: probe_mtrl <block.bin>

use mercs2_formats::ffcs::read_u32_le;

fn main() {
    let path = std::env::args().nth(1).expect("block path");
    let block = std::fs::read(&path).expect("read");
    let ulen = read_u32_le(&block, 16) as usize;
    let ucfx = &block[20..20 + ulen];
    let data_off = read_u32_le(ucfx, 4) as usize;
    let ndesc = read_u32_le(ucfx, 16) as usize;

    let leaf_at = |i: usize| -> (usize, usize, [u8; 4]) {
        let ro = 20 + i * 20;
        let mut t = [0u8; 4];
        t.copy_from_slice(&ucfx[ro..ro + 4]);
        (data_off + read_u32_le(ucfx, ro + 4) as usize, read_u32_le(ucfx, ro + 8) as usize, t)
    };
    let cont = |i: usize| read_u32_le(ucfx, 20 + i * 20 + 4) == 0xFFFF_FFFF;

    // MTRL: walk 116-byte material records; each record's texture hashes follow a
    // 0003009X marker. Print material index -> first (diffuse) texture hash.
    let mut mtrl = None;
    for i in 0..ndesc {
        let (o, sz, t) = leaf_at(i);
        if &t == b"MTRL" && !cont(i) {
            mtrl = Some((o, sz));
        }
    }
    let (mo, msz) = mtrl.expect("MTRL");
    eprintln!("MTRL @ {mo} size {msz} ({} x116 = {})", msz / 116, (msz / 116) * 116);
    let m = &ucfx[mo..mo + msz];
    let nmat = msz / 116;
    let mut mat_diffuse = vec![0u32; nmat];
    for mi in 0..nmat {
        let base = mi * 116;
        // find the 0003009X marker within this record, diffuse = next u32
        let mut diffuse = 0u32;
        for w in 0..(116 / 4) {
            let v = read_u32_le(m, base + w * 4);
            if v & 0xffff_0000 == 0x0003_0000 {
                if base + (w + 1) * 4 + 4 <= msz {
                    diffuse = read_u32_le(m, base + (w + 1) * 4);
                }
                break;
            }
        }
        mat_diffuse[mi] = diffuse;
        eprintln!("  mat{mi:>2}: diffuse={diffuse:#010x}");
    }

    // Per PRMG group: find a material index. The PRMG INFO leaf (first info after
    // PRMG marker, but the GROUP-level info) often carries a u32 material index.
    // Print each group's leading INFO u32s so we can spot the material index.
    let prmg: Vec<usize> = (0..ndesc).filter(|&i| {
        let ro = 20 + i * 20; &ucfx[ro..ro + 4] == b"PRMG" && read_u32_le(ucfx, ro + 4) == 0xFFFF_FFFF
    }).collect();
    eprintln!("--- {} PRMG groups: field[5]=matidx -> diffuse ---", prmg.len());
    for (gi, &pr) in prmg.iter().enumerate() {
        let nxt = if gi + 1 < prmg.len() { prmg[gi + 1] } else { ndesc };
        for i in (pr + 1)..nxt {
            let (o, sz, t) = leaf_at(i);
            if &t == b"INFO" && !cont(i) {
                let matidx = read_u32_le(ucfx, o + 20) as usize;
                let f0 = read_u32_le(ucfx, o + 0);
                let diff = mat_diffuse.get(matidx).copied().unwrap_or(0);
                eprintln!("  grp{gi:>2}: f0(nmat?)={f0} matidx={matidx} diffuse={diff:#010x}");
                let _ = sz;
                break;
            }
        }
    }
}
