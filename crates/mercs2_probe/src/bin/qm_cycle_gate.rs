//! Forge: a live-gate `vz-patch.wad` whose scripts carry DEPS cycles, for testing the engine's
//! script loader against the model of it (pc-v10 queue `0x59CD30`, step `0x59CD70`, pump `0x5A2BC0`).
//!
//!   cargo run -p mercs2_probe --bin qm_cycle_gate -- --game <vz.wad> --out <dir> [--mark <list>] <shipment-dir> [...]
//!
//! `--mark <list>` names a file of retail script names, one per line (`script_deps_cycles` prints a
//! cycle cluster's members): each gets an append of `print("[qmcyc] body <name>")`, so the log shows the
//! order the scripts' bodies finish in. The markers link first, as the Shipment `qm-cycle-gate-markers`.
//!
//! qm refuses a set whose code, at its top level, uses a module whose body has not run yet (M0314),
//! which a gate that prints what each body sees does, so qm cannot build this gate. This forge
//! links the Shipments' Lua with qm's own link (`link_into_level_blocks`, qm's hooks and loader
//! included), prints every import finding (M0209, M0313, M0314) instead of refusing on it, and writes
//! the touched scripts blocks with qm's own block emitter, WAD assembler and self-check. Every
//! `patch_lua` append is **spliced** onto the retail bytecode (`append_lua`), never recompiled from
//! the corpus, so the scripts under test run their retail code with the append at the end of their
//! body. Only `add_script` and `patch_lua` are handled; any other contribution kind is an error.
//!
//! Writes `<dir>/vz-patch.wad` and prints its size and sha256. Exit 0 on success, 2 on any error.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mercs2_formats::patch_wad::{build_patch_wad_multi, FFCS_CERT_BLOB};
use mercs2_quartermaster::manifest::Contribution;
use mercs2_quartermaster::{build, discover, link};

fn run(game_wad: &Path, out_dir: &Path, mark: Option<&Path>, shipments: &[PathBuf]) -> Result<(), String> {
    let mut mutations = Vec::new();
    let mut additions = Vec::new();
    let mut order = Vec::new();
    if let Some(list) = mark {
        const MARKERS: &str = "qm-cycle-gate-markers";
        let text = std::fs::read_to_string(list).map_err(|e| format!("{}: {e}", list.display()))?;
        for name in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
                return Err(format!("{}: {name:?} is not a bare lowercase script name", list.display()));
            }
            mutations.push(link::ScriptMutation {
                shipment: MARKERS.into(),
                target: name.into(),
                append: format!("print(\"[qmcyc] body {name}\")\n"),
            });
        }
        order.push(MARKERS.to_string());
    }
    for dir in shipments {
        let s = discover::open(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (i, c) in s.manifest.contributions.iter().enumerate() {
            if !matches!(c, Contribution::AddScript { .. } | Contribution::PatchLua { .. }) {
                return Err(format!("{}: contributions[{i}] is {}; the forge handles add_script and patch_lua only", dir.display(), c.kind()));
            }
        }
        mutations.extend(build::script_mutations(&s.manifest, &s.root).map_err(|e| format!("{}: {e}", dir.display()))?);
        additions.extend(build::script_additions(&s.manifest, &s.root).map_err(|e| format!("{}: {e}", dir.display()))?);
        order.push(s.manifest.shipment.name.clone());
    }
    let mut game = mercs2_quartermaster::GameStack::open(&[game_wad.to_path_buf()]).map_err(|e| e.to_string())?;
    let mut loaded = build::load_script_blocks(&mut game, link::SCRIPT_BLOCKS, "forge").map_err(|e| e.to_string())?;
    let spliced: BTreeSet<u32> = mutations.iter().map(|m| mercs2_quartermaster::manifest::asset_hash(&m.target)).collect();
    let out = {
        let mut targets: Vec<link::TargetBlock<'_>> =
            loaded.iter_mut().map(|lb| link::TargetBlock { path: lb.path.clone(), block: &mut lb.block }).collect();
        // Every append is spliced, so the corpus is never read.
        link::link_into_level_blocks(
            &mut targets,
            Path::new("/nonexistent-corpus-the-forge-splices"),
            &mutations,
            &[],
            &[],
            &[],
            &[],
            &additions,
            &[],
            &order,
            &spliced,
            link::QmHooks::Always,
            None,
        )
        .map_err(|e| e.to_string())?
    };
    for f in &out.findings {
        println!("finding\t{}\t{}", f.rule.code(), f.message);
    }
    for l in &out.scripts {
        println!("linked\t{}\t{}\t{:?}", l.target, l.bytecode_bytes, l.contributors);
    }
    let touched: BTreeSet<usize> = out.scripts.iter().map(|l| l.block).collect();
    let blocks = build::script_patch_blocks(&loaded, &touched, "forge").map_err(|e| e.to_string())?;
    let csum = mercs2_formats::donor::base_csum(game_wad)?;
    let wad = build_patch_wad_multi(&blocks, csum.0, csum.1, &FFCS_CERT_BLOB)?;
    for d in build::verify_emitted(&wad, &mercs2_quartermaster::shader::ShaderKeys::with(&[])).map_err(|e| e.to_string())? {
        println!("self-check\t{d}");
    }
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let path = out_dir.join("vz-patch.wad");
    std::fs::write(&path, &wad).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote\t{}\t{}\t{}", path.display(), wad.len(), build::sha256_hex(&wad));
    Ok(())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut game, mut out, mut mark, mut shipments) = (None, None, None, Vec::new());
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => game = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--mark" => mark = args.next().map(PathBuf::from),
            _ => shipments.push(PathBuf::from(a)),
        }
    }
    let (Some(game), Some(out)) = (game, out) else {
        eprintln!("usage: qm_cycle_gate --game <vz.wad> --out <dir> [--mark <list>] <shipment-dir> [...]");
        return ExitCode::from(2);
    };
    if shipments.is_empty() {
        eprintln!("qm_cycle_gate: name at least one Shipment directory");
        return ExitCode::from(2);
    }
    match run(&game, &out, mark.as_deref(), &shipments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("qm_cycle_gate: {e}");
            ExitCode::from(2)
        }
    }
}
