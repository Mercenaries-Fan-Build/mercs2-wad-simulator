//! humanstate_probe — locate and dump every HumanStateTable asset (type hash
//! 0xECE70371 = pandemic_hash_m2("HumanStateTable")) in vz.wad.
//!
//! Container layout is the same UCFX dim-table shape the animationtable family uses:
//!   block = [u32 count][count×16B: name_hash,type_hash,field_c,size][containers]
//!   INFO body = [u16 keyDims][u16 totalDims][u16 count]
//!   TYPE body = totalDims × ([ASCII name]\0 [u16 field])
//!   VALU body = count rows × totalDims u32

use std::collections::BTreeMap;

use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;

const TYPE_HUMANSTATE: u32 = 0xECE7_0371;
const TYPE_ANIMTABLE: u32 = 0x2073_59C7;
const NONE_SENTINEL: u32 = 0x27DE_7135;
const ACTIONTABLE: u32 = 0x6802_C321;

fn r_u16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn r_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// (index, name_hash, type_hash, offset, size) for every entry in a multi-entry block.
fn entries(dec: &[u8]) -> Vec<(usize, u32, u32, usize, usize)> {
    let mut out = Vec::new();
    if dec.len() < 4 {
        return out;
    }
    let count = r_u32(dec, 0) as usize;
    let max = dec.len().saturating_sub(4) / 16;
    if count == 0 || count > 200_000 {
        return out;
    }
    let mut pos = 4 + count * 16;
    for i in 0..count.min(max) {
        let b = 4 + i * 16;
        let nh = r_u32(dec, b);
        let th = r_u32(dec, b + 4);
        let sz = r_u32(dec, b + 12) as usize;
        out.push((i, nh, th, pos, sz));
        pos += sz;
    }
    out
}

fn chunks(cont: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    if cont.len() < 20 || &cont[0..4] != b"UCFX" {
        return out;
    }
    let data_area = r_u32(cont, 4) as usize;
    let ndesc = r_u32(cont, 16) as usize;
    if 20 + ndesc * 20 > cont.len() {
        return out;
    }
    for i in 0..ndesc {
        let off = 20 + i * 20;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&cont[off..off + 4]);
        let body_off = r_u32(cont, off + 4) as usize;
        let body_sz = r_u32(cont, off + 8) as usize;
        let start = data_area + body_off;
        if start + body_sz <= cont.len() {
            out.push((tag, start, body_sz));
        }
    }
    out
}

fn column_names(body: &[u8], total_dims: usize) -> Vec<String> {
    let mut names = Vec::new();
    let mut p = 0;
    for _ in 0..total_dims {
        let start = p;
        while p < body.len() && body[p] != 0 {
            p += 1;
        }
        names.push(String::from_utf8_lossy(&body[start..p]).to_string());
        p += 1;
        p += 2;
        if p > body.len() {
            break;
        }
    }
    names
}

struct Table {
    key_dims: usize,
    total_dims: usize,
    info_count: usize,
    names: Vec<String>,
    valu: Option<(usize, usize)>,
}

fn parse_table(cont: &[u8]) -> Table {
    let chs = chunks(cont);
    let (mut kd, mut td, mut ct) = (0usize, 0usize, 0usize);
    for (tag, start, sz) in &chs {
        if tag == b"INFO" && *sz >= 6 {
            kd = r_u16(cont, *start) as usize;
            td = r_u16(cont, *start + 2) as usize;
            ct = r_u16(cont, *start + 4) as usize;
        }
    }
    let mut names = Vec::new();
    let mut valu = None;
    for (tag, start, sz) in &chs {
        if tag == b"TYPE" {
            names = column_names(&cont[*start..*start + *sz], td);
        }
        if tag == b"VALU" {
            valu = Some((*start, *sz));
        }
    }
    Table { key_dims: kd, total_dims: td, info_count: ct, names, valu }
}

/// Candidate name vocabulary — every one is hashed live, nothing is hard-coded.
const WORDS: &[&str] = &[
    "*", "Upright", "upright", "Swim", "swim", "InVehicle", "invehicle", "crouched", "Crouched",
    "prone", "Prone", "cower", "Cower", "carrying", "Carrying", "carried", "Carried", "Subdued",
    "subdued", "KnockedDown", "knockeddown", "Knockdown", "knockdown", "onladder", "OnLadder",
    "ladder", "Ladder", "crouchcover", "CrouchCover", "cover", "Cover", "inair", "InAir", "air",
    "scuba", "Scuba", "humanshield", "HumanShield", "shield", "Shield", "idle", "Idle", "walk",
    "Walk", "run", "Run", "jog", "Jog", "sprint", "Sprint", "fidget", "Fidget", "die", "Die",
    "dead", "Dead", "dive", "Dive", "jump", "Jump", "fall", "Fall", "getup", "GetUp", "pickup",
    "PickUp", "climb", "Climb", "land", "Land", "turn", "Turn", "aim", "Aim", "fire", "Fire",
    "reload", "Reload", "throw", "Throw", "melee", "Melee", "punch", "Punch", "kick", "Kick",
    "stand", "Stand", "standing", "Standing", "sit", "Sit", "sitting", "Sitting", "sleep",
    "Sleep", "swimming", "Swimming", "wade", "Wade", "tread", "Tread", "float", "Float",
    "underwater", "UnderWater", "Underwater", "surface", "Surface", "driver", "Driver",
    "passenger", "Passenger", "gunner", "Gunner", "seat", "Seat", "vehicle", "Vehicle",
    "parachute", "Parachute", "rappel", "Rappel", "zipline", "ZipLine", "grapple", "Grapple",
    "hover", "Hover", "flying", "Flying", "fly", "Fly", "ragdoll", "RagDoll", "Ragdoll",
    "stagger", "Stagger", "stumble", "Stumble", "flinch", "Flinch", "hit", "Hit", "damage",
    "Damage", "injured", "Injured", "wounded", "Wounded", "burning", "Burning", "burn", "Burn",
    "electrocuted", "Electrocuted", "stunned", "Stunned", "stun", "Stun", "surrender",
    "Surrender", "surrendered", "Surrendered", "handsup", "HandsUp", "arrest", "Arrest",
    "detained", "Detained", "captured", "Captured", "hostage", "Hostage", "restrained",
    "Restrained", "grabbed", "Grabbed", "grab", "Grab", "held", "Held", "hold", "Hold",
    "dragging", "Dragging", "drag", "Drag", "dragged", "Dragged", "pushing", "Pushing",
    "mounted", "Mounted", "mount", "Mount", "dismount", "Dismount", "enter", "Enter", "exit",
    "Exit", "entering", "Entering", "exiting", "Exiting", "vault", "Vault", "mantle", "Mantle",
    "roll", "Roll", "dodge", "Dodge", "slide", "Slide", "sprinting", "Sprinting", "crouch",
    "Crouch", "crouching", "Crouching", "proning", "kneel", "Kneel", "kneeling", "Kneeling",
    "lying", "Lying", "lie", "Lie", "downed", "Downed", "down", "Down", "revive", "Revive",
    "reviving", "Reviving", "respawn", "Respawn", "spawn", "Spawn", "none", "None", "any",
    "Any", "all", "All", "default", "Default", "normal", "Normal", "base", "Base", "generic",
    "Generic", "unknown", "Unknown", "invalid", "Invalid", "null", "Null", "empty", "Empty",
    "true", "True", "false", "False", "yes", "Yes", "no", "No", "on", "On", "off", "Off",
    "left", "Left", "right", "Right", "front", "Front", "back", "Back", "forward", "Forward",
    "backward", "Backward", "up", "Up", "start", "Start", "stop", "Stop", "loop", "Loop",
    "once", "Once", "human", "Human", "Human1", "player", "Player", "npc", "NPC", "ai", "AI",
    "civilian", "Civilian", "soldier", "Soldier", "merc", "Merc", "pmc", "PMC",
    "HumanState", "humanstate", "State", "state", "Stance", "stance", "Action", "action",
    "AimState", "Tandem", "Target", "ActionDirection", "DamageDirection", "AnimationHandles",
    "PartitionMask", "Looping", "Driven", "ActionMask", "LocomotionMask", "StateName",
    "Locomotion", "locomotion", "Partition", "partition", "Mask", "mask", "Flags", "flags",
    "Priority", "priority", "Blend", "blend", "Transition", "transition", "Next", "next",
    "Parent", "parent", "Child", "child", "Type", "type", "Name", "name", "Id", "id",
    "swimidle", "SwimIdle", "swimwalk", "treadwater", "TreadWater", "waterentry", "WaterEntry",
    "highdive", "HighDive", "cannonball", "belly", "Belly", "backstroke", "BackStroke",
    "freestyle", "FreeStyle", "breaststroke", "BreastStroke",
];

fn crack(v: u32) -> Option<&'static str> {
    WORDS.iter().copied().find(|w| pandemic_hash_m2(w) == v)
}

fn fmt(v: u32) -> String {
    match crack(v) {
        Some(n) => format!("0x{v:08X}({n})"),
        None => format!("0x{v:08X}"),
    }
}

/// Read NUL-terminated ASCII strings out of a chunk body from `p` onward.
fn strs(body: &[u8], from: usize, n: usize) -> (Vec<String>, usize) {
    let mut out = Vec::new();
    let mut p = from;
    for _ in 0..n {
        if p >= body.len() {
            out.push(String::new());
            continue;
        }
        let s = p;
        while p < body.len() && body[p] != 0 {
            p += 1;
        }
        out.push(String::from_utf8_lossy(&body[s..p]).to_string());
        p += 1;
    }
    (out, p)
}

/// Decode the SINF (stance) / AINF (action) / TRNS (transition) chunk stream.
///   INFO = [u16 a][u16 stanceCount]
///   SINF = [name\0][u16 actionCount]
///   AINF = [name\0][u16 transitionCount][6 × string]
///   TRNS = [7 × string]
fn decode_state_machine(
    cont: &[u8],
    chs: &[([u8; 4], usize, usize)],
) -> (BTreeMap<u32, String>, BTreeMap<u32, Vec<u32>>) {
    use std::collections::BTreeSet;
    let mut stances: Vec<(String, u16, Vec<(String, u16, Vec<String>, Vec<Vec<String>>)>)> =
        Vec::new();
    let mut trns_field_counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut ainf_field_counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut all_strings: BTreeSet<String> = BTreeSet::new();
    let mut info_a = 0u16;
    let mut info_b = 0u16;
    let mut n_sinf = 0usize;
    let mut n_ainf = 0usize;
    let mut n_trns = 0usize;
    let mut other_tags: BTreeMap<[u8; 4], usize> = BTreeMap::new();

    for (tag, st, cz) in chs {
        let body = &cont[*st..*st + *cz];
        match &tag[..] {
            b"INFO" => {
                if body.len() >= 4 {
                    info_a = r_u16(body, 0);
                    info_b = r_u16(body, 2);
                }
            }
            b"SINF" => {
                n_sinf += 1;
                let (v, p) = strs(body, 0, 1);
                let n = if p + 1 < body.len() { r_u16(body, p) } else { 0 };
                all_strings.insert(v[0].clone());
                stances.push((v[0].clone(), n, Vec::new()));
            }
            b"AINF" => {
                n_ainf += 1;
                let (v, p) = strs(body, 0, 1);
                let n = if p + 1 < body.len() { r_u16(body, p) } else { 0 };
                // remaining bytes are a run of NUL-terminated strings
                let mut rest = Vec::new();
                let mut q = p + 2;
                while q < body.len() {
                    let s = q;
                    while q < body.len() && body[q] != 0 {
                        q += 1;
                    }
                    rest.push(String::from_utf8_lossy(&body[s..q]).to_string());
                    q += 1;
                }
                *ainf_field_counts.entry(rest.len()).or_default() += 1;
                all_strings.insert(v[0].clone());
                for s in &rest {
                    all_strings.insert(s.clone());
                }
                if let Some(last) = stances.last_mut() {
                    last.2.push((v[0].clone(), n, rest, Vec::new()));
                }
            }
            b"TRNS" => {
                n_trns += 1;
                let mut fields = Vec::new();
                let mut q = 0usize;
                while q < body.len() {
                    let s = q;
                    while q < body.len() && body[q] != 0 {
                        q += 1;
                    }
                    fields.push(String::from_utf8_lossy(&body[s..q]).to_string());
                    q += 1;
                }
                *trns_field_counts.entry(fields.len()).or_default() += 1;
                for s in &fields {
                    all_strings.insert(s.clone());
                }
                if let Some(st) = stances.last_mut() {
                    if let Some(ac) = st.2.last_mut() {
                        ac.3.push(fields);
                    }
                }
            }
            other => {
                let mut t = [0u8; 4];
                t.copy_from_slice(other);
                *other_tags.entry(t).or_default() += 1;
            }
        }
    }

    println!(
        "  INFO(raw) = [0x{info_a:04X}, 0x{info_b:04X}]  ({info_a} / {info_b})   chunk census: SINF={n_sinf} AINF={n_ainf} TRNS={n_trns} other={:?}",
        other_tags
            .iter()
            .map(|(t, n)| format!("{}x{n}", String::from_utf8_lossy(t)))
            .collect::<Vec<_>>()
    );
    println!("  AINF trailing-field-count histogram: {ainf_field_counts:?}");
    println!("  TRNS field-count histogram: {trns_field_counts:?}");

    // ---- schema summary ----
    println!("\n  === STANCE -> ACTION -> TRANSITION tree ({} stances) ===", stances.len());
    let mut total_actions = 0usize;
    let mut total_trns = 0usize;
    for (sname, nact, acts) in &stances {
        total_actions += acts.len();
        println!(
            "  STANCE {sname:16} hash=0x{:08X}  declared actions={nact} actual={}",
            pandemic_hash_m2(sname),
            acts.len()
        );
        for (aname, ntr, flags, trs) in acts {
            total_trns += trs.len();
            println!(
                "     ACTION {aname:28} hash=0x{:08X} declared trns={ntr} actual={} flags={:?}",
                pandemic_hash_m2(aname),
                trs.len(),
                flags
            );
            for f in trs {
                println!("        TRNS {:?}", f);
            }
        }
    }
    println!("  totals: stances={} actions={total_actions} transitions={total_trns}", stances.len());
    println!("\n  === per-stance summary ===");
    for (sname, nact, acts) in &stances {
        let tr: usize = acts.iter().map(|(_, _, _, t)| t.len()).sum();
        println!(
            "     {sname:18} 0x{:08X}  actions={} (declared {nact})  transitions={tr}",
            pandemic_hash_m2(sname),
            acts.len()
        );
    }

    // ---- distinct vocabularies ----
    let mut stance_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut action_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut event_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut tgt_stance: BTreeMap<String, usize> = BTreeMap::new();
    let mut tgt_action: BTreeMap<String, usize> = BTreeMap::new();
    for (sname, _, acts) in &stances {
        *stance_names.entry(sname.clone()).or_default() += 1;
        for (aname, _, _, trs) in acts {
            *action_names.entry(aname.clone()).or_default() += 1;
            for f in trs {
                if !f.is_empty() {
                    *event_names.entry(f[0].clone()).or_default() += 1;
                }
                if f.len() > 1 {
                    *tgt_stance.entry(f[1].clone()).or_default() += 1;
                }
                if f.len() > 2 {
                    *tgt_action.entry(f[2].clone()).or_default() += 1;
                }
            }
        }
    }
    for (label, m) in [
        ("STANCE names", &stance_names),
        ("ACTION names", &action_names),
        ("TRNS field0 (event)", &event_names),
        ("TRNS field1 (target stance)", &tgt_stance),
        ("TRNS field2 (target action)", &tgt_action),
    ] {
        println!("\n  DISTINCT {label} ({}):", m.len());
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for (s, n) in v {
            println!("     0x{:08X} {s:34} x{n}", pandemic_hash_m2(s));
        }
    }

    // ---- crack the ActionTable's unnamed hashes against this container's whole string pool ----
    println!("\n  === CRACK unnamed ActionTable hashes against the {} strings in this container ===", all_strings.len());
    for (label, want) in [
        ("Stance 0x42C96259", 0x42C9_6259u32),
        ("Stance 0xB9832CE2", 0xB983_2CE2u32),
    ] {
        let hit: Vec<&String> = all_strings.iter().filter(|s| pandemic_hash_m2(s) == want).collect();
        println!("     {label}: {hit:?}");
    }
    // Full name->hash table of every string in the container (for external cracking).
    println!("  === all container strings (hash, string) ===");
    for s in &all_strings {
        println!("     0x{:08X}  {s}", pandemic_hash_m2(s));
    }

    let pool: BTreeMap<u32, String> =
        all_strings.iter().map(|s| (pandemic_hash_m2(s), s.clone())).collect();
    let map: BTreeMap<u32, Vec<u32>> = stances
        .iter()
        .map(|(s, _, acts)| {
            (pandemic_hash_m2(s), acts.iter().map(|(a, _, _, _)| pandemic_hash_m2(a)).collect())
        })
        .collect();
    (pool, map)
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let path = argv
        .iter()
        .position(|a| a == "--wad")
        .and_then(|i| argv.get(i + 1).cloned())
        .or_else(|| wad::resolve_vz_wad(None))
        .expect("locate vz.wad");
    println!("WAD = {path}");
    let mut w = wad::open(&path).expect("open wad");
    {
        let rows = wad::aset_types(&w, TYPE_HUMANSTATE);
        println!("ASET rows for name 0x{TYPE_HUMANSTATE:08X}: {rows:?}");
    }
    let nblocks = wad::block_paths(&w).len();
    println!("blocks in archive: {nblocks}");
    println!(
        "hash check: HumanStateTable=0x{:08X} AnimationTable=0x{:08X} Knockdown=0x{:08X} \
         Upright=0x{:08X} Swim=0x{:08X} *=0x{:08X}",
        pandemic_hash_m2("HumanStateTable"),
        pandemic_hash_m2("AnimationTable"),
        pandemic_hash_m2("Knockdown"),
        pandemic_hash_m2("Upright"),
        pandemic_hash_m2("Swim"),
        pandemic_hash_m2("*"),
    );

    // ---- (0) self-test: block 3185 must yield the known ActionTable via BOTH paths ----
    if nblocks > 3185 {
        let full = wad::decompress_block_index(&mut w, 3185).expect("full 3185");
        let head = wad::peek_block_head(&mut w, 3185, 1 << 20).expect("head 3185");
        println!(
            "\nself-test blk3185: full={}B entries={}  head={}B entries={}",
            full.len(),
            entries(&full).len(),
            head.len(),
            entries(&head).len()
        );
        println!(
            "  full[0..16]={:02X?}\n  head[0..16]={:02X?}",
            &full[..16.min(full.len())],
            &head[..16.min(head.len())]
        );
    }

    // ---- (1) whole-archive scan for entries whose TYPE hash == 0xECE70371 ----
    let mut hits: Vec<(u16, usize, u32, usize)> = Vec::new(); // block, idx, name_hash, size
    let mut animtable_hits: Vec<(u16, u32, usize)> = Vec::new();
    let quick = std::env::args().any(|a| a == "--quick");
    let mut scanned = 0usize;
    let mut truncated: Vec<u16> = Vec::new();
    let mut type_census: BTreeMap<u32, usize> = BTreeMap::new();
    for b in 0..if quick { 0 } else { nblocks as u16 } {
        let head = match wad::peek_block_head(&mut w, b, 1 << 20) {
            Ok(h) => h,
            Err(_) => continue,
        };
        scanned += 1;
        if head.len() >= 4 {
            let c = r_u32(&head, 0) as usize;
            if c > 0 && c <= 200_000 && 4 + c * 16 > head.len() {
                truncated.push(b);
            }
        }
        for (i, nh, th, _off, sz) in entries(&head) {
            *type_census.entry(th).or_default() += 1;
            if th == TYPE_HUMANSTATE {
                hits.push((b, i, nh, sz));
            }
            if th == TYPE_ANIMTABLE {
                animtable_hits.push((b, nh, sz));
            }
        }
    }
    println!("\n== HEAD SCAN: {scanned}/{nblocks} blocks readable, {} with truncated entry tables ==", truncated.len());
    // Pass 2: fully decompress only the blocks whose entry table did not fit in the head.
    for (n, b) in truncated.iter().enumerate() {
        if n % 20 == 0 {
            eprintln!("  full-parse {n}/{}", truncated.len());
        }
        let Ok(dec) = wad::decompress_block_index(&mut w, *b) else { continue };
        for (i, nh, th, _off, sz) in entries(&dec) {
            *type_census.entry(th).or_default() += 1;
            if th == TYPE_HUMANSTATE {
                hits.push((*b, i, nh, sz));
            }
            if th == TYPE_ANIMTABLE {
                animtable_hits.push((*b, nh, sz));
            }
        }
    }
    println!("== entry TYPE census across archive: {} distinct type hashes ==", type_census.len());
    let mut tc: Vec<_> = type_census.iter().collect();
    tc.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (th, n) in tc.iter().take(60) {
        println!("   type 0x{th:08X} x{n}");
    }
    println!("\n== SCAN RESULT ==");
    println!("type 0x{TYPE_HUMANSTATE:08X} (HumanStateTable) entries: {}", hits.len());
    for (b, i, nh, sz) in &hits {
        println!("   block {b:5} entry[{i}] name=0x{nh:08X} size={sz}");
    }
    println!("type 0x{TYPE_ANIMTABLE:08X} (AnimationTable) entries: {} (for reference)", animtable_hits.len());
    let mut at_by_name: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for (_b, nh, sz) in &animtable_hits {
        let e = at_by_name.entry(*nh).or_insert((0, 0));
        e.0 += 1;
        e.1 = *sz;
    }
    for (nh, (n, sz)) in &at_by_name {
        println!("   0x{nh:08X} x{n} size={sz}");
    }

    if quick {
        hits.push((3185, 300, TYPE_HUMANSTATE, 561024));
    }
    if hits.is_empty() {
        println!("\n!! NO HumanStateTable containers found in vz.wad");
        return;
    }

    // ---- (2/3) dump each hit ----
    let mut hs_pool: BTreeMap<u32, String> = BTreeMap::new();
    let mut hs_map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut blocks: Vec<u16> = hits.iter().map(|(b, _, _, _)| *b).collect();
    blocks.sort_unstable();
    blocks.dedup();
    for b in blocks {
        let dec = match wad::decompress_block_index(&mut w, b) {
            Ok(d) => d,
            Err(e) => {
                println!("block {b}: decompress failed: {e}");
                continue;
            }
        };
        for (i, nh, th, off, sz) in entries(&dec) {
            if th != TYPE_HUMANSTATE {
                continue;
            }
            if off + sz > dec.len() {
                println!("block {b} entry[{i}] 0x{nh:08X}: span out of range");
                continue;
            }
            let cont = &dec[off..off + sz];
            println!("\n======== HumanStateTable 0x{nh:08X} (block {b}, entry {i}, {sz} bytes) ========");
            let chs = chunks(cont);
            println!(
                "  chunks: {:?}",
                chs.iter()
                    .map(|(t, _, s)| format!("{}({s}B)", String::from_utf8_lossy(t)))
                    .collect::<Vec<_>>()
            );
            if chs.is_empty() {
                println!("  not a UCFX container; first 64 bytes: {:02X?}", &cont[..sz.min(64)]);
                continue;
            }
            // --- raw dump of the first few chunks of each distinct tag ---
            let mut seen: BTreeMap<[u8; 4], usize> = BTreeMap::new();
            for (tag, st, cz) in &chs {
                let n = seen.entry(*tag).or_default();
                if *n >= 6 {
                    continue;
                }
                *n += 1;
                let body = &cont[*st..*st + *cz];
                let ascii: String = body
                    .iter()
                    .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
                    .collect();
                println!(
                    "  RAW {} #{} ({cz}B) @0x{st:X}\n      hex: {:02X?}\n      asc: {ascii}",
                    String::from_utf8_lossy(tag),
                    *n - 1,
                    body
                );
            }
            // ===== structured SINF/AINF/TRNS decode =====
            let (pool, smap) = decode_state_machine(cont, &chs);
            hs_pool.extend(pool);
            hs_map.extend(smap);

            let t = parse_table(cont);
            println!(
                "  INFO: keyDims={} totalDims={} count={}",
                t.key_dims, t.total_dims, t.info_count
            );
            println!("  columns: {:?}", t.names);
            let Some((vs, vz)) = t.valu else {
                println!("  (no VALU chunk — not a dim table)");
                continue;
            };
            let rows = if t.total_dims > 0 { (vz / 4) / t.total_dims } else { 0 };
            println!("  VALU: {vz} bytes = {} u32 -> {rows} rows (INFO count={})", vz / 4, t.info_count);
            let get = |row: usize, ci: usize| r_u32(cont, vs + (row * t.total_dims + ci) * 4);

            // distinct values, every column
            for (ci, cname) in t.names.iter().enumerate() {
                let mut m: BTreeMap<u32, usize> = BTreeMap::new();
                for r in 0..rows {
                    *m.entry(get(r, ci)).or_default() += 1;
                }
                let mut v: Vec<_> = m.iter().collect();
                v.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
                let list: Vec<String> =
                    v.iter().take(40).map(|(val, n)| format!("{}:{n}", fmt(**val))).collect();
                let key = if ci < t.key_dims { "KEY " } else { "val " };
                println!("  {key}DISTINCT {cname} ({}): {}", m.len(), list.join(" "));
            }

            // full row dump for small tables
            if rows <= 512 {
                println!("  --- rows ---");
                for r in 0..rows {
                    let cells: Vec<String> = (0..t.total_dims)
                        .map(|c| format!("{}={}", t.names.get(c).map(|s| s.as_str()).unwrap_or("?"), fmt(get(r, c))))
                        .collect();
                    println!("   [{r:4}] {}", cells.join("  "));
                }
            }

            // ---- (4) targeted cross-checks ----
            println!("  --- cross-check probes ---");
            for (nm, want) in [
                ("Knockdown", pandemic_hash_m2("Knockdown")),
                ("Upright", pandemic_hash_m2("Upright")),
                ("Swim", pandemic_hash_m2("Swim")),
                ("*", pandemic_hash_m2("*")),
                ("unnamed-A", 0x42C9_6259u32),
                ("unnamed-B", 0xB983_2CE2u32),
                ("KnockedDown", pandemic_hash_m2("KnockedDown")),
                ("InVehicle", pandemic_hash_m2("InVehicle")),
                ("crouched", pandemic_hash_m2("crouched")),
                ("prone", pandemic_hash_m2("prone")),
                ("cower", pandemic_hash_m2("cower")),
                ("carrying", pandemic_hash_m2("carrying")),
                ("carried", pandemic_hash_m2("carried")),
                ("Subdued", pandemic_hash_m2("Subdued")),
                ("onladder", pandemic_hash_m2("onladder")),
                ("crouchcover", pandemic_hash_m2("crouchcover")),
                ("inair", pandemic_hash_m2("inair")),
                ("scuba", pandemic_hash_m2("scuba")),
                ("humanshield", pandemic_hash_m2("humanshield")),
            ] {
                let mut per_col: Vec<(String, usize)> = Vec::new();
                for (ci, cname) in t.names.iter().enumerate() {
                    let n = (0..rows).filter(|&r| get(r, ci) == want).count();
                    if n > 0 {
                        per_col.push((cname.clone(), n));
                    }
                }
                if per_col.is_empty() {
                    println!("   {nm:14} 0x{want:08X}: ABSENT");
                } else {
                    let s: Vec<String> = per_col.iter().map(|(c, n)| format!("{c}×{n}")).collect();
                    println!("   {nm:14} 0x{want:08X}: {}", s.join(", "));
                }
            }
        }
    }

    // ---- (6) relationship to the ActionTable ----
    println!("\n======== ActionTable 0x{ACTIONTABLE:08X} reference ========");
    if let Ok(dec) = wad::decompress_block_index(&mut w, 3185) {
        for (_i, nh, th, off, sz) in entries(&dec) {
            if nh != ACTIONTABLE || th != TYPE_ANIMTABLE || off + sz > dec.len() {
                continue;
            }
            let cont = &dec[off..off + sz];
            let t = parse_table(cont);
            println!("  keyDims={} totalDims={} count={}", t.key_dims, t.total_dims, t.info_count);
            println!("  columns: {:?}", t.names);
            let Some((vs, vz)) = t.valu else { continue };
            let rows = (vz / 4) / t.total_dims;
            let get = |row: usize, ci: usize| r_u32(cont, vs + (row * t.total_dims + ci) * 4);
            let nm = |v: u32| -> String {
                match hs_pool.get(&v) {
                    Some(s) => format!("0x{v:08X}({s})"),
                    None if v == NONE_SENTINEL => format!("0x{v:08X}(*)"),
                    None => format!("0x{v:08X}(?)"),
                }
            };
            for want in ["Stance", "Action"] {
                if let Some(ci) = t.names.iter().position(|n| n.eq_ignore_ascii_case(want)) {
                    let mut m: BTreeMap<u32, usize> = BTreeMap::new();
                    for r in 0..rows {
                        *m.entry(get(r, ci)).or_default() += 1;
                    }
                    let mut v: Vec<_> = m.iter().collect();
                    v.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
                    let named = v
                        .iter()
                        .filter(|(val, _)| hs_pool.contains_key(*val) || **val == NONE_SENTINEL)
                        .count();
                    println!(
                        "  DISTINCT {want} ({} distinct; {named} named from the HumanStateTable pool):",
                        m.len()
                    );
                    for (val, n) in v {
                        println!("     {} x{n}", nm(*val));
                    }
                }
            }

            // Row-level join: is every (Stance, Action) key pair in the ActionTable a state that
            // the HumanStateTable declares?
            let (cs, ca) = (
                t.names.iter().position(|n| n.eq_ignore_ascii_case("Stance")).unwrap(),
                t.names.iter().position(|n| n.eq_ignore_ascii_case("Action")).unwrap(),
            );
            let mut pairs: BTreeMap<(u32, u32), usize> = BTreeMap::new();
            for r in 0..rows {
                *pairs.entry((get(r, cs), get(r, ca))).or_default() += 1;
            }
            let (mut ok, mut wild, mut miss) = (0usize, 0usize, 0usize);
            let mut missing: Vec<(u32, u32)> = Vec::new();
            for (s, a) in pairs.keys() {
                if *s == NONE_SENTINEL || *a == NONE_SENTINEL {
                    wild += 1;
                } else if hs_map.get(s).map(|v| v.contains(a)).unwrap_or(false) {
                    ok += 1;
                } else {
                    miss += 1;
                    missing.push((*s, *a));
                }
            }
            println!(
                "\n  JOIN ActionTable(Stance,Action) -> HumanStateTable: {} distinct pairs | {ok} declared | {wild} wildcard | {miss} NOT declared",
                pairs.len()
            );
            for (s, a) in missing.iter().take(60) {
                println!("     unmatched: Stance={} Action={}", nm(*s), nm(*a));
            }

            // Reverse: how many HumanStateTable (stance, action) states have ActionTable rows?
            let mut have = 0usize;
            let mut lack = 0usize;
            let mut lack_ex: Vec<(u32, u32)> = Vec::new();
            for (s, acts) in &hs_map {
                for a in acts {
                    if pairs.contains_key(&(*s, *a)) {
                        have += 1;
                    } else {
                        lack += 1;
                        lack_ex.push((*s, *a));
                    }
                }
            }
            println!(
                "  REVERSE HumanStateTable states -> ActionTable rows: {have} have an exact row, {lack} have none"
            );
            for (s, a) in lack_ex.iter().take(20) {
                println!("     no ActionTable row: Stance={} Action={}", nm(*s), nm(*a));
            }

            // Crack the still-unnamed Action values against every ASCII run in the whole
            // resident block (26 MB) — a much broader pool than one container.
            let unknown: Vec<u32> = {
                let mut u: Vec<u32> = Vec::new();
                for r in 0..rows {
                    let a = get(r, ca);
                    if a != NONE_SENTINEL && !hs_pool.contains_key(&a) && !u.contains(&a) {
                        u.push(a);
                    }
                }
                u
            };
            println!("\n  CRACK {} still-unnamed ActionTable Action hashes vs all ASCII runs in block 3185:", unknown.len());
            let mut found: BTreeMap<u32, String> = BTreeMap::new();
            let mut p = 0usize;
            let mut runs = 0usize;
            while p < dec.len() {
                if !(0x20..0x7F).contains(&dec[p]) {
                    p += 1;
                    continue;
                }
                let s = p;
                while p < dec.len() && (0x20..0x7F).contains(&dec[p]) {
                    p += 1;
                }
                if p - s >= 3 && p - s <= 96 {
                    runs += 1;
                    let txt = std::str::from_utf8(&dec[s..p]).unwrap_or("");
                    // try the whole run and every suffix-trimmed sub-run split on non-alnum
                    for cand in std::iter::once(txt).chain(txt.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')) {
                        if cand.len() < 3 {
                            continue;
                        }
                        let h = pandemic_hash_m2(cand);
                        if unknown.contains(&h) {
                            found.entry(h).or_insert_with(|| cand.to_string());
                        }
                    }
                }
            }
            println!("     scanned {runs} ASCII runs");
            for u in &unknown {
                println!("     0x{u:08X} -> {:?}", found.get(u));
            }
        }
    }
}
