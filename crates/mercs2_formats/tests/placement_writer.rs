//! Does the placement writer edit exactly one field and nothing else?
//!
//! Step 0 (`placement_roundtrip_survey.rs`) proved an in-place patch is bounded. This exercises the
//! writer it justified: `patch_transform` / `patch_model` must change precisely the targeted field
//! of the targeted entity, leave every other byte alone (pad, tail, sibling records), read back
//! through `load_placements` as the new value, and restore byte-for-byte when reverted.

use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::placement::{comp_inventory, load_placements, patch_model, patch_transform};
use mercs2_formats::sges::decompress_block;

fn vz_wad() -> Option<PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// The first placement block that carries a Transform (and, for the model test, a ModelName).
fn a_placement_block() -> Option<Vec<u8>> {
    let wad = vz_wad()?;
    let mut file = std::fs::File::open(&wad).ok()?;
    let size = file.metadata().ok()?.len();
    let archive = load_ffcs_archive(&mut file, size).ok()?;
    for (idx, path) in archive.paths.iter().enumerate() {
        let p = path.to_lowercase();
        if p.contains("layers_static") || p.contains("vz_state") {
            if let Ok(dec) = decompress_block(&mut file, &archive.indx, idx as u16) {
                if load_placements(&dec).map(|v| !v.is_empty()).unwrap_or(false) {
                    return Some(dec);
                }
            }
        }
    }
    None
}

/// ★ Move an entity: `patch_transform` changes its pos, reads back as the new value, and reverting
/// restores the block byte-for-byte — so an edit is exactly one field, nothing more.
#[test]
fn patch_transform_moves_one_entity_and_reverts_byte_identically() {
    let Some(original) = a_placement_block() else {
        eprintln!("SKIPPING: no vz.wad / no placement block");
        return;
    };
    let places = load_placements(&original).expect("parse");
    // Pick a placement with a resolvable key and note its pos/quat.
    let target = places[0].clone();
    let key = target.key;

    let mut edited = original.clone();
    let new_pos = [target.pos[0] + 12.5, target.pos[1] - 3.0, target.pos[2] + 7.25];
    let n = patch_transform(&mut edited, key, Some(new_pos), None);
    assert!(n >= 1, "the key must match at least one Transform record");
    assert_ne!(edited, original, "an edit must change the bytes");

    // Read back through the parser: the moved entity reports the new pos, its quat is untouched.
    let after = load_placements(&edited).expect("re-parse edited");
    let moved = after.iter().find(|p| p.key == key).expect("entity still present");
    assert_eq!(moved.pos, new_pos, "the new position must read back");
    assert_eq!(moved.quat, target.quat, "the rotation must be untouched");

    // Only that entity moved — every other placement is unchanged.
    for (a, b) in load_placements(&original).unwrap().iter().zip(&after) {
        if a.key != key {
            assert_eq!(a.pos, b.pos, "a non-target entity {:08X} moved", a.key);
        }
    }

    // Revert: writing the original pos back reproduces the block exactly.
    patch_transform(&mut edited, key, Some(target.pos), None);
    assert_eq!(edited, original, "reverting the edit must restore the block byte-for-byte");
}

/// ★ Reskin an entity: `patch_model` repoints a ModelName record, and reverting restores the bytes.
#[test]
fn patch_model_repoints_one_entity_and_reverts() {
    let Some(original) = a_placement_block() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    // Find a ModelName record's key + current hash by reading its data span directly.
    let comps = comp_inventory(&original);
    let Some(mn) = comps.iter().find(|c| c.info_name.as_deref() == Some("ModelName") && c.data_size.unwrap_or(0) >= 8) else {
        eprintln!("SKIPPING: no ModelName COMP in this block");
        return;
    };
    let off = mn.data_off.unwrap();
    let key = u32::from_le_bytes(original[off..off + 4].try_into().unwrap());
    let old_hash = u32::from_le_bytes(original[off + 4..off + 8].try_into().unwrap());
    let new_hash = old_hash ^ 0x5A5A_5A5A;

    let mut edited = original.clone();
    let n = patch_model(&mut edited, key, new_hash);
    assert!(n >= 1, "the key must match at least one ModelName record");
    assert_eq!(
        u32::from_le_bytes(edited[off + 4..off + 8].try_into().unwrap()),
        new_hash,
        "the model hash must be repointed"
    );
    // Nothing outside the 4 hash bytes of matching records changed near this record's key.
    assert_eq!(&edited[off..off + 4], &original[off..off + 4], "the entity key must be untouched");

    patch_model(&mut edited, key, old_hash);
    assert_eq!(edited, original, "reverting the reskin must restore the block byte-for-byte");
}

/// A key that names no placement patches nothing and leaves the block untouched.
#[test]
fn an_unknown_key_patches_nothing() {
    let Some(original) = a_placement_block() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    let mut edited = original.clone();
    let n = patch_transform(&mut edited, 0xFFFF_FFFE, Some([1.0, 2.0, 3.0]), None);
    assert_eq!(n, 0, "a nonexistent key must match no record");
    assert_eq!(edited, original, "a no-match edit must not change any bytes");
}
