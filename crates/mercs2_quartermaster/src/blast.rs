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
    /// A file placed in the game folder, keyed on its PATH relative to that folder.
    ///
    /// The path, not the filename: `scripts/config.ini` and `plugins/config.ini` are two different
    /// files and do not fight, while two Shipments both writing `scripts/config.ini` overwrite each
    /// other. Keying on the bare name would have called the first pair a conflict and been right
    /// about the second by accident.
    FileArtifact { path: String },
}

impl Claim {
    /// `(claim, display name)` for a named asset.
    fn asset(name: &str) -> (Claim, Option<String>) {
        (
            Claim::Asset {
                hash: crate::manifest::asset_hash(name),
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
    /// Overwriting a shipped asset, same hash.
    Replace,
    /// Opaque bytes we cannot reason about (`raw`). Always fails closed.
    Opaque,
}

/// Scripts whose composition semantics we have actually reversed, and which therefore merge by
/// source concatenation instead of whole-block replacement.
///
/// This is the curated half of the composition catalog. It is deliberately a SHORT allow-list:
/// everything absent from it falls to `Exclusive`, so being wrong here costs a false conflict
/// (annoying, visible) rather than a silent mutual annihilation (catastrophic, invisible).
const MERGEABLE_SCRIPTS: &[&str] = &["wifpmcinterior"];

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
        // Replacing a shipped asset: the WAD stack is last-mounted-wins and picking the winner is
        // exactly what load order is for.
        Claim::Asset { .. } => MergeClass::LastWins,
        Claim::Script { name } if MERGEABLE_SCRIPTS.contains(&name.as_str()) => {
            MergeClass::OrderedList
        }
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
                donor,
                ..
            } => {
                // Additive: a brand-new hash of our own.
                push(Access::Write, Claim::asset(name), Intent::Additive);
                push(
                    Access::Write,
                    bare(Claim::OutfitSlot {
                        wearer: wearer.clone(),
                        slug: slug.clone(),
                    }),
                    Intent::Additive,
                );
                // The wardrobe table lives here, so the script is claimed too.
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
            // Same shape as a movie or a texture: one new hash, nothing borrowed.
            Contribution::AddSound { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            // A movie mints a new hash and borrows nothing — one write claim, no read claim. The
            // `Additive` intent is what makes two Shipments choosing the same movie name a hard
            // conflict rather than a load-order question: the chunk registry is first-writer-wins,
            // so the loser does not lose visibly, it simply is not there.
            Contribution::AddMovie { name, .. } => {
                push(Access::Write, Claim::asset(name), Intent::Additive);
            }
            Contribution::AddModel { name, donor, .. } => {
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
            Contribution::EditStateMachine { target, .. } => {
                push(Access::Write, Claim::asset(target), Intent::Replace);
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
                            bare(Claim::FileArtifact {
                                // Built with the SAME joiner the lowering uses, so the claim and
                                // the emitted placement cannot describe different paths.
                                path: crate::build::place_path(crate::build::ASI_SUBDIR, file),
                            }),
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
                        bare(Claim::FileArtifact {
                            path: crate::build::place_path(dest.relative_dir(), name),
                        }),
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
