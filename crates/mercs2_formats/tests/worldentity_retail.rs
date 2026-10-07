//! The retail `worldentity` container, read and written by `mercs2_formats::worldentity`.
//!
//! * The container `0x50075B3B` parses with `ucfx::parse_ucfx_tree` and through the typed model,
//!   and re-writes to the identical 1,819,324 bytes.
//! * Every component's `data` is consumed exactly by its layout, and every record re-encodes from
//!   its typed values.
//! * The `info` record count equals the records, and every `flgs` bitset equals the classes the
//!   components give its key.
//! * The facts the format doc states about this container: its counts, the C4 template's records,
//!   the two key ranges.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::schema::FieldValue;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{parse_block_entry_table, parse_ucfx_tree, write_ucfx_tree};
use mercs2_formats::worldentity::{
    Layout, Payload, Value, WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH,
};

/// The retail container and the index of the block it is in.
fn retail_worldentity() -> (u16, Vec<u8>) {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut f = File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS");
    let rows: Vec<_> =
        archive.aset.iter().filter(|a| a.asset_hash == RETAIL_WORLDENTITY_NAME_HASH).collect();
    assert_eq!(rows.len(), 1, "one ASET row names the worldentity");
    let bi = rows[0].block_index();
    let dec = decompress_block(&mut f, &archive.indx, bi).expect("decompress");
    let (_n, entries) = parse_block_entry_table(&dec);
    let mut pos = 4 + 16 * entries.len();
    let mut found = Vec::new();
    for e in &entries {
        let end = pos + e.chunk_size as usize;
        if e.type_hash == WORLDENTITY_TYPE_HASH {
            assert_eq!(e.name_hash, RETAIL_WORLDENTITY_NAME_HASH);
            found.push(dec[pos..end].to_vec());
        }
        pos = end;
    }
    assert_eq!(found.len(), 1, "block {bi} carries one worldentity");
    assert!(archive.paths[bi as usize].ends_with(r"\VZ\resident_P000_Q3.block"), "{}", archive.paths[bi as usize]);
    (bi, found.pop().unwrap())
}

#[test]
fn the_retail_worldentity_round_trips_through_the_tree_and_the_typed_model() {
    let (bi, c) = retail_worldentity();
    assert_eq!(bi, 3185);
    assert_eq!(c.len(), 1_819_324);

    // The generic UCFX tree reads it strictly and re-writes it.
    let tree = parse_ucfx_tree(&c).expect("parse_ucfx_tree");
    assert_eq!(tree.len(), 200, "CHDR, enum, UNIQ, 195 COMP, flgt, flgs");
    assert_eq!(write_ucfx_tree(&tree), c);

    // The typed model reads every chunk and re-writes the identical bytes.
    let we = WorldEntity::parse(&c).expect("typed parse");
    assert_eq!(we.write().expect("write"), c);

    assert_eq!((we.header.field0, we.header.stride_gate, we.header.flags), (0, 0x33, 1));
    assert_eq!(we.enums.len(), 72);
    assert_eq!(we.instances.len(), 6126);
    assert!(we.instances.windows(2).all(|w| w[0] < w[1]), "UNIQ ascending");
    assert_eq!(we.components.len(), 195);
    assert_eq!(we.flag_table.classes.len(), 215);
    assert_eq!(we.flags.len(), 6057);

    let layouts: BTreeMap<String, Layout> =
        we.components.iter().map(|c| (c.class.clone(), c.layout)).collect();
    assert_eq!(layouts["Name"], Layout::Name);
    assert_eq!(layouts["PointLocation"], Layout::PointLocation);
    assert_eq!(layouts["NetCategoryInfo"], Layout::PackedU16);
    assert_eq!(we.components.iter().filter(|c| c.layout == Layout::Fixed).count(), 192);
    assert_eq!(we.groups_of("LightObject").count(), 2);
}

#[test]
fn every_record_re_encodes_from_its_typed_values() {
    let (_, c) = retail_worldentity();
    let we = WorldEntity::parse(&c).expect("typed parse");
    let mut records = 0usize;
    let mut fields = 0usize;
    for comp in &we.components {
        for r in &comp.records {
            let v = comp.decode(&r.payload).expect("decode");
            fields += v.len();
            assert_eq!(comp.encode(&v).expect("encode"), r.payload, "{}", comp.class);
            records += 1;
        }
    }
    let keys: usize = we.components.iter().map(|c| c.keys().count()).sum();
    eprintln!("{records} records, {fields} field values, {keys} keyed entries");
    assert_eq!(we.names().unwrap().len(), 6126);
}

#[test]
fn every_flgs_bitset_is_the_classes_its_key_has() {
    let (_, c) = retail_worldentity();
    let we = WorldEntity::parse(&c).expect("typed parse");
    for f in &we.flags {
        assert_eq!(f.bits, we.derived_flag_bits(f.key), "key 0x{:08X}", f.key);
    }
    // Keys without a flgs record are the ones whose classes give no bit.
    let with: std::collections::BTreeSet<u32> = we.flags.iter().map(|f| f.key).collect();
    let without: Vec<u32> = we.instances.iter().copied().filter(|k| !with.contains(k)).collect();
    assert_eq!(without.len(), 69);
    assert!(without.iter().all(|&k| we.derived_flag_bits(k).is_empty()));
    // Name and NetCategoryInfo are the two classes flgt does not list.
    let unlisted: Vec<&str> = we
        .components
        .iter()
        .filter(|c| we.flag_table.bit_of(&c.class).is_none())
        .map(|c| c.class.as_str())
        .collect();
    assert_eq!(unlisted, ["Name", "NetCategoryInfo"]);
}

#[test]
fn the_c4_template_and_the_key_ranges() {
    let (_, c) = retail_worldentity();
    let we = WorldEntity::parse(&c).expect("typed parse");
    let names = we.names().unwrap();
    let key_of = |n: &str| names.iter().find(|(m, _)| m == n).map(|(_, k)| k.to_vec());
    assert_eq!(key_of("global_particle_explosion_c4"), Some(vec![0x8000_8028]));
    assert_eq!(key_of("fx_Explosion_HugeOil_RigOnly"), Some(vec![0x8000_8756]));
    assert_eq!(key_of("global_particle_fire_carhood"), Some(vec![0x8000_8C06]));

    let classes_of = |key: u32| -> Vec<&str> {
        we.components.iter().filter(|c| c.keys().any(|k| k == key)).map(|c| c.class.as_str()).collect()
    };
    assert_eq!(
        classes_of(0x8000_8028),
        ["EffectTemplate", "HibernationControl", "RedEffectComponent", "SoundEffect", "Name"]
    );
    assert_eq!(
        classes_of(0x8000_8C06),
        ["EffectTemplate", "HibernationControl", "PhysicalLink", "RedEffectComponent", "SoundEffect", "Name"]
    );
    assert_eq!(
        classes_of(0x8000_8756),
        [
            "CameraShake",
            "DamageKey",
            "EffectTemplate",
            "Explosive",
            "GenericLOD",
            "HibernationControl",
            "ObjectScript",
            "PhysicalLink",
            "SoundEffect",
            "Stimulus",
            "Name"
        ]
    );

    // The C4 template's RedEffectComponent `name` field (0x1DE5C824) is the effect asset
    // global_explosion_c4.
    let red = we.groups_of("RedEffectComponent").next().unwrap();
    let rec = red.records.iter().find(|r| r.keys.contains(&0x8000_8028)).unwrap();
    let v = red.decode(&rec.payload).unwrap();
    let at = red.schema.fields.iter().position(|f| f.name_hash == 0x1DE5_C824).unwrap();
    assert_eq!(v[at], Value::Field(FieldValue::U32(0x41B4_326E)));
    assert_eq!(mercs2_formats::hash::pandemic_hash_m2("global_explosion_c4"), 0x41B4_326E);

    // Two key ranges: the top nibble is 8 or 9, every key carries the template bit.
    let all = we.all_keys();
    let n8 = all.iter().filter(|&&k| k >> 28 == 8).count();
    let n9 = all.iter().filter(|&&k| k >> 28 == 9).count();
    assert_eq!((n8, n9, all.len()), (5993, 133, 6126));
    assert_eq!(all.iter().copied().max(), Some(0x9000_01F7));
    assert_eq!(all.iter().copied().filter(|k| k >> 28 == 8).max(), Some(0x8000_B3C4));

    // The PointLocation record: a 32-byte blob and an empty string.
    let pl = we.groups_of("PointLocation").next().unwrap();
    assert!(matches!(&pl.records[0].payload, Payload::PointLocation { text, .. } if text.is_empty()));
}
