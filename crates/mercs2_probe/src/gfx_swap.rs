//! `gfx-swap-constant` and `gfx-patch-actions`: [`mercs2_formats::gfx_actions`] on a movie file.

use mercs2_formats::gfx_actions::{check_decodes, parse_patches, patch_actions, swap_constant, Target};
use std::path::Path;

/// `mercs2_probe gfx-patch-actions <in.gfx> <out.gfx> <patches.tsv>`
pub fn run_patch(args: &[String]) -> Result<(), String> {
    let [input, output, table] = args else {
        return Err("usage: mercs2_probe gfx-patch-actions <in.gfx> <out.gfx> <patches.tsv>".into());
    };
    let patches = parse_patches(&std::fs::read_to_string(table).map_err(|e| format!("read {table}: {e}"))?)?;
    let data = std::fs::read(input).map_err(|e| format!("read {input}: {e}"))?;
    let (out, done) = patch_actions(&data, &patches)?;
    check_decodes(&out)?;
    std::fs::write(Path::new(output), &out).map_err(|e| format!("write {output}: {e}"))?;
    for (t, len) in &done {
        println!("{t:?}: DoAction now {len} bytes");
    }
    println!("wrote {output} ({} bytes)", out.len());
    Ok(())
}

/// `mercs2_probe gfx-swap-constant <in.gfx> <out.gfx> <from> <to> <target>...`
pub fn run(args: &[String]) -> Result<(), String> {
    let [input, output, from, to, targets @ ..] = args else {
        return Err("usage: mercs2_probe gfx-swap-constant <in.gfx> <out.gfx> <from> <to> <root:<frame>|sprite:<id>:<frame>>...".into());
    };
    if targets.is_empty() {
        return Err("name at least one target block".into());
    }
    let targets = targets.iter().map(|t| Target::parse(t)).collect::<Result<Vec<_>, _>>()?;
    let data = std::fs::read(input).map_err(|e| format!("read {input}: {e}"))?;
    let (out, done) = swap_constant(&data, &targets, from, to)?;
    check_decodes(&out)?;
    std::fs::write(Path::new(output), &out).map_err(|e| format!("write {output}: {e}"))?;
    for (t, len) in &done {
        println!("{t:?}: {from:?} -> {to:?} (DoAction now {len} bytes)");
    }
    println!("wrote {output} ({} bytes)", out.len());
    Ok(())
}
