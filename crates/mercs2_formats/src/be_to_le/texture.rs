//! Xbox textures: the split their `INFO` declares, and each rung's bytes in PC form.
//!
//! A texture lives in up to four blocks: rung `_P000` holds `NAME`, `INFO` and `BODY`, and each finer
//! rung a lone `BODY`. `INFO` ends in four u16 mip masks, finest rung first, and a rung's `BODY` is
//! exactly its masked mips, largest first: 36,722 of 36,723 PC bodies and 36,845 of 36,846 Xbox bodies
//! (`mercs2_probe tex-rungs`; the one left over on each side is the cube, six faces of the chain).
//!
//! Xbox surfaces are tiled in 32-block macrotiles, so each mip takes `(w, h)` rounded up to 32 blocks.
//! Mips under 32 texels on a side share one surface, the tail, sized from the first of them.
//!
//! The PC engine takes a rung's mips from its mask with no guard, so a texture must arrive with a
//! non-zero `_P000` mask and a `BODY` for every mask it sets (`docs/format_reference.md` §4.3). Xbox
//! bakes break both on purpose: a 42-byte `INFO` flags a map the runtime can stand in for, empties its
//! `_P000`, folds that rung's mips into `_P001`, and keeps the real split in a second table. Conversion
//! puts those mips back, so [`TextureTable`] reads every block before any block is converted.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;

use super::convert::{swap16_blocks, untile_surface};
use crate::dlc_input::{read_be_block, IndxEntry};
use crate::ffcs::load_ffcs_archive;
use crate::texture_encode::{decode_dxt_surface, encode_bc3};
use crate::types::{TYPE_HASH_MODEL, TYPE_HASH_TEXTURE, TYPE_ID_TEXTURE};

/// `INFO +8` bit of a normal map. An Xbox one in DXT1 is DXT5 on PC, all 2,770 of them.
const FLAG_NORMAL: u16 = 0x10;

struct Format {
    block_px: usize,
    pitch: usize,
    log_bpb: usize,
    pc_format: [u8; 4],
    /// Size of the words to swap, in bytes.
    word: usize,
}

/// Xbox D3D format word low byte (`INFO +17`) to its layout and the PC format.
fn xbox_format(b: u8) -> Result<Format, String> {
    let block = |pitch, log_bpb, pc_format| Format { block_px: 4, pitch, log_bpb, pc_format, word: 2 };
    match b {
        0x52 => Ok(block(8, 3, *b"DXT1")),
        0x53 => Ok(block(16, 4, *b"DXT3")),
        0x54 => Ok(block(16, 4, *b"DXT5")),
        0x86 => Ok(Format { block_px: 1, pitch: 4, log_bpb: 2, pc_format: [0x15, 0, 0, 0], word: 4 }),
        _ => Err(format!("format byte 0x{b:02X} is not one a console texture was seen to use")),
    }
}

/// One mip of one face, in linear order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mip {
    pub face: usize,
    pub mip: usize,
    pub bytes: Vec<u8>,
}

/// An Xbox texture's `INFO`, read as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleTexture {
    pub width: usize,
    pub height: usize,
    pub depth: u16,
    pub mips: usize,
    pub flags: u16,
    /// `INFO +10`: 1 a plain texture, 2 a cube.
    pub kind: u16,
    /// `INFO +12`, which the PC reader skips.
    pub reserved: u16,
    /// `INFO +17`.
    pub format: u8,
    /// The float at `INFO +18`, as stored.
    pub bias: [u8; 4],
    /// Mips in each rung's bytes, `[_P000, _P001, _P002, _P003]`.
    pub masks: [u16; 4],
    /// The split of the second table of a 42-byte `INFO`, in the same order.
    pub split: Option<[u16; 4]>,
    /// A PS3 body: PC's mips, linear and in PC's byte order.
    pub linear: bool,
}

impl ConsoleTexture {
    /// Read a 25-byte PS3 `INFO`. Bit `0x20` of PC's flags is not on PS3; PC reads only bit 0.
    fn parse_ps3(info: &[u8]) -> Result<Self, String> {
        let word = |o: usize| u16::from_be_bytes([info[o], info[o + 1]]);
        if word(8) != 2 {
            return Err(format!("PS3 kind {} is not one a DLC texture was seen to use", word(8)));
        }
        let format = match info[11] {
            0x86 => 0x52,
            0x88 => 0x54,
            b => return Err(format!("PS3 format byte 0x{b:02X} is not one a DLC texture was seen to use")),
        };
        Ok(Self {
            width: word(0) as usize,
            height: word(2) as usize,
            depth: word(4),
            mips: word(6) as usize,
            flags: u16::from(info[12]),
            kind: 1,
            reserved: 1,
            format,
            bias: [info[13], info[14], info[15], info[16]],
            masks: [word(23), word(21), word(19), word(17)],
            split: None,
            linear: true,
        })
    }

    /// Read a big-endian `INFO`: 34 or 42 bytes from Xbox, 25 from PS3.
    pub fn parse(info: &[u8]) -> Result<Self, String> {
        if info.len() == 25 {
            return Self::parse_ps3(info);
        }
        if info.len() != 34 && info.len() != 42 {
            return Err(format!("a console texture INFO is 25, 34 or 42 bytes, not {}", info.len()));
        }
        let word = |o: usize| u16::from_be_bytes([info[o], info[o + 1]]);
        let table = |o: usize| [word(o + 6), word(o + 4), word(o + 2), word(o)];
        Ok(Self {
            width: word(0) as usize,
            height: word(2) as usize,
            depth: word(4),
            mips: word(6) as usize,
            flags: word(8),
            kind: word(10),
            reserved: word(12),
            format: info[17],
            bias: [info[18], info[19], info[20], info[21]],
            masks: table(26),
            split: (info.len() == 42).then(|| table(34)),
            linear: false,
        })
    }

    fn faces(&self) -> usize {
        if self.kind == 2 {
            6
        } else {
            1
        }
    }

    /// The mips each rung holds on PC, `[_P000, _P001, _P002, _P003]`.
    pub fn pc_masks(&self) -> [u16; 4] {
        self.split.unwrap_or(self.masks)
    }

    fn is_dxt1_normal(&self) -> bool {
        self.format == 0x52 && self.flags & FLAG_NORMAL != 0
    }

    /// The PC format word of this texture.
    pub fn pc_format(&self) -> Result<[u8; 4], String> {
        Ok(if self.is_dxt1_normal() { *b"DXT5" } else { xbox_format(self.format)?.pc_format })
    }

    fn mip_px(&self, mip: usize) -> (usize, usize) {
        ((self.width >> mip).max(1), (self.height >> mip).max(1))
    }

    /// The first mip of the tail, or `mips` when the chain has none.
    fn first_tail_mip(&self) -> usize {
        (0..self.mips)
            .find(|&m| {
                let (w, h) = self.mip_px(m);
                w.min(h) < 32
            })
            .unwrap_or(self.mips)
    }

    /// Where a tail mip sits in the tail surface, in blocks: 16 texels along the short axis for the
    /// first, halving for each after it. A texture of one mip has no tail to share and sits at the origin.
    fn tail_offset(&self, mip: usize, block_px: usize) -> Result<(usize, usize), String> {
        if self.mips == 1 {
            return Ok((0, 0));
        }
        let packed = mip - self.first_tail_mip();
        if packed > 2 {
            return Err(format!("tail mip {mip} of a {}x{} texture is past the three a tail holds", self.width, self.height));
        }
        let step = (16usize >> packed) / block_px;
        Ok(if self.width.next_power_of_two() > self.height.next_power_of_two() { (0, step) } else { (step, 0) })
    }

    /// Untile a rung's `BODY` into its mips, face by face, largest mip first. Mips are laid out one
    /// after another, every face of a cube at each, with the faces' tails last.
    pub fn untile(&self, mask: u16, tiled: &[u8]) -> Result<Vec<Mip>, String> {
        let f = xbox_format(self.format)?;
        if self.linear {
            return self.split_linear(mask, tiled, &f);
        }
        let first_tail = self.first_tail_mip();
        let (mut out, mut pos) = (Vec::new(), 0usize);
        let mut tails = vec![None::<(Vec<u8>, usize)>; self.faces()];
        for mip in (0..self.mips).filter(|m| mask >> m & 1 != 0) {
            let (wpx, hpx) = self.mip_px(mip);
            let (wb, hb) = (wpx.div_ceil(f.block_px), hpx.div_ceil(f.block_px));
            let (awb, ahb) = ((wb + 31) & !31, (hb + 31) & !31);
            for (face, tail) in tails.iter_mut().enumerate() {
                if mip < first_tail || tail.is_none() {
                    let size = awb * ahb * f.pitch;
                    let surface = tiled.get(pos..pos + size).ok_or_else(|| {
                        format!("mip {mip} of a {}x{} texture runs past a {} byte body", self.width, self.height, tiled.len())
                    })?;
                    let mut linear = untile_surface(surface, awb, ahb, f.pitch, f.log_bpb);
                    if f.word == 2 {
                        swap16_blocks(&mut linear);
                    } else {
                        linear.chunks_exact_mut(f.word).for_each(<[u8]>::reverse);
                    }
                    pos += size;
                    if mip < first_tail {
                        let mut bytes = Vec::with_capacity(wb * hb * f.pitch);
                        for row in 0..hb {
                            let s = row * awb * f.pitch;
                            bytes.extend_from_slice(&linear[s..s + wb * f.pitch]);
                        }
                        out.push(Mip { face, mip, bytes });
                        continue;
                    }
                    *tail = Some((linear, awb));
                }
                let (surface, stride) = tail.as_ref().expect("set above");
                let (bx, by) = self.tail_offset(mip, f.block_px)?;
                let mut bytes = Vec::with_capacity(wb * hb * f.pitch);
                for row in 0..hb {
                    let s = ((by + row) * stride + bx) * f.pitch;
                    let line = surface.get(s..s + wb * f.pitch).ok_or_else(|| format!("tail mip {mip} runs past the tail"))?;
                    bytes.extend_from_slice(line);
                }
                out.push(Mip { face, mip, bytes });
            }
        }
        if pos != tiled.len() {
            return Err(format!("mask 0x{mask:X} of a {}x{} texture covers {pos} of a {} byte body", self.width, self.height, tiled.len()));
        }
        out.sort_by_key(|m| (m.face, m.mip));
        Ok(out)
    }

    /// A linear rung's `BODY` as its mips, largest first, a texture of one face.
    fn split_linear(&self, mask: u16, body: &[u8], f: &Format) -> Result<Vec<Mip>, String> {
        let (mut out, mut pos) = (Vec::new(), 0usize);
        for mip in (0..self.mips).filter(|m| mask >> m & 1 != 0) {
            let (wpx, hpx) = self.mip_px(mip);
            let size = wpx.div_ceil(f.block_px) * hpx.div_ceil(f.block_px) * f.pitch;
            let bytes = body.get(pos..pos + size).ok_or_else(|| {
                format!("mip {mip} of a {}x{} texture runs past a {} byte body", self.width, self.height, body.len())
            })?;
            out.push(Mip { face: 0, mip, bytes: bytes.to_vec() });
            pos += size;
        }
        if pos != body.len() {
            return Err(format!("mask 0x{mask:X} of a {}x{} texture covers {pos} of a {} byte body", self.width, self.height, body.len()));
        }
        Ok(out)
    }

    /// One mip's bytes as PC stores them. A DXT1 normal keeps X and Y in red and green; PC keeps Y in
    /// the colour block and X in alpha.
    fn pc_mip(&self, mip: &Mip) -> Result<Vec<u8>, String> {
        if !self.is_dxt1_normal() {
            return Ok(mip.bytes.clone());
        }
        let (wpx, hpx) = self.mip_px(mip.mip);
        let (wb, hb) = (wpx.div_ceil(4), hpx.div_ceil(4));
        let rgba: Vec<f32> = decode_dxt_surface(b"DXT1", wb, hb, &mip.bytes)?
            .iter()
            .flat_map(|t| [t[1] as f32, t[1] as f32, t[1] as f32, t[0] as f32])
            .collect();
        Ok(encode_bc3(wb * 4, hb * 4, &rgba))
    }

    /// The 34-byte PC `INFO`.
    pub fn pc_info(&self) -> Result<[u8; 34], String> {
        let format = self.pc_format()?;
        let f = xbox_format(self.format)?;
        let pitch = if self.is_dxt1_normal() { 16 } else { f.pitch };
        let total: usize = (0..self.mips)
            .map(|m| {
                let (w, h) = self.mip_px(m);
                w.div_ceil(f.block_px) * h.div_ceil(f.block_px) * pitch
            })
            .sum::<usize>()
            * self.faces();
        let mut out = [0u8; 34];
        let mut put = |o: usize, v: u16| out[o..o + 2].copy_from_slice(&v.to_le_bytes());
        put(0, self.width as u16);
        put(2, self.height as u16);
        put(4, self.depth);
        put(6, self.mips as u16);
        put(8, self.flags & !FLAG_NORMAL);
        put(10, self.kind);
        put(12, self.reserved);
        let [p0, p1, p2, p3] = self.pc_masks();
        put(26, p3);
        put(28, p2);
        put(30, p1);
        put(32, p0);
        out[14..18].copy_from_slice(&format);
        // A PS3 bake keeps PC's 0x10.
        if self.linear {
            out[8..10].copy_from_slice(&self.flags.to_le_bytes());
        }
        out[18..22].copy_from_slice(&[self.bias[3], self.bias[2], self.bias[1], self.bias[0]]);
        out[22..26].copy_from_slice(&(total as u32).to_le_bytes());
        Ok(out)
    }
}

/// The digit of `_P00N_` in a block's path, the rung it holds.
pub fn rung_of_path(path: &str) -> Option<usize> {
    let at = path.rfind("_P00")?;
    let digit = *path.as_bytes().get(at + 4)?;
    (path.as_bytes().get(at + 5) == Some(&b'_') && (b'0'..=b'3').contains(&digit)).then(|| (digit - b'0') as usize)
}

/// One row of a chunk: `(tag, absolute offset, size)`.
pub type ChunkRow = ([u8; 4], usize, usize);

/// A big-endian chunk's rows, tags as they read in a little-endian container.
pub fn container_rows(container: &[u8]) -> Result<Vec<ChunkRow>, String> {
    let word = |o: usize| container.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or("container is short");
    let (area, n) = (word(4)? as usize, word(16)? as usize);
    let data = if area > 0 { area } else { 20 + n * 20 };
    (0..n)
        .map(|i| {
            let r = 20 + i * 20;
            let mut tag: [u8; 4] = container.get(r..r + 4).and_then(|s| s.try_into().ok()).ok_or("row is short")?;
            tag.reverse();
            // The rows of an empty `BODY` carry the all-ones offset.
            let off = word(r + 4)?;
            Ok(if off == u32::MAX { (tag, data, 0) } else { (tag, data + off as usize, word(r + 8)? as usize) })
        })
        .collect()
}

/// One chunk of a block: `(hash, type hash, chunk bytes)`.
pub type BlockChunk<'a> = (u32, u32, &'a [u8]);

/// Every chunk of a big-endian block, in table order.
pub fn block_chunks(block: &[u8]) -> Result<Vec<BlockChunk<'_>>, String> {
    let word = |o: usize| block.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or("block table is short");
    let n = word(0)? as usize;
    let (mut off, mut out) = (4 + n * 16, Vec::with_capacity(n));
    for i in 0..n {
        let r = 4 + i * 16;
        let size = word(r + 12)? as usize;
        let chunk = block.get(off..off + size).ok_or_else(|| format!("chunk {i} runs past the block"))?;
        out.push((word(r)?, word(r + 4)?, chunk));
        off += size;
    }
    Ok(out)
}

/// The chunks of type `ty` in a big-endian block, `(hash, chunk bytes)`.
pub fn typed_chunks(block: &[u8], ty: u32) -> Result<Vec<(u32, &[u8])>, String> {
    Ok(block_chunks(block)?.into_iter().filter(|&(_, t, _)| t == ty).map(|(h, _, c)| (h, c)).collect())
}

/// The texture chunks of a big-endian block, `(hash, chunk bytes)`.
pub fn texture_chunks(block: &[u8]) -> Result<Vec<(u32, &[u8])>, String> {
    typed_chunks(block, TYPE_HASH_TEXTURE)
}

fn row_bytes<'a>(chunk: &'a [u8], tag: &[u8; 4]) -> Result<Option<&'a [u8]>, String> {
    match container_rows(chunk)?.into_iter().find(|(t, ..)| t == tag) {
        None => Ok(None),
        Some((_, off, size)) => chunk.get(off..off + size).map(Some).ok_or_else(|| format!("{} runs past the chunk", String::from_utf8_lossy(tag))),
    }
}

/// The texture hashes of a PC archive's ASET.
pub fn archive_textures(wad: &Path) -> Result<HashSet<u32>, String> {
    let mut f = File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let archive = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    Ok(archive.aset.iter().filter(|e| e.type_id == TYPE_ID_TEXTURE).map(|e| e.asset_hash).collect())
}

/// Every texture of a big-endian archive by hash, so a rung's lone `BODY` can find its `INFO` and the
/// resident chunk the mips folded into `_P001`.
#[derive(Debug, Default)]
pub struct TextureTable {
    pub textures: HashMap<u32, ConsoleTexture>,
    /// The `_P000` mips of a 42-byte texture, read out of its `_P001`.
    folded: HashMap<u32, Vec<Mip>>,
    /// Textures with a 42-byte `INFO` and no `_P001`: no pixels in this archive. The base game has them.
    empty: HashSet<u32>,
    /// Each model's material flags, which its lower rungs' chunks do not hold.
    material_flags: HashMap<u32, Vec<u16>>,
}

impl TextureTable {
    /// Read an archive given how to fetch block `i` of `n`, with the rung its path names, and the
    /// texture hashes of the base game. Fails on any texture whose rungs do not match its masks.
    pub fn read(
        n: usize,
        block: impl Fn(usize) -> Result<(Vec<u8>, Option<usize>), String>,
        base: &HashSet<u32>,
    ) -> Result<Self, String> {
        let mut table = Self::default();
        let mut present: HashMap<u32, [bool; 4]> = HashMap::new();
        for i in 0..n {
            let (bytes, rung) = block(i)?;
            for (hash, chunk) in typed_chunks(&bytes, TYPE_HASH_MODEL).map_err(|e| format!("block {i}: {e}"))? {
                if let Some(mtrl) = row_bytes(chunk, b"MTRL")? {
                    table.material_flags.insert(hash, super::ps3_model::material_flags(mtrl).map_err(|e| format!("0x{hash:08X}: {e}"))?);
                }
            }
            for (hash, chunk) in texture_chunks(&bytes).map_err(|e| format!("block {i}: {e}"))? {
                let rung = rung.ok_or_else(|| format!("block {i} holds texture 0x{hash:08X} and names no rung"))?;
                if row_bytes(chunk, b"BODY")?.is_some_and(|b| !b.is_empty()) {
                    present.entry(hash).or_default()[rung] = true;
                }
                if let Some(info) = row_bytes(chunk, b"INFO")? {
                    let tex = ConsoleTexture::parse(info).map_err(|e| format!("0x{hash:08X}: {e}"))?;
                    if table.textures.insert(hash, tex).is_some() {
                        return Err(format!("0x{hash:08X} has two INFO rows in one archive"));
                    }
                }
            }
        }
        for (hash, tex) in &table.textures {
            let bytes = present.get(hash).copied().unwrap_or_default();
            let rungs: Vec<usize> = (0..4).filter(|&r| tex.masks[r] != 0).collect();
            if rungs.iter().all(|&r| !bytes[r]) && tex.split.is_some() {
                if !base.contains(hash) {
                    return Err(format!("0x{hash:08X} has no pixels in this archive and is not in the base game"));
                }
                table.empty.insert(*hash);
                continue;
            }
            if let Some(r) = (0..4).find(|&r| (tex.masks[r] != 0) != bytes[r]) {
                return Err(format!("0x{hash:08X}: rung {r} and its mask 0x{:X} disagree", tex.masks[r]));
            }
        }
        for i in 0..n {
            let (bytes, rung) = block(i)?;
            if rung != Some(1) {
                continue;
            }
            for (hash, chunk) in texture_chunks(&bytes).map_err(|e| format!("block {i}: {e}"))? {
                let tex = table.textures.get(&hash).ok_or_else(|| format!("0x{hash:08X} has a rung and no INFO"))?;
                let Some(split) = tex.split else { continue };
                let body = row_bytes(chunk, b"BODY")?.ok_or_else(|| format!("0x{hash:08X} has no BODY"))?;
                let mips = tex.untile(tex.masks[1], body).map_err(|e| format!("0x{hash:08X}: {e}"))?;
                table.folded.insert(hash, mips.into_iter().filter(|m| split[0] >> m.mip & 1 != 0).collect());
            }
        }
        Ok(table)
    }

    /// [`Self::read`] over a big-endian archive's blocks.
    pub fn from_doh(doh: &[u8], indx: &[IndxEntry], paths: &[String], base: &HashSet<u32>) -> Result<Self, String> {
        Self::read(
            indx.len(),
            |i| {
                let bytes = read_be_block(doh, &indx[i]).map_err(|e| format!("block {i}: {e}"))?;
                Ok((bytes, paths.get(i).and_then(|p| rung_of_path(p))))
            },
            base,
        )
    }

    /// A table of big-endian model chunks' materials alone, for converting models with no textures.
    pub fn of_models<'a>(chunks: impl Iterator<Item = (u32, &'a [u8])>) -> Result<Self, String> {
        let mut table = Self::default();
        for (hash, chunk) in chunks {
            if let Some(mtrl) = row_bytes(chunk, b"MTRL")? {
                table.material_flags.insert(hash, super::ps3_model::material_flags(mtrl).map_err(|e| format!("0x{hash:08X}: {e}"))?);
            }
        }
        Ok(table)
    }

    /// A texture whose chunk has nothing to carry, left for the base game to serve.
    pub fn is_empty_stub(&self, hash: u32) -> bool {
        self.empty.contains(&hash)
    }

    /// The flags of a model's materials, from whichever of its chunks holds `MTRL`.
    pub fn material_flags(&self, hash: u32) -> Option<&[u16]> {
        self.material_flags.get(&hash).map(Vec::as_slice)
    }

    /// The mips of a rung in PC order and form.
    fn pc_body(&self, hash: u32, tex: &ConsoleTexture, rung: usize, tiled: &[u8]) -> Result<Vec<u8>, String> {
        let wanted = tex.pc_masks()[rung];
        let mut mips: Vec<Mip> = tex.untile(tex.masks[rung], tiled)?.into_iter().filter(|m| wanted >> m.mip & 1 != 0).collect();
        if rung == 0 && tex.split.is_some() {
            mips = self.folded.get(&hash).ok_or_else(|| format!("0x{hash:08X} has no mips for _P000"))?.clone();
        }
        let mut body = Vec::new();
        for mip in &mips {
            body.extend_from_slice(&tex.pc_mip(mip)?);
        }
        Ok(body)
    }

    /// One big-endian chunk as PC stores it, without its `CSUM` trailer.
    pub fn convert_chunk(&self, hash: u32, rung: usize, chunk: &[u8]) -> Result<Vec<u8>, String> {
        let tex = self.textures.get(&hash).ok_or_else(|| format!("0x{hash:08X} has no INFO in this archive"))?;
        let word = |o: usize| u32::from_be_bytes([chunk[o], chunk[o + 1], chunk[o + 2], chunk[o + 3]]);
        let rows = container_rows(chunk)?;
        let mut blob: Vec<u8> = Vec::new();
        let mut table: Vec<u8> = Vec::with_capacity(rows.len() * 20);
        for (i, (tag, off, size)) in rows.iter().enumerate() {
            let source = chunk.get(*off..off + size).ok_or("a row runs past the chunk")?;
            let body = match tag {
                b"INFO" => tex.pc_info()?.to_vec(),
                b"BODY" => self.pc_body(hash, tex, rung, source)?,
                _ => source.to_vec(),
            };
            let r = 20 + i * 20;
            table.extend_from_slice(tag);
            table.extend_from_slice(&(blob.len() as u32).to_le_bytes());
            table.extend_from_slice(&(body.len() as u32).to_le_bytes());
            table.extend_from_slice(&word(r + 12).to_le_bytes());
            table.extend_from_slice(&word(r + 16).to_le_bytes());
            blob.extend_from_slice(&body);
        }
        let mut out = Vec::with_capacity(20 + table.len() + blob.len());
        out.extend_from_slice(b"UCFX");
        out.extend_from_slice(&((20 + rows.len() * 20) as u32).to_le_bytes());
        out.extend_from_slice(&word(8).to_le_bytes());
        out.extend_from_slice(&word(12).to_le_bytes());
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(&blob);
        Ok(out)
    }
}

/// What [`super::convert::convert_block`] needs from the whole archive for one block.
#[derive(Clone, Copy)]
pub struct BlockTables<'a> {
    pub textures: &'a TextureTable,
    /// The rung the block's path names.
    pub rung: Option<usize>,
    /// The archive's streamed banks. A big-endian streamed bank errors without one.
    pub streams: Option<&'a super::audio::StreamTable>,
}

/// Untile a whole tiled DXT chain, every mip in one body.
pub fn untile_dxt_chain(tiled: &[u8], width: usize, height: usize, fourcc: &[u8; 4], mips: usize) -> Result<Vec<u8>, String> {
    let format = match fourcc {
        b"DXT1" => 0x52,
        b"DXT3" => 0x53,
        b"DXT5" => 0x54,
        _ => return Err(format!("{} is not a block format", String::from_utf8_lossy(fourcc))),
    };
    let tex = ConsoleTexture {
        width,
        height,
        depth: 1,
        mips,
        flags: 0,
        kind: 1,
        reserved: 1,
        format,
        bias: [0; 4],
        masks: [0xFFFF, 0, 0, 0],
        split: None,
        linear: false,
    };
    Ok(tex.untile((1u32 << mips) as u16 - 1, tiled)?.into_iter().flat_map(|m| m.bytes).collect())
}

#[cfg(test)]
mod tests {
    use super::super::convert::tiled_block_index;
    use super::*;

    fn tex(w: usize, h: usize, mips: usize, masks: [u16; 4], split: Option<[u16; 4]>) -> ConsoleTexture {
        ConsoleTexture { width: w, height: h, depth: 1, mips, flags: 0, kind: 1, reserved: 1, format: 0x52, bias: [0; 4], masks, split, linear: false }
    }

    /// A 25-byte PS3 `INFO` of a 16x16 DXT1 with mips 0 and 1 in `_P001`, mip 2 in `_P000`.
    fn ps3_info(flags: u8) -> Vec<u8> {
        let mut info = vec![0u8; 25];
        for (o, v) in [(0, 16u16), (2, 16), (4, 1), (6, 3), (8, 2), (10, 0x86), (17, 0), (19, 0), (21, 3), (23, 4)] {
            info[o..o + 2].copy_from_slice(&v.to_be_bytes());
        }
        info[12] = flags;
        info[13..17].copy_from_slice(&[0x40, 0x80, 0, 0]);
        info
    }

    #[test]
    fn a_ps3_info_reads_as_pc_fields() {
        let t = ConsoleTexture::parse(&ps3_info(0x0E)).unwrap();
        assert_eq!((t.width, t.mips, t.flags, t.format), (16, 3, 0x0E, 0x52));
        assert_eq!(t.masks, [4, 3, 0, 0]);
        let pc = t.pc_info().unwrap();
        assert_eq!(&pc[14..18], b"DXT1");
        assert_eq!(&pc[18..22], &[0, 0, 0x80, 0x40], "the float reverses");
        assert_eq!(u16::from_le_bytes([pc[8], pc[9]]), 0x0E);
        assert_eq!(u16::from_le_bytes([pc[32], pc[33]]), 4, "_P000's mask");
    }

    #[test]
    fn a_ps3_rung_is_its_mips_as_stored() {
        let t = ConsoleTexture::parse(&ps3_info(0)).unwrap();
        let body: Vec<u8> = (0..160).collect();
        let mips = t.untile(3, &body).unwrap();
        assert_eq!(mips.iter().map(|m| m.bytes.len()).collect::<Vec<_>>(), [128, 32]);
        assert_eq!(mips[1].bytes[0], 128);
        assert!(t.untile(3, &body[..159]).is_err());
    }

    #[test]
    fn a_ps3_dxt1_normal_stays_flagged_and_turns_dxt5() {
        let t = ConsoleTexture::parse(&ps3_info(0x10)).unwrap();
        let pc = t.pc_info().unwrap();
        assert_eq!(&pc[14..18], b"DXT5");
        assert_eq!(u16::from_le_bytes([pc[8], pc[9]]), 0x10);
    }

    #[test]
    fn a_ps3_kind_or_format_no_dlc_texture_uses_is_an_error() {
        let mut cube = ps3_info(0);
        cube[9] = 4;
        assert!(ConsoleTexture::parse(&cube).is_err());
        let mut other = ps3_info(0);
        other[11] = 0x85;
        assert!(ConsoleTexture::parse(&other).is_err());
    }

    /// Write one DXT1 block of `value` into a tiled surface at block `(x, y)`, words big-endian.
    fn put_block(surface: &mut [u8], stride: usize, x: usize, y: usize, value: u8) {
        let at = tiled_block_index(x, y, stride, 3) * 8;
        let mut b = [value; 8];
        b.chunks_exact_mut(2).for_each(<[u8]>::reverse);
        surface[at..at + 8].copy_from_slice(&b);
    }

    #[test]
    fn info_masks_run_finest_rung_first_on_disk() {
        let mut info = vec![0u8; 42];
        info[0..2].copy_from_slice(&256u16.to_be_bytes());
        info[2..4].copy_from_slice(&128u16.to_be_bytes());
        info[6..8].copy_from_slice(&7u16.to_be_bytes());
        for (i, v) in [0u16, 3, 0x7C, 0, 0, 3, 4, 0x78].iter().enumerate() {
            info[26 + i * 2..28 + i * 2].copy_from_slice(&v.to_be_bytes());
        }
        let t = ConsoleTexture::parse(&info).unwrap();
        assert_eq!(t.masks, [0, 0x7C, 3, 0]);
        assert_eq!(t.split, Some([0x78, 4, 3, 0]));
        assert_eq!(t.pc_masks(), [0x78, 4, 3, 0]);
        assert!(ConsoleTexture::parse(&info[..26]).is_err());
    }

    #[test]
    fn a_rung_gives_its_surface_mips_then_its_tail_mips() {
        // A 256 DXT1: mip 3 (32 texels) takes its own surface, mips 4 to 6 share the tail.
        let t = tex(256, 256, 7, [0x78, 4, 3, 0], None);
        let mut body = vec![0u8; 8192 * 2];
        let (surface, tail) = body.split_at_mut(8192);
        put_block(surface, 32, 7, 7, 0x11);
        put_block(tail, 32, 4, 0, 0x22);
        put_block(tail, 32, 7, 3, 0x23);
        put_block(tail, 32, 2, 0, 0x33);
        put_block(tail, 32, 1, 0, 0x44);
        let mips = t.untile(0x78, &body).unwrap();
        assert_eq!(mips.iter().map(|m| m.mip).collect::<Vec<_>>(), vec![3, 4, 5, 6]);
        assert_eq!(mips.iter().map(|m| m.bytes.len()).collect::<Vec<_>>(), vec![512, 128, 32, 8]);
        assert_eq!(mips[0].bytes[(7 * 8 + 7) * 8], 0x11);
        assert_eq!(mips[1].bytes[0], 0x22);
        assert_eq!(mips[1].bytes[(3 * 4 + 3) * 8], 0x23);
        assert_eq!(mips[2].bytes[0], 0x33);
        assert_eq!(mips[3].bytes[0], 0x44);
    }

    #[test]
    fn a_wide_texture_stacks_its_tail_down_and_a_lone_mip_sits_at_the_origin() {
        let wide = tex(64, 32, 4, [0xE, 1, 0, 0], None);
        let mut tail = vec![0u8; 8192];
        put_block(&mut tail, 32, 0, 4, 0x55);
        assert_eq!(wide.untile(0xE, &tail).unwrap()[0].bytes[0], 0x55);
        let lone = tex(16, 16, 1, [1, 0, 0, 0], None);
        let mut surface = vec![0u8; 8192];
        put_block(&mut surface, 32, 0, 0, 0x66);
        assert_eq!(lone.untile(1, &surface).unwrap()[0].bytes[0], 0x66);
    }

    #[test]
    fn a_body_the_mask_does_not_cover_exactly_fails() {
        let t = tex(256, 256, 7, [0x78, 4, 3, 0], None);
        assert!(t.untile(0x78, &vec![0u8; 8192]).is_err());
        assert!(t.untile(0x78, &vec![0u8; 8192 * 3]).is_err());
    }

    #[test]
    fn a_tall_tail_is_as_wide_as_its_first_mip_needs() {
        // 1024x64 DXT1: mips 2 to 4 are under 32 texels tall and 64 blocks wide at the start of the tail.
        let t = tex(1024, 64, 5, [0x1C, 0, 0, 0], None);
        assert_eq!(t.untile(0x1C, &vec![0u8; 16384]).unwrap().len(), 3);
    }

    #[test]
    fn a_cube_lays_every_face_of_a_mip_before_the_next_mip() {
        let mut t = tex(128, 128, 6, [0xFFFF, 0, 0, 0], None);
        t.kind = 2;
        let mut body = vec![0u8; 196608];
        put_block(&mut body[8192 * 5..], 32, 0, 0, 0x77); // face 5 of mip 0
        put_block(&mut body[8192 * 6..], 32, 0, 0, 0x88); // face 0 of mip 1
        let mips = t.untile(0xFFFF, &body).unwrap();
        assert_eq!(mips.iter().find(|m| (m.face, m.mip) == (5, 0)).unwrap().bytes[0], 0x77);
        assert_eq!(mips.iter().find(|m| (m.face, m.mip) == (0, 1)).unwrap().bytes[0], 0x88);
    }

    #[test]
    fn a_dxt1_normal_becomes_dxt5_with_x_in_alpha_and_y_in_colour() {
        let mut t = tex(4, 4, 1, [1, 0, 0, 0], None);
        t.flags = FLAG_NORMAL;
        // One solid block: both endpoints red 31, green 0, so X is full and Y is zero.
        let block = 0xF800u16.to_le_bytes();
        let bytes = [block[0], block[1], block[0], block[1], 0, 0, 0, 0].to_vec();
        let out = t.pc_mip(&Mip { face: 0, mip: 0, bytes }).unwrap();
        assert_eq!(out.len(), 16);
        let texel = decode_dxt_surface(b"DXT5", 1, 1, &out).unwrap()[0];
        assert_eq!((texel[0], texel[1], texel[3]), (0, 0, 255));
        assert_eq!(t.pc_format().unwrap(), *b"DXT5");
    }

    #[test]
    fn pc_info_carries_the_split_and_clears_the_normal_flag() {
        let mut t = tex(256, 256, 7, [0, 0x7C, 3, 0], Some([0x78, 4, 3, 0]));
        t.flags = FLAG_NORMAL | 0x20;
        let info = t.pc_info().unwrap();
        let word = |o: usize| u16::from_le_bytes([info[o], info[o + 1]]);
        assert_eq!((word(0), word(6), word(8)), (256, 7, 0x20));
        assert_eq!(&info[14..18], b"DXT5");
        assert_eq!((word(26), word(28), word(30), word(32)), (0, 3, 4, 0x78));
        assert_eq!(u32::from_le_bytes([info[22], info[23], info[24], info[25]]), 87376);
    }

    #[test]
    fn rung_comes_from_the_path() {
        assert_eq!(rung_of_path(r"blocks\vz\c30044-c20001_P002_Q1.block"), Some(2));
        assert_eq!(rung_of_path("resident_P000_Q3.block"), Some(0));
        assert_eq!(rung_of_path("nothing.block"), None);
    }

    fn chunk(info: Option<&[u8]>, body: &[u8]) -> Vec<u8> {
        let rows: Vec<(&[u8; 4], &[u8])> = info.map(|i| (b"OFNI", i)).into_iter().chain([(b"YDOB", body)]).collect();
        let mut head = b"XFCU".to_vec();
        let area = 20 + rows.len() * 20;
        head.extend_from_slice(&(area as u32).to_be_bytes());
        head.extend_from_slice(&[0; 8]);
        head.extend_from_slice(&(rows.len() as u32).to_be_bytes());
        let mut blob = Vec::new();
        for (i, (tag, b)) in rows.iter().enumerate() {
            head.extend_from_slice(*tag);
            head.extend_from_slice(&(blob.len() as u32).to_be_bytes());
            head.extend_from_slice(&(b.len() as u32).to_be_bytes());
            head.extend_from_slice(&((rows.len() - 1 - i) as u32).to_be_bytes());
            head.extend_from_slice(&[0; 4]);
            blob.extend_from_slice(b);
        }
        head.extend_from_slice(&blob);
        head
    }

    fn block(hash: u32, c: &[u8]) -> Vec<u8> {
        let mut b = 1u32.to_be_bytes().to_vec();
        b.extend_from_slice(&hash.to_be_bytes());
        b.extend_from_slice(&TYPE_HASH_TEXTURE.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&(c.len() as u32).to_be_bytes());
        b.extend_from_slice(c);
        b
    }

    fn info42() -> Vec<u8> {
        let mut info = vec![0u8; 42];
        info[0..2].copy_from_slice(&256u16.to_be_bytes());
        info[2..4].copy_from_slice(&256u16.to_be_bytes());
        info[4..6].copy_from_slice(&1u16.to_be_bytes());
        info[6..8].copy_from_slice(&7u16.to_be_bytes());
        info[8..10].copy_from_slice(&0x20u16.to_be_bytes());
        info[10..12].copy_from_slice(&1u16.to_be_bytes());
        info[17] = 0x52;
        for (i, v) in [0u16, 0, 0x7F, 0, 0, 0, 7, 0x78].iter().enumerate() {
            info[26 + i * 2..28 + i * 2].copy_from_slice(&v.to_be_bytes());
        }
        info
    }

    #[test]
    fn the_mips_folded_into_p001_go_back_to_p000() {
        const H: u32 = 0x1234_5678;
        let resident = block(H, &chunk(Some(&info42()), &[]));
        // Mips 0 to 3 take their own surfaces, then the tail: 32768 + 8192 * 4.
        let mut tiled = vec![0u8; 65536];
        put_block(&mut tiled[32768 + 8192 + 8192..], 32, 0, 0, 0x99); // first block of mip 3
        let rung = block(H, &chunk(None, &tiled));
        let table = TextureTable::read(
            2,
            |i| Ok(if i == 0 { (resident.clone(), Some(0)) } else { (rung.clone(), Some(1)) }),
            &HashSet::new(),
        )
        .unwrap();
        let chunk_of = |b: &[u8]| texture_chunks(b).unwrap()[0].1.to_vec();
        let p0 = table.convert_chunk(H, 0, &chunk_of(&resident)).unwrap();
        let p1 = table.convert_chunk(H, 1, &chunk_of(&rung)).unwrap();
        let leaf = |c: &[u8], tag: &str| -> Vec<u8> {
            let mut sealed = c.to_vec();
            let crc = crate::crc32::crc32_mercs2(&sealed);
            sealed.extend_from_slice(b"CSUM");
            sealed.extend_from_slice(&crc.to_le_bytes());
            let tree = crate::ucfx::parse_ucfx_tree(&sealed).unwrap();
            tree.iter().find(|n| n.tag_str() == tag).and_then(|n| n.body.clone()).unwrap().to_vec()
        };
        let body0 = leaf(&p0, "BODY");
        assert_eq!(body0.len(), 512 + 128 + 32 + 8);
        assert_eq!(body0[0], 0x99);
        assert_eq!(leaf(&p1, "BODY").len(), 32768 + 8192 + 2048);
        let info = leaf(&p0, "INFO");
        assert_eq!((u16::from_le_bytes([info[30], info[31]]), u16::from_le_bytes([info[32], info[33]])), (7, 0x78));
    }

    #[test]
    fn a_texture_with_no_pixels_is_left_to_the_base_game_or_refused() {
        const H: u32 = 0xABCD_0001;
        let resident = block(H, &chunk(Some(&info42()), &[]));
        let read = |base: &HashSet<u32>| TextureTable::read(1, |_| Ok((resident.clone(), Some(0))), base);
        assert!(read(&HashSet::new()).unwrap_err().contains("no pixels"));
        assert!(read(&[H].into()).unwrap().is_empty_stub(H));
    }

    #[test]
    fn a_rung_without_a_mask_is_refused() {
        const H: u32 = 0xABCD_0002;
        let mut info = info42();
        info.truncate(34);
        for (i, v) in [0u16, 0, 0, 0x7F].iter().enumerate() {
            info[26 + i * 2..28 + i * 2].copy_from_slice(&v.to_be_bytes());
        }
        let resident = block(H, &chunk(Some(&info), &[1; 8]));
        let rung = block(H, &chunk(None, &[1; 8]));
        let err = TextureTable::read(
            2,
            |i| Ok(if i == 0 { (resident.clone(), Some(0)) } else { (rung.clone(), Some(1)) }),
            &HashSet::new(),
        )
        .unwrap_err();
        assert!(err.contains("disagree"), "{err}");
    }
}
