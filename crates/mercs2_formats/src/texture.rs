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
//!   tex_count×u32 hashes @108 | u32 pixel-shader key | u32`, stride
//!   `116 + tex_count*4`. Slot order is diffuse(0), specular(1), normal(2), at
//!   most 10. The record count comes from the loader's own source
//!   ([`MtrlSource`]). (`Mtrl_Parse` = `FUN_00858790`;
//!   `material_shader_spec.md` §1a.)
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

use crate::ffcs::{read_f32_le, read_u16_le, read_u32_le, FfcsArchive};
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

    /// Map a DXT FourCC to a format. Public so a donor swap can re-encode the user's image
    /// into whatever the donor container already uses.
    pub fn from_fourcc(fourcc: &[u8]) -> Option<TexFormat> {
        match fourcc {
            b"DXT1" => Some(TexFormat::Bc1),
            b"DXT5" => Some(TexFormat::Bc3),
            _ => None,
        }
    }
}

/// One parsed MTRL material record: its texture-asset hashes in slot order
/// (diffuse, specular, normal, …). Slot 0 = diffuse.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MtrlMaterial {
    pub textures: Vec<u32>,
    /// The `u16 flags@104` word. Encodes render mode (e.g. `0x0080` on 3-map materials, `0x0088`
    /// adds a bit) — NOT a per-material intact/ruin gate (verified: the tank's ruin material and an
    /// intact material share `0x0080`). Kept so material rendering can honour it instead of drawing
    /// every submesh flat-opaque.
    pub flags: u16,
    /// The 104-byte float preamble before the flags — material properties (tint / blend / alpha /
    /// specular params). 26 floats; not yet interpreted, but no longer discarded.
    pub preamble: Vec<f32>,
    /// The pixel-shader key after the texture hashes: `pandemic_hash_m2` of a registered pixel
    /// shader name. `Mtrl_Parse` looks it up in the pixel-shader registry and stores the found
    /// index at material `+0x182`.
    pub shader_key: u32,
}

impl MtrlMaterial {
    /// Diffuse (albedo) texture hash — slot 0, or `None` if the record has no textures.
    pub fn diffuse(&self) -> Option<u32> {
        self.textures.first().copied()
    }

    /// Specular / gloss (`_sm`) texture hash — slot 1, or `None`. (Slot 0 = diffuse, slot 1 =
    /// specular, slot 2 = normal — the authored MTRL slot order.)
    pub fn specular(&self) -> Option<u32> {
        self.textures.get(1).copied()
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

/// Which asset loader reads a container's `MTRL` leaf. Each loader takes its material count from
/// its own source, so the walker is told which one it is reading for.
///
/// Every loader parses one record with `Mtrl_Parse` (`FUN_00858790`): a 104-byte preamble, `u16
/// flags @104`, `u16 tex_count @106`, `tex_count × u32` texture hashes, the `u32` pixel-shader key,
/// then one more `u32` (stored at material `+0x7c`). The record is `116 + 4·tex_count` bytes.
/// `Mtrl_Parse` copies the hashes into a 10-slot array at material `+0x144`, so a `tex_count`
/// above 10 writes past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtrlSource {
    /// A `model` container (`0x5B724250`). The count is `u32 @0x24` of the top-level 72-byte `INFO`,
    /// and the records fill the leaf exactly. The model loader's material loop (`0x00414b61`) runs
    /// to the count at model object `+0x44`; every one of the 3,007 retail model `MTRL` leaves
    /// parses exactly with the `INFO @0x24` word.
    Model,
    /// A `terrainmesh` container (`0x7C569307`). `FUN_004a8f30` copies the top-level 32-byte `INFO`
    /// and takes the count from `u32 @0x18`. After the records, the leaf holds `count × 4` blocks
    /// of 16 floats (`count × 256` bytes).
    TerrainMesh,
    /// A `font` container (`0x99E77ACE`). `FUN_004ac8e0` copies the top-level 16-byte `INFO` and
    /// takes the count from `u32 @0`. The records fill the leaf exactly.
    Font,
    /// A `lowresterrain` container (`0x1602815C`). Each `MTRL` leaf holds exactly one record.
    LowResTerrain,
    /// A `scrub` container (`0x600B904E`). Each `SCRB` node's `MTRL` leaf holds exactly one record,
    /// read by `FUN_004a5230` with a single `Mtrl_Parse`.
    Scrub,
}

/// Why a container's `MTRL` data does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtrlError {
    /// The bytes are not a UCFX container, or its descriptor rows do not fit.
    NotUcfx,
    /// A descriptor row's body lies outside the container.
    RowOutOfBounds { row: usize },
    /// The loader's count source is missing or shorter than the count word.
    CountSource { source: MtrlSource, detail: String },
    /// More than one `MTRL` leaf sits where the loader reads one.
    MultipleMtrl { source: MtrlSource, count: usize },
    /// A record's `tex_count` exceeds the 10 texture slots `Mtrl_Parse` fills.
    TexCount { material: usize, tex_count: usize },
    /// The leaf ends inside record `material`.
    Truncated { material: usize, needed: usize, len: usize },
    /// The leaf length is not what the count and the loader's layout give.
    Length { source: MtrlSource, count: usize, expected: usize, actual: usize },
}

impl std::fmt::Display for MtrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MtrlError::NotUcfx => write!(f, "not a UCFX container with in-bounds descriptor rows"),
            MtrlError::RowOutOfBounds { row } => {
                write!(f, "UCFX descriptor row {row} names a body outside the container")
            }
            MtrlError::CountSource { source, detail } => {
                write!(f, "{source:?} material count source: {detail}")
            }
            MtrlError::MultipleMtrl { source, count } => {
                write!(f, "{source:?} container has {count} MTRL leaves where its loader reads one")
            }
            MtrlError::TexCount { material, tex_count } => write!(
                f,
                "MTRL record {material} has tex_count {tex_count}; Mtrl_Parse fills 10 texture slots"
            ),
            MtrlError::Truncated { material, needed, len } => write!(
                f,
                "MTRL record {material} needs {needed} bytes but the leaf is {len} bytes"
            ),
            MtrlError::Length { source, count, expected, actual } => write!(
                f,
                "{source:?} MTRL leaf is {actual} bytes; {count} records and the loader's layout give {expected}"
            ),
        }
    }
}

impl std::error::Error for MtrlError {}

/// Every distinct texture hash sitting at `slot` across all of a model container's materials.
///
/// This is the `from` set for repointing a whole model onto one skin: [`MtrlRepoint`] is a
/// value-scan over the MTRL blob, so replacing each distinct hash at slot 0 with one new hash makes
/// every material in the container name the same diffuse.
///
/// [`MtrlRepoint`]: crate::model_inject::MtrlRepoint
///
/// Deliberately NOT keyed on the host groups. Host selection is decided inside the lowering, after
/// the caller has had to build its texture blocks, and a repoint set that depended on it would have
/// to be computed twice or guessed. Repointing every material is also the honest reading of one
/// `textures:` block for one outfit — and the groups that are not hosts get neutralised anyway.
///
/// Slot order is `0 = diffuse, 1 = SPECULAR, 2 = NORMAL` (see [`MtrlMaterial::specular`]) — not the
/// intuitive d/n/s.
pub fn material_slot_hashes(container: &[u8], slot: usize) -> Result<Vec<u32>, MtrlError> {
    let mut out: Vec<u32> = Vec::new();
    for m in parse_mtrl(container, MtrlSource::Model)? {
        if let Some(&h) = m.textures.get(slot) {
            if h != 0 && !out.contains(&h) {
                out.push(h);
            }
        }
    }
    Ok(out)
}

/// The descendant count stored in descriptor row `i`.
fn row_descendants(v: &UcfxView<'_>, i: usize) -> usize {
    read_u32_le(v.buf, 20 + i * 20 + 16) as usize
}

/// The direct children of row `parent` (`None` = the top level), as row indices.
fn row_children(v: &UcfxView<'_>, parent: Option<usize>) -> Vec<usize> {
    let (mut i, end) = match parent {
        None => (0, v.n_desc),
        Some(p) => (p + 1, (p + 1 + row_descendants(v, p)).min(v.n_desc)),
    };
    let mut out = Vec::new();
    while i < end {
        out.push(i);
        i += 1 + row_descendants(v, i);
    }
    out
}

fn row_body<'a>(v: &UcfxView<'a>, i: usize) -> Result<&'a [u8], MtrlError> {
    let (s, e) = v.resolve(i).ok_or(MtrlError::RowOutOfBounds { row: i })?;
    Ok(&v.buf[s..e])
}

/// The `MTRL` leaves among `rows`.
fn mtrl_rows(v: &UcfxView<'_>, rows: &[usize]) -> Vec<usize> {
    rows.iter().copied().filter(|&i| v.tag(i) == b"MTRL" && !v.is_marker(i)).collect()
}

/// The count word at `offset` of the one `INFO` leaf among `rows`, whose length must be `len`.
fn info_count(
    v: &UcfxView<'_>,
    rows: &[usize],
    source: MtrlSource,
    len: usize,
    offset: usize,
) -> Result<usize, MtrlError> {
    let infos: Vec<usize> =
        rows.iter().copied().filter(|&i| v.tag(i) == b"INFO" && !v.is_marker(i)).collect();
    let [info] = infos[..] else {
        return Err(MtrlError::CountSource {
            source,
            detail: format!("{} INFO leaves beside the MTRL leaf; the loader reads one", infos.len()),
        });
    };
    let body = row_body(v, info)?;
    if body.len() != len {
        return Err(MtrlError::CountSource {
            source,
            detail: format!("INFO is {} bytes; the loader reads {len}", body.len()),
        });
    }
    Ok(read_u32_le(body, offset) as usize)
}

/// Parse exactly `count` records from the front of `body`; returns them and the bytes consumed.
fn parse_records(body: &[u8], count: usize) -> Result<(Vec<MtrlMaterial>, usize), MtrlError> {
    let mut out = Vec::with_capacity(count);
    let mut p = 0usize;
    for material in 0..count {
        if p + 108 > body.len() {
            return Err(MtrlError::Truncated { material, needed: p + 108, len: body.len() });
        }
        let tex_count = read_u16_le(body, p + 106) as usize;
        if tex_count > 10 {
            return Err(MtrlError::TexCount { material, tex_count });
        }
        let end = p + 116 + tex_count * 4;
        if end > body.len() {
            return Err(MtrlError::Truncated { material, needed: end, len: body.len() });
        }
        out.push(MtrlMaterial {
            textures: (0..tex_count).map(|k| read_u32_le(body, p + 108 + k * 4)).collect(),
            flags: read_u16_le(body, p + 104),
            preamble: (0..26).map(|k| read_f32_le(body, p + k * 4)).collect(),
            shader_key: read_u32_le(body, p + 108 + tex_count * 4),
        });
        p = end;
    }
    Ok((out, p))
}

/// Parse one `MTRL` leaf of `count` records plus `tail_per_material` bytes per material, which must
/// fill the leaf exactly.
fn parse_leaf(
    body: &[u8],
    source: MtrlSource,
    count: usize,
    tail_per_material: usize,
) -> Result<Vec<MtrlMaterial>, MtrlError> {
    let (records, used) = parse_records(body, count)?;
    let expected = used + count * tail_per_material;
    if expected != body.len() {
        return Err(MtrlError::Length { source, count, expected, actual: body.len() });
    }
    Ok(records)
}

/// Parse every MTRL material record in a container, the way the loader for `source` reads them.
///
/// The material count comes from the loader's own source (see [`MtrlSource`]), and the records
/// plus the loader's trailing data must fill the leaf exactly. A container with no `MTRL` leaf
/// has no materials. Any other shape is an [`MtrlError`].
pub fn parse_mtrl(container: &[u8], source: MtrlSource) -> Result<Vec<MtrlMaterial>, MtrlError> {
    let v = UcfxView::new(container).ok_or(MtrlError::NotUcfx)?;
    let top = row_children(&v, None);
    match source {
        MtrlSource::Model | MtrlSource::TerrainMesh | MtrlSource::Font => {
            let leaves = mtrl_rows(&v, &top);
            let leaf = match leaves[..] {
                [] => return Ok(Vec::new()),
                [leaf] => leaf,
                _ => return Err(MtrlError::MultipleMtrl { source, count: leaves.len() }),
            };
            let (count, tail) = match source {
                MtrlSource::Model => (info_count(&v, &top, source, 72, 0x24)?, 0),
                MtrlSource::TerrainMesh => (info_count(&v, &top, source, 32, 0x18)?, 256),
                _ => (info_count(&v, &top, source, 16, 0)?, 0),
            };
            parse_leaf(row_body(&v, leaf)?, source, count, tail)
        }
        MtrlSource::LowResTerrain => {
            let mut out = Vec::new();
            for leaf in mtrl_rows(&v, &top) {
                out.extend(parse_leaf(row_body(&v, leaf)?, source, 1, 0)?);
            }
            Ok(out)
        }
        MtrlSource::Scrub => {
            let mut out = Vec::new();
            for scrb in top.iter().copied().filter(|&i| v.tag(i) == b"SCRB") {
                let leaves = mtrl_rows(&v, &row_children(&v, Some(scrb)));
                if leaves.len() > 1 {
                    return Err(MtrlError::MultipleMtrl { source, count: leaves.len() });
                }
                for leaf in leaves {
                    out.extend(parse_leaf(row_body(&v, leaf)?, source, 1, 0)?);
                }
            }
            Ok(out)
        }
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
    let prmg: Vec<usize> = (0..v.n_desc)
        .filter(|&i| v.tag(i) == b"PRMG" && v.is_marker(i))
        .collect();
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
pub fn terrain_group_layers(container: &[u8]) -> Result<Vec<Vec<u32>>, MtrlError> {
    let mats = parse_mtrl(container, MtrlSource::TerrainMesh)?;
    Ok(terrain_group_material_index(container)
        .into_iter()
        .map(|mi| {
            mats.get(mi)
                .map(|m| {
                    m.textures
                        .iter()
                        .copied()
                        .filter(|&h| h != TERRAIN_LAYER_MARKER)
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect())
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
///
/// Public because a *donor* swap needs the container's raw bytes, not a parsed view: to
/// replace a texture safely you re-encode the new image into the donor's own dimensions
/// and format and splice only its `BODY` ([`replace_body`]), leaving every structural
/// field of a container the engine already accepts byte-identical.
pub fn extract_container(
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

/// Build a texture UCFX container (`INFO` + `BODY`) from encoded DXT data, wrapped as a single-entry
/// block ready for `compress_sges` + a `type_id 27` ASET row — the inverse of [`parse_texture_container`].
/// `all_mips` MUST be the full dimension-derived linear mip chain (a short body livelocks the game
/// streamer). INFO is the 34-byte header the retail loader reads: `u16 width@0, u16 height@2,
/// u16 mip_count@6, fourcc@14, u32 total_size@22, residency@26/30 = 0` (fully resident).
pub fn build_texture_block(name_hash: u32, td: &TextureData) -> Vec<u8> {
    let mut info = vec![0u8; 34];
    info[0..2].copy_from_slice(&(td.width as u16).to_le_bytes());
    info[2..4].copy_from_slice(&(td.height as u16).to_le_bytes());
    info[6..8].copy_from_slice(&(td.mip_count as u16).to_le_bytes());
    info[14..18].copy_from_slice(td.format.fourcc());
    info[22..26].copy_from_slice(&(td.all_mips.len() as u32).to_le_bytes());
    // @26/@30 residency descriptor left 0 = fully resident (no streamed tail).
    let body = &td.all_mips;

    // UCFX with two leaves (INFO, BODY): 20-byte header + 2×20 descriptors + data + CSUM.
    let ndesc = 2u32;
    let data_off = 20 + ndesc * 20;
    let mut ucfx = Vec::new();
    ucfx.extend_from_slice(b"UCFX");
    ucfx.extend_from_slice(&data_off.to_le_bytes());
    ucfx.extend_from_slice(&0u32.to_le_bytes());
    ucfx.extend_from_slice(&0u32.to_le_bytes());
    ucfx.extend_from_slice(&ndesc.to_le_bytes());
    // INFO leaf @0, BODY leaf @info.len()
    for (tag, off, len) in [
        (b"INFO", 0u32, info.len() as u32),
        (b"BODY", info.len() as u32, body.len() as u32),
    ] {
        ucfx.extend_from_slice(tag);
        ucfx.extend_from_slice(&off.to_le_bytes());
        ucfx.extend_from_slice(&len.to_le_bytes());
        ucfx.extend_from_slice(&0u32.to_le_bytes());
        ucfx.extend_from_slice(&0u32.to_le_bytes());
    }
    ucfx.extend_from_slice(&info);
    ucfx.extend_from_slice(body);
    let csum = crate::crc32::crc32_mercs2(&ucfx);
    ucfx.extend_from_slice(b"CSUM");
    ucfx.extend_from_slice(&csum.to_le_bytes());

    let mut block = Vec::with_capacity(20 + ucfx.len());
    block.extend_from_slice(&1u32.to_le_bytes());
    block.extend_from_slice(&name_hash.to_le_bytes());
    block.extend_from_slice(&TYPE_HASH_TEXTURE.to_le_bytes());
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&(ucfx.len() as u32).to_le_bytes());
    block.extend_from_slice(&ucfx);
    block
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
    parse_texture_container(&container).map_err(|e| format!("texture 0x{name_hash:08X}: {e}"))
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
    let full_chain =
        linear_mip_chain_size(width as usize, height as usize, format.fourcc(), full_mips);
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

/// Splice new pixel data into a UCFX texture container's `BODY` leaf, in place, and
/// recompute the container CSUM. Returns the rebuilt container.
///
/// **The new body must be exactly the same length as the old one.** That is the whole
/// point: a texture swap done as a *donor BODY-swap* — re-encode the user's image to the
/// donor's own width/height/format, then overwrite only its pixels — keeps every
/// structural field (INFO dims, fourcc, mip count, residency descriptor, descriptor
/// offsets) byte-identical to a container the engine already accepts.
///
/// This makes the nastiest failure mode *unrepresentable* rather than merely validated:
/// a fully-resident texture whose BODY is not exactly `linear_mip_chain_size(...)` makes
/// the engine's streaming worker over-read, returning `STATUS_BUFFER_TOO_SMALL`, and the
/// page never reaches ready state — a **world-load livelock** (a hang, not a crash).
/// Because the length cannot change here, that size can never drift.
///
/// Same shape as `scripts_block::replace_lua`, which is proven in-game.
///
/// Callers doing a swap should also refuse donors that are *not* fully resident
/// (`texsize::info_is_fully_resident`): for a streamed cell texture the base WAD's own
/// finer `_P00N` pages can overwrite an override made under the same hash.
pub fn replace_body(container: &[u8], new_body: &[u8]) -> Result<Vec<u8>, String> {
    let (bs, be) = {
        let v = UcfxView::new(container).ok_or("not a UCFX texture container")?;
        (0..v.n_desc)
            .find(|&i| v.tag(i) == b"BODY")
            .and_then(|i| v.resolve(i))
            .ok_or("no BODY leaf")?
    };

    if new_body.len() != be - bs {
        return Err(format!(
            "new BODY is {} bytes but the container's BODY is {} bytes — a texture swap must \
             preserve the donor's exact mip-chain size (re-encode to the donor's dimensions \
             and format)",
            new_body.len(),
            be - bs
        ));
    }

    let mut out = container.to_vec();
    out[bs..be].copy_from_slice(new_body);

    // Recompute the trailing CSUM: crc32_mercs2 over everything before the `CSUM` tag.
    let tag = out
        .windows(4)
        .rposition(|w| w == b"CSUM")
        .ok_or("container has no CSUM trailer")?;
    if tag + 8 > out.len() {
        return Err("truncated CSUM trailer".into());
    }
    let csum = crate::crc32::crc32_mercs2(&out[..tag]);
    out[tag + 4..tag + 8].copy_from_slice(&csum.to_le_bytes());

    Ok(out)
}

/// Build a **fully-resident** `NAME`/`INFO`/`BODY` texture container.
///
/// This is the shape a texture *replacement* must take, and it is the one shape proven to
/// work in-game (it is what the shipped mattias_v5 / Obama skins use, and a faithful port
/// of `tools/dds_to_ucfx_texture.py`, which produced them).
///
/// # Why a replacement must be fully resident
///
/// Most of the game's textures are **streamed**: `texsize::info_is_fully_resident` is false
/// for 9,562 of the 13,339 retail textures, and their inline `BODY` is only a small
/// resident *tail* — the high mips live in separate streaming blocks. You therefore cannot
/// reskin one by overwriting its body in place: you'd be painting the 32×32 tail while the
/// real pixels stream in from elsewhere.
///
/// The fix the engine already supports is to publish a *fully resident* container under the
/// same asset hash: `INFO[26..32] = 0` (+ the `0xFFFF` sentinel at 32) tells it "there is no
/// streaming, the whole chain is inline", and it reads exactly
/// [`linear_mip_chain_size`] bytes from `BODY`.
///
/// # The invariant that must not be broken
///
/// `body` **must** be exactly `linear_mip_chain_size(width, height, fourcc, dxt_mip_count(w,h))`.
/// The engine reads the full dimension-derived chain regardless of the header's mip field, so
/// a short body makes the streaming worker over-read → `STATUS_BUFFER_TOO_SMALL` → the page
/// never becomes ready → the **world load hangs**. This function enforces it rather than
/// trusting the caller.
pub fn build_resident_texture(
    name: &str,
    width: u32,
    height: u32,
    format: TexFormat,
    body: &[u8],
) -> Result<Vec<u8>, String> {
    let mips = dxt_mip_count(width as usize, height as usize);
    let want = linear_mip_chain_size(width as usize, height as usize, format.fourcc(), mips);
    if body.len() != want {
        return Err(format!(
            "BODY is {} bytes but a fully-resident {width}x{height} {} texture needs exactly \
             {want} (a short body makes the engine over-read and hang the world load)",
            body.len(),
            String::from_utf8_lossy(format.fourcc()),
        ));
    }

    // NAME: NUL-terminated, padded to an even length.
    let mut name_b = name.as_bytes().to_vec();
    name_b.push(0);
    if name_b.len() % 2 != 0 {
        name_b.push(0);
    }

    // INFO (34 bytes): w, h, 1, mips, 0, 1, 1 as u16s; fourcc @14; total_size @22;
    // [26..32] = 0 marks fully resident; u16 0xFFFF sentinel @32.
    let mut info = vec![0u8; 34];
    for (i, v) in [width as u16, height as u16, 1, mips as u16, 0, 1, 1]
        .iter()
        .enumerate()
    {
        info[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
    }
    info[14..18].copy_from_slice(format.fourcc());
    info[22..26].copy_from_slice(&(body.len() as u32).to_le_bytes());
    info[32..34].copy_from_slice(&0xFFFFu16.to_le_bytes());

    // Leaves are 4-byte aligned within the data area; u2 counts the siblings after it.
    let rows: [(&[u8; 4], &[u8], u32); 3] = [
        (b"NAME", &name_b, 2),
        (b"INFO", &info, 1),
        (b"BODY", body, 0),
    ];

    let mut blob: Vec<u8> = Vec::with_capacity(name_b.len() + info.len() + body.len() + 8);
    let mut placed: Vec<(&[u8; 4], u32, u32, u32)> = Vec::with_capacity(3);
    for (tag, data, u2) in rows {
        while blob.len() % 4 != 0 {
            blob.push(0);
        }
        placed.push((tag, blob.len() as u32, data.len() as u32, u2));
        blob.extend_from_slice(data);
    }

    let data_off: u32 = 20 + 3 * 20;
    let mut c: Vec<u8> = Vec::with_capacity(data_off as usize + blob.len() + 8);
    c.extend_from_slice(b"UCFX");
    c.extend_from_slice(&data_off.to_le_bytes());
    c.extend_from_slice(&0u32.to_le_bytes());
    c.extend_from_slice(&0u32.to_le_bytes());
    c.extend_from_slice(&3u32.to_le_bytes()); // n_desc
    for (tag, off, sz, u2) in placed {
        c.extend_from_slice(tag);
        c.extend_from_slice(&off.to_le_bytes());
        c.extend_from_slice(&sz.to_le_bytes());
        c.extend_from_slice(&u2.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
    }
    c.extend_from_slice(&blob);

    let csum = crate::crc32::crc32_mercs2(&c);
    c.extend_from_slice(b"CSUM");
    c.extend_from_slice(&csum.to_le_bytes());

    Ok(c)
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
pub fn assemble_hires(
    width: u32,
    height: u32,
    format: TexFormat,
    mut bodies: Vec<Vec<u8>>,
) -> TextureData {
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
    let full_chain = linear_mip_chain_size(
        width as usize,
        height as usize,
        format.fourcc(),
        dxt_mip_count(width as usize, height as usize),
    );
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
            if acc <= all_mips.len() {
                n += 1;
            } else {
                break;
            }
        }
        n.max(1)
    };
    TextureData {
        width,
        height,
        format,
        mip0,
        all_mips,
        mip_count,
    }
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

    use crate::ucfx::{write_ucfx_tree, UcfxNode};

    /// An MTRL leaf body: one record per entry of `(texture hashes, pixel-shader key)`.
    fn make_mtrl_body(records: &[(&[u32], u32)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (hashes, key) in records {
            body.extend_from_slice(&[0u8; 104]);
            body.extend_from_slice(&0x0080u16.to_le_bytes());
            body.extend_from_slice(&(hashes.len() as u16).to_le_bytes());
            for &h in *hashes {
                body.extend_from_slice(&h.to_le_bytes());
            }
            body.extend_from_slice(&key.to_le_bytes());
            body.extend_from_slice(&0u32.to_le_bytes());
        }
        body
    }

    /// An `INFO` body of `len` bytes holding `count` at `offset`.
    fn info(len: usize, offset: usize, count: u32) -> Vec<u8> {
        let mut b = vec![0u8; len];
        b[offset..offset + 4].copy_from_slice(&count.to_le_bytes());
        b
    }

    fn container(nodes: Vec<UcfxNode>) -> Vec<u8> {
        write_ucfx_tree(&nodes)
    }

    #[test]
    fn model_count_comes_from_info_0x24_and_fills_the_leaf() {
        let body = make_mtrl_body(&[
            (&[0x11111111, 0x22222222, 0x33333333], 0xCAEFE1FE),
            (&[0xAAAAAAAA], 0x322FCD56),
        ]);
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(72, 0x24, 2)),
            UcfxNode::leaf(*b"MTRL", body),
        ]);
        let mats = parse_mtrl(&c, MtrlSource::Model).unwrap();
        assert_eq!(mats.len(), 2);
        assert_eq!(mats[0].textures, vec![0x11111111, 0x22222222, 0x33333333]);
        assert_eq!(mats[0].shader_key, 0xCAEFE1FE);
        assert_eq!(mats[1].textures, vec![0xAAAAAAAA]);
        assert_eq!(mats[1].shader_key, 0x322FCD56);
    }

    #[test]
    fn model_leaf_longer_than_the_count_is_a_length_error() {
        let body = make_mtrl_body(&[(&[1], 5), (&[2], 6)]);
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(72, 0x24, 1)),
            UcfxNode::leaf(*b"MTRL", body),
        ]);
        assert!(matches!(
            parse_mtrl(&c, MtrlSource::Model),
            Err(MtrlError::Length { count: 1, expected: 120, actual: 240, .. })
        ));
    }

    #[test]
    fn model_leaf_shorter_than_the_count_is_truncated() {
        let body = make_mtrl_body(&[(&[1], 5)]);
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(72, 0x24, 2)),
            UcfxNode::leaf(*b"MTRL", body),
        ]);
        assert!(matches!(
            parse_mtrl(&c, MtrlSource::Model),
            Err(MtrlError::Truncated { material: 1, .. })
        ));
    }

    #[test]
    fn tex_count_above_ten_is_refused() {
        let hashes = [7u32; 11];
        let body = make_mtrl_body(&[(&hashes, 5)]);
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(72, 0x24, 1)),
            UcfxNode::leaf(*b"MTRL", body),
        ]);
        assert!(matches!(
            parse_mtrl(&c, MtrlSource::Model),
            Err(MtrlError::TexCount { material: 0, tex_count: 11 })
        ));
    }

    #[test]
    fn zero_textures_parse_to_the_key_after_the_counts() {
        let body = make_mtrl_body(&[(&[], 0x1234)]);
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(72, 0x24, 1)),
            UcfxNode::leaf(*b"MTRL", body),
        ]);
        let mats = parse_mtrl(&c, MtrlSource::Model).unwrap();
        assert!(mats[0].textures.is_empty());
        assert_eq!(mats[0].shader_key, 0x1234);
    }

    #[test]
    fn info_of_the_wrong_length_is_a_count_source_error() {
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(32, 0x18, 1)),
            UcfxNode::leaf(*b"MTRL", make_mtrl_body(&[(&[1], 5)])),
        ]);
        assert!(matches!(
            parse_mtrl(&c, MtrlSource::Model),
            Err(MtrlError::CountSource { source: MtrlSource::Model, .. })
        ));
    }

    #[test]
    fn a_missing_info_is_a_count_source_error() {
        let c = container(vec![UcfxNode::leaf(*b"MTRL", make_mtrl_body(&[(&[1], 5)]))]);
        assert!(matches!(
            parse_mtrl(&c, MtrlSource::Model),
            Err(MtrlError::CountSource { .. })
        ));
    }

    #[test]
    fn two_model_mtrl_leaves_are_refused() {
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(72, 0x24, 1)),
            UcfxNode::leaf(*b"MTRL", make_mtrl_body(&[(&[1], 5)])),
            UcfxNode::leaf(*b"MTRL", make_mtrl_body(&[(&[1], 5)])),
        ]);
        assert!(matches!(
            parse_mtrl(&c, MtrlSource::Model),
            Err(MtrlError::MultipleMtrl { count: 2, .. })
        ));
    }

    #[test]
    fn a_container_without_mtrl_has_no_materials() {
        let c = container(vec![UcfxNode::leaf(*b"INFO", info(72, 0x24, 0))]);
        assert_eq!(parse_mtrl(&c, MtrlSource::Model).unwrap(), Vec::new());
    }

    #[test]
    fn non_ucfx_bytes_are_refused() {
        assert_eq!(parse_mtrl(b"not a container", MtrlSource::Model), Err(MtrlError::NotUcfx));
    }

    #[test]
    fn terrain_mesh_count_is_info_0x18_with_a_256_byte_tail_per_material() {
        let mut body = make_mtrl_body(&[(&[1, 2], 5), (&[3], 6)]);
        body.extend_from_slice(&[0u8; 2 * 256]);
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(32, 0x18, 2)),
            UcfxNode::leaf(*b"MTRL", body.clone()),
        ]);
        assert_eq!(parse_mtrl(&c, MtrlSource::TerrainMesh).unwrap().len(), 2);
        body.truncate(body.len() - 4);
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(32, 0x18, 2)),
            UcfxNode::leaf(*b"MTRL", body),
        ]);
        assert!(matches!(parse_mtrl(&c, MtrlSource::TerrainMesh), Err(MtrlError::Length { .. })));
    }

    #[test]
    fn font_count_is_info_word_0() {
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(16, 0, 2)),
            UcfxNode::leaf(*b"MTRL", make_mtrl_body(&[(&[1], 5), (&[2], 6)])),
        ]);
        assert_eq!(parse_mtrl(&c, MtrlSource::Font).unwrap().len(), 2);
    }

    #[test]
    fn low_res_terrain_leaves_hold_one_record_each() {
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(4, 0, 1)),
            UcfxNode::leaf(*b"MTRL", make_mtrl_body(&[(&[1], 5)])),
        ]);
        assert_eq!(parse_mtrl(&c, MtrlSource::LowResTerrain).unwrap().len(), 1);
        let c = container(vec![UcfxNode::leaf(*b"MTRL", make_mtrl_body(&[(&[1], 5), (&[2], 6)]))]);
        assert!(matches!(
            parse_mtrl(&c, MtrlSource::LowResTerrain),
            Err(MtrlError::Length { count: 1, .. })
        ));
    }

    #[test]
    fn scrub_reads_one_record_under_each_scrb() {
        let scrb = |key: u32| {
            UcfxNode::marker(
                *b"SCRB",
                vec![
                    UcfxNode::leaf(*b"INFO", vec![0u8; 20]),
                    UcfxNode::leaf(*b"MTRL", make_mtrl_body(&[(&[1], key)])),
                ],
            )
        };
        let c = container(vec![scrb(10), scrb(11)]);
        let mats = parse_mtrl(&c, MtrlSource::Scrub).unwrap();
        assert_eq!(mats.iter().map(|m| m.shader_key).collect::<Vec<_>>(), vec![10, 11]);
    }

    #[test]
    fn material_slot_hashes_collects_distinct_nonzero_hashes() {
        let body = make_mtrl_body(&[(&[1, 9], 5), (&[1, 0], 5), (&[2, 9], 5)]);
        let c = container(vec![
            UcfxNode::leaf(*b"INFO", info(72, 0x24, 3)),
            UcfxNode::leaf(*b"MTRL", body),
        ]);
        assert_eq!(material_slot_hashes(&c, 0).unwrap(), vec![1, 2]);
        assert_eq!(material_slot_hashes(&c, 1).unwrap(), vec![9]);
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

    /// A container we build must parse back as fully resident, with the complete chain the
    /// engine will read. If either drifts, the world load hangs — so pin both.
    #[test]
    fn build_resident_texture_round_trips() {
        for (w, h, fmt) in [
            (256u32, 256u32, TexFormat::Bc1),
            (512, 512, TexFormat::Bc3),
            (1024, 512, TexFormat::Bc1),
        ] {
            let mips = dxt_mip_count(w as usize, h as usize);
            let want = linear_mip_chain_size(w as usize, h as usize, fmt.fourcc(), mips);
            let body = vec![0x5Au8; want];

            let c = build_resident_texture("mod_tex", w, h, fmt, &body).expect("build");

            let t = parse_texture_container(&c).expect("parse back");
            assert_eq!((t.width, t.height), (w, h));
            assert_eq!(t.format, fmt);
            assert_eq!(t.mip_count as usize, mips);
            assert_eq!(t.all_mips.len(), want, "the full chain must be inline");
            assert_eq!(texture_name(&c).as_deref(), Some("mod_tex"));

            // The residency descriptor is what tells the engine not to stream.
            let info = info_of(&c);
            assert!(
                crate::texsize::info_is_fully_resident(&info),
                "must be marked fully resident"
            );

            // And the CSUM must verify, or the loader rejects the container.
            let tag = c.windows(4).rposition(|x| x == b"CSUM").expect("CSUM");
            let stored = u32::from_le_bytes(c[tag + 4..tag + 8].try_into().unwrap());
            assert_eq!(stored, crate::crc32::crc32_mercs2(&c[..tag]));
        }
    }

    /// A body that isn't exactly the dimension-derived chain is the livelock bug. Refuse it.
    #[test]
    fn build_resident_texture_rejects_a_short_body() {
        let err = build_resident_texture("t", 256, 256, TexFormat::Bc1, &[0u8; 100]).unwrap_err();
        assert!(err.contains("over-read"), "got: {err}");
    }

    /// Read a container's INFO leaf (test helper).
    pub(super) fn info_of(container: &[u8]) -> Vec<u8> {
        let v = UcfxView::new(container).expect("ucfx");
        for i in 0..v.n_desc {
            if v.tag(i) == b"INFO" {
                if let Some((s, e)) = v.resolve(i) {
                    return container[s..e].to_vec();
                }
            }
        }
        panic!("no INFO");
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
        let row = |c: &mut Vec<u8>, tag: &[u8; 4], u0: u32, size: u32| {
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
