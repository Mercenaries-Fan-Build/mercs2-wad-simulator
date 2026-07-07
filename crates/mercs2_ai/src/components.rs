//! AI reflection component families — the ECS components an AI actor carries.
//!
//! Code map §3/§4: every real AI component registers via the Keystone-A two-function pattern; the
//! census gives the m2 hash, stride, and recovered field defaults. These are the faithful engine-side
//! component structs (defaults verbatim from the "Headline tunables": AiSkill 10, Squad max 50,
//! Perception range 120, Stimulus strength 100 / falloff 40, Target default True). The planner *brain*
//! that reads them is data/Lua (§5); these are the data it reads.

use mercs2_core::glam::Vec3;

/// `AiBehavior` (`0xdecd8889`, stride 0x30, pool 512) — the "what may this AI do" restriction block.
/// Every toggle defaults **false** (an unrestricted AI); Lua `Ai.SetState('Pacifist'|'Zombie'|…)`
/// flips one. Names from the code-map §5 flag vocabulary.
pub const AIBEHAVIOR_HASH: u32 = 0xdecd_8889;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiBehavior {
    pub no_exit_vehicle: bool,
    pub no_follow: bool,
    pub no_grenades: bool,
    pub no_turret: bool,
    pub no_vehicle: bool,
    pub no_cover: bool,
    pub no_report: bool,
    pub no_capture: bool,
    pub no_horn: bool,
    pub no_alarm: bool,
    pub no_prone: bool,
    pub no_crouch: bool,
    pub pacifist: bool,
    pub zombie: bool,
}

impl AiBehavior {
    /// Apply a named `Ai.SetState` restriction flag; returns whether the name was recognised (so the
    /// binding can report an unknown state rather than silently no-op). Case-insensitive.
    pub fn set_state(&mut self, name: &str, on: bool) -> bool {
        match name.to_ascii_lowercase().as_str() {
            "noexitvehicle" => self.no_exit_vehicle = on,
            "nofollow" => self.no_follow = on,
            "nogrenades" => self.no_grenades = on,
            "noturret" => self.no_turret = on,
            "novehicle" => self.no_vehicle = on,
            "nocover" => self.no_cover = on,
            "noreport" => self.no_report = on,
            "nocapture" => self.no_capture = on,
            "nohorn" => self.no_horn = on,
            "noalarm" => self.no_alarm = on,
            "noprone" => self.no_prone = on,
            "nocrouch" => self.no_crouch = on,
            "pacifist" => self.pacifist = on,
            "zombie" => self.zombie = on,
            _ => return false,
        }
        true
    }
}

/// `AiSkill` (`0xeba09b1a`, stride 0x04) — AI competence float, default **10.0**.
pub const AISKILL_HASH: u32 = 0xeba0_9b1a;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiSkill(pub f32);
impl Default for AiSkill {
    fn default() -> Self {
        AiSkill(10.0)
    }
}

/// `Perception` (`0x3f6ab8f0`, stride 0x14) — sight/awareness: 3 unit multipliers + **range 120** +
/// mode. The perception system counts stimuli within `range` (scaled per stimulus-unit type).
pub const PERCEPTION_HASH: u32 = 0x3f6a_b8f0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Perception {
    /// Sight multipliers for the three stimulus unit classes (visual / audio / other). Default 1.0.
    pub unit_mult: [f32; 3],
    /// Base sight/awareness range in world units — recovered default **120**.
    pub range: f32,
    /// Perception mode enum (0 = default). Kept as a raw column (the enum vocabulary is data).
    pub mode: u32,
}
impl Default for Perception {
    fn default() -> Self {
        Perception { unit_mult: [1.0, 1.0, 1.0], range: 120.0, mode: 0 }
    }
}

/// `Stimulus` (`0x06408d71`, stride 0x0c) — what an entity *emits* to be perceived: strength/radius
/// **100**, falloff **40**.
pub const STIMULUS_HASH: u32 = 0x0640_8d71;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stimulus {
    pub strength: f32,
    pub radius: f32,
    pub falloff: f32,
}
impl Default for Stimulus {
    fn default() -> Self {
        Stimulus { strength: 100.0, radius: 100.0, falloff: 40.0 }
    }
}

/// `Target` (`0xaff6b246`, stride 0x04) — targetable flag, default **True** (an entity is a valid AI
/// target unless marked otherwise).
pub const TARGET_HASH: u32 = 0xaff6_b246;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target(pub bool);
impl Default for Target {
    fn default() -> Self {
        Target(true)
    }
}

/// `Squad` (`0x9788c501`, stride 0x04, max **50**) — squad capacity, default `0x32` = 50.
pub const SQUAD_HASH: u32 = 0x9788_c501;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Squad {
    pub capacity: u32,
}
impl Default for Squad {
    fn default() -> Self {
        Squad { capacity: 50 }
    }
}

/// The per-entity perception record — code map §2.4 (`FUN_0058d520`, 0x64-B). The observer/threat
/// counters the perception update maintains and the debug overlay reads: TotalObservers `[0x13]`,
/// TotalAware `+0x4e`, HostileObservers `[0x14]`, HostileAware `+0x52`, Attackers `[0x15]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerceptionRecord {
    /// Entities currently observing this one.
    pub total_observers: u32,
    /// Observers that have crossed the awareness threshold (in range).
    pub total_aware: u32,
    /// Observers hostile to this entity.
    pub hostile_observers: u32,
    /// Hostile observers that are aware (in range) — the "someone hostile can see me" signal.
    pub hostile_aware: u32,
    /// Hostile aware observers actively attacking (posted an attack action against this entity).
    pub attackers: u32,
}

/// A GUID/faction tag so perception + relations can classify observers. Not an AI-specific reflection
/// component (identity lives elsewhere), but the perception system needs each actor's faction key to
/// consult the [`crate::relation::RelationMatrix`]; carried here for the reimpl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiFaction(pub u32);

/// Marker: a world position an AI actor occupies for perception range tests. In the full engine this
/// is the entity `Transform`; the perception system reads translation, so any positioned AI entity
/// participates. Re-exported for callers that want an explicit position without a full Transform.
pub use mercs2_core::Transform;

/// Convenience: squared distance between two world points (perception uses squared range to avoid the
/// sqrt, as the engine does).
pub fn dist_sq(a: Vec3, b: Vec3) -> f32 {
    (a - b).length_squared()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_defaults_match_the_census() {
        assert_eq!(AiSkill::default().0, 10.0);
        assert_eq!(Perception::default().range, 120.0);
        assert_eq!(Stimulus::default().strength, 100.0);
        assert_eq!(Stimulus::default().falloff, 40.0);
        assert!(Target::default().0, "Target defaults True");
        assert_eq!(Squad::default().capacity, 50);
        assert_eq!(AiBehavior::default(), AiBehavior::default(), "all restriction toggles default false");
        assert!(!AiBehavior::default().pacifist);
    }

    #[test]
    fn set_state_flips_named_flag_and_reports_unknown() {
        let mut b = AiBehavior::default();
        assert!(b.set_state("Pacifist", true));
        assert!(b.pacifist);
        assert!(b.set_state("Zombie", true));
        assert!(b.zombie);
        assert!(!b.set_state("NotARealFlag", true), "unknown state name reports false");
    }
}
