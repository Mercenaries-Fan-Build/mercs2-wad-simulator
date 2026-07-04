//! Mercenaries 2 world-placement loader (`layers_static`, WAD block index 29).
//!
//! Ports `tools/placement_extractor.py` (the layers_static path only). See
//! `docs/placement_data_format.md` §2. Iterates all 173 UCFX sub-blocks, walks
//! each CHDR/COMP chunk table (generalising `terrain::read_lrterrain_object_records`),
//! and pulls every `Transform` + `Name` COMP, matching them by u32 entity key.
//!
//! Coordinates stay ENTIRELY in native game space (left-handed, +Y up). The
//! Python reference contains UE/glTF coordinate flips — those are NOT ported.
//!
//! Input: `layers_static_block` — decompressed `layers_static_P000_Q3` block:
//! 173 UCFX sub-blocks; each has a CHDR chunk table of COMP components whose
//! `info`/`data` child offsets are RELATIVE to `data_area_start`.

const CHUNK_HDR: usize = 20;

/// One world placement: entity key, optional name, world position + rotation
/// quaternion, and the sub-block it came from.
#[derive(Debug, Clone)]
pub struct Placement {
    pub key: u32,
    pub name: Option<String>,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub sub_block: u16,
}

fn read_u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn read_f32_le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
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

/// One COMP component within a sub-block: its `info` type name and the absolute
/// byte span of its `data` child.
struct CompChild {
    info_name: Option<String>,
    data: Option<(usize, usize)>,
}

/// Public COMP inventory record for a single COMP in a sub-block: its `info`
/// type name, the payload stride declared by its `schm` child (bytes after the
/// entity key; total record stride = 4 + payload_stride), and the absolute byte
/// span of its `data` child inside the whole block buffer.
#[derive(Debug, Clone)]
pub struct CompInfo {
    pub sub_block: u16,
    pub info_name: Option<String>,
    pub payload_stride: Option<u32>,
    pub data_off: Option<usize>,
    pub data_size: Option<usize>,
}

/// Enumerate EVERY COMP across every UCFX sub-block of a decompressed block (both
/// `layers_static` and `vz_state`-style overlay blocks use the same UCFX/CHDR/COMP
/// layout with data-relative child offsets). Reports each COMP's info type-name,
/// `schm` payload stride, and data span. Used by the `--comp-probe`.
pub fn comp_inventory(block: &[u8]) -> Vec<CompInfo> {
    let ucfx_positions = find_all(block, b"UCFX");
    let mut out = Vec::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            block.len()
        };
        for c in walk_sub_block_comps_full(block, ucfx_pos, block_end) {
            out.push(CompInfo {
                sub_block: si as u16,
                info_name: c.info_name,
                payload_stride: c.payload_stride,
                data_off: c.data.map(|(o, _)| o),
                data_size: c.data.map(|(_, s)| s),
            });
        }
    }
    out
}

/// Like `walk_sub_block_comps` but also captures each COMP's `schm` payload
/// stride (`schm[4:8]` per docs §2.6).
struct CompChildFull {
    info_name: Option<String>,
    payload_stride: Option<u32>,
    data: Option<(usize, usize)>,
}

fn walk_sub_block_comps_full(
    data: &[u8],
    ucfx_pos: usize,
    block_end: usize,
) -> Vec<CompChildFull> {
    let mut out = Vec::new();
    if ucfx_pos + 8 > data.len() {
        return out;
    }
    let ucfx_size = read_u32_le(data, ucfx_pos + 4) as usize;
    let search_end = (ucfx_pos + ucfx_size + 200).min(data.len());
    let chdr_pos = match data[ucfx_pos..search_end]
        .windows(4)
        .position(|w| w == b"CHDR")
    {
        Some(p) => ucfx_pos + p,
        None => return out,
    };
    if chdr_pos + 20 > data.len() {
        return out;
    }
    let chdr_entries = read_u32_le(data, chdr_pos + 12) as usize;

    let mut pos = chdr_pos + 20;
    let mut chunks: Vec<(Vec<u8>, Vec<([u8; 4], usize, usize)>)> = Vec::new();
    for _ in 0..chdr_entries {
        if pos + CHUNK_HDR > block_end {
            break;
        }
        let tag = &data[pos..pos + 4];
        if tag != b"COMP" && tag != b"enum" && tag != b"flgt" && tag != b"flgs" {
            break;
        }
        let num_children = read_u32_le(data, pos + 16) as usize;
        let mut children = Vec::with_capacity(num_children);
        let mut child_pos = pos + CHUNK_HDR;
        for _ in 0..num_children {
            if child_pos + CHUNK_HDR > block_end {
                break;
            }
            let mut ctag = [0u8; 4];
            ctag.copy_from_slice(&data[child_pos..child_pos + 4]);
            let coff = read_u32_le(data, child_pos + 4) as usize;
            let csz = read_u32_le(data, child_pos + 8) as usize;
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
        let mut payload_stride: Option<u32> = None;
        let mut data_child: Option<(usize, usize)> = None;
        for (ctag, coff, csz) in children {
            let abs_off = data_area_start + coff;
            if ctag == b"info" && abs_off + csz <= data.len() {
                let raw = &data[abs_off..abs_off + csz];
                if let Some(nul) = raw.iter().position(|&b| b == 0) {
                    if nul > 0 {
                        info_name = Some(String::from_utf8_lossy(&raw[..nul]).into_owned());
                    }
                }
            } else if ctag == b"schm" && abs_off + 8 <= data.len() {
                payload_stride = Some(read_u32_le(data, abs_off + 4));
            } else if ctag == b"data" {
                data_child = Some((abs_off, *csz));
            }
        }
        out.push(CompChildFull { info_name, payload_stride, data: data_child });
    }
    out
}

/// Walk one UCFX sub-block's CHDR chunk table and return every COMP's info-name
/// + data span. Generalises `terrain::read_lrterrain_object_records`: instead of
/// stopping at `LowResTerrainObject`, it collects ALL COMPs. COMP child offsets
/// are relative to `data_area_start` (the end of the chunk descriptor table).
fn walk_sub_block_comps(
    data: &[u8],
    ucfx_pos: usize,
    block_end: usize,
) -> Vec<CompChild> {
    let mut out = Vec::new();
    if ucfx_pos + 8 > data.len() {
        return out;
    }
    let ucfx_size = read_u32_le(data, ucfx_pos + 4) as usize;

    // CHDR chunk within this sub-block.
    let search_end = (ucfx_pos + ucfx_size + 200).min(data.len());
    let chdr_pos = match data[ucfx_pos..search_end]
        .windows(4)
        .position(|w| w == b"CHDR")
    {
        Some(p) => ucfx_pos + p,
        None => return out,
    };
    if chdr_pos + 20 > data.len() {
        return out;
    }
    let chdr_entries = read_u32_le(data, chdr_pos + 12) as usize;

    // Walk the CHDR chunk table: COMP/enum/flgt/flgs rows, each with children.
    let mut pos = chdr_pos + 20;
    let mut chunks: Vec<(Vec<u8>, Vec<([u8; 4], usize, usize)>)> = Vec::new();
    for _ in 0..chdr_entries {
        if pos + CHUNK_HDR > block_end {
            break;
        }
        let tag = &data[pos..pos + 4];
        if tag != b"COMP" && tag != b"enum" && tag != b"flgt" && tag != b"flgs" {
            break;
        }
        let num_children = read_u32_le(data, pos + 16) as usize;
        let mut children = Vec::with_capacity(num_children);
        let mut child_pos = pos + CHUNK_HDR;
        for _ in 0..num_children {
            if child_pos + CHUNK_HDR > block_end {
                break;
            }
            let mut ctag = [0u8; 4];
            ctag.copy_from_slice(&data[child_pos..child_pos + 4]);
            let coff = read_u32_le(data, child_pos + 4) as usize;
            let csz = read_u32_le(data, child_pos + 8) as usize;
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
            if ctag == b"info" && abs_off + csz <= data.len() {
                let raw = &data[abs_off..abs_off + csz];
                if let Some(nul) = raw.iter().position(|&b| b == 0) {
                    if nul > 0 {
                        info_name = Some(String::from_utf8_lossy(&raw[..nul]).into_owned());
                    }
                }
            } else if ctag == b"data" {
                data_child = Some((abs_off, *csz));
            }
        }
        out.push(CompChild { info_name, data: data_child });
    }
    out
}

/// Parse a `Transform` COMP `data` blob into (key -> (pos, quat)) records.
/// Each record is 42 bytes: u32 key + XYZ + pad + quat(xyzw) + 6-byte tail.
fn parse_transform_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, [f32; 3], [f32; 4])> {
    const STRIDE: usize = 42;
    let mut out = Vec::new();
    let n = size / STRIDE;
    for i in 0..n {
        let r = off + i * STRIDE;
        if r + STRIDE > data.len() {
            break;
        }
        let key = read_u32_le(data, r);
        let pos = [
            read_f32_le(data, r + 4),
            read_f32_le(data, r + 8),
            read_f32_le(data, r + 12),
        ];
        let quat = [
            read_f32_le(data, r + 20),
            read_f32_le(data, r + 24),
            read_f32_le(data, r + 28),
            read_f32_le(data, r + 32),
        ];
        out.push((key, pos, quat));
    }
    out
}

/// Parse a `Name` COMP `data` blob into (key -> name) records. Each record is
/// `[u32 key][ascii "EntityName 0xKEY\0"]`; the stored string is the full
/// `"Name 0xHEXID"` — we keep the bare name portion (before the trailing hex id).
fn parse_name_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    if off + size > data.len() {
        return out;
    }
    let blob = &data[off..off + size];
    let mut p = 0usize;
    while p + 4 < blob.len() {
        let key = u32::from_le_bytes([blob[p], blob[p + 1], blob[p + 2], blob[p + 3]]);
        p += 4;
        // The string runs to the next NUL.
        let start = p;
        while p < blob.len() && blob[p] != 0 {
            p += 1;
        }
        if p <= start {
            // Empty string; skip the NUL and continue.
            p += 1;
            continue;
        }
        let s = String::from_utf8_lossy(&blob[start..p]).into_owned();
        // Strip a trailing " 0xHEXID" if present (gameplay name is the head).
        let name = match s.rfind(" 0x") {
            Some(i) => s[..i].to_string(),
            None => s,
        };
        out.push((key, name));
        // Consume the NUL terminator (and any run of padding NULs).
        while p < blob.len() && blob[p] == 0 {
            p += 1;
        }
    }
    out
}

/// Load all world placements from a decompressed `layers_static` block.
///
/// Iterates every UCFX sub-block, walks its COMP table, matches `Name` COMP
/// records to `Transform` COMP records by u32 entity key (within the sub-block),
/// and emits one `Placement` per Transform record. No coordinate flips.
pub fn load_placements(layers_static_block: &[u8]) -> Result<Vec<Placement>, String> {
    let ucfx_positions = find_all(layers_static_block, b"UCFX");
    if ucfx_positions.is_empty() {
        return Err("no UCFX sub-blocks found in layers_static".into());
    }

    let mut out: Vec<Placement> = Vec::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            layers_static_block.len()
        };
        let comps = walk_sub_block_comps(layers_static_block, ucfx_pos, block_end);

        // Collect this sub-block's Transform records and Name map.
        let mut transforms: Vec<(u32, [f32; 3], [f32; 4])> = Vec::new();
        let mut names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        for c in &comps {
            let Some((off, size)) = c.data else { continue };
            match c.info_name.as_deref() {
                Some("Transform") => {
                    transforms.extend(parse_transform_records(layers_static_block, off, size));
                }
                Some("Name") => {
                    for (k, n) in parse_name_records(layers_static_block, off, size) {
                        names.entry(k).or_insert(n);
                    }
                }
                _ => {}
            }
        }

        for (key, pos, quat) in transforms {
            out.push(Placement {
                key,
                name: names.get(&key).cloned(),
                pos,
                quat,
                sub_block: si as u16,
            });
        }
    }

    if out.is_empty() {
        return Err("no Transform COMP records found in layers_static".into());
    }
    Ok(out)
}

/// Per-entity streaming/LOD directive parsed from a `HibernationControl` COMP (ECS class
/// `0xe18afd65`). The engine caches an object out (hibernates it) once the player is beyond its
/// hibernation distance, and picks its LOD tier by the LOD distances; this is the per-object
/// control INPUT that drives the streaming manager's load/hibernate/LOD decision (NOT a blanket
/// per-class radius). See `docs/modernization/world_streaming_spec.md` §10 and
/// `docs/mercs2-pdb-analysis/world-streaming.md` §Hibernation.
///
/// On-disk record (verified against retail `layers_static`, 10-byte stride):
/// `{key:u32, dist0:u16, dist1:u8, dist2:u8, dist3:u8, flag:u8}`. `dist0` is the per-object
/// hibernation distance (default 100, here overridden 213–500; the engine warns if > 400 — "may
/// fall through terrain"); `dist1..3` are the LOD-tier distances (class defaults 160 / 60 / 20 in
/// every sampled record, i.e. stored as single bytes because they fit). `flag` is a bool (0 in
/// every sampled record — likely `NoDelOnHibernateIfPreplaced` or a runtime marker).
#[derive(Debug, Clone, Copy)]
pub struct Hibernation {
    /// The 4 LOD/hibernation distances, in schema order (defaults 100 / 160 / 60 / 20).
    /// `dist[0]` is the per-object hibernation (stream-out) distance; `dist[1..4]` are LOD tiers.
    pub dist: [u16; 4],
    /// Trailing bool byte (0 in all sampled retail records).
    pub flag: u8,
}

impl Hibernation {
    /// The distance past which the engine caches this object out (`dist[0]`, the per-object field).
    pub fn hibernation_distance(&self) -> u16 {
        self.dist[0]
    }
}

/// Parse a `HibernationControl` COMP `data` blob into (key -> [`Hibernation`]) records. Each record
/// is 10 bytes: `{u32 key, u16 dist0, u8 dist1, u8 dist2, u8 dist3, u8 flag}` (empirically verified
/// — the on-disk stride is 10, not the `schm` in-memory footprint). `dist1..3` are widened to u16.
fn parse_hibernation_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, Hibernation)> {
    const STRIDE: usize = 10;
    let mut out = Vec::new();
    if off + size > data.len() {
        return out;
    }
    let n = size / STRIDE;
    for i in 0..n {
        let r = off + i * STRIDE;
        if r + STRIDE > data.len() {
            break;
        }
        let key = read_u32_le(data, r);
        let dist0 = u16::from_le_bytes([data[r + 4], data[r + 5]]);
        let h = Hibernation {
            dist: [dist0, data[r + 6] as u16, data[r + 7] as u16, data[r + 8] as u16],
            flag: data[r + 9],
        };
        out.push((key, h));
    }
    out
}

/// Parse the `TerrainObject` COMP into (entity_key -> hi-res terrainmesh asset hash) records. Each
/// record is 8 bytes `{u32 entity_key, u32 terrainmesh_hash}`; the hash is a `0x7C569307`
/// "terrainmesh" asset (`extract_container`). Join with the entity's `Transform` (same key) for the
/// world placement — this is how the 400 hi-res terrain tiles are positioned (the c3 cell-id in the
/// block NAME is unrelated to the tile's world position).
fn parse_terrain_object_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, u32)> {
    const STRIDE: usize = 8;
    let mut out = Vec::new();
    if off + size > data.len() {
        return out;
    }
    let n = size / STRIDE;
    for i in 0..n {
        let r = off + i * STRIDE;
        if r + STRIDE > data.len() {
            break;
        }
        out.push((read_u32_le(data, r), read_u32_le(data, r + 4)));
    }
    out
}

/// One hi-res terrain tile placement: its `0x7C569307` terrainmesh asset hash and the world
/// transform (pos + quat, native game space) of its owning entity.
#[derive(Debug, Clone)]
pub struct TerrainTile {
    pub key: u32,
    pub terrainmesh_hash: u32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
}

/// Load every hi-res terrain-tile placement from a decompressed UCFX block: parse the `TerrainObject`
/// COMP (`key -> terrainmesh_hash`) and join it to the `Transform` COMP (`key -> pos/quat`) by entity
/// key within each sub-block. Coordinates stay native game space (LH, +Y up); no flips.
pub fn load_terrain_tiles(block: &[u8]) -> Vec<TerrainTile> {
    let ucfx_positions = find_all(block, b"UCFX");
    let mut out: Vec<TerrainTile> = Vec::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            block.len()
        };
        let mut xform: std::collections::HashMap<u32, ([f32; 3], [f32; 4])> =
            std::collections::HashMap::new();
        let mut terr: Vec<(u32, u32)> = Vec::new();
        for c in walk_sub_block_comps_full(block, ucfx_pos, block_end) {
            let Some((off, size)) = c.data else { continue };
            match c.info_name.as_deref() {
                Some("Transform") => {
                    for (k, p, q) in parse_transform_records(block, off, size) {
                        xform.entry(k).or_insert((p, q));
                    }
                }
                Some("TerrainObject") => {
                    terr.extend(parse_terrain_object_records(block, off, size));
                }
                _ => {}
            }
        }
        for (key, terrainmesh_hash) in terr {
            let Some(&(pos, quat)) = xform.get(&key) else { continue };
            out.push(TerrainTile { key, terrainmesh_hash, pos, quat });
        }
    }
    out
}

/// Load every `HibernationControl` per-entity streaming directive from a decompressed UCFX block,
/// keyed by entity key (matched across sub-blocks; keys are block-unique). This is the per-object
/// control data the streaming runtime joins to each placement to decide stream-in/out + LOD tier.
pub fn load_hibernation(block: &[u8]) -> std::collections::HashMap<u32, Hibernation> {
    let ucfx_positions = find_all(block, b"UCFX");
    let mut out: std::collections::HashMap<u32, Hibernation> = std::collections::HashMap::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            block.len()
        };
        for c in walk_sub_block_comps_full(block, ucfx_pos, block_end) {
            let Some((off, size)) = c.data else { continue };
            if c.info_name.as_deref() == Some("HibernationControl") {
                for (k, h) in parse_hibernation_records(block, off, size) {
                    out.entry(k).or_insert(h);
                }
            }
        }
    }
    out
}

/// A parsed `LightObject` ECS component record (reflection class `0x97e8ee92`) — a placed dynamic
/// light. The field order is taken verbatim from the reflection deserialize template
/// `FUN_006622e0` (Ghidra decomp) and `docs/mercs2-ecs/05_presentation_audio_fx.md`:
///
/// ```text
/// FUN_00656210(0)          -> int   light_type / id     (reflected field 0)
/// FUN_00656610(0)          -> rgb   color  (3 floats)    (reflected fields 1..3)
/// FUN_00656320(0) x 9      -> float params[9]            (reflected fields 4..12)
/// ```
///
/// giving a reflection stride of `0x34` (4 + 12 + 36 = 52 bytes). On disk each COMP `data` record
/// is prefixed with the u32 entity key (the same `[key][payload]` layout every other COMP uses —
/// Transform, Name, HibernationControl), so the on-disk record stride is `4 + 0x34 = 56` bytes.
///
/// The nine floats all default to `0.0` in the shipping `.rdata` (their live values arrive from the
/// level stream); the semantic *names* below are INFERRED from the class purpose and corroborated by
/// the UE port bindings (`docs/ue_game_bindings.md` → PointLight: color, `Intensity`,
/// `AttenuationRadius`). We therefore keep all nine raw floats and expose `intensity()`/`radius()`
/// as the best-known mapping (`params[0]` / `params[1]`) — a downstream consumer can re-map without
/// re-parsing if a live capture pins a different slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightObject {
    /// Reflected int field 0 — light type / id (point vs spot vs ..., engine-internal enum).
    pub light_type: u32,
    /// Reflected rgb color (fields 1..3), linear 0..1.
    pub color: [f32; 3],
    /// Reflected floats (fields 4..12): intensity, radius/attenuation, falloff, cone angles, …
    /// (exact per-slot semantics inferred; see the type doc). Kept raw so nothing is lost.
    pub params: [f32; 9],
}

impl LightObject {
    /// Reflection class hash (`pandemic_hash_m2("LightObject")`).
    pub const HASH: u32 = 0x97e8_ee92;
    /// Reflected payload stride (int + rgb + 9 floats) = 52 bytes.
    pub const PAYLOAD_STRIDE: usize = 0x34;
    /// On-disk COMP record stride: u32 entity key + reflected payload = 56 bytes.
    pub const RECORD_STRIDE: usize = 4 + Self::PAYLOAD_STRIDE;

    /// Inferred light intensity (`params[0]`; UE `Intensity`).
    pub fn intensity(&self) -> f32 {
        self.params[0]
    }
    /// Inferred attenuation radius in native game metres (`params[1]`; UE `AttenuationRadius`).
    pub fn radius(&self) -> f32 {
        self.params[1]
    }
}

/// Parse a `LightObject` COMP `data` blob into (entity_key -> [`LightObject`]) records. Each record
/// is [`LightObject::RECORD_STRIDE`] (56) bytes: `{u32 key, u32 light_type, f32 r,g,b, f32 params[9]}`.
fn parse_light_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, LightObject)> {
    const STRIDE: usize = LightObject::RECORD_STRIDE;
    let mut out = Vec::new();
    if off + size > data.len() {
        return out;
    }
    let n = size / STRIDE;
    for i in 0..n {
        let r = off + i * STRIDE;
        if r + STRIDE > data.len() {
            break;
        }
        let key = read_u32_le(data, r);
        let light_type = read_u32_le(data, r + 4);
        let color = [
            read_f32_le(data, r + 8),
            read_f32_le(data, r + 12),
            read_f32_le(data, r + 16),
        ];
        let mut params = [0f32; 9];
        for (j, p) in params.iter_mut().enumerate() {
            *p = read_f32_le(data, r + 20 + j * 4);
        }
        out.push((key, LightObject { light_type, color, params }));
    }
    out
}

/// One placed dynamic light: its `LightObject` params joined to the owning entity's `Transform`
/// (world position) by u32 key within the sub-block, plus the optional gameplay `Name`.
#[derive(Debug, Clone)]
pub struct PlacedLight {
    pub key: u32,
    pub name: Option<String>,
    pub pos: [f32; 3],
    pub light: LightObject,
    pub sub_block: u16,
}

impl PlacedLight {
    /// World position (native game space, LH +Y up) — the light's placement.
    pub fn position(&self) -> [f32; 3] {
        self.pos
    }
}

/// Harvest every placed `LightObject` from a decompressed UCFX block (`layers_static` or a
/// `vz_state` overlay), joining each light to its entity `Transform` (world position) and `Name`
/// by key within the sub-block. Records without a matching `Transform` are skipped (their world
/// position is unknown). Coordinates stay native game space; no flips.
pub fn light_inventory(block: &[u8]) -> Vec<PlacedLight> {
    let ucfx_positions = find_all(block, b"UCFX");
    let mut out: Vec<PlacedLight> = Vec::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            block.len()
        };
        let comps = walk_sub_block_comps(block, ucfx_pos, block_end);

        let mut transforms: std::collections::HashMap<u32, [f32; 3]> = std::collections::HashMap::new();
        let mut names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        let mut lights: Vec<(u32, LightObject)> = Vec::new();
        for c in &comps {
            let Some((off, size)) = c.data else { continue };
            match c.info_name.as_deref() {
                Some("Transform") => {
                    for (k, p, _q) in parse_transform_records(block, off, size) {
                        transforms.entry(k).or_insert(p);
                    }
                }
                Some("Name") => {
                    for (k, n) in parse_name_records(block, off, size) {
                        names.entry(k).or_insert(n);
                    }
                }
                Some("LightObject") => {
                    lights.extend(parse_light_records(block, off, size));
                }
                _ => {}
            }
        }

        for (key, light) in lights {
            let Some(&pos) = transforms.get(&key) else { continue };
            out.push(PlacedLight {
                key,
                name: names.get(&key).cloned(),
                pos,
                light,
                sub_block: si as u16,
            });
        }
    }
    out
}

/// Yaw (radians) around +Y from a placement quaternion: `2 * atan2(qy, qw)`.
pub fn yaw_from_quat(quat: &[f32; 4]) -> f32 {
    2.0 * quat[1].atan2(quat[3])
}

/// One prop placement: the entity key, the model asset hash it renders as (the `ModelName`
/// COMP record's second u32 = `pandemic_hash_m2(model-name)` = the model ASET `asset_hash`),
/// its world transform (pos + full quat, native game space — no flip), and the optional
/// gameplay name (from the `Name` COMP).
#[derive(Debug, Clone)]
pub struct ModelPlacement {
    pub key: u32,
    pub model_hash: u32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub name: Option<String>,
    /// Per-entity streaming/LOD directive (from a `HibernationControl` COMP on the same entity),
    /// if present. `None` = the object uses class-default distances (100 / 160 / 60 / 20).
    pub hibernation: Option<Hibernation>,
}

/// Parse a `ModelName` COMP `data` blob into (key -> model_hash) records. Each record is
/// `{u32 entity_key, u32 model_hash}` (8-byte stride); the model_hash is the model ASET
/// asset_hash (== `pandemic_hash_m2(model-name)`).
fn parse_model_name_records(data: &[u8], off: usize, size: usize) -> Vec<(u32, u32)> {
    const STRIDE: usize = 8;
    let mut out = Vec::new();
    if off + size > data.len() {
        return out;
    }
    let n = size / STRIDE;
    for i in 0..n {
        let r = off + i * STRIDE;
        if r + STRIDE > data.len() {
            break;
        }
        out.push((read_u32_le(data, r), read_u32_le(data, r + 4)));
    }
    out
}

/// Load every `ModelName` prop placement from a decompressed UCFX block (exterior
/// `layers_static` block 29, or an interior `vz_state_*` block such as 667), joined to its
/// `Transform` (pos + full quat) and `Name` COMP by u32 entity key within each sub-block.
///
/// The `ModelName` COMP is the entity->mesh link the engine uses for discrete props/furniture
/// (buildings are baked into c3 cells). One `ModelPlacement` is emitted per `ModelName` record
/// that has a matching `Transform` in the same sub-block; records without a Transform are
/// skipped (their world position is unknown). Coordinates stay entirely in native game space
/// (left-handed, +Y up) — no flips. The full quaternion is preserved (16% of props carry
/// pitch/roll, not just yaw).
pub fn load_model_placements(block: &[u8]) -> Vec<ModelPlacement> {
    let ucfx_positions = find_all(block, b"UCFX");
    let mut out: Vec<ModelPlacement> = Vec::new();
    for (si, &ucfx_pos) in ucfx_positions.iter().enumerate() {
        let block_end = if si + 1 < ucfx_positions.len() {
            ucfx_positions[si + 1]
        } else {
            block.len()
        };

        let mut xform: std::collections::HashMap<u32, ([f32; 3], [f32; 4])> =
            std::collections::HashMap::new();
        let mut names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        let mut hib: std::collections::HashMap<u32, Hibernation> = std::collections::HashMap::new();
        let mut models: Vec<(u32, u32)> = Vec::new();
        for c in walk_sub_block_comps_full(block, ucfx_pos, block_end) {
            let Some((off, size)) = c.data else { continue };
            match c.info_name.as_deref() {
                Some("Transform") => {
                    for (k, p, q) in parse_transform_records(block, off, size) {
                        xform.entry(k).or_insert((p, q));
                    }
                }
                Some("Name") => {
                    for (k, n) in parse_name_records(block, off, size) {
                        names.entry(k).or_insert(n);
                    }
                }
                Some("HibernationControl") => {
                    for (k, h) in parse_hibernation_records(block, off, size) {
                        hib.entry(k).or_insert(h);
                    }
                }
                Some("ModelName") => {
                    models.extend(parse_model_name_records(block, off, size));
                }
                _ => {}
            }
        }

        for (key, model_hash) in models {
            let Some(&(pos, quat)) = xform.get(&key) else { continue };
            out.push(ModelPlacement {
                key,
                model_hash,
                pos,
                quat,
                name: names.get(&key).cloned(),
                hibernation: hib.get(&key).copied(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Transform record is exactly 42 bytes: key + XYZ + pad + quat + 6-tail.
    #[test]
    fn transform_record_stride_42() {
        // Two back-to-back records; second key must be read at +42.
        let mut d = vec![0u8; 84];
        d[0..4].copy_from_slice(&0x1111_1111u32.to_le_bytes());
        d[4..8].copy_from_slice(&12.5f32.to_le_bytes()); // x
        d[8..12].copy_from_slice(&(-3.0f32).to_le_bytes()); // y
        d[12..16].copy_from_slice(&7.0f32.to_le_bytes()); // z
        d[32..36].copy_from_slice(&1.0f32.to_le_bytes()); // qw
        d[42..46].copy_from_slice(&0x2222_2222u32.to_le_bytes());
        let recs = parse_transform_records(&d, 0, 84);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].0, 0x1111_1111);
        assert_eq!(recs[0].1, [12.5, -3.0, 7.0]);
        assert_eq!(recs[0].2, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(recs[1].0, 0x2222_2222);
    }

    /// Name records are `[u32 key][ascii name 0xHEXID\0]`; the bare name is kept.
    #[test]
    fn name_record_strips_hex_id() {
        let mut d = Vec::new();
        d.extend_from_slice(&0x00AB_CDEFu32.to_le_bytes());
        d.extend_from_slice(b"pmc_interior 0xABCDEF\0");
        let recs = parse_name_records(&d, 0, d.len());
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].0, 0x00AB_CDEF);
        assert_eq!(recs[0].1, "pmc_interior");
    }

    /// A ModelName record is `{u32 key, u32 model_hash}` at 8-byte stride.
    #[test]
    fn model_name_record_stride_8() {
        let mut d = vec![0u8; 16];
        d[0..4].copy_from_slice(&0xAAAA_0001u32.to_le_bytes());
        d[4..8].copy_from_slice(&0x5B72_4250u32.to_le_bytes());
        d[8..12].copy_from_slice(&0xAAAA_0002u32.to_le_bytes());
        d[12..16].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        let recs = parse_model_name_records(&d, 0, 16);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0], (0xAAAA_0001, 0x5B72_4250));
        assert_eq!(recs[1], (0xAAAA_0002, 0xDEAD_BEEF));
    }

    /// A HibernationControl record is 10 bytes: {u32 key, u16 dist0, u8 dist1, u8 dist2, u8 dist3,
    /// u8 flag}. Bytes are the first two real retail records from layers_static sub_block 2.
    #[test]
    fn hibernation_record_stride_10() {
        // rec0: key 0x00095b1d, dist0=0x00df=223, 160/60/20, flag 0.
        // rec3: key 0x00095b59, dist0=0x01f4=500 (>400 warn), 160/60/20, flag 0.
        let d: Vec<u8> = vec![
            0x1d, 0x5b, 0x09, 0x00, 0xdf, 0x00, 0xa0, 0x3c, 0x14, 0x00, //
            0x59, 0x5b, 0x09, 0x00, 0xf4, 0x01, 0xa0, 0x3c, 0x14, 0x00,
        ];
        let recs = parse_hibernation_records(&d, 0, d.len());
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].0, 0x0009_5b1d);
        assert_eq!(recs[0].1.dist, [223, 160, 60, 20]);
        assert_eq!(recs[0].1.hibernation_distance(), 223);
        assert_eq!(recs[0].1.flag, 0);
        assert_eq!(recs[1].0, 0x0009_5b59);
        assert_eq!(recs[1].1.dist, [500, 160, 60, 20]);
        assert!(recs[1].1.hibernation_distance() > 400); // triggers the fall-through-terrain warning
    }

    /// A LightObject record is 56 bytes: {u32 key, u32 type, f32 r,g,b, f32 params[9]} — the
    /// reflection field order from FUN_006622e0 (int + rgb + 9 floats), prefixed by the entity key.
    #[test]
    fn light_object_record_stride_56() {
        assert_eq!(LightObject::RECORD_STRIDE, 56);
        assert_eq!(LightObject::PAYLOAD_STRIDE, 0x34);
        let mut d = vec![0u8; LightObject::RECORD_STRIDE * 2];
        // rec0
        d[0..4].copy_from_slice(&0x0009_5c10u32.to_le_bytes()); // key
        d[4..8].copy_from_slice(&1u32.to_le_bytes()); // light_type
        d[8..12].copy_from_slice(&1.0f32.to_le_bytes()); // r
        d[12..16].copy_from_slice(&0.5f32.to_le_bytes()); // g
        d[16..20].copy_from_slice(&0.25f32.to_le_bytes()); // b
        d[20..24].copy_from_slice(&8.0f32.to_le_bytes()); // params[0] = intensity
        d[24..28].copy_from_slice(&12.5f32.to_le_bytes()); // params[1] = radius
        d[52..56].copy_from_slice(&0.75f32.to_le_bytes()); // params[8] (last float, at +32)
        // rec1 key must be read exactly at +56.
        d[56..60].copy_from_slice(&0x0009_5c11u32.to_le_bytes());
        d[60..64].copy_from_slice(&2u32.to_le_bytes());

        let recs = parse_light_records(&d, 0, d.len());
        assert_eq!(recs.len(), 2);
        let (k0, l0) = recs[0];
        assert_eq!(k0, 0x0009_5c10);
        assert_eq!(l0.light_type, 1);
        assert_eq!(l0.color, [1.0, 0.5, 0.25]);
        assert_eq!(l0.intensity(), 8.0);
        assert_eq!(l0.radius(), 12.5);
        assert_eq!(l0.params[8], 0.75);
        assert_eq!(recs[1].0, 0x0009_5c11);
        assert_eq!(recs[1].1.light_type, 2);
        assert_eq!(LightObject::HASH, 0x97e8_ee92);
    }

    /// `light_inventory` joins each LightObject to its entity Transform by key and drops any light
    /// whose key has no Transform (unknown world position).
    #[test]
    fn light_inventory_joins_transform_and_drops_orphan() {
        // Build a minimal one-sub-block UCFX/CHDR/COMP fixture with a Transform COMP and a
        // LightObject COMP. Layout mirrors walk_sub_block_comps: UCFX(hdr) .. CHDR(hdr, entries) ..
        // COMP rows (each: 20B hdr + child descriptors) .. data area (children, offsets relative to
        // the end of the descriptor table).
        fn u32b(v: u32) -> [u8; 4] {
            v.to_le_bytes()
        }
        // --- data-area children (built first so we know their sizes/offsets) ---
        // Transform: two records (light key 0x10 present, plus an orphan-free extra 0x11).
        let mut xform = Vec::new();
        for (key, x) in [(0x10u32, 100.0f32), (0x11u32, 7.0f32)] {
            let mut r = vec![0u8; 42];
            r[0..4].copy_from_slice(&u32b(key));
            r[4..8].copy_from_slice(&x.to_le_bytes()); // pos.x
            r[32..36].copy_from_slice(&1.0f32.to_le_bytes()); // qw
            xform.extend_from_slice(&r);
        }
        // LightObject: key 0x10 (joins) + key 0x99 (orphan, no Transform -> dropped).
        let mut lights = Vec::new();
        for (key, inten) in [(0x10u32, 5.0f32), (0x99u32, 9.0f32)] {
            let mut r = vec![0u8; LightObject::RECORD_STRIDE];
            r[0..4].copy_from_slice(&u32b(key));
            r[8..12].copy_from_slice(&1.0f32.to_le_bytes()); // r
            r[20..24].copy_from_slice(&inten.to_le_bytes()); // intensity
            lights.extend_from_slice(&r);
        }
        let info_t = b"Transform\0";
        let info_l = b"LightObject\0";

        // Descriptor table = 2 COMP rows. Each COMP row: 20B header (tag, _, _, _, num_children) +
        // num_children * 20B child descriptors {ctag, coff(rel data_area), csz, _, _}.
        // COMP0 (Transform): children info, data. COMP1 (LightObject): children info, data.
        let comp_hdr = |num_children: u32| {
            let mut h = vec![0u8; CHUNK_HDR];
            h[0..4].copy_from_slice(b"COMP");
            h[16..20].copy_from_slice(&u32b(num_children));
            h
        };
        let child = |tag: &[u8; 4], coff: u32, csz: u32| {
            let mut h = vec![0u8; CHUNK_HDR];
            h[0..4].copy_from_slice(tag);
            h[4..8].copy_from_slice(&u32b(coff));
            h[8..12].copy_from_slice(&u32b(csz));
            h
        };
        // Compute data-area child offsets (relative to data_area_start), laid out in order:
        // [info_t][xform][info_l][lights].
        let off_info_t = 0u32;
        let off_xform = off_info_t + info_t.len() as u32;
        let off_info_l = off_xform + xform.len() as u32;
        let off_lights = off_info_l + info_l.len() as u32;

        let mut table = Vec::new();
        table.extend(comp_hdr(2));
        table.extend(child(b"info", off_info_t, info_t.len() as u32));
        table.extend(child(b"data", off_xform, xform.len() as u32));
        table.extend(comp_hdr(2));
        table.extend(child(b"info", off_info_l, info_l.len() as u32));
        table.extend(child(b"data", off_lights, lights.len() as u32));

        let mut data_area = Vec::new();
        data_area.extend_from_slice(info_t);
        data_area.extend_from_slice(&xform);
        data_area.extend_from_slice(info_l);
        data_area.extend_from_slice(&lights);

        // Assemble block: UCFX header (size covers CHDR onwards is not required by the walker beyond
        // the +200 search window), CHDR header (entries=2), the descriptor table, then the data area.
        let mut chdr = vec![0u8; 20];
        chdr[0..4].copy_from_slice(b"CHDR");
        chdr[12..16].copy_from_slice(&u32b(2)); // chdr_entries
        let mut block = Vec::new();
        let mut ucfx = vec![0u8; 8];
        ucfx[0..4].copy_from_slice(b"UCFX");
        // ucfx_size: any value; the CHDR search window is ucfx_pos..ucfx_pos+ucfx_size+200.
        ucfx[4..8].copy_from_slice(&u32b(8));
        block.extend_from_slice(&ucfx);
        block.extend_from_slice(&chdr);
        block.extend_from_slice(&table);
        block.extend_from_slice(&data_area);

        let placed = light_inventory(&block);
        assert_eq!(placed.len(), 1, "only the light with a matching Transform survives");
        assert_eq!(placed[0].key, 0x10);
        assert_eq!(placed[0].pos, [100.0, 0.0, 0.0]);
        assert_eq!(placed[0].light.intensity(), 5.0);
        assert_eq!(placed[0].light.color, [1.0, 0.0, 0.0]);
    }

    /// A pure-yaw quaternion sample is unit length, and yaw round-trips.
    #[test]
    fn quat_unit_length_and_yaw() {
        let yaw = 1.2345f32;
        let q = [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()];
        let mag = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!((mag - 1.0).abs() < 1e-5, "quat mag {mag}");
        assert!((yaw_from_quat(&q) - yaw).abs() < 1e-4);
    }
}
