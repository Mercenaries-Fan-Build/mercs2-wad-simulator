//! Author a NEW `SceneObject` world placement into a decompressed placement block
//! (`layers_static` block 29, or a `vz_state` overlay), WITHOUT overriding any
//! existing entity. Appends one fresh UCFX sub-block carrying a `Name` +
//! `ModelName` + `Transform` COMP for a single new entity (Transform pos+quat,
//! ModelName→our model hash). The engine streams it like any other prop.
//!
//! Nothing existing is disturbed: every original sub-block is preserved
//! byte-for-byte; we only concatenate one more UCFX sub-block. The COMP/CHDR
//! container + `schm` schema layout is the world format's scaffolding (entity-
//! count-independent); the entity's records are authored fresh. Model authoring
//! is fully from-scratch (see `model_build`).
//!
//! The 3-COMP structure is taken from a TEMPLATE sub-block already present in the
//! block (one that has exactly Name+ModelName+Transform, e.g. layers_static
//! sub-block 15) so the exact `enum/flgt/flgs/schm/info/CHDR` bytes the engine
//! accepts are preserved; only the three `data` children are rewritten to one
//! record and offsets recomputed.

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

const HDR: usize = 20;

/// Locate the byte spans of every `UCFX` sub-block in a decompressed block.
fn ucfx_subblocks(block: &[u8]) -> Vec<(usize, usize)> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 4 <= block.len() {
        if &block[i..i + 4] == b"UCFX" {
            starts.push(i);
            i += 4;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (k, &s) in starts.iter().enumerate() {
        let e = if k + 1 < starts.len() { starts[k + 1] } else { block.len() };
        out.push((s, e));
    }
    out
}

/// A parsed CHDR row (COMP/enum/flgt/flgs): its 20-byte header + child descriptors.
struct Row {
    hdr: [u8; 20],
    tag: [u8; 4],
    children: Vec<([u8; 4], u32, u32, u32, u32)>, // (ctag, coff, csz, u3, u4)
}

/// Parse one sub-block's CHDR table into rows + the data-area start (relative to
/// the sub-block start `s`). Child `coff` are relative to that data-area start.
fn parse_sub(block: &[u8], s: usize, e: usize) -> Option<(usize, Vec<Row>, usize)> {
    // CHDR follows the UCFX header.
    let ci = block[s..e.min(block.len())].windows(4).position(|w| w == b"CHDR")?;
    let chdr = s + ci;
    if chdr + HDR > block.len() {
        return None;
    }
    let entries = rd_u32(block, chdr + 12) as usize;
    let mut p = chdr + HDR;
    let mut rows = Vec::new();
    for _ in 0..entries {
        if p + HDR > e {
            break;
        }
        let tag = &block[p..p + 4];
        if tag != b"COMP" && tag != b"enum" && tag != b"flgt" && tag != b"flgs" {
            break;
        }
        let nch = rd_u32(block, p + 16) as usize;
        let mut hdr = [0u8; 20];
        hdr.copy_from_slice(&block[p..p + 20]);
        let mut children = Vec::with_capacity(nch);
        let mut cp = p + HDR;
        for _ in 0..nch {
            if cp + HDR > e {
                break;
            }
            let mut ct = [0u8; 4];
            ct.copy_from_slice(&block[cp..cp + 4]);
            children.push((ct, rd_u32(block, cp + 4), rd_u32(block, cp + 8), rd_u32(block, cp + 12), rd_u32(block, cp + 16)));
            cp += HDR;
        }
        let mut t = [0u8; 4];
        t.copy_from_slice(&block[p..p + 4]);
        rows.push(Row { hdr, tag: t, children });
        p = cp;
    }
    let data_area_start = p; // absolute
    Some((chdr, rows, data_area_start))
}

fn comp_type_name(block: &[u8], das: usize, row: &Row) -> Option<String> {
    for &(ct, coff, csz, _, _) in &row.children {
        if &ct == b"info" {
            let a = das + coff as usize;
            let raw = block.get(a..a + csz as usize)?;
            let n = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            if n > 0 {
                return Some(String::from_utf8_lossy(&raw[..n]).into_owned());
            }
        }
    }
    None
}

/// Build a Transform 42-byte record for one entity.
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

/// Append a new `SceneObject` placement (Name+ModelName+Transform) to `block`,
/// cloning the COMP structure of `template_sub` (a sub-block index that has all
/// three COMPs). Returns the new block bytes.
pub fn append_placement(
    block: &[u8],
    template_sub: usize,
    key: u32,
    name: &str,
    model_hash: u32,
    pos: [f32; 3],
    quat: [f32; 4],
) -> Result<Vec<u8>, String> {
    let subs = ucfx_subblocks(block);
    let &(s, e) = subs.get(template_sub).ok_or("template sub-block out of range")?;
    let (_chdr, rows, das) = parse_sub(block, s, e).ok_or("template CHDR parse failed")?;

    // The authored per-COMP data bodies for our single entity.
    let name_rec = {
        let mut r = Vec::new();
        r.extend_from_slice(&key.to_le_bytes());
        r.extend_from_slice(format!("{name} 0x{key:08X}").as_bytes());
        r.push(0); // string NUL
        r.push(0); // per-record flag
        r
    };
    let modelname_rec = {
        let mut r = Vec::new();
        r.extend_from_slice(&key.to_le_bytes());
        r.extend_from_slice(&model_hash.to_le_bytes());
        r
    };
    let transform_rec = transform_record(key, pos, quat);

    // Re-emit the sub-block: keep the UCFX header + CHDR header + every row header,
    // rewrite each COMP's `data` child body (Name/ModelName/Transform → 1 record),
    // keep `info`/`schm` verbatim, and lay all child bodies into a fresh data area
    // with recomputed `coff`.
    let ucfx_size_field_pos = s + 4;
    let orig_ucfx_size = rd_u32(block, ucfx_size_field_pos);

    // Collect the new child bodies in emission order, assign new coff.
    let mut new_data: Vec<u8> = Vec::new();
    let mut new_rows: Vec<(Row, Vec<Option<u32>>)> = Vec::new(); // (row, per-child new coff)

    for row in &rows {
        let tname = if &row.tag == b"COMP" { comp_type_name(block, das, row) } else { None };
        let mut new_coffs: Vec<Option<u32>> = Vec::with_capacity(row.children.len());
        for &(ct, coff, csz, _u3, _u4) in &row.children {
            // choose the body: rewritten data for our COMPs, else verbatim.
            let body: Vec<u8> = if &ct == b"data" {
                match tname.as_deref() {
                    Some("Name") => name_rec.clone(),
                    Some("ModelName") => modelname_rec.clone(),
                    Some("Transform") => transform_rec.clone(),
                    _ => block[das + coff as usize..das + coff as usize + csz as usize].to_vec(),
                }
            } else {
                block[das + coff as usize..das + coff as usize + csz as usize].to_vec()
            };
            // align child body to 4 bytes within the data area.
            while new_data.len() % 4 != 0 {
                new_data.push(0);
            }
            let nc = new_data.len() as u32;
            new_data.extend_from_slice(&body);
            new_coffs.push(Some(nc));
        }
        // clone row (with possibly-updated child sizes)
        let mut nr = Row { hdr: row.hdr, tag: row.tag, children: row.children.clone() };
        // rewrite child csz for the rewritten data children
        for (i, (ct, _coff, csz, u3, u4)) in row.children.iter().enumerate() {
            let new_sz = if &ct[..] == b"data" {
                match tname.as_deref() {
                    Some("Name") => name_rec.len() as u32,
                    Some("ModelName") => modelname_rec.len() as u32,
                    Some("Transform") => transform_rec.len() as u32,
                    _ => *csz,
                }
            } else {
                *csz
            };
            nr.children[i] = (*ct, new_coffs[i].unwrap(), new_sz, *u3, *u4);
        }
        new_rows.push((nr, new_coffs));
    }

    // Serialize the new sub-block.
    // Header: UCFX + size + (copy the bytes between +8 and CHDR verbatim, i.e. the
    // sub-block preamble up to the CHDR chunk).
    let chdr_abs = {
        let ci = block[s..e].windows(4).position(|w| w == b"CHDR").unwrap();
        s + ci
    };
    let mut out = Vec::new();
    // preamble = [s .. chdr_abs) verbatim (UCFX magic + size field + whatever precedes CHDR)
    out.extend_from_slice(&block[s..chdr_abs]);
    let chdr_hdr = &block[chdr_abs..chdr_abs + HDR];
    out.extend_from_slice(chdr_hdr); // CHDR 20-byte header (entries count unchanged)
    // rows: header (20B) + children descriptors (20B each, with new coff/csz)
    for (nr, _) in &new_rows {
        out.extend_from_slice(&nr.hdr);
        for &(ct, coff, csz, u3, u4) in &nr.children {
            out.extend_from_slice(&ct);
            out.extend_from_slice(&coff.to_le_bytes());
            out.extend_from_slice(&csz.to_le_bytes());
            out.extend_from_slice(&u3.to_le_bytes());
            out.extend_from_slice(&u4.to_le_bytes());
        }
    }
    // data area
    out.extend_from_slice(&new_data);

    // Patch the UCFX size field: original size covered (up to) the sub-block; set
    // it to our new total minus the 8-byte UCFX+size header (mirror the source: the
    // field was orig_ucfx_size for the template). Use new content length from +8.
    let new_ucfx_size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&new_ucfx_size.to_le_bytes());
    let _ = orig_ucfx_size;

    // The block is [u32 count][count × 16-byte entries {name,type,field_c,size}][data =
    // the sub-blocks concatenated in entry order]. The ENGINE iterates the entry table —
    // it does NOT scan for "UCFX" — so a sub-block appended without an entry is invisible
    // (our reader scans UCFX, which is why it round-tripped but never rendered). Add a
    // new LAYER entry (type 0xE6B81A54, the same as every other placement sub-block) for
    // our sub-block and bump the count. Entry `size` = our sub-block byte length; the
    // engine locates sub-blocks by cumulative size, and each sub-block's CHDR child
    // offsets are relative to its own data-area, so growing the entry table by 16 bytes
    // (shifting all sub-blocks) is transparent.
    const LAYER_TYPE: u32 = 0xE6B8_1A54;
    let count = rd_u32(block, 0);
    let table_end = 4 + count as usize * 16;
    let mut result = Vec::with_capacity(block.len() + 16 + out.len());
    result.extend_from_slice(&(count + 1).to_le_bytes()); // bumped sub-block count
    result.extend_from_slice(&block[4..table_end]); // original entries
    // new entry: name = our entity key (unique), type = LAYER, field_c = 0, size = sub-block len
    result.extend_from_slice(&key.to_le_bytes());
    result.extend_from_slice(&LAYER_TYPE.to_le_bytes());
    result.extend_from_slice(&0u32.to_le_bytes());
    result.extend_from_slice(&(out.len() as u32).to_le_bytes());
    result.extend_from_slice(&block[table_end..]); // original sub-block data
    result.extend_from_slice(&out); // our new sub-block (matches the appended entry)
    Ok(result)
}
