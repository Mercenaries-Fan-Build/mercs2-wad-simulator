//! `mopp_overlay_forge` — item 3 of the MOPP in-game plan: wrap a MOPP-swapped PHY2 collision cell in
//! a `vz-patch.wad` overlay that OVERRIDES exactly that one block, last-wins.
//!
//! Three swap modes drive the 3-rung in-game ladder over the SAME resident spawn-floor cell:
//!   --mode identity    (default) re-serialize the SAME decoded MOPP bytes → block is byte-identical to
//!                       base. Proves the deploy pipeline is sound + non-breaking (rung 1).
//!   --mode return-all   swap the floor MOPP idx 0 to `encode_return_all(N)` (N = source key count):
//!                       universal-collision. Proves OUR compiler's bytecode is game-valid + correct —
//!                       the player stands only if the game ran our MOPP (rung 3, positive attribution).
//!   --mode empty        swap the floor MOPP(s) to the emit-nothing MOPP (`[0x00]`, a lone RETURN → 0
//!                       keys). Removes this cell's collision → the player FALLS THROUGH. Negative
//!                       control (rung 2, positive attribution: our block drives collision). `--all-mopps`
//!                       empties every MOPP in the chosen container (the whole floor shell).
//!
//! Pipeline (all offline-gated; base `vz.wad` is READ-ONLY and stays pristine):
//!   1. decompress the target block; walk its entry table -> model containers.
//!   2. pick the container carrying a PHY2 with an hkpMoppCode (or --model <hash>).
//!   3. extract that PHY2 body; run `swap_phy2_mopp(.., mode)` for each targeted MOPP.
//!   4. splice the swapped body back into the container — RESIZE-AWARE (descriptor size, later bodies'
//!      offsets, CSUM) — and patch the block entry table's chunk_size -> reassemble the block.
//!      GATE: rebuilt block re-walks clean (CSUM/descriptors), and for identity is byte-identical to base.
//!   5. carry EVERY ASET row that points at this block; `PatchBlock::from_decompressed` (sges +
//!      packed_field); `build_patch_wad_multi` (auto-sentinels dangling LOD rungs -> no 549GB wedge).
//!   6. `aset_refcheck` the written WAD separately as the deploy gate.
//!
//! Usage:
//!   mopp_overlay_forge --block 2612 --report
//!   mopp_overlay_forge --block 2612 --mode return-all --out .../vz-patch-mopp-returnall.wad \
//!       --merge-into .../live-vz-patch.wad --merge-out .../...-MERGED.wad
//!   mopp_overlay_forge --block 2612 --mode empty --all-mopps --out .../vz-patch-mopp-empty.wad ...

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::havok::{parse_phy2_body, Shape};
use mercs2_formats::model_cubeize::parse_segm;
use mercs2_formats::mopp;
use mercs2_formats::phy2_build::{build_phy2_kind, build_phy2_multi_kind, MoppKind};
use mercs2_formats::patch_wad::{
    build_patch_wad_multi, merge_patch_wads, AsetEntry, PatchBlock, FFCS_CERT_BLOB,
};
use mercs2_formats::phy2_moppswap::{
    count_phy2_mopps, decode_phy2_mopp_keys, swap_phy2_mopp, validate_swapped_body, SwapMode,
};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{walk_decompressed_block, ParsedBlock};
use sha2::{Digest, Sha256};
use std::path::Path;

fn sha256_hex(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}
fn arg(a: &[String], n: &str) -> Option<String> {
    a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned()
}
fn has(a: &[String], n: &str) -> bool {
    a.iter().any(|x| x == n)
}
fn mode_name(m: SwapMode) -> &'static str {
    match m {
        SwapMode::Identity => "identity",
        SwapMode::ReturnAll => "return-all",
        SwapMode::Empty => "empty",
        SwapMode::Spatial => "spatial",
    }
}

/// Locate the PHY2 chunk inside a UCFX container: returns (body_start, body_size) relative to the
/// container. `body_start` is absolute in the container bytes.
fn phy2_span_in_container(container: &[u8]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().ok()?) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().ok()?) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > container.len() {
            break;
        }
        if &container[r..r + 4] != b"PHY2" {
            continue;
        }
        let row_u0 = u32::from_le_bytes(container[r + 4..r + 8].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(container[r + 8..r + 12].try_into().ok()?) as usize;
        if row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let start = if data_area_off > 0 { data_area_off + row_u0 } else { 8 + row_u0 };
        if start + size > container.len() {
            return None;
        }
        return Some((start, size));
    }
    None
}

/// First MOPP index whose source keys are a clean contiguous [0..N-1] run.
fn first_contiguous_mopp(body: &[u8]) -> usize {
    for i in 0..count_phy2_mopps(body) {
        if let Ok(d) = decode_phy2_mopp_keys(body, i) {
            let (ks, range, missing) = d.key_summary();
            if !ks.is_empty() && missing.is_empty() && range == Some((0, ks.len() as u32 - 1)) {
                return i;
            }
        }
    }
    0
}

/// Replace the PHY2 body at absolute container offset `pstart` (old length `psize`) with `new_body`,
/// fixing the UCFX descriptor table (this PHY2's `body_size`; any body located AFTER it shifts by the
/// size delta), splicing the data region, and recomputing the trailing CSUM. Returns the rebuilt
/// container. `delta == 0` (in-place-size swaps: identity, empty) produces a container with only the
/// changed body bytes + recomputed CSUM — byte-identical when the body itself is unchanged.
fn replace_phy2_in_container(
    container: &[u8],
    pstart: usize,
    psize: usize,
    new_body: &[u8],
) -> Result<Vec<u8>, String> {
    if container.len() < 28 || &container[0..4] != b"UCFX" {
        return Err("container is not a UCFX packet".into());
    }
    if container.len() < 8 || &container[container.len() - 8..container.len() - 4] != b"CSUM" {
        return Err("container has no CSUM trailer".into());
    }
    let csum_start = container.len() - 8;
    if pstart + psize > csum_start {
        return Err("PHY2 body overlaps the CSUM trailer".into());
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().unwrap()) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().unwrap()) as usize;
    let delta = new_body.len() as i64 - psize as i64;
    let phy2_rel = pstart
        .checked_sub(if data_area_off > 0 { data_area_off } else { 8 })
        .ok_or("PHY2 body starts before the data area")?;

    // Patch the descriptor table (rows sit before the data area, i.e. before `pstart`).
    let mut c = container.to_vec();
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > c.len() {
            break;
        }
        let ru0 = u32::from_le_bytes(c[r + 4..r + 8].try_into().unwrap());
        if ru0 == 0xFFFF_FFFF {
            continue;
        }
        let ru0u = ru0 as usize;
        if ru0u == phy2_rel && &c[r..r + 4] == b"PHY2" {
            c[r + 8..r + 12].copy_from_slice(&(new_body.len() as u32).to_le_bytes());
        } else if ru0u > phy2_rel {
            let nv = (ru0 as i64 + delta) as u32;
            c[r + 4..r + 8].copy_from_slice(&nv.to_le_bytes());
        }
    }

    // Splice the data region and recompute the CSUM over everything before the trailer.
    let mut out = Vec::with_capacity((c.len() as i64 + delta).max(0) as usize);
    out.extend_from_slice(&c[..pstart]);
    out.extend_from_slice(new_body);
    out.extend_from_slice(&c[pstart + psize..csum_start]);
    let crc = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

/// Re-author a WHOLE PHY2 body from the target container's OWN decoded collision geometry, and gate it
/// offline. Single-shape → `build_phy2`; multi-shape → `build_phy2_multi`. The original 48-byte PHY2
/// prefix (name-hash + framing fields) is preserved verbatim except byte-32 (the packfile size, which the
/// authored packfile changes) so the engine still resolves this body to the same asset. Returns the
/// authored body (prefix + authored packfile + authored engine-faithful AA/CC/EE descriptor-chain wrapper;
/// multi-shape emits the retail SHARED-pool layout: one common frame + one shared pool, global indices, all
/// N EEs → that pool — see `build_multi_mesh_wrapper` / `build_shared_mesh_set`).
fn author_whole_phy2(
    container: &[u8],
    base_body: &[u8],
    ename: u32,
    mopp_kind: MoppKind,
) -> Result<Vec<u8>, String> {
    // 1. Decode the source meshes (in packfile order) → model-local tris/verts.
    let src = parse_phy2_body(base_body).map_err(|e| format!("decode source PHY2: {e}"))?;
    let meshes: Vec<(Vec<[u32; 3]>, Vec<[f32; 3]>)> = src
        .shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Mesh(m) if !m.indices.is_empty() => Some((
                m.indices.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect::<Vec<_>>(),
                m.vertices.clone(),
            )),
            _ => None,
        })
        .collect();
    if meshes.is_empty() {
        return Err("source PHY2 has no decodable WpMeshShape16".into());
    }
    let n_shapes = meshes.len();
    let segm = parse_segm(container);
    println!(
        "  AUTHORED: {} decodable mesh(es); container SEGM records = {}",
        n_shapes,
        segm.len()
    );
    for (i, (t, v)) in meshes.iter().enumerate() {
        println!("    mesh[{i}]: {} tris, {} verts", t.len(), v.len());
    }
    if !segm.is_empty() && segm.len() != n_shapes {
        return Err(format!(
            "shape count {n_shapes} != SEGM record count {} — authored shapes MUST match SEGM count",
            segm.len()
        ));
    }
    if n_shapes > 1 {
        println!(
            "  MULTI-SHAPE authoring (retail SHARED pool): one common quantization frame + ONE shared vertex \
             pool that every sub-mesh indexes with GLOBAL indices, all N leaf EEs → that one pool (EE+76 → \
             wrapper+1232), MERGED 2N−1 BV-tree ((N−1) internal AA nodes w/ CC union-AABB + two children, N \
             leaf AA nodes → EE). Reproduces the retail floor's frame + record/CC/EE/pool topology; tree is \
             our own proven-edge caterpillar."
        );
    }
    if mopp_kind == MoppKind::ReturnAll {
        println!(
            "  MOPP = RETURN-ALL (diagnostic): each shape's spatial MOPP replaced by encode_return_all(ntris) \
             — SAME m_info + mesh + wrapper, ONLY the m_data bytecode changes. STAND ⇒ spatial frame is the \
             culprit; FALL ⇒ upstream m_info/AABB/mesh/binding."
        );
    }

    // 2. Author the whole PHY2.
    let name = format!("authored_collider_0x{ename:08X}");
    let authored = if n_shapes == 1 {
        build_phy2_kind(&name, &meshes[0].0, &meshes[0].1, mopp_kind)?
    } else {
        build_phy2_multi_kind(&name, &meshes, mopp_kind)?
    };

    // 3. Preserve the original 48-byte prefix (name-hash + framing) except byte-32 = packfile size.
    if base_body.len() < 48 || authored.len() < 48 {
        return Err("PHY2 body shorter than the 48-byte prefix".into());
    }
    let mut out = Vec::with_capacity(authored.len());
    out.extend_from_slice(&base_body[0..48]); // original prefix verbatim
    out[32..36].copy_from_slice(&authored[32..36]); // …but the authored packfile size
    out.extend_from_slice(&authored[48..]); // authored packfile + wrapper

    // 4. Offline gate: re-parse the authored body.
    let pf = parse_phy2_body(&out).map_err(|e| format!("re-parse authored PHY2: {e}"))?;
    for class in ["WpMeshShape16", "hkpMoppBvTreeShape", "hkpMoppCode", "WpArray"] {
        let want = if class == "WpArray" { 1 } else { n_shapes as u32 };
        let got = pf.class_counts.get(class).copied().unwrap_or(0);
        if got != want {
            return Err(format!("authored gate: class {class} count {got} != {want}"));
        }
    }
    // Each mesh decodes back to its OWN triangle indices (deterministic — from the packfile).
    let re_meshes: Vec<&mercs2_formats::havok::MeshShape> = pf
        .shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Mesh(m) if !m.indices.is_empty() => Some(m),
            _ => None,
        })
        .collect();
    if re_meshes.len() != n_shapes {
        return Err(format!("authored gate: {} meshes decoded, want {n_shapes}", re_meshes.len()));
    }
    // For a multi-shape (shared-pool) body the packfile index arrays are GLOBAL indices, so validate GEOMETRY
    // (triangle vertex POSITIONS, order-preserving) rather than raw index equality. Single-shape stays local,
    // so raw index equality still holds there.
    if n_shapes == 1 {
        let want: Vec<[u16; 3]> =
            meshes[0].0.iter().map(|t| [t[0] as u16, t[1] as u16, t[2] as u16]).collect();
        if re_meshes[0].indices != want {
            return Err("authored gate: single-shape mesh indices did not round-trip".into());
        }
    } else {
        for (i, (tris, verts)) in meshes.iter().enumerate() {
            if re_meshes[i].indices.len() != tris.len() {
                return Err(format!("authored gate: mesh[{i}] triangle count changed"));
            }
            for (t, gt) in tris.iter().enumerate() {
                for k in 0..3 {
                    let got = re_meshes[i].vertices[re_meshes[i].indices[t][k] as usize];
                    let want = verts[gt[k] as usize];
                    let d = ((got[0] - want[0]).powi(2) + (got[1] - want[1]).powi(2) + (got[2] - want[2]).powi(2)).sqrt();
                    if d > 0.1 {
                        return Err(format!("authored gate: mesh[{i}] tri {t} corner {k} position err {d} > 0.1"));
                    }
                }
            }
        }
    }
    // For single-shape, verticies must round-trip within quant error (the reader locks the one pool).
    if n_shapes == 1 {
        let re = re_meshes[0];
        let (tris, verts) = &meshes[0];
        let maxidx = *tris.iter().flat_map(|t| t.iter()).max().unwrap() as usize;
        let mut worst = 0.0f32;
        for i in 0..=maxidx {
            let (a, b) = (re.vertices[i], verts[i]);
            let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            worst = worst.max(d);
        }
        if worst > 0.1 {
            return Err(format!("authored gate: single-shape vertex round-trip error {worst} > 0.1"));
        }
        println!("    single-shape vertex round-trip: worst dequant error {worst:.5} m (< 0.1)");

        // ★ ENGINE-FAITHFUL WRAPPER GATE: walk the trailing AA/CC/EE descriptor chain exactly as the
        // retail loader relocates it (`&body[0]+offset`) and prove every pool/descriptor pointer resolves.
        // This is the property the old pool-first wrapper lacked → in-game AV @0x0248C15A.
        let mesh = mercs2_formats::phy2_build::write_wpmesh16(&meshes[0].0, &meshes[0].1)?;
        let packfile_size = u32::from_le_bytes(out[32..36].try_into().unwrap()) as usize;
        let pkend = 48 + packfile_size;
        mercs2_formats::phy2_build::validate_wrapper_chain(&out, pkend, &mesh)
            .map_err(|e| format!("authored gate: faithful wrapper chain FAILED: {e}"))?;
        println!("    faithful wrapper chain: AA→CC→AA→EE→pool all relocate in-bounds (engine-walkable)");
    } else {
        // ★ MERGED BV-TREE ENGINE-FAITHFUL WRAPPER GATE: walk the WHOLE 2N−1 tree from the root (every
        // internal two-child + every leaf EE/pool/tail) exactly as the loader relocates it, and prove each
        // leaf's EE+76 pool pointer addresses ITS OWN pool whose dequantized verts reproduce that mesh's
        // input (deterministic, not the tolerant content scan). This is the property the old N-linked-chains
        // wrapper lacked — the full-tree walk is the gate that was missing.
        // Build the SHARED-POOL layout from the same meshes (the exact structure the builder emits) and walk
        // the merged tree, asserting every leaf's EE+76 points to the ONE shared pool.
        let sms = mercs2_formats::phy2_build::build_shared_mesh_set(&meshes)
            .map_err(|e| format!("authored gate: shared-mesh-set: {e}"))?;
        let per_mesh_ntris: Vec<u32> = sms.subs.iter().map(|s| s.tris_global.len() as u32).collect();
        let packfile_size = u32::from_le_bytes(out[32..36].try_into().unwrap()) as usize;
        let pkend = 48 + packfile_size;
        mercs2_formats::phy2_build::validate_multi_wrapper_chain(&out, pkend, &sms.pool, &per_mesh_ntris)
            .map_err(|e| format!("authored gate: merged BV-tree walk FAILED: {e}"))?;
        // Per-mesh vertex correctness via the ONE shared pool (leaf l's EE at ee_region + l*108). Dequantize
        // each sub-mesh's GLOBAL triangle indices under the common frame and compare to the input positions.
        let ic = if n_shapes <= 1 { 1 } else { n_shapes - 1 };
        let ee_region = (ic + n_shapes) * 68 + ic * 108;
        let shared_pool =
            u32::from_le_bytes(out[pkend + ee_region + 76..pkend + ee_region + 80].try_into().unwrap()) as usize;
        let mut worst = 0.0f32;
        for (i, (tris, verts)) in meshes.iter().enumerate() {
            let ee = pkend + ee_region + i * 108;
            let poolptr = u32::from_le_bytes(out[ee + 76..ee + 76 + 4].try_into().unwrap()) as usize;
            if poolptr != shared_pool {
                return Err(format!("authored gate: mesh[{i}] EE+76 pool {poolptr} != shared pool {shared_pool}"));
            }
            let deqg = |g: usize| -> [f32; 3] {
                let o = shared_pool + g * 6;
                [
                    sms.min[0] + u16::from_le_bytes([out[o], out[o + 1]]) as f32 * sms.scale[0],
                    sms.min[1] + u16::from_le_bytes([out[o + 2], out[o + 3]]) as f32 * sms.scale[1],
                    sms.min[2] + u16::from_le_bytes([out[o + 4], out[o + 5]]) as f32 * sms.scale[2],
                ]
            };
            for (t, gt) in sms.subs[i].tris_global.iter().enumerate() {
                for k in 0..3 {
                    let got = deqg(gt[k] as usize);
                    let want = verts[tris[t][k] as usize];
                    let d = ((got[0] - want[0]).powi(2) + (got[1] - want[1]).powi(2) + (got[2] - want[2]).powi(2)).sqrt();
                    worst = worst.max(d);
                }
            }
        }
        if worst > 0.1 {
            return Err(format!("authored gate: multi-shape per-mesh vertex round-trip error {worst} > 0.1"));
        }
        println!(
            "    merged BV-tree walk (SHARED pool): {n_shapes} leaves reached from root, all EE+76 → the one \
             shared pool @body[{shared_pool}]; per-mesh global-index dequant worst {worst:.5} m (< 0.1)"
        );
        // ★ DISTINCT-SHAPE BINDING GATE: the packfile global-fixup graph must bind N DISTINCT shapes
        // (WpArray → N distinct bvtrees → N distinct moppcodes + N distinct meshes), never collapsed onto
        // shape[0]. This is the invariant the live A/B x32dbg proof flagged and that the mesh/MOPP census
        // (virtual-fixup enumeration) is structurally blind to.
        mercs2_formats::phy2_build::validate_multi_shape_binding(&out, n_shapes)
            .map_err(|e| format!("authored gate: distinct-shape binding FAILED: {e}"))?;
        println!("    distinct-shape binding: WpArray → {n_shapes} distinct bvtrees → {n_shapes} distinct moppcodes + meshes (record[i]→shape[i])");
    }
    // Each MOPP decodes clean to [0..tris_i).
    let re_mopps = mopp::extract_mopp_with_info(&out);
    if re_mopps.len() != n_shapes {
        return Err(format!("authored gate: {} MOPPs, want {n_shapes}", re_mopps.len()));
    }
    for (i, (code, _)) in re_mopps.iter().enumerate() {
        let d = mopp::decode(code);
        if d.error.is_some() || d.consumed != code.len() {
            return Err(format!("authored gate: MOPP[{i}] decode {:?}, coverage {}/{}", d.error, d.consumed, code.len()));
        }
        let (ks, range, missing) = d.key_summary();
        let want_n = meshes[i].0.len();
        if ks.len() != want_n || range != Some((0, want_n as u32 - 1)) || !missing.is_empty() {
            return Err(format!("authored gate: MOPP[{i}] keys {} range {range:?} (want [0..{want_n}))", ks.len()));
        }
    }
    println!(
        "  AUTHORED offline gate PASS: {n_shapes} shape(s), class census OK, indices + MOPP keys round-trip"
    );
    Ok(out)
}

/// One MOPP's key summary for the report / gate lines.
fn mopp_line(body: &[u8], idx: usize) -> String {
    match decode_phy2_mopp_keys(body, idx) {
        Ok(d) => {
            let (ks, range, missing) = d.key_summary();
            let tag = if !ks.is_empty() && missing.is_empty() && range == Some((0, ks.len() as u32 - 1))
            {
                "single-subpart [0..N-1]"
            } else if missing.is_empty() {
                "contiguous (offset base)"
            } else {
                "multi-subpart / gaps"
            };
            format!(
                "{} keys, range {:?}, {} missing, err {:?}  [{tag}]",
                ks.len(),
                range,
                missing.len(),
                d.error
            )
        }
        Err(e) => format!("decode failed: {e}"),
    }
}

fn print_report(parsed: &ParsedBlock, block: u16) {
    println!("=== mopp_overlay_forge REPORT — block {block} MOPP inventory ===");
    let mut any = false;
    for (i, c) in parsed.containers.iter().enumerate() {
        let Some((s, sz)) = phy2_span_in_container(c) else {
            continue;
        };
        let body = &c[s..s + sz];
        let n = count_phy2_mopps(body);
        if n == 0 {
            continue;
        }
        any = true;
        let nh = parsed.entries[i].name_hash;
        println!(
            "container[{i}] name 0x{nh:08X}  container {} B  PHY2 {sz} B @ +0x{s:X}  {n} MOPP(s):",
            c.len()
        );
        for m in 0..n {
            println!("    mopp[{m}]: {}", mopp_line(body, m));
        }
    }
    if !any {
        println!("(no container with a PHY2+MOPP in this block)");
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("mopp_overlay_forge: error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let block: u16 = arg(&args, "--block").and_then(|s| s.parse().ok()).unwrap_or(2612);
    let model_hash: Option<u32> = arg(&args, "--model")
        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());
    let report = has(&args, "--report");
    let all_mopps = has(&args, "--all-mopps");
    // `authored` is a whole-PHY2 re-authoring mode (not a MOPP swap): decode the target container's
    // mesh(es), re-author a fully-authored PHY2 (WpMeshShape16 + engine-faithful descriptor-chain wrapper
    // + native MOPP) from that SAME geometry, and splice it in. Single-shape uses build_phy2 (proven); a
    // multi-shape container uses build_phy2_multi (see the wrapper caveat printed at run time).
    let authored = matches!(arg(&args, "--mode").as_deref(), Some("authored"));
    // On the authored path, `--mopp` selects which MOPP bytecode each shape carries: `spatial` (default,
    // the real collision) or `return-all` (the frame-ignoring diagnostic — SAME mesh/wrapper/m_info, only
    // the bytecode changes; see phy2_build::MoppKind).
    let mopp_kind = match arg(&args, "--mopp").as_deref() {
        None | Some("spatial") => MoppKind::Spatial,
        Some("return-all") | Some("returnall") => MoppKind::ReturnAll,
        Some(other) => return Err(format!("unknown --mopp {other} (spatial|return-all)")),
    };
    let mode = if authored {
        SwapMode::Identity // placeholder; unused on the authored path
    } else {
        match arg(&args, "--mode").as_deref() {
            None | Some("identity") => SwapMode::Identity,
            Some("return-all") | Some("returnall") => SwapMode::ReturnAll,
            Some("empty") => SwapMode::Empty,
            Some(other) => {
                return Err(format!("unknown --mode {other} (identity|return-all|empty|authored)"))
            }
        }
    };
    let default_out = format!(
        "C:/Users/Shadow/AppData/Local/Temp/hunt/mopp/overlay/vz-patch-mopp-{}.wad",
        if authored {
            match mopp_kind {
                MoppKind::ReturnAll => "authored-returnall",
                MoppKind::Spatial => "authored",
            }
        } else {
            match mode {
                SwapMode::Identity => "identity",
                SwapMode::ReturnAll => "returnall",
                SwapMode::Empty => "empty",
                SwapMode::Spatial => "spatial",
            }
        }
    );
    let out = arg(&args, "--out").unwrap_or(default_out);

    let vz = game_paths::vz_wad(Path::new("."))
        .ok_or("vz.wad not found — set MERCS2_GAME_DIR or .mercs2-local.toml")?;
    let mut f = std::fs::File::open(&vz).map_err(|e| format!("open {vz:?}: {e}"))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("ffcs: {e}"))?;

    let path = ar
        .paths
        .get(block as usize)
        .cloned()
        .ok_or_else(|| format!("block {block} has no PTHS path"))?;

    // 1. decompress + walk
    let dec = decompress_block(&mut f, &ar.indx, block).map_err(|e| format!("decompress {block}: {e}"))?;
    let dec_sha = sha256_hex(&dec);
    let (parsed, issues) = walk_decompressed_block(&dec, "target");
    for is in &issues {
        eprintln!("  base walk issue: {} :: {}", is.context, is.detail);
    }

    if report {
        println!("base wad : {}", vz.display());
        println!("target   : block {block}  '{path}'");
        println!("decompressed block: {} B  sha256 {dec_sha}", dec.len());
        print_report(&parsed, block);
        return Ok(());
    }

    let authored_label = match mopp_kind {
        MoppKind::ReturnAll => "authored/return-all",
        MoppKind::Spatial => "authored",
    };
    println!("=== mopp_overlay_forge (mode={}) ===", if authored { authored_label } else { mode_name(mode) });
    println!("base wad : {}", vz.display());
    println!("target   : block {block}  '{path}'");
    println!("decompressed block: {} B  sha256 {dec_sha}", dec.len());

    // 2. pick the container carrying a PHY2+MOPP (optionally constrained to --model hash)
    let mut chosen: Option<usize> = None;
    for (i, c) in parsed.containers.iter().enumerate() {
        if let Some(mh) = model_hash {
            if parsed.entries[i].name_hash != mh {
                continue;
            }
        }
        if let Some((s, sz)) = phy2_span_in_container(c) {
            if count_phy2_mopps(&c[s..s + sz]) > 0 {
                chosen = Some(i);
                break;
            }
        }
    }
    let ci = chosen.ok_or("no container with a PHY2+MOPP found in this block")?;
    let ename = parsed.entries[ci].name_hash;
    let (pstart, psize) = phy2_span_in_container(&parsed.containers[ci]).unwrap();
    let base_body = parsed.containers[ci][pstart..pstart + psize].to_vec();
    let n_mopps = count_phy2_mopps(&base_body);
    println!(
        "container[{ci}] name 0x{ename:08X}: PHY2 body {psize} B @ container+0x{pstart:X}, {n_mopps} MOPP(s)",
        );
    println!("  src PHY2 body sha256 {}", sha256_hex(&base_body));

    let new_body = if authored {
        author_whole_phy2(&parsed.containers[ci], &base_body, ename, mopp_kind)?
    } else {
        // Which MOPP indices does this mode target?
        let default_idx = arg(&args, "--mopp").and_then(|s| s.parse::<usize>().ok());
        let targets: Vec<usize> = match mode {
            SwapMode::Empty if all_mopps => (0..n_mopps).collect(),
            _ => vec![default_idx.unwrap_or_else(|| first_contiguous_mopp(&base_body))],
        };
        println!("  targeting MOPP index(es): {targets:?}");

        // Apply the swap(s) sequentially. Identity & empty are size-preserving (in-place), so successive
        // MOPP indices stay valid across calls; return-all is applied to a single index.
        let mut body = base_body.clone();
        for &idx in &targets {
            if idx >= count_phy2_mopps(&body) {
                return Err(format!("mopp index {idx} out of range ({n_mopps} MOPPs)"));
            }
            let src_keys = decode_phy2_mopp_keys(&body, idx)?.keys;
            let (nb, rep) = swap_phy2_mopp(&body, idx, mode)?;
            let expected: Option<&[u32]> = if mode == SwapMode::Identity { Some(&src_keys) } else { None };
            let g = validate_swapped_body(&nb, idx, mode, expected);
            let gate_ok = g.reparse_ok
                && g.mopp_present
                && g.decode_clean
                && g.decode_coverage_full
                && g.keys_as_expected
                && g.mesh_still_decodes;
            println!(
                "  mopp[{idx}]: {} src keys -> {} decoded key(s), buf {}->{} B, packfile {}->{} B ({}), offline-gate {}",
                rep.source_key_count,
                g.decoded_key_count,
                rep.old_buf_len,
                rep.new_buf_len,
                rep.old_packfile_size,
                rep.new_packfile_size,
                if rep.grew { "GREW/appended" } else { "in-place" },
                if gate_ok { "PASS" } else { "FAIL" }
            );
            if !gate_ok {
                return Err(format!("mopp[{idx}] offline gate FAILED: {g:?}"));
            }
            if mode == SwapMode::Identity && nb != body {
                return Err("identity swap was not byte-identical".into());
            }
            body = nb;
        }
        body
    };
    println!("  new PHY2 body: {} B  sha256 {}", new_body.len(), sha256_hex(&new_body));

    // 4. splice the swapped body back into the container (RESIZE-AWARE) + rebuild the block.
    let new_container = replace_phy2_in_container(&parsed.containers[ci], pstart, psize, &new_body)?;
    let header_end = 4 + parsed.entry_count as usize * 16;
    let mut rebuilt = Vec::with_capacity(dec.len() + new_container.len());
    rebuilt.extend_from_slice(&dec[0..header_end]); // count + entry table
    // patch this container's entry-table chunk_size to the (possibly resized) container length.
    let cs_off = 4 + ci * 16 + 12;
    rebuilt[cs_off..cs_off + 4].copy_from_slice(&(new_container.len() as u32).to_le_bytes());
    for (i, c) in parsed.containers.iter().enumerate() {
        if i == ci {
            rebuilt.extend_from_slice(&new_container);
        } else {
            rebuilt.extend_from_slice(c);
        }
    }
    let rebuilt_sha = sha256_hex(&rebuilt);
    println!("rebuilt block: {} B  sha256 {rebuilt_sha}", rebuilt.len());

    // GATE A: rebuilt block re-walks clean (CSUM + descriptor bounds) — the resize-correctness proof.
    let (_rp, ri) = walk_decompressed_block(&rebuilt, "rebuilt");
    if !ri.is_empty() {
        for is in &ri {
            eprintln!("  rebuilt walk issue: {} :: {}", is.context, is.detail);
        }
        return Err("rebuilt block failed the re-walk (CSUM/descriptor) gate".into());
    }
    println!("  RE-WALK GATE: rebuilt block walks clean (CSUM + descriptors OK)");
    // GATE B (identity only): byte-identical to base.
    if mode == SwapMode::Identity && !authored {
        println!(
            "  IDENTITY GATE — rebuilt == base decompressed: {}",
            if rebuilt == dec { "YES (byte-identical)" } else { "NO !!" }
        );
        if rebuilt != dec {
            return Err("identity rebuilt block is NOT byte-identical to base".into());
        }
    }

    // 5. carry EVERY ASET row that points at this block; build the overlay
    let aset: Vec<AsetEntry> = ar
        .aset
        .iter()
        .filter(|e| e.block_index() == block)
        .map(|e| AsetEntry::new(e.asset_hash, e.secondary_ref, e.packed_block_ref, e.type_id))
        .collect();
    let tier = ar.indx.get(block as usize).map(|i| i.packed_field);
    println!(
        "carrying {} ASET row(s) for block {block}; inherit tier {:?}",
        aset.len(),
        tier.map(|t| t >> 24)
    );
    let pblk = PatchBlock::from_decompressed(&rebuilt, path.clone(), aset, tier)?;
    println!(
        "patch block: {} declared pages, sges {} B",
        pblk.declared_pages(),
        pblk.compressed_data.len()
    );

    if let Some(parent) = Path::new(&out).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let wad = build_patch_wad_multi(std::slice::from_ref(&pblk), 0, None, &FFCS_CERT_BLOB)?;
    std::fs::write(&out, &wad).map_err(|e| format!("write {out}: {e}"))?;
    println!("\nWROTE standalone overlay: {out}");
    println!("  {} B  sha256 {}", wad.len(), sha256_hex(&wad));

    // Optional: also emit a variant that MERGES this block into a live vz-patch.wad (append; the live
    // overlay is preserved, our block added, last-wins for c33294).
    if let Some(live) = arg(&args, "--merge-into") {
        let existing = std::fs::read(&live).map_err(|e| format!("read live {live}: {e}"))?;
        let merged = merge_patch_wads(&existing, vec![pblk], false)?;
        let mout = arg(&args, "--merge-out")
            .unwrap_or_else(|| format!("{}-MERGED.wad", out.trim_end_matches(".wad")));
        std::fs::write(&mout, &merged).map_err(|e| format!("write {mout}: {e}"))?;
        println!("\nWROTE merged overlay (live + our block): {mout}");
        println!("  live in : {} ({} B)", live, existing.len());
        println!("  {} B  sha256 {}", merged.len(), sha256_hex(&merged));
    }

    println!("\nMount as data/vz-patch.wad (overlay, last-wins). Gate next: aset_refcheck <wad>");
    Ok(())
}
