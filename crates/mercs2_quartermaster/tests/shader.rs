//! The shader kinds, hermetically: synthetic stores stand in for the game's, so every rule gets a
//! case that fires and a case that stays quiet with no game present.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::shader3::{store_id, ShaderKind, Store, StoreBuilder};
use mercs2_formats::sm3asm;
use mercs2_quartermaster::build::{self, BuildError};
use mercs2_quartermaster::lint::{self, Diagnostic};
use mercs2_quartermaster::shader::{self, DataFile, Stage};
use mercs2_quartermaster::shader_import::{self, Declared};
use mercs2_quartermaster::{discover, LoadedShipment};

const VS_ASM: &str = "vs_3_0\ndcl_position v0\ndcl_position o0\nmov o0, v0\n";
const VS_LOW_ASM: &str = "vs_3_0\ndcl_position v0\ndcl_position o0\nmov o0, v0.xyzz\n";
const PS_ASM: &str = "ps_3_0\nmov oC0, c0\n";

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qm_shader_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("scratch");
    dir
}

fn asm(src: &str) -> Vec<u8> {
    sm3asm::assemble(src).expect("fixture assembles")
}

/// A Shipment at `dir` requiring the `shader-registry` capability when `capability`.
fn shipment(dir: &Path, capability: bool, contributions: &str) -> LoadedShipment {
    let load = if capability { "load:\n  requires:\n    - capability: shader-registry\n" } else { "" };
    std::fs::write(
        dir.join("manifest.yaml"),
        format!(
            "format: 2\nshipment: {{ name: shader-test, version: 1.0.0, target: retail }}\n{load}contributions:\n{contributions}"
        ),
    )
    .expect("write manifest");
    discover::open(dir).expect("open shipment")
}

fn add_vertex(name: &str, stem: &str) -> String {
    format!(
        "  - kind: add_shader\n    family: vertex\n    classes:\n      - {{ name: {name}, stem: {stem}, \
         shader: {{asm: src/vs.asm}}, shader_low: {{asm: src/vs_low.asm}} }}\n"
    )
}

fn replace_mesh_vp(low: bool) -> String {
    let mut s = "  - kind: replace_shader\n    target: PgMeshVP\n    shader: {asm: src/vs.asm}\n".to_string();
    if low {
        s.push_str("    shader_low: {asm: src/vs_low.asm}\n");
    }
    s
}

fn write_sources(dir: &Path) {
    std::fs::write(dir.join("src/vs.asm"), VS_ASM).unwrap();
    std::fs::write(dir.join("src/vs_low.asm"), VS_LOW_ASM).unwrap();
    std::fs::write(dir.join("src/ps.asm"), PS_ASM).unwrap();
}

/// Synthetic stores: `shader3.bin` and `shader3Low.bin` hold `PgMeshVP` (vertex) and `PgSkyFP`
/// (pixel, high only); the game's `data` folder holds a VT and an R2VB pair with one record each.
/// Returns `(original data dir, game data dir)`.
fn stores(dir: &Path) -> (PathBuf, PathBuf) {
    let original = dir.join("original");
    let game = dir.join("game_data");
    std::fs::create_dir_all(&original).unwrap();
    std::fs::create_dir_all(&game).unwrap();
    let write = |path: PathBuf, recs: &[(&str, bool, ShaderKind, &str)]| {
        let mut b = StoreBuilder::new();
        for (stem, low, kind, src) in recs {
            b.add(store_id(stem, *low).unwrap(), *kind, asm(src), &[]).unwrap();
        }
        std::fs::write(path, b.to_bytes().unwrap()).unwrap();
    };
    write(
        original.join("shader3.bin"),
        &[("PgMeshVP", false, ShaderKind::Vertex, VS_ASM), ("PgSkyFP", false, ShaderKind::Pixel, PS_ASM)],
    );
    write(original.join("shader3Low.bin"), &[("PgMeshVP", true, ShaderKind::Vertex, VS_ASM)]);
    write(game.join("shaderVT.bin"), &[("VtOnlyVP", false, ShaderKind::Vertex, VS_ASM)]);
    write(game.join("shaderVTLow.bin"), &[("VtOnlyVP", true, ShaderKind::Vertex, VS_ASM)]);
    write(game.join("shaderR2VB.bin"), &[("R2vbOnlyVP", false, ShaderKind::Vertex, VS_ASM)]);
    write(game.join("shaderR2VBLow.bin"), &[("R2vbOnlyVP", true, ShaderKind::Vertex, VS_ASM)]);
    (original, game)
}

fn codes(d: &[Diagnostic]) -> Vec<&'static str> {
    d.iter().map(|x| x.rule.code).collect()
}

fn game_checks(s: &LoadedShipment, original: &Path, game: &Path) -> Vec<Diagnostic> {
    lint::shader_game_checks(&s.manifest, &s.root, game, original).expect("the stores read")
}

#[test]
fn a_key_registered_by_rows_that_split_the_configurations_is_registered_everywhere() {
    // PgMeshNoTangentAmbientWindVP loads PgMeshVPAmbientWindNoTangent.sho with the VT pair and
    // PgMeshVPNoTangent.sho without it.
    let key = pandemic_hash_m2("PgMeshNoTangentAmbientWindVP");
    let rows: Vec<_> = shader::registered().iter().filter(|r| r.key == key).collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.configs.len() < shader::Config::ALL.len()));
    assert!(shader::retail_keys_everywhere(Stage::Vertex).contains(&key));
}

// ── M0230 ──────────────────────────────────────────────────────────────────────────────────────

#[test]
fn m0230_quiet_for_sources_that_load() {
    let dir = scratch("m0230_quiet");
    write_sources(&dir);
    let s = shipment(&dir, true, &(add_vertex("MyVP", "MyVP") + &replace_mesh_vp(true)));
    assert_eq!(codes(&lint::lint(&s.manifest, Some(&s.root), None)), Vec::<&str>::new());
}

#[test]
fn m0230_fires_for_asm_that_does_not_assemble() {
    let dir = scratch("m0230_asm");
    write_sources(&dir);
    std::fs::write(dir.join("src/vs.asm"), "vs_3_0\nnot_an_opcode r0\n").unwrap();
    let s = shipment(&dir, true, &add_vertex("MyVP", "MyVP"));
    assert!(codes(&lint::lint(&s.manifest, Some(&s.root), None)).contains(&"M0230"));
}

#[test]
fn m0230_fires_for_a_blob_the_codec_cannot_express_or_without_a_ctab() {
    let dir = scratch("m0230_blob");
    write_sources(&dir);
    // A blob with no CTAB: version token, one `mov`, end token.
    let mut no_ctab = asm(VS_ASM);
    let tokens = sm3asm::to_tokens(&no_ctab).unwrap();
    let len = ((tokens[1] >> 16) & 0x7fff) as usize;
    let mut stripped = vec![tokens[0]];
    stripped.extend_from_slice(&tokens[2 + len..]);
    no_ctab = sm3asm::to_bytes(&stripped);
    std::fs::write(dir.join("src/no_ctab.bin"), &no_ctab).unwrap();
    std::fs::write(dir.join("src/junk.bin"), [1u8, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    // The assembler writes a CTAB into every blob, so a blob without one does not round-trip.
    for (file, why) in [("src/no_ctab.bin", "different bytes"), ("src/junk.bin", "disassemble")] {
        let s = shipment(
            &dir,
            true,
            &format!("  - kind: replace_shader\n    target: PgMeshVP\n    shader: {{blob: {file}}}\n"),
        );
        let d = lint::lint(&s.manifest, Some(&s.root), None);
        let m = d.iter().find(|x| x.rule.code == "M0230").unwrap_or_else(|| panic!("{file}: {d:?}"));
        assert!(m.message.contains(why), "{}", m.message);
    }
}

#[test]
fn m0230_fires_for_a_stage_other_than_the_family() {
    let dir = scratch("m0230_stage");
    write_sources(&dir);
    std::fs::write(dir.join("src/vs.asm"), PS_ASM).unwrap();
    let s = shipment(&dir, true, &add_vertex("MyVP", "MyVP"));
    let d = lint::lint(&s.manifest, Some(&s.root), None);
    assert!(d.iter().any(|x| x.rule.code == "M0230" && x.message.contains("pixel shader")), "{d:?}");
}

#[test]
fn m0230_fires_for_classes_sharing_a_stem_with_different_bytes() {
    let dir = scratch("m0230_share");
    write_sources(&dir);
    std::fs::write(dir.join("src/ps2.asm"), "ps_3_0\nmov oC0, c1\n").unwrap();
    let class = |name: &str, src: &str| {
        format!("      - {{ name: {name}, stem: Glow, shader: {{asm: {src}}}, shader_low: {{asm: src/ps.asm}} }}\n")
    };
    let s = shipment(
        &dir,
        true,
        &format!(
            "  - kind: add_shader\n    family: pixel\n    classes:\n{}{}{}{}",
            class("GlowFP", "src/ps.asm"),
            class("GlowFP_pl", "src/ps.asm"),
            class("GlowFP_sl", "src/ps2.asm"),
            class("GlowFP_pl_sl", "src/ps.asm")
        ),
    );
    let d = lint::lint(&s.manifest, Some(&s.root), None);
    assert!(d.iter().any(|x| x.rule.code == "M0230" && x.message.contains("shares stem")), "{d:?}");
}

// ── M0231 / M0234 ──────────────────────────────────────────────────────────────────────────────

#[test]
fn m0231_fires_without_the_capability_and_is_quiet_with_it() {
    let dir = scratch("m0231");
    write_sources(&dir);
    let without = shipment(&dir, false, &add_vertex("MyVP", "MyVP"));
    assert!(codes(&lint::lint(&without.manifest, None, None)).contains(&"M0231"));
    let with = shipment(&dir, true, &add_vertex("MyVP", "MyVP"));
    assert!(!codes(&lint::lint(&with.manifest, None, None)).contains(&"M0231"));
    // replace_shader registers nothing, so it needs no capability.
    let replace = shipment(&dir, false, &replace_mesh_vp(true));
    assert!(!codes(&lint::lint(&replace.manifest, None, None)).contains(&"M0231"));
}

#[test]
fn m0234_fires_for_a_malformed_class_set() {
    let dir = scratch("m0234");
    write_sources(&dir);
    let cases = [
        // A pixel family with one class.
        "  - kind: add_shader\n    family: pixel\n    classes:\n      - { name: A, stem: A, shader: {asm: src/ps.asm}, shader_low: {asm: src/ps.asm} }\n".to_string(),
        // A vertex family with two classes.
        format!("{}      - {{ name: B, stem: B, shader: {{asm: src/vs.asm}}, shader_low: {{asm: src/vs_low.asm}} }}\n", add_vertex("A", "A")),
        // A stem that keeps its extension.
        add_vertex("A", "A.sho"),
        // Names whose keys collide (the hash folds case).
        format!(
            "  - kind: add_shader\n    family: pixel\n    classes:\n{}",
            ["GlowFP", "glowfp", "GlowFP_sl", "GlowFP_pl_sl"]
                .iter()
                .map(|n| format!("      - {{ name: {n}, stem: Glow, shader: {{asm: src/ps.asm}}, shader_low: {{asm: src/ps.asm}} }}\n"))
                .collect::<String>()
        ),
    ];
    for c in cases {
        let s = shipment(&dir, true, &c);
        assert!(codes(&lint::lint(&s.manifest, None, None)).contains(&"M0234"), "{c}");
    }
    let quiet = shipment(
        &dir,
        true,
        &format!(
            "  - kind: add_shader\n    family: pixel\n    classes:\n{}",
            ["GlowFP", "GlowFP_pl", "GlowFP_sl", "GlowFP_pl_sl"]
                .iter()
                .map(|n| format!("      - {{ name: {n}, stem: Glow, shader: {{asm: src/ps.asm}}, shader_low: {{asm: src/ps.asm}} }}\n"))
                .collect::<String>()
        ),
    );
    assert!(!codes(&lint::lint(&quiet.manifest, None, None)).contains(&"M0234"));
}

// ── the game + original checks ─────────────────────────────────────────────────────────────────

#[test]
fn game_checks_are_quiet_for_a_valid_replace_and_add() {
    let dir = scratch("game_quiet");
    write_sources(&dir);
    let (original, game) = stores(&dir);
    let s = shipment(&dir, true, &(add_vertex("MyVP", "MyVP") + &replace_mesh_vp(true)));
    assert_eq!(codes(&game_checks(&s, &original, &game)), Vec::<&str>::new());
}

#[test]
fn m0232_fires_for_a_missing_target_a_stage_mismatch_and_a_wrong_shader_low() {
    let dir = scratch("m0232");
    write_sources(&dir);
    let (original, game) = stores(&dir);
    let cases = [
        // No shader_low for a stem shader3Low.bin has.
        replace_mesh_vp(false),
        // A pixel source over a vertex record.
        "  - kind: replace_shader\n    target: PgMeshVP\n    shader: {asm: src/ps.asm}\n    shader_low: {asm: src/vs_low.asm}\n".into(),
        // A shader_low for a stem shader3Low.bin does not have.
        "  - kind: replace_shader\n    target: PgSkyFP\n    shader: {asm: src/ps.asm}\n    shader_low: {asm: src/ps.asm}\n".into(),
        // A stem no store and no registration has.
        "  - kind: replace_shader\n    target: NoSuchStem\n    shader: {asm: src/vs.asm}\n".into(),
    ];
    for c in cases {
        let s = shipment(&dir, false, &c);
        assert!(codes(&game_checks(&s, &original, &game)).contains(&"M0232"), "{c}");
    }
    let quiet = shipment(
        &dir,
        false,
        "  - kind: replace_shader\n    target: PgSkyFP\n    shader: {asm: src/ps.asm}\n",
    );
    assert_eq!(codes(&game_checks(&quiet, &original, &game)), Vec::<&str>::new());
}

#[test]
fn m0233_fires_for_an_id_resident_elsewhere_and_a_retail_name() {
    let dir = scratch("m0233");
    write_sources(&dir);
    let (original, game) = stores(&dir);
    for c in [add_vertex("MyVP", "VtOnlyVP"), add_vertex("MyVP", "PgMeshVP"), add_vertex("PgMeshVP", "MyVP")] {
        let s = shipment(&dir, true, &c);
        assert!(codes(&game_checks(&s, &original, &game)).contains(&"M0233"), "{c}");
    }
}

#[test]
fn m0237_fires_for_a_constant_the_family_never_binds() {
    let dir = scratch("m0237");
    write_sources(&dir);
    let (original, game) = stores(&dir);
    std::fs::write(
        dir.join("src/vs.asm"),
        "vs_3_0\n.ctab creator=\"t\" target=\"vs_3_0\" flags=0x0\n.const myTint c0 1 : vector float 1x4 [1]\n\
         .endctab\ndcl_position v0\ndcl_position o0\nadd o0, v0, c0\n",
    )
    .unwrap();
    let s = shipment(&dir, true, &add_vertex("MyVP", "MyVP"));
    let d = game_checks(&s, &original, &game);
    assert!(d.iter().any(|x| x.rule.code == "M0237" && x.message.contains("myTint")), "{d:?}");
    // A constant the vertex family binds is quiet.
    std::fs::write(
        dir.join("src/vs.asm"),
        "vs_3_0\n.ctab creator=\"t\" target=\"vs_3_0\" flags=0x0\n.const LocalToWorld c0 4 : matrix_rows float 4x4 [1]\n\
         .endctab\ndcl_position v0\ndcl_position o0\nm4x4 o0, v0, c0\n",
    )
    .unwrap();
    assert!(!codes(&game_checks(&s, &original, &game)).contains(&"M0237"));
}

#[test]
fn m0239_fires_when_the_vertex_registry_would_overflow() {
    let dir = scratch("m0239");
    write_sources(&dir);
    let (original, game) = stores(&dir);
    let room = shader::VERTEX_CAPACITY - shader::retail_count(shader::Config::ALL[5], Stage::Vertex);
    let at = |n: usize| (0..n).map(|i| add_vertex(&format!("CapVP{i}"), &format!("CapVP{i}"))).collect::<String>();
    let full = shipment(&dir, true, &at(room));
    assert!(!codes(&game_checks(&full, &original, &game)).contains(&"M0239"));
    let over = shipment(&dir, true, &at(room + 1));
    let d = game_checks(&over, &original, &game);
    assert!(d.iter().any(|x| x.rule.code == "M0239" && x.severity == lint::Severity::Hang), "{:?}", codes(&d));
}

// ── store edits ────────────────────────────────────────────────────────────────────────────────

#[test]
fn edits_replace_in_place_and_append_and_keep_the_originals_sha() {
    let dir = scratch("edits");
    write_sources(&dir);
    let (original, game) = stores(&dir);
    let s = shipment(&dir, true, &(replace_mesh_vp(true) + &add_vertex("MyVP", "MyVP")));
    let edits = shader::shipment_edits("shader-test", &s.manifest, &s.root).expect("edits");
    let originals = shader::read_originals(&original).unwrap();
    assert_eq!(
        originals.sha256[&DataFile::Shader3],
        build::sha256_hex(&std::fs::read(original.join("shader3.bin")).unwrap())
    );
    let extra = shader::read_extra_pairs(&game).unwrap();
    let out = shader::apply_edits(&originals, &extra, &edits).expect("apply");
    let high = Store::parse(out[&DataFile::Shader3].clone()).unwrap();
    let low = Store::parse(out[&DataFile::Shader3Low].clone()).unwrap();
    // The replaced record keeps its position; the added one comes last.
    assert_eq!(high.records[0].id, store_id("PgMeshVP", false).unwrap());
    assert_eq!(high.blob(&high.records[0]), asm(VS_ASM).as_slice());
    assert_eq!(low.blob(&low.records[0]), asm(VS_LOW_ASM).as_slice());
    assert_eq!(high.records.last().unwrap().id, store_id("MyVP", false).unwrap());
    assert_eq!(low.records.last().unwrap().id, store_id("MyVP", true).unwrap());
    assert_eq!(high.records.len(), 3);
    assert_eq!(low.records.len(), 2);
}

#[test]
fn two_shipments_with_disjoint_stems_merge_in_order() {
    let dir = scratch("merge");
    write_sources(&dir);
    let (original, game) = stores(&dir);
    let a = shipment(&dir.join("."), true, &add_vertex("AVP", "AVP"));
    let a_edits = shader::shipment_edits("a", &a.manifest, &a.root).unwrap();
    let b = shipment(&dir.join("."), true, &add_vertex("BVP", "BVP"));
    let b_edits = shader::shipment_edits("b", &b.manifest, &b.root).unwrap();
    let originals = shader::read_originals(&original).unwrap();
    let extra = shader::read_extra_pairs(&game).unwrap();
    let edits: Vec<_> = a_edits.into_iter().chain(b_edits).collect();
    let out = shader::apply_edits(&originals, &extra, &edits).unwrap();
    let high = Store::parse(out[&DataFile::Shader3].clone()).unwrap();
    let ids: Vec<u32> = high.records.iter().map(|r| r.id).collect();
    assert_eq!(&ids[2..], &[store_id("AVP", false).unwrap(), store_id("BVP", false).unwrap()]);
    // The same stem twice is refused.
    let twice: Vec<_> = edits.iter().chain(edits.iter()).cloned().collect();
    let e = shader::apply_edits(&originals, &extra, &twice).unwrap_err();
    assert_eq!(e.code, "M0233");
}

// ── build ──────────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_shader_kind_without_original_data_is_an_error_naming_the_flag() {
    let dir = scratch("no_original");
    write_sources(&dir);
    let s = shipment(&dir, false, &replace_mesh_vp(true));
    match build::build(&s, None, None, None, None, None) {
        Err(e @ BuildError::OriginalDataRequired { .. }) => {
            assert!(e.to_string().contains("--original-data"), "{e}");
        }
        other => panic!("expected OriginalDataRequired, got {other:?}"),
    }
}

#[test]
fn the_header_lists_exactly_the_declared_registrations() {
    let dir = scratch("header");
    write_sources(&dir);
    let s = shipment(
        &dir,
        true,
        &format!(
            "  - kind: add_shader\n    family: blur_pixel\n    classes:\n{}{}",
            ["GlowFP", "GlowFP_pl", "GlowFP_sl", "GlowFP_pl_sl"]
                .iter()
                .map(|n| format!("      - {{ name: {n}, stem: Glow, shader: {{asm: src/ps.asm}}, shader_low: {{asm: src/ps.asm}} }}\n"))
                .collect::<String>(),
            add_vertex("GlowVP", "GlowVP")
        ),
    );
    let h = shader::header("shader-test", &s.manifest).expect("a header");
    assert!(h.contains("#define SHADER_TEST_SHADER_0_FAMILY M2_SHADER_FAMILY_BLUR_PIXEL"), "{h}");
    assert!(h.contains("static const m2_shader_class SHADER_TEST_SHADER_0_CLASSES[4] = {"), "{h}");
    assert!(h.contains("{ \"GlowFP_pl_sl\", \"Glow.sho\" }, /* light class 3 */"), "{h}");
    assert!(h.contains("#define SHADER_TEST_SHADER_1_FAMILY M2_SHADER_FAMILY_VERTEX"), "{h}");
    assert!(h.contains("static const m2_shader_class SHADER_TEST_SHADER_1_CLASSES[1] = {"), "{h}");
    assert!(h.contains("#include \"m2_shader.h\""), "{h}");
    assert_eq!(h.matches("light class").count(), 5);
    let none = shipment(&dir, false, &replace_mesh_vp(true));
    assert!(shader::header("shader-test", &none.manifest).is_none());
}

// ── the add_model import on a synthetic model ──────────────────────────────────────────────────

/// A one-group static model: a 72-byte top `INFO` naming one material, `MTRL` with that material
/// (three textures, `flags`), and `GEOM → MESH → PRMG` with a 60-byte `INFO`, a `STRM` whose decl is
/// `POSITION, TEXCOORD0, NORMAL, TANGENT`, and one `PRMT` record naming material 0. Wrapped as the
/// single-entry block a lowering produces.
fn model_block(flags: u16) -> Vec<u8> {
    use mercs2_formats::ucfx::{write_ucfx_tree, UcfxNode};
    let mut top_info = vec![0u8; 72];
    top_info[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
    let mut mtrl = vec![0u8; 104];
    mtrl.extend_from_slice(&flags.to_le_bytes());
    mtrl.extend_from_slice(&3u16.to_le_bytes());
    for h in [1u32, 2, 3, 0xDEAD_BEEF, 0] {
        mtrl.extend_from_slice(&h.to_le_bytes());
    }
    let decl: Vec<u8> = [
        [0u8, 0, 0, 0, 16, 0, 0, 0],
        [0, 0, 8, 0, 15, 0, 5, 0],
        [0, 0, 12, 0, 16, 0, 3, 0],
        [0, 0, 20, 0, 16, 0, 6, 0],
        [0xff, 0, 0, 0, 17, 0, 0, 0],
    ]
    .concat();
    let mut prmt = vec![0u8; 16];
    prmt[8] = 3;
    let tree = vec![
        UcfxNode::leaf(*b"INFO", top_info),
        UcfxNode::leaf(*b"MTRL", mtrl),
        UcfxNode::marker(
            *b"GEOM",
            vec![
                UcfxNode::leaf(*b"INFO", 1u32.to_le_bytes().to_vec()),
                UcfxNode::marker(
                    *b"MESH",
                    vec![
                        UcfxNode::leaf(*b"INFO", 1u32.to_le_bytes().to_vec()),
                        UcfxNode::marker(
                            *b"PRMG",
                            vec![
                                UcfxNode::leaf(*b"INFO", vec![0u8; 60]),
                                UcfxNode::marker(
                                    *b"STRM",
                                    vec![
                                        UcfxNode::leaf(*b"info", [4u32, 28, 3].iter().flat_map(|v| v.to_le_bytes()).collect()),
                                        UcfxNode::leaf(*b"decl", decl),
                                        UcfxNode::leaf(*b"data", vec![0u8; 84]),
                                    ],
                                ),
                                UcfxNode::leaf(*b"PRMT", prmt),
                            ],
                        ),
                    ],
                ),
            ],
        ),
    ];
    let container = write_ucfx_tree(&tree);
    let mut block = Vec::new();
    for v in [1u32, 0x1234, mercs2_formats::types::TYPE_HASH_MODEL, 0, container.len() as u32] {
        block.extend_from_slice(&v.to_le_bytes());
    }
    block.extend_from_slice(&container);
    block
}

fn declared(pixel: Option<&str>, vertex: Option<&str>, shadow: Option<&str>) -> Declared {
    Declared { pixel: pixel.map(str::to_string), vertex: vertex.map(str::to_string), shadow: shadow.map(str::to_string) }
}

#[test]
fn the_import_writes_the_declared_and_derived_words() {
    let mut block = model_block(0x0088);
    let done = shader_import::import_into_block(&mut block, 0, &declared(Some("PgDiffSpecNormFP"), None, None), &[], &BTreeMap::new())
        .expect("imports");
    // The declaration names the pixel shader; with it, retail names one main and one shadow shader
    // for an alpha-tested MESH group with this declaration.
    assert_eq!(done.vertex, "PgMeshNoColorVP");
    assert_eq!(done.shadow, "PgMeshTexShadowVP");
    let container = &block[20..];
    let (groups, materials) = shader_import::read_model(container).unwrap();
    assert_eq!(groups[0].vertex, pandemic_hash_m2("PgMeshNoColorVP"));
    assert_eq!(groups[0].shadow, pandemic_hash_m2("PgMeshTexShadowVP"));
    assert_eq!(materials[0].key, pandemic_hash_m2("PgDiffSpecNormFP"));
    // The CSUM was rewritten over the new bytes.
    assert!(mercs2_formats::ucfx::parse_ucfx_tree(container).is_ok());
}

#[test]
fn the_import_asks_for_a_declaration_when_retail_is_ambiguous() {
    let mut block = model_block(0x0080);
    let e = shader_import::import_into_block(&mut block, 0, &declared(None, None, None), &[], &BTreeMap::new()).unwrap_err();
    assert_eq!(e.code, "M0235");
    assert!(e.message.contains("extras.pixel_shader") && e.message.contains("PgDiffSpecNormFP"), "{}", e.message);
    // An opaque group of this shape has two retail shadow shaders.
    let e = shader_import::import_into_block(&mut block, 0, &declared(Some("PgDiffSpecNormFP"), None, None), &[], &BTreeMap::new())
        .unwrap_err();
    assert_eq!(e.code, "M0236");
    assert!(e.message.contains("extras.shadow_vertex_shader"), "{}", e.message);
}

#[test]
fn a_declared_name_must_be_registered() {
    let mut block = model_block(0x0088);
    let e = shader_import::import_into_block(&mut block, 0, &declared(Some("NoSuchFP"), None, None), &[], &BTreeMap::new())
        .unwrap_err();
    assert_eq!(e.code, "M0235");
    let e = shader_import::import_into_block(
        &mut block,
        0,
        &declared(Some("PgDiffSpecNormFP"), Some("PgDiffSpecNormFP"), None),
        &[],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert_eq!(e.code, "M0236", "a pixel shader named as the vertex shader");
}

#[test]
fn m0238_fires_for_a_vertex_shader_reading_an_input_the_declaration_lacks() {
    let mut block = model_block(0x0088);
    // PgMeshVP reads COLOR (usage 10), which this group's declaration does not carry.
    let e = shader_import::import_into_block(
        &mut block,
        0,
        &declared(Some("PgDiffSpecNormFP"), Some("PgMeshVP"), None),
        &[],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert_eq!(e.code, "M0238");
    assert!(e.message.contains("10.0"), "{}", e.message);
}

#[test]
fn an_added_vertex_shader_is_registered_for_the_import_with_its_own_inputs() {
    let dir = scratch("import_added");
    write_sources(&dir);
    let s = shipment(&dir, true, &add_vertex("MyVP", "MyVP"));
    let added = shader::added("shader-test", &s.manifest);
    let inputs = shader_import::added_vertex_inputs(&s.manifest, &s.root).unwrap();
    assert_eq!(inputs[&pandemic_hash_m2("MyVP")], vec![vec![(0, 0)], vec![(0, 0)]]);
    let mut block = model_block(0x0088);
    let done = shader_import::import_into_block(
        &mut block,
        0,
        &declared(Some("PgDiffSpecNormFP"), Some("MyVP"), Some("PgMeshTexShadowVP")),
        &added,
        &inputs,
    )
    .expect("imports");
    assert_eq!(done.vertex, "MyVP");
}

#[test]
fn declarations_come_from_gltf_extras() {
    let dir = scratch("extras");
    let gltf = r#"{"asset":{"version":"2.0"},
"materials":[{"extras":{"pixel_shader":"PgDiffFP"}},{}],
"meshes":[{"extras":{"vertex_shader":"PgMeshVP"},"primitives":[{"attributes":{},"extras":{"shadow_vertex_shader":"PgMeshShadowVP"}}]}]}"#;
    std::fs::write(dir.join("m.gltf"), gltf).unwrap();
    assert_eq!(
        shader_import::read_declared(&dir.join("m.gltf")).unwrap(),
        declared(Some("PgDiffFP"), Some("PgMeshVP"), Some("PgMeshShadowVP"))
    );
    let two = r#"{"asset":{"version":"2.0"},"materials":[{"extras":{"pixel_shader":"PgDiffFP"}},{"extras":{"pixel_shader":"PgFastFP"}}]}"#;
    std::fs::write(dir.join("two.gltf"), two).unwrap();
    assert!(shader_import::read_declared(&dir.join("two.gltf")).unwrap_err().contains("PgFastFP"));
}

// ── the load plan ──────────────────────────────────────────────────────────────────────────────

#[test]
fn the_plan_lists_each_items_data_files_and_links_both_stores() {
    use mercs2_quartermaster::compat::{plan, PlanInput};
    use mercs2_quartermaster::plan::Producer;
    let dir = scratch("plan");
    write_sources(&dir);
    let shader_dir = dir.join("a");
    std::fs::create_dir_all(shader_dir.join("src")).unwrap();
    write_sources(&shader_dir);
    let a = shipment(&shader_dir, false, &replace_mesh_vp(true));
    let plain_dir = dir.join("b");
    std::fs::create_dir_all(&plain_dir).unwrap();
    std::fs::write(
        plain_dir.join("manifest.yaml"),
        "format: 2\nshipment: { name: plain, version: 1.0.0, target: retail }\ncontributions: []\n",
    )
    .unwrap();
    let b = discover::open(&plain_dir).unwrap();
    let p = plan(&[PlanInput { id: "a", shipment: &a }, PlanInput { id: "b", shipment: &b }], Producer::Preflight, None)
        .expect("plan");
    assert_eq!(p.format, 2);
    assert_eq!(p.items[0].data_files, vec![DataFile::Shader3, DataFile::Shader3Low]);
    assert!(p.items[1].data_files.is_empty());
    assert_eq!(p.link_file_paths, vec![DataFile::Shader3, DataFile::Shader3Low]);
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["link_file_paths"], serde_json::json!(["data/shader3.bin", "data/shader3Low.bin"]));
    let none = plan(&[PlanInput { id: "b", shipment: &b }], Producer::Preflight, None).unwrap();
    assert!(none.link_file_paths.is_empty());
}
