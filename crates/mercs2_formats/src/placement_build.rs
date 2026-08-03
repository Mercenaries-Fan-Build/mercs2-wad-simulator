//! Author NEW `SceneObject` world placements into a decompressed placement block
//! (`layers_static` block 29, or a `vz_state` overlay) WITHOUT disturbing any
//! existing entity. Emits one fresh UCFX sub-block carrying `Name` + `ModelName`
//! + `Transform` COMPs (plus the shared `enum`/`schm`/`flgt`/`flgs` scaffolding)
//! for N new entities, and appends it to the block's entry table so the engine
//! enumerates it. Every original sub-block is preserved byte-for-byte.
//!
//! A layer sub-block is a FLAT UCFX container the way `ucfx::walk_decompressed_block`
//! parses it: `[20-byte UCFX header][16 descriptors ×20][data area][8-byte CSUM]`.
//! `header[4]` = `data_area_off` (= **340**: 20-byte header + 16×20 descriptors);
//! every descriptor's `coff` is RELATIVE to that. The 16 descriptors are, in order:
//! `CHDR`, `enum`, then 3 × (`COMP` marker + `info` + `schm` + `data`), then `flgt`,
//! `flgs`. A `COMP` marker row has `coff == 0xFFFFFFFF` (a container, no body).
//!
//! We CLONE the descriptor scaffolding and the shared bodies (`CHDR`, `enum`, each
//! COMP's `info`/`schm`, `flgt`) VERBATIM from a template sub-block that already has
//! exactly `Name`/`ModelName`/`Transform`, and rebuild only the four per-entity
//! bodies:
//!   - `Name`      `data`: `[u32 key][ascii "<name> 0x<key>"\0][0 flag]`        (variable)
//!   - `ModelName` `data`: `[u32 key][u32 model_hash]`                          (8 B)
//!   - `Transform` `data`: `[u32 key][f32 x,y,z][u32 pad][f32 qx,qy,qz,qw][6 B]` (42 B)
//!   - `flgs`            : `[u32 count=N][N × {u32 key, 32-B per-entity state}]` (keyed)
//!
//! `flgt` is a single 4-byte constant hash — NOT keyed — and is copied verbatim.
//! Because we keep all 16 descriptor rows, `data_area_off` stays 340; we repack the
//! data area contiguously (no alignment — retail packs tight, e.g. a 1353-byte
//! odd-length `Name` body butts straight against the next), recompute every leaf's
//! `coff`/`size`, and recompute the trailing CSUM. The container therefore passes
//! `ucfx::walk_decompressed_block` by construction, and the flgs `count` + the three
//! `data` sizes all agree on N. See `docs/placement_data_format.md`.

use crate::crc32::crc32_mercs2;
use crate::ucfx::parse_block_entry_table;

const HDR: usize = 20;

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// A new entity to place: its (unique) entity key, the model asset hash it renders
/// as, its world transform (pos + quat, native game space), and its gameplay name.
#[derive(Debug, Clone)]
pub struct NewEntity {
    pub key: u32,
    pub model_hash: u32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub name: String,
}

/// Build a `Transform` 42-byte record for one entity.
fn transform_record(key: u32, pos: [f32; 3], quat: [f32; 4]) -> Vec<u8> {
    let mut r = Vec::with_capacity(42);
    r.extend_from_slice(&key.to_le_bytes());
    for v in pos {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r.extend_from_slice(&[0u8; 4]); // pad
    for v in quat {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r.extend_from_slice(&[0u8; 6]); // tail
    debug_assert_eq!(r.len(), 42);
    r
}

/// `Name` COMP `data`: one `[u32 key][ascii "<name> 0x<key>"\0][0 flag]` record per
/// entity (mirrors the template's `key + cstring + \0 + \0` layout).
fn name_data(ents: &[NewEntity]) -> Vec<u8> {
    let mut v = Vec::new();
    for e in ents {
        v.extend_from_slice(&e.key.to_le_bytes());
        v.extend_from_slice(format!("{} 0x{:08x}", e.name, e.key).as_bytes());
        v.push(0); // string NUL
        v.push(0); // per-record flag (0 in retail Name COMPs)
    }
    v
}

/// `ModelName` COMP `data`: one `[u32 key][u32 model_hash]` record per entity.
fn modelname_data(ents: &[NewEntity]) -> Vec<u8> {
    let mut v = Vec::with_capacity(ents.len() * 8);
    for e in ents {
        v.extend_from_slice(&e.key.to_le_bytes());
        v.extend_from_slice(&e.model_hash.to_le_bytes());
    }
    v
}

/// `Transform` COMP `data`: one 42-byte record per entity.
fn transform_data(ents: &[NewEntity]) -> Vec<u8> {
    let mut v = Vec::with_capacity(ents.len() * 42);
    for e in ents {
        v.extend_from_slice(&transform_record(e.key, e.pos, e.quat));
    }
    v
}

/// `flgs` body: `[u32 count=N][N × {u32 key, 32-byte per-entity state}]`. The state
/// payload is cloned from the template (identical across every retail record: a
/// `0x00008000` marker, else zero), keeping the new entities' per-entity flags at
/// the same defaults the engine expects.
fn flgs_data(ents: &[NewEntity], payload: &[u8; 32]) -> Vec<u8> {
    let mut v = Vec::with_capacity(4 + ents.len() * 36);
    v.extend_from_slice(&(ents.len() as u32).to_le_bytes());
    for e in ents {
        v.extend_from_slice(&e.key.to_le_bytes());
        v.extend_from_slice(payload);
    }
    v
}

/// One parsed descriptor row from the template's flat table.
struct Desc {
    row: [u8; 20],
    tag: [u8; 4],
    u0: u32,
    size: u32,
}

/// Byte span (offset, len) of the block-entry container at table index `idx`.
fn nth_container(block: &[u8], idx: usize) -> Option<(usize, usize)> {
    let (count, entries) = parse_block_entry_table(block);
    if idx >= count as usize {
        return None;
    }
    let mut off = 4 + count as usize * 16;
    for (i, e) in entries.iter().enumerate() {
        if i == idx {
            return Some((off, e.chunk_size as usize));
        }
        off += e.chunk_size as usize;
    }
    None
}

/// Rebuild a layer sub-block container from the scaffolding of `template`, carrying
/// exactly `ents` (see the module docs). Returns the new container bytes
/// (`UCFX` header … `CSUM`). The three data COMPs and `flgs` are regenerated; every
/// other descriptor body is byte-identical to the template.
fn build_layer_container(template: &[u8], ents: &[NewEntity]) -> Result<Vec<u8>, String> {
    if template.len() < HDR || &template[0..4] != b"UCFX" {
        return Err("template is not a UCFX container".into());
    }
    let daf = rd_u32(template, 4) as usize;
    let ndesc = rd_u32(template, 16) as usize;
    if daf != HDR + ndesc * 20 || daf > template.len() {
        return Err(format!(
            "template descriptor table inconsistent (data_area_off={daf}, ndesc={ndesc})"
        ));
    }

    let mut descs = Vec::with_capacity(ndesc);
    for k in 0..ndesc {
        let ro = HDR + k * 20;
        let mut row = [0u8; 20];
        row.copy_from_slice(&template[ro..ro + 20]);
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&template[ro..ro + 4]);
        descs.push(Desc {
            row,
            tag,
            u0: rd_u32(template, ro + 4),
            size: rd_u32(template, ro + 8),
        });
    }

    // Leaf body slice (relative to the data area). None for container markers.
    let body = |u0: u32, size: u32| -> Option<&[u8]> {
        if u0 == 0xFFFF_FFFF {
            return None;
        }
        let s = daf + u0 as usize;
        template.get(s..s + size as usize)
    };

    // Clone the template's 32-byte per-entity flgs state payload (record = 4-byte
    // key + 32-byte payload, after the u32 count header). Zero if absent/short.
    let flgs_payload: [u8; 32] = {
        let mut p = [0u8; 32];
        if let Some(d) = descs.iter().find(|d| &d.tag == b"flgs") {
            if let Some(b) = body(d.u0, d.size) {
                if b.len() >= 4 + 36 {
                    p.copy_from_slice(&b[8..8 + 32]);
                }
            }
        }
        p
    };

    let mut have_comps = (false, false, false); // Name, ModelName, Transform seen
    let mut data_area: Vec<u8> = Vec::new();
    let mut new_rows: Vec<[u8; 20]> = Vec::with_capacity(ndesc);
    let mut current_comp: Option<String> = None;

    for d in &descs {
        let mut row = d.row;
        if d.u0 == 0xFFFF_FFFF {
            // COMP container marker — copy verbatim; its `info` child sets the type.
            current_comp = None;
            new_rows.push(row);
            continue;
        }
        let verbatim = || {
            body(d.u0, d.size)
                .map(|b| b.to_vec())
                .ok_or_else(|| format!("template leaf {:?} body out of bounds", d.tag))
        };
        let new_body: Vec<u8> = match &d.tag {
            b"data" => match current_comp.as_deref() {
                Some("Name") => {
                    have_comps.0 = true;
                    name_data(ents)
                }
                Some("ModelName") => {
                    have_comps.1 = true;
                    modelname_data(ents)
                }
                Some("Transform") => {
                    have_comps.2 = true;
                    transform_data(ents)
                }
                // Any other keyed COMP the template might carry: keep it verbatim
                // (its keys stay the template's; this writer only authors the three).
                _ => verbatim()?,
            },
            b"flgs" => flgs_data(ents, &flgs_payload),
            b"info" => {
                let mut b = verbatim()?;
                let n = b.iter().position(|&x| x == 0).unwrap_or(b.len());
                if n > 0 {
                    let name = String::from_utf8_lossy(&b[..n]).into_owned();
                    // The engine sizes entity REGISTRATION off this COMP's `info`
                    // record-count (the 3rd u32 after the name's NUL, at n+1+8), NOT the
                    // data-body size. For every COMP whose `data` we regenerate it must
                    // equal N, or the engine registers only the template's count of
                    // entities and the regenerated `flgs` then walks past the last
                    // registered key -> FUN_00649c00 miss -> NULL -> AV @0x00655163.
                    // (ucfx-check / load_model_placements read count from data size, so
                    // this mismatch is invisible to them — engine vs parser disagree.)
                    if matches!(name.as_str(), "Name" | "ModelName" | "Transform") {
                        let coff = n + 1 + 8;
                        if coff + 4 <= b.len() {
                            b[coff..coff + 4].copy_from_slice(&(ents.len() as u32).to_le_bytes());
                        }
                    }
                    current_comp = Some(name);
                }
                b
            }
            // CHDR, enum, schm, flgt: shared scaffolding, copied verbatim.
            _ => verbatim()?,
        };
        let coff = data_area.len() as u32;
        data_area.extend_from_slice(&new_body);
        row[4..8].copy_from_slice(&coff.to_le_bytes());
        row[8..12].copy_from_slice(&(new_body.len() as u32).to_le_bytes());
        new_rows.push(row);
    }

    if have_comps != (true, true, true) {
        return Err(format!(
            "template is missing a Name/ModelName/Transform COMP (found {have_comps:?})"
        ));
    }

    // Assemble: UCFX header (verbatim — data_area_off & ndesc unchanged) + the
    // recomputed descriptor rows + the repacked data area + a fresh CSUM.
    let mut out = Vec::with_capacity(daf + data_area.len() + 8);
    out.extend_from_slice(&template[0..HDR]);
    for row in &new_rows {
        out.extend_from_slice(row);
    }
    debug_assert_eq!(out.len(), daf, "data area must start at data_area_off");
    out.extend_from_slice(&data_area);
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());
    Ok(out)
}

/// Append a fresh layer sub-block carrying `ents` to a decompressed placement block,
/// cloning the COMP scaffolding of the `template_sub`-th block-entry container (one
/// that has `Name`+`ModelName`+`Transform`, e.g. `layers_static` sub-block 15).
/// Returns the new block bytes.
///
/// `layer_hash` is the appended sub-block's **entry-table name** — the u32 that every
/// existing sub-block sets equal to its own layer ASET `asset_hash` (verified: block-29
/// entries 0/1/2 = 0xB41FC710/0xF52AD72F/0x6FF3D556). The retail engine loads a layer
/// BY NAME-HASH through the asset system (`Pg.LoadLayer`/`LoadAsset("layer", nameHash)`
/// → `FUN_0045E440(m2("layer")=0xE6B81A54, nameHash)` → the layer record), so this must
/// be the same `H` a matching ASET row advertises, or nothing can ever resolve to it.
///
/// The block is `[u32 count][count × 16-byte entries {name,type,field_c,size}][data
/// = the sub-blocks concatenated in entry order]`. We add one LAYER entry
/// (type `0xE6B81A54`, `size` = our container length) and bump the count. Growing
/// the table by 16 bytes shifts every sub-block, but each sub-block is located by
/// cumulative `size` and its child offsets are data-area-relative, so the shift is
/// transparent.
pub fn append_placements(
    block: &[u8],
    template_sub: usize,
    ents: &[NewEntity],
    layer_hash: u32,
) -> Result<Vec<u8>, String> {
    if ents.is_empty() {
        return Err("no entities to place".into());
    }
    let (t_off, t_len) =
        nth_container(block, template_sub).ok_or("template sub-block out of range")?;
    let template = block
        .get(t_off..t_off + t_len)
        .ok_or("template span out of range")?;
    let container = build_layer_container(template, ents)?;

    const LAYER_TYPE: u32 = 0xE6B8_1A54;
    let count = rd_u32(block, 0);
    let table_end = 4 + count as usize * 16;
    let name_hash = layer_hash; // entry-table name = the layer's ASET asset hash H

    let mut result = Vec::with_capacity(block.len() + 16 + container.len());
    result.extend_from_slice(&(count + 1).to_le_bytes()); // bumped sub-block count
    result.extend_from_slice(&block[4..table_end]); // original entries
    result.extend_from_slice(&name_hash.to_le_bytes());
    result.extend_from_slice(&LAYER_TYPE.to_le_bytes());
    result.extend_from_slice(&0u32.to_le_bytes());
    result.extend_from_slice(&(container.len() as u32).to_le_bytes());
    result.extend_from_slice(&block[table_end..]); // original sub-block data
    result.extend_from_slice(&container); // our new sub-block (matches the appended entry)
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placement::load_model_placements;
    use crate::ucfx::verify_ucfx_container;

    /// Build a minimal-but-valid layer template container with the exact 16-descriptor
    /// shape the real `layers_static` sub-block has: `CHDR, enum, 3×(COMP+info+schm+data),
    /// flgt, flgs`. Bodies are placeholders (the writer regenerates the data COMPs +
    /// flgs); the template's flgs carries a 1-entity `[count][key][32-B payload]` so the
    /// payload clone path is exercised.
    fn synthetic_template() -> Vec<u8> {
        // (tag, body, is_container_marker, f12, f16)
        // Realistic `info` body: name + NUL + 4 trailing u32s; the 3rd (at name_len+1+8)
        // is the per-COMP record COUNT the engine sizes registration off (template = 1).
        let info_body = |name: &str| -> Vec<u8> {
            let mut b = name.as_bytes().to_vec();
            b.push(0);
            b.extend_from_slice(&[0u8; 8]); // u32[0], u32[1]
            b.extend_from_slice(&1u32.to_le_bytes()); // u32[2] = record count
            b.extend_from_slice(&[0u8; 4]); // u32[3]
            b
        };
        let name_info = info_body("Name");
        let mn_info = info_body("ModelName");
        let tf_info = info_body("Transform");
        let mut flgs_body = Vec::new();
        flgs_body.extend_from_slice(&1u32.to_le_bytes()); // count
        flgs_body.extend_from_slice(&0xAAAA_AAAAu32.to_le_bytes()); // template key
        flgs_body.extend_from_slice(&[0u8; 16]);
        flgs_body.extend_from_slice(&0x0000_8000u32.to_le_bytes()); // marker
        flgs_body.extend_from_slice(&[0u8; 12]);
        assert_eq!(flgs_body.len(), 4 + 36);

        let rows: Vec<([u8; 4], Vec<u8>, bool, u32, u32)> = vec![
            (*b"CHDR", vec![0u8; 8], false, 6, 0),
            (*b"enum", vec![7u8; 8], false, 5, 0),
            (*b"COMP", vec![], true, 4, 3),
            (*b"info", name_info, false, 2, 0),
            (*b"schm", vec![1u8; 4], false, 1, 0),
            (*b"data", vec![9u8; 6], false, 0, 0), // Name (regenerated)
            (*b"COMP", vec![], true, 3, 3),
            (*b"info", mn_info, false, 2, 0),
            (*b"schm", vec![2u8; 4], false, 1, 0),
            (*b"data", vec![0u8; 8], false, 0, 0), // ModelName (regenerated)
            (*b"COMP", vec![], true, 2, 3),
            (*b"info", tf_info, false, 2, 0),
            (*b"schm", vec![3u8; 4], false, 1, 0),
            (*b"data", vec![0u8; 42], false, 0, 0), // Transform (regenerated)
            (*b"flgt", 0xE9DA_BB4Au32.to_le_bytes().to_vec(), false, 1, 0),
            (*b"flgs", flgs_body, false, 0, 0),
        ];
        let ndesc = rows.len();
        let daf = HDR + ndesc * 20;

        let mut desc_tbl = Vec::new();
        let mut data_area = Vec::new();
        for (tag, body, marker, f12, f16) in &rows {
            desc_tbl.extend_from_slice(tag);
            if *marker {
                desc_tbl.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // coff = marker
                desc_tbl.extend_from_slice(&0u32.to_le_bytes()); // size
            } else {
                desc_tbl.extend_from_slice(&(data_area.len() as u32).to_le_bytes());
                desc_tbl.extend_from_slice(&(body.len() as u32).to_le_bytes());
                data_area.extend_from_slice(body);
            }
            desc_tbl.extend_from_slice(&f12.to_le_bytes());
            desc_tbl.extend_from_slice(&f16.to_le_bytes());
        }

        let mut c = Vec::new();
        c.extend_from_slice(b"UCFX");
        c.extend_from_slice(&(daf as u32).to_le_bytes()); // data_area_off
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&(ndesc as u32).to_le_bytes());
        c.extend_from_slice(&desc_tbl);
        c.extend_from_slice(&data_area);
        let csum = crc32_mercs2(&c);
        c.extend_from_slice(b"CSUM");
        c.extend_from_slice(&csum.to_le_bytes());
        c
    }

    /// The rebuilt container passes the gate (`verify_ucfx_container` = no issues) and
    /// re-parses to exactly the authored entities — the writer's success criterion.
    #[test]
    fn rebuilt_layer_is_gate_valid_and_reparses() {
        let template = synthetic_template();
        // The template itself is a valid single-entity container.
        assert!(
            verify_ucfx_container(&template, "template", 0xE6B8_1A54).is_none(),
            "synthetic template must itself be gate-valid"
        );

        let ents = vec![
            NewEntity {
                key: 0x00F0_0002,
                model_hash: 0x9F2B_CEBE,
                pos: [10.0, -2.0, 30.0],
                quat: [0.0, 0.0, 0.0, 1.0],
                name: "propA".into(),
            },
            NewEntity {
                key: 0x00F0_0003,
                model_hash: 0x9F2B_CEBE,
                pos: [11.0, -2.0, 31.0],
                quat: [0.0, 0.0, 0.0, 1.0],
                name: "propB".into(),
            },
        ];
        let out = build_layer_container(&template, &ents).expect("build");

        // Gate: no "exceeds container" / CSUM issues.
        let issues = verify_ucfx_container(&out, "rebuilt", 0xE6B8_1A54);
        assert!(issues.is_none(), "gate issues: {issues:?}");

        // data_area_off unchanged (16 descriptors), CSUM trailer present.
        assert_eq!(rd_u32(&out, 4) as usize, HDR + 16 * 20);
        assert_eq!(&out[out.len() - 8..out.len() - 4], b"CSUM");

        // Re-parse: both entities present with the right model + position.
        let placed = load_model_placements(&out);
        assert_eq!(placed.len(), 2, "both authored entities must re-parse");
        for e in &ents {
            let p = placed
                .iter()
                .find(|p| p.key == e.key)
                .unwrap_or_else(|| panic!("key {:08X} missing", e.key));
            assert_eq!(p.model_hash, e.model_hash);
            assert_eq!(p.pos, e.pos);
        }

        // ★Regression guard for the AV @0x00655163 (2026-08-02): the ENGINE sizes
        // entity registration off each COMP's `info` record-count (3rd u32 after the
        // name NUL), NOT the data-body size. If a regenerated COMP's info keeps the
        // template's count while data/flgs grow to N, the engine registers too few
        // entities and the flgs walk NULL-derefs. ucfx-check / load_model_placements
        // read count from data size, so ONLY this explicit check catches the mismatch.
        let daf = rd_u32(&out, 4) as usize;
        let ndesc = rd_u32(&out, 16) as usize;
        let mut info_seen = 0usize;
        for k in 0..ndesc {
            let ro = 20 + k * 20;
            if &out[ro..ro + 4] == b"info" {
                let coff = rd_u32(&out, ro + 4) as usize;
                let body = &out[daf + coff..];
                let nl = body.iter().position(|&b| b == 0).unwrap();
                assert_eq!(
                    rd_u32(body, nl + 1 + 8) as usize,
                    ents.len(),
                    "info.count must equal N — the engine's registration size"
                );
                info_seen += 1;
            }
        }
        assert_eq!(info_seen, 3, "expected Name/ModelName/Transform info descriptors");
    }
}

/// Single-entity convenience wrapper over [`append_placements`]. `layer_hash` is the
/// appended sub-block's entry-table name (the layer's ASET `asset_hash` H).
#[allow(clippy::too_many_arguments)]
pub fn append_placement(
    block: &[u8],
    template_sub: usize,
    key: u32,
    name: &str,
    model_hash: u32,
    pos: [f32; 3],
    quat: [f32; 4],
    layer_hash: u32,
) -> Result<Vec<u8>, String> {
    append_placements(
        block,
        template_sub,
        &[NewEntity {
            key,
            model_hash,
            pos,
            quat,
            name: name.to_string(),
        }],
        layer_hash,
    )
}
