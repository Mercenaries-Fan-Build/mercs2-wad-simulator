//! Every retail FaceFX container reads to the last byte of its archive.
//!
//! Covers every `facefxactor` and `facefxanimationset` entry of every block of `vz.wad`, found by
//! type hash. The counts are asserted exactly, so a run that reads fewer containers fails instead of
//! passing. `pmc_hum_mattias`'s actor is checked field by field against its bytes.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::fs::File;
use std::path::{Path, PathBuf};

use mercs2_formats::facefx::{self, NodeKind};
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_FACEFX_ACTOR, TYPE_HASH_FACEFX_ANIMSET};

/// FaceFX containers in the blocks of retail `vz.wad` (a shared asset counts once per block).
const ACTORS: usize = 31;
const ANIMSETS: usize = 86;

fn vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"))
}

/// Every FaceFX container of `path`: (where, type hash, name hash, bytes).
fn containers(path: &Path) -> Vec<(String, u32, u32, Vec<u8>)> {
    let mut f = File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut out = Vec::new();
    for bi in 0..arch.indx.len() {
        let block = decompress_block(&mut f, &arch.indx, bi as u16)
            .unwrap_or_else(|e| panic!("{} block {bi}: {e}", path.display()));
        let spans = mercs2_formats::ucfx::block_chunk_spans(&block).unwrap_or_else(|e| panic!("block {bi}: {e}"));
        for (e, span) in spans {
            if e.type_hash == TYPE_HASH_FACEFX_ACTOR || e.type_hash == TYPE_HASH_FACEFX_ANIMSET {
                let what = format!("block {bi} ({}) 0x{:08X}", arch.paths[bi], e.name_hash);
                out.push((what, e.type_hash, e.name_hash, block[span].to_vec()));
            }
        }
    }
    out
}

#[test]
fn every_facefx_container_reads_to_its_end() {
    let all = containers(&vz_wad());
    let (mut actors, mut sets, mut failed) = (0, 0, Vec::new());
    for (what, ty, _, bytes) in &all {
        let r = if *ty == TYPE_HASH_FACEFX_ACTOR {
            actors += 1;
            facefx::parse_actor_container(bytes).map(|_| ())
        } else {
            sets += 1;
            facefx::parse_animset_container(bytes).map(|_| ())
        };
        if let Err(e) = r {
            failed.push(format!("{what}: {e}"));
        }
    }
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
    assert_eq!((actors, sets), (ACTORS, ANIMSETS));
}

#[test]
fn mattias_actor_holds_his_face_bones_and_poses() {
    let mattias = mercs2_formats::hash::pandemic_hash_m2("pmc_hum_mattias");
    let all = containers(&vz_wad());
    let (_, _, _, bytes) = all
        .iter()
        .find(|(_, ty, name, _)| *ty == TYPE_HASH_FACEFX_ACTOR && *name == mattias)
        .expect("pmc_hum_mattias has a facefxactor");
    let a = facefx::parse_actor_container(bytes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((a.header.version, a.header.licensee.as_str()), (1610, "Pandemic Studios, Inc."));
    assert_eq!((a.names.len(), a.object_hint), (56, 33));
    let bones: Vec<&str> = a.root.master_bones.iter().map(|b| b.bone.name.as_str()).collect();
    assert_eq!(bones.len(), 20);
    assert_eq!((bones[0], bones[19]), ("bone_eyeball_left", "bone_tongue_tip"));
    let eye = &a.root.master_bones[0].bone;
    assert_eq!(eye.position[0].to_bits(), 0xBD4ACAFF);
    assert_eq!(eye.scale, [1.0, 1.0, 1.0]);
    assert_eq!(a.root.nodes.len(), 33);
    let eat = a.root.nodes.iter().find(|n| n.name == "Eat").expect("an Eat node");
    assert_eq!(eat.class, "FxBonePoseNode");
    assert!(matches!(&eat.kind, NodeKind::BonePose(b) if b.len() == 11));
    assert_eq!(a.root.groups[0].name, "Default");
}
