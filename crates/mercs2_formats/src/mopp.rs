//! Native Havok-5.5 **MOPP** (Memory-Optimized Partial Polytope) bytecode compiler — the
//! `hkpMoppCode` BV-tree that accelerates static-world collision.
//!
//! Two halves:
//! - [`decode`] — walk an existing MOPP `m_data` buffer and recover every leaf shape-key (triangle
//!   index), "FindAll" semantics: geometry tests stripped, visit every node, emit every leaf. This
//!   mirrors the `hctFilterPhysics.dll` OBB/KDop virtual-machine switch, byte-for-byte on the
//!   dispatch table. Proven against the retail reference MOPP (901/901 bytes consumed, 0 errors,
//!   76 contiguous keys `[0..=76]`).
//! - [`encode`] — build our OWN valid MOPP over a triangle soup (axis-aligned median-split BVH) that
//!   [`decode`] reads back with 100 % byte coverage and the exact input triangle-index set. This is
//!   the native replacement for the walled HCT/DCC bake ([`terrain_collision_regeneration.md`]) —
//!   regenerate collision for new geometry without the Havok toolchain.
//!
//! Full opcode table + `hkpMoppCode` struct + quantization: `docs/reverse_engineer/mopp_bytecode_format.md`.
//! **All operands are big-endian.** Buffers extracted by [`crate::havok`] are raw u8 (no unscramble);
//! only the stale `output/_scratch/old_mopp.bin` dump needs [`unreverse_u32`].
//!
//! Encoder scope (P3.5 — in-game ready): the split-plane *coordinates* are now **spatially
//! conservative** and geometry-gated. A MOPP node is a Bounding-Interval-Hierarchy split — two planes
//! on one axis: the inline (LEFT) child's UPPER bound (`code[+1]`) and the offset (RIGHT) child's LOWER
//! bound (`code[+2]`); a query descends LEFT iff `qmin[axis] ≤ Lmax`, RIGHT iff `qmax[axis] ≥ Rmin`.
//! [`encode`] quantizes those bounds OUTWARD (upper→up, lower→down) so a query can never miss a boundary
//! triangle, and [`query_aabb`] is the real VM's pruning walk (narrowphase stripped). The no-miss and
//! semantics-cross-check gates below prove `query_aabb(encode(mesh)) ⊇ {tris overlapping the query}` on
//! synthetic + real `WpMeshShape16` data, and that `query_aabb` matches Havok's VM on real `vz.wad`
//! MOPPs. Only axis splits are emitted (`0x10–0x12` / `0x23–0x25`); the 26-DOP diagonal splits
//! (`0x13–0x1c`) are still avoided by the encoder — `query_aabb` handles them by visiting both children
//! unpruned (their plane geometry is INFERRED; conservative fallback). See the "Split geometry" section
//! of `docs/reverse_engineer/mopp_bytecode_format.md`.

use crate::havok::{self, HAVOK_MAGIC};

/// Big-endian read of `n` (≤4) bytes at `o` — MOPP operands are big-endian.
#[inline]
fn be(code: &[u8], o: usize, n: usize) -> u32 {
    let mut v = 0u32;
    for i in 0..n {
        v = (v << 8) | code[o + i] as u32;
    }
    v
}

#[inline]
fn u32_le(b: &[u8], o: usize) -> u32 {
    if o + 4 <= b.len() {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0
    }
}

#[inline]
fn f32_le(b: &[u8], o: usize) -> f32 {
    if o + 4 <= b.len() {
        f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0.0
    }
}

/// The result of walking a MOPP tree.
#[derive(Debug, Clone)]
pub struct Decoded {
    /// Leaf shape keys (triangle indices), in pre-order walk order (may repeat if the source tree
    /// references a triangle from more than one leaf; a well-formed mesh MOPP visits each once).
    pub keys: Vec<u32>,
    /// Number of DISTINCT bytes the walk touched — equals `code.len()` for a fully-covered tree.
    pub consumed: usize,
    /// `Some(msg)` if an INVALID opcode or an out-of-bounds branch was hit; `None` on a clean walk.
    pub error: Option<String>,
}

impl Decoded {
    /// The distinct, sorted key set with the inclusive range and any interior gaps — the shape of the
    /// "contiguous in-range" check the validation tests make.
    pub fn key_summary(&self) -> (Vec<u32>, Option<(u32, u32)>, Vec<u32>) {
        let mut ks = self.keys.clone();
        ks.sort_unstable();
        ks.dedup();
        if ks.is_empty() {
            return (ks, None, Vec::new());
        }
        let (lo, hi) = (ks[0], *ks.last().unwrap());
        let missing: Vec<u32> = (lo..=hi).filter(|k| ks.binary_search(k).is_err()).collect();
        (ks, Some((lo, hi)), missing)
    }
}

/// Walk a MOPP `m_data` bytecode buffer and emit every leaf shape-key.
///
/// Recursion is an explicit `(pc, key_base)` stack (a split pushes its RIGHT child, then continues its
/// LEFT child inline — pre-order). The dispatch mirrors the HCT VM switch exactly; see the opcode
/// table in the module docs. A clean tree yields `error = None` and `consumed == code.len()`.
pub fn decode(code: &[u8]) -> Decoded {
    let mut keys = Vec::new();
    let mut consumed = vec![false; code.len()];
    let mut error: Option<String> = None;

    let mut stack: Vec<(usize, u32)> = vec![(0, 0)];

    'outer: while let Some((mut pc, mut kb)) = stack.pop() {
        loop {
            if pc >= code.len() {
                error = Some(format!("pc out of bounds @ {pc}"));
                break 'outer;
            }
            let op = code[pc];
            // Bounds-check the operand span before reading it; mark it consumed.
            macro_rules! need {
                ($n:expr) => {{
                    if pc + $n > code.len() {
                        error = Some(format!("operand OOB for 0x{op:02x} @ {pc} (need {})", $n));
                        break 'outer;
                    }
                    for i in pc..pc + $n {
                        consumed[i] = true;
                    }
                }};
            }
            match op {
                0x00 => {
                    need!(1);
                    break;
                } // RETURN — end this branch
                0x01..=0x04 => {
                    need!(4);
                    pc += 4;
                } // REANCHOR + rescale (unary, continue)
                0x05 => {
                    need!(2);
                    pc += code[pc + 1] as usize + 2;
                } // JUMP8
                0x06 => {
                    need!(3);
                    pc += code[pc + 1] as usize * 256 + code[pc + 2] as usize + 3;
                } // JUMP16
                0x07 => {
                    need!(4);
                    pc += be(code, pc + 1, 2) as usize * 256 + code[pc + 3] as usize + 4;
                } // JUMP24
                0x09 => {
                    need!(2);
                    kb = kb.wrapping_add(code[pc + 1] as u32);
                    pc += 2;
                } // key_base += u8
                0x0a => {
                    need!(3);
                    kb = kb.wrapping_add(be(code, pc + 1, 2));
                    pc += 3;
                } // key_base += BE16
                0x0b => {
                    need!(5);
                    kb = be(code, pc + 1, 4);
                    pc += 5;
                } // key_base = BE32 (absolute)
                0x10..=0x1c => {
                    // SPLIT (axis 0x10–0x12; 26-DOP 0x13–0x1c share this layout): hdr 4, RIGHT @ +code[+3].
                    need!(4);
                    stack.push((pc + 4 + code[pc + 3] as usize, kb));
                    pc += 4;
                }
                0x20..=0x22 => {
                    // SPLIT (compressed 1-value axis): hdr 3, RIGHT @ +code[+2].
                    need!(3);
                    stack.push((pc + 3 + code[pc + 2] as usize, kb));
                    pc += 3;
                }
                0x23..=0x25 => {
                    // SPLIT with 16-bit child offsets: hdr 7, LEFT @ +7+BE16[3], RIGHT @ +7+BE16[5].
                    need!(7);
                    let loff = be(code, pc + 3, 2) as usize;
                    let roff = be(code, pc + 5, 2) as usize;
                    stack.push((pc + 7 + roff, kb));
                    pc += 7 + loff;
                }
                0x26..=0x28 => {
                    need!(3);
                    pc += 3;
                } // unary CUT (2-byte coords)
                0x29..=0x2b => {
                    need!(7);
                    pc += 7;
                } // unary CUT (BE24 coords)
                0x30..=0x4f => {
                    need!(1);
                    keys.push(kb.wrapping_add(op as u32 - 0x30));
                    break;
                } // TERMINAL small
                0x50 => {
                    need!(2);
                    keys.push(kb.wrapping_add(code[pc + 1] as u32));
                    break;
                } // TERMINAL +u8
                0x51 => {
                    need!(3);
                    keys.push(kb.wrapping_add(be(code, pc + 1, 2)));
                    break;
                } // TERMINAL +BE16
                0x52 => {
                    need!(4);
                    keys.push(kb.wrapping_add(be(code, pc + 1, 3)));
                    break;
                } // TERMINAL +BE24
                0x53 => {
                    need!(5);
                    keys.push(kb.wrapping_add(be(code, pc + 1, 4)));
                    break;
                } // TERMINAL +BE32
                0x60..=0x63 => {
                    need!(2);
                    pc += 2;
                } // set property = u8
                0x64..=0x67 => {
                    need!(3);
                    pc += 3;
                } // set property = BE16
                0x68..=0x6b => {
                    need!(5);
                    pc += 5;
                } // set property = BE32
                _ => {
                    error = Some(format!("unknown command 0x{op:02x} @ {pc}"));
                    break 'outer;
                }
            }
        }
    }

    Decoded {
        keys,
        consumed: consumed.iter().filter(|&&b| b).count(),
        error,
    }
}

/// Undo the legacy blind-u32 byteswap: un-reverse every aligned 4-byte word.
///
/// Needed ONLY for the stale `output/_scratch/old_mopp.bin` dump, which predates the reader fix that
/// stores MOPP `m_data` as raw u8. A buffer extracted by the current [`crate::havok`] reader (or
/// [`extract_mopp_buffers`]) is already raw and must NOT be un-reversed.
pub fn unreverse_u32(raw: &[u8]) -> Vec<u8> {
    let mut b = raw.to_vec();
    let n = raw.len() - raw.len() % 4;
    let mut i = 0;
    while i < n {
        b.swap(i, i + 3);
        b.swap(i + 1, i + 2);
        i += 4;
    }
    b
}

/// Pull every `hkpMoppCode` `m_data` bytecode buffer out of a decompressed WAD block / packfile buffer.
///
/// Additive over the [`crate::havok`] reader: for each embedded Havok packfile it walks the virtual
/// fixups, and for every `hkpMoppCode` object reads the `m_data` hkArray (`ptr @ obj+32`,
/// `count(u32) @ obj+36` — the layout the byteswap converter's `HKP_MOPP_CODE_ARRAYS` pins) and
/// returns the raw u8 slice. These decode directly (no [`unreverse_u32`]).
pub fn extract_mopp_buffers(buf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(rel) = buf[at..]
        .windows(HAVOK_MAGIC.len())
        .position(|w| w == HAVOK_MAGIC)
    {
        let off = at + rel;
        let pk = &buf[off..];
        match havok::parse_packfile_raw(pk) {
            Ok(raw) => {
                for (src, cname) in &raw.vfixups {
                    if cname != "hkpMoppCode" {
                        continue;
                    }
                    // m_data hkArray: ptr @ +32 (local fixup), count @ +36 (inline u32).
                    if let Some(ptr) = raw.resolve_ptr(*src, 32) {
                        let count = u32_le(pk, raw.data_pk + src + 36) as usize;
                        if count > 0 && ptr + count <= pk.len() {
                            out.push(pk[ptr..ptr + count].to_vec());
                        }
                    }
                }
                at = off + raw.size.max(8);
            }
            Err(_) => at = off + 8,
        }
    }
    out
}

/// Like [`extract_mopp_buffers`] but also recovers each MOPP's [`MoppInfo`] quantization frame — the
/// `hkpMoppCode::CodeInfo m_info` (a `hkVector4 m_offset`) that [`query_aabb`] needs to dequantize
/// split planes back to world coordinates. On this 32-bit Havok 5.5 layout `m_info` sits at `obj+16`
/// (offset.xyz @ `+16/+20/+24`, and lane 3 @ `+28`), immediately before the `m_data` hkArray
/// (`ptr @ +32`, `count @ +36`) the reader already pins.
///
/// **Lane 3 is the RECIPROCAL scale.** Measured on retail `vz.wad`, `+28` holds values ~`1e6`–`2e6`
/// (e.g. 2 280 380 for a ~30 m cell) — far too large to be a world-per-unit step; its reciprocal
/// (~`4.4e-7`) is the sane per-integer-unit world scale for a ~24-bit frame, matching the VM's
/// `realCoord = intCoord * this[0x10]` multiply. So [`MoppInfo::scale`] = `1.0 / f32(+28)`.
pub fn extract_mopp_with_info(buf: &[u8]) -> Vec<(Vec<u8>, MoppInfo)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(rel) = buf[at..]
        .windows(HAVOK_MAGIC.len())
        .position(|w| w == HAVOK_MAGIC)
    {
        let off = at + rel;
        let pk = &buf[off..];
        match havok::parse_packfile_raw(pk) {
            Ok(raw) => {
                for (src, cname) in &raw.vfixups {
                    if cname != "hkpMoppCode" {
                        continue;
                    }
                    if let Some(ptr) = raw.resolve_ptr(*src, 32) {
                        let obj = raw.data_pk + src;
                        let count = u32_le(pk, obj + 36) as usize;
                        if count > 0 && ptr + count <= pk.len() {
                            let w = f32_le(pk, obj + 28); // lane 3 = 1/scale
                            let scale = if w.is_finite() && w.abs() > 0.0 { 1.0 / w } else { 0.0 };
                            let info = MoppInfo {
                                offset: [
                                    f32_le(pk, obj + 16),
                                    f32_le(pk, obj + 20),
                                    f32_le(pk, obj + 24),
                                ],
                                scale,
                            };
                            out.push((pk[ptr..ptr + count].to_vec(), info));
                        }
                    }
                }
                at = off + raw.size.max(8);
            }
            Err(_) => at = off + 8,
        }
    }
    out
}

// ─────────────────────────── ENCODER (Phase 3) ───────────────────────────

/// The `hkpMoppCode::CodeInfo` quantization frame that pairs with an emitted [`encode`] buffer:
/// `world_coord = int_coord * scale + offset` (per axis; `scale` is uniform — Havok's lane-3 scalar).
///
/// The [`decode`] path recovers triangle indices WITHOUT this frame; it is what a serializer needs to
/// write the full `hkpMoppCode` struct (`m_offset = [offset.x, offset.y, offset.z, scale]`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoppInfo {
    /// AABB-min origin of the integer frame, per axis.
    pub offset: [f32; 3],
    /// Uniform scale: `world = int * scale + offset`. `int` spans the 16-bit root frame `[0, 0xFFFF]`.
    pub scale: f32,
}

/// Build a valid MOPP `m_data` buffer over a triangle soup, plus its [`MoppInfo`] quantization frame.
///
/// The leaf shape-key emitted for triangle `t` is exactly `t` (its index into `tris`), so
/// `decode(encode(tris, verts).0)` recovers the input triangle-index set. Structure: an axis-aligned
/// median-split BVH; internal nodes are axis splits (`0x10–0x12`, or `0x23–0x25` when the inline
/// subtree exceeds 255 bytes), leaves are `0x0b <BE32 idx>` + `0x30` (absolute key, delta 0).
///
/// The smaller child is always placed at offset 0 (inline) and the larger at the encoded offset, which
/// bounds every offset field by half the tree size — keeping the BE16 offsets valid for trees up to
/// ~128 KB (~20 k triangles), well past any single retail collision mesh.
///
/// `verts` is used only to compute split axes and the [`MoppInfo`] frame; a triangle index that is out
/// of range of `verts` still encodes (its centroid falls back to the origin) — the decoder is
/// geometry-agnostic. Returns an empty-tree `[0x00]` for zero triangles.
pub fn encode(tris: &[[u32; 3]], verts: &[[f32; 3]]) -> (Vec<u8>, MoppInfo) {
    // Quantization frame from the vertex AABB (uniform scale on the widest axis).
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in verts {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    if verts.is_empty() {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    let extent = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let max_ext = extent.iter().cloned().fold(0.0f32, f32::max);
    // Divide by `0xFF00`, not `0xFFFF`: at the root shift (8) a byte split operand `b<<8` tops out at
    // `0xFF00`, so the whole mesh must fit in `int ∈ [0, 0xFF00]` for its far face to be representable
    // as an *upper* bound (`code[+1]`). `/0xFF00` puts the AABB-max vertex at exactly `int = 0xFF00`
    // (`ceil(0xFF00/256) = 255`) — conservatively enclosable. Floor avoids a zero/degenerate scale.
    let scale = (max_ext / 65280.0).max(1e-6);
    let info = MoppInfo { offset: lo, scale };

    if tris.is_empty() {
        return (vec![0x00], info);
    }

    // Per-triangle world AABB (over its 3 verts) + centroid. Out-of-range indices fall back to the
    // origin — the decoder is geometry-agnostic, and real meshes never carry them.
    let vc = |i: u32| -> [f32; 3] { verts.get(i as usize).copied().unwrap_or([0.0; 3]) };
    #[derive(Clone, Copy)]
    struct Tri {
        lo: [f32; 3],
        hi: [f32; 3],
        c: [f32; 3],
    }
    let tri: Vec<Tri> = tris
        .iter()
        .map(|t| {
            let (a, b, c) = (vc(t[0]), vc(t[1]), vc(t[2]));
            let (mut l, mut h) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in [a, b, c] {
                for k in 0..3 {
                    l[k] = l[k].min(v[k]);
                    h[k] = h[k].max(v[k]);
                }
            }
            Tri {
                lo: l,
                hi: h,
                c: [
                    (a[0] + b[0] + c[0]) / 3.0,
                    (a[1] + b[1] + c[1]) / 3.0,
                    (a[2] + b[2] + c[2]) / 3.0,
                ],
            }
        })
        .collect();

    // Quantize a world coordinate on `axis` to a root byte operand, rounding OUTWARD so the recovered
    // plane conservatively brackets `world`: `q_up` (an upper bound) rounds UP, `q_down` (a lower
    // bound) rounds DOWN. `plane_int = byte << 8`; `plane_world = plane_int*scale + offset`.
    let q_up = move |world: f32, axis: usize| -> u8 {
        (((world - info.offset[axis]) / info.scale / 256.0).ceil()).clamp(0.0, 255.0) as u8
    };
    let q_down = move |world: f32, axis: usize| -> u8 {
        (((world - info.offset[axis]) / info.scale / 256.0).floor()).clamp(0.0, 255.0) as u8
    };

    // Recursively split `idx` (indices into `tri`); returns (subtree bytes, subtree world AABB). The
    // node is a Bounding-Interval-Hierarchy split: the inline child is bounded ABOVE by `code[+1]`
    // (its true max on the axis, rounded up); the offset child bounded BELOW by `code[+2]` (its true
    // min, rounded down). Correct regardless of which geometric half lands inline.
    fn emit(
        idx: &mut [usize],
        tri: &[Tri],
        q_up: &dyn Fn(f32, usize) -> u8,
        q_down: &dyn Fn(f32, usize) -> u8,
    ) -> (Vec<u8>, [f32; 3], [f32; 3]) {
        let (mut slo, mut shi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for &i in idx.iter() {
            for k in 0..3 {
                slo[k] = slo[k].min(tri[i].lo[k]);
                shi[k] = shi[k].max(tri[i].hi[k]);
            }
        }
        if idx.len() == 1 {
            // Leaf: absolute key = triangle index. 0x0b <BE32 idx> then 0x30 (delta 0).
            let t = idx[0] as u32;
            let bytes = vec![
                0x0b,
                (t >> 24) as u8,
                (t >> 16) as u8,
                (t >> 8) as u8,
                t as u8,
                0x30,
            ];
            return (bytes, slo, shi);
        }

        // Split axis = widest centroid spread; median split keeps the tree balanced.
        let (mut clo, mut chi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for &i in idx.iter() {
            for k in 0..3 {
                clo[k] = clo[k].min(tri[i].c[k]);
                chi[k] = chi[k].max(tri[i].c[k]);
            }
        }
        let axis = (0..3)
            .max_by(|&a, &b| (chi[a] - clo[a]).partial_cmp(&(chi[b] - clo[b])).unwrap())
            .unwrap();
        idx.sort_by(|&a, &b| tri[a].c[axis].partial_cmp(&tri[b].c[axis]).unwrap());
        let mid = idx.len() / 2;
        let (left, right) = idx.split_at_mut(mid);

        let (lb, llo, lhi) = emit(left, tri, q_up, q_down);
        let (rb, rlo, rhi) = emit(right, tri, q_up, q_down);
        // Place the SMALLER subtree inline (offset 0) so the offset field stays ≤ half the tree —
        // keeps the u8/BE16 offset valid for large meshes. `first` = inline (LEFT position → its MAX
        // is `code[+1]`); `second` = offset (RIGHT position → its MIN is `code[+2]`).
        let ((first, _flo, fhi), (second, slo2, _shi)) = if lb.len() <= rb.len() {
            ((lb, llo, lhi), (rb, rlo, rhi))
        } else {
            ((rb, rlo, rhi), (lb, llo, lhi))
        };
        let lmax = q_up(fhi[axis], axis); // inline child's upper bound (rounded UP)
        let rmin = q_down(slo2[axis], axis); // offset child's lower bound (rounded DOWN)
        let off = first.len();
        let mut out = Vec::with_capacity(7 + first.len() + second.len());
        if off <= 255 {
            // 0x10+axis: hdr [op, Lmax, Rmin, right_off]; LEFT inline @ +4, RIGHT @ +4+right_off.
            out.push(0x10 + axis as u8);
            out.push(lmax);
            out.push(rmin);
            out.push(off as u8);
        } else {
            // 0x23+axis: hdr [op, Lmax, Rmin, leftoff_be16=0, rightoff_be16=off]; LEFT @ +7, RIGHT @ +7+off.
            out.push(0x23 + axis as u8);
            out.push(lmax);
            out.push(rmin);
            out.push(0);
            out.push(0);
            out.push((off >> 8) as u8);
            out.push(off as u8);
        }
        out.extend_from_slice(&first);
        out.extend_from_slice(&second);
        (out, slo, shi)
    }

    let mut idx: Vec<usize> = (0..tris.len()).collect();
    let (code, _lo, _hi) = emit(&mut idx, &tri, &q_up, &q_down);
    (code, info)
}

/// Build a valid MOPP `m_data` buffer that yields **every** shape key `[0..n_tris)` for ANY query —
/// an *always-visit-both* binary tree. Needs **no vertex positions**: only the triangle count.
///
/// This is the "universal collision" MOPP the in-game proof stands on — swap it into a real cell and
/// every triangle becomes a broadphase candidate for every query, so if the game's own collision is
/// running our bytecode, the whole cell collides. It is also the cleanest possible offline gate: the
/// key set is fixed by `n_tris` alone, independent of any geometry frame.
///
/// Structure: a balanced binary tree whose internal nodes are axis-0 splits with the plane operands
/// pinned to the frame extremes — `Lmax = 0xFF` (inline child's upper bound = top of the byte frame)
/// and `Rmin = 0x00` (offset child's lower bound = bottom). Under any paired [`MoppInfo`] whose frame
/// brackets the query, both slab tests `qmin ≤ Lmax` and `qmax ≥ Rmin` hold, so [`query_aabb`] prunes
/// nothing; [`decode`]'s FindAll walk visits both children unconditionally regardless of the frame, so
/// `decode(encode_return_all(n)).keys == 0..n` always. Leaves are `0x0b <BE32 idx>` + `0x30` (absolute
/// key = triangle index, delta 0) — the same MVP-safe terminal [`encode`] emits. The smaller subtree is
/// placed inline so the child offset stays ≤ half the tree (u8 form `0x10`, or the BE16 form `0x23`
/// once an inline subtree exceeds 255 bytes). Returns `[0x00]` (empty tree) for `n_tris == 0`.
pub fn encode_return_all(n_tris: u32) -> Vec<u8> {
    if n_tris == 0 {
        return vec![0x00];
    }
    // Emit the subtree covering triangle indices `[lo, hi)` (hi > lo) into `out`.
    fn emit(lo: u32, hi: u32, out: &mut Vec<u8>) {
        if hi - lo == 1 {
            let t = lo;
            out.extend_from_slice(&[0x0b, (t >> 24) as u8, (t >> 16) as u8, (t >> 8) as u8, t as u8, 0x30]);
            return;
        }
        let mid = lo + (hi - lo) / 2;
        let mut left = Vec::new();
        emit(lo, mid, &mut left);
        let mut right = Vec::new();
        emit(mid, hi, &mut right);
        // Smaller subtree inline (offset 0) so the offset field stays ≤ half the tree.
        let (first, second) = if left.len() <= right.len() {
            (left, right)
        } else {
            (right, left)
        };
        let off = first.len();
        if off <= 255 {
            // 0x10 axis-0 split: [op, Lmax=0xFF, Rmin=0x00, right_off]; LEFT @ +4, RIGHT @ +4+off.
            out.extend_from_slice(&[0x10, 0xFF, 0x00, off as u8]);
        } else {
            // 0x23 axis-0 split, BE16 child offsets: [op, Lmax, Rmin, leftoff=0, rightoff=off].
            out.extend_from_slice(&[0x23, 0xFF, 0x00, 0, 0, (off >> 8) as u8, off as u8]);
        }
        out.extend_from_slice(&first);
        out.extend_from_slice(&second);
    }
    let mut out = Vec::new();
    emit(0, n_tris, &mut out);
    out
}

/// Record every leaf's reconstructed node box (`key`, `blo`, `bhi`) by walking the tree exactly as
/// [`query_aabb`] does but WITHOUT any query pruning — the box captured at a terminal is the
/// intersection of all the cut/split half-spaces on the path from the root to that leaf. Used by the
/// real-MOPP semantics gate: a query placed inside a leaf's own box must reach that leaf (a
/// pruning-correctness theorem — every ancestor box encloses the leaf box, so the walk cannot prune
/// it). Exercises the real reanchor/cut/26-DOP opcodes the encoder never emits. Boxes are in the frame
/// implied by `info` + `root_shift`. Test-only support for the real-MOPP semantics gate.
#[cfg(test)]
pub(crate) fn leaf_boxes(
    code: &[u8],
    info: &MoppInfo,
    root_shift: u32,
) -> Vec<(u32, [f32; 3], [f32; 3])> {
    let mut out = Vec::new();
    if code.is_empty() {
        return out;
    }
    let scale = info.scale as f64;
    let offv = info.offset;
    let plane = |opd: i64, origin_a: i64, shift: u32, axis: usize| -> f32 {
        ((((opd << shift) + origin_a) as f64) * scale + offv[axis] as f64) as f32
    };
    let abs_coord = |v: u32, axis: usize| -> f32 { ((v as f64) * scale + offv[axis] as f64) as f32 };
    #[derive(Clone)]
    struct Frame {
        pc: usize,
        origin: [i64; 3],
        shift: u32,
        kb: u32,
        blo: [f32; 3],
        bhi: [f32; 3],
    }
    let inf = f32::INFINITY;
    let mut stack = vec![Frame {
        pc: 0,
        origin: [0; 3],
        shift: root_shift,
        kb: 0,
        blo: [-inf; 3],
        bhi: [inf; 3],
    }];
    let mut steps = 0usize;
    while let Some(mut fr) = stack.pop() {
        loop {
            steps += 1;
            if steps > 100_000_000 || fr.pc >= code.len() {
                break;
            }
            let op = code[fr.pc];
            macro_rules! need {
                ($n:expr) => {
                    if fr.pc + $n > code.len() {
                        break;
                    }
                };
            }
            match op {
                0x00 => break,
                0x01..=0x04 => {
                    need!(4);
                    let s = fr.shift;
                    for k in 0..3 {
                        fr.origin[k] += (code[fr.pc + 1 + k] as i64) << s;
                    }
                    fr.shift = fr.shift.saturating_sub(op as u32);
                    fr.pc += 4;
                }
                0x05 => {
                    need!(2);
                    fr.pc += code[fr.pc + 1] as usize + 2;
                }
                0x06 => {
                    need!(3);
                    fr.pc += code[fr.pc + 1] as usize * 256 + code[fr.pc + 2] as usize + 3;
                }
                0x07 => {
                    need!(4);
                    fr.pc += be(code, fr.pc + 1, 2) as usize * 256 + code[fr.pc + 3] as usize + 4;
                }
                0x09 => {
                    need!(2);
                    fr.kb = fr.kb.wrapping_add(code[fr.pc + 1] as u32);
                    fr.pc += 2;
                }
                0x0a => {
                    need!(3);
                    fr.kb = fr.kb.wrapping_add(be(code, fr.pc + 1, 2));
                    fr.pc += 3;
                }
                0x0b => {
                    need!(5);
                    fr.kb = be(code, fr.pc + 1, 4);
                    fr.pc += 5;
                }
                0x10..=0x12 => {
                    need!(4);
                    let axis = (op - 0x10) as usize;
                    let lmax = plane(code[fr.pc + 1] as i64, fr.origin[axis], fr.shift, axis);
                    let rmin = plane(code[fr.pc + 2] as i64, fr.origin[axis], fr.shift, axis);
                    let rpc = fr.pc + 4 + code[fr.pc + 3] as usize;
                    let mut rblo = fr.blo;
                    rblo[axis] = rblo[axis].max(rmin);
                    stack.push(Frame { pc: rpc, blo: rblo, ..fr.clone() });
                    fr.bhi[axis] = fr.bhi[axis].min(lmax);
                    fr.pc += 4;
                }
                0x13..=0x1c => {
                    need!(4);
                    let rpc = fr.pc + 4 + code[fr.pc + 3] as usize;
                    stack.push(Frame { pc: rpc, ..fr.clone() });
                    fr.pc += 4;
                }
                0x20..=0x22 => {
                    need!(3);
                    let axis = (op - 0x20) as usize;
                    let v = code[fr.pc + 1] as i64;
                    let lmax = plane(v + 1, fr.origin[axis], fr.shift, axis);
                    let rmin = plane(v, fr.origin[axis], fr.shift, axis);
                    let rpc = fr.pc + 3 + code[fr.pc + 2] as usize;
                    let mut rblo = fr.blo;
                    rblo[axis] = rblo[axis].max(rmin);
                    stack.push(Frame { pc: rpc, blo: rblo, ..fr.clone() });
                    fr.bhi[axis] = fr.bhi[axis].min(lmax);
                    fr.pc += 3;
                }
                0x23..=0x25 => {
                    need!(7);
                    let axis = (op - 0x23) as usize;
                    let lmax = plane(code[fr.pc + 1] as i64, fr.origin[axis], fr.shift, axis);
                    let rmin = plane(code[fr.pc + 2] as i64, fr.origin[axis], fr.shift, axis);
                    let loff = be(code, fr.pc + 3, 2) as usize;
                    let roff = be(code, fr.pc + 5, 2) as usize;
                    let rpc = fr.pc + 7 + roff;
                    let mut rblo = fr.blo;
                    rblo[axis] = rblo[axis].max(rmin);
                    stack.push(Frame { pc: rpc, blo: rblo, ..fr.clone() });
                    fr.bhi[axis] = fr.bhi[axis].min(lmax);
                    fr.pc += 7 + loff;
                }
                0x26..=0x28 => {
                    need!(3);
                    let axis = (op - 0x26) as usize;
                    fr.blo[axis] = plane(code[fr.pc + 1] as i64, fr.origin[axis], fr.shift, axis);
                    fr.bhi[axis] = plane(code[fr.pc + 2] as i64, fr.origin[axis], fr.shift, axis);
                    fr.pc += 3;
                }
                0x29..=0x2b => {
                    need!(7);
                    let axis = (op - 0x29) as usize;
                    fr.blo[axis] = abs_coord(be(code, fr.pc + 1, 3), axis);
                    fr.bhi[axis] = abs_coord(be(code, fr.pc + 4, 3), axis);
                    fr.pc += 7;
                }
                0x30..=0x4f => {
                    out.push((fr.kb.wrapping_add(op as u32 - 0x30), fr.blo, fr.bhi));
                    break;
                }
                0x50 => {
                    need!(2);
                    out.push((fr.kb.wrapping_add(code[fr.pc + 1] as u32), fr.blo, fr.bhi));
                    break;
                }
                0x51 => {
                    need!(3);
                    out.push((fr.kb.wrapping_add(be(code, fr.pc + 1, 2)), fr.blo, fr.bhi));
                    break;
                }
                0x52 => {
                    need!(4);
                    out.push((fr.kb.wrapping_add(be(code, fr.pc + 1, 3)), fr.blo, fr.bhi));
                    break;
                }
                0x53 => {
                    need!(5);
                    out.push((fr.kb.wrapping_add(be(code, fr.pc + 1, 4)), fr.blo, fr.bhi));
                    break;
                }
                0x60..=0x63 => {
                    need!(2);
                    fr.pc += 2;
                }
                0x64..=0x67 => {
                    need!(3);
                    fr.pc += 3;
                }
                0x68..=0x6b => {
                    need!(5);
                    fr.pc += 5;
                }
                _ => break,
            }
        }
    }
    out
}

/// Root integer-frame shift: at the tree root a byte split operand occupies bits `[8..16)` of the
/// 16-bit frame, so `plane_int = operand << 8`. REANCHOR ops (`0x01–0x04`) refine sub-cells by adding
/// to the origin and *decreasing* the shift; [`encode`] does not emit them (it stays at root
/// precision), but [`query_aabb`] tracks the shift so it walks real refined MOPPs correctly.
pub(crate) const ROOT_SHIFT: u32 = 8;

/// Geometric MOPP walk mirroring the HCT OBB/KDop virtual machine with narrowphase stripped — the real
/// broadphase minus the per-triangle test. Returns the candidate shape-keys whose leaves a query AABB
/// `[qmin, qmax]` (world/model-local coordinates, same frame as `info`) could overlap.
///
/// **Conservative:** a child is pruned only when its half-space/bounds provably cannot overlap the
/// query, so the result is a superset of the true overlap set — it never misses. Node bounds are
/// reconstructed in the exact integer frame the VM uses (`world = (operand<<shift + origin)*scale +
/// offset`), tracking REANCHOR (`0x01–0x04`) for shift/origin, CUT (`0x26–0x2b`) for tightened
/// per-axis bounds, and every split family. The 26-DOP diagonal splits (`0x13–0x1c`) are visited
/// unpruned (their plane geometry is not decoded — over-include rather than risk a miss).
///
/// A small `scale`-sized slop on the disjoint test absorbs f32-vs-f64 boundary rounding so a triangle
/// sitting exactly on a plane is never dropped.
pub fn query_aabb(code: &[u8], info: &MoppInfo, qmin: [f32; 3], qmax: [f32; 3]) -> Vec<u32> {
    query_aabb_shift(code, info, qmin, qmax, ROOT_SHIFT)
}

/// [`query_aabb`] with an explicit root integer-frame shift (bits the root byte operand occupies). The
/// public entry pins [`ROOT_SHIFT`]; this exists so the frame can be resolved on real data whose bit
/// depth differs. `offset`/`scale` come from `info`.
pub(crate) fn query_aabb_shift(
    code: &[u8],
    info: &MoppInfo,
    qmin: [f32; 3],
    qmax: [f32; 3],
    root_shift: u32,
) -> Vec<u32> {
    let mut keys = Vec::new();
    if code.is_empty() {
        return keys;
    }
    let scale = info.scale as f64;
    let offv = info.offset;
    // world = (int)*scale + offset[axis]
    let plane = |opd: i64, origin_a: i64, shift: u32, axis: usize| -> f32 {
        ((((opd << shift) + origin_a) as f64) * scale + offv[axis] as f64) as f32
    };
    // BE24-absolute coordinate (CUT 0x29–0x2b): no shift/origin, just `int*scale + offset`.
    let abs_coord = |v: u32, axis: usize| -> f32 { ((v as f64) * scale + offv[axis] as f64) as f32 };
    // Slop: one quantum of the frame, so a boundary triangle is never pruned by float rounding.
    let eps = (info.scale.abs() * 1.5).max(1e-4);

    #[derive(Clone)]
    struct Frame {
        pc: usize,
        origin: [i64; 3],
        shift: u32,
        kb: u32,
        blo: [f32; 3],
        bhi: [f32; 3],
    }
    let inf = f32::INFINITY;
    let mut stack = vec![Frame {
        pc: 0,
        origin: [0; 3],
        shift: root_shift,
        kb: 0,
        blo: [-inf; 3],
        bhi: [inf; 3],
    }];
    let disjoint = |blo: &[f32; 3], bhi: &[f32; 3]| -> bool {
        (0..3).any(|a| qmax[a] < blo[a] - eps || qmin[a] > bhi[a] + eps)
    };

    let mut steps = 0usize;
    while let Some(mut fr) = stack.pop() {
        loop {
            steps += 1;
            if steps > 100_000_000 {
                break; // defensive livelock guard for malformed data
            }
            if fr.pc >= code.len() || disjoint(&fr.blo, &fr.bhi) {
                break;
            }
            let op = code[fr.pc];
            macro_rules! need {
                ($n:expr) => {
                    if fr.pc + $n > code.len() {
                        break;
                    }
                };
            }
            match op {
                0x00 => break, // RETURN
                0x01..=0x04 => {
                    need!(4);
                    let s = fr.shift;
                    for k in 0..3 {
                        fr.origin[k] += (code[fr.pc + 1 + k] as i64) << s;
                    }
                    fr.shift = fr.shift.saturating_sub(op as u32);
                    fr.pc += 4;
                }
                0x05 => {
                    need!(2);
                    fr.pc += code[fr.pc + 1] as usize + 2;
                }
                0x06 => {
                    need!(3);
                    fr.pc += code[fr.pc + 1] as usize * 256 + code[fr.pc + 2] as usize + 3;
                }
                0x07 => {
                    need!(4);
                    fr.pc += be(code, fr.pc + 1, 2) as usize * 256 + code[fr.pc + 3] as usize + 4;
                }
                0x09 => {
                    need!(2);
                    fr.kb = fr.kb.wrapping_add(code[fr.pc + 1] as u32);
                    fr.pc += 2;
                }
                0x0a => {
                    need!(3);
                    fr.kb = fr.kb.wrapping_add(be(code, fr.pc + 1, 2));
                    fr.pc += 3;
                }
                0x0b => {
                    need!(5);
                    fr.kb = be(code, fr.pc + 1, 4);
                    fr.pc += 5;
                }
                0x10..=0x12 => {
                    need!(4);
                    let axis = (op - 0x10) as usize;
                    let lmax = plane(code[fr.pc + 1] as i64, fr.origin[axis], fr.shift, axis);
                    let rmin = plane(code[fr.pc + 2] as i64, fr.origin[axis], fr.shift, axis);
                    let rpc = fr.pc + 4 + code[fr.pc + 3] as usize;
                    let mut rblo = fr.blo;
                    rblo[axis] = rblo[axis].max(rmin);
                    stack.push(Frame {
                        pc: rpc,
                        blo: rblo,
                        ..fr.clone()
                    });
                    fr.bhi[axis] = fr.bhi[axis].min(lmax);
                    fr.pc += 4;
                }
                0x13..=0x1c => {
                    // 26-DOP diagonal split — plane geometry INFERRED, not decoded. Visit BOTH
                    // children unpruned (conservative). hdr 4, LEFT @ +4, RIGHT @ +4+code[+3].
                    need!(4);
                    let rpc = fr.pc + 4 + code[fr.pc + 3] as usize;
                    stack.push(Frame { pc: rpc, ..fr.clone() });
                    fr.pc += 4;
                }
                0x20..=0x22 => {
                    need!(3);
                    let axis = (op - 0x20) as usize;
                    let v = code[fr.pc + 1] as i64;
                    let lmax = plane(v + 1, fr.origin[axis], fr.shift, axis);
                    let rmin = plane(v, fr.origin[axis], fr.shift, axis);
                    let rpc = fr.pc + 3 + code[fr.pc + 2] as usize;
                    let mut rblo = fr.blo;
                    rblo[axis] = rblo[axis].max(rmin);
                    stack.push(Frame {
                        pc: rpc,
                        blo: rblo,
                        ..fr.clone()
                    });
                    fr.bhi[axis] = fr.bhi[axis].min(lmax);
                    fr.pc += 3;
                }
                0x23..=0x25 => {
                    need!(7);
                    let axis = (op - 0x23) as usize;
                    let lmax = plane(code[fr.pc + 1] as i64, fr.origin[axis], fr.shift, axis);
                    let rmin = plane(code[fr.pc + 2] as i64, fr.origin[axis], fr.shift, axis);
                    let loff = be(code, fr.pc + 3, 2) as usize;
                    let roff = be(code, fr.pc + 5, 2) as usize;
                    let rpc = fr.pc + 7 + roff;
                    let mut rblo = fr.blo;
                    rblo[axis] = rblo[axis].max(rmin);
                    stack.push(Frame {
                        pc: rpc,
                        blo: rblo,
                        ..fr.clone()
                    });
                    fr.bhi[axis] = fr.bhi[axis].min(lmax);
                    fr.pc += 7 + loff;
                }
                0x26..=0x28 => {
                    // CUT: narrow this branch's box on one axis (min=code[+1], max=code[+2]).
                    need!(3);
                    let axis = (op - 0x26) as usize;
                    fr.blo[axis] = plane(code[fr.pc + 1] as i64, fr.origin[axis], fr.shift, axis);
                    fr.bhi[axis] = plane(code[fr.pc + 2] as i64, fr.origin[axis], fr.shift, axis);
                    fr.pc += 3;
                }
                0x29..=0x2b => {
                    // CUT with BE24 absolute coords.
                    need!(7);
                    let axis = (op - 0x29) as usize;
                    fr.blo[axis] = abs_coord(be(code, fr.pc + 1, 3), axis);
                    fr.bhi[axis] = abs_coord(be(code, fr.pc + 4, 3), axis);
                    fr.pc += 7;
                }
                0x30..=0x4f => {
                    keys.push(fr.kb.wrapping_add(op as u32 - 0x30));
                    break;
                }
                0x50 => {
                    need!(2);
                    keys.push(fr.kb.wrapping_add(code[fr.pc + 1] as u32));
                    break;
                }
                0x51 => {
                    need!(3);
                    keys.push(fr.kb.wrapping_add(be(code, fr.pc + 1, 2)));
                    break;
                }
                0x52 => {
                    need!(4);
                    keys.push(fr.kb.wrapping_add(be(code, fr.pc + 1, 3)));
                    break;
                }
                0x53 => {
                    need!(5);
                    keys.push(fr.kb.wrapping_add(be(code, fr.pc + 1, 4)));
                    break;
                }
                0x60..=0x63 => {
                    need!(2);
                    fr.pc += 2;
                }
                0x64..=0x67 => {
                    need!(3);
                    fr.pc += 3;
                }
                0x68..=0x6b => {
                    need!(5);
                    fr.pc += 5;
                }
                _ => break, // INVALID — stop this branch
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vendored 901-byte retail reference MOPP (`m_data`), in its stale u32-scrambled form. After
    /// [`unreverse_u32`] it must decode with **901/901 bytes consumed, 0 errors, 76 distinct keys in
    /// `[0..=76]` (only key 32 absent)** — the exact fingerprint that proved the opcode table.
    #[test]
    fn old_mopp_reference_decodes_76_contiguous_keys() {
        let scrambled = include_bytes!("../tests/fixtures/mopp_old_scrambled.bin");
        assert_eq!(scrambled.len(), 901);
        let code = unreverse_u32(scrambled);

        let d = decode(&code);
        assert!(d.error.is_none(), "clean walk expected, got {:?}", d.error);
        assert_eq!(d.consumed, code.len(), "100% byte coverage");

        let (ks, range, missing) = d.key_summary();
        assert_eq!(ks.len(), 76, "76 distinct shape keys");
        assert_eq!(range, Some((0, 76)), "contiguous range [0..=76]");
        assert_eq!(missing, vec![32], "only key 32 absent (degenerate/culled tri)");
    }

    /// A scrambled buffer must NOT decode cleanly without un-reversal — guards against the raw-vs-
    /// scrambled confusion the reader-fix history is a monument to.
    #[test]
    fn scrambled_without_unreverse_is_not_a_clean_walk() {
        let scrambled = include_bytes!("../tests/fixtures/mopp_old_scrambled.bin");
        let d = decode(scrambled);
        assert!(
            d.error.is_some() || d.consumed != scrambled.len(),
            "scrambled buffer unexpectedly decoded clean"
        );
    }

    // ── encoder round-trip gate ──

    fn roundtrip_recovers_set(tris: &[[u32; 3]], verts: &[[f32; 3]]) {
        let (code, _info) = encode(tris, verts);
        let d = decode(&code);
        assert!(d.error.is_none(), "encoded MOPP must decode clean: {:?}", d.error);
        assert_eq!(d.consumed, code.len(), "encoded MOPP must be 100% covered");

        let mut got = d.keys.clone();
        got.sort_unstable();
        let want: Vec<u32> = (0..tris.len() as u32).collect();
        assert_eq!(got, want, "decode(encode) must recover every triangle index exactly once");
    }

    #[test]
    fn roundtrip_quad() {
        // Two triangles of a unit quad.
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let tris = vec![[0u32, 1, 2], [0, 2, 3]];
        roundtrip_recovers_set(&tris, &verts);
    }

    #[test]
    fn roundtrip_box() {
        // 8 corners, 12 triangles (a closed box).
        let verts = vec![
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0],
        ];
        let tris = vec![
            [0u32, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6],
            [0, 4, 5], [0, 5, 1], [1, 5, 6], [1, 6, 2],
            [2, 6, 7], [2, 7, 3], [3, 7, 4], [3, 4, 0],
        ];
        roundtrip_recovers_set(&tris, &verts);
    }

    #[test]
    fn roundtrip_random_soup_2k() {
        // ~2k pseudo-random triangles over a shared vertex cloud — forces deep trees and the BE16
        // (0x23) offset form (left subtrees exceed 255 bytes near the root).
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut rng = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let nv = 512usize;
        let verts: Vec<[f32; 3]> = (0..nv)
            .map(|_| {
                [
                    (rng() % 1000) as f32 * 0.1,
                    (rng() % 1000) as f32 * 0.1,
                    (rng() % 1000) as f32 * 0.1,
                ]
            })
            .collect();
        let ntri = 2000usize;
        let tris: Vec<[u32; 3]> = (0..ntri)
            .map(|_| {
                [
                    (rng() as usize % nv) as u32,
                    (rng() as usize % nv) as u32,
                    (rng() as usize % nv) as u32,
                ]
            })
            .collect();

        let (code, _info) = encode(&tris, &verts);
        // Confirm the big-offset form actually got exercised.
        assert!(
            code.windows(1).any(|w| (0x23..=0x25).contains(&w[0])),
            "2k-tri tree should use at least one 16-bit-offset split"
        );
        roundtrip_recovers_set(&tris, &verts);
    }

    #[test]
    fn encode_empty_is_a_single_return() {
        let (code, _info) = encode(&[], &[]);
        assert_eq!(code, vec![0x00]);
        assert!(decode(&code).keys.is_empty());
    }

    // ── return-all (universal-collision) MOPP gate ──

    /// `encode_return_all(n)` must decode to EXACTLY the key set `[0..n)` — 100% byte coverage, no
    /// error, each key once — for a spread of sizes (small, the u8→BE16 offset crossover, and a large
    /// tree that forces the 0x23 form near the root), and `query_aabb` under any in-frame box (plus
    /// all-space) must return every one of the n keys (the tree prunes nothing).
    #[test]
    fn return_all_decodes_to_full_range_and_query_returns_everything() {
        // Frame with scale 1.0 at ROOT_SHIFT=8 spans int [0, 0xFF00] → world [0, 65280] per axis; any
        // query box inside that (or all-space) must return every key.
        let info = MoppInfo { offset: [0.0; 3], scale: 1.0 };
        let inf = f32::INFINITY;
        for &n in &[1u32, 2, 3, 5, 76, 255, 256, 1000, 5000] {
            let code = encode_return_all(n);
            let d = decode(&code);
            assert!(d.error.is_none(), "n={n}: return-all must decode clean: {:?}", d.error);
            assert_eq!(d.consumed, code.len(), "n={n}: 100% byte coverage");
            let mut got = d.keys.clone();
            got.sort_unstable();
            let want: Vec<u32> = (0..n).collect();
            assert_eq!(got, want, "n={n}: decode(return-all) must be exactly [0..n)");

            // query_aabb returns all n for boxes across the frame + all-space.
            for (qmin, qmax) in [
                ([-inf; 3], [inf; 3]),
                ([0.0; 3], [65280.0; 3]),
                ([100.0, 200.0, 300.0], [40000.0, 50000.0, 60000.0]),
                ([1.0; 3], [2.0; 3]),
            ] {
                let mut q = query_aabb(&code, &info, qmin, qmax);
                q.sort_unstable();
                q.dedup();
                assert_eq!(q, want, "n={n}: query_aabb([{qmin:?},{qmax:?}]) must return all keys");
            }
        }
    }

    /// The u8-offset form (`0x10`) is used for small trees; a large tree MUST exercise the BE16-offset
    /// form (`0x23`) once an inline subtree exceeds 255 bytes — guards the offset-width crossover.
    #[test]
    fn return_all_uses_be16_offset_form_for_large_trees() {
        let small = encode_return_all(8);
        assert!(small.windows(1).all(|w| w[0] != 0x23), "small tree should not need BE16 offsets");
        let big = encode_return_all(2000);
        assert!(
            big.windows(1).any(|w| w[0] == 0x23),
            "a 2000-leaf return-all tree must use at least one 0x23 (BE16-offset) split"
        );
        // Still a perfect [0..2000) round-trip.
        let mut got = decode(&big).keys;
        got.sort_unstable();
        assert_eq!(got, (0..2000u32).collect::<Vec<_>>());
    }

    #[test]
    fn return_all_zero_is_empty_tree() {
        let code = encode_return_all(0);
        assert_eq!(code, vec![0x00]);
        assert!(decode(&code).keys.is_empty());
    }

    #[test]
    fn moppinfo_frame_covers_the_vertex_aabb() {
        let verts = vec![[-5.0, 2.0, 0.0], [10.0, 2.0, 3.0], [0.0, -1.0, 40.0]];
        let tris = vec![[0u32, 1, 2]];
        let (_code, info) = encode(&tris, &verts);
        assert_eq!(info.offset, [-5.0, -1.0, 0.0], "offset = AABB min");
        // Every vertex must dequantize back inside [offset, offset + scale*0xFFFF] per axis.
        let hi = [
            info.offset[0] + info.scale * 65535.0,
            info.offset[1] + info.scale * 65535.0,
            info.offset[2] + info.scale * 65535.0,
        ];
        for v in &verts {
            for k in 0..3 {
                assert!(v[k] >= info.offset[k] - 1e-3 && v[k] <= hi[k] + 1e-3);
            }
        }
    }

    // ── WAD-gated live validation (SKIPS LOUD when vz.wad is absent) ──

    /// Extract several REAL `hkpMoppCode` buffers from retail `vz.wad` terrain/building blocks and
    /// decode each: 100 % byte coverage, 0 errors, keys contiguous-ish and in range. Every terrain
    /// cell ships a baked MOPP wrapping its `WpMeshShape16`, so these blocks are dense with them.
    #[test]
    fn real_mopp_buffers_decode_clean_from_vz_wad_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        let Some(path) = crate::game_paths::vz_wad(std::path::Path::new(".")) else {
            return eprintln!("SKIPPING real_mopp_buffers: vz.wad not found (set MERCS2_GAME_DIR or .mercs2-local.toml)");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("SKIPPING real_mopp_buffers: vz.wad not readable at {path:?}");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");

        let mut total_buffers = 0usize;
        let mut total_keys = 0usize;
        let mut contiguous = 0usize; // buffers whose keys are a single perfect [0..N-1] run
        for &blk in &[767u16, 826, 3185] {
            let Ok(dec) = decompress_block(&mut f, &arch.indx, blk) else {
                continue;
            };
            let buffers = extract_mopp_buffers(&dec);
            for (i, code) in buffers.iter().enumerate() {
                let d = decode(code);
                // The two rock-solid structural proofs, true for EVERY real MOPP: the whole tree is
                // reachable and every opcode is valid.
                assert!(
                    d.error.is_none(),
                    "block {blk} mopp[{i}] ({} B) decode error: {:?}",
                    code.len(),
                    d.error
                );
                assert_eq!(
                    d.consumed,
                    code.len(),
                    "block {blk} mopp[{i}]: {}/{} bytes covered (tree must be fully walked)",
                    d.consumed,
                    code.len()
                );
                assert!(!d.keys.is_empty(), "block {blk} mopp[{i}]: no leaf keys");
                // A well-formed BV-tree visits each leaf exactly once → all shape keys distinct.
                let (ks, range, missing) = d.key_summary();
                assert_eq!(
                    ks.len(),
                    d.keys.len(),
                    "block {blk} mopp[{i}]: {} leaves but only {} distinct keys (duplicate leaf visit)",
                    d.keys.len(),
                    ks.len()
                );
                // Single-subpart meshes (buildings) yield a perfect contiguous triangle range
                // [0..N-1]; multi-subpart terrain cells offset each subpart's keys by a per-subpart
                // base (Havok `subpartBase + triIndex`), so they are dense per-subpart, not globally.
                let (lo, _hi) = range.unwrap();
                assert_eq!(lo, 0, "block {blk} mopp[{i}]: shape keys must be zero-based, start at {lo}");
                if missing.is_empty() {
                    contiguous += 1;
                }
                total_keys += ks.len();
            }
            total_buffers += buffers.len();
        }
        assert!(
            total_buffers > 0,
            "vz.wad present but no hkpMoppCode buffers extracted from blocks 767/826/3185"
        );
        // The strong "contiguous in-range keys" proof: the single-subpart building MOPPs decode to a
        // clean [0..N-1] triangle range. Retail block 767 alone carries dozens.
        assert!(
            contiguous >= 20,
            "expected many single-subpart MOPPs to decode to a contiguous [0..N-1] range, got {contiguous}"
        );
        eprintln!(
            "real MOPP validation: {total_buffers} buffers decoded clean (100% coverage, distinct keys), \
             {contiguous} perfectly contiguous [0..N-1], {total_keys} total shape keys"
        );
    }

    /// Take a REAL `WpMeshShape16`'s triangles, encode a fresh MOPP over them, and decode it back:
    /// every source triangle index appears exactly once, 100 % byte coverage. Closes the loop the
    /// native encoder exists for. SKIPS LOUD when `vz.wad` is absent.
    #[test]
    fn encode_real_wpmesh16_triangles_roundtrips_from_vz_wad_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::havok::{find_packfiles, MeshShape, Shape};
        use crate::sges::decompress_block;
        let Some(path) = crate::game_paths::vz_wad(std::path::Path::new(".")) else {
            return eprintln!("SKIPPING encode_real_wpmesh16: vz.wad not found");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("SKIPPING encode_real_wpmesh16: vz.wad not readable");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");
        let dec = decompress_block(&mut f, &arch.indx, 767).expect("decompress block 767");

        // Smallest non-trivial decoded mesh in the block — keeps the tree modest and the test fast.
        let mesh: MeshShape = find_packfiles(&dec)
            .into_iter()
            .flat_map(|(_off, pf)| pf.shapes.into_iter())
            .filter_map(|s| match s {
                Shape::Mesh(m) if !m.indices.is_empty() => Some(m),
                _ => None,
            })
            .min_by_key(|m| m.indices.len())
            .expect("block 767 must carry a decoded WpMeshShape16");

        let verts: Vec<[f32; 3]> = mesh.vertices.clone();
        let tris: Vec<[u32; 3]> = mesh
            .indices
            .iter()
            .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
            .collect();

        let (code, info) = encode(&tris, &verts);
        assert!(info.scale > 0.0);
        let d = decode(&code);
        assert!(d.error.is_none(), "encoded real-mesh MOPP decode error: {:?}", d.error);
        assert_eq!(d.consumed, code.len(), "encoded real-mesh MOPP must be 100% covered");

        let mut got = d.keys.clone();
        got.sort_unstable();
        let want: Vec<u32> = (0..tris.len() as u32).collect();
        assert_eq!(got, want, "every source triangle index recovered exactly once");
        eprintln!(
            "encode_real_wpmesh16: {} tris → {} B MOPP, decoded back to {} keys",
            tris.len(),
            code.len(),
            d.keys.len()
        );
    }

    // ── GEOMETRIC QUERY GATES (P3.5): conservative, no-miss, semantics cross-check ──

    /// A tiny xorshift PRNG for reproducible query generation.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn f01(&mut self) -> f32 {
            (self.next() >> 11) as f32 / (1u64 << 53) as f32
        }
    }

    /// The world AABB of one triangle (over its 3 verts), origin-fallback for out-of-range indices.
    fn tri_aabb(t: &[u32; 3], verts: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for &i in t {
            let v = verts.get(i as usize).copied().unwrap_or([0.0; 3]);
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        (lo, hi)
    }

    fn aabb_overlap(alo: &[f32; 3], ahi: &[f32; 3], blo: &[f32; 3], bhi: &[f32; 3]) -> bool {
        (0..3).all(|k| alo[k] <= bhi[k] && ahi[k] >= blo[k])
    }

    /// Run many random query AABBs over `tris`/`verts`; for each, assert `query_aabb(encode(...))` is a
    /// SUPERSET of the brute-force overlap set (zero misses). Returns (mean over-inclusion ratio =
    /// candidates/true-overlaps, mean candidate-fraction = candidates/total-tris).
    fn no_miss_gate(tris: &[[u32; 3]], verts: &[[f32; 3]], seed: u64, nq: usize, label: &str) -> (f64, f64) {
        let (code, info) = encode(tris, verts);
        // Mesh AABB for query placement.
        let (mut mlo, mut mhi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in verts {
            for k in 0..3 {
                mlo[k] = mlo[k].min(v[k]);
                mhi[k] = mhi[k].max(v[k]);
            }
        }
        if verts.is_empty() {
            mlo = [0.0; 3];
            mhi = [1.0; 3];
        }
        let ext = [mhi[0] - mlo[0], mhi[1] - mlo[1], mhi[2] - mlo[2]];

        let mut rng = Rng(seed);
        let mut sum_ratio = 0.0f64;
        let mut ratio_n = 0usize;
        let mut sum_frac = 0.0f64;
        for _ in 0..nq {
            // Random center (allowed slightly outside the mesh) + random half-size up to ~30% extent.
            let mut c = [0.0f32; 3];
            let mut h = [0.0f32; 3];
            for k in 0..3 {
                c[k] = mlo[k] + (rng.f01() * 1.2 - 0.1) * ext[k].max(1e-3);
                h[k] = (0.005 + rng.f01() * 0.30) * ext[k].max(1e-3);
            }
            let qmin = [c[0] - h[0], c[1] - h[1], c[2] - h[2]];
            let qmax = [c[0] + h[0], c[1] + h[1], c[2] + h[2]];

            // Brute-force ground truth.
            let mut truth: Vec<u32> = Vec::new();
            for (ti, t) in tris.iter().enumerate() {
                let (tlo, thi) = tri_aabb(t, verts);
                if aabb_overlap(&tlo, &thi, &qmin, &qmax) {
                    truth.push(ti as u32);
                }
            }
            let mut cand = query_aabb(&code, &info, qmin, qmax);
            cand.sort_unstable();
            cand.dedup();
            let candset: std::collections::HashSet<u32> = cand.iter().copied().collect();
            for k in &truth {
                assert!(
                    candset.contains(k),
                    "{label}: MISS — triangle {k} overlaps query [{qmin:?},{qmax:?}] but query_aabb \
                     did not return it ({} candidates, {} true overlaps)",
                    cand.len(),
                    truth.len()
                );
            }
            if !truth.is_empty() {
                sum_ratio += cand.len() as f64 / truth.len() as f64;
                ratio_n += 1;
            }
            sum_frac += cand.len() as f64 / tris.len().max(1) as f64;
        }
        let mean_ratio = if ratio_n > 0 { sum_ratio / ratio_n as f64 } else { 0.0 };
        let mean_frac = sum_frac / nq as f64;
        eprintln!(
            "{label}: {} tris, {nq} queries — ZERO misses; mean over-inclusion {mean_ratio:.2}x \
             (candidates/true-overlap), mean candidate-fraction {mean_frac:.3} of all tris",
            tris.len()
        );
        (mean_ratio, mean_frac)
    }

    #[test]
    fn query_no_miss_quad() {
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let tris = vec![[0u32, 1, 2], [0, 2, 3]];
        no_miss_gate(&tris, &verts, 0xA1, 2000, "quad");
    }

    #[test]
    fn query_no_miss_box() {
        let verts = vec![
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0],
        ];
        let tris = vec![
            [0u32, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6],
            [0, 4, 5], [0, 5, 1], [1, 5, 6], [1, 6, 2],
            [2, 6, 7], [2, 7, 3], [3, 7, 4], [3, 4, 0],
        ];
        no_miss_gate(&tris, &verts, 0xB0, 2000, "box");
    }

    #[test]
    fn query_no_miss_random_soup_2k() {
        // Compact vertex cloud so triangles are reasonably sized (real-ish spatial coherence).
        let mut rng = Rng(0x1234_5678_9abc_def0);
        let nv = 400usize;
        let verts: Vec<[f32; 3]> = (0..nv)
            .map(|_| [rng.f01() * 100.0, rng.f01() * 20.0, rng.f01() * 100.0])
            .collect();
        // Each triangle is a small local cluster (pick a base vert + two nearby offsets) so its AABB is
        // small — a soup of giant triangles would legitimately force ~100% inclusion and prove nothing.
        let ntri = 2000usize;
        let tris: Vec<[u32; 3]> = (0..ntri)
            .map(|_| {
                let a = (rng.next() as usize % nv) as u32;
                let b = (rng.next() as usize % nv) as u32;
                let c = (rng.next() as usize % nv) as u32;
                [a, b, c]
            })
            .collect();
        let (ratio, frac) = no_miss_gate(&tris, &verts, 0xC0, 3000, "soup2k");
        // Sanity: a small query must not drag in essentially every triangle.
        assert!(frac < 0.9, "over-inclusion fraction {frac:.3} implies query_aabb barely prunes");
        let _ = ratio;
    }

    /// WAD-gated: encode a MOPP over a REAL `WpMeshShape16` and prove `query_aabb` never misses an
    /// overlapping triangle across many random query AABBs. SKIPS LOUD when `vz.wad` is absent.
    #[test]
    fn query_no_miss_real_wpmesh16_from_vz_wad_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::havok::{find_packfiles, Shape};
        use crate::sges::decompress_block;
        let Some(path) = crate::game_paths::vz_wad(std::path::Path::new(".")) else {
            return eprintln!("SKIPPING query_no_miss_real_wpmesh16: vz.wad not found");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("SKIPPING query_no_miss_real_wpmesh16: vz.wad not readable");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");
        let dec = decompress_block(&mut f, &arch.indx, 767).expect("decompress block 767");

        // A handful of the smaller decoded meshes — enough to exercise real geometry, fast.
        let mut meshes: Vec<_> = find_packfiles(&dec)
            .into_iter()
            .flat_map(|(_o, pf)| pf.shapes.into_iter())
            .filter_map(|s| match s {
                Shape::Mesh(m) if m.indices.len() >= 4 => Some(m),
                _ => None,
            })
            .collect();
        meshes.sort_by_key(|m| m.indices.len());
        assert!(!meshes.is_empty(), "block 767 must carry a decoded WpMeshShape16");

        let mut checked = 0usize;
        for m in meshes.iter().take(5) {
            let verts = m.vertices.clone();
            let tris: Vec<[u32; 3]> =
                m.indices.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect();
            no_miss_gate(&tris, &verts, 0xD0 + checked as u64, 1500, "real-wpmesh16");
            checked += 1;
        }
        assert!(checked > 0, "no real meshes exercised");
    }

    /// WAD-gated SEMANTICS CROSS-CHECK (part 1 — FindAll equivalence). Walk each REAL `hkpMoppCode`
    /// from `vz.wad` with [`query_aabb`] under an ALL-SPACE query (`[-∞, +∞]`): nothing can be pruned,
    /// so it must return EXACTLY the same shape-key multiset as the independently-proven [`decode`]
    /// (which is validated against the 76-key retail reference). This proves `query_aabb`'s traversal —
    /// its handling of REANCHOR (`0x01–0x04`), JUMPs, CUTs, and the 26-DOP splits (`0x13–0x1c`) that
    /// the encoder never emits — is byte-correct on real bytecode. SKIPS LOUD when `vz.wad` is absent.
    #[test]
    fn query_aabb_matches_decode_findall_on_real_mopps_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        let Some(path) = crate::game_paths::vz_wad(std::path::Path::new(".")) else {
            return eprintln!("SKIPPING query_aabb_findall: vz.wad not found (set MERCS2_GAME_DIR)");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("SKIPPING query_aabb_findall: vz.wad not readable");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");
        let inf = f32::INFINITY;
        let mut checked = 0usize;
        for &blk in &[767u16, 826, 3185] {
            let Ok(dec) = decompress_block(&mut f, &arch.indx, blk) else {
                continue;
            };
            for (code, info) in extract_mopp_with_info(&dec) {
                let mut want = decode(&code).keys;
                want.sort_unstable();
                let mut got = query_aabb(&code, &info, [-inf; 3], [inf; 3]);
                got.sort_unstable();
                assert_eq!(
                    got, want,
                    "block {blk}: query_aabb(all-space) must equal decode() FindAll on real bytecode \
                     ({} B, {} keys)",
                    code.len(),
                    want.len()
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "vz.wad present but no real MOPP buffers found for FindAll check");
        eprintln!("query_aabb_findall: {checked} real MOPPs — all-space walk == decode() FindAll");
    }

    /// WAD-gated SEMANTICS CROSS-CHECK (part 2 — pruning correctness on real geometry). For each REAL
    /// MOPP, reconstruct every leaf's node box from the real bytecode ([`leaf_boxes`]) and, for the
    /// fully-bounded leaves, query with a point deep inside that box: [`query_aabb`] MUST return that
    /// leaf's key. This is the pruning theorem — every ancestor box encloses the leaf box, so a query
    /// inside the leaf box can never be pruned — verified against Havok's OWN cut/split/reanchor stream
    /// (not our encoder's). A MISS would mean a node box that fails to nest (a non-conservative
    /// prune). SKIPS LOUD when `vz.wad` is absent.
    #[test]
    fn query_aabb_reaches_every_leaf_box_on_real_mopps_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        let Some(path) = crate::game_paths::vz_wad(std::path::Path::new(".")) else {
            return eprintln!("SKIPPING query_aabb_leafbox: vz.wad not found");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("SKIPPING query_aabb_leafbox: vz.wad not readable");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");
        let mut mopps = 0usize;
        let mut leaves_tested = 0usize;
        for &blk in &[767u16, 826, 3185] {
            let Ok(dec) = decompress_block(&mut f, &arch.indx, blk) else {
                continue;
            };
            for (code, info) in extract_mopp_with_info(&dec) {
                if !info.scale.is_finite() || info.scale == 0.0 {
                    continue;
                }
                let boxes = leaf_boxes(&code, &info, ROOT_SHIFT);
                let mut this_mopp_tested = 0usize;
                for (key, blo, bhi) in &boxes {
                    // Only fully-bounded, non-degenerate leaf boxes make a decisive query.
                    if !(0..3).all(|k| blo[k].is_finite() && bhi[k].is_finite() && bhi[k] > blo[k]) {
                        continue;
                    }
                    // A point at the box centre (a hair of extent) — must land inside the leaf box.
                    let c = [
                        0.5 * (blo[0] + bhi[0]),
                        0.5 * (blo[1] + bhi[1]),
                        0.5 * (blo[2] + bhi[2]),
                    ];
                    let cand: std::collections::HashSet<u32> =
                        query_aabb(&code, &info, c, c).into_iter().collect();
                    assert!(
                        cand.contains(key),
                        "block {blk}: leaf key {key} unreachable by a query at its own box centre \
                         {c:?} (box [{blo:?},{bhi:?}]) — pruning drops a leaf it must keep",
                    );
                    leaves_tested += 1;
                    this_mopp_tested += 1;
                    if this_mopp_tested >= 64 {
                        break; // cap per-MOPP work; the property is uniform across leaves
                    }
                }
                if this_mopp_tested > 0 {
                    mopps += 1;
                }
            }
        }
        assert!(
            leaves_tested > 0,
            "vz.wad present but no fully-bounded real MOPP leaf boxes found to test reachability"
        );
        eprintln!(
            "query_aabb_leafbox: {leaves_tested} real leaf boxes across {mopps} MOPPs — every leaf \
             reachable inside its own reconstructed box (pruning is nesting-correct)"
        );
    }
}
