//! THROWAWAY (2026-07-21): prove the terrain_A fixtures are the real WAD bytes.
//! Usage: terrain_provenance <wad> <fixture_suffix> <asset_hash_hex>...
use mercs2_formats::ffcs;
use mercs2_formats::sges;
use std::fs::File;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let wadpath = &a[0];
    let fixdir = &a[1];
    let suffix = &a[2];
    let mut f = File::open(wadpath).unwrap();
    let size = f.metadata().unwrap().len();
    let arc = ffcs::load_ffcs_archive(&mut f, size).unwrap();
    println!("{wadpath}: endian={:?} indx={} aset={} pths={}", arc.endian, arc.indx.len(), arc.aset.len(), arc.paths.len());

    for h in &a[3..] {
        let hash = u32::from_str_radix(h, 16).unwrap();
        let rows: Vec<_> = arc.aset.iter().filter(|r| r.asset_hash == hash).collect();
        if rows.is_empty() { println!("  {h}: NOT IN ASET"); continue; }
        let mut blk = rows[0].block_index();
        if blk == 0xFFFF { blk = (rows[0].packed_block_ref & 0xFFFF) as u16; println!("    [mixed-endian ASET] hi16 is sentinel; using lo16 block={blk}"); }
        let name = arc.paths.get(blk as usize).cloned().unwrap_or_default();
        println!("  {h}: {} aset row(s), block={blk} type=0x{:08x} chain={:?} path={name}",
            rows.len(), rows[0].type_id, rows[0].lod_chain());
        let mut raw = match sges::decompress_block(&mut f, &arc.indx, blk) {
            Ok(v) => v, Err(e) => { println!("    decompress failed: {e}"); continue; }
        };
        if raw.len() >= 4 && &raw[..4] == b"segs" {
            let n0 = raw.len();
            raw = mercs2_formats::dlc_input::decompress_be_sges(&raw, 0, n0).expect("be sges");
            println!("    BE segs: {n0} -> {} bytes", raw.len());
        }
        // walk the entry table (endianness follows the archive)
        let be = matches!(arc.endian, ffcs::Endian::Big);
        let rd = |o: usize| if be { ffcs::read_u32_be(&raw, o) } else { ffcs::read_u32_le(&raw, o) };
        let n = rd(0) as usize;
        println!("    head: {:?}", &raw[..32.min(raw.len())]);
        if n > 64 { println!("    implausible entry count {n}; searching for fixture bytes in the block"); let fx = std::fs::read(format!("{fixdir}/{h}{suffix}")).unwrap(); let hit = raw.windows(64).position(|w| w == &fx[..64]); println!("    fixture[0..64] found at {:?} ; block len {}", hit, raw.len()); continue; }
        let mut off = 4 + n * 16;
        println!("    block len={} entries={n}", raw.len());
        let fix = std::fs::read(format!("{fixdir}/{h}{suffix}")).unwrap();
        let mut found = false;
        for i in 0..n {
            let nh = rd(4 + i * 16);
            let th = rd(8 + i * 16);
            let sz = rd(4 + i * 16 + 12) as usize;
            let body = &raw[off..off + sz];
            if nh == hash {
                let eq = body == fix.as_slice();
                let pref = body.iter().zip(fix.iter()).take_while(|(x, y)| x == y).count();
                println!("    entry[{i}] name=0x{nh:08x} type=0x{th:08x} size={sz} -> fixture len={} MATCH={eq} common_prefix={pref}",
                    fix.len(), );
                found = true;
            }
            off += sz;
        }
        if !found { println!("    (no entry with name_hash 0x{hash:08x} in block {blk})"); }
    }
}
