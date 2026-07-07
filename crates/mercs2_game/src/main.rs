//! `mercs2_game` — the Mercenaries 2 game exe.
//!
//! This is the *game* layer: it configures and boots the asset-agnostic engine from the player's real
//! save. No Mercenaries-specific data lives in the engine; it lives here. The boot:
//! 1. find the newest `.profile` in the player's save folder,
//! 2. parse it → header + `SaveState` (active contract, mission flow, the `vz_state_*` world-state
//!    overlays to activate, playtime — see `mercs2_formats::save`),
//! 3. render the engine's streaming world IN-PROCESS at the authentic **PMC-interior spawn**
//!    (`MrxUtil._TeleportHero` = `(3794, 451, -3911)`, off-map high-Y).
//!
//! There is NO separate engine binary: `mercs2_game` IS the game exe. It calls the engine library's
//! public render entry point (`mercs2_engine::game_world::run_game_world`) directly, so
//! `cargo run -p mercs2_game` always rebuilds a fresh engine and opens the window itself.
//!
//! See `docs/modernization/pangea_engine_alignment.md` for the engine/game split this realizes.
//! Run `mercs2_game` to boot; `mercs2_game --plan` to print the boot-state without rendering.

use std::path::{Path, PathBuf};

use mercs2_formats::save;

mod collision; // GAME collision: world triangle soup + camera-boom raycast + player capsule push-out
mod gameplay; // GAME: fleet gameplay systems (physics/vehicle/combat/audio) ticked each fixed step
mod hero; // GAME character identity: 3 heroes + wardrobe outfit lists (_tCharacterMap/_tOutfits)
mod menu; // GAME shell menu: main menu + save browser (native ChangeShellState reimpl)
mod pmc; // GAME-specific PMC interior assembly (constants + load_pmc_interior)
mod script_host; // GAME-specific Lua interior boot (EngineHost impl + run_interior_boot)
mod world; // GAME render/boot: full TPS/free world render path (player avatar, 10-stage load)

use pmc::PMC_INTERIOR_SPAWN;

/// The engine loads the PMC interior ROOM (shells + furniture, by PATH) as static geometry at the
/// spawn (`mercs2_engine::game_world`), because the room shells don't resolve via the streaming
/// name-hash overlay recipe. So the game does NOT fold `vz_state_pmcinterior` here (that would
/// double-load the furniture). Extra interior overlays (recruit variants) could be added later.
const INTERIOR_OVERLAYS: &[&str] = &[];

/// `%USERPROFILE%\Documents\My Games\Mercenaries 2\SaveGames`, if it exists.
fn save_games_dir() -> Option<PathBuf> {
    let up = std::env::var("USERPROFILE").ok()?;
    let p = Path::new(&up).join("Documents/My Games/Mercenaries 2/SaveGames");
    p.is_dir().then_some(p)
}

/// Recruit-unlock + stockpile state from the newest save (for dev tools without a `SaveState` in hand).
fn newest_save_interior() -> (pmc::RecruitUnlocks, pmc::Stockpile) {
    let dir = save_games_dir();
    let path = dir.as_ref().and_then(|d| newest_profile(d));
    eprintln!("[save] SaveGames dir = {dir:?}");
    eprintln!("[save] newest .profile = {path:?}");
    let prof = path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| match save::parse(&b) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("[save] parse FAILED: {e}");
                None
            }
        });
    let ss = prof.as_ref().and_then(|p| match p.save_state() {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("[save] save_state FAILED: {e}");
            None
        }
    });
    eprintln!("[save] unlocked_starters = {:?}", ss.as_ref().map(|s| &s.unlocked_starters));
    let recruits = ss
        .map(|s| pmc::RecruitUnlocks::from_starters(&s.unlocked_starters))
        .unwrap_or_default();
    let stockpile = pmc::Stockpile {
        cash: prof.as_ref().map(|p| p.cash as i64).unwrap_or(0),
        ..Default::default()
    };
    (recruits, stockpile)
}

/// The most-recently-modified `.profile` in `dir` (the game's autosave/continue slot).
fn newest_profile(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("profile"))
        })
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // GAME dev tool: headless PMC interior assembly + floor/furniture Y check (no window). It is fine
    // for the GAME to parse its own args for dev modes — the ENGINE never does. `MERCS2_FURNDBG=1`
    // adds per-item floor Ys.
    if args.iter().any(|a| a == "--interior-assemble") {
        let (recruits, stockpile) = newest_save_interior();
        match mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            Some(mut w) => {
                let _ = pmc::load_pmc_interior(&mut w, recruits, &stockpile);
            }
            None => eprintln!("--interior-assemble: no vz.wad found"),
        }
        return;
    }

    // GAME dev tool: search EVERY WAD in the install (vz/English/shell/Loading) for a mesh by name or
    // 0xHASH — to locate assets missing from vz.wad. `--find-mesh <name|0xHASH>`.
    if let Some(i) = args.iter().position(|a| a == "--find-mesh") {
        let arg = args.get(i + 1).cloned().unwrap_or_default();
        let hash = arg
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        println!("[find-mesh] '{arg}' -> 0x{hash:08X}");
        if let Some(vz) = mercs2_engine::wad::registry_vz_wad() {
            if let Some(dir) = Path::new(&vz).parent() {
                for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("wad")) {
                        let Some(ps) = p.to_str() else { continue };
                        match mercs2_engine::wad::open(ps) {
                            Ok(mut w) => {
                                let models = mercs2_engine::wad::model_list(&w);
                                let present = models.iter().any(|(h, _)| *h == hash);
                                let types = mercs2_engine::wad::aset_types(&w, hash);
                                let mesh = mercs2_engine::game_world::load_model_by_hash(&mut w, hash)
                                    .map(|(m, _, _)| format!("MESH {}v/{}t", m.verts.len(), m.indices.len() / 3))
                                    .unwrap_or_else(|| "no-mesh".into());
                                // Type-agnostic: resolve the block by ANY primary ASET, walk to a model chunk.
                                let typed = mercs2_engine::wad::extract_container_typed(&mut w, hash, mercs2_engine::wad::MODEL_TYPE_HASH)
                                    .ok()
                                    .and_then(|c| mercs2_engine::mesh::build_indexed_from_container(&c).ok())
                                    .map(|(v, i, _, _)| format!("typed-model {}v/{}t", v.len(), i.len() / 3))
                                    .unwrap_or_else(|| "no-typed-model".into());
                                println!(
                                    "  {:<14} {} models, present={present}, types={types:?}, {mesh}, {typed}",
                                    p.file_name().unwrap().to_string_lossy(), models.len()
                                );
                            }
                            Err(err) => println!("  {}: open failed: {err}", p.file_name().unwrap().to_string_lossy()),
                        }
                    }
                }
            }
        }
        return;
    }

    // GAME dev tool: dump EVERY ASET (name_hash, type_id, primary) in vz.wad, for reverse-name hunts
    // over non-primary assets (props whose mesh isn't a type-19 model). `--dump-asets`.
    if args.iter().any(|a| a == "--dump-asets") {
        if let Some(w) = mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            for (h, t, prim) in mercs2_engine::wad::all_asets(&w) {
                println!("{h:08x} {t} {}", if prim { 1 } else { 0 });
            }
        }
        return;
    }

    // GAME dev tool: dump the COMP-type inventory of a WAD block (default 667, the PMC interior
    // overlay) — to see which components each placement carries (Transform/Name/ModelName/Model/…).
    if let Some(i) = args.iter().position(|a| a == "--comps") {
        let blk: u16 = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(667);
        if let Some(mut w) = mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            if let Ok(data) = mercs2_engine::wad::decompress_block_index(&mut w, blk) {
                println!("[comps] block {blk} COMP inventory:");
                for ci in mercs2_formats::placement::comp_inventory(&data) {
                    println!("  {ci:?}");
                }
            }
        }
        return;
    }

    // GAME dev tool: dump every ModelName furniture placement in a block (default 667, PMC interior)
    // as {model_hash, pos, quat, yaw} sorted by X — to see the authored layout (is the wardrobe
    // really on one side, or is the view flipped?). `--interior-placements [block]`.
    if let Some(i) = args.iter().position(|a| a == "--interior-placements") {
        let blk: u16 = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(667);
        if let Some(mut w) = mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            if let Ok(data) = mercs2_engine::wad::decompress_block_index(&mut w, blk) {
                let mut ps = mercs2_formats::placement::load_model_placements(&data);
                ps.sort_by(|a, b| a.pos[0].total_cmp(&b.pos[0]));
                let spawn = pmc::PMC_INTERIOR_SPAWN;
                println!("[interior-placements] block {blk}: {} ModelName placements (spawn {spawn:?}):", ps.len());
                for p in &ps {
                    let yaw = 2.0 * p.quat[1].atan2(p.quat[3]);
                    let (dx, dz) = (p.pos[0] - spawn[0], p.pos[2] - spawn[2]);
                    println!(
                        "  0x{:08X}  pos [{:8.2},{:8.2},{:8.2}]  yaw {:+.2}  d_from_spawn [dx {:+7.2}, dz {:+7.2}]",
                        p.model_hash, p.pos[0], p.pos[1], p.pos[2], yaw, dx, dz
                    );
                }
            }
        }
        return;
    }

    // GAME dev tool: positively ID the real PMC interior HALL shell (the `HqInterior` actor mesh,
    // unnamed hash). Scan every vz.wad model; a room-sized HOLLOW mesh whose LOCAL bbox contains the
    // player-enter hardpoint (spawn - actor_origin = (44, 0.75, -71), any sign combo) is the hall.
    // Report room-sized candidates sorted by footprint. `--hall-hunt`.
    if args.iter().any(|a| a == "--hall-hunt") {
        if let Some(mut w) = mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            // Hardpoint local offset (spawn - actor origin (3750,450,-3840)); try all 4 XZ signs since
            // the mesh's local axes/origin corner are unknown.
            let hp = [
                pmc::PMC_INTERIOR_SPAWN[0] - 3750.0,
                pmc::PMC_INTERIOR_SPAWN[1] - 450.0,
                pmc::PMC_INTERIOR_SPAWN[2] - (-3840.0),
            ];
            let corners: [[f32; 3]; 4] = [
                [hp[0], hp[1], hp[2]], [-hp[0], hp[1], hp[2]],
                [hp[0], hp[1], -hp[2]], [-hp[0], hp[1], -hp[2]],
            ];
            let models = mercs2_engine::wad::model_list(&w);
            println!("[hall-hunt] scanning {} models for a room enclosing hardpoint ±({:.1},{:.1},±{:.1})...", models.len(), hp[0], hp[1], hp[2]);
            let mut cands: Vec<(u32, usize, usize, [f32; 3], [f32; 3], bool)> = Vec::new();
            for (hash, _blk) in &models {
                let Some((m, bmin, bmax)) = mercs2_engine::game_world::load_model_by_hash(&mut w, *hash) else { continue };
                let (dx, dy, dz) = (bmax[0] - bmin[0], bmax[1] - bmin[1], bmax[2] - bmin[2]);
                // Room-sized: big XZ footprint, plausible ceiling height (not a tower / flat plane).
                if dx.max(dz) < 30.0 || dy < 2.5 || dy > 25.0 {
                    continue;
                }
                let inside = |c: &[f32; 3]| (0..3).all(|k| c[k] >= bmin[k] - 1.0 && c[k] <= bmax[k] + 1.0);
                let encloses = corners.iter().any(inside);
                cands.push((*hash, m.verts.len(), m.indices.len() / 3, bmin, bmax, encloses));
            }
            // Enclosing candidates first, then by footprint (dx*dz) desc.
            cands.sort_by(|a, b| {
                let fa = (a.4[0] - a.3[0]) * (a.4[2] - a.3[2]);
                let fb = (b.4[0] - b.3[0]) * (b.4[2] - b.3[2]);
                b.5.cmp(&a.5).then(fb.partial_cmp(&fa).unwrap_or(std::cmp::Ordering::Equal))
            });
            println!("[hall-hunt] {} room-sized candidates (ENCLOSES=hardpoint inside local bbox):", cands.len());
            let hashes: std::collections::BTreeSet<u32> = cands.iter().map(|c| c.0).collect();
            let names = mercs2_engine::worldutil::rainbow_names(&hashes); // hash -> name if reversible
            for (hash, v, t, bmin, bmax, enc) in cands.iter().take(40) {
                let nm = names.get(hash).map(|s| s.as_str()).unwrap_or("<unreversed>");
                println!(
                    "  {}0x{hash:08X} {nm:<38} {v:>6}v/{t:>6}t  bbox x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}] ({:.0}x{:.0}x{:.0})",
                    if *enc { "ENCLOSES " } else { "         " },
                    bmin[0], bmax[0], bmin[1], bmax[1], bmin[2], bmax[2],
                    bmax[0] - bmin[0], bmax[1] - bmin[1], bmax[2] - bmin[2]
                );
            }
        }
        return;
    }

    // GAME dev tool: audit texture streaming. For a MODEL hash, build it, collect its distinct
    // diffuse texture hashes, and for each show EVERY ASET row (block + primary) the texture hash
    // appears under, extracting each block's chunk to report dims + resident body + whether that
    // block carries the FULL mip0 (high-res) or only the streamed resident tail. `--tex-audit <0xMODEL>`.
    if let Some(i) = args.iter().position(|a| a == "--tex-audit") {
        let arg = args.get(i + 1).cloned().unwrap_or_default();
        let mhash = arg.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        if let Some(mut w) = mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            let Some((m, _, _)) = mercs2_engine::game_world::load_model_by_hash(&mut w, mhash) else {
                eprintln!("--tex-audit: model 0x{mhash:08X} did not build");
                return;
            };
            let mut texs: Vec<u32> = m.draws.iter().filter_map(|d| d.diffuse).collect();
            texs.sort_unstable();
            texs.dedup();
            println!("[tex-audit] model 0x{mhash:08X}: {} draws, {} distinct diffuse textures", m.draws.len(), texs.len());
            for th in texs.iter().take(12) {
                let rows = mercs2_engine::wad::aset_types(&w, *th);
                let tex_rows: Vec<(bool, u16)> = rows.iter().filter(|(t, _, _)| *t == 27).map(|(_, p, b)| (*p, *b)).collect();
                println!("  texture 0x{th:08X}: {} ASET texture-rows {:?}", tex_rows.len(), tex_rows);
                for (prim, blk) in &tex_rows {
                    match mercs2_engine::wad::tex_from_block(&mut w, *blk, *th) {
                        Some(td) => {
                            use mercs2_formats::texture::TexFormat;
                            let bb = if matches!(td.format, TexFormat::Bc1) { 8 } else { 16 };
                            let full0 = ((td.width as usize + 3) / 4) * ((td.height as usize + 3) / 4) * bb;
                            let has_full = td.mip0.len() >= full0;
                            println!(
                                "      block {blk:>4} (primary={prim}): {}x{} {:?} mips={} body={}B mip0={}B/{}B {}",
                                td.width, td.height, td.format, td.mip_count, td.all_mips.len(), td.mip0.len(), full0,
                                if has_full { "<< FULL mip0 (HIGH-RES)" } else { "resident tail only" }
                            );
                        }
                        None => println!("      block {blk:>4} (primary={prim}): no texture chunk 0x{th:08X}"),
                    }
                }
                // Full hi-res assembly across the cell subtree.
                if let Ok(hd) = mercs2_engine::wad::extract_texture_hires(&mut w, *th) {
                    use mercs2_formats::texture::TexFormat;
                    let bb = if matches!(hd.format, TexFormat::Bc1) { 8 } else { 16 };
                    let full0 = ((hd.width as usize + 3) / 4) * ((hd.height as usize + 3) / 4) * bb;
                    println!(
                        "      => HIRES assembled: {}x{} mips={} body={}B mip0={}B/{}B {}",
                        hd.width, hd.height, hd.mip_count, hd.all_mips.len(), hd.mip0.len(), full0,
                        if hd.mip0.len() >= full0 { "<< FULL mip0 ✓" } else { "still tail-only" }
                    );
                }
            }
        }
        return;
    }

    // GAME dev tool: EXHAUSTIVELY locate every block that carries a texture chunk for a hash (by
    // scanning every block's entry table, not just the ASET table) — to find the high-res streaming
    // copy that isn't ASET-indexed. Reports (block, chunk_size) sorted big-first + parses the largest.
    // `--tex-locate <0xHASH>`.
    if let Some(i) = args.iter().position(|a| a == "--tex-locate") {
        let arg = args.get(i + 1).cloned().unwrap_or_default();
        let th = arg.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(arg.trim_start_matches('_')));
        if let Some(mut w) = mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) {
            let nblocks = mercs2_engine::wad::block_paths(&w).len();
            println!("[tex-locate] 0x{th:08X}: scanning {nblocks} block entry tables...");
            let mut hits: Vec<(u16, u32)> = Vec::new();
            for blk in 0..nblocks as u16 {
                let Ok(head) = mercs2_engine::wad::peek_block_head(&mut w, blk, 512 * 1024) else { continue };
                let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&head);
                if count == 0 { continue; }
                for e in &entries {
                    if e.type_hash == 0xF011157A && e.name_hash == th {
                        hits.push((blk, e.chunk_size));
                    }
                }
            }
            hits.sort_by(|a, b| b.1.cmp(&a.1));
            println!("[tex-locate] {} block(s) carry a texture chunk for 0x{th:08X}:", hits.len());
            for (blk, sz) in &hits {
                let path = mercs2_engine::wad::block_paths(&w).get(*blk as usize).cloned().unwrap_or_default();
                println!("  block {blk:>4}  chunk_size {sz:>8}B  {path}");
            }
            // Inspect the raw chunk bytes of each hit block (are the finer LOD blocks UCFX containers
            // with INFO, or raw mip payloads?).
            for (blk, sz) in &hits {
                if let Ok(dec) = mercs2_engine::wad::decompress_block_index(&mut w, *blk) {
                    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
                    let mut off = 4 + count as usize * 16;
                    for e in &entries {
                        let end = off + e.chunk_size as usize;
                        if e.type_hash == 0xF011157A && e.name_hash == th && end <= dec.len() {
                            let head: Vec<String> = dec[off..(off + 40).min(end)].iter().map(|b| format!("{b:02x}")).collect();
                            let tag = String::from_utf8_lossy(&dec[off..(off + 4).min(end)]).to_string();
                            println!("  block {blk} ({sz}B) chunk[0..40]: {}  tag='{tag}'", head.join(" "));
                            break;
                        }
                        off = end;
                    }
                }
            }
        }
        return;
    }

    // GAME dev tool: diagnose the interior collision — build the hall collision triangles exactly as
    // the game does (mesh 0x39AF17DC at actor origin), then simulate the player walking +Z from spawn
    // into the archway, applying push_out each step, and report where they stick + the blocking walls.
    // `--coll-probe`.
    if args.iter().any(|a| a == "--coll-probe") {
        use mercs2_core::glam::Vec3;
        let Some(mut w) = mercs2_engine::wad::registry_vz_wad().and_then(|p| mercs2_engine::wad::open(&p).ok()) else { return; };
        // Same as pmc.rs: hall at actor origin (3750,450,-3840), identity quat.
        let origin = Vec3::new(3750.0, 450.0, -3840.0);
        let Some((m, _, _)) = mercs2_engine::game_world::load_model_by_hash_state(&mut w, 0x39AF17DC, 0x01) else {
            eprintln!("--coll-probe: hall mesh did not build"); return;
        };
        let mut tris: Vec<[Vec3; 3]> = Vec::new();
        for idx in m.indices.chunks_exact(3) {
            tris.push([
                Vec3::from(m.verts[idx[0] as usize].pos) + origin,
                Vec3::from(m.verts[idx[1] as usize].pos) + origin,
                Vec3::from(m.verts[idx[2] as usize].pos) + origin,
            ]);
        }
        let spawn = Vec3::new(pmc::PMC_INTERIOR_SPAWN[0], pmc::PMC_INTERIOR_SPAWN[1], pmc::PMC_INTERIOR_SPAWN[2]);
        println!("[coll-probe] {} hall tris; walking +Z from spawn {spawn:?}", tris.len());
        let (radius, height, step_h) = (0.35f32, 1.8f32, 0.5f32);
        // Walk in each cardinal direction; report total distance travelled before it stalls (a wall
        // stops it in a metre or two; the open archway lets it travel far).
        for (name, dir) in [("+Z (arch)", Vec3::Z), ("-Z (back)", -Vec3::Z), ("+X", Vec3::X), ("-X", -Vec3::X)] {
            let mut pos = spawn;
            let mut travelled = 0.0f32;
            for _ in 0..160 {
                let resolved = crate::collision::move_character(&tris, pos, dir * 0.2, radius, height, step_h, true);
                let adv = (resolved - pos).length();
                travelled += adv;
                pos = resolved;
                if adv < 0.02 {
                    break;
                }
            }
            println!("  walk {name:12}: travelled {travelled:5.1} m -> end {pos:?}");
        }
        let stuck_at: Option<Vec3> = None;
        // Dump the near-vertical walls around the stuck point at chest height.
        if let Some(p) = stuck_at {
            let c = p + Vec3::Y * (height * 0.55);
            println!("[coll-probe] near-vertical walls within 1.5 m of chest {c:?}:");
            let mut near: Vec<(f32, Vec3, [Vec3; 3])> = Vec::new();
            for t in &tris {
                let n = (t[1] - t[0]).cross(t[2] - t[0]);
                let nl = n.length();
                if nl < 1e-6 || (n.y / nl).abs() > 0.5 { continue; }
                let ctr = (t[0] + t[1] + t[2]) / 3.0;
                let d = ((ctr - c) * Vec3::new(1.0, 0.0, 1.0)).length();
                if d < 1.5 { near.push((d, n / nl, *t)); }
            }
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (d, n, t) in near.iter().take(12) {
                println!("   d={d:.2} normal=({:+.2},{:+.2},{:+.2}) v0={:?} v1={:?} v2={:?}", n.x, n.y, n.z, t[0], t[1], t[2]);
            }
        }
        return;
    }

    // GAME dev tool: scan c3 models for flat, floor-sized meshes (PMC-floor candidates).
    if args.iter().any(|a| a == "--c3-flat") {
        if let Some(p) = mercs2_engine::wad::registry_vz_wad() {
            if let Err(e) = mercs2_engine::diag::c3_flat_report(&p) {
                eprintln!("--c3-flat: {e}");
            }
        }
        return;
    }

    let plan_only = args.iter().any(|a| a == "--plan");
    // Optional explicit profile path (positional); else newest in the save folder.
    let explicit = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from);

    // ── Default boot = the SHELL MENU (retail flow) ──────────────────────────
    // No explicit .profile and no dev mode: open on the main menu; the player picks
    // Continue / New Game / Load Game (save browser) / Quit and the selected save drives the
    // world boot in-loop. An explicit .profile arg, `--plan` and `--stream` keep the direct
    // no-menu boots (dev workflows).
    if explicit.is_none() && !plan_only && !args.iter().any(|a| a == "--stream") {
        let slots = menu::scan_slots(save_games_dir());
        println!("[shell] main menu: {} save(s) available", slots.len());
        let wadpath = require_vz_wad();
        let orbit = args.iter().any(|a| a == "--interior-orbit");
        pollster::block_on(world::run_scene_world_loading(
            wadpath,
            true,
            true,
            true,
            true,
            true,
            orbit,
            pmc::RecruitUnlocks::default(),
            pmc::Stockpile::default(),
            hero::player_model_candidates(1, 0, 0), // retail new game: Mattias, tier 0, default skin
            Some(menu::Menu::new(slots)),
        ));
        return;
    }

    let profile_path = match explicit.or_else(|| save_games_dir().and_then(|d| newest_profile(&d))) {
        Some(p) => p,
        None => {
            eprintln!("mercs2_game: no .profile save found in %USERPROFILE%\\Documents\\My Games\\Mercenaries 2\\SaveGames. Pass a .profile path.");
            std::process::exit(1);
        }
    };

    let bytes = match std::fs::read(&profile_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("mercs2_game: read {}: {e}", profile_path.display());
            std::process::exit(1);
        }
    };
    let profile = match save::parse(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("mercs2_game: parse {}: {e}", profile_path.display());
            std::process::exit(1);
        }
    };
    let state = profile.save_state().ok();
    let spawn = PMC_INTERIOR_SPAWN; // authentic game-start = PMC interior

    // ── Boot banner (the game's start-state, from the save) ──────────────────
    let line = "=".repeat(66);
    println!("{line}");
    println!("  MERCENARIES 2 - booting from save");
    println!("  profile   : {}", profile.save_name());
    println!(
        "  file      : {}",
        profile_path.file_name().and_then(|s| s.to_str()).unwrap_or("?")
    );
    println!("  contract  : {}", profile.active_contract());
    println!(
        "  playtime  : {}s   cash: {}   fuel: {}",
        profile.play_time_seconds, profile.cash, profile.fuel
    );
    if let Some(s) = &state {
        if !s.flow_chain.is_empty() {
            println!("  flow      : {}", s.flow_chain.join(" -> "));
        }
        println!(
            "  missions  : {} active ({})",
            s.active_missions.len(),
            s.active_missions
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!(
            "  overlays  : {} vz_state world-state layers to activate",
            s.layers.len()
        );
        for l in s.layers.iter().take(4) {
            println!("                {l}");
        }
        if s.layers.len() > 4 {
            println!("                ... +{} more", s.layers.len() - 4);
        }
    } else {
        println!("  (SaveSingleton Lua state unavailable - header only)");
    }
    println!(
        "  spawn     : PMC interior ({:.1}, {:.1}, {:.1})",
        spawn[0], spawn[1], spawn[2]
    );
    println!("{line}");

    if plan_only {
        println!("[mercs2_game] --plan: boot-state only; not rendering the world.");
        return;
    }

    // ── Render the engine's streaming world IN-PROCESS at the spawn ──────────
    // No separate engine binary: mercs2_game calls the engine library's public render entry point
    // directly. The active vz_state overlays are the save's world-state layers PLUS the PMC interior
    // overlay (the game-start spawn IS the interior); the engine resolves each -> its WAD block and
    // folds it into the streaming manager.
    let mut layers: Vec<String> = state.as_ref().map(|s| s.layers.clone()).unwrap_or_default();
    for l in INTERIOR_OVERLAYS {
        if !layers.iter().any(|x| x == *l) {
            layers.push((*l).to_string());
        }
    }

    // The engine consumes the original game's assets from vz.wad (EA Games registry install dir).
    let wadpath = require_vz_wad();
    // Default boot = the FULL game world: player + third-person camera + PMC interior + c3 cells +
    // placements + props — all core components ON. `--stream` selects the alternate streaming free-fly
    // world; `--interior-orbit` adds the debug orbit camera.
    // The save drives the interior: which recruits are unlocked (state layers + bays) and how much
    // cash/supplies the player has (stockpile piles).
    let recruits = state
        .as_ref()
        .map(|s| pmc::RecruitUnlocks::from_starters(&s.unlocked_starters))
        .unwrap_or_default();
    let stockpile = pmc::Stockpile { cash: profile.cash as i64, ..Default::default() };
    if args.iter().any(|a| a == "--stream") {
        println!(
            "[mercs2_game] streaming world (free-fly): spawn=({:.1},{:.1},{:.1}) overlays={}",
            spawn[0], spawn[1], spawn[2], layers.len()
        );
        let sp = stockpile.clone();
        pollster::block_on(mercs2_engine::game_world::run_game_world(
            wadpath,
            Some(PMC_INTERIOR_SPAWN),
            layers,
            move |world, scene, wad| populate_pmc_interior(world, scene, wad, recruits, &sp),
        ));
    } else {
        println!("[mercs2_game] full world: TPS + PMC interior + c3 cells + placements + props");
        let orbit = args.iter().any(|a| a == "--interior-orbit");
        // Direct boot: the player model comes from THIS profile's hero + upgrade tier, like a
        // menu boot (costume file byte not yet located; 0 = wardrobe unused in all known saves).
        let hero_idx = profile.character_index; // header @0x4D, 1-based
        let models = hero::player_model_candidates(hero_idx, profile.upgrade_index, 0);
        println!(
            "[mercs2_game] character: {} [{}]",
            hero::hero(hero_idx).display,
            hero::look_label(hero_idx, profile.upgrade_index, 0)
        );
        pollster::block_on(world::run_scene_world_loading(
            wadpath, true, true, true, true, true, orbit, recruits, stockpile, models, None,
        ));
    }
}

/// Resolve the install's `vz.wad` via the EA Games registry key, or exit with the install hint.
fn require_vz_wad() -> String {
    match mercs2_engine::wad::registry_vz_wad() {
        Some(p) => p,
        None => {
            eprintln!(
                "mercs2_game: no vz.wad found — install Mercenaries 2 so that the EA Games registry \
                 key resolves to a folder containing data\\vz.wad."
            );
            std::process::exit(1);
        }
    }
}

/// GAME world population: once the engine's streaming world has loaded, spawn the PMC interior into
/// the engine's World/Scene. The interior spawns because the GAME asks for it — the engine has no
/// concept of a "PMC interior". Runs the authentic `MrxUtil.SpawnActor` path (`run_interior_boot`)
/// then realizes the resolved geometry (`load_pmc_interior`) as ECS entities.
///
/// (Phase-1 seam: `load_pmc_interior` / `run_interior_boot` still physically live in `mercs2_engine`
/// and are called here through its public API; a follow-up moves those bodies into this crate so the
/// engine holds none of it.)
fn populate_pmc_interior(
    world: &mut mercs2_core::World,
    scene: &mut mercs2_engine::scene::Scene,
    wad: &mut mercs2_engine::wad::Wad,
    recruits: pmc::RecruitUnlocks,
    stockpile: &pmc::Stockpile,
) {
    use mercs2_core::glam::{Quat, Vec3};
    use mercs2_core::{AnimState, ModelRef, SkinPalette, Transform};
    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let intents = script_host::run_interior_boot();
    for r in &intents {
        println!(
            "[lua] Pg.Spawn '{}' (name={}) at ({:.1},{:.1},{:.1}) -> guid 0x{:x}",
            r.template, r.name, r.pos[0], r.pos[1], r.pos[2], r.guid
        );
    }
    let want = intents
        .iter()
        .any(|r| r.template.eq_ignore_ascii_case(script_host::PMC_INTERIOR_TEMPLATE));
    if !want {
        return;
    }
    match pmc::load_pmc_interior(wad, recruits, stockpile) {
        Ok(pieces) => {
            let n = pieces.len();
            for (m, pos, quat) in pieces {
                if !scene.has_model(m.hash) {
                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                }
                let nbones = scene.model_bone_count(m.hash).max(1);
                world.spawn((
                    Transform {
                        translation: Vec3::new(pos[0], pos[1], pos[2]),
                        rotation: Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]),
                        scale: Vec3::ONE,
                    },
                    ModelRef { model: m.hash },
                    AnimState::default(),
                    SkinPalette { mats: vec![IDENTITY; nbones] },
                ));
            }
            println!("[game] PMC interior: {n} pieces placed");
        }
        Err(e) => eprintln!("[game] PMC interior load failed: {e}"),
    }
}
