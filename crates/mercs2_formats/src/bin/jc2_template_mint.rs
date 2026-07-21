//! jc2_template_mint — mint a NOVEL vehicle spawn template into the worldentity resident singleton
//! (block 3185) by appending one cloned entity (source 0x80009FC3 "CRX (racing) (Driver)") under a
//! fresh handle, with ModelName repointed to our JC2 model 0xB89B2F9A, plus a Name registry record
//! and a guidmap handle append. The loader re-hashes the COMP hashmaps on load, so appending records
//! at the end of each COMP `data` chunk is order-independent (see FUN_00654940).
//!
//! Stage gate: a byte-identical UCFX round-trip self-test runs first — if rebuilding the worldentity
//! container with NO changes does not reproduce the original bytes, we abort before any surgery.
//!
//! Usage: jc2_template_mint [vz.wad path]

use std::fs::File;

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;

const WORLDENTITY_TYPE: u32 = 0x5647_C35D;
const GUIDMAP_TYPE: u32 = 0x140E_8728;
const SRC_HANDLE: u32 = 0x8000_9FC3;
const NEW_HANDLE: u32 = 0x8000_B3C5;
const JC2_MODEL: u32 = 0xB89B_2F9A;
const NEW_NAME: &str = "JC2 Sportscar";
const SCRIPT_TYPE: u32 = 0x4249_8680; // resident-script type_hash
const SCRIPT_NAME: &str = "mrxjc2sportscar";
const DEPS_HOST: &str = "mrxshop"; // non-cyclic parent whose DEPS pulls our chunk (heli recipe)

/// Build a resident-script container: UCFX(data_area_off=80) + INFO/DEPS/BINN + CSUM.
/// INFO = [0x05][u16 name_len][name][0x00][0x00 0x00 0x00]; DEPS = [u8 count][count u32]; BINN = LuaQ.
fn build_script_chunk(name: &str, deps: &[u32], luaq: &[u8]) -> Vec<u8> {
    let mut info = vec![0x05u8];
    info.extend_from_slice(&(name.len() as u16).to_le_bytes());
    info.extend_from_slice(name.as_bytes());
    info.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // terminator + 3-byte metadata (0 = no reflection entries)
    let mut deps_body = vec![deps.len() as u8];
    for &d in deps { deps_body.extend_from_slice(&d.to_le_bytes()); }
    let binn = luaq.to_vec();
    let u = Ucfx {
        data_area_off: 80,
        w8: 0,
        w12: 0,
        descs: vec![
            (*b"INFO", 0, info.len() as u32, 2, 0),
            (*b"DEPS", info.len() as u32, deps_body.len() as u32, 1, 0),
            (*b"BINN", (info.len() + deps_body.len()) as u32, binn.len() as u32, 0, 0),
        ],
        bodies: vec![Some(info), Some(deps_body), Some(binn)],
        trailer: b"CSUM\0\0\0\0".to_vec(),
    };
    u.build()
}

/// Replace a script container's BINN body (the LuaQ bytecode) in place, rebuilding UCFX + CSUM.
/// INFO/DEPS are untouched — an in-place bytecode swap (no new asset, no DEPS wiring), which loads
/// (heli experiment V2) unlike an appended chunk (deadlocks the resident-block load at phase 8).
fn replace_binn(container: &[u8], new_luaq: &[u8]) -> Vec<u8> {
    let mut u = Ucfx::parse(container);
    for i in 0..u.descs.len() {
        if &u.descs[i].0 == b"BINN" {
            u.bodies[i] = Some(new_luaq.to_vec());
            break;
        }
    }
    u.build()
}

/// Append `new_hash` to a script container's DEPS list (count++), rebuilding the UCFX + CSUM.
#[allow(dead_code)]
fn add_dep(container: &[u8], new_hash: u32) -> Vec<u8> {
    let mut u = Ucfx::parse(container);
    for i in 0..u.descs.len() {
        if &u.descs[i].0 == b"DEPS" {
            let body = u.bodies[i].as_ref().unwrap();
            let count = body[0] as usize;
            let mut nb = vec![(count + 1) as u8];
            nb.extend_from_slice(&body[1..1 + count * 4]);
            nb.extend_from_slice(&new_hash.to_le_bytes());
            u.bodies[i] = Some(nb);
            break;
        }
    }
    u.build()
}

/// A UCFX container decomposed into its header words + ordered descriptor rows + bodies.
struct Ucfx {
    data_area_off: u32,
    w8: u32,
    w12: u32,
    /// descriptor rows: (tag, row_u0, size, w3, w4). row_u0 == 0xFFFFFFFF => sentinel (no body).
    descs: Vec<([u8; 4], u32, u32, u32, u32)>,
    /// body bytes per descriptor index (None for sentinels).
    bodies: Vec<Option<Vec<u8>>>,
    /// trailing bytes after the body region (e.g. CSUM trailer), verbatim.
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
            descs.push((tag, read_u32_le(c, ro + 4), read_u32_le(c, ro + 8), read_u32_le(c, ro + 12), read_u32_le(c, ro + 16)));
        }
        let data_start = data_area_off as usize;
        // Read bodies; track the max body end to find the trailer.
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

    /// Re-emit the container. Bodies are laid out in ascending original-row_u0 order (file order),
    /// so an unchanged parse->build reproduces the original bytes exactly.
    fn build(&self) -> Vec<u8> {
        let ndesc = self.descs.len();
        let data_start = 20 + ndesc * 20;
        assert_eq!(data_start, self.data_area_off as usize, "data_area_off must equal header+desc table");
        // Order descriptor indices by original row_u0 (sentinels excluded).
        let mut order: Vec<usize> = (0..ndesc).filter(|&i| self.descs[i].1 != 0xFFFF_FFFF).collect();
        order.sort_by_key(|&i| self.descs[i].1);
        // Lay bodies, recompute row_u0.
        let mut body_region: Vec<u8> = Vec::new();
        let mut new_row: Vec<u32> = vec![0xFFFF_FFFF; ndesc];
        let mut new_size: Vec<u32> = self.descs.iter().map(|d| d.2).collect();
        for &i in &order {
            let b = self.bodies[i].as_ref().unwrap();
            new_row[i] = body_region.len() as u32;
            new_size[i] = b.len() as u32;
            body_region.extend_from_slice(b);
        }
        // Emit header + descriptor table + bodies + trailer.
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
        // Rebuild trailer: if it is a CSUM trailer, recompute over the body-so-far.
        if self.trailer.len() >= 8 && &self.trailer[0..4] == b"CSUM" {
            let crc = crc32_mercs2(&out); // CRC over everything BEFORE the CSUM magic
            out.extend_from_slice(b"CSUM");
            out.extend_from_slice(&crc.to_le_bytes());
            // any bytes beyond the 8-byte CSUM record (padding) copied verbatim
            out.extend_from_slice(&self.trailer[8..]);
        } else {
            out.extend_from_slice(&self.trailer);
        }
        out
    }

    /// Find (schm_idx, data_idx) of a COMP group by class name (from its `info` body).
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
                    if self.bodies[ii].as_ref().and_then(|b| parse_info_name(b)).as_deref() == Some(class_name) {
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

    fn comp_data_index(&self, class_name: &str) -> Option<usize> {
        self.comp_group(class_name).map(|(_, d)| d)
    }
}

/// A worldentity COMP `data` chunk = `[prefix][fixed [u32 key][payload] records]`. Detect the prefix
/// size h (smallest making the record region a whole multiple of stride, with a plausible first key).
/// One shared bucket in a worldentity COMP `data` chunk: `[u32 N][N × u32 key][payload P bytes]`.
/// The N entity-keys SHARE the single payload (entities on the same skeleton share bone-based config).
struct Bucket { keys: Vec<u32>, payload: Vec<u8> }

/// Parse a COMP `data` chunk into shared buckets. `p` = schm payload_stride. Returns None if the
/// chunk does not consume exactly (wrong P / not this format).
fn parse_buckets(data: &[u8], p: usize) -> Option<Vec<Bucket>> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos + 4 <= data.len() {
        let n = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        pos += 4;
        if n == 0 || n > 1_000_000 { return None; }
        if pos + n * 4 + p > data.len() { return None; }
        let mut keys = Vec::with_capacity(n);
        for k in 0..n {
            keys.push(u32::from_le_bytes([data[pos+k*4], data[pos+k*4+1], data[pos+k*4+2], data[pos+k*4+3]]));
        }
        pos += n * 4;
        let payload = data[pos..pos + p].to_vec();
        pos += p;
        out.push(Bucket { keys, payload });
    }
    (pos == data.len()).then_some(out)
}

fn build_buckets(buckets: &[Bucket]) -> Vec<u8> {
    let mut out = Vec::new();
    for b in buckets {
        out.extend_from_slice(&(b.keys.len() as u32).to_le_bytes());
        for &k in &b.keys { out.extend_from_slice(&k.to_le_bytes()); }
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

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad".into()
    });
    let mut f = File::open(&path).unwrap_or_else(|_| panic!("open {path}"));
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).expect("ffcs");

    // Locate block 3185 (contains worldentity).
    let mut blk: Option<Vec<u8>> = None;
    for bi in 0..arch.indx.len() {
        let Ok(dec) = decompress_block(&mut f, &arch.indx, bi as u16) else { continue };
        if dec.len() < 4 { continue; }
        let count = read_u32_le(&dec, 0) as usize;
        if count == 0 || count > 100_000 { continue; }
        let mut has = false;
        for ei in 0..count {
            let base = 4 + ei * 16;
            if base + 16 > dec.len() { break; }
            if read_u32_le(&dec, base + 4) == WORLDENTITY_TYPE { has = true; break; }
        }
        if has { eprintln!("block {bi} has worldentity"); blk = Some(dec); break; }
    }
    let blk = blk.expect("no worldentity block");

    // Extract worldentity + guidmap containers with their positions.
    let count = read_u32_le(&blk, 0) as usize;
    let mut pos = 4 + count * 16;
    let mut we: Option<Vec<u8>> = None;
    let mut gm: Option<Vec<u8>> = None;
    let mut we_entry = (0u32, 0u32, 0u32); // (name_hash, type_hash, field_c)
    for ei in 0..count {
        let base = 4 + ei * 16;
        let name_hash = read_u32_le(&blk, base);
        let type_hash = read_u32_le(&blk, base + 4);
        let field_c = read_u32_le(&blk, base + 8);
        let chunk_size = read_u32_le(&blk, base + 12) as usize;
        let container = blk[pos..pos + chunk_size].to_vec();
        pos += chunk_size;
        if type_hash == WORLDENTITY_TYPE { we = Some(container); we_entry = (name_hash, type_hash, field_c); }
        else if type_hash == GUIDMAP_TYPE { gm = Some(container); }
    }
    let we = we.expect("worldentity");
    let _gm = gm.expect("guidmap");

    // Base ASET row + path for worldentity (to mirror into the patch).
    let we_aset = arch.aset.iter().find(|a| a.asset_hash == we_entry.0).cloned();
    eprintln!("worldentity base ASET: {:?}", we_aset.as_ref().map(|a|
        (format!("hash 0x{:08X}", a.asset_hash), format!("sec 0x{:08X}", a.secondary_ref),
         format!("blk {} sub 0x{:04X}", a.block_index(), a.sub_entry()), format!("type 0x{:08X}", a.type_id))));
    let we_path = arch.paths.get(we_aset.as_ref().map(|a| a.block_index() as usize).unwrap_or(usize::MAX)).cloned();
    eprintln!("worldentity block path: {we_path:?}");

    // --- Stage gate: byte-identical round-trip of the worldentity container ---
    let parsed = Ucfx::parse(&we);
    eprintln!("worldentity: {} descriptors, data_area_off={}, w8=0x{:08X}, w12=0x{:08X}, trailer={}B ({})",
        parsed.descs.len(), parsed.data_area_off, parsed.w8, parsed.w12, parsed.trailer.len(),
        if parsed.trailer.len() >= 4 { std::str::from_utf8(&parsed.trailer[0..4]).unwrap_or("????") } else { "-" });
    let rebuilt = parsed.build();
    if rebuilt == we {
        println!("ROUND-TRIP: byte-identical ✓ ({} bytes)", rebuilt.len());
    } else {
        println!("ROUND-TRIP: MISMATCH ✗ (orig {} vs rebuilt {})", we.len(), rebuilt.len());
        // find first differing offset
        let n = we.len().min(rebuilt.len());
        let mut d = None;
        for i in 0..n { if we[i] != rebuilt[i] { d = Some(i); break; } }
        println!("  first diff at {:?}", d);
        if let Some(off) = d {
            let a = off.saturating_sub(8);
            println!("  orig    : {:02X?}", &we[a..(off+16).min(we.len())]);
            println!("  rebuilt : {:02X?}", &rebuilt[a..(off+16).min(rebuilt.len())]);
        }
        std::process::exit(1);
    }

    // --- SURGERY: clone source entity 0x80009FC3 under NEW_HANDLE across every COMP it appears in ---
    // Walk every COMP group, get its schm stride, clone any SRC record. ModelName's payload[0] (the
    // model hash field) is repointed to our JC2 model.
    let mut we2 = Ucfx::parse(&we);
    // Enumerate all COMP class names present.
    let mut classes: Vec<String> = Vec::new();
    {
        let mut i = 0;
        while i < we2.descs.len() {
            if &we2.descs[i].0 == b"COMP" && we2.descs[i].1 == 0xFFFF_FFFF {
                let mut j = i + 1;
                while j < we2.descs.len() && we2.descs[j].1 != 0xFFFF_FFFF {
                    if &we2.descs[j].0 == b"info" {
                        if let Some(n) = we2.bodies[j].as_ref().and_then(|b| parse_info_name(b)) { classes.push(n); }
                    }
                    j += 1;
                }
                i = j;
            } else { i += 1; }
        }
    }
    println!("\n--- SURGERY: adding 0x{NEW_HANDLE:08X} alongside 0x{SRC_HANDLE:08X} (shared-bucket) ---");
    // Universal format: [u32 N][N keys][shared payload]. Our conformant model keeps the CRX bones,
    // so for every comp where SRC participates we JOIN its bucket (share the bone-based config).
    // ModelName is the sole exception: our entity gets its OWN bucket pointing at our model.
    let mut joined = 0;
    let mut own_model = false;
    for cname in &classes {
        if cname == "Name" { continue; }
        let Some((schm_idx, data_idx)) = we2.comp_group(cname) else { continue };
        let Some(si) = schm_idx else { continue };
        let Some(schm_body) = we2.bodies[si].clone() else { continue };
        let Some(schema) = mercs2_formats::schema::ComponentSchema::from_schm_body(&schm_body, false) else { continue };
        if schema.is_variable_length() { continue; }
        let p = schema.payload_stride as usize;
        let data = we2.bodies[data_idx].clone().unwrap();
        let Some(mut buckets) = parse_buckets(&data, p) else { continue };

        if cname == "ModelName" {
            // Own bucket: 1 key (our entity) -> our model hash (4-byte payload).
            buckets.push(Bucket { keys: vec![NEW_HANDLE], payload: JC2_MODEL.to_le_bytes().to_vec() });
            we2.bodies[data_idx] = Some(build_buckets(&buckets));
            own_model = true;
            println!("  {:>22} NEW bucket [1][0x{NEW_HANDLE:08X}] -> 0x{JC2_MODEL:08X}", cname);
            continue;
        }
        // Join every bucket that contains SRC (dedup-guarded).
        let mut added = 0;
        for b in buckets.iter_mut() {
            if b.keys.contains(&SRC_HANDLE) && !b.keys.contains(&NEW_HANDLE) {
                b.keys.push(NEW_HANDLE);
                added += 1;
            }
        }
        if added > 0 {
            we2.bodies[data_idx] = Some(build_buckets(&buckets));
            joined += 1;
            println!("  {:>22} joined SRC bucket (payload {p}B, +{added} bucket(s))", cname);
        }
    }
    println!("  joined {joined} shared-config comps; own ModelName bucket: {own_model}");

    // --- Name COMP: append [u32 enabled=1][u32 handle][cstring name\0][u8 pad] ---
    if let Some((_, name_data_idx)) = we2.comp_group("Name") {
        let mut nd = we2.bodies[name_data_idx].clone().unwrap();
        nd.extend_from_slice(&1u32.to_le_bytes());
        nd.extend_from_slice(&NEW_HANDLE.to_le_bytes());
        nd.extend_from_slice(NEW_NAME.as_bytes());
        nd.push(0); // string terminator
        nd.push(0); // 1-byte pad (matches shipped framing)
        we2.bodies[name_data_idx] = Some(nd);
        println!("  Name: appended \"{NEW_NAME}\" -> 0x{NEW_HANDLE:08X}");
    }

    // Rebuild the modified worldentity container.
    let mut we_new = we2.build();
    // Diagnostic: JC2_SKIP_WE=1 ships the UNMODIFIED worldentity (isolates worldentity-edit vs
    // mrxrewarddata-edit as the phase-10 hang cause).
    if std::env::var("JC2_SKIP_WE").is_ok() {
        we_new = we.clone();
        println!("\n[JC2_SKIP_WE] worldentity UNMODIFIED (isolation build)");
    }
    println!("\nworldentity: {} -> {} bytes (+{})", we.len(), we_new.len(), we_new.len() as i64 - we.len() as i64);

    // --- SELF-TEST: re-parse the rebuilt container and confirm the new entity is present ---
    let check = Ucfx::parse(&we_new);
    // CSUM valid?
    let csum_off = we_new.len() - 8;
    let stored = u32::from_le_bytes([we_new[csum_off+4], we_new[csum_off+5], we_new[csum_off+6], we_new[csum_off+7]]);
    let computed = crc32_mercs2(&we_new[..csum_off]);
    println!("SELF-TEST CSUM: stored 0x{stored:08X} computed 0x{computed:08X} {}",
        if stored == computed { "✓" } else { "✗" });
    // ModelName bucket for NEW handle present with our model?
    let mut ok_model = false;
    if let Some((si, di)) = check.comp_group("ModelName") {
        let schema = mercs2_formats::schema::ComponentSchema::from_schm_body(check.bodies[si.unwrap()].as_ref().unwrap(), false).unwrap();
        let p = schema.payload_stride as usize;
        if let Some(buckets) = parse_buckets(check.bodies[di].as_ref().unwrap(), p) {
            for b in &buckets {
                if b.keys.contains(&NEW_HANDLE) {
                    ok_model = u32::from_le_bytes([b.payload[0], b.payload[1], b.payload[2], b.payload[3]]) == JC2_MODEL;
                    break;
                }
            }
        }
    }
    println!("SELF-TEST ModelName[0x{NEW_HANDLE:08X}] -> 0x{JC2_MODEL:08X}: {}", if ok_model { "✓" } else { "✗" });
    // Verify NEW joined a representative vehicle comp (VehiclePart) alongside SRC.
    let mut ok_vp = false;
    if let Some((si, di)) = check.comp_group("VehiclePart") {
        let schema = mercs2_formats::schema::ComponentSchema::from_schm_body(check.bodies[si.unwrap()].as_ref().unwrap(), false).unwrap();
        if let Some(buckets) = parse_buckets(check.bodies[di].as_ref().unwrap(), schema.payload_stride as usize) {
            ok_vp = buckets.iter().any(|b| b.keys.contains(&NEW_HANDLE) && b.keys.contains(&SRC_HANDLE));
        }
    }
    println!("SELF-TEST VehiclePart: NEW shares SRC bucket: {}", if ok_vp { "✓" } else { "✗" });
    // Name present?
    let mut ok_name = false;
    if let Some((_, di)) = check.comp_group("Name") {
        let d = check.bodies[di].as_ref().unwrap();
        let needle = NEW_NAME.as_bytes();
        ok_name = d.windows(needle.len()).any(|w| w == needle);
    }
    println!("SELF-TEST Name has \"{NEW_NAME}\": {}", if ok_name { "✓" } else { "✗" });

    std::fs::create_dir_all("../../output").ok();

    // --- Rebuild the FULL block 3185 with worldentity replaced in place (all other entries intact) ---
    // Resident scripts + worldentity require full-block replacement (asset_injection_playbook §5); the
    // smuggler's `--inject-block` then overlays this and carries block 3185's existing ASET + path.
    let (we_name, we_type, _we_fieldc) = we_entry;
    let mrxrewarddata_hash = pandemic_hash_m2("mrxrewarddata");
    let mrxsupportdata_hash = pandemic_hash_m2("mrxsupportdata");

    // In-place store injection (in the on-shop-path resident scripts, faction "Pmc" = Eva's shop):
    //  - mrxsupportdata.Init gets the tSupportData catalog entry (delivery + tUnlockStatus.Pmc).
    //  - mrxrewarddata gets a _tRewards reward row {jc2sportscar,"Pmc"} so GetAllPotentialShopItems("Pmc")
    //    surfaces it. Both are full-block-replaced (per-hash overlay does NOT load); no new asset/DEPS.
    let rw_luaq = std::fs::read("../../output/mrxrewarddata_edited.luaq")
        .expect("compile output/mrxrewarddata_edited.luaq first");
    let sd_luaq = std::fs::read("../../output/mrxsupportdata_edited.luaq")
        .expect("compile output/mrxsupportdata_edited.luaq first");

    let count = read_u32_le(&blk, 0) as usize;
    let mut entries: Vec<(u32, u32, u32, Vec<u8>)> = Vec::new();
    let (mut edited_rw, mut edited_sd) = (false, false);
    {
        let mut pos = 4 + count * 16;
        for ei in 0..count {
            let base = 4 + ei * 16;
            let nh = read_u32_le(&blk, base);
            let th = read_u32_le(&blk, base + 4);
            let fc = read_u32_le(&blk, base + 8);
            let sz = read_u32_le(&blk, base + 12) as usize;
            let raw = &blk[pos..pos + sz];
            pos += sz;
            let body = if nh == we_name && th == we_type {
                we_new.clone()
            } else if nh == mrxrewarddata_hash && th == SCRIPT_TYPE {
                edited_rw = true;
                replace_binn(raw, &rw_luaq)
            } else if nh == mrxsupportdata_hash && th == SCRIPT_TYPE {
                edited_sd = true;
                replace_binn(raw, &sd_luaq)
            } else {
                raw.to_vec()
            };
            entries.push((nh, th, fc, body));
        }
    }
    if !edited_rw { eprintln!("WARNING: mrxrewarddata not found"); }
    if !edited_sd { eprintln!("WARNING: mrxsupportdata not found"); }
    println!("\nstore injection: mrxsupportdata BINN {}B + mrxrewarddata BINN {}B (faction Pmc)", sd_luaq.len(), rw_luaq.len());

    let mut full: Vec<u8> = Vec::new();
    full.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (nh, th, fc, b) in &entries {
        full.extend_from_slice(&nh.to_le_bytes());
        full.extend_from_slice(&th.to_le_bytes());
        full.extend_from_slice(&fc.to_le_bytes());
        full.extend_from_slice(&(b.len() as u32).to_le_bytes());
    }
    for (_, _, _, b) in &entries { full.extend_from_slice(b); }

    std::fs::write("../../output/jc2_block3185_minted.bin", &full).unwrap();
    println!("wrote output/jc2_block3185_minted.bin ({} bytes, {} entries: worldentity + edited mrxrewarddata)",
        full.len(), entries.len());
    println!("\nPACKAGE: smuggler --source-wad vz.wad --extra-only \\");
    println!("  --inject-block \"resident_P000_Q3:output/jc2_block3185_minted.bin\" --output output/jc2_sportscar_template.wad");
    let _ = (SCRIPT_NAME, DEPS_HOST); // retained for reference; no longer used in the loadable path
}
