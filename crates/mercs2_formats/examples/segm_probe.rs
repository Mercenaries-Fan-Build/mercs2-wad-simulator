//! TEMPORARY reverse-engineering probe: dump a model's SEGM records, HIER bones,
//! and per-PRMG-group INFO/PRMT bytes to reverse the drawing-group -> bone binding.
//!
//! Usage: cargo run -p mercs2_formats --example segm_probe -- <vz.wad> 0xA3C1FABC

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::skeleton::Skeleton;
use mercs2_formats::ucfx::parse_block_entry_table;
use std::fs::File;

const MODEL_TYPE_HASH: u32 = 0x5B72_4250;
const MODEL_ASET_TYPE_ID: u32 = 19;

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn rd_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn extract_container(wadpath: &str, name_hash: u32) -> Result<Vec<u8>, String> {
    let mut file = File::open(wadpath).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = load_ffcs_archive(&mut file, size).map_err(|e| e.to_string())?;
    let block = archive
        .aset
        .iter()
        .find(|e| e.asset_hash == name_hash && e.type_id == MODEL_ASET_TYPE_ID && e.is_primary())
        .map(|e| e.block_index())
        .ok_or("no primary model ASET")?;
    let dec = decompress_block(&mut file, &archive.indx, block)?;
    let (count, entries) = parse_block_entry_table(&dec);
    let mut off = 4 + count as usize * 16;
    for e in &entries {
        let end = off + e.chunk_size as usize;
        if e.type_hash == MODEL_TYPE_HASH && e.name_hash == name_hash && end <= dec.len() {
            return Ok(dec[off..end].to_vec());
        }
        off = end;
    }
    Err("model container not found".into())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wad = &args[1];
    if args[2] == "--list" {
        // enumerate model assets and report those with a SEGM chunk (skinned humanoids).
        let mut file = File::open(wad).unwrap();
        let size = file.metadata().unwrap().len();
        let archive = load_ffcs_archive(&mut file, size).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for e in &archive.aset {
            if e.type_id != MODEL_ASET_TYPE_ID || !e.is_primary() || !seen.insert(e.asset_hash) {
                continue;
            }
            if let Ok(u) = extract_container(wad, e.asset_hash) {
                if u.len() > 20 && &u[0..4] == b"UCFX" {
                    let nd = rd_u32(&u, 16) as usize;
                    let mut has_segm = 0usize;
                    for i in 0..nd {
                        let ro = 20 + i * 20;
                        if &u[ro..ro + 4] == b"SEGM" && rd_u32(&u, ro + 4) != 0xFFFF_FFFF {
                            has_segm = rd_u32(&u, ro + 8) as usize / 4;
                        }
                    }
                    if has_segm > 0 {
                        println!("0x{:08X}  segm_records={}", e.asset_hash, has_segm);
                    }
                }
            }
        }
        return;
    }
    let hash = u32::from_str_radix(args[2].trim_start_matches("0x"), 16).unwrap();
    let ucfx = extract_container(wad, hash).expect("extract");

    let data_off = rd_u32(&ucfx, 4) as usize;
    let ndesc = rd_u32(&ucfx, 16) as usize;
    println!("UCFX data_off={data_off} ndesc={ndesc}");

    if args.len() > 3 && args[3] == "--desc" {
        // full descriptor table dump: index, tag, marker?, u0/off, size, first 4 u32 of payload
        for i in 0..ndesc {
            let ro = 20 + i * 20;
            let tag = String::from_utf8_lossy(&ucfx[ro..ro + 4]).to_string();
            let u0 = rd_u32(&ucfx, ro + 4);
            let sz = rd_u32(&ucfx, ro + 8) as usize;
            let x2 = rd_u32(&ucfx, ro + 12);
            let x3 = rd_u32(&ucfx, ro + 16);
            let mk = u0 == 0xFFFF_FFFF;
            let pv = if !mk {
                let o = data_off + u0 as usize;
                let n = (sz / 4).min(6);
                let w: Vec<String> = (0..n).map(|k| format!("{}", rd_u32(&ucfx, o + k * 4))).collect();
                format!("u32={:?}", w)
            } else {
                String::new()
            };
            println!(
                "{i:3} {tag}{} off={u0:<10} sz={sz:<6} x2={x2:<10} x3={x3:<10} {pv}",
                if mk { "*" } else { " " }
            );
        }
        return;
    }

    if args.len() > 3 && args[3] == "--mtrl" {
        // dump MTRL records: index, flags, tex_count, texture hashes
        let mut mtrl = None;
        for i in 0..ndesc {
            let ro = 20 + i * 20;
            if &ucfx[ro..ro + 4] == b"MTRL" && rd_u32(&ucfx, ro + 4) != 0xFFFF_FFFF {
                mtrl = Some((data_off + rd_u32(&ucfx, ro + 4) as usize, rd_u32(&ucfx, ro + 8) as usize));
            }
        }
        let (mo, msz) = mtrl.expect("no MTRL");
        let mut o = mo;
        let mut mi = 0;
        while o + 108 <= mo + msz {
            let flags = rd_u16(&ucfx, o + 104);
            let tc = rd_u16(&ucfx, o + 106) as usize;
            if tc == 0 || tc > 10 {
                break;
            }
            let mut hs = Vec::new();
            for k in 0..tc {
                hs.push(format!("0x{:08X}", rd_u32(&ucfx, o + 108 + k * 4)));
            }
            println!("MTRL[{mi:2}] flags=0x{flags:04X} tex_count={tc} hashes={:?}", hs);
            o += 116 + tc * 4;
            mi += 1;
        }
        return;
    }

    // Reconstruct the marker tree (x3 = subtree descendant count for marker rows).
    // Top-level drawing groups = direct children of the GEOM marker.
    if args.len() > 3 && args[3] == "--tree" {
        // parse SEGM -> seg_id_index -> bone (record order == seg index)
        let mut segm_bone: Vec<u16> = Vec::new();
        for i in 0..ndesc {
            let ro = 20 + i * 20;
            if &ucfx[ro..ro + 4] == b"SEGM" && rd_u32(&ucfx, ro + 4) != 0xFFFF_FFFF {
                let o = data_off + rd_u32(&ucfx, ro + 4) as usize;
                let n = rd_u32(&ucfx, ro + 8) as usize / 4;
                for r in 0..n {
                    segm_bone.push(rd_u16(&ucfx, o + r * 4));
                }
            }
        }
        let mut block = vec![0u8; 20];
        block[16..20].copy_from_slice(&(ucfx.len() as u32).to_le_bytes());
        block.extend_from_slice(&ucfx);
        let skel = Skeleton::from_block(&block).ok();

        // find GEOM marker
        let mut geom = None;
        for i in 0..ndesc {
            let ro = 20 + i * 20;
            if &ucfx[ro..ro + 4] == b"GEOM" && rd_u32(&ucfx, ro + 4) == 0xFFFF_FFFF {
                geom = Some(i);
                break;
            }
        }
        let geom = geom.expect("no GEOM marker");
        let geom_ro = 20 + geom * 20;
        let geom_desc = rd_u32(&ucfx, geom_ro + 16) as usize; // x3 = descendants

        // iterate direct children of GEOM: start at geom+1, each child consumes 1 + its x3.
        let mut idx = geom + 1;
        let end = geom + 1 + geom_desc;
        let mut group_no = 0usize;
        println!("GEOM desc{geom} descendants={geom_desc}");
        println!("grp# tag  x2  x3  segbone  bonepos            child_PRMGs[material]");
        while idx < end {
            let ro = 20 + idx * 20;
            let tag = String::from_utf8_lossy(&ucfx[ro..ro + 4]).to_string();
            let mk = rd_u32(&ucfx, ro + 4) == 0xFFFF_FFFF;
            let x2 = rd_u32(&ucfx, ro + 12);
            let x3 = rd_u32(&ucfx, ro + 16) as usize;
            let sub_end = idx + 1 + x3;
            // collect child PRMG groups' PRMT[0] (material idx) + index_count
            let mut mats: Vec<String> = Vec::new();
            let mut has_area = false;
            let mut has_blend = false;
            for j in (idx + 1)..sub_end {
                let jro = 20 + j * 20;
                let jt = &ucfx[jro..jro + 4];
                let ju0 = rd_u32(&ucfx, jro + 4);
                let jsz = rd_u32(&ucfx, jro + 8) as usize;
                if jt == b"AREA" {
                    has_area = true;
                }
                if jt == b"decl" && ju0 != 0xFFFF_FFFF {
                    // decl elems: count 8-byte-ish; check for BLENDINDICES type. crude: size/?
                    // BLENDINDICES presence heuristic: decl size > 48 with >=7 elements
                    if jsz >= 56 {
                        has_blend = true;
                    }
                }
                if jt == b"PRMT" && ju0 != 0xFFFF_FFFF {
                    let o = data_off + ju0 as usize;
                    for rr in 0..(jsz / 16) {
                        let rb = o + rr * 16;
                        mats.push(format!(
                            "m{}({}v)",
                            rd_u32(&ucfx, rb),
                            rd_u32(&ucfx, rb + 8)
                        ));
                    }
                }
            }
            if mk && (tag == "SKIN" || tag == "MESH") {
                let bone = segm_bone.get(group_no).copied().unwrap_or(0xFFFF);
                let bp = skel
                    .as_ref()
                    .and_then(|s| s.bones.get(bone as usize))
                    .map(|b| {
                        let p = b.world_pos();
                        format!("b{bone:3} [{:5.2},{:5.2},{:5.2}]", p[0], p[1], p[2])
                    })
                    .unwrap_or_else(|| format!("b{bone}"));
                println!(
                    "{group_no:3} {tag} x2={x2:2} x3={x3:3} {bp} area={} blend={} {:?}",
                    has_area, has_blend, mats
                );
                group_no += 1;
                idx = sub_end;
            } else {
                idx += 1;
            }
        }
        return;
    }

    let mut prmg_ix = Vec::new();
    let mut segm = None;
    for i in 0..ndesc {
        let ro = 20 + i * 20;
        let tag = &ucfx[ro..ro + 4];
        let u0 = rd_u32(&ucfx, ro + 4);
        let sz = rd_u32(&ucfx, ro + 8) as usize;
        let mk = u0 == 0xFFFF_FFFF;
        if tag == b"PRMG" && mk {
            prmg_ix.push(i);
        }
        if tag == b"SEGM" && !mk {
            segm = Some((data_off + u0 as usize, sz));
        }
    }

    // HIER via Skeleton (needs a 20-byte wrapper block).
    let mut block = vec![0u8; 20];
    block[16..20].copy_from_slice(&(ucfx.len() as u32).to_le_bytes());
    block.extend_from_slice(&ucfx);
    let skel = Skeleton::from_block(&block).ok();
    if let Some(s) = &skel {
        println!("HIER: {} bones", s.bones.len());
        // dump bones referenced by SEGM + neighbors so we can name them
        for b in &s.bones {
            if [5u32, 6, 7, 8, 31, 41, 42].contains(&(b.index as u32)) {
                let p = b.world_pos();
                println!(
                    "  bone{:3} hash=0x{:08X} parent={:3} pos=[{:6.2},{:6.2},{:6.2}]",
                    b.index, b.name_hash, b.parent, p[0], p[1], p[2]
                );
            }
        }
    }

    if let Some((off, sz)) = segm {
        print!("SEGM raw bytes:");
        for k in 0..sz {
            if k % 16 == 0 {
                print!("\n  ");
            }
            print!("{:02X} ", ucfx[off + k]);
        }
        println!();
        let nrec = sz / 4;
        println!("SEGM off={off} size={sz} -> {nrec} records");
        println!("rec  u16@0  u8@2  u8@3   bone");
        for r in 0..nrec {
            let b = off + r * 4;
            let f0 = rd_u16(&ucfx, b);
            let (f2, f3) = (ucfx[b + 2], ucfx[b + 3]);
            let info = skel
                .as_ref()
                .and_then(|s| s.bones.get(f0 as usize))
                .map(|bn| {
                    let p = bn.world_pos();
                    format!("hash=0x{:08X} pos=[{:6.2},{:6.2},{:6.2}]", bn.name_hash, p[0], p[1], p[2])
                })
                .unwrap_or_default();
            println!("{r:3}  {f0:5}  {f2:4}  {f3:4}   {info}");
        }
    } else {
        println!("NO SEGM");
    }

    // Per-PRMG group: dump PRMG-INFO (u32+f32), all INFO chunks, PRMT, MESH INFO, SKIN.
    println!("\n--- per-group ---");
    let fbits = |u: u32| -> f32 { f32::from_bits(u) };
    for (gi, &pr) in prmg_ix.iter().enumerate() {
        let nxt = prmg_ix.get(gi + 1).copied().unwrap_or(ndesc);
        let mut tags = String::new();
        let mut prmt_hex = String::new();
        // record every INFO chunk in this group with the tag that PRECEDED its marker
        let mut infos: Vec<(String, usize, usize)> = Vec::new(); // (owner_tag, off, size)
        let mut cur_owner = String::from("PRMG");
        let mut mesh_infos: Vec<(usize, usize)> = Vec::new();
        let mut skin_infos: Vec<(usize, usize)> = Vec::new();
        let mut in_mesh = false;
        let mut in_skin = false;
        for i in pr..nxt {
            let ro = 20 + i * 20;
            let tag = &ucfx[ro..ro + 4];
            let u0 = rd_u32(&ucfx, ro + 4);
            let sz = rd_u32(&ucfx, ro + 8) as usize;
            let mk = u0 == 0xFFFF_FFFF;
            tags.push_str(&format!("{}{} ", String::from_utf8_lossy(tag), if mk { "*" } else { "" }));
            if mk {
                // a marker chunk begins a sub-object; remember its tag for following leaf INFO
                cur_owner = String::from_utf8_lossy(tag).to_string();
                in_mesh = tag == b"MESH";
                in_skin = tag == b"SKIN";
            }
            if !mk {
                let o = data_off + u0 as usize;
                if tag == b"INFO" {
                    infos.push((cur_owner.clone(), o, sz));
                    if in_mesh {
                        mesh_infos.push((o, sz));
                    }
                    if in_skin {
                        skin_infos.push((o, sz));
                    }
                }
                if tag == b"PRMT" {
                    let mut recs = Vec::new();
                    for rr in 0..(sz / 16) {
                        let rb = o + rr * 16;
                        recs.push(format!(
                            "{{{},{},{},{}}}",
                            rd_u32(&ucfx, rb),
                            rd_u32(&ucfx, rb + 4),
                            rd_u32(&ucfx, rb + 8),
                            rd_u32(&ucfx, rb + 12)
                        ));
                    }
                    prmt_hex = format!("PRMT[{sz}B]={}", recs.join(" "));
                }
            }
        }
        println!("G{gi:2} :: {tags}");
        println!("     {prmt_hex}");
        for (owner, o, sz) in &infos {
            let n = (sz / 4).min(15);
            let u: Vec<String> = (0..n).map(|k| format!("{}", rd_u32(&ucfx, o + k * 4))).collect();
            let ff: Vec<String> = (0..n).map(|k| format!("{:.4}", fbits(rd_u32(&ucfx, o + k * 4)))).collect();
            println!("     [{owner}].INFO[{sz}B] u32={:?}", u);
            println!("                       f32={:?}", ff);
        }
        // dump MESH data chunk raw (may contain a bone/seg index)
        for (o, sz) in &mesh_infos {
            print!("     MESH.INFO raw[{sz}B]:");
            for k in 0..(*sz).min(64) {
                if k % 16 == 0 { print!("\n       "); }
                print!("{:02X} ", ucfx[o + k]);
            }
            println!();
        }
        for (o, sz) in &skin_infos {
            print!("     SKIN.INFO raw[{sz}B]:");
            for k in 0..(*sz).min(64) {
                if k % 16 == 0 { print!("\n       "); }
                print!("{:02X} ", ucfx[o + k]);
            }
            println!();
        }
    }
}
