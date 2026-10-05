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
//! * The fixtures' effects take the retail per-position mode at every unresolved `PTYP` position, the
//!   retail mode of the `COLR` binary16, and the retail rule that a gravity's `ampl` is its
//!   magnitude.
//! * Sprites: the retail free square is 512² at (1536, 0); `qm-fx-a`'s ring joins the fxdict and is
//!   drawn there, the atlas changing only in the square's blocks; both orders of `qm-fx-a` and
//!   `qm-fx-b` give the same fxdict and atlas bytes; a frame of another Shipment's sprite needs
//!   `requires`, `qm build` leaving it to `qm link`; a sprites-only Shipment emits no effects block;
//!   M0306 and M0307 fire; a repaint of `vfx` is the base the sprites are drawn on, a repaint that
//!   fills the free square leaves the sprites M0307, and two repaints conflict.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

mod common {
    pub mod corpus;
}

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mercs2_formats::fxdict::{parse_effect_container, parse_fxdict_container, write_effect_container, EffectContainer, FxRect};
use mercs2_formats::texture::parse_texture_container;
use mercs2_formats::texture_encode::{decode_bc3_block, encode_bc3, mip_chain};
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
use mercs2_quartermaster::sprite::{self, Square};
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

/// The 2048² DXT5 body's texel `(x, y)` at mip `level`.
fn texel(body: &[u8], level: usize, x: usize, y: usize) -> [u8; 4] {
    let offset: usize = (0..level).map(|l| (2048 >> l) * (2048 >> l)).sum();
    let bw = (2048 >> level) / 4;
    let o = offset + ((y / 4) * bw + x / 4) * 16;
    decode_bc3_block(&body[o..o + 16])[(y % 4) * 4 + x % 4]
}

/// The blocks, as `(mip, bx, by)`, in which two 2048² DXT5 bodies of 10 mips differ.
fn changed_blocks(a: &[u8], b: &[u8]) -> Vec<(usize, usize, usize)> {
    assert_eq!(a.len(), b.len());
    let mut out = Vec::new();
    let mut offset = 0;
    for level in 0..10 {
        let bw = (2048 >> level) / 4;
        for by in 0..bw {
            for bx in 0..bw {
                let o = offset + (by * bw + bx) * 16;
                if a[o..o + 16] != b[o..o + 16] {
                    out.push((level, bx, by));
                }
            }
        }
        offset += bw * bw * 16;
    }
    out
}

/// Whether block `(bx, by)` of mip `level` holds a texel of `square`.
fn in_square(square: Square, level: usize, bx: usize, by: usize) -> bool {
    let (x0, y0, side) = (square.x >> level, square.y >> level, (square.side >> level).max(1));
    bx * 4 < x0 + side && x0 < bx * 4 + 4 && by * 4 < y0 + side && y0 < by * 4 + 4
}

/// Write an 8-bit RGBA PNG of `w × h` whose texel `(x, y)` is `f(x, y)`.
fn write_png(path: &Path, w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) {
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&f(x, y));
        }
    }
    let file = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&data).unwrap();
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
    let keys: std::collections::BTreeSet<u32> = base.fxdict.iter().map(|r| r.key).collect();
    let records = frames.iter().filter(|h| keys.contains(h)).count();
    let textures = frames.iter().filter(|&&h| game.has_asset(h, mercs2_formats::types::TYPE_ID_TEXTURE)).count();
    assert_eq!((frames.len(), records, textures, keys.len()), (566, 546, 0, 630));
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

    // The resident block: the game's, with the template appended to the worldentity, the ring's
    // record in the fxdict and the ring drawn into the atlas.
    let (raw, _) = &blocks[RESIDENT];
    let resident = ScriptsBlock::parse(raw).unwrap();
    let (game_resident, _) = game.block_and_rows_by_path(r"\VZ\resident_P000_Q3.block").unwrap();
    let game_resident = ScriptsBlock::parse(&game_resident).unwrap();
    let at = fx::worldentity_entry(&resident).unwrap();
    let (fxdict_at, atlas_at) = (fx::fxdict_entry(&resident).unwrap(), fx::atlas_entry(&resident).unwrap());
    for (i, (e, b)) in resident.entries.iter().zip(&game_resident.entries).enumerate() {
        if ![at, fxdict_at, atlas_at].contains(&i) {
            assert!(e.bytes == b.bytes && e.name_hash == b.name_hash, "resident entry {i} is the game's");
        }
    }
    let ring = pandemic_hash_m2("qm_fx_a_ring");
    let records = parse_fxdict_container(&resident.entries[fxdict_at].bytes).unwrap();
    assert_eq!(records.len(), 631);
    let mut want_records = base.fxdict.clone();
    want_records.push(FxRect { key: ring, u: 0.75, v: 1.0 - 64.0 / 2048.0, w: 64.0 / 2048.0, h: 64.0 / 2048.0 });
    mercs2_formats::fxdict::sort_fxdict(&mut want_records).unwrap();
    assert_eq!(records, want_records, "the game's records and the ring's, at (1536, 0) 64²");
    let built = parse_texture_container(&resident.entries[atlas_at].bytes).unwrap().all_mips;
    let game_body = &base.atlas.body;
    let square = Square { x: 1536, y: 0, side: 512 };
    for (level, bx, by) in changed_blocks(game_body, &built) {
        assert!(in_square(square, level, bx, by), "mip {level} block ({bx}, {by}) lies outside the square");
    }
    // The ring: alpha on the circle of radius 24 about its centre, none at the centre.
    assert!(texel(&built, 0, 1536 + 56, 32)[3] > 200);
    assert_eq!(texel(&built, 0, 1536 + 32, 32)[3], 0);
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
        let resident = ScriptsBlock::parse(&blocks[RESIDENT].0).unwrap();
        let fxdict = resident.entries[fx::fxdict_entry(&resident).unwrap()].bytes.clone();
        let atlas = resident.entries[fx::atlas_entry(&resident).unwrap()].bytes.clone();
        let records = parse_fxdict_container(&fxdict).unwrap();
        assert_eq!(records.len(), 632, "{label}: the game's 630 and the two sprites'");
        // The ring's key (0x10BC3021) is below the star's (0xB0EF64D6): the ring takes the free
        // square's first 64² cell, (1536, 0), and the star the next, (1600, 0).
        for (sp, x) in [("qm_fx_a_ring", 1536.0), ("qm_fx_b_star", 1600.0)] {
            let r = records.iter().find(|r| r.key == pandemic_hash_m2(sp)).unwrap_or_else(|| panic!("{label} {sp}"));
            assert_eq!((r.u * 2048.0, r.top() * 2048.0, r.w * 2048.0, r.h * 2048.0), (x, 0.0, 64.0, 64.0), "{label} {sp}");
        }
        contents.push((by_hash, templates, we.instances.clone(), fxdict, atlas));
    }
    assert!(contents[0] == contents[1], "both load orders merge the same effects, templates, fxdict and atlas bytes");
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
    // Quiet: a new effect drawing a frame of the game's fxdict, and a C4 edit.
    let mut drawn = effect::read(&fixture("qm-fx-a/src/qm_fx_cyan_burst.yaml")).unwrap();
    drawn.emitters[0].particle.frames = vec![format!("0x{:08X}", GameFx::read(&mut game()).unwrap().fxdict[0].key)];
    let drawn = effect::to_string(&drawn, Format::Yaml).unwrap();
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

/// The fixtures' effects are authored; where a value has no name to author it by, it is retail's
/// commonest: each of the 15 unresolved `PTYP` positions takes the mode of the 820 retail emitters
/// (value, flags and curve together), and every `COLR` key takes the commonest binary16 of the
/// 82,000 retail keys. A gravity's `ampl` is its magnitude, as in all 225 retail gravities.
#[test]
fn the_fixture_effects_take_retails_commonest_value_where_nothing_names_one() {
    let mut game = game();
    let base = GameFx::read(&mut game).unwrap();
    let defs: Vec<&mercs2_formats::fxdict::AttrDef> = mercs2_formats::fxdict::PTYP_ATTRIBUTES_BEFORE_COLR
        .iter()
        .chain(mercs2_formats::fxdict::PTYP_ATTRIBUTES_AFTER_COLR.iter())
        .collect();
    let mut counts: Vec<BTreeMap<String, (usize, mercs2_formats::fxdict::Atrb)>> = vec![BTreeMap::new(); defs.len()];
    let mut halves: BTreeMap<u16, usize> = BTreeMap::new();
    let (mut emitters, mut gravities, mut ampl_is_magnitude) = (0, 0, 0);
    for e in base.effects.entries.iter().filter(|e| e.type_hash == TYPE_HASH_EFFECT) {
        let fx = parse_effect_container(&e.bytes).unwrap();
        for em in &fx.emitters {
            emitters += 1;
            for (i, a) in em.particle.attributes.iter().enumerate() {
                counts[i].entry(format!("{a:?}")).or_insert((0, a.clone())).0 += 1;
            }
            for k in em.particle.colr.keys.iter() {
                *halves.entry(k.half_bits).or_default() += 1;
            }
        }
        for f in &fx.forces {
            if let mercs2_formats::fxdict::ForceKind::Gravity { magnitude, .. } = f.kind {
                gravities += 1;
                ampl_is_magnitude += usize::from(f.attributes[0].value == mercs2_formats::fxdict::AtrbValue::F32(magnitude));
            }
        }
    }
    assert_eq!((emitters, gravities, ampl_is_magnitude), (820, 225, 225));
    let (half, n) = halves.iter().max_by_key(|(_, n)| **n).map(|(h, n)| (*h, *n)).unwrap();
    assert_eq!((half, n), (0x3C00, 46_104));
    let unresolved: Vec<usize> = (0..defs.len()).filter(|&i| defs[i].name.is_none()).collect();
    assert_eq!(unresolved.len(), 15);
    for f in ["qm-fx-a/src/qm_fx_cyan_burst.yaml", "qm-fx-b/src/qm_fx_green_burst.yaml"] {
        let fx = effect::read(&fixture(f)).unwrap().lower().unwrap();
        let p = &fx.emitters[0].particle;
        for &i in &unresolved {
            let modes: Vec<&(usize, mercs2_formats::fxdict::Atrb)> = counts[i].values().collect();
            let top = modes.iter().map(|(n, _)| *n).max().unwrap();
            let at_top: Vec<_> = modes.iter().filter(|(n, _)| *n == top).collect();
            assert_eq!(at_top.len(), 1, "position {i} has one mode");
            assert_eq!(p.attributes[i], at_top[0].1, "{f}: position {i} (0x{:08X})", defs[i].hash);
        }
        assert!(p.colr.keys.iter().all(|k| k.half_bits == half), "{f}");
        for force in &fx.forces {
            if let mercs2_formats::fxdict::ForceKind::Gravity { magnitude, .. } = force.kind {
                assert_eq!(force.attributes[0].value, mercs2_formats::fxdict::AtrbValue::F32(magnitude), "{f}");
            }
        }
    }
}

#[test]
fn the_retail_free_square_is_512_at_1536_0() {
    let base = GameFx::read(&mut game()).unwrap();
    assert_eq!(sprite::free_square(&base.atlas, &base.fxdict), Some(Square { x: 1536, y: 0, side: 512 }));
}

/// A Shipment named `name` at a scratch directory: `contributions`, `files` under `src/`, and
/// `requires`.
fn shipment_named(label: &str, name: &str, requires: &[&str], contributions: &str, files: &[(&str, Vec<u8>)]) -> LoadedShipment {
    let dir = scratch(label);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for (f, bytes) in files {
        std::fs::write(dir.join("src").join(f), bytes).unwrap();
    }
    let load = if requires.is_empty() { String::new() } else { format!("load: {{ requires: [{}] }}\n", requires.join(", ")) };
    std::fs::write(
        dir.join("manifest.yaml"),
        format!("format: 2\nshipment: {{ name: {name}, version: 1.0.0, target: retail }}\n{load}contributions:\n{contributions}"),
    )
    .unwrap();
    open(&dir)
}

/// An `add_fx` of `qm_fx_user` whose effect draws `frame`, with the template `qm_user_tpl`.
fn user_of(frame: &str) -> (String, Vec<u8>) {
    let mut form = effect::read(&fixture("qm-fx-a/src/qm_fx_cyan_burst.yaml")).unwrap();
    form.emitters[0].particle.frames = vec![frame.into()];
    (
        add_fx("qm_fx_user", "qm_user_tpl", "qm_fx_user"),
        effect::to_string(&form, Format::Yaml).unwrap().into_bytes(),
    )
}

#[test]
fn a_frame_of_another_shipments_sprite_needs_requires_and_build_leaves_it_to_link() {
    let a = open(&fixture("qm-fx-a"));
    let (contribution, text) = user_of("qm_fx_a_ring");
    let c = shipment_named("user-free", "qm-fx-user", &[], &contribution, &[("fx.yaml", text.clone())]);
    let mut g = game();
    let ids = ["shipment:qm-fx-a".to_string(), "shipment:qm-fx-user".to_string()];
    let inputs = [PlanInput { id: &ids[0], shipment: &a }, PlanInput { id: &ids[1], shipment: &c }];
    let corpus = common::corpus::corpus_root().expect("the vendored Lua corpus");
    let e = build::link_installed(&inputs, &mut g, &corpus, &scratch("user-free-link"), None).err().expect("refused");
    let message = e.to_string();
    assert!(message.contains("[M0259]") && message.contains("does not require"), "{message}");

    let c = shipment_named("user-req", "qm-fx-user", &["qm-fx-a"], &contribution, &[("fx.yaml", text)]);
    let rep = build::build(&c, Some(&mut game()), None, Some(&scratch("user-req-build")), None, None).unwrap_or_else(|e| panic!("{e}"));
    assert!(rep.log.iter().any(|l| l.contains("resolved by qm link")), "{:?}", rep.log);
    let (blocks, _) = link("user-req-link", &[&a, &c]);
    assert!(blocks.contains_key(fx::EFFECTS_BLOCK.1));
}

#[test]
fn a_sprites_only_shipment_emits_no_effects_block() {
    let ring = std::fs::read(fixture("qm-fx-a/src/qm_fx_a_ring.png")).unwrap();
    let s = shipment_named(
        "sprites-only",
        "qm-fx-sprites",
        &[],
        "  - kind: add_fx_sprite\n    name: qm_only_ring\n    image: src/ring.png\n",
        &[("ring.png", ring)],
    );
    let rep = build::build(&s, Some(&mut game()), None, Some(&scratch("sprites-only-build")), None, None).unwrap_or_else(|e| panic!("{e}"));
    let blocks = blocks_of(rep.wad.as_ref().expect("an overlay"));
    assert_eq!(blocks.keys().collect::<Vec<_>>(), vec![RESIDENT]);
    let (blocks, promised) = link("sprites-only-link", &[&s]);
    assert_eq!(blocks.keys().collect::<Vec<_>>(), vec![RESIDENT]);
    assert!(!promised.iter().any(|p| p == fx::EFFECTS_BLOCK.1), "{promised:?}");
}

#[test]
fn the_sprite_game_rules_fire_on_what_they_describe() {
    let base = GameFx::read(&mut game()).unwrap();
    // M0306: a name hashing to a record of the game's fxdict.
    let keys: std::collections::BTreeSet<u32> = base.fxdict.iter().map(|r| r.key).collect();
    let taken = (0u64..).map(|i| format!("qm_frame_{i}")).find(|n| keys.contains(&pandemic_hash_m2(n))).unwrap();
    let ring = std::fs::read(fixture("qm-fx-a/src/qm_fx_a_ring.png")).unwrap();
    let s = shipment_named(
        "m0306",
        "qm-fx-m0306",
        &[],
        &format!("  - kind: add_fx_sprite\n    name: {taken}\n    image: src/ring.png\n"),
        &[("ring.png", ring)],
    );
    let codes = |s: &LoadedShipment| lint::fx_game_checks(&s.manifest, &s.root, &mut game()).unwrap();
    let d = codes(&s);
    assert_eq!(d.iter().map(|d| d.rule.code).collect::<Vec<_>>(), vec!["M0306"], "{taken}");

    // M0307: two 512² sprites need twice the free square.
    let dir = scratch("m0307-images");
    write_png(&dir.join("big.png"), 512, 512, |_, _| [255, 255, 255, 255]);
    let big = std::fs::read(dir.join("big.png")).unwrap();
    let s = shipment_named(
        "m0307",
        "qm-fx-m0307",
        &[],
        "  - kind: add_fx_sprite\n    name: qm_big_one\n    image: src/big.png\n  - kind: add_fx_sprite\n    name: qm_big_two\n    image: src/big.png\n",
        &[("big.png", big)],
    );
    let d = codes(&s);
    assert_eq!(d.iter().map(|d| d.rule.code).collect::<Vec<_>>(), vec!["M0307", "M0307"]);
    assert!(d[0].message.contains("need 524288 texels") && d[0].message.contains("512x512 at (1536, 0)"), "{}", d[0].message);
}

/// The repaint: a gradient, opaque, but for the transparent 512² square at (1536, 0).
fn repaint_texel(x: usize, y: usize) -> [u8; 4] {
    if x >= 1536 && y < 512 {
        [0, 0, 0, 0]
    } else {
        [(x / 8) as u8, (y / 8) as u8, 128, 255]
    }
}

#[test]
fn a_repaint_is_the_base_the_sprites_are_drawn_on_and_two_repaints_conflict() {
    let dir = scratch("repaint-images");
    write_png(&dir.join("vfx.png"), 2048, 2048, repaint_texel);
    let painted = std::fs::read(dir.join("vfx.png")).unwrap();
    let ring = std::fs::read(fixture("qm-fx-a/src/qm_fx_a_ring.png")).unwrap();
    let s = shipment_named(
        "repaint",
        "qm-fx-repaint",
        &[],
        "  - kind: replace_texture\n    target: vfx\n    image: src/vfx.png\n  - kind: add_fx_sprite\n    name: qm_painted_ring\n    image: src/ring.png\n",
        &[("vfx.png", painted.clone()), ("ring.png", ring)],
    );
    let rep = build::build(&s, Some(&mut game()), None, Some(&scratch("repaint-build")), None, None).unwrap_or_else(|e| panic!("{e}"));
    let blocks = blocks_of(rep.wad.as_ref().expect("an overlay"));
    assert_eq!(blocks.keys().collect::<Vec<_>>(), vec![RESIDENT], "the repaint goes into the resident block");
    let resident = ScriptsBlock::parse(&blocks[RESIDENT].0).unwrap();
    let built = parse_texture_container(&resident.entries[fx::atlas_entry(&resident).unwrap()].bytes).unwrap().all_mips;
    let px: Vec<f32> = (0..2048 * 2048).flat_map(|i| repaint_texel(i % 2048, i / 2048)).map(|v| v as f32).collect();
    let painted_body = mip_chain(2048, 2048, 4, &px, encode_bc3);
    let square = Square { x: 1536, y: 0, side: 512 };
    let changed = changed_blocks(&painted_body, &built);
    assert!(!changed.is_empty(), "the ring is drawn");
    for (level, bx, by) in changed {
        assert!(in_square(square, level, bx, by), "mip {level} block ({bx}, {by}) differs from the repaint outside the square");
    }
    assert!(texel(&built, 0, 1536 + 56, 32)[3] > 200, "the ring sits on the repaint");
    assert_eq!(texel(&built, 0, 0, 0), texel(&painted_body, 0, 0, 0), "outside the square, the repaint's texels");
    assert_eq!(texel(&built, 0, 0, 0)[3], 255);

    // A second Shipment repainting the atlas conflicts with the first.
    let t = shipment_named(
        "repaint-two",
        "qm-fx-repaint-two",
        &[],
        "  - kind: replace_texture\n    target: \"0x89E211AF\"\n    image: src/vfx.png\n",
        &[("vfx.png", painted)],
    );
    let mut g = game();
    let ids = ["shipment:qm-fx-repaint".to_string(), "shipment:qm-fx-repaint-two".to_string()];
    let inputs = [PlanInput { id: &ids[0], shipment: &s }, PlanInput { id: &ids[1], shipment: &t }];
    let corpus = common::corpus::corpus_root().expect("the vendored Lua corpus");
    match build::link_installed(&inputs, &mut g, &corpus, &scratch("repaint-two-link"), None) {
        Err(build::BuildError::Plan(plan)) => assert!(
            plan.findings.iter().any(|f| f.code == "M0207" && f.message.contains("the repaint of the vfx atlas")),
            "{:?}",
            plan.findings
        ),
        other => panic!("two repaints link: {:?}", other.map(|r| r.log)),
    }
}

#[test]
fn a_repaint_that_fills_the_free_square_leaves_the_sprites_m0307() {
    let dir = scratch("repaint-full-images");
    write_png(&dir.join("vfx.png"), 2048, 2048, |x, y| [(x / 8) as u8, (y / 8) as u8, 128, 255]);
    let painted = std::fs::read(dir.join("vfx.png")).unwrap();
    let ring = std::fs::read(fixture("qm-fx-a/src/qm_fx_a_ring.png")).unwrap();
    let s = shipment_named(
        "repaint-full",
        "qm-fx-repaint-full",
        &[],
        "  - kind: replace_texture\n    target: vfx\n    image: src/vfx.png\n  - kind: add_fx_sprite\n    name: qm_lost_ring\n    image: src/ring.png\n",
        &[("vfx.png", painted), ("ring.png", ring)],
    );
    let d = lint::fx_game_checks(&s.manifest, &s.root, &mut game()).unwrap();
    assert_eq!(d.iter().map(|d| d.rule.code).collect::<Vec<_>>(), vec!["M0307"]);
    assert!(d[0].message.contains("has no free square"), "{}", d[0].message);
    assert_eq!(d[0].at, Some(1));
}
