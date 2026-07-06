//! GAME character identity — the three playable heroes, their look progression, and wardrobe data.
//!
//! Ground truth is the retail SPAWN path (`mrxplayer.lua:155-179`, `GetTemplateAndModelName`):
//! * the spawn TEMPLATE is `_tCharacterMap.templates[iUpgrade]` (`<hero>upgrade1..3`), or the
//!   base template at tier 0 — **the hero's look progresses with the upgrade tier**, because
//!   each template carries its own model. `iUpgrade` = profile header `@0x4F`
//!   (`Get/SetProfileUpgrade` = profile object `+0x62`).
//! * a wardrobe costume (`iCostume` = profile object `+0x63`, wardrobe stores `_tOutfits`
//!   index − 1) OVERRIDES the template model via `_tCharacterMap.models[iCostume]` (1-based);
//!   `0` = no override. Every observed save stores costume 0 (wardrobe never used), and the
//!   costume byte's FILE offset is therefore not yet locatable — the engine object offset is
//!   proven, the file byte is not.
//! * the HERO byte is header `@0x4D` (`Get/SetProfileCharacter` = object `+0x61`), values
//!   engine-coded 1 mattias / 2 chris / 3 jen (`FUN_00634810` → `SHELL.SelectCharacter.*`).
//!
//! Upgrade-template MODELS: tier 0 = the hero base model (verified in vz.wad). Mattias tier 3 =
//! the `pmc_hum_mattias_v3` "MetalHead" look (user-verified against the retail endgame save).
//! Tier 1/2 and Chris/Jen tier models are NOT yet extracted from their templates (registry
//! handles 0x8000A1BE.., docs/data/live_registry_hashes.csv) — they fall back to the base model
//! until that reflection walk is done.

pub struct Hero {
    /// `_tCharacterMap.base` id.
    pub base: &'static str,
    /// Hero display name (also the retail manual-save filename prefix).
    pub display: &'static str,
    /// The hero's base/Original model (costume byte 0).
    pub base_model: &'static str,
    /// DLC-extended `_tCharacterMap.models` in order: costume byte `k` (1-based) → `costumes[k-1]`.
    /// (display name, model name) — names follow the wardrobe `_tOutfits` alignment.
    pub costumes: &'static [(&'static str, &'static str)],
}

/// `_tCharacterMap` order — hero index 1..=3 (1-based).
pub const HEROES: [Hero; 3] = [
    Hero {
        base: "mattias",
        display: "Mattias Nilsson",
        base_model: "pmc_hum_mattias",
        costumes: &[
            ("MetalHead", "pmc_hum_mattias_v3"),
            ("Suit", "pmc_hum_mattias_v2"),
            ("Jacket", "pmc_hum_mattias_v4"),
            ("ChickenSuit", "pmc_hum_mattias_chickensuit"),
            ("Ewan", "pmc_hum_helipilot_unlockable"),
            ("Misha", "pmc_hum_proppilot_unlockable"),
            ("Obama", "pmc_hum_obama"),
            ("Palin", "pmc_hum_sarah"),
            ("GrandpaMattias", "pmc_hum_mattias_v5"),
            ("Starter", "gr_hum_starter_1"),
            ("Stealth", "pmc_hum_stealth"),
            ("Hoang", "pmc_hum_hoang"),
            ("BossFake", "gr_hum_boss_fake"),
            ("Pilot", "oc_hum_pilot"),
        ],
    },
    Hero {
        base: "chris",
        display: "Chris Jacobs",
        base_model: "pmc_hum_chris",
        costumes: &[
            ("Vacation", "pmc_hum_chris_v2"),
            ("Commando", "pmc_hum_chris_v3"),
            ("OffDuty", "pmc_hum_chris_v4"),
            ("ChickenSuit", "pmc_hum_chris_chickensuit"),
            ("PirateBoss", "pr_hum_boss"),
            ("Blanco", "pmc_hum_blanco"),
            ("Obama", "pmc_hum_obama"),
            ("Palin", "pmc_hum_sarah"),
        ],
    },
    Hero {
        base: "jen",
        display: "Jennifer Mui",
        base_model: "pmc_hum_jen",
        costumes: &[
            ("Rebel", "pmc_hum_jen_v3"),
            ("Tactical", "pmc_hum_jen_v5"),
            ("NoJacket", "pmc_hum_jen_v2"),
            ("CatSuit", "pmc_hum_jen_v4"),
            ("ChickenSuit", "pmc_hum_jen_chickensuit"),
            ("Fiona", "pmc_hum_fiona_unlockable"),
            ("Eva", "pmc_hum_mechanic"),
            ("Obama", "pmc_hum_obama"),
            ("Palin", "pmc_hum_sarah"),
            ("Diablo", "pmc_hum_diablo"),
        ],
    },
];

/// Hero for a 1-based index (out of range → Mattias, the retail default).
pub fn hero(character_index: u8) -> &'static Hero {
    HEROES
        .get((character_index as usize).wrapping_sub(1))
        .unwrap_or(&HEROES[0])
}

/// The model an upgrade TEMPLATE carries, where known. Tier 0 = the hero base model. Mattias
/// tier 3 = `pmc_hum_mattias_v3` (user-verified against the retail endgame save — the
/// "MetalHead" look). Other tiers/heroes fall back to the base model until their template
/// reflection blocks are walked (registry handles in docs/data/live_registry_hashes.csv).
fn upgrade_model(h: &'static Hero, upgrade: u8) -> &'static str {
    match (h.base, upgrade) {
        ("mattias", 3) => "pmc_hum_mattias_v3",
        _ => h.base_model,
    }
}

/// Look label for the save browser / boot banner: the wardrobe outfit when one is set, else the
/// upgrade tier's look.
pub fn look_label(character_index: u8, upgrade: u8, costume_byte: u8) -> String {
    let h = hero(character_index);
    if costume_byte > 0 {
        if let Some((n, _)) = h.costumes.get(costume_byte as usize - 1) {
            return (*n).to_string();
        }
    }
    match upgrade {
        0 => "default".to_string(),
        3 if h.base == "mattias" => "upgrade 3 (MetalHead)".to_string(),
        n => format!("upgrade {n}"),
    }
}

/// Ordered player-model candidates for a save, following the retail spawn rule
/// (`GetTemplateAndModelName`): wardrobe costume override first (`models[costume]`, 1-based,
/// 0 = none), else the upgrade template's model; then the hero base model, then the proven-good
/// render model (`pmc_hum_mattias_v3`) as the last resort — the loader tries each until one builds.
pub fn player_model_candidates(character_index: u8, upgrade: u8, costume_byte: u8) -> Vec<String> {
    let h = hero(character_index);
    let mut out: Vec<String> = Vec::new();
    let mut push = |m: &str| {
        if !out.iter().any(|x| x == m) {
            out.push(m.to_string());
        }
    };
    if costume_byte > 0 {
        if let Some((_, model)) = h.costumes.get(costume_byte as usize - 1) {
            push(model);
        }
    }
    push(upgrade_model(h, upgrade));
    push(h.base_model);
    push("pmc_hum_mattias_v3");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_tier_drives_the_look() {
        // User ground truth: 2.5M-cash mid save (upgrade 0) = DEFAULT skin;
        // completed endgame save (upgrade 3) = the MetalHead v3 look. Wardrobe untouched (0).
        assert_eq!(player_model_candidates(1, 0, 0)[0], "pmc_hum_mattias");
        assert_eq!(player_model_candidates(1, 3, 0)[0], "pmc_hum_mattias_v3");
        assert_eq!(look_label(1, 0, 0), "default");
        assert_eq!(look_label(1, 3, 0), "upgrade 3 (MetalHead)");
        // Jen fresh save (hero 3, upgrade 0) = her base model.
        assert_eq!(hero(3).base, "jen");
        assert_eq!(player_model_candidates(3, 0, 0)[0], "pmc_hum_jen");
    }

    #[test]
    fn wardrobe_costume_overrides_the_template() {
        // Retail rule: costume byte k (1-based into _tCharacterMap.models) overrides the
        // upgrade template's model; 0 = no override.
        assert_eq!(player_model_candidates(1, 3, 5)[0], "pmc_hum_helipilot_unlockable"); // "Ewan"
        assert_eq!(look_label(1, 3, 5), "Ewan");
        assert_eq!(player_model_candidates(3, 0, 1)[0], "pmc_hum_jen_v3"); // jen "Rebel"
    }

    #[test]
    fn out_of_range_falls_back() {
        assert_eq!(hero(0).base, "mattias");
        assert_eq!(hero(9).base, "mattias");
        let c = player_model_candidates(3, 0, 200); // jen, garbage costume → base then proven
        assert_eq!(c[0], "pmc_hum_jen");
        assert!(c.contains(&"pmc_hum_mattias_v3".to_string()));
    }
}
