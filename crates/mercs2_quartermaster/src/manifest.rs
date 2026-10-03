//! The Shipment manifest — ONE serde model that reads YAML, JSON and TOML.
//!
//! Spec: `.claude/plans/workshop-mods-rebuild-04-manifest-format.md` (rev 3).
//!
//! Two invariants this module exists to hold:
//!
//! * **One model, three formats.** The same logical document must deserialize identically from
//!   `manifest.yaml`, `.json` and `.toml`. That is why `Contribution` is internally tagged by
//!   `kind` — a shape that serializes identically across all three — rather than per-kind
//!   top-level arrays.
//! * **A name is preferred; a bare hash is legal.** Anywhere an existing asset is referenced,
//!   `0xHHHHHHHH` resolves to that hash and anything else is hashed as a name — see [`asset_hash`],
//!   which every such site must route through. The base game ships hashes, so requiring names would
//!   forbid referring to assets our name table does not cover. The linter still offers the name when
//!   it can reverse one (M0130), because a hash is one-way and a manifest full of them cannot be
//!   read or reviewed — but that is a suggestion, not a gate.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The manifest format this build reads. It is the ONLY format: any other value, older or newer, is
/// a loud reject (see [`Manifest::validate`]).
pub const FORMAT_VERSION: u32 = 2;

/// Maximum length of `shipment.name` — it becomes the output filename `_build/<name>.wad`.
pub const MAX_NAME_LEN: usize = 64;

/// DLL stems no Shipment may be named after (M0211), lowercase.
///
/// One list with two uses: a Shipment named after one of these is refused by validation, and the
/// same names are the DLLs a Shipment may never ship into the game root. Names only, never
/// versions. `pmc_bb` is the loader,
/// `cruise` its sidecar, and `dxwrapper` / `binkw32` are DLLs the loader writes a `BUILD dll=` line
/// for. Compared lowercased.
pub const DENY_LISTED_DLL_STEMS: &[&str] = &["pmc_bb", "cruise", "dxwrapper", "binkw32"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub shipment: Shipment,
    /// Legacy files this Shipment replaces. qm refuses while any of them is still in the game
    /// folder (`qm build`, `qm preflight`, `qm link`); it never deletes one.
    #[serde(default)]
    pub supersedes: Vec<Superseded>,
    #[serde(default)]
    pub load: Load,
    #[serde(default)]
    pub contributions: Vec<Contribution>,
}

/// One legacy file a Shipment supersedes: a file NAME inside a named game-folder destination.
///
/// `dest` is a [`PlaceIn`] for the same reason `place_file`'s is: the directory half is a name out
/// of a closed set, never a path. `file` must be a single filename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Superseded {
    pub dest: PlaceIn,
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shipment {
    /// Slug. `^[a-z0-9]+(-[a-z0-9]+)*$`, <= [`MAX_NAME_LEN`]. Unique; used by deps AND as the
    /// output filename.
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Semver (`semver::Version`). Anything else fails validation.
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub target: Target,
    /// The range of `mercs2_quartermaster` versions that can build this (a semver range, M0172).
    #[serde(default)]
    pub quartermaster: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Retail,
    Reimpl,
    /// RESERVED. Parses so the Quartermaster can reject it by NAME with an explanation rather than
    /// emitting a bare "unknown variant" — split-vs-shared semantics are deferred until the reimpl
    /// consumer is real (Plan 04 Open-Q4).
    Both,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Load {
    /// Hard deps — `qm preflight` / `qm link` fail if one is unsatisfied. They are also the only
    /// declared ordering source: a requirement's provider loads before its consumer. Cross-shipment
    /// references are COMPUTED (read-set); this field carries only what the Quartermaster cannot
    /// infer.
    #[serde(default)]
    pub requires: Vec<Requirement>,
    /// Shipments this one cannot be installed beside — by name, or by name within a version range.
    #[serde(default)]
    pub conflicts: Vec<ConflictDecl>,
    /// Capability tokens this Shipment declares it provides — the other side of
    /// [`Requirement::Capability`]. Multiple Shipments can `provides` the same token so a consumer
    /// can `requires` it interchangeably (e.g., three different widescreen-fix mods all
    /// `provides: [widescreen]`, a UI mod `requires: [{capability: widescreen}]`, and any one of
    /// them satisfies the dep). Free-form strings; conventionally lowercase-kebab.
    #[serde(default)]
    pub provides: Vec<String>,
}

/// `{ shipment, version }`: a Shipment by name, within a semver range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShipmentReq {
    pub shipment: String,
    pub version: String,
}

/// `{ capability }`: any Shipment that `provides` the token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityReq {
    pub capability: String,
}

/// `{ name, version }`: a form this format does NOT accept. It parses only so validation can name
/// the replacement (`{ shipment, version }`) instead of printing a bare "no variant matched".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibleReq {
    pub name: String,
    pub version: String,
}

/// A hard dependency.
///
/// * `Shipment(name)` — another Shipment, by name, any version.
/// * `ShipmentRange { shipment, version }` — another Shipment within a semver range (`"^1.0.0"`).
/// * `Capability { capability }` — any Shipment that `provides` the token.
/// * `Compatible { name, version }` — rejected by validation, with the `{ shipment, version }`
///   spelling in the message.
///
/// Untagged. Each object variant wraps a `deny_unknown_fields` struct, so an object with a key from
/// two forms (`{ shipment, name, version }`) matches none of them and fails to parse, and so does
/// any shape the model does not have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Requirement {
    Shipment(String),
    ShipmentRange(ShipmentReq),
    Capability(CapabilityReq),
    Compatible(CompatibleReq),
}

/// A declared incompatibility: a Shipment by name, or by name within a semver range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConflictDecl {
    Name(String),
    Range(ShipmentReq),
}

impl ConflictDecl {
    /// The Shipment name this entry names.
    pub fn name(&self) -> &str {
        match self {
            ConflictDecl::Name(n) => n,
            ConflictDecl::Range(r) => &r.shipment,
        }
    }

    /// The version range, when one is given.
    pub fn range(&self) -> Option<&str> {
        match self {
            ConflictDecl::Name(_) => None,
            ConflictDecl::Range(r) => Some(&r.version),
        }
    }
}

/// Parse a bare `0xHHHHHHHH` asset reference into the hash it names.
///
/// `None` when this is a name rather than a hash — including hex too long to be a `u32`, which
/// cannot be an asset hash whatever else it is.
pub fn bare_hash(reference: &str) -> Option<u32> {
    let s = reference.trim();
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    if hex.is_empty() || hex.len() > 8 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// Resolve an asset reference to the hash the engine will look up.
///
/// A bare `0x…` **is** the hash; anything else is a name and gets hashed. Both spellings are legal:
/// the game itself ships hashes, so a modder working on an asset our name table does not cover has
/// nothing else to write. The linter still prefers names (M0130) — a hash is one-way, so a manifest
/// full of them is unreadable and undiffable — but that is a preference, not a mandate.
///
/// **Every** site that turns a reference into a hash must come through here. Hashing the *string*
/// `"0x56130E64"` yields `0xC6B71C1F`, so a builder that parsed it while the conflict system hashed
/// it would claim one asset and write another, and conflict detection would silently stop matching.
pub fn asset_hash(reference: &str) -> u32 {
    bare_hash(reference).unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(reference.trim()))
}

/// The wardrobe heroes, in the spelling the tool PREFERS and shows.
///
/// `jen`, not `jennifer` (user preference). This is the input/label vocabulary — the runtime
/// `_tOutfits` KEY is a separate question answered by [`wearer_table_key`], because the game's table
/// is keyed `chris` / `jennifer` / `mattias` (`wifpmcinterior.lua` lines 156/183/215) and the third
/// key is `jennifer`. A row appended to `_tOutfits.jen` would land in a table nothing reads.
pub const WEARERS: [&str; 3] = ["mattias", "chris", "jen"];

/// The runtime `_tOutfits` key for a wearer spelling, or `None` if it is not a hero.
///
/// This is the ONE place the `jen`/`jennifer` split is resolved. Both spellings — the preferred
/// `jen` and the literal runtime key `jennifer` — map to `jennifer`, which is what the shipped
/// wardrobe table is actually keyed by. Every site that turns a `wearer` into the `_tOutfits` table
/// key MUST route through here, or an outfit silently appends to a table the game never reads
/// (the M0140 failure).
pub fn wearer_table_key(wearer: &str) -> Option<&'static str> {
    match wearer.trim().to_ascii_lowercase().as_str() {
        "mattias" => Some("mattias"),
        "chris" => Some("chris"),
        "jen" | "jennifer" => Some("jennifer"),
        _ => None,
    }
}

/// A declared blast-radius entry. A name, or a bare hash where no name is known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Touch(pub String);

impl Touch {
    /// True when the author wrote a bare hash instead of a name. Legal — the base game ships hashes
    /// — but the linter offers the name when it can reverse one, because a hash is one-way and a
    /// manifest full of them cannot be read or reviewed.
    ///
    /// The draft spec's own example paired `ch_veh_boat_destroyer` with `0xE54047D5` — which is
    /// actually `al_veh_boat_destroyer`. That drift is the reason this predicate exists.
    pub fn is_bare_hash(&self) -> bool {
        bare_hash(&self.0).is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Data,
    Script,
    Code,
    Runtime,
}

/// Where a `place_file` contribution puts its file — a **closed set of named destinations**, never
/// a path.
///
/// This enum IS the design of [`Contribution::PlaceFile`]. `native_hook` has no destination field
/// at all, which is what makes `Mercenaries2.exe` and `data\vz.wad` unreachable by CONSTRUCTION
/// rather than by a lint somebody could suppress. A kind that places companion files had to keep
/// that property while admitting more than one destination, and the only way to hold both is to let
/// the author pick a NAME out of a fixed list instead of writing a path.
///
/// So there is no spelling of `dest:` that is a path. `..`, `/etc/passwd`, `C:\Windows`,
/// `\\host\share` and a symlink are not *rejected* — they do not parse, and serde says which
/// variants exist. The destination half of a placement carries no author bytes whatsoever; only the
/// filename does, and that comes from the source file (see `build::game_folder_name_refusal`).
///
/// **The four ASI roots are measured, not assumed.** `pmc_bb.dll` v3.0.0 carries the format strings
/// `%s*.asi`, `%sscripts\`, `%splugins\` and `%supdate\`, so the loader globs the game directory
/// itself plus exactly those three subfolders. A destination outside that set would put a plugin's
/// companion where nothing looks for it.
///
/// ⚠ **The three `scripts/On*` rungs rest on weaker evidence, and are recorded as weaker.** They
/// come from Plan 03's write-up of Wally's Lua bridge — "script loader (`OnBoot/`/`OnLoad/`
/// (world-load-triggered)/`OnKey/`)" — which is prose about a repo this workspace does not vendor.
/// Nobody here has read the directory scan that consumes them, so the exact spelling and the parent
/// directory are inferred. They sit under `scripts/` because that is where the bridge `.asi` itself
/// goes, and because every companion path measured in this ecosystem so far resolves against the
/// loading module's OWN directory rather than the game root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceIn {
    /// The game directory itself — where `Mercenaries2.exe` lives. The loader globs `%s*.asi` here.
    GameRoot,
    /// `scripts\` — where the ecosystem already puts plugins and their companions.
    Scripts,
    /// `plugins\`.
    Plugins,
    /// `update\`.
    Update,
    /// The Lua bridge's boot-time script rung.
    OnBoot,
    /// The Lua bridge's world-load script rung.
    OnLoad,
    /// The Lua bridge's key-binding script rung.
    OnKey,
}

impl PlaceIn {
    /// The directory this destination names, relative to the game folder, with forward slashes.
    ///
    /// Forward slashes on purpose: the loader's own literals are backslashed (`%sscripts\`), but
    /// this is a filesystem path a deploy tool joins, not an engine path like a backslashed `PTHS`
    /// entry. The game root is the empty string, so joining is uniform.
    ///
    /// Every arm is a literal. Nothing an author writes reaches this string.
    pub const fn relative_dir(self) -> &'static str {
        match self {
            PlaceIn::GameRoot => "",
            PlaceIn::Scripts => "scripts",
            PlaceIn::Plugins => "plugins",
            PlaceIn::Update => "update",
            PlaceIn::OnBoot => "scripts/OnBoot",
            PlaceIn::OnLoad => "scripts/OnLoad",
            PlaceIn::OnKey => "scripts/OnKey",
        }
    }

    /// Every destination, so a test can assert a property of the whole set rather than of the
    /// arms somebody remembered to list.
    pub const ALL: [PlaceIn; 7] = [
        PlaceIn::GameRoot,
        PlaceIn::Scripts,
        PlaceIn::Plugins,
        PlaceIn::Update,
        PlaceIn::OnBoot,
        PlaceIn::OnLoad,
        PlaceIn::OnKey,
    ];
}

/// Optional cross-rig retarget on an import that is not already hero-rigged. Inline rather than a
/// standalone kind so v1 avoids inter-contribution reference machinery entirely (Plan 04 Q6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retarget {
    /// The source rig's convention — `cod`, `valve`, `mixamo`, `unreal`, `pandemic`, `generic`.
    ///
    /// Documentation and a sanity check, not the instruction. Detection runs from the bone names in
    /// the file itself; this records what the author believed so a mismatch can be reported.
    pub from: String,

    /// The RESOLVED bone map: source bone name → target bone name, or `~` to drop the bone.
    ///
    /// # Why the map and not just `from:`
    ///
    /// A convention name is not enough to reproduce a remap. **Five** conventions carry explicit
    /// correction tables (`cod`, `valve`, `mixamo`, `unreal`, `pandemic` — see
    /// `retarget::explicit_target_name`), because the generic keyword mapper misreads their
    /// namings: CoD's `j_shoulder` is the upper arm, ValveBiped carries four spine rungs against
    /// Pandemic's three. Pandemic's table is an identity rather than a correction. And any bone in
    /// any of them can be hand-adjusted in the Workshop.
    ///
    /// On "hand-verified": `retarget.rs` states the honest position, which is narrower than this
    /// doc used to claim — *"CoD is verified against a real asset (Roze); ValveBiped/Mixamo/Unreal
    /// use their standardised bone names."* Treat the other four as convention-following rather
    /// than measured.
    ///
    /// Carrying only `from:` would mean a Shipment built by someone else, or rebuilt later, silently
    /// differed from what the author previewed and approved.
    ///
    /// So the Workshop writes the map it actually used. It is verbose, and that is the point: it is
    /// reviewable in a diff, and the build is reproducible from the Shipment alone.
    ///
    /// Omit it and the build falls back to `char_skin::automap` on the names in the file, which is
    /// correct for generic and Mixamo-style rigs and is reported as a warning for the rest.
    #[serde(default)]
    pub bones: Option<std::collections::BTreeMap<String, Option<String>>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Textures {
    #[serde(default)]
    pub diffuse: Option<PathBuf>,
    #[serde(default)]
    pub normal: Option<PathBuf>,
    #[serde(default)]
    pub specular: Option<PathBuf>,
}

/// A language the engine can run in: an entry of its language table.
///
/// The table (`0x00CF281C`, nine pointers, bounded by `FUN_00826a10`) reads english, spanish,
/// italian, french, german, japanese, english_uk, allcaps, russian; `*DAT_01176018` indexes it. The
/// engine opens `.\Data\<entry>.wad` and `.\Data\<entry>-patch.wad` by the entry
/// (`FUN_004BFE20`, `FUN_004BFEF0`), and retail Lua appends the same entry to every `vo_*` bank
/// name before loading it (`_GetLocalizedName` with `Gui.GetLanguageName`,
/// `mrxsoundbanks.lua:80-87`), so the token names both a language's WADs and its voice-over banks.
/// `english_uk` and `allcaps` are selectable only from the command line (option `0xC13F3DE2`,
/// `FUN_00826a10`); the OS-locale map (`FUN_00826a90`) never picks them, and the `GetLanguage` Lua
/// binding (`0x005E6420`) reports both as English.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    English,
    Spanish,
    Italian,
    French,
    German,
    Japanese,
    Russian,
}

impl Language {
    /// Every language, in table order.
    pub const ALL: [Language; 7] = [
        Language::English,
        Language::Spanish,
        Language::Italian,
        Language::French,
        Language::German,
        Language::Japanese,
        Language::Russian,
    ];

    /// The table's entry: the base name of `.\Data\<token>.wad`, and the suffix of a `vo_*` bank's
    /// entry name.
    pub const fn token(self) -> &'static str {
        match self {
            Language::English => "english",
            Language::Spanish => "spanish",
            Language::Italian => "italian",
            Language::French => "french",
            Language::German => "german",
            Language::Japanese => "japanese",
            Language::Russian => "russian",
        }
    }
}

/// A session of the game that loads sound banks from Lua, and so a place a mod loader loads a bank.
///
/// Each session is one level WAD's Lua VM (the VM is closed and recreated on every level swap,
/// `scripting_host_binding_code_map.md`):
///
/// * `gameplay` — the `vz` level. Retail loads its banks in `MrxSoundBootstrap.LoadBanks`
///   (`resident/mrxsoundbootstrap.lua:192-246`) and unloads them in `ExitGame` (`:188-190`); the
///   mod loader loads in `wifpmcinterior._OnEnter` and unloads after `ExitGame`. Its blocks ship in
///   the Shipment overlay.
/// * `front_end` — the `shell` level (the main menu). Retail loads its banks in
///   `MrxSound.EnterShellState` (`shell/mrxsound.lua:5-15`) and unloads them in `ExitShellState`
///   (`:17-27`); the front-end loader runs after each. Its blocks ship in the shell patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadSession {
    Gameplay,
    FrontEnd,
}

impl LoadSession {
    /// Both sessions, in this order.
    pub const ALL: [LoadSession; 2] = [LoadSession::Gameplay, LoadSession::FrontEnd];

    /// The manifest spelling.
    pub const fn token(self) -> &'static str {
        match self {
            LoadSession::Gameplay => "gameplay",
            LoadSession::FrontEnd => "front_end",
        }
    }
}

/// One cue of an authored sound bank: a PCM16 WAV played by one single-wave group through one
/// single-track cue — the shape of retail `ui_PDA_Open_01_st` (`audio_code_map.md` §11.6). Every
/// field is a field of that group or cue, named for what the engine does with it, at the offset it
/// is written to (§11.4); all are required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundCue {
    /// The name `Sound.CueSound` is given. The cue guid is `pandemic_hash_m2(name)`.
    pub name: String,
    /// The `src/`-relative WAV: uncompressed 16-bit PCM, mono or stereo, at any rate above zero.
    pub wave: PathBuf,
    /// Group `+0x2C`: the sound instance's base volume, in dB (`FUN_0083d770`), written as the
    /// linear gain `10^(dB/20)`.
    pub group_gain_db: f64,
    /// Cue `+0x08`: the cue's gain, in dB (`FUN_00835060` multiplies the cue's volume by it each
    /// frame), written as the linear gain `10^(dB/20)`.
    pub cue_gain_db: f64,
    /// Group `+0x30`: the sound instance's base pitch, in semitones (`FUN_0083d700`).
    pub pitch_semitones: f32,
    /// Group `+0x14`: `true` plays the cue from its emitter's own source, positioned, when it has
    /// one; `false` plays it from the shared 2D source (`FUN_00837830`, `0x008378A9`).
    pub positional: bool,
    /// Group `+0x18`: full volume up to this distance from the listener (`FUN_0083d3a0`).
    pub min_distance: f32,
    /// Group `+0x1C`: silent from this distance (`FUN_0083d3a0`).
    pub max_distance: f32,
    /// Group `+0x24`: the exponent of the fall-off between the two distances (`FUN_0083d3a0`).
    pub distance_exponent: f32,
    /// Group `+0x28`: how much of the Doppler shift applies (`FUN_0083b120`).
    pub doppler_scale: f32,
    /// Cue `+0x06`, the start limit: the cue starts only while fewer than this many instances of it
    /// are playing, and 0 starts it every time (`FUN_00834ad0` compares it with a count in the cue's
    /// runtime record that `FUN_008354e0` raises when an instance plays and `FUN_00835850` lowers
    /// when one finishes).
    pub start_limit: u8,
    /// Group `+0x00`, the sound id. Its one reader is `FUN_008369e0`, which refuses to start a
    /// group whose id is `0xEA1343AA`, `0xC05D8686` or `0xBB8AE67D` unless the game runs in English;
    /// in retail it equals the guid of a cue that plays the group in 411 of `vz.wad`'s 1,776 groups
    /// and differs in 1,278.
    pub sound_id: u32,
    /// Group `+0x10`, the priority: `GetWavePriority` returns it times the wave's distance volume
    /// (`0x00837EDF`), and with every voice busy a new instance takes the voice of the lowest-priority
    /// wave only when its own priority is higher (`FUN_00837830`).
    pub priority: f32,
    /// Group `+0x20`, carried as written. No engine reader is known: it is copied into the wave
    /// (`0x00838F70`, wave `+0x68`), whose getter (wave vtable `+0x44`, `0x00838F30`) has no call
    /// site. 1.0 in every retail group but one.
    pub group_20: f32,
    /// Single-track cue `+0x16`, carried as written. No engine reader is known: the cue's `{soundbank,
    /// group}` reference is read at `+0x10` and `+0x14` only (`FUN_0082e7d0`, `FUN_0083d410`). 0 in
    /// most retail cues; a bank that carries a non-zero value carries the same one in every
    /// single-track cue.
    pub cue_16: u16,
    /// The wave record's `+0x00` clip hash, carried as written. No engine reader is known:
    /// `FUN_00837830` reads the record at `+0x05`..`+0x20` and not `+0x00`.
    pub clip_hash: u32,
}

/// Which faction vendor a shop item is offered at (`add_shop_item`). Six shops key off
/// `MrxStarter.GetFaction()`; the runtime key is Capitalized and matched by exact string, so a
/// lowercase key reaches no shop. `Pmc` is Eva's custom-vehicle shop (obscures locked items, price
/// scale forced to 1.0); the other five are outpost vendors (reputation-scaled price, and a locked
/// item there is still purchasable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopVendor {
    All,
    Chi,
    Gur,
    Oil,
    Pir,
    Pmc,
}

impl ShopVendor {
    /// The exact runtime faction key (`sFactionId` / `GetFaction()`), Capitalized.
    pub fn faction_key(self) -> &'static str {
        match self {
            ShopVendor::All => "All",
            ShopVendor::Chi => "Chi",
            ShopVendor::Gur => "Gur",
            ShopVendor::Oil => "Oil",
            ShopVendor::Pir => "Pir",
            ShopVendor::Pmc => "Pmc",
        }
    }
}

/// Which of the shop's TWO disjoint catalogs an item lives in. `MrxShop.Open` reads both the support
/// catalog (`MrxSupportData.tSupportData`, behaviour-carrying `oSupport` items) and the equipment
/// catalog (`WifEquipmentData._tEquipment`, fuel tanks / grapple) — different schemas, and different
/// reward-row fields (`tSupport` vs `tEquipment`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopCatalog {
    #[default]
    Support,
    Equipment,
}

/// `sType` — the closed enum the store icon map (`tTypeToIcon`) and reward-string markup key on. A
/// novel value renders a nil icon and no markup, so it is not a free string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopItemType {
    Airstrike,
    Supply,
    Light,
    Heavy,
    Civilian,
    Boat,
    Heli,
}

impl ShopItemType {
    pub fn lua(self) -> &'static str {
        match self {
            ShopItemType::Airstrike => "Airstrike",
            ShopItemType::Supply => "Supply",
            ShopItemType::Light => "Light",
            ShopItemType::Heavy => "Heavy",
            ShopItemType::Civilian => "Civilian",
            ShopItemType::Boat => "Boat",
            ShopItemType::Heli => "Heli",
        }
    }
}

/// The equipment `nType`. Only fuel tanks and grappling hooks are ever inserted into a shop
/// (`mrxshop` hardcodes `bIsFuelTank or bIsGrapplingHook`); the costume type exists in the enum but
/// is never shopped, so it is intentionally not offered here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopEquipmentType {
    FuelTank,
    GrapplingHook,
}

impl ShopEquipmentType {
    /// The `knType*` constant name the equipment table references.
    pub fn lua_const(self) -> &'static str {
        match self {
            ShopEquipmentType::FuelTank => "knTypeFuelTank",
            ShopEquipmentType::GrapplingHook => "knTypeGrapplingHook",
        }
    }
}

/// The `oSupport` behaviour a SUPPORT-catalog item constructs. Any support is `<module>:Create()`
/// plus optional setters — the exact shape the DLC's own catalog uses
/// (`mrxcratedelivery:Create()` → `SetCargo` / `SetDeliveryVehicle`). `module` must be an ALREADY
/// resident, imported `MrxSupport` subclass; shipping a NOVEL subclass is a separate path (a new
/// resident chunk hits the phase-8 world-load deadlock and needs the `qm_modloader` trampoline).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShopBehaviour {
    pub module: String,
    #[serde(default)]
    pub cargo: Option<String>,
    #[serde(default)]
    pub delivery_vehicle: Option<String>,
    /// A `src/`-relative Lua source for a NOVEL `MrxSupport` subclass (e.g. a new airstrike). When
    /// present, `module` is NOT assumed resident: the source is minted as a new `scripts_vz` script
    /// and `import`-ed post-world-load through the `qm_modloader` trampoline, and the catalog row is
    /// DEFERRED into that loader — because the ordinary eager append would run `module:Create()` at
    /// resident-load time, when the novel global is still `nil`, aborting a resident script (the
    /// phase-8 deadlock's cousin). Omit to reference a module the game already ships.
    ///
    /// Author-facing caveat (the linter warns): a novel-behaviour item surfaces reliably only at
    /// Eva's PMC shop — the trampoline fires on PMC-interior entry, so an outpost vendor that opens
    /// first caches its list without the item. Co-op requires both peers to install the Shipment.
    #[serde(default)]
    pub script: Option<PathBuf>,
}

fn default_shop_max_stock() -> u32 {
    99
}

/// Where an [`Contribution::AddModel`] prop's static collision comes from.
///
/// The rigid `add_model` path injects a novel mesh into a donor container but keeps the donor's
/// `PHY2` collision **verbatim** — so a new prop physically collides with the *donor's* shape, not
/// its own. This selects between that legacy behaviour and regenerating collision from the model's
/// own geometry.
///
/// Regeneration is safe on ANY donor because collision is NOT `SEGM`-bound: the engine walks the
/// self-contained `WpArray` of shapes in the `PHY2` packfile (each model-local, placed by the
/// object/cell transform), while `SEGM` is a pure RENDER draw table. So `follow_geometry` replaces
/// only the `PHY2` body and leaves `SEGM`/`INDX`/`HIER` and every render chunk byte-unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionSource {
    /// Reuse the donor's `PHY2` collision verbatim — today's behaviour, backward compatible. The new
    /// prop collides with the donor's shape (correct when the donor's footprint matches the mesh).
    #[default]
    Donor,
    /// Regenerate the static collision from THIS model's own injected mesh
    /// ([`mercs2_formats::phy2_build::build_phy2_multi`]) and splice it in, replacing the donor `PHY2`.
    /// Collision then follows the novel geometry. Rigid/static path only (the skinned `retarget` path
    /// uses ragdoll/capsule collision, out of scope).
    FollowGeometry,
}

/// Where a shader's bytecode comes from: `{asm: <path>}`, SM3 assembly text (`sm3asm` syntax) the
/// builder assembles, or `{blob: <path>}`, a compiled `vs_3_0` / `ps_3_0` blob. Both paths are
/// `src/`-relative.
///
/// Untagged over one-key structs, so it is the same one-key map in YAML, JSON and TOML, and a map
/// with both keys, or neither, matches no form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShaderSource {
    Asm(AsmSource),
    Blob(BlobSource),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsmSource {
    pub asm: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlobSource {
    pub blob: PathBuf,
}

impl ShaderSource {
    pub fn asm(path: impl Into<PathBuf>) -> ShaderSource {
        ShaderSource::Asm(AsmSource { asm: path.into() })
    }

    pub fn blob(path: impl Into<PathBuf>) -> ShaderSource {
        ShaderSource::Blob(BlobSource { blob: path.into() })
    }

    pub fn path(&self) -> &std::path::Path {
        match self {
            ShaderSource::Asm(a) => &a.asm,
            ShaderSource::Blob(b) => &b.blob,
        }
    }
}

/// One registration of an `add_shader`: the `name` the engine keys it by (`pandemic_hash_m2`), and
/// the store `stem` it loads (`<stem>.sho`, whose record ids are `<stem>_3.sho` in `shader3.bin` and
/// `<stem>_3l.sho` in `shader3Low.bin`). `shader_low` is the `shader3Low.bin` bytecode, which the
/// engine loads when the ShaderLevel setting is off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShaderClass {
    pub name: String,
    pub stem: String,
    pub shader: ShaderSource,
    pub shader_low: ShaderSource,
}

/// A cell of the 40 × 40 grid of 200 m cells TINY stand-ins are placed on: `col` =
/// `floor((x + 4000) / 200)`, `row` = `floor((z + 4000) / 200)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TinyCell {
    pub row: u32,
    pub col: u32,
}

/// One ordered, internally-tagged list. Cross-kind apply order within a Shipment is preserved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Contribution {
    /// A wardrobe outfit. **Two sides of one coin, decided by whether a `model` FILE is supplied:**
    ///
    /// * **With `model`** (a `src/`-relative model file): Data(new model) + Script(`_tOutfits` entry).
    ///   A novel mesh is injected onto a donor rig and its wardrobe row added — the custom-skin path.
    /// * **Without `model`**: Script(`_tOutfits` entry) only. `name` is the name of a model the game
    ///   ALREADY ships, so nothing is injected and no new hash is minted — the base game's own re-skin
    ///   ("wear an existing character"). `donor`/`textures`/`retarget`/`single_group` are injection-only
    ///   and ignored here.
    ///
    /// Either way the Script half is the SAME `_tOutfits` append on `wifpmcinterior`, which the linker
    /// reconciles across the installed set — so outfits of both kinds compose instead of clobbering.
    AddOutfit {
        /// ASSET identity → `pandemic_hash_m2` → `_tOutfits.Model`; what `Player.SetOutfit` receives.
        /// With `model`, this is the minted new asset's name; without it, an existing model's name.
        name: String,
        /// `_tOutfits.Name` — the unlock/tracking key. Merge key is `(wearer, slug)`, NOT `slug`
        /// alone: retail reuses `Original` and `ChickenSuit` across all three heroes.
        slug: String,
        /// `_tOutfits.PlayerVisibleName`. Localization is unresolved (Plan 04 Open-Q7).
        display: String,
        /// `_tOutfits` key: `chris` | `jennifer` | `mattias`.
        wearer: String,
        /// The `src/`-relative model FILE to inject. Omit to wear a model the game already ships
        /// (named by `name`) — no injection, no new hash.
        #[serde(default)]
        model: Option<PathBuf>,
        /// Host whose rig/materials are BORROWED — read-only, never written. Omit to auto-pick.
        /// Injection-only; ignored when `model` is omitted.
        #[serde(default)]
        donor: Option<String>,
        #[serde(default)]
        textures: Textures,
        #[serde(default)]
        retarget: Option<Retarget>,
        /// Force the whole mesh into ONE donor draw group, wearing the source's OWN retargeted
        /// weights (no donor-weight resample, no per-material split).
        ///
        /// A draw group caps at ~48 distinct bones / 8 palette ranges, so a DENSE import
        /// (`donor_transfer` resampling the retail rig can pull in 50+ bones) is otherwise forced
        /// onto the multi-group balanced split, where our injector fills a few host groups and
        /// neuters the donor's others — a donor-structure-dependent setup a foreign-rig character
        /// has been observed to render unstably (culls/teleports on camera rotation). This flag
        /// takes the proven single-host path instead: the conform maps limbs 1:1 through the
        /// convention table and fingers fold to the hand, so the source's own weights use ~half the
        /// bones and fit one group. The cost is per-material textures (one group carries one
        /// material) and the donor-resampled limb polish — accept it when placement stability
        /// matters more than skin fidelity.
        #[serde(default)]
        single_group: bool,
    },
    /// Data, new-hash additive.
    AddModel {
        name: String,
        model: PathBuf,
        /// Host whose rig/materials are BORROWED — read-only, never written. Omit to auto-pick.
        #[serde(default)]
        donor: Option<String>,
        /// The donor draw group the geometry is injected into (0..=63).
        ///
        /// Omit to let the builder pick. The Workshop's conform bench has always had this control
        /// and had nowhere to record it, so a conformed placement could be previewed and then not
        /// expressed — the transform was baked into vertices while the host group was lost.
        #[serde(default)]
        group: Option<u32>,
        /// The model's OWN skin. Empty means it wears the donor's materials, which is right for a
        /// prop and wrong for a novel mesh — the case this field exists for.
        ///
        /// Each supplied map ships as its own resident texture (`<name>_dm` / `_sm` / `_nm`) and the
        /// donor's MTRL is repointed onto it, slot by slot (diffuse, specular, normal):
        ///
        /// * with `retarget:` (skinned), every donor material's hash at that slot;
        /// * without it (rigid), the hashes the HOST `group:`'s materials name at that slot. Each of
        ///   those materials must carry the textured flag `0x0080` — a `0x0000` material is
        ///   flat-shaded and ignores bound textures — and the build refuses one that does not.
        ///
        /// A map with nothing to repoint onto is a build error rather than a texture shipped unused.
        #[serde(default)]
        textures: Textures,
        #[serde(default)]
        retarget: Option<Retarget>,
        /// Where the prop's static collision comes from. Default [`CollisionSource::Donor`] keeps the
        /// donor's `PHY2` verbatim (today's behaviour); [`CollisionSource::FollowGeometry`] regenerates
        /// it from this model's own mesh so collision follows the novel geometry. Rigid path only.
        #[serde(default)]
        collision: CollisionSource,
    },
    /// Data, new-hash additive. A standalone texture under a name the author chooses.
    ///
    /// Distinct from [`Contribution::ReplaceTexture`], which is same-hash and can only overwrite
    /// something retail already ships. Novel textures were previously expressible ONLY as the three
    /// slots inside an `add_outfit`, under a hash derived as `<outfit>_<slot>` — so a texture could
    /// not be named, shared between contributions, or exist on its own at all.
    AddTexture {
        /// ASSET identity → `pandemic_hash_m2`. This is the hash a material repoints onto.
        name: String,
        image: PathBuf,
        /// Encode as a normal map: DXT5nm with this project's `R=1, G=ny, B=1, A=nx` swizzle.
        ///
        /// Not inferable from the image — a normal map is just an RGB PNG — and getting it wrong
        /// produces lighting that is subtly inverted rather than an error, so the author declares it.
        #[serde(default)]
        normal_map: bool,
    },
    /// Data + Script. A new sound bank: its soundbank, sounddb and wavebank, encoded from the
    /// authored cues (`mercs2_audio::encode`) and shipped as one block of three entries under
    /// `pandemic_hash_m2(bank)`, the shape of every retail bank (`audio_code_map.md` §11.1). The mod
    /// loader of each session in `load_in` loads it (`MrxSoundBanks.LoadWaveBank` /
    /// `LoadSoundBank`), since a cue plays only once its bank is loaded.
    AddSound {
        /// The bank name: the entry name hash of all three tables and the name the loader loads.
        bank: String,
        /// The category every cue's group is in: a name of the game's category tree
        /// (`mercs2_audio::encode::RETAIL_CATEGORY_NAMES`).
        category: String,
        /// The cues, in bank order.
        cues: Vec<SoundCue>,
        /// The sessions whose loader loads the bank: at least one, each at most once
        /// ([`LoadSession`]). The bank's block ships to each listed session's WAD.
        load_in: Vec<LoadSession>,
    },
    /// Data, SAME-HASH. Replace a bank the game ships: its soundbank and sounddb are encoded from
    /// the authored cues and shipped under the bank's own entry name, which the game's own load of
    /// the bank reads. The cues' waves ship in a wavebank of their own, which the mod loader loads.
    /// A cue of the game's bank the replacement does not declare is gone.
    ReplaceSoundBank {
        /// The bank name, as the game's Lua loads it (`ui_hud`, `vo_mattias`).
        bank: String,
        /// For a `vo_*` bank, the language whose copy is replaced: the entry is
        /// `<bank>.<language>`. Absent for any other bank.
        #[serde(default)]
        language: Option<Language>,
        /// As [`Contribution::AddSound::category`].
        category: String,
        /// The bank's cues, in bank order.
        cues: Vec<SoundCue>,
    },
    /// Data, SAME-HASH. Replace one cue of a bank the game ships: the bank's soundbank is forked,
    /// a new single-wave group is appended, and the cue is rewritten to play it. The cue keeps its
    /// index, so the bank's own sounddb still routes to it; every other cue and group is left as
    /// the game has it. The wave ships in a wavebank of its own, which the mod loader loads.
    ReplaceSoundCue {
        /// The bank the cue is in, as the game's Lua loads it.
        bank: String,
        /// As [`Contribution::ReplaceSoundBank::language`].
        #[serde(default)]
        language: Option<Language>,
        /// The category of the new group.
        category: String,
        /// The cue: `name` is the cue it replaces.
        cue: SoundCue,
    },
    /// Data, new-hash additive. A Scaleform GFx movie (`cfx_pack`, type_id 23) added as a WAD asset,
    /// so Lua can point `SetSwfFile` at it.
    ///
    /// Shaped on [`Contribution::AddModel`] — `name` plus the artifact — rather than on the
    /// community `gfx_tool` manifest that first described this workflow. Three fields that manifest
    /// carries are deliberately absent, because each of them is a decision the author should not be
    /// making:
    ///
    /// * no `type`, because the kind IS the type. A movie is always `cfx_pack`; a `type:` field
    ///   would be a way to spell the wrong one, and the ASET row's type id decides which loader the
    ///   engine dispatches.
    /// * no `target_patch`, because the Quartermaster always emits its own overlay block. There is
    ///   no "auto" to resolve and no shipped block for an author to name.
    /// * no `donor`. `add_model` needs one because it borrows a rig and materials; a movie is
    ///   self-contained, so there is nothing to borrow and nothing to pick wrong.
    AddMovie {
        /// ASSET identity → `pandemic_hash_m2`. This is what a Lua caller passes to `SetSwfFile` /
        /// `GetShellGfxFilename`, so it is a name and not a filename — retail's are bare
        /// (`topbar`, `pause_menu`, `minimap`), with no extension and no path.
        name: String,
        /// The `.gfx` movie, `src/`-relative. Copied into the WAD **verbatim**: compressed `CFX` and
        /// uncompressed `GFX` both ship in retail, so neither is converted to the other.
        movie: PathBuf,
    },
    /// Data(movie) + Script(the Lua that shows it). The Easy face over `add_movie` + `patch_lua`.
    ///
    /// `add_movie` mints the `cfx_pack`; nothing displays it until Lua binds it to a `FlashWidget`.
    /// This kind generates that binding, so a whole custom UI element is one contribution. The
    /// engine loads a movie onto a widget by NAME — the shipped `loadingscreen_standalone` is loaded
    /// exactly this way (`mrxgui.lua`): `w = FlashWidget:new(); w:SetSwfFile(<name>); w:Play()`. It
    /// is a PROVEN capability.
    ///
    /// The show is hooked into `wifpmcinterior`'s `_OnEnter` — the moment the player enters the PMC
    /// HQ, GUI fully up, every session — because that is the ONE resident script the linker is
    /// proven to merge (`add_outfit` uses it too). A once-guard means the widget is created a single
    /// time. Where the widget sits and when it hides are the author's to tune; the generated Lua is
    /// a working default, not a finished HUD.
    AddUi {
        /// The movie asset name minted as a `cfx_pack` and passed to `SetSwfFile`.
        name: String,
        /// The `.gfx` movie, `src/`-relative — verbatim, exactly like `add_movie`.
        movie: PathBuf,
    },
    /// Data, same-hash, FULLY RESIDENT. Non-destructive means the base WAD is never modified — not
    /// that the asset's appearance is preserved.
    ReplaceTexture { target: String, image: PathBuf },
    /// Script. A DECLARED MUTATION, not a finished block: the Quartermaster links `scripts_vz`
    /// across the installed set at deploy, so two Shipments patching Lua do not annihilate.
    PatchLua { target: String, append: PathBuf },
    /// Script. Mint a WHOLE NEW Lua module the engine can `import` / `dynamic_import` by NAME.
    ///
    /// Distinct from [`PatchLua`], which appends to an existing script; this one ships a fresh
    /// script asset with its own ASET row, so `_MODULES[<name>]` starts populated the moment the
    /// engine resolves the name. Same underlying primitive the linker already uses for
    /// `qm_modloader` and a novel `add_shop_item` behaviour (`link.rs::add_script`), promoted to
    /// a first-class contribution kind so a Shipment can inject its own module — a custom mission
    /// class, a bespoke helper library, an ASI-callable table — without a runtime `dynamic_import`
    /// hook.
    ///
    /// The `name` is what the engine hashes to locate the asset — the exact string a `Lua` caller
    /// passes to `import("<name>")` / `dynamic_import("<name>")` / mrxtask's `sModuleName`. It is
    /// not a filename and carries no extension; retail's are bare (`mrxtaskcontract`,
    /// `mrxmissionflow`, `pmccon001`).
    ///
    /// The `source` is a `.lua` file, `src/`-relative, compiled to LuaQ 5.1 bytecode at build
    /// time and packed into the `scripts_vz` block. It is NOT executed at load time — Lua modules
    /// run on first import — so `inherit()` / `import()` calls at the top of the file resolve
    /// against modules that ARE loaded by then (mrxmissionflow, wifmissiondata, MrxTaskContract).
    ///
    /// ⚠ **`name` cannot collide with a shipped script.** The registry is first-writer-wins on
    /// asset hash, so a name that already exists silently drops one of the two. The linter (M0197)
    /// refuses a name it recognises in the base corpus.
    AddScript {
        /// The module name the engine `import`s. Bare, lowercase-ish, no extension, no path — see
        /// the doc comment on this variant.
        name: String,
        /// The `.lua` source file to compile, `src/`-relative.
        source: PathBuf,
    },
    /// Script. Wholesale REPLACE the bytecode of an existing shipped script, keeping its name and
    /// asset hash. The append counterpart to [`PatchLua`] and the additive counterpart to
    /// [`AddScript`]: this one takes a `.lua` file, compiles it to LuaQ, and swaps the target
    /// script's bytecode in place (`scripts_block::replace_lua`). Same asset hash means every
    /// existing `import(<target>)` call site now returns YOUR module, no rebinding required.
    ///
    /// Use when the desired change is a full rewrite rather than an append — a stock script whose
    /// structure you cannot cleanly wrap, or a Lua-side reimplementation of an engine subsystem.
    /// This is a `LastWins` claim (like `replace_texture`), so two Shipments replacing the same
    /// script is a load-order question rather than a hard conflict; the later-mounted wins and the
    /// earlier's bytecode is silently absent.
    ///
    /// ⚠ **Prefer `patch_lua` when the change is additive.** Two `patch_lua` mods on the same
    /// script COMPOSE (the linker concatenates their appends and compiles once); two `replace_lua`
    /// mods CLOBBER (the later one erases the earlier). Only reach for `replace_lua` when the
    /// change genuinely cannot be expressed as an append.
    ReplaceLua {
        /// The shipped script to replace, e.g. `wifpmcinterior`.
        target: String,
        /// The `.lua` source that becomes the new bytecode. `src/`-relative.
        source: PathBuf,
    },
    /// Data, SAME-HASH. Swap the collision (PHY2) chunk of a shipped model without touching its
    /// meshes / materials / skeleton / anything else. Uses
    /// [`mercs2_formats::phy2_container::replace_phy2_in_container`]; the model asset hash is
    /// preserved, so every entity referencing the model transparently picks up the new collision.
    ///
    /// The `phy2` file must be a fully-formed PHY2 body (Havok packfile + trailing wrapper if the
    /// donor had one), matching the shape [`build_phy2_multi`] emits. Same-hash, `Replace` intent
    /// — two mods swapping the same model's PHY2 is a load-order question, not a hard conflict.
    ReplacePhy2 {
        /// The model to swap collision on, by name or bare `0xHHHHHHHH` hash.
        target: String,
        /// A `src/`-relative PHY2 binary. Verbatim bytes.
        phy2: PathBuf,
    },
    /// Data. Add a single NEW entity to an existing placement layer (surgical add — the layer's
    /// other placements are read from the base, our entity is appended, and the whole layer's
    /// placement block is re-emitted as an overlay). The additive counterpart to `edit_world`
    /// (which patches existing entities in-place).
    ///
    /// A concrete placement carries a model hash, a transform (position + orientation quaternion),
    /// and an entity key (`u32`) — the same three fields `edit_world` edits. The build refuses a
    /// key that already exists in the target layer (M0198), because reusing a key silently
    /// overwrites the existing entity and that is not what "add" means.
    AddPlacement {
        /// The layer name (`layers_static`, `vz_state_pmccon004_pristine`, …). Used both as the
        /// carrier block (where the appended sub-block lands) AND as the COMP-scaffolding template
        /// the new sub-block clones from — the existing layer's Transform / ModelName / Name / flgs
        /// COMP layouts are cloned verbatim, then extended with the author's new entity.
        layer: String,
        /// The new entity's config, `src/`-relative — YAML with `key`, `model`, `position`,
        /// `orientation` (either `quat: [x,y,z,w]` or `yaw: <degrees>`).
        entity: PathBuf,
    },
    /// Data. Mint a WHOLE NEW placement layer, mountable as its own overlay + toggle-able via
    /// `activate_layer`. Distinct from `add_placement` (which appends to an existing layer):
    /// this one creates a layer container from scratch, hash-keyed by `pandemic_hash_m2(name)`,
    /// carrying the author's entity list.
    ///
    /// Common combo: `add_layer` mints `vz_state_myMission` from a YAML entity list, then
    /// `activate_layer` toggles it ON so the world load pages it in.
    AddLayer {
        /// The new layer name (hashed to become the ASET key).
        name: String,
        /// The existing layer whose COMP scaffolding (Transform / ModelName / Name / flgs layouts,
        /// FLGS 32-byte per-entity payload template, etc.) to clone. Every real layer has to have
        /// this scaffolding; `append_placements` clones it from a template rather than authoring
        /// COMP defaults per author. `layers_static` is the usual choice.
        template: String,
        /// `src/`-relative YAML — a list of entity records (same shape as `add_placement.entity`,
        /// one row per entity).
        entities: PathBuf,
    },
    /// Data. Add a NEW animation clip: an `animation` asset (ASET type id 16) under the clip's name.
    ///
    /// The build assembles the container every retail Havok clip uses
    /// (`mercs2_formats::anim_container`): `info` (`01 00`), `data` (the `clip` packfile, verbatim),
    /// `trnm` (the `trnm` file, verbatim) and, when `events` is given, `evnt` (verbatim). The three
    /// sources must belong together — the `trnm` count equals the clip's `numTransformTracks`, the
    /// events parse and run in time order — and M0213 checks that before anything is built.
    AddAnimation {
        /// The clip name (hashed to become the ASET key; what the engine's animation lookups use).
        name: String,
        /// A Havok 5.5 packfile carrying one `hka*Animation` — what the Havok content tools emit.
        clip: PathBuf,
        /// The track→bone binding, as the `trnm` chunk body:
        /// `[u16 count][u16 flags][u32 lead][count × u32 HIER bone name-hash]`.
        trnm: PathBuf,
        /// Optional timed events, as the `evnt` chunk body:
        /// `[u32 count]` then per event `[f32 seconds][name NUL][category NUL]`. Retail uses these
        /// for sound cues, voice lines and gameplay markers (`opendoor`). Omit for a clip with none.
        #[serde(default)]
        events: Option<PathBuf>,
    },
    /// Data, SAME-HASH. Wholesale REPLACE a shipped Havok clip, keeping its name: the same container
    /// [`Contribution::AddAnimation`] builds, under the target's hash. Every model that queries the
    /// clip by name picks up the new data on next lookup.
    ///
    /// The target must be a Havok clip. The 29 retail `animation` assets that are `MANM` keyframe
    /// animations instead are refused by name at build time — these sources cannot express one.
    ReplaceAnimation {
        /// The shipped animation to replace.
        target: String,
        /// As [`Contribution::AddAnimation::clip`].
        clip: PathBuf,
        /// As [`Contribution::AddAnimation::trnm`].
        trnm: PathBuf,
        /// As [`Contribution::AddAnimation::events`]. Omitting it ships the clip with no `evnt`,
        /// whether or not the clip it replaces had one.
        #[serde(default)]
        events: Option<PathBuf>,
    },
    /// Data + Code. Register NEW shaders in the engine's shader registry and add their bytecode to
    /// the shader stores.
    ///
    /// `family` is the record family the engine constructs for the shader: one vtable, which
    /// decides the registry (vertex or pixel), the constants the engine binds, and the draw code
    /// that reaches it. `classes` is exactly 4 entries for a pixel family, in the engine's
    /// light-class order (base, `_pl`, `_sl`, `_pl_sl`: the material's index plus the light class
    /// selects the pixel shader), or exactly 1 for a vertex family. Entries that share a `stem`
    /// share one store record, and their sources must assemble to the same bytes.
    ///
    /// The registration itself happens at runtime: the author's ASI calls the m2-sdk
    /// `shader-registry` API with the tables `qm build` writes to `<shipment>.shaders.h`, so the
    /// Shipment must `load.requires: [{capability: shader-registry}]` (M0231).
    AddShader {
        family: crate::shader::ShaderFamily,
        classes: Vec<ShaderClass>,
    },
    /// Data. Replace the bytecode of a shipped shader in the stores, in place: `target` is the
    /// registered `.sho` stem (`PgMeshVP` for `PgMeshVP.sho`). `shader` replaces its record in
    /// `shader3.bin`; `shader_low` replaces its record in `shader3Low.bin`, and is required exactly
    /// when the stem has one there. The stage comes from each source's version token and must be
    /// the record's.
    ReplaceShader {
        target: String,
        shader: ShaderSource,
        #[serde(default)]
        shader_low: Option<ShaderSource>,
    },
    /// Data. Add a NEW particle-effect entry to the fxdict, callable by its name from Lua and
    /// engine spawn sites. Pre-encoded `fxdict` payload (the sequence of tagged sub-chunks:
    /// `efct` / `emtr` / `emit` / `poff` / `trfm` / `ptyp` / `colr` / `frce` / `text` — see
    /// `mercs2_formats::fxdict`).
    AddFx {
        /// The effect name.
        name: String,
        /// The pre-encoded fxdict entry blob.
        payload: PathBuf,
    },
    /// Data, SAME-HASH. Wholesale REPLACE a shipped fx entry with a new fxdict payload.
    ReplaceFx {
        target: String,
        payload: PathBuf,
    },
    /// Data, SAME-HASH. REPLACE a single shipped terrain cell (heightmap / texturing / MOPP
    /// collision) with pre-encoded bytes. The heightmap format + MOPP-baked collision codec are
    /// only partially wrapped in `mercs2_formats::terrain`; this variant takes the fully-encoded
    /// cell as opaque bytes for authors who produced them externally (e.g., extracted, edited,
    /// re-baked via the Havok tools referenced in `memory/mopp-bake-oracle-hct-recipe.md`).
    ReplaceTerrainCell {
        /// The terrain-cell asset name (or bare hash).
        target: String,
        /// Pre-encoded cell bytes (heightmap + collision).
        cell: PathBuf,
    },
    /// Data. SWIT/STAT/CHDR/CEXE rewrite (`FUN_004cf340`, decoded).
    EditStateMachine { target: String, states: PathBuf },
    /// Data. Edit a placement LAYER (`vz_state` overlay or `layers_static`): move / rotate / re-model
    /// its entities in place. `layer` is a PTHS-path needle (`vz_state_pmccon004`, `layers_static`);
    /// `edits` is a `src/`-relative YAML of per-entity changes (extract a baseline with
    /// `qm extract-world`). Emitted as an overlay that shadows the base layer block; the
    /// `placement::patch_*` writer is proven byte-identical on a no-op across 747 retail layers.
    EditWorld { layer: String, edits: PathBuf },
    /// Data. A TINY far-distance stand-in: one model drawn in place of the world objects of one
    /// 200 m grid cell, each object's part drawn while the object is intact (role `intact`) or
    /// ruined (role `ruined`), and the `TinyGeometryObject` placement that loads it.
    ///
    /// The model is named `<layer>_tinygeometry_tgr<row>_tgc<col>_0x<key>`; the placement, keyed
    /// `key`, goes into `layer` at the cell's centre. Every primitive of `model` declares
    /// `extras.tiny_role` and every vertex a `_TINY_SLOT`, an index into `objects`.
    AddTinyGeometry {
        /// The layer the placement goes into, by name (`vz_state_mar_city_pristine`).
        layer: String,
        /// The 200 m grid cell: `row` from z, `col` from x, each 0..40.
        cell: TinyCell,
        /// The placement's entity key (GUID). No layer of the game may already use it.
        key: u32,
        /// The world objects the stand-in draws, each a bare `0xGUID` or the name of a placement in
        /// `layer`. `_TINY_SLOT` indexes this list.
        objects: Vec<String>,
        /// `src/`-relative `.glb` / `.gltf`.
        model: PathBuf,
    },
    /// Script. Turn a normally-hidden world-state layer ON — the PERMANENT, whole-mission
    /// counterpart to [`Contribution::EditWorld`]'s in-place placement edits.
    ///
    /// A `vz_state` overlay is switched at runtime by `MrxLayerManager.MarkForAddition("<layer>")`
    /// (and `MarkForRemoval` for the layer it supersedes) — the exact calls a vanilla contract makes
    /// (`OilCon001.Activated`: add `_act1`, remove `_pristine`; `mrxtaskcontractoutpost`: add
    /// captured, remove defense). The registration is baked into the Quartermaster-owned
    /// `qm_modloader` and reached by the same one-line trampoline `add_ui` uses, so N activations
    /// merge cleanly and the resident script never grows with mod count. Each mark runs under `pcall`,
    /// so a mistyped layer name cannot wedge the loader (it silently does nothing — M0194 warns).
    ActivateLayer {
        /// The layer to `MarkForAddition`, e.g. `vz_state_pmccon004_destroyed`. CASE-SENSITIVE — the
        /// name hashes to a UCFX block in the ASET, and a wrong case reaches no layer.
        layer: String,
        /// Layers to `MarkForRemoval` first — the pristine / prior overlay this one replaces. Omit to
        /// only add.
        #[serde(default)]
        replaces: Vec<String>,
    },
    /// Data, SAME-HASH. Add BRAND-NEW keys to a shipped string table.
    ///
    /// The additive companion to [`EditStringDb`]: the engine's localizer resolves `[Foo.Bar]` at
    /// render time by hashing "Foo.Bar" and looking up in the string table, so an added key is
    /// reachable the moment a widget renders text containing it. Mostly needed by mods that
    /// introduce their own names — a new mission id (`[FioDef001.Title]`), a new ability slug,
    /// a new HUD prompt.
    ///
    /// Installed together, every Shipment's additions and edits to one table are merged by `qm link`
    /// into one table, by key hash in load order; the later Shipment's text wins. To OVERRIDE an
    /// existing key's text, use [`EditStringDb`]; a mixed intent must be split into two rows.
    /// The same shell/vz duplication caveat applies as for [`EditStringDb`] (M0191 warns).
    // `snake_case` would derive `add_string_db_keys`; the kind is `add_stringdb_keys` everywhere else
    // (`kind()`, `ALL_KINDS`, the docs), so the tag is pinned, as `edit_stringdb`'s is.
    #[serde(rename = "add_stringdb_keys")]
    AddStringDbKeys {
        /// The string-table asset — `english`, `french`, `english_dlc01`, …
        target: String,
        /// A `src/`-relative file mapping bracket keys (`[Menu.Play]`) to their text, exactly the
        /// format [`EditStringDb`] takes. The build rejects any key that ALREADY exists in the
        /// target table (use `edit_stringdb` for those instead — mixing intents is a design bug).
        strings: PathBuf,
    },
    /// Data, SAME-HASH. Rewrite every string whose current text is EXACTLY `old` (fix-pack surface).
    ///
    /// A community bug report almost always names a string by the text the player sees, not by its
    /// bracket key. This kind takes a `.pairs` file — one `old<TAB>new` pair per line, `#` starting
    /// a comment line, blank lines skipped, nothing trimmed or unescaped — and rewrites every entry
    /// whose current text is exactly `old`. Requiring the FULL string match keeps this from mangling
    /// unrelated lines that merely contain the phrase. A pair whose `old` matches no entry is an
    /// error, and so is a line with no tab, more than one tab, or an empty `old`.
    ///
    /// Installed together, `qm link` merges it with every other Shipment's writes to the table, in
    /// load order: the text match runs against the table AS MERGED SO FAR (the base plus every
    /// earlier write), and a later write wins. So it composes with `edit_stringdb` /
    /// `add_stringdb_keys` on the same table, in this Shipment and others.
    // `snake_case` would derive `replace_string_db_text`; pinned to the documented tag, as
    // `edit_stringdb`'s is.
    #[serde(rename = "replace_stringdb_text")]
    ReplaceStringDbText {
        /// The string-table asset.
        target: String,
        /// A `src/`-relative pairs file: one `old<TAB>new` per line.
        pairs: PathBuf,
    },
    /// Data, SAME-HASH. Correct or localise strings in a shipped string table.
    ///
    /// The Shipment's own overlay carries ONE edited copy of the target `stringdb`, with all of this
    /// Shipment's `edit_stringdb` / `add_stringdb_keys` / `replace_stringdb_text` on that table
    /// applied in contribution order, each seeing the earlier ones' edits. Installed together, every
    /// Shipment's writes to one table are merged by `qm link` into one table, in load order, the later
    /// write winning — so editors of one table compose. The codec
    /// (`mercs2_formats::stringdb`) is proven byte-identical against all six retail language tables,
    /// and arbitrary-length edits are supported — the heap is rebuilt and the descriptors repointed.
    ///
    /// ⚠ A shared UI string (button prompts, options, PDA chrome) lives in BOTH `shell.wad` and
    /// `vz.wad`'s english table, served at different times — front end from shell, gameplay from vz
    /// (`docs/fixpack/wad_duplicate_inventory.md` §C). Editing one table is a half-fix; the linter
    /// warns when the target is one of those shared tables.
    // `snake_case` on the enum would derive `edit_string_db`; the kind tag and every doc say
    // `edit_stringdb`, matching `stringdb` everywhere else, so the tag is pinned explicitly.
    #[serde(rename = "edit_stringdb")]
    EditStringDb {
        /// The string-table asset — `english`, `french`, `english_dlc01`, …
        target: String,
        /// A `src/`-relative file mapping bracket keys (`[Menu.Play]`) to their new text.
        strings: PathBuf,
    },
    /// Data, NEW base WAD. Adds a **novel language** the install never shipped — a `.\Data\<name>.wad`
    /// the engine opens by name, carrying that language's `stringdb`.
    ///
    /// This is the one kind that places a NEW BASE WAD in `data/` rather than an overlay. It has to:
    /// the engine builds BOTH the mounted filename (`.\Data\<name>.wad`) AND the stringdb key
    /// (`pandemic_hash_m2(name) × 0x39E5E978`) from the same language-name string, and a missing base
    /// `<name>.wad` is a hard `exit(1)` (`FUN_004bfe20`). So a new selectable language is a new base
    /// WAD, which no overlay kind can express. The `data/` write is safe by CONSTRUCTION — the
    /// filename is builder-derived from `name` and refused if it collides with a WAD the game already
    /// ships (`build::language_name_refusal`), so it can only ever ADD, never shadow `vz.wad` or a
    /// shipped language.
    ///
    /// Selection is a SEPARATE concern, and not this kind's: PC has no in-game language selector (the
    /// language is chosen at boot from OS-locale), and switching the game into an installed language
    /// is handled by Modkit. `add_language` ships the CONTENT only. The RE is in
    /// `docs/reverse_engineer/language_asi_hook_contract.md`.
    AddLanguage {
        /// The language name → the mounted `.\Data\<name>.wad` filename AND `pandemic_hash_m2(name)`
        /// for the stringdb key. A lowercase `[a-z0-9_]` token (it becomes a filename), and never a
        /// name the game already ships — both enforced by `build::language_name_refusal` / M0200.
        name: String,
        /// The label a selector UI (Modkit) shows for this language. Metadata — it is not lowered into
        /// the WAD, because the engine has no selector to read it.
        display: String,
        /// A `src/`-relative translation file, one edit per line (`[Menu.Play] = text`), exactly the
        /// format [`Contribution::EditStringDb`] takes. The build forks `base` and applies these; keys
        /// left untranslated keep the base text.
        strings: PathBuf,
        /// The shipped string table to fork as the starting point. Omit for `english`.
        #[serde(default)]
        base: Option<String>,
    },
    /// Code. Retail: a prebuilt ASI placed in `pmc_bb.dll`'s search path. To DEPEND on someone
    /// else's ASI, require the Shipment that ships it in `load.requires` — never vendor a
    /// third-party binary. `dest` is deliberately absent: the author cannot name a path, so the exe
    /// and `vz.wad` stay unreachable by construction.
    NativeHook {
        target: Target,
        #[serde(default)]
        plugin: Option<PathBuf>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        touches: Vec<Touch>,
        /// Byte-signature guard for each `touches` address. When set, the plugin is expected to
        /// verify each address's leading N bytes against the given hex signature at load-time and
        /// bail (leaving the exe untouched) on mismatch — the pattern the working ASIs already
        /// follow by hand (`anim_table_expand.asi`, `dontcry_native.asi`). Recording it in the
        /// manifest lets the linter emit a M0199 when a plugin's declared `touches` list drifts
        /// from a sig verified against a known engine build.
        ///
        /// Map keys are `touches` addresses in `0xHHHHHHHH` hex; values are hex-encoded prologue
        /// bytes (`"55 8B EC 51 53 56 57"` etc.). Missing address = no guard for that touch.
        #[serde(default)]
        signature_guard: std::collections::BTreeMap<String, String>,
    },
    /// Code. A companion FILE placed in the game folder beside the plugins that read it.
    ///
    /// The gap this closes: an `.asi` whose `.ini` cannot ship is useless. Every real Code-layer
    /// mod measured here is a plugin PLUS companions — `quiet_freeplay_vo.asi` +
    /// `quiet_freeplay_vo.ini`, `multiplayer_restore.asi` + `multiplayer_restore.ini` — and a Lua
    /// framework ships only `.lua` files, with no `.asi` of its own at all. Neither was expressible.
    ///
    /// Two fields, and neither is a path into the game:
    ///
    /// * `file` is a `src/`-relative source path, checked by exactly the same rules as every other
    ///   source (absolute, `..` and outward symlinks are all M0111 errors). It supplies the BYTES
    ///   and the FILENAME; an author cannot rename on the way out, so the reserved-name refusal
    ///   `native_hook` already carries applies unchanged.
    /// * `dest` is a [`PlaceIn`] — a name from a closed set, never a path.
    ///
    /// An `.asi` is deliberately NOT placeable this way: it goes through
    /// [`Contribution::NativeHook`], which reads the PE headers the loader will `LoadLibrary` and
    /// records the hooked addresses. Letting a companion be a plugin would be a way around both.
    PlaceFile { file: PathBuf, dest: PlaceIn },
    /// Code. A runtime DLL placed in the game root, where the plugins that import it by name find
    /// it (the directory of `Mercenaries2.exe` is the first place Windows searches).
    ///
    /// Three rules keep this from being a way to overwrite the game's own DLLs or the loader:
    ///
    /// * the file name must be `<shipment.name>.dll`, compared lowercased — so a runtime Shipment
    ///   ships exactly one DLL, named after itself;
    /// * its stem must not be on [`DENY_LISTED_DLL_STEMS`] (which also makes those stems reserved
    ///   Shipment names, M0211);
    /// * it must be a loadable i386 PE DLL.
    ///
    /// `dll` is a `src/`-relative source path. There is no destination field and no rename: the
    /// destination is always the game root and the placed name is the source file's name.
    AddRuntimeDll { dll: PathBuf },
    /// Script (composed). A purchasable item added to one or more faction shops.
    ///
    /// Delivered as LINKED APPENDS onto the resident catalog scripts, never a block replace: a
    /// full-block replace of the resident script block is `Exclusive`/last-wins and silently
    /// annihilates a second shop mod — the exact failure `patch_lua` and the linker exist to
    /// prevent. `catalog: support` appends a `tSupportData` row to `mrxsupportdata`;
    /// `catalog: equipment` appends a `_tEquipment` row to `wifequipmentdata`. Either way a reward
    /// row per listed vendor faction is appended to `mrxrewarddata`, which is the only source
    /// `MrxShop.Open` reads (`GetAllPotentialShopItems(<faction>)`).
    ///
    /// Dependencies the author owns: `name`/`description` that are `[stringdb tokens]` need a
    /// companion `edit_stringdb` (an unresolved token renders raw); `icon` is an atlas key (a novel
    /// icon renders blank); a support `behaviour.cargo` names a spawnable template that must already
    /// exist. Runtime faction keys are Capitalized on emit.
    AddShopItem {
        /// Catalog id → `tSupportData`/`_tEquipment` key and the reward id.
        id: String,
        /// `sName` — a stringdb token (`[vehicle.m1a1]`) or a literal.
        name: String,
        /// `sDescription` — token or literal.
        #[serde(default)]
        description: String,
        /// `sIcon` (support) / `sTexture` (equipment) — an atlas key.
        icon: String,
        /// The vendor shop(s) this item is offered at — one reward row is emitted per vendor.
        shops: Vec<ShopVendor>,
        /// Which catalog. Default `support`.
        #[serde(default)]
        catalog: ShopCatalog,
        /// `sType` (support catalog). Needed for a support item to get an icon + reward markup.
        #[serde(rename = "type", default)]
        item_type: Option<ShopItemType>,
        /// `nCashCost` (support) / `nCost` (equipment).
        #[serde(default)]
        cash_cost: u64,
        /// `nFuelCost` (support only).
        #[serde(default)]
        fuel_cost: u64,
        /// `nMaxStock` (support). Capped at 99 by `Init`.
        #[serde(default = "default_shop_max_stock")]
        max_stock: u32,
        /// `tUnlockStatus` — unlocked in every listed vendor. Required in Eva's obscured shop, where
        /// a locked item is unbuyable.
        #[serde(default)]
        unlocked: bool,
        /// The `oSupport` behaviour (support catalog). Omit for equipment.
        #[serde(default)]
        behaviour: Option<ShopBehaviour>,
        /// The equipment `nType` (equipment catalog). Omit for support.
        #[serde(default)]
        equipment_type: Option<ShopEquipmentType>,
    },
    /// The OPEN LOWER BOUND — opaque payload plus a DECLARED blast radius, so the linter and the
    /// conflict system can reason without understanding the bytes.
    Raw {
        #[serde(default)]
        description: Option<String>,
        payload: PathBuf,
        target_layer: Layer,
        touches: Vec<Touch>,
    },
}

impl Contribution {
    /// Every `kind` tag the format knows — the ONE list downstream surfaces check themselves against.
    ///
    /// This exists because a kind can be fully implemented and still be unreachable. The Workshop's
    /// add-menu is a hand-written table, and `edit_state_machine` was absent from it: the format
    /// parsed it, `blast` claimed for it, the linter had rules about it, and there was no way to add
    /// one from the UI. Nothing failed — it simply was not offered.
    ///
    /// Keeping the list here does not make it self-updating (Rust cannot enumerate variants), but it
    /// makes ONE place authoritative, and the conformance and UI tests assert against it rather than
    /// against private copies that drift independently.
    pub const ALL_KINDS: &'static [&'static str] = &[
        "add_outfit",
        "add_model",
        "add_texture",
        "add_sound",
        "replace_sound_bank",
        "replace_sound_cue",
        "add_movie",
        "add_ui",
        "replace_texture",
        "patch_lua",
        "add_script",
        "replace_lua",
        "replace_phy2",
        "add_placement",
        "add_layer",
        "add_animation",
        "replace_animation",
        "add_shader",
        "replace_shader",
        "add_fx",
        "replace_fx",
        "replace_terrain_cell",
        "edit_state_machine",
        "edit_world",
        "add_tiny_geometry",
        "activate_layer",
        "edit_stringdb",
        "add_stringdb_keys",
        "replace_stringdb_text",
        "add_language",
        "native_hook",
        "place_file",
        "add_runtime_dll",
        "add_shop_item",
        "raw",
    ];

    /// Kinds the format once had and no longer does, each with the reason. A manifest naming one
    /// fails to parse with that reason ([`crate::ReadError::RemovedKind`]) rather than with serde's
    /// bare "unknown variant", so an author learns the kind is gone rather than misspelled.
    pub const REMOVED_KINDS: &'static [(&'static str, &'static str)] = &[
        (
            "add_ai_squad_template",
            "removed in qm 3.1.0. It shipped author bytes under an author-supplied type id and \
             type hash for an AI squad asset whose format and type id have not been \
             reverse-engineered, so nothing about it could be checked. Opaque bytes with a \
             declared blast radius are what `raw` is for.",
        ),
        (
            "add_schema",
            "removed in qm 3.1.0. A schema is the column layout of a component table inside a \
             placement layer, not a standalone asset, and nothing loads one on its own.",
        ),
    ];

    /// The kind tag as written in the manifest — for diagnostics that must name it back to the author.
    pub fn kind(&self) -> &'static str {
        match self {
            Contribution::AddOutfit { .. } => "add_outfit",
            Contribution::AddModel { .. } => "add_model",
            Contribution::AddTexture { .. } => "add_texture",
            Contribution::AddSound { .. } => "add_sound",
            Contribution::ReplaceSoundBank { .. } => "replace_sound_bank",
            Contribution::ReplaceSoundCue { .. } => "replace_sound_cue",
            Contribution::AddMovie { .. } => "add_movie",
            Contribution::AddUi { .. } => "add_ui",
            Contribution::ReplaceTexture { .. } => "replace_texture",
            Contribution::PatchLua { .. } => "patch_lua",
            Contribution::AddScript { .. } => "add_script",
            Contribution::ReplaceLua { .. } => "replace_lua",
            Contribution::ReplacePhy2 { .. } => "replace_phy2",
            Contribution::AddPlacement { .. } => "add_placement",
            Contribution::AddLayer { .. } => "add_layer",
            Contribution::AddAnimation { .. } => "add_animation",
            Contribution::ReplaceAnimation { .. } => "replace_animation",
            Contribution::AddShader { .. } => "add_shader",
            Contribution::ReplaceShader { .. } => "replace_shader",
            Contribution::AddFx { .. } => "add_fx",
            Contribution::ReplaceFx { .. } => "replace_fx",
            Contribution::ReplaceTerrainCell { .. } => "replace_terrain_cell",
            Contribution::EditStateMachine { .. } => "edit_state_machine",
            Contribution::EditWorld { .. } => "edit_world",
            Contribution::AddTinyGeometry { .. } => "add_tiny_geometry",
            Contribution::ActivateLayer { .. } => "activate_layer",
            Contribution::EditStringDb { .. } => "edit_stringdb",
            Contribution::AddStringDbKeys { .. } => "add_stringdb_keys",
            Contribution::ReplaceStringDbText { .. } => "replace_stringdb_text",
            Contribution::AddLanguage { .. } => "add_language",
            Contribution::NativeHook { .. } => "native_hook",
            Contribution::PlaceFile { .. } => "place_file",
            Contribution::AddRuntimeDll { .. } => "add_runtime_dll",
            Contribution::AddShopItem { .. } => "add_shop_item",
            Contribution::Raw { .. } => "raw",
        }
    }
}

/// Why a manifest was rejected. Every variant is loud by design — a silent mis-parse is the failure
/// mode the format most wants to avoid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidateError {
    /// The manifest declares any format other than [`FORMAT_VERSION`] — older or newer.
    UnsupportedFormat {
        found: u32,
    },
    /// `target: both` — reserved, rejected in v1.
    TargetBothReserved,
    EmptyName,
    NameTooLong {
        len: usize,
    },
    /// `shipment.name`, lowercased, is a deny-listed DLL stem ([`DENY_LISTED_DLL_STEMS`]).
    ReservedName {
        name: String,
    },
    NameNotSlug {
        name: String,
    },
    /// `shipment.version` is not a semver version.
    VersionNotSemver {
        version: String,
        error: String,
    },
    /// A version range does not parse as a `semver::VersionReq`. `at` names the field.
    BadRange {
        at: String,
        range: String,
        error: String,
    },
    /// A `{ name, version }` requirement — not a form this format has.
    CompatibleForm {
        at: String,
        name: String,
        version: String,
    },
    /// A Shipment name in `requires` / `conflicts` is not a slug, so it can name no Shipment.
    ReferenceNotSlug {
        at: String,
        name: String,
    },
    /// `requires` / `conflicts` names this Shipment itself.
    SelfReference {
        at: String,
        name: String,
    },
    /// A `supersedes` entry's `file` is not a single filename.
    SupersededNotAFilename {
        at: String,
        file: String,
        why: String,
    },
}

impl ValidateError {
    /// The rule code this failure is reported under, for the failures that have one of their own.
    /// `None` is the generic "manifest fails schema validation" (lint reports it as M0100).
    pub fn code(&self) -> Option<&'static str> {
        match self {
            ValidateError::BadRange { .. } => Some("M0172"),
            ValidateError::SelfReference { .. } => Some("M0173"),
            ValidateError::ReservedName { .. } => Some("M0211"),
            ValidateError::UnsupportedFormat { .. }
            | ValidateError::TargetBothReserved
            | ValidateError::EmptyName
            | ValidateError::NameTooLong { .. }
            | ValidateError::NameNotSlug { .. }
            | ValidateError::VersionNotSemver { .. }
            | ValidateError::CompatibleForm { .. }
            | ValidateError::ReferenceNotSlug { .. }
            | ValidateError::SupersededNotAFilename { .. } => None,
        }
    }
}

impl std::fmt::Display for ValidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidateError::UnsupportedFormat { found } => write!(
                f,
                "manifest declares format {found}, but the only manifest format is \
                 {FORMAT_VERSION} — refusing to guess. Write `format: {FORMAT_VERSION}`."
            ),
            ValidateError::TargetBothReserved => write!(
                f,
                "target: both is reserved and rejected in v1 — split-vs-shared semantics are \
                 undecided. Use `retail` or `reimpl`."
            ),
            ValidateError::EmptyName => write!(f, "shipment.name is empty"),
            ValidateError::NameTooLong { len } => write!(
                f,
                "shipment.name is {len} chars; the limit is {MAX_NAME_LEN} (it becomes _build/<name>.wad)"
            ),
            ValidateError::ReservedName { name } => write!(
                f,
                "shipment.name {name:?} is reserved: it is the name of a DLL no Shipment may ship \
                 ({}), compared case-insensitively. Pick another name.",
                DENY_LISTED_DLL_STEMS.join(", ")
            ),
            ValidateError::NameNotSlug { name } => write!(
                f,
                "shipment.name {name:?} is not a slug — expected ^[a-z0-9]+(-[a-z0-9]+)*$ \
                 (lowercase, digits, single hyphens, no leading/trailing hyphen)"
            ),
            ValidateError::VersionNotSemver { version, error } => write!(
                f,
                "shipment.version {version:?} is not a semver version ({error}) — write \
                 MAJOR.MINOR.PATCH, e.g. 1.0.0"
            ),
            ValidateError::BadRange { at, range, error } => write!(
                f,
                "{at}: {range:?} is not a valid semver range ({error}). Use a range resolution can \
                 compare against — e.g. \"^1.0.0\" or \">=0.7.0, <1.0.0\"."
            ),
            ValidateError::CompatibleForm { at, name, version } => write!(
                f,
                "{at}: `{{ name, version }}` is not a requirement form. Write \
                 `{{ shipment: {name}, version: \"{version}\" }}`."
            ),
            ValidateError::ReferenceNotSlug { at, name } => write!(
                f,
                "{at}: {name:?} is not a Shipment name — Shipment names are slugs, \
                 ^[a-z0-9]+(-[a-z0-9]+)*$"
            ),
            ValidateError::SelfReference { at, name } => write!(
                f,
                "{at}: names this Shipment itself ({name:?}). A Shipment cannot require or \
                 conflict with itself."
            ),
            ValidateError::SupersededNotAFilename { at, file, why } => write!(
                f,
                "{at}: file {file:?} is not a single filename: {why}"
            ),
        }
    }
}

impl std::error::Error for ValidateError {}

/// `^[a-z0-9]+(-[a-z0-9]+)*$`, hand-rolled to avoid a regex dependency for one pattern. The rule for
/// `shipment.name` and for every Shipment name `requires` / `conflicts` refers to.
pub fn is_slug(s: &str) -> bool {
    if s.is_empty() || s.starts_with('-') || s.ends_with('-') || s.contains("--") {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Parse a version range the one way qm parses every range: `semver::VersionReq`, the grammar
/// Cargo uses. Manifest validation (M0172) and `qm check-range` both come through here, so a range
/// is valid in a manifest exactly when it is valid anywhere else.
pub fn parse_range(range: &str) -> Result<semver::VersionReq, String> {
    semver::VersionReq::parse(range).map_err(|e| e.to_string())
}

/// A semver range check, reported under M0172 with the field it came from.
fn check_range(at: String, range: &str) -> Result<(), ValidateError> {
    parse_range(range)
        .map(|_| ())
        .map_err(|error| ValidateError::BadRange {
            at,
            range: range.to_string(),
            error,
        })
}

/// A Shipment name referenced from `requires` / `conflicts`: a slug, and not this Shipment.
fn check_reference(at: String, name: &str, own: &str) -> Result<(), ValidateError> {
    if !is_slug(name) {
        return Err(ValidateError::ReferenceNotSlug {
            at,
            name: name.to_string(),
        });
    }
    if name == own {
        return Err(ValidateError::SelfReference {
            at,
            name: name.to_string(),
        });
    }
    Ok(())
}

impl Manifest {
    /// Schema-level checks that do not need the filesystem or a game install — so this runs in CI.
    pub fn validate(&self) -> Result<(), ValidateError> {
        // Format gate FIRST: another format may mean anything, so no other check is meaningful.
        // There is one format; older and newer are both refused.
        if self.format != FORMAT_VERSION {
            return Err(ValidateError::UnsupportedFormat { found: self.format });
        }
        if self.shipment.target == Target::Both {
            return Err(ValidateError::TargetBothReserved);
        }
        let name = &self.shipment.name;
        if name.is_empty() {
            return Err(ValidateError::EmptyName);
        }
        if name.len() > MAX_NAME_LEN {
            return Err(ValidateError::NameTooLong { len: name.len() });
        }
        // Before the slug check: `pmc_bb` and a mixed-case `Cruise` are not slugs either, and the
        // reserved-name refusal is the one that says why the name cannot be used.
        let lowered = name.to_ascii_lowercase();
        if DENY_LISTED_DLL_STEMS.contains(&lowered.as_str()) {
            return Err(ValidateError::ReservedName { name: name.clone() });
        }
        if !is_slug(name) {
            return Err(ValidateError::NameNotSlug { name: name.clone() });
        }
        if let Err(e) = semver::Version::parse(&self.shipment.version) {
            return Err(ValidateError::VersionNotSemver {
                version: self.shipment.version.clone(),
                error: e.to_string(),
            });
        }
        if let Some(range) = &self.shipment.quartermaster {
            check_range("shipment.quartermaster".into(), range)?;
        }
        for (i, req) in self.load.requires.iter().enumerate() {
            let at = format!("load.requires[{i}]");
            match req {
                Requirement::Shipment(target) => check_reference(at, target, name)?,
                Requirement::ShipmentRange(r) => {
                    check_reference(at.clone(), &r.shipment, name)?;
                    check_range(at, &r.version)?;
                }
                Requirement::Capability(_) => {}
                Requirement::Compatible(c) => {
                    return Err(ValidateError::CompatibleForm {
                        at,
                        name: c.name.clone(),
                        version: c.version.clone(),
                    })
                }
            }
        }
        for (i, decl) in self.load.conflicts.iter().enumerate() {
            let at = format!("load.conflicts[{i}]");
            check_reference(at.clone(), decl.name(), name)?;
            if let Some(range) = decl.range() {
                check_range(at, range)?;
            }
        }
        for (i, s) in self.supersedes.iter().enumerate() {
            if let Some(why) = crate::build::single_filename_refusal(&s.file) {
                return Err(ValidateError::SupersededNotAFilename {
                    at: format!("supersedes[{i}]"),
                    file: s.file.clone(),
                    why,
                });
            }
        }
        Ok(())
    }
}
