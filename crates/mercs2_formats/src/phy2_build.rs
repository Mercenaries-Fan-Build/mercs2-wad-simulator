//! **Author a whole `PHY2` collision chunk from a triangle mesh** — the write side that closes the
//! "collision follows new model geometry" gap ([[static-mopp-mesh-collision-binding]]).
//!
//! [`crate::phy2_moppswap`] can only swap a MOPP into an *existing* mesh + wrapper; [`crate::havok_write`]
//! serializes a packfile container but emits no collision objects; [`crate::mopp`] bakes the BV-tree
//! bytecode. This module is the missing piece: given model-local `tris` + `verts` it emits
//!
//! 1. a **`WpMeshShape16`** object (quantized `min`/`scale` frame, `u16` triangle-index array, one
//!    subpart) plus its **out-of-band quantized vertex pool** in the trailing engine wrapper, and
//! 2. a **whole `PHY2` body** `[prefix][Havok packfile][wrapper]` whose packfile object graph is the
//!    proven single-static-collider shape `WpArray → hkpMoppBvTreeShape → {hkpMoppCode, WpMeshShape16}`,
//!    with a MOPP baked over the SAME triangle order as the index array.
//!
//! Everything is gated OFFLINE by round-tripping the output back through the reader
//! ([`crate::havok::parse_phy2_body`] / `decode_mesh_shape16`) and the MOPP walker ([`crate::mopp`]).
//!
//! ## Packfile object graph (PROVEN on retail `vz.wad` block 826, a single-mesh terrain cell)
//! ```text
//!   WpArray(root)  @0    : hkArray<hkpShape*>{ ptr@+8, size@+12=1, cap@+16=0xC0000001 }
//!     └ elem @+32        : one u32 pointer  ──(global fixup)──▶ hkpMoppBvTreeShape
//!   hkpMoppBvTreeShape @48: 64 bytes, all-zero on disk; two global-fixup pointers:
//!         +16 ──▶ hkpMoppCode      (m_code)
//!         +52 ──▶ WpMeshShape16    (hkpSingleShapeContainer child)
//!     (type=0xb / m_info are set by the load-time ctor FUN_00a0cc30, NOT stored on disk)
//!   hkpMoppCode @112     : +16 m_info=[off.x,off.y,off.z, 1/scale] · +32 m_data ptr · +36 count ·
//!                          +40 capAndFlags=0xC0000000|count · +44 buildType=1
//!   WpMeshShape16        : +24 convexRadius=0.01f · +28 subpart-array ptr(→+48) · +32 nsub · +36 cap ·
//!                          +40 (runtime) vertex-pool base ptr — 0 on disk, wired at load from the wrapper
//!     subpart @+48 (48B) : +0 min[3] · +16 scale[3] · +28 1.0f · +32 idx-array ptr · +36 acnt ·
//!                          +40 secondary-array ptr · +44 secondary count
//! ```
//! **The subpart secondary array (`+40` ptr / `+44` count = object `+88`/`+92`) is REQUIRED for collision.**
//! `WpMeshShape16::getAabb` (`FUN_00725ba0`) computes the shape's bounding box by iterating it as a u16
//! vertex-index list into the vertex pool (object `+0x28`); an empty one leaves the AABB inverted
//! (`+FLT_MAX .. -FLT_MAX`), so the broadphase never reports the shape and the player falls through. We emit
//! it as the full index list `0..nverts` (retail stores a smaller extremal subset; the full list is a correct
//! conservative superset). PROVEN by decomp + byte-diff of retail floor `0x39AF17DC`; see
//! [`WpMesh16::secondary_bytes`] and `Temp/hunt/hunt_collide/progress.md`.
//!
//! **Deltas from retail we DO NOT reproduce** (documented, harmless to the offline gate): the trailing
//! wrapper's exact Havok median-split BV-tree geometry (we emit a proven-edge caterpillar with the same node
//! count/offsets); and the packfile's full 19-entry class hierarchy is reproduced verbatim, but only 4
//! classes are actually referenced. See [`build_phy2`] and the module tests.

use crate::havok_write::{write_classnames, write_packfile, DataSection};
use crate::mopp;

/// `0.01f` — the per-mesh convex/collision radius every retail `WpMeshShape16` carries at `obj+24`.
const CONVEX_RADIUS: f32 = 0.01;

/// The constant hash both retail single-mesh trailing-wrapper AA records carry at record+32
/// (0xE8EB75D7 and 0x86D7CF92 both = this). A shape/type tag, reproduced verbatim.
const WRAPPER_HASH: u32 = 0x2063_76f6;
/// `FLT_MAX` bit pattern (0x7f7fffff) — the CC-block "min" AABB scratch the engine overwrites at load.
const FLT_MAX_BITS: u32 = 0x7f7f_ffff;
/// `0.3f` — the four constant floats in the CC scratch block.
const CC_POINT3: u32 = 0x3e99_999a;

/// **Author the FAITHFUL PHY2 trailing engine-wrapper for a single collision mesh.** (Replaces the old
/// pool-first layout that crashed the retail loader at `0x0248C15A`.)
///
/// The retail wrapper is a serialized heap-snapshot of the runtime collision objects, delimited by
/// `0xAA/0xBB/0xCC/0xDD/0xEE` markers, whose pointer fields are **chunk-body-absolute u32 offsets** the
/// engine relocates as `real_ptr = &body[0] + offset`. PROVEN by byte measurement on retail containers
/// `0xE8EB75D7` + `0x86D7CF92` (identical structure); see `docs`/`wrapper_progress.md`.
///
/// `pkend` = the body offset where the wrapper begins (`48 + packfile.len()`); every stored pointer is
/// `pkend + wrapper_relative_offset`. `vmin`/`vmax` = the true float AABB of the mesh. `k` = the AA-record
/// count field (retail 1–2; non-pointer, refcount-like — defaulted by callers). The fixed header is
/// exactly 352 bytes; the pool then follows at wrapper+352.
pub fn build_mesh_wrapper(
    mesh: &WpMesh16,
    vmin: [f32; 3],
    vmax: [f32; 3],
    pkend: usize,
    k: u32,
) -> Vec<u8> {
    let nverts = mesh.pool.len() as u32;
    let ntris = mesh.tris.len() as u32;
    let pool = mesh.pool_bytes();
    // Tail (the {12}{ntris} pair) sits after the pool, 4-byte aligned.
    let pool_start = 352usize;
    let mut tail_off = pool_start + pool.len();
    while !tail_off.is_multiple_of(4) {
        tail_off += 1;
    }
    // body-absolute pointer = pkend + wrapper-relative offset.
    let p = |rel: usize| -> u32 { (pkend + rel) as u32 };
    const FF: u32 = 0xFFFF_FFFF;

    let mut w: Vec<u8> = Vec::with_capacity(tail_off + 8);
    let u32b = |w: &mut Vec<u8>, v: u32| w.extend_from_slice(&v.to_le_bytes());
    let f32b = |w: &mut Vec<u8>, v: f32| w.extend_from_slice(&v.to_le_bytes());

    // ── AA#1 @0 (hkpMoppBvTreeShape node) ──
    u32b(&mut w, 0xAAAA_AAAA);
    w.extend_from_slice(&[0u8; 12]); // vec3 = 0
    u32b(&mut w, 0x8000_0000); // quat.x = -0.0
    u32b(&mut w, 0x8000_0000); // quat.y = -0.0
    u32b(&mut w, 0x8000_0000); // quat.z = -0.0
    f32b(&mut w, 1.0); // quat.w
    u32b(&mut w, WRAPPER_HASH); // +32
    u32b(&mut w, k); // +36
    u32b(&mut w, p(136)); // +40 → CC
    u32b(&mut w, FF); // +44
    u32b(&mut w, FF); // +48
    u32b(&mut w, FF); // +52
    u32b(&mut w, p(68)); // +56 → AA#2
    u32b(&mut w, FF); // +60
    debug_assert_eq!(w.len(), 64);
    u32b(&mut w, 0xBBBB_BBBB); // @64 AA#1 end

    // ── AA#2 @68 (WpMeshShape16 node) ──
    u32b(&mut w, 0xAAAA_AAAA);
    w.extend_from_slice(&[0u8; 12]);
    u32b(&mut w, 0x8000_0000);
    u32b(&mut w, 0x8000_0000);
    u32b(&mut w, 0x8000_0000);
    f32b(&mut w, 1.0);
    u32b(&mut w, WRAPPER_HASH); // +32 (rel +100)
    u32b(&mut w, k); // +36
    u32b(&mut w, FF); // +40
    u32b(&mut w, p(244)); // +44 → EE
    u32b(&mut w, FF); // +48
    u32b(&mut w, p(0)); // +52 → AA#1 (back-link)
    u32b(&mut w, FF); // +56
    u32b(&mut w, FF); // +60
    debug_assert_eq!(w.len(), 132);
    u32b(&mut w, 0xBBBB_BBBB); // @132 AA#2 end

    // ── CC @136 (runtime AABB scratch — constant template the engine overwrites) ──
    u32b(&mut w, 0xCCCC_CCCC);
    for _ in 0..9 {
        u32b(&mut w, FLT_MAX_BITS); // +140..176: 9× FLT_MAX
    }
    for _ in 0..4 {
        u32b(&mut w, 0); // +176..192
    }
    for _ in 0..4 {
        u32b(&mut w, CC_POINT3); // +192..208: 4× 0.3f
    }
    for _ in 0..4 {
        u32b(&mut w, 0); // +208..224
    }
    for _ in 0..3 {
        u32b(&mut w, 0); // +224..236
    }
    u32b(&mut w, 1); // +236
    debug_assert_eq!(w.len(), 240);
    u32b(&mut w, 0xDDDD_DDDD); // @240 CC end

    // ── EE @244 (WpMeshShape16 subpart descriptor) ──
    u32b(&mut w, 0xEEEE_EEEE);
    u32b(&mut w, p(tail_off)); // +4 → {12}
    u32b(&mut w, 1); // +8
    u32b(&mut w, p(tail_off + 4)); // +12 → {ntris}
    u32b(&mut w, 2); // +16
    u32b(&mut w, 0x27); // +20 (constant)
    f32b(&mut w, vmin[0]); // +24 min.x
    f32b(&mut w, vmin[1]);
    f32b(&mut w, vmin[2]);
    f32b(&mut w, vmax[0]); // +36 max.x
    f32b(&mut w, vmax[1]);
    f32b(&mut w, vmax[2]);
    u32b(&mut w, 4); // +48 (constant)
    for _ in 0..4 {
        u32b(&mut w, 0); // +52..68
    }
    u32b(&mut w, nverts); // +68
    u32b(&mut w, 0); // +72
    u32b(&mut w, p(352)); // +76 → vertex pool
    for _ in 0..6 {
        u32b(&mut w, 0); // +80..104
    }
    u32b(&mut w, FF); // +104
    debug_assert_eq!(w.len(), 352, "wrapper header must be exactly 352 bytes before the pool");

    // ── vertex POOL @352 ──
    w.extend_from_slice(&pool);
    while w.len() < tail_off {
        w.push(0);
    }
    // ── tail: {12}{ntris} ──
    u32b(&mut w, 12);
    u32b(&mut w, ntris);
    w
}

/// **Walk the authored trailing wrapper exactly as the retail loader does** and prove every relocated
/// pointer resolves to the object it should. This is the offline gate that the pool-first layout would
/// have FAILED (its AA-record pointer fields were pool bytes → garbage relocation → the in-game AV).
///
/// `body` = the whole PHY2 body. `pkend` = the wrapper start (`48 + packfile.len()`). Returns `Ok` only
/// when: AA#1→+40 lands on `0xCCCCCCCC`; AA#1→+56 lands on AA#2 (`0xAAAAAAAA`); AA#2→+44 lands on the EE
/// block (`0xEEEEEEEE`); EE→+76 lands on a pool whose first vertex equals `mesh.pool[0]`; EE→+4 / EE→+12
/// resolve to the `{12}` / `{ntris}` tail; and every offset is in-bounds.
pub fn validate_wrapper_chain(body: &[u8], pkend: usize, mesh: &WpMesh16) -> Result<(), String> {
    let rd = |abs: usize| -> Result<u32, String> {
        if abs + 4 > body.len() {
            return Err(format!("wrapper ptr target {abs} out of body bounds ({})", body.len()));
        }
        Ok(u32::from_le_bytes(body[abs..abs + 4].try_into().unwrap()))
    };
    // The engine relocates a stored offset O as &body[0] + O. So a pointer field at wrapper-relative
    // position `field_rel` holds an offset O; its target is body[O].
    let follow = |field_rel: usize| -> Result<usize, String> {
        let o = rd(pkend + field_rel)? as usize;
        if o >= body.len() {
            return Err(format!("relocated ptr @wrapper+{field_rel} = body[{o}] out of bounds"));
        }
        Ok(o)
    };
    let expect_marker = |abs: usize, want: u32, what: &str| -> Result<(), String> {
        let got = rd(abs)?;
        if got != want {
            return Err(format!("{what}: expected marker {want:#010x} at body[{abs}], got {got:#010x}"));
        }
        Ok(())
    };

    // AA#1 must be the first bytes of the wrapper.
    expect_marker(pkend, 0xAAAA_AAAA, "AA#1")?;
    // AA#1+40 → CC block.
    let cc = follow(40)?;
    expect_marker(cc, 0xCCCC_CCCC, "AA#1+40→CC")?;
    // AA#1+56 → AA#2.
    let aa2 = follow(56)?;
    expect_marker(aa2, 0xAAAA_AAAA, "AA#1+56→AA#2")?;
    // AA#2+44 → EE block. AA#2 is at wrapper+68, so its +44 field is at wrapper-rel 68+44 = 112.
    let ee = follow(112)?;
    expect_marker(ee, 0xEEEE_EEEE, "AA#2+44→EE")?;
    // AA#2+52 → AA#1 (back-link) at wrapper-rel 68+52 = 120.
    let back = follow(120)?;
    expect_marker(back, 0xAAAA_AAAA, "AA#2+52→AA#1 back-link")?;
    // EE is at wrapper+244; its +76 field (pool ptr) is at wrapper-rel 244+76 = 320.
    let pool = follow(320)?;
    // First quantized vertex must match the mesh pool.
    if pool + 6 > body.len() {
        return Err("EE+76→pool: pool runs past body".into());
    }
    let want0 = &mesh.pool_bytes()[..6.min(mesh.pool.len() * 6)];
    if !want0.is_empty() && &body[pool..pool + want0.len()] != want0 {
        return Err(format!(
            "EE+76→pool @body[{pool}]: first vertex bytes {:02x?} != mesh pool[0] {:02x?}",
            &body[pool..pool + want0.len()],
            want0
        ));
    }
    // EE+4 → {12}, EE+12 → {ntris}. EE at wrapper+244 → fields at wrapper-rel 248 and 256.
    let twelve = follow(248)?;
    if rd(twelve)? != 12 {
        return Err(format!("EE+4→tail: expected 12, got {}", rd(twelve)?));
    }
    let ntris_ptr = follow(256)?;
    if rd(ntris_ptr)? != mesh.tris.len() as u32 {
        return Err(format!(
            "EE+12→tail: expected ntris {}, got {}",
            mesh.tris.len(),
            rd(ntris_ptr)?
        ));
    }
    Ok(())
}

/// Write ONE 68-byte `AA…BB` descriptor record: the constant identity-transform header (vec3 0, quat
/// (-0,-0,-0,1)), the shape hash + `k`, then the six pointer fields at record offsets `+40,+44,+48,+52,
/// +56,+60`, then the `0xBBBBBBBB` terminator. `fields` are the already-resolved values (body-absolute
/// offsets `pkend+rel`, or `0xFFFFFFFF` for an absent link). Byte-structurally identical to the two AA
/// records the IN-GAME-PROVEN single-mesh [`build_mesh_wrapper`] emits — only the six pointer fields vary,
/// which is exactly how retail differentiates an INTERNAL node (`+40→CC`, `+56/+60→children`) from a LEAF
/// (`+44→EE`, `+52→parent`, `+60→sibling`). `rec_rel` = this record's wrapper-relative start; `w.len()`
/// must equal it on entry.
fn write_aa_record(w: &mut Vec<u8>, rec_rel: usize, k: u32, fields: [u32; 6]) {
    debug_assert_eq!(w.len(), rec_rel, "AA record must start at its declared wrapper offset");
    let u32b = |w: &mut Vec<u8>, v: u32| w.extend_from_slice(&v.to_le_bytes());
    let f32b = |w: &mut Vec<u8>, v: f32| w.extend_from_slice(&v.to_le_bytes());
    u32b(w, 0xAAAA_AAAA);
    w.extend_from_slice(&[0u8; 12]); // vec3 = 0
    u32b(w, 0x8000_0000); // quat.x = -0.0
    u32b(w, 0x8000_0000); // quat.y = -0.0
    u32b(w, 0x8000_0000); // quat.z = -0.0
    f32b(w, 1.0); // quat.w
    u32b(w, WRAPPER_HASH); // +32
    u32b(w, k); // +36
    for f in fields {
        u32b(w, f); // +40,+44,+48,+52,+56,+60
    }
    debug_assert_eq!(w.len(), rec_rel + 64);
    u32b(w, 0xBBBB_BBBB); // +64
}

/// Write ONE 108-byte `CC…DD` runtime-AABB scratch block — the constant template every retail internal
/// (`hkpMoppBvTreeShape`) node points to at `+40`; the engine overwrites it with the real union AABB at
/// load. Byte-identical to the CC block [`build_mesh_wrapper`] emits and to all three CC blocks measured
/// in the retail floor wrapper. `rel` = its wrapper-relative start.
fn write_cc_block(w: &mut Vec<u8>, rel: usize) {
    debug_assert_eq!(w.len(), rel, "CC block must start at its declared wrapper offset");
    let u32b = |w: &mut Vec<u8>, v: u32| w.extend_from_slice(&v.to_le_bytes());
    u32b(w, 0xCCCC_CCCC);
    for _ in 0..9 {
        u32b(w, FLT_MAX_BITS); // 9× FLT_MAX
    }
    for _ in 0..4 {
        u32b(w, 0);
    }
    for _ in 0..4 {
        u32b(w, CC_POINT3); // 4× 0.3f
    }
    for _ in 0..4 {
        u32b(w, 0);
    }
    for _ in 0..3 {
        u32b(w, 0);
    }
    u32b(w, 1);
    debug_assert_eq!(w.len(), rel + 104);
    u32b(w, 0xDDDD_DDDD); // @+104 CC end
}

/// Write ONE 108-byte `EE` `WpMeshShape16` subpart descriptor — what a retail LEAF node points to at
/// `+44`. `pool_ptr` / `tail_ptr` are body-absolute (`pkend+rel`) offsets of the vertex pool and this leaf's
/// `{12}{ntris}` tail pair. In the multi-mesh SHARED-pool form every leaf passes the SAME `pool_ptr` +
/// `nverts` (retail's byte-identical EEs); only `tail_ptr` differs per leaf. Byte-identical to the EE block
/// [`build_mesh_wrapper`] emits (the in-game-proven single-mesh subpart). `rel` = its wrapper-relative start.
fn write_ee_block(
    w: &mut Vec<u8>,
    rel: usize,
    nverts: u32,
    vmin: [f32; 3],
    vmax: [f32; 3],
    pool_ptr: u32,
    tail_ptr: u32,
) {
    debug_assert_eq!(w.len(), rel, "EE block must start at its declared wrapper offset");
    const FF: u32 = 0xFFFF_FFFF;
    let u32b = |w: &mut Vec<u8>, v: u32| w.extend_from_slice(&v.to_le_bytes());
    let f32b = |w: &mut Vec<u8>, v: f32| w.extend_from_slice(&v.to_le_bytes());
    u32b(w, 0xEEEE_EEEE);
    u32b(w, tail_ptr); // +4 → {12}
    u32b(w, 1); // +8
    u32b(w, tail_ptr + 4); // +12 → {ntris}
    u32b(w, 2); // +16
    u32b(w, 0x27); // +20
    f32b(w, vmin[0]); // +24 min
    f32b(w, vmin[1]);
    f32b(w, vmin[2]);
    f32b(w, vmax[0]); // +36 max
    f32b(w, vmax[1]);
    f32b(w, vmax[2]);
    u32b(w, 4); // +48
    for _ in 0..4 {
        u32b(w, 0); // +52..68
    }
    u32b(w, nverts); // +68
    u32b(w, 0); // +72
    u32b(w, pool_ptr); // +76 → this leaf's vertex pool
    for _ in 0..6 {
        u32b(w, 0); // +80..104
    }
    u32b(w, FF); // +104
    debug_assert_eq!(w.len(), rel + 108, "EE block must be exactly 108 bytes");
}

/// One collision shape's source geometry: `(triangle-index triples, model-local vertices)`. A whole
/// multi-shape container ([`build_phy2_multi`]) is a slice of these, one per `SEGM` record.
pub type MeshSoup = (Vec<[u32; 3]>, Vec<[f32; 3]>);

/// Which MOPP bytecode to bake into each authored shape's `hkpMoppCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoppKind {
    /// The real **spatial** median-split BVH ([`mopp::encode`]) — collision honours the geometry via the
    /// MOPP coordinate frame (`m_info`). The default; what every shipped authored overlay uses.
    #[default]
    Spatial,
    /// The frame-ignoring **return-all** BVH ([`mopp::encode_return_all`]) — every query visits every
    /// leaf, so the mesh collides for ANY query regardless of the MOPP coordinate frame.
    ///
    /// A **DIAGNOSTIC**: it keeps the SAME `m_info` frame, the SAME quantized mesh, and the SAME trailing
    /// wrapper the spatial path computes, changing ONLY the `hkpMoppCode.m_data` bytecode. Bake it over an
    /// authored floor to isolate a fall-through: if the player then STANDS, the mesh, wrapper, broadphase,
    /// and `m_info` plumbing are all valid and the spatial MOPP frame is the sole culprit; if they still
    /// FALL, the problem is upstream (`m_info`/AABB/mesh/binding). The return-all buffer is emitted in the
    /// SAME footprint the spatial code would occupy (zero-padded if shorter), so every downstream body
    /// offset (mesh, index array, packfile length, trailing wrapper) stays byte-identical to the spatial
    /// build.
    ReturnAll,
}

/// Bake ONE shape's `hkpMoppCode` payload for the requested [`MoppKind`], returning
/// `(m_info, on-disk m_data buffer, m_data count)`.
///
/// The [`mopp::MoppInfo`] frame is ALWAYS the spatial one ([`mopp::encode`]), so `m_info` is byte-identical
/// across kinds. For [`MoppKind::ReturnAll`] the returned buffer is [`mopp::encode_return_all`] placed in
/// the SAME byte footprint the spatial code occupies (zero-padded when shorter — the engine reads exactly
/// `count` bytes via the `hkArray`, so trailing pad is inert), which keeps the packfile length + every later
/// body offset + the trailing wrapper byte-identical to the spatial build. The ONLY authored-body delta
/// between kinds is then the `m_data` bytecode content (and, only if return-all is strictly shorter than
/// spatial, the `count`/`cap` fields). Panics if the return-all code is LONGER than the spatial footprint
/// (it would shift the body layout and break the controlled-test invariant); for every real collision mesh
/// measured the two are equal length, so this never fires.
fn bake_shape_mopp(
    tris: &[[u32; 3]],
    verts: &[[f32; 3]],
    kind: MoppKind,
) -> (mopp::MoppInfo, Vec<u8>, u32) {
    let (spatial, info) = mopp::encode(tris, verts);
    match kind {
        MoppKind::Spatial => {
            let count = spatial.len() as u32;
            (info, spatial, count)
        }
        MoppKind::ReturnAll => {
            let ra = mopp::encode_return_all(tris.len() as u32);
            let count = ra.len() as u32;
            assert!(
                ra.len() <= spatial.len(),
                "return-all MOPP ({} B) exceeds the spatial footprint ({} B) for {} tris — \
                 emitting it would shift the authored body layout and break the controlled diff",
                ra.len(),
                spatial.len(),
                tris.len()
            );
            let mut buf = ra;
            buf.resize(spatial.len(), 0); // pad the spatial footprint (no-op when lengths are equal)
            (info, buf, count)
        }
    }
}

/// The internal-node count for an `n`-leaf merged BV-tree wrapper. A binary tree over `n` leaves has
/// `n-1` internal nodes; the degenerate single-mesh case keeps ONE internal (the `hkpMoppBvTreeShape`
/// root) over its one leaf — the IN-GAME-PROVEN 2-record layout. So `k = max(n-1, 1)`.
#[inline]
fn internal_count(n: usize) -> usize {
    if n <= 1 {
        1
    } else {
        n - 1
    }
}

/// One sub-mesh in the retail SHARED-POOL layout. `tris_global` is the packfile index array in **global**
/// vertex indices (into the one shared pool); `[vbase, vbase+vcount)` is the contiguous global range this
/// sub-mesh owns (its secondary/getAabb vertex-index list). `local_tris`/`local_verts` are the compacted
/// 0-based geometry retained ONLY for baking this sub-mesh's `hkpMoppCode` (keys `[0..ntris)`).
pub struct SubMeshBuild {
    /// Packfile index array in GLOBAL vertex indices (into [`SharedMeshSet::pool`]).
    pub tris_global: Vec<[u16; 3]>,
    /// First global vertex index this sub-mesh owns (its secondary/getAabb list start).
    pub vbase: u32,
    /// Number of vertices this sub-mesh owns (`[vbase, vbase+vcount)`).
    pub vcount: u32,
    /// Compacted 0-based triangles (for baking this sub's `hkpMoppCode`, keys `[0..ntris)`).
    pub local_tris: Vec<[u32; 3]>,
    /// Compacted local vertices, in ascending original-index order.
    pub local_verts: Vec<[f32; 3]>,
}

/// The retail multi-mesh collision layout — **ONE quantized vertex pool under a COMMON quantization frame**,
/// which every sub-mesh's index array addresses with GLOBAL indices — byte-measured on floor `0x39AF17DC`:
/// all 4 `WpMeshShape16` subparts carry the SAME `min`/`scale`, their index arrays partition one 2817-vertex
/// pool (mesh0 `0..2285`, mesh1 `2286..2571`, …), and all 4 wrapper `EE` records point to that ONE shared
/// pool (`EE+76 → +1232`, `nverts=2817`). This is what lets the loader wire every shape's runtime vertex
/// base (`obj+0x28`) to the same pool under one frame; the old per-mesh independent frames + distinct pools
/// made shapes 1..N resolve their (local) indices against shape[0]'s pool → inverted `getAabb` → fall-through.
pub struct SharedMeshSet {
    /// Common dequant frame (`dequant(pool[g]) = min + pool[g]*scale`), shared by every sub-mesh subpart.
    pub min: [f32; 3],
    pub scale: [f32; 3],
    /// Whole-container float AABB — what every byte-identical `EE` record stores (`EE+24 min / +36 max`).
    pub whole_min: [f32; 3],
    pub whole_max: [f32; 3],
    /// The one shared quantized pool, in global-index order.
    pub pool: Vec<[u16; 3]>,
    pub subs: Vec<SubMeshBuild>,
}

/// Compact a mesh to only the vertices its triangles actually use, in ascending original-index order, and
/// re-base its triangle indices to `0..used`. For fresh 0-based meshes this is (near) identity; for a mesh
/// decoded out of a retail SHARED-POOL container — whose index array is GLOBAL and whose vertex list carries
/// the whole shared prefix — it drops the redundant prefix so re-concatenation reproduces retail's exact
/// per-sub-mesh partition (no 62 KB bloat). Order-preserving, so the MOPP bakes over the same triangle order.
fn localize(tris: &[[u32; 3]], verts: &[[f32; 3]]) -> (Vec<[u32; 3]>, Vec<[f32; 3]>) {
    let mut used: Vec<u32> = tris.iter().flatten().copied().collect();
    used.sort_unstable();
    used.dedup();
    let mut map = std::collections::HashMap::with_capacity(used.len());
    let mut lverts = Vec::with_capacity(used.len());
    for (li, &g) in used.iter().enumerate() {
        map.insert(g, li as u32);
        lverts.push(verts[g as usize]);
    }
    let ltris: Vec<[u32; 3]> =
        tris.iter().map(|t| [map[&t[0]], map[&t[1]], map[&t[2]]]).collect();
    (ltris, lverts)
}

/// Build the retail SHARED-POOL layout from N independent (or decoded-global) meshes: localize each, then
/// concatenate the localized vertex lists into ONE global pool, quantize the whole pool under a COMMON frame
/// (the container AABB), and re-index every sub-mesh's triangles into that global pool. Fails if the mesh set
/// is empty, any sub-mesh fails basic validation, or the combined pool exceeds 65 536 vertices (a `u16` index
/// cannot address more).
pub fn build_shared_mesh_set(meshes: &[MeshSoup]) -> Result<SharedMeshSet, String> {
    if meshes.is_empty() {
        return Err("build_shared_mesh_set: no meshes".into());
    }
    // 1. Localize each mesh; validate; accumulate the global vertex list + per-sub base ranges.
    let mut locals: Vec<(Vec<[u32; 3]>, Vec<[f32; 3]>)> = Vec::with_capacity(meshes.len());
    let mut gverts: Vec<[f32; 3]> = Vec::new();
    let mut bases: Vec<u32> = Vec::with_capacity(meshes.len());
    for (i, (tris, verts)) in meshes.iter().enumerate() {
        if verts.is_empty() {
            return Err(format!("mesh {i} has no vertices"));
        }
        if tris.is_empty() {
            return Err(format!("mesh {i} has no triangles"));
        }
        for (ti, t) in tris.iter().enumerate() {
            for &v in t {
                if v as usize >= verts.len() {
                    return Err(format!("mesh {i} triangle {ti} index {v} out of range (nverts={})", verts.len()));
                }
            }
        }
        let (lt, lv) = localize(tris, verts);
        bases.push(gverts.len() as u32);
        for v in &lv {
            if !v.iter().all(|c| c.is_finite()) {
                return Err(format!("mesh {i} has a non-finite vertex {v:?}"));
            }
            gverts.push(*v);
        }
        locals.push((lt, lv));
    }
    if gverts.len() > 65_536 {
        return Err(format!(
            "shared vertex pool has {} vertices; WpMeshShape16 u16 indices address at most 65536",
            gverts.len()
        ));
    }

    // 2. Common quantization frame over the WHOLE pool (the container AABB).
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &gverts {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let mut scale = [0.0f32; 3];
    for k in 0..3 {
        let ext = hi[k] - lo[k];
        scale[k] = if ext > 0.0 { ext / 65535.0 } else { 1.0 };
    }
    let quant = |v: [f32; 3]| -> [u16; 3] {
        let mut q = [0u16; 3];
        for k in 0..3 {
            let u = ((v[k] - lo[k]) / scale[k]).round();
            q[k] = u.clamp(0.0, 65535.0) as u16;
        }
        q
    };
    let pool: Vec<[u16; 3]> = gverts.iter().map(|&v| quant(v)).collect();

    // 3. Per-sub-mesh: global index array + owned range + retained local geometry for the MOPP.
    let mut subs = Vec::with_capacity(meshes.len());
    for (i, (lt, lv)) in locals.into_iter().enumerate() {
        let vbase = bases[i];
        let tris_global: Vec<[u16; 3]> =
            lt.iter().map(|t| [(t[0] + vbase) as u16, (t[1] + vbase) as u16, (t[2] + vbase) as u16]).collect();
        subs.push(SubMeshBuild {
            tris_global,
            vbase,
            vcount: lv.len() as u32,
            local_tris: lt,
            local_verts: lv,
        });
    }

    Ok(SharedMeshSet { min: lo, scale, whole_min: lo, whole_max: hi, pool, subs })
}

/// **Author the FAITHFUL MERGED 2N−1 BV-tree PHY2 trailing wrapper** — the fix for the N-linked-chains
/// wrapper that crashed the retail loader at `0x0248C0E9` (garbage relocated pointer). Reproduces the
/// retail floor topology byte-measured on `0x39AF17DC`: `(k+N)` `AA…BB` records (k = N−1 internal + N
/// leaf) → k `CC…DD` union-AABB scratch blocks → N `EE` subpart blocks → ONE shared quantized pool (all N
/// EEs point to it, `nverts` = the whole-container count) → N `{12}{ntris_i}` tails.
///
/// The tree is a right-leaning caterpillar that uses ONLY edge types PROVEN to load (single-mesh in-game +
/// retail measurement): every `internal+56→leaf`, `internal+60→internal|FF`, `leaf+52→internal`,
/// `leaf+60→sibling-leaf|FF`. It deliberately avoids the two UNPROVEN edges (`internal+56→internal`,
/// `internal+60→leaf`) a generic balanced tree would use. Layout, per record index:
/// * internal `j` @ `j*68` (root = record 0): `+40→CC(j)`, `+56→leaf(j)`, `+60→ internal(j+1)` (or FF on
///   the last internal). `+44/+48/+52 = FF`.
/// * leaf `l` @ `(k+l)*68`: `+44→EE(l)`, `+52→ its parent internal`, `+40/+48/+56 = FF`.
///   Leaves `0..k` are the `+56` children of internals `0..k`; the one extra leaf `n-1` (present only for
///   n≥2) hangs off leaf 0 as its `+60` sibling (its parent is internal 0), exactly as retail hangs its 4th
///   leaf off a `+56`-child leaf. `leaf(0)+60 → leaf(n-1)`; every other leaf `+60 = FF`.
///
/// For N=1 this is BYTE-IDENTICAL to [`build_mesh_wrapper`] (internal 0 `+56→leaf 0`, `+60=FF`; leaf 0
/// `+52→internal 0`). For N=4 the record/CC/EE/pool offsets match retail exactly (CC@476/584/692,
/// EE@800/908/1016/1124, pool@1232). CC blocks are the constant scratch template (the engine recomputes
/// real AABBs at load), as in retail. Gated by [`validate_multi_wrapper_chain`] (a full recursive tree
/// walk from the root).
///
/// **SHARED POOL (retail-faithful).** All N leaf `EE` records are byte-identical bar their tail pointers:
/// each stores `nverts = pool.len()` (the whole-container count), the whole-container float AABB
/// (`whole_min`/`whole_max`), and the SAME `EE+76 → shared pool` pointer — exactly as the retail floor's 4
/// EEs all point to the one pool at `wrapper+1232`. The single shared quantized pool is laid ONCE after the
/// EE blocks, followed by N `{12}{ntris_i}` tails. This is the crux of the fall-through fix: the loader wires
/// every shape's runtime vertex base (`obj+0x28`) to this one pool under the one common frame, so no shape
/// resolves another shape's (mismatched) pool.
fn build_multi_mesh_wrapper(
    pool: &[[u16; 3]],
    whole_min: [f32; 3],
    whole_max: [f32; 3],
    per_mesh_ntris: &[u32],
    pkend: usize,
    k: u32,
) -> Vec<u8> {
    let n = per_mesh_ntris.len();
    let ic = internal_count(n);
    let n_records = ic + n;
    let cc_region = n_records * 68;
    let ee_region = cc_region + ic * 108;
    let pool_region = ee_region + n * 108;

    // ONE shared pool @pool_region, 4-aligned, then N per-mesh {12}{ntris_i} tails.
    let nverts = pool.len() as u32;
    let mut pool_bytes: Vec<u8> = Vec::with_capacity(pool.len() * 6);
    for v in pool {
        pool_bytes.extend_from_slice(&v[0].to_le_bytes());
        pool_bytes.extend_from_slice(&v[1].to_le_bytes());
        pool_bytes.extend_from_slice(&v[2].to_le_bytes());
    }
    let mut tail_start = pool_region + pool_bytes.len();
    while !tail_start.is_multiple_of(4) {
        tail_start += 1;
    }
    let tail_rel = |i: usize| tail_start + i * 8;
    let total = tail_start + n * 8;

    let p = |rel: usize| -> u32 { (pkend + rel) as u32 };
    const FF: u32 = 0xFFFF_FFFF;
    let internal_rel = |j: usize| j * 68;
    let leaf_rec = |l: usize| ic + l; // leaf l's record index
    let leaf_rel = |l: usize| leaf_rec(l) * 68;

    let mut w: Vec<u8> = Vec::with_capacity(total);

    // ── AA records: internals 0..ic, then leaves 0..n (record order == memory order) ──
    for j in 0..ic {
        // internal j: +40→CC(j), +44/+48/+52 FF, +56→leaf(j), +60→internal(j+1) or FF (last).
        let cc = p(cc_region + j * 108);
        let child_a = p(leaf_rel(j)); // +56 → this internal's own leaf (proven internal+56→leaf)
        let child_b = if j + 1 < ic { p(internal_rel(j + 1)) } else { FF }; // +60 → next internal (proven)
        write_aa_record(&mut w, internal_rel(j), k, [cc, FF, FF, FF, child_a, child_b]);
    }
    for l in 0..n {
        // leaf l: +44→EE(l); +52→parent internal; +60→sibling-leaf (only leaf 0 for n≥2) else FF.
        let ee = p(ee_region + l * 108);
        // parent: leaves 0..ic are the +56 child of internal l; the extra leaf (n-1) hangs off internal 0.
        let parent = if l < ic { internal_rel(l) } else { internal_rel(0) };
        let sibling = if l == 0 && n >= 2 { p(leaf_rel(n - 1)) } else { FF };
        write_aa_record(&mut w, leaf_rel(l), k, [FF, ee, FF, p(parent), FF, sibling]);
    }
    debug_assert_eq!(w.len(), cc_region);

    // ── CC blocks: one per internal ──
    for j in 0..ic {
        write_cc_block(&mut w, cc_region + j * 108);
    }
    debug_assert_eq!(w.len(), ee_region);

    // ── EE blocks: one per leaf/mesh, ALL pointing to the ONE shared pool (byte-identical bar the tail
    //    pointer), nverts = whole-container count, whole-container AABB — the retail shared-pool form. ──
    for l in 0..n {
        write_ee_block(
            &mut w,
            ee_region + l * 108,
            nverts,
            whole_min,
            whole_max,
            p(pool_region),
            p(tail_rel(l)),
        );
    }
    debug_assert_eq!(w.len(), pool_region);

    // ── ONE shared pool, then per-mesh {12}{ntris_i} tails ──
    w.extend_from_slice(&pool_bytes);
    while !w.len().is_multiple_of(4) {
        w.push(0);
    }
    debug_assert_eq!(w.len(), tail_start);
    for &ntris in per_mesh_ntris {
        w.extend_from_slice(&12u32.to_le_bytes());
        w.extend_from_slice(&ntris.to_le_bytes());
    }
    debug_assert_eq!(w.len(), total);
    w
}

/// **Walk the WHOLE merged BV-tree from the root**, exactly as the retail loader relocates + traverses it,
/// and prove every relocated pointer lands in-bounds on the correct marker. This is the gate the old
/// N-chain walker LACKED — it only followed the linear per-shape chains and never validated that the tree's
/// internal-node two-child pointers resolve. Starting at record 0 (the root, `wrapper+0`) it follows every
/// `internal → {CC, +56 child, +60 child}` and `leaf → {EE, +52 parent, +60 sibling}` edge, asserting:
/// * every AA record has its `AAAAAAAA`/`BBBBBBBB` framing;
/// * each internal `+40` lands on `CCCCCCCC`; each leaf `+44` lands on `EEEEEEEE`;
/// * each leaf's `EE+76` pool reproduces THAT mesh's `pool[0]`, and `EE+4/EE+12` resolve to `{12}/{ntris_l}`;
/// * every child/parent/sibling pointer resolves to an AA-record boundary (in-bounds, `%68==0`);
/// * the tree is acyclic (no record visited twice) and EVERY record + EVERY one of the N leaves is reached.
///
/// A node must expose EXACTLY ONE of `+40` (internal) / `+44` (leaf) — the clean separation the author
/// emits. `leaf l` (record `internal_count+l`) carries `mesh l`.
///
/// **SHARED-POOL gate (retail-faithful).** `pool` is the ONE shared quantized pool; `per_mesh_ntris[l]` is
/// leaf `l`'s triangle count. Every leaf's `EE+76` must point to the SAME shared pool (the retail form), the
/// pool's first vertex must reproduce `pool[0]`, every `EE+68` must equal `pool.len()`, and each `EE+4`/`EE+12`
/// must resolve to `{12}`/`{ntris_l}`. (The old gate asserted N *distinct* pools; that per-leaf layout is the
/// exact divergence that collapsed the runtime pool wiring onto shape[0].)
pub fn validate_multi_wrapper_chain(
    body: &[u8],
    pkend: usize,
    pool: &[[u16; 3]],
    per_mesh_ntris: &[u32],
) -> Result<(), String> {
    let n = per_mesh_ntris.len();
    if n == 0 {
        return Err("validate_multi_wrapper_chain: no meshes".into());
    }
    let ic = internal_count(n);
    let n_records = ic + n;
    let aa_region = n_records * 68;

    let rd = |abs: usize| -> Result<u32, String> {
        if abs + 4 > body.len() {
            return Err(format!("ptr target {abs} out of body bounds ({})", body.len()));
        }
        Ok(u32::from_le_bytes(body[abs..abs + 4].try_into().unwrap()))
    };
    let expect = |abs: usize, want: u32, what: &str| -> Result<(), String> {
        let got = rd(abs)?;
        if got != want {
            return Err(format!("{what}: expected {want:#010x} at body[{abs}], got {got:#010x}"));
        }
        Ok(())
    };
    // A pointer field at `abs`: FF → None; else the body-absolute target (bounds-checked).
    let follow_opt = |abs: usize| -> Result<Option<usize>, String> {
        let v = rd(abs)?;
        if v == 0xFFFF_FFFF {
            return Ok(None);
        }
        let o = v as usize;
        if o >= body.len() {
            return Err(format!("relocated ptr @body[{abs}] = body[{o}] out of bounds ({})", body.len()));
        }
        Ok(Some(o))
    };
    // Body-absolute AA-record target → record index (must be an in-bounds record boundary).
    let rec_index = |o: usize| -> Result<usize, String> {
        if o < pkend {
            return Err(format!("AA ptr body[{o}] is before the wrapper (pkend {pkend})"));
        }
        let rel = o - pkend;
        if rel >= aa_region || !rel.is_multiple_of(68) {
            return Err(format!("AA ptr body[{o}] (wrapper+{rel}) is not a record boundary"));
        }
        Ok(rel / 68)
    };

    // Root is record 0 at wrapper+0.
    expect(pkend, 0xAAAA_AAAA, "root AA record")?;
    let mut visited = vec![false; n_records];
    let mut leaves_seen = vec![false; n];
    // Each leaf's EE+76 pool pointer, indexed by leaf/mesh — asserted DISTINCT below so no two shapes
    // share a pool (the wrapper-level form of the "all leaves → shape[0]" regression).
    let mut leaf_pool_ptr = vec![0usize; n];
    let mut stack = vec![0usize];
    while let Some(rec) = stack.pop() {
        if visited[rec] {
            return Err(format!("record {rec} reached twice — the tree has a cycle / shared child"));
        }
        visited[rec] = true;
        let base = pkend + rec * 68;
        expect(base, 0xAAAA_AAAA, &format!("record {rec} AA marker"))?;
        expect(base + 64, 0xBBBB_BBBB, &format!("record {rec} BBBB terminator"))?;
        let f40 = follow_opt(base + 40)?;
        let f44 = follow_opt(base + 44)?;
        match (f40, f44) {
            (Some(cc), None) => {
                // INTERNAL node: +40→CC, +56→child A (required), +60→child B (optional).
                expect(cc, 0xCCCC_CCCC, &format!("internal {rec} +40→CC"))?;
                let a = follow_opt(base + 56)?
                    .ok_or_else(|| format!("internal {rec} +56 child A missing"))?;
                stack.push(rec_index(a)?);
                if let Some(b) = follow_opt(base + 60)? {
                    stack.push(rec_index(b)?);
                }
            }
            (None, Some(ee)) => {
                // LEAF node: +44→EE, +52→parent, +60→sibling (optional). Carries mesh (rec-ic).
                expect(ee, 0xEEEE_EEEE, &format!("leaf {rec} +44→EE"))?;
                let l = rec
                    .checked_sub(ic)
                    .filter(|&l| l < n)
                    .ok_or_else(|| format!("leaf record {rec} outside leaf range [{ic}..{n_records})"))?;
                leaves_seen[l] = true;
                // parent back-link must resolve to an AA record.
                let par = follow_opt(base + 52)?
                    .ok_or_else(|| format!("leaf {rec} +52 parent link missing"))?;
                rec_index(par)?;
                // EE+76 pool must point to the ONE shared pool and reproduce pool[0]; EE+68 = pool.len();
                // EE+4/EE+12 → {12}/{ntris_l}.
                let poolptr = rd(ee + 76)? as usize;
                leaf_pool_ptr[l] = poolptr;
                let nverts_field = rd(ee + 68)?;
                if nverts_field != pool.len() as u32 {
                    return Err(format!(
                        "leaf {rec} EE+68 nverts {nverts_field} != shared pool len {}",
                        pool.len()
                    ));
                }
                let mut want0 = Vec::with_capacity(6);
                if let Some(v0) = pool.first() {
                    want0.extend_from_slice(&v0[0].to_le_bytes());
                    want0.extend_from_slice(&v0[1].to_le_bytes());
                    want0.extend_from_slice(&v0[2].to_le_bytes());
                }
                if !want0.is_empty() {
                    if poolptr + want0.len() > body.len() {
                        return Err(format!("leaf {rec} EE+76→pool @body[{poolptr}] runs past body"));
                    }
                    if body[poolptr..poolptr + want0.len()] != want0[..] {
                        return Err(format!(
                            "leaf {rec} EE+76→shared pool @body[{poolptr}]: first vertex {:02x?} != pool[0] {:02x?}",
                            &body[poolptr..poolptr + want0.len()],
                            want0
                        ));
                    }
                }
                let twelve = rd(ee + 4)? as usize;
                if rd(twelve)? != 12 {
                    return Err(format!("leaf {rec} EE+4→tail: expected 12, got {}", rd(twelve)?));
                }
                let ntris_ptr = rd(ee + 12)? as usize;
                if rd(ntris_ptr)? != per_mesh_ntris[l] {
                    return Err(format!(
                        "leaf {rec} EE+12→tail: expected ntris {}, got {}",
                        per_mesh_ntris[l],
                        rd(ntris_ptr)?
                    ));
                }
                if let Some(sib) = follow_opt(base + 60)? {
                    stack.push(rec_index(sib)?);
                }
            }
            (Some(_), Some(_)) => {
                return Err(format!("record {rec} exposes BOTH +40(CC) and +44(EE) — not the clean author model"));
            }
            (None, None) => {
                return Err(format!("record {rec} exposes neither +40(CC) nor +44(EE)"));
            }
        }
    }
    for (i, v) in visited.iter().enumerate() {
        if !*v {
            return Err(format!("record {i} is unreachable from the root — an orphaned node"));
        }
    }
    for (l, v) in leaves_seen.iter().enumerate() {
        if !*v {
            return Err(format!("mesh/leaf {l} was never reached by the tree walk"));
        }
    }
    // SHARED-POOL gate: EVERY leaf's EE+76 must point to the SAME shared pool — the retail form. (Retail's 4
    // EEs all point to wrapper+1232; per-leaf distinct pools are the exact divergence that collapsed the
    // runtime obj+0x28 wiring onto shape[0] and dropped the player through the floor.)
    for l in 1..n {
        if leaf_pool_ptr[l] != leaf_pool_ptr[0] {
            return Err(format!(
                "leaf {l} EE+76 pool body[{}] != leaf 0's shared pool body[{}] — not the retail shared-pool form",
                leaf_pool_ptr[l], leaf_pool_ptr[0]
            ));
        }
    }
    Ok(())
}

/// **DISTINCT-SHAPE BINDING gate** — parse the authored `PHY2` packfile's *global-fixup* object graph and
/// prove the `WpArray` binds **N distinct shapes**, each fully wired to its OWN `hkpMoppCode` + `WpMeshShape16`
/// (`shape[i] → mesh[i]`, every shape covered exactly once), never collapsed onto `shape[0]`.
///
/// This closes the exact hole the live A/B x32dbg proof exposed and that NO other offline check saw: the
/// reader ([`crate::havok::parse_phy2_body`]) enumerates meshes by *virtual* fixup and never consults the
/// global-fixup table, so a `WpArray` whose N element pointers all relocate to `bvtree[0]` (→ N runtime
/// records all bound to shape[0], shapes 1..N orphaned → the player falls through) round-trips as "N distinct
/// meshes" and passes every mesh/MOPP census. Here we walk the actual `{src, sec, dst}` global fixups:
/// * `WpArray.data` (`+8`, a LOCAL fixup) → the element array; each element `elem[i]` (`+i*4`) must carry a
///   global fixup to a **`hkpMoppBvTreeShape`**, and the N targets must be **pairwise distinct**;
/// * each `bvtree[i]+16` (`m_code`) → a **distinct `hkpMoppCode`**, and each `bvtree[i]+52` (child) → a
///   **distinct `WpMeshShape16`** — so no two shapes share a MOPP or a mesh.
///
/// `body` = the whole PHY2 body (prefix + packfile + wrapper). `n` = the expected shape count. Returns `Ok`
/// only when all three N-distinct invariants hold.
pub fn validate_multi_shape_binding(body: &[u8], n: usize) -> Result<(), String> {
    let off = body
        .windows(8)
        .position(|w| w == crate::havok::HAVOK_MAGIC)
        .ok_or("validate_multi_shape_binding: no Havok packfile magic in body")?;
    let raw = crate::havok::parse_packfile_raw(&body[off..])
        .map_err(|e| format!("validate_multi_shape_binding: packfile parse: {e}"))?;

    // Object src (data-relative) → class name, for identifying a global-fixup target.
    let class_at = |src: usize| -> Option<&str> {
        raw.vfixups.iter().find(|(s, _)| *s == src).map(|(_, c)| c.as_str())
    };
    // Resolve a global fixup at data-relative `src` to (class, data-relative target src). Only sec==2
    // (__data__) intra-file references are meaningful here.
    let follow_global = |src: usize, what: &str| -> Result<usize, String> {
        match raw.gf.get(&src) {
            Some(&(2, dst)) => Ok(dst),
            Some(&(sec, _)) => Err(format!("{what}: global fixup points to section {sec}, not __data__(2)")),
            None => Err(format!("{what}: no global fixup (null pointer — shape binding is broken)")),
        }
    };

    // WpArray root + its element array (element ptr is a LOCAL fixup on WpArray+8).
    let wparray_src = raw
        .vfixups
        .iter()
        .find(|(_, c)| c == "WpArray")
        .map(|(s, _)| *s)
        .ok_or("validate_multi_shape_binding: no WpArray root")?;
    let size = u32::from_le_bytes(
        body[off + raw.data_pk + wparray_src + 12..off + raw.data_pk + wparray_src + 16]
            .try_into()
            .unwrap(),
    ) as usize;
    if size != n {
        return Err(format!("WpArray size {size} != expected shape count {n}"));
    }
    let elem_data = *raw
        .lf
        .get(&(wparray_src + 8))
        .ok_or("validate_multi_shape_binding: WpArray+8 has no element-array local fixup")?;

    let mut bvtrees: Vec<usize> = Vec::with_capacity(n);
    for i in 0..n {
        let bv = follow_global(elem_data + i * 4, &format!("WpArray elem[{i}]"))?;
        match class_at(bv) {
            Some("hkpMoppBvTreeShape") => {}
            other => {
                return Err(format!(
                    "WpArray elem[{i}] → object of class {other:?}, expected hkpMoppBvTreeShape"
                ))
            }
        }
        bvtrees.push(bv);
    }
    // Pairwise-distinct bvtrees (the core "not all → shape[0]" assertion).
    for a in 0..n {
        for b in (a + 1)..n {
            if bvtrees[a] == bvtrees[b] {
                return Err(format!(
                    "WpArray elem[{a}] and elem[{b}] BOTH bind bvtree@data+{} — shapes collapsed onto one (all → shape[0] class bug)",
                    bvtrees[a]
                ));
            }
        }
    }
    // Each bvtree → a distinct m_code and a distinct child mesh.
    let mut codes: Vec<usize> = Vec::with_capacity(n);
    let mut meshes_seen: Vec<usize> = Vec::with_capacity(n);
    for (i, &bv) in bvtrees.iter().enumerate() {
        let code = follow_global(bv + 16, &format!("bvtree[{i}]+16 (m_code)"))?;
        if class_at(code) != Some("hkpMoppCode") {
            return Err(format!("bvtree[{i}] m_code → {:?}, expected hkpMoppCode", class_at(code)));
        }
        let mesh = follow_global(bv + 52, &format!("bvtree[{i}]+52 (child)"))?;
        if class_at(mesh) != Some("WpMeshShape16") {
            return Err(format!("bvtree[{i}] child → {:?}, expected WpMeshShape16", class_at(mesh)));
        }
        codes.push(code);
        meshes_seen.push(mesh);
    }
    for a in 0..n {
        for b in (a + 1)..n {
            if codes[a] == codes[b] {
                return Err(format!("shapes {a} and {b} share hkpMoppCode@data+{}", codes[a]));
            }
            if meshes_seen[a] == meshes_seen[b] {
                return Err(format!("shapes {a} and {b} share WpMeshShape16@data+{}", meshes_seen[a]));
            }
        }
    }
    Ok(())
}

/// The 19-class hierarchy a retail mesh `PHY2` packfile declares, in order, with the real Havok class
/// signatures (extracted from `vz.wad` blocks 826/2612). Only `WpArray`, `hkpMoppBvTreeShape`,
/// `hkpMoppCode` and `WpMeshShape16` are referenced by fixups; the rest are type-registration entries
/// reproduced verbatim for faithfulness.
const MESH_CLASSES: &[(u32, &str)] = &[
    (0x38771f8e, "hkClass"),
    (0xa5240f57, "hkClassMember"),
    (0x8a3609cf, "hkClassEnum"),
    (0xce6f8a6c, "hkClassEnumItem"),
    (0xb1a39537, "WpMeshShape16"),
    (0xdbba3c29, "hkpMoppBvTreeShape"),
    (0xe7eca7eb, "hkpBvTreeShape"),
    (0x7f01287c, "WpArray"),
    (0x7f54a876, "WpMeshShape16MeshSubpart"),
    (0x89141815, "WpMeshShape16Triangle"),
    (0xe0708a00, "hkpShapeContainer"),
    (0x9b1a3265, "hkpShapeCollection"),
    (0xe0708a00, "hkBaseObject"),
    (0x666490a1, "hkpShape"),
    (0x73aa1d38, "hkpSingleShapeContainer"),
    (0xd8fdbb08, "hkpMoppCodeCodeInfo"),
    (0x3b1c1113, "hkReferencedObject"),
    (0x72ee59f8, "hkpMoppCode"),
    (0x4117d60e, "hkMoppBvTreeShapeBase"),
];

/// A quantized `WpMeshShape16`: the dequant frame, the `u16×3` vertex pool, and the validated `u16`
/// index triples. `dequant(pool[i]) = min + pool[i]*scale`; the pool is what lands in the trailing
/// engine wrapper.
#[derive(Debug, Clone, PartialEq)]
pub struct WpMesh16 {
    /// AABB-min dequant offset, per axis.
    pub min: [f32; 3],
    /// Per-axis dequant step (`extent / 0xFFFF`; `1.0` on a degenerate axis).
    pub scale: [f32; 3],
    /// Quantized vertex pool, one `u16×3` per input vertex, in input order.
    pub pool: Vec<[u16; 3]>,
    /// Triangle index triples (each index `< pool.len()`), in input order — the SAME order the MOPP is
    /// baked over.
    pub tris: Vec<[u16; 3]>,
}

impl WpMesh16 {
    /// Dequantize vertex `i` back to model-local space (`min + pool[i]*scale`).
    pub fn dequant(&self, i: usize) -> [f32; 3] {
        let q = self.pool[i];
        [
            self.min[0] + q[0] as f32 * self.scale[0],
            self.min[1] + q[1] as f32 * self.scale[1],
            self.min[2] + q[2] as f32 * self.scale[2],
        ]
    }

    /// The raw `u16×3` pool as little-endian bytes (6 bytes/vertex) — the trailing-wrapper vertex pool.
    pub fn pool_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.pool.len() * 6);
        for v in &self.pool {
            b.extend_from_slice(&v[0].to_le_bytes());
            b.extend_from_slice(&v[1].to_le_bytes());
            b.extend_from_slice(&v[2].to_le_bytes());
        }
        b
    }

    /// The packfile **secondary array** (subpart `+40` ptr / `+44` count = object `+88`/`+92`): the u16
    /// vertex-index list `WpMeshShape16::getAabb` (`FUN_00725ba0`) iterates to compute the shape's bounding
    /// box (`aabb = ∪ dequant(pool[secondary[i]])`). Retail stores a small extremal subset; we list **every**
    /// vertex index `0..nverts` (a correct conservative superset → getAabb yields the exact full AABB). An
    /// EMPTY secondary array makes getAabb return the inverted init AABB (`+FLT_MAX .. -FLT_MAX`), so the
    /// broadphase never reports the shape and the player falls through — the authored-mesh collide bug.
    pub fn secondary_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.pool.len() * 2);
        for i in 0..self.pool.len() as u32 {
            b.extend_from_slice(&(i as u16).to_le_bytes());
        }
        b
    }

    /// The packfile index array: `acnt × 8` bytes `{u16 a, b, c, 0}` (the trailing `u16` is the retail
    /// material/pad slot). This is what the subpart's `+32` pointer targets.
    pub fn index_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.tris.len() * 8);
        for t in &self.tris {
            b.extend_from_slice(&t[0].to_le_bytes());
            b.extend_from_slice(&t[1].to_le_bytes());
            b.extend_from_slice(&t[2].to_le_bytes());
            b.extend_from_slice(&0u16.to_le_bytes());
        }
        b
    }
}

/// Quantize a model-local mesh into a [`WpMesh16`].
///
/// `min` = AABB min, `scale[k]` = `extent[k] / 0xFFFF` (or `1.0` where an axis is flat), so every
/// vertex maps into `[0, 0xFFFF]` with round-to-nearest and the dequant error is ≤ half a step. Fails
/// if the mesh has no vertices, more than 65 536 vertices (a `u16` index cannot address them), or any
/// triangle index is out of range.
pub fn write_wpmesh16(tris: &[[u32; 3]], verts: &[[f32; 3]]) -> Result<WpMesh16, String> {
    if verts.is_empty() {
        return Err("mesh has no vertices".into());
    }
    if verts.len() > 65_536 {
        return Err(format!(
            "mesh has {} vertices; WpMeshShape16 u16 indices address at most 65536",
            verts.len()
        ));
    }
    if tris.is_empty() {
        return Err("mesh has no triangles".into());
    }
    for (i, t) in tris.iter().enumerate() {
        for &v in t {
            if v as usize >= verts.len() {
                return Err(format!("triangle {i} index {v} out of range (nverts={})", verts.len()));
            }
        }
    }

    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in verts {
        if !v.iter().all(|c| c.is_finite()) {
            return Err(format!("non-finite vertex {v:?}"));
        }
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let mut scale = [0.0f32; 3];
    for k in 0..3 {
        let ext = hi[k] - lo[k];
        scale[k] = if ext > 0.0 { ext / 65535.0 } else { 1.0 };
    }
    let quant = |v: [f32; 3]| -> [u16; 3] {
        let mut q = [0u16; 3];
        for k in 0..3 {
            let u = ((v[k] - lo[k]) / scale[k]).round();
            q[k] = u.clamp(0.0, 65535.0) as u16;
        }
        q
    };
    let pool: Vec<[u16; 3]> = verts.iter().map(|&v| quant(v)).collect();
    let tris16: Vec<[u16; 3]> = tris.iter().map(|t| [t[0] as u16, t[1] as u16, t[2] as u16]).collect();

    Ok(WpMesh16 { min: lo, scale, pool, tris: tris16 })
}

#[inline]
fn put_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn put_f32(b: &mut [u8], o: usize, v: f32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// Assemble a whole `PHY2` body from a model-local triangle mesh.
///
/// Layout: `[48-byte PHY2 prefix][Havok-5.5 LE packfile][trailing wrapper]`. `asset_name` sets the
/// prefix name-hash via [`crate::hash::pandemic_hash_m2`] (the asset identity). The MOPP is baked with
/// [`mopp::encode`] over `tris` in the SAME order as the emitted index array, so its leaf shape-keys are
/// exactly `[0..tris.len())`.
///
/// Returns the full body bytes. Round-trips through [`crate::havok::parse_phy2_body`] (all three shape
/// classes present, the mesh + MOPP re-decode) — see the tests.
pub fn build_phy2(asset_name: &str, tris: &[[u32; 3]], verts: &[[f32; 3]]) -> Result<Vec<u8>, String> {
    build_phy2_kind(asset_name, tris, verts, MoppKind::Spatial)
}

/// [`build_phy2`] with an explicit [`MoppKind`]. [`MoppKind::Spatial`] is the shipped path;
/// [`MoppKind::ReturnAll`] bakes the frame-ignoring return-all MOPP while keeping `m_info` + mesh +
/// wrapper byte-identical (the collision diagnostic — see [`MoppKind`]).
pub fn build_phy2_kind(
    asset_name: &str,
    tris: &[[u32; 3]],
    verts: &[[f32; 3]],
    kind: MoppKind,
) -> Result<Vec<u8>, String> {
    let mesh = write_wpmesh16(tris, verts)?;
    let (mopp_info, mopp_code, mopp_count) = bake_shape_mopp(tris, verts, kind);

    // ── __data__ object region (body-relative offsets) ──
    let mut body: Vec<u8> = Vec::new();
    let mut local: Vec<(u32, u32)> = Vec::new();
    let mut global: Vec<(u32, u32, u32)> = Vec::new();
    let mut virt: Vec<(u32, u32, u32)> = Vec::new();

    // classnames-body-relative name offsets (for the virtual fixups + header root).
    let (_cn_body, cn_offs) = write_classnames(MESH_CLASSES);
    let cnoff = |name: &str| -> u32 {
        let i = MESH_CLASSES.iter().position(|(_, n)| *n == name).unwrap();
        cn_offs[i]
    };

    // append 16-aligned helper
    let pad16 = |b: &mut Vec<u8>| while !b.len().is_multiple_of(16) { b.push(0) };

    // WpArray root @0 (32 bytes): +8 ptr(→elem), +12 size=1, +16 cap.
    let wparray_off = body.len() as u32; // 0
    body.resize(body.len() + 32, 0);
    // (ptr@+8 filled by local fixup relocation at load; on disk it's 0)
    put_u32(&mut body, 12, 1); // size
    put_u32(&mut body, 16, 0xC000_0001); // capAndFlags = LOCKED|DONT_DEALLOCATE | 1

    // WpArray element storage @32: one u32 pointer (→ bvtree), padded to 16 → 32..48.
    let elem_off = body.len() as u32; // 32
    body.resize(body.len() + 4, 0);
    pad16(&mut body); // → 48

    // hkpMoppBvTreeShape @48 (64 zero bytes); pointers set by global fixups.
    let bvtree_off = body.len() as u32; // 48
    body.resize(body.len() + 64, 0);

    // hkpMoppCode @112 (48 bytes).
    let moppcode_off = body.len() as u32; // 112
    body.resize(body.len() + 48, 0);
    put_f32(&mut body, moppcode_off as usize + 16, mopp_info.offset[0]);
    put_f32(&mut body, moppcode_off as usize + 20, mopp_info.offset[1]);
    put_f32(&mut body, moppcode_off as usize + 24, mopp_info.offset[2]);
    let lane3 = if mopp_info.scale.abs() > 0.0 { 1.0 / mopp_info.scale } else { 0.0 };
    put_f32(&mut body, moppcode_off as usize + 28, lane3);
    // +32 m_data ptr (local fixup), +36 count, +40 cap, +44 buildType
    put_u32(&mut body, moppcode_off as usize + 36, mopp_count);
    put_u32(&mut body, moppcode_off as usize + 40, 0xC000_0000 | (mopp_count & 0x3FFF_FFFF));
    put_u32(&mut body, moppcode_off as usize + 44, 1); // buildType

    // MOPP bytecode buffer @160, padded to 16.
    let moppbytes_off = body.len() as u32; // 160
    body.extend_from_slice(&mopp_code);
    pad16(&mut body);

    // WpMeshShape16 base (48) + one subpart (48).
    let mesh_off = body.len() as u32;
    body.resize(body.len() + 96, 0);
    let m = mesh_off as usize;
    put_f32(&mut body, m + 24, CONVEX_RADIUS); // convex radius 0.01f
    // +28 subpart-array ptr (local fixup → inline subpart at +48)
    put_u32(&mut body, m + 32, 1); // nsub
    put_u32(&mut body, m + 36, 0xC000_0001); // cap = |1
    // subpart @ mesh+48
    let sp = m + 48;
    put_f32(&mut body, sp, mesh.min[0]);
    put_f32(&mut body, sp + 4, mesh.min[1]);
    put_f32(&mut body, sp + 8, mesh.min[2]);
    put_f32(&mut body, sp + 16, mesh.scale[0]);
    put_f32(&mut body, sp + 20, mesh.scale[1]);
    put_f32(&mut body, sp + 24, mesh.scale[2]);
    put_f32(&mut body, sp + 28, 1.0);
    // +32 index-array ptr (local fixup), +36 acnt, +40 secondary ptr (local fixup), +44 secondary count.
    put_u32(&mut body, sp + 36, mesh.tris.len() as u32);
    put_u32(&mut body, sp + 44, mesh.pool.len() as u32); // secondary count = nverts (getAabb loop bound)

    // Triangle index array, padded to 16.
    let index_off = body.len() as u32;
    body.extend_from_slice(&mesh.index_bytes());
    pad16(&mut body);

    // Secondary vertex-index array (subpart+40): what WpMeshShape16::getAabb iterates. Without it the
    // broadphase AABB is empty and the player falls through. See WpMesh16::secondary_bytes.
    let secondary_off = body.len() as u32;
    body.extend_from_slice(&mesh.secondary_bytes());
    pad16(&mut body);

    // ── fixups (body-relative, sorted ascending by src) ──
    // local: pointer fields relocated within __data__
    local.push((wparray_off + 8, elem_off)); // WpArray.data → elem
    local.push((moppcode_off + 32, moppbytes_off)); // m_data → bytecode
    local.push((mesh_off + 28, mesh_off + 48)); // subpart array → inline subpart
    local.push((mesh_off + 48 + 32, index_off)); // subpart.indices → index array
    local.push((mesh_off + 48 + 40, secondary_off)); // subpart.secondary → vertex-index array
    local.sort_by_key(|(s, _)| *s);
    // global: object pointers (sec 2 = __data__)
    global.push((elem_off, 2, bvtree_off)); // WpArray elem → bvtree
    global.push((bvtree_off + 16, 2, moppcode_off)); // bvtree.m_code → moppcode
    global.push((bvtree_off + 52, 2, mesh_off)); // bvtree child → mesh
    global.sort_by_key(|(s, _, _)| *s);
    // virtual: object → class (sec 0 = __classnames__)
    virt.push((wparray_off, 0, cnoff("WpArray")));
    virt.push((bvtree_off, 0, cnoff("hkpMoppBvTreeShape")));
    virt.push((moppcode_off, 0, cnoff("hkpMoppCode")));
    virt.push((mesh_off, 0, cnoff("WpMeshShape16")));
    virt.sort_by_key(|(s, _, _)| *s);

    let data = DataSection { body, local, global, virt };
    let packfile = write_packfile(MESH_CLASSES, cnoff("WpArray"), &data);

    // ── trailing wrapper: the FAITHFUL AA/BB/CC/DD/EE descriptor chain with chunk-body-absolute pool
    //    pointers (reversed from retail 0xE8EB75D7/0x86D7CF92; the engine relocates each stored offset
    //    as &body[0]+offset). `pkend` = wrapper start = 48 + packfile.len(). K=1 (refcount-like field,
    //    retail 1–2; non-pointer, not part of the walk — see build_mesh_wrapper). ──
    let pkend = 48 + packfile.len();
    // True float AABB of the mesh (the EE block stores it verbatim).
    let (mut vmin, mut vmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in verts {
        for k in 0..3 {
            vmin[k] = vmin[k].min(v[k]);
            vmax[k] = vmax[k].max(v[k]);
        }
    }
    let wrapper = build_mesh_wrapper(&mesh, vmin, vmax, pkend, 1);

    // ── PHY2 body: prefix(48) + packfile + wrapper ──
    let name_hash = crate::hash::pandemic_hash_m2(asset_name);
    let mut out = Vec::with_capacity(48 + packfile.len() + wrapper.len());
    let mut prefix = [0u8; 48];
    put_u32(&mut prefix, 0, 0x39); // constant tag observed on every retail PHY2
    put_u32(&mut prefix, 4, name_hash);
    put_u32(&mut prefix, 8, 1); // shape count (cosmetic for our reader)
    put_u32(&mut prefix, 12, 1);
    put_u32(&mut prefix, 16, 1);
    put_u32(&mut prefix, 28, mesh.pool.len() as u32); // nverts slot (cosmetic)
    put_u32(&mut prefix, 32, packfile.len() as u32); // PROVEN: byte32 = packfile size
    out.extend_from_slice(&prefix);
    out.extend_from_slice(&packfile);
    out.extend_from_slice(&wrapper);

    // Prove the wrapper is engine-walkable: every relocated body-absolute pointer resolves to its
    // descriptor / the pool (the gate the crashed pool-first layout fails).
    debug_assert!(validate_wrapper_chain(&out, pkend, &mesh).is_ok());
    Ok(out)
}

/// Assemble a whole `PHY2` body carrying **N** static collision shapes — the multi-shape generalization
/// of [`build_phy2`], for a container whose model binds several collision meshes (one `SEGM` record per
/// mesh; e.g. the PMC-HQ floor container `0x39AF17DC` = 4 meshes / 4 SEGM records).
///
/// Object graph (matches the retail floor's virtual-fixup order — a clean generalization of the proven
/// single-shape graph):
/// ```text
///   WpArray(root) @0 : hkArray<hkpShape*>{ size = N, cap = 0xC0000000|N }, N element pointers
///   for each mesh i:
///     hkpMoppBvTreeShape : +16 → hkpMoppCode[i]  · +52 → WpMeshShape16[i]
///     hkpMoppCode[i]     : m_info[i] · m_data[i] · buildType=1
///     WpMeshShape16[i]   : one subpart (COMMON min/scale · GLOBAL idx-array · secondary = its global range)
/// ```
/// **All N sub-meshes share ONE quantization frame and ONE global vertex pool** (retail's byte-measured form
/// on `0x39AF17DC`): every subpart carries the same `min`/`scale`, its index array uses GLOBAL indices into
/// the one pool, and the wrapper lays that pool ONCE with all N leaf `EE` records pointing to it. This is the
/// fall-through fix — the loader wires every shape's runtime vertex base (`obj+0x28`) to the same pool under
/// the same frame, so no shape resolves another shape's (mismatched) pool. See [`build_shared_mesh_set`].
///
/// The trailing wrapper is the FAITHFUL MERGED 2N−1 BV-tree ([`build_multi_mesh_wrapper`]): `(N−1)` internal
/// `AA` nodes (each with a `CC` union-AABB scratch + two child pointers) and `N` leaf `AA` nodes (each → its
/// `EE` subpart → the ONE shared pool + its `{12}{ntris_i}` tail), reproducing the retail floor's measured
/// record/CC/EE/pool offset topology. Gated by [`validate_multi_wrapper_chain`], a full recursive walk from
/// the root that resolves every internal two-child + leaf pointer and asserts all EEs share the one pool.
/// Every MOPP is baked over its sub-mesh's COMPACTED local geometry, so leaf keys stay `[0..tris_i.len())`.
///
/// The tree topology is our own caterpillar (using only edge types PROVEN to load: single-mesh in-game +
/// the retail-floor measurement), NOT Havok's exact median-split — but the node count, record stride, and
/// CC/EE/pool offsets match retail byte-for-byte. See the module docs / `bvtree_progress.md`.
///
/// Fails if `meshes` is empty or any mesh fails [`write_wpmesh16`]'s constraints.
pub fn build_phy2_multi(asset_name: &str, meshes: &[MeshSoup]) -> Result<Vec<u8>, String> {
    build_phy2_multi_kind(asset_name, meshes, MoppKind::Spatial)
}

/// [`build_phy2_multi`] with an explicit [`MoppKind`] applied to EVERY shape's MOPP.
/// [`MoppKind::ReturnAll`] is the whole-floor collision diagnostic: each shape's spatial MOPP is replaced
/// by [`mopp::encode_return_all`] while its `m_info`, quantized mesh, and the merged 2N−1 BV-tree wrapper
/// stay byte-identical to the spatial build — so the ONLY delta across the N shapes is the `m_data`
/// bytecode. See [`MoppKind`] for the STAND/FALL interpretation.
pub fn build_phy2_multi_kind(
    asset_name: &str,
    meshes: &[MeshSoup],
    kind: MoppKind,
) -> Result<Vec<u8>, String> {
    build_phy2_multi_hashed_kind(crate::hash::pandemic_hash_m2(asset_name), meshes, kind)
}

/// [`build_phy2_multi`] keyed by the asset's name **hash** rather than its name string.
///
/// Prefix word 1 is the owning asset's name hash. Most callers know the asset by name and let
/// [`build_phy2_multi`] hash it. An asset known only by its hash — every retail terrain cell
/// (`0x7C569307`) is registered under a bare hash with no recovered name — passes the hash here, so
/// nothing has to invent a string that happens to hash to it. For
/// `name_hash == pandemic_hash_m2(asset_name)` the output is byte-identical to [`build_phy2_multi`].
pub fn build_phy2_multi_hashed(name_hash: u32, meshes: &[MeshSoup]) -> Result<Vec<u8>, String> {
    build_phy2_multi_hashed_kind(name_hash, meshes, MoppKind::Spatial)
}

/// [`build_phy2_multi_kind`] keyed by the asset's name hash — see [`build_phy2_multi_hashed`].
pub fn build_phy2_multi_hashed_kind(
    name_hash: u32,
    meshes: &[MeshSoup],
    kind: MoppKind,
) -> Result<Vec<u8>, String> {
    if meshes.is_empty() {
        return Err("build_phy2_multi: no meshes".into());
    }
    let n = meshes.len();
    // Build the retail SHARED-POOL layout: one common quantization frame + one global vertex pool, every
    // sub-mesh's index array in global indices (byte-measured on floor 0x39AF17DC). The per-sub MOPP is
    // baked over that sub's COMPACTED local geometry, so its leaf keys stay [0..ntris_i).
    let sms = build_shared_mesh_set(meshes)?;
    let mut mopps: Vec<(mopp::MoppInfo, Vec<u8>, u32)> = Vec::with_capacity(n);
    for s in &sms.subs {
        mopps.push(bake_shape_mopp(&s.local_tris, &s.local_verts, kind));
    }
    let per_mesh_ntris: Vec<u32> = sms.subs.iter().map(|s| s.tris_global.len() as u32).collect();

    let (_cn_body, cn_offs) = write_classnames(MESH_CLASSES);
    let cnoff = |name: &str| -> u32 {
        let i = MESH_CLASSES.iter().position(|(_, n)| *n == name).unwrap();
        cn_offs[i]
    };
    let pad16 = |b: &mut Vec<u8>| while !b.len().is_multiple_of(16) { b.push(0) };

    let mut body: Vec<u8> = Vec::new();
    let mut local: Vec<(u32, u32)> = Vec::new();
    let mut global: Vec<(u32, u32, u32)> = Vec::new();
    let mut virt: Vec<(u32, u32, u32)> = Vec::new();

    // WpArray root @0 (32B): +8 ptr(→elem), +12 size=N, +16 cap.
    let wparray_off = body.len() as u32; // 0
    body.resize(body.len() + 32, 0);
    put_u32(&mut body, 12, n as u32);
    put_u32(&mut body, 16, 0xC000_0000 | (n as u32 & 0x3FFF_FFFF));

    // WpArray element storage: N × u32 pointer (→ each bvtree), padded to 16.
    let elem_off = body.len() as u32; // 32
    body.resize(body.len() + n * 4, 0);
    pad16(&mut body);

    // Per-shape objects, interleaved: bvtree, moppcode, mopp-bytecode, mesh(+subpart), index-array.
    let mut bvtree_offs = Vec::with_capacity(n);
    let mut moppcode_offs = Vec::with_capacity(n);
    let mut mesh_offs = Vec::with_capacity(n);
    let mut moppbytes_offs = Vec::with_capacity(n);
    let mut index_offs = Vec::with_capacity(n);
    let mut secondary_offs = Vec::with_capacity(n);
    for i in 0..n {
        // hkpMoppBvTreeShape (64 zero bytes); pointers via global fixups.
        let bvtree_off = body.len() as u32;
        body.resize(body.len() + 64, 0);
        bvtree_offs.push(bvtree_off);

        // hkpMoppCode (48 bytes).
        let moppcode_off = body.len() as u32;
        body.resize(body.len() + 48, 0);
        let (info, code, count) = &mopps[i];
        put_f32(&mut body, moppcode_off as usize + 16, info.offset[0]);
        put_f32(&mut body, moppcode_off as usize + 20, info.offset[1]);
        put_f32(&mut body, moppcode_off as usize + 24, info.offset[2]);
        let lane3 = if info.scale.abs() > 0.0 { 1.0 / info.scale } else { 0.0 };
        put_f32(&mut body, moppcode_off as usize + 28, lane3);
        put_u32(&mut body, moppcode_off as usize + 36, *count);
        put_u32(&mut body, moppcode_off as usize + 40, 0xC000_0000 | (*count & 0x3FFF_FFFF));
        put_u32(&mut body, moppcode_off as usize + 44, 1); // buildType
        moppcode_offs.push(moppcode_off);

        // MOPP bytecode buffer, padded to 16.
        let moppbytes_off = body.len() as u32;
        body.extend_from_slice(code);
        pad16(&mut body);
        moppbytes_offs.push(moppbytes_off);

        // WpMeshShape16 base (48) + one subpart (48).
        let mesh_off = body.len() as u32;
        body.resize(body.len() + 96, 0);
        let m = mesh_off as usize;
        put_f32(&mut body, m + 24, CONVEX_RADIUS);
        put_u32(&mut body, m + 32, 1); // nsub
        put_u32(&mut body, m + 36, 0xC000_0001);
        // Subpart carries the COMMON frame (shared by all sub-meshes — the retail invariant).
        let sp = m + 48;
        put_f32(&mut body, sp, sms.min[0]);
        put_f32(&mut body, sp + 4, sms.min[1]);
        put_f32(&mut body, sp + 8, sms.min[2]);
        put_f32(&mut body, sp + 16, sms.scale[0]);
        put_f32(&mut body, sp + 20, sms.scale[1]);
        put_f32(&mut body, sp + 24, sms.scale[2]);
        put_f32(&mut body, sp + 28, 1.0);
        put_u32(&mut body, sp + 36, per_mesh_ntris[i]);
        put_u32(&mut body, sp + 44, sms.subs[i].vcount); // secondary count = this sub's owned vertex range
        mesh_offs.push(mesh_off);

        // Triangle index array (GLOBAL indices into the shared pool), padded to 16.
        let index_off = body.len() as u32;
        for t in &sms.subs[i].tris_global {
            body.extend_from_slice(&t[0].to_le_bytes());
            body.extend_from_slice(&t[1].to_le_bytes());
            body.extend_from_slice(&t[2].to_le_bytes());
            body.extend_from_slice(&0u16.to_le_bytes());
        }
        pad16(&mut body);
        index_offs.push(index_off);

        // Secondary vertex-index array (subpart+40): this sub's GLOBAL vertex indices [vbase..vbase+vcount) —
        // what WpMeshShape16::getAabb iterates (into obj+0x28 = the shared pool) to compute this shape's AABB.
        let secondary_off = body.len() as u32;
        let vb = sms.subs[i].vbase;
        for g in vb..vb + sms.subs[i].vcount {
            body.extend_from_slice(&(g as u16).to_le_bytes());
        }
        pad16(&mut body);
        secondary_offs.push(secondary_off);
    }

    // Fixups.
    local.push((wparray_off + 8, elem_off)); // WpArray.data → elem
    for i in 0..n {
        local.push((moppcode_offs[i] + 32, moppbytes_offs[i])); // m_data → bytecode
        local.push((mesh_offs[i] + 28, mesh_offs[i] + 48)); // subpart array → inline subpart
        local.push((mesh_offs[i] + 48 + 32, index_offs[i])); // subpart.indices → index array
        local.push((mesh_offs[i] + 48 + 40, secondary_offs[i])); // subpart.secondary → vertex-index array
    }
    local.sort_by_key(|(s, _)| *s);
    for i in 0..n {
        global.push((elem_off + i as u32 * 4, 2, bvtree_offs[i])); // WpArray elem[i] → bvtree[i]
        global.push((bvtree_offs[i] + 16, 2, moppcode_offs[i])); // bvtree.m_code → moppcode
        global.push((bvtree_offs[i] + 52, 2, mesh_offs[i])); // bvtree child → mesh
    }
    global.sort_by_key(|(s, _, _)| *s);
    virt.push((wparray_off, 0, cnoff("WpArray")));
    for i in 0..n {
        virt.push((bvtree_offs[i], 0, cnoff("hkpMoppBvTreeShape")));
        virt.push((moppcode_offs[i], 0, cnoff("hkpMoppCode")));
        virt.push((mesh_offs[i], 0, cnoff("WpMeshShape16")));
    }
    virt.sort_by_key(|(s, _, _)| *s);

    let data = DataSection { body, local, global, virt };
    let packfile = write_packfile(MESH_CLASSES, cnoff("WpArray"), &data);

    // ── Trailing wrapper: the FAITHFUL MERGED 2N−1 BV-tree ((N−1) internal AA nodes each with a CC
    //    union-AABB scratch + two child pointers, N leaf AA nodes each → its EE subpart), reproducing the
    //    retail floor's measured record/CC/EE/pool offset topology, then ONE shared quantized pool that every
    //    EE points to (retail form — all 4 EEs → wrapper+1232). Every stored pointer is chunk-body-absolute
    //    (pkend+rel); the loader relocates each as &body[0]+offset. ──
    let pkend = 48 + packfile.len();
    let wrapper =
        build_multi_mesh_wrapper(&sms.pool, sms.whole_min, sms.whole_max, &per_mesh_ntris, pkend, 1);
    let total_verts: usize = sms.pool.len();

    let mut out = Vec::with_capacity(48 + packfile.len() + wrapper.len());
    let mut prefix = [0u8; 48];
    put_u32(&mut prefix, 0, 0x39);
    put_u32(&mut prefix, 4, name_hash);
    put_u32(&mut prefix, 8, n as u32); // shape count
    put_u32(&mut prefix, 12, 1);
    put_u32(&mut prefix, 16, 1);
    put_u32(&mut prefix, 28, total_verts as u32);
    put_u32(&mut prefix, 32, packfile.len() as u32);
    out.extend_from_slice(&prefix);
    out.extend_from_slice(&packfile);
    out.extend_from_slice(&wrapper);

    // Prove the merged tree walks + every leaf shares the ONE pool (the retail form), AND that the packfile
    // WpArray binds N DISTINCT shapes (never collapsed onto shape[0]).
    debug_assert!(validate_multi_wrapper_chain(&out, pkend, &sms.pool, &per_mesh_ntris).is_ok());
    debug_assert!(validate_multi_shape_binding(&out, n).is_ok());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::havok::{parse_phy2_body, Shape};

    /// Decode a PHY2 body the way the reader does and return the single WpMeshShape16 mesh.
    fn decoded_mesh(body: &[u8]) -> crate::havok::MeshShape {
        let pf = parse_phy2_body(body).expect("parse authored PHY2");
        pf.shapes
            .iter()
            .find_map(|s| match s {
                Shape::Mesh(m) if !m.indices.is_empty() => Some(m.clone()),
                _ => None,
            })
            .expect("a decoded WpMeshShape16 mesh")
    }

    fn edge(p: [f32; 3], q: [f32; 3]) -> f32 {
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    }

    /// The writer round-trips through the reader: exact triangle indices back, vertices within the
    /// quantization error, for a quad, a box, and a ~2k-triangle soup.
    fn roundtrip(name: &str, tris: &[[u32; 3]], verts: &[[f32; 3]], quant_tol: f32) {
        let body = build_phy2(name, tris, verts).expect("build_phy2");
        let m = decoded_mesh(&body);

        // EXACT triangle indices (order + values).
        let got: Vec<[u16; 3]> = m.indices.clone();
        let want: Vec<[u16; 3]> = tris.iter().map(|t| [t[0] as u16, t[1] as u16, t[2] as u16]).collect();
        assert_eq!(got, want, "{name}: triangle indices must round-trip exactly");

        // Vertices within quantization error.
        let maxidx = tris.iter().flat_map(|t| t.iter()).copied().max().unwrap() as usize;
        for i in 0..=maxidx {
            let d = edge(m.vertices[i], verts[i]);
            assert!(d <= quant_tol, "{name}: vertex {i} dequant error {d} > {quant_tol}");
        }
    }

    #[test]
    fn roundtrip_quad() {
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let tris = vec![[0u32, 1, 2], [0, 2, 3]];
        roundtrip("quad", &tris, &verts, 1e-3);
    }

    #[test]
    fn roundtrip_box() {
        let verts = vec![
            [0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 3.0, 0.0], [0.0, 3.0, 0.0],
            [0.0, 0.0, 4.0], [2.0, 0.0, 4.0], [2.0, 3.0, 4.0], [0.0, 3.0, 4.0],
        ];
        let tris = vec![
            [0u32, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6],
            [0, 4, 5], [0, 5, 1], [1, 5, 6], [1, 6, 2],
            [2, 6, 7], [2, 7, 3], [3, 7, 4], [3, 4, 0],
        ];
        roundtrip("box", &tris, &verts, 1e-3);
    }

    /// A ~2k-triangle grid-plane soup (small cells → all edges ≪ 40 m so the reader's content scan
    /// locks the pool), exercising a large u16 index range + a big vertex pool.
    fn grid_mesh(n: usize) -> (Vec<[u32; 3]>, Vec<[f32; 3]>) {
        let mut verts = Vec::new();
        for z in 0..=n {
            for x in 0..=n {
                verts.push([x as f32 * 0.5, ((x + z) % 3) as f32 * 0.25, z as f32 * 0.5]);
            }
        }
        let w = (n + 1) as u32;
        let mut tris = Vec::new();
        for z in 0..n as u32 {
            for x in 0..n as u32 {
                let a = z * w + x;
                let (b, c, d) = (a + 1, a + w, a + w + 1);
                tris.push([a, c, b]);
                tris.push([b, c, d]);
            }
        }
        (tris, verts)
    }

    /// The hash-keyed entry point is the name-keyed one minus the hashing: same bytes for the hash of
    /// the same name, and prefix word 1 is exactly the hash passed in.
    #[test]
    fn hashed_entry_point_matches_the_named_one() {
        let meshes = vec![grid_mesh(6), grid_mesh(4)];
        let by_name = build_phy2_multi("hashed_vs_named", &meshes).expect("named build");
        let hash = crate::hash::pandemic_hash_m2("hashed_vs_named");
        let by_hash = build_phy2_multi_hashed(hash, &meshes).expect("hashed build");
        assert_eq!(by_name, by_hash, "the same hash must produce the same body");

        let other = build_phy2_multi_hashed(0xA241_BC0C, &meshes).expect("hashed build");
        assert_eq!(u32::from_le_bytes(other[4..8].try_into().unwrap()), 0xA241_BC0C);
        assert_eq!(other[8..], by_hash[8..], "only the name-hash word may differ");
    }

    #[test]
    fn roundtrip_soup_2k() {
        let (tris, verts) = grid_mesh(32); // 32*32*2 = 2048 triangles, 1089 verts
        assert!(tris.len() >= 2000, "expected ~2k tris, got {}", tris.len());
        roundtrip("soup2k", &tris, &verts, 1e-2);
    }

    /// Whole-PHY2 census + MOPP gates: the assembled body re-parses, shows all three collision classes,
    /// the MOPP decodes to the exact key set `[0..ntris)`, and `query_aabb` over the whole mesh is a
    /// superset of the brute-force AABB-overlap set (no-miss).
    #[test]
    fn assembled_phy2_census_and_mopp_gates() {
        let (tris, verts) = grid_mesh(20); // 800 tris
        let body = build_phy2("test_collider", &tris, &verts).expect("build");
        let pf = parse_phy2_body(&body).expect("parse");
        assert!(pf.version.starts_with("Havok-5.5"), "version {:?}", pf.version);
        for class in ["WpMeshShape16", "hkpMoppBvTreeShape", "hkpMoppCode"] {
            assert!(pf.class_counts.get(class).copied().unwrap_or(0) >= 1, "missing {class}");
        }
        assert_eq!(pf.class_counts.get("WpArray").copied(), Some(1), "one root WpArray");

        // Structural topology must match the retail canonical single-mesh graph: WpArray@0,
        // hkpMoppBvTreeShape@48, hkpMoppCode@112, then the mesh — and the child global fixups present.
        let off = body.windows(8).position(|w| w == crate::havok::HAVOK_MAGIC).unwrap();
        let raw = crate::havok::parse_packfile_raw(&body[off..]).unwrap();
        let mut vf: Vec<(usize, String)> = raw.vfixups.clone();
        vf.sort_by_key(|(s, _)| *s);
        assert_eq!(vf[0], (0, "WpArray".into()), "root at src 0");
        assert_eq!(vf[1], (48, "hkpMoppBvTreeShape".into()), "bvtree at 48");
        assert_eq!(vf[2], (112, "hkpMoppCode".into()), "moppcode at 112");
        assert_eq!(vf[3].1, "WpMeshShape16", "fourth object is the mesh");

        // Pull the MOPP m_data back out and decode it.
        let mopps = mopp::extract_mopp_with_info(&body);
        assert_eq!(mopps.len(), 1, "one hkpMoppCode");
        let (code, info) = &mopps[0];
        let dec = mopp::decode(code);
        assert!(dec.error.is_none(), "MOPP decodes clean: {:?}", dec.error);
        assert_eq!(dec.consumed, code.len(), "100% MOPP byte coverage");
        let (ks, range, missing) = dec.key_summary();
        assert_eq!(ks.len(), tris.len(), "one leaf key per triangle");
        assert_eq!(range, Some((0, tris.len() as u32 - 1)), "keys are [0..ntris)");
        assert!(missing.is_empty(), "no missing key");

        // query_aabb no-miss: for a handful of triangle-centred boxes, the pruned candidate set must
        // include the triangles that actually overlap the query box (brute force).
        let vc = |i: u32| verts[i as usize];
        for &ti in &[0usize, 137, 400, tris.len() - 1] {
            let (a, b, c) = (vc(tris[ti][0]), vc(tris[ti][1]), vc(tris[ti][2]));
            let mut qmin = [f32::MAX; 3];
            let mut qmax = [f32::MIN; 3];
            for v in [a, b, c] {
                for k in 0..3 {
                    qmin[k] = qmin[k].min(v[k]);
                    qmax[k] = qmax[k].max(v[k]);
                }
            }
            let cand: std::collections::HashSet<u32> =
                mopp::query_aabb(code, info, qmin, qmax).into_iter().collect();
            // brute-force overlap
            for (j, t) in tris.iter().enumerate() {
                let (ta, tb, tc) = (vc(t[0]), vc(t[1]), vc(t[2]));
                let mut tlo = [f32::MAX; 3];
                let mut thi = [f32::MIN; 3];
                for v in [ta, tb, tc] {
                    for k in 0..3 {
                        tlo[k] = tlo[k].min(v[k]);
                        thi[k] = thi[k].max(v[k]);
                    }
                }
                let overlap = (0..3).all(|k| tlo[k] <= qmax[k] && thi[k] >= qmin[k]);
                if overlap {
                    assert!(cand.contains(&(j as u32)), "query missed overlapping tri {j} (probe {ti})");
                }
            }
        }
    }

    /// Multi-shape assembly: `build_phy2_multi` over N meshes must produce the canonical N-shape graph
    /// (WpArray→N×{bvtree,moppcode,mesh}), re-parse to N WpMeshShape16 shapes carrying each mesh's OWN
    /// exact triangle indices, and N MOPPs each keyed `[0..tris_i)`.
    ///
    /// ⚠ This does NOT assert per-mesh vertex positions. The reader's vertex-pool locator is a tolerant
    /// CONTENT SCAN with no per-mesh pointer — for a container with several pools it cannot reliably pick
    /// pool[i] for mesh[i] (a wrong pool often still decodes to <40 m edges → a false lock). Indices come
    /// from the packfile (deterministic) so they are asserted; vertices via the scan are NOT a trustworthy
    /// multi-pool oracle. Authoring an ENGINE-valid multi-pool wrapper needs the retail pointer-framing
    /// (the `0xAA…/0xCC…` linked records whose fields hold body-absolute pool offsets) — see the deploy
    /// notes. That is why the shipped in-game gate targets a SINGLE-shape container (one unambiguous pool).
    #[test]
    fn multi_shape_roundtrip() {
        // Three IRREGULAR (deterministically-jittered) meshes CO-LOCATED in one tight ~60 m container —
        // exactly the regime the retail floor lives in (all 4 of its sub-meshes share one ~95 m container +
        // one quantization frame). The shared-pool layout puts all three into ONE pool under one COMMON
        // frame, so the reader locks the single unambiguous pool; the irregular jitter keeps it out of the
        // degenerate all-coincident false-lock. (The old test spread the meshes hundreds of metres apart to
        // force per-mesh pool discrimination — counterproductive now: a huge common frame would coarsen the
        // one shared pool and is unlike any real container.)
        let shape = |n: usize, seed: u32, dx: f32, dy: f32, dz: f32| {
            let (t, mut v) = grid_mesh(n);
            let mut s = seed;
            let mut rnd = || {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                (s >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            };
            for p in v.iter_mut() {
                p[0] = p[0] * 4.0 + dx + rnd() * 1.5;
                p[1] = p[1] * 4.0 + dy + rnd() * 1.5;
                p[2] = p[2] * 4.0 + dz + rnd() * 1.5;
            }
            (t, v)
        };
        let (t0, v0) = shape(10, 0x1111, 0.0, 0.0, 0.0); // 200 tris, ~20 m
        let (t1, v1) = shape(6, 0x9E37, 18.0, 3.0, -12.0); // 72 tris, nearby
        let (t2, v2) = shape(4, 0x5A5A, -14.0, 2.0, 16.0); // 32 tris, nearby
        let meshes = vec![(t0.clone(), v0.clone()), (t1.clone(), v1.clone()), (t2.clone(), v2.clone())];
        let body = build_phy2_multi("multi_collider", &meshes).expect("build_phy2_multi");

        let pf = parse_phy2_body(&body).expect("parse multi");
        assert_eq!(pf.class_counts.get("WpArray").copied(), Some(1), "one root");
        assert_eq!(pf.class_counts.get("WpMeshShape16").copied(), Some(3), "3 meshes");
        assert_eq!(pf.class_counts.get("hkpMoppBvTreeShape").copied(), Some(3), "3 bvtrees");
        assert_eq!(pf.class_counts.get("hkpMoppCode").copied(), Some(3), "3 moppcodes");

        let decoded: Vec<&crate::havok::MeshShape> = pf
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Mesh(m) if !m.indices.is_empty() => Some(m),
                _ => None,
            })
            .collect();
        assert_eq!(decoded.len(), 3, "3 decodable meshes");
        // Indices are GLOBAL now (shared pool), so we validate GEOMETRY not raw index values: each triangle's
        // three decoded vertex POSITIONS must match the input triangle's positions (order-preserving) within
        // the shared frame's quantization step. Same triangle COUNT + order per mesh.
        let sms = build_shared_mesh_set(&meshes).unwrap();
        let step = (sms.scale[0].powi(2) + sms.scale[1].powi(2) + sms.scale[2].powi(2)).sqrt();
        let tol = step * 2.0;
        for (i, (tris, verts)) in meshes.iter().enumerate() {
            assert_eq!(decoded[i].indices.len(), tris.len(), "mesh {i}: triangle count");
            for (t, gt) in tris.iter().enumerate() {
                for k in 0..3 {
                    let got = decoded[i].vertices[decoded[i].indices[t][k] as usize];
                    let want = verts[gt[k] as usize];
                    assert!(edge(got, want) <= tol, "mesh {i} tri {t} corner {k}: pos err {} > {tol}", edge(got, want));
                }
            }
        }

        // Topology: WpArray root, then bvtree/moppcode/mesh per shape, in order.
        let off = body.windows(8).position(|w| w == crate::havok::HAVOK_MAGIC).unwrap();
        let raw = crate::havok::parse_packfile_raw(&body[off..]).unwrap();
        let mut vf = raw.vfixups.clone();
        vf.sort_by_key(|(s, _)| *s);
        assert_eq!(vf[0].1, "WpArray");
        let bv = vf.iter().filter(|(_, c)| c == "hkpMoppBvTreeShape").count();
        let mc = vf.iter().filter(|(_, c)| c == "hkpMoppCode").count();
        let ms = vf.iter().filter(|(_, c)| c == "WpMeshShape16").count();
        assert_eq!((bv, mc, ms), (3, 3, 3), "3 of each collision class");

        // Each MOPP decodes to its own [0..tris_i).
        let mopps = mopp::extract_mopp_with_info(&body);
        assert_eq!(mopps.len(), 3, "3 MOPPs");
        for (i, (code, _)) in mopps.iter().enumerate() {
            let d = mopp::decode(code);
            assert!(d.error.is_none(), "mopp {i} decodes clean: {:?}", d.error);
            let (ks, range, missing) = d.key_summary();
            assert_eq!(ks.len(), meshes[i].0.len(), "mopp {i} one key per tri");
            assert_eq!(range, Some((0, meshes[i].0.len() as u32 - 1)), "mopp {i} keys [0..ntris)");
            assert!(missing.is_empty(), "mopp {i} no missing key");
        }

        // ★ FAITHFUL merged BV-tree (SHARED pool): the full recursive tree walk proves every internal
        // two-child + leaf EE/pool/tail pointer relocates in-bounds, EVERY leaf's EE+76 points to the ONE
        // shared pool, and EE+68 = the whole-container vertex count. Deterministic oracle (no content scan).
        let packfile_size = u32::from_le_bytes(body[32..36].try_into().unwrap()) as usize;
        let pkend = 48 + packfile_size;
        let per_mesh_ntris: Vec<u32> = sms.subs.iter().map(|s| s.tris_global.len() as u32).collect();
        validate_multi_wrapper_chain(&body, pkend, &sms.pool, &per_mesh_ntris)
            .expect("merged BV-tree (shared pool) must walk clean");
        // Every leaf's EE+76 must be the SAME shared pool pointer, and dequantizing each sub-mesh's GLOBAL
        // triangle indices from that pool (under the common frame) reproduces the input positions.
        let ic = internal_count(meshes.len());
        let cc_region = (ic + meshes.len()) * 68;
        let ee_region = cc_region + ic * 108;
        let shared_pool =
            u32::from_le_bytes(body[pkend + ee_region + 76..pkend + ee_region + 80].try_into().unwrap()) as usize;
        for (i, (tris, verts)) in meshes.iter().enumerate() {
            let ee = pkend + ee_region + i * 108;
            let poolptr = u32::from_le_bytes(body[ee + 76..ee + 76 + 4].try_into().unwrap()) as usize;
            assert_eq!(poolptr, shared_pool, "mesh {i}: EE+76 must be the ONE shared pool");
            let deqg = |g: usize| -> [f32; 3] {
                let o = shared_pool + g * 6;
                [
                    sms.min[0] + u16::from_le_bytes([body[o], body[o + 1]]) as f32 * sms.scale[0],
                    sms.min[1] + u16::from_le_bytes([body[o + 2], body[o + 3]]) as f32 * sms.scale[1],
                    sms.min[2] + u16::from_le_bytes([body[o + 4], body[o + 5]]) as f32 * sms.scale[2],
                ]
            };
            for (t, gt) in sms.subs[i].tris_global.iter().enumerate() {
                for k in 0..3 {
                    let got = deqg(gt[k] as usize);
                    let want = verts[tris[t][k] as usize];
                    assert!(edge(got, want) <= tol, "mesh {i} tri {t} corner {k}: shared-pool dequant err {} > {tol}", edge(got, want));
                }
            }
        }
    }

    /// ★ DISTINCT-SHAPE BINDING gate: the authored multi-shape PHY2 must bind N DISTINCT shapes via the
    /// packfile global-fixup graph (WpArray → N distinct bvtrees → N distinct moppcodes + N distinct
    /// meshes), the invariant the live A/B x32dbg proof said our floor violated (all 4 records → shape[0]).
    /// A positive check on a 4-mesh build, plus a NEGATIVE control: forcibly collapse the WpArray's element
    /// pointers so every element binds bvtree[0], and assert the gate REJECTS it — the exact failure the
    /// mesh/MOPP census is blind to (it enumerates meshes by virtual fixup, ignoring the global table).
    #[test]
    fn distinct_shape_binding_gate_catches_collapse() {
        let meshes: Vec<MeshSoup> = (0..4)
            .map(|s| {
                let (t, mut v) = grid_mesh(5 + s);
                for (i, p) in v.iter_mut().enumerate() {
                    p[0] += (s * 60) as f32;
                    p[1] += (i % 4) as f32 * 0.5;
                }
                (t, v)
            })
            .collect();
        let body = build_phy2_multi("bind_test", &meshes).expect("build");

        // Positive: the honest build binds 4 distinct shapes.
        validate_multi_shape_binding(&body, 4).expect("honest multi build must bind 4 distinct shapes");

        // Negative control: rewrite the WpArray element global fixups so elem[1..4] all point to bvtree[0],
        // then confirm the gate catches it (while a plain class census still counts 4 meshes).
        let off = body.windows(8).position(|w| w == crate::havok::HAVOK_MAGIC).unwrap();
        let raw = crate::havok::parse_packfile_raw(&body[off..]).unwrap();
        let wp = raw.vfixups.iter().find(|(_, c)| c == "WpArray").map(|(s, _)| *s).unwrap();
        let elem_data = *raw.lf.get(&(wp + 8)).unwrap();
        let bvtree0_dst = raw.gf.get(&elem_data).copied().unwrap().1;
        // Locate the global-fixup table bytes and force elem[1..4]'s dst to bvtree0.
        let mut bad = body.clone();
        // Section-header re-derivation (mirror parse_packfile_raw) to find the gf table.
        let pk0 = off;
        let sec = |s: usize, k: usize| {
            u32::from_le_bytes(
                bad[pk0 + 0x40 + s * 48 + 20 + k * 4..pk0 + 0x40 + s * 48 + 20 + k * 4 + 4]
                    .try_into()
                    .unwrap(),
            ) as usize
        };
        let body0 = pk0 + 0x40 + 3 * 48;
        let data_pk = body0 + sec(0, 6) + sec(1, 6);
        let (d_gf, d_vf) = (sec(2, 2), sec(2, 3));
        let mut k = data_pk + d_gf;
        let mut collapsed = 0;
        while k + 12 <= data_pk + d_vf {
            let src = u32::from_le_bytes(bad[k..k + 4].try_into().unwrap()) as usize;
            if src == 0xFFFF_FFFF {
                break;
            }
            // elem[1..4] live at elem_data+4, +8, +12 (data-relative src).
            if src == elem_data + 4 || src == elem_data + 8 || src == elem_data + 12 {
                bad[k + 8..k + 12].copy_from_slice(&(bvtree0_dst as u32).to_le_bytes());
                collapsed += 1;
            }
            k += 12;
        }
        assert_eq!(collapsed, 3, "should have rewritten 3 element fixups");
        // The census is still happy (4 mesh objects exist)…
        let pf = parse_phy2_body(&bad).expect("still parses");
        assert_eq!(pf.class_counts.get("WpMeshShape16").copied(), Some(4), "census still sees 4 meshes");
        // …but the binding gate REJECTS the collapse.
        assert!(
            validate_multi_shape_binding(&bad, 4).is_err(),
            "gate MUST reject a WpArray whose elements all bind bvtree[0]"
        );
    }

    /// ★ RETAIL-TOPOLOGY gate: for N=4 the authored merged BV-tree must reproduce the EXACT record/marker
    /// offsets byte-measured on the retail floor `0x39AF17DC` — 7 AA records (0,68,…,408), 3 CC blocks
    /// (476/584/692), 4 EE blocks (800/908/1016/1124), pools starting @1232 — and every pointer must be a
    /// PROVEN edge type (internal+56→leaf, internal+60→internal|FF, leaf+52→internal, leaf+60→leaf|FF),
    /// never the two unproven edges. This is the "byte-compare the tree topology against retail" gate.
    #[test]
    fn merged_tree_matches_retail_floor_offsets() {
        // Four irregular meshes (sizes are irrelevant to the header offsets, which depend only on N).
        let meshes: Vec<MeshSoup> = (0..4)
            .map(|s| {
                let (t, mut v) = grid_mesh(4 + s);
                for (i, p) in v.iter_mut().enumerate() {
                    p[0] += (s * 100) as f32;
                    p[1] += (i % 3) as f32 * 0.7;
                }
                (t, v)
            })
            .collect();
        let sms = build_shared_mesh_set(&meshes).unwrap();
        let per_mesh_ntris: Vec<u32> = sms.subs.iter().map(|s| s.tris_global.len() as u32).collect();
        let pkend = 4096usize;
        let w = build_multi_mesh_wrapper(&sms.pool, sms.whole_min, sms.whole_max, &per_mesh_ntris, pkend, 1);
        let mark = |rel: usize| u32::from_le_bytes(w[rel..rel + 4].try_into().unwrap());

        // 7 AA records @ 0,68,…,408, each with its BBBB terminator @+64.
        for r in 0..7 {
            assert_eq!(mark(r * 68), 0xAAAA_AAAA, "AA record {r} @ +{}", r * 68);
            assert_eq!(mark(r * 68 + 64), 0xBBBB_BBBB, "BBBB {r} @ +{}", r * 68 + 64);
        }
        // 3 CC blocks (internals) @ 476/584/692, each DDDD @+104.
        for (j, cc) in [476usize, 584, 692].into_iter().enumerate() {
            assert_eq!(mark(cc), 0xCCCC_CCCC, "CC {j} @ +{cc}");
            assert_eq!(mark(cc + 104), 0xDDDD_DDDD, "DDDD {j} @ +{}", cc + 104);
        }
        // 4 EE blocks (leaves) @ 800/908/1016/1124.
        for (l, ee) in [800usize, 908, 1016, 1124].into_iter().enumerate() {
            assert_eq!(mark(ee), 0xEEEE_EEEE, "EE {l} @ +{ee}");
        }
        // Pool region starts @1232 (right after the 4th EE block).
        assert_eq!(800 + 4 * 108, 1232, "EE region ends at the retail pool offset");

        // The full recursive walk must pass (proves every relocated pointer + all 4 leaves reached).
        let body = {
            let mut b = vec![0u8; pkend];
            b.extend_from_slice(&w);
            b
        };
        validate_multi_wrapper_chain(&body, pkend, &sms.pool, &per_mesh_ntris)
            .expect("retail-topology merged tree walks clean");

        // Every internal (records 0..3) exposes +40→CC and +56→a LEAF record (proven edge); the root's and
        // interior internals' +60 → an internal or FF (proven), never a leaf.
        let ff = 0xFFFF_FFFFu32;
        let field = |rec: usize, off: usize| u32::from_le_bytes(w[rec * 68 + off..rec * 68 + off + 4].try_into().unwrap());
        let is_leaf_ptr = |v: u32| -> bool {
            v != ff && ((v as usize - pkend) / 68) >= 3 // leaf records are 3..7
        };
        let is_internal_ptr = |v: u32| -> bool { v != ff && ((v as usize - pkend) / 68) < 3 };
        for j in 0..3 {
            assert_ne!(field(j, 40), ff, "internal {j} must have +40→CC");
            assert!(is_leaf_ptr(field(j, 56)), "internal {j} +56 must point to a LEAF (proven edge)");
            let c60 = field(j, 60);
            assert!(c60 == ff || is_internal_ptr(c60), "internal {j} +60 must be internal|FF (proven edge)");
            assert!(!is_leaf_ptr(c60), "internal {j} +60 must NOT be a leaf (unproven edge)");
        }
    }

    /// The multi-mesh wrapper is a strict generalization of the single-mesh one: for N=1 the two builders
    /// must emit BYTE-IDENTICAL wrappers, so the in-game-proven single-mesh format is preserved exactly.
    #[test]
    fn multi_wrapper_n1_equals_single() {
        let (tris, verts) = grid_mesh(16);
        let mesh = write_wpmesh16(&tris, &verts).unwrap();
        let (mut vmin, mut vmax) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &verts {
            for k in 0..3 {
                vmin[k] = vmin[k].min(v[k]);
                vmax[k] = vmax[k].max(v[k]);
            }
        }
        let pkend = 12345; // arbitrary base; both builders use the same relocation arithmetic
        let single = build_mesh_wrapper(&mesh, vmin, vmax, pkend, 1);
        let multi =
            build_multi_mesh_wrapper(&mesh.pool, vmin, vmax, &[mesh.tris.len() as u32], pkend, 1);
        assert_eq!(single, multi, "N=1 multi wrapper must byte-match the proven single-mesh wrapper");
    }

    /// ★ FAITHFUL-WRAPPER gate: the authored PHY2's trailing wrapper is an engine-walkable AA/BB/CC/DD/EE
    /// descriptor chain (reversed from retail 0xE8EB75D7/0x86D7CF92) — every chunk-body-absolute pointer
    /// the loader relocates (`&body[0]+offset`) resolves to its descriptor / the pool. This is the exact
    /// property the crashed pool-first layout lacked (its AA-record pointer fields were pool bytes →
    /// `&body[0]+garbage` = A48E1608 → AV @0x0248C15A). Positive walk + a negative control (a corrupted
    /// pool pointer must be caught).
    #[test]
    fn faithful_wrapper_chain_walks_and_catches_corruption() {
        let (tris, verts) = grid_mesh(20); // 800 tris, 441 verts — irregular enough for the reader
        let mesh = write_wpmesh16(&tris, &verts).unwrap();
        let body = build_phy2("faithful_collider", &tris, &verts).expect("build");

        // Locate the wrapper start (pkend) the same way the loader does: prefix[32] = packfile size.
        let packfile_size = u32::from_le_bytes(body[32..36].try_into().unwrap()) as usize;
        let pkend = 48 + packfile_size;

        // The wrapper header markers must land at the PROVEN fixed relative offsets.
        let m = |rel: usize| u32::from_le_bytes(body[pkend + rel..pkend + rel + 4].try_into().unwrap());
        assert_eq!(m(0), 0xAAAA_AAAA, "AA#1 marker at wrapper+0");
        assert_eq!(m(64), 0xBBBB_BBBB, "AA#1 end at +64");
        assert_eq!(m(68), 0xAAAA_AAAA, "AA#2 at +68");
        assert_eq!(m(132), 0xBBBB_BBBB, "AA#2 end at +132");
        assert_eq!(m(136), 0xCCCC_CCCC, "CC at +136");
        assert_eq!(m(240), 0xDDDD_DDDD, "DD at +240");
        assert_eq!(m(244), 0xEEEE_EEEE, "EE at +244");

        // Full engine-faithful chain walk must succeed.
        validate_wrapper_chain(&body, pkend, &mesh).expect("faithful wrapper must walk clean");

        // The pool pointer (EE+76 = wrapper+320) must be the body-absolute offset of wrapper+352.
        let pool_ptr = m(320) as usize;
        assert_eq!(pool_ptr, pkend + 352, "EE+76 pool pointer = pkend+352 (body-absolute)");
        assert_eq!(
            &body[pool_ptr..pool_ptr + 6],
            &mesh.pool_bytes()[..6],
            "relocated pool pointer lands on the first quantized vertex"
        );

        // Negative control: corrupt the pool pointer → the walk must reject it (this is the class of bug
        // that produced the in-game AV).
        let mut bad = body.clone();
        let corrupt = 0xA48E_1608u32.to_le_bytes(); // the exact garbage offset from the crash
        bad[pkend + 320..pkend + 320 + 4].copy_from_slice(&corrupt);
        assert!(
            validate_wrapper_chain(&bad, pkend, &mesh).is_err(),
            "a corrupted pool pointer must be caught by the chain gate"
        );
    }

    /// Assert the [`MoppKind::ReturnAll`] build changes ONLY the MOPP `m_data` bytecode vs the
    /// [`MoppKind::Spatial`] build of the same `meshes`: same total length, per-shape `m_info` frame
    /// identical, bytecode different but still decoding to `[0..ntris)`, and every differing byte confined
    /// to a shape's MOPP buffer (⇒ mesh, subpart, index arrays, and the ENTIRE trailing wrapper are
    /// byte-identical). Returns the number of differing bytes (all inside MOPP buffers) for reporting.
    fn assert_only_mopp_differs(spatial: &[u8], returnall: &[u8], meshes: &[MeshSoup]) -> usize {
        let n = meshes.len();
        // (1) Same length ⇒ same packfile length ⇒ same pkend ⇒ the wrapper cannot have moved.
        assert_eq!(spatial.len(), returnall.len(), "return-all must not change the body length");

        // (2) m_info identical, bytecode different, keys still [0..ntris) for BOTH.
        let ms = mopp::extract_mopp_with_info(spatial);
        let mr = mopp::extract_mopp_with_info(returnall);
        assert_eq!(ms.len(), n, "spatial MOPP count");
        assert_eq!(mr.len(), n, "return-all MOPP count");
        for i in 0..n {
            assert_eq!(ms[i].1, mr[i].1, "shape {i}: m_info frame must be identical");
            assert_ne!(ms[i].0, mr[i].0, "shape {i}: the bytecode must actually differ");
            for (tag, code) in [("spatial", &ms[i].0), ("returnall", &mr[i].0)] {
                let d = mopp::decode(code);
                assert!(d.error.is_none() && d.consumed == code.len(), "shape {i} {tag} decode clean");
                let (ks, range, missing) = d.key_summary();
                assert_eq!(ks.len(), meshes[i].0.len(), "shape {i} {tag}: one key per tri");
                assert_eq!(range, Some((0, meshes[i].0.len() as u32 - 1)), "shape {i} {tag}: [0..ntris)");
                assert!(missing.is_empty(), "shape {i} {tag}: no missing key");
            }
        }

        // (3) Every byte that differs must lie inside one shape's spatial MOPP buffer; everything else is
        //     byte-identical (m_info, mesh, subpart, index arrays, and the ENTIRE trailing wrapper).
        let mut mask = vec![false; spatial.len()];
        let mut cursor = 0usize;
        for (i, (code, _)) in ms.iter().enumerate() {
            // The spatial bytecode is a unique byte string; locate it (searching forward from the last hit
            // keeps the per-shape regions in order).
            let at = cursor
                + spatial[cursor..]
                    .windows(code.len())
                    .position(|w| w == code.as_slice())
                    .unwrap_or_else(|| panic!("shape {i}: spatial MOPP buffer not found in body"));
            for b in mask.iter_mut().take(at + code.len()).skip(at) {
                *b = true;
            }
            // The return-all bytecode occupies exactly the same span.
            assert_eq!(
                &returnall[at..at + mr[i].0.len()],
                mr[i].0.as_slice(),
                "shape {i}: return-all bytecode must sit in the SAME span as the spatial buffer"
            );
            cursor = at + code.len();
        }
        let mut ndiff = 0usize;
        for (idx, (&a, &b)) in spatial.iter().zip(returnall.iter()).enumerate() {
            if a != b {
                ndiff += 1;
                assert!(mask[idx], "byte {idx} differs but is OUTSIDE any MOPP m_data buffer (mesh/wrapper/m_info changed!)");
            }
        }
        ndiff
    }

    /// ★ CONTROLLED-DIFF gate for the collision diagnostic: [`MoppKind::ReturnAll`] must change ONLY the
    /// MOPP `m_data` bytecode — the `m_info` frame, the quantized mesh, and the whole merged BV-tree wrapper
    /// must be byte-identical to the [`MoppKind::Spatial`] build. This is exactly what makes the in-game
    /// STAND/FALL test a clean single-variable experiment (only the spatial frame vs a frame-ignoring
    /// bytecode changes).
    #[test]
    fn returnall_only_changes_mopp_mdata() {
        // Four irregular meshes standing in for the floor's 4 shapes (real geometry uses the same builder).
        let meshes: Vec<MeshSoup> = (0..4)
            .map(|s| {
                let (t, mut v) = grid_mesh(6 + s * 3);
                let mut seed = 0xC0FFEEu32 ^ (s as u32).wrapping_mul(0x9E3779B1);
                for (i, p) in v.iter_mut().enumerate() {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    p[0] = p[0] * 4.0 + (s * 137) as f32 + (seed >> 9) as f32 / (1u32 << 23) as f32;
                    p[1] = p[1] * 4.0 + (i % 5) as f32 * 0.6;
                    p[2] = p[2] * 4.0 - (s * 91) as f32;
                }
                (t, v)
            })
            .collect();

        let spatial = build_phy2_multi_kind("diag", &meshes, MoppKind::Spatial).expect("spatial");
        let returnall = build_phy2_multi_kind("diag", &meshes, MoppKind::ReturnAll).expect("returnall");
        let ndiff = assert_only_mopp_differs(&spatial, &returnall, &meshes);
        assert!(ndiff > 0, "the bytecode must actually change somewhere");
    }

    /// ★ REAL-FLOOR controlled-diff proof: decode the retail PMC-HQ floor `0x39AF17DC` (block 2612, the
    /// 4-shape STAND surface), author its PHY2 both ways, and prove the return-all build differs from the
    /// spatial build ONLY in the 4 MOPP `m_data` blocks — m_info, the 4 quantized meshes, and the merged
    /// 2N−1 BV-tree wrapper are byte-identical. This is the exact controlled-test invariant behind the
    /// floor4-returnall overlay. SKIPS (stays green) when `vz.wad` is absent.
    #[test]
    fn returnall_floor_only_changes_mopp_mdata_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        use crate::ucfx::{extract_chunk_body, parse_block_entry_table};
        let Some(path) = crate::game_paths::vz_wad_from_env()
            .or_else(|| crate::game_paths::wad_from_local_config(std::path::Path::new(".")))
        else {
            return eprintln!("SKIPPING returnall_floor: vz.wad not found");
        };
        let mut f = std::fs::File::open(&path).unwrap();
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs");
        let dec = decompress_block(&mut f, &arch.indx, 2612).expect("block 2612");
        let (count, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + count as usize * 16;
        let mut floor: Option<Vec<u8>> = None;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if end > dec.len() {
                break;
            }
            if e.name_hash == 0x39AF_17DC {
                floor = extract_chunk_body(&dec[pos..end], b"PHY2");
                if floor.is_some() {
                    break;
                }
            }
            pos = end;
        }
        let Some(body) = floor else {
            return eprintln!("SKIPPING returnall_floor: floor 0x39AF17DC not found in block 2612");
        };
        let pf = parse_phy2_body(&body).expect("parse floor PHY2");
        let meshes: Vec<MeshSoup> = pf
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Mesh(m) if !m.indices.is_empty() => Some((
                    m.indices.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect(),
                    m.vertices.clone(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(meshes.len(), 4, "floor 0x39AF17DC decodes to 4 collision meshes");

        let spatial = build_phy2_multi_kind("floor", &meshes, MoppKind::Spatial).expect("spatial");
        let returnall = build_phy2_multi_kind("floor", &meshes, MoppKind::ReturnAll).expect("returnall");
        let ndiff = assert_only_mopp_differs(&spatial, &returnall, &meshes);
        eprintln!(
            "REAL FLOOR 0x39AF17DC: spatial vs return-all authored PHY2 = {} B each; {} byte(s) differ, \
             ALL inside the 4 MOPP m_data blocks (m_info + 4 meshes + merged BV-tree wrapper byte-identical)",
            spatial.len(),
            ndiff
        );
    }

    /// ★ DEPLOYED-ARTIFACT byte-diff: the spatial-fixed floor overlay vs the deployed return-all STAND
    /// build differ ONLY in (a) the 4 MOPP `m_data` bytecode blocks and (b) the 4 `m_info` lane-3 scale
    /// words (`hkpMoppCode obj+28`) — the frame correction the shift-16 fix REQUIRES (`scale` goes from a
    /// 16-bit `ext/0xFF00` frame to the engine's 24-bit `ext/0xFF0000` frame). Every other byte —
    /// `m_info` offset xyz, the shared vertex pool, all index/secondary arrays, the merged BV-tree wrapper
    /// — is byte-identical. Reads the two standalone overlay WADs from the hunt dir; SKIPS if absent.
    #[test]
    fn spatialfix_vs_returnall_only_mdata_and_scale_differ_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        use crate::ucfx::{extract_chunk_body, parse_block_entry_table};
        let ov = std::path::Path::new("C:/Users/Shadow/AppData/Local/Temp/hunt/mopp/overlay");
        let old_p = ov.join("vz-patch-authored-floor4-sharedpool-returnall.wad");
        let new_p = ov.join("vz-patch-authored-floor4-sharedpool-spatialfix.wad");
        if !old_p.exists() || !new_p.exists() {
            return eprintln!("SKIPPING spatialfix_vs_returnall: overlay WADs not present");
        }
        // Extract the 0x39AF17DC PHY2 body from block 2612 of a patch WAD.
        let extract_floor = |p: &std::path::Path| -> Vec<u8> {
            let mut f = std::fs::File::open(p).unwrap();
            let size = f.metadata().unwrap().len();
            let arch = load_ffcs_archive(&mut f, size).expect("ffcs");
            // Patch WADs reindex blocks locally, so scan every block for the floor entry.
            for bi in 0..arch.indx.len() {
                let Ok(dec) = decompress_block(&mut f, &arch.indx, bi as u16) else {
                    continue;
                };
                let (count, entries) = parse_block_entry_table(&dec);
                let mut pos = 4 + count as usize * 16;
                for e in &entries {
                    let end = pos + e.chunk_size as usize;
                    if end > dec.len() {
                        break;
                    }
                    if e.name_hash == 0x39AF_17DC {
                        if let Some(b) = extract_chunk_body(&dec[pos..end], b"PHY2") {
                            return b;
                        }
                    }
                    pos = end;
                }
            }
            panic!("floor 0x39AF17DC PHY2 not found in {p:?}");
        };
        let old_body = extract_floor(&old_p);
        let new_body = extract_floor(&new_p);
        assert_eq!(old_body.len(), new_body.len(), "bodies must be the same length (wrapper unmoved)");

        // Build the allowed-diff mask over the NEW body: each hkpMoppCode's m_data content span + its
        // 4-byte lane-3 scale word (obj+28).
        let off = new_body.windows(8).position(|w| w == crate::havok::HAVOK_MAGIC).unwrap();
        let raw = crate::havok::parse_packfile_raw(&new_body[off..]).expect("parse new body packfile");
        let mut mask = vec![false; new_body.len()];
        let mut n_mopp = 0usize;
        for (src, cname) in &raw.vfixups {
            if cname != "hkpMoppCode" {
                continue;
            }
            n_mopp += 1;
            let obj = raw.data_pk + src; // body-absolute
            // lane-3 scale word at obj+28..+32 (offset xyz at +16/+20/+24 stay identical).
            for b in mask.iter_mut().take(off + obj + 32).skip(off + obj + 28) {
                *b = true;
            }
            // m_data content span: ptr @ obj+32 (local fixup), count @ obj+36.
            if let Some(ptr) = raw.resolve_ptr(*src, 32) {
                let count = u32::from_le_bytes(
                    new_body[off + obj + 36..off + obj + 40].try_into().unwrap(),
                ) as usize;
                for b in mask.iter_mut().take(off + ptr + count).skip(off + ptr) {
                    *b = true;
                }
            }
        }
        assert_eq!(n_mopp, 4, "expected 4 hkpMoppCode objects");

        let mut ndiff = 0usize;
        let mut n_scale = 0usize;
        let mut n_mdata = 0usize;
        for (idx, (&a, &b)) in old_body.iter().zip(new_body.iter()).enumerate() {
            if a != b {
                ndiff += 1;
                assert!(
                    mask[idx],
                    "byte {idx} differs but is OUTSIDE any MOPP m_data / lane3 span (mesh/pool/wrapper/offset changed!)"
                );
            }
        }
        // Count how the diff splits between the two allowed regions (informational).
        for (src, cname) in &raw.vfixups {
            if cname != "hkpMoppCode" {
                continue;
            }
            let obj = raw.data_pk + src;
            for k in (off + obj + 28)..(off + obj + 32) {
                if old_body[k] != new_body[k] {
                    n_scale += 1;
                }
            }
            if let Some(ptr) = raw.resolve_ptr(*src, 32) {
                let count = u32::from_le_bytes(new_body[off + obj + 36..off + obj + 40].try_into().unwrap()) as usize;
                for k in (off + ptr)..(off + ptr + count) {
                    if old_body[k] != new_body[k] {
                        n_mdata += 1;
                    }
                }
            }
        }
        eprintln!(
            "DEPLOYED DIFF (spatialfix vs return-all): body {} B, {ndiff} bytes differ — {n_mdata} in the 4 \
             MOPP m_data blocks + {n_scale} in the 4 m_info lane3 scale words; ALL else byte-identical \
             (offset xyz, pool, indices, wrapper).",
            new_body.len()
        );
    }

    /// ★ SPATIAL-MOPP NO-MISS + PRUNE GATE (the fall-through fix's offline judge). For the retail floor
    /// `0x39AF17DC`, author the SHARED-POOL spatial MOPPs exactly as the forge does (bake over each sub's
    /// compacted local geometry), then query each MOPP in the SAME space the engine queries — the
    /// COMMON-FRAME-DEQUANTIZED shape-local positions the mesh verts actually resolve to at load. Asserts,
    /// per subpart: (1) the MOPP decodes to `[0..ntris_i)`; (2) **0 misses** — a query at every triangle's
    /// world AABB returns that triangle's key; (3) **real pruning** — a box far outside the mesh returns
    /// ZERO candidates (not a degenerate return-all). This is what `ROOT_SHIFT=16` buys: at the old shift 8
    /// the RIGHT-child planes landed 256× out and a far box could not be pruned / near tris were missed.
    /// SKIPS (stays green) when `vz.wad` is absent.
    #[test]
    fn spatial_mopp_no_miss_and_prunes_in_common_frame_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        use crate::ucfx::{extract_chunk_body, parse_block_entry_table};
        let Some(path) = crate::game_paths::vz_wad_from_env()
            .or_else(|| crate::game_paths::wad_from_local_config(std::path::Path::new(".")))
        else {
            return eprintln!("SKIPPING spatial_mopp_no_miss: vz.wad not found");
        };
        let mut f = std::fs::File::open(&path).unwrap();
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs");
        let dec = decompress_block(&mut f, &arch.indx, 2612).expect("block 2612");
        let (count, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + count as usize * 16;
        let mut floor: Option<Vec<u8>> = None;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if end > dec.len() {
                break;
            }
            if e.name_hash == 0x39AF_17DC {
                floor = extract_chunk_body(&dec[pos..end], b"PHY2");
                if floor.is_some() {
                    break;
                }
            }
            pos = end;
        }
        let Some(body) = floor else {
            return eprintln!("SKIPPING spatial_mopp_no_miss: floor not found");
        };
        let pf = parse_phy2_body(&body).expect("parse floor");
        let meshes: Vec<MeshSoup> = pf
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Mesh(m) if !m.indices.is_empty() => Some((
                    m.indices.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect(),
                    m.vertices.clone(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(meshes.len(), 4, "floor decodes to 4 meshes");

        let sms = build_shared_mesh_set(&meshes).expect("shared mesh set");
        // Common-frame dequant of a GLOBAL pool index — the exact world-local position the engine sees.
        let deq = |g: usize| -> [f32; 3] {
            [
                sms.min[0] + sms.pool[g][0] as f32 * sms.scale[0],
                sms.min[1] + sms.pool[g][1] as f32 * sms.scale[1],
                sms.min[2] + sms.pool[g][2] as f32 * sms.scale[2],
            ]
        };
        let mut total_prune_hits = 0usize;
        for (i, sub) in sms.subs.iter().enumerate() {
            let (info, code, _cnt) = bake_shape_mopp(&sub.local_tris, &sub.local_verts, MoppKind::Spatial);
            // (1) decodes to [0..ntris_i)
            let d = mopp::decode(&code);
            let (ks, range, missing) = d.key_summary();
            let ntris = sub.tris_global.len();
            assert_eq!(ks.len(), ntris, "sub {i}: one key per triangle");
            assert_eq!(range, Some((0, ntris as u32 - 1)), "sub {i}: keys [0..ntris)");
            assert!(missing.is_empty(), "sub {i}: contiguous keys");

            // Per-triangle common-frame world AABB + the whole-sub AABB.
            let (mut wlo, mut whi) = ([f32::MAX; 3], [f32::MIN; 3]);
            let tri_box = |t: usize| -> ([f32; 3], [f32; 3]) {
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for &g in &sub.tris_global[t] {
                    let v = deq(g as usize);
                    for k in 0..3 {
                        lo[k] = lo[k].min(v[k]);
                        hi[k] = hi[k].max(v[k]);
                    }
                }
                (lo, hi)
            };
            // (2) 0 misses over every triangle, in common-frame space.
            let mut miss = 0usize;
            for t in 0..ntris {
                let (lo, hi) = tri_box(t);
                for k in 0..3 {
                    wlo[k] = wlo[k].min(lo[k]);
                    whi[k] = whi[k].max(hi[k]);
                }
                if !mopp::query_aabb(&code, &info, lo, hi).contains(&(t as u32)) {
                    miss += 1;
                }
            }
            assert_eq!(miss, 0, "sub {i}: {miss}/{ntris} triangles MISSED by their own spatial MOPP query");

            // (3) real pruning (NOT degenerate return-all): a box far outside the mesh must return
            // STRICTLY FEWER candidates than the return-all MOPP of the same triangles (which returns
            // ALL ntris under any box). A CUT-less BIH still over-includes on the unbounded (max) side of
            // the offset child — that is benign conservatism (the narrowphase rejects extras; it never
            // drops the floor). What must hold is that the spatial tree discriminates space at all.
            let ext = [whi[0] - wlo[0], whi[1] - wlo[1], whi[2] - wlo[2]];
            let em = ext.iter().cloned().fold(0.0f32, f32::max).max(1.0);
            let far_lo = [whi[0] + em + 50.0, wlo[1], wlo[2]];
            let far_hi = [whi[0] + em + 60.0, whi[1], whi[2]];
            // Positive discrimination: a query overlapping ONLY triangle 0's tiny box must return far
            // fewer than every triangle (a degenerate all-visit tree would return all ntris).
            let (t0lo, t0hi) = tri_box(0);
            let near0 = mopp::query_aabb(&code, &info, t0lo, t0hi).len();
            let far_spatial = mopp::query_aabb(&code, &info, far_lo, far_hi).len();
            assert!(
                far_spatial < ntris && near0 < ntris,
                "sub {i}: no pruning — far={far_spatial}/{ntris}, near0={near0}/{ntris} (degenerate all-visit)"
            );
            total_prune_hits += 1;
            eprintln!(
                "  sub {i}: {ntris} tris — 0 misses; near-tri0 cands={near0}, far box cands={far_spatial} \
                 (far pruned {:.0}%)",
                100.0 * (1.0 - far_spatial as f32 / ntris as f32)
            );
        }
        assert_eq!(total_prune_hits, 4, "all 4 subparts must pass no-miss + prune");
        eprintln!(
            "SPATIAL-MOPP GATE (ROOT_SHIFT=16): floor 0x39AF17DC — all 4 subparts no-miss over their own \
             triangles in common-frame-dequantized space AND prune a far box below return-all."
        );
    }

    /// ★ RETAIL CROSS-CHECK for the distinct-shape binding gate: decode the retail PMC-HQ floor
    /// `0x39AF17DC` (block 2612), and prove (a) RETAIL's own packfile binds 4 DISTINCT shapes
    /// (`WpArray → 4 distinct bvtrees → 4 distinct moppcodes + 4 distinct meshes` — i.e. `record[i] →
    /// shape[i]`, the invariant our authored floor must match), and (b) our re-authored floor from the same
    /// 4 meshes ALSO passes the gate. This is the offline form of the live A/B x32dbg comparison.
    /// SKIPS (stays green) when `vz.wad` is absent.
    #[test]
    fn retail_floor_binds_four_distinct_shapes_and_reauthor_matches() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        use crate::ucfx::{extract_chunk_body, parse_block_entry_table};
        let Some(path) = crate::game_paths::vz_wad_from_env()
            .or_else(|| crate::game_paths::wad_from_local_config(std::path::Path::new(".")))
        else {
            return eprintln!("SKIPPING retail_floor_binds: vz.wad not found");
        };
        let mut f = std::fs::File::open(&path).unwrap();
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs");
        let dec = decompress_block(&mut f, &arch.indx, 2612).expect("block 2612");
        let (count, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + count as usize * 16;
        let mut floor: Option<Vec<u8>> = None;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if end > dec.len() {
                break;
            }
            if e.name_hash == 0x39AF_17DC {
                floor = extract_chunk_body(&dec[pos..end], b"PHY2");
                if floor.is_some() {
                    break;
                }
            }
            pos = end;
        }
        let Some(body) = floor else {
            return eprintln!("SKIPPING retail_floor_binds: floor not found");
        };

        // (a) RETAIL binds 4 distinct shapes — record[i] → shape[i], never collapsed.
        validate_multi_shape_binding(&body, 4)
            .expect("RETAIL floor 0x39AF17DC must bind 4 DISTINCT shapes (record[i]→shape[i])");

        // (b) Our re-author from the same 4 decoded meshes also binds 4 distinct shapes.
        let pf = parse_phy2_body(&body).expect("parse floor");
        let soup: Vec<MeshSoup> = pf
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Mesh(m) if !m.indices.is_empty() => Some((
                    m.indices.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect(),
                    m.vertices.clone(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(soup.len(), 4, "floor decodes to 4 meshes");
        let authored = build_phy2_multi("floor", &soup).expect("re-author floor");
        validate_multi_shape_binding(&authored, 4)
            .expect("re-authored floor must bind 4 DISTINCT shapes");
        eprintln!("retail floor + re-authored floor BOTH bind 4 distinct shapes (record[i]→shape[i])");
    }

    /// REAL-BUILDING validation: take a genuine single-subpart `WpMeshShape16` out of retail `vz.wad`,
    /// decode it, re-author a PHY2 from those exact tris/verts, and prove the re-authored body decodes
    /// back to the SAME triangle indices + a functional MOPP. Reports structural deltas vs the original.
    /// SKIPS (stays green) when `vz.wad` is absent.
    #[test]
    fn reauthor_real_building_from_vz_wad_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        use crate::ucfx::{extract_chunk_body, parse_block_entry_table};
        let Some(path) = crate::game_paths::vz_wad_from_env()
            .or_else(|| crate::game_paths::wad_from_local_config(std::path::Path::new(".")))
        else {
            return eprintln!("SKIPPING reauthor_real_building: vz.wad not found");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("SKIPPING reauthor_real_building: vz.wad not readable");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs");
        // Block 767 = small building colliders; find a decoded single-mesh with a recovered pool.
        let dec = decompress_block(&mut f, &arch.indx, 767).expect("block 767");
        let (count, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + count as usize * 16;
        let mut src_mesh: Option<crate::havok::MeshShape> = None;
        'outer: for e in &entries {
            let end = pos + e.chunk_size as usize;
            if end > dec.len() {
                break;
            }
            if let Some(b) = extract_chunk_body(&dec[pos..end], b"PHY2") {
                if let Ok(pf) = parse_phy2_body(&b) {
                    for s in pf.shapes {
                        if let Shape::Mesh(m) = s {
                            if !m.indices.is_empty() && m.vertices.len() <= 65_536 {
                                src_mesh = Some(m);
                                break 'outer;
                            }
                        }
                    }
                }
            }
            pos = end;
        }
        let Some(src) = src_mesh else {
            return eprintln!("SKIPPING reauthor_real_building: no decodable WpMeshShape16 in block 767");
        };

        let verts: Vec<[f32; 3]> = src.vertices.clone();
        let tris: Vec<[u32; 3]> =
            src.indices.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect();

        let body = build_phy2("reauthored_building", &tris, &verts).expect("build");
        let pf = parse_phy2_body(&body).expect("re-parse authored building");
        for class in ["WpMeshShape16", "hkpMoppBvTreeShape", "hkpMoppCode"] {
            assert!(pf.class_counts.get(class).copied().unwrap_or(0) >= 1, "authored missing {class}");
        }
        let re = decoded_mesh(&body);
        assert_eq!(re.indices, src.indices, "re-authored mesh must decode to the SAME triangle indices");
        // vertices within quant error of the source (source is itself quantized, so tolerance is loose).
        let maxidx = *src.indices.iter().flat_map(|t| t.iter()).max().unwrap() as usize;
        for i in 0..=maxidx {
            let d = edge(re.vertices[i], src.vertices[i]);
            assert!(d < 0.05, "vertex {i} re-quant error {d}");
        }
        // functional MOPP: decodes to [0..ntris)
        let mopps = mopp::extract_mopp_with_info(&body);
        let dec2 = mopp::decode(&mopps[0].0);
        let (ks, range, _) = dec2.key_summary();
        assert_eq!(ks.len(), tris.len());
        assert_eq!(range, Some((0, tris.len() as u32 - 1)));

        eprintln!(
            "reauthored real building: {} tris, {} verts → PHY2 {} B (packfile+wrapper); classes {:?}",
            tris.len(),
            verts.len(),
            body.len(),
            pf.class_counts
        );
    }
}
