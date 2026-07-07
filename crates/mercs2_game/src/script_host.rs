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

use mercs2_audio::{AudioEngine, VoiceId};
use mercs2_script::{EngineHost, ScriptHost};

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
/// to realize. Holds no GPU/ECS state — deliberately, so it can live behind the VM's `RefCell`.
pub struct GameScriptHost {
    pub spawns: Vec<SpawnRequest>,
    by_name: HashMap<String, u64>,
    by_guid: HashMap<u64, usize>,
    next_guid: u64,
    level: String,
    /// The live audio system the game's `Sound.*` / music Lua drives. **Shared** (`Rc<RefCell>`) so the
    /// game loop ticks the SAME engine each frame (`GameplaySystems::tick` → `audio.tick`) that the Lua
    /// `EngineHost` forwarding cues into — one `mercs2_audio` stack, driven from both sides.
    audio: Rc<RefCell<AudioEngine>>,
    /// The AI mechanism the game's `Ai.*` Lua drives: the recovered 1024-slot action ring + the
    /// `[-100,100]` relation matrix (`mercs2_ai::AiWorld`, AI code map §8). `Ai.Goal` posts to the ring;
    /// `Ai.SetRelation`/`GetRelation` read/write the matrix. Per-entity perception records are ticked
    /// over the ECS world by the runtime, not here.
    ai: mercs2_ai::AiWorld,
    /// Per-actor `AiBehavior` restriction flags set by `Ai.SetState` (keyed by actor GUID).
    ai_states: std::collections::HashMap<u64, mercs2_ai::AiBehavior>,
    /// The faction/reputation manager the game's `Ai.AddInfraction`/`SetInfractionMultiplier`/attitude
    /// Lua drives — the recovered combat→faction mood bridge + `[-100,100]` relation model
    /// (`mercs2_faction::FactionWorld`, faction code map). Seeded with the recovered initial relations.
    faction: mercs2_faction::FactionWorld,
    /// The living-world population/spawner manager the game's `Ai.TweakAttachedSpawners*`/spawn-list Lua
    /// drives (`mercs2_population::PopulationWorld`, world-streaming/AI code maps §7).
    population: mercs2_population::PopulationWorld,
    /// The hero spawn position the game's Lua set via `Object.SetPosition(Player.GetLocalCharacter(),
    /// …)` — the base game's `MrxUtil._TeleportHero` bottoms out to exactly that (mrxutil.lua:328). The
    /// boot reads this to place the player: the spawn is **Lua-authored, no engine-constant fallback**.
    hero_teleport: Option<[f32; 3]>,
    /// The world's named markers (lowercased name → world pos) — the engine's `Pg.GetGuidByName`→pos
    /// table. Set from the loaded world so the real boot flow's `CreatePlayerCharacter` resolves the
    /// spawn location marker (e.g. `PmcCon001_Start1`) to coords.
    named_locations: std::collections::HashMap<String, [f32; 3]>,
    /// Minted GUID → the marker name it stands for, so `Object.GetPosition(guid)` on a
    /// `Pg.GetGuidByName` result resolves back through `named_locations`.
    marker_guids: std::collections::HashMap<u64, String>,
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
    /// mission Lua drives through its lifecycle (`mercs2_vehicle::HijackFsm`).
    hijacks: HashMap<u64, mercs2_vehicle::HijackFsm>,
    /// Per-vehicle turret/rotor aim (`Vehicle.SetTurretPitch/Yaw`, `Vehicle.SpinHeli`).
    turrets: HashMap<u64, mercs2_vehicle::TurretAim>,
    /// Engine settings the `Sys.Set*` config surface writes and the matching `Sys.*` getters read
    /// (the game holds these; the rest of the engine reads them). `Set*`→`Get*` are real roundtrips.
    settings: SysSettings,
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
            audio: Rc::new(RefCell::new(AudioEngine::default())),
            ai: mercs2_ai::AiWorld::new(),
            ai_states: std::collections::HashMap::new(),
            faction: mercs2_faction::FactionWorld::with_default_relations(),
            population: mercs2_population::PopulationWorld::new(),
            hero_teleport: None,
            named_locations: std::collections::HashMap::new(),
            marker_guids: std::collections::HashMap::new(),
            hero_spawn: None,
            hero_character: String::new(),
            player_character: HashMap::new(),
            human_states: HashMap::new(),
            hijacks: HashMap::new(),
            turrets: HashMap::new(),
            settings: SysSettings::default(),
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

    /// Give the host the world's named markers + the hero template, so the real boot flow's
    /// `CreatePlayerCharacter(location=<name>)` resolves against them (`Pg.GetGuidByName`→`GetPosition`)
    /// and `Pg.Spawn(hero, …)` places the hero at the marker.
    pub fn set_boot_context(&mut self, named_locations: std::collections::HashMap<String, [f32; 3]>, hero_character: impl Into<String>) {
        self.named_locations = named_locations;
        self.hero_character = hero_character.into();
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
        println!("[{source}] {msg}");
    }
    fn get_level_name(&self) -> String {
        self.level.clone()
    }
    fn guid_by_name(&mut self, name: &str) -> u64 {
        // A spawned object with that name wins; otherwise a NAMED WORLD MARKER (the base game's
        // Pg.GetGuidByName over placed markers, e.g. spawn-location points) mints a stable GUID whose
        // position resolves through `named_locations` in `object_get_position`.
        if let Some(g) = self.by_name.get(name).copied() {
            return g;
        }
        if self.named_locations.contains_key(&name.to_ascii_lowercase()) {
            self.next_guid += 1;
            let guid = self.next_guid;
            self.by_name.insert(name.to_string(), guid);
            self.marker_guids.insert(guid, name.to_ascii_lowercase());
            return guid;
        }
        0
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
        // The hero is a Lua-addressable object: teleporting it (the base game's _TeleportHero →
        // Object.SetPosition path) records the spawn the boot consumes. Other GUIDs are spawn requests.
        if guid == HERO_GUID {
            self.hero_teleport = Some(pos);
            return;
        }
        if let Some(r) = self.req_mut(guid) {
            r.pos = pos;
        }
    }
    fn object_get_position(&mut self, guid: u64) -> [f32; 3] {
        // A named world marker (from Pg.GetGuidByName) resolves through named_locations — this is how
        // CreatePlayerCharacter turns a spawn-location NAME into coords. Else a spawn request's pos.
        if let Some(name) = self.marker_guids.get(&guid) {
            return self.named_locations.get(name).copied().unwrap_or([0.0; 3]);
        }
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
    fn object_set_yaw(&mut self, guid: u64, yaw: f32) {
        if let Some(r) = self.req_mut(guid) {
            r.yaw = yaw;
        }
    }
    fn teleport_hero(&mut self, pos: [f32; 3]) {
        self.hero_teleport = Some(pos);
    }
    fn add_layers(&mut self, _layers: &[String]) {}

    // ===== Sound / music → the live `mercs2_audio::AudioEngine` (the fleet audio system, wired in). =====
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

    // ===== AI order surface → the recovered mechanism (`mercs2_ai::AiWorld`). =====
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
        let adjust = mercs2_population::SpawnerAdjust {
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
        let fsm = self.hijacks.entry(veh).or_insert_with(mercs2_vehicle::HijackFsm::new);
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
        let aim = self.turrets.entry(veh).or_insert_with(mercs2_vehicle::TurretAim::new);
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
            || self.marker_guids.contains_key(&guid)
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
            Ok(_) => eprintln!("[script] real boot produced no spawns; using inline glue"),
            Err(e) => eprintln!("[script] real boot failed ({e}); using inline glue"),
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
            eprintln!("[script] resident host init failed ({e}); world runs script-less");
            return None;
        }
    };
    if let Err(e) = sh.register_engine(host) {
        eprintln!("[script] resident register_engine failed ({e}); world runs script-less");
        return None;
    }
    // Auto-stub the binding tables that game modules touch at load time (logged no-ops); the real
    // gameplay bindings (Pg.Spawn/Object.*/Event/Sound/Ai) stay live.
    let trace: Rc<RefCell<BTreeSet<String>>> = Rc::new(RefCell::new(BTreeSet::new()));
    if let Err(e) = sh.enable_autostub(trace) {
        eprintln!("[script] resident autostub failed ({e}); world runs script-less");
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
pub fn run_boot_flow(sh: &ScriptHost, contract: &str, character: &str) {
    println!("[world] ===== vanilla boot Lua flow: MrxBootstrap.Start() =====");
    // Drive the flow the way the engine does: MrxBootstrap.Start() registers the callbacks, then the
    // mission flow sets the spawn location (SetSpawnLocations(<Contract>_Start1)) and the player-joined
    // path spawns the hero (CreatePlayerCharacter → Pg.GetGuidByName → Object.GetPosition → Pg.Spawn).
    // Wrapped in pcall so a later unbacked call (AttachToCharacter/OnPlayerInit) doesn't abort — the
    // Pg.Spawn (the hero placement) runs first, so the spawn is captured regardless.
    let src = format!(
        "import(\"MrxBootstrap\")\n\
         import(\"MrxPlayer\")\n\
         MrxBootstrap.Start()\n\
         MrxPlayer.SetSpawnLocations({{ \"{contract}_Start1\" }})\n\
         local ok, err = pcall(MrxPlayer.CreatePlayerCharacter, true, 0, \"{character}\", \"{contract}_Start1\")\n\
         if not ok then Debug.Printf(\"CreatePlayerCharacter aborted: \" .. tostring(err)) end\n"
    );
    match sh.exec(&src, "@boot_flow") {
        Ok(()) => println!("[world] ===== boot flow returned (Start + spawn) ====="),
        Err(e) => eprintln!("[world] ===== boot flow error (first divergence): {e} ====="),
    }
}

/// Advance the resident script host one fixed step: pump the Lua event/timer system (`Event.__pump(dt)`)
/// so `TimerRelative` fires and posted events dispatch. A no-op if `Event`/`__pump` aren't present.
/// Errors are logged, not fatal (a mission-script bug must not kill the render loop).
pub fn pump_resident(sh: &ScriptHost, dt: f32) {
    if let Err(e) = sh.exec(
        &format!("if Event and Event.__pump then Event.__pump({dt}) end"),
        "@resident_pump",
    ) {
        eprintln!("[script] resident pump error: {e}");
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
            eprintln!("[script] host init failed: {e}");
            return Vec::new();
        }
    };
    if let Err(e) = sh.register_engine(host.clone()) {
        eprintln!("[script] register_engine failed: {e}");
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
        eprintln!("[script] interior boot failed: {e}");
        return Vec::new();
    }
    let spawns = std::mem::take(&mut host.borrow_mut().spawns);
    spawns
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The audio system is wired in: real game `Sound.*` Lua drives the live `mercs2_audio::AudioEngine`
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
    /// drives `mercs2_ai::AiWorld` (the ring), `mercs2_faction::FactionWorld` (the mood bridge), and the
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
    /// → `Pg.Spawn(hero, x,y,z)` — exactly what `MrxPlayer.CreatePlayerCharacter` runs. The marker
    /// resolves through the world's `named_locations`, and the hero's Pg.Spawn position is captured for
    /// the loop. No hardcoded coordinate anywhere.
    #[test]
    fn boot_spawn_chain_resolves_marker_to_hero_spawn() {
        let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
        let mut nl = std::collections::HashMap::new();
        nl.insert("pmccon001_start1".to_string(), [10.0, 20.0, 30.0]);
        host.borrow_mut().set_boot_context(nl, "chris");
        let sh = resident_script_host(host.clone()).expect("resident host");

        // The CreatePlayerCharacter chain (name → guid → position → Pg.Spawn(hero)).
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
            "the hero must spawn at the marker the name resolved to — Lua-driven, no const"
        );
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
