//! Model / mesh UCFX consumption (GEOM, STRM, IBUF, BNDS, HIER, PRMG).

use crate::consume::ConsumeResult;
use mercs2_formats::chunk_validate::validate_skin_containers;
use mercs2_formats::ffcs::{read_f32_le, read_u32_le};
use mercs2_formats::ucfx::extract_chunk_body;

fn read_u16_le(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([data[off], data[off + 1]])
}

pub fn consume_model(container: &[u8], _data_body: Option<&[u8]>, label: &str) -> ConsumeResult {
    let mut issues = Vec::new();
    let mut meshes_validated = 0usize;
    let mut xref_hashes = Vec::new();
    let mut material_diffuse_hashes = Vec::new();
    let mut bounds_violations = 0usize;
    // Advisory (NON-fatal): heuristic checks with unverified offsets/strides that
    // false-positive on WADs that load fine in-game. Reported but excluded from
    // the verdict (mirrors ecs_float_violations).
    let mut vertex_advisory = 0usize;
    let mut bounds_advisory = 0usize;
    let mut structural_advisory = 0u32;
    // FATAL: counts a converter defect the engine cannot survive (e.g. an MTRL
    // texture-count that overruns the engine's fixed 10-slot array → heap
    // corruption / AV 0x0084DD5B). Routed into the verdict.
    let mut structural_violations = 0u32;

    if let Some(geom) = extract_chunk_body(container, b"GEOM") {
        if geom.len() >= 8 {
            let n_groups = read_u32_le(&geom, 0);
            if n_groups > 10_000 {
                issues.push(format!("{label}: GEOM n_groups={n_groups} implausible"));
            } else {
                meshes_validated += 1;
            }
        }
    }

    // STRM vertex validation (container-based) — also yields vertex_count and sampled positions
    let strm_vertex_count: Option<usize>;
    let sampled_positions: Vec<[f32; 3]>;
    {
        let (found, vv, vc, positions, strm_issues) = validate_strm_vertices(container, label);
        vertex_advisory += vv;
        strm_vertex_count = vc;
        sampled_positions = positions;
        issues.extend(strm_issues);
        if found {
            meshes_validated += 1;
        } else if let Some(strm) = extract_chunk_body(container, b"STRM") {
            if strm.len() < 4 {
                issues.push(format!("{label}: STRM too small"));
            } else {
                meshes_validated += 1;
            }
        }
    }

    // IBUF: index count validation + P2-3 max(index) < vertex_count
    if let Some(ibuf) = extract_chunk_body(container, b"IBUF") {
        if ibuf.len() < 4 {
            issues.push(format!("{label}: IBUF too small"));
        } else {
            let index_count = read_u32_le(&ibuf, 0);
            let needed = 4 + index_count as usize * 2;
            if needed > ibuf.len() && index_count < 10_000_000 {
                issues.push(format!(
                    "{label}: IBUF index_count={index_count} needs ~{needed} bytes, have {}",
                    ibuf.len()
                ));
            } else {
                meshes_validated += 1;

                if let Some(vert_count) = strm_vertex_count {
                    let actual_indices = ((ibuf.len() - 4) / 2).min(index_count as usize);
                    let mut max_idx: u16 = 0;
                    for i in 0..actual_indices {
                        let idx = read_u16_le(&ibuf, 4 + i * 2);
                        if idx > max_idx {
                            max_idx = idx;
                        }
                    }
                    if actual_indices > 0 && (max_idx as usize) >= vert_count {
                        issues.push(format!(
                            "{label}: IBUF max index {max_idx} >= STRM vertex_count {vert_count}"
                        ));
                        structural_advisory += 1;
                    }
                }
            }
        }
    }

    // BNDS: 40-byte structure (center_xyz + radius + min_xyz + max_xyz)
    if let Some(bnds) = extract_chunk_body(container, b"BNDS") {
        if bnds.len() >= 40 {
            let cx = read_f32_le(&bnds, 0);
            let cy = read_f32_le(&bnds, 4);
            let cz = read_f32_le(&bnds, 8);
            let radius = read_f32_le(&bnds, 12);
            let min_x = read_f32_le(&bnds, 16);
            let min_y = read_f32_le(&bnds, 20);
            let min_z = read_f32_le(&bnds, 24);
            let max_x = read_f32_le(&bnds, 28);
            let max_y = read_f32_le(&bnds, 32);
            let max_z = read_f32_le(&bnds, 36);

            let all = [cx, cy, cz, radius, min_x, min_y, min_z, max_x, max_y, max_z];
            if all.iter().any(|f| !f.is_finite()) {
                issues.push(format!("{label}: BNDS contains NaN/Inf"));
                bounds_violations += 1;
            } else {
                let mut ok = true;
                if min_x > max_x || min_y > max_y || min_z > max_z {
                    issues.push(format!(
                        "{label}: BNDS AABB inverted: min=({min_x}, {min_y}, {min_z}) max=({max_x}, {max_y}, {max_z})"
                    ));
                    ok = false;
                }
                if radius <= 0.0 {
                    issues.push(format!("{label}: BNDS radius non-positive: {radius}"));
                    ok = false;
                }
                if cx < min_x || cx > max_x || cy < min_y || cy > max_y || cz < min_z || cz > max_z {
                    issues.push(format!(
                        "{label}: BNDS center ({cx}, {cy}, {cz}) outside AABB"
                    ));
                    ok = false;
                }
                if !ok {
                    bounds_violations += 1;
                }

                // P2-4: BNDS envelope must contain sampled STRM vertices
                if ok && !sampled_positions.is_empty() {
                    let ext_x = (max_x - min_x).max(1.0);
                    let ext_y = (max_y - min_y).max(1.0);
                    let ext_z = (max_z - min_z).max(1.0);
                    let eps_x = (0.01 * ext_x).max(1.0);
                    let eps_y = (0.01 * ext_y).max(1.0);
                    let eps_z = (0.01 * ext_z).max(1.0);

                    let mut oob_count = 0usize;
                    for pos in &sampled_positions {
                        let [vx, vy, vz] = *pos;
                        if vx < min_x - eps_x || vx > max_x + eps_x
                            || vy < min_y - eps_y || vy > max_y + eps_y
                            || vz < min_z - eps_z || vz > max_z + eps_z
                        {
                            oob_count += 1;
                        }
                    }
                    if oob_count > 0 {
                        issues.push(format!(
                            "{label}: BNDS envelope does not contain {oob_count}/{} sampled vertices",
                            sampled_positions.len()
                        ));
                        structural_advisory += 1;
                    }
                }
            }
            meshes_validated += 1;
        }
    }

    // HIER: 176-byte node validation
    if let Some(hier) = extract_chunk_body(container, b"HIER") {
        if hier.len() >= 176 {
            if hier.len() % 176 != 0 {
                issues.push(format!(
                    "{label}: HIER body len {} not a multiple of 176",
                    hier.len()
                ));
                bounds_advisory += 1;
            } else {
                let node_count = hier.len() / 176;
                let mut hier_ok = true;
                for ni in 0..node_count {
                    let base = ni * 176;
                    let mut mat_ok = true;
                    for fi in 0..16 {
                        let f = read_f32_le(&hier, base + 16 + fi * 4);
                        if !f.is_finite() {
                            issues.push(format!(
                                "{label}: HIER node[{ni}] local matrix float[{fi}] NaN/Inf"
                            ));
                            mat_ok = false;
                            hier_ok = false;
                            break;
                        }
                    }
                    if !mat_ok {
                        continue;
                    }
                    let bmin = [
                        read_f32_le(&hier, base + 144),
                        read_f32_le(&hier, base + 148),
                        read_f32_le(&hier, base + 152),
                    ];
                    let bmax = [
                        read_f32_le(&hier, base + 160),
                        read_f32_le(&hier, base + 164),
                        read_f32_le(&hier, base + 168),
                    ];
                    if bmin.iter().chain(bmax.iter()).any(|f| !f.is_finite()) {
                        issues.push(format!("{label}: HIER node[{ni}] tail bbox NaN/Inf"));
                        hier_ok = false;
                    } else if bmin[0] > bmax[0] || bmin[1] > bmax[1] || bmin[2] > bmax[2] {
                        issues.push(format!(
                            "{label}: HIER node[{ni}] tail bbox inverted: min=({}, {}, {}) max=({}, {}, {})",
                            bmin[0], bmin[1], bmin[2], bmax[0], bmax[1], bmax[2]
                        ));
                        hier_ok = false;
                    }
                }
                if !hier_ok {
                    bounds_advisory += 1;
                }
            }
            meshes_validated += 1;
        }
    }

    // PRMG INFO validation (container-based)
    {
        let (prmg_v, prmg_issues) = validate_prmg_info(container, label);
        bounds_advisory += prmg_v;
        issues.extend(prmg_issues);
    }

    // AREA sub-area validation (container-based). Engine handler @0x4a4ab0.
    {
        let (area_v, area_issues) = validate_area(container, label);
        structural_advisory += area_v;
        issues.extend(area_issues);
    }

    if let Some(mtrl) = extract_chunk_body(container, b"MTRL") {
        // MTRL layout (decompile FUN_00858790, spatial/streaming docs):
        // [u32/f32 × 26 = 104B][u16 flags @104][u16 count @106][u32 hash × count @108][u32×2].
        // The texture hashes the engine resolves live at +108 (count @106), NOT at +0
        // (+0 is the first material param). Reading +0 produced garbage that resolved to
        // nothing. The engine writes into a fixed 10-slot array, so cap the count at 10.
        if mtrl.len() >= 108 {
            // The engine (FUN_00858790) writes `raw_count` texture records into a
            // FIXED 10-slot array at material+0xAC. A raw count > 10 overruns it →
            // heap corruption → world-load AV (0x0084DD5B / downstream 0x004CF58B).
            // We must FLAG the raw count (fatal); the clamp below is only so our
            // own xref read stays in-bounds — it must NOT hide the overrun.
            let raw_count = read_u16_le(&mtrl, 106) as usize;
            if raw_count > 10 {
                issues.push(format!(
                    "{label}: MTRL texture-count {raw_count} > 10 — overruns engine's \
                     fixed 10-slot array (heap corruption / AV 0x0084DD5B)"
                ));
                structural_violations += 1;
            }
            let count = raw_count.min(10);
            if std::env::var("MTRL_DEBUG").is_ok() {
                let raw_count = read_u16_le(&mtrl, 106);
                eprintln!(
                    "[MTRL/model] {label}: len={} flags@104=0x{:04X} raw_count@106={} (cap {}) off0=0x{:08X}",
                    mtrl.len(),
                    read_u16_le(&mtrl, 104),
                    raw_count,
                    count,
                    read_u32_le(&mtrl, 0),
                );
                let hi = mtrl.len().min(160);
                let hex: String = mtrl[96..hi]
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        if (96 + i) % 4 == 0 { format!(" |{:3}| {:02x}", 96 + i, b) }
                        else { format!("{:02x}", b) }
                    })
                    .collect();
                eprintln!("    bytes[96..{hi}]:{hex}");
            }
            for i in 0..count {
                let off = 108 + i * 4;
                if off + 4 > mtrl.len() {
                    break;
                }
                let tex_hash = read_u32_le(&mtrl, off);
                if tex_hash != 0 && tex_hash != 0xFFFF_FFFF {
                    if std::env::var("MTRL_DEBUG").is_ok() {
                        eprintln!("    hash[{i}] @+{off} = 0x{tex_hash:08X}");
                    }
                    xref_hashes.push(tex_hash);
                }
            }

            // MULTI-material walk for the DLC texture-provenance render check.
            // The single-material read above only sees material[0]'s textures;
            // real character models are multi-material (often many materials).
            // Walk every material at the engine/converter stride (116 + count*4,
            // count-pair @material+104) and collect each material's DIFFUSE hash
            // (the first texture hash of the group, at material+108). These feed
            // the provenance advisory in simulate.rs.
            material_diffuse_hashes = mtrl_diffuse_hashes(&mtrl);
        }
    }

    // Skin-container and DEPS checks are heuristic (unverified offsets); they
    // fire on retail-shipped models too, so they are advisory, not fatal.
    for skin_msg in validate_skin_containers(container) {
        issues.push(format!("{label}: {skin_msg}"));
        structural_advisory += 1;
    }

    if let Some(deps) = extract_chunk_body(container, b"DEPS") {
        if let Some(msg) = mercs2_formats::chunk_validate::validate_deps_body(&deps) {
            issues.push(format!("{label}: {msg}"));
            structural_advisory += 1;
        }
    }

    // Buffer-too-small: a texture EMBEDDED in this model container (uppercase
    // INFO/BODY) never gets its own consume_texture dispatch, so check it here.
    // Headline signal — routed to texture_buffer_issues, NOT structural_violations.
    let (texture_buffer_issues, _) =
        crate::texture::check_embedded_texture_buffers(container, label);

    ConsumeResult {
        consumed: true,
        issues,
        xref_hashes,
        material_diffuse_hashes,
        meshes_validated,
        bounds_violations,
        structural_violations,
        texture_buffer_issues,
        vertex_advisory,
        bounds_advisory,
        structural_advisory,
        ..Default::default()
    }
}

/// Walk EVERY material in an MTRL body and return each material's DIFFUSE texture
/// hash (the first texture hash of the material's texture group), in material order.
///
/// Layout (mirrors the converter's `convert_mtrl` / `mtrl_tiles_standard` walk and
/// the engine parser FUN_00858790): each material record is
///   `[104-byte param block][u16 flags @104][u16 count @106][count × u32 tex-hash][8-byte tail]`
/// so the record stride is `116 + count*4` and the DIFFUSE hash is the first of the
/// `count` hashes, at `material_start + 108`. A skinned character model can carry
/// many materials; the per-record stride varies with each material's texture count
/// (e.g. 120/128 bytes for counts 1/3).
///
/// The count word can be in PC form `[flags][count]` or transposed `[count][0]`
/// (the generic u32 swap leaves material[1..] transposed); we accept either. If a
/// record's count is implausible (0 or >64) or the next stride runs off the end,
/// we stop walking — never reading garbage past the array.
fn mtrl_diffuse_hashes(mtrl: &[u8]) -> Vec<u32> {
    const PARAM_BLOCK: usize = 104;
    const TAIL: usize = 8;
    let mut out = Vec::new();
    let mut rec = 0usize;
    while rec + PARAM_BLOCK + 4 <= mtrl.len() {
        let cp = rec + PARAM_BLOCK; // count-pair offset (flags@cp, count@cp+2)
        let lo = read_u16_le(mtrl, cp);
        let hi = read_u16_le(mtrl, cp + 2);
        // PC form is [flags][count]; the generic swap may transpose to [count][0].
        let count = if (1..=64).contains(&hi) {
            hi as usize
        } else if hi == 0 && (1..=64).contains(&lo) {
            lo as usize // transposed
        } else {
            break; // unrecognized record layout — stop, don't read garbage
        };
        let diffuse_off = cp + 4; // first texture hash = diffuse
        if diffuse_off + 4 > mtrl.len() {
            break;
        }
        let diffuse = read_u32_le(mtrl, diffuse_off);
        if diffuse != 0 && diffuse != 0xFFFF_FFFF {
            out.push(diffuse);
        }
        rec += PARAM_BLOCK + 4 + count * 4 + TAIL;
    }
    out
}

struct ContainerChild {
    tag: [u8; 4],
    body_start: usize,
    body_size: usize,
}

/// Find a container descriptor (u0 == 0xFFFFFFFF) by tag and return its
/// immediate children (descriptors up to the next container header).
fn find_container_children(container: &[u8], target_tag: &[u8; 4]) -> Vec<ContainerChild> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Vec::new();
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc || n_desc == 0 {
        return Vec::new();
    }

    let mut found = false;
    let mut children = Vec::new();

    for i in 0..n_desc {
        let row_off = 20 + i * 20;
        if row_off + 20 > container.len() {
            break;
        }
        let tag = &container[row_off..row_off + 4];
        let row_u0 = read_u32_le(container, row_off + 4);

        if tag == target_tag && row_u0 == 0xFFFF_FFFF {
            found = true;
            continue;
        }

        if found {
            if row_u0 == 0xFFFF_FFFF {
                break;
            }
            let row_u0 = row_u0 as usize;
            let body_size = read_u32_le(container, row_off + 8) as usize;
            let body_start = if data_area_off > 0 {
                data_area_off + row_u0
            } else {
                8 + row_u0
            };
            if body_start + body_size > container.len() {
                continue;
            }
            let mut child_tag = [0u8; 4];
            child_tag.copy_from_slice(tag);
            children.push(ContainerChild {
                tag: child_tag,
                body_start,
                body_size,
            });
        }
    }

    children
}

/// Validate STRM vertex buffer via container descriptor walk.
/// Returns (found, vertex_violations, vertex_count, sampled_positions, issues).
fn validate_strm_vertices(container: &[u8], label: &str) -> (bool, usize, Option<usize>, Vec<[f32; 3]>, Vec<String>) {
    let children = find_container_children(container, b"STRM");
    if children.is_empty() {
        return (false, 0, None, Vec::new(), Vec::new());
    }

    let mut decl_body: Option<&[u8]> = None;
    let mut data_body: Option<&[u8]> = None;
    let mut info_body: Option<&[u8]> = None;

    for child in &children {
        match &child.tag {
            b"decl" => {
                decl_body = Some(&container[child.body_start..child.body_start + child.body_size])
            }
            b"data" => {
                data_body = Some(&container[child.body_start..child.body_start + child.body_size])
            }
            b"info" => {
                info_body = Some(&container[child.body_start..child.body_start + child.body_size])
            }
            _ => {}
        }
    }

    let decl = match decl_body {
        Some(d) if d.len() >= 8 => d,
        _ => return (true, 0, None, Vec::new(), Vec::new()),
    };
    let data = match data_body {
        Some(d) if !d.is_empty() => d,
        _ => return (true, 0, None, Vec::new(), Vec::new()),
    };

    // The AUTHORITATIVE per-vertex stride is the STRM `info` chunk's stride field
    // ({u32 _, u32 stride, u32 count} — stride@+4), NOT the decl-derived extent.
    // Retail PC decls legitimately contain FLOAT16_4 elements whose declared
    // offset+size overruns the packed vertex stride (e.g. a normal at offset 12
    // declared FLOAT16_4 (8B) → extent 20 over a 16-byte vertex): the engine binds
    // the stream at the info stride and lets the final element's read overlap.
    // Deriving the stride from the decl over-reads by +4B/vertex and reported
    // thousands of FALSE NaN — the vertex data itself is correct (0 NaN at the info
    // stride). Prefer the info-chunk stride; fall back to the decl extent only when
    // the info chunk is absent/implausible.
    let (decl_stride, decl_all_u16, decl_first_off) = decl_vertex_format(decl);
    let info_stride = info_body
        .filter(|b| b.len() >= 12)
        .map(|b| read_u32_le(b, 4) as usize)
        .filter(|&s| (6..=256).contains(&s));
    let stride = info_stride.unwrap_or(decl_stride);
    if stride < 6 || stride > 256 {
        return (
            true,
            0,
            None,
            Vec::new(),
            vec![format!(
                "{label}: STRM decl stride {stride} out of range [6, 256]"
            )],
        );
    }

    // Decide the position format (FLOAT16 vs FLOAT32) from the position SIZE: the
    // position occupies offset 0 .. the first listed element's offset. 8 bytes ⟹
    // FLOAT16_4 (f16); 12 bytes ⟹ FLOAT3 (f32). This structural signal is reliable;
    // the alternatives are not: gating on "all listed elements are u16" misclassifies
    // a FLOAT16 position when the mesh also lists a FLOAT32 element, and sampling the
    // f32 view misclassifies an f16 mesh whose first vertices happen to read as a sane
    // float3. Reading f16 bytes as f32 produces ~1e10 garbage (the 0.00786/denormal
    // signature), so getting this right is what keeps the bounded check honest.
    let pos_is_f16 = match decl_first_off {
        Some(0) => decl_all_u16, // position is the first listed element: trust its kind
        Some(off) => off <= 8,   // implicit position spanning `off` bytes (8=F16_4, 12=FLOAT3)
        None => stride < 12 || !float3_view_is_sane(data, stride), // no elements: data heuristic
    };
    let pos_bytes = if pos_is_f16 { 6 } else { 12 };
    let vertex_count = data.len() / stride;
    let check_count = vertex_count.min(128);
    let mut violations = 0usize;
    let mut issues = Vec::new();
    let mut positions = Vec::with_capacity(check_count);

    for vi in 0..check_count {
        let off = vi * stride;
        if off + pos_bytes > data.len() {
            break;
        }
        let (vx, vy, vz) = if pos_is_f16 {
            (
                read_f16_le(data, off),
                read_f16_le(data, off + 2),
                read_f16_le(data, off + 4),
            )
        } else {
            (
                read_f32_le(data, off),
                read_f32_le(data, off + 4),
                read_f32_le(data, off + 8),
            )
        };

        // Position sanity is not just finiteness (NaN/Inf) but a plausible
        // mesh-local coordinate magnitude. Measured ground truth: across all 5260
        // DLC meshes / 2,106,274 vertices the max |component| is 249.125 (mesh-local
        // — meshes are placed in the world via transforms), while the stride-misread
        // corruption this guards against decodes to >=1e11. The 1e6 bound is ~4000x
        // the real maximum yet far below any corruption, so finite-but-absurd values
        // are caught and real data is never flagged. (FLOAT16 positions are <=65504
        // by construction; the bound chiefly matters for the FLOAT32-position path.)
        const MAX_VERTEX_COORD: f32 = 1.0e6;
        let in_range = |f: f32| f.is_finite() && f.abs() <= MAX_VERTEX_COORD;
        if !in_range(vx) || !in_range(vy) || !in_range(vz) {
            violations += 1;
            if issues.len() < 5 {
                issues.push(format!(
                    "{label}: STRM vertex[{vi}] NaN/Inf/out-of-range (|coord|>1e6): ({vx}, {vy}, {vz})"
                ));
            }
        } else {
            positions.push([vx, vy, vz]);
        }
    }

    (true, violations, Some(vertex_count), positions, issues)
}

/// Per-vertex stride + whether the position element is FLOAT16, parsed from the PC
/// D3DVERTEXELEMENT9 declaration (8-byte header + 8-byte elements + END). The
/// position is treated as FLOAT16 when every listed element is a 2-byte
/// (SHORT/FLOAT16) type — the same gate the converter's `apply_strm_vertex_fix`
/// uses; otherwise FLOAT32. The STRM `decl` child can list only the trailing
/// element, so the stride is the max element end (not a header field).
fn decl_vertex_format(decl: &[u8]) -> (usize, bool, Option<usize>) {
    let mut p = 8usize; // skip the 8-byte PC decl header
    let mut stride = 0usize;
    let mut n = 0usize;
    let mut all_u16 = true;
    let mut first_off: Option<usize> = None;
    while p + 8 <= decl.len() {
        let stream = u16::from_le_bytes([decl[p], decl[p + 1]]);
        let typ = decl[p + 4];
        if stream == 0x00ff || typ == 17 {
            break;
        }
        let offset = u16::from_le_bytes([decl[p + 2], decl[p + 3]]) as usize;
        if first_off.is_none() {
            first_off = Some(offset);
        }
        let sz = pc_decltype_size(typ);
        if offset + sz > stride {
            stride = offset + sz;
        }
        if !matches!(typ, 6 | 7 | 9 | 10 | 11 | 12 | 15 | 16) {
            all_u16 = false;
        }
        n += 1;
        p += 8;
    }
    (stride, n > 0 && all_u16, first_off)
}

/// Whether the FLOAT32 view of the position (offset 0, 3 components) is a sane
/// in-world float3 across the sampled vertices — mirrors the converter's safety net.
/// Used to distinguish a genuine FLOAT32 buffer (whose incomplete decl happens to
/// list only a u16 element) from a scrambled-then-corrected half-float buffer.
fn float3_view_is_sane(data: &[u8], stride: usize) -> bool {
    let n = data.len() / stride;
    let sample = n.min(16);
    if sample == 0 {
        return false;
    }
    let sane = |f: f32| f == 0.0 || (f.is_finite() && f.abs() >= 1e-3 && f.abs() <= 1e6);
    for v in 0..sample {
        let o = v * stride;
        if o + 12 > data.len() {
            return false;
        }
        if !(sane(read_f32_le(data, o))
            && sane(read_f32_le(data, o + 4))
            && sane(read_f32_le(data, o + 8)))
        {
            return false;
        }
    }
    true
}

/// PC D3DDECLTYPE byte sizes (mirrors the converter's `pc_d3ddecltype_size`).
fn pc_decltype_size(t: u8) -> usize {
    match t {
        1 | 7 | 10 | 12 | 16 => 8,
        2 => 12,
        3 => 16,
        _ => 4,
    }
}

/// Decode a little-endian IEEE-754 half-float (FLOAT16) to f32.
fn read_f16_le(d: &[u8], off: usize) -> f32 {
    let h = u16::from_le_bytes([d[off], d[off + 1]]);
    let sign = (h >> 15) & 1;
    let exp = (h >> 10) & 0x1f;
    let frac = (h & 0x3ff) as u32;
    let val = if exp == 0 {
        (frac as f32 / 1024.0) * 2f32.powi(-14)
    } else if exp == 0x1f {
        if frac == 0 {
            f32::INFINITY
        } else {
            f32::NAN
        }
    } else {
        (1.0 + frac as f32 / 1024.0) * 2f32.powi(exp as i32 - 15)
    };
    if sign == 1 {
        -val
    } else {
        val
    }
}

/// Validate PRMG INFO sub-descriptors (60-byte bounding records).
/// Returns (violations, issues).
fn validate_prmg_info(container: &[u8], label: &str) -> (usize, Vec<String>) {
    let children = find_container_children(container, b"PRMG");
    if children.is_empty() {
        return (0, Vec::new());
    }

    let mut violations = 0usize;
    let mut issues = Vec::new();

    for child in &children {
        if &child.tag != b"INFO" {
            continue;
        }
        let body = &container[child.body_start..child.body_start + child.body_size];
        if body.len() < 60 {
            continue;
        }

        let cx = read_f32_le(body, 20);
        let cy = read_f32_le(body, 24);
        let cz = read_f32_le(body, 28);
        let radius = read_f32_le(body, 32);
        let min_x = read_f32_le(body, 36);
        let min_y = read_f32_le(body, 40);
        let min_z = read_f32_le(body, 44);
        let max_x = read_f32_le(body, 48);
        let max_y = read_f32_le(body, 52);
        let max_z = read_f32_le(body, 56);

        let all = [cx, cy, cz, radius, min_x, min_y, min_z, max_x, max_y, max_z];
        if all.iter().any(|f| !f.is_finite()) {
            issues.push(format!("{label}: PRMG INFO contains NaN/Inf"));
            violations += 1;
            continue;
        }

        let mut ok = true;
        if radius <= 0.0 {
            issues.push(format!("{label}: PRMG INFO radius non-positive: {radius}"));
            ok = false;
        }
        if min_x > max_x || min_y > max_y || min_z > max_z {
            issues.push(format!(
                "{label}: PRMG INFO bbox inverted: min=({min_x}, {min_y}, {min_z}) max=({max_x}, {max_y}, {max_z})"
            ));
            ok = false;
        }
        if cx < min_x || cx > max_x || cy < min_y || cy > max_y || cz < min_z || cz > max_z {
            issues.push(format!(
                "{label}: PRMG INFO center ({cx}, {cy}, {cz}) outside bbox"
            ));
            ok = false;
        }
        if !ok {
            violations += 1;
        }
    }

    (violations, issues)
}

/// Validate AREA sub-area containers, mirroring the engine's mesh consumer
/// `Mesh_ConsumeChunk` (0x00478270): the AREA tag (0x41455241) is dispatched at
/// 0x00478366 to the AREA handler `FUN_004a4ab0`.
///
/// What the engine actually does with AREA (decompiled @0x004a4ab0):
///   * Treats AREA as a UCFX container and enters its child descriptor range.
///   * Walks every child; for each child tagged `"info"` (0x6f666e69) it obtains
///     the chunk reader (0x00464780) and reads EXACTLY 4 bytes from the body via
///     the reader vtable (`call [[reader]+0x14]`, count = 4) — a u32 header word.
///   * The read value is otherwise structural (discarded by this handler); there
///     is NO fixed-size array and NO count-driven allocation. AREA is therefore
///     not the heap-overrun hazard MTRL is (MTRL's count@106 overruns a fixed
///     10-slot array — see the MTRL block above). Consequently AREA defects are
///     ADVISORY here (the engine does not crash on them), not fatal.
///
/// Scope note: PC retail vz.wad ships ZERO AREA chunks (verified across the full
/// ~2.5 GB image); AREA only appears in console/DLC source data being ported, so
/// this check guards CONVERTED content. The on-disk descriptor layout is parsed
/// by the validated `find_container_children` walk (the engine's `edi` table is a
/// runtime-parsed form whose field offsets do not map 1:1 to the disk record, so
/// we rely on the disk parser + the engine's semantic contract, not raw offsets).
///
/// Contract checked:
///   * an AREA descriptor that exists but is not a container (no children) is
///     flagged — the engine's handler only ever walks AREA as a container.
///   * each `"info"` child body must be >= 4 bytes (the engine's 4-byte read).
///   * an AREA container carrying no `"info"` child has no payload the handler
///     can consume.
/// Returns (advisory_violations, issues).
fn validate_area(container: &[u8], label: &str) -> (u32, Vec<String>) {
    let children = find_container_children(container, b"AREA");
    if children.is_empty() {
        // No AREA-as-container. If a flat (non-container) AREA descriptor exists,
        // the engine's handler — which only walks AREA as a container — would skip
        // it, leaving dead data; flag that as a converter defect.
        if extract_chunk_body(container, b"AREA").is_some() {
            return (
                1,
                vec![format!(
                    "{label}: AREA descriptor is not a container; engine handler \
                     @0x4a4ab0 only walks AREA as a container (payload unreachable)"
                )],
            );
        }
        return (0, Vec::new());
    }

    let mut advisory = 0u32;
    let mut issues = Vec::new();
    let mut info_children = 0usize;

    for child in &children {
        if &child.tag != b"info" {
            continue;
        }
        info_children += 1;
        if child.body_size < 4 {
            issues.push(format!(
                "{label}: AREA 'info' child body {} bytes < 4 — engine reads a \
                 4-byte header @0x4a4ab0 (truncated/over-read)",
                child.body_size
            ));
            advisory += 1;
        }
    }

    if info_children == 0 {
        issues.push(format!(
            "{label}: AREA container has no 'info' child — engine's AREA handler \
             @0x4a4ab0 has no payload to consume"
        ));
        advisory += 1;
    }

    (advisory, issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal UCFX container from descriptors `(tag, u0, body_size)` plus
    /// a trailing data area. Mirrors the layout `find_container_children` parses:
    /// header[0..4]="UCFX", [4..8]=data_area_off, [16..20]=n_desc, then 20-byte rows.
    fn build_ucfx(descs: &[(&[u8; 4], u32, u32)], data: &[u8]) -> Vec<u8> {
        let n = descs.len();
        let data_area_off = 20 + n * 20;
        let mut buf = vec![0u8; data_area_off];
        buf[0..4].copy_from_slice(b"UCFX");
        buf[4..8].copy_from_slice(&(data_area_off as u32).to_le_bytes());
        buf[16..20].copy_from_slice(&(n as u32).to_le_bytes());
        for (i, (tag, u0, size)) in descs.iter().enumerate() {
            let off = 20 + i * 20;
            buf[off..off + 4].copy_from_slice(*tag);
            buf[off + 4..off + 8].copy_from_slice(&u0.to_le_bytes());
            buf[off + 8..off + 12].copy_from_slice(&size.to_le_bytes());
        }
        buf.extend_from_slice(data);
        buf
    }

    const C: u32 = 0xFFFF_FFFF; // container marker (row_u0)

    #[test]
    fn area_absent_is_clean() {
        let c = build_ucfx(&[(b"GEOM", 0, 4)], &[1, 2, 3, 4]);
        assert_eq!(validate_area(&c, "t").0, 0);
    }

    #[test]
    fn area_container_with_valid_info_is_clean() {
        // AREA container + a 4-byte "info" child (engine reads 4 bytes).
        let c = build_ucfx(&[(b"AREA", C, 0), (b"info", 0, 4)], &[0, 0, 0, 0]);
        let (v, issues) = validate_area(&c, "t");
        assert_eq!(v, 0, "{issues:?}");
    }

    #[test]
    fn area_info_too_small_is_flagged() {
        let c = build_ucfx(&[(b"AREA", C, 0), (b"info", 0, 2)], &[0, 0]);
        let (v, issues) = validate_area(&c, "t");
        assert_eq!(v, 1);
        assert!(issues[0].contains("< 4"), "{issues:?}");
    }

    #[test]
    fn area_container_without_info_is_flagged() {
        let c = build_ucfx(&[(b"AREA", C, 0), (b"data", 0, 8)], &[0; 8]);
        let (v, issues) = validate_area(&c, "t");
        assert_eq!(v, 1);
        assert!(issues[0].contains("no 'info' child"), "{issues:?}");
    }

    /// Build a synthetic multi-material MTRL body: a sequence of records, each
    /// `[104-byte param block][u16 flags][u16 count][count × u32 hash][8-byte tail]`.
    /// The first hash of each record is its diffuse. Returns the body bytes.
    fn build_mtrl(materials: &[(u16, &[u32])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (flags, hashes) in materials {
            body.extend(std::iter::repeat(0u8).take(104)); // param block
            body.extend_from_slice(&flags.to_le_bytes());
            body.extend_from_slice(&(hashes.len() as u16).to_le_bytes());
            for h in *hashes {
                body.extend_from_slice(&h.to_le_bytes());
            }
            body.extend(std::iter::repeat(0u8).take(8)); // tail
        }
        body
    }

    #[test]
    fn mtrl_diffuse_walk_collects_every_material() {
        // 3 materials with counts 1/3/2 — diffuse = first hash of each group.
        let body = build_mtrl(&[
            (0x0080, &[0xAABBCCDD]),
            (0x0080, &[0x11111111, 0x22222222, 0x33333333]),
            (0x0000, &[0x44444444, 0x55555555]),
        ]);
        let diffuse = mtrl_diffuse_hashes(&body);
        assert_eq!(diffuse, vec![0xAABBCCDD, 0x11111111, 0x44444444]);
    }

    #[test]
    fn mtrl_diffuse_walk_skips_null_and_sentinel() {
        let body = build_mtrl(&[
            (0, &[0x00000000]),       // null diffuse — skipped
            (0, &[0xFFFFFFFF]),       // sentinel diffuse — skipped
            (0, &[0xDEADBEEF]),       // kept
        ]);
        assert_eq!(mtrl_diffuse_hashes(&body), vec![0xDEADBEEF]);
    }

    #[test]
    fn mtrl_diffuse_walk_stops_on_bad_count() {
        // A valid first material, then a record whose count word is garbage:
        // the walk must stop, not read past the array.
        let mut body = build_mtrl(&[(0, &[0x12345678])]);
        // Append a bogus param block + an implausible count (0x7FFF) -> stop.
        body.extend(std::iter::repeat(0u8).take(104));
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&0x7FFFu16.to_le_bytes());
        body.extend(std::iter::repeat(0u8).take(8));
        assert_eq!(mtrl_diffuse_hashes(&body), vec![0x12345678]);
    }

    #[test]
    fn flat_area_descriptor_is_flagged() {
        // AREA present as a non-container leaf → engine handler never walks it.
        let c = build_ucfx(&[(b"AREA", 0, 4)], &[0, 0, 0, 0]);
        let (v, issues) = validate_area(&c, "t");
        assert_eq!(v, 1);
        assert!(issues[0].contains("not a container"), "{issues:?}");
    }
}
