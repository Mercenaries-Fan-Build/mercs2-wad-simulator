//! Stage-2 destruction-state extractor.
//!
//! `destruction_extract <blob> --out-dir <dir>` reads a decompressed block, and
//! for every model container that carries a `SWIT` switch list, classifies its
//! HIER nodes into intact / break_piece / static (via
//! [`mercs2_formats::orchestrator`]). Emits `destruction.json` keyed by model
//! hash. A block with no switching models writes `{"orchestrated": false}`.
//!
//! The post-stage-2 join (`tools/destruction_join.py`) maps a stripped geometry
//! block's submeshes to its orchestrator's `destruction.json` by model hash +
//! HIER node, so the workbench can show one destruction state at a time.

use mercs2_formats::orchestrator;
use mercs2_formats::ucfx;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let mut blob: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out-dir" => out_dir = args.next().map(PathBuf::from),
            s if !s.starts_with("--") && blob.is_none() => blob = Some(PathBuf::from(s)),
            _ => {}
        }
    }
    let (Some(blob), Some(out_dir)) = (blob, out_dir) else {
        eprintln!("usage: destruction_extract <blob> --out-dir <dir>");
        return 2;
    };

    let data = match fs::read(&blob) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("destruction_extract: read {}: {e}", blob.display());
            return 1;
        }
    };
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("destruction_extract: mkdir {}: {e}", out_dir.display());
        return 1;
    }

    let (parsed, _issues) = ucfx::walk_decompressed_block(&data, "destruction_extract");
    let mut models = Vec::new();
    let mut any_orchestrated = false;
    for (i, container) in parsed.containers.iter().enumerate() {
        // A model container carries an INDX (mesh→node map) and/or a SWIT
        // (destruction switch). Geometry blocks have INDX but no SWIT; the
        // orchestrator has both. Emit either so the join can map mesh→node→state.
        let indx = orchestrator::parse_indx(container);
        let d = orchestrator::classify(container);
        if indx.is_empty() && d.is_none() {
            continue; // not a model container
        }
        let model_hash = parsed.entries.get(i).map(|e| e.name_hash).unwrap_or(0);
        let nodes: Vec<_> = d
            .as_ref()
            .map(|d| {
                d.nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "hier_node": n.hier_node,
                            "parent": n.parent,
                            "hash": format!("0x{:08X}", n.hash),
                            "destruction_state": n.state.as_str(),
                            "switch_group": n.switch_group,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !nodes.is_empty() {
            any_orchestrated = true;
        }
        // Grounded collision hulls (PHY2 verts placed in model space via their
        // HIER node world transform; hull→node from SEGM) — for the viewer overlay.
        let hulls: Vec<_> = orchestrator::grounded_hulls(container)
            .iter()
            .map(|h| json!({ "node": h.node, "vertices": h.vertices }))
            .collect();
        // INDX from classify() (orchestrator's own) preferred; else the standalone parse.
        let indx_out = d.as_ref().map(|d| d.indx.clone()).filter(|v| !v.is_empty()).unwrap_or(indx);
        models.push(json!({
            "model_hash": format!("0x{:08X}", model_hash),
            "switch_group_count": d.as_ref().map_or(0, |d| d.switch_group_count),
            "hull_count": d.as_ref().map_or(0, |d| d.hull_count),
            "indx": indx_out,
            "warnings": d.as_ref().map(|d| d.warnings.clone()).unwrap_or_default(),
            "nodes": nodes,
            "hulls": hulls,
        }));
    }

    let manifest = json!({
        "schema": "mercs2_destruction/1",
        "extractor": "mercs2_formats::orchestrator",
        "orchestrated": any_orchestrated,
        "orchestrated_models": models,
    });
    if let Err(e) = fs::write(
        out_dir.join("destruction.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    ) {
        eprintln!("destruction_extract: write manifest: {e}");
        return 1;
    }
    let count = manifest["orchestrated_models"].as_array().map_or(0, |a| a.len());
    println!(
        "destruction_extract: {count} destructible model(s) from {}",
        blob.display()
    );
    0
}
