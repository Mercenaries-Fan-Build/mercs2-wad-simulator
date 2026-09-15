//! `emmylua-gen` — write EmmyLua stub files for this crate's engine binding surface.
//!
//! One `.lua` file per Lua global, into `<out>/<Global>.lua`. Default output is
//! `output/emmylua/mercs2/` under the workspace root; pass `--out <dir>` to override.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let out = parse_out_arg().unwrap_or_else(default_out);

    std::fs::create_dir_all(&out)?;

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
        "{}\nWrote to {}",
        mercs2_script::stubs::summarize(&report),
        out.display()
    );
    Ok(())
}

/// Parse a `--out <dir>` argument. Everything else is silently ignored — the tool has one
/// switch and does not pretend to have more.
fn parse_out_arg() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--out" {
            return args.next().map(PathBuf::from);
        }
        if let Some(rest) = a.strip_prefix("--out=") {
            return Some(PathBuf::from(rest));
        }
    }
    None
}

/// Default output directory: `<workspace_root>/output/emmylua/mercs2/`, mirroring the
/// `output/engine_reassembled/` convention (`mercs2_reassemble/README.md`).
///
/// `CARGO_MANIFEST_DIR` here is `tools/wad_simulator/crates/mercs2_script/`, so popping the crate
/// name and `crates/` leaves the workspace root at `tools/wad_simulator/`.
fn default_out() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // pop `mercs2_script`, `crates` → we are at `tools/wad_simulator/`.
    p.pop();
    p.pop();
    p.push("output");
    p.push("emmylua");
    p.push("mercs2");
    p
}
