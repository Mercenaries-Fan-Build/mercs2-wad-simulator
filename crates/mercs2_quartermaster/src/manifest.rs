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

/// A hard dependency: either another Shipment by name, or an EXTERNAL artifact pinned by digest.
///
/// The external form exists so a Shipment can depend on a third-party ASI (published on a GitHub
/// release, which carries the hash) **without vendoring someone else's binary**. Pinning the digest
/// is what makes the reference tamper-evident — see the trust discussion in the spec: this is
/// integrity, not authenticity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Requirement {
    Shipment(String),
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

/// Optional cross-rig retarget on an import that is not already hero-rigged. Inline rather than a
/// standalone kind so v1 avoids inter-contribution reference machinery entirely (Plan 04 Q6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retarget {
    pub from: String,
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

/// One ordered, internally-tagged list. Cross-kind apply order within a Shipment is preserved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Contribution {
    /// Data(new model) + Script(`_tOutfits` entry).
    AddOutfit {
        /// ASSET identity → `pandemic_hash_m2` → `_tOutfits.Model`; what `Player.SetOutfit` receives.
        name: String,
        /// `_tOutfits.Name` — the unlock/tracking key. Merge key is `(wearer, slug)`, NOT `slug`
        /// alone: retail reuses `Original` and `ChickenSuit` across all three heroes.
        slug: String,
        /// `_tOutfits.PlayerVisibleName`. Localization is unresolved (Plan 04 Open-Q7).
        display: String,
        /// `_tOutfits` key: `chris` | `jennifer` | `mattias`.
        wearer: String,
        model: PathBuf,
        /// Host whose rig/materials are BORROWED — read-only, never written. Omit to auto-pick.
        #[serde(default)]
        donor: Option<String>,
        #[serde(default)]
        textures: Textures,
        #[serde(default)]
        retarget: Option<Retarget>,
    },
    /// Data, new-hash additive.
    AddModel {
        name: String,
        model: PathBuf,
        #[serde(default)]
        donor: Option<String>,
        #[serde(default)]
        retarget: Option<Retarget>,
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
    /// Data, same-hash, FULLY RESIDENT. Non-destructive means the base WAD is never modified — not
    /// that the asset's appearance is preserved.
    ReplaceTexture { target: String, image: PathBuf },
    /// Script. A DECLARED MUTATION, not a finished block: the Quartermaster links `scripts_vz`
    /// across the installed set at deploy, so two Shipments patching Lua do not annihilate.
    PatchLua { target: String, append: PathBuf },
    /// Data. SWIT/STAT/CHDR/CEXE rewrite (`FUN_004cf340`, decoded).
    EditStateMachine { target: String, states: PathBuf },
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
    /// The kind tag as written in the manifest — for diagnostics that must name it back to the author.
    pub fn kind(&self) -> &'static str {
        match self {
            Contribution::AddOutfit { .. } => "add_outfit",
            Contribution::AddModel { .. } => "add_model",
            Contribution::AddMovie { .. } => "add_movie",
            Contribution::ReplaceTexture { .. } => "replace_texture",
            Contribution::PatchLua { .. } => "patch_lua",
            Contribution::EditStateMachine { .. } => "edit_state_machine",
            Contribution::NativeHook { .. } => "native_hook",
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
