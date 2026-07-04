//! Texture / material extraction for the reimplementation renderer.
//!
//! Given a model UCFX container (as pulled by the model path — see
//! [`crate::model_cubeize`]) plus the `vz.wad` archive, this module resolves each
//! drawing group's material and returns the diffuse texture's **raw DXT/BC body**
//! ready for a direct `wgpu` upload (BC1/BC3 upload natively — no CPU decode).
//!
//! # What is parsed here
//!
//! * **MTRL chunk** — a packed array of material records. Each record is
//!   `104 B float preamble | u16 flags @104 | u16 tex_count @106 |
//!   tex_count×u32 hashes @108`, inter-record stride `116 + tex_count*4`. Slot
//!   order is diffuse(0), specular(1), normal(2). Cap 10. (Decompile-verified
//!   `Mtrl_Parse` = `FUN_00858790`; `material_shader_spec.md` §1a.)
//! * **PRMG groups → material index** — each `PRMG` drawing group carries a
//!   `PRMT` leaf of **16-byte records** `{u32 material_index @0, u32 @4,
//!   u32 @8, u32 @12}`. The first word is the index into the MTRL material array.
//!   (See `texture_extraction_notes.md` for the double-blind confirmation:
//!   PRMT[.0] as a material index resolves to body-part-correct texture NAMEs for
//!   every mattias_v3 group, and the layout generalises to the base model.)
//! * **Texture container** — a UCFX with `NAME` / `INFO` / `BODY` leaves. `INFO`
//!   is `u16 width @0, u16 height @2, u16 @4, u16 mip_count @6, … fourcc @14`
//!   (4-byte "DXT1"/"DXT5"). `BODY` is the contiguous linear DXT mip chain (no
//!   framing); its length equals `linear_mip_chain_size(w, h, fourcc,
//!   dxt_mip_count(w, h))` for a fully-resident character texture.
//!
//! The WAD access mirrors the model path exactly: `load_ffcs_archive` →
//! `decompress_block` → `parse_block_entry_table`, selecting the chunk whose
//! `type_hash == TYPE_HASH_TEXTURE`.

use std::fs::File;

use crate::ffcs::{read_u16_le, read_u32_le, FfcsArchive};
use crate::sges::decompress_block;
use crate::texsize::{dxt_format, dxt_mip_count, linear_mip_chain_size};
use crate::types::{TYPE_HASH_MODEL, TYPE_HASH_TEXTURE, TYPE_ID_MODEL, TYPE_ID_TEXTURE};
use crate::ucfx::parse_block_entry_table;

/// wgpu-native compressed texture format for a character map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexFormat {
    /// DXT1 → `Bc1RgbaUnorm(Srgb)` (8 bytes / 4×4 block).
    Bc1,
    /// DXT5 → `Bc3RgbaUnorm(Srgb)` (16 bytes / 4×4 block).
    Bc3,
}

impl TexFormat {
    /// The DXT FourCC this format was decoded from.
    pub fn fourcc(self) -> &'static [u8; 4] {
        match self {
            TexFormat::Bc1 => b"DXT1",
            TexFormat::Bc3 => b"DXT5",
        }
    }

    fn from_fourcc(fourcc: &[u8]) -> Option<TexFormat> {
        match fourcc {
            b"DXT1" => Some(TexFormat::Bc1),
            b"DXT5" => Some(TexFormat::Bc3),
            _ => None,
        }
    }
}

/// One parsed MTRL material record: its texture-asset hashes in slot order
/// (diffuse, specular, normal, …). Slot 0 = diffuse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MtrlMaterial {
    pub textures: Vec<u32>,
}

impl MtrlMaterial {
    /// Diffuse (albedo) texture hash — slot 0, or `None` if the record has no textures.
    pub fn diffuse(&self) -> Option<u32> {
        self.textures.first().copied()
    }
}

/// A ready-to-upload texture: raw DXT/BC bytes plus dimensions and format.
#[derive(Debug, Clone)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub format: TexFormat,
    /// Mip level 0 (the largest surface) only — a sub-slice of `all_mips`.
    pub mip0: Vec<u8>,
    /// The full linear mip chain, contiguous — upload directly to a `wgpu` texture.
    pub all_mips: Vec<u8>,
    pub mip_count: u32,
}

// ---------------------------------------------------------------------------
// UCFX descriptor helpers (mirrors crate::ucfx / model_cubeize: 20-byte header,
// 20-byte rows; u0 == 0xFFFFFFFF marks a container; abs = data_area_off + u0).
// ---------------------------------------------------------------------------

struct UcfxView<'a> {
    buf: &'a [u8],
    data_area_off: usize,
    n_desc: usize,
}

impl<'a> UcfxView<'a> {
    fn new(buf: &'a [u8]) -> Option<UcfxView<'a>> {
        if buf.len() < 20 || &buf[0..4] != b"UCFX" {
            return None;
        }
        let data_area_off = read_u32_le(buf, 4) as usize;
        let n_desc = read_u32_le(buf, 16) as usize;
        let max_desc = buf.len().saturating_sub(20) / 20;
        if n_desc > max_desc {
            return None;
        }
        Some(UcfxView {
            buf,
            data_area_off,
            n_desc,
        })
    }

    fn tag(&self, i: usize) -> &[u8] {
        let ro = 20 + i * 20;
        &self.buf[ro..ro + 4]
    }
    fn u0(&self, i: usize) -> u32 {
        read_u32_le(self.buf, 20 + i * 20 + 4)
    }
    fn size(&self, i: usize) -> usize {
        read_u32_le(self.buf, 20 + i * 20 + 8) as usize
    }
    fn is_marker(&self, i: usize) -> bool {
        self.u0(i) == 0xFFFF_FFFF
    }

    /// Resolve a leaf row (non-marker) to `(start, end)` in the container.
    fn resolve(&self, i: usize) -> Option<(usize, usize)> {
        let u0 = self.u0(i);
        if u0 == 0xFFFF_FFFF {
            return None;
        }
        let start = if self.data_area_off > 0 {
            self.data_area_off + u0 as usize
        } else {
            8 + u0 as usize
        };
        let end = start.checked_add(self.size(i))?;
        (end <= self.buf.len()).then_some((start, end))
    }
}

// ---------------------------------------------------------------------------
// MTRL
// ---------------------------------------------------------------------------

/// Parse every MTRL material record in a model container.
///
/// Walks the packed material array in each `MTRL` leaf: `[u16 flags @104]
/// [u16 tex_count @106]`, then `tex_count × u32` hashes @108, inter-record
/// stride `116 + tex_count*4`. `tex_count` (1..=10, high byte 0) is the reliable
/// record-boundary signature; on an out-of-range count the walk stops (rather
/// than mis-reading float props as hashes).
pub fn parse_mtrl(container: &[u8]) -> Vec<MtrlMaterial> {
    let mut out = Vec::new();
    let Some(v) = UcfxView::new(container) else {
        return out;
    };
    for i in 0..v.n_desc {
        if v.tag(i) != b"MTRL" {
            continue;
        }
        let Some((s, e)) = v.resolve(i) else { continue };
        parse_mtrl_body(&container[s..e], &mut out);
    }
    out
}

/// Parse a single MTRL chunk body (packed material-record array) into `out`.
fn parse_mtrl_body(body: &[u8], out: &mut Vec<MtrlMaterial>) {
    let mut p = 0usize;
    while p + 108 <= body.len() {
        let tex_count = read_u16_le(body, p + 106) as usize;
        // Record-boundary signature: 1..=10, high byte 0. Anything else means we
        // have walked off the packed array (a rare trailing-float tail); stop.
        if tex_count == 0 || tex_count > 10 {
            break;
        }
        let hashes_end = p + 108 + tex_count * 4;
        if hashes_end > body.len() {
            break;
        }
        let mut textures = Vec::with_capacity(tex_count);
        for k in 0..tex_count {
            textures.push(read_u32_le(body, p + 108 + k * 4));
        }
        out.push(MtrlMaterial { textures });
        p += 116 + tex_count * 4;
    }
}

// ---------------------------------------------------------------------------
// PRMG group -> material index
// ---------------------------------------------------------------------------

/// Group i → material index (into [`parse_mtrl`]'s output).
///
/// One entry per `PRMG` drawing group, in descriptor order. The index is the
/// first word of the group's first `PRMT` 16-byte record. A group whose first
/// PRMT record names material `m` binds `MtrlMaterial[m]`. Groups with no PRMT
/// leaf (non-drawing) map to `0`.
///
/// NOTE: a multi-material group (several distinct PRMT records) is reported by
/// its *first* material here; see [`group_prmt_material_indices`] for the full
/// per-record list.
pub fn group_material_indices(container: &[u8]) -> Vec<usize> {
    group_prmt_material_indices(container)
        .into_iter()
        .map(|recs| recs.first().copied().unwrap_or(0))
        .collect()
}

/// Group i → the full list of material indices from its PRMT records.
///
/// Each 16-byte PRMT record's first word is a material index; a group may carry
/// several (a multi-material sub-mesh set). Single-material groups have their one
/// index duplicated in the file; duplicates are collapsed here in first-seen
/// order.
pub fn group_prmt_material_indices(container: &[u8]) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let Some(v) = UcfxView::new(container) else {
        return out;
    };

    // Row-level scan: each PRMG marker starts a group that runs to the next PRMG.
    let prmg: Vec<usize> = (0..v.n_desc)
        .filter(|&i| v.tag(i) == b"PRMG" && v.is_marker(i))
        .collect();

    for (gi, &pr) in prmg.iter().enumerate() {
        let nxt = prmg.get(gi + 1).copied().unwrap_or(v.n_desc);
        let mut mats: Vec<usize> = Vec::new();
        for i in pr..nxt {
            if v.tag(i) == b"PRMT" && !v.is_marker(i) {
                if let Some((s, e)) = v.resolve(i) {
                    let n = (e - s) / 16;
                    for r in 0..n {
                        let mi = read_u32_le(container, s + r * 16) as usize;
                        if !mats.contains(&mi) {
                            mats.push(mi);
                        }
                    }
                }
            }
        }
        out.push(mats);
    }
    out
}

/// The `A3CD72A7` (BE `a772cda3`) marker that delimits detail layers inside a terrainmesh MTRL record.
pub const TERRAIN_LAYER_MARKER: u32 = 0xA3CD_72A7;

/// Per PRMG drawing group, the MTRL material-record INDEX bound to it. The terrainmesh binds the
/// material via the group's `INFO` leaf (byte-verified: field @+8 = the material index, `< records`),
/// NOT the PRMT (whose first word is geometry data on terrain). Order matches the draw order of
/// [`super::model_cubeize::read_model_meshes`] / `build_indexed_from_container`.
pub fn terrain_group_material_index(container: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let Some(v) = UcfxView::new(container) else {
        return out;
    };
    let prmg: Vec<usize> = (0..v.n_desc).filter(|&i| v.tag(i) == b"PRMG" && v.is_marker(i)).collect();
    for (gi, &pr) in prmg.iter().enumerate() {
        let nxt = prmg.get(gi + 1).copied().unwrap_or(v.n_desc);
        let mut mi = 0usize;
        for i in (pr + 1)..nxt {
            let t = v.tag(i);
            if (t == b"STRM" || t == b"IBUF") && v.is_marker(i) {
                break;
            }
            if t == b"INFO" && !v.is_marker(i) {
                if let Some((s, e)) = v.resolve(i) {
                    if s + 12 <= e {
                        mi = read_u32_le(container, s + 8) as usize;
                    }
                }
                break;
            }
        }
        out.push(mi);
    }
    out
}

/// Per PRMG drawing group, the ordered terrain DETAIL-LAYER texture hashes (≤~4) the group blends:
/// its material (via [`terrain_group_material_index`]) minus the `A3CD72A7` layer markers. The
/// per-vertex COLOR weights blend these layers. Empty vec = group has no valid material.
pub fn terrain_group_layers(container: &[u8]) -> Vec<Vec<u32>> {
    let mats = parse_mtrl(container);
    terrain_group_material_index(container)
        .into_iter()
        .map(|mi| {
            mats.get(mi)
                .map(|m| m.textures.iter().copied().filter(|&h| h != TERRAIN_LAYER_MARKER).collect())
                .unwrap_or_default()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Texture resolution
// ---------------------------------------------------------------------------

/// Pull the UCFX container for `name_hash` of ASET `type_id` / `type_hash`.
///
/// Resolution order (mirrors the engine's streaming resolver):
/// 1. the **primary** ASET row (`sub_entry == 0xFFFF`) → its block; then
/// 2. failing that, **any** ASET row of the right `type_id` for this hash — the
///    texture is a shared/aliased asset carried as a *sub-entry* in another
///    asset's block (verified: e.g. `pmc_hum_strap` diffuse `0x6D74F10B` has no
///    primary row, only a sub-entry into block 2583). Both cases decompress the
///    row's block and select the entry whose `name_hash` (then `type_hash`)
///    matches, so a shared block yields the right chunk.
fn extract_container(
    file: &mut File,
    archive: &FfcsArchive,
    name_hash: u32,
    type_id: u32,
    type_hash: u32,
) -> Result<Vec<u8>, String> {
    // Candidate blocks: primary first, then any other row of the same type.
    let mut blocks: Vec<u16> = Vec::new();
    for e in &archive.aset {
        if e.asset_hash == name_hash && e.type_id == type_id && e.is_primary() {
            blocks.push(e.block_index());
        }
    }
    for e in &archive.aset {
        if e.asset_hash == name_hash && e.type_id == type_id && !e.is_primary() {
            let b = e.block_index();
            if !blocks.contains(&b) {
                blocks.push(b);
            }
        }
    }
    if blocks.is_empty() {
        return Err(format!("no ASET (type_id {type_id}) for 0x{name_hash:08X}"));
    }

    for block in blocks {
        let dec = match decompress_block(file, &archive.indx, block) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let (count, entries) = parse_block_entry_table(&dec);
        let header_end = 4 + count as usize * 16;

        // Prefer the entry whose name_hash + type_hash both match.
        let mut off = header_end;
        for e in &entries {
            let end = off + e.chunk_size as usize;
            if e.type_hash == type_hash && e.name_hash == name_hash && end <= dec.len() {
                return Ok(dec[off..end].to_vec());
            }
            off = end;
        }
        // Otherwise the first entry of the right type (blocks keyed by type only).
        let mut off = header_end;
        for e in &entries {
            let end = off + e.chunk_size as usize;
            if e.type_hash == type_hash && end <= dec.len() {
                return Ok(dec[off..end].to_vec());
            }
            off = end;
        }
    }
    Err(format!(
        "container 0x{name_hash:08X} (type_hash 0x{type_hash:08X}) not found in any candidate block"
    ))
}

/// Load a model container from the archive by its asset name hash.
pub fn extract_model(
    file: &mut File,
    archive: &FfcsArchive,
    name_hash: u32,
) -> Result<Vec<u8>, String> {
    extract_container(file, archive, name_hash, TYPE_ID_MODEL, TYPE_HASH_MODEL)
}

/// Resolve a texture asset (`type_id 27`) to its ready-to-upload DXT/BC data.
///
/// Pulls the primary texture ASET → decompresses its block → finds the texture
/// UCFX chunk → reads its `INFO` (dims + fourcc) and `BODY` (the linear DXT mip
/// chain). The returned [`TextureData::all_mips`] is the raw compressed body,
/// uploadable to a `wgpu` `Bc1`/`Bc3` texture with the full mip chain.
pub fn extract_texture(
    file: &mut File,
    archive: &FfcsArchive,
    name_hash: u32,
) -> Result<TextureData, String> {
    let container =
        extract_container(file, archive, name_hash, TYPE_ID_TEXTURE, TYPE_HASH_TEXTURE)?;
    parse_texture_container(&container)
        .map_err(|e| format!("texture 0x{name_hash:08X}: {e}"))
}

/// Parse a texture UCFX container (`NAME`/`INFO`/`BODY`) into [`TextureData`].
///
/// INFO layout (verified against retail mattias_v3, two independent methods):
/// `u16 width @0, u16 height @2, u16 @4, u16 mip_count @6, … fourcc @14`.
/// BODY is the contiguous linear DXT mip chain.
pub fn parse_texture_container(container: &[u8]) -> Result<TextureData, String> {
    let v = UcfxView::new(container).ok_or("not a UCFX texture container")?;

    let mut info: Option<(usize, usize)> = None;
    let mut body: Option<(usize, usize)> = None;
    for i in 0..v.n_desc {
        match v.tag(i) {
            b"INFO" if info.is_none() => info = v.resolve(i),
            b"BODY" if body.is_none() => body = v.resolve(i),
            _ => {}
        }
    }
    let (is, ie) = info.ok_or("no INFO leaf")?;
    let (bs, be) = body.ok_or("no BODY leaf")?;
    let info = &container[is..ie];
    if info.len() < 18 {
        return Err(format!("INFO too short ({} bytes)", info.len()));
    }

    let width = read_u16_le(info, 0) as u32;
    let height = read_u16_le(info, 2) as u32;
    let declared_mips = read_u16_le(info, 6) as u32;
    let fourcc = &info[14..18];
    let format = TexFormat::from_fourcc(fourcc).ok_or_else(|| {
        format!(
            "unsupported texture fourcc {:?} (only DXT1/DXT5)",
            std::str::from_utf8(fourcc).unwrap_or("????")
        )
    })?;
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return Err(format!("implausible dimensions {width}x{height}"));
    }

    let all_mips = container[bs..be].to_vec();

    // Mip count: prefer the dimension-derived chain (the count the engine
    // instantiates, `texsize::dxt_mip_count`); fall back to the INFO field if the
    // body is a shorter (streamed) resident tail.
    let full_mips = dxt_mip_count(width as usize, height as usize);
    let full_chain = linear_mip_chain_size(
        width as usize,
        height as usize,
        format.fourcc(),
        full_mips,
    );
    let mip_count = if all_mips.len() >= full_chain {
        full_mips as u32
    } else {
        declared_mips.max(1)
    };

    // mip0 = the largest surface (level 0) prefix of the chain.
    let (block_px, texel_pitch, _) = dxt_format(format.fourcc()).ok_or("non-DXT format")?;
    let wb = (width as usize).div_ceil(block_px).max(1);
    let hb = (height as usize).div_ceil(block_px).max(1);
    let mip0_len = (wb * hb * texel_pitch).min(all_mips.len());
    let mip0 = all_mips[..mip0_len].to_vec();

    Ok(TextureData {
        width,
        height,
        format,
        mip0,
        all_mips,
        mip_count,
    })
}

/// Return just the raw BODY leaf bytes of a UCFX texture container. Works for the resident full
/// container (`NAME`/`INFO`/`BODY`) AND for the streaming higher-mip containers, which ship a lone
/// `BODY` chunk (one finer mip level's raw DXT bytes, no INFO/NAME). `None` if there's no BODY leaf.
pub fn texture_body(container: &[u8]) -> Option<Vec<u8>> {
    let v = UcfxView::new(container)?;
    for i in 0..v.n_desc {
        if v.tag(i) == b"BODY" {
            let (s, e) = v.resolve(i)?;
            return Some(container[s..e].to_vec());
        }
    }
    None
}

/// Assemble a full-resolution [`TextureData`] from a resident container (dims/format + its resident
/// mip tail) plus the higher-mip BODY payloads streamed from finer LOD blocks. Each `body` is a
/// contiguous mip-chain segment (a lone finer mip, or the resident tail); the geometric 4× mip ratio
/// guarantees that ordering them by size DESCENDING and concatenating reproduces the full linear
/// chain mip0..mipN. Duplicate-sized segments are de-duped (the resident block may be scanned twice).
pub fn assemble_hires(width: u32, height: u32, format: TexFormat, mut bodies: Vec<Vec<u8>>) -> TextureData {
    bodies.sort_by(|a, b| b.len().cmp(&a.len()));
    let mut seen = std::collections::HashSet::new();
    let mut all_mips = Vec::new();
    for body in bodies {
        if seen.insert(body.len()) {
            all_mips.extend_from_slice(&body);
        }
    }
    let (block_px, texel_pitch, _) = dxt_format(format.fourcc()).unwrap_or((4, 8, 3));
    let wb = (width as usize).div_ceil(block_px).max(1);
    let hb = (height as usize).div_ceil(block_px).max(1);
    let mip0_len = (wb * hb * texel_pitch).min(all_mips.len());
    let mip0 = all_mips[..mip0_len].to_vec();
    let full_chain = linear_mip_chain_size(width as usize, height as usize, format.fourcc(), dxt_mip_count(width as usize, height as usize));
    let mip_count = if all_mips.len() >= full_chain {
        dxt_mip_count(width as usize, height as usize) as u32
    } else {
        // Partial: count whole mip levels present from the top.
        let mut n = 0u32;
        let mut acc = 0usize;
        for l in 0..dxt_mip_count(width as usize, height as usize) {
            let wl = (width as usize >> l).div_ceil(block_px).max(1);
            let hl = (height as usize >> l).div_ceil(block_px).max(1);
            acc += wl * hl * texel_pitch;
            if acc <= all_mips.len() { n += 1; } else { break; }
        }
        n.max(1)
    };
    TextureData { width, height, format, mip0, all_mips, mip_count }
}

/// Read a texture container's `NAME` leaf (for diagnostics / naming), if present.
pub fn texture_name(container: &[u8]) -> Option<String> {
    let v = UcfxView::new(container)?;
    for i in 0..v.n_desc {
        if v.tag(i) == b"NAME" {
            let (s, e) = v.resolve(i)?;
            let raw = &container[s..e];
            return Some(
                String::from_utf8_lossy(raw)
                    .trim_end_matches('\0')
                    .to_string(),
            );
        }
    }
    None
}

/// Read a texture asset's `NAME` from the archive without decoding its body.
pub fn extract_texture_name(
    file: &mut File,
    archive: &FfcsArchive,
    name_hash: u32,
) -> Option<String> {
    let container =
        extract_container(file, archive, name_hash, TYPE_ID_TEXTURE, TYPE_HASH_TEXTURE).ok()?;
    texture_name(&container)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal MTRL leaf body with `count` records of the given tex_counts.
    fn make_mtrl_body(records: &[&[u32]]) -> Vec<u8> {
        let mut body = Vec::new();
        for hashes in records {
            let tc = hashes.len();
            body.extend_from_slice(&[0u8; 104]); // preamble
            body.extend_from_slice(&0x0080u16.to_le_bytes()); // flags
            body.extend_from_slice(&(tc as u16).to_le_bytes()); // tex_count
            for &h in *hashes {
                body.extend_from_slice(&h.to_le_bytes());
            }
            body.extend_from_slice(&[0u8; 8]); // trailing (116 + tc*4 stride)
        }
        body
    }

    #[test]
    fn parse_mtrl_body_multi_record() {
        let body = make_mtrl_body(&[
            &[0x11111111, 0x22222222, 0x33333333],
            &[0xAAAAAAAA],
            &[0xDEADBEEF, 0xCAFEBABE, 0x0BADF00D],
        ]);
        let mut out = Vec::new();
        parse_mtrl_body(&body, &mut out);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].textures, vec![0x11111111, 0x22222222, 0x33333333]);
        assert_eq!(out[0].diffuse(), Some(0x11111111));
        assert_eq!(out[1].textures, vec![0xAAAAAAAA]);
        assert_eq!(out[2].textures, vec![0xDEADBEEF, 0xCAFEBABE, 0x0BADF00D]);
    }

    #[test]
    fn parse_mtrl_body_stops_on_bad_count() {
        // A record followed by a bogus tex_count (0) halts the walk cleanly.
        let mut body = make_mtrl_body(&[&[0x12345678, 0x9ABCDEF0, 0x0F0F0F0F]]);
        body.extend_from_slice(&[0u8; 108]); // all-zero -> tex_count 0 -> stop
        let mut out = Vec::new();
        parse_mtrl_body(&body, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn tex_format_fourcc_roundtrip() {
        assert_eq!(TexFormat::from_fourcc(b"DXT1"), Some(TexFormat::Bc1));
        assert_eq!(TexFormat::from_fourcc(b"DXT5"), Some(TexFormat::Bc3));
        assert_eq!(TexFormat::from_fourcc(b"DXT3"), None);
        assert_eq!(TexFormat::Bc1.fourcc(), b"DXT1");
        assert_eq!(TexFormat::Bc3.fourcc(), b"DXT5");
    }

    /// Build a minimal texture UCFX container (NAME/INFO/BODY) for a w×h DXT1 tex.
    fn make_tex_container(w: u16, h: u16, name: &str) -> Vec<u8> {
        let mips = dxt_mip_count(w as usize, h as usize);
        let body_len = linear_mip_chain_size(w as usize, h as usize, b"DXT1", mips);
        let name_bytes = {
            let mut b = name.as_bytes().to_vec();
            b.push(0);
            b
        };
        // INFO: width@0, height@2, u16@4=1, mip_count@6, then pad, fourcc@14.
        let mut info = vec![0u8; 20];
        info[0..2].copy_from_slice(&w.to_le_bytes());
        info[2..4].copy_from_slice(&h.to_le_bytes());
        info[4..6].copy_from_slice(&1u16.to_le_bytes());
        info[6..8].copy_from_slice(&(mips as u16).to_le_bytes());
        info[14..18].copy_from_slice(b"DXT1");
        let body = vec![0x55u8; body_len];

        let data_area_off = (20 + 3 * 20) as u32;
        let mut c = Vec::new();
        c.extend_from_slice(b"UCFX");
        c.extend_from_slice(&data_area_off.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&3u32.to_le_bytes()); // n_desc

        let name_off = 0u32;
        let info_off = name_bytes.len() as u32;
        let body_off = info_off + info.len() as u32;
        let mut row = |tag: &[u8; 4], u0: u32, size: u32| {
            c.extend_from_slice(tag);
            c.extend_from_slice(&u0.to_le_bytes());
            c.extend_from_slice(&size.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
        };
        row(b"NAME", name_off, name_bytes.len() as u32);
        row(b"INFO", info_off, info.len() as u32);
        row(b"BODY", body_off, body.len() as u32);
        c.extend_from_slice(&name_bytes);
        c.extend_from_slice(&info);
        c.extend_from_slice(&body);
        c
    }

    #[test]
    fn parse_texture_container_dims_and_chain() {
        let c = make_tex_container(256, 256, "pmc_hum_test_head");
        let t = parse_texture_container(&c).expect("parse");
        assert_eq!(t.width, 256);
        assert_eq!(t.height, 256);
        assert_eq!(t.format, TexFormat::Bc1);
        assert_eq!(t.mip_count, dxt_mip_count(256, 256) as u32);
        // Full 256x256 DXT1 chain to 4x4 = 43688 bytes (retail-verified head size).
        assert_eq!(t.all_mips.len(), 43688);
        // mip0 = 256/4 * 256/4 * 8 = 32768.
        assert_eq!(t.mip0.len(), 32768);
        assert_eq!(texture_name(&c).as_deref(), Some("pmc_hum_test_head"));
    }

    #[test]
    fn group_prmt_material_indices_dedups() {
        // Build a model container with two PRMG groups, each with a PRMT leaf.
        // G0: two identical records -> material 3. G1: records {6,7,6} -> [6,7].
        fn prmt_record(mat: u32) -> [u8; 16] {
            let mut r = [0u8; 16];
            r[0..4].copy_from_slice(&mat.to_le_bytes());
            r
        }
        let mut data_area = Vec::new();
        let g0_prmt_off = data_area.len() as u32;
        data_area.extend_from_slice(&prmt_record(3));
        data_area.extend_from_slice(&prmt_record(3));
        let g1_prmt_off = data_area.len() as u32;
        data_area.extend_from_slice(&prmt_record(6));
        data_area.extend_from_slice(&prmt_record(7));
        data_area.extend_from_slice(&prmt_record(6));

        let data_area_off = (20 + 4 * 20) as u32;
        let mut c = Vec::new();
        c.extend_from_slice(b"UCFX");
        c.extend_from_slice(&data_area_off.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&4u32.to_le_bytes());
        let mut row = |c: &mut Vec<u8>, tag: &[u8; 4], u0: u32, size: u32| {
            c.extend_from_slice(tag);
            c.extend_from_slice(&u0.to_le_bytes());
            c.extend_from_slice(&size.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
        };
        row(&mut c, b"PRMG", 0xFFFF_FFFF, 0);
        row(&mut c, b"PRMT", g0_prmt_off, 32);
        row(&mut c, b"PRMG", 0xFFFF_FFFF, 0);
        row(&mut c, b"PRMT", g1_prmt_off, 48);
        c.extend_from_slice(&data_area);

        let per = group_prmt_material_indices(&c);
        assert_eq!(per, vec![vec![3], vec![6, 7]]);
        let first = group_material_indices(&c);
        assert_eq!(first, vec![3, 6]);
    }
}
