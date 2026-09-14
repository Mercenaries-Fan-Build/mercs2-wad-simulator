//! landingcraft_mint — mint a NOVEL static-prop spawn template ("landing_craft") into the worldentity
//! resident singleton by cloning the minimal "box" template (0x80000002) under a fresh handle with its
//! `ModelName` repointed to the landing-craft model 0x592057C4, plus a `Name` registry record.
//!
//! The edit happens IN PLACE inside the ALREADY-DEPLOYED resident block (`blocks\VZ\resident_P000_Q3`)
//! carried by the live landing-craft overlay — so no ASET row, no registration, no block path changes;
//! the worldentity container merely grows by the Name record + the joined bucket keys. That overlay's
//! resident-block override already loads 100% for the user, so this inherits proven packaging (no
//! resident double-registration; jc2-arc-recon-detour #39).
//!
//! guidmap is LEFT UNCHANGED — it is the save/load + network guid<->entity cross-reference
//! (type_hash_registry 0x140E8728), NOT the Pg.Spawn resolution path (that is the Name registry
//! @0xDF6B88 built from the Name COMP; object_assembly_model.md §3). The box template carries ZERO
//! outbound handle references, so the clone introduces no dangling reference.
//!
//! Usage: landingcraft_mint <merged_overlay.wad> <out.wad> [vz.wad]
//!   arg1 = the live landing-craft MERGED overlay (resident block + model block)
//!   arg2 = output overlay (resident block edited in place; model block untouched)
//!   arg3 = base vz.wad (OPTIONAL) — only to verify the overlay's worldentity == base's

use std::fs::File;

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::patch_wad::{merge_patch_wads, read_patch_wad, PatchBlock};
use mercs2_formats::schema::ComponentSchema;
use mercs2_formats::sges::{compress_sges, decompress_block, decompress_sges};
use sha2::{Digest, Sha256};

const WORLDENTITY_TYPE: u32 = 0x5647_C35D;
const GUIDMAP_TYPE: u32 = 0x140E_8728;
const BOX_HANDLE: u32 = 0x8000_0002;
const LC_MODEL: u32 = 0x5920_57C4;
const NEW_NAME: &str = "landing_craft";
/// Every shared-config COMP the box (0x80000002) participates in besides ModelName. Each of these is
/// a shared-payload bucket carrying only floats/hashes/counts (no handle refs) — joining is safe.
const JOIN_COMPS: [&str; 5] = [
    "Health",
    "HibernationControl",
    "MaterialMapping",
    "ObjectMaterial",
    "_PropPhysics",
];

fn sha16(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    let d = h.finalize();
    d.iter().take(8).map(|x| format!("{x:02x}")).collect()
}
fn sha_full(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}

// ----- UCFX container decompose/rebuild (byte-identical round-trip); copied from jc2_template_mint -----
struct Ucfx {
    data_area_off: u32,
    w8: u32,
    w12: u32,
    descs: Vec<([u8; 4], u32, u32, u32, u32)>,
    bodies: Vec<Option<Vec<u8>>>,
    trailer: Vec<u8>,
}
impl Ucfx {
    fn parse(c: &[u8]) -> Ucfx {
        assert_eq!(&c[0..4], b"UCFX", "not a UCFX container");
        let data_area_off = read_u32_le(c, 4);
        let w8 = read_u32_le(c, 8);
        let w12 = read_u32_le(c, 12);
        let ndesc = read_u32_le(c, 16) as usize;
        let mut descs = Vec::with_capacity(ndesc);
        for i in 0..ndesc {
            let ro = 20 + i * 20;
            let mut tag = [0u8; 4];
            tag.copy_from_slice(&c[ro..ro + 4]);
            descs.push((
                tag,
                read_u32_le(c, ro + 4),
                read_u32_le(c, ro + 8),
                read_u32_le(c, ro + 12),
                read_u32_le(c, ro + 16),
            ));
        }
        let data_start = data_area_off as usize;
        let mut bodies = Vec::with_capacity(ndesc);
        let mut max_end = data_start;
        for &(_, row_u0, size, _, _) in &descs {
            if row_u0 == 0xFFFF_FFFF {
                bodies.push(None);
            } else {
                let s = data_start + row_u0 as usize;
                let e = s + size as usize;
                bodies.push(Some(c[s..e].to_vec()));
                max_end = max_end.max(e);
            }
        }
        let trailer = c[max_end..].to_vec();
        Ucfx { data_area_off, w8, w12, descs, bodies, trailer }
    }

    fn build(&self) -> Vec<u8> {
        let ndesc = self.descs.len();
        let data_start = 20 + ndesc * 20;
        assert_eq!(data_start, self.data_area_off as usize, "data_area_off must equal header+desc table");
        let mut order: Vec<usize> = (0..ndesc).filter(|&i| self.descs[i].1 != 0xFFFF_FFFF).collect();
        order.sort_by_key(|&i| self.descs[i].1);
        let mut body_region: Vec<u8> = Vec::new();
        let mut new_row: Vec<u32> = vec![0xFFFF_FFFF; ndesc];
        let mut new_size: Vec<u32> = self.descs.iter().map(|d| d.2).collect();
        for &i in &order {
            let b = self.bodies[i].as_ref().unwrap();
            new_row[i] = body_region.len() as u32;
            new_size[i] = b.len() as u32;
            body_region.extend_from_slice(b);
        }
        let mut out = Vec::with_capacity(data_start + body_region.len() + self.trailer.len());
        out.extend_from_slice(b"UCFX");
        out.extend_from_slice(&self.data_area_off.to_le_bytes());
        out.extend_from_slice(&self.w8.to_le_bytes());
        out.extend_from_slice(&self.w12.to_le_bytes());
        out.extend_from_slice(&(ndesc as u32).to_le_bytes());
        for i in 0..ndesc {
            let (tag, _, _, w3, w4) = self.descs[i];
            out.extend_from_slice(&tag);
            out.extend_from_slice(&new_row[i].to_le_bytes());
            out.extend_from_slice(&new_size[i].to_le_bytes());
            out.extend_from_slice(&w3.to_le_bytes());
            out.extend_from_slice(&w4.to_le_bytes());
        }
        out.extend_from_slice(&body_region);
        if self.trailer.len() >= 8 && &self.trailer[0..4] == b"CSUM" {
            let crc = crc32_mercs2(&out);
            out.extend_from_slice(b"CSUM");
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&self.trailer[8..]);
        } else {
            out.extend_from_slice(&self.trailer);
        }
        out
    }

    fn comp_group(&self, class_name: &str) -> Option<(Option<usize>, usize)> {
        let mut i = 0;
        while i < self.descs.len() {
            if &self.descs[i].0 == b"COMP" && self.descs[i].1 == 0xFFFF_FFFF {
                let (mut info_idx, mut schm_idx, mut data_idx) = (None, None, None);
                let mut j = i + 1;
                while j < self.descs.len() && self.descs[j].1 != 0xFFFF_FFFF {
                    match &self.descs[j].0 {
                        b"info" => info_idx = Some(j),
                        b"schm" => schm_idx = Some(j),
                        b"data" => data_idx = Some(j),
                        _ => {}
                    }
                    j += 1;
                }
                if let (Some(ii), Some(di)) = (info_idx, data_idx) {
                    if self.bodies[ii].as_ref().and_then(|b| parse_info_name(b)).as_deref()
                        == Some(class_name)
                    {
                        return Some((schm_idx, di));
                    }
                }
                i = j;
            } else {
                i += 1;
            }
        }
        None
    }
}

struct Bucket {
    keys: Vec<u32>,
    payload: Vec<u8>,
}
fn parse_buckets(data: &[u8], p: usize) -> Option<Vec<Bucket>> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos + 4 <= data.len() {
        let n = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if n == 0 || n > 1_000_000 {
            return None;
        }
        if pos + n * 4 + p > data.len() {
            return None;
        }
        let mut keys = Vec::with_capacity(n);
        for k in 0..n {
            keys.push(u32::from_le_bytes([
                data[pos + k * 4],
                data[pos + k * 4 + 1],
                data[pos + k * 4 + 2],
                data[pos + k * 4 + 3],
            ]));
        }
        pos += n * 4;
        out.push(Bucket { keys, payload: data[pos..pos + p].to_vec() });
        pos += p;
    }
    (pos == data.len()).then_some(out)
}
fn build_buckets(buckets: &[Bucket]) -> Vec<u8> {
    let mut out = Vec::new();
    for b in buckets {
        out.extend_from_slice(&(b.keys.len() as u32).to_le_bytes());
        for &k in &b.keys {
            out.extend_from_slice(&k.to_le_bytes());
        }
        out.extend_from_slice(&b.payload);
    }
    out
}
fn parse_info_name(info: &[u8]) -> Option<String> {
    let nul = info.iter().position(|&x| x == 0)?;
    if nul > 0 && info[..nul].iter().all(|&x| (32..127).contains(&x)) {
        Some(String::from_utf8_lossy(&info[..nul]).into_owned())
    } else {
        None
    }
}

/// A resident block = `[u32 count][count × (name,type,fieldc,size)][bodies...]`. Extract the entries
/// (with bodies) so a single body can be swapped and the block re-emitted.
struct ResidentBlock {
    entries: Vec<(u32, u32, u32, Vec<u8>)>, // name, type, field_c, body
}
impl ResidentBlock {
    fn parse(blk: &[u8]) -> ResidentBlock {
        let count = read_u32_le(blk, 0) as usize;
        let mut pos = 4 + count * 16;
        let mut entries = Vec::with_capacity(count);
        for ei in 0..count {
            let base = 4 + ei * 16;
            let nh = read_u32_le(blk, base);
            let th = read_u32_le(blk, base + 4);
            let fc = read_u32_le(blk, base + 8);
            let sz = read_u32_le(blk, base + 12) as usize;
            entries.push((nh, th, fc, blk[pos..pos + sz].to_vec()));
            pos += sz;
        }
        ResidentBlock { entries }
    }
    fn find(&self, type_hash: u32) -> Option<usize> {
        self.entries.iter().position(|e| e.1 == type_hash)
    }
    fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for (nh, th, fc, b) in &self.entries {
            out.extend_from_slice(&nh.to_le_bytes());
            out.extend_from_slice(&th.to_le_bytes());
            out.extend_from_slice(&fc.to_le_bytes());
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        }
        for (_, _, _, b) in &self.entries {
            out.extend_from_slice(b);
        }
        out
    }
}

/// Parse a guidmap CHDR into its flat handle array (for the free-handle collision check only).
fn guidmap_handles(gm: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    if gm.len() < 40 || &gm[0..4] != b"UCFX" {
        return out;
    }
    let data_off = read_u32_le(gm, 4) as usize;
    let ndesc = read_u32_le(gm, 16) as usize;
    for i in 0..ndesc {
        let ro = 20 + i * 20;
        if &gm[ro..ro + 4] == b"CHDR" {
            let u0 = read_u32_le(gm, ro + 4) as usize;
            if u0 == 0xFFFF_FFFF {
                continue;
            }
            let start = data_off + u0;
            let mut p = start + 24; // 24-byte header
            while p + 4 <= gm.len() {
                let w = read_u32_le(gm, p);
                if (w & 0xFFFF_0000) == 0x8000_0000 || (w & 0xFFFF_0000) == 0x9000_0000 {
                    out.push(w);
                    p += 4;
                } else {
                    break;
                }
            }
            break;
        }
    }
    out
}

/// Fill the guidmap's reserved free key slot (index count-1, shipped as 0) with `new_handle`, so the
/// world-load registry-build enumerates it. ZERO structural change: no shift, no resize, no count bump
/// (count already accounts for 6127 slots; only 6126 are used, slot 6126 is a reserved 0). Returns the
/// rebuilt guidmap container (Ucfx CSUM recomputed) + (count, slot_byte_offset_in_body). Aborts if the
/// slot is not 0 (interpretation guard against corrupting a live entry).
fn guidmap_fill_free_slot(gm: &[u8], new_handle: u32) -> (Vec<u8>, u32, usize) {
    let mut u = Ucfx::parse(gm);
    assert_eq!(u.build(), gm, "guidmap UCFX round-trip not byte-identical — aborting");
    // find the CHDR body descriptor
    let chdr_idx = (0..u.descs.len())
        .find(|&i| &u.descs[i].0 == b"CHDR" && u.bodies[i].is_some())
        .expect("guidmap has no CHDR body");
    let body = u.bodies[chdr_idx].as_mut().unwrap();
    let count = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
    let slot_off = 24 + (count as usize - 1) * 4; // last key slot (reserved/free)
    let cur = u32::from_le_bytes([body[slot_off], body[slot_off + 1], body[slot_off + 2], body[slot_off + 3]]);
    assert_eq!(cur, 0, "guidmap free slot[{}] @body+0x{slot_off:X} is 0x{cur:08X}, not 0 — abort (would corrupt a live entry)", count - 1);
    // also ensure the handle isn't already present anywhere in the key array
    let mut p = 24usize;
    while p + 4 <= body.len() {
        let w = u32::from_le_bytes([body[p], body[p + 1], body[p + 2], body[p + 3]]);
        if !((w & 0xFFFF_0000) == 0x8000_0000 || (w & 0xFFFF_0000) == 0x9000_0000 || w == 0) { break; }
        assert_ne!(w, new_handle, "handle already in guidmap");
        p += 4;
    }
    body[slot_off..slot_off + 4].copy_from_slice(&new_handle.to_le_bytes());
    (u.build(), count, slot_off)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: landingcraft_mint <merged_overlay.wad> <out.wad> [vz.wad]");
        std::process::exit(1);
    }
    let merged_path = &args[0];
    let out_path = &args[1];
    let vz_path = args.get(2).cloned();

    let merged_bytes = std::fs::read(merged_path).unwrap_or_else(|e| panic!("read {merged_path}: {e}"));
    let merged = read_patch_wad(&merged_bytes).expect("parse merged overlay");
    println!("=== input overlay {} ({} blocks) sha {} ===", merged_path, merged.blocks.len(), sha16(&merged_bytes));
    for b in &merged.blocks {
        println!("  block: {} ({} ASET rows)", b.path_string, b.aset_entries.len());
    }

    // Locate the resident block (block 3185 override) inside the overlay.
    let res_idx = merged
        .blocks
        .iter()
        .position(|b| b.path_string.to_lowercase().replace('/', "\\").ends_with("\\resident_p000_q3.block"))
        .expect("overlay has no resident_P000_Q3 block");
    let res_block = &merged.blocks[res_idx];
    let res_dec = decompress_sges(&res_block.compressed_data).expect("decompress resident block");
    println!("\nresident block: {} decompressed bytes, {} ASET rows", res_dec.len(), res_block.aset_entries.len());

    let mut resident = ResidentBlock::parse(&res_dec);
    // byte-identical resident round-trip gate (structure integrity)
    assert_eq!(resident.build(), res_dec, "resident block re-emit not byte-identical");
    println!("GATE resident-block round-trip: byte-identical OK");

    let we_idx = resident.find(WORLDENTITY_TYPE).expect("no worldentity in resident block");
    let we = resident.entries[we_idx].3.clone();
    let gm = resident
        .find(GUIDMAP_TYPE)
        .map(|i| resident.entries[i].3.clone())
        .expect("no guidmap in resident block");
    println!("worldentity: {} bytes (sha {}); guidmap: {} bytes (sha {})", we.len(), sha16(&we), gm.len(), sha16(&gm));

    // Optional: verify overlay worldentity == base vz.wad worldentity (live overlay didn't alter it).
    if let Some(vz) = &vz_path {
        if let Ok(mut f) = File::open(vz) {
            let size = f.metadata().unwrap().len();
            if let Ok(arch) = load_ffcs_archive(&mut f, size) {
                let mut base_we: Option<Vec<u8>> = None;
                for bi in 0..arch.indx.len() {
                    let Ok(dec) = decompress_block(&mut f, &arch.indx, bi as u16) else { continue };
                    if dec.len() < 4 { continue; }
                    let cnt = read_u32_le(&dec, 0) as usize;
                    if cnt == 0 || cnt > 100_000 { continue; }
                    let mut pos = 4 + cnt * 16;
                    let mut found = None;
                    for ei in 0..cnt {
                        let base = 4 + ei * 16;
                        if base + 16 > dec.len() { break; }
                        let th = read_u32_le(&dec, base + 4);
                        let sz = read_u32_le(&dec, base + 12) as usize;
                        if th == WORLDENTITY_TYPE { found = Some(dec[pos..pos + sz].to_vec()); }
                        pos += sz;
                    }
                    if let Some(w) = found { base_we = Some(w); break; }
                }
                match base_we {
                    Some(bw) => println!(
                        "base vz.wad worldentity sha {} — overlay {} base ({} bytes)",
                        sha16(&bw),
                        if bw == we { "== (unchanged by live overlay)" } else { "!= DIFFERS from" },
                        bw.len()
                    ),
                    None => println!("base vz.wad worldentity: not found (skipped compare)"),
                }
            }
        }
    }

    // --- Confirm the box template is the minimal single-node, zero-ref clone source ---
    let mut we2 = Ucfx::parse(&we);
    assert_eq!(we2.build(), we, "worldentity UCFX round-trip not byte-identical");
    println!("GATE worldentity UCFX round-trip: byte-identical OK");

    // Choose a fresh handle: max 0x8000xxxx bucket key + 1, verified absent everywhere.
    let mut max_h = 0u32;
    let mut all_handles: std::collections::HashSet<u32> = std::collections::HashSet::new();
    // Enumerate COMP classes.
    let mut classes: Vec<String> = Vec::new();
    {
        let mut i = 0;
        while i < we2.descs.len() {
            if &we2.descs[i].0 == b"COMP" && we2.descs[i].1 == 0xFFFF_FFFF {
                let mut j = i + 1;
                while j < we2.descs.len() && we2.descs[j].1 != 0xFFFF_FFFF {
                    if &we2.descs[j].0 == b"info" {
                        if let Some(n) = we2.bodies[j].as_ref().and_then(|b| parse_info_name(b)) {
                            classes.push(n);
                        }
                    }
                    j += 1;
                }
                i = j;
            } else {
                i += 1;
            }
        }
    }
    for cname in &classes {
        if cname == "Name" { continue; }
        let Some((schm_idx, data_idx)) = we2.comp_group(cname) else { continue };
        let Some(si) = schm_idx else { continue };
        let Some(schm_body) = we2.bodies[si].clone() else { continue };
        let Some(schema) = ComponentSchema::from_schm_body(&schm_body, false) else { continue };
        if schema.is_variable_length() { continue; }
        let Some(buckets) = parse_buckets(we2.bodies[data_idx].as_ref().unwrap(), schema.payload_stride as usize) else { continue };
        for b in &buckets {
            for &k in &b.keys {
                if (k & 0xFFFF_0000) == 0x8000_0000 {
                    max_h = max_h.max(k);
                    all_handles.insert(k);
                }
            }
        }
    }
    for h in guidmap_handles(&gm) {
        all_handles.insert(h);
        if (h & 0xFFFF_0000) == 0x8000_0000 { max_h = max_h.max(h); }
    }
    let mut new_handle = max_h + 1;
    while all_handles.contains(&new_handle) { new_handle += 1; }
    let name_hash = pandemic_hash_m2(NEW_NAME);
    println!(
        "\nfresh handle 0x{new_handle:08X} (max 0x8000xxxx in use 0x{max_h:08X}); \
         pandemic_hash_m2(\"{NEW_NAME}\") = 0x{name_hash:08X}"
    );

    // --- SURGERY ---
    println!("\n--- SURGERY: cloning box 0x{BOX_HANDLE:08X} -> 0x{new_handle:08X} ---");
    // ModelName: OWN bucket [1][NEW][LC_MODEL].
    {
        let (schm_idx, data_idx) = we2.comp_group("ModelName").expect("ModelName COMP");
        let schema = ComponentSchema::from_schm_body(we2.bodies[schm_idx.unwrap()].as_ref().unwrap(), false).unwrap();
        let p = schema.payload_stride as usize;
        assert_eq!(p, 4, "ModelName payload_stride must be 4");
        let mut buckets = parse_buckets(we2.bodies[data_idx].as_ref().unwrap(), p).expect("ModelName buckets");
        assert!(buckets.iter().any(|b| b.keys.contains(&BOX_HANDLE)), "box not in ModelName");
        buckets.push(Bucket { keys: vec![new_handle], payload: LC_MODEL.to_le_bytes().to_vec() });
        we2.bodies[data_idx] = Some(build_buckets(&buckets));
        println!("  ModelName NEW bucket [1][0x{new_handle:08X}] -> 0x{LC_MODEL:08X}");
    }
    // JOIN_COMPS: add NEW to whatever bucket the box is in (shared payload).
    let mut joined = Vec::new();
    for cname in JOIN_COMPS {
        let Some((schm_idx, data_idx)) = we2.comp_group(cname) else {
            panic!("box comp {cname} not present in worldentity — abort");
        };
        let schema = ComponentSchema::from_schm_body(we2.bodies[schm_idx.unwrap()].as_ref().unwrap(), false).unwrap();
        let p = schema.payload_stride as usize;
        let mut buckets = parse_buckets(we2.bodies[data_idx].as_ref().unwrap(), p)
            .unwrap_or_else(|| panic!("{cname} not shared-bucket parseable"));
        let mut added = false;
        for b in buckets.iter_mut() {
            if b.keys.contains(&BOX_HANDLE) && !b.keys.contains(&new_handle) {
                b.keys.push(new_handle);
                added = true;
            }
        }
        assert!(added, "box not found in {cname} bucket");
        we2.bodies[data_idx] = Some(build_buckets(&buckets));
        joined.push(format!("{cname}(P{p})"));
    }
    println!("  joined box buckets: {}", joined.join(", "));
    // Name COMP: append [u32 enabled=1][u32 handle][cstring name\0][u8 pad].
    {
        let (_, name_data_idx) = we2.comp_group("Name").expect("Name COMP");
        let mut nd = we2.bodies[name_data_idx].clone().unwrap();
        nd.extend_from_slice(&1u32.to_le_bytes());
        nd.extend_from_slice(&new_handle.to_le_bytes());
        nd.extend_from_slice(NEW_NAME.as_bytes());
        nd.push(0);
        nd.push(0);
        we2.bodies[name_data_idx] = Some(nd);
        println!("  Name appended \"{NEW_NAME}\" -> 0x{new_handle:08X}");
    }

    let we_new = we2.build();
    println!("\nworldentity {} -> {} bytes (+{})", we.len(), we_new.len(), we_new.len() as i64 - we.len() as i64);

    // --- SELF-TEST + DANGLING-REF AUDIT on the rebuilt worldentity ---
    let check = Ucfx::parse(&we_new);
    let csum_off = we_new.len() - 8;
    let stored = u32::from_le_bytes([we_new[csum_off + 4], we_new[csum_off + 5], we_new[csum_off + 6], we_new[csum_off + 7]]);
    let computed = crc32_mercs2(&we_new[..csum_off]);
    println!("SELF-TEST CSUM {} (stored 0x{stored:08X} computed 0x{computed:08X})", if stored == computed { "OK" } else { "FAIL" });
    assert_eq!(stored, computed, "worldentity CSUM mismatch");

    // Collect NEW's payloads across every comp; audit that no payload references a nonexistent handle.
    let mut new_model_ok = false;
    let mut dangling: Vec<(String, u32)> = Vec::new();
    let mut new_comps: Vec<String> = Vec::new();
    // Build the full set of valid handles AFTER the edit (all bucket keys + NEW).
    let mut valid: std::collections::HashSet<u32> = std::collections::HashSet::new();
    valid.insert(new_handle);
    let mut i = 0;
    let mut cls: Vec<String> = Vec::new();
    while i < check.descs.len() {
        if &check.descs[i].0 == b"COMP" && check.descs[i].1 == 0xFFFF_FFFF {
            let mut j = i + 1;
            while j < check.descs.len() && check.descs[j].1 != 0xFFFF_FFFF {
                if &check.descs[j].0 == b"info" {
                    if let Some(n) = check.bodies[j].as_ref().and_then(|b| parse_info_name(b)) { cls.push(n); }
                }
                j += 1;
            }
            i = j;
        } else { i += 1; }
    }
    for cname in &cls {
        if cname == "Name" { continue; }
        let Some((schm_idx, data_idx)) = check.comp_group(cname) else { continue };
        let Some(si) = schm_idx else { continue };
        let Some(schema) = ComponentSchema::from_schm_body(check.bodies[si].as_ref().unwrap(), false) else { continue };
        if schema.is_variable_length() { continue; }
        let Some(buckets) = parse_buckets(check.bodies[data_idx].as_ref().unwrap(), schema.payload_stride as usize) else { continue };
        for b in &buckets { for &k in &b.keys { if (k & 0xFFFF_0000) == 0x8000_0000 { valid.insert(k); } } }
    }
    for cname in &cls {
        if cname == "Name" { continue; }
        let Some((schm_idx, data_idx)) = check.comp_group(cname) else { continue };
        let Some(si) = schm_idx else { continue };
        let Some(schema) = ComponentSchema::from_schm_body(check.bodies[si].as_ref().unwrap(), false) else { continue };
        if schema.is_variable_length() { continue; }
        let p = schema.payload_stride as usize;
        let Some(buckets) = parse_buckets(check.bodies[data_idx].as_ref().unwrap(), p) else { continue };
        for b in &buckets {
            if !b.keys.contains(&new_handle) { continue; }
            new_comps.push(cname.clone());
            if cname == "ModelName" {
                new_model_ok = b.payload.len() >= 4
                    && u32::from_le_bytes([b.payload[0], b.payload[1], b.payload[2], b.payload[3]]) == LC_MODEL;
            }
            // audit: any 0x8000xxxx handle in this payload must resolve
            for c in b.payload.chunks_exact(4) {
                let w = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                if (w & 0xFFFF_0000) == 0x8000_0000 && !valid.contains(&w) {
                    dangling.push((cname.clone(), w));
                }
            }
        }
    }
    println!("SELF-TEST new handle comps: {:?}", new_comps);
    println!("SELF-TEST ModelName[NEW] -> 0x{LC_MODEL:08X}: {}", if new_model_ok { "OK" } else { "FAIL" });
    assert!(new_model_ok, "ModelName for new handle wrong");
    // Name present?
    let name_ok = {
        let (_, di) = check.comp_group("Name").unwrap();
        let d = check.bodies[di].as_ref().unwrap();
        d.windows(NEW_NAME.len()).any(|w| w == NEW_NAME.as_bytes())
    };
    println!("SELF-TEST Name has \"{NEW_NAME}\": {}", if name_ok { "OK" } else { "FAIL" });
    assert!(name_ok);
    if dangling.is_empty() {
        println!("DANGLING-REF AUDIT: 0 dangling handle refs from new template (box carries none) — CLEAN");
    } else {
        println!("DANGLING-REF AUDIT: {} DANGLING refs -> ABORT: {:?}", dangling.len(), dangling);
        std::process::exit(2);
    }

    // --- Rebuild resident block with worldentity swapped in place ---
    resident.entries[we_idx].3 = we_new;

    // --- GUIDMAP: fill reserved free slot so the registry-build enumerates the new handle ---
    // (registry @0xDF6B88 is built by ENUMERATING guidmap, using the Name COMP as handle->name; the
    //  runtime reverse index is REHASHED at load, so no on-disk bucket table needs authoring.)
    {
        let gm_idx = resident.find(GUIDMAP_TYPE).expect("no guidmap in resident block");
        let gm_old = resident.entries[gm_idx].3.clone();
        let (gm_new, gcount, gslot) = guidmap_fill_free_slot(&gm_old, new_handle);
        assert_eq!(gm_new.len(), gm_old.len(), "guidmap size changed (should be a 4-byte in-place edit)");
        // verify CSUM of rebuilt guidmap
        let co = gm_new.len() - 8;
        let g_stored = u32::from_le_bytes([gm_new[co + 4], gm_new[co + 5], gm_new[co + 6], gm_new[co + 7]]);
        let g_comp = crc32_mercs2(&gm_new[..co]);
        assert_eq!(g_stored, g_comp, "guidmap CSUM mismatch after edit");
        // confirm the handle is now in the key array
        assert!(guidmap_handles(&gm_new).contains(&new_handle), "guidmap key not set");
        resident.entries[gm_idx].3 = gm_new;
        let gm_bytes = gm_old.len();
        let gslot_idx = gcount - 1;
        println!(
            "\nguidmap: filled reserved free slot[{gslot_idx}] @body+0x{gslot:X} <- 0x{new_handle:08X} \
             (count {gcount} unchanged, size {gm_bytes} B unchanged, CSUM OK)"
        );
    }

    let res_new = resident.build();
    let res_comp = compress_sges(&res_new).expect("recompress resident block");
    // preserve tier byte; recompute pages
    let tier = res_block.packed_field & 0xFF00_0000;
    let pages = ((res_new.len() + 0x7FFF) / 0x8000) as u32;
    let mut new_res_pb: PatchBlock = res_block.clone();
    new_res_pb.compressed_data = res_comp;
    new_res_pb.packed_field = tier | pages;
    println!(
        "\nresident block: {} -> {} decompressed bytes; packed_field 0x{:08X} -> 0x{:08X}",
        res_dec.len(), res_new.len(), res_block.packed_field, new_res_pb.packed_field
    );

    // --- Merge (replace resident block in place; model block untouched) ---
    let out = merge_patch_wads(&merged_bytes, vec![new_res_pb], true).expect("merge");
    std::fs::write(out_path, &out).expect("write out");
    println!("\nwrote {out_path} ({} bytes) sha256 {}", out.len(), sha_full(&out));

    // --- Post-build verification: re-read, confirm structure + new template intact ---
    let rb = read_patch_wad(&out).expect("re-parse out");
    let ri = rb.blocks.iter().position(|b| b.path_string.to_lowercase().replace('/', "\\").ends_with("\\resident_p000_q3.block")).expect("resident present");
    let model_present = rb.blocks.iter().any(|b| b.path_string.to_lowercase().contains("592057c4"));
    let rdec = decompress_sges(&rb.blocks[ri].compressed_data).expect("decompress out resident");
    let rres = ResidentBlock::parse(&rdec);
    let rwe = &rres.entries[rres.find(WORLDENTITY_TYPE).unwrap()].3;
    let rcheck = Ucfx::parse(rwe);
    let (_, ndi) = rcheck.comp_group("Name").unwrap();
    let has_name = rcheck.bodies[ndi].as_ref().unwrap().windows(NEW_NAME.len()).any(|w| w == NEW_NAME.as_bytes());
    let rgm = &rres.entries[rres.find(GUIDMAP_TYPE).unwrap()].3;
    let gm_has_handle = guidmap_handles(rgm).contains(&new_handle);
    println!(
        "POST-BUILD: {} blocks; model block 0x592057C4 present: {}; resident re-parses OK; \
         worldentity has \"{NEW_NAME}\": {}; guidmap has 0x{new_handle:08X}: {}",
        rb.blocks.len(), model_present, has_name, gm_has_handle
    );
    assert!(model_present && has_name && gm_has_handle, "post-build verification failed");
    println!("\nDONE. Spawn (live bridge): Pg.Spawn(\"{NEW_NAME}\", x, y, z)  |  or by hash: Pg.Spawn(0x{name_hash:08X}, x, y, z)");
}
