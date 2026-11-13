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

use mercs2_formats::scripts_block::ScriptsBlock;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Why `name` cannot be a bare chunk name, or `None` when it can.
///
/// Retail's LuaQ headers store the bare script name (the module note above): no `@` prefix and no
/// `.lua` suffix. Lua 5.1 treats a leading `@` (a file name) and a leading `=` (a literal) as sigils
/// when it prints a chunk name, so neither may start one. A name is also one path component, never
/// empty, and carries no NUL. `qm compile-lua` checks the chunk name it compiles under with this.
pub fn chunk_name_refusal(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("it is empty".into());
    }
    if name.starts_with('@') || name.starts_with('=') {
        return Some(format!(
            "it starts with {:?}, which Lua reads as a chunk-name sigil; retail chunk names are the \
             bare script name",
            &name[..1]
        ));
    }
    if name.to_ascii_lowercase().ends_with(".lua") {
        return Some(
            "it ends in `.lua`; retail chunk names are the bare script name, with no extension".into(),
        );
    }
    if name.contains('\0') {
        return Some("it contains a NUL byte".into());
    }
    if name.contains('/') || name.contains('\\') {
        return Some("it contains a path separator; a chunk name is one bare name".into());
    }
    None
}

/// A level WAD whose scripts the linker edits: `vz.wad` (gameplay) or `shell.wad` (the front end).
///
/// The two never share a mount slot (`fixpack/wad_duplicate_inventory.md` §B.5), and each level runs
/// its own Lua VM, so a script is edited per level and one level's edit never reaches the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Vz,
    Shell,
}

impl Level {
    /// The level whose Lua VM runs `session`.
    pub const fn of(session: crate::manifest::LoadSession) -> Level {
        match session {
            crate::manifest::LoadSession::Gameplay => Level::Vz,
            crate::manifest::LoadSession::FrontEnd => Level::Shell,
        }
    }

    /// The level's WAD, for messages.
    pub const fn wad(self) -> &'static str {
        match self {
            Level::Vz => "vz.wad",
            Level::Shell => "shell.wad",
        }
    }
}

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
/// ⚠ **The resident needle is ANCHORED on purpose, folder included.** `block_by_path` matches a
/// substring over the stack from the top, and `resident_P000_Q3` alone also matches
/// `sound_resident_P000_Q3.block` — a completely different block — while `\resident_P000_Q3.block`
/// also matches `English.wad`'s `blocks\English\resident_P000_Q3.block`, which sits above `vz.wad`
/// in a stack that holds a language WAD. The front end's scripts are in `shell.wad`, which never
/// shares a mount slot with `vz.wad`; they are [`SHELL_SCRIPT_BLOCKS`], read from `shell.wad` and
/// shipped in the shell patch.
pub const SCRIPT_BLOCKS: &[(&str, &str)] = &[
    ("scripts_vz", r"blocks\VZ\scripts_vz_P000_Q3.block"),
    (r"\VZ\resident_P000_Q3.block", r"blocks\VZ\resident_P000_Q3.block"),
];

/// The front end's scripts block in `shell.wad`, as `(PTHS needle, PTHS path)`: the one block of
/// the 28 shell scripts, `MrxSound` among them.
pub const SHELL_SCRIPT_BLOCKS: &[(&str, &str)] =
    &[(r"\Shell\resident_P000_Q3.block", r"blocks\Shell\resident_P000_Q3.block")];

/// The script the engine itself queues first in the front end: pc-v10 `0x4C05E0`–`0x4C05EC` queues
/// `0x49031655` when `Sys.AutoLoad` is clear, which it is in retail. Each GUI layout loads after it,
/// by name (`dynamic_import` through `MrxGuiBase.LoadGUIFile`).
pub const FRONT_END_ROOTS: &[&str] = &["shellbootstrap"];

/// The script the engine queues first in the world load, before the level's master: pc-v10's
/// shell-to-game stepper requests `0x40CC2CB3` as a script (`0x4BC94D`), and its pass finishes before
/// `LevelBootstrap.LoadLevel` requests the master (live dump: its pass 29, `vz`'s 43).
pub const WORLD_BOOT_ROOT: &str = "gamebootstrap";

/// The corpus subfolders the `vz.wad` blocks' base sources are searched in, in order.
pub const VZ_SOURCE_DIRS: &[&str] = &["vz", "resident", "shell"];

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

/// One Shipment's declaration of a NOVEL script module — a whole new `import`-able Lua module
/// that mints its own `scripts_vz` entry and ASET row. Compiled to LuaQ at link time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptAddition {
    pub shipment: String,
    /// The module name (`import("<name>")` / `dynamic_import("<name>")` / `sModuleName`). Bare,
    /// no extension. Hashed with `pandemic_hash_m2` to become the ASET row's key.
    pub name: String,
    /// Full Lua source text to compile.
    pub source: String,
    /// When qm's loader brings the module up, for a library ([`crate::manifest::LibraryLoad`]).
    pub load: Option<crate::manifest::LibraryLoad>,
}

/// One Shipment's declaration of a wholesale REPLACE on an existing script's bytecode.
/// Same asset hash, new body; the last-mounted replace wins if two Shipments claim one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptReplacement {
    pub shipment: String,
    /// The shipped script to replace, e.g. `wifpmcinterior`.
    pub target: String,
    /// Full Lua source text that becomes the new bytecode.
    pub source: String,
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
    /// A contributor that is not in the resolved `order`. Every real contributor comes from a
    /// Shipment in the set, so this is an internal error in whatever built `order`, never an
    /// author's mistake — and never something to paper over with a fallback position.
    NotInOrder {
        shipment: String,
    },
    /// A script the link mints — an `add_script` module, a novel shop module, `qm_modloader` —
    /// whose name a script of one of the blocks already carries (M0197). The registry keeps the
    /// first script of a name, so one of the two would not be the one that loads.
    NameTaken {
        name: String,
        shipment: String,
        /// The PTHS path of the block that carries it.
        path: String,
    },
    /// One novel shop module minted from two sources: by two Shipments (M0207), or by one with two
    /// different `behaviour.script` files (M0120). Only one source can be the module.
    SupportModuleTwice {
        module: String,
        first: String,
        second: String,
    },
    /// qm's hook and library code could not be generated for this set ([`crate::loader::host_tails`]):
    /// a library at a hook qm does not generate, or in a session that hook does not fire in.
    Hooks(String),
    /// The set's outfits could not be linked into a level ([`crate::outfits`]): its wardrobe or player
    /// script could not be read, an outfit was refused, or the heroes' own chatter sets could not be
    /// named.
    Outfits(String),
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
            LinkError::NotInOrder { shipment } => write!(
                f,
                "internal error: {shipment} contributes to the link but is not in the resolved load \
                 order — the order must name every Shipment in the set"
            ),
            LinkError::NameTaken { name, shipment, path } => write!(
                f,
                "[M0197] {shipment} mints the script {name:?}, but {path} already carries a script \
                 of that name; the registry keeps the first, so one of the two would not be the one \
                 that loads — rename the module"
            ),
            LinkError::SupportModuleTwice { module, first, second } => write!(
                f,
                "the novel shop module {module:?} is minted from two sources, by {first} and by \
                 {second}; one module name has one source — rename one"
            ),
            LinkError::Hooks(m) => write!(f, "qm's hooks and library loader: {m}"),
            LinkError::Outfits(m) => write!(f, "the set's outfits: {m}"),
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

/// What [`link_into_blocks`] produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkOutput {
    /// Every script linked, replaced or minted.
    pub scripts: Vec<LinkedScript>,
    /// The import findings of the Shipment Lua the link put into scripts ([`crate::imports::check`]):
    /// M0209, M0313 and M0314, sorted. The caller decides what each refuses.
    pub findings: Vec<crate::imports::ImportFinding>,
    /// Every script module the link added, for the load plan's `libraries`: the libraries qm's loader
    /// loads, in the order it loads them, then the modules only an `import` loads, in mint order.
    pub modules: Vec<LinkedModule>,
}

/// One script module a link added to a session: a row of the load plan's `libraries`
/// (`mercs2_formats::load_plan::LibraryRow`, which names the Shipment by its request id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedModule {
    pub module: String,
    /// The Shipment that adds it; `None` for qm's own loaders.
    pub shipment: Option<String>,
    pub session: crate::manifest::LoadSession,
    /// The hook qm's loader loads it at; `None` when only an `import` loads it.
    pub when: Option<String>,
    /// Whether the session's settle hook checks it is ready ([`crate::loader::settles`]).
    pub settles: bool,
}

/// The rows of the libraries a link loads in `session`, from the list it splices: `qm`, qm's own
/// loader when the link loads one, first, then the Shipments' `libraries`.
fn library_modules(
    session: crate::manifest::LoadSession,
    qm: Option<&crate::loader::Library>,
    libraries: &[crate::loader::Library],
) -> Result<Vec<LinkedModule>, LinkError> {
    let row = |lib: &crate::loader::Library, shipment: Option<String>| {
        let hook = crate::hooks::hook(&lib.when)
            .ok_or_else(|| LinkError::Hooks(format!("hook {} is not in qm's catalogue (data/hooks.tsv)", lib.when)))?;
        Ok(LinkedModule { module: lib.module.clone(), shipment, session, when: Some(lib.when.clone()), settles: crate::loader::settles(hook) })
    };
    let mut rows = Vec::with_capacity(libraries.len() + 1);
    if let Some(q) = qm {
        rows.push(row(q, None)?);
    }
    for lib in libraries {
        rows.push(row(lib, Some(lib.owner.clone()))?);
    }
    Ok(rows)
}

/// The row of a module the link mints that only an `import` loads.
fn imported_module(module: &str, shipment: &str, session: crate::manifest::LoadSession) -> LinkedModule {
    LinkedModule { module: module.to_string(), shipment: Some(shipment.to_string()), session, when: None, settles: false }
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
/// Searched in `subfolders`, in order, then the stub directory. The `vz.wad` blocks pass
/// [`VZ_SOURCE_DIRS`]. On a miss, every path tried comes back.
pub fn base_source_path(corpus_root: &Path, subfolders: &[&str], target: &str) -> Result<PathBuf, Vec<PathBuf>> {
    let mut tried = Vec::new();
    for sub in subfolders {
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

/// The name the synthetic mod-loader trampoline contributes under. It is not a Shipment, so it is
/// never in a resolved `order`; it sorts after every real contributor.
pub const MODLOADER_CONTRIBUTOR: &str = "quartermaster-modloader";

/// One ordering edge between two request items (indices into the request): `first` loads before
/// `then`. Built from `requires` alone (a provider before its consumer); nothing else orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderEdge {
    pub first: usize,
    pub then: usize,
}

/// A resolved load order over request items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOrder {
    /// Request indices, in load order. Every index appears exactly once.
    pub order: Vec<usize>,
    /// Per request index: the edge that last delayed the item, when an item with a LARGER request
    /// index loaded before it; otherwise `None`.
    pub held_back_by: Vec<Option<usize>>,
}

/// The load order over `n` request items and the `requires` edges between them.
///
/// A Kahn topological sort. Whenever more than one item is ready, the one with the LOWEST request
/// index goes first, so the request order (the player's list) breaks every tie and declared edges
/// always beat it. The result is a function of the request and its edges alone.
///
/// A cycle has no order: the error carries the edges on each cycle (indices into `edges`), one list
/// per strongly connected tangle, so every one can be named rather than an arbitrary pick made.
pub fn resolve_load_order(n: usize, edges: &[OrderEdge]) -> Result<ResolvedOrder, Vec<Vec<usize>>> {
    let mut indeg = vec![0usize; n];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in edges.iter().enumerate() {
        indeg[e.then] += 1;
        outgoing[e.first].push(i);
    }
    let mut ready: BTreeSet<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut position: Vec<Option<usize>> = vec![None; n];
    while let Some(&next) = ready.iter().next() {
        ready.remove(&next);
        position[next] = Some(order.len());
        order.push(next);
        for &ei in &outgoing[next] {
            let t = edges[ei].then;
            indeg[t] -= 1;
            if indeg[t] == 0 {
                ready.insert(t);
            }
        }
    }
    if order.len() != n {
        return Err(cycle_edges(n, edges, &position));
    }

    let mut held_back_by = vec![None; n];
    for (pos, &x) in order.iter().enumerate() {
        if !order[..pos].iter().any(|&y| y > x) {
            continue;
        }
        // X waited on something: an item with a larger request index went first, which only an
        // incoming edge can cause. Name the edge from its last-emitted predecessor (the lowest-index
        // edge when there are several between the same pair).
        let last = edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.then == x)
            .max_by_key(|(i, e)| (position[e.first], std::cmp::Reverse(*i)))
            .map(|(i, _)| i)
            .expect("an item that is held back has at least one incoming edge");
        held_back_by[x] = Some(last);
    }
    Ok(ResolvedOrder {
        order,
        held_back_by,
    })
}

/// The edges on a cycle, grouped by strongly connected component, among the items the sort could
/// not place (`position[i] == None`).
///
/// An unplaced item is on a cycle or downstream of one; only edges whose ends reach each other are
/// cycle edges. Groups are sorted by their lowest item index, edges within a group by index.
fn cycle_edges(n: usize, edges: &[OrderEdge], position: &[Option<usize>]) -> Vec<Vec<usize>> {
    let stuck: Vec<bool> = (0..n).map(|i| position[i].is_none()).collect();
    // reach[a] = every stuck item reachable from `a` through stuck items.
    let reach: Vec<BTreeSet<usize>> = (0..n)
        .map(|a| {
            let mut seen = BTreeSet::new();
            if !stuck[a] {
                return seen;
            }
            let mut stack = vec![a];
            while let Some(u) = stack.pop() {
                for e in edges.iter().filter(|e| e.first == u && stuck[e.then]) {
                    if seen.insert(e.then) {
                        stack.push(e.then);
                    }
                }
            }
            seen
        })
        .collect();
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, e) in edges.iter().enumerate() {
        if stuck[e.first] && stuck[e.then] && reach[e.then].contains(&e.first) {
            // The component's representative: its lowest item index. Both ends of a cycle edge
            // share one component, so either end names it.
            let rep = (0..n)
                .find(|&m| m == e.first || (reach[m].contains(&e.first) && reach[e.first].contains(&m)))
                .expect("e.first is itself a candidate");
            groups.entry(rep).or_default().push(i);
        }
    }
    groups.into_values().collect()
}

/// An item's position in `order`, for sorting what it contributed.
///
/// The synthetic mod-loader trampoline ([`MODLOADER_CONTRIBUTOR`]) sorts after every real
/// contributor. Any other name missing from `order` is [`LinkError::NotInOrder`].
fn order_rank(order: &[String], shipment: &str) -> Result<usize, LinkError> {
    if shipment == MODLOADER_CONTRIBUTOR {
        return Ok(order.len());
    }
    order
        .iter()
        .position(|n| n == shipment)
        .ok_or_else(|| LinkError::NotInOrder {
            shipment: shipment.to_string(),
        })
}

/// Stable-sort `items` by their contributor's position in `order`, keeping each contributor's own
/// relative order, then by `key` within a contributor.
fn sort_by_order<T, K: Ord>(
    items: &mut Vec<&T>,
    order: &[String],
    shipment: impl Fn(&T) -> &str,
    key: impl Fn(&T) -> K,
) -> Result<(), LinkError> {
    let mut ranked = Vec::with_capacity(items.len());
    for it in items.drain(..) {
        ranked.push((order_rank(order, shipment(it))?, it));
    }
    ranked.sort_by(|(ra, a), (rb, b)| ra.cmp(rb).then_with(|| key(a).cmp(&key(b))));
    items.extend(ranked.into_iter().map(|(_, it)| it));
    Ok(())
}

/// Concatenate the base source with every mutation's append, in the resolved load order.
///
/// `order` is the resolved sequence of Shipment names (the load plan's `order`). Appends follow it;
/// one Shipment's own appends keep their manifest order. The synthetic mod-loader trampoline goes
/// last, and any other contributor missing from `order` is [`LinkError::NotInOrder`].
pub fn linked_source(
    base: &str,
    mutations: &[&ScriptMutation],
    order: &[String],
) -> Result<(String, Vec<String>), LinkError> {
    let mut ordered: Vec<&ScriptMutation> = mutations.to_vec();
    sort_by_order(&mut ordered, order, |m| m.shipment.as_str(), |_| ())?;

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
    Ok((out, contributors))
}

/// The `tUnlockStatus` table literal for a shop item: `{ <Fac> = 1, … }` when unlocked in every
/// listed vendor, or `{}` when it should ship locked. Locked matters only in Eva's obscured shop
/// (where a locked item is unbuyable); in the five faction shops a locked item is still purchasable.
pub fn shop_unlock_table(shops: &[crate::manifest::ShopVendor], unlocked: bool) -> String {
    if !unlocked {
        return "{}".to_string();
    }
    let mut s = String::from("{ ");
    for (i, v) in shops.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("{} = 1", v.faction_key()));
    }
    s.push_str(" }");
    s
}

/// A SUPPORT-catalog row appended to `mrxsupportdata` for one shop item.
///
/// Wrapped in a function called where it is defined, so the `local oSupport` is scoped per item and N
/// appended rows never collide, and so the INFO globals count is computable: that model runs only the
/// main function and takes every call to return nil, so `oSupport:SetCargo` on the result of
/// `Create()` in the main function is "a method call on nil" to it
/// ([`mercs2_formats::script_meta::count_globals`]); the row sets no global. The function inherits
/// the module's environment, as any function the main function creates does. `tSupportData` is a
/// module-global, so a top-level append lands before `Init()`'s auto-name loop runs — the loop then
/// picks the row up. The item key uses bracket form so an arbitrary author id is safe.
#[allow(clippy::too_many_arguments)]
pub fn shop_support_row_append(
    id: &str,
    name: &str,
    description: &str,
    icon: &str,
    item_type: &str,
    cash_cost: u64,
    fuel_cost: u64,
    max_stock: u32,
    unlock_table: &str,
    module: &str,
    cargo: Option<&str>,
    delivery_vehicle: Option<&str>,
) -> String {
    let mut b = String::new();
    // Inside `do`, the `(` cannot be read as a call on whatever the appended-to source ends with.
    b.push_str("do (function()\n");
    b.push_str(&format!("  local oSupport = {module}:Create()\n"));
    if let Some(c) = cargo {
        b.push_str(&format!("  oSupport:SetCargo({})\n", lua_string(c)));
    }
    if let Some(dv) = delivery_vehicle {
        b.push_str(&format!("  oSupport:SetDeliveryVehicle({})\n", lua_string(dv)));
    }
    b.push_str(&format!(
        "  tSupportData[{}] = {{ sName = {}, sDescription = {}, sIcon = {}, nMaxStock = {}, \
         nCashCost = {}, nFuelCost = {}, oSupport = oSupport, sType = {}, tUnlockStatus = {} }}\n",
        lua_string(id),
        lua_string(name),
        lua_string(description),
        lua_string(icon),
        max_stock,
        cash_cost,
        fuel_cost,
        lua_string(item_type),
        unlock_table,
    ));
    b.push_str("end)() end\n");
    b
}

/// An EQUIPMENT-catalog row appended to `wifequipmentdata`. `_tEquipment` is a module-global; only
/// fuel-tank / grapple `nType`s are ever shopped (the `mrxshop` insert gate), which is what
/// `ntype_const` is restricted to.
pub fn shop_equipment_row_append(
    id: &str,
    name: &str,
    description: &str,
    texture: &str,
    ntype_const: &str,
    cost: u64,
) -> String {
    format!(
        "_tEquipment[{}] = {{ sName = {}, sDescription = {}, sTexture = {}, nType = {}, nCost = {} }}\n",
        lua_string(id),
        lua_string(name),
        lua_string(description),
        lua_string(texture),
        ntype_const,
        cost,
    )
}

/// The `mrxrewarddata` reward rows that SURFACE a shop item — one per vendor faction, because
/// `GetAllPotentialShopItems(f)` matches a row's `sFactionId` against exactly one faction. `field`
/// is `tSupport` (support catalog) or `tEquipment` (equipment catalog).
pub fn shop_reward_append(id: &str, field: &str, shops: &[crate::manifest::ShopVendor]) -> String {
    let mut out = String::new();
    for v in shops {
        let fac = v.faction_key();
        out.push_str(&format!(
            "_tRewards[{}] = {{ sFactionId = {}, {field} = {{ {{ {}, {} }} }} }}\n",
            lua_string(&format!("{id}Reward_{fac}")),
            lua_string(fac),
            lua_string(id),
            lua_string(fac),
        ));
    }
    out
}

/// The name of the Quartermaster mod-loader script — a NEW `scripts_vz` script the linker mints and
/// `add_script`s into the block, distinct from any retail script. Its ASET row (type 35, primary)
/// is emitted automatically by `script_patch_blocks`' new-entry branch, which is the other half of
/// the DLC's own recipe for a new importable script (`dlc_aset_normalize.py`).
///
/// `import` resolves a module by `(hash of its name, script type 0x42498680)` through the typed-asset
/// lookup, with no block or WAD in the key (`_SYS._IMPORT` = `FUN_005AE2D0`, decomp 219707-219760),
/// so a script minted into any mounted block resolves. It lands in the block that carries
/// `wifpmcinterior`, `scripts_vz`, beside every other script the link mints. qm's world entry
/// (`world.scripts_loaded`) loads it as a library ([`crate::loader`]).
pub const QM_MODLOADER_NAME: &str = "qm_modloader";

/// The name of the front end's mod loader — a NEW script the linker mints into `shell.wad`'s
/// scripts block ([`SHELL_SCRIPT_BLOCKS`]). qm's front-end entry (`shell.boot`) loads it as a library,
/// and it loads and unloads every front-end sound bank ([`qm_shell_modloader_source`]).
pub const QM_SHELL_MODLOADER_NAME: &str = "qm_shell_modloader";

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

/// One novel-behaviour shop item — a `MrxSupport` subclass shipped as source, plus the catalog row
/// that must be DEFERRED into the loader (its `module:Create()` cannot run at resident-load time).
///
/// The linker mints `source` as a new `scripts_vz` script named `module` (like `qm_modloader`
/// itself), then bakes an `_inits` closure that `import`s it and constructs the catalog + reward
/// rows post-world-load. Ordered by `(shipment, id)` for a byte-identical bake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportRegistration {
    pub shipment: String,
    /// Runtime module name — both the minted script name and what the loader `import`s.
    pub module: String,
    /// The novel subclass Lua source, compiled + minted as a new `scripts_vz` script.
    pub source: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    /// `sType` string (already validated to the closed set by the manifest layer).
    pub item_type: String,
    pub cash_cost: u64,
    pub fuel_cost: u64,
    pub max_stock: u32,
    /// The `tUnlockStatus` table literal (from `shop_unlock_table`).
    pub unlock_table: String,
    pub cargo: Option<String>,
    pub delivery_vehicle: Option<String>,
    /// Vendor faction keys (Capitalized) for the deferred reward rows.
    pub shops: Vec<String>,
}

/// One sound bank a loader must load: an `add_sound` bank (its wavebank and soundbank), or the
/// wavebank that carries a sound override's waves (its wavebank only — the overridden soundbank is
/// one retail Lua already loads). A bank is only heard once it is loaded, and retail loads its own
/// banks by name through `MrxSoundBanks.LoadWaveBank` / `LoadSoundBank` (`mrxsoundbootstrap.lua`,
/// `mrxsound.lua`), so the loaders do the same for these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundBankRegistration {
    /// Which Shipment asked for it — the tie-break for the deterministic bake order.
    pub shipment: String,
    /// The bank name `MrxSoundBanks` is given; for a stream, the stream's name without `.pws`.
    pub bank: String,
    /// What the loader loads under that name.
    pub load: SoundLoad,
    /// The sessions whose loader loads it: the gameplay loader ([`qm_modloader_source`]) and the
    /// front end's ([`qm_shell_modloader_source`]).
    pub sessions: BTreeSet<crate::manifest::LoadSession>,
}

/// What a [`SoundBankRegistration`] has its loader load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundLoad {
    /// A bank qm ships: its wavebank, and its soundbank as well when `soundbank` (an `add_sound` bank).
    Shipped { soundbank: bool },
    /// The soundbank of a bank qm does not ship: the game's, or a level asset's (`load_sound`).
    SoundBank,
    /// The wavebank of a bank qm does not ship (`load_sound`).
    WaveBank,
    /// A stream file (`load_sound`), opened before any bank of the session loads, as retail opens its
    /// own (`mrxsoundbanks.lua:89-108`): `<folder>\<audio dir>\<localized name>.pws`, under the alias
    /// `<name>.pws`, and closed after the session's banks unload.
    Stream { folder: String },
}

/// The registrations of `regs` that `session`'s loader loads, in load `order`, then by bank name.
fn session_sounds<'a>(
    regs: &'a [SoundBankRegistration],
    session: crate::manifest::LoadSession,
    order: &[String],
) -> Result<Vec<&'a SoundBankRegistration>, LinkError> {
    let mut out: Vec<&SoundBankRegistration> = regs.iter().filter(|r| r.sessions.contains(&session)).collect();
    sort_by_order(&mut out, order, |r| r.shipment.as_str(), |r| r.bank.clone())?;
    Ok(out)
}

/// The hook subscriptions that load and unload `regs`' banks, at `load` and `unload`: the ONE
/// generator both loaders use. Each bank's two handlers are subscribed under its Shipment's name, so a
/// failure names the Shipment.
///
/// Each bank is loaded with the calls retail makes for its own banks — the wavebank, then, for an
/// `add_sound` bank, the soundbank (`mrxsoundbootstrap.lua:195-217`) — and unloaded with their
/// mirrors, in `regs` order. `QM.sounds[bank]` records a load, so a second `load` in one session loads
/// nothing again and `unload` unloads only what was loaded. No callback is passed: `MrxSoundBanks`
/// keeps ONE batch callback (`_funcBatchComplete`, `mrxsoundbanks.lua:10-36`) and a new one replaces
/// the caller's, which for the front end is retail's `_StartShellMusic`. `MrxSoundBanks` reports no
/// failure: `_FlagAssetOpComplete` takes no argument (`mrxsoundbanks.lua:141-152`) and the engine's
/// callback carries no flag (`audio_code_map.md` §11.2), so a bank's presence is checked at build time
/// ([`crate::sound::check_loader_banks`]).
///
/// A stream ([`SoundLoad::Stream`]) is opened before every bank and closed after every bank unloads, as
/// retail opens its streams before it loads a bank (`mrxsoundbanks.lua:110-111`): its handlers are
/// subscribed first for the load and last for the unload. The path is the one the DLC opens its own
/// streams by (`dlctest01_soundbootstrap.lua:186-193`), with the folder in place of
/// `Net.DLCGetMountPoint()`, the name localized by retail's `MrxSoundBanks._GetLocalizedName`.
fn sound_subscriptions(load: &str, unload: &str, regs: &[&SoundBankRegistration]) -> String {
    let (load, unload) = (lua_string(load), lua_string(unload));
    let subscribe = |hook: &str, key: &str, test: &str, calls: &str, mark: &str, owner: &str| {
        format!("QM.on({hook}, function()\n  if {test}QM.sounds[{key}] then\n{calls}    QM.sounds[{key}] = {mark}\n  end\nend, {owner})\n")
    };
    let streams: Vec<&&SoundBankRegistration> = regs.iter().filter(|r| matches!(r.load, SoundLoad::Stream { .. })).collect();
    let banks: Vec<&&SoundBankRegistration> = regs.iter().filter(|r| !matches!(r.load, SoundLoad::Stream { .. })).collect();
    let stream_path = |r: &SoundBankRegistration, folder: &str| {
        format!(
            "{} .. \"\\\\\" .. string.upper(Sound.GetAudioDir()) .. \"\\\\\" .. string.upper(MrxSoundBanks._GetLocalizedName({})) .. \".PWS\"",
            lua_string(folder),
            lua_string(&r.bank)
        )
    };
    let mut out = String::new();
    for r in &streams {
        let SoundLoad::Stream { folder } = &r.load else { unreachable!("filtered on streams") };
        let key = lua_string(&format!("{}.pws", r.bank));
        let calls = format!("    Sound.OpenStreamFile({}, {key})\n", stream_path(r, folder));
        out.push_str(&subscribe(&load, &key, "not ", &calls, "true", &lua_string(&r.shipment)));
    }
    for r in &banks {
        let n = lua_string(&r.bank);
        let (key, on, off) = match &r.load {
            SoundLoad::Shipped { soundbank } => {
                let sb = |verb: &str| if *soundbank { format!("    MrxSoundBanks.{verb}SoundBank({n})\n") } else { String::new() };
                (n.clone(), format!("    MrxSoundBanks.LoadWaveBank({n})\n{}", sb("Load")), format!("    MrxSoundBanks.UnloadWaveBank({n})\n{}", sb("Unload")))
            }
            SoundLoad::SoundBank => (
                lua_string(&format!("soundbank {}", r.bank)),
                format!("    MrxSoundBanks.LoadSoundBank({n})\n"),
                format!("    MrxSoundBanks.UnloadSoundBank({n})\n"),
            ),
            SoundLoad::WaveBank => (
                lua_string(&format!("wavebank {}", r.bank)),
                format!("    MrxSoundBanks.LoadWaveBank({n})\n"),
                format!("    MrxSoundBanks.UnloadWaveBank({n})\n"),
            ),
            SoundLoad::Stream { .. } => unreachable!("filtered out"),
        };
        let owner = lua_string(&r.shipment);
        out.push_str(&subscribe(&load, &key, "not ", &on, "true", &owner));
        out.push_str(&subscribe(&unload, &key, "", &off, "nil", &owner));
    }
    for r in &streams {
        let SoundLoad::Stream { folder } = &r.load else { unreachable!("filtered on streams") };
        let key = lua_string(&format!("{}.pws", r.bank));
        let calls = format!("    Sound.CloseStreamFile({}, {key})\n", stream_path(r, folder));
        out.push_str(&subscribe(&unload, &key, "", &calls, "nil", &lua_string(&r.shipment)));
    }
    out
}

/// The `qm_modloader` library: every library [`crate::loader`] loads for qm itself in gameplay, when
/// any UI, layer, support or gameplay sound registration needs it.
pub fn qm_modloader_library() -> crate::loader::Library {
    crate::loader::Library {
        module: QM_MODLOADER_NAME.to_string(),
        owner: crate::loader::QM_OWNER.to_string(),
        when: "world.scripts_loaded".to_string(),
        sessions: vec![crate::manifest::LoadSession::Gameplay],
    }
}

/// The `qm_shell_modloader` library, when any registration loads a bank in the front end.
pub fn qm_shell_modloader_library() -> crate::loader::Library {
    crate::loader::Library {
        module: QM_SHELL_MODLOADER_NAME.to_string(),
        owner: crate::loader::QM_OWNER.to_string(),
        when: "shell.boot".to_string(),
        sessions: vec![crate::manifest::LoadSession::FrontEnd],
    }
}

/// The whole `qm_modloader` script, baked from every UI, layer, support and gameplay sound
/// registration.
///
/// This is the "expandable load space": everything a mod adds lives *here*, in a script Quartermaster
/// owns and re-mints each link, and the game's scripts carry only qm's hooks. qm's world entry loads it
/// as a library ([`qm_modloader_library`]); its body subscribes each registration to qm's hooks through
/// `_G._QM` ([`crate::loader`]), under the Shipment that declared it, so a registration that fails
/// stops the game naming its Shipment (the error policy).
///
/// The UI, layer and support registrations run at `world.begin`, each once per VM (`QM.ran`, keyed by
/// the Shipment and the registration, so two Shipments' registrations of one name both run). Sound
/// banks load at `world.begin` and unload at `world.sound_exit`, after retail's own `UnloadBanks`
/// ([`sound_subscriptions`]). Every module a registration calls is imported at the top, so it is a
/// DEPS child of this script and loaded before it runs: `MrxGuiBase` for `MrxGuiBase.FlashWidget`
/// (retail's own form, `mrxguipda.lua:802`), `MrxLayerManager`, `MrxSupportData`, `MrxRewardData`,
/// `MrxSoundBanks`, and each support subclass.
///
/// Registrations are baked in the resolved load `order` — the same order [`linked_source`] follows —
/// and within one Shipment by movie / layer / id / bank, so one request bakes byte-identically
/// whatever order the registrations arrive in. A Shipment that requires another is therefore
/// registered after it. A contributor missing from `order` is [`LinkError::NotInOrder`].
pub fn qm_modloader_source(
    regs: &[UiRegistration],
    layers: &[LayerRegistration],
    support: &[SupportRegistration],
    sounds: &[SoundBankRegistration],
    order: &[String],
) -> Result<String, LinkError> {
    let mut imports = String::new();
    let mut subs = String::new();
    let once = |key: &str, owner: &str, body: &str| {
        format!(
            "QM.on(\"world.begin\", function()\n  if QM.ran[{key}] then\n    return\n  end\n  QM.ran[{key}] = true\n{body}end, {owner})\n",
            key = lua_string(key),
            owner = lua_string(owner),
        )
    };

    let sounds = session_sounds(sounds, crate::manifest::LoadSession::Gameplay, order)?;
    if !sounds.is_empty() {
        imports.push_str("import(\"MrxSoundBanks\")\n");
    }

    let mut ordered: Vec<&UiRegistration> = regs.iter().collect();
    sort_by_order(&mut ordered, order, |r| r.shipment.as_str(), |r| r.movie.clone())?;
    if !ordered.is_empty() {
        imports.push_str("import(\"MrxGuiBase\")\n");
    }
    for r in &ordered {
        let n = lua_string(&r.movie);
        // The creation sequence — new → SetSwfFile → Play → SetVisible — is the one from decompiled
        // mrxgui.lua (`loadingscreen_standalone`); the handle is parked in `QM.ui[name]` for the author.
        subs.push_str(&format!("-- {}: {}\n", r.shipment, r.movie));
        subs.push_str(&once(
            &format!("{} ui {}", r.shipment, r.movie),
            &r.shipment,
            &format!("  local w = MrxGuiBase.FlashWidget:new()\n  w:SetSwfFile({n})\n  w:Play()\n  w:SetVisible(true)\n  QM.ui[{n}] = w\n"),
        ));
    }

    // Each layer activation is `MarkForRemoval`(old) then `MarkForAddition`(new) — the vanilla-contract
    // order (remove pristine, add act) so the two never both apply. The immediate mark forms (no
    // callback), ordered by load order, then `add`.
    let mut ordered_layers: Vec<&LayerRegistration> = layers.iter().collect();
    sort_by_order(&mut ordered_layers, order, |r| r.shipment.as_str(), |r| r.add.clone())?;
    if !ordered_layers.is_empty() {
        imports.push_str("import(\"MrxLayerManager\")\n");
    }
    for r in &ordered_layers {
        let mut body = String::new();
        for rem in &r.remove {
            body.push_str(&format!("  MrxLayerManager.MarkForRemoval({})\n", lua_string(rem)));
        }
        body.push_str(&format!("  MrxLayerManager.MarkForAddition({})\n", lua_string(&r.add)));
        subs.push_str(&format!("-- {}: activate {}\n", r.shipment, r.add));
        subs.push_str(&once(&format!("{} layer {}", r.shipment, r.add), &r.shipment, &body));
    }

    // Each novel-behaviour shop item constructs `oSupport` from its minted subclass, DEFERS the catalog
    // + reward rows (the eager append can't — the module is nil at resident-load), re-applies the
    // `SetSupportName` the `mrxsupportdata.Init` tail loop already ran without, applies the item's
    // saved unlock status and stock (`_QM.saved`, which retail could not restore into a row that did
    // not exist yet), and nulls the never-invalidated `gtAllSupport` cache so the next shop Open
    // rebuilds with the new id. Ordered by load order, then `id`.
    let mut ordered_support: Vec<&SupportRegistration> = support.iter().collect();
    sort_by_order(&mut ordered_support, order, |r| r.shipment.as_str(), |r| r.id.clone())?;
    if !ordered_support.is_empty() {
        imports.push_str("import(\"MrxSupportData\")\nimport(\"MrxRewardData\")\n");
    }
    let mut imported: BTreeSet<&str> = BTreeSet::new();
    for r in &ordered_support {
        if imported.insert(r.module.as_str()) {
            imports.push_str(&format!("import({})\n", lua_string(&r.module)));
        }
        let mut body = String::new();
        body.push_str(&format!("  local oSupport = {}:Create()\n", r.module));
        if let Some(c) = &r.cargo {
            body.push_str(&format!("  oSupport:SetCargo({})\n", lua_string(c)));
        }
        if let Some(dv) = &r.delivery_vehicle {
            body.push_str(&format!("  oSupport:SetDeliveryVehicle({})\n", lua_string(dv)));
        }
        body.push_str(&format!("  oSupport:SetSupportName({})\n", lua_string(&r.id)));
        body.push_str(&format!(
            "  MrxSupportData.tSupportData[{}] = {{ sName = {}, sDescription = {}, sIcon = {}, \
             nMaxStock = {}, nCashCost = {}, nFuelCost = {}, oSupport = oSupport, sType = {}, \
             tUnlockStatus = {} }}\n",
            lua_string(&r.id),
            lua_string(&r.name),
            lua_string(&r.description),
            lua_string(&r.icon),
            r.max_stock.min(99),
            r.cash_cost,
            r.fuel_cost,
            lua_string(&r.item_type),
            r.unlock_table,
        ));
        // Retail restored the save's shop state during the world load, before this row existed; apply
        // this item's the way MrxSupportData.LoadSingleton does (mrxsupportdata.lua:2452-2455).
        body.push_str(&format!(
            "  local tSaved = QM.saved and QM.saved.tSupportData and QM.saved.tSupportData[{id}]\n  \
             if tSaved then\n    \
             MrxSupportData.tSupportData[{id}].tUnlockStatus = tSaved.tUnlockStatus\n    \
             MrxSupportData.tSupportData[{id}].nGlobalStock = tSaved.nGlobalStock\n  \
             end\n",
            id = lua_string(&r.id),
        ));
        for fac in &r.shops {
            body.push_str(&format!(
                "  MrxRewardData._tRewards[{}] = {{ sFactionId = {}, tSupport = {{ {{ {}, {} }} }} }}\n",
                lua_string(&format!("{}Reward_{}", r.id, fac)),
                lua_string(fac),
                lua_string(&r.id),
                lua_string(fac),
            ));
        }
        body.push_str("  MrxRewardData.gtAllSupport = nil\n");
        subs.push_str(&format!("-- {}: shop ability {} ({})\n", r.shipment, r.id, r.module));
        subs.push_str(&once(&format!("{} support {}", r.shipment, r.id), &r.shipment, &body));
    }

    subs.push_str(&sound_subscriptions("world.begin", "world.sound_exit", &sounds));

    Ok(format!(
        "-- {name} — Quartermaster's expandable mod load space (generated; do not hand-edit).\n\
         --\n\
         -- qm's world entry loads this as a library. Its body subscribes every registration to qm's\n\
         -- hooks through _G._QM, under the Shipment that declared it.\n\
         {imports}\
         local QM = _G._QM\n\
         QM.ui = QM.ui or {{}}\n\
         QM.sounds = QM.sounds or {{}}\n\
         QM.ran = QM.ran or {{}}\n\
         {subs}",
        name = QM_MODLOADER_NAME,
    ))
}

/// The whole `qm_shell_modloader` script: the front end's sound loader, baked from every
/// registration whose sessions include the front end, in load `order`, then by bank name.
///
/// qm's front-end entry loads it as a library ([`qm_shell_modloader_library`]). Its body subscribes
/// each bank to `shell.open`, after retail's `EnterShellState` loads the shell's own banks, and to
/// `shell.sound_exit`, after retail's `ExitShellState` unloads them ([`sound_subscriptions`]). A
/// contributor missing from `order` is [`LinkError::NotInOrder`].
pub fn qm_shell_modloader_source(sounds: &[SoundBankRegistration], order: &[String]) -> Result<String, LinkError> {
    let sounds = session_sounds(sounds, crate::manifest::LoadSession::FrontEnd, order)?;
    Ok(format!(
        "-- {name} — Quartermaster's front-end sound loader (generated; do not hand-edit).\n\
         --\n\
         -- qm's front-end entry loads this as a library. Its body subscribes each bank to qm's\n\
         -- shell.open and shell.sound_exit hooks through _G._QM, under the Shipment that declared it.\n\
         import(\"MrxSoundBanks\")\n\
         local QM = _G._QM\n\
         QM.sounds = QM.sounds or {{}}\n\
         {subs}",
        name = QM_SHELL_MODLOADER_NAME,
        subs = sound_subscriptions("shell.open", "shell.sound_exit", &sounds),
    ))
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

/// Compile each target of `mutations` once, its base source (found under `subfolders`,
/// [`base_source_path`]) followed by every append in load `order`, and splice it into whichever of
/// `blocks` carries the target first.
///
/// A target in `spliced` has no source to compile: it is a level's own script, whose bytecode stays as
/// it is, and the appends compile on their own and run at its end
/// ([`mercs2_luac::splice::append_chunk`]).
fn link_appends(
    blocks: &mut [TargetBlock<'_>],
    corpus_root: &Path,
    subfolders: &[&str],
    mutations: &[ScriptMutation],
    order: &[String],
    spliced: &BTreeSet<u32>,
) -> Result<Vec<LinkedScript>, LinkError> {
    // Group by target so each script is compiled ONCE with all its appends, which is the entire
    // point — compiling per Shipment would mean the last one wins again, just more slowly.
    let mut by_target: BTreeMap<&str, Vec<&ScriptMutation>> = BTreeMap::new();
    for m in mutations {
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
        if spliced.contains(&crate::manifest::asset_hash(target)) {
            let (source, contributors) = linked_source("", &group, order)?;
            let fail = |message: String| LinkError::Compile { target: target.to_string(), message };
            // A level's script can keep a name of its own under a retail asset: `mrxplayer` is
            // `dlc01_player` in the DLC.
            let name = mercs2_formats::scripts_block::container_info(&block.entries[idx].bytes).map_err(&fail)?.name;
            let appended = mercs2_luac::compile(&source, &name).map_err(|e| fail(e.to_string()))?;
            let base = block.extract_lua(idx).map_err(&fail)?;
            let bytecode = mercs2_luac::splice::append_chunk(&base, &appended, &name).map_err(|e| fail(e.to_string()))?;
            block.append_lua(idx, &bytecode, &appended).map_err(|e| LinkError::Splice {
                target: target.to_string(),
                message: e,
            })?;
            linked.push(LinkedScript {
                target: target.to_string(),
                contributors,
                base_source_bytes: 0,
                linked_source_bytes: source.len(),
                bytecode_bytes: bytecode.len(),
                block: bi,
            });
            continue;
        }
        let source_path = base_source_path(corpus_root, subfolders, target).map_err(|tried| LinkError::NoBaseSource {
            target: target.to_string(),
            tried,
        })?;
        let base = std::fs::read_to_string(&source_path).map_err(|e| LinkError::Compile {
            target: target.to_string(),
            message: format!("reading base source {}: {e}", source_path.display()),
        })?;

        let (source, contributors) = linked_source(&base, &group, order)?;
        // BARE chunk name — see the module note. `@name.lua` produces a chunk 5 bytes off retail.
        let bytecode = mercs2_luac::compile(&source, target).map_err(|e| LinkError::Compile {
            target: target.to_string(),
            message: e.to_string(),
        })?;
        block.replace_lua(idx, &bytecode).map_err(|e| LinkError::Splice {
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
    Ok(linked)
}

/// Splice `tail` onto the script at `idx` of `block`, returning the new chunk's size. Nothing is
/// recompiled from the corpus: the tail compiles on its own, under the host's INFO name, and runs at
/// the end of the host's chunk ([`mercs2_luac::splice::append_chunk`]), after any Shipment append
/// already linked into it.
///
/// The host's main chunk must return only at its end, where the tail runs; an earlier `return` is
/// [`LinkError::Hooks`], since the tail would not run on that path. Each function the tail wraps must
/// be one the host's main chunk defines in that bytecode; one it does not define is [`LinkError::Hooks`]
/// naming the hooks and the host, rather than a wrapper that fails on every boot. Both are read with
/// [`mercs2_formats::script_meta::main_run`].
fn splice_tail(block: &mut ScriptsBlock, idx: usize, tail: &crate::loader::Tail) -> Result<usize, LinkError> {
    let fail_name = mercs2_formats::scripts_block::container_info(&block.entries[idx].bytes);
    let name = fail_name.map_err(|message| LinkError::Splice { target: format!("script entry {idx}"), message })?.name;
    let fail = |message: String| LinkError::Compile { target: name.clone(), message };
    let base = block.extract_lua(idx).map_err(&fail)?;
    let run = mercs2_formats::script_meta::main_run(&base)
        .map_err(|e| LinkError::Hooks(format!("reading {name}'s main chunk, to splice qm's hooks onto it: {e}")))?;
    if let Some(at) = run.early_return {
        return Err(LinkError::Hooks(format!(
            "{name}'s main chunk can return at instruction {at}, before its end, where qm's hooks run; \
             a master or host script must not return from its top level"
        )));
    }
    for (target, ids) in &tail.wraps {
        if !run.functions.contains(target) {
            return Err(LinkError::Hooks(format!(
                "hook {} wraps {name}.{target}, and {name}'s main chunk defines no function {target}",
                ids.join(", ")
            )));
        }
    }
    let appended = mercs2_luac::compile(&tail.source, &name).map_err(|e| fail(e.to_string()))?;
    let bytecode = mercs2_luac::splice::append_chunk(&base, &appended, &name).map_err(|e| fail(e.to_string()))?;
    block
        .append_lua(idx, &bytecode, &appended)
        .map_err(|e| LinkError::Splice { target: name.clone(), message: e })?;
    Ok(bytecode.len())
}

/// Splice each `(host, tail)` of [`crate::loader::host_tails`] onto its host script, in whichever of
/// `blocks` carries it first ([`splice_tail`]).
pub fn splice_tails(blocks: &mut [TargetBlock<'_>], tails: &[(String, crate::loader::Tail)]) -> Result<Vec<LinkedScript>, LinkError> {
    let mut linked = Vec::new();
    for (host, tail) in tails {
        let (bi, idx) = blocks
            .iter()
            .enumerate()
            .find_map(|(bi, tb)| tb.block.find_script_by_name(host).map(|idx| (bi, idx)))
            .ok_or_else(|| LinkError::UnknownScript { target: host.clone(), shipment: MODLOADER_CONTRIBUTOR.to_string() })?;
        let bytecode_bytes = splice_tail(blocks[bi].block, idx, tail)?;
        linked.push(LinkedScript {
            target: host.clone(),
            contributors: vec![MODLOADER_CONTRIBUTOR.to_string()],
            base_source_bytes: 0,
            linked_source_bytes: tail.source.len(),
            bytecode_bytes,
            block: bi,
        });
    }
    Ok(linked)
}

/// The block index and entry index of the first script named `name` among `blocks`.
fn find_script(blocks: &[TargetBlock<'_>], name: &str) -> Option<(usize, usize)> {
    blocks.iter().enumerate().find_map(|(bi, tb)| tb.block.find_script_by_name(name).map(|idx| (bi, idx)))
}

/// Link the set's `outfits` into `level` ([`crate::outfits`]): read the level's retail lists from its
/// wardrobe and player scripts as the link has them so far (every append and replacement applied),
/// generate each hero's list in load `order`, and splice the tails onto the wardrobe, the player script
/// and the master (a custom level's `master` block, else [`crate::hooks::MASTER_HOST`] in `blocks`).
/// The heroes' own chatter sets are read from the worldentity of the resident block in `blocks` only when
/// a row has one. A level the set adds no outfit to is left as it ships.
fn link_outfits(
    blocks: &mut [TargetBlock<'_>],
    master: Option<&mut ScriptsBlock>,
    level: &str,
    outfits: &[crate::outfits::AddedOutfit],
    order: &[String],
) -> Result<Vec<LinkedScript>, LinkError> {
    use crate::outfits::{WARDROBE_SCRIPT, PLAYER_SCRIPT};
    if outfits.is_empty() {
        return Ok(Vec::new());
    }
    let fail = |m: String| LinkError::Outfits(format!("{level}: {m}"));
    let mut added: Vec<&crate::outfits::AddedOutfit> = outfits.iter().collect();
    sort_by_order(&mut added, order, |a| a.shipment.as_str(), |_| ())?;
    let added: Vec<crate::outfits::AddedOutfit> = added.into_iter().cloned().collect();
    let chunk = |blocks: &[TargetBlock<'_>], name: &str| -> Result<Vec<u8>, LinkError> {
        let (bi, idx) = find_script(blocks, name).ok_or_else(|| fail(format!("no {name} script in the blocks linked")))?;
        blocks[bi].block.extract_lua(idx).map_err(|m| fail(format!("{name}: {m}")))
    };
    let retail = crate::outfits::RetailLevel::read(&chunk(blocks, WARDROBE_SCRIPT)?, &chunk(blocks, PLAYER_SCRIPT)?).map_err(&fail)?;
    let lists = crate::outfits::generate(level, &retail, &added).map_err(&fail)?;
    let hero_chatter = if lists.heroes.iter().any(|h| h.rows.iter().any(|r| r.chatter.is_some())) {
        let (bi, idx) = blocks
            .iter()
            .enumerate()
            .find_map(|(bi, tb)| crate::fx::worldentity_entry(tb.block).ok().map(|idx| (bi, idx)))
            .ok_or_else(|| fail("no block linked holds the worldentity, which names the heroes' chatter sets".into()))?;
        let we = mercs2_formats::worldentity::WorldEntity::parse(&blocks[bi].block.entries[idx].bytes)
            .map_err(|m| fail(format!("the worldentity in {}: {m}", blocks[bi].path)))?;
        Some(crate::outfits::hero_chatter(&we, &retail).map_err(&fail)?)
    } else {
        None
    };
    let Some(tails) = crate::outfits::tails(&lists, &retail, hero_chatter.as_ref()).map_err(&fail)? else {
        return Ok(Vec::new());
    };
    let mut contributors: Vec<String> = Vec::new();
    for a in &added {
        if !contributors.contains(&a.shipment) {
            contributors.push(a.shipment.clone());
        }
    }
    let mut linked = Vec::new();
    let mut on_block = |blocks: &mut [TargetBlock<'_>], host: &str, tail: &crate::loader::Tail| -> Result<(), LinkError> {
        let (bi, idx) = find_script(blocks, host).ok_or_else(|| fail(format!("no {host} script in the blocks linked")))?;
        let bytecode_bytes = splice_tail(blocks[bi].block, idx, tail)?;
        linked.push(LinkedScript {
            target: host.to_string(),
            contributors: contributors.clone(),
            base_source_bytes: 0,
            linked_source_bytes: tail.source.len(),
            bytecode_bytes,
            block: bi,
        });
        Ok(())
    };
    on_block(blocks, WARDROBE_SCRIPT, &tails.wardrobe)?;
    on_block(blocks, PLAYER_SCRIPT, &tails.player)?;
    if let Some(briefing) = &tails.briefing {
        on_block(blocks, crate::outfits::BRIEFING_SCRIPT, briefing)?;
    }
    match master {
        Some(master) => {
            splice_tail(master, 0, &tails.master)?;
            master.verify_csums().map_err(|e| LinkError::Block(format!("CSUMs after splicing the level's master: {e}")))?;
        }
        None => on_block(blocks, crate::hooks::MASTER_HOST, &tails.master)?,
    }
    Ok(linked)
}

/// The libraries of `additions` that load in `session`, in load order.
fn session_libraries(additions: &[ScriptAddition], session: crate::manifest::LoadSession, order: &[String]) -> Result<Vec<crate::loader::Library>, LinkError> {
    let mut libs: Vec<&ScriptAddition> = additions.iter().filter(|a| a.load.as_ref().is_some_and(|l| l.sessions.contains(&session))).collect();
    sort_by_order(&mut libs, order, |a| a.shipment.as_str(), |_| ())?;
    Ok(libs
        .into_iter()
        .map(|a| {
            let load = a.load.as_ref().expect("filtered on load");
            crate::loader::Library { module: a.name.clone(), owner: a.shipment.clone(), when: load.when.clone(), sessions: vec![session] }
        })
        .collect())
}

/// Whether an `add_script` is minted into the blocks of `session`'s WAD: a library loads there, and a
/// module with no `load:` is imported by gameplay code, as before `load:` existed.
fn mints_in(a: &ScriptAddition, session: crate::manifest::LoadSession) -> bool {
    match &a.load {
        Some(l) => l.sessions.contains(&session),
        None => session == crate::manifest::LoadSession::Gameplay,
    }
}

/// Generate `session`'s host tails ([`crate::loader::generated_hooks`], [`crate::loader::host_tails`])
/// for `libraries` and splice them onto `blocks` ([`splice_tails`]). In a custom level, `master` is the
/// level's own master script, a block of that one entry, and the master's tail
/// ([`crate::hooks::MASTER_HOST`]) goes onto it instead; it is not one of `blocks`, so it is not in the
/// result.
fn link_session_hooks(
    blocks: &mut [TargetBlock<'_>],
    libraries: &[crate::loader::Library],
    session: crate::manifest::LoadSession,
    master: Option<&mut ScriptsBlock>,
) -> Result<Vec<LinkedScript>, LinkError> {
    let hooks = crate::loader::generated_hooks(session, libraries).map_err(LinkError::Hooks)?;
    let tails = crate::loader::host_tails(&hooks, libraries).map_err(LinkError::Hooks)?;
    let mut tails: Vec<(String, crate::loader::Tail)> =
        tails.into_iter().filter(|((s, _), _)| *s == session).map(|((_, host), t)| (host, t)).collect();
    if let Some(master) = master {
        if master.entries.len() != 1 {
            return Err(LinkError::Block(format!("internal error: a level's master block holds {} entries, not 1", master.entries.len())));
        }
        if let Some(at) = tails.iter().position(|(h, _)| h == crate::hooks::MASTER_HOST) {
            let (_, tail) = tails.remove(at);
            splice_tail(master, 0, &tail)?;
            master.verify_csums().map_err(|e| LinkError::Block(format!("CSUMs after splicing the level's master: {e}")))?;
        }
    }
    splice_tails(blocks, &tails)
}

/// Link qm's front-end hooks and loader into `shell.wad`'s scripts block ([`SHELL_SCRIPT_BLOCKS`]), as
/// `hooks` says.
///
/// Each `add_script` library that loads in the front end is minted into the block that carries
/// `shellbootstrap`, in load order. When any of `sound_regs` loads a bank in the front end,
/// `qm_shell_modloader` ([`qm_shell_modloader_source`]) is minted beside them and loads as qm's own
/// library. Then the front end's hooks are spliced onto their hosts ([`link_session_hooks`]): the entry
/// `shell.boot` onto `shellbootstrap`, which loads the libraries. Nothing is recompiled from the corpus.
/// Every touched block must still CSUM-verify.
pub fn link_front_end(
    blocks: &mut [TargetBlock<'_>],
    sound_regs: &[SoundBankRegistration],
    additions: &[ScriptAddition],
    order: &[String],
    hooks: QmHooks,
) -> Result<LinkOutput, LinkError> {
    let session = crate::manifest::LoadSession::FrontEnd;
    let sounds = session_sounds(sound_regs, session, order)?;
    let shipment_libraries = session_libraries(additions, session, order)?;
    let splice = match hooks {
        QmHooks::Always => true,
        QmHooks::ForLibraries => !sounds.is_empty() || !shipment_libraries.is_empty(),
    };
    if !splice {
        return Ok(LinkOutput { scripts: Vec::new(), findings: Vec::new(), modules: Vec::new() });
    }
    let before = deps_graph(blocks, None)?;
    let bi = blocks
        .iter()
        .position(|tb| tb.block.find_script_by_name("shellbootstrap").is_some())
        .ok_or_else(|| LinkError::UnknownScript { target: "shellbootstrap".to_string(), shipment: MODLOADER_CONTRIBUTOR.to_string() })?;
    let mut linked = Vec::new();
    let mut minted: Vec<&ScriptAddition> = additions.iter().filter(|a| mints_in(a, session)).collect();
    sort_by_order(&mut minted, order, |a| a.shipment.as_str(), |_| ())?;
    for a in minted {
        refuse_taken(blocks, &a.name, &a.shipment)?;
        let bytecode = mercs2_luac::compile(&a.source, &a.name).map_err(|e| LinkError::Compile { target: a.name.clone(), message: e.to_string() })?;
        blocks[bi].block.add_script(&a.name, &bytecode).map_err(|m| LinkError::Splice { target: a.name.clone(), message: m })?;
        linked.push(LinkedScript {
            target: a.name.clone(),
            contributors: vec![a.shipment.clone()],
            base_source_bytes: 0,
            linked_source_bytes: a.source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }
    if !sounds.is_empty() {
        let source = qm_shell_modloader_source(sound_regs, order)?;
        refuse_taken(blocks, QM_SHELL_MODLOADER_NAME, MODLOADER_CONTRIBUTOR)?;
        let bytecode = mercs2_luac::compile(&source, QM_SHELL_MODLOADER_NAME).map_err(|e| LinkError::Compile {
            target: QM_SHELL_MODLOADER_NAME.to_string(),
            message: e.to_string(),
        })?;
        blocks[bi].block.add_script(QM_SHELL_MODLOADER_NAME, &bytecode).map_err(|m| LinkError::Splice {
            target: QM_SHELL_MODLOADER_NAME.to_string(),
            message: m,
        })?;
        let mut names: Vec<&String> = sounds.iter().map(|r| &r.shipment).collect();
        names.dedup();
        linked.push(LinkedScript {
            target: QM_SHELL_MODLOADER_NAME.to_string(),
            contributors: names.into_iter().cloned().collect(),
            base_source_bytes: 0,
            linked_source_bytes: source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }
    // qm's own loader loads first.
    let qm = (!sounds.is_empty()).then(qm_shell_modloader_library);
    let libraries: Vec<crate::loader::Library> = qm.iter().chain(&shipment_libraries).cloned().collect();
    linked.extend(link_session_hooks(blocks, &libraries, session, None)?);
    for b in linked.iter().map(|l| l.block).collect::<BTreeSet<_>>() {
        blocks[b]
            .block
            .verify_csums()
            .map_err(|e| LinkError::Block(format!("CSUMs after linking {}: {e}", blocks[b].path)))?;
    }
    // Every module minted here is a library (`mints_in`), so the libraries are every row.
    let modules = library_modules(session, qm.as_ref(), &shipment_libraries)?;
    let sources: Vec<crate::imports::Source<'_>> = additions
        .iter()
        .filter(|a| mints_in(a, session))
        .map(|a| crate::imports::Source { shipment: &a.shipment, what: format!("add_script {}", a.name), script: &a.name, source: &a.source })
        .collect();
    let findings = import_findings(blocks, None, FRONT_END_ROOTS, &before, &sources, &[], &linked)?;
    Ok(LinkOutput { scripts: linked, findings, modules })
}

/// The DEPS graph of every script of `blocks` and of a level's own `master` block
/// ([`crate::imports::DepsGraph`]).
fn deps_graph(blocks: &[TargetBlock<'_>], master: Option<&ScriptsBlock>) -> Result<crate::imports::DepsGraph, LinkError> {
    crate::imports::DepsGraph::of(blocks.iter().map(|tb| &*tb.block).chain(master)).map_err(LinkError::Block)
}

/// The script a level's link loads first: a custom level's own master (its `master` block's one
/// entry), else the catalogue's master ([`crate::hooks::MASTER_HOST`]).
fn level_master(master: Option<&ScriptsBlock>) -> Result<String, LinkError> {
    let Some(m) = master else { return Ok(crate::hooks::MASTER_HOST.to_string()) };
    match m.entries.as_slice() {
        [e] => mercs2_formats::scripts_block::container_info(&e.bytes)
            .map(|i| i.name)
            .map_err(|e| LinkError::Block(format!("the level's master: INFO: {e}"))),
        other => Err(LinkError::Block(format!("internal error: a level's master block holds {} entries, not 1", other.len()))),
    }
}

/// The import findings ([`crate::imports::check`]) of a link of `blocks` (and a level's `master`)
/// that the engine loads from `roots`, started from `before`, put `sources` into scripts, minted
/// `import_only` with no `load:`, and produced `linked`.
fn import_findings(
    blocks: &[TargetBlock<'_>],
    master: Option<&ScriptsBlock>,
    roots: &[&str],
    before: &crate::imports::DepsGraph,
    sources: &[crate::imports::Source<'_>],
    import_only: &[(&str, &str)],
    linked: &[LinkedScript],
) -> Result<Vec<crate::imports::ImportFinding>, LinkError> {
    let after = deps_graph(blocks, master)?;
    let mut contributors: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for l in linked {
        let names = contributors.entry(mercs2_formats::hash::pandemic_hash_m2(&l.target)).or_default();
        for c in &l.contributors {
            if !names.contains(c) {
                names.push(c.clone());
            }
        }
    }
    crate::imports::check(&crate::imports::LinkChecks { before, after: &after, roots, sources, import_only, contributors: &contributors })
        .map_err(|message| LinkError::Compile { target: "the import checks".into(), message })
}

/// Refuse minting a script named `name` when a script of ANY of `blocks` already carries it
/// ([`LinkError::NameTaken`]). A new script is minted into the `wifpmcinterior` block, and
/// `ScriptsBlock::add_script` refuses only a name of that one block; a name another block carries
/// (`mrxplayer` in `resident`) would mint a second script of one name.
fn refuse_taken(blocks: &[TargetBlock<'_>], name: &str, shipment: &str) -> Result<(), LinkError> {
    match blocks.iter().find(|tb| tb.block.find_script_by_name(name).is_some()) {
        Some(tb) => Err(LinkError::NameTaken {
            name: name.to_string(),
            shipment: shipment.to_string(),
            path: tb.path.clone(),
        }),
        None => Ok(()),
    }
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
///
/// `order` is the resolved load order of the contributing Shipments' names, and every ordered
/// decision here follows it: append concatenation, which `replace_lua` wins (the later one), the
/// mint order of `add_script` modules and support subclasses, and the `qm_modloader` bake. A
/// contributor missing from it is [`LinkError::NotInOrder`].
///
/// Once everything is linked and minted, every Shipment-authored Lua source — `patch_lua` appends,
/// `add_script` modules, `replace_lua` sources and support modules — goes through the import checks
/// ([`crate::imports::check`]: M0209, M0313, M0314), which come back as [`LinkOutput::findings`];
/// the caller decides what each refuses.
pub fn link_into_blocks(
    blocks: &mut [TargetBlock<'_>],
    corpus_root: &Path,
    mutations: &[ScriptMutation],
    ui_regs: &[UiRegistration],
    layer_regs: &[LayerRegistration],
    support_regs: &[SupportRegistration],
    sound_regs: &[SoundBankRegistration],
    additions: &[ScriptAddition],
    replacements: &[ScriptReplacement],
    order: &[String],
) -> Result<LinkOutput, LinkError> {
    link_into_level_blocks(
        blocks,
        corpus_root,
        mutations,
        ui_regs,
        layer_regs,
        support_regs,
        sound_regs,
        additions,
        replacements,
        order,
        &BTreeSet::new(),
        QmHooks::Always,
        None,
        crate::outfits::DEFAULT_LEVEL,
        &[],
    )
}

/// Whether a link splices qm's hooks and library loader ([`crate::loader`]) onto their hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QmHooks {
    /// The deploy link of the installed set ([`link_into_blocks`]), and each custom level's link:
    /// always, mods or no mods (W-a), so every hook's entry line prints in every installed game.
    Always,
    /// A standalone build of one Shipment: only when it needs qm's loader (a library, or a registration
    /// `qm_modloader` carries), so its overlay claims no block it does not otherwise touch. The deploy
    /// link generates the hooks for the installed set.
    ForLibraries,
}

/// [`link_into_blocks`] over a level's blocks, in which the scripts of `spliced` are the level's own,
/// splicing qm's hooks as `hooks` says. In a custom level, `master` is the level's own master script,
/// a block of that one entry, which takes the master's hooks ([`crate::hooks::MASTER_HOST`]).
/// `level` names the level, and `outfits` are the set's added outfits, linked after the hooks
/// ([`link_outfits`]); the ones that do not join `level` are left out by the generator.
pub fn link_into_level_blocks(
    blocks: &mut [TargetBlock<'_>],
    corpus_root: &Path,
    mutations: &[ScriptMutation],
    ui_regs: &[UiRegistration],
    layer_regs: &[LayerRegistration],
    support_regs: &[SupportRegistration],
    sound_regs: &[SoundBankRegistration],
    additions: &[ScriptAddition],
    replacements: &[ScriptReplacement],
    order: &[String],
    spliced: &BTreeSet<u32>,
    hooks: QmHooks,
    mut master: Option<&mut ScriptsBlock>,
    level: &str,
    outfits: &[crate::outfits::AddedOutfit],
) -> Result<LinkOutput, LinkError> {
    let session = crate::manifest::LoadSession::Gameplay;
    let root = level_master(master.as_deref())?;
    // Anything that lives in the load space — a UI widget, a layer activation, a novel support
    // behaviour or a sound bank to load in gameplay — needs `qm_modloader`, minted after the base
    // scripts link (below) and loaded by qm's world entry.
    let gameplay_sounds = sound_regs.iter().any(|r| r.sessions.contains(&session));
    let needs_loader = !ui_regs.is_empty() || !layer_regs.is_empty() || !support_regs.is_empty() || gameplay_sounds;

    let before = deps_graph(blocks, master.as_deref())?;
    let mut linked = link_appends(blocks, corpus_root, VZ_SOURCE_DIRS, mutations, order, spliced)?;
    // The modules minted below that only an `import` loads, in mint order.
    let mut imported: Vec<LinkedModule> = Vec::new();

    // Apply each `replace_lua` wholesale swap. Same asset hash, new bytecode -- every existing
    // `import(<target>)` call site now returns the new module without rebinding. Applied in load
    // order, so where two replace one target the later Shipment's is what remains.
    let mut in_order: Vec<&ScriptReplacement> = replacements.iter().collect();
    sort_by_order(&mut in_order, order, |r| r.shipment.as_str(), |_| ())?;
    for r in in_order {
        let (bi, idx) = blocks
            .iter()
            .enumerate()
            .find_map(|(bi, tb)| tb.block.find_script_by_name(&r.target).map(|idx| (bi, idx)))
            .ok_or_else(|| LinkError::UnknownScript {
                target: r.target.clone(),
                shipment: r.shipment.clone(),
            })?;
        let bytecode = mercs2_luac::compile(&r.source, &r.target)
            .map_err(|e| LinkError::Compile {
                target: r.target.clone(),
                message: e.to_string(),
            })?;
        blocks[bi]
            .block
            .replace_lua(idx, &bytecode)
            .map_err(|m| LinkError::Splice {
                target: r.target.clone(),
                message: m,
            })?;
        linked.push(LinkedScript {
            target: r.target.clone(),
            contributors: vec![r.shipment.clone()],
            base_source_bytes: 0,
            linked_source_bytes: r.source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }

    // Mint each first-class `add_script` addition that gameplay loads or imports ([`mints_in`]). Each
    // one becomes a fresh entry in the `scripts_vz` block with its own primary type-35 ASET row (via
    // `script_patch_blocks`' new-entry branch), so the engine's `import` / `dynamic_import` locates it
    // the same way any shipped script is located. Lives in the `wifpmcinterior`-carrying block beside
    // `qm_modloader`, which keeps every minted script in one discoverable place.
    let gameplay_additions: Vec<&ScriptAddition> = additions.iter().filter(|a| mints_in(a, session)).collect();
    if !gameplay_additions.is_empty() {
        let bi = blocks
            .iter()
            .position(|tb| tb.block.find_script_by_name("wifpmcinterior").is_some())
            .ok_or_else(|| LinkError::UnknownScript {
                target: "wifpmcinterior".to_string(),
                shipment: gameplay_additions[0].shipment.clone(),
            })?;
        let mut additions = gameplay_additions;
        sort_by_order(&mut additions, order, |a| a.shipment.as_str(), |_| ())?;
        for a in additions {
            refuse_taken(blocks, &a.name, &a.shipment)?;
            let bytecode = mercs2_luac::compile(&a.source, &a.name)
                .map_err(|e| LinkError::Compile {
                    target: a.name.clone(),
                    message: e.to_string(),
                })?;
            blocks[bi]
                .block
                .add_script(&a.name, &bytecode)
                .map_err(|m| LinkError::Splice {
                    target: a.name.clone(),
                    message: m,
                })?;
            linked.push(LinkedScript {
                target: a.name.clone(),
                contributors: vec![a.shipment.clone()],
                base_source_bytes: 0,
                linked_source_bytes: a.source.len(),
                bytecode_bytes: bytecode.len(),
                block: bi,
            });
            if a.load.is_none() {
                imported.push(imported_module(&a.name, &a.shipment, session));
            }
        }
    }

    // Mint the mod loader. It is a NEW `scripts_vz` script — `add_script` appends its container and
    // (via `script_patch_blocks`' new-entry branch) its primary type-35 ASET row, the two halves the
    // DLC's own recipe ships. It goes in the block carrying `wifpmcinterior`, beside the additions.
    if needs_loader {
        let bi = blocks
            .iter()
            .position(|tb| tb.block.find_script_by_name("wifpmcinterior").is_some())
            .ok_or_else(|| LinkError::UnknownScript {
                target: "wifpmcinterior".to_string(),
                shipment: MODLOADER_CONTRIBUTOR.to_string(),
            })?;

        // Mint each NOVEL support subclass as its own new `scripts_vz` script, which the loader
        // imports, so it is a DEPS child the engine loads first. Same `add_script` mechanism as
        // `qm_modloader` below.
        // Two items of one Shipment sharing one subclass and one source mint it once; one module
        // from two Shipments, or from two sources, is refused rather than one source dropped.
        let mut minted: std::collections::BTreeMap<u32, &SupportRegistration> = std::collections::BTreeMap::new();
        let mut support_in_order: Vec<&SupportRegistration> = support_regs.iter().collect();
        sort_by_order(&mut support_in_order, order, |r| r.shipment.as_str(), |_| ())?;
        for r in support_in_order {
            let key = mercs2_formats::hash::pandemic_hash_m2(&r.module);
            if let Some(first) = minted.get(&key) {
                if first.shipment == r.shipment && first.source == r.source {
                    continue;
                }
                return Err(LinkError::SupportModuleTwice {
                    module: r.module.clone(),
                    first: first.shipment.clone(),
                    second: r.shipment.clone(),
                });
            }
            minted.insert(key, r);
            refuse_taken(blocks, &r.module, &r.shipment)?;
            let bytecode =
                mercs2_luac::compile(&r.source, &r.module).map_err(|e| LinkError::Compile {
                    target: r.module.clone(),
                    message: e.to_string(),
                })?;
            blocks[bi]
                .block
                .add_script(&r.module, &bytecode)
                .map_err(|m| LinkError::Splice {
                    target: r.module.clone(),
                    message: m,
                })?;
            linked.push(LinkedScript {
                target: r.module.clone(),
                contributors: vec![r.shipment.clone()],
                base_source_bytes: 0,
                linked_source_bytes: r.source.len(),
                bytecode_bytes: bytecode.len(),
                block: bi,
            });
            // `qm_modloader` imports it.
            imported.push(imported_module(&r.module, &r.shipment, session));
        }

        let source = qm_modloader_source(ui_regs, layer_regs, support_regs, sound_regs, order)?;
        refuse_taken(blocks, QM_MODLOADER_NAME, MODLOADER_CONTRIBUTOR)?;
        // BARE chunk name, like every other script here — see the module note.
        let bytecode =
            mercs2_luac::compile(&source, QM_MODLOADER_NAME).map_err(|e| LinkError::Compile {
                target: QM_MODLOADER_NAME.to_string(),
                message: e.to_string(),
            })?;
        blocks[bi]
            .block
            .add_script(QM_MODLOADER_NAME, &bytecode)
            .map_err(|m| LinkError::Splice {
                target: QM_MODLOADER_NAME.to_string(),
                message: m,
            })?;
        let mut names: Vec<&String> = ui_regs
            .iter()
            .map(|r| &r.shipment)
            .chain(layer_regs.iter().map(|r| &r.shipment))
            .chain(support_regs.iter().map(|r| &r.shipment))
            .chain(
                sound_regs
                    .iter()
                    .filter(|r| r.sessions.contains(&crate::manifest::LoadSession::Gameplay))
                    .map(|r| &r.shipment),
            )
            .collect();
        names.sort();
        names.dedup();
        sort_by_order(&mut names, order, |n| n.as_str(), |_| ())?;
        let contributors: Vec<String> = names.into_iter().cloned().collect();
        linked.push(LinkedScript {
            target: QM_MODLOADER_NAME.to_string(),
            contributors,
            base_source_bytes: 0,
            linked_source_bytes: source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }

    // qm's own hooks and the library loader: spliced last, so they run at the end of each host, after
    // every Shipment append already linked into it. qm's own library loads first.
    let shipment_libraries = session_libraries(additions, session, order)?;
    let qm = needs_loader.then(qm_modloader_library);
    let libraries: Vec<crate::loader::Library> = qm.iter().chain(&shipment_libraries).cloned().collect();
    let splice = match hooks {
        QmHooks::Always => true,
        QmHooks::ForLibraries => !libraries.is_empty(),
    };
    if splice {
        linked.extend(link_session_hooks(blocks, &libraries, session, master.as_deref_mut())?);
    }
    // The outfits after the hooks: their wrappers go around whatever the hooks wrapped.
    linked.extend(link_outfits(blocks, master.as_deref_mut(), level, outfits, order)?);
    // With no splice, `libraries` is empty: nothing is loaded at a hook.
    let mut modules = library_modules(session, qm.as_ref(), &shipment_libraries)?;
    modules.extend(imported);

    // Every block we touched must still verify. `replace_lua` recomputes each container's CSUM, so
    // a failure here means the block itself was left inconsistent.
    for bi in linked.iter().map(|l| l.block).collect::<std::collections::BTreeSet<_>>() {
        blocks[bi]
            .block
            .verify_csums()
            .map_err(|e| LinkError::Block(format!("CSUMs after linking {}: {e}", blocks[bi].path)))?;
    }

    // The import checks, only now, when every addition, support module and `qm_modloader` has been
    // minted into the blocks.
    use crate::imports::Source;
    let mut sources: Vec<Source<'_>> = Vec::new();
    for m in mutations {
        sources.push(Source { shipment: &m.shipment, what: format!("patch_lua append to {}", m.target), script: &m.target, source: &m.append });
    }
    for a in additions.iter().filter(|a| mints_in(a, session)) {
        sources.push(Source { shipment: &a.shipment, what: format!("add_script {}", a.name), script: &a.name, source: &a.source });
    }
    for r in replacements {
        sources.push(Source { shipment: &r.shipment, what: format!("replace_lua {}", r.target), script: &r.target, source: &r.source });
    }
    for r in support_regs {
        sources.push(Source { shipment: &r.shipment, what: format!("support module {}", r.module), script: &r.module, source: &r.source });
    }
    let import_only: Vec<(&str, &str)> =
        additions.iter().filter(|a| a.load.is_none() && mints_in(a, session)).map(|a| (a.name.as_str(), a.shipment.as_str())).collect();
    let findings =
        import_findings(blocks, master.as_deref(), &[WORLD_BOOT_ROOT, root.as_str()], &before, &sources, &import_only, &linked)?;
    Ok(LinkOutput {
        scripts: linked,
        findings,
        modules,
    })
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

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// The ordering property the whole design rests on: one resolved order, same bytes, whatever
    /// order the mutations arrive in.
    #[test]
    fn concatenation_follows_the_order_not_the_input_order() {
        let a = mutation("aaa-mod", "s", "-- A\n");
        let b = mutation("zzz-mod", "s", "-- Z\n");
        let order = names(&["aaa-mod", "zzz-mod"]);
        let (one, c1) = linked_source("base\n", &[&a, &b], &order).unwrap();
        let (two, c2) = linked_source("base\n", &[&b, &a], &order).unwrap();
        assert_eq!(one, two, "input order must not change the linked source");
        assert_eq!(c1, c2);
        assert_eq!(c1, vec!["aaa-mod", "zzz-mod"]);
        // Both appends must survive — this is the annihilation the linker exists to prevent.
        assert!(one.contains("-- A") && one.contains("-- Z"), "{one}");
    }

    /// A base script with no trailing newline must not weld into the first append.
    #[test]
    fn a_base_without_a_trailing_newline_is_separated() {
        let m = mutation("mod", "s", "print('x')\n");
        let (src, _) = linked_source("local t = 1", &[&m], &names(&["mod"])).unwrap();
        assert!(src.starts_with("local t = 1\n"), "{src}");
        assert!(
            !src.contains("local t = 1--"),
            "appended source welded onto the base: {src}"
        );
    }

    #[test]
    fn each_append_is_attributed_in_the_source() {
        let m = mutation("sean-devlin", "s", "-- outfit\n");
        let (src, _) = linked_source("base\n", &[&m], &names(&["sean-devlin"])).unwrap();
        assert!(src.contains("appended by Shipment: sean-devlin"), "{src}");
    }

    fn edge(first: usize, then: usize) -> OrderEdge {
        OrderEdge { first, then }
    }

    /// With no edges the order IS the request order, and nothing is held back.
    #[test]
    fn with_no_edges_the_order_is_the_request_order() {
        let r = resolve_load_order(3, &[]).unwrap();
        assert_eq!(r.order, vec![0, 1, 2]);
        assert_eq!(r.held_back_by, vec![None, None, None]);
    }

    /// A consumer listed before its provider waits for it, and records the edge that held it back.
    #[test]
    fn a_requires_edge_beats_request_order() {
        // 0 = ess, 1 = lua-bridge; ess requires lua-bridge.
        let r = resolve_load_order(2, &[edge(1, 0)]).unwrap();
        assert_eq!(r.order, vec![1, 0]);
        assert_eq!(r.held_back_by, vec![Some(0), None]);
    }

    /// The held-back edge is the one from the LAST-emitted predecessor.
    #[test]
    fn held_back_names_the_last_emitted_predecessor() {
        // 0 = consumer of both 1 and 2; 1 and 2 have no edges between them.
        let r = resolve_load_order(3, &[edge(1, 0), edge(2, 0)]).unwrap();
        assert_eq!(r.order, vec![1, 2, 0]);
        assert_eq!(r.held_back_by[0], Some(1), "2 went last, so its edge (index 1) held 0 back");
    }

    /// A cycle has no order. Every edge on it is named, and an edge merely downstream of it is not.
    #[test]
    fn a_cycle_names_every_edge_on_it_and_only_those() {
        // 0 -> 1 -> 2 -> 0 is the cycle; 2 -> 3 hangs off it.
        let edges = [edge(0, 1), edge(1, 2), edge(2, 0), edge(2, 3)];
        let cycles = resolve_load_order(4, &edges).expect_err("a cycle has no order");
        assert_eq!(cycles, vec![vec![0, 1, 2]]);
    }

    /// Two separate cycles are two findings, each with its own edges.
    #[test]
    fn two_cycles_are_reported_separately() {
        let edges = [edge(0, 1), edge(1, 0), edge(2, 3), edge(3, 2)];
        let cycles = resolve_load_order(4, &edges).expect_err("cycles");
        assert_eq!(cycles, vec![vec![0, 1], vec![2, 3]]);
    }

    /// The resolved order actually reorders the appends: `b` ahead of `a` in `order` puts `b`'s source
    /// first, whatever the names sort to.
    #[test]
    fn the_order_reorders_the_appends() {
        let a = mutation("a-mod", "s", "-- A\n");
        let b = mutation("b-mod", "s", "-- B\n");
        let order = names(&["b-mod", "a-mod"]);
        let (forced, contrib) = linked_source("base\n", &[&a, &b], &order).unwrap();
        assert!(forced.find("-- B").unwrap() < forced.find("-- A").unwrap(), "{forced}");
        assert_eq!(contrib, vec!["b-mod", "a-mod"]);
    }

    /// A contributor the order does not name is an internal error, never a guessed position.
    #[test]
    fn a_contributor_missing_from_the_order_is_an_error() {
        let a = mutation("a-mod", "s", "-- A\n");
        match linked_source("base\n", &[&a], &names(&["other"])) {
            Err(LinkError::NotInOrder { shipment }) => assert_eq!(shipment, "a-mod"),
            other => panic!("expected NotInOrder, got {other:?}"),
        }
    }

    /// qm's own contributor is not a Shipment: it is never in `order`, and it goes last.
    #[test]
    fn the_modloader_contributor_sorts_last() {
        let t = mutation(MODLOADER_CONTRIBUTOR, "s", "-- T\n");
        let a = mutation("a-mod", "s", "-- A\n");
        let (src, contrib) = linked_source("base\n", &[&t, &a], &names(&["a-mod"])).unwrap();
        assert!(src.find("-- A").unwrap() < src.find("-- T").unwrap(), "{src}");
        assert_eq!(contrib, vec!["a-mod", MODLOADER_CONTRIBUTOR]);
    }

    fn reg(shipment: &str, movie: &str) -> UiRegistration {
        UiRegistration {
            shipment: shipment.into(),
            movie: movie.into(),
        }
    }

    /// The loader reaches qm's dispatcher through `_G._QM` and subscribes each movie's FlashWidget to
    /// `world.begin` under its own Shipment, once per VM, with nothing swallowing an error.
    #[test]
    fn the_mod_loader_subscribes_each_movie_under_its_shipment() {
        let src = qm_modloader_source(
            &[reg("mod-a", "my_hud"), reg("mod-b", "my_map")],
            &[],
            &[],
            &[],
            &names(&["mod-a", "mod-b"]),
        )
        .unwrap();
        assert!(src.contains("local QM = _G._QM\n"), "reaches the dispatcher through _G: {src}");
        assert!(src.contains("import(\"MrxGuiBase\")\n"), "imports the module FlashWidget is in: {src}");
        for (shipment, movie) in [("mod-a", "my_hud"), ("mod-b", "my_map")] {
            assert!(src.contains(&format!("w:SetSwfFile(\"{movie}\")")), "missing {movie}: {src}");
            assert!(src.contains(&format!("if QM.ran[\"{shipment} ui {movie}\"] then")), "once per VM: {src}");
            assert!(src.contains(&format!("QM.ui[\"{movie}\"] = w\nend, \"{shipment}\")")), "owned by {shipment}: {src}");
        }
        assert_eq!(src.matches("MrxGuiBase.FlashWidget:new()").count(), 2, "one widget per movie: {src}");
        assert!(!src.contains("pcall") && !src.contains("if w."), "no error is swallowed: {src}");
        mercs2_luac::compile(&src, QM_MODLOADER_NAME).expect("the loader compiles");
    }

    /// The loader is qm's own library, loaded at the world entry, named for the error policy as qm.
    #[test]
    fn the_loaders_are_qm_s_own_libraries_at_the_session_entries() {
        let gameplay = qm_modloader_library();
        assert_eq!((gameplay.module.as_str(), gameplay.owner.as_str(), gameplay.when.as_str()), (QM_MODLOADER_NAME, crate::loader::QM_OWNER, "world.scripts_loaded"));
        let front = qm_shell_modloader_library();
        assert_eq!((front.module.as_str(), front.owner.as_str(), front.when.as_str()), (QM_SHELL_MODLOADER_NAME, crate::loader::QM_OWNER, "shell.boot"));
        let hooks = crate::loader::generated_hooks(LoadSession::Gameplay, std::slice::from_ref(&gameplay)).unwrap();
        let tails = crate::loader::host_tails(&hooks, &[gameplay]).unwrap();
        let vz = &tails[&(LoadSession::Gameplay, "vz".to_string())].source;
        assert!(vz.find("QM.fire(\"world.scripts_loaded\")").unwrap() < vz.find("QM.load(\"qm_modloader\", \"Quartermaster\", true)").unwrap(), "{vz}");
    }

    /// Same registrations and order, same bytes, whatever order the registrations arrive in.
    #[test]
    fn the_bake_is_independent_of_input_order() {
        let order = names(&["aaa", "zzz"]);
        let a = qm_modloader_source(&[reg("aaa", "one"), reg("zzz", "two")], &[], &[], &[], &order).unwrap();
        let b = qm_modloader_source(&[reg("zzz", "two"), reg("aaa", "one")], &[], &[], &[], &order).unwrap();
        assert_eq!(a, b, "input order must not change the baked loader");
        assert!(a.find("one").unwrap() < a.find("two").unwrap(), "{a}");
    }

    /// The bake follows the resolved order, not the names. `ess` is ahead of the consumer in
    /// `order` (the consumer requires it), so its registration is baked first even though the
    /// consumer's name sorts earlier.
    #[test]
    fn the_bake_follows_the_resolved_order() {
        let order = names(&["ess", "a-consumer"]);
        let src = qm_modloader_source(
            &[reg("a-consumer", "consumer_hud"), reg("ess", "ess_ui")],
            &[],
            &[],
            &[],
            &order,
        )
        .unwrap();
        assert!(
            src.find("ess_ui").unwrap() < src.find("consumer_hud").unwrap(),
            "ess must be registered before the Shipment that requires it: {src}"
        );
    }

    /// A movie name cannot break out of its Lua string and inject code into the block we compile.
    #[test]
    fn a_movie_name_is_escaped_in_the_bake() {
        let src =
            qm_modloader_source(&[reg("m", "evil\") os.exit() --")], &[], &[], &[], &names(&["m"])).unwrap();
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

    /// A layer activation subscribes MarkForRemoval(old) then MarkForAddition(new) to `world.begin`
    /// under its Shipment, like the UI widgets — the vanilla contract order.
    #[test]
    fn the_mod_loader_marks_layers_for_addition_and_removal() {
        let src = qm_modloader_source(
            &[],
            &[layer("act-mod", "vz_state_pmccon004_destroyed", &["vz_state_pmccon004_pristine"])],
            &[],
            &[],
            &names(&["act-mod"]),
        )
        .unwrap();
        assert!(src.contains("import(\"MrxLayerManager\")\n"), "imports what it calls: {src}");
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
        assert!(src.contains("if QM.ran[\"act-mod layer vz_state_pmccon004_destroyed\"] then"), "once per VM: {src}");
        assert!(src.contains("end, \"act-mod\")"), "owned by its Shipment: {src}");
        assert!(!src.contains("pcall") && !src.contains("if MrxLayerManager"), "no error is swallowed: {src}");
    }

    /// UI and layer registrations coexist in one loader, and one order bakes byte-identically
    /// whatever order the registrations arrive in.
    #[test]
    fn ui_and_layer_registrations_share_one_deterministic_loader() {
        let order = names(&["aaa", "ui-mod", "zzz"]);
        let a = qm_modloader_source(
            &[reg("ui-mod", "my_hud")],
            &[layer("aaa", "layer_a", &[]), layer("zzz", "layer_z", &[])],
            &[],
            &[],
            &order,
        )
        .unwrap();
        let b = qm_modloader_source(
            &[reg("ui-mod", "my_hud")],
            &[layer("zzz", "layer_z", &[]), layer("aaa", "layer_a", &[])],
            &[],
            &[],
            &order,
        )
        .unwrap();
        assert_eq!(a, b, "input order must not change the baked loader");
        assert!(a.contains("w:SetSwfFile(\"my_hud\")"), "the widget is still baked: {a}");
        assert!(a.find("layer_a").unwrap() < a.find("layer_z").unwrap(), "ordered by load order: {a}");
    }

    /// A layer name cannot break out of its Lua string and inject code into the block we compile.
    #[test]
    fn a_layer_name_is_escaped_in_the_bake() {
        let src = qm_modloader_source(
            &[],
            &[layer("m", "evil\") os.exit() --", &[])],
            &[],
            &[],
            &names(&["m"]),
        )
        .unwrap();
        assert!(src.contains("evil\\\") os.exit()"), "the embedded quote must be escaped: {src}");
        assert!(!src.contains("Addition(\"evil\") os"), "the injection must not close the string: {src}");
    }

    fn sound_in(shipment: &str, bank: &str, soundbank: bool, sessions: &[LoadSession]) -> SoundBankRegistration {
        SoundBankRegistration {
            shipment: shipment.into(),
            bank: bank.into(),
            load: SoundLoad::Shipped { soundbank },
            sessions: sessions.iter().copied().collect(),
        }
    }

    fn sound(shipment: &str, bank: &str, soundbank: bool) -> SoundBankRegistration {
        sound_in(shipment, bank, soundbank, &[LoadSession::Gameplay])
    }

    use crate::manifest::LoadSession;

    /// A `load_sound`: the stream opens before any bank loads, from its level's folder by the DLC's own
    /// path, and closes after every bank unloads; the soundbank and the wavebank load alone, each by its
    /// own name, and unload with their mirrors.
    #[test]
    fn a_load_sound_opens_its_stream_first_and_closes_it_last() {
        let load = |bank: &str, load: SoundLoad| SoundBankRegistration {
            shipment: "biua".into(),
            bank: bank.into(),
            load,
            sessions: [LoadSession::Gameplay].into(),
        };
        let regs = [
            load("vo_resident_DLCTest", SoundLoad::SoundBank),
            load("vo_stream_DLCTest", SoundLoad::WaveBank),
            load("vo_stream_DLCTest", SoundLoad::Stream { folder: "DLC\\dlc01".into() }),
            sound("a-mod", "a_sounds", true),
        ];
        let src = qm_modloader_source(&[], &[], &[], &regs, &names(&["biua", "a-mod"])).unwrap();
        let at = |needle: &str| src.find(needle).unwrap_or_else(|| panic!("{needle} missing: {src}"));
        let open = at("Sound.OpenStreamFile(\"DLC\\\\dlc01\" .. \"\\\\\" .. string.upper(Sound.GetAudioDir()) .. \"\\\\\" .. \
                       string.upper(MrxSoundBanks._GetLocalizedName(\"vo_stream_DLCTest\")) .. \".PWS\", \"vo_stream_DLCTest.pws\")");
        let close = at("Sound.CloseStreamFile(");
        assert!(open < at("MrxSoundBanks.LoadWaveBank(\"vo_stream_DLCTest\")") && open < at("LoadWaveBank(\"a_sounds\")"), "{src}");
        assert!(close > at("UnloadSoundBank(\"vo_resident_DLCTest\")") && close > at("UnloadWaveBank(\"a_sounds\")"), "{src}");
        assert!(!src.contains("LoadSoundBank(\"vo_stream_DLCTest\")") && !src.contains("LoadWaveBank(\"vo_resident_DLCTest\")"), "{src}");
        assert!(src.contains("QM.sounds[\"soundbank vo_resident_DLCTest\"]") && src.contains("QM.sounds[\"vo_stream_DLCTest.pws\"]"), "{src}");
        assert_eq!((src.matches("OpenStreamFile(").count(), src.matches("CloseStreamFile(").count()), (1, 1));
        if let Err(e) = mercs2_luac::compile(&src, "qm_modloader") {
            panic!("the loader does not compile: {e}\n{src}");
        }
    }

    /// The gameplay loader loads each bank at `world.begin` as the retail load calls (the wavebank,
    /// then, for an added bank, its soundbank) in load order then bank name, whatever order the
    /// registrations arrive in; it unloads each at `world.sound_exit` with the mirrors; a bank name is
    /// escaped like any other; and a front-end registration is not in it.
    #[test]
    fn the_mod_loader_loads_gameplay_sound_banks_in_load_order() {
        let order = names(&["ess", "b-mod", "a-mod"]);
        let regs = [
            sound("a-mod", "a_sounds", true),
            sound("ess", "qm_ess_ui_hud", false),
            sound("b-mod", "zz_bank", true),
            sound("b-mod", "b_bank", true),
            sound_in("a-mod", "menu_only", true, &[LoadSession::FrontEnd]),
        ];
        let widget = [reg("ess", "ess_ui")];
        let src = qm_modloader_source(&widget, &[], &[], &regs, &order).unwrap();
        let mut reversed = regs.clone();
        reversed.reverse();
        assert_eq!(src, qm_modloader_source(&widget, &[], &[], &reversed, &order).unwrap());

        let at = |needle: &str| src.find(needle).unwrap_or_else(|| panic!("{needle} missing: {src}"));
        assert!(at("LoadWaveBank(\"qm_ess_ui_hud\")") < at("LoadWaveBank(\"b_bank\")"));
        assert!(at("LoadWaveBank(\"b_bank\")") < at("LoadWaveBank(\"zz_bank\")"));
        assert!(at("LoadWaveBank(\"zz_bank\")") < at("LoadWaveBank(\"a_sounds\")"));
        assert!(at("UnloadWaveBank(\"qm_ess_ui_hud\")") < at("UnloadWaveBank(\"b_bank\")"));
        assert!(at("UnloadWaveBank(\"zz_bank\")") < at("UnloadWaveBank(\"a_sounds\")"));
        assert!(at("LoadWaveBank(\"b_bank\")") < at("LoadSoundBank(\"b_bank\")"), "wavebank, then soundbank");
        assert!(!src.contains("LoadSoundBank(\"qm_ess_ui_hud\")"), "an override's wavebank only: {src}");
        assert_eq!(src.matches(".LoadSoundBank(").count(), 3);
        assert_eq!(src.matches(".UnloadSoundBank(").count(), 3);
        assert!(!src.contains("menu_only"), "a front-end bank is the front end's: {src}");
        assert!(
            src.contains("QM.on(\"world.begin\", function()\n  if not QM.sounds[\"b_bank\"] then\n    MrxSoundBanks.LoadWaveBank(\"b_bank\")"),
            "a bank loads at world.begin, outside the once-per-VM guard: {src}"
        );
        assert!(src.contains("QM.on(\"world.sound_exit\", function()\n  if QM.sounds[\"b_bank\"] then"), "{src}");
        assert!(src.contains("QM.sounds[\"b_bank\"] = true\n  end\nend, \"b-mod\")"), "per-bank load state, owned by its Shipment: {src}");
        assert!(!src.contains("if MrxSoundBanks"), "no existence check on the sound path: {src}");
        assert!(src.starts_with("-- ") && src.contains("\nimport(\"MrxSoundBanks\")\n"), "imports what it calls: {src}");

        let evil = qm_modloader_source(&[], &[], &[], &[sound("m\nos.exit()", "x\") os.exit() --", true)], &names(&["m\nos.exit()"]))
            .unwrap();
        assert!(evil.contains("x\\\") os.exit()"), "the embedded quote must be escaped: {evil}");
        assert!(!evil.contains("\nos.exit()"), "a Shipment name cannot break out of its comment: {evil}");
    }

    /// Both loaders' sound subscriptions come from ONE generator: the front end's are the gameplay
    /// loader's at the front end's hooks, for the same registrations.
    #[test]
    fn both_loaders_share_one_sound_generator() {
        let both = [LoadSession::Gameplay, LoadSession::FrontEnd];
        let regs = [sound_in("a", "qm_a_ui_hud", false, &both), sound_in("a", "added", true, &both)];
        let order = names(&["a"]);
        let gameplay = qm_modloader_source(&[], &[], &[], &regs, &order).unwrap();
        let front = qm_shell_modloader_source(&regs, &order).unwrap();
        let body = |src: &str, load: &str, unload: &str| -> String {
            let start = src.find("QM.on(").expect("the subscriptions");
            src[start..].replace(&format!("\"{load}\""), "LOAD").replace(&format!("\"{unload}\""), "UNLOAD")
        };
        assert_eq!(body(&gameplay, "world.begin", "world.sound_exit"), body(&front, "shell.open", "shell.sound_exit"));
    }

    /// The front-end loader: the front-end banks only, in load order then bank name, through
    /// `_G._QM`; it imports `MrxSoundBanks` and carries no `pcall` and no existence check.
    #[test]
    fn the_front_end_loader_loads_front_end_banks_only() {
        let order = names(&["z-first", "a-second"]);
        let regs = [
            sound_in("a-second", "qm_a-second_ui_shell", false, &[LoadSession::FrontEnd]),
            sound_in("z-first", "zz", true, &[LoadSession::FrontEnd, LoadSession::Gameplay]),
            sound_in("z-first", "aa", true, &[LoadSession::FrontEnd]),
            sound_in("z-first", "gameplay_only", true, &[LoadSession::Gameplay]),
        ];
        let src = qm_shell_modloader_source(&regs, &order).unwrap();
        let at = |needle: &str| src.find(needle).unwrap_or_else(|| panic!("{needle} missing: {src}"));
        assert!(at("LoadWaveBank(\"aa\")") < at("LoadWaveBank(\"zz\")"));
        assert!(at("LoadWaveBank(\"zz\")") < at("LoadWaveBank(\"qm_a-second_ui_shell\")"));
        assert!(!src.contains("gameplay_only"), "{src}");
        assert!(src.contains("local QM = _G._QM\n"), "{src}");
        assert!(src.contains("import(\"MrxSoundBanks\")"), "{src}");
        assert!(!src.contains("pcall") && !src.contains("if MrxSoundBanks"), "{src}");
        mercs2_luac::compile(&src, QM_SHELL_MODLOADER_NAME).expect("the front-end loader compiles");
        assert!(matches!(qm_shell_modloader_source(&regs, &names(&["z-first"])), Err(LinkError::NotInOrder { .. })));
    }

    /// A Lua VM with the module system retail's `_SYS._IMPORT` implements: each module runs in its
    /// own environment (`__index` → `_G`), `import(m)` loads it once and binds it in the caller's
    /// environment, and `_G._MODULES` holds every loaded module. `dynamic_import(m, cb)` runs `m` and
    /// calls `cb(env)` when `__dyn` is `"run"`, calls `cb(nil)` when it is `"nil"`, never calls back
    /// when it is `"never"`, and queues the request for `__answer()` when it is `"defer"`. `print`,
    /// `MrxSoundBanks` and each host script's retail function
    /// record every call in the global `calls`; `Pg.LoadIsRetry()` returns `__retry`;
    /// `Sys.RequestGameState` records its state. `sources` maps a module name to its source.
    fn module_vm(sources: &[(&str, String)]) -> mercs2_luac::rt::Lua {
        let lua = mercs2_luac::rt::Lua::new().expect("a Lua VM");
        let t = lua.create_table().unwrap();
        for (name, src) in sources {
            t.set(*name, src.as_str()).unwrap();
        }
        lua.globals().set("__sources", t).unwrap();
        lua.load(
            r#"
calls = {}
_MODULES = {}
__dyn = "run"
__retry = false
function print(s) table.insert(calls, s) end
Pg = { LoadIsRetry = function() return __retry end }
Event = { ScriptEvent = "ScriptEvent", GameStateChange = "GameStateChange", CreatePersistent = function() end }
Sys = { RequestGameState = function(s) table.insert(calls, "RequestGameState " .. s) end }
local function load(name)
  local m = _MODULES[name]
  if not m then
    local src = __sources[name]
    if not src then error("import: no module " .. name) end
    m = setmetatable({}, { __index = _G })
    _MODULES[name] = m
    local f = assert(loadstring(src, name))
    setfenv(f, m)
    f()
  end
  return m
end
function import(name)
  local m = load(name)
  getfenv(2)[name] = m
  return m
end
__pending = {}
function dynamic_import(name, cb)
  if __dyn == "run" then
    cb(load(name))
  elseif __dyn == "nil" then
    cb(nil)
  elseif __dyn == "defer" then
    table.insert(__pending, { name, cb })
  end
  return true
end
function __answer()
  local p = __pending
  __pending = {}
  for i = 1, #p do
    p[i][2](load(p[i][1]))
  end
end
"#,
        )
        .exec()
        .expect("the module system");
        lua
    }

    /// Each host script of `session` as qm's link leaves it, by host, over the hooks a link generates
    /// ([`crate::loader::generated_hooks`]) for `libraries`: `retail` (the functions a test gives the
    /// host, by host), then an empty function for each other function the tail wraps, then the tail.
    fn session_hosts(session: LoadSession, libraries: &[crate::loader::Library], retail: &[(&str, &str)]) -> BTreeMap<String, String> {
        let hooks = crate::loader::generated_hooks(session, libraries).unwrap();
        crate::loader::host_tails(&hooks, libraries)
            .unwrap()
            .into_iter()
            .filter(|((s, _), _)| *s == session)
            .map(|((_, host), t)| {
                let own = retail.iter().find(|(h, _)| *h == host).map(|(_, src)| *src).unwrap_or("");
                let mut src = own.to_string();
                for (target, _) in &t.wraps {
                    if !own.contains(&format!("function {target}(")) {
                        src.push_str(&format!("function {target}() end\n"));
                    }
                }
                src.push_str(&t.source);
                (host, src)
            })
            .collect()
    }

    const GUI_BASE: &str = r#"
FlashWidget = {}
function FlashWidget:new() return setmetatable({}, { __index = FlashWidget }) end
function FlashWidget:SetSwfFile(n) table.insert(calls, "SetSwfFile " .. n) end
function FlashWidget:Play() table.insert(calls, "Play") end
function FlashWidget:SetVisible(v) table.insert(calls, "SetVisible " .. tostring(v)) end
"#;

    /// A gameplay session: the retail hosts with qm's tails spliced on, `qm_modloader` as `source`,
    /// and the retail modules it imports.
    fn gameplay_vm(source: String, extra: &[(&str, String)]) -> mercs2_luac::rt::Lua {
        let hosts = session_hosts(
            LoadSession::Gameplay,
            &[qm_modloader_library()],
            &[
                ("vz", "function _StartPlayerVisibleGameplay() table.insert(calls, \"retail start\") end\n"),
                ("mrxsoundbootstrap", "function ExitGame() table.insert(calls, \"retail ExitGame\") end\n"),
            ],
        );
        let mut sources = vec![
            ("vz", hosts["vz"].clone()),
            ("mrxsoundbootstrap", hosts["mrxsoundbootstrap"].clone()),
            ("MrxSoundBanks", BANKS.to_string()),
            ("MrxGuiBase", GUI_BASE.to_string()),
            (QM_MODLOADER_NAME, source),
        ];
        sources.extend(extra.iter().cloned());
        module_vm(&sources)
    }

    const BANKS: &str = r#"
function LoadWaveBank(n, cb) table.insert(calls, "LoadWaveBank " .. n .. (cb and " +cb" or "")) end
function LoadSoundBank(n, cb) table.insert(calls, "LoadSoundBank " .. n .. (cb and " +cb" or "")) end
function UnloadWaveBank(n, cb) table.insert(calls, "UnloadWaveBank " .. n .. (cb and " +cb" or "")) end
function UnloadSoundBank(n, cb) table.insert(calls, "UnloadSoundBank " .. n .. (cb and " +cb" or "")) end
"#;

    fn calls(lua: &mercs2_luac::rt::Lua) -> Vec<String> {
        let t: mercs2_luac::rt::Table = lua.globals().get("calls").unwrap();
        t.sequence_values::<String>().map(|v| v.unwrap()).collect()
    }

    /// ★ The front end, run: `shell.boot` loads `qm_shell_modloader`; each `shell.open` (after retail's
    /// `EnterShellState`) loads the front-end banks in order, each bank once per load, and each
    /// `shell.sound_exit` (after retail's `ExitShellState`) unloads what was loaded; a second open
    /// loads nothing again, a second exit unloads nothing, and the next open loads again. Every hook
    /// prints its entry line. No callback is passed, so retail's batch callback stands.
    #[test]
    fn the_front_end_loads_its_banks_at_each_shell_open_and_unloads_them_after_each_sound_exit() {
        let order = names(&["a", "b"]);
        let regs = [
            sound_in("b", "qm_b_ui_hud", false, &[LoadSession::FrontEnd]),
            sound_in("a", "added", true, &[LoadSession::FrontEnd]),
        ];
        let hosts = session_hosts(
            LoadSession::FrontEnd,
            &[qm_shell_modloader_library()],
            &[(
                "mrxsound",
                "function EnterShellState() table.insert(calls, \"retail EnterShellState\") end\n\
                 function ExitShellState() table.insert(calls, \"retail ExitShellState\") end\n",
            )],
        );
        let lua = module_vm(&[
            ("mrxsound", hosts["mrxsound"].clone()),
            ("shellbootstrap", hosts["shellbootstrap"].clone()),
            ("MrxSoundBanks", BANKS.to_string()),
            (QM_SHELL_MODLOADER_NAME, qm_shell_modloader_source(&regs, &order).unwrap()),
        ]);
        lua.load(
            "import(\"mrxsound\")\nimport(\"shellbootstrap\")\nmrxsound.EnterShellState()\nmrxsound.EnterShellState()\n\
             mrxsound.ExitShellState()\nmrxsound.ExitShellState()\nmrxsound.EnterShellState()\n",
        )
        .exec()
        .expect("the front end runs");
        assert_eq!(
            calls(&lua),
            [
                "[qm] hook shell.boot: enter",
                "[qm] library qm_shell_modloader: ready",
                "retail EnterShellState",
                "[qm] hook shell.open: enter",
                "LoadWaveBank added",
                "LoadSoundBank added",
                "LoadWaveBank qm_b_ui_hud",
                "retail EnterShellState",
                "[qm] hook shell.open: enter",
                "retail ExitShellState",
                "[qm] hook shell.sound_exit: enter",
                "UnloadWaveBank added",
                "UnloadSoundBank added",
                "UnloadWaveBank qm_b_ui_hud",
                "retail ExitShellState",
                "[qm] hook shell.sound_exit: enter",
                "retail EnterShellState",
                "[qm] hook shell.open: enter",
                "LoadWaveBank added",
                "LoadSoundBank added",
                "LoadWaveBank qm_b_ui_hud",
            ]
        );
    }

    /// ★ A gameplay session, run: the world entry loads `qm_modloader`; `world.begin` (after retail's
    /// `_StartPlayerVisibleGameplay`, not on a retry, when the same wrapper dispatches `world.retry`)
    /// runs each registration once per VM and loads the banks; `world.sound_exit` (after retail's
    /// `ExitGame`) unloads them. `_QM` reaches every host
    /// through `_G`, from modules that each run in their own environment.
    #[test]
    fn a_gameplay_session_loads_at_world_begin_and_unloads_after_exit_game() {
        let regs = [sound("a", "qm_a_ui_hud", false), sound("a", "added", true)];
        let lua = gameplay_vm(qm_modloader_source(&[reg("a", "my_hud")], &[], &[], &regs, &names(&["a"])).unwrap(), &[]);
        lua.load(
            "import(\"mrxsoundbootstrap\")\nimport(\"vz\")\nvz._StartPlayerVisibleGameplay()\n\
             __retry = true\nvz._StartPlayerVisibleGameplay()\n__retry = false\nvz._StartPlayerVisibleGameplay()\n\
             mrxsoundbootstrap.ExitGame()\n",
        )
        .exec()
        .expect("the session runs");
        assert_eq!(
            calls(&lua),
            [
                "[qm] hook world.scripts_loaded: enter",
                "[qm] library qm_modloader: ready",
                "retail start",
                "[qm] hook world.begin: enter",
                "SetSwfFile my_hud",
                "Play",
                "SetVisible true",
                "LoadWaveBank added",
                "LoadSoundBank added",
                "LoadWaveBank qm_a_ui_hud",
                "retail start",
                "[qm] hook world.retry: enter",
                "retail start",
                "[qm] hook world.begin: enter",
                "retail ExitGame",
                "[qm] hook world.sound_exit: enter",
                "UnloadWaveBank added",
                "UnloadSoundBank added",
                "UnloadWaveBank qm_a_ui_hud",
            ]
        );
    }

    /// ★ Two Shipments activating one layer, each superseding a different one, both run: each
    /// registration's once-guard is its own Shipment's, so the second's `MarkForRemoval` is not dropped.
    #[test]
    fn two_shipments_activating_one_layer_both_run() {
        let manager = "function MarkForRemoval(n) table.insert(calls, \"remove \" .. n) end\n\
                       function MarkForAddition(n) table.insert(calls, \"add \" .. n) end\n"
            .to_string();
        let src = qm_modloader_source(
            &[],
            &[layer("a-mod", "act_layer", &["pristine_a"]), layer("b-mod", "act_layer", &["pristine_b"])],
            &[],
            &[],
            &names(&["a-mod", "b-mod"]),
        )
        .unwrap();
        let lua = gameplay_vm(src, &[("MrxLayerManager", manager)]);
        lua.load("import(\"mrxsoundbootstrap\")\nimport(\"vz\")\nvz._StartPlayerVisibleGameplay()\nvz._StartPlayerVisibleGameplay()\n")
            .exec()
            .expect("runs");
        let marks: Vec<String> = calls(&lua).into_iter().filter(|c| c.starts_with("add ") || c.starts_with("remove ")).collect();
        assert_eq!(marks, ["remove pristine_a", "add act_layer", "remove pristine_b", "add act_layer"], "each once, both Shipments");
    }

    /// ★ The order seen live on 2026-11-07, loading a save into an HQ: `hq.enter` requests its library
    /// 9 ms before `world.begin`, and `dynamic_import` answers after `world.begin`. Only the entry's
    /// libraries are checked at the settle hook, so this is not a failure, and the late library comes up
    /// when the engine answers.
    #[test]
    fn a_library_requested_just_before_the_settle_hook_is_not_a_failure() {
        let late = crate::loader::Library {
            module: "qm_gate_hq".into(),
            owner: "gate".into(),
            when: "hq.enter".into(),
            sessions: vec![LoadSession::Gameplay],
        };
        let hosts = session_hosts(
            LoadSession::Gameplay,
            &[late],
            &[
                ("vz", "function _StartPlayerVisibleGameplay() table.insert(calls, \"retail start\") end\n"),
                ("mrxhq", "function GlobalEnter(bPmc) table.insert(calls, \"retail GlobalEnter\") end\n"),
            ],
        );
        let lua = module_vm(&[
            ("vz", hosts["vz"].clone()),
            ("mrxhq", hosts["mrxhq"].clone()),
            ("mrxsoundbootstrap", hosts["mrxsoundbootstrap"].clone()),
            ("qm_gate_hq", "print(\"[gate] qm_gate_hq: body ran\")\n".to_string()),
        ]);
        lua.load(
            "import(\"mrxsoundbootstrap\")\nimport(\"mrxhq\")\nimport(\"vz\")\n__dyn = \"defer\"\n\
             mrxhq.GlobalEnter(true)\nvz._StartPlayerVisibleGameplay()\n__answer()\n",
        )
        .exec()
        .expect("runs");
        assert_eq!(
            calls(&lua),
            [
                "[qm] hook world.scripts_loaded: enter",
                "retail GlobalEnter",
                "[qm] hook hq.enter: enter",
                "retail start",
                "[qm] hook world.begin: enter",
                "[gate] qm_gate_hq: body ran",
                "[qm] library qm_gate_hq: ready",
            ]
        );
    }

    /// ★ A novel shop item keeps its saved state: the world loads from a save holding the item's unlock
    /// status and stock (`world.load` passes it), retail restores the other rows before the item's row
    /// exists, and the loader applies the saved values when it inserts the row at `world.begin`. With
    /// no save, the row keeps the manifest's values.
    #[test]
    fn a_novel_shop_item_keeps_its_saved_unlock_status_and_stock() {
        let modules = [
            ("MrxSupportData", "tSupportData = {}\n".to_string()),
            ("MrxRewardData", "_tRewards = {}\n".to_string()),
            ("DLC_MrxGreenGoblinBomb", "function Create(self) return { SetSupportName = function() end } end\n".to_string()),
        ];
        let src = qm_modloader_source(&[], &[], &[support("bomb-mod", "ggbomb", "DLC_MrxGreenGoblinBomb")], &[], &names(&["bomb-mod"])).unwrap();
        let report = "import(\"MrxSupportData\")\nlocal row = MrxSupportData.tSupportData.ggbomb\n\
                      table.insert(calls, \"stock \" .. tostring(row.nGlobalStock) .. \", Pmc \" .. tostring(row.tUnlockStatus.Pmc))\n";
        let run = |load: &str| {
            let lua = gameplay_vm(src.clone(), &modules);
            lua.load(format!("import(\"mrxsoundbootstrap\")\nimport(\"vz\")\n{load}vz._StartPlayerVisibleGameplay()\n{report}")).exec().expect("runs");
            calls(&lua).last().cloned().expect("a report")
        };
        let saved = "vz.LoadSingleton({ tSupportData = { ggbomb = { tUnlockStatus = { Pmc = 2 }, nGlobalStock = 5 } } })\n";
        assert_eq!(run(saved), "stock 5, Pmc 2", "the saved state is applied");
        assert_eq!(run("vz.LoadSingleton(nil)\n"), "stock nil, Pmc 1", "a new game keeps the manifest's values");
    }

    /// ★ `menu.open`, run against retail's own `HandleInput` (`mrxguishell.lua:309-329`): it fires once,
    /// after the original, on the start press that sets `bHitStart`; never before the shell Flash has
    /// loaded, while the legal-screen delay holds, for another button, or for a second start press.
    #[test]
    fn menu_open_fires_once_on_the_start_press_that_brings_up_the_main_menu() {
        let retail = "import(\"MrxGuiBase\")\n_bWaitForDelay = 0\n\
                      function HandleInput(oWidget, tEvent)\n\
                        local oFlash = oWidget.CustomData.oFlash\n\
                        if _bWaitForDelay ~= 0 or MrxGuiBase.Joystick.BUTTON_PAD2_D ~= tEvent.ButtonPress or not oFlash.CustomData.bLoaded then\n\
                        elseif not oWidget.CustomData.bHitStart then\n\
                          oWidget.CustomData.bHitStart = true\n\
                          table.insert(calls, \"retail start press\")\n\
                        end\n\
                      end\n";
        let hosts = session_hosts(LoadSession::FrontEnd, &[], &[("mrxguishell", retail)]);
        let lua = module_vm(&[
            ("mrxguishell", hosts["mrxguishell"].clone()),
            ("MrxGuiBase", "Joystick = { BUTTON_PAD2_D = 7, BUTTON_ALT1_2 = 9 }\n".to_string()),
        ]);
        lua.load(
            "import(\"mrxguishell\")\n\
             local w = { CustomData = { oFlash = { CustomData = { bLoaded = false } } } }\n\
             table.insert(calls, \"press before the Flash loaded\")\n\
             mrxguishell.HandleInput(w, { ButtonPress = 7 })\n\
             w.CustomData.oFlash.CustomData.bLoaded = true\n\
             mrxguishell._bWaitForDelay = 1\n\
             table.insert(calls, \"press during the delay\")\n\
             mrxguishell.HandleInput(w, { ButtonPress = 7 })\n\
             mrxguishell._bWaitForDelay = 0\n\
             table.insert(calls, \"another button\")\n\
             mrxguishell.HandleInput(w, { ButtonPress = 9 })\n\
             table.insert(calls, \"the start press\")\n\
             mrxguishell.HandleInput(w, { ButtonPress = 7 })\n\
             table.insert(calls, \"a second start press\")\n\
             mrxguishell.HandleInput(w, { ButtonPress = 7 })\n",
        )
        .exec()
        .expect("the shell runs");
        let seen: Vec<String> = calls(&lua).into_iter().filter(|c| !c.starts_with("[qm] hook shell.")).collect();
        assert_eq!(
            seen,
            [
                "press before the Flash loaded",
                "press during the delay",
                "another button",
                "the start press",
                "retail start press",
                "[qm] hook menu.open: enter",
                "a second start press",
            ],
            "{:?}",
            calls(&lua)
        );
    }

    /// ★ The error policy, run. A registration that errors stops the hook naming its Shipment: with no
    /// retail dialog it prints `FATAL-NODIALOG` for pmc_bb and requests `Exiting`; with `MrxGuiDialogBox` loaded and a local
    /// player it opens `MrxGuiDialogBox.DisplayDialogBox`, and dismissing it requests `Exiting`. A library the engine
    /// cannot load (a `nil` callback), and one it never calls back for (caught at `world.begin`), fail
    /// naming the library.
    #[test]
    fn a_failure_stops_the_game_naming_its_owner() {
        let broken = "function MarkForRemoval() end\n".to_string();
        let src = qm_modloader_source(&[], &[layer("act-mod", "new_layer", &[])], &[], &[], &names(&["act-mod"])).unwrap();
        let run = |lua: &mercs2_luac::rt::Lua, pre: &str| {
            lua.load(format!("{pre}import(\"mrxsoundbootstrap\")\nimport(\"vz\")\nvz._StartPlayerVisibleGameplay()\n")).exec().expect("runs");
            calls(lua)
        };
        let fatal = |calls: &[String], prefix: &str| calls.iter().any(|c| c.starts_with(prefix));

        let nodialog = run(&gameplay_vm(src.clone(), &[("MrxLayerManager", broken.clone())]), "");
        assert!(fatal(&nodialog, "[qm] FATAL act-mod at world.begin: "), "{nodialog:?}");
        assert!(fatal(&nodialog, "[qm] FATAL-NODIALOG act-mod at world.begin: "), "{nodialog:?}");
        // With nothing watching the print (pmc_bb's quiet variants), the prologue exits itself, at once.
        let at = nodialog.iter().position(|c| c.starts_with("[qm] FATAL-NODIALOG ")).expect("printed");
        assert_eq!(nodialog.get(at + 1).map(String::as_str), Some("RequestGameState Exiting"), "{nodialog:?}");

        // Retail's DisplayDialogBox (mrxguidialogbox.lua:15): on a choice it calls fCallback with the
        // option's index appended to tCallbackArgs (_CloseAndCallCallback :369-382).
        let dialog = "function DisplayDialogBox(p, msg, opts, def, cb, args, x, y, h, v, pause, cancel)\n\
                      table.insert(calls, \"dialog \" .. msg .. \" / \" .. opts[1] .. \" / pause \" .. tostring(pause) .. \" / cancel \" .. tostring(cancel))\n\
                      _G.__choose = function(i) table.insert(args, i) cb(unpack(args)) end\n\
                      return {}\n\
                      end\n"
            .to_string();
        let lua = gameplay_vm(src.clone(), &[("MrxLayerManager", broken), ("MrxGuiDialogBox", dialog)]);
        let shown = run(&lua, "import(\"MrxGuiDialogBox\")\nPlayer = { GetLocalPlayer = function() return 1 end }\n");
        let line = shown.iter().find(|c| c.starts_with("dialog ")).unwrap_or_else(|| panic!("no dialog: {shown:?}"));
        assert!(line.starts_with("dialog Quartermaster: act-mod at world.begin: "), "{line}");
        assert!(line.ends_with(" / Exit / pause true / cancel 1"), "{line}");
        assert!(!fatal(&shown, "[qm] FATAL-NODIALOG"), "{shown:?}");
        lua.load("__choose(1)").exec().expect("chooses Exit");
        let after = calls(&lua);
        assert_eq!(&after[after.len() - 2..], ["[qm] FATAL dismissed: requesting Exiting", "RequestGameState Exiting"]);

        let refused = run(&gameplay_vm(src.clone(), &[]), "__dyn = \"nil\"\n");
        assert!(refused.contains(&"[qm] FATAL Quartermaster at load qm_modloader: the engine could not load module qm_modloader".to_string()), "{refused:?}");
        let never = run(&gameplay_vm(src, &[]), "__dyn = \"never\"\n");
        assert!(never.contains(&"[qm] FATAL Quartermaster at world.begin: library qm_modloader was requested but never loaded".to_string()), "{never:?}");
    }

    fn support(shipment: &str, id: &str, module: &str) -> SupportRegistration {
        SupportRegistration {
            shipment: shipment.into(),
            module: module.into(),
            source: "-- a novel MrxSupport subclass\n".into(),
            id: id.into(),
            name: format!("[{id}.name]"),
            description: format!("[{id}.desc]"),
            icon: "support_cluster_bomb".into(),
            item_type: "Airstrike".into(),
            cash_cost: 100000,
            fuel_cost: 50,
            max_stock: 8,
            unlock_table: "{ Pmc = 1 }".into(),
            cargo: None,
            delivery_vehicle: None,
            shops: vec!["Pmc".into()],
        }
    }

    /// A novel support behaviour DEFERS its whole catalog row into the loader: import the minted
    /// subclass, construct `oSupport`, write `tSupportData` live, re-apply the name the `Init` tail
    /// loop no longer runs for it, add the reward row, and null the never-invalidated shop cache —
    /// none of which can run at resident-load, which is the entire reason this path exists.
    #[test]
    fn the_mod_loader_defers_a_novel_support_behaviour() {
        let src = qm_modloader_source(
            &[],
            &[],
            &[support("bomb-mod", "ggbomb", "DLC_MrxGreenGoblinBomb")],
            &[],
            &names(&["bomb-mod"]),
        )
        .unwrap();
        let top = src.find("local QM = _G._QM").expect("the body");
        for m in ["MrxSupportData", "MrxRewardData", "DLC_MrxGreenGoblinBomb"] {
            let at = src.find(&format!("import(\"{m}\")\n")).unwrap_or_else(|| panic!("imports {m}: {src}"));
            assert!(at < top, "{m} is imported at the top, a DEPS child: {src}");
        }
        assert!(src.contains("DLC_MrxGreenGoblinBomb:Create()"), "constructs oSupport: {src}");
        assert!(
            src.contains("oSupport:SetSupportName(\"ggbomb\")"),
            "re-applies the name Init would have: {src}"
        );
        assert!(
            src.contains("MrxSupportData.tSupportData[\"ggbomb\"]"),
            "defers the catalog row: {src}"
        );
        assert!(
            src.contains("MrxRewardData._tRewards[\"ggbombReward_Pmc\"]"),
            "defers the reward row: {src}"
        );
        assert!(
            src.contains("MrxRewardData.gtAllSupport = nil"),
            "nulls the never-invalidated cache: {src}"
        );
        assert!(src.contains("MrxRewardData.gtAllSupport = nil\nend, \"bomb-mod\")"), "owned by its Shipment: {src}");
        assert!(!src.contains("pcall") && !src.contains("if oSupport.") && !src.contains("if MrxSupportData"), "no error is swallowed: {src}");
        mercs2_luac::compile(&src, QM_MODLOADER_NAME).expect("the loader compiles");
    }
}
