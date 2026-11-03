//! Every retail model material's pixel shader, resolved through the retail registry and shader store.
//!
//! A material names its pixel shader by key; `mercs2_formats::shader_registrations` maps the key to
//! its `.sho` record and `PixelSamplers` reads the samplers that record declares, which is what the
//! renderer binds the material's textures by.
//!
//! Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
//! `.mercs2-local.toml` and the `shader3.bin` / `shader3Low.bin` beside it, and fails when they are
//! absent.

use std::path::PathBuf;

use mercs2_engine::wad;
use mercs2_formats::shader3::Store;
use mercs2_formats::shader_registrations::PixelSamplers;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};

fn vz_wad_path() -> PathBuf {
    let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"))
}

/// The retail store `name` from the game's data folder (beside `vz.wad`).
fn store(name: &str) -> Store {
    let path = vz_wad_path().with_file_name(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    Store::parse(bytes).unwrap_or_else(|e| panic!("{}: {e:?}", path.display()))
}

/// Both stores carry a record for every pixel registration but `PgCompositeFP`, whose record is only in
/// shader3Low.bin (shader_store_format.md §4: its shader3.bin id 0xc2a01365 is absent), and every pixel
/// shader a retail model material names declares the same samplers in both, so the material slot roles
/// do not depend on ShaderLevel. (Post-process shaders may differ: `PgToneMappingFP`'s low record drops
/// `blurMap2` and `depthMap`.)
#[test]
fn material_shaders_declare_the_same_samplers_in_both_stores() {
    use std::collections::BTreeSet;
    let high = PixelSamplers::from_store(&store("shader3.bin"), false).unwrap_or_else(|e| panic!("{e}"));
    let low = PixelSamplers::from_store(&store("shader3Low.bin"), true).unwrap_or_else(|e| panic!("{e}"));
    let high_absent: Vec<&str> = high.absent().collect();
    assert_eq!(high_absent.len(), 1, "{high_absent:#?}");
    assert!(high_absent[0].starts_with("PgCompositeFP "), "{high_absent:#?}");
    assert_eq!(low.absent().count(), 0, "{:#?}", low.absent().collect::<Vec<_>>());

    let path = vz_wad_path();
    let mut w = wad::open(path.to_str().expect("UTF-8 vz.wad path")).unwrap_or_else(|e| panic!("{e}"));
    let mut keys = BTreeSet::new();
    wad::for_each_model_hier(&mut w, |_, _| {}, |hash, read| {
        let (container, _) = read.unwrap_or_else(|e| panic!("0x{hash:08X}: {e}"));
        for m in parse_mtrl(container, MtrlSource::Model).unwrap_or_else(|e| panic!("0x{hash:08X}: {e:?}")) {
            keys.insert(m.shader_key);
        }
    });
    for &k in &keys {
        assert_eq!(high.samplers(k).unwrap(), low.samplers(k).unwrap(), "0x{k:08X}");
    }
    assert!(keys.len() > 20, "{} material shaders", keys.len());
}

#[test]
fn every_model_material_names_a_registered_pixel_shader() {
    let samplers = PixelSamplers::from_store(&store("shader3.bin"), false).unwrap_or_else(|e| panic!("{e}"));
    let path = vz_wad_path();
    let mut w = wad::open(path.to_str().expect("UTF-8 vz.wad path")).unwrap_or_else(|e| panic!("{e}"));
    let (mut materials, mut failures) = (0usize, Vec::new());
    wad::for_each_model_hier(&mut w, |_, _| {}, |hash, read| {
        let (container, _) = read.unwrap_or_else(|e| panic!("0x{hash:08X}: {e}"));
        for (i, m) in parse_mtrl(container, MtrlSource::Model).unwrap_or_else(|e| panic!("0x{hash:08X}: {e:?}")).iter().enumerate() {
            materials += 1;
            if let Err(e) = samplers.samplers(m.shader_key) {
                failures.push(format!("0x{hash:08X} mat{i}: {e}"));
            }
        }
    });
    assert!(failures.is_empty(), "{} of {materials} materials: {failures:?}", failures.len());
    assert!(materials > 16_000, "{materials} materials");
}

/// `FUN_00858320` binds material slot `i` to texture stage `i` (`FUN_00484280(i, …)` at `0x858390`),
/// so what a slot means is the sampler the material's pixel shader declares at stage `i`. Over every
/// retail model material, each non-empty slot lands on one of the four roles the renderer knows, or on
/// a stage its shader does not sample.
#[test]
fn every_model_material_slot_is_a_known_sampler_role() {
    use std::collections::BTreeMap;
    let samplers = PixelSamplers::from_store(&store("shader3.bin"), false).unwrap_or_else(|e| panic!("{e}"));
    let path = vz_wad_path();
    let mut w = wad::open(path.to_str().expect("UTF-8 vz.wad path")).unwrap_or_else(|e| panic!("{e}"));
    let mut roles: BTreeMap<(usize, String), usize> = BTreeMap::new();
    let mut unknown = Vec::new();
    wad::for_each_model_hier(&mut w, |_, _| {}, |hash, read| {
        let (container, _) = read.unwrap_or_else(|e| panic!("0x{hash:08X}: {e}"));
        for (i, m) in parse_mtrl(container, MtrlSource::Model).unwrap_or_else(|e| panic!("0x{hash:08X}: {e:?}")).iter().enumerate() {
            for slot in 0..m.textures.len() {
                if m.slot(slot).is_none() {
                    continue;
                }
                let role = samplers
                    .sampler_at(m.shader_key, slot as u8)
                    .unwrap_or_else(|e| panic!("0x{hash:08X} mat{i}: {e}"))
                    .unwrap_or("(not sampled)")
                    .to_string();
                if !matches!(role.as_str(), "diffuseMap" | "specularMap" | "emissiveMap" | "normalMap" | "(not sampled)") {
                    unknown.push(format!("0x{hash:08X} mat{i} slot {slot}: {role}"));
                }
                *roles.entry((slot, role)).or_default() += 1;
            }
        }
    });
    println!("slot roles over retail model materials: {roles:#?}");
    assert!(unknown.is_empty(), "{} slots with a sampler the renderer does not know: {unknown:?}", unknown.len());
    assert!(roles.get(&(1, "emissiveMap".to_string())).copied().unwrap_or(0) > 0, "{roles:#?}");
}

/// What every retail model material's pixel shader writes to `oC0.w` is one of the `OutputAlpha`
/// shapes, read off its own program, and the blended ones (blend mode 1 or 2) span the diffuse,
/// vertex-alpha and specular shapes.
#[test]
fn every_model_material_shader_has_a_known_output_alpha() {
    use std::collections::BTreeMap;
    let samplers = PixelSamplers::from_store(&store("shader3.bin"), false).unwrap_or_else(|e| panic!("{e}"));
    let path = vz_wad_path();
    let mut w = wad::open(path.to_str().expect("UTF-8 vz.wad path")).unwrap_or_else(|e| panic!("{e}"));
    let (mut all, mut blended): (BTreeMap<String, usize>, BTreeMap<String, usize>) = Default::default();
    let mut failures = Vec::new();
    wad::for_each_model_hier(&mut w, |_, _| {}, |hash, read| {
        let (container, _) = read.unwrap_or_else(|e| panic!("0x{hash:08X}: {e}"));
        for (i, m) in parse_mtrl(container, MtrlSource::Model).unwrap_or_else(|e| panic!("0x{hash:08X}: {e:?}")).iter().enumerate() {
            match samplers.output_alpha(m.shader_key) {
                Ok(a) => {
                    *all.entry(format!("{a:?}")).or_default() += 1;
                    if mercs2_formats::texture::material_blended(m.flags) {
                        *blended.entry(format!("{a:?}")).or_default() += 1;
                    }
                }
                Err(e) => failures.push(format!("0x{hash:08X} mat{i}: {e}")),
            }
        }
    });
    println!("output alpha over retail model materials: {all:#?}\nblended: {blended:#?}");
    assert!(failures.is_empty(), "{} materials: {failures:?}", failures.len());
    for shape in ["Diffuse", "DiffuseVertex", "DiffuseOrSpecular", "DiffuseVertexOrSpecular"] {
        assert!(blended.contains_key(shape), "no blended material is {shape}: {blended:#?}");
    }
    assert_eq!(blended.values().sum::<usize>(), 503 + 72, "{blended:#?}");
}
