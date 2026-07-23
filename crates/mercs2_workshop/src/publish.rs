//! Mod publishing — package NOVEL (new-hash) assets into a patch WAD, natively.
//!
//! Milestone M3 of `docs/modernization/workshop_publish_pipeline.md`. Each new asset ships as
//! its own single-entry block (`[u32 count=1][16-byte entry][UCFX]`) with an ASET row keyed by
//! `pandemic_hash_m2(name)` — the cube_mod-proven shape; no retail-block surgery. The model
//! container itself is built by `model_inject::inject_into_donor_block` (the CJ donor recipe:
//! a real container the engine already accepts, geometry rebuilt, name re-stamped, CSUM
//! recomputed — never a from-scratch UCFX, that's the sarah-hang).
//!
//! Publishing runs on a worker thread (the frame loop never stalls): resolve each donor across
//! the wad stack (last-wins), inject, compress, assemble via `patch_wad::build_patch_wad_multi`,
//! write, SHA-256 (mandate: bind results to bytes), then SELF-TEST by reopening the written wad
//! and engine-loading every new hash. The report lands on a channel the app drains per frame.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

use mercs2_engine::{mesh, wad};
use mercs2_formats::ffcs::{find_chunk, load_ffcs_archive};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::model_inject::{
    drawing_group_caps, inject_fresh_skeleton, inject_parts_into_donor_block,
    inject_static_into_donor_block, ExternalMesh, InjectPart, MtrlRepoint, SkelPart,
};
use mercs2_formats::texture::{group_material_indices, parse_mtrl};
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::{compress_sges, decompress_block};
use mercs2_formats::skeleton::Skeleton;
use mercs2_formats::ucfx::parse_block_entry_table;

/// The two shared body switch nodes every vehicle carries (see vehicle_model_spec §5).
const NODE_INTACT_BODY: u32 = 0x255E_AB53; // PristineState SHOWs this — static host, hidden on death
const NODE_MAIN_ROTOR: u32 = 0xA998_B636; // bone_rotor: the engine-spun main-rotor node (Mi-26)

const MODEL_TYPE_HASH: u32 = 0x5B72_4250; // pandemic_hash_m2("model")
const MODEL_ASET_TYPE_ID: u32 = 19;

/// One novel model queued for publishing.
#[derive(Clone)]
pub struct NewModelItem {
    /// The new asset's name (registry-style); the shipped hash is `pandemic_hash_m2(name)`.
    pub name: String,
    pub hash: u32,
    /// Donor model asset hash (its container hosts the injected geometry).
    pub donor: u32,
    pub donor_label: String,
    /// RAW donor group ordinal that hosts the mesh (others are neutralised). This is the
    /// engine's actually-rendered group (see `inject_static`'s raw-group targeting), not a
    /// loose "has geometry" index.
    pub target_group: usize,
    /// Reverse triangle winding on inject (RH→LH) — set when imported faces cull inside-out.
    pub flip: bool,
    pub mesh: ExternalMesh,
    /// The imported model's own textures (straight RGBA8) for the hosted group's material slots:
    /// 0 = diffuse (`_dm`), 1 = specular (`_sm`), 2 = normal (`_nm`, DXT5nm). `None` keeps the
    /// donor's texture for that slot. These repoint the target group's donor MTRL record so the
    /// injected geometry wears ITS OWN skin, not the donor's.
    pub diffuse: Option<(u32, u32, Vec<u8>)>,
    pub specular: Option<(u32, u32, Vec<u8>)>,
    pub normal: Option<(u32, u32, Vec<u8>)>,
}

/// Outcome of one publish run.
pub struct PublishReport {
    pub path: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    /// Per item: (name, self-test outcome — Ok("v/t counts") or the load error).
    pub results: Vec<(String, Result<String, String>)>,
}

/// Handle to an in-flight publish (poll `rx` once per frame).
pub struct Publisher {
    pub rx: Receiver<Result<PublishReport, String>>,
}

/// Kick a publish off on a worker thread. `wad_paths` is the live stack order
/// (`[base, overlays…]`) — donors resolve last-wins, exactly like the browser.
pub fn publish_in_background(
    wad_paths: Vec<String>,
    items: Vec<NewModelItem>,
    output: PathBuf,
) -> Publisher {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(publish(&wad_paths, &items, &output));
    });
    Publisher { rx }
}

/// Find a model container by name-hash in a decompressed block: (start, end, field_c).
fn find_model(dec: &[u8], want: u32) -> Option<(usize, usize, u32)> {
    let (count, entries) = parse_block_entry_table(dec);
    let mut offset = 4 + count as usize * 16;
    for e in &entries {
        let end = offset + e.chunk_size as usize;
        if end > dec.len() {
            break;
        }
        if e.type_hash == MODEL_TYPE_HASH && e.name_hash == want {
            return Some((offset, end, e.field_c));
        }
        offset = end;
    }
    None
}

/// Resolve a donor model container across the stack (reverse order, last-wins), sourcing from
/// the block its ASET entry points to — the same container the engine instantiates.
/// Returns the donor wrapped as a SINGLE-ENTRY block (what `inject_into_donor_block` takes).
pub fn donor_block(wad_paths: &[String], donor: u32) -> Result<Vec<u8>, String> {
    let mut last = format!("donor 0x{donor:08X}: not in any wad of the stack");
    for path in wad_paths.iter().rev() {
        let mut file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                last = format!("open {path}: {e}");
                continue;
            }
        };
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let archive = match load_ffcs_archive(&mut file, size) {
            Ok(a) => a,
            Err(e) => {
                last = format!("FFCS {path}: {e}");
                continue;
            }
        };
        let Some(entry) = archive
            .aset
            .iter()
            .find(|e| e.asset_hash == donor && e.type_id == MODEL_ASET_TYPE_ID)
        else {
            continue;
        };
        let block_index = entry.block_index() as u16;
        let dec = match decompress_block(&mut file, &archive.indx, block_index) {
            Ok(d) => d,
            Err(e) => {
                last = format!("decompress block {block_index} of {path}: {e}");
                continue;
            }
        };
        let Some((start, end, field_c)) = find_model(&dec, donor) else {
            last = format!("donor 0x{donor:08X}: ASET points at block {block_index} of {path} but no model container there");
            continue;
        };
        let container = &dec[start..end];
        let mut block = Vec::with_capacity(20 + container.len());
        block.extend_from_slice(&1u32.to_le_bytes());
        block.extend_from_slice(&donor.to_le_bytes());
        block.extend_from_slice(&MODEL_TYPE_HASH.to_le_bytes());
        block.extend_from_slice(&field_c.to_le_bytes());
        block.extend_from_slice(&(container.len() as u32).to_le_bytes());
        block.extend_from_slice(container);
        return Ok(block);
    }
    Err(last)
}

/// The whole publish, blocking (runs on the worker).
fn publish(
    wad_paths: &[String],
    items: &[NewModelItem],
    output: &PathBuf,
) -> Result<PublishReport, String> {
    if items.is_empty() {
        return Err("mod project is empty".into());
    }
    if wad_paths.is_empty() {
        return Err("no wads open".into());
    }
    // Never write over a wad we read from (also: the DLC-port vz-patch.wad is sacrosanct).
    let out_str = output.to_string_lossy().to_lowercase();
    if wad_paths.iter().any(|p| p.to_lowercase() == out_str) {
        return Err(format!(
            "output {} is an open source wad — pick a different file",
            output.display()
        ));
    }

    // ── Per item: donor → inject → single-entry block → compress → PatchBlock + ASET row. ──
    const TEXTURE_ASET_TYPE_ID: u32 = 27;
    let mut blocks: Vec<PatchBlock> = Vec::new();
    for item in items {
        let donor = donor_block(wad_paths, item.donor)?;
        // ── Textures: repoint the hosted group's donor MTRL slots at the model's OWN skin. ──
        // Slot order (verified): 0 = diffuse, 1 = specular, 2 = normal (DXT5nm). We look up the
        // target group's material record, encode each supplied map to its own type-27 texture
        // block, and repoint the donor slot hash → our hash (value-scan over MTRL).
        let mut repoints: Vec<MtrlRepoint> = Vec::new();
        if item.diffuse.is_some() || item.specular.is_some() || item.normal.is_some() {
            let ucfx_len = u32::from_le_bytes([donor[16], donor[17], donor[18], donor[19]]) as usize;
            if 20 + ucfx_len <= donor.len() {
                let ucfx = &donor[20..20 + ucfx_len];
                let mats = parse_mtrl(ucfx);
                let gmi = group_material_indices(ucfx);
                let mat_idx = gmi.get(item.target_group).copied().unwrap_or(0);
                if let Some(mat) = mats.get(mat_idx) {
                    let slots: [(usize, &str, &Option<(u32, u32, Vec<u8>)>, bool); 3] = [
                        (0, "dm", &item.diffuse, false),
                        (1, "sm", &item.specular, false),
                        (2, "nm", &item.normal, true),
                    ];
                    for (slot, suffix, img, is_normal) in slots {
                        let (Some((w, h, rgba)), Some(&from)) = (img, mat.textures.get(slot)) else {
                            continue;
                        };
                        let to = pandemic_hash_m2(&format!("{}_{suffix}", item.name));
                        let td = if is_normal {
                            crate::texenc::encode_normal_full_chain(*w, *h, rgba)
                        } else {
                            crate::texenc::encode_rgba_full_chain(*w, *h, rgba)
                        };
                        let tblock = mercs2_formats::texture::build_texture_block(to, &td);
                        let comp = compress_sges(&tblock).map_err(|e| format!("{}: tex sges: {e}", item.name))?;
                        let aset = vec![AsetEntry::new(to, 0xFFFF_FFFF, 0x0000_FFFF, TEXTURE_ASET_TYPE_ID)];
                        let mut tpb = PatchBlock::new(comp, format!("blocks\\VZ\\mod_{to:08x}.block"), aset);
                        tpb.packed_field = ((tblock.len() + 0x7FFF) / 0x8000) as u32;
                        blocks.push(tpb);
                        repoints.push(MtrlRepoint { from, to });
                        eprintln!(
                            "[publish] tex {}_{suffix} (0x{to:08X}) {w}x{h} {} mips -> group {} mat {mat_idx} slot {slot} (was 0x{from:08X})",
                            item.name, td.mip_count, item.target_group
                        );
                    }
                }
            }
        }
        // The improved conform path: mesh already carries the user's baked transform, so auto-fit
        // is OFF; target the RAW rendered group; winding per the item's flip flag; neutralise the
        // rest. (`inject_static_into_donor_block` — same as the `inject_static` CLI.)
        let (new_block, stats) = inject_static_into_donor_block(
            &donor,
            &item.mesh,
            0,
            &repoints,
            item.hash,
            false, // fit_to_template: OFF (the panel already positioned it)
            item.flip,
            false, // keep_groups
            false, // all_groups
            &[item.target_group],
            1.0,
            false, // neutralize_only
        )
        .map_err(|e| format!("{}: inject into donor {}: {e}", item.name, item.donor_label))?;
        let compressed =
            compress_sges(&new_block).map_err(|e| format!("{}: sges: {e}", item.name))?;
        let aset = vec![AsetEntry::new(item.hash, 0xFFFF_FFFF, 0x0000_FFFF, MODEL_ASET_TYPE_ID)];
        let mut pb = PatchBlock::new(
            compressed,
            format!("blocks\\VZ\\mod_{:08x}.block", item.hash),
            aset,
        );
        pb.packed_field = ((new_block.len() + 0x7FFF) / 0x8000) as u32;
        eprintln!(
            "[publish] {} (0x{:08X}) <- donor {} group {}: {} verts, {} tris",
            item.name, item.hash, item.donor_label, item.target_group,
            stats.vertex_count, stats.triangle_count
        );
        blocks.push(pb);
    }

    // ── Assemble the patch WAD (CSUM value/meta mirrored from the base, like cube_mod). ──
    let mut base = std::fs::File::open(&wad_paths[0])
        .map_err(|e| format!("open {}: {e}", wad_paths[0]))?;
    let base_size = base.metadata().map(|m| m.len()).unwrap_or(0);
    let base_archive =
        load_ffcs_archive(&mut base, base_size).map_err(|e| format!("base FFCS: {e}"))?;
    let csum_value = find_chunk(&base_archive.chunks, b"CSUM").map(|r| r.offset).unwrap_or(0);
    let csum_meta = find_chunk(&base_archive.chunks, b"CSUM").map(|r| r.meta);

    let wad_bytes = build_patch_wad_multi(&blocks, csum_value, csum_meta, &FFCS_CERT_BLOB)?;
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
    }
    std::fs::write(output, &wad_bytes).map_err(|e| format!("write {}: {e}", output.display()))?;
    let sha = sha256_hex(&wad_bytes);

    // ── Self-test: reopen the WRITTEN wad and engine-load every new hash from it. ──
    let mut results = Vec::new();
    match wad::open(&output.to_string_lossy()) {
        Ok(mut w) => {
            for item in items {
                let r = wad::extract_container(&mut w, item.hash)
                    .and_then(|c| mesh::build_indexed_from_container(&c))
                    .map(|(verts, indices, draws, _)| {
                        format!("{} verts / {} tris / {} groups", verts.len(), indices.len() / 3, draws.len())
                    });
                results.push((item.name.clone(), r));
            }
        }
        Err(e) => {
            for item in items {
                results.push((item.name.clone(), Err(format!("reopen wad: {e}"))));
            }
        }
    }

    Ok(PublishReport { path: output.clone(), bytes: wad_bytes.len(), sha256: sha, results })
}

/// One logical part of a novel model imported for a FRESH-SKELETON inject: world-space geometry +
/// which donor articulation it rides (rotor vs static body) + its donor material slot.
pub struct SkelRawPart {
    pub label: String,
    pub mesh: ExternalMesh,
    /// Ride the donor's engine-spun main-rotor node (true) or the static intact-body node (false).
    pub is_rotor: bool,
    pub material_index: u32,
}

/// Publish a NOVEL model that KEEPS THE DONOR'S REAL SKELETON (HIER/SEGM/PHY2) but wears the
/// imported geometry AND its OWN per-material textures — the conformant, spawn-safe path for
/// vehicles. Each imported material is routed to its own donor drawing group (so glass/lights/body
/// keep distinct skins), and that group's donor MTRL slots (0=diffuse,1=specular,2=normal-DXT5nm)
/// are repointed at the imported material's encoded textures. Whole-model replace (non-host groups
/// neutralised). Mints under `m2(name)`; self-tests by reload.
pub fn publish_conformant(
    wad_paths: &[String],
    donor_hash: u32,
    donor_label: &str,
    name: &str,
    parts: Vec<SkelRawPart>,
    mat_images: Vec<Option<(u32, u32, Vec<u8>)>>,
    spec_images: Vec<Option<(u32, u32, Vec<u8>)>>,
    normal_images: Vec<Option<(u32, u32, Vec<u8>)>>,
    output: &PathBuf,
) -> Result<PublishReport, String> {
    if parts.is_empty() {
        return Err("no parts to inject".into());
    }
    let out_str = output.to_string_lossy().to_lowercase();
    if wad_paths.iter().any(|p| p.to_lowercase() == out_str) {
        return Err(format!("output {} is an open source wad — pick another file", output.display()));
    }
    let donor = donor_block(wad_paths, donor_hash)?;
    let hash = pandemic_hash_m2(name);

    // donor drawing groups + their materials (same indexing)
    let ucfx_len = u32::from_le_bytes([donor[16], donor[17], donor[18], donor[19]]) as usize;
    let ucfx = &donor[20..20 + ucfx_len];
    let gmi = group_material_indices(ucfx);
    let donor_mats = parse_mtrl(ucfx);
    let caps = drawing_group_caps(&donor); // (ordinal, vertex_cap, tri_cap)
    if caps.is_empty() {
        return Err("donor has no drawing groups".into());
    }

    // merge our parts by material index -> one mesh per imported material
    use std::collections::BTreeMap;
    let mut by_mat: BTreeMap<u32, ExternalMesh> = BTreeMap::new();
    for p in &parts {
        let m = by_mat.entry(p.material_index).or_insert_with(|| ExternalMesh {
            positions: Vec::new(), normals: Vec::new(), uvs: Vec::new(),
            tris: Vec::new(), joints: Vec::new(), weights: Vec::new(),
        });
        let base = m.positions.len() as u32;
        m.positions.extend_from_slice(&p.mesh.positions);
        m.normals.extend_from_slice(&p.mesh.normals);
        m.uvs.extend_from_slice(&p.mesh.uvs);
        for t in &p.mesh.tris {
            m.tris.push([t[0] + base, t[1] + base, t[2] + base]);
        }
    }
    // ONE donor drawing group per imported material (GROW mode lets the group hold our higher-poly
    // geometry). Each material claims a group with a DISTINCT donor material so the global MTRL
    // repoints never collide, and that group's slots are repointed at the material's own textures.
    let our_mats: Vec<u32> = by_mat.keys().copied().collect();
    // Only LOD0 groups (SEGM state_mask == 0 or & 0x01) are drawn at the default view state — the
    // Host on the donor's TEXTURED groups (material flags & 0x0080 sample a texture; flags 0x0000 is
    // a flat-shade variant that ignores bound textures). Those textured groups often sit on a
    // non-default LOD tier, so we SEGM-promote each host to state 0x01 (below) to make it draw+read.
    // group_index -> SEGM index, for promotion.
    let seg_of_group: std::collections::HashMap<usize, usize> =
        mercs2_formats::model_cubeize::read_model_meshes_segm(ucfx, None)
            .map(|ms| ms.iter().map(|m| (m.group_index, m.seg_id)).collect())
            .unwrap_or_default();
    // candidate host groups: drawing groups whose donor material is textured, ordered biggest-cap first
    let mut cand: Vec<(usize, usize, u32)> = Vec::new(); // (group, donor_mat_idx, diffuse_hash)
    let mut caps_sorted: Vec<(usize, u32, u32)> = caps.clone();
    caps_sorted.sort_by_key(|&(_g, _v, t)| std::cmp::Reverse(t));
    for &(g, _v, _t) in &caps_sorted {
        let dm = gmi.get(g).copied().unwrap_or(usize::MAX);
        if let Some(mat) = donor_mats.get(dm) {
            if mat.flags & 0x0080 != 0 {
                let dif = mat.textures.first().copied().unwrap_or(0);
                cand.push((g, dm, dif));
            }
        }
    }
    let drawing: Vec<usize> = cand.iter().map(|&(g, _, _)| g).collect();
    eprintln!("[conformant] {} textured host groups available: {drawing:?}", drawing.len());
    let mut promote_segm: Vec<usize> = Vec::new();
    const TEXTURE_ASET_TYPE_ID: u32 = 27;
    let mut tex_blocks: Vec<PatchBlock> = Vec::new();
    let mut meshes: Vec<ExternalMesh> = Vec::new();
    let mut hosts: Vec<Vec<usize>> = Vec::new();
    let mut repoints: Vec<Vec<MtrlRepoint>> = Vec::new();
    let mut cand_iter = cand.iter();
    // dedup by donor DIFFUSE HASH — the MTRL repoint is a global value-scan, so two hosts sharing a
    // texture hash would collide (the CRX's flat materials all share 0xFCAE37AB).
    let mut used_hash: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut assigned_count = 0usize;
    for om in &our_mats {
        let mesh = by_mat.remove(om).unwrap();
        // next textured group whose donor diffuse hash is not yet claimed
        let picked = loop {
            match cand_iter.next() {
                Some(&(g, dm, dif)) => {
                    if used_hash.insert(dif) {
                        break Some((g, dm));
                    }
                }
                None => break None,
            }
        };
        let Some((host, dm_idx)) = picked else {
            // Out of distinct donor materials (the donor's LOD0 tier has fewer materials than our
            // model). Rather than drop this material's geometry, merge it into the first host so it
            // still draws (sharing that group's skin). Full per-material fidelity would need
            // promoting more donor groups to LOD0 (SEGM state-mask) — a further step.
            if let Some(first) = meshes.first_mut() {
                let base = first.positions.len() as u32;
                first.positions.extend_from_slice(&mesh.positions);
                first.normals.extend_from_slice(&mesh.normals);
                first.uvs.extend_from_slice(&mesh.uvs);
                for t in &mesh.tris {
                    first.tris.push([t[0] + base, t[1] + base, t[2] + base]);
                }
                eprintln!("[conformant] material {om} merged into host {} (shared skin)", hosts[0][0]);
            }
            continue;
        };
        // promote this host group to the default view-state so it draws + reads
        if let Some(&si) = seg_of_group.get(&host) {
            promote_segm.push(si);
        }
        let mut rp: Vec<MtrlRepoint> = Vec::new();
        let slots: [(usize, &str, &Vec<Option<(u32, u32, Vec<u8>)>>, bool); 3] = [
            (0, "dm", &mat_images, false),
            (1, "sm", &spec_images, false),
            (2, "nm", &normal_images, true),
        ];
        for (slot, suffix, imgs, is_normal) in slots {
            let (Some(Some((w, h, rgba))), Some(&from)) =
                (imgs.get(*om as usize), donor_mats.get(dm_idx).and_then(|m| m.textures.get(slot)))
            else {
                continue;
            };
            let to = pandemic_hash_m2(&format!("{name}_{suffix}_{om}"));
            let td = if is_normal {
                crate::texenc::encode_normal_full_chain(*w, *h, rgba)
            } else {
                crate::texenc::encode_rgba_full_chain(*w, *h, rgba)
            };
            let tblock = mercs2_formats::texture::build_texture_block(to, &td);
            let comp = compress_sges(&tblock).map_err(|e| format!("{name}: tex sges: {e}"))?;
            let aset = vec![AsetEntry::new(to, 0xFFFF_FFFF, 0x0000_FFFF, TEXTURE_ASET_TYPE_ID)];
            let mut tpb = PatchBlock::new(comp, format!("blocks\\VZ\\mod_{to:08x}.block"), aset);
            tpb.packed_field = ((tblock.len() + 0x7FFF) / 0x8000) as u32;
            tex_blocks.push(tpb);
            rp.push(MtrlRepoint { from, to });
            eprintln!(
                "[conformant] mat {om} ({} tris) -> group {host} donor-mat {dm_idx} slot {slot} -> {name}_{suffix}_{om} (0x{to:08X})",
                mesh.tris.len()
            );
        }
        meshes.push(mesh);
        hosts.push(vec![host]);
        repoints.push(rp);
        assigned_count += 1;
    }
    let assign_len = assigned_count;
    let inject_parts: Vec<InjectPart> = (0..meshes.len())
        .map(|i| InjectPart { mesh: &meshes[i], hosts: &hosts[i], repoints: &repoints[i] })
        .collect();

    eprintln!("[conformant] promoting {} SEGM records to LOD0: {promote_segm:?}", promote_segm.len());
    let (new_block, audits, stats) =
        inject_parts_into_donor_block(&donor, &inject_parts, hash, false, true, &promote_segm) // grow
            .map_err(|e| format!("{name}: conform onto {donor_label}: {e}"))?;
    for a in &audits {
        eprintln!(
            "[conformant] audit group {}: wrote {} verts / {} tris (donor cap {} verts)",
            a.group, a.injected_vc, a.triangles, a.donor_vc
        );
    }
    eprintln!(
        "[conformant] {name} (0x{hash:08X}) <- donor {donor_label}: {} verts, {} tris, {} materials placed",
        stats.vertex_count, stats.triangle_count, assign_len
    );

    let compressed = compress_sges(&new_block).map_err(|e| format!("{name}: sges: {e}"))?;
    let aset = vec![AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, MODEL_ASET_TYPE_ID)];
    let mut model_pb =
        PatchBlock::new(compressed, format!("blocks\\VZ\\mod_{hash:08x}.block"), aset);
    model_pb.packed_field = ((new_block.len() + 0x7FFF) / 0x8000) as u32;
    let mut blocks = vec![model_pb];
    blocks.extend(tex_blocks);

    // assemble + self-test (reuse the multi-block packer)
    let mut base = std::fs::File::open(&wad_paths[0]).map_err(|e| format!("open base: {e}"))?;
    let base_size = base.metadata().map(|m| m.len()).unwrap_or(0);
    let base_archive = load_ffcs_archive(&mut base, base_size).map_err(|e| format!("base FFCS: {e}"))?;
    let csum_value = find_chunk(&base_archive.chunks, b"CSUM").map(|r| r.offset).unwrap_or(0);
    let csum_meta = find_chunk(&base_archive.chunks, b"CSUM").map(|r| r.meta);
    let wad_bytes = build_patch_wad_multi(&blocks, csum_value, csum_meta, &FFCS_CERT_BLOB)?;
    std::fs::write(output, &wad_bytes).map_err(|e| format!("write {}: {e}", output.display()))?;
    let sha = sha256_hex(&wad_bytes);
    let mut results = Vec::new();
    match wad::open(&output.to_string_lossy()) {
        Ok(mut w) => {
            let r = wad::extract_container(&mut w, hash)
                .and_then(|c| mesh::build_indexed_from_container(&c))
                .map(|(verts, indices, draws, _)| {
                    format!("{} verts / {} tris / {} groups", verts.len(), indices.len() / 3, draws.len())
                });
            results.push((name.to_string(), r));
        }
        Err(e) => results.push((name.to_string(), Err(format!("reopen wad: {e}")))),
    }
    Ok(PublishReport { path: output.clone(), bytes: wad_bytes.len(), sha256: sha, results })
}

/// Publish a NOVEL model onto a FRESH SKELETON of novel bones (does NOT overwrite the donor).
/// Resolves the donor's real body / rotor node indices, authors one novel bone per part parented
/// under the right articulation, conforms via `inject_fresh_skeleton`, mints under `m2(name)`, and
/// self-tests by reopening the written WAD.
pub fn publish_skel(
    wad_paths: &[String],
    donor_hash: u32,
    donor_label: &str,
    name: &str,
    raw_parts: Vec<SkelRawPart>,
    // Per-glTF-material textures (straight RGBA8), indexed by the part's `material_index`. `None`
    // keeps the donor's texture for that slot. MTRL slots: 0 = diffuse, 1 = specular, 2 = normal
    // (empirically-confirmed authored order — the normal slot expects DXT5nm).
    mat_images: Vec<Option<(u32, u32, Vec<u8>)>>,    // slot 0 diffuse
    spec_images: Vec<Option<(u32, u32, Vec<u8>)>>,   // slot 1 specular
    normal_images: Vec<Option<(u32, u32, Vec<u8>)>>, // slot 2 normal (DXT5nm)
    output: &PathBuf,
) -> Result<PublishReport, String> {
    if raw_parts.is_empty() {
        return Err("no parts to inject".into());
    }
    let out_str = output.to_string_lossy().to_lowercase();
    if wad_paths.iter().any(|p| p.to_lowercase() == out_str) {
        return Err(format!("output {} is an open source wad — pick another file", output.display()));
    }
    let donor = donor_block(wad_paths, donor_hash)?;
    let skel = Skeleton::from_block(&donor).map_err(|e| format!("donor skeleton: {e}"))?;
    let body_node = skel
        .by_hash(NODE_INTACT_BODY)
        .ok_or("donor has no intact-body node 0x255EAB53")?;
    let rotor_node = skel.by_hash(NODE_MAIN_ROTOR).unwrap_or(body_node);

    let hash = pandemic_hash_m2(name);

    // ONE novel bone per imported part (`inject_fresh_skeleton` grows the donor's draw-group pool as
    // needed so each fits). A part flagged `is_rotor` parents under the engine-spun main-rotor node
    // (so it spins); everything else parents under the static intact-body node. The FIRST rotor part
    // keeps the exact `bone_rotor` name so the donor's spin command still resolves; the rest get
    // fresh unique hashes (they inherit the spin by being in that node's subtree).
    let mut rotor_seen = false;
    let parts: Vec<SkelPart> = raw_parts
        .iter()
        .enumerate()
        .map(|(i, rp)| {
            let slug: String = rp
                .label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
                .collect();
            let (bone_name_hash, parent_node) = if rp.is_rotor {
                let h = if !rotor_seen {
                    rotor_seen = true;
                    pandemic_hash_m2("bone_rotor")
                } else {
                    pandemic_hash_m2(&format!("bone_{name}_{slug}_{i}"))
                };
                (h, rotor_node)
            } else {
                (pandemic_hash_m2(&format!("bone_{name}_{slug}_{i}")), body_node)
            };
            SkelPart {
                label: format!("{}_{i}", if rp.label.is_empty() { "part" } else { &rp.label }),
                mesh: ExternalMesh {
                    positions: rp.mesh.positions.clone(),
                    normals: rp.mesh.normals.clone(),
                    uvs: rp.mesh.uvs.clone(),
                    tris: rp.mesh.tris.clone(),
                    joints: Vec::new(),
                    weights: Vec::new(),
                },
                bone_name_hash,
                parent_node,
                material_index: rp.material_index,
            }
        })
        .collect();

    // ---- Textures: encode each glTF material's diffuse to its own texture asset, and point the
    // matching donor MTRL record (the part's material_index) slot 0 at it. Records with no image
    // keep the donor skin. Each texture ships as its own type-27 block in the same patch WAD. ----
    const TEXTURE_ASET_TYPE_ID: u32 = 27;
    let mut tex_blocks: Vec<PatchBlock> = Vec::new();
    let mut mtrl_sets: Vec<(usize, usize, u32)> = Vec::new();
    // (slot, suffix, image-array, is-normal) — slot 0 diffuse, 1 specular, 2 normal (DXT5nm).
    let slots: [(usize, &str, &Vec<Option<(u32, u32, Vec<u8>)>>, bool); 3] = [
        (0, "dm", &mat_images, false),
        (1, "sm", &spec_images, false),
        (2, "nm", &normal_images, true),
    ];
    for (slot, suffix, imgs, is_normal) in slots {
        for (mat_idx, img) in imgs.iter().enumerate() {
            let Some((w, h, rgba)) = img else { continue };
            // Only encode materials that are actually used by a part.
            if !raw_parts.iter().any(|rp| rp.material_index as usize == mat_idx) {
                continue;
            }
            let tex_hash = pandemic_hash_m2(&format!("{name}_{suffix}_{mat_idx}"));
            let td = if is_normal {
                crate::texenc::encode_normal_full_chain(*w, *h, rgba)
            } else {
                crate::texenc::encode_rgba_full_chain(*w, *h, rgba)
            };
            let block = mercs2_formats::texture::build_texture_block(tex_hash, &td);
            let compressed = compress_sges(&block).map_err(|e| format!("{name}: tex sges: {e}"))?;
            let aset =
                vec![AsetEntry::new(tex_hash, 0xFFFF_FFFF, 0x0000_FFFF, TEXTURE_ASET_TYPE_ID)];
            let mut pb =
                PatchBlock::new(compressed, format!("blocks\\VZ\\mod_{tex_hash:08x}.block"), aset);
            pb.packed_field = ((block.len() + 0x7FFF) / 0x8000) as u32;
            tex_blocks.push(pb);
            mtrl_sets.push((mat_idx, slot, tex_hash));
            eprintln!(
                "[publish-skel] tex {name}_{suffix}_{mat_idx} (0x{tex_hash:08X}) {w}x{h} {} mips -> MTRL {mat_idx} slot {slot}",
                td.mip_count
            );
        }
    }

    let (new_block, report) =
        inject_fresh_skeleton(&donor, &parts, &mtrl_sets, hash, 1.0, 0.0, false, 100.0)
            .map_err(|e| format!("{name}: inject onto {donor_label}: {e}"))?;
    eprintln!("[publish-skel] {name} (0x{hash:08X}) <- donor {donor_label}\n{report}");

    let compressed = compress_sges(&new_block).map_err(|e| format!("{name}: sges: {e}"))?;
    let aset = vec![AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, MODEL_ASET_TYPE_ID)];
    let mut pb =
        PatchBlock::new(compressed, format!("blocks\\VZ\\mod_{hash:08x}.block"), aset);
    pb.packed_field = ((new_block.len() + 0x7FFF) / 0x8000) as u32;

    let mut base =
        std::fs::File::open(&wad_paths[0]).map_err(|e| format!("open {}: {e}", wad_paths[0]))?;
    let base_size = base.metadata().map(|m| m.len()).unwrap_or(0);
    let base_archive =
        load_ffcs_archive(&mut base, base_size).map_err(|e| format!("base FFCS: {e}"))?;
    let csum_value = find_chunk(&base_archive.chunks, b"CSUM").map(|r| r.offset).unwrap_or(0);
    let csum_meta = find_chunk(&base_archive.chunks, b"CSUM").map(|r| r.meta);
    let mut all_blocks = vec![pb];
    all_blocks.extend(tex_blocks);
    let wad_bytes = build_patch_wad_multi(&all_blocks, csum_value, csum_meta, &FFCS_CERT_BLOB)?;
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
    }
    std::fs::write(output, &wad_bytes).map_err(|e| format!("write {}: {e}", output.display()))?;
    let sha = sha256_hex(&wad_bytes);

    let mut results = Vec::new();
    match wad::open(&output.to_string_lossy()) {
        Ok(mut w) => {
            let r = wad::extract_container(&mut w, hash)
                .and_then(|c| mesh::build_indexed_from_container(&c))
                .map(|(verts, indices, draws, _)| {
                    format!("{} verts / {} tris / {} groups", verts.len(), indices.len() / 3, draws.len())
                });
            results.push((name.to_string(), r));
        }
        Err(e) => results.push((name.to_string(), Err(format!("reopen wad: {e}")))),
    }
    Ok(PublishReport { path: output.clone(), bytes: wad_bytes.len(), sha256: sha, results })
}

// ── Minimal dependency-free SHA-256 (FIPS 180-4) — same implementation as loadprobe's
// (bin-only crate, can't be depended on); NIST vectors in the tests below. ──

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Lowercase hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let bitlen = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in msg.chunks_exact(64) {
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for x in h {
        out.push_str(&format!("{:08x}", x));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn nist_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}
