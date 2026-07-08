//! swim_clip_probe — find the swimming animation clip(s) for a merc through the real ActionTable +
//! AnimationLookup (resident block 3185). Stance/Action state names are `pandemic_hash_m2(name)`
//! (verified: m2("Upright")=0x12C07B18, m2("Fidget")=0x0C0A7FA6), so we probe swim-ish state keys and
//! print which `(Stance, Action)` resolves to a clip via `AnimSelector::resolve_state`.

use mercs2_engine::wad;
use mercs2_formats::anim_select::AnimSelector;
use mercs2_formats::hash::pandemic_hash_m2 as m2;

fn main() {
    let mut w = wad::registry_vz_wad()
        .and_then(|p| wad::open(&p).ok())
        .expect("open vz.wad");
    let dec = wad::decompress_block_index(&mut w, 3185).expect("decompress 3185");
    let sel = AnimSelector::from_resident_block(&dec).expect("AnimSelector from resident block");

    // Dump the distinct (Stance, Action) hashes so the real state names can be reverse-looked-up in the
    // rainbow table (tools/rainbow_table.json: name -> m2 hash).
    let states = sel.action_states();
    println!("== distinct (Stance, Action) hashes: {} ==", states.len());
    let mut stset = std::collections::BTreeSet::new();
    for (st, _) in &states {
        stset.insert(*st);
    }
    println!("distinct Stance hashes ({}):", stset.len());
    for st in &stset {
        print!("0x{st:08X} ");
    }
    println!("\n");

    let none = 0x27DE_7135u32; // NONE sentinel = the shared/character-agnostic key swim clips use
    let swim_stance = m2("Swim");
    println!("== Swim (action -> handle -> shared clip via cn=NONE) ==");
    println!("  m2(Idle)=0x{:08X} m2(Move)=0x{:08X} m2(Fidget)=0x{:08X}", m2("Idle"), m2("Move"), m2("Fidget"));
    for (st, ac) in &states {
        if *st != swim_stance {
            continue;
        }
        for h in sel.handles_for_state(*st, *ac) {
            if let Some(clip) = sel.resolve_handle(h, none) {
                println!("  action=0x{ac:08X} handle=0x{h:08X} clip=0x{clip:08X}");
            }
        }
    }

    // The Swim stance (m2("Swim")=0x614DB965) IS present — dump its (Action -> resolved clip) per merc.
    println!("== Swim-stance (0x{swim_stance:08X}) rows ==");
    for merc in ["mattias", "chris", "jennifer"] {
        let c = AnimSelector::character_name(merc);
        for (st, ac) in &states {
            if *st != swim_stance {
                continue;
            }
            let clip = sel.resolve_state(*st, *ac, c);
            println!("  {merc:>8}  action=0x{ac:08X} -> {clip:?}");
        }
    }

    // For each Swim-stance (action) row, get its handle and dump ALL lookup rows for that handle
    // (CharacterName, clip) — to reveal the generic character key shared swim clips resolve under.
    println!("== Swim-stance handles -> lookup (CharacterName, clip) ==");
    let mut swim_handles = std::collections::BTreeSet::new();
    for (st, ac) in &states {
        if *st == swim_stance {
            for h in sel.handles_for_state(*st, *ac) {
                swim_handles.insert(h);
            }
        }
    }
    for h in &swim_handles {
        let rows = sel.handle_clips(*h);
        if rows.is_empty() {
            continue;
        }
        print!("  handle 0x{h:08X}:");
        for (cn, clip) in rows.iter().take(6) {
            print!(" (cn=0x{cn:08X} clip=0x{clip:08X})");
        }
        println!();
    }

    // Are the 6 shared swim clips present in the merc animgroup blocks (3154 mattias / 3278 chris /
    // 3362 jennifer)? If yes, adding them to the player's `wanted` clip set will load real swim data.
    use mercs2_formats::animgroup::parse_animgroup;
    let clip_set = |w: &mut mercs2_engine::wad::Wad, blk: u16| -> std::collections::BTreeSet<u32> {
        mercs2_engine::wad::decompress_block_index(w, blk)
            .ok()
            .and_then(|d| parse_animgroup(&d).ok())
            .map(|ag| ag.clips.iter().map(|c| c.name_hash).collect())
            .unwrap_or_default()
    };
    let swim_clips = [0x52CC8375u32, 0x97C840ED, 0x64B3CC44, 0x03BD0113, 0xE0ACE9FF, 0xAEB6C9BD];
    for (merc, blk) in [("mattias", 3154u16), ("chris", 3278), ("jennifer", 3362)] {
        let set = clip_set(&mut w, blk);
        let present: Vec<String> = swim_clips
            .iter()
            .filter(|c| set.contains(c))
            .map(|c| format!("0x{c:08X}"))
            .collect();
        println!("  {merc} block {blk}: {}/{} swim clips present {present:?}", present.len(), swim_clips.len());
    }

    // Locate the shared block(s) that actually carry the swim clip animation DATA (they are not in the
    // per-merc animgroups). Scan every animgroup block for the 6 swim clip hashes.
    println!("== scanning animgroup blocks for swim clip data ==");
    for blk in mercs2_engine::wad::animgroup_blocks(&w) {
        let set = clip_set(&mut w, blk);
        let hits: Vec<String> = swim_clips.iter().filter(|c| set.contains(c)).map(|c| format!("0x{c:08X}")).collect();
        if !hits.is_empty() {
            println!("  block {blk}: {} swim clips {hits:?} (total clips {})", hits.len(), set.len());
        }
    }

    let stances = ["Swim", "Swimming", "InWater", "Water", "Wade", "Wading", "Float", "Tread"];
    let actions = ["Idle", "Move", "Fidget", "Swim", "Stroke", "Forward", "None", "Tread"];

    for merc in ["mattias", "chris", "jennifer"] {
        let c = AnimSelector::character_name(merc);
        println!("== {merc} (0x{c:08X}) ==");
        let mut any = false;
        for st in stances {
            for ac in actions {
                if let Some(clip) = sel.resolve_state(m2(st), m2(ac), c) {
                    println!("  ({st:>9}, {ac:>7}) stance=0x{:08X} action=0x{:08X} -> clip 0x{clip:08X}", m2(st), m2(ac));
                    any = true;
                }
            }
        }
        if !any {
            println!("  (no swim-ish state resolved for this merc)");
        }
    }
}
