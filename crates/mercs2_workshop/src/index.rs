//! Asset catalog for the workshop: every browsable asset in the open WAD, with names resolved
//! through the reference bundle's name pack (`workshop_data/names.bin`) overlaid by the ASET
//! dictionary compiled into the binary. The bundle is found (or downloaded) by
//! [`require_data_home`]; the raw corpora the pack is built from are read only by `--pack-data`
//! ([`load_all_names_raw`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mercs2_engine::wad;

/// How the Library previews a category, stated HONESTLY.
///
/// The catalog indexes every ASET type, but only two of them have a faithful preview today. A
/// category whose preview is not implemented says so rather than offering a dead affordance — the
/// standing no-hidden-features rule, and the reason `Preview::None` carries its own explanation
/// instead of the UI inventing one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Preview {
    /// Full pipeline render on the pedestal.
    Render,
    /// 2D plate view (hi-mip streamed).
    Plate,
    /// Browsable with metadata; no faithful preview yet, and here is why.
    None(&'static str),
}

/// A browsable asset category.
///
/// Plan 02 organises the Library around the retail ASET type-discriminator table rather than
/// around the two types the tool happened to load first: *"Expand the browsable types beyond
/// Models/Textures."* Before this, `AssetIndex::build` kept exactly two type ids and dropped every
/// other ASET row on the floor, so 5,000+ animations, 314 effects, 248 audio banks and every font
/// and string table were invisible to the tool that is supposed to be the content browser.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Model,
    Texture,
    Animation,
    Effect,
    Audio,
    UiGfx,
    FaceFx,
    Text,
    /// World-state placement layers (`vz_state_*` / `layers_static`) — the overlays `edit_world`
    /// and `activate_layer` operate on. Browse-only: a layer is a set of placements, not a render.
    Layer,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Model,
        Kind::Texture,
        Kind::Animation,
        Kind::Effect,
        Kind::Audio,
        Kind::UiGfx,
        Kind::FaceFx,
        Kind::Text,
        Kind::Layer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Model => "MODELS",
            Kind::Texture => "TEXTURES",
            Kind::Animation => "ANIMATIONS",
            Kind::Effect => "EFFECTS",
            Kind::Audio => "AUDIO",
            Kind::UiGfx => "UI / GFX",
            Kind::FaceFx => "FACEFX",
            Kind::Text => "FONTS & TEXT",
            Kind::Layer => "WORLD LAYERS",
        }
    }

    /// Short label for the category strip, where eight of them share one row.
    pub fn short(self) -> &'static str {
        match self {
            Kind::Model => "Models",
            Kind::Texture => "Textures",
            Kind::Animation => "Anims",
            Kind::Effect => "VFX",
            Kind::Audio => "Audio",
            Kind::UiGfx => "UI",
            Kind::FaceFx => "FaceFX",
            Kind::Text => "Text",
            Kind::Layer => "Layers",
        }
    }

    /// The ASET type ids that belong to this category.
    ///
    /// Ids without a constant in `mercs2_formats::types` are spelled numerically and named in a
    /// comment; they come from `aset_type_ids::type_id_for_type_hash`, which is the registry.
    pub fn type_ids(self) -> &'static [u32] {
        use mercs2_formats::types::*;
        match self {
            Kind::Model => &[TYPE_ID_MODEL],
            Kind::Texture => &[TYPE_ID_TEXTURE],
            Kind::Animation => &[TYPE_ID_ANIMATION],
            Kind::Effect => &[TYPE_ID_EFFECT, TYPE_ID_FX_DICTIONARY],
            // wavebank, soundbank, sounddb(13)
            Kind::Audio => &[TYPE_ID_WAVEBANK, TYPE_ID_SOUNDBANK, 13],
            Kind::UiGfx => &[TYPE_ID_CFX_PACK],
            // facefx_animset(5), facefx_actor(34)
            Kind::FaceFx => &[5, 34],
            Kind::Text => &[TYPE_ID_FONT, TYPE_ID_STRINGDB],
            Kind::Layer => &[TYPE_ID_LAYER],
        }
    }

    pub fn preview(self) -> Preview {
        match self {
            Kind::Model => Preview::Render,
            Kind::Texture => Preview::Plate,
            // A clip needs a rig to play on, and the Library has no subject of its own. The
            // per-character catalog on a loaded model's Animation card is where clips play today.
            Kind::Animation => Preview::None(
                "a clip needs a rig — load a character and use its Animation card",
            ),
            Kind::Effect => Preview::None(
                "the effect format is decoded but its parameters are still hash-only",
            ),
            Kind::Audio => Preview::None("playback is not wired into the Library yet"),
            Kind::UiGfx => Preview::None("no Scaleform renderer in the workshop"),
            Kind::FaceFx => Preview::None("needs a head to drive"),
            Kind::Text => Preview::None("fonts render as a plate; string tables as a list"),
            Kind::Layer => Preview::None(
                "a layer is a placement set — browse it, then edit_world / activate_layer",
            ),
        }
    }
}

#[derive(Clone)]
pub struct AssetRow {
    pub hash: u32,
    pub block: u16,
    /// Which wad of the open stack owns this asset (0 = base, 1.. = overlays; last-wins).
    pub src: usize,
    /// Registry name, when the hash reverses (most placeable content does).
    pub name: Option<String>,
    /// A spawn template's view of a character kit: the template's `Name`. The row loads the kit
    /// (`hash`) showing that template's outfit (`app::KitOutfits`). `None` for an asset row.
    pub template: Option<String>,
    /// A template row's `FactionMarker` (`mercs2_formats::mixr::KitTemplate::faction`): the faction
    /// is the template's, not its kit's. `None` for an asset row.
    pub faction: Option<u32>,
}

impl AssetRow {
    /// List label: the template name for a template row, else the registry name when known, else
    /// the raw hash.
    pub fn label(&self) -> String {
        if let Some(t) = &self.template {
            return t.clone();
        }
        match &self.name {
            Some(n) => n.clone(),
            None => format!("0x{:08X}", self.hash),
        }
    }

    /// Whether this row is model `hash` showing `template`'s outfit (`None`: no template). A kit's
    /// template rows all share the kit's hash, so the hash alone names every one of them.
    pub fn is(&self, hash: u32, template: Option<&str>) -> bool {
        self.hash == hash && self.template.as_deref() == template
    }

    /// The faction this row lists under in an Inspect Lens: a template row by its own
    /// `FactionMarker`, named through `labels` ([`faction_labels`]); any other row by its name's
    /// prefix before the first `_`, upper-case. A row with neither shows its hash, the way an
    /// unnamed row is labelled everywhere.
    pub fn faction_group(&self, labels: &HashMap<u32, String>) -> String {
        if let Some(f) = self.faction {
            return labels.get(&f).cloned().unwrap_or_else(|| format!("0x{f:08X}"));
        }
        match &self.name {
            Some(n) => n.split('_').next().unwrap_or(n).to_ascii_uppercase(),
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

    /// The browser's grouping bucket for this asset, derived from the `<faction>_<domain>_…` naming
    /// convention: vehicles break out by class, then characters / buildings / weapons / props /
    /// world-state / other. Drives the grouped model navigator. `Unnamed` = hash-only rows.
    pub fn category(&self) -> &'static str {
        if self.template.is_some() {
            return "Character";
        }
        let Some(name) = self.name.as_ref() else { return "Unnamed" };
        if let Some(c) = self.vehicle_class() {
            return vehicle_display(c);
        }
        let n = name.to_ascii_lowercase();
        if n.contains("hum_") || n.contains("_hum") || n.contains("civ_") || n.contains("merc") {
            "Character"
        } else if n.contains("_bld") || n.contains("building") || n.contains("_hq") {
            "Building"
        } else if n.contains("_wpn") || n.contains("weapon") {
            "Weapon"
        } else if n.contains("vz_state") || n.contains("_state_") {
            "World state"
        } else if n.contains("_prop")
            || n.contains("_env")
            || n.contains("debris")
            || n.contains("foliage")
            || n.contains("plant")
            || n.contains("tree")
        {
            "Prop / scenery"
        } else if n.contains("_veh") {
            "Vehicle (other)"
        } else {
            "Other"
        }
    }
}

/// Display-cased category name for a `vehicle_class()` token.
fn vehicle_display(c: &str) -> &'static str {
    match c {
        "helicopter" => "Helicopter",
        "tank" => "Tank",
        "apc" => "APC",
        "vtol" => "VTOL",
        "jet" => "Jet",
        "car" => "Car",
        "truck" => "Truck",
        "van" => "Van",
        "semi" => "Semi",
        "trailer" => "Trailer",
        "towed" => "Towed",
        "motorcycle" => "Motorcycle",
        "boat" => "Boat",
        _ => "Vehicle (other)",
    }
}

/// The order the model navigator lists its category groups in.
pub const CATEGORY_ORDER: &[&str] = &[
    "Helicopter", "Tank", "APC", "VTOL", "Jet", "Car", "Truck", "Van", "Semi", "Trailer", "Towed",
    "Motorcycle", "Boat", "Vehicle (other)", "Character", "Building", "Weapon", "Prop / scenery",
    "World state", "Other", "Unnamed",
];

/// Sort rank for a category (unknown categories sort last).
pub fn category_order(cat: &str) -> usize {
    CATEGORY_ORDER.iter().position(|&c| c == cat).unwrap_or(CATEGORY_ORDER.len())
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

#[derive(Clone)]
pub struct AssetIndex {
    /// One row list per [`Kind`], parallel to `Kind::ALL`. A `Vec` of lists rather than a field
    /// per category: eight named fields would need eight matches to iterate, and the two that
    /// existed already had to be listed by hand in `rows`, `build` and `apply_names`.
    rows: Vec<Vec<AssetRow>>,
    pub names: HashMap<u32, String>,
    /// The world's templates that name a model (`mercs2_formats::mixr::kit_templates`), as given to
    /// [`Self::add_kit_templates`] — what a kit preview reads its outfits from.
    pub kit_templates: Vec<mercs2_formats::mixr::KitTemplate>,
    /// Every world template that names a model — what [`faction_labels`] learns its pairs from.
    pub world_templates: Vec<mercs2_formats::mixr::KitTemplate>,
    /// Each `FactionMarker`'s group label ([`faction_labels`]), kept current with the names.
    pub faction_labels: HashMap<u32, String>,
}

/// Each `FactionMarker` value, labelled with the model-name prefix it pairs with most often across
/// `templates` whose model has a name (upper-case, e.g. `HUD_faction_AN` → `AL`) — the spelling the
/// rest of the catalog groups under. Prefixes tied for most are joined with `/`. A marker no named
/// template carries has no label.
pub fn faction_labels(
    templates: &[mercs2_formats::mixr::KitTemplate],
    names: &HashMap<u32, String>,
) -> HashMap<u32, String> {
    let mut pairs: HashMap<u32, std::collections::BTreeMap<String, usize>> = HashMap::new();
    for t in templates {
        let (Some(f), Some(model)) = (t.faction, names.get(&t.model)) else { continue };
        let prefix = model.split('_').next().unwrap_or(model).to_ascii_uppercase();
        *pairs.entry(f).or_default().entry(prefix).or_default() += 1;
    }
    pairs
        .into_iter()
        .map(|(f, counts)| {
            let top = counts.values().copied().max().unwrap_or(0);
            let label: Vec<String> = counts.into_iter().filter(|(_, n)| *n == top).map(|(p, _)| p).collect();
            (f, label.join("/"))
        })
        .collect()
}

impl AssetIndex {
    pub fn rows(&self, kind: Kind) -> &[AssetRow] {
        self.rows
            .get(Kind::ALL.iter().position(|k| *k == kind).unwrap_or(0))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// The 3D models. Kept as a named accessor because most of the app is about them.
    pub fn models(&self) -> &[AssetRow] {
        self.rows(Kind::Model)
    }

    pub fn textures(&self) -> &[AssetRow] {
        self.rows(Kind::Texture)
    }

    /// How many assets a category holds — for the category strip's counts.
    pub fn count(&self, kind: Kind) -> usize {
        self.rows(kind).len()
    }

    /// Build the catalog from EVERY open WAD's ASET table (base + overlays, in stack order — a
    /// later wad's row wins, the game's patch rule). ALL model ASETs are listed, not just
    /// primaries: shared/instanced meshes (recruits like Misha/Ewan/Fiona, costume skins, world
    /// props) ride as SUB-ENTRIES and have no primary row — `extract_container` falls back to
    /// sub-entries, so they load fine once listed. `names` may be empty at boot — the app loads
    /// the (multi-second) name corpora on a background thread and calls [`Self::apply_names`]
    /// when they arrive, so the window opens immediately.
    pub fn build(wads: &[wad::Wad], names: HashMap<u32, String>) -> AssetIndex {
        // type_id -> which category it lands in, derived from `Kind::type_ids` so the mapping is
        // stated once. An id no category claims is skipped rather than guessed at.
        let mut of_type: HashMap<u32, usize> = HashMap::new();
        for (i, k) in Kind::ALL.iter().enumerate() {
            for t in k.type_ids() {
                of_type.insert(*t, i);
            }
        }
        let mut buckets: Vec<HashMap<u32, AssetRow>> =
            (0..Kind::ALL.len()).map(|_| HashMap::new()).collect();
        for (src, w) in wads.iter().enumerate() {
            let primary_block: HashMap<u32, u16> = wad::model_list(w).into_iter().collect();
            for (hash, ty, _) in wad::all_asets(w) {
                let Some(&i) = of_type.get(&ty) else { continue };
                // Only models carry a primary block here; the rest are reached through their own
                // ASET row, so 0 is the honest "not resolved by this index" value.
                let block = if ty == wad::MODEL_ASET_TYPE_ID {
                    primary_block.get(&hash).copied().unwrap_or(0)
                } else {
                    0
                };
                buckets[i].insert(hash, AssetRow { hash, block, src, name: None, template: None, faction: None });
            }
        }
        let mut idx = AssetIndex {
            rows: buckets.into_iter().map(|b| b.into_values().collect()).collect(),
            names: HashMap::new(),
            kit_templates: Vec::new(),
            world_templates: Vec::new(),
            faction_labels: HashMap::new(),
        };
        idx.apply_names(names);
        idx
    }

    /// Add one model row per kit template (`app::load_kit_templates`: a template that picks a
    /// character-kit outfit), labelled by the template's `Name`, beside its kit's own row. A template
    /// whose model has no row in the catalog adds nothing. `world` is every template that names a
    /// model, which the faction labels are learned from. Returns how many rows were added.
    pub fn add_kit_templates(
        &mut self,
        templates: &[mercs2_formats::mixr::KitTemplate],
        world: &[mercs2_formats::mixr::KitTemplate],
    ) -> usize {
        let Some(mi) = Kind::ALL.iter().position(|k| *k == Kind::Model) else { return 0 };
        self.world_templates = world.to_vec();
        let mut added = Vec::new();
        for t in templates {
            let (Some(name), Some(kit)) = (&t.name, self.rows[mi].iter().find(|r| r.hash == t.model && r.template.is_none()))
            else {
                continue;
            };
            added.push(AssetRow { template: Some(name.clone()), faction: t.faction, ..kit.clone() });
        }
        let n = added.len();
        self.rows[mi].extend(added);
        self.kit_templates = templates.to_vec();
        let names = std::mem::take(&mut self.names);
        self.apply_names(names);
        n
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
        for rows in self.rows.iter_mut() {
            for r in rows.iter_mut() {
                r.name = self.names.get(&r.hash).cloned();
            }
            rows.sort_by(order);
        }
        self.faction_labels = faction_labels(&self.world_templates, &self.names);
    }
}

/// The workshop's bundled reference data (`workshop_data/`): everything the tool consults that
/// is NOT a game-distributed file — the merged name pack, registry rows, spawnable templates,
/// ECS schemas, the decompiled-Lua corpus. Built by `--pack-data`, resolved here so the app runs
/// self-contained (no repo checkout needed). Resolution: `MERCS2_WORKSHOP_DATA` env, then
/// the saved setting, then `workshop_data/` next to the exe, then a `workshop_data/` walk-up from
/// the CWD, then the copy [`crate::fetch`] installs.
///
/// A candidate counts only when it is a COMPLETE bundle: it holds [`BUNDLE_MANIFEST`], which
/// `--pack-data` writes after every other file. The committed, partial `workshop_data/` of a repo
/// checkout (the Ess API, ECS registry and audio manifests, no `names.bin`) holds no manifest and
/// does not count.
///
/// This is the lookup alone; [`require_data_home`] adds the automatic download.
pub fn data_home() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MERCS2_WORKSHOP_DATA") {
        let p = PathBuf::from(p);
        if is_complete_bundle(&p) {
            return Some(p);
        }
    }
    if let Some(p) = crate::settings::load().data_dir {
        if is_complete_bundle(&p) {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("workshop_data");
            if is_complete_bundle(&cand) {
                return Some(cand);
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for dir in cwd.ancestors() {
            let cand = dir.join("workshop_data");
            if is_complete_bundle(&cand) {
                return Some(cand);
            }
        }
    }
    // LAST: a bundle this tool downloaded for itself (see `fetch`). Deliberately behind every other
    // step — a bundle the user installed or pointed Settings at must always beat a fetched copy.
    crate::fetch::installed_dir().filter(|p| is_complete_bundle(p))
}

/// The file `--pack-data` writes last into a bundle; its presence is what makes a folder a
/// complete bundle for [`data_home`].
pub const BUNDLE_MANIFEST: &str = "manifest.json";

/// Whether `dir` is a complete reference bundle (holds [`BUNDLE_MANIFEST`]).
fn is_complete_bundle(dir: &Path) -> bool {
    dir.join(BUNDLE_MANIFEST).is_file()
}

/// The reference bundle, found through [`data_home`] or, when none is found, downloaded and
/// installed through [`crate::fetch::install`]. An error when both fail, carrying where to
/// download the bundle by hand and where to put it. `report` receives the download's stage for the
/// boot loading screen.
pub fn require_data_home(report: &mut impl FnMut(f32, &'static str)) -> Result<PathBuf, String> {
    if let Some(home) = data_home() {
        return Ok(home);
    }
    report(0.02, "reference data (downloading workshop_data)");
    eprintln!(
        "[data] no complete workshop_data bundle found — downloading {} from {}",
        crate::fetch::ASSET,
        crate::fetch::release_page()
    );
    let installed = crate::fetch::install(&mut |e| {
        if let crate::fetch::Event::Progress(m) = e {
            eprintln!("[data] {m}");
        }
    })
    .map_err(|e| {
        format!(
            "no complete workshop_data reference bundle found (a folder holding {BUNDLE_MANIFEST}; \
             searched MERCS2_WORKSHOP_DATA, the Settings data folder, workshop_data next to the \
             executable, workshop_data in the working directory and its parents, and the downloaded \
             copy), and downloading it failed: {e}. {}",
            crate::fetch::manual_install_help()
        )
    })?;
    eprintln!("[data] installed workshop_data to {}", installed.display());
    Ok(installed)
}

/// The full name map: the bundle's name pack (`workshop_data/names.bin`) overlaid by the embedded
/// ASET dictionary. An error when the bundle cannot be found or downloaded, or its pack is
/// missing or unreadable.
pub fn load_all_names() -> Result<HashMap<u32, String>, String> {
    load_all_names_staged(|_, _| {})
}

/// [`load_all_names`] with per-stage progress reporting (`fraction 0..1`, stage label) — drives
/// the app's boot loading screen.
pub fn load_all_names_staged(
    mut report: impl FnMut(f32, &'static str),
) -> Result<HashMap<u32, String>, String> {
    let home = require_data_home(&mut report)?;
    let pack = home.join("names.bin");
    if !pack.is_file() {
        return Err(format!(
            "the reference bundle {} has no names.bin. {}",
            home.display(),
            crate::fetch::manual_install_help()
        ));
    }
    report(0.10, "bundled name pack");
    let mut names = load_names_pack(&pack)?;
    // The curated ASET dictionary is applied last and overrides the pack.
    names.extend(embedded_aset_names());
    Ok(names)
}

/// The curated ASET hash→name dictionary (every name we have actually CRACKED), compiled into the
/// binary. Unlike the rainbow table this is not speculative — each entry is a verified preimage.
const EMBEDDED_ASET_NAMES: &str = include_str!("../data/aset_names.tsv");

/// Parse [`EMBEDDED_ASET_NAMES`] (`0xHASH\tname` per line, `#` comments).
pub fn embedded_aset_names() -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for line in EMBEDDED_ASET_NAMES.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((h, name)) = line.split_once('\t') else { continue };
        if let Ok(h) = u32::from_str_radix(h.trim().trim_start_matches("0x"), 16) {
            out.insert(h, name.trim().to_string());
        }
    }
    out
}

/// The RAW corpora merge behind `--pack-data` when the repo's `data/production_names.json` is
/// absent, merged lowest→highest priority: the Jul-08 devkit strings, the bone-name candidates,
/// the rainbow table, the ASET-derived name fragments, then the live registry dump. Every source
/// is a dev-machine file named by `.mercs2-local.toml` (searched upward from `start`): the
/// `jul08_strings` and `rainbow_table` keys, and the `data/` folder under `notes_docs_dir`. A
/// missing key or file is an error naming it.
pub fn load_all_names_raw(start: &Path) -> Result<HashMap<u32, String>, String> {
    use mercs2_formats::game_paths;
    // 57k authored strings from the devkit Xbox build — resolves engine command/message names
    // (SetStateOnMsg, KillObjectsLinkedToHP, …) that the retail exe strips.
    let mut names = load_name_lines(&game_paths::local_config_jul08_strings(start)?)?;
    let notes_docs = game_paths::local_config_notes_docs_dir(start)?;
    names.extend(load_name_lines(&notes_data_file(&notes_docs, "bone_name_candidates.txt")?)?);
    names.extend(load_rainbow_json(&game_paths::local_config_rainbow_table(start)?)?);
    // Names cracked from the WAD's own naming convention rather than a wordlist: a texture
    // `X_dm`/`X_nm`/`X_sm` names its own model `X`, so a named texture yields its model's exact
    // preimage. Textures resolve ~95% but models only ~46%, so this closes a gap the rainbow
    // table structurally cannot — it is what makes most unnamed vehicle/prop models (the whole
    // aircraft roster: A-10, AC-130, C-130, Predator, Pave Low, AH-1Z, MiG-27, …) browsable.
    // Produced by `wad_simulator --bin asset_gap_probe --emit`; every entry is a verified preimage.
    for f in ASET_NAME_FRAGMENTS {
        names.extend(load_rainbow_json(&notes_data_file(&notes_docs, f)?)?);
    }
    names.extend(load_names_csv(&notes_data_file(&notes_docs, "live_registry_hashes.csv")?)?);
    Ok(names)
}

/// `<notes_docs_dir>/data/<name>`, or an error naming the `notes_docs_dir` key and the missing file.
pub fn notes_data_file(notes_docs: &Path, name: &str) -> Result<PathBuf, String> {
    let p = notes_docs.join("data").join(name);
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!(
            "{} is not a file (`notes_docs_dir` in {} names {})",
            p.display(),
            mercs2_formats::game_paths::LOCAL_CONFIG,
            notes_docs.display()
        ))
    }
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

/// How many names a `names.bin` holds, from its header alone — no blob parse, so the settings page
/// can validate a picked bundle without paying the full load. `None` if it is not a names pack.
pub fn names_pack_count(path: &Path) -> Option<u32> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 12];
    std::io::Read::read_exact(&mut f, &mut head).ok()?;
    if &head[0..8] != NAMES_PACK_MAGIC {
        return None;
    }
    Some(u32::from_le_bytes(head[8..12].try_into().ok()?))
}

/// Load `names.bin`; an error naming the file when it cannot be read or is not a well-formed pack.
pub fn load_names_pack(path: &Path) -> Result<HashMap<u32, String>, String> {
    let t0 = std::time::Instant::now();
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bad = |what: &str| format!("{} is not a readable name pack: {what}", path.display());
    if data.len() < 16 || &data[0..8] != NAMES_PACK_MAGIC {
        return Err(bad("no M2NAMES1 header"));
    }
    let count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let table_end = 12 + count * 8;
    if data.len() < table_end + 4 {
        return Err(bad("truncated entry table"));
    }
    let blob_len = u32::from_le_bytes([
        data[table_end],
        data[table_end + 1],
        data[table_end + 2],
        data[table_end + 3],
    ]) as usize;
    let blob = data
        .get(table_end + 4..table_end + 4 + blob_len)
        .ok_or_else(|| bad("truncated string blob"))?;
    let mut names = HashMap::with_capacity(count);
    for i in 0..count {
        let e = 12 + i * 8;
        let hash = u32::from_le_bytes([data[e], data[e + 1], data[e + 2], data[e + 3]]);
        let off = u32::from_le_bytes([data[e + 4], data[e + 5], data[e + 6], data[e + 7]]) as usize;
        let end = blob
            .get(off..)
            .and_then(|s| s.iter().position(|&b| b == 0))
            .ok_or_else(|| bad(&format!("entry {i} has no NUL-terminated name")))?
            + off;
        names.insert(hash, String::from_utf8_lossy(&blob[off..end]).into_owned());
    }
    eprintln!(
        "[names] {} names from bundled pack {} ({:.2}s)",
        names.len(),
        path.display(),
        t0.elapsed().as_secs_f32()
    );
    Ok(names)
}

/// The ASET-derived name fragments under `<notes_docs_dir>/data/` (same JSON shape as the rainbow
/// table), each a set of preimages derived from the WAD:
///   `aset_discovered_names` — texture-base -> model naming convention (`X_dm` names `X`)
///   `aset_block_strings`    — identifiers mined out of decompressed block payloads
///   `aset_expanded_names`   — slot-grammar expansion of the above (road/clip/atlas families)
///   `aset_external_names`   — block-path stems mined from the CONSOLE WADs (`aset_external_mine`).
///                             The PC bake strips generated-asset names; the PS3/Xbox builds ship an
///                             uncompressed block-path table that still spells them, and a block path
///                             IS the asset's name. Biggest single source of model names (+510).
///   `aset_model_names`      — per-model derivation from the textures the model ITSELF uses
///                             (`mercs2_probe --bin model_namer`): the model name is an
///                             order-preserving token SUBSEQUENCE of one of its texture names, which
///                             catches the middle-token case (`civ_hum_casualfemale_ub_c` ->
///                             `civ_hum_casualfemale_c`) that no suffix-strip can reach.
const ASET_NAME_FRAGMENTS: [&str; 6] = [
    "aset_discovered_names.json",
    "aset_block_strings.json",
    "aset_expanded_names.json",
    "aset_model_names.json",
    "aset_external_names.json",
    "aset_roster_names.json",
];

/// Load the decompiled-Lua corpus for reference search: `(display path, content, lowercased)`,
/// from the reference bundle's `lua/` and `lua_dlc/` (~7 MB total). The display path keeps the
/// bank the script ships in (`lua/resident/mrxmusic.lua`), because a module can ship in more than
/// one bank and each copy is its own script. An error when the bundle cannot be found or
/// downloaded, lacks either folder, or holds a script that cannot be read.
pub fn load_lua_corpus() -> Result<Vec<(String, String, String)>, String> {
    lua_corpus_at(&require_data_home(&mut |_, _| {})?)
}

/// The Lua corpus of the reference bundle at `home`, as [`load_lua_corpus`] reads it.
pub fn lua_corpus_at(home: &Path) -> Result<Vec<(String, String, String)>, String> {
    let mut out = Vec::new();
    for (tag, sub) in [("lua", "lua"), ("dlc", "lua_dlc")] {
        let d = home.join(sub);
        if !d.is_dir() {
            return Err(format!(
                "the reference bundle {} has no {sub}/ folder. {}",
                home.display(),
                crate::fetch::manual_install_help()
            ));
        }
        collect_lua(&d, &d, tag, &mut out)?;
    }
    eprintln!("[lua] reference corpus: {} scripts", out.len());
    Ok(out)
}

/// The Lua corpus of the reference bundle the corpus gates are pinned to — release v0.17.0's
/// `mercs2-workshop-data.zip`, the one CI downloads — as (folder, scripts). A gate run over any
/// other bundle fails on the count, so a stale or partial bundle cannot pass on fewer scripts.
#[cfg(test)]
pub const PINNED_CORPUS: [(&str, usize); 2] = [("lua", 675), ("lua_dlc", 39)];

#[cfg(test)]
mod lua_corpus_tests {
    use super::collect_lua;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mercs2-workshop-{name}-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("clear the scratch dir");
        }
        std::fs::create_dir_all(&dir).expect("make the scratch dir");
        dir
    }

    /// A module that ships in two banks is two scripts, and each keeps its bank in its path.
    #[test]
    fn a_script_keeps_the_bank_it_ships_in() {
        let root = scratch("banks");
        for bank in ["resident", "shell"] {
            std::fs::create_dir_all(root.join(bank)).expect("bank dir");
            std::fs::write(root.join(bank).join("mrxmusic.lua"), format!("-- {bank}\n")).expect("write");
        }
        let mut out = Vec::new();
        collect_lua(&root, &root, "lua", &mut out).expect("collects");
        let paths: Vec<&str> = out.iter().map(|(p, _, _)| p.as_str()).collect();
        assert_eq!(paths, vec!["lua/resident/mrxmusic.lua", "lua/shell/mrxmusic.lua"]);
        std::fs::remove_dir_all(&root).expect("clean up");
    }

    /// A script that cannot be read stops the load with its name, rather than leaving the corpus
    /// one script short with nothing said.
    #[test]
    fn an_unreadable_script_is_an_error_naming_it() {
        let root = scratch("unreadable");
        std::fs::write(root.join("bad.lua"), [0xff, 0xfe, 0x00]).expect("write");
        let mut out = Vec::new();
        let err = collect_lua(&root, &root, "lua", &mut out).expect_err("not UTF-8");
        assert!(err.contains("bad.lua"), "{err}");
        std::fs::remove_dir_all(&root).expect("clean up");
    }
}

fn collect_lua(root: &Path, dir: &Path, tag: &str, out: &mut Vec<(String, String, String)>) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths = Vec::new();
    for e in rd {
        paths.push(e.map_err(|e| format!("{}: {e}", dir.display()))?.path());
    }
    paths.sort();
    for p in paths {
        if p.is_dir() {
            collect_lua(root, &p, tag, out)?;
        } else if p.extension().and_then(|x| x.to_str()) == Some("lua") {
            let content = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let rel = p.strip_prefix(root).map_err(|e| format!("{}: {e}", p.display()))?;
            let rel = rel.to_string_lossy().replace('\\', "/");
            let lower = content.to_ascii_lowercase();
            out.push((format!("{tag}/{rel}"), content, lower));
        }
    }
    Ok(())
}

/// Hash a plain name-per-line list (bone-name candidates, devkit strings) into hash → name.
fn load_name_lines(path: &Path) -> Result<HashMap<u32, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut map = HashMap::new();
    for line in text.lines() {
        let n = line.trim();
        if !n.is_empty() {
            map.insert(mercs2_formats::hash::pandemic_hash_m2(n), n.to_string());
        }
    }
    eprintln!("[names] {} name-per-line entries from {}", map.len(), path.display());
    Ok(map)
}

/// Parse the rainbow table (`{"pandemic_hash_m2": {"0xHEX": ["name", …]}}`) into hash → first name.
fn load_rainbow_json(path: &Path) -> Result<HashMap<u32, String>, String> {
    let t0 = std::time::Instant::now();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let root = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|e| format!("{}: bad JSON: {e}", path.display()))?;
    let obj = root
        .get("pandemic_hash_m2")
        .and_then(|v| v.as_object())
        .ok_or_else(|| format!("{}: no `pandemic_hash_m2` object", path.display()))?;
    let mut map = HashMap::new();
    for (hex, names) in obj {
        let Ok(h) = u32::from_str_radix(hex.trim_start_matches("0x"), 16) else { continue };
        if let Some(n) = names.as_array().and_then(|a| a.first()).and_then(|v| v.as_str()) {
            map.insert(h, n.to_string());
        }
    }
    eprintln!(
        "[names] {} rainbow-table names from {} ({:.1}s)",
        map.len(),
        path.display(),
        t0.elapsed().as_secs_f32()
    );
    Ok(map)
}

/// Parse the registry dump (`name,pandemic_hash_m2_hex,…`) into hash → name.
fn load_names_csv(path: &Path) -> Result<HashMap<u32, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
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
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::AssetRow;

    fn row(hash: u32, template: Option<&str>) -> AssetRow {
        AssetRow { hash, block: 0, src: 0, name: None, template: template.map(str::to_string), faction: None }
    }

    #[test]
    fn a_kit_template_row_is_only_its_own_template() {
        let soldier = row(0x45CC_35FE, Some("Allied Soldier"));
        let officer = row(0x45CC_35FE, Some("Allied Officer"));
        let kit = row(0x45CC_35FE, None);
        assert!(soldier.is(0x45CC_35FE, Some("Allied Soldier")));
        assert!(!officer.is(0x45CC_35FE, Some("Allied Soldier")));
        assert!(!kit.is(0x45CC_35FE, Some("Allied Soldier")));
        assert!(kit.is(0x45CC_35FE, None));
        assert!(!soldier.is(0x45CC_35FE, None));
        assert!(!soldier.is(0x4D4F_42B5, Some("Allied Soldier")));
    }

    use mercs2_formats::hash::pandemic_hash_m2 as m2;
    use mercs2_formats::mixr::KitTemplate;
    use std::collections::HashMap;

    fn template(model: u32, faction: &str, name: &str) -> KitTemplate {
        KitTemplate { key: 0, name: Some(name.into()), model, profile: 0, faction: Some(m2(faction)) }
    }

    /// Retail pairs, from `kit_templates --factions`: the oc executive kit is worn by OC executives
    /// and by the VZ defector; the civilian aid-worker kit only by civilians.
    fn retail_like() -> (Vec<KitTemplate>, HashMap<u32, String>) {
        let (exec, aid) = (0x2EF7_E3C3, 0x01ED_3D6F);
        let names = HashMap::from([
            (exec, "oc_hum_super_executive".to_string()),
            (aid, "civ_hum_super_aidworkerfemale".to_string()),
        ]);
        let templates = vec![
            template(exec, "HUD_faction_OC", "OC Executive"),
            template(exec, "HUD_faction_OC", "OC Executive (Phone)"),
            template(exec, "HUD_faction_VZ", "VZ Oil Company Defector"),
            template(aid, "HUD_faction_CV", "Aid Worker (Female)"),
        ];
        (templates, names)
    }

    /// A marker is labelled with the prefix it pairs with most; a marker only carried by a template
    /// on another faction's kit takes that kit's prefix, which is all the data says about it.
    #[test]
    fn a_faction_marker_is_labelled_by_the_prefix_it_pairs_with() {
        let (templates, names) = retail_like();
        let labels = super::faction_labels(&templates, &names);
        assert_eq!(labels[&m2("HUD_faction_OC")], "OC");
        assert_eq!(labels[&m2("HUD_faction_CV")], "CIV");
        assert_eq!(labels[&m2("HUD_faction_VZ")], "OC");
    }

    #[test]
    fn prefixes_tied_for_a_marker_are_both_shown() {
        let names = HashMap::from([(1, "al_hum_a".to_string()), (2, "civ_hum_b".to_string())]);
        let templates = vec![template(1, "HUD_faction_AN", "a"), template(2, "HUD_faction_AN", "b")];
        assert_eq!(super::faction_labels(&templates, &names)[&m2("HUD_faction_AN")], "AL/CIV");
    }

    /// A template row groups by its own marker, so the defector sits with its faction rather than
    /// its kit's; an asset row by its name's prefix; a nameless row by its hash.
    #[test]
    fn a_row_groups_by_its_template_faction_else_its_name_prefix() {
        let labels = HashMap::from([(m2("HUD_faction_VZ"), "VZ".to_string())]);
        let mut defector = row(0x2EF7_E3C3, Some("VZ Oil Company Defector"));
        defector.name = Some("oc_hum_super_executive".into());
        defector.faction = Some(m2("HUD_faction_VZ"));
        assert_eq!(defector.faction_group(&labels), "VZ");
        let mut hero = row(0x1234, None);
        hero.name = Some("pmc_hum_mattias".into());
        assert_eq!(hero.faction_group(&labels), "PMC");
        assert_eq!(row(0xDEAD_BEEF, None).faction_group(&labels), "0xDEADBEEF");
    }
}
