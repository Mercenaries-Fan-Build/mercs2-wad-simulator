//! The eight faction identities + the recovered initial-relation policy (code map §3).
//!
//! `mrxfactionmanager.lua` keys the relation matrix by faction **GUID**
//! (`tFactionData.uGuid = Pg.GetGuidByName(sFactionTemplate)`). The crate is otherwise GUID-agnostic
//! (`u32` keys), but the eight template names, their abbreviations, the mutable-attitude gate, and the
//! initial relations are recovered policy, so we pin them here.

use mercs2_formats::hash::pandemic_hash_m2;

/// The eight faction templates (`_tFactions :66`), in the [`crate::components::Suspect`] slot order
/// (8 factions × 1 dword). Abbreviations: All=Allied, Chi=China, Civ=Civ, Gur=Guerilla, Oil=OC,
/// Pir=Pirate, Pmc=PMC, Vza=VZ.
pub const FACTION_TEMPLATES: [&str; 8] =
    ["Allied", "China", "Civ", "Guerilla", "OC", "Pirate", "PMC", "VZ"];

/// The three-letter abbreviations paired with [`FACTION_TEMPLATES`] (same order).
pub const FACTION_ABBREVS: [&str; 8] = ["All", "Chi", "Civ", "Gur", "Oil", "Pir", "Pmc", "Vza"];

/// The **mutable-attitude** factions (`CanAttitudeBeMutable :512`): only these get a HUD meter and can
/// change their attitude toward the PMC. Civ / PMC / VZ are fixed.
pub const DYNAMIC_FACTIONS: [&str; 5] = ["Allied", "China", "Guerilla", "OC", "Pirate"];

/// Whether a faction template has a mutable attitude (a meter + can change vs the PMC).
pub fn is_dynamic(template: &str) -> bool {
    DYNAMIC_FACTIONS.iter().any(|t| t.eq_ignore_ascii_case(template))
}

/// The faction GUID key for a template name. The engine resolves this via `Pg.GetGuidByName`, a
/// name-registry lookup; the m2 hash of the template name is the faithful hashing primitive behind
/// that registry, so we use it as the GUID key here.
///
/// **Honesty:** `Pg.GetGuidByName` may return a registry *handle* rather than the raw m2 hash (the
/// name registry's value bit-31 can flag a template handle). If the game supplies real GUIDs, feed
/// those to [`crate::FactionWorld`] instead — this helper is the standalone default.
pub fn faction_guid(template: &str) -> u32 {
    pandemic_hash_m2(template)
}

/// The self-relation every faction holds toward itself (`:73` etc.) — `+100`.
pub const SELF_RELATION: i32 = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_templates_and_abbrevs_aligned() {
        assert_eq!(FACTION_TEMPLATES.len(), 8);
        assert_eq!(FACTION_ABBREVS.len(), 8);
        assert_eq!(FACTION_TEMPLATES[6], "PMC");
        assert_eq!(FACTION_ABBREVS[6], "Pmc");
    }

    #[test]
    fn dynamic_gate() {
        assert!(is_dynamic("Allied"));
        assert!(is_dynamic("oc")); // case-insensitive
        assert!(!is_dynamic("Civ"));
        assert!(!is_dynamic("PMC"));
        assert!(!is_dynamic("VZ"));
    }

    #[test]
    fn guids_are_distinct_and_stable() {
        let guids: Vec<u32> = FACTION_TEMPLATES.iter().map(|t| faction_guid(t)).collect();
        for (i, a) in guids.iter().enumerate() {
            for (j, b) in guids.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "faction guids must be distinct");
                }
            }
        }
        assert_eq!(faction_guid("PMC"), pandemic_hash_m2("PMC"));
    }
}
