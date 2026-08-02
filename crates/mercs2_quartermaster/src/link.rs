//! The Lua linker — the thing that lets two script mods coexist.
//!
//! Script entries load from the **block**, not per-hash, so editing one script means re-emitting
//! every script in that block. Under one-overlay-per-Shipment plus last-mounted-wins, two Shipments
//! that each ship a finished `scripts_vz` do not merge and do not error: the later one wins and the
//! earlier one's Lua vanishes — model, wardrobe row and all — with nothing reported. That single
//! failure is why `patch_lua` declares a *mutation* instead of shipping a block, and why this module
//! exists.
//!
//! Targets resolve across every block in [`SCRIPT_BLOCKS`] — `scripts_vz` and `resident` — because
//! the framework modules most worth patching (`mrxplayer`, `mrxguipda`, the `MrxTask*` family) are
//! resident, not `vz`.
//!
//! So linking happens across the **installed set**, not per build:
//!
//! ```text
//!   base script source (vendored corpus)
//!     + each Shipment's `append:` source, in a deterministic order
//!     -> mercs2_luac::compile
//!     -> splice into the block  -> one scripts_vz for everyone
//! ```
//!
//! Our own field guide reached the same conclusion independently, from the modder's side: "N mods
//! union by plain text concatenation, compiled once. That is why exactly one thing must own
//! `scripts_vz`."
//!
//! ## Two facts this depends on, both measured rather than assumed
//!
//! - **The chunk name must be the bare script name** — no `@`, no `.lua`. Retail's LuaQ headers
//!   store it verbatim, and `mercs2_luac/tests/parity.rs` found this by way of all 113 scripts
//!   differing from retail by a constant 5 bytes (`@` + `.lua`) until it was corrected.
//! - **No retail `scripts_vz` container carries metadata after its bytecode**, so `replace_lua`'s
//!   refusal to touch such a container never fires here — surveyed across all 114 in
//!   `mercs2_formats/tests/scripts_block_survey.rs`, which also pins that a no-op splice reproduces
//!   the block byte for byte.
//!
//! ## Path-in, like everything else here
//!
//! The base sources come from the vendored decompiled corpus, taken as a **path**. That is partly
//! the crate's standing discipline and partly forced: the crate that owns the corpus
//! (`mercs2_script`) links a second, incompatible Lua runtime — see the note in `Cargo.toml`.

use crate::manifest::Load;
use mercs2_formats::scripts_block::ScriptsBlock;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Every block a `patch_lua` target may live in, as `(PTHS needle, PTHS path)`.
///
/// Game scripts are split across blocks: `scripts_vz` holds the 114 content scripts (contracts,
/// jobs, tutorials) and `resident` holds the ~240 framework modules (`Mrx*`, world-entity scripts)
/// that are always loaded. A fix targeting `mrxplayer` is unreachable without the second.
///
/// Searched in order, so a name present in both resolves to `scripts_vz`. No shipped script name
/// appears in both, and the type-aware lookup makes a cross-type collision impossible; the order is
/// fixed so the outcome stays deterministic if that ever stops being true.
///
/// ⚠ **The resident needle is ANCHORED on purpose.** `block_by_path` matches a substring, and
/// unanchored `resident_P000_Q3` also matches `sound_resident_P000_Q3.block` — a completely
/// different block. `shell` is absent deliberately: it lives in `shell.wad`, which never shares a
/// mount slot with `vz.wad`, so it needs its own overlay rather than a row here.
pub const SCRIPT_BLOCKS: &[(&str, &str)] = &[
    ("scripts_vz", r"blocks\VZ\scripts_vz_P000_Q3.block"),
    (r"\resident_P000_Q3.block", r"blocks\VZ\resident_P000_Q3.block"),
];

/// One Shipment's declared edit to one script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptMutation {
    /// Which Shipment asked for it — the tie-break for ordering, and what a conflict names.
    pub shipment: String,
    /// Base script, e.g. `wifpmcinterior`.
    pub target: String,
    /// Source text appended after the base. Not bytecode: the whole point is that N appends
    /// concatenate and compile once.
    pub append: String,
}

#[derive(Debug)]
pub enum LinkError {
    /// The base script is not in the block being linked.
    UnknownScript {
        target: String,
        shipment: String,
    },
    /// The base script has no source in the corpus, so there is nothing to append to.
    ///
    /// Real and expected for some targets: the corpus covers 370 of 382 scripts, and the gaps are
    /// modules `unluac` could not round-trip. Those are structurally un-linkable, and saying so is
    /// better than emitting a block that silently drops the mod.
    NoBaseSource {
        target: String,
        tried: Vec<PathBuf>,
    },
    Compile {
        target: String,
        message: String,
    },
    Splice {
        target: String,
        message: String,
    },
    /// `load.after`/`load.before` across the installed set form a cycle, so no deterministic order
    /// exists. Names the Shipments still tangled after everything orderable was placed.
    LoadCycle {
        names: Vec<String>,
    },
    Block(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::UnknownScript { target, shipment } => write!(
                f,
                "{shipment} patches {target:?}, which is not in this scripts block — check the \
                 spelling; a name that does not exist simply misses"
            ),
            LinkError::NoBaseSource { target, tried } => write!(
                f,
                "no decompiled source for {target:?}, so there is nothing to append to (tried: {}). \
                 The corpus covers 370 of 382 scripts; the gaps are modules the decompiler could not \
                 round-trip, and they cannot be linked",
                tried.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
            LinkError::Compile { target, message } => {
                write!(f, "compiling the linked {target:?}: {message}")
            }
            LinkError::Splice { target, message } => {
                write!(f, "splicing {target:?} back into the block: {message}")
            }
            LinkError::LoadCycle { names } => write!(
                f,
                "load order is cyclic — {} reference each other through load.after / load.before, so \
                 no order satisfies them all. Break the cycle by removing one constraint",
                names.join(", ")
            ),
            LinkError::Block(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// What the link produced, for the build log and for diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedScript {
    pub target: String,
    /// Shipments that contributed, in the order their source was concatenated.
    pub contributors: Vec<String>,
    pub base_source_bytes: usize,
    pub linked_source_bytes: usize,
    pub bytecode_bytes: usize,
    /// Index into the `blocks` slice handed to [`link_into_blocks`] — which block this script was
    /// spliced into, so the caller emits only the blocks that actually changed.
    pub block: usize,
}

/// One candidate scripts block, with the PTHS path the overlay must carry for it.
///
/// Game scripts live in more than one block and a `patch_lua` target may be in any of them, so the
/// linker takes the set and resolves each target against it rather than being told which block to
/// use.
pub struct TargetBlock<'a> {
    /// The block's own PTHS path, e.g. `blocks\VZ\scripts_vz_P000_Q3.block`. Carried through to the
    /// emitted `PatchBlock` so the overlay shadows the right base block.
    pub path: String,
    pub block: &'a mut ScriptsBlock,
}

/// Find a script's decompiled source under the corpus.
///
/// Searched in `vz`, `resident`, `shell`, then the stub directory — `vz/<name>.lua` is the mapping
/// that matters for `scripts_vz`, and the others are there so the same helper serves the resident
/// block later without growing a second lookup.
pub fn base_source_path(corpus_root: &Path, target: &str) -> Result<PathBuf, Vec<PathBuf>> {
    let mut tried = Vec::new();
    for sub in ["vz", "resident", "shell"] {
        let p = corpus_root.join(sub).join(format!("{target}.lua"));
        if p.is_file() {
            return Ok(p);
        }
        tried.push(p);
    }
    // `corpus/stubs` sits beside `corpus/mercs2-luacd/src`, not inside it.
    if let Some(corpus_dir) = corpus_root.parent().and_then(|p| p.parent()) {
        let p = corpus_dir.join("stubs").join(format!("{target}.lua"));
        if p.is_file() {
            return Ok(p);
        }
        tried.push(p);
    }
    Err(tried)
}

/// The deterministic load order over an installed set: an alphabetical base that `load.after` /
/// `load.before` then constrain, tie-broken by name so it is stable.
///
/// **Name is the base, not install order.** Two installs of the same set must produce byte-identical
/// output, or verify-by-hash means nothing and a saved costume position can shift under the player
/// between deploys. `after`/`before` do not replace that sort — they constrain it: a Kahn
/// topological pass that, among ready Shipments, always takes the alphabetically smallest. A
/// constraint naming an uninstalled Shipment is inert (you cannot order against what is not there); a
/// cycle is a named error, never a silent arbitrary pick.
pub fn resolve_load_order(shipments: &[(String, Load)]) -> Result<Vec<String>, LinkError> {
    let names: BTreeSet<&str> = shipments.iter().map(|(n, _)| n.as_str()).collect();
    // Directed edges `x -> y` meaning x loads before y. `after: A` on N ⇒ A before N. `before: B` on
    // N ⇒ N before B. Edges touching an uninstalled name, or self-edges, are dropped.
    let mut edges: Vec<(&str, &str)> = Vec::new();
    for (n, load) in shipments {
        for a in &load.after {
            if names.contains(a.as_str()) && a != n {
                edges.push((a.as_str(), n.as_str()));
            }
        }
        for b in &load.before {
            if names.contains(b.as_str()) && b != n {
                edges.push((n.as_str(), b.as_str()));
            }
        }
    }
    let mut succ: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut indeg: BTreeMap<&str, usize> = names.iter().map(|&n| (n, 0)).collect();
    for (from, to) in edges {
        if succ.entry(from).or_default().insert(to) {
            *indeg.get_mut(to).unwrap() += 1;
        }
    }
    let mut ready: BTreeSet<&str> =
        indeg.iter().filter(|(_, &d)| d == 0).map(|(&n, _)| n).collect();
    let mut order: Vec<String> = Vec::with_capacity(names.len());
    while let Some(&n) = ready.iter().next() {
        ready.remove(n);
        order.push(n.to_string());
        if let Some(ss) = succ.get(n) {
            for &s in ss {
                let d = indeg.get_mut(s).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(s);
                }
            }
        }
    }
    if order.len() != names.len() {
        let placed: BTreeSet<&str> = order.iter().map(|s| s.as_str()).collect();
        let tangled: Vec<String> =
            names.iter().filter(|n| !placed.contains(*n)).map(|s| s.to_string()).collect();
        return Err(LinkError::LoadCycle { names: tangled });
    }
    Ok(order)
}

/// Concatenate the base source with every mutation's append, in the deterministic load order.
///
/// `order` is the resolved sequence from [`resolve_load_order`]; a Shipment absent from it (an empty
/// `order`, or the synthetic mod-loader trampoline) falls back to name order, so the historical
/// behaviour — pure alphabetical — is exactly the empty-`order` case. Load order decides who *wins* a
/// conflict; it does not get to decide the bytes of a merge that has no conflict, which is why the
/// tie-break is still the name.
pub fn linked_source(
    base: &str,
    mutations: &[&ScriptMutation],
    order: &[String],
) -> (String, Vec<String>) {
    let rank = |name: &str| order.iter().position(|n| n == name).unwrap_or(usize::MAX);
    let mut ordered: Vec<&&ScriptMutation> = mutations.iter().collect();
    ordered.sort_by(|a, b| {
        rank(&a.shipment)
            .cmp(&rank(&b.shipment))
            .then_with(|| a.shipment.cmp(&b.shipment))
    });

    let mut out = String::with_capacity(
        base.len() + ordered.iter().map(|m| m.append.len()).sum::<usize>() + 256,
    );
    out.push_str(base);
    if !base.ends_with('\n') {
        out.push('\n');
    }
    let mut contributors = Vec::new();
    for m in &ordered {
        // Attributed in the source itself: when someone decompiles a linked block to work out why
        // their game behaves oddly, the answer should be readable rather than inferred.
        out.push_str(&format!(
            "\n-- [Quartermaster] appended by Shipment: {}\n",
            m.shipment
        ));
        out.push_str(&m.append);
        if !m.append.ends_with('\n') {
            out.push('\n');
        }
        contributors.push(m.shipment.clone());
    }
    (out, contributors)
}

/// The `_tOutfits` row for one outfit, as source.
///
/// `_tOutfits` is a GLOBAL declared without `local`, so a mod never needs an AST edit — appending a
/// `table.insert` after the base source is enough, and N of them union by plain concatenation. The
/// row's three fields are three distinct strings: `Model` is the asset name `Player.SetOutfit`
/// receives, `Name` is the unlock/tracking key, and `PlayerVisibleName` is what the wardrobe shows.
///
/// **Append only, never insert.** Index 2 is reserved for the unlock-code outfit, and a saved
/// costume is a POSITION into this list — inserting would silently re-dress every existing player.
pub fn outfit_row_append(wearer: &str, slug: &str, model: &str, display: &str) -> String {
    // Normalize to the RUNTIME `_tOutfits` key — the game's third key is `jennifer`, so an outfit
    // authored with the preferred `jen` must land there, not in an empty `_tOutfits.jen` the game
    // never reads. An unknown wearer falls through as-written; M0140 already flags it.
    let key = crate::manifest::wearer_table_key(wearer).unwrap_or(wearer);
    format!(
        "table.insert(_tOutfits.{key}, {{ Name = {}, Model = {}, PlayerVisibleName = {} }})\n",
        lua_string(slug),
        lua_string(model),
        lua_string(display),
    )
}

/// The name of the Quartermaster mod-loader script — a NEW `scripts_vz` script the linker mints and
/// `add_script`s into the block, distinct from any retail script. Its ASET row (type 35, primary)
/// is emitted automatically by `script_patch_blocks`' new-entry branch, which is the other half of
/// the DLC's own recipe for a new importable script (`dlc_aset_normalize.py`).
///
/// `import` resolves by name → `pandemic_hash_m2` → ASET lookup, and is **scripts_vz only**, so this
/// lands in the same `scripts_vz` block as its trampoline host `wifpmcinterior`, never the resident.
pub const QM_MODLOADER_NAME: &str = "qm_modloader";

/// One `add_ui` registration, resolved to the movie the loader must show. The linker collects these
/// across every Shipment and bakes them into a single generated `qm_modloader` script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiRegistration {
    /// Which Shipment asked for it — the tie-break for the deterministic bake order.
    pub shipment: String,
    /// The `cfx_pack` movie name the FlashWidget plays (`add_movie`'s asset name).
    pub movie: String,
}

/// One `activate_layer` registration, resolved to the layer marks the loader must apply. The linker
/// collects these across every Shipment and bakes them into the same `qm_modloader` script `add_ui`
/// uses, so a UI mod and a layer mod share one load space and one trampoline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerRegistration {
    /// Which Shipment asked for it — the tie-break for the deterministic bake order.
    pub shipment: String,
    /// The layer to `MrxLayerManager.MarkForAddition`.
    pub add: String,
    /// Layers to `MrxLayerManager.MarkForRemoval` first — the overlay(s) this one supersedes.
    pub remove: Vec<String>,
}

/// The whole `qm_modloader` script, baked from every UI registration.
///
/// This is the "expandable load space" the user asked for: the game's resident scripts stay
/// untouched but for a one-line trampoline (see [`qm_trampoline_append`]); everything a mod adds
/// lives *here*, in a script Quartermaster owns and re-mints each link. It defines the global `_QM`
/// (matching how `import` publishes a module — `import("MrxPlayer")` then `MrxPlayer.Start()`), so
/// the trampoline depends on the side effect, not on `import`'s return value.
///
/// Idempotent and fail-soft: `_registered`/`_ran` guards make a re-import or a re-entered interior a
/// no-op, and each registration runs under `pcall` so one bad movie cannot wedge the loader.
///
/// Registrations are ordered by `(shipment, movie)` / `(shipment, add)` so the same set bakes
/// byte-identically whatever order they arrive in — the same determinism rule [`linked_source`] holds.
pub fn qm_modloader_source(regs: &[UiRegistration], layers: &[LayerRegistration]) -> String {
    let mut ordered: Vec<&UiRegistration> = regs.iter().collect();
    ordered.sort_by(|a, b| a.shipment.cmp(&b.shipment).then_with(|| a.movie.cmp(&b.movie)));

    let mut inits = String::new();
    for r in &ordered {
        let n = lua_string(&r.movie);
        // The creation sequence — new → SetSwfFile → Play → SetVisible — is the proven one from
        // decompiled mrxgui.lua (`loadingscreen_standalone`). Existence-checked so a stripped widget
        // table degrades rather than errors; the handle is parked in `_QM.ui[name]` for the author.
        inits.push_str(&format!(
            "  -- {shipment}: {movie}\n  \
             table.insert(_QM._inits, function()\n    \
             local w = FlashWidget:new(); w:SetSwfFile({n})\n    \
             if w.Play then w:Play() end\n    \
             if w.SetVisible then w:SetVisible(true) end\n    \
             _QM.ui[{n}] = w\n  \
             end)\n",
            shipment = r.shipment,
            movie = r.movie,
        ));
    }

    // Layer activations bake into the SAME `_QM._inits` list, so they run under the same once-guard
    // and the same `pcall` as the UI widgets. Each is `MarkForRemoval`(old) then `MarkForAddition`
    // (new) — the vanilla-contract order (remove pristine, add act) so the two never both apply.
    // Ordered by `(shipment, add)` for the byte-identical bake. `MarkForAddition`/`MarkForRemoval`
    // are the immediate mark forms (no callback), existence-checked so a stripped table degrades.
    let mut ordered_layers: Vec<&LayerRegistration> = layers.iter().collect();
    ordered_layers.sort_by(|a, b| a.shipment.cmp(&b.shipment).then_with(|| a.add.cmp(&b.add)));
    for r in &ordered_layers {
        let add = lua_string(&r.add);
        let mut body = String::new();
        for rem in &r.remove {
            body.push_str(&format!(
                "      if MrxLayerManager.MarkForRemoval then MrxLayerManager.MarkForRemoval({}) end\n",
                lua_string(rem),
            ));
        }
        body.push_str(&format!(
            "      if MrxLayerManager.MarkForAddition then MrxLayerManager.MarkForAddition({add}) end\n"
        ));
        inits.push_str(&format!(
            "  -- {shipment}: activate {layer}\n  \
             table.insert(_QM._inits, function()\n    \
             if MrxLayerManager then\n{body}    end\n  \
             end)\n",
            shipment = r.shipment,
            layer = r.add,
        ));
    }

    format!(
        "-- {name} — Quartermaster's expandable mod load space (generated; do not hand-edit).\n\
         --\n\
         -- The resident scripts import this by name; it defines the global _QM and its run() entry.\n\
         -- All modded UI registrations live here, so the resident carries only a one-line trampoline\n\
         -- into this script (see the [Quartermaster] trampoline appended to wifpmcinterior).\n\
         _QM = _QM or {{}}\n\
         _QM.ui = _QM.ui or {{}}\n\
         if not _QM._registered then\n\
         \x20 _QM._registered = true\n\
         \x20 _QM._inits = {{}}\n\
         {inits}\
         \x20 function _QM.run()\n\
         \x20   if _QM._ran then return end\n\
         \x20   _QM._ran = true\n\
         \x20   for _, f in ipairs(_QM._inits) do pcall(f) end\n\
         \x20 end\n\
         end\n",
        name = QM_MODLOADER_NAME,
    )
}

/// The one-line trampoline appended to `wifpmcinterior` — the ONLY thing the resident carries.
///
/// It wraps `_OnEnter` (the PMC-interior entry hook: GUI is fully up by then, it fires every
/// session, and it is file-local so this concatenated append can wrap it — the same property the
/// `_tOutfits` append relies on), synchronously `import`s `qm_modloader`, and runs it once. `import`
/// is scripts_vz-only and synchronous, so by the time `_QM.run()` is reached the module is loaded
/// and `_QM` is defined. Guarded through `_QM.run`'s own `_ran`, so re-entering the interior is a
/// no-op. Nothing here grows as mods are added — new mods only enlarge `qm_modloader`.
pub fn qm_trampoline_append() -> String {
    format!(
        "\n-- [Quartermaster] mod-loader trampoline. The expandable load space is {name} (a\n\
         -- scripts_vz script imported by ASET hash); this is the only line the resident carries.\n\
         do\n\
         \x20 local _qm_prev_OnEnter = _OnEnter\n\
         \x20 _OnEnter = function(...)\n\
         \x20   if _qm_prev_OnEnter then _qm_prev_OnEnter(...) end\n\
         \x20   import({name_lit})\n\
         \x20   if _QM and _QM.run then _QM.run() end\n\
         \x20 end\n\
         end\n",
        name = QM_MODLOADER_NAME,
        name_lit = lua_string(QM_MODLOADER_NAME),
    )
}

/// A Lua string literal with quotes and backslashes escaped, so an author-supplied `display:`
/// cannot terminate the string and inject code into the block we compile.
fn lua_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Source the Quartermaster appends ONCE per target, after every Shipment's contribution.
///
/// For the wardrobe this is the availability lift. `GetAvailableCostumes()` returns
/// `_nAvailableCostumes or 1`, and the menu only offers entry `i` when the count is `>= i` — so an
/// appended outfit is in the WAD, in the table, and **unreachable** unless the count grows too.
///
/// It is emitted here rather than by each Shipment precisely because "exactly once" is the
/// property: two Shipments each hard-coding `shipped + 1` produce the same number, the later
/// definition wins, and one outfit stays invisible. Deriving it from the final list length is the
/// only form that survives N contributors.
///
/// Curated per target and empty by default, in the same fail-closed spirit as the merge classes.
pub fn derived_epilogue(target: &str) -> Option<String> {
    match target {
        "wifpmcinterior" => Some(
            "\n-- [Quartermaster] derived: the wardrobe gate is a COUNT, and the menu only offers\n\
             -- entry i when it is >= i. Derived from the final list length so it is correct for\n\
             -- any number of appended outfits.\n\
             function GetAvailableCostumes()\n\
             \x20 local n = 1\n\
             \x20 for _, list in pairs(_tOutfits) do\n\
             \x20   if #list > n then n = #list end\n\
             \x20 end\n\
             \x20 return n\n\
             end\n"
                .to_string(),
        ),
        _ => None,
    }
}

/// Link every mutation into `block`, returning what changed.
///
/// `block` is the base game's `scripts_vz`, already parsed. Mutations targeting the same script are
/// merged; mutations targeting different scripts are independent.
///
/// The single-block convenience case of [`link_into_blocks`].
pub fn link_into(
    block: &mut ScriptsBlock,
    corpus_root: &Path,
    mutations: &[ScriptMutation],
) -> Result<Vec<LinkedScript>, LinkError> {
    let mut blocks = [TargetBlock {
        path: String::new(),
        block,
    }];
    link_into_blocks(&mut blocks, corpus_root, mutations, &[], &[], &[])
}

/// Link every mutation into whichever of `blocks` actually carries its target script.
///
/// Mutations targeting the same script are merged; mutations targeting different scripts are
/// independent, **including when they land in different blocks**. Blocks are searched in the order
/// given and the first script-typed match wins.
///
/// Only blocks that were spliced come back in the results (via [`LinkedScript::block`]) — a block
/// nothing targeted must not be re-emitted, or the overlay would shadow a base block with a
/// byte-identical copy for no reason.
pub fn link_into_blocks(
    blocks: &mut [TargetBlock<'_>],
    corpus_root: &Path,
    mutations: &[ScriptMutation],
    ui_regs: &[UiRegistration],
    layer_regs: &[LayerRegistration],
    order: &[String],
) -> Result<Vec<LinkedScript>, LinkError> {
    // Anything that lives in the load space — a UI widget or a layer activation — needs the loader
    // minted and the resident trampoline installed.
    let needs_loader = !ui_regs.is_empty() || !layer_regs.is_empty();
    // Fold the mod-loader trampoline in as a synthetic `wifpmcinterior` mutation when any load-space
    // mod registered — ONE line regardless of how many, so the resident never grows with mod count.
    // The expandable part is `qm_modloader`, minted after the base scripts link (below).
    let mut all_mutations: Vec<ScriptMutation> = mutations.to_vec();
    if needs_loader {
        all_mutations.push(ScriptMutation {
            shipment: "quartermaster-modloader".into(),
            target: "wifpmcinterior".into(),
            append: qm_trampoline_append(),
        });
    }

    // Group by target so each script is compiled ONCE with all its appends, which is the entire
    // point — compiling per Shipment would mean the last one wins again, just more slowly.
    let mut by_target: BTreeMap<&str, Vec<&ScriptMutation>> = BTreeMap::new();
    for m in &all_mutations {
        by_target.entry(m.target.as_str()).or_default().push(m);
    }

    let mut linked = Vec::new();
    for (target, group) in by_target {
        // Type-aware lookup: the resident block carries ~240 Lua chunks among ~7,000 entries of
        // other types, so a name-hash-only match could resolve to a texture.
        let (bi, idx) = blocks
            .iter()
            .enumerate()
            .find_map(|(bi, tb)| tb.block.find_script_by_name(target).map(|idx| (bi, idx)))
            .ok_or_else(|| LinkError::UnknownScript {
                target: target.to_string(),
                shipment: group[0].shipment.clone(),
            })?;
        let block = &mut *blocks[bi].block;
        let source_path =
            base_source_path(corpus_root, target).map_err(|tried| LinkError::NoBaseSource {
                target: target.to_string(),
                tried,
            })?;
        let base = std::fs::read_to_string(&source_path).map_err(|e| LinkError::Compile {
            target: target.to_string(),
            message: format!("reading base source {}: {e}", source_path.display()),
        })?;

        let (mut source, contributors) = linked_source(&base, &group, order);
        // Emitted once, AFTER every Shipment's append — see `derived_epilogue`. Putting it here
        // rather than in each Shipment is what makes "exactly once" structural.
        if let Some(epilogue) = derived_epilogue(target) {
            source.push_str(&epilogue);
        }
        // BARE chunk name — see the module note. `@name.lua` produces a chunk 5 bytes off retail.
        let bytecode = mercs2_luac::compile(&source, target).map_err(|e| LinkError::Compile {
            target: target.to_string(),
            message: e,
        })?;
        block
            .replace_lua(idx, &bytecode)
            .map_err(|e| LinkError::Splice {
                target: target.to_string(),
                message: e,
            })?;

        linked.push(LinkedScript {
            target: target.to_string(),
            contributors,
            base_source_bytes: base.len(),
            linked_source_bytes: source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }

    // Mint the mod loader. It is a NEW `scripts_vz` script — `add_script` appends its container and
    // (via `script_patch_blocks`' new-entry branch) its primary type-35 ASET row, the two halves the
    // DLC's own recipe ships. It goes in the block carrying `wifpmcinterior`, because `import` is
    // scripts_vz-only and that is where the trampoline calls it from.
    if needs_loader {
        let source = qm_modloader_source(ui_regs, layer_regs);
        // BARE chunk name, like every other script here — see the module note.
        let bytecode =
            mercs2_luac::compile(&source, QM_MODLOADER_NAME).map_err(|e| LinkError::Compile {
                target: QM_MODLOADER_NAME.to_string(),
                message: e,
            })?;
        let bi = blocks
            .iter()
            .position(|tb| tb.block.find_script_by_name("wifpmcinterior").is_some())
            .ok_or_else(|| LinkError::UnknownScript {
                target: "wifpmcinterior".to_string(),
                shipment: "quartermaster-modloader".to_string(),
            })?;
        blocks[bi]
            .block
            .add_script(QM_MODLOADER_NAME, &bytecode)
            .map_err(|m| LinkError::Splice {
                target: QM_MODLOADER_NAME.to_string(),
                message: m,
            })?;
        let mut contributors: Vec<String> = ui_regs
            .iter()
            .map(|r| r.shipment.clone())
            .chain(layer_regs.iter().map(|r| r.shipment.clone()))
            .collect();
        contributors.sort();
        contributors.dedup();
        linked.push(LinkedScript {
            target: QM_MODLOADER_NAME.to_string(),
            contributors,
            base_source_bytes: 0,
            linked_source_bytes: source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }

    // Every block we touched must still verify. `replace_lua` recomputes each container's CSUM, so
    // a failure here means the block itself was left inconsistent.
    for bi in linked.iter().map(|l| l.block).collect::<std::collections::BTreeSet<_>>() {
        blocks[bi]
            .block
            .verify_csums()
            .map_err(|e| LinkError::Block(format!("CSUMs after linking {}: {e}", blocks[bi].path)))?;
    }
    Ok(linked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mutation(shipment: &str, target: &str, append: &str) -> ScriptMutation {
        ScriptMutation {
            shipment: shipment.into(),
            target: target.into(),
            append: append.into(),
        }
    }

    /// ★ The preferred spelling `jen` must reach the RUNTIME key `jennifer`, or the outfit appends
    /// to a `_tOutfits.jen` the game never reads. This is the whole reason the split exists.
    #[test]
    fn jen_appends_to_the_jennifer_table() {
        let row = outfit_row_append("jen", "MyFit", "pmc_hum_jen", "[X]");
        assert!(row.contains("_tOutfits.jennifer"), "jen must normalize to jennifer: {row}");
        assert!(!row.contains("_tOutfits.jen,"), "must not emit the empty jen table: {row}");
        // The literal runtime key still works, and mattias/chris pass straight through.
        assert!(outfit_row_append("jennifer", "F", "m", "d").contains("_tOutfits.jennifer"));
        assert!(outfit_row_append("mattias", "F", "m", "d").contains("_tOutfits.mattias"));
        assert!(outfit_row_append("chris", "F", "m", "d").contains("_tOutfits.chris"));
    }

    /// The ordering property the whole design rests on: same set of Shipments, same bytes, whatever
    /// order they arrive in.
    #[test]
    fn concatenation_order_is_deterministic_regardless_of_input_order() {
        let a = mutation("aaa-mod", "s", "-- A\n");
        let b = mutation("zzz-mod", "s", "-- Z\n");
        let (one, c1) = linked_source("base\n", &[&a, &b], &[]);
        let (two, c2) = linked_source("base\n", &[&b, &a], &[]);
        assert_eq!(one, two, "install order must not change the linked source");
        assert_eq!(c1, c2);
        assert_eq!(c1, vec!["aaa-mod", "zzz-mod"]);
        // Both appends must survive — this is the annihilation the linker exists to prevent.
        assert!(one.contains("-- A") && one.contains("-- Z"), "{one}");
    }

    /// A base script with no trailing newline must not weld into the first append.
    #[test]
    fn a_base_without_a_trailing_newline_is_separated() {
        let m = mutation("mod", "s", "print('x')\n");
        let (src, _) = linked_source("local t = 1", &[&m], &[]);
        assert!(src.starts_with("local t = 1\n"), "{src}");
        assert!(
            !src.contains("local t = 1--"),
            "appended source welded onto the base: {src}"
        );
    }

    #[test]
    fn each_append_is_attributed_in_the_source() {
        let m = mutation("sean-devlin", "s", "-- outfit\n");
        let (src, _) = linked_source("base\n", &[&m], &[]);
        assert!(src.contains("appended by Shipment: sean-devlin"), "{src}");
    }

    fn with_load(name: &str, after: &[&str], before: &[&str]) -> (String, Load) {
        (
            name.to_string(),
            Load {
                after: after.iter().map(|s| s.to_string()).collect(),
                before: before.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
        )
    }

    /// With no constraints the order is pure alphabetical — the stable base every install shares.
    #[test]
    fn load_order_defaults_to_alphabetical() {
        let ships = [with_load("zulu", &[], &[]), with_load("alpha", &[], &[]), with_load("mike", &[], &[])];
        assert_eq!(resolve_load_order(&ships).unwrap(), vec!["alpha", "mike", "zulu"]);
    }

    /// `after` / `before` constrain that base, and the result is the same whatever order the set is
    /// presented in — the determinism the whole design rests on, now with constraints.
    #[test]
    fn after_and_before_constrain_the_alphabetical_base() {
        // `zulu` must load before `alpha` (via before), and `mike` after `alpha` (via after).
        let a = [
            with_load("alpha", &[], &[]),
            with_load("mike", &["alpha"], &[]),
            with_load("zulu", &[], &["alpha"]),
        ];
        let want = vec!["zulu".to_string(), "alpha".to_string(), "mike".to_string()];
        assert_eq!(resolve_load_order(&a).unwrap(), want);
        // Same set, reversed input — identical output.
        let b = [
            with_load("zulu", &[], &["alpha"]),
            with_load("mike", &["alpha"], &[]),
            with_load("alpha", &[], &[]),
        ];
        assert_eq!(resolve_load_order(&b).unwrap(), want);
    }

    /// `after` and `before` are two spellings of one edge and must agree: X.before=[Y] and
    /// Y.after=[X] both mean X loads first.
    #[test]
    fn before_and_after_are_symmetric() {
        let via_before = [with_load("x", &[], &["y"]), with_load("y", &[], &[])];
        let via_after = [with_load("x", &[], &[]), with_load("y", &["x"], &[])];
        assert_eq!(resolve_load_order(&via_before).unwrap(), vec!["x", "y"]);
        assert_eq!(resolve_load_order(&via_after).unwrap(), vec!["x", "y"]);
    }

    /// A constraint naming a Shipment that is not installed is inert — you cannot order against what
    /// is not there — so the rest still order alphabetically rather than failing.
    #[test]
    fn a_constraint_on_an_absent_shipment_is_inert() {
        let ships = [with_load("beta", &["not-installed"], &[]), with_load("alpha", &[], &[])];
        assert_eq!(resolve_load_order(&ships).unwrap(), vec!["alpha", "beta"]);
    }

    /// A cycle has no valid order, so it is a named error rather than an arbitrary pick.
    #[test]
    fn a_cycle_is_a_named_error() {
        let ships = [with_load("a", &["b"], &[]), with_load("b", &["a"], &[])];
        match resolve_load_order(&ships) {
            Err(LinkError::LoadCycle { names }) => {
                assert!(names.contains(&"a".to_string()) && names.contains(&"b".to_string()));
            }
            other => panic!("expected a LoadCycle, got {other:?}"),
        }
    }

    /// Load order actually reorders the appends: `b` forced before `a` overrides the alphabetical
    /// default, so `b`'s source concatenates first.
    #[test]
    fn load_order_reorders_the_appends() {
        let a = mutation("a-mod", "s", "-- A\n");
        let b = mutation("b-mod", "s", "-- B\n");
        // Default (empty order) is alphabetical: A then B.
        let (def, _) = linked_source("base\n", &[&a, &b], &[]);
        assert!(def.find("-- A").unwrap() < def.find("-- B").unwrap());
        // Force b before a.
        let order = vec!["b-mod".to_string(), "a-mod".to_string()];
        let (forced, contrib) = linked_source("base\n", &[&a, &b], &order);
        assert!(forced.find("-- B").unwrap() < forced.find("-- A").unwrap(), "{forced}");
        assert_eq!(contrib, vec!["b-mod", "a-mod"]);
    }

    fn reg(shipment: &str, movie: &str) -> UiRegistration {
        UiRegistration {
            shipment: shipment.into(),
            movie: movie.into(),
        }
    }

    /// The loader defines the global the trampoline depends on, exposes `run`, and enrols each
    /// movie's FlashWidget under the proven creation sequence.
    #[test]
    fn the_mod_loader_defines_qm_and_registers_each_movie() {
        let src = qm_modloader_source(&[reg("mod-a", "my_hud"), reg("mod-b", "my_map")], &[]);
        assert!(src.contains("_QM = _QM or"), "must define the _QM global: {src}");
        assert!(src.contains("function _QM.run()"), "must expose run(): {src}");
        // Each movie enrols via the loadingscreen_standalone sequence.
        for movie in ["my_hud", "my_map"] {
            assert!(src.contains(&format!("w:SetSwfFile(\"{movie}\")")), "missing {movie}: {src}");
        }
        assert_eq!(src.matches("FlashWidget:new()").count(), 2, "one widget per movie: {src}");
        // Guarded so a re-import or a re-entered interior is a no-op, and one bad movie is contained.
        assert!(src.contains("if _QM._ran then return end"), "run must be once-only: {src}");
        assert!(src.contains("pcall(f)"), "each init must be fail-soft: {src}");
    }

    /// Same registrations, same bytes, whatever order they arrive in — the determinism rule the whole
    /// linker rests on, applied to the bake.
    #[test]
    fn the_bake_is_order_independent() {
        let a = qm_modloader_source(&[reg("aaa", "one"), reg("zzz", "two")], &[]);
        let b = qm_modloader_source(&[reg("zzz", "two"), reg("aaa", "one")], &[]);
        assert_eq!(a, b, "install order must not change the baked loader");
        // ordered by (shipment, movie): aaa/one appears before zzz/two.
        assert!(a.find("one").unwrap() < a.find("two").unwrap(), "{a}");
    }

    /// A movie name cannot break out of its Lua string and inject code into the block we compile.
    #[test]
    fn a_movie_name_is_escaped_in_the_bake() {
        let src = qm_modloader_source(&[reg("m", "evil\") os.exit() --")], &[]);
        // The escaped form keeps the payload INSIDE the string literal ...
        assert!(src.contains("evil\\\") os.exit()"), "the embedded quote must be escaped: {src}");
        // ... and the unescaped breakout (a bare `evil") ` that would end the string early) is absent.
        assert!(!src.contains("(\"evil\") os"), "the injection must not close the string: {src}");
    }

    fn layer(shipment: &str, add: &str, remove: &[&str]) -> LayerRegistration {
        LayerRegistration {
            shipment: shipment.into(),
            add: add.into(),
            remove: remove.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// A layer activation bakes MarkForRemoval(old) then MarkForAddition(new) into the same guarded,
    /// pcall-wrapped `_QM._inits` list the UI widgets use — the vanilla contract order.
    #[test]
    fn the_mod_loader_marks_layers_for_addition_and_removal() {
        let src = qm_modloader_source(
            &[],
            &[layer("act-mod", "vz_state_pmccon004_destroyed", &["vz_state_pmccon004_pristine"])],
        );
        assert!(src.contains("_QM = _QM or"), "must still define the _QM global: {src}");
        assert!(
            src.contains("MrxLayerManager.MarkForAddition(\"vz_state_pmccon004_destroyed\")"),
            "must add the new layer: {src}"
        );
        assert!(
            src.contains("MrxLayerManager.MarkForRemoval(\"vz_state_pmccon004_pristine\")"),
            "must remove the superseded layer: {src}"
        );
        // Remove precedes add — the vanilla order so the two never both apply.
        assert!(
            src.find("MarkForRemoval").unwrap() < src.find("MarkForAddition").unwrap(),
            "removal must be baked before addition: {src}"
        );
        // Guarded and fail-soft like the widgets.
        assert!(src.contains("if MrxLayerManager then"), "existence-checked: {src}");
        assert!(src.contains("pcall(f)"), "runs under the shared pcall: {src}");
    }

    /// UI and layer registrations coexist in one loader, and the same set bakes byte-identically
    /// whatever order it arrives in.
    #[test]
    fn ui_and_layer_registrations_share_one_deterministic_loader() {
        let a = qm_modloader_source(
            &[reg("ui-mod", "my_hud")],
            &[layer("aaa", "layer_a", &[]), layer("zzz", "layer_z", &[])],
        );
        let b = qm_modloader_source(
            &[reg("ui-mod", "my_hud")],
            &[layer("zzz", "layer_z", &[]), layer("aaa", "layer_a", &[])],
        );
        assert_eq!(a, b, "install order must not change the baked loader");
        assert!(a.contains("w:SetSwfFile(\"my_hud\")"), "the widget is still baked: {a}");
        assert!(a.find("layer_a").unwrap() < a.find("layer_z").unwrap(), "ordered by (shipment,add): {a}");
    }

    /// A layer name cannot break out of its Lua string and inject code into the block we compile.
    #[test]
    fn a_layer_name_is_escaped_in_the_bake() {
        let src = qm_modloader_source(&[], &[layer("m", "evil\") os.exit() --", &[])]);
        assert!(src.contains("evil\\\") os.exit()"), "the embedded quote must be escaped: {src}");
        assert!(!src.contains("Addition(\"evil\") os"), "the injection must not close the string: {src}");
    }

    /// The trampoline is exactly what the resident carries: it wraps `_OnEnter`, imports the loader
    /// by name (so `import` hashes it to the ASET row `add_script` mints), and runs it once.
    #[test]
    fn the_trampoline_is_one_self_contained_hook() {
        let t = qm_trampoline_append();
        assert!(t.contains("_OnEnter = function"), "must wrap the entry hook: {t}");
        assert!(t.contains("import(\"qm_modloader\")"), "must import by name: {t}");
        assert!(t.contains("_QM.run()"), "must run the loader: {t}");
        // It calls the previous _OnEnter, so wrapping it never drops the game's own behaviour.
        assert!(t.contains("_qm_prev_OnEnter"), "must chain the prior hook: {t}");
    }
}
