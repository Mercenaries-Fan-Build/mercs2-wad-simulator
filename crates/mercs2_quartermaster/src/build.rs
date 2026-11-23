//! The builder: lint-gate → lower → assemble one overlay WAD → emit, verified by hash.
//!
//! Two mandates shape this module.
//!
//! **Gated on EXIT CODE, never a printed count.** [`build`] returns `Err(BuildError::Blocked)` when
//! the linter finds anything at `Error` or above; a caller cannot accidentally ship by ignoring
//! stdout.
//!
//! **Verified by hash** (`verify-artifacts-by-hash-not-size-mtime`). Every artifact carries its
//! sha256 in the [`Placement`] record, which is also what makes a file drop reversible — a WAD
//! overlay is undone by deleting one file, but an `.asi` in the game folder is not backable-out
//! unless something wrote down what was placed and where.
//!
//! ## Lowering status
//!
//! Every kind lowers here.
//!
//! `raw` is the open lower bound: opaque bytes plus an author-DECLARED blast radius. It is the one
//! kind with no encoder behind it, so the lowering checks everything structural it can and refuses
//! rather than warns — and it requires the declared `touches` to match the payload's own entry
//! table exactly, because that declaration is the only thing that can mint the ASET rows.
//!
//! `add_outfit` is the composed case, and the reason [`Lowering`] has more than one outcome: a Data
//! half (the model, injected into a hero-rigged donor) that lowers immediately, and a Script half
//! that cannot, because the link generates the wardrobe lists from the installed set's outfits at
//! once ([`crate::outfits`]). Linking a Shipment's own outfits and mutations here keeps its overlay
//! valid **standalone**; the cross-Shipment relink is deploy's job, and skipping it is what lets one
//! script mod overwrite another's Lua.
//!
//! `add_movie` needs NO game stack: a Scaleform movie is self-contained, so unlike a texture
//! replacement (whose dimensions come from the target) or a model (whose rig comes from a donor)
//! there is nothing to read out of retail. It therefore lowers with `game: None`, as `add_texture`,
//! `add_animation` and `add_sound` do. Where its block goes comes from `data/retail_movies.tsv`
//! ([`crate::retail_movies`]): a `shell.wad` movie's block goes to the shell patch, which carries
//! `shell.wad`'s CSUM row, so only that case needs the game.
//!
//! `native_hook` and `place_file` are the kinds that produce no WAD content at all — a file placed
//! in the game folder, plus the [`Placement`] record that makes the drop reversible. `native_hook`
//! places the `.asi` and chooses its directory outright; `place_file` places the companions that
//! `.asi` reads, and lets the author pick a destination NAME from a closed set
//! ([`crate::manifest::PlaceIn`]) rather than write a path. Neither can be pointed at the game
//! executable or a WAD, and neither needs a game stack, so both lower with `game: None`.
//!
//! A contribution the lowering cannot honour returns `Unsupported` with the reason. A refusal with
//! a reason is honest; a contribution that is quietly skipped produces a WAD that looks fine and
//! does nothing.

use crate::discover::LoadedShipment;
use crate::game::{GameStack, Platform};
use crate::link::{self, ScriptMutation};
use crate::lint::{self, Diagnostic, WARDROBE_HEROES};
use crate::manifest::{Contribution, Layer};
use crate::names::NameTable;
use mercs2_formats::donor;
use mercs2_formats::mesh_import;
use mercs2_formats::model_inject::inject_static_into_donor_block;
use mercs2_formats::{char_import, char_lower};
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::texture::{build_texture_block, TexFormat, TextureData};
use mercs2_formats::texture_encode::{self, encode_bc1, encode_bc3, mip_chain};
use mercs2_formats::types::{
    TYPE_HASH_ANIMATION, TYPE_HASH_LAYER, TYPE_HASH_MODEL, TYPE_HASH_STRINGDB,
    TYPE_HASH_TERRAIN_MESH, TYPE_ID_ANIMATION, TYPE_ID_CFX_PACK, TYPE_ID_EFFECT, TYPE_ID_LAYER,
    TYPE_ID_MODEL, TYPE_ID_SCRIPT, TYPE_ID_STRINGDB, TYPE_ID_TERRAIN_MESH, TYPE_ID_TEXTURE,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// One scripts block read out of the game stack, with the ASET rows it publishes.
pub struct LoadedScriptBlock {
    /// The block's own PTHS path, carried through to the emitted `PatchBlock`.
    pub path: String,
    pub block: ScriptsBlock,
    /// `asset_hash -> (packed_block_ref, secondary_ref, type_id)`, as the base WAD had them.
    pub rows: std::collections::HashMap<u32, (u32, u32, u32)>,
    /// The block's index in the WAD it was read from, and that WAD's position in the stack: the rows'
    /// rungs are in that WAD's index space.
    pub source_block_index: u32,
    pub wad: usize,
    /// The blocks the rows name as LOD rungs, carried with the block so its rows keep their chains
    /// ([`GameStack::block_for_reemit`]).
    pub rung_blocks: Vec<PatchBlock>,
}

/// Load each of `blocks` (`(PTHS needle, PTHS path)`, [`link::SCRIPT_BLOCKS`] or
/// [`link::SHELL_SCRIPT_BLOCKS`]) from `stack`.
///
/// A block missing from the stack is **skipped, not fatal**. A synthetic or overlay-only stack may
/// carry `scripts_vz` and nothing else, and if a mutation actually needed the absent block the
/// linker already reports `UnknownScript` naming the target — which tells the author what to fix,
/// where "no resident block in the game stack" would not. A stack with none of them is an error.
pub fn load_script_blocks(
    stack: &mut GameStack,
    blocks: &[(&str, &str)],
    kind: &'static str,
) -> Result<Vec<LoadedScriptBlock>, BuildError> {
    let mut out = Vec::new();
    for (needle, path) in blocks {
        let Some(found) = stack.block_for_reemit(needle) else {
            continue;
        };
        let found = found.map_err(|m| BuildError::Lower { index: 0, kind, message: format!("reading {path}: {m}") })?;
        let block = ScriptsBlock::parse(&found.raw).map_err(|m| BuildError::Lower {
            index: 0,
            kind,
            message: format!("parsing {path}: {m}"),
        })?;
        out.push(LoadedScriptBlock {
            path: (*path).to_string(),
            block,
            rows: found.rows,
            source_block_index: found.source_block_index,
            wad: found.wad,
            rung_blocks: found.rung_blocks,
        });
    }
    if out.is_empty() {
        return Err(BuildError::Lower {
            index: 0,
            kind,
            message: format!(
                "none of the scripts blocks {:?} is in {}",
                blocks.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
                stack.paths().iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
        });
    }
    Ok(out)
}

/// Emit a `PatchBlock` for each loaded block in `touched` (indices into `loaded`): the scripts blocks
/// the link spliced, and the resident block when its worldentity carries added templates
/// ([`lower_fx`]).
///
/// **Only the touched blocks.** Re-emitting an untouched block would shadow the base with a
/// byte-identical copy — harmless in isolation, but it puts the whole ~7,000-entry resident block
/// into every overlay that patches one `vz` script, and makes the overlay's contents stop meaning
/// "what this Shipment changed".
///
/// ★ **A row for EVERY entry the block carries, taken from the base WAD.** Not just the scripts:
/// an asset present in a block with no ASET row naming it in the same WAD is the **M0004 HANG** —
/// nothing can resolve it by hash, and the world load stops completing with no error. Retail never
/// ships that shape; all 30,006 asset hashes in `vz.wad`'s blocks have a row.
///
/// The rows are **copied from the block's own rows in the base WAD** rather than synthesised,
/// because `type_id` selects which loader is dispatched and the type-hash→id tables are known wrong
/// for 12 of 36 ids. For `scripts_vz` this is a no-op restatement (114 script rows); for the
/// resident block it preserves ~6,800 rows this code has no business inventing — get one wrong and
/// the validator reports the container as unreadable by the loader it was handed to.
///
/// A hash with no row in the base falls back to a sentinel script row. That should not happen for a
/// block read out of the stack — retail gives every carried asset a row — and a row that exists
/// beats the M0004 hang of no row at all.
///
/// ★ **The rows keep their LOD chains.** A row's `_P001`..`_P003` rungs name other blocks of the base
/// WAD (vz resident's PMC-courtyard textures name `resident-pmcoutpost_fountain_P001`..`_P003`). The
/// emitted block carries its `source_block_index` and the blocks its rows name come with it
/// ([`LoadedScriptBlock::rung_blocks`]), so the writer re-points every rung; without them it sentinels
/// them and those assets render at their coarsest tier.
pub fn script_patch_blocks(
    loaded: &[LoadedScriptBlock],
    touched: &std::collections::BTreeSet<usize>,
    kind: &'static str,
) -> Result<Vec<PatchBlock>, BuildError> {
    let fail = |message: String| BuildError::Lower { index: 0, kind, message };
    let mut out = Vec::new();
    let mut carried: Vec<PatchBlock> = Vec::new();
    // Every source index emitted here must be in ONE WAD's index space, or the writer's
    // source -> patch map would join blocks of different WADs.
    let mut space: Option<usize> = None;
    for &bi in touched {
        let lb = &loaded[bi];
        match space {
            None => space = Some(lb.wad),
            Some(w) if w != lb.wad => {
                return Err(fail(format!(
                    "{} was read from the stack's WAD #{}, and another re-emitted block from WAD #{w}: their LOD rungs number different WADs' blocks",
                    lb.path, lb.wad
                )))
            }
            Some(_) => {}
        }
        for rb in &lb.rung_blocks {
            if !carried.iter().any(|c| c.path_string.eq_ignore_ascii_case(&rb.path_string)) {
                carried.push(rb.clone());
            }
        }
        let aset: Vec<AsetEntry> = lb
            .block
            .entries
            .iter()
            .map(|e| match lb.rows.get(&e.name_hash) {
                Some(&(packed, secondary, type_id)) => {
                    AsetEntry::new(e.name_hash, secondary, packed, type_id)
                }
                None => AsetEntry::new(e.name_hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_SCRIPT),
            })
            .collect();
        let mut block = PatchBlock::from_decompressed(&lb.block.serialize(), lb.path.clone(), aset, None)
            .map_err(|m| BuildError::Lower {
                index: 0,
                kind,
                message: m,
            })?;
        block.source_block_index = Some(lb.source_block_index);
        // Every rung a row names must ship: the block itself or one carried with it. A rung that does
        // not would be sentinelled by the writer and its asset would lose its finer tiers.
        let ships: std::collections::BTreeSet<u32> =
            lb.rung_blocks.iter().filter_map(|b| b.source_block_index).chain([lb.source_block_index]).collect();
        for e in &block.aset_entries {
            for r in [e.u32_2 & 0xFFFF, e.u32_1 >> 16, e.u32_1 & 0xFFFF] {
                if r != 0xFFFF && !ships.contains(&r) {
                    return Err(fail(format!("{}: asset 0x{:08X} names block {r} as a LOD rung, and it is not carried", lb.path, e.asset_hash)));
                }
            }
        }
        out.push(block);
    }
    // A carried block that is itself re-emitted here ships once, as the re-emitted copy.
    carried.retain(|c| !out.iter().any(|o| o.path_string.eq_ignore_ascii_case(&c.path_string)));
    out.extend(carried);
    Ok(out)
}

/// The `level: vz` rows whose block `vz.wad` already has ([`crate::level::joins_link`]): they cannot ship
/// in a block of their own, so each is added to the deploy link's copy of that block, one of `loaded`
/// ([`link::SCRIPT_BLOCKS`]), and those blocks' indices come back. Each such asset has one rung and is
/// new to the game (a `level: vz` row only adds). Its ASET row has the shape of the block's own rows for
/// an asset of its type, copied from one of them with the asset's hash: the base WAD's rows are the only
/// truth for a type's row, and a block with no asset of the type is refused rather than given a guess.
fn join_vz_rows(
    shipments: &[&LoadedShipment],
    loaded: &mut [LoadedScriptBlock],
    game: &GameStack,
    log: &mut Vec<String>,
) -> Result<std::collections::BTreeSet<usize>, BuildError> {
    let fail = |message: String| BuildError::Lower { index: 0, kind: "link", message };
    let mut touched = std::collections::BTreeSet::new();
    for s in shipments {
        for a in crate::level::assets_of(&s.manifest, crate::level::VZ) {
            let Some(path) = crate::level::joins_link(&a, game) else { continue };
            let who = format!("{} level: vz {} ({})", s.manifest.shipment.name, a.name, a.asset_type);
            let [rung] = a.rungs else {
                return Err(fail(format!("{who}: its block {path} is vz.wad's, which the link re-emits whole, and it has {} rungs, not 1", a.rungs.len())));
            };
            let bi = loaded.iter().position(|lb| lb.path.eq_ignore_ascii_case(&path)).ok_or_else(|| {
                fail(format!(
                    "{who}: vz.wad has its block {path}, and the link re-emits only {:?}, so it has no copy of that block to join",
                    loaded.iter().map(|lb| lb.path.as_str()).collect::<Vec<_>>()
                ))
            })?;
            let (type_hash, _) = mercs2_formats::types::type_by_name(a.asset_type).ok_or_else(|| fail(format!("{who}: no asset type is named {:?}", a.asset_type)))?;
            let name = crate::manifest::asset_hash(a.name);
            let lb = &mut loaded[bi];
            if lb.block.entries.iter().any(|e| e.name_hash == name && e.type_hash == type_hash) {
                return Err(fail(format!("{who}: {path} already holds it, and a level: vz row only adds")));
            }
            // A single-block sibling: `_P001`..`_P003` all sentinel (`AsetEntry::is_single_block`), as a
            // one-rung asset's row must be.
            let single = |&(packed, secondary, _): &(u32, u32, u32)| packed & 0xFFFF == 0xFFFF && secondary == 0xFFFF_FFFF;
            let sibling = lb
                .block
                .entries
                .iter()
                .filter(|e| e.type_hash == type_hash)
                .find_map(|e| lb.rows.get(&e.name_hash).filter(|r| single(r)).map(|r| (e.field_c, *r)))
                .ok_or_else(|| fail(format!("{who}: {path} holds no single-block asset of its type with a row to copy the row's shape from")))?;
            if lb.rows.contains_key(&name) {
                return Err(fail(format!("{who}: {path} already has a row for its hash 0x{name:08X}")));
            }
            let file = s.root.join(&rung.file);
            let bytes = std::fs::read(&file).map_err(|e| fail(format!("{who}: {}: {e}", file.display())))?;
            log.push(format!("{who}: joined to the link's {path}, {} B", bytes.len()));
            lb.block.entries.push(mercs2_formats::scripts_block::Entry { name_hash: name, type_hash, field_c: sibling.0, bytes });
            lb.rows.insert(name, sibling.1);
            touched.insert(bi);
        }
    }
    Ok(touched)
}

/// Whether a Shipment set has front-end work for qm's front-end link: a sound bank or an `add_script`
/// library that loads in the front end.
fn has_front_end(sound_regs: &[link::SoundBankRegistration], additions: &[link::ScriptAddition]) -> bool {
    let fe = crate::manifest::LoadSession::FrontEnd;
    sound_regs.iter().any(|r| r.sessions.contains(&fe)) || additions.iter().any(|a| a.load.as_ref().is_some_and(|l| l.sessions.contains(&fe)))
}

/// What a set gives qm's front-end link: its sound registrations, the levels the Extras page lists, the
/// menu items it adds, and its `add_script` additions.
struct FrontEndSet<'a> {
    sound_regs: &'a [link::SoundBankRegistration],
    levels: &'a [link::LevelEntry],
    items: &'a [link::MenuItemEntry],
    additions: &'a [link::ScriptAddition],
}

/// Link qm's front-end hooks and loader ([`link::link_front_end`]) into `shell.wad`'s scripts block,
/// as `hooks` says, read from the `shell.wad` beside `game`'s base WAD ([`GameStack::open_sibling`]),
/// and return the block to ship in the shell patch — every row copied from `shell.wad`
/// ([`script_patch_blocks`]), with the modules the link added ([`link::LinkOutput::modules`]). Empty
/// when `hooks` asks for nothing. A missing `shell.wad` is an error.
fn link_shell_loader(
    game: &GameStack,
    set: FrontEndSet<'_>,
    order: &[String],
    hooks: link::QmHooks,
    kind: &'static str,
    log: &mut Vec<String>,
) -> Result<(Vec<PatchBlock>, Vec<link::LinkedModule>, Vec<crate::imports::ImportFinding>), BuildError> {
    let FrontEndSet { sound_regs, levels, items, additions } = set;
    if hooks == link::QmHooks::ForLibraries && !has_front_end(sound_regs, additions) && levels.is_empty() && items.is_empty() {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }
    let fail = |message: String| BuildError::Lower { index: 0, kind, message };
    let mut shell = game.open_sibling("shell.wad").map_err(fail)?;
    let mut loaded = load_script_blocks(&mut shell, link::SHELL_SCRIPT_BLOCKS, kind)?;
    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|lb| link::TargetBlock { path: lb.path.clone(), block: &mut lb.block })
        .collect();
    let linked = link::link_front_end(&mut targets, sound_regs, levels, items, additions, order, hooks).map_err(|e| fail(e.to_string()))?;
    drop(targets);
    for l in &linked.scripts {
        log.push(format!(
            "linked {} in {} (shell.wad): {} → {} B source, {} B bytecode, from {:?}",
            l.target, loaded[l.block].path, l.base_source_bytes, l.linked_source_bytes, l.bytecode_bytes, l.contributors
        ));
    }
    let blocks = script_patch_blocks(&loaded, &linked.scripts.iter().map(|l| l.block).collect(), kind)?;
    Ok((blocks, linked.modules, linked.findings))
}

/// A standalone build's import findings ([`crate::imports::check`]). A top-level use of a module
/// whose body has not run yet (M0314, [`crate::imports::ImportRule::LoadOrder`]) is in every set this
/// Shipment is part of, so it fails the build, and so does an unresolved name (M0209) that is one of
/// the Shipment's own `add_script` modules, which no other Shipment may mint: it is minted only in
/// another session. A reorder of retail's loading (M0314, [`crate::imports::ImportRule::Reorder`]) is
/// a warning everywhere. Any other M0209 or M0313 may be answered by another Shipment of the set, so
/// it is a warning in the build log; `qm link` checks it against the installed set and refuses the
/// set on it.
fn solo_import_findings(
    findings: &[crate::imports::ImportFinding],
    manifest: &crate::manifest::Manifest,
    kind: &'static str,
    log: &mut Vec<String>,
) -> Result<(), BuildError> {
    use crate::imports::ImportRule;
    use mercs2_formats::hash::pandemic_hash_m2;
    let own = |module: &str| {
        let h = pandemic_hash_m2(module);
        manifest.contributions.iter().any(|c| match c {
            Contribution::AddScript { name, .. } => pandemic_hash_m2(name.as_str()) == h,
            _ => false,
        })
    };
    for f in findings {
        if f.rule.is_warning() {
            log.push(format!("warning {f}"));
            continue;
        }
        let certain = f.rule == ImportRule::LoadOrder || (f.rule == ImportRule::Unresolved && own(&f.module));
        if certain {
            return Err(BuildError::Lower { index: 0, kind, message: f.to_string() });
        }
        log.push(format!("warning {f} (`qm link` checks this against the installed set, and refuses the set on it)"));
    }
    Ok(())
}

/// Which level a [`lower_fx`] merges for.
#[derive(Clone, Copy)]
enum FxLevel {
    /// The main campaign, `vz`.
    Main,
    /// A custom level's link ([`link_level`]).
    Custom,
}

/// `add_fx_sprite`, `add_fx` and `replace_fx` of `shipments`, in order, and the set's
/// `replace_texture` of the `vfx` atlas ([`crate::fx::merge`]): the game's effects block with every
/// edit and addition, at its own path, when the set has an `add_fx` or a `replace_fx`; and, written
/// into the resident block of `loaded`, the game's worldentity with every added template when the
/// set adds one, the `fxdict` with every sprite's record when the set adds a sprite, and the atlas
/// with every sprite drawn on the base when the set adds a sprite or repaints it.
///
/// In the main campaign (`level`), the worldentity also takes the templates of every `level: vz` row;
/// a custom level runs its own worldentity, which those rows do not reach.
///
/// Returns the effects block and the index into `loaded` of the resident block when any of its
/// entries was rewritten. The effects block carries every row the game gives its entries, copied,
/// and a primary row with sentinel rungs for each added effect; the resident block's rows are
/// copied from the game by [`script_patch_blocks`].
fn lower_fx(
    shipments: &[&LoadedShipment],
    game: &mut GameStack,
    loaded: &mut [LoadedScriptBlock],
    scope: crate::fx::Scope,
    level: FxLevel,
    kind: &'static str,
    log: &mut Vec<String>,
) -> Result<(Option<PatchBlock>, Option<usize>), BuildError> {
    let fail = |index: usize, message: String| BuildError::Lower { index, kind, message };
    let (_, resident_path) = link::SCRIPT_BLOCKS[1];
    let resident = loaded
        .iter()
        .position(|lb| lb.path == resident_path)
        .ok_or_else(|| fail(0, format!("the game stack has no {resident_path}, which holds the worldentity")))?;
    let we_at = crate::fx::worldentity_entry(&loaded[resident].block).map_err(|m| fail(0, m))?;
    let we = mercs2_formats::worldentity::WorldEntity::parse(&loaded[resident].block.entries[we_at].bytes)
        .map_err(|m| fail(0, format!("the worldentity in {resident_path}: {m}")))?;
    let fxdict_at = crate::fx::fxdict_entry(&loaded[resident].block).map_err(|m| fail(0, format!("{resident_path}: {m}")))?;
    let atlas_at = crate::fx::atlas_entry(&loaded[resident].block).map_err(|m| fail(0, format!("{resident_path}: {m}")))?;
    let (records, game_atlas) =
        crate::fx::sprites_of(&loaded[resident].block).map_err(|m| fail(0, format!("{resident_path}: {m}")))?;
    let (needle, effects_path) = crate::fx::EFFECTS_BLOCK;
    let (raw, rows) = game
        .block_and_rows_by_path(needle)
        .ok_or_else(|| fail(0, format!("the game stack has no {effects_path}")))?;
    let effects = ScriptsBlock::parse(&raw).map_err(|m| fail(0, format!("{effects_path}: {m}")))?;
    let set: Vec<crate::fx::FxShipment<'_>> =
        shipments.iter().map(|s| crate::fx::FxShipment { manifest: &s.manifest, root: &s.root }).collect();
    let (atlas, repainted) = crate::fx::base_atlas(&game_atlas, &set).map_err(|m| fail(0, m))?;
    if repainted {
        log.push(format!("{resident_path}: the vfx atlas 0x{:08X} is repainted", crate::sprite::VFX_ATLAS));
    }
    let schemas = match level {
        FxLevel::Main => crate::fx::level_schemas(game, shipments.iter().map(|s| &s.manifest)).map_err(|m| fail(0, m))?,
        FxLevel::Custom => None,
    };
    let base = crate::fx::FxBase {
        effects: &effects.entries,
        worldentity: &we,
        fxdict: &records,
        atlas: &atlas,
        repainted,
        level_templates: match level {
            FxLevel::Main => crate::fx::LevelTemplates::main(schemas.as_ref()),
            FxLevel::Custom => crate::fx::LevelTemplates::Skip,
        },
    };
    let merged = crate::fx::merge(&base, &set, scope).map_err(|f| {
        let index = f.problems.first().map(|p| p.index).or_else(|| f.conflicts.first().map(|c| c.claimants[0].index));
        fail(index.unwrap_or(0), f.to_string())
    })?;
    log.extend(merged.log.iter().cloned());

    let effects_block = if shipments.iter().any(|s| crate::fx::has_fx(&s.manifest)) {
        let mut aset = Vec::with_capacity(merged.effects.len());
        for e in &merged.effects {
            aset.push(if merged.added.contains(&e.name_hash) {
                AsetEntry::new(e.name_hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_EFFECT)
            } else {
                let &(packed, secondary, type_id) = rows.get(&e.name_hash).ok_or_else(|| {
                    fail(0, format!("{effects_path} entry 0x{:08X} has no ASET row in the game", e.name_hash))
                })?;
                AsetEntry::new(e.name_hash, secondary, packed, type_id)
            });
        }
        let block = ScriptsBlock { entries: merged.effects }.serialize();
        log.push(format!(
            "{effects_path}: {} entries ({} added), {} bytes",
            aset.len(),
            merged.added.len(),
            block.len()
        ));
        Some(PatchBlock::from_decompressed(&block, effects_path.to_string(), aset, None).map_err(|m| fail(0, m))?)
    } else {
        None
    };
    let mut rewritten = false;
    if let Some(we) = merged.worldentity {
        let bytes = we.write().map_err(|m| fail(0, format!("writing the worldentity: {m}")))?;
        log.push(format!(
            "{resident_path}: worldentity 0x{:08X} {} -> {} bytes, {} templates",
            mercs2_formats::worldentity::RETAIL_WORLDENTITY_NAME_HASH,
            loaded[resident].block.entries[we_at].bytes.len(),
            bytes.len(),
            we.instances.len()
        ));
        loaded[resident].block.entries[we_at].bytes = bytes;
        rewritten = true;
    }
    if let Some(bytes) = merged.fxdict {
        log.push(format!(
            "{resident_path}: fxdict 0x{:08X} {} -> {} bytes, {} sprite(s) added",
            crate::fx::FXDICT_NAME_HASH,
            loaded[resident].block.entries[fxdict_at].bytes.len(),
            bytes.len(),
            merged.placed.len()
        ));
        loaded[resident].block.entries[fxdict_at].bytes = bytes;
        rewritten = true;
    }
    if let Some(bytes) = merged.atlas {
        log.push(format!(
            "{resident_path}: vfx atlas 0x{:08X}, {} bytes{}",
            crate::sprite::VFX_ATLAS,
            bytes.len(),
            match merged.square {
                Some(sq) => format!(", sprites drawn into the free square {sq}"),
                None => String::new(),
            }
        ));
        loaded[resident].block.entries[atlas_at].bytes = bytes;
        rewritten = true;
    }
    Ok((effects_block, rewritten.then_some(resident)))
}

/// The loader self-check ([`crate::sound::check_loader_banks`]): every bank qm ships that the gameplay
/// loader loads has its wavebank in `overlay`, and every one the front-end loader loads has its
/// wavebank in `shell`. A `load_sound` bank or stream is not qm's: `lint::game_checks` (M0223) checks it
/// against the game and the Shipment's own rows.
fn check_sound_loaders(
    sound_regs: &[link::SoundBankRegistration],
    overlay: &[&PatchBlock],
    shell: &[&PatchBlock],
    kind: &'static str,
) -> Result<(), BuildError> {
    use crate::manifest::LoadSession;
    for (session, blocks) in [(LoadSession::Gameplay, overlay), (LoadSession::FrontEnd, shell)] {
        let banks: Vec<&str> = sound_regs
            .iter()
            .filter(|r| r.sessions.contains(&session) && matches!(r.load, link::SoundLoad::Shipped { .. }))
            .map(|r| r.bank.as_str())
            .collect();
        crate::sound::check_loader_banks(link::Level::of(session), &banks, blocks)
            .map_err(|message| BuildError::Lower { index: 0, kind, message })?;
    }
    Ok(())
}

/// The CSUM row value of the `shell.wad` beside `game`'s base WAD, which a shell patch is stamped with.
fn shell_csum(game: &GameStack, kind: &'static str) -> Result<u32, BuildError> {
    let fail = |message: String| BuildError::Lower { index: 0, kind, message };
    let base = game.paths().first().map(|p| p.to_path_buf()).ok_or_else(|| fail("the game stack is empty".into()))?;
    let shell = crate::sound::sibling_wad(&base, "shell.wad").map_err(fail)?;
    mercs2_formats::donor::base_csum(&shell).map_err(fail)
}

/// Where a built artifact has to end up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Inside the Shipment's overlay WAD.
    Overlay,
    /// A file placed in the game folder, at this path relative to it — an `.asi` in the loader's
    /// search path, or a companion beside it.
    GameFolder { relative: String },
    /// A NEW WAD in `data/`, at this path relative to the game folder: `data/<name>.wad` for
    /// `add_language`, `data/<name>-patch.wad` for `add_level`. It is ADDITIVE and collision-checked,
    /// never over a shipped WAD. `display` is the label a selector shows for it.
    DataWad { relative: String, display: String },
    /// A patch WAD, at `relative` under the output directory, whose blocks a deploy step merges —
    /// with every other installed Shipment's for the same `language`, a link output last — into
    /// `data/<language>-patch.wad`, which the engine mounts directly above `data/<language>.wad`
    /// (`FUN_004BFEF0`: `%s\%s-patch.wad` with the language table's name).
    LanguagePatch { language: String, relative: String },
    /// A patch WAD, named by [`Placement::name`] in the output directory, whose blocks a deploy step
    /// merges — with every other installed Shipment's, a link output last — into
    /// `data/shell-patch.wad`, which the engine mounts directly above `shell.wad` in the front end
    /// (`FUN_004BFDA0`: `%s\%s-patch.wad` with the level name `FUN_004C1280` sets to `shell`).
    ShellPatch,
    /// A level's patch WAD, at `relative` (`data/<level>-patch.wad`) under the output directory, the
    /// same path it takes under the game folder. `vz-patch.wad` is not mounted in a level, so a deploy
    /// step merges every installed Shipment's overlay under it. `qm link`'s supersedes `qm build`'s:
    /// it has the set's scripts in it. `display` is the label a selector shows.
    LevelPatch { level: String, relative: String, display: String },
    /// A copy, made by the deploy step, of the game file at `from` to `to` (both relative to the game
    /// folder). No bytes are written to the output directory; [`Placement::sha256`] and
    /// [`Placement::bytes`] describe `from` as the build read it.
    StreamCopy { from: String, to: String },
    /// A game data file at `relative` (a closed set: `data/shader3.bin`, `data/shader3Low.bin`),
    /// written under the output directory at the same relative path: the original store from
    /// `--original-data`, whose sha256 is `base_sha256`, with the Shipment's (or, from `qm link`,
    /// the installed set's) shader edits applied.
    DataFile { relative: crate::shader::DataFile, base_sha256: String },
}

/// One emitted artifact and its digest.
///
/// For a [`Destination::GameFolder`] artifact, `relative` names the file BOTH under the build
/// directory and under the game folder — the output mirrors the tree it will be copied into, so a
/// deploy step never has to reconstruct one from the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The bare filename, for messages. The path is on the [`Destination`].
    pub name: String,
    pub bytes: usize,
    pub sha256: String,
    pub destination: Destination,
}

/// Where the Quartermaster puts an `.asi`, relative to the game folder.
///
/// **The author never names this**, which is the whole point: `native_hook` has no `dest` field, so
/// there is no spelling of a Shipment that writes next to `Mercenaries2.exe` or into `data\vz.wad`.
/// Those stay unreachable by construction rather than by a lint rule somebody could suppress.
///
/// `pmc_bb.dll` (v3.0.0, read directly: the format strings `%s*.asi`, `%sscripts\`, `%splugins\`,
/// `%supdate\`) globs four roots — the game directory itself and these three subfolders. `scripts\`
/// is chosen because it is where the ecosystem already puts them (`cruise.asi`, `dlc_enable.asi`)
/// and because keeping mod files out of the game root makes an uninstall obvious.
///
/// A forward slash on purpose: the loader's own literal is `scripts\`, but this string is a
/// filesystem path a deploy tool joins, not an engine path like the backslashed PTHS entries.
pub const ASI_SUBDIR: &str = "scripts";

/// The one `.asi` name the loader refuses to load: it skips its own.
///
/// Read from the binary, not assumed. A plugin shipped under this name would be placed correctly,
/// hash correctly, and never load — with the loader logging nothing at all, because it never
/// considered the file.
pub const RESERVED_ASI: &str = "pmc_bb.asi";

/// The base-WAD basenames the game already ships, which `add_language` must never overwrite.
///
/// The engine opens `.\Data\<name>.wad` by name; a novel language is ADDITIVE, so it may only
/// introduce a name the game does not ship. These are the level/shell/loading WADs plus the six
/// shipped language WADs — placing over any of them would shadow base-game data.
pub const RESERVED_WAD_BASENAMES: &[&str] = &[
    "vz", "shell", "loading", "english", "french", "german", "italian", "spanish", "japanese",
    "russian",
];

/// Refuse an `add_language` name that could not be a novel language WAD.
///
/// Two failure modes, both about `name` becoming BOTH `.\Data\<name>.wad` AND the stringdb key: it
/// must be a lowercase `[a-z0-9_]` token (a single, filesystem-safe path component — the engine's own
/// language names are `english`, `english_uk`), and it must not be a WAD the game already ships, which
/// this kind would otherwise overwrite. This is the `data/` safety pivot in code: [`PlaceIn`] bans
/// `data/` outright, and `add_language` earns it back only for a builder-derived, collision-checked
/// name. Called by the linter (M0200, so template CI says so) AND the lowering (so the refusal
/// survives a suppressed rule).
pub fn language_name_refusal(name: &str) -> Option<String> {
    wad_name_refusal("add_language", name)
}

/// The table an `add_language` forks when it names no `base`.
pub const LANGUAGE_BASE: &str = "english";

/// An `add_language`'s string table as the build writes it, before it is re-keyed to the language's
/// name: `base` (or [`LANGUAGE_BASE`]) as `game` ships it, with the `strings` file (under `root`)
/// applied by [`mercs2_formats::stringdb::edit_container`]. Also the edits read. A base the stack
/// does not hold, a file that does not read, one with no edits, or a key the base lacks is refused.
pub fn language_table(
    game: &mut GameStack,
    root: &Path,
    strings: &Path,
    base: Option<&str>,
) -> Result<(std::collections::BTreeMap<String, String>, StringTableWrite), String> {
    let base_name = base.unwrap_or(LANGUAGE_BASE);
    let base_hash = crate::manifest::asset_hash(base_name);
    let container = game.container_for_asset(base_hash, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB).ok_or_else(|| {
        format!(
            "base table {base_name:?} (0x{base_hash:08X}) is not a string table in the configured game \
             stack — a new language forks a shipped table, so `base` must name one that exists (default \
             `{LANGUAGE_BASE}`)"
        )
    })?;
    let path = root.join(strings);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let edits = parse_string_edits(&text)?;
    if edits.is_empty() {
        return Err(format!(
            "{} declares no strings — a language identical to {base_name} is not a new language",
            path.display()
        ));
    }
    let merged = mercs2_formats::stringdb::edit_container(&container, &edits)?;
    Ok((edits, StringTableWrite { table: base_hash, target: base_name.to_string(), copy: TableCopy::Vz, base: container, merged }))
}

/// Refuse a name `kind` would turn into a new base WAD: [`language_name_refusal`]'s two failure modes.
pub fn wad_name_refusal(kind: &str, name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty() {
        return Some("it is empty".into());
    }
    if !n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Some(format!(
            "{n:?} is not a token — expected lowercase a-z, digits and underscore only. It becomes the \
             filename `.\\Data\\{n}.wad` and is hashed by name, so a separator, an uppercase letter or \
             punctuation cannot appear in it"
        ));
    }
    if RESERVED_WAD_BASENAMES.contains(&n) {
        return Some(format!(
            "{n:?} is a WAD the game already ships (`.\\Data\\{n}.wad`). {kind} only ADDS and must never \
             overwrite a base WAD — pick a name that is not one of: {}",
            RESERVED_WAD_BASENAMES.join(", ")
        ));
    }
    None
}

/// Join a game-folder directory and a filename into the relative path a deploy tool writes.
///
/// One function so the PLACEMENT and the [`crate::blast::Claim::FileArtifact`] that guards it can
/// never disagree — a claim computed one way and a path emitted another is a conflict system that
/// quietly stops matching. Forward slashes, and the game root is the empty string so joining is
/// uniform.
pub fn place_path(dir: &str, file: &str) -> String {
    if dir.is_empty() {
        file.to_string()
    } else {
        format!("{dir}/{file}")
    }
}

/// Extensions no Shipment may write into the game folder, whatever destination it names.
///
/// This is the second half of "the exe and the WADs are unreachable". [`crate::manifest::PlaceIn`]
/// takes the DESTINATION out of the author's hands; this takes the parts of the FILENAME that could
/// still clobber something load-bearing. Both halves are needed: `dest: game_root` is a legitimate
/// destination the loader really globs, and it is also where `Mercenaries2.exe` lives.
const FORBIDDEN_PLACEMENT_EXT: &[(&str, &str)] = &[
    (
        "wad",
        "a WAD is the base game's data (`data\\vz.wad`) or a Shipment's own overlay. The overlay is \
         emitted as `_build/<name>.wad` and mounted by the deploy step — it is never placed by an \
         author, and the format cannot express a write into the base WAD at all",
    ),
    (
        "exe",
        "`Mercenaries2.exe` is the game. An exe edit stays unrepresentable rather than merely \
         linted, and that is only true if no file placement can write one",
    ),
    (
        "dll",
        "a DLL in the game folder is either the game's own, the loader (`pmc_bb.dll`) or a runtime \
         other plugins import by name. A plugin ships as an `.asi` through `native_hook`, and a \
         runtime DLL ships through `add_runtime_dll`, which places `<shipment.name>.dll` in the \
         game root and refuses the loader's and the game's names",
    ),
];

/// Reject a filename that must not be written into the game folder, for any destination.
///
/// Everything here is about the NAME, because the name is the only part of a placement an author
/// influences — it comes from the source file, so `src/../..` is already an M0111 error before this
/// runs. What is left is a filename that is not a single path component (a deploy tool joining
/// `scripts/` + `..\..\Mercenaries2.exe`, or + `C:\evil`, or + `\\host\share\x` escapes the game
/// folder on the Windows machine that consumes the record, even though every one of those is a
/// perfectly ordinary filename on the macOS machine that built it), and a name that would clobber
/// something load-bearing.
///
/// Applies to `native_hook` too. Its extension check is narrower — it REQUIRES `.asi` — but the
/// component and reserved-name rules are the same file-in-the-game-folder rules.
pub fn game_folder_name_refusal(name: &str) -> Option<String> {
    if let Some(why) = single_filename_refusal(name) {
        return Some(why);
    }
    if name.eq_ignore_ascii_case(RESERVED_ASI) {
        return Some(format!(
            "{RESERVED_ASI} is reserved: the loader skips its own name, so a file shipped under it \
             is never loaded and nothing is logged, because the file is never considered"
        ));
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    if let Some(ext) = ext {
        for (denied, why) in FORBIDDEN_PLACEMENT_EXT {
            if ext == *denied {
                return Some(format!("it is a `.{denied}`, and {why}"));
            }
        }
    }
    None
}

/// Reject a name that is not ONE filename: empty, `.`/`..`, or carrying a separator or a drive
/// colon.
///
/// The component half of [`game_folder_name_refusal`], on its own because a name can need it without
/// being a file qm writes: a `supersedes` entry names a legacy file to DETECT in the game folder, so
/// the extension and reserved-name refusals, which exist to stop a write, do not apply to it.
pub fn single_filename_refusal(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("it has no filename at all".into());
    }
    if name == "." || name == ".." {
        return Some(format!(
            "{name:?} names a directory, not a file. A placement is a single file written into the \
             game folder"
        ));
    }
    if let Some(bad) = name.chars().find(|c| matches!(c, '/' | '\\' | ':')) {
        return Some(format!(
            "it contains {bad:?}, so it is not a single filename. A deploy tool joins this onto a \
             game-folder directory ON WINDOWS, where a separator, a drive letter (`C:\\…`) or a UNC \
             prefix (`\\\\host\\share`) would leave the game folder entirely — while on the \
             machine that built the Shipment all three are ordinary characters in a filename"
        ));
    }
    None
}

/// Reject the file name of an `add_runtime_dll`, or `None` when it may be placed in the game root.
///
/// The single-filename rule is [`single_filename_refusal`], the same one every game-folder placement
/// uses. On top of it, and all compared lowercased because Windows file names are case-insensitive:
///
/// * the extension must be `.dll`;
/// * the stem must not be on [`crate::manifest::DENY_LISTED_DLL_STEMS`] — the loader, its sidecar
///   and the game-folder DLLs the loader reports;
/// * the name must be `<shipment_name>.dll`, so a runtime Shipment ships exactly one DLL, named
///   after itself.
///
/// One function, called by the linter (M0162), `qm preflight` / `qm link` (M0162 in the plan) and
/// the lowering (so the refusal survives a rule being suppressed).
pub fn runtime_dll_name_refusal(name: &str, shipment_name: &str) -> Option<String> {
    if let Some(why) = single_filename_refusal(name) {
        return Some(why);
    }
    let lowered = name.to_lowercase();
    let Some(stem) = lowered.strip_suffix(".dll") else {
        return Some(
            "it is not a `.dll`. `add_runtime_dll` places a runtime DLL that plugins import by \
             name; a plugin is an `.asi` shipped through `native_hook`, and any other file is a \
             `place_file`"
                .into(),
        );
    };
    if crate::manifest::DENY_LISTED_DLL_STEMS.contains(&stem) {
        return Some(format!(
            "`{stem}.dll` is a DLL no Shipment may ship ({}, compared case-insensitively): the \
             loader, its sidecar and the game-folder DLLs the loader reports",
            crate::manifest::DENY_LISTED_DLL_STEMS
                .iter()
                .map(|s| format!("{s}.dll"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let wanted = format!("{}.dll", shipment_name.to_lowercase());
    if lowered != wanted {
        return Some(format!(
            "a runtime DLL must be named after its Shipment, `{wanted}` (compared \
             case-insensitively), so that one runtime Shipment ships exactly one DLL and two \
             Shipments can never ship one name"
        ));
    }
    None
}

/// Reject a filename a `place_file` may not use: everything [`game_folder_name_refusal`] rejects,
/// plus an `.asi`.
///
/// An `.asi` is a plugin, not a companion. Refusing it here rather than quietly placing it is what
/// stops `place_file` being a way around `native_hook`: that kind reads the PE headers
/// `LoadLibrary` will reject, refuses the loader's reserved name, and records the addresses the
/// plugin hooks so two plugins fighting over one is a conflict rather than whichever the filesystem
/// enumerated first. A companion route that also produced a loadable `*.asi` would skip all three
/// and still yield a file the loader globs.
///
/// One function, called by both the linter (M0162, so template CI says so on the push) and the
/// lowering (so the refusal survives a rule being suppressed).
pub fn companion_name_refusal(name: &str) -> Option<String> {
    if name.to_ascii_lowercase().ends_with(".asi") {
        return Some(
            "it is an `.asi`, which the loader globs and loads as native code. Ship it as a \
             `native_hook` instead: that reads the PE headers `LoadLibrary` will reject, refuses \
             the loader's own reserved name, and records the addresses it hooks so two plugins \
             fighting over one is a conflict rather than whichever the filesystem enumerated first"
                .into(),
        );
    }
    game_folder_name_refusal(name)
}

/// What lowering one contribution produced.
///
/// Three outcomes rather than `Option<PatchBlock>`, because the Code layer genuinely does not
/// produce a block: an `.asi` is a file in the game folder, and a format that could only express
/// WAD content could not describe our own live bridge.
enum Lowering {
    /// Nothing to emit here. The contribution's effect is realised elsewhere — a `patch_lua`
    /// declares a mutation that the linker applies later.
    Nothing,
    Block(PatchBlock),
    /// A model's LOD chain ([`emit_model_chain`]): its blocks number each other by group-local source
    /// indices, so [`build`] renumbers the group past every other block before it joins the overlay.
    Chain(Vec<PatchBlock>),
    /// Several blocks from one contribution. A skinned outfit with `textures:` emits the model
    /// block plus one `TYPE_ID_TEXTURE` block per supplied map, and they have to travel together —
    /// the model's MTRL repoints name hashes that only resolve if these ship alongside it.
    Blocks(Vec<PatchBlock>),
    /// An `add_movie` / `add_ui` block, bound for the patch of the level that mounts the WAD retail ships
    /// the movie in ([`crate::retail_movies::route`]).
    Movie(PatchBlock, crate::retail_movies::MovieRoute),
    /// A NEW language's base WAD placed in `data/`. The engine mounts
    /// `.\Data\<language>.wad` in its language slot, above the level WAD (`FUN_004BFE20`), so its
    /// string table, fonts and voice-over tables resolve from there. `stream_copy` is the voice
    /// stream the language plays from, as `(from, to)` under the game folder.
    LanguageWad {
        language: String,
        display: String,
        blocks: Vec<PatchBlock>,
        stream_copy: (String, String),
    },
    /// A level ([`crate::level`]): `blocks` become `data/<name>-patch.wad`, and the game file
    /// [`crate::level::BASE_FROM`] is copied to `data/<name>.wad` at deploy.
    LevelWad {
        name: String,
        display: String,
        level: crate::level::Level,
    },
    /// A file placed in the game folder. Carries its bytes so the caller writes them exactly once,
    /// next to the digest it records for them.
    File {
        name: String,
        relative: String,
        bytes: Vec<u8>,
    },
}

/// Read the assembled WAD back and run [`crate::lint::artifact_checks`] on it.
///
/// Deliberately re-parses the bytes rather than checking the in-memory `Vec<PatchBlock>` that went
/// in. Those blocks have not been through `build_patch_wad_multi`'s LOD-rung remap, so their rungs
/// are still source-relative — checking them would answer the wrong question. More usefully, this
/// verifies what will actually be on disk, so a serializer bug is in scope too.
///
/// Fails the build on any blocking finding. That is the whole value: both structural bugs this
/// crate has shipped were invisible in the manifest and plain in the bytes.
pub fn verify_emitted(wad: &[u8], keys: &crate::shader::ShaderKeys) -> Result<Vec<crate::lint::Diagnostic>, BuildError> {
    let contents =
        mercs2_formats::patch_wad::read_patch_wad(wad).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "verify",
            message: format!("the WAD we just wrote does not read back: {m}"),
        })?;
    // The CSUM meta sizes the engine's block-entry install ring, which never shrinks below the largest
    // meta mounted; a block with more entries than that is retried forever and its level never starts.
    let meta = u32::from_le_bytes([wad[0x2C], wad[0x2D], wad[0x2E], wad[0x2F]]);
    for b in &contents.blocks {
        let n = mercs2_formats::patch_wad::block_entry_count(b)
            .map_err(|m| BuildError::Lower { index: 0, kind: "verify", message: format!("the WAD we just wrote: {m}") })?;
        if n > meta {
            return Err(BuildError::Lower {
                index: 0,
                kind: "verify",
                message: format!("the WAD we just wrote has CSUM meta {meta}, and {} holds {n} entries: the game would never install it", b.path_string),
            });
        }
    }
    let mut found = crate::lint::artifact_checks(&contents.blocks);
    found.extend(crate::lint::shader_key_checks(&contents.blocks, &keys.pixel, &keys.vertex));
    if crate::lint::blocks_build(&found) {
        return Err(BuildError::Artifact { diagnostics: found });
    }
    Ok(found)
}

#[derive(Debug, Clone)]
pub struct BuildReport {
    /// Everything the linter said, including non-blocking warnings.
    pub diagnostics: Vec<Diagnostic>,
    /// The overlay WAD, when any contribution produced one.
    pub wad: Option<PathBuf>,
    /// Every artifact with its digest — the record deploy/undo consumes.
    pub placements: Vec<Placement>,
    pub log: Vec<String>,
}

#[derive(Debug)]
pub enum BuildError {
    /// The linter found something at `Error` or above.
    Blocked(Vec<Diagnostic>),
    /// A contribution needed the retail WADs and none were configured.
    GameRequired {
        index: usize,
        kind: &'static str,
    },
    /// The configured stack is a console bake and we cannot yet EMIT for one.
    ConsoleOutputUnsupported,
    /// A contribution the lowering refuses, with the reason.
    Unsupported {
        index: usize,
        kind: &'static str,
        reason: String,
    },
    Lower {
        index: usize,
        kind: &'static str,
        message: String,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    /// The WAD we just assembled failed its own self-check. Always a builder bug rather than an
    /// author one, and fatal on purpose: the whole point of a HANG-class rule is that the game will
    /// not tell anybody what went wrong.
    Artifact {
        diagnostics: Vec<crate::lint::Diagnostic>,
    },
    /// The load plan over the set is not ok, so nothing is linked. The plan carries the findings
    /// and has been written beside where the link output would go.
    Plan(Box<crate::plan::LoadPlan>),
    /// A file this Shipment supersedes is still in the game folder. qm never deletes it.
    Superseded {
        shipment: String,
        relative: String,
    },
    /// The plan or a superseded-file probe could not be computed at all.
    Compat(crate::compat::CompatError),
    /// A shader kind is present and no `--original-data` directory was given: the stores are read
    /// only from there.
    OriginalDataRequired { index: usize, kind: &'static str },
    /// The original stores, or the game's VT and R2VB store pairs, do not read.
    ShaderData(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Blocked(d) => {
                let blocking = d.iter().filter(|x| x.severity >= lint::Severity::Error).count();
                writeln!(
                    f,
                    "build blocked by {blocking} finding(s) at error or above ({} finding(s) in all):",
                    d.len()
                )?;
                let mut sorted: Vec<&Diagnostic> = d.iter().collect();
                sorted.sort_by(|a, b| b.severity.cmp(&a.severity));
                for x in sorted {
                    writeln!(f, "  {x}")?;
                }
                Ok(())
            }
            BuildError::GameRequired { index, kind } => write!(
                f,
                "contributions[{index}] ({kind}) needs the retail WADs — configure the game folder \
                 (Workshop Settings, or `qm --game <dir>`). `qm lint` runs without one."
            ),
            BuildError::ConsoleOutputUnsupported => write!(
                f,
                "the configured game stack is a CONSOLE bake (Xbox 360 / PS3, big-endian `SCFF`), \
                 and we cannot emit for one yet. Note this is not merely an endianness flip: \
                 `ucfx_byteswap` converts console → PC only, and the reverse needs GPU texture \
                 RE-tiling, XMA/Xbox-ADPCM audio encoding, big-endian Lua bytecode and Xbox vertex \
                 declarations. Reading a console WAD is supported; writing one is not."
            ),
            BuildError::Unsupported {
                index,
                kind,
                reason,
            } => {
                write!(
                    f,
                    "contributions[{index}] ({kind}) cannot be lowered: {reason}"
                )
            }
            BuildError::Lower {
                index,
                kind,
                message,
            } => {
                write!(f, "contributions[{index}] ({kind}): {message}")
            }
            BuildError::Io { path, message } => write!(f, "{}: {message}", path.display()),
            BuildError::Artifact { diagnostics } => {
                write!(f, "the assembled WAD failed its self-check:")?;
                for d in diagnostics {
                    write!(f, "\n  {d}")?;
                }
                Ok(())
            }
            BuildError::Plan(plan) => {
                let errors: Vec<&crate::plan::Finding> = plan
                    .findings
                    .iter()
                    .filter(|x| x.severity == crate::plan::FindingSeverity::Error)
                    .collect();
                write!(
                    f,
                    "the load plan is not ok — {} error finding(s), so nothing was linked:",
                    errors.len()
                )?;
                for x in errors {
                    write!(f, "\n  [{}] {}: {}", x.code, x.items.join(", "), x.message)?;
                }
                Ok(())
            }
            BuildError::Superseded { shipment, relative } => write!(
                f,
                "{shipment} supersedes {relative}, which is still in the game folder — remove it \
                 first (qm never deletes it)"
            ),
            BuildError::Compat(e) => write!(f, "{e}"),
            BuildError::OriginalDataRequired { index, kind } => write!(
                f,
                "contributions[{index}] ({kind}) edits the shader stores, which are read only from \
                 the original data directory: pass --original-data <dir> holding the game's own \
                 shader3.bin and shader3Low.bin"
            ),
            BuildError::ShaderData(message) => write!(f, "shader stores: {message}"),
        }
    }
}

impl std::error::Error for BuildError {}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Which drawing group of the donor hosts the injected geometry on the rigid path when the manifest
/// names none: an `add_model` without `group:`, and a rigid `add_outfit`, which has no `group:`
/// field. Group 0 is the common case for a simple prop.
const DEFAULT_TARGET_GROUP: usize = 0;

/// M0317 against the corpus, as a build error: every `set_spawn_lists` row of `manifest` names a call
/// of its target the wrapper can set, and a list the shipped Lua sets
/// ([`crate::lint::spawn_list_source_checks`]). `who` leads each message.
fn check_spawn_lists(manifest: &crate::manifest::Manifest, corpus_root: &Path, who: &str) -> Result<(), BuildError> {
    let found = crate::lint::spawn_list_source_checks(manifest, corpus_root);
    match found.first() {
        None => Ok(()),
        Some(d) => Err(BuildError::Lower {
            index: d.at.unwrap_or(0),
            kind: "set_spawn_lists",
            message: format!("{who}{}", found.iter().map(|d| d.message.as_str()).collect::<Vec<_>>().join("; ")),
        }),
    }
}

/// Every script mutation a Shipment declares for `level` (`vz` or a custom level's name), derived from
/// its manifest alone. A `patch_lua` and a `set_spawn_lists` apply in every level; an `add_shop_item`
/// only in the levels it lists.
///
/// **Hermetic on purpose — no game stack, no lowering.** That is what lets the same function serve
/// two callers that are otherwise very different: [`build`], linking one Shipment so its overlay is
/// valid standalone, and [`link_installed`], linking N Shipments at deploy so none of them
/// overwrites another. If mutations could only be obtained as a side effect of lowering, the deploy
/// path would have to re-run model injection just to find out which scripts are touched.
pub fn script_mutations(
    manifest: &crate::manifest::Manifest,
    root: &Path,
    level: &str,
) -> Result<Vec<ScriptMutation>, BuildError> {
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        match c {
            // add_outfit's Script half is no append: the link generates each level's wardrobe lists
            // and character map from every added outfit at once ([`added_outfits`], `crate::outfits`).
            Contribution::AddOutfit { .. } => {}
            // add_ui's Script half is NOT a plain append. Its FlashWidget registration is baked into
            // the generated `qm_modloader` script and reached through a one-line trampoline the
            // linker synthesizes — so the resident never grows with mod count. Collected separately
            // by `ui_registrations`; the Data half (the cfx_pack movie) is in `lower`.
            Contribution::AddUi { .. } => {}
            // activate_layer contributes no script APPEND either — its marks are baked into
            // `qm_modloader` from `layer_registrations`, not concatenated onto a base script.
            Contribution::ActivateLayer { .. } => {}
            Contribution::PatchLua { target, append } => {
                let path = root.join(append);
                let source = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
                    index,
                    kind: "patch_lua",
                    message: format!("reading {}: {e}", path.display()),
                })?;
                out.push(ScriptMutation {
                    shipment: shipment.clone(),
                    target: target.clone(),
                    append: source,
                });
            }
            // No rows, no Lua: an empty `set_spawn_lists` claims no script block.
            Contribution::SetSpawnLists { target, lists } if !lists.is_empty() => {
                out.push(ScriptMutation {
                    shipment: shipment.clone(),
                    target: target.clone(),
                    append: crate::spawn_lists::wrapper(lists),
                });
            }
            Contribution::AddShopItem {
                id,
                name,
                description,
                icon,
                shops,
                catalog,
                item_type,
                cash_cost,
                fuel_cost,
                max_stock,
                unlocked,
                behaviour,
                equipment_type,
                levels,
            } => {
                use crate::manifest::ShopCatalog;
                if levels.is_empty() {
                    return Err(BuildError::Lower {
                        index,
                        kind: "add_shop_item",
                        message: format!("shop item {id:?}: `levels` is empty, so it is sold in no level"),
                    });
                }
                if !levels.iter().any(|l| l == level) {
                    continue;
                }
                // A NOVEL support behaviour (behaviour.script set) is NOT a load-time append: it
                // defers into `qm_modloader` via `support_registrations`, because its `module:Create()`
                // would run against a nil global at resident-load. Skip it here.
                if behaviour.as_ref().is_some_and(|b| b.script.is_some()) {
                    continue;
                }
                match catalog {
                    ShopCatalog::Support => {
                        let b = behaviour.as_ref().ok_or_else(|| BuildError::Lower {
                            index,
                            kind: "add_shop_item",
                            message: format!(
                                "support shop item {id:?} needs a `behaviour` (the oSupport module)"
                            ),
                        })?;
                        let itype = item_type.as_ref().map_or("Supply", |t| t.lua());
                        let unlock = link::shop_unlock_table(shops, *unlocked);
                        out.push(ScriptMutation {
                            shipment: shipment.clone(),
                            target: "mrxsupportdata".into(),
                            append: link::shop_support_row_append(
                                id,
                                name,
                                description,
                                icon,
                                itype,
                                *cash_cost,
                                *fuel_cost,
                                *max_stock,
                                &unlock,
                                &b.module,
                                b.cargo.as_deref(),
                                b.delivery_vehicle.as_deref(),
                            ),
                        });
                    }
                    ShopCatalog::Equipment => {
                        let et = equipment_type.as_ref().ok_or_else(|| BuildError::Lower {
                            index,
                            kind: "add_shop_item",
                            message: format!(
                                "equipment shop item {id:?} needs an `equipment_type` \
                                 (fuel_tank | grappling_hook)"
                            ),
                        })?;
                        out.push(ScriptMutation {
                            shipment: shipment.clone(),
                            target: "wifequipmentdata".into(),
                            append: link::shop_equipment_row_append(
                                id,
                                name,
                                description,
                                icon,
                                et.lua_const(),
                                *cash_cost,
                            ),
                        });
                    }
                }
                let field = match catalog {
                    ShopCatalog::Support => "tSupport",
                    ShopCatalog::Equipment => "tEquipment",
                };
                out.push(ScriptMutation {
                    shipment: shipment.clone(),
                    target: "mrxrewarddata".into(),
                    append: link::shop_reward_append(id, field, shops),
                });
            }
            _ => {}
        }
    }
    Ok(out)
}

/// An `add_placement` / `add_layer` entity file ([`crate::world::parse_entities`]), read and
/// refused exactly as an `edit_world` edits file is: `reading <path>: <error>` when it cannot be read,
/// `<path>: <why>` when it does not parse.
pub(crate) fn read_entities_file(path: &Path) -> Result<Vec<mercs2_formats::placement_build::NewEntity>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    crate::world::parse_entities(&text).map_err(|m| format!("{}: {m}", path.display()))
}

/// `container` (a layer sub-block) with `ents` inserted in place, each with the `flgs` state of a
/// `Name` + `ModelName` + `Transform` entity, after any of those COMPs the layer lacks is added
/// from `scaffold` ([`mercs2_formats::placement_build::with_comps`]).
fn insert_entities(
    container: &[u8],
    ents: &[mercs2_formats::placement_build::NewEntity],
    scaffold: &mercs2_formats::placement_build::LayerScaffold,
) -> Result<Vec<u8>, String> {
    use mercs2_formats::placement_build as pb;
    let mut c = pb::with_comps(container, scaffold)?;
    for e in ents {
        c = pb::insert_entity(&c, e, pb::FLGS_MODEL_PLACEMENT)?;
    }
    Ok(c)
}

/// One write of a Shipment's into a layer block: an `edit_world`'s edits, an `add_placement`'s or
/// `add_layer`'s entities, or a stand-in's placement.
#[derive(Debug, Clone)]
pub enum WorldWrite {
    /// `edit_world`: `doc` (read from `file`) applied to the block `layer` names.
    Edit { layer: String, file: PathBuf, doc: crate::world::WorldDoc },
    /// `add_placement`: `ents` inserted into the layer named `layer`.
    Place { layer: String, ents: Vec<mercs2_formats::placement_build::NewEntity> },
    /// `add_layer`: a new layer `name` placing `ents`, in the block `template` names.
    NewLayer { name: String, template: String, ents: Vec<mercs2_formats::placement_build::NewEntity> },
    /// `add_tiny_geometry`: the stand-in's placement into its layer.
    StandIn(crate::tiny::StandInPlace),
}

impl WorldWrite {
    /// The write contribution `c` (of the Shipment at `root`) makes, its file read: `None` for a
    /// kind that writes no layer block, or for `add_tiny_geometry`, whose placement comes from its
    /// lowering ([`crate::tiny::lower_shipment`]).
    pub fn of(c: &Contribution, root: &Path) -> Result<Option<WorldWrite>, String> {
        Ok(match c {
            Contribution::EditWorld { layer, edits } => {
                let file = root.join(edits);
                let text = std::fs::read_to_string(&file).map_err(|e| format!("reading {}: {e}", file.display()))?;
                let doc = crate::world::parse(&text).map_err(|m| format!("{}: {m}", file.display()))?;
                Some(WorldWrite::Edit { layer: layer.clone(), file, doc })
            }
            Contribution::AddPlacement { layer, entity } => {
                Some(WorldWrite::Place { layer: layer.clone(), ents: read_entities_file(&root.join(entity))? })
            }
            Contribution::AddLayer { name, template, entities } => Some(WorldWrite::NewLayer {
                name: name.clone(),
                template: template.clone(),
                ents: read_entities_file(&root.join(entities))?,
            }),
            _ => None,
        })
    }

    /// The stack's block the write goes into: the block a needle names for an `edit_world` and an
    /// `add_layer` ([`GameStack::layer_block_for_edit`]), the block holding the layer of that name
    /// for an `add_placement` and a stand-in ([`GameStack::layer_by_name`]). A name that is no layer
    /// of the stack — a block path among them — is refused.
    pub fn base(&self, game: &mut GameStack) -> Result<crate::game::LayerEditInputs, String> {
        let by_name = |game: &mut GameStack, layer: &str| {
            game.layer_by_name(layer)?.map(|(inputs, _)| inputs).ok_or_else(|| {
                format!("{layer:?} is not the name of a layer of the game stack; `layer` names one layer, like vz_state_pmccon004, not a block path")
            })
        };
        match self {
            WorldWrite::Edit { layer, .. } => game.layer_block_for_edit(layer).ok_or_else(|| {
                format!(
                    "no layer matching {layer:?} in the game stack — the target is a PTHS-path needle \
                     like \"vz_state_pmccon004\" or \"layers_static\""
                )
            }),
            WorldWrite::NewLayer { template, .. } => {
                game.layer_block_for_edit(template).ok_or_else(|| format!("no layer matching {template:?} in the game stack"))
            }
            WorldWrite::Place { layer, .. } => by_name(game, layer),
            WorldWrite::StandIn(p) => by_name(game, &p.layer),
        }
    }

    /// Whether the write needs the game's layer scaffolding ([`GameStack::layer_scaffold`]).
    pub fn builds_comps(&self) -> bool {
        matches!(self, WorldWrite::Place { .. } | WorldWrite::NewLayer { .. })
    }

    /// `block` with the write applied, and what it did (an `edit_world`'s counts). `scaffold` is
    /// required by an `add_placement` and an `add_layer` ([`Self::builds_comps`]).
    pub fn apply(
        &self,
        block: &[u8],
        scaffold: Option<&mercs2_formats::placement_build::LayerScaffold>,
    ) -> Result<(Vec<u8>, WriteCounts), String> {
        use mercs2_formats::placement_build as pb;
        let need = || scaffold.ok_or_else(|| "the layer scaffolding was not read".to_string());
        // The layer named `name` in `block`: its entry index and container.
        let entry = |name: &str| {
            pb::find_entry(block, crate::manifest::asset_hash(name), TYPE_HASH_LAYER)
                .map(|(i, c)| (i, c.to_vec()))
                .ok_or_else(|| format!("the block holds no layer {name}"))
        };
        match self {
            WorldWrite::Edit { layer, file, doc } => {
                let (edited, moved, reskinned) = edit_world_block(block, doc, layer)?;
                if edited == block {
                    return Err(edit_world_no_change(file, layer));
                }
                Ok((edited, WriteCounts { edits: doc.edits.len(), moved, reskinned }))
            }
            WorldWrite::Place { layer, ents } => {
                let (i, c) = entry(layer)?;
                let edited = insert_entities(&c, ents, need()?).map_err(|m| format!("layer {layer}: {m}"))?;
                Ok((pb::replace_entry(block, i, &edited)?, WriteCounts::default()))
            }
            WorldWrite::NewLayer { name, ents, .. } => {
                let hash = crate::manifest::asset_hash(name);
                if pb::find_entry(block, hash, TYPE_HASH_LAYER).is_some() {
                    return Err(format!("the block already holds a layer named {name}"));
                }
                let carrier = mercs2_formats::ucfx::block_chunk_spans(block)?
                    .into_iter()
                    .find(|(e, _)| e.type_hash == TYPE_HASH_LAYER)
                    .map(|(_, r)| block[r].to_vec())
                    .ok_or("the block holds no layer for the new one to be built beside")?;
                let fresh = pb::empty_layer(&carrier, need()?)?;
                let container = insert_entities(&fresh, ents, need()?).map_err(|m| format!("layer {name}: {m}"))?;
                Ok((pb::append_entry(block, hash, TYPE_HASH_LAYER, &container)?, WriteCounts::default()))
            }
            WorldWrite::StandIn(p) => {
                let (i, c) = entry(&p.layer)?;
                let edited = crate::tiny::layer_with_placement(&c, p.key, p.row, p.col, p.hash).map_err(|m| format!("layer {}: {m}", p.layer))?;
                Ok((pb::replace_entry(block, i, &edited)?, WriteCounts::default()))
            }
        }
    }
}

/// What an `edit_world` write did: its edit count, and the Transform and ModelName records written.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteCounts {
    pub edits: usize,
    pub moved: usize,
    pub reskinned: usize,
}

/// Lower every write of a Shipment's to a layer block (its `edit_world`, `add_placement` and
/// `add_layer` contributions, and `stand_ins`, its stand-ins' placements): the writes to one block
/// applied to it in contribution order, and that block shipped once. Each overlay ships a whole
/// block, so two of one block would leave one write's records out (the registry keeps the first
/// block's rows).
fn lower_world(
    manifest: &crate::manifest::Manifest,
    root: &Path,
    game: &mut GameStack,
    stand_ins: Vec<crate::tiny::StandInPlace>,
    log: &mut Vec<String>,
) -> Result<Vec<PatchBlock>, BuildError> {
    let mut writes: Vec<(usize, &'static str, WorldWrite)> = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        let kind = c.kind();
        if let Some(w) = WorldWrite::of(c, root).map_err(|message| BuildError::Lower { index, kind, message })? {
            writes.push((index, kind, w));
        }
    }
    writes.extend(stand_ins.into_iter().map(|p| (p.index, "add_tiny_geometry", WorldWrite::StandIn(p))));
    writes.sort_by_key(|w| w.0);
    let scaffold = match writes.iter().find(|w| w.2.builds_comps()) {
        Some(&(index, kind, _)) => Some(game.layer_scaffold().map_err(|message| BuildError::Lower { index, kind, message })?),
        None => None,
    };
    // Per block: the stack's block, the block as written so far, whether a layer was appended, and
    // the first write to it (its contribution and kind).
    type BlockWrites = (crate::game::LayerEditInputs, Vec<u8>, bool, (usize, &'static str));
    let mut blocks: Vec<BlockWrites> = Vec::new();
    for (index, kind, w) in &writes {
        let (index, kind) = (*index, *kind);
        let fail = |message: String| BuildError::Lower { index, kind, message };
        let base = w.base(game).map_err(fail)?;
        let at = match blocks.iter().position(|b| b.0.path == base.path) {
            Some(at) => at,
            None => {
                let block = base.block.clone();
                blocks.push((base, block, false, (index, kind)));
                blocks.len() - 1
            }
        };
        let (written, counts) = w.apply(&blocks[at].1, scaffold.as_ref()).map_err(|m| fail(format!("{}: {m}", blocks[at].0.path)))?;
        let what = match w {
            WorldWrite::Edit { layer, .. } => format!(
                "edit_world {layer}: {} edit(s) \u{2192} {} moved, {} re-modelled",
                counts.edits, counts.moved, counts.reskinned
            ),
            WorldWrite::Place { layer, ents } => format!("add_placement {layer}: {} entities inserted into the layer", ents.len()),
            WorldWrite::NewLayer { name, ents, .. } => format!("add_layer {name}: a new layer of {} entities", ents.len()),
            WorldWrite::StandIn(p) => format!("add_tiny_geometry placement 0x{:08X} into {}", p.key, p.layer),
        };
        log.push(format!("contributions[{index}] {what}; layer block {}", blocks[at].0.path));
        blocks[at].1 = written;
        blocks[at].2 |= matches!(w, WorldWrite::NewLayer { .. });
    }
    let mut out = Vec::with_capacity(blocks.len());
    for (base, block, appended, (index, kind)) in blocks {
        let built = if appended { emit_appended_layer(&base, &block) } else { emit_edited_layer(&base, &block) };
        out.push(built.map_err(|message| BuildError::Lower { index, kind, message })?);
    }
    Ok(out)
}

/// The refusal of an `edit_world` whose edits (`file`) change nothing in `layer`.
pub fn edit_world_no_change(file: &Path, layer: &str) -> String {
    format!(
        "{} declares no effective change to {layer:?} — an edit_world that moves nothing \
         would ship an overlay that only restates the base layer",
        file.display()
    )
}

/// `block` (the layer block `layer` names) with `doc`'s edits applied in place
/// (`placement::patch_transform` / `patch_model`), each edit's entity a bare `0xKEY` or a name the
/// block's placements carry, and each container an edit wrote into resealed
/// (`placement::reseal`); and how many Transform and ModelName records the edits wrote.
pub fn edit_world_block(block: &[u8], doc: &crate::world::WorldDoc, layer: &str) -> Result<(Vec<u8>, usize, usize), String> {
    // Name → key, from the layer's own placements, so an author can target by entity name.
    let places = mercs2_formats::placement::load_placements(block).unwrap_or_default();
    let key_by_name: std::collections::HashMap<&str, u32> =
        places.iter().filter_map(|p| p.name.as_deref().map(|n| (n, p.key))).collect();

    let records = mercs2_formats::placement::RecordIndex::read(block);
    let mut edited = block.to_vec();
    let (mut moved, mut reskinned) = (0usize, 0usize);
    let mut written = std::collections::BTreeSet::new();
    for e in &doc.edits {
        // Resolve the target to a Transform record key.
        let key = crate::manifest::bare_hash(&e.entity)
            .or_else(|| key_by_name.get(e.entity.as_str()).copied())
            .ok_or_else(|| {
                format!(
                    "edit_world: entity {:?} is neither a bare 0xKEY nor a name in {layer:?} \
                     — extract a baseline with `qm extract-world` and edit that",
                    e.entity
                )
            })?;
        if e.pos.is_some() || e.quat.is_some() {
            let n = records.patch_transform(&mut edited, key, e.pos, e.quat);
            if n == 0 {
                return Err(format!("edit_world: no Transform for entity {:?} (0x{key:08X}) in {layer:?}", e.entity));
            }
            moved += n;
        }
        if let Some(m) = &e.model {
            let mh = crate::manifest::asset_hash(m);
            let n = records.patch_model(&mut edited, key, mh);
            if n == 0 {
                return Err(format!(
                    "edit_world: entity {:?} (0x{key:08X}) has no ModelName to re-model in {layer:?}",
                    e.entity
                ));
            }
            reskinned += n;
        }
        written.extend(records.offsets(key));
    }
    // Each container an edit wrote into carries a CSUM of its old bytes until it is resealed.
    mercs2_formats::placement::reseal(&mut edited, &written).map_err(|e| format!("edit_world: {layer:?}: {e}"))?;
    Ok((edited, moved, reskinned))
}

/// What an `edit_world`, `add_placement` or `add_layer` builds: the stack's block it is built from,
/// the block as built, and for an `edit_world` its edit count and the Transform and ModelName
/// records they wrote.
#[derive(Debug, Clone)]
pub struct WorldBlock {
    pub base: crate::game::LayerEditInputs,
    pub block: Vec<u8>,
    pub edits: usize,
    pub moved: usize,
    pub reskinned: usize,
}

/// The block contribution `c` (of the Shipment at `root`) builds on its own, as the lowering applies
/// its write ([`WorldWrite`]): `None` for a kind that builds no world block, `Err` with the
/// lowering's refusal. The build applies every write of a Shipment's to one block together
/// ([`lower_world`]).
pub fn world_block(c: &Contribution, root: &Path, game: &mut GameStack) -> Result<Option<WorldBlock>, String> {
    let Some(w) = WorldWrite::of(c, root)? else { return Ok(None) };
    let base = w.base(game)?;
    let scaffold = if w.builds_comps() { Some(game.layer_scaffold()?) } else { None };
    let (block, counts) = w.apply(&base.block, scaffold.as_ref()).map_err(|m| format!("{}: {m}", base.path))?;
    Ok(Some(WorldBlock { base, block, edits: counts.edits, moved: counts.moved, reskinned: counts.reskinned }))
}

/// A layer block with entries appended, as an overlay of `inputs`' block: one ASET row per entry
/// of `new_block`. On WAD merge the base's rows still point at BASE block indices, and the
/// overlay's block gets re-indexed; without rows for the pre-existing sub-blocks, they resolve to a
/// stale base index and M0004 fires.
fn emit_appended_layer(inputs: &crate::game::LayerEditInputs, new_block: &[u8]) -> Result<PatchBlock, String> {
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(new_block);
    let mut asets = Vec::with_capacity(count as usize);
    for e in &entries {
        let tid = mercs2_formats::aset_type_ids::type_id_for_type_hash(e.type_hash).unwrap_or(TYPE_ID_LAYER);
        asets.push(AsetEntry::new(e.name_hash, 0xFFFF_FFFF, 0x0000_FFFF, tid));
    }
    PatchBlock::from_decompressed(new_block, inputs.path.clone(), asets, None)
}

/// Shared helper for the passthrough kinds: read an author-supplied binary file and wrap it as a
/// single-entry mod block carrying one primary ASET row at `pandemic_hash_m2(<name_or_target>)`.
///
/// The manifest side (contribution parse, path sandbox, block emit, ASET wire) is real today;
/// the format side (an encoder that produces the bytes from an author-friendly source description)
/// is orthogonal future work per asset type. A modder with an external encoder can ship any of
/// these kinds NOW.
fn opaque_new_asset(
    root: &Path,
    payload: &Path,
    name_or_target: &str,
    type_hash: u32,
    type_id: u32,
    index: usize,
    kind: &'static str,
) -> Result<Lowering, BuildError> {
    let hash = crate::manifest::asset_hash(name_or_target);
    let bytes = std::fs::read(root.join(payload)).map_err(|e| BuildError::Lower {
        index,
        kind,
        message: format!("reading {}: {e}", root.join(payload).display()),
    })?;
    Ok(Lowering::Block(opaque_container_block(
        hash, type_hash, type_id, &bytes, index, kind,
    )?))
}

/// Wrap an already-produced container as a single-entry mod block whose row names no finer rung.
/// Split from [`opaque_new_asset`] so a codepath that produces the bytes in-Rust rather than reading a
/// file can share the block-emit half. Not for a shipped model, whose rungs `emit_model_chain` carries.
pub(crate) fn opaque_container_block(
    hash: u32,
    type_hash: u32,
    type_id: u32,
    bytes: &[u8],
    index: usize,
    kind: &'static str,
) -> Result<PatchBlock, BuildError> {
    let mut block_data = Vec::new();
    block_data.extend_from_slice(&1u32.to_le_bytes());
    block_data.extend_from_slice(&hash.to_le_bytes());
    block_data.extend_from_slice(&type_hash.to_le_bytes());
    block_data.extend_from_slice(&0u32.to_le_bytes());
    block_data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    block_data.extend_from_slice(bytes);
    let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, type_id);
    PatchBlock::from_decompressed(
        &block_data,
        format!("blocks\\VZ\\mod_{hash:08x}.block"),
        vec![aset],
        None,
    )
    .map_err(|m| BuildError::Lower { index, kind, message: m })
}

/// `add_animation` / `replace_animation`: the retail Havok clip container, from its sources.
///
/// The container is `mercs2_formats::anim_container::build_clip_container` — `info` / `data` /
/// `trnm` / optional `evnt`, packed, with its CSUM — the shape all 4,232 retail clips take and that
/// the writer reproduces byte-for-byte over every one of them. The same pairing check M0213 runs
/// hermetically runs again here, because a build must not depend on somebody having linted first.
///
/// A REPLACE also reads the target out of the game stack: it must exist, and it must be a Havok
/// clip. The 29 retail `animation` assets that are `MANM` keyframe animations cannot be expressed by
/// these sources, and writing a clip over one would change what kind of asset it is.
fn lower_animation(
    root: &Path,
    name_or_target: &str,
    sources: AnimationSources<'_>,
    replace_in: Option<&mut GameStack>,
    index: usize,
    kind: &'static str,
    log: &mut Vec<String>,
) -> Result<Lowering, BuildError> {
    use mercs2_formats::anim_container;
    let err = |m: String| BuildError::Lower { index, kind, message: m };
    let read = |p: &Path| {
        std::fs::read(root.join(p)).map_err(|e| err(format!("reading {}: {e}", p.display())))
    };
    let hash = crate::manifest::asset_hash(name_or_target);
    if let Some(game) = replace_in {
        let existing = game
            .container_for_asset(hash, TYPE_HASH_ANIMATION, TYPE_ID_ANIMATION)
            .ok_or_else(|| {
                err(format!(
                    "{name_or_target:?} (0x{hash:08X}) is not an animation in the game stack — a \
                     replace needs a shipped clip to replace"
                ))
            })?;
        let chunks = anim_container::parse_container(&existing)
            .map_err(|m| err(format!("the shipped {name_or_target:?} does not read: {m}")))?;
        match anim_container::classify(&chunks).map_err(err)? {
            anim_container::AnimContainerKind::HavokClip { .. } => {}
            anim_container::AnimContainerKind::Keyframe => {
                return Err(err(format!(
                    "the shipped {name_or_target:?} (0x{hash:08X}) is a MANM keyframe animation, \
                     not a Havok clip. `clip`/`trnm`/`events` describe a Havok clip, so replacing \
                     it would change the asset's kind; no source this kind takes can express a \
                     MANM animation."
                )));
            }
        }
    }
    let clip_bytes = read(sources.clip)?;
    let trnm_bytes = read(sources.trnm)?;
    let evnt_bytes = sources.events.map(read).transpose()?;
    let container =
        anim_container::build_clip_container(&clip_bytes, &trnm_bytes, evnt_bytes.as_deref())
            .map_err(err)?;
    log.push(format!(
        "contributions[{index}] {kind} {name_or_target} 0x{hash:08X}: clip {} B, trnm {} B, {} \
         → container {} B",
        clip_bytes.len(),
        trnm_bytes.len(),
        match &evnt_bytes {
            Some(e) => format!("evnt {} B", e.len()),
            None => "no evnt".to_string(),
        },
        container.len()
    ));
    Ok(Lowering::Block(opaque_container_block(
        hash,
        TYPE_HASH_ANIMATION,
        TYPE_ID_ANIMATION,
        &container,
        index,
        kind,
    )?))
}

/// The three `src/` files an animation contribution names.
struct AnimationSources<'a> {
    clip: &'a Path,
    trnm: &'a Path,
    events: Option<&'a Path>,
}

/// Every `replace_lua`, ready for the linker to compile + swap in place.
pub fn script_replacements(
    manifest: &crate::manifest::Manifest,
    root: &Path,
) -> Result<Vec<link::ScriptReplacement>, BuildError> {
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        if let Contribution::ReplaceLua { target, source } = c {
            let path = root.join(source);
            let src = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
                index,
                kind: "replace_lua",
                message: format!("reading {}: {e}", path.display()),
            })?;
            out.push(link::ScriptReplacement {
                shipment: shipment.clone(),
                target: target.clone(),
                source: src,
            });
        }
    }
    Ok(out)
}

/// Every `add_outfit` of a Shipment, in manifest order, as the outfit generator takes it
/// ([`crate::outfits::AddedOutfit`]): `name` is the model the wardrobe row puts on, `slug` its `Name`.
/// Hermetic, like [`script_mutations`].
pub fn added_outfits(manifest: &crate::manifest::Manifest) -> Vec<crate::outfits::AddedOutfit> {
    manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::AddOutfit { name, slug, display, wearer, chatter, levels, briefing_safe, .. } => {
                Some(crate::outfits::AddedOutfit {
                    shipment: manifest.shipment.name.clone(),
                    slug: slug.clone(),
                    model: name.clone(),
                    display: display.clone(),
                    wearers: wearer.heroes().into_iter().map(str::to_string).collect(),
                    levels: levels.clone(),
                    chatter: chatter.clone(),
                    briefing_safe: *briefing_safe,
                })
            }
            _ => None,
        })
        .collect()
}

/// Every `add_script`, ready for the linker to compile + mint as a fresh scripts_vz entry.
///
/// Reads each source file up front, so a missing `.lua` is a build-time (deploy-time) error with
/// the shipment name and the offending path, not a mysterious silent skip at link time.
pub fn script_additions(
    manifest: &crate::manifest::Manifest,
    root: &Path,
) -> Result<Vec<link::ScriptAddition>, BuildError> {
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        if let Contribution::AddScript { name, source, load, .. } = c {
            if let Some(l) = load {
                let refusals = crate::lint::library_load_refusals(name, l);
                if !refusals.is_empty() {
                    return Err(BuildError::Lower { index, kind: "add_script", message: refusals.join(" ") });
                }
            }
            let path = root.join(source);
            let src = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
                index,
                kind: "add_script",
                message: format!("reading {}: {e}", path.display()),
            })?;
            out.push(link::ScriptAddition {
                shipment: shipment.clone(),
                name: name.clone(),
                source: src,
                load: load.clone(),
            });
        }
    }
    Ok(out)
}

/// Every `add_ui`'s FlashWidget registration, for the linker to bake into `qm_modloader`.
///
/// Kept separate from [`script_mutations`] because a UI mod does NOT append to a base script — its
/// registration lives in the Quartermaster-owned load space, which qm's hooks load and run. Returning
/// an empty vec (no `add_ui`) means no loader is minted at all.
pub fn ui_registrations(manifest: &crate::manifest::Manifest) -> Vec<link::UiRegistration> {
    let shipment = manifest.shipment.name.clone();
    manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::AddUi { name, .. } => Some(link::UiRegistration {
                shipment: shipment.clone(),
                // `SetSwfFile` hashes this name to find the cfx_pack, so it must be the SAME `name`
                // the movie block is registered under (see the AddMovie/AddUi arm in `lower`, which
                // mints the pack at `asset_hash(name)`), not the source file's stem.
                movie: name.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Every `activate_layer`'s `MrxLayerManager` marks, for the linker to bake into `qm_modloader`.
///
/// Kept separate from [`script_mutations`] for the same reason as [`ui_registrations`]: it does not
/// append to a base script but lives in the Quartermaster-owned load space, which qm's hooks load and
/// run. A UI mod and a layer mod both feed this loader, so either alone is enough to mint it.
pub fn layer_registrations(manifest: &crate::manifest::Manifest) -> Vec<link::LayerRegistration> {
    let shipment = manifest.shipment.name.clone();
    manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::ActivateLayer { layer, replaces } => Some(link::LayerRegistration {
                shipment: shipment.clone(),
                add: layer.clone(),
                remove: replaces.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Every NOVEL-behaviour shop item's registration, for the linker to mint the `MrxSupport` subclass
/// and bake its DEFERRED catalog row into `qm_modloader`. A shop item is novel only when its
/// `behaviour.script` is set; a resident-module item stays on the load-time append path in
/// [`script_mutations`]. Support catalog only — equipment carries no behaviour. Every level mints the
/// module, so every level loads the same modules; the catalog row goes into the levels the item lists
/// ([`link::SupportRegistration::levels`]).
pub fn support_registrations(
    manifest: &crate::manifest::Manifest,
    root: &Path,
) -> Result<Vec<link::SupportRegistration>, BuildError> {
    use crate::manifest::ShopCatalog;
    let shipment = manifest.shipment.name.clone();
    let mut out = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        let Contribution::AddShopItem {
            id,
            name,
            description,
            icon,
            shops,
            catalog,
            item_type,
            cash_cost,
            fuel_cost,
            max_stock,
            unlocked,
            behaviour,
            levels,
            ..
        } = c
        else {
            continue;
        };
        let Some(b) = behaviour else { continue };
        let Some(script) = &b.script else { continue };
        if !matches!(catalog, ShopCatalog::Support) {
            return Err(BuildError::Lower {
                index,
                kind: "add_shop_item",
                message: format!("shop item {id:?}: behaviour.script is support-catalog only"),
            });
        }
        let path = root.join(script);
        let source = std::fs::read_to_string(&path).map_err(|e| BuildError::Lower {
            index,
            kind: "add_shop_item",
            message: format!("reading behaviour.script {}: {e}", path.display()),
        })?;
        out.push(link::SupportRegistration {
            shipment: shipment.clone(),
            module: b.module.clone(),
            source,
            id: id.clone(),
            name: name.clone(),
            description: description.clone(),
            icon: icon.clone(),
            item_type: item_type.as_ref().map_or("Supply", |t| t.lua()).to_string(),
            cash_cost: *cash_cost,
            fuel_cost: *fuel_cost,
            max_stock: *max_stock,
            unlock_table: link::shop_unlock_table(shops, *unlocked),
            cargo: b.cargo.clone(),
            delivery_vehicle: b.delivery_vehicle.clone(),
            shops: shops.iter().map(|v| v.faction_key().to_string()).collect(),
            levels: levels.clone(),
        });
    }
    Ok(out)
}

/// Mint a Scaleform movie as a `cfx_pack` patch block under `name`, from the file at `root/movie`.
///
/// Shared by `add_movie` and `add_ui` so the two ship byte-identical Data — the only difference is
/// that `add_ui` also bakes a FlashWidget registration into `qm_modloader`. Parses before wrapping
/// (a non-movie leaf still checksums and resolves — the loader is the first to notice, and it names
/// nothing), then emits the pack with its ADDITIVE, PRIMARY cfx_pack ASET row.
fn lower_movie(
    root: &Path,
    name: &str,
    movie: &Path,
    index: usize,
    kind: &'static str,
    log: &mut Vec<String>,
) -> Result<PatchBlock, BuildError> {
    let path = root.join(movie);
    let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
        index,
        kind,
        message: format!("reading {}: {e}", path.display()),
    })?;

    let parsed = mercs2_formats::gfx_stream::TagStream::parse(&bytes).map_err(|m| BuildError::Lower {
        index,
        kind,
        message: format!(
            "{} is not a Scaleform movie this build can read: {m}. Expected a `.gfx` beginning \
             with `GFX` or `CFX` (or the SWF spellings `FWS`/`CWS`) — retail ships 61 `CFX` and 3 \
             `GFX`, so either is fine, but a project file, an already-wrapped container or a \
             truncated export is not.",
            path.display()
        ),
    })?;

    let hash = crate::manifest::asset_hash(name);
    let block_bytes = mercs2_formats::gfx::build_cfx_pack_block(hash, &bytes);

    // The tag census is logged rather than merely counted: an emitter that silently dropped the
    // movie's content still produces a valid header, and "0 tags" in the log is the only place that
    // would show.
    let features = mercs2_formats::gfx::features(&parsed);
    let [w, h] = parsed.stage_px().map_err(|m| BuildError::Lower { index, kind, message: format!("{}: {m}", path.display()) })?;
    log.push(format!(
        "contributions[{index}] {kind} {name} 0x{hash:08X} ← {} \
         {} v{} {}x{} px, {} tag(s): {} shape(s), {} sprite(s), {} button(s), \
         {} edit-text, {} DoAction, {} import(s), {} GFx-ext → {} bytes",
        path.display(),
        String::from_utf8_lossy(&parsed.magic),
        parsed.version,
        w,
        h,
        parsed.tags.len(),
        features.shapes,
        features.sprites,
        features.buttons,
        features.edit_texts,
        features.do_action,
        features.imports,
        features.gfx_ext_tags,
        block_bytes.len()
    ));

    movie_block(hash, &block_bytes).map_err(|m| BuildError::Lower {
        index,
        kind,
        message: m,
    })
}

/// The block a movie named by `hash` ships in.
pub fn movie_block_path(hash: u32) -> String {
    format!("blocks\\VZ\\mod_{hash:08x}.block")
}

/// The patch block of the `cfx_pack` container `block_bytes` of the movie named by `hash`, with its row:
/// ADDITIVE and PRIMARY. A movie has no LOD chain at all, so both rung halves stay at their sentinels —
/// `0x0000` in the low 16 is the dangling-rung HANG, not "no rung".
fn movie_block(hash: u32, block_bytes: &[u8]) -> Result<PatchBlock, String> {
    let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_CFX_PACK);
    PatchBlock::from_decompressed(block_bytes, movie_block_path(hash), vec![aset], None)
}

/// Encode one authored image into a `TYPE_ID_TEXTURE` patch block under `asset_name`.
///
/// Returns `(block, to_hash)`. For a skin, the caller pairs `to_hash` with each donor hash the model
/// names at that slot to build the [`MtrlRepoint`]s; for a standalone `add_texture` the hash IS the
/// asset and nothing is repointed.
///
/// The name is a PARAMETER rather than being derived as `<outfit>_<slot>` here, which is what
/// previously made a novel texture inexpressible on its own: the encoder already did every hard
/// part (swizzle, format choice, full resident mip chain, primary ASET row) and simply had no way
/// to be told what to call its output.
///
/// Format policy, from `character_kit.md`: a NORMAL map is BC3/DXT5nm — this project's swizzle is
/// `R=1, G=ny, B=1, A=nx` (matching `mercs2_workshop::texenc::encode_normal_full_chain`, so preview
/// and shipped agree). Diffuse and specular are BC1 unless the source carries real alpha, which
/// needs BC3. Textures are fully resident: the whole mip chain ships, because a short BODY makes the
/// engine over-read and the world-load livelocks.
fn build_named_texture(
    index: usize,
    kind: &'static str,
    asset_name: &str,
    path: &Path,
    is_normal: bool,
) -> Result<(PatchBlock, u32), BuildError> {
    let err = |m: String| BuildError::Lower { index, kind, message: m };
    let img = read_png_rgba(path).map_err(err)?;
    build_texture_from_rgba(index, kind, asset_name, img, is_normal, None, &path.display().to_string())
}

/// Build a texture block from a GLB's embedded PNG bytes — the per-material skin path. `brighten`
/// gamma-lifts a too-dark imported diffuse; `label` names the material for any error.
fn build_named_texture_bytes(
    index: usize,
    kind: &'static str,
    asset_name: &str,
    png_bytes: &[u8],
    is_normal: bool,
    brighten: Option<f32>,
    label: &str,
) -> Result<(PatchBlock, u32), BuildError> {
    let err = |m: String| BuildError::Lower { index, kind, message: m };
    let img = read_png_rgba_bytes(png_bytes, label).map_err(err)?;
    build_texture_from_rgba(index, kind, asset_name, img, is_normal, brighten, label)
}

/// The encoder half: already-decoded RGBA → a resident texture block + its asset hash. Shared by the
/// file-path and embedded-bytes callers.
fn build_texture_from_rgba(
    index: usize,
    kind: &'static str,
    asset_name: &str,
    img: Rgba,
    is_normal: bool,
    // Gamma < 1 brightens the RGB (alpha untouched). Imported diffuse maps are often authored much
    // darker than the game's dim interior lighting can show; a lift brings them into the range retail
    // albedos sit in. `None` for a hand-authored `textures:` map, which is presumed already correct.
    brighten: Option<f32>,
    label: &str,
) -> Result<(PatchBlock, u32), BuildError> {
    let err = |m: String| BuildError::Lower {
        index,
        kind,
        message: m,
    };
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || w % 4 != 0 || h % 4 != 0 {
        return Err(err(format!(
            "{label}: {w}x{h} — a block-compressed texture needs both dimensions to be multiples of 4"
        )));
    }

    // Swizzle first, then decide the format from what the pixels actually contain.
    let pixels: Vec<f32> = if is_normal {
        img.pixels
            .chunks_exact(4)
            .flat_map(|p| [255.0, p[1], 255.0, p[0]])
            .collect()
    } else if let Some(g) = brighten {
        let lift = |v: f32| ((v / 255.0).powf(g) * 255.0).clamp(0.0, 255.0);
        img.pixels
            .chunks_exact(4)
            .flat_map(|p| [lift(p[0]), lift(p[1]), lift(p[2]), p[3]])
            .collect()
    } else {
        img.pixels.clone()
    };
    let has_alpha = !is_normal && pixels.chunks_exact(4).any(|p| p[3] < 254.0);
    let format = if is_normal || has_alpha {
        TexFormat::Bc3
    } else {
        TexFormat::Bc1
    };
    let body = match format {
        TexFormat::Bc1 => {
            let rgb = drop_alpha(&pixels);
            texture_encode::mip_chain(w, h, 3, &rgb, texture_encode::encode_bc1)
        }
        TexFormat::Bc3 => texture_encode::mip_chain(w, h, 4, &pixels, texture_encode::encode_bc3),
    };

    resident_texture_block(asset_name, w, h, format, &body).map_err(|e| err(format!("{label}: {e}")))
}

/// Texture `asset_name`, `w` by `h`, of mip chain `body` in `format`, as a patch block of its own with
/// its ASET row, and the asset's hash.
pub fn resident_texture_block(asset_name: &str, w: usize, h: usize, format: TexFormat, body: &[u8]) -> Result<(PatchBlock, u32), String> {
    let to = crate::manifest::asset_hash(asset_name);
    // Ship the FULLY-RESIDENT container shape the engine's texture binder actually reads —
    // NAME / INFO / BODY with the `0xFFFF` sentinel at INFO@32 and INFO[26..32]=0. The old
    // `build_texture_block` emitted only INFO/BODY (no NAME, no sentinel), so its texture LOADED but
    // never BOUND: the injected model sampled it black. This is the obama skin recipe — ship a texture
    // block shaped like a real resident texture. `build_resident_texture` enforces the exact mip-chain
    // length too (a short body hangs the world load), so it validates `body` on the way through.
    let container = mercs2_formats::texture::build_resident_texture(asset_name, w as u32, h as u32, format, body)?;
    let mut block_bytes = Vec::with_capacity(20 + container.len());
    block_bytes.extend_from_slice(&1u32.to_le_bytes()); // flags/version
    block_bytes.extend_from_slice(&to.to_le_bytes()); // asset name hash
    block_bytes.extend_from_slice(&mercs2_formats::types::TYPE_HASH_TEXTURE.to_le_bytes());
    block_bytes.extend_from_slice(&0u32.to_le_bytes());
    block_bytes.extend_from_slice(&(container.len() as u32).to_le_bytes());
    block_bytes.extend_from_slice(&container);
    let aset = AsetEntry::new(to, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_TEXTURE);
    let block = PatchBlock::from_decompressed(
        &block_bytes,
        format!("blocks\\VZ\\mod_{to:08x}.block"),
        vec![aset],
        None,
    )?;
    Ok((block, to))
}

/// Bilinear-resample an [`Rgba`] to `(w, h)`. Straight (non-premultiplied) RGBA; a clone when the
/// dimensions already match. The build has no `image` crate, so the one resampler it needs for the
/// single-group atlas lives here.
fn resize_rgba(src: &Rgba, w: usize, h: usize) -> Rgba {
    if src.width == w && src.height == h {
        return Rgba { width: w, height: h, pixels: src.pixels.clone() };
    }
    let (sw, sh) = (src.width.max(1), src.height.max(1));
    let mut px = vec![0.0f32; w * h * 4];
    let sx = sw as f32 / w as f32;
    let sy = sh as f32 / h as f32;
    let at = |xx: usize, yy: usize, c: usize| src.pixels[(yy * sw + xx) * 4 + c];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let ty = fy - y0 as f32;
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let tx = fx - x0 as f32;
            for c in 0..4 {
                let top = at(x0, y0, c) * (1.0 - tx) + at(x1, y0, c) * tx;
                let bot = at(x0, y1, c) * (1.0 - tx) + at(x1, y1, c) * tx;
                px[(y * w + x) * 4 + c] = top * (1.0 - ty) + bot * ty;
            }
        }
    }
    Rgba { width: w, height: h, pixels: px }
}

/// Bake the used materials' diffuse maps into ONE atlas texture, returning the atlas image and each
/// material's cell as `[u0, v0, su, sv]` (fractions of the atlas). This is what lets `single_group`
/// wear a MULTI-material import's whole correct skin from one draw group / one material: the caller
/// remaps each part's UVs into its cell (see `char_lower::LowerOpts::atlas_cells`).
///
/// A shelf packer into a fixed 2048-wide atlas, tallest-first, each cell capped so the pack fits in
/// 2048 tall (dropping the cap 1024→512→… on overflow). Deterministic given the same inputs, which
/// keeps the build reproducible.
fn bake_diffuse_atlas(mats: &[(usize, Rgba)]) -> (Rgba, std::collections::HashMap<usize, [f32; 4]>) {
    use std::collections::HashMap;
    const WIDTH: usize = 2048;
    const MAXH: usize = 2048;
    for &cap in &[1024usize, 512, 256, 128, 64] {
        // Cell size = native, capped to `cap` and to the atlas width.
        let mut cells: Vec<(usize, usize, usize)> = mats
            .iter()
            .map(|(m, img)| (*m, img.width.min(cap).min(WIDTH).max(4), img.height.min(cap).max(4)))
            .collect();
        cells.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        let mut placed: HashMap<usize, (usize, usize, usize, usize)> = HashMap::new();
        let (mut x, mut y, mut row_h, mut used_h, mut ok) = (0usize, 0usize, 0usize, 0usize, true);
        for &(m, cw, ch) in &cells {
            if x + cw > WIDTH {
                x = 0;
                y += row_h;
                row_h = 0;
            }
            if y + ch > MAXH {
                ok = false;
                break;
            }
            placed.insert(m, (x, y, cw, ch));
            x += cw;
            row_h = row_h.max(ch);
            used_h = used_h.max(y + ch);
        }
        if !ok {
            continue;
        }
        let atlas_h = ((used_h + 3) / 4 * 4).max(4);
        let mut atlas = Rgba { width: WIDTH, height: atlas_h, pixels: vec![0.0; WIDTH * atlas_h * 4] };
        for i in 0..WIDTH * atlas_h {
            atlas.pixels[i * 4 + 3] = 255.0; // opaque
        }
        let mut cellmap = HashMap::new();
        for (m, img) in mats {
            let (px, py, cw, ch) = placed[m];
            let r = resize_rgba(img, cw, ch);
            for ry in 0..ch {
                for rx in 0..cw {
                    let s = (ry * cw + rx) * 4;
                    let dst = ((py + ry) * WIDTH + (px + rx)) * 4;
                    atlas.pixels[dst..dst + 4].copy_from_slice(&r.pixels[s..s + 4]);
                }
            }
            cellmap.insert(
                *m,
                [
                    px as f32 / WIDTH as f32,
                    py as f32 / atlas_h as f32,
                    cw as f32 / WIDTH as f32,
                    ch as f32 / atlas_h as f32,
                ],
            );
        }
        return (atlas, cellmap);
    }
    // Unreachable for any real input (cap 64 fits thousands of cells); a valid 4x4 keeps it total.
    let mut px = vec![0.0f32; 64];
    for i in 0..16 {
        px[i * 4 + 3] = 255.0;
    }
    (Rgba { width: 4, height: 4, pixels: px }, std::collections::HashMap::new())
}

/// Regenerate a rigid prop's static collision from its OWN injected mesh and splice it into the
/// model block, replacing the donor's `PHY2`. Backs `add_model collision: follow_geometry`.
///
/// `new_block` is the block returned by [`inject_static_into_donor_block`]
/// (`[20-byte block header][UCFX container incl. CSUM]`); `mesh` is the imported glTF geometry in
/// MODEL-LOCAL space (the rigid path injects it with `fit_to_template=false`, so its verts are the
/// same frame the render mesh occupies and the frame the engine queries collision in).
///
/// Only the `PHY2` chunk changes: [`build_phy2_multi`] emits one whole-mesh `WpMeshShape16`+MOPP
/// shape (N=1 is byte-identical to the proven single-mesh path), and the donor's 48-byte PHY2 prefix
/// (asset-hash + framing) is preserved except byte-32 (the authored packfile size). `SEGM`, `INDX`,
/// `HIER` and every render chunk stay byte-for-byte identical — collision is not `SEGM`-bound.
/// Partition a mesh into `MeshSoup` chunks of at most `max_tris` triangles each, so every chunk's
/// authored MOPP fits the encoder's 16-bit child-jump budget. Triangles are sorted along the longest
/// bbox axis first, so each chunk is spatially compact (tight AABB → real broadphase pruning). Every
/// chunk re-indexes only the vertices it uses. One chunk (small mesh) returns the whole mesh, so the
/// N=1 path is unchanged.
fn split_mesh_for_mopp(
    tris: &[[u32; 3]],
    positions: &[[f32; 3]],
    max_tris: usize,
) -> Vec<mercs2_formats::phy2_build::MeshSoup> {
    use std::collections::HashMap;
    let n = tris.len();
    if n == 0 {
        return vec![(Vec::new(), Vec::new())];
    }
    let n_chunks = n.div_ceil(max_tris.max(1)).max(1);
    // Longest bbox axis, from triangle centroids.
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    let cen: Vec<[f32; 3]> = tris
        .iter()
        .map(|t| {
            let (a, b, c) = (
                positions[t[0] as usize],
                positions[t[1] as usize],
                positions[t[2] as usize],
            );
            let ct = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            for k in 0..3 {
                lo[k] = lo[k].min(ct[k]);
                hi[k] = hi[k].max(ct[k]);
            }
            ct
        })
        .collect();
    let axis = (0..3)
        .max_by(|&a, &b| (hi[a] - lo[a]).partial_cmp(&(hi[b] - lo[b])).unwrap())
        .unwrap();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| cen[a][axis].partial_cmp(&cen[b][axis]).unwrap());

    let chunk_len = n.div_ceil(n_chunks);
    let mut soups = Vec::with_capacity(n_chunks);
    for chunk in order.chunks(chunk_len) {
        let mut vmap: HashMap<u32, u32> = HashMap::new();
        let mut verts: Vec<[f32; 3]> = Vec::new();
        let mut ctris: Vec<[u32; 3]> = Vec::with_capacity(chunk.len());
        for &ti in chunk {
            let t = tris[ti];
            let mut nt = [0u32; 3];
            for k in 0..3 {
                nt[k] = *vmap.entry(t[k]).or_insert_with(|| {
                    verts.push(positions[t[k] as usize]);
                    (verts.len() - 1) as u32
                });
            }
            ctris.push(nt);
        }
        soups.push((ctris, verts));
    }
    soups
}

fn regenerate_prop_collision(
    new_block: &[u8],
    mesh: &mercs2_formats::model_inject::ExternalMesh,
    name: &str,
) -> Result<Vec<u8>, String> {
    use mercs2_formats::phy2_container::{phy2_span_in_container, replace_phy2_in_container};

    if mesh.tris.is_empty() || mesh.positions.is_empty() {
        return Err("injected mesh has no triangles to build collision from".into());
    }
    if new_block.len() < 20 {
        return Err("model block too small".into());
    }
    let ucfx_len = u32::from_le_bytes(new_block[16..20].try_into().unwrap()) as usize;
    let container = new_block
        .get(20..20 + ucfx_len)
        .ok_or("model block truncated before end of UCFX container")?;

    let (pstart, psize) = phy2_span_in_container(container)
        .ok_or("donor container has no PHY2 collision chunk to replace (follow_geometry needs one)")?;
    let base_body = &container[pstart..pstart + psize];
    if base_body.len() < 48 {
        return Err("donor PHY2 body shorter than the 48-byte prefix".into());
    }

    // Build a fresh whole-model PHY2 from the injected geometry as N `WpMeshShape16` shapes sharing
    // one common frame + vertex pool (the proven shared-pool multi-shape path). A single-shape MOPP's
    // child-jump offsets are only 16-bit (`mopp::encode`'s `0x23` split), so a tree over more than
    // ~11 k triangles overflows and only part of it decodes. Splitting into ≤ `MAX_MOPP_TRIS`-triangle
    // shapes keeps every shape's MOPP inside the 16-bit budget while collision still follows the FULL
    // mesh. Shapes bind via the packfile `WpArray`, not `SEGM`, so any shape count is legal on any
    // donor (census-proven). N=1 (a small mesh) stays byte-identical to the proven single-mesh path.
    const MAX_MOPP_TRIS: usize = 8000;
    let soups = split_mesh_for_mopp(&mesh.tris, &mesh.positions, MAX_MOPP_TRIS);
    let authored = mercs2_formats::phy2_build::build_phy2_multi(name, &soups)?;
    if authored.len() < 48 {
        return Err("authored PHY2 body shorter than the 48-byte prefix".into());
    }

    // Preserve the donor's 48-byte PHY2 prefix verbatim except byte-32 (the authored packfile size).
    let mut new_body = Vec::with_capacity(authored.len());
    new_body.extend_from_slice(&base_body[0..48]);
    new_body[32..36].copy_from_slice(&authored[32..36]);
    new_body.extend_from_slice(&authored[48..]);

    let new_container = replace_phy2_in_container(container, pstart, psize, &new_body)?;

    // Rewrap the block header, updating the UCFX length field (offset 16).
    let mut out = new_block[0..20].to_vec();
    out[16..20].copy_from_slice(&(new_container.len() as u32).to_le_bytes());
    out.extend_from_slice(&new_container);
    Ok(out)
}

/// A rigid render group indexes vertices with u16 and draws ONE triangle strip, so a dense mesh
/// whose strip would exceed 65 534 indices cannot be injected as its render geometry. This
/// cluster-decimates a RENDER copy just enough to fit. Collision (`follow_geometry`) is regenerated
/// from the FULL mesh separately, so the visible LOD drops while the collider stays geometry-tight.
///
/// Returns the mesh unchanged when its strip already fits (the common case), else the finest
/// decimation that fits — plus the (verts, tris) it landed on, for the build log. The glTF's
/// custom vertex attributes come back for the returned mesh's vertices: a decimated vertex takes
/// the mean `_SWAY_WEIGHT` of the vertices it merges (its position is their mean), and only
/// vertices sharing a `_TINY_SLOT` merge.
fn fit_render_mesh_to_u16_strip(
    mesh: &mercs2_formats::model_inject::ExternalMesh,
    custom: &mercs2_formats::mesh_import::CustomAttributes,
) -> (
    mercs2_formats::model_inject::ExternalMesh,
    mercs2_formats::mesh_import::CustomAttributes,
    Option<(usize, usize)>,
) {
    use mercs2_formats::model_inject::to_strip_connected;
    const U16_STRIP_MAX: usize = 65534;
    if to_strip_connected(&mesh.tris).len() <= U16_STRIP_MAX {
        return (mesh.clone(), custom.clone(), None);
    }
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &mesh.positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
    let mut cell_lo = diag / 8192.0; // fine   -> many verts
    let mut cell_hi = diag / 2.0; // coarse -> few verts
    let mut best = cluster_decimate_ext(mesh, cell_hi, custom);
    // Smallest cell (most detail) whose connected strip still fits.
    for _ in 0..48 {
        let mid = (cell_lo * cell_hi).sqrt();
        let d = cluster_decimate_ext(mesh, mid, custom);
        if to_strip_connected(&d.0.tris).len() <= U16_STRIP_MAX {
            best = d;
            cell_hi = mid;
        } else {
            cell_lo = mid;
        }
        if cell_hi / cell_lo < 1.02 {
            break;
        }
    }
    let n = (best.0.positions.len(), best.0.tris.len());
    (best.0, best.1, Some(n))
}

/// Vertex-cluster decimation on an [`mercs2_formats::model_inject::ExternalMesh`]: quantise positions
/// to a `cell` grid, collapse each occupied cell to its centroid, drop degenerate / zero-area tris,
/// recompute area-weighted normals. UVs are zeroed (this render path serves an untextured prop) and
/// skin arrays dropped (rigid). Mirrors `mesh_prep::cluster_decimate`.
fn cluster_decimate_ext(
    m: &mercs2_formats::model_inject::ExternalMesh,
    cell: f32,
    custom: &mercs2_formats::mesh_import::CustomAttributes,
) -> (mercs2_formats::model_inject::ExternalMesh, mercs2_formats::mesh_import::CustomAttributes) {
    use std::collections::HashMap;
    let inv = 1.0 / cell;
    // A `_TINY_SLOT` names the world object a vertex belongs to, so vertices of different slots
    // never merge.
    let key = |v: usize, p: &[f32; 3]| {
        (
            (p[0] * inv).floor() as i64,
            (p[1] * inv).floor() as i64,
            (p[2] * inv).floor() as i64,
            custom.tiny_slot.as_ref().map(|s| s[v]),
        )
    };
    let mut cells: HashMap<(i64, i64, i64, Option<u32>), u32> = HashMap::new();
    let mut sum: Vec<[f64; 3]> = Vec::new();
    let mut cnt: Vec<u32> = Vec::new();
    let mut remap: Vec<u32> = Vec::with_capacity(m.positions.len());
    for (v, p) in m.positions.iter().enumerate() {
        let k = key(v, p);
        let idx = *cells.entry(k).or_insert_with(|| {
            sum.push([0.0; 3]);
            cnt.push(0);
            (sum.len() - 1) as u32
        });
        let i = idx as usize;
        sum[i] = [
            sum[i][0] + p[0] as f64,
            sum[i][1] + p[1] as f64,
            sum[i][2] + p[2] as f64,
        ];
        cnt[i] += 1;
        remap.push(idx);
    }
    let positions: Vec<[f32; 3]> = sum
        .iter()
        .zip(&cnt)
        .map(|(s, &c)| {
            let c = c.max(1) as f64;
            [(s[0] / c) as f32, (s[1] / c) as f32, (s[2] / c) as f32]
        })
        .collect();
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(m.tris.len());
    for t in &m.tris {
        let (a, b, c) = (
            remap[t[0] as usize],
            remap[t[1] as usize],
            remap[t[2] as usize],
        );
        if a == b || b == c || a == c {
            continue;
        }
        let n = cross(
            sub(positions[b as usize], positions[a as usize]),
            sub(positions[c as usize], positions[a as usize]),
        );
        if n[0] * n[0] + n[1] * n[1] + n[2] * n[2] <= 1e-20 {
            continue;
        }
        tris.push([a, b, c]);
    }
    let mut normals = vec![[0.0f32; 3]; positions.len()];
    for t in &tris {
        let (ia, ib, ic) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let n = cross(sub(positions[ib], positions[ia]), sub(positions[ic], positions[ia]));
        for &i in &[ia, ib, ic] {
            for k in 0..3 {
                normals[i][k] += n[k];
            }
        }
    }
    for nrm in normals.iter_mut() {
        let l = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        if l > 1e-12 {
            nrm[0] /= l;
            nrm[1] /= l;
            nrm[2] /= l;
        } else {
            *nrm = [0.0, 1.0, 0.0];
        }
    }
    let uvs = vec![[0.0f32; 2]; positions.len()];
    let sway = custom.sway.as_ref().map(|w| {
        let mut total = vec![0.0f64; positions.len()];
        for (v, &cluster) in remap.iter().enumerate() {
            total[cluster as usize] += w[v] as f64;
        }
        total.iter().zip(&cnt).map(|(t, &c)| (t / c.max(1) as f64) as f32).collect()
    });
    let tiny_slot = custom.tiny_slot.as_ref().map(|s| {
        let mut out = vec![0u32; positions.len()];
        for (v, &cluster) in remap.iter().enumerate() {
            out[cluster as usize] = s[v];
        }
        out
    });
    let mesh = mercs2_formats::model_inject::ExternalMesh {
        positions,
        normals,
        uvs,
        tris,
        joints: Vec::new(),
        weights: Vec::new(),
    };
    (mesh, mercs2_formats::mesh_import::CustomAttributes { sway, tiny_slot })
}

/// Lower a rigged `.glb` onto a donor — the SKINNED path, shared by `add_model` and `add_outfit`.
///
/// This used to be a flat refusal (`BuildError::Unsupported`), because everything it needs lived in
/// binary crates: the skinned glTF reader in the Workshop, a second copy in `mercs2_poc`, and the
/// bone mapper alongside them. All three are library code now, so a Shipment can finally ship a
/// character instead of being told the format supports one in principle.
/// The donor's UCFX container: a donor block is `[count][16-byte entry][container]`, and the entry's
/// last word is the container's length.
fn donor_container(donor_blk: &[u8]) -> &[u8] {
    let n = u32::from_le_bytes(donor_blk[16..20].try_into().unwrap_or([0; 4])) as usize;
    donor_blk.get(20..20 + n).unwrap_or(&[])
}

/// A model's own maps, as the manifest names them: the model's name (the texture assets are
/// `<name>_dm` / `_sm` / `_nm`), its `textures:` block, and the Shipment root those paths resolve in.
struct ModelSkin<'a> {
    name: &'a str,
    textures: &'a crate::manifest::Textures,
    root: &'a Path,
}

/// An author's `textures:` maps → one resident texture block each, plus the MTRL repoints that bind
/// them. Shared by the skinned and rigid model lowerings; they differ only in WHICH materials a map
/// replaces, which `froms(slot)` answers with the donor hashes currently at that MTRL slot (slot
/// order `0 = diffuse, 1 = specular, 2 = normal`). `scope` names that set for the error a map with
/// nothing to replace raises.
fn author_texture_repoints(
    index: usize,
    kind: &'static str,
    skin: ModelSkin<'_>,
    scope: &str,
    froms: impl Fn(usize) -> Vec<u32>,
    log: &mut Vec<String>,
) -> Result<(Vec<PatchBlock>, Vec<mercs2_formats::model_inject::MtrlRepoint>), BuildError> {
    let name = skin.name;
    let mut tex_blocks: Vec<PatchBlock> = Vec::new();
    let mut repoints: Vec<mercs2_formats::model_inject::MtrlRepoint> = Vec::new();
    for (slot, suffix, src, is_normal) in [
        (0usize, "dm", &skin.textures.diffuse, false),
        (1, "sm", &skin.textures.specular, false),
        (2, "nm", &skin.textures.normal, true),
    ] {
        let Some(rel) = src else { continue };
        // The historical naming for a model's own maps, spelled at the call site.
        let (block, to) = build_named_texture(
            index,
            kind,
            &format!("{name}_{suffix}"),
            &skin.root.join(rel),
            is_normal,
        )?;
        let slot_froms = froms(slot);
        if slot_froms.is_empty() {
            // Nothing to bind it to. Shipping the texture anyway would look like it worked.
            return Err(BuildError::Lower {
                index,
                kind,
                message: format!(
                    "`textures.{}` was supplied, but the donor names no texture at MTRL slot \
                     {slot} ({suffix}) for {scope} — there is nothing to repoint onto it.",
                    match slot {
                        0 => "diffuse",
                        1 => "specular",
                        _ => "normal",
                    },
                ),
            });
        }
        log.push(format!(
            "contributions[{index}] {kind} {name}: {name}_{suffix} 0x{to:08X} replaces {} donor \
             hash(es) at slot {slot}",
            slot_froms.len()
        ));
        for from in slot_froms {
            repoints.push(mercs2_formats::model_inject::MtrlRepoint { from, to });
        }
        tex_blocks.push(block);
    }
    Ok((tex_blocks, repoints))
}

/// The `MTRL` flags bit a rigid material must carry to sample a texture. Records without it (flags
/// `0x0000`) are flat-shaded and ignore whatever is bound (`docs/modding/field_guide.md`, the MTRL
/// record-count trap; the Workshop's publish path hosts only on such textured groups).
pub const MTRL_TEXTURED: u16 = 0x0080;

/// The rigid path's `textures:` → texture blocks + repoints, over the HOST group's materials.
///
/// Empty `textures:` returns nothing to ship: the prop wears the donor's materials.
fn rigid_texture_repoints(
    index: usize,
    kind: &'static str,
    skin: ModelSkin<'_>,
    donor_ucfx: &[u8],
    donor_name: &str,
    host_group: usize,
    log: &mut Vec<String>,
) -> Result<(Vec<PatchBlock>, Vec<mercs2_formats::model_inject::MtrlRepoint>), BuildError> {
    let t = skin.textures;
    let slots: Vec<usize> = [(0usize, &t.diffuse), (1, &t.specular), (2, &t.normal)]
        .iter()
        .filter(|(_, p)| p.is_some())
        .map(|(s, _)| *s)
        .collect();
    if slots.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let groups = mercs2_formats::texture::group_prmt_material_indices(donor_ucfx);
    let mats = mercs2_formats::texture::parse_mtrl(donor_ucfx, mercs2_formats::texture::MtrlSource::Model)
        .map_err(|e| BuildError::Lower {
            index,
            kind,
            message: format!("donor {donor_name} MTRL: {e}"),
        })?;
    let froms = rigid_slot_froms(host_group, &groups, &mats, &slots).map_err(|m| {
        BuildError::Lower {
            index,
            kind,
            message: format!("donor {donor_name}: {m}"),
        }
    })?;
    author_texture_repoints(
        index,
        kind,
        skin,
        &format!("host group {host_group}'s materials in donor {donor_name}"),
        |slot| froms.get(&slot).cloned().unwrap_or_default(),
        log,
    )
}

/// Which donor texture hashes the rigid host group's materials name at each supplied MTRL slot.
///
/// `groups` is `texture::group_prmt_material_indices` (every material each PRMG group's PRMT records
/// name) and `mats` is `texture::parse_mtrl`. A material the repoint touches — one naming a texture
/// at a supplied slot — must carry [`MTRL_TEXTURED`]; any that does not is an error, listed. A slot
/// no host material names maps to an empty list, which the caller refuses with the slot's name.
fn rigid_slot_froms(
    host_group: usize,
    groups: &[Vec<usize>],
    mats: &[mercs2_formats::texture::MtrlMaterial],
    slots: &[usize],
) -> Result<std::collections::BTreeMap<usize, Vec<u32>>, String> {
    let host = groups.get(host_group).ok_or_else(|| {
        format!(
            "host group {host_group} does not exist (the donor has {} PRMG groups)",
            groups.len()
        )
    })?;
    if host.is_empty() {
        return Err(format!("host group {host_group} binds no material (no PRMT record)"));
    }
    let mut out: std::collections::BTreeMap<usize, Vec<u32>> = std::collections::BTreeMap::new();
    let mut untextured: Vec<String> = Vec::new();
    for &m in host {
        let mat = mats.get(m).ok_or_else(|| {
            format!(
                "host group {host_group} names material {m}, but MTRL holds {} records",
                mats.len()
            )
        })?;
        let touched: Vec<(usize, u32)> = slots
            .iter()
            .filter_map(|&s| mat.textures.get(s).copied().filter(|&h| h != 0).map(|h| (s, h)))
            .collect();
        if touched.is_empty() {
            continue;
        }
        if mat.flags & MTRL_TEXTURED == 0 {
            untextured.push(format!("material {m} (flags 0x{:04X})", mat.flags));
            continue;
        }
        for (s, h) in touched {
            let list = out.entry(s).or_default();
            if !list.contains(&h) {
                list.push(h);
            }
        }
    }
    if !untextured.is_empty() {
        return Err(format!(
            "host group {host_group}'s {} lack the textured flag 0x{MTRL_TEXTURED:04X}, so they \
             are flat-shaded and would not sample the supplied map. Pick a `group:` whose \
             materials are textured.",
            untextured.join(", ")
        ));
    }
    Ok(out)
}

fn lower_skinned(
    index: usize,
    kind: &'static str,
    name: &str,
    model: &Path,
    donor_name: &str,
    retarget: &crate::manifest::Retarget,
    root: &Path,
    game: &mut GameStack,
    names: Option<&NameTable>,
    // The author's own skin: the `textures:` of the `add_outfit` / `add_model`. Empty means the
    // donor's materials.
    textures: &crate::manifest::Textures,
    // Force one draw group + the source's own weights (see `AddOutfit::single_group`). `add_model`
    // passes `false`.
    single_group: bool,
    // The first host draw group: `add_model`'s `group:` (`AddModel::group`). `None` lets the split
    // pick the largest groups; `add_outfit` has no `group:` field.
    first_host: Option<u32>,
    log: &mut Vec<String>,
) -> Result<Skinned, BuildError> {
    let lower_err = |m: String| BuildError::Lower {
        index,
        kind,
        message: m,
    };

    let donor_hash = crate::manifest::asset_hash(donor_name);
    let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
    let donor_blk = donor::donor_block(&paths, donor_hash).map_err(lower_err)?;

    let glb = char_import::load_char_glb(&root.join(model)).map_err(lower_err)?;

    // Names of the source rig's joints, in palette order — what both the convention detector and
    // the `bones:` map key on.
    let source_names: Vec<String> = glb
        .joint_nodes
        .iter()
        .map(|&n| glb.node_name.get(n).cloned().unwrap_or_default())
        .collect();

    let detected = mercs2_formats::retarget::SourceRig::detect(&source_names);
    // Compare against the canonical slug, not a substring of the prose label. `from: cod` is the
    // documented spelling and `"call of duty (iw-engine)"` does not contain "cod", so this warned
    // on every correctly-authored CoD manifest — and, being a substring test, `from: a` matched
    // everything. A warning that fires on correct input teaches authors to ignore warnings.
    let declared = retarget.from.trim().to_ascii_lowercase();
    if !declared.is_empty() && declared != detected.slug() {
        log.push(format!(
            "contributions[{index}] {kind} {name}: manifest says `from: {}` but the bone names read \
             as `{}` ({}) — building from the names in the file",
            retarget.from,
            detected.slug(),
            detected.label()
        ));
    }

    // The RESOLVED map wins when the Shipment carries one. Resolve target bone NAMES onto this
    // donor's own HIER indices by name hash, so a map authored against one donor still lands
    // correctly on another that orders its bones differently.
    let skel = mercs2_formats::skeleton::Skeleton::from_block(&donor_blk)
        .map_err(|e| lower_err(format!("donor has no readable HIER skeleton: {e}")))?;
    let hier_of_name: std::collections::HashMap<u32, u32> = skel
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name_hash, i as u32))
        .collect();

    let mut overrides: std::collections::HashMap<usize, Option<u32>> =
        std::collections::HashMap::new();
    let mut unresolved: Vec<String> = Vec::new();
    if let Some(map) = &retarget.bones {
        for (src, tgt) in map {
            let Some(si) = source_names.iter().position(|n| n == src) else {
                unresolved.push(format!("{src} (not a bone in the model)"));
                continue;
            };
            match tgt {
                None => {
                    overrides.insert(si, None); // explicit drop
                }
                Some(t) => {
                    // Through `asset_hash`, like every other reference in this format: a bare
                    // `0x…` IS the hash, anything else is a name. This called `pandemic_hash_m2`
                    // directly, which made a hash-spelled target unrepresentable — it would be
                    // hashed as the STRING "0x1C2E8837" and miss. That matters here more than
                    // most places: 21 of pmc_hum_mattias's 116 bones have no name in any corpus we
                    // have, so a name-only `bones:` map cannot address them at all and the
                    // Workshop was dropping those rows.
                    let h = crate::manifest::asset_hash(t);
                    match hier_of_name.get(&h) {
                        Some(&hier) => {
                            overrides.insert(si, Some(hier));
                        }
                        None => unresolved.push(format!("{src} -> {t} (donor has no such bone)")),
                    }
                }
            }
        }
        if !unresolved.is_empty() {
            return Err(lower_err(format!(
                "`retarget.bones` does not fit this pairing: {}. The map is the reproducible record \
                 of a remap, so a stale entry is an error rather than something to skip.",
                unresolved.join("; ")
            )));
        }
        log.push(format!(
            "contributions[{index}] {kind} {name}: applied {} explicit bone mappings",
            overrides.len()
        ));
    } else {
        // No explicit map — so derive the SAME one the Workshop's preview derives.
        //
        // This used to leave `overrides` empty, which meant `build_character` fell through to the
        // generic `char_skin::automap` no matter what the source rig was. The convention tables had
        // been lifted into the library one commit earlier precisely so this call site could reach
        // them, and it never called them. Measured on a ValveBiped source against
        // `pmc_hum_mattias`: 21 of 45 source joints landed differently, including the whole spine
        // ladder and all 18 finger bones folded onto the hands. The author previewed the corrected
        // conform and shipped the uncorrected one, with only a warning to say so.
        //
        // Bone NAMES are load-bearing here: the tables resolve targets by name, and a HIER-derived
        // skeleton is hash-named, against which every table entry misses and `mapped_count()` is 0.
        // Hence `from_skeleton_with_names` over the curated table.
        let target = mercs2_formats::char_skin::TargetSkeleton::from_skeleton_with_names(
            &skel,
            |h| names.and_then(|n| n.reverse(h)).map(|s| s.to_string()),
        );
        let named = target
            .bones
            .iter()
            .filter(|b| !b.name.starts_with("hash_"))
            .count();
        let target_names: Vec<String> = target.bones.iter().map(|b| b.name.clone()).collect();
        let target_pos: Vec<[f32; 3]> = target
            .bones
            .iter()
            .map(|b| [b.pos[0] as f32, b.pos[1] as f32, b.pos[2] as f32])
            .collect();
        // Source bind positions, index-aligned to `source_names`. `Retarget::align_by_position`
        // needs them, and `node_world` is row-major with the translation at [3]/[7]/[11].
        let source_pos: Vec<[f32; 3]> = glb
            .joint_nodes
            .iter()
            .map(|&n| {
                glb.node_world
                    .get(n)
                    .map(|m| [m[3] as f32, m[7] as f32, m[11] as f32])
                    .unwrap_or([0.0, 0.0, 0.0])
            })
            .collect();
        let rt = mercs2_formats::retarget::Retarget::build_with_pos(
            source_names.clone(),
            source_pos,
            target_names,
            target_pos,
        );
        overrides = rt.convention_overrides(target.bones.len());
        log.push(format!(
            "contributions[{index}] {kind} {name}: {} bone map from the {} table \
             ({named}/{} donor bones named, {} source joints mapped)",
            if overrides.is_empty() { "generic automap" } else { "convention" },
            rt.convention.slug(),
            target.bones.len(),
            overrides.len()
        ));
        if named * 2 < target.bones.len() {
            log.push(format!(
                "contributions[{index}] {kind} {name}: WARNING — only {named} of {} donor bones \
                 could be named, so the convention tables have little to match against. The bone \
                 map will be closer to the generic automap than to the Workshop's preview.",
                target.bones.len()
            ));
        }
    }

    // ── The author's skin ────────────────────────────────────────────────────────────────────
    //
    // `textures:` was parsed and used by nothing: `lower_skinned` passed `repoints: Vec::new()`, so
    // an outfit shipped wearing the DONOR's materials whatever the author supplied. Each supplied
    // map becomes its own resident texture block, and every donor hash currently sitting at that
    // MTRL slot is repointed onto it.
    //
    // The `from` set comes from every material in the container rather than from the host groups:
    // hosts are chosen inside the lowering, after this has to run. Repointing all of them is also
    // the honest reading of one `textures:` block for one outfit, and non-hosts are neutralised.
    let donor_ucfx = donor_container(&donor_blk);
    let donor_slots: Vec<Vec<u32>> = (0..3)
        .map(|slot| mercs2_formats::texture::material_slot_hashes(donor_ucfx, slot))
        .collect::<Result<_, _>>()
        .map_err(|e| BuildError::Lower {
            index,
            kind,
            message: format!("donor {donor_name} MTRL: {e}"),
        })?;
    let (mut tex_blocks, mut repoints) = author_texture_repoints(
        index,
        kind,
        ModelSkin { name, textures, root },
        &format!("any material of donor {donor_name}"),
        |slot| donor_slots[slot].clone(),
        log,
    )?;

    // PER-MATERIAL skins from the GLB's OWN embedded textures. When the author supplies no manual
    // `textures:`, a multi-material import wears each source material on the body region its triangles
    // host — separate head / torso / legs maps instead of one skin smeared over all of it (or the
    // donor's). The blocks + hashes are built here; the per-group repointing happens in
    // `character_into_donor`, where the host of each source part is known. Only materials a part
    // actually references are built.
    let mut part_material_textures: std::collections::HashMap<usize, [Option<u32>; 3]> =
        std::collections::HashMap::new();
    let author_supplied_skin =
        textures.diffuse.is_some() || textures.specular.is_some() || textures.normal.is_some();
    // Per-material skins need one host draw group PER source part, which is exactly what
    // `single_group` collapses away. When it is set the whole mesh shares one material, so building
    // per-material blocks would ship textures nothing repoints onto — skip them.
    if !author_supplied_skin && !single_group {
        let used: std::collections::BTreeSet<usize> =
            glb.parts.iter().filter_map(|p| p.material).collect();
        if !used.is_empty() {
            let mat_tex =
                mercs2_formats::char_import::load_char_material_textures(&root.join(model))
                    .map_err(lower_err)?;

            // A shared FLAT-MATTE specular. The donor's spec map lit the imported UVs with the wrong
            // (Mattias) highlights — the glitchy green blooms. Repointing every host material's spec
            // slot onto one near-black texture makes the import matte, which is right for tactical
            // gear (its glTF specularFactor is ~0.01) and kills the blooms. Built once, shared.
            let flat_spec: Option<u32> = {
                let px = vec![10.0f32; 4 * 4 * 4]; // 4x4 near-black RGBA
                let img = Rgba { width: 4, height: 4, pixels: px };
                match build_texture_from_rgba(index, kind, &format!("{name}_matte_sm"), img, false, None, "matte spec") {
                    Ok((block, to)) => { tex_blocks.push(block); Some(to) }
                    Err(_) => None,
                }
            };

            for &m in &used {
                let Some(tex) = mat_tex.get(m) else { continue };
                let mut slots: [Option<u32>; 3] = [None; 3];
                // DIFFUSE, brightened. Imported maps are authored far darker than the game's dim
                // interior lighting shows (measured ~18% average), so a gamma lift brings them into
                // the visible range. The NORMAL is still held back — shipping the GLB's own turned the
                // model black — so the donor's normal lights it; the flat spec below kills the wrong
                // highlights that produced.
                for (slot, suffix, png, is_normal, gamma) in
                    [(0usize, "dm", &tex.diffuse, false, Some(0.6f32))]
                {
                    let Some(bytes) = png else { continue };
                    let tex_name = format!("{name}_m{m}_{suffix}");
                    match build_named_texture_bytes(index, kind, &tex_name, bytes, is_normal, gamma, &tex_name)
                    {
                        Ok((block, to)) => {
                            slots[slot] = Some(to);
                            tex_blocks.push(block);
                        }
                        // A material whose image is not 4-aligned (or is a codec we can't read) is
                        // skipped, not fatal — that part just keeps the donor's skin.
                        Err(e) => log.push(format!(
                            "contributions[{index}] {kind} {name}: material {m} {suffix} skipped ({e})"
                        )),
                    }
                }
                // Every host material goes matte via the shared flat spec.
                slots[1] = flat_spec;
                if slots.iter().any(|s| s.is_some()) {
                    part_material_textures.insert(m, slots);
                }
            }
            if !part_material_textures.is_empty() {
                log.push(format!(
                    "contributions[{index}] {kind} {name}: shipping {} per-material skin(s) from the \
                     GLB's own embedded textures",
                    part_material_textures.len()
                ));
            }
        }
    }

    // SINGLE-GROUP DIFFUSE ATLAS. Per-material skins need one host group per material, which
    // `single_group` collapses away — so instead bake all the used materials' diffuse maps into ONE
    // atlas texture and let the lowering remap each part's UVs into its cell. That is what makes a
    // MULTI-material import wear its whole correct skin on one draw group, entirely from the GLB's own
    // embedded textures and the manifest — no hand-massaged source asset. Built here so the whole
    // Shipment stays reproducible from `single_group: true` + the original model.
    let mut atlas_cells: std::collections::HashMap<usize, [f32; 4]> = std::collections::HashMap::new();
    if !author_supplied_skin && single_group {
        let used: std::collections::BTreeSet<usize> =
            glb.parts.iter().filter_map(|p| p.material).collect();
        if !used.is_empty() {
            let mat_tex =
                mercs2_formats::char_import::load_char_material_textures(&root.join(model))
                    .map_err(lower_err)?;
            let mut decoded: Vec<(usize, Rgba)> = Vec::new();
            for &m in &used {
                let Some(mt) = mat_tex.get(m) else { continue };
                let Some(bytes) = &mt.diffuse else { continue };
                match read_png_rgba_bytes(bytes, &format!("{name} material {m} diffuse")) {
                    Ok(img) => decoded.push((m, img)),
                    Err(e) => log.push(format!(
                        "contributions[{index}] {kind} {name}: material {m} diffuse skipped ({e})"
                    )),
                }
            }
            if !decoded.is_empty() {
                let (atlas_img, cells) = bake_diffuse_atlas(&decoded);
                let (aw, ah) = (atlas_img.width, atlas_img.height);
                match build_texture_from_rgba(
                    index,
                    kind,
                    &format!("{name}_dm"),
                    atlas_img,
                    false,
                    Some(0.6),
                    &format!("{name}_dm atlas"),
                ) {
                    Ok((block, to)) => {
                        let froms = donor_slots[0].clone();
                        for from in froms {
                            repoints.push(mercs2_formats::model_inject::MtrlRepoint { from, to });
                        }
                        tex_blocks.push(block);
                        atlas_cells = cells;
                        // MATTE SPEC. Without this the donor's (Mattias's) specular map lights the
                        // atlas, so the import renders as shiny dark metal — the same wrong-highlight
                        // failure the per-material path kills with a flat spec. Repoint the donor's
                        // whole specular slot onto one near-black 4x4, which is right for tactical
                        // gear (its glTF specularFactor is ~0.01) and matte's out the metal sheen.
                        let matte: Vec<f32> = vec![10.0; 4 * 4 * 4];
                        let matte_img = Rgba { width: 4, height: 4, pixels: matte };
                        if let Ok((sblock, sto)) = build_texture_from_rgba(
                            index,
                            kind,
                            &format!("{name}_matte_sm"),
                            matte_img,
                            false,
                            None,
                            "matte spec",
                        ) {
                            for from in donor_slots[1].clone() {
                                repoints.push(mercs2_formats::model_inject::MtrlRepoint {
                                    from,
                                    to: sto,
                                });
                            }
                            tex_blocks.push(sblock);
                        }
                        // FLAT NORMAL. The mesh has ONE UV set, remapped into the atlas for the
                        // DIFFUSE — but the donor's normal map lives in the donor's UV layout, so
                        // sampling it with atlas UVs reads garbage normals and lights whole regions as
                        // if they faced away: the model goes black. Repoint the normal slot onto a flat
                        // tangent-space normal (0,0,1) so lighting falls back to the mesh's own
                        // (conformed, correct) geometry normals instead. `is_normal` swizzles the
                        // input, so a (128,128,255) texel encodes the neutral normal.
                        let flatn: Vec<f32> = [128.0f32, 128.0, 255.0, 255.0]
                            .iter()
                            .cycle()
                            .take(4 * 4 * 4)
                            .copied()
                            .collect();
                        let flatn_img = Rgba { width: 4, height: 4, pixels: flatn };
                        if let Ok((nblock, nto)) = build_texture_from_rgba(
                            index,
                            kind,
                            &format!("{name}_flat_nm"),
                            flatn_img,
                            true,
                            None,
                            "flat normal",
                        ) {
                            for from in donor_slots[2].clone() {
                                repoints.push(mercs2_formats::model_inject::MtrlRepoint {
                                    from,
                                    to: nto,
                                });
                            }
                            tex_blocks.push(nblock);
                        }
                        log.push(format!(
                            "contributions[{index}] {kind} {name}: baked a {}x{} diffuse atlas over \
                             {} materials for the single group (0x{to:08X}) + matte spec + flat normal",
                            aw,
                            ah,
                            decoded.len()
                        ));
                    }
                    Err(e) => log.push(format!(
                        "contributions[{index}] {kind} {name}: diffuse atlas skipped ({e:?}); wears \
                         the donor's skin"
                    )),
                }
            }
        }
    }

    // Say what a skin-less outfit actually ships. `wad_simulator` reports this after the fact —
    // "material[N] diffuse 0x… is base-resident but not shipped by the patch (fallback render) …
    // flags fallback-render risk in menu/wardrobe scenes" — and an author who never runs it has no
    // way to know. The materials are the DONOR's, resolved out of the base WAD at runtime, so the
    // outfit depends on those textures being resident wherever it is drawn. When the donor is also
    // the wearer that is nearly always true; when it is not, the wardrobe is where it shows.
    if repoints.is_empty() && part_material_textures.is_empty() {
        log.push(format!(
            "contributions[{index}] {kind} {name}: no `textures:` and no embedded GLB textures — \
             wears donor {donor_name}'s materials, which this patch does not ship. Fine in-world \
             where they are resident; the wardrobe/menu scene is where a fallback render would show. \
             Supply `textures:` or embed maps in the GLB to carry its own."
        ));
    }

    let opts = char_lower::LowerOpts {
        overrides,
        // A model already on the game's own rig keeps the author's weights: there is no fuzzy map
        // to repair, and resampling would discard everything painted on new geometry.
        native_rig: detected == mercs2_formats::retarget::SourceRig::Pandemic,
        repoints,
        part_material_textures,
        single_host: single_group,
        atlas_cells,
    };

    let hash = crate::manifest::asset_hash(name);
    let out = char_lower::character_into_donor_at(&donor_blk, &glb, hash, &opts, first_host.map(|g| g as usize))
        .map_err(lower_err)?;

    // Report every host, not `hosts[0]`. A two-host build logging "group 3" said nothing about
    // group 7 also being rewritten and the other 26 neutralised.
    log.push(format!(
        "contributions[{index}] {kind} {name} 0x{hash:08X} ← donor {donor_name} groups {:?}: \
         {} verts, {} tris, {} bones / {} palette slots | {}",
        out.hosts,
        out.stats.vertex_count,
        out.stats.triangle_count,
        out.skin.stats.bones,
        out.skin.palette_slots,
        out.transfer
    ));
    // `CharSkin::warnings` was populated all along and read by nothing on this path — including the
    // one that says an extremity will be stranded in space. A warning nobody prints was not issued.
    for w in &out.warnings {
        log.push(format!("contributions[{index}] {kind} {name}: WARNING — {w}"));
    }
    use mercs2_formats::char_skin::validate::Status;
    for c in out.report.checks.iter().filter(|c| c.status != Status::Ok) {
        log.push(format!(
            "contributions[{index}] {kind} {name}: {} = {:?}",
            c.title, c.status
        ));
    }

    // A repoint that matched nothing means the author's skin is in the WAD and nothing wears it.
    // xfer_apply warns here; a Shipment must not ship a silent substitution.
    let dead: Vec<String> = out
        .stats
        .mtrl_repoints
        .iter()
        .filter(|(_, _, n)| *n == 0)
        .map(|(f, t, _)| format!("0x{f:08X} -> 0x{t:08X}"))
        .collect();
    if !dead.is_empty() {
        return Err(lower_err(format!(
            "{} MTRL repoint(s) matched nothing in donor {donor_name}: {}. The texture blocks would              ship and nothing would reference them.",
            dead.len(),
            dead.join(", ")
        )));
    }

    Ok(Skinned {
        block: out.block,
        tex_blocks,
        hosts: out.hosts,
        vertex_sources: out.stats.vertex_sources,
        custom: glb.custom,
        source_tris: glb.tris,
        donor: donor_blk,
    })
}

/// What [`lower_skinned`] produced.
struct Skinned {
    /// The lowered model block.
    block: Vec<u8>,
    /// The author's texture blocks, which ship with it.
    tex_blocks: Vec<PatchBlock>,
    /// Every donor group the mesh was split across.
    hosts: Vec<usize>,
    /// The injector's source vertex per written vertex, per host group.
    vertex_sources: Vec<(usize, Vec<u32>)>,
    /// The glTF's custom vertex attributes per source vertex.
    custom: mercs2_formats::mesh_import::CustomAttributes,
    /// The source triangles, indexing those vertices.
    source_tris: Vec<[u32; 3]>,
    /// The donor block before injection.
    donor: Vec<u8>,
}

/// Re-emit an edited placement LAYER block as an overlay that shadows the base by PTHS path.
///
/// A layer (vz_state / layers_static) is edited as a whole block — the placement records live in its
/// COMP sub-blocks, patched in place by `placement::patch_transform` / `patch_model`. The overlay
/// carries the edited block at the base's own path, its ASET rows restated verbatim, and the source
/// block index recorded so `build_patch_wad_multi` re-points any block refs. On a no-op the decoded
/// content is byte-identical to the base (the writer's proven property), which is what makes shadowing
/// the whole block safe.
pub fn emit_edited_layer(
    inputs: &crate::game::LayerEditInputs,
    edited_block: &[u8],
) -> Result<PatchBlock, String> {
    let aset: Vec<AsetEntry> = inputs
        .rows
        .iter()
        .map(|r| AsetEntry::new(r.asset_hash, r.secondary_ref, r.packed_block_ref, r.type_id))
        .collect();
    let mut block = PatchBlock::from_decompressed(edited_block, inputs.path.clone(), aset, None)?;
    block.source_block_index = Some(inputs.block_index);
    Ok(block)
}

/// Re-emit a shipped model's whole LOD chain with `_P000`'s container replaced by `edited_p000`.
///
/// One single-entry block per rung the model's row names (`docs/modernization/model_chain_edits.md`):
///
/// - `_P000` carries `edited_p000`; each finer rung carries its retail container byte for byte.
/// - Each entry's third word is the byte sum of the containers before it, the shape every retail model
///   rung has (`mercs2_probe model-chains`), restated because `_P000` may have changed size.
/// - Each block keeps its source block's tier byte: the engine copies it to the stream node as the
///   block's eviction class, and a finer rung written as tier 0 could not be evicted by coarse requests.
/// - Paths are new (`mod_<hash>.block`, then `mod_<hash>_P00N_Q<3-N>.block`), so no retail block is
///   shadowed; the engine fetches a rung by the block index in its row, not by path.
/// - Only `_P000` carries the ASET row. Its rungs name the group's blocks by group-local source indices
///   `0..n`, which `build_patch_wad_multi` maps to their places once [`build`] has renumbered the group.
pub fn emit_model_chain(chain: &crate::game::ModelChain, edited_p000: &[u8]) -> Result<Vec<PatchBlock>, String> {
    let hash = chain.hash;
    if chain.rungs.is_empty() || chain.rungs.len() > 4 {
        return Err(format!("model 0x{hash:08X}: a chain has 1 to 4 rungs, this one {}", chain.rungs.len()));
    }
    let mut blocks = Vec::with_capacity(chain.rungs.len());
    let mut before = 0u32;
    for (n, rung) in chain.rungs.iter().enumerate() {
        let container = if n == 0 { edited_p000 } else { &rung.container };
        let len = u32::try_from(container.len()).map_err(|_| format!("model 0x{hash:08X} _P{n:03}: container over 4 GB"))?;
        let mut data = Vec::with_capacity(20 + container.len());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&hash.to_le_bytes());
        data.extend_from_slice(&TYPE_HASH_MODEL.to_le_bytes());
        data.extend_from_slice(&before.to_le_bytes());
        data.extend_from_slice(&len.to_le_bytes());
        data.extend_from_slice(container);
        before = before.checked_add(len).ok_or_else(|| format!("model 0x{hash:08X}: the chain's containers pass 4 GB"))?;
        let (path, rows) = if n == 0 {
            let slot = |r: usize| (r < chain.rungs.len()).then_some(r as u32);
            let (secondary, p001) = mercs2_formats::patch_wad::rung_chain([slot(0), slot(1), slot(2), slot(3)]);
            (format!("blocks\\VZ\\mod_{hash:08x}.block"), vec![AsetEntry::new(hash, secondary, p001, TYPE_ID_MODEL)])
        } else {
            (format!("blocks\\VZ\\mod_{hash:08x}_P{n:03}_Q{}.block", 3 - n), Vec::new())
        };
        let mut block = PatchBlock::from_decompressed(&data, path, rows, Some(rung.packed_field))?;
        block.source_block_index = Some(n as u32);
        blocks.push(block);
    }
    Ok(blocks)
}

/// The rungs a re-emitted chain carries, for the build log: each with its source block and tier.
fn chain_summary(chain: &crate::game::ModelChain) -> String {
    let rungs: Vec<String> = chain
        .rungs
        .iter()
        .enumerate()
        .map(|(n, r)| format!("_P{n:03} from block {} tier {}", r.block, r.packed_field >> 24))
        .collect();
    format!("carries {} rung(s) of {}: {}", rungs.len(), chain.wad.display(), rungs.join(", "))
}

/// The block a texture contribution ships, with the texture's hash: what [`lower`] ships for it, and
/// what the Workshop's Depot shows. `None` for a `replace_texture` of the `vfx` atlas: a repaint of
/// the atlas is the base every sprite of the set is drawn on, and `build` and `link` write it into the
/// resident block with the sprites ([`lower_fx`]); and for any other kind.
///
/// * `add_texture` — a standalone novel texture. Needs NO game stack: the dimensions and format
///   come from the author's own image rather than from a target the way `replace_texture` does,
///   which makes this (with `raw` and `add_movie`) one of the few kinds that exercises the whole
///   emission contract hermetically — the shape template CI runs in.
/// * `replace_texture` — the target's own dimensions and format are the spec.
pub fn texture_block(
    index: usize,
    contribution: &Contribution,
    root: &Path,
    game: Option<&mut GameStack>,
    log: &mut Vec<String>,
) -> Result<Option<(PatchBlock, u32)>, BuildError> {
    let kind = contribution.kind();
    match contribution {
        Contribution::ReplaceTexture { .. } if crate::fx::repaints_atlas(contribution) => Ok(None),
        Contribution::ReplaceTexture { target, image } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            let hash = crate::manifest::asset_hash(target);

            // The target's OWN dimensions and format are the spec: a replacement is same-hash and
            // fully resident, so it must match what the engine already expects to read.
            let existing = game.texture(hash).ok_or_else(|| BuildError::Lower {
                index,
                kind,
                message: format!(
                    "{target:?} (0x{hash:08X}) is not in the configured game stack — check the \
                     spelling; a name that does not exist hashes to a lookup that simply misses"
                ),
            })?;

            let (w, h) = (existing.width as usize, existing.height as usize);
            let rgba = read_png_rgba(&root.join(image)).map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            if rgba.width != w || rgba.height != h {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "image is {}x{} but {target} is {w}x{h}; a replacement is SAME-HASH and \
                         fully resident, so it must match the shipped dimensions exactly",
                        rgba.width, rgba.height
                    ),
                });
            }

            // Encode with the shipping encoder (`texture_encode`), not the workbench-preview one.
            let fourcc = *existing.format.fourcc();
            let body = match existing.format {
                TexFormat::Bc1 => {
                    let rgb = drop_alpha(&rgba.pixels);
                    mip_chain(w, h, 3, &rgb, encode_bc1)
                }
                TexFormat::Bc3 => mip_chain(w, h, 4, &rgba.pixels, encode_bc3),
            };
            // `build_texture_block` emits the UCFX container ALREADY WRAPPED in a single-entry
            // block table. A patch block is `[entry table][containers…]`, not a bare container —
            // handing over a raw container makes the loader read the `UCFX` magic as an entry-table
            // field. (Caught by `wad_simulator`, not by any digest check: the WAD hashed fine and
            // was structurally nonsense.)
            let mip_count = texture_encode::mip_count(w, h) as u32;
            let td = TextureData {
                width: existing.width,
                height: existing.height,
                format: existing.format,
                mip0: body[..mip0_len(w, h, existing.format).min(body.len())].to_vec(),
                all_mips: body,
                mip_count,
            };
            let block_bytes = build_texture_block(hash, &td);
            log.push(format!(
                "contributions[{index}] replace_texture {target} 0x{hash:08X} {w}x{h} {} \
                 {mip_count} mips → {} bytes",
                String::from_utf8_lossy(&fourcc),
                block_bytes.len()
            ));

            // Same hash, texture type, PRIMARY. The low 16 bits of `packed_block_ref` MUST be
            // 0xFFFF: that is what `is_primary()` tests, and any other value names a `_P001` LOD
            // block one level finer. A row pointing at a rung that does not exist is the
            // dangling-LOD-rung trap — a 549 GB buffer request and an open-world stream HANG.
            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_TEXTURE);
            // Path convention matches the proven publish pipeline
            // (`mercs2_workshop::publish`, docs/modernization/workshop_publish_pipeline.md):
            // `blocks\VZ\mod_<hash>.block`. It lands in PTHS; matching the shape that has actually
            // shipped working WADs costs nothing.
            let block = PatchBlock::from_decompressed(
                &block_bytes,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            Ok(Some((block, hash)))
        }
        Contribution::AddTexture {
            name,
            image,
            normal_map,
        } => {
            let (block, hash) =
                build_named_texture(index, kind, name, &root.join(image), *normal_map)?;
            log.push(format!(
                "contributions[{index}] add_texture {name} 0x{hash:08X} <- {} ({})",
                root.join(image).display(),
                if *normal_map { "DXT5nm normal" } else { "colour" }
            ));
            Ok(Some((block, hash)))
        }
        _ => Ok(None),
    }
}

/// Lower a single contribution into a patch block.
fn lower(
    index: usize,
    contribution: &Contribution,
    root: &Path,
    game: Option<&mut GameStack>,
    // Host-provided, like the game stack — the crate never reaches into the filesystem for it.
    // Load-bearing for the skinned path: without bone NAMES the retarget correction tables have
    // nothing to match against and every build silently falls back to the generic automap.
    names: Option<&NameTable>,
    // `shipment.name`: an `add_runtime_dll` must be named after it.
    shipment_name: &str,
    // The whole manifest: `add_model`'s shader import resolves against the Shipment's own
    // `add_shader` registrations.
    manifest: &crate::manifest::Manifest,
    log: &mut Vec<String>,
) -> Result<Lowering, BuildError> {
    let kind = contribution.kind();
    match contribution {
        Contribution::ReplaceTexture { .. } | Contribution::AddTexture { .. } => {
            Ok(texture_block(index, contribution, root, game, log)?.map_or(Lowering::Nothing, |(block, _)| Lowering::Block(block)))
        }

        Contribution::AddModel {
            name,
            model,
            donor,
            group,
            textures,
            retarget,
            collision,
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            // Resolved Q2 says `donor` may be omitted and auto-picked. Auto-pick is not written, so
            // this asks rather than guessing — a wrong host silently produces a prop with the wrong
            // rig and materials.
            let Some(donor_name) = donor else {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "add_model has no donor auto-pick — name a `donor:` explicitly. \
                             The donor supplies the rig, materials and state machine, so picking \
                             the wrong one fails quietly rather than loudly."
                        .into(),
                });
            };

            // SKINNED path. `retarget:` means the source carries a rig to be re-posed onto the
            // donor's, which needs char_skin's palette-relative BLENDINDICES and the matching
            // INFO(56) range table. Without it this stays the rigid lowering, which leaves joints
            // empty — correct for a prop, wrong for anything that animates.
            if let Some(rt) = retarget {
                // `textures:` is the model's OWN skin. Empty stays the old behaviour — a prop
                // wears the donor's materials, which is right for a prop and was wrong for a novel
                // mesh, the case the field was added for.
                let skinned = lower_skinned(
                    index, kind, name, model, donor_name, rt, root, game, names, textures, false,
                    *group, log,
                )?;
                let (mut new_block, tex_blocks) = (skinned.block, skinned.tex_blocks);
                let geometry = crate::shader_import::Geometry {
                    vertex_sources: &skinned.vertex_sources,
                    custom: &skinned.custom,
                    source_tris: &skinned.source_tris,
                    donor: &skinned.donor,
                };
                import_shaders(index, kind, name, &root.join(model), &mut new_block, &skinned.hosts, geometry, manifest, root, log)?;
                let hash = crate::manifest::asset_hash(name);
                let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
                let block = PatchBlock::from_decompressed(
                    &new_block,
                    format!("blocks\\VZ\\mod_{hash:08x}.block"),
                    vec![aset],
                    None,
                )
                .map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;
                // The model's MTRL repoints name hashes that only resolve if the skin travels with
                // it, so they ship as one `Blocks` group and cannot be separated by accident.
                if tex_blocks.is_empty() {
                    return Ok(Lowering::Block(block));
                }
                let mut out = vec![block];
                out.extend(tex_blocks);
                return Ok(Lowering::Blocks(out));
            }

            let donor_hash = crate::manifest::asset_hash(donor_name);
            let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
            let donor_blk =
                donor::donor_block(&paths, donor_hash).map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;

            let mesh = mesh_import::external_mesh_from_gltf(&root.join(model)).map_err(|m| {
                BuildError::Lower {
                    index,
                    kind,
                    message: m,
                }
            })?;

            let hash = crate::manifest::asset_hash(name);
            // The author's `group:` when given, else the default. The conform bench has always let
            // a host group be picked and had nowhere to record it, so a placement could be
            // previewed and then not expressed.
            let host_group = group.map(|g| g as usize).unwrap_or(DEFAULT_TARGET_GROUP);
            // The rigid render group is u16-indexed and draws one strip, so a dense mesh's strip can
            // exceed 65 534. Fit a RENDER copy to that budget; `follow_geometry` regenerates collision
            // from the FULL `mesh` below, so the collider stays geometry-tight while the visible LOD
            // drops. A mesh that already fits is returned unchanged (`render_decim` = None).
            let custom = mesh_import::custom_attributes_from_gltf(&root.join(model)).map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            let (render_mesh, render_custom, render_decim) = fit_render_mesh_to_u16_strip(&mesh, &custom);

            // The model's OWN skin. On the rigid path the host group keeps the donor's material
            // records (the PRMT material index is preserved), so a supplied map replaces the
            // hashes the HOST group's materials name at that slot. A rigid material samples a
            // texture only when its flags carry 0x0080 — a 0x0000 record is flat-shaded and ignores
            // whatever is bound — so a repoint onto one would ship a texture nothing draws, and is
            // refused rather than shipped.
            let (tex_blocks, repoints) = rigid_texture_repoints(
                index,
                kind,
                ModelSkin {
                    name,
                    textures,
                    root,
                },
                donor_container(&donor_blk),
                donor_name,
                host_group,
                log,
            )?;

            // Flags mirror the workshop's proven call: auto-fit OFF (the mesh carries its own
            // transform), target the raw rendered group, neutralise the rest.
            let (new_block, stats) = inject_static_into_donor_block(
                &donor_blk,
                &render_mesh,
                host_group,
                &repoints,
                hash,
                false,
                false,
                false,
                false,
                &[host_group],
                1.0,
                false,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("inject into donor {donor_name}: {m}"),
            })?;

            log.push(format!(
                "contributions[{index}] add_model {name} 0x{hash:08X} ← donor {donor_name} \
                 group {host_group}: {} verts, {} tris",
                stats.vertex_count, stats.triangle_count
            ));
            // A repoint that matched nothing means the author's map is in the WAD and nothing
            // wears it — the same refusal the skinned path makes.
            let dead: Vec<String> = stats
                .mtrl_repoints
                .iter()
                .filter(|(_, _, n)| *n == 0)
                .map(|(f, t, _)| format!("0x{f:08X} -> 0x{t:08X}"))
                .collect();
            if !dead.is_empty() {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} MTRL repoint(s) matched nothing in donor {donor_name}: {}. The \
                         texture blocks would ship and nothing would reference them.",
                        dead.len(),
                        dead.join(", ")
                    ),
                });
            }
            if let Some((rv, rt)) = render_decim {
                log.push(format!(
                    "contributions[{index}] add_model {name} 0x{hash:08X}: render LOD decimated to \
                     {rt} tris / {rv} verts to fit the rigid group's u16 strip ({} tris source \
                     preserved for collision)",
                    mesh.tris.len()
                ));
            }

            // OPT-IN: regenerate static collision from the model's OWN injected geometry, replacing
            // the donor's PHY2. Safe on ANY donor — collision is not SEGM-bound (the loader walks the
            // self-contained WpArray of model-local shapes); SEGM/INDX/HIER stay byte-unchanged. The
            // default (`CollisionSource::Donor`) keeps the donor PHY2 verbatim (backward compatible).
            let new_block = match collision {
                crate::manifest::CollisionSource::Donor => new_block,
                crate::manifest::CollisionSource::FollowGeometry => {
                    let regen =
                        regenerate_prop_collision(&new_block, &mesh, name).map_err(|m| {
                            BuildError::Lower {
                                index,
                                kind,
                                message: format!(
                                    "follow_geometry collision for {name} (donor {donor_name}): {m}"
                                ),
                            }
                        })?;
                    log.push(format!(
                        "contributions[{index}] add_model {name} 0x{hash:08X}: collision \
                         follow_geometry — regenerated PHY2 from {} tris, {} verts (SEGM untouched)",
                        mesh.tris.len(),
                        mesh.positions.len()
                    ));
                    regen
                }
            };
            let mut new_block = new_block;
            let geometry = crate::shader_import::Geometry {
                vertex_sources: &stats.vertex_sources,
                custom: &render_custom,
                source_tris: &render_mesh.tris,
                donor: &donor_blk,
            };
            import_shaders(index, kind, name, &root.join(model), &mut new_block, &[host_group], geometry, manifest, root, log)?;

            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
            let block = PatchBlock::from_decompressed(
                &new_block,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            // As on the skinned path: the MTRL names hashes only these texture blocks resolve, so
            // they ship as one group.
            if tex_blocks.is_empty() {
                return Ok(Lowering::Block(block));
            }
            let mut out = vec![block];
            out.extend(tex_blocks);
            Ok(Lowering::Blocks(out))
        }

        // `add_outfit` is a FIXED composition of add_model + a wardrobe row. The Data half lowers
        // here; the Script half is linked later, because the link generates each level's wardrobe
        // lists and character map from the installed set's outfits at once (`added_outfits`,
        // `crate::outfits`).
        // `display` is the SCRIPT half's field and is consumed by `added_outfits`; only the Data
        // half is lowered here. `slug` is kept for the log line, which is how an author confirms the
        // wardrobe row they expected is the one that was generated.
        Contribution::AddOutfit {
            name,
            slug,
            wearer,
            model,
            donor,
            retarget,
            textures,
            single_group,
            ..
        } => {
            // No model FILE → wear a model the game already ships (named by `name`): nothing to
            // inject here; the link generates the `_tOutfits` row (`added_outfits`). The injection-only
            // fields (donor / textures / retarget / single_group) do not apply.
            let Some(model) = model.as_ref() else {
                return Ok(Lowering::Nothing);
            };
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            // `textures:` binds through the SKINNED lowering's MTRL repoints. The rigid path has
            // no repoint step, so an outfit built without `retarget:` wears the donor's materials —
            // which is what silently happened to every `textures:` block before this refusal
            // existed. Refuse rather than substitute.
            if (textures.diffuse.is_some()
                || textures.normal.is_some()
                || textures.specular.is_some())
                && retarget.is_none()
            {
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "`textures:` needs the skinned lowering, which is selected by \
                             `retarget:`. The rigid path hosts geometry on the donor and performs \
                             no MTRL repoint, so the outfit would wear the DONOR's skin whatever \
                             you put here. Add `retarget:`, or drop `textures:`."
                        .into(),
                });
            }
            // Auto-pick when omitted (Plan 04 Q2): an outfit's `wearer` NAMES its valid host — the
            // hero whose wardrobe it joins and whose rig it must animate on. `pmc_hum_<wearer>` is
            // that hero's base model, and it is validated against the stack so a wrong pick fails
            // loudly here rather than silently producing an outfit rigged to nothing. `donor:` is
            // still an optional override for a variant host (`pmc_hum_mattias_v3`, …). For a list of
            // wearers it is the first listed hero's: the game ships one human skeleton, identical to
            // the millimetre across its humanoid models (bone census 2026-07-27), so any hero's rig
            // animates on every hero, and the hosts differ only in bone count and materials.
            let picked;
            let donor_name = match donor {
                Some(d) => d.as_str(),
                None => {
                    let first = wearer.heroes().first().copied().unwrap_or_default();
                    let host = auto_donor_for_wearer(first).ok_or_else(|| BuildError::Unsupported {
                        index,
                        kind,
                        reason: format!(
                            "no donor given and `wearer: {wearer}` is not a hero the auto-pick \
                             knows ({}). Name a `donor:` explicitly.",
                            WARDROBE_HEROES.join(", ")
                        ),
                    })?;
                    let hh = crate::manifest::asset_hash(host);
                    if !game.has_asset(hh, TYPE_ID_MODEL) {
                        return Err(BuildError::Lower {
                            index,
                            kind,
                            message: format!(
                                "auto-picked donor {host:?} (0x{hh:08X}) for wearer {wearer} is not \
                                 in the configured game stack — name a `donor:` explicitly"
                            ),
                        });
                    }
                    log.push(format!(
                        "contributions[{index}] add_outfit {name}: donor auto-picked {host} for \
                         wearer {wearer}"
                    ));
                    picked = host.to_string();
                    picked.as_str()
                }
            };

            // SKINNED path — an outfit that animates has to be re-posed onto the donor's rig.
            if let Some(rt) = retarget {
                let skinned = lower_skinned(
                    index, kind, name, model, donor_name, rt, root, game, names, textures,
                    *single_group, None, log,
                )?;
                let (new_block, tex_blocks) = (skinned.block, skinned.tex_blocks);
                let hash = crate::manifest::asset_hash(name);
                let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
                let block = PatchBlock::from_decompressed(
                    &new_block,
                    format!("blocks\\VZ\\mod_{hash:08x}.block"),
                    vec![aset],
                    None,
                )
                .map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;
                log.push(format!(
                    "contributions[{index}] add_outfit {name}: wardrobe row {wearer}/{slug}"
                ));
                // Model block first, then its textures — they must ship together, since the
                // model's MTRL repoints name hashes only these blocks resolve.
                let mut out = vec![block];
                out.extend(tex_blocks);
                return Ok(Lowering::Blocks(out));
            }

            let donor_hash = crate::manifest::asset_hash(donor_name);
            let paths: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
            let donor_blk =
                donor::donor_block(&paths, donor_hash).map_err(|m| BuildError::Lower {
                    index,
                    kind,
                    message: m,
                })?;
            let mesh = mesh_import::external_mesh_from_gltf(&root.join(model)).map_err(|m| {
                BuildError::Lower {
                    index,
                    kind,
                    message: m,
                }
            })?;

            let hash = crate::manifest::asset_hash(name);
            let (new_block, stats) = inject_static_into_donor_block(
                &donor_blk,
                &mesh,
                DEFAULT_TARGET_GROUP,
                &[],
                hash,
                false,
                false,
                false,
                false,
                &[DEFAULT_TARGET_GROUP],
                1.0,
                false,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("inject into donor {donor_name}: {m}"),
            })?;

            let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_MODEL);
            let block = PatchBlock::from_decompressed(
                &new_block,
                format!("blocks\\VZ\\mod_{hash:08x}.block"),
                vec![aset],
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;

            // The Script half. `Model` is the ASSET name SetOutfit receives; `Name` is the
            // unlock/tracking key; both are distinct from the display string.
            log.push(format!(
                "contributions[{index}] add_outfit {name} 0x{hash:08X} ← donor {donor_name}: \
                 {} verts, {} tris | wardrobe row {wearer}/{slug}",
                stats.vertex_count, stats.triangle_count
            ));
            Ok(Lowering::Block(block))
        }

        // A Scaleform GFx movie, added as a new `cfx_pack` asset.
        //
        // The one lowering here that needs NO game stack: `replace_texture` reads the target's
        // dimensions and `add_model` borrows a donor's rig, but a movie is self-contained — the
        // container holds the whole asset and nothing is conformed to anything. So this builds in
        // template CI, where the retail WADs will never exist.
        //
        // The movie is validated and then copied VERBATIM. `TagStream::parse` is the check that the
        // bytes are a movie at all; it is deliberately not followed by a re-encode, because retail
        // ships both compressed `CFX` (61 assets) and uncompressed `GFX` (3), so there is no
        // encoding to normalise TO, and swapping an author's verified bytes for ones nobody has run
        // is exactly the kind of helpfulness that produces a WAD that looks fine and does nothing.
        // Encoded from the authored cues with the Shipment's other sound, after every contribution
        // (`sound::lower_shipment_sound`): the block ships to each `load_in` session's WAD, and that
        // session's loader loads it.
        Contribution::AddSound { .. } => Ok(Lowering::Nothing),
        // Ships nothing: its registrations (`sound::sound_registrations`) are loaded by qm's loader.
        Contribution::LoadSound { .. } => Ok(Lowering::Nothing),
        // Lowered together, per bank, after every contribution (`sound::lower_overrides`): several
        // overrides of one bank share one forked soundbank and one override wavebank; a replaced line
        // is an override of every bank whose groups play it.
        Contribution::ReplaceSoundBank { .. } | Contribution::ReplaceSoundCue { .. } | Contribution::ReplaceSoundWave { .. } => {
            if game.is_none() {
                return Err(BuildError::GameRequired { index, kind });
            }
            Ok(Lowering::Nothing)
        }

        Contribution::AddMovie { name, movie } => {
            let route = crate::retail_movies::route(name).map_err(|message| BuildError::Lower { index, kind, message })?;
            Ok(Lowering::Movie(lower_movie(root, name, movie, index, kind, log)?, route))
        }

        // add_ui = the Data half of add_movie (this cfx_pack) + a Script half the linker bakes into
        // `qm_modloader` (collected by `ui_registrations`). Its block IS a movie block — same encoder,
        // same primary cfx_pack ASET row — so a hand-authored `add_movie` + trampoline and an `add_ui`
        // ship byte-identical Data. The composition, not a new format, is what makes the movie appear.
        Contribution::AddUi { name, movie } => {
            let route = crate::retail_movies::route(name).map_err(|message| BuildError::Lower { index, kind, message })?;
            Ok(Lowering::Movie(lower_movie(root, name, movie, index, kind, log)?, route))
        }

        // Contributes no block: its whole effect is a declared mutation, collected by
        // `script_mutations` and realised at link time.
        Contribution::PatchLua { .. } | Contribution::SetSpawnLists { .. } => Ok(Lowering::Nothing),
        // Realised at link time by `script_additions` + `link_into_blocks`. Nothing to pack here.
        Contribution::AddScript { .. } => Ok(Lowering::Nothing),
        // Realised at link time by `script_replacements` + `link_into_blocks`. Nothing to pack here.
        Contribution::ReplaceLua { .. } => Ok(Lowering::Nothing),

        // Swap a shipped model's PHY2 collision in place. Uses the proven container primitive
        // `phy2_container::replace_phy2_in_container`; the model asset hash is preserved. PHY2 lives
        // in `_P000`, and the model's whole chain is re-emitted (`emit_model_chain`) so its finer rungs
        // ship with it.
        Contribution::ReplacePhy2 { target, phy2 } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            let hash = crate::manifest::asset_hash(target);
            let chain = game
                .model_chain(hash)
                .map_err(|message| BuildError::Lower { index, kind, message })?
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("{target:?} (0x{hash:08X}) is not a model in the game stack"),
                })?;
            let container = &chain.rungs[0].container;
            let new_phy2 = std::fs::read(root.join(phy2)).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", root.join(phy2).display()),
            })?;
            let (pstart, psize) = mercs2_formats::phy2_container::phy2_span_in_container(container)
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("model container for {target:?} has no PHY2 chunk to replace"),
                })?;
            let edited = mercs2_formats::phy2_container::replace_phy2_in_container(
                container, pstart, psize, &new_phy2,
            )
            .map_err(|m| BuildError::Lower { index, kind, message: m })?;
            log.push(format!(
                "contributions[{index}] replace_phy2 {target} 0x{hash:08X}: _P000 container {} -> {} bytes; {}",
                container.len(),
                edited.len(),
                chain_summary(&chain)
            ));
            Ok(Lowering::Chain(
                emit_model_chain(&chain, &edited).map_err(|message| BuildError::Lower { index, kind, message })?,
            ))
        }

        // The passthrough kinds below all follow the same shape: read author-supplied bytes,
        // wrap them as a single-entry mod block with the right (type_hash, type_id) ASET row.
        // For each, the format side that would produce those bytes from a source description is
        // future work; the manifest side is real today for anyone who has the encoder.
        // Lowered after the loop with the Shipment's other writes to the same layer block
        // ([`lower_world`]).
        Contribution::AddPlacement { .. } | Contribution::AddLayer { .. } => {
            if game.is_none() {
                return Err(BuildError::GameRequired { index, kind });
            }
            Ok(Lowering::Nothing)
        }
        // A new clip needs nothing from retail, so it lowers hermetically.
        Contribution::AddAnimation {
            name,
            clip,
            trnm,
            events,
        } => lower_animation(
            root,
            name,
            AnimationSources {
                clip,
                trnm,
                events: events.as_deref(),
            },
            None,
            index,
            kind,
            log,
        ),
        // A replace reads its target out of retail to check it is a Havok clip.
        Contribution::ReplaceAnimation {
            target,
            clip,
            trnm,
            events,
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            lower_animation(
                root,
                target,
                AnimationSources {
                    clip,
                    trnm,
                    events: events.as_deref(),
                },
                Some(game),
                index,
                kind,
                log,
            )
        }
        // Store edits, not WAD blocks: `build` applies every shader kind of the Shipment to the
        // original stores together ([`crate::shader::apply_edits`]) and writes `data/shader3*.bin`.
        Contribution::AddShader { .. } | Contribution::ReplaceShader { .. } => Ok(Lowering::Nothing),
        // `build` applies every add_fx_sprite, add_fx and replace_fx of the Shipment to the game's
        // fxdict, atlas, effects block and worldentity together ([`lower_fx`]).
        Contribution::AddFx { .. } | Contribution::ReplaceFx { .. } | Contribution::AddFxSprite { .. } => {
            Ok(Lowering::Nothing)
        }
        Contribution::ReplaceTerrainCell { target, cell } => opaque_new_asset(root, cell, target, TYPE_HASH_TERRAIN_MESH, TYPE_ID_TERRAIN_MESH, index, kind),

        // No Data half: a shop item is pure Script-layer catalog + reward appends (see
        // `script_mutations`), composed by the linker. Nothing to pack into a block.
        Contribution::AddShopItem { .. } => Ok(Lowering::Nothing),

        // Contributes no block either: its effect is a layer registration baked into `qm_modloader`,
        // collected by `layer_registrations` and realised at link time — exactly like `add_ui`'s
        // Script half, but with no Data movie of its own.
        Contribution::ActivateLayer { .. } => Ok(Lowering::Nothing),

        // The OPEN LOWER BOUND: bytes we cannot interpret, plus a radius the author DECLARED.
        //
        // Every other kind has a second line of defence — an encoder that knows the shape, a donor
        // to conform to. `raw` has none, so everything structural that CAN be checked is checked
        // here, and a failure is a hard error rather than a warning. The declared `touches` is not
        // decoration either: it is the only thing that can mint the ASET rows, so it must agree
        // with the payload's own entry table exactly, in BOTH directions. A hash in `touches` that
        // the payload does not carry mints a row resolving to a block that does not contain it; a
        // hash the payload carries that `touches` omits is M0004's silent wedge, and it would also
        // mean the conflict system never saw the claim.
        Contribution::Raw {
            description,
            payload,
            target_layer,
            touches,
        } => {
            // The overlay is a WAD, and the Data layer is the only one a WAD holds. The other
            // three are refused by NAME rather than lowered into something plausible.
            match target_layer {
                Layer::Data => {}
                Layer::Script => {
                    return Err(BuildError::Unsupported {
                        index,
                        kind,
                        reason:
                            "a raw payload on the SCRIPT layer would ship a finished scripts_vz \
                             block. WAD resolution is last-mounted-wins, so it would silently \
                             delete every other installed Shipment's Lua — including the wardrobe \
                             rows `add_outfit` generates. That is the exact annihilation \
                             `patch_lua` exists to prevent by shipping a MUTATION instead of a \
                             block, and no declared blast radius can make it safe. Use `patch_lua`."
                                .into(),
                    });
                }
                Layer::Code => {
                    return Err(BuildError::Unsupported {
                        index,
                        kind,
                        reason:
                            "a raw payload on the CODE layer has nowhere to go: `raw` carries no \
                             destination field, and inventing one would hand the author a way to \
                             name Mercenaries2.exe or data/vz.wad — which is precisely what \
                             `native_hook` omitting `dest` keeps unreachable. Use `native_hook`, \
                             which places the file in the loader's search path for you."
                                .into(),
                    });
                }
                Layer::Runtime => {
                    return Err(BuildError::Unsupported {
                        index,
                        kind,
                        reason:
                            "the RUNTIME layer has no artifact. Nothing in the format says what a \
                             runtime payload is or where it would be placed, so there is no \
                             lowering to write — only a guess, and a guess here emits a WAD that \
                             looks fine and does nothing."
                                .into(),
                    });
                }
            }

            let path = root.join(payload);
            let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", path.display()),
            })?;

            // Two shapes an author plausibly hands us that are NOT a block. Both are named
            // explicitly, because the generic "does not parse" message sends them looking in the
            // wrong place.
            if bytes.len() >= 4 && &bytes[0..4] == b"sges" {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} starts with the `sges` magic, so it is a COMPRESSED block. Supply the \
                         DECOMPRESSED bytes — the builder compresses and computes `packed_field` \
                         from their length, and a pre-compressed payload would be compressed twice \
                         while claiming the wrong decompressed page count.",
                        path.display()
                    ),
                });
            }
            if bytes.len() >= 4 && &bytes[0..4] == b"UCFX" {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} starts with `UCFX`, so it is a bare CONTAINER, not a block. A patch \
                         block is `[entry table][containers…]`: the loader reads the first word as \
                         an entry count, so it would read the `UCFX` magic as one. Prepend the \
                         table — `[u32 count][count × (name_hash, type_hash, field_c, chunk_size)]`.",
                        path.display()
                    ),
                });
            }

            // Coherence, in the same sense `lint::coherent_block` means it: the declared count must
            // be honoured and every container must fit. `parse_block_entry_table` reads the first
            // word as a count unconditionally, so anything else yields confident nonsense.
            let (parsed, issues) =
                mercs2_formats::ucfx::walk_decompressed_block(&bytes, "raw payload");
            if parsed.entry_count == 0
                || parsed.entries.len() != parsed.entry_count as usize
                || parsed.containers.len() != parsed.entries.len()
            {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} does not read as a patch block: its first word declares {} entr(ies) \
                         but only {} row(s) and {} container(s) fit in {} bytes. A block is \
                         `[u32 count][count × 16-byte rows][containers…]`.",
                        path.display(),
                        parsed.entry_count,
                        parsed.entries.len(),
                        parsed.containers.len(),
                        bytes.len()
                    ),
                });
            }
            if !issues.is_empty() {
                let detail: Vec<String> = issues
                    .iter()
                    .map(|i| format!("{}: {}", i.context, i.detail))
                    .collect();
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{} is structurally invalid — {}. These are the checks the engine's own \
                         reader performs; a payload that fails them loads as garbage rather than \
                         failing loudly.",
                        path.display(),
                        detail.join("; ")
                    ),
                });
            }

            // `touches` are asset REFERENCES: a bare `0x…` is that hash, anything else is a name.
            let declared: std::collections::BTreeSet<u32> = touches
                .iter()
                .map(|t| crate::manifest::asset_hash(&t.0))
                .collect();
            let carried: std::collections::BTreeSet<u32> =
                parsed.entries.iter().map(|e| e.name_hash).collect();
            let hexes = |set: std::collections::BTreeSet<u32>| {
                set.iter()
                    .map(|h| format!("0x{h:08X}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let missing: std::collections::BTreeSet<u32> =
                declared.difference(&carried).copied().collect();
            if !missing.is_empty() {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "`touches` claims {} which the payload's entry table does not carry. The \
                         claim is what mints the ASET row, so this would publish a row pointing at \
                         a block that has no such asset in it — the lookup resolves, the block \
                         loads, and the asset is simply absent.",
                        hexes(missing)
                    ),
                });
            }
            let extra: std::collections::BTreeSet<u32> =
                carried.difference(&declared).copied().collect();
            if !extra.is_empty() {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "the payload carries {} which `touches` does not claim. Nothing else can \
                         infer a raw block's radius, so an unclaimed asset gets no ASET row (the \
                         M0004 silent wedge) and the conflict system never sees the claim at all — \
                         two Shipments could overwrite one asset without either being told.",
                        hexes(extra)
                    ),
                });
            }

            // The TYPE comes from the bytes, never from the author: the ASET row's type id decides
            // which loader the engine dispatches, and a guess there resolves the asset into the
            // wrong subsystem.
            let mut aset = Vec::new();
            for e in &parsed.entries {
                let type_id = mercs2_formats::types::type_id_for_type_hash(e.type_hash)
                    .ok_or_else(|| BuildError::Lower {
                        index,
                        kind,
                        message: format!(
                            "entry 0x{:08X} declares type hash 0x{:08X}, which is not one of the \
                             {} types the retail census found. The ASET row's type id is derived \
                             from it and decides which loader is dispatched, so there is nothing \
                             safe to guess.",
                            e.name_hash,
                            e.type_hash,
                            mercs2_formats::types::TYPE_HASH_REGISTRY.len()
                        ),
                    })?;
                // Sentinel rungs. A `0x0000` low-16 is the dangling-rung HANG, not "no rung".
                aset.push(AsetEntry::new(
                    e.name_hash,
                    0xFFFF_FFFF,
                    0x0000_FFFF,
                    type_id,
                ));
            }

            let first = parsed.entries[0].name_hash;
            log.push(format!(
                "contributions[{index}] raw {} {} bytes, {} entr(ies): {}",
                description.as_deref().unwrap_or("(no description)"),
                bytes.len(),
                parsed.entries.len(),
                parsed
                    .entries
                    .iter()
                    .map(|e| format!(
                        "0x{:08X} {}",
                        e.name_hash,
                        mercs2_formats::types::type_name_from_hash(e.type_hash)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));

            let block = PatchBlock::from_decompressed(
                &bytes,
                format!("blocks\\VZ\\mod_{first:08x}.block"),
                aset,
                None,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: m,
            })?;
            Ok(Lowering::Block(block))
        }

        // The Code layer. This is the ONE kind that emits no WAD content: on retail a plugin is an
        // `.asi` — a plain Windows DLL under a different extension — dropped where `pmc_bb.dll`
        // globs for it. So it lowers to a placement, and the placement record is what makes it
        // reversible: an overlay is undone by deleting one file, but a file dropped into the game
        // folder cannot be backed out unless something wrote down what was put where.
        Contribution::NativeHook {
            target,
            plugin,
            symbol,
            touches,
            // A record-only field: qm carries the sig map through so lint/report can compare it
            // against a live plugin's expected prologue bytes; it does not affect the packed WAD.
            signature_guard: _,
        } => {
            let Some(plugin) = plugin else {
                // M0161 already blocks the both-absent case, so reaching here means a `symbol` with
                // no payload.
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: format!(
                        "this contribution names the symbol {} but ships no `plugin:`, and the \
                         Quartermaster does not compile native code — there is no binary for it to \
                         produce. Build the hook into an `.asi` and ship that, or, if the plugin is \
                         somebody else's, require the Shipment that ships it in `load.requires` \
                         rather than vendoring their binary.",
                        symbol.as_deref().unwrap_or("(none)")
                    ),
                });
            };
            if *target != crate::manifest::Target::Retail {
                // M0160 already blocks reimpl+plugin as an Error, so this is the belt to its
                // braces: if that rule is ever relaxed, the lowering must still not place an ASI
                // into a runtime that has no loader for one.
                return Err(BuildError::Unsupported {
                    index,
                    kind,
                    reason: "an `.asi` is a RETAIL mechanism — `pmc_bb.dll` loads it into the \
                             retail exe. The reimpl Code layer is a Rust/wasm/Lua plugin and has no \
                             consumer yet, so there is nothing to place."
                        .into(),
                });
            }

            let path = root.join(plugin);
            let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", path.display()),
            })?;

            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("{} has no usable file name", path.display()),
                })?
                .to_string();

            // The loader globs `*.asi`. A plugin under any other extension is placed correctly and
            // never even considered — the quietest possible failure, and the file is right there
            // looking installed. The name is also the FileArtifact claim the conflict system keys
            // on, so renaming here would make the claim and the placement disagree.
            if !name.to_ascii_lowercase().ends_with(".asi") {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{name} is not an `.asi`. The loader globs `*.asi` across the game folder \
                         and scripts/plugins/update, so a file under any other extension is never \
                         considered — it would sit in the right place, hashing correctly, doing \
                         nothing. Rename the built DLL to `.asi`."
                    ),
                });
            }
            // The shared file-in-the-game-folder rules: a single path component, and not the
            // loader's own reserved name. `place_file` runs the same check, so the two kinds cannot
            // drift into disagreeing about what a placeable filename is.
            if let Some(why) = game_folder_name_refusal(&name) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("{name} cannot be placed: {why}. Rename it."),
                });
            }
            if let Some(why) = crate::pe::pe_dll_load_blocker(&bytes, kind) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{name} cannot be loaded by the game: {why}. The loader reports this as \
                         `[FAILED] … (error: …)` in pmc_blackbox.log rather than silently, but only \
                         to someone who reads it."
                    ),
                });
            }

            let relative = place_path(ASI_SUBDIR, &name);
            // The digest is recorded plainly and claims only INTEGRITY. A hash of malware is a
            // correct hash, and a Shipment recording its own payload's digest proves internal
            // consistency and nothing else — so the log says what an ASI is rather than letting a
            // green digest read as a safety check.
            log.push(format!(
                "contributions[{index}] native_hook {name} → {relative}: {} bytes, sha256 {} \
                 (hooks: {}) — UNRESTRICTED NATIVE CODE in the game process; the digest proves the \
                 bytes are unmodified, not that they are safe",
                bytes.len(),
                sha256_hex(&bytes),
                if touches.is_empty() {
                    "none declared".to_string()
                } else {
                    touches
                        .iter()
                        .map(|t| t.0.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ));

            Ok(Lowering::File {
                name,
                relative,
                bytes,
            })
        }

        // A companion file. Mechanically this is `native_hook` minus the PE checks and plus a
        // destination the author named — but the destination is a NAME out of a closed set, so the
        // property that matters is unchanged: no author input reaches the directory half of the
        // path, and the filename half comes from the source file rather than from a field.
        //
        // Needs no game stack, so it lowers in template CI.
        Contribution::PlaceFile { file, dest } => {
            let path = root.join(file);
            let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", path.display()),
            })?;

            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("{} has no usable file name", path.display()),
                })?
                .to_string();

            // M0162 already blocks every one of these as an Error, so this is the belt to its
            // braces — the same shape M0160/M0161 have with `native_hook`'s lowering. A refusal
            // that lives only in a lint rule is a refusal that stops existing the moment somebody
            // adds a way to suppress rules.
            if let Some(why) = companion_name_refusal(&name) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("{name} cannot be placed: {why}."),
                });
            }

            let relative = place_path(dest.relative_dir(), &name);
            log.push(format!(
                "contributions[{index}] place_file {name} → {relative}: {} bytes, sha256 {}",
                bytes.len(),
                sha256_hex(&bytes),
            ));

            Ok(Lowering::File {
                name,
                relative,
                bytes,
            })
        }

        // A runtime DLL, placed in the game root. The same shape as `native_hook`'s placement —
        // bytes read once, the name refused before anything is written, the PE header checked —
        // with the destination fixed to the game root and the name fixed to `<shipment.name>.dll`.
        // Needs no game stack, so it lowers in template CI.
        Contribution::AddRuntimeDll { dll } => {
            let path = root.join(dll);
            let bytes = std::fs::read(&path).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", path.display()),
            })?;
            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!("{} has no usable file name", path.display()),
                })?
                .to_string();
            // M0162 already blocks these as an Error; this is the belt to its braces.
            if let Some(why) = runtime_dll_name_refusal(&name, shipment_name) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("{name} cannot be placed: {why}."),
                });
            }
            if let Some(why) = crate::pe::pe_dll_load_blocker(&bytes, kind) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("{name} cannot be loaded by the game: {why}."),
                });
            }
            let relative = place_path(crate::manifest::PlaceIn::GameRoot.relative_dir(), &name);
            log.push(format!(
                "contributions[{index}] add_runtime_dll {name} → {relative}: {} bytes, sha256 {} — \
                 UNRESTRICTED NATIVE CODE in the game process; the digest proves the bytes are \
                 unmodified, not that they are safe",
                bytes.len(),
                sha256_hex(&bytes),
            ));
            Ok(Lowering::File {
                name,
                relative,
                bytes,
            })
        }

        // The destruction state machine, EDITED in place. The machine lives in the model's `_P000`
        // container; the model's ASET row also names finer rungs holding its near geometry. We do NOT
        // shadow any base block: the model's whole chain is re-emitted as its own single-entry blocks
        // (`emit_model_chain`), `_P000` edited and every finer rung carried byte for byte, so no
        // block-mate is shadowed and no rung is dropped. `states:` is the
        // extracted-then-edited machine (see `crate::states`); `serialize_state_machine` REGENERATES
        // the whole family from it, so an edit may add or remove nodes and states, not only rewrite
        // them. What it cannot make safe is state IDENTITY: a state's hash is the engine's GLOBAL
        // `SetState` address, so an edit that names a state outside the known vocabulary (and outside
        // the model's own base states) is warned about — it ships, but the damage system will never
        // reach that state.
        Contribution::EditStateMachine { target, states } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            let hash = crate::manifest::asset_hash(target);
            let chain = game
                .model_chain(hash)
                .map_err(|message| BuildError::Lower { index, kind, message })?
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{target:?} (0x{hash:08X}) is not a model in the configured game stack — check \
                         the spelling; a name that does not exist hashes to a lookup that simply misses"
                    ),
                })?;
            let p000 = &chain.rungs[0].container;

            let base = mercs2_formats::orchestrator::parse_state_machine(p000)
                .ok_or_else(|| BuildError::Lower {
                    index,
                    kind,
                    message: format!(
                        "{target:?} is a model but carries no destruction state machine — \
                         edit_state_machine only applies to destructibles (SWIT/NODE/STAT/CHDR/CEXE)"
                    ),
                })?;

            let text = std::fs::read_to_string(root.join(states)).map_err(|e| BuildError::Lower {
                index,
                kind,
                message: format!("reading {}: {e}", root.join(states).display()),
            })?;
            let edited_sm = crate::states::parse(&text).map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!("{}: {m}", root.join(states).display()),
            })?;

            // ── State IDENTITY guard (M0193) ────────────────────────────────────────────────────
            //
            // A state's `name_hash` is the engine's global `SetState` address, not a per-model label.
            // Warn on any edited state whose hash is neither one the base model already used nor a
            // member of the known global vocabulary — it is legal to ship, but the damage system's
            // transitions will never reach it, so the state is effectively dead.
            let base_states: std::collections::BTreeSet<u32> =
                base.nodes.iter().flat_map(|n| n.states.iter().map(|s| s.name_hash)).collect();
            for node in &edited_sm.nodes {
                for st in &node.states {
                    if !base_states.contains(&st.name_hash)
                        && !mercs2_formats::orchestrator::is_known_state(st.name_hash)
                    {
                        log.push(format!(
                            "contributions[{index}] WARNING ({}): state 0x{:08X} is not a known \
                             destruction state and was not in the base model — the engine's SetState \
                             will never transition into it, so it is unreachable. Edit a state's \
                             Enter/Exit scripts rather than renaming it, or use a vocabulary state \
                             (PristineState, DamagedState, DestroyedState, GoneState, …).",
                            crate::lint::M0193_STATE_OFF_VOCABULARY.code,
                            st.name_hash
                        ));
                    }
                }
            }

            let edited = mercs2_formats::orchestrator::serialize_state_machine(
                p000,
                &edited_sm,
            )
            .map_err(|m| BuildError::Lower {
                index,
                kind,
                message: format!(
                    "applying the edited states to {target:?}: {m}. The family is regenerated in \
                     full, so adding or removing nodes and states is fine — start from an extracted \
                     baseline so the shape and state identities are the model's own"
                ),
            })?;

            let n_nodes = edited_sm.nodes.len();
            let n_states: usize = edited_sm.nodes.iter().map(|n| n.states.len()).sum();
            let (b_nodes, b_states): (usize, usize) =
                (base.nodes.len(), base.nodes.iter().map(|n| n.states.len()).sum());
            log.push(format!(
                "contributions[{index}] edit_state_machine {target} 0x{hash:08X}: \
                 {b_nodes}\u{2192}{n_nodes} node(s), {b_states}\u{2192}{n_states} state(s); \
                 _P000 container {} -> {} bytes; {}",
                p000.len(),
                edited.len(),
                chain_summary(&chain)
            ));
            Ok(Lowering::Chain(
                emit_model_chain(&chain, &edited).map_err(|message| BuildError::Lower { index, kind, message })?,
            ))
        }

        // Edit a placement LAYER (vz_state / layers_static): move / rotate / re-model its entities in
        // place, with the proven in-place `placement::patch_*` writer (matched by entity key, or by
        // name via the layer's own Name COMP). Lowered after the loop with the Shipment's other
        // writes to the same layer block ([`lower_world`]).
        Contribution::EditWorld { .. } => {
            if game.is_none() {
                return Err(BuildError::GameRequired { index, kind });
            }
            Ok(Lowering::Nothing)
        }

        // Stand-ins are lowered after the loop: their models by [`crate::tiny::lower_shipment`], their
        // placements with the Shipment's other writes to each layer block ([`lower_world`]).
        Contribution::AddTinyGeometry { .. } => Ok(Lowering::Nothing),

        // String tables are lowered after the loop, all of a Shipment's writes to one table
        // together and in contribution order ([`merge_string_tables`], strict), so a later
        // contribution sees an earlier one's edits and each table ships as ONE block.
        Contribution::EditStringDb { .. }
        | Contribution::AddStringDbKeys { .. }
        | Contribution::ReplaceStringDbText { .. } => Ok(Lowering::Nothing),

        // Compiled with its `add_level`.
        Contribution::LevelAsset { .. } => Ok(Lowering::Nothing),

        // Composed by `qm link` with every installed Shipment's items into one edit of the page's movie
        // ([`crate::menu_items`]); a Shipment's own build ships nothing for it.
        Contribution::AddMenuItem { .. } => Ok(Lowering::Nothing),

        // Resolved by `package::materialize` before the build; `build` refuses one that is not.
        Contribution::FetchPackage { .. } => Err(BuildError::Lower {
            index,
            kind,
            message: "a fetch_package row reached the lowering unresolved: run it through package::materialize first".to_string(),
        }),

        // A NEW level: its assets compiled into a WAD above a copy of `vz.wad` (`crate::level`).
        Contribution::AddLevel { name, display, streams, .. } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            if let Some(why) = wad_name_refusal(kind, name) {
                return Err(BuildError::Lower { index, kind, message: format!("add_level name {name:?} is not usable: {why}") });
            }
            let level = crate::level::read(root, name, &crate::level::assets_of(manifest, name), streams, game)
                .map_err(|message| BuildError::Lower { index, kind, message })?;
            log.push(format!(
                "contributions[{index}] add_level {name}: {} block(s), {} stream(s), {} script and texture reference(s), {} of them only in the base, {} pixel and {} vertex shader(s) → .\\Data\\{name}-patch.wad above a copy of vz.wad",
                level.blocks.len(),
                level.streams.len(),
                level.references,
                level.from_base,
                level.pixel_shaders,
                level.vertex_shaders,
            ));
            Ok(Lowering::LevelWad { name: name.clone(), display: display.clone(), level })
        }

        // A NEW language: `data/<name>.wad` carries its string table (the base table forked, the
        // translation applied, re-keyed to `m2(name)`), its fonts and atlases (forked from the
        // base's, `language::fork_fonts`) and English's voice-over tables re-keyed to
        // `<bank>.<name>` (`language::fork_vo_tables`). The voice stream is copied at deploy.
        Contribution::AddLanguage {
            name,
            display,
            strings,
            base,
        } => {
            let Some(game) = game else {
                return Err(BuildError::GameRequired { index, kind });
            };
            // Belt to M0200's braces: never mint a WAD name that shadows a shipped one, even if the
            // rule was suppressed. The `data/` write is only safe because this holds.
            if let Some(why) = language_name_refusal(name) {
                return Err(BuildError::Lower {
                    index,
                    kind,
                    message: format!("add_language name {name:?} is not usable: {why}"),
                });
            }

            let base_name = base.as_deref().unwrap_or(LANGUAGE_BASE);
            let (edits, table) = language_table(game, root, strings, base.as_deref())
                .map_err(|message| BuildError::Lower { index, kind, message })?;
            let (base_hash, edited) = (table.table, table.merged);
            let hash = crate::manifest::asset_hash(name);
            let fail = |message: String| BuildError::Lower { index, kind, message };

            let mut assets = vec![crate::language::Asset {
                name_hash: hash,
                type_hash: TYPE_HASH_STRINGDB,
                type_id: TYPE_ID_STRINGDB,
                container: edited,
            }];
            let fonts = crate::language::fork_fonts(game, base_name, name).map_err(fail)?;
            assets.extend(fonts);
            let main = crate::language::assets_block(format!("blocks\\{name}\\{name}.block"), &assets)
                .map_err(fail)?;

            // English's voice-over tables, one block per re-keyed entry name.
            let vo = crate::language::fork_vo_tables(game, name).map_err(fail)?;
            let mut by_name: std::collections::BTreeMap<u32, Vec<crate::language::Asset>> =
                std::collections::BTreeMap::new();
            for a in vo {
                by_name.entry(a.name_hash).or_default().push(a);
            }
            let mut blocks = vec![main];
            let vo_tables: usize = by_name.values().map(Vec::len).sum();
            for (entry, tables) in &by_name {
                blocks.push(
                    crate::language::assets_block(format!("blocks\\{name}\\vo_{entry:08x}.block"), tables)
                        .map_err(fail)?,
                );
            }
            log.push(format!(
                "contributions[{index}] add_language {name} 0x{hash:08X} ← fork {base_name} \
                 0x{base_hash:08X}: {} key(s) translated, fonts and atlases {}_18/_20, {vo_tables} \
                 voice-over table(s) under {} name(s) → .\\Data\\{name}.wad",
                edits.len(),
                name,
                by_name.len(),
            ));
            Ok(Lowering::LanguageWad {
                language: name.clone(),
                display: display.clone(),
                blocks,
                stream_copy: (
                    crate::language::VO_STREAM_FROM.to_string(),
                    crate::language::vo_stream_to(name),
                ),
            })
        }
    }
}

/// The hero base model that hosts an outfit for `wearer`, or `None` for an unknown hero.
///
/// An outfit's `wearer` is which hero's wardrobe it joins, so that hero's own base model is its
/// only correct donor — the rig and materials it must animate on. This is the auto-pick Plan 04 Q2
/// resolved for `add_outfit`.
///
/// Both `jen` (the Workshop's pill label) and `jennifer` (the `_tOutfits` key the linter checks)
/// map to `pmc_hum_jen`, because the model name follows neither spelling and both reach the same
/// hero. `add_model` has no wearer and gets no auto-pick — a prop's host is geometry-dependent, and
/// the weight-transfer history shows guessing it is how a model ends up rigged to nothing.
fn auto_donor_for_wearer(wearer: &str) -> Option<&'static str> {
    match wearer.trim().to_ascii_lowercase().as_str() {
        "mattias" => Some("pmc_hum_mattias"),
        "chris" => Some("pmc_hum_chris"),
        "jen" | "jennifer" => Some("pmc_hum_jen"),
        _ => None,
    }
}

/// Parse a string-edits file: `[Bracket.Key] = New text` per line, or `[Bracket.Key]: New text`.
///
/// Deliberately line-oriented rather than YAML: the values are UTF-16 UI text that routinely
/// carries `:`, `%s`, quotes and colons, and a YAML parser would demand escaping the very
/// characters the strings are made of. `#` begins a comment; blank lines are skipped. The key is the
/// bracket key verbatim, hashed by the engine's own `pandemic_hash_m2`. Each line is read by
/// [`string_edit_line`].
pub fn parse_string_edits(text: &str) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut out = std::collections::BTreeMap::new();
    for (n, raw) in text.lines().enumerate() {
        if let Some((key, value)) = string_edit_line(raw).map_err(|m| format!("line {}: {m}", n + 1))? {
            out.insert(key, value);
        }
    }
    Ok(out)
}

/// One line of a string-edits file ([`parse_string_edits`]): `None` for a blank or `#` line, else the
/// key as written and its text.
pub fn string_edit_line(raw: &str) -> Result<Option<(String, String)>, String> {
    let line = raw.trim_end_matches(['\r', '\n']);
    let t = line.trim_start();
    if t.is_empty() || t.starts_with('#') {
        return Ok(None);
    }
    // The key is either a `[Bracket.Key]` token or a bare `0xHASH` — the same rule
    // `stringdb::edit_container` resolves them by, since a bracket key's reverse is not always
    // known and a dump gives only the hash.
    let (key, rest) = if t.starts_with('[') {
        let close = t.find(']').ok_or("unterminated `[` key")?;
        (t[..=close].to_string(), t[close + 1..].trim_start())
    } else {
        // A `0xHASH key = value`: split on the first `=`/`:`, then the left side is the key.
        let sep = t.find(['=', ':']).ok_or("expected `=` or `:` after the key")?;
        let k = t[..sep].trim();
        let is_hash = k
            .strip_prefix("0x")
            .or_else(|| k.strip_prefix("0X"))
            .is_some_and(|h| !h.is_empty() && h.len() <= 8 && h.chars().all(|c| c.is_ascii_hexdigit()));
        if !is_hash {
            return Err(format!("expected a `[Bracket.Key]` or a bare `0xHASH`, got {k:?}"));
        }
        (k.to_string(), &t[sep..])
    };
    let value = rest
        .trim_start()
        .strip_prefix('=')
        .or_else(|| rest.trim_start().strip_prefix(':'))
        .ok_or("expected `=` or `:` after the key")?
        .trim();
    Ok(Some((key, value.to_string())))
}

/// The string-edits line ([`string_edit_line`]) that sets `key` to `text`: `[Key] = text`. A key is
/// a `[Bracket.Key]` or a bare `0xHASH`. Refused, with the reason, when the line cannot hold it: a
/// newline in either, leading or trailing whitespace in the text (the reader trims it), a `]` inside
/// a bracket key.
pub fn string_edit_text(key: &str, text: &str) -> Result<String, String> {
    let bracket = key.strip_prefix('[').and_then(|k| k.strip_suffix(']'));
    let hash = key
        .strip_prefix("0x")
        .or_else(|| key.strip_prefix("0X"))
        .is_some_and(|h| !h.is_empty() && h.len() <= 8 && h.chars().all(|c| c.is_ascii_hexdigit()));
    match bracket {
        Some("") => return Err("the key is empty".into()),
        Some(inner) if inner.contains([']', '\n', '\r']) => {
            return Err(format!("{key}: a key cannot hold `]` or a line break"))
        }
        Some(_) => {}
        None if hash => {}
        None => return Err(format!("{key:?}: a key is a `[Bracket.Key]` or a bare `0xHASH`")),
    }
    if text.contains(['\n', '\r']) {
        return Err("the text holds a line break, and a strings file holds one line per key".into());
    }
    if text.trim() != text {
        return Err("the text starts or ends with whitespace, which a strings file trims".into());
    }
    let line = if text.is_empty() { format!("{key} =") } else { format!("{key} = {text}") };
    match string_edit_line(&line) {
        Ok(Some((k, v))) if k == key && v == text => Ok(line),
        _ => Err(format!("{line:?} does not read back as {key} = {text:?}")),
    }
}

/// The refusal for `replace_stringdb_text` pairs whose old text matches no entry: it names the
/// Shipment, the table and every unmatched old text.
fn text_misses_message(shipment: &str, table: &str, misses: &[&String]) -> String {
    format!(
        "{shipment}: in the string table {table}, no entry's text is exactly {} — a replacement that \
         matches nothing changes nothing, so it is refused (check the spelling, and that the text is \
         the table's text at this point in the load order)",
        misses.iter().map(|m| format!("{m:?}")).collect::<Vec<_>>().join(", ")
    )
}

/// Parse a `replace_stringdb_text` pairs file: one `old<TAB>new` pair per line, in file order.
///
/// The format is the kind's documented one (`manifest::Contribution::ReplaceStringDbText`): a line
/// starting with `#` is a comment and a blank line is skipped; every other line is exactly one tab
/// between the old text and the new. Nothing is trimmed or unescaped — the text is compared with the
/// table's text exactly, so a space or a `:` in it is part of it. A trailing `\r` (a CRLF file) is
/// not part of the text. A line with no tab, more than one tab, or an empty old text is refused,
/// naming `file` and the line.
pub fn parse_text_pairs(text: &str, file: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.split('\n').enumerate() {
        if let Some(pair) = text_pair_line(raw).map_err(|m| format!("{file}:{}: {m}", n + 1))? {
            out.push(pair);
        }
    }
    Ok(out)
}

/// One line of a pairs file ([`parse_text_pairs`]), split on `\n`: `None` for a blank or `#` line,
/// else the old text and the new.
pub fn text_pair_line(raw: &str) -> Result<Option<(String, String)>, String> {
    let line = raw.strip_suffix('\r').unwrap_or(raw);
    if line.trim().is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let mut parts = line.split('\t');
    let (Some(old), Some(new)) = (parts.next(), parts.next()) else {
        return Err("no tab — each line is `old<TAB>new`".into());
    };
    if parts.next().is_some() {
        return Err("more than one tab — each line is exactly `old<TAB>new`".into());
    }
    if old.is_empty() {
        return Err("the old text is empty — there is nothing to match".into());
    }
    Ok(Some((old.to_string(), new.to_string())))
}

/// The pairs line ([`text_pair_line`]) that replaces `old` with `new`: `old<TAB>new`. Refused, with
/// the reason, when the line cannot hold it: a tab or a line break in either text, an empty old text,
/// an old text starting with `#` (a comment) or one that, with the new, is only whitespace (a blank
/// line).
pub fn text_pair_text(old: &str, new: &str) -> Result<String, String> {
    if old.is_empty() {
        return Err("the old text is empty — there is nothing to match".into());
    }
    for (what, t) in [("old", old), ("new", new)] {
        if t.contains('\t') {
            return Err(format!("the {what} text holds a tab, and a pairs line is `old<TAB>new`"));
        }
        if t.contains(['\n', '\r']) {
            return Err(format!("the {what} text holds a line break, and a pairs file holds one pair per line"));
        }
    }
    if old.starts_with('#') {
        return Err("the old text starts with `#`, which makes the line a comment".into());
    }
    let line = format!("{old}\t{new}");
    if line.trim().is_empty() {
        return Err("both texts are only spaces, which makes the line blank".into());
    }
    match text_pair_line(&line) {
        Ok(Some((o, n))) if o == old && n == new => Ok(line),
        _ => Err(format!("{line:?} does not read back as the pair written")),
    }
}

pub(crate) struct Rgba {
    pub(crate) width: usize,
    pub(crate) height: usize,
    /// Straight RGBA as `f32` in 0..=255, the shape `texture_encode` expects.
    pub(crate) pixels: Vec<f32>,
}

pub(crate) fn read_png_rgba(path: &Path) -> Result<Rgba, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    read_png_rgba_from(file, &path.display().to_string())
}

/// Decode a PNG from an in-memory buffer — the embedded-image path (a GLB carries its textures as
/// PNG bytes in a buffer view, so there is no file to open).
fn read_png_rgba_bytes(bytes: &[u8], label: &str) -> Result<Rgba, String> {
    read_png_rgba_from(std::io::Cursor::new(bytes), label)
}

/// Shared decoder over any reader, so a file path and an embedded buffer share one code path.
fn read_png_rgba_from<R: std::io::Read>(r: R, label: &str) -> Result<Rgba, String> {
    let decoder = png::Decoder::new(r);
    let mut reader = decoder.read_info().map_err(|e| format!("{label}: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{label}: {e}"))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = info.color_type.samples();
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{label}: only 8-bit PNGs are supported"));
    }
    let mut pixels = vec![0f32; w * h * 4];
    for i in 0..w * h {
        let src = i * channels;
        let (r, g, b, a) = match channels {
            1 => (buf[src], buf[src], buf[src], 255),
            2 => (buf[src], buf[src], buf[src], buf[src + 1]),
            3 => (buf[src], buf[src + 1], buf[src + 2], 255),
            _ => (buf[src], buf[src + 1], buf[src + 2], buf[src + 3]),
        };
        pixels[i * 4] = r as f32;
        pixels[i * 4 + 1] = g as f32;
        pixels[i * 4 + 2] = b as f32;
        pixels[i * 4 + 3] = a as f32;
    }
    Ok(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

fn drop_alpha(rgba: &[f32]) -> Vec<f32> {
    rgba.chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect()
}

/// Byte length of mip level 0 for a BC-compressed surface: 4x4 blocks, 8 bytes for BC1, 16 for BC3.
fn mip0_len(w: usize, h: usize, format: TexFormat) -> usize {
    let blocks = w.div_ceil(4).max(1) * h.div_ceil(4).max(1);
    blocks
        * match format {
            TexFormat::Bc1 => 8,
            TexFormat::Bc3 => 16,
        }
}

/// Lint, lower, assemble, emit.
///
/// `out_dir` defaults to `<root>/_build`. Returns `Err(BuildError::Blocked)` rather than a report
/// when the linter blocks — the gate is the return type, not a field a caller might not read.
pub fn build(
    shipment: &LoadedShipment,
    mut game: Option<&mut GameStack>,
    names: Option<&NameTable>,
    out_dir: Option<&Path>,
    corpus_root: Option<&Path>,
    original_data: Option<&Path>,
) -> Result<BuildReport, BuildError> {
    let mut log = Vec::new();
    let manifest = &shipment.manifest;
    // `fetch_package` and `from:` are resolved before build by `package::materialize`; one that reaches
    // here would build a level without its assets.
    for (index, c) in manifest.contributions.iter().enumerate() {
        let unresolved = match c {
            Contribution::FetchPackage { .. } => true,
            Contribution::AddLevel { from, .. } | Contribution::LevelAsset { from, .. } => from.is_some(),
            _ => false,
        };
        if unresolved {
            return Err(BuildError::Lower {
                index,
                kind: c.kind(),
                message: "a fetch_package row reached the build unresolved: run it through package::materialize first".to_string(),
            });
        }
    }

    let mut diagnostics = lint::lint(manifest, Some(&shipment.root), names);
    // Rules that need the retail WADs run only when a stack is configured. They are appended
    // BEFORE the gate so a game-aware Error would still block, even though M0007 is a warning.
    if let Some(g) = game.as_deref_mut() {
        diagnostics.extend(lint::game_checks(manifest, g));
        diagnostics.extend(
            lint::fx_game_checks(manifest, &shipment.root, g)
                .map_err(|message| BuildError::Lower { index: 0, kind: "fx", message })?,
        );
        diagnostics.extend(lint::placement_game_checks(manifest, &shipment.root, g));
    }
    // The shader kinds read the original stores from `--original-data` and the game's VT and R2VB
    // pairs from its `data` folder, so both are required, never skipped.
    let shader_kind = manifest
        .contributions
        .iter()
        .enumerate()
        .find(|(_, c)| matches!(c, Contribution::AddShader { .. } | Contribution::ReplaceShader { .. }));
    if let Some((index, c)) = shader_kind {
        let Some(original) = original_data else {
            return Err(BuildError::OriginalDataRequired { index, kind: c.kind() });
        };
        let Some(g) = game.as_deref() else {
            return Err(BuildError::GameRequired { index, kind: c.kind() });
        };
        let data = crate::shader::game_data_dir(g).map_err(BuildError::ShaderData)?;
        diagnostics.extend(
            lint::shader_game_checks(manifest, &shipment.root, &data, original).map_err(BuildError::ShaderData)?,
        );
    }
    let shader_keys = crate::shader::ShaderKeys::with(&crate::shader::added(&manifest.shipment.name, manifest));
    if lint::blocks_build(&diagnostics) {
        return Err(BuildError::Blocked(diagnostics));
    }
    // Reading a console bake is fine and supported; EMITTING for one is not. Checked before any
    // lowering so the failure names the real reason rather than surfacing as a texture-encode error.
    if game
        .as_deref()
        .is_some_and(|g| g.platform() != Platform::Pc)
    {
        return Err(BuildError::ConsoleOutputUnsupported);
    }
    log.push(format!(
        "lint: {} finding(s), none blocking",
        diagnostics.len()
    ));

    // Superseded legacy files, before anything is lowered. The probe needs the game folder, so a
    // Shipment that declares `supersedes` cannot build without a game: the check is never skipped.
    if !manifest.supersedes.is_empty() {
        let name = &manifest.shipment.name;
        let Some(vz) = game.as_deref().and_then(|g| g.paths().first().map(|p| p.to_path_buf()))
        else {
            return Err(BuildError::Compat(crate::compat::CompatError::GameRootRequired {
                id: name.clone(),
            }));
        };
        let root = crate::compat::game_root_of(&vz)
            .map_err(|message| BuildError::Compat(crate::compat::CompatError::GameRoot { message }))?;
        for entry in &manifest.supersedes {
            let present = crate::compat::superseded_present(&root, entry).map_err(|message| {
                BuildError::Compat(crate::compat::CompatError::Probe {
                    id: name.clone(),
                    message,
                })
            })?;
            if present {
                return Err(BuildError::Superseded {
                    shipment: name.clone(),
                    relative: place_path(entry.dest.relative_dir(), &entry.file),
                });
            }
        }
        log.push(format!(
            "supersedes: none of {} legacy file(s) is in the game folder",
            manifest.supersedes.len()
        ));
    }

    // NOTE: self-conflicts are NOT re-checked here. `lint` already reports them as blocking M0120
    // findings, so a Shipment that claims one target twice never reaches this point. A second check
    // would be a redundant path with different formatting — and the first version of it returned on
    // the first conflict, hiding the rest.

    let mut blocks = Vec::new();
    // add_movie / add_ui blocks of `shell.wad` movies: they join the shell patch below.
    let mut shell_movies: Vec<PatchBlock> = Vec::new();
    let mut chains: Vec<Vec<PatchBlock>> = Vec::new();
    let mut files = Vec::new();
    let mut lang_wads: Vec<(String, String, Vec<PatchBlock>, (String, String))> = Vec::new();
    let mut level_wads: Vec<(String, String, crate::level::Level)> = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        match lower(
            index,
            c,
            &shipment.root,
            game.as_deref_mut(),
            names,
            &manifest.shipment.name,
            manifest,
            &mut log,
        )? {
            Lowering::Nothing => {}
            Lowering::Block(b) => blocks.push(b),
            Lowering::Chain(bs) => chains.push(bs),
            Lowering::Blocks(bs) => blocks.extend(bs),
            Lowering::Movie(b, route) => {
                use crate::retail_movies::MovieRoute;
                match route {
                    MovieRoute::Overlay => blocks.push(b),
                    MovieRoute::Shell => shell_movies.push(b),
                    MovieRoute::ShellAndOverlay => {
                        shell_movies.push(b.clone());
                        blocks.push(b);
                    }
                }
            }
            Lowering::LanguageWad {
                language,
                display,
                blocks: bs,
                stream_copy,
            } => lang_wads.push((language, display, bs, stream_copy)),
            Lowering::LevelWad { name, display, level } => level_wads.push((name, display, level)),
            Lowering::File {
                name,
                relative,
                bytes,
            } => files.push((name, relative, bytes)),
        }
    }
    // World: each add_tiny_geometry's model, and one overlay per layer block carrying every write
    // this Shipment makes to it — edit_world, add_placement, add_layer and the stand-ins'
    // placements, in contribution order.
    if let Some((index, c)) = manifest.contributions.iter().enumerate().find(|(_, c)| {
        matches!(
            c,
            Contribution::EditWorld { .. }
                | Contribution::AddPlacement { .. }
                | Contribution::AddLayer { .. }
                | Contribution::AddTinyGeometry { .. }
        )
    }) {
        let Some(game) = game.as_deref_mut() else {
            return Err(BuildError::GameRequired { index, kind: c.kind() });
        };
        let (models, stand_ins) = crate::tiny::lower_shipment(manifest, &shipment.root, game, &mut log)
            .map_err(|(index, message)| BuildError::Lower { index, kind: "add_tiny_geometry", message })?;
        blocks.extend(models);
        blocks.extend(lower_world(manifest, &shipment.root, game, stand_ins, &mut log)?);
    }
    // String tables: every edit_stringdb / add_stringdb_keys / replace_stringdb_text of this
    // Shipment, per table, in contribution order — the same code `qm link` merges a set with. The
    // gameplay copies go in the overlay, the front end's (shell.wad's) in the shell patch.
    let mut shell_tables: Vec<PatchBlock> = Vec::new();
    if let Some((index, c)) = manifest.contributions.iter().enumerate().find(|(_, c)| {
        matches!(
            c,
            Contribution::EditStringDb { .. }
                | Contribution::AddStringDbKeys { .. }
                | Contribution::ReplaceStringDbText { .. }
        )
    }) {
        let Some(game) = game.as_deref_mut() else {
            return Err(BuildError::GameRequired {
                index,
                kind: c.kind(),
            });
        };
        let tables = merge_string_tables(&[shipment], game, StringMerge::Strict, &mut log)?;
        blocks.extend(tables.overlay);
        shell_tables = tables.shell.into_iter().map(|(_, _, b)| b).collect();
    }
    // Sound: every add_sound, replace_sound_bank and replace_sound_cue of this Shipment
    // (`sound::lower_shipment_sound`). The blocks go to the level of each session that loads them —
    // the overlay for gameplay, the shell patch for the front end — and a language's `vo_*` banks to
    // its patch; each session's loader loads the registrations.
    let lowered = crate::sound::lower_shipment_sound(shipment, game.as_deref_mut(), &mut log)
        .map_err(|(index, kind, message)| BuildError::Lower { index, kind, message })?;
    let engine_banks = lowered.engine_banks;
    blocks.extend(lowered.overlay);
    let mut shell_blocks: Vec<PatchBlock> = lowered.shell;
    shell_blocks.extend(shell_movies);
    shell_blocks.extend(shell_tables);
    let language_blocks = lowered.language;
    let sound_regs = lowered.registrations;
    let gameplay_sounds = sound_regs.iter().any(|r| r.sessions.contains(&crate::manifest::LoadSession::Gameplay));
    let mutations = script_mutations(manifest, &shipment.root, crate::level::VZ)?;
    let ui_regs = ui_registrations(manifest);
    let layer_regs = layer_registrations(manifest);
    let support_regs = support_registrations(manifest, &shipment.root)?;
    let additions = script_additions(manifest, &shipment.root)?;
    let replacements = script_replacements(manifest, &shipment.root)?;
    let outfits = added_outfits(manifest);

    // ── Link the Script layer ──────────────────────────────────────────────────────────────────
    //
    // Linking this Shipment's own mutations produces a `scripts_vz` that is correct for a SOLO
    // install, which is what keeps each overlay valid standalone and verify-by-hash meaningful.
    //
    // ⚠ It is NOT the whole story. When several script-touching Shipments are installed together,
    // the deploy step must re-link all of their mutations into ONE block — otherwise the last WAD
    // mounted wins and the others' Lua disappears, which is the failure the linker exists to
    // prevent. That cross-Shipment relink belongs to deploy (Modkit), and this is deliberately only
    // its single-Shipment case.
    //
    // `ui_regs` / `layer_regs` / `additions` count too: an add_ui / activate_layer / add_script with
    // no other script edit still needs the linker to run (respectively: mints `qm_modloader` and
    // splices the hooks that load it, the same, mints a fresh scripts_vz entry). So a non-empty of any
    // of them must trigger the link even when `mutations` is empty. So must an add_outfit, whose
    // wardrobe rows and character map the link generates. A bank or a library the front end
    // loads links into `shell.wad`'s scripts block (below).
    //
    // `add_fx_sprite`, `add_fx`, `replace_fx` and a `replace_texture` of the `vfx` atlas share the
    // load: a template, a sprite's record and the atlas go into the resident block, and the block is
    // emitted once, with every script, template, record and the atlas.
    let touches_scripts = !mutations.is_empty()
        || !ui_regs.is_empty()
        || !layer_regs.is_empty()
        || !support_regs.is_empty()
        || gameplay_sounds
        || !additions.is_empty()
        || !replacements.is_empty()
        || !outfits.is_empty();
    let fx_kind = manifest.contributions.iter().enumerate().find(|(_, c)| {
        matches!(c, Contribution::AddFx { .. } | Contribution::ReplaceFx { .. } | Contribution::AddFxSprite { .. })
            || crate::fx::repaints_atlas(c)
            || matches!(c, Contribution::LevelAsset { level, templates, .. } if level == crate::level::VZ && !templates.is_empty())
    });
    if touches_scripts || fx_kind.is_some() {
        let Some(game) = game.as_deref_mut() else {
            return Err(match fx_kind {
                Some((index, c)) => BuildError::GameRequired { index, kind: c.kind() },
                None => BuildError::GameRequired { index: 0, kind: "patch_lua" },
            });
        };
        let mut loaded = load_script_blocks(game, link::SCRIPT_BLOCKS, "patch_lua")?;
        let mut touched = std::collections::BTreeSet::new();
        if touches_scripts {
            let Some(corpus) = corpus_root else {
                return Err(BuildError::Lower {
                    index: 0,
                    kind: "patch_lua",
                    message:
                        "linking Lua needs the decompiled corpus (the base source to append to). It \
                              ships in the reference bundle as workshop_data/lua; for `qm`, pass \
                              --corpus <dir> or --workshop-data <dir> (env MERCS2_WORKSHOP_DATA)."
                            .into(),
                });
            };
            check_spawn_lists(manifest, corpus, "")?;
            let mut targets: Vec<link::TargetBlock<'_>> = loaded
                .iter_mut()
                .map(|lb| link::TargetBlock {
                    path: lb.path.clone(),
                    block: &mut lb.block,
                })
                .collect();
            // A single Shipment has nothing to order against, so the resolved order is itself.
            // Cross-Shipment order is `link_installed`'s job.
            let solo_order = [manifest.shipment.name.clone()];
            let linked = link::link_into_level_blocks(
                &mut targets,
                corpus,
                &mutations,
                &ui_regs,
                &layer_regs,
                &support_regs,
                &sound_regs,
                &additions,
                &replacements,
                &solo_order,
                &std::collections::BTreeSet::new(),
                link::QmHooks::ForLibraries,
                None,
                crate::outfits::DEFAULT_LEVEL,
                &outfits,
            )
            .map_err(|e| BuildError::Lower {
                index: 0,
                kind: "patch_lua",
                message: e.to_string(),
            })?;
            drop(targets);
            solo_import_findings(&linked.findings, manifest, "patch_lua", &mut log)?;
            for l in &linked.scripts {
                log.push(format!(
                    "linked {} in {}: {} → {} B source, {} B bytecode, from {:?}",
                    l.target,
                    loaded[l.block].path,
                    l.base_source_bytes,
                    l.linked_source_bytes,
                    l.bytecode_bytes,
                    l.contributors
                ));
                touched.insert(l.block);
            }
        }
        if let Some((index, c)) = fx_kind {
            let (effects, resident) =
                lower_fx(&[shipment], game, &mut loaded, crate::fx::Scope::Build, FxLevel::Main, c.kind(), &mut log)
                    .map_err(|e| match e {
                        BuildError::Lower { index: 0, kind, message } => BuildError::Lower { index, kind, message },
                        other => other,
                    })?;
            blocks.extend(effects);
            touched.extend(resident);
        }
        // Say what shipping a scripts block COSTS, because the number is not obvious and the
        // failure surfaces far away.
        //
        // A scripts block is claimed whole: the overlay carries every script asset in it, not just
        // the one that was patched. So a second installed mod that also touches scripts overlaps on
        // all of them, and neither contains the other — Modkit reports it as a half-applied mod,
        // and without that check it is the silent mutual annihilation the linter exists for.
        // `qm link` across the installed set is the fix; a standalone overlay cannot be.
        let script_blocks = script_patch_blocks(&loaded, &touched, "patch_lua")?;
        let carried: usize = script_blocks.iter().map(|b| b.aset_entries.len()).sum();
        if carried > 1 {
            log.push(format!(
                "scripts: this overlay claims {carried} script asset(s) — the whole block, not just \
                 the patched script. It is valid on its own, and cannot be installed beside another \
                 script-touching mod until `qm link` rebuilds them together."
            ));
        }
        blocks.extend(script_blocks);
    }

    // The front end's hooks and loader, linked into `shell.wad`'s scripts block and shipped in the shell
    // patch. Nothing is recompiled from the corpus: qm's code is spliced onto the retail bytecode.
    if has_front_end(&sound_regs, &additions) {
        let Some(game) = game.as_deref() else {
            return Err(BuildError::GameRequired { index: 0, kind: "front-end loader" });
        };
        let solo_order = [manifest.shipment.name.clone()];
        // A standalone build writes no load plan: the deploy link records the set's libraries.
        // The Extras page lists the levels of the installed set: `qm link`'s, never one Shipment's build.
        let (front_end, _modules, findings) = link_shell_loader(
            game,
            FrontEndSet { sound_regs: &sound_regs, levels: &[], items: &[], additions: &additions },
            &solo_order,
            link::QmHooks::ForLibraries,
            "front-end loader",
            &mut log,
        )?;
        solo_import_findings(&findings, manifest, "front-end loader", &mut log)?;
        shell_blocks.extend(front_end);
    }
    check_sound_loaders(
        &sound_regs,
        &blocks.iter().collect::<Vec<_>>(),
        &shell_blocks.iter().collect::<Vec<_>>(),
        "sound loader",
    )?;
    // Every bank the engine loads ships its three tables in one block of the overlay.
    crate::sound::check_engine_banks(&engine_banks, &blocks.iter().collect::<Vec<_>>())
        .map_err(|message| BuildError::Lower { index: 0, kind: "sound", message })?;

    // Model chains, each numbered after every block collected before it, earlier chains included. Two
    // models whose finer rungs share a retail block would otherwise claim one source index, and
    // `build_patch_wad_multi`'s source → patch map would point one model's rung at the other's block,
    // which does not hold it: the engine's rung gate spins on that, a stream hang.
    for mut chain in chains {
        mercs2_formats::patch_wad::rebase_after(&mut chain, &blocks)
            .map_err(|message| BuildError::Lower { index: 0, kind: "assemble", message })?;
        blocks.extend(chain);
    }

    // `level_asset` rows of the main campaign: their blocks, numbered after every other block of the
    // overlay (`patch_wad::rebase_after`).
    let vz_assets = crate::level::assets_of(manifest, crate::level::VZ);
    if !vz_assets.is_empty() {
        let index = manifest
            .contributions
            .iter()
            .position(|c| matches!(c, Contribution::LevelAsset { level, templates, .. } if level == crate::level::VZ && templates.is_empty()))
            .expect("assets_of found a level: vz row");
        let fail = |message: String| BuildError::Lower { index, kind: "level_asset", message };
        let Some(g) = game.as_deref() else {
            return Err(BuildError::GameRequired { index, kind: "level_asset" });
        };
        let opened = crate::sound::opened_streams(manifest);
        let (mut vz_blocks, joined) = crate::level::compile_vz(&shipment.root, &vz_assets, g, &opened).map_err(fail)?;
        for j in &joined {
            log.push(format!("level: vz: {j} is vz.wad's block, so it ships only through `qm link`, in the link's copy of it"));
        }
        mercs2_formats::patch_wad::rebase_after(&mut vz_blocks, &blocks).map_err(fail)?;
        log.push(format!(
            "level: vz: {} asset(s) in {} block(s), after the overlay's {} other block(s)",
            vz_assets.len(),
            vz_blocks.len(),
            blocks.len()
        ));
        blocks.extend(vz_blocks);
    }

    // Mirror the base WAD's CSUM value into the overlay, as the proven publish path does. The row's
    // meta is the writer's to compute from the overlay's own blocks (`patch_wad::csum_meta_for`).
    let csum = match game
        .as_deref()
        .and_then(|g| g.paths().first().map(|p| p.to_path_buf()))
    {
        Some(base) => mercs2_formats::donor::base_csum(&base).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "assemble",
            message: m,
        })?,
        None => 0,
    };

    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| shipment.root.join("_build"));
    std::fs::create_dir_all(&out_dir).map_err(|e| BuildError::Io {
        path: out_dir.clone(),
        message: e.to_string(),
    })?;

    let mut placements = Vec::new();
    let mut wad_path = None;
    if !blocks.is_empty() {
        let wad = build_patch_wad_multi(&blocks, csum, &FFCS_CERT_BLOB).map_err(|m| {
            BuildError::Lower {
                index: 0,
                kind: "assemble",
                message: m,
            }
        })?;
        // Self-check BEFORE writing: a WAD that would hang the game should not reach the disk at
        // all, where a later step could mistake its presence for success.
        let found = verify_emitted(&wad, &shader_keys)?;
        for d in &found {
            log.push(format!("self-check: {d}"));
        }
        diagnostics.extend(found);

        let name = format!("{}.wad", manifest.shipment.name);
        let path = out_dir.join(&name);
        std::fs::write(&path, &wad).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let digest = sha256_hex(&wad);
        std::fs::write(
            out_dir.join(format!("{name}.sha256")),
            format!("{digest}  {name}\n"),
        )
        .map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        log.push(format!(
            "wrote {name}: {} bytes, sha256 {digest}",
            wad.len()
        ));
        placements.push(Placement {
            name,
            bytes: wad.len(),
            sha256: digest,
            destination: Destination::Overlay,
        });
        wad_path = Some(path);
    }

    // Language WADs. A NEW base WAD per `add_language`, placed in `data/` — NOT the Shipment overlay,
    // because the engine opens it by name. Assembled with the same machinery, self-checked before it
    // reaches disk, and recorded as a `Destination::DataWad` so deploy places it in `data/` — the one
    // place a Shipment writes a WAD, earned only by the collision-checked name (`language_name_refusal`).
    for (language, display, lang_blocks, (from, to)) in lang_wads {
        let wad =
            build_patch_wad_multi(&lang_blocks, csum, &FFCS_CERT_BLOB).map_err(|m| {
                BuildError::Lower {
                    index: 0,
                    kind: "add_language",
                    message: m,
                }
            })?;
        let found = verify_emitted(&wad, &shader_keys)?;
        for d in &found {
            log.push(format!("self-check ({language}.wad): {d}"));
        }
        diagnostics.extend(found);

        let relative = format!("data/{language}.wad");
        let path = out_dir.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BuildError::Io {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }
        std::fs::write(&path, &wad).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let digest = sha256_hex(&wad);
        log.push(format!(
            "wrote {relative}: {} bytes, sha256 {digest} → new base language WAD",
            wad.len()
        ));
        placements.push(Placement {
            name: format!("{language}.wad"),
            bytes: wad.len(),
            sha256: digest,
            destination: Destination::DataWad { relative, display },
        });

        // The voice stream the language plays its voice-over from: a copy of English's, made at
        // deploy. Its record carries the digest of the source as read here.
        let vz = game
            .as_deref()
            .and_then(|g| g.paths().first().map(|p| p.to_path_buf()))
            .ok_or(BuildError::GameRequired { index: 0, kind: "add_language" })?;
        let game_root = crate::compat::game_root_of(&vz)
            .map_err(|message| BuildError::Compat(crate::compat::CompatError::GameRoot { message }))?;
        let source = game_root.join(&from);
        let (bytes, sha256) = sha256_file(&source)?;
        log.push(format!(
            "{to} ← copy of {from} at deploy: {bytes} bytes, sha256 {sha256}"
        ));
        placements.push(Placement {
            name: to.rsplit('/').next().unwrap_or(&to).to_string(),
            bytes,
            sha256,
            destination: Destination::StreamCopy { from, to },
        });
    }

    // Levels. The level's own WAD is the patch slot above a copy of `vz.wad`, so it carries
    // `vz.wad`'s CSUM row, as every overlay does. Its streams go to the level's own folder
    // (`level::stream_relative`), and the output mirrors the tree a deploy copies into.
    for (name, display, level) in level_wads {
        let fail = |message: String| BuildError::Lower { index: 0, kind: "add_level", message };
        let wad = build_patch_wad_multi(&level.blocks, csum, &FFCS_CERT_BLOB).map_err(fail)?;
        let found = verify_emitted(&wad, &shader_keys)?;
        for d in &found {
            log.push(format!("self-check ({name}-patch.wad): {d}"));
        }
        diagnostics.extend(found);
        let write = |relative: &str, bytes: &[u8]| -> Result<(), BuildError> {
            let path = out_dir.join(relative);
            let io = |e: std::io::Error, path: &Path| BuildError::Io { path: path.to_path_buf(), message: e.to_string() };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
            }
            std::fs::write(&path, bytes).map_err(|e| io(e, &path))
        };
        let relative = crate::level::patch_relative(&name);
        write(&relative, &wad)?;
        let digest = sha256_hex(&wad);
        log.push(format!("wrote {relative}: {} bytes, sha256 {digest} → the level above a copy of vz.wad", wad.len()));
        placements.push(Placement {
            name: format!("{name}-patch.wad"),
            bytes: wad.len(),
            sha256: digest,
            destination: Destination::LevelPatch { level: name.clone(), relative, display },
        });

        let vz = game
            .as_deref()
            .and_then(|g| g.paths().first().map(|p| p.to_path_buf()))
            .ok_or(BuildError::GameRequired { index: 0, kind: "add_level" })?;
        let (bytes, sha256) = sha256_file(&vz)?;
        let to = crate::level::base_relative(&name);
        log.push(format!("{to} ← copy of {} at deploy: {bytes} bytes, sha256 {sha256}", crate::level::BASE_FROM));
        placements.push(Placement {
            name: format!("{name}.wad"),
            bytes,
            sha256,
            destination: Destination::StreamCopy { from: crate::level::BASE_FROM.to_string(), to },
        });

        for (file, stream) in &level.streams {
            let relative = crate::level::stream_relative(&name, file);
            write(&relative, stream)?;
            let digest = sha256_hex(stream);
            log.push(format!("wrote {relative}: {} bytes, sha256 {digest}", stream.len()));
            placements.push(Placement {
                name: file.clone(),
                bytes: stream.len(),
                sha256: digest,
                destination: Destination::GameFolder { relative },
            });
        }
    }

    // The shell patch and the language patches: patch WADs a deploy step merges into
    // `data/shell-patch.wad` and `data/<language>-patch.wad`. The shell patch mounts above
    // `shell.wad`, so it carries `shell.wad`'s CSUM row.
    if !shell_blocks.is_empty() {
        let name = format!("{}.shell-patch.wad", manifest.shipment.name);
        let game = game.as_deref().ok_or(BuildError::GameRequired { index: 0, kind: "shell patch" })?;
        placements.push(write_patch_wad(
            &out_dir,
            &name,
            &shell_blocks,
            shell_csum(game, "shell patch")?,
            Destination::ShellPatch,
            &shader_keys,
            &mut log,
            &mut diagnostics,
        )?);
    }
    for (language, lblocks) in language_blocks {
        let relative = format!("language_patch/{}.wad", language.token());
        placements.push(write_patch_wad(
            &out_dir,
            &relative,
            &lblocks,
            csum,
            Destination::LanguagePatch {
                language: language.token().to_string(),
                relative: relative.clone(),
            },
            &shader_keys,
            &mut log,
            &mut diagnostics,
        )?);
    }

    // Code-layer artifacts. The build directory MIRRORS the tree these will be copied into, so
    // `destination.relative` names the file both here and in the game folder and a deploy step can
    // copy the tree wholesale. Writing them flat was fine while the only destination was `scripts/`
    // and stopped being fine the moment there were seven: two placements differing only in
    // destination — `scripts/OnBoot/init.lua` and `scripts/OnLoad/init.lua` — are not a conflict,
    // they are two files, and flattening them would have one silently overwrite the other in the
    // output while both records claimed the same digest.
    //
    // The digest is taken from the bytes that were WRITTEN, read back off the disk, rather than
    // from the buffer we happen to hold: the record's whole job is to describe what is actually
    // there, and a digest of the intended bytes would still verify after a truncated write.
    for (name, relative, bytes) in files {
        let path = out_dir.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BuildError::Io {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }
        std::fs::write(&path, &bytes).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let written = std::fs::read(&path).map_err(|e| BuildError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let digest = sha256_hex(&written);
        log.push(format!(
            "wrote {name}: {} bytes, sha256 {digest} → place at {relative}",
            written.len()
        ));
        placements.push(Placement {
            name,
            bytes: written.len(),
            sha256: digest,
            destination: Destination::GameFolder { relative },
        });
    }

    // The shader stores: the originals with this Shipment's edits applied in contribution order.
    if let Some(original) = original_data.filter(|_| crate::shader::has_shader_kinds(manifest)) {
        let game = game.as_deref().ok_or(BuildError::GameRequired { index: 0, kind: "shader stores" })?;
        let edits = crate::shader::shipment_edits(&manifest.shipment.name, manifest, &shipment.root).map_err(|f| {
            let first = &f[0];
            BuildError::Lower { index: first.index, kind: "shader", message: first.message.clone() }
        })?;
        placements.extend(write_shader_stores(original, game, &edits, &out_dir, &mut log)?);
    }
    if let Some(header) = crate::shader::header(&manifest.shipment.name, manifest) {
        let name = format!("{}.shaders.h", manifest.shipment.name);
        let path = out_dir.join(&name);
        std::fs::write(&path, header).map_err(|e| BuildError::Io { path: path.clone(), message: e.to_string() })?;
        log.push(format!("wrote {name}: the shader registration tables for the Shipment's ASI"));
    }

    // The placement record: what goes where, each with its digest. Deploy/undo consumes this — a
    // file drop cannot be backed out without it.
    write_placement_record(&out_dir, &placements)?;

    let log_text = log.join("\n") + "\n";
    std::fs::write(out_dir.join("build.log"), &log_text).map_err(|e| BuildError::Io {
        path: out_dir.join("build.log"),
        message: e.to_string(),
    })?;

    Ok(BuildReport {
        diagnostics,
        wad: wad_path,
        placements,
        log,
    })
}

/// `add_model`'s shader import ([`crate::shader_import`]): resolve each host group's vertex shaders
/// and its materials' pixel shaders from the retail convention or the glTF's `extras`, and write
/// them into `block`.
fn import_shaders(
    index: usize,
    kind: &'static str,
    name: &str,
    model: &Path,
    block: &mut [u8],
    hosts: &[usize],
    geometry: crate::shader_import::Geometry,
    manifest: &crate::manifest::Manifest,
    root: &Path,
    log: &mut Vec<String>,
) -> Result<(), BuildError> {
    let lower = |message: String| BuildError::Lower { index, kind, message };
    let declared = crate::shader_import::read_declared(model).map_err(lower)?;
    let added = crate::shader::added(&manifest.shipment.name, manifest);
    let added_vertex = crate::shader_import::added_vertex_shaders(manifest, root).map_err(lower)?;
    for &host in hosts {
        let done = crate::shader_import::import_into_block(block, host, &declared, &added, &added_vertex, geometry)
            .map_err(|e| lower(format!("[{}] {name}: {}", e.code, e.message)))?;
        let w = match done.position_w {
            crate::shader_import::PositionW::One => "1".to_string(),
            crate::shader_import::PositionW::Sway => "the _SWAY_WEIGHT of each vertex".to_string(),
            crate::shader_import::PositionW::TinySlot => "the _TINY_SLOT of each vertex".to_string(),
        };
        log.push(format!(
            "contributions[{index}] {kind} {name}: group {} vertex shader {}, shadow {}, pixel {}, POSITION.w {w}",
            done.group,
            done.vertex,
            done.shadow,
            done.pixels.iter().map(|(m, p)| format!("material {m} {p}")).collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(())
}

/// Apply `edits` to the original stores in `original` and write both under `out_dir/data/`, each
/// recorded as a [`Destination::DataFile`] naming the original's sha256.
fn write_shader_stores(
    original: &Path,
    game: &GameStack,
    edits: &[crate::shader::Edit],
    out_dir: &Path,
    log: &mut Vec<String>,
) -> Result<Vec<Placement>, BuildError> {
    let originals = crate::shader::read_originals(original).map_err(BuildError::ShaderData)?;
    let data = crate::shader::game_data_dir(game).map_err(BuildError::ShaderData)?;
    let extra = crate::shader::read_extra_pairs(&data).map_err(BuildError::ShaderData)?;
    let stores = crate::shader::apply_edits(&originals, &extra, edits).map_err(|e| match e.at {
        Some((_, index)) => BuildError::Lower { index, kind: "shader", message: format!("[{}] {}", e.code, e.message) },
        None => BuildError::ShaderData(e.to_string()),
    })?;
    let mut out = Vec::new();
    for (file, bytes) in stores {
        let path = out_dir.join(file.relative());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BuildError::Io { path: parent.to_path_buf(), message: e.to_string() })?;
        }
        std::fs::write(&path, &bytes).map_err(|e| BuildError::Io { path: path.clone(), message: e.to_string() })?;
        let (written, digest) = sha256_file(&path)?;
        let base = originals.sha256[&file].clone();
        let n = edits.iter().filter(|e| e.file == file).count();
        log.push(format!(
            "wrote {}: {written} bytes, sha256 {digest}, {n} edit(s) over the original (sha256 {base})",
            file.relative()
        ));
        out.push(Placement {
            name: file.file_name().to_string(),
            bytes: written,
            sha256: digest,
            destination: Destination::DataFile { relative: file, base_sha256: base },
        });
    }
    Ok(out)
}

/// The size and sha256 of the file at `path`, read in chunks.
fn sha256_file(path: &Path) -> Result<(usize, String), BuildError> {
    use std::io::Read;
    let io = |e: std::io::Error| BuildError::Io { path: path.to_path_buf(), message: e.to_string() };
    let mut f = std::fs::File::open(path).map_err(io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0usize;
    loop {
        let n = f.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n;
    }
    Ok((total, hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()))
}

/// Assemble `blocks` as a patch WAD at `relative` under `out_dir`, self-check it before it reaches
/// disk ([`verify_emitted`]), and record it with its digest under `destination`.
fn write_patch_wad(
    out_dir: &Path,
    relative: &str,
    blocks: &[PatchBlock],
    csum: u32,
    destination: Destination,
    keys: &crate::shader::ShaderKeys,
    log: &mut Vec<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Placement, BuildError> {
    let wad = build_patch_wad_multi(blocks, csum, &FFCS_CERT_BLOB).map_err(|m| {
        BuildError::Lower {
            index: 0,
            kind: "assemble",
            message: format!("{relative}: {m}"),
        }
    })?;
    let found = verify_emitted(&wad, keys)?;
    for d in &found {
        log.push(format!("self-check ({relative}): {d}"));
    }
    diagnostics.extend(found);
    let path = out_dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| BuildError::Io {
            path: parent.to_path_buf(),
            message: e.to_string(),
        })?;
    }
    std::fs::write(&path, &wad).map_err(|e| BuildError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    let digest = sha256_hex(&wad);
    log.push(format!("wrote {relative}: {} bytes, sha256 {digest}", wad.len()));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| BuildError::Io {
            path: path.clone(),
            message: "the output path has no file name".into(),
        })?;
    Ok(Placement {
        name,
        bytes: wad.len(),
        sha256: digest,
        destination,
    })
}

/// The PTHS path of the single-entry block a string table ships in: `blocks\VZ\mod_<hash>.block`.
///
/// One function for both producers of one — a Shipment's own build and the link, both through
/// [`merge_string_tables`] — so the link's merged table shadows the per-Shipment copies at exactly
/// their path.
pub fn stringdb_block_path(table: u32) -> String {
    format!("blocks\\VZ\\mod_{table:08x}.block")
}

/// Wrap a string-table container as its own single-entry block: `[count][name][type][field_c]
/// [size][container]`, one PRIMARY stringdb ASET row. A string table has no LOD chain, so the low 16
/// bits carry the sentinel — anything else would dangle (M0001).
fn stringdb_block(table: u32, container: &[u8]) -> Result<PatchBlock, String> {
    let mut block_data = Vec::with_capacity(20 + container.len());
    block_data.extend_from_slice(&1u32.to_le_bytes()); // entry count
    block_data.extend_from_slice(&table.to_le_bytes());
    block_data.extend_from_slice(&TYPE_HASH_STRINGDB.to_le_bytes());
    block_data.extend_from_slice(&0u32.to_le_bytes());
    block_data.extend_from_slice(&(container.len() as u32).to_le_bytes());
    block_data.extend_from_slice(container);
    let aset = AsetEntry::new(table, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_STRINGDB);
    PatchBlock::from_decompressed(&block_data, stringdb_block_path(table), vec![aset], None)
}

/// Every string table `qm link` merges across a set: the targets of every `edit_stringdb`,
/// `add_stringdb_keys` and `replace_stringdb_text` in it, by asset hash.
pub fn merged_string_tables<'a>(
    manifests: impl IntoIterator<Item = &'a crate::manifest::Manifest>,
) -> std::collections::BTreeSet<u32> {
    manifests
        .into_iter()
        .flat_map(|m| m.contributions.iter())
        .filter_map(|c| match c {
            Contribution::EditStringDb { target, .. }
            | Contribution::AddStringDbKeys { target, .. }
            | Contribution::ReplaceStringDbText { target, .. } => {
                Some(crate::manifest::asset_hash(target))
            }
            _ => None,
        })
        .collect()
}

/// Every block `qm link` re-emits for a set, as the load plan's `link_block_paths` states it: the
/// `vz.wad` scripts blocks ([`link::SCRIPT_BLOCKS`]), the `shell.wad` scripts block
/// ([`link::SHELL_SCRIPT_BLOCKS`]), then the blocks those carry as LOD rungs ([`linked_rung_paths`]),
/// then each merged string table's block in hash order, then each
/// merged sound bank's block in entry-hash order ([`crate::sound::linked_sound_entries`]; one path
/// for the bank in every WAD that carries it; for a bank the engine loads, the block carries its
/// sounddb and wavebank too), then — when any Shipment has an `add_fx` or a `replace_fx` — the
/// effects block ([`crate::fx::EFFECTS_BLOCK`]), then — when any Shipment has an `add_level` or an
/// `add_menu_item` — the block of qm's edited SHELL ([`crate::shell_extras`], [`crate::menu_items`]).
/// A deploy step drops the per-Shipment copies of exactly these blocks, because the link WAD carries
/// the set-wide version of each. The rungs, and the banks a `replace_sound_wave` reaches, are read
/// from `game`.
pub fn link_block_paths<'a>(
    manifests: impl IntoIterator<Item = &'a crate::manifest::Manifest>,
    game: &mut GameStack,
) -> Result<Vec<String>, String> {
    let manifests: Vec<&crate::manifest::Manifest> = manifests.into_iter().collect();
    let fx = manifests.iter().any(|m| crate::fx::has_fx(m));
    let rungs = linked_rung_paths(game)?;
    let sound = crate::sound::linked_sound_entries(manifests.iter().copied(), Some(game))?;
    Ok(link::SCRIPT_BLOCKS
        .iter()
        .chain(link::SHELL_SCRIPT_BLOCKS)
        .map(|(_, p)| p.to_string())
        .chain(rungs)
        .chain(merged_string_tables(manifests.iter().copied()).into_iter().map(stringdb_block_path))
        .chain(sound.into_iter().map(crate::sound::block_path))
        .chain(fx.then(|| crate::fx::EFFECTS_BLOCK.1.to_string()))
        .chain(
            manifests
                .iter()
                .any(|m| has_level(m) || has_menu_item(m))
                .then(|| movie_block_path(crate::manifest::asset_hash(crate::shell_extras::SHELL))),
        )
        .collect())
}

/// Whether `manifest` adds a level.
fn has_level(manifest: &crate::manifest::Manifest) -> bool {
    manifest.contributions.iter().any(|c| matches!(c, Contribution::AddLevel { .. }))
}

/// Whether `manifest` adds a menu item.
fn has_menu_item(manifest: &crate::manifest::Manifest) -> bool {
    manifest.contributions.iter().any(|c| matches!(c, Contribution::AddMenuItem { .. }))
}

/// One `add_menu_item` of a set: its Shipment and contribution index, and its fields.
struct SetItem<'a> {
    shipment: &'a str,
    index: usize,
    menu: &'a str,
    label: &'a str,
    after: &'a str,
    help: Option<&'a str>,
    action: &'a str,
}

/// Every `add_menu_item` of `shipments`, in their order, then each one's contribution order.
fn menu_items_of<'a>(shipments: &[&'a LoadedShipment]) -> Vec<SetItem<'a>> {
    let mut out = Vec::new();
    for s in shipments {
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            if let Contribution::AddMenuItem { menu, label, after, help, action } = c {
                out.push(SetItem {
                    shipment: &s.manifest.shipment.name,
                    index,
                    menu,
                    label,
                    after,
                    help: help.as_deref(),
                    action,
                });
            }
        }
    }
    out
}

/// Each item's `action` names a function of a library the set loads in the front end (an `add_script`
/// whose `load:` includes `front_end`): the dispatcher finds the module among the loaded ones by name,
/// ignoring case, as `QM.module` does.
fn check_item_actions(items: &[SetItem<'_>], additions: &[link::ScriptAddition]) -> Result<(), BuildError> {
    let fe = crate::manifest::LoadSession::FrontEnd;
    for i in items {
        let fail = |message: String| BuildError::Lower { index: i.index, kind: "add_menu_item", message: format!("{}: menu item {}: {message}", i.shipment, i.label) };
        let (module, function) = i.action.split_once('.').ok_or_else(|| fail(format!("action {:?} is not Module.Function", i.action)))?;
        if function.is_empty() {
            return Err(fail(format!("action {:?} names no function", i.action)));
        }
        let loaded = additions.iter().any(|a| a.name.eq_ignore_ascii_case(module) && a.load.as_ref().is_some_and(|l| l.sessions.contains(&fe)));
        if !loaded {
            return Err(fail(format!(
                "action {:?}: no add_script of the installed set named {module} loads in the front end (load: in: [front_end])",
                i.action
            )));
        }
    }
    Ok(())
}

/// `movie` (SHELL) with `items` added, page by page in the catalogue's order, each page's items in the
/// set's order; and each page's label art with its items.
fn compose_menu_items<'a, 'b>(
    items: &'b [SetItem<'a>],
    movie: &[u8],
) -> Result<(Vec<u8>, Vec<(crate::menu_items::LabelArt, Vec<&'b SetItem<'a>>)>), String> {
    let pages = crate::menu_items::pages()?;
    for i in items {
        if !pages.iter().any(|p| p.id == i.menu) {
            return Err(format!("{}: menu item {} is on {:?}, which is no page of qm's menu catalogue", i.shipment, i.label, i.menu));
        }
    }
    let mut out = movie.to_vec();
    let mut arts = Vec::new();
    for page in &pages {
        let on_page: Vec<&SetItem<'_>> = items.iter().filter(|i| i.menu == page.id).collect();
        if on_page.is_empty() {
            continue;
        }
        if page.movie != crate::shell_extras::SHELL {
            return Err(format!("menu page {} is in movie {}, and qm link composes items into SHELL only", page.id, page.movie));
        }
        if on_page.len() > page.capacity() {
            return Err(format!(
                "menu page {} takes {} added item(s), and the installed set adds {}: {}",
                page.id,
                page.capacity(),
                on_page.len(),
                on_page.iter().map(|i| format!("{} ({})", i.label, i.shipment)).collect::<Vec<_>>().join(", ")
            ));
        }
        let new: Vec<crate::menu_items::NewItem> = on_page
            .iter()
            .map(|i| crate::menu_items::NewItem {
                label: i.label.to_string(),
                help: i.help.map(str::to_string),
                event: i.label.to_string(),
                after: i.after.to_string(),
            })
            .collect();
        let (edited, art) = crate::menu_items::add_items(page, &out, &new)?;
        out = edited;
        arts.push((art, on_page));
    }
    Ok((out, arts))
}

/// Each page's label texture, set in every installed language (`<language>.wad` beside `vz.wad`): each
/// item's label text from that language's front-end string table (`shell_tables`, the link's merged
/// copy, else `shell.wad`'s own), retail's art for the grain and outline from `shell.wad` with the
/// language's WAD over it, split into letters by retail's own labels' text as `shell.wad` ships it. A
/// label or help key with no text in an installed language fails, every one named. Each texture goes in
/// the language's patch.
fn menu_label_textures(
    arts: &[(crate::menu_items::LabelArt, Vec<&SetItem<'_>>)],
    game: &GameStack,
    shell_tables: &[(u32, Vec<u8>)],
    shell: &mut GameStack,
    log: &mut Vec<String>,
) -> Result<Vec<(crate::manifest::Language, PatchBlock)>, String> {
    use crate::menu_items::{key_hash, label_texture, label_texture_block};
    let present = game.present_side_wads()?;
    let base = game.paths().first().map(|p| p.to_path_buf()).ok_or("the game stack is empty")?;
    let shell_path = crate::sound::sibling_wad(&base, "shell.wad")?;
    let mut missing: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for language in crate::manifest::Language::ALL {
        let file = format!("{}.wad", language.token());
        let Some(wad) = present.iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&file))) else {
            continue;
        };
        let table = crate::manifest::asset_hash(language.token());
        // Retail's own labels are read from shell.wad's table as shipped: the art was drawn from that
        // text, whatever a Shipment of the set writes over it. The added labels are read from the
        // table as the link merged it.
        let shipped = shell
            .container_for_asset(table, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)
            .ok_or_else(|| format!("{file} is installed, and shell.wad has no {} string table for the front end", language.token()))?;
        let merged = shell_tables.iter().find(|(t, _)| *t == table).map_or_else(|| shipped.clone(), |(_, c)| c.clone());
        let retail_db = mercs2_formats::stringdb::read_container(&shipped)?;
        let db = mercs2_formats::stringdb::read_container(&merged)?;
        let retail_text = |key: u32| retail_db.entries.iter().find(|e| e.key_hash == key).map(|e| e.text.clone());
        let text = |key: u32| db.entries.iter().find(|e| e.key_hash == key).map(|e| e.text.clone());
        let mut stack = GameStack::open(&[shell_path.clone(), wad.clone()]).map_err(|e| e.to_string())?;
        for (art, items) in arts {
            let mut texts = Vec::with_capacity(items.len());
            for i in items {
                for key in std::iter::once(i.label).chain(i.help) {
                    if text(key_hash(key)).is_none() {
                        missing.push(format!("{} contributions[{}]: {key} in {}", i.shipment, i.index, language.token()));
                    }
                }
                texts.push(text(key_hash(i.label)).unwrap_or_default());
            }
            if !missing.is_empty() {
                continue;
            }
            let retail = crate::label_art::read(&mut stack, &art.retail)?;
            let image = label_texture(art, &retail, &retail_text, &texts).map_err(|e| format!("{}: {e}", language.token()))?;
            log.push(format!("{} {}x{} in {}: {} label(s)", art.name, art.size[0], art.size[1], language.token(), texts.len()));
            out.push((language, label_texture_block(art, &image)?));
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "a menu item's label or help key has no text in an installed language's front-end string table: {}. Add it with add_stringdb_keys (target: the language)",
            missing.join("; ")
        ));
    }
    Ok(out)
}

/// The blocks `qm link` carries with the scripts blocks it re-emits, so their rows keep their LOD
/// chains ([`script_patch_blocks`], [`GameStack::rung_paths`]): the rungs of `game`'s
/// [`link::SCRIPT_BLOCKS`], then those of the [`link::SHELL_SCRIPT_BLOCKS`] of the `shell.wad`
/// beside it, by PTHS path, each once. A scripts block the stack does not have carries nothing, as
/// [`load_script_blocks`] skips it.
pub fn linked_rung_paths(game: &GameStack) -> Result<Vec<String>, String> {
    let shell = game.open_sibling("shell.wad")?;
    let mut out: Vec<String> = Vec::new();
    for (stack, blocks) in [(game, link::SCRIPT_BLOCKS), (&shell, link::SHELL_SCRIPT_BLOCKS)] {
        for (needle, _) in blocks {
            let Some(paths) = stack.rung_paths(needle) else {
                continue;
            };
            for p in paths? {
                if !out.iter().any(|o| o.eq_ignore_ascii_case(&p)) {
                    out.push(p);
                }
            }
        }
    }
    Ok(out)
}

/// How [`merge_string_tables`] treats key writes: the one difference between a Shipment's own
/// build and the set-wide link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringMerge {
    /// `qm build` of ONE Shipment: an `edit_stringdb` key must exist in the table as edited so far,
    /// an `add_stringdb_keys` key must not, and a file with no rows is refused. These are the rules
    /// each kind has always had, now checked in contribution order.
    Strict,
    /// `qm link` of the set: every key write is an upsert, the later winning, with
    /// no existence check — each Shipment's own build already applied [`StringMerge::Strict`].
    Upsert,
}

/// Which kind of key write a row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyWrite {
    Edit,
    Add,
}

/// One write to a string table, in order.
enum TableWrite {
    /// `edit_stringdb` / `add_stringdb_keys`: set the key's text. `label` is the key as written.
    Key {
        key: u32,
        label: String,
        text: String,
        write: KeyWrite,
        who: String,
    },
    /// `replace_stringdb_text`: every entry whose CURRENT text is exactly `old` — in the table as
    /// edited so far — gets `new`. Matching nothing is an error naming `who`.
    Text { old: String, new: String, who: String },
}

/// Apply string-table writes, in order, into one table per target. The ONE implementation of
/// string-table lowering: `qm build` calls it for a single Shipment ([`StringMerge::Strict`]) and
/// `qm link` for the whole set ([`StringMerge::Upsert`]).
///
/// Each `edit_stringdb`, `add_stringdb_keys` and `replace_stringdb_text` of each Shipment in
/// `shipments` (for the link, the plan's order), in contribution order, is applied to the base table
/// as edited so far:
///
/// * a key write goes by key hash ([`mercs2_formats::stringdb::key_hash`]); how a missing or an
///   existing key is treated is `mode`'s;
/// * a text replacement changes every entry whose current text is exactly `old`
///   ([`mercs2_formats::stringdb::StringDb::replace_exact_text`]), and matching nothing is an error
///   naming the Shipment, the table and the text, in both modes.
///
/// A later write wins. Each copy of a table is merged on its own: the gameplay one in `game` (shipped
/// in the overlay) and the front end's in the `shell.wad` beside it (shipped in the shell patch), each
/// present one, so a key reaches the front end as well as gameplay; a table in neither is an error.
/// One block per copy comes back ([`stringdb_block`]), both at the table's one path.
fn merge_string_tables(
    shipments: &[&LoadedShipment],
    game: &mut GameStack,
    mode: StringMerge,
    log: &mut Vec<String>,
) -> Result<MergedTables, BuildError> {
    let stage: &'static str = match mode {
        StringMerge::Strict => "string tables",
        StringMerge::Upsert => "link",
    };
    let fail = |message: String| BuildError::Lower { index: 0, kind: stage, message };
    let mut out = MergedTables::default();
    for t in apply_string_tables(shipments, game, mode, log)? {
        let block = stringdb_block(t.table, &t.merged).map_err(fail)?;
        let path = stringdb_block_path(t.table);
        if out.paths.last() != Some(&path) {
            out.paths.push(path);
        }
        match t.copy {
            TableCopy::Vz => out.overlay.push(block),
            TableCopy::Shell => out.shell.push((t.table, t.merged, block)),
        }
    }
    Ok(out)
}

/// Which copy of a string table a write is to: the gameplay one in vz.wad's stack, or the front
/// end's in shell.wad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableCopy {
    Vz,
    Shell,
}

/// One copy of a string table as a Shipment's build writes it: the table's asset hash, the target as
/// the first contribution to it spells it, which copy, the shipped container and the container with
/// every write applied.
pub struct StringTableWrite {
    pub table: u32,
    pub target: String,
    pub copy: TableCopy,
    pub base: Vec<u8>,
    pub merged: Vec<u8>,
}

/// Every string table `s`'s own build writes, as `qm build` writes it ([`merge_string_tables`],
/// strict): each table's `edit_stringdb`, `add_stringdb_keys` and `replace_stringdb_text`
/// contributions applied in contribution order to each copy of the table the game ships, the vz
/// copy before the shell one. A write `qm build` refuses is the same error here.
pub fn shipment_string_tables(s: &LoadedShipment, game: &mut GameStack) -> Result<Vec<StringTableWrite>, String> {
    apply_string_tables(&[s], game, StringMerge::Strict, &mut Vec::new()).map_err(|e| e.to_string())
}

/// [`merge_string_tables`] up to the merged containers.
fn apply_string_tables(
    shipments: &[&LoadedShipment],
    game: &mut GameStack,
    mode: StringMerge,
    log: &mut Vec<String>,
) -> Result<Vec<StringTableWrite>, BuildError> {
    let stage: &'static str = match mode {
        StringMerge::Strict => "string tables",
        StringMerge::Upsert => "link",
    };
    let fail = |message: String| BuildError::Lower {
        index: 0,
        kind: stage,
        message,
    };
    let mut by_table: std::collections::BTreeMap<u32, (String, Vec<TableWrite>, Vec<String>)> =
        std::collections::BTreeMap::new();
    for s in shipments {
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let (target, file) = match c {
                Contribution::EditStringDb { target, strings }
                | Contribution::AddStringDbKeys { target, strings } => (target, strings),
                Contribution::ReplaceStringDbText { target, pairs } => (target, pairs),
                _ => continue,
            };
            let name = &s.manifest.shipment.name;
            let who = format!("{name} contributions[{index}] ({})", c.kind());
            let path = s.root.join(file);
            let text = std::fs::read_to_string(&path)
                .map_err(|e| fail(format!("{who}: reading {}: {e}", path.display())))?;
            // A pairs file for text replacements, a strings file for the key kinds.
            let rows: Vec<(String, String)> = match c {
                Contribution::ReplaceStringDbText { .. } => {
                    parse_text_pairs(&text, &file.display().to_string())
                }
                _ => parse_string_edits(&text).map(|m| m.into_iter().collect()),
            }
            .map_err(|m| fail(format!("{who}: {m}")))?;
            if mode == StringMerge::Strict && rows.is_empty() {
                return Err(fail(format!(
                    "{who}: {} declares nothing — a contribution that changes nothing would ship a \
                     same-hash overlay that only restates the base table",
                    file.display()
                )));
            }
            let entry = by_table
                .entry(crate::manifest::asset_hash(target))
                .or_insert_with(|| (target.clone(), Vec::new(), Vec::new()));
            for (left, right) in rows {
                entry.1.push(match c {
                    Contribution::ReplaceStringDbText { .. } => TableWrite::Text {
                        old: left,
                        new: right,
                        who: who.clone(),
                    },
                    _ => TableWrite::Key {
                        key: mercs2_formats::stringdb::key_hash(&left),
                        label: left,
                        text: right,
                        write: if matches!(c, Contribution::AddStringDbKeys { .. }) {
                            KeyWrite::Add
                        } else {
                            KeyWrite::Edit
                        },
                        who: who.clone(),
                    },
                });
            }
            if !entry.2.contains(name) {
                entry.2.push(name.clone());
            }
        }
    }
    let mut tables = Vec::new();
    let mut shell = if by_table.is_empty() { None } else { Some(game.open_sibling("shell.wad").map_err(fail)?) };
    for (table, (target, writes, contributors)) in by_table {
        // The gameplay copy (vz.wad's stack) and the front end's (shell.wad); each one present is edited.
        let copies: Vec<(TableCopy, Vec<u8>)> = [
            (TableCopy::Vz, game.container_for_asset(table, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB)),
            (TableCopy::Shell, shell.as_mut().and_then(|s| s.container_for_asset(table, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB))),
        ]
        .into_iter()
        .filter_map(|(at, c)| c.map(|c| (at, c)))
        .collect();
        if copies.is_empty() {
            return Err(fail(format!(
                "{target:?} (0x{table:08X}) is a string table in neither vz.wad's stack nor shell.wad — check \
                 the spelling; there is no base table for {} to edit",
                contributors.join(", ")
            )));
        }
        for (copy, container) in copies {
            let merged = merge_table(&container, table, &target, &writes, &contributors, mode, log).map_err(&fail)?;
            tables.push(StringTableWrite { table, target: target.clone(), copy, base: container, merged });
        }
    }
    Ok(tables)
}

/// The string tables a set's writes were merged into: each table's block for the gameplay overlay (its
/// copy in vz.wad's stack) and for the shell patch (its copy in shell.wad, with the merged container),
/// and every table's block path once, in table order.
#[derive(Default)]
struct MergedTables {
    overlay: Vec<PatchBlock>,
    shell: Vec<(u32, Vec<u8>, PatchBlock)>,
    paths: Vec<String>,
}

/// `writes` applied to `container`, one copy of table `target`, in order.
fn merge_table(
    container: &[u8],
    table: u32,
    target: &str,
    writes: &[TableWrite],
    contributors: &[String],
    mode: StringMerge,
    log: &mut Vec<String>,
) -> Result<Vec<u8>, String> {
    {
        let merged = mercs2_formats::stringdb::apply_container(container, |db| {
            for w in writes {
                match w {
                    TableWrite::Key { key, label, text, write, who } => {
                        let exists = db.set_by_hash(*key, text);
                        match (mode, write, exists) {
                            (StringMerge::Strict, KeyWrite::Edit, false) => {
                                return Err(format!(
                                    "{who}: {label} is not a key in {target} — check the spelling; \
                                     the engine hashes the bracket key verbatim, so an unknown key \
                                     is a lookup that simply misses. To add a key, use \
                                     add_stringdb_keys"
                                ))
                            }
                            (StringMerge::Strict, KeyWrite::Add, true) => {
                                return Err(format!(
                                    "{who}: {label} already exists in {target} — use \
                                     edit_stringdb to overwrite, not add_stringdb_keys"
                                ))
                            }
                            (_, _, true) => {}
                            (_, _, false) => {
                                db.add_by_hash(*key, text);
                            }
                        }
                    }
                    TableWrite::Text { old, new, who } => {
                        if db.replace_exact_text(old, new) == 0 {
                            return Err(text_misses_message(who, target, &[old]));
                        }
                    }
                }
            }
            Ok(())
        })
        .map_err(|m| format!("{target:?} (0x{table:08X}): {m}"))?;
        log.push(format!(
            "string table {target} 0x{table:08X}: {} write(s) from {} applied in order, container \
             {} -> {} bytes",
            writes.len(),
            contributors.join(", "),
            container.len(),
            merged.len()
        ));
        Ok(merged)
    }
}

/// What the set's link compiles, handed to [`link_level`] for each level.
struct LinkSet<'a> {
    shipments: &'a [&'a LoadedShipment],
    corpus_root: &'a Path,
    ui_regs: &'a [link::UiRegistration],
    layer_regs: &'a [link::LayerRegistration],
    support_regs: &'a [link::SupportRegistration],
    sound_regs: &'a [link::SoundBankRegistration],
    additions: &'a [link::ScriptAddition],
    replacements: &'a [link::ScriptReplacement],
    outfits: &'a [crate::outfits::AddedOutfit],
    order: &'a [String],
    /// Whether a Shipment has a kind the fx merge applies in a custom level
    /// ([`crate::fx::merges_level_fx`]).
    level_fx: bool,
}

/// Each support shop item of `shipments` sold in `level` against the worldentity that level runs: the
/// one in `loaded`'s resident block once the link has merged into it (the campaign's with every
/// `level: vz` template, or a custom level's own). The item spawns its cargo and its delivery vehicle
/// by template name (`SetCargo`, `SetDeliveryVehicle`). A template the worldentity lacks fails the link,
/// naming the item, the level and the template: Blow It Up Again's campaign items, linked into `dlc01`,
/// whose worldentity lacks three of their cargo templates, stopped `dlc01`'s world load
/// with nothing logged (live 2026-11-20). A compact worldentity is read by `vz.wad`'s class layouts.
fn check_shop_templates(
    shipments: &[&LoadedShipment],
    level: &str,
    loaded: &[LoadedScriptBlock],
    game: &GameStack,
) -> Result<(), BuildError> {
    use crate::manifest::ShopCatalog;
    let mut needs: Vec<(usize, String, &str)> = Vec::new();
    for s in shipments {
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let Contribution::AddShopItem { id, catalog: ShopCatalog::Support, behaviour: Some(b), levels, .. } = c else { continue };
            if !levels.iter().any(|l| l == level) {
                continue;
            }
            for t in [&b.cargo, &b.delivery_vehicle].into_iter().flatten() {
                needs.push((index, format!("{}: shop item {id:?}", s.manifest.shipment.name), t.as_str()));
            }
        }
    }
    if needs.is_empty() {
        return Ok(());
    }
    let fail = |index: usize, message: String| BuildError::Lower { index, kind: "add_shop_item", message };
    let (_, resident_path) = link::SCRIPT_BLOCKS[1];
    let resident = loaded
        .iter()
        .find(|lb| lb.path == resident_path)
        .ok_or_else(|| fail(needs[0].0, format!("level {level}: the game stack has no {resident_path}, which holds the worldentity")))?;
    let at = crate::fx::worldentity_entry(&resident.block).map_err(|m| fail(needs[0].0, format!("level {level}: {m}")))?;
    let vz = *game.paths().first().ok_or_else(|| fail(needs[0].0, "the game stack is empty".into()))?;
    let schemas = mercs2_formats::be_to_le::ecs::ComponentSchemas::from_vz_wad(vz).map_err(|m| fail(needs[0].0, m))?;
    let we = mercs2_formats::worldentity::WorldEntity::parse_compact(&resident.block.entries[at].bytes, &schemas)
        .map_err(|m| fail(needs[0].0, format!("level {level}: its worldentity: {m}")))?;
    let names: std::collections::BTreeSet<u32> =
        we.name_hashes().map_err(|m| fail(needs[0].0, format!("level {level}: its worldentity: {m}")))?.into_iter().map(|(h, _)| h).collect();
    let missing: Vec<&(usize, String, &str)> =
        needs.iter().filter(|(_, _, t)| !names.contains(&mercs2_formats::hash::pandemic_hash_m2(t))).collect();
    let Some(first) = missing.first() else { return Ok(()) };
    let lines: Vec<String> = missing
        .iter()
        .map(|(_, who, t)| format!("{who} is sold in {level} (`levels`), and {level}'s worldentity has no template {t:?}"))
        .collect();
    Err(fail(first.0, lines.join("; ")))
}

/// The blocks of one level's whole patch.
///
/// The engine mounts no `vz-patch.wad` in a level, so the set's link has to be in the level's own
/// patch, and linked against what the level runs. `vz.wad`'s scripts blocks are read with the level's
/// assets in place of their twins, the set's scripts and effects are linked into those, and a script
/// the level overrides takes its appends by splicing ([`link::link_into_level_blocks`]). A block the
/// link touched ships whole, under its own path, so an asset of the level in it leaves the level's
/// own blocks: one WAD holds one row of a key. The link's other blocks, `linked`, ship as they are.
/// Also returns the modules the level's link added ([`link::LinkOutput::modules`]) and its import
/// findings ([`link::LinkOutput::findings`]).
fn link_level(
    level: &crate::level::LevelOf<'_>,
    set: &LinkSet<'_>,
    linked: &[PatchBlock],
    game: &mut GameStack,
    log: &mut Vec<String>,
) -> Result<(Vec<PatchBlock>, Vec<link::LinkedModule>, Vec<crate::imports::ImportFinding>), BuildError> {
    let fail = |message: String| BuildError::Lower { index: 0, kind: "link", message };
    // The set's script work in this level: every `patch_lua`, and the shop items it is sold in.
    let mut mutations = Vec::new();
    for s in set.shipments {
        mutations.extend(script_mutations(&s.manifest, &s.root, level.name)?);
    }
    let assets = crate::level::assets_of(level.manifest, level.name);
    let mut own = std::collections::BTreeMap::new();
    for a in &assets {
        own.insert(a.key().map_err(fail)?, *a);
    }
    let mut loaded = load_script_blocks(game, link::SCRIPT_BLOCKS, "link")?;
    let mut spliced = std::collections::BTreeSet::new();
    let mut placed: Vec<(usize, (u32, u32))> = Vec::new();
    for (bi, lb) in loaded.iter_mut().enumerate() {
        for e in &mut lb.block.entries {
            let key = (e.name_hash, e.type_hash);
            let Some(a) = own.get(&key) else { continue };
            let [r] = a.rungs else {
                return Err(fail(format!("{}: an asset of {} has {} rungs, and an entry has one", a.name, lb.path, a.rungs.len())));
            };
            let at = level.root.join(&r.file);
            e.bytes = std::fs::read(&at).map_err(|err| fail(format!("{}: {err}", at.display())))?;
            e.field_c = 0;
            if e.type_hash == mercs2_formats::types::TYPE_HASH_SCRIPT {
                spliced.insert(e.name_hash);
            }
            placed.push((bi, key));
        }
    }
    // The level runs its own master `L` (`LoadLevel(L, L)`), never `vz`'s chunk, so the master's hooks
    // go onto `L`: a level asset, not a twin of a `vz.wad` script, which ships from the level's own
    // blocks with the link's bytes in place of its file.
    let master_key = (crate::manifest::asset_hash(level.name), mercs2_formats::types::TYPE_HASH_SCRIPT);
    let master_asset = assets
        .iter()
        .find(|a| a.key().is_ok_and(|k| k == master_key))
        .ok_or_else(|| fail(format!("level {}: its level_asset rows hold no master script {:?}", level.name, level.name)))?;
    let [master_rung] = master_asset.rungs else {
        return Err(fail(format!("level {}: its master script has {} rungs, and a script has one", level.name, master_asset.rungs.len())));
    };
    let master_file = level.root.join(&master_rung.file);
    let mut master = mercs2_formats::scripts_block::ScriptsBlock {
        entries: vec![mercs2_formats::scripts_block::Entry {
            name_hash: master_key.0,
            type_hash: master_key.1,
            field_c: mercs2_formats::scripts_block::SCRIPT_FIELD_C,
            bytes: std::fs::read(&master_file).map_err(|e| fail(format!("{}: {e}", master_file.display())))?,
        }],
    };
    // Linked on every deploy (W-a): the level's hooks print their entry lines, mods or no mods.
    let mut touched = std::collections::BTreeSet::new();
    let modules;
    let findings;
    {
        let mut targets: Vec<link::TargetBlock<'_>> =
            loaded.iter_mut().map(|lb| link::TargetBlock { path: lb.path.clone(), block: &mut lb.block }).collect();
        let out = link::link_into_level_blocks(
            &mut targets,
            set.corpus_root,
            &mutations,
            set.ui_regs,
            set.layer_regs,
            set.support_regs,
            set.sound_regs,
            set.additions,
            set.replacements,
            set.order,
            &spliced,
            link::QmHooks::Always,
            Some(&mut master),
            level.name,
            set.outfits,
        )
        .map_err(|e| fail(format!("level {}: {e}", level.name)))?;
        touched.extend(out.scripts.iter().map(|l| l.block));
        modules = out.modules;
        findings = out.findings;
    }
    let master_bytes = master.entries.remove(0).bytes;
    log.push(format!("level {}: qm's world entry spliced onto its master {}, {} B", level.name, level.name, master_bytes.len()));
    let linked_assets = std::collections::BTreeMap::from([(master_key, master_bytes)]);
    let mut effects = None;
    if set.level_fx {
        let (block, resident) = lower_fx(set.shipments, game, &mut loaded, crate::fx::Scope::Link, FxLevel::Custom, "link", log)?;
        effects = block;
        touched.extend(resident);
    }
    check_shop_templates(set.shipments, level.name, &loaded, game)?;
    let moved: std::collections::BTreeSet<(u32, u32)> =
        placed.iter().filter(|(bi, _)| touched.contains(bi)).map(|(_, key)| *key).collect();
    let mut kept = Vec::with_capacity(assets.len());
    for a in &assets {
        if !moved.contains(&a.key().map_err(fail)?) {
            kept.push(*a);
        }
    }
    let mut blocks = script_patch_blocks(&loaded, &touched, "link")?;
    blocks.extend(effects);
    // The link's other blocks, but for one this patch carries already: a LOD rung the re-emitted
    // resident brings ships once, and must be the same block.
    for b in linked
        .iter()
        .filter(|b| b.path_string != crate::fx::EFFECTS_BLOCK.1 && !link::SCRIPT_BLOCKS.iter().any(|(_, p)| *p == b.path_string))
    {
        match blocks.iter().find(|o| o.path_string.eq_ignore_ascii_case(&b.path_string)) {
            None => blocks.push(b.clone()),
            Some(o) if o.compressed_data == b.compressed_data && o.aset_entries == b.aset_entries => {}
            Some(_) => {
                return Err(fail(format!(
                    "level {}: the link's {} and the level patch's differ, and the patch carries one block of a path",
                    level.name, b.path_string
                )))
            }
        }
    }
    blocks.extend(crate::level::compile(level.root, level.name, &kept, &linked_assets).map_err(fail)?);
    let mut paths: Vec<String> = blocks.iter().map(|b| b.path_string.to_ascii_lowercase()).collect();
    paths.sort();
    if let Some(w) = paths.windows(2).find(|w| w[0] == w[1]) {
        return Err(fail(format!("level {}: the patch carries {} twice", level.name, w[0])));
    }
    log.push(format!(
        "level {}: {} block(s), {} of its assets moved into the {} linked block(s), {} script(s) of its own open to appends",
        level.name,
        blocks.len(),
        moved.len(),
        touched.len(),
        spliced.len()
    ));
    Ok((blocks, modules, findings))
}

/// Write `plan` at [`crate::plan::GAME_FOLDER_RELATIVE`] under `out_dir`, the path it takes under the
/// game folder, and return its `game_folder` placement, hashed from the bytes written.
fn place_plan(out_dir: &Path, plan: &crate::plan::LoadPlan) -> Result<Placement, BuildError> {
    let relative = crate::plan::GAME_FOLDER_RELATIVE;
    let path = out_dir.join(relative);
    let io = |message: String| BuildError::Io { path: path.clone(), message };
    let dir = path.parent().ok_or_else(|| io("has no parent directory".into()))?;
    std::fs::create_dir_all(dir).map_err(|e| io(e.to_string()))?;
    crate::plan::write_json(&path, plan, "the load plan").map_err(io)?;
    let bytes = std::fs::read(&path).map_err(|e| io(e.to_string()))?;
    Ok(Placement {
        name: crate::plan::PLAN_FILE.to_string(),
        bytes: bytes.len(),
        sha256: sha256_hex(&bytes),
        destination: Destination::GameFolder { relative: relative.to_string() },
    })
}

/// The load plan's `libraries` rows of `modules`, each Shipment named by its request id. An ok plan has
/// one item per name (no M0203), so the name finds exactly one.
fn library_rows(
    modules: &[link::LinkedModule],
    inputs: &[crate::compat::PlanInput<'_>],
) -> Result<Vec<crate::plan::LibraryRow>, BuildError> {
    modules
        .iter()
        .map(|m| {
            let item = match &m.shipment {
                None => None,
                Some(name) => Some(
                    inputs
                        .iter()
                        .find(|i| i.shipment.manifest.shipment.name == *name)
                        .map(|i| i.id.to_string())
                        .ok_or_else(|| BuildError::Lower {
                            index: 0,
                            kind: "link",
                            message: format!("internal error: {name} adds the module {} but is not in the request", m.module),
                        })?,
                ),
            };
            Ok(crate::plan::LibraryRow { module: m.module.clone(), item, session: m.session, when: m.when.clone(), settles: m.settles })
        })
        .collect()
}

/// The filename of the deploy-time link overlay. Named to sort and read as "last".
pub const LINK_WAD_NAME: &str = "zz-quartermaster-link.wad";

/// The filename of the link's shell patch: the merged sound banks `shell.wad` carries and the
/// front end's scripts block with the set's front-end loader.
pub const LINK_SHELL_PATCH_NAME: &str = "zz-quartermaster-link.shell-patch.wad";

/// What a cross-Shipment link produced.
#[derive(Debug, Clone)]
pub struct LinkReport {
    pub wad: Option<PathBuf>,
    pub placements: Vec<Placement>,
    pub linked: Vec<crate::link::LinkedScript>,
    /// The load plan the link followed — always `ok`, since a plan that is not ok refuses the link
    /// ([`BuildError::Plan`]) — with the `libraries` the link loads. Written to `load-plan.json` beside
    /// `placement.json`, and placed in the game folder at [`crate::plan::GAME_FOLDER_RELATIVE`].
    pub plan: crate::plan::LoadPlan,
    pub log: Vec<String>,
}

/// Link **every installed Shipment's** script mutations into ONE overlay.
///
/// This is the half `build` cannot do. Each Shipment's own overlay carries a `scripts_vz` linked
/// from its own mutations, which makes it valid standalone — but WAD resolution is last-mounted-
/// wins, so installing two of them means one Shipment's Lua silently disappears. That is the exact
/// failure the mutation-not-a-block design exists to prevent, and preventing it requires a step
/// that sees all of them at once.
///
/// The result **must be mounted LAST**, after every Shipment overlay. Because it is built from all
/// their mutations together it is a superset of each, so whichever per-Shipment block it shadows, it
/// shadows with something strictly more complete.
///
/// Returns `wad: None` when no installed Shipment touches a script — there is nothing to shadow, and
/// emitting an overlay that merely restates the base block would be noise a user has to reason about.
///
/// **Plan first.** The set's load plan ([`crate::compat::plan`]) is computed before anything is
/// linked: a missing or out-of-range requirement, a declared or claimed conflict, a superseded file
/// still present, or a requirement cycle refuses the link with [`BuildError::Plan`], after writing
/// the plan to `out_dir`. Everything the link orders — append concatenation, `replace_lua`, the
/// `add_script` mint order and the `qm_modloader` bake — follows the plan's `order`, in which a
/// Shipment always comes after the Shipments it requires and the request order breaks ties.
pub fn link_installed(
    inputs: &[crate::compat::PlanInput<'_>],
    game: &mut GameStack,
    corpus_root: &Path,
    out_dir: &Path,
    original_data: Option<&Path>,
) -> Result<LinkReport, BuildError> {
    // A stale plan must never read as this run's, whatever happens below.
    crate::plan::remove_stale(out_dir).map_err(|message| BuildError::Io {
        path: out_dir.join(crate::plan::PLAN_FILE),
        message,
    })?;
    if game.platform() != Platform::Pc {
        return Err(BuildError::ConsoleOutputUnsupported);
    }
    let mut log = Vec::new();
    let mut mutations = Vec::new();

    let root = crate::compat::game_root_of(game.paths()[0])
        .map_err(|message| BuildError::Compat(crate::compat::CompatError::GameRoot { message }))?;
    let mut plan = crate::compat::plan(inputs, crate::plan::Producer::Link, &root)
        .map_err(BuildError::Compat)?;
    let write_plan = |plan: &crate::plan::LoadPlan| {
        crate::plan::write_plan(out_dir, plan).map_err(|message| BuildError::Io {
            path: out_dir.join(crate::plan::PLAN_FILE),
            message,
        })
    };
    if !plan.ok {
        write_plan(&plan)?;
        return Err(BuildError::Plan(Box::new(plan)));
    }
    // `ok` means no M0174, so the order exists; and no M0203, so each name is one item.
    let order_ids = plan.order.clone().ok_or_else(|| BuildError::Lower {
        index: 0,
        kind: "link",
        message: "internal error: an ok load plan has no order".into(),
    })?;
    let mut shipments: Vec<&LoadedShipment> = Vec::with_capacity(inputs.len());
    for oid in &order_ids {
        let input = inputs.iter().find(|i| i.id == oid.as_str()).ok_or_else(|| BuildError::Lower {
            index: 0,
            kind: "link",
            message: format!("internal error: the load order names {oid}, which is not in the request"),
        })?;
        shipments.push(input.shipment);
    }
    let order: Vec<String> = shipments
        .iter()
        .map(|s| s.manifest.shipment.name.clone())
        .collect();
    if shipments.len() > 1 {
        log.push(format!(
            "load order (requires first, then request order): {}",
            order.join(", ")
        ));
    }

    // Two `replace_fx` that resolve to one effect are a claim conflict even when either names the
    // effect through a template, and only the game's worldentity resolves a template: each target is
    // resolved here, against the game plus the set's additions in load order, and a collision refuses
    // the link as M0207, beside the conflicts the plan found by name.
    if shipments.iter().any(|s| crate::fx::has_fx(&s.manifest)) {
        let fail = |message: String| BuildError::Lower { index: 0, kind: "link", message };
        let base = crate::fx::GameFx::read(game).map_err(fail)?;
        let fx_base = crate::fx::FxBase {
            effects: &base.effects.entries,
            worldentity: &base.worldentity,
            fxdict: &base.fxdict,
            atlas: &base.atlas,
            repainted: false,
            // `conflicts` resolves `replace_fx` targets only; level templates are the merge's.
            level_templates: crate::fx::LevelTemplates::None,
        };
        let set: Vec<crate::fx::FxShipment<'_>> =
            shipments.iter().map(|s| crate::fx::FxShipment { manifest: &s.manifest, root: &s.root }).collect();
        let found = crate::fx::conflicts(&fx_base, &set);
        if !found.is_empty() {
            use crate::plan::{
                ClaimClass, ClaimConflictRow, ClaimantEntry, ConflictRow, ConflictSource, Finding, FindingRef,
                FindingSeverity, Section,
            };
            for c in found {
                let mut who: Vec<usize> = Vec::new();
                let mut claimants = Vec::new();
                for cl in &c.claimants {
                    let at = inputs
                        .iter()
                        .position(|i| i.shipment.manifest.shipment.name == cl.shipment)
                        .ok_or_else(|| fail(format!("internal error: {} is not in the request", cl.shipment)))?;
                    if !who.contains(&at) {
                        who.push(at);
                    }
                    claimants.push(ClaimantEntry {
                        item: inputs[at].id.to_string(),
                        contribution: cl.index,
                        kind: "replace_fx".into(),
                    });
                }
                who.sort_unstable();
                let row = plan.conflicts.len();
                plan.findings.push(Finding {
                    code: "M0207".into(),
                    severity: FindingSeverity::Error,
                    message: c.to_string(),
                    items: who.iter().map(|&i| inputs[i].id.to_string()).collect(),
                    refs: vec![FindingRef { section: Section::Conflicts, index: row }],
                    fix: None,
                });
                plan.conflicts.push(ConflictRow::Claims(ClaimConflictRow {
                    source: ConflictSource::Claims,
                    claim: format!("effect 0x{:08X}", c.effect),
                    class: ClaimClass::Exclusive,
                    claimants,
                }));
            }
            crate::plan::sort_findings(&mut plan.findings, |item| inputs.iter().position(|p| p.id == item));
            plan.ok = false;
            write_plan(&plan)?;
            return Err(BuildError::Plan(Box::new(plan)));
        }
    }

    // The shader stores: every Shipment's edits applied to the originals in load order, one store
    // per file for the whole set.
    let mut added = Vec::new();
    for s in &shipments {
        added.extend(crate::shader::added(&s.manifest.shipment.name, &s.manifest));
    }
    let shader_keys = crate::shader::ShaderKeys::with(&added);
    // M0235 / M0236 over the set: the shaders each add_model declares must be registered by retail,
    // by its own Shipment or by one it requires (transitively), since only those load before it.
    for (si, s) in shipments.iter().enumerate() {
        let mut providers: Vec<&str> = vec![order_ids[si].as_str()];
        let mut i = 0;
        while i < providers.len() {
            let consumer = providers[i];
            for e in plan.edges.iter().filter(|e| e.then == consumer) {
                if !providers.contains(&e.first.as_str()) {
                    providers.push(e.first.as_str());
                }
            }
            i += 1;
        }
        let mut visible = Vec::new();
        for (sj, t) in shipments.iter().enumerate() {
            if providers.contains(&order_ids[sj].as_str()) {
                visible.extend(crate::shader::added(&t.manifest.shipment.name, &t.manifest));
            }
        }
        let keys = crate::shader::ShaderKeys::with(&visible);
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let Contribution::AddModel { model, .. } = c else { continue };
            let declared = crate::shader_import::read_declared(&s.root.join(model)).map_err(|message| BuildError::Lower {
                index,
                kind: "add_model",
                message: format!("{}: {message}", s.manifest.shipment.name),
            })?;
            for (name, set, code, stage) in [
                (&declared.pixel, &keys.pixel, "M0235", "pixel"),
                (&declared.vertex, &keys.vertex, "M0236", "vertex"),
                (&declared.shadow, &keys.vertex, "M0236", "vertex"),
            ] {
                if let Some(n) = name {
                    if !set.contains(&mercs2_formats::hash::pandemic_hash_m2(n)) {
                        write_plan(&plan)?;
                        return Err(BuildError::Lower {
                            index,
                            kind: "add_model",
                            message: format!(
                                "[{code}] {}: the glTF declares {stage} shader {n:?}, which neither retail, \
                                 this Shipment nor a Shipment it requires registers in every configuration",
                                s.manifest.shipment.name
                            ),
                        });
                    }
                }
            }
        }
    }
    let mut store_placements = Vec::new();
    let shader_kind = shipments.iter().find_map(|s| {
        s.manifest
            .contributions
            .iter()
            .enumerate()
            .find(|(_, c)| matches!(c, Contribution::AddShader { .. } | Contribution::ReplaceShader { .. }))
            .map(|(index, c)| (index, c.kind()))
    });
    if let Some((index, kind)) = shader_kind {
        let original = original_data.ok_or(BuildError::OriginalDataRequired { index, kind })?;
        if let Some((a, code, message)) = crate::shader::registration_findings(&added).into_iter().next() {
            write_plan(&plan)?;
            return Err(BuildError::Lower {
                index: a.index,
                kind: "add_shader",
                message: format!("[{code}] {}: {message}", a.shipment),
            });
        }
        let mut edits = Vec::new();
        for s in &shipments {
            edits.extend(crate::shader::shipment_edits(&s.manifest.shipment.name, &s.manifest, &s.root).map_err(|f| {
                BuildError::Lower {
                    index: f[0].index,
                    kind: "shader",
                    message: format!("{}: {}", s.manifest.shipment.name, f[0].message),
                }
            })?);
        }
        std::fs::create_dir_all(out_dir).map_err(|e| BuildError::Io { path: out_dir.to_path_buf(), message: e.to_string() })?;
        store_placements = write_shader_stores(original, game, &edits, out_dir, &mut log)?;
    }

    let mut ui_regs: Vec<link::UiRegistration> = Vec::new();
    let mut layer_regs: Vec<link::LayerRegistration> = Vec::new();
    let mut support_regs: Vec<link::SupportRegistration> = Vec::new();
    let mut additions: Vec<link::ScriptAddition> = Vec::new();
    let mut replacements: Vec<link::ScriptReplacement> = Vec::new();
    let mut outfits: Vec<crate::outfits::AddedOutfit> = Vec::new();
    let mut sound_regs: Vec<link::SoundBankRegistration> = Vec::new();
    // What each Shipment's own build ships for sound, by level: the loaders' self-check below reads
    // it. Lowered through the one function `qm build` ships with (`sound::lower_shipment_sound`).
    let mut shipped_overlay: Vec<PatchBlock> = Vec::new();
    let mut shipped_shell: Vec<PatchBlock> = Vec::new();
    for s in &shipments {
        let mut sound_log = Vec::new();
        let sound = crate::sound::lower_shipment_sound(s, Some(&mut *game), &mut sound_log).map_err(|(index, kind, message)| {
            BuildError::Lower {
                index,
                kind,
                message: format!("{}: {message}", s.manifest.shipment.name),
            }
        })?;
        sound_regs.extend(sound.registrations);
        shipped_overlay.extend(sound.overlay);
        shipped_shell.extend(sound.shell);
        check_spawn_lists(&s.manifest, corpus_root, &format!("{}: ", s.manifest.shipment.name))?;
        mutations.extend(script_mutations(&s.manifest, &s.root, crate::level::VZ)?);
        ui_regs.extend(ui_registrations(&s.manifest));
        layer_regs.extend(layer_registrations(&s.manifest));
        support_regs.extend(support_registrations(&s.manifest, &s.root)?);
        additions.extend(script_additions(&s.manifest, &s.root)?);
        replacements.extend(script_replacements(&s.manifest, &s.root)?);
        outfits.extend(added_outfits(&s.manifest));
    }
    // The script link runs on every deploy, here and in each custom level: qm's hook dispatcher and
    // world loader entry are spliced onto the master whatever the set holds (W-a), so every installed
    // game prints each hook's entry line, mods or no mods. A Shipment's own script work links in the
    // same pass.
    // Every string table any Shipment edits or adds keys to is merged into one link-owned copy.
    let tables = merged_string_tables(shipments.iter().map(|s| &s.manifest));
    // Every sound bank any Shipment's replace_sound_cue targets or replace_sound_wave reaches, likewise.
    let sound_entries = crate::sound::linked_sound_entries(shipments.iter().map(|s| &s.manifest), Some(&mut *game))
        .map_err(|message| BuildError::Lower { index: 0, kind: "link", message })?;
    // Every effect, template and sprite, into one effects block, one worldentity, one fxdict and
    // one atlas.
    let fx = shipments.iter().any(|s| crate::fx::merges_fx(&s.manifest));
    let levels = crate::level::levels_of(&shipments);
    // Every level is on the Extras page ([`link::shell_pages_source`]).
    let mut level_entries = Vec::with_capacity(levels.len());
    for l in &levels {
        let date = l.date.map(crate::level::parse_level_date).transpose().map_err(|m| BuildError::Lower {
            index: 0,
            kind: "add_level",
            message: format!("{}: add_level {}: {m}", l.manifest.shipment.name, l.name),
        })?;
        level_entries.push(link::LevelEntry {
            shipment: l.manifest.shipment.name.clone(),
            level: l.name.to_string(),
            display: l.display.to_string(),
            mission: l.mission.map(str::to_string),
            details: l.details.map(str::to_string),
            date,
        });
    }
    // A shop item is sold in `vz` and the levels the set adds; any other name it lists is none of them.
    for s in &shipments {
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let Contribution::AddShopItem { id, levels: sold, .. } = c else { continue };
            if let Some(l) = sold.iter().find(|l| *l != crate::level::VZ && !levels.iter().any(|lv| lv.name == l.as_str())) {
                return Err(BuildError::Lower {
                    index,
                    kind: "add_shop_item",
                    message: format!(
                        "{}: shop item {id:?} is sold in {l:?} (`levels`), and no installed Shipment adds a level {l:?}",
                        s.manifest.shipment.name
                    ),
                });
            }
        }
    }
    log.push(format!(
        "linking {} mutation(s), {} UI, {} layer and {} sound-bank registration(s), {} string \
         table(s), {} sound bank(s) and {} effects from {} Shipment(s)",
        mutations.len(),
        ui_regs.len(),
        layer_regs.len(),
        sound_regs.len(),
        tables.len(),
        sound_entries.len(),
        if fx { "the set's" } else { "no" },
        shipments.len()
    ));

    let mut patches: Vec<PatchBlock> = Vec::new();
    let linked: Vec<link::LinkedScript>;
    let gameplay_modules: Vec<link::LinkedModule>;
    // The import findings of every link of the set: `vz`, the front end and each custom level.
    let mut import_found: Vec<crate::imports::ImportFinding> = Vec::new();
    let mut effects_block = None;
    {
        let mut loaded = load_script_blocks(game, link::SCRIPT_BLOCKS, "link")?;
        let mut touched = std::collections::BTreeSet::new();
        {
            let mut targets: Vec<link::TargetBlock<'_>> = loaded
                .iter_mut()
                .map(|lb| link::TargetBlock {
                    path: lb.path.clone(),
                    block: &mut lb.block,
                })
                .collect();
            let linked_out = link::link_into_level_blocks(
                &mut targets,
                corpus_root,
                &mutations,
                &ui_regs,
                &layer_regs,
                &support_regs,
                &sound_regs,
                &additions,
                &replacements,
                &order,
                &std::collections::BTreeSet::new(),
                link::QmHooks::Always,
                None,
                crate::outfits::DEFAULT_LEVEL,
                &outfits,
            )
            .map_err(|e| BuildError::Lower {
                index: 0,
                kind: "link",
                message: e.to_string(),
            })?;
            drop(targets);
            import_found.extend(linked_out.findings);
            linked = linked_out.scripts;
            gameplay_modules = linked_out.modules;
            for l in &linked {
                log.push(format!(
                    "linked {} in {}: {} → {} B source, {} B bytecode, from {:?}",
                    l.target,
                    loaded[l.block].path,
                    l.base_source_bytes,
                    l.linked_source_bytes,
                    l.bytecode_bytes,
                    l.contributors
                ));
                touched.insert(l.block);
            }
        }
        // The set's sprites, effects and templates, merged in load order: the effects block, and
        // the templates, the fxdict and the atlas in the same resident block the scripts above were
        // linked into.
        if fx {
            let (effects, resident) = lower_fx(&shipments, game, &mut loaded, crate::fx::Scope::Link, FxLevel::Main, "link", &mut log)?;
            effects_block = effects;
            touched.extend(resident);
        }
        touched.extend(join_vz_rows(&shipments, &mut loaded, game, &mut log)?);
        check_shop_templates(&shipments, crate::level::VZ, &loaded, game)?;
        patches.extend(script_patch_blocks(&loaded, &touched, "link")?);
    }
    // The plan promised the effects block exactly when a Shipment has an fx kind.
    let effects_promised = plan.link_block_paths.iter().any(|p| p == crate::fx::EFFECTS_BLOCK.1);
    if effects_promised != effects_block.is_some() {
        return Err(BuildError::Lower {
            index: 0,
            kind: "link",
            message: format!(
                "internal error: the plan {} the effects block and the link {} it",
                if effects_promised { "promised" } else { "did not promise" },
                if effects_block.is_some() { "merged" } else { "did not merge" }
            ),
        });
    }
    patches.extend(effects_block);

    // The merged string tables. The plan's `link_block_paths` promised exactly these, and a deploy
    // step drops the per-Shipment copies of each on that promise, so a mismatch is an internal error.
    let table_blocks = merge_string_tables(&shipments, game, StringMerge::Upsert, &mut log)?;
    let sound_paths: std::collections::BTreeSet<String> =
        sound_entries.iter().map(|&e| crate::sound::block_path(e)).collect();
    let rung_paths = linked_rung_paths(game).map_err(|message| BuildError::Lower { index: 0, kind: "link", message })?;
    let promised: Vec<String> = plan
        .link_block_paths
        .iter()
        .filter(|p| {
            !link::SCRIPT_BLOCKS.iter().chain(link::SHELL_SCRIPT_BLOCKS).any(|(_, s)| s == p)
                && !rung_paths.contains(p)
                && !sound_paths.contains(*p)
                && *p != crate::fx::EFFECTS_BLOCK.1
                && **p != movie_block_path(crate::manifest::asset_hash(crate::shell_extras::SHELL))
        })
        .cloned()
        .collect();
    if promised != table_blocks.paths {
        return Err(BuildError::Lower {
            index: 0,
            kind: "link",
            message: format!(
                "internal error: the plan promised the string-table blocks {promised:?}, and the \
                 link merged {:?}",
                table_blocks.paths
            ),
        });
    }
    patches.extend(table_blocks.overlay);
    // The front end's copies of the merged tables, in the link's shell patch; their containers are the
    // text an added menu item's label is set from.
    let shell_tables: Vec<(u32, Vec<u8>)> = table_blocks.shell.iter().map(|(t, c, _)| (*t, c.clone())).collect();
    let mut shell_table_blocks: Vec<PatchBlock> = table_blocks.shell.into_iter().map(|(_, _, b)| b).collect();

    // The merged sound banks: one soundbank per bank a replace_sound_cue targets, carrying every
    // Shipment's cue overrides, in each WAD that carries the bank; for a bank the engine loads, with
    // its sounddb and the one wavebank of every Shipment's override waves, in load order.
    let mut shell_blocks: Vec<PatchBlock> = Vec::new();
    let mut language_blocks: std::collections::BTreeMap<crate::manifest::Language, Vec<PatchBlock>> =
        std::collections::BTreeMap::new();
    if !sound_entries.is_empty() {
        let lowered = crate::sound::lower_overrides(
            &shipments,
            game,
            crate::sound::OverrideScope::Link,
            &mut log,
        )
        .map_err(|message| BuildError::Lower { index: 0, kind: "link", message })?;
        let merged: std::collections::BTreeSet<String> = lowered
            .overlay
            .iter()
            .chain(&lowered.shell)
            .chain(lowered.language.values().flatten())
            .map(|b| b.path_string.clone())
            .collect();
        if merged != sound_paths {
            return Err(BuildError::Lower {
                index: 0,
                kind: "link",
                message: format!(
                    "internal error: the plan promised the sound-bank blocks {sound_paths:?}, and \
                     the link merged {merged:?}"
                ),
            });
        }
        patches.extend(lowered.overlay);
        // Every bank the engine loads ships its three tables in one block of the link WAD.
        crate::sound::check_engine_banks(&lowered.engine_banks, &patches.iter().collect::<Vec<_>>())
            .map_err(|message| BuildError::Lower { index: 0, kind: "link", message })?;
        shell_blocks = lowered.shell;
        language_blocks = lowered.language;
    }
    shell_blocks.append(&mut shell_table_blocks);

    // The set's menu items, in load order, then each Shipment's contribution order; each runs a function
    // of a library the set loads in the front end.
    let set_items = menu_items_of(&shipments);
    check_item_actions(&set_items, &additions)?;
    let item_entries: Vec<link::MenuItemEntry> = set_items
        .iter()
        .map(|i| link::MenuItemEntry { shipment: i.shipment.to_string(), label: i.label.to_string(), action: i.action.to_string() })
        .collect();

    // The front end's hooks and loader, on every deploy (W-a): every Shipment's front-end banks in one
    // `qm_shell_modloader` and every front-end library, linked into `shell.wad`'s scripts block and
    // shipped in the link's shell patch.
    let (front_end, front_end_modules, front_end_found) =
        link_shell_loader(
            game,
            FrontEndSet { sound_regs: &sound_regs, levels: &level_entries, items: &item_entries, additions: &additions },
            &order,
            link::QmHooks::Always,
            "link",
            &mut log,
        )?;
    import_found.extend(front_end_found);
    shell_blocks.extend(front_end);
    // The Extras page is reachable on PC, and its hero pick starts the level chosen, only in qm's edit of
    // SHELL, and the menu items are qm's edit of it too, so it ships whenever the set has a level or a menu
    // item, and no Shipment may replace SHELL itself.
    if !level_entries.is_empty() || !set_items.is_empty() {
        let shell_hash = crate::manifest::asset_hash(crate::shell_extras::SHELL);
        for s in &shipments {
            for (index, c) in s.manifest.contributions.iter().enumerate() {
                let (Contribution::AddMovie { name, .. } | Contribution::AddUi { name, .. }) = c else { continue };
                if crate::manifest::asset_hash(name) == shell_hash {
                    return Err(BuildError::Lower {
                        index,
                        kind: c.kind(),
                        message: format!(
                            "{}: replaces {name}, and qm edits SHELL itself when the set has a level or a menu item (the Extras page lists {}; menu items: {})",
                            s.manifest.shipment.name,
                            if level_entries.is_empty() { "none".to_string() } else { level_entries.iter().map(|l| l.level.as_str()).collect::<Vec<_>>().join(", ") },
                            if set_items.is_empty() { "none".to_string() } else { set_items.iter().map(|i| i.label).collect::<Vec<_>>().join(", ") }
                        ),
                    });
                }
            }
        }
        let fail = |message: String| BuildError::Lower { index: 0, kind: "link", message };
        let mut shell = game.open_sibling("shell.wad").map_err(fail)?;
        let container = shell
            .container_for_asset(shell_hash, mercs2_formats::types::TYPE_HASH_CFX_PACK, TYPE_ID_CFX_PACK)
            .ok_or_else(|| fail("shell.wad holds no SHELL movie".into()))?;
        let retail = mercs2_formats::gfx::cfx_pack_movie(&container, "shell.wad SHELL").map_err(fail)?;
        let mut movie = retail.clone();
        if !level_entries.is_empty() {
            movie = crate::shell_extras::edit(&retail).map_err(fail)?;
            log.push(format!("SHELL: qm's Extras edit, {} → {} bytes, for {} level(s)", retail.len(), movie.len(), level_entries.len()));
        }
        if !set_items.is_empty() {
            let (edited, arts) = compose_menu_items(&set_items, &movie).map_err(fail)?;
            log.push(format!("SHELL: {} menu item(s) added, {} → {} bytes", set_items.len(), movie.len(), edited.len()));
            movie = edited;
            for (language, block) in menu_label_textures(&arts, game, &shell_tables, &mut shell, &mut log).map_err(fail)? {
                language_blocks.entry(language).or_default().push(block);
            }
        }
        shell_blocks.push(movie_block(shell_hash, &mercs2_formats::gfx::build_cfx_pack_block(shell_hash, &movie)).map_err(fail)?);
    }
    // The loaders load only what the set's builds ship, level by level.
    check_sound_loaders(
        &sound_regs,
        &shipped_overlay.iter().collect::<Vec<_>>(),
        &shipped_shell.iter().collect::<Vec<_>>(),
        "link",
    )?;

    // Each level's whole patch: the WAD a Shipment's `vz.wad` overlay cannot reach by mounting. Linked
    // here, before anything is written, so its import findings refuse the set with the others.
    let set = LinkSet {
        shipments: &shipments,
        corpus_root,
        ui_regs: &ui_regs,
        layer_regs: &layer_regs,
        support_regs: &support_regs,
        sound_regs: &sound_regs,
        additions: &additions,
        replacements: &replacements,
        outfits: &outfits,
        order: &order,
        level_fx: shipments.iter().any(|s| crate::fx::merges_level_fx(&s.manifest)),
    };
    let mut level_blocks = Vec::with_capacity(levels.len());
    for level in &levels {
        let (blocks, level_modules, level_found) = link_level(level, &set, &patches, game, &mut log)?;
        // A level links the same set, so it loads what `vz` loads, and the one plan describes both.
        if level_modules != gameplay_modules {
            return Err(BuildError::Lower {
                index: 0,
                kind: "link",
                message: format!(
                    "internal error: level {} adds the modules {:?}, and vz adds {:?}; the load plan has one list for both",
                    level.name,
                    level_modules.iter().map(|m| &m.module).collect::<Vec<_>>(),
                    gameplay_modules.iter().map(|m| &m.module).collect::<Vec<_>>()
                ),
            });
        }
        import_found.extend(level_found);
        level_blocks.push((level, blocks));
    }

    // M0209, M0313 and the M0314 error refuse the set; the M0314 reorder is a warning in the plan. A
    // level links what `vz` links, so its findings mostly repeat `vz`'s; each is reported once.
    import_found.sort();
    import_found.dedup();
    if !import_found.is_empty() {
        for f in &import_found {
            let mut refs = Vec::new();
            for s in &f.shipments {
                let at = inputs.iter().position(|i| i.shipment.manifest.shipment.name == *s).ok_or_else(|| BuildError::Lower {
                    index: 0,
                    kind: "link",
                    message: format!("internal error: {s} carries Lua the link checked but is not in the request"),
                })?;
                refs.push(at);
            }
            refs.sort_unstable();
            refs.dedup();
            let (severity, word) = if f.rule.is_warning() {
                (crate::plan::FindingSeverity::Warning, "warning")
            } else {
                (crate::plan::FindingSeverity::Error, "error")
            };
            plan.findings.push(crate::plan::Finding {
                code: f.rule.code().into(),
                severity,
                message: f.message.clone(),
                items: refs.iter().map(|&i| inputs[i].id.to_string()).collect(),
                refs: refs.iter().map(|&index| crate::plan::FindingRef { section: crate::plan::Section::Items, index }).collect(),
                fix: None,
            });
            log.push(format!("{word} {f}"));
        }
        crate::plan::sort_findings(&mut plan.findings, |item| inputs.iter().position(|p| p.id == item));
    }
    if import_found.iter().any(|f| !f.rule.is_warning()) {
        plan.ok = false;
        write_plan(&plan)?;
        return Err(BuildError::Plan(Box::new(plan)));
    }

    // A deploy drops a Shipment's copy of a block only when the plan promised it; a block the link
    // ships unpromised would sit beside that copy, and the two would conflict.
    for b in patches.iter().chain(&shell_blocks) {
        if !plan.link_block_paths.iter().any(|p| p.eq_ignore_ascii_case(&b.path_string)) {
            return Err(BuildError::Lower {
                index: 0,
                kind: "link",
                message: format!("internal error: the link ships {}, and the plan did not promise it", b.path_string),
            });
        }
    }

    let csum =
        mercs2_formats::donor::base_csum(game.paths()[0]).map_err(|m| BuildError::Lower {
            index: 0,
            kind: "link",
            message: m,
        })?;
    std::fs::create_dir_all(out_dir).map_err(|e| BuildError::Io {
        path: out_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let mut placements = store_placements;
    let mut diagnostics = Vec::new();
    if !shell_blocks.is_empty() {
        placements.push(write_patch_wad(
            out_dir,
            LINK_SHELL_PATCH_NAME,
            &shell_blocks,
            shell_csum(game, "link")?,
            Destination::ShellPatch,
            &shader_keys,
            &mut log,
            &mut diagnostics,
        )?);
    }
    for (language, lblocks) in language_blocks {
        let relative = format!("language_patch/{}.wad", language.token());
        placements.push(write_patch_wad(
            out_dir,
            &relative,
            &lblocks,
            csum,
            Destination::LanguagePatch {
                language: language.token().to_string(),
                relative: relative.clone(),
            },
            &shader_keys,
            &mut log,
            &mut diagnostics,
        )?);
    }
    for (level, blocks) in level_blocks {
        let relative = crate::level::patch_relative(level.name);
        placements.push(write_patch_wad(
            out_dir,
            &relative,
            &blocks,
            csum,
            Destination::LevelPatch {
                level: level.name.to_string(),
                relative: relative.clone(),
                display: level.display.to_string(),
            },
            &shader_keys,
            &mut log,
            &mut diagnostics,
        )?);
    }
    plan.libraries = Some(library_rows(
        &gameplay_modules.iter().chain(&front_end_modules).cloned().collect::<Vec<_>>(),
        inputs,
    )?);
    // The plan goes into the game folder too, where the Lua bridge's startup check reads it.
    placements.push(place_plan(out_dir, &plan)?);
    if patches.is_empty() {
        write_plan(&plan)?;
        write_placement_record(out_dir, &placements)?;
        log.push(format!(
            "wrote {}, {PLACEMENT_RECORD}: {} placement(s), no link overlay",
            crate::plan::PLAN_FILE,
            placements.len()
        ));
        return Ok(LinkReport {
            wad: None,
            placements,
            linked,
            plan,
            log,
        });
    }
    let wad_bytes =
        build_patch_wad_multi(&patches, csum, &FFCS_CERT_BLOB).map_err(|m| {
            BuildError::Lower {
                index: 0,
                kind: "link",
                message: m,
            }
        })?;

    // The link WAD is mounted LAST and so wins outright. It gets the same self-check as any other,
    // and for the same reason: nothing downstream would notice a defect here.
    let self_check = verify_emitted(&wad_bytes, &shader_keys)?;

    let path = out_dir.join(LINK_WAD_NAME);
    std::fs::write(&path, &wad_bytes).map_err(|e| BuildError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    let digest = sha256_hex(&wad_bytes);
    log.push(format!(
        "wrote {LINK_WAD_NAME}: {} bytes, sha256 {digest}",
        wad_bytes.len()
    ));
    for d in &self_check {
        log.push(format!("self-check: {d}"));
    }

    // The same record `build` writes, for the same reason: this WAD has to be MOUNTED, and where
    // it must sit in the mount order is not recoverable from the file itself. The name encodes the
    // intent ("sorts last") but a deploy step reading a directory should not have to infer a
    // contract from a filename — `destination: overlay` in the record is the contract.
    placements.insert(
        0,
        Placement {
            name: LINK_WAD_NAME.to_string(),
            bytes: wad_bytes.len(),
            sha256: digest,
            destination: Destination::Overlay,
        },
    );
    write_plan(&plan)?;
    write_placement_record(out_dir, &placements)?;
    log.push(format!(
        "wrote {}, {PLACEMENT_RECORD}: {} placement(s)",
        crate::plan::PLAN_FILE,
        placements.len()
    ));

    Ok(LinkReport {
        wad: Some(path),
        placements,
        linked,
        plan,
        log,
    })
}

/// The file name every `qm` output directory carries. There is exactly one.
pub const PLACEMENT_RECORD: &str = "placement.json";

/// The `placement.json` format this build writes. Its destination kinds are `overlay`,
/// `game_folder`, `data_wad`, `language_patch`, `shell_patch`, `level_patch`, `stream_copy` and
/// `data_file` ([`Destination`]).
pub const PLACEMENT_FORMAT: u32 = 2;

/// Write `placement.json` into `out_dir`, creating it if needed.
///
/// # Why every emitting path calls this
///
/// `build` wrote a placement record and `link_installed` did not, so a deploy tool had **two**
/// contracts for the same question: read the record for one output directory, and special-case
/// `zz-quartermaster-link.wad` by name for the other. A second output path nobody documented is
/// how a deploy step ends up guessing, and a guess about which files to mount is not a guess that
/// fails loudly — it mounts the wrong set and the game boots.
///
/// It is also written when `placements` is EMPTY, which is the case that actually needed
/// deciding. An absent file is ambiguous between "this step produced nothing" and "this step
/// never ran", and those want opposite responses from a deploy tool. `{"placements": []}` says
/// the first one outright.
fn write_placement_record(out_dir: &Path, placements: &[Placement]) -> Result<(), BuildError> {
    std::fs::create_dir_all(out_dir).map_err(|e| BuildError::Io {
        path: out_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let path = out_dir.join(PLACEMENT_RECORD);
    std::fs::write(&path, placement_json(placements)).map_err(|e| BuildError::Io {
        path,
        message: e.to_string(),
    })
}

fn placement_json(placements: &[Placement]) -> String {
    let entries: Vec<serde_json::Value> = placements
        .iter()
        .map(|p| {
            let dest = match &p.destination {
                Destination::Overlay => serde_json::json!({ "kind": "overlay" }),
                Destination::GameFolder { relative } => {
                    serde_json::json!({ "kind": "game_folder", "relative": relative })
                }
                Destination::DataWad { relative, display } => {
                    serde_json::json!({ "kind": "data_wad", "relative": relative, "display": display })
                }
                Destination::LanguagePatch { language, relative } => {
                    serde_json::json!({ "kind": "language_patch", "language": language, "relative": relative })
                }
                Destination::ShellPatch => serde_json::json!({ "kind": "shell_patch" }),
                Destination::LevelPatch { level, relative, display } => {
                    serde_json::json!({ "kind": "level_patch", "level": level, "relative": relative, "display": display })
                }
                Destination::StreamCopy { from, to } => {
                    serde_json::json!({ "kind": "stream_copy", "from": from, "to": to })
                }
                Destination::DataFile { relative, base_sha256 } => {
                    serde_json::json!({ "kind": "data_file", "relative": relative.relative(), "base_sha256": base_sha256 })
                }
            };
            serde_json::json!({
                "name": p.name,
                "bytes": p.bytes,
                "sha256": p.sha256,
                "destination": dest,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "format": PLACEMENT_FORMAT,
        "placements": entries,
    }))
    .unwrap_or_else(|_| "{}".into())
        + "\n"
}

#[cfg(test)]
mod placement_record {
    use super::*;

    #[test]
    fn a_data_file_names_its_closed_relative_and_the_originals_sha() {
        let json: serde_json::Value = serde_json::from_str(&placement_json(&[Placement {
            name: "shader3Low.bin".into(),
            bytes: 16,
            sha256: "ab".into(),
            destination: Destination::DataFile {
                relative: crate::shader::DataFile::Shader3Low,
                base_sha256: "cd".into(),
            },
        }]))
        .unwrap();
        assert_eq!(json["format"], 2);
        assert_eq!(
            json["placements"][0]["destination"],
            serde_json::json!({ "kind": "data_file", "relative": "data/shader3Low.bin", "base_sha256": "cd" })
        );
        assert_eq!(json["placements"][0]["sha256"], "ab");
    }
}

#[cfg(test)]
mod spawn_list_mutations {
    /// A `set_spawn_lists` with rows is the wrapper, appended to its target as a `patch_lua` is; one
    /// with none is no mutation, so it claims no scripts block.
    #[test]
    fn set_spawn_lists_appends_the_wrapper_and_no_rows_append_nothing() {
        let yaml = "format: 2\nshipment:\n  name: s\n  version: \"1.0.0\"\n  target: retail\ncontributions:\n  \
                    - kind: set_spawn_lists\n    target: gurcon002\n    lists:\n      \
                    - function: SetupDangerousObjBuildings\n        building: TallCommBuild\n        group: Rooftop\n        \
                    list: Spawnlist (VZ Elite)\n  \
                    - kind: set_spawn_lists\n    target: gurcon003\n    lists: []\n";
        let m = crate::from_str(yaml, crate::Format::Yaml).expect("parses");
        let got = super::script_mutations(&m, std::path::Path::new("."), crate::level::VZ).unwrap();
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!((got[0].shipment.as_str(), got[0].target.as_str()), ("s", "gurcon002"));
        let crate::manifest::Contribution::SetSpawnLists { lists, .. } = &m.contributions[0] else { unreachable!() };
        assert_eq!(got[0].append, crate::spawn_lists::wrapper(lists));
        assert!(got[0].append.contains("Ai.TweakAttachedSpawnersInGroup(TallCommBuild, \"Rooftop\", {"), "{}", got[0].append);
        assert_eq!(crate::blast::claims(&m).len(), 1, "only the contribution with rows claims its script");
    }
}

#[cfg(test)]
mod existing_model_outfit {
    use crate::manifest::Contribution;

    /// An `add_outfit` with no `model` file is the "wear an existing in-game model" form: it parses
    /// with `model: None`, injects nothing, yet is still an outfit the link adds to the wardrobe,
    /// wearing the existing model, and no script append.
    #[test]
    fn add_outfit_without_a_model_file_is_still_a_wardrobe_outfit() {
        let yaml = r#"
format: 2
shipment:
  name: wear-solano
  version: "1.0.0"
  target: retail
contributions:
  - kind: add_outfit
    name: vz_hum_solano
    slug: Solano
    display: Solano
    wearer: chris
"#;
        let m = crate::from_str(yaml, crate::Format::Yaml).expect("parses");
        match &m.contributions[0] {
            Contribution::AddOutfit { model, name, .. } => {
                assert!(model.is_none(), "no model file -> existing-model outfit");
                assert_eq!(name, "vz_hum_solano");
            }
            other => panic!("wrong kind: {}", other.kind()),
        }

        // The Script half: an outfit for the link's wardrobe lists, on the existing model.
        assert!(super::script_mutations(&m, std::path::Path::new("."), crate::level::VZ).unwrap().is_empty());
        assert_eq!(
            super::added_outfits(&m),
            vec![crate::outfits::AddedOutfit {
                shipment: "wear-solano".into(),
                slug: "Solano".into(),
                model: "vz_hum_solano".into(),
                display: "Solano".into(),
                wearers: vec!["chris".into()],
                levels: None,
                chatter: None,
                briefing_safe: true,
            }]
        );
    }
}

#[cfg(test)]
mod text_pairs_tests {
    use super::parse_text_pairs;

    /// Free text on both sides, exactly as written: spaces, `:`, `=`, `[` and `%s` are text.
    #[test]
    fn pairs_are_old_tab_new_verbatim() {
        let text = "# a comment\n\nPress Start: to begin\tPress [A] = %s\r\n  \n Leading space\t\n";
        assert_eq!(
            parse_text_pairs(text, "src/p.txt"),
            Ok(vec![
                ("Press Start: to begin".to_string(), "Press [A] = %s".to_string()),
                (" Leading space".to_string(), String::new()),
            ])
        );
    }

    #[test]
    fn a_malformed_line_names_the_file_and_line() {
        for (text, line, why) in [
            ("ok\tfine\nno tab here\n", 2, "no tab"),
            ("a\tb\tc\n", 1, "more than one tab"),
            ("# c\n\tnew\n", 2, "old text is empty"),
        ] {
            let e = parse_text_pairs(text, "src/p.txt").unwrap_err();
            assert!(e.starts_with(&format!("src/p.txt:{line}: ")), "{e}");
            assert!(e.contains(why), "{e}");
        }
    }

    /// The writer's line reads back as the pair, and what a line cannot hold is refused.
    #[test]
    fn a_written_pair_reads_back_and_what_a_line_cannot_hold_is_refused() {
        use super::{text_pair_line, text_pair_text};
        for (old, new) in [("Press Start: to begin", "Press [A] = %s"), (" Leading space", ""), ("a", " b ")] {
            let line = text_pair_text(old, new).unwrap();
            assert_eq!(text_pair_line(&line), Ok(Some((old.to_string(), new.to_string()))));
        }
        for (old, new, why) in [
            ("", "x", "empty"),
            ("a\tb", "x", "tab"),
            ("a", "b\nc", "line break"),
            ("a", "b\r", "line break"),
            ("#a", "x", "comment"),
            (" ", " ", "blank"),
        ] {
            let e = text_pair_text(old, new).unwrap_err();
            assert!(e.contains(why), "{old:?} {new:?}: {e}");
        }
    }
}

#[cfg(test)]
mod string_edits_tests {
    use super::{string_edit_line, string_edit_text};

    #[test]
    fn a_written_edit_reads_back_and_what_a_line_cannot_hold_is_refused() {
        for (key, text) in [("[Menu.Play]", "Play: now = %s"), ("0x0000B29D", ""), ("[A]", "x")] {
            let line = string_edit_text(key, text).unwrap();
            assert_eq!(string_edit_line(&line), Ok(Some((key.to_string(), text.to_string()))));
        }
        for (key, text, why) in [
            ("Menu.Play", "x", "`[Bracket.Key]`"),
            ("[]", "x", "empty"),
            ("[a]b]", "x", "`]`"),
            ("[A]", "two\nlines", "line break"),
            ("[A]", " padded", "whitespace"),
            ("0xZZ", "x", "`[Bracket.Key]`"),
        ] {
            let e = string_edit_text(key, text).unwrap_err();
            assert!(e.contains(why), "{key:?} {text:?}: {e}");
        }
    }
}

#[cfg(test)]
mod donor_tests {
    use super::auto_donor_for_wearer;

    #[test]
    fn each_hero_maps_to_its_own_base_model() {
        assert_eq!(auto_donor_for_wearer("mattias"), Some("pmc_hum_mattias"));
        assert_eq!(auto_donor_for_wearer("chris"), Some("pmc_hum_chris"));
        // Both the wardrobe key and the UI pill reach the same hero model.
        assert_eq!(auto_donor_for_wearer("jennifer"), Some("pmc_hum_jen"));
        assert_eq!(auto_donor_for_wearer("jen"), Some("pmc_hum_jen"));
        // Case- and space-insensitive, since it runs on author input.
        assert_eq!(auto_donor_for_wearer("  Mattias "), Some("pmc_hum_mattias"));
        assert_eq!(auto_donor_for_wearer("bulldog"), None);
    }
}

#[cfg(test)]
mod rigid_texture_tests {
    use super::{rigid_slot_froms, MTRL_TEXTURED};
    use mercs2_formats::texture::MtrlMaterial;

    fn mat(flags: u16, textures: &[u32]) -> MtrlMaterial {
        MtrlMaterial {
            textures: textures.to_vec(),
            flags,
            preamble: Vec::new(),
            shader_key: 0,
        }
    }

    /// The host group's materials, and only theirs, supply the hashes a map replaces — per slot,
    /// deduplicated. Material 2 belongs to another group and must not contribute.
    #[test]
    fn froms_come_from_the_host_groups_materials_only() {
        let mats = [
            mat(MTRL_TEXTURED, &[0xA0, 0xA1, 0xA2]),
            mat(MTRL_TEXTURED | 0x0008, &[0xB0, 0xA1, 0xB2]),
            mat(MTRL_TEXTURED, &[0xC0, 0xC1, 0xC2]),
        ];
        let groups = vec![vec![2], vec![0, 1]];
        let froms = rigid_slot_froms(1, &groups, &mats, &[0, 1]).unwrap();
        assert_eq!(froms.get(&0), Some(&vec![0xA0, 0xB0]));
        assert_eq!(froms.get(&1), Some(&vec![0xA1]), "a shared hash is listed once");
        assert_eq!(froms.get(&2), None, "an unsupplied slot is not repointed");
    }

    /// A touched host material without the textured flag is flat-shaded: the map would ship and
    /// never draw. That is a hard error naming the material, not a skipped material.
    #[test]
    fn an_untextured_host_material_is_a_hard_error() {
        let mats = [mat(MTRL_TEXTURED, &[0xA0]), mat(0x0000, &[0xB0])];
        let groups = vec![vec![0, 1]];
        let e = rigid_slot_froms(0, &groups, &mats, &[0]).unwrap_err();
        assert!(e.contains("material 1") && e.contains("0x0080"), "{e}");
    }

    /// An untextured material the repoint does NOT touch (nothing at the supplied slot) is fine.
    #[test]
    fn an_untouched_untextured_material_is_not_an_error() {
        let mats = [mat(MTRL_TEXTURED, &[0xA0, 0xA1, 0xA2]), mat(0x0000, &[0xB0])];
        let groups = vec![vec![0, 1]];
        let froms = rigid_slot_froms(0, &groups, &mats, &[2]).unwrap();
        assert_eq!(froms.get(&2), Some(&vec![0xA2]));
    }

    #[test]
    fn a_missing_host_group_or_material_is_an_error() {
        let mats = [mat(MTRL_TEXTURED, &[0xA0])];
        assert!(rigid_slot_froms(3, &[vec![0]], &mats, &[0]).unwrap_err().contains("does not exist"));
        assert!(rigid_slot_froms(0, &[vec![]], &mats, &[0]).unwrap_err().contains("no material"));
        assert!(rigid_slot_froms(0, &[vec![5]], &mats, &[0]).unwrap_err().contains("material 5"));
    }
}
