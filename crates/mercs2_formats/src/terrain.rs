//! Mercenaries 2 low-resolution world terrain loader.
//!
//! Ports the parsing/mapping logic of `tools/terrain_extractor.py` +
//! `tools/ucfx_mesh_codec.py` (the `_read_low_res_terrain_toc`,
//! `_read_lrterrain_object_records`, `_parse_prmg_body` / `decode_submesh`
//! flat-STRM path, and the world-placement formula). See
//! `docs/format_reference.md` §13 and `docs/placement_data_format.md` §2.9.
//!
//! Coordinates stay ENTIRELY in native game space (left-handed, +Y up). The
//! Python reference contains UE/glTF coordinate flips — those are NOT ported.
//!
//! Inputs:
//!   * `low_res_block`      — decompressed `low_res_terrain_P000_Q3` block:
//!                            401-entry 16 B TOC + back-to-back UCFX containers.
//!   * `layers_static_block`— decompressed `layers_static_P000_Q3` block:
//!                            173 UCFX sub-blocks; sub-block 13 holds the
//!                            `LowResTerrainObject` COMP (tile -> mesh_hash).

use crate::texture::TextureData;

/// 20x20 grid, 400 m tiles, origin at -3800 m (tile centers -3800..3800).
const GRID: usize = 20;
const TILE_SPAN_M: f32 = 400.0;
const ORIGIN_M: f32 = -3800.0;

/// The `LowResTerrainObject` COMP lives in `layers_static` sub-block 13
/// (verified retail Mercenaries 2 PC build).
const LRTERRAIN_SUB_BLOCK_INDEX: usize = 13;

const CHUNK_HDR: usize = 20;
const CONTAINER_SENTINEL: u32 = 0xFFFF_FFFF;

/// A merged, world-space terrain mesh in native game coordinates.
pub struct TerrainMesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// Number of grid cells that got a placed tile (expect 400).
    pub tiles_placed: usize,
    /// Number of tile UCFX containers decoded (expect 400).
    pub tiles_decoded: usize,
    /// TOC entry count read from `low_res_block[0]` (expect 401).
    pub toc_entry_count: u32,
    /// Shared `vz_lrterrain` DXT1 atlas, when the texture entry parsed.
    pub texture: Option<TextureData>,
}

fn read_u16_le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn read_u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
/// Decode a little-endian IEEE-754 half-float (matches `model_cubeize::read_f16_le`).
fn read_f16_le(b: &[u8], o: usize) -> f32 {
    let h = u16::from_le_bytes([b[o], b[o + 1]]);
    let sign = (h >> 15) & 1;
    let exp = (h >> 10) & 0x1f;
    let frac = (h & 0x3ff) as u32;
    let val = if exp == 0 {
        (frac as f32 / 1024.0) * 2f32.powi(-14)
    } else if exp == 0x1f {
        if frac == 0 { f32::INFINITY } else { f32::NAN }
    } else {
        (1.0 + frac as f32 / 1024.0) * 2f32.powi(exp as i32 - 15)
    };
    if sign == 1 { -val } else { val }
}

fn find_all(data: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= data.len() {
        if &data[i..i + needle.len()] == needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
//   TOC parse (mirrors _read_low_res_terrain_toc)
// ---------------------------------------------------------------------------

/// Map `TOC.hash1` -> UCFX iteration index (0..N-1) for the low_res_terrain block.
///
/// Entry 0 stores the count in u32[0] but its u32[1] is a real mesh hash1.
/// Each entry `j`'s UCFX container is at `data_base + Σ sizes[1..j]`. Entries
/// whose UCFX has `u3 < 10` (the dummy tile + the texture container) are
/// filtered out (matching `iter_ucfx_containers`), so iteration indices past
/// them are shifted. Returns `(hash1 -> iter_idx, toc_entry_count)`.
fn read_low_res_terrain_toc(data: &[u8]) -> (std::collections::HashMap<u32, usize>, u32) {
    let mut out = std::collections::HashMap::new();
    if data.len() < 16 {
        return (out, 0);
    }
    let n_entries = read_u32_le(data, 0) as usize;
    if n_entries < 2 || n_entries > 100_000 || data.len() < n_entries * 16 {
        return (out, n_entries as u32);
    }
    let toc_area_end = n_entries * 16;
    // First UCFX magic just past the TOC = data_base.
    let data_base = match data[toc_area_end..(toc_area_end + 64).min(data.len())]
        .windows(4)
        .position(|w| w == b"UCFX")
    {
        Some(p) => toc_area_end + p,
        None => return (out, n_entries as u32),
    };

    // Cumulative container offsets: expected_off[0] = data_base, then += size[j].
    let mut expected_off = Vec::with_capacity(n_entries);
    expected_off.push(data_base);
    for j in 1..n_entries {
        let sz = read_u32_le(data, j * 16) as usize;
        expected_off.push(expected_off[j - 1] + sz);
    }

    let mut iter_idx = 0usize;
    for j in 0..n_entries {
        let off = expected_off[j];
        if off + 20 > data.len() || &data[off..off + 4] != b"UCFX" {
            continue;
        }
        let u3 = read_u32_le(data, off + 16);
        if u3 < 10 || u3 > 50_000 {
            continue; // dummy tile (u3=3) and texture container are filtered out
        }
        let h1 = read_u32_le(data, j * 16 + 4);
        out.insert(h1, iter_idx);
        iter_idx += 1;
    }
    (out, n_entries as u32)
}

// ---------------------------------------------------------------------------
//   LowResTerrainObject COMP (mirrors _read_lrterrain_object_records)
// ---------------------------------------------------------------------------

/// Ordered `mesh_hash` list from `LowResTerrainObject` COMP (record i = cell
/// (row=i/20, col=i%20)). Empty if the COMP is not found.
fn read_lrterrain_object_records(layers_static: &[u8]) -> Vec<u32> {
    let ucfx_positions = find_all(layers_static, b"UCFX");
    if LRTERRAIN_SUB_BLOCK_INDEX >= ucfx_positions.len() {
        return Vec::new();
    }
    let ucfx_pos = ucfx_positions[LRTERRAIN_SUB_BLOCK_INDEX];
    if ucfx_pos + 8 > layers_static.len() {
        return Vec::new();
    }
    let ucfx_size = read_u32_le(layers_static, ucfx_pos + 4) as usize;
    let block_end = if LRTERRAIN_SUB_BLOCK_INDEX + 1 < ucfx_positions.len() {
        ucfx_positions[LRTERRAIN_SUB_BLOCK_INDEX + 1]
    } else {
        layers_static.len()
    };

    // CHDR chunk within this sub-block.
    let search_end = (ucfx_pos + ucfx_size + 200).min(layers_static.len());
    let chdr_pos = match layers_static[ucfx_pos..search_end]
        .windows(4)
        .position(|w| w == b"CHDR")
    {
        Some(p) => ucfx_pos + p,
        None => return Vec::new(),
    };
    if chdr_pos + 20 > layers_static.len() {
        return Vec::new();
    }
    let chdr_entries = read_u32_le(layers_static, chdr_pos + 12) as usize;

    // Walk the CHDR chunk table: COMP/enum/flgt/flgs rows, each with children.
    let mut pos = chdr_pos + 20;
    // Collect (tag, children:(ctag,coff,csz)).
    let mut chunks: Vec<(Vec<u8>, Vec<([u8; 4], usize, usize)>)> = Vec::new();
    for _ in 0..chdr_entries {
        if pos + CHUNK_HDR > block_end {
            break;
        }
        let tag = &layers_static[pos..pos + 4];
        if tag != b"COMP" && tag != b"enum" && tag != b"flgt" && tag != b"flgs" {
            break;
        }
        let num_children = read_u32_le(layers_static, pos + 16) as usize;
        let mut children = Vec::with_capacity(num_children);
        let mut child_pos = pos + CHUNK_HDR;
        for _ in 0..num_children {
            if child_pos + CHUNK_HDR > block_end {
                break;
            }
            let mut ctag = [0u8; 4];
            ctag.copy_from_slice(&layers_static[child_pos..child_pos + 4]);
            let coff = read_u32_le(layers_static, child_pos + 4) as usize;
            let csz = read_u32_le(layers_static, child_pos + 8) as usize;
            children.push((ctag, coff, csz));
            child_pos += CHUNK_HDR;
        }
        chunks.push((tag.to_vec(), children));
        pos = child_pos;
    }
    let data_area_start = pos;

    for (tag, children) in &chunks {
        if tag != b"COMP" {
            continue;
        }
        let mut info_name: Option<String> = None;
        let mut data_child: Option<(usize, usize)> = None;
        for (ctag, coff, csz) in children {
            let abs_off = data_area_start + coff;
            if ctag == b"info" && abs_off + csz <= layers_static.len() {
                let raw = &layers_static[abs_off..abs_off + csz];
                if let Some(nul) = raw.iter().position(|&b| b == 0) {
                    if nul > 0 {
                        info_name =
                            Some(String::from_utf8_lossy(&raw[..nul]).into_owned());
                    }
                }
            } else if ctag == b"data" {
                data_child = Some((abs_off, *csz));
            }
        }
        if info_name.as_deref() == Some("LowResTerrainObject") {
            if let Some((off, size)) = data_child {
                let n_records = size / 12;
                let mut out = Vec::with_capacity(n_records);
                for i in 0..n_records {
                    let rec_off = off + i * 12;
                    if rec_off + 12 > layers_static.len() {
                        break;
                    }
                    // record = (entity_key u32, mesh_hash u32, scene_object_id u32)
                    let mesh_hash = read_u32_le(layers_static, rec_off + 4);
                    out.push(mesh_hash);
                }
                return out;
            }
        }
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
//   UCFX container iteration + GEOM/PRMG flat-row walk
// ---------------------------------------------------------------------------

struct Container {
    data_base: usize,
    /// Flat chunk rows: (tag, [u0,u1,u2,u3]).
    chunks: Vec<([u8; 4], [u32; 4])>,
}

/// Iterate UCFX containers in `data` (u3 in [10, 50000]) — mirrors `iter_ucfx_containers`.
fn iter_ucfx_containers(data: &[u8]) -> Vec<Container> {
    let mut out = Vec::new();
    for ucfx_off in find_all(data, b"UCFX") {
        if ucfx_off + 20 > data.len() {
            continue;
        }
        let u0 = read_u32_le(data, ucfx_off + 4);
        let u3 = read_u32_le(data, ucfx_off + 16);
        if u3 < 10 || u3 > 50_000 {
            continue;
        }
        if ucfx_off + 20 + (u3 as usize) * CHUNK_HDR > data.len() {
            continue;
        }
        let data_base = ucfx_off + u0 as usize;
        if data_base >= data.len() {
            continue;
        }
        let mut chunks = Vec::with_capacity(u3 as usize);
        for i in 0..u3 as usize {
            let cpos = ucfx_off + 20 + i * CHUNK_HDR;
            let mut tag = [0u8; 4];
            tag.copy_from_slice(&data[cpos..cpos + 4]);
            let cu = [
                read_u32_le(data, cpos + 4),
                read_u32_le(data, cpos + 8),
                read_u32_le(data, cpos + 12),
                read_u32_le(data, cpos + 16),
            ];
            chunks.push((tag, cu));
        }
        out.push(Container { data_base, chunks });
    }
    out
}

/// Each GEOM container owns the next `u3` flat chunk rows — mirrors
/// `_iter_geom_child_row_slices`.
fn geom_child_row_slices(chunks: &[([u8; 4], [u32; 4])]) -> Vec<Vec<([u8; 4], [u32; 4])>> {
    let mut out = Vec::new();
    let mut ii = 0;
    while ii < chunks.len() {
        let (tag, u) = &chunks[ii];
        if tag == b"GEOM" && u[0] == CONTAINER_SENTINEL {
            let n = u[3] as usize;
            if n > 0 && ii + 1 + n <= chunks.len() {
                out.push(chunks[ii + 1..ii + 1 + n].to_vec());
            }
            ii += 1 + n;
        } else {
            ii += 1;
        }
    }
    out
}

struct PrmgBody {
    vb_off: usize,
    vb_len: usize,
    ib_off: usize,
    ib_len: usize,
}

/// Extract VB/IB offsets from a GEOM child slice — mirrors `_parse_prmg_body`
/// (the flat STRM/IBUF path used by terrain tiles; no PRMG row).
fn parse_prmg_body(body: &[([u8; 4], [u32; 4])]) -> Option<PrmgBody> {
    let (mut vb_off, mut vb_len, mut ib_off, mut ib_len) = (0usize, 0usize, 0usize, 0usize);
    let (mut got_vb, mut got_ib) = (false, false);

    let mut j = 0;
    while j < body.len() {
        let (bt, bu) = &body[j];
        if bt == b"STRM" && bu[0] == CONTAINER_SENTINEL {
            let nch = bu[3] as usize;
            if nch > 0 && j + 1 + nch <= body.len() {
                let strm_rows = &body[j + 1..j + 1 + nch];
                let mut decl_seen = false;
                for (st, su) in strm_rows {
                    let lt = st.to_ascii_lowercase();
                    if &lt == b"decl" {
                        decl_seen = true;
                    } else if &lt == b"data" && decl_seen {
                        vb_off = su[0] as usize;
                        vb_len = su[1] as usize;
                        got_vb = true;
                        break;
                    }
                }
            }
            j += 1 + nch;
        } else if bt == b"IBUF" && bu[0] == CONTAINER_SENTINEL {
            let nch = bu[3] as usize;
            if nch > 0 && j + 1 + nch <= body.len() {
                let ib_rows = &body[j + 1..j + 1 + nch];
                for (it, iu) in ib_rows {
                    if &it.to_ascii_lowercase() == b"data" {
                        ib_off = iu[0] as usize;
                        ib_len = iu[1] as usize;
                        got_ib = true;
                        break;
                    }
                }
            }
            j += 1 + nch;
        } else {
            j += 1;
        }
    }

    if got_vb && got_ib && vb_len > 0 && ib_len >= 6 {
        Some(PrmgBody { vb_off, vb_len, ib_off, ib_len })
    } else {
        None
    }
}

/// D3D9 triangle-strip -> triangle list, skipping sentinel/degenerate triplets
/// (mirrors `u16_strip_to_tris`).
fn strip_to_tris(indices: &[u16]) -> Vec<[u32; 3]> {
    let mut tris = Vec::new();
    if indices.len() < 3 {
        return tris;
    }
    for i in 0..indices.len() - 2 {
        let (a, b, c) = (indices[i], indices[i + 1], indices[i + 2]);
        if a == 65535 || b == 65535 || c == 65535 {
            continue;
        }
        if a == b || b == c || a == c {
            continue;
        }
        if i % 2 == 0 {
            tris.push([a as u32, b as u32, c as u32]);
        } else {
            tris.push([a as u32, c as u32, b as u32]);
        }
    }
    tris
}

/// Decode one terrain tile: flat f16 vertex buffer + triangle-strip IBUF.
/// Returns (local positions, uvs, triangles). Mirrors `decode_submesh` for the
/// flat-STRM terrain case: stride = vb_len / n_verts (typically 16 B =
/// pos f16x3 + uv f16x2 + 2 B pad), positions are f16 vec3 at offset 0, UVs
/// f16x2 at offset 8. NO coordinate flips.
fn decode_tile(
    data: &[u8],
    data_base: usize,
    sub: &PrmgBody,
) -> Option<(Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[u32; 3]>)> {
    let vb_abs = data_base + sub.vb_off;
    let ib_abs = data_base + sub.ib_off;
    if vb_abs + sub.vb_len > data.len() || ib_abs + sub.ib_len > data.len() {
        return None;
    }
    let n_idx = sub.ib_len / 2;
    let mut indices = Vec::with_capacity(n_idx);
    for k in 0..n_idx {
        indices.push(read_u16_le(data, ib_abs + k * 2));
    }
    if indices.is_empty() {
        return None;
    }
    let max_idx = indices.iter().copied().filter(|&x| x != 65535).max().unwrap_or(0) as usize;
    let n_verts = max_idx + 1;
    if n_verts == 0 || sub.vb_len % n_verts != 0 {
        return None;
    }
    let stride = sub.vb_len / n_verts;
    if stride < 8 {
        return None;
    }

    let mut positions = Vec::with_capacity(n_verts);
    let mut uvs = Vec::with_capacity(n_verts);
    for v in 0..n_verts {
        let o = vb_abs + v * stride;
        let x = read_f16_le(data, o);
        let y = read_f16_le(data, o + 2);
        let z = read_f16_le(data, o + 4);
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return None;
        }
        positions.push([x, y, z]);
        // UV at offset 8 (pos f16x3 @0, w f16 @6, uv f16x2 @8).
        let (u, vv) = if stride >= 12 {
            (read_f16_le(data, o + 8), read_f16_le(data, o + 10))
        } else {
            (0.0, 0.0)
        };
        uvs.push([u, vv]);
    }

    let tris = strip_to_tris(&indices);
    if tris.is_empty() {
        return None;
    }
    let need = tris.iter().flat_map(|t| t.iter()).copied().max().unwrap_or(0) as usize + 1;
    if need > positions.len() {
        return None;
    }
    Some((positions, uvs, tris))
}

// ---------------------------------------------------------------------------
//   Terrain texture (vz_lrterrain DXT1 atlas)
// ---------------------------------------------------------------------------

/// Find and decode the shared `vz_lrterrain` DXT1 atlas. The texture container
/// is the TOC entry with `u3 < 10` carrying INFO + NAME + DXT1 chunks
/// (docs §13.4.2). Returns `None` if not found / not parseable.
fn extract_terrain_texture(data: &[u8]) -> Option<TextureData> {
    use crate::texsize::{dxt_format, dxt_mip_count};

    for ucfx_off in find_all(data, b"UCFX") {
        if ucfx_off + 20 > data.len() {
            continue;
        }
        let u0 = read_u32_le(data, ucfx_off + 4) as usize;
        let u3 = read_u32_le(data, ucfx_off + 16) as usize;
        // Texture container: small chunk count (u3=3: INFO/NAME/DXT1), not a mesh tile.
        if u3 == 0 || u3 >= 10 {
            continue;
        }
        if ucfx_off + 20 + u3 * CHUNK_HDR > data.len() {
            continue;
        }
        let data_base = ucfx_off + u0;
        let mut info: Option<(usize, usize)> = None;
        let mut body: Option<(usize, usize)> = None;
        for i in 0..u3 {
            let cpos = ucfx_off + 20 + i * CHUNK_HDR;
            let tag = &data[cpos..cpos + 4];
            let coff = read_u32_le(data, cpos + 4) as usize;
            let csz = read_u32_le(data, cpos + 8) as usize;
            if coff == CONTAINER_SENTINEL as usize {
                continue;
            }
            let abs = data_base + coff;
            if abs + csz > data.len() {
                continue;
            }
            match tag {
                b"INFO" if info.is_none() => info = Some((abs, csz)),
                // Pixel data chunk: DXT1 (terrain) or a generic BODY leaf.
                b"DXT1" | b"BODY" if body.is_none() => body = Some((abs, csz)),
                _ => {}
            }
        }
        let (Some((is, isz)), Some((bs, bsz))) = (info, body) else { continue };
        if isz < 18 {
            continue;
        }
        let info_b = &data[is..is + isz];
        let width = read_u16_le(info_b, 0) as u32;
        let height = read_u16_le(info_b, 2) as u32;
        let declared_mips = read_u16_le(info_b, 6) as u32;
        let fourcc = &info_b[14..18];
        let format = match fourcc {
            b"DXT1" => crate::texture::TexFormat::Bc1,
            b"DXT5" => crate::texture::TexFormat::Bc3,
            _ => continue,
        };
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            continue;
        }
        let all_mips = data[bs..bs + bsz].to_vec();
        let (block_px, texel_pitch, _) = dxt_format(format.fourcc())?;
        let wb = (width as usize).div_ceil(block_px).max(1);
        let hb = (height as usize).div_ceil(block_px).max(1);
        let mip0_len = (wb * hb * texel_pitch).min(all_mips.len());
        let mip0 = all_mips[..mip0_len].to_vec();
        let mip_count = declared_mips.max(1).min(dxt_mip_count(width as usize, height as usize) as u32);
        return Some(TextureData {
            width,
            height,
            format,
            mip0,
            all_mips,
            mip_count,
        });
    }
    None
}

// ---------------------------------------------------------------------------
//   Public entry
// ---------------------------------------------------------------------------

/// World X/Z center for cell `(row, col)` (placement formula, native game space).
fn tile_world_center(row: usize, col: usize) -> (f32, f32) {
    let cx = ORIGIN_M + col as f32 * TILE_SPAN_M;
    let cz = ORIGIN_M + row as f32 * TILE_SPAN_M;
    (cx, cz)
}

/// Load + merge the 20x20 low-resolution terrain grid into one world-space mesh.
///
/// Ports `terrain_extractor.py`: decode every UCFX tile (flat STRM/IBUF), map
/// each grid cell to a tile via `LowResTerrainObject.mesh_hash -> TOC.hash1 ->
/// iter index`, and offset each tile's local vertices to its placement center.
/// All coordinates are native game space (LH, +Y up); no flips are applied.
pub fn load_terrain(
    low_res_block: &[u8],
    layers_static_block: &[u8],
) -> Result<TerrainMesh, String> {
    // 1) Decode every UCFX tile in file (iteration) order.
    let containers = iter_ucfx_containers(low_res_block);
    let mut tiles: Vec<(Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[u32; 3]>)> = Vec::new();
    for c in &containers {
        for rows in geom_child_row_slices(&c.chunks) {
            if let Some(sub) = parse_prmg_body(&rows) {
                if let Some(tile) = decode_tile(low_res_block, c.data_base, &sub) {
                    tiles.push(tile);
                }
                break; // one mesh group per tile
            }
        }
    }
    let tiles_decoded = tiles.len();

    // 2) TOC: hash1 -> iter index.
    let (hash_to_idx, toc_entry_count) = read_low_res_terrain_toc(low_res_block);

    // 3) LowResTerrainObject records: cell (row,col) -> mesh_hash.
    let records = read_lrterrain_object_records(layers_static_block);

    let texture = extract_terrain_texture(low_res_block);

    if tiles_decoded != GRID * GRID {
        return Err(format!(
            "decoded {tiles_decoded} tiles, expected {} (toc_entry_count={toc_entry_count}, \
             containers={}, records={})",
            GRID * GRID,
            containers.len(),
            records.len(),
        ));
    }
    if records.len() != GRID * GRID {
        return Err(format!(
            "LowResTerrainObject records = {}, expected {} (toc_entry_count={toc_entry_count}, \
             tiles_decoded={tiles_decoded})",
            records.len(),
            GRID * GRID,
        ));
    }
    if hash_to_idx.is_empty() {
        return Err("low_res_terrain TOC produced no hash1 -> index map".into());
    }

    // 4) Grid assignment: cell i -> mesh_hash -> iter index. Unmatched cells
    //    fall back to a unique unused iter index (mirrors the Python fallback).
    let mut grid_idx: Vec<Option<usize>> = vec![None; GRID * GRID];
    let mut used = std::collections::HashSet::new();
    for (i, &mesh_hash) in records.iter().enumerate() {
        if let Some(&idx) = hash_to_idx.get(&mesh_hash) {
            if idx < tiles_decoded {
                grid_idx[i] = Some(idx);
                used.insert(idx);
            }
        }
    }
    let unmatched: Vec<usize> = (0..GRID * GRID).filter(|&i| grid_idx[i].is_none()).collect();
    if !unmatched.is_empty() {
        let spare: Vec<usize> = (0..tiles_decoded).filter(|i| !used.contains(i)).collect();
        if unmatched.len() == spare.len() {
            for (&cell, &idx) in unmatched.iter().zip(spare.iter()) {
                grid_idx[cell] = Some(idx);
                used.insert(idx);
            }
        }
    }

    // 5) Merge: offset each cell's tile to its world placement center.
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut tiles_placed = 0usize;
    for row in 0..GRID {
        for col in 0..GRID {
            let cell = row * GRID + col;
            let Some(idx) = grid_idx[cell] else { continue };
            let (verts, tuvs, tris) = &tiles[idx];
            let (cx, cz) = tile_world_center(row, col);
            let base = positions.len() as u32;
            for (p, uv) in verts.iter().zip(tuvs.iter()) {
                positions.push([p[0] + cx, p[1], p[2] + cz]);
                uvs.push(*uv);
            }
            for t in tris {
                indices.push(base + t[0]);
                indices.push(base + t[1]);
                indices.push(base + t[2]);
            }
            tiles_placed += 1;
        }
    }

    Ok(TerrainMesh {
        positions,
        uvs,
        indices,
        tiles_placed,
        tiles_decoded,
        toc_entry_count,
        texture,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_matches_d3d9_winding() {
        // Even i -> (a,b,c); odd i -> (a,c,b); sentinel/degenerate skipped.
        let idx = [0u16, 1, 2, 3, 65535, 4, 5, 6];
        let tris = strip_to_tris(&idx);
        // (0,1,2) even, (1,3->2 has 65535 next? no) let's assert the first two.
        assert_eq!(tris[0], [0, 1, 2]);
        assert_eq!(tris[1], [1, 3, 2]); // odd index -> swapped
        // Triplets touching 65535 are dropped.
        assert!(tris.iter().all(|t| !t.contains(&65535)));
    }

    #[test]
    fn tile_center_formula() {
        assert_eq!(tile_world_center(0, 0), (-3800.0, -3800.0));
        assert_eq!(tile_world_center(19, 19), (3800.0, 3800.0));
        assert_eq!(tile_world_center(1, 2), (-3000.0, -3400.0));
    }

    #[test]
    fn f16_decode_known_values() {
        // 0x3C00 = 1.0, 0x4000 = 2.0, 0xC000 = -2.0, 0x0000 = 0.0.
        let b = [0x00u8, 0x3C, 0x00, 0x40, 0x00, 0xC0, 0x00, 0x00];
        assert_eq!(read_f16_le(&b, 0), 1.0);
        assert_eq!(read_f16_le(&b, 2), 2.0);
        assert_eq!(read_f16_le(&b, 4), -2.0);
        assert_eq!(read_f16_le(&b, 6), 0.0);
    }
}
