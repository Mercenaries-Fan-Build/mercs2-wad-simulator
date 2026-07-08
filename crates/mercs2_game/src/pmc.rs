//! GAME-specific PMC HQ interior assembly.
//!
//! This is Mercenaries-specific content — where the PMC interior is, which blocks/meshes make it up,
//! the destruction-state tiers, the floor heights — so it lives in the GAME, not the engine. It calls
//! the engine's asset-agnostic public API (`mercs2_engine::wad` / `game_world::load_model_by_hash*` /
//! `render::LoadedModel`) to turn WAD data into renderable meshes, then the game spawns them.

use std::collections::HashMap;

use mercs2_engine::game_world::{load_model_by_hash, load_model_by_hash_state};
use mercs2_engine::render::LoadedModel;
use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::placement::{load_model_placements, load_placements};

// (The PMC interior asset block `pmc_interior_P000_Q3.block` = 3490 is FaceFX/Scaleform only, no
// geometry — the renderable interior is placed instances in the state overlay blocks 667/711/461/703.)

/// Authored game-start spawn (`MrxUtil._TeleportHero`). The interior placements are already in this
/// world space (their floor sits at Y≈450.8), so loaded geometry is placed at the authored world
/// position with NO synthetic offset.
pub const PMC_INTERIOR_SPAWN: [f32; 3] = [3794.0427, 450.7505, -3911.0322];

/// The `HqInterior` actor position — `mrxhq.lua:657` `SpawnActor(vPosition = {3750, 450, -3840})`. The
/// interior geometry loads at this origin (see `load_pmc_interior`).
pub const PMC_INTERIOR_ACTOR_ORIGIN: [f32; 3] = [3750.0, 450.0, -3840.0];

/// Derive the HQ-interior hero spawn **the way the base game does** — not a baked constant. The game
/// spawns the `HqInterior` actor at `vPosition {3750,450,-3840}` (`mrxhq.lua`) then teleports the hero to
/// the `hp_playerA_enter` hardpoint on the interior hall mesh (`MrxUtil.TeleportHeroesToHardpoints`,
/// mrxbriefing.lua). So the spawn = actor position (Lua data) + that hardpoint (mesh HIER). Since the
/// interior geometry is placed at the actor origin, this lands the hero on the interior floor. Falls back
/// to the actor origin (hall centre, over the floor) if the hardpoint node isn't present.
pub fn derive_interior_spawn(w: &mut wad::Wad) -> [f32; 3] {
    let hp = pandemic_hash_m2("hp_playerA_enter");
    let hall = pandemic_hash_m2("pmcoutpost_interior_hq");
    if let Ok(container) = wad::extract_container(w, hall) {
        let hier = mercs2_formats::orchestrator::parse_hier(&container);
        if let Some(idx) = hier.iter().position(|n| n.hash == hp) {
            let m = hier_node_root_matrix(&hier, idx);
            return [
                PMC_INTERIOR_ACTOR_ORIGIN[0] + m[12],
                PMC_INTERIOR_ACTOR_ORIGIN[1] + m[13],
                PMC_INTERIOR_ACTOR_ORIGIN[2] + m[14],
            ];
        }
    }
    PMC_INTERIOR_ACTOR_ORIGIN
}

/// Root-relative transform of HIER node `idx` = local · parent.local · … (row-major; translation at
/// `[12],[13],[14]`).
fn hier_node_root_matrix(hier: &[mercs2_formats::orchestrator::HierNode], idx: usize) -> [f32; 16] {
    let mut m = hier[idx].local;
    let mut p = hier[idx].parent;
    while let Some(pi) = p {
        m = mat4_mul_rowmajor(&m, &hier[pi].local);
        p = hier[pi].parent;
    }
    m
}

/// Row-major 4×4 multiply: `(a·b)[r][c] = Σ a[r][k]·b[k][c]`.
fn mat4_mul_rowmajor(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0.0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[r * 4 + k] * b[k * 4 + c];
            }
            o[r * 4 + c] = s;
        }
    }
    o
}

/// Which Villa recruits are unlocked (from the save's `tStarterData`). Drives which PMC-interior state
/// layers + recruit bays load — the game shows only the unlocked recruits (wifpmcinterior.lua
/// `_GetStarterLayers`), NOT all of them. Derive with [`RecruitUnlocks::from_starters`].
#[derive(Clone, Copy, Debug, Default)]
pub struct RecruitUnlocks {
    /// Ewen (`HelPmcBoss`) → `Vz_State_PmcInterior_Hel` (703); recruitheli mesh is absent from vz.wad.
    pub hel: bool,
    /// Eva (`MecPmcBoss`) → `Vz_State_PmcInterior_Mec` (461) if unlocked, else `_MecAbsent` (291);
    /// recruitmechanic mesh 0xE8EB75D7.
    pub mec: bool,
    /// Misha (`JetPmcBoss`) → `Vz_State_PmcInterior_Jet` (711); recruitjet mesh 0x86D7CF92.
    pub jet: bool,
}

impl RecruitUnlocks {
    /// Derive from the save's unlocked starter ids (`SaveState::unlocked_starters`). `PmcBoss` (Fiona)
    /// is the always-present base and is ignored.
    pub fn from_starters(starters: &[String]) -> Self {
        let has = |n: &str| starters.iter().any(|s| s == n);
        RecruitUnlocks {
            hel: has("HelPmcBoss"),
            mec: has("MecPmcBoss"),
            jet: has("JetPmcBoss"),
        }
    }
}

/// Save-driven stockpile quantities (cash + per-support-type qty). The villa's physical money/supply
/// PILES appear tier-by-tier as these grow (wifpmcinterior.lua `_tStockpile` thresholds +
/// `_SetStockpileCategoryQty`). NOTE: currently only `pmcoutpost_stockpile_money 1` is placed in the
/// interior blocks, and its mesh is NOT resolvable in vz.wad (no ModelName COMP, name-hash misses) —
/// the general placement→mesh gap — so this gates correctly but renders nothing until that mesh is
/// found. Support piles + higher money tiers aren't placed at all.
#[derive(Clone, Debug, Default)]
pub struct Stockpile {
    pub cash: i64,
    pub support: std::collections::HashMap<String, i64>,
}

impl Stockpile {
    fn qty(&self, cat: &str) -> i64 {
        if cat == "money" {
            self.cash
        } else {
            self.support.get(cat).copied().unwrap_or(0)
        }
    }
    /// `_tStockpile[cat]` thresholds; pile tier `i` (1-based) is visible iff `qty >= threshold[i-1]`.
    fn thresholds(cat: &str) -> &'static [i64] {
        match cat {
            "money" => &[
                1000, 976562, 1953125, 3906250, 7812500, 15625000, 31250000, 625000000, 125000000,
                250000000, 500000000, 1000000000,
            ],
            "moab" => &[1, 2, 2],
            "bunkerbuster" | "fuelairbomb" => &[1, 2, 3],
            // artillery/bombingrun/clusterbomb/combatairpatrol/laserguidedbomb/rocketartillery/…
            _ => &[1, 5, 9],
        }
    }
    /// Is stockpile pile `<cat> <tier>` visible for this save's quantities?
    pub fn tier_visible(&self, cat: &str, tier: usize) -> bool {
        Self::thresholds(cat)
            .get(tier.saturating_sub(1))
            .map(|&t| self.qty(cat) >= t)
            .unwrap_or(false)
    }
}

/// The MESH asset a stockpile category renders as. The placement name (`pmcoutpost_stockpile_money 1`)
/// does NOT hash to the asset — the real asset is named differently (found via reverse-hashing every
/// vz.wad ASET: `--find-mesh`/`--dump-asets`). money → `global_moneylargea` (0xA14A463B, 48v, verified).
/// The `stockpile_<cat>` support assets are type-27 and don't `build_indexed` yet (TODO).
fn stockpile_mesh(cat: &str) -> Option<u32> {
    match cat {
        "money" => Some(0xA14A463B),
        _ => None,
    }
}

/// Parse a `pmcoutpost_stockpile_<category> <tier>` placement name → (category, tier); `None` otherwise.
fn parse_stockpile_name(name: Option<&str>) -> Option<(String, usize)> {
    let n = name?;
    let core = n.split(" 0x").next().unwrap_or(n).trim_start_matches('_');
    let rest = core.strip_prefix("pmcoutpost_stockpile_")?;
    let (cat, tier) = rest.rsplit_once(' ')?;
    Some((cat.to_string(), tier.parse().ok()?))
}

/// Assemble the PMC HQ interior from its keyed entities for the given recruit-unlock state: the base
/// `Vz_State_PmcInterior` (667) always, plus each recruit's PRESENT state layer + bay if unlocked (else
/// the ABSENT layer — only Eva/Mec has one). Plus the actor-anchored shell buildings. Returns
/// (model, world pos, world quat) per instance. See memory `pmc-teleport-coords-and-interior`.
pub fn load_pmc_interior(
    w: &mut wad::Wad,
    recruits: RecruitUnlocks,
    stockpile: &Stockpile,
) -> Result<Vec<(LoadedModel, [f32; 3], [f32; 4])>, String> {
    let mut out: Vec<(LoadedModel, [f32; 3], [f32; 4])> = Vec::new();
    let (mut tv, mut tt) = (0usize, 0usize);
    let mut distinct: HashMap<u32, usize> = HashMap::new();
    let mut wmin = [f32::MAX; 3];
    let mut wmax = [f32::MIN; 3];

    // Furniture: for EVERY entity in the interior state blocks, resolve its mesh via the ModelName COMP
    // hash if present, else the entity name hashed (asset names drop the leading `_` and ` 0xKEY`
    // suffix), and place it at its authored Transform. Locators/hardpoints (no mesh) simply skip.
    let (mut furn_ymin, mut furn_ymax) = (f32::MAX, f32::MIN);
    // Per wifpmcinterior.lua `_GetStarterLayers`: base always; each recruit contributes its PRESENT
    // layer if unlocked, else its ABSENT layer (only Eva/Mec has one). Loading all of them (the old
    // behavior) shows every recruit's bay regardless of the save — wrong for an early game.
    let mut state_blocks: Vec<u16> = vec![667]; // Vz_State_PmcInterior (base)
    state_blocks.push(if recruits.mec { 461 } else { 291 }); // Eva: _Mec present / _MecAbsent
    if recruits.jet {
        state_blocks.push(711); // Misha: _Jet
    }
    if recruits.hel {
        state_blocks.push(703); // Ewen: _Hel
    }
    if std::env::var("MERCS2_ALLBLOCKS").is_ok() {
        state_blocks = vec![667, 711, 461, 703, 291]; // dev: dump every interior variant
    }
    println!(
        "[interior] recruits unlocked: hel={} mec={} jet={} -> state blocks {:?}",
        recruits.hel, recruits.mec, recruits.jet, state_blocks
    );
    for &blk in &state_blocks {
        let Ok(data) = wad::decompress_block_index(w, blk) else { continue };
        let model_by_key: HashMap<u32, u32> = load_model_placements(&data)
            .into_iter()
            .map(|mp| (mp.key, mp.model_hash))
            .collect();
        let placements = load_placements(&data).unwrap_or_default();
        let mut resolved = 0usize;
        for p in &placements {
            if std::env::var("MERCS2_ALLNAMES").is_ok() {
                println!("[name] blk{blk} '{}'", p.name.as_deref().unwrap_or("?"));
            }
            // Stockpile piles (`pmcoutpost_stockpile_<cat> <tier>`) grow with the player's cash/supplies:
            // tier `i` is visible only once the save's quantity reaches `_tStockpile[cat][i]`
            // (wifpmcinterior.lua `_SetStockpileCategoryQty`). Hide the tiers the save hasn't reached, and
            // resolve their mesh by CATEGORY (the placement name doesn't hash to the asset).
            let sp = parse_stockpile_name(p.name.as_deref());
            if let Some((cat, tier)) = &sp {
                if !stockpile.tier_visible(cat, *tier) {
                    continue;
                }
            }
            let hash = sp
                .as_ref()
                .and_then(|(cat, _)| stockpile_mesh(cat))
                .or_else(|| model_by_key.get(&p.key).copied())
                .or_else(|| {
                    p.name.as_deref().map(|n| {
                        let base = n.split(" 0x").next().unwrap_or(n).trim_start_matches('_');
                        pandemic_hash_m2(base)
                    })
                });
            let Some(hash) = hash else { continue };
            let Some((m, bmin, bmax)) = load_model_by_hash(w, hash) else { continue };
            tv += m.verts.len();
            tt += m.indices.len() / 3;
            for c in 0..3 {
                wmin[c] = wmin[c].min(p.pos[c] + bmin[c]);
                wmax[c] = wmax[c].max(p.pos[c] + bmax[c]);
            }
            furn_ymin = furn_ymin.min(p.pos[1] + bmin[1]);
            furn_ymax = furn_ymax.max(p.pos[1] + bmin[1]);
            if std::env::var("MERCS2_FURNDBG").is_ok() {
                println!(
                    "[furn] blk{blk} '{}' floor Y {:.2} (pos {:.2}, mesh bmin {:.2})",
                    p.name.as_deref().unwrap_or("?"), p.pos[1] + bmin[1], p.pos[1], bmin[1]
                );
            }
            let (dx, _dy, dz) = (bmax[0] - bmin[0], bmax[1] - bmin[1], bmax[2] - bmin[2]);
            if dx > 18.0 || dz > 18.0 {
                println!(
                    "[interior]   LARGE mesh 0x{hash:08X} '{}' {}v @ ({:.1},{:.1},{:.1})",
                    p.name.as_deref().unwrap_or("?"), m.verts.len(), p.pos[0], p.pos[1], p.pos[2]
                );
            }
            *distinct.entry(hash).or_insert(0) += 1;
            resolved += 1;
            out.push((m, p.pos, p.quat));
        }
        println!(
            "[interior] block {blk}: {} transforms, {} ModelName, {resolved} resolved to a mesh",
            placements.len(), model_by_key.len()
        );
    }

    // Actor-anchored interior meshes. The player teleports onto the `HqInterior` actor (mrxbriefing.lua
    // `Object.SetName(tActors[1],"HqInterior")`; player at hardpoint `hp_playerA`), spawned at
    // (3750,450,-3840). The enclosing MAIN HALL is the `pmcoutpost_interior_hq` mesh (0x39AF17DC, a
    // NON-primary type-19 model — 131,834 tris, the ornate columned hall with the tiled floor), NOT the
    // `_pmcoutpost_bld_hq_livedin` EXTERIOR building (wifpmcinterior.lua distinguishes them: uRealPmc =
    // the _bld_ exterior vs uFakePmc = "HqInterior"). Recruit bays (`pmcoutpost_interior_recruit*`) attach
    // to the same actor origin when unlocked. All share the interior naming `pmcoutpost_interior_<room>`.
    const ACTOR_ORIGIN: [f32; 3] = [3750.0, 450.0, -3840.0];
    const IDENT_QUAT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    // HqInterior actor parts, resolved by NAME (pandemic_hash_m2 — the live registry is the identity
    // source, no hardcoded hashes). BASE structures are always present; recruit bays are added per
    // _GetStarterLayers by unlock. See docs/modernization/pmc_interior_loading.md.
    let mut interior_parts: Vec<&str> = vec![
        "pmcoutpost_interior_hq",       // the ornate main hall
        "pmcoutpost_interior_sickbay",  // sickbay
        "pmcoutpost_interior_scaffold", // scaffolding
        "proutpost_interior_job",       // base job room
    ];
    if recruits.mec {
        interior_parts.push("pmcoutpost_interior_recruitmechanic");
    }
    if recruits.jet {
        interior_parts.push("pmcoutpost_interior_recruitjet");
    }
    if recruits.hel {
        interior_parts.push("pmcoutpost_interior_recruitheli");
    }
    for name in &interior_parts {
        let hash = pandemic_hash_m2(name);
        if let Some((m, bmin, bmax)) = load_model_by_hash_state(w, hash, 0x01) {
            for c in 0..3 {
                wmin[c] = wmin[c].min(ACTOR_ORIGIN[c] + bmin[c]);
                wmax[c] = wmax[c].max(ACTOR_ORIGIN[c] + bmax[c]);
            }
            tv += m.verts.len();
            tt += m.indices.len() / 3;
            *distinct.entry(hash).or_insert(0) += 1;
            println!(
                "[interior] actor part '{name}' 0x{hash:08X}: {} v / {} t @ origin; FLOOR Y {:.2}",
                m.verts.len(), m.indices.len() / 3, ACTOR_ORIGIN[1] + bmin[1]
            );
            out.push((m, ACTOR_ORIGIN, IDENT_QUAT));
        } else {
            println!("[interior] actor part '{name}' 0x{hash:08X}: NOT FOUND in vz.wad");
        }
    }

    println!(
        "[interior] assembled {} instance(s) ({} distinct meshes), {tv} verts / {tt} tris",
        out.len(), distinct.len()
    );
    if furn_ymin <= furn_ymax {
        println!(
            "[interior] FLOOR CHECK: furniture bottoms Y {:.2}..{:.2}; hero feet Y {:.2}",
            furn_ymin, furn_ymax, PMC_INTERIOR_SPAWN[1]
        );
    }
    if !out.is_empty() {
        println!(
            "[interior] WORLD BBOX min=({:.1},{:.1},{:.1}) max=({:.1},{:.1},{:.1})",
            wmin[0], wmin[1], wmin[2], wmax[0], wmax[1], wmax[2]
        );
    }
    Ok(out)
}
