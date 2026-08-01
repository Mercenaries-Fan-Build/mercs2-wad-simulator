//! The domain spine (Plan 02 §C): seven curated lenses over the same catalog.
//!
//! One implementation, seven instances. Each domain carries a *lens* — a predicate over
//! [`index::AssetRow`], reusing the same `category()` / `vehicle_class()` classification the Library
//! browses by — plus the Lua scripts that govern it and the contribution kinds that make sense
//! there. Navigator = the filtered subject list; centre = the viewport; inspector = the same detail
//! the Library shows. Thin by design: the domains "start as thin browsers that thicken over time",
//! so this module owns only what makes a domain a domain — the lens and its metadata — and leaves
//! rendering to the pages that call it.
//!
//! Four domains (World, Characters, Weapons, Driving) filter the MODEL catalog by name/category.
//! The other three do not: Audio browses the audio catalog by asset Kind, and Missions/Systems are
//! Lua/Code surfaces whose subjects are scripts and hosts, not model rows — [`Domain::model_lens`]
//! returns `false` for those, and their pages source their subjects elsewhere.

use crate::index::AssetRow;

/// One curated view of the game's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    /// Environment: buildings, props, scenery, world state.
    World,
    /// Heroes, mercs, civilians — anything worn on a person.
    Characters,
    /// Guns and ordnance.
    Weapons,
    /// Everything with wheels, tracks, rotors or a hull.
    Driving,
    /// Wavebanks, soundbanks, sound tables.
    Audio,
    /// Contracts, jobs, objectives — the Lua that drives play.
    Missions,
    /// The engine layer: native hooks, placed files, the framework scripts.
    Systems,
}

impl Domain {
    /// Every domain, in rail order.
    pub const ALL: [Domain; 7] = [
        Domain::World,
        Domain::Characters,
        Domain::Weapons,
        Domain::Driving,
        Domain::Audio,
        Domain::Missions,
        Domain::Systems,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Domain::World => "World",
            Domain::Characters => "Characters",
            Domain::Weapons => "Weapons",
            Domain::Driving => "Driving",
            Domain::Audio => "Audio",
            Domain::Missions => "Missions",
            Domain::Systems => "Systems",
        }
    }

    /// A 2-char monogram for the rail — line-art-free (drawn as text by `RailIcon::Glyph`).
    pub fn short(self) -> &'static str {
        match self {
            Domain::World => "Wd",
            Domain::Characters => "Ch",
            Domain::Weapons => "Wp",
            Domain::Driving => "Dr",
            Domain::Audio => "Au",
            Domain::Missions => "Ms",
            Domain::Systems => "Sy",
        }
    }

    /// One line for the domain's header — what it is, and that it is a thin browser today.
    pub fn blurb(self) -> &'static str {
        match self {
            Domain::World => "Buildings, props and world state — the environment you fight in.",
            Domain::Characters => "Heroes, mercs and civilians. Edit a rig from here.",
            Domain::Weapons => "Guns and ordnance.",
            Domain::Driving => "Vehicles of every class — the donor pool for a new ride.",
            Domain::Audio => "Wavebanks, soundbanks and sound tables.",
            Domain::Missions => "The contracts and jobs the game's Lua drives.",
            Domain::Systems => "The engine layer: native hooks, placed files, framework scripts.",
        }
    }

    /// Does this asset belong to this domain's MODEL lens? True only for the four domains that filter
    /// the model catalog; Audio/Missions/Systems source their subjects elsewhere and return `false`.
    ///
    /// The predicate reuses `AssetRow::category()` / `vehicle_class()` verbatim, so the domain view
    /// and the Library's own buckets can never disagree about what a thing is.
    pub fn model_lens(self, row: &AssetRow) -> bool {
        match self {
            Domain::Driving => row.vehicle_class().is_some(),
            Domain::Characters => row.category() == "Character",
            Domain::Weapons => row.category() == "Weapon",
            Domain::World => matches!(
                row.category(),
                "Building" | "Prop / scenery" | "World state"
            ),
            // Not model-catalog domains.
            Domain::Audio | Domain::Missions | Domain::Systems => false,
        }
    }

    /// Whether this domain browses the model catalog at all (vs. audio / scripts / hosts).
    pub fn browses_models(self) -> bool {
        matches!(
            self,
            Domain::World | Domain::Characters | Domain::Weapons | Domain::Driving
        )
    }

    /// Corpus path/name needles for the Lua that governs this domain — the scripts a domain edit is
    /// likely to touch. Empty where the domain is asset-only. Matched case-insensitively against a
    /// corpus entry's path by the caller.
    pub fn governing_scripts(self) -> &'static [&'static str] {
        match self {
            Domain::World => &["atmosphere", "world", "layer", "region"],
            Domain::Characters => &["wifpmcinterior", "outfit", "wardrobe", "player", "hero"],
            Domain::Weapons => &["weapon", "wpn", "ammo"],
            Domain::Driving => &["vehicle", "veh", "traffic", "drive"],
            Domain::Audio => &["sound", "audio", "music", "voice"],
            Domain::Missions => &["mission", "contract", "objective", "job", "con0", "vzacon"],
            Domain::Systems => &["bootstrap", "resident", "mrxtask", "mrxstate", "sys"],
        }
    }

    /// The contribution kinds that make sense in this domain — what "Add to Shipment" offers here.
    pub fn kinds(self) -> &'static [&'static str] {
        match self {
            Domain::World => &["add_model", "replace_texture", "add_texture", "edit_state_machine"],
            Domain::Characters => &["add_outfit", "add_model", "add_texture", "replace_texture"],
            Domain::Weapons => &["add_model", "replace_texture", "add_texture"],
            Domain::Driving => &["add_model", "replace_texture", "add_texture", "edit_state_machine"],
            Domain::Audio => &["add_sound"],
            Domain::Missions => &["patch_lua", "edit_stringdb"],
            Domain::Systems => &["native_hook", "place_file", "patch_lua", "add_ui", "raw"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str) -> AssetRow {
        AssetRow { hash: 0, block: 0, src: 0, name: Some(name.to_string()) }
    }

    /// The lenses sort real retail names into the domain the Library would also file them under —
    /// the whole point of reusing `category()` / `vehicle_class()` rather than a parallel rule.
    #[test]
    fn lenses_match_the_library_buckets() {
        let heli = row("al_veh_helicopter_havoc");
        let hero = row("pmc_hum_mattias");
        let bld = row("al_bld_hq_01");
        let wpn = row("us_wpn_rifle_m4");

        assert!(Domain::Driving.model_lens(&heli));
        assert!(Domain::Characters.model_lens(&hero));
        assert!(Domain::World.model_lens(&bld));
        assert!(Domain::Weapons.model_lens(&wpn));

        // Each is claimed by exactly ONE model domain — the lenses partition, they do not overlap.
        for r in [&heli, &hero, &bld, &wpn] {
            let claims = Domain::ALL.iter().filter(|d| d.model_lens(r)).count();
            assert_eq!(claims, 1, "{} claimed by {claims} domains", r.label());
        }
    }

    /// The non-model domains never claim a model row — their subjects come from audio / scripts /
    /// hosts, so a stray model must not appear in them.
    #[test]
    fn audio_missions_systems_take_no_model_rows() {
        let anything = row("al_veh_car_sedan");
        for d in [Domain::Audio, Domain::Missions, Domain::Systems] {
            assert!(!d.model_lens(&anything));
            assert!(!d.browses_models());
        }
    }

    /// An unnamed (hash-only) row belongs to no domain lens — there is nothing to classify on.
    #[test]
    fn an_unnamed_row_belongs_to_no_domain() {
        let bare = AssetRow { hash: 0xDEAD_BEEF, block: 0, src: 0, name: None };
        assert!(Domain::ALL.iter().all(|d| !d.model_lens(&bare)));
    }
}
