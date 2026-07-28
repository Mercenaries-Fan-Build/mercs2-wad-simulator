//! Player-related ECS components (`player_code_map.md` §6).
//!
//! Four reflection components hang off the player concern. Each carries the **registrar** that installs
//! it, the **container** global it lives in, and its **element stride** — read from the binary, not
//! inferred, with the container name taken via the `vtable+0x34` master key rather than guessed from
//! the registrar.
//!
//! # ⚠ Container parameters are runtime state, not contracts
//!
//! Registrars publish *initial* values that the engine then **re-parameterises at runtime**. The proof
//! is `ControllerPlayer`: `FUN_00640410` registers it with capacity `0x100`
//! (`0x00640421 mov ecx, 0x100`), yet the same word in the dumped image reads **`0x60`**, with shift 5
//! instead of 8. The re-parameteriser takes the container in a register, so an absolute-address scan
//! cannot even find it.
//!
//! So the capacity/shift constants below are named `*_INITIAL_*` and are **documentation, not
//! invariants**. The map's §10.11 states the rule plainly: *do not hardcode a container's
//! capacity/stride/shift from its registrar*. Stride is the one value stable enough to assert on.

use mercs2_core::Entity;

/// `ControllerPlayer` — the input→control binding block.
///
/// Registrar `FUN_00640410`, container `0x017BCEF8`, element `0x0C`. The 12-byte payload's exact layout
/// is **unknown** (`docs/mercs2-ecs/03_controllers_physics.md:78` calls it "plausibly a vec3"), so this
/// models the component's *presence and identity*, not a byte image — consistent with this workspace's
/// rule that the exe is the oracle for behaviour, not for the byte layout of a runtime instance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControllerPlayer {
    /// The player slot this control binding belongs to.
    pub slot: u8,
}

pub const CONTROLLER_PLAYER_REGISTRAR: u32 = 0x0064_0410;
pub const CONTROLLER_PLAYER_CONTAINER: u32 = 0x017B_CEF8;
pub const CONTROLLER_PLAYER_STRIDE: usize = 0x0C;
/// Registered capacity — **initial**, and observed as `0x60` in the dump. See the module docs.
pub const CONTROLLER_PLAYER_INITIAL_CAPACITY: usize = 0x100;

/// `VehicleDisguiseScale` — disguise falloff tuning.
///
/// Registrar `FUN_006413F0`, container `0x017BD5D8`, element `0x0C` = three floats
/// (`docs/mercs2-ecs/04_player_vehicle_human.md:63`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleDisguiseScale {
    /// The three disguise scale factors. Their individual meanings are not recovered; the count and
    /// the type are.
    pub scale: [f32; 3],
}

pub const VEHICLE_DISGUISE_SCALE_REGISTRAR: u32 = 0x0064_13F0;
pub const VEHICLE_DISGUISE_SCALE_CONTAINER: u32 = 0x017B_D5D8;
pub const VEHICLE_DISGUISE_SCALE_STRIDE: usize = 0x0C;

/// `GrappleParameters` — grapple/winch tunables.
///
/// Registrar `FUN_00643D50`, container `0x017BE848`, element `0x1C`. Pairs with the per-player
/// grapple-enabled byte at `player+0x158` (`SetGrappleEnabled` `0x005DFC85`), which is the gate; these
/// are the tunables it gates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GrappleParameters {
    /// The 0x1C-byte tuning block's fields are not individually recovered. Modelled as opaque so the
    /// component can be carried and round-tripped without inventing field names.
    pub raw: [u8; GRAPPLE_PARAMETERS_STRIDE],
}

pub const GRAPPLE_PARAMETERS_REGISTRAR: u32 = 0x0064_3D50;
pub const GRAPPLE_PARAMETERS_CONTAINER: u32 = 0x017B_E848;
pub const GRAPPLE_PARAMETERS_STRIDE: usize = 0x1C;

/// `ModelMixerProfile` — costume/upgrade persistence.
///
/// Registrar `FUN_00643A40`, container `0x017BE708`, element **4**. The container and element were
/// unknown until the `[[0x017BE708]+0x34] = FUN_00643AE0` naming closed them; the registrar writes that
/// container's vtable at `0x00643A93`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelMixerProfile {
    /// The 4-byte profile word — the costume/upgrade selection this entity renders with.
    pub profile: u32,
}

pub const MODEL_MIXER_PROFILE_REGISTRAR: u32 = 0x0064_3A40;
pub const MODEL_MIXER_PROFILE_CONTAINER: u32 = 0x017B_E708;
pub const MODEL_MIXER_PROFILE_STRIDE: usize = 4;

/// The six `Controller*` containers `GetControlBindingType` (`0x005DD430`) probes with `player+0x24`,
/// **in this order**, to turn a control source into a type string.
///
/// Order is load-bearing: the cfunc returns on the first container that holds the key, so a reimpl that
/// probes in a different order reports a different type for an entity carrying more than one.
pub const CONTROL_BINDING_TYPES: [(u32, &str); 6] = [
    (0x017B_CF98, "car"),
    (0x017B_D038, "tank"),
    (0x017B_D0D8, "helicopter"),
    (0x017B_D088, "livingworld"),
    (0x017B_CFE8, "boat"),
    (0x017B_D128, "ladder"),
];

/// Resolve a control source to its binding-type string by probing the six `Controller*` containers in
/// retail's order.
///
/// `has_component` answers "does this entity carry the component in container `c`" — supplied by the
/// caller because this crate does not own those containers (they are vehicle/world components).
/// Returns `None` when no container claims it, which is the on-foot answer.
pub fn control_binding_type(
    entity: Option<Entity>,
    mut has_component: impl FnMut(Entity, u32) -> bool,
) -> Option<&'static str> {
    let e = entity?;
    CONTROL_BINDING_TYPES.iter().find(|(c, _)| has_component(e, *c)).map(|(_, name)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The strides are the one container number stable enough to assert on, and they match the sizes
    /// each component's field set implies.
    #[test]
    fn strides_match_the_modelled_payloads() {
        assert_eq!(VEHICLE_DISGUISE_SCALE_STRIDE, 3 * std::mem::size_of::<f32>(), "three floats");
        assert_eq!(MODEL_MIXER_PROFILE_STRIDE, std::mem::size_of::<u32>(), "a single 4-byte word");
        assert_eq!(GRAPPLE_PARAMETERS_STRIDE, 0x1C);
        assert_eq!(CONTROLLER_PLAYER_STRIDE, 0x0C);
        // The one that proves the module docs' point: the registered capacity is NOT what the dump
        // holds, so it must never be treated as an invariant.
        assert_eq!(CONTROLLER_PLAYER_INITIAL_CAPACITY, 0x100);
        assert_ne!(
            CONTROLLER_PLAYER_INITIAL_CAPACITY, 0x60,
            "the dump reads 0x60 — registrar constants are initial values, not contracts"
        );
    }

    /// The six control-binding containers are distinct and probed in retail's order.
    #[test]
    fn control_binding_probe_order_is_the_recovered_one() {
        let names: Vec<&str> = CONTROL_BINDING_TYPES.iter().map(|(_, n)| *n).collect();
        assert_eq!(names, ["car", "tank", "helicopter", "livingworld", "boat", "ladder"]);
        let mut seen = std::collections::HashSet::new();
        for (c, _) in CONTROL_BINDING_TYPES {
            assert!(seen.insert(c), "container {c:#x} listed twice");
        }
    }

    /// The probe returns the **first** matching container, and `None` on foot.
    #[test]
    fn control_binding_type_returns_the_first_match() {
        let mut w = mercs2_core::World::new();
        let e = w.spawn((7u8,));

        assert_eq!(control_binding_type(None, |_, _| true), None, "no control source -> on foot");
        assert_eq!(control_binding_type(Some(e), |_, _| false), None, "in nothing -> on foot");

        // Carried by both `tank` and `boat`: the earlier probe wins.
        let both = |_: Entity, c: u32| c == 0x017B_D038 || c == 0x017B_CFE8;
        assert_eq!(control_binding_type(Some(e), both), Some("tank"));
    }
}
