//! Asset catalog for the workshop: every browsable asset in the open WAD, with names resolved
//! through the live registry dump (`docs/data/live_registry_hashes.csv`, 82k names captured from
//! the running game's name-hash table — see memory `name-registry-spawn-by-hash`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mercs2_engine::wad;

/// What an [`AssetRow`] is, which decides how Enter previews it (3D model vs 2D texture plate).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Model,
    Texture,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Model => "MODELS",
            Kind::Texture => "TEXTURES",
        }
    }
}

pub struct AssetRow {
    pub hash: u32,
    pub block: u16,
    /// Which wad of the open stack owns this asset (0 = base, 1.. = overlays; last-wins).
    pub src: usize,
    /// Registry name, when the hash reverses (most placeable content does).
    pub name: Option<String>,
}

impl AssetRow {
    /// List label: the registry name when known, else the raw hash.
    pub fn label(&self) -> String {
        match &self.name {
            Some(n) => n.clone(),
            None => format!("0x{:08X}", self.hash),
        }
    }

    /// Vehicle class for the Model Workbench inventory, or `None` if not a vehicle. Data-driven
    /// off the `<faction>_veh_<class>_<name>` naming convention (plus a few irregular names like
    /// `uh1huey`). The class token after `_veh_` is normalised into the 12-type taxonomy from
    /// `docs/reverse_engineer/valid_model_structure_map.md` §6.
    pub fn vehicle_class(&self) -> Option<&'static str> {
        let n = self.name.as_ref()?.to_ascii_lowercase();
        let cls = n.split("_veh_").nth(1)?.split('_').next().unwrap_or("");
        Some(classify_vehicle_token(cls))
    }
}

/// Normalise a `_veh_<token>_` class token into the model-structure-map taxonomy.
pub fn classify_vehicle_token(cls: &str) -> &'static str {
    if cls.contains("heli") || cls.starts_with("uh1") || cls.contains("huey") || cls.contains("copter") {
        "helicopter"
    } else if cls.contains("boat") || cls.contains("ship") {
        "boat"
    } else if cls.contains("tank") {
        "tank"
    } else if cls.contains("apc") {
        "apc"
    } else if cls.contains("vtol") || cls.contains("f35") || cls.contains("harrier") {
        "vtol"
    } else if cls.contains("moto") || cls.contains("bike") {
        "motorcycle"
    } else if cls.contains("semi") {
        "semi"
    } else if cls.contains("trailer") {
        "trailer"
    } else if cls.contains("van") {
        "van"
    } else if cls.contains("towed") || cls.contains("artillery") || cls.contains("howitzer") {
        "towed"
    } else if cls.contains("truck") {
        "truck"
    } else if cls == "car" || cls.contains("car") {
        "car"
    } else if cls.contains("jet") || cls.contains("plane") || cls.contains("a10") || cls.contains("f117") {
        "jet"
    } else {
        "other"
    }
}

pub struct AssetIndex {
    pub models: Vec<AssetRow>,
    pub textures: Vec<AssetRow>,
    pub names: HashMap<u32, String>,
}

impl AssetIndex {
    pub fn rows(&self, kind: Kind) -> &[AssetRow] {
        match kind {
            Kind::Model => &self.models,
            Kind::Texture => &self.textures,
        }
    }

    /// Build the catalog from EVERY open WAD's ASET table (base + overlays, in stack order — a
    /// later wad's row wins, the game's patch rule). ALL model ASETs are listed, not just
    /// primaries: shared/instanced meshes (recruits like Misha/Ewan/Fiona, costume skins, world
    /// props) ride as SUB-ENTRIES and have no primary row — `extract_container` falls back to
    /// sub-entries, so they load fine once listed. `names` may be empty at boot — the app loads
    /// the (multi-second) name corpora on a background thread and calls [`Self::apply_names`]
    /// when they arrive, so the window opens immediately.
    pub fn build(wads: &[wad::Wad], names: HashMap<u32, String>) -> AssetIndex {
        let mut models: HashMap<u32, AssetRow> = HashMap::new();
        let mut textures: HashMap<u32, AssetRow> = HashMap::new();
        for (src, w) in wads.iter().enumerate() {
            let primary_block: HashMap<u32, u16> = wad::model_list(w).into_iter().collect();
            for (hash, ty, _) in wad::all_asets(w) {
                if ty == wad::MODEL_ASET_TYPE_ID {
                    models.insert(
                        hash,
                        AssetRow {
                            hash,
                            block: primary_block.get(&hash).copied().unwrap_or(0),
                            src,
                            name: None,
                        },
                    );
                } else if ty == mercs2_formats::types::TYPE_ID_TEXTURE {
                    textures.insert(hash, AssetRow { hash, block: 0, src, name: None });
                }
            }
        }
        let mut idx = AssetIndex {
            models: models.into_values().collect(),
            textures: textures.into_values().collect(),
            names: HashMap::new(),
        };
        idx.apply_names(names);
        idx
    }

    /// Attach a (new) hash→name map: resolve every row's label and re-sort — named assets first
    /// (alphabetical), unnamed by hash, so the browsable content sits up top.
    pub fn apply_names(&mut self, names: HashMap<u32, String>) {
        self.names = names;
        let order = |a: &AssetRow, b: &AssetRow| match (&a.name, &b.name) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.hash.cmp(&b.hash),
        };
        for rows in [&mut self.models, &mut self.textures] {
            for r in rows.iter_mut() {
                r.name = self.names.get(&r.hash).cloned();
            }
            rows.sort_by(order);
        }
    }
}

/// The workshop's bundled reference data (`workshop_data/`): everything the tool consults that
/// is NOT a game-distributed file — the merged name pack, registry rows, spawnable templates,
/// ECS schemas, the decompiled-Lua corpus. Built by `--pack-data`, resolved here so the app runs
/// self-contained (no repo checkout needed). Resolution: `MERCS2_WORKSHOP_DATA` env, then
/// `workshop_data/` next to the exe, then a `workshop_data/` walk-up from the CWD.
pub fn data_home() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MERCS2_WORKSHOP_DATA") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("workshop_data");
            if cand.is_dir() {
                return Some(cand);
            }
        }
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join("workshop_data");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Load EVERY name corpus, merged lowest→highest priority: bone-name candidates (141k, HIER
/// bones), the rainbow table (733k m2 preimages — most asset/clip hashes), then the live
/// registry dump (ground truth from the running game). Prefers the bundled binary pack
/// (`workshop_data/names.bin`, sub-second) and falls back to the raw repo corpora (~8 s).
pub fn load_all_names(names_csv: Option<PathBuf>) -> HashMap<u32, String> {
    load_all_names_staged(names_csv, |_, _| {})
}

/// [`load_all_names`] with per-stage progress reporting (`fraction 0..1`, stage label) — drives
/// the app's boot loading screen. Fractions are stage START marks weighted by observed timings
/// (the rainbow table dominates the slow path).
pub fn load_all_names_staged(
    names_csv: Option<PathBuf>,
    mut report: impl FnMut(f32, &'static str),
) -> HashMap<u32, String> {
    if let Some(home) = data_home() {
        let pack = home.join("names.bin");
        if pack.is_file() {
            report(0.10, "bundled name pack");
            if let Some(names) = load_names_pack(&pack) {
                report(1.0, "done");
                return names;
            }
            eprintln!("[names] {} unreadable — falling back to raw corpora", pack.display());
        }
    }
    load_all_names_raw(names_csv, report)
}

/// The RAW corpora merge (no `names.bin` fast path) — `--pack-data` uses this so rebuilding the
/// pack can never circularly read the stale pack it is replacing.
pub fn load_all_names_raw(
    names_csv: Option<PathBuf>,
    mut report: impl FnMut(f32, &'static str),
) -> HashMap<u32, String> {
    report(0.02, "devkit strings (Jul-08 prototype)");
    // 57k authored strings from the devkit Xbox build — resolves engine command/message names
    // (SetStateOnMsg, KillObjectsLinkedToHP, …) that the retail exe strips.
    let mut names = repo_walk_up("output/jul08_prototype/mercs2_xenon_p.pe_full_strings.txt")
        .map(load_name_lines)
        .unwrap_or_default();
    report(0.08, "bone-name candidates");
    for (h, n) in default_bone_candidates().map(load_name_lines).unwrap_or_default() {
        names.insert(h, n);
    }
    report(0.15, "rainbow table (733k hashes)");
    for (h, n) in default_rainbow_json().map(load_rainbow_json).unwrap_or_default() {
        names.insert(h, n);
    }
    report(0.90, "live registry names");
    for (h, n) in names_csv.map(load_names_csv).unwrap_or_default() {
        names.insert(h, n);
    }
    report(1.0, "done");
    names
}

// ── names.bin: the merged hash→name map in a load-fast binary layout. ──
// magic "M2NAMES1" | u32 count | count × (u32 hash, u32 blob_offset) | u32 blob_len | blob
// (strings NUL-terminated in the blob; entries sorted by hash).
const NAMES_PACK_MAGIC: &[u8; 8] = b"M2NAMES1";

/// Write the merged name map as `names.bin` (see the format note above).
pub fn write_names_pack(path: &Path, names: &HashMap<u32, String>) -> std::io::Result<()> {
    let mut entries: Vec<(&u32, &String)> = names.iter().collect();
    entries.sort_by_key(|(h, _)| **h);
    let mut table = Vec::with_capacity(entries.len() * 8);
    let mut blob: Vec<u8> = Vec::new();
    for (h, n) in &entries {
        table.extend_from_slice(&h.to_le_bytes());
        table.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        blob.extend_from_slice(n.as_bytes());
        blob.push(0);
    }
    let mut out = Vec::with_capacity(16 + table.len() + blob.len());
    out.extend_from_slice(NAMES_PACK_MAGIC);
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&table);
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(&blob);
    std::fs::write(path, out)
}

/// Load `names.bin`; `None` on any structural mismatch (caller falls back to raw corpora).
fn load_names_pack(path: &Path) -> Option<HashMap<u32, String>> {
    let t0 = std::time::Instant::now();
    let data = std::fs::read(path).ok()?;
    if data.len() < 16 || &data[0..8] != NAMES_PACK_MAGIC {
        return None;
    }
    let count = u32::from_le_bytes(data[8..12].try_into().ok()?) as usize;
    let table_end = 12 + count * 8;
    if data.len() < table_end + 4 {
        return None;
    }
    let blob_len = u32::from_le_bytes(data[table_end..table_end + 4].try_into().ok()?) as usize;
    let blob = data.get(table_end + 4..table_end + 4 + blob_len)?;
    let mut names = HashMap::with_capacity(count);
    for i in 0..count {
        let e = 12 + i * 8;
        let hash = u32::from_le_bytes(data[e..e + 4].try_into().ok()?);
        let off = u32::from_le_bytes(data[e + 4..e + 8].try_into().ok()?) as usize;
        let end = blob[off..].iter().position(|&b| b == 0)? + off;
        names.insert(hash, String::from_utf8_lossy(&blob[off..end]).into_owned());
    }
    eprintln!(
        "[names] {} names from bundled pack {} ({:.2}s)",
        names.len(),
        path.display(),
        t0.elapsed().as_secs_f32()
    );
    Some(names)
}

/// Find a repo-relative file by walking up from the CWD.
fn repo_walk_up(rel: &str) -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join(rel);
        if cand.is_file() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Find `docs/data/bone_name_candidates.txt` by walking up from the CWD.
fn default_bone_candidates() -> Option<PathBuf> {
    repo_walk_up("docs/data/bone_name_candidates.txt")
}

/// Load the decompiled-Lua corpus for reference search: `(display path, content, lowercased)`.
/// Bundled `workshop_data/lua[_dlc]` preferred; repo corpora as dev fallback. ~7 MB total.
pub fn load_lua_corpus() -> Vec<(String, String, String)> {
    let mut roots: Vec<(String, PathBuf)> = Vec::new();
    if let Some(home) = data_home() {
        for (tag, sub) in [("lua", "lua"), ("dlc", "lua_dlc")] {
            let d = home.join(sub);
            if d.is_dir() {
                roots.push((tag.into(), d));
            }
        }
    }
    if roots.is_empty() {
        for (tag, rel) in [("lua", "docs/mercs2-luacd/src"), ("dlc", "docs/mercs2-dlc-luacd/src")] {
            if let Some(d) = repo_walk_up_dir(rel) {
                roots.push((tag.into(), d));
            }
        }
    }
    let mut out = Vec::new();
    for (tag, root) in roots {
        collect_lua(&root, &tag, &mut out);
    }
    eprintln!("[lua] reference corpus: {} scripts", out.len());
    out
}

fn repo_walk_up_dir(rel: &str) -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join(rel);
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn collect_lua(dir: &Path, tag: &str, out: &mut Vec<(String, String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_lua(&p, tag, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("lua") {
            if let Ok(content) = std::fs::read_to_string(&p) {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                let lower = content.to_ascii_lowercase();
                out.push((format!("{tag}/{name}"), content, lower));
            }
        }
    }
}

/// Hash a plain name-per-line list (bone-name candidates) into hash → name.
fn load_name_lines(path: PathBuf) -> HashMap<u32, String> {
    let Ok(text) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for line in text.lines() {
        let n = line.trim();
        if !n.is_empty() {
            map.insert(mercs2_formats::hash::pandemic_hash_m2(n), n.to_string());
        }
    }
    eprintln!("[names] {} name-per-line entries from {}", map.len(), path.display());
    map
}

/// Find `docs/data/live_registry_hashes.csv` by walking up from the CWD (works from the repo root
/// and from `tools/wad_simulator`); `MERCS2_NAMES` overrides.
pub fn default_names_csv() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MERCS2_NAMES") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join("docs/data/live_registry_hashes.csv");
        if cand.is_file() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Find `tools/rainbow_table.json` by walking up from the CWD; `MERCS2_RAINBOW` overrides.
pub fn default_rainbow_json() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MERCS2_RAINBOW") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join("tools/rainbow_table.json");
        if cand.is_file() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Parse the rainbow table (`{"pandemic_hash_m2": {"0xHEX": ["name", …]}}`) into hash → first name.
fn load_rainbow_json(path: PathBuf) -> HashMap<u32, String> {
    let t0 = std::time::Instant::now();
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("[names] unreadable: {}", path.display());
        return HashMap::new();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&text) else {
        eprintln!("[names] bad JSON: {}", path.display());
        return HashMap::new();
    };
    let mut map = HashMap::new();
    if let Some(obj) = root.get("pandemic_hash_m2").and_then(|v| v.as_object()) {
        for (hex, names) in obj {
            let Ok(h) = u32::from_str_radix(hex.trim_start_matches("0x"), 16) else { continue };
            if let Some(n) = names.as_array().and_then(|a| a.first()).and_then(|v| v.as_str()) {
                map.insert(h, n.to_string());
            }
        }
    }
    eprintln!(
        "[names] {} rainbow-table names from {} ({:.1}s)",
        map.len(),
        path.display(),
        t0.elapsed().as_secs_f32()
    );
    map
}

/// Parse the registry dump (`name,pandemic_hash_m2_hex,…`) into hash → name.
fn load_names_csv(path: PathBuf) -> HashMap<u32, String> {
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("[names] unreadable: {}", path.display());
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for line in text.lines().skip(1) {
        let mut cols = line.split(',');
        let (Some(name), Some(hex)) = (cols.next(), cols.next()) else { continue };
        let Some(h) = hex.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok()) else {
            continue;
        };
        map.entry(h).or_insert_with(|| name.to_string());
    }
    eprintln!("[names] {} registry names from {}", map.len(), path.display());
    map
}
