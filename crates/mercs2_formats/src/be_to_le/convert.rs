//! Block-level UCFX byteswap converter (main public API).
//!
//! Entry point: [`convert_block`] converts a decompressed Xbox 360 big-endian UCFX
//! block to PC little-endian format. Handles:
//!
//! - **Entry table** (name/type/size/field_c): reads in BE, writes as LE
//! - **Descriptors** (UCFX header): tag and field swaps
//! - **ECS bodies** (Layer, WorldEntity, GuidMap): schema-driven field conversion
//! - **Generic bodies** (Texture, Mesh, Animation, etc.): tag-aware dispatch
//! - **Embedded type conversions**:
//!   - Havok packfile (`layoutRules` restoration)
//!   - Texture untiling (DXT GPU-tiled → linear)
//!   - Wavebank audio transcode (PCM / XMA / XMA2 / MP3 → PC PCM16)
//!   - Mesh vertex format translation (Xbox 12-byte → PC 8-byte D3DVERTEXELEMENT9)
//!   - Lua bytecode (unluac round-trip)
//!   - Terrainmesh re-encoding (vertex expansion + index destrip)
//!
//! Optionally collects schema field coverage stats via [`SchemaCoverageReport`].
//!
//! # Examples
//! ```no_run
//! # use mercs2_formats::be_to_le::convert;
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let be_block: Vec<u8> = vec![/* raw big-endian UCFX bytes */];
//! let le_block = convert::convert_block(&be_block, false, None, None, None, None)?;
//! # Ok(())
//! # }
//! ```

use crate::crc32::crc32_mercs2;
use crate::ffcs::{read_u16_be, read_u32_be};
use crate::tags::ChunkTag;
use crate::types;

use super::ecs::{convert_ecs_bodies, convert_guidmap_body, ComponentSchemas};
use super::havok;
use super::lua;
use super::report::SchemaCoverageReport;
use super::texture::BlockTables;

/// When `true`, the per-block / per-entry diagnostics below are suppressed.
/// The in-process `dlc_port` driver sets this (it converts thousands of blocks);
/// the CLI leaves it `false` so single-block runs stay verbose.
#[doc(hidden)]
pub static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

macro_rules! vlog {
    ($($arg:tt)*) => {
        if !$crate::be_to_le::convert::QUIET.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!($($arg)*);
        }
    };
}

const TYPE_HASH_ECS_NODE: u32 = types::TYPE_HASH_LAYER; // 0xE6B81A54
const TYPE_HASH_WORLD_ENTITY: u32 = types::TYPE_HASH_WORLD_ENTITY_DATA; // 0x5647C35D
const TYPE_HASH_GUIDMAP: u32 = types::TYPE_HASH_GUIDMAP; // 0x140E8728

/// Strip a trailing CSUM/MUSC 8-byte trailer from a raw chunk if present.
/// Both Xbox and PC WADs include the CSUM in the entry table's `chunk_size`.
fn strip_csum_trailer(raw: &[u8]) -> &[u8] {
    if raw.len() >= 8 {
        let tail_tag = &raw[raw.len() - 8..raw.len() - 4];
        if tail_tag == b"CSUM" || tail_tag == b"MUSC" {
            return &raw[..raw.len() - 8];
        }
    }
    raw
}

/// Descriptor row parsed from a UCFX container's descriptor table.
pub(super) struct Descriptor {
    pub(super) tag: ChunkTag,
    pub(super) tag_bytes_le: [u8; 4],
    pub(super) row_u0: u32,
    pub(super) body_size: u32,
    pub(super) row_u3: u32,
    pub(super) row_u4: u32,
}

/// Convert a decompressed BE block to LE format.
/// When `report` is `Some`, collects schema field coverage stats.
/// `lua_tools` is the java + unluac pair that converts `BINN` (Lua bytecode) chunks; a block
/// holding a `BINN` chunk errors when it is `None`.
/// `schemas` is the PC bake's class layouts ([`ComponentSchemas::from_vz_wad`]); a block holding a
/// compact ECS container errors when it is `None`.
/// `tables` is the archive's [`TextureTable`], this block's rung and its streamed banks; a block
/// holding a big-endian texture errors when it is `None`, a streamed bank when it holds none.
pub fn convert_block(
    be_data: &[u8],
    dry_run: bool,
    mut report: Option<&mut SchemaCoverageReport>,
    lua_tools: Option<&lua::LuaTools>,
    schemas: Option<&ComponentSchemas>,
    tables: Option<BlockTables>,
) -> Result<Vec<u8>, String> {
    if be_data.len() < 4 {
        return Err("Block too small".into());
    }

    let entry_count = read_u32_be(be_data, 0) as usize;
    if entry_count > 50000 {
        return Err(format!("Implausible entry count: {}", entry_count));
    }

    let header_size = 4 + entry_count * 16;
    if header_size > be_data.len() {
        return Err("Entry table exceeds block size".into());
    }

    vlog!("  Entry count: {}", entry_count);

    struct EntryInfo {
        name_hash: u32,
        type_hash: u32,
        field_c: u32,
        chunk_size: u32,
    }

    let mut entries = Vec::with_capacity(entry_count);
    for i in 0..entry_count {
        let off = 4 + i * 16;
        entries.push(EntryInfo {
            name_hash: read_u32_be(be_data, off),
            type_hash: read_u32_be(be_data, off + 4),
            field_c: read_u32_be(be_data, off + 8),
            chunk_size: read_u32_be(be_data, off + 12),
        });
    }

    // Dry-run: just walk and report tags
    if dry_run {
        let mut offset = header_size;
        for (ei, entry) in entries.iter().enumerate() {
            let container_end = offset + entry.chunk_size as usize;
            if container_end > be_data.len() {
                return Err(format!("Entry {} container exceeds block (offset={}, size={})",
                    ei, offset, entry.chunk_size));
            }

            let container = strip_csum_trailer(&be_data[offset..container_end]);
            let type_name = types::type_name_from_hash(entry.type_hash);
            vlog!("  Entry {}: name=0x{:08x} type_hash=0x{:08x} ({}) size={}",
                ei, entry.name_hash, entry.type_hash, type_name, entry.chunk_size);

            match walk_container_tags(container, ei) {
                Ok(()) => {}
                Err(e) => vlog!("    [skip] {}", e),
            }

            offset = container_end;
        }
        return Ok(Vec::new());
    }

    // --- Actual conversion ---
    // Two-pass: first convert all containers to compute correct sizes,
    // then write the entry table with correct chunk_size (including CSUM)
    // and offset (cumulative position in the data area).

    // Pass 1: Convert all containers and collect results.
    // chunk_size INCLUDES the 8-byte CSUM trailer on both Xbox and PC,
    // so strip it before converting and re-add a fresh one afterwards.
    let mut converted_containers: Vec<Vec<u8>> = Vec::with_capacity(entry_count);
    let mut kept: Vec<&EntryInfo> = Vec::with_capacity(entry_count);
    let mut offset = header_size;
    for (ei, entry) in entries.iter().enumerate() {
        let container_end = offset + entry.chunk_size as usize;
        if container_end > be_data.len() {
            return Err(format!("Entry {} container exceeds block (offset={}, size={})",
                ei, offset, entry.chunk_size));
        }

        let container = strip_csum_trailer(&be_data[offset..container_end]);
        let type_name = types::type_name_from_hash(entry.type_hash);
        vlog!("  Converting entry {}: type=0x{:08x} ({}) size={} (UCFX={})",
            ei, entry.type_hash, type_name, entry.chunk_size, container.len());

        offset = container_end;
        if entry.type_hash == types::TYPE_HASH_TEXTURE && container.starts_with(b"XFCU") {
            let t = tables.ok_or_else(|| format!("Entry {ei} 0x{:08x}: a texture needs the archive's TextureTable", entry.name_hash))?;
            if t.textures.is_empty_stub(entry.name_hash) {
                continue;
            }
            let rung = t.rung.ok_or_else(|| format!("Entry {ei} 0x{:08x}: this block's path names no rung", entry.name_hash))?;
            let converted = t.textures.convert_chunk(entry.name_hash, rung, container)
                .map_err(|e| format!("Entry {ei} 0x{:08x}: {e}", entry.name_hash))?;
            converted_containers.push(converted);
            kept.push(entry);
            continue;
        }

        if entry.type_hash == types::TYPE_HASH_MODEL && super::ps3_model::has_sspu(container) {
            let flags = tables
                .and_then(|t| t.textures.material_flags(entry.name_hash))
                .ok_or_else(|| format!("Entry {ei} 0x{:08x}: a PS3 model needs its materials from the archive", entry.name_hash))?;
            let converted = super::ps3_model::convert(container, flags, |rest| {
                convert_container(rest, false, ei, entry.type_hash, report.as_deref_mut(), lua_tools, schemas, None)
            })
            .map_err(|e| format!("Entry {ei} 0x{:08x}: {e}", entry.name_hash))?;
            converted_containers.push(converted);
            kept.push(entry);
            continue;
        }

        let fields = match entry.type_hash {
            types::TYPE_HASH_FOLIAGE => Some(super::foliage::convert as fn(&[u8]) -> Result<Vec<u8>, String>),
            types::TYPE_HASH_WATERMAP => Some(super::watermap::convert as fn(&[u8]) -> Result<Vec<u8>, String>),
            types::TYPE_HASH_SOUNDBANK => Some(super::audio::convert_soundbank as fn(&[u8]) -> Result<Vec<u8>, String>),
            types::TYPE_HASH_SOUNDDB => Some(super::audio::convert_sounddb as fn(&[u8]) -> Result<Vec<u8>, String>),
            _ => None,
        };
        if let Some(convert) = fields.filter(|_| container.starts_with(b"XFCU")) {
            let converted = convert(container).map_err(|e| format!("Entry {ei} 0x{:08x}: {e}", entry.name_hash))?;
            converted_containers.push(converted);
            kept.push(entry);
            continue;
        }

        if entry.type_hash == crate::scrub::TYPE_HASH && super::ps3_model::has_sspu(container) {
            let converted = super::ps3_scrub::convert(container).map_err(|e| format!("Entry {ei} 0x{:08x}: {e}", entry.name_hash))?;
            converted_containers.push(converted);
            kept.push(entry);
            continue;
        }

        let is_ecs = entry.type_hash == TYPE_HASH_ECS_NODE
            || entry.type_hash == TYPE_HASH_WORLD_ENTITY
            || entry.type_hash == TYPE_HASH_GUIDMAP;
        let stream = tables.and_then(|t| t.streams).and_then(|s| s.get(entry.name_hash));
        let converted = convert_container(container, is_ecs, ei, entry.type_hash, report.as_deref_mut(), lua_tools, schemas, stream)
            .map_err(|e| format!("Entry {ei} 0x{:08x}: {e}", entry.name_hash))?;
        converted_containers.push(converted);
        kept.push(entry);
    }

    // Pass 2: Compute correct chunk_sizes (UCFX + 8 for CSUM trailer).
    // field_c is a per-entry offset/cookie. It IS 0 in retail base-game blocks, but the DLC's
    // multi-LOD combined-texture blocks carry distinct nonzero field_c per entry (0x60D7,
    // 0x60DA, …) that the engine uses to locate each entry. Zeroing it (the old behaviour)
    // makes the loader mislocate the texture-component object pointer, so it lands mid-record
    // array → ECS component-table holds a record-interior pointer → wild vcall at world load
    // (FUN_007E0420 / grid-pop 0x4CC064). Preserve+byteswap the source value; swapping 0 is a
    // no-op, so this is safe for base-game blocks too. (Matches Python _serialize_entry_table_le.)
    let pc_chunk_sizes: Vec<u32> = converted_containers.iter()
        .map(|c| (c.len() as u32) + 8)
        .collect();

    // Write output: entry table then containers
    let mut output = Vec::with_capacity(be_data.len());

    // Write LE entry table
    output.extend_from_slice(&(kept.len() as u32).to_le_bytes());
    for (ei, entry) in kept.iter().enumerate() {
        output.extend_from_slice(&entry.name_hash.to_le_bytes());
        output.extend_from_slice(&entry.type_hash.to_le_bytes());
        output.extend_from_slice(&entry.field_c.to_le_bytes());
        output.extend_from_slice(&pc_chunk_sizes[ei].to_le_bytes());
    }

    // Append converted containers with CSUM trailers
    for converted in &converted_containers {
        let crc = crc32_mercs2(converted);
        output.extend_from_slice(converted);
        output.extend_from_slice(b"CSUM");
        output.extend_from_slice(&crc.to_le_bytes());
    }

    vlog!("  Output size: {} bytes", output.len());
    Ok(output)
}

/// Convert a single BE UCFX container to LE.
/// One big-endian ECS container, converted on its own: `worldentity`, a layer, a guidmap. The
/// result ends in its `CSUM` trailer, as a chunk of [`convert_block`]'s output does.
pub fn convert_ecs_container(
    container: &[u8],
    type_hash: u32,
    schemas: Option<&ComponentSchemas>,
) -> Result<Vec<u8>, String> {
    let mut out = convert_container(strip_csum_trailer(container), true, 0, type_hash, None, None, schemas, None)?;
    let crc = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

fn convert_container(
    container: &[u8],
    is_ecs: bool,
    entry_idx: usize,
    type_hash: u32,
    report: Option<&mut SchemaCoverageReport>,
    lua_tools: Option<&lua::LuaTools>,
    schemas: Option<&ComponentSchemas>,
    stream: Option<&super::audio::PcStream>,
) -> Result<Vec<u8>, String> {
    if container.len() < 20 {
        return Err(format!("Entry {} container too small ({})", entry_idx, container.len()));
    }

    let magic = &container[0..4];
    if magic != b"XFCU" && magic != b"UCFX" {
        return Err(format!("Entry {}: bad UCFX magic {:?}", entry_idx, magic));
    }

    let is_be = magic == b"XFCU";
    let read_u32: fn(&[u8], usize) -> u32 = if is_be { read_u32_be } else { crate::ffcs::read_u32_le };

    // Read UCFX header fields (bytes 0..20)
    let data_area_off = read_u32(container, 4) as usize;
    let _unk_08 = read_u32(container, 8);
    let _unk_0c = read_u32(container, 12);
    let n_desc = read_u32(container, 16) as usize;

    if n_desc > 10000 {
        return Err(format!("Entry {}: implausible descriptor count {}", entry_idx, n_desc));
    }

    let desc_table_end = 20 + n_desc * 20;
    if desc_table_end > container.len() {
        return Err(format!("Entry {}: descriptor table exceeds container", entry_idx));
    }

    // Parse all descriptors from BE
    let mut descriptors = Vec::with_capacity(n_desc);
    for di in 0..n_desc {
        let row_start = 20 + di * 20;
        let mut tag_bytes = [0u8; 4];
        tag_bytes.copy_from_slice(&container[row_start..row_start + 4]);
        if is_be {
            tag_bytes.reverse();
        }
        let tag = ChunkTag::from_bytes(tag_bytes);

        let row_u0 = read_u32(container, row_start + 4);
        let body_size = read_u32(container, row_start + 8);
        let row_u3 = read_u32(container, row_start + 12);
        let row_u4 = read_u32(container, row_start + 16);

        descriptors.push(Descriptor {
            tag,
            tag_bytes_le: tag_bytes,
            row_u0,
            body_size,
            row_u3,
            row_u4,
        });
    }

    // PS3 1-to-1 terrain / lowres native path: short-circuit the entire
    // Xbox-tuned pipeline (generic sweep + apply_decl_translate + apply_
    // terrainmesh_reencode) because the PS3 stride and attribute offsets
    // differ from Xbox. The native path produces PC-ready bytes directly
    // from the raw BE container; see be_to_le::ps3_native.
    if is_be && super::ps3_native::classify_container(container, type_hash).is_some() {
        return super::ps3_native::convert_container_ps3(container, entry_idx, type_hash);
    }

    // Build output container
    let mut out = Vec::with_capacity(container.len());

    // Write LE UCFX header
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&(data_area_off as u32).to_le_bytes());
    out.extend_from_slice(&_unk_08.to_le_bytes());
    out.extend_from_slice(&_unk_0c.to_le_bytes());
    out.extend_from_slice(&(n_desc as u32).to_le_bytes());

    // Write LE descriptor table
    for desc in &descriptors {
        out.extend_from_slice(&desc.tag_bytes_le);
        out.extend_from_slice(&desc.row_u0.to_le_bytes());
        out.extend_from_slice(&desc.body_size.to_le_bytes());
        out.extend_from_slice(&desc.row_u3.to_le_bytes());
        out.extend_from_slice(&desc.row_u4.to_le_bytes());
    }

    // Copy/convert the data area
    // The data area starts at either `data_area_off` or right after the desc table
    let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };

    // Pad from desc_table_end to data_start (if there's a gap)
    if data_start > desc_table_end && data_start <= container.len() {
        out.extend_from_slice(&container[desc_table_end..data_start]);
    }

    if is_ecs {
        if !is_be || data_area_off == 0 {
            return Err(format!("Entry {entry_idx}: an ECS container must be big-endian with a data area"));
        }
        let mut data_area = container[data_start..].to_vec();
        if type_hash == TYPE_HASH_GUIDMAP {
            convert_guidmap_body(&mut data_area, &descriptors)?;
        } else {
            convert_ecs_bodies(&mut data_area, &descriptors, schemas, report)?;
        }
        out.extend_from_slice(&data_area);
    } else {
        convert_generic_bodies(&mut out, container, &descriptors, data_area_off, is_be, type_hash, entry_idx, report)?;
        if is_be {
            apply_mixr_swap(&mut out, container, &descriptors, data_area_off)?;
        }
    }

    // Repair embedded Havok packfile headers whose 4 × u8 `layoutRules` were
    // wrongly u32-swapped by the generic body sweep (mirrors
    // `tools/ucfx_be_to_le._fix_embedded_havok_layoutrules`). Offsets in `out`
    // match `container` 1:1 when the byteswap preserves size (animation bodies).
    if is_be && out.len() == container.len() {
        fix_embedded_havok_layoutrules(container, &mut out);
    }

    // Xbox 360 wavebanks: transcode embedded audio clips (PCM byte swap / XMA, XMA2
    // and MP3 via ffmpeg → PC PCM16) and reframe the container (the audio body resizes).
    // The generic pass left the `data` chunk raw BE for this.
    if is_be && type_hash == types::TYPE_HASH_WAVEBANK {
        apply_wavebank_transcode(&mut out, &descriptors, data_area_off, desc_table_end, stream)?;
    }

    // Xbox 360 mesh vertex declarations: translate each `decl` chunk from the
    // 12-byte Xbox element format to the 8-byte PC D3DVERTEXELEMENT9 array (a
    // shrink + container reframe). No-op when the container has no `decl`.
    if is_be {
        apply_decl_translate(&mut out, &descriptors, data_area_off, desc_table_end)?;
        // Mesh STRM vertex buffers: re-correct FLOAT16/SHORT vertex components that
        // the generic u32 sweep scrambled (see apply_strm_vertex_fix). Must run after
        // apply_decl_translate so the PC D3DVERTEXELEMENT9 decl is available. Without
        // this, half-float terrain vertices come out spatially transposed -> invalid
        // world coordinates -> world-load streaming stall when the geometry is placed.
        apply_strm_vertex_fix(&mut out, &descriptors, data_area_off, desc_table_end);
    }

    // Xbox 360 Lua bytecode: convert each `BINN` chunk BE→PC LE via the unluac
    // disassemble→flip-endianness→reassemble round-trip (NOT a byte-swap; see
    // lua.rs). The generic pass left BINN raw BE. No-op when there is no BINN.
    if is_be {
        apply_binn_transcode(&mut out, &descriptors, data_area_off, desc_table_end, lua_tools)?;
    }

    // Xbox 360 terrainmesh (0x7C569307): a genuine re-encode (vertices widen to the
    // PC stride, indices de-strip, info/count fields rewrite). Runs last, gated on
    // the terrainmesh type_hash, and rebuilds + reframes the whole data area. See
    // apply_terrainmesh_reencode / docs/terrainmesh_reencode_implementation.md.
    if is_be && type_hash == types::TYPE_HASH_TERRAIN_MESH {
        apply_terrainmesh_reencode(&mut out, &descriptors, data_area_off, desc_table_end, true)?;
    } else if is_be && type_hash == types::TYPE_HASH_LOWRES_TERRAIN {
        // The low-res terrain tile (0x1602815C) needs the same STRM vertex widening +
        // info-stride rewrite, but PC keeps its IBUF as a triangle STRIP — de-stripping to a
        // list over-inflates it (9500 BE → 13472 vs PC's 11852). It was never gated in at
        // all, so its tiles kept the Xbox vertex format and their decoded positions fell
        // outside their own BNDS envelope.
        apply_terrainmesh_reencode(&mut out, &descriptors, data_area_off, desc_table_end, true)?;
    } else if is_be && type_hash == types::TYPE_HASH_MODEL {
        // Skinned character MODEL: the decl translation widened its DEC3N normals/tangents
        // to FLOAT16_4 (stride 32→40) but the data/info stayed 32, so the decl over-runs the
        // vertex buffer → BUFFER_TOO_SMALL → model never signals ObjectIsReady "awake". Apply
        // the STRM vertex widening + info-stride rewrite, and de-strip the IBUF into a
        // degenerate-stitched triangle STRIP (models draw as strips; emitting a list makes
        // the engine read list data as a strip → spikes). MTRL transposition fix is applied
        // separately (wad_builder fix-model-mtrl).
        apply_terrainmesh_reencode(&mut out, &descriptors, data_area_off, desc_table_end, false)?;
    }

    Ok(out)
}

/// Convert every `BINN` (Lua bytecode) chunk in `out` via the unluac round-trip
/// (`lua::convert_binn_be_to_le`) and reframe the container if a body resizes.
/// Mirrors `apply_wavebank_transcode`, generalized to multiple BINN chunks:
/// offsets/sizes are re-read from the (possibly already-shifted) `out` table.
/// Errors when the container holds a `BINN` chunk and `lua_tools` is `None`.
fn apply_binn_transcode(
    out: &mut Vec<u8>,
    descriptors: &[Descriptor],
    data_area_off: usize,
    desc_table_end: usize,
    lua_tools: Option<&lua::LuaTools>,
) -> Result<(), String> {
    let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };
    let binn_idxs: Vec<usize> = descriptors
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            matches!(d.tag, ChunkTag::Binn)
                && d.row_u0 != 0xFFFF_FFFF
                && d.body_size > 0
        })
        .map(|(i, _)| i)
        .collect();
    if binn_idxs.is_empty() {
        return Ok(());
    }
    let tools = lua_tools.ok_or(
        "block holds BINN (Lua bytecode) chunks; converting them needs java and unluac (LuaTools)",
    )?;
    for idx in binn_idxs {
        let row_field = 20 + idx * 20 + 4;
        let size_field = 20 + idx * 20 + 8;
        // Current offset/size (earlier BINN splices may have shifted this one).
        let row_u0 = u32::from_le_bytes(out[row_field..row_field + 4].try_into().unwrap()) as usize;
        let body_size =
            u32::from_le_bytes(out[size_field..size_field + 4].try_into().unwrap()) as usize;
        let abs = data_start + row_u0;
        let old_end = abs + body_size;
        if old_end > out.len() {
            return Err(format!("BINN body [{abs}..{old_end}] exceeds container ({})", out.len()));
        }
        let be_body = out[abs..old_end].to_vec();
        let new_body = lua::convert_binn_be_to_le(&be_body, tools)?;

        out[size_field..size_field + 4].copy_from_slice(&(new_body.len() as u32).to_le_bytes());
        let tail = out[old_end..].to_vec();
        out.truncate(abs);
        out.extend_from_slice(&new_body);
        out.extend_from_slice(&tail);

        let delta = new_body.len() as i64 - body_size as i64;
        if delta != 0 {
            for (j, _) in descriptors.iter().enumerate() {
                if j == idx {
                    continue;
                }
                let rf = 20 + j * 20 + 4;
                let cur = u32::from_le_bytes(out[rf..rf + 4].try_into().unwrap());
                if cur != 0xFFFF_FFFF && data_start + cur as usize >= old_end {
                    out[rf..rf + 4].copy_from_slice(&(((cur as i64) + delta) as u32).to_le_bytes());
                }
            }
        }
    }
    Ok(())
}

/// Xbox-360 wavebank: transcode the audio `data` chunk (PCM / XMA / XMA2 / MP3 → PC PCM16)
/// and reframe the container, mirroring `apply_texture_untile`. The body was left
/// raw BE by `convert_generic_bodies`' wavebank no-op arm, so the BE record fields
/// are intact for `audio::convert_wavebank_data`.
fn apply_wavebank_transcode(
    out: &mut Vec<u8>,
    descriptors: &[Descriptor],
    data_area_off: usize,
    desc_table_end: usize,
    stream: Option<&super::audio::PcStream>,
) -> Result<(), String> {
    let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };
    let Some(data_idx) = descriptors
        .iter()
        .position(|d| d.tag == ChunkTag::Data && d.row_u0 != 0xFFFF_FFFF && d.body_size > 0)
    else {
        return Ok(());
    };
    let d = &descriptors[data_idx];
    let body_abs = data_start + d.row_u0 as usize;
    let old_end = body_abs + d.body_size as usize;
    if old_end > out.len() {
        return Ok(());
    }
    let body_be = out[body_abs..old_end].to_vec();
    let new_body = super::audio::convert_wavebank_data(&body_be, stream)
        .map_err(|e| format!("wavebank transcode: {e}"))?;

    // Update this chunk's body_size field (descriptor row layout: tag@0, row_u0@+4,
    // body_size@+8; rows start at +20, stride 20 — same as apply_texture_untile).
    let size_field = 20 + data_idx * 20 + 8;
    out[size_field..size_field + 4].copy_from_slice(&(new_body.len() as u32).to_le_bytes());

    // Splice the new body in place of the old.
    let tail = out[old_end..].to_vec();
    out.truncate(body_abs);
    out.extend_from_slice(&new_body);
    out.extend_from_slice(&tail);

    // Shift any chunk that sat after the wavebank body by the size delta.
    let delta = new_body.len() as i64 - d.body_size as i64;
    if delta != 0 {
        for (i, dd) in descriptors.iter().enumerate() {
            if i == data_idx || dd.row_u0 == 0xFFFF_FFFF {
                continue;
            }
            if data_start + dd.row_u0 as usize >= old_end {
                let row_field = 20 + i * 20 + 4;
                let new_u0 = (dd.row_u0 as i64 + delta) as u32;
                out[row_field..row_field + 4].copy_from_slice(&new_u0.to_le_bytes());
            }
        }
    }
    Ok(())
}

/// 8-byte Havok 5.5 packfile magic (`57 E0 E0 57 10 C0 C0 10`). Palindromic per
/// u32 word, so it survives a u32 byte-swap and is found at the same offset in
/// the BE input and the LE output.
const HAVOK_PACKFILE_MAGIC: [u8; 8] = [0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10];

/// Restore embedded Havok packfile `layoutRules` (`{ u8 ptrSize; u8 littleEndian;
/// u8 reusePadding; u8 emptyBaseClass }` at magic `+16`) that a blanket u32 swap
/// reversed (BE `04 00 00 01` → `01 00 00 04`). Copies the 4 bytes verbatim from
/// the BE source and sets `littleEndian = 1`. `be` and `out` must be the same length.
fn fix_embedded_havok_layoutrules(be: &[u8], out: &mut [u8]) {
    if be.len() != out.len() || be.len() < HAVOK_PACKFILE_MAGIC.len() {
        return;
    }
    let mut pos = 0usize;
    while pos + 20 <= be.len() {
        match be[pos..]
            .windows(HAVOK_PACKFILE_MAGIC.len())
            .position(|w| w == HAVOK_PACKFILE_MAGIC)
        {
            Some(rel) => {
                let m = pos + rel;
                if m + 20 <= be.len() {
                    out[m + 16..m + 20].copy_from_slice(&be[m + 16..m + 20]);
                    out[m + 17] = 1; // littleEndian
                }
                pos = m + 8;
            }
            None => break,
        }
    }
}

/// Resolve a descriptor's body slice within the original container.
fn body_range(container: &[u8], row_u0: u32, body_size: u32, data_area_off: usize) -> Option<(usize, usize)> {
    if row_u0 == 0xFFFFFFFF {
        return None; // sentinel
    }
    let start = if data_area_off > 0 {
        data_area_off + row_u0 as usize
    } else {
        8 + row_u0 as usize
    };
    let end = start + body_size as usize;
    if end <= container.len() {
        Some((start, end))
    } else {
        None
    }
}

/// Rewrite a kit model's `MIXR` from its big-endian source, at the widths its loader reads
/// (`mercs2_formats::mixr`): `INFO` three u16, `NAME` one u32 per part, `EXMX` bytes, `PROF` a u32 and
/// a byte per part for each profile. The generic pass knows those tags from other chunks. It runs
/// before anything reframes `out`, so a body sits at the same offset in both.
fn apply_mixr_swap(
    out: &mut [u8],
    container: &[u8],
    descriptors: &[Descriptor],
    data_area_off: usize,
) -> Result<(), String> {
    for (i, d) in descriptors.iter().enumerate().filter(|(_, d)| &d.tag_bytes_le == b"MIXR") {
        let kids = descriptors.get(i + 1..i + 1 + d.row_u4 as usize).ok_or("MIXR claims more children than rows")?;
        let tags: Vec<&[u8; 4]> = kids.iter().map(|k| &k.tag_bytes_le).collect();
        if tags != [b"INFO", b"NAME", b"EXMX", b"PROF"] {
            return Err(format!("MIXR children are {tags:?}"));
        }
        let mut ranges = Vec::with_capacity(4);
        for k in kids {
            let r = body_range(container, k.row_u0, k.body_size, data_area_off).ok_or("a MIXR child runs past the container")?;
            out[r.0..r.1].copy_from_slice(&container[r.0..r.1]);
            ranges.push(r);
        }
        let (info, name, prof) = (ranges[0], ranges[1], ranges[3]);
        if info.1 - info.0 != 6 {
            return Err(format!("MIXR INFO is {} bytes", info.1 - info.0));
        }
        let parts = read_u16_be(container, info.0 + 2) as usize;
        let profiles = read_u16_be(container, info.0 + 4) as usize;
        if name.1 - name.0 != 4 * parts || prof.1 - prof.0 != profiles * (4 + parts) {
            return Err(format!("MIXR of {parts} parts and {profiles} profiles: NAME {}, PROF {}", name.1 - name.0, prof.1 - prof.0));
        }
        (0..3).for_each(|w| swap_u16(out, info.0 + 2 * w));
        swap_u32_array(&mut out[name.0..name.1]);
        for p in 0..profiles {
            swap_u32(out, prof.0 + p * (4 + parts));
        }
    }
    Ok(())
}

/// Convert an `info` descriptor body outside an ECS container from BE to LE, in place.
///
/// Two on-disk shapes:
///   - ASCII/named: `[name\0][u32 hash][u32 a][u32 b][u32 c]` — swap only the trailing
///     u32 fields, leave the name string bytes untouched.
///   - Compact binary (no name, e.g. `[u32 comp_hash][u32][u32][u32]`) — swap all u32s.
///
/// Discriminator: a valid leading name is non-empty AND decodes as UTF-8 before the first
/// NUL. A 4-byte BE component hash regularly contains a 0x00 byte (e.g. `1D E5 C8 24` ->
/// "Name") so "has a NUL" is NOT enough; the bytes before it must also be valid text.
/// Mirrors `_convert_ecs_info` in tools/ucfx_be_to_le.py.
fn convert_info_body_inplace(info_body: &mut [u8]) {
    let nul_pos = info_body.iter().position(|&b| b == 0);
    match nul_pos {
        Some(np) if np > 0 && std::str::from_utf8(&info_body[..np]).is_ok() => {
            let u32_start = np + 1;
            let n_u32 = (info_body.len() - u32_start) / 4;
            for fi in 0..n_u32 {
                swap_u32(info_body, u32_start + fi * 4);
            }
        }
        _ => {
            // Compact binary (no leading name): every dword is a u32 to swap.
            swap_u32_array(info_body);
        }
    }
}

/// Convert a `schm` (component schema) body in place from BE to LE.
///
/// Layout (verified against retail PC `layers_static` / `vz_mar_roads`):
///   +0  u32 n_fields
///   +4  u32 payload_stride
///   +8  n_fields x 16-byte field entries:
///         +0  u32 type_code
///         +4  u32 name_hash
///         +8  u32 unk (always 0)
///         +12 offset word = { u16 byte_offset; u8 a; u8 b }
///
/// The trailing two bytes of the offset word are endian-neutral u8 fields
/// (bit index / size), so the word is NOT a u32. A full u32 swap moves the
/// `byte_offset` into the high 16 bits; the engine (and every retail PC
/// block) stores it in the low 16 bits. Swap only the `byte_offset` u16 and
/// leave the trailing two bytes in place. (swap-first-u16 reproduces retail
/// 47/47 on vz_mar_roads and 12/12 on layers_static; full u32 swap matches
/// only zero-offset fields.) Mirrors `_convert_schm_body` in ucfx_be_to_le.py.
pub(super) fn swap_schm_body_inplace(buf: &mut [u8]) {
    if buf.len() < 8 {
        swap_u32_array(buf);
        return;
    }
    let n_fields = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if n_fields > 200 || (8 + n_fields as usize * 16) > buf.len() {
        // Not a recognizable field table — preserve legacy behaviour.
        swap_u32_array(buf);
        return;
    }
    // Header: n_fields + payload_stride are plain u32.
    buf[0..4].reverse();
    buf[4..8].reverse();
    for i in 0..n_fields as usize {
        let off = 8 + i * 16;
        buf[off..off + 4].reverse(); // type_code
        buf[off + 4..off + 8].reverse(); // name_hash
        buf[off + 8..off + 12].reverse(); // unk
        // offset word: BE [b0,b1,b2,b3] -> LE [b1,b0,b2,b3]
        buf.swap(off + 12, off + 13);
    }
    // Any trailing bytes (none expected) are left untouched.
}

/// Convert bodies for non-ECS (generic) containers with tag-aware dispatch.
fn convert_generic_bodies(
    out: &mut Vec<u8>,
    container: &[u8],
    descriptors: &[Descriptor],
    data_area_off: usize,
    _is_be: bool,
    type_hash: u32,
    entry_idx: usize,
    mut report: Option<&mut SchemaCoverageReport>,
) -> Result<(), String> {
    let data_start = if data_area_off > 0 { data_area_off } else {
        20 + descriptors.len() * 20
    };

    if data_start >= container.len() {
        return Ok(());
    }

    let mut data_area = container[data_start..].to_vec();
    let is_texture = type_hash == types::TYPE_HASH_TEXTURE;

    let mut ctx = ContainerCtx::None;
    let mut group = ChunkTag::Unknown(*b"????");
    for desc in descriptors {
        if desc.row_u0 == 0xFFFFFFFF {
            group = desc.tag;
            // Group sentinel: open a container context (mirrors _classify_contexts).
            ctx = match desc.tag {
                ChunkTag::Strm | ChunkTag::Geom => ContainerCtx::Strm,
                ChunkTag::Ibuf => ContainerCtx::Ibuf,
                ChunkTag::Chdr | ChunkTag::Comp | ChunkTag::Stat | ChunkTag::Prmt => {
                    ContainerCtx::Meta
                }
                ChunkTag::Unknown(b) if b == *b"EXEC" => ContainerCtx::Meta,
                _ => ctx, // unrelated sentinel: keep current
            };
            continue;
        }

        if let Some((start, end)) = body_range(container, desc.row_u0, desc.body_size, data_area_off) {
            let body_local_start = start - data_start;
            let body_local_end = end - data_start;
            if body_local_end <= data_area.len() {
                match desc.tag {
                    ChunkTag::Syek | ChunkTag::Srts => {
                        // Native BE on all platforms — no swap
                    }
                    ChunkTag::Strs => {
                        convert_strs_inplace(&mut data_area[body_local_start..body_local_end], type_hash)?;
                    }
                    tag if is_string_tag(tag) => {
                        // String data — no swap
                    }
                    ChunkTag::Prmt => {
                        // Mesh draw-call records: 16-byte [u32 material_index]
                        // [u32 start_index][u16 index_count][u16 base_vertex]
                        // [u16 max_vertex_index][u16 vertex_span] (PRMT_WALKER).
                        // Old blanket swap_u16_array transposed the two leading u32s.
                        // walk_records bails to the count-safe u16 swap on any body
                        // that isn't a clean 16-byte multiple.
                        let body = &mut data_area[body_local_start..body_local_end];
                        if !walk_records(body, prmt_walker(type_hash)) {
                            swap_u16_array(body);
                        }
                    }
                    ChunkTag::Trns => {
                        // TRNS: NUL-terminated ASCII state-name strings ("Complete\0
                        // Subdued\0Idle\0"...). PC keeps them verbatim; the generic u32
                        // default reverses each 4-byte group ("Comp"->"pmoC"). No swap.
                        // (rosetta oracle: TRNS 8744 size-equal diffs -> 0.)
                    }
                    ChunkTag::Unknown(b) if b == *b"SEGM" => {
                        // SEGM = 4-byte records `[u16 bone-remap @0][u8 seg_index @2]
                        // [u8 group @3]`. The u16 bone-remap is big-endian and MUST be
                        // byte-swapped to LE; the two u8s stay put (so neither a blanket
                        // u32 swap nor "no swap" is right). The old "no swap" only looked
                        // correct on models whose bone-remap is 0 (a no-op); skinned
                        // characters with real remaps (mattias/jen/chris v2/v3/v4/...) got
                        // garbled per-segment bone palettes → those body parts skinned to
                        // junk bones → mangled posed mesh while remap=0 parts stayed fine.
                        // Verified swap-u16@0-per-record == retail oracle for 6 base
                        // character models; a no-op for the zero-remap models that
                        // previously "passed". Each record's u16 is the bone index.
                        let body = &mut data_area[body_local_start..body_local_end];
                        let mut o = 0;
                        while o + 4 <= body.len() {
                            body.swap(o, o + 1);
                            o += 4;
                        }
                    }
                    ChunkTag::Inst | ChunkTag::Bshi => {
                        // INST / BSHI: u16 record / index arrays. PC = per-u16 byte-swap of
                        // the BE source; the generic u32 default transposes each pair (e.g.
                        // BSHI {0,1,2,3} -> {1,0,3,2}). (oracle: INST 1020 + BSHI 186 -> 0.)
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Hier => {
                        // HIER: 176-byte (0xb0) node array. Per node: u32 node-hash @0,
                        // a u16 pair @4/@6 (index + parent, 0xffff = root), then f32
                        // transform matrix + bbox @8 (reversed as u32 — u16-swapping f32
                        // = NaN bboxes, the documented hazard). The OLD default u32 swap
                        // got the f32 right but TRANSPOSED the @4/@6 u16 pair (oracle:
                        // 2946 size-eq diffs). The per-node walker fixes only that pair.
                        let body = &mut data_area[body_local_start..body_local_end];
                        if !convert_hier_inplace(body) {
                            swap_u32_array(body); // odd length -> prior safe behaviour
                        }
                    }
                    ChunkTag::Trck => {
                        // TRCK: [u32 hash][u32 hash] then a u16 array (variable length).
                        convert_trck_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Ptms => {
                        // PTMS: 8-byte records [u32][u16][u16].
                        let body = &mut data_area[body_local_start..body_local_end];
                        if !walk_records(body, &PTMS_WALKER) {
                            swap_u32_array(body);
                        }
                    }
                    ChunkTag::Ptch => {
                        // PTCH: 56-byte records (f32 + u16 pair @0x34).
                        let body = &mut data_area[body_local_start..body_local_end];
                        if !convert_ptch_inplace(body) {
                            swap_u32_array(body);
                        }
                    }
                    ChunkTag::Mtrl => {
                        // Material: mixed u32/f32 header + u16 flags + u16 texture-count + u32 hash
                        // array. A blanket swap transposes the u16 count and the engine overruns its
                        // fixed 10-slot {hash,0xF011157A,0} record array (FUN_00858790, world-load
                        // AV 0x0084DD5B). Per-field swap — see convert_mtrl. Previously this tag had
                        // NO arm and fell through to the generic u32 swap (the divergence from the
                        // Python converter that caused the crash).
                        let records = if type_hash == types::TYPE_HASH_TERRAIN_MESH {
                            let info = descriptors
                                .iter()
                                .find(|d| d.tag == ChunkTag::InfoUpper)
                                .and_then(|d| body_range(container, d.row_u0, d.body_size, data_area_off))
                                .ok_or("a terrainmesh has no INFO")?;
                            Some(terrain_material_count(&container[info.0..info.1])?)
                        } else {
                            None
                        };
                        convert_mtrl(&mut data_area[body_local_start..body_local_end], records)?;
                    }
                    ChunkTag::Enum => {
                        // Enum definitions: mixed strings + u32 fields
                        convert_enum_body_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Ibuf => {
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Indx => {
                        // Mesh index buffer (inside GEOM): u16 array. Mirrors Python
                        // (`tag == "INDX" -> _convert_u16_array`); generic u32 transposes
                        // the index u16 pairs.
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Info if type_hash == types::TYPE_HASH_ANIMATION => {
                        // Animation lowercase `info`: u16 array (Python `info`+_TYPE_ANIMATION).
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Info => {
                        // Lowercase `info` in a non-ECS container: named/compact ECS info
                        // body (the generic u32 fallback corrupts named `info`).
                        convert_info_body_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::InfoUpper if is_texture => {
                        convert_texture_info(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::InfoUpper
                        if type_hash == types::TYPE_HASH_TERRAIN_MESH && group == ChunkTag::Prmg =>
                    {
                        convert_terrain_draw_group_info(&mut data_area[body_local_start..body_local_end])?;
                    }
                    ChunkTag::InfoUpper if type_hash == types::TYPE_HASH_MODEL => {
                        // Model INFO. Buffer at entry = RAW big-endian body. The generic
                        // u32 sweep is byte-exact for the 4B scalar and 60B all-f32 shapes,
                        // but WRONG for two shapes whose tails are u16 fields — a blanket
                        // 4-byte reversal TRANSPOSES each u16 pair:
                        //   72B model-bbox header : u32/f32 fields [0:64] + four trailing
                        //     u16 [64:72] (generic gave BE 0x0000FFFF -> 0xFFFF0000;
                        //     retail keeps each LE u16 in place).
                        //   56B PRMG/sub-mesh record : 6×u32 header [0:24] (flags, 2 hashes,
                        //     count) + a u16 (value,count) tail [24:56] (generic transposes
                        //     each pair).
                        // Verified raw-Xbox->retail across 10 base character models: 4B/60B/
                        // 72B are 100% byte-exact; 56B matches retail for every byte EXCEPT
                        // four u16s at off 30/42/50/52, which the PC asset compiler
                        // re-quantized from the mesh's vertex bounds (provably two-to-one,
                        // not derivable from the Xbox bytes — render-side bound hints). The
                        // u16-swap leaves those at the nearest valid value; the destructive
                        // pair-transposition (the engine-affecting corruption) is fixed.
                        // (Three independent RE agents converged on this exact transform.)
                        let body = &mut data_area[body_local_start..body_local_end];
                        match body.len() {
                            72 => {
                                swap_u32_array(&mut body[0..64]);
                                swap_u16_array(&mut body[64..72]);
                            }
                            56 => {
                                swap_u32_array(&mut body[0..24]);
                                swap_u16_array(&mut body[24..56]);
                            }
                            _ => swap_u32_array(body),
                        }
                    }
                    ChunkTag::InfoUpper if type_hash == types::TYPE_HASH_SCRIPT => {
                        // Script INFO: [u8][u16 name_len@1][u8×2][ASCII name][NUL][u8 cnt]
                        // [u16 flags@(5+name_len)] — swap only the two u16 fields; leave
                        // u8/ASCII. Mirrors Python `_convert_script_info`; the generic u32
                        // sweep scrambles the ASCII script name.
                        convert_script_info_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::InfoUpper if type_hash == TYPE_HASH_CFX_PACK => {
                        // CFX INFO: u32 prefix + zlib stream (copy deflate verbatim).
                        convert_cfx_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::InfoUpper if type_hash == types::TYPE_HASH_STANCE => {
                        // Stance/named-registry INFO = the dims triple
                        // [u16 keyDims][u16 totalDims][u16 count]. The generic u32 sweep
                        // reverses bytes[0:4] (transposing keyDims/totalDims) and LEAVES
                        // bytes[4:6], so the row count stays big-endian — e.g. BE 1036
                        // (0x040C) is read as LE 0x0C04 = 3076. The engine then walks 3075
                        // rows through ~1036 of real data into garbage, blowing past the
                        // fixed 1024-slot table in FUN_0067cfb0 → world-load livelock
                        // (@0x67D130). Swap each u16 in place (no transposition).
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Type if type_hash == types::TYPE_HASH_STANCE => {
                        // Stance/named-registry TYPE = the dimension-name table:
                        // totalDims × ([ASCII name]\0 [u16 field]). The generic u32 sweep
                        // reverses the ASCII in 4-byte groups ("Stance" -> "natS…"),
                        // scrambling the dimension names. Leave the ASCII + null and swap
                        // only each trailing u16.
                        convert_stance_type_names_inplace(
                            &mut data_area[body_local_start..body_local_end],
                        );
                    }
                    ChunkTag::Body if is_texture => {
                        // DXT compressed pixel data — leave as-is here; a BE texture's
                        // full DXT BODY is untiled afterward by apply_texture_untile.
                    }
                    ChunkTag::Schm | ChunkTag::Flgs => {
                        swap_u32_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Decl => {
                        // Leave raw BE here; translated + reframed afterwards by
                        // apply_decl_translate. The Xbox `decl` is a *format
                        // translation* (12B elements -> 8B D3DVERTEXELEMENT9), not
                        // a byte-swap — a blind u16/u32 swap yields an invalid decl
                        // (out-of-range Type -> engine "Unsupported data type" AV).
                    }
                    ChunkTag::Chdr => {
                        if type_hash == types::TYPE_HASH_MODEL {
                            // Mesh CHDR is {u32 property_hash, u32 count} — a
                            // compiled-expression header (CHDR+CEXE behaviour-tree
                            // records parsed by engine 0x004CF340). The placement
                            // {u16,u16,u32} swap half-swaps the hash
                            // (0x9DA97065 -> 0x70659DA9) → parser finds no match →
                            // NULL write at world load (0x004CF58B). Full u32 swap.
                            swap_u32_array(&mut data_area[body_local_start..body_local_end]);
                        } else {
                            convert_chdr_body_inplace(
                                &mut data_area[body_local_start..body_local_end],
                            );
                        }
                    }
                    ChunkTag::Deps => {
                        // DEPS format: [u8 count][u32 hash × count]
                        // Preserve the count byte, only swap the hash array
                        let body_len = body_local_end - body_local_start;
                        if body_len > 1 {
                            swap_u32_array(&mut data_area[body_local_start + 1..body_local_end]);
                        }
                    }
                    ChunkTag::Efct => {
                        // EFCT effect header: array of u16 fields (magic @ +2,
                        // count @ +14). A u32-word swap transposes each pair of
                        // u16s, moving 0x0226 to +0 and zeroing the +14
                        // sub-component count, crashing the effect loader on the
                        // first COLR append (AV @ 0x00493102). See Python
                        // _convert_efct_header / docs/spatial_hash_crash_analysis.md.
                        convert_efct_header_inplace(
                            &mut data_area[body_local_start..body_local_end],
                        );
                    }
                    ChunkTag::Emtr => {
                        // EMTR: 2-byte emitter count (genuine u16).
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Geom if type_hash == types::TYPE_HASH_EFFECT && body_local_end - body_local_start == 4 => {
                        // An emitter's GEOM: u16 shape index, u16 count. EMTR's GEOM children, a
                        // u32 count and its records, are longer unless the count is 0, which reads
                        // the same either way.
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    // SEGM/BSHI removed: they were lumped with HIER on the same (now
                    // disproven) "Python u16 group" reasoning. Default u32 is the
                    // known-good baseline; re-add a u16 arm only if a chunk is VERIFIED
                    // to hold u16 data (not f32) against the validator/retail.
                    ChunkTag::Binn => {
                        // Lua bytecode: leave RAW BE here; converted afterward by
                        // apply_binn_transcode (unluac disassemble→flip-endianness→
                        // reassemble — NOT a byte-swap; the body may resize). Mirrors
                        // the wavebank `data` no-op + apply_wavebank_transcode reframe.
                    }
                    ChunkTag::Minf => {
                        // MINF: [u32 hash][u16] (6 bytes). The old whole-body u16 swap
                        // transposed the u32 hash (oracle: @0 flagged). u32-swap the hash,
                        // u16-swap the rest.
                        let s = &mut data_area[body_local_start..body_local_end];
                        if s.len() >= 6 {
                            swap_u32(s, 0);
                            swap_u16_array(&mut s[4..]);
                        } else {
                            swap_u16_array(s);
                        }
                    }
                    ChunkTag::Unknown(b) if b == *b"evnt" => {
                        // evnt: [u32 count][per event: u32 timestamp + 2 NUL-strings].
                        // Swap count + each timestamp; ASCII strings stay. Mirrors
                        // Python `_convert_evnt_body`.
                        convert_evnt_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Unknown(b) if b == *b"trnm" => {
                        // trnm (track-name): [u16 count][u16 pad][u32 hashes...]. Mirrors
                        // Python `_convert_trnm_body`. A blanket u32 sweep transposes the
                        // count/pad u16 pair (the residual divergence under many entry
                        // types whose bodies carry a nested trnm sub-container).
                        convert_trnm_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Phy2 => {
                        // PHY2 is a Havok 5.5 collision packfile (u32 header +
                        // embedded packfile), NOT a u32 array. A blanket u32 swap
                        // scrambles the ASCII __classnames__ strings; the Havok
                        // loader then fails its by-name class lookup
                        // (STATUS_OBJECT_NAME_NOT_FOUND) and dereferences scrambled
                        // data → AV at mercenaries2.exe 0x00414B4C. Convert
                        // section-aware (see havok::convert_phy2_be_to_le).
                        let body = data_area[body_local_start..body_local_end].to_vec();
                        let conv = havok::convert_phy2_be_to_le(&body)
                            .map_err(|e| format!("PHY2 Havok convert (entry {entry_idx}): {e}"))?;
                        if conv.len() != body.len() {
                            return Err(format!(
                                "PHY2 Havok convert changed size {} -> {} (entry {entry_idx})",
                                body.len(),
                                conv.len()
                            ));
                        }
                        data_area[body_local_start..body_local_end].copy_from_slice(&conv);
                    }
                    ChunkTag::Data if type_hash == types::TYPE_HASH_ANIMATION => {
                        // Animation `data` is a Havok 5.5 packfile (magic at +0).
                        // A blanket u32 sweep scrambles its __classnames__ strings
                        // AND half-swaps the u16 bone-indices / u8 compressed
                        // bitstream buffers in __data__ → wrong counts → over-alloc
                        // → heap corruption that surfaces later (e.g. the allocator
                        // free-list AV 0x0084DD5B). Convert class-aware, matching
                        // Python's `data`+_TYPE_ANIMATION dispatch.
                        let body = data_area[body_local_start..body_local_end].to_vec();
                        let conv = havok::convert_havok_be_to_le(&body)
                            .map_err(|e| format!("animation Havok convert (entry {entry_idx}): {e}"))?;
                        if conv.len() != body.len() {
                            return Err(format!(
                                "animation Havok convert changed size {} -> {} (entry {entry_idx})",
                                body.len(),
                                conv.len()
                            ));
                        }
                        data_area[body_local_start..body_local_end].copy_from_slice(&conv);
                    }
                    ChunkTag::Data if type_hash == types::TYPE_HASH_WAVEBANK => {
                        // Wavebank `data` is the audio body. Leave it RAW BE here; it
                        // is transcoded (PCM byte swap / XMA, XMA2 and MP3 via ffmpeg →
                        // PC PCM16) and the container reframed by apply_wavebank_transcode
                        // afterward — the body resizes, so it can't be an in-place swap
                        // (mirrors the texture BODY path left raw for apply_texture_untile).
                    }
                    ChunkTag::Data if type_hash == types::TYPE_HASH_PATH => {
                        // Path `data`: first 8 bytes are 4 u16 count/index fields, the
                        // rest is a u32/f32 array. Mirrors Python `_TYPE_PATH`:
                        //   _convert_u16_array(body[:8]) + _convert_u32_array(body[8:]).
                        // A blanket u32 swap transposes the leading u16 pair (path data
                        // corruption); these path entries always rode the Python route
                        // (bundled in dlc01 level blocks) so the bug was never exercised.
                        let split = (body_local_start + 8).min(body_local_end);
                        swap_u16_array(&mut data_area[body_local_start..split]);
                        swap_u32_array(&mut data_area[split..body_local_end]);
                    }
                    ChunkTag::Data if type_hash == types::TYPE_HASH_LINEREGION => {
                        convert_lineregion_data(&mut data_area[body_local_start..body_local_end])?;
                    }
                    ChunkTag::Data if type_hash == TYPE_HASH_CFX_PACK => {
                        // CFX data: u32 prefix + zlib stream (copy deflate verbatim).
                        convert_cfx_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Data if ctx == ContainerCtx::Ibuf => {
                        // Index-buffer data under an IBUF group sentinel: u16 array
                        // (Python `data`+IBUF context). Generic u32 transposes the
                        // index u16 pairs.
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    other_tag => {
                        if let Some(ref mut rpt) = report {
                            // Tags the engine dispatches on but we have NOT yet
                            // validated as WAD chunks (or that belong to non-UCFX
                            // subsystems) get a loud "requires deeper investigation"
                            // entry instead of the quiet generic-fallback note.
                            match crate::tag_registry::needs_investigation(other_tag.as_bytes()) {
                                Some(info) => rpt.record_needs_investigation(
                                    entry_idx,
                                    &format!("{}", other_tag),
                                    info.subsystem.label(),
                                    info.note,
                                    desc.body_size,
                                ),
                                None => {
                                    let type_name = types::type_name_from_hash(type_hash);
                                    rpt.record_generic_fallback(
                                        entry_idx,
                                        type_hash,
                                        type_name,
                                        &format!("{}", other_tag),
                                        desc.body_size,
                                    );
                                }
                            }
                        }
                        swap_u32_array(&mut data_area[body_local_start..body_local_end]);
                    }
                }
            }
        }
    }

    out.extend_from_slice(&data_area);
    Ok(())
}

/// Tags whose bodies are string data (no swap needed).
fn is_string_tag(tag: ChunkTag) -> bool {
    matches!(tag, ChunkTag::Name)
}

/// A `stringdb`'s `STRS`: a u32 count of UTF-16 code units, then the heap of NUL-terminated
/// UTF-16 strings (`crate::stringdb`). A console table is big-endian in both, the heap included,
/// and PC reads the heap as UTF-16LE, so each code unit is swapped.
fn convert_strs_inplace(body: &mut [u8], type_hash: u32) -> Result<(), String> {
    if type_hash != types::TYPE_HASH_STRINGDB {
        return Err(format!(
            "STRS in a {} chunk: only a stringdb's STRS is known",
            types::type_name_from_hash(type_hash)
        ));
    }
    let units = body.get(..4).map(|w| u32::from_be_bytes([w[0], w[1], w[2], w[3]]) as usize);
    if units.and_then(|u| u.checked_mul(2)).map(|b| 4 + b) != Some(body.len()) {
        return Err(format!("STRS: a {}-byte body for a heap of {units:?} code units", body.len()));
    }
    swap_u32(body, 0);
    swap_u16_array(&mut body[4..]);
    Ok(())
}

// --- Byte swap primitives ---

pub(super) fn swap_u32(data: &mut [u8], offset: usize) {
    data[offset..offset + 4].reverse();
}

pub(super) fn swap_u16(data: &mut [u8], offset: usize) {
    data[offset..offset + 2].reverse();
}

/// Swap every aligned 2-byte word in a slice. Trailing odd byte is left untouched.
fn swap_u16_array(data: &mut [u8]) {
    let n = data.len() / 2;
    for i in 0..n {
        let off = i * 2;
        data[off..off + 2].reverse();
    }
}

// ---------------------------------------------------------------------------
//   Field-aware per-record byte-swap walker
//
//   Many UCFX chunks are ARRAYS of mixed-width records. A blanket u16/u32 swap
//   of the whole body transposes the minority field width (the "grab-first /
//   stamp-the-rest" bug). A `RecordWalker` is a declarative transcription of how
//   the ENGINE READER consumes a record (the field-by-field width sequence,
//   stride, and count source — see docs in the per-type specs). `walk_records`
//   interprets it, swapping each field at its native width. It is non-destructive:
//   it returns `false` WITHOUT mutating if the body does not match the declared
//   layout, so the caller can fall back to a count-safe swap.
// ---------------------------------------------------------------------------

/// Field width/semantics within a record.
#[derive(Clone, Copy, PartialEq)]
enum FieldKind {
    U32,
    U16,
}

/// One field of a record layout (offsets are implicit from order).
struct FieldSpec {
    width: u8,
    kind: FieldKind,
}

/// Where the record count comes from. (Sibling-INFO counts are not needed: the
/// per-chunk body length recovers the count for fixed-stride record arrays.)
#[allow(dead_code)]
enum CountSource {
    /// count = body_len / stride (records fill the body exactly).
    BodyLenDivStride,
    /// a single struct (or a fixed number of records).
    Fixed(usize),
    /// a big-endian u16 count at `offset` (read pre-swap), records follow it.
    U16PrefixAt(usize),
    /// a big-endian u32 count at `offset` (read pre-swap), records follow it.
    U32PrefixAt(usize),
}

/// A declarative per-record field-swap walker (the engine reader's field
/// sequence). `fields` order == on-wire layout; `stride` = record size.
struct RecordWalker {
    fields: &'static [FieldSpec],
    stride: usize,
    count: CountSource,
}

/// Apply a field-aware per-record byte-swap. Returns `false` WITHOUT mutating on
/// any layout mismatch (caller should fall back to a count-safe swap — never
/// `swap_u32_array` on a body that may hold a u16 count).
fn walk_records(data: &mut [u8], w: &RecordWalker) -> bool {
    if w.stride == 0 {
        return false;
    }
    debug_assert_eq!(
        w.fields.iter().map(|f| f.width as usize).sum::<usize>(),
        w.stride,
        "RecordWalker field widths must sum to stride",
    );
    // Resolve the record region [start, start + count*stride) and validate it
    // fits the body BEFORE mutating anything.
    let (start, count) = match w.count {
        CountSource::BodyLenDivStride => {
            if data.is_empty() || data.len() % w.stride != 0 {
                return false;
            }
            (0usize, data.len() / w.stride)
        }
        CountSource::Fixed(n) => match n.checked_mul(w.stride) {
            Some(sz) if sz <= data.len() => (0usize, n),
            _ => return false,
        },
        CountSource::U16PrefixAt(off) => {
            if off + 2 > data.len() {
                return false;
            }
            let n = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
            let body = off + 2;
            match n.checked_mul(w.stride).map(|sz| body.checked_add(sz)) {
                Some(Some(end)) if end <= data.len() => (body, n),
                _ => return false,
            }
        }
        CountSource::U32PrefixAt(off) => {
            if off + 4 > data.len() {
                return false;
            }
            let n =
                u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]) as usize;
            let body = off + 4;
            match n.checked_mul(w.stride).map(|sz| body.checked_add(sz)) {
                Some(Some(end)) if end <= data.len() => (body, n),
                _ => return false,
            }
        }
    };
    // Swap the prefix count field itself (validated above).
    match w.count {
        CountSource::U16PrefixAt(off) => swap_u16(data, off),
        CountSource::U32PrefixAt(off) => swap_u32(data, off),
        _ => {}
    }
    for ri in 0..count {
        let mut off = start + ri * w.stride;
        for f in w.fields {
            match f.kind {
                FieldKind::U32 => data[off..off + 4].reverse(),
                FieldKind::U16 => data[off..off + 2].reverse(),
            }
            off += f.width as usize;
        }
    }
    true
}

/// PRMT — per-PRMG draw-call records (16 bytes). Field layout from the engine
/// reader and `tools/ucfx_mesh_codec.py:540`:
///   u32 material_index, u32 start_index, u16 index_count, u16 base_vertex,
///   u16 max_vertex_index, u16 vertex_span.
/// The old `swap_u16_array` transposed the two leading u32s (verified via the
/// oracle: `00000700` -> `07000000`). NOTE: the trailing u16 counts often differ
/// from PC by *value* (the IBUF strip->list re-encode), so this fixes the field
/// FORMAT (the u32 transposition) but `size_eq_diff` may not reach 0 for meshes
/// whose index buffer PC re-encoded.
/// PRMT of a terrain chunk: 20-byte draw records, `docs/terrain_cell_format.md`:
///   u32 vertex shader, u32 material, u32 start_index, u16 prim_count, u16 min, u16 max, u16.
const TERRAIN_PRMT_WALKER: RecordWalker = RecordWalker {
    fields: &[
        FieldSpec { width: 4, kind: FieldKind::U32 },
        FieldSpec { width: 4, kind: FieldKind::U32 },
        FieldSpec { width: 4, kind: FieldKind::U32 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
    ],
    stride: TERRAIN_DRAW,
    count: CountSource::BodyLenDivStride,
};

/// Bytes of a terrain draw record.
const TERRAIN_DRAW: usize = 20;

/// The PRMT walker of a chunk of `type_hash`: a body in 16-byte multiples can hold 20-byte records.
fn prmt_walker(type_hash: u32) -> &'static RecordWalker {
    if type_hash == types::TYPE_HASH_TERRAIN_MESH || type_hash == types::TYPE_HASH_LOWRES_TERRAIN {
        &TERRAIN_PRMT_WALKER
    } else {
        &PRMT_WALKER
    }
}

const PRMT_WALKER: RecordWalker = RecordWalker {
    fields: &[
        FieldSpec { width: 4, kind: FieldKind::U32 },
        FieldSpec { width: 4, kind: FieldKind::U32 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
    ],
    stride: 16,
    count: CountSource::BodyLenDivStride,
};

/// PTMS — 8-byte records `[u32][u16][u16]`. The generic u32 default transposes
/// the @4/@6 u16 pair of each record (verified via the oracle inspector).
const PTMS_WALKER: RecordWalker = RecordWalker {
    fields: &[
        FieldSpec { width: 4, kind: FieldKind::U32 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
        FieldSpec { width: 2, kind: FieldKind::U16 },
    ],
    stride: 8,
    count: CountSource::BodyLenDivStride,
};

/// TRCK — variable-length: `[u32 hash @0][u32 hash @4]` then a u16 array. NOT
/// fixed-stride (observed 26 / 34 bytes). The two leading u32 hashes must be
/// u32-swapped; everything from +8 is u16. (Oracle inspector: only @0/@4 were
/// transposed — the u16 tail is already correct under a u16 swap.)
fn convert_trck_inplace(body: &mut [u8]) {
    if body.len() < 8 {
        swap_u16_array(body);
        return;
    }
    swap_u32(body, 0);
    swap_u32(body, 4);
    swap_u16_array(&mut body[8..]);
}

/// PTCH — 56-byte (0x38) records: f32 patch/transform data @0..0x34 then a u16
/// pair @0x34/@0x36. The generic u32 default transposes that trailing u16 pair.
/// Returns false (no mutation) on a body that isn't a clean 56-byte multiple.
fn convert_ptch_inplace(body: &mut [u8]) -> bool {
    const STRIDE: usize = 0x38;
    if body.is_empty() || body.len() % STRIDE != 0 {
        return false;
    }
    for rec in body.chunks_exact_mut(STRIDE) {
        let mut o = 0;
        while o + 4 <= 0x34 {
            rec[o..o + 4].reverse(); // f32 (reversed as u32)
            o += 4;
        }
        rec[0x34..0x36].reverse(); // u16
        rec[0x36..0x38].reverse(); // u16
    }
    true
}

/// HIER — 176-byte (0xb0) skeleton/node records. Per node: u32 node-hash @0, u16
/// @4, u16 @6 (index + parent pair), then f32 transform matrix + bbox @8 (reversed
/// as u32). 176 = 4 + 2 + 2 + 168(=42×4). A 45-element `FieldSpec` table would be
/// unwieldy, so this is the hand-written equivalent (the plan's escape hatch).
/// Returns false (no mutation) on a body that isn't a clean 176-byte multiple.
fn convert_hier_inplace(body: &mut [u8]) -> bool {
    const STRIDE: usize = 0xb0;
    if body.is_empty() || body.len() % STRIDE != 0 {
        return false;
    }
    for rec in body.chunks_exact_mut(STRIDE) {
        rec[0..4].reverse(); // u32 node hash
        rec[4..6].reverse(); // u16
        rec[6..8].reverse(); // u16
        rec[8..10].reverse(); // u16 (index)
        rec[10..12].reverse(); // u16 (parent, 0xffff = root)
        let mut o = 12; // 0xc
        while o + 4 <= STRIDE {
            rec[o..o + 4].reverse(); // f32 transform / bbox (reversed as u32)
            o += 4;
        }
    }
    true
}

/// Convert a Texture INFO body (34 bytes, mixed u16/u32 fields).
/// Layout verified from format_reference.md:
///   +0: u16 width, +2: u16 height, +4: u32, +8: u32,
///   +12: u16, +14: bytes[8] (fourcc and related — leave as-is),
///   +22: u32 total_size, +26: u32, +30: u32
/// If body is smaller than 34 bytes, fall back to u32 array.
fn convert_texture_info(body: &mut [u8]) {
    if body.len() < 34 {
        swap_u32_array(body);
        return;
    }
    swap_u16(body, 0);  // width
    swap_u16(body, 2);  // height
    swap_u32(body, 4);
    swap_u32(body, 8);
    swap_u16(body, 12);
    // bytes 14..22: fourcc and format data — leave as raw bytes
    swap_u32(body, 22); // total_size
    swap_u32(body, 26);
    swap_u32(body, 30);
}

// ── Xbox 360 texture untile + INFO rebuild ───────────────────────────
//
// What a rung's bytes are and how they split is `super::texture`; these two read one surface.

/// XGAddress2DTiledOffset returning a *block* index (gildor/Noesis form).
pub(super) fn tiled_block_index(x: usize, y: usize, width_blocks: usize, log_bpb: usize) -> usize {
    let aligned_w = (width_blocks + 31) & !31;
    let macro_ = ((x >> 5) + (y >> 5) * (aligned_w >> 5)) << (log_bpb + 7);
    let micro = ((x & 7) + ((y & 0xE) << 2)) << log_bpb;
    let offset = macro_ + ((micro & !0xF) << 1) + (micro & 0xF) + ((y & 1) << 4);
    (((offset & !0x1FF) << 3)
        + ((y & 16) << 7)
        + ((offset & 0x1C0) << 2)
        + (((((y & 8) >> 2) + (x >> 3)) & 3) << 6)
        + (offset & 0x3F))
        >> log_bpb
}

/// Untile one mip surface (block granularity) to linear block order.
pub(super) fn untile_surface(
    tiled: &[u8],
    width_blocks: usize,
    height_blocks: usize,
    texel_pitch: usize,
    log_bpb: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; width_blocks * height_blocks * texel_pitch];
    for j in 0..height_blocks {
        let row = j * width_blocks;
        for i in 0..width_blocks {
            let ti = tiled_block_index(i, j, width_blocks, log_bpb);
            let src = ti * texel_pitch;
            if src + texel_pitch > tiled.len() {
                continue;
            }
            let dst = (row + i) * texel_pitch;
            out[dst..dst + texel_pitch].copy_from_slice(&tiled[src..src + texel_pitch]);
        }
    }
    out
}

/// Byte-swap every 16-bit word (Xbox DXT block words are big-endian).
pub(super) fn swap16_blocks(data: &mut [u8]) {
    let n = data.len() / 2;
    for i in 0..n {
        data.swap(i * 2, i * 2 + 1);
    }
}

/// Xbox-360 vertex-fetch format byte (`(b >> 8) & 0xFF`) -> PC D3DDECLTYPE.
/// Derived + golden-tested against the retail PC oracle
/// (`tools/_decl_golden_test.py`).
fn xbox_decl_format_to_pc_type(fmt: u8) -> Option<u8> {
    match fmt {
        0x23 => Some(15), // FLOAT16_2
        0x21 => Some(16), // FLOAT16_4
        0x28 => Some(4),  // UBYTE4
        0x22 => Some(5),  // SHORT2
        0x20 => Some(8),  // D3DCOLOR
        _ => None,
    }
}

/// Convert a PS3-authored compact `decl` body (`03 00 ...`) into a PC
/// `D3DVERTEXELEMENT9` array.
///
/// The PS3 DLC01 ships a non-interleaved multi-stream vertex layout: one STRM
/// group per attribute, each with its own `info` (stride = per-stream, not
/// per-vertex), `data`, and `decl`. The `decl` body is a 2-byte header `03 00`
/// followed by a compact token sequence describing that one stream's attribute
/// set. The entire DLC01 PS3 inventory uses only 11 distinct patterns across
/// 7,806 descriptor rows; the per-pattern enumeration + Xbox-DOH oracle
/// citations live in `docs/_descriptor_walker_oracle.md`.
///
/// Because each PS3 STRM group describes only one slice of the full vertex,
/// its `decl` cannot by itself produce a usable PC decl: the PC engine's
/// vertex layout is single-stream interleaved and requires every attribute in
/// one record. Translating a single PS3 compact decl in isolation would emit
/// a geometry-less or partial decl, silently breaking the mesh. The honest
/// behaviour is to fail loud here and have the caller route the DLC via the
/// X360-DOH import path (the current production flow — see §4-§5 of
/// `docs/_dlc01_pipeline_readiness.md`), which already converts cleanly.
///
/// Returns `Err` on every input, with the recognised pattern and detected
/// stride (if the sibling `info` chunk is available via the data_area header)
/// annotated so callers can bucket the rejection accurately.
fn convert_decl_ps3_compact(be: &[u8]) -> Result<Vec<u8>, String> {
    debug_assert!(be.len() >= 2 && be[0] == 0x03 && be[1] == 0x00);
    let hex = {
        let mut s = String::with_capacity(be.len() * 2);
        for b in be {
            use std::fmt::Write;
            let _ = write!(&mut s, "{:02x}", b);
        }
        s
    };
    // Classify by observed catalogue (see docs/_descriptor_walker_oracle.md).
    // Each known pattern cites its Xbox-DOH oracle decl so a reader can see
    // what the PC conversion WOULD need to synthesise if the full multi-
    // stream merge pass is wired. Patterns are keyed by exact bytes — a
    // novel pattern falls through to the generic refusal below.
    let semantic = match hex.as_str() {
        // Terrain / lowres patterns (type_hash 0x7C569307 / 0x1602815C):
        "0300000304060304060a0201"
            => "terrain stride-14 ground (xbox-oracle: COLOR@8 D3DCOLOR + NORMAL@12 F16x4)",
        "0300000303060802060a0201030e0e04"
            => "terrain stride-22 (xbox-oracle: UV@8 F16x2 + NORMAL@12 F16x4 + TANGENT@16 F16x4)",
        "0300000303060802040a0304060e020103120e04"
            => "terrain stride-26 (xbox-oracle: UV@8 F16x2 + COLOR@12 D3DCOLOR + NORMAL@16 F16x4 + TANGENT@20 F16x4)",
        "0300000303060802040a0304060e0201"
            => "terrain stride-18 (xbox-oracle: UV@8 F16x2 + COLOR@12 D3DCOLOR + NORMAL@16 F16x4)",
        "0300000303060802060a0201"
            => "terrain stride-14 variant (xbox-oracle: UV@8 F16x2 + NORMAL@12 F16x4)",
        "0300000306060201"
            => "lowres-terrain stride-10 (xbox-oracle: NORMAL@8 F16x4)",
        // Mesh / foliage patterns (type_hash 0x5B724250 / 0x600B904E).
        // These are 1-to-many against Xbox-DOH decls at the same block/desc
        // index — the compact body alone cannot select which Xbox decl to
        // emit. See docs/_descriptor_walker_oracle.md §Mesh ambiguity.
        "03000802" => "mesh/foliage compact stride-4 (ambiguous: maps to 10 Xbox decls)",
        "0300080206040201" => "mesh compact stride-8 variant-a (xbox-oracle: UV@8 F16x2 + NORMAL@12 F16x4)",
        "0300080204040304" => "mesh/foliage compact stride-8 variant-b (ambiguous: maps to 9+ Xbox decls)",
        _ => "unrecognised PS3 compact decl pattern",
    };
    Err(format!(
        "PS3 compact decl detected (bytes=`{hex}`, {}): the PS3 DLC ships a \
         non-interleaved multi-stream vertex layout; one PS3 STRM group / `decl` describes \
         ONE attribute slice. The PC engine requires a single-stream interleaved \
         D3DVERTEXELEMENT9 array. See docs/_descriptor_walker_oracle.md — route this \
         block through the X360 DOH import path (dlc_port --x360-stfs <dlc01.doh>) \
         or implement the multi-STRM merge pass before translating here.",
        semantic
    ))
}

/// Standard Direct3D9 D3DDECLTYPE byte sizes (enum value -> bytes).
fn pc_d3ddecltype_size(t: u8) -> u16 {
    match t {
        1 | 7 | 10 | 12 | 16 => 8,
        2 => 12,
        3 => 16,
        _ => 4,
    }
}

/// Translate a console-authored vertex declaration (`decl` chunk) to a PC
/// `D3DVERTEXELEMENT9` array.
///
/// Two authored layouts are observed on-disk:
///
/// - **Xbox 360 fetch-decl** (`be[0] == 0x00`): 12B header + N×12B elements (BE
///   `u32 a, b, c`). END has `a>>16 == 0x00ff`. The 2,196 Xbox DLC01 blocks and
///   every retail Xbox mesh/terrain decl use this layout. Translation mirrors
///   `tools/ucfx_be_to_le._convert_decl` and is golden-tested byte-exact against
///   the retail PC oracle (`tools/_decl_golden_test.py`).
///
/// - **PS3 compact decl** (`be[0] == 0x03 && be[1] == 0x00`): a 2-byte header
///   followed by a per-STRM-group token sequence. Each STRM group ships one
///   attribute of a non-interleaved multi-stream vertex layout; the compact
///   bytes describe only that single stream (strides observed: 4/8/10/14/18/
///   22/26 bytes, vs Xbox single-stream strides 20/24/28/32/36 for the same
///   semantic meshes). Routed to `convert_decl_ps3_compact`, which pattern-
///   matches against the 11-pattern vocabulary catalogued (with per-pattern
///   Xbox-DOH oracle citations) in `docs/_descriptor_walker_oracle.md`.
///
/// PC output in both paths:
///   `8B header [0, 16]` + N×8B D3DVERTEXELEMENT9 (`u16 Stream, u16 Offset, u8
///   Type, u8 Method, u8 Usage, u8 UsageIndex`); END = D3DDECL_END (Type 17).
///
/// Fails loud on an unknown format byte rather than emitting a guessed type.
fn convert_decl(be: &[u8]) -> Result<Vec<u8>, String> {
    // PS3 compact decl: `03 00 ...`. The Xbox fetch-decl header always starts
    // with four zero bytes (`a` of the header record), so this gate is
    // unambiguous. See docs/_descriptor_walker_oracle.md.
    if be.len() >= 2 && be[0] == 0x03 && be[1] == 0x00 {
        return convert_decl_ps3_compact(be);
    }
    if be.len() < 12 {
        return Err(format!("decl body too small for a vertex declaration ({} bytes)", be.len()));
    }
    // Empty declaration: a sub-mesh with no vertex format of its own — the Xbox
    // source ships just the END element (12B `00ff0000 ffffffff 00000000`), no
    // header. Normal for reskins that reuse an existing mesh's vertex layout
    // (base-game Mattias has 15). Retail PC ships the bare 8-byte D3DDECL_END.
    if (read_u32_be(be, 0) >> 16) == 0x00ff {
        return Ok(vec![0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00]);
    }
    let mut out = Vec::with_capacity(be.len());
    out.extend_from_slice(&0u32.to_le_bytes()); // PC header word 0
    out.extend_from_slice(&16u32.to_le_bytes()); // PC header stride-gate = 16
    let mut pos = 12usize; // skip the 12-byte Xbox header
    // Offset auto-increments by element size WITHIN a stream, but resets to the
    // source offset (0) at the start of each new stream — matching retail.
    let mut off: u16 = 0;
    let mut prev_stream: Option<u16> = None;
    let mut n_elems = 0usize;
    let mut saw_end = false;
    while pos + 12 <= be.len() {
        let a = read_u32_be(be, pos);
        let b = read_u32_be(be, pos + 4);
        let c = read_u32_be(be, pos + 8);
        pos += 12;
        if (a >> 16) == 0x00ff {
            // PC D3DDECL_END: Stream=0x00ff, Offset=0, Type=17 (UNUSED)
            out.extend_from_slice(&0x00ffu16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&[17, 0, 0, 0]);
            saw_end = true;
            break;
        }
        // Xbox element: a = [stream:16][offset:16]; b byte1 = format; c byte1 =
        // Usage, c byte2 = UsageIndex. PC D3DVERTEXELEMENT9 = {Stream, Offset,
        // Type, Method, Usage, UsageIndex}. Multi-stream decls (blend-shape /
        // extra-texcoord channels) carry stream 1..N — must be preserved, else the
        // channels collapse onto stream 0 / UsageIndex 0 (duplicate semantic) and
        // D3D CreateVertexDeclaration returns E_FAIL.
        let stream = ((a >> 16) & 0xffff) as u16;
        let cur_off = if prev_stream != Some(stream) { (a & 0xffff) as u16 } else { off };
        let fmt = ((b >> 8) & 0xff) as u8;
        let typ = xbox_decl_format_to_pc_type(fmt).ok_or_else(|| {
            format!("unknown Xbox decl format byte 0x{:02X} (element b=0x{:08X})", fmt, b)
        })?;
        out.extend_from_slice(&stream.to_le_bytes()); // Stream (Xbox a>>16)
        out.extend_from_slice(&cur_off.to_le_bytes()); // Offset
        out.extend_from_slice(&[
            typ,                      // Type
            0,                        // Method
            ((c >> 16) & 0xff) as u8, // Usage    (Xbox c byte 1)
            ((c >> 8) & 0xff) as u8,  // UsageIndex (Xbox c byte 2)
        ]);
        off = cur_off + pc_d3ddecltype_size(typ);
        prev_stream = Some(stream);
        n_elems += 1;
    }
    // Fail loud rather than silently emit a geometry-less decl: a header-only or
    // unterminated declaration means the source was truncated/empty, and a
    // 0-element decl would silently drop the mesh's geometry.
    if n_elems == 0 || !saw_end {
        return Err(format!(
            "decl translation produced {} vertex element(s), END={}, from a {}-byte \
             source - the vertex declaration is truncated or empty; refusing to emit a \
             geometry-less decl (would silently drop the mesh)",
            n_elems, saw_end, be.len()
        ));
    }
    Ok(out)
}

/// Reframe a mesh container in `out`: translate every `decl` chunk from the
/// Xbox 12-byte-element format to the PC 8-byte `D3DVERTEXELEMENT9` array (a
/// shrink). The generic body pass leaves `decl` bodies as raw BE.
///
/// SIZE-PRESERVING, matching the Python reference (`_convert_container`, which
/// keeps every chunk's original `row_u0` and zero-fills the freed bytes — see
/// `tools/ucfx_be_to_le.py`): the translated (shorter) body is written in place,
/// the remainder of the original decl span is zero-padded, only the decl's
/// `body_size` field is updated, and NO later offsets shift. This keeps the
/// following `data`/`IBUF` vertex/index chunks at their original (16-aligned)
/// offsets — the engine reads via the descriptor table, and the GPU buffers
/// require that alignment. The previous compaction (splice + shift later
/// offsets) misaligned every chunk after the decl and diverged from both Python
/// and the retail PC layout.
fn apply_decl_translate(
    out: &mut Vec<u8>,
    descriptors: &[Descriptor],
    data_area_off: usize,
    desc_table_end: usize,
) -> Result<(), String> {
    let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };

    for (idx, d) in descriptors.iter().enumerate() {
        if d.tag != ChunkTag::Decl || d.row_u0 == 0xFFFFFFFF || d.body_size == 0 {
            continue;
        }
        let size_field = 20 + idx * 20 + 8;
        let body_size = d.body_size as usize;
        let abs = data_start + d.row_u0 as usize;
        if abs + body_size > out.len() {
            return Err(format!(
                "decl body [{}..{}] exceeds container ({})",
                abs, abs + body_size, out.len()
            ));
        }
        let be_body = out[abs..abs + body_size].to_vec();
        let new_body = convert_decl(&be_body)?;
        if new_body.len() > body_size {
            return Err(format!("decl translation grew {} -> {}", body_size, new_body.len()));
        }
        // Write the shorter body in place, zero-pad the freed tail of the span,
        // keep every other offset (and the container length) unchanged.
        out[abs..abs + new_body.len()].copy_from_slice(&new_body);
        for b in &mut out[abs + new_body.len()..abs + body_size] {
            *b = 0;
        }
        out[size_field..size_field + 4].copy_from_slice(&(new_body.len() as u32).to_le_bytes());
    }
    Ok(())
}

/// Per-element component size for a PC D3DDECLTYPE: 4 = FLOAT32/D3DCOLOR
/// (u32-swap), 2 = SHORT/FLOAT16/normalized-short (u16-swap), 1 = UBYTE (no swap).
fn decl_type_component_size(t: u8) -> u8 {
    match t {
        6 | 7 | 9 | 10 | 11 | 12 | 15 | 16 => 2, // SHORT2/4, *N shorts, FLOAT16_2/4
        5 | 8 => 1,                              // UBYTE4, UBYTE4N
        _ => 4,                                  // FLOAT1-4, D3DCOLOR, UDEC3/DEC3N, unknown
    }
}

/// Re-correct STRM vertex buffers whose FLOAT16/SHORT components were scrambled
/// by the generic body pass's blanket u32 swap.
///
/// The generic pass byte-swaps a mesh's `data` (vertex) chunk in 4-byte groups —
/// correct for FLOAT32 elements, but FLOAT16/SHORT elements are 2-byte components,
/// so a u32 swap *transposes each adjacent pair*: a terrain FLOAT16_4 position
/// (X,Y,Z,W) comes out (Y,X,W,Z). At format level that's a valid (huge/denormal)
/// f32 — which is why the validator/docs call it "non-fatal" — but the engine now
/// places this geometry in the world, so the scrambled coordinates break in-world
/// placement (the world-load streaming stall). Where a STRM group's vertex
/// declaration is pure-f16/short, we re-correct every 4-byte group by swapping its
/// two u16 halves, which *exactly undoes* the wrong u32 swap. FLOAT32 vertex decls
/// are left untouched (their u32 swap was already correct), so f32 meshes — the
/// large majority — are unaffected.
///
/// Runs AFTER `apply_decl_translate` so the decl is the translated PC
/// D3DVERTEXELEMENT9 array. Mesh-only by construction (no STRM group elsewhere).
/// NOTE: the STRM `decl` child can list only the trailing element, so we key off
/// the per-vertex stride (max element end) and treat the whole stride as f16 —
/// safe because the gate requires *every listed element* to be a u16 type.
fn apply_strm_vertex_fix(
    out: &mut Vec<u8>,
    descriptors: &[Descriptor],
    data_area_off: usize,
    desc_table_end: usize,
) {
    let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };
    let mut i = 0;
    while i < descriptors.len() {
        if descriptors[i].tag != ChunkTag::Strm || descriptors[i].row_u0 != 0xFFFFFFFF {
            i += 1;
            continue;
        }
        // Collect the decl + data + info children of this STRM group (until the next sentinel).
        let mut decl: Option<(usize, usize)> = None;
        let mut data: Option<(usize, usize)> = None;
        let mut info: Option<(usize, usize)> = None;
        let mut j = i + 1;
        while j < descriptors.len() {
            let cd = &descriptors[j];
            if cd.row_u0 == 0xFFFFFFFF {
                break;
            }
            let start = data_start + cd.row_u0 as usize;
            let size = cd.body_size as usize;
            if start + size <= out.len() {
                match cd.tag {
                    ChunkTag::Decl => decl = Some((start, size)),
                    ChunkTag::Data => data = Some((start, size)),
                    ChunkTag::Info => info = Some((start, size)),
                    _ => {}
                }
            }
            j += 1;
        }
        i = j;

        let (ds, dl, vs, vl) = match (decl, data) {
            (Some((ds, dl)), Some((vs, vl))) => (ds, dl, vs, vl),
            _ => continue,
        };
        if dl < 16 {
            continue;
        }
        // Parse the PC decl: stride (max element end) + whether every element is u16-type.
        let decl_bytes = &out[ds..ds + dl];
        let mut p = 8usize; // skip the 8-byte PC decl header
        let mut stride = 0usize;
        let mut n_elems = 0usize;
        let mut all_u16 = true;
        while p + 8 <= decl_bytes.len() {
            let stream = u16::from_le_bytes([decl_bytes[p], decl_bytes[p + 1]]);
            let typ = decl_bytes[p + 4];
            if stream == 0x00ff || typ == 17 {
                break;
            }
            let offset = u16::from_le_bytes([decl_bytes[p + 2], decl_bytes[p + 3]]) as usize;
            let end = offset + pc_d3ddecltype_size(typ) as usize;
            if end > stride {
                stride = end;
            }
            if decl_type_component_size(typ) != 2 {
                all_u16 = false;
            }
            n_elems += 1;
            p += 8;
        }
        if n_elems == 0 || !all_u16 || stride < 4 || stride % 4 != 0 || vl < stride {
            continue;
        }
        // If the STRM `info` stride differs from this (PC-decl) stride, the data is
        // STILL at its Xbox stride and will be widened + position-fixed by
        // rebuild_terrain_vertices (DEC3N normals/tangents widen to FLOAT16_4). The
        // all-u16 grouping correction here would then run at the WRONG (PC) stride
        // over Xbox-stride bytes, and rebuild's half-unswap would double-process the
        // position → Z/W transposed. (Only all-u16 static model submeshes hit this —
        // e.g. mattias_v5's waist props sub 13/14 thrown to Z=1.0 on the floor.) Skip
        // these; rebuild owns them. STRM already at PC stride (info==decl) proceed.
        if let Some((is, il)) = info {
            if il >= 8 {
                let info_stride =
                    u32::from_le_bytes([out[is + 4], out[is + 5], out[is + 6], out[is + 7]])
                        as usize;
                if info_stride != 0 && info_stride != stride {
                    continue;
                }
            }
        }
        let n_verts = vl / stride;

        // Data-driven SAFETY NET. The generic pass already u32-swapped this buffer.
        // The STRM `decl` child can list only a trailing u16 element while the
        // (unlisted) position element is FLOAT32 — and applying the f16 correction
        // to a genuine f32 vertex buffer CORRUPTS it (early world-load crash). So
        // only re-correct when the current FLOAT32 view of the position is garbage
        // (= scrambled half-floats). If every sampled vertex reads as a sane,
        // in-world float3, the vertices are genuinely FLOAT32 and we leave them.
        if stride >= 12 {
            let sane = |f: f32| f == 0.0 || (f.is_finite() && (1e-3..=1e6).contains(&f.abs()));
            let sample = n_verts.min(16);
            let mut all_f32_sane = sample > 0;
            for v in 0..sample {
                let o = vs + v * stride;
                let fx = f32::from_le_bytes([out[o], out[o + 1], out[o + 2], out[o + 3]]);
                let fy = f32::from_le_bytes([out[o + 4], out[o + 5], out[o + 6], out[o + 7]]);
                let fz = f32::from_le_bytes([out[o + 8], out[o + 9], out[o + 10], out[o + 11]]);
                if !(sane(fx) && sane(fy) && sane(fz)) {
                    all_f32_sane = false;
                    break;
                }
            }
            if all_f32_sane {
                continue; // genuine FLOAT32 vertices — do not touch
            }
        }

        // u16-correct every 4-byte group across each vertex stride (undo the wrong u32 swap).
        for v in 0..n_verts {
            let vb = vs + v * stride;
            let mut g = 0usize;
            while g + 4 <= stride {
                let o = vb + g;
                out.swap(o, o + 2);
                out.swap(o + 1, o + 3);
                g += 4;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Terrainmesh (mesh_A / 0x7C569307) Xbox-360 → PC re-encode
//
// PC retail terrainmesh is a genuine re-encode, ~+170 KB larger per mesh, not a
// byte-swap. The transform is per-chunk and reframes the container (bodies
// resize, later offsets shift). See docs/terrainmesh_reencode_implementation.md
// for the full diagnosis, the empirically-pinned DEC3N formula, and exactly
// which chunks are byte-exact vs only geometry-correct.
//
// Implemented here (gated on type_hash == TYPE_HASH_TERRAIN_MESH):
//   * STRM `info` stride rewrite (byte-exact): pc_stride = be_stride + 4×(#FLOAT16_4).
//   * STRM `data` vertex widening (byte-exact for POSITION/FLOAT16_2/D3DCOLOR;
//     geometry-correct for NORMAL/TANGENT via the DEC3N decode — see below).
//   * IBUF `data` de-strip (geometry-correct): Xbox tri-strip w/ 0xFFFF restart
//     → PC tri-list, + IBUF `info` index-count rewrite.
//   * MTRL per-record count-pair fix (engine-correct; MTRL is not byte-exact
//     because PC additionally drops sub-records / shrinks).
// ---------------------------------------------------------------------------

/// Encode an f32 as an IEEE-754 binary16 (half) bit pattern, round-to-nearest-
/// even — matches Rust `f32 as f16` / the `<e>` pack the oracle compares against.
fn f32_to_f16_bits(f: f32) -> u16 {
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let mut mant = (x & 0x007f_ffff) as i32;
    let exp = ((x >> 23) & 0xff) as i32;
    if exp == 0xff {
        // Inf/NaN
        return sign | 0x7c00 | (if mant != 0 { 0x0200 } else { 0 });
    }
    let mut e = exp - 127 + 15;
    if e >= 0x1f {
        // overflow → Inf
        return sign | 0x7c00;
    }
    if e <= 0 {
        // subnormal / underflow
        if e < -10 {
            return sign;
        }
        mant |= 0x0080_0000; // implicit 1
        let shift = 14 - e; // 14..24
        let half_mant = mant >> shift;
        // round to nearest even
        let rem = mant & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        let mut hm = half_mant as u16;
        if rem > halfway || (rem == halfway && (hm & 1) == 1) {
            hm += 1;
        }
        return sign | hm;
    }
    // normal
    let mut hm = (mant >> 13) as u16;
    let rem = mant & 0x1fff;
    let halfway = 0x1000;
    if rem > halfway || (rem == halfway && (hm & 1) == 1) {
        hm += 1;
        if hm == 0x0400 {
            // mantissa overflow → bump exponent
            hm = 0;
            e += 1;
            if e >= 0x1f {
                return sign | 0x7c00;
            }
        }
    }
    sign | ((e as u16) << 10) | hm
}

/// Decode an Xbox-360 DEC3N-packed normal/tangent (4 BE bytes, read here as a
/// big-endian u32) into a PC `FLOAT16_4` (4 little-endian half-floats; a normal's W
/// is 1.0, a tangent's W its handedness).
///
/// **Bit layout (pinned empirically from 24,282 base-terrainmesh NORMAL pairs;
/// mean angular error 0.05°, max 0.12° — see the implementation doc):**
///   * NORMAL (HEND3N, 11-11-10): X=sx(bits[0:11])/1023, Y=sx(bits[11:22])/1023,
///     Z=sx(bits[22:32])/511.
///   * TANGENT/BINORMAL (DEC3N, 10-10-10-2): X=sx(bits[0:10])/511,
///     Y=sx(bits[10:20])/511, Z=sx(bits[20:30])/511, W = −1 when sx(bits[30:32]) < 0,
///     else +1 (the handedness, ±1 as on retail PC terrain).
///   * A NORMAL's W = 1.0
/// The two share a stream-0 4-byte slot but use **different bit splits** — NORMAL is
/// Xbox HEND3N (11-11-10), TANGENT/BINORMAL is D3DDECLTYPE_DEC3N (10-10-10-2). Using
/// the normal's 11-11-10 split for a tangent garbles it (~125° error → broken
/// normal-mapped shader). `ten_ten_ten=true` selects the tangent layout. Verified vs
/// the retail base_v2 oracle: normal 11-11-10 = 4102/4102 within 1°, tangent 10-10-10
/// matches retail (0° on v0). The decoded vector is normalized, then re-encoded to
/// binary16 — geometry-correct, not byte-exact (≤1–2 ULP from the PC quantization).
fn dec3n_to_half4_le(u: u32, ten_ten_ten: bool) -> [u8; 8] {
    let sx = |v: u32, bits: u32| -> i32 {
        let m = (v & ((1 << bits) - 1)) as i32;
        let half = 1 << (bits - 1);
        if m >= half { m - (1 << bits) } else { m }
    };
    let (x, y, z) = if ten_ten_ten {
        (
            sx(u, 10) as f32 / 511.0,
            sx(u >> 10, 10) as f32 / 511.0,
            sx(u >> 20, 10) as f32 / 511.0,
        )
    } else {
        (
            sx(u, 11) as f32 / 1023.0,
            sx(u >> 11, 11) as f32 / 1023.0,
            sx(u >> 22, 10) as f32 / 511.0,
        )
    };
    let mag = (x * x + y * y + z * z).sqrt();
    let (nx, ny, nz) = if mag > 0.0 {
        (x / mag, y / mag, z / mag)
    } else {
        (0.0, 0.0, 0.0)
    };
    // A tangent's handedness is the sign of its top two bits, read as a signed 2-bit value: 0 and 1 are +1, -1 is
    // -1. On DLC01 terrain this gives every vertex the w its PS3 copy carries (`mercs2_probe terrain-vertex-compare`).
    let w = if ten_ten_ten && sx(u >> 30, 2) < 0 { -1.0 } else { 1.0 };
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&f32_to_f16_bits(nx).to_le_bytes());
    out[2..4].copy_from_slice(&f32_to_f16_bits(ny).to_le_bytes());
    out[4..6].copy_from_slice(&f32_to_f16_bits(nz).to_le_bytes());
    out[6..8].copy_from_slice(&f32_to_f16_bits(w).to_le_bytes());
    out
}

/// A parsed PC D3DVERTEXELEMENT9 (already translated by `apply_decl_translate`).
struct DeclElem {
    offset_pc: usize,
    typ: u8,
    /// D3DDECLUSAGE (3=NORMAL, 6=TANGENT, 7=BINORMAL, …). Selects the DEC3N bit
    /// layout for type-16 (FLOAT16_4) elements: NORMAL=11-11-10, else=10-10-10.
    usage: u8,
}

/// Parse a translated PC `decl` body into its element list + computed PC stride.
/// Returns `None` if the decl is malformed or has no usable elements.
fn parse_pc_decl(decl: &[u8]) -> Option<(Vec<DeclElem>, usize)> {
    if decl.len() < 8 {
        return None;
    }
    let mut elems = Vec::new();
    let mut stride = 0usize;
    let mut p = 8usize; // skip the 8-byte PC decl header
    while p + 8 <= decl.len() {
        let stream = u16::from_le_bytes([decl[p], decl[p + 1]]);
        let typ = decl[p + 4];
        if stream == 0x00ff || typ == 17 {
            break;
        }
        let usage = decl[p + 6]; // D3DVERTEXELEMENT9.Usage
        let offset = u16::from_le_bytes([decl[p + 2], decl[p + 3]]) as usize;
        // Only stream-0, non-position (offset>0) elements participate in the stream-0
        // vertex widening. Blend-shape UV streams (stream>0) live in separate BSHP
        // bodies; the off-0 position is the implicit 8-byte prefix handled directly by
        // rebuild_terrain_vertices. (Single-stream terrain with an unlisted position is
        // unaffected — it has neither stream>0 nor an off-0 listed element.)
        if stream != 0 || offset == 0 {
            p += 8;
            continue;
        }
        let end = offset + pc_d3ddecltype_size(typ) as usize;
        if end > stride {
            stride = end;
        }
        elems.push(DeclElem { offset_pc: offset, typ, usage });
        p += 8;
    }
    if elems.is_empty() {
        return None;
    }
    Some((elems, stride))
}

/// Rebuild one STRM vertex buffer from the Xbox-packed source stride to the wider
/// PC stride, driven by the (already-PC-translated) decl.
///
/// `src` is the generic-swapped Xbox vertex body (the blanket u32 swap already
/// ran over it). `be_stride` / `pc_stride` come from the STRM `info` chunks.
///
/// Element handling:
///   * POSITION (unlisted FLOAT16_4 @ off 0, 8B both): un-do the wrong u32 swap
///     (swap the two u16 halves of each 4-byte group) → byte-exact.
///   * FLOAT16_2 (type 15, 4B): same u16-half un-swap → byte-exact.
///   * FLOAT16_4 listed (type 16): the **Xbox source is 4 packed DEC3N bytes**,
///     widened to an 8B PC FLOAT16_4 via `dec3n_to_half4_le` → geometry-correct.
///   * D3DCOLOR / UBYTE4 (types 4/5, 4B): copy the 4 bytes verbatim. (The generic
///     u32 swap reversed them; the Xbox→PC D3DCOLOR mapping is itself a reversal,
///     so the net is the original BE order — which equals PC. Byte-exact.)
///   * anything else: copy `pc_size` bytes verbatim (u32 swap already applied).
fn rebuild_terrain_vertices(
    src: &[u8],
    be_stride: usize,
    pc_stride: usize,
    elems: &[DeclElem],
    n_verts: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; pc_stride * n_verts];
    // Build the Xbox-side offset for each PC element. Position occupies [0,8) on
    // both. Listed elements are placed in PC-offset order; on the Xbox side a
    // FLOAT16_4 is 4 bytes, everything else keeps its PC size.
    let mut sorted: Vec<&DeclElem> = elems.iter().collect();
    sorted.sort_by_key(|e| e.offset_pc);
    // Compute be offsets.
    let mut be_off = Vec::with_capacity(sorted.len());
    let mut cur = 8usize; // position prefix (8B on both)
    for e in &sorted {
        be_off.push(cur);
        let bsz = if e.typ == 16 { 4 } else { pc_d3ddecltype_size(e.typ) as usize };
        cur += bsz;
    }
    let half_unswap = |dst: &mut [u8], s: &[u8]| {
        // s,dst are 4-byte groups; undo the wrong u32 swap = swap the two u16 halves.
        // Generic pass turned BE [b0 b1 b2 b3] into [b3 b2 b1 b0]; we want PC
        // [b1 b0 b3 b2] (each u16 little-endian). From the current [b3 b2 b1 b0]:
        dst[0] = s[2];
        dst[1] = s[3];
        dst[2] = s[0];
        dst[3] = s[1];
    };
    for v in 0..n_verts {
        let so = v * be_stride;
        let dst_o = v * pc_stride;
        if so + be_stride > src.len() || dst_o + pc_stride > out.len() {
            break;
        }
        // Position: 8 bytes = two 4-byte groups, u16-half un-swap each.
        half_unswap(&mut out[dst_o..dst_o + 4], &src[so..so + 4]);
        half_unswap(&mut out[dst_o + 4..dst_o + 8], &src[so + 4..so + 8]);
        for (k, e) in sorted.iter().enumerate() {
            let bo = so + be_off[k];
            let po = dst_o + e.offset_pc;
            match e.typ {
                16 => {
                    // FLOAT16_4 normal/tangent: 4 packed DEC3N bytes → 8B FLOAT16_4.
                    // src bytes were u32-swapped by the generic pass; the original
                    // BE u32 = reverse of the 4 swapped bytes.
                    if bo + 4 <= src.len() {
                        let b = &src[bo..bo + 4];
                        let u = u32::from_le_bytes([b[0], b[1], b[2], b[3]]); // = BE-of-original
                        // NORMAL (usage 3) is HEND3N 11-11-10; TANGENT/BINORMAL use
                        // DEC3N 10-10-10-2. Wrong split garbles tangents (~125° error).
                        let half4 = dec3n_to_half4_le(u, e.usage != 3);
                        out[po..po + 8].copy_from_slice(&half4);
                    }
                }
                15 => {
                    // FLOAT16_2: 4B, u16-half un-swap.
                    if bo + 4 <= src.len() {
                        let mut tmp = [0u8; 4];
                        half_unswap(&mut tmp, &src[bo..bo + 4]);
                        out[po..po + 4].copy_from_slice(&tmp);
                    }
                }
                4 | 5 => {
                    // D3DCOLOR / UBYTE4 (4B): the PC component order equals the
                    // *generic-u32-swapped* Xbox bytes verbatim. Raw BE `00 fe 00 00`
                    // → generic swap `00 00 fe 00` == PC. (Investigation A's observed
                    // "rotate" `fecbcbcb→cbcbcbfe` was raw-BE vs PC; the generic pass
                    // already applies that reversal.) Byte-exact — copy src as-is.
                    if bo + 4 <= src.len() {
                        out[po..po + 4].copy_from_slice(&src[bo..bo + 4]);
                    }
                }
                _ => {
                    let sz = pc_d3ddecltype_size(e.typ) as usize;
                    if bo + sz <= src.len() && po + sz <= out.len() {
                        out[po..po + sz].copy_from_slice(&src[bo..bo + sz]);
                    }
                }
            }
        }
    }
    out
}

/// Convert an Xbox triangle strip (u16 indices, `0xFFFF` primitive-restart) into a
/// SINGLE PC degenerate-stitched triangle strip (`D3DPT_TRIANGLESTRIP` — the form
/// retail character models use), dropping the 0xFFFF markers. Adjacent strip
/// segments are joined with zero-area degenerate triangles; a parity pad ensures
/// every segment's first vertex lands at an EVEN strip position, so each PRMT
/// sub-range (drawn separately, which resets the GPU's winding alternation at its
/// start_index) renders with correct winding.
///
/// Returns (strip bytes, posmap) where `posmap[p]` = the output strip position of
/// the vertex at input position `p` (the current/next position for a 0xFFFF), used
/// to remap PRMT `{start_index, index_count}` from Xbox-strip into PC-strip space.
fn degen_stitch_with_map(src: &[u8]) -> (Vec<u8>, Vec<u32>) {
    let n = src.len() / 2;
    let idx: Vec<u16> = (0..n)
        .map(|i| u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]))
        .collect();
    let mut out: Vec<u16> = Vec::with_capacity(n + n / 4 + 8);
    let mut posmap: Vec<u32> = vec![0u32; n + 1];
    let mut in_segment = false;
    for p in 0..n {
        let x = idx[p];
        if x == 0xffff {
            posmap[p] = out.len() as u32;
            in_segment = false;
        } else {
            if !in_segment {
                if let Some(&last) = out.last() {
                    // Bridge previous segment -> this one: [last, x] (parity pad an
                    // extra `last` when the current length is odd) so the segment's
                    // first vertex (pushed below) lands at an even position.
                    let l = out.len();
                    out.push(last);
                    if l % 2 == 1 {
                        out.push(last);
                    }
                    out.push(x);
                }
                in_segment = true;
            }
            posmap[p] = out.len() as u32;
            out.push(x);
        }
    }
    posmap[n] = out.len() as u32;
    let mut bytes = Vec::with_capacity(out.len() * 2);
    for x in out {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    (bytes, posmap)
}

/// One terrain group's strip and draws in PC form, from the little-endian Xbox ones.
///
/// PC draws each `PRMT` record as its own strip with no primitive restart
/// (`docs/terrain_cell_format.md` §5.2), `[start, start + prim_count + 2)`. An Xbox draw's range holds
/// `0xFFFF` restarts, so each is rebuilt on its own: a restart becomes degenerate triangles, padded so
/// the next segment lands on an even position counted from the draw's start, which keeps its winding.
/// A range two records share is emitted once. A record of no triangles stays as it is.
fn terrain_strips(ibuf: &[u8], prmt: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let idx: Vec<u16> = ibuf.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    let (mut strip, mut draws) = (Vec::with_capacity(idx.len()), prmt.to_vec());
    let mut placed: std::collections::HashMap<(usize, usize), (u32, u16)> = std::collections::HashMap::new();
    for rec in draws.chunks_exact_mut(TERRAIN_DRAW) {
        let start = u32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]) as usize;
        let prims = u16::from_le_bytes([rec[12], rec[13]]) as usize;
        if prims == 0 {
            continue;
        }
        let (new_start, new_prims) = match placed.get(&(start, prims)) {
            Some(p) => *p,
            None => {
                let range = idx.get(start..start + prims + 2).ok_or_else(|| {
                    format!("a terrain draw of {prims} triangles at {start} runs past a strip of {}", idx.len())
                })?;
                let at = strip.len();
                for (i, seg) in range.split(|x| *x == 0xFFFF).filter(|s| !s.is_empty()).enumerate() {
                    if i > 0 {
                        let last = *strip.last().ok_or("a terrain segment follows nothing")?;
                        let odd = (strip.len() - at) % 2 == 1;
                        strip.push(last);
                        if odd {
                            strip.push(last);
                        }
                        strip.push(seg[0]);
                    }
                    strip.extend_from_slice(seg);
                }
                let count = u16::try_from((strip.len() - at).saturating_sub(2))
                    .map_err(|_| format!("a terrain draw at {start} grew past 65,535 triangles"))?;
                let p = (at as u32, count);
                placed.insert((start, prims), p);
                p
            }
        };
        rec[8..12].copy_from_slice(&new_start.to_le_bytes());
        rec[12..14].copy_from_slice(&new_prims.to_le_bytes());
    }
    Ok((strip.iter().flat_map(|x| x.to_le_bytes()).collect(), draws))
}

/// One terrain group's strip and draws in PC form, from the big-endian PS3 ones.
///
/// A PS3 `PRMT` record is `{u32 vertex shader, u32 material, u32 start, u32 index count, u16 min, u16 max}`
/// over a triangle list. Each range becomes a strip of the same triangles at the same winding
/// ([`crate::terrainmesh::stripify`]). A range two records share is emitted once. `vertex_shader` gives a
/// record's PC key. PS3 has no field for a PC record's last u16, which `FUN_004AAAB0`'s draws do not read:
/// it takes the distinct index count, Xbox's value on all 20,230 draws of the DLC's terrain.
pub(super) fn terrain_strips_of_lists(
    ibuf: &[u8],
    prmt: &[u8],
    vertex_shader: impl Fn(u32) -> u32,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    if prmt.len() % TERRAIN_DRAW != 0 {
        return Err(format!("a terrain PRMT of {} bytes is not {TERRAIN_DRAW}-byte records", prmt.len()));
    }
    let idx: Vec<u16> = ibuf.chunks_exact(2).map(|b| u16::from_be_bytes([b[0], b[1]])).collect();
    let (mut strip, mut draws) = (Vec::with_capacity(idx.len()), Vec::with_capacity(prmt.len()));
    let mut placed: std::collections::HashMap<(usize, usize), (u32, u16)> = std::collections::HashMap::new();
    for rec in prmt.chunks_exact(TERRAIN_DRAW) {
        let word = |o: usize| u32::from_be_bytes([rec[o], rec[o + 1], rec[o + 2], rec[o + 3]]);
        let (start, count) = (word(8) as usize, word(12) as usize);
        let list = idx
            .get(start..start + count)
            .filter(|_| count % 3 == 0)
            .ok_or_else(|| format!("a terrain draw of {count} indices at {start} is no list in {} indices", idx.len()))?;
        let (at, prims) = match placed.get(&(start, count)) {
            Some(p) => *p,
            None => {
                let tris: Vec<[u16; 3]> = list
                    .chunks_exact(3)
                    .map(|t| [t[0], t[1], t[2]])
                    .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
                    .collect();
                let run = crate::terrainmesh::stripify(&tris)?;
                let at = strip.len() as u32;
                let prims = u16::try_from(run.len().saturating_sub(2))
                    .map_err(|_| format!("a terrain draw at {start} grew past 65,535 triangles"))?;
                strip.extend_from_slice(&run);
                placed.insert((start, count), (at, prims));
                (at, prims)
            }
        };
        let distinct = list.iter().collect::<std::collections::HashSet<_>>().len() as u16;
        draws.extend_from_slice(&vertex_shader(word(0)).to_le_bytes());
        draws.extend_from_slice(&word(4).to_le_bytes());
        draws.extend_from_slice(&at.to_le_bytes());
        for field in [prims, *list.iter().min().unwrap_or(&0), *list.iter().max().unwrap_or(&0), distinct] {
            draws.extend_from_slice(&field.to_le_bytes());
        }
    }
    Ok((strip.iter().flat_map(|x| x.to_le_bytes()).collect(), draws))
}

/// Terrainmesh (`0x7C569307`) re-encode pass. Runs LAST (after the generic body
/// sweep, `apply_decl_translate`, and `apply_strm_vertex_fix`), gated on the
/// terrainmesh type_hash. Rebuilds the container's data area with PC-format chunk
/// bodies and reframes every descriptor offset.
///
/// Note this SUPERSEDES `apply_strm_vertex_fix` for terrainmesh STRM `data` (it
/// re-derives vertices from the generic-swapped source itself), and overrides
/// `apply_decl_translate`'s size-preserving decl handling by using the actual
/// translated decl length (PC ships the shrunk decl).
///
/// Returns `Ok(())`; on any structural surprise it bails out leaving `out`
/// unchanged from the prior passes (still a loadable byte-swap), never corrupting.
fn apply_terrainmesh_reencode(
    out: &mut Vec<u8>,
    descriptors: &[Descriptor],
    data_area_off: usize,
    desc_table_end: usize,
    // true = a terrain kind, whose every 20-byte PRMT draw is its own strip (`terrain_strips`);
    // false = a skinned character MODEL, which needs ONLY the STRM vertex widening + info-stride
    // rewrite (its IBUF is already a list, and its MTRL is already correct).
    terrain_draws: bool,
) -> Result<(), String> {
    let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };
    if data_start > out.len() {
        return Ok(());
    }

    // Snapshot the current (post-generic-swap / post-decl-translate) bodies, keyed
    // by descriptor index, then rebuild the data area chunk-by-chunk in row_u0
    // order. PC terrainmesh bodies are contiguous (zero gaps, no alignment pad),
    // verified across the worked corpus — so a simple concatenation reproduces the
    // PC layout shape.
    #[derive(Clone)]
    struct Body {
        idx: usize,
        old_u0: u32,
        bytes: Vec<u8>,
    }
    let mut bodies: Vec<Body> = Vec::new();
    for (idx, d) in descriptors.iter().enumerate() {
        if d.row_u0 == 0xFFFF_FFFF {
            continue; // container sentinel — no body
        }
        // Read the CURRENT row_u0 / body_size from `out`'s descriptor table, not the
        // stale BE `descriptors` values — earlier passes (apply_decl_translate)
        // already rewrote some sizes in place (e.g. decl shrinks). Row layout:
        // tag@0, row_u0@+4, body_size@+8; rows at 20 + idx*20.
        let row = 20 + idx * 20;
        let cur_u0 =
            u32::from_le_bytes([out[row + 4], out[row + 5], out[row + 6], out[row + 7]]);
        let cur_size =
            u32::from_le_bytes([out[row + 8], out[row + 9], out[row + 10], out[row + 11]]) as usize;
        if cur_u0 == 0xFFFF_FFFF {
            continue;
        }
        let abs = data_start + cur_u0 as usize;
        if abs + cur_size > out.len() {
            return Ok(()); // unexpected — bail, leave prior passes intact
        }
        bodies.push(Body { idx, old_u0: cur_u0, bytes: out[abs..abs + cur_size].to_vec() });
    }
    bodies.sort_by_key(|b| b.old_u0);

    // Identify STRM/IBUF groups so the per-stream `info` stride / index-count can be
    // rewritten alongside the `data` body re-encode. We walk the original
    // descriptor order (sentinels included) to pair each group's children.
    // Map: descriptor idx -> role within its group.
    // STRM group children: info(12B), decl, data. IBUF group children: info(4B), data.
    use std::collections::HashMap;
    let mut group_of: HashMap<usize, (ChunkTag, Vec<usize>)> = HashMap::new();
    {
        let mut k = 0usize;
        while k < descriptors.len() {
            let tag = descriptors[k].tag;
            if (tag == ChunkTag::Strm || tag == ChunkTag::Ibuf)
                && descriptors[k].row_u0 == 0xFFFF_FFFF
            {
                let mut kids = Vec::new();
                let mut j = k + 1;
                while j < descriptors.len() && descriptors[j].row_u0 != 0xFFFF_FFFF {
                    // only direct leaf children (until the next sentinel)
                    kids.push(j);
                    j += 1;
                }
                group_of.insert(k, (tag, kids));
                k = j;
            } else {
                k += 1;
            }
        }
    }

    // Build quick lookups: child idx -> (group_tag, child_tag-role)
    // We need, per STRM group: the decl (for the element list / strides) and the
    // info + data indices. Per IBUF group: info + data indices.
    // Compute a per-data-idx plan: how to transform its body.
    enum Plan {
        StrmData { be_stride: usize, pc_stride: usize, elems: Vec<DeclElem>, n_verts: usize },
        StrmInfoStride { pc_stride: u32 },
        IbufData,
        IbufInfoCount,
    }
    let mut plans: HashMap<usize, Plan> = HashMap::new();

    for (_g, (gtag, kids)) in &group_of {
        // find child roles by tag
        let mut info_idx = None;
        let mut decl_idx = None;
        let mut data_idx = None;
        for &ci in kids {
            match descriptors[ci].tag {
                ChunkTag::Info => info_idx = Some(ci),
                ChunkTag::Decl => decl_idx = Some(ci),
                ChunkTag::Data => data_idx = Some(ci),
                _ => {}
            }
        }
        if *gtag == ChunkTag::Strm {
            let (Some(ii), Some(di), Some(vi)) = (info_idx, decl_idx, data_idx) else { continue };
            // fetch bodies (current, post-swap) from the snapshot
            let info_body = bodies.iter().find(|b| b.idx == ii).map(|b| b.bytes.clone());
            let decl_body = bodies.iter().find(|b| b.idx == di).map(|b| b.bytes.clone());
            let (Some(info_body), Some(decl_body)) = (info_body, decl_body) else { continue };
            if info_body.len() < 12 {
                continue;
            }
            let be_stride = u32::from_le_bytes([info_body[4], info_body[5], info_body[6], info_body[7]]) as usize;
            let vcount = u32::from_le_bytes([info_body[8], info_body[9], info_body[10], info_body[11]]) as usize;
            let Some((elems, pc_stride)) = parse_pc_decl(&decl_body) else { continue };
            if be_stride == 0 || pc_stride == 0 || vcount == 0 {
                continue;
            }
            plans.insert(vi, Plan::StrmData { be_stride, pc_stride, elems, n_verts: vcount });
            plans.insert(ii, Plan::StrmInfoStride { pc_stride: pc_stride as u32 });
        } else if *gtag == ChunkTag::Ibuf {
            // Both terrainmesh AND character-model index buffers are Xbox triangle
            // strips with 0xFFFF primitive-restart, which D3D9 cannot parse (it reads
            // 0xFFFF as a literal vertex index -> exploded mesh). Both become strips with
            // no restart; their PRMT draw-call ranges follow them below.
            let (Some(ii), Some(vi)) = (info_idx, data_idx) else { continue };
            plans.insert(vi, Plan::IbufData);
            plans.insert(ii, Plan::IbufInfoCount);
        }
    }

    // Transform each body per its plan, recording new lengths. IBUF data must be
    // computed before its info (to know the new index count), so do a first pass
    // over data bodies, then patch the paired info bodies.
    // Map idx -> new bytes.
    let mut new_bytes: HashMap<usize, Vec<u8>> = HashMap::new();
    // record IBUF data new index count, keyed by the IBUF group, to patch info.
    // We pair info<->data within a group via the group kids.
    let mut ibuf_count_for_info: HashMap<usize, u32> = HashMap::new();
    // Build a map data_idx -> info_idx for IBUF groups.
    let mut ibuf_info_of_data: HashMap<usize, usize> = HashMap::new();
    // (original strip index count, posmap) per de-stripped IBUF, for PRMT remap.
    let mut ibuf_posmaps: Vec<(usize, Vec<u32>)> = Vec::new();
    for (_g, (gtag, kids)) in &group_of {
        if *gtag == ChunkTag::Ibuf {
            let mut info_idx = None;
            let mut data_idx = None;
            for &ci in kids {
                match descriptors[ci].tag {
                    ChunkTag::Info => info_idx = Some(ci),
                    ChunkTag::Data => data_idx = Some(ci),
                    _ => {}
                }
            }
            if let (Some(ii), Some(vi)) = (info_idx, data_idx) {
                ibuf_info_of_data.insert(vi, ii);
            }
        }
    }

    for b in &bodies {
        match plans.get(&b.idx) {
            Some(Plan::StrmData { be_stride, pc_stride, elems, n_verts }) => {
                let widened = rebuild_terrain_vertices(&b.bytes, *be_stride, *pc_stride, elems, *n_verts);
                new_bytes.insert(b.idx, widened);
            }
            Some(Plan::StrmInfoStride { pc_stride }) => {
                let mut info = b.bytes.clone();
                if info.len() >= 8 {
                    info[4..8].copy_from_slice(&pc_stride.to_le_bytes());
                }
                new_bytes.insert(b.idx, info);
            }
            // A terrain group's strip is rebuilt with its draws, below.
            Some(Plan::IbufData) if terrain_draws => {}
            Some(Plan::IbufData) => {
                // Character models render as a triangle STRIP (D3DPT_TRIANGLESTRIP, like retail
                // base_v2's degenerate-stitched strips): emitting a list for a model makes the
                // engine read list data as a strip -> spikes.
                let (out_idx, posmap) = degen_stitch_with_map(&b.bytes);
                let count = (out_idx.len() / 2) as u32;
                if let Some(&ii) = ibuf_info_of_data.get(&b.idx) {
                    ibuf_count_for_info.insert(ii, count);
                }
                ibuf_posmaps.push((b.bytes.len() / 2, posmap));
                new_bytes.insert(b.idx, out_idx);
            }
            _ => {}
        }
    }
    // Each terrain group's PRMT comes before its IBUF in row order.
    if terrain_draws {
        for (idx, d) in descriptors.iter().enumerate() {
            if d.tag != ChunkTag::Prmt || d.row_u0 == 0xFFFF_FFFF {
                continue;
            }
            let draws = bodies.iter().find(|b| b.idx == idx).ok_or("a terrain PRMT has no body")?;
            let &vi = ibuf_info_of_data.keys().filter(|v| **v > idx).min().ok_or("a terrain PRMT has no IBUF after it")?;
            let ibuf = bodies.iter().find(|b| b.idx == vi).ok_or("a terrain IBUF has no body")?;
            let (strip, prmt) = terrain_strips(&ibuf.bytes, &draws.bytes)?;
            ibuf_count_for_info.insert(ibuf_info_of_data[&vi], (strip.len() / 2) as u32);
            new_bytes.insert(vi, strip);
            new_bytes.insert(idx, prmt);
        }
    }
    // Patch IBUF info bodies with the new index count.
    for b in &bodies {
        if let Some(Plan::IbufInfoCount) = plans.get(&b.idx) {
            if let Some(&count) = ibuf_count_for_info.get(&b.idx) {
                let mut info = b.bytes.clone();
                if info.len() >= 4 {
                    info[0..4].copy_from_slice(&count.to_le_bytes());
                }
                new_bytes.insert(b.idx, info);
            }
        }
    }

    // Remap a model's PRMT draw-call records {start_index, index_count} from Xbox-strip index
    // space into the degenerate-stitched PC strip (the parity pad guarantees each range start
    // lands on an even strip position so the sub-range draw resets winding correctly). A
    // terrain group has no posmap: its draws are 20 bytes, rebuilt above. Record layout (16 bytes):
    //   u32 material_index, u32 start_index, u16 index_count, u16 base_vertex,
    //   u16 max_vertex_index, u16 vertex_span. start/count index into the paired
    //   IBUF; base/max/span are vertex-space (unchanged by the index reorder). Match
    //   each PRMT body to the IBUF whose original strip length is the tightest fit
    //   covering its max range, then map via that IBUF's posmap.
    for (idx, d) in descriptors.iter().enumerate() {
        if d.tag != ChunkTag::Prmt || d.row_u0 == 0xFFFF_FFFF || (d.body_size as usize) < 16 {
            continue;
        }
        let Some(snap) = bodies.iter().find(|b| b.idx == idx) else { continue };
        let mut body = snap.bytes.clone();
        let nrec = body.len() / 16;
        let mut max_range = 0usize;
        for r in 0..nrec {
            let s = u32::from_le_bytes([body[r * 16 + 4], body[r * 16 + 5], body[r * 16 + 6], body[r * 16 + 7]]) as usize;
            let c = u16::from_le_bytes([body[r * 16 + 8], body[r * 16 + 9]]) as usize;
            max_range = max_range.max(s + c);
        }
        let Some((_, posmap)) = ibuf_posmaps
            .iter()
            .filter(|(orig, pm)| *orig >= max_range && pm.len() > max_range)
            .min_by_key(|(orig, _)| *orig)
        else {
            continue;
        };
        for r in 0..nrec {
            let s = u32::from_le_bytes([body[r * 16 + 4], body[r * 16 + 5], body[r * 16 + 6], body[r * 16 + 7]]) as usize;
            let c = u16::from_le_bytes([body[r * 16 + 8], body[r * 16 + 9]]) as usize;
            let ns = posmap[s.min(posmap.len() - 1)];
            let ne = posmap[(s + c).min(posmap.len() - 1)];
            let new_count = ne.saturating_sub(ns).min(0xFFFF) as u16;
            body[r * 16 + 4..r * 16 + 8].copy_from_slice(&ns.to_le_bytes());
            body[r * 16 + 8..r * 16 + 10].copy_from_slice(&new_count.to_le_bytes());
        }
        new_bytes.insert(idx, body);
    }

    // Reassemble the data area in row_u0 order, recomputing offsets, and patch the
    // descriptor table (row_u0 + body_size) in `out`.
    let mut new_data: Vec<u8> = Vec::with_capacity(out.len());
    // new_u0[idx] = offset within data area
    let mut new_u0: HashMap<usize, u32> = HashMap::new();
    let mut new_len: HashMap<usize, u32> = HashMap::new();
    for b in &bodies {
        let off = new_data.len() as u32;
        let bytes = new_bytes.get(&b.idx).unwrap_or(&b.bytes);
        new_u0.insert(b.idx, off);
        new_len.insert(b.idx, bytes.len() as u32);
        new_data.extend_from_slice(bytes);
    }

    // Rewrite the header region [0..data_start] + new data area.
    let mut rebuilt = Vec::with_capacity(data_start + new_data.len());
    rebuilt.extend_from_slice(&out[..data_start]);
    rebuilt.extend_from_slice(&new_data);

    // Patch descriptor rows (row_u0 @ +4, body_size @ +8 within each 20B row at 20+idx*20).
    for (idx, d) in descriptors.iter().enumerate() {
        if d.row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let row = 20 + idx * 20;
        if let Some(&u0) = new_u0.get(&idx) {
            rebuilt[row + 4..row + 8].copy_from_slice(&u0.to_le_bytes());
        }
        if let Some(&ln) = new_len.get(&idx) {
            rebuilt[row + 8..row + 12].copy_from_slice(&ln.to_le_bytes());
        }
    }

    *out = rebuilt;
    Ok(())
}

/// Convert an `enum` body in-place from BE to LE.
///
/// Layout (verified from base game):
///   [u32 total_enum_count]
///   repeated total_enum_count times:
///     [null-terminated ASCII enum name]
///     [u32 name_hash]
///     [u32 value_count]
///     repeated value_count times:
///       [null-terminated ASCII value name]
///       [u32 value_hash]
///       [u32 ordinal]
pub(super) fn convert_enum_body_inplace(data: &mut [u8]) {
    if data.len() < 4 {
        return;
    }

    let total = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    swap_u32(data, 0);
    vlog!("      enum body: total_enum_count={}", total);

    let mut pos = 4usize;
    for _ in 0..total {
        if pos >= data.len() {
            break;
        }
        // Skip null-terminated enum name string
        match data[pos..].iter().position(|&b| b == 0) {
            Some(nul_rel) => pos += nul_rel + 1,
            None => break,
        }
        // Swap name_hash (u32)
        if pos + 4 > data.len() {
            break;
        }
        swap_u32(data, pos);
        pos += 4;
        // Swap value_count (u32)
        if pos + 4 > data.len() {
            break;
        }
        let val_count = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        swap_u32(data, pos);
        pos += 4;
        // Walk each value: [name\0] [u32 hash] [u32 ordinal]
        for _ in 0..val_count {
            if pos >= data.len() {
                break;
            }
            match data[pos..].iter().position(|&b| b == 0) {
                Some(nul_rel) => pos += nul_rel + 1,
                None => break,
            }
            if pos + 8 > data.len() {
                break;
            }
            swap_u32(data, pos);     // value_hash
            pos += 4;
            swap_u32(data, pos);     // ordinal
            pos += 4;
        }
    }
}

/// Convert a stance/named-registry `TYPE` body (the dimension-name table) BE→LE.
///
/// Layout (verified against retail vz.wad ActionTable):
///   repeated for each dimension:
///     [null-terminated ASCII name]   (e.g. "Stance", "Action", "AimState", …)
///     [u16 field]                    (a small per-dimension value, =4 in retail)
///
/// The ASCII names are endian-neutral; only the trailing u16 is swapped. A blanket
/// u32 sweep reverses the ASCII in 4-byte groups ("Stance"→"natS…"), scrambling the
/// dimension names the engine matches against.
fn convert_stance_type_names_inplace(data: &mut [u8]) {
    let mut pos = 0usize;
    while pos < data.len() {
        // Skip the null-terminated ASCII dimension name.
        match data[pos..].iter().position(|&b| b == 0) {
            Some(nul_rel) => pos += nul_rel + 1,
            None => break,
        }
        // Swap the trailing per-dimension u16 field.
        if pos + 2 > data.len() {
            break;
        }
        swap_u16(data, pos);
        pos += 2;
    }
}

/// Swap every aligned 4-byte word in a slice. Trailing bytes (< 4) are left untouched.
pub(super) fn swap_u32_array(data: &mut [u8]) {
    let n = data.len() / 4;
    for i in 0..n {
        let off = i * 4;
        data[off..off + 4].reverse();
    }
}

/// MTRL material body: per-field byte-swap matching the engine parser `FUN_00858790` @0x00858790.
///
/// On-wire layout (verified from the decompiled read sequence — 26 leading u32/f32, two u16s, then
/// `count` u32 hashes, then 2 trailing u32):
///   `[u32/f32 × 26 = 104B] [u16 flags @104] [u16 texture-count @106] [u32 hash × count @108] [u32 × 2 tail]`
///
/// The engine reads the count as a **u16** and writes `count` 12-byte `{hash, 0xF011157A, 0}`
/// records into a **FIXED 10-slot** embedded array at `material+0xac`. A blanket u32 swap (the old
/// Rust fall-through) **transposes that u16 count with its neighbour** — a valid count (≤10) becomes
/// garbage (e.g. `4 → 0x0400 = 1024`) and the engine overruns the material object into the pool
/// arena → world-load AV at 0x0084DD5B. (Python's blanket *u16* swap kept the count right but wrecked
/// the u32/f32 colour block — the opposite error. This per-field swap fixes both.)
///
/// A model's body holds records of this layout end to end, and one that does not tile fails. A
/// terrainmesh's holds `records` of them ([`terrain_material_count`]), then 256 bytes of 4-byte words
/// for each (retail PC tile `0xCA67E07B`: 267 records end at 39,652 of 108,004 bytes).
/// 26-dword (104-byte) colour/scalar preamble that precedes each material's flag word.
const MTRL_PRE: usize = 26 * 4; // 104
/// Texture slots `FUN_00858790` fills.
const MTRL_MAX_TEXTURES: usize = 10;
/// A terrainmesh's `INFO` holds six floats of bounds, then this word: its material count.
const TERRAIN_INFO_MATERIALS: usize = 0x18;
/// Bytes a terrainmesh's `MTRL` holds for each material after the records.
const TERRAIN_MATERIAL_TABLE: usize = 256;

/// Bytes of a terrain draw group's `INFO`.
const TERRAIN_DRAW_GROUP_INFO: usize = 44;

/// A terrain draw group's `INFO` (`docs/terrain_cell_format.md` §4.4): two u32 draw counts, then three
/// pass groups of `{u32 draw_count, u16 first_draw, u16 terrain_end, u16 alt_draw_count, u16 index}`.
/// A word swap transposes each u16 pair, and `FUN_004AAAB0` then draws past the group's records.
fn convert_terrain_draw_group_info(body: &mut [u8]) -> Result<(), String> {
    if body.len() != TERRAIN_DRAW_GROUP_INFO {
        return Err(format!("a terrain draw group INFO of {} bytes, not {TERRAIN_DRAW_GROUP_INFO}", body.len()));
    }
    swap_u32_array(&mut body[..8]);
    for tier in body[8..].chunks_exact_mut(12) {
        swap_u32(tier, 0);
        swap_u16_array(&mut tier[4..]);
    }
    Ok(())
}

/// Bytes of a `lineregion` `data` header, and of each of its points.
const LINEREGION_HEADER: usize = 8;
const LINEREGION_POINT: usize = 8;
/// The one `lineregion` kind any shipped region holds (625 of 625 on PC).
const LINEREGION_KIND_POLYGON: u16 = 2;

/// A `lineregion`'s `data`: four u16s, then `count` points of `{f32 x, f32 z}`. The loader
/// (`0x0045E8A0`, slot 4 of the lineregion vtable `0x00BAA8B0`) skips the u16 at +0, reads the count
/// at +2 and a signed u16 at +4, skips +6, then the points. A word swap of the header trades the
/// kind and the count, so every region reads as 2 points and no point is inside it. The kind is
/// one the loader skips, not checks; another kind is refused because no shipped region has one to
/// show its layout.
fn convert_lineregion_data(body: &mut [u8]) -> Result<(), String> {
    if body.len() < LINEREGION_HEADER {
        return Err(format!("a lineregion data of {} bytes, shorter than its header", body.len()));
    }
    let kind = read_u16_be(body, 0);
    let count = read_u16_be(body, 2) as usize;
    if kind != LINEREGION_KIND_POLYGON {
        return Err(format!("a lineregion of kind {kind}: only kind {LINEREGION_KIND_POLYGON} is known"));
    }
    if LINEREGION_HEADER + count * LINEREGION_POINT != body.len() {
        return Err(format!("a lineregion data of {} bytes for {count} points", body.len()));
    }
    swap_u16_array(&mut body[..LINEREGION_HEADER]);
    swap_u32_array(&mut body[LINEREGION_HEADER..]);
    Ok(())
}

/// The material count in a big-endian terrainmesh `INFO` body.
pub(super) fn terrain_material_count(info: &[u8]) -> Result<usize, String> {
    if info.len() < TERRAIN_INFO_MATERIALS + 4 {
        return Err(format!("terrainmesh INFO is {} bytes", info.len()));
    }
    Ok(read_u32_be(info, TERRAIN_INFO_MATERIALS) as usize)
}

fn convert_mtrl(body: &mut [u8], records: Option<usize>) -> Result<(), String> {
    let mut p = 0usize;
    let mut done = 0usize;
    while records.map_or(p < body.len(), |n| done < n) {
        done += 1;
        if p + MTRL_PRE + 4 > body.len() {
            return Err(format!("MTRL record at +{p}: the {}-byte body ends in its header", body.len()));
        }
        let count = read_u16_be(body, p + MTRL_PRE + 2) as usize;
        let end = p + 116 + count * 4;
        if count > MTRL_MAX_TEXTURES || end > body.len() {
            return Err(format!("MTRL record at +{p} names {count} textures in a {}-byte body", body.len()));
        }
        swap_u32_array(&mut body[p..p + MTRL_PRE]);
        swap_u16(body, p + MTRL_PRE);
        swap_u16(body, p + MTRL_PRE + 2);
        swap_u32_array(&mut body[p + MTRL_PRE + 4..end]);
        p = end;
    }
    if body.len() - p != records.unwrap_or(0) * TERRAIN_MATERIAL_TABLE {
        return Err(format!("MTRL: {} bytes after its {done} records", body.len() - p));
    }
    swap_u32_array(&mut body[p..]);
    Ok(())
}

/// EFCT particle-effect header: an array of u16 fields. Mirrors
/// `_convert_efct_header` in `tools/ucfx_be_to_le.py`.
///
/// Verified against real Xbox 360 DLC bytes (`blocks\dlc01\effects_P000_Q3`)
/// cross-checked with the retail PC oracle (`pc-game-vz.wad`): the constant
/// `0x0226` magic is a u16 at byte +2 in BOTH the big-endian source and the
/// little-endian output, and the `u16 @ +14` sub-component count likewise stays
/// in place. The count gates the engine effect loader's descriptor-array
/// allocation (`mercenaries2.exe` 0x00492AF0).
///
/// A whole-**u32**-word swap (the regression introduced in df9c418 and reverted
/// here) transposes the two halves of every u32, moving the `0x0226` magic to
/// byte +0 and zeroing the +14 count — the loader then allocates a zero-length
/// array (`[EDI+0x60]` NULL) and crashes on the first `COLR` record append (AV
/// write to 0x4 at 0x00493102). A per-field u16 swap preserves the field
/// positions exactly.
fn convert_efct_header_inplace(data: &mut [u8]) {
    swap_u16_array(data);
}

/// trnm (track-name) body: `[u16 count][u16 pad][u32 hashes...]`. Mirrors
/// `tools/ucfx_be_to_le.py::_convert_trnm_body`: swap the u16 count, leave the
/// 2 padding bytes byte-for-byte, u32-swap the hash array, leave any sub-u32
/// tail. A blanket u32 sweep would transpose the count/pad u16 pair.
fn convert_trnm_inplace(body: &mut [u8]) {
    if body.len() < 4 {
        swap_u16_array(body);
        return;
    }
    swap_u16(body, 0); // count; body[2..4] padding stays as-is
    swap_u32_array(&mut body[4..]); // complete u32 hashes; sub-u32 tail untouched
}

/// CFX pack type_hash (fonts/effects/resident sub-assets: u32 header + zlib).
const TYPE_HASH_CFX_PACK: u32 = 0xFE0E8320;

/// evnt body: `[u32 count][per event: u32 timestamp][2 × NUL-terminated string]`.
/// Swap the count and each timestamp; the ASCII strings are endian-neutral.
/// Mirrors `tools/ucfx_be_to_le.py::_convert_evnt_body`.
fn convert_evnt_inplace(body: &mut [u8]) {
    if body.len() < 4 {
        return;
    }
    let count = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
    swap_u32(body, 0);
    let mut pos = 4usize;
    for _ in 0..count {
        if pos + 4 > body.len() {
            break;
        }
        swap_u32(body, pos);
        pos += 4;
        for _ in 0..2 {
            while pos < body.len() && body[pos] != 0 {
                pos += 1;
            }
            if pos < body.len() {
                pos += 1; // skip the NUL
            }
        }
    }
}

/// Container context for chunk dispatch, mirroring Python `_classify_contexts`.
/// A group-sentinel descriptor (row_u0 == 0xFFFFFFFF) opens a context that the
/// following chunks inherit until the next container sentinel. The key use:
/// `data` under `IBUF` is a u16 index buffer, under `STRM`/elsewhere it is u32.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ContainerCtx {
    None,
    Strm,
    Ibuf,
    Meta,
}

/// Script INFO body: `[u8][u16 name_len @1][u8×2][ASCII name][NUL][u8 cnt][u16
/// flags @ 5+name_len]`. Swap ONLY the two u16 fields; u8 and ASCII are
/// endian-neutral. Mirrors `tools/ucfx_be_to_le.py::_convert_script_info`.
fn convert_script_info_inplace(body: &mut [u8]) {
    if body.len() < 4 {
        return;
    }
    let name_len = u16::from_be_bytes([body[1], body[2]]) as usize; // BE = true length
    swap_u16(body, 1); // name_len -> LE
    let flags_off = 5 + name_len;
    if flags_off + 2 <= body.len() {
        swap_u16(body, flags_off); // flags
    }
}

/// First offset of a zlib wrapper (`0x78` + valid CMF byte) in a CFX payload.
fn find_zlib_offset(data: &[u8]) -> Option<usize> {
    const CMFS: [u8; 8] = [0x01, 0x5E, 0x9C, 0xDA, 0x20, 0x7D, 0xBB, 0xFB];
    if data.len() < 2 {
        return None;
    }
    for i in 0..data.len() - 1 {
        if data[i] == 0x78 && CMFS.contains(&data[i + 1]) {
            return Some(i);
        }
    }
    None
}

/// CFX + zlib body: u32-swap the aligned prefix before the zlib stream, copy the
/// unaligned prefix remainder and the deflate stream verbatim (it's
/// endian-neutral). Mirrors `_convert_cfx_compressed_data`. Falls back to a u32
/// sweep if no zlib stream is found (rather than raising).
fn convert_cfx_inplace(body: &mut [u8]) {
    // The Scaleform `.gfx`/SWF payload is a platform-independent LITTLE-ENDIAN
    // file (magic CFX/CWS = zlib-compressed, GFX/FWS = raw; the 3-byte magic is
    // followed by a u8 version + LE u32 FileLength). It is BYTE-IDENTICAL on
    // Xbox and PC — verified: the Minimap (0x71A70B2A) inner blob is equal in
    // xbox-vz.wad and retail vz.wad, header included. So it must be copied
    // VERBATIM; only an engine prefix BEFORE the Scaleform magic (if any) is
    // byte-swapped. The previous logic swapped everything before the zlib
    // stream, which includes the `CFX\x08` magic+length header -> "\x08XFC" +
    // a transposed length, breaking the GFx loader (the Map/Minimap HUD
    // type-confusion world-load crash). See memory scaleformgfx-cfx-blind-swap.
    const MAGICS: [&[u8; 3]; 4] = [b"CFX", b"CWS", b"GFX", b"FWS"];
    let scan = body.len().min(64);
    let magic_off = (0..scan.saturating_sub(3))
        .find(|&i| MAGICS.iter().any(|m| &body[i..i + 3] == &m[..]));
    match magic_off {
        Some(p) => {
            // Swap only the engine prefix preceding the `.gfx` magic (4-aligned);
            // copy the Scaleform file (magic onward) byte-for-byte. For these HUD
            // assets the magic is at offset 0, so the whole body is verbatim.
            let prefix_end = p - (p % 4);
            swap_u32_array(&mut body[..prefix_end]);
        }
        // No recognizable Scaleform magic: keep the prior conservative behavior
        // (swap the u32 prefix before the zlib stream; leave the deflate verbatim).
        None => match find_zlib_offset(body) {
            Some(zoff) => {
                let prefix_end = zoff - (zoff % 4);
                swap_u32_array(&mut body[..prefix_end]);
            }
            None => swap_u32_array(body),
        },
    }
}

/// Swap the CHDR header as `{ u16 @+0 ; u16 @+2 ; u32 @+4 }`.
///
/// The engine chunk dispatcher (0x654940) reads the CHDR body as two `u16`
/// fields followed by a `u32` flags word — a single generic reader, so every
/// CHDR header uses this layout regardless of total body size. The `u16 @ +2`
/// is written to the process-global stride gate `[0x01176078]`; the Transform
/// record builder (0x0063D7C0) strides 42 only when that value is `>= 0x2A`,
/// otherwise 40. Reversing the first 8 bytes as two `u32` words *transposes*
/// the two `u16` fields, zeroing the gate → 40-byte strides → spatial-hash
/// access violation on save-load. See docs/spatial_hash_crash_analysis.md.
///
/// Swaps only the bytes that are present; callers handle bytes beyond +8.
fn swap_chdr_header_inplace(data: &mut [u8]) {
    if data.len() >= 2 {
        swap_u16(data, 0);
    }
    if data.len() >= 4 {
        swap_u16(data, 2);
    }
    if data.len() >= 8 {
        swap_u32(data, 4);
    }
}

/// CHDR bodies: 8-byte header scalars only when the descriptor spans a large region.
///
/// The header is `{ u16; u16; u32 }` in BOTH the small and large/guidmap
/// branches (the engine's CHDR reader is generic); only the handling of bytes
/// beyond the 8-byte header differs.
pub(super) fn convert_chdr_body_inplace(data: &mut [u8]) {
    if data.len() <= 16 {
        swap_chdr_header_inplace(data);
        // Words beyond the 8-byte header are plain u32 scalars.
        if data.len() > 8 {
            swap_u32_array(&mut data[8..]);
        }
    } else {
        // Large/guidmap CHDR: only the 8-byte header is CHDR-specific; the rest
        // of the region is reached via sibling enum/COMP/flgs descriptors and is
        // left untouched here (unchanged from prior behavior).
        swap_chdr_header_inplace(data);
    }
}

/// Walk a BE UCFX container's descriptor table, identifying all chunk tags (diagnostic only).
fn walk_container_tags(container: &[u8], entry_idx: usize) -> Result<(), String> {
    if container.len() < 20 {
        return Err(format!("Entry {} container too small", entry_idx));
    }

    let magic = &container[0..4];
    if magic != b"XFCU" && magic != b"UCFX" {
        return Err(format!("Entry {}: bad UCFX magic {:?}", entry_idx, magic));
    }

    let is_be = magic == b"XFCU";
    let read_u32: fn(&[u8], usize) -> u32 = if is_be { read_u32_be } else { crate::ffcs::read_u32_le };

    let data_area_off = read_u32(container, 4) as usize;
    let n_desc = read_u32(container, 16) as usize;

    if n_desc > 10000 {
        return Err(format!("Entry {}: implausible descriptor count {}", entry_idx, n_desc));
    }

    for di in 0..n_desc {
        let row_start = 20 + di * 20;
        if row_start + 20 > container.len() {
            break;
        }

        let mut tag_bytes = [0u8; 4];
        tag_bytes.copy_from_slice(&container[row_start..row_start + 4]);
        if is_be {
            tag_bytes.reverse();
        }

        let tag = ChunkTag::from_bytes(tag_bytes);
        let row_u0 = read_u32(container, row_start + 4);
        let body_size = read_u32(container, row_start + 8);

        let is_sentinel = row_u0 == 0xFFFFFFFF;

        if is_sentinel {
            vlog!("    desc[{}]: {} (group marker)", di, tag);
        } else {
            let body_off = if data_area_off > 0 {
                data_area_off + row_u0 as usize
            } else {
                8 + row_u0 as usize
            };

            if tag.is_native_be() {
                vlog!("    desc[{}]: {} @{} size={} [NATIVE BE - NO SWAP]",
                    di, tag, body_off, body_size);
            } else {
                vlog!("    desc[{}]: {} @{} size={}", di, tag, body_off, body_size);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        convert_block, convert_cfx_inplace, convert_container, convert_chdr_body_inplace, convert_decl,
        convert_efct_header_inplace, convert_info_body_inplace, convert_mtrl, convert_strs_inplace,
        convert_lineregion_data, convert_terrain_draw_group_info,
        fix_embedded_havok_layoutrules, types, HAVOK_PACKFILE_MAGIC,
    };

    // Build a synthetic BE MTRL body: [u32*26][u16 flags][u16 count][u32*count][u32*2 tail].
    fn make_mtrl_be(flags: u16, count: u16) -> Vec<u8> {
        let mut be = Vec::new();
        for i in 0..26u32 {
            be.extend_from_slice(&(0x01020304u32.wrapping_add(i)).to_be_bytes());
        }
        be.extend_from_slice(&flags.to_be_bytes());
        be.extend_from_slice(&count.to_be_bytes());
        for i in 0..count as u32 {
            be.extend_from_slice(&(0xAABBCC00u32 | i).to_be_bytes());
        }
        be.extend_from_slice(&0xDEADBEEFu32.to_be_bytes()); // tail 0
        be.extend_from_slice(&0xCAFEF00Du32.to_be_bytes()); // tail 1
        assert_eq!(be.len(), 116 + count as usize * 4);
        be
    }

    #[test]
    fn mtrl_u16_count_survives_swap() {
        let count: u16 = 4;
        let mut body = make_mtrl_be(0x1234, count);
        convert_mtrl(&mut body, None).unwrap();

        // THE invariant: the u16 texture-count @offset 106 must read back as 4 in LE, NOT transposed.
        // Transposing it (the old Rust u32 fall-through) makes the engine write `count` 12-byte
        // {hash,0xF011157A,0} records into a fixed 10-slot array -> overrun -> world-load AV 0x84DD5B.
        assert_eq!(
            u16::from_le_bytes([body[106], body[107]]),
            count,
            "MTRL u16 texture-count must survive the swap"
        );
        // flags u16 @104 survives as a u16
        assert_eq!(u16::from_le_bytes([body[104], body[105]]), 0x1234);
        // leading u32/f32 colour was full-width byte-reversed (NOT u16-transposed)
        assert_eq!(u32::from_le_bytes([body[0], body[1], body[2], body[3]]), 0x01020304);
        // first hash u32 @108 was full-width byte-reversed
        assert_eq!(u32::from_le_bytes([body[108], body[109], body[110], body[111]]), 0xAABBCC00);
        // trailing u32 @ 108+count*4 survived as a u32
        let t = 108 + count as usize * 4;
        assert_eq!(u32::from_le_bytes([body[t], body[t + 1], body[t + 2], body[t + 3]]), 0xDEADBEEF);
    }

    #[test]
    fn a_console_strs_heap_becomes_utf16le() {
        // "Hi\0" + "\0": 4 code units, big-endian.
        let mut body = 4u32.to_be_bytes().to_vec();
        body.extend_from_slice(&[0, b'H', 0, b'i', 0, 0, 0, 0]);
        convert_strs_inplace(&mut body, types::TYPE_HASH_STRINGDB).unwrap();
        assert_eq!(&body[..4], &4u32.to_le_bytes());
        assert_eq!(&body[4..], &[b'H', 0, b'i', 0, 0, 0, 0, 0]);
    }

    #[test]
    fn a_strs_whose_count_is_not_its_heap_or_not_of_a_stringdb_is_refused() {
        let mut body = 5u32.to_be_bytes().to_vec();
        body.extend_from_slice(&[0, b'H', 0, 0]);
        assert!(convert_strs_inplace(&mut body.clone(), types::TYPE_HASH_STRINGDB).is_err());
        body[3] = 2;
        assert!(convert_strs_inplace(&mut body, types::TYPE_HASH_TEXTURE).is_err());
    }

    #[test]
    fn a_terrain_draw_group_info_keeps_its_pass_group_u16s() {
        let mut body = [3u32.to_be_bytes(), 1u32.to_be_bytes()].concat();
        for (count, first, end, alt, index) in [(1u32, 0u16, 1u16, 1u16, 0u16), (1, 1, 2, 0, 1), (1, 2, 3, 0, 2)] {
            body.extend_from_slice(&count.to_be_bytes());
            for v in [first, end, alt, index] {
                body.extend_from_slice(&v.to_be_bytes());
            }
        }
        convert_terrain_draw_group_info(&mut body).unwrap();
        assert_eq!(&body[..8], &[3, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(&body[20..32], &[1, 0, 0, 0, 1, 0, 2, 0, 0, 0, 1, 0], "tier 1: count, first 1, end 2, alt 0, index 1");
        assert!(convert_terrain_draw_group_info(&mut body[..40]).is_err());
    }

    #[test]
    fn a_lineregion_keeps_its_kind_and_point_count_apart() {
        // Kind 2, 3 points, +4 = 0x0102, +6 = 0, then (x, z) per point.
        let mut body = [2u16, 3, 0x0102, 0].iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<u8>>();
        for v in [1.5f32, -2.0, 300.25, 4.0, -5.5, 6.0] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        let mut be = body.clone();
        convert_lineregion_data(&mut be).unwrap();
        assert_eq!(&be[..8], &[2, 0, 3, 0, 2, 1, 0, 0], "kind 2, count 3, the +4 u16 in place");
        assert_eq!(f32::from_le_bytes([be[16], be[17], be[18], be[19]]), 300.25);
        assert_eq!(f32::from_le_bytes([be[28], be[29], be[30], be[31]]), 6.0);

        assert!(convert_lineregion_data(&mut body.clone()[..24]).is_err(), "a body short of its points");
        body[1] = 3;
        assert!(convert_lineregion_data(&mut body).is_err(), "a kind no shipped region has");
    }

    #[test]
    fn mtrl_count_zero_ok() {
        let mut body = make_mtrl_be(0x0001, 0); // 116 bytes, no hashes
        convert_mtrl(&mut body, None).unwrap();
        assert_eq!(u16::from_le_bytes([body[106], body[107]]), 0);
        assert_eq!(u32::from_le_bytes([body[0], body[1], body[2], body[3]]), 0x01020304);
    }

    #[test]
    fn cfx_payload_copied_verbatim() {
        // Scaleform .gfx (CFX = zlib) is a platform-independent LITTLE-ENDIAN
        // file: convert_cfx_inplace must copy it byte-for-byte and NOT swap the
        // "CFX\x08" + LE-length header. The old code swapped it -> "\x08XFC",
        // truncating/breaking the Map/Minimap HUD (world-load type-confusion).
        // Oracle: the Minimap inner blob is byte-identical in xbox-vz.wad and
        // retail vz.wad, header included.
        let mut body = vec![
            b'C', b'F', b'X', 0x08, 0x01, 0xab, 0x00, 0x00, // CFX, ver, LE len=0xab01
            0x78, 0xda, 0xcc, 0xb8, 0x05, 0x58, // zlib deflate stream
        ];
        let orig = body.clone();
        convert_cfx_inplace(&mut body);
        assert_eq!(body, orig, "CFX .gfx body must be copied verbatim (no header swap)");
    }

    #[test]
    fn cfx_engine_prefix_swapped_payload_verbatim() {
        // If an engine u32 prefix precedes the Scaleform magic, swap ONLY that
        // prefix; the .gfx (magic onward) stays verbatim.
        let mut body = vec![
            0x01, 0x02, 0x03, 0x04, // 4-byte BE engine prefix -> swapped
            b'C', b'F', b'X', 0x08, 0x01, 0xab, 0x00, 0x00, 0x78, 0xda, 0xcc,
        ];
        convert_cfx_inplace(&mut body);
        assert_eq!(&body[0..4], &[0x04, 0x03, 0x02, 0x01], "engine prefix swapped");
        assert_eq!(
            &body[4..12],
            &[b'C', b'F', b'X', 0x08, 0x01, 0xab, 0x00, 0x00],
            "CFX header verbatim"
        );
        assert_eq!(&body[12..], &[0x78, 0xda, 0xcc], "zlib verbatim");
    }

    #[test]
    fn a_body_records_do_not_tile_fails() {
        assert!(convert_mtrl(&mut vec![0u8; 50], None).unwrap_err().contains("ends in its header"));
        let mut body = make_mtrl_be(0, 2);
        body.truncate(body.len() - 1);
        assert!(convert_mtrl(&mut body, None).unwrap_err().contains("names 2 textures"));
        assert!(convert_mtrl(&mut make_mtrl_be(0, 11), None).unwrap_err().contains("names 11 textures"));
    }

    #[test]
    fn a_terrain_body_is_its_records_then_a_table_for_each() {
        let mut body = make_mtrl_be(0, 1);
        for _ in 0..64 {
            body.extend_from_slice(&1.0f32.to_be_bytes());
        }
        convert_mtrl(&mut body, Some(1)).unwrap();
        assert_eq!(u16::from_le_bytes([body[106], body[107]]), 1);
        assert_eq!(&body[120..124], &1.0f32.to_le_bytes());
        assert_eq!(&body[372..376], &1.0f32.to_le_bytes());
        let mut short = make_mtrl_be(0, 1);
        short.extend_from_slice(&[0; 252]);
        assert!(convert_mtrl(&mut short, Some(1)).unwrap_err().contains("252 bytes after its 1 records"));
    }

    #[test]
    fn info_compact_swaps_all_u32s() {
        // Real BE compact `info` from xbox-vz.wad: [u32 comp_hash][u32 a][u32 b][u32 c].
        // The hash 0x1DE5C824 ("Name") is byte-reversed on disk as 1D E5 C8 24, and
        // fields a/b are small big-endian values (a=91, b=3). A 0x00 falls at offset 4,
        // so a naive "first NUL => name" parser leaves it un-swapped (the historical bug).
        let mut body = [
            0xfb, 0x31, 0xf1, 0xef, 0x00, 0x00, 0x00, 0x5b,
            0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00,
        ];
        convert_info_body_inplace(&mut body);
        assert_eq!(
            body,
            [
                0xef, 0xf1, 0x31, 0xfb, 0x5b, 0x00, 0x00, 0x00,
                0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
            "compact info must swap every u32 (hash + fields), not bail on the inner NUL"
        );

        // Value preserved: the converted hash read little-endian equals the source hash
        // read big-endian (the engine now reads the true hash instead of a byte-reversed one).
        assert_eq!(u32::from_le_bytes([body[0], body[1], body[2], body[3]]), 0xFB31F1EF);
    }

    #[test]
    fn info_named_swaps_only_trailing_u32s() {
        // ASCII/named form: [name\0][u32 hash][u32 a][u32 b][u32 c]; the name bytes must
        // be left untouched, only the four trailing u32s swapped.
        let mut body = Vec::from(&b"Name\x00"[..]);
        body.extend_from_slice(&0x24c8e51du32.to_be_bytes()); // hash (BE on disk)
        body.extend_from_slice(&1u32.to_be_bytes());
        body.extend_from_slice(&101u32.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        let mut got = body.clone();
        convert_info_body_inplace(&mut got);
        let mut want = Vec::from(&b"Name\x00"[..]);
        want.extend_from_slice(&0x24c8e51du32.to_le_bytes());
        want.extend_from_slice(&1u32.to_le_bytes());
        want.extend_from_slice(&101u32.to_le_bytes());
        want.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(got, want, "named info must preserve the name and swap only trailing u32s");
    }

    #[test]
    fn entry_table_preserves_field_c() {
        // The block-level entry table is 16-byte rows {name_hash, type_hash, field_c, chunk_size}.
        // The DLC's multi-LOD combined-texture blocks carry a NONZERO per-entry field_c (an
        // offset the loader uses to locate each texture, e.g. 0x60D7). The old converter
        // hard-zeroed it (`&0u32.to_le_bytes()`), which mislocated the texture-component object
        // pointer → ECS component-table held a record-interior pointer → world-load crash
        // (FUN_007E0420 wild vcall / grid-pop 0x4CC064). field_c must be preserved+byteswapped.
        fn be32(v: u32) -> [u8; 4] {
            v.to_be_bytes()
        }
        // Minimal BE container: XFCU header with n_desc=0, plus an 8-byte CSUM trailer.
        let mut container = Vec::new();
        container.extend_from_slice(b"XFCU"); // BE magic
        container.extend_from_slice(&be32(20)); // data_area_off = 20 (after header)
        container.extend_from_slice(&be32(0)); // unk_08
        container.extend_from_slice(&be32(0)); // unk_0c
        container.extend_from_slice(&be32(0)); // n_desc = 0
        container.extend_from_slice(b"CSUM"); // CSUM trailer (stripped before convert)
        container.extend_from_slice(&0u32.to_le_bytes());
        let chunk_size = container.len() as u32;

        let name_hash = 0xAABBCCDDu32;
        let type_hash = 0x12345678u32; // generic (not texture/ECS) → trivial convert
        let field_c = 0x000060D7u32; // the load-bearing nonzero value

        let mut block = Vec::new();
        block.extend_from_slice(&be32(1)); // entry_count = 1
        block.extend_from_slice(&be32(name_hash));
        block.extend_from_slice(&be32(type_hash));
        block.extend_from_slice(&be32(field_c));
        block.extend_from_slice(&be32(chunk_size));
        block.extend_from_slice(&container);

        let out = convert_block(&block, false, None, None, None, None).expect("convert_block must succeed");

        // LE entry table row 0: [0..4]=count, [4..8]=name, [8..12]=type, [12..16]=field_c.
        assert_eq!(
            u32::from_le_bytes([out[12], out[13], out[14], out[15]]),
            field_c,
            "entry-table field_c must be preserved, not zeroed"
        );
        assert_eq!(u32::from_le_bytes([out[4], out[5], out[6], out[7]]), name_hash);
        assert_eq!(u32::from_le_bytes([out[8], out[9], out[10], out[11]]), type_hash);

        // Regression guard: a zero field_c (the old bug) would have failed the assert above.
        // Also confirm base-game blocks (field_c == 0) stay 0 — swapping 0 is a no-op.
        let mut block0 = block.clone();
        block0[12..16].copy_from_slice(&be32(0)); // field_c = 0
        let out0 = convert_block(&block0, false, None, None, None, None).expect("convert_block must succeed");
        assert_eq!(
            u32::from_le_bytes([out0[12], out0[13], out0[14], out0[15]]),
            0,
            "base-game field_c==0 must remain 0"
        );
    }

    #[test]
    fn decl_translate_matches_retail() {
        // REAL base-game Xbox 360 mesh `decl` (blocks/vz/c30001_p000_q3.block),
        // big-endian source as extracted from xbox-vz.wad:
        let be: [u8; 60] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x2c, 0x23, 0x5f, 0x00, 0x05, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x0c, 0x00, 0x2a, 0x21, 0x90, 0x00, 0x03, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x10, 0x00, 0x1a, 0x21, 0x87, 0x00, 0x06, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ];
        // Byte-exact retail PC decl (pc-game vz.wad) it must reproduce: header
        // [0, 16] + 3 D3DVERTEXELEMENT9 (Off 8/12/20, Type 15/16/16, Use 5/3/6)
        // + D3DDECL_END. Offsets are cumulative by D3DDECLTYPE size, NOT the Xbox
        // slot value (which is index*4+8).
        let pc: [u8; 40] = [
            0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00,
            0x00, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x14, 0x00, 0x10, 0x00, 0x06, 0x00,
            0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00,
        ];
        assert_eq!(convert_decl(&be).unwrap(), pc, "Xbox->PC decl must match retail");

        // A blind u32 swap (the old behaviour) does NOT reproduce retail.
        let mut blind = be;
        for ch in blind.chunks_exact_mut(4) {
            ch.reverse();
        }
        assert_ne!(&blind[..], &pc[..]);

        // Unknown format byte must error, never silently corrupt.
        let bad: [u8; 24] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x99, 0x99, 0x99, 0x00, 0x05, 0x00, 0x00,
        ];
        assert!(convert_decl(&bad).is_err(), "unknown format byte must error");

        // Header-only / truncated decl must FAIL LOUD, not emit a geometry-less
        // decl that silently drops the mesh (the skinned-mesh truncation case).
        let hdr_only = [0u8; 12];
        assert!(convert_decl(&hdr_only).is_err(), "header-only decl must error");
        // Elements but no END terminator (truncated tail) must also error.
        let no_end: [u8; 24] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x2c, 0x23, 0x5f, 0x00, 0x05, 0x00, 0x00,
        ];
        assert!(convert_decl(&no_end).is_err(), "unterminated decl must error");

        // Empty declaration (reskin sub-mesh, no own vertex format): the Xbox
        // source is just the END element (12B), no header. It must NOT error —
        // it maps to retail's bare 8-byte D3DDECL_END, exactly as base-game
        // Mattias's 15 empty decls do. Distinct from a truncation (above): the
        // END marker is present, so geometry is intentionally absent, not lost.
        let empty: [u8; 12] = [
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(
            convert_decl(&empty).unwrap(),
            [0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00],
            "empty Xbox decl must map to retail's bare PC D3DDECL_END"
        );
    }

    /// The PS3 DLC01 ships compact `03 00 ...` decls (per-STRM-group, one
    /// attribute per stream of a non-interleaved multi-stream vertex layout).
    /// Pre-fix, convert_decl classified these as either "body too small"
    /// (<12B) or "produced 0 elements" (12B parsed as a header with no END
    /// marker). Post-fix, the 0x03/0x00 header is recognised and the
    /// `convert_decl_ps3_compact` arm fires with a PS3-specific message that
    /// names the pattern and routes the operator to the X360-DOH import.
    ///
    /// Fixtures taken from the DLC01 PS3 SCFF
    /// (ps3_dlc01_be.scff, sha256 cc58b68d614786eb..., census in
    /// scratchpad/descriptor_walker_oracle/ps3_decl_census/decl_census.txt).
    #[test]
    fn ps3_compact_decl_terrain_stride14_ground_detected() {
        // Terrain stride-14 ground (type_hash 0x7C569307, 1,296 descriptors):
        // block[152] desc[10] parent=STRM, bytes `03 00 00 03 04 06 03 04 06
        // 0a 02 01`. The Xbox-DOH equivalent at the same block/desc index is
        // COLOR @ 8 D3DCOLOR + NORMAL @ 12 F16x4.
        let be: [u8; 12] = [
            0x03, 0x00, 0x00, 0x03, 0x04, 0x06, 0x03, 0x04, 0x06, 0x0a, 0x02, 0x01,
        ];
        let err = convert_decl(&be).unwrap_err();
        assert!(
            err.contains("terrain stride-14 ground"),
            "expected pattern-named refusal, got: {err}"
        );
        assert!(err.contains("multi-stream"), "diagnostic must name the architectural cause: {err}");
    }

    #[test]
    fn ps3_compact_decl_terrain_stride26_detected() {
        // Terrain stride-26 (type_hash 0x7C569307, 294 descriptors): block[165]
        // desc[215] parent=STRM, bytes `03 00 00 03 03 06 08 02 04 0a 03 04 06
        // 0e 02 01 03 12 0e 04`. Xbox-DOH equiv: UV@8 F16x2 + COLOR@12 D3DCOLOR
        // + NORMAL@16 F16x4 + TANGENT@20 F16x4.
        let be: [u8; 20] = [
            0x03, 0x00, 0x00, 0x03, 0x03, 0x06, 0x08, 0x02, 0x04, 0x0a, 0x03, 0x04,
            0x06, 0x0e, 0x02, 0x01, 0x03, 0x12, 0x0e, 0x04,
        ];
        let err = convert_decl(&be).unwrap_err();
        assert!(err.contains("stride-26"), "{err}");
    }

    #[test]
    fn ps3_compact_decl_mesh_stride4_detected() {
        // Mesh/foliage compact stride-4 (type_hash 0x5B724250 / 0x600B904E,
        // 4,028 + 703 descriptors): block[29] desc[89] parent=STRM, bytes
        // `03 00 08 02`. 1-to-many against Xbox-DOH decls at the same block/
        // desc (10 distinct Xbox decls for 0x5B724250 alone) — the compact
        // body alone cannot select which Xbox decl to emit, so the refusal
        // must name the ambiguity.
        let be: [u8; 4] = [0x03, 0x00, 0x08, 0x02];
        let err = convert_decl(&be).unwrap_err();
        assert!(err.contains("stride-4"), "{err}");
        assert!(err.contains("multi-stream"), "{err}");
    }

    #[test]
    fn ps3_compact_decl_unknown_pattern_fails_loud() {
        // A pattern not in the 11-row DLC01 vocabulary must still error (never
        // fall through to a `_` match / silent skip, per project rules), but
        // with the generic "unrecognised" tag so callers can tell it apart
        // from the known catalogue.
        let be: [u8; 6] = [0x03, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
        let err = convert_decl(&be).unwrap_err();
        assert!(err.contains("unrecognised"), "unknown PS3 pattern must be labelled: {err}");
    }

    #[test]
    fn xbox_decl_still_dispatches_after_ps3_arm_added() {
        // The PS3-prefix gate must only fire on 0x03 0x00. A valid Xbox decl
        // (`a` of the header record is 00000000, so byte 0 == 0x00) must still
        // route to the Xbox path and produce the retail PC decl.
        let be: [u8; 48] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x1a, 0x23, 0x60, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x08, 0x00, 0x18, 0x28, 0x86, 0x00, 0x0a, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x0c, 0x00, 0x2a, 0x21, 0x90, 0x00, 0x03, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00,
        ];
        let out = convert_decl(&be).expect("Xbox decl must still convert");
        // Expect: PC header (8B) + 2 elems × 8B + END (8B) = 32B.
        assert_eq!(out.len(), 32);
        assert_eq!(&out[0..8], &[0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00]);
        // Elem 1: stream=0, offset=8, type=4 D3DCOLOR, method=0, usage=10 COLOR
        assert_eq!(&out[8..16], &[0x00, 0x00, 0x08, 0x00, 0x04, 0x00, 0x0a, 0x00]);
        // Elem 2: stream=0, offset=12, type=16 F16x4, method=0, usage=3 NORMAL
        assert_eq!(&out[16..24], &[0x00, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x03, 0x00]);
        // END
        assert_eq!(&out[24..32], &[0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00]);
    }

    /// The dispatch, not just the helper: an `EFCT` row in a BE container must reach the u16
    /// header swap. `EFCT` became a named `ChunkTag` (it used to arrive as `Unknown`); an arm that
    /// still matched `Unknown(b"EFCT")` would drop it into the generic u32 sweep.
    #[test]
    fn an_efct_row_is_converted_by_the_u16_header_swap() {
        let be_body: [u8; 18] = [
            0x00, 0x02, 0x02, 0x26, 0x00, 0x00, 0x00, 0x02, 0x00, 0x0c, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x04, 0x03, 0x20,
        ];
        let mut c = Vec::new();
        c.extend_from_slice(b"XFCU");
        for v in [40u32, 0, 0, 1] {
            c.extend_from_slice(&v.to_be_bytes());
        }
        c.extend_from_slice(b"TCFE"); // "EFCT", byte-reversed as on Xbox
        for v in [0u32, 18, 0, 0] {
            c.extend_from_slice(&v.to_be_bytes());
        }
        c.extend_from_slice(&be_body);
        let out = convert_container(&c, false, 0, crate::types::TYPE_HASH_EFFECT, None, None, None, None).expect("convert");
        assert_eq!(&out[20..24], b"EFCT");
        let mut want = be_body;
        convert_efct_header_inplace(&mut want);
        assert_eq!(&out[40..58], &want, "EFCT body must take the u16 header swap");
        assert_eq!(u16::from_le_bytes([out[42], out[43]]), 0x0226);
    }

    /// One row of `body` tagged `GEOM`, as Xbox stores it.
    fn effect_geom(body: &[u8]) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(b"XFCU");
        for v in [40u32, 0, 0, 1] {
            c.extend_from_slice(&v.to_be_bytes());
        }
        c.extend_from_slice(b"MOEG");
        for v in [0u32, body.len() as u32, 0, 0] {
            c.extend_from_slice(&v.to_be_bytes());
        }
        c.extend_from_slice(body);
        c
    }

    /// An emitter's `GEOM` keeps its two u16s in order, where the u32 sweep would trade the shape
    /// index for the count. A shape table's `GEOM` stays u32 and f32.
    #[test]
    fn an_emitter_geom_is_two_u16s() {
        let e = crate::types::TYPE_HASH_EFFECT;
        let out = convert_container(&effect_geom(&[0, 1, 0, 2]), false, 0, e, None, None, None, None).expect("convert");
        assert_eq!(&out[40..44], &[1, 0, 2, 0]);
        let mut shapes = 1u32.to_be_bytes().to_vec();
        shapes.extend_from_slice(&[0x3F, 0x80, 0, 0].repeat(13));
        let out = convert_container(&effect_geom(&shapes), false, 0, e, None, None, None, None).expect("convert");
        assert_eq!(&out[40..48], &[1, 0, 0, 0, 0, 0, 0x80, 0x3F]);
    }

    #[test]
    fn efct_header_u16_swap_preserves_count_gate() {
        // REAL Xbox 360 DLC particle EFCT (blocks\dlc01\effects_P000_Q3.block,
        // entry 0x5af5da9f), big-endian source as extracted from the retail DLC:
        let be: [u8; 18] = [
            0x00, 0x02, 0x02, 0x26, 0x00, 0x00, 0x00, 0x02, 0x00, 0x0c, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x04, 0x03, 0x20,
        ];
        // Correct LE (matches the retail PC pc-game-vz.wad EFCT layout): the
        // 0x0226 magic stays at byte +2 and the sub-component count 0x0004 stays
        // at byte +14 — a per-field u16 swap.
        let correct_le: [u8; 18] = [
            0x02, 0x00, 0x26, 0x02, 0x00, 0x00, 0x02, 0x00, 0x0c, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x04, 0x00, 0x20, 0x03,
        ];

        let mut fixed = be;
        convert_efct_header_inplace(&mut fixed);
        assert_eq!(fixed, correct_le, "u16 swap must reproduce retail layout");
        assert_eq!(u16::from_le_bytes([fixed[2], fixed[3]]), 0x0226); // magic @ +2
        assert_eq!(u16::from_le_bytes([fixed[14], fixed[15]]), 0x0004); // count @ +14

        // The regressed whole-u32-word swap moves the magic to +0 and zeroes
        // the +14 count gate → effect loader NULL-derefs on the first COLR append.
        let mut buggy = be;
        let n = buggy.len() / 4;
        for i in 0..n {
            let off = i * 4;
            buggy[off..off + 4].reverse();
        }
        if buggy.len() - n * 4 >= 2 {
            let off = n * 4;
            buggy[off..off + 2].reverse();
        }
        assert_ne!(buggy, correct_le);
        assert_eq!(u16::from_le_bytes([buggy[0], buggy[1]]), 0x0226); // magic moved to +0
        assert_eq!(u16::from_le_bytes([buggy[14], buggy[15]]), 0x0000); // count zeroed
    }

    #[test]
    fn chdr_header_per_u16_swap_matches_retail_oracle() {
        // BE source: u16@+0=0x0000, u16@+2=0x0038, u32@+4=0x00000002.
        // Retail layers_static block 29 CHDR (LE oracle) = 00 00 38 00 02 00 00 00.
        let mut data = vec![0x00, 0x00, 0x00, 0x38, 0x00, 0x00, 0x00, 0x02];
        convert_chdr_body_inplace(&mut data);
        assert_eq!(&data, &[0x00, 0x00, 0x38, 0x00, 0x02, 0x00, 0x00, 0x00]);
        // u16@+2 must read 56 (0x0038) so the engine strides 42 (>= 0x2A).
        assert_eq!(u16::from_le_bytes([data[2], data[3]]), 0x0038);
        assert_eq!(u16::from_le_bytes([data[0], data[1]]), 0x0000);
        // flags is a genuine u32 → whole-u32 swap is correct there.
        assert_eq!(
            u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            0x0000_0002
        );

        // Regression guard: the OLD whole-u32 swap of the first 8 bytes would
        // transpose the u16 fields → u16@+2 == 0 (< 42 → stride 40 → CRASH).
        let buggy = [0x38u8, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00];
        assert_eq!(u16::from_le_bytes([buggy[2], buggy[3]]), 0x0000);
        assert_ne!(&data[..], &buggy[..]);
    }

    #[test]
    fn mesh_chdr_full_u32_swap_preserves_property_hash() {
        // A CHDR inside a MESH (type 0x5B724250) is {u32 property_hash, u32 count}
        // — a CHDR+CEXE compiled-expression header, NOT the placement {u16,u16,u32}.
        // The engine parser 0x004CF340 matches the hash against fixed constants;
        // the placement swap half-swaps it → unrecognized → NULL write @0x004CF58B.
        // BE source: hash 0x9DA97065, count 7.
        let be = [0x9Du8, 0xA9, 0x70, 0x65, 0x00, 0x00, 0x00, 0x07];

        // The mesh path (type == TYPE_HASH_MODEL) does a full u32 swap → the hash
        // is preserved exactly, so the engine recognizes it.
        let mut good = be;
        super::swap_u32_array(&mut good);
        assert_eq!(
            u32::from_le_bytes([good[0], good[1], good[2], good[3]]),
            0x9DA9_7065,
            "mesh CHDR hash must survive intact (engine-recognized)"
        );
        assert_eq!(u32::from_le_bytes([good[4], good[5], good[6], good[7]]), 7);

        // The placement path WOULD half-swap the hash into the crashing value.
        let mut bad = be;
        convert_chdr_body_inplace(&mut bad);
        assert_eq!(
            u32::from_le_bytes([bad[0], bad[1], bad[2], bad[3]]),
            0x7065_9DA9,
            "placement {{u16,u16,u32}} swap half-swaps the hash (the bug)"
        );
        assert_ne!(&good[..], &bad[..]);
    }

    #[test]
    fn chdr_large_guidmap_only_swaps_header() {
        // Large/guidmap CHDR: only the 8-byte header is swapped; the trailing
        // region (reached via sibling descriptors) is left untouched.
        let mut data = vec![0x00u8, 0x00, 0x00, 0x38, 0x00, 0x00, 0x00, 0x02];
        let tail: Vec<u8> = (0u8..40).collect();
        data.extend_from_slice(&tail);
        convert_chdr_body_inplace(&mut data);
        assert_eq!(&data[..8], &[0x00, 0x00, 0x38, 0x00, 0x02, 0x00, 0x00, 0x00]);
        assert_eq!(&data[8..], &tail[..], "trailing guidmap region untouched");
    }

    #[test]
    fn layoutrules_embedded_repair() {
        // Embedded packfile header at offset 16; BE layoutRules = 04 00 00 01.
        let m = 16usize;
        let mut be = vec![0xAAu8; 64];
        be[m..m + 8].copy_from_slice(&HAVOK_PACKFILE_MAGIC);
        be[m + 16..m + 20].copy_from_slice(&[0x04, 0x00, 0x00, 0x01]);

        // Simulate the bug: blanket u32-swap reverses layoutRules → 01 00 00 04.
        let mut out = be.clone();
        out[m + 16..m + 20].copy_from_slice(&[0x01, 0x00, 0x00, 0x04]);
        assert_eq!(out[m + 17], 0, "precondition: littleEndian scrambled to 0");

        fix_embedded_havok_layoutrules(&be, &mut out);

        assert_eq!(&out[m + 16..m + 20], &[0x04, 0x01, 0x00, 0x01]);
        assert_eq!(out[m + 17], 1, "littleEndian restored to 1");
    }

    // -- Terrainmesh re-encode primitives ---------------------------------

    #[test]
    fn f16_encode_roundtrip_matches_std() {
        // Compare against the std `f32 as f16`-equivalent via `<e>` bit pattern
        // for a spread of values (the oracle compares these exact bytes).
        for &v in &[
            0.0f32, 1.0, -1.0, 0.5, 0.999, 0.04211, 0.00768, -0.4131, 0.88330,
            0.22156, 6.1e-5, 1e-7, 65504.0,
        ] {
            let got = super::f32_to_f16_bits(v);
            // reference: round-trip through f32->f16 via half-aware bit twiddling
            // using the well-known reference (the `half` crate algorithm) is not
            // available here; instead assert the decoded half is within 1 ULP of v.
            let decoded = half_to_f32(got);
            let err = (decoded - v).abs();
            let tol = (v.abs() * 2.0_f32.powi(-10)).max(2.0_f32.powi(-24));
            assert!(err <= tol + 1e-6, "f16({v}) -> {decoded} err {err} tol {tol}");
        }
    }

    fn half_to_f32(h: u16) -> f32 {
        let s = (h >> 15) & 1;
        let e = (h >> 10) & 0x1f;
        let m = h & 0x3ff;
        let val = if e == 0 {
            (m as f32) * 2.0f32.powi(-24)
        } else if e == 0x1f {
            f32::INFINITY
        } else {
            (1.0 + m as f32 / 1024.0) * 2.0f32.powi(e as i32 - 15)
        };
        if s == 1 { -val } else { val }
    }

    #[test]
    fn dec3n_decodes_known_normals() {
        // Pinned 11-11-10 layout: verified pairs from the worked terrainmesh
        // (raw Xbox BE u32 -> PC unit normal). Direction must match to ~0.1°.
        let cases: &[(u32, [f32; 3])] = &[
            (0x001ff007, [0.0076, 1.0, 0.0004]),
            (0x055ff007, [0.0077, 0.999, 0.0421]),
            (0x1c5c4659, [-0.4131, 0.8833, 0.2216]),
        ];
        for &(u, exp) in cases {
            let h = super::dec3n_to_half4_le(u, false); // NORMAL = 11-11-10
            let nx = half_to_f32(u16::from_le_bytes([h[0], h[1]]));
            let ny = half_to_f32(u16::from_le_bytes([h[2], h[3]]));
            let nz = half_to_f32(u16::from_le_bytes([h[4], h[5]]));
            let w = half_to_f32(u16::from_le_bytes([h[6], h[7]]));
            assert!((w - 1.0).abs() < 1e-3, "W must be 1.0, got {w}");
            // normalize expected
            let em = (exp[0] * exp[0] + exp[1] * exp[1] + exp[2] * exp[2]).sqrt();
            let dot = (nx * exp[0] + ny * exp[1] + nz * exp[2]) / em;
            let ang = dot.clamp(-1.0, 1.0).acos().to_degrees();
            // Tolerance is dominated by the coarse 4-decimal expected literals here;
            // the measured error vs the actual PC half-floats across 24k samples is
            // mean 0.05° / max 0.12° (see the implementation doc).
            assert!(ang < 1.5, "DEC3N {u:08x} angular error {ang}° > 1.5");
        }
    }

    #[test]
    fn dec3n_tangent_uses_10_10_10() {
        // Retail base_v2 oracle, main-STRM vertex 0 TANGENT: raw Xbox BE u32
        // 0x7fe30a25 -> retail PC FLOAT16_4 tangent (-0.929, 0.380, -0.004).
        // The 10-10-10-2 split reproduces it to 0°; the 11-11-10 normal split
        // gives ~126° (the bug that garbled every tangent).
        let u = 0x7fe30a25u32;
        let exp = [-0.929f32, 0.380, -0.004];
        let h = super::dec3n_to_half4_le(u, true); // TANGENT = 10-10-10
        let tx = half_to_f32(u16::from_le_bytes([h[0], h[1]]));
        let ty = half_to_f32(u16::from_le_bytes([h[2], h[3]]));
        let tz = half_to_f32(u16::from_le_bytes([h[4], h[5]]));
        let em = (exp[0] * exp[0] + exp[1] * exp[1] + exp[2] * exp[2]).sqrt();
        let dot = (tx * exp[0] + ty * exp[1] + tz * exp[2]) / em;
        let ang = dot.clamp(-1.0, 1.0).acos().to_degrees();
        assert!(ang < 2.0, "tangent 10-10-10 angular error {ang}° > 2 (got {tx},{ty},{tz})");
        // And the wrong (normal) layout must NOT match — guards against regressing.
        let hw = super::dec3n_to_half4_le(u, false);
        let wx = half_to_f32(u16::from_le_bytes([hw[0], hw[1]]));
        let wy = half_to_f32(u16::from_le_bytes([hw[2], hw[3]]));
        let wz = half_to_f32(u16::from_le_bytes([hw[4], hw[5]]));
        let wang = ((wx * exp[0] + wy * exp[1] + wz * exp[2]) / em)
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees();
        assert!(wang > 30.0, "11-11-10 should garble this tangent, got {wang}°");
    }

    /// A tangent's top two bits, as a signed 2-bit value, give its w: negative is −1, 0 and 1 are +1. A normal's w
    /// is 1.0 whatever its top bits hold.
    #[test]
    fn a_tangents_top_bits_are_its_handedness() {
        let w = |u: u32, tangent: bool| {
            let h = super::dec3n_to_half4_le(u, tangent);
            half_to_f32(u16::from_le_bytes([h[6], h[7]]))
        };
        let xyz = 0x0030_0a25u32 & 0x3fff_ffff;
        assert_eq!(w(xyz | 0xc000_0000, true), -1.0, "top bits 11");
        assert_eq!(w(xyz | 0x8000_0000, true), -1.0, "top bits 10");
        assert_eq!(w(xyz | 0x4000_0000, true), 1.0, "top bits 01");
        assert_eq!(w(xyz, true), 1.0, "top bits 00");
        assert_eq!(w(xyz | 0xc000_0000, false), 1.0, "a normal");
    }

    #[test]
    fn prmt_walker_swaps_u32_then_u16_per_record() {
        // One 16-byte BE PRMT record: material_index=0x00000007, start_index=0x10,
        // index_count=0x0203, base=0x0405, max=0x0607, span=0x0809.
        let mut be = Vec::new();
        be.extend_from_slice(&0x00000007u32.to_be_bytes());
        be.extend_from_slice(&0x00000010u32.to_be_bytes());
        be.extend_from_slice(&0x0203u16.to_be_bytes());
        be.extend_from_slice(&0x0405u16.to_be_bytes());
        be.extend_from_slice(&0x0607u16.to_be_bytes());
        be.extend_from_slice(&0x0809u16.to_be_bytes());
        let mut out = be.clone();
        assert!(super::walk_records(&mut out, &super::PRMT_WALKER));
        // u32s reversed as u32 (NOT u16-transposed: old bug gave 00 00 07 00).
        assert_eq!(&out[0..4], &[0x07, 0x00, 0x00, 0x00]);
        assert_eq!(&out[4..8], &[0x10, 0x00, 0x00, 0x00]);
        // u16s reversed per-u16.
        assert_eq!(u16::from_le_bytes([out[8], out[9]]), 0x0203);
        assert_eq!(u16::from_le_bytes([out[10], out[11]]), 0x0405);
        assert_eq!(u16::from_le_bytes([out[14], out[15]]), 0x0809);
    }

    /// A little-endian 20-byte terrain draw.
    fn terrain_draw(start: u32, prims: u16) -> Vec<u8> {
        let mut r = vec![0u8; 20];
        r[8..12].copy_from_slice(&start.to_le_bytes());
        r[12..14].copy_from_slice(&prims.to_le_bytes());
        r
    }

    fn strip_bytes(idx: &[u16]) -> Vec<u8> {
        idx.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// The triangles of a strip range with their winding, degenerates dropped, each rotated so its
    /// lowest index leads, the way a strip's alternating order is read.
    fn wound(strip: &[u16]) -> Vec<[u16; 3]> {
        strip
            .windows(3)
            .enumerate()
            .filter(|(_, t)| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .map(|(i, t)| {
                let t = if i % 2 == 0 { [t[0], t[1], t[2]] } else { [t[1], t[0], t[2]] };
                let at = (0..3).min_by_key(|&k| t[k]).expect("three");
                [t[at], t[(at + 1) % 3], t[(at + 2) % 3]]
            })
            .collect()
    }

    #[test]
    fn a_terrain_draw_swaps_as_twenty_bytes_whole_words() {
        let mut be = vec![0u8; 80];
        be[8..12].copy_from_slice(&10u32.to_be_bytes());
        be[28..32].copy_from_slice(&0x025Eu32.to_be_bytes());
        // Four records are 80 bytes, a multiple of the model's 16.
        assert!(super::walk_records(&mut be, super::prmt_walker(crate::types::TYPE_HASH_TERRAIN_MESH)));
        assert_eq!(u32::from_le_bytes([be[8], be[9], be[10], be[11]]), 10);
        assert_eq!(u32::from_le_bytes([be[28], be[29], be[30], be[31]]), 0x025E);
    }

    #[test]
    fn a_terrain_draw_keeps_its_triangles_and_winding_across_a_restart() {
        // Two segments of a draw, the first of an odd length, then a second draw of its own.
        let source = [0u16, 1, 2, 3, 4, 0xFFFF, 5, 6, 7, 8, 9, 10, 11];
        let draws = [terrain_draw(0, 8), terrain_draw(10, 1)].concat();
        let (strip, prmt) = super::terrain_strips(&strip_bytes(&source), &draws).unwrap();
        let idx: Vec<u16> = strip.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        assert!(!idx.contains(&0xFFFF), "PC has no restart");
        let read = |r: usize| {
            let at = r * 20;
            let start = u32::from_le_bytes([prmt[at + 8], prmt[at + 9], prmt[at + 10], prmt[at + 11]]) as usize;
            let prims = u16::from_le_bytes([prmt[at + 12], prmt[at + 13]]) as usize;
            &idx[start..start + prims + 2]
        };
        let want = [wound(&[0, 1, 2, 3, 4]), wound(&[5, 6, 7, 8])].concat();
        assert_eq!(wound(read(0)), want);
        assert_eq!(wound(read(1)), wound(&[9, 10, 11]));
    }

    #[test]
    fn terrain_draws_of_one_range_share_it() {
        let draws = [terrain_draw(0, 1), terrain_draw(0, 1)].concat();
        let (strip, prmt) = super::terrain_strips(&strip_bytes(&[0, 1, 2]), &draws).unwrap();
        assert_eq!(strip.len(), 6);
        assert_eq!(prmt[..20], prmt[20..]);
    }

    #[test]
    fn a_terrain_draw_past_its_strip_is_an_error() {
        assert!(super::terrain_strips(&strip_bytes(&[0, 1, 2]), &terrain_draw(0, 4)).is_err());
    }

    /// A PS3 draw record, big-endian.
    fn ps3_draw(vs: u32, start: u32, count: u32) -> Vec<u8> {
        [vs.to_be_bytes(), 7u32.to_be_bytes(), start.to_be_bytes(), count.to_be_bytes(), [0, 0, 0, 0]].concat()
    }

    fn be_indices(i: &[u16]) -> Vec<u8> {
        i.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    /// The triangles a PC strip draws, each rotated to its lowest index, winding kept, degenerates gone.
    fn strip_triangles(s: &[u16]) -> Vec<[u16; 3]> {
        let mut out: Vec<[u16; 3]> = wound(s);
        out.retain(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
        out
    }

    #[test]
    fn a_ps3_draw_keeps_its_triangles_and_winding_as_a_strip() {
        // A run that continues as a strip, then a triangle that shares no edge, at both parities.
        let list = [0u16, 1, 2, 2, 1, 3, 2, 3, 4, 8, 9, 10, 9, 11, 10, 20, 21, 22];
        let (strip, prmt) = super::terrain_strips_of_lists(&be_indices(&list), &ps3_draw(0xAA, 0, 18), |k| k).unwrap();
        let idx: Vec<u16> = strip.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        let prims = u16::from_le_bytes([prmt[12], prmt[13]]) as usize;
        assert_eq!(idx.len(), prims + 2);
        let mut want: Vec<[u16; 3]> = list
            .chunks_exact(3)
            .map(|t| {
                let at = (0..3).min_by_key(|&i| t[i]).unwrap();
                [t[at], t[(at + 1) % 3], t[(at + 2) % 3]]
            })
            .collect();
        want.sort_unstable();
        let mut got = strip_triangles(&idx);
        got.sort_unstable();
        assert_eq!(got, want);
    }

    #[test]
    fn a_ps3_record_becomes_a_pc_record() {
        let (strip, prmt) = super::terrain_strips_of_lists(&be_indices(&[5, 6, 7, 6, 8, 7]), &ps3_draw(0xAA, 0, 6), |k| k + 1).unwrap();
        assert_eq!(strip.len(), 8, "one strip of 4");
        assert_eq!(u32::from_le_bytes([prmt[0], prmt[1], prmt[2], prmt[3]]), 0xAB, "the PC key");
        assert_eq!(u32::from_le_bytes([prmt[4], prmt[5], prmt[6], prmt[7]]), 7, "the material");
        let tail: Vec<u16> = prmt[12..20].chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(tail, [2, 5, 8, 4], "triangles, min, max, distinct indices");
    }

    #[test]
    fn ps3_draws_of_one_range_share_it() {
        let draws = [ps3_draw(1, 0, 3), ps3_draw(2, 0, 3)].concat();
        let (strip, prmt) = super::terrain_strips_of_lists(&be_indices(&[0, 1, 2]), &draws, |k| k).unwrap();
        assert_eq!(strip.len(), 6);
        assert_eq!(prmt[8..20], prmt[28..40]);
    }

    #[test]
    fn a_ps3_draw_past_its_list_or_not_of_triangles_is_an_error() {
        assert!(super::terrain_strips_of_lists(&be_indices(&[0, 1, 2]), &ps3_draw(1, 0, 6), |k| k).is_err());
        assert!(super::terrain_strips_of_lists(&be_indices(&[0, 1, 2, 3]), &ps3_draw(1, 0, 4), |k| k).is_err());
    }

    #[test]
    fn walk_records_bails_on_bad_length() {
        // 15 bytes is not a clean 16-byte multiple -> false, no mutation.
        let mut body = vec![0xAAu8; 15];
        let snapshot = body.clone();
        assert!(!super::walk_records(&mut body, &super::PRMT_WALKER));
        assert_eq!(body, snapshot);
    }

    #[test]
    fn convert_mtrl_walks_three_material_array() {
        // Reproduces model 0x849972EE (global_weapon_c4land_projectile): a 372-byte
        // MTRL with 3 materials (counts 1/3/2, strides 120/128) — the layout that
        // crashed when only material[0] was repaired. Build the BE source, convert,
        // assert every material's count + flags survive AND hashes/preamble u32-swap.
        const PB: usize = 104;
        let mut be = vec![0u8; 372];
        // preamble dword 0 (BE) -> must u32-swap
        be[0..4].copy_from_slice(&0x11223344u32.to_be_bytes());
        // material[0] @104: flags=0x0080 count=1, one hash, float props
        be[PB..PB + 2].copy_from_slice(&0x0080u16.to_be_bytes());
        be[PB + 2..PB + 4].copy_from_slice(&1u16.to_be_bytes());
        be[PB + 4..PB + 8].copy_from_slice(&0xAABBCCDDu32.to_be_bytes()); // hash
        be[PB + 8..PB + 12].copy_from_slice(&1.0f32.to_be_bytes()); // a prop (must not match a flag word)
        // material[1] @224: flags=0x0080 count=3, three hashes
        let m1 = 224;
        be[m1..m1 + 2].copy_from_slice(&0x0080u16.to_be_bytes());
        be[m1 + 2..m1 + 4].copy_from_slice(&3u16.to_be_bytes());
        be[m1 + 4..m1 + 8].copy_from_slice(&0x0D4FA498u32.to_be_bytes());
        // material[2] @352: flags=0 count=2, two hashes
        let m2 = 352;
        be[m2..m2 + 2].copy_from_slice(&0x0000u16.to_be_bytes());
        be[m2 + 2..m2 + 4].copy_from_slice(&2u16.to_be_bytes());

        convert_mtrl(&mut be, None).unwrap();

        // preamble dword 0 u32-swapped
        assert_eq!(&be[0..4], &[0x44, 0x33, 0x22, 0x11]);
        // material[0]: flags@104=0x0080, count@106=1, hash u32-swapped
        assert_eq!(u16::from_le_bytes([be[PB], be[PB + 1]]), 0x0080);
        assert_eq!(u16::from_le_bytes([be[PB + 2], be[PB + 3]]), 1);
        assert_eq!(&be[PB + 4..PB + 8], &[0xDD, 0xCC, 0xBB, 0xAA]);
        // material[1]: flags=0x0080, count=3 (NOT 0x80=128) — the bug this fixes
        assert_eq!(u16::from_le_bytes([be[m1], be[m1 + 1]]), 0x0080);
        assert_eq!(u16::from_le_bytes([be[m1 + 2], be[m1 + 3]]), 3);
        assert_eq!(&be[m1 + 4..m1 + 8], &[0x98, 0xA4, 0x4F, 0x0D]);
        // material[2]: flags=0, count=2
        assert_eq!(u16::from_le_bytes([be[m2], be[m2 + 1]]), 0x0000);
        assert_eq!(u16::from_le_bytes([be[m2 + 2], be[m2 + 3]]), 2);
    }

    #[test]
    fn convert_mtrl_single_material_no_regression() {
        // One material (count=1): flags/count in place, preamble + hash + tail u32-swapped.
        const PB: usize = 104;
        let mut be = vec![0u8; 120];
        be[PB..PB + 2].copy_from_slice(&0x0001u16.to_be_bytes()); // flags
        be[PB + 2..PB + 4].copy_from_slice(&1u16.to_be_bytes()); // count
        be[PB + 4..PB + 8].copy_from_slice(&0x01020304u32.to_be_bytes()); // hash
        convert_mtrl(&mut be, None).unwrap();
        assert_eq!(u16::from_le_bytes([be[PB], be[PB + 1]]), 0x0001);
        assert_eq!(u16::from_le_bytes([be[PB + 2], be[PB + 3]]), 1);
        assert_eq!(&be[PB + 4..PB + 8], &[0x04, 0x03, 0x02, 0x01]);
    }
}


// -------------------------------------------------------------------------
//  PS3 native path -- helper surface
//
//  These `pub(super)` wrappers let `ps3_native` reuse the Xbox-fetch-decl
//  translation, the DEC3N -> F16x4 decoder, and the per-tag body swap arms,
//  so the native path stays byte-identical to the Xbox-DOH -> PC output.
// -------------------------------------------------------------------------

/// Run the Xbox-fetch-decl -> PC D3DVERTEXELEMENT9 translation on `xbox_bytes`.
/// Infallible for the hard-coded Xbox decl bytes carried by `ps3_native::PATTERNS`
/// (every entry is a known-good Xbox fetch-decl); panics only on a programming
/// error (a catalogue entry with malformed Xbox bytes).
pub(super) fn ps3_native_translate_xbox_decl(xbox_bytes: &[u8]) -> Vec<u8> {
    convert_decl(xbox_bytes).expect("ps3_native: catalogue xbox_decl must translate")
}

/// Public wrapper over the Xbox DEC3N -> PC FLOAT16_4 decoder so ps3_native can
/// use the same bit-split rules (HEND3N 11-11-10 for NORMAL, DEC3N 10-10-10
/// for TANGENT/BINORMAL) as the Xbox path.
pub(super) fn ps3_native_dec3n_to_half4_le(u: u32, ten_ten_ten: bool) -> [u8; 8] {
    dec3n_to_half4_le(u, ten_ten_ten)
}

/// Per-tag body translator for the PS3 native path.
///
/// Called for every non-STRM-handled descriptor body in a PS3 terrain / lowres
/// container. Produces PC-ready LE bytes from the raw BE body, dispatching on
/// (`tag`, `group_tag`, `type_hash`) with the same per-tag arms as
/// `convert_generic_bodies` -- so for the shared bodies (IBUF info/data,
/// PRMG/PRMT, MTRL, CHDR, PRMG, PHY2, HIER, etc.) PS3 and Xbox paths produce
/// the same PC bytes.
pub(super) fn ps3_native_translate_body(
    be_body: &[u8],
    tag: ChunkTag,
    tag_le: [u8; 4],
    group_tag: ChunkTag,
    type_hash: u32,
    terrain_materials: Option<usize>,
) -> Result<Vec<u8>, String> {
    let _ = tag_le; // reserved for future tag-byte-sensitive arms
    let mut body = be_body.to_vec();
    // Mirror the per-tag arms from `convert_generic_bodies`. We only reach this
    // function for non-STRM descriptors in a PS3 terrain / lowres container,
    // so the ECS-only arms (`convert_ecs_bodies`) are not needed.
    match tag {
        ChunkTag::Syek | ChunkTag::Srts => { /* native BE on all platforms */ }
        ChunkTag::Strs => convert_strs_inplace(&mut body, type_hash)?,
        _ if is_string_tag(tag) => { /* string data */ }
        ChunkTag::Prmt => {
            if !walk_records(&mut body, prmt_walker(type_hash)) {
                swap_u16_array(&mut body);
            }
        }
        ChunkTag::Trns => { /* keep raw */ }
        ChunkTag::Unknown(b) if b == *b"SEGM" => {
            let mut o = 0;
            while o + 4 <= body.len() {
                body.swap(o, o + 1);
                o += 4;
            }
        }
        ChunkTag::Inst | ChunkTag::Bshi => swap_u16_array(&mut body),
        ChunkTag::Hier => {
            if !convert_hier_inplace(&mut body) {
                swap_u32_array(&mut body);
            }
        }
        ChunkTag::Trck => convert_trck_inplace(&mut body),
        ChunkTag::Ptms => {
            if !walk_records(&mut body, &PTMS_WALKER) {
                swap_u32_array(&mut body);
            }
        }
        ChunkTag::Ptch => {
            if !convert_ptch_inplace(&mut body) {
                swap_u32_array(&mut body);
            }
        }
        ChunkTag::Mtrl => convert_mtrl(&mut body, terrain_materials)?,
        ChunkTag::Enum => convert_enum_body_inplace(&mut body),
        ChunkTag::Ibuf => swap_u16_array(&mut body),
        ChunkTag::Indx => swap_u16_array(&mut body),
        ChunkTag::Info if type_hash == types::TYPE_HASH_ANIMATION => swap_u16_array(&mut body),
        ChunkTag::Info => convert_info_body_inplace(&mut body),
        ChunkTag::InfoUpper if type_hash == types::TYPE_HASH_STANCE => swap_u16_array(&mut body),
        ChunkTag::InfoUpper if type_hash == types::TYPE_HASH_TERRAIN_MESH && group_tag == ChunkTag::Prmg => {
            convert_terrain_draw_group_info(&mut body)?
        }
        ChunkTag::Type if type_hash == types::TYPE_HASH_STANCE => {
            convert_stance_type_names_inplace(&mut body)
        }
        ChunkTag::Schm | ChunkTag::Flgs => swap_u32_array(&mut body),
        ChunkTag::Decl => {
            // STRM decl bodies are already handled by the STRM pass before this
            // function is called. A bare `decl` outside a STRM group is unexpected
            // for terrain / lowres -- leave the body raw so a reader can see it.
        }
        ChunkTag::Chdr => {
            if type_hash == types::TYPE_HASH_MODEL {
                swap_u32_array(&mut body);
            } else {
                convert_chdr_body_inplace(&mut body);
            }
        }
        ChunkTag::Deps => {
            if body.len() > 1 {
                swap_u32_array(&mut body[1..]);
            }
        }
        ChunkTag::Efct => convert_efct_header_inplace(&mut body),
        ChunkTag::Emtr => swap_u16_array(&mut body),
        ChunkTag::Binn => { /* leave raw -- apply_binn_transcode handles elsewhere */ }
        ChunkTag::Minf => {
            if body.len() >= 6 {
                swap_u32(&mut body, 0);
                swap_u16_array(&mut body[4..]);
            } else {
                swap_u16_array(&mut body);
            }
        }
        ChunkTag::Unknown(b) if b == *b"evnt" => convert_evnt_inplace(&mut body),
        ChunkTag::Unknown(b) if b == *b"trnm" => convert_trnm_inplace(&mut body),
        ChunkTag::Phy2 => {
            let conv = havok::convert_phy2_be_to_le(&body)
                .map_err(|e| format!("ps3_native PHY2 Havok convert: {e}"))?;
            if conv.len() != body.len() {
                return Err(format!(
                    "ps3_native PHY2 size change {} -> {}",
                    body.len(),
                    conv.len()
                ));
            }
            body = conv;
        }
        ChunkTag::Data if group_tag == ChunkTag::Ibuf => swap_u16_array(&mut body),
        _ => swap_u32_array(&mut body),
    }
    Ok(body)
}
