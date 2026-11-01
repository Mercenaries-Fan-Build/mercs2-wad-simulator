//! `tex-compare` — an Xbox archive's textures, converted and decoded mip by mip, against their PC twins.
//!
//! Each platform compressed its own blocks, so a correct conversion rarely equals PC byte for byte.
//! What has to agree is the picture: one row per texture and mip with the mean absolute difference of
//! each decoded channel. A surface read from the wrong place shows up far above the codec's noise.
//! `visible_rgb` is the colour difference over texels the twin does not make fully transparent, since
//! PC clears the colour under those. `our_a_vs_their_r` compares a converted normal map's X with the
//! red of a twin still in DXT1. `--ps3 <archive>` compares against a PS3 archive instead, which is how
//! a DLC texture with no PC twin gets checked.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::File;
use std::path::Path;

use mercs2_formats::be_to_le::texture::{archive_textures, container_rows, rung_of_path, texture_chunks, TextureTable};
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::texture::texture_body;
use mercs2_formats::texture_encode::decode_dxt_surface;
use mercs2_formats::types::TYPE_HASH_TEXTURE;
use mercs2_formats::ucfx::{block_chunk_spans, parse_ucfx_tree};

/// Every mip of one PC texture: dimensions, FourCC and each mip's bytes.
struct PcTexture {
    width: usize,
    height: usize,
    fourcc: [u8; 4],
    /// By `(face, mip)`.
    mips: HashMap<(usize, usize), Vec<u8>>,
}

/// The fields of a PC `INFO` a rung's bytes depend on.
struct PcInfo {
    width: usize,
    height: usize,
    mips: usize,
    faces: usize,
    fourcc: [u8; 4],
    /// `[_P000, _P001, _P002, _P003]`.
    masks: [u16; 4],
}

fn pc_info(chunk: &[u8]) -> Result<Option<PcInfo>, String> {
    let tree = parse_ucfx_tree(chunk)?;
    let Some(info) = tree.iter().find(|n| n.tag_str() == "INFO").and_then(|n| n.body.as_ref()) else { return Ok(None) };
    let word = |o: usize| u16::from_le_bytes([info[o], info[o + 1]]);
    Ok(Some(PcInfo {
        width: word(0) as usize,
        height: word(2) as usize,
        mips: word(6) as usize,
        faces: if word(10) == 2 { 6 } else { 1 },
        fourcc: [info[14], info[15], info[16], info[17]],
        masks: [word(32), word(30), word(28), word(26)],
    }))
}

fn block_bytes(fourcc: &[u8; 4]) -> usize {
    if fourcc == b"DXT1" {
        8
    } else {
        16
    }
}

fn pc_textures(vz_wad: &Path) -> Result<HashMap<u32, PcTexture>, String> {
    let mut f = File::open(vz_wad).map_err(|e| format!("{}: {e}", vz_wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", vz_wad.display()))?;
    // Resident rows carry the masks, so they are read first and the rungs follow.
    let (mut infos, mut bodies) = (HashMap::new(), Vec::new());
    for b in 0..pc.indx.len() {
        let dec = decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("pc block {b}: {e}"))?;
        let path = pc.paths.get(b).map_or("", String::as_str);
        let rung = rung_of_path(path);
        for (e, range) in block_chunk_spans(&dec).map_err(|e| format!("pc block {b}: {e}"))? {
            if e.type_hash != TYPE_HASH_TEXTURE {
                continue;
            }
            let chunk = &dec[range];
            if let Some(info) = pc_info(chunk).map_err(|err| format!("pc 0x{:08X}: {err}", e.name_hash))? {
                infos.insert(e.name_hash, info);
            }
            let body = texture_body(chunk).ok_or_else(|| format!("pc 0x{:08X} has no BODY", e.name_hash))?;
            let rung = rung.ok_or_else(|| format!("pc block {path} names no rung"))?;
            bodies.push((e.name_hash, rung, body));
        }
    }
    split_rungs(&infos, bodies)
}

/// Cut each rung's `BODY` into its masked mips.
fn split_rungs(infos: &HashMap<u32, PcInfo>, bodies: Vec<(u32, usize, Vec<u8>)>) -> Result<HashMap<u32, PcTexture>, String> {
    let mut out = HashMap::new();
    for (hash, rung, body) in bodies {
        let info = infos.get(&hash).ok_or_else(|| format!("0x{hash:08X} has no INFO"))?;
        let t: &mut PcTexture = out.entry(hash).or_insert_with(|| PcTexture { width: info.width, height: info.height, fourcc: info.fourcc, mips: HashMap::new() });
        if block_format(&info.fourcc).is_none() {
            continue;
        }
        let pitch = block_bytes(&info.fourcc);
        let mut pos = 0;
        for face in 0..info.faces {
            for mip in (0..info.mips).filter(|m| info.masks[rung] >> m & 1 != 0) {
                let (wb, hb) = ((info.width >> mip).max(1).div_ceil(4), (info.height >> mip).max(1).div_ceil(4));
                let end = pos + wb * hb * pitch;
                let bytes = body.get(pos..end).ok_or_else(|| format!("0x{hash:08X} rung {rung} is short of mip {mip}"))?;
                t.mips.insert((face, mip), bytes.to_vec());
                pos = end;
            }
        }
    }
    Ok(out)
}

/// Every mip of a PS3 archive's textures. The PS3 bake is linear and holds PC's split: its masks equal
/// PC's on all 13,253 shared textures. Its 25-byte big-endian `INFO` has width, height and mips at
/// PC's offsets, the format at +11 (0x86 DXT1, 0x88 DXT5) and the masks at +17.
fn ps3_textures(archive: &Path) -> Result<HashMap<u32, PcTexture>, String> {
    let (doh, _) = load_stfs_or_doh(archive)?;
    let (_, tables) = parse_be_ffcs(&doh)?;
    let indx = tables.iter().find(|r| r.tag == "INDX").ok_or("no INDX")?;
    let entries = parse_be_indx(&doh, indx.offset as usize, indx.meta as usize);
    let paths = tables.iter().find(|r| r.tag == "PTHS").map_or(Vec::new(), |r| parse_be_pths(&doh, r.offset as usize, r.meta as usize));
    let (mut infos, mut bodies) = (HashMap::new(), Vec::new());
    for (bi, e) in entries.iter().enumerate() {
        let be = read_be_block(&doh, e).map_err(|err| format!("ps3 block {bi}: {err}"))?;
        let path = paths.get(bi).map_or("", String::as_str);
        for (hash, chunk) in texture_chunks(&be).map_err(|err| format!("ps3 block {bi}: {err}"))? {
            for (tag, off, size) in container_rows(chunk)? {
                let bytes = chunk.get(off..off + size).ok_or("a PS3 row runs past its chunk")?;
                if &tag == b"INFO" {
                    let word = |o: usize| u16::from_be_bytes([bytes[o], bytes[o + 1]]);
                    let fourcc = match bytes[11] {
                        0x86 => *b"DXT1",
                        0x88 => *b"DXT5",
                        b => return Err(format!("ps3 0x{hash:08X}: format byte 0x{b:02X}")),
                    };
                    infos.insert(hash, PcInfo {
                        width: word(0) as usize,
                        height: word(2) as usize,
                        mips: word(6) as usize,
                        faces: if word(8) == 4 { 6 } else { 1 },
                        fourcc,
                        masks: [word(23), word(21), word(19), word(17)],
                    });
                } else if &tag == b"BODY" {
                    bodies.push((hash, rung_of_path(path).ok_or_else(|| format!("ps3 block {path} names no rung"))?, bytes.to_vec()));
                }
            }
        }
    }
    split_rungs(&infos, bodies)
}

fn block_format(fourcc: &[u8; 4]) -> Option<()> {
    matches!(fourcc, b"DXT1" | b"DXT3" | b"DXT5").then_some(())
}

pub fn tex_compare(archive: &Path, vz_wad: &Path, ps3: Option<&Path>, out: &Path) -> Result<(), String> {
    let pc = match ps3 {
        Some(archive) => ps3_textures(archive)?,
        None => pc_textures(vz_wad)?,
    };
    println!("{} twins", pc.len());
    let (doh, _) = load_stfs_or_doh(archive)?;
    let (_, tables) = parse_be_ffcs(&doh)?;
    let indx = tables.iter().find(|r| r.tag == "INDX").ok_or("no INDX")?;
    let entries = parse_be_indx(&doh, indx.offset as usize, indx.meta as usize);
    let paths = tables.iter().find(|r| r.tag == "PTHS").map_or(Vec::new(), |r| parse_be_pths(&doh, r.offset as usize, r.meta as usize));
    let table = TextureTable::from_doh(&doh, &entries, &paths, &archive_textures(vz_wad)?)?;
    let mut tsv = String::from("hash\tw\th\tconverted\tpc\tflags\tface\tmip\tr\tg\tb\ta\tvisible_rgb\tour_a_vs_their_r\n");
    let (mut no_twin, mut other_size, mut not_blocks, mut stubs) = (0usize, 0usize, 0usize, 0usize);
    for (bi, e) in entries.iter().enumerate() {
        let path = paths.get(bi).map_or("", String::as_str);
        let be = read_be_block(&doh, e).map_err(|err| format!("block {bi}: {err}"))?;
        let rung = rung_of_path(path);
        for (hash, chunk) in texture_chunks(&be).map_err(|err| format!("block {bi}: {err}"))? {
            let tex = table.textures.get(&hash).ok_or_else(|| format!("0x{hash:08X} has no INFO"))?;
            if table.is_empty_stub(hash) {
                stubs += 1;
                continue;
            }
            let rung = rung.ok_or_else(|| format!("block {path} names no rung"))?;
            let converted = table.convert_chunk(hash, rung, chunk).map_err(|err| format!("0x{hash:08X} {path}: {err}"))?;
            let Some(twin) = pc.get(&hash) else {
                no_twin += 1;
                continue;
            };
            if twin.width != tex.width || twin.height != tex.height {
                other_size += 1;
                continue;
            }
            let fourcc = tex.pc_format()?;
            if block_format(&twin.fourcc).is_none() || block_format(&fourcc).is_none() {
                not_blocks += 1;
                continue;
            }
            let body = texture_body(&converted).ok_or_else(|| format!("0x{hash:08X} converted without a BODY"))?;
            let (mut pos, faces) = (0, if tex.kind == 2 { 6 } else { 1 });
            for face in 0..faces {
                for mip in (0..tex.mips).filter(|m| tex.pc_masks()[rung] >> m & 1 != 0) {
                    let theirs = twin.mips.get(&(face, mip)).ok_or_else(|| format!("0x{hash:08X} has no PC face {face} mip {mip}"))?;
                    let (wb, hb) = ((tex.width >> mip).max(1).div_ceil(4), (tex.height >> mip).max(1).div_ceil(4));
                    let end = pos + wb * hb * block_bytes(&fourcc);
                    let ours = body.get(pos..end).ok_or_else(|| format!("0x{hash:08X} rung {rung} is short of mip {mip}"))?;
                    pos = end;
                    let (a, b) = (decode_dxt_surface(&fourcc, wb, hb, ours)?, decode_dxt_surface(&twin.fourcc, wb, hb, theirs)?);
                    let (mut sum, mut visible, mut seen, mut x) = ([0u64; 4], 0u64, 0u64, 0u64);
                    for (p, q) in a.iter().zip(&b) {
                        for k in 0..4 {
                            sum[k] += p[k].abs_diff(q[k]) as u64;
                        }
                        x += p[3].abs_diff(q[0]) as u64;
                        if q[3] != 0 {
                            visible += 1;
                            seen += (0..3).map(|k| p[k].abs_diff(q[k]) as u64).sum::<u64>();
                        }
                    }
                    let n = a.len() as f64;
                    writeln!(tsv, "0x{hash:08X}\t{}\t{}\t{}\t{}\t0x{:X}\t{face}\t{mip}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
                        tex.width, tex.height, String::from_utf8_lossy(&fourcc), String::from_utf8_lossy(&twin.fourcc), tex.flags,
                        sum[0] as f64 / n, sum[1] as f64 / n, sum[2] as f64 / n, sum[3] as f64 / n,
                        seen as f64 / (3 * visible.max(1)) as f64, x as f64 / n)
                        .map_err(|err| err.to_string())?;
                }
            }
            if pos != body.len() {
                return Err(format!("0x{hash:08X} rung {rung} holds {} bytes, its mask covers {pos}", body.len()));
            }
        }
    }
    println!("not compared: {stubs} empty stubs, {no_twin} chunks without a PC twin, {other_size} of another size, {not_blocks} not block compressed");
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))
}
