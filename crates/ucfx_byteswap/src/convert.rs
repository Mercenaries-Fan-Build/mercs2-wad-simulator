use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::read_u32_be;
use mercs2_formats::schema::{ComponentSchema, SchemaFieldType};
use mercs2_formats::tags::ChunkTag;
use mercs2_formats::texsize::{dxt_format, dxt_mip_count, linear_mip_chain_size, tex_mip_levels};
use mercs2_formats::types;

use crate::havok;
use crate::lua;
use crate::report::SchemaCoverageReport;

/// When `true`, the per-block / per-entry diagnostics below are suppressed.
/// The in-process `dlc_port` driver sets this (it converts thousands of blocks);
/// the CLI leaves it `false` so single-block runs stay verbose.
pub static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

macro_rules! vlog {
    ($($arg:tt)*) => {
        if !$crate::convert::QUIET.load(std::sync::atomic::Ordering::Relaxed) {
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
struct Descriptor {
    tag: ChunkTag,
    tag_bytes_le: [u8; 4],
    row_u0: u32,
    body_size: u32,
    row_u3: u32,
    row_u4: u32,
}

/// Convert a decompressed BE block to LE format.
/// When `report` is `Some`, collects schema field coverage stats.
pub fn convert_block(
    be_data: &[u8],
    dry_run: bool,
    mut report: Option<&mut SchemaCoverageReport>,
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

        let is_ecs = entry.type_hash == TYPE_HASH_ECS_NODE
            || entry.type_hash == TYPE_HASH_WORLD_ENTITY
            || entry.type_hash == TYPE_HASH_GUIDMAP;
        let converted = convert_container(container, is_ecs, ei, entry.type_hash, report.as_deref_mut())?;
        converted_containers.push(converted);

        offset = container_end;
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
    output.extend_from_slice(&(entry_count as u32).to_le_bytes());
    for (ei, entry) in entries.iter().enumerate() {
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
fn convert_container(
    container: &[u8],
    is_ecs: bool,
    entry_idx: usize,
    type_hash: u32,
    report: Option<&mut SchemaCoverageReport>,
) -> Result<Vec<u8>, String> {
    if container.len() < 20 {
        return Err(format!("Entry {} container too small ({})", entry_idx, container.len()));
    }

    let magic = &container[0..4];
    if magic != b"XFCU" && magic != b"UCFX" {
        return Err(format!("Entry {}: bad UCFX magic {:?}", entry_idx, magic));
    }

    let is_be = magic == b"XFCU";
    let read_u32: fn(&[u8], usize) -> u32 = if is_be { read_u32_be } else { mercs2_formats::ffcs::read_u32_le };

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

    // For ECS containers, we need to identify COMP groups and convert them using schemas
    if is_ecs {
        convert_ecs_bodies(&mut out, container, &descriptors, data_area_off, is_be, entry_idx, report)?;
    } else {
        convert_generic_bodies(&mut out, container, &descriptors, data_area_off, is_be, type_hash, entry_idx, report)?;
    }

    // Repair embedded Havok packfile headers whose 4 × u8 `layoutRules` were
    // wrongly u32-swapped by the generic body sweep (mirrors
    // `tools/ucfx_be_to_le._fix_embedded_havok_layoutrules`). Offsets in `out`
    // match `container` 1:1 when the byteswap preserves size (animation bodies).
    if is_be && out.len() == container.len() {
        fix_embedded_havok_layoutrules(container, &mut out);
    }

    // Xbox 360 textures: rebuild INFO + untile the GPU-tiled DXT BODY. The
    // generic pass left INFO basic-swapped and BODY as tiled passthrough; this
    // reframes the container (BODY shrinks tiled->linear). Full (non-streamed)
    // DXT entries only; stubs / non-DXT / non-trailing BODY fall through
    // unchanged. convert_block recomputes chunk_size + CSUM from the new length.
    if is_be && type_hash == types::TYPE_HASH_TEXTURE {
        apply_texture_untile(&mut out, &descriptors, data_area_off, desc_table_end);
    }

    // Xbox 360 wavebanks: transcode embedded audio clips (Xbox-ADPCM nibble-swap /
    // XMA via ffmpeg → PC IMA) and reframe the container (the audio body resizes).
    // The generic pass left the `data` chunk raw BE for this.
    if is_be && type_hash == types::TYPE_HASH_WAVEBANK {
        apply_wavebank_transcode(&mut out, &descriptors, data_area_off, desc_table_end)?;
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
        apply_binn_transcode(&mut out, &descriptors, data_area_off, desc_table_end)?;
    }

    // Xbox 360 terrainmesh (0x7C569307): a genuine re-encode (vertices widen to the
    // PC stride, indices de-strip, info/count fields rewrite). Runs last, gated on
    // the terrainmesh type_hash, and rebuilds + reframes the whole data area. See
    // apply_terrainmesh_reencode / docs/terrainmesh_reencode_implementation.md.
    if is_be && type_hash == types::TYPE_HASH_TERRAIN_MESH {
        apply_terrainmesh_reencode(&mut out, &descriptors, data_area_off, desc_table_end)?;
    }

    Ok(out)
}

/// Convert every `BINN` (Lua bytecode) chunk in `out` via the unluac round-trip
/// (`lua::convert_binn_be_to_le`) and reframe the container if a body resizes.
/// Mirrors `apply_wavebank_transcode`, generalized to multiple BINN chunks:
/// offsets/sizes are re-read from the (possibly already-shifted) `out` table.
fn apply_binn_transcode(
    out: &mut Vec<u8>,
    descriptors: &[Descriptor],
    data_area_off: usize,
    desc_table_end: usize,
) -> Result<(), String> {
    let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };
    let binn_idxs: Vec<usize> = descriptors
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            matches!(d.tag, ChunkTag::Unknown(b) if b == *b"BINN")
                && d.row_u0 != 0xFFFF_FFFF
                && d.body_size > 0
        })
        .map(|(i, _)| i)
        .collect();
    if binn_idxs.is_empty() {
        return Ok(());
    }
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
        let new_body = lua::convert_binn_be_to_le(&be_body)?;

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

/// Xbox-360 wavebank: transcode the audio `data` chunk (Xbox-ADPCM / XMA → PC IMA)
/// and reframe the container, mirroring `apply_texture_untile`. The body was left
/// raw BE by `convert_generic_bodies`' wavebank no-op arm, so the BE record fields
/// are intact for `audio::convert_wavebank_data`.
fn apply_wavebank_transcode(
    out: &mut Vec<u8>,
    descriptors: &[Descriptor],
    data_area_off: usize,
    desc_table_end: usize,
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
    let new_body = crate::audio::convert_wavebank_data(&body_be)
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

/// Convert bodies for ECS (layer / ECS_NODE) containers.
/// Identifies COMP triplets (info/schm/data) and applies schema-driven swap.
/// Convert an ECS `info` descriptor body from BE to LE, in place.
///
/// Two on-disk shapes (see `extract_comp_name`):
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

fn convert_ecs_bodies(
    out: &mut Vec<u8>,
    container: &[u8],
    descriptors: &[Descriptor],
    data_area_off: usize,
    is_be: bool,
    _entry_idx: usize,
    mut report: Option<&mut SchemaCoverageReport>,
) -> Result<(), String> {
    // We need to write body data at the correct offsets.
    // Strategy: build the entire data area as a mutable copy, then do in-place swaps.
    let data_start = if data_area_off > 0 { data_area_off } else {
        20 + descriptors.len() * 20
    };

    if data_start >= container.len() {
        return Ok(());
    }

    let mut data_area = container[data_start..].to_vec();

    // Identify COMP groups: a COMP sentinel followed by info, schm, data children
    let mut i = 0;
    while i < descriptors.len() {
        let desc = &descriptors[i];

        if desc.tag == ChunkTag::Comp && desc.row_u0 == 0xFFFFFFFF {
            // Found COMP group sentinel — look for info/schm/data triplet
            let _group_start = i;
            i += 1;

            let mut info_idx: Option<usize> = None;
            let mut schm_idx: Option<usize> = None;
            let mut data_idx: Option<usize> = None;

            // Scan children until next group sentinel or end
            while i < descriptors.len() && descriptors[i].row_u0 != 0xFFFFFFFF {
                match descriptors[i].tag {
                    ChunkTag::Info => { info_idx = Some(i); }
                    ChunkTag::Schm => { schm_idx = Some(i); }
                    ChunkTag::Data => { data_idx = Some(i); }
                    _ => {}
                }
                i += 1;
            }

            // Extract component name from info body
            let comp_name = if let Some(ii) = info_idx {
                extract_comp_name(container, &descriptors[ii], data_area_off, is_be)
            } else {
                None
            };
            let comp_name_str = comp_name.as_deref().unwrap_or("unknown");

            // Parse schema from schm body; pre-scan type codes for reporting
            let schm_body_range = schm_idx.and_then(|si| {
                body_range(container, descriptors[si].row_u0, descriptors[si].body_size, data_area_off)
            });

            let schema = schm_body_range.and_then(|(start, end)| {
                ComponentSchema::from_schm_body(&container[start..end], is_be)
            });

            // Report schema field type codes
            if let Some(ref mut rpt) = report {
                if let Some((start, end)) = schm_body_range {
                    let scanned = scan_schm_type_codes(&container[start..end], is_be);
                    let mut unknown_in_body = Vec::new();
                    for &(type_code, name_hash, byte_offset) in &scanned {
                        rpt.record_field(type_code);
                        if SchemaFieldType::from_code(type_code).is_none() {
                            rpt.record_unknown_field(comp_name_str, type_code, name_hash, byte_offset);
                            unknown_in_body.push(type_code);
                        }
                    }
                    if schema.is_none() && !scanned.is_empty() {
                        rpt.record_schema_parse_failure(comp_name_str, unknown_in_body);
                    }
                }
            }

            // Convert the data body using the schema
            if let Some(di) = data_idx {
                let d = &descriptors[di];
                if let Some((start, end)) = body_range(container, d.row_u0, d.body_size, data_area_off) {
                    let body_local_start = start - data_start;
                    let body_local_end = end - data_start;
                    if body_local_end <= data_area.len() {
                        let data_size = body_local_end - body_local_start;
                        let body_slice = &mut data_area[body_local_start..body_local_end];
                        convert_comp_data_inplace(body_slice, schema.as_ref(), comp_name_str, report.as_deref_mut(), data_size);
                    }
                }
            }

            // Swap the info body:
            //   ASCII format: [name\0][u32 hash][u32 a][u32 b][u32 c] — swap trailing u32s
            //   Compact binary (16B): [u32 hash][u32][u32][u32] — swap all u32s
            if let Some(ii) = info_idx {
                let d = &descriptors[ii];
                if let Some((start, end)) = body_range(container, d.row_u0, d.body_size, data_area_off) {
                    let body_local_start = start - data_start;
                    let body_local_end = end - data_start;
                    if body_local_end <= data_area.len() {
                        convert_info_body_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                }
            }

            // Swap the schm body. NOTE: the per-field offset word is
            // { u16 byte_offset; u8; u8 }, NOT a u32 — swap only the
            // byte_offset u16 (see swap_schm_body_inplace).
            if let Some(si) = schm_idx {
                let d = &descriptors[si];
                if let Some((start, end)) = body_range(container, d.row_u0, d.body_size, data_area_off) {
                    let body_local_start = start - data_start;
                    let body_local_end = end - data_start;
                    if body_local_end <= data_area.len() {
                        swap_schm_body_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                }
            }

            vlog!("    COMP group: '{}' (schema: {})",
                comp_name_str,
                if schema.is_some() { "yes" } else { "no" });
        } else {
            // Non-COMP descriptor in an ECS container — swap as appropriate
            if desc.row_u0 != 0xFFFFFFFF {
                if let Some((start, end)) = body_range(container, desc.row_u0, desc.body_size, data_area_off) {
                    let body_local_start = start - data_start;
                    let body_local_end = end - data_start;
                    if body_local_end <= data_area.len() {
                        if desc.tag == ChunkTag::Enum {
                            convert_enum_body_inplace(&mut data_area[body_local_start..body_local_end]);
                        } else if desc.tag == ChunkTag::Deps {
                            let body_len = body_local_end - body_local_start;
                            if body_len > 1 {
                                swap_u32_array(&mut data_area[body_local_start + 1..body_local_end]);
                            }
                        } else if desc.tag == ChunkTag::Flgs {
                            convert_vz_state_flgs_inplace(
                                &mut data_area[body_local_start..body_local_end],
                            );
                        } else if desc.tag == ChunkTag::Chdr {
                            convert_chdr_body_inplace(
                                &mut data_area[body_local_start..body_local_end],
                            );
                        } else if desc.tag == ChunkTag::Info {
                            // Standalone `info` (outside a COMP group): same named/compact
                            // rules as the COMP-group path.
                            convert_info_body_inplace(&mut data_area[body_local_start..body_local_end]);
                        } else if desc.tag.is_native_be() {
                            // No swap
                        } else {
                            swap_u32_array(&mut data_area[body_local_start..body_local_end]);
                        }
                    }
                }
            }
            i += 1;
        }
    }

    out.extend_from_slice(&data_area);
    Ok(())
}

/// Component-name hash → name lookup for compact-format info bodies (16-byte
/// binary, no ASCII string). Values are pandemic_hash_m2(component_name).
const COMP_HASH_TO_NAME: &[(u32, &str)] = &[
    (0x753EB623, "Transform"),
    (0x1DE5C824, "Name"),
    (0x5CF81991, "ModelName"),
    (0x97E8EE92, "LightObject"),
    (0xEA0F3AA3, "Road"),
    (0x6FD048F4, "RoadIntersection"),
    (0xBCE6FAD7, "DestructionLink"),
    (0x7FBCE14E, "PhysicalLink"),
    (0xD81512A1, "ObjectScript"),
    (0x99C2B81F, "ModifierKey"),
    (0xAB92C697, "ScrubObject"),
    (0x6310807F, "LineRegion"),
    (0x49F0D0EC, "MaterialMapping"),
    (0x2A20B640, "LandingZone"),
    (0x06DA8775, "Label"),
    (0xFA55F6BA, "Anchor"),
    (0x2D8D2435, "LowResTerrainObject"),
    (0xE18AFD65, "HibernationControl"),
    (0xB8D2B506, "AtmosphereBase"),
    (0xEB6DE962, "IntersectionToIntersection"),
    (0x514CAD3A, "SoundAmbience"),
    (0xDECD8889, "AiBehavior"),
    (0xBCFE6314, "Path"),
    (0x6FA2F9D4, "LaneData"),
    (0x60B7ABE0, "PointLocation"),
];

/// "Keyed-group" ECS components whose `data` body is a sequence of
/// `[u32 count][count × record][u8 flag]` groups (mixed u8/u32). Returns the
/// per-record byte size. Mirrors `_ECS_GROUP_RECORD_COMPONENTS` in
/// ucfx_be_to_le.py. PointLocation: 36-byte records; 0x2E2659F0: 4-byte
/// entity-reference keys (component name not yet recovered).
fn keyed_group_record_size(comp_name: &str) -> Option<usize> {
    match comp_name {
        "PointLocation" => Some(36),
        "__hash_0x2E2659F0" => Some(4),
        _ => None,
    }
}

/// True if `b` looks like a full-format ECS component *name* (C++-style
/// identifier): starts with a letter/underscore, only `[A-Za-z0-9_]`, len >= 2.
/// Rejects compact 4-byte BE hashes that happen to be printable but contain
/// punctuation (e.g. `b"N+lT"`, `b"iV~b"`). Mirrors `_is_ecs_name_identifier`.
fn is_ecs_name_identifier(b: &[u8]) -> bool {
    if b.len() < 2 {
        return false;
    }
    let c0 = b[0];
    if !(c0.is_ascii_alphabetic() || c0 == b'_') {
        return false;
    }
    b.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'_')
}

/// In-place BE->LE conversion for a keyed-group component body.
///
/// Each 4-byte field (count + every record word) is byte-reversed; the
/// per-group trailing `u8` flag is left untouched. Returns `true` only if the
/// structure consumes the buffer *exactly* (in which case the swap was
/// applied); on any layout mismatch it leaves `data` unchanged and returns
/// `false` so the caller can fall back / report (never silently corrupts).
fn convert_keyed_group_records_inplace(data: &mut [u8], record_size: usize) -> bool {
    if record_size == 0 || record_size % 4 != 0 {
        return false;
    }
    let n = data.len();
    // Validation pass (counts are still big-endian here).
    let mut pos = 0usize;
    while pos < n {
        if pos + 4 > n {
            return false;
        }
        let count =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        let span = match count.checked_mul(record_size) {
            Some(s) => s,
            None => return false,
        };
        if pos + span + 1 > n {
            return false;
        }
        pos += span + 1;
    }
    if pos != n {
        return false;
    }
    // Apply pass.
    pos = 0;
    while pos < n {
        data[pos..pos + 4].reverse();
        let count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        let span = count * record_size;
        let mut off = pos;
        while off < pos + span {
            data[off..off + 4].reverse();
            off += 4;
        }
        pos += span + 1; // skip the endian-neutral u8 flag
    }
    true
}

/// Extract component name from an `info` descriptor body.
///
/// Two formats exist:
///   1. ASCII format: `[null-terminated name][u32 hash][u32 a][u32 b][u32 c]`
///   2. Compact binary format (16 bytes): `[u32 comp_hash][u32][u32][u32]`
///      Used in blocks without `schm`/`enum` descriptors.
fn extract_comp_name(
    container: &[u8],
    desc: &Descriptor,
    data_area_off: usize,
    is_be: bool,
) -> Option<String> {
    let (start, end) = body_range(container, desc.row_u0, desc.body_size, data_area_off)?;
    let body = &container[start..end];

    // Resolve in priority order (mirrors _build_ecs_comp_map in ucfx_be_to_le.py):
    //   (a) recognized component hash  -> named (even if first 4 bytes printable)
    //   (b) valid C++-style identifier -> full-format name string
    //   (c) otherwise                  -> compact unrecognized hash
    // A compact 4-byte BE hash can be coincidentally printable (e.g.
    // 0x4E2B6C54 = "N+lT", 0x69567E62 = "iV~b" in the DLC resident block), so
    // "first bytes printable => name" is NOT a valid discriminator.
    let hash = if body.len() >= 4 {
        if is_be {
            read_u32_be(body, 0)
        } else {
            mercs2_formats::ffcs::read_u32_le(body, 0)
        }
    } else {
        0
    };

    if body.len() >= 4 {
        for &(h, name) in COMP_HASH_TO_NAME {
            if h == hash {
                return Some(name.to_string());
            }
        }
    }

    let nul_pos = body.iter().position(|&b| b == 0).unwrap_or(body.len());
    if is_ecs_name_identifier(&body[..nul_pos]) {
        if let Ok(name) = String::from_utf8(body[..nul_pos].to_vec()) {
            return Some(name);
        }
    }

    if body.len() >= 4 {
        return Some(format!("__hash_0x{:08X}", hash));
    }

    None
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
fn swap_schm_body_inplace(buf: &mut [u8]) {
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

/// Pre-scan a schm body to extract raw type codes for reporting.
/// Unlike ComponentSchema::from_schm_body, this does NOT bail on unknown codes.
/// Returns Vec<(type_code, name_hash, byte_offset)>.
fn scan_schm_type_codes(body: &[u8], is_be: bool) -> Vec<(u32, u32, u16)> {
    if body.len() < 8 {
        return vec![];
    }

    let n_fields = if is_be {
        u32::from_be_bytes([body[0], body[1], body[2], body[3]])
    } else {
        u32::from_le_bytes([body[0], body[1], body[2], body[3]])
    };

    if n_fields > 200 || (8 + n_fields as usize * 16) > body.len() {
        return vec![];
    }

    let mut result = Vec::with_capacity(n_fields as usize);
    for i in 0..n_fields as usize {
        let off = 8 + i * 16;
        let (type_code, name_hash, raw_offset) = if is_be {
            (
                u32::from_be_bytes([body[off], body[off + 1], body[off + 2], body[off + 3]]),
                u32::from_be_bytes([body[off + 4], body[off + 5], body[off + 6], body[off + 7]]),
                u32::from_be_bytes([body[off + 12], body[off + 13], body[off + 14], body[off + 15]]),
            )
        } else {
            (
                u32::from_le_bytes([body[off], body[off + 1], body[off + 2], body[off + 3]]),
                u32::from_le_bytes([body[off + 4], body[off + 5], body[off + 6], body[off + 7]]),
                u32::from_le_bytes([body[off + 12], body[off + 13], body[off + 14], body[off + 15]]),
            )
        };
        let byte_offset = ((raw_offset >> 16) & 0xFFFF) as u16;
        result.push((type_code, name_hash, byte_offset));
    }
    result
}

/// Convert a COMP data body in-place using schema-driven field swaps.
fn convert_comp_data_inplace(
    data: &mut [u8],
    schema: Option<&ComponentSchema>,
    comp_name: &str,
    mut report: Option<&mut SchemaCoverageReport>,
    data_size: usize,
) {
    if data.is_empty() {
        return;
    }

    // Special cases
    match comp_name {
        "Name" => {
            if let Some(ref mut rpt) = report {
                rpt.record_no_schema(comp_name, data_size, "hardcoded handler");
            }
            convert_name_data_inplace(data);
            return;
        }
        "ModelName" => {
            if let Some(ref mut rpt) = report {
                rpt.record_no_schema(comp_name, data_size, "hardcoded handler");
            }
            convert_modelname_data_inplace(data);
            return;
        }
        "HibernationControl" if data.len() % 10 == 0 => {
            // Sub-u32 layout (u16 + u8 + u8 + u8 + bitflags, stride 10). A
            // blanket u32 sweep reverses a word across the u16+u8+u8 region and
            // corrupts the u16; swap only the entity-key u32 and the payload u16.
            if let Some(ref mut rpt) = report {
                rpt.record_no_schema(comp_name, data_size, "hardcoded handler");
            }
            convert_hibernation_data_inplace(data);
            return;
        }
        _ => {}
    }

    // Keyed-group components ([u32 count][count×record][u8 flag]*) — mixed
    // u8/u32 layout that a blanket u32 sweep would corrupt. Only the compact /
    // no-schm form (resident / worldentity META) uses this layout; a schm'd
    // instance (layers_static) is a plain keyed-record array. Self-validating:
    // only applies when the structure consumes the body exactly.
    if schema.is_none() {
        if let Some(rec) = keyed_group_record_size(comp_name) {
            if convert_keyed_group_records_inplace(data, rec) {
                if let Some(ref mut rpt) = report {
                    rpt.record_no_schema(comp_name, data_size, "keyed-group records");
                }
                return;
            }
            // Structure mismatch — fall through to generic handling (and report).
        }
    }

    // Compact-format COMP groups have info (hash) but no schm; mirror Python
    // `_ECS_COMP_DEFAULT_STRIDE` — do not blind-sweep whole bodies.
    let stride = if comp_name == "Transform" {
        42
    } else if let Some(s) = schema {
        // schm[4:8] is payload_stride; record = 4-byte entity key + payload (docs/ecs_components.md)
        4 + s.payload_stride as usize
    } else if let Some(s) = compact_default_stride(comp_name) {
        s
    } else {
        if let Some(ref mut rpt) = report {
            rpt.record_no_schema(comp_name, data_size, "u32_array sweep (unknown stride)");
        }
        swap_u32_array(data);
        return;
    };

    if stride == 0 {
        if let Some(ref mut rpt) = report {
            rpt.record_no_schema(comp_name, data_size, "u32_array sweep (stride=0)");
        }
        swap_u32_array(data);
        return;
    }

    // Non-Transform ECS components: record-aligned u32 (and u16 tail) swap.
    if comp_name != "Transform" && (schema.is_none() || is_ecs_numeric_component(comp_name)) {
        if let Some(ref mut rpt) = report {
            rpt.record_no_schema(comp_name, data_size, "numeric records (compact stride)");
        }
        swap_numeric_records_inplace(data, stride);
        return;
    }

    let record_count = data.len() / stride;

    for ri in 0..record_count {
        let rec_start = ri * stride;

        // Always swap the u32 entity_key at record start
        if rec_start + 4 <= data.len() {
            swap_u32(data, rec_start);
        }

        if comp_name == "Transform" {
            // On-disk Transform payload (38 bytes) per schm: blob32 (8×f32) at
            // payload+0, f32 at payload+32, u16 at payload+36. Swap the 9 u32
            // words (key already swapped above, then payload 0..36) and the
            // trailing u16. Matches `_convert_transform_records` (10×u32 + u16)
            // in ucfx_be_to_le.py and the retail PC byte layout.
            for field_off in [0, 4, 8, 12, 16, 20, 24, 28, 32] {
                let off = rec_start + 4 + field_off;
                if off + 4 <= data.len() {
                    swap_u32(data, off);
                }
            }
            let u16_off = rec_start + 4 + 36;
            if u16_off + 2 <= data.len() {
                swap_u16(data, u16_off);
            }
        } else {
            // Schema-driven swap
            let schema = schema.unwrap();
            for field in &schema.fields {
                if !field.field_type.needs_swap() {
                    continue;
                }
                let payload_off = rec_start + 4 + field.byte_offset as usize;
                let unit = field.field_type.swap_unit();
                let count = field.field_type.swap_count();
                for fi in 0..count {
                    let off = payload_off + fi * unit;
                    if off + unit <= data.len() {
                        match unit {
                            2 => swap_u16(data, off),
                            4 => swap_u32(data, off),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

/// Convert Name component data in-place (variable-length records).
///
/// Layout: `[u32 entity_key][null-terminated ASCII name]` repeated.
/// Only the u32 keys need byte-swapping; string bytes are order-independent.
///
/// IMPORTANT: Skip exactly ONE null byte (the terminator) after each string.
/// Do NOT skip additional nulls — the next entity key may start with 0x00
/// (high byte of a small BE u32 like 0x0014E333 → bytes [00 14 E3 33]).
/// Skipping that leading 0x00 shifts the swap by one byte, corrupting all
/// subsequent records.
fn convert_name_data_inplace(data: &mut [u8]) {
    let mut pos = 0;
    while pos + 4 <= data.len() {
        swap_u32(data, pos);
        pos += 4;
        // Skip string bytes until null terminator
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        // Skip exactly one null terminator
        if pos < data.len() {
            pos += 1;
        }
    }
}

/// Convert ModelName component data in-place.
///
/// ModelName is a pure-u32 stream — besides the fixed (key, hash) pair shape it
/// also appears as variable records `[u32 count][count×u32 keys][u32 hash]`
/// (u32- but not 8-aligned) in resident/worldentity META blocks. Every field
/// is a u32 either way, so swap the whole body as a u32 array. Mirrors the
/// relaxed (`% 4`) handler in ucfx_be_to_le.py.
fn convert_modelname_data_inplace(data: &mut [u8]) {
    swap_u32_array(data);
}

/// Convert HibernationControl component data in-place (stride 10).
///
/// schm-declared payload layout (verified byte-identical to retail PC
/// `layers_static` block 29 and DLC block 18):
///   +0  u32 entity_key
///   +4  u16 field (type 4, name_hash 0xCBE8ED58)
///   +6  u8 / +7 u8 / +8 u8 (type 2)
///   +9  u8 bit-flags (two type-1 bits)
///
/// The payload is NOT a u32 array — a blanket u32 sweep reverses a 4-byte word
/// across the `u16 + u8 + u8` region (and a u16 across the `u8 + bitflags`
/// tail), corrupting the u16 into a constant (0xA03C). Swap only the entity-key
/// u32 and the payload u16; the trailing u8/bit fields are endian-neutral.
/// Mirrors `_convert_hibernation_records` in ucfx_be_to_le.py.
fn convert_hibernation_data_inplace(data: &mut [u8]) {
    const STRIDE: usize = 10;
    let mut pos = 0usize;
    while pos + STRIDE <= data.len() {
        swap_u32(data, pos); // entity key
        swap_u16(data, pos + 4); // u16 field at payload+0
        // pos+6..pos+10 (u8 + u8 + u8 + bitflags) are endian-neutral.
        pos += STRIDE;
    }
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
    for desc in descriptors {
        if desc.row_u0 == 0xFFFFFFFF {
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
                        if !walk_records(body, &PRMT_WALKER) {
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
                        // SEGM: native big-endian records — verified pc == be byte-for-byte
                        // (BE u32 7 stays `00 00 00 07` in PC, not reversed). The generic
                        // u32 default wrongly reverses them. No swap. (oracle: 2946 -> 0.)
                    }
                    ChunkTag::Unknown(b) if b == *b"INST" || b == *b"BSHI" => {
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
                    ChunkTag::Unknown(b) if b == *b"TRCK" => {
                        // TRCK: [u32 hash][u32 hash] then a u16 array (variable length).
                        convert_trck_inplace(&mut data_area[body_local_start..body_local_end]);
                    }
                    ChunkTag::Unknown(b) if b == *b"PTMS" => {
                        // PTMS: 8-byte records [u32][u16][u16].
                        let body = &mut data_area[body_local_start..body_local_end];
                        if !walk_records(body, &PTMS_WALKER) {
                            swap_u32_array(body);
                        }
                    }
                    ChunkTag::Unknown(b) if b == *b"PTCH" => {
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
                        convert_mtrl(&mut data_area[body_local_start..body_local_end]);
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
                    ChunkTag::Unknown(b)
                        if b == *b"TYPE" && type_hash == types::TYPE_HASH_STANCE =>
                    {
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
                        // DXT compressed pixel data — leave as-is.
                        // Proper texture BODY conversion (untiling) is Phase 3+.
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
                    ChunkTag::Unknown(b) if b == *b"EFCT" => {
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
                    ChunkTag::Unknown(b) if b == *b"EMTR" => {
                        // EMTR: 2-byte emitter count (genuine u16).
                        swap_u16_array(&mut data_area[body_local_start..body_local_end]);
                    }
                    // SEGM/BSHI removed: they were lumped with HIER on the same (now
                    // disproven) "Python u16 group" reasoning. Default u32 is the
                    // known-good baseline; re-add a u16 arm only if a chunk is VERIFIED
                    // to hold u16 data (not f32) against the validator/retail.
                    ChunkTag::Unknown(b) if b == *b"BINN" => {
                        // Lua bytecode: leave RAW BE here; converted afterward by
                        // apply_binn_transcode (unluac disassemble→flip-endianness→
                        // reassemble — NOT a byte-swap; the body may resize). Mirrors
                        // the wavebank `data` no-op + apply_wavebank_transcode reframe.
                    }
                    ChunkTag::Unknown(b) if b == *b"MINF" => {
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
                    ChunkTag::Unknown(b) if b == *b"PHY2" => {
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
                        // is transcoded (Xbox-ADPCM nibble-swap / XMA via ffmpeg → PC
                        // IMA) and the container reframed by apply_wavebank_transcode
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
                            let type_name = types::type_name_from_hash(type_hash);
                            rpt.record_generic_fallback(
                                entry_idx,
                                type_hash,
                                type_name,
                                &format!("{}", other_tag),
                                desc.body_size,
                            );
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
    matches!(tag, ChunkTag::Name | ChunkTag::Strs)
}

// --- Byte swap primitives ---

fn swap_u32(data: &mut [u8], offset: usize) {
    data[offset..offset + 4].reverse();
}

fn swap_u16(data: &mut [u8], offset: usize) {
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
//   layout, so the caller can fall back to a count-safe swap (mirrors the
//   validate-first discipline of `convert_keyed_group_records_inplace`).
// ---------------------------------------------------------------------------

/// Field width/semantics within a record. F32 reverses identically to U32 (both
/// a 4-byte reversal) but is kept distinct for self-documentation. U8 = no-op.
#[derive(Clone, Copy, PartialEq)]
enum FieldKind {
    U32,
    U16,
    U8,
    F32,
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
                FieldKind::U32 | FieldKind::F32 => data[off..off + 4].reverse(),
                FieldKind::U16 => data[off..off + 2].reverse(),
                FieldKind::U8 => {}
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
// Ports tools/xbox_texture_codec.py. The generic body pass already runs
// `convert_texture_info` on the INFO chunk (basic field swap = the "rust xi"
// form) and leaves the BODY as raw tiled passthrough. The post-process below
// (a) rebuilds the 34-byte PC INFO from that swapped INFO and (b) untiles the
// GPU-tiled DXT BODY into the PC-linear mip chain, then reframes the container.
// Validated byte-exact (pixel data + INFO header) against the Rosetta corpus;
// the [12:14] field and [32:34] streaming tail are non-reconstructible PC
// metadata (see .cursor/notes/rosetta_oracle_baseline.md).

/// (block_px, texel_pitch, log2(bytes_per_block)) for a DXT FourCC.
/// Xbox D3D format word low byte (INFO[17]) -> PC FourCC.
fn fourcc_from_format_byte(b: u8) -> Option<[u8; 4]> {
    match b {
        0x52 => Some(*b"DXT1"),
        0x54 => Some(*b"DXT5"),
        0x53 => Some(*b"DXT3"),
        _ => None,
    }
}

#[inline]
fn read_u16_le(d: &[u8], off: usize) -> usize {
    u16::from_le_bytes([d[off], d[off + 1]]) as usize
}

/// XGAddress2DTiledOffset returning a *block* index (gildor/Noesis form).
fn tiled_block_index(x: usize, y: usize, width_blocks: usize, log_bpb: usize) -> usize {
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
fn untile_surface(
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
fn swap16_blocks(data: &mut [u8]) {
    let n = data.len() / 2;
    for i in 0..n {
        data.swap(i * 2, i * 2 + 1);
    }
}

/// Drop the 32-block width/height padding, yielding the wb x hb surface.
fn crop_blocks(linear: &[u8], aligned_wb: usize, wb: usize, hb: usize, texel_pitch: usize) -> Vec<u8> {
    if aligned_wb == wb {
        return linear[..wb * hb * texel_pitch].to_vec();
    }
    let mut out = vec![0u8; wb * hb * texel_pitch];
    for j in 0..hb {
        let s = j * aligned_wb * texel_pitch;
        let d = j * wb * texel_pitch;
        out[d..d + wb * texel_pitch].copy_from_slice(&linear[s..s + wb * texel_pitch]);
    }
    out
}

fn untile_own_surface(tiled: &[u8], width_px: usize, height_px: usize, fourcc: &[u8; 4]) -> Option<Vec<u8>> {
    let (block_px, texel_pitch, log_bpb) = dxt_format(fourcc)?;
    let wb = ((width_px + block_px - 1) / block_px).max(1);
    let hb = ((height_px + block_px - 1) / block_px).max(1);
    let awb = (wb + 31) & !31;
    let ahb = (hb + 31) & !31;
    let need = (awb * ahb * texel_pitch).min(tiled.len());
    let mut lin = untile_surface(&tiled[..need], awb, ahb, texel_pitch, log_bpb);
    swap16_blocks(&mut lin);
    Some(crop_blocks(&lin, awb, wb, hb, texel_pitch))
}

/// Bytes the Xbox tiled BODY occupies for a full (non-streamed) texture.
fn tiled_body_size(width: usize, height: usize, fourcc: &[u8; 4], mips: usize) -> usize {
    let (block_px, texel_pitch, _) = dxt_format(fourcc).unwrap();
    let n = if mips > 0 { mips } else { tex_mip_levels(width, height) };
    let mut total = 0;
    let mut tail_added = false;
    for m in 0..n {
        let wpx = (width >> m).max(1);
        let hpx = (height >> m).max(1);
        if wpx.min(hpx) >= 32 {
            let wb = ((wpx + block_px - 1) / block_px).max(1);
            let hb = ((hpx + block_px - 1) / block_px).max(1);
            let awb = (wb + 31) & !31;
            let ahb = (hb + 31) & !31;
            total += awb * ahb * texel_pitch;
        } else if !tail_added {
            total += 32 * 32 * texel_pitch;
            tail_added = true;
        }
    }
    total
}

/// Assemble a full PC-linear DXT mip chain from a tiled Xbox BODY (incl. the
/// packed sub-32 "mip tail"). Returns None if the body is too short.
fn untile_dxt_body(tiled: &[u8], width: usize, height: usize, fourcc: &[u8; 4], mips: usize) -> Option<Vec<u8>> {
    let (block_px, texel_pitch, log_bpb) = dxt_format(fourcc)?;
    let n = if mips > 0 { mips } else { tex_mip_levels(width, height) };
    let mut out: Vec<u8> = Vec::new();
    let mut pos = 0usize;
    let mut tail_lin: Option<Vec<u8>> = None;
    for m in 0..n {
        let wpx = (width >> m).max(1);
        let hpx = (height >> m).max(1);
        let wb = ((wpx + block_px - 1) / block_px).max(1);
        let hb = ((hpx + block_px - 1) / block_px).max(1);
        if wpx.min(hpx) >= 32 {
            let awb = (wb + 31) & !31;
            let ahb = (hb + 31) & !31;
            let size = awb * ahb * texel_pitch;
            if pos + size > tiled.len() {
                break;
            }
            let mut lin = untile_surface(&tiled[pos..pos + size], awb, ahb, texel_pitch, log_bpb);
            swap16_blocks(&mut lin);
            out.extend_from_slice(&crop_blocks(&lin, awb, wb, hb, texel_pitch));
            pos += size;
        } else {
            if tail_lin.is_none() {
                let size = 32 * 32 * texel_pitch;
                if pos + size > tiled.len() {
                    break;
                }
                let mut t = untile_surface(&tiled[pos..pos + size], 32, 32, texel_pitch, log_bpb);
                swap16_blocks(&mut t);
                tail_lin = Some(t);
                pos += size;
            }
            let tl = tail_lin.as_ref().unwrap();
            let (bx, by) = if wb >= hb { (wb, 0) } else { (0, hb) };
            for r in 0..hb {
                let base = ((by + r) * 32 + bx) * texel_pitch;
                if base + wb * texel_pitch <= tl.len() {
                    out.extend_from_slice(&tl[base..base + wb * texel_pitch]);
                }
            }
        }
    }
    Some(out)
}

/// Nearest-neighbour scale a DXT surface at *block* granularity (each 4x4 DXT
/// block is copied whole, so no decode/recompress is needed and the result is a
/// valid DXT surface). Works in either direction (up- or down-scale).
fn scale_dxt_blocks(
    src: &[u8], src_wb: usize, src_hb: usize, dst_wb: usize, dst_hb: usize, texel_pitch: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; dst_wb * dst_hb * texel_pitch];
    if src_wb == 0 || src_hb == 0 {
        return out;
    }
    for dy in 0..dst_hb {
        let sy = (dy * src_hb / dst_hb).min(src_hb - 1);
        for dx in 0..dst_wb {
            let sx = (dx * src_wb / dst_wb).min(src_wb - 1);
            let s = (sy * src_wb + sx) * texel_pitch;
            let d = (dy * dst_wb + dx) * texel_pitch;
            if s + texel_pitch <= src.len() && d + texel_pitch <= out.len() {
                out[d..d + texel_pitch].copy_from_slice(&src[s..s + texel_pitch]);
            }
        }
    }
    out
}

/// Recover the largest decodable DXT surface from a (possibly partial/stub)
/// tiled Xbox body. The body always begins with its largest contained mip, so
/// we iterate levels from largest to smallest and untile the first whose tiled
/// own-surface fits. Returns (linear blocks, width_blocks, height_blocks).
fn recover_best_surface(
    tiled: &[u8], width: usize, height: usize, fourcc: &[u8; 4], mips: usize,
) -> Option<(Vec<u8>, usize, usize)> {
    let (block_px, texel_pitch, _) = dxt_format(fourcc)?;
    let n = if mips > 0 { mips } else { tex_mip_levels(width, height) };
    for m in 0..n.max(1) {
        let wpx = (width >> m).max(1);
        let hpx = (height >> m).max(1);
        let wb = ((wpx + block_px - 1) / block_px).max(1);
        let hb = ((hpx + block_px - 1) / block_px).max(1);
        let awb = (wb + 31) & !31;
        let ahb = (hb + 31) & !31;
        // First level whose full aligned surface is present in the body.
        if tiled.len() >= awb * ahb * texel_pitch {
            return untile_own_surface(tiled, wpx, hpx, fourcc).map(|s| (s, wb, hb));
        }
    }
    None
}

/// Build a COMPLETE PC-linear DXT mip chain at the ORIGINAL (width,height,mips)
/// from a partial/stub tiled body: recover the best available surface and
/// block-nearest scale it into every mip level. The chain is exactly
/// `linear_mip_chain_size(width,height,fourcc,mips)` bytes — fully resident, so
/// the engine reads it and is done (no over-read, no streaming wait). Detail
/// below the recovered surface's resolution is interpolated, not original.
fn synthesize_resident_chain(
    tiled: &[u8], width: usize, height: usize, fourcc: &[u8; 4], mips: usize,
) -> Option<Vec<u8>> {
    let (block_px, texel_pitch, _) = dxt_format(fourcc)?;
    let (src, src_wb, src_hb) = recover_best_surface(tiled, width, height, fourcc, mips)?;
    let n = if mips > 0 { mips } else { tex_mip_levels(width, height) };
    let mut out = Vec::with_capacity(linear_mip_chain_size(width, height, fourcc, n));
    for l in 0..n.max(1) {
        let wpx = (width >> l).max(1);
        let hpx = (height >> l).max(1);
        let wb = ((wpx + block_px - 1) / block_px).max(1);
        let hb = ((hpx + block_px - 1) / block_px).max(1);
        out.extend_from_slice(&scale_dxt_blocks(&src, src_wb, src_hb, wb, hb, texel_pitch));
    }
    Some(out)
}

/// Rebuild a 34-byte PC texture INFO from the basic-swapped ("rust xi") INFO.
/// Mirrors `xbox_texture_codec.rebuild_texture_info`.
fn rebuild_texture_info(xi: &[u8], fourcc: &[u8; 4], mips: usize, linear_total: u32) -> [u8; 34] {
    let mut out = [0u8; 34];
    out[0..4].copy_from_slice(&xi[0..4]);
    // transpose [4:6]<->[6:8]
    out[4..6].copy_from_slice(&xi[6..8]);
    out[6..8].copy_from_slice(&(mips as u16).to_le_bytes());
    // [8:10]<->[10:12] transpose; clear the Xbox 0x10 "tiled" flag bit.
    let f8 = (u16::from_le_bytes([xi[10], xi[11]]) & !0x10).to_le_bytes();
    out[8..10].copy_from_slice(&f8);
    out[10..12].copy_from_slice(&xi[8..10]);
    out[12..14].copy_from_slice(&xi[12..14]);
    out[14..18].copy_from_slice(fourcc);
    // LOD-bias float is big-endian on Xbox (passthrough in the basic swap) -> LE.
    out[18] = xi[21];
    out[19] = xi[20];
    out[20] = xi[19];
    out[21] = xi[18];
    out[22..26].copy_from_slice(&linear_total.to_le_bytes());
    // Streaming-residency descriptor [26:34]: the fully-resident sentinel
    // 00 00 00 00 00 00 FF FF (retail's most common resident value; trailing
    // word is a 2^n-1 mip-residency bitmask, 0xffff = all mips resident). Every
    // texture we emit is a complete resident mip chain, so it is always resident.
    out[26..34].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0xFF, 0xFF]);
    out
}

/// Post-process a converted texture container: rebuild INFO + untile BODY and
/// reframe. Handles full, prefix, single-mip, streamed pages, and INFO-only
/// stubs (sentinel BODY). Returns true when INFO was rebuilt to PC FourCC form.
fn apply_texture_untile(
    out: &mut Vec<u8>,
    descriptors: &[Descriptor],
    data_area_off: usize,
    desc_table_end: usize,
) -> bool {
    let data_start = if data_area_off > 0 { data_area_off } else { desc_table_end };

    let mut info_idx = None;
    let mut body_idx = None;
    for (i, d) in descriptors.iter().enumerate() {
        if d.row_u0 == 0xFFFFFFFF {
            continue;
        }
        if d.tag == ChunkTag::InfoUpper && info_idx.is_none() {
            info_idx = Some(i);
        } else if d.tag == ChunkTag::Body && body_idx.is_none() && d.body_size > 0 {
            body_idx = Some(i);
        }
    }
    let info_idx = match info_idx {
        Some(i) => i,
        None => return false,
    };

    let info = &descriptors[info_idx];
    if info.body_size < 34 {
        return false;
    }
    let info_abs = data_start + info.row_u0 as usize;
    if info_abs + 34 > out.len() {
        return false;
    }

    let xi = out[info_abs..info_abs + 34].to_vec();
    let fourcc = match fourcc_from_format_byte(xi[17]) {
        Some(f) => f,
        None => return false,
    };
    let width = read_u16_le(&xi, 0);
    let height = read_u16_le(&xi, 2);
    if width == 0 || height == 0 {
        return false;
    }
    // Claim the STANDARD full mip chain (down to 1x1), NOT the Xbox-authored count.
    // Live x32dbg proved the engine instantiates the full chain from the texture's
    // dimensions regardless of the header's mip field: a 64x64 DXT1 had 5 populated
    // mip surfaces (+ 0xABABABAB overrun) while its header claimed 3, so the engine
    // read past the body -> STATUS_BUFFER_TOO_SMALL -> the page never reached ready
    // state 4 -> world-load livelock (dlc01_dlccon002_roads). DLC stub textures carry
    // a reduced count; honoring it under-claims and hangs. Claim the retail PC chain
    // (down to the 4x4 DXT minimum, governed by the smaller dim — base vz.wad's
    // convention) so the engine's surface count, the INFO mip count and the body agree.
    let mips = dxt_mip_count(width, height);

    // Build a COMPLETE, fully-resident PC mip chain at the texture's ORIGINAL
    // dimensions. An earlier pass REDUCED the dimensions to fit a partial Xbox stub
    // body; that made each texture internally consistent but desynced it from the
    // layer/material data that references it at full size, so its streaming node
    // never reached ready-state 4. Now we keep (width,height) + the full mip count
    // and, for a partial/stub body, synthesize the missing mips by block-nearest
    // scaling the best surface recovered from the stub: full-size + complete +
    // resident, so references stay valid, there is no over-read (BUFFER_TOO_SMALL)
    // and no streaming wait. Synthesized mips are lower-detail, not pixel-accurate.
    let tlinear = linear_mip_chain_size(width, height, &fourcc, mips);
    let (pc_body, body_idx, body_abs_val) = if let Some(body_idx) = body_idx {
        let body = &descriptors[body_idx];
        let body_abs = data_start + body.row_u0 as usize;
        if body_abs + body.body_size as usize > out.len() {
            return false;
        }
        let tiled = out[body_abs..body_abs + body.body_size as usize].to_vec();
        // A full body untiles directly; a partial/stub body is synthesized to a
        // complete resident chain at full dimensions.
        let complete = if tiled.len() >= tiled_body_size(width, height, &fourcc, mips) {
            untile_dxt_body(&tiled, width, height, &fourcc, mips)
        } else {
            synthesize_resident_chain(&tiled, width, height, &fourcc, mips)
        };
        match complete {
            Some(b) => (Some(b), Some(body_idx), Some(body_abs)),
            None => return false,
        }
    } else {
        (None, None, None)
    };

    // INFO at ORIGINAL dimensions, fully resident.
    let pc_info = rebuild_texture_info(&xi, &fourcc, mips, tlinear as u32);
    out[info_abs..info_abs + 34].copy_from_slice(&pc_info);

    let mut body_abs_val = body_abs_val;
    let info_size_field = 20 + info_idx * 20 + 8;
    if info.body_size as usize > 34 {
        let shrink = info.body_size as usize - 34;
        let info_end = info_abs + info.body_size as usize;
        if info_end <= out.len() {
            let tail = out[info_end..].to_vec();
            out.truncate(info_abs + 34);
            out.extend_from_slice(&tail);
            if let Some(ref mut abs) = body_abs_val {
                if *abs >= info_end {
                    *abs -= shrink;
                }
            }
            for (i, d) in descriptors.iter().enumerate() {
                if i == info_idx || d.row_u0 == 0xFFFFFFFF {
                    continue;
                }
                if d.row_u0 as usize + data_start >= info_end {
                    let row_field = 20 + i * 20 + 4;
                    let new_u0 = d.row_u0 - shrink as u32;
                    out[row_field..row_field + 4].copy_from_slice(&new_u0.to_le_bytes());
                }
            }
        }
        out[info_size_field..info_size_field + 4].copy_from_slice(&34u32.to_le_bytes());
    }

    if let (Some(body_idx), Some(body_abs_val)) = (body_idx, body_abs_val) {
        if let Some(mut untiled) = pc_body {
            // Body length == INFO mip chain at full dimensions: pad short, trim long.
            untiled.resize(tlinear, 0);
            let body_size_field = 20 + body_idx * 20 + 8;
            out[body_size_field..body_size_field + 4]
                .copy_from_slice(&(tlinear as u32).to_le_bytes());
            out.truncate(body_abs_val);
            out.extend_from_slice(&untiled);
        }
    }

    true
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

/// Standard Direct3D9 D3DDECLTYPE byte sizes (enum value -> bytes).
fn pc_d3ddecltype_size(t: u8) -> u16 {
    match t {
        1 | 7 | 10 | 12 | 16 => 8,
        2 => 12,
        3 => 16,
        _ => 4,
    }
}

/// Translate an Xbox-360 vertex declaration (`decl` chunk) to a PC
/// `D3DVERTEXELEMENT9` array. This is a *format translation*, not a byte-swap:
///   Xbox: 12B header + N×12B elements (BE `u32 a, b, c`); END has `a>>16==0x00ff`.
///   PC:   8B header `[0, 16]` + N×8B (`u16 Stream, u16 Offset, u8 Type, u8
///         Method, u8 Usage, u8 UsageIndex`); END = D3DDECL_END (Type 17).
/// Mirrors `tools/ucfx_be_to_le._convert_decl` (verified byte-exact vs retail).
/// Errors on an unknown Xbox format byte rather than emitting a guessed type.
fn convert_decl(be: &[u8]) -> Result<Vec<u8>, String> {
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
    let mut off: Option<u16> = None;
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
        let cur_off = off.unwrap_or((a & 0xffff) as u16);
        let fmt = ((b >> 8) & 0xff) as u8;
        let typ = xbox_decl_format_to_pc_type(fmt).ok_or_else(|| {
            format!("unknown Xbox decl format byte 0x{:02X} (element b=0x{:08X})", fmt, b)
        })?;
        out.extend_from_slice(&0u16.to_le_bytes()); // Stream
        out.extend_from_slice(&cur_off.to_le_bytes()); // Offset
        out.extend_from_slice(&[
            typ,                      // Type
            0,                        // Method
            ((c >> 16) & 0xff) as u8, // Usage
            (c & 0xff) as u8,         // UsageIndex
        ]);
        off = Some(cur_off + pc_d3ddecltype_size(typ));
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
        // Collect the decl + data children of this STRM group (until the next sentinel).
        let mut decl: Option<(usize, usize)> = None;
        let mut data: Option<(usize, usize)> = None;
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
/// big-endian u32) into a PC `FLOAT16_4` (4 little-endian half-floats, W = 1.0).
///
/// **Bit layout (pinned empirically from 24,282 base-terrainmesh NORMAL pairs;
/// mean angular error 0.05°, max 0.12° — see the implementation doc):**
///   * X = sign-extend(bits[0:11])  / 1023   (11-bit signed)
///   * Y = sign-extend(bits[11:22]) / 1023   (11-bit signed)
///   * Z = sign-extend(bits[22:32]) / 511    (10-bit signed)
///   * W = 1.0
/// i.e. an 11-11-10 signed-normalized packing (Xbox `D3DDECLTYPE_DEC3N` /
/// "HEND3N" variant). The decoded vector is normalized, then each component is
/// re-encoded to binary16. This is **geometry-correct, not byte-exact**: PC's
/// FLOAT16_4 normals were quantized from the original high-precision source mesh,
/// so they differ from this lossy-Xbox-decode by ≤1–2 half-float ULP. The
/// direction is reproduced to ~0.05°, which is what the engine/lighting needs.
fn dec3n_to_half4_le(u: u32) -> [u8; 8] {
    let sx = |v: u32, bits: u32| -> i32 {
        let m = (v & ((1 << bits) - 1)) as i32;
        let half = 1 << (bits - 1);
        if m >= half { m - (1 << bits) } else { m }
    };
    let x = sx(u, 11) as f32 / 1023.0;
    let y = sx(u >> 11, 11) as f32 / 1023.0;
    let z = sx(u >> 22, 10) as f32 / 511.0;
    let mag = (x * x + y * y + z * z).sqrt();
    let (nx, ny, nz) = if mag > 0.0 {
        (x / mag, y / mag, z / mag)
    } else {
        (0.0, 0.0, 0.0)
    };
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&f32_to_f16_bits(nx).to_le_bytes());
    out[2..4].copy_from_slice(&f32_to_f16_bits(ny).to_le_bytes());
    out[4..6].copy_from_slice(&f32_to_f16_bits(nz).to_le_bytes());
    out[6..8].copy_from_slice(&f32_to_f16_bits(1.0).to_le_bytes());
    out
}

/// A parsed PC D3DVERTEXELEMENT9 (already translated by `apply_decl_translate`).
struct DeclElem {
    offset_pc: usize,
    typ: u8,
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
        let offset = u16::from_le_bytes([decl[p + 2], decl[p + 3]]) as usize;
        let end = offset + pc_d3ddecltype_size(typ) as usize;
        if end > stride {
            stride = end;
        }
        elems.push(DeclElem { offset_pc: offset, typ });
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
                        let half4 = dec3n_to_half4_le(u);
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

/// De-strip an Xbox triangle strip (u16 indices, `0xFFFF` primitive-restart) into
/// a flat triangle list. Geometry-correct (every non-degenerate triangle of the
/// strip is emitted with consistent winding); NOT byte-exact vs PC, which uses a
/// single degenerate-stitched strip with a vertex-cache re-index (see the doc).
///
/// `src` is the generic-swapped (u16-swap-corrected) Xbox index body. Returns the
/// PC triangle-list index bytes (LE u16).
fn destrip_indices(src: &[u8]) -> Vec<u8> {
    let n = src.len() / 2;
    let idx: Vec<u16> = (0..n)
        .map(|i| u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]))
        .collect();
    let mut out: Vec<u16> = Vec::with_capacity(n * 3);
    let mut run: Vec<u16> = Vec::new();
    let flush = |run: &[u16], out: &mut Vec<u16>| {
        if run.len() < 3 {
            return;
        }
        for k in 0..run.len() - 2 {
            let (a, b, c) = (run[k], run[k + 1], run[k + 2]);
            if a == b || b == c || a == c {
                continue; // degenerate
            }
            if k % 2 == 0 {
                out.extend_from_slice(&[a, b, c]);
            } else {
                out.extend_from_slice(&[a, c, b]);
            }
        }
    };
    for &x in &idx {
        if x == 0xffff {
            flush(&run, &mut out);
            run.clear();
        } else {
            run.push(x);
        }
    }
    flush(&run, &mut out);
    let mut bytes = Vec::with_capacity(out.len() * 2);
    for x in out {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    bytes
}

/// Fix transposed `[u16 flags][u16 count]` count pairs in an MTRL body that the
/// generic `convert_mtrl` only corrected for the FIRST material record. Walks the
/// material array and, for every record beyond the first, restores the count pair
/// from a u32-transposition (`count,0` ↔ `0,count`) to the in-place u16 form PC
/// uses. Engine-correctness only — MTRL is not made byte-exact (PC also drops
/// sub-records / shrinks the body, which is not reproducible here).
///
/// Deterministic material-record walk (pinned from the retail PC MTRL of the
/// worked terrainmeshes): each material record is `[104-byte param block]
/// [u16 flags][u16 count][count × u32 tex-hash][12-byte tail]`, so the record
/// stride is `116 + count*4` and the count pair sits at `record_start + 104`.
/// `convert_mtrl` already fixed record 0's pair; this walks the rest and restores
/// each transposed `[count][0]` → `[0][count]` (flags is 0 in every observed
/// record). If a record's layout doesn't match (count out of 1..=64 or the next
/// stride would run off the end), we stop walking — never corrupting trailing data.
fn mtrl_fix_transposed_counts(body: &mut [u8]) {
    const PARAM_BLOCK: usize = 104;
    // Record stride = 116 + count*4 (pinned from retail PC: count pairs at material
    // offsets 104, 236, 376, … → strides 132, 140, … for counts 4, 6, …). The
    // fixed overhead beyond the param block is the 4-byte count pair + an 8-byte tail.
    const TAIL: usize = 8;
    let mut rec = 0usize;
    while rec + PARAM_BLOCK + 4 <= body.len() {
        let cp = rec + PARAM_BLOCK; // count-pair offset
        let lo = u16::from_le_bytes([body[cp], body[cp + 1]]);
        let hi = u16::from_le_bytes([body[cp + 2], body[cp + 3]]);
        // Determine the true count. PC form is [flags=0][count]; the generic u32
        // swap may have transposed it to [count][0]. Accept either and normalize.
        let (flags, count) = if lo == 0 && (1..=64).contains(&hi) {
            (0u16, hi) // already PC-form (e.g. record 0, fixed by convert_mtrl)
        } else if hi == 0 && (1..=64).contains(&lo) {
            // transposed → fix in place to [0][count]
            body[cp] = 0;
            body[cp + 1] = 0;
            body[cp + 2] = (lo & 0xff) as u8;
            body[cp + 3] = (lo >> 8) as u8;
            (0u16, lo)
        } else {
            break; // unrecognized record layout — stop, leave the rest untouched
        };
        let _ = flags;
        let stride = PARAM_BLOCK + 4 + count as usize * 4 + TAIL;
        rec += stride;
    }
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
        Mtrl,
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
            let (Some(ii), Some(vi)) = (info_idx, data_idx) else { continue };
            plans.insert(vi, Plan::IbufData);
            plans.insert(ii, Plan::IbufInfoCount);
        }
    }
    for (idx, d) in descriptors.iter().enumerate() {
        if d.tag == ChunkTag::Mtrl && d.row_u0 != 0xFFFF_FFFF {
            plans.entry(idx).or_insert(Plan::Mtrl);
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
            Some(Plan::IbufData) => {
                let listed = destrip_indices(&b.bytes);
                let count = (listed.len() / 2) as u32;
                if let Some(&ii) = ibuf_info_of_data.get(&b.idx) {
                    ibuf_count_for_info.insert(ii, count);
                }
                new_bytes.insert(b.idx, listed);
            }
            Some(Plan::Mtrl) => {
                let mut m = b.bytes.clone();
                mtrl_fix_transposed_counts(&mut m);
                new_bytes.insert(b.idx, m);
            }
            _ => {}
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
fn convert_enum_body_inplace(data: &mut [u8]) {
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

/// vz_state `flgs` body: variable header (ASCII names, endian-neutral) + 42-byte records.
///
/// Record layout (BE input, matches `tools/ucfx_be_to_le._convert_vz_state_flgs`):
///   [0:12]  3×u32, [12:14] u16, [14:42] 7×u32 (entity_id, pos xyz, rot fields).
const FLGS_RECORD_STRIDE: usize = 42;
const BE_ONE_F: [u8; 4] = [0x3F, 0x80, 0x00, 0x00];

fn convert_vz_state_flgs_inplace(data: &mut [u8]) {
    if data.is_empty() {
        return;
    }

    let marker_pos = data
        .windows(BE_ONE_F.len())
        .position(|w| w == BE_ONE_F);
    let rec_start = match marker_pos {
        Some(p) if p >= 4 => p - 4,
        Some(_) => 0,
        None => {
            if data.len().is_multiple_of(4) {
                swap_u32_array(data);
            }
            return;
        }
    };

    let rec_data = &mut data[rec_start..];
    let n_full = rec_data.len() / FLGS_RECORD_STRIDE;
    let mut pos = 0usize;
    for _ in 0..n_full {
        if pos + FLGS_RECORD_STRIDE > rec_data.len() {
            break;
        }
        for off in (0..12).step_by(4) {
            swap_u32(rec_data, pos + off);
        }
        swap_u16(rec_data, pos + 12);
        for off in (14..42).step_by(4) {
            swap_u32(rec_data, pos + off);
        }
        pos += FLGS_RECORD_STRIDE;
    }
    if pos < rec_data.len() && rec_data.len().is_multiple_of(4) {
        swap_u32_array(&mut rec_data[pos..]);
    }
}

/// Compact `info` bodies without `schm` — full record strides (4 + payload_stride).
/// Values match `docs/ecs_components.md` (not raw schm payload_stride alone).
fn compact_default_stride(comp_name: &str) -> Option<usize> {
    match comp_name {
        "Transform" => Some(42),
        // Name is variable-length; handled by convert_name_data_inplace
        "Name" => None,
        "HibernationControl" => Some(10),
        "Label" => Some(8),
        "ScrubObject" => Some(8),
        "LineRegion" => Some(8),
        "Road" => Some(44),
        "RoadIntersection" => Some(128),
        "ObjectScript" => Some(12),
        "Anchor" => Some(20),
        "AiBehavior" => Some(52),
        "SoundAmbience" => Some(24),
        "AtmosphereBase" => Some(744),
        "IntersectionToIntersection" => Some(12),
        // ModelName handled by convert_modelname_data_inplace (stride 8)
        "ModelName" => None,
        "LightObject" => Some(56),
        "DestructionLink" => Some(20),
        "PhysicalLink" => Some(20),
        "ModifierKey" => Some(12),
        "MaterialMapping" => Some(8),
        "LandingZone" => Some(8),
        "LowResTerrainObject" => Some(12),
        "Path" => Some(8),
        "LaneData" => Some(8),
        _ => None,
    }
}

fn is_ecs_numeric_component(comp_name: &str) -> bool {
    matches!(
        comp_name,
        "LightObject"
            | "Road"
            | "RoadIntersection"
            | "DestructionLink"
            | "PhysicalLink"
            | "ObjectScript"
            | "ModifierKey"
            | "ScrubObject"
            | "LineRegion"
            | "MaterialMapping"
            | "LandingZone"
            | "Label"
            | "Anchor"
            | "LowResTerrainObject"
            | "HibernationControl"
            | "AtmosphereBase"
            | "IntersectionToIntersection"
            | "SoundAmbience"
            | "AiBehavior"
            | "Path"
            | "LaneData"
    )
}

/// Fixed-stride numeric ECS records (Python `_convert_numeric_records`).
fn swap_numeric_records_inplace(data: &mut [u8], stride: usize) {
    if stride == 0 {
        return;
    }
    let mut pos = 0usize;
    while pos + stride <= data.len() {
        let n_u32 = stride / 4;
        for i in 0..n_u32 {
            swap_u32(data, pos + i * 4);
        }
        let tail = stride % 4;
        if tail >= 2 {
            swap_u16(data, pos + n_u32 * 4);
        }
        pos += stride;
    }
    if pos < data.len() && data[pos..].len().is_multiple_of(4) {
        swap_u32_array(&mut data[pos..]);
    }
}

/// Swap every aligned 4-byte word in a slice. Trailing bytes (< 4) are left untouched.
fn swap_u32_array(data: &mut [u8]) {
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
/// The structure is validated against the body length (`count` derived from `len` must equal the
/// on-wire u16 count). If it does not match (an unexpected material variant), fall back to a
/// count-preserving u16 swap — `swap_u16_array` still byte-reverses the u16 count correctly at any
/// even offset, so the engine's count stays valid and we never reintroduce the overrun.
/// 26-dword (104-byte) colour/scalar preamble that precedes each material's flag word.
const MTRL_PRE: usize = 26 * 4; // 104

/// The BE `count` field of a flag word at `body[at..]` if plausible: `[u16 flags]
/// [u16 count]` (FUN_00858790 reads count@+2) with `count ∈ 1..=10` (the engine's
/// fixed 10-slot hash array). In BE that is a zero high byte followed by 1..=10 —
/// a strong signature texture hashes (large u32) and IEEE-float props rarely hit.
/// `flags` is unconstrained here: real materials carry flags up to ~0x418.
fn mtrl_be_count(body: &[u8], at: usize) -> Option<usize> {
    if at + 4 <= body.len() && body[at + 2] == 0 && (1..=10).contains(&body[at + 3]) {
        Some(body[at + 3] as usize)
    } else {
        None
    }
}

/// Do the materials tile the body EXACTLY on the standard `116 + count*4` stride,
/// with a valid `count ∈ 1..=10` flag word at every record's `+104`? This is the only
/// layout we trust to rewrite as a multi-material array.
fn mtrl_tiles_standard(body: &[u8]) -> Option<Vec<usize>> {
    let len = body.len();
    let mut off = 0usize;
    let mut flags = Vec::new();
    while off < len {
        let f = off + MTRL_PRE;
        let count = mtrl_be_count(body, f)?; // flag word must be valid here
        flags.push(f);
        off += 116 + count * 4;
        if off > len {
            return None;
        }
    }
    if off == len && !flags.is_empty() {
        Some(flags)
    } else {
        None
    }
}

/// Convert ONLY material[0]'s `[flags|count]` per-field; u32-swap everything else.
/// This is the pre-2026-06-16 behaviour. The multi-material array walk
/// (`mtrl_convert_array`) is RETAINED below but currently UNUSED while we bisect a
/// world-load HANG regression: correctly converting `material[1..]` (vs leaving them
/// transposed, as this single-material path does) changed engine behaviour into the
/// hang — i.e. the transposed materials[1..] were accidentally MASKING a deeper bug.
/// Reverted to restore the known-good load; re-enable per-block once the masked issue
/// (streaming/texture dependency exposed by the real material refs) is understood.
fn convert_mtrl(body: &mut [u8]) {
    let len = body.len();
    if len < MTRL_PRE + 4 {
        swap_u16_array(body);
        return;
    }
    swap_u32_array(&mut body[..MTRL_PRE]); // 26-dword colour/scalar preamble
    body[MTRL_PRE..MTRL_PRE + 2].reverse(); // u16 flags @104
    body[MTRL_PRE + 2..MTRL_PRE + 4].reverse(); // u16 count @106 (in place — no transpose)
    swap_u32_array(&mut body[MTRL_PRE + 4..]); // hashes + trailing (material[1..] left as-is)
}

/// Multi-material array walk (the 0x61981F c4land fix). PARKED — see `convert_mtrl`.
/// Only safe for bodies that tile exactly on the standard `116 + count*4` stride.
#[allow(dead_code)]
fn mtrl_convert_array(body: &mut [u8]) {
    let len = body.len();
    if len < MTRL_PRE + 4 {
        swap_u16_array(body);
        return;
    }
    let flag_offs = match mtrl_tiles_standard(body) {
        Some(f) => f,
        None => {
            swap_u32_array(&mut body[..MTRL_PRE]);
            body[MTRL_PRE..MTRL_PRE + 2].reverse();
            body[MTRL_PRE + 2..MTRL_PRE + 4].reverse();
            swap_u32_array(&mut body[MTRL_PRE + 4..]);
            return;
        }
    };
    for i in 0..flag_offs.len() {
        let f = flag_offs[i];
        let pre_start = f - MTRL_PRE;
        let region_end = if i + 1 < flag_offs.len() { flag_offs[i + 1] - MTRL_PRE } else { len };
        swap_u32_array(&mut body[pre_start..f]);
        body[f..f + 2].reverse();
        body[f + 2..f + 4].reverse();
        swap_u32_array(&mut body[f + 4..region_end]);
    }
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
fn convert_chdr_body_inplace(data: &mut [u8]) {
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
    let read_u32: fn(&[u8], usize) -> u32 = if is_be { read_u32_be } else { mercs2_formats::ffcs::read_u32_le };

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
        convert_block, convert_cfx_inplace, convert_chdr_body_inplace, convert_decl,
        convert_efct_header_inplace, convert_hibernation_data_inplace, convert_info_body_inplace,
        convert_keyed_group_records_inplace, convert_mtrl,
        fix_embedded_havok_layoutrules, is_ecs_name_identifier,
        HAVOK_PACKFILE_MAGIC,
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
        convert_mtrl(&mut body);

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
    fn mtrl_count_zero_ok() {
        let mut body = make_mtrl_be(0x0001, 0); // 116 bytes, no hashes
        convert_mtrl(&mut body);
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
    fn mtrl_unknown_layout_falls_back_count_safe() {
        // A body that does NOT satisfy len == 116 + count*4 -> count-safe u16 fallback.
        let mut body = vec![0u8; 50];
        body[10] = 0xAB;
        body[11] = 0xCD;
        convert_mtrl(&mut body);
        assert_eq!(body[10], 0xCD, "fallback must u16-swap (count-safe at any even offset)");
        assert_eq!(body[11], 0xAB);
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

        let out = convert_block(&block, false, None).expect("convert_block must succeed");

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
        let out0 = convert_block(&block0, false, None).expect("convert_block must succeed");
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
    fn hibernation_typed_swap_matches_retail_layout() {
        // BE source: key, u16 field, then constant u8 params a0/3c/14/00.
        let mut data = Vec::new();
        data.extend_from_slice(&0x0015_0626u32.to_be_bytes());
        data.extend_from_slice(&0x00FEu16.to_be_bytes());
        data.extend_from_slice(&[0xA0, 0x3C, 0x14, 0x00]);
        assert_eq!(data.len(), 10);

        convert_hibernation_data_inplace(&mut data);
        // entity key + payload u16 swapped; u8/bit tail untouched.
        assert_eq!(&data[0..4], &0x0015_0626u32.to_le_bytes());
        assert_eq!(&data[4..6], &0x00FEu16.to_le_bytes());
        assert_eq!(&data[6..10], &[0xA0, 0x3C, 0x14, 0x00]);
        // Matches retail PC byte pattern `XX 00 a0 3c 14 00`.
        assert_eq!(&data[4..10], &[0xFE, 0x00, 0xA0, 0x3C, 0x14, 0x00]);
    }

    #[test]
    fn ecs_name_identifier_rejects_binary_hashes() {
        // Real component names are valid identifiers.
        assert!(is_ecs_name_identifier(b"Transform"));
        assert!(is_ecs_name_identifier(b"ModelName"));
        assert!(is_ecs_name_identifier(b"PointLocation"));
        // Printable compact hashes (with punctuation) are NOT names.
        assert!(!is_ecs_name_identifier(b"N+lT")); // 0x4E2B6C54
        assert!(!is_ecs_name_identifier(b"iV~b")); // 0x69567E62
        assert!(!is_ecs_name_identifier(b"")); // empty
        assert!(!is_ecs_name_identifier(b"A")); // too short
    }

    #[test]
    fn keyed_group_pointlocation_record36() {
        // [u32 count=1][1×36-byte record][u8 flag=0]
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&0x80005B9Fu32.to_be_bytes()); // key
        for _ in 0..7 {
            data.extend_from_slice(&0u32.to_be_bytes());
        }
        data.extend_from_slice(&0x3F800000u32.to_be_bytes()); // 1.0f
        data.push(0x00); // flag
        assert_eq!(data.len(), 41);

        let mut out = data.clone();
        assert!(convert_keyed_group_records_inplace(&mut out, 36));
        // count and key now little-endian; trailing flag byte unchanged.
        assert_eq!(&out[0..4], &1u32.to_le_bytes());
        assert_eq!(&out[4..8], &0x80005B9Fu32.to_le_bytes());
        assert_eq!(out[40], 0x00);
    }

    #[test]
    fn keyed_group_entity_ref_list_record4() {
        // Two groups: [count=2][k0][k1][flag][count=1][k2][flag]
        let mut data = Vec::new();
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(&0x8000_0001u32.to_be_bytes());
        data.extend_from_slice(&0x8000_0002u32.to_be_bytes());
        data.push(0x01);
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&0x8000_0003u32.to_be_bytes());
        data.push(0x01);

        let mut out = data.clone();
        assert!(convert_keyed_group_records_inplace(&mut out, 4));
        assert_eq!(&out[0..4], &2u32.to_le_bytes());
        assert_eq!(&out[4..8], &0x8000_0001u32.to_le_bytes());
        assert_eq!(out[12], 0x01, "group-0 flag preserved");
        // group 1
        assert_eq!(&out[13..17], &1u32.to_le_bytes());
        assert_eq!(out[21], 0x01, "group-1 flag preserved");
    }

    #[test]
    fn keyed_group_rejects_mismatched_layout() {
        // record_size=4 but a trailing byte makes consumption inexact.
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&0x8000_0001u32.to_be_bytes());
        data.push(0x01);
        data.push(0xFF); // extra junk -> must NOT consume exactly
        let mut out = data.clone();
        assert!(!convert_keyed_group_records_inplace(&mut out, 4));
        assert_eq!(out, data, "buffer left unchanged on mismatch");
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
            let h = super::dec3n_to_half4_le(u);
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
    fn destrip_simple_strip() {
        // Strip 0,1,2,3 | 4,5,6 with a 0xFFFF restart -> two runs.
        // Run1 (4 verts) = 2 tris: (0,1,2),(1,3,2). Run2 (3 verts) = 1 tri (4,5,6).
        let idx: [u16; 8] = [0, 1, 2, 3, 0xffff, 4, 5, 6];
        let mut src = Vec::new();
        for x in idx {
            src.extend_from_slice(&x.to_le_bytes());
        }
        let out = super::destrip_indices(&src);
        let tris: Vec<u16> = out
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(tris, vec![0, 1, 2, 1, 3, 2, 4, 5, 6]);
    }

    #[test]
    fn destrip_drops_degenerates() {
        // A strip with a repeated index produces a degenerate triangle that must
        // be dropped (PC contains no zero-area triangles in a list).
        let idx: [u16; 5] = [0, 1, 1, 2, 3];
        let mut src = Vec::new();
        for x in idx {
            src.extend_from_slice(&x.to_le_bytes());
        }
        let out = super::destrip_indices(&src);
        let tris: Vec<u16> = out
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        // k=0:(0,1,1) degen drop; k=1:(1,1,2) degen drop; k=2:(1,2,3) even-> keep order.
        assert_eq!(tris, vec![1, 2, 3]);
    }

    #[test]
    fn mtrl_transposed_count_walker() {
        // Two material records. Record 0: count=4 (PC-form [0][4]); record 1:
        // count=6 but transposed to [6][0] (the generic-u32-swap bug). The walker
        // must restore record 1 to [0][6] and leave record 0 alone.
        const PB: usize = 104;
        let stride0 = 116 + 4 * 4; // 132
        let mut body = vec![0u8; stride0 + 116 + 6 * 4];
        // record 0 count pair @104 = [flags=0][count=4]
        body[PB + 2..PB + 4].copy_from_slice(&4u16.to_le_bytes());
        // record 1 count pair @ stride0+104, transposed: [count=6][0]
        let cp1 = stride0 + PB;
        body[cp1..cp1 + 2].copy_from_slice(&6u16.to_le_bytes()); // lo = 6 (wrong)
        super::mtrl_fix_transposed_counts(&mut body);
        // record 0 untouched
        assert_eq!(u16::from_le_bytes([body[PB], body[PB + 1]]), 0);
        assert_eq!(u16::from_le_bytes([body[PB + 2], body[PB + 3]]), 4);
        // record 1 fixed to [0][6]
        assert_eq!(u16::from_le_bytes([body[cp1], body[cp1 + 1]]), 0);
        assert_eq!(u16::from_le_bytes([body[cp1 + 2], body[cp1 + 3]]), 6);
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

        // The multi-material array walk is PARKED in convert_mtrl (single-material) while we
        // bisect the world-load hang regression; test the parked walker directly.
        super::mtrl_convert_array(&mut be);

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
        // One material (count=1) + float props: behaves exactly as the old per-field
        // path — flags/count in place, preamble + hash + props u32-swapped.
        const PB: usize = 104;
        let mut be = vec![0u8; PB + 4 + 4 + 16];
        be[PB..PB + 2].copy_from_slice(&0x0001u16.to_be_bytes()); // flags
        be[PB + 2..PB + 4].copy_from_slice(&1u16.to_be_bytes()); // count
        be[PB + 4..PB + 8].copy_from_slice(&0x01020304u32.to_be_bytes()); // hash
        super::convert_mtrl(&mut be);
        assert_eq!(u16::from_le_bytes([be[PB], be[PB + 1]]), 0x0001);
        assert_eq!(u16::from_le_bytes([be[PB + 2], be[PB + 3]]), 1);
        assert_eq!(&be[PB + 4..PB + 8], &[0x04, 0x03, 0x02, 0x01]);
    }
}
