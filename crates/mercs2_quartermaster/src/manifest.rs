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

/// Schema version this build understands. A manifest declaring a NEWER version is a loud reject;
/// an older one is accepted (see [`Manifest::validate`]).
pub const FORMAT_VERSION: u32 = 1;

/// Maximum length of `shipment.name` — it becomes the output filename `build/<name>.wad`.
pub const MAX_NAME_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub shipment: Shipment,
    #[serde(default)]
    pub load: Load,
    #[serde(default)]
    pub contributions: Vec<Contribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shipment {
    /// Slug. `^[a-z0-9]+(-[a-z0-9]+)*$`, <= [`MAX_NAME_LEN`]. Unique; used by deps AND as the
    /// output filename.
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub target: Target,
    /// Minimum `mercs2_quartermaster` that can build this.
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
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub before: Vec<String>,
    /// Hard deps — build fails if absent. Cross-shipment references are COMPUTED (read-set); this
    /// field carries only what the Quartermaster cannot infer.
    #[serde(default)]
    pub requires: Vec<Requirement>,
    #[serde(default)]
    pub conflicts: Vec<String>,
}

/// A hard dependency, in one of three forms:
///
/// * `Shipment(name)` — another Shipment, by name.
/// * `Compatible { name, version }` — a Modkit-MANAGED component (e.g. `m2-sdk`) at a semver
///   RANGE (`"^0.1"`). Resolved like a package manager: Modkit installs the highest released
///   version that satisfies the range, ONE copy shared across every Shipment that needs it. This
///   is the form for a first-party managed runtime — it updates on its own cadence and two mods
///   can share a single install, neither of which a byte-exact pin allows.
/// * `External { url, sha256 }` — a third-party artifact pinned by digest. For an ASI with no
///   managed component and no semver contract: the digest makes the reference tamper-evident (see
///   the spec's trust discussion — integrity, not authenticity). Do NOT reach for it to lock a
///   managed component to one build; that defeats resolution and freezes the dependency.
///
/// Untagged and unambiguous: a bare string is a `Shipment`; `{name,version}` is `Compatible`;
/// `{url,sha256}` is `External` — disjoint required keys, so serde cannot confuse them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Requirement {
    Shipment(String),
    Compatible { name: String, version: String },
    External { url: String, sha256: String },
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

/// Which audio table a bank is.
///
/// A closed set of three, because the ASET type id decides which loader the engine dispatches
/// and there is nothing safe to guess. All three are opaque `data` wrappers in retail —
/// `soundbank` 98/98, `sounddb` 58/58, `wavebank` 92/93 — measured in
/// `mercs2_formats/tests/novel_asset_shape_survey.rs`.
///
/// The bytes are copied VERBATIM: this crate has no encoder for any of them, and swapping an
/// author's working bank for one nobody has run is the kind of helpfulness that produces a WAD
/// which looks fine and does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundKind {
    Wavebank,
    Soundbank,
    Sounddb,
}

impl SoundKind {
    /// The ASET `type_id` and UCFX `type_hash` the engine dispatches on.
    pub fn ids(self) -> (u32, u32) {
        use mercs2_formats::types::*;
        match self {
            SoundKind::Wavebank => (TYPE_ID_WAVEBANK, TYPE_HASH_WAVEBANK),
            SoundKind::Soundbank => (TYPE_ID_SOUNDBANK, TYPE_HASH_SOUNDBANK),
            // No constant for sounddb in `types`; the pair comes from `aset_type_ids`, which is
            // the registry the rest of the workspace reads.
            SoundKind::Sounddb => (13, 0xE527_3C14),
        }
    }
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

/// One ordered, internally-tagged list. Cross-kind apply order within a Shipment is preserved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        #[serde(default)]
        textures: Textures,
        #[serde(default)]
        retarget: Option<Retarget>,
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
    /// Data, new-hash additive. An audio bank under a name the author chooses.
    ///
    /// Expressible because the container turned out to be an opaque `data` wrapper — the same shape
    /// `add_movie` already shipped — rather than because anything here understands audio. The
    /// survey measured it across the whole archive: `soundbank` 98/98, `sounddb` 58/58 and
    /// `wavebank` 92/93 are bare `data`, which is 248 assets and ~366 MB of retail content that had
    /// no way into a Shipment at all.
    ///
    /// The bytes ship VERBATIM; nothing here encodes or validates them, so `bank` must already be a
    /// table the game accepts.
    AddSound {
        /// ASSET identity → `pandemic_hash_m2`.
        name: String,
        bank: PathBuf,
        /// Which table this is. Not inferable from the bytes, and the type id decides which loader
        /// runs, so the author declares it.
        sound: SoundKind,
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
    /// Data. SWIT/STAT/CHDR/CEXE rewrite (`FUN_004cf340`, decoded).
    EditStateMachine { target: String, states: PathBuf },
    /// Data. Edit a placement LAYER (`vz_state` overlay or `layers_static`): move / rotate / re-model
    /// its entities in place. `layer` is a PTHS-path needle (`vz_state_pmccon004`, `layers_static`);
    /// `edits` is a `src/`-relative YAML of per-entity changes (extract a baseline with
    /// `qm extract-world`). Emitted as an overlay that shadows the base layer block; the
    /// `placement::patch_*` writer is proven byte-identical on a no-op across 747 retail layers.
    EditWorld { layer: String, edits: PathBuf },
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
    /// Data, SAME-HASH. Correct or localise strings in a shipped string table.
    ///
    /// Same-hash and last-wins, like [`Contribution::ReplaceTexture`]: the overlay carries an
    /// edited copy of the target `stringdb` and the mount order decides which wins. The codec
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
    /// Selection is a SEPARATE concern: PC has no in-game language selector (the language is chosen at
    /// boot from OS-locale), so a companion `native_hook` — the language-selector `.asi` — forces the
    /// index. `add_language` ships the CONTENT; pairing it with that plugin is what makes the language
    /// reachable (M0201 warns when it is missing). The RE and the plugin contract are in
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
    /// else's ASI use `load.requires` with a pinned digest instead — never vendor a third-party
    /// binary. `dest` is deliberately absent: the author cannot name a path, so the exe and
    /// `vz.wad` stay unreachable by construction.
    NativeHook {
        target: Target,
        #[serde(default)]
        plugin: Option<PathBuf>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        touches: Vec<Touch>,
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
        "add_movie",
        "add_ui",
        "replace_texture",
        "patch_lua",
        "edit_state_machine",
        "edit_world",
        "activate_layer",
        "edit_stringdb",
        "add_language",
        "native_hook",
        "place_file",
        "add_shop_item",
        "raw",
    ];

    /// The kind tag as written in the manifest — for diagnostics that must name it back to the author.
    pub fn kind(&self) -> &'static str {
        match self {
            Contribution::AddOutfit { .. } => "add_outfit",
            Contribution::AddModel { .. } => "add_model",
            Contribution::AddTexture { .. } => "add_texture",
            Contribution::AddSound { .. } => "add_sound",
            Contribution::AddMovie { .. } => "add_movie",
            Contribution::AddUi { .. } => "add_ui",
            Contribution::ReplaceTexture { .. } => "replace_texture",
            Contribution::PatchLua { .. } => "patch_lua",
            Contribution::EditStateMachine { .. } => "edit_state_machine",
            Contribution::EditWorld { .. } => "edit_world",
            Contribution::ActivateLayer { .. } => "activate_layer",
            Contribution::EditStringDb { .. } => "edit_stringdb",
            Contribution::AddLanguage { .. } => "add_language",
            Contribution::NativeHook { .. } => "native_hook",
            Contribution::PlaceFile { .. } => "place_file",
            Contribution::AddShopItem { .. } => "add_shop_item",
            Contribution::Raw { .. } => "raw",
        }
    }
}

/// Why a manifest was rejected. Every variant is loud by design — a silent mis-parse is the failure
/// mode the format most wants to avoid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidateError {
    /// The manifest declares a schema version this build does not know.
    FutureFormat {
        found: u32,
        known: u32,
    },
    /// `target: both` — reserved, rejected in v1.
    TargetBothReserved,
    EmptyName,
    NameTooLong {
        len: usize,
    },
    NameNotSlug {
        name: String,
    },
}

impl std::fmt::Display for ValidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidateError::FutureFormat { found, known } => write!(
                f,
                "manifest declares format {found}, but this Quartermaster only knows up to {known} — \
                 refusing to guess. Upgrade the Quartermaster."
            ),
            ValidateError::TargetBothReserved => write!(
                f,
                "target: both is reserved and rejected in v1 — split-vs-shared semantics are \
                 undecided. Use `retail` or `reimpl`."
            ),
            ValidateError::EmptyName => write!(f, "shipment.name is empty"),
            ValidateError::NameTooLong { len } => write!(
                f,
                "shipment.name is {len} chars; the limit is {MAX_NAME_LEN} (it becomes build/<name>.wad)"
            ),
            ValidateError::NameNotSlug { name } => write!(
                f,
                "shipment.name {name:?} is not a slug — expected ^[a-z0-9]+(-[a-z0-9]+)*$ \
                 (lowercase, digits, single hyphens, no leading/trailing hyphen)"
            ),
        }
    }
}

impl std::error::Error for ValidateError {}

/// `^[a-z0-9]+(-[a-z0-9]+)*$`, hand-rolled to avoid a regex dependency for one pattern.
fn is_slug(s: &str) -> bool {
    if s.is_empty() || s.starts_with('-') || s.ends_with('-') || s.contains("--") {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Manifest {
    /// Schema-level checks that do not need the filesystem or a game install — so this runs in CI.
    pub fn validate(&self) -> Result<(), ValidateError> {
        // Version gate FIRST: a newer manifest may mean anything, so no other check is meaningful.
        // Direction matters — NEWER than known is the reject; older is accepted.
        if self.format > FORMAT_VERSION {
            return Err(ValidateError::FutureFormat {
                found: self.format,
                known: FORMAT_VERSION,
            });
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
        if !is_slug(name) {
            return Err(ValidateError::NameNotSlug { name: name.clone() });
        }
        Ok(())
    }
}
