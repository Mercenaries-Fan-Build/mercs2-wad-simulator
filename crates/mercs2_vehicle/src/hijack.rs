//! The vehicle **hijack** state machine + **turret aim**, the two runtime models behind the
//! `Vehicle.Hijack*` / `Vehicle.SetTurret*` / `Vehicle.SpinHeli` Lua surface.
//!
//! Hijacking in Mercs 2 is script-driven over an engine-owned FSM: the mission/`MrxVehicle` Lua posts
//! the lifecycle events (`HijackStart` → tank-motion → `SetHijackSuccess` → `HijackComplete`, or the
//! `HijackAbort`/`CancelHijack` branches) and the engine holds the state that gates the mount animation,
//! seat transfer, and hand-off of control. The verb vocabulary IS the transition set (vehicle code map
//! §1.2 seat/enter ring); there is no compiled planner — the engine owns the state, the Lua owns the
//! policy. This module is that owned state.

/// The hijack lifecycle state for one vehicle. `Idle` = not being hijacked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HijackState {
    /// Not being hijacked (terminal for `Cancel`/`AbortDone`).
    #[default]
    Idle,
    /// `HijackStart` — the hijack attempt has begun (approach/grab); the mount animation is playing.
    Started,
    /// `StartTankHijackMotion` — the tank-specific hijack motion (climb/hatch) is running.
    TankMotion,
    /// `SetHijackSuccess` — the attempt succeeded; awaiting `HijackComplete` to hand over control.
    Succeeded,
    /// `HijackComplete` — control handed to the hijacker (terminal success).
    Complete,
    /// `HijackAbort` — the attempt is unwinding (dismount/eject animation).
    Aborting,
}

impl HijackState {
    /// Human-readable name, matching the Lua `SetHijackState` string vocabulary.
    pub fn name(self) -> &'static str {
        match self {
            HijackState::Idle => "idle",
            HijackState::Started => "started",
            HijackState::TankMotion => "tank_motion",
            HijackState::Succeeded => "succeeded",
            HijackState::Complete => "complete",
            HijackState::Aborting => "aborting",
        }
    }

    /// Parse a `SetHijackState(name)` string back to a state (unknown ⇒ `None`).
    pub fn from_name(name: &str) -> Option<HijackState> {
        Some(match name.to_ascii_lowercase().as_str() {
            "idle" => HijackState::Idle,
            "started" => HijackState::Started,
            "tank_motion" | "tankmotion" => HijackState::TankMotion,
            "succeeded" | "success" => HijackState::Succeeded,
            "complete" | "completed" => HijackState::Complete,
            "aborting" | "abort" => HijackState::Aborting,
            _ => return None,
        })
    }

    /// Is a hijack in progress (started but not yet terminal)?
    pub fn is_active(self) -> bool {
        matches!(self, HijackState::Started | HijackState::TankMotion | HijackState::Succeeded)
    }
}

/// The per-vehicle hijack FSM. Transitions are the `Vehicle.Hijack*` verbs; each returns the resulting
/// state so the host can report it back to Lua. Illegal transitions are ignored (state unchanged),
/// matching the engine's tolerant applier.
#[derive(Clone, Copy, Debug, Default)]
pub struct HijackFsm {
    pub state: HijackState,
    /// Whether the attempt was flagged successful (`SetHijackSuccess`), independent of `Complete`.
    pub success: bool,
}

impl HijackFsm {
    pub fn new() -> Self {
        Self::default()
    }

    /// `HijackStart` — begin an attempt (only from `Idle`; a re-start while active is ignored).
    pub fn start(&mut self) -> HijackState {
        if self.state == HijackState::Idle {
            self.state = HijackState::Started;
            self.success = false;
        }
        self.state
    }

    /// `StartTankHijackMotion` / `StopTankHijackMotion` — toggle the tank-climb motion sub-state (only
    /// meaningful while active). `on=false` returns to `Started`.
    pub fn tank_motion(&mut self, on: bool) -> HijackState {
        match (on, self.state) {
            (true, HijackState::Started) => self.state = HijackState::TankMotion,
            (false, HijackState::TankMotion) => self.state = HijackState::Started,
            _ => {}
        }
        self.state
    }

    /// `SetHijackSuccess` — flag success and advance to `Succeeded` (from an active attempt).
    pub fn set_success(&mut self) -> HijackState {
        if self.state.is_active() {
            self.success = true;
            self.state = HijackState::Succeeded;
        }
        self.state
    }

    /// `HijackComplete` — hand over control (terminal success). Idempotent once complete.
    pub fn complete(&mut self) -> HijackState {
        if self.state == HijackState::Succeeded || self.state == HijackState::Started
            || self.state == HijackState::TankMotion
        {
            self.success = true;
            self.state = HijackState::Complete;
        }
        self.state
    }

    /// `HijackAbort` — begin unwinding the attempt.
    pub fn abort(&mut self) -> HijackState {
        if self.state.is_active() {
            self.success = false;
            self.state = HijackState::Aborting;
        }
        self.state
    }

    /// `HijackAbortDone` — finish the abort, returning to `Idle`.
    pub fn abort_done(&mut self) -> HijackState {
        if self.state == HijackState::Aborting {
            self.state = HijackState::Idle;
        }
        self.state
    }

    /// `CancelHijack` — hard cancel from any non-complete state back to `Idle`.
    pub fn cancel(&mut self) -> HijackState {
        if self.state != HijackState::Complete {
            self.success = false;
            self.state = HijackState::Idle;
        }
        self.state
    }

    /// `SetHijackState(name)` — explicit state override (tolerant; unknown name ignored).
    pub fn set_state(&mut self, name: &str) -> HijackState {
        if let Some(s) = HijackState::from_name(name) {
            self.state = s;
            if s == HijackState::Complete || s == HijackState::Succeeded {
                self.success = true;
            }
        }
        self.state
    }
}

/// Turret / rotor articulation for a vehicle (`Vehicle.SetTurretPitch/Yaw`, `Vehicle.SpinHeli`). The
/// drive sim reads these as the turret/rotor targets; radians.
#[derive(Clone, Copy, Debug, Default)]
pub struct TurretAim {
    /// Turret yaw (heading) target, radians.
    pub yaw: f32,
    /// Turret pitch (elevation) target, radians.
    pub pitch: f32,
    /// Helicopter rotor spinning (`SpinHeli(true/false)`) — gates rotor visual + lift availability.
    pub rotor_spinning: bool,
}

impl TurretAim {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hijack_happy_path_starts_succeeds_completes() {
        let mut h = HijackFsm::new();
        assert_eq!(h.start(), HijackState::Started);
        assert!(h.state.is_active());
        assert_eq!(h.tank_motion(true), HijackState::TankMotion);
        assert_eq!(h.tank_motion(false), HijackState::Started);
        assert_eq!(h.set_success(), HijackState::Succeeded);
        assert!(h.success);
        assert_eq!(h.complete(), HijackState::Complete);
        // Complete is terminal: cancel cannot undo a completed hijack.
        assert_eq!(h.cancel(), HijackState::Complete);
    }

    #[test]
    fn hijack_abort_and_cancel_return_to_idle() {
        let mut h = HijackFsm::new();
        h.start();
        assert_eq!(h.abort(), HijackState::Aborting);
        assert_eq!(h.abort_done(), HijackState::Idle);
        assert!(!h.success);

        let mut h2 = HijackFsm::new();
        h2.start();
        h2.tank_motion(true);
        assert_eq!(h2.cancel(), HijackState::Idle);
    }

    #[test]
    fn set_state_is_tolerant() {
        let mut h = HijackFsm::new();
        assert_eq!(h.set_state("succeeded"), HijackState::Succeeded);
        assert!(h.success);
        assert_eq!(h.set_state("bogus"), HijackState::Succeeded); // unknown ignored
    }
}
