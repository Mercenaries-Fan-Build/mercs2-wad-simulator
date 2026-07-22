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
mod retarget;
mod shot;
mod texenc;
mod texpng;

use mercs2_engine::{mesh, wad};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |flag: &str| {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
    };

    // Probe: can wgpu get ANY adapter here (real GPU or software fallback)? Decides whether an
    // offscreen render can run in this environment at all.
    if args.iter().any(|a| a == "--gpu-check") {
        let instance = wgpu::Instance::default();
        for fb in [false, true] {
            let a = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: fb,
            }));
            match a {
                Some(a) => {
                    let i = a.get_info();
                    println!("adapter(fallback={fb}): {} type={:?} backend={:?}", i.name, i.device_type, i.backend);
                }
                None => println!("adapter(fallback={fb}): NONE"),
            }
        }
        return;
    }

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
        if let Err(e) = pack_data(&out, names_csv, &wadpath, &overlays) {
            eprintln!("--pack-data: {e}");
        }
        return;
    }

    // Headless: one TSV row per model, in ONE wad open:
    //   `hash <TAB> resolves <TAB> bones <TAB> skinned_groups <TAB> verts <TAB> tris <TAB> tex,tex,...`
    //
    // For NAME RECOVERY. A model's hash is all we have when its name was never recovered, but the
    // TEXTURES it binds are frequently named, and a texture name carries the model's own stem
    // (`pmc_veh_motorcycle_r71_dm` -> `pmc_veh_motorcycle_r71`). That turns naming an unknown model
    // from a blind 32-bit search into a handful of candidates checked against ONE hash, which is the
    // difference between a result and a pile of collisions. `--check` already computes this set per
    // model; this is the same thing for the whole catalog without re-opening the 2.5 GB wad 900 times.
    //
    // A model that does not resolve is REPORTED (`resolves=0`) rather than skipped: "no geometry in
    // any open wad" is itself the finding for a name hunt — such an asset binds no textures, so it
    // has no prior to work from and no amount of candidate generation will reach it.
    if args.iter().any(|a| a == "--dump-bindings") {
        let mut w = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let hashes: Vec<u32> = {
            let idx = index::AssetIndex::build(&w.wads, index::load_all_names(names_csv.clone()));
            idx.models.iter().map(|r| r.hash).collect()
        };
        println!("hash\tresolves\tbones\tskinned_groups\tverts\ttris\ttextures");
        let (mut ok, mut failed) = (0usize, 0usize);
        for h in hashes {
            let Ok(md) = app::load_model_data(&mut w, h) else {
                println!("0x{h:08X}\t0\t0\t0\t0\t0\t");
                failed += 1;
                continue;
            };
            let mut tex: Vec<u32> = md
                .draws
                .iter()
                .flat_map(|d| [d.diffuse, d.normal, d.specular])
                .flatten()
                .collect();
            tex.sort_unstable();
            tex.dedup();
            let list: Vec<String> = tex.iter().map(|t| format!("0x{t:08X}")).collect();
            let skinned = md.draws.iter().filter(|d| d.skinned).count();
            println!(
                "0x{h:08X}\t1\t{}\t{skinned}\t{}\t{}\t{}",
                md.skin.rig.len(),
                md.verts.len(),
                md.indices.len() / 3,
                list.join(",")
            );
            ok += 1;
        }
        eprintln!("--dump-bindings: {ok} models dumped, {failed} unresolvable");
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
        // Conformant path: keep the donor's REAL skeleton, route each imported material to its own
        // donor drawing group, and repoint that group's MTRL slots at the material's OWN textures
        // (diffuse/specular/normal-DXT5nm). So glass/lights/body each keep their distinct skin.
        let (parts, mat_images, spec_images, normal_images) =
            match extract_skel_parts(std::path::Path::new(mesh_path)) {
                Ok(p) => p,
                Err(e) => return eprintln!("--mod-new: import {mesh_path}: {e}"),
            };
        let _ = group;
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let mut paths = vec![wadpath.clone()];
        paths.extend(overlays.iter().cloned());
        match publish::publish_conformant(
            &paths, donor, donor_arg, name, parts, mat_images, spec_images, normal_images, &out,
        ) {
            Ok(r) => {
                println!("wrote {} ({} bytes)\nsha256 {}", r.path.display(), r.bytes, r.sha256);
                for (n, res) in &r.results {
                    match res {
                        Ok(s) => println!("self-test {n} (0x{hash:08X}): {s}"),
                        Err(e) => println!("self-test {n} (0x{hash:08X}): FAIL {e}"),
                    }
                }
            }
            Err(e) => eprintln!("--mod-new: {e}"),
        }
        return;
    }

    // --mod-skel <name> <donor|0xHASH> <mesh.glb> [--mod-out path]
    // Conform a NOVEL multi-part model onto a FRESH SKELETON of novel bones (one authored HIER node
    // per part) minted under a NEW hash — does NOT overwrite the donor. Large parts auto-split
    // across donor draw groups (u16). Rotor parts ride the donor's engine-spun rotor node.
    if let Some(i) = args.iter().position(|a| a == "--mod-skel") {
        let (Some(name), Some(donor_arg), Some(mesh_path)) =
            (args.get(i + 1), args.get(i + 2), args.get(i + 3))
        else {
            eprintln!("--mod-skel <name> <donor name|0xHASH> <mesh.glb>");
            return;
        };
        let donor = donor_arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(donor_arg));
        let out = get("--mod-out").map(std::path::PathBuf::from).unwrap_or_else(|| {
            std::path::Path::new(&wadpath)
                .parent()
                .map(|d| d.join("vz-mod-skel.wad"))
                .unwrap_or_else(|| std::path::PathBuf::from("vz-mod-skel.wad"))
        });
        let (parts, mat_images, spec_images, normal_images) =
            match extract_skel_parts(std::path::Path::new(mesh_path)) {
                Ok(p) => p,
                Err(e) => return eprintln!("--mod-skel: import {mesh_path}: {e}"),
            };
        println!("[mod-skel] {} parts, {} materials from {mesh_path}", parts.len(), mat_images.len());
        let mut paths = vec![wadpath.clone()];
        paths.extend(overlays.iter().cloned());
        match publish::publish_skel(
            &paths, donor, donor_arg, name, parts, mat_images, spec_images, normal_images, &out,
        ) {
            Ok(r) => {
                println!("wrote {} ({} bytes)\nsha256 {}", r.path.display(), r.bytes, r.sha256);
                for (n, res) in &r.results {
                    match res {
                        Ok(s) => println!("self-test {n}: {s}"),
                        Err(e) => println!("self-test {n}: FAIL {e}"),
                    }
                }
            }
            Err(e) => eprintln!("--mod-skel: {e}"),
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

    // Read-only diagnostic: dump a donor container's MTRL records (per-slot texture hashes) so the
    // authored slot order (diffuse/normal/specular) can be confirmed by hash-matching _dm/_nm/_sm.
    if let Some(i) = args.iter().position(|a| a == "--dump-mtrl") {
        let Some(donor_arg) = args.get(i + 1) else {
            eprintln!("--dump-mtrl <donor name|0xHASH>");
            return;
        };
        let donor = donor_arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(donor_arg));
        let mut paths = vec![wadpath.clone()];
        paths.extend(overlays.iter().cloned());
        match publish::donor_block(&paths, donor) {
            Ok(block) => {
                let magic = String::from_utf8_lossy(&block[0..4.min(block.len())]).to_string();
                let has_ucfx = block.windows(4).position(|w| w == b"UCFX");
                let has_mtrl = block.windows(4).position(|w| w == b"MTRL");
                println!("donor {donor_arg} (0x{donor:08X}): {} bytes, magic {:?}, UCFX@{:?}, MTRL@{:?}",
                    block.len(), magic, has_ucfx, has_mtrl);
                // parse from the UCFX start (block may carry a leading header)
                let ucfx_start = has_ucfx.unwrap_or(0);
                let mats = mercs2_formats::texture::parse_mtrl(&block[ucfx_start..]);
                println!("donor {donor_arg} (0x{donor:08X}): {} MTRL records", mats.len());
                for (mi, m) in mats.iter().enumerate() {
                    let slots: Vec<String> = m.textures.iter().enumerate()
                        .map(|(s, h)| format!("slot{s}=0x{h:08X}")).collect();
                    println!("  MTRL {mi}: flags=0x{:04X} texc={} [{}]", m.flags, m.textures.len(), slots.join(" "));
                }
            }
            Err(e) => eprintln!("--dump-mtrl: {e}"),
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
            Ok(im) => {
                println!(
                    "{file}: {} verts, {} tris, {} draw groups, {} textures, bbox {:?}..{:?}",
                    im.verts.len(),
                    im.indices.len() / 3,
                    im.draws.len(),
                    im.textures.len(),
                    im.stats.bbox_min,
                    im.stats.bbox_max
                );
                // Report the source rig: an unrigged import (empty skin_joints) is the
                // failure mode that silently sinks a character — the Skeleton workbench
                // has nothing to retarget. Name it here so it is caught before injection.
                if im.skin_joints.is_empty() {
                    println!("  rig: NONE (unrigged) — the Skeleton/Retarget workbench needs a skinned source");
                } else {
                    let rig = crate::retarget::SourceRig::detect(&im.skin_joints);
                    println!(
                        "  rig: {} — {} source joints (e.g. {})",
                        rig.label(),
                        im.skin_joints.len(),
                        im.skin_joints
                            .iter()
                            .take(3)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
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
    // Headless OFFSCREEN RENDER of a retargeted import to PNGs (front/side/3q) — a visual check.
    // Usage: --shot <file.glb> [--rebind-target <name>] [--shot-out <prefix>] [--shot-clip 0x..]
    if let Some(glb) = get("--shot") {
        const IDENT: [[f32; 4]; 4] =
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let target = get("--rebind-target").unwrap_or_else(|| "pmc_hum_jen_v2".into());
        let out = get("--shot-out").unwrap_or_else(|| "shot".into());
        let im = match import::import_model(std::path::Path::new(&glb)) {
            Ok(i) => i,
            Err(e) => return eprintln!("--shot: {e}"),
        };
        let thash = mercs2_formats::hash::pandemic_hash_m2(target.trim_start_matches('_'));
        let mut w = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: {e}"),
        };
        let (tnames, tpos, tparents) = app::target_bone_info(&mut w, thash);
        let tmd = match app::load_model_data(&mut w, thash) {
            Ok(m) => m,
            Err(e) => return eprintln!("--shot target {target}: {e}"),
        };
        // NON-DESTRUCTIVE retarget: keep the imported mesh + its own skeleton (bind pose, proportions,
        // gear) untouched; only RELABEL the imported bones with the target's bone-name hashes so the
        // target's clips bind, then transfer the clip's ROTATIONS onto the imported skeleton.
        let r = crate::retarget::Retarget::build_full(
            im.skin_joints.clone(), im.skin_joint_pos.clone(), im.skin_ibm.clone(), im.skin_parents.clone(),
            tnames.clone(), tpos.clone(), tparents,
        );
        let table = r.joint_table(tnames.len().max(1));
        let target_hashes: Vec<u32> = tmd.skin.rig.iter().map(|b| b.name_hash).collect();
        let rig = r.animation_rig(&table, &target_hashes);
        if rig.is_empty() {
            return eprintln!("--shot: import carried no skeleton (need a rigged glTF/glb)");
        }
        // Mesh vertices are used AS AUTHORED; joints keep indexing the import's own bones.
        let mut sv = Vec::with_capacity(im.verts.len());
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &im.verts {
            for k in 0..3 {
                lo[k] = lo[k].min(v.pos[k]);
                hi[k] = hi[k].max(v.pos[k]);
            }
            sv.push(shot::SV {
                pos: v.pos,
                _p0: 0.0,
                normal: v.normal,
                _p1: 0.0,
                uv: v.uv,
                _p2: [0.0, 0.0],
                joints: [v.joints[0] as u32, v.joints[1] as u32, v.joints[2] as u32, v.joints[3] as u32],
                weights: [
                    v.weights[0] as f32 / 255.0, v.weights[1] as f32 / 255.0,
                    v.weights[2] as f32 / 255.0, v.weights[3] as f32 / 255.0,
                ],
            });
        }
        // Palette indexed by the IMPORT's own bones. Bind pose (no clip) = identity. With a clip, drive
        // the import's skeleton via CROSS-SKELETON retarget: the clip binds to JEN's rig (source), and
        // its per-bone world rotation deltas are transferred onto the import's bind pose.
        let bone_count = rig.len();
        let jen_hashes: Vec<u32> = tmd.skin.rig.iter().map(|b| b.name_hash).collect();
        let palette: Vec<[[f32; 4]; 4]> = match get("--shot-clip")
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        {
            Some(ch) => match w.clip_for_rig(&jen_hashes, ch) {
                Some(ca) => {
                    let sample = ca.clip.sample_local(ca.clip.duration * 0.35);
                    mercs2_engine::pose::havok_palette_retarget_cross(
                        &rig, &tmd.skin.rig, &table, &sample, &ca.track_to_hier, ca.num_transform_tracks,
                    )
                }
                None => {
                    eprintln!("--shot: clip 0x{ch:08X} does not bind the target rig; rendering bind pose");
                    vec![IDENT; bone_count]
                }
            },
            None => vec![IDENT; bone_count],
        };
        // One untextured range: --shot previews a raw glTF import, which carries no WAD material.
        let draws = vec![shot::DrawTex { index_start: 0, index_count: im.indices.len() as u32, diffuse: None }];
        shot::render(&sv, &im.indices, &palette, (lo, hi), &out, &draws);
        return;
    }

    // Headless: render a WAD ASSET (by name/hash, overlay-aware) to PNGs on the engine renderer.
    // Loads the full LOD-block chain (same path as --check/preview), then offscreen-renders it static
    // (identity palette — model-space bake). Writes `<out>_front/_side/_threeq.png`.
    // Usage: --render <name|0xHASH> [--render-out prefix]
    if let Some(arg) = get("--render") {
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        let out = get("--render-out").unwrap_or_else(|| "render".into());
        let mut w = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let md = match app::load_model_data(&mut w, hash) {
            Ok(md) => md,
            Err(e) => return eprintln!("--render '{arg}' (0x{hash:08X}): {e}"),
        };
        let hier: Vec<u32> = md.skin.rig.iter().map(|b| b.name_hash).collect();

        // List the clips this model's rig can bind, so a clip can be chosen by name rather than
        // guessed at. Names come from the same index the GUI uses.
        if args.iter().any(|a| a == "--render-clips") {
            let idx = index::AssetIndex::build(&w.wads, index::load_all_names(names_csv.clone()));
            let (clips, names) = app::clips_for_export(&mut w, &arg, &md.skin.rig, &idx);
            println!("--render-clips 0x{hash:08X}: {} clip(s) bind this rig", clips.len());
            for c in &clips {
                let bound = c.track_to_hier.iter().filter(|t| t.is_some()).count();
                println!(
                    "  0x{:08X}  {:<40} {:>3}/{:<3} tracks bound  {:.2}s",
                    c.name_hash,
                    names.get(&c.name_hash).cloned().unwrap_or_default(),
                    bound,
                    c.num_transform_tracks,
                    c.clip.duration
                );
            }
            return;
        }

        // Real skinning: the model's own BLENDINDICES/BLENDWEIGHT, not a single identity bone.
        // Without this the render cannot show a skinning defect at all, which is the whole point.
        let sv: Vec<shot::SV> = md
            .verts
            .iter()
            .map(|v| shot::SV {
                pos: v.pos,
                _p0: 0.0,
                normal: v.normal,
                _p1: 0.0,
                uv: v.uv,
                _p2: [0.0, 0.0],
                joints: [v.joints[0] as u32, v.joints[1] as u32, v.joints[2] as u32, v.joints[3] as u32],
                weights: [
                    v.weights[0] as f32 / 255.0,
                    v.weights[1] as f32 / 255.0,
                    v.weights[2] as f32 / 255.0,
                    v.weights[3] as f32 / 255.0,
                ],
            })
            .collect();

        // IN-PLACE posing, the same call app.rs uses for a WAD model. Correct here because this
        // geometry already lives in its own rig's bind space -- unlike --shot, which previews a
        // foreign import and must retarget across skeletons. No clip = bind pose (identity palette),
        // which is the "before animation" view.
        const IDENT: [[f32; 4]; 4] =
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let t_frac: f32 = get("--render-t").and_then(|s| s.parse().ok()).unwrap_or(0.35);
        let palette: Vec<[[f32; 4]; 4]> = match get("--render-clip")
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        {
            Some(ch) => match w.clip_for_rig(&hier, ch) {
                Some(ca) => {
                    let bound = ca.track_to_hier.iter().filter(|t| t.is_some()).count();
                    println!(
                        "  clip 0x{ch:08X}: {bound}/{} tracks bound, {:.2}s, sampling t={t_frac}",
                        ca.num_transform_tracks, ca.clip.duration
                    );
                    if bound == 0 {
                        eprintln!("  WARNING: clip binds NO tracks on this rig -- this is a bind pose, not an animation");
                    }
                    let sample = ca.clip.sample_local(ca.clip.duration * t_frac);
                    mercs2_engine::pose::havok_palette_in_place(
                        &md.skin.rig, &sample, &ca.track_to_hier, ca.num_transform_tracks,
                    )
                }
                None => {
                    eprintln!("--render: clip 0x{ch:08X} not found for this rig; rendering bind pose");
                    vec![IDENT; md.skin.rig.len().max(1)]
                }
            },
            None => vec![IDENT; md.skin.rig.len().max(1)],
        };

        // THE ENGINE'S DRAW GATE. The mesh builder emits a range for every PRMG group it can
        // de-stripe, but the engine only draws a group whose PRMT draw count (+8) is non-zero --
        // and that is exactly the field injection zeroes to neutralise the groups it did not fill.
        // Without this filter the preview renders the DONOR's leftover head, hair and gear inside
        // the import and disagrees with the game, which makes it worse than useless: it would show
        // a defect that is not there and hide one that is.
        let gate: std::collections::HashSet<usize> = match w.extract_container(hash) {
            Ok(c) => match mercs2_formats::model_inject::group_draw_report(&c) {
                Ok(rep) => rep.iter().filter(|r| r.1 > 0 && r.2 > 0).map(|r| r.0).collect(),
                Err(e) => {
                    eprintln!("--render: draw report failed ({e}); rendering every group");
                    md.draws.iter().map(|d| d.group_index).collect()
                }
            },
            Err(e) => {
                eprintln!("--render: container unavailable ({e}); rendering every group");
                md.draws.iter().map(|d| d.group_index).collect()
            }
        };

        // One range per draw group, each binding its own diffuse. Decoding here (not in `shot`)
        // reuses texpng::decode_bc, which already resolves a streamed texture's resident mip tail.
        let draws: Vec<shot::DrawTex> = md
            .draws
            .iter()
            // Clause 2 as well as clause 1. The engine tests lod_mask against the object's
            // view_state with an ANY-BIT overlap, and load_model_data asks for tier 0x01, so a host
            // group in the 0x02/0x04/0x08 tier is simply never drawn. Missing this shipped a build
            // whose torso and arms were invisible in game while rendering perfectly here: the host
            // group had been chosen for its material, and nothing checked its tier.
            .filter(|d| gate.contains(&d.group_index) && (d.lod_mask & 0x01) != 0)
            .map(|d| shot::DrawTex {
                index_start: d.index_start,
                index_count: d.index_count,
                diffuse: d
                    .diffuse
                    .and_then(|h| md.textures.get(&h))
                    .map(|td| texpng::decode_bc(td)),
            })
            .collect();
        let untextured = draws.iter().filter(|d| d.diffuse.is_none()).count();
        println!(
            "--render 0x{hash:08X}: {} verts / {} tris / {} draws ({untextured} untextured), {} bones, bbox {:?}..{:?}",
            md.verts.len(),
            md.indices.len() / 3,
            md.draws.len(),
            md.skin.rig.len(),
            md.stats.bbox_min,
            md.stats.bbox_max
        );
        // Ranges with a zero index count are the injector's NEUTRALISED groups. Reporting them
        // separately is what proves this preview draws what the engine draws: if the neutralised
        // geometry were rendered here, the picture would show the donor's leftovers and quietly
        // disagree with the game.
        let live: u32 = draws.iter().map(|d| d.index_count).sum();
        println!(
            "    drawing {} of {} groups (PRMT gate), {live} of {} indices",
            draws.len(),
            md.draws.len(),
            md.indices.len()
        );
        for d in md.draws.iter().filter(|d| gate.contains(&d.group_index) && d.index_count > 0) {
            if d.lod_mask & 0x01 == 0 {
                println!(
                    "    group {:>2}  {:>6} idx  lod 0x{:02X}  <-- NOT IN TIER 0x01, invisible in game",
                    d.group_index, d.index_count, d.lod_mask
                );
                continue;
            }
            // lod_mask and node are draw-gate clauses 2 and 3: the engine tests lod_mask against
            // the object's view_state (ANY-bit overlap) and `node` against its node-enable table
            // (negative = no node = always visible). A host group that fails either is invisible in
            // game while rendering fine here, which is precisely how a costume can lose its torso.
            println!(
                "    group {:>2}  {:>6} idx  lod 0x{:02X}  node {:>4}  seg {:>2}  diffuse {}  {}",
                d.group_index,
                d.index_count,
                d.lod_mask,
                d.node,
                d.seg_id,
                d.diffuse.map(|h| format!("0x{h:08X}")).unwrap_or_else(|| "-".into()),
                match d.diffuse {
                    Some(h) if md.textures.contains_key(&h) => "loaded",
                    Some(_) => "MISSING from block",
                    None => "untextured",
                }
            );
        }
        shot::render(&sv, &md.indices, &palette, (md.stats.bbox_min, md.stats.bbox_max), &out, &draws);
        return;
    }

    // Headless: numerically diagnose the retarget REBIND. Rebinds the import onto the target both ways
    // (world_bind as-is vs transposed) and reports, per key body part, where its verts LAND vs where
    // the target bone actually is — so we can see which convention is correct instead of eyeballing.
    // Usage: --rebind-check <file.glb> [--rebind-target <name>]
    if let Some(file) = get("--rebind-check") {
        let target = get("--rebind-target").unwrap_or_else(|| "pmc_hum_jen_v2".into());
        let im = match import::import_model(std::path::Path::new(&file)) {
            Ok(i) => i,
            Err(e) => return eprintln!("--rebind-check: {e}"),
        };
        let thash = mercs2_formats::hash::pandemic_hash_m2(target.trim_start_matches('_'));
        let mut w = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let (tnames, tpos, tparents) = app::target_bone_info(&mut w, thash);
        let tmd = match app::load_model_data(&mut w, thash) {
            Ok(m) => m,
            Err(e) => return eprintln!("--rebind-check target {target}: {e}"),
        };
        let world_bind: Vec<[[f32; 4]; 4]> = tmd.skin.rig.iter().map(|b| b.world_bind).collect();
        let r = crate::retarget::Retarget::build_full(
            im.skin_joints.clone(),
            im.skin_joint_pos.clone(),
            im.skin_ibm.clone(),
            im.skin_parents.clone(),
            tnames.clone(),
            tpos.clone(),
            tparents,
        );
        let table = r.joint_table(tnames.len().max(1));
        // Unit scale source→target from the bind-pose Y extents (inches→metres for a CoD import).
        let yext = |ps: &[[f32; 3]]| {
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for p in ps {
                lo = lo.min(p[1]);
                hi = hi.max(p[1]);
            }
            (hi - lo).max(1e-3)
        };
        let uscale = yext(&tpos) / yext(&im.skin_joint_pos);
        println!("source→target unit scale = {uscale:.5}");
        // Dump the mapping for every hip/pelvis/leg/gear source bone WITH the source and target X sign
        // (side): a mismatch (src +X → tgt −X) is a left/right swap for that bone.
        println!("--- hip/pelvis/gear region mapping (src.x sign vs tgt.x sign) ---");
        for (si, name) in im.skin_joints.iter().enumerate() {
            let ln = name.to_ascii_lowercase();
            if !(ln.contains("hip") || ln.contains("mainroot") || ln.contains("origin")
                || ln.contains("sling") || ln.contains("cosmetic") || ln.contains("holster")
                || ln.contains("knee") || ln.contains("ball"))
            {
                continue;
            }
            let ti = table.get(si).copied().unwrap_or(0);
            let sz = im.skin_joint_pos.get(si).map(|p| p[2]).unwrap_or(0.0); // source side = Z (left=-Z)
            let tx = tpos.get(ti).map(|p| p[0]).unwrap_or(0.0); // target side = X (left=+X)
            let ssrc = if sz < -0.5 { "L" } else if sz > 0.5 { "R" } else { "mid" };
            let stgt = if tx > 0.03 { "L" } else if tx < -0.03 { "R" } else { "mid" };
            let swap = ssrc != stgt && ssrc != "mid" && stgt != "mid";
            println!(
                "  {name:26} src.z={sz:+.2} src-side={ssrc} -> {:20} tgt.x={tx:+.2} tgt-side={stgt} {}",
                tnames.get(ti).cloned().unwrap_or_default(),
                if swap { "*** SIDE SWAP ***" } else { "" }
            );
        }
        // Key source bones and the region they represent — BOTH sides, to catch a left/right swap.
        let probes = [
            "j_mainroot", "j_hip_le", "j_hip_ri", "j_knee_le", "j_knee_ri", "j_wrist_le",
            "j_wrist_ri", "j_shoulder_le", "j_shoulder_ri", "j_hipholster_ri", "j_ankle_le",
            "j_ankle_ri",
        ];
        // Variant A = the world-space rebind the workshop now ships (`Retarget::rebind_matrices`,
        // position snap + shortest-arc direction). Variant B = the legacy `TargetBind·SourceInvBind`
        // form, kept side-by-side so the OFF→MATCH and TWIST collapse between them is visible.
        let tgt_bind_asis: Vec<glam::Mat4> = world_bind.iter().map(glam::Mat4::from_cols_array_2d).collect();
        for (label, legacy) in
            [("A: world-space (new)", false), ("B: legacy TargetBind\u{00B7}SourceInvBind", true)]
        {
            let scale_m = glam::Mat4::from_scale(glam::Vec3::splat(uscale));
            let rebind: Vec<glam::Mat4> = if legacy {
                (0..im.skin_ibm.len())
                    .map(|s| {
                        let t = table.get(s).copied().unwrap_or(0).min(tgt_bind_asis.len().saturating_sub(1));
                        tgt_bind_asis.get(t).copied().unwrap_or(glam::Mat4::IDENTITY)
                            * scale_m
                            * glam::Mat4::from_cols_array_2d(&im.skin_ibm[s])
                    })
                    .collect()
            } else {
                r.rebind_matrices(&table, &tgt_bind_asis)
            };
            let reb = |v: &mesh::Vertex| -> glam::Vec3 {
                let (mut np, mut wsum) = (glam::Vec3::ZERO, 0.0f32);
                for k in 0..4 {
                    let wt = v.weights[k] as f32 / 255.0;
                    if wt <= 0.0 {
                        continue;
                    }
                    let rb = rebind.get(v.joints[k] as usize).copied().unwrap_or(glam::Mat4::IDENTITY);
                    np += wt * rb.transform_point3(glam::Vec3::from(v.pos));
                    wsum += wt;
                }
                if wsum > 1e-6 {
                    np / wsum
                } else {
                    glam::Vec3::from(v.pos)
                }
            };
            // Centroid of the verts whose DOMINANT bone is source joint `si`, in rebound space.
            let src_centroid = |si: usize| -> Option<glam::Vec3> {
                let (mut c, mut n) = (glam::Vec3::ZERO, 0.0f32);
                for v in &im.verts {
                    let dom = (0..4).max_by_key(|&k| v.weights[k]).unwrap();
                    if v.joints[dom] as usize == si && v.weights[dom] > 0 {
                        c += reb(v);
                        n += 1.0;
                    }
                }
                (n > 0.0).then(|| c / n)
            };
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &im.verts {
                let p = reb(v);
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            // Determinant sign of a representative rebind matrix — negative = a reflection (mirror),
            // which flips facing (Z) and inverts normals (parts render as invisible backfaces).
            let det = rebind.get(1).map(|m| m.determinant()).unwrap_or(0.0);
            println!(
                "\n[{label}] rebound bbox y {:.2}..{:.2} (height {:.2} m)  rebind det={:.4} {}",
                lo[1], hi[1], hi[1] - lo[1], det,
                if det < 0.0 { "*** REFLECTION (mirror/flip) ***" } else { "" }
            );
            for pname in probes {
                let Some(si) = im.skin_joints.iter().position(|n| n == pname) else { continue };
                let ti = table.get(si).copied().unwrap_or(0);
                let tp = tpos.get(ti).copied().unwrap_or([0.0; 3]);
                let Some(c) = src_centroid(si) else { continue };
                let ok = |a: f32, b: f32| (a - b).abs() < 0.20;
                // TWIST: the position check above can MATCH while the limb is rotationally twisted. Compare
                // the rebound limb direction (this bone's centroid → its mapped child's centroid) against
                // the target bone's bind direction (bone → its target child). A large angle = a twist the
                // position probe can't see — the crumpled-hand failure mode.
                let child_src = r
                    .source_parents
                    .iter()
                    .enumerate()
                    .find(|&(c, &p)| p == si as i32 && table.get(c).copied() != Some(ti))
                    .map(|(c, _)| c);
                let twist = child_src.and_then(|cs| {
                    let cc = src_centroid(cs)?;
                    let tc = table.get(cs).copied().unwrap_or(ti);
                    let limb = (cc - c).normalize_or_zero();
                    let bone = (glam::Vec3::from(tpos.get(tc).copied().unwrap_or(tp)) - glam::Vec3::from(tp))
                        .normalize_or_zero();
                    if limb.length_squared() > 0.0 && bone.length_squared() > 0.0 {
                        Some(limb.dot(bone).clamp(-1.0, 1.0).acos().to_degrees())
                    } else {
                        None
                    }
                });
                println!(
                    "  {pname:14} -> {:18} land ({:+.2},{:+.2},{:+.2}) bone ({:+.2},{:+.2},{:+.2}) {} {}",
                    tnames.get(ti).cloned().unwrap_or_default(),
                    c.x, c.y, c.z, tp[0], tp[1], tp[2],
                    if ok(c.x, tp[0]) && ok(c.y, tp[1]) && ok(c.z, tp[2]) { "MATCH" } else { "OFF" },
                    match twist {
                        Some(a) => format!("TWIST {a:5.1}\u{00B0}{}", if a > 30.0 { " ***" } else { "" }),
                        None => String::new(),
                    }
                );
            }
        }

        // ── POSED-FRAME CHECK ── apply a real clip to the shipping (world-space) rebound mesh and see
        // whether each body part still tracks its (posed) bone. Detects the animation-time bug the
        // static bind check can't: if a limb's verts drift far from its posed bone, the deformation is wrong.
        let tgt_bind: Vec<glam::Mat4> = world_bind.iter().map(glam::Mat4::from_cols_array_2d).collect();
        let rebind = r.rebind_matrices(&table, &tgt_bind);
        let rebound: Vec<glam::Vec3> = im
            .verts
            .iter()
            .map(|v| {
                let (mut np, mut wsum) = (glam::Vec3::ZERO, 0.0f32);
                for k in 0..4 {
                    let wt = v.weights[k] as f32 / 255.0;
                    if wt <= 0.0 {
                        continue;
                    }
                    np += wt
                        * rebind
                            .get(v.joints[k] as usize)
                            .copied()
                            .unwrap_or(glam::Mat4::IDENTITY)
                            .transform_point3(glam::Vec3::from(v.pos));
                    wsum += wt;
                }
                if wsum > 1e-6 {
                    np / wsum
                } else {
                    glam::Vec3::from(v.pos)
                }
            })
            .collect();
        // Optionally test a SPECIFIC clip by hash (--rebind-clip 0x...) loaded via the CHARACTER path
        // (clip_for_rig) — that's where the workshop's jennifer/pistol clips come from — else a normal
        // generic full-body clip.
        let want_clip = get("--rebind-clip")
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());
        let hier: Vec<u32> = tmd.skin.rig.iter().map(|b| b.name_hash).collect();
        let owned = want_clip.and_then(|h| w.clip_for_rig(&hier, h));
        let clips = w.clips_for_model(&tmd.skin.rig);
        let Some(ca) = owned.as_ref().or_else(|| {
            clips
                .iter()
                .filter(|c| c.num_transform_tracks >= 20 && c.num_transform_tracks <= 70)
                .max_by_key(|c| c.num_transform_tracks)
                .or_else(|| clips.iter().max_by_key(|c| c.num_transform_tracks))
        }) else {
            println!("\n(no clips bind to this rig — can't run posed check)");
            return;
        };
        let sample = ca.clip.sample_local(ca.clip.duration * 0.35);
        let palette = mercs2_engine::pose::havok_palette(
            &tmd.skin.rig,
            &sample,
            &ca.track_to_hier,
            ca.num_transform_tracks,
        );
        let skin_of = |t: usize| glam::Mat4::from_cols_array_2d(&palette[t.min(palette.len() - 1)]);
        // Posed bone position = the bone origin pushed through its skin matrix.
        let posed_bone = |t: usize| skin_of(t).transform_point3(glam::Vec3::from(tpos[t.min(tpos.len() - 1)]));
        let skinned = |i: usize| -> glam::Vec3 {
            let v = &im.verts[i];
            let (mut acc, mut wsum) = (glam::Vec3::ZERO, 0.0f32);
            for k in 0..4 {
                let wt = v.weights[k] as f32 / 255.0;
                if wt <= 0.0 {
                    continue;
                }
                let t = table.get(v.joints[k] as usize).copied().unwrap_or(0);
                acc += wt * skin_of(t).transform_point3(rebound[i]);
                wsum += wt;
            }
            if wsum > 1e-6 {
                acc / wsum
            } else {
                rebound[i]
            }
        };
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for i in 0..im.verts.len() {
            let p = skinned(i);
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        println!(
            "\n== POSED FRAME (clip 0x{:08X}, {} tracks, t={:.2}s) ==",
            ca.name_hash,
            ca.num_transform_tracks,
            ca.clip.duration * 0.35
        );
        println!(
            "posed skinned bbox x {:.2}..{:.2} y {:.2}..{:.2} z {:.2}..{:.2}",
            lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
        );
        for pname in probes {
            let Some(si) = im.skin_joints.iter().position(|n| n == pname) else { continue };
            let ti = table.get(si).copied().unwrap_or(0);
            // Dominant verts of this source bone.
            let idxs: Vec<usize> = (0..im.verts.len())
                .filter(|&i| {
                    let v = &im.verts[i];
                    let dom = (0..4).max_by_key(|&k| v.weights[k]).unwrap();
                    v.joints[dom] as usize == si && v.weights[dom] > 0
                })
                .collect();
            if idxs.is_empty() {
                continue;
            }
            // Spread (RMS distance from centroid) at BIND (rebound) vs POSED (skinned). A rigid limb
            // keeps its spread → ratio≈1; a twist/collapse/balloon shows ratio far from 1 even when the
            // CENTROID still tracks the bone. This is the distortion the position-only check missed.
            let mean = |f: &dyn Fn(usize) -> glam::Vec3| {
                let s: glam::Vec3 = idxs.iter().map(|&i| f(i)).sum();
                s / idxs.len() as f32
            };
            let rms = |f: &dyn Fn(usize) -> glam::Vec3, c: glam::Vec3| {
                (idxs.iter().map(|&i| (f(i) - c).length_squared()).sum::<f32>() / idxs.len() as f32).sqrt()
            };
            let bind_f: &dyn Fn(usize) -> glam::Vec3 = &|i| rebound[i];
            let pose_f: &dyn Fn(usize) -> glam::Vec3 = &|i| skinned(i);
            let bc = mean(bind_f);
            let pc = mean(pose_f);
            let brms = rms(bind_f, bc);
            let prms = rms(pose_f, pc);
            let ratio = if brms > 1e-4 { prms / brms } else { 0.0 };
            let b = posed_bone(ti);
            let d = (pc - b).length();
            println!(
                "  {pname:14} posed@({:+.2},{:+.2},{:+.2}) dist={:.2} {}  spread {:.3}->{:.3} ratio={:.2} {}",
                pc.x, pc.y, pc.z,
                d,
                if d < 0.25 { "tracks" } else { "DRIFT" },
                brms,
                prms,
                ratio,
                if ratio > 1.6 || ratio < 0.6 { "*** DISTORTED ***" } else { "rigid-ok" }
            );
        }

        // ── JEN NATIVE CONTROL ── pose Jen's OWN mesh with the SAME clip and same metric. If Jen's
        // spread ratios match Roze's, the retarget deforms identically to native (→ any mangling is the
        // clip, not the retarget). If Jen stays rigid where Roze distorts, the retarget is the culprit.
        println!("-- JEN NATIVE (control, same clip) --");
        let jskin = |i: usize| -> glam::Vec3 {
            let v = &tmd.verts[i];
            let (mut acc, mut wsum) = (glam::Vec3::ZERO, 0.0f32);
            for k in 0..4 {
                let wt = v.weights[k] as f32 / 255.0;
                if wt <= 0.0 {
                    continue;
                }
                acc += wt * skin_of(v.joints[k] as usize).transform_point3(glam::Vec3::from(v.pos));
                wsum += wt;
            }
            if wsum > 1e-6 {
                acc / wsum
            } else {
                glam::Vec3::from(v.pos)
            }
        };
        for pname in probes {
            let Some(si) = im.skin_joints.iter().position(|n| n == pname) else { continue };
            let ti = table.get(si).copied().unwrap_or(0);
            let idxs: Vec<usize> = (0..tmd.verts.len())
                .filter(|&i| {
                    let v = &tmd.verts[i];
                    let dom = (0..4).max_by_key(|&k| v.weights[k]).unwrap();
                    v.joints[dom] as usize == ti && v.weights[dom] > 0
                })
                .collect();
            if idxs.is_empty() {
                continue;
            }
            let mean = |f: &dyn Fn(usize) -> glam::Vec3| {
                idxs.iter().map(|&i| f(i)).sum::<glam::Vec3>() / idxs.len() as f32
            };
            let rms = |f: &dyn Fn(usize) -> glam::Vec3, c: glam::Vec3| {
                (idxs.iter().map(|&i| (f(i) - c).length_squared()).sum::<f32>() / idxs.len() as f32).sqrt()
            };
            let bind_f: &dyn Fn(usize) -> glam::Vec3 = &|i| glam::Vec3::from(tmd.verts[i].pos);
            let pose_f: &dyn Fn(usize) -> glam::Vec3 = &jskin;
            let brms = rms(bind_f, mean(bind_f));
            let prms = rms(pose_f, mean(pose_f));
            let ratio = if brms > 1e-4 { prms / brms } else { 0.0 };
            println!(
                "  {:18} spread bind={:.3} posed={:.3} ratio={:.2} {}",
                tnames.get(ti).cloned().unwrap_or_default(),
                brms,
                prms,
                ratio,
                if ratio > 1.6 || ratio < 0.6 { "*** DISTORTED ***" } else { "rigid-ok" }
            );
        }
        return;
    }

    // Headless: dump a model's HIER bone names in index order (the strings the retarget maps onto).
    if let Some(arg) = get("--bones") {
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        let mut w = match app::WadStack::open(&wadpath, &overlays) {
            Ok(s) => s,
            Err(e) => return eprintln!("workshop: cannot open {wadpath}: {e}"),
        };
        let (names, pos, _parents) = app::target_bone_info(&mut w, hash);
        println!("{arg} (0x{hash:08X}): {} bones", names.len());
        for (i, (n, p)) in names.iter().zip(pos.iter()).enumerate() {
            println!("{i:3} {n}  [{:.3}, {:.3}, {:.3}]", p[0], p[1], p[2]);
        }
        return;
    }

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

/// Every hash `names.bin` is actually asked to resolve: ASET catalog assets, animgroup clip names, and
/// Extract a GLB into per-primitive parts (world-space geometry baked from the node transforms) for
/// a fresh-skeleton inject. Each primitive → one `SkelRawPart`; a primitive whose material name (or
/// its owning node's name) contains "rotor" is flagged to ride the donor's engine-spun rotor node.
/// Materials all point at donor slot 0 for now (real skins are a later pass).
fn extract_skel_parts(
    path: &std::path::Path,
) -> Result<
    (
        Vec<publish::SkelRawPart>,
        Vec<Option<(u32, u32, Vec<u8>)>>, // diffuse (slot 0)
        Vec<Option<(u32, u32, Vec<u8>)>>, // specular (slot 1)
        Vec<Option<(u32, u32, Vec<u8>)>>, // normal (slot 2)
    ),
    String,
> {
    use mercs2_formats::model_inject::ExternalMesh;
    let (doc, buffers, images) =
        gltf::import(path).map_err(|e| format!("gltf import: {e}"))?;
    let buf = |b: gltf::Buffer| buffers.get(b.index()).map(|d| &d.0[..]);

    // Per-material base-colour image as straight RGBA8 (index by glTF material index). None = the
    // material carries no diffuse texture (keep the donor's skin for parts using it).
    let to_rgba = |img: &gltf::image::Data| -> Option<(u32, u32, Vec<u8>)> {
        use gltf::image::Format::*;
        let (w, h, p) = (img.width, img.height, &img.pixels);
        let rgba: Vec<u8> = match img.format {
            R8G8B8A8 => p.clone(),
            R8G8B8 => p.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            R8 => p.iter().flat_map(|&v| [v, v, v, 255]).collect(),
            R8G8 => p.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
            _ => return None, // 16-bit / float formats: skip (rare for game skins)
        };
        Some((w, h, rgba))
    };
    let mat_count = doc.materials().len().max(1);
    // slot 0 = diffuse (baseColor), slot 1 = specular (metallicRoughness / JC2 _mpm),
    // slot 2 = normal (normalTexture / JC2 _nrm). Empirically-confirmed MTRL slot order.
    let mut mat_images: Vec<Option<(u32, u32, Vec<u8>)>> = vec![None; mat_count];
    let mut spec_images: Vec<Option<(u32, u32, Vec<u8>)>> = vec![None; mat_count];
    let mut normal_images: Vec<Option<(u32, u32, Vec<u8>)>> = vec![None; mat_count];
    for m in doc.materials() {
        let Some(idx) = m.index() else { continue };
        if let Some(t) = m.pbr_metallic_roughness().base_color_texture() {
            if let Some(img) = images.get(t.texture().source().index()) {
                mat_images[idx] = to_rgba(img);
            }
        }
        if let Some(t) = m.pbr_metallic_roughness().metallic_roughness_texture() {
            if let Some(img) = images.get(t.texture().source().index()) {
                spec_images[idx] = to_rgba(img);
            }
        }
        if let Some(t) = m.normal_texture() {
            if let Some(img) = images.get(t.texture().source().index()) {
                normal_images[idx] = to_rgba(img);
            }
        }
    }

    // column-major 4x4 multiply (glTF), point/dir transforms.
    fn mm(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
        let mut o = [[0f32; 4]; 4];
        for c in 0..4 {
            for r in 0..4 {
                for k in 0..4 {
                    o[c][r] += a[k][r] * b[c][k];
                }
            }
        }
        o
    }
    fn mp(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
        [
            m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
            m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
            m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
        ]
    }
    fn md(m: &[[f32; 4]; 4], v: [f32; 3]) -> [f32; 3] {
        let o = [
            m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2],
            m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2],
            m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2],
        ];
        let l = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt().max(1e-8);
        [o[0] / l, o[1] / l, o[2] / l]
    }
    const IDENT: [[f32; 4]; 4] =
        [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("no scene")?;
    let mut parts = Vec::new();
    let mut stack: Vec<(gltf::Node, [[f32; 4]; 4])> =
        scene.nodes().map(|n| (n, IDENT)).collect();
    while let Some((node, parent)) = stack.pop() {
        let world = mm(&parent, &node.transform().matrix());
        for c in node.children() {
            stack.push((c, world));
        }
        let node_name = node.name().unwrap_or("").to_ascii_lowercase();
        let Some(mesh) = node.mesh() else { continue };
        for prim in mesh.primitives() {
            let reader = prim.reader(buf);
            let Some(pos) = reader.read_positions() else { continue };
            let positions: Vec<[f32; 3]> = pos.map(|p| mp(&world, p)).collect();
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|it| it.map(|n| md(&world, n)).collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|tc| tc.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
            let idx: Vec<u32> = match reader.read_indices() {
                Some(ind) => ind.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            let tris: Vec<[u32; 3]> = idx.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            if positions.is_empty() || tris.is_empty() {
                continue;
            }
            let mat = prim.material();
            let mat_name = mat.name().unwrap_or("").to_ascii_lowercase();
            let is_rotor = mat_name.contains("rotor") || node_name.contains("rotor");
            let label = node
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("mat{}", mat.index().unwrap_or(0)));
            parts.push(publish::SkelRawPart {
                label,
                mesh: ExternalMesh {
                    positions,
                    normals,
                    uvs,
                    tris,
                    joints: Vec::new(),
                    weights: Vec::new(),
                },
                is_rotor,
                material_index: mat.index().unwrap_or(0) as u32,
            });
        }
    }
    if parts.is_empty() {
        return Err("no mesh primitives in GLB".into());
    }
    Ok((parts, mat_images, spec_images, normal_images))
}

/// HIER node names (bone/hardpoint). The rainbow table names ~972k hashes but only these ~tens-of-k are
/// ever displayed, so intersecting against this set drops the speculative-candidate bulk. Returns an
/// EMPTY set if the WAD can't be opened (released binary with no game data) — the caller then skips
/// trimming rather than writing an empty pack.
fn referenced_hashes(wadpath: &str, overlays: &[String]) -> std::collections::HashSet<u32> {
    use std::collections::HashSet;
    // Scan the WHOLE open stack, not just the base. A patch WAD carries its own assets (260 models /
    // 1,259 textures in the retail vz-patch), and a hash that lives ONLY there was previously absent
    // from `referenced`, so the trim below silently deleted its name — the workshop could never show
    // a name for any patch/DLC-only asset no matter how many were recovered, and it failed quietly:
    // the name was in the corpus, hash-verified, and just never appeared.
    let mut set: HashSet<u32> = HashSet::new();
    for p in std::iter::once(wadpath).chain(overlays.iter().map(|s| s.as_str())) {
        set.extend(referenced_hashes_one(p));
    }
    set
}

/// The per-WAD scan behind [`referenced_hashes`].
fn referenced_hashes_one(wadpath: &str) -> std::collections::HashSet<u32> {
    use mercs2_engine::wad;
    use std::collections::{BTreeMap, HashSet};
    let mut set: HashSet<u32> = HashSet::new();
    let Ok(mut w) = wad::open(wadpath) else {
        eprintln!("[pack] no WAD at {wadpath} — writing UNTRIMMED names.bin");
        return set;
    };
    for (asset_hash, _type, _prim) in wad::all_asets(&w) {
        set.insert(asset_hash);
    }
    // HIER node hashes: sweep models grouped by block so each block decompresses once.
    let mut by_block: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for (hash, block) in wad::model_list_all(&w) {
        by_block.entry(block).or_default().push(hash);
    }
    for (block, models) in by_block {
        let Ok(dec) = wad::decompress_block_index(&mut w, block) else { continue };
        for m in models {
            // Fast path from the decompressed block; fall back to full extraction for multi-block
            // models model_span_in can't resolve (e.g. al_veh_boat_destroyer).
            let mut hier = wad::model_span_in(&dec, m)
                .map(|c| mercs2_formats::orchestrator::parse_hier(&c))
                .unwrap_or_default();
            if hier.is_empty() {
                if let Ok(c) = wad::extract_container(&mut w, m) {
                    hier = mercs2_formats::orchestrator::parse_hier(&c);
                }
            }
            for n in hier {
                set.insert(n.hash);
            }
        }
    }
    // animgroup clip name-hashes + the per-track BONE hashes (catches anim-only bones like bone_rotor
    // / bone_yaw_radar that live in no mesh HIER) + any serialized skeleton bones.
    for blk in wad::animgroup_blocks(&w) {
        let Ok(dec) = wad::decompress_block_index(&mut w, blk) else { continue };
        if let Ok(ag) = mercs2_formats::animgroup::parse_animgroup(&dec) {
            if let Some(sk) = &ag.skeleton {
                set.extend(sk.bone_name_hashes.iter().copied());
            }
            for c in &ag.clips {
                set.insert(c.name_hash);
                set.extend(c.binding.track_to_bone_hash.iter().copied());
            }
        }
    }
    set
}

/// `--pack-data <dir>`: assemble the reference bundle. Sources are the repo corpora (found by
/// the same walk-up the app's fallback path uses); output is a portable directory.
/// The committed production lookup: `data/production_names.json` (curated, hash-verified node/asset
/// names), found by walking up from the CWD and from the exe. This is the authoritative, bundleable
/// name source — it lives in THIS repo, so it needs neither the parent-repo rainbow table nor the WAD.
fn load_production_names() -> Option<std::collections::HashMap<u32, String>> {
    let mut roots = vec![std::env::current_dir().ok()?];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            roots.push(d.to_path_buf());
        }
    }
    for root in roots {
        let mut dir = root.as_path();
        loop {
            let cand = dir.join("data/production_names.json");
            if cand.is_file() {
                let txt = std::fs::read_to_string(&cand).ok()?;
                let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
                let map = v.get("pandemic_hash_m2")?.as_object()?;
                let mut out = std::collections::HashMap::with_capacity(map.len());
                for (k, name) in map {
                    if let (Ok(h), Some(n)) =
                        (u32::from_str_radix(k.trim_start_matches("0x"), 16), name.as_str())
                    {
                        out.insert(h, n.to_string());
                    }
                }
                return Some(out);
            }
            dir = dir.parent()?;
        }
    }
    None
}

fn pack_data(
    out: &std::path::Path,
    names_csv: Option<std::path::PathBuf>,
    wadpath: &str,
    // The patch/DLC overlays opened on top of the base — their assets must be scanned too, or the
    // trim drops every patch-only name. See `referenced_hashes`.
    overlays: &[String],
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;

    // 1. names.bin. PREFERRED source: the committed, curated `data/production_names.json` that ships
    // IN this repo. When it is present the pack is fully self-contained — no 32 MB parent-repo rainbow
    // table and no game WAD needed, so any machine can rebuild the bundle. Falls back to the slow raw
    // corpora + WAD-trim path only when the production file is absent (i.e. when regenerating it).
    let (mut names, full) = match load_production_names() {
        Some(p) => {
            eprintln!("[pack] names.bin: from committed data/production_names.json ({} names)", p.len());
            let n = p.len();
            (p, n)
        }
        None => {
            eprintln!("[pack] no production_names.json — merging raw name corpora (slow one-time parse)…");
            let mut names = index::load_all_names_raw(names_csv, |_, _| {});
            if names.is_empty() {
                return Err("no name corpora found — run from the repo checkout".into());
            }
            let full = names.len();
            eprintln!(
                "[pack] scanning WAD + {} overlay(s) for referenced hashes (assets + clips + bone nodes)…",
                overlays.len()
            );
            let referenced = referenced_hashes(wadpath, overlays);
            if !referenced.is_empty() {
                names.retain(|h, _| referenced.contains(h));
            }
            (names, full)
        }
    };
    index::write_names_pack(&out.join("names.bin"), &names).map_err(|e| e.to_string())?;
    eprintln!(
        "[pack] names.bin: {} names (source pool {full})",
        names.len(),
    );

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
