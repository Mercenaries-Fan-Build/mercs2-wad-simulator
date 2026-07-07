//! The two ECS decal-toggle components — fully mapped (code map §3).
//!
//! Both register via the Keystone-A two-function pattern (registrar + stream deserializer) and are
//! **4-byte tags** whose presence on an entity suppresses decals for it:
//!
//! | component | m2 hash | registrar | deserializer | role |
//! |---|---|---|---|---|
//! | [`DisableDecals`]   | `0xff4533e5` | `FUN_00643bd0` (`PTR_00bc18d8`, stride 4) | `FUN_0063d060` | suppress **all** decal render on the entity |
//! | [`Disable3DDecals`] | `0x69a0e0e4` | `FUN_00643c80` (`PTR_00bc1928`, stride 4) | `FUN_0063d0d0` | disable the projected / "3D" decal pass |
//!
//! (Code map §3 note: `0xff4533e5` is *also* a config token via `FUN_00826820(0xff4533e5,0)` →
//! global bool `DAT_01175c37`, a parallel command-line switch — low confidence, not modelled here.)
//!
//! The stride-4 payload dword's exact semantics are the deserializer's `Read(buf,4,0)` word; the
//! component's *presence* is the recovered signal, so these carry the raw dword and default to `1`
//! (set/enabled-suppression).

use mercs2_core::{Entity, World};

/// `DisableDecals` (`0xff4533e5`, stride 4) — a 4-byte tag suppressing **all** decal render on the
/// entity it is attached to (code map §3).
pub const DISABLE_DECALS_HASH: u32 = 0xff45_33e5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisableDecals(pub u32);
impl Default for DisableDecals {
    fn default() -> Self {
        DisableDecals(1)
    }
}
impl DisableDecals {
    /// Whether the tag is active (nonzero payload = suppress).
    pub fn is_set(self) -> bool {
        self.0 != 0
    }
}

/// `Disable3DDecals` (`0x69a0e0e4`, stride 4) — a 4-byte tag disabling the projected ("3D") decal
/// pass on the entity (code map §3). Distinct from [`DisableDecals`]: this suppresses only the
/// projected/3D pass, not flat/screen decals.
pub const DISABLE_3D_DECALS_HASH: u32 = 0x69a0_e0e4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Disable3DDecals(pub u32);
impl Default for Disable3DDecals {
    fn default() -> Self {
        Disable3DDecals(1)
    }
}
impl Disable3DDecals {
    pub fn is_set(self) -> bool {
        self.0 != 0
    }
}

/// Whether `entity` suppresses **all** decals — carries an active [`DisableDecals`] tag. A spawn
/// request against such an entity is dropped (the engine skips its decal render).
pub fn suppresses_all_decals(world: &World, entity: Entity) -> bool {
    world
        .get::<&DisableDecals>(entity)
        .map(|c| c.is_set())
        .unwrap_or(false)
}

/// Whether `entity` suppresses the **projected/3D** decal pass — carries an active [`Disable3DDecals`]
/// tag. Projected decals (the ones this crate's pool manages) are dropped against such an entity.
pub fn suppresses_3d_decals(world: &World, entity: Entity) -> bool {
    world
        .get::<&Disable3DDecals>(entity)
        .map(|c| c.is_set())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_component_hashes() {
        assert_eq!(DISABLE_DECALS_HASH, 0xff45_33e5);
        assert_eq!(DISABLE_3D_DECALS_HASH, 0x69a0_e0e4);
    }

    #[test]
    fn tags_default_set_and_report_presence() {
        let mut world = World::new();
        let plain = world.spawn((0u8,)); // no tag
        let no_decals = world.spawn((DisableDecals::default(),));
        let no_3d = world.spawn((Disable3DDecals::default(),));

        assert!(!suppresses_all_decals(&world, plain));
        assert!(suppresses_all_decals(&world, no_decals));

        assert!(!suppresses_3d_decals(&world, plain));
        assert!(suppresses_3d_decals(&world, no_3d));
        // DisableDecals is the all-pass tag; it does not by itself set the 3D-only query.
        assert!(!suppresses_3d_decals(&world, no_decals));
    }

    #[test]
    fn zero_payload_is_inactive() {
        assert!(!DisableDecals(0).is_set());
        assert!(DisableDecals(1).is_set());
        assert!(!Disable3DDecals(0).is_set());
    }
}
