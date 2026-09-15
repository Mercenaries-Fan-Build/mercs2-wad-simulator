//! `emmylua-gen` — write both artifacts for the Mercenaries 2 Lua binding surface.
//!
//! - **EmmyLua stubs** (`<stubs-out>/<Global>.lua`) — lua-language-server input for IDE
//!   autocomplete. Default: `tools/wad_simulator/output/emmylua/mercs2/`. Override with
//!   `--stubs-out <dir>` (or `--out <dir>`, legacy alias).
//! - **Markdown docs pages** (`<docs-out>/<Global>.md` + `index.md`) — one browsable reference
//!   page per Lua global, rendered by GitHub. Default: `docs/modding/bindings/` in the parent
//!   repo. Override with `--docs-out <dir>`.
//!
//! Pass `--stubs-only` or `--docs-only` to emit just one set.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let cli = parse_args();

    if cli.emit_stubs {
        write_stubs(&cli.stubs_out)?;
    }
    if cli.emit_docs {
        write_docs(&cli.docs_out)?;
    }
    Ok(())
}

fn write_stubs(out: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(out)?;
    let (files, report) = mercs2_script::stubs::generate();
    for f in &files {
        let path = out.join(format!("{}.lua", f.global));
        std::fs::write(&path, &f.source)?;
    }
    // Convenience `.luarc.json` snippet — write it beside the generated files so a modder
    // pointing their editor at this folder sees the pattern spelled out.
    let luarc = out.join(".luarc.example.json");
    std::fs::write(
        &luarc,
        r#"{
  "$schema": "https://raw.githubusercontent.com/LuaLS/vscode-lua/master/setting/schema.json",
  "runtime.version": "Lua 5.1",
  "workspace.library": [
    "./"
  ],
  "diagnostics.disable": ["lowercase-global"]
}
"#,
    )?;
    println!(
        "stubs: {}\n  → {}",
        mercs2_script::stubs::summarize(&report),
        out.display()
    );
    Ok(())
}

fn write_docs(out: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(out)?;
    let (files, report) = mercs2_script::docs::generate_docs();
    for f in &files {
        let path = out.join(mercs2_script::docs::filename_for(f.global));
        std::fs::write(&path, &f.source)?;
    }
    // Index page — one row per Lua global, sorted by binding-count desc so the most heavily
    // used surfaces (`Player`, `Hud`, `_GuiInternal`, `Object`, `Pg`) surface first.
    let index = mercs2_script::docs::generate_index();
    std::fs::write(out.join("index.md"), &index)?;
    println!(
        "docs:  {}\n  → {}",
        mercs2_script::docs::summarize(&report),
        out.display()
    );
    Ok(())
}

/// The tool's whole CLI surface. Two output paths and two opt-out flags — the interesting
/// combinations, no filler.
struct Cli {
    stubs_out: PathBuf,
    docs_out: PathBuf,
    emit_stubs: bool,
    emit_docs: bool,
}

/// Hand-parsed. `clap` is not in this crate's dependency tree and pulling it in for four flags
/// is more churn than help.
fn parse_args() -> Cli {
    let mut cli = Cli {
        stubs_out: default_stubs_out(),
        docs_out: default_docs_out(),
        emit_stubs: true,
        emit_docs: true,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" | "--stubs-out" => {
                if let Some(v) = args.next() {
                    cli.stubs_out = PathBuf::from(v);
                }
            }
            "--docs-out" => {
                if let Some(v) = args.next() {
                    cli.docs_out = PathBuf::from(v);
                }
            }
            "--stubs-only" => cli.emit_docs = false,
            "--docs-only" => cli.emit_stubs = false,
            other => {
                if let Some(rest) = other.strip_prefix("--out=") {
                    cli.stubs_out = PathBuf::from(rest);
                } else if let Some(rest) = other.strip_prefix("--stubs-out=") {
                    cli.stubs_out = PathBuf::from(rest);
                } else if let Some(rest) = other.strip_prefix("--docs-out=") {
                    cli.docs_out = PathBuf::from(rest);
                }
                // Silently ignore unknown args. This tool has no surface to be strict about.
            }
        }
    }
    cli
}

/// Default stubs output: `<wad_simulator>/output/emmylua/mercs2/`.
///
/// `CARGO_MANIFEST_DIR` = `tools/wad_simulator/crates/mercs2_script/`; pop the crate name and
/// `crates/` and we are at `tools/wad_simulator/`.
fn default_stubs_out() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // mercs2_script → crates
    p.pop(); // crates → tools/wad_simulator
    p.push("output");
    p.push("emmylua");
    p.push("mercs2");
    p
}

/// Default docs output: `<parent-repo-root>/docs/modding/bindings/`.
///
/// `tools/wad_simulator/` is a nested git repo inside `notes-on-the-released-game/`, so two
/// more pops from the wad_simulator root land us at the parent repo, whose `docs/` tree serves
/// the modder-facing markdown that GitHub renders.
fn default_docs_out() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // mercs2_script
    p.pop(); // crates
    p.pop(); // wad_simulator
    p.pop(); // tools → parent repo root
    p.push("docs");
    p.push("modding");
    p.push("bindings");
    p
}
