//! Blast radius: what a Shipment touches, and whether two Shipments can coexist.
//!
//! Spec: Plan 04 "Composition". Two ideas carry this module.
//!
//! **A claim has an ACCESS.** A contribution does not merely write — `donor:` is *borrowed*, read
//! and never modified. Separating read from write is what lets the linter distinguish "these two
//! mods fight" from "this mod depends on something that is not there."
//!
//! **A claim has a MERGE CLASS, and the class is a property of the TARGET, not of the mod.** The
//! base game decides how a given table composes; we only encode what it already does. Hence
//! [`merge_class`] is a lookup over curated domain knowledge, and its default is
//! [`MergeClass::Exclusive`] — **fail closed**. An unrecognized target stays expressible (the open
//! lower bound survives) but cannot silently co-install.
//!
//! Everything here is hermetic. Whether a READ is satisfied by the base game needs the WAD stack
//! and is therefore the caller's problem; [`unsatisfied_reads`] answers only the part that can be
//! answered without a game — "no Shipment in this set provides it."

use crate::manifest::{Contribution, Manifest, Touch};
use std::collections::BTreeMap;

/// Read or write. `donor:` is the reason this distinction exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    Read,
    Write,
}

/// How multiple claimants on ONE target combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MergeClass {
    /// One claimant. A second is a hard error — there is no ordering that fixes it.
    Exclusive,
    /// Many claimants, union by key. The key IS the claim identity, so two claimants on one claim
    /// means a genuine duplicate-key collision.
    KeyedSet,
    /// Many claimants, append-only, with Quartermaster-computed companions (e.g. the wardrobe's
    /// availability count derived from final list length).
    OrderedList,
    /// Many claimants; the later one wins and load order is the user's answer.
    LastWins,
}

impl MergeClass {
    /// Whether more than one claimant on the same target is an error.
    pub fn collides_when_shared(self) -> bool {
        match self {
            MergeClass::Exclusive | MergeClass::KeyedSet => true,
            MergeClass::OrderedList | MergeClass::LastWins => false,
        }
    }
}

/// A single thing claimed. Equality is what conflict detection groups on, so the identity of each
/// variant is chosen to be exactly "the unit that can collide".
///
/// **`Asset` is keyed on the HASH alone, with no name field.** The name is carried out-of-band on
/// [`ClaimRecord::name`] for diagnostics. This is not a style choice: `touches` may name an asset
/// OR give a bare hash, and if the name participated in identity those two spellings would be
/// different claims — letting a Shipment evade conflict detection by writing `0xE54047D5` instead
/// of `al_veh_boat_destroyer`. The engine keys on the hash; so do we.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Claim {
    /// A Data-layer asset, identified by hash.
    Asset { hash: u32 },
    /// A Lua script. The unit of replacement is the containing block, so claiming a script is
    /// claiming a share of that block.
    Script { name: String },
    /// A row in the wardrobe. Key is `(wearer, slug)` — NOT slug alone: retail reuses `Original`
    /// and `ChickenSuit` across all three heroes.
    OutfitSlot { wearer: String, slug: String },
    /// A hooked native address or symbol.
    NativeHook { at: String },
    /// A file placed in the game folder, keyed on its PATH relative to that folder, LOWERCASED.
    ///
    /// Lowercased because the game folder is on Windows, where `Foo.ini` and `foo.ini` are one file:
    /// two Shipments whose names differ only in case overwrite each other. Build keys only through
    /// [`Claim::file_artifact`].
    ///
    /// The path, not the filename: `scripts/config.ini` and `plugins/config.ini` are two different
    /// files and do not fight, while two Shipments both writing `scripts/config.ini` overwrite each
    /// other. Keying on the bare name would have called the first pair a conflict and been right
    /// about the second by accident.
    FileArtifact { path: String },
    /// A sound cue, keyed on its guid (`pandemic_hash_m2` of its name) and, for a cue of a `vo_*`
    /// bank, the language whose copy of the bank it is in. One language's cue does not touch
    /// another's: each language's banks are separate entries (`<bank>.<language>`).
    SoundCue { guid: u32, language: Option<String> },
}

impl Claim {
    /// The claim on a game-folder file at `relative` (as [`crate::build::place_path`] joins it),
    /// keyed lowercased — Windows file names are case-insensitive.
    pub fn file_artifact(relative: &str) -> Claim {
        Claim::FileArtifact {
            path: relative.to_lowercase(),
        }
    }

    /// `(claim, display name)` for a named asset.
    fn asset(name: &str) -> (Claim, Option<String>) {
        (
            Claim::Asset {
                hash: crate::manifest::asset_hash(name),
            },
            Some(name.to_string()),
        )
    }

    /// `(claim, display name)` for a sound cue.
    fn sound_cue(name: &str, language: Option<crate::manifest::Language>) -> (Claim, Option<String>) {
        (
            Claim::SoundCue {
                guid: crate::manifest::asset_hash(name),
                language: language.map(|l| l.token().to_string()),
            },
            Some(name.to_string()),
        )
    }

    /// A `touches:` entry — a name, or the documented escape of a bare hash for a hash with no
    /// known name. Both spellings MUST produce the same claim.
    fn from_touch(t: &Touch) -> (Claim, Option<String>) {
        if t.is_bare_hash() {
            let hex = t.0.trim().trim_start_matches("0x").trim_start_matches("0X");
            if let Ok(hash) = u32::from_str_radix(hex, 16) {
                return (Claim::Asset { hash }, None);
            }
        }
        Claim::asset(t.0.trim())
    }

    /// Human label. `name` comes from [`ClaimRecord::name`] when the author gave one.
    pub fn describe(&self, name: Option<&str>) -> String {
        match self {
            Claim::Asset { hash } => match name {
                Some(n) => format!("asset {n} (0x{hash:08X})"),
                None => format!("asset 0x{hash:08X}"),
            },
            Claim::Script { name } => format!("script {name}"),
            Claim::OutfitSlot { wearer, slug } => format!("outfit {wearer}/{slug}"),
            Claim::NativeHook { at } => format!("native hook at {at}"),
            Claim::FileArtifact { path } => format!("file artifact {path}"),
            Claim::SoundCue { guid, language } => match (name, language) {
                (Some(n), Some(l)) => format!("sound cue {n} (0x{guid:08X}, {l})"),
                (Some(n), None) => format!("sound cue {n} (0x{guid:08X})"),
                (None, Some(l)) => format!("sound cue 0x{guid:08X} ({l})"),
                (None, None) => format!("sound cue 0x{guid:08X}"),
            },
        }
    }

    /// Label with no name available.
    pub fn label(&self) -> String {
        self.describe(None)
    }
}

/// Why a target is being claimed. The merge class depends on this as well as on the target — the
/// same asset hash is a `KeyedSet` when MINTED and `LastWins` when REPLACED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Minting a brand-new hash of our own.
    Additive,
    /// Overwriting a shipped asset, same hash, where the WAD stack's last-mounted-wins is an
    /// acceptable answer (a texture: the loser is visibly overridden, and load order picks).
    Replace,
    /// Overwriting a shipped asset, same hash, where two writers are a hard conflict: no load order
    /// makes both edits true, and the loser's is silently absent.
    ReplaceExclusive,
    /// Editing a shipped table that `qm link` MERGES across the installed set, in load order: a
    /// string table's `edit_stringdb` / `add_stringdb_keys` / `replace_stringdb_text`, the later
    /// write winning. Many writers compose.
    Merged,
    /// Opaque bytes we cannot reason about (`raw`). Always fails closed.
    Opaque,
}

/// The merge class for a claim. Curated domain knowledge; **default is Exclusive**.
pub fn merge_class(claim: &Claim, access: Access, intent: Intent) -> MergeClass {
    // A read never conflicts with anything — many mods may borrow one donor.
    if access == Access::Read {
        return MergeClass::LastWins;
    }
    // Opaque bytes: we cannot infer replacement-vs-addition, so we do not guess. This must be
    // checked BEFORE the target-shaped rules, or a `raw` block declaring an asset name would
    // silently inherit that asset's ordinary (permissive) semantics.
    if intent == Intent::Opaque {
        return MergeClass::Exclusive;
    }
    match claim {
        // Minting a NEW asset name: two Shipments choosing the same name collide, and the chunk
        // registry is FIRST-wins, so one of them silently vanishes. A hard error, not load order.
        Claim::Asset { .. } if intent == Intent::Additive => MergeClass::KeyedSet,
        // A table the link merges across the set: every writer's edits land in one link-owned copy.
        Claim::Asset { .. } if intent == Intent::Merged => MergeClass::OrderedList,
        // A replacement whose loser would be silently absent (a shader, an effect, a layer edit):
        // no load order makes both true, so a second writer is a hard conflict.
        Claim::Asset { .. } if intent == Intent::ReplaceExclusive => MergeClass::Exclusive,
        // Replacing a shipped asset: the WAD stack is last-mounted-wins and picking the winner is
        // exactly what load order is for.
        Claim::Asset { .. } => MergeClass::LastWins,
        // An APPEND to any script composes: the linker concatenates every Shipment's appends onto
        // the base source, in load order, and compiles once. A wholesale replacement (`replace_lua`)
        // cannot compose with anything — neither with a second replacement nor with an append,
        // which would be appended to source that is no longer there. Both use the SAME claim, so
        // the stricter class wins and an append beside a replacement is a conflict.
        Claim::Script { .. } if intent == Intent::Additive => MergeClass::OrderedList,
        Claim::Script { .. } => MergeClass::Exclusive,
        Claim::OutfitSlot { .. } => MergeClass::KeyedSet,
        // No arbitration exists: ASI discovery is filesystem order across four directories, so
        // there is no load order that resolves two plugins hooking one address.
        Claim::NativeHook { .. } => MergeClass::Exclusive,
        // A file placement is a claim on a filesystem PATH, and the filesystem is the one layer
        // here with no arbitration of any kind: no WAD stack to reorder, no first-writer registry,
        // no load order. Whichever deploy step runs last simply overwrites.
        //
        // It is `Exclusive` rather than `LastWins` because of what losing MEANS. When a texture
        // loses at the WAD stack the base asset shows and the user fixes it by reordering; when a
        // companion file loses, the plugin that reads it does not fall back — it reads somebody
        // else's config, with the file sitting right there looking installed and nothing logged.
        // And it is not `KeyedSet`, because there is no key: the bytes are opaque, so there is
        // nothing to union on. Same reasoning as `raw`, reached from the other direction.
        Claim::FileArtifact { .. } => MergeClass::Exclusive,
        // An added cue's name is a key across the installed set: FindCue answers with the first
        // loaded table that has the guid (`FUN_00835a70`), so two Shipments adding one cue name
        // leave one of them silent.
        Claim::SoundCue { .. } if intent == Intent::Additive => MergeClass::KeyedSet,
        // A replaced cue has one winner in the table the game loads; a second replacement of the
        // same cue cannot also take effect.
        Claim::SoundCue { .. } => MergeClass::Exclusive,
    }
}

/// One claim made by one contribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRecord {
    pub index: usize,
    pub kind: &'static str,
    pub access: Access,
    pub claim: Claim,
    pub class: MergeClass,
    /// The name the author wrote, when there was one. Diagnostics only — deliberately NOT part of
    /// [`Claim`] identity (see the type's docs).
    pub name: Option<String>,
}

/// Compute the blast radius of a manifest — COMPUTED for typed kinds, DECLARED only for `raw`.
pub fn claims(manifest: &Manifest) -> Vec<ClaimRecord> {
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        let kind = c.kind();
        let mut push = |access: Access, (claim, name): (Claim, Option<String>), intent: Intent| {
            let class = merge_class(&claim, access, intent);
            out.push(ClaimRecord {
                index,
                kind,
                access,
                claim,
                class,
                name,
            });
        };
        let bare = |c: Claim| (c, None);
        match c {
            Contribution::AddOutfit {
                name,
                slug,
                wearer,
                model,
                donor,
                ..
            } => {
                // Only INJECTED outfits mint a new hash. An existing-model outfit (`model` omitted)
                // references a model that already exists, so it makes no Additive asset claim.
                if model.is_some() {
                    push(Access::Write, Claim::asset(name), Intent::Additive);
                }
                push(
                    Access::Write,
                    bare(Claim::OutfitSlot {
                        wearer: wearer.clone(),
                        slug: slug.clone(),
                    }),
                    Intent::Additive,
                );
                // The wardrobe table lives here, so the script is claimed too — both kinds append a row.
                push(
                    Access::Write,
                    bare(Claim::Script {
                        name: "wifpmcinterior".into(),
                    }),
                    Intent::Additive,
                );
                if let Some(d) = donor {
                    push(Access::Read, Claim::asset(d), Intent::Replace);
                }
            }
            // A standalone texture is the same shape as a movie: one new hash, nothing borrowed.
            // `Additive` (not `Replace`) is what makes two Shipments minting the same texture name
            // a hard conflict — the registry is first-writer-wins, so the loser is silently absent
            // rather than visibly overridden.
            Contribution::AddTexture { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            // A new bank: its entry hash, and each cue's name, which the cue guid is the hash of.
            Contribution::AddSound { bank, cues, .. } => {
                push(Access::Write, Claim::asset(bank), Intent::Additive);
                for c in cues {
                    push(Access::Write, Claim::sound_cue(&c.name, None), Intent::Additive);
                }
            }
            // The bank's entry (`<bank>` or `<bank>.<language>`) and every cue the replacement
            // declares: each has one winner in the table the game loads.
            Contribution::ReplaceSoundBank { bank, language, cues, .. } => {
                let entry = crate::sound::entry_name(bank, *language);
                push(Access::Write, Claim::asset(&entry), Intent::ReplaceExclusive);
                for c in cues {
                    push(Access::Write, Claim::sound_cue(&c.name, *language), Intent::ReplaceExclusive);
                }
            }
            Contribution::ReplaceSoundCue { language, cue, .. } => {
                push(Access::Write, Claim::sound_cue(&cue.name, *language), Intent::ReplaceExclusive);
            }
            // A movie mints a new hash and borrows nothing — one write claim, no read claim. The
            // `Additive` intent is what makes two Shipments choosing the same movie name a hard
            // conflict rather than a load-order question: the chunk registry is first-writer-wins,
            // so the loser does not lose visibly, it simply is not there.
            Contribution::AddMovie { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            // add_ui is a movie (Additive write on its new hash, same first-writer-wins rule as
            // AddMovie) PLUS a Script-layer touch: it appends the one-line mod-loader trampoline to
            // `wifpmcinterior`. That claim is Additive, so two UI mods merge (both trampolines fold
            // to one; both registrations bake into `qm_modloader`) rather than conflicting — the same
            // shape as an outfit's wardrobe-row append.
            Contribution::AddUi { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
                push(
                    Access::Write,
                    bare(Claim::Script {
                        name: "wifpmcinterior".into(),
                    }),
                    Intent::Additive,
                );
            }
            Contribution::AddModel { name, donor, .. } => {
                // The model's own hash. With `collision: follow_geometry` the regenerated PHY2 ships
                // INSIDE this same model block (it replaces the donor's PHY2 chunk in the injected
                // container, and touches no SEGM/other asset), so the model-hash Write claim already
                // covers the collision — no separate claim is needed.
                push(Access::Write, Claim::asset(name), Intent::Additive);
                if let Some(d) = donor {
                    push(Access::Read, Claim::asset(d), Intent::Replace);
                }
            }
            Contribution::ReplaceTexture { target, .. } => {
                // Same hash as the shipped asset — a replacement, not an addition.
                push(Access::Write, Claim::asset(target), Intent::Replace);
            }
            Contribution::PatchLua { target, .. } => {
                push(
                    Access::Write,
                    bare(Claim::Script {
                        name: target.clone(),
                    }),
                    Intent::Additive,
                );
            }
            // add_script mints a whole new script asset. Same claim shape as add_movie / add_texture:
            // one new hash, `Additive`, so two Shipments minting the same `name` are a hard conflict
            // (first-writer-wins registry — the loser is silently absent, not visibly overridden).
            Contribution::AddScript { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            // replace_lua swaps a shipped script's bytecode in place. `Replace` on a script is
            // `Exclusive`: two replacements of one script conflict. The claim is on the SCRIPT (not
            // the asset hash) so it also conflicts with a `patch_lua` on the same target -- an
            // append + a wholesale replace of the same script cannot both be true.
            Contribution::ReplaceLua { target, .. } => {
                push(
                    Access::Write,
                    bare(Claim::Script {
                        name: target.clone(),
                    }),
                    Intent::Replace,
                );
            }
            // replace_phy2 swaps a shipped model's collision. Same-hash, and a second Shipment
            // swapping the same model's collision is a hard conflict.
            Contribution::ReplacePhy2 { target, .. } => {
                push(Access::Write, Claim::asset(target), Intent::ReplaceExclusive);
            }
            // add_placement writes into an existing layer's placement block. Additive by design --
            // two Shipments adding disjoint entity keys to the same layer merge, same-key is the
            // hard conflict (handled by the layer's own key check at build time).
            Contribution::AddPlacement { layer, .. } => {
                push(Access::Write, Claim::asset(layer), Intent::Additive);
            }
            // add_layer mints a NEW layer asset. Same shape as add_texture / add_movie: two
            // Shipments minting the same layer name is a hard conflict.
            Contribution::AddLayer { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            // Novel Havok animation clip. New hash, Additive.
            Contribution::AddAnimation { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            // Same-hash animation swap. Two swaps of one clip are a hard conflict.
            Contribution::ReplaceAnimation { target, .. } => {
                push(Access::Write, Claim::asset(target), Intent::ReplaceExclusive);
            }
            // Novel shader. New hash, Additive.
            Contribution::AddShader { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            Contribution::ReplaceShader { target, .. } => {
                push(Access::Write, Claim::asset(target), Intent::ReplaceExclusive);
            }
            // Novel particle effect. New hash, Additive.
            Contribution::AddFx { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            Contribution::ReplaceFx { target, .. } => {
                push(Access::Write, Claim::asset(target), Intent::ReplaceExclusive);
            }
            // Terrain cell wholesale replace. Same-hash; two replacements are a hard conflict.
            Contribution::ReplaceTerrainCell { target, .. } => {
                push(Access::Write, Claim::asset(target), Intent::ReplaceExclusive);
            }
            // A shop item claims the catalog script it appends a row to (support vs equipment) plus
            // `mrxrewarddata` for the reward row. Appends are `OrderedList`, so N shop mods union
            // rather than clobber.
            Contribution::AddShopItem { catalog, .. } => {
                let catalog_script = match catalog {
                    crate::manifest::ShopCatalog::Support => "mrxsupportdata",
                    crate::manifest::ShopCatalog::Equipment => "wifequipmentdata",
                };
                push(
                    Access::Write,
                    bare(Claim::Script {
                        name: catalog_script.into(),
                    }),
                    Intent::Additive,
                );
                push(
                    Access::Write,
                    bare(Claim::Script {
                        name: "mrxrewarddata".into(),
                    }),
                    Intent::Additive,
                );
            }
            Contribution::EditStateMachine { target, .. } => {
                push(Access::Write, Claim::asset(target), Intent::ReplaceExclusive);
            }
            // Edits the WHOLE layer block, emitted as an overlay. Two Shipments editing one layer
            // cannot both win — the earlier overlay's edits would be silently absent — so it is a
            // hard conflict, and with an `add_placement` on the same layer too (the stricter class
            // wins).
            Contribution::EditWorld { layer, .. } => {
                push(Access::Write, Claim::asset(layer), Intent::ReplaceExclusive);
            }
            // No Data half — its whole effect is a registration baked into `qm_modloader`, reached by
            // the same one-line trampoline `add_ui` appends to `wifpmcinterior`. Additive, so N layer
            // mods (and UI mods) fold to one trampoline and one loader rather than conflicting.
            Contribution::ActivateLayer { .. } => {
                push(
                    Access::Write,
                    bare(Claim::Script {
                        name: "wifpmcinterior".into(),
                    }),
                    Intent::Additive,
                );
            }
            // Key edits, key additions and text replacements on a shipped string table. `qm link`
            // merges every installed Shipment's writes to one table into ONE link-owned copy, in
            // load order, the later write winning (a text replacement resolves against the table as
            // merged so far) — so writers to one table compose, whatever they touch. The claim is on
            // the table's asset hash, so a `raw` declaring that table still fails closed.
            Contribution::EditStringDb { target, .. }
            | Contribution::AddStringDbKeys { target, .. }
            | Contribution::ReplaceStringDbText { target, .. } => {
                push(Access::Write, Claim::asset(target), Intent::Merged);
            }
            // A NEW language: mints a new stringdb hash (`hash(name)`) carried in a new base WAD.
            // Additive, so two Shipments adding the same language collide (KeyedSet) rather than one
            // silently winning — the same shape as add_texture / add_movie.
            Contribution::AddLanguage { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            Contribution::NativeHook {
                plugin,
                symbol,
                touches,
                ..
            } => {
                for t in touches {
                    push(
                        Access::Write,
                        bare(Claim::NativeHook { at: t.0.clone() }),
                        Intent::Replace,
                    );
                }
                if let Some(s) = symbol {
                    push(
                        Access::Write,
                        bare(Claim::NativeHook { at: s.clone() }),
                        Intent::Replace,
                    );
                }
                if let Some(p) = plugin {
                    if let Some(file) = p.file_name().and_then(|f| f.to_str()) {
                        push(
                            Access::Write,
                            // Built with the SAME joiner the lowering uses, so the claim and the
                            // emitted placement cannot describe different paths.
                            bare(Claim::file_artifact(&crate::build::place_path(
                                crate::build::ASI_SUBDIR,
                                file,
                            ))),
                            Intent::Replace,
                        );
                    }
                }
            }
            // The destination is a closed-set NAME, so the directory half of this path is a
            // literal; only the filename comes from the author, and it comes from their source
            // file. That is the same shape as the `native_hook` claim above, which is the point —
            // a companion and the plugin it belongs to must be able to collide with each other.
            Contribution::PlaceFile { file, dest } => {
                if let Some(name) = file.file_name().and_then(|f| f.to_str()) {
                    push(
                        Access::Write,
                        bare(Claim::file_artifact(&crate::build::place_path(
                            dest.relative_dir(),
                            name,
                        ))),
                        Intent::Replace,
                    );
                }
            }
            // A runtime DLL in the game root: the same FileArtifact claim as any other placement,
            // so two Shipments shipping one DLL name are a hard conflict.
            Contribution::AddRuntimeDll { dll } => {
                if let Some(name) = dll.file_name().and_then(|f| f.to_str()) {
                    push(
                        Access::Write,
                        bare(Claim::file_artifact(&crate::build::place_path(
                            crate::manifest::PlaceIn::GameRoot.relative_dir(),
                            name,
                        ))),
                        Intent::Replace,
                    );
                }
            }
            Contribution::Raw { touches, .. } => {
                // The open lower bound: we cannot infer anything about the bytes, so we trust the
                // declared radius and fail closed on class.
                for t in touches {
                    push(Access::Write, Claim::from_touch(t), Intent::Opaque);
                }
            }
        }
    }
    out
}

/// Two contributions in ONE Shipment claiming the same target in a way that cannot accumulate.
///
/// The rule is **not** "any duplicate is an error" — an outfit pack legitimately adds several
/// outfits, and they all share the wardrobe script claim. Only `OrderedList` genuinely accumulates;
/// every other class has exactly one winner, and inside a single Shipment there is no load order
/// for the author to appeal to. Two `replace_texture` on one target is `LastWins` across Shipments
/// but here means one of them is simply dead — almost always a copy-paste mistake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfConflict {
    pub claim: Claim,
    pub class: MergeClass,
    pub indices: Vec<usize>,
    pub name: Option<String>,
}

impl std::fmt::Display for SelfConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list: Vec<String> = self
            .indices
            .iter()
            .map(|i| format!("contributions[{i}]"))
            .collect();
        write!(
            f,
            "{} is claimed by {} in one Shipment — only one can take effect",
            self.claim.describe(self.name.as_deref()),
            list.join(" and ")
        )
    }
}

/// Duplicate WRITE claims within a single manifest that cannot accumulate.
pub fn self_conflicts(manifest: &Manifest) -> Vec<SelfConflict> {
    let mut by_claim: BTreeMap<Claim, (MergeClass, Vec<usize>, Option<String>)> = BTreeMap::new();
    for r in claims(manifest)
        .into_iter()
        .filter(|r| r.access == Access::Write)
    {
        let entry = by_claim
            .entry(r.claim)
            .or_insert_with(|| (r.class, Vec::new(), r.name.clone()));
        if r.class == MergeClass::Exclusive {
            entry.0 = MergeClass::Exclusive;
        }
        if entry.2.is_none() {
            entry.2 = r.name.clone();
        }
        if !entry.1.contains(&r.index) {
            entry.1.push(r.index);
        }
    }
    by_claim
        .into_iter()
        .filter(|(_, (class, indices, _))| indices.len() > 1 && *class != MergeClass::OrderedList)
        .map(|(claim, (class, indices, name))| SelfConflict {
            claim,
            class,
            indices,
            name,
        })
        .collect()
}

/// Who claimed a thing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Claimant {
    pub shipment: String,
    pub index: usize,
}

/// Two Shipments claiming one target in a way the target cannot absorb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub claim: Claim,
    pub class: MergeClass,
    pub claimants: Vec<Claimant>,
    pub name: Option<String>,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let who: Vec<String> = self
            .claimants
            .iter()
            .map(|c| format!("{}[{}]", c.shipment, c.index))
            .collect();
        let why = match self.class {
            MergeClass::Exclusive => {
                "only one Shipment may claim it, and no load order resolves this"
            }
            MergeClass::KeyedSet => "the key must be unique across installed Shipments",
            _ => "unexpected: this class does not collide",
        };
        write!(
            f,
            "{} claimed by {} — {why}",
            self.claim.describe(self.name.as_deref()),
            who.join(", ")
        )
    }
}

/// Conflicts across a set of installed Shipments.
///
/// Note what is NOT a conflict, because it is the whole point: two Shipments each adding a wardrobe
/// outfit claim different `OutfitSlot`s and share an `OrderedList` script, so they compose. Two
/// Shipments replacing the same texture are `LastWins` — the user picks with load order.
pub fn conflicts(shipments: &[(&str, &Manifest)]) -> Vec<Conflict> {
    let mut by_claim: BTreeMap<Claim, (MergeClass, Vec<Claimant>, Option<String>)> =
        BTreeMap::new();
    for (name, manifest) in shipments {
        for r in claims(manifest)
            .into_iter()
            .filter(|r| r.access == Access::Write)
        {
            let entry = by_claim
                .entry(r.claim)
                .or_insert_with(|| (r.class, Vec::new(), r.name.clone()));
            // Fail closed: if two contributions disagree about a target's class, take the stricter.
            // This is what stops a `raw` block laundering an asset into permissive semantics by
            // declaring a target some typed contribution also claims.
            if r.class == MergeClass::Exclusive {
                entry.0 = MergeClass::Exclusive;
            }
            if entry.2.is_none() {
                entry.2 = r.name.clone();
            }
            entry.1.push(Claimant {
                shipment: (*name).to_string(),
                index: r.index,
            });
        }
    }
    by_claim
        .into_iter()
        .filter_map(|(claim, (class, claimants, name))| {
            let distinct: std::collections::BTreeSet<&str> =
                claimants.iter().map(|c| c.shipment.as_str()).collect();
            // Only ACROSS Shipments — within one, `self_conflicts` already reported it.
            if distinct.len() > 1 && class.collides_when_shared() {
                Some(Conflict {
                    claim,
                    class,
                    claimants,
                    name,
                })
            } else {
                None
            }
        })
        .collect()
}

/// A read that no Shipment in the set provides.
///
/// **This is only half the answer.** Most reads (`donor: pmc_hum_mattias`) are satisfied by the
/// BASE GAME, which needs the WAD stack to confirm — so a result here means "not provided by these
/// Shipments", not "missing". The caller decides whether to then check the base WAD. Separating the
/// two is what keeps this function usable in CI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsatisfiedRead {
    pub claim: Claim,
    pub by: Claimant,
}

pub fn unsatisfied_reads(shipments: &[(&str, &Manifest)]) -> Vec<UnsatisfiedRead> {
    let mut written = std::collections::BTreeSet::new();
    for (_, m) in shipments {
        for r in claims(m).into_iter().filter(|r| r.access == Access::Write) {
            written.insert(r.claim);
        }
    }
    let mut out = Vec::new();
    for (name, m) in shipments {
        for r in claims(m).into_iter().filter(|r| r.access == Access::Read) {
            if !written.contains(&r.claim) {
                out.push(UnsatisfiedRead {
                    claim: r.claim,
                    by: Claimant {
                        shipment: (*name).to_string(),
                        index: r.index,
                    },
                });
            }
        }
    }
    out
}
