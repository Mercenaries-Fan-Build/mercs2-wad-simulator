//! The vehicle-control command ring — the transport between control verbs (player input, AI driving
//! states, Lua) and the per-class drive `HandleCommand` switch (`vehicle_code_map.md` §1).
//!
//! The retail PC rings are **standalone multi-consumer broadcast queues** (not fields inside the
//! per-class controller singletons — that was the Xbox layout). Each is a bounded ring of fixed
//! records with a per-record subscriber bitmask; a consumer registers a bit once (`FUN_004068d0`),
//! then each frame fetches records still carrying its bit, clears the bit, and the ring compacts
//! records nobody wants. **Every subscriber sees every command once** — so replay/HUD/network can
//! watch the same stream the drive model reads.
//!
//! Enqueue = `FUN_00538c90` (car/tank ring `0x011C0230`, cap 0x200) / `FUN_00538d20` (boat/heli ring
//! `0x011C2478`, cap 100). Record layout (`+0` target handle, `+4` command-id, `+8` float payload,
//! `+0xC` aux). The pump `FUN_00532f80` drains the rings each game-layer tick and dispatches by
//! command-id through the per-class switch.

use crate::components::{VehicleClass, VehicleControls};

/// Command IDs — bit-identical Xbox↔PC (`vehicle_code_map.md` §1.5). The hash function that produced
/// them is UNKNOWN, but the constants are exactly what the ring compares against, so we use them
/// verbatim. (CONFIRM-LIVE only for the *hash*, not the values.)
pub mod cmd {
    /// Turn/steer — car `+0x28`, heli `+0x100`, boat `+0x2C` (all as `1.0 − payload`).
    pub const TURN: u32 = 0x3483DBF1;
    /// Brake — car `+0x30`.
    pub const BRAKE: u32 = 0x55B8E0A1;
    /// Accel channel A — car fwd (`+0x2C`), heli `+0xFC`.
    pub const ACCEL_A: u32 = 0x0490757F;
    /// Accelerate — boat `+0x28`, heli `+0xF8`.
    pub const ACCELERATE: u32 = 0x7D3B632C;
    /// Combined accel/brake axis (car).
    pub const ACCEL_BRAKE: u32 = 0x460C5913;
    /// Handbrake — car `+0x38`.
    pub const HANDBRAKE: u32 = 0x574220AC;
    /// 5th car channel `+0x34` / heli 4th axis `+0x104`.
    pub const AUX5: u32 = 0x37086E0A;
    /// ClearControls — zeroes the drive-obj input fields (all classes).
    pub const CLEAR_CONTROLS: u32 = 0x6C5F1491;
    /// Position/focus ping.
    pub const POSITION_PING: u32 = 0x262E1E47;
    /// SpinHeli (Lua only), gated on `vt+0xE0()==4`.
    pub const SPIN_HELI: u32 = 0x30FBBF64;
    /// Skid/burnout notification (emitted by the wheel side-force monitor).
    pub const SKID: u32 = 0x9EAEC21D;
}

/// One 0x10-byte ring record (rings 4/5). Consumers key on `id`, filter on `target`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CommandRecord {
    /// `+0` — target vehicle handle ([`crate::components::Vehicle::handle`]).
    pub target: u32,
    /// `+4` — command id (see [`cmd`]).
    pub id: u32,
    /// `+8` — float payload.
    pub payload: f32,
    /// `+0xC` — aux/flags.
    pub aux: u32,
}

impl CommandRecord {
    pub fn new(target: u32, id: u32, payload: f32) -> Self {
        Self { target, id, payload, aux: 0 }
    }
}

/// A standalone multi-consumer broadcast ring (rings 4/5 of §1.2). Capacity is the exe's cap
/// (`0x200` car/tank, `100` boat/heli). Subscriber bits: up to 8 channels (`FUN_004068d0` allocates
/// `1<<n`, max 8).
pub struct CommandRing {
    cap: usize,
    /// Live records; `mask[i]` is the subscriber bitmask still carrying record `i`.
    records: Vec<CommandRecord>,
    mask: Vec<u8>,
    /// Bits currently allocated to consumers (`mask |= 1<<n`).
    allocated: u8,
    /// The `lock` byte (`DAT_011c2458`): while set, enqueues are dropped (the exe's guard). We keep
    /// it for fidelity; the win32 CriticalSection is elided (single sim thread).
    pub locked: bool,
}

impl CommandRing {
    /// The car/tank ring (`0x011C0230`, cap `0x200`).
    pub fn car() -> Self {
        Self::with_capacity(0x200)
    }
    /// The boat/heli ring (`0x011C2478`, cap `100`).
    pub fn boat_heli() -> Self {
        Self::with_capacity(100)
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            cap,
            records: Vec::new(),
            mask: Vec::new(),
            allocated: 0,
            locked: false,
        }
    }

    /// Register a consumer, returning its channel bit (`FUN_004068d0`: scan mask, `mask |= 1<<n`).
    /// Up to 8 channels; returns `None` when exhausted.
    pub fn subscribe(&mut self) -> Option<u8> {
        for n in 0..8u8 {
            let bit = 1u8 << n;
            if self.allocated & bit == 0 {
                self.allocated |= bit;
                return Some(bit);
            }
        }
        None
    }

    /// Enqueue a record to **all** current subscribers (`FUN_00538c90`/`FUN_00538d20`): if unlocked
    /// and under cap, append the record tagged with every allocated subscriber bit. Returns whether
    /// it was accepted.
    pub fn enqueue(&mut self, rec: CommandRecord) -> bool {
        if self.locked || self.records.len() >= self.cap {
            return false;
        }
        self.records.push(rec);
        self.mask.push(self.allocated);
        true
    }

    /// Drain the records still carrying `channel`, clearing the bit and compacting records nobody
    /// wants (the per-frame fetch, e.g. `FUN_0040ea90`). Records with other subscribers' bits
    /// survive for those consumers.
    pub fn drain_channel(&mut self, channel: u8) -> Vec<CommandRecord> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.records.len() {
            if self.mask[i] & channel != 0 {
                out.push(self.records[i]);
                self.mask[i] &= !channel;
            }
            if self.mask[i] == 0 {
                // nobody wants it anymore → compact
                self.records.remove(i);
                self.mask.remove(i);
            } else {
                i += 1;
            }
        }
        out
    }

    /// Live record count (for tests / diagnostics).
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Per-class `HandleCommand` switch — writes a drained record into the vehicle's drive-obj fields.
///
/// Car/tank = `FUN_00437300`, heli = `FUN_00435790`, boat = `FUN_00441900`. `payload` is the
/// record's `+8` float. Returns `true` if the id was recognised for this class.
pub fn handle_command(class: VehicleClass, ctrl: &mut VehicleControls, rec: &CommandRecord) -> bool {
    match class {
        VehicleClass::Car | VehicleClass::Bike | VehicleClass::Tank => {
            handle_command_car(ctrl, rec)
        }
        VehicleClass::Helicopter => handle_command_heli(ctrl, rec),
        VehicleClass::Boat => handle_command_boat(ctrl, rec),
        VehicleClass::Jet => false,
    }
}

/// Car/tank `HandleCommand` (`FUN_00437300`, drive obj `this[0x56]`).
fn handle_command_car(ctrl: &mut VehicleControls, rec: &CommandRecord) -> bool {
    match rec.id {
        // +0x28 = clamp(1.0 − payload) Turn. Signed steer (kept in [-1,1]); < 0 left, > 0 right.
        cmd::TURN => {
            ctrl.turn = (1.0 - rec.payload).clamp(-1.0, 1.0);
        }
        // +0x2C/+0x30 forward accel by reverse flag.
        cmd::ACCEL_A => {
            ctrl.accel = rec.payload;
        }
        cmd::ACCELERATE => {
            ctrl.accel = rec.payload;
        }
        // +0x30 Brake.
        cmd::BRAKE => {
            ctrl.brake = rec.payload;
        }
        // Combined accel/brake axis: payload > 0.5 ⇒ accel, else brake with the (1 − v) swap.
        cmd::ACCEL_BRAKE => {
            if rec.payload >= 0.5 {
                ctrl.accel = (rec.payload - 0.5) * 2.0;
                ctrl.brake = 0.0;
            } else {
                ctrl.brake = 1.0 - rec.payload * 2.0;
                ctrl.accel = 0.0;
            }
        }
        // +0x38 Handbrake.
        cmd::HANDBRAKE => {
            ctrl.handbrake = rec.payload;
        }
        // +0x34 (5th, open).
        cmd::AUX5 => {
            ctrl.aux5 = rec.payload;
        }
        // Zero +0x28..+0x38.
        cmd::CLEAR_CONTROLS => {
            ctrl.turn = 0.0;
            ctrl.accel = 0.0;
            ctrl.brake = 0.0;
            ctrl.aux5 = 0.0;
            ctrl.handbrake = 0.0;
        }
        cmd::POSITION_PING => { /* world-pos ping FUN_008d5ba0/b70 — no drive-state change */ }
        _ => return false,
    }
    true
}

/// Heli `HandleCommand` (`FUN_00435790`, drive obj `this+0x140`, 4 axes).
fn handle_command_heli(ctrl: &mut VehicleControls, rec: &CommandRecord) -> bool {
    match rec.id {
        cmd::ACCELERATE => ctrl.heli_lift = rec.payload, // +0xF8
        cmd::ACCEL_A => ctrl.heli_a = rec.payload,       // +0xFC
        cmd::TURN => ctrl.heli_yaw = rec.payload,        // +0x100
        cmd::AUX5 => ctrl.heli_b = rec.payload,          // +0x104
        cmd::CLEAR_CONTROLS => {
            ctrl.heli_lift = 0.0;
            ctrl.heli_a = 0.0;
            ctrl.heli_yaw = 0.0;
            ctrl.heli_b = 0.0;
        }
        _ => return false,
    }
    true
}

/// Boat `HandleCommand` (`FUN_00441900`, drive obj `this+0x140`, 2 axes).
fn handle_command_boat(ctrl: &mut VehicleControls, rec: &CommandRecord) -> bool {
    match rec.id {
        cmd::ACCELERATE => ctrl.boat_throttle = rec.payload, // +0x28
        cmd::TURN => ctrl.boat_turn = 1.0 - rec.payload,     // +0x2C as 1 − v
        cmd::CLEAR_CONTROLS => {
            ctrl.boat_throttle = 0.0;
            ctrl.boat_turn = 0.0;
        }
        _ => return false,
    }
    true
}
