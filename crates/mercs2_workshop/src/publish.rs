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
use mercs2_formats::model_inject::{inject_static_into_donor_block, ExternalMesh};
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::{compress_sges, decompress_block};
use mercs2_formats::ucfx::parse_block_entry_table;

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
fn donor_block(wad_paths: &[String], donor: u32) -> Result<Vec<u8>, String> {
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
    let mut blocks: Vec<PatchBlock> = Vec::new();
    for item in items {
        let donor = donor_block(wad_paths, item.donor)?;
        // The improved conform path: mesh already carries the user's baked transform, so auto-fit
        // is OFF; target the RAW rendered group; winding per the item's flip flag; neutralise the
        // rest. (`inject_static_into_donor_block` — same as the `inject_static` CLI.)
        let (new_block, stats) = inject_static_into_donor_block(
            &donor,
            &item.mesh,
            0,
            &[],
            item.hash,
            false, // fit_to_template: OFF (the panel already positioned it)
            item.flip,
            false, // keep_groups
            false, // all_groups
            &[item.target_group],
            1.0,
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

    let wad_bytes = build_patch_wad_multi(&blocks, csum_value, csum_meta, &FFCS_CERT_BLOB);
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
