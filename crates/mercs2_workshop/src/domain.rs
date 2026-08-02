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

    /// The rail's hand-drawn icon for this domain — a shape the eye reads faster than a monogram.
    pub fn rail_icon(self) -> crate::gui::theme::RailIcon {
        use crate::gui::theme::RailIcon;
        match self {
            Domain::World => RailIcon::World,
            Domain::Characters => RailIcon::Characters,
            Domain::Weapons => RailIcon::Weapons,
            Domain::Driving => RailIcon::Driving,
            Domain::Audio => RailIcon::Audio,
            Domain::Missions => RailIcon::Missions,
            Domain::Systems => RailIcon::Systems,
        }
    }

    /// A 2-char monogram for the rail — the fallback label, still shown under the icon.
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
            Domain::World => "Buildings, props and world state — including the vz_state overlays you \
                              move (edit_world) and switch on (activate_layer).",
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

    /// Whether this domain also browses the world-state placement layers (`index::Kind::Layer`).
    /// World alone does: the `vz_state` overlays are its subject as much as buildings and props are.
    pub fn browses_layers(self) -> bool {
        matches!(self, Domain::World)
    }

    /// Whether this domain browses the AUDIO catalog (`index::Kind::Audio`). Audio's subjects are the
    /// wavebanks / soundbanks / sound tables the game ships, not model rows.
    pub fn browses_audio(self) -> bool {
        matches!(self, Domain::Audio)
    }

    /// Whether this domain browses the decompiled Lua CORPUS. Missions and Systems are script/host
    /// surfaces — their subjects are the contracts, jobs and framework modules that drive play, keyed
    /// off [`governing_scripts`](Domain::governing_scripts) matched against each script's path.
    pub fn browses_scripts(self) -> bool {
        matches!(self, Domain::Missions | Domain::Systems)
    }

    /// Does a corpus script at `path` belong to this domain's script lens? True when the path contains
    /// any governing needle (case-insensitive). Only meaningful for the script domains.
    pub fn script_lens(self, path: &str) -> bool {
        let p = path.to_ascii_lowercase();
        self.governing_scripts().iter().any(|n| p.contains(n))
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
            Domain::World => &[
                "add_model",
                "replace_texture",
                "add_texture",
                "edit_state_machine",
                "edit_world",
                "activate_layer",
            ],
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

    /// World owns the vz_state overlays: it offers the two world-scale kinds and is the one domain
    /// that browses the layer catalog. Without this, the kinds ship but the domain never routes them.
    #[test]
    fn world_routes_the_overlay_kinds_and_alone_browses_layers() {
        assert!(Domain::World.kinds().contains(&"edit_world"));
        assert!(Domain::World.kinds().contains(&"activate_layer"));
        assert!(Domain::World.browses_layers());
        assert!(Domain::ALL.iter().filter(|d| d.browses_layers()).count() == 1);
    }

    /// An unnamed (hash-only) row belongs to no domain lens — there is nothing to classify on.
    #[test]
    fn an_unnamed_row_belongs_to_no_domain() {
        let bare = AssetRow { hash: 0xDEAD_BEEF, block: 0, src: 0, name: None };
        assert!(Domain::ALL.iter().all(|d| !d.model_lens(&bare)));
    }
}
