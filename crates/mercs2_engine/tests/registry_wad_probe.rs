//! Integration probes for the asset layer against the REAL `vz.wad`.
//!
//! `#[ignore]`d: they need the retail install (discovered via the EA registry key). Run with
//! `cargo test -p mercs2_engine --test registry_wad_probe -- --ignored --nocapture`.
//!
//! What these pin down (measured with `mercs2_probe --bin aset_probe`):
//! - `oc_veh_helicopter_md500` (`0x9FCAE910`) — model chunk in block **3350**.
//! - Its textures `0x22101D86` / `0xFB385BF0` — blocks **2977** / **2976**.
//!
//! An asset's dependencies live in OTHER blocks. That is the whole reason the engine has a residency
//! + registry layer rather than a per-hash extractor.

use mercs2_engine::registry::AssetRegistry;
use mercs2_engine::wad;
use mercs2_formats::types::{TYPE_HASH_MODEL, TYPE_HASH_TEXTURE};

const MD500: u32 = 0x9FCA_E910;
const MD500_BLOCK: u16 = 3350;
const TEX_A: u32 = 0x2210_1D86; // block 2977
const TEX_B: u32 = 0xFB38_5BF0; // block 2976

fn open_base() -> Option<Vec<wad::Wad>> {
    let path = wad::registry_vz_wad()?;
    Some(vec![wad::open(&path).ok()?])
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn resolving_a_model_makes_its_block_resident_and_registers_block_mates() {
    let Some(mut wads) = open_base() else { return eprintln!("no vz.wad; skipping") };
    let mut r = AssetRegistry::default();

    let c = r.resolve(&mut wads, TYPE_HASH_MODEL, MD500).expect("md500 model resolves");
    assert_eq!(c.block, MD500_BLOCK, "md500's model chunk lives in block 3350");
    assert!(r.slice(c).is_some_and(|b| b.starts_with(b"UCFX")), "resolved bytes are a UCFX container");

    let s = r.stats();
    assert_eq!(s.resident_blocks, 1);
    assert!(
        s.registered_chunks > 1,
        "one block registers ALL its chunks, not just the requested one (got {})",
        s.registered_chunks
    );

    // The block carries a type-27 texture row under the model's OWN name hash — a hash can own rows
    // of several types, which is why the registry is keyed by (type_hash, name_hash).
    assert!(r.lookup(TYPE_HASH_MODEL, MD500).is_some());
    assert!(r.lookup(TYPE_HASH_TEXTURE, MD500).is_some(), "0x9FCAE910 is also a texture");
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn textures_resolve_from_other_blocks_than_the_model() {
    let Some(mut wads) = open_base() else { return eprintln!("no vz.wad; skipping") };
    let mut r = AssetRegistry::default();

    r.resolve(&mut wads, TYPE_HASH_MODEL, MD500).expect("model");
    assert_eq!(r.stats().resident_blocks, 1);
    // Not registered by the model's block: these textures live elsewhere in the archive.
    assert!(r.lookup(TYPE_HASH_TEXTURE, TEX_A).is_none());

    let a = r.resolve(&mut wads, TYPE_HASH_TEXTURE, TEX_A).expect("texture A streams in");
    assert_ne!(a.block, MD500_BLOCK, "texture A is NOT in the model's block");
    let b = r.resolve(&mut wads, TYPE_HASH_TEXTURE, TEX_B).expect("texture B streams in");
    assert_ne!(b.block, MD500_BLOCK);
    assert_ne!(a.block, b.block, "and the two textures are in different blocks from each other");

    assert_eq!(r.stats().resident_blocks, 3, "model block + one block per texture");
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn a_resolved_chunk_equals_what_the_old_per_hash_extractor_returned() {
    // The registry must be a drop-in for `wad::extract_container`, byte for byte — otherwise the
    // switchover silently changes what every model loader sees.
    let Some(mut wads) = open_base() else { return eprintln!("no vz.wad; skipping") };
    let mut r = AssetRegistry::default();

    for hash in [MD500, 0xA3C1_FABC /* mattias_v3 */, 0xE540_47D5 /* boat_destroyer */] {
        let legacy = wad::extract_container(&mut wads[0], hash).expect("legacy extract");
        let c = r.resolve(&mut wads, TYPE_HASH_MODEL, hash).expect("registry resolve");
        assert_eq!(r.slice(c).unwrap(), &legacy[..], "0x{hash:08X} differs from the legacy path");
    }
}

#[test]
#[ignore = "needs the retail vz.wad"]
fn eviction_drops_chunks_and_a_later_resolve_streams_them_back() {
    let Some(mut wads) = open_base() else { return eprintln!("no vz.wad; skipping") };
    let mut r = AssetRegistry::with_capacity(1); // force eviction on the second block

    r.resolve(&mut wads, TYPE_HASH_MODEL, MD500).expect("model");
    r.resolve(&mut wads, TYPE_HASH_TEXTURE, TEX_A).expect("texture");
    assert_eq!(r.stats().resident_blocks, 1, "cap of 1 holds");
    assert!(r.stats().evicted_total >= 1);
    assert!(r.lookup(TYPE_HASH_MODEL, MD500).is_none(), "evicted block's chunks unregister");

    // Retail would fault on the stale handle here (AV 0x47AA5C). We stream it back in.
    let again = r.resolve(&mut wads, TYPE_HASH_MODEL, MD500).expect("re-streams on demand");
    assert_eq!(again.block, MD500_BLOCK);
}
