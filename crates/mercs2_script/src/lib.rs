//! `mercs2_script` — the engine's Lua script host.
//!
//! This is the **engine** side of scripting: the VM, the module system (`import`/`inherit`/
//! `dynamic_import`), and the *mechanism* for exposing engine services to Lua (the `Sys.*` / `Debug.*`
//! / `Pg.*` / `Event.*` C-binding tables). It is asset-agnostic — it knows nothing about Mercenaries.
//! The game's Mercenaries Lua (`docs/mercs2-luacd/`, the `Mrx*`/mission/contract modules) runs *on*
//! this host and drives the engine through it. This realizes charter **Phase 3** ("embed Lua 5.4; run
//! migrated scripts validated by Surface B") and the engine/game split in
//! `docs/modernization/pangea_engine_alignment.md`.
//!
//! ## Seam: inversion of control
//! The host never calls the engine directly. Instead the engine implements [`EngineHost`] and hands it
//! in via [`ScriptHost::register_engine`]. The binding closures call that trait. So the dependency
//! points *into* this crate (engine → script host), never the reverse — the same shape as the original
//! `Sys.*` C-binding table calling into the native engine.
//!
//! ## What Phase 1 installs
//! - Lua 5.4 (`mlua`, vendored) + the **measured** 5.1→5.4 compat prelude (charter migration table:
//!   `unpack`, `table.getn`, `math.mod`, `string.gfind`, `loadstring`; the heavy constructs are 0 files).
//! - The **module system**: `import(name)` loads a corpus `.lua` into its own `_ENV` table (metatable
//!   `__index → _G`) so the file's bare `function Foo()` become module members; `inherit(base)` chains
//!   `__index → base`; results cache in `_MODULES`. This is the C-level environment-set the original
//!   engine did (`_SYS._IMPORT`), done here with `Chunk::set_environment`.
//! - The confirmed **engine binding tables** the boot slice needs: `Debug`, `Sys`, `Pg`, `Event`, plus
//!   a provisional `_Engine` seam for actor/layer spawning (renamed to its real C-binding once
//!   `mrxutil.lua`'s `SpawnActor` bottom-out is pinned against `binding_map.json`).
//!
//! Later phases widen the binding surface toward the captured 53-table / 1216-fn Surface-B inventory
//! (`mods/lua_trace_asi/reference/binding_map.json`) and run the real `mrxbootstrap` module tree.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{Lua, Result as LuaResult, Table};

pub mod bindings;
pub use bindings::{coverage_json, install_all, totals, NsCoverage, Totals};

/// The engine services the script bindings call. The engine (`mercs2_engine`) implements this; the
/// script host only ever talks to the engine through it. Every method here corresponds to an original
/// engine C-binding (or a small cluster of them) that Mercenaries Lua invokes.
///
/// Phase 1 covers the boot + PMC-interior-spawn slice; methods are added as the binding surface widens.
pub trait EngineHost {
    /// `Debug.Printf` / `Debug.Print` sink (the game's Lua log stream — the `[lua]` lines).
    fn log(&mut self, source: &str, msg: &str);
    /// `Sys.GetLevelName` — the current master level (e.g. `"vz"`).
    fn get_level_name(&self) -> String;
    /// `Sys.StartWithResources` — the dev/cheat "start rich" flag.
    fn start_with_resources(&self) -> bool {
        false
    }
    /// `Pg.GetGuidByName` — resolve a placed-object name to its runtime GUID (0 = not found; the
    /// binding maps 0 → Lua `nil` so the game's `if not uGuid` control flow is authentic).
    fn guid_by_name(&mut self, name: &str) -> u64;
    /// `Pg.Spawn(template, x,y,z,yaw, bLink, bHighDetail)` — instantiate a template actor. This is the
    /// bottom-out of `MrxUtil.SpawnActor`, and where a template NAME (e.g. `HqInterior`) is resolved
    /// into geometry. Returns the new actor's GUID (0 on failure → Lua `nil`).
    fn pg_spawn(&mut self, template: &str, pos: [f32; 3], yaw: f32, high_detail: bool) -> u64;
    /// `Object.SetName` — bind a placed name to a runtime GUID.
    fn object_set_name(&mut self, guid: u64, name: &str);
    /// `Object.SetPosition` — move an actor to a world position.
    fn object_set_position(&mut self, guid: u64, pos: [f32; 3]);
    /// `Object.SetYaw` — set an actor's heading (degrees).
    fn object_set_yaw(&mut self, guid: u64, yaw: f32);
    /// `Object.GetPosition`.
    fn object_get_position(&mut self, guid: u64) -> [f32; 3] {
        let _ = guid;
        [0.0; 3]
    }
    /// `Object.GetYaw`.
    fn object_get_yaw(&mut self, guid: u64) -> f32 {
        let _ = guid;
        0.0
    }
    /// `MrxUtil._TeleportHero` — move the player to a world position. (Lua binding wired in a later
    /// phase, once its C-binding bottom-out is pinned; the seam is final.)
    fn teleport_hero(&mut self, pos: [f32; 3]);
    /// The bottom-out of `MrxLayerManager.Add({..})`: request `vz_state_*` world-state layers. (Lua
    /// binding wired in a later phase; the seam is final.)
    fn add_layers(&mut self, layers: &[String]);

    // ===== Player: economy (money/fuel — signed i32 on the profile/economy singleton `[0x1176054]`,
    // see the money-fuel-datatype notes; `i64` here so the Lua number round-trips exactly). =====
    /// `Player.GetCash`.
    fn player_cash(&self) -> i64 {
        0
    }
    /// `Player.SetCash`.
    fn player_set_cash(&mut self, cash: i64) {
        let _ = cash;
    }
    /// `Player.GetFuel`.
    fn player_fuel(&self) -> i64 {
        0
    }
    /// `Player.SetFuel`.
    fn player_set_fuel(&mut self, fuel: i64) {
        let _ = fuel;
    }
    /// `Player.GetFuelCapacity`.
    fn player_fuel_capacity(&self) -> i64 {
        0
    }
    /// `Player.SetFuelCapacity`.
    fn player_set_fuel_capacity(&mut self, cap: i64) {
        let _ = cap;
    }

    // ===== Player / character GUID getters (0 = none → the binding maps it to Lua `nil`). =====
    /// `Player.GetLocalPlayer` — the local player object's GUID.
    fn player_local_player(&self) -> u64 {
        0
    }
    /// `Player.GetAnyCharacter` — any player-controlled character (the most-called `Player` cfunc).
    fn player_any_character(&self) -> u64 {
        0
    }
    /// `Player.GetLocalCharacter`.
    fn player_local_character(&self) -> u64 {
        0
    }
    /// `Player.GetPrimaryCharacter`.
    fn player_primary_character(&self) -> u64 {
        0
    }
    /// `Player.GetSecondaryCharacter` (0 = no second player).
    fn player_secondary_character(&self) -> u64 {
        0
    }
    /// `Player.IsLocal`.
    fn player_is_local(&self, guid: u64) -> bool {
        let _ = guid;
        true
    }

    // ===== Object: health / life / labels (the highest-traffic `Object` cfuncs). =====
    /// `Object.GetHealth`.
    fn object_health(&self, guid: u64) -> f32 {
        let _ = guid;
        0.0
    }
    /// `Object.SetHealth`.
    fn object_set_health(&mut self, guid: u64, hp: f32) {
        let _ = (guid, hp);
    }
    /// `Object.GetMaxHealth`.
    fn object_max_health(&self, guid: u64) -> f32 {
        let _ = guid;
        0.0
    }
    /// `Object.IsAlive`.
    fn object_is_alive(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Object.Kill`.
    fn object_kill(&mut self, guid: u64) {
        let _ = guid;
    }
    /// `Object.Revive`.
    fn object_revive(&mut self, guid: u64) {
        let _ = guid;
    }
    /// `Object.Remove`.
    fn object_remove(&mut self, guid: u64) {
        let _ = guid;
    }
    /// `Object.GetName`.
    fn object_name(&self, guid: u64) -> String {
        let _ = guid;
        String::new()
    }
    /// `Object.AddLabel`.
    fn object_add_label(&mut self, guid: u64, label: &str) {
        let _ = (guid, label);
    }
    /// `Object.RemoveLabel`.
    fn object_remove_label(&mut self, guid: u64, label: &str) {
        let _ = (guid, label);
    }
    /// `Object.HasLabel`.
    fn object_has_label(&self, guid: u64, label: &str) -> bool {
        let _ = (guid, label);
        false
    }
    /// `Object.SetInvincible`.
    fn object_set_invincible(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }

    // ===== Sys: game-state request + autosave (the world-load handshake `loadprobe` scores). =====
    /// `Sys.RequestGameState` — request a game-state transition (`"WaitForStreaming"`,
    /// `"WaitForTether"`, `"InGame"`, …); the engine drives the FSM + fires `Event.GameStateChange`.
    fn sys_request_game_state(&mut self, state: &str) {
        let _ = state;
    }
    /// `Sys.RequestAutosave`.
    fn sys_request_autosave(&mut self) {}
    /// `Sys.IsLoadingOrStreaming` — the busy-flag gate (`mgr+0x4c35c`).
    fn sys_is_loading_or_streaming(&self) -> bool {
        false
    }
    /// `Sys.GuidToString`.
    fn sys_guid_to_string(&self, guid: u64) -> String {
        format!("{guid:#x}")
    }

    // ===== Vehicle (the real host forwards to `mercs2_vehicle`; the harness backs it with seat state). =====
    /// `Vehicle.GetDriver` (0 = empty seat → nil).
    fn vehicle_driver(&self, veh: u64) -> u64 {
        let _ = veh;
        0
    }
    /// `Vehicle.GetRiders`.
    fn vehicle_riders(&self, veh: u64) -> Vec<u64> {
        let _ = veh;
        Vec::new()
    }
    /// `Vehicle.GetFromRider` — the vehicle a rider occupies (0 = on foot).
    fn vehicle_from_rider(&self, rider: u64) -> u64 {
        let _ = rider;
        0
    }
    /// `Vehicle.GetSeatFromRider`.
    fn vehicle_seat_from_rider(&self, rider: u64) -> String {
        let _ = rider;
        String::new()
    }
    /// `Vehicle.GetSeatByType`.
    fn vehicle_seat_by_type(&self, veh: u64, ty: &str) -> String {
        let _ = (veh, ty);
        String::new()
    }
    /// `Vehicle.Enter(veh, rider, seat)` → success.
    fn vehicle_enter(&mut self, veh: u64, rider: u64, seat: &str) -> bool {
        let _ = (veh, rider, seat);
        false
    }
    /// `Vehicle.Exit(rider)` → success.
    fn vehicle_exit(&mut self, rider: u64) -> bool {
        let _ = rider;
        false
    }
    /// `Vehicle.Usable`.
    fn vehicle_usable(&self, veh: u64) -> bool {
        let _ = veh;
        false
    }
    /// `Vehicle.IsFlying`.
    fn vehicle_is_flying(&self, veh: u64) -> bool {
        let _ = veh;
        false
    }
    /// `Vehicle.IsFlipped`.
    fn vehicle_is_flipped(&self, veh: u64) -> bool {
        let _ = veh;
        false
    }
    /// `Vehicle.SetParts`.
    fn vehicle_set_parts(&mut self, veh: u64) {
        let _ = veh;
    }
    /// `Vehicle.OpenDoor` / `Vehicle.CloseDoor`.
    fn vehicle_set_door(&mut self, veh: u64, open: bool) {
        let _ = (veh, open);
    }
    /// `Vehicle.SetCanPlayerUse`.
    fn vehicle_set_can_player_use(&mut self, veh: u64, can: bool) {
        let _ = (veh, can);
    }
    /// `Vehicle.EnableTurret`.
    fn vehicle_enable_turret(&mut self, veh: u64, on: bool) {
        let _ = (veh, on);
    }
    /// `Vehicle.ClearControls`.
    fn vehicle_clear_controls(&mut self, veh: u64) {
        let _ = veh;
    }

    // ===== Sound / music / VO (the real host forwards to `mercs2_audio::AudioEngine`). =====
    /// `Sound.CueSound` → voice id (0 = failed → nil).
    fn sound_cue(&mut self, cue: &str) -> u64 {
        let _ = cue;
        0
    }
    /// `Sound.StopSound`.
    fn sound_stop(&mut self, voice: u64) {
        let _ = voice;
    }
    /// `Sound.PauseSound`.
    fn sound_pause(&mut self, voice: u64) {
        let _ = voice;
    }
    /// `Sound.SetCategoryVolume`.
    fn sound_set_category_volume(&mut self, cat: &str, vol: f32) {
        let _ = (cat, vol);
    }
    /// `Sound.SetMasterVolume`.
    fn sound_set_master_volume(&mut self, vol: f32) {
        let _ = vol;
    }
    /// `Sound.FadeCategoryDown` (`down=true`) / `FadeCategoryUp`.
    fn sound_fade_category(&mut self, cat: &str, down: bool) {
        let _ = (cat, down);
    }
    /// `Sound.StopAndFlushAllSounds`.
    fn sound_stop_all(&mut self) {}
    /// `Sound.TransitionMusic` → accepted.
    fn sound_transition_music(&mut self, state: &str) -> bool {
        let _ = state;
        false
    }
    /// `Sound.AddMusicState`.
    fn sound_add_music_state(&mut self, name: &str) {
        let _ = name;
    }
    /// `Sound.AddMusicTransition`.
    fn sound_add_music_transition(&mut self, from: &str, to: &str) {
        let _ = (from, to);
    }
    /// `Sound.SetDynamicMusic`.
    fn sound_set_dynamic_music(&mut self, on: bool) {
        let _ = on;
    }
    /// `Sound.IsDynamicMusic`.
    fn sound_is_dynamic_music(&self) -> bool {
        false
    }
    /// `Sound.BindMusicCue`.
    fn sound_bind_music_cue(&mut self, state: &str, cue: &str) {
        let _ = (state, cue);
    }
    /// `Sound.ClearMusicCues`.
    fn sound_clear_music_cues(&mut self) {}
    /// `Sound.CueAmbience` → voice id.
    fn sound_cue_ambience(&mut self, cue: &str) -> u64 {
        let _ = cue;
        0
    }
    /// `Sound.StopAmbience`.
    fn sound_stop_ambience(&mut self) {}
    /// `Sound.GetAudioDir`.
    fn sound_audio_dir(&self) -> String {
        String::new()
    }
    /// `Sound._GetLibVersion`.
    fn sound_lib_version(&self) -> String {
        "PgAudio".into()
    }
    /// `Sound.LockActionLevelMusic`.
    fn sound_lock_action_level_music(&mut self, level: i64) {
        let _ = level;
    }
    /// `VO.Cue` → voice id.
    fn vo_cue(&mut self, cue: &str) -> u64 {
        let _ = cue;
        0
    }

    /// `Object.GetVelocity` — speed magnitude (m/s).
    fn object_velocity(&self, guid: u64) -> f32 {
        let _ = guid;
        0.0
    }

    // ===== AI order surface (`Ai.*` → the real host forwards to `mercs2_ai::AiWorld`). =====
    // The engine supplies the mechanism (the hash-addressed action ring + the relation matrix); the
    // goal/state vocabulary is authored data (AI code map §5/§8). These post to that mechanism.
    /// `Ai.Goal(guid, goal)` — hash the goal verb and post it to the AI action ring (`DirectAction`).
    /// Returns whether the ring accepted it (false = the 1024-slot budget was full).
    fn ai_goal(&mut self, guid: u64, goal: &str) -> bool {
        let _ = (guid, goal);
        false
    }
    /// `Ai.DirectAction(guid, actionHash)` — post a pre-hashed action to the AI ring.
    fn ai_direct_action(&mut self, guid: u64, action_hash: u32) -> bool {
        let _ = (guid, action_hash);
        false
    }
    /// `Ai.SetRelation(from, to, value)` — set the directed attitude, clamped `[-100,100]`.
    fn ai_set_relation(&mut self, from: u64, to: u64, value: i64) {
        let _ = (from, to, value);
    }
    /// `Ai.GetRelation(from, to)` — the directed attitude (`0` if unset).
    fn ai_get_relation(&self, from: u64, to: u64) -> i64 {
        let _ = (from, to);
        0
    }
    /// `Ai.SetState(guid, state, on)` — flip a named `AiBehavior` restriction flag; returns whether the
    /// state name was recognised.
    fn ai_set_state(&mut self, guid: u64, state: &str, on: bool) -> bool {
        let _ = (guid, state, on);
        false
    }
}

/// Shared, single-threaded handle to the engine host. The VM and the engine live on the same thread
/// (the render loop is `pollster::block_on` on main), so `Rc<RefCell<…>>` is the right sharing model —
/// no `Send` is required (and `mlua`'s default build does not demand it).
pub type SharedHost = Rc<RefCell<dyn EngineHost>>;

/// The 5.1→5.4 compatibility prelude — exactly the constructs the charter measured across the 409
/// corpus files. The heavy ones (`setfenv`/`module`/`loadstring`/`table.setn`/`math.mod`/
/// `string.gfind`) are 0 files, so this is small and non-invasive.
const COMPAT_PRELUDE: &str = r#"
-- charter "Lua 5.1 -> 5.4 migration surface" compat aliases
unpack = unpack or table.unpack
loadstring = loadstring or load
if not table.getn then function table.getn(t) return #t end end
math.mod = math.mod or math.fmod
string.gfind = string.gfind or string.gmatch
_MODULES = _MODULES or {}
"#;

/// Bring-up auto-stub layer (opt-in). Installs a `_G` metatable so a read of an as-yet-unimplemented
/// Capitalized global (an engine binding table the game Lua expects) resolves to a logged no-op stub
/// — indexable AND callable, recursively — instead of erroring. Lets the real import cascade complete;
/// every stubbed name is reported to `__stub_note` (a reimpl-side Surface-B binding trace). Lowercase
/// misses stay `nil` (normal semantics).
const AUTOSTUB_LUA: &str = r#"
local function makestub(path)
  return setmetatable({}, {
    __index = function(_, k) __stub_note(path .. "." .. tostring(k)); return makestub(path .. "." .. tostring(k)) end,
    __call  = function(_, ...) __stub_note("call:" .. path); return nil end,
  })
end
setmetatable(_G, {
  __index = function(_, k)
    if type(k) == "string" and string.match(k, "^%u") then
      __stub_note("global:" .. k)
      local s = makestub(k)
      rawset(_G, k, s)
      return s
    end
    return nil
  end,
})
"#;

/// The module loader: resolves `import`/`inherit` names to corpus `.lua` files, executes each in its
/// own environment, and caches the result. Held behind an `Rc` and captured by the loader closures.
struct Loader {
    /// lowercased module name (file stem) → source path.
    index: HashMap<String, PathBuf>,
    /// lowercased module name → its loaded environment table (also the module's public surface).
    loaded: RefCell<HashMap<String, Table>>,
    /// Stack of environment tables for the currently-executing `import` chain, so `inherit()` can find
    /// "the module being defined right now" and set its `__index` to the base.
    stack: RefCell<Vec<Table>>,
}

impl Loader {
    fn new(roots: &[PathBuf]) -> Self {
        let mut index = HashMap::new();
        for root in roots {
            index_lua_files(root, &mut index);
        }
        Loader {
            index,
            loaded: RefCell::new(HashMap::new()),
            stack: RefCell::new(Vec::new()),
        }
    }

    /// `import(name)` — load `name` once (cached), bind it as a global, return its module table.
    fn import(&self, lua: &Lua, name: &str) -> LuaResult<Table> {
        let key = name.to_ascii_lowercase();
        if let Some(t) = self.loaded.borrow().get(&key) {
            lua.globals().set(name, t.clone())?;
            return Ok(t.clone());
        }
        let path = self.index.get(&key).cloned().ok_or_else(|| {
            mlua::Error::RuntimeError(format!("import: module '{name}' not found in roots"))
        })?;
        let src = std::fs::read_to_string(&path)
            .map_err(|e| mlua::Error::RuntimeError(format!("import '{name}': {e}")))?;

        // Fresh environment; misses fall through to the globals (stdlib, other modules, engine tables).
        let env = lua.create_table()?;
        let mt = lua.create_table()?;
        mt.set("__index", lua.globals())?;
        let _ = env.set_metatable(Some(mt));

        // Register BEFORE exec so a cyclic import sees the (partial) table instead of re-loading.
        self.loaded.borrow_mut().insert(key.clone(), env.clone());
        lua.globals().set(name, env.clone())?;

        self.stack.borrow_mut().push(env.clone());
        let res = lua
            .load(&src)
            .set_name(format!("@{name}"))
            .set_environment(env.clone())
            .exec();
        self.stack.borrow_mut().pop();
        res?;
        Ok(env)
    }

    /// `inherit(base)` — the OO base-class mechanism: ensure `base` is loaded, then point the
    /// currently-defining module's `__index` at it (so it inherits base's methods; base itself still
    /// chains to `_G`).
    fn inherit(&self, lua: &Lua, base: &str) -> LuaResult<Table> {
        let base_tbl = self.import(lua, base)?;
        let cur = self.stack.borrow().last().cloned();
        if let Some(cur) = cur {
            let mt = lua.create_table()?;
            mt.set("__index", base_tbl.clone())?;
            let _ = cur.set_metatable(Some(mt));
        }
        Ok(base_tbl)
    }
}

/// The engine's Lua script host.
pub struct ScriptHost {
    lua: Lua,
    loader: Rc<Loader>,
}

impl ScriptHost {
    /// Build a host whose `import`/`inherit` resolve module names against `roots` (recursively indexed
    /// `.lua` files — e.g. `docs/mercs2-luacd/src`). Installs the compat prelude and the module system.
    pub fn new(roots: Vec<PathBuf>) -> LuaResult<Self> {
        let lua = Lua::new();
        lua.load(COMPAT_PRELUDE).set_name("@compat_prelude").exec()?;

        let loader = Rc::new(Loader::new(&roots));

        let imp = loader.clone();
        let import_fn = lua.create_function(move |lua, name: String| imp.import(lua, &name))?;
        lua.globals().set("import", import_fn)?;

        // `dynamic_import` is import-at-runtime; same resolution for our purposes.
        let dimp = loader.clone();
        let dyn_import_fn =
            lua.create_function(move |lua, name: String| dimp.import(lua, &name))?;
        lua.globals().set("dynamic_import", dyn_import_fn)?;

        let inh = loader.clone();
        let inherit_fn = lua.create_function(move |lua, base: String| inh.inherit(lua, &base))?;
        lua.globals().set("inherit", inherit_fn)?;

        Ok(ScriptHost { lua, loader })
    }

    /// A host with no module roots — for unit tests / bindings-only use.
    pub fn bare() -> LuaResult<Self> {
        Self::new(Vec::new())
    }

    /// Install the engine binding tables backed by `host`. Idempotent-ish: call once after `new`.
    ///
    /// The surface is modular: one file per engine namespace under [`bindings`], each declaring its
    /// required cfunc surface and installing this build's real/stub bodies. Phase 1's real bodies (the
    /// boot + PMC-interior slice: `Debug`, `Sys`, `Pg`, `Object`, `Ai`, `Vehicle`, `Event`) live in
    /// those files; every other namespace's cfuncs are still "stubs remaining" (see
    /// [`Self::register_engine_reported`] / [`bindings::coverage_json`]). The `Mrx*` modules are
    /// *game* Lua and come from the corpus via `import`, not from here.
    pub fn register_engine(&self, host: SharedHost) -> LuaResult<()> {
        self.register_engine_reported(host).map(|_| ())
    }

    /// Like [`Self::register_engine`], but returns the per-namespace [`bindings::NsCoverage`] so the
    /// coverage gate can measure "N stubs remaining" across the whole binding surface. Installing is a
    /// side effect (globals are set); the returned records are pure data.
    pub fn register_engine_reported(&self, host: SharedHost) -> LuaResult<Vec<NsCoverage>> {
        bindings::install_all(&self.lua, &host)
    }

    /// Install the lenient bring-up auto-stub layer ([`AUTOSTUB_LUA`]): reads of unimplemented
    /// Capitalized engine binding tables resolve to logged no-op stubs so the real import cascade
    /// completes. Every stubbed name is inserted into `sink` — the reimpl-side Surface-B binding trace
    /// telling us exactly which bindings the real scripts touch. Call AFTER `register_engine` so the
    /// real bindings take precedence; stubs only fill the gaps.
    pub fn enable_autostub(
        &self,
        sink: Rc<RefCell<std::collections::BTreeSet<String>>>,
    ) -> LuaResult<()> {
        let note = self.lua.create_function(move |_, name: String| {
            sink.borrow_mut().insert(name);
            Ok(())
        })?;
        self.lua.globals().set("__stub_note", note)?;
        self.lua.load(AUTOSTUB_LUA).set_name("@autostub").exec()?;
        Ok(())
    }

    /// Access the underlying VM (for advanced wiring / tests).
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Load and cache a corpus module by name, returning its table.
    pub fn import_module(&self, name: &str) -> LuaResult<Table> {
        self.loader.import(&self.lua, name)
    }

    /// Execute a source chunk in the global environment (for boot glue / tests).
    pub fn exec(&self, src: &str, name: &str) -> LuaResult<()> {
        self.lua.load(src).set_name(name.to_string()).exec()
    }

    /// Evaluate a source chunk and return a typed result.
    pub fn eval<T: mlua::FromLuaMulti>(&self, src: &str) -> LuaResult<T> {
        self.lua.load(src).eval()
    }
}

/// Recursively index `*.lua` files under `dir` by lowercased file stem → path. First writer wins on a
/// collision (roots earlier in the list take precedence).
fn index_lua_files(dir: &Path, out: &mut HashMap<String, PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            index_lua_files(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("lua") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                out.entry(stem.to_ascii_lowercase()).or_insert(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `EngineHost` that records what the bindings called, for assertions.
    #[derive(Default)]
    struct RecordingHost {
        logs: Vec<String>,
        /// (template, spawn-pos, yaw, high_detail) per `Pg.Spawn`.
        spawns: Vec<(String, [f32; 3], f32, bool)>,
        names: Vec<(u64, String)>,
        positions: Vec<(u64, [f32; 3])>,
        yaws: Vec<(u64, f32)>,
        layers: Vec<String>,
        teleports: Vec<[f32; 3]>,
        next_guid: u64,
    }
    impl EngineHost for RecordingHost {
        fn log(&mut self, _source: &str, msg: &str) {
            self.logs.push(msg.to_string());
        }
        fn get_level_name(&self) -> String {
            "vz".to_string()
        }
        fn start_with_resources(&self) -> bool {
            true
        }
        fn guid_by_name(&mut self, _name: &str) -> u64 {
            0 // "not yet spawned" → binding returns nil, so `if not uGuid` takes the Spawn path
        }
        fn pg_spawn(&mut self, template: &str, pos: [f32; 3], yaw: f32, high_detail: bool) -> u64 {
            self.next_guid += 1;
            self.spawns.push((template.to_string(), pos, yaw, high_detail));
            self.next_guid
        }
        fn object_set_name(&mut self, guid: u64, name: &str) {
            self.names.push((guid, name.to_string()));
        }
        fn object_set_position(&mut self, guid: u64, pos: [f32; 3]) {
            self.positions.push((guid, pos));
        }
        fn object_set_yaw(&mut self, guid: u64, yaw: f32) {
            self.yaws.push((guid, yaw));
        }
        fn teleport_hero(&mut self, pos: [f32; 3]) {
            self.teleports.push(pos);
        }
        fn add_layers(&mut self, layers: &[String]) {
            self.layers.extend_from_slice(layers);
        }
    }

    #[test]
    fn compat_prelude_bridges_5_1_constructs() {
        let h = ScriptHost::bare().unwrap();
        let (a, b): (i64, i64) = h.eval("return unpack({10, 20})").unwrap();
        assert_eq!((a, b), (10, 20));
        let n: i64 = h.eval("return table.getn({1,2,3,4})").unwrap();
        assert_eq!(n, 4);
        // loadstring alias present
        let ok: bool = h.eval("return loadstring ~= nil").unwrap();
        assert!(ok);
    }

    #[test]
    fn module_system_import_and_inherit() {
        // Build a tiny two-module corpus in a temp dir.
        let dir = std::env::temp_dir().join(format!("m2script_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("basething.lua"),
            "function Greet() return \"base\" end\nfunction Kind() return \"BASE\" end\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("childthing.lua"),
            "inherit(\"BaseThing\")\nfunction Kind() return \"CHILD\" end\n",
        )
        .unwrap();

        let h = ScriptHost::new(vec![dir.clone()]).unwrap();
        let child = h.import_module("ChildThing").unwrap();
        // own method
        let kind: String = child.get::<mlua::Function>("Kind").unwrap().call(()).unwrap();
        assert_eq!(kind, "CHILD");
        // inherited method (via __index chain to BaseThing)
        let greet: String = child.get::<mlua::Function>("Greet").unwrap().call(()).unwrap();
        assert_eq!(greet, "base");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn authentic_spawnactor_recipe_routes_to_host() {
        let host = Rc::new(RefCell::new(RecordingHost::default()));
        let h = ScriptHost::bare().unwrap();
        h.register_engine(host.clone()).unwrap();

        // Debug.Printf -> host.log ; Sys.GetLevelName -> host
        let lvl: String = h
            .eval("Debug.Printf(\"gui loaded\"); return Sys.GetLevelName()")
            .unwrap();
        assert_eq!(lvl, "vz");

        // Pg.GetGuidByName returns nil for an unspawned name → the game's `if not uGuid` is authentic.
        let is_nil: bool = h.eval("return Pg.GetGuidByName(\"Nope\") == nil").unwrap();
        assert!(is_nil);

        // Run the EXACT MrxUtil.SpawnActor body for the inanimate HqInterior against the real
        // Pg.Spawn / Object.* bindings (mrxutil.lua:463-490).
        let guid: i64 = h
            .eval(
                r#"
                local uGuid = Pg.GetGuidByName("HqInterior")
                if not uGuid then uGuid = Pg.Spawn("PmcHqInterior", 0, 0, 0, 0, false, true) end
                Object.SetName(uGuid, "HqInterior")
                Object.SetPosition(uGuid, 3750, 450, -3840)
                Object.SetYaw(uGuid, 0)
                return uGuid
                "#,
            )
            .unwrap();
        assert_eq!(guid, 1);

        let hb = host.borrow();
        assert_eq!(hb.logs, vec!["gui loaded".to_string()]);
        assert_eq!(
            hb.spawns,
            vec![("PmcHqInterior".to_string(), [0.0, 0.0, 0.0], 0.0, true)]
        );
        assert_eq!(hb.names, vec![(1u64, "HqInterior".to_string())]);
        assert_eq!(hb.positions, vec![(1u64, [3750.0, 450.0, -3840.0])]);
        assert_eq!(hb.yaws, vec![(1u64, 0.0)]);
    }

    /// The Wave-0 E3 **coverage gate**. Installs the whole binding surface, writes the machine-readable
    /// `binding_coverage.json` next to the crate, and asserts the current baseline so any later silo's
    /// progress (or a regression) is visible as a diff. `remaining` = required cfuncs still lacking a
    /// real body — the "N stubs remaining" metric, which must only ever go **down**.
    ///
    /// Later silos: when you fill a namespace, re-run this test to regenerate the JSON, then bump the
    /// asserted `EXPECTED_REAL` / `EXPECTED_REMAINING` below (they should move in opposite directions).
    #[test]
    fn coverage_report() {
        // Baseline of the current build. Update as silos land bodies (the Lua-hook TDD pass added the
        // Event system + Player economy/getters + Object health/labels + Sys game-state handshake).
        const EXPECTED_NAMESPACES: usize = 35;
        const EXPECTED_REQUIRED: usize = 1086;
        // Baseline after the Wave-3 binding pass (5 agents backed ~520 bindings across HUD/Net/
        // presentation/math+string+util/object-family; +3 real cross-cutting fixes). Bump when a silo
        // lands more bodies. real 86→190 (+104 real); stubs jumped with the faithful no-op surface.
        const EXPECTED_REAL: usize = 190;
        const EXPECTED_STUB: usize = 428; // 9→428: the faithful no-op surface (HUD/Net/presentation/…)

        let host = Rc::new(RefCell::new(RecordingHost::default()));
        let h = ScriptHost::bare().unwrap();
        let cov = h.register_engine_reported(host).unwrap();

        let t = totals(&cov);
        assert_eq!(t.namespaces, EXPECTED_NAMESPACES, "namespace count changed");
        assert_eq!(
            t.required, EXPECTED_REQUIRED,
            "required cfunc surface changed — did the seed move?"
        );
        assert_eq!(
            t.real, EXPECTED_REAL,
            "real-body count regressed/changed — bump EXPECTED_REAL when a silo lands bodies"
        );
        assert_eq!(t.stub, EXPECTED_STUB, "stub count changed");
        assert_eq!(
            t.remaining,
            EXPECTED_REQUIRED - EXPECTED_REAL,
            "'stubs remaining' must equal required-real"
        );

        // Spot-check the boot-slice namespaces route correctly.
        let by = |name: &str| cov.iter().find(|c| c.namespace == name).unwrap();
        assert_eq!(by("Debug").real_count(), 1);
        assert_eq!(by("Sys").real_count(), 7);
        assert_eq!(by("Pg").real_count(), 2);
        assert_eq!(by("Object").real_count(), 18);
        assert_eq!(by("Object").stub_count(), 3);
        assert_eq!(by("Player").real_count(), 14);
        assert_eq!(by("Event").real_count(), 4);
        assert_eq!(by("Vehicle").real_count(), 16);
        assert_eq!(by("Sound").real_count(), 20);
        // Pg.Spawn/GetGuidByName really live in table 0x00b99328 (the trace corrects the doc label).
        assert_eq!(by("Pg").table_va, 0x00B99328);

        // Emit the machine-readable report for CI / later silos to watch trend to zero.
        let json = coverage_json(&cov);
        let out =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binding_coverage.json");
        std::fs::write(&out, &json).expect("write binding_coverage.json");
        assert!(json.contains("\"remaining\""));
    }
}
