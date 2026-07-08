//! The engine's implementation of the script host's `EngineHost` seam.
//!
//! This is where the game's Lua meets the engine: `mercs2_script` owns the VM + the `Pg.Spawn` /
//! `Object.*` binding *mechanism*; here the engine provides the *behavior*. The game's Lua calls
//! `MrxUtil.SpawnActor(...)` (→ `Pg.Spawn` + `Object.*`); those bindings drive [`GameScriptHost`],
//! which records the actor-spawn *intents*. The render loop (`game_world`) then realizes each intent
//! by resolving its template → geometry and spawning ECS entities.
//!
//! **Why record-then-realize instead of spawning directly inside the binding?** The bindings run
//! inside the Lua VM behind an `Rc<RefCell<dyn EngineHost>>`; the actual spawn needs `&mut Scene`
//! (GPU) and `&mut World` (ECS), which are owned by the render loop. Recording intents keeps the VM
//! free of the GPU/ECS borrow and lets the engine realize them at the right point in the frame. This
//! is the same split the original engine used: script requests, engine fulfills on the load path.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::audio::{AudioEngine, VoiceId};
use mercs2_core::{Entity, GuidMap, Transform, World};
use mercs2_formats::hash::pandemic_hash_m2;
use crate::script::{EngineHost, ScriptHost};

/// The engine's actor-template name for the PMC player HQ interior. `Pg.Spawn(PMC_INTERIOR_TEMPLATE)`
/// resolves to the PMC interior geometry (see `game_world::load_pmc_interior`). The template→mesh
/// resolution for the enclosing hall SHELL is the open sub-problem.
pub const PMC_INTERIOR_TEMPLATE: &str = "PmcHqInterior";

/// The PMC interior actor origin — `mrxhq.lua:657` `SpawnActor(..., vPosition = {3750, 450, -3840})`.
pub const PMC_INTERIOR_ACTOR_ORIGIN: [f32; 3] = [3750.0, 450.0, -3840.0];

/// One actor the game's Lua asked the engine to spawn, captured from the `Pg.Spawn` + `Object.*` call
/// sequence. `pos`/`yaw` reflect the final transform after any `Object.SetPosition`/`SetYaw`.
#[derive(Clone, Debug)]
pub struct SpawnRequest {
    pub guid: u64,
    pub template: String,
    pub name: String,
    pub pos: [f32; 3],
    pub yaw: f32,
}

/// The engine side of the script seam: Lua drives it; it records [`SpawnRequest`]s for the render loop
/// to realize. It holds no GPU state, and only a **shared** (`Rc<RefCell>`) handle to the ECS World +
/// guidmap — so it still lives behind the VM's `RefCell` while its `Object.*`/`Pg.GetGuidByName` bodies
/// resolve against LIVE entities instead of shadow tables (see [`attach_world`](Self::attach_world)).
pub struct GameScriptHost {
    pub spawns: Vec<SpawnRequest>,
    by_name: HashMap<String, u64>,
    by_guid: HashMap<u64, usize>,
    next_guid: u64,
    level: String,
    /// The live ECS World (single source of truth), shared with the frame loop. `None` when the host is
    /// constructed standalone (tests) — those keep the shadow-table fallbacks below. When attached,
    /// name/position/state reads resolve against live entities via [`guids`](Self::guids).
    world: Option<Rc<RefCell<World>>>,
    /// The guidmap singleton (name-hash → `Entity`, guid ↔ `Entity`), shared with the loop. Attached
    /// alongside [`world`](Self::world) by [`attach_world`](Self::attach_world).
    guids: Option<Rc<RefCell<GuidMap>>>,
    /// The live audio system the game's `Sound.*` / music Lua drives. **Shared** (`Rc<RefCell>`) so the
    /// game loop ticks the SAME engine each frame (`GameplaySystems::tick` → `audio.tick`) that the Lua
    /// `EngineHost` forwarding cues into — one `mercs2_audio` stack, driven from both sides.
    audio: Rc<RefCell<AudioEngine>>,
    /// The AI mechanism the game's `Ai.*` Lua drives: the recovered 1024-slot action ring + the
    /// `[-100,100]` relation matrix (`crate::ai::AiWorld`, AI code map §8). `Ai.Goal` posts to the ring;
    /// `Ai.SetRelation`/`GetRelation` read/write the matrix. Per-entity perception records are ticked
    /// over the ECS world by the runtime, not here.
    ai: crate::ai::AiWorld,
    /// Per-actor `AiBehavior` restriction flags set by `Ai.SetState` (keyed by actor GUID).
    ai_states: std::collections::HashMap<u64, crate::ai::AiBehavior>,
    /// The faction/reputation manager the game's `Ai.AddInfraction`/`SetInfractionMultiplier`/attitude
    /// Lua drives — the recovered combat→faction mood bridge + `[-100,100]` relation model
    /// (`crate::faction::FactionWorld`, faction code map). Seeded with the recovered initial relations.
    faction: crate::faction::FactionWorld,
    /// The living-world population/spawner manager the game's `Ai.TweakAttachedSpawners*`/spawn-list Lua
    /// drives (`crate::population::PopulationWorld`, world-streaming/AI code maps §7).
    population: crate::population::PopulationWorld,
    /// The hero spawn position the game's Lua set via `Object.SetPosition(Player.GetLocalCharacter(),
    /// …)` — the base game's `MrxUtil._TeleportHero` bottoms out to exactly that (mrxutil.lua:328). The
    /// boot reads this to place the player: the spawn is **Lua-authored, no engine-constant fallback**.
    hero_teleport: Option<[f32; 3]>,
    /// Where the boot flow's `Pg.Spawn(hero, x, y, z, …)` placed the hero — the spawn the loop reads
    /// (the REAL flow result, superseding the engine-side marker shortcut).
    hero_spawn: Option<[f32; 3]>,
    /// The hero template name the boot spawns (`chris`/`mattias`/`jen`), for the fired boot flow.
    hero_character: String,
    /// `Player.AttachToCharacter` bindings: player GUID → the character it controls. The local player
    /// defaults to [`HERO_GUID`] (`player_character_of`) even before an explicit attach.
    player_character: HashMap<u64, u64>,
    /// `Human.SetState`/`DoAction` driven state per humanoid GUID: `(stance, action)`. The boot teleport
    /// (`mrxutil.lua:314`) records `("upright","idle")`; civ/hijack scripts record their stance+anim.
    human_states: HashMap<u64, (String, String)>,
    /// Per-vehicle hijack FSM (`Vehicle.Hijack*`), keyed by vehicle GUID — the engine-owned state the
    /// mission Lua drives through its lifecycle (`crate::vehicle::HijackFsm`).
    hijacks: HashMap<u64, crate::vehicle::HijackFsm>,
    /// Per-vehicle turret/rotor aim (`Vehicle.SetTurretPitch/Yaw`, `Vehicle.SpinHeli`).
    turrets: HashMap<u64, crate::vehicle::TurretAim>,
    /// Engine settings the `Sys.Set*` config surface writes and the matching `Sys.*` getters read
    /// (the game holds these; the rest of the engine reads them). `Set*`→`Get*` are real roundtrips.
    settings: SysSettings,
    /// Per-object label set (`Object.AddLabel`/`RemoveLabel`/`HasLabel`) — the tags mission Lua and the
    /// `ObjectFilter` predicate query against.
    object_labels: HashMap<u64, std::collections::HashSet<String>>,
    /// The `ObjectFilter.*` handle registry (label boolean-expr + include/exclude sets).
    object_filters: mercs2_core::ObjectFilterRegistry,
    /// The object attachment graph: child GUID → parent GUID (`Object.Attach`/`Detach`). `GetParent`/
    /// `IsAttached`/`GetAttachedObjects` read it.
    attachments: HashMap<u64, u64>,
    /// The retained-mode HUD widget tree the `Hud.*` Lua drives (`crate::widgets::WidgetTree`).
    hud: crate::widgets::WidgetTree,
    /// The HUD world-marker set the `Gui._Marker*` Lua drives.
    markers: crate::widgets::MarkerSet,
    /// Global render/post-FX parameter state the `Atmosphere`/`Bloom`/`Graphics`/`Fade` Lua drives.
    render: mercs2_core::RenderState,
    /// Cinematic camera controller state the `CameraFx.*` Lua drives.
    camera_fx: CameraFxState,
    /// Per-character weapon loadout (`Inventory.*`): character GUID → its weapon GUIDs.
    loadouts: HashMap<u64, Vec<u64>>,
    /// Per-weapon ammo state (`Weapon.*`).
    weapons: HashMap<u64, WeaponState>,
    /// Objects currently on fire (`Fire.Ignite`/`Extinguish`).
    burning: std::collections::HashSet<u64>,
    /// Per-object health `(current, max)` (`Object.*Health`, `SendDamage`, `Kill`/`Revive`).
    health: HashMap<u64, (f32, f32)>,
    /// `Pg.CreateRegion` trigger regions: handle → `(center, radius)`; `region_names` maps name→handle.
    regions: HashMap<u64, ([f32; 3], f32)>,
    region_names: HashMap<String, u64>,
    next_region: u64,
    /// Active alarms (`Pg.ActivateAlarm`/`ToggleAlarm`).
    alarms: std::collections::HashSet<u64>,
    /// Per-player designator charges (`Airstrike.*Designator`); presence = equipped.
    designators: HashMap<u64, i32>,
    /// Recorded ordnance/plane spawns (`Airstrike.Spawn*`/`Flyby`/`ConeSpawn`) for the runtime to realize.
    airstrikes: Vec<(String, [f32; 3])>,
    /// Per-human runtime flags (`Human.*` action verbs).
    human_flags: HashMap<u64, HumanFlags>,
    /// Network session state (`Net.*`).
    net: NetState,
    /// Per-object state-machine state (`ObjectState.SetState`).
    object_states_sm: HashMap<u64, String>,
    /// Active node FX emitters per object (`ObjectState.StartEmitter`/`StopEmitter`).
    emitters: HashMap<u64, std::collections::HashSet<String>>,
    /// Bound facial anim set + current expression per face (`Face.*`).
    faces: HashMap<u64, (String, String)>,
    /// The active mission report `(faction, delay)` (`Report.*`).
    report: Option<(u64, f32)>,
    /// Named player-mode boolean flags (`Player.Set*` gameplay gates the engine reads).
    player_modes: HashMap<String, bool>,
    /// Named player-mode scalars (`SetHealthClamp`/`SetSwimmingSearchRadius`/`SetAimMode`).
    player_scalars: HashMap<String, f32>,
    /// Which seat GUID each human occupies (`Vehicle.EnterBySeatGuid`/`TransferToSeat`, `ForceExitSeat`).
    human_seats: HashMap<u64, u64>,
    /// Count of `[lua]` `Debug.Printf` lines the game's Lua has emitted — the ground-truth that the
    /// game code is executing against the engine (used by the boot-flow regression test).
    pub lua_log_lines: usize,
    /// Set once the game's Lua prints `GlobalExit - Complete` — loadprobe phase 20, the world-load
    /// state machine ran to completion ("world fully loaded").
    pub world_load_complete: bool,
    /// Set once every streaming layer request the master-script boot issued has been fulfilled
    /// (`MrxLayerManager` drained its op queue) — the real world-streaming milestone.
    pub world_layers_loaded: bool,
    /// Dynamic-music / DSP / audio-mode command log (`Sound.*` director config).
    sound_cmds: Vec<(String, Vec<String>)>,
    /// Replicated mission-event log (`Net.SendEvent_*` etc.) the runtime realizes locally in SP.
    net_events: Vec<(String, Vec<String>)>,
    /// Generic engine-command log (Hud/Object/Camera/Lti/Sys/Gui action verbs) the runtime consumes.
    script_cmds: Vec<(String, Vec<String>)>,
    /// Requested game states (`Sys.RequestGameState`) awaiting the engine's state-machine service — the
    /// resident pump drains these and fires the matching `Event.GameStateChange` to advance `MrxState`.
    pending_game_states: Vec<String>,
    /// The player economy singleton (`Player.GetCash`/`GetFuel` — signed i32 on `[0x1176054]`, the
    /// money/fuel notes). Host-owned engine state (the correct home — not a shadow of ECS data); seeded
    /// from the loaded save's stockpile at boot, then the game's Lua drives it. Was trait-default 0.
    cash: i64,
    fuel: i64,
    fuel_capacity: i64,
}

/// Script-driven cinematic camera controller state (`CameraFx.*`): the pose/shake/blend the camera
/// system applies. The engine owns it; the camera update reads it.
#[derive(Clone, Debug)]
pub struct CameraFxState {
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
    pub position: [f32; 3],
    pub lookat: [f32; 3],
    pub shake: f32,
    pub blending: bool,
    pub held: bool,
    /// The object the camera follows (`Follow`), 0 = none.
    pub follow_guid: u64,
    /// The selected named cinematic shot (`SetShot`).
    pub shot: String,
}

impl Default for CameraFxState {
    fn default() -> Self {
        CameraFxState {
            yaw: 0.0,
            pitch: 0.0,
            fov: 60.0,
            position: [0.0; 3],
            lookat: [0.0; 3],
            shake: 0.0,
            blending: false,
            held: false,
            follow_guid: 0,
            shot: String::new(),
        }
    }
}

/// Default object health when an object is first touched by a health op (no per-object stats DB yet).
const DEFAULT_MAX_HEALTH: f32 = 100.0;

/// Designator charges granted by `Airstrike.EquipDesignator`/`RefillDesignator`.
const DESIGNATOR_CHARGES: i32 = 3;

/// Network session mode (`Net.*`). The offline single-player game defaults to `Server` (it is its own
/// authoritative host) with no active session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetMode {
    Offline,
    Lobby,
    Client,
    Server,
}

/// Network session state the `Net.*` surface drives.
#[derive(Clone, Debug)]
pub struct NetState {
    pub mode: NetMode,
    pub active: bool,
    pub multiplayer: bool,
    pub host_name: String,
}

impl Default for NetState {
    fn default() -> Self {
        NetState { mode: NetMode::Server, active: false, multiplayer: false, host_name: String::new() }
    }
}

/// Per-human runtime flags the `Human.*` action verbs toggle.
#[derive(Clone, Copy, Debug)]
pub struct HumanFlags {
    pub weapons_enabled: bool,
    pub fire_lock: bool,
    pub knocked_down: bool,
    pub ragdoll: bool,
    pub jostle_enabled: bool,
    pub corpse_cleanup: bool,
    pub weapon_drawn: bool,
    pub carrying: bool,
    pub grappling: bool,
    pub swimming: bool,
}

impl Default for HumanFlags {
    fn default() -> Self {
        HumanFlags {
            weapons_enabled: true,
            fire_lock: false,
            knocked_down: false,
            ragdoll: false,
            jostle_enabled: true,
            corpse_cleanup: true,
            weapon_drawn: false,
            carrying: false,
            grappling: false,
            swimming: false,
        }
    }
}

/// Per-weapon ammo state (`Weapon.*`).
#[derive(Clone, Copy, Debug)]
pub struct WeaponState {
    pub clip: i32,
    pub reserve: i32,
    pub max_clip: i32,
    pub max_reserve: i32,
    pub primary: bool,
    pub designator: bool,
}

impl Default for WeaponState {
    fn default() -> Self {
        WeaponState { clip: 0, reserve: 0, max_clip: 30, max_reserve: 300, primary: true, designator: false }
    }
}

/// Emit a `[bind]` line to the app log (the same stdout sink as `[world]`/`[lua]`) whenever the game's
/// Lua drives one of the recorded-command engine bindings — the ground-truth confirmation that the
/// binding surface is loaded and firing against the game's code. Args are truncated for readability.
fn log_binding(ns: &str, verb: &str, args: &[String]) {
    let shown = args.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
    let more = if args.len() > 6 { format!(", …+{}", args.len() - 6) } else { String::new() };
    println!("[bind] {ns}.{verb}({shown}{more})");
}

/// Stable hash of a VO cue name → its cue guid, so `VO.Cue(name)` and a later `VO.Cancel(name)` address
/// the same line (FNV-1a; internal consistency, not the game's exact m2 cue hash).
fn vo_cue_hash(cue: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in cue.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// The `Sys.*` engine-config the script host owns (`Sys.SetTimeScale`/`SetTutorialsEnabled`/… write it;
/// `Sys.TutorialsEnabled`/`GetMasterScriptName`/… read it). Retail-PC defaults.
#[derive(Clone, Debug)]
pub struct SysSettings {
    /// `Sys.SetTimeScale` — global sim time multiplier (1.0 = real time). The fixed-tick reads this.
    pub time_scale: f32,
    /// `Sys.SetMasterScriptName` — the master boot script (`GetMasterScriptName`).
    pub master_script: String,
    /// `Sys.SetTutorialsEnabled` ↔ `Sys.TutorialsEnabled`.
    pub tutorials_enabled: bool,
    /// `Sys.SetAutosaveEnabled` — gates `Sys.RequestAutosave`.
    pub autosave_enabled: bool,
    /// `Sys.SetLuaSaveVersion` — the save-format version the Lua stamps into profiles.
    pub lua_save_version: i64,
    /// `Sys.SetNumberOfViewports` — split-screen viewport count (1 on PC single-player).
    pub viewports: i64,
    /// `Sys.SetAssetRequestMax` — the streaming asset-request budget.
    pub asset_request_max: i64,
    /// `Sys.StartSingleplayer` — a single-player session has been started.
    pub singleplayer: bool,
}

impl Default for SysSettings {
    fn default() -> Self {
        SysSettings {
            time_scale: 1.0,
            master_script: String::new(),
            tutorials_enabled: true,
            autosave_enabled: true,
            lua_save_version: 0,
            viewports: 1,
            asset_request_max: 0,
            singleplayer: false,
        }
    }
}

/// The GUID the local player object is registered under (distinct from [`HERO_GUID`], the character it
/// controls). `Player.GetLocalPlayer`/`GetPrimaryPlayer` return this; `Player.GetCharacter(it)` → hero.
pub const LOCAL_PLAYER_GUID: u64 = 0x0000_0002;

/// The GUID the player hero is registered under so the game's Lua can address it (`Player.Get*Character`
/// return this; `Object.SetPosition`/`SetYaw` on it drive the real player). Distinct from the
/// script-spawn GUID space (`0x1000_0000+`).
pub const HERO_GUID: u64 = 0x0000_0001;

impl GameScriptHost {
    pub fn new(level: impl Into<String>) -> Self {
        GameScriptHost {
            spawns: Vec::new(),
            by_name: HashMap::new(),
            by_guid: HashMap::new(),
            next_guid: 0x1000_0000, // distinct, non-zero GUID space for script-spawned actors
            level: level.into(),
            world: None, // attached by the loop via `attach_world`; None in standalone tests
            guids: None,
            audio: Rc::new(RefCell::new(AudioEngine::default())),
            ai: crate::ai::AiWorld::new(),
            ai_states: std::collections::HashMap::new(),
            faction: crate::faction::FactionWorld::with_default_relations(),
            population: crate::population::PopulationWorld::new(),
            hero_teleport: None,
            hero_spawn: None,
            hero_character: String::new(),
            player_character: HashMap::new(),
            human_states: HashMap::new(),
            hijacks: HashMap::new(),
            turrets: HashMap::new(),
            settings: SysSettings::default(),
            object_labels: HashMap::new(),
            object_filters: mercs2_core::ObjectFilterRegistry::new(),
            attachments: HashMap::new(),
            hud: crate::widgets::WidgetTree::new(),
            markers: crate::widgets::MarkerSet::new(),
            render: mercs2_core::RenderState::new(),
            camera_fx: CameraFxState::default(),
            loadouts: HashMap::new(),
            weapons: HashMap::new(),
            burning: std::collections::HashSet::new(),
            health: HashMap::new(),
            regions: HashMap::new(),
            region_names: HashMap::new(),
            next_region: 0x5000_0000,
            alarms: std::collections::HashSet::new(),
            designators: HashMap::new(),
            airstrikes: Vec::new(),
            human_flags: HashMap::new(),
            net: NetState::default(),
            object_states_sm: HashMap::new(),
            emitters: HashMap::new(),
            faces: HashMap::new(),
            report: None,
            player_modes: HashMap::new(),
            player_scalars: HashMap::new(),
            human_seats: HashMap::new(),
            lua_log_lines: 0,
            world_load_complete: false,
            world_layers_loaded: false,
            sound_cmds: Vec::new(),
            net_events: Vec::new(),
            script_cmds: Vec::new(),
            pending_game_states: Vec::new(),
            cash: 0,
            fuel: 0,
            fuel_capacity: 0,
        }
    }

    /// Seed the economy from the loaded save (the stockpile's cash pile). Fuel/capacity are set by the
    /// game's Lua during init (support-data/player setup), so they start at 0 and round-trip from there.
    pub fn set_cash(&mut self, cash: i64) {
        self.cash = cash;
    }

    /// Attach the live ECS World + guidmap the frame loop owns, so this host's `Object.*` /
    /// `Pg.GetGuidByName` bodies resolve against LIVE entities (position from the entity's `Transform`,
    /// not a shadow table). Called once at boot. Standalone (test) hosts never attach → shadow fallback.
    pub fn attach_world(&mut self, world: Rc<RefCell<World>>, guids: Rc<RefCell<GuidMap>>) {
        self.world = Some(world);
        self.guids = Some(guids);
    }

    /// The entity a GUID resolves to via the attached guidmap (None if no World attached / guid unknown).
    fn entity_of(&self, guid: u64) -> Option<Entity> {
        self.guids.as_ref()?.borrow().entity_by_guid(guid)
    }

    /// A copy of `guid`'s live `Transform` from the attached World, if the entity has one.
    fn transform_of(&self, guid: u64) -> Option<Transform> {
        let e = self.entity_of(guid)?;
        let world = self.world.as_ref()?.borrow();
        world.get::<&Transform>(e).ok().map(|t| *t)
    }

    /// Mutate `guid`'s live `Transform` in the attached World; returns whether it was applied.
    fn with_transform_mut(&self, guid: u64, f: impl FnOnce(&mut Transform)) -> bool {
        let Some(e) = self.entity_of(guid) else { return false };
        let Some(world_rc) = self.world.as_ref() else { return false };
        let world = world_rc.borrow();
        let Ok(mut t) = world.get::<&mut Transform>(e) else { return false };
        f(&mut t);
        true
    }

    /// A copy of `guid`'s live `Health` component (the SAME component the combat silo reads/writes), if
    /// the entity carries one — so Lua `Object.*Health` and combat damage never diverge for live actors.
    fn health_of(&self, guid: u64) -> Option<crate::combat::Health> {
        let e = self.entity_of(guid)?;
        let world = self.world.as_ref()?.borrow();
        world.get::<&crate::combat::Health>(e).ok().map(|h| *h)
    }

    /// Read-or-init (`max = default_max`) and mutate `guid`'s live `Health`, writing it back. Returns
    /// whether it was applied (false if no live entity → the caller keeps the shadow fallback).
    fn with_health(&self, guid: u64, default_max: f32, f: impl FnOnce(&mut crate::combat::Health)) -> bool {
        let Some(e) = self.entity_of(guid) else { return false };
        let Some(world_rc) = self.world.as_ref() else { return false };
        let mut world = world_rc.borrow_mut();
        let mut h = world
            .get::<&crate::combat::Health>(e)
            .map(|h| *h)
            .unwrap_or_else(|_| crate::combat::Health::new(default_max));
        f(&mut h);
        world.insert_one(e, h).is_ok()
    }

    /// Register an entity into the attached guidmap under an explicit `guid` (+ optional name-hash) — the
    /// loop calls this when it realizes a spawn or creates a named marker entity. No-op without a guidmap.
    pub fn register_entity(&self, e: Entity, guid: u64, name_hash: Option<u32>) {
        if let Some(g) = &self.guids {
            g.borrow_mut().register(e, name_hash, guid);
        }
    }

    /// Register a named marker/entity, minting a fresh guid; returns it (0 if no guidmap attached).
    pub fn register_named_entity(&self, e: Entity, name_hash: u32) -> u64 {
        match &self.guids {
            Some(g) => g.borrow_mut().register_named(e, name_hash),
            None => 0,
        }
    }

    /// The `(stance, action)` a `Human.SetState`/`DoAction` last drove onto `guid`, if any — the loop's
    /// humanoid animation system reads this to pick the clip (record-then-realize, like spawns).
    #[allow(dead_code)] // consumed by the loop's humanoid-anim realize step (wired next), like `audio()`.
    pub fn human_state(&self, guid: u64) -> Option<&(String, String)> {
        self.human_states.get(&guid)
    }

    /// Look up a spawned actor's template (its model), for `Object.GetModelName` / name resolution.
    fn template_of(&self, guid: u64) -> Option<&str> {
        self.by_guid.get(&guid).and_then(|&i| self.spawns.get(i)).map(|r| r.template.as_str())
    }

    fn name_of(&self, guid: u64) -> Option<&str> {
        self.by_guid.get(&guid).and_then(|&i| self.spawns.get(i)).map(|r| r.name.as_str())
    }

    /// A shared handle to the live audio engine, for the game loop to `tick`/`render_tick` each frame
    /// (and for `GameplaySystems` to own the tick side of the same engine the Lua cues into). Consumed
    /// once a `GameScriptHost` is made loop-resident (the persistent-Lua step) so its `Sound.*` cues and
    /// the loop's `gameplay.tick` drive one engine; today the default boot loop owns its own.
    #[allow(dead_code)]
    pub fn audio(&self) -> Rc<RefCell<AudioEngine>> {
        self.audio.clone()
    }

    /// Drain the spawn intents recorded since the last call (the loop realizes these into ECS
    /// entities each frame — runtime `Pg.Spawn`s become drivable vehicles / rendered props). Clears
    /// the `by_guid` index too so realized requests aren't re-mutated by a later `Object.Set*`.
    pub fn take_new_spawns(&mut self) -> Vec<SpawnRequest> {
        self.by_guid.clear();
        std::mem::take(&mut self.spawns)
    }

    /// Set the hero template for the boot flow, and tag the hero object with its identity label. Named
    /// markers are no longer passed here — they live in the World + guidmap (the loader entity-izes them),
    /// so `CreatePlayerCharacter(location=<name>)` resolves through `Pg.GetGuidByName` → the live entity.
    pub fn set_boot_context(&mut self, hero_character: impl Into<String>) {
        self.hero_character = hero_character.into();
        // The engine tags the player character object with its identity label (mattias/jennifer/chris) at
        // creation; the game reads it via `MrxUtil.GetCharacterIdentity → Object.HasLabel(uChar, <id>)`
        // (mrxutil.lua:649) throughout the mission/faction/HUD code. Wire that label onto the hero object
        // so identity resolves to the chosen merc instead of erroring "not one of M/J/C".
        let id = self.hero_character.to_ascii_lowercase();
        if !id.is_empty() {
            self.object_add_label(HERO_GUID, &id);
        }
    }

    /// The hero template the boot spawns (for the fired boot flow's `CreatePlayerCharacter`).
    pub fn hero_character(&self) -> &str {
        &self.hero_character
    }

    /// Where the boot flow's `Pg.Spawn(hero, …)` placed the hero — the REAL flow's spawn result the loop
    /// reads to position the player (supersedes the engine-side marker shortcut). `None` until it fires.
    pub fn take_hero_spawn(&mut self) -> Option<[f32; 3]> {
        self.hero_spawn.take()
    }

    /// Drain the requested game states awaiting the state-machine service (the resident pump fires the
    /// matching `Event.GameStateChange` for each to advance the `MrxState` world-load chain).
    pub fn take_pending_game_states(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_game_states)
    }

    /// The hero spawn position the game's Lua requested via `MrxUtil._TeleportHero`, if any. The boot
    /// places the player here — the spawn is Lua-authored (no engine-constant fallback).
    pub fn take_hero_teleport(&mut self) -> Option<[f32; 3]> {
        self.hero_teleport.take()
    }

    fn req_mut(&mut self, guid: u64) -> Option<&mut SpawnRequest> {
        let i = *self.by_guid.get(&guid)?;
        self.spawns.get_mut(i)
    }
}

impl EngineHost for GameScriptHost {
    fn log(&mut self, source: &str, msg: &str) {
        if source == "lua" {
            self.lua_log_lines += 1;
            // loadprobe phase 20 — the world-load state machine reached GlobalExit ("world fully loaded").
            if msg.contains("GlobalExit - Complete") {
                self.world_load_complete = true;
            }
            // The world's streaming layers all loaded (MrxLayerManager fulfilled every request) — the
            // real streaming milestone the master-script boot drives (loadprobe 16/18-19 territory).
            if msg.contains("All layer operations processed and fulfilled") {
                self.world_layers_loaded = true;
            }
        }
        println!("[{source}] {msg}");
    }
    fn get_level_name(&self) -> String {
        self.level.clone()
    }
    fn guid_by_name(&mut self, name: &str) -> u64 {
        // A spawned object with that name wins (record-then-realize keeps its guid).
        if let Some(g) = self.by_name.get(name).copied() {
            return g;
        }
        // Resolve the named entity (a marker or a streamed/spawned object) through the live guidmap —
        // the real `Pg.GetGuidByName` over live entities, not a side table. 0 when unknown / no world.
        let h = pandemic_hash_m2(&name.to_ascii_lowercase());
        self.guids
            .as_ref()
            .and_then(|gm| gm.borrow().guid_by_name_hash(h))
            .unwrap_or(0)
    }
    fn pg_spawn(&mut self, template: &str, pos: [f32; 3], yaw: f32, _high_detail: bool) -> u64 {
        self.next_guid += 1;
        let guid = self.next_guid;
        // The hero character spawn (boot flow's CreatePlayerCharacter → Pg.Spawn(hero, x,y,z)) records
        // the spawn position the loop reads to place the player — the REAL flow's result.
        if !self.hero_character.is_empty() && template.eq_ignore_ascii_case(&self.hero_character) {
            self.hero_spawn = Some(pos);
        }
        let idx = self.spawns.len();
        self.spawns.push(SpawnRequest {
            guid,
            template: template.to_string(),
            name: String::new(),
            pos,
            yaw,
        });
        self.by_guid.insert(guid, idx);
        guid
    }
    fn object_set_name(&mut self, guid: u64, name: &str) {
        if let Some(r) = self.req_mut(guid) {
            r.name = name.to_string();
        }
        self.by_name.insert(name.to_string(), guid);
    }
    fn object_set_position(&mut self, guid: u64, pos: [f32; 3]) {
        // The hero is teleported (`_TeleportHero` → Object.SetPosition) during the boot Lua flow, BEFORE
        // its ECS entity exists — record it so the boot can place the player. (Once the hero entity is
        // registered, the live move below also applies.)
        if guid == HERO_GUID {
            self.hero_teleport = Some(pos);
        }
        // Live entity: move its Transform in the World.
        if self.with_transform_mut(guid, |t| t.translation = pos.into()) {
            return;
        }
        // Fallback: an un-realized spawn request's recorded position (the entity isn't live yet).
        if let Some(r) = self.req_mut(guid) {
            r.pos = pos;
        }
    }
    fn object_get_position(&mut self, guid: u64) -> [f32; 3] {
        // Live entity (a named marker or a realized/streamed object) — position from its `Transform`.
        // This is the real `Object.GetPosition`: a physics-moved object reports its CURRENT position.
        if let Some(t) = self.transform_of(guid) {
            return t.translation.to_array();
        }
        // Fallback: an un-realized spawn request's recorded pos (the entity isn't live yet).
        self.by_guid
            .get(&guid)
            .and_then(|&i| self.spawns.get(i))
            .map(|r| r.pos)
            .unwrap_or([0.0; 3])
    }
    fn player_any_character(&self) -> u64 {
        HERO_GUID
    }
    fn player_local_character(&self) -> u64 {
        HERO_GUID
    }
    fn player_primary_character(&self) -> u64 {
        // Single-player boot: the primary character IS the local hero. The game queries this constantly
        // (GetCharacterIdentity(Player.GetPrimaryCharacter()), weapon/HUD/mission code); the trait default
        // returned 0 → HasLabel(0,…) failed → "not one of M/J/C".
        HERO_GUID
    }

    // ===== Player economy → the host's real cash/fuel store (was trait-default 0). Signed-i32 domain
    // with the documented 1-billion cash soft-clamp; fuel clamped to capacity. =====
    fn player_cash(&self) -> i64 {
        self.cash
    }
    fn player_set_cash(&mut self, cash: i64) {
        self.cash = cash.clamp(0, 1_000_000_000);
    }
    fn player_fuel(&self) -> i64 {
        self.fuel
    }
    fn player_set_fuel(&mut self, fuel: i64) {
        // Clamp to capacity once it's been set; unbounded before then (Lua may set fuel before capacity).
        let cap = if self.fuel_capacity > 0 { self.fuel_capacity } else { i64::MAX };
        self.fuel = fuel.clamp(0, cap);
    }
    fn player_fuel_capacity(&self) -> i64 {
        self.fuel_capacity
    }
    fn player_set_fuel_capacity(&mut self, cap: i64) {
        self.fuel_capacity = cap.max(0);
        self.fuel = self.fuel.min(self.fuel_capacity);
    }

    fn object_set_yaw(&mut self, guid: u64, yaw: f32) {
        // Live entity: set its Transform rotation about +Y.
        if self.with_transform_mut(guid, |t| t.rotation = mercs2_core::glam::Quat::from_rotation_y(yaw)) {
            return;
        }
        // Fallback: an un-realized spawn request's recorded yaw.
        if let Some(r) = self.req_mut(guid) {
            r.yaw = yaw;
        }
    }
    fn teleport_hero(&mut self, pos: [f32; 3]) {
        self.hero_teleport = Some(pos);
    }
    fn add_layers(&mut self, _layers: &[String]) {}

    // ===== Sound / music → the live `crate::audio::AudioEngine` (the fleet audio system, wired in). =====
    fn sound_cue(&mut self, cue: &str) -> u64 {
        // Unknown cue (no sounddb / not found) returns 0 → Lua nil, faithful to the exe.
        self.audio.borrow_mut().cue_sound_by_name(cue, None, None).map(|v| v.0 as u64).unwrap_or(0)
    }
    fn sound_stop(&mut self, voice: u64) {
        self.audio.borrow_mut().stop_sound(VoiceId(voice as u32));
    }
    fn sound_pause(&mut self, voice: u64) {
        self.audio.borrow_mut().pause_sound(VoiceId(voice as u32));
    }
    fn sound_stop_all(&mut self) {
        self.audio.borrow_mut().stop_and_flush_all_sounds();
    }
    fn sound_set_master_volume(&mut self, vol: f32) {
        self.audio.borrow_mut().set_master_volume(vol, 0.0);
    }
    fn sound_transition_music(&mut self, state: &str) -> bool {
        self.audio.borrow_mut().transition_music(state)
    }
    fn sound_add_music_state(&mut self, name: &str) {
        self.audio.borrow_mut().add_music_state(name, [0.0; 5]);
    }
    fn sound_add_music_transition(&mut self, from: &str, to: &str) {
        self.audio.borrow_mut().add_music_transition(from, to);
    }
    fn sound_set_dynamic_music(&mut self, on: bool) {
        self.audio.borrow_mut().set_dynamic_music(on);
    }
    fn sound_is_dynamic_music(&self) -> bool {
        self.audio.borrow().is_dynamic_music()
    }
    fn sound_set_category_pitch(&mut self, category: &str, pitch: f32, length: f32) {
        self.audio.borrow_mut().set_category_pitch(category, pitch, length);
    }
    fn sound_load_bank(&mut self, name: &str, wave: bool) -> bool {
        // Residency tracking is real (BankManager slots); the wave/sound distinction picks the loader.
        let mut a = self.audio.borrow_mut();
        if wave { a.load_wave_bank(name, None) } else { a.load_sound_bank(name, None) }
    }
    fn sound_unload_bank(&mut self, name: &str) -> bool {
        self.audio.borrow_mut().unload_bank(name, None)
    }
    fn sound_request_ambience_bank(&mut self, name: &str) -> bool {
        self.audio.borrow_mut().request_ambience_bank(name)
    }
    fn sound_bank_loaded(&self, name: &str) -> bool {
        self.audio.borrow().bank_is_loaded(name)
    }

    // ===== AI order surface → the recovered mechanism (`crate::ai::AiWorld`). =====
    fn ai_goal(&mut self, guid: u64, goal: &str) -> bool {
        self.ai.goal(guid as u32, goal)
    }
    fn ai_direct_action(&mut self, guid: u64, action_hash: u32) -> bool {
        self.ai.direct_action(guid as u32, action_hash)
    }
    fn ai_set_relation(&mut self, from: u64, to: u64, value: i64) {
        self.ai.set_relation(from as u32, to as u32, value as i32);
    }
    fn ai_get_relation(&self, from: u64, to: u64) -> i64 {
        self.ai.get_relation(from as u32, to as u32) as i64
    }
    fn ai_set_state(&mut self, guid: u64, state: &str, on: bool) -> bool {
        self.ai_states.entry(guid).or_default().set_state(state, on)
    }
    fn ai_order(&mut self, guid: u64, verb: &str) -> bool {
        self.ai.order(guid as u32, verb)
    }
    fn ai_add_infraction(&mut self, _offender: u64, faction: u64, amount: i64) {
        self.faction.add_scripted_infraction(faction as u32, amount as i32);
    }
    fn ai_set_infraction_multiplier(&mut self, faction: u64, multiplier: i64) {
        self.faction.set_infraction_multiplier(faction as u32, multiplier as i32);
    }
    fn ai_tweak_spawners(&mut self, _target: u64, group_mask: u8, state: Option<&str>, force_respawn: bool) -> u32 {
        // Map the Lua `{SpawnerState=…}` verb to the recovered spawner state byte: "on" resumes,
        // "off"/"despawn" force-despawns (terminal state 5). Unknown/absent ⇒ no state overwrite.
        let spawner_state = state.and_then(|s| match s.to_ascii_lowercase().as_str() {
            "on" => Some(0u8),
            "off" | "despawn" | "disable" => Some(5u8),
            _ => None,
        });
        let adjust = crate::population::SpawnerAdjust {
            group_mask,
            spawner_state,
            spawn_list: None,
            force_respawn,
        };
        self.population.tweak_attached_spawners(&adjust)
    }
    fn ai_set_attitude(&mut self, faction: u64, toward: u64, relation: i64) {
        // `Ai.SetAttitude`/`ChangeRelation` write the faction manager's directed relation (which emits
        // the attitude event + drives price/pursuit), mirrored into the AI matrix the perception tick reads.
        self.faction.set_relation(faction as u32, toward as u32, relation as i32);
        self.ai.set_relation(faction as u32, toward as u32, relation as i32);
    }

    // ===== Vehicle hijack FSM + turret aim → `mercs2_vehicle` (held per-vehicle on the host). =====
    fn vehicle_hijack_event(&mut self, veh: u64, event: &str) -> String {
        let fsm = self.hijacks.entry(veh).or_insert_with(crate::vehicle::HijackFsm::new);
        let state = match event {
            "start" => fsm.start(),
            "tank_motion_on" => fsm.tank_motion(true),
            "tank_motion_off" => fsm.tank_motion(false),
            "success" => fsm.set_success(),
            "complete" => fsm.complete(),
            "abort" => fsm.abort(),
            "abort_done" => fsm.abort_done(),
            "cancel" => fsm.cancel(),
            other => fsm.set_state(other.strip_prefix("set:").unwrap_or(other)),
        };
        state.name().to_string()
    }
    fn vehicle_hijack_state(&self, veh: u64) -> String {
        self.hijacks.get(&veh).map(|f| f.state.name()).unwrap_or("idle").to_string()
    }
    fn vehicle_set_turret(&mut self, veh: u64, pitch: Option<f32>, yaw: Option<f32>, spin: Option<bool>) {
        let aim = self.turrets.entry(veh).or_insert_with(crate::vehicle::TurretAim::new);
        if let Some(p) = pitch {
            aim.pitch = p;
        }
        if let Some(y) = yaw {
            aim.yaw = y;
        }
        if let Some(s) = spin {
            aim.rotor_spinning = s;
        }
    }

    // ===== Sys engine-config store (Set* ↔ Get* real roundtrips). =====
    fn sys_request_game_state(&mut self, state: &str) {
        // Queue the requested state; the resident pump services it (fires Event.GameStateChange) so the
        // MrxState world-load chain advances (Loading → WaitForGame → GlobalEnter → … → GlobalExit).
        self.pending_game_states.push(state.to_string());
    }
    fn sys_set_time_scale(&mut self, scale: f32) {
        self.settings.time_scale = scale.max(0.0);
    }
    fn sys_time_scale(&self) -> f32 {
        self.settings.time_scale
    }
    fn sys_set_level_name(&mut self, name: &str) {
        self.level = name.to_string();
    }
    fn sys_set_master_script_name(&mut self, name: &str) {
        self.settings.master_script = name.to_string();
    }
    fn sys_master_script_name(&self) -> String {
        if self.settings.master_script.is_empty() {
            self.level.clone()
        } else {
            self.settings.master_script.clone()
        }
    }
    fn sys_set_tutorials_enabled(&mut self, on: bool) {
        self.settings.tutorials_enabled = on;
    }
    fn sys_tutorials_enabled(&self) -> bool {
        self.settings.tutorials_enabled
    }
    fn sys_set_autosave_enabled(&mut self, on: bool) {
        self.settings.autosave_enabled = on;
    }
    fn sys_set_lua_save_version(&mut self, version: i64) {
        self.settings.lua_save_version = version;
    }
    fn sys_set_viewports(&mut self, n: i64) {
        self.settings.viewports = n.max(1);
    }
    fn sys_set_asset_request_max(&mut self, n: i64) {
        self.settings.asset_request_max = n.max(0);
    }
    fn sys_start_singleplayer(&mut self) {
        self.settings.singleplayer = true;
    }

    // ===== Object labels + ObjectFilter query registry. =====
    fn object_add_label(&mut self, guid: u64, label: &str) {
        self.object_labels.entry(guid).or_default().insert(label.to_string());
    }
    fn object_remove_label(&mut self, guid: u64, label: &str) {
        if let Some(set) = self.object_labels.get_mut(&guid) {
            set.remove(label);
        }
    }
    fn object_has_label(&self, guid: u64, label: &str) -> bool {
        self.object_labels.get(&guid).is_some_and(|s| s.contains(label))
    }
    fn object_filter_create(&mut self) -> u64 {
        self.object_filters.create()
    }
    fn object_filter_copy(&mut self, src: u64) -> u64 {
        self.object_filters.copy(src)
    }
    fn object_filter_set_expr(&mut self, handle: u64, expr: &str) {
        if let Some(f) = self.object_filters.get_mut(handle) {
            f.expr = expr.to_string();
        }
    }
    fn object_filter_add(&mut self, handle: u64, guid: u64, include: bool) {
        if let Some(f) = self.object_filters.get_mut(handle) {
            f.add(guid, include);
        }
    }
    fn object_filter_remove(&mut self, handle: u64, guid: u64) {
        if let Some(f) = self.object_filters.get_mut(handle) {
            f.remove(guid);
        }
    }
    fn object_filter_clear(&mut self, handle: u64) {
        if let Some(f) = self.object_filters.get_mut(handle) {
            f.clear_objects();
        }
    }
    fn object_filter_use_players(&mut self, handle: u64, on: bool) {
        if let Some(f) = self.object_filters.get_mut(handle) {
            f.use_players = on;
        }
    }
    fn object_filter_objects(&self, handle: u64) -> Vec<u64> {
        self.object_filters.get(handle).map(|f| f.include.clone()).unwrap_or_default()
    }
    fn object_filter_eval(&self, handle: u64, guid: u64) -> bool {
        match self.object_filters.get(handle) {
            Some(f) => f.matches(guid, |label| self.object_has_label(guid, label)),
            None => false,
        }
    }
    fn object_filter_gc(&mut self, handle: u64) {
        self.object_filters.remove(handle);
    }

    // ===== HUD widget tree + markers → mercs2_ui. =====
    fn hud(&mut self) -> Option<&mut crate::widgets::WidgetTree> {
        Some(&mut self.hud)
    }
    fn hud_ref(&self) -> Option<&crate::widgets::WidgetTree> {
        Some(&self.hud)
    }
    fn markers(&mut self) -> Option<&mut crate::widgets::MarkerSet> {
        Some(&mut self.markers)
    }
    fn markers_ref(&self) -> Option<&crate::widgets::MarkerSet> {
        Some(&self.markers)
    }
    fn render_state(&mut self) -> Option<&mut mercs2_core::RenderState> {
        Some(&mut self.render)
    }
    fn render_state_ref(&self) -> Option<&mercs2_core::RenderState> {
        Some(&self.render)
    }

    // ===== Cinematic camera controller. =====
    fn camera_set_yaw(&mut self, yaw: f32) { self.camera_fx.yaw = yaw; }
    fn camera_yaw(&self) -> f32 { self.camera_fx.yaw }
    fn camera_set_pitch(&mut self, pitch: f32) { self.camera_fx.pitch = pitch; }
    fn camera_pitch(&self) -> f32 { self.camera_fx.pitch }
    fn camera_set_fov(&mut self, fov: f32) { self.camera_fx.fov = fov; }
    fn camera_fov(&self) -> f32 { self.camera_fx.fov }
    fn camera_set_position(&mut self, pos: [f32; 3]) { self.camera_fx.position = pos; }
    fn camera_set_lookat(&mut self, target: [f32; 3]) { self.camera_fx.lookat = target; }
    fn camera_shake(&mut self, intensity: f32) { self.camera_fx.shake = intensity; }
    fn camera_set_blending(&mut self, on: bool) { self.camera_fx.blending = on; }
    fn camera_follow(&mut self, guid: u64) { self.camera_fx.follow_guid = guid; }
    fn camera_hold(&mut self, on: bool) { self.camera_fx.held = on; }
    fn camera_set_shot(&mut self, shot: &str) { self.camera_fx.shot = shot.to_string(); }

    // ===== Inventory: per-character weapon loadout. =====
    fn inventory_set_weapons(&mut self, character: u64, weapons: Vec<u64>) {
        self.loadouts.insert(character, weapons);
    }
    fn inventory_weapons(&self, character: u64) -> Vec<u64> {
        self.loadouts.get(&character).cloned().unwrap_or_default()
    }
    fn inventory_primary(&self, character: u64) -> u64 {
        self.loadouts.get(&character).and_then(|w| w.first().copied()).unwrap_or(0)
    }
    fn inventory_secondary(&self, character: u64) -> u64 {
        self.loadouts.get(&character).and_then(|w| w.get(1).copied()).unwrap_or(0)
    }
    fn inventory_equip(&mut self, character: u64, weapon: u64) {
        let slots = self.loadouts.entry(character).or_default();
        if !slots.contains(&weapon) {
            slots.push(weapon);
        }
    }
    fn inventory_drop(&mut self, character: u64, weapon: u64) {
        if let Some(slots) = self.loadouts.get_mut(&character) {
            slots.retain(|&w| w != weapon);
        }
    }
    fn inventory_destroy_all(&mut self, character: u64) {
        self.loadouts.remove(&character);
    }

    // ===== Weapon ammo. =====
    fn weapon_set_ammo(&mut self, weapon: u64, clip: Option<i32>, reserve: Option<i32>) {
        let w = self.weapons.entry(weapon).or_default();
        if let Some(c) = clip {
            w.clip = c.max(0);
            w.max_clip = w.max_clip.max(w.clip);
        }
        if let Some(r) = reserve {
            w.reserve = r.max(0);
            w.max_reserve = w.max_reserve.max(w.reserve);
        }
    }
    fn weapon_clip(&self, weapon: u64) -> i32 {
        self.weapons.get(&weapon).map(|w| w.clip).unwrap_or(0)
    }
    fn weapon_reserve(&self, weapon: u64) -> i32 {
        self.weapons.get(&weapon).map(|w| w.reserve).unwrap_or(0)
    }
    fn weapon_max_clip(&self, weapon: u64) -> i32 {
        self.weapons.get(&weapon).map(|w| w.max_clip).unwrap_or(WeaponState::default().max_clip)
    }
    fn weapon_max_reserve(&self, weapon: u64) -> i32 {
        self.weapons.get(&weapon).map(|w| w.max_reserve).unwrap_or(WeaponState::default().max_reserve)
    }
    fn weapon_reload(&mut self, weapon: u64) {
        let w = self.weapons.entry(weapon).or_default();
        let need = (w.max_clip - w.clip).max(0);
        let take = need.min(w.reserve);
        w.clip += take;
        w.reserve -= take;
    }
    fn weapon_is_primary(&self, weapon: u64) -> bool {
        self.weapons.get(&weapon).map(|w| w.primary).unwrap_or(true)
    }
    fn weapon_is_designator(&self, weapon: u64) -> bool {
        self.weapons.get(&weapon).map(|w| w.designator).unwrap_or(false)
    }

    // ===== Fire. =====
    fn fire_ignite(&mut self, object: u64) {
        self.burning.insert(object);
    }
    fn fire_extinguish(&mut self, object: u64) {
        self.burning.remove(&object);
    }
    fn object_is_burning(&self, object: u64) -> bool {
        self.burning.contains(&object)
    }

    // ===== Health / damage → the live `crate::combat::Health` component (shared with combat), with the
    // shadow HashMap as the pre-realize / no-entity fallback (like `spawns[].pos` for position). =====
    fn object_health(&self, guid: u64) -> f32 {
        if let Some(h) = self.health_of(guid) {
            return h.cur;
        }
        self.health.get(&guid).map(|&(c, _)| c).unwrap_or(DEFAULT_MAX_HEALTH)
    }
    fn object_set_health(&mut self, guid: u64, hp: f32) {
        if self.with_health(guid, DEFAULT_MAX_HEALTH, |h| h.cur = hp.clamp(0.0, h.max)) {
            return;
        }
        let e = self.health.entry(guid).or_insert((DEFAULT_MAX_HEALTH, DEFAULT_MAX_HEALTH));
        e.0 = hp.clamp(0.0, e.1);
    }
    fn object_max_health(&self, guid: u64) -> f32 {
        if let Some(h) = self.health_of(guid) {
            return h.max;
        }
        self.health.get(&guid).map(|&(_, m)| m).unwrap_or(DEFAULT_MAX_HEALTH)
    }
    fn object_is_alive(&self, guid: u64) -> bool {
        if let Some(h) = self.health_of(guid) {
            return h.cur > 0.0;
        }
        self.health.get(&guid).map(|&(c, _)| c > 0.0).unwrap_or(true)
    }
    fn object_kill(&mut self, guid: u64) {
        if self.with_health(guid, DEFAULT_MAX_HEALTH, |h| h.cur = 0.0) {
            return;
        }
        let e = self.health.entry(guid).or_insert((DEFAULT_MAX_HEALTH, DEFAULT_MAX_HEALTH));
        e.0 = 0.0;
    }
    fn object_revive(&mut self, guid: u64) {
        if self.with_health(guid, DEFAULT_MAX_HEALTH, |h| h.cur = h.max) {
            return;
        }
        let e = self.health.entry(guid).or_insert((DEFAULT_MAX_HEALTH, DEFAULT_MAX_HEALTH));
        e.0 = e.1;
    }
    fn object_send_damage(&mut self, target: u64, amount: f32) -> bool {
        // Live entity: subtract from the shared Health; report whether it died.
        if let Some(h) = self.health_of(target) {
            let died = (h.cur - amount) <= 0.0;
            self.with_health(target, DEFAULT_MAX_HEALTH, |h| h.cur = (h.cur - amount).max(0.0));
            return died;
        }
        let e = self.health.entry(target).or_insert((DEFAULT_MAX_HEALTH, DEFAULT_MAX_HEALTH));
        e.0 = (e.0 - amount).max(0.0);
        e.0 <= 0.0
    }

    // ===== Pg regions + alarms. =====
    fn pg_create_region(&mut self, name: &str, center: [f32; 3], radius: f32) -> u64 {
        // Re-creating a named region reuses its handle (idempotent for mission re-entry).
        let handle = *self.region_names.entry(name.to_string()).or_insert_with(|| {
            let h = self.next_region;
            self.next_region += 1;
            h
        });
        self.regions.insert(handle, (center, radius));
        handle
    }
    fn pg_alarm_set(&mut self, guid: u64, on: bool) {
        if on {
            self.alarms.insert(guid);
        } else {
            self.alarms.remove(&guid);
        }
    }
    fn pg_alarm_toggle(&mut self, guid: u64) -> bool {
        if self.alarms.contains(&guid) {
            self.alarms.remove(&guid);
            false
        } else {
            self.alarms.insert(guid);
            true
        }
    }
    fn pg_alarm_active(&self, guid: u64) -> bool {
        self.alarms.contains(&guid)
    }

    // ===== Airstrike designators + ordnance. =====
    fn airstrike_equip_designator(&mut self, player: u64) {
        self.designators.insert(player, DESIGNATOR_CHARGES);
    }
    fn airstrike_remove_designator(&mut self, player: u64) {
        self.designators.remove(&player);
    }
    fn airstrike_refill_designator(&mut self, player: u64) {
        self.designators.insert(player, DESIGNATOR_CHARGES);
    }
    fn airstrike_designator_owner(&self) -> u64 {
        self.designators.keys().copied().min().unwrap_or(0)
    }
    fn airstrike_spawn(&mut self, kind: &str, pos: [f32; 3]) {
        self.airstrikes.push((kind.to_string(), pos));
    }

    // ===== Object attachment graph (Attach/Detach ↔ GetParent/IsAttached/GetAttachedObjects). =====
    fn object_attach(&mut self, child: u64, parent: u64) {
        self.attachments.insert(child, parent);
    }
    fn object_detach(&mut self, child: u64) {
        self.attachments.remove(&child);
    }
    fn object_parent(&self, guid: u64) -> u64 {
        self.attachments.get(&guid).copied().unwrap_or(0)
    }
    fn object_is_attached(&self, guid: u64) -> bool {
        self.attachments.contains_key(&guid)
    }
    fn object_attached_objects(&self, guid: u64) -> Vec<u64> {
        self.attachments.iter().filter(|(_, &p)| p == guid).map(|(&c, _)| c).collect()
    }

    // ===== VO / dialogue → the real `crate::audio::VoManager` (via the shared AudioEngine). =====
    fn vo_cue(&mut self, cue: &str) -> u64 {
        // Cue names hash to a stable u32 guid so Cue↔Cancel(cue) address the same VO line. Contract
        // priority is the default mission-dialogue tier; the VO routes through the real voice pool.
        let guid = vo_cue_hash(cue);
        let ok = self.audio.borrow_mut().vo_cue(0, guid, crate::audio::VoPriority::Contract, true, None);
        if ok { guid as u64 } else { 0 }
    }
    fn vo_cancel(&mut self, cue: &str) {
        self.audio.borrow_mut().vo_cancel(vo_cue_hash(cue));
    }
    fn vo_cancel_all(&mut self) {
        self.audio.borrow_mut().vo_cancel_all();
    }
    fn vo_set_paused(&mut self, paused: bool) {
        self.audio.borrow_mut().vo_set_paused(paused);
    }
    fn vo_set_cinematic_mode(&mut self, enable: bool) {
        self.audio.borrow_mut().vo_set_cinematic_mode(enable);
    }

    // ===== Player identity / session / binding (single local player controlling the hero). =====
    fn player_local_player(&self) -> u64 {
        LOCAL_PLAYER_GUID
    }
    fn player_get_player(&self, id: i64) -> u64 {
        if id <= 1 { LOCAL_PLAYER_GUID } else { 0 }
    }
    fn player_primary_player(&self) -> u64 {
        LOCAL_PLAYER_GUID
    }
    fn player_character_of(&self, player: u64) -> u64 {
        if let Some(&c) = self.player_character.get(&player) {
            return c;
        }
        if player == LOCAL_PLAYER_GUID {
            HERO_GUID
        } else {
            0
        }
    }
    fn player_is_local(&self, guid: u64) -> bool {
        // The local player and the hero it controls are local; a hypothetical second player is remote.
        guid == LOCAL_PLAYER_GUID || guid == HERO_GUID
    }
    fn player_selected_character(&self) -> String {
        self.hero_character.clone()
    }
    fn player_attach_to_character(&mut self, player: u64, character: u64) {
        self.player_character.insert(player, character);
    }
    fn player_detach_from_character(&mut self, player: u64) {
        self.player_character.remove(&player);
    }
    fn player_unbind(&mut self, player: u64) {
        self.player_character.remove(&player);
    }

    // ===== Object identity (derived from the recorded spawn requests + the hero). =====
    fn object_name(&self, guid: u64) -> String {
        self.name_of(guid).unwrap_or("").to_string()
    }
    fn object_model_name(&self, guid: u64) -> String {
        self.template_of(guid).unwrap_or("").to_string()
    }
    fn object_is_player_controlled(&self, guid: u64) -> bool {
        guid == HERO_GUID
    }
    fn object_is_valid(&self, guid: u64) -> bool {
        guid == HERO_GUID
            || self.by_guid.contains_key(&guid)
            || self.entity_of(guid).is_some()
    }

    // ===== Human driven state (record-then-realize, keyed by GUID). =====
    fn human_set_state(&mut self, guid: u64, stance: &str, action: &str) {
        self.human_states
            .insert(guid, (stance.to_string(), action.to_string()));
    }
    fn human_do_action(&mut self, guid: u64, action: &str) {
        // Keep the current stance; DoAction only changes the one-shot action.
        let stance = self
            .human_states
            .get(&guid)
            .map(|(s, _)| s.clone())
            .unwrap_or_default();
        self.human_states.insert(guid, (stance, action.to_string()));
    }
    fn human_is_swimming(&self, guid: u64) -> bool {
        self.human_flags.get(&guid).map(|f| f.swimming).unwrap_or(false)
    }
    fn human_is_carrying(&self, guid: u64) -> bool {
        self.human_flags.get(&guid).map(|f| f.carrying).unwrap_or(false)
    }
    fn human_is_grappling(&self, guid: u64) -> bool {
        self.human_flags.get(&guid).map(|f| f.grappling).unwrap_or(false)
    }
    fn human_enable_weapons(&mut self, guid: u64, on: bool) {
        self.human_flags.entry(guid).or_default().weapons_enabled = on;
    }
    fn human_weapons_enabled(&self, guid: u64) -> bool {
        self.human_flags.get(&guid).map(|f| f.weapons_enabled).unwrap_or(true)
    }
    fn human_set_fire_lock(&mut self, guid: u64, on: bool) {
        self.human_flags.entry(guid).or_default().fire_lock = on;
    }
    fn human_knockdown(&mut self, guid: u64) {
        let f = self.human_flags.entry(guid).or_default();
        f.knocked_down = true;
        f.ragdoll = true;
    }
    fn human_set_ragdoll(&mut self, guid: u64, on: bool) {
        self.human_flags.entry(guid).or_default().ragdoll = on;
    }
    fn human_is_knocked_down(&self, guid: u64) -> bool {
        self.human_flags.get(&guid).map(|f| f.knocked_down).unwrap_or(false)
    }
    fn human_stop_grappling(&mut self, guid: u64) {
        self.human_flags.entry(guid).or_default().grappling = false;
    }
    fn human_drop_carried(&mut self, guid: u64) {
        self.human_flags.entry(guid).or_default().carrying = false;
    }
    fn human_set_jostle(&mut self, guid: u64, on: bool) {
        self.human_flags.entry(guid).or_default().jostle_enabled = on;
    }
    fn human_set_corpse_cleanup(&mut self, guid: u64, on: bool) {
        self.human_flags.entry(guid).or_default().corpse_cleanup = on;
    }
    fn human_set_weapon_drawn(&mut self, guid: u64, drawn: bool) {
        self.human_flags.entry(guid).or_default().weapon_drawn = drawn;
    }

    // ===== Net session mode. =====
    fn net_session_start(&mut self, mode: &str, host: Option<&str>) {
        self.net.mode = match mode {
            "client" => NetMode::Client,
            "lobby" => NetMode::Lobby,
            _ => NetMode::Server,
        };
        self.net.active = true;
        self.net.multiplayer = true;
        if let Some(h) = host {
            self.net.host_name = h.to_string();
        }
    }
    fn net_stop(&mut self) {
        self.net = NetState::default();
    }
    fn net_is_server(&self) -> bool {
        self.net.mode == NetMode::Server
    }
    fn net_is_client(&self) -> bool {
        self.net.mode == NetMode::Client
    }
    fn net_is_active(&self) -> bool {
        self.net.active
    }
    fn net_is_multiplayer(&self) -> bool {
        self.net.multiplayer
    }
    fn net_is_lobby(&self) -> bool {
        self.net.mode == NetMode::Lobby
    }
    fn net_host_name(&self) -> String {
        self.net.host_name.clone()
    }

    // ===== Object state machine + emitters. =====
    fn object_sm_set_state(&mut self, guid: u64, state: &str) {
        self.object_states_sm.insert(guid, state.to_string());
    }
    fn object_sm_state(&self, guid: u64) -> String {
        self.object_states_sm.get(&guid).cloned().unwrap_or_default()
    }
    fn object_start_emitter(&mut self, guid: u64, name: &str) {
        self.emitters.entry(guid).or_default().insert(name.to_string());
    }
    fn object_stop_emitter(&mut self, guid: u64, name: &str) {
        if let Some(set) = self.emitters.get_mut(&guid) {
            set.remove(name);
        }
    }
    fn object_emitter_active(&self, guid: u64, name: &str) -> bool {
        self.emitters.get(&guid).is_some_and(|s| s.contains(name))
    }

    // ===== Facial animation. =====
    fn face_bind_anim_set(&mut self, guid: u64, set: Option<&str>) {
        let e = self.faces.entry(guid).or_default();
        e.0 = set.unwrap_or("").to_string();
    }
    fn face_play(&mut self, guid: u64, name: &str) {
        self.faces.entry(guid).or_default().1 = name.to_string();
    }
    fn face_current(&self, guid: u64) -> String {
        self.faces.get(&guid).map(|(_, e)| e.clone()).unwrap_or_default()
    }

    // ===== Mission report → the faction manager. =====
    fn report_init(&mut self) {
        // The faction reporter scores infractions against the PMC faction.
        self.report = Some((self.faction.pmc() as u64, 0.0));
    }
    fn report_set_delay(&mut self, seconds: f32) {
        if let Some(r) = self.report.as_mut() {
            r.1 = seconds;
        }
    }
    fn report_finish(&mut self, _success: bool) {
        // Finalize: flush the faction's accumulated infractions into its relation (the mood report).
        if let Some((faction, _)) = self.report.take() {
            self.faction.report(faction as u32);
        }
    }
    fn report_infractions(&self) -> i64 {
        match self.report {
            Some((faction, _)) => {
                let acc = self.faction.accumulator(faction as u32);
                if acc.is_empty() { 0 } else { 1 }
            }
            None => 0,
        }
    }

    // ===== Player mode flags. =====
    fn player_set_mode(&mut self, key: &str, on: bool) {
        self.player_modes.insert(key.to_string(), on);
    }
    fn player_mode(&self, key: &str, default: bool) -> bool {
        self.player_modes.get(key).copied().unwrap_or(default)
    }
    fn player_set_mode_scalar(&mut self, key: &str, value: f32) {
        self.player_scalars.insert(key.to_string(), value);
    }

    // ===== Seat occupancy + weapon restore. =====
    fn human_enter_seat(&mut self, human: u64, seat: u64) {
        self.human_seats.insert(human, seat);
    }
    fn human_exit_seat(&mut self, human: u64) {
        self.human_seats.remove(&human);
    }
    fn human_seat(&self, human: u64) -> u64 {
        self.human_seats.get(&human).copied().unwrap_or(0)
    }
    fn weapon_restore_ammo(&mut self, weapon: u64) {
        let w = self.weapons.entry(weapon).or_default();
        w.clip = w.max_clip;
        w.reserve = w.max_reserve;
    }
    fn sound_cmd(&mut self, verb: &str, args: Vec<String>) {
        log_binding("Sound", verb, &args);
        self.sound_cmds.push((verb.to_string(), args));
    }
    fn net_event(&mut self, verb: &str, args: Vec<String>) {
        log_binding("Net", verb, &args);
        self.net_events.push((verb.to_string(), args));
    }
    fn script_cmd(&mut self, verb: &str, args: Vec<String>) {
        // `verb` is already namespaced ("Ns.Verb"); split for a clean log line.
        let (ns, v) = verb.split_once('.').unwrap_or(("Script", verb));
        log_binding(ns, v, &args);
        self.script_cmds.push((verb.to_string(), args));
    }
}

/// Boot the PMC interior THROUGH the script host and return the actor-spawn intents the engine must
/// realize. Prefers the REAL `MrxUtil.SpawnActor` (imported from the decompiled Lua corpus); falls
/// back to an inlined copy of its body if the corpus isn't reachable or the import cascade fails, so
/// the game boot never breaks. Either way the interior spawns because the script asked for it.
pub fn run_interior_boot() -> Vec<SpawnRequest> {
    if let Some(root) = discover_lua_root() {
        match run_interior_boot_real(&root) {
            Ok(spawns) if !spawns.is_empty() => {
                println!(
                    "[script] interior boot via REAL MrxUtil.SpawnActor (corpus {}): {} spawn(s)",
                    root.display(),
                    spawns.len()
                );
                return spawns;
            }
            Ok(_) => println!("[script] real boot produced no spawns; using inline glue"),
            Err(e) => println!("[script] real boot failed ({e}); using inline glue"),
        }
    }
    run_interior_boot_inline()
}

/// Build a **loop-resident** `ScriptHost` bound to `host` — the persistent mission-Lua VM the game loop
/// pumps every frame (`Event.__pump`, runtime `Pg.Spawn`, `Sound.*`), as opposed to the one-shot
/// [`run_interior_boot`] host that is dropped after harvesting the boot spawns. Registers the engine
/// bindings against `host` and enables auto-stubbing so the game modules' load-time binding-table
/// touches (VO/Hud/Net/…) don't error. Returns `None` (with a logged reason) if the VM can't start, so
/// the boot degrades to a script-less world rather than failing.
///
/// Keystone K1 (`engine_support_inventory.md` §6.1): the host is the socket the whole
/// record-then-realize spawn path + the Lua event/timer system + audible `Sound.*` cues plug into.
pub fn resident_script_host(host: Rc<RefCell<GameScriptHost>>) -> Option<ScriptHost> {
    use std::collections::BTreeSet;
    let sh = match discover_lua_root() {
        Some(root) => ScriptHost::new(vec![root]),
        None => ScriptHost::bare(),
    };
    let sh = match sh {
        Ok(s) => s,
        Err(e) => {
            println!("[script] resident host init failed ({e}); world runs script-less");
            return None;
        }
    };
    match sh.register_engine_reported(host) {
        Ok(cov) => {
            let ns = cov.len();
            let total: usize = cov.iter().map(|c| c.required.len()).sum();
            println!("[bind] engine binding surface installed: {total} cfuncs across {ns} namespaces (watch for [bind] lines as the game's Lua drives them)");
        }
        Err(e) => {
            println!("[script] resident register_engine failed ({e}); world runs script-less");
            return None;
        }
    }
    // Auto-stub the binding tables that game modules touch at load time (logged no-ops); the real
    // gameplay bindings (Pg.Spawn/Object.*/Event/Sound/Ai) stay live.
    let trace: Rc<RefCell<BTreeSet<String>>> = Rc::new(RefCell::new(BTreeSet::new()));
    if let Err(e) = sh.enable_autostub(trace) {
        println!("[script] resident autostub failed ({e}); world runs script-less");
        return None;
    }
    Some(sh)
}

/// Run the **real vanilla boot Lua flow** through the resident host (bisect against the pmc_bb
/// `[lua]` trace). `MrxBootstrap.Start()` (mrxbootstrap.lua:14) imports the resident modules
/// (MrxPlayer/MrxPmc/MrxState/MrxUtil/…), registers the GUI-loaded + local-player-joined callbacks, and
/// calls `MrxPlayer.Start()`. Each `Debug.Printf` in that cascade surfaces as a `[lua]` line here, so
/// this is exactly what to diff against vanilla to find the first divergence. The spawn itself
/// (`MrxPlayer.OnPlayerJoined` → `SetSpawnLocations`/`CreatePlayerCharacter`) is event-driven — it fires
/// once the engine signals GUI-loaded + player-joined (wired next). Errors are logged, not fatal.
pub fn run_boot_flow(sh: &ScriptHost, host: &Rc<RefCell<GameScriptHost>>, contract: &str, character: &str) {
    println!("[world] ===== vanilla boot Lua flow: MrxBootstrap.Start() =====");
    // Drive the flow the way the engine does: MrxBootstrap.Start() registers the callbacks, then the
    // mission flow sets the spawn location (SetSpawnLocations(<Contract>_Start1)) and the player-joined
    // path spawns the hero (CreatePlayerCharacter → Pg.GetGuidByName → Object.GetPosition → Pg.Spawn).
    // Wrapped in pcall so a later unbacked call (AttachToCharacter/OnPlayerInit) doesn't abort — the
    // Pg.Spawn (the hero placement) runs first, so the spawn is captured regardless.
    let src = format!(
        "import(\"MrxBootstrap\")\n\
         import(\"MrxPlayer\")\n\
         import(\"MrxGui\")\n\
         import(\"LevelBootstrap\")\n\
         LevelBootstrap.LoadLevel(\"vz\", \"vz\")\n\
         -- Shell-bootstrap fade setup (MrxGuiShellBootstrap.LoadMovieLayouts -> _InitFadeFlash) that we\n\
         -- skip by not running the shell: create the fade-flash widget the GlobalEnter fade uses.\n\
         local fe, fi = pcall(MrxGui._InitFadeFlash)\n\
         if not fe then Debug.Printf(\"_InitFadeFlash aborted: \" .. tostring(fi)) end\n\
         -- Run the vz master script as the SOLE boot entry. Its Init is the real boot: \n\
         --   SetHandleStateTransitions(false) + MrxBootstrap.Start(_AttemptGameplaySetup) +\n\
         --   MrxState.EnableFade(false) + MrxPlayer.Reset + LoadSingleton(nil) -> _LoadLayers ->\n\
         --   MrxLayerManager.Add -> the layer streaming the pump completes (Pg.LoadLayer callback) ->\n\
         --   _AttemptGameplaySetup static/dynamic -> MrxPlayer.Start (spawn) + _CompleteGameplaySetup\n\
         --   (act staging). We only supply the two async gates the non-rendering load can't signal.\n\
         local me, mi = pcall(import, \"xQ!L\")\n\
         if not me then Debug.Printf(\"master script (vz) aborted: \" .. tostring(mi)) end\n\
         MrxPlayer.SetSpawnLocations({{ \"{contract}_Start1\" }})\n\
         -- GUI-load-complete gate (the shell's GUI-file loads finish).\n\
         local ge, ie = pcall(MrxBootstrap._GuiLoaded)\n\
         if not ge then Debug.Printf(\"_GuiLoaded aborted: \" .. tostring(ie)) end\n"
    );
    let _ = character;
    match sh.exec(&src, "@boot_flow") {
        Ok(()) => println!("[world] ===== boot flow started (Start + spawn); servicing state machine ====="),
        Err(e) => println!("[world] ===== boot flow error (first divergence): {e} ====="),
    }

    // Service the world-load state machine: pump the Lua timer/event system and fire the
    // `Event.GameStateChange` events for each `Sys.RequestGameState` the chain requests (we have no real
    // streaming/tether wait, so each requested state completes immediately — enter then exit). This
    // advances MrxState: Loading → WaitForGame → GlobalEnter → WaitForStreaming → … → GlobalExit.
    let mut idle_rounds = 0;
    for _ in 0..1200 {
        let before = host.borrow().lua_log_lines;
        pump_resident(sh, 0.1);
        let states = host.borrow_mut().take_pending_game_states();
        let serviced = !states.is_empty();
        for st in states {
            // Firing the "exit" phase runs the state's ReadyToExit callbacks — for WaitForStreaming that
            // is `_SecondaryStreamComplete → _StartPlayerVisibleGameplay → WifMissionFlow.Refresh(Exit,
            // WAITFORGAME)`, the chain that reaches GlobalExit. Surface any error (don't swallow it).
            if let Err(e) = sh.fire_state_change(&st, "enter") {
                println!("[script] GameStateChange({st}, enter) error: {e}");
            }
            if let Err(e) = sh.fire_state_change(&st, "exit") {
                println!("[script] GameStateChange({st}, exit) error: {e}");
            }
        }
        // Progress = a state was serviced OR the Lua produced new output (a timer/callback fired).
        let progressed = serviced || host.borrow().lua_log_lines != before;
        if progressed {
            idle_rounds = 0;
        } else {
            idle_rounds += 1;
            if idle_rounds >= 20 {
                break; // truly settled: no state requests, no timers, no callbacks pending
            }
        }
    }
    println!("[world] ===== boot flow settled =====");
}

/// Advance the resident script host one fixed step: pump the Lua event/timer system (`Event.__pump(dt)`)
/// so `TimerRelative` fires and posted events dispatch. A no-op if `Event`/`__pump` aren't present.
/// Errors are logged, not fatal (a mission-script bug must not kill the render loop).
pub fn pump_resident(sh: &ScriptHost, dt: f32) {
    // Fire completed layer loads (the engine's async streaming callback) THEN pump timers/events, so the
    // MrxLayerManager fulfilment + the gameplay-setup signal it triggers advance each tick.
    if let Err(e) = sh.exec(
        &format!(
            "if Pg and Pg.__flush_layer_loads then Pg.__flush_layer_loads() end\n\
             if Event and Event.__pump then Event.__pump({dt}) end"
        ),
        "@resident_pump",
    ) {
        println!("[script] resident pump error: {e}");
    }
}

/// Locate the decompiled Lua corpus root (`docs/mercs2-luacd/src`): `MERCS2_LUA_ROOT` if set, else the
/// dev path baked from this crate's location. Returns `None` at a shipped install (corpus not present).
fn discover_lua_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MERCS2_LUA_ROOT") {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            return Some(pb);
        }
    }
    // crate dir = <repo>/tools/wad_simulator/crates/mercs2_engine → up 4 to <repo>.
    let baked = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../docs/mercs2-luacd/src");
    baked.is_dir().then_some(baked)
}

/// Run the interior boot through the REAL corpus `MrxUtil.SpawnActor` — no inlined copy. Imports the
/// module (which cascades through its own imports) and calls the actual function that ships in the
/// game. Its body uses only bindings the engine already provides (`Pg.Spawn`/`Object.*`/`Debug`/
/// `Event`), so a successful import means real game code is driving the engine.
pub fn run_interior_boot_real(root: &Path) -> Result<Vec<SpawnRequest>, String> {
    use std::collections::BTreeSet;
    let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
    let sh = ScriptHost::new(vec![root.to_path_buf()]).map_err(|e| e.to_string())?;
    sh.register_engine(host.clone()).map_err(|e| e.to_string())?;
    // Let the real import cascade COMPLETE: auto-stub the engine binding tables the game modules touch
    // at load time (VO/Hud/Net/Graphics/…) as logged no-ops. The interior spawn itself uses only the
    // real bindings (Pg.Spawn/Object.*); the stubs just keep unrelated top-level code from erroring.
    let trace: Rc<RefCell<BTreeSet<String>>> = Rc::new(RefCell::new(BTreeSet::new()));
    sh.enable_autostub(trace.clone()).map_err(|e| e.to_string())?;
    let o = PMC_INTERIOR_ACTOR_ORIGIN;
    let src = format!(
        "import(\"MrxUtil\")\n\
         MrxUtil.SpawnActor(\"{tpl}\", \"HqInterior\", {{ {x}, {y}, {z} }}, nil, 0, false, false)\n",
        tpl = PMC_INTERIOR_TEMPLATE,
        x = o[0],
        y = o[1],
        z = o[2]
    );
    sh.exec(&src, "@interior_boot_real").map_err(|e| e.to_string())?;
    let stubbed: Vec<String> = trace
        .borrow()
        .iter()
        .filter_map(|s| s.strip_prefix("global:").map(String::from))
        .collect();
    if !stubbed.is_empty() {
        println!(
            "[script] real boot completed; auto-stubbed {} engine binding table(s): {}",
            stubbed.len(),
            stubbed.join(", ")
        );
    }
    let spawns = std::mem::take(&mut host.borrow_mut().spawns);
    Ok(spawns)
}

/// The fallback: the exact inanimate-`HqInterior` branch of `MrxUtil.SpawnActor` (mrxutil.lua:463),
/// inlined as engine-embedded boot glue for when the corpus isn't reachable.
pub fn run_interior_boot_inline() -> Vec<SpawnRequest> {
    let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
    let sh = match ScriptHost::bare() {
        Ok(s) => s,
        Err(e) => {
            println!("[script] host init failed: {e}");
            return Vec::new();
        }
    };
    if let Err(e) = sh.register_engine(host.clone()) {
        println!("[script] register_engine failed: {e}");
        return Vec::new();
    }
    let o = PMC_INTERIOR_ACTOR_ORIGIN;
    let src = format!(
        "local uGuid = Pg.GetGuidByName(\"HqInterior\")\n\
         if not uGuid then uGuid = Pg.Spawn(\"{tpl}\", 0, 0, 0, 0, false, true) end\n\
         Object.SetName(uGuid, \"HqInterior\")\n\
         Object.SetPosition(uGuid, {x}, {y}, {z})\n\
         Object.SetYaw(uGuid, 0)\n",
        tpl = PMC_INTERIOR_TEMPLATE,
        x = o[0],
        y = o[1],
        z = o[2]
    );
    if let Err(e) = sh.exec(&src, "@interior_boot") {
        println!("[script] interior boot failed: {e}");
        return Vec::new();
    }
    let spawns = std::mem::take(&mut host.borrow_mut().spawns);
    spawns
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The audio system is wired in: real game `Sound.*` Lua drives the live `crate::audio::AudioEngine`
    /// through the `EngineHost` forwarding (not a test double). `SetDynamicMusic`/`IsDynamicMusic`
    /// round-trip deterministically; an unknown cue (no sounddb) returns nil, faithful to the exe.
    #[test]
    fn game_lua_sound_drives_real_audio_engine() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        let dyn_on: bool = sh
            .eval("Sound.SetDynamicMusic(true); return Sound.IsDynamicMusic()")
            .unwrap();
        assert!(dyn_on, "SetDynamicMusic/IsDynamicMusic must round-trip through the real AudioEngine");
        assert!(host.borrow().audio.borrow().is_dynamic_music());

        // Music FSM: registering a state then a self-transition drives the real dual-deck FSM.
        sh.exec(r#"Sound.AddMusicState("combat")"#, "@ms").unwrap();

        // CueSound with no bank loaded → nil (faithful); the forwarding is exercised regardless.
        let cue_nil: bool = sh.eval(r#"return Sound.CueSound("ui_confirm") == nil"#).unwrap();
        assert!(cue_nil, "unknown cue with no sounddb loaded returns nil");

        // Bank load/unload drives the REAL BankManager (slot table + 64-in-flight throttle): the request
        // is accepted (a slot is taken). Residency completes across frames via the streaming callback
        // (async, not driven here); the observable Lua contract is the accepted-bool.
        let loaded: bool = sh.eval(r#"return Sound.LoadSoundBank("weapons")"#).unwrap();
        assert!(loaded, "LoadSoundBank is accepted by the BankManager");
        let unloaded: bool = sh.eval(r#"return Sound.UnloadBank("weapons")"#).unwrap();
        assert!(unloaded, "UnloadBank releases the slot");
        // Category pitch drives the real mixer: SetCategoryPitch queues a change the engine tick applies
        // (length 0 ⇒ snaps in one tick).
        sh.exec(r#"Sound.SetCategoryPitch("sfx", 1.5, 0.0)"#, "@p").unwrap();
        host.borrow().audio.borrow_mut().tick(1.0 / 60.0);
        assert_eq!(host.borrow().audio.borrow().get_category_pitch("sfx"), 1.5);
    }

    /// The `Ai.*` order/faction/spawner surface is WIRED to real mechanisms (not no-ops): game Lua
    /// drives `crate::ai::AiWorld` (the ring), `crate::faction::FactionWorld` (the mood bridge), and the
    /// infraction-multiplier gate — asserted on the live host state the bindings forwarded into.
    #[test]
    fn game_lua_ai_drives_ring_and_faction() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Order verb (table form) posts to the recovered 1024-slot action ring.
        sh.exec(r#"Ai.Anchor({AIGuid = 0x1000, AnchorRadius = 0})"#, "@ai").unwrap();
        sh.exec(r#"Ai.Goal(0x1000, "Attack")"#, "@ai").unwrap();
        assert_eq!(host.borrow().ai.bus.len(), 2, "Ai order + goal both post to the ring");

        // Faction: a scripted infraction accrues into the mood accumulator...
        let faction: i64 = 777;
        sh.exec(&format!("Ai.AddInfraction(1, {faction}, 100)"), "@ai").unwrap();
        assert!(!host.borrow().faction.accumulator(faction as u32).is_empty(), "AddInfraction accrues mood");

        // ...and SetInfractionMultiplier(0) DISABLES further infractions for that faction (shipped
        // gurcon002 pattern): a second faction at multiplier 0 stays empty.
        let quiet: i64 = 888;
        sh.exec(&format!("Ai.SetInfractionMultiplier({quiet}, 0); Ai.AddInfraction(1, {quiet}, 100)"), "@ai").unwrap();
        assert!(host.borrow().faction.accumulator(quiet as u32).is_empty(), "multiplier 0 ignores infractions");

        // SetAttitude writes the directed relation the faction manager (and AI matrix) hold.
        sh.exec("Ai.SetAttitude(777, 42, -100)", "@ai").unwrap();
        assert_eq!(host.borrow().faction.get_relation(777, 42), -100);
        assert_eq!(host.borrow().ai.get_relation(777, 42), -100);
    }

    /// The `Vehicle.Hijack*`/`SetTurret*` surface is WIRED to the real `mercs2_vehicle` hijack FSM +
    /// turret aim (not no-ops): game Lua drives the lifecycle and the host state advances accordingly.
    #[test]
    fn game_lua_vehicle_hijack_and_turret() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        let veh: i64 = 0x2000;
        // Full happy-path lifecycle through Lua; each verb returns the resulting state name.
        let started: String = sh.eval(&format!("return Vehicle.HijackStart({veh})")).unwrap();
        assert_eq!(started, "started");
        let done: String = sh
            .eval(&format!("Vehicle.SetHijackSuccess({veh}); return Vehicle.HijackComplete({veh})"))
            .unwrap();
        assert_eq!(done, "complete");
        assert_eq!(host.borrow().vehicle_hijack_state(veh as u64), "complete");

        // Turret + rotor articulation lands on the host TurretAim.
        sh.exec(&format!("Vehicle.SetTurretYaw({veh}, 1.5); Vehicle.SpinHeli({veh}, true)"), "@v").unwrap();
        let aim = host.borrow().turrets.get(&(veh as u64)).copied().unwrap();
        assert_eq!(aim.yaw, 1.5);
        assert!(aim.rotor_spinning);

        // Cancel from a fresh vehicle returns to idle.
        let cancelled: String = sh
            .eval("Vehicle.HijackStart(0x3000); return Vehicle.CancelHijack(0x3000)")
            .unwrap();
        assert_eq!(cancelled, "idle");

        // Seat occupancy + weapon restore land on real host state.
        sh.exec("Vehicle.EnterBySeatGuid(0x11, 0x99)", "@v").unwrap();
        assert_eq!(host.borrow().human_seat(0x11), 0x99);
        sh.exec("Human.ForceExitSeatNoSnap(0x11)", "@v").unwrap();
        assert_eq!(host.borrow().human_seat(0x11), 0);
        sh.exec("Weapon.SetClipAmmo(0x88, 1); Vehicle.RestoreAmmo(0x88)", "@v").unwrap();
        assert_eq!(host.borrow().weapon_clip(0x88), host.borrow().weapon_max_clip(0x88));
    }

    /// The `Sys.Set*` config surface is WIRED to a real settings store: `Set*` ↔ `Get*` roundtrip
    /// through the host (not no-ops that drop the write).
    #[test]
    fn game_lua_sys_settings_roundtrip() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Tutorials toggle roundtrips through Set→Get.
        let before: bool = sh.eval("return Sys.TutorialsEnabled()").unwrap();
        assert!(before, "default tutorials enabled");
        let after: bool = sh.eval("Sys.SetTutorialsEnabled(false); return Sys.TutorialsEnabled()").unwrap();
        assert!(!after, "SetTutorialsEnabled persisted");

        // Master-script name roundtrips (was aliased to level name; now a real settable field).
        let master: String = sh
            .eval(r#"Sys.SetMasterScriptName("mrxbootstrap"); return Sys.GetMasterScriptName()"#)
            .unwrap();
        assert_eq!(master, "mrxbootstrap");

        // Time scale + viewports land on the store.
        sh.exec("Sys.SetTimeScale(0.5); Sys.SetNumberOfViewports(2)", "@s").unwrap();
        assert_eq!(host.borrow().sys_time_scale(), 0.5);
        assert_eq!(host.borrow().settings.viewports, 2);
    }

    /// `ObjectFilter.*` is WIRED to the real `mercs2_core` filter registry + object label store: the
    /// label boolean-expression predicate evaluates against real labels, and the include/exclude sets
    /// work — all driven through Lua.
    #[test]
    fn game_lua_object_filter_evaluates_real_predicate() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Label two objects, then filter for "China&&Vehicle".
        sh.exec(
            r#"
            Object.AddLabel(100, "China"); Object.AddLabel(100, "Vehicle")
            Object.AddLabel(200, "China")
            uFilter = ObjectFilter.Create()
            ObjectFilter.SetFilter(uFilter, "China&&Vehicle")
        "#,
            "@of",
        )
        .unwrap();

        // 100 (China+Vehicle) matches; 200 (China only) does not — real predicate evaluation.
        let m100: bool = sh.eval("return ObjectFilter.Eval(uFilter, 100)").unwrap();
        let m200: bool = sh.eval("return ObjectFilter.Eval(uFilter, 200)").unwrap();
        assert!(m100, "China&&Vehicle matches the labelled vehicle");
        assert!(!m200, "China-only object fails China&&Vehicle");

        // Explicit include overrides a failing predicate; GetObjects returns the include set.
        sh.exec("ObjectFilter.AddObject(uFilter, 200, true)", "@of").unwrap();
        let m200b: bool = sh.eval("return ObjectFilter.Eval(uFilter, 200)").unwrap();
        assert!(m200b, "explicit include forces a match");
        let objs: Vec<i64> = sh.eval("return ObjectFilter.GetObjects(uFilter, false)").unwrap();
        assert_eq!(objs, vec![200]);
    }

    /// `Object.Attach`/`Detach` drive a REAL attachment graph the getters read (were no-op stubs +
    /// default getters — the parent never changed).
    #[test]
    fn game_lua_object_attach_graph() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        sh.exec("Object.Attach(500, 10); Object.Attach(501, 10)", "@a").unwrap();
        let parent: i64 = sh.eval("return Object.GetParent(500)").unwrap();
        assert_eq!(parent, 10, "GetParent reads the attachment graph");
        let attached: bool = sh.eval("return Object.IsAttached(500)").unwrap();
        assert!(attached);
        let mut kids: Vec<i64> = sh.eval("return Object.GetAttachedObjects(10)").unwrap();
        kids.sort();
        assert_eq!(kids, vec![500, 501], "both children read back");

        sh.exec("Object.Detach(500)", "@a").unwrap();
        assert_eq!(host.borrow().object_parent(500), 0, "Detach clears the parent");
        assert!(!host.borrow().object_is_attached(500));
    }

    /// `VO.*` drives the real `crate::audio::VoManager`: a cue plays a line (active), Cancel stops it,
    /// SetCinematicMode toggles the real flag — all through Lua (were no-op stubs).
    #[test]
    fn game_lua_vo_drives_real_vo_manager() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Cue a line → the VoManager has an active line.
        let handle: Option<i64> = sh.eval(r#"return VO.Cue(1, "vo_intro_001")"#).unwrap();
        assert!(handle.is_some(), "VO.Cue returns a non-nil handle when the line starts");
        assert!(host.borrow().audio.borrow().vo_is_active(), "VoManager has an active line");

        // Cancel by the same cue name stops it.
        sh.exec(r#"VO.Cancel(1, "vo_intro_001")"#, "@vo").unwrap();
        assert!(!host.borrow().audio.borrow().vo_is_active(), "Cancel stopped the active VO line");

        // Cinematic mode toggles the real flag.
        sh.exec("VO.SetCinematicMode(true)", "@vo").unwrap();
        assert!(host.borrow().audio.borrow().vo_cinematic_mode());
    }

    /// `Hud.*` drives the REAL `crate::widgets::WidgetTree`: create widgets, set/get their state, parent
    /// them, and text/image data round-trips — all through Lua (was a no-op HUD).
    #[test]
    fn game_lua_hud_drives_real_widget_tree() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Create a text widget, set its text + visibility → read them back.
        sh.exec(
            r#"
            wRoot = Hud.CreateWidget()
            wText = Hud.CreateTextWidget()
            Hud.SetTextText(wText, "OBJECTIVE COMPLETE")
            Hud.SetTextScale(wText, 2.0)
            Hud.SetWidgetVisible(wText, false)
            Hud.SetWidgetLocation(wText, 100, 200)
            Hud.AddWidgetChild(wRoot, wText)
        "#,
            "@hud",
        )
        .unwrap();

        let text: String = sh.eval("return Hud.GetTextText(wText)").unwrap();
        assert_eq!(text, "OBJECTIVE COMPLETE");
        let scale: f32 = sh.eval("return Hud.GetTextScale(wText)").unwrap();
        assert_eq!(scale, 2.0);
        let vis: bool = sh.eval("return Hud.GetWidgetVisible(wText)").unwrap();
        assert!(!vis, "SetWidgetVisible(false) persisted");
        let loc: (f32, f32) = sh.eval("return Hud.GetWidgetLocation(wText)").unwrap();
        assert_eq!(loc, (100.0, 200.0));

        // The tree really parented the text under the root.
        let wtext: i64 = sh.eval("return wText").unwrap();
        let kids: Vec<i64> = sh.eval("return Hud.GetWidgetChildren(wRoot)").unwrap();
        assert_eq!(kids, vec![wtext]);
        assert_eq!(host.borrow().hud.len(), 2, "two widgets live in the tree");

        // Gui markers drive the real MarkerSet.
        sh.exec(
            r#"
            mObj = Gui.AddObjective()
            Gui._MarkerSetLocation(mObj, 300, 5, 400)
            Gui._MarkerSetFollowGuid(mObj, 0x1234)
            Gui._MarkerPulse(mObj)
        "#,
            "@mk",
        )
        .unwrap();
        let mid: u64 = sh.eval::<i64>("return mObj").unwrap() as u64;
        let mk = host.borrow();
        let marker = mk.markers.get(mid).unwrap();
        assert_eq!(marker.location, [300.0, 5.0, 400.0]);
        assert_eq!(marker.follow_guid, 0x1234);
        assert!(marker.pulsing);
    }

    /// The presentation namespaces drive the real `mercs2_core::RenderState`: the atmosphere generic
    /// value/color store + bloom/graphics/fade params round-trip through Lua (were no-op stubs).
    #[test]
    fn game_lua_render_state_roundtrip() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Atmosphere generic value store (the dominant real usage).
        let v: f32 = sh.eval(r#"Atmosphere.SetValue("fog_density", 0.35); return Atmosphere.GetValue("fog_density")"#).unwrap();
        assert_eq!(v, 0.35);
        sh.exec("Atmosphere.Begin(); Atmosphere.SetLightIntensity(2.5)", "@atm").unwrap();
        assert!(host.borrow().render.atmosphere.active);
        assert_eq!(host.borrow().render.atmosphere.value("light_intensity"), 2.5);

        // Bloom + graphics + fade land on the state.
        sh.exec("Bloom.SetThreshold(0.8); Graphics.SetGamma(1.2); Fade.CameraFade(0,0,0,1)", "@fx").unwrap();
        assert_eq!(host.borrow().render.bloom.threshold, 0.8);
        assert_eq!(host.borrow().render.graphics.gamma, 1.2);
        assert_eq!(host.borrow().render.fade.camera_fade, [0.0, 0.0, 0.0, 1.0]);
        // Graphics shadow distance Set↔Get round-trips.
        let sd: f32 = sh.eval("Graphics.SetShadowBaseDistance(250); return Graphics.GetShadowBaseDistance()").unwrap();
        assert_eq!(sd, 250.0);

        // CameraFx cinematic controller: pose Set↔Get + follow/shake land on the host.
        let yaw: f32 = sh.eval("Camera.SetYaw(1.25); return Camera.GetYaw()").unwrap();
        assert_eq!(yaw, 1.25);
        sh.exec("Camera.SetPosition(1,2,3); Camera.Follow(0x77); Camera.Shake(0.5)", "@cam").unwrap();
        assert_eq!(host.borrow().camera_fx.position, [1.0, 2.0, 3.0]);
        assert_eq!(host.borrow().camera_fx.follow_guid, 0x77);
        assert_eq!(host.borrow().camera_fx.shake, 0.5);
    }

    /// `Inventory.*` drives a REAL per-character weapon loadout: set/get/equip/drop round-trips through
    /// Lua (was empty getters + no-op mutators).
    #[test]
    fn game_lua_inventory_loadout() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        let c: i64 = 0x1000;
        sh.exec(&format!("Inventory.SetAllWeapons({c}, {{10, 20, 30}})"), "@inv").unwrap();
        let all: Vec<i64> = sh.eval(&format!("return Inventory.GetAllWeapons({c})")).unwrap();
        assert_eq!(all, vec![10, 20, 30]);
        let prim: i64 = sh.eval(&format!("return Inventory.GetPrimaryWeapon({c})")).unwrap();
        let sec: i64 = sh.eval(&format!("return Inventory.GetSecondaryWeapon({c})")).unwrap();
        assert_eq!((prim, sec), (10, 20));

        // Equip adds, Drop removes.
        sh.exec(&format!("Inventory.EquipWeapon({c}, 40); Inventory.DropWeapon({c}, 20)"), "@inv").unwrap();
        let after: Vec<i64> = sh.eval(&format!("return Inventory.GetAllWeapons({c})")).unwrap();
        assert_eq!(after, vec![10, 30, 40]);
        // A character with no loadout reads nil primary.
        let none: Option<i64> = sh.eval("return Inventory.GetPrimaryWeapon(0x9999)").unwrap();
        assert_eq!(none, None);
    }

    /// Weapon ammo, Fire burning state, and Object health/SendDamage are REAL host state driven through
    /// Lua (were no-op stubs / empty getters).
    #[test]
    fn game_lua_weapon_fire_damage() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Weapon ammo: set clip + reserve, then Reload pulls from reserve into the clip.
        let w: i64 = 0x555;
        sh.exec(&format!("Weapon.SetClipAmmo({w}, 5); Weapon.SetReserveAmmo({w}, 90)"), "@wp").unwrap();
        assert_eq!(sh.eval::<i64>(&format!("return Weapon.GetClipAmmo({w})")).unwrap(), 5);
        sh.exec(&format!("Weapon.Reload({w})"), "@wp").unwrap();
        // clip refills to max_clip (30), reserve drops by the 25 taken.
        assert_eq!(sh.eval::<i64>(&format!("return Weapon.GetClipAmmo({w})")).unwrap(), 30);
        assert_eq!(sh.eval::<i64>(&format!("return Weapon.GetReserveAmmo({w})")).unwrap(), 65);

        // Fire: Ignite sets burning, Extinguish clears it.
        sh.exec("Fire.Ignite(0x700)", "@fr").unwrap();
        assert!(host.borrow().object_is_burning(0x700));
        sh.exec("Fire.Extinguish(0x700)", "@fr").unwrap();
        assert!(!host.borrow().object_is_burning(0x700));

        // SendDamage reduces health; enough damage kills (returns true).
        let died_partial: bool = sh.eval("return ObjectState.SendDamage(0x800, 40)").unwrap();
        assert!(!died_partial);
        assert_eq!(host.borrow().object_health(0x800), 60.0);
        let died: bool = sh.eval("return ObjectState.SendDamage(0x800, 100)").unwrap();
        assert!(died, "lethal damage returns died=true");
        assert!(!host.borrow().object_is_alive(0x800));
    }

    /// `Pg` regions/alarms + `Airstrike` designators/ordnance drive real host state through Lua.
    #[test]
    fn game_lua_pg_regions_and_airstrike() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Region registry: CreateRegion mints a stable handle; re-creating the name reuses it.
        let r1: i64 = sh.eval(r#"return Pg.CreateRegion("bank_lobby", 10, 0, 20, 5)"#).unwrap();
        let r2: i64 = sh.eval(r#"return Pg.CreateRegion("bank_lobby", 11, 0, 21, 6)"#).unwrap();
        assert_eq!(r1, r2, "same-named region reuses its handle");
        assert_eq!(host.borrow().regions.get(&(r1 as u64)).copied(), Some(([11.0, 0.0, 21.0], 6.0)));

        // Alarm state: Activate then Toggle.
        sh.exec("Pg.ActivateAlarm(0x42, true)", "@al").unwrap();
        assert!(host.borrow().pg_alarm_active(0x42));
        let now: bool = sh.eval("return Pg.ToggleAlarm(0x42)").unwrap();
        assert!(!now, "toggle turns the active alarm off");

        // Airstrike designator lifecycle + FindDesignatorOwner.
        sh.exec("Airstrike.EquipDesignator(0x2)", "@as").unwrap();
        let owner: Option<i64> = sh.eval("return Airstrike.FindDesignatorOwner()").unwrap();
        assert_eq!(owner, Some(2));
        // Ordnance spawn is recorded (kind + position).
        sh.exec("Airstrike.SpawnOrdnance(100, 5, 200)", "@as").unwrap();
        assert_eq!(host.borrow().airstrikes.last().unwrap(), &("ordnance".to_string(), [100.0, 5.0, 200.0]));
    }

    /// `Human.*` weapon/ragdoll/grapple flag verbs drive the real per-human flag store through Lua.
    #[test]
    fn game_lua_human_flags() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        let g: i64 = 0x1000;
        assert!(host.borrow().human_weapons_enabled(g as u64), "weapons enabled by default");
        sh.exec(&format!("Human.DisableWeapons({g})"), "@hu").unwrap();
        assert!(!host.borrow().human_weapons_enabled(g as u64), "DisableWeapons persisted");
        sh.exec(&format!("Human.EnableWeapons({g})"), "@hu").unwrap();
        assert!(host.borrow().human_weapons_enabled(g as u64));

        sh.exec(&format!("Human.Knockdown({g})"), "@hu").unwrap();
        assert!(host.borrow().human_is_knocked_down(g as u64), "Knockdown ragdolls the human");

        // StopGrappling clears the grapple flag; IsGrappling reads the real store.
        host.borrow_mut().human_flags.entry(g as u64).or_default().grappling = true;
        let grap: bool = sh.eval(&format!("return Human.IsGrappling({g})")).unwrap();
        assert!(grap);
        sh.exec(&format!("Human.StopGrappling({g})"), "@hu").unwrap();
        assert!(!host.borrow().human_is_grappling(g as u64));
    }

    /// `Net.*` session mode drives real NetState: SP defaults to offline server; ConnectToServer/
    /// StartServer/Stop transition it, and the getters read it.
    #[test]
    fn game_lua_net_session_mode() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // SP default: server, not active, not multiplayer.
        assert!(sh.eval::<bool>("return Net.IsServer()").unwrap());
        assert!(!sh.eval::<bool>("return Net.IsActive()").unwrap());
        assert!(!sh.eval::<bool>("return Net.IsClient()").unwrap());

        // ConnectToServer → client + active + host name.
        sh.exec(r#"Net.ConnectToServer("dedicated-01")"#, "@net").unwrap();
        assert!(sh.eval::<bool>("return Net.IsClient()").unwrap());
        assert!(!sh.eval::<bool>("return Net.IsServer()").unwrap());
        assert!(sh.eval::<bool>("return Net.IsActive()").unwrap());
        assert_eq!(sh.eval::<String>("return Net.GetHostName()").unwrap(), "dedicated-01");

        // Stop → back to the offline SP server.
        sh.exec("Net.Stop()", "@net").unwrap();
        assert!(sh.eval::<bool>("return Net.IsServer()").unwrap());
        assert!(!sh.eval::<bool>("return Net.IsActive()").unwrap());
    }

    /// ObjectState emitters/state, Face expression, and Report lifecycle drive real host state.
    #[test]
    fn game_lua_objectstate_face_report() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // Emitters + state-machine state.
        sh.exec(r#"ObjectState.StartEmitter(0x10, "smoke"); ObjectState.SetState(0x10, "Damaged")"#, "@os").unwrap();
        assert!(host.borrow().object_emitter_active(0x10, "smoke"));
        assert_eq!(host.borrow().object_sm_state(0x10), "Damaged");
        sh.exec(r#"ObjectState.StopEmitter(0x10, "smoke")"#, "@os").unwrap();
        assert!(!host.borrow().object_emitter_active(0x10, "smoke"));

        // Face: bound set + current expression.
        sh.exec(r#"Face.BindFaceAnimSet(0x20, "mattias_faces"); Face.PlayFacialExpression(0x20, "angry")"#, "@fa").unwrap();
        assert_eq!(host.borrow().face_current(0x20), "angry");

        // Report lifecycle finalizes the faction mood report (no infractions → 0).
        sh.exec("Report.Init({ SimultaneousReporters = 1 }); Report.SetDelay(2.0)", "@rp").unwrap();
        let inf: i64 = sh.eval("return Report.GetInfractions()").unwrap();
        assert_eq!(inf, 0);
        sh.exec("Report.Completed()", "@rp").unwrap();
    }

    /// `Player.Set*` mode gates drive the real player-mode store the engine reads.
    #[test]
    fn game_lua_player_modes() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        sh.exec("Player.SetInputEnabled(false); Player.SetCinematicMode(true); Player.SetHealthClamp(0.25)", "@pl").unwrap();
        // engine reads via player_mode(key, default)
        assert!(!host.borrow().player_mode("input_enabled", true), "input disabled");
        assert!(host.borrow().player_mode("cinematic_mode", false), "cinematic on");
        // unset gate returns the caller's default
        assert!(host.borrow().player_mode("scope_enabled", false) == false);
        assert_eq!(host.borrow().player_scalars.get("health_clamp").copied(), Some(0.25));

        sh.exec("Player.SetGrappleEnabled(true); Player.SetAimMode(2)", "@pl").unwrap();
        assert!(host.borrow().player_mode("grapple_enabled", false));
        assert_eq!(host.borrow().player_scalars.get("aim_mode").copied(), Some(2.0));
    }

    /// The recorded-command bindings (record_all / sound_cmd / net_event) capture the game's calls as
    /// real intents AND emit `[bind]` app-log lines — the ground-truth that the surface is live.
    #[test]
    fn game_lua_recorded_bindings_capture_and_log() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // A generic script_cmd (Ai spawner control), a net_event, and a sound_cmd.
        sh.exec("Ai.SetRoadSpawning(true)", "@r").unwrap();
        sh.exec(r#"Net.SendEvent_Fanfare("victory", 3)"#, "@r").unwrap();
        sh.exec(r#"Sound.AddFactionMusic(42, "china_theme")"#, "@r").unwrap();

        let h = host.borrow();
        assert!(h.script_cmds.iter().any(|(v, _)| v == "Ai.SetRoadSpawning"), "Ai.SetRoadSpawning recorded");
        assert!(h.net_events.iter().any(|(v, a)| v == "SendEvent_Fanfare" && a == &["victory", "3"]), "net event recorded with args");
        assert!(h.sound_cmds.iter().any(|(v, a)| v == "AddFactionMusic" && a == &["42", "china_theme"]), "sound cmd recorded with args");
    }

    /// The REAL vanilla boot Lua flow runs against the on-disk corpus and executes deep (the module
    /// `Init()` two-phase convention, `getfenv`/`setfenv`, the `debug` lib, `_GuiInternal`, and the
    /// numeric `_GetLibVersion` all have to work). Asserts the game's Lua emitted a substantial number
    /// of `[lua]` `Debug.Printf` lines — the ground-truth that it's running against the engine. Skipped
    /// (not failed) if the decompiled corpus isn't present (CI without `docs/mercs2-luacd`).
    #[test]
    fn boot_flow_runs_real_game_lua() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let Some(sh) = resident_script_host(host.clone()) else {
            eprintln!("[skip] decompiled Lua corpus not present — boot-flow regression skipped");
            return;
        };
        host.borrow_mut().set_boot_context("chris");
        run_boot_flow(&sh, &host, "PmcCon001", "chris");
        let (lines, complete, layers) = {
            let h = host.borrow();
            (h.lua_log_lines, h.world_load_complete, h.world_layers_loaded)
        };
        assert!(
            lines > 100,
            "expected the game's Lua to run deep (>100 [lua] lines); got {lines} — a boot regression"
        );
        // The real streaming milestone: every world-layer request the master boot issued was fulfilled
        // (MrxLayerManager drained its op queue). If this fails the load never streamed the world in
        // (e.g. Pg.AssetExists culling layers), so GlobalExit below would be meaningless.
        assert!(
            layers,
            "expected every streaming layer request to be fulfilled ('All layer operations processed and \
             fulfilled'); it was not — a regression in the layer-streaming completion (Pg.AssetExists / \
             Pg.LoadLayer / __flush_layer_loads / MrxLayerManager op queue)"
        );
        // loadprobe phase 20: the world-load state machine ran the full master path — GlobalEnter, act
        // staging, mission-flow init, WaitForStreaming, and the WifMissionFlow.Refresh → Exit(WAITFORGAME)
        // that reaches GlobalExit ("world fully loaded").
        assert!(
            complete,
            "expected the world-load state machine to reach GlobalExit - Complete (loadprobe phase 20, \
             'world fully loaded'); it did not — a regression in the GameStateChange bridge / GlobalEnter \
             gates / act staging (StagingAct1) / mission flow (_StartPlayerVisibleGameplay → Refresh) / \
             event-kind constants (Event.WeaponEvent et al.) / fade path / pump loop"
        );
    }

    /// `Pg.LoadLayer` registers its status-change callback and the pump's `Pg.__flush_layer_loads`
    /// fires it with success — the engine's async layer-streaming completion that lets MrxLayerManager
    /// fulfil the request and signal gameplay setup (`_AttemptGameplaySetup{"static"}`).
    #[test]
    fn pg_loadlayer_fires_completion_callback() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();

        // LoadLayer returns true (accepted) and defers the callback; nothing fires until the flush.
        let accepted: bool = sh.eval(r#"
            _fired = nil
            local ok = Pg.LoadLayer("testlayer", true, function(req, name, typ, success)
                _fired = { req, name, success }
            end, {})
            return ok
        "#).unwrap();
        assert!(accepted, "LoadLayer accepted");
        assert!(sh.eval::<bool>("return _fired == nil").unwrap(), "callback deferred, not synchronous");

        // The pump flush fires it with (Load, layer, ..., success=true).
        pump_resident(&sh, 0.1);
        let (req, name, ok): (String, String, bool) = sh
            .eval("return _fired[1], _fired[2], _fired[3]")
            .unwrap();
        assert_eq!((req.as_str(), name.as_str(), ok), ("Load", "testlayer", true));
    }

    /// The resident host (K1) stays alive across frames: a runtime `Pg.Spawn` is recorded and drained
    /// via `take_new_spawns` (the loop then realizes it), and `pump_resident` advances the Lua event
    /// system without error. This is the socket the persistent mission-Lua plugs into.
    #[test]
    fn resident_host_pumps_and_drains_runtime_spawns() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = resident_script_host(host.clone()).expect("resident host starts");

        // A runtime spawn (as a mission/population script would issue) is recorded on the live host.
        sh.exec(r#"Pg.Spawn("civilian_sedan", 10, 0, 20, 0, false, true)"#, "@t").unwrap();
        let drained = host.borrow_mut().take_new_spawns();
        assert_eq!(drained.len(), 1, "resident host records a runtime Pg.Spawn for the loop to realize");
        assert_eq!(drained[0].template, "civilian_sedan");
        // Draining clears it — the next frame starts empty.
        assert!(host.borrow_mut().take_new_spawns().is_empty());

        // The per-frame pump runs the Lua event/timer system without error.
        pump_resident(&sh, 1.0 / 60.0);
    }

    /// The base-game hero teleport is `Object.SetPosition(Player.GetLocalCharacter(), x, y, z)`
    /// (mrxutil.lua:328). Running that through the live host registers the hero spawn the boot consumes
    /// — Lua-authored, no engine constant. This is the "wire the Lua parts together" mechanism.
    #[test]
    fn lua_teleport_via_object_setposition_drives_hero_spawn() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = resident_script_host(host.clone()).expect("resident host");
        // Exactly what MrxUtil._TeleportHero does: move the local character to a world position.
        sh.exec(
            "Object.SetPosition(Player.GetLocalCharacter(), 3794.0, 451.0, -3911.0, false)",
            "@teleport",
        )
        .unwrap();
        let pos = host.borrow_mut().take_hero_teleport().expect("hero teleport recorded");
        assert_eq!(pos, [3794.0, 451.0, -3911.0]);
        // Drained — a second read is None (the boot consumes it once).
        assert!(host.borrow_mut().take_hero_teleport().is_none());
    }

    /// The full base-game spawn chain, host-side: `Pg.GetGuidByName(marker)` → `Object.GetPosition(guid)`
    /// → `Pg.Spawn(hero, x,y,z)` — exactly what `MrxPlayer.CreatePlayerCharacter` runs. The marker is a
    /// LIVE entity in the World + guidmap (the loader entity-izes named markers the same way), so the name
    /// resolves to a real entity and the position comes from its `Transform`. No shadow table, no const.
    #[test]
    fn boot_spawn_chain_resolves_marker_to_hero_spawn() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        // Attach a live World + guidmap and register the spawn-location marker as a real entity.
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids.clone());
        {
            let e = world
                .borrow_mut()
                .spawn((Transform::from_translation(mercs2_core::glam::Vec3::new(10.0, 20.0, 30.0)),));
            host.borrow().register_named_entity(e, pandemic_hash_m2("pmccon001_start1"));
        }
        host.borrow_mut().set_boot_context("chris");
        let sh = resident_script_host(host.clone()).expect("resident host");

        // The CreatePlayerCharacter chain (name → guid → live position → Pg.Spawn(hero)).
        sh.exec(
            "local g = Pg.GetGuidByName('PmcCon001_Start1')\n\
             local x, y, z = Object.GetPosition(g)\n\
             Pg.Spawn('chris', x, y, z, 0, false, false, false)",
            "@spawn_chain",
        )
        .unwrap();
        assert_eq!(
            host.borrow_mut().take_hero_spawn(),
            Some([10.0, 20.0, 30.0]),
            "the hero must spawn at the marker the name resolved to — from the LIVE guidmap, no const"
        );
    }

    /// The core proof that this is real, not a shadow: `Object.GetPosition` reads the entity's LIVE
    /// `Transform`, so moving the entity in the World (as physics/animation would) changes what the Lua
    /// binding returns — something the old `named_locations`/`spawns[]` side tables could never do.
    #[test]
    fn object_get_position_reflects_a_live_world_move() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids.clone());
        // A named entity at the origin.
        let e = world.borrow_mut().spawn((Transform::IDENTITY,));
        let guid = host.borrow().register_named_entity(e, pandemic_hash_m2("test_marker"));
        assert_eq!(host.borrow_mut().object_get_position(guid), [0.0, 0.0, 0.0]);

        // Move it in the World (the loop's physics/anim would do this) — the binding reports the new pos.
        world.borrow().get::<&mut Transform>(e).unwrap().translation = mercs2_core::glam::Vec3::new(5.0, 6.0, 7.0);
        assert_eq!(host.borrow_mut().object_get_position(guid), [5.0, 6.0, 7.0]);

        // And name resolution + the write path round-trip through the same live entity.
        assert_eq!(host.borrow_mut().guid_by_name("Test_Marker"), guid);
        host.borrow_mut().object_set_position(guid, [1.0, 2.0, 3.0]);
        assert_eq!(host.borrow_mut().object_get_position(guid), [1.0, 2.0, 3.0]);
    }

    /// Lua `Object.*Health` and the combat silo read/write the SAME `Health` component on a live entity —
    /// no divergence. The old shadow HashMap and the combat `Health` were disjoint; now Lua damage is
    /// visible to combat and vice-versa.
    #[test]
    fn health_binding_shares_the_combat_health_component() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids.clone());
        // A combat entity carrying a Health component (as the resolver / streaming would spawn it).
        let e = world.borrow_mut().spawn((Transform::IDENTITY, crate::combat::Health::new(100.0)));
        let g = 0x1000_5000u64;
        host.borrow().register_entity(e, g, None);

        assert_eq!(host.borrow().object_health(g), 100.0);
        // Lua damage writes the SAME component the combat silo reads.
        assert!(!host.borrow_mut().object_send_damage(g, 30.0));
        assert_eq!(host.borrow().object_health(g), 70.0);
        assert_eq!(world.borrow().get::<&crate::combat::Health>(e).unwrap().cur, 70.0, "combat sees the Lua damage");
        // Kill via Lua → combat sees dead.
        host.borrow_mut().object_kill(g);
        assert!(!host.borrow().object_is_alive(g));
        assert!(world.borrow().get::<&crate::combat::Health>(e).unwrap().is_dead());
    }

    /// `Player.GetCash`/`GetFuel` now read a real store (were trait-default 0): seed + round-trip + the
    /// documented 1-billion cash soft-cap + fuel clamped to capacity.
    #[test]
    fn player_economy_round_trips_and_caps() {
        let mut h = GameScriptHost::new("vz");
        assert_eq!(h.player_cash(), 0);
        h.set_cash(50_000);
        assert_eq!(h.player_cash(), 50_000);
        h.player_set_cash(2_000_000_000); // over the 1B soft cap
        assert_eq!(h.player_cash(), 1_000_000_000);

        h.player_set_fuel(500); // capacity unset → unbounded
        assert_eq!(h.player_fuel(), 500);
        h.player_set_fuel_capacity(100); // clamps current fuel down
        assert_eq!(h.player_fuel(), 100);
        h.player_set_fuel(150); // clamp to capacity
        assert_eq!(h.player_fuel(), 100);
    }

    #[test]
    fn interior_boot_records_the_hqinterior_spawn() {
        let intents = run_interior_boot();
        assert_eq!(intents.len(), 1, "one SpawnActor for the PMC interior");
        let r = &intents[0];
        assert_eq!(r.template, PMC_INTERIOR_TEMPLATE);
        assert_eq!(r.name, "HqInterior");
        assert_eq!(r.pos, PMC_INTERIOR_ACTOR_ORIGIN);
        assert_ne!(r.guid, 0);
    }
}
