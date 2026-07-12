//! Extract per-line VO for EVERY language from the console (big-endian) build.
//!
//! ## Why this exists next to `vo_extract`
//!
//! On PC the dialogue is EMBEDDED in `English.wad` (461 MB) and the `.pws` is barely used —
//! English.wad's wavebanks address only ~1.4 MB of a 798 MB stream. There is no
//! `French.wad`/`German.wad` on PC, so the PC build cannot yield the other languages at all.
//!
//! The console build streams instead of embedding, which is why `game-files/audio/` has BOTH:
//!
//!   `<LANG>.WAD`              big-endian (`SCFF`), ~60-70 MB — the INDEX only
//!   `VO_STREAM.<LANG>.PWS`    ~330-390 MB — the audio, an MP3 bitstream (`FF FA ..`,
//!                             MPEG-1 Layer III, mono 44.1 kHz)
//!
//! A `.pws` is a headerless blob store: no index, no per-blob header. So the language WAD's
//! wavebank records — `(data_offset, data_size)` — are the ONLY way to cut the stream into
//! individual lines. Each carved blob is a standalone, playable MP3.
//!
//! (`xbox-vz.wad` is NOT the index: it has zero `vo_` paths and an ASET type histogram
//! identical to PC `vz.wad` — it is the game WAD, not the speech WAD.)
//!
//! BE plumbing is reused from `mercs2_formats::dlc_input` (the Xbox-360 DLC reader). The UCFX
//! walk and wavebank record read are BE variants of the LE ones, hand-rolled here.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use clap::Parser;

use mercs2_formats::dlc_input::{
    decompress_be_sges, parse_be_aset, parse_be_ffcs, parse_be_indx, parse_be_pths, PAGE_SIZE,
};

/// ASET type id for a wavebank (type_id is stored LE even inside the BE container).
const TYPE_WAVEBANK: u32 = 6;
/// pandemic_hash_m2("wavebank") — the UCFX container type.
const TH_WAVEBANK: u32 = 0xF753_F6D0;

#[derive(Parser)]
#[command(about = "Extract per-line VO for every language (console BE WADs + .pws streams)")]
struct Cli {
    /// Directory holding `<LANG>.WAD` (the index) beside `VO_STREAM.<LANG>.PWS` (the audio).
    #[arg(long, default_value = "game-files/audio")]
    audios: PathBuf,

    #[arg(long, default_value = "output/vo_lang")]
    out: PathBuf,

    /// Recon only: report each language's banks and how much of its stream they address.
    #[arg(long)]
    list: bool,

    /// Dump N raw codec-0x0C blobs (.bin) + their stats, so the codec can be identified.
    #[arg(long, default_value_t = 0)]
    dump_raw: usize,
}

fn be32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// BE UCFX chunk-body fetch — mirrors `mercs2_formats::ucfx::extract_chunk_body` with BE reads.
/// The console may also store the magic byte-reversed, so accept `UCFX` or `XFCU`.
fn be_chunk_body(container: &[u8], tag: &[u8; 4]) -> Option<Vec<u8>> {
    if container.len() < 20 {
        return None;
    }
    let magic = &container[0..4];
    if magic != b"UCFX" && magic != b"XFCU" {
        return None;
    }
    let rev = magic == b"XFCU";
    let data_area_off = be32(container, 4) as usize;
    let n_desc = be32(container, 16) as usize;
    if n_desc > container.len().saturating_sub(20) / 20 {
        return None;
    }
    let mut want = *tag;
    if rev {
        want.reverse();
    }
    for i in 0..n_desc {
        let row = 20 + i * 20;
        if row + 20 > container.len() {
            break;
        }
        if container[row..row + 4] != want[..] {
            continue;
        }
        let u0 = be32(container, row + 4) as usize;
        let body_size = be32(container, row + 8) as usize;
        if u0 == 0xFFFF_FFFF {
            continue;
        }
        let start = if data_area_off > 0 { data_area_off + u0 } else { 8 + u0 };
        let end = start.checked_add(body_size)?;
        if end > container.len() {
            return None;
        }
        return Some(container[start..end].to_vec());
    }
    None
}

/// One wavebank clip. Same 36-byte record as PC, same corrected field map (see `vo_extract`):
/// +00 hash, +05 channels, +06 bytes/sample, +08 rate, +12 byte size, +16 samples, +32 offset.
struct ClipRec {
    clip_hash: u32,
    data_size: u32,
    data_offset: u32,
    /// fmt dword @+4 is raw bytes (NOT endian-swapped): [_, channels, codec, _].
    channels: u8,
    codec: u8,
    sample_rate: u32,
    /// The competing candidates, kept so `--list` can decide the layout from the data
    /// instead of from anyone's assumption.
    f12: u32,
    f16: u32,
    f32: u32,
}

fn parse_records_be(body: &[u8]) -> Vec<ClipRec> {
    let mut out = Vec::new();
    if body.len() < 24 {
        return out;
    }
    // ★ `count` @+0 is stored LITTLE-endian even inside the big-endian body — the same quirk as
    // the ASET type_id. Reading it BE yields 486,539,264 (= 0x1D000000), whose byte-reverse is
    // 0x1D = 29, the real bank size. Getting this wrong invents thousands of phantom clips.
    // (Authority: ucfx_byteswap::audio::convert_wavebank_data, a port of the tested converter.)
    let count = u32::from_le_bytes([body[0], body[1], body[2], body[3]]) as usize;
    let populated = u16::from_be_bytes([body[8], body[9]]) as usize;
    let records_off = be32(body, 16) as usize;
    if count > 10_000 || records_off > body.len() {
        return out;
    }
    let pop = if populated > 0 { populated.min(count) } else { count };

    for i in 0..pop {
        let roff = records_off + i * 36;
        if roff + 36 > body.len() {
            break;
        }
        let f12 = be32(body, roff + 12);
        let f16 = be32(body, roff + 16);
        let f32 = be32(body, roff + 32);
        let rec = ClipRec {
            clip_hash: be32(body, roff),
            // BODY-relative, and the same map as PC: offset @+32, byte size @+12.
            data_offset: f32,
            data_size: f12,
            channels: { let c = body[roff + 5]; if c == 0 { 1 } else { c } },
            codec: body[roff + 6],
            sample_rate: be32(body, roff + 8),
            f12,
            f16,
            f32,
        };
        if rec.clip_hash == 0 && f12 == 0 && f16 == 0 {
            continue;
        }
        out.push(rec);
    }
    out
}

/// Decide the record layout from the DATA, not from an assumption.
///
/// Two rival models are on the table:
///   A (ucfx_byteswap's converter): offset=+12, size=+16
///   B (derived from PC, `vo_extract`): offset=+32, size=+12, samples=+16
///
/// If clips are packed back-to-back in the stream then, sorted by the TRUE offset, each
/// consecutive delta equals the TRUE size. That is a hard constraint neither model can fake.
/// MP3 gives a second, independent check: every blob must begin on a frame sync (0xFFEx).
fn analyze_layout(banks: &[(String, Vec<ClipRec>)], pws: &mut File, pws_len: u64) {
    for (name, off_get, size_get) in [
        ("A: off=+12 size=+16", 0usize, 1usize),
        ("B: off=+32 size=+12", 2usize, 2usize),
    ] {
        let mut rows: Vec<(u64, u64)> = Vec::new();
        for (_, cs) in banks {
            for c in cs {
                let off = match off_get {
                    0 => c.f12,
                    _ => c.f32,
                } as u64;
                let size = match size_get {
                    1 => c.f16,
                    _ => c.f12,
                } as u64;
                if size > 0 {
                    rows.push((off, size));
                }
            }
        }
        rows.sort();
        rows.dedup();

        let mut overlaps = 0usize;
        let mut delta_matches = 0usize;
        let mut prev_end = 0u64;
        for w in rows.windows(2) {
            let (o, s) = w[0];
            let (n, _) = w[1];
            if o < prev_end {
                overlaps += 1;
            }
            // packed => next offset == this offset + this size
            if n.abs_diff(o + s) <= 16 {
                delta_matches += 1;
            }
            prev_end = o + s;
        }
        let total: u64 = rows.iter().map(|r| r.1).sum();

        // Independent check: does each candidate offset land on an MP3 frame sync?
        let mut synced = 0usize;
        let mut checked = 0usize;
        for (o, _) in rows.iter().take(400) {
            if *o + 2 > pws_len {
                continue;
            }
            let mut b = [0u8; 2];
            if pws.seek(SeekFrom::Start(*o)).is_ok() && pws.read_exact(&mut b).is_ok() {
                checked += 1;
                if b[0] == 0xFF && (b[1] & 0xE0) == 0xE0 {
                    synced += 1;
                }
            }
        }
        println!(
            "    {name}: {} clips, overlaps={overlaps}, packed-deltas={delta_matches}/{}, \
             sum={:.0} MB (stream {:.0} MB), sync={synced}/{checked}",
            rows.len(),
            rows.len().saturating_sub(1),
            total as f64 / 1e6,
            pws_len as f64 / 1e6,
        );
    }
}

/// Which half of the ASET `u2` holds the block index?
///
/// `dlc_input::AsetEntry::block_index()` returns the HIGH 16 bits (the PC/LE convention:
/// `{block:hi16, sub:lo16}`). The console language WADs store it the other way round — observed
/// `u2 = 0xFFFF001B`, i.e. hi16 = 0xFFFF (the resolve-by-hash sub sentinel) and lo16 = block 27,
/// which is in range while 65535 is not. Rather than hard-code either convention, pick the half
/// that is actually a valid block index and treat 0xFFFF as the sentinel it is.
fn block_of(u2: u32, n_blocks: usize) -> Option<u16> {
    let hi = ((u2 >> 16) & 0xFFFF) as u16;
    let lo = (u2 & 0xFFFF) as u16;
    for cand in [hi, lo] {
        if cand != 0xFFFF && (cand as usize) < n_blocks {
            return Some(cand);
        }
    }
    None
}

/// MPEG-1 Layer III bitrate table (kbps), indexed by the header's 4-bit bitrate index.
const MP3_BITRATE: [u32; 16] =
    [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
/// MPEG-1 sample rates, indexed by the header's 2-bit rate index.
const MP3_RATE: [u32; 4] = [44100, 48000, 32000, 0];

/// Length of the MPEG-1 Layer III frame starting at `p`, or None if that isn't a valid header.
fn mp3_frame_len(b: &[u8], p: usize) -> Option<usize> {
    if p + 4 > b.len() {
        return None;
    }
    // sync (11 bits) + MPEG-1 (0b11) + Layer III (0b01)
    if b[p] != 0xFF || (b[p + 1] & 0xE0) != 0xE0 {
        return None;
    }
    if (b[p + 1] >> 3) & 0x03 != 0b11 {
        return None; // not MPEG-1
    }
    if (b[p + 1] >> 1) & 0x03 != 0b01 {
        return None; // not Layer III
    }
    let br = MP3_BITRATE[((b[p + 2] >> 4) & 0x0F) as usize];
    let sr = MP3_RATE[((b[p + 2] >> 2) & 0x03) as usize];
    if br == 0 || sr == 0 {
        return None;
    }
    let pad = ((b[p + 2] >> 1) & 1) as usize;
    Some((144 * br as usize * 1000) / sr as usize + pad)
}

/// Carve the MP3 streams out of a bank body by WALKING FRAMES.
///
/// The record offsets do not reliably land on a frame start (only ~15% do), but MP3 is
/// self-delimiting: a valid frame header states its own length, so a run of back-to-back valid
/// frames IS a clip, and the zero padding between clips ends each run. This recovers the exact
/// clip boundaries without trusting the offsets at all -- and the number of runs it finds is an
/// independent check against the bank's record count.
fn mp3_chains(body: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 4 <= body.len() {
        let Some(len) = mp3_frame_len(body, p) else {
            p += 1;
            continue;
        };
        // Walk as long as frames chain head-to-tail.
        let start = p;
        let mut q = p;
        let mut frames = 0usize;
        while let Some(l) = mp3_frame_len(body, q) {
            if q + l > body.len() {
                break;
            }
            q += l;
            frames += 1;
        }
        // A real line is many frames; a chance 0xFFEx pair is one or two.
        if frames >= 8 {
            out.push((start, q));
            p = q;
        } else {
            p = start + len.max(1);
        }
    }
    out
}

fn safe(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect()
}

/// Parse one big-endian language WAD into its VO wavebanks: `(scene label, clips)`.
/// A scene bank: its label, its decompressed wavebank BODY, and its clips.
///
/// ★ The console language WAD EMBEDS the audio, just as the PC WAD does. The `+32` offsets land
/// inside the body (e.g. [1128 .. 1,507,832] in a 1,591,552-byte body) and the `+12` sizes tile
/// it. So the record layout is the SAME as PC (offset=+32, size=+12) — it is BODY-relative, not
/// stream-relative. The `.pws` is not needed for these lines.
type Bank = (String, Vec<u8>, Vec<ClipRec>);

fn index_language(
    wad: &Path,
    verbose: bool,
) -> Result<Vec<Bank>, Box<dyn std::error::Error>> {
    let mut f = File::open(wad)?;
    let file_len = f.metadata()?.len();

    let mut hdr = vec![0u8; 0xD8];
    f.read_exact(&mut hdr)?;
    let (_v, rows) = parse_be_ffcs(&hdr)?;
    let row_of = |tag: &str| rows.iter().find(|r| r.tag == tag).cloned();
    let indx_row = row_of("INDX").ok_or("no INDX")?;
    let aset_row = row_of("ASET").ok_or("no ASET")?;
    let pths_row = row_of("PTHS").ok_or("no PTHS")?;

    let mut read_at = |off: u64, len: usize| -> std::io::Result<Vec<u8>> {
        let mut b = vec![0u8; len];
        f.seek(SeekFrom::Start(off))?;
        f.read_exact(&mut b)?;
        Ok(b)
    };

    let indx_buf = read_at(indx_row.offset as u64, indx_row.meta as usize * 12)?;
    let indx = parse_be_indx(&indx_buf, 0, indx_row.meta as usize);
    let aset_buf = read_at(aset_row.offset as u64, aset_row.meta as usize * 16)?;
    let aset = parse_be_aset(&aset_buf, 0, aset_row.meta as usize);
    let pths_len = ((file_len - pths_row.offset as u64).min(8 * 1024 * 1024)) as usize;
    let pths_buf = read_at(pths_row.offset as u64, pths_len)?;
    let paths = parse_be_pths(&pths_buf, 0, pths_row.meta as usize);

    if verbose {
        println!(
            "  INDX {} blocks, ASET {} rows, PTHS {} paths",
            indx.len(),
            aset.len(),
            paths.len()
        );
        let mut hist: HashMap<u32, usize> = HashMap::new();
        for e in &aset {
            *hist.entry(e.u3).or_default() += 1;
        }
        let mut h: Vec<(u32, usize)> = hist.into_iter().collect();
        h.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!("  ASET type_id histogram: {h:?}");
        println!("  sample paths:");
        for p in paths.iter().take(6) {
            println!("    {p}");
        }
    }

    let mut by_block: HashMap<u16, Vec<u32>> = HashMap::new();
    for e in &aset {
        // BE ASET row is {asset_hash, u1, u2, u3}; u3 is the type_id.
        if e.u3 == TYPE_WAVEBANK {
            if let Some(b) = block_of(e.u2, indx.len()) {
                by_block.entry(b).or_default().push(e.asset_hash);
            }
        }
    }
    if verbose {
        println!(
            "  wavebank ASET rows: {}, resolving to {} blocks",
            aset.iter().filter(|e| e.u3 == TYPE_WAVEBANK).count(),
            by_block.len()
        );
    }

    let mut banks: Vec<Bank> = Vec::new();
    for (blk, hashes) in &by_block {
        let path = paths.get(*blk as usize).cloned().unwrap_or_default();
        let Some(entry) = indx.get(*blk as usize) else { continue };
        let start = entry.file_offset() as u64;
        let size = entry.page_count as usize * PAGE_SIZE;
        if size == 0 || start + size as u64 > file_len {
            continue;
        }
        let raw = read_at(start, size)?;
        let dec = match decompress_be_sges(&raw, 0, size) {
            Ok(d) => d,
            Err(e) => {
                if verbose {
                    println!("  [{path}] decompress FAILED: {e}");
                }
                continue;
            }
        };

        // BE block entry table: count, then 16-byte rows {name_hash, type_hash, field_c, chunk_size},
        // then the containers back-to-back.
        let count = be32(&dec, 0) as usize;
        if verbose {
            let tys: Vec<String> = (0..count.min(6))
                .map(|i| format!("0x{:08X}", be32(&dec, 4 + i * 16 + 4)))
                .collect();
            println!(
                "  [{}] dec {} B, {count} entries, type_hashes: {}",
                path.rsplit('\\').next().unwrap_or(""),
                dec.len(),
                tys.join(" ")
            );
        }
        let mut off = 4 + count * 16;
        for i in 0..count {
            let r = 4 + i * 16;
            if r + 16 > dec.len() {
                break;
            }
            let name_hash = be32(&dec, r);
            let type_hash = be32(&dec, r + 4);
            let chunk_size = be32(&dec, r + 12) as usize;
            let cstart = off;
            off += chunk_size;
            if type_hash != TH_WAVEBANK {
                continue;
            }
            if !hashes.contains(&name_hash) {
                if verbose {
                    println!(
                        "    name_hash 0x{name_hash:08X} not in this block's ASET list {:?}",
                        hashes.iter().map(|h| format!("0x{h:08X}")).collect::<Vec<_>>()
                    );
                }
                continue;
            }
            if cstart + chunk_size > dec.len() {
                if verbose {
                    println!("    container OOB: {cstart}+{chunk_size} > {}", dec.len());
                }
                continue;
            }
            let container = &dec[cstart..cstart + chunk_size];
            if verbose {
                let n_desc = be32(container, 16) as usize;
                let dao = be32(container, 4);
                let tags: Vec<String> = (0..n_desc.min(8))
                    .map(|d| {
                        let row = 20 + d * 20;
                        if row + 4 > container.len() {
                            return "??".into();
                        }
                        let mut t = [container[row], container[row + 1], container[row + 2], container[row + 3]];
                        t.reverse(); // XFCU => tags are byte-reversed too
                        String::from_utf8_lossy(&t).to_string()
                    })
                    .collect();
                println!(
                    "    wavebank container: magic=XFCU size={chunk_size} data_area_off={dao} n_desc={n_desc} tags={tags:?}"
                );
                let hex: Vec<String> =
                    container[0..40.min(container.len())].iter().map(|b| format!("{b:02X}")).collect();
                println!("      head: {}", hex.join(" "));
            }
            // The console container tags the payload lowercase `data` (stored byte-reversed as
            // `atad`), not the `DATA` the PC containers use. Accept either.
            let body = be_chunk_body(container, b"data")
                .or_else(|| be_chunk_body(container, b"DATA"));
            if verbose {
                if let Some(b) = &body {
                    // Hunt the per-bank BASE offset into the .pws. The record offsets top out
                    // around 2 MB while the stream is ~360 MB, so each bank must address its own
                    // region. Dump the header words + the record min/max so the base shows itself.
                    let recs = parse_records_be(b);
                    let (mn12, mx12) = recs.iter().fold((u32::MAX, 0u32), |(a, z), c| {
                        (a.min(c.f12), z.max(c.f12))
                    });
                    let (mn32, mx32) = recs.iter().fold((u32::MAX, 0u32), |(a, z), c| {
                        (a.min(c.f32), z.max(c.f32))
                    });
                    let hdr: Vec<String> =
                        (0..6).map(|w| format!("+{:02}=0x{:08X}", w * 4, be32(b, w * 4))).collect();
                    println!(
                        "      bank name=0x{name_hash:08X} body={} B clips={} hdr[{}]",
                        b.len(),
                        recs.len(),
                        hdr.join(" ")
                    );
                    println!(
                        "        f12 range [{mn12}..{mx12}]  f32 range [{mn32}..{mx32}]  \
                         extra20..28 of rec0: {:?}",
                        (0..3)
                            .map(|j| be32(b, be32(b, 16) as usize + 20 + j * 4))
                            .collect::<Vec<_>>()
                    );
                }
            }
            if let Some(body) = body {
                let label = path
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".block")
                    .trim_end_matches("_P000_Q3")
                    .to_string();
                let recs = parse_records_be(&body);
                banks.push((label, body, recs));
            }
        }
    }
    Ok(banks)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Pair each <LANG>.WAD with VO_STREAM.<LANG>.PWS.
    let mut pairs: Vec<(String, PathBuf, PathBuf)> = Vec::new();
    for e in std::fs::read_dir(&cli.audios)? {
        let p = e?.path();
        if p.extension().map(|x| x.to_ascii_lowercase()) != Some("wad".into()) {
            continue;
        }
        let lang = p.file_stem().unwrap().to_string_lossy().to_lowercase();
        let pws = cli.audios.join(format!("VO_STREAM.{}.PWS", lang.to_uppercase()));
        if !pws.is_file() {
            eprintln!("  {lang}: no matching VO_STREAM.*.PWS — skipped");
            continue;
        }
        pairs.push((lang, p, pws));
    }
    pairs.sort();

    let mut grand = 0usize;
    for (lang, wad, pws) in &pairs {
        let pws_len = std::fs::metadata(pws)?.len();
        println!("\n=== {lang} ({:.0} MB stream)", pws_len as f64 / 1e6);

        let banks = index_language(wad, cli.list)?;
        let clips: usize = banks.iter().map(|b| b.2.len()).sum();
        println!("  {} VO wavebanks, {clips} clips", banks.len());

        // Identify codec 0x0C. `f16` is the sample count on PC; if that holds here, then
        // bytes/sample = f12/f16 gives the compression ratio and the implied bitrate outright,
        // which pins the codec family without guessing.
        if cli.dump_raw > 0 {
            let dir = cli.out.join(format!("{lang}_raw"));
            std::fs::create_dir_all(&dir)?;
            let mut done = 0usize;
            for (label, body, cs) in &banks {
                for c in cs {
                    if done >= cli.dump_raw {
                        break;
                    }
                    let (off, size) = (c.data_offset as usize, c.data_size as usize);
                    if size == 0 || off + size > body.len() {
                        continue;
                    }
                    let blob = &body[off..off + size];
                    let secs = c.f16 as f64 / c.sample_rate.max(1) as f64;
                    let kbps = (size as f64 * 8.0) / secs / 1000.0;
                    let head: Vec<String> =
                        blob[..24.min(blob.len())].iter().map(|b| format!("{b:02X}")).collect();
                    println!(
                        "    {label} 0x{:08X}: size={size} f16={} rate={} -> {:.2}s, {:.0} kbps, bytes/sample={:.3}",
                        c.clip_hash, c.f16, c.sample_rate, secs, kbps,
                        size as f64 / c.f16.max(1) as f64
                    );
                    println!("      head: {}", head.join(" "));
                    let p = dir.join(format!("{}_0x{:08X}.bin", safe(label), c.clip_hash));
                    File::create(p)?.write_all(blob)?;
                    done += 1;
                }
                if done >= cli.dump_raw {
                    break;
                }
            }
            println!("    dumped {done} raw blobs -> {}", dir.display());
            continue;
        }

        // What codec are the EMBEDDED clips? The .pws is MP3, but the WAD payload does not
        // frame-sync — so it is an Xbox codec (0x05 ADPCM / 0x01|0x69 XMA), which is exactly
        // what ucfx_byteswap::audio transcodes to PC IMA.
        let mut codecs: HashMap<(u8, u8), usize> = HashMap::new();
        for (_, _, cs) in &banks {
            for c in cs {
                *codecs.entry((c.channels, c.codec)).or_default() += 1;
            }
        }
        let mut cv: Vec<((u8, u8), usize)> = codecs.into_iter().collect();
        cv.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!(
            "  (channels, codec) histogram: {:?}",
            cv.iter()
                .map(|((ch, cd), n)| format!("ch{ch}/codec0x{cd:02X}:{n}"))
                .collect::<Vec<_>>()
        );

        // Carve from the BANK BODY (the audio is embedded; the .pws is a separate, larger pool).
        // Each blob is a standalone MP3 bitstream, so it is written as a playable .mp3 — carving
        // is lossless, and re-encoding to .wav would only inflate it.
        let dir = cli.out.join(lang);
        std::fs::create_dir_all(&dir)?;
        let (mut n, mut oob, mut unsynced) = (0usize, 0usize, 0usize);
        let mut bytes = 0u64;
        let mut sync_at: HashMap<usize, usize> = HashMap::new();

        // ★ Codec 0x0C IS MP3 (MPEG-1 Layer III). Proven: each blob's first non-zero byte is
        // `FF FA 40 C0` -- the very frame header the .pws streams carry. Clips are zero-padded and
        // the record offsets only land on a frame start ~15% of the time, so do NOT trust them for
        // the cut. MP3 is self-delimiting: a valid frame states its own length, so a run of
        // back-to-back frames IS a clip and the zero padding ends it. Carve by walking frames; the
        // number of runs found is then an INDEPENDENT check against the bank's record count.
        // Neither signal alone suffices: the record offsets rarely land on a frame start, and a
        // raw frame-walk OVER-segments (VBR frames break the chain). Combine them, which makes the
        // cut exact AND checkable:
        //
        //   * MPEG-1 Layer III always carries 1152 samples per frame, and the record gives the
        //     sample count (f16) -- so the clip's frame COUNT is known exactly.
        //   * The record's data_offset anchors roughly where the clip starts; the true start is
        //     the first valid frame at/after it (skipping the zero padding).
        //   * Walking exactly that many frames must consume ~data_size bytes. That agreement is
        //     the validation -- a wrong cut cannot satisfy it.
        const SAMPLES_PER_FRAME: u32 = 1152;
        let mut size_ok = 0usize;
        for (label, body, cs) in &banks {
            for (i, c) in cs.iter().enumerate() {
                let want_frames = c.f16.div_ceil(SAMPLES_PER_FRAME) as usize;
                if want_frames == 0 {
                    continue;
                }
                // First valid frame at/after the anchor.
                let mut p = c.data_offset as usize;
                let limit = (p + 4096).min(body.len());
                while p < limit && mp3_frame_len(body, p).is_none() {
                    p += 1;
                }
                if p >= limit {
                    unsynced += 1;
                    continue;
                }
                let start = p;

                // Walking every frame is unreliable (VBR frames with unusual bitrate indices break
                // the chain), but it is also unnecessary: on the clips where the walk DID complete,
                // the bytes consumed matched the record's `data_size` exactly (313/313). That
                // validated `data_size`, so take the record's end directly and use the frame walk
                // only as a spot-check of the ones it can follow.
                let end = (c.data_offset as usize + c.data_size as usize).min(body.len());
                if end <= start {
                    oob += 1;
                    continue;
                }

                let mut got = 0usize;
                let mut q = start;
                while got < want_frames {
                    let Some(l) = mp3_frame_len(body, q) else { break };
                    if q + l > body.len() {
                        break;
                    }
                    q += l;
                    got += 1;
                }
                if got == want_frames && (q - start).abs_diff(end - start) <= 256 {
                    size_ok += 1;
                }

                let out = dir.join(format!("{}__{:04}_0x{:08X}.mp3", safe(label), i, c.clip_hash));
                File::create(out)?.write_all(&body[start..end])?;
                bytes += (end - start) as u64;
                n += 1;
            }
        }
        grand += n;
        println!(
            "  wrote {n}/{clips} mp3 ({:.0} MB) — size agrees with record: {size_ok}/{n}; \
             {unsynced} no frame at anchor, {oob} short frame-run",
            bytes as f64 / 1e6
        );
        let _ = (pws_len, sync_at);
    }

    if !cli.list {
        println!("\nTOTAL {grand} files across {} languages", pairs.len());
    }
    Ok(())
}
