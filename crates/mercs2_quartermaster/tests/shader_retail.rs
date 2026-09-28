//! The shader tables and the store edits against the retail game.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml` and the stores beside it, and fails if they are absent.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::shader3::Store;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use mercs2_formats::types::{TYPE_HASH_FONT, TYPE_HASH_LOWRES_TERRAIN, TYPE_HASH_MODEL, TYPE_HASH_TERRAIN_MESH};
use mercs2_formats::ucfx::{read_ucfx_rows, walk_decompressed_block};
use mercs2_quartermaster::shader::{self, DataFile, Stage};
use mercs2_quartermaster::shader_import::{self, Tables};

fn vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"))
}

fn game_data() -> PathBuf {
    vz_wad().parent().expect("vz.wad is in the data folder").to_path_buf()
}

/// `(type hash, container)` of every container in `vz.wad` that carries an `MTRL` row, and of every
/// model container (a model's finer LOD rungs carry groups and no `MTRL`).
fn mtrl_containers() -> &'static Vec<(u32, Vec<u8>)> {
    static C: OnceLock<Vec<(u32, Vec<u8>)>> = OnceLock::new();
    C.get_or_init(|| {
        let wad = vz_wad();
        let mut f = std::fs::File::open(&wad).expect("open vz.wad");
        let size = f.metadata().expect("stat vz.wad").len();
        let archive = load_ffcs_archive(&mut f, size).expect("read FFCS tables");
        let mut out = Vec::new();
        for block in 0..archive.indx.len() {
            let dec = decompress_block(&mut f, &archive.indx, block as u16).expect("decompress block");
            let (parsed, _) = walk_decompressed_block(&dec, "block");
            for (e, c) in parsed.entries.iter().zip(parsed.containers) {
                if e.type_hash == TYPE_HASH_MODEL
                    || read_ucfx_rows(&c).is_ok_and(|rows| rows.iter().any(|r| &r.tag == b"MTRL"))
                {
                    out.push((e.type_hash, c));
                }
            }
        }
        out
    })
}

fn source(type_hash: u32) -> Option<MtrlSource> {
    match type_hash {
        TYPE_HASH_MODEL => Some(MtrlSource::Model),
        TYPE_HASH_TERRAIN_MESH => Some(MtrlSource::TerrainMesh),
        TYPE_HASH_FONT => Some(MtrlSource::Font),
        TYPE_HASH_LOWRES_TERRAIN => Some(MtrlSource::LowResTerrain),
        mercs2_formats::scrub::TYPE_HASH => Some(MtrlSource::Scrub),
        _ => None,
    }
}

/// Every retail material's pixel-shader key and every model group's vertex-shader words are
/// registrations of that stage made in every configuration.
#[test]
fn every_retail_material_and_group_key_is_registered_everywhere() {
    let keys = shader::ShaderKeys::with(&[]);
    let (mut materials, mut groups) = (0usize, 0usize);
    let mut missing = BTreeSet::new();
    for (th, c) in mtrl_containers() {
        let src = source(*th).unwrap_or_else(|| panic!("MTRL in a container of type {th:#010X}"));
        for m in parse_mtrl(c, src).expect("retail MTRL parses") {
            materials += 1;
            if !keys.pixel.contains(&m.shader_key) {
                missing.insert(format!("pixel 0x{:08X}", m.shader_key));
            }
        }
        if *th == TYPE_HASH_MODEL {
            let (gs, _) = shader_import::read_model(c).expect("a retail model reads");
            for g in gs {
                groups += 1;
                for k in [g.vertex, g.shadow] {
                    if !keys.vertex.contains(&k) {
                        missing.insert(format!("vertex 0x{k:08X}"));
                    }
                }
            }
        }
    }
    eprintln!("{materials} materials, {groups} model groups");
    assert!(materials > 50_000 && groups > 50_000, "{materials} materials, {groups} groups");
    assert!(missing.is_empty(), "{missing:?}");
}

/// The committed import rules are exactly the census of the retail models, and resolving each
/// retail observation gives retail's shader or an ambiguity that names it.
#[test]
fn shader_import_census() {
    let mut tables: Tables = Default::default();
    let mut observations = Vec::new();
    for (th, c) in mtrl_containers() {
        if *th != TYPE_HASH_MODEL {
            continue;
        }
        let obs = shader_import::census_container(c).expect("a retail model reads");
        for (rule, input, shader) in obs {
            tables.entry(rule).or_default().entry(input.clone()).or_default().insert(shader.clone());
            observations.push((rule, input, shader));
        }
    }
    let committed = shader_import::rules();
    if &tables != committed {
        panic!("the committed rules differ from the census:\n{}", shader_import::to_tsv(&tables));
    }
    let mut derived = 0usize;
    for (rule, input, shader) in &observations {
        match shader_import::resolve(*rule, input, None) {
            Ok(s) => {
                assert_eq!(&s, shader, "{} {input}", rule.token());
                derived += 1;
            }
            Err(e) => assert!(e.contains(shader.as_str()), "{} {input}: {e}", rule.token()),
        }
    }
    eprintln!("{} observations, {derived} resolved without a declaration", observations.len());
}

/// The stores round-trip, and a replace plus an add change exactly the records they name.
#[test]
fn retail_stores_take_a_replace_and_an_add() {
    let data = game_data();
    let originals = shader::read_originals(&data).expect("the retail stores");
    let extra = shader::read_extra_pairs(&data).expect("the VT and R2VB pairs");
    let unchanged = shader::apply_edits(&originals, &extra, &[]).expect("no edits");
    for f in DataFile::ALL {
        assert_eq!(unchanged[&f], std::fs::read(data.join(f.file_name())).unwrap(), "{} rewrites byte-identically", f.file_name());
    }

    let high = &originals.stores[&DataFile::Shader3];
    let low = &originals.stores[&DataFile::Shader3Low];
    let id = mercs2_formats::shader3::store_id("PgMeshVP", false).unwrap();
    let rec = high.records.iter().find(|r| r.id == id).expect("PgMeshVP_3.sho");
    // The replacement: retail's own text with one more instruction.
    let text = mercs2_formats::sm3asm::disassemble(high.blob(rec)).unwrap();
    let changed_text = text.replacen("\ndcl_position o0", "\ndcl_position o0\nnop", 1);
    let dir = std::env::temp_dir().join(format!("qm_shader_retail_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/mesh.asm"), &changed_text).unwrap();
    let low_rec = low.records.iter().find(|r| r.id == mercs2_formats::shader3::store_id("PgMeshVP", true).unwrap()).unwrap();
    std::fs::write(dir.join("src/mesh_low.asm"), mercs2_formats::sm3asm::disassemble(low.blob(low_rec)).unwrap()).unwrap();
    std::fs::write(dir.join("src/new.asm"), "vs_3_0\ndcl_position v0\ndcl_position o0\nmov o0, v0\n").unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        "format: 2\nshipment: { name: retail-shaders, version: 1.0.0, target: retail }\n\
         load:\n  requires:\n    - capability: shader-registry\n\
         contributions:\n\
         \x20 - kind: replace_shader\n    target: PgMeshVP\n    shader: {asm: src/mesh.asm}\n    shader_low: {asm: src/mesh_low.asm}\n\
         \x20 - kind: add_shader\n    family: vertex\n    classes:\n\
         \x20     - { name: QmRetailTestVP, stem: QmRetailTestVP, shader: {asm: src/new.asm}, shader_low: {asm: src/new.asm} }\n",
    )
    .unwrap();
    let s = mercs2_quartermaster::discover::open(&dir).unwrap();
    assert!(
        mercs2_quartermaster::lint::shader_game_checks(&s.manifest, &s.root, &data, &data).unwrap().is_empty(),
        "retail PgMeshVP takes a replace and a new stem is free"
    );
    let edits = shader::shipment_edits("retail-shaders", &s.manifest, &s.root).expect("sources load");
    let out = shader::apply_edits(&originals, &extra, &edits).expect("applies");
    let new_high = Store::parse(out[&DataFile::Shader3].clone()).unwrap();
    assert_eq!(new_high.records.len(), high.records.len() + 1);
    for (a, b) in high.records.iter().zip(&new_high.records) {
        assert_eq!(a.id, b.id, "record order is kept");
        if a.id == id {
            assert_eq!(new_high.blob(b), mercs2_formats::sm3asm::assemble(&changed_text).unwrap().as_slice());
        } else {
            assert_eq!(high.blob(a), new_high.blob(b));
        }
    }
    let added = new_high.records.last().unwrap();
    assert_eq!(added.id, mercs2_formats::shader3::store_id("QmRetailTestVP", false).unwrap());
    assert_eq!(Stage::Vertex.kind(), added.kind);
    let new_low = Store::parse(out[&DataFile::Shader3Low].clone()).unwrap();
    assert_eq!(new_low.records.len(), low.records.len() + 1);
}

fn retail_game() -> mercs2_quartermaster::GameStack {
    mercs2_quartermaster::GameStack::open(&[vz_wad()]).expect("open the game stack")
}

/// A Shipment adding one vertex shader under `stem`, at a fresh directory.
fn adder(label: &str, stem: &str) -> mercs2_quartermaster::LoadedShipment {
    let dir = std::env::temp_dir().join(format!("qm_shader_retail_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/vs.asm"), "vs_3_0\ndcl_position v0\ndcl_position o0\nmov o0, v0\n").unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        format!(
            "format: 2\nshipment: {{ name: {label}, version: 1.0.0, target: retail }}\n\
             load:\n  requires:\n    - capability: shader-registry\n\
             contributions:\n\
             \x20 - kind: add_shader\n    family: vertex\n    classes:\n\
             \x20     - {{ name: {stem}, stem: {stem}, shader: {{asm: src/vs.asm}}, shader_low: {{asm: src/vs.asm}} }}\n"
        ),
    )
    .unwrap();
    mercs2_quartermaster::discover::open(&dir).unwrap()
}

/// `qm build` writes both stores as `data_file` placements naming the originals' sha256, and the
/// registration header.
#[test]
fn a_shader_build_writes_both_stores_and_the_header() {
    use mercs2_quartermaster::build::{self, Destination};
    let data = game_data();
    let mut game = retail_game();
    let s = adder("shader-build", "QmBuildVP");
    let out = s.root.join("_build");
    let report = build::build(&s, Some(&mut game), None, Some(&out), None, Some(&data)).expect("builds");
    let files: Vec<(DataFile, String)> = report
        .placements
        .iter()
        .filter_map(|p| match &p.destination {
            Destination::DataFile { relative, base_sha256 } => Some((*relative, base_sha256.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(files.iter().map(|(f, _)| *f).collect::<Vec<_>>(), DataFile::ALL.to_vec());
    for (f, base) in &files {
        assert_eq!(base, &build::sha256_hex(&std::fs::read(data.join(f.file_name())).unwrap()));
        let written = std::fs::read(out.join(f.relative())).unwrap();
        let store = Store::parse(written).unwrap();
        assert_eq!(store.records.last().unwrap().id, mercs2_formats::shader3::store_id("QmBuildVP", f.is_low()).unwrap());
    }
    let header = std::fs::read_to_string(out.join("shader-build.shaders.h")).expect("the header");
    assert!(header.contains("{ \"QmBuildVP\", \"QmBuildVP.sho\" }"), "{header}");
    let record = std::fs::read_to_string(out.join("placement.json")).unwrap();
    assert!(record.contains("\"kind\": \"data_file\""), "{record}");
}

/// `qm link` merges the set's edits into one store per file, emits them as `data_file`
/// placements, and the plan lists both in `link_file_paths`.
#[test]
fn link_merges_disjoint_stems_into_one_store_per_file() {
    use mercs2_quartermaster::build::{self, Destination};
    use mercs2_quartermaster::compat::PlanInput;
    let data = game_data();
    let mut game = retail_game();
    let a = adder("shader-link-a", "QmLinkAVP");
    let b = adder("shader-link-b", "QmLinkBVP");
    let out = std::env::temp_dir().join(format!("qm_shader_retail_{}_link_out", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    // The capability's provider, as the m2-sdk Shipment declares it.
    let sdk_dir = std::env::temp_dir().join(format!("qm_shader_retail_{}_sdk", std::process::id()));
    let _ = std::fs::remove_dir_all(&sdk_dir);
    std::fs::create_dir_all(&sdk_dir).unwrap();
    std::fs::write(
        sdk_dir.join("manifest.yaml"),
        "format: 2\nshipment: { name: m2-sdk, version: 1.0.0, target: retail }\nload:\n  provides: [shader-registry]\ncontributions: []\n",
    )
    .unwrap();
    let sdk = mercs2_quartermaster::discover::open(&sdk_dir).unwrap();
    let inputs = [
        PlanInput { id: "a", shipment: &a },
        PlanInput { id: "b", shipment: &b },
        PlanInput { id: "sdk", shipment: &sdk },
    ];
    let report = build::link_installed(&inputs, &mut game, &out, &out, Some(&data)).expect("links");
    assert_eq!(report.plan.link_file_paths, DataFile::ALL.to_vec());
    let stores: Vec<&build::Placement> =
        report.placements.iter().filter(|p| matches!(p.destination, Destination::DataFile { .. })).collect();
    assert_eq!(stores.len(), 2);
    let high = Store::parse(std::fs::read(out.join("data/shader3.bin")).unwrap()).unwrap();
    let ids: Vec<u32> = high.records.iter().rev().take(2).map(|r| r.id).collect();
    assert_eq!(
        ids,
        vec![
            mercs2_formats::shader3::store_id("QmLinkBVP", false).unwrap(),
            mercs2_formats::shader3::store_id("QmLinkAVP", false).unwrap()
        ]
    );
    // Without the original data a set with a shader kind does not link.
    let err = build::link_installed(&inputs, &mut game, &out, &out, None).unwrap_err();
    assert!(err.to_string().contains("--original-data"), "{err}");
}
