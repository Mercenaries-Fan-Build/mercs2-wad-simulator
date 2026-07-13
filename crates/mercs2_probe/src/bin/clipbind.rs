//! What skeleton is a clip actually authored against?
//!
//! Mattias's AnimationLookup rows name 100 clips, but 31 of them bind ZERO of their 50 transform
//! tracks to `pmc_hum_mattias_v3`'s HIER — the export records them as unbound rather than shipping a
//! dead T-pose. This prints a clip's `trnm` track->bone-hash binding with names reversed from the
//! rainbow table, so the rig it DOES belong to is identifiable instead of a mystery.
//!
//! usage: clipbind <0xCLIPHASH> [0xMODELHASH]   (model defaults to pmc_hum_mattias_v3)

use mercs2_engine::{model::Model, wad};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let hx = |s: &Option<&String>| {
        s.and_then(|a| a.strip_prefix("0x")).and_then(|h| u32::from_str_radix(h, 16).ok())
    };
    let clip_hash = hx(&args.get(1)).unwrap_or(0x8EE5_BA8B);
    let mhash = hx(&args.get(2)).unwrap_or(0xA3C1_FABC);
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");

    let m = Model::load(&mut w, mhash).expect("load model");
    let (_v, _i, _d, stats) = m.flatten();
    let hier: Vec<u32> = stats.rig.iter().map(|b| b.name_hash).collect();
    let hier_set: std::collections::HashSet<u32> = hier.iter().copied().collect();

    // Find the clip in the WAD's animgroup blocks and read its trnm binding.
    let mut found = None;
    for blk in wad::animgroup_blocks(&mut w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = mercs2_formats::animgroup::parse_animgroup(&data) else { continue };
        if ag.clips.iter().any(|c| c.name_hash == clip_hash) {
            found = Some((blk, ag));
            break;
        }
    }
    let Some((blk, ag)) = found else {
        return println!("clip 0x{clip_hash:08X}: not found in any animgroup block");
    };

    // Census of the WHOLE block: a clip with an empty `trnm` can only be interpreted against what
    // its SIBLINGS in the same block bind to, so print that distribution before concluding anything.
    let mut census: std::collections::BTreeMap<(u32, usize), usize> = std::collections::BTreeMap::new();
    for c in &ag.clips {
        *census.entry((c.num_transform_tracks, c.binding.track_to_bone_hash.len())).or_insert(0) += 1;
    }
    println!("block {blk}: {} clips — (transform_tracks, trnm_len) census:", ag.clips.len());
    for ((ntt, tl), n) in &census {
        println!("   {n:>3} clips: {ntt:>3} tracks, trnm {tl:>3} {}", if *tl == 0 { "  <-- NO BINDING" } else { "" });
    }
    let primary = ag.binding.as_ref().map(|b| b.track_to_bone_hash.len()).unwrap_or(0);
    println!("block primary binding (widest clip's trnm): {primary} bones\n");

    // Is the 50-track rig's binding present in the block but UNCONSUMED (e.g. a block-level `trnm`
    // the per-clip reader never looks at)? Count the raw `trnm` tags and compare with the number of
    // clips that actually got one. Equal => the binding is genuinely not in the data.
    let raw = wad::decompress_block_index(&mut w, blk).expect("block");
    let tags = raw.windows(4).filter(|x| *x == b"trnm").count();
    let with_binding = ag.clips.iter().filter(|c| !c.binding.track_to_bone_hash.is_empty()).count();
    println!(
        "raw `trnm` tags in block: {tags}   clips that parsed one: {with_binding}   -> {}",
        if tags > with_binding {
            "UNCONSUMED binding chunk(s) exist — the mapping IS in the data"
        } else {
            "no spare binding — the 50-track clips ship no trnm at all"
        }
    );

    // Dump this clip's OWN container descriptor rows — the reader scans exactly these, so whatever
    // makes it miss the `trnm` is visible here.
    {
        let rd = |b: &[u8], o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let n = rd(&raw, 0) as usize;
        let mut pos = 4 + n * 16;
        for i in 0..n {
            let e = 4 + i * 16;
            let (nh, th, sz) = (rd(&raw, e), rd(&raw, e + 4), rd(&raw, e + 12) as usize);
            if pos + sz > raw.len() {
                break;
            }
            if nh == clip_hash && th == mercs2_formats::types::TYPE_HASH_ANIMATION {
                let c = &raw[pos..pos + sz];
                println!("\nclip container: {} bytes, magic {:?}", sz, String::from_utf8_lossy(&c[0..4]));
                let data_off = rd(c, 4);
                let ndesc = rd(c, 16) as usize;
                let maxd = c.len().saturating_sub(20) / 20;
                println!("  data_area_off={data_off}  n_desc={ndesc}  (max rows that fit: {maxd}) {}",
                    if ndesc > maxd { "<-- n_desc > max_desc: THE READER BAILS HERE" } else { "" });
                for r in 0..ndesc.min(maxd).min(12) {
                    let ro = 20 + r * 20;
                    let tag = String::from_utf8_lossy(&c[ro..ro + 4]).to_string();
                    let (u0, size) = (rd(c, ro + 4), rd(c, ro + 8));
                    println!("    row {r}: {tag:<6} u0={u0:<12} size={size:<8}{}",
                        if u0 == 0xFFFF_FFFF { "  (container marker -> reader SKIPS this row)" } else { "" });
                    if &c[ro..ro + 4] == b"trnm" && u0 != 0xFFFF_FFFF {
                        // The body the reader parses as [u32 count][u32 leading][count x u32 hash].
                        let s = data_off as usize + u0 as usize;
                        let body = &c[s..(s + size as usize).min(c.len())];
                        println!("      trnm body: size={} => implies count {} if size==8+4*count",
                            body.len(), (body.len().saturating_sub(8)) / 4);
                        println!("      first 6 words: {:?}",
                            (0..6).map(|k| format!("0x{:08X}", rd(body, k * 4))).collect::<Vec<_>>());
                        println!("      READER READS count = {} (word 0)", rd(body, 0));
                    }
                }
                break;
            }
            pos += sz;
        }
    }

    let c = ag.clips.iter().find(|c| c.name_hash == clip_hash).unwrap().clone();

    let bones = &c.binding.track_to_bone_hash;
    let names = mercs2_engine::worldutil::rainbow_names(&bones.iter().copied().collect());
    let resolved = bones.iter().filter(|b| hier_set.contains(b)).count();

    println!(
        "clip 0x{clip_hash:08X} (block {blk}, class {}): {} transform tracks, {} bone bindings",
        c.class, c.num_transform_tracks, bones.len()
    );
    println!(
        "vs model 0x{mhash:08X} HIER ({} bones): {resolved} of {} track bones resolve\n",
        hier.len(),
        bones.len()
    );
    println!("track bones (name from rainbow table; '*' = present in this model's HIER):");
    for (i, b) in bones.iter().enumerate().take(24) {
        let mark = if hier_set.contains(b) { '*' } else { ' ' };
        println!("  {mark} [{i:>3}] 0x{b:08X}  {}", names.get(b).map(String::as_str).unwrap_or("<unnamed>"));
    }
    if bones.len() > 24 {
        println!("  ... {} more", bones.len() - 24);
    }
}
