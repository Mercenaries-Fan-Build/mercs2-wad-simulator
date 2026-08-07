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

/// The save payload the master script's `LoadSingleton(tSaveData)` reads on boot — the subset of a
/// retail `.profile` that the boot branch actually consumes.
///
/// **This type is what decides where a boot starts.** `vz/xQ!L.lua:634-686` branches on whether it got a
/// save table at all:
///
/// * `nil` (**new game**) → `sMissionId = "VzaCon001"` →
///   `WifMissionFlow.GetMissionStartLocations("VzaCon001")` → `{"VzaCon001_Start1"}` (the opening
///   contract's own `tStartLocations`, `vz/wifmissiondata.lua:763`).
/// * a table **with `tRetryLocations`** (**resuming mid-contract**) → the contract's checkpoint
///   marker(s), `_bLoadIntoWorld` — the hero resumes in the world at the checkpoint. `_bPmcRequired`
///   stays false, so this does NOT enter the PMC (correct for a pre-PMC / in-progress save).
/// * a table **without `tRetryLocations`** (**resuming at the hub**) → `{"Pmc_Entry1", "Pmc_Entry2"}`
///   with `_bPmcRequired = true` — the PMC HQ entrance, then teleported inside. This is the reason a
///   between-contracts save resumes at the HQ.
///
/// Getting these branches wrong is what made a resume misfire: collapsing them made "New Game" open
/// inside the PMC, and dropping [`retry_locations`](Self::retry_locations) made every mid-contract
/// resume fall through to `Pmc_Entry1` (the sea-level HQ marker) and land in the water. Nothing here is
/// a coordinate: the flow yields marker NAMES, which `CreatePlayerCharacter` resolves through
/// `Pg.GetGuidByName` against live world entities.
///
/// Field names mirror the retail Lua keys (`MrxMissionFlow.SaveSingleton`, `mrxmissionflow.lua:597`),
/// so the mapping from [`mercs2_formats::save::SaveState`] is one-to-one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BootSaveState {
    /// `tFlowData.tMyFlowData` — the awarded flow keys (mission id → value). `HasKey`/`GetKeyValue`
    /// read this, and every `fPrereq` in the flow graph is a `HasKey` test, so this table is what
    /// decides which missions are unlocked.
    pub flow_keys: Vec<(String, f64)>,
    /// `tFlowData.tCulledBindings` — bindings that already fired. `LoadSingleton` culls them from the
    /// graph so a resumed save does not replay the intro cinematic and re-unlock `VzaCon001`.
    pub culled_bindings: Vec<String>,
    /// `tFlowData.tActiveMissions` — in-progress mission ids. `LoadSingleton` calls
    /// `UnlockMission(id, tMissionSaveData, false)` for each.
    pub active_missions: Vec<String>,
    /// `tSaveData.tRetryLocations` — the active contract's retry-checkpoint marker(s). Present ONLY for a
    /// save taken mid-contract; a between-contracts (PMC hub) save omits it. **This is the field the
    /// master script's `LoadSingleton` (`vz/xQ!L.lua:645-652`) branches on to decide a resume's spawn:**
    ///
    /// * **non-empty** (mid-mission) → `_bLoadIntoWorld`, hero spawns at the checkpoint marker in the
    ///   world — it does NOT take the PMC path, and `_bPmcRequired` stays false.
    /// * **empty** (at the hub) → `{"Pmc_Entry1","Pmc_Entry2"}` + `_bPmcRequired = true` — the PMC HQ
    ///   entrance, then teleported inside via `WifPmcInterior.Enter`/`MrxHq._TeleportHero`.
    ///
    /// Leaving this empty for a genuinely mid-mission (pre-PMC) save is what made a VzaCon001 resume land
    /// at the sea-level `Pmc_Entry1` marker (Y=0, in the water) instead of at its checkpoint.
    pub retry_locations: Vec<String>,
    /// `tLayerData` — the active `vz_state_*` world-overlay layer names.
    pub layers: Vec<String>,
    /// `tTransitData.bEnabled` — the transit (fast-travel) system's master switch.
    pub transit_enabled: bool,
    /// `tTransitData[n]` — per-landing-zone transit state, sorted by zone.
    ///
    /// This used to go in as an EMPTY table, so a resumed game came back with every zone at its
    /// `MrxTransit.Reset` default: no faction, disabled, fanfare unplayed. The shape is measured from
    /// the vendored retail saves and cross-checked against a live capture — see
    /// `mercs2_formats::save`'s `transit_data_decodes_from_the_retail_saves`.
    pub transit_zones: Vec<mercs2_formats::save::TransitZone>,
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
    /// The player concern (`crates/mercs2_player`): the ≤2-slot roster, the one profile/economy
    /// singleton, the global disguise gate, the boundary sets and the callback registry.
    ///
    /// Replaces the former inline `cash`/`fuel`/`fuel_capacity` scalars, the `player_character` map and
    /// the untyped `player_modes`/`player_scalars` HashMaps. The script host owns it (rather than
    /// `GameplaySystems`) because Lua is its primary driver; the tick reaches it by reference.
    player: mercs2_player::PlayerWorld,
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
    /// The retained-mode HUD widget tree the `_GuiInternal.*` Lua drives (`crate::widgets::WidgetTree`).
    hud: crate::widgets::WidgetTree,
    /// The HUD world-marker set the `Gui._Marker*` Lua drives.
    markers: crate::widgets::MarkerSet,
    /// Global render/post-FX parameter state the `Atmosphere`/`Bloom`/`Graphics`/`Fade` Lua drives.
    render: mercs2_core::RenderSettings,
    /// Cinematic camera controller state the `CameraFx.*` Lua drives.
    camera_fx: CameraFxState,
    /// Per-weapon ammo state (`Weapon.*`).
    weapons: HashMap<u64, WeaponState>,
    /// Objects currently on fire (`Graphics.FuelTrail.Ignite`/`Extinguish`).
    burning: std::collections::HashSet<u64>,
    /// Per-object health `(current, max)` (`Object.*Health`, `SendDamage`, `Kill`/`Revive`).
    health: HashMap<u64, (f32, f32)>,
    /// `Junk.CreateRegion` trigger regions: handle → `(center, radius)`; `region_names` maps name→handle.
    regions: HashMap<u64, ([f32; 3], f32)>,
    region_names: HashMap<String, u64>,
    next_region: u64,
    /// Active alarms (`Junk.ActivateAlarm`/`ToggleAlarm`).
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
    /// The world's transit landing pads as `(zone, slot, guid)`, sorted by `(zone, slot)` — the index
    /// `Pg.GetAllLandingZones(nSlot)` enumerates. Populated from the world data's `LandingZone` COMP by
    /// [`register_landing_zones`]; EMPTY on a host with no world loaded, which is the faithful answer
    /// (a world with no landing zones has none to return — see the `MrxTransit` note there).
    landing_zones: Vec<(u32, u32, u64)>,
    /// Layer name → the objects that layer brings in, so a completed `Pg.LoadLayer` can wake them.
    /// Set from world data by [`crate::worldutil::layer_index`]; empty on a worldless host, which
    /// correctly wakes nothing.
    layer_index: crate::worldutil::LayerIndex,
    /// The save the boot is resuming, in the shape `xQ!L.LoadSingleton(tSaveData)` reads — `None` for a
    /// NEW GAME. This is the single input that picks the master script's boot branch, so it is also what
    /// decides the hero's start position. See [`BootSaveState`] and [`run_boot_flow`].
    boot_save_state: Option<BootSaveState>,
    /// Guids whose `ObjectHibernation` "awake" fires on the NEXT pump — see
    /// [`GameScriptHost::queue_layer_wakes`] for why it is deferred by a tick rather than immediate.
    pending_wakes: Vec<u64>,
    /// Who is sitting where: rider guid → `(vehicle guid, seat code)`.
    ///
    /// Keyed by the RIDER, matching retail: `Object.InSeat` (`0x005CD9F0`) probes `RiderLink+0x50`, so
    /// the occupancy edge lives on the character, not the vehicle (`object_entity_core_code_map.md`
    /// §"InSeat"). That also makes "a character is in at most one seat" structural.
    seats: std::collections::HashMap<u64, (u64, String)>,
    /// Seat transitions whose `ObjectInSeat` fires on the next pump: `(occupant, vehicle, seat, action)`.
    /// Deferred for the same reason as [`pending_wakes`](Self::pending_wakes) — `Vehicle.Enter` is
    /// called FROM Lua, so firing inline would reenter the host mid-borrow.
    pending_seat_events: Vec<(u64, u64, String, &'static str)>,
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

/// Map the boot hero name to the EXACT object label `MrxUtil.GetCharacterIdentity(uChar)` tests —
/// the lowercase ids `"mattias"` / `"jennifer"` / `"chris"` it loops over (`mrxutil.lua:649`,
/// decompiled retail). The reimpl's hero name for Jennifer is the short `jen`
/// (`mrxplayer.lua` `_tCharacterMap.base`), so it folds to the `jennifer` label the engine expects.
/// Returns `None` for an unknown/empty hero (no label stamped, matching the pre-boot state).
fn hero_identity_label(hero: &str) -> Option<&'static str> {
    match hero.to_ascii_lowercase().as_str() {
        "mattias" => Some("mattias"),
        "chris" => Some("chris"),
        "jen" | "jennifer" => Some("jennifer"),
        _ => None,
    }
}

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
            // The single-player boot roster: one joined local player in slot 0, already possessing the
            // hero. Possession at construction (rather than waiting for an `AttachToCharacter`) is what
            // makes `Player.GetLocalCharacter`/`GetPrimaryCharacter` resolve during boot — the game
            // queries them constantly before any script attaches anything.
            player: {
                let mut w = mercs2_player::PlayerWorld::single_player();
                mercs2_player::possession::attach_to_character(
                    &mut w.roster,
                    0,
                    HERO_GUID,
                    mercs2_player::CheatFlags::default(),
                );
                w
            },
            human_states: HashMap::new(),
            hijacks: HashMap::new(),
            turrets: HashMap::new(),
            settings: SysSettings::default(),
            object_labels: HashMap::new(),
            object_filters: mercs2_core::ObjectFilterRegistry::new(),
            attachments: HashMap::new(),
            hud: crate::widgets::WidgetTree::new(),
            markers: crate::widgets::MarkerSet::new(),
            render: mercs2_core::RenderSettings::new(),
            camera_fx: CameraFxState::default(),
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
            human_seats: HashMap::new(),
            lua_log_lines: 0,
            world_load_complete: false,
            world_layers_loaded: false,
            sound_cmds: Vec::new(),
            net_events: Vec::new(),
            script_cmds: Vec::new(),
            pending_game_states: Vec::new(),
            landing_zones: Vec::new(), // filled from world data by `register_landing_zones`
            layer_index: Default::default(), // filled from world data by `set_layer_index`
            // No save until one is picked: a bare host boots the NEW-GAME branch, like the retail
            // engine with no profile to restore.
            boot_save_state: None,
            pending_wakes: Vec::new(),
            seats: std::collections::HashMap::new(),
            pending_seat_events: Vec::new(),
        }
    }

    /// Seed the economy from the loaded save (the stockpile's cash pile). Fuel/capacity are set by the
    /// game's Lua during init (support-data/player setup), so they start at 0 and round-trip from there.
    ///
    /// Seeding deliberately goes through the faithful setter, which means it does **not** arm the
    /// autosave — `SetCash` is one of the five profile setters that never OR the dirty flag.
    pub fn set_cash(&mut self, cash: i64) {
        self.player.profile.set_cash(cash.clamp(i32::MIN as i64, i32::MAX as i64) as i32, false);
    }

    /// The player concern this host owns, for the tick and the engine-side consumers (the camera
    /// teleport queue, the seat/ride control-source seam).
    pub fn player(&self) -> &mercs2_player::PlayerWorld {
        &self.player
    }

    /// Mutable [`player`](Self::player).
    pub fn player_mut(&mut self) -> &mut mercs2_player::PlayerWorld {
        &mut self.player
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
        self.guids.as_ref()?.borrow().entity_by_guid(self.resolve_guid(guid))
    }

    /// The reverse of [`entity_of`](Self::entity_of): the GUID an entity is registered under.
    ///
    /// Needed wherever a subsystem answers in entities but Lua speaks GUIDs — `Human.Inventory`'s
    /// getters, most of all, since the values they return get passed straight back into `Object.*`.
    fn guid_of(&self, e: Entity) -> Option<u64> {
        self.guids.as_ref()?.borrow().guid_of(e)
    }

    /// Resolve `Player.GetAnyCharacter`'s sentinel to a concrete character.
    ///
    /// `GetAnyCharacter` (`FUN_005DE260`) performs no lookup — it pushes the constant lightuserdata
    /// `0xF0000000` meaning "whichever character", and the **downstream** `Object.*` / `Human.*` calls
    /// are what resolve it. With 223 call sites it is the most-used `Player` binding in the game, so
    /// every guid-keyed path on this host has to go through here or those sites silently address a
    /// non-existent entity.
    ///
    /// Anything that is not the sentinel passes through untouched.
    fn resolve_guid(&self, guid: u64) -> u64 {
        if guid == mercs2_player::ANY_CHARACTER_SENTINEL {
            let c = self.player.resolve_any_character();
            if c != 0 {
                return c;
            }
        }
        guid
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

    /// A copy of `guid`'s live `Health` component (the SAME component the combat system reads/writes), if
    /// the entity carries one — so Lua `Object.*Health` and combat damage never diverge for live actors.
    fn health_of(&self, guid: u64) -> Option<mercs2_core::Health> {
        let e = self.entity_of(guid)?;
        let world = self.world.as_ref()?.borrow();
        world.get::<&mercs2_core::Health>(e).ok().map(|h| *h)
    }

    /// Read-or-init (`max = default_max`) and mutate `guid`'s live `Health`, writing it back. Returns
    /// whether it was applied (false if no live entity → the caller keeps the shadow fallback).
    fn with_health(&self, guid: u64, default_max: f32, f: impl FnOnce(&mut mercs2_core::Health)) -> bool {
        let Some(e) = self.entity_of(guid) else { return false };
        let Some(world_rc) = self.world.as_ref() else { return false };
        let mut world = world_rc.borrow_mut();
        let mut h = world
            .get::<&mercs2_core::Health>(e)
            .map(|h| *h)
            .unwrap_or_else(|_| mercs2_core::Health::new(default_max));
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

    /// `Pg.GetAllLandingZones(nSlot)`: the transit landing pads serving co-op player slot `nSlot`, as
    /// `(zone, guid)` pairs in ascending zone order.
    ///
    /// The caller must key the Lua table it builds by `zone`, NOT by position. `MrxTransit.Reset`
    /// (`corpus/mercs2-luacd/src/resident/mrxtransit.lua:334-342`) writes `_tLandingZones[nIndex]`
    /// straight from the iteration key of this list, and both `wifhqdata`'s `nLandingZone` fields and
    /// mission scripts (`MrxTransit.SetLocationIsNuked(30, …)`) address zones by absolute number. Retail
    /// vz's set is sparse: `1..8, 12, 15..18, 20..25, 27..30`.
    ///
    /// Empty when no world is loaded — the honest answer for a world with no pads, and the reason the
    /// shipped `MrxTransit.SaveSingleton` bug (`:367` iterates `_tLandingZones` with none of the
    /// `if not _tLandingZones` guards its siblings at `:138`/`:151` carry) can fire here at all.
    pub fn landing_zones(&self, slot: u32) -> Vec<(u32, u64)> {
        self.landing_zones
            .iter()
            .filter(|(_, s, _)| *s == slot)
            .map(|(zone, _, guid)| (*zone, *guid))
            .collect()
    }

    /// Record the world's landing-pad index (`(zone, slot, guid)`); see [`register_landing_zones`],
    /// which is what the world loader calls.
    pub fn set_landing_zones(&mut self, mut pads: Vec<(u32, u32, u64)>) {
        pads.sort_unstable();
        self.landing_zones = pads;
    }

    /// Install the layer → objects index (see [`crate::worldutil::layer_index`]). Call once at world
    /// load, after the named markers are registered so their guids exist.
    pub fn set_layer_index(&mut self, index: crate::worldutil::LayerIndex) {
        self.layer_index = index;
    }

    /// Hand the boot the save it is resuming, or `None` for a **new game**. Call before
    /// [`run_boot_flow`] — this is the input the master script branches on, so it decides both the
    /// unlocked-mission set and the hero's start marker. See [`BootSaveState`].
    pub fn set_boot_save_state(&mut self, save: Option<BootSaveState>) {
        self.boot_save_state = save;
    }

    /// The save this boot is resuming (`None` = new game). Read by [`run_boot_flow`] to answer
    /// `Pg.LoadGame`.
    pub fn boot_save_state(&self) -> Option<&BootSaveState> {
        self.boot_save_state.as_ref()
    }

    /// Translate a completed layer load into the guids that must wake, and queue them.
    ///
    /// # Why this is deferred a tick rather than fired here
    ///
    /// `Pg.__flush_layer_loads` fires each layer's `_LayerStatusChange` callback, and that cascade is
    /// what *arms* the awake-gates: `MrxLayerManager` marks the request complete, which runs the
    /// mission's setup — `VzaCon001.StandardSetup` (`vz/vzacon001.lua:78`) then calls
    /// `Event.Create(Event.ObjectHibernation, {uBoat, "a"}, …)`. Waking inside the flush would fire the
    /// event *before* anything had registered for it, and a one-shot event with no listener is simply
    /// lost — the gate would then wait forever, which is the exact failure this is fixing.
    ///
    /// Deferring also happens to be the faithful shape: retail streams objects in over subsequent
    /// frames, so an object does not wake on the same frame its layer is requested.
    ///
    /// Unknown layers contribute nothing. A name with no registered entity contributes nothing either
    /// — `guid_by_name` returns 0 and firing on guid 0 would match any handler that failed to resolve
    /// its own subject, which is worse than not firing.
    pub fn queue_layer_wakes(&mut self, layer: &str) -> usize {
        let names: Vec<String> = self.layer_index.objects_in(layer).to_vec();
        let mut n = 0;
        for name in names {
            let g = self.guid_by_name(&name);
            if g != 0 && !self.pending_wakes.contains(&g) {
                self.pending_wakes.push(g);
                n += 1;
            }
        }
        n
    }

    /// Drain the guids queued by [`queue_layer_wakes`](Self::queue_layer_wakes).
    pub fn take_pending_wakes(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.pending_wakes)
    }

    /// Drain the seat transitions queued by `Vehicle.Enter`/`Vehicle.Exit`:
    /// `(occupant, vehicle, seat, action)` with `action` ∈ `{"e", "x"}`.
    pub fn take_pending_seat_events(&mut self) -> Vec<(u64, u64, String, &'static str)> {
        std::mem::take(&mut self.pending_seat_events)
    }

    /// Who is in `vehicle`, as `(rider, seat)` pairs. Read-only view for engine systems that need the
    /// vehicle→riders direction; the stored edge is rider→vehicle (see
    /// [`seats`](Self::seats)), so this is a scan, which is fine for the handful of seats a vehicle has.
    pub fn riders_of(&self, vehicle: u64) -> Vec<(u64, String)> {
        let mut v: Vec<(u64, String)> = self
            .seats
            .iter()
            .filter(|(_, (veh, _))| *veh == vehicle)
            .map(|(rider, (_, seat))| (*rider, seat.clone()))
            .collect();
        v.sort_unstable(); // deterministic order for callers and tests
        v
    }

    /// Register a named marker/entity, minting a fresh guid; returns it (0 if no guidmap attached).
    pub fn register_named_entity(&self, e: Entity, name_hash: u32) -> u64 {
        match &self.guids {
            Some(g) => g.borrow_mut().register_named(e, name_hash),
            None => 0,
        }
    }

    /// Register an entity that carries NO `Name` COMP, minting a fresh guid; returns it (0 without a
    /// guidmap). Addressable by guid only — nothing can resolve it by name, which is exactly the state
    /// of the one retail landing pad that ships without a `Name` record.
    pub fn register_anonymous_entity(&self, e: Entity) -> u64 {
        match &self.guids {
            Some(g) => {
                let mut gm = g.borrow_mut();
                let guid = gm.mint();
                gm.register(e, None, guid);
                guid
            }
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
        self.by_guid.get(&self.resolve_guid(guid)).and_then(|&i| self.spawns.get(i)).map(|r| r.template.as_str())
    }

    fn name_of(&self, guid: u64) -> Option<&str> {
        self.by_guid.get(&self.resolve_guid(guid)).and_then(|&i| self.spawns.get(i)).map(|r| r.name.as_str())
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
        // (mrxutil.lua:649) throughout the mission/faction/HUD code. Stamp that exact label on the
        // pre-boot hero object (`HERO_GUID`, the construction-time possession) so identity resolves during
        // the window BEFORE `CreatePlayerCharacter` re-possesses onto the spawned hero. The spawned hero
        // itself is tagged in `pg_spawn` (that guid is what `GetCharacterIdentity` actually reads).
        if let Some(label) = hero_identity_label(&self.hero_character) {
            self.object_add_label(HERO_GUID, label);
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
    /// Forwards to the inherent [`GameScriptHost::landing_zones`] — needed because the binding reaches
    /// this host through `dyn EngineHost`, which cannot see inherent methods.
    fn landing_zones(&self, slot: u32) -> Vec<(u32, u64)> {
        GameScriptHost::landing_zones(self, slot)
    }
    /// `Vehicle.Enter(veh, rider, seat)` — seat the rider and queue the `ObjectInSeat` event.
    ///
    /// Retail runs an animated mount (`FUN_00540690`, `vehicle_code_map.md` §1) that takes real time;
    /// this is the state change without the animation. The *event* is what gameplay scripts observe,
    /// and it is queued rather than fired inline because this call arrives FROM Lua — see
    /// [`GameScriptHost::pending_seat_events`].
    ///
    /// A rider already in a seat is moved, emitting the exit for the old seat first, so the
    /// "at most one seat per character" invariant holds and any handler watching the old vehicle sees
    /// the departure. Returns false only for a nil rider or vehicle, which cannot be seated.
    fn vehicle_enter(&mut self, veh: u64, rider: u64, seat: &str) -> bool {
        if veh == 0 || rider == 0 {
            return false;
        }
        let seat = if seat.is_empty() { "d" } else { seat }.to_ascii_lowercase();
        if let Some((old_veh, old_seat)) = self.seats.remove(&rider) {
            if old_veh != veh || old_seat != seat {
                self.pending_seat_events.push((rider, old_veh, old_seat, "x"));
            }
        }
        self.seats.insert(rider, (veh, seat.clone()));
        self.pending_seat_events.push((rider, veh, seat, "e"));
        true
    }

    /// `Vehicle.Exit(rider)` — unseat and queue the exit event. False if the rider was not seated.
    fn vehicle_exit(&mut self, rider: u64) -> bool {
        match self.seats.remove(&rider) {
            Some((veh, seat)) => {
                self.pending_seat_events.push((rider, veh, seat, "x"));
                true
            }
            None => false,
        }
    }

    /// `Vehicle.GetSeatFromRider(rider)` — the seat code, or `""` when not seated.
    fn vehicle_seat_from_rider(&self, rider: u64) -> String {
        self.seats.get(&rider).map(|(_, s)| s.clone()).unwrap_or_default()
    }

    /// `Object.InSeat(guid)` — is this character occupying a seat.
    fn object_in_seat(&self, guid: u64) -> bool {
        self.seats.contains_key(&guid)
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
            // Retail character templates carry the identity label, which the spawned character inherits.
            // `CreatePlayerCharacter` immediately `Player.AttachToCharacter`s this guid, so it is what
            // `Player.GetPrimaryCharacter()` (hence `MrxUtil.GetCharacterIdentity`) returns — stamp the
            // label here so `Object.HasLabel(uChar, "mattias"|"jennifer"|"chris")` succeeds.
            if let Some(label) = hero_identity_label(&self.hero_character) {
                self.object_add_label(guid, label);
            }
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
        if self.resolve_guid(guid) == HERO_GUID {
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
    // ===== The player concern → `mercs2_player`. =====
    //
    // These two accessors replace 21 bespoke overrides. Three of the old ones encoded behaviour the
    // code map contradicts and which is now deliberately gone:
    //   * the 1-billion cash clamp — a **Lua** soft-clamp in `MrxPmc.AddCashQty`, not native, and
    //     `mrxpmc.lua:474,538` bypass it;
    //   * clamping fuel to capacity — `mrxpmc.lua:114-115`'s job, not the engine's;
    //   * `player_any_character` returning `HERO_GUID` — retail pushes a constant sentinel and lets
    //     `Object.*`/`Human.*` resolve it.
    fn player_world(&mut self) -> Option<&mut mercs2_player::PlayerWorld> {
        Some(&mut self.player)
    }
    fn player_world_ref(&self) -> Option<&mercs2_player::PlayerWorld> {
        Some(&self.player)
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
    fn object_filter_add(&mut self, handle: u64, guid: u64, exclude: bool) {
        if let Some(f) = self.object_filters.get_mut(handle) {
            f.add(guid, exclude);
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
    fn render_state(&mut self) -> Option<&mut mercs2_core::RenderSettings> {
        Some(&mut self.render)
    }
    fn render_state_ref(&self) -> Option<&mercs2_core::RenderSettings> {
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

    // ===== `Human.Inventory.*` → `mercs2_combat::inventory` over the live ECS World. =====
    //
    // This replaced a `loadouts: HashMap<u64, Vec<u64>>` shadow table on this host. The shadow could not
    // be right in kind, not just in detail: shipped Lua calls `Object.GetParent(w)`,
    // `Weapon.GetReserveAmmo(w)` and `Object.HasLabel(w, "Grenade")` on whatever `GetAllWeapons` returns,
    // so those values must be **real ECS entities** ([[ecs-world-source-of-truth-deshadow]]).
    //
    // With no world attached (bare/test hosts) every call reports the neutral answer rather than
    // silently succeeding against a side table — which is also what an unresolvable handle does in
    // retail.
    fn inventory_set_weapons(&mut self, character: u64, weapons: Vec<u64>) -> bool {
        let (Some(world), Some(ch)) = (self.world.clone(), self.entity_of(character)) else {
            return false;
        };
        let ws: Vec<Entity> = weapons.iter().filter_map(|&g| self.entity_of(g)).collect();
        let mut wd = world.borrow_mut();
        mercs2_combat::inventory::set_all_weapons(&mut wd, ch, &ws)
    }
    fn inventory_weapons(&self, character: u64, exclude_flagged: bool) -> Vec<u64> {
        let (Some(world), Some(ch)) = (self.world.as_ref(), self.entity_of(character)) else {
            return Vec::new();
        };
        mercs2_combat::inventory::get_all(&world.borrow(), ch, exclude_flagged)
            .into_iter()
            .filter_map(|e| self.guid_of(e))
            .collect()
    }
    fn inventory_primary(&self, character: u64) -> u64 {
        let (Some(world), Some(ch)) = (self.world.as_ref(), self.entity_of(character)) else {
            return 0;
        };
        mercs2_combat::inventory::primary_weapon(&world.borrow(), ch)
            .and_then(|e| self.guid_of(e))
            .unwrap_or(0)
    }
    fn inventory_secondary(&self, character: u64) -> u64 {
        let (Some(world), Some(ch)) = (self.world.as_ref(), self.entity_of(character)) else {
            return 0;
        };
        mercs2_combat::inventory::secondary_weapon(&world.borrow(), ch)
            .and_then(|e| self.guid_of(e))
            .unwrap_or(0)
    }
    fn inventory_vehicle_weapon(&self, character: u64) -> u64 {
        let (Some(world), Some(ch)) = (self.world.as_ref(), self.entity_of(character)) else {
            return 0;
        };
        mercs2_combat::inventory::vehicle_weapon(&world.borrow(), ch)
            .and_then(|e| self.guid_of(e))
            .unwrap_or(0)
    }
    fn inventory_equip(&mut self, character: u64, weapon: u64) -> bool {
        let (Some(world), Some(ch), Some(w)) =
            (self.world.clone(), self.entity_of(character), self.entity_of(weapon))
        else {
            return false;
        };
        let mut wd = world.borrow_mut();
        mercs2_combat::inventory::equip(&mut wd, ch, w)
    }
    fn inventory_drop(&mut self, character: u64, weapon: u64) -> bool {
        let (Some(world), Some(ch), Some(w)) =
            (self.world.clone(), self.entity_of(character), self.entity_of(weapon))
        else {
            return false;
        };
        let mut wd = world.borrow_mut();
        mercs2_combat::inventory::drop_weapon(&mut wd, ch, w)
    }
    fn inventory_destroy_all(&mut self, character: u64) {
        let (Some(world), Some(ch)) = (self.world.clone(), self.entity_of(character)) else {
            return;
        };
        // Queues only — the reap runs from the frame pump, so a script that destroys and re-applies
        // within one frame still sees valid handles.
        mercs2_combat::inventory::destroy_all_weapons(&mut world.borrow_mut(), ch);
    }
    fn inventory_reload_all(&mut self, character: u64, arg2: Option<bool>) -> Option<bool> {
        let (Some(world), Some(ch)) = (self.world.clone(), self.entity_of(character)) else {
            return None;
        };
        let mut wd = world.borrow_mut();
        mercs2_combat::inventory::reload_all(&mut wd, ch, arg2)
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

    // ===== Graphics.FuelTrail. =====
    fn fire_ignite(&mut self, object: u64) {
        self.burning.insert(object);
    }
    fn fire_extinguish(&mut self, object: u64) {
        self.burning.remove(&object);
    }
    fn object_is_burning(&self, object: u64) -> bool {
        self.burning.contains(&object)
    }

    // ===== Health / damage → the live `mercs2_core::Health` component (shared with combat), with the
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

    // Player identity / session / binding now live on `self.player` — see `player_world()` above.
    /// The hero *template* selection (`chris`/`mattias`/`jen`), which is a game-side choice rather than
    /// a field of the retail player or profile record, so it stays here.
    fn player_selected_character(&self) -> String {
        self.hero_character.clone()
    }

    // ===== Object identity (derived from the recorded spawn requests + the hero). =====
    fn object_name(&self, guid: u64) -> String {
        self.name_of(guid).unwrap_or("").to_string()
    }
    fn object_model_name(&self, guid: u64) -> String {
        self.template_of(guid).unwrap_or("").to_string()
    }
    /// Returns the controlling player's GUID, `0` for none — **not** a predicate. See the trait docs;
    /// the shipped Lua binds the result and passes it to `Player.*`.
    fn object_is_player_controlled(&self, guid: u64) -> u64 {
        self.player.player_for_controlled_object(guid)
    }
    fn object_is_valid(&self, guid: u64) -> bool {
        let guid = self.resolve_guid(guid);
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

    // The stringly-keyed player-mode store is gone: the gates are typed fields on
    // `mercs2_player::PlayerObject` now, reached through `player_world()`.

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

/// Entity-ize the world's named markers into the live `World` + guidmap, so `Pg.GetGuidByName`
/// resolves them. Call once at world load, with [`crate::worldutil::world_name_index`], and **before**
/// [`register_landing_zones`] — a pad that carries a `Name` reuses the entity created here rather than
/// making a second one at the same spot.
///
/// `Pg.GetGuidByName` is the corpus's most-called binding (1240 sites) and the mission scripts' only
/// route to an authored entity; a name with no entity behind it returns nil, and shipped scripts
/// generally do not check. Feed this the *whole* index — see `world_name_index`'s header for why
/// `layers_static` alone silently loses ~38,000 names.
///
/// Names arrive already case-folded (the index's keys are lowercase); `pandemic_hash_m2` folds again,
/// so passing either spelling is equivalent.
pub fn register_named_markers(
    host: &Rc<RefCell<GameScriptHost>>,
    world: &Rc<RefCell<World>>,
    index: &std::collections::HashMap<String, [f32; 3]>,
) {
    {
        let mut w = world.borrow_mut();
        let h = host.borrow();
        for (name, pos) in index {
            let e = w.spawn((Transform::from_translation((*pos).into()),));
            h.register_named_entity(e, pandemic_hash_m2(name));
        }
    }
    println!("[world] {} named markers registered as live entities (guidmap)", index.len());
}

/// Entity-ize the world's transit landing pads into the live `World` + guidmap and record the
/// `(zone, slot) -> guid` index `Pg.GetAllLandingZones` answers from. Call once, at world load, with
/// [`crate::worldutil::landing_zone_pads`] over the level's `layers_static` block.
///
/// **Why the pads need entities at all:** `MrxTransit` treats each returned value as an ordinary object
/// guid — `Object.GetLocalizedName(uZoneGuid)` at `mrxtransit.lua:337`, `Object.GetPosition` for the map
/// blip at `:104`/`:189`, `MrxUtil.TeleportHeroesToLocations({uLocation1, uLocation2})` at `:89`. A bare
/// number would nil-index the moment the transit UI opened.
///
/// A pad that carries a `Name` COMP is looked up through the guidmap first, so it shares the ONE entity
/// the named-marker pass already created for that name (the shipped Lua reaches the same pads by name —
/// `Pg.GetGuidByName("01_pmc_hq_lz_playerone")`, `vz/wifpmcinterior.lua:2108` — and both routes must
/// land on the same object). Unnamed pads (retail vz has exactly one: zone 12 slot 1) get a fresh
/// anonymous entity at their authored `Transform`; they are reachable only through this index, which is
/// why the `LandingZone` COMP and not the name convention is the enumeration authority.
pub fn register_landing_zones(
    host: &Rc<RefCell<GameScriptHost>>,
    world: &Rc<RefCell<World>>,
    pads: &[crate::worldutil::LandingZonePad],
) {
    let mut index: Vec<(u32, u32, u64)> = Vec::with_capacity(pads.len());
    for pad in pads {
        let name_hash = pad.name.as_deref().map(pandemic_hash_m2);
        // Reuse the entity the named-marker pass registered under this name, if there is one.
        let existing = pad
            .name
            .as_deref()
            .map(|n| host.borrow_mut().guid_by_name(n))
            .filter(|g| *g != 0);
        let guid = match existing {
            Some(g) => g,
            None => {
                let e = world.borrow_mut().spawn((Transform::from_translation(pad.pos.into()),));
                match name_hash {
                    Some(h) => host.borrow().register_named_entity(e, h),
                    None => host.borrow().register_anonymous_entity(e),
                }
            }
        };
        if guid != 0 {
            index.push((pad.zone, pad.slot, guid));
        }
    }
    let n = index.len();
    host.borrow_mut().set_landing_zones(index);
    println!("[world] {n} transit landing pads registered (Pg.GetAllLandingZones)");
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
    // The corpus first, then the stand-in root for the handful of shipped modules the 370/382 decompile
    // does not include (`corpus/stubs/`). Earlier roots win a name collision, so a module becoming
    // available in the corpus automatically shadows its stand-in.
    let roots = mercs2_script::corpus::roots();
    let sh = if roots.is_empty() { ScriptHost::bare() } else { ScriptHost::new(roots) };
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

/// The `vz` master script — the level's boot entry. Its obfuscated retail name; `import`ing it runs
/// `Init()`, which is the real boot (see [`run_boot_flow`]).
const VZ_MASTER_SCRIPT: &str = "xQ!L";

/// Publish the host's [`BootSaveState`] as the Lua global `__boot_save_state`, in the exact shape
/// `xQ!L.LoadSingleton(tSaveData)` reads. Returns whether a save was published (`false` = new game,
/// and the global is set to `nil` so `Pg.LoadGame` answers false).
///
/// # The key set is required; the contents are optional
///
/// `_GameplaySetup_LoadWorldState` (`xQ!L.lua:792-812`) fans the save out to ~18 sibling
/// `LoadSingleton`s, and they do **not** guard their argument — `WifPmcInterior.LoadSingleton` opens
/// with a bare `if tSaveData.bUnlocked then` (`wifpmcinterior.lua:1632`), which throws on `nil`. Retail
/// never hits that because `SaveSingleton` always writes every key. So each of those keys gets an EMPTY
/// TABLE here: the subsystem sees "a save with nothing recorded for me", which every one of them
/// handles (they all test individual fields), instead of "no save at all", which none of them do.
///
/// Two keys are handled as **branch conditions** rather than passed to a loader, and an empty table is
/// truthy in Lua, so an empty one must be published as `nil`, not `{}`:
///
/// * `tRetryLocations` (`:645`) — the resume-spawn gate. Published as a real 1-based marker array WHEN
///   [`BootSaveState::retry_locations`] is non-empty (a mid-contract save), so `LoadSingleton` takes its
///   in-world checkpoint branch; left `nil` otherwise, so a hub save falls through to `Pmc_Entry1` +
///   `_bPmcRequired`. An EMPTY table here would be truthy and divert the spawn to a branch with no
///   marker — which is exactly the bug this gate fixes.
/// * `vEquippedSupport` (`:815`) — non-nil would replay an empty loadout over the default one.
///
/// Only content grounded in the decompile is filled in; a guessed shape would be worse than an empty
/// one, since a wrong table silently loads wrong state.
///
/// | key | consumer | source |
/// |---|---|---|
/// | `tFlowData.tMyFlowData` | `WifMissionFlow.LoadSingleton` → `_tMyFlowData` (`HasKey`) | `xQ!L.lua:861`, `mrxmissionflow.lua:609` |
/// | `tFlowData.tCulledBindings` | binding cull, so fired bindings don't replay | `mrxmissionflow.lua:612-631` |
/// | `tFlowData.tActiveMissions` | `UnlockMission(id, tData, false)` per entry | `mrxmissionflow.lua:632-634` |
/// | `tLayerData` | `MrxLayerManager.LoadSingleton` | `xQ!L.lua:707` |
fn install_boot_save_state(sh: &ScriptHost, host: &Rc<RefCell<GameScriptHost>>) -> mercs2_script::mercs2_luac::rt::Result<bool> {
    let lua = sh.lua();
    let Some(save) = host.borrow().boot_save_state().cloned() else {
        lua.globals().set("__boot_save_state", mercs2_script::mercs2_luac::rt::Value::Nil)?;
        return Ok(false);
    };

    let flow_keys = lua.create_table()?;
    for (k, v) in &save.flow_keys {
        flow_keys.set(k.as_str(), *v)?;
    }
    // `tCulledBindings` is iterated with `pairs` and its VALUES are the binding names — a 1-based array.
    let culled = lua.create_table()?;
    for (i, name) in save.culled_bindings.iter().enumerate() {
        culled.set(i + 1, name.as_str())?;
    }
    // `tActiveMissions` is keyed BY mission name; the value is that mission's own save blob, replayed
    // through `UnlockMission(name, tData, false)`. We carry ids only, so each blob is an empty table —
    // the mission unlocks and re-registers, it just doesn't restore mid-mission progress yet.
    let active = lua.create_table()?;
    for id in &save.active_missions {
        active.set(id.as_str(), lua.create_table()?)?;
    }
    let flow = lua.create_table()?;
    flow.set("tMyFlowData", flow_keys)?;
    flow.set("tCulledBindings", culled)?;
    flow.set("tActiveMissions", active)?;

    let layers = lua.create_table()?;
    for (i, l) in save.layers.iter().enumerate() {
        layers.set(i + 1, l.as_str())?;
    }

    let root = lua.create_table()?;
    // Every sibling sub-save the master script fans out to, in `xQ!L.lua` order. Present-but-empty, for
    // the reason in this function's doc comment: these loaders index their argument unguarded.
    for key in [
        "tPlayerData",         // :767  MrxPlayer.LoadSingleton
        "tBoundaryData",       // :793  WifVzBoundary.LoadSingleton
        "tPmcData",            // :795  MrxPmc.LoadSingleton
        // tSupportData (:796 MrxSupportData.LoadSingleton) is special-cased below — unlike its siblings
        // it indexes a sub-key unguarded, so a bare empty table is not enough.
        "tRewardData",         // :797  MrxRewardData.LoadSingleton
        "tFactionData",        // :798  MrxFactionManager.LoadSingleton
        "tPmcInteriorData",    // :799  WifPmcInterior.LoadSingleton  <- the one that first threw
        "tActiveHint",         // :800  WifHints.LoadSingleton
        "tActiveBio",          // :801  WifBios.LoadSingleton
        "tMunitionsData",      // :802  Munitions.LoadSingleton
        "tTutorialData",       // :805  MrxTutorialManager.LoadSingleton
        "tShopData",           // :806  MrxShop.LoadSingleton
        "tWifEquipmentData",   // :807  WifEquipmentData.LoadSingleton
        "tMrxAchievementsData", // :808 MrxAchievements.LoadSingleton
        "tLockedGates",        // :810  friendlygate.LoadSingleton
        "tStarterData",        // :862  MrxStarterManager.LoadSingleton
        "tStatsData",          // :864  MrxStatsManager.LoadSingleton
    ] {
        root.set(key, lua.create_table()?)?;
    }
    // `tSupportData` — `MrxSupportData.LoadSingleton` (`mrxsupportdata.lua:2445`) does an UNGUARDED
    // `tRequirementsObtained = tSaveData._tRequirementsObtained` (`:2449`), so an absent key nils the
    // module global; the recruit setters (`SetMechanicRecruited` `:79`, via `SynchNetRecruits` `:105`,
    // fired on the `WaitForStreaming` state exit) then INDEX it and throw. Unlike its siblings it does
    // not test individual fields, so a present-but-empty outer table is not enough — the inner
    // `_tRequirementsObtained` must itself be a table. Seed it empty ("nothing recorded for me"): the
    // global loads as a valid empty table, the setters assign into it, and `IsSupportEquippable` (`:57`)
    // simply reports not-yet-obtained until real support save-state is reconstructed. (Per this
    // function's rule we do NOT guess the recruit booleans — `Init` `:115` seeds them all before the
    // load, and an empty save-table is the honest "this save recorded no support progress".)
    let support = lua.create_table()?;
    support.set("_tRequirementsObtained", lua.create_table()?)?;
    root.set("tSupportData", support)?;
    // `WifPmcInterior.SetAvailableCostumes` (`:1522`) does `nAvailableCostumes - GetAvailableCostumes()`
    // with no nil guard, and `GetAvailableCostumes` reports `_nAvailableCostumes or 1`. Passing 1 is the
    // neutral value: zero newly-unlocked costumes, so the unlock fanfare stays silent.
    root.set("nAvailableCostumes", 1)?;
    root.set("tFlowData", flow)?;
    root.set("tLayerData", layers)?;

    // `tTransitData` — the shape `MrxTransit.LoadSingleton` (`mrxtransit.lua:378-404`) reads back:
    // the `bEnabled` master switch plus one sub-table per zone, keyed by ABSOLUTE zone number (the
    // set is sparse, so the key is not a position). `LoadSingleton` iterates with `pairs` and skips
    // non-number keys, and it indexes `_tLandingZones[nIndex]` unguarded — so a zone that is not in
    // the world's pad set must not appear here.
    let transit = lua.create_table()?;
    transit.set("bEnabled", save.transit_enabled)?;
    for z in &save.transit_zones {
        let t = lua.create_table()?;
        t.set("bEnabled", z.enabled)?;
        t.set("bIsNuked", z.is_nuked)?;
        t.set("bHasPlayedFanfare", z.played_fanfare)?;
        // Left NIL when the zone is unaffiliated, not written as a placeholder: `LoadSingleton`
        // branches on `if tData.sFactionAbbrev then` before touching the attitude test.
        if let Some(f) = &z.faction {
            t.set("sFactionAbbrev", f.as_str())?;
        }
        transit.set(z.zone, t)?;
    }
    root.set("tTransitData", transit)?;

    // `tRetryLocations` (`xQ!L.lua:645`) — the master script's resume-spawn GATE, read as a branch
    // condition (not handed to a loader). A save taken mid-contract carries its checkpoint marker(s)
    // here; publishing them makes `LoadSingleton` take the in-world checkpoint branch (`_bLoadIntoWorld`)
    // instead of `{"Pmc_Entry1","Pmc_Entry2"}` + `_bPmcRequired`, matching retail for a pre-PMC resume.
    // A save WITHOUT retry locations (a between-contracts / PMC hub save) leaves this NIL — an empty
    // table would be truthy and wrongly divert the spawn — so a hub resume still reaches the HQ entrance.
    if !save.retry_locations.is_empty() {
        let retry = lua.create_table()?;
        for (i, m) in save.retry_locations.iter().enumerate() {
            retry.set(i + 1, m.as_str())?;
        }
        root.set("tRetryLocations", retry)?;
    }

    lua.globals().set("__boot_save_state", root)?;
    Ok(true)
}

/// Run the **real vanilla boot Lua flow** through the resident host (bisect against the pmc_bb
/// `[lua]` trace). `MrxBootstrap.Start()` (mrxbootstrap.lua:14) imports the resident modules
/// (MrxPlayer/MrxPmc/MrxState/MrxUtil/…), registers the GUI-loaded + local-player-joined callbacks, and
/// calls `MrxPlayer.Start()`. Each `Debug.Printf` in that cascade surfaces as a `[lua]` line here, so
/// this is exactly what to diff against vanilla to find the first divergence.
///
/// # Where the hero starts is decided HERE, by the master script — not by this function
///
/// `xQ!L.Init()` (`vz/xQ!L.lua:457`) ends with:
///
/// ```lua
/// _bNewSession = true
/// local bSaveGameFound = Pg.LoadGame("InitialSaveData")
/// if not bSaveGameFound then LoadSingleton(nil) end
/// _bNewSession = nil
/// ```
///
/// and `LoadSingleton` (`:626-686`) picks the start marker from whether it got a save:
///
/// | branch | start locations | meaning |
/// |---|---|---|
/// | `tSaveData == nil` (**new game**) | `GetMissionStartLocations("VzaCon001")` → `{"VzaCon001_Start1"}` | the opening contract, before the player owns the PMC |
/// | `tSaveData.tRetryLocations` set (**resume mid-contract**) | the save's checkpoint marker(s), `_bLoadIntoWorld` | resume in the world at the checkpoint; NOT the PMC (a pre-PMC / in-progress save) |
/// | `tSaveData ~= nil`, no `tRetryLocations` (**resume at hub**) | `{"Pmc_Entry1", "Pmc_Entry2"}`, `_bPmcRequired = true` | the PMC HQ entrance |
///
/// This function's job is only to answer `Pg.LoadGame` truthfully from [`GameScriptHost::boot_save_state`]
/// and let the branch run. It deliberately does **not** call `MrxPlayer.SetSpawnLocations` itself: doing
/// that (with `<contract>_Start1`) is what previously made every New Game start inside the PMC interior,
/// because it overwrote the master script's answer a few lines after it was computed.
///
/// A script error anywhere in this flow is FATAL — see [`lua_fatal`].
pub fn run_boot_flow(sh: &ScriptHost, host: &Rc<RefCell<GameScriptHost>>, character: &str) {
    println!("[world] ===== vanilla boot Lua flow: MrxBootstrap.Start() =====");
    // Publish the save (or nothing) for the `Pg.LoadGame` seam below to hand to `LoadSingleton`.
    let resuming = match install_boot_save_state(sh, host) {
        Ok(v) => v,
        Err(e) => {
            println!("[world] boot save-state install failed ({e}) — booting as a new game");
            false
        }
    };
    println!(
        "[world] boot branch: {}",
        if resuming {
            let mid_contract = host.borrow().boot_save_state().map(|s| !s.retry_locations.is_empty()).unwrap_or(false);
            if mid_contract {
                "RESUME mid-contract -> checkpoint marker (in world, pre-PMC)"
            } else {
                "RESUME at hub -> Pmc_Entry1 (HQ entrance)"
            }
        } else {
            "NEW GAME -> VzaCon001_Start1 (opening contract)"
        }
    );
    // Drive the flow the way the engine does: MrxBootstrap.Start() registers the callbacks, the master
    // script's Init decides the spawn markers, and the player-joined path spawns the hero
    // (CreatePlayerCharacter → Pg.GetGuidByName → Object.GetPosition → Pg.Spawn).
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
         -- The engine's save-restore role. Retail `Pg.LoadGame(sName)` reads the profile and hands it to\n\
         -- the master script's LoadSingleton; returning false means \"no save\", which is the signal Init\n\
         -- uses to call LoadSingleton(nil) itself (the NEW GAME branch). We must call LoadSingleton from\n\
         -- INSIDE this call, because Init only sets `_bNewSession = true` around it -- and LoadSingleton\n\
         -- checks that flag to decide between MrxPlayer.SetSpawnLocations (a fresh spawn) and\n\
         -- `_tTeleportLocations` (moving an already-live hero).\n\
         Pg.LoadGame = function(sName)\n\
           local tSave = __boot_save_state\n\
           if not tSave then return false end\n\
           local M = _G[\"{master}\"]\n\
           if not (M and M.LoadSingleton) then return false end\n\
           M.LoadSingleton(tSave)\n\
           return true\n\
         end\n\
         -- Run the vz master script as the SOLE boot entry. Its Init is the real boot: \n\
         --   SetHandleStateTransitions(false) + MrxBootstrap.Start(_AttemptGameplaySetup) +\n\
         --   MrxState.EnableFade(false) + MrxPlayer.Reset + Pg.LoadGame -> LoadSingleton -> _LoadLayers ->\n\
         --   MrxLayerManager.Add -> the layer streaming the pump completes (Pg.LoadLayer callback) ->\n\
         --   _AttemptGameplaySetup static/dynamic -> MrxPlayer.Start (spawn) + _CompleteGameplaySetup\n\
         --   (act staging). We only supply the two async gates the non-rendering load can't signal.\n\
         local me, mi = pcall(import, \"{master}\")\n\
         if not me then Debug.Printf(\"master script (vz) aborted: \" .. tostring(mi)) end\n\
         -- GUI-load-complete gate (the shell's GUI-file loads finish).\n\
         local ge, ie = pcall(MrxBootstrap._GuiLoaded)\n\
         if not ge then Debug.Printf(\"_GuiLoaded aborted: \" .. tostring(ie)) end\n\
         -- The engine's player-joined signal. Retail wires it as\n\
         -- `Player.SetPlayerJoinedCallback(MrxPlayer.OnPlayerJoined)` (mrxplayer.lua:132); OnPlayerJoined\n\
         -- resolves the character config, reads `_tSpawnLocations[iPlayerId + 1]`, and calls\n\
         -- CreatePlayerCharacter with it (mrxplayer.lua:185-188). We call CreatePlayerCharacter with\n\
         -- those same two arguments rather than OnPlayerJoined itself: the co-op/GUI/AI bookkeeping it\n\
         -- does around the spawn (Player.BindToLocal, MrxGuiManager.CreateGui, Ai.AddSubject) has no\n\
         -- backing yet and would abort the callback before Pg.Spawn ran. The spawn-deciding half --\n\
         -- marker name -> Pg.GetGuidByName -> Object.GetPosition -> Pg.Spawn -- is verbatim vanilla.\n\
         local tSpawnLocs = MrxPlayer._tSpawnLocations\n\
         local vLoc = tSpawnLocs and tSpawnLocs[1] or Player.GetPlayerStart()\n\
         Debug.Printf(\"boot spawn marker: \" .. tostring(vLoc))\n\
         local ce, ci = pcall(MrxPlayer.CreatePlayerCharacter, true, 0, \"{character}\", vLoc)\n\
         if not ce then Debug.Printf(\"CreatePlayerCharacter aborted: \" .. tostring(ci)) end\n",
        master = VZ_MASTER_SCRIPT,
        character = character,
    );
    match sh.exec(&src, "@boot_flow") {
        Ok(()) => println!("[world] ===== boot flow started (Start + spawn); servicing state machine ====="),
        Err(e) => lua_fatal(format_args!("boot flow (first divergence): {e}")),
    }

    // Service the world-load state machine: pump the Lua timer/event system and fire the
    // `Event.GameStateChange` events for each `Sys.RequestGameState` the chain requests (we have no real
    // streaming/tether wait, so each requested state completes immediately — enter then exit). This
    // advances MrxState: Loading → WaitForGame → GlobalEnter → WaitForStreaming → … → GlobalExit.
    let mut idle_rounds = 0;
    for _ in 0..1200 {
        let before = host.borrow().lua_log_lines;
        pump_resident(sh, host, 0.1);
        let states = host.borrow_mut().take_pending_game_states();
        let serviced = !states.is_empty();
        for st in states {
            // Firing the "exit" phase runs the state's ReadyToExit callbacks — for WaitForStreaming that
            // is `_SecondaryStreamComplete → _StartPlayerVisibleGameplay → WifMissionFlow.Refresh(Exit,
            // WAITFORGAME)`, the chain that reaches GlobalExit. Surface any error (don't swallow it).
            // `{e:?}` deliberately, not `{e}`: a runtime error's Display carries the Lua traceback,
            // but a binding ARGUMENT-conversion failure (`Error::BadArgument`) prints only its cause
            // — "bad argument #1: error converting Lua nil to f64" with no hint which binding, which
            // is unactionable. The Debug form keeps the wrapper, naming the function.
            if let Err(e) = sh.fire_state_change(&st, "enter") {
                lua_fatal(format_args!("GameStateChange({st}, enter): {e:?}"));
            }
            if let Err(e) = sh.fire_state_change(&st, "exit") {
                lua_fatal(format_args!("GameStateChange({st}, exit): {e:?}"));
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

/// A Lua error is a BLOCKER: report it and take the process down. No opt-out.
///
/// These used to be printed and stepped over. That was worse than it looked: a script that errors
/// mid-callback has left the state machine part-way through a transition — gates stay armed, the
/// callbacks behind them never fire, and the world limps on in a state no shipped build ever reaches.
/// The run then *appears* to boot, so the failure reads as ugly logging rather than the blocker it is,
/// and it survives into every later session because nothing forces it to be dealt with.
fn lua_fatal(context: std::fmt::Arguments<'_>) -> ! {
    eprintln!("\n[script] FATAL: {context}");
    eprintln!(
        "[script] A mission script errored, so the state machine is stranded part-way through a\n\
         [script] transition. Continuing would run the world in a state no shipped build reaches."
    );
    let _ = std::io::Write::flush(&mut std::io::stderr());
    panic!("lua script error: {context}");
}

/// Advance the resident script host one fixed step: pump the Lua event/timer system (`Event.__pump(dt)`)
/// so `TimerRelative` fires and posted events dispatch. A no-op if `Event`/`__pump` aren't present.
///
/// Script errors raised in here are FATAL — see [`lua_fatal`].
pub fn pump_resident(sh: &ScriptHost, host: &Rc<RefCell<GameScriptHost>>, dt: f32) {
    // Objects whose layer completed LAST tick wake now — before this tick's flush, so the gates armed
    // by that flush's callback cascade are already registered. See `GameScriptHost::queue_layer_wakes`
    // for why this cannot fire inside the flush itself.
    //
    // The drain is bound to a local FIRST, deliberately. Writing `for guid in
    // host.borrow_mut().take_pending_wakes()` keeps the `RefMut` alive for the whole loop, and these
    // callbacks are precisely the ones that reenter the host — `_PutPlayersInBoat`
    // (`vz/vzacon001.lua:83`) calls `Player.GetAllPlayers()` on the first guid it wakes, which panics
    // with "RefCell already mutably borrowed".
    let wakes = host.borrow_mut().take_pending_wakes();
    for guid in wakes {
        if let Err(e) = sh.fire_object_hibernation(guid, mercs2_script::PHASE_AWAKE) {
            lua_fatal(format_args!("object-wake callback (guid {guid:#x}): {e:?}"));
        }
    }

    // Seat transitions from last tick's `Vehicle.Enter`/`Vehicle.Exit`. Same drain-to-a-local
    // discipline: these callbacks reenter the host freely (`EnsureHeroesInBoat` → `AssetsLoaded` →
    // the whole mission-flow cascade).
    //
    // Looped until empty, not drained once: a seat handler may seat somebody else — `vzacon001.lua`'s
    // `_PutPlayersInBoat` seats both heroes, and `EnsureHeroesInBoat` only advances once the LAST one
    // is in. Draining a single batch would leave the second transition to the following tick and, in a
    // pump that runs a bounded number of times, could strand the gate.
    // Bounded: a handler that seats somebody on every fire (a swap loop, or a persistent handler
    // re-entering its own vehicle) would otherwise spin the frame forever. Eight rounds is far above
    // any real cascade — the deepest shipped one is two heroes into one boat — and hitting the cap is
    // reported rather than silently truncated, because a silent cap here looks exactly like a gate that
    // never fired.
    const MAX_SEAT_CASCADE: usize = 8;
    for round in 0..MAX_SEAT_CASCADE {
        let seat_events = host.borrow_mut().take_pending_seat_events();
        if seat_events.is_empty() {
            break;
        }
        for (occupant, vehicle, seat, action) in seat_events {
            if let Err(e) = sh.fire_object_in_seat(occupant, vehicle, &seat, action) {
                lua_fatal(format_args!("seat callback (occupant {occupant:#x} {action}): {e:?}"));
            }
        }
        if round == MAX_SEAT_CASCADE - 1 && !host.borrow().pending_seat_events.is_empty() {
            println!(
                "[script] WARNING: seat-event cascade still producing after {MAX_SEAT_CASCADE} rounds; \
                 the remainder runs next tick. Suspect a handler that re-seats on every fire."
            );
        }
    }

    // Fire completed layer loads (the engine's async streaming callback) THEN pump timers/events, so the
    // MrxLayerManager fulfilment + the gameplay-setup signal it triggers advance each tick.
    if let Err(e) = sh.exec(
        &format!(
            "if Pg and Pg.__flush_layer_loads then Pg.__flush_layer_loads() end\n\
             if Event and Event.__pump then Event.__pump({dt}) end"
        ),
        "@resident_pump",
    ) {
        lua_fatal(format_args!("resident pump: {e:?}"));
    }

    // Collect the layers that just streamed in and queue their objects for next tick's wake.
    if let Ok(streamed) = sh.take_streamed_layers() {
        let mut h = host.borrow_mut();
        for layer in streamed {
            let n = h.queue_layer_wakes(&layer);
            if n > 0 {
                println!("[stream] layer '{layer}' streamed in: {n} object(s) queued to wake");
            }
        }
    }
    // Then the engine-completed callbacks: movie ends, boundary crossings, PDA/satellite, disguise.
    // Without this the retained closures are never invoked and anything waiting on one hangs — which is
    // what kept the world-load machine parked in `STATE_WAITFORGAME` behind the intro cinematic.
    let shared: mercs2_script::SharedHost = host.clone();
    if let Err(e) = sh.pump_callbacks(&shared, dt) {
        lua_fatal(format_args!("callback pump: {e:?}"));
    }
}

/// Locate the vendored Lua corpus root. Returns `None` only where the corpus is genuinely absent
/// (a crates.io consumer), and callers skip rather than fail.
///
/// Delegates to [`mercs2_script::corpus::root`] — **no path is constructed here**. The baked path this
/// used to carry assumed one particular checkout layout, so from a differently-laid-out clone it
/// resolved to nothing and every corpus-driven test silently reported "0 [lua] lines", including
/// `boot_flow_runs_real_game_lua`, which was failing for that reason and not for a boot regression.
fn discover_lua_root() -> Option<PathBuf> {
    mercs2_script::corpus::root()
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

    /// GAP-2 regression: the hero's identity label round-trips so `MrxUtil.GetCharacterIdentity`
    /// (`mrxutil.lua:649`, which loops `Object.HasLabel(uChar, "mattias"|"jennifer"|"chris")`) resolves
    /// instead of erroring "not one of M/J/C". Two stamp sites must both hold: the pre-boot `HERO_GUID`
    /// possession, and the `Pg.Spawn`'d hero guid that `CreatePlayerCharacter → AttachToCharacter`
    /// possesses (what `Player.GetPrimaryCharacter` actually returns). Also pins the `jen → jennifer`
    /// fold — the reimpl's short base name vs the label the retail Lua tests.
    #[test]
    fn hero_identity_label_round_trips_for_getcharacteridentity() {
        for (hero, label) in [("mattias", "mattias"), ("chris", "chris"), ("jen", "jennifer")] {
            let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
            host.borrow_mut().set_boot_context(hero);
            // Pre-spawn window: the construction-time possession points at HERO_GUID.
            assert!(
                host.borrow().object_has_label(HERO_GUID, label),
                "{hero}: HERO_GUID must carry the identity label {label} before the spawn"
            );
            // The boot's Pg.Spawn(hero, …) mints a fresh guid (the one AttachToCharacter possesses).
            let spawned = host.borrow_mut().pg_spawn(hero, [1.0, 0.0, 2.0], 0.0, false);
            assert_ne!(spawned, HERO_GUID, "the spawned hero gets a distinct script-space guid");
            assert!(
                host.borrow().object_has_label(spawned, label),
                "{hero}: GetCharacterIdentity reads the SPAWNED guid — it must carry {label}"
            );
        }
        // A non-hero spawn is not tagged (no fabricated identity on ambient actors).
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        host.borrow_mut().set_boot_context("mattias");
        let civ = host.borrow_mut().pg_spawn("civ_hum_male_business", [0.0; 3], 0.0, false);
        assert!(!host.borrow().object_has_label(civ, "mattias"));
    }

    /// Mint a handle from a literal, for tests that need a **known** guid on both sides — e.g.
    /// asserting `host.faction.accumulator(777)` after driving Lua that names 777.
    ///
    /// Scripts never do this. The engine hands handles out (`Pg.GetGuidByName`,
    /// `Player.GetLocalCharacter`) and they cross as lightuserdata; `mercs2_script::Guid` refuses to
    /// read one out of a number, because this VM's `lua_Number` is f32 and cannot carry a handle
    /// above 2^24 without aliasing a different object. These tests used to pass bare integers and
    /// relied on a transitional arm that has since been removed. Every literal below is small enough
    /// to be exact in f32, so minting one here is a faithful stand-in for an engine-supplied handle.
    fn install_guid_helper(sh: &ScriptHost) {
        let f = sh
            .lua()
            .create_function(|_, n: f64| {
                // The literal arrives through a Lua number, i.e. f32. Above 2^24 that silently
                // rounds — `__guid(0x1000_0001)` would mint 0x1000_0000 and quietly fail to match
                // the handle the host fires with. Refuse instead: a test needing a real-range guid
                // must use `set_guid`, which never crosses a number.
                assert!(
                    n.abs() < (1u64 << 24) as f64,
                    "__guid({n}) is beyond f32's exact integer range — use set_guid() instead"
                );
                Ok(mercs2_script::Guid(n as u64))
            })
            .unwrap();
        sh.lua().globals().set("__guid", f).unwrap();
    }

    /// Bind a handle to a Lua global directly, without it ever being a number.
    ///
    /// This is how the engine really gives a script a handle, and the only way to carry one at or
    /// above 2^24 — which every dynamic guid is (`mercs2_core::FIRST_DYNAMIC_GUID` = 2^28).
    #[allow(dead_code)]
    fn set_guid(sh: &ScriptHost, name: &str, g: u64) {
        sh.lua().globals().set(name, mercs2_script::Guid(g)).unwrap();
    }

    /// The audio system is wired in: real game `Sound.*` Lua drives the live `crate::audio::AudioEngine`
    /// through the `EngineHost` forwarding (not a test double). `SetDynamicMusic`/`IsDynamicMusic`
    /// round-trip deterministically; an unknown cue (no sounddb) returns nil, faithful to the exe.
    #[test]
    fn game_lua_sound_drives_real_audio_engine() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

        let dyn_on: bool = sh
            .eval("Sound.SetDynamicMusic(true); return Sound.IsDynamicMusic()")
            .unwrap();
        assert!(dyn_on, "SetDynamicMusic/IsDynamicMusic must round-trip through the real AudioEngine");
        assert!(host.borrow().audio.borrow().is_dynamic_music());

        // Music FSM: registering a state then a self-transition drives the real dual-deck FSM.
        sh.exec(r#"Sound.AddMusicState("combat")"#, "@ms").unwrap();

        // CueSound with no bank loaded → nil (faithful); the forwarding is exercised regardless.
        // `Sound.CueSound(uEmitter, sCue)` — the emitter is arg 1 (118 shipped call sites); a cue-only
        // call raises now that handles are typed.
        let cue_nil: bool = sh.eval(r#"return Sound.CueSound(0, "ui_confirm") == nil"#).unwrap();
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
        install_guid_helper(&sh);

        // Order verb (table form) posts to the recovered 1024-slot action ring.
        sh.exec(r#"Ai.Anchor({AIGuid = __guid(0x1000), AnchorRadius = 0})"#, "@ai").unwrap();
        sh.exec(r#"Ai.Goal(__guid(0x1000), "Attack")"#, "@ai").unwrap();
        assert_eq!(host.borrow().ai.bus.len(), 2, "Ai order + goal both post to the ring");

        // Faction: a scripted infraction accrues into the mood accumulator...
        let faction: i64 = 777;
        sh.exec(&format!("Ai.AddInfraction(__guid(1), __guid({faction}), 100)"), "@ai").unwrap();
        assert!(!host.borrow().faction.accumulator(faction as u32).is_empty(), "AddInfraction accrues mood");

        // ...and SetInfractionMultiplier(0) DISABLES further infractions for that faction (shipped
        // gurcon002 pattern): a second faction at multiplier 0 stays empty.
        let quiet: i64 = 888;
        sh.exec(&format!("Ai.SetInfractionMultiplier(__guid({quiet}), 0); Ai.AddInfraction(__guid(1), __guid({quiet}), 100)"), "@ai").unwrap();
        assert!(host.borrow().faction.accumulator(quiet as u32).is_empty(), "multiplier 0 ignores infractions");

        // SetAttitude writes the directed relation the faction manager (and AI matrix) hold.
        sh.exec("Ai.SetAttitude(__guid(777), __guid(42), -100)", "@ai").unwrap();
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
        install_guid_helper(&sh);

        let veh: i64 = 0x2000;
        // Full happy-path lifecycle through Lua; each verb returns the resulting state name.
        let started: String = sh.eval(&format!("return Vehicle.HijackStart(__guid({veh}))")).unwrap();
        assert_eq!(started, "started");
        let done: String = sh
            .eval(&format!("Vehicle.SetHijackSuccess(__guid({veh})); return Vehicle.HijackComplete(__guid({veh}))"))
            .unwrap();
        assert_eq!(done, "complete");
        assert_eq!(host.borrow().vehicle_hijack_state(veh as u64), "complete");

        // Turret + rotor articulation lands on the host TurretAim.
        sh.exec(&format!("Vehicle.SetTurretYaw(__guid({veh}), 1.5); Vehicle.SpinHeli(__guid({veh}), true)"), "@v").unwrap();
        let aim = host.borrow().turrets.get(&(veh as u64)).copied().unwrap();
        assert_eq!(aim.yaw, 1.5);
        assert!(aim.rotor_spinning);

        // Cancel from a fresh vehicle returns to idle.
        let cancelled: String = sh
            .eval("Vehicle.HijackStart(__guid(0x3000)); return Vehicle.CancelHijack(__guid(0x3000))")
            .unwrap();
        assert_eq!(cancelled, "idle");

        // Seat occupancy + weapon restore land on real host state.
        sh.exec("Vehicle.EnterBySeatGuid(__guid(0x11), __guid(0x99))", "@v").unwrap();
        assert_eq!(host.borrow().human_seat(0x11), 0x99);
        sh.exec("Human.ForceExitSeatNoSnap(__guid(0x11))", "@v").unwrap();
        assert_eq!(host.borrow().human_seat(0x11), 0);
        sh.exec("Weapon.SetClipAmmo(__guid(0x88), 1); Vehicle.RestoreAmmo(__guid(0x88))", "@v").unwrap();
        assert_eq!(host.borrow().weapon_clip(0x88), host.borrow().weapon_max_clip(0x88));
    }

    /// The `Sys.Set*` config surface is WIRED to a real settings store: `Set*` ↔ `Get*` roundtrip
    /// through the host (not no-ops that drop the write).
    #[test]
    fn game_lua_sys_settings_roundtrip() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

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
        install_guid_helper(&sh);

        // Label two objects, then filter for "China&&Vehicle".
        sh.exec(
            r#"
            Object.AddLabel(__guid(100), "China"); Object.AddLabel(__guid(100), "Vehicle")
            Object.AddLabel(__guid(200), "China")
            uFilter = ObjectFilter.Create()
            ObjectFilter.SetFilter(uFilter, "China&&Vehicle")
        "#,
            "@of",
        )
        .unwrap();

        // 100 (China+Vehicle) matches; 200 (China only) does not — real predicate evaluation.
        let m100: bool = sh.eval("return ObjectFilter.Eval(uFilter, __guid(100))").unwrap();
        let m200: bool = sh.eval("return ObjectFilter.Eval(uFilter, __guid(200))").unwrap();
        assert!(m100, "China&&Vehicle matches the labelled vehicle");
        assert!(!m200, "China-only object fails China&&Vehicle");

        // Explicit include overrides a failing predicate. Arg 3 is **bExclude**, so `false` includes
        // (retail polarity — see `ObjectFilter::add`; this test asserted the inverse until 2026-07-26).
        sh.exec("ObjectFilter.AddObject(uFilter, __guid(200), false)", "@of").unwrap();
        let m200b: bool = sh.eval("return ObjectFilter.Eval(uFilter, __guid(200))").unwrap();
        assert!(m200b, "explicit include forces a match");
        let objs: Vec<mercs2_script::Guid> =
            sh.eval("return ObjectFilter.GetObjects(uFilter, false)").unwrap();
        let objs: Vec<i64> = objs.into_iter().map(|g| g.raw() as i64).collect();
        assert_eq!(objs, vec![200]);

        // And the other way: `true` excludes, beating a passing predicate.
        sh.exec("ObjectFilter.AddObject(uFilter, __guid(100), true)", "@of").unwrap();
        let m100b: bool = sh.eval("return ObjectFilter.Eval(uFilter, __guid(100))").unwrap();
        assert!(!m100b, "bExclude=true must exclude even when the predicate passes");
    }

    /// `Object.Attach`/`Detach` drive a REAL attachment graph the getters read (were no-op stubs +
    /// default getters — the parent never changed).
    #[test]
    fn game_lua_object_attach_graph() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

        sh.exec("Object.Attach(__guid(500), __guid(10)); Object.Attach(__guid(501), __guid(10))", "@a").unwrap();
        // Handles cross the boundary as lightuserdata now (see `mercs2_script::guid`), so read them
        // as `Guid` rather than `i64` — and assert the Lua-visible type, since that is what the
        // shipped `type(u) == "userdata"` gates test.
        let parent: mercs2_script::Guid = sh.eval("return Object.GetParent(__guid(500))").unwrap();
        assert_eq!(parent.raw(), 10, "GetParent reads the attachment graph");
        let kind: String = sh.eval("return type(Object.GetParent(__guid(500)))").unwrap();
        assert_eq!(kind, "userdata");

        let attached: bool = sh.eval("return Object.IsAttached(__guid(500))").unwrap();
        assert!(attached);
        let mut kids: Vec<mercs2_script::Guid> =
            sh.eval("return Object.GetAttachedObjects(__guid(10))").unwrap();
        kids.sort();
        assert_eq!(kids.iter().map(|g| g.raw()).collect::<Vec<_>>(), vec![500, 501], "both children");

        sh.exec("Object.Detach(__guid(500))", "@a").unwrap();
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
        install_guid_helper(&sh);

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

    /// `_GuiInternal.*` drives the REAL `crate::widgets::WidgetTree`: create widgets, set/get their state, parent
    /// them, and text/image data round-trips — all through Lua (was a no-op HUD).
    #[test]
    fn game_lua_hud_drives_real_widget_tree() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

        // Create a text widget, set its text + visibility → read them back.
        sh.exec(
            r#"
            wRoot = _GuiInternal.CreateWidget()
            wText = _GuiInternal.CreateTextWidget()
            _GuiInternal.SetTextText(wText, "OBJECTIVE COMPLETE")
            _GuiInternal.SetTextScale(wText, 2.0)
            _GuiInternal.SetWidgetVisible(wText, false)
            _GuiInternal.SetWidgetLocation(wText, 100, 200, 340, 264)
            _GuiInternal.AddWidgetChild(wRoot, wText)
        "#,
            "@hud",
        )
        .unwrap();

        let text: String = sh.eval("return _GuiInternal.GetTextText(wText)").unwrap();
        assert_eq!(text, "OBJECTIVE COMPLETE");
        let scale: f32 = sh.eval("return _GuiInternal.GetTextScale(wText)").unwrap();
        assert_eq!(scale, 2.0);
        let vis: bool = sh.eval("return _GuiInternal.GetWidgetVisible(wText)").unwrap();
        assert!(!vis, "SetWidgetVisible(false) persisted");
        // A widget location is a rect: four in, four out (`mrxguibase.lua` `Widget:GetLocation`
        // destructures `nX1, nY1, nX2, nY2`).
        let loc: (f32, f32, f32, f32) = sh.eval("return _GuiInternal.GetWidgetLocation(wText)").unwrap();
        assert_eq!(loc, (100.0, 200.0, 340.0, 264.0));

        // The tree really parented the text under the root.
        let wtext: i64 = sh.eval("return wText").unwrap();
        let kids: Vec<i64> = sh.eval("return _GuiInternal.GetWidgetChildren(wRoot)").unwrap();
        assert_eq!(kids, vec![wtext]);
        assert_eq!(host.borrow().hud.len(), 2, "two widgets live in the tree");

        // Gui markers drive the real MarkerSet.
        sh.exec(
            r#"
            mObj = Gui.AddObjective()
            Gui._MarkerSetLocation(mObj, 300, 5, 400)
            Gui._MarkerSetFollowGuid(mObj, __guid(0x1234))
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

    /// The presentation namespaces drive the real `mercs2_core::RenderSettings`: the atmosphere generic
    /// value/color store + bloom/graphics/fade params round-trip through Lua (were no-op stubs).
    #[test]
    fn game_lua_render_state_roundtrip() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

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
        // `Camera.Shake(uCamera, sShake, uTarget, nAmp, nTime)` — the single-float form never existed.
        sh.exec(
            "Camera.SetPosition(1,2,3); Camera.Follow(__guid(0x77)); Camera.Shake(__guid(0), \"ShakeCameraMedium\", __guid(0), 0.5, 0)",
            "@cam",
        )
        .unwrap();
        assert_eq!(host.borrow().camera_fx.position, [1.0, 2.0, 3.0]);
        assert_eq!(host.borrow().camera_fx.follow_guid, 0x77);
        assert_eq!(host.borrow().camera_fx.shake, 0.5);
    }

    /// `Human.Inventory.*` drives a REAL per-character weapon loadout: set/get/equip/drop round-trips through
    /// Lua (was empty getters + no-op mutators).
    #[test]
    fn game_lua_inventory_loadout() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

        // The loadout is real ECS state now, so this needs a live world with entities that carry
        // `RuntimeInventory` (the human) and `Equipment` (the weapons). The previous version of this
        // test passed bare integers against a flat `HashMap<u64, Vec<u64>>` shadow table on the host —
        // a model `inventory_equipment_code_map.md` §10 rejects outright.
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        let (ch, rifle, carbine, pistol) = {
            let mut w = world.borrow_mut();
            let mut g = guids.borrow_mut();
            use mercs2_combat::components::{Equipment, EquipmentType, RuntimeInventory};
            let ch = w.spawn((RuntimeInventory::default(), mercs2_core::HumanState::default()));
            let rifle = w.spawn((Equipment { class: EquipmentType::Primary },));
            let carbine = w.spawn((Equipment { class: EquipmentType::Primary },));
            let pistol = w.spawn((Equipment { class: EquipmentType::Secondary },));
            for (e, guid) in [(ch, 0x1000u64), (rifle, 0x10), (carbine, 0x20), (pistol, 0x30)] {
                g.register(e, None, guid);
            }
            (ch, rifle, carbine, pistol)
        };
        let _ = (ch, rifle, carbine, pistol);
        host.borrow_mut().attach_world(world.clone(), guids.clone());

        // `SetAllWeapons` returns a BOOLEAN (it pushed nothing before).
        let ok: bool = sh.eval("return Human.Inventory.SetAllWeapons(__guid(0x1000), {__guid(0x10), __guid(0x20), __guid(0x30)})").unwrap();
        assert!(ok, "SetAllWeapons pushes a boolean");

        // `GetAllWeapons` returns ONE array table — primaries (equipped first) then secondaries.
        // Asserted through Lua so the assertion sees exactly what a script sees.
        let (n, last): (i64, mercs2_script::Guid) = sh
            .eval(
                "local t = Human.Inventory.GetAllWeapons(__guid(0x1000)) \
                 return #t, t[#t]",
            )
            .unwrap();
        assert_eq!(n, 3, "one flat list of all three weapons");
        assert_eq!(last.raw(), 0x30, "the secondary sorts after the primaries");

        // And the handles it hands back are lightuserdata, so a script's own `type(w) == "userdata"`
        // gate passes on them.
        let kind: String =
            sh.eval("return type(Human.Inventory.GetAllWeapons(__guid(0x1000))[1])").unwrap();
        assert_eq!(kind, "userdata", "handles cross the boundary as lightuserdata");

        // Primary and secondary are occupied SIMULTANEOUSLY — the single-index model made these two
        // getters mutually exclusive.
        let p: mercs2_script::Guid = sh.eval("return Human.Inventory.GetPrimaryWeapon(__guid(0x1000))").unwrap();
        let s: mercs2_script::Guid = sh.eval("return Human.Inventory.GetSecondaryWeapon(__guid(0x1000))").unwrap();
        assert!(p.is_some() && s.raw() == 0x30, "both slots live at once: {p:?} / {s:?}");

        // `DropWeapon` returns a boolean.
        let dropped: bool = sh.eval("return Human.Inventory.DropWeapon(__guid(0x1000), __guid(0x20))").unwrap();
        assert!(dropped);
        // Not because `Drop` promotes — because `GetPrimaryWeapon` falls back to `+0x0C` (§4.6/§8.3).
        let p: mercs2_script::Guid = sh.eval("return Human.Inventory.GetPrimaryWeapon(__guid(0x1000))").unwrap();
        assert_eq!(p.raw(), 0x10, "the getter falls back to the other primary");

        // `ReloadAll` REQUIRES its second argument — nil without it, per retail's bail.
        let bare: Option<bool> = sh.eval("return Human.Inventory.ReloadAll(__guid(0x1000))").unwrap();
        assert_eq!(bare, None, "no arg 2 -> nil");
        let with: Option<bool> = sh.eval("return Human.Inventory.ReloadAll(__guid(0x1000), false)").unwrap();
        assert_eq!(with, Some(true));

        // An unresolvable handle reads nil and does not raise.
        let none: Option<i64> = sh.eval("return Human.Inventory.GetPrimaryWeapon(__guid(0x9999))").unwrap();
        assert_eq!(none, None);
    }

    /// Weapon ammo, Fire burning state, and Object health/SendDamage are REAL host state driven through
    /// Lua (were no-op stubs / empty getters).
    #[test]
    fn game_lua_weapon_fire_damage() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

        // Weapon ammo: set clip + reserve, then Reload pulls from reserve into the clip.
        let w: i64 = 0x555;
        sh.exec(&format!("Weapon.SetClipAmmo(__guid({w}), 5); Weapon.SetReserveAmmo(__guid({w}), 90)"), "@wp").unwrap();
        assert_eq!(sh.eval::<i64>(&format!("return Weapon.GetClipAmmo(__guid({w}))")).unwrap(), 5);
        sh.exec(&format!("Weapon.Reload(__guid({w}))"), "@wp").unwrap();
        // clip refills to max_clip (30), reserve drops by the 25 taken.
        assert_eq!(sh.eval::<i64>(&format!("return Weapon.GetClipAmmo(__guid({w}))")).unwrap(), 30);
        assert_eq!(sh.eval::<i64>(&format!("return Weapon.GetReserveAmmo(__guid({w}))")).unwrap(), 65);

        // Fire: Ignite sets burning, Extinguish clears it.
        sh.exec("Graphics.FuelTrail.Ignite(__guid(0x700))", "@fr").unwrap();
        assert!(host.borrow().object_is_burning(0x700));
        sh.exec("Graphics.FuelTrail.Extinguish(__guid(0x700))", "@fr").unwrap();
        assert!(!host.borrow().object_is_burning(0x700));

        // SendDamage reduces health; enough damage kills (returns true).
        let died_partial: bool = sh.eval("return ObjectState.SendDamage(__guid(0x800), 40)").unwrap();
        assert!(!died_partial);
        assert_eq!(host.borrow().object_health(0x800), 60.0);
        let died: bool = sh.eval("return ObjectState.SendDamage(__guid(0x800), 100)").unwrap();
        assert!(died, "lethal damage returns died=true");
        assert!(!host.borrow().object_is_alive(0x800));
    }

    /// `Pg` regions/alarms + `Airstrike` designators/ordnance drive real host state through Lua.
    #[test]
    fn game_lua_pg_regions_and_airstrike() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

        // Region registry: CreateRegion mints a stable handle; re-creating the name reuses it.
        let r1: mercs2_script::Guid =
            sh.eval(r#"return Junk.CreateRegion("bank_lobby", 10, 0, 20, 5)"#).unwrap();
        let r2: mercs2_script::Guid =
            sh.eval(r#"return Junk.CreateRegion("bank_lobby", 11, 0, 21, 6)"#).unwrap();
        let (r1, r2) = (r1.raw() as i64, r2.raw() as i64);
        assert_eq!(r1, r2, "same-named region reuses its handle");
        assert_eq!(host.borrow().regions.get(&(r1 as u64)).copied(), Some(([11.0, 0.0, 21.0], 6.0)));

        // Alarm state: Activate then Toggle.
        sh.exec("Junk.ActivateAlarm(__guid(0x42), true)", "@al").unwrap();
        assert!(host.borrow().pg_alarm_active(0x42));
        let now: bool = sh.eval("return Junk.ToggleAlarm(__guid(0x42))").unwrap();
        assert!(!now, "toggle turns the active alarm off");

        // Airstrike designator lifecycle + FindDesignatorOwner.
        sh.exec("Airstrike.EquipDesignator(__guid(0x2))", "@as").unwrap();
        let owner: mercs2_script::Guid = sh.eval("return Airstrike.FindDesignatorOwner()").unwrap();
        let owner = owner.is_some().then(|| owner.raw() as i64);
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
        install_guid_helper(&sh);

        let g: i64 = 0x1000;
        assert!(host.borrow().human_weapons_enabled(g as u64), "weapons enabled by default");
        sh.exec(&format!("Human.DisableWeapons(__guid({g}))"), "@hu").unwrap();
        assert!(!host.borrow().human_weapons_enabled(g as u64), "DisableWeapons persisted");
        sh.exec(&format!("Human.EnableWeapons(__guid({g}))"), "@hu").unwrap();
        assert!(host.borrow().human_weapons_enabled(g as u64));

        sh.exec(&format!("Human.Knockdown(__guid({g}))"), "@hu").unwrap();
        assert!(host.borrow().human_is_knocked_down(g as u64), "Knockdown ragdolls the human");

        // StopGrappling clears the grapple flag; IsGrappling reads the real store.
        host.borrow_mut().human_flags.entry(g as u64).or_default().grappling = true;
        let grap: bool = sh.eval(&format!("return Human.IsGrappling(__guid({g}))")).unwrap();
        assert!(grap);
        sh.exec(&format!("Human.StopGrappling(__guid({g}))"), "@hu").unwrap();
        assert!(!host.borrow().human_is_grappling(g as u64));
    }

    /// `Net.*` session mode drives real NetState: SP defaults to offline server; ConnectToServer/
    /// StartServer/Stop transition it, and the getters read it.
    #[test]
    fn game_lua_net_session_mode() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

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
        install_guid_helper(&sh);

        // Emitters + state-machine state.
        sh.exec(r#"ObjectState.StartEmitter(__guid(0x10), "smoke"); ObjectState.SetState(__guid(0x10), "Damaged")"#, "@os").unwrap();
        assert!(host.borrow().object_emitter_active(0x10, "smoke"));
        assert_eq!(host.borrow().object_sm_state(0x10), "Damaged");
        sh.exec(r#"ObjectState.StopEmitter(__guid(0x10), "smoke")"#, "@os").unwrap();
        assert!(!host.borrow().object_emitter_active(0x10, "smoke"));

        // Face: bound set + current expression.
        sh.exec(r#"Face.BindFaceAnimSet(__guid(0x20), "mattias_faces"); Face.PlayFacialExpression(__guid(0x20), "angry")"#, "@fa").unwrap();
        assert_eq!(host.borrow().face_current(0x20), "angry");

        // Report lifecycle finalizes the faction mood report (no infractions → 0).
        sh.exec("Report.Init({ SimultaneousReporters = 1 }); Report.SetDelay(2.0)", "@rp").unwrap();
        let inf: i64 = sh.eval("return Report.GetInfractions()").unwrap();
        assert_eq!(inf, 0);
        sh.exec("Report.Completed()", "@rp").unwrap();
    }

    /// `Player.Set*` mode gates take `(handle, value)` and are **observable by their getters**.
    ///
    /// This is the regression test for the inversion defect: every gate used to be declared
    /// `|_, on: Option<bool>|`, so it read argument 1 — the player handle — as its flag, and mlua's
    /// Lua-truthiness conversion (`_ => true`) meant a handle always converted to `true`. Passing
    /// `false` therefore *set* the gate. `mrxutil.lua:975` calls `SetCinematicMode(uPlayer, false)`.
    #[test]
    fn game_lua_player_mode_gates_take_a_handle_and_a_value() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

        // The shipped shape: the player handle first, then the value.
        sh.exec(
            "local p = Player.GetLocalPlayer() \
             Player.SetInputEnabled(p, false) \
             Player.SetCinematicMode(p, true) \
             Player.SetHealthClamp(p, true) \
             Player.SetGrappleEnabled(p, true)",
            "@pl",
        )
        .unwrap();

        {
            let h = host.borrow();
            let p = h.player().roster.local().expect("a local player");
            assert!(!p.input_enabled, "SetInputEnabled(p, false) must DISABLE input");
            assert!(p.in_cinematic_mode(), "cinematic on");
            assert!(p.health_clamp);
            assert!(p.grapple_enabled);
            assert!(!p.in_pmc, "a gate nobody set stays at its resting value");
        }

        // ...and the getter agrees with the setter, which `InCinematicMode` could not do before (it
        // returned a hardcoded `false` while the setter wrote to a store nothing read).
        let cine: bool = sh.eval("return Player.InCinematicMode(Player.GetLocalPlayer())").unwrap();
        assert!(cine, "InCinematicMode must observe SetCinematicMode");

        // Cinematic mode is a COUNTER (`+0x1B4`), so one exit does not cancel two entries.
        sh.exec(
            "local p = Player.GetLocalPlayer() \
             Player.SetCinematicMode(p, true) Player.SetCinematicMode(p, false)",
            "@pl",
        )
        .unwrap();
        let cine: bool = sh.eval("return Player.InCinematicMode(Player.GetLocalPlayer())").unwrap();
        assert!(cine, "the outer cinematic entry is still active");
        sh.exec("Player.SetCinematicMode(Player.GetLocalPlayer(), false)", "@pl").unwrap();
        let cine: bool = sh.eval("return Player.InCinematicMode(Player.GetLocalPlayer())").unwrap();
        assert!(!cine, "matched exits clear it");
    }

    /// `SetAimMode`/`SetHealthClamp` must tolerate a **nil** handle: `hero.lua:42,109,424` calls
    /// `Player.SetAimMode(Player.GetSecondaryPlayer(), true)`, and there is no second player in
    /// single-player. Typed `f32`, these raised.
    #[test]
    fn game_lua_mode_gates_tolerate_a_nil_second_player() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

        assert!(
            sh.eval::<Option<i64>>("return Player.GetSecondaryPlayer()").unwrap().is_none(),
            "single-player has no second player"
        );
        // Must not raise, and must not touch the primary.
        sh.exec(
            "Player.SetAimMode(Player.GetSecondaryPlayer(), true) \
             Player.SetHealthClamp(Player.GetSecondaryPlayer(), true)",
            "@pl",
        )
        .expect("a nil handle is a silent no-op, not a Lua error");

        let h = host.borrow();
        let p = h.player().roster.local().unwrap();
        assert_eq!(p.aim_mode, 0, "the absent player's write must not land on the primary");
        assert!(!p.health_clamp);
    }

    /// The recorded-command bindings (record_all / sound_cmd / net_event) capture the game's calls as
    /// real intents AND emit `[bind]` app-log lines — the ground-truth that the surface is live.
    #[test]
    fn game_lua_recorded_bindings_capture_and_log() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);

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
        // The world's transit pads, from the VENDORED retail table — no vz.wad needed. They are the
        // archive's own records (`worldutil::landing_zone_pads_match_the_vendored_table` pins that),
        // so this host is corpus-only WITHOUT being world-less: `Pg.GetAllLandingZones` answers from
        // world data, and the Lua corpus contains no pads to answer from.
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids);
        register_landing_zones(&host, &world, &crate::worldutil::retail_landing_zone_pads());
        let Some(sh) = resident_script_host(host.clone()) else {
            eprintln!("[skip] decompiled Lua corpus not present — boot-flow regression skipped");
            return;
        };
        host.borrow_mut().set_boot_context("chris");
        run_boot_flow(&sh, &host, "chris");
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
        // KNOWN BLOCKER — and note it is *not* the paths this assertion's message names below.
        //
        // Since the `Player` surface and the HUD's retained callbacks became real, the boot runs far
        // deeper than it used to: `WifVzBoundary.SetupBoundary` → `MrxMissionFlow.Refresh` → the `Start`
        // binding's blocking sequence → the intro cinematic `01_AOA_C` → `HideSlow` → `UnlockMission`.
        // The widget-animation chain and the movie end callback both fire correctly now.
        //
        // Where it stops is `MrxTransit.SaveSingleton` (`mrxtransit.lua:367`), which does
        // `pairs(_tLandingZones)` without the `if not _tLandingZones` guard its siblings at :138/:151
        // carry. `_tLandingZones` is `false` because `Reset()` (:321-332) bails when
        // `Pg.GetAllLandingZones` comes back empty. In retail the world always has landing zones, so the
        // missing guard never bites.
        //
        // WHY IT IS STILL EMPTY *HERE*: this host is deliberately worldless (`GameScriptHost::new`, no
        // `attach_world`), and a world with no landing pads honestly has none to return. The pads
        // themselves are no longer missing — they are real, read from the `LandingZone` COMP
        // (`worldutil::landing_zone_pads` → `register_landing_zones`), and
        // `mrxtransit_resets_and_saves_against_real_landing_zones` below proves `Reset` +
        // `SaveSingleton` both run clean once a world supplies them. Supplying them needs the retail
        // vz.wad, which CI does not have; fabricating 46 pads to fake it would be inventing world data.
        //
        // AND IT IS NO LONGER THE LAST BLOCKER. `boot_flow_against_a_populated_world` runs this same
        // flow with the world's real contents behind it (every named placement + the transit pads) and
        // gets much further: 3308 `[lua]` lines, `MrxTransit.SaveSingleton` clean, all six
        // `vz_state_vzacon001*` layers fulfilled, `GlobalEnter - Complete`, and `STATE_WAITFORSTREAMING`
        // actually reaching refcount 0 once.
        //
        // Where THAT one parks is one link further on: `VzaCon001.StandardSetup` (`vz/vzacon001.lua:78`)
        // runs — its `Net.DoneReloadingLayers()` is the last binding call in the log — and arms
        // `Event.Create(Event.ObjectHibernation, {uBoat, "a"}, _PutPlayersInBoat, {uBoat})` on a boat
        // guid that now genuinely resolves (`vzacon001_boat_gate_arms_against_a_real_guid`). Nothing
        // exits the state until that event FIRES: the heroes get seated, `EnsureHeroesInBoat` calls
        // `AssetsLoaded` → `MrxMissionFlow._OnAssetsLoaded` (`:261-266`).
        //
        // Both producers now exist, and `boot_flow_against_a_populated_world` runs the load to
        // COMPLETION — GlobalExit, then the first mission live ("VZA001: Go to the Beach"):
        //
        //   1. `ObjectHibernation` — `worldutil::layer_index` (layers are ASET type 9, so
        //      `Vz_State_VzaCon001` is a plain archive lookup → block 179, the boat's own block) plus
        //      the pending-wake drain at the top of `pump_resident`.
        //   2. `ObjectInSeat` — `Vehicle.Enter`/`Exit` keep real occupancy on this host and queue
        //      transitions that the same pump fires.
        //
        // Together they close `vz/vzacon001.lua` end to end: boat wakes → `_PutPlayersInBoat` seats both
        // heroes → `EnsureHeroesInBoat` sees the last one in → `AssetsLoaded` →
        // `MrxMissionFlow._OnAssetsLoaded` (`:261-266`).
        //
        // (This note used to say "needs layer streaming — different system". That was wrong three times
        // over: the boat was a placement in a block `load_placements` already read, the layer that
        // brings it in was one ASET lookup away, and the seat event needed state this host could simply
        // keep. None of it needed new parsing or new data.)
        // NOT asserted here: `complete`. A worldless host structurally cannot reach GlobalExit — the
        // chain above dies on `Pg.GetAllLandingZones` returning empty, which is the honest answer for a
        // world with no pads. Asserting it made this a permanently-red test whose own comment explained
        // why it could never pass, which is worse than no assertion: a real regression would have been
        // indistinguishable from the standing failure.
        //
        // The completion expectation lives in `boot_flow_against_a_populated_world`, which supplies the
        // world and so can legitimately be held to it. What this test uniquely covers — that the corpus
        // boots deep against the real host with NO retail data present — is fully asserted above.
        println!("[boot] worldless host: lines={lines} layers={layers} complete={complete}");
    }

    /// The real `MrxTransit` boot path, end to end, against the REAL retail landing-zone data: `Reset()`
    /// builds `_tLandingZones` from `Pg.GetAllLandingZones(1)`/`(2)`, and `SaveSingleton()` — the call
    /// that ends the boot in `boot_flow_runs_real_game_lua` — returns a table instead of raising.
    ///
    /// This is the regression that pins the whole path: `mrxtransit.lua:367` iterates `_tLandingZones`
    /// with **none** of the `if not _tLandingZones` guards its siblings at `:138`/`:151` carry, so an
    /// empty `Pg.GetAllLandingZones` is a hard `bad argument #1 to 'for iterator'`. Retail never trips it
    /// because the world always has pads; the fix is to HAVE the pads, not to guard the shipped bug.
    ///
    /// SKIPS (passes) without the retail vz.wad or the decompiled Lua corpus.
    #[test]
    fn mrxtransit_resets_and_saves_against_real_landing_zones() {
        let Some(ls) = crate::worldutil::schema_wire_tests::retail_layers_static() else {
            return eprintln!("[skip] vz.wad not present — MrxTransit landing-zone test skipped");
        };
        let pads = crate::worldutil::landing_zone_pads(&ls);
        assert!(!pads.is_empty(), "retail layers_static must yield landing pads");

        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids.clone());
        register_landing_zones(&host, &world, &pads);

        let Some(sh) = resident_script_host(host.clone()) else {
            return eprintln!("[skip] decompiled Lua corpus not present — MrxTransit test skipped");
        };

        // Reset() bails early unless `Pg.GetAllLandingZones(1)` comes back non-empty (`:330-332`).
        sh.exec(r#"MrxTransit = import("MrxTransit") MrxTransit.Reset()"#, "@transit")
            .expect("MrxTransit.Reset runs");
        assert!(
            sh.eval::<bool>("return MrxTransit.IsSystemInitialized()").unwrap(),
            "Reset must have built _tLandingZones (it returns early on an empty zone list)"
        );

        // The crash site. Count the numeric keys: they are the landing-zone numbers, straight from the
        // iteration key of the list the binding returned.
        let zones: Vec<u32> = sh
            .eval(
                "local t = MrxTransit.SaveSingleton()\n\
                 local out = {}\n\
                 for k, v in pairs(t) do if type(k) == 'number' then out[#out+1] = k end end\n\
                 table.sort(out)\n\
                 return out",
            )
            .expect("MrxTransit.SaveSingleton must not raise once the world has landing zones");
        assert_eq!(
            zones,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 12, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 27, 28, 29, 30],
            "the save table must be keyed by absolute zone number, sparse, exactly as authored"
        );

        // `uLocation1`/`uLocation2` are the two player slots' pads — distinct objects, both addressable
        // as ordinary world objects (`Object.GetPosition`, `mrxtransit.lua:104`).
        let (d1, d2): (f32, f32) = sh
            .eval(
                "local a = Pg.GetAllLandingZones(1)\n\
                 local b = Pg.GetAllLandingZones(2)\n\
                 local x1, y1, z1 = Object.GetPosition(a[1])\n\
                 local x2, y2, z2 = Object.GetPosition(b[1])\n\
                 return math.abs(x1 - x2) + math.abs(z1 - z2), math.abs(x1) + math.abs(z1)",
            )
            .unwrap();
        assert!(d1 > 0.0, "a zone's two player pads are distinct positions");
        assert!(d2 > 0.0, "the pads report their authored world position, not the origin");
    }

    /// The boot flow against a FULLY POPULATED world: every named placement plus the transit pads.
    ///
    /// `boot_flow_runs_real_game_lua` runs the same flow against a deliberately worldless host, so it
    /// stays runnable wherever the corpus is checked out. This one is the same flow with the world's
    /// actual contents behind it, and is the test that can advance past the world-dependent gates.
    ///
    /// SKIPS (passes) without the retail vz.wad or the Lua corpus.
    #[test]
    fn boot_flow_against_a_populated_world() {
        let Some(ls) = crate::worldutil::schema_wire_tests::retail_layers_static() else {
            return eprintln!("[skip] vz.wad not present — populated-world boot skipped");
        };
        let Some(path) = crate::wad::resolve_vz_wad(None) else {
            return eprintln!("[skip] vz.wad path unavailable");
        };
        let Ok(mut wad) = crate::wad::open(&path) else { return eprintln!("[skip] vz.wad would not open") };
        let index = crate::worldutil::world_name_index(&mut wad, &ls);
        let pads = crate::worldutil::landing_zone_pads(&ls);

        let layers = crate::worldutil::layer_index(&mut wad);

        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids.clone());
        // Names first, pads second — a pad carrying a `Name` must reuse the one entity, not make a twin.
        register_named_markers(&host, &world, &index);
        register_landing_zones(&host, &world, &pads);
        // Layer index last: it wakes objects by name through the guidmap, so the names must resolve.
        host.borrow_mut().set_layer_index(layers);

        let Some(sh) = resident_script_host(host.clone()) else {
            return eprintln!("[skip] decompiled Lua corpus not present — populated-world boot skipped");
        };
        host.borrow_mut().set_boot_context("chris");
        run_boot_flow(&sh, &host, "chris");

        let (lines, complete, layers) = {
            let h = host.borrow();
            (h.lua_log_lines, h.world_load_complete, h.world_layers_loaded)
        };
        println!("[boot] populated world: lines={lines} layers={layers} complete={complete}");
        assert!(layers, "every streaming layer request must be fulfilled");

        // THE BOAT GATE FIRES. `VzaCon001.StandardSetup` arms
        // `Event.Create(Event.ObjectHibernation, {uBoat, "a"}, _PutPlayersInBoat, {uBoat})`
        // (`vz/vzacon001.lua:120`) and waits. The engine now supplies the missing producer: the
        // `vz_state_vzacon001*` layers complete, their objects wake, and `_PutPlayersInBoat` runs.
        //
        // `Net.SendEvent_ForceClientTether()` is that function's LAST statement (`:112-114`), so seeing
        // it recorded proves the whole body ran — players enumerated, characters seated via
        // `Vehicle.Enter`, seat events armed — not merely that the callback was entered.
        //
        // Asserted instead of the `[lua]` line count because `_PutPlayersInBoat` contains no
        // `Debug.Printf`: the boot genuinely advances here while `lines` does not move at all.
        assert!(
            host.borrow().net_events.iter().any(|(v, _)| v == "SendEvent_ForceClientTether"),
            "expected `_PutPlayersInBoat` to run to completion via the woken boat's \
             ObjectHibernation gate; it did not. Check that the `vz_state_vzacon001` layer resolved \
             (worldutil::layer_index), that the boat's name is in the guidmap \
             (worldutil::world_name_index), and that the pump still fires pending wakes BEFORE the \
             layer flush."
        );

        // ...and the mission actually STARTS. `AddPdaObjective` is issued by the objective system once
        // `VzaCon001` is running, which only happens after `EnsureHeroesInBoat` → `AssetsLoaded`. This is
        // the difference between "the load machine said done" and "the first mission is live": the boot
        // now reaches `VZA001: Go to the Beach`.
        assert!(
            host.borrow().net_events.iter().any(|(v, _)| v == "SendEvent_AddPdaObjective"),
            "expected the first mission objective to be posted once VzaCon001 started; it was not — \
             the seat chain (`EnsureHeroesInBoat` → `AssetsLoaded`) did not complete"
        );

        // THE WHOLE WORLD LOAD COMPLETES. loadprobe phase 20 — GlobalEnter, act staging, mission-flow
        // init, WaitForStreaming, and the `WifMissionFlow.Refresh → Exit(WAITFORGAME)` that reaches
        // GlobalExit ("world fully loaded").
        //
        // This was a tracked frontier rather than an assertion until the two producers landed: the
        // `ObjectHibernation` wake (`worldutil::layer_index` + the pending-wake drain) and the
        // `ObjectInSeat` fire (`Vehicle.Enter` → `take_pending_seat_events`). The chain it unblocks is
        // `vz/vzacon001.lua` end to end — boat wakes → `_PutPlayersInBoat` seats both heroes →
        // `EnsureHeroesInBoat` sees the last one in → `AssetsLoaded` → `MrxMissionFlow._OnAssetsLoaded`
        // (`:261-266`).
        assert!(
            complete,
            "the world-load state machine must reach GlobalExit - Complete; it did not ({lines} `[lua]` \
             lines). Check, in order: the boat wakes (`worldutil::layer_index` resolved \
             `vz_state_vzacon001`, `world_name_index` has the boat), `_PutPlayersInBoat` ran \
             (SendEvent_ForceClientTether below), and the seat events fired \
             (`Vehicle.Enter` → `pump_resident`'s seat drain → `EnsureHeroesInBoat` → `AssetsLoaded`)."
        );
        // A completed load must still have run the game's Lua deep — `complete` alone could in
        // principle be reached by a state machine that skipped the content.
        assert!(lines > 3_000, "a real load runs the game's Lua deep; got {lines} `[lua]` lines");
    }

    /// A REAL retail save, parsed: the vendored chris 0%-completion (pre-PMC-takeover) profile.
    ///
    /// Read from the actual `.profile` rather than reconstructed, so this is the whole save — flow
    /// keys, transit blob and all — not just the parts a log happens to print. The same save is
    /// visible in `game-files/pmc_blackbox-chris-save-0-percent-pre-pmc-takeover.log`, and the two
    /// agree on every field the capture shows:
    ///
    /// ```text
    /// [lua] Culling binding "Start"        @mrxmissionflow:1079   -> flow_chain ["Start", "VzaCon001"]
    /// [lua] Culling binding "VzaCon001"    @mrxmissionflow:1079
    /// [lua] -- sSelectedMission = PmcCon001                       -> active_missions ["PmcCon001"]
    /// [lua]   ----=== # ... save data: 250  @mrxlayermanager:560  -> 250 layers
    /// [lua] SetSystemEnabled( false, nil, nil  @mrxtransit:418    -> transit_enabled false
    /// ```
    fn retail_resume_save() -> Option<BootSaveState> {
        let path = mercs2_formats::game_paths::save_fixtures().join("Chris Jacobs_6A499ED6.profile");
        let bytes = std::fs::read(path).ok()?;
        let profile = mercs2_formats::save::parse(&bytes).ok()?;
        let lua = profile.decompress_lua().ok()?;
        let s = mercs2_formats::save::parse_save_state(&String::from_utf8_lossy(&lua)).ok()?;
        Some(BootSaveState {
            flow_keys: s.completed_flow.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            culled_bindings: s.flow_chain.clone(),
            active_missions: s.active_missions.iter().map(|m| m.id.clone()).collect(),
            retry_locations: s.retry_locations.clone(),
            layers: s.layers.clone(),
            transit_enabled: s.transit_enabled,
            transit_zones: s.transit_zones.clone(),
        })
    }

    /// **The RESUME counterpart to [`boot_flow_against_a_populated_world`].**
    ///
    /// That test drives the NEW-GAME branch (`Pg.LoadGame` false → `VzaCon001_Start1`). Every retail
    /// capture we have is a save RESUME, so this is the like-for-like: same populated world, but with
    /// a save installed so `xQ!L.LoadSingleton` takes a resume branch.
    ///
    /// The save is measured from the chris 0% capture ([`retail_resume_save`]) rather than invented. That
    /// capture is a **mid-contract VzaCon001** save (`tRetryLocations = {"PmcCon001_Start1"}`), so it is a
    /// PRE-PMC resume: the master script must spawn the hero at the contract CHECKPOINT marker, NOT at
    /// `Pmc_Entry1` (the sea-level HQ entrance — landing there drops a pre-PMC hero in the water). The
    /// post-PMC / hub resume that DOES reach `Pmc_Entry1` is covered by
    /// [`new_game_and_resume_take_different_boot_branches`].
    ///
    /// This is also what gives the two worldless boot tests their landing zones: `MrxTransit.Reset`
    /// bails when `Pg.GetAllLandingZones` is empty, leaving `_tLandingZones = false` for
    /// `SaveSingleton` to iterate. With the real pads registered, `Reset` completes exactly as all
    /// three retail captures show it doing (each reaches `@mrxtransit:563`, past the population loop).
    ///
    /// SKIPS (passes) without the retail vz.wad or the Lua corpus.
    #[test]
    fn boot_flow_resume_against_a_populated_world() {
        let Some(ls) = crate::worldutil::schema_wire_tests::retail_layers_static() else {
            return eprintln!("[skip] vz.wad not present — populated-world resume skipped");
        };
        let Some(path) = crate::wad::resolve_vz_wad(None) else {
            return eprintln!("[skip] vz.wad path unavailable");
        };
        let Ok(mut wad) = crate::wad::open(&path) else {
            return eprintln!("[skip] vz.wad would not open");
        };
        let index = crate::worldutil::world_name_index(&mut wad, &ls);
        let pads = crate::worldutil::landing_zone_pads(&ls);
        let layers = crate::worldutil::layer_index(&mut wad);

        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids.clone());
        register_named_markers(&host, &world, &index);
        register_landing_zones(&host, &world, &pads);
        host.borrow_mut().set_layer_index(layers);

        // THE difference from the new-game test: a save is installed, so `Pg.LoadGame` answers true.
        let Some(save) = retail_resume_save() else {
            return eprintln!("[skip] save fixture unreadable — populated-world resume skipped");
        };
        let save_zone_count = save.transit_zones.len();
        host.borrow_mut().set_boot_save_state(Some(save));

        let Some(sh) = resident_script_host(host.clone()) else {
            return eprintln!("[skip] decompiled Lua corpus not present — populated-world resume skipped");
        };
        host.borrow_mut().set_boot_context("chris");
        run_boot_flow(&sh, &host, "chris");

        // The mid-contract resume branch was taken. `xQ!L.LoadSingleton` (`:645-652`) picks the save's
        // `tRetryLocations` checkpoint marker when the save carries them, rather than
        // `{"Pmc_Entry1", "Pmc_Entry2"}` (a hub save) or `VzaCon001_Start1` (a new game).
        let marker: Option<String> = sh
            .exec(
                "__resume_marker = MrxPlayer and MrxPlayer._tSpawnLocations and MrxPlayer._tSpawnLocations[1]",
                "@probe",
            )
            .ok()
            .and_then(|()| sh.lua().globals().get::<Option<String>>("__resume_marker").ok().flatten());
        assert_eq!(
            marker.as_deref(),
            Some("PmcCon001_Start1"),
            "a mid-contract (pre-PMC) resume must spawn at the save's tRetryLocations checkpoint marker"
        );
        assert_ne!(
            marker.as_deref(),
            Some("Pmc_Entry1"),
            "a pre-PMC resume must NOT take the PMC HQ-entrance path — that is the Y=0 sea-level marker \
             that drops the hero in the water"
        );

        // `MrxTransit.Reset` completed, so the shipped `SaveSingleton` bug cannot fire. Asserted
        // through the Lua rather than our own index: what matters is what the SCRIPT ended up with.
        let zones: Option<i64> = sh
            .exec(
                "__zone_count = 0\n\
                 if MrxTransit and type(MrxTransit._tLandingZones) == \"table\" then\n\
                 for _ in pairs(MrxTransit._tLandingZones) do __zone_count = __zone_count + 1 end\n\
                 end",
                "@probe",
            )
            .ok()
            .and_then(|()| sh.lua().globals().get::<Option<i64>>("__zone_count").ok().flatten());
        assert_eq!(
            zones,
            Some(23),
            "MrxTransit.Reset must populate all 23 authored zones (22 affiliated + the zone-6 bFake \
             pad); see worldutil's retail_capture_corroborates_the_authored_landing_zone_set"
        );

        // THE SAVE'S TRANSIT BLOB REACHED THE SCRIPT. `tTransitData` used to be handed over as an
        // empty table, so a resumed game came back with every zone at its `Reset` default. The save
        // carries all 23; `MrxTransit.LoadSingleton` must have applied them.
        assert_eq!(save_zone_count, 23, "the vendored save carries the full authored zone set");
        let restored: Option<i64> = sh
            .exec(
                "__restored = 0\n\
                 if MrxTransit and type(MrxTransit._tLandingZones) == \"table\" then\n\
                 for _, z in pairs(MrxTransit._tLandingZones) do\n\
                 if z.bEnabled ~= nil then __restored = __restored + 1 end\n\
                 end end",
                "@probe",
            )
            .ok()
            .and_then(|()| sh.lua().globals().get::<Option<i64>>("__restored").ok().flatten());
        assert_eq!(
            restored,
            Some(23),
            "every zone in the save's tTransitData must land on `_tLandingZones`; an empty blob \
             leaves them at the Reset default and this reads 0"
        );

        let lines = host.borrow().lua_log_lines;
        println!("[boot] populated-world RESUME: {lines} `[lua]` lines, spawn {marker:?}");
        assert!(lines > 1_000, "a real resume runs the game's Lua deep; got {lines} `[lua]` lines");
    }

    /// `VzaCon001`'s boat gate arms against a REAL guid, through the real binding.
    ///
    /// This is the acceptance test for the whole name-index path. `VzaCon001.StandardSetup`
    /// (`vz/vzacon001.lua:66-119`) does `Event.ObjectHibernation(Pg.GetGuidByName(...), "a")` and waits;
    /// with the boat resolving to nil the boot parked there forever, and the note in
    /// `boot_flow_runs_real_game_lua` used to call this "layer streaming, different system". It was not —
    /// the boat is a placement in block 179 that `load_placements` already read; we were only indexing
    /// block 29. Nothing was missing but the identification.
    ///
    /// SKIPS (passes) without the retail vz.wad or the Lua corpus.
    #[test]
    fn vzacon001_boat_gate_arms_against_a_real_guid() {
        let Some(ls) = crate::worldutil::schema_wire_tests::retail_layers_static() else {
            return eprintln!("[skip] vz.wad not present — VzaCon001 boat-gate test skipped");
        };
        let Some(path) = crate::wad::resolve_vz_wad(None) else {
            return eprintln!("[skip] vz.wad path unavailable");
        };
        let Ok(mut wad) = crate::wad::open(&path) else { return eprintln!("[skip] vz.wad would not open") };
        let index = crate::worldutil::world_name_index(&mut wad, &ls);

        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids.clone());
        register_named_markers(&host, &world, &index);

        // The binding — not the index — must answer. This is the path `vzacon001.lua` actually takes.
        let guid = host.borrow_mut().guid_by_name("VzaCon001_StartingBoat");
        assert_ne!(guid, 0, "Pg.GetGuidByName must resolve the boat once streamed layers are indexed");

        let Some(sh) = resident_script_host(host.clone()) else {
            return eprintln!("[skip] decompiled Lua corpus not present — boat-gate test skipped");
        };

        // Through Lua, as a lightuserdata guid, and reaching the same object: the boat answers
        // `Object.GetPosition` at its authored spot, so the gate is arming on a real world object rather
        // than on a handle that merely happens to be non-nil.
        let (x, y, z): (f32, f32, f32) = sh
            .eval(
                "local u = Pg.GetGuidByName(\"VzaCon001_StartingBoat\")\n\
                 assert(u ~= nil, \"boat guid is nil in Lua\")\n\
                 assert(type(u) == \"userdata\", \"guids reach shipped scripts as userdata\")\n\
                 return Object.GetPosition(u)",
            )
            .expect("the boat resolves and is positionable through the shipped binding surface");
        assert!(
            (x - -1726.98).abs() < 1.0 && (y - -36.35).abs() < 1.0 && (z - 2068.80).abs() < 1.0,
            "the boat's authored block-179 position; got ({x}, {y}, {z})"
        );

        // And the gate itself. This is `vzacon001.lua:120` verbatim in shape —
        // `Event.Create(Event.ObjectHibernation, {uBoat, "a"}, _PutPlayersInBoat, {uBoat})` — the call
        // that used to be handed a nil `uBoat`. `Event` is a global namespace, not an importable module.
        sh.exec(
            "local uBoat = Pg.GetGuidByName(\"VzaCon001_StartingBoat\")\n\
             _hEvent = Event.Create(Event.ObjectHibernation, {uBoat, \"a\"}, function() _woke = true end, {uBoat})",
            "@boatgate",
        )
        .expect("Event.Create arms ObjectHibernation on the boat guid");
        assert!(
            sh.eval::<bool>("return _hEvent ~= nil").unwrap(),
            "Event.Create must hand back a handle — the mission holds it to cancel the gate later"
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
        install_guid_helper(&sh);

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
        pump_resident(&sh, &host, 0.1);
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
        pump_resident(&sh, &host, 1.0 / 60.0);
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

    /// `install_boot_save_state` publishes exactly the table `xQ!L.LoadSingleton` reads — and publishes
    /// **nothing** for a new game, because "no save" is the signal that picks the new-game branch.
    #[test]
    fn boot_save_state_publishes_the_flow_tables() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let Some(sh) = resident_script_host(host.clone()) else {
            return eprintln!("[skip] decompiled Lua corpus not present");
        };

        // New game: the global is nil and the installer reports "not resuming".
        assert!(!install_boot_save_state(&sh, &host).unwrap(), "no save = new game");
        sh.exec("assert(__boot_save_state == nil, 'a new game must publish no save table')", "@t").unwrap();

        // Resuming AT THE HUB (no retry locations): the flow tables land in the shape
        // `MrxMissionFlow.LoadSingleton` destructures, and `tRetryLocations` is NIL so `LoadSingleton`
        // takes its `{Pmc_Entry1,Pmc_Entry2}` + `_bPmcRequired` path.
        host.borrow_mut().set_boot_save_state(Some(BootSaveState {
            flow_keys: vec![("VzaCon001".into(), 1.0), ("PmcCon001".into(), 1.0)],
            culled_bindings: vec!["Start".into(), "VzaCon001".into()],
            active_missions: vec!["OilCon020".into()],
            retry_locations: Vec::new(),
            layers: vec!["vz_state_pmcinterior".into()],
            transit_enabled: false,
            transit_zones: Vec::new(),
        }));
        assert!(install_boot_save_state(&sh, &host).unwrap(), "a save = resume");
        sh.exec(
            "local s = __boot_save_state\n\
             assert(s, 'save table must be published')\n\
             assert(s.tFlowData.tMyFlowData.VzaCon001 == 1, 'HasKey(\"VzaCon001\") must be true')\n\
             assert(s.tFlowData.tCulledBindings[1] == 'Start', 'culled bindings are a 1-based array')\n\
             assert(s.tFlowData.tCulledBindings[2] == 'VzaCon001', 'order preserved')\n\
             assert(type(s.tFlowData.tActiveMissions.OilCon020) == 'table', 'active missions keyed by id')\n\
             assert(s.tLayerData[1] == 'vz_state_pmcinterior', 'layer overlays carried')\n\
             assert(s.tRetryLocations == nil, 'a hub save must publish NO tRetryLocations (Pmc_Entry1 path)')",
            "@t",
        )
        .unwrap();

        // Resuming MID-CONTRACT (retry locations present): `tRetryLocations` is published as a 1-based
        // marker array so `LoadSingleton` takes its in-world checkpoint branch instead of `Pmc_Entry1`.
        host.borrow_mut().set_boot_save_state(Some(BootSaveState {
            flow_keys: vec![("VzaCon001".into(), 1.0)],
            culled_bindings: vec!["Start".into()],
            retry_locations: vec!["Checkpoint_PMC001_VillaReached".into()],
            ..Default::default()
        }));
        assert!(install_boot_save_state(&sh, &host).unwrap(), "a mid-contract save = resume");
        sh.exec(
            "local s = __boot_save_state\n\
             assert(s, 'save table must be published')\n\
             assert(type(s.tRetryLocations) == 'table', 'a mid-contract save must publish tRetryLocations')\n\
             assert(s.tRetryLocations[1] == 'Checkpoint_PMC001_VillaReached', 'checkpoint marker carried, 1-based')",
            "@t",
        )
        .unwrap();
    }

    /// **The regression this whole change exists for.** New Game and Continue must take DIFFERENT boot
    /// branches in `xQ!L.LoadSingleton`, and therefore start the hero at different markers:
    ///
    /// * new game → `VzaCon001_Start1` — the opening contract, before the player owns the PMC
    /// * resuming → `Pmc_Entry1` — the PMC HQ entrance
    ///
    /// Previously BOTH landed in the PMC interior, because the boot chunk called
    /// `MrxPlayer.SetSpawnLocations({"<contract>_Start1"})` right after the master script had already
    /// decided, overwriting the answer. Asserting on `MrxPlayer._tSpawnLocations` pins the master
    /// script's decision itself, upstream of any world/marker resolution.
    /// SKIPS (passes) without the retail vz.wad — see the world-data note inside.
    #[test]
    fn new_game_and_resume_take_different_boot_branches() {
        // WHY THIS ONE NEEDS THE ARCHIVE. The RESUME branch enters the PMC HQ interior, and
        // `WifPmcInterior._EnablePortals` (`vz/wifpmcinterior.lua:1000-1006`) resolves each portal by
        // NAME — `Pg.GetGuidByName(tPortalData.sExterior_Entrance)` — then indexes `_tPortals[uGuid]`
        // unguarded, so an unresolved name is a hard `table index is nil`.
        //
        // Those names are the world's whole named object graph (10,290 placements, ~345 KB), not a
        // bounded table like the 46 landing pads that `retail_landing_zone_pads` vendors. Extracting
        // the pads is a specific record set; extracting this would be redistributing the world, which
        // this repo deliberately does not do. So it is read from the archive, and the test skips
        // without one — the convention every other world-dependent test here follows.
        let world_data = (|| {
            let ls = crate::worldutil::schema_wire_tests::retail_layers_static()?;
            let path = crate::wad::resolve_vz_wad(None)?;
            let mut wad = crate::wad::open(&path).ok()?;
            Some(crate::worldutil::world_name_index(&mut wad, &ls))
        })();
        let Some(names) = world_data else {
            return eprintln!("[skip] vz.wad not present — boot-branch test skipped");
        };

        // The marker name the master script settled on, for a given boot save state.
        let spawn_marker_for = |save: Option<BootSaveState>| -> Option<String> {
            let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
            let world = Rc::new(RefCell::new(World::new()));
            let guids = Rc::new(RefCell::new(GuidMap::new()));
            host.borrow_mut().attach_world(world.clone(), guids);
            // Names first, pads second: a pad carrying a `Name` must reuse the one entity, not twin it.
            register_named_markers(&host, &world, &names);
            // Real transit pads from the vendored retail table — `MrxTransit.Reset` needs them on
            // BOTH branches, and the resume branch reaches `SaveSingleton` through `UnlockMission`.
            register_landing_zones(&host, &world, &crate::worldutil::retail_landing_zone_pads());
            host.borrow_mut().set_boot_save_state(save);
            let sh = resident_script_host(host.clone())?;
            host.borrow_mut().set_boot_context("mattias");
            run_boot_flow(&sh, &host, "mattias");
            sh.exec(
                "__test_marker = MrxPlayer and MrxPlayer._tSpawnLocations and MrxPlayer._tSpawnLocations[1]",
                "@probe",
            )
            .ok()?;
            sh.lua().globals().get::<Option<String>>("__test_marker").ok().flatten()
        };

        let Some(new_game) = spawn_marker_for(None) else {
            return eprintln!("[skip] decompiled Lua corpus not present — boot-branch test skipped");
        };
        // POST-PMC / hub resume: a save with NO retry locations falls through to the PMC HQ entrance.
        let resumed_hub = spawn_marker_for(Some(BootSaveState {
            flow_keys: vec![("VzaCon001".into(), 1.0), ("PmcCon001".into(), 1.0)],
            culled_bindings: vec!["Start".into(), "VzaCon001".into()],
            ..Default::default()
        }))
        .expect("the corpus was present a moment ago");
        // PRE-PMC / mid-contract resume: a save WITH retry locations spawns at its checkpoint marker,
        // NOT at Pmc_Entry1. This is the case the water-spawn bug lived in.
        let resumed_midcontract = spawn_marker_for(Some(BootSaveState {
            flow_keys: vec![("VzaCon001".into(), 1.0)],
            culled_bindings: vec!["Start".into()],
            retry_locations: vec!["Checkpoint_PMC001_VillaReached".into()],
            ..Default::default()
        }))
        .expect("the corpus was present a moment ago");

        println!(
            "[boot-branch] new game -> {new_game}   resume(hub) -> {resumed_hub}   \
             resume(mid-contract) -> {resumed_midcontract}"
        );
        assert_eq!(
            new_game, "VzaCon001_Start1",
            "a NEW GAME must start at the opening contract (vz/xQ!L.lua:665-670 + \
             wifmissiondata.lua:766), not inside the PMC the player does not own yet"
        );
        assert_eq!(
            resumed_hub, "Pmc_Entry1",
            "RESUMING a hub save (no tRetryLocations) must start at the PMC HQ entrance (vz/xQ!L.lua:650-652)"
        );
        assert_eq!(
            resumed_midcontract, "Checkpoint_PMC001_VillaReached",
            "RESUMING a mid-contract save must start at its tRetryLocations checkpoint (vz/xQ!L.lua:645-648)"
        );
        assert_ne!(
            resumed_midcontract, "Pmc_Entry1",
            "a pre-PMC resume must NOT be diverted to the PMC HQ entrance"
        );
        assert_ne!(new_game, resumed_hub, "the new-game and hub-resume branches must not collapse into one");
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

    /// Lua `Object.*Health` and the combat system read/write the SAME `Health` component on a live entity —
    /// no divergence. The old shadow HashMap and the combat `Health` were disjoint; now Lua damage is
    /// visible to combat and vice-versa.
    #[test]
    fn health_binding_shares_the_combat_health_component() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let world = Rc::new(RefCell::new(World::new()));
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        host.borrow_mut().attach_world(world.clone(), guids.clone());
        // A combat entity carrying a Health component (as the resolver / streaming would spawn it).
        let e = world.borrow_mut().spawn((Transform::IDENTITY, mercs2_core::Health::new(100.0)));
        let g = 0x1000_5000u64;
        host.borrow().register_entity(e, g, None);

        assert_eq!(host.borrow().object_health(g), 100.0);
        // Lua damage writes the SAME component the combat system reads.
        assert!(!host.borrow_mut().object_send_damage(g, 30.0));
        assert_eq!(host.borrow().object_health(g), 70.0);
        assert_eq!(world.borrow().get::<&mercs2_core::Health>(e).unwrap().cur, 70.0, "combat sees the Lua damage");
        // Kill via Lua → combat sees dead.
        host.borrow_mut().object_kill(g);
        assert!(!host.borrow().object_is_alive(g));
        assert!(world.borrow().get::<&mercs2_core::Health>(e).unwrap().is_dead());
    }

    /// The economy round-trips through the profile singleton, in the **signed-i32 domain with no
    /// native caps**.
    ///
    /// This test previously asserted a 1-billion cash clamp and a fuel-to-capacity clamp. Both were
    /// inventions: `economy_cash_fuel_singleton.md` shows the setters store a raw dword (native ceiling
    /// `i32::MAX`), and the limits are **Lua** soft-clamps in `MrxPmc` — which `mrxpmc.lua:474,538`
    /// bypass by calling `Player.AddCash`/`SetCash` directly. Clamping natively made those bypasses
    /// unobservable.
    #[test]
    fn player_economy_round_trips_in_the_i32_domain() {
        let mut h = GameScriptHost::new("vz");
        assert_eq!(h.player().profile.cash, 0);
        h.set_cash(50_000);
        assert_eq!(h.player().profile.cash, 50_000);

        h.player_mut().profile.set_cash(2_000_000_000, false);
        assert_eq!(h.player().profile.cash, 2_000_000_000, "no native 1e9 clamp");

        h.player_mut().profile.set_fuel(500, false);
        assert_eq!(h.player().profile.fuel, 500);
        h.player_mut().profile.set_fuel_capacity(100);
        assert_eq!(h.player().profile.fuel, 500, "capacity does not natively clamp current fuel");
        h.player_mut().profile.set_fuel(150, false);
        assert_eq!(h.player().profile.fuel, 150, "nor does it clamp a later write");
    }

    /// Seeding cash from the save leaves the profile **un-autosaved** — `SetCash` is one of the five
    /// setters that never OR the dirty flag `+0x11`, which gates `autoSave` (`FUN_00614540`).
    /// A shipped bug, reproduced deliberately; the fix is queued in `mercs2_player/DEFERRED.md`.
    #[test]
    fn seeding_cash_does_not_arm_the_autosave() {
        let mut h = GameScriptHost::new("vz");
        h.set_cash(50_000);
        assert!(!h.player().profile.autosave_due(), "the shipped autosave bug, observable");

        // ...whereas a fuel change does dirty it, so the flag itself works.
        h.player_mut().profile.set_fuel(10, false);
        assert!(h.player().profile.autosave_due());
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

#[cfg(test)]
mod seat_tests {
    use super::*;

    /// Mint a handle from a literal, for tests that need a **known** guid on both sides — e.g.
    /// asserting `host.faction.accumulator(777)` after driving Lua that names 777.
    ///
    /// Scripts never do this. The engine hands handles out (`Pg.GetGuidByName`,
    /// `Player.GetLocalCharacter`) and they cross as lightuserdata; `mercs2_script::Guid` refuses to
    /// read one out of a number, because this VM's `lua_Number` is f32 and cannot carry a handle
    /// above 2^24 without aliasing a different object. These tests used to pass bare integers and
    /// relied on a transitional arm that has since been removed. Every literal below is small enough
    /// to be exact in f32, so minting one here is a faithful stand-in for an engine-supplied handle.
    fn install_guid_helper(sh: &ScriptHost) {
        let f = sh
            .lua()
            .create_function(|_, n: f64| {
                // The literal arrives through a Lua number, i.e. f32. Above 2^24 that silently
                // rounds — `__guid(0x1000_0001)` would mint 0x1000_0000 and quietly fail to match
                // the handle the host fires with. Refuse instead: a test needing a real-range guid
                // must use `set_guid`, which never crosses a number.
                assert!(
                    n.abs() < (1u64 << 24) as f64,
                    "__guid({n}) is beyond f32's exact integer range — use set_guid() instead"
                );
                Ok(mercs2_script::Guid(n as u64))
            })
            .unwrap();
        sh.lua().globals().set("__guid", f).unwrap();
    }

    /// Bind a handle to a Lua global directly, without it ever being a number.
    ///
    /// This is how the engine really gives a script a handle, and the only way to carry one at or
    /// above 2^24 — which every dynamic guid is (`mercs2_core::FIRST_DYNAMIC_GUID` = 2^28).
    #[allow(dead_code)]
    fn set_guid(sh: &ScriptHost, name: &str, g: u64) {
        sh.lua().globals().set(name, mercs2_script::Guid(g)).unwrap();
    }

    fn host_with_script() -> (Rc<RefCell<GameScriptHost>>, ScriptHost) {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let sh = ScriptHost::bare().unwrap();
        sh.register_engine(host.clone()).unwrap();
        install_guid_helper(&sh);
        (host, sh)
    }

    /// `Vehicle.Enter` establishes real state that `Object.InSeat` and `Vehicle.GetSeatFromRider` read
    /// back, and `Vehicle.Exit` clears it. Before this, `Enter` returned false and changed nothing.
    #[test]
    fn enter_and_exit_round_trip_through_the_bindings() {
        let (host, sh) = host_with_script();
        let (veh, rider) = (0x1000_0001u64, 0x1000_0002u64);
        // Real-range handles: bound as lightuserdata, the way the engine hands them to a script.
        set_guid(&sh, "uVeh", veh);
        set_guid(&sh, "uRider", rider);

        assert!(!sh.eval::<bool>(&format!("return Object.InSeat(uRider)")).unwrap(), "starts unseated");
        assert!(sh.eval::<bool>(&format!("return Vehicle.Enter(uVeh, uRider, \"d\")")).unwrap());
        assert!(sh.eval::<bool>(&format!("return Object.InSeat(uRider)")).unwrap(), "seated");
        assert!(sh.eval::<bool>(&format!("return Object.InVehicle(uRider)")).unwrap(), "same state");
        assert_eq!(
            sh.eval::<String>(&format!("return Vehicle.GetSeatFromRider(uRider)")).unwrap(),
            "d"
        );
        assert_eq!(host.borrow().riders_of(veh), vec![(rider, "d".to_string())]);

        assert!(sh.eval::<bool>(&format!("return Vehicle.Exit(uRider)")).unwrap());
        assert!(!sh.eval::<bool>(&format!("return Object.InSeat(uRider)")).unwrap(), "unseated");
        assert!(host.borrow().riders_of(veh).is_empty());
        // Exiting a rider who is not seated is false, not a panic or a phantom event.
        assert!(!sh.eval::<bool>(&format!("return Vehicle.Exit(uRider)")).unwrap());
    }

    /// A rider moved to another vehicle emits the EXIT for the old seat before the enter — so a handler
    /// watching the vehicle they left actually sees them leave, and the rider is never recorded in two
    /// seats at once.
    #[test]
    fn moving_seats_emits_the_exit_first() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let (car, boat, rider) = (0x1000_0001u64, 0x1000_0003u64, 0x1000_0002u64);
        {
            let mut h = host.borrow_mut();
            h.vehicle_enter(car, rider, "d");
            h.vehicle_enter(boat, rider, "p");
        }
        let evs = host.borrow_mut().take_pending_seat_events();
        let shape: Vec<(u64, &str)> = evs.iter().map(|(_, v, _, a)| (*v, *a)).collect();
        assert_eq!(
            shape,
            vec![(car, "e"), (car, "x"), (boat, "e")],
            "enter car, then LEAVE car, then enter boat — in that order"
        );
        assert_eq!(host.borrow().riders_of(car), vec![], "no longer in the car");
        assert_eq!(host.borrow().riders_of(boat).len(), 1);
    }

    /// The filter wildcards from the corpus: vehicle `0` = any vehicle, seat `"a"` = any seat, and a
    /// non-guid occupant (the string `"Hero"`) = any occupant. Case is ignored on seat and action.
    #[test]
    fn in_seat_filter_wildcards_match() {
        let (host, sh) = host_with_script();
        let (veh, rider) = (0x1000_0001u64, 0x1000_0002u64);
        // Real-range handles: bound as lightuserdata, the way the engine hands them to a script.
        set_guid(&sh, "uVeh", veh);
        set_guid(&sh, "uRider", rider);
        // `{uCharacter, 0, "d", "x"}` — wifpmcgarage.lua:472: this character leaving the driver seat of
        // ANY vehicle. Plus an any-seat and an any-occupant registration.
        sh.exec(
            &format!(
                "_hits = {{}}\n\
                 Event.Create(Event.ObjectInSeat, {{uRider, 0, \"d\", \"x\"}}, function() _hits[#_hits+1]=\"anyveh\" end)\n\
                 Event.Create(Event.ObjectInSeat, {{uRider, uVeh, \"a\", \"E\"}}, function() _hits[#_hits+1]=\"anyseat\" end)\n\
                 Event.Create(Event.ObjectInSeat, {{\"Hero\", uVeh, \"D\", \"e\"}}, function() _hits[#_hits+1]=\"anyocc\" end)"
            ),
            "@seatfilter",
        )
        .unwrap();

        // An ENTER into the driver seat: the any-seat and any-occupant filters match; the exit filter
        // must not (wrong action), even though its vehicle wildcard would otherwise accept.
        sh.fire_object_in_seat(rider, veh, "d", "e").unwrap();
        let mut hits: Vec<String> = sh.eval("return _hits").unwrap();
        hits.sort();
        assert_eq!(hits, vec!["anyocc", "anyseat"], "action must still discriminate");

        // Now the exit, from a DIFFERENT vehicle: the `0` vehicle wildcard accepts it.
        sh.exec("_hits = {}", "@r").unwrap();
        sh.fire_object_in_seat(rider, 0x9999, "d", "x").unwrap();
        assert_eq!(sh.eval::<Vec<String>>("return _hits").unwrap(), vec!["anyveh"]);
        let _ = host;
    }

    /// The callback receives its registered `cbargs` FOLLOWED BY `(occupant, vehicle)`.
    ///
    /// Pinned against the shipped signature that proves the order:
    /// `_OnVehicleExit(vRegion, nSlot, uCharacter, uVehicle)` registered with `{vRegion, nSlot}`
    /// (`wifpmcgarage.lua:470-523`). Getting this backwards would hand every seat handler its arguments
    /// transposed, which Lua would not complain about.
    #[test]
    fn the_callback_gets_cbargs_then_occupant_then_vehicle() {
        let (_host, sh) = host_with_script();
        let (veh, rider) = (0x1000_0001u64, 0x1000_0002u64);
        // Real-range handles: bound as lightuserdata, the way the engine hands them to a script.
        set_guid(&sh, "uVeh", veh);
        set_guid(&sh, "uRider", rider);
        sh.exec(
            &format!(
                "Event.Create(Event.ObjectInSeat, {{uRider, uVeh, \"d\", \"e\"}},\n\
                 function(a, b, uChar, uVeh)\n\
                   _got = {{a, b, tostring(type(uChar)), tostring(type(uVeh))}}\n\
                 end, {{\"region\", 7}})"
            ),
            "@seatargs",
        )
        .unwrap();
        sh.fire_object_in_seat(rider, veh, "d", "e").unwrap();
        let got: Vec<String> = sh.eval("return {tostring(_got[1]), tostring(_got[2]), _got[3], _got[4]}").unwrap();
        assert_eq!(
            got,
            vec!["region", "7", "userdata", "userdata"],
            "cbargs first, then the two handles — and handles reach scripts as userdata"
        );
    }
}
