//! worldentity_probe — read-only inspection of the worldentity master ECS container
//! (name 0x50075B3B / type 0x5647C35D) in vz.wad resident block ~3185.
//!
//! Enumerates every COMP group (class name + type-hash + record count), then for the
//! vehicle-template-relevant components dumps sample records + entity_keys so we can see
//! whether spawn templates (0x8000_xxxx handles) live here alongside placed instances.
//!
//! Usage: worldentity_probe [vz.wad path]

use std::fs::File;

use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::schema::{parse_comp_groups, FieldValue};
use mercs2_formats::sges::decompress_block;

const WORLDENTITY_TYPE: u32 = 0x5647_C35D;
const GUIDMAP_TYPE: u32 = 0x140E_8728;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad".into()
    });
    let mut f = File::open(&path).unwrap_or_else(|_| panic!("open {path}"));
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).expect("ffcs");
    eprintln!("blocks: {}", arch.indx.len());

    // Find the block whose entry table contains the worldentity type-hash.
    let mut target: Option<(usize, Vec<u8>)> = None;
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
        if has {
            eprintln!("worldentity found in block index {bi}");
            target = Some((bi, dec));
            break;
        }
    }
    let (_bi, dec) = target.expect("no block carries the worldentity type");

    // Walk the block entry table; pull the worldentity + guidmap containers.
    let count = read_u32_le(&dec, 0) as usize;
    let mut pos = 4 + count * 16;
    let mut we_container: Option<Vec<u8>> = None;
    let mut gm_container: Option<Vec<u8>> = None;
    for ei in 0..count {
        let base = 4 + ei * 16;
        let name_hash = read_u32_le(&dec, base);
        let type_hash = read_u32_le(&dec, base + 4);
        let chunk_size = read_u32_le(&dec, base + 12) as usize;
        if pos + chunk_size > dec.len() { break; }
        let container = dec[pos..pos + chunk_size].to_vec();
        pos += chunk_size;
        if type_hash == WORLDENTITY_TYPE {
            eprintln!("worldentity entry: name=0x{name_hash:08X} size={chunk_size}");
            we_container = Some(container);
        } else if type_hash == GUIDMAP_TYPE {
            eprintln!("guidmap entry: name=0x{name_hash:08X} size={chunk_size}");
            gm_container = Some(container);
        }
    }

    let we = we_container.expect("worldentity container");
    println!("\n=== worldentity container: {} bytes, magic {:?} ===",
        we.len(), std::str::from_utf8(&we[0..4]).unwrap_or("????"));

    let groups = parse_comp_groups(&we);
    println!("COMP groups: {}", groups.len());

    // Component classes a vehicle template is built from (vehicles.md).
    let veh_classes = [
        "VehicleName", "VehicleClass", "VehiclePart", "VehiclePartType", "VehicleAmmo",
        "VehicleHealth", "VehicleDisguise", "VehicleSpawnList", "ModelName", "Name",
        "CarPhysicsV2", "TankPhysics", "BoatPhysics",
    ];
    let veh_hashes: Vec<(String, u32)> =
        veh_classes.iter().map(|c| (c.to_string(), pandemic_hash_m2(c))).collect();

    // Summary table of all groups.
    println!("\n--- all COMP groups (name | type_hash | schm? | data bytes | stride | records) ---");
    for g in &groups {
        let name = g.name.clone().unwrap_or_else(|| "<none>".into());
        let th = g.type_hash.unwrap_or(0);
        let dlen = g.data.as_ref().map(|d| d.len()).unwrap_or(0);
        let (stride, nrec) = match g.schema() {
            Some(s) if !s.is_variable_length() && s.record_stride() > 0 =>
                (s.record_stride(), dlen / s.record_stride().max(1)),
            _ => (0, 0),
        };
        let interesting = veh_classes.contains(&name.as_str());
        if interesting || dlen > 0 {
            println!("{:>28} | 0x{:08X} | {} | {:>8} | {:>4} | {:>6}{}",
                name, th, if g.schm.is_some() { "Y" } else { "-" }, dlen, stride, nrec,
                if interesting { "   <-- vehicle" } else { "" });
        }
    }

    // Header auto-detect: worldentity COMP `data` = [header?][records]. Find the smallest header
    // h in {0,4,8,12,16} for which (len-h) is a whole multiple of the schm record stride.
    let detect = |dlen: usize, stride: usize| -> Option<usize> {
        if stride == 0 { return None; }
        [0usize, 4, 8, 12, 16].into_iter().find(|&h| dlen >= h && (dlen - h) % stride == 0)
    };
    let fmt_val = |h: &u32, v: &FieldValue| match v {
        FieldValue::U32(x) => format!("0x{h:08X}=0x{x:08X}"),
        FieldValue::F32(x) => format!("0x{h:08X}={x}"),
        FieldValue::U16(x) => format!("0x{h:08X}={x}"),
        FieldValue::U8(x) => format!("0x{h:08X}={x}"),
        FieldValue::Bit(x) => format!("0x{h:08X}={x}"),
        other => format!("0x{h:08X}={other:?}"),
    };

    // GLOBAL SCAN: across every group, are there ANY entity_keys with bit-31 set (template handles)?
    println!("\n=== bit-31 (template-handle) entity_key scan across all groups ===");
    let mut total_bit31 = 0usize;
    let mut total_keys = 0usize;
    let mut bit31_examples: Vec<(String, u32)> = Vec::new();
    for g in &groups {
        let (Some(schema), Some(data)) = (g.schema(), g.data.as_ref()) else { continue };
        if schema.is_variable_length() { continue; }
        let stride = schema.record_stride();
        let Some(h) = detect(data.len(), stride) else { continue };
        for rec in data[h..].chunks_exact(stride) {
            let key = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
            total_keys += 1;
            if key & 0x8000_0000 != 0 {
                total_bit31 += 1;
                if bit31_examples.len() < 20 {
                    bit31_examples.push((g.name.clone().unwrap_or_default(), key));
                }
            }
        }
    }
    println!("total records scanned: {total_keys}; with bit-31: {total_bit31}");
    for (n, k) in &bit31_examples { println!("    bit31 key 0x{k:08X} in {n}"); }

    // CRX cross-reference: the template handles my notes recorded + known model hashes.
    let crx_handles = [0x8000_85c5u32, 0x8000_9c7a, 0x8000_9c7b, 0x8000_9c7c, 0x8000_9c7d, 0x8000_9c7e];
    let crx_models = [0xFCAE_37ABu32, 0x03F9_5C1D, pandemic_hash_m2("civ_veh_car_crx_racing")];
    let jc2_model = 0xB89B_2F9Au32;
    println!("\n=== ModelName record hunt (CRX handles/models + our JC2 model) ===");
    if let Some(g) = groups.iter().find(|g| g.name.as_deref() == Some("ModelName")) {
        if let (Some(schema), Some(data)) = (g.schema(), g.data.as_ref()) {
            let stride = schema.record_stride();
            if let Some(h) = detect(data.len(), stride) {
                let mfield = schema.fields.first().map(|f| f.name_hash).unwrap_or(0);
                let mut n = 0;
                for rec in data[h..].chunks_exact(stride) {
                    let key = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
                    let model = u32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]);
                    let hit_h = crx_handles.contains(&key);
                    let hit_m = crx_models.contains(&model) || model == jc2_model;
                    if hit_h || hit_m {
                        println!("    ModelName key=0x{key:08X} -> model=0x{model:08X}{}{}",
                            if hit_h { " [CRX-handle]" } else { "" },
                            if hit_m { " [known-model]" } else { "" });
                    }
                    n += 1;
                }
                let _ = mfield;
                println!("    (scanned {n} ModelName records, header {h}B)");
            }
        }
    }

    // Per-vehicle-component sample dump (header-aware).
    for (cname, chash) in &veh_hashes {
        let Some(g) = groups.iter().find(|g| g.type_hash == Some(*chash)
            || g.name.as_deref() == Some(cname.as_str())) else { continue };
        let Some(schema) = g.schema() else {
            println!("\n### {cname} (0x{chash:08X}): variable-length or no schm");
            continue;
        };
        let Some(data) = g.data.as_ref() else { continue };
        if schema.is_variable_length() {
            println!("\n### {cname}: variable-length (StringRef) — {} data bytes", data.len());
            continue;
        }
        let stride = schema.record_stride();
        let Some(h) = detect(data.len(), stride) else {
            println!("\n### {cname}: stride {stride} does not divide len {} (any header) — first 32B: {:02X?}",
                data.len(), &data[..32.min(data.len())]);
            continue;
        };
        let nrec = (data.len() - h) / stride;
        println!("\n### {cname} (0x{chash:08X}) stride {stride} header {h}B — {nrec} records; fields: {}",
            schema.fields.iter().map(|fl| format!("0x{:08X}:{:?}@{}", fl.name_hash, fl.field_type, fl.byte_offset))
                .collect::<Vec<_>>().join(", "));
        let mut bit31 = 0;
        for rec in data[h..].chunks_exact(stride) {
            if u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]) & 0x8000_0000 != 0 { bit31 += 1; }
        }
        println!("    entity_keys: {nrec} total, {bit31} with bit-31");
        for rec in data[h..].chunks_exact(stride).take(6) {
            let key = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
            let payload = &rec[4..];
            let mut parts = Vec::new();
            for fl in &schema.fields {
                let o = fl.byte_offset as usize;
                if o + 4 <= payload.len() {
                    let w = u32::from_le_bytes([payload[o], payload[o+1], payload[o+2], payload[o+3]]);
                    parts.push(fmt_val(&fl.name_hash, &FieldValue::U32(w)));
                }
            }
            println!("    key=0x{key:08X}  {}", parts.join(" "));
        }
    }

    // === RECIPE: enumerate the COMPLETE component set for target car-template handles ===
    // Every COMP whose data (header-aware) contains a record keyed by the handle = the template's
    // component recipe. This is exactly what a novel template must replicate.
    let targets: Vec<u32> = std::env::args().skip(2)
        .filter_map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .collect();
    let targets = if targets.is_empty() {
        vec![0x8000_9FC3u32, 0x8000_9C7A, 0x8000_9C7B, 0x8000_9C7C]
    } else { targets };

    for th in &targets {
        println!("\n########## component recipe for handle 0x{th:08X} ##########");
        for g in &groups {
            let (Some(schema), Some(data)) = (g.schema(), g.data.as_ref()) else { continue };
            if schema.is_variable_length() { continue; }
            let stride = schema.record_stride();
            let Some(h) = detect(data.len(), stride) else { continue };
            for rec in data[h..].chunks_exact(stride) {
                let key = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
                if key != *th { continue; }
                let payload = &rec[4..];
                let mut parts = Vec::new();
                for fl in &schema.fields {
                    let o = fl.byte_offset as usize;
                    if o + 4 <= payload.len() {
                        let w = u32::from_le_bytes([payload[o], payload[o+1], payload[o+2], payload[o+3]]);
                        parts.push(fmt_val(&fl.name_hash, &FieldValue::U32(w)));
                    }
                }
                println!("  {:>26} (0x{:08X}) stride {stride}: {}",
                    g.name.clone().unwrap_or_default(), g.type_hash.unwrap_or(0), parts.join(" "));
            }
        }
    }

    // === Reference-graph + subgraph-closure analysis (the mint tool's core design input) ===
    // Parse Name COMP -> handle->name. Then build owner_handle -> {referenced 0x8000xxxx}. Pick car
    // "(Driver)" roots and compute each root's transitive closure; flag handles referenced by >1
    // distinct root as SHARED (must not be remapped by a clone).
    {
        // Parse Name COMP: flat [u32 enabled][u32 handle][cstring name\0].
        let mut names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        if let Some(g) = groups.iter().find(|g| g.name.as_deref() == Some("Name")) {
            if let Some(data) = g.data.as_ref() {
                let mut p = 0usize;
                while p + 8 <= data.len() {
                    let _enabled = u32::from_le_bytes([data[p], data[p+1], data[p+2], data[p+3]]);
                    let handle = u32::from_le_bytes([data[p+4], data[p+5], data[p+6], data[p+7]]);
                    p += 8;
                    let start = p;
                    while p < data.len() && data[p] != 0 { p += 1; }
                    let name = String::from_utf8_lossy(&data[start..p]).into_owned();
                    p += 1; // skip \0
                    // Framing carries a 1-byte pad after the string terminator (records restart with
                    // the enabled=1 word 01 00 00 00). Skip a lone 0x00 pad before the next record.
                    if p < data.len() && data[p] == 0 { p += 1; }
                    if handle & 0x8000_0000 != 0 && !name.is_empty() {
                        names.insert(handle, name);
                    }
                }
            }
        }
        eprintln!("parsed {} handle->name entries", names.len());

        // Reference graph: owner -> referenced handles (from every non-variable COMP, header-aware).
        let mut refs: std::collections::HashMap<u32, std::collections::HashSet<u32>> = std::collections::HashMap::new();
        let mut ref_by: std::collections::HashMap<u32, std::collections::HashSet<u32>> = std::collections::HashMap::new();
        for g in &groups {
            let (Some(schema), Some(data)) = (g.schema(), g.data.as_ref()) else { continue };
            if schema.is_variable_length() { continue; }
            let stride = schema.record_stride();
            let Some(h) = detect(data.len(), stride) else { continue };
            for rec in data[h..].chunks_exact(stride) {
                let owner = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
                if owner & 0x8000_0000 == 0 { continue; }
                let payload = &rec[4..];
                for fl in &schema.fields {
                    let o = fl.byte_offset as usize;
                    if o + 4 <= payload.len() {
                        let w = u32::from_le_bytes([payload[o], payload[o+1], payload[o+2], payload[o+3]]);
                        // template handle = 0x8000xxxx (model/data hashes also set bit-31, exclude them)
                        if (w & 0xFFFF_0000) == 0x8000_0000 && w != owner {
                            refs.entry(owner).or_default().insert(w);
                            ref_by.entry(w).or_default().insert(owner);
                        }
                    }
                }
            }
        }

        // Car "(Driver)" roots.
        let mut roots: Vec<(u32, String)> = names.iter()
            .filter(|(_, n)| n.contains("(Driver)") && (n.contains("Car") || n.contains("CRX") || n.contains("Sports")))
            .map(|(h, n)| (*h, n.clone())).collect();
        roots.sort_by_key(|(h, _)| *h);
        println!("\n=== car '(Driver)' spawnable roots ({}) ===", roots.len());
        for (h, n) in roots.iter().take(30) { println!("  0x{h:08X}  {n}"); }

        // PRIVATE-CLUSTER growth (dominator region): start at root; absorb a referenced node only
        // if ALL of its referrers are already in the cluster (nothing outside depends on it) and it
        // is not itself a named spawnable template. References that leave the cluster (to shared
        // infra: weapons/soldiers/generic seats) are KEPT pointing at the originals.
        let chosen: Vec<(u32, String)> = roots.iter()
            .filter(|(_, n)| n.contains("CRX")).cloned().collect();
        for (root, rname) in chosen {
            let mut cluster: std::collections::HashSet<u32> = std::collections::HashSet::new();
            cluster.insert(root);
            loop {
                let mut added = false;
                // candidates = nodes referenced by the cluster but not yet in it
                let cands: Vec<u32> = cluster.iter()
                    .filter_map(|h| refs.get(h)).flatten().copied()
                    .filter(|c| !cluster.contains(c)).collect();
                for c in cands {
                    if names.get(&c).is_some_and(|n| n != &rname) { continue; } // other named template
                    let referrers = ref_by.get(&c);
                    let all_in = referrers.map(|s| s.iter().all(|r| cluster.contains(r))).unwrap_or(true);
                    if all_in { cluster.insert(c); added = true; }
                }
                if !added { break; }
            }
            // External refs the cluster keeps (shared infra) + models it points at.
            let mut ext_refs: std::collections::BTreeSet<u32> = Default::default();
            for h in &cluster {
                if let Some(rs) = refs.get(h) {
                    for &r in rs { if !cluster.contains(&r) { ext_refs.insert(r); } }
                }
            }
            println!("\n=== PRIVATE cluster of 0x{root:08X} \"{rname}\": {} nodes (clone these) ===", cluster.len());
            let mut cv: Vec<u32> = cluster.iter().copied().collect(); cv.sort();
            for hh in &cv {
                // which COMPs does this node carry?
                let comps: Vec<String> = groups.iter().filter(|g| {
                    let (Some(sc), Some(d)) = (g.schema(), g.data.as_ref()) else { return false };
                    if sc.is_variable_length() { return false; }
                    let st = sc.record_stride();
                    let Some(hd) = detect(d.len(), st) else { return false };
                    d[hd..].chunks_exact(st).any(|r| u32::from_le_bytes([r[0],r[1],r[2],r[3]]) == *hh)
                }).map(|g| g.name.clone().unwrap_or_default()).collect();
                println!("  0x{hh:08X} {:24} [{}]", names.get(hh).cloned().unwrap_or_default(), comps.join(","));
            }
            println!("  KEEPS {} external (shared) refs: {}", ext_refs.len(),
                ext_refs.iter().take(24).map(|h| format!("0x{h:08X}{}", names.get(h).map(|n| format!("({n})")).unwrap_or_default()))
                    .collect::<Vec<_>>().join(" "));
        }

        // === CLONE MANIFEST: every record keyed by the source vehicle entity = the mint input ===
        let src = 0x8000_9FC3u32;
        // Free handle = smallest unused 0x8000xxxx above all seen keys.
        let mut max_h = 0u32;
        for g in &groups {
            let (Some(sc), Some(d)) = (g.schema(), g.data.as_ref()) else { continue };
            if sc.is_variable_length() { continue; }
            let st = sc.record_stride();
            let Some(hd) = detect(d.len(), st) else { continue };
            for r in d[hd..].chunks_exact(st) {
                let k = u32::from_le_bytes([r[0],r[1],r[2],r[3]]);
                if (k & 0xFFFF_0000) == 0x8000_0000 { max_h = max_h.max(k); }
            }
        }
        for &k in names.keys() { if (k & 0xFFFF_0000) == 0x8000_0000 { max_h = max_h.max(k); } }
        let free = max_h + 1;
        println!("\n=== CLONE MANIFEST: source entity 0x{src:08X} -> fresh handle 0x{free:08X} ===");
        println!("(max handle in use = 0x{max_h:08X}; guidmap count 6127)");
        let mut total = 0;
        for g in &groups {
            let (Some(sc), Some(d)) = (g.schema(), g.data.as_ref()) else { continue };
            if sc.is_variable_length() { continue; }
            let st = sc.record_stride();
            let Some(hd) = detect(d.len(), st) else { continue };
            let n = d[hd..].chunks_exact(st).filter(|r| u32::from_le_bytes([r[0],r[1],r[2],r[3]]) == src).count();
            if n > 0 {
                total += n;
                let has_model = g.name.as_deref() == Some("ModelName");
                let nrec_total = (d.len() - hd) / st;
                let head_u32: Vec<String> = d[..hd.min(d.len())].chunks(4)
                    .map(|c| format!("0x{:08X}", u32::from_le_bytes([c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0), *c.get(3).unwrap_or(&0)]))).collect();
                println!("  {:>26} (0x{:08X}) stride {st} hdr {hd}B [{}] nrec_total {nrec_total} x{n}{}",
                    g.name.clone().unwrap_or_default(), g.type_hash.unwrap_or(0), head_u32.join(","),
                    if has_model { "  <-- repoint->0xB89B2F9A" } else { "" });
            }
        }
        println!("  TOTAL {total} records to clone under 0x{free:08X}; + Name[\"JC2 Sportscar\"->0x{free:08X}] + guidmap append");
    }

    // === VERIFY the SoA-bucket model: data = concat of [u32 N][N keys u32][N payloads] to end ===
    println!("\n=== SoA-bucket parse verification (worldentity COMP data = [N][N keys][N payloads]*) ===");
    for cname in ["ModelName", "_CarWheel", "VehiclePart", "HibernationControl", "FactionMarker",
                  "Label", "NodeHealth", "Health", "SoundEffect", "Turret"] {
        if let Some(g) = groups.iter().find(|g| g.name.as_deref() == Some(cname)) {
            if let (Some(sc), Some(d)) = (g.schema(), g.data.as_ref()) {
                let ps = sc.payload_stride as usize;
                // Brute-force the serialized per-value size V for the multimap model
                // [u32 N][u32 key][N × V]. Report every V that consumes the chunk EXACTLY.
                let try_v = |v: usize| -> Option<(usize, usize)> {
                    let mut pos = 0usize; let mut keys = 0usize; let mut vals = 0usize;
                    while pos + 8 <= d.len() {
                        let n = u32::from_le_bytes([d[pos], d[pos+1], d[pos+2], d[pos+3]]) as usize;
                        let key = u32::from_le_bytes([d[pos+4], d[pos+5], d[pos+6], d[pos+7]]);
                        let key_ok = (key & 0xFFFF_0000) == 0x8000_0000 || (key & 0xFFFF_0000) == 0x9000_0000 || key < 0x0010_0000;
                        if !key_ok || n > 200_000 { return None; }
                        pos += 8 + n * v; keys += 1; vals += n;
                    }
                    (pos == d.len()).then_some((keys, vals))
                };
                // SoA model: [u32 N][N × u32 key][N × P payload] buckets to end. N==0 = empty bucket.
                let try_soa = |p: usize| -> Option<(usize, usize)> {
                    let mut pos = 0usize; let mut groups = 0usize; let mut recs = 0usize;
                    while pos + 4 <= d.len() {
                        let n = u32::from_le_bytes([d[pos], d[pos+1], d[pos+2], d[pos+3]]) as usize;
                        pos += 4;
                        if n > 200_000 { return None; }
                        if n > 0 {
                            if pos + 4 > d.len() { return None; }
                            let key = u32::from_le_bytes([d[pos], d[pos+1], d[pos+2], d[pos+3]]);
                            let key_ok = (key & 0x8000_0000) != 0 || key < 0x0010_0000;
                            if !key_ok { return None; }
                        }
                        pos += n * 4 + n * p; groups += 1; recs += n;
                    }
                    (pos == d.len()).then_some((groups, recs))
                };
                // SHARED-bucket model: [u32 N][N × u32 key][ONE shared payload of P bytes] to end.
                let try_shared = |p: usize| -> Option<(usize, usize)> {
                    let mut pos = 0usize; let mut buckets = 0usize; let mut keys = 0usize;
                    while pos + 4 <= d.len() {
                        let n = u32::from_le_bytes([d[pos], d[pos+1], d[pos+2], d[pos+3]]) as usize;
                        pos += 4;
                        if n == 0 || n > 200_000 { return None; }
                        if pos + 4 > d.len() { return None; }
                        let key = u32::from_le_bytes([d[pos], d[pos+1], d[pos+2], d[pos+3]]);
                        if !((key & 0x8000_0000) != 0 || key < 0x0010_0000) { return None; }
                        pos += n * 4 + p; buckets += 1; keys += n;
                    }
                    (pos == d.len()).then_some((buckets, keys))
                };
                let sizes = [4usize,6,8,10,12,16,20,24,28,32,36,40,44,48,52,56,60,64,68,72,84,100,116,124,144];
                let mut mm = Vec::new(); let mut soa = Vec::new(); let mut shr = Vec::new();
                for &v in &sizes { if let Some((k, vv)) = try_v(v) { mm.push(format!("V{v}({k}k/{vv}v)")); } }
                for &p in &sizes { if let Some((g, r)) = try_soa(p) { soa.push(format!("P{p}({g}g/{r}r)")); } }
                for &p in &sizes { if let Some((b, k)) = try_shared(p) { shr.push(format!("P{p}({b}b/{k}k)")); } }
                println!("  {:>20} ps={:>3} len={:>7} | MM: {} | SoA: {} | SHARED: {}", cname, ps, d.len(),
                    if mm.is_empty() { "-".into() } else { mm.join(",") },
                    if soa.is_empty() { "-".into() } else { soa.join(",") },
                    if shr.is_empty() { "-".into() } else { shr.join(",") });
            }
        }
    }

    // === Full u32 dump of _CarWheel + HibernationControl for hand-decode ===
    for cname in ["_CarWheel", "HibernationControl"] {
        if let Some(g) = groups.iter().find(|g| g.name.as_deref() == Some(cname)) {
            if let Some(d) = g.data.as_ref() {
                println!("\n--- {cname} u32 dump (first 200 bytes) ---");
                for o in (0..200.min(d.len())).step_by(4) {
                    if o + 4 <= d.len() {
                        let w = u32::from_le_bytes([d[o], d[o+1], d[o+2], d[o+3]]);
                        let f = f32::from_bits(w);
                        let tag = if (w & 0xFFFF_0000) == 0x8000_0000 { "handle" }
                            else if w < 0x1000 { "small" }
                            else if f.abs() > 1e-6 && f.abs() < 1e9 { "~float" } else { "" };
                        println!("  +{o:>4} (0x{o:03X}): 0x{w:08X}  {:>12.4}  {tag}", if tag == "~float" { f } else { 0.0 });
                    }
                }
            }
        }
    }

    // === RAW head+tail of worldentity COMP data (resolve prefix vs trailer vs stride) ===
    for cname in ["ModelName", "_CarWheel", "VehiclePart", "HibernationControl", "FactionMarker", "Label"] {
        if let Some(g) = groups.iter().find(|g| g.name.as_deref() == Some(cname)) {
            if let (Some(sc), Some(d)) = (g.schema(), g.data.as_ref()) {
                let st = sc.record_stride();
                println!("\n--- {cname}: schm payload_stride={} record_stride={} data_len={} (len%stride={}) ---",
                    sc.payload_stride, st, d.len(), d.len() % st);
                let hx = |b: &[u8]| b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ");
                println!("  HEAD 40: {}", hx(&d[..40.min(d.len())]));
                let tl = d.len().saturating_sub(40);
                println!("  TAIL 40: {}", hx(&d[tl..]));
                // interpret head as u32s
                let u = |o: usize| u32::from_le_bytes([d[o], d[o+1], d[o+2], d[o+3]]);
                print!("  HEAD u32: ");
                for i in 0..8 { if (i*4+4) <= d.len() { print!("0x{:08X} ", u(i*4)); } }
                println!();
            }
        }
    }

    // === Name COMP raw dump (name-string -> handle mapping = the spawn registry source) ===
    if let Some(g) = groups.iter().find(|g| g.name.as_deref() == Some("Name")) {
        if let Some(data) = g.data.as_ref() {
            println!("\n=== Name COMP data: {} bytes; schm fields: {} ===", data.len(),
                g.schema().map(|s| s.fields.iter()
                    .map(|f| format!("0x{:08X}:{:?}@{}", f.name_hash, f.field_type, f.byte_offset))
                    .collect::<Vec<_>>().join(", ")).unwrap_or_default());
            println!("first 256 bytes hex:");
            for row in data[..256.min(data.len())].chunks(16) {
                let hex: Vec<String> = row.iter().map(|b| format!("{b:02X}")).collect();
                let asc: String = row.iter().map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect();
                println!("  {:<48} {}", hex.join(" "), asc);
            }
        }
    }

    if let Some(gm) = gm_container {
        println!("\n=== guidmap container: {} bytes, magic {:?} ===",
            gm.len(), std::str::from_utf8(&gm[0..4.min(gm.len())]).unwrap_or("????"));
        // Raw descriptor table (guidmap is not a COMP tree).
        if gm.len() >= 20 && &gm[0..4] == b"UCFX" {
            let data_off = read_u32_le(&gm, 4) as usize;
            let ndesc = read_u32_le(&gm, 16) as usize;
            println!("data_area_off={data_off} ndesc={ndesc}");
            for i in 0..ndesc.min(40) {
                let ro = 20 + i * 20;
                if ro + 20 > gm.len() { break; }
                let tag = std::str::from_utf8(&gm[ro..ro+4]).unwrap_or("????");
                let u0 = read_u32_le(&gm, ro + 4);
                let sz = read_u32_le(&gm, ro + 8);
                println!("  desc[{i}] {tag:?} u0=0x{u0:08X} sz={sz}");
            }
            // Hexdump the first non-sentinel body.
            for i in 0..ndesc {
                let ro = 20 + i * 20;
                if ro + 20 > gm.len() { break; }
                let u0 = read_u32_le(&gm, ro + 4) as usize;
                let sz = read_u32_le(&gm, ro + 8) as usize;
                if u0 == 0xFFFF_FFFF { continue; }
                let start = if data_off > 0 { data_off + u0 } else { 8 + u0 };
                if start + sz.min(128) > gm.len() { break; }
                let tag = std::str::from_utf8(&gm[ro..ro+4]).unwrap_or("????");
                println!("  body desc[{i}] {tag:?} @0x{start:X} sz {sz}, first 128B:");
                for row in gm[start..start + 128.min(sz)].chunks(16) {
                    let hex: Vec<String> = row.iter().map(|b| format!("{b:02X}")).collect();
                    let asc: String = row.iter().map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect();
                    println!("    {:<48} {}", hex.join(" "), asc);
                }
                break;
            }
        }
    }
}
