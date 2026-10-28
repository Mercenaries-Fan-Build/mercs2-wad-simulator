//! `anim_codec_census` — every animation clip of every animgroup block, by the Havok class it is
//! stored as and whether its frames decode, plus the clips no animgroup block lists.
//!
//! What the Inspect Anims tab can play is decided by this: a clip whose frames do not decode plays
//! nothing, so the census says how many of each codec there are and which ones they are.
//!
//! ```text
//! cargo run --release -p mercs2_probe --bin anim_codec_census -- [--wad <vz.wad>]... [--list delta]
//! ```
//! `--wad` is repeatable (default: `VZ_WAD` / the registry). `--list <codec>` prints that codec's
//! clips (`interleaved`, `wavelet`, `delta`, `spline`, `unreadable`, `unlisted`). Exit 1 when a block
//! or clip does not parse — counted, named, and a failure.

use mercs2_engine::wad;
use mercs2_formats::anim::AnimType;
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut wads: Vec<String> = Vec::new();
    let mut list: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--wad" => {
                wads.push(args.get(i + 1).cloned().unwrap_or_else(|| usage("--wad needs a path")));
                i += 2;
            }
            "--list" => {
                list = Some(args.get(i + 1).cloned().unwrap_or_else(|| usage("--list needs a codec")));
                i += 2;
            }
            a => usage(&format!("unknown argument {a}")),
        }
    }
    if wads.is_empty() {
        let p = wad::resolve_vz_wad(None).unwrap_or_else(|| usage("no vz.wad found — pass --wad or set VZ_WAD"));
        wads.push(p);
    }
    let mut by_codec: BTreeMap<&'static str, BTreeSet<u32>> = BTreeMap::new();
    // Clip → its binding's bone hashes (first WAD listing it), for the track counts and the rigs.
    let mut bones_of: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut failures = 0usize;
    for path in &wads {
        let mut w = wad::open(path).unwrap_or_else(|e| usage(&format!("{path}: {e}")));
        let mut listed: BTreeSet<u32> = BTreeSet::new();
        for blk in wad::animgroup_blocks(&w) {
            let data = match wad::decompress_block_index(&mut w, blk) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("{path}: block {blk}: {e}");
                    failures += 1;
                    continue;
                }
            };
            let ag = match mercs2_formats::animgroup::parse_animgroup(&data) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("{path}: block {blk}: animgroup: {e}");
                    failures += 1;
                    continue;
                }
            };
            for c in &ag.clips {
                listed.insert(c.name_hash);
                bones_of.entry(c.name_hash).or_insert_with(|| c.binding.track_to_bone_hash.clone());
                if c.havok_len == 0 {
                    // No Havok packfile: a MANM material animation (MANM/MINF/TRCK rows).
                    by_codec.entry("manm (material animation)").or_default().insert(c.name_hash);
                    continue;
                }
                let codec = match c.packfile(&data).and_then(|pk| mercs2_formats::anim::parse_anim(pk)) {
                    Ok(clip) => match (clip.anim_type, clip.decoded) {
                        (AnimType::Interleaved, true) => "interleaved",
                        (AnimType::Wavelet, true) => "wavelet",
                        (AnimType::Wavelet, false) => "wavelet (frames not decoded)",
                        (AnimType::Delta, _) => "delta",
                        (AnimType::Spline, _) => "spline",
                        (AnimType::Interleaved, false) => "interleaved (frames not decoded)",
                    },
                    Err(e) => {
                        eprintln!("{path}: block {blk}: clip 0x{:08X}: {e}", c.name_hash);
                        "unreadable"
                    }
                };
                by_codec.entry(codec).or_default().insert(c.name_hash);
            }
        }
        // Animation ASET rows no animgroup block lists: the assets the census cannot classify.
        let rows: BTreeSet<u32> = wad::all_asets(&w)
            .into_iter()
            .filter(|(_, ty, _)| *ty == mercs2_formats::types::TYPE_ID_ANIMATION)
            .map(|(h, _, _)| h)
            .collect();
        for h in rows.difference(&listed) {
            by_codec.entry("unlisted").or_default().insert(*h);
        }
    }
    println!("=== ANIMATION CODEC CENSUS ({} wad(s)) ===", wads.len());
    for (codec, set) in &by_codec {
        println!("  {codec:<34} {:>6}", set.len());
    }
    // Transform-track counts (the binding's length).
    let mut tracks: BTreeMap<&'static str, usize> = BTreeMap::new();
    for b in bones_of.values() {
        let bucket = match b.len() {
            0 => "0 tracks",
            1 => "1 track",
            2..=19 => "2-19 tracks",
            20..=59 => "20-59 tracks",
            60..=105 => "60-105 tracks",
            _ => "over 105 tracks",
        };
        *tracks.entry(bucket).or_default() += 1;
    }
    println!("=== BY TRACK COUNT ({} listed clips, longest {}) ===", bones_of.len(), bones_of.values().map(Vec::len).max().unwrap_or(0));
    for (b, n) in &tracks {
        println!("  {b:<34} {n:>6}");
    }
    // Models some clip drives: a model of the first WAD whose HIER holds a bone a clip's binding
    // names, split by whether that clip has 20+ tracks (a body clip) or fewer (a part clip).
    let mut w0 = wad::open(&wads[0]).unwrap_or_else(|e| usage(&format!("{}: {e}", wads[0])));
    let all_bones: BTreeSet<u32> = bones_of.values().flatten().copied().collect();
    let part_bones: BTreeSet<u32> = bones_of.values().filter(|b| b.len() < 20).flatten().copied().collect();
    let (mut rigged, mut driven, mut part_driven) = (0usize, 0usize, 0usize);
    wad::for_each_model_hier(&mut w0, |_, _| {}, |_, read| {
        let Ok((_, hier)) = read else { return };
        if hier.len() < 2 {
            return;
        }
        rigged += 1;
        if hier.iter().any(|n| all_bones.contains(&n.hash)) {
            driven += 1;
        }
        if hier.iter().any(|n| part_bones.contains(&n.hash)) {
            part_driven += 1;
        }
    });
    println!("=== RIGS ({}) ===", wads[0]);
    println!("  models with 2+ HIER nodes           {rigged:>6}");
    println!("  ... a clip's binding names a node   {driven:>6}");
    println!("  ... a <20-track clip names a node   {part_driven:>6}");
    if let Some(want) = list {
        let set = by_codec.iter().find(|(k, _)| k.starts_with(want.as_str())).map(|(_, v)| v);
        match set {
            Some(s) => {
                for h in s {
                    println!("0x{h:08X}");
                }
            }
            None => println!("(no {want} clips)"),
        }
    }
    let unreadable = by_codec.get("unreadable").map_or(0, |s| s.len());
    if failures + unreadable > 0 {
        eprintln!("{failures} block(s) and {unreadable} clip(s) did not parse");
        std::process::exit(1);
    }
}

fn usage(msg: &str) -> ! {
    eprintln!("anim_codec_census: {msg}\nusage: anim_codec_census [--wad <path>]... [--list <codec>]");
    std::process::exit(2)
}
