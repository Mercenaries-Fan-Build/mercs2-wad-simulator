//! `add_fx` and `replace_fx` against the retail effects block and worldentity.
//!
//! * The effects block re-writes byte for byte, and every one of its 314 effects re-expresses
//!   through the effect form, reads back from YAML and re-encodes to its own bytes.
//! * `qm build` of `tests/fixtures/fx/qm-fx-a` emits the effects block and the resident block at
//!   their own paths, each the game's with exactly the fixture's edits and additions.
//! * `qm link` of `qm-fx-a` and `qm-fx-b`, in either order, emits one effects block and one
//!   resident block with both Shipments' effects and templates; with a third Shipment that patches a
//!   resident script, the one resident block carries the script and both templates.
//! * The game-gated rules M0257–M0261 fire on what they describe.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

mod common {
    pub mod corpus;
}

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mercs2_formats::fxdict::{parse_effect_container, write_effect_container, EffectContainer};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::patch_wad::{read_patch_wad, PatchBlock};
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::sges::decompress_sges;
use mercs2_formats::types::{TYPE_HASH_EFFECT, TYPE_HASH_MODEL, TYPE_ID_EFFECT};
use mercs2_formats::worldentity::{derived_template_key, WorldEntity};
use mercs2_quartermaster::compat::PlanInput;
use mercs2_quartermaster::effect::{self, EffectForm};
use mercs2_quartermaster::fx::{self, GameFx};
use mercs2_quartermaster::lint;
use mercs2_quartermaster::template::TemplateForm;
use mercs2_quartermaster::{build, discover, Contribution, Format, GameStack, LoadedShipment};

const RESIDENT: &str = r"blocks\VZ\resident_P000_Q3.block";

fn game() -> GameStack {
    let vz = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    GameStack::open(&[vz]).expect("open the game stack")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fx").join(name)
}

fn open(dir: &Path) -> LoadedShipment {
    discover::open(dir).unwrap_or_else(|e| panic!("{}: {e:?}", dir.display()))
}

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qm_fx_retail_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// Every block of a WAD on disk, by path, decompressed, with the block's rows.
fn blocks_of(wad: &Path) -> BTreeMap<String, (Vec<u8>, PatchBlock)> {
    let bytes = std::fs::read(wad).expect("read the WAD");
    read_patch_wad(&bytes)
        .expect("the WAD reads back")
        .blocks
        .into_iter()
        .map(|b| (b.path_string.clone(), (decompress_sges(&b.compressed_data).expect("decompress"), b)))
        .collect()
}

fn effect_of(block: &ScriptsBlock, h: u32) -> EffectContainer {
    let e = block.entries.iter().find(|e| e.name_hash == h && e.type_hash == TYPE_HASH_EFFECT).expect("the effect");
    parse_effect_container(&e.bytes).expect("the effect parses")
}

/// Every emitter of `fx` is `base`'s with the first three bytes of each colour key `rgb`, and nothing
/// else differs.
fn recoloured(fx: &EffectContainer, base: &EffectContainer, rgb: [u8; 3]) {
    let mut want = base.clone();
    for e in want.emitters.iter_mut() {
        for k in e.particle.colr.keys.iter_mut() {
            k.colour = [rgb[0], rgb[1], rgb[2], k.colour[3]];
        }
    }
    assert_eq!(fx, &want);
}

fn worldentity_of(block: &ScriptsBlock) -> WorldEntity {
    WorldEntity::parse(&block.entries[fx::worldentity_entry(block).unwrap()].bytes).expect("the worldentity parses")
}

/// The `add_fx` template of a manifest, by template name.
fn template_of(s: &LoadedShipment, name: &str) -> TemplateForm {
    s.manifest
        .contributions
        .iter()
        .find_map(|c| match c {
            Contribution::AddFx { template, .. } if template.name == name => Some(template.clone()),
            _ => None,
        })
        .expect("the template")
}

#[test]
fn the_effects_block_and_every_effect_re_encode_through_the_form() {
    let mut game = game();
    let base = GameFx::read(&mut game).expect("the game's effects and worldentity");
    let (raw, _) = game.block_and_rows_by_path(fx::EFFECTS_BLOCK.0).unwrap();
    assert_eq!(base.effects.serialize(), raw, "the effects block re-writes byte for byte");
    let effects: Vec<_> = base.effects.entries.iter().filter(|e| e.type_hash == TYPE_HASH_EFFECT).collect();
    let models = base.effects.entries.iter().filter(|e| e.type_hash == TYPE_HASH_MODEL).count();
    assert_eq!((effects.len(), models, base.effects.entries.len()), (314, 46, 360));
    for e in effects {
        let fx = parse_effect_container(&e.bytes).unwrap_or_else(|m| panic!("0x{:08X}: {m}", e.name_hash));
        assert_eq!(write_effect_container(&fx).unwrap(), e.bytes, "0x{:08X} re-writes", e.name_hash);
        let text = effect::to_string(&EffectForm::express(&fx), Format::Yaml).unwrap();
        let back = effect::from_str(&text, Format::Yaml).unwrap_or_else(|m| panic!("0x{:08X}: {m}", e.name_hash));
        assert_eq!(back.encode().unwrap_or_else(|m| panic!("0x{:08X}: {m}", e.name_hash)), e.bytes, "0x{:08X}", e.name_hash);
    }
}

/// A `TEXT` frame is the key of an `fxdict` record: the loader looks each one up there
/// (`FUN_004911a0` -> `FUN_00491510`, a binary search over the record keys) and packs the record's
/// four values as halves. 546 of the 566 distinct retail frames are records; no retail frame is a
/// texture asset.
#[test]
fn retail_frames_are_fxdict_records() {
    let mut game = game();
    let base = GameFx::read(&mut game).unwrap();
    let mut frames = std::collections::BTreeSet::new();
    for e in base.effects.entries.iter().filter(|e| e.type_hash == TYPE_HASH_EFFECT) {
        for em in parse_effect_container(&e.bytes).unwrap().emitters {
            frames.extend(em.particle.text.frames);
        }
    }
    let records = frames.iter().filter(|h| base.frames.contains(h)).count();
    let textures = frames.iter().filter(|&&h| game.has_asset(h, mercs2_formats::types::TYPE_ID_TEXTURE)).count();
    assert_eq!((frames.len(), records, textures, base.frames.len()), (566, 546, 0, 630));
}

#[test]
fn building_qm_fx_a_emits_the_effects_and_resident_blocks_with_its_edits_and_additions() {
    let mut game = game();
    let base = GameFx::read(&mut game).unwrap();
    let s = open(&fixture("qm-fx-a"));
    let out = scratch("build-a");
    let rep = build::build(&s, Some(&mut game), None, Some(&out), None, None).unwrap_or_else(|e| panic!("{e}"));
    let blocks = blocks_of(rep.wad.as_ref().expect("an overlay"));
    let paths: Vec<&String> = blocks.keys().collect();
    assert_eq!(paths, vec![fx::EFFECTS_BLOCK.1, RESIDENT], "the effects block and the resident block");

    // The effects block: the game's entries in order, the C4 effect magenta, the new effect last.
    let (raw, eb) = &blocks[fx::EFFECTS_BLOCK.1];
    let effects = ScriptsBlock::parse(raw).unwrap();
    let c4 = pandemic_hash_m2("global_explosion_c4");
    let cyan = pandemic_hash_m2("qm_fx_cyan_burst");
    assert_eq!(effects.entries.len(), base.effects.entries.len() + 1);
    for (e, b) in effects.entries.iter().zip(&base.effects.entries) {
        assert_eq!((e.name_hash, e.type_hash, e.field_c), (b.name_hash, b.type_hash, b.field_c));
        if e.name_hash == c4 {
            recoloured(&effect_of(&effects, c4), &parse_effect_container(&b.bytes).unwrap(), [255, 0, 255]);
        } else {
            assert!(e.bytes == b.bytes, "0x{:08X} is the game's", e.name_hash);
        }
    }
    let last = effects.entries.last().unwrap();
    assert_eq!((last.name_hash, last.type_hash, last.field_c), (cyan, TYPE_HASH_EFFECT, 0));
    let form = effect::read(&fixture("qm-fx-a/src/qm_fx_cyan_burst.yaml")).unwrap();
    assert_eq!(last.bytes, form.encode().unwrap());
    for r in &eb.aset_entries {
        match base.effects_rows.get(&r.asset_hash) {
            _ if r.asset_hash == cyan => {
                assert_eq!((r.u32_1, r.u32_2 & 0xFFFF, r.u32_3), (0xFFFF_FFFF, 0xFFFF, TYPE_ID_EFFECT))
            }
            Some(&(packed, secondary, type_id)) => {
                assert_eq!((r.u32_1, r.u32_2 & 0xFFFF, r.u32_3), (secondary, packed & 0xFFFF, type_id))
            }
            None => panic!("row 0x{:08X} is neither the game's nor the added effect's", r.asset_hash),
        }
    }
    assert_eq!(eb.aset_entries.len(), effects.entries.len());

    // The resident block: the game's, with the template appended to the worldentity.
    let (raw, _) = &blocks[RESIDENT];
    let resident = ScriptsBlock::parse(raw).unwrap();
    let (game_resident, _) = game.block_and_rows_by_path(r"\VZ\resident_P000_Q3.block").unwrap();
    let game_resident = ScriptsBlock::parse(&game_resident).unwrap();
    let at = fx::worldentity_entry(&resident).unwrap();
    for (i, (e, b)) in resident.entries.iter().zip(&game_resident.entries).enumerate() {
        if i != at {
            assert!(e.bytes == b.bytes && e.name_hash == b.name_hash, "resident entry {i} is the game's");
        }
    }
    assert_eq!(resident.entries.len(), game_resident.entries.len());
    let tpl = template_of(&s, "qm_cyan_burst");
    let key = derived_template_key("qm_cyan_burst");
    let mut want = base.worldentity.clone();
    want.append_template(&tpl.lower(&want, key).unwrap()).unwrap();
    assert!(resident.entries[at].bytes == want.write().unwrap(), "the game's worldentity plus the one template");
    let we = worldentity_of(&resident);
    assert_eq!(fx::template_keys(&we, "qm_cyan_burst").unwrap(), vec![key]);
    assert_eq!(fx::template_effect(&we, key).unwrap(), cyan);
}

/// Link `shipments` (in that order) and return the link WAD's blocks and the load plan's
/// `link_block_paths`.
fn link(label: &str, shipments: &[&LoadedShipment]) -> (BTreeMap<String, (Vec<u8>, PatchBlock)>, Vec<String>) {
    let mut game = game();
    let ids: Vec<String> = shipments.iter().map(|s| format!("shipment:{}", s.manifest.shipment.name)).collect();
    let inputs: Vec<PlanInput<'_>> =
        shipments.iter().zip(&ids).map(|(s, id)| PlanInput { id, shipment: s }).collect();
    let corpus = common::corpus::corpus_root().expect("the vendored Lua corpus");
    let out = scratch(label);
    let rep = build::link_installed(&inputs, &mut game, &corpus, &out, None).unwrap_or_else(|e| panic!("{e}"));
    (blocks_of(rep.wad.as_ref().expect("a link overlay")), rep.plan.link_block_paths)
}

#[test]
fn linking_qm_fx_a_and_qm_fx_b_merges_both_in_either_order() {
    let mut g = game();
    let base = GameFx::read(&mut g).unwrap();
    let (a, b) = (open(&fixture("qm-fx-a")), open(&fixture("qm-fx-b")));
    let carhood = fx::template_effect(&base.worldentity, fx::template_keys(&base.worldentity, "global_particle_fire_carhood").unwrap()[0]).unwrap();
    let c4 = pandemic_hash_m2("global_explosion_c4");
    let mut contents = Vec::new();
    for (label, set) in [("link-ab", [&a, &b]), ("link-ba", [&b, &a])] {
        let (blocks, promised) = link(label, &set);
        let paths: Vec<&String> = blocks.keys().collect();
        assert_eq!(paths, vec![fx::EFFECTS_BLOCK.1, RESIDENT], "{label}");
        assert!(paths.iter().all(|p| promised.contains(p)), "{label}: {promised:?}");
        let effects = ScriptsBlock::parse(&blocks[fx::EFFECTS_BLOCK.1].0).unwrap();
        recoloured(&effect_of(&effects, c4), &effect_of(&base.effects, c4), [255, 0, 255]);
        recoloured(&effect_of(&effects, carhood), &effect_of(&base.effects, carhood), [255, 255, 0]);
        let we = worldentity_of(&ScriptsBlock::parse(&blocks[RESIDENT].0).unwrap());
        let mut templates = BTreeMap::new();
        for (t, fx_name) in [("qm_cyan_burst", "qm_fx_cyan_burst"), ("qm_green_burst", "qm_fx_green_burst")] {
            let key = derived_template_key(t);
            assert_eq!(fx::template_keys(&we, t).unwrap(), vec![key], "{label} {t}");
            assert_eq!(fx::template_effect(&we, key).unwrap(), pandemic_hash_m2(fx_name), "{label} {t}");
            templates.insert(t, TemplateForm::express(&we, key).unwrap());
        }
        let by_hash: BTreeMap<u32, Vec<u8>> = effects.entries.iter().map(|e| (e.name_hash, e.bytes.clone())).collect();
        contents.push((by_hash, templates, we.instances.clone()));
    }
    assert!(contents[0] == contents[1], "both load orders merge the same effects and templates");
}

#[test]
fn a_resident_patch_lua_and_both_templates_share_one_resident_block() {
    let (a, b) = (open(&fixture("qm-fx-a")), open(&fixture("qm-fx-b")));
    let dir = scratch("resident-lua");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.lua"), "-- qm fx retail: resident reach\n").unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        "format: 2\nshipment: { name: qm-fx-lua, version: 1.0.0, target: retail }\ncontributions:\n  \
         - kind: patch_lua\n    target: mrxplayer\n    append: src/a.lua\n",
    )
    .unwrap();
    let c = open(&dir);
    let (blocks, _) = link("link-abc", &[&a, &b, &c]);
    let resident: Vec<&String> = blocks.keys().filter(|p| p.as_str() == RESIDENT).collect();
    assert_eq!(resident.len(), 1);
    let block = ScriptsBlock::parse(&blocks[RESIDENT].0).unwrap();
    let at = block.find_script_by_name("mrxplayer").expect("mrxplayer");
    let lua = block.extract_lua(at).unwrap();
    let mut g = game();
    let (raw, _) = g.block_and_rows_by_path(r"\VZ\resident_P000_Q3.block").unwrap();
    let game_block = ScriptsBlock::parse(&raw).unwrap();
    assert_ne!(lua, game_block.extract_lua(game_block.find_script_by_name("mrxplayer").unwrap()).unwrap(), "the script is linked");
    let we = worldentity_of(&block);
    for t in ["qm_cyan_burst", "qm_green_burst"] {
        assert_eq!(fx::template_keys(&we, t).unwrap(), vec![derived_template_key(t)], "{t}");
    }
}

/// A Shipment directory with `effect_text` (or the fixture's cyan effect) at `src/fx.yaml`, `edits` at
/// `src/e.yaml`, and `contributions`.
fn shipment(label: &str, contributions: &str, effect_text: Option<String>, edits: &str) -> LoadedShipment {
    let dir = scratch(label);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let text = effect_text
        .unwrap_or_else(|| std::fs::read_to_string(fixture("qm-fx-a/src/qm_fx_cyan_burst.yaml")).unwrap());
    std::fs::write(dir.join("src/fx.yaml"), text).unwrap();
    std::fs::write(dir.join("src/e.yaml"), edits).unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        format!("format: 2\nshipment: {{ name: qm-fx-rule, version: 1.0.0, target: retail }}\ncontributions:\n{contributions}"),
    )
    .unwrap();
    open(&dir)
}

fn add_fx(name: &str, template: &str, red: &str) -> String {
    format!(
        "  - kind: add_fx\n    name: {name}\n    effect: src/fx.yaml\n    template:\n      name: {template}\n      \
         name_flag: 1\n      components:\n        EffectTemplate: {{ \"0xB9D95A23\": \"0x00000000\" }}\n        \
         RedEffectComponent: {{ name: {red}, \"0x4D7D459B\": 1.0, \"0x62C7746E\": 0.0, \"0xE8DABAE6\": 1.0, \
         \"0x216E8465\": 10.0, \"0x3902F594\": 0.0, \"0xE351CA81\": \"0x00000001\", \"0xB9BA2DFE\": 0.0, \
         \"0xF88C32BA\": \"0x00000001\", \"0x95323A93\": \"0x00000000\", \"0xE8764CF8\": \"0x00000000\", \
         \"0xA87B6266\": -2.0, \"0x4E4CCD85\": 10.0, \"0x87519019\": 30 }}\n"
    )
}

const MAGENTA: &str = "edits:\n  - { op: colour_rgb, emitter: 0, rgb: [255, 0, 255] }\n";

fn game_codes(s: &LoadedShipment) -> Vec<&'static str> {
    lint::fx_game_checks(&s.manifest, &s.root, &mut game()).unwrap().iter().map(|d| d.rule.code).collect()
}

#[test]
fn the_fx_game_rules_fire_on_what_they_describe() {
    // Quiet: a new effect drawing the fixture's frames (fxdict records), and a C4 edit.
    let drawn = std::fs::read_to_string(fixture("qm-fx-a/src/qm_fx_cyan_burst.yaml")).unwrap();
    let mut unknown = effect::read(&fixture("qm-fx-a/src/qm_fx_cyan_burst.yaml")).unwrap();
    unknown.emitters[0].particle.frames = vec!["qm_no_such_frame".into()];
    let unknown = effect::to_string(&unknown, Format::Yaml).unwrap();
    let quiet = format!("{}  - kind: replace_fx\n    target: {{ effect: global_explosion_c4 }}\n    edits: src/e.yaml\n", add_fx("qm_fx_rule", "qm_rule_tpl", "qm_fx_rule"));
    assert_eq!(game_codes(&shipment("quiet", &quiet, Some(drawn.clone()), MAGENTA)), Vec::<&str>::new());

    // M0257: a retail template name, and a name whose derived key a retail template already holds.
    let s = shipment("m0257-name", &add_fx("qm_fx_rule", "global_particle_explosion_c4", "qm_fx_rule"), Some(drawn.clone()), MAGENTA);
    assert_eq!(game_codes(&s), vec!["M0257"]);
    let keys = GameFx::read(&mut game()).unwrap().worldentity.all_keys();
    let taken = (0u32..).map(|i| format!("qm_key_{i}")).find(|n| keys.contains(&derived_template_key(n))).unwrap();
    let s = shipment("m0257-key", &add_fx("qm_fx_rule", &taken, "qm_fx_rule"), Some(drawn.clone()), MAGENTA);
    assert_eq!(game_codes(&s), vec!["M0257"], "{taken} derives 0x{:08X}", derived_template_key(&taken));

    // M0258: an effect the game has.
    let s = shipment("m0258", &add_fx("global_explosion_c4", "qm_rule_tpl", "global_explosion_c4"), Some(drawn.clone()), MAGENTA);
    assert_eq!(game_codes(&s), vec!["M0258"]);

    // M0259: a frame that names no texture, and a template that names no effect.
    let s = shipment("m0259-frame", &add_fx("qm_fx_rule", "qm_rule_tpl", "qm_fx_rule"), Some(unknown), MAGENTA);
    assert_eq!(game_codes(&s), vec!["M0259"]);
    let s = shipment("m0259-red", &add_fx("qm_fx_rule", "qm_rule_tpl", "qm_fx_nothing"), Some(drawn.clone()), MAGENTA);
    assert_eq!(game_codes(&s), vec!["M0259"]);

    // M0260: a template that starts no effect.
    let s = shipment(
        "m0260",
        "  - kind: replace_fx\n    target: { template: fx_Explosion_HugeOil_RigOnly }\n    edits: src/e.yaml\n",
        None,
        MAGENTA,
    );
    let d = lint::fx_game_checks(&s.manifest, &s.root, &mut game()).unwrap();
    assert_eq!(d.iter().map(|d| d.rule.code).collect::<Vec<_>>(), vec!["M0260"]);
    assert!(d[0].message.contains("has 0 RedEffectComponent"), "{}", d[0].message);

    // M0261: an emitter the C4 effect does not have.
    let s = shipment(
        "m0261",
        "  - kind: replace_fx\n    target: { effect: global_explosion_c4 }\n    edits: src/e.yaml\n",
        None,
        "edits:\n  - { op: colour_rgb, emitter: 9, rgb: [255, 0, 255] }\n",
    );
    assert_eq!(game_codes(&s), vec!["M0261"]);
}
