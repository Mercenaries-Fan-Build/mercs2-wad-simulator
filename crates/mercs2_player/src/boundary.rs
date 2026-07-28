//! The boundary cluster — the play-area fence (`player_code_map.md` §7).
//!
//! Ten cfuncs spanning `0x005DC160`–`0x005DD040`. Two things about it are load-bearing and neither is
//! obvious from the names:
//!
//! **1. It is server-authoritative.** `AddBoundary` (`0x005DC903`), `RemoveBoundary` (`0x005DCA33`) and
//! `RemoveAllBoundary` (`0x005DCB31`) all open with `cmp byte [0xdfbd77], 0` and, on a **client**, push
//! boolean `false` and return without doing the work. `DAT_00DFBD77` is `Net.IsClient` — the engine
//! names it itself, via five consecutive `Net` accessors over five consecutive bytes
//! (`Net.IsEnabled` `0xDFBD74` … `Net.IsServer` `0xDFBD78`), all published once per frame by
//! `FUN_006CECF0` from a single net-session role enum.
//!
//! Modelled here as [`NetAuthority`] passed in, rather than read from a global, so the crate stays
//! testable and does not reach into `mercs2_net`.
//!
//! **2. The state bits are on a sub-object, not the player.** `+0x4F5` (out of bounds) and `+0x4F7`
//! (warning zone) live on the object at `player+0x08` — the bodies do `mov eax,[eax+8]` *then*
//! `mov al,[eax+0x4f5]`. See [`crate::object::BoundaryState`].
//!
//! **What is behind a seam.** `SetOutBoundary` / `IsPositionOutBoundary` / `RemoveBoundary` delegate
//! their *storage* into the SecuROM-adjacent boundary module (`024E3AB0` set, `024E3A20` point-test,
//! `024E8030` remove), so the shape recovered here is the player-facing half: the set of boundaries and
//! the callback pair.

use crate::callbacks::{CallbackArg, CallbackRegistry, CallbackSlot};
use crate::object::PlayerObject;

/// The net-session role, as published to `DAT_00DFBD74`–`0xDFBD78` by `FUN_006CECF0` from
/// `[[edi+0x24] + 0x0C]`.
///
/// The boundary cfuncs only care about the client/not-client distinction, but the whole enum is
/// modelled because the same byte block backs `Net.IsEnabled`/`IsActive`/`IsLobby`/`IsClient`/`IsServer`
/// and splitting it would invite two sources of truth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NetAuthority {
    /// Not in a networked session — the single-player case, which is **not** a client, so boundary
    /// operations run.
    #[default]
    Standalone,
    /// Role enum `1` → `Net.IsClient`. Boundary mutations early-out with `false`.
    Client,
    /// Role enum `2` → `Net.IsServer`.
    Server,
    /// Role enum `4` → `Net.IsLobby`.
    Lobby,
}

impl NetAuthority {
    /// `DAT_00DFBD77` — whether boundary mutations must early-out.
    pub fn is_client(self) -> bool {
        self == NetAuthority::Client
    }

    /// Whether this role may perform a boundary mutation, i.e. the `je` at `0x005DCA48` is taken.
    pub fn may_mutate_boundaries(self) -> bool {
        !self.is_client()
    }
}

/// One boundary in the play-area fence. The storage lives behind the SecuROM boundary module in retail,
/// so the fields here are the ones the player-facing cfuncs supply and read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boundary {
    /// The GUID the boundary is registered under — what `GetAllBoundaryGuid` returns.
    pub guid: u64,
    /// Centre, world space.
    pub centre: [f32; 3],
    /// Radius in metres. `Pg.SetBoundaryRadius(38.5)` is the shipped Lua analogue.
    pub radius: f32,
}

impl Boundary {
    /// Whether `point` lies outside this boundary — the point-test `024E3A20` delegates to.
    pub fn excludes(&self, point: [f32; 3]) -> bool {
        let dx = point[0] - self.centre[0];
        let dy = point[1] - self.centre[1];
        let dz = point[2] - self.centre[2];
        (dx * dx + dy * dy + dz * dz) > self.radius * self.radius
    }
}

/// The per-player boundary set plus the callback pair at `player+0x380`/`+0x384`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoundarySet {
    boundaries: Vec<Boundary>,
    /// The single "out boundary" set by `SetOutBoundary` — distinct from the [`boundaries`] list, which
    /// `AddBoundary` appends to.
    out_boundary: Option<Boundary>,
}

impl BoundarySet {
    /// `Player.AddBoundary(...)` — `0x005DC900`.
    ///
    /// **Server-authoritative**: on a client this does nothing and returns `false`
    /// (`0x005DC903 cmp byte [0xdfbd77], 0`).
    pub fn add(&mut self, net: NetAuthority, b: Boundary) -> bool {
        if !net.may_mutate_boundaries() {
            return false;
        }
        self.boundaries.retain(|e| e.guid != b.guid);
        self.boundaries.push(b);
        true
    }

    /// `Player.RemoveBoundary(uGuid)` — `0x005DCA30`. Server-authoritative
    /// (`0x005DCA33` / `0x005DCA48 je 0x005DCA78`).
    pub fn remove(&mut self, net: NetAuthority, guid: u64) -> bool {
        if !net.may_mutate_boundaries() {
            return false;
        }
        let before = self.boundaries.len();
        self.boundaries.retain(|e| e.guid != guid);
        before != self.boundaries.len()
    }

    /// `Player.RemoveAllBoundary()` — `0x005DCB31`. Server-authoritative.
    pub fn remove_all(&mut self, net: NetAuthority) -> bool {
        if !net.may_mutate_boundaries() {
            return false;
        }
        self.boundaries.clear();
        self.out_boundary = None;
        true
    }

    /// `Player.SetOutBoundary(...)` — the single out-of-play fence.
    pub fn set_out_boundary(&mut self, net: NetAuthority, b: Boundary) -> bool {
        if !net.may_mutate_boundaries() {
            return false;
        }
        self.out_boundary = Some(b);
        true
    }

    /// `Player.GetOutBoundary()` — `None` pushes nil, keeping the shipped `if not uX` flow authentic.
    pub fn out_boundary(&self) -> Option<Boundary> {
        self.out_boundary
    }

    /// `Player.GetAllBoundaryGuid()` — the GUIDs, in registration order.
    pub fn all_guids(&self) -> Vec<u64> {
        self.boundaries.iter().map(|b| b.guid).collect()
    }

    /// `Player.IsPositionOutBoundary(x, y, z)` — the point-test.
    ///
    /// A position is out when the out-boundary excludes it, or when any registered boundary does. With
    /// no boundaries at all nothing is out, which is why the shipped default answer is `false`.
    pub fn is_position_out(&self, point: [f32; 3]) -> bool {
        if let Some(o) = self.out_boundary {
            if o.excludes(point) {
                return true;
            }
        }
        self.boundaries.iter().any(|b| b.excludes(point))
    }

    /// How many boundaries are registered.
    pub fn len(&self) -> usize {
        self.boundaries.len()
    }

    /// Whether no boundaries are registered.
    pub fn is_empty(&self) -> bool {
        self.boundaries.is_empty()
    }
}

/// `Player.IsBoundaryDeath(uCharacter)` — cfunc entry `0x005DD040` (§3 row 32).
///
/// ⚠ Takes a **character** handle: it resolves through `FUN_006CDB70` (`GetPlayerForCharacter`) at
/// `0x005DD0AC`, not the `Players` container — `mrxplayer.lua:342,349` pass `uChar`.
///
/// The body reads **two** things, and an earlier revision of this function read only the first:
/// * `byte [player+0x66] == 4` (`0x005DD0C3`) — the mode gate;
/// * the out-of-bounds bit on the sub-object at `player+0x08` — `0x005DD0D2 mov eax,[eax+8]` then
///   `[eax+0x4F5]`, the same double-deref `GetOutBoundary` and `IsInWarningZone` perform (§2.2).
///
/// Mode 4 alone is not boundary death: a player can be in that mode while inside the fence.
pub fn is_boundary_death(p: &PlayerObject) -> bool {
    p.mode == crate::object::PlayerMode::BOUNDARY_DEATH && p.boundary.out_of_bounds
}

/// Re-evaluate a player's boundary bits and fire its callback on a transition.
///
/// Retail keeps `out_of_bounds` / `in_warning_zone` on the sub-object at `player+0x08` and drives the
/// callback stored at `+0x380`/`+0x384`. The callback fires on the *edge*, not every tick — firing every
/// frame would flood a script that shows a "return to the battlefield" prompt.
///
/// `warning_margin` widens the fence to produce the warning band.
pub fn update_boundary_state(
    p: &mut PlayerObject,
    set: &BoundarySet,
    position: [f32; 3],
    warning_margin: f32,
    callbacks: &mut CallbackRegistry,
) {
    let out = set.is_position_out(position);
    // The warning band is the region that would be out if the fence were `warning_margin` tighter.
    let warn = !out && {
        let mut tightened = set.clone();
        for b in tightened.boundaries.iter_mut() {
            b.radius = (b.radius - warning_margin).max(0.0);
        }
        if let Some(o) = tightened.out_boundary.as_mut() {
            o.radius = (o.radius - warning_margin).max(0.0);
        }
        tightened.is_position_out(position)
    };

    let was_out = p.boundary.out_of_bounds;
    p.boundary.out_of_bounds = out;
    p.boundary.in_warning_zone = warn;

    if out != was_out {
        callbacks.fire(
            CallbackSlot::Boundary(p.slot),
            vec![CallbackArg::Guid(p.guid), CallbackArg::Bool(out)],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(guid: u64, radius: f32) -> Boundary {
        Boundary { guid, centre: [0.0, 0.0, 0.0], radius }
    }

    /// **Every boundary mutation early-outs on a client** and reports `false`. Standalone and server
    /// both proceed — single-player is not a client, so the fence must still work offline.
    #[test]
    fn boundary_mutations_are_server_authoritative() {
        for role in [NetAuthority::Standalone, NetAuthority::Server, NetAuthority::Lobby] {
            let mut s = BoundarySet::default();
            assert!(s.add(role, b(1, 10.0)), "{role:?} may mutate");
            assert_eq!(s.len(), 1);
        }

        let mut s = BoundarySet::default();
        assert!(!s.add(NetAuthority::Client, b(1, 10.0)), "a client early-outs with false");
        assert!(s.is_empty(), "and changes nothing");

        // Same for the other three mutators, against a set the server populated.
        let mut s = BoundarySet::default();
        s.add(NetAuthority::Server, b(1, 10.0));
        assert!(!s.remove(NetAuthority::Client, 1));
        assert!(!s.remove_all(NetAuthority::Client));
        assert!(!s.set_out_boundary(NetAuthority::Client, b(2, 5.0)));
        assert_eq!(s.len(), 1, "the client changed nothing");
    }

    /// With no fence, nothing is out — the reason the shipped neutral answer is `false`.
    #[test]
    fn an_empty_fence_excludes_nothing() {
        let s = BoundarySet::default();
        assert!(!s.is_position_out([1000.0, 0.0, 1000.0]));
    }

    /// The point-test honours both the `AddBoundary` list and the separate out-boundary.
    #[test]
    fn the_point_test_covers_both_kinds_of_boundary() {
        let mut s = BoundarySet::default();
        s.add(NetAuthority::Server, b(1, 10.0));
        assert!(!s.is_position_out([5.0, 0.0, 0.0]), "inside");
        assert!(s.is_position_out([50.0, 0.0, 0.0]), "outside the added boundary");

        let mut s = BoundarySet::default();
        s.set_out_boundary(NetAuthority::Server, b(9, 3.0));
        assert!(s.is_position_out([5.0, 0.0, 0.0]), "outside the out-boundary");
        assert_eq!(s.all_guids(), Vec::<u64>::new(), "the out-boundary is not in the guid list");
    }

    /// Re-adding the same guid replaces rather than duplicating.
    #[test]
    fn adding_the_same_guid_twice_replaces() {
        let mut s = BoundarySet::default();
        s.add(NetAuthority::Server, b(1, 10.0));
        s.add(NetAuthority::Server, b(1, 99.0));
        assert_eq!(s.len(), 1);
        assert!(!s.is_position_out([50.0, 0.0, 0.0]), "the second radius won");
    }

    /// `IsBoundaryDeath` reads **both** the `+0x66 == 4` mode gate and the out-of-bounds bit on the
    /// `+0x08` sub-object. Either alone is not boundary death.
    #[test]
    fn boundary_death_needs_the_mode_and_the_out_of_bounds_bit() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        assert!(!is_boundary_death(&p), "neither");

        p.mode = crate::object::PlayerMode::BOUNDARY_DEATH;
        assert!(!is_boundary_death(&p), "mode 4 while INSIDE the fence is not death");

        p.boundary.out_of_bounds = true;
        assert!(is_boundary_death(&p), "mode 4 AND out of bounds");

        p.mode = crate::object::PlayerMode(3);
        assert!(!is_boundary_death(&p), "out of bounds in another mode is not death either");
    }

    /// The callback fires on the **edge**, not every tick — and it carries the new state.
    #[test]
    fn the_boundary_callback_fires_on_transitions_only() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        let mut set = BoundarySet::default();
        set.add(NetAuthority::Server, b(1, 10.0));
        let mut cbs = CallbackRegistry::default();
        let id = cbs.mint();
        cbs.bind(CallbackSlot::Boundary(0), id);

        // Inside, twice: no transition, no fire.
        update_boundary_state(&mut p, &set, [0.0, 0.0, 0.0], 2.0, &mut cbs);
        update_boundary_state(&mut p, &set, [1.0, 0.0, 0.0], 2.0, &mut cbs);
        assert!(cbs.take_fires().is_empty());

        // Step outside: one fire carrying `true`.
        update_boundary_state(&mut p, &set, [50.0, 0.0, 0.0], 2.0, &mut cbs);
        let fires = cbs.take_fires();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].args, vec![CallbackArg::Guid(0x2), CallbackArg::Bool(true)]);
        assert!(p.boundary.out_of_bounds);

        // Stay outside: no further fires.
        update_boundary_state(&mut p, &set, [60.0, 0.0, 0.0], 2.0, &mut cbs);
        assert!(cbs.take_fires().is_empty(), "still out is not a transition");

        // Return: one fire carrying `false`.
        update_boundary_state(&mut p, &set, [0.0, 0.0, 0.0], 2.0, &mut cbs);
        let fires = cbs.take_fires();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].args[1], CallbackArg::Bool(false));
        assert!(!p.boundary.out_of_bounds);
    }

    /// The warning zone is the band just inside the fence, and is mutually exclusive with being out.
    #[test]
    fn the_warning_zone_is_the_band_inside_the_fence() {
        let mut p = PlayerObject::joined_local(0, 0x2);
        let mut set = BoundarySet::default();
        set.add(NetAuthority::Server, b(1, 10.0));
        let mut cbs = CallbackRegistry::default();

        update_boundary_state(&mut p, &set, [0.0, 0.0, 0.0], 2.0, &mut cbs);
        assert!(!p.boundary.in_warning_zone, "well inside");

        update_boundary_state(&mut p, &set, [9.0, 0.0, 0.0], 2.0, &mut cbs);
        assert!(p.boundary.in_warning_zone, "within 2 m of the 10 m fence");
        assert!(!p.boundary.out_of_bounds);

        update_boundary_state(&mut p, &set, [50.0, 0.0, 0.0], 2.0, &mut cbs);
        assert!(p.boundary.out_of_bounds);
        assert!(!p.boundary.in_warning_zone, "out and warning are mutually exclusive");
    }
}
