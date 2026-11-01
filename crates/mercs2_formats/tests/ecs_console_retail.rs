//! The console bakes' ECS containers, converted, against PC retail.
//!
//! The PS3 bake's `worldentity`, 923 layers and guidmap are compact, and the PC bake writes the same
//! world out named. With the layouts read from the PC bake, every record of the PS3 bake must equal
//! its PC twin byte for byte, so a width, stride or bit order that is wrong shows up here.
//!
//! Kit models' `MIXR` tables get the same check from the Xbox bake, whose meshes convert.
//!
//! Game-gated: built by the `retail` feature, reads the `vz_wad`, `ps3_vz_wad` and `xbox_vz_wad` of
//! the repo-root `.mercs2-local.toml`, and fails if one is absent.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use mercs2_formats::be_to_le::convert::convert_block;
use mercs2_formats::be_to_le::ecs::{ecs_containers, is_compact, read_records, ComponentSchemas};
use mercs2_formats::be_to_le::texture::{archive_textures, rung_of_path, BlockTextures, TextureTable};
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block};
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths::{local_config_ps3_vz_wad, local_config_vz_wad, local_config_xbox_vz_wad};
use mercs2_formats::mixr::Mixr;
use mercs2_formats::placement::comp_inventory;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_GUIDMAP, TYPE_HASH_WORLD_ENTITY_DATA};
use mercs2_formats::ucfx::block_chunk_spans;
use mercs2_formats::worldentity::WorldEntity;

#[test]
fn every_ps3_record_equals_its_pc_twin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pc = local_config_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let ps3 = local_config_ps3_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let schemas = ComponentSchemas::from_vz_wad(&pc).unwrap();
    assert_eq!(schemas.len(), 207);
    let (console, retail) = (ecs_containers(&ps3, &schemas).unwrap(), ecs_containers(&pc, &schemas).unwrap());
    assert_eq!(console.len(), 925);
    assert_eq!(console.keys().collect::<Vec<_>>(), retail.keys().collect::<Vec<_>>());
    let mut records = 0usize;
    for (key, c) in &console {
        let r = &retail[key];
        if key.1 == TYPE_HASH_GUIDMAP {
            assert_eq!(c, r, "the guidmap");
            continue;
        }
        let (a, b) = (read_records(c, &schemas).unwrap(), read_records(r, &schemas).unwrap());
        for tag in ["UNIQ", "flgt", "flgs"] {
            assert_eq!(a.rows.get(tag), b.rows.get(tag), "0x{:08X} {tag}", key.0);
        }
        for (hash, class) in &a.classes {
            let twin = &b.classes[hash];
            for (k, v) in &class.by_key {
                assert_eq!(Some(v), twin.by_key.get(k), "0x{:08X} {} 0x{k:08X}", key.0, class.class);
                records += 1;
            }
        }
    }
    assert_eq!(records, 220_085);
}

#[test]
fn every_xbox_mixr_equals_its_pc_twin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pc = local_config_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let xbox = local_config_xbox_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let contains = |b: &[u8], tag: &[u8; 4]| b.windows(4).any(|w| w == tag);
    let mut f = File::open(&pc).unwrap();
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).unwrap();
    let mut twins = HashMap::new();
    for b in 0..ar.indx.len() {
        let dec = decompress_block(&mut f, &ar.indx, b as u16).unwrap();
        if !contains(&dec, b"MIXR") {
            continue;
        }
        for (e, r) in block_chunk_spans(&dec).unwrap() {
            if let Some(m) = Mixr::find(&dec[r]).unwrap() {
                twins.insert((e.name_hash, e.type_hash), m);
            }
        }
    }
    assert!(!twins.is_empty());
    let doh = std::fs::read(&xbox).unwrap();
    let (_, rows) = parse_be_ffcs(&doh).unwrap();
    let indx = rows.iter().find(|r| r.tag == "INDX").unwrap();
    let entries = parse_be_indx(&doh, indx.offset as usize, indx.meta as usize);
    let paths_row = rows.iter().find(|r| r.tag == "PTHS").unwrap();
    let paths = parse_be_pths(&doh, paths_row.offset as usize, paths_row.meta as usize);
    let table = TextureTable::from_doh(&doh, &entries, &paths, &archive_textures(&pc).unwrap()).unwrap();
    let mut compared = 0usize;
    for (i, e) in entries.iter().enumerate() {
        let be = read_be_block(&doh, e).unwrap();
        if !contains(&be, b"RXIM") {
            continue;
        }
        let textures = BlockTextures { table: &table, rung: rung_of_path(&paths[i]) };
        let le = convert_block(&be, false, None, None, None, Some(textures)).unwrap();
        for (entry, r) in block_chunk_spans(&le).unwrap() {
            let Some(m) = Mixr::find(&le[r]).unwrap() else { continue };
            assert_eq!(twins.get(&(entry.name_hash, entry.type_hash)), Some(&m), "0x{:08X}", entry.name_hash);
            compared += 1;
        }
    }
    assert!(compared > 0);
    eprintln!("{compared} MIXR tables equal their PC twins");
}

#[test]
fn an_inflated_console_container_reads_as_its_pc_twin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pc = local_config_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let ps3 = local_config_ps3_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let schemas = ComponentSchemas::from_vz_wad(&pc).unwrap();
    let (console, retail) = (ecs_containers(&ps3, &schemas).unwrap(), ecs_containers(&pc, &schemas).unwrap());
    for (key, c) in console.iter().filter(|(k, _)| k.1 != TYPE_HASH_GUIDMAP) {
        assert!(is_compact(c), "0x{:08X}", key.0);
        assert!(!is_compact(&retail[key]), "0x{:08X}", key.0);
        let named = schemas.inflate(c).unwrap();
        assert!(!is_compact(&named));
        // The console strips most layers' Name groups; every other group is there under its name.
        let classes = |c: &[u8]| {
            let mut v: Vec<String> =
                comp_inventory(c).into_iter().filter_map(|i| i.info_name).filter(|n| n != "Name").collect();
            v.sort();
            v
        };
        assert_eq!(classes(&named), classes(&retail[key]), "0x{:08X}", key.0);
        let (a, b) = (read_records(&named, &schemas).unwrap(), read_records(&retail[key], &schemas).unwrap());
        for (hash, class) in &a.classes {
            for (k, v) in &class.by_key {
                assert_eq!(Some(v), b.classes[hash].by_key.get(k), "0x{:08X} {} 0x{k:08X}", key.0, class.class);
            }
        }
    }
    let we = console.iter().find(|(k, _)| k.1 == TYPE_HASH_WORLD_ENTITY_DATA).map(|(_, v)| v).unwrap();
    let we = WorldEntity::parse(&schemas.inflate(we).unwrap()).unwrap();
    let twin = retail.iter().find(|(k, _)| k.1 == TYPE_HASH_WORLD_ENTITY_DATA).map(|(_, v)| v).unwrap();
    let twin = WorldEntity::parse(twin).unwrap();
    let names = |w: &WorldEntity| {
        let mut v: Vec<(String, Vec<u32>)> =
            w.names().unwrap().into_iter().map(|(n, k)| (n, k.to_vec())).collect();
        v.sort();
        v
    };
    assert_eq!(names(&we), names(&twin));
}

#[test]
fn a_converted_compact_worldentity_parses_and_rewrites_to_its_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pc = local_config_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let ps3 = local_config_ps3_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let schemas = ComponentSchemas::from_vz_wad(&pc).unwrap();
    let console = ecs_containers(&ps3, &schemas).unwrap();
    let bytes = console.iter().find(|(k, _)| k.1 == TYPE_HASH_WORLD_ENTITY_DATA).map(|(_, v)| v).unwrap();
    assert!(WorldEntity::parse(bytes).unwrap_err().contains("class layouts"));
    let we = WorldEntity::parse_compact(bytes, &schemas).unwrap();
    assert!(we.header.is_compact());
    assert_eq!(&we.write().unwrap(), bytes);
    for key in we.all_keys() {
        let want = we.derived_flag_bits(key);
        let got = we.flags.iter().find(|f| f.key == key).map_or(&[][..], |f| &f.bits[..]);
        assert_eq!(got, &want[..], "flgs of 0x{key:08X}");
    }
}
