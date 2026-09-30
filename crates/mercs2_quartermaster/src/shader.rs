//! The shader kinds: `replace_shader` edits a shipped shader's bytecode in the stores, and
//! `add_shader` adds new store records plus the registration tables an author's ASI hands the
//! m2-sdk `shader-registry` API.
//!
//! Two committed tables, written by `tools/extract_shader_registry.py` from the unpacked exe and
//! the retail stores, carry what the engine does:
//!
//! * `data/shader_families.tsv` — one row per shader record vtable: its stage (the load handler at
//!   vtable `+8` is `FUN_0085af00` for a vertex shader, `FUN_0085b1a0` for a pixel shader), its
//!   record size, and the constant names its binder (vtable `+0x10`) resolves.
//! * `data/registered_shaders.tsv` — every registration `FUN_0084f130` and its sub-registrars make,
//!   with the configurations (resident store pair × ShaderLevel) it is made in, plus `PgCompositeFP`,
//!   which the composite pass constructor registers outside `FUN_0084f130` (site `0x0246A443`; its
//!   class argument comes from a SecuROM call, so the row's class is `-`). A vertex shader's row
//!   also lists, per store holding its record, the inputs and the CTAB constants the record declares.
//!
//! The stores themselves are read only from the `--original-data` directory, never from the game
//! folder: the game's copy is whatever the last deploy left there.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::shader3::{self, ShaderKind, Store, StoreBuilder};
use mercs2_formats::sm3asm;
use serde::{Deserialize, Serialize};

use crate::manifest::{Contribution, Manifest, ShaderSource};

/// The capability an `add_shader` Shipment requires: the m2-sdk registers the shaders at runtime.
pub const CAPABILITY: &str = "shader-registry";

/// Names the pixel registry holds: `FUN_0085ab70` indexes `0x0197ba40[0x800]` and its insert
/// (`FUN_0085b7c0`, a linear probe for a free slot, `& 0x7ff`) never gives up on a full table.
pub const PIXEL_CAPACITY: usize = 0x800;
/// Names the vertex registry holds: `FUN_0085abd0` indexes `0x0197e248[0x100]` and its insert
/// (`FUN_00632250`, a linear probe for a free slot, `& 0xff`) never gives up on a full table.
pub const VERTEX_CAPACITY: usize = 0x100;

/// The vertex-family constant an AmbientWind shader reads: `PgMeshVPAmbientWind_3.sho` builds its
/// sway from `WindMatrix` and scales it by `POSITION.w` (`mad r0, v0.w, r0, r2`), so a group drawn
/// by a shader declaring it takes its per-vertex sway weight from `POSITION.w`.
pub const WIND_CONSTANT: &str = "WindMatrix";

const FAMILIES_TSV: &str = include_str!("../data/shader_families.tsv");
const REGISTERED_TSV: &str = include_str!("../data/registered_shaders.tsv");

/// Which registry a shader is in, and which D3D create call its store record takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Vertex,
    Pixel,
}

impl Stage {
    pub fn kind(self) -> ShaderKind {
        match self {
            Stage::Vertex => ShaderKind::Vertex,
            Stage::Pixel => ShaderKind::Pixel,
        }
    }

    pub fn token(self) -> &'static str {
        match self {
            Stage::Vertex => "vertex",
            Stage::Pixel => "pixel",
        }
    }

    fn from_kind(kind: ShaderKind) -> Stage {
        match kind {
            ShaderKind::Vertex => Stage::Vertex,
            ShaderKind::Pixel => Stage::Pixel,
        }
    }

    /// The stage a blob's version token names.
    pub fn of_version(token: u32) -> Option<Stage> {
        match token {
            sm3asm::VS_3_0 => Some(Stage::Vertex),
            sm3asm::PS_3_0 => Some(Stage::Pixel),
            _ => None,
        }
    }

    fn capacity(self) -> usize {
        match self {
            Stage::Vertex => VERTEX_CAPACITY,
            Stage::Pixel => PIXEL_CAPACITY,
        }
    }
}

/// The shader record families: one per record vtable in the retail exe. The declaration order is
/// the row order of `data/shader_families.tsv`, and the m2-sdk's `m2_shader_family` enumerator
/// values are these positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderFamily {
    Pixel,
    AdaptiveLuminancePixel,
    AntiAliasingPixel,
    BillboardTreePixel,
    BlobShadowPixel,
    BlurPixel,
    CloudGenPixel,
    CloudRenderPixel,
    CompositePixel,
    DecalPixel,
    DownSamplePixel,
    FxPixel,
    HdrFlarePixel,
    MotionBlurPixel,
    RainPixel,
    RibbonPixel,
    ScaleformCxformPixel,
    ScaleformSolidColorPixel,
    ScaleformStripPixel,
    ScaleformTextTexturePixel,
    ShimmerPixel,
    SkyPixel,
    ToneMappingPixel,
    WaterHeightMapPixel,
    WaterPixel,
    WaterWakePixel,
    Vertex,
    BillboardTreeInstanceVertex,
    BillboardTreeVertex,
    BlobShadowVertex,
    CloudRenderVertex,
    DecalVertex,
    FxVertex,
    MeshCombinerVertex,
    RainVertex,
    RibbonVertex,
    RoadVertex,
    ScaleformGlyphVertex,
    ScaleformStripVertex,
    ScrubVertex,
    SkyVertex,
    SunVertex,
    TerrainMeshVertex,
    WaterVertex,
    WaterWakeVertex,
}

/// One row of `data/shader_families.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyInfo {
    pub name: String,
    pub stage: Stage,
    pub vtable: u32,
    pub size: u32,
    /// The constant names the family's binder resolves (`FUN_0085ac40` per name): the only
    /// constants the engine sets for a shader of this family.
    pub constants: Vec<String>,
}

impl ShaderFamily {
    pub const ALL: [ShaderFamily; 45] = [
        ShaderFamily::Pixel, ShaderFamily::AdaptiveLuminancePixel, ShaderFamily::AntiAliasingPixel,
        ShaderFamily::BillboardTreePixel, ShaderFamily::BlobShadowPixel, ShaderFamily::BlurPixel,
        ShaderFamily::CloudGenPixel, ShaderFamily::CloudRenderPixel, ShaderFamily::CompositePixel,
        ShaderFamily::DecalPixel, ShaderFamily::DownSamplePixel, ShaderFamily::FxPixel,
        ShaderFamily::HdrFlarePixel, ShaderFamily::MotionBlurPixel, ShaderFamily::RainPixel,
        ShaderFamily::RibbonPixel, ShaderFamily::ScaleformCxformPixel,
        ShaderFamily::ScaleformSolidColorPixel, ShaderFamily::ScaleformStripPixel,
        ShaderFamily::ScaleformTextTexturePixel, ShaderFamily::ShimmerPixel, ShaderFamily::SkyPixel,
        ShaderFamily::ToneMappingPixel, ShaderFamily::WaterHeightMapPixel, ShaderFamily::WaterPixel,
        ShaderFamily::WaterWakePixel, ShaderFamily::Vertex, ShaderFamily::BillboardTreeInstanceVertex,
        ShaderFamily::BillboardTreeVertex, ShaderFamily::BlobShadowVertex,
        ShaderFamily::CloudRenderVertex, ShaderFamily::DecalVertex, ShaderFamily::FxVertex,
        ShaderFamily::MeshCombinerVertex, ShaderFamily::RainVertex, ShaderFamily::RibbonVertex,
        ShaderFamily::RoadVertex, ShaderFamily::ScaleformGlyphVertex,
        ShaderFamily::ScaleformStripVertex, ShaderFamily::ScrubVertex, ShaderFamily::SkyVertex,
        ShaderFamily::SunVertex, ShaderFamily::TerrainMeshVertex, ShaderFamily::WaterVertex,
        ShaderFamily::WaterWakeVertex,
    ];

    /// The family's row in `data/shader_families.tsv`.
    pub fn info(self) -> &'static FamilyInfo {
        &families()[self as usize]
    }

    pub fn name(self) -> &'static str {
        &self.info().name
    }

    pub fn stage(self) -> Stage {
        self.info().stage
    }

    /// Registrations a family takes per `add_shader`: the four light classes of a pixel shader, one
    /// vertex shader.
    pub fn class_count(self) -> usize {
        match self.stage() {
            Stage::Pixel => 4,
            Stage::Vertex => 1,
        }
    }

    /// The m2-sdk enumerator (`M2_SHADER_FAMILY_<NAME>`).
    pub fn c_enumerator(self) -> String {
        format!("M2_SHADER_FAMILY_{}", self.name().to_ascii_uppercase())
    }
}

/// Every family, in `data/shader_families.tsv` row order.
pub fn families() -> &'static [FamilyInfo] {
    static F: OnceLock<Vec<FamilyInfo>> = OnceLock::new();
    F.get_or_init(|| {
        let rows: Vec<FamilyInfo> = tsv_rows(FAMILIES_TSV, &["family", "stage", "vtable", "size", "constants"])
            .map(|f| FamilyInfo {
                name: f[0].to_string(),
                stage: parse_stage(f[1]),
                vtable: parse_hex(f[2]),
                size: parse_hex(f[3]),
                constants: f[4].split(',').map(str::to_string).collect(),
            })
            .collect();
        if rows.len() != ShaderFamily::ALL.len() {
            panic!(
                "data/shader_families.tsv has {} families; ShaderFamily has {}",
                rows.len(),
                ShaderFamily::ALL.len()
            );
        }
        rows
    })
}

/// A resident configuration of the registry: which extra store pair the caps word loads
/// (`DAT_01176288 + 0x5e4` bit 2: the VT pair; bit 2 clear and bit 3 set: the R2VB pair; neither:
/// none) and the ShaderLevel byte `DAT_00dfc345`, which gates the `_pl`/`_sl`/`_pl_sl`
/// registrations and selects the `_3.sho` or `_3l.sho` store records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Config {
    pub extra: ExtraStores,
    pub shader_level: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtraStores {
    None,
    R2vb,
    Vt,
}

impl Config {
    pub const ALL: [Config; 6] = [
        Config { extra: ExtraStores::None, shader_level: false },
        Config { extra: ExtraStores::None, shader_level: true },
        Config { extra: ExtraStores::R2vb, shader_level: false },
        Config { extra: ExtraStores::R2vb, shader_level: true },
        Config { extra: ExtraStores::Vt, shader_level: false },
        Config { extra: ExtraStores::Vt, shader_level: true },
    ];

    /// The `registered_shaders.tsv` spelling: `none0` … `vt1`.
    pub fn token(self) -> String {
        let e = match self.extra {
            ExtraStores::None => "none",
            ExtraStores::R2vb => "r2vb",
            ExtraStores::Vt => "vt",
        };
        format!("{e}{}", u8::from(self.shader_level))
    }

    fn parse(s: &str) -> Config {
        *Config::ALL
            .iter()
            .find(|c| c.token() == s)
            .unwrap_or_else(|| panic!("data/registered_shaders.tsv names configuration {s:?}"))
    }

    /// Whether a registration of light class `class` is made: class 0 always, classes 1-3 only with
    /// ShaderLevel on (`FUN_0084f130`'s `DAT_00dfc345 != 0` blocks).
    pub fn registers_class(self, class: u32) -> bool {
        class == 0 || self.shader_level
    }
}

/// One row of `data/registered_shaders.tsv`: a registration the retail registry makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    pub name: String,
    pub key: u32,
    pub sho: String,
    pub stage: Stage,
    pub family: String,
    /// The class argument of the registration call. `None` for `PgCompositeFP`, whose class a
    /// SecuROM call computes.
    pub class: Option<u32>,
    pub configs: BTreeSet<Config>,
    /// For a vertex shader: each store holding a record for its stem, with the `(usage, index)` of
    /// every input that record's bytecode declares.
    pub inputs: Vec<(String, Vec<(u8, u8)>)>,
    /// For a vertex shader: each store holding a record for its stem, with the constant names that
    /// record's CTAB declares.
    pub constants: Vec<(String, Vec<String>)>,
}

/// Every retail registration.
pub fn registered() -> &'static [Registered] {
    static R: OnceLock<Vec<Registered>> = OnceLock::new();
    R.get_or_init(|| {
        tsv_rows(REGISTERED_TSV, &["name", "key", "sho", "stage", "family", "class", "configs", "inputs", "constants"])
            .map(|f| Registered {
                name: f[0].to_string(),
                key: parse_hex(f[1]),
                sho: f[2].to_string(),
                stage: parse_stage(f[3]),
                family: f[4].to_string(),
                class: match f[5] {
                    "-" => None,
                    c => Some(c.parse().unwrap_or_else(|_| panic!("registered_shaders.tsv class {c:?}"))),
                },
                configs: f[6].split(',').map(Config::parse).collect(),
                inputs: parse_inputs(f[7]),
                constants: parse_constants(f[8]),
            })
            .collect()
    })
}

/// The retail keys of `stage` registered in every configuration. One key can be registered by
/// several rows, each in some configurations (`PgMeshNoTangentAmbientWindVP` loads a different
/// `.sho` with the VT pair resident), so the configurations are united per key.
pub fn retail_keys_everywhere(stage: Stage) -> BTreeSet<u32> {
    let mut by_key: BTreeMap<u32, BTreeSet<Config>> = BTreeMap::new();
    for r in registered().iter().filter(|r| r.stage == stage) {
        by_key.entry(r.key).or_default().extend(r.configs.iter().copied());
    }
    by_key.into_iter().filter(|(_, c)| c.len() == Config::ALL.len()).map(|(k, _)| k).collect()
}

/// Whether the retail vertex shader `key` declares `constant` in the CTAB of its record in any
/// store, in any configuration.
pub fn retail_vertex_declares(key: u32, constant: &str) -> bool {
    registered()
        .iter()
        .filter(|r| r.key == key && r.stage == Stage::Vertex)
        .flat_map(|r| r.constants.iter())
        .any(|(_, names)| names.iter().any(|n| n == constant))
}

/// The retail registration name for `key`, when there is one.
pub fn retail_name(key: u32) -> Option<&'static str> {
    registered().iter().find(|r| r.key == key).map(|r| r.name.as_str())
}

/// The store stems the retail registry loads for `stem`'s registrations, case-folded like the
/// engine's hash.
fn stem_of(sho: &str) -> &str {
    &sho[..sho.len() - 4]
}

/// The families of every retail registration that loads `<stem>.sho`.
pub fn retail_families_of_stem(stem: &str) -> BTreeSet<&'static str> {
    let key = pandemic_hash_m2(stem);
    registered()
        .iter()
        .filter(|r| pandemic_hash_m2(stem_of(&r.sho)) == key)
        .map(|r| r.family.as_str())
        .collect()
}

/// Retail registrations made in `config`, per stage.
pub fn retail_count(config: Config, stage: Stage) -> usize {
    let keys: BTreeSet<u32> = registered()
        .iter()
        .filter(|r| r.stage == stage && r.configs.contains(&config))
        .map(|r| r.key)
        .collect();
    keys.len()
}

fn tsv_rows<'a>(text: &'a str, header: &[&str]) -> impl Iterator<Item = Vec<&'a str>> + 'a {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
    if head != header {
        panic!("TSV header {head:?} is not {header:?}");
    }
    let n = header.len();
    lines.map(move |l| {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() != n {
            panic!("TSV row {l:?} has {} fields, not {n}", f.len());
        }
        f
    })
}

/// `-`, or `<store>=<usage>.<index>+…` per store, `;`-separated.
fn parse_inputs(s: &str) -> Vec<(String, Vec<(u8, u8)>)> {
    if s == "-" {
        return Vec::new();
    }
    s.split(';')
        .map(|entry| {
            let (store, list) = entry.split_once('=').unwrap_or_else(|| panic!("registered_shaders.tsv inputs {entry:?}"));
            let inputs = list
                .split('+')
                .filter(|x| !x.is_empty())
                .map(|ui| {
                    let (u, i) = ui.split_once('.').unwrap_or_else(|| panic!("registered_shaders.tsv input {ui:?}"));
                    (u.parse().expect("usage"), i.parse().expect("usage index"))
                })
                .collect();
            (store.to_string(), inputs)
        })
        .collect()
}

/// `-`, or `<store>=<name>+…` per store, `;`-separated.
fn parse_constants(s: &str) -> Vec<(String, Vec<String>)> {
    if s == "-" {
        return Vec::new();
    }
    s.split(';')
        .map(|entry| {
            let (store, list) = entry.split_once('=').unwrap_or_else(|| panic!("registered_shaders.tsv constants {entry:?}"));
            (store.to_string(), list.split('+').filter(|x| !x.is_empty()).map(str::to_string).collect())
        })
        .collect()
}

fn parse_hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or_else(|_| panic!("TSV hex field {s:?}"))
}

fn parse_stage(s: &str) -> Stage {
    match s {
        "vertex" => Stage::Vertex,
        "pixel" => Stage::Pixel,
        other => panic!("TSV stage {other:?}"),
    }
}

// ── data files ─────────────────────────────────────────────────────────────────────────────────

/// A game data file a Shipment edits, relative to the game folder. A closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DataFile {
    #[serde(rename = "data/shader3.bin")]
    Shader3,
    #[serde(rename = "data/shader3Low.bin")]
    Shader3Low,
}

impl DataFile {
    pub const ALL: [DataFile; 2] = [DataFile::Shader3, DataFile::Shader3Low];

    pub fn relative(self) -> &'static str {
        match self {
            DataFile::Shader3 => "data/shader3.bin",
            DataFile::Shader3Low => "data/shader3Low.bin",
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            DataFile::Shader3 => "shader3.bin",
            DataFile::Shader3Low => "shader3Low.bin",
        }
    }

    /// Whether this is the store the engine reads with ShaderLevel off (`_3l.sho` ids).
    pub fn is_low(self) -> bool {
        self == DataFile::Shader3Low
    }

    pub fn parse(relative: &str) -> Option<DataFile> {
        DataFile::ALL.into_iter().find(|d| d.relative() == relative)
    }
}

/// Whether a manifest has any shader kind: then its build, link and game-gated lint read the
/// stores from `--original-data`, and it edits both [`DataFile`]s.
pub fn has_shader_kinds(manifest: &Manifest) -> bool {
    manifest
        .contributions
        .iter()
        .any(|c| matches!(c, Contribution::AddShader { .. } | Contribution::ReplaceShader { .. }))
}

/// The data files a manifest edits: both stores when it has a shader kind.
pub fn data_files(manifest: &Manifest) -> Vec<DataFile> {
    if has_shader_kinds(manifest) {
        DataFile::ALL.to_vec()
    } else {
        Vec::new()
    }
}

/// The original stores, read from the `--original-data` directory.
#[derive(Debug)]
pub struct Originals {
    pub stores: BTreeMap<DataFile, Store>,
    /// sha256 of each original file, lowercase hex.
    pub sha256: BTreeMap<DataFile, String>,
}

/// Read `shader3.bin` and `shader3Low.bin` from the `--original-data` directory.
pub fn read_originals(dir: &Path) -> Result<Originals, String> {
    let mut stores = BTreeMap::new();
    let mut sha256 = BTreeMap::new();
    for f in DataFile::ALL {
        let path = dir.join(f.file_name());
        let bytes = std::fs::read(&path).map_err(|e| format!("--original-data {}: {e}", path.display()))?;
        sha256.insert(f, crate::build::sha256_hex(&bytes));
        let store = Store::parse(bytes).map_err(|e| format!("--original-data {}: {e}", path.display()))?;
        stores.insert(f, store);
    }
    Ok(Originals { stores, sha256 })
}

/// The VT and R2VB store pairs, from the game's `data` folder: only their ids are used, to check
/// that an edited store collides with neither.
#[derive(Debug)]
pub struct ExtraPairs {
    pub vt: [Store; 2],
    pub r2vb: [Store; 2],
}

pub fn read_extra_pairs(game_data: &Path) -> Result<ExtraPairs, String> {
    let read = |name: &str| -> Result<Store, String> {
        let path = game_data.join(name);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Store::parse(bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    Ok(ExtraPairs {
        vt: [read("shaderVT.bin")?, read("shaderVTLow.bin")?],
        r2vb: [read("shaderR2VB.bin")?, read("shaderR2VBLow.bin")?],
    })
}

/// The game's `data` folder, from the stack's `vz.wad`.
pub fn game_data_dir(game: &crate::game::GameStack) -> Result<std::path::PathBuf, String> {
    let vz = game.paths().first().map(|p| p.to_path_buf()).ok_or("the game stack is empty")?;
    Ok(crate::compat::game_root_of(&vz)?.join("data"))
}

// ── sources ────────────────────────────────────────────────────────────────────────────────────

/// A source's bytecode and the stage its version token names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bytecode {
    pub blob: Vec<u8>,
    pub stage: Stage,
    /// Top-level constant names of the CTAB, samplers excluded (the engine binds samplers by
    /// texture stage, not by name).
    pub constants: Vec<String>,
}

/// Load a shader source: assemble `asm` text with `sm3asm::assemble`; take a `blob` only when its
/// `sm3asm::disassemble` text assembles back to the same bytes. Either way the result must pass
/// `check_blob` for its version token's stage and carry a `CTAB`.
pub fn load_source(root: &Path, src: &ShaderSource) -> Result<Bytecode, String> {
    let path = root.join(src.path());
    let blob = match src {
        ShaderSource::Asm(_) => {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            sm3asm::assemble(&text).map_err(|e| format!("{}: does not assemble: {e}", src.path().display()))?
        }
        ShaderSource::Blob(_) => {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let text = sm3asm::disassemble(&bytes)
                .map_err(|e| format!("{}: does not disassemble: {e}", src.path().display()))?;
            let again = sm3asm::assemble(&text)
                .map_err(|e| format!("{}: its disassembly does not assemble: {e}", src.path().display()))?;
            if again != bytes {
                return Err(format!(
                    "{}: its disassembly assembles to different bytes, so the blob holds something \
                     the SM3 codec cannot express",
                    src.path().display()
                ));
            }
            bytes
        }
    };
    bytecode(&blob).map_err(|m| format!("{}: {m}", src.path().display()))
}

/// Check a blob and read its stage and CTAB constants.
pub fn bytecode(blob: &[u8]) -> Result<Bytecode, String> {
    let token = blob
        .get(0..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or("the blob is shorter than a version token")?;
    let stage = Stage::of_version(token)
        .ok_or_else(|| format!("version token 0x{token:08X} is neither vs_3_0 nor ps_3_0"))?;
    shader3::check_blob(stage.kind(), blob).map_err(|e| e.to_string())?;
    let ctab = sm3asm::find_ctab(blob)
        .map_err(|e| e.to_string())?
        .ok_or("the blob has no CTAB comment; the engine reads its constant table by name")?;
    let constants = ctab
        .constants
        .iter()
        .filter(|c| c.register_set != 3)
        .map(|c| c.name.clone())
        .collect();
    Ok(Bytecode { blob: blob.to_vec(), stage, constants })
}

// ── edits ──────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Replace,
    Add,
}

/// One record edit a Shipment makes to one store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub shipment: String,
    pub index: usize,
    pub file: DataFile,
    pub stem: String,
    pub op: Op,
    pub code: Bytecode,
}

impl Edit {
    pub fn id(&self) -> u32 {
        pandemic_hash_m2(&format!("{}{}", self.stem, if self.file.is_low() { "_3l.sho" } else { "_3.sho" }))
    }
}

/// A failure tied to one contribution and one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub index: usize,
    pub code: &'static str,
    pub message: String,
}

fn finding(index: usize, code: &'static str, message: String) -> Finding {
    Finding { index, code, message }
}

/// The stem refusal: a stem is a `.sho` file name without `.sho`, which the engine copies into a
/// 0x81-byte record field (`FUN_0085ac90` copies the sho name to record `+0xb`; the class word is
/// at `+0x8c`).
pub fn stem_refusal(stem: &str) -> Option<String> {
    if stem.is_empty() {
        return Some("the stem is empty".into());
    }
    if stem.to_ascii_lowercase().ends_with(".sho") {
        return Some("the stem ends in .sho; it names the file without the extension".into());
    }
    if stem.len() + 4 > 0x80 {
        return Some(format!("<stem>.sho is {} characters; the record holds 128", stem.len() + 4));
    }
    if !stem.bytes().all(|b| b.is_ascii_graphic() && b != b'/' && b != b'\\') {
        return Some("the stem has a character that is not printable ASCII, or a path separator".into());
    }
    None
}

/// Every store edit of one Shipment, in contribution order. Findings are M0230: a source that
/// does not load, a stage that disagrees with its family or target, or entries sharing a stem whose
/// sources differ.
pub fn shipment_edits(shipment: &str, manifest: &Manifest, root: &Path) -> Result<Vec<Edit>, Vec<Finding>> {
    let mut out = Vec::new();
    let mut bad = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        match c {
            Contribution::ReplaceShader { target, shader, shader_low } => {
                for (file, src) in [(DataFile::Shader3, Some(shader)), (DataFile::Shader3Low, shader_low.as_ref())] {
                    let Some(src) = src else { continue };
                    match load_source(root, src) {
                        Ok(code) => out.push(Edit {
                            shipment: shipment.to_string(),
                            index,
                            file,
                            stem: target.clone(),
                            op: Op::Replace,
                            code,
                        }),
                        Err(m) => bad.push(finding(index, "M0230", m)),
                    }
                }
            }
            Contribution::AddShader { family, classes } => {
                let mut stems: Vec<(String, Bytecode, Bytecode)> = Vec::new();
                for (ci, class) in classes.iter().enumerate() {
                    let high = load_source(root, &class.shader);
                    let low = load_source(root, &class.shader_low);
                    let (high, low) = match (high, low) {
                        (Ok(h), Ok(l)) => (h, l),
                        (h, l) => {
                            for e in [h.err(), l.err()].into_iter().flatten() {
                                bad.push(finding(index, "M0230", format!("classes[{ci}]: {e}")));
                            }
                            continue;
                        }
                    };
                    for (which, code) in [("shader", &high), ("shader_low", &low)] {
                        if code.stage != family.stage() {
                            bad.push(finding(
                                index,
                                "M0230",
                                format!(
                                    "classes[{ci}].{which} is a {} shader; family {} is a {} family",
                                    code.stage.token(),
                                    family.name(),
                                    family.stage().token()
                                ),
                            ));
                        }
                    }
                    let key = pandemic_hash_m2(&class.stem);
                    match stems.iter().find(|(s, _, _)| pandemic_hash_m2(s) == key) {
                        Some((s, h, l)) => {
                            if h.blob != high.blob || l.blob != low.blob {
                                bad.push(finding(
                                    index,
                                    "M0230",
                                    format!(
                                        "classes[{ci}] shares stem {s:?} with an earlier class but its \
                                         sources assemble to different bytes; one stem is one store record"
                                    ),
                                ));
                            }
                        }
                        None => stems.push((class.stem.clone(), high, low)),
                    }
                }
                for (stem, high, low) in stems {
                    for (file, code) in [(DataFile::Shader3, high), (DataFile::Shader3Low, low)] {
                        out.push(Edit { shipment: shipment.to_string(), index, file, stem: stem.clone(), op: Op::Add, code });
                    }
                }
            }
            _ => {}
        }
    }
    if bad.is_empty() {
        Ok(out)
    } else {
        Err(bad)
    }
}

/// A failed application of one edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError {
    /// The Shipment and contribution whose edit failed; `None` for a fault in the original stores.
    pub at: Option<(String, usize)>,
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.at {
            Some((shipment, index)) => write!(f, "[{}] {shipment} contributions[{index}]: {}", self.code, self.message),
            None => write!(f, "[{}] --original-data: {}", self.code, self.message),
        }
    }
}

/// Apply `edits`, in order, to the original stores; return the bytes of both stores.
///
/// * A replace (M0232) needs the stem's record in that store with the source's stage, and a
///   `shader_low` exactly when `shader3Low.bin` has the stem.
/// * An add (M0233) needs an id no resident store holds and no earlier edit added. Every
///   configuration's resident set — both stores plus the VT pair, both plus the R2VB pair — stays
///   below the engine's 0x1200-slot id table.
pub fn apply_edits(originals: &Originals, extra: &ExtraPairs, edits: &[Edit]) -> Result<BTreeMap<DataFile, Vec<u8>>, EditError> {
    let mut builders: BTreeMap<DataFile, StoreBuilder> =
        originals.stores.iter().map(|(f, s)| (*f, StoreBuilder::from_store(s))).collect();
    let fail = |e: &Edit, code: &'static str, message: String| EditError {
        at: Some((e.shipment.clone(), e.index)),
        code,
        message,
    };
    let extra_ids: BTreeSet<u32> = extra
        .vt
        .iter()
        .chain(extra.r2vb.iter())
        .flat_map(|s| s.records.iter().map(|r| r.id))
        .collect();
    // A replace of the high store with no low source, for a stem the low store has.
    for e in edits.iter().filter(|e| e.op == Op::Replace && e.file == DataFile::Shader3) {
        let low_id = pandemic_hash_m2(&format!("{}_3l.sho", e.stem));
        let low_has = originals.stores[&DataFile::Shader3Low].records.iter().any(|r| r.id == low_id);
        let low_given = edits.iter().any(|x| {
            x.op == Op::Replace && x.file == DataFile::Shader3Low && x.index == e.index && x.shipment == e.shipment
        });
        if low_has && !low_given {
            return Err(fail(
                e,
                "M0232",
                format!(
                    "{} has a record in shader3Low.bin ({}_3l.sho), so replace_shader needs \
                     `shader_low`: with ShaderLevel off the engine loads that record",
                    e.stem, e.stem
                ),
            ));
        }
    }
    for e in edits {
        let id = e.id();
        let b = builders.get_mut(&e.file).expect("both stores are loaded");
        match e.op {
            Op::Replace => {
                let Some(entry) = b.entries().iter().find(|x| x.id == id) else {
                    return Err(fail(
                        e,
                        "M0232",
                        format!(
                            "{} has no record {} in {}: replace_shader targets a shipped store record",
                            e.stem,
                            if e.file.is_low() { format!("{}_3l.sho", e.stem) } else { format!("{}_3.sho", e.stem) },
                            e.file.file_name()
                        ),
                    ));
                };
                let have = Stage::from_kind(entry.kind);
                if have != e.code.stage {
                    return Err(fail(
                        e,
                        "M0232",
                        format!(
                            "{} in {} is a {} shader; the source is a {} shader",
                            e.stem,
                            e.file.file_name(),
                            have.token(),
                            e.code.stage.token()
                        ),
                    ));
                }
                b.replace_in_place(id, e.code.blob.clone(), &[]).map_err(|x| fail(e, "M0230", x.to_string()))?;
            }
            Op::Add => {
                if extra_ids.contains(&id) || b.entries().iter().any(|x| x.id == id) {
                    return Err(fail(
                        e,
                        "M0233",
                        format!(
                            "stem {} gives store id 0x{id:08X} in {}, which a resident store already holds",
                            e.stem,
                            e.file.file_name()
                        ),
                    ));
                }
                b.add(id, e.code.stage.kind(), e.code.blob.clone(), &[]).map_err(|x| fail(e, "M0233", x.to_string()))?;
            }
        }
    }
    // Ids resident together: the two stores plus either extra pair.
    let high_ids: Vec<u32> = builders[&DataFile::Shader3].entries().iter().map(|x| x.id).collect();
    let low_ids: Vec<u32> = builders[&DataFile::Shader3Low].entries().iter().map(|x| x.id).collect();
    for (label, pair) in [("the VT pair", &extra.vt), ("the R2VB pair", &extra.r2vb)] {
        let mut seen = BTreeSet::new();
        let mut total = 0usize;
        for id in high_ids.iter().chain(low_ids.iter()).copied().chain(pair.iter().flat_map(|s| s.records.iter().map(|r| r.id))) {
            total += 1;
            if !seen.insert(id) {
                let message = format!("store id 0x{id:08X} is resident twice with {label}");
                return Err(match edits.iter().find(|e| e.op == Op::Add && e.id() == id) {
                    Some(culprit) => fail(culprit, "M0233", message),
                    None => originals_error(message),
                });
            }
        }
        if total >= shader3::TABLE_SLOTS {
            let message = format!(
                "the stores resident with {label} hold {total} records; the engine's id table has \
                 0x{:X} slots and its insert never gives up on a full table",
                shader3::TABLE_SLOTS
            );
            return Err(match edits.iter().rev().find(|e| e.op == Op::Add) {
                Some(last) => fail(last, "M0233", message),
                None => originals_error(format!("{message}, in the original stores")),
            });
        }
    }
    let mut out = BTreeMap::new();
    for (f, b) in builders {
        let bytes = b.to_bytes().map_err(|x| originals_error(format!("{}: {x}", f.file_name())))?;
        out.insert(f, bytes);
    }
    Ok(out)
}

/// An error in the stores `--original-data` holds, not in any edit.
fn originals_error(message: String) -> EditError {
    EditError { at: None, code: "M0233", message }
}

// ── registrations ──────────────────────────────────────────────────────────────────────────────

/// One registration an `add_shader` makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub shipment: String,
    pub index: usize,
    pub family: ShaderFamily,
    pub name: String,
    pub key: u32,
    pub stem: String,
    pub class: u32,
}

/// Every registration a manifest's `add_shader`s make, in order.
pub fn added(shipment: &str, manifest: &Manifest) -> Vec<Added> {
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        if let Contribution::AddShader { family, classes } = c {
            for (ci, class) in classes.iter().enumerate() {
                out.push(Added {
                    shipment: shipment.to_string(),
                    index,
                    family: *family,
                    name: class.name.clone(),
                    key: pandemic_hash_m2(&class.name),
                    stem: class.stem.clone(),
                    class: if family.stage() == Stage::Pixel { ci as u32 } else { 0 },
                });
            }
        }
    }
    out
}

/// Keys of `stage` registered in every configuration: retail's, plus the added registrations of
/// that stage made in every configuration (light class 0).
pub fn keys_everywhere(stage: Stage, added: &[Added]) -> BTreeSet<u32> {
    let mut keys = retail_keys_everywhere(stage);
    keys.extend(added.iter().filter(|a| a.family.stage() == stage && a.class == 0).map(|a| a.key));
    keys
}

/// The keys a material and a primitive group may name: the registrations of each stage made in
/// every configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderKeys {
    pub pixel: BTreeSet<u32>,
    pub vertex: BTreeSet<u32>,
}

impl ShaderKeys {
    /// Retail's, plus `added`.
    pub fn with(added: &[Added]) -> ShaderKeys {
        ShaderKeys { pixel: keys_everywhere(Stage::Pixel, added), vertex: keys_everywhere(Stage::Vertex, added) }
    }
}

/// M0233 name collisions and M0239 capacity, over the retail registry plus `added`.
pub fn registration_findings(added: &[Added]) -> Vec<(Added, &'static str, String)> {
    let mut out = Vec::new();
    let mut seen: BTreeMap<u32, &Added> = BTreeMap::new();
    for a in added {
        if let Some(r) = registered().iter().find(|r| r.key == a.key) {
            out.push((
                a.clone(),
                "M0233",
                format!(
                    "{:?} hashes to 0x{:08X}, the key of the retail registration {:?}; the engine keeps \
                     the first registration of a key",
                    a.name, a.key, r.name
                ),
            ));
        } else if let Some(prev) = seen.get(&a.key) {
            out.push((
                a.clone(),
                "M0233",
                format!(
                    "{:?} hashes to 0x{:08X}, the key of {:?} ({} contributions[{}])",
                    a.name, a.key, prev.name, prev.shipment, prev.index
                ),
            ));
        } else {
            seen.insert(a.key, a);
        }
    }
    for config in Config::ALL {
        for stage in [Stage::Pixel, Stage::Vertex] {
            let new: BTreeSet<u32> = added
                .iter()
                .filter(|a| a.family.stage() == stage && config.registers_class(a.class))
                .map(|a| a.key)
                .collect();
            let total = retail_count(config, stage) + new.len();
            if total > stage.capacity() {
                if let Some(last) = added.iter().rev().find(|a| a.family.stage() == stage) {
                    out.push((
                        last.clone(),
                        "M0239",
                        format!(
                            "configuration {} registers {total} {} shaders; the registry holds 0x{:X}",
                            config.token(),
                            stage.token(),
                            stage.capacity()
                        ),
                    ));
                }
            }
        }
    }
    out
}

/// M0237: an added or replaced shader's CTAB constants that its family's binder never resolves.
pub fn unbound_constants(family: &FamilyInfo, code: &Bytecode) -> Vec<String> {
    code.constants.iter().filter(|c| !family.constants.contains(c)).cloned().collect()
}

// ── the C header ───────────────────────────────────────────────────────────────────────────────

/// The C header an author's ASI includes: one `m2_shader_class` table and family per `add_shader`,
/// exactly as declared. `None` when the manifest has no `add_shader`.
pub fn header(shipment: &str, manifest: &Manifest) -> Option<String> {
    let adds: Vec<(usize, ShaderFamily, &Vec<crate::manifest::ShaderClass>)> = manifest
        .contributions
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            Contribution::AddShader { family, classes } => Some((i, *family, classes)),
            _ => None,
        })
        .collect();
    if adds.is_empty() {
        return None;
    }
    let prefix: String = shipment
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
        .collect();
    let guard = format!("{prefix}_SHADERS_H");
    let mut h = String::new();
    h.push_str(&format!("/* {shipment}.shaders.h: the shader registrations of Shipment {shipment}, written by qm build.\n"));
    h.push_str(" * Queue each from DllMain (DLL_PROCESS_ATTACH) with m2_shader_add_pixel / m2_shader_add_vertex. */\n");
    h.push_str(&format!("#ifndef {guard}\n#define {guard}\n\n#include \"m2_shader.h\"\n\n"));
    for (index, family, classes) in adds {
        let stage = family.stage();
        h.push_str(&format!(
            "/* contributions[{index}]: {} family, {} stage */\n",
            family.name(),
            stage.token()
        ));
        h.push_str(&format!("#define {prefix}_SHADER_{index}_FAMILY {}\n", family.c_enumerator()));
        let name = format!("{prefix}_SHADER_{index}_CLASSES");
        match stage {
            Stage::Pixel => h.push_str(&format!("static const m2_shader_class {name}[4] = {{\n")),
            Stage::Vertex => h.push_str(&format!("static const m2_shader_class {name}[1] = {{\n")),
        }
        for (ci, class) in classes.iter().enumerate() {
            h.push_str(&format!(
                "    {{ {}, {} }}, /* light class {ci} */\n",
                c_string(&class.name),
                c_string(&format!("{}.sho", class.stem))
            ));
        }
        h.push_str("};\n\n");
    }
    h.push_str(&format!("#endif /* {guard} */\n"));
    Some(h)
}

fn c_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_family_enum_is_the_families_table_in_order() {
        let names: Vec<String> = ShaderFamily::ALL
            .iter()
            .map(|f| serde_json::to_value(f).unwrap().as_str().unwrap().to_string())
            .collect();
        let table: Vec<&str> = families().iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, table);
        for (i, f) in ShaderFamily::ALL.iter().enumerate() {
            assert_eq!(*f as usize, i);
            assert!(f.name().ends_with(f.stage().token()), "{} is a {} family", f.name(), f.stage().token());
        }
    }

    #[test]
    fn the_retail_registry_matches_the_live_counts_at_shader_level_one() {
        // The dumped registry's live counts, ShaderLevel 1: pixel 0xf2, vertex 0x81. The dump holds
        // every registration FUN_0084f130 makes and not PgCompositeFP, which the composite pass
        // constructor registers afterwards: its record at 0x0127CB58 holds key 0 in the dump and
        // the run-once byte 0x011759C0 is 0.
        for extra in [ExtraStores::None, ExtraStores::R2vb, ExtraStores::Vt] {
            let c = Config { extra, shader_level: true };
            assert_eq!(retail_count(c, Stage::Pixel), 0xf2 + 1);
            assert_eq!(retail_count(c, Stage::Vertex), 0x81);
        }
    }

    #[test]
    fn data_file_is_a_closed_set() {
        assert_eq!(DataFile::parse("data/shader3.bin"), Some(DataFile::Shader3));
        assert_eq!(DataFile::parse("data/shader3Low.bin"), Some(DataFile::Shader3Low));
        assert_eq!(DataFile::parse("data/shaderVT.bin"), None);
        assert!(serde_json::from_str::<DataFile>("\"data/shaderVT.bin\"").is_err());
    }

    #[test]
    fn stem_refusals() {
        assert!(stem_refusal("MyGlowFP").is_none());
        assert!(stem_refusal("").is_some());
        assert!(stem_refusal("MyGlowFP.sho").is_some());
        assert!(stem_refusal("a/b").is_some());
        assert!(stem_refusal(&"x".repeat(125)).is_some());
    }
}
