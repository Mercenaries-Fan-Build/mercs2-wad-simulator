//! Settle what an ASET row's `secondary_ref` / `packed_block_ref` actually encode.
//!
//! Two independent investigations disagreed about where a model's finer-LOD block index lives:
//!   H1  `packed_block_ref` LOW16   (docs/aset_format.md calls this "a sub-entry offset within
//!                                    the block", marked Verified: Yes)
//!   H2  `secondary_ref`   HI16
//!
//! Both cannot be right, and a whole documentation cull is queued behind the answer, so decide it
//! by measurement rather than by preferring an author. Two tests:
//!
//!   1. NAMED GROUND TRUTH — `civ_hum_beachfemale_a` demonstrably carries geometry in blocks 2110
//!      (`c32143_P000_Q3`) and 4587 (`c32143-c21152_P001_Q2`). Whichever field yields 4587 wins.
//!      `pmc_hum_mattias` is the negative control: one block, so the field must read "none".
//!
//!   2. WHOLE-WAD PARTITION — over every row, does the candidate field ALWAYS name a block whose
//!      path is one LOD level finer than the primary's, and never anything else? A field that is
//!      really an intra-block ordinal would point at arbitrary block indices.
//!
//! Usage: aset_decode [--wad game-files/vz.wad] [<model-name-or-0xhash> ...]

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use std::collections::BTreeMap;
use std::fs::File;

const MODEL: u32 = 19;
const TEXTURE: u32 = 27;

fn parse_hash(s: &str) -> u32 {
    s.strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| pandemic_hash_m2(s))
}

/// `_P00N` level parsed out of a block path, if present.
fn lod_level(path: &str) -> Option<u32> {
    let i = path.find("_P00")?;
    path[i + 4..].chars().next()?.to_digit(10)
}

/// Cell stem: `c32143-c21152_P001_Q2.block` -> `c32143`, so a finer rung can be tied to its parent.
fn stem(path: &str) -> &str {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let base = base.split("_P00").next().unwrap_or(base);
    base.split('-').next().unwrap_or(base)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut wad = "game-files/vz.wad".to_string();
    let mut names: Vec<String> = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--wad" { wad = it.next().unwrap_or(wad); } else { names.push(a); }
    }
    if names.is_empty() {
        names = vec!["civ_hum_beachfemale_a".into(), "pmc_hum_mattias".into()];
    }

    let mut f = File::open(&wad).unwrap_or_else(|e| panic!("open {wad}: {e}"));
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).unwrap_or_else(|e| panic!("parse {wad}: {e}"));
    let path_of = |i: usize| ar.paths.get(i).map(|s| s.as_str()).unwrap_or("<none>");

    println!("== {wad}: {} aset rows, {} paths, endian {:?}", ar.aset.len(), ar.paths.len(), ar.endian);
    // Sanity gate: type_id is a small discriminator (0..35). If the rows come through with values
    // like 0x13000000 the archive was parsed with the wrong endianness, and EVERY conclusion below
    // would be drawn from byte-swapped garbage. Say so loudly rather than reporting zeros.
    let bogus = ar.aset.iter().filter(|e| e.type_id > 64).count();
    if bogus > ar.aset.len() / 2 {
        println!(
            "   !! {bogus}/{} rows have type_id > 64 (e.g. 0x{:08X}) — these rows are byte-swapped.\n\
             \x20     Endianness detection failed for this WAD; the measurements below are MEANINGLESS.",
            ar.aset.len(),
            ar.aset.iter().find(|e| e.type_id > 64).map(|e| e.type_id).unwrap_or(0),
        );
    }
    println!();

    // ---- 1. named ground truth -------------------------------------------------------------
    for n in &names {
        let h = parse_hash(n);
        let rows: Vec<_> = ar.aset.iter().filter(|e| e.asset_hash == h).collect();
        println!("-- {n} (0x{h:08X}): {} row(s)", rows.len());
        for e in rows {
            let blk = (e.packed_block_ref >> 16) as usize;
            let low16 = (e.packed_block_ref & 0xFFFF) as u16;
            let sec_hi = (e.secondary_ref >> 16) as u16;
            let sec_lo = (e.secondary_ref & 0xFFFF) as u16;
            println!("   raw: secondary_ref=0x{:08X}  packed_block_ref=0x{:08X}  type={}",
                e.secondary_ref, e.packed_block_ref, e.type_id);
            println!("   primary block {blk:5}  {}", path_of(blk));
            println!("   H1 packed.low16 = {low16:5} (0x{low16:04X})  -> {}",
                if low16 == 0xFFFF { "SENTINEL (none)".into() } else { format!("{}", path_of(low16 as usize)) });
            println!("   H2 secondary.hi16 = {sec_hi:5} (0x{sec_hi:04X}) lo16={sec_lo:5}  -> {}",
                if e.secondary_ref == 0xFFFF_FFFF { "SENTINEL (none)".into() } else { format!("{}", path_of(sec_hi as usize)) });
        }
        println!();
    }

    // ---- 2. whole-WAD partition ------------------------------------------------------------
    // Which block indices are ever named by each candidate field, and what LOD level are they?
    let mut h1_levels: BTreeMap<Option<u32>, usize> = BTreeMap::new();
    let mut h2_levels: BTreeMap<Option<u32>, usize> = BTreeMap::new();
    let mut primary_levels: BTreeMap<Option<u32>, usize> = BTreeMap::new();
    let (mut h1_n, mut h2_n) = (0usize, 0usize);
    let (mut h1_stem_ok, mut h1_finer_ok) = (0usize, 0usize);
    let (mut h2_stem_ok, mut h2_finer_ok) = (0usize, 0usize);
    let mut h1_oob = 0usize;
    let mut h2_oob = 0usize;

    for e in ar.aset.iter().filter(|e| e.type_id == MODEL || e.type_id == TEXTURE) {
        let blk = (e.packed_block_ref >> 16) as usize;
        let pl = lod_level(path_of(blk));
        *primary_levels.entry(pl).or_default() += 1;

        let low16 = (e.packed_block_ref & 0xFFFF) as usize;
        if low16 != 0xFFFF {
            h1_n += 1;
            if low16 >= ar.paths.len() { h1_oob += 1; } else {
                *h1_levels.entry(lod_level(path_of(low16))).or_default() += 1;
                if stem(path_of(low16)) == stem(path_of(blk)) { h1_stem_ok += 1; }
                if let (Some(a), Some(b)) = (pl, lod_level(path_of(low16))) {
                    if b == a + 1 { h1_finer_ok += 1; }
                }
            }
        }
        if e.secondary_ref != 0xFFFF_FFFF {
            h2_n += 1;
            let hi = (e.secondary_ref >> 16) as usize;
            if hi >= ar.paths.len() { h2_oob += 1; } else {
                *h2_levels.entry(lod_level(path_of(hi))).or_default() += 1;
                if stem(path_of(hi)) == stem(path_of(blk)) { h2_stem_ok += 1; }
                if let (Some(a), Some(b)) = (pl, lod_level(path_of(hi))) {
                    if b == a + 1 { h2_finer_ok += 1; }
                }
            }
        }
    }

    let pct = |a: usize, b: usize| if b == 0 { 0.0 } else { 100.0 * a as f64 / b as f64 };
    println!("== whole-WAD test over model(19)+texture(27) rows");
    println!("   primary block LOD levels: {primary_levels:?}");
    println!("\n   H1  packed_block_ref LOW16, non-sentinel rows: {h1_n}   out-of-range: {h1_oob}");
    println!("       LOD level of the named block: {h1_levels:?}");
    println!("       same cell stem as primary: {h1_stem_ok} ({:.1}%)", pct(h1_stem_ok, h1_n));
    println!("       exactly one level finer:   {h1_finer_ok} ({:.1}%)", pct(h1_finer_ok, h1_n));
    println!("\n   H2  secondary_ref HI16,      non-sentinel rows: {h2_n}   out-of-range: {h2_oob}");
    println!("       LOD level of the named block: {h2_levels:?}");
    println!("       same cell stem as primary: {h2_stem_ok} ({:.1}%)", pct(h2_stem_ok, h2_n));
    println!("       exactly one level finer:   {h2_finer_ok} ({:.1}%)", pct(h2_finer_ok, h2_n));
    // H3: what does secondary_ref LOW16 hold when the word is non-sentinel? If a row really
    // encodes a CHAIN, this should be the next rung again (_P003) rather than a flag.
    let mut h3_levels: BTreeMap<Option<u32>, usize> = BTreeMap::new();
    let (mut h3_n, mut h3_sent, mut h3_stem_ok) = (0usize, 0usize, 0usize);
    for e in ar.aset.iter().filter(|e| e.type_id == MODEL || e.type_id == TEXTURE) {
        if e.secondary_ref == 0xFFFF_FFFF { continue; }
        let lo = (e.secondary_ref & 0xFFFF) as usize;
        if lo == 0xFFFF { h3_sent += 1; continue; }
        h3_n += 1;
        if lo < ar.paths.len() {
            *h3_levels.entry(lod_level(path_of(lo))).or_default() += 1;
            let blk = (e.packed_block_ref >> 16) as usize;
            if stem(path_of(lo)) == stem(path_of(blk)) { h3_stem_ok += 1; }
        }
    }
    println!("\n   H3  secondary_ref LOW16 (of non-sentinel rows): {h3_n} real, {h3_sent} sentinel");
    println!("       LOD level of the named block: {h3_levels:?}");
    println!("       same cell stem as primary: {h3_stem_ok}");

    println!("\n   A field that really encodes a finer-LOD BLOCK should be ~100% same-stem and");
    println!("   ~100% one-level-finer. An intra-block ordinal should be neither.");
}
