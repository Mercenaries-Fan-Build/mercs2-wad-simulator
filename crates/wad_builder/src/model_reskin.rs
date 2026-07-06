//! Re-rig Sarah's two STATIC `MESH` eye slots into SKINNED groups, so the PC
//! skinned-mesh consumer (FUN_004796f0) loads them like every other group — keeping
//! all 17 GEOM slots (NO slot removal, which kept mis-aligning chunks).
//!
//! Each static eye sub-mesh is `MESH → INFO(4) → [PRMG → INFO(60 PgMesh) →
//! STRM{info,decl(24: POS/TEX/usage10/NORMAL),data} → AREA → IBUF → PRMT] × 2`.
//! Obama's skinned eyes are `SKIN → INFO(4) → [PRMG → INFO(56 PgSkin) →
//! STRM{info,decl(32: POS/TEX/usage10/BLENDIDX/BLENDWEIGHT/NORMAL),data} → IBUF →
//! PRMT] × 2` — blendidx is a DIRECT HIER bone index, weight 0xFF (=1.0).
//!
//! Conversion per eye MESH slot:
//!   1. retag `MESH` → `SKIN`;
//!   2. drop the `AREA` chunk-group inside each inner PRMG;
//!   3. rewrite each inner PRMG's INFO(60 PgMesh) → INFO(56 PgSkin) (obama byte
//!      template, shaders PgSkinNoTangentVP/PgSkinShadowVP);
//!   4. re-encode each STRM: decl 24→32 (insert BLENDINDICES@16 + BLENDWEIGHT@20),
//!      data: per vertex copy POS(8)+TEX(4)+usage10(4), insert blendidx=[bone,0,0,0]
//!      + weight=[0xFF,0,0,0], then NORMAL(8); update STRM info stride 24→32;
//!   5. fix sibling spans (the AREA removal shortens each PRMG by 3 rows).
//! The eye verts stay in their authored (eyeball-bone-local) space and bind to the
//! eyeball bone the model's SEGM already designated for that slot.
//!
//! Rebuilds the body region + u0 + data_base + n_desc and recomputes CSUM.

use mercs2_formats::crc32::crc32_mercs2;

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

#[derive(Clone)]
struct Desc {
    tag: [u8; 4],
    u0: u32,
    size: u32,
    u3: u32,
    u4: u32,
    body: Vec<u8>,
}

const MESH: &[u8; 4] = b"MESH";
const SKIN: &[u8; 4] = b"SKIN";
const AREA: &[u8; 4] = b"AREA";
const PRMG: &[u8; 4] = b"PRMG";
const INFO: &[u8; 4] = b"INFO";
const STRM: &[u8; 4] = b"STRM";
const DECL: &[u8; 4] = b"decl";
const INFOL: &[u8; 4] = b"info";
const DATA: &[u8; 4] = b"data";

/// One decl element: stream, offset, type, usage. (8-byte on-disk row:
/// `[u16 stream][u16 offset][u8 type][u8 pad][u8 usage][u8 pad]`.)
#[derive(Clone, Copy)]
struct DeclElem {
    stream: u16,
    offset: u16,
    typ: u8,
    usage: u8,
}

fn decl_type_size(typ: u8) -> usize {
    match typ {
        16 => 8, // FLOAT16_4
        15 => 4, // FLOAT16_2
        5 => 4,  // UBYTE4 (also D3DCOLOR path here as 4)
        8 => 4,  // UBYTE4N
        4 => 4,  // D3DCOLOR
        _ => 4,
    }
}

fn parse_decl(b: &[u8]) -> Vec<DeclElem> {
    let mut v = Vec::new();
    for e in 0..b.len() / 8 {
        let stream = u16::from_le_bytes([b[e * 8], b[e * 8 + 1]]);
        let offset = u16::from_le_bytes([b[e * 8 + 2], b[e * 8 + 3]]);
        let typ = b[e * 8 + 4];
        let usage = b[e * 8 + 6];
        v.push(DeclElem { stream, offset, typ, usage });
    }
    v
}

/// Serialize a decl element list (re-laying offsets contiguously from 0, stream 0).
/// Appends the 0xFF terminator row. Returns (decl_bytes, stride).
fn build_decl(elems: &[DeclElem]) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut off = 0usize;
    for e in elems {
        out.extend_from_slice(&0u16.to_le_bytes()); // stream 0
        out.extend_from_slice(&(off as u16).to_le_bytes());
        out.push(e.typ);
        out.push(0);
        out.push(e.usage);
        out.push(0);
        off += decl_type_size(e.typ);
    }
    // terminator
    out.extend_from_slice(&[0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00]);
    (out, off)
}

fn find_child(descs: &[Desc], start: usize, end: usize, tag: &[u8; 4]) -> Option<usize> {
    (start..=end.min(descs.len() - 1)).find(|&i| &descs[i].tag == tag)
}

/// Re-encode one eye STRM group's chunks in place within `descs`:
///   - PRMG INFO(60)→INFO(56 skin template) using `skin_info_template`,
///   - STRM info stride 24→32, decl→SKIN_DECL_32, data 24-stride→32-stride with bind.
/// `bone` is the HIER bone index to bind every vertex to.
/// Decode a little-endian IEEE half-float (f16) at the start of `b`.
fn decode_f16(b: &[u8]) -> f32 {
    half_to_f32(u16::from_le_bytes([b[0], b[1]]))
}
fn encode_f16(x: f32) -> [u8; 2] {
    f32_to_half(x).to_le_bytes()
}
fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let mant = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign << 31
        } else {
            // subnormal
            let mut e = -1i32;
            let mut m = mant;
            while (m & 0x400) == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3ff;
            (sign << 31) | (((127 - 15 + e + 1) as u32) << 23) | (m << 13)
        }
    } else if exp == 0x1f {
        (sign << 31) | (0xff << 23) | (mant << 13)
    } else {
        (sign << 31) | (((exp as i32 - 15 + 127) as u32) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}
fn f32_to_half(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7fffff;
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let mant = (mant | 0x800000) >> (1 - exp);
        // round to nearest
        let m = ((mant + 0x1000) >> 13) as u16;
        sign | m
    } else if exp >= 0x1f {
        sign | 0x7c00 // inf/nan
    } else {
        let m = ((mant + 0x1000) >> 13) as u16;
        // handle mantissa overflow into exponent
        sign | (((exp as u16) << 10) + m)
    }
}

fn reskin_prmg(
    descs: &mut [Desc],
    pi: usize,
    info_bone: u8,
    translation: [f32; 3],
    skin_info_template: &[u8; 56],
) -> Result<(), String> {
    let span = descs[pi].u4 as usize;
    let end = pi + span;
    // Pre-scan the source STRM decl to learn whether this sub-mesh has a TANGENT
    // channel (selects the vertex-shader pair in the INFO below).
    let strm_pre = find_child(descs, pi + 1, end, STRM).ok_or("PRMG: no STRM")?;
    let strm_pre_end = strm_pre + descs[strm_pre].u4 as usize;
    let decl_pre = find_child(descs, strm_pre + 1, strm_pre_end, DECL).ok_or("STRM: no decl")?;
    let src_decl_has_tangent = parse_decl(&descs[decl_pre].body)
        .iter()
        .any(|e| e.stream != 255 && e.usage == 6);

    // PRMG INFO(56 expected after swap; currently the 60-byte PgMesh INFO).
    let info_i = find_child(descs, pi + 1, end, INFO).ok_or("PRMG: no INFO")?;
    // Use obama's skinned eye-group INFO(56) VERBATIM. dwords [7,10..14] are runtime
    // cache pointers baked into obama's on-disk block from a prior load (0x106e… heap
    // addresses); the engine overwrites them at instantiation. Obama ships + loads with
    // exactly these bytes, so to be byte-structurally identical we copy them verbatim
    // (NOT zeroed — zeroing them left the runtime object malformed). The per-eye
    // material is selected by PRMT, not this INFO.
    //
    // The ONLY per-sub-mesh variation is the vertex-shader pair (INFO dwords [3]=VP,
    // [4]=shadow-VP), which obama keys to the vertex format: the iris (no TANGENT) uses
    // PgSkinNoTangentVP/PgSkinShadowVP, the reflection (has TANGENT) uses
    // PgSkinNoColorVP/PgSkinTexShadowVP — exactly matching obama's two eye sub-meshes.
    let has_tangent = src_decl_has_tangent;
    let mut ni = skin_info_template.to_vec();
    if has_tangent {
        ni[12..16].copy_from_slice(&0xb2677dd7u32.to_le_bytes()); // PgSkinNoColorVP
        ni[16..20].copy_from_slice(&0xdd3622c2u32.to_le_bytes()); // PgSkinTexShadowVP
    } // else: template already carries PgSkinNoTangentVP/PgSkinShadowVP.

    // INFO dword[6] = `(hi16 = bone-palette size) | (lo16 = BASE bone index)`. This is
    // the field that drives the eye's skinning DRAWABLE: obama's eye groups carry
    // 0x10026 (size 1, base bone 38 = obama's bone_eyeball_right) and bind via
    // blendidx==0 (palette[0]). The obama template's lo16=38 is WRONG for any other
    // skeleton (Sarah's bone 38 is a mouth bone), which built a garbage drawable →
    // 0x6B6FDA vtable crash. Set lo16 to THIS slot's actual eyeball bone (keeping
    // obama's hi16=1 single-bone palette).
    ni[24..28].copy_from_slice(&((1u32 << 16) | info_bone as u32).to_le_bytes());

    descs[info_i].body = ni;
    descs[info_i].size = 56;

    // STRM container, then its info(12)/decl/data leaves.
    let strm_i = find_child(descs, pi + 1, end, STRM).ok_or("PRMG: no STRM")?;
    let strm_end = strm_i + descs[strm_i].u4 as usize;
    let sinfo = find_child(descs, strm_i + 1, strm_end, INFOL).ok_or("STRM: no info")?;
    let sdecl = find_child(descs, strm_i + 1, strm_end, DECL).ok_or("STRM: no decl")?;
    let sdata = find_child(descs, strm_i + 1, strm_end, DATA).ok_or("STRM: no data")?;

    // STRM info(12) = [field0, stride, vcount].
    if descs[sinfo].body.len() != 12 {
        return Err("STRM info not 12 bytes".into());
    }
    let field0 = rd_u32(&descs[sinfo].body, 0);
    let old_stride = rd_u32(&descs[sinfo].body, 4) as usize;
    let vcount = rd_u32(&descs[sinfo].body, 8) as usize;

    // Parse the source (static) decl and find the NORMAL element. We insert
    // BLENDINDICES(UBYTE4) + BLENDWEIGHT(UBYTE4N) immediately BEFORE NORMAL (obama's
    // convention for both eye sub-mesh variants). Already-skinned decls are left as-is.
    let src_elems = parse_decl(&descs[sdecl].body);
    if src_elems.iter().any(|e| e.stream != 255 && (e.usage == 1 || e.usage == 2)) {
        return Err("STRM already has blend channels".into());
    }
    let normal_pos = src_elems
        .iter()
        .position(|e| e.stream != 255 && e.usage == 3)
        .ok_or("STRM decl has no NORMAL element")?;

    // Build the new element list: copy through, inserting BLENDIDX+BLENDWT before NORMAL.
    // Capture per-element (src_offset, size) so we can re-pack the vertex data.
    let mut new_elems: Vec<DeclElem> = Vec::new();
    let mut copy_plan: Vec<(usize, usize)> = Vec::new(); // (src_off, size) for copied elems
    for (i, e) in src_elems.iter().enumerate() {
        if e.stream == 255 {
            continue;
        }
        if i == normal_pos {
            new_elems.push(DeclElem { stream: 0, offset: 0, typ: 5, usage: 2 }); // BLENDIDX
            new_elems.push(DeclElem { stream: 0, offset: 0, typ: 8, usage: 1 }); // BLENDWT
        }
        new_elems.push(*e);
        copy_plan.push((e.offset as usize, decl_type_size(e.typ)));
    }
    let (new_decl, new_stride) = build_decl(&new_elems);

    // STRM info field0 = decl element count INCLUDING the terminator row (verified vs
    // obama: field0==7 for 7-element decls, 8 for 8-element). Adding BLENDIDX+BLENDWT
    // grew the decl by 2 elements, so field0 must grow too — otherwise the engine
    // processes the wrong number of vertex elements and the runtime object is malformed.
    let _ = field0;
    let new_field0 = (new_elems.len() + 1) as u32; // +1 for the terminator row
    let mut ninfo = Vec::with_capacity(12);
    ninfo.extend_from_slice(&new_field0.to_le_bytes());
    ninfo.extend_from_slice(&(new_stride as u32).to_le_bytes());
    ninfo.extend_from_slice(&(vcount as u32).to_le_bytes());
    descs[sinfo].body = ninfo;

    descs[sdecl].body = new_decl;
    descs[sdecl].size = descs[sdecl].body.len() as u32;

    // Re-pack the vertex data at the new stride. For each vertex, copy the source
    // elements (in order), inserting the 8 bind bytes at the NORMAL boundary.
    let src = descs[sdata].body.clone();
    if src.len() != old_stride * vcount {
        return Err(format!("eye data size {} != {old_stride}*{vcount}", src.len()));
    }
    let mut dst = Vec::with_capacity(new_stride * vcount);
    for v in 0..vcount {
        let base = v * old_stride;
        let mut ci = 0usize; // index into copy_plan
        for (i, e) in src_elems.iter().enumerate() {
            if e.stream == 255 {
                continue;
            }
            if i == normal_pos {
                // BLENDINDICES = palette[0] (obama's convention: the actual bone is in
                // INFO dword[6].lo16, blendidx just selects within the per-group palette;
                // a single-bone eye palette → index 0). BLENDWEIGHT = 0xFF (1.0).
                dst.extend_from_slice(&[0, 0, 0, 0]); // BLENDINDICES → palette[0]
                dst.extend_from_slice(&[0xff, 0, 0, 0]); // BLENDWEIGHT (1.0)
            }
            let (soff, sz) = copy_plan[ci];
            if i == 0 && e.usage == 0 && sz >= 6 {
                // POSITION element: translate from the static eye-local space into model
                // space (the eye-socket world position of the eyeball bone), then bind to
                // GlobalSRT (identity) so the placement renders verbatim. The eyeball-bone
                // world matrix is pure translation (rotation ≈ identity), so we only add
                // `translation` to the f16 x/y/z; w (and any extra bytes) copied verbatim.
                let px = decode_f16(&src[base + soff..]) + translation[0];
                let py = decode_f16(&src[base + soff + 2..]) + translation[1];
                let pz = decode_f16(&src[base + soff + 4..]) + translation[2];
                dst.extend_from_slice(&encode_f16(px));
                dst.extend_from_slice(&encode_f16(py));
                dst.extend_from_slice(&encode_f16(pz));
                dst.extend_from_slice(&src[base + soff + 6..base + soff + sz]); // w + rest
            } else {
                dst.extend_from_slice(&src[base + soff..base + soff + sz]);
            }
            ci += 1;
        }
    }
    if dst.len() != new_stride * vcount {
        return Err(format!("repack produced {} bytes, expected {new_stride}*{vcount}", dst.len()));
    }
    descs[sdata].body = dst;
    descs[sdata].size = (new_stride * vcount) as u32;
    Ok(())
}

/// Fix the `u3` (reverse-sibling index) of a PRMG's surviving DIRECT children after
/// removing one of them (the AREA chunk). Must be called BEFORE the `u4` span fixup
/// (it relies on the ORIGINAL spans to enumerate the PRMG's direct children).
/// For each removed direct child at reverse-index R, every surviving direct child with
/// u3 > R is decremented by 1.
fn fix_prmg_child_u3(descs: &mut [Desc], pi: usize, remove: &[bool]) {
    let n = descs.len();
    let pend = (pi + descs[pi].u4 as usize).min(n - 1);
    // Enumerate the PRMG's direct children (rows consumed by original u4 spans).
    let mut children: Vec<usize> = Vec::new();
    let mut c = pi + 1;
    while c <= pend {
        children.push(c);
        c += descs[c].u4 as usize + 1;
    }
    // Reverse-indices of removed direct children.
    let removed_ridx: Vec<u32> = children.iter().filter(|&&c| remove[c]).map(|&c| descs[c].u3).collect();
    for &c in &children {
        if remove[c] {
            continue;
        }
        let dec = removed_ridx.iter().filter(|&&r| r < descs[c].u3).count() as u32;
        descs[c].u3 -= dec;
    }
}

/// Read a row-major 4x4 f32 matrix from `b` at dword offset `dw`.
fn read_mat4(b: &[u8], dw: usize) -> [[f32; 4]; 4] {
    let mut m = [[0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            let o = (dw + r * 4 + c) * 4;
            m[r][c] = f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        }
    }
    m
}

/// Row-vector × matrix multiply (`v @ M`), v homogeneous.
fn mul_row(v: [f32; 4], m: &[[f32; 4]; 4]) -> [f32; 4] {
    let mut o = [0f32; 4];
    for c in 0..4 {
        o[c] = v[0] * m[0][c] + v[1] * m[1][c] + v[2] * m[2][c] + v[3] * m[3][c];
    }
    o
}

/// 4x4 × 4x4 multiply (`A @ B`, row-major, row-vector convention).
fn mul_mat(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut o = [[0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            o[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c] + a[r][3] * b[3][c];
        }
    }
    o
}

/// Compute a bone's WORLD-space translation by chaining its per-bone LOCAL matrix
/// (HIER bone record: 176 bytes; local matrix = f32[16] at dword 1..17; parent index =
/// `dword[2] & 0xFFFF`) up the parent chain to the root. World = M_bone @ M_parent @ … .
fn bone_world_translation(hier: &[u8], bone: usize) -> Result<[f32; 3], String> {
    const BONE_SZ: usize = 176;
    let nb = hier.len() / BONE_SZ;
    if bone >= nb {
        return Err(format!("bone {bone} >= {nb}"));
    }
    let parent_of = |bi: usize| -> i32 {
        let d2 = u32::from_le_bytes([
            hier[bi * BONE_SZ + 8],
            hier[bi * BONE_SZ + 9],
            hier[bi * BONE_SZ + 10],
            hier[bi * BONE_SZ + 11],
        ]);
        let p = (d2 & 0xffff) as usize;
        if p < nb && p != bi { p as i32 } else { -1 }
    };
    // local matrix is the f32[16] block at dword 4 (byte offset 16) of the 176-byte bone
    // record (dword0=name_hash, dwords1..4=parent/flags, then the 4x4 local matrix).
    let local = |bi: usize| read_mat4(&hier[bi * BONE_SZ..(bi + 1) * BONE_SZ], 4);

    let mut world = local(bone);
    let mut visited = vec![bone];
    let mut cur = parent_of(bone);
    while cur >= 0 && !visited.contains(&(cur as usize)) {
        let p = cur as usize;
        world = mul_mat(&world, &local(p));
        visited.push(p);
        cur = parent_of(p);
    }
    let t = mul_row([0.0, 0.0, 0.0, 1.0], &world);
    Ok([t[0], t[1], t[2]])
}

/// Re-rig the two MESH eye slots into SKIN slots. `bones` maps the i-th MESH slot
/// (in descriptor order) to the HIER eyeball-bone index used for the model-space
/// placement translation. Returns the number of MESH slots reskinned.
pub fn reskin_container_eyes(
    container: &mut Vec<u8>,
    bones: &[u8],
    obama_eye_info: &[u8; 56],
) -> Result<usize, String> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_base = rd_u32(container, 4) as usize;
    let n_desc = rd_u32(container, 16) as usize;
    if 20 + n_desc * 20 > container.len() {
        return Err("descriptor table out of range".into());
    }
    let mut descs: Vec<Desc> = Vec::with_capacity(n_desc);
    for d in 0..n_desc {
        let o = 20 + d * 20;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&container[o..o + 4]);
        let u0 = rd_u32(container, o + 4);
        let size = rd_u32(container, o + 8);
        let u3 = rd_u32(container, o + 12);
        let u4 = rd_u32(container, o + 16);
        let body = if u0 != 0xFFFF_FFFF && size > 0 {
            container[data_base + u0 as usize..data_base + u0 as usize + size as usize].to_vec()
        } else {
            Vec::new()
        };
        descs.push(Desc { tag, u0, size, u3, u4, body });
    }

    // Collect MESH slot indices in order.
    let mesh_idx: Vec<usize> = (0..n_desc).filter(|&i| &descs[i].tag == MESH).collect();
    if mesh_idx.is_empty() {
        return Ok(0);
    }
    if bones.len() < mesh_idx.len() {
        return Err(format!("need {} bone(s), got {}", mesh_idx.len(), bones.len()));
    }

    // Mark AREA chunk-groups (inside the MESH subtrees) for removal.
    let mut remove = vec![false; n_desc];
    for i in 0..n_desc {
        if &descs[i].tag == AREA {
            let span = descs[i].u4 as usize;
            for k in 0..=span {
                if i + k < n_desc {
                    remove[i + k] = true;
                }
            }
        }
    }

    // Parse HIER so we can place each eye in MODEL space: the static eye verts live in
    // eyeball-bone-LOCAL space; the eyeball bone's WORLD matrix (chain of per-bone local
    // matrices up to the root) is essentially a pure translation to the eye socket
    // (rotation ≈ identity, verified). We translate the eye POSITIONs by that world
    // translation, then bind to the HEAD bone (index 25). Sarah's eyeball-bone inverse-
    // bind matrices (M2) are converter-broken (both share the head's M2, so binding to
    // them collapses both eyes); but the HEAD bone's own M2 IS correct
    // (world(25) @ M2[25] == I, verified) — so model-space eye verts bound to bone 25:
    //  - at REST: skin = vert_model @ M2[25] @ world(25) == vert_model → stay in socket;
    //  - ANIMATED: skin = vert_model @ M2[25] @ head_anim_world → eyes RIDE THE HEAD.
    // (Binding to GlobalSRT(0) placed them right but static — bone 0 doesn't animate.)
    const HEAD_BONE: u8 = 25;
    let hier = descs
        .iter()
        .find(|d| &d.tag == b"HIER")
        .map(|d| d.body.clone())
        .ok_or("no HIER chunk")?;

    for (mi, &m) in mesh_idx.iter().enumerate() {
        let eye_bone = bones[mi];
        let translation = bone_world_translation(&hier, eye_bone as usize)?;
        let mend = m + descs[m].u4 as usize;
        // Each direct-child PRMG of this MESH. Bind to the HEAD bone (INFO[6].lo16 = 25).
        let mut c = m + 1;
        while c <= mend {
            if &descs[c].tag == PRMG {
                reskin_prmg(&mut descs, c, HEAD_BONE, translation, obama_eye_info)?;
                c += descs[c].u4 as usize + 1;
            } else {
                c += 1;
            }
        }
        descs[m].tag = *SKIN;
    }

    // CRITICAL (do this BEFORE the u4 fixup, while spans are still ORIGINAL): `u3` is
    // a REVERSE-SIBLING index local to each parent's child list (last child u3==0;
    // verified vs obama). Each eye PRMG had its AREA chunk (a direct child between STRM
    // and IBUF) removed, shortening that PRMG's direct-child list by 1. Every surviving
    // direct child of an affected PRMG that came BEFORE the removed AREA must have its
    // u3 decremented — otherwise the engine builds the runtime object (and its
    // vtable/sub-pointers) from a stale sibling count → garbage vcall (the
    // heap-dependent C0000005 active-render crash). Enumerate each MESH slot's direct
    // children using ORIGINAL spans and fix every inner PRMG.
    for &m in &mesh_idx {
        let mend = m + descs[m].u4 as usize;
        let mut c = m + 1;
        while c <= mend {
            if &descs[c].tag == PRMG {
                fix_prmg_child_u3(&mut descs, c, &remove);
                c += descs[c].u4 as usize + 1;
            } else {
                c += 1;
            }
        }
    }

    // Decrement every surviving container row's u4 span by removed rows in its subtree.
    let mut removed_prefix = vec![0u32; n_desc + 1];
    for j in 0..n_desc {
        removed_prefix[j + 1] = removed_prefix[j] + if remove[j] { 1 } else { 0 };
    }
    for j in 0..n_desc {
        if remove[j] || descs[j].u4 == 0 {
            continue;
        }
        let lo = j + 1;
        let hi = (j + descs[j].u4 as usize).min(n_desc - 1);
        if lo <= hi {
            descs[j].u4 -= removed_prefix[hi + 1] - removed_prefix[lo];
        }
    }

    // Re-serialize: fresh body region + u0, data_base, n_desc, CSUM.
    let survivors: Vec<&Desc> =
        descs.iter().enumerate().filter(|(j, _)| !remove[*j]).map(|(_, d)| d).collect();
    let new_n = survivors.len();
    let new_data_base = 20 + new_n * 20;
    let mut out = Vec::new();
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&(new_data_base as u32).to_le_bytes());
    out.extend_from_slice(&container[8..12]);
    out.extend_from_slice(&container[12..16]);
    out.extend_from_slice(&(new_n as u32).to_le_bytes());
    let mut body_region: Vec<u8> = Vec::new();
    let mut rows: Vec<u8> = Vec::with_capacity(new_n * 20);
    for d in &survivors {
        let u0_out = if d.u0 != 0xFFFF_FFFF && d.size > 0 {
            let off = body_region.len() as u32;
            body_region.extend_from_slice(&d.body);
            off
        } else {
            d.u0
        };
        rows.extend_from_slice(&d.tag);
        rows.extend_from_slice(&u0_out.to_le_bytes());
        rows.extend_from_slice(&d.size.to_le_bytes());
        rows.extend_from_slice(&d.u3.to_le_bytes());
        rows.extend_from_slice(&d.u4.to_le_bytes());
    }
    out.extend_from_slice(&rows);
    out.extend_from_slice(&body_region);
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());

    *container = out;
    Ok(mesh_idx.len())
}
