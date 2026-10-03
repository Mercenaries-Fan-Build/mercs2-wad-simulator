//! Every retail template re-expressed through the template author form and re-encoded.
//!
//! For each of the 6,126 template keys of the retail `worldentity`, the template is written as a
//! form ([`TemplateForm::express`]), serialized to YAML, read back, checked and typed against the
//! schemas ([`TemplateForm::lower`]), and every component record re-encoded. Each must equal the
//! retail record's payload byte for byte. This is what shows the form covers every class, field
//! type and bit field retail uses.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::fs::File;
use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::parse_block_entry_table;
use mercs2_formats::worldentity::{
    Payload, WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH,
};
use mercs2_quartermaster::template::{self, TemplateForm};
use mercs2_quartermaster::Format;

fn retail_worldentity() -> WorldEntity {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut f = File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS");
    let row = archive
        .aset
        .iter()
        .find(|a| a.asset_hash == RETAIL_WORLDENTITY_NAME_HASH)
        .expect("the worldentity ASET row");
    let dec = decompress_block(&mut f, &archive.indx, row.block_index()).expect("decompress");
    let (_n, entries) = parse_block_entry_table(&dec);
    let mut pos = 4 + 16 * entries.len();
    for e in &entries {
        let end = pos + e.chunk_size as usize;
        if e.type_hash == WORLDENTITY_TYPE_HASH && e.name_hash == RETAIL_WORLDENTITY_NAME_HASH {
            return WorldEntity::parse(&dec[pos..end]).expect("parse");
        }
        pos = end;
    }
    panic!("no worldentity in its block");
}

#[test]
fn every_retail_template_re_expresses_through_the_form_byte_identically() {
    let we = retail_worldentity();
    let mut templates = 0usize;
    let mut records = 0usize;
    let mut classes = std::collections::BTreeSet::new();
    for &key in &we.instances {
        let form = TemplateForm::express(&we, key).unwrap_or_else(|e| panic!("express 0x{key:08X}: {e}"));
        let text = template::to_string(&form, Format::Yaml).expect("to yaml");
        let back = template::from_str(&text, Format::Yaml).expect("from yaml");
        assert_eq!(back, form, "0x{key:08X}: the YAML text does not read back to the same form");
        let decl = back.lower(&we, key).unwrap_or_else(|e| panic!("lower 0x{key:08X}: {e}"));

        // The retail records of this key, class by class in the form's (sorted) order, each class's
        // records in container order.
        let mut want: Vec<(String, Payload)> = Vec::new();
        let mut names: Vec<&str> = we.components.iter().map(|c| c.class.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        for class in names.into_iter().filter(|&c| c != "Name") {
            for c in we.groups_of(class) {
                for r in c.records.iter().filter(|r| r.keys.contains(&key)) {
                    want.push((class.to_string(), r.payload.clone()));
                }
            }
        }
        assert_eq!(decl.components.len(), want.len(), "0x{key:08X}: record count");
        for (d, (class, payload)) in decl.components.iter().zip(&want) {
            assert_eq!(&d.class, class);
            let comp = &we.components[we.append_group(class).unwrap()];
            assert_eq!(&comp.encode(&d.values).unwrap(), payload, "0x{key:08X} {class}");
            classes.insert(class.clone());
            records += 1;
        }
        templates += 1;
    }
    eprintln!("{templates} templates, {records} records, {} classes", classes.len());
    assert_eq!(templates, 6126);
    // Every class but Name is held by some template.
    assert_eq!(classes.len(), 193);
}
