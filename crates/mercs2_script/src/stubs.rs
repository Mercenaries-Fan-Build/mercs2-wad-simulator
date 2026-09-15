//! EmmyLua / lua-language-server stub generator for this crate's engine binding surface.
//!
//! # What it produces
//!
//! One `.lua` stub file per engine Lua global (`Pg.lua`, `Player.lua`, `Object.lua`, …), each
//! annotated with `---@meta` so `lua-language-server` treats it as a type declaration only. A
//! Shipment author drops these into their editor's workspace (via a `.luarc.json` `workspace
//! .library` entry, or by placing the folder alongside their `src/`) and immediately gets
//! autocomplete for every one of the engine's **1086 cfuncs across 35+ namespaces**.
//!
//! Lives inside `mercs2_script` because the data source *is* this crate — the [`bindings`]
//! module's [`NAMESPACES`](crate::bindings::NAMESPACES) table. Keeping the generator adjacent
//! means the drift-alarm test that pins `NAMESPACES` to `install_all`'s output is an in-crate
//! invariant rather than a cross-crate one.
//!
//! # What it does NOT produce (yet)
//!
//! Per-argument type annotations. This crate records binding **names** and their **corpus call
//! counts**, not their argument types — Lua is dynamically typed and the retail cfunc bodies use
//! a uniform `luaL_check*` reader ABI, so structural arg-type recovery is a separate RE task.
//! Every function is declared `function Ns.Foo(...) end`, so autocomplete shows the function
//! name and the doc comment carries the corpus-call count + retail table VA.
//!
//! # Regeneration
//!
//! `cargo run -p mercs2_script --bin emmylua-gen [--out <dir>]`
//!
//! # The plan doc
//!
//! See `docs/modding/lua_engine_seam_hardening.md#community-accelerators` for the design context.

use crate::bindings::{NamespaceMeta, Required, NAMESPACES};
use std::collections::BTreeMap;

/// One generated `.lua` stub file — the header + a class declaration + one function stub per
/// binding, produced from every namespace that shares this Lua global.
pub struct StubFile {
    /// The Lua global the stub declares (`Pg`, `Player`, `Object`, …). Two engine namespaces
    /// may share a global — the file merges them under one `---@class`.
    pub global: &'static str,
    /// The rendered stub bytes; write verbatim to `<out>/<global>.lua`.
    pub source: String,
}

/// A run's aggregated stats — every consumer wants at least the file count and the total
/// bindings written.
pub struct GenReport {
    pub files: usize,
    pub globals: usize,
    pub namespaces: usize,
    pub bindings: usize,
}

/// Produce one [`StubFile`] per Lua global, plus an aggregated report. Deterministic: two runs
/// against the same [`NAMESPACES`](crate::bindings::NAMESPACES) snapshot produce byte-identical
/// output (globals sorted alphabetically, bindings within each global sorted by name).
///
/// Consumers wanting the whole surface in a single file can concatenate the [`StubFile::source`]
/// strings — each begins with its own `---@meta` header, and lua-language-server tolerates
/// multiple `---@meta` markers.
pub fn generate() -> (Vec<StubFile>, GenReport) {
    generate_from(NAMESPACES)
}

/// Same as [`generate`] but driven from any slice of [`NamespaceMeta`] — used by tests, and by
/// tooling that wants to filter down to a subset (a single namespace, only the backed ones, …).
pub fn generate_from(namespaces: &[NamespaceMeta]) -> (Vec<StubFile>, GenReport) {
    // Group namespaces by Lua global. One of the 36 namespaces shares a global with another
    // (`camera` + `camera_fx` both declare `GLOBAL = "Camera"`), and the stub file has to
    // declare one class per global, not per NamespaceMeta.
    let mut by_global: BTreeMap<&'static str, Vec<&NamespaceMeta>> = BTreeMap::new();
    for m in namespaces {
        by_global.entry(m.global).or_default().push(m);
    }

    let mut files = Vec::with_capacity(by_global.len());
    let mut bindings = 0usize;
    let mut ns_count = 0usize;
    for (global, metas) in &by_global {
        let (source, count) = render_file(global, metas);
        bindings += count;
        ns_count += metas.len();
        files.push(StubFile {
            global,
            source,
        });
    }
    let report = GenReport {
        files: files.len(),
        globals: by_global.len(),
        namespaces: ns_count,
        bindings,
    };
    (files, report)
}

/// Render one `.lua` file for a single Lua global. Returns `(source, bindings_written)` so the
/// caller can tally without re-scanning the string.
fn render_file(global: &str, metas: &[&NamespaceMeta]) -> (String, usize) {
    let mut s = String::new();

    // Header: mark the file as a pure declaration ("meta" per lua-language-server), and tell the
    // reader where the truth lives.
    s.push_str("---@meta\n");
    s.push_str("--\n");
    s.push_str(&format!("-- Mercenaries 2 engine bindings — `{global}`\n"));
    s.push_str("--\n");
    s.push_str(
        "-- GENERATED FILE — do not edit by hand.\n\
         -- Regenerate with:  cargo run -p mercs2_script --bin emmylua-gen\n",
    );
    s.push_str("--\n");
    s.push_str(&format!(
        "-- Source: `mercs2_script::bindings::NAMESPACES` ({} namespace{} {} this Lua global).\n",
        metas.len(),
        if metas.len() == 1 { "" } else { "s" },
        if metas.len() == 1 { "under" } else { "share" }
    ));
    s.push_str(
        "-- Docs: `docs/modding/lua_engine_seam_hardening.md#community-accelerators`.\n",
    );
    s.push_str("--\n");
    s.push_str(
        "-- Function signatures are `(...)` because Lua is dynamically typed and per-argument\n\
         -- types are not yet recovered from the retail cfunc bodies. Autocomplete works on the\n\
         -- function *name*; hover shows the retail table VA + how many shipped call sites use it.\n\n",
    );

    // Per-source-namespace summary — retail table VA + surface size — so a modder can jump into
    // the Ghidra decomp knowing exactly which luaL_Reg table this comes from.
    for m in metas {
        s.push_str(&format!(
            "-- `{}` — retail table VA `0x{:08X}`, {} required cfunc{}.\n",
            m.namespace,
            m.table_va,
            m.required.len(),
            if m.required.len() == 1 { "" } else { "s" }
        ));
    }
    s.push('\n');

    // Class declaration for the global. lua-language-server picks this up so `Pg.` shows the
    // full member list on completion.
    s.push_str(&format!("---@class {global}\n"));
    // Hierarchical globals (`Graphics.FuelTrail`, `Human.Inventory`) need each prefix declared
    // before the leaf assignment — otherwise `A.B = A.B or {}` errors on the read of `A.B` when
    // `A` is nil. Split on `.` and emit a guard per segment; a single-segment global reduces to
    // just the leaf.
    let mut path = String::new();
    for (i, segment) in global.split('.').enumerate() {
        if i > 0 {
            path.push('.');
        }
        path.push_str(segment);
        s.push_str(&format!("{path} = {path} or {{}}\n"));
    }
    s.push('\n');

    // Every required cfunc across every namespace under this global — deduped by name (two
    // namespaces sharing a global still get the same table on the Lua side, so a name defined in
    // both is only rendered once) and sorted alphabetically for a stable diff.
    let mut all: BTreeMap<&'static str, &'static Required> = BTreeMap::new();
    for m in metas {
        for r in m.required {
            // Keep the first entry: two namespaces sharing a global and a name is not a shape
            // the retail surface presents — the drift-alarm test would fire — but if it ever
            // arises the earlier ns wins, which matches `install_all`'s registration order.
            all.entry(r.name).or_insert(r);
        }
    }

    for (name, req) in &all {
        render_function(&mut s, global, name, req);
    }

    (s, all.len())
}

/// Render one function stub — an EmmyLua doc block plus a bare `function Global.Name(...) end`.
///
/// The doc block cites the retail corpus call count so an author picking between two candidate
/// bindings knows which one shipped scripts actually use (`Pg.Spawn` is called from many sites;
/// `Pg.SpawnDelayed` from a handful). The parameter is left un-annotated (`...`) rather than
/// typed — misannotating an arg is worse than not annotating it, and structural arg-type
/// recovery is a separate RE task (see `docs/lua_call_sites_from_scripts.md`).
fn render_function(s: &mut String, global: &str, name: &str, req: &Required) {
    s.push_str("--- ");
    if req.corpus_calls == 0 {
        s.push_str("Not called from any decompiled shipped script.");
    } else {
        s.push_str(&format!(
            "Called from {} shipped script site{}.",
            req.corpus_calls,
            if req.corpus_calls == 1 { "" } else { "s" }
        ));
    }
    s.push('\n');
    s.push_str("---@vararg any\n");
    s.push_str("---@return any ...\n");
    s.push_str(&format!("function {global}.{name}(...) end\n\n"));
}

/// A short human-readable summary of the run, suitable for a CLI's stdout.
pub fn summarize(report: &GenReport) -> String {
    format!(
        "wrote {files} file(s): {globals} Lua global(s) covering {namespaces} namespace(s), \
         {bindings} unique binding(s)",
        files = report.files,
        globals = report.globals,
        namespaces = report.namespaces,
        bindings = report.bindings,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_covers_every_namespace() {
        // Every namespace in the offline metadata table must show up in exactly one output
        // file — a rule that lost a namespace would silently drop bindings a modder relies on.
        let (files, report) = generate();
        assert_eq!(
            report.namespaces,
            NAMESPACES.len(),
            "namespace coverage regressed"
        );
        assert!(!files.is_empty());
        // One known merge: `camera` + `camera_fx` both declare `GLOBAL = "Camera"`, so they
        // collapse to one stub file. Every other namespace has a distinct global (including
        // `pg` = `Pg`, `pg_world` = `Junk`, `sys` = `Sys`, `sys_module` = `_SYS`), so
        // `files < namespaces` is the shape.
        assert!(
            report.globals < report.namespaces,
            "at least one Lua global must merge two namespaces (Camera = camera + camera_fx)"
        );
    }

    #[test]
    fn every_file_declares_the_class_and_starts_with_meta() {
        let (files, _) = generate();
        for f in &files {
            assert!(
                f.source.starts_with("---@meta\n"),
                "{}: missing ---@meta header",
                f.global
            );
            assert!(
                f.source
                    .contains(&format!("---@class {}\n", f.global)),
                "{}: missing ---@class",
                f.global
            );
            assert!(
                f.source
                    .contains(&format!("\n{} = {} or {{}}\n", f.global, f.global)),
                "{}: missing global table init",
                f.global
            );
        }
    }

    #[test]
    fn a_known_binding_shows_up_verbatim() {
        // `Pg.Spawn` is the canonical entry point — the shipped `mrxtaskcontract` chain and
        // every custom mission depends on it. A stub file for `Pg` that omitted it would fail
        // silently against every editor, so pin it explicitly here.
        let (files, _) = generate();
        let pg = files
            .iter()
            .find(|f| f.global == "Pg")
            .expect("Pg stub file must exist");
        assert!(
            pg.source.contains("function Pg.Spawn(...) end"),
            "Pg.Spawn missing from Pg stubs"
        );
        assert!(
            pg.source.contains("function Pg.GetGuidByName(...) end"),
            "Pg.GetGuidByName missing from Pg stubs"
        );
    }

    #[test]
    fn output_is_deterministic() {
        let (a, _) = generate();
        let (b, _) = generate();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.global, y.global);
            assert_eq!(x.source, y.source, "{}", x.global);
        }
    }

    #[test]
    fn hierarchical_globals_emit_a_guard_per_prefix() {
        // `Graphics.FuelTrail` and `Human.Inventory` are dotted globals: the raw assignment
        // `Graphics.FuelTrail = Graphics.FuelTrail or {}` errors on the read of `Graphics.FuelTrail`
        // when `Graphics` itself is nil. Each prefix must be guarded first.
        let (files, _) = generate();
        for global in ["Graphics.FuelTrail", "Human.Inventory"] {
            let f = files
                .iter()
                .find(|f| f.global == global)
                .unwrap_or_else(|| panic!("{global} stub file must exist"));
            let (parent, _) = global.split_once('.').unwrap();
            assert!(
                f.source.contains(&format!("\n{parent} = {parent} or {{}}\n")),
                "{global}: missing prefix guard for {parent}"
            );
            assert!(
                f.source
                    .contains(&format!("{global} = {global} or {{}}\n")),
                "{global}: missing leaf assignment"
            );
            // The prefix guard must precede the leaf assignment in the file, otherwise reading
            // the leaf lookup on the RHS still errors before the parent exists.
            let parent_at = f
                .source
                .find(&format!("\n{parent} = {parent} or"))
                .unwrap();
            let leaf_at = f.source.find(&format!("{global} = {global} or")).unwrap();
            assert!(
                parent_at < leaf_at,
                "{global}: parent guard must precede leaf assignment"
            );
        }
    }

    #[test]
    fn summarize_names_every_axis() {
        let report = GenReport {
            files: 33,
            globals: 33,
            namespaces: 36,
            bindings: 1092,
        };
        let s = summarize(&report);
        assert!(s.contains("33"), "{s}");
        assert!(s.contains("36"), "{s}");
        assert!(s.contains("1092"), "{s}");
    }
}
