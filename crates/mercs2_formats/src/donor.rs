//! Donor resolution and patch-block framing — the pieces every publisher needs.
//!
//! Extracted from `mercs2_workshop::publish`, which is a **binary-only** crate: the logic below had
//! actually shipped working WADs but could not be called from anywhere else, so a second consumer
//! (`mercs2_quartermaster`) had the choice of depending on a GUI binary or forking the one proven
//! path. Neither is acceptable, hence this module.
//!
//! What stayed behind in the workshop: the worker thread, progress reporting, and the post-write
//! self-test (which re-opens the WAD through `mercs2_engine` — winit + wgpu, far too heavy for a
//! headless crate).
//!
//! **No encoder choice is baked in here.** Callers pass already-encoded data. The workshop's own
//! `texenc` is self-described as "fine for workbench preview", while [`crate::texture_encode`] is
//! the port of the encoder that has actually produced working game textures — a difference this
//! module deliberately does not decide on anyone's behalf.

use crate::ffcs::{find_chunk, load_ffcs_archive};
use crate::sges::decompress_block;
use crate::types::{TYPE_HASH_MODEL, TYPE_ID_MODEL};
use crate::ucfx::parse_block_entry_table;
use std::path::Path;

/// Wrap one UCFX container as a **single-entry block** — the shape `inject_*_into_donor_block` takes
/// and the shape a patch block must be.
///
/// A decompressed block is `[entry table][containers…]`, never a bare container: the 20-byte header
/// is `count=1`, then `name_hash`, `type_hash`, `field_c`, `chunk_size`. Handing a raw container to
/// `PatchBlock` instead makes the loader read the container's `UCFX` magic as an entry-table field —
/// the WAD hashes fine and is structurally meaningless, which no digest check will catch.
pub fn single_entry_block(
    name_hash: u32,
    type_hash: u32,
    field_c: u32,
    container: &[u8],
) -> Vec<u8> {
    let mut block = Vec::with_capacity(20 + container.len());
    block.extend_from_slice(&1u32.to_le_bytes());
    block.extend_from_slice(&name_hash.to_le_bytes());
    block.extend_from_slice(&type_hash.to_le_bytes());
    block.extend_from_slice(&field_c.to_le_bytes());
    block.extend_from_slice(&(container.len() as u32).to_le_bytes());
    block.extend_from_slice(container);
    block
}

/// Locate a model container inside a decompressed block: `(start, end, field_c)`.
pub fn find_model_container(decompressed: &[u8], want: u32) -> Option<(usize, usize, u32)> {
    let (count, entries) = parse_block_entry_table(decompressed);
    let mut offset = 4 + count as usize * 16;
    for e in &entries {
        let end = offset + e.chunk_size as usize;
        if end > decompressed.len() {
            break;
        }
        if e.type_hash == TYPE_HASH_MODEL && e.name_hash == want {
            return Some((offset, end, e.field_c));
        }
        offset = end;
    }
    None
}

/// Resolve a donor model across the WAD stack, **reverse order — last mounted wins**, sourcing from
/// the block its ASET row points at (the same container the engine instantiates).
///
/// Returns it already wrapped by [`single_entry_block`].
///
/// Errors carry the LAST failure rather than the first: a donor missing from the top overlay but
/// present in the base is normal, so reporting the first miss would name the wrong WAD.
pub fn donor_block<P: AsRef<Path>>(wad_paths: &[P], donor: u32) -> Result<Vec<u8>, String> {
    let mut last = format!("donor 0x{donor:08X}: not in any wad of the stack");
    for path in wad_paths.iter().rev() {
        let path = path.as_ref();
        let mut file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                last = format!("open {}: {e}", path.display());
                continue;
            }
        };
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let archive = match load_ffcs_archive(&mut file, size) {
            Ok(a) => a,
            Err(e) => {
                last = format!("FFCS {}: {e}", path.display());
                continue;
            }
        };
        let Some(entry) = archive
            .aset
            .iter()
            .find(|e| e.asset_hash == donor && e.type_id == TYPE_ID_MODEL)
        else {
            continue;
        };
        let block_index = entry.block_index();
        let dec = match decompress_block(&mut file, &archive.indx, block_index) {
            Ok(d) => d,
            Err(e) => {
                last = format!("decompress block {block_index} of {}: {e}", path.display());
                continue;
            }
        };
        let Some((start, end, field_c)) = find_model_container(&dec, donor) else {
            last = format!(
                "donor 0x{donor:08X}: ASET points at block {block_index} of {} but no model \
                 container is there",
                path.display()
            );
            continue;
        };
        return Ok(single_entry_block(
            donor,
            TYPE_HASH_MODEL,
            field_c,
            &dec[start..end],
        ));
    }
    Err(last)
}

/// The base WAD's `CSUM` `(value, meta)`, to be mirrored into an overlay.
///
/// The proven publish path carries these across from the base rather than zeroing them, so any
/// overlay we emit should do the same — it costs one header read and keeps our output the same
/// shape as WADs that are known to load.
pub fn base_csum<P: AsRef<Path>>(base_wad: P) -> Result<(u32, Option<u32>), String> {
    let path = base_wad.as_ref();
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let archive = load_ffcs_archive(&mut file, size)
        .map_err(|e| format!("base FFCS {}: {e}", path.display()))?;
    let row = find_chunk(&archive.chunks, b"CSUM");
    Ok((row.map(|r| r.offset).unwrap_or(0), row.map(|r| r.meta)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_entry_block_frames_the_container_at_offset_20() {
        let container = b"UCFX....payload".to_vec();
        let block = single_entry_block(0xDEAD_BEEF, TYPE_HASH_MODEL, 7, &container);

        assert_eq!(
            u32::from_le_bytes(block[0..4].try_into().unwrap()),
            1,
            "entry count"
        );
        assert_eq!(
            u32::from_le_bytes(block[4..8].try_into().unwrap()),
            0xDEAD_BEEF
        );
        assert_eq!(
            u32::from_le_bytes(block[8..12].try_into().unwrap()),
            TYPE_HASH_MODEL
        );
        assert_eq!(
            u32::from_le_bytes(block[12..16].try_into().unwrap()),
            7,
            "field_c"
        );
        assert_eq!(
            u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize,
            container.len()
        );
        // The container must start AFTER the table — this is the framing bug the doc comment warns
        // about, and it is invisible to any hash check.
        assert_eq!(&block[20..24], b"UCFX");
    }

    /// `find_model_container` must round-trip what `single_entry_block` produced.
    #[test]
    fn a_framed_block_is_found_again() {
        let container = b"UCFXcontents-here".to_vec();
        let block = single_entry_block(0x1234_5678, TYPE_HASH_MODEL, 3, &container);
        let (start, end, field_c) =
            find_model_container(&block, 0x1234_5678).expect("must find what we just framed");
        assert_eq!(field_c, 3);
        assert_eq!(&block[start..end], &container[..]);
        assert_eq!(
            find_model_container(&block, 0xFFFF_0000),
            None,
            "wrong hash must miss"
        );
    }

    #[test]
    fn an_empty_stack_reports_the_donor_rather_than_a_path_error() {
        let empty: [&Path; 0] = [];
        let err = donor_block(&empty, 0xABCD_1234).unwrap_err();
        assert!(err.contains("0xABCD1234"), "{err}");
    }
}
