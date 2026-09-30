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
use mercs2_quartermaster::shader_import::{self, Rule, Tables};

fn vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"))
}

fn game_data() -> PathBuf {
    vz_wad().parent().expect("vz.wad is in the data folder").to_path_buf()
}

/// One container of `vz.wad` with the block and entry that hold it.
struct Held {
    block: u16,
    name_hash: u32,
    type_hash: u32,
    container: Vec<u8>,
}

/// The archive's tables, and every container in `vz.wad` that carries an `MTRL` row or is a model
/// container (a model's finer LOD rungs carry groups and no `MTRL`).
fn retail() -> &'static (mercs2_formats::ffcs::FfcsArchive, Vec<Held>) {
    static C: OnceLock<(mercs2_formats::ffcs::FfcsArchive, Vec<Held>)> = OnceLock::new();
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
                    out.push(Held { block: block as u16, name_hash: e.name_hash, type_hash: e.type_hash, container: c });
                }
            }
        }
        (archive, out)
    })
}

fn mtrl_containers() -> impl Iterator<Item = (u32, &'static Vec<u8>)> {
    retail().1.iter().map(|h| (h.type_hash, &h.container))
}

/// Every model container, each with its resident container when it is a finer LOD rung: a model
/// ASET row's `lod_chain` names the resident block first and the rung blocks after it, and each
/// block holds the model under the row's asset hash. A model container no row names is its own
/// resident.
fn models_with_residents() -> Vec<(&'static [u8], Option<&'static [u8]>)> {
    use std::collections::BTreeMap;
    let (archive, held) = retail();
    let by_block: BTreeMap<(u16, u32), &Held> =
        held.iter().filter(|h| h.type_hash == TYPE_HASH_MODEL).map(|h| ((h.block, h.name_hash), h)).collect();
    let mut resident_of: BTreeMap<(u16, u32), (u16, u32)> = BTreeMap::new();
    for row in archive.aset.iter().filter(|r| r.type_id == mercs2_formats::types::TYPE_ID_MODEL) {
        let chain = row.lod_chain();
        let base = (chain[0], row.asset_hash);
        assert!(by_block.contains_key(&base), "model ASET 0x{:08X} names block {} without it", row.asset_hash, chain[0]);
        for &b in &chain[1..] {
            let rung = (b, row.asset_hash);
            assert!(by_block.contains_key(&rung), "model ASET 0x{:08X} names LOD block {b} without it", row.asset_hash);
            if let Some(prev) = resident_of.insert(rung, base) {
                assert_eq!(prev, base, "LOD block {b} of 0x{:08X} has two resident blocks", row.asset_hash);
            }
        }
    }
    by_block
        .iter()
        .map(|(k, h)| (h.container.as_slice(), resident_of.get(k).map(|r| by_block[r].container.as_slice())))
        .collect()
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
        let src = source(th).unwrap_or_else(|| panic!("MTRL in a container of type {th:#010X}"));
        for m in parse_mtrl(c, src).expect("retail MTRL parses") {
            materials += 1;
            if !keys.pixel.contains(&m.shader_key) {
                missing.insert(format!("pixel 0x{:08X}", m.shader_key));
            }
        }
        if th == TYPE_HASH_MODEL {
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

/// The committed import rules are exactly the census of the retail models, every LOD rung read
/// against its resident container's materials, and resolving each retail observation gives
/// retail's shader or an ambiguity that names it. Every shadow input has exactly one choice.
#[test]
fn shader_import_census() {
    let mut tables: Tables = Default::default();
    let mut observations = Vec::new();
    let models = models_with_residents();
    let rungs = models.iter().filter(|(_, r)| r.is_some()).count();
    let mut from_rungs = 0usize;
    for (c, resident) in &models {
        let obs = shader_import::census_container(c, *resident).expect("a retail model reads");
        if resident.is_some() {
            from_rungs += obs.len();
        }
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
    let per_rule = |r: Rule| observations.iter().filter(|(rule, _, _)| *rule == r).count();
    eprintln!(
        "{} model containers ({rungs} LOD rungs), {} observations ({from_rungs} from LOD rungs; pixel {}, \
         vertex {}, shadow {}), {derived} resolved without a declaration",
        models.len(),
        observations.len(),
        per_rule(Rule::Pixel),
        per_rule(Rule::Vertex),
        per_rule(Rule::Shadow)
    );
    let several: Vec<String> = committed[&Rule::Shadow]
        .iter()
        .filter(|(_, c)| c.len() != 1)
        .map(|(i, c)| format!("{i}: {c:?}"))
        .collect();
    assert!(several.is_empty(), "shadow inputs with several choices: {several:?}");
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

// ── the registration sites in the unpacked exe ─────────────────────────────────────────────────

/// `FUN_0085ac90`, the registration call: `__thiscall(record, name, sho, class)`.
const REGISTER: u32 = 0x0085_AC90;

/// The unpacked exe's sections: `(name, virtual address, virtual size, file offset, file size)`.
struct Image {
    bytes: Vec<u8>,
    sections: Vec<(String, u32, u32, usize, usize)>,
}

impl Image {
    fn open() -> Image {
        let path = mercs2_formats::game_paths::local_config_unpacked_exe(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("{e}"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]) as usize;
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(&bytes[0..2], b"MZ", "{} is not a PE image", path.display());
        let pe = u32_at(0x3C) as usize;
        assert_eq!(&bytes[pe..pe + 4], b"PE\0\0");
        let count = u16_at(pe + 6);
        let optional = u16_at(pe + 20);
        let base = u32_at(pe + 24 + 28);
        let table = pe + 24 + optional;
        let sections = (0..count)
            .map(|i| {
                let s = table + 40 * i;
                let name = String::from_utf8_lossy(&bytes[s..s + 8]).trim_end_matches('\0').to_string();
                (name, base + u32_at(s + 12), u32_at(s + 8).max(u32_at(s + 16)), u32_at(s + 20) as usize, u32_at(s + 16) as usize)
            })
            .collect();
        Image { bytes, sections }
    }

    /// The file bytes from `va` to the end of its section's file data.
    fn from(&self, va: u32) -> Option<&[u8]> {
        self.sections.iter().find(|(_, s, size, _, _)| (*s..s + size).contains(&va)).and_then(|(_, s, _, raw, raw_size)| {
            let d = (va - s) as usize;
            (d < *raw_size).then(|| &self.bytes[raw + d..raw + raw_size])
        })
    }

    /// The NUL-terminated printable string at `va`, when there is one of at least three characters.
    fn string(&self, va: u32) -> Option<String> {
        let b = self.from(va)?;
        let end = b.iter().take(0x100).position(|&c| c == 0)?;
        let s = &b[..end];
        (s.len() >= 3 && s.iter().all(|c| (0x20..0x7f).contains(c))).then(|| String::from_utf8_lossy(s).into_owned())
    }

    /// Every address whose bytes are `call REGISTER`, `jmp REGISTER` (rel32) or
    /// `push REGISTER; ret`, in the code sections.
    fn register_sites(&self) -> Vec<(u32, &'static str)> {
        let mut out = Vec::new();
        for (name, va, _, raw, raw_size) in &self.sections {
            if !matches!(name.as_str(), ".text" | "Stext" | ".securom") {
                continue;
            }
            let code = &self.bytes[*raw..raw + raw_size];
            for i in 0..code.len().saturating_sub(5) {
                let at = va + i as u32;
                let rel = i32::from_le_bytes(code[i + 1..i + 5].try_into().unwrap());
                let target = at.wrapping_add(5).wrapping_add(rel as u32);
                match code[i] {
                    0xE8 if target == REGISTER => out.push((at, "call")),
                    0xE9 if target == REGISTER => out.push((at, "jmp")),
                    0x68 if code[i + 1..i + 5] == REGISTER.to_le_bytes() && code.get(i + 5) == Some(&0xC3) => {
                        out.push((at, "push/ret"))
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// The `(name, sho)` a registration site pushes: the longest straight-line run of instructions
    /// ending exactly at `site` (decoded from each back-off up to 0x80 bytes; a run stops at any
    /// instruction that is not straight-line), and in it the last `push imm32` of a string not
    /// ending in `.sho` (the name) and the last of one ending in `.sho`. `None` when no run pushes a
    /// name.
    fn pushes(&self, site: u32) -> Option<(String, Option<String>)> {
        use iced_x86::{Decoder, DecoderOptions, FlowControl, Mnemonic, OpKind};
        let mut found = None;
        for back in 1..=0x80u32 {
            let start = site - back;
            let Some(bytes) = self.from(start) else { continue };
            let mut d = Decoder::with_ip(32, &bytes[..bytes.len().min(back as usize + 16)], start as u64, DecoderOptions::NONE);
            let (mut name, mut sho, mut ok) = (None, None, false);
            while d.can_decode() {
                let ins = d.decode();
                if ins.ip() == site as u64 {
                    ok = true;
                    break;
                }
                if ins.is_invalid() || ins.next_ip() > site as u64 || ins.flow_control() != FlowControl::Next {
                    break;
                }
                if ins.mnemonic() == Mnemonic::Push && ins.op0_kind() == OpKind::Immediate32 {
                    if let Some(s) = self.string(ins.immediate32()) {
                        if s.to_ascii_lowercase().ends_with(".sho") {
                            sho = Some(s);
                        } else {
                            name = Some(s);
                        }
                    }
                }
            }
            if ok {
                if let Some(n) = name {
                    found = Some((n, sho));
                }
            }
        }
        found
    }

    /// Whether some `push <name>` in the code reaches a registration site going forward, through
    /// straight-line code and unconditional `jmp`s (a shared tail joins several branches, each
    /// pushing its own name, before one call).
    fn name_reaches_a_site(&self, name: &str, sites: &BTreeSet<u32>) -> bool {
        use iced_x86::{Decoder, DecoderOptions, FlowControl, Mnemonic};
        let mut needle = name.as_bytes().to_vec();
        needle.push(0);
        let mut addrs = Vec::new();
        for (_, va, _, raw, raw_size) in &self.sections {
            let data = &self.bytes[*raw..raw + raw_size];
            for (i, w) in data.windows(needle.len()).enumerate() {
                if w == needle.as_slice() && (i == 0 || data[i - 1] == 0) {
                    addrs.push(va + i as u32);
                }
            }
        }
        for (sname, va, _, raw, raw_size) in &self.sections {
            if !matches!(sname.as_str(), ".text" | "Stext" | ".securom") {
                continue;
            }
            let code = &self.bytes[*raw..raw + raw_size];
            for i in 0..code.len().saturating_sub(5) {
                if code[i] != 0x68 || !addrs.iter().any(|a| code[i + 1..i + 5] == a.to_le_bytes()) {
                    continue;
                }
                let mut ip = va + i as u32;
                for _ in 0..64 {
                    if sites.contains(&ip) {
                        return true;
                    }
                    let Some(bytes) = self.from(ip) else { break };
                    let mut d = Decoder::with_ip(32, &bytes[..bytes.len().min(16)], ip as u64, DecoderOptions::NONE);
                    let ins = d.decode();
                    if ins.is_invalid() {
                        break;
                    }
                    ip = match ins.flow_control() {
                        FlowControl::Next => ins.next_ip() as u32,
                        FlowControl::UnconditionalBranch if ins.mnemonic() == Mnemonic::Jmp => ins.near_branch32(),
                        _ => break,
                    };
                }
            }
        }
        false
    }
}

/// Every `FUN_0085ac90` site in the unpacked exe is a `registered_shaders.tsv` row, the `_li`
/// alternate of one (the same name, a `_li.sho`), or a site whose `.sho` is not a push before it
/// (a shared tail taking the `.sho` from two or three branches, or `PgCompositeFP`'s decoded
/// pointer), under a registered name; and every row's name has a site.
#[test]
fn every_registration_site_is_a_registered_row() {
    let img = Image::open();
    let rows = shader::registered();
    let names: BTreeSet<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    let sites = img.register_sites();
    let (mut as_row, mut as_li, mut no_sho) = (0usize, 0usize, Vec::new());
    let mut seen = BTreeSet::new();
    let mut bad = Vec::new();
    for &(at, how) in &sites {
        let Some((name, sho)) = img.pushes(at) else {
            bad.push(format!("{at:#010x} ({how}): no name push reaches it"));
            continue;
        };
        if !names.contains(name.as_str()) {
            bad.push(format!("{at:#010x} ({how}): {name:?} is not a registered name"));
            continue;
        }
        seen.insert(name.clone());
        match sho {
            Some(s) if rows.iter().any(|r| r.name == name && r.sho.eq_ignore_ascii_case(&s)) => as_row += 1,
            Some(s) if s.to_ascii_lowercase().ends_with("_li.sho") => as_li += 1,
            Some(s) => bad.push(format!("{at:#010x} ({how}): {name:?} with {s:?} is no row and no _li alternate")),
            None => no_sho.push(format!("{at:#010x} {name}")),
        }
    }
    let site_set: BTreeSet<u32> = sites.iter().map(|(at, _)| *at).collect();
    let joined: Vec<&str> = names.iter().copied().filter(|n| !seen.contains(*n)).collect();
    let unsited: Vec<&&str> = joined.iter().filter(|n| !img.name_reaches_a_site(n, &site_set)).collect();
    let by_kind = |k: &str| sites.iter().filter(|(_, h)| *h == k).count();
    eprintln!(
        "{} sites ({} call, {} jmp, {} push/ret): {as_row} rows, {as_li} _li alternates, {} with no .sho push {no_sho:?}; \
         {} registered names, {} of them pushed only on branches joining a site ({joined:?})",
        sites.len(),
        by_kind("call"),
        by_kind("jmp"),
        by_kind("push/ret"),
        no_sho.len(),
        names.len(),
        joined.len()
    );
    assert!(bad.is_empty(), "{} sites fail:\n{}", bad.len(), bad.join("\n"));
    assert!(unsited.is_empty(), "registered names with no site: {unsited:?}");
}

// ── TINY far-LOD hosts ─────────────────────────────────────────────────────────────────────────

/// Each `PRMG`'s index strip, in row order.
fn group_strips(container: &[u8]) -> Vec<Vec<u32>> {
    use mercs2_formats::ucfx::{parse_ucfx_tree, UcfxNode};
    fn walk(n: &UcfxNode, out: &mut Vec<Vec<u32>>) {
        if &n.tag == b"PRMG" && n.body.is_none() {
            let data = n
                .children
                .iter()
                .find(|c| &c.tag == b"IBUF")
                .and_then(|ib| ib.children.iter().find(|c| &c.tag == b"data"))
                .and_then(|d| d.body.clone())
                .unwrap_or_default();
            out.push(data.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32).collect());
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    for n in &parse_ucfx_tree(container).expect("a retail model parses") {
        walk(n, &mut out);
    }
    out
}

/// Every retail `TINY` group, imported onto itself with its own `POSITION.w` as `_TINY_SLOT` and
/// its materials' pixel shader declared, keeps its vertex shader (its intact or ruined role) and
/// its shadow shader, and the container comes back byte for byte.
#[test]
fn every_retail_tiny_group_keeps_its_role_and_its_slots() {
    use mercs2_formats::mesh_import::CustomAttributes;
    use mercs2_quartermaster::shader_import::{Declared, Geometry, PositionW};
    let (mut groups_seen, mut ruined) = (0usize, 0usize);
    for h in retail().1.iter().filter(|h| h.type_hash == TYPE_HASH_MODEL) {
        let (groups, materials) = shader_import::read_model(&h.container).expect("a retail model reads");
        if !groups.iter().any(|g| g.kind == "TINY") {
            continue;
        }
        let mut block = Vec::new();
        for v in [1u32, h.name_hash, h.type_hash, 0, h.container.len() as u32] {
            block.extend_from_slice(&v.to_le_bytes());
        }
        block.extend_from_slice(&h.container);
        let slots = shader_import::tiny_slots(&block).expect("a TINY container lists its ids");
        let strips = group_strips(&h.container);
        for g in groups.iter().filter(|g| g.kind == "TINY") {
            let (off, ty) = g.position.expect("a TINY group has a POSITION");
            assert_eq!(ty, 16, "a TINY POSITION is FLOAT16_4");
            let w: Vec<u32> = (0..g.vertex_count)
                .map(|i| mercs2_formats::model_inject::read_f16_le(&h.container, g.stream_at + i * g.stride + off + 6) as u32)
                .collect();
            let pixels: BTreeSet<&str> =
                g.materials.iter().map(|&m| shader::retail_name(materials[m].key).expect("a retail pixel shader")).collect();
            assert_eq!(pixels.len(), 1, "TINY group {} of 0x{:08X}: {pixels:?}", g.ordinal, h.name_hash);
            let declared = Declared { pixel: pixels.iter().next().map(|p| p.to_string()), vertex: None, shadow: None };
            let custom = CustomAttributes { sway: None, tiny_slot: Some(w.clone()) };
            let sources = vec![(g.ordinal, (0..g.vertex_count as u32).collect())];
            let tris = mercs2_formats::model_inject::strip_to_tris(&strips[g.ordinal]);
            let geometry = Geometry { vertex_sources: &sources, custom: &custom, source_tris: &tris, donor: &block };
            let mut out = block.clone();
            let done = shader_import::import_into_block(&mut out, g.ordinal, &declared, &[], &Default::default(), geometry)
                .unwrap_or_else(|e| panic!("TINY group {} of 0x{:08X}: [{}] {}", g.ordinal, h.name_hash, e.code, e.message));
            assert_eq!(Some(done.vertex.as_str()), shader::retail_name(g.vertex));
            assert_eq!(Some(done.shadow.as_str()), shader::retail_name(g.shadow));
            assert_eq!(done.position_w, PositionW::TinySlot);
            assert!(w.iter().all(|&s| s < slots));
            assert!(out == block, "TINY group {} of 0x{:08X} does not come back byte for byte", g.ordinal, h.name_hash);
            groups_seen += 1;
            ruined += usize::from(done.vertex.ends_with("_Ruin"));
        }
    }
    eprintln!("{groups_seen} TINY groups ({ruined} ruined) keep their role and slots");
    assert!(groups_seen > 2000 && ruined > 1000, "{groups_seen} TINY groups, {ruined} ruined");
}
