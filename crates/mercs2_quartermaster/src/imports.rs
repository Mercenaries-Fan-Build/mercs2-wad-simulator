//! What Shipment Lua reaches by module name, checked once a link has minted everything.
//!
//! A script body runs only when the engine queues its resource: through a running script's DEPS,
//! boot, `Sys.RequiredAsset(name, "script")` or `dynamic_import` (memory
//! `lua-import-never-executes-module`). `import` itself only binds a module whose body already ran.
//! So the link checks three things over the scripts it produced:
//!
//! - **M0209**: every DEPS entry of Shipment code names a script of the link's blocks. DEPS is read
//!   from the compiled chunk ([`mercs2_formats::script_meta::deps_of`]), exactly what the engine
//!   loads: one entry per `import`/`inherit` call with a literal name, in any call form (`import "x"`
//!   too), nested functions included. An entry nothing provides leaves its importer waiting on it, and
//!   an `import` of a name no loaded WAD holds crashes the game (`0x5AE362` on v1.1).
//! - **M0313**: every `add_script` module with no `load:` is reached from something that runs: a DEPS
//!   edge, or a literal by-name route ([`by_name_routes`]), from a retail script, a library, qm's
//!   loader or a module reached that way. A module nothing reaches never runs.
//! - **M0314**: the link leaves the engine's load order sound. A model of the loader
//!   ([`load_passes`]) runs on the blocks before and after the link. It is an error when an import
//!   the link adds is of a module whose body has not run when the importer's body runs, and Shipment
//!   code of the importer reads that module at its top level (in a DEPS cycle the loader runs a body
//!   while a module it imports has only its env built; the import binds that env, which fills when
//!   the module's body runs, so a read inside a function that runs later finds it full). It is a
//!   warning when the order the engine loads the scripts that were already loading in changes.
//!   Retail's own cycles (an 84-script cluster) never fire: only what the link changes counts.
//!
//! The lexer here ([`lua_tokens`]) reads Lua 5.1 source as tokens, so that comments and string
//! contents are never read as code. The source-level checks use it: the by-name routes, and the
//! `pcall(import(...))` warning (M0315, [`pcall_imports`]).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::script_meta::top_level_uses;
use mercs2_formats::scripts_block::{container_deps, container_info, ScriptsBlock};
use mercs2_formats::types::TYPE_HASH_SCRIPT;

/// Which of the link's import rules a finding breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportRule {
    /// M0209: a DEPS entry names no script of the link's blocks.
    Unresolved,
    /// M0313: an `add_script` with no `load:` that nothing that runs reaches.
    NothingRuns,
    /// M0314, an error: Shipment code reads, at its script's top level, a module the link makes it
    /// import whose body has not run when that script's body runs.
    LoadOrder,
    /// M0314, a warning: the link changes the order retail scripts load in.
    Reorder,
}

impl ImportRule {
    pub fn code(self) -> &'static str {
        match self {
            ImportRule::Unresolved => "M0209",
            ImportRule::NothingRuns => "M0313",
            ImportRule::LoadOrder | ImportRule::Reorder => "M0314",
        }
    }

    /// Whether a finding of this rule is a warning wherever it is reported: it never refuses a link or
    /// a build.
    pub fn is_warning(self) -> bool {
        self == ImportRule::Reorder
    }
}

/// One import finding of a link.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImportFinding {
    pub rule: ImportRule,
    /// The Shipments whose code it is in, in load order.
    pub shipments: Vec<String>,
    /// The module the finding is about: the unresolved name, the module nothing runs, the importer
    /// whose top level reads a module not yet run (M0314 load order), or the comma-separated scripts
    /// whose body order changes (M0314 reorder).
    pub module: String,
    pub message: String,
}

impl std::fmt::Display for ImportFinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.rule.code(), self.message)
    }
}

/// One piece of Shipment Lua a link put into a script.
#[derive(Debug, Clone)]
pub struct Source<'a> {
    pub shipment: &'a str,
    /// For the message: `patch_lua append to <target>`, `add_script <name>`, `replace_lua <target>`
    /// or `support module <name>`.
    pub what: String,
    /// The script it lands in: a `patch_lua` or `replace_lua` target, or the minted module's name.
    pub script: &'a str,
    pub source: &'a str,
}

/// Every script of `blocks`, by name hash: its INFO name and its DEPS hashes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DepsGraph {
    pub names: BTreeMap<u32, String>,
    pub deps: BTreeMap<u32, Vec<u32>>,
}

impl DepsGraph {
    /// The graph of every script entry of `blocks`. A container whose INFO or DEPS does not read is an
    /// error naming it.
    pub fn of<'b>(blocks: impl IntoIterator<Item = &'b ScriptsBlock>) -> Result<Self, String> {
        let mut g = DepsGraph::default();
        for block in blocks {
            for e in block.entries.iter().filter(|e| e.type_hash == TYPE_HASH_SCRIPT) {
                let name = container_info(&e.bytes).map_err(|m| format!("script 0x{:08X}: INFO: {m}", e.name_hash))?.name;
                let deps = container_deps(&e.bytes).map_err(|m| format!("script {name}: DEPS: {m}"))?;
                g.names.insert(e.name_hash, name);
                g.deps.insert(e.name_hash, deps);
            }
        }
        Ok(g)
    }

    fn name(&self, hash: u32) -> String {
        self.names.get(&hash).cloned().unwrap_or_else(|| format!("0x{hash:08X}"))
    }
}

/// A stage of one script's load: the loader's state-2, state-3 and state-4 handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The module's environment is built (`_MODULES[name]`, `_THIS`, `[1]` = its name); its table
    /// exists but holds nothing of its body yet.
    Env,
    /// The body runs.
    Body,
    /// `Init` runs.
    Init,
}

/// One pass of the engine's script loader: one root queued, then pumped until every queued script is
/// ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPass {
    /// The script queued.
    pub root: u32,
    /// Whether it is loaded by name at a time the model cannot order; false for the engine's own
    /// sequence ([`LinkChecks::roots`]).
    pub by_name: bool,
    /// Every stage the pass ran, in order.
    pub events: Vec<(Stage, u32)>,
    /// Every `(script, dependency)` whose dependency's body had not run when the script's body ran,
    /// in the order the bodies ran: what the script's top-level code reads from the dependency is not
    /// there yet (the dependency's table fills when its body runs).
    pub early: Vec<(u32, u32)>,
}

/// The engine's script loader over a [`DepsGraph`], read from pc-v10: queue `0x59CD30` (push the
/// script at the list head, then queue each DEPS child, so children end up ahead of it; a script is
/// queued once), step `0x59CD70` (a script climbs at most one state past the lowest of its direct
/// children, once per pump pass, guarded by the pass stamp) and pump `0x5A2BC0` (step every queued
/// script head to tail; unlink each one that is ready). Every script starts parsed (state 1); a DEPS
/// entry naming no script of the graph is passed over (M0209 reports it). Live-checked on retail
/// (2026-11-09): with the world's passes `gamebootstrap` then `vz`, the model gives the order the 84
/// scripts of retail's DEPS cycle ran their bodies in, exactly, and with a `load:` library's pass the
/// order its cycle ran in.
struct Loader<'g> {
    g: &'g DepsGraph,
    /// 2 env built, 3 body ran, 4 Init ran, 5 ready; a script not in the map is still parsed (1).
    state: BTreeMap<u32, u8>,
    mark: BTreeMap<u32, u64>,
    stamp: u64,
}

impl<'g> Loader<'g> {
    fn state(&self, r: u32) -> u8 {
        self.state.get(&r).copied().unwrap_or(1)
    }

    fn deps(&self, r: u32) -> impl Iterator<Item = u32> + 'g {
        let g = self.g;
        g.deps.get(&r).into_iter().flatten().copied().filter(move |d| g.names.contains_key(d))
    }

    fn queue(&self, r: u32, list: &mut Vec<u32>, queued: &mut BTreeSet<u32>) {
        if !queued.insert(r) {
            return;
        }
        list.insert(0, r);
        for c in self.deps(r) {
            self.queue(c, list, queued);
        }
    }

    fn step(&mut self, r: u32, top: bool, pass: &mut LoadPass) -> bool {
        if self.state(r) == 5 {
            return true;
        }
        if top {
            self.stamp += 1;
        }
        if self.mark.get(&r) == Some(&self.stamp) {
            return false;
        }
        self.mark.insert(r, self.stamp);
        let mut target = 5;
        for c in self.deps(r) {
            self.step(c, false, pass);
            target = target.min(self.state(c));
        }
        while self.state(r) <= target {
            let s = self.state(r) + 1;
            self.state.insert(r, s);
            match s {
                2 => pass.events.push((Stage::Env, r)),
                3 => {
                    pass.events.push((Stage::Body, r));
                    for d in self.deps(r) {
                        if self.state(d) < 3 && !pass.early.contains(&(r, d)) {
                            pass.early.push((r, d));
                        }
                    }
                }
                _ => {
                    pass.events.push((Stage::Init, r));
                    self.state.insert(r, 5);
                    return true;
                }
            }
            if s == target {
                return false;
            }
        }
        false
    }

    fn pump(&mut self, mut list: Vec<u32>, pass: &mut LoadPass) -> Result<(), String> {
        while !list.is_empty() {
            let was = (self.state.clone(), list.len());
            for r in list.clone() {
                if self.step(r, true, pass) {
                    list.retain(|&x| x != r);
                }
            }
            if (self.state.clone(), list.len()) == was {
                let stuck: Vec<String> = list.iter().map(|&h| self.g.name(h)).collect();
                return Err(format!("the loader model stopped with {} queued and none advancing", stuck.join(", ")));
            }
        }
        Ok(())
    }
}

/// Every load pass the engine makes over `g`. First the engine's own sequence: each of `roots` in
/// turn, every one pumped until ready before the next is queued (the world: `gamebootstrap`, then the
/// level's master; the front end: `shellbootstrap`). Every script that leaves unloaded then loads in a
/// pass of its own, started from the state the sequence left, rooted at each unloaded script no other
/// unloaded script depends on, and at each of `extra` still unloaded (a library's `load:`, a literal
/// by-name route). Those are loaded by name at a time the model cannot order, so each is run on its
/// own.
pub fn load_passes(g: &DepsGraph, roots: &[u32], extra: &BTreeSet<u32>) -> Result<Vec<LoadPass>, String> {
    let mut ld = Loader { g, state: BTreeMap::new(), mark: BTreeMap::new(), stamp: 0 };
    let mut out = Vec::new();
    for &r in roots {
        if !g.names.contains_key(&r) {
            return Err(format!("the loader root 0x{r:08X} is no script of the linked blocks"));
        }
        let mut list = Vec::new();
        ld.queue(r, &mut list, &mut BTreeSet::new());
        let mut pass = LoadPass { root: r, by_name: false, events: Vec::new(), early: Vec::new() };
        ld.pump(list, &mut pass)?;
        out.push(pass);
    }
    let settled = ld.state.clone();
    let unloaded: BTreeSet<u32> = g.names.keys().copied().filter(|h| !settled.contains_key(h)).collect();
    let depended: BTreeSet<u32> = unloaded.iter().flat_map(|&h| ld.deps(h)).filter(|d| unloaded.contains(d)).collect();
    let late: BTreeSet<u32> =
        unloaded.iter().copied().filter(|h| !depended.contains(h) || extra.contains(h)).collect();
    for r in late {
        ld.state = settled.clone();
        let mut list = Vec::new();
        ld.queue(r, &mut list, &mut BTreeSet::new());
        let mut pass = LoadPass { root: r, by_name: true, events: Vec::new(), early: Vec::new() };
        ld.pump(list, &mut pass)?;
        out.push(pass);
    }
    Ok(out)
}

/// The DEPS entries of `source` compiled on its own, the way the link compiles a Shipment's Lua.
/// A source that does not compile is an error with the compiler's message.
pub fn source_deps(source: &str, chunk_name: &str) -> Result<Vec<mercs2_formats::script_meta::Dep>, String> {
    Ok(source_chunk(source, chunk_name)?.1)
}

/// `source` compiled on its own, the way the link compiles a Shipment's Lua, with its DEPS entries.
fn source_chunk(source: &str, chunk_name: &str) -> Result<(Vec<u8>, Vec<mercs2_formats::script_meta::Dep>), String> {
    let luaq = mercs2_luac::compile(source, chunk_name).map_err(|e| e.to_string())?;
    let deps = mercs2_formats::script_meta::deps_of(&luaq)?;
    Ok((luaq, deps))
}

/// What one link's checks are given.
pub struct LinkChecks<'a> {
    /// Every script of the link's blocks before anything was linked.
    pub before: &'a DepsGraph,
    /// The same blocks after the link.
    pub after: &'a DepsGraph,
    /// The scripts the engine queues first when the link's blocks load, in order: its master pass
    /// ([`load_passes`]).
    pub roots: &'a [&'a str],
    /// Every piece of Shipment Lua the link put into a script.
    pub sources: &'a [Source<'a>],
    /// The `add_script` modules the link minted that have no `load:`, as `(module, shipment)`: each
    /// must be reached ([`ImportRule::NothingRuns`]).
    pub import_only: &'a [(&'a str, &'a str)],
    /// Who contributed each script the link changed or minted, by the script's name hash, in load
    /// order.
    pub contributors: &'a BTreeMap<u32, Vec<String>>,
}

/// The import findings of one link ([`ImportRule`]), sorted.
pub fn check(c: &LinkChecks<'_>) -> Result<Vec<ImportFinding>, String> {
    let mut out: BTreeSet<ImportFinding> = BTreeSet::new();
    let mut routes: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    // Which Shipments' sources list each `(script, dependency)` edge, in source order.
    let mut added_by: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
    // Each source compiled, with its DEPS, in source order.
    let mut compiled: Vec<(Vec<u8>, Vec<mercs2_formats::script_meta::Dep>)> = Vec::new();
    for s in c.sources {
        let (luaq, deps) = source_chunk(s.source, s.script).map_err(|m| format!("{}: its {}: {m}", s.shipment, s.what))?;
        compiled.push((luaq, deps.clone()));
        for d in deps {
            let by = added_by.entry((pandemic_hash_m2(s.script), d.hash)).or_default();
            if !by.iter().any(|x| x == s.shipment) {
                by.push(s.shipment.to_string());
            }
            if !c.after.names.contains_key(&d.hash) {
                let call = if d.inherit { "inherit" } else { "import" };
                out.insert(ImportFinding {
                    rule: ImportRule::Unresolved,
                    shipments: vec![s.shipment.to_string()],
                    module: d.name.clone(),
                    message: format!(
                        "{}: {call}({:?}) in its {} names no script of the linked blocks, no add_script \
                         of the set and no module the Quartermaster mints; the engine never runs a \
                         script whose dependency is missing, and an import of it crashes the game",
                        s.shipment, d.name, s.what
                    ),
                });
            }
        }
        let from = pandemic_hash_m2(s.script);
        for name in by_name_routes(s.source) {
            routes.entry(from).or_default().insert(pandemic_hash_m2(&name));
        }
    }

    // Every edge after the link, DEPS and by-name routes together.
    let mut edges: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for (&n, deps) in &c.after.deps {
        edges.entry(n).or_default().extend(deps.iter().copied().filter(|d| c.after.names.contains_key(d)));
    }

    // M0314: the engine's load order, before and after the link.
    for f in load_order_findings(c, &compiled, &added_by, &routes)? {
        out.insert(f);
    }

    // M0313: reachability from everything that runs.
    for (from, to) in routes {
        edges.entry(from).or_default().extend(to);
    }
    let import_only: BTreeSet<u32> = c.import_only.iter().map(|(m, _)| pandemic_hash_m2(m)).collect();
    let mut reached: BTreeSet<u32> = c.after.names.keys().copied().filter(|h| !import_only.contains(h)).collect();
    let mut queue: VecDeque<u32> = reached.iter().copied().collect();
    while let Some(n) = queue.pop_front() {
        for &m in edges.get(&n).into_iter().flatten() {
            if reached.insert(m) {
                queue.push_back(m);
            }
        }
    }
    for (module, shipment) in c.import_only {
        if !reached.contains(&pandemic_hash_m2(module)) {
            out.insert(ImportFinding {
                rule: ImportRule::NothingRuns,
                shipments: vec![shipment.to_string()],
                module: module.to_string(),
                message: format!(
                    "{shipment}: add_script {module:?} has no load: and nothing that runs reaches it: no \
                     import or inherit of it, and no literal sModuleName, dynamic_import or \
                     Sys.RequiredAsset naming it, in a retail script, a library or a module reached \
                     from one; the engine never runs it. Give it a load: or reach it from code that runs"
                ),
            });
        }
    }
    Ok(out.into_iter().collect())
}

/// The scripts whose DEPS the link changed or added, each with its DEPS after the link.
fn changed_scripts(c: &LinkChecks<'_>) -> BTreeMap<u32, Vec<u32>> {
    c.after.deps.iter().filter(|(h, deps)| c.before.deps.get(h) != Some(deps)).map(|(&h, d)| (h, d.clone())).collect()
}

/// Where a pass loads, for a message.
fn pass_name(g: &DepsGraph, pass: &LoadPass) -> String {
    format!("when {} loads", g.name(pass.root))
}

fn stage_name(g: &DepsGraph, (stage, h): (Stage, u32)) -> String {
    match stage {
        Stage::Env => format!("{}'s env", g.name(h)),
        Stage::Body => format!("{}'s body", g.name(h)),
        Stage::Init => format!("{}'s Init", g.name(h)),
    }
}

/// The first difference between `before`'s order and `after`'s order of the scripts `before` ran, as
/// `(what runs there now, what ran there before)`, or `None` when the order is the same.
fn first_difference(before: &LoadPass, after: &LoadPass) -> Option<((Stage, u32), Option<(Stage, u32)>)> {
    let ran: BTreeSet<u32> = before.events.iter().map(|&(_, h)| h).collect();
    let kept: Vec<(Stage, u32)> = after.events.iter().copied().filter(|(_, h)| ran.contains(h)).collect();
    if kept == before.events {
        return None;
    }
    let i = before.events.iter().zip(&kept).position(|(a, b)| a != b).unwrap_or(kept.len().min(before.events.len()));
    match (kept.get(i), before.events.get(i)) {
        (Some(&now), was) => Some((now, was.copied())),
        (None, Some(&was)) => Some((was, None)),
        (None, None) => None,
    }
}

/// M0314 over one link: the loader model ([`load_passes`]) run on the blocks before and after it.
/// An error ([`ImportRule::LoadOrder`]) for each piece of Shipment code that, at its script's top
/// level ([`top_level_uses`]), uses a module the link makes the script import whose body has not run
/// when the script's body does: the module's table holds nothing of its body then. A warning
/// ([`ImportRule::Reorder`]) for each pass whose order of the scripts that were already loading
/// changes, naming the Shipments whose code changed the scripts that alone reproduce the change, or
/// every changed script's when no single one does.
fn load_order_findings(
    c: &LinkChecks<'_>,
    compiled: &[(Vec<u8>, Vec<mercs2_formats::script_meta::Dep>)],
    added_by: &BTreeMap<(u32, u32), Vec<String>>,
    routes: &BTreeMap<u32, BTreeSet<u32>>,
) -> Result<Vec<ImportFinding>, String> {
    let roots: Vec<u32> = c.roots.iter().map(|r| pandemic_hash_m2(r)).collect();
    let before = load_passes(c.before, &roots, &BTreeSet::new())?;
    // What loaded on its own before still does (something loads it by name), and so does each literal
    // by-name route of Shipment code.
    let extra: BTreeSet<u32> = routes
        .values()
        .flatten()
        .copied()
        .chain(before.iter().filter(|p| p.by_name).map(|p| p.root))
        .collect();
    let after = load_passes(c.after, &roots, &extra)?;
    let changed = changed_scripts(c);
    let shipments_of = |a: u32| -> Result<Vec<String>, String> {
        let mut out: Vec<String> = Vec::new();
        let old: BTreeSet<u32> = c.before.deps.get(&a).into_iter().flatten().copied().collect();
        for &b in c.after.deps.get(&a).into_iter().flatten().filter(|b| !old.contains(b)) {
            // An edge no Shipment source lists is qm's own generated code (`qm_modloader` importing a
            // support module), which carries the contributors it was generated for.
            for s in added_by.get(&(a, b)).or_else(|| c.contributors.get(&a)).into_iter().flatten() {
                if !out.contains(s) {
                    out.push(s.clone());
                }
            }
        }
        if out.is_empty() {
            for s in c.contributors.get(&a).into_iter().flatten() {
                if !out.contains(s) {
                    out.push(s.clone());
                }
            }
        }
        if out.is_empty() {
            return Err(format!("internal error: the link changed the DEPS of {} but contributed nothing to it", c.after.name(a)));
        }
        Ok(out)
    };
    let mut out = Vec::new();

    // Shipment code that uses, at its script's top level, a module the link makes the script import
    // whose body has not run when the script's body does. An added edge no Shipment source lists is
    // qm's own generated code (`qm_modloader`), which uses what it imports only inside its hook
    // handlers ([`crate::link::qm_modloader_source`]).
    for pass in &after {
        for &(a, d) in &pass.early {
            if c.before.deps.get(&a).is_some_and(|old| old.contains(&d)) {
                continue;
            }
            let in_a: Vec<usize> = (0..c.sources.len()).filter(|&i| pandemic_hash_m2(c.sources[i].script) == a).collect();
            // The names `import`/`inherit` bind the module under in the script: as each call spells it.
            let bound: BTreeSet<&str> =
                in_a.iter().flat_map(|&i| &compiled[i].1).filter(|dep| dep.hash == d).map(|dep| dep.name.as_str()).collect();
            for &i in &in_a {
                let s = &c.sources[i];
                for &name in &bound {
                    let lines = top_level_uses(&compiled[i].0, name).map_err(|m| format!("{}: its {}: {m}", s.shipment, s.what))?;
                    if lines.is_empty() {
                        continue;
                    }
                    let lines: Vec<String> = lines.iter().map(u32::to_string).collect();
                    let (an, dn) = (c.after.name(a), c.after.name(d));
                    out.push(ImportFinding {
                        rule: ImportRule::LoadOrder,
                        shipments: vec![s.shipment.to_string()],
                        module: an.clone(),
                        message: format!(
                            "{}: its {} uses {name} at its top level (line {}), and {} imports {dn}, whose \
                             body the engine runs after {an}'s ({}): when that line runs, {dn}'s table holds \
                             nothing of its body yet. Use {name} inside a function that runs later: the \
                             engine calls a module's Init after the body of every module it imports",
                            s.shipment,
                            s.what,
                            lines.join(", "),
                            an,
                            pass_name(c.after, pass),
                        ),
                    });
                }
            }
        }
    }

    // A pass whose order of the scripts already loading changes.
    for b in &before {
        let a = after.iter().find(|p| p.root == b.root);
        let what = match a {
            None => format!(
                "{} no longer loads on its own: the link makes an earlier load run it",
                c.before.name(b.root)
            ),
            Some(a) => match first_difference(b, a) {
                None => continue,
                Some((now, Some(was))) => format!(
                    "the link changes the order the engine loads retail scripts in {}: {} now runs where {} ran",
                    pass_name(c.before, b),
                    stage_name(c.after, now),
                    stage_name(c.before, was)
                ),
                Some((now, None)) => format!(
                    "the link changes the order the engine loads retail scripts in {}: {} no longer runs",
                    pass_name(c.before, b),
                    stage_name(c.before, now)
                ),
            },
        };
        // The changed scripts that alone reproduce it.
        let mut culprits = Vec::new();
        for (&h, deps) in &changed {
            let mut one = c.before.clone();
            one.names.entry(h).or_insert_with(|| c.after.name(h));
            one.deps.insert(h, deps.clone());
            let alone = load_passes(&one, &roots, &BTreeSet::new())?;
            let same = match alone.iter().find(|p| p.root == b.root) {
                None => false,
                Some(p) => first_difference(b, p).is_none(),
            };
            if !same {
                culprits.push(h);
            }
        }
        if culprits.is_empty() {
            culprits = changed.keys().copied().collect();
        }
        let mut shipments: Vec<String> = Vec::new();
        for &h in &culprits {
            for s in shipments_of(h)? {
                if !shipments.contains(&s) {
                    shipments.push(s);
                }
            }
        }
        let scripts: Vec<String> = culprits.iter().map(|&h| c.after.name(h)).collect();
        out.push(ImportFinding {
            rule: ImportRule::Reorder,
            shipments: shipments.clone(),
            module: scripts.join(", "),
            message: format!("{}: {what} (through the DEPS the link gives {})", shipments.join(", "), scripts.join(", ")),
        });
    }
    Ok(out)
}

/// The module names `source` reaches by name without a DEPS edge, each a literal the engine runs
/// the named script from: a `sModuleName = "x"` field (the task system `dynamic_import`s a row's
/// module when it activates), `dynamic_import("x", …)` and `Sys.RequiredAsset("x", "script", …)`.
/// A name built at runtime is not seen.
pub fn by_name_routes(source: &str) -> Vec<String> {
    let t: Vec<LuaToken> = lua_tokens(source).into_iter().map(|t| t.kind).collect();
    let name = |i: usize, n: &str| matches!(t.get(i), Some(LuaToken::Name(x)) if x == n);
    let punct = |i: usize, p: &str| matches!(t.get(i), Some(LuaToken::Punct(x)) if *x == p);
    let string = |i: usize| match t.get(i) {
        Some(LuaToken::ShortString(s)) => Some(s.clone()),
        _ => None,
    };
    let global_call = |i: usize| !(i > 0 && (punct(i - 1, ".") || punct(i - 1, ":") || name(i - 1, "function")));
    let mut out = Vec::new();
    for i in 0..t.len() {
        if name(i, "sModuleName") && punct(i + 1, "=") && !(i > 0 && punct(i - 1, ".")) {
            out.extend(string(i + 2));
        } else if name(i, "dynamic_import") && global_call(i) && punct(i + 1, "(") {
            out.extend(string(i + 2));
        } else if name(i, "Sys") && global_call(i) && punct(i + 1, ".") && name(i + 2, "RequiredAsset") && punct(i + 3, "(") {
            if let (Some(module), true, Some(kind)) = (string(i + 4), punct(i + 5, ","), string(i + 6)) {
                if kind == "script" {
                    out.push(module);
                }
            }
        }
    }
    out
}

/// Every `import` call inside a `pcall` or `xpcall` in `source`, as `(byte offset of the import,
/// the protected form)`: `pcall(import, …)`, and an `import` call in the body of a function literal
/// that is the first argument of `pcall`/`xpcall`, outside any function nested in it. A missing
/// module crashes the game natively, so the `pcall` protects nothing (M0315).
pub fn pcall_imports(source: &str) -> Vec<(usize, &'static str)> {
    let t = lua_tokens(source);
    let name = |i: usize, n: &str| matches!(t.get(i).map(|t| &t.kind), Some(LuaToken::Name(x)) if x == n);
    let punct = |i: usize, p: &str| matches!(t.get(i).map(|t| &t.kind), Some(LuaToken::Punct(x)) if *x == p);
    let global = |i: usize| !(i > 0 && (punct(i - 1, ".") || punct(i - 1, ":") || name(i - 1, "function")));
    let mut out = Vec::new();
    for i in 0..t.len() {
        if !((name(i, "pcall") || name(i, "xpcall")) && global(i) && punct(i + 1, "(")) {
            continue;
        }
        if name(i + 2, "import") {
            out.push((t[i + 2].at, "pcall(import, …)"));
            continue;
        }
        if !name(i + 2, "function") {
            continue;
        }
        // Skip the parameter list, then walk the body to its `end`.
        let mut j = i + 3;
        while j < t.len() && !punct(j, ")") {
            j += 1;
        }
        j += 1;
        // Open blocks: true for a function literal, false for `if` / `do`.
        let mut open: Vec<bool> = vec![true];
        while j < t.len() && !open.is_empty() {
            if name(j, "function") {
                open.push(true);
            } else if name(j, "if") || name(j, "do") {
                open.push(false);
            } else if name(j, "end") {
                open.pop();
            } else if open.len() == open.iter().filter(|f| !**f).count() + 1
                && name(j, "import")
                && global(j)
                && (punct(j + 1, "(") || matches!(t.get(j + 1).map(|t| &t.kind), Some(LuaToken::ShortString(_))))
            {
                out.push((t[j].at, "pcall(function() import(…) end)"));
            }
            j += 1;
        }
    }
    out
}

/// A Lua token and the byte offset it starts at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: LuaToken,
    pub at: usize,
}

/// A Lua token, as far as these checks need one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LuaToken {
    Name(String),
    /// A `"…"` or `'…'` string, escapes decoded.
    ShortString(String),
    /// A `[[…]]` string.
    LongString,
    Number,
    Punct(&'static str),
}

/// The level of a long bracket opening at `b[i]` (`[[` is 0, `[==[` is 2), or `None`.
fn long_bracket_level(b: &[u8], i: usize) -> Option<usize> {
    if b.get(i) != Some(&b'[') {
        return None;
    }
    let mut j = i + 1;
    while b.get(j) == Some(&b'=') {
        j += 1;
    }
    (b.get(j) == Some(&b'[')).then_some(j - i - 1)
}

/// The index just past the long bracket close `]=*]` of `level` at or after `from`, or the end of
/// the input when it is unterminated (the compiler reports that; here it only has to stop).
fn long_bracket_end(b: &[u8], from: usize, level: usize) -> usize {
    let mut k = from;
    while k < b.len() {
        if b[k] == b']' {
            let mut j = k + 1;
            while b.get(j) == Some(&b'=') {
                j += 1;
            }
            if j - k - 1 == level && b.get(j) == Some(&b']') {
                return j + 1;
            }
        }
        k += 1;
    }
    b.len()
}

/// `source` as Lua 5.1 tokens: `--` line and `--[[ ]]` / `--[==[ ]==]` long comments are skipped,
/// `"…"` / `'…'` short strings decoded with their escapes, and `[[ ]]` / `[==[ ]==]` long strings
/// kept as one token.
pub(crate) fn lua_tokens(source: &str) -> Vec<Token> {
    const PUNCT: &[&str] = &[
        "...", "..", "==", "~=", "<=", ">=", "(", ")", "{", "}", "[", "]", ";", ":", ",", ".", "+",
        "-", "*", "/", "%", "^", "#", "<", ">", "=",
    ];
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let at = i;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'-' && b.get(i + 1) == Some(&b'-') {
            match long_bracket_level(b, i + 2) {
                Some(level) => i = long_bracket_end(b, i + 2 + level + 2, level),
                None => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                }
            }
        } else if let Some(level) = long_bracket_level(b, i) {
            i = long_bracket_end(b, i + level + 2, level);
            out.push(Token { kind: LuaToken::LongString, at });
        } else if c == b'"' || c == b'\'' {
            let (text, next) = short_string(b, i + 1, c);
            out.push(Token { kind: LuaToken::ShortString(text), at });
            i = next;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Token { kind: LuaToken::Name(source[at..i].to_string()), at });
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            // A numeral, including `0x1F`, `1e-5` and `.5`: letters, digits, `.`, and a sign right
            // after an exponent marker.
            i += 1;
            while i < b.len() {
                let d = b[i];
                let signed_exponent = (d == b'+' || d == b'-') && matches!(b[i - 1], b'e' | b'E');
                if d.is_ascii_alphanumeric() || d == b'.' || signed_exponent {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Token { kind: LuaToken::Number, at });
        } else if let Some(p) = PUNCT.iter().find(|p| b[i..].starts_with(p.as_bytes())) {
            out.push(Token { kind: LuaToken::Punct(p), at });
            i += p.len();
        } else {
            // A byte Lua itself would reject; the compiler reports it. Stepping over it keeps the
            // scan going without inventing a token. Non-ASCII bytes only occur inside strings and
            // comments in valid Lua 5.1, and those are consumed above.
            i += 1;
        }
    }
    out
}

/// Decode a short string whose body starts at `b[start]` and ends at the unescaped `quote`, and
/// return it with the index just past the closing quote. Lua 5.1's escapes: `\a \b \f \n \r \t
/// \v \\ \" \'`, a backslash-newline, and `\ddd` (up to three decimal digits).
fn short_string(b: &[u8], start: usize, quote: u8) -> (String, usize) {
    let mut bytes = Vec::new();
    let mut i = start;
    while i < b.len() && b[i] != quote && b[i] != b'\n' {
        if b[i] == b'\\' && i + 1 < b.len() {
            let e = b[i + 1];
            i += 2;
            match e {
                b'a' => bytes.push(0x07),
                b'b' => bytes.push(0x08),
                b'f' => bytes.push(0x0C),
                b'n' | b'\n' => bytes.push(b'\n'),
                b'r' => bytes.push(b'\r'),
                b't' => bytes.push(b'\t'),
                b'v' => bytes.push(0x0B),
                d if d.is_ascii_digit() => {
                    let mut value = u32::from(d - b'0');
                    let mut n = 1;
                    while n < 3 && i < b.len() && b[i].is_ascii_digit() {
                        value = value * 10 + u32::from(b[i] - b'0');
                        i += 1;
                        n += 1;
                    }
                    bytes.push(value as u8);
                }
                other => bytes.push(other),
            }
        } else {
            bytes.push(b[i]);
            i += 1;
        }
    }
    (String::from_utf8_lossy(&bytes).into_owned(), (i + 1).min(b.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(deps: Vec<mercs2_formats::script_meta::Dep>) -> Vec<(String, bool)> {
        deps.into_iter().map(|d| (d.name, d.inherit)).collect()
    }

    /// The compiled DEPS see every literal call form, inherit included, inside functions too; never
    /// a comment, a string, a field, a method, a computed name or `dynamic_import`.
    #[test]
    fn source_deps_reads_every_literal_call_form_and_nothing_else() {
        let src = r#"
local a = import("ess")
local b = import 'no_parens'
inherit("MrxTaskContract")
-- import("in_a_line_comment")
--[[ import("in_a_long_comment") ]]
local s = "import(\"in_a_string\")"
local c = t.import("a_field")
local d = t:import("a_method")
local e = dynamic_import("dynamic", function() end)
local f = import(name)
local g = import("a" .. suffix)
function later() return import("in_a_function") end
"#;
        assert_eq!(
            names(source_deps(src, "probe").expect("compiles")),
            vec![
                ("ess".to_string(), false),
                ("no_parens".to_string(), false),
                ("MrxTaskContract".to_string(), true),
                ("in_a_function".to_string(), false),
            ]
        );
    }

    #[test]
    fn source_deps_reports_a_compile_error() {
        assert!(source_deps("local = 1\n", "probe").is_err());
    }

    /// The literal by-name routes; a computed name, a field `sModuleName` read, a comment and a
    /// `RequiredAsset` of another asset type are not.
    #[test]
    fn by_name_routes_finds_the_literal_routes_only() {
        let src = r#"
tMissionData["AbcCon001"] = { sModuleName = "AbcCon001", sFactionId = "Pmc" }
self:CreateChild({ sModuleName = 'MrxTaskObjectiveKill' })
dynamic_import("lib_a", function(m) end)
Sys.RequiredAsset("lib_b", "script", cb)
Sys.RequiredAsset("tex", "texture", cb)
dynamic_import(sName, cb)
local x = t.sModuleName == "not_a_route"
-- dynamic_import("in_a_comment")
--[==[ dynamic_import("in_a_level_2_comment") ]]==]
local l = [[ dynamic_import("in_a_long_string") ]]
local s = "dynamic_import(\"in_a_string\")"
x.dynamic_import("a_field")
dynamic_import("esc\097ped", cb)
"#;
        assert_eq!(by_name_routes(src), vec!["AbcCon001", "MrxTaskObjectiveKill", "lib_a", "lib_b", "escaped"]);
    }

    /// Ess's own forms, `pcall(import, …)` and an import nested deeper in the body are found; an
    /// import outside the pcall, or in a function nested in the pcall'd one, is not.
    #[test]
    fn pcall_imports_finds_imports_the_pcall_cannot_protect() {
        let src = r#"
pcall(function() import("MrxVoSequence") end)   -- optional VO
local ok = pcall(import, "MrxMusic")
xpcall(function()
  if x then import "Deep" end
  local later = function() import("Nested") end
end, handler)
import("Outside")
pcall(f, import)
"#;
        let found: Vec<(&str, &str)> =
            pcall_imports(src).into_iter().map(|(at, form)| (&src[at..at + 6], form)).collect();
        assert_eq!(
            found,
            vec![
                ("import", "pcall(function() import(…) end)"),
                ("import", "pcall(import, …)"),
                ("import", "pcall(function() import(…) end)"),
            ]
        );
        let lines: Vec<usize> = pcall_imports(src).iter().map(|(at, _)| src[..*at].matches('\n').count() + 1).collect();
        assert_eq!(lines, vec![2, 3, 5]);
    }

    /// Numbers and operators do not derail the lexer.
    #[test]
    fn the_lexer_survives_numbers_and_operators() {
        let src = "x = 1e-5 + 0x1F .. dynamic_import('after_concat') ... y = .5; z = a..dynamic_import(\"b\")\n";
        assert_eq!(by_name_routes(src), vec!["after_concat", "b"]);
    }

    fn graph(scripts: &[(&str, &[&str])]) -> DepsGraph {
        let mut g = DepsGraph::default();
        for (n, deps) in scripts {
            g.names.insert(pandemic_hash_m2(n), n.to_string());
            g.deps.insert(pandemic_hash_m2(n), deps.iter().map(|d| pandemic_hash_m2(d)).collect());
        }
        g
    }

    /// The cycle gate's library, as the game ran it live (`qm-cycle-gate`, 2026-11-09): `b`'s body
    /// first, with `a` only an env, then `a`'s, then the Inits in the same order, then the root.
    #[test]
    fn the_loader_model_runs_a_cycle_the_way_the_game_did() {
        let g = graph(&[("qm_cyc_root", &["qm_cyc_a"]), ("qm_cyc_a", &["qm_cyc_b"]), ("qm_cyc_b", &["qm_cyc_a"])]);
        let passes = load_passes(&g, &[pandemic_hash_m2("qm_cyc_root")], &BTreeSet::new()).expect("model");
        assert_eq!(passes.len(), 1, "{passes:?}");
        let events: Vec<(Stage, String)> = passes[0].events.iter().map(|&(s, h)| (s, g.name(h))).collect();
        let want: Vec<(Stage, String)> = [
            (Stage::Env, "qm_cyc_a"),
            (Stage::Env, "qm_cyc_b"),
            (Stage::Body, "qm_cyc_b"),
            (Stage::Body, "qm_cyc_a"),
            (Stage::Init, "qm_cyc_b"),
            (Stage::Init, "qm_cyc_a"),
            (Stage::Env, "qm_cyc_root"),
            (Stage::Body, "qm_cyc_root"),
            (Stage::Init, "qm_cyc_root"),
        ]
        .into_iter()
        .map(|(s, n)| (s, n.to_string()))
        .collect();
        assert_eq!(events, want);
        assert_eq!(passes[0].early, vec![(pandemic_hash_m2("qm_cyc_b"), pandemic_hash_m2("qm_cyc_a"))]);
    }

    /// A script the master pass leaves unloaded loads in a pass of its own, from what the master
    /// pass left: its dependency already loaded is not loaded again.
    #[test]
    fn what_the_master_pass_leaves_loads_on_its_own_after_it() {
        let g = graph(&[("vz", &["util"]), ("util", &[]), ("contract", &["util", "job"]), ("job", &[])]);
        let passes = load_passes(&g, &[pandemic_hash_m2("vz")], &BTreeSet::new()).expect("model");
        let roots: Vec<(String, bool)> = passes.iter().map(|p| (g.name(p.root), p.by_name)).collect();
        assert_eq!(roots, vec![("vz".to_string(), false), ("contract".to_string(), true)]);
        let bodies: Vec<String> = passes[1].events.iter().filter(|e| e.0 == Stage::Body).map(|e| g.name(e.1)).collect();
        assert_eq!(bodies, vec!["job", "contract"]);
    }

    fn m0314(after: &[(&str, &[&str])], source: &str) -> Vec<ImportFinding> {
        // Retail's cycle a → c → b → a, under the master p.
        let before = graph(&[("p", &["a"]), ("a", &["c"]), ("c", &["b"]), ("b", &["a"])]);
        let after = graph(after);
        let target = source.split('|').next().expect("target");
        let src = source.split('|').nth(1).expect("source");
        let contributors = BTreeMap::from([(pandemic_hash_m2(target), vec!["s".to_string()])]);
        let sources = [Source { shipment: "s", what: format!("patch_lua append to {target}"), script: target, source: src }];
        check(&LinkChecks { before: &before, after: &after, roots: &["p"], sources: &sources, import_only: &[], contributors: &contributors })
            .expect("checks")
    }

    /// An edge to a module whose body already ran first changes nothing (the live run B case).
    #[test]
    fn an_added_import_of_a_module_already_run_is_no_m0314() {
        let found = m0314(&[("p", &["a"]), ("a", &["c"]), ("c", &["b", "a"]), ("b", &["a"])], "c|import('a')");
        assert_eq!(found, vec![]);
    }

    /// The graph after `a` of retail's cycle imports `b`, whose body runs after `a`'s; the order stays
    /// the same.
    const A_IMPORTS_B_EARLY: &[(&str, &[&str])] = &[("p", &["a"]), ("a", &["c", "b"]), ("c", &["b"]), ("b", &["a"])];

    /// Shipment code that uses, at its top level, a module whose body has not run yet is M0314, an
    /// error, naming the line, directly or through a local it copied the module into.
    #[test]
    fn a_top_level_use_of_a_module_whose_body_has_not_run_is_m0314() {
        for (src, line) in [
            ("import('b')\nlocal x = b.value\n", 2),
            ("import('b')\n\nb:Start()\n", 3),
            ("import('b')\nlocal B = b\nif B then\n  B.n = 1\nend\n", 4),
            ("import('b')\nlocal n = #b\n", 2),
        ] {
            let found = m0314(A_IMPORTS_B_EARLY, &format!("a|{src}"));
            let got: Vec<(ImportRule, &str)> = found.iter().map(|f| (f.rule, f.module.as_str())).collect();
            assert_eq!(got, vec![(ImportRule::LoadOrder, "a")], "{src}: {found:?}");
            assert!(!found[0].rule.is_warning());
            assert!(
                found[0].message.starts_with(&format!(
                    "s: its patch_lua append to a uses b at its top level (line {line}), and a imports b, whose \
                     body the engine runs after a's (when p loads)"
                )),
                "{}",
                found[0].message
            );
            assert_eq!(found[0].shipments, vec!["s".to_string()]);
        }
    }

    /// The same import is no finding when the code uses the module only inside functions, or only
    /// keeps the table or passes it on at its top level: the table fills when the module's body runs.
    #[test]
    fn an_import_of_a_module_whose_body_has_not_run_used_later_is_no_m0314() {
        for src in [
            "import('b')\n",
            "import('b')\nfunction Init()\n  return b.value\nend\n",
            "import('b')\nlocal B = b\nfunction Get() return B.value end\n",
            "import('b')\nlocal t = { b }\n",
            "import('b')\nlocal x = c.value\n",
            "local f = function() import('b') end\nlocal x = B.value\n",
        ] {
            assert_eq!(m0314(A_IMPORTS_B_EARLY, &format!("a|{src}")), vec![], "{src}");
        }
    }

    /// An added import that reorders retail's own loading is an M0314 warning.
    #[test]
    fn an_added_import_that_reorders_retail_is_an_m0314_warning() {
        let found = m0314(&[("p", &["a"]), ("a", &["b", "c"]), ("c", &["b"]), ("b", &["a"])], "a|import('b')");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].rule, found[0].rule.code()), (ImportRule::Reorder, "M0314"));
        assert!(found[0].rule.is_warning());
        assert!(found[0].message.contains("when p loads: a's env now runs where c's env ran"), "{}", found[0].message);
    }

    /// An import-only module is reached through a DEPS edge or a by-name route from something that
    /// runs, transitively; one reached only from another unreached module is M0313, and an
    /// unresolved name is M0209.
    #[test]
    fn an_import_only_module_must_be_reached_from_code_that_runs() {
        let before = graph(&[("wifpmcinterior", &[])]);
        let after = graph(&[
            ("wifpmcinterior", &["util_a"]),
            ("util_a", &["util_b"]),
            ("util_b", &[]),
            ("contract", &[]),
            ("orphan", &["orphan_child"]),
            ("orphan_child", &[]),
        ]);
        let contributors = BTreeMap::new();
        let sources = [
            Source { shipment: "s", what: "patch_lua append to wifpmcinterior".into(), script: "wifpmcinterior", source: "import('util_a') x = { sModuleName = 'contract' }" },
            Source { shipment: "s", what: "add_script util_a".into(), script: "util_a", source: "import('util_b')" },
            Source { shipment: "s", what: "add_script orphan".into(), script: "orphan", source: "import('orphan_child') import('missing')" },
        ];
        let import_only = [("util_a", "s"), ("util_b", "s"), ("contract", "s"), ("orphan", "s"), ("orphan_child", "s")];
        let found = check(&LinkChecks {
            before: &before,
            after: &after,
            roots: &["wifpmcinterior"],
            sources: &sources,
            import_only: &import_only,
            contributors: &contributors,
        })
            .expect("checks");
        let got: Vec<(&str, &str)> = found.iter().map(|f| (f.rule.code(), f.module.as_str())).collect();
        assert_eq!(got, vec![("M0209", "missing"), ("M0313", "orphan"), ("M0313", "orphan_child")]);
    }
}
