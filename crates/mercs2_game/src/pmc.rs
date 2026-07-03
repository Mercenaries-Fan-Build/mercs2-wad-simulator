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
            // (wifpmcinterior.lua `_SetStockpileCategoryQty`). Hide the tiers the save hasn't reached.
            if let Some((cat, tier)) = parse_stockpile_name(p.name.as_deref()) {
                if !stockpile.tier_visible(&cat, tier) {
                    continue;
                }
            }
            let hash = model_by_key.get(&p.key).copied().or_else(|| {
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

    // Actor-anchored shell buildings + recruit bays (wifpmcinterior.lua `_tBuildings` + mrxstarter
    // SpawnActor @ (3750,450,-3840)). The `_livedin` HQ building is the enclosing MAIN HALL; it renders
    // its INTACT destruction tier (mask 0x04; 0x03 = ruined). The hall + suites are seated at the hero
    // floor Y (450.75, where the block-667 furniture sits); the bays/garage keep the actor anchor.
    const ACTOR_ORIGIN: [f32; 3] = [3750.0, 450.0, -3840.0];
    const IDENT_QUAT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    let hall_origin = [ACTOR_ORIGIN[0], PMC_INTERIOR_SPAWN[1], ACTOR_ORIGIN[2]]; // Y = 450.75
    // The HQ shell buildings (Fiona's HQ) are always present; the recruit bays only when unlocked.
    let mut interior_actor_meshes: Vec<(&str, u32, u8, [f32; 3])> = vec![
        ("pmcoutpost_bld_hq_livedin", 0x3E629E14, 0x04, hall_origin), // MAIN HALL — floor at hero level
        ("pmcoutpost_bld_hqsuites", 0xD5D65249, 0x04, hall_origin),   // suites room (hall level)
        ("pmcoutpost_bld_hqgarage_livedin", 0x33AC0183, 0x04, ACTOR_ORIGIN), // garage (own level)
    ];
    if recruits.mec {
        interior_actor_meshes.push(("recruitmechanic", 0xE8EB75D7, 0x01, ACTOR_ORIGIN));
    }
    if recruits.jet {
        interior_actor_meshes.push(("recruitjet", 0x86D7CF92, 0x01, ACTOR_ORIGIN));
    }
    for &(name, hash, state_bit, pos) in &interior_actor_meshes {
        if let Some((m, bmin, bmax)) = load_model_by_hash_state(w, hash, state_bit) {
            for c in 0..3 {
                wmin[c] = wmin[c].min(pos[c] + bmin[c]);
                wmax[c] = wmax[c].max(pos[c] + bmax[c]);
            }
            tv += m.verts.len();
            tt += m.indices.len() / 3;
            *distinct.entry(hash).or_insert(0) += 1;
            println!(
                "[interior] actor mesh '{name}' 0x{hash:08X}: {} v / {} t @ ({:.2},{:.2},{:.2}); FLOOR Y {:.2}",
                m.verts.len(), m.indices.len() / 3, pos[0], pos[1], pos[2], pos[1] + bmin[1]
            );
            out.push((m, pos, IDENT_QUAT));
        } else {
            println!("[interior] actor mesh '{name}' 0x{hash:08X}: NOT FOUND in vz.wad");
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
