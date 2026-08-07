//! The simple-spawner mechanism — `UpdateSimpleSpawners` (Xbox `FUN_82338768` ↔ PC `FUN_004e4100`).
//!
//! Code map §6 (H core). `PopulationSimpleSpawner` is a **class-manager, not a flat descriptor**
//! (which is why it never appeared in the 231-class registry TSVs): a 768-cap instance pool that the
//! per-frame update walks, dispatching **four family updaters** — Window / NoModel / Hardpoint / Path —
//! each draining its own **128-cap** pending-spawn queue. A spawner is a timer + a placement radius +
//! a target spawn list; when its countdown elapses it emits one **spawn request** (template + a
//! transform) which the game's spawn resolver later realizes into an entity (that terminal commit is
//! the SecuROM-VM-dispatched worker `0x24F3200` — a seam, not built here).
//!
//! Recovered instance field layout (PC, from the consumers, §6): `+0x20` guid, `+0x2c` adjust-target,
//! `+0x58` faction/list index, `+0x5c`/`+0x60`/`+0x6c` interval+countdown+reload, `+0x63`/`+0x64`/`+0x68`
//! the 3 type discriminators, `+0x78`/`+0x7c` radii (× scale `DAT_00b97eec`), `+0x89` state, `+0x8b`
//! group, `+0x8c` done. Terminal state byte = **5** (`SimpleSpawnerStateEnum` = 5 members); groups
//! **< 8**; Window radius² = **25600 = 160²**.

use mercs2_core::glam::Vec3;
use mercs2_core::Transform;

use crate::components::{SkirmishSpawnList, SpawnFaction};

/// Instance-pool cap (`cdbsizes.ini`, both builds) — `PopulationSimpleSpawner` = 768.
pub const SIMPLE_SPAWNER_POOL: u32 = 768;
/// `PopulationList` pool (`cdbsizes.ini`) — 1024.
pub const POPULATION_LIST_POOL: u32 = 1024;
/// `SpawnerAdjust` pool (`cdbsizes.ini`) — 16/16.
pub const SPAWNER_ADJUST_POOL: (u32, u32) = (16, 16);
/// `SpawnOnDeath` pool (PC image strings) — 384/128.
pub const SPAWN_ON_DEATH_POOL: (u32, u32) = (384, 128);

/// Terminal spawner state (`+0x89 == 5` ⇒ exhausted/removed). `SimpleSpawnerStateEnum` has 5 members,
/// so 5 is the one-past-the-last "done" sentinel the update tests against.
pub const SPAWNER_STATE_TERMINAL: u8 = 5;
/// Spawner group count — `+0x8b < 8` (the 8-group bit loop, both builds).
pub const SPAWNER_GROUP_COUNT: u8 = 8;
/// Category count — `SimpleSpawnerTypeEnum` = 4 (the 3 type discriminators `+0x63/+0x64/+0x68` fold to
/// 4 categories; the exact fold is a **confirm-live** item §10.2, so it is not synthesised here).
pub const SPAWNER_CATEGORY_COUNT: u8 = 4;
/// Each pending-spawn queue is capped at 128 (`0x80`) — 4 families × cap 128 (code map §6 / §9).
pub const SPAWN_QUEUE_CAP: usize = 0x80;
/// Window-spawner activation radius² = 160² (code map §6, both builds).
pub const WINDOW_RADIUS_SQ: f32 = 25600.0;

/// The four `UpdateSimpleSpawners` family updaters (Xbox lists `0x82C1F488/7B8/958/C88` ↔ PC procs
/// `FUN_004e1590/1ad0/2110/1d50` draining queues `DAT_00dccb00/ce30/cfd0/d300`).
///
/// **Confirm-live (§10.2):** the PC proc↔queue↔family pairing is by size/position, **not proven** —
/// the four procs are structurally interchangeable. Window is the safest anchor (its Xbox fn carries
/// the `'Have %d window Spawners'` string + the `OccupiedBuildingSpawnCallback` dispatch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnerFamily {
    /// Spawns units at building windows (`OccupiedBuildingSpawnCallback`); radius² 160².
    Window = 0,
    /// Spawns units with no placement model (ambient/off-screen).
    NoModel = 1,
    /// Spawns units at fixed hardpoints.
    Hardpoint = 2,
    /// Spawns units along a path.
    Path = 3,
}

impl SpawnerFamily {
    /// All four families in fixed dispatch order (the order `UpdateSimpleSpawners` fans out).
    pub const ALL: [SpawnerFamily; 4] = [
        SpawnerFamily::Window,
        SpawnerFamily::NoModel,
        SpawnerFamily::Hardpoint,
        SpawnerFamily::Path,
    ];
}

/// One emitted spawn request — the output of the spawner/density mechanism. It is **not** an engine
/// entity: the game's spawn resolver (the SecuROM-VM terminal worker, a seam) realizes it later. This
/// is the `0xBC`-byte spawn-record's essential payload: *what* to spawn, *where*, on whose list, in
/// which group. Requests carry the [`crate::SPAWN_EVENT_HASH`] `Event.Post` context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnRequest {
    /// Name-registry hash of the template to spawn (resolved via the `0xDF6B88` family →
    /// [[name-registry-spawn-by-hash]] on the engine side). 0 = "let the list pick" (data-driven).
    pub template: u32,
    /// Where to place the spawned unit — the spawner's world transform offset by its radius.
    pub transform: Transform,
    /// Which faction spawn-list channel this request draws from.
    pub faction: SpawnFaction,
    /// Which of the 8 spawner groups issued it (for `TweakAttachedSpawnersInGroup` targeting).
    pub group: u8,
    /// Emitting family.
    pub family: SpawnerFamily,
}

/// A `PopulationSimpleSpawner` pool instance — the recovered `+0x20…+0x8c` field layout as a struct.
/// A timer-driven emitter: while not terminal, its `countdown` decrements by `dt`; on reaching zero it
/// emits one [`SpawnRequest`] and reloads. This is the faithful mechanism the family updaters run over
/// their instances; the *decision of which template* comes from the attached [`SkirmishSpawnList`]
/// (data), not from a compiled body.
#[derive(Clone, Debug, PartialEq)]
pub struct SimpleSpawner {
    /// `+0x20` — this spawner's guid.
    pub guid: u32,
    /// `+0x2c` — the `SpawnerAdjust` target this spawner answers to (0 = none).
    pub adjust_target: u32,
    /// `+0x58` — faction/list index: which spawn-list channel the emitted units belong to.
    pub faction: SpawnFaction,
    /// `+0x5c` — reload **interval** in seconds between spawns.
    pub interval: f32,
    /// `+0x60` — **countdown** to the next spawn (seconds); decrements by `dt`.
    pub countdown: f32,
    /// `+0x6c` — **reload** value the countdown is reset to after a spawn (usually == `interval`).
    pub reload: f32,
    /// `+0x63/+0x64/+0x68` — the 3 raw type discriminators (fold to a `SimpleSpawnerType` category;
    /// the fold is confirm-live, so kept raw — see [`SPAWNER_CATEGORY_COUNT`]).
    pub type_disc: [u8; 3],
    /// `+0x78` — primary spawn/activation radius (× the runtime scale `DAT_00b97eec`).
    pub radius: f32,
    /// `+0x7c` — secondary radius.
    pub radius2: f32,
    /// `+0x89` — state byte; `== 5` ([`SPAWNER_STATE_TERMINAL`]) ⇒ exhausted/removed.
    pub state: u8,
    /// `+0x8b` — group index, `< 8` ([`SPAWNER_GROUP_COUNT`]).
    pub group: u8,
    /// `+0x8c` — "done" flag.
    pub done: bool,
    /// Which family updater owns this instance.
    pub family: SpawnerFamily,
    /// World transform of the spawner (placement anchor).
    pub transform: Transform,
}

impl Default for SimpleSpawner {
    fn default() -> Self {
        SimpleSpawner {
            guid: 0,
            adjust_target: 0,
            faction: SpawnFaction::Vz,
            interval: 1.0,
            countdown: 1.0,
            reload: 1.0,
            type_disc: [0; 3],
            radius: 0.0,
            radius2: 0.0,
            state: 0,
            group: 0,
            done: false,
            family: SpawnerFamily::Window,
            transform: Transform::IDENTITY,
        }
    }
}

impl SimpleSpawner {
    /// Whether the spawner has reached its terminal state (`+0x89 == 5`) and should be skipped/removed.
    pub fn is_terminal(&self) -> bool {
        self.state == SPAWNER_STATE_TERMINAL
    }

    /// This spawner's activation radius **squared**, in the horizontal (XZ) plane. The recovered
    /// `UpdateSimpleSpawners` gate tests the player/viewport against `+0x78 radius` (× runtime scale
    /// `DAT_00b97eec`, treated as 1 here — the scale is not statically recovered). When the authored
    /// `+0x78` radius is present (`> 0`) it wins; otherwise the recovered Window-family default
    /// [`WINDOW_RADIUS_SQ`] (160²) applies (code map §6).
    ///
    /// **CONFIRM-LIVE:** `register_population_spawners` does not yet load the per-instance `+0x78`
    /// value, so in practice `radius == 0` and every spawner falls back to the 160² default. The
    /// `radius > 0` branch is the faithful path for when that field is threaded in.
    fn activation_radius_sq(&self) -> f32 {
        if self.radius > 0.0 {
            self.radius * self.radius // CONFIRM-LIVE: per-instance +0x78 authored radius (× DAT_00b97eec scale = 1)
        } else {
            WINDOW_RADIUS_SQ // recovered 160² Window-family default
        }
    }

    /// Whether SOME `viewport` (a player/camera anchor) lies within this spawner's activation radius,
    /// measured as an **XZ** (ground-plane) distance from `transform.translation` — population is a
    /// horizontal system, so vertical (`Y`) separation is deliberately ignored. With no viewports the
    /// spawner has no anchor and is treated as out of range (cached-out).
    pub fn within_activation_radius(&self, viewports: &[Vec3]) -> bool {
        let r2 = self.activation_radius_sq();
        let p = self.transform.translation;
        viewports.iter().any(|v| {
            let dx = p.x - v.x;
            let dz = p.z - v.z;
            dx * dx + dz * dz <= r2
        })
    }

    /// Advance this spawner by `dt`, returning `Some(request)` on the frame its countdown elapses (then
    /// reloading), else `None`. Terminal / done spawners never fire. The emitted request places the
    /// unit at the spawner's transform (the family updater applies family-specific offsets/queries; the
    /// radius carried on the request lets the resolver jitter within it).
    ///
    /// **Activation-radius FIRING gate** (`UpdateSimpleSpawners`, Xbox `FUN_82338768` ↔ PC
    /// `FUN_004e4100`; the eligibility position/radius test `FUN_004e38d0`, code map §6): a spawner
    /// only advances/fires when SOME `viewport` is within its activation radius
    /// ([`within_activation_radius`](Self::within_activation_radius)). Out of range it is **cached-out**
    /// — it does NOT fire and its countdown does NOT decrement, so when a viewport comes back within
    /// range the timer resumes where it left off (§5 CacheIn/CacheOut behavior). This is what clusters
    /// ambient spawns around the player instead of firing every spawner across the whole map.
    pub fn update(&mut self, dt: f32, viewports: &[Vec3]) -> Option<SpawnRequest> {
        if self.is_terminal() || self.done {
            return None;
        }
        if !self.within_activation_radius(viewports) {
            // Cached-out: no anchor in range → do not fire and do not decrement the countdown.
            return None;
        }
        self.countdown -= dt;
        if self.countdown > 0.0 {
            return None;
        }
        // Fired: reload the timer and emit a request. Reload of 0 would busy-fire, so the terminal
        // "one-shot then done" spawner is expressed by state==5 / done, not a zero reload.
        self.countdown = self.reload;
        Some(SpawnRequest {
            // Slot→template resolution (CF-1): resolve this spawner's faction/list channel to a real,
            // corpus-verified faction human template so the fired unit realizes a faction-correct
            // Character, not the invisible `template 0` prop. (The exact per-slot crowd pick is the
            // CONFIRM-LIVE runtime spawn-list `int[]` — see [`crate::slot_table`].) The vehicle
            // channel has no human template → 0, which stays the veh-naming/prop path on the resolver.
            template: crate::slot_table::faction_template_hash(self.faction),
            transform: self.transform,
            faction: self.faction,
            group: self.group,
            family: self.family,
        })
    }
}

/// A cap-128 pending-spawn queue for one family (the `DAT_00dcc*` / cnt `DAT_00dcc*` pairs). Bounded
/// exactly like the exe: once at [`SPAWN_QUEUE_CAP`], further enqueues are dropped, not overwritten.
#[derive(Default, Debug)]
pub struct SpawnQueue {
    pending: Vec<SpawnRequest>,
    dropped: u64,
}

impl SpawnQueue {
    /// Enqueue a request if below the 128-cap, else drop it. Returns whether it was accepted.
    pub fn push(&mut self, req: SpawnRequest) -> bool {
        if self.pending.len() < SPAWN_QUEUE_CAP {
            self.pending.push(req);
            true
        } else {
            self.dropped += 1;
            false
        }
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Drain every queued request (the deferred-instantiate stage pulls them for the resolver).
    pub fn drain(&mut self) -> Vec<SpawnRequest> {
        std::mem::take(&mut self.pending)
    }
}

/// A `SpawnerAdjust` record (`TweakAttachedSpawners` / `…InGroup`) — the primary script-facing lever
/// (code map §7, heavy shipped Lua usage). Copies a 0x60-byte adjust record over an 8-group bit loop:
/// for each spawner whose group bit is set, apply the new state and/or spawn-list, then
/// despawn/force-respawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnerAdjust {
    /// Which of the 8 groups this adjust targets — one bit per group (`1 << group`). `0xFF` = all.
    pub group_mask: u8,
    /// New spawner state to write (`Some` ⇒ overwrite `+0x89`); `Some(5)` force-despawns (terminal).
    pub spawner_state: Option<u8>,
    /// New spawn-list channel to switch attached spawners to (`Some` ⇒ overwrite `+0x58`).
    pub spawn_list: Option<SpawnFaction>,
    /// Reset each matched spawner's countdown to fire immediately (force-respawn).
    pub force_respawn: bool,
}

impl Default for SpawnerAdjust {
    fn default() -> Self {
        SpawnerAdjust {
            group_mask: 0xFF,
            spawner_state: None,
            spawn_list: None,
            force_respawn: false,
        }
    }
}

/// The 768-cap simple-spawner instance pool + the four family queues — the `PopulationSimpleSpawner`
/// class-manager (`@0x00DF8510`) reimplemented as the mechanism the update walks.
#[derive(Default)]
pub struct SimpleSpawnerManager {
    spawners: Vec<SimpleSpawner>,
    /// The four cap-128 pending-spawn queues, indexed by [`SpawnerFamily`] as `usize`.
    queues: [SpawnQueue; 4],
}

impl SimpleSpawnerManager {
    pub fn new() -> Self {
        SimpleSpawnerManager::default()
    }

    /// Register a spawner instance (register `FUN_004e4620`). Refused (returns `None`) once the 768-cap
    /// pool is full, exactly as `cdbsizes.ini` bounds it. Returns the pool index on success.
    pub fn register(&mut self, spawner: SimpleSpawner) -> Option<usize> {
        if self.spawners.len() >= SIMPLE_SPAWNER_POOL as usize {
            return None;
        }
        self.spawners.push(spawner);
        Some(self.spawners.len() - 1)
    }

    pub fn spawners(&self) -> &[SimpleSpawner] {
        &self.spawners
    }
    pub fn spawners_mut(&mut self) -> &mut [SimpleSpawner] {
        &mut self.spawners
    }
    pub fn queue(&self, family: SpawnerFamily) -> &SpawnQueue {
        &self.queues[family as usize]
    }

    /// `UpdateSimpleSpawners` — fan out over the four families in fixed order (§6). Each non-terminal
    /// spawner **within activation radius of some `viewport`** advances its timer; a fired spawner
    /// enqueues one request onto its family's cap-128 queue. Out-of-range spawners are cached-out (they
    /// neither fire nor drain their countdown), so only spawners near a player anchor emit. Returns the
    /// number of requests enqueued this tick.
    pub fn update(&mut self, dt: f32, viewports: &[Vec3]) -> u32 {
        let mut fired = 0;
        for family in SpawnerFamily::ALL {
            for sp in self.spawners.iter_mut().filter(|s| s.family == family) {
                if let Some(req) = sp.update(dt, viewports) {
                    if self.queues[family as usize].push(req) {
                        fired += 1;
                    }
                }
            }
        }
        fired
    }

    /// Drain the deferred-instantiate stage: pull every queued request from all four family queues
    /// (the post-update spawn-queue drain, code map §3 tail). The resolver realizes these into entities.
    pub fn drain_requests(&mut self) -> Vec<SpawnRequest> {
        let mut out = Vec::new();
        for q in &mut self.queues {
            out.extend(q.drain());
        }
        out
    }

    /// Apply a [`SpawnerAdjust`] (`TweakAttachedSpawners`): the 8-group bit loop. For every spawner whose
    /// group bit is set in `adjust.group_mask`, overwrite state/list as requested and optionally
    /// force-respawn (countdown → 0). Returns how many spawners it touched.
    pub fn apply_adjust(&mut self, adjust: &SpawnerAdjust) -> u32 {
        let mut touched = 0;
        for sp in self.spawners.iter_mut() {
            debug_assert!(sp.group < SPAWNER_GROUP_COUNT);
            if adjust.group_mask & (1u8 << sp.group) == 0 {
                continue;
            }
            if let Some(state) = adjust.spawner_state {
                sp.state = state;
            }
            if let Some(list) = adjust.spawn_list {
                sp.faction = list;
            }
            if adjust.force_respawn {
                sp.countdown = 0.0;
                sp.done = false;
            }
            touched += 1;
        }
        touched
    }
}

/// `Ai.SetSpawnList`'s change log (code map §7): the engine keeps a **256-entry** log of spawn-list
/// changes and answers `GetSpawnListChangeInfo` from a **64-row** query window. Both are the recovered
/// capacities (Xbox 0x100 / 0x40; PC shape matches). Modeled here as the two caps for the reimpl seam.
pub const SPAWN_LIST_CHANGE_LOG_CAP: usize = 256;
pub const SPAWN_LIST_CHANGE_QUERY_ROWS: usize = 64;

/// Look up a template from a [`SkirmishSpawnList`] slot (the data-authored table a spawner draws from).
/// Returns the raw slot int; interpreting it (unit/count/faction index) is content-authored, so this is
/// the honest boundary — the engine mechanism selects *a slot*, the *meaning* is data.
pub fn spawn_list_slot(list: &SkirmishSpawnList, slot: usize) -> Option<i32> {
    list.slots.get(slot).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawner(interval: f32, family: SpawnerFamily, group: u8) -> SimpleSpawner {
        SimpleSpawner {
            interval,
            countdown: interval,
            reload: interval,
            family,
            group,
            faction: SpawnFaction::Vz,
            ..SimpleSpawner::default()
        }
    }

    /// Recovered spawner constants.
    #[test]
    fn recovered_spawner_constants() {
        assert_eq!(SIMPLE_SPAWNER_POOL, 768);
        assert_eq!(POPULATION_LIST_POOL, 1024);
        assert_eq!(SPAWNER_STATE_TERMINAL, 5);
        assert_eq!(SPAWNER_GROUP_COUNT, 8);
        assert_eq!(SPAWNER_CATEGORY_COUNT, 4);
        assert_eq!(SPAWN_QUEUE_CAP, 128);
        assert_eq!(WINDOW_RADIUS_SQ, 160.0 * 160.0);
        assert_eq!(SPAWN_LIST_CHANGE_LOG_CAP, 256);
        assert_eq!(SPAWN_LIST_CHANGE_QUERY_ROWS, 64);
        assert_eq!(SpawnerFamily::ALL.len(), 4);
    }

    /// In-range viewport for the origin-anchored test spawners (well within the 160 m Window default).
    const HERE: [Vec3; 1] = [Vec3::ZERO];

    /// A spawner fires exactly when its countdown elapses, then reloads — not before, not every frame.
    #[test]
    fn spawner_fires_on_interval_and_reloads() {
        let mut sp = spawner(1.0, SpawnerFamily::Window, 0);
        assert!(sp.update(0.4, &HERE).is_none(), "0.4s < 1.0s interval");
        assert!(sp.update(0.4, &HERE).is_none(), "0.8s total, still under");
        let req = sp.update(0.4, &HERE).expect("2 ticks past 1.0s should fire");
        assert_eq!(req.faction, SpawnFaction::Vz);
        assert_eq!(req.family, SpawnerFamily::Window);
        assert!((sp.countdown - 1.0).abs() < 1e-6, "reloaded to interval");
        assert!(sp.update(0.5, &HERE).is_none(), "just reloaded — no immediate refire");
    }

    /// Activation-radius FIRING gate (`UpdateSimpleSpawners`, PC `FUN_004e4100` eligibility
    /// position/radius test): a spawner fires only when SOME viewport is within its activation radius.
    /// Out of range it is cached-out — it neither fires NOR decrements its countdown, so the timer
    /// resumes on return. This is the fix that clusters ambient spawns around the hero.
    #[test]
    fn spawner_gated_by_activation_radius() {
        let mut sp = SimpleSpawner {
            transform: Transform::from_translation(Vec3::new(1000.0, 0.0, 0.0)),
            ..spawner(1.0, SpawnerFamily::Window, 0)
        };
        // Viewport 1000 m away (>> the 160 m Window default): dormant. Many ticks must NOT fire and
        // must NOT drain the countdown.
        let far = [Vec3::ZERO];
        for _ in 0..10 {
            assert!(sp.update(1.0, &far).is_none(), "out of range: never fires");
        }
        assert!((sp.countdown - 1.0).abs() < 1e-6, "dormant spawner keeps its full countdown");

        // Viewport within the 160 m default radius (10 m away): the timer resumes and fires.
        let near = [Vec3::new(990.0, 0.0, 0.0)];
        assert!(sp.update(0.5, &near).is_none(), "0.5s < 1.0s interval");
        let req = sp.update(0.6, &near).expect("crosses interval while in range → fires");
        assert_eq!(req.family, SpawnerFamily::Window);

        // No viewport at all ⇒ no anchor ⇒ out of range (cached-out).
        assert!(sp.update(1.0, &[]).is_none(), "no viewport → dormant");
    }

    /// The gate measures XZ (ground-plane) distance — vertical (Y) separation is ignored, so a
    /// viewport directly above/below still activates the spawner (the population is a horizontal
    /// system; this is why the live bug had actors floating at Y=160 across the map).
    #[test]
    fn activation_radius_ignores_vertical_separation() {
        let sp = SimpleSpawner {
            transform: Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)),
            ..spawner(1.0, SpawnerFamily::Window, 0)
        };
        // Same XZ as the spawner but 500 m up: XZ distance 0 → in range despite the huge Y gap.
        assert!(sp.within_activation_radius(&[Vec3::new(50.0, 500.0, 50.0)]));
        // 300 m away in XZ (> 160 m) → out of range regardless of matching Y.
        assert!(!sp.within_activation_radius(&[Vec3::new(350.0, 0.0, 50.0)]));
    }

    /// The per-instance `+0x78` radius overrides the 160² default when authored (`> 0`).
    #[test]
    fn spawner_uses_per_instance_radius_when_authored() {
        // radius 50 m (radius² 2500): a viewport 100 m away is OUT of range, even though 100 m is
        // inside the 160 m Window default — the authored radius wins.
        let mut sp = SimpleSpawner {
            radius: 50.0,
            transform: Transform::from_translation(Vec3::new(100.0, 0.0, 0.0)),
            ..spawner(1.0, SpawnerFamily::Window, 0)
        };
        assert!(sp.update(1.0, &HERE).is_none(), "100 m > 50 m authored radius → dormant");
        // Viewport 40 m away → inside the authored 50 m → fires.
        assert!(sp.update(1.0, &[Vec3::new(60.0, 0.0, 0.0)]).is_some(), "40 m < 50 m → fires");
    }

    /// CF-1: a fired spawner resolves its faction channel to a REAL, non-zero template (not the old
    /// literal `0`), and the template matches `slot_table`'s per-channel resolution — so the request
    /// realizes a faction-correct Character downstream, not the invisible template-0 prop.
    #[test]
    fn fired_spawner_resolves_a_nonzero_faction_template() {
        for f in [
            SpawnFaction::Vz,
            SpawnFaction::Gur,
            SpawnFaction::Pir,
            SpawnFaction::Ped,
        ] {
            let mut sp = SimpleSpawner {
                faction: f,
                ..spawner(1.0, SpawnerFamily::NoModel, 0)
            };
            let req = sp.update(1.0, &HERE).expect("crosses the 1.0s interval");
            assert_ne!(req.template, 0, "{f:?} spawner must emit a real template, not 0");
            assert_eq!(req.template, crate::slot_table::faction_template_hash(f));
            assert_eq!(req.faction, f);
        }
    }

    /// A terminal (state 5) spawner never fires.
    #[test]
    fn terminal_spawner_is_inert() {
        let mut sp = spawner(0.1, SpawnerFamily::Path, 0);
        sp.state = SPAWNER_STATE_TERMINAL;
        assert!(sp.is_terminal());
        assert!(sp.update(10.0, &HERE).is_none());
    }

    /// The queue caps at 128 and drops the overflow.
    #[test]
    fn queue_caps_at_128() {
        let mut q = SpawnQueue::default();
        let req = SpawnRequest {
            template: 0,
            transform: Transform::IDENTITY,
            faction: SpawnFaction::Ped,
            group: 0,
            family: SpawnerFamily::NoModel,
        };
        for _ in 0..SPAWN_QUEUE_CAP {
            assert!(q.push(req));
        }
        assert!(!q.push(req), "129th dropped");
        assert_eq!(q.len(), SPAWN_QUEUE_CAP);
        assert_eq!(q.dropped(), 1);
    }

    /// The manager fans out over families and drains fired requests; the pool caps at 768.
    #[test]
    fn manager_updates_families_and_drains() {
        let mut mgr = SimpleSpawnerManager::new();
        mgr.register(spawner(1.0, SpawnerFamily::Window, 0)).unwrap();
        mgr.register(spawner(1.0, SpawnerFamily::Path, 1)).unwrap();
        assert_eq!(mgr.update(0.5, &HERE), 0, "under interval, nothing fires");
        assert_eq!(mgr.update(0.6, &HERE), 2, "both cross 1.0s this tick");
        let reqs = mgr.drain_requests();
        assert_eq!(reqs.len(), 2);
        assert!(mgr.drain_requests().is_empty(), "drain empties the queues");
    }

    #[test]
    fn pool_caps_at_768() {
        let mut mgr = SimpleSpawnerManager::new();
        for _ in 0..SIMPLE_SPAWNER_POOL {
            assert!(mgr.register(SimpleSpawner::default()).is_some());
        }
        assert!(mgr.register(SimpleSpawner::default()).is_none(), "769th refused");
    }

    /// `TweakAttachedSpawners` applies over the 8-group bit mask: only matched groups change state/list.
    #[test]
    fn spawner_adjust_group_bit_loop() {
        let mut mgr = SimpleSpawnerManager::new();
        mgr.register(spawner(1.0, SpawnerFamily::Window, 0)).unwrap(); // group 0
        mgr.register(spawner(1.0, SpawnerFamily::Window, 3)).unwrap(); // group 3
        mgr.register(spawner(1.0, SpawnerFamily::Window, 5)).unwrap(); // group 5

        // Target only group 3 → despawn (state 5) + switch to the Guerilla list.
        let adjust = SpawnerAdjust {
            group_mask: 1 << 3,
            spawner_state: Some(SPAWNER_STATE_TERMINAL),
            spawn_list: Some(SpawnFaction::Gur),
            force_respawn: false,
        };
        assert_eq!(mgr.apply_adjust(&adjust), 1, "only the group-3 spawner matches");
        let s = mgr.spawners();
        assert!(!s[0].is_terminal());
        assert!(s[1].is_terminal());
        assert_eq!(s[1].faction, SpawnFaction::Gur);
        assert!(!s[2].is_terminal());
    }

    /// Force-respawn zeroes the countdown so the next update fires immediately.
    #[test]
    fn force_respawn_fires_next_tick() {
        let mut mgr = SimpleSpawnerManager::new();
        mgr.register(spawner(100.0, SpawnerFamily::Hardpoint, 2)).unwrap();
        mgr.apply_adjust(&SpawnerAdjust {
            group_mask: 0xFF,
            force_respawn: true,
            ..SpawnerAdjust::default()
        });
        assert_eq!(mgr.update(0.0, &HERE), 1, "countdown zeroed → fires on the very next tick");
    }
}
