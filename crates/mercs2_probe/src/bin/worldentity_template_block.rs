//! worldentity_template_block — author a MINIMAL, standalone worldentity COMP block that carries only
//! ONE new spawn template's records, for the m2-sdk M2 "post-load replay" (drive the game's own COMP
//! loader `FUN_00654940` over this block after world-load so the engine mints the dense index, inserts
//! the reflection records + real signature, and makes a novel `0x8000xxxx` handle live & Pg.Spawn-able).
//!
//! This is the input side of the M2 mechanism (spawn-registry-runtime-injection). It is DISTINCT from
//! `landingcraft_mint`, which edits the WHOLE 1.82 MB resident worldentity in place for the WAD-override
//! route (inert — double-registration). Feeding that whole container to `FUN_00654940` post-load would
//! re-process all ~58k records. M2 needs a block with ONLY the new handle's records so the additive
//! loader adds exactly one entity (base's records are untouched — confirmed: the loader mints each key's
//! dense index on demand and inserts per-key, it never reconstructs pools).
//!
//! Block layout (mirrors the base worldentity's own chunk framing so the loader accepts it):
//!   CHDR [s16 field0][s16 stride][u32 flags]                          (copied from base)
//!   per donor comp:  COMP · info(classname) · schm(schema) · data([1][new_handle][payload])
//!   Name:            COMP · info("Name")    · data(one Name record: [1][new_handle]["name"\0][pad])
//!   flgs [u32 1][u32 new_handle][32-byte signature]                  (donor's signature, re-keyed)
//! ModelName's payload is repointed to the new model hash; every other comp copies the donor's payload
//! (shared floats/hashes/counts, no handle refs — safe to clone).
//!
//! FourCCs the loader dispatches on (all little-endian in the desc table): COMP=0x504d4f43 data=0x61746164
//! schm=0x6d686373 info=0x6f666e69 CHDR=0x52444843 flgs=0x73676c66 UNIQ=0x51494e55.
//!
//! Usage:
//!   worldentity_template_block <vz.wad> <out.wetb>       author (defaults: donor box 0x80000002)
//!   worldentity_template_block <vz.wad> --inspect        dump base worldentity descriptor inventory
//!   flags: --donor 0xHANDLE  --name NAME  --model 0xHASH  --handle 0xHANDLE(force fresh)

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::schema::ComponentSchema;
use mercs2_formats::sges::decompress_block;
use sha2::{Digest, Sha256};
use std::fs::File;

const WORLDENTITY_TYPE: u32 = 0x5647_C35D;
const BOX_HANDLE: u32 = 0x8000_0002;
const LC_MODEL: u32 = 0x5920_57C4;

fn sha_full(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}

// ───────────────────────── UCFX container (parse/build; from landingcraft_mint) ─────────────────────
struct Ucfx {
    data_area_off: u32,
    w8: u32,
    w12: u32,
    descs: Vec<([u8; 4], u32, u32, u32, u32)>, // tag, row_off, size, w3, w4
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

    /// Build a UCFX from an ordered (tag, body) list — used to author the fresh minimal block. The
    /// on-disk navigation fields w3(+0xc) and w4(+0x10) are DERIVED to match the base worldentity's exact
    /// convention: a COMP descriptor parents the info/schm/data that immediately follow it; everything
    /// else is top-level. w4 = child count of a COMP (the loader's descend-gate — `FUN_00654940` reads
    /// chunk+0x10 to decide whether to process the group), 0 otherwise. w3 = top-level countdown
    /// `T - index` for top-level items, or remaining-children-after for a group child. bodies == None ⇒ a
    /// container marker (row_off sentinel 0xFFFFFFFF). Emits a CSUM trailer (CRC-32 mercs2).
    fn build_from(items: &[([u8; 4], Option<Vec<u8>>)], with_csum: bool) -> Vec<u8> {
        let ndesc = items.len();
        // Classify: a child is an info/schm/data that follows a COMP (until the next top-level tag).
        let is_child: Vec<bool> = (0..ndesc)
            .map(|i| {
                if !matches!(&items[i].0, b"info" | b"schm" | b"data") {
                    return false;
                }
                // walk back over any preceding info/schm/data; the group opener must be a COMP
                let mut j = i;
                while j > 0 && matches!(&items[j - 1].0, b"info" | b"schm" | b"data") {
                    j -= 1;
                }
                j > 0 && &items[j - 1].0 == b"COMP"
            })
            .collect();
        let total_top = (0..ndesc).filter(|&i| !is_child[i]).count() as u32;
        let mut w3 = vec![0u32; ndesc];
        let mut w4 = vec![0u32; ndesc];
        let mut top_idx = 0u32;
        for i in 0..ndesc {
            if is_child[i] {
                // remaining children after this one in the group
                let mut k = i + 1;
                let mut rem = 0u32;
                while k < ndesc && is_child[k] {
                    rem += 1;
                    k += 1;
                }
                w3[i] = rem;
            } else {
                w3[i] = total_top - top_idx;
                top_idx += 1;
                if &items[i].0 == b"COMP" {
                    // child count = consecutive children following
                    let mut k = i + 1;
                    let mut c = 0u32;
                    while k < ndesc && is_child[k] {
                        c += 1;
                        k += 1;
                    }
                    w4[i] = c;
                }
            }
        }
        let data_area_off = (20 + ndesc * 20) as u32;
        let mut body_region: Vec<u8> = Vec::new();
        let mut row: Vec<u32> = vec![0xFFFF_FFFF; ndesc];
        let mut size: Vec<u32> = vec![0; ndesc];
        for i in 0..ndesc {
            if let Some(b) = &items[i].1 {
                row[i] = body_region.len() as u32;
                size[i] = b.len() as u32;
                body_region.extend_from_slice(b);
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"UCFX");
        out.extend_from_slice(&data_area_off.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // w8
        out.extend_from_slice(&0u32.to_le_bytes()); // w12
        out.extend_from_slice(&(ndesc as u32).to_le_bytes());
        for i in 0..ndesc {
            out.extend_from_slice(&items[i].0);
            out.extend_from_slice(&row[i].to_le_bytes());
            out.extend_from_slice(&size[i].to_le_bytes());
            out.extend_from_slice(&w3[i].to_le_bytes());
            out.extend_from_slice(&w4[i].to_le_bytes());
        }
        out.extend_from_slice(&body_region);
        if with_csum {
            let crc = crc32_mercs2(&out);
            out.extend_from_slice(b"CSUM");
            out.extend_from_slice(&crc.to_le_bytes());
        }
        out
    }

    /// Walk COMP groups: yields (group_start_idx, class_name, info_idx, schm_idx?, data_idx?).
    fn comp_groups(&self) -> Vec<(usize, String, usize, Option<usize>, Option<usize>)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.descs.len() {
            if &self.descs[i].0 == b"COMP" && self.descs[i].1 == 0xFFFF_FFFF {
                let (mut info_i, mut schm_i, mut data_i) = (None, None, None);
                let mut j = i + 1;
                while j < self.descs.len() && self.descs[j].1 != 0xFFFF_FFFF {
                    match &self.descs[j].0 {
                        b"info" => info_i = Some(j),
                        b"schm" => schm_i = Some(j),
                        b"data" => data_i = Some(j),
                        _ => {}
                    }
                    j += 1;
                }
                if let Some(ii) = info_i {
                    if let Some(name) = self.bodies[ii].as_ref().and_then(|b| parse_info_name(b)) {
                        out.push((i, name, ii, schm_i, data_i));
                    }
                }
                i = j;
            } else {
                i += 1;
            }
        }
        out
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
        let n = read_u32_le(data, pos) as usize;
        pos += 4;
        if n == 0 || n > 1_000_000 || pos + n * 4 + p > data.len() {
            return None;
        }
        let mut keys = Vec::with_capacity(n);
        for k in 0..n {
            keys.push(read_u32_le(data, pos + k * 4));
        }
        pos += n * 4;
        out.push(Bucket { keys, payload: data[pos..pos + p].to_vec() });
        pos += p;
    }
    (pos == data.len()).then_some(out)
}
fn build_bucket(key: u32, payload: &[u8]) -> Vec<u8> {
    // A single-key bucket: [u32 1][u32 key][payload].
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&key.to_le_bytes());
    out.extend_from_slice(payload);
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

/// Read the resident block that carries the worldentity, return its worldentity container bytes.
fn load_worldentity(vz_path: &str) -> Vec<u8> {
    let mut f = File::open(vz_path).unwrap_or_else(|e| panic!("open {vz_path}: {e}"));
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).expect("parse vz.wad FFCS");
    for bi in 0..arch.indx.len() {
        let Ok(dec) = decompress_block(&mut f, &arch.indx, bi as u16) else { continue };
        if dec.len() < 4 {
            continue;
        }
        let cnt = read_u32_le(&dec, 0) as usize;
        if cnt == 0 || cnt > 100_000 {
            continue;
        }
        let mut pos = 4 + cnt * 16;
        for ei in 0..cnt {
            let base = 4 + ei * 16;
            if base + 16 > dec.len() {
                break;
            }
            let th = read_u32_le(&dec, base + 4);
            let sz = read_u32_le(&dec, base + 12) as usize;
            if pos + sz > dec.len() {
                break;
            }
            if th == WORLDENTITY_TYPE {
                eprintln!("worldentity found in block index {bi} ({sz} bytes)");
                return dec[pos..pos + sz].to_vec();
            }
            pos += sz;
        }
    }
    panic!("no block carries the worldentity type 0x{WORLDENTITY_TYPE:08X}");
}

/// Find the `flgs` chunk and return the 32-byte signature for `handle`, if present.
/// flgs body = [u32 count][ {u32 handle}{32-byte sig} × count ].
fn donor_signature(we: &Ucfx, handle: u32) -> Option<[u8; 32]> {
    for i in 0..we.descs.len() {
        if &we.descs[i].0 == b"flgs" {
            let body = we.bodies[i].as_ref()?;
            if body.len() < 4 {
                return None;
            }
            let count = read_u32_le(body, 0) as usize;
            let mut pos = 4;
            for _ in 0..count {
                if pos + 4 + 32 > body.len() {
                    break;
                }
                let h = read_u32_le(body, pos);
                if h == handle {
                    let mut sig = [0u8; 32];
                    sig.copy_from_slice(&body[pos + 4..pos + 4 + 32]);
                    return Some(sig);
                }
                pos += 4 + 32;
            }
        }
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: worldentity_template_block <vz.wad> <out.wetb> [--donor 0xH] [--name N] [--model 0xH]");
        eprintln!("       worldentity_template_block <vz.wad> --inspect");
        std::process::exit(1);
    }
    let vz_path = args[0].clone();
    let inspect = args.iter().any(|a| a == "--inspect");
    let mut out_path: Option<String> = None;
    let mut donor = BOX_HANDLE;
    let mut name = "landing_craft".to_string();
    let mut model = LC_MODEL;
    let mut forced_handle: Option<u32> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--inspect" => {}
            "--donor" => { donor = parse_hex(&args[i + 1]); i += 1; }
            "--name" => { name = args[i + 1].clone(); i += 1; }
            "--model" => { model = parse_hex(&args[i + 1]); i += 1; }
            "--handle" => { forced_handle = Some(parse_hex(&args[i + 1])); i += 1; }
            s if !s.starts_with("--") => out_path = Some(s.to_string()),
            other => { eprintln!("unknown flag {other}"); std::process::exit(1); }
        }
        i += 1;
    }

    let we_bytes = load_worldentity(&vz_path);
    let we = Ucfx::parse(&we_bytes);
    let groups = we.comp_groups();

    // Which comps does the donor participate in? (fixed-stride, bucket-parseable comps only.)
    let mut donor_comps: Vec<(String, usize, Option<usize>, u32, Vec<u8>)> = Vec::new(); // name, data_idx, schm_idx, stride, donor_payload
    let mut has_chdr = false;
    let mut chdr_body: Option<Vec<u8>> = None;
    for i in 0..we.descs.len() {
        if &we.descs[i].0 == b"CHDR" {
            has_chdr = true;
            chdr_body = we.bodies[i].clone();
        }
    }
    for (_, cname, _info_i, schm_i, data_i) in &groups {
        if cname == "Name" {
            continue;
        }
        let (Some(si), Some(di)) = (schm_i, data_i) else { continue };
        let Some(schema) = ComponentSchema::from_schm_body(we.bodies[*si].as_ref().unwrap(), false) else { continue };
        if schema.is_variable_length() {
            continue;
        }
        let p = schema.payload_stride as usize;
        let Some(buckets) = parse_buckets(we.bodies[*di].as_ref().unwrap(), p) else { continue };
        if let Some(b) = buckets.iter().find(|b| b.keys.contains(&donor)) {
            donor_comps.push((cname.clone(), *di, *schm_i, p as u32, b.payload.clone()));
        }
    }
    let donor_sig = donor_signature(&we, donor);

    if inspect {
        println!("=== base worldentity: {} bytes, {} descriptors, {} COMP groups ===", we_bytes.len(), we.descs.len(), groups.len());
        println!("data_area_off={} w8={} w12={} trailer={}B", we.data_area_off, we.w8, we.w12, we.trailer.len());
        println!("CHDR present: {} ({} bytes)", has_chdr, chdr_body.as_ref().map(|b| b.len()).unwrap_or(0));
        let flgs = we.descs.iter().any(|d| &d.0 == b"flgs");
        let uniq = we.descs.iter().any(|d| &d.0 == b"UNIQ");
        println!("flgs present: {flgs}   UNIQ present: {uniq}");
        println!("donor 0x{donor:08X} signature: {}", donor_sig.map(|s| s.iter().map(|x| format!("{x:02x}")).collect::<String>()).unwrap_or_else(|| "NOT FOUND".into()));
        println!("\ndescriptor order (first 40):");
        for (k, d) in we.descs.iter().take(40).enumerate() {
            let tag = String::from_utf8_lossy(&d.0);
            let ro = if d.1 == 0xFFFF_FFFF { "----".into() } else { format!("{}", d.1) };
            println!("  [{k:3}] {tag:5} row={ro:>8} size={:>6}  w3=0x{:08X} w4=0x{:08X}", d.2, d.3, d.4);
        }
        println!("\ndonor 0x{donor:08X} participates in {} comps:", donor_comps.len());
        for (c, _, si, p, pay) in &donor_comps {
            println!("  {c:24} stride={p:<3} schm={} payload={}B", si.is_some(), pay.len());
        }
        return;
    }

    let out_path = out_path.expect("need an output path (or --inspect)");

    // Fresh handle: max 0x8000xxxx bucket key + 1, verified absent everywhere (or --handle forced).
    let new_handle = forced_handle.unwrap_or_else(|| pick_fresh_handle(&we, &groups));
    let name_hash = pandemic_hash_m2(&name);
    println!("=== authoring template block ===");
    println!("donor 0x{donor:08X}  ->  new handle 0x{new_handle:08X}  name \"{name}\" (hash 0x{name_hash:08X})  model 0x{model:08X}");
    assert!(!donor_comps.is_empty(), "donor participates in no bucket-parseable comps — abort");
    let sig = donor_sig.expect("donor signature not found in base flgs — cannot author a real (non-hollow) signature");

    // Assemble the minimal descriptor list as an ordered (tag, body) list; build_from derives the
    // nav fields (w3/w4) to match base exactly.
    let mut items: Vec<([u8; 4], Option<Vec<u8>>)> = Vec::new();

    // CHDR (copied) — sets stride/flags the loader latches before comps.
    if let Some(cb) = &chdr_body {
        items.push((*b"CHDR", Some(cb.clone())));
    }
    // UNIQ [u32 1][u32 new_handle] — the instantiation key-list (base carries the full 6126-handle list
    // here; mirror the shape with just our one handle so the loader's instantiate pass runs additively
    // for it). The loader alloc is per-call-local (local_10c), so a 1-entry list resizes nothing global.
    {
        let mut ub = Vec::new();
        ub.extend_from_slice(&1u32.to_le_bytes());
        ub.extend_from_slice(&new_handle.to_le_bytes());
        items.push((*b"UNIQ", Some(ub)));
    }
    // Per donor comp: COMP · info · schm · data([1][new_handle][payload]).
    let mut authored: Vec<String> = Vec::new();
    for (cname, _data_i, schm_i, _stride, donor_payload) in &donor_comps {
        let info_body = groups
            .iter()
            .find(|g| &g.1 == cname)
            .and_then(|g| we.bodies[g.2].clone())
            .expect("comp info body");
        let payload = if cname == "ModelName" {
            model.to_le_bytes().to_vec()
        } else {
            donor_payload.clone()
        };
        let data = build_bucket(new_handle, &payload);
        items.push((*b"COMP", None));
        items.push((*b"info", Some(info_body)));
        if let Some(si) = schm_i {
            items.push((*b"schm", we.bodies[*si].clone()));
        }
        items.push((*b"data", Some(data)));
        authored.push(format!("{cname}({}B)", payload.len()));
    }
    // Name COMP: COMP · info("Name") · data(one record [u32 1][u32 handle][name\0][pad]).
    if let Some(g) = groups.iter().find(|g| g.1 == "Name") {
        let info_body = we.bodies[g.2].clone().expect("Name info");
        let mut nd = Vec::new();
        nd.extend_from_slice(&1u32.to_le_bytes());
        nd.extend_from_slice(&new_handle.to_le_bytes());
        nd.extend_from_slice(name.as_bytes());
        nd.push(0);
        nd.push(0);
        items.push((*b"COMP", None));
        items.push((*b"info", Some(info_body)));
        items.push((*b"data", Some(nd)));
        authored.push("Name".into());
    }
    // flgs: [u32 1][u32 new_handle][32-byte donor signature].
    {
        let mut fb = Vec::new();
        fb.extend_from_slice(&1u32.to_le_bytes());
        fb.extend_from_slice(&new_handle.to_le_bytes());
        fb.extend_from_slice(&sig);
        items.push((*b"flgs", Some(fb)));
    }

    let block = Ucfx::build_from(&items, true);
    println!("authored comps: {}", authored.join(", "));
    println!("block: {} descriptors, {} bytes", items.len(), block.len());

    // ── Structural gate: re-parse and confirm every authored record round-trips ───────────────────
    let chk = Ucfx::parse(&block);
    println!("\nauthored descriptor nav fields (tag row size w3 w4):");
    for (k, d) in chk.descs.iter().enumerate() {
        let ro = if d.1 == 0xFFFF_FFFF { "----".into() } else { format!("{}", d.1) };
        println!("  [{k:2}] {:5} row={ro:>6} size={:>4}  w3={} w4={}", String::from_utf8_lossy(&d.0), d.2, d.3, d.4);
    }
    let mut ok = true;
    for (cname, _, _, _stride, _) in &donor_comps {
        let g = chk.comp_groups().into_iter().find(|g| &g.1 == cname);
        let present = g
            .and_then(|g| g.4)
            .and_then(|di| chk.bodies[di].clone())
            .map(|d| d.len() >= 8 && read_u32_le(&d, 4) == new_handle)
            .unwrap_or(false);
        if !present {
            eprintln!("GATE FAIL: {cname} data missing new handle");
            ok = false;
        }
    }
    let sig_ok = donor_signature(&chk, new_handle) == Some(sig);
    println!("GATE re-parse: comps={} sig={}", if ok { "OK" } else { "FAIL" }, if sig_ok { "OK" } else { "FAIL" });
    // CSUM self-check
    let co = block.len() - 8;
    let stored = read_u32_le(&block, co + 4);
    let computed = crc32_mercs2(&block[..co]);
    println!("GATE CSUM: stored 0x{stored:08X} computed 0x{computed:08X} {}", if stored == computed { "OK" } else { "FAIL" });
    assert!(ok && sig_ok && stored == computed, "structural gate failed");

    std::fs::write(&out_path, &block).unwrap_or_else(|e| panic!("write {out_path}: {e}"));
    println!("\nwrote {out_path} ({} bytes) sha256 {}", block.len(), sha_full(&block));
    println!("M2: the SDK builds a 68-byte load context over this block + calls FUN_00654940(ctx,0).");
    println!("    then Pg.Spawn(\"{name}\") | Pg.Spawn(0x{name_hash:08X}) resolves handle 0x{new_handle:08X}.");
}

fn parse_hex(s: &str) -> u32 {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(s, 16).unwrap_or_else(|_| panic!("bad hex: {s}"))
}

fn pick_fresh_handle(we: &Ucfx, groups: &[(usize, String, usize, Option<usize>, Option<usize>)]) -> u32 {
    let mut max_h = 0u32;
    let mut used = std::collections::HashSet::new();
    for (_, cname, _, schm_i, data_i) in groups {
        if cname == "Name" {
            continue;
        }
        let (Some(si), Some(di)) = (schm_i, data_i) else { continue };
        let Some(schema) = ComponentSchema::from_schm_body(we.bodies[*si].as_ref().unwrap(), false) else { continue };
        if schema.is_variable_length() {
            continue;
        }
        let Some(buckets) = parse_buckets(we.bodies[*di].as_ref().unwrap(), schema.payload_stride as usize) else { continue };
        for b in &buckets {
            for &k in &b.keys {
                if (k & 0xFFFF_0000) == 0x8000_0000 {
                    max_h = max_h.max(k);
                    used.insert(k);
                }
            }
        }
    }
    let mut h = max_h + 1;
    while used.contains(&h) {
        h += 1;
    }
    h
}
