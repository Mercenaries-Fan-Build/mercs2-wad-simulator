//! `clipbones` — which animation CLIPS drive a given set of bone name-hashes.
//!
//! The inverse of `clipbind`: instead of "which bones does this clip drive", ask
//! "which clips touch THESE bones, and what are those clips called". A clip's name
//! usually says what it animates, so an unnamed bone driven only by (say) the facial
//! or a prop clip is identified by the company it keeps.
//!
//! ```text
//! cargo run -p mercs2_probe --bin clipbones -- 0xE6FF1D72 0x491E8967 ...
//! ```

use mercs2_engine::wad;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let targets: Vec<u32> = args
        .iter()
        .filter_map(|a| a.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok()))
        .collect();
    if targets.is_empty() {
        eprintln!("usage: clipbones 0xHASH [0xHASH ...]");
        std::process::exit(2);
    }
    let path = wad::registry_vz_wad().unwrap_or_else(|| "game-files/vz.wad".into());
    let mut w = wad::open(&path).expect("open vz.wad");

    // `--order 0xCLIP`: dump that clip's ENTIRE track->bone binding IN TRACK ORDER.
    // Track order follows the authored skeleton, so an unnamed bone is pinned
    // anatomically by the named bones on either side of it.
    if let Some(i) = std::env::args().position(|a| a == "--order") {
        let clip = std::env::args()
            .nth(i + 1)
            .and_then(|s| s.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok()))
            .expect("--order 0xCLIPHASH");
        for blk in wad::animgroup_blocks(&w) {
            let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
            let Ok(ag) = mercs2_formats::animgroup::parse_animgroup(&data) else { continue };
            let Some(c) = ag.clips.iter().find(|c| c.name_hash == clip) else { continue };
            let all: std::collections::BTreeSet<u32> =
                c.binding.track_to_bone_hash.iter().copied().collect();
            let nm = mercs2_engine::worldutil::rainbow_names(&all);
            println!("clip 0x{clip:08X} in block {blk}: {} tracks", c.binding.track_to_bone_hash.len());
            for (t, h) in c.binding.track_to_bone_hash.iter().enumerate() {
                let n = nm.get(h).map(|s| s.as_str()).unwrap_or("******** UNNAMED ********");
                println!("  track {t:>3}  0x{h:08X}  {n}");
            }
            return;
        }
        eprintln!("clip 0x{clip:08X} not found");
        return;
    }

    let names = {
        let set: std::collections::BTreeSet<u32> = targets.iter().copied().collect();
        mercs2_engine::worldutil::rainbow_names(&set)
    };

    for blk in wad::animgroup_blocks(&w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = mercs2_formats::animgroup::parse_animgroup(&data) else { continue };

        // Only report animgroups that actually bind one of the targets.
        let mut any = false;
        for c in &ag.clips {
            let hit: Vec<u32> = targets
                .iter()
                .copied()
                .filter(|t| c.binding.track_to_bone_hash.contains(t))
                .collect();
            if hit.is_empty() {
                continue;
            }
            if !any {
                let p = wad::block_paths(&w).get(blk as usize).cloned().unwrap_or_default();
                println!("\n=== block {blk}  {p}");
                any = true;
            }
            let clip_set: std::collections::BTreeSet<u32> = std::iter::once(c.name_hash).collect();
            let cn = mercs2_engine::worldutil::rainbow_names(&clip_set);
            let cname = cn.get(&c.name_hash).cloned().unwrap_or_else(|| "?".into());
            println!(
                "  clip 0x{:08X} {:<34} tracks={:<4} class={:?}  binds {} of the targets:",
                c.name_hash,
                cname,
                c.num_transform_tracks,
                c.class,
                hit.len()
            );
            for t in hit {
                let n = names.get(&t).map(|s| s.as_str()).unwrap_or("** UNNAMED **");
                println!("      0x{t:08X} {n}");
            }
        }
    }
}
