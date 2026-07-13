//! `mercs2_workshop` — the engine's first dedicated developer tool.
//!
//! A native workshop over the SAME `mercs2_engine` renderer the game boots: browse every model /
//! texture in the install, preview them faithfully (materials, skinning, clips), inspect layers
//! (HIER, per-material draw groups, texture plates), and compose an editable sandbox scene —
//! all without booting the game. See `docs/modernization/workshop_charter.md` for the
//! workbench roadmap (mission design, model import/fix, AV replacement, unlock auditing).
//!
//! Run `cargo run -p mercs2_workshop` to open the window. Every workbench action also has a
//! headless flag, so the same code paths are scriptable.
//!
//! WAD selection (applies to all modes):
//! - `--wad <path>` overrides the registry-discovered `vz.wad`.
//! - `--overlay <path>` (repeatable) opens patch WADs ON TOP of the base — the game's own
//!   `vz-patch.wad` mechanism (last-opened wins), which is how DLC-ported content (obama, sarah,
//!   the DLC world blocks) gets into the workbench. A `vz-patch.wad` sitting next to the base
//!   wad is auto-loaded, exactly as the retail exe does; `--no-auto-patch` disables that.
//! - `--names <csv>` overrides the name corpus (default: see `index::default_names_csv`).
//!
//! Headless modes (each runs and exits). `<name|0xH>` takes a raw m2 hash or a name to hash:
//! - `--list [filter]`                     — catalog dump (models + textures), substring-filtered
//! - `--inventory [class]`                 — models grouped by vehicle class (heli, tank, car, …)
//! - `--check <name|0xH>`                  — load a model end-to-end: geometry, textures, clips,
//!                                           bones, LOD tiers, destruction SM, ActionTable coverage
//! - `--states <name|0xH>`                 — dump the destruction state machine, names resolved
//! - `--export <name|0xH>`                 — OBJ + MTL + PNG -> `workshop_export/<name>/`
//! - `--export-bundle <name|0xH|class:X> [--out dir]`
//!                                         — LOSSLESS bundle (see `bundle`): editable glTF + PNG
//!                                           skins + every LOD rung's ORIGINAL bytes + manifest
//! - `--mod-new <name> <donor|0xH> <mesh> [--mod-group N] [--mod-out path]`
//!                                         — publish a NOVEL new-hash model into a patch WAD
//! - `--import-check <file>`               — parse a foreign `.obj`/`.gltf`/`.glb`, print what imports
//! - `--tex-check <name|0xH>`              — one texture's dims/format/mips
//! - `--tex-png <name|0xH> <out.png>`      — decode a texture to PNG (full mip chain)
//! - `--tex-png-block <blk> <0xH> <out.png>` — decode ONE block's texture chunk (one mip level)
//! - `--tex-scan` / `--tex-scan-blocks`    — dims/format of every texture ASET / every texture CHUNK
//! - `--hash <names…>` / `--hash-file <f>` — m2-hash names (hash-hunting)
//! - `--block-strings <blk>`               — printable ASCII (≥5 chars) from a decompressed block
//! - `--pack-data [dir]`                   — build the redistributable `workshop_data/` bundle
//!
//! Module map:
//! - [`app`]     — the window: winit loop over the engine `Scene`, asset workbench, editable
//!                 sandbox (`workshop_scene.json`), the OBJ/PNG exporter.
//! - [`bundle`]  — the lossless export bundle; nothing is discarded (raw rung bytes survive).
//! - [`gui`]     — egui host: hand-rolled winit-0.29 → egui bridge + the egui-wgpu paint path.
//! - [`import`]  — foreign model import (`.obj`/`.gltf`/`.glb`) into the engine's `ModelData`.
//! - [`index`]   — asset catalog + name resolution (packed `names.bin`, embedded ASET dictionary,
//!                 repo-corpora fallback).
//! - [`luaview`] — read-only Lua source viewer (hand-rolled lexer/highlighter).
//! - [`publish`] — background-threaded mod publishing: inject, compress, SHA-256, load self-test.
//! - [`texenc`]  — CPU BC1/BC3 encode for imported images.
//! - [`texpng`]  — CPU BC1/BC3 decode + PNG write for the headless texture dumps.

mod app;
mod bundle;
mod gui;
mod import;
mod index;
mod luaview;
mod publish;
mod texenc;
mod texpng;

use mercs2_engine::{mesh, wad};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |flag: &str| {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
    };

    let wadpath = match get("--wad").or_else(wad::registry_vz_wad) {
        Some(p) => p,
        None => {
            eprintln!("workshop: no vz.wad found (install not in registry) — pass --wad <path>");
            return;
        }
    };
    let names_csv = get("--names").map(std::path::PathBuf::from).or_else(index::default_names_csv);

    // Overlay stack: every `--overlay <path>`, in argument order, plus (unless --no-auto-patch)
    // an auto-loaded `vz-patch.wad` next to the base — the retail exe's own patch lookup.
    let mut overlays: Vec<String> = Vec::new();
    if !args.iter().any(|a| a == "--no-auto-patch") {
        let auto = std::path::Path::new(&wadpath).with_file_name("vz-patch.wad");
        if auto.is_file() {
            overlays.push(auto.to_string_lossy().into_owned());
        }
    }
    for i in 0..args.len().saturating_sub(1) {
        if args[i] == "--overlay" {
            overlays.push(args[i + 1].clone());
        }
    }

    // Build the redistributable reference bundle: everything the workshop consults that is NOT a
    // game-distributed file, in load-fast formats. The app then runs self-contained from
    // `workshop_data/` next to the exe (see index::data_home) — and this is the substrate the
    // richer insight features (Identity panel, COMP inspector, the Lua blueprint editor) read.
    if let Some(i) = args.iter().position(|a| a == "--pack-data") {
        let out = args
            .get(i + 1)
            .filter(|s| !s.starts_with("--"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("workshop_data"));
        if let Err(e) = pack_data(&out, names_csv) {
            eprintln!("--pack-data: {e}");
        }
        return;
    }

    // Headless: dump the catalog (optionally substring-filtered) and exit.
    if let Some(i) = args.iter().position(|a| a == "--list") {
        let filter = args.get(i + 1).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        let stack = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let idx = index::AssetIndex::build(&stack.wads, index::load_all_names(names_csv));
        for (kind, rows) in
            [(index::Kind::Model, &idx.models), (index::Kind::Texture, &idx.textures)]
        {
            for r in rows.iter().filter(|r| {
                filter.is_empty() || r.label().to_ascii_lowercase().contains(&filter)
            }) {
                println!("{}{}\t0x{:08X}\t{}", kind.label(), stack.tag(r.src), r.hash, r.label());
            }
        }
        return;
    }

    // Headless: the Model Workbench vehicle inventory — models grouped by class (helicopter,
    // tank, car, boat, …). `--inventory [class]` filters to one class.
    if let Some(i) = args.iter().position(|a| a == "--inventory") {
        let want = args.get(i + 1).filter(|s| !s.starts_with("--")).map(|s| s.to_ascii_lowercase());
        let stack = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let idx = index::AssetIndex::build(&stack.wads, index::load_all_names(names_csv));
        let mut by_class: std::collections::BTreeMap<&'static str, Vec<&index::AssetRow>> =
            std::collections::BTreeMap::new();
        for r in &idx.models {
            if let Some(c) = r.vehicle_class() {
                if want.as_deref().is_none_or(|w| w == c) {
                    by_class.entry(c).or_default().push(r);
                }
            }
        }
        for (class, mut rows) in by_class {
            rows.sort_by(|a, b| a.label().cmp(&b.label()));
            println!("\n== {class} ({}) ==", rows.len());
            for r in rows {
                println!("  0x{:08X}{}  {}", r.hash, stack.tag(r.src), r.label());
            }
        }
        return;
    }

    // Headless: publish a NOVEL new-hash model asset (the GUI Mod-project flow, scriptable).
    // --mod-new <name> <donor name|0xHASH> <mesh.obj|.gltf|.glb> [--mod-group N] [--mod-out path]
    if let Some(i) = args.iter().position(|a| a == "--mod-new") {
        let (Some(name), Some(donor_arg), Some(mesh_path)) =
            (args.get(i + 1), args.get(i + 2), args.get(i + 3))
        else {
            eprintln!("--mod-new <name> <donor name|0xHASH> <mesh file>");
            return;
        };
        let donor = donor_arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(donor_arg));
        let group: usize = get("--mod-group").and_then(|s| s.parse().ok()).unwrap_or(0);
        let out = get("--mod-out").map(std::path::PathBuf::from).unwrap_or_else(|| {
            std::path::Path::new(&wadpath)
                .parent()
                .map(|d| d.join("vz-mod.wad"))
                .unwrap_or_else(|| std::path::PathBuf::from("vz-mod.wad"))
        });
        let imp = match import::import_model(std::path::Path::new(mesh_path)) {
            Ok(m) => m,
            Err(e) => return eprintln!("--mod-new: import {mesh_path}: {e}"),
        };
        let mesh = mercs2_formats::model_inject::ExternalMesh {
            positions: imp.verts.iter().map(|v| v.pos).collect(),
            normals: imp.verts.iter().map(|v| v.normal).collect(),
            uvs: imp.verts.iter().map(|v| v.uv).collect(),
            tris: imp.indices.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
            joints: Vec::new(),
            weights: Vec::new(),
        };
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let item = publish::NewModelItem {
            name: name.clone(),
            hash,
            donor,
            donor_label: donor_arg.clone(),
            target_group: group,
            flip: false,
            mesh,
        };
        let mut paths = vec![wadpath.clone()];
        paths.extend(overlays.iter().cloned());
        let publisher = publish::publish_in_background(paths, vec![item], out);
        match publisher.rx.recv() {
            Ok(Ok(r)) => {
                println!("wrote {} ({} bytes)\nsha256 {}", r.path.display(), r.bytes, r.sha256);
                for (n, res) in &r.results {
                    match res {
                        Ok(s) => println!("self-test {n} (0x{hash:08X}): {s}"),
                        Err(e) => println!("self-test {n} (0x{hash:08X}): FAIL {e}"),
                    }
                }
            }
            Ok(Err(e)) => eprintln!("--mod-new: {e}"),
            Err(_) => eprintln!("--mod-new: worker died"),
        }
        return;
    }

    // Headless: export a model to workshop_export/<name>/ (OBJ + MTL + PNG textures).
    if let Some(arg) = get("--export") {
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        match app::export_by_hash(&wadpath, &overlays, hash, &arg) {
            Ok(dir) => println!("exported {arg} -> {dir}"),
            Err(e) => eprintln!("--export {arg}: {e}"),
        }
        return;
    }

    // Headless: LOSSLESS export bundle(s) — editable glTF + PNG skins + the ORIGINAL container
    // bytes of every LOD rung + a reassembly manifest. `--export-bundle <name|0xHASH|class:heli>`.
    // Nothing is discarded: chunks we cannot yet decode survive byte-exact in raw/.
    if let Some(i) = args.iter().position(|a| a == "--export-bundle") {
        let arg = args.get(i + 1).cloned().unwrap_or_default();
        let outroot = std::path::PathBuf::from(
            args.iter().position(|a| a == "--out").and_then(|j| args.get(j + 1)).cloned()
                .unwrap_or_else(|| "workshop_export".into()),
        );
        let stack = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let idx = index::AssetIndex::build(&stack.wads, index::load_all_names(names_csv));
        // A whole vehicle class ("class:helicopter") or a single asset.
        let targets: Vec<(u32, String)> = if let Some(cls) = arg.strip_prefix("class:") {
            idx.models.iter()
                .filter(|r| r.vehicle_class() == Some(cls))
                .map(|r| (r.hash, r.label()))
                .collect()
        } else {
            let h = arg.strip_prefix("0x").and_then(|x| u32::from_str_radix(x, 16).ok())
                .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(&arg));
            let label = idx.names.get(&h).cloned().unwrap_or_else(|| arg.clone());
            vec![(h, label)]
        };
        if targets.is_empty() {
            return eprintln!("--export-bundle: nothing matched '{arg}'");
        }
        drop(stack);
        let (mut ok, mut fail) = (0, 0);
        for (h, label) in &targets {
            match app::export_bundle_by_hash(&wadpath, &overlays, *h, label, &idx, &outroot) {
                Ok(d) => { ok += 1; println!("  [ok]   {label} (0x{h:08X}) -> {d}"); }
                Err(e) => { fail += 1; eprintln!("  [FAIL] {label} (0x{h:08X}): {e}"); }
            }
        }
        println!("
exported {ok} bundle(s), {fail} failed -> {}", outroot.display());
        return;
    }

    // Headless: m2-hash arbitrary names (for identifying unknown hashes by guess-list).
    if let Some(i) = args.iter().position(|a| a == "--hash") {
        for a in &args[i + 1..] {
            println!("0x{:08X}  {a}", mercs2_formats::hash::pandemic_hash_m2(a));
        }
        return;
    }

    // Headless: dump printable ASCII strings (≥5 chars) from a decompressed block — pairs with
    // --hash-file for hash-hunting names that live INSIDE game data (Havok annotations etc.).
    if let Some(blk) = get("--block-strings") {
        let blk: u16 = blk.parse().unwrap_or(0);
        let mut w = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        match mercs2_engine::wad::decompress_block_index(&mut w.wads[0], blk) {
            Ok(dec) => {
                let mut cur = Vec::new();
                for &b in dec.iter().chain(std::iter::once(&0u8)) {
                    if (0x20..0x7f).contains(&b) {
                        cur.push(b);
                    } else {
                        if cur.len() >= 5 {
                            println!("{}", String::from_utf8_lossy(&cur));
                        }
                        cur.clear();
                    }
                }
            }
            Err(e) => eprintln!("--block-strings {blk}: {e}"),
        }
        return;
    }

    // Headless: m2-hash every line of a file (bulk hash-hunting, e.g. exe string harvests).
    if let Some(f) = get("--hash-file") {
        match std::fs::read_to_string(&f) {
            Ok(text) => {
                for line in text.lines() {
                    let n = line.trim();
                    if !n.is_empty() {
                        println!("0x{:08X}\t{n}", mercs2_formats::hash::pandemic_hash_m2(n));
                    }
                }
            }
            Err(e) => eprintln!("--hash-file {f}: {e}"),
        }
        return;
    }

    // Headless: dump a model's destruction STATE MACHINE (the engine's own format —
    // docs/destruction_orchestrator_format.md) with names resolved, plus HIER/SEGM correlation.
    if let Some(arg) = get("--states") {
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        let mut w = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let container = match w.extract_container(hash) {
            Ok(c) => c,
            Err(e) => return eprintln!("--states '{arg}' (0x{hash:08X}): {e}"),
        };
        let names = index::load_all_names(names_csv);
        let nm = |h: u32| names.get(&h).cloned().unwrap_or_else(|| format!("0x{h:08X}"));
        let hier = mercs2_formats::orchestrator::parse_hier(&container);
        let hier_hashes: std::collections::HashSet<u32> = hier.iter().map(|n| n.hash).collect();
        let tiers = mesh::state_tiers(&container);
        println!(
            "0x{hash:08X} {}: {} HIER nodes, SEGM tiers [{}]",
            nm(hash),
            hier.len(),
            tiers.iter().map(|b| format!("0x{b:02X}")).collect::<Vec<_>>().join(" ")
        );
        match mercs2_formats::orchestrator::parse_state_machine(&container) {
            None => println!("no state machine (NODE/STAT family absent)"),
            Some(sm) => {
                println!(
                    "state machine: {} switch slot(s) {:?}, {} node(s)",
                    sm.switch_slots.len(),
                    sm.switch_slots.iter().map(|s| format!("0x{s:08X}")).collect::<Vec<_>>(),
                    sm.nodes.len()
                );
                let resolve = |h: u32| {
                    let tag = if hier_hashes.contains(&h) { "HIER:" } else { "" };
                    format!("{tag}{}", nm(h))
                };
                for (ni, node) in sm.nodes.iter().enumerate() {
                    println!("  node {ni}: {} ({} state(s))", nm(node.name_hash), node.states.len());
                    for st in &node.states {
                        println!("    state {}", nm(st.name_hash));
                        let enter = mercs2_formats::orchestrator::decode_script(&st.enter, resolve);
                        let exit = mercs2_formats::orchestrator::decode_script(&st.exit, resolve);
                        if !enter.is_empty() {
                            println!("      enter: {enter}");
                        }
                        if !exit.is_empty() {
                            println!("      exit:  {exit}");
                        }
                    }
                }
            }
        }
        return;
    }

    // Headless: parse a foreign model (.obj/.gltf/.glb) and print what would import.
    if let Some(file) = get("--import-check") {
        match import::import_model(std::path::Path::new(&file)) {
            Ok(im) => println!(
                "{file}: {} verts, {} tris, {} draw groups, {} textures, bbox {:?}..{:?}",
                im.verts.len(),
                im.indices.len() / 3,
                im.draws.len(),
                im.textures.len(),
                im.stats.bbox_min,
                im.stats.bbox_max
            ),
            Err(e) => eprintln!("--import-check {file}: {e}"),
        }
        return;
    }

    // Headless: decode one texture and print its dims/format/mips.
    if let Some(arg) = get("--tex-check") {
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        let mut w = match wad::open(&wadpath) {
            Ok(w) => w,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        match wad::extract_texture(&mut w, hash) {
            Ok(td) => println!(
                "0x{hash:08X}: {}x{}  {:?}  {} mips  {} bytes",
                td.width, td.height, td.format, td.mip_count, td.all_mips.len()
            ),
            Err(e) => eprintln!("--tex-check '{arg}' (0x{hash:08X}): {e}"),
        }
        return;
    }

    // Headless: dims/format of EVERY texture ASET in the wad (one pass — for hunting art by size).
    if args.iter().any(|a| a == "--tex-scan") {
        let mut w = match wad::open(&wadpath) {
            Ok(w) => w,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let names = index::load_all_names(names_csv);
        let list: Vec<u32> = wad::all_asets(&w)
            .into_iter()
            .filter(|&(_, ty, primary)| ty == mercs2_formats::types::TYPE_ID_TEXTURE && primary)
            .map(|(h, _, _)| h)
            .collect();
        for h in list {
            if let Ok(td) = wad::extract_texture(&mut w, h) {
                let name = names.get(&h).map(String::as_str).unwrap_or("-");
                println!("0x{h:08X}\t{}x{}\t{:?}\t{name}", td.width, td.height, td.format);
            }
        }
        return;
    }

    // Headless: walk EVERY block's entry table and report each texture CHUNK (block, hash, dims).
    // Ground truth when ASET resolution dedupes to the wrong chunk (multi-texture shell blocks).
    if args.iter().any(|a| a == "--tex-scan-blocks") {
        let mut w = match wad::open(&wadpath) {
            Ok(w) => w,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let names = index::load_all_names(names_csv);
        let nblocks = wad::block_paths(&w).len();
        for blk in 0..nblocks as u16 {
            let Ok(dec) = wad::decompress_block_index(&mut w, blk) else { continue };
            let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
            let mut off = 4 + count as usize * 16;
            for e in &entries {
                let end = off + e.chunk_size as usize;
                if e.type_hash == mercs2_formats::types::TYPE_HASH_TEXTURE && end <= dec.len() {
                    if let Ok(td) = mercs2_formats::texture::parse_texture_container(&dec[off..end]) {
                        let name = names.get(&e.name_hash).map(String::as_str).unwrap_or("-");
                        println!(
                            "blk {blk}\t0x{:08X}\t{}x{}\t{:?}\toff {off}\tfield_c 0x{:X}\t{name}",
                            e.name_hash, td.width, td.height, td.format, e.field_c
                        );
                    }
                }
                off = end;
            }
        }
        return;
    }

    // Headless: decode one texture chunk from ONE specific block to PNG
    // (`--tex-png-block <block> <0xHASH> <out.png>`).
    if let Some(i) = args.iter().position(|a| a == "--tex-png-block") {
        let (Some(blk), Some(arg), Some(out)) = (args.get(i + 1), args.get(i + 2), args.get(i + 3))
        else {
            return eprintln!("--tex-png-block <block> <0xHASH> <out.png>");
        };
        let blk: u16 = blk.parse().unwrap_or(0);
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        let mut w = match wad::open(&wadpath) {
            Ok(w) => w,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        match wad::tex_from_block(&mut w, blk, hash) {
            Some(td) => {
                // Decoded dims, not declared: one block holds one mip level, so the surface here is
                // usually coarser than the texture's full size.
                let (pw, ph, rgba) = texpng::decode_bc(&td);
                match texpng::write_png(out, pw, ph, &rgba) {
                    Ok(()) => println!("blk {blk} 0x{hash:08X}: {pw}x{ph} of {}x{} {:?} -> {out}", td.width, td.height, td.format),
                    Err(e) => eprintln!("--tex-png-block: PNG write failed: {e}"),
                }
            }
            None => eprintln!("--tex-png-block: no texture 0x{hash:08X} in block {blk}"),
        }
        return;
    }

    // Headless: decode one texture to PNG for visual inspection (`--tex-png <name|0xH> <out.png>`).
    if let Some(i) = args.iter().position(|a| a == "--tex-png") {
        let (Some(arg), Some(out)) = (args.get(i + 1), args.get(i + 2)) else {
            return eprintln!("--tex-png <name|0xHASH> <out.png>");
        };
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        let mut w = match wad::open(&wadpath) {
            Ok(w) => w,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        // Full mip chain assembled from the finer LOD blocks, falling back to the resident tail —
        // a texture dump should hand back the real image, not the coarse residency budget.
        match wad::extract_texture_hires(&mut w, hash).or_else(|_| wad::extract_texture(&mut w, hash)) {
            Ok(td) => {
                let (pw, ph, rgba) = texpng::decode_bc(&td);
                match texpng::write_png(out, pw, ph, &rgba) {
                    Ok(()) => println!("0x{hash:08X}: {pw}x{ph} of {}x{} {:?} -> {out}", td.width, td.height, td.format),
                    Err(e) => eprintln!("--tex-png: PNG write failed: {e}"),
                }
            }
            Err(e) => eprintln!("--tex-png '{arg}' (0x{hash:08X}): {e}"),
        }
        return;
    }

    // Headless: load one model end-to-end (geometry + textures + clips) and print its stats.
    // Overlay-aware: resolves through the full stack, like the window does.
    if let Some(arg) = get("--check") {
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        let mut w = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let container = match w.extract_container(hash) {
            Ok(c) => c,
            Err(e) => return eprintln!("--check '{arg}' (0x{hash:08X}): {e}"),
        };
        // What resolving ONE model cost the asset layer: the owning block went resident and every
        // chunk it carries (sibling models, the resident texture tail, the scrub) is now registered.
        // `shadowed` counts chunks a later block re-declared and lost to the first-wins insert rule.
        let rs = w.registry.stats();
        println!(
            "asset layer: {} resident block(s), {} chunk(s) registered, {} shadowed, {} evicted",
            rs.resident_blocks, rs.registered_chunks, rs.shadowed_total, rs.evicted_total
        );
        // Load through the SAME path the preview and the exporter use: the full LOD-block chain.
        // `build_indexed_from_container` reads only the resident block, so for any model whose near
        // geometry lives in a finer rung it found nothing and reported "no placed drawing groups" —
        // a false failure. `civ_hum_beachfemale_a` is the case in point: its resident container holds
        // only mask-0x08 (far) geometry, and `Model::load` assembles 2 rungs / 15 draws just fine.
        match app::load_model_data(&mut w, hash) {
            Ok(md) => {
                let (verts, indices, draws, stats) = (&md.verts, &md.indices, &md.draws, &md.stats);
                let mut tex_all = std::collections::HashSet::new();
                for d in draws {
                    for h in [d.diffuse, d.normal, d.specular].into_iter().flatten() {
                        tex_all.insert(h);
                    }
                }
                // `load_model_data` already resolved every slot at full resolution.
                let tex_ok = md.textures.len();
                let tiers: Vec<String> = md.tiers.iter().map(|b| format!("0x{b:02X}")).collect();
                let skin = &md.skin;
                let clips = w.clips_for_model(&skin.rig);
                println!(
                    "0x{hash:08X}: {} verts, {} tris, {} draw groups, {} bones, {} clips, {tex_ok}/{} textures, tiers [{}], bbox {:?}..{:?}",
                    verts.len(),
                    indices.len() / 3,
                    draws.len(),
                    skin.rig.len(),
                    clips.len(),
                    tex_all.len(),
                    tiers.join(" "),
                    stats.bbox_min,
                    stats.bbox_max
                );
                // The engine's destruction state machine (game data — see --states for the dump).
                if let Some(sm) = mercs2_formats::orchestrator::parse_state_machine(&container) {
                    println!(
                        "destruction state machine: {} switch node(s), {} state(s), {} switch slot(s)",
                        sm.nodes.len(),
                        sm.nodes.iter().map(|n| n.states.len()).sum::<usize>(),
                        sm.switch_slots.len()
                    );
                }

                // Character-specific clip set (the AnimationLookup chain), when the name maps.
                if let Some(sel) = w.anim_selector() {
                    for cand in app::character_candidates(&arg) {
                        let key = mercs2_formats::anim_select::AnimSelector::character_name(&cand);
                        let rows = sel.character_clips(key);
                        if !rows.is_empty() {
                            let mut clips: Vec<u32> = rows.iter().map(|r| r.clip).collect();
                            clips.sort_unstable();
                            clips.dedup();
                            println!(
                                "character '{cand}' (0x{key:08X}): {} lookup rows, {} distinct clips; first 8: {}",
                                rows.len(),
                                clips.len(),
                                clips
                                    .iter()
                                    .take(8)
                                    .map(|c| format!("0x{c:08X}"))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            );
                            // ActionTable coverage: how many of this character's handles have
                            // state rows, and the loadout-context row count.
                            let mut handles: Vec<u32> = rows.iter().map(|r| r.handle).collect();
                            handles.sort_unstable();
                            handles.dedup();
                            let with_actions = handles
                                .iter()
                                .filter(|&&h| !sel.handle_actions(h).is_empty())
                                .count();
                            let action_rows: usize =
                                handles.iter().map(|&h| sel.handle_actions(h).len()).sum();
                            let ctx_rows: usize =
                                handles.iter().map(|&h| sel.lookup_context(h, key).len()).sum();
                            println!(
                                "  ActionTable: {with_actions}/{} handles have state rows ({action_rows} rows); {ctx_rows} loadout-context rows",
                                handles.len()
                            );
                            break;
                        }
                    }
                }
            }
            Err(e) => eprintln!("--check '{arg}' (0x{hash:08X}): container parse FAILED: {e}"),
        }
        return;
    }

    app::run(app::Options { wadpath, overlays, names_csv });
}

/// `--pack-data <dir>`: assemble the reference bundle. Sources are the repo corpora (found by
/// the same walk-up the app's fallback path uses); output is a portable directory.
fn pack_data(out: &std::path::Path, names_csv: Option<std::path::PathBuf>) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;

    // 1. The merged name map (devkit strings + bones + rainbow + registry) → load-fast
    // names.bin. RAW path on purpose: never read the stale pack we are replacing.
    eprintln!("[pack] merging name corpora (slow raw parse — one-time cost)…");
    let names = index::load_all_names_raw(names_csv, |_, _| {});
    if names.is_empty() {
        return Err("no name corpora found — run from the repo checkout".into());
    }
    index::write_names_pack(&out.join("names.bin"), &names).map_err(|e| e.to_string())?;
    eprintln!("[pack] names.bin: {} names", names.len());

    // 2. Structured reference tables + corpora, copied verbatim for the insight features.
    let repo_file = |rel: &str| -> Option<std::path::PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let cand = dir.join(rel);
            if cand.exists() {
                return Some(cand);
            }
            if !dir.pop() {
                return None;
            }
        }
    };
    for (rel, dest) in [
        ("docs/data/live_registry_hashes.csv", "live_registry_hashes.csv"),
        ("docs/data/spawnable_templates.csv", "spawnable_templates.csv"),
    ] {
        match repo_file(rel) {
            Some(src) => {
                std::fs::copy(&src, out.join(dest)).map_err(|e| e.to_string())?;
                eprintln!("[pack] {dest}");
            }
            None => eprintln!("[pack] WARNING: {rel} not found — skipped"),
        }
    }
    for (rel, dest) in [
        ("docs/mercs2-ecs", "ecs_schemas"),
        ("docs/mercs2-luacd/src", "lua"),
        ("docs/mercs2-dlc-luacd/src", "lua_dlc"),
    ] {
        match repo_file(rel) {
            Some(src) => {
                let n = copy_tree(&src, &out.join(dest))?;
                eprintln!("[pack] {dest}/: {n} files");
            }
            None => eprintln!("[pack] WARNING: {rel} not found — skipped"),
        }
    }

    // 3. Manifest (provenance + counts, for the app's About/diagnostics).
    let manifest = format!(
        "{{\n  \"format\": 1,\n  \"names\": {},\n  \"sources\": [\"bone_name_candidates\", \"rainbow_table\", \"live_registry\"],\n  \"contents\": [\"names.bin\", \"live_registry_hashes.csv\", \"spawnable_templates.csv\", \"ecs_schemas/\", \"lua/\", \"lua_dlc/\"]\n}}\n",
        names.len()
    );
    std::fs::write(out.join("manifest.json"), manifest).map_err(|e| e.to_string())?;
    println!("packed workshop_data -> {}", out.display());
    Ok(())
}

/// Recursive copy; returns the number of files copied.
fn copy_tree(src: &std::path::Path, dst: &std::path::Path) -> Result<usize, String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let mut n = 0usize;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        let d = dst.join(entry.file_name());
        if p.is_dir() {
            n += copy_tree(&p, &d)?;
        } else {
            std::fs::copy(&p, &d).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}
