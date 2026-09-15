//! Markdown docs-page generator for this crate's engine binding surface.
//!
//! # What it produces
//!
//! One `.md` page per engine Lua global (`Pg.md`, `Player.md`, …) plus an `index.md`, ready for
//! GitHub to render or `mdbook` to wrap. A Shipment author browses to
//! `docs/modding/bindings/Pg.md` on the repo and immediately sees the retail table VA, how many
//! shipped script sites call each binding, and links to the reverse-engineering code maps that
//! describe the underlying system.
//!
//! Same data source as [`crate::stubs`]: the [`NAMESPACES`](crate::bindings::NAMESPACES) table.
//! Two views of the same surface — IDE autocomplete for the author's editor, browsable
//! reference for the author's search bar. Both regenerate from one `emmylua-gen` invocation.
//!
//! # What's on each page (for now)
//!
//! - Retail table VA + binding count + which namespace(s) share the global.
//! - A binding table with corpus-call count per row.
//! - Cross-links back to `docs/reverse_engineer/scripting_host_binding_code_map.md` (the master
//!   index) and to the paired EmmyLua stub at `output/emmylua/mercs2/<Global>.lua`.
//!
//! # What's NOT on each page (yet)
//!
//! - Per-binding parameter names / types. Same limitation as the stubs — not structurally
//!   recovered yet. When they land, this module's `render_binding_row` grows a column.
//! - Auto-cross-linking each binding to prose in `docs/reverse_engineer/*.md`. That needs an
//!   ID→doc-chunk join over the ~120 named cfuncs from the code map — a follow-up.

use crate::bindings::{NamespaceMeta, Required, NAMESPACES};
use crate::stubs::GenReport;
use std::collections::BTreeMap;

/// One generated `.md` doc page — the front-matter, retail-VA header, and one row per binding
/// for every namespace that shares this Lua global. Written verbatim to
/// `<docs_out>/<global>.md`.
pub struct DocFile {
    pub global: &'static str,
    pub source: String,
}

/// Produce one [`DocFile`] per Lua global plus an aggregated report. Deterministic (same
/// invariants as [`crate::stubs::generate`]).
pub fn generate_docs() -> (Vec<DocFile>, GenReport) {
    generate_docs_from(NAMESPACES)
}

/// Same as [`generate_docs`] but driven from any [`NamespaceMeta`] slice — used by tests.
pub fn generate_docs_from(namespaces: &[NamespaceMeta]) -> (Vec<DocFile>, GenReport) {
    // Same grouping as `stubs::generate_from`: one page per Lua global, merging namespaces that
    // share one. Kept in step by construction — both generators consume the same slice.
    let mut by_global: BTreeMap<&'static str, Vec<&NamespaceMeta>> = BTreeMap::new();
    for m in namespaces {
        by_global.entry(m.global).or_default().push(m);
    }

    let mut files = Vec::with_capacity(by_global.len());
    let mut bindings = 0usize;
    let mut ns_count = 0usize;
    for (global, metas) in &by_global {
        let (source, count) = render_page(global, metas);
        bindings += count;
        ns_count += metas.len();
        files.push(DocFile {
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

/// The `index.md` page — one line per Lua global, sorted alphabetically, with binding count.
///
/// Kept as a separate function (rather than returning it in [`generate_docs`]) so a consumer
/// that wants only the per-global pages can skip building the index, and a consumer that wants
/// only the index (a widget on a modding-portal front page) can build just this one.
pub fn generate_index() -> String {
    generate_index_from(NAMESPACES)
}

/// Same as [`generate_index`] driven from an arbitrary [`NamespaceMeta`] slice.
pub fn generate_index_from(namespaces: &[NamespaceMeta]) -> String {
    let mut by_global: BTreeMap<&'static str, Vec<&NamespaceMeta>> = BTreeMap::new();
    for m in namespaces {
        by_global.entry(m.global).or_default().push(m);
    }
    let mut total_bindings = 0usize;
    let mut rows: Vec<(&'static str, usize, Vec<&'static str>)> = Vec::new();
    for (global, metas) in &by_global {
        let mut names: BTreeMap<&'static str, ()> = BTreeMap::new();
        for m in metas {
            for r in m.required {
                names.insert(r.name, ());
            }
        }
        total_bindings += names.len();
        let ns_names: Vec<&'static str> = metas.iter().map(|m| m.namespace).collect();
        rows.push((global, names.len(), ns_names));
    }
    // Sort by binding-count desc so the most heavily used globals surface first, then
    // alphabetically as a tie-break — a modder scanning for "which Lua global do I want" reads
    // top-down.
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let mut s = String::new();
    s.push_str("# Mercenaries 2 — engine Lua binding surface\n\n");
    s.push_str(
        "One page per Lua global. **GENERATED — do not edit by hand.** Regenerate with \
         `cargo run -p mercs2_script --bin emmylua-gen`.\n\n",
    );
    s.push_str(&format!(
        "**{} Lua global{}** covering **{} namespace{}** and **{} unique binding{}** — the \
         entire installed surface as this crate wires it. Source: \
         `mercs2_script::bindings::NAMESPACES`; the drift-alarm test in `mercs2_script::tests::\
         coverage_report` pins the count to `install_all`'s output.\n\n",
        by_global.len(),
        if by_global.len() == 1 { "" } else { "s" },
        namespaces.len(),
        if namespaces.len() == 1 { "" } else { "s" },
        total_bindings,
        if total_bindings == 1 { "" } else { "s" }
    ));

    s.push_str("For IDE autocomplete on the same surface, load the paired EmmyLua stubs — see \
                `output/emmylua/mercs2/` (regenerated by the same command) and the accompanying \
                `.luarc.example.json`.\n\n");

    s.push_str("See also:\n\n");
    s.push_str(
        "- [`docs/reverse_engineer/scripting_host_binding_code_map.md`](../../reverse_engineer/\
         scripting_host_binding_code_map.md) — the master index of every `luaL_Reg` table with \
         its retail VA + representative decoded cfuncs.\n",
    );
    s.push_str(
        "- [`docs/lua_engine_bindings_audit_deep_dive.md`](../../lua_engine_bindings_audit_deep_\
         dive.md) — the counts + call-site frequencies this table is built on.\n",
    );
    s.push_str(
        "- [`docs/modding/lua_engine_seam_hardening.md`](../lua_engine_seam_hardening.md) — the \
         design for the whole seam-hardening effort, of which this is one piece.\n\n",
    );

    s.push_str("## Globals\n\n");
    s.push_str("| Lua global | Bindings | Namespaces |\n");
    s.push_str("|------------|---------:|------------|\n");
    for (global, count, ns_names) in &rows {
        let filename = global.replace('.', "_");
        let joined = ns_names.join(", ");
        s.push_str(&format!(
            "| [`{global}`]({filename}.md) | {count} | `{joined}` |\n"
        ));
    }
    s.push('\n');
    s
}

/// Render one `.md` page for a single Lua global. Returns `(source, bindings_rendered)`.
fn render_page(global: &str, metas: &[&NamespaceMeta]) -> (String, usize) {
    let mut s = String::new();

    // The heading names the global exactly as a Lua caller writes it — dotted globals
    // (`Graphics.FuelTrail`, `Human.Inventory`) keep the dot; downstream link-building replaces
    // the dot with underscore for the filename, but the visible name stays intact.
    s.push_str(&format!("# `{global}` — engine binding surface\n\n"));

    // A one-line status block per source namespace: retail table VA + surface size. Two
    // namespaces sharing a global (`Camera` = `camera` + `camera_fx`) show two rows.
    for m in metas {
        s.push_str(&format!(
            "- **`{}`** — retail table VA `0x{:08X}`, {} required cfunc{}.\n",
            m.namespace,
            m.table_va,
            m.required.len(),
            if m.required.len() == 1 { "" } else { "s" }
        ));
    }
    s.push('\n');

    s.push_str(
        "**GENERATED — do not edit by hand.** Regenerate with \
         `cargo run -p mercs2_script --bin emmylua-gen`.\n\n",
    );

    s.push_str(
        "For IDE autocomplete on the same surface, load the paired EmmyLua stub — see \
         `output/emmylua/mercs2/",
    );
    s.push_str(global);
    s.push_str(".lua` (regenerated by the same command).\n\n");

    s.push_str("See also:\n\n");
    s.push_str(
        "- [Master binding-surface index](../../reverse_engineer/scripting_host_binding_code_map.md) \
         — every `luaL_Reg` table, its retail VA, and the representative cfuncs whose bodies are decoded.\n",
    );
    s.push_str(
        "- [Modding field guide](../field_guide.md) — the 17 collected traps a Shipment author \
         should read before touching any of these bindings.\n\n",
    );

    // Dedupe by name across every namespace that shares this global (same semantics as the stub
    // renderer). Sort alphabetically for a stable diff.
    let mut all: BTreeMap<&'static str, &'static Required> = BTreeMap::new();
    for m in metas {
        for r in m.required {
            all.entry(r.name).or_insert(r);
        }
    }

    s.push_str("## Bindings\n\n");
    s.push_str(&format!(
        "{count} binding{s} on this global. `Corpus` = distinct references to the binding \
         across the decompiled shipped Lua (both `docs/mercs2-luacd/` and \
         `docs/mercs2-dlc-luacd/src/`); `0` means no shipped script calls it, which is a hint \
         that a novel one may not have an established idiom to follow.\n\n",
        count = all.len(),
        s = if all.len() == 1 { "" } else { "s" }
    ));
    s.push_str("| Binding | Corpus calls |\n");
    s.push_str("|---------|-------------:|\n");
    for (name, req) in &all {
        s.push_str(&format!("| `{global}.{name}` | {c} |\n", c = req.corpus_calls));
    }
    s.push('\n');

    (s, all.len())
}

/// A short human-readable summary — mirrors [`crate::stubs::summarize`] so the CLI can print
/// one line per artifact set.
pub fn summarize(report: &GenReport) -> String {
    format!(
        "wrote {files} docs page(s): {globals} Lua global(s) covering {namespaces} namespace(s), \
         {bindings} unique binding(s)",
        files = report.files,
        globals = report.globals,
        namespaces = report.namespaces,
        bindings = report.bindings,
    )
}

/// Convert a Lua-global name into the filename we serve it at — `Graphics.FuelTrail` becomes
/// `Graphics_FuelTrail.md` so dots don't confuse GitHub's file navigation. The stubs generator
/// deliberately keeps the dot in `<Global>.lua` because lua-language-server wants the exact
/// runtime path; the docs pages have no runtime constraint, so we pick the less-ambiguous form.
pub fn filename_for(global: &str) -> String {
    format!("{}.md", global.replace('.', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_covers_every_namespace() {
        let (files, report) = generate_docs();
        assert_eq!(report.namespaces, NAMESPACES.len());
        assert!(!files.is_empty());
        // Same merge invariant as the stubs generator: one Lua global (`Camera`) is shared by
        // two namespaces (`camera` + `camera_fx`), so `files < namespaces`.
        assert!(report.globals < report.namespaces);
    }

    #[test]
    fn every_page_carries_the_generated_disclaimer() {
        let (files, _) = generate_docs();
        for f in &files {
            assert!(
                f.source.starts_with(&format!("# `{}` — engine binding surface\n\n", f.global)),
                "{}: heading missing or wrong",
                f.global
            );
            assert!(
                f.source.contains("**GENERATED — do not edit by hand.**"),
                "{}: missing GENERATED disclaimer",
                f.global
            );
        }
    }

    #[test]
    fn pg_page_carries_the_canonical_bindings() {
        // `Pg.Spawn` and `Pg.GetGuidByName` are the two entry points every custom mission ends
        // up calling. If either disappears from the page, the docs have silently regressed.
        let (files, _) = generate_docs();
        let pg = files
            .iter()
            .find(|f| f.global == "Pg")
            .expect("Pg page must exist");
        assert!(
            pg.source.contains("`Pg.Spawn`"),
            "Pg.Spawn missing from Pg.md"
        );
        assert!(
            pg.source.contains("`Pg.GetGuidByName`"),
            "Pg.GetGuidByName missing from Pg.md"
        );
    }

    #[test]
    fn hierarchical_globals_serve_at_a_dot_free_filename() {
        // `Graphics.FuelTrail` becomes `Graphics_FuelTrail.md` so GitHub does not treat the
        // literal dot as an extension boundary. The index rendered above uses this same rule,
        // so both must go through `filename_for`.
        assert_eq!(filename_for("Pg"), "Pg.md");
        assert_eq!(
            filename_for("Graphics.FuelTrail"),
            "Graphics_FuelTrail.md"
        );
        assert_eq!(filename_for("Human.Inventory"), "Human_Inventory.md");
    }

    #[test]
    fn index_lists_every_global_with_its_binding_count() {
        let idx = generate_index();
        // Every Lua global's row must appear, keyed by the same filename `filename_for` produces
        // — a link this page renders that points at nothing is exactly the bug the test catches.
        let mut seen: BTreeMap<&'static str, bool> = BTreeMap::new();
        for m in NAMESPACES {
            seen.entry(m.global).or_insert(false);
        }
        for global in seen.keys() {
            let filename = global.replace('.', "_");
            assert!(
                idx.contains(&format!("[`{global}`]({filename}.md)")),
                "index missing link for {global} -> {filename}.md"
            );
        }
        // The summary block names the axes.
        assert!(idx.contains("Lua global"));
        assert!(idx.contains("unique binding"));
    }

    #[test]
    fn output_is_deterministic() {
        let (a, _) = generate_docs();
        let (b, _) = generate_docs();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.global, y.global);
            assert_eq!(x.source, y.source, "{}", x.global);
        }
        assert_eq!(generate_index(), generate_index());
    }
}
