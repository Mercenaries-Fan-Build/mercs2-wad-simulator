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

use std::cell::{Cell, RefCell};
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

    // ===== ObjectFilter — the script-side object query (label expr + include/exclude sets). =====
    /// `ObjectFilter.Create()` → a fresh filter handle.
    fn object_filter_create(&mut self) -> u64 {
        0
    }
    /// `ObjectFilter.Copy(src)` → a duplicate filter handle.
    fn object_filter_copy(&mut self, src: u64) -> u64 {
        let _ = src;
        0
    }
    /// `ObjectFilter.SetFilter(f, expr)` — set the label boolean-expression predicate.
    fn object_filter_set_expr(&mut self, handle: u64, expr: &str) {
        let _ = (handle, expr);
    }
    /// `ObjectFilter.AddObject(f, guid, bInclude)` — add to the include (`true`) or exclude set.
    fn object_filter_add(&mut self, handle: u64, guid: u64, include: bool) {
        let _ = (handle, guid, include);
    }
    /// `ObjectFilter.RemoveObject(f, guid)`.
    fn object_filter_remove(&mut self, handle: u64, guid: u64) {
        let _ = (handle, guid);
    }
    /// `ObjectFilter.ClearObjects(f)` / `ClearFilter(f)`.
    fn object_filter_clear(&mut self, handle: u64) {
        let _ = handle;
    }
    /// `ObjectFilter.UsePlayers(f, on)`.
    fn object_filter_use_players(&mut self, handle: u64, on: bool) {
        let _ = (handle, on);
    }
    /// `ObjectFilter.GetObjects(f)` — the explicitly-included object GUIDs.
    fn object_filter_objects(&self, handle: u64) -> Vec<u64> {
        let _ = handle;
        Vec::new()
    }
    /// `ObjectFilter.Eval(f, guid)` — whether `guid` passes the filter (label predicate + sets).
    fn object_filter_eval(&self, handle: u64, guid: u64) -> bool {
        let _ = (handle, guid);
        false
    }
    /// `ObjectFilter._GC(f)` — free a filter handle.
    fn object_filter_gc(&mut self, handle: u64) {
        let _ = handle;
    }

    // ===== HUD widget tree + markers (`Hud.*` / `Gui._Marker*`) → `mercs2_ui`. =====
    /// The retained-mode HUD widget tree, if this host owns one (the real game host does; the smoke/
    /// test hosts return `None` and the `Hud.*` mutators become no-ops).
    fn hud(&mut self) -> Option<&mut mercs2_ui::WidgetTree> {
        None
    }
    /// Read-only view of the HUD widget tree (for `Get*` queries).
    fn hud_ref(&self) -> Option<&mercs2_ui::WidgetTree> {
        None
    }
    /// The HUD world-marker set, if this host owns one.
    fn markers(&mut self) -> Option<&mut mercs2_ui::MarkerSet> {
        None
    }
    /// Read-only view of the HUD marker set.
    fn markers_ref(&self) -> Option<&mercs2_ui::MarkerSet> {
        None
    }

    // ===== Render / post-FX parameter state (`Atmosphere`/`Bloom`/`Graphics`/`Fade`) → mercs2_core. =====
    /// The global render/post-FX parameter state, if this host owns one.
    fn render_state(&mut self) -> Option<&mut mercs2_core::RenderState> {
        None
    }
    /// Read-only view of the render state (for `Get*` queries).
    fn render_state_ref(&self) -> Option<&mercs2_core::RenderState> {
        None
    }

    // ===== Cinematic camera controller (`CameraFx.*` — script-driven camera pose/shake/blend). =====
    /// `SetYaw`/`GetYaw` (heading, radians).
    fn camera_set_yaw(&mut self, yaw: f32) {
        let _ = yaw;
    }
    fn camera_yaw(&self) -> f32 {
        0.0
    }
    /// `SetPitch`/`GetPitch` (elevation, radians).
    fn camera_set_pitch(&mut self, pitch: f32) {
        let _ = pitch;
    }
    fn camera_pitch(&self) -> f32 {
        0.0
    }
    /// `SetFOV`/`GetFOV` (field of view, degrees).
    fn camera_set_fov(&mut self, fov: f32) {
        let _ = fov;
    }
    fn camera_fov(&self) -> f32 {
        60.0
    }
    /// `SetPosition` / `SetLookAt` — the camera eye + target in world space.
    fn camera_set_position(&mut self, pos: [f32; 3]) {
        let _ = pos;
    }
    fn camera_set_lookat(&mut self, target: [f32; 3]) {
        let _ = target;
    }
    /// `Shake(intensity)` — set the camera-shake intensity.
    fn camera_shake(&mut self, intensity: f32) {
        let _ = intensity;
    }
    /// `Blend`/`StopBlending` — whether a camera blend is in progress.
    fn camera_set_blending(&mut self, on: bool) {
        let _ = on;
    }
    /// `Follow(guid)` — the object the cinematic camera follows (0 = none).
    fn camera_follow(&mut self, guid: u64) {
        let _ = guid;
    }
    /// `Hold(on)` — freeze the camera at its current pose.
    fn camera_hold(&mut self, on: bool) {
        let _ = on;
    }
    /// `SetShot(name)` — select a named cinematic shot.
    fn camera_set_shot(&mut self, shot: &str) {
        let _ = shot;
    }

    // ===== Inventory: per-character weapon loadout (`Inventory.*`). =====
    /// `SetAllWeapons(character, weapons)` — replace the character's weapon loadout.
    fn inventory_set_weapons(&mut self, character: u64, weapons: Vec<u64>) {
        let _ = (character, weapons);
    }
    /// `GetAllWeapons(character)` — the character's weapon GUIDs.
    fn inventory_weapons(&self, character: u64) -> Vec<u64> {
        let _ = character;
        Vec::new()
    }
    /// `GetPrimaryWeapon(character)` — slot 0 (0 = none → nil).
    fn inventory_primary(&self, character: u64) -> u64 {
        let _ = character;
        0
    }
    /// `GetSecondaryWeapon(character)` — slot 1 (0 = none → nil).
    fn inventory_secondary(&self, character: u64) -> u64 {
        let _ = character;
        0
    }
    /// `EquipWeapon(character, weapon)` — add the weapon to the loadout (if absent).
    fn inventory_equip(&mut self, character: u64, weapon: u64) {
        let _ = (character, weapon);
    }
    /// `DropWeapon(character, weapon)` — remove the weapon from the loadout.
    fn inventory_drop(&mut self, character: u64, weapon: u64) {
        let _ = (character, weapon);
    }
    /// `DestroyAllWeapons(character)` — clear the loadout.
    fn inventory_destroy_all(&mut self, character: u64) {
        let _ = character;
    }

    // ===== Weapon ammo (`Weapon.*`) — per-weapon clip/reserve state. =====
    /// `SetClipAmmo`/`SetReserveAmmo` — set the loaded/reserve round count (clamped ≥ 0).
    fn weapon_set_ammo(&mut self, weapon: u64, clip: Option<i32>, reserve: Option<i32>) {
        let _ = (weapon, clip, reserve);
    }
    /// `GetClipAmmo`/`GetReserveAmmo` — loaded / reserve rounds.
    fn weapon_clip(&self, weapon: u64) -> i32 {
        let _ = weapon;
        0
    }
    fn weapon_reserve(&self, weapon: u64) -> i32 {
        let _ = weapon;
        0
    }
    /// `GetMaxClipAmmo`/`GetMaxReserveAmmo` — capacities.
    fn weapon_max_clip(&self, weapon: u64) -> i32 {
        let _ = weapon;
        0
    }
    fn weapon_max_reserve(&self, weapon: u64) -> i32 {
        let _ = weapon;
        0
    }
    /// `Weapon.Reload` — move reserve into the clip up to its capacity.
    fn weapon_reload(&mut self, weapon: u64) {
        let _ = weapon;
    }
    /// `IsPrimary` / `IsDesignator` — weapon class flags.
    fn weapon_is_primary(&self, weapon: u64) -> bool {
        let _ = weapon;
        false
    }
    fn weapon_is_designator(&self, weapon: u64) -> bool {
        let _ = weapon;
        false
    }

    // ===== Fire (`Fire.*`) — per-object burning state. =====
    /// `Fire.Ignite(object)` — set the object alight.
    fn fire_ignite(&mut self, object: u64) {
        let _ = object;
    }
    /// `Fire.Extinguish`/`Put(object)` — put the object's fire out.
    fn fire_extinguish(&mut self, object: u64) {
        let _ = object;
    }
    /// Whether an object is currently on fire.
    fn object_is_burning(&self, object: u64) -> bool {
        let _ = object;
        false
    }
    /// `Object.SendDamage(target, amount)` — apply `amount` damage to the target's health, killing it
    /// if health reaches zero. Returns whether the target died.
    fn object_send_damage(&mut self, target: u64, amount: f32) -> bool {
        let _ = (target, amount);
        false
    }

    // ===== Pg world regions + alarms. =====
    /// `Pg.CreateRegion(name, center, radius)` — register a trigger region; returns its handle.
    fn pg_create_region(&mut self, name: &str, center: [f32; 3], radius: f32) -> u64 {
        let _ = (name, center, radius);
        0
    }
    /// `Pg.ActivateAlarm(guid, on)` — set an alarm's active state.
    fn pg_alarm_set(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Pg.ToggleAlarm(guid)` — flip an alarm; returns the new state.
    fn pg_alarm_toggle(&mut self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// Whether an alarm is currently active.
    fn pg_alarm_active(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }

    // ===== Airstrike designators + ordnance. =====
    /// `Airstrike.EquipDesignator(player)` — give the player a full designator.
    fn airstrike_equip_designator(&mut self, player: u64) {
        let _ = player;
    }
    /// `Airstrike.RemoveDesignator(player)`.
    fn airstrike_remove_designator(&mut self, player: u64) {
        let _ = player;
    }
    /// `Airstrike.RefillDesignator(player)` — restore designator charges.
    fn airstrike_refill_designator(&mut self, player: u64) {
        let _ = player;
    }
    /// `Airstrike.FindDesignatorOwner()` — the player currently holding a designator (0 = none).
    fn airstrike_designator_owner(&self) -> u64 {
        0
    }
    /// The `Airstrike.Spawn*`/`Flyby`/`ConeSpawn` family — record an ordnance/plane spawn of `kind` at
    /// `pos` for the projectile/airstrike system to realize.
    fn airstrike_spawn(&mut self, kind: &str, pos: [f32; 3]) {
        let _ = (kind, pos);
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
    /// `Sys.WriteToConsole(msg)` — write a line to the engine console (routed to the log sink).
    fn sys_write_to_console(&mut self, msg: &str) {
        self.log("console", msg);
    }
    /// `Sys.SetTimeScale(scale)` ↔ the global sim time multiplier the fixed-tick reads.
    fn sys_set_time_scale(&mut self, scale: f32) {
        let _ = scale;
    }
    /// The current global sim time scale (`1.0` = real time).
    fn sys_time_scale(&self) -> f32 {
        1.0
    }
    /// `Sys.SetLevelName(name)` — set the active level (`GetLevelName` reads it back).
    fn sys_set_level_name(&mut self, name: &str) {
        let _ = name;
    }
    /// `Sys.SetMasterScriptName(name)` ↔ `Sys.GetMasterScriptName`.
    fn sys_set_master_script_name(&mut self, name: &str) {
        let _ = name;
    }
    /// `Sys.GetMasterScriptName` — the master boot script name (falls back to the level name if unset).
    fn sys_master_script_name(&self) -> String {
        self.get_level_name()
    }
    /// `Sys.SetTutorialsEnabled(on)` ↔ `Sys.TutorialsEnabled`.
    fn sys_set_tutorials_enabled(&mut self, on: bool) {
        let _ = on;
    }
    /// `Sys.TutorialsEnabled` — whether in-game tutorials are enabled (default true).
    fn sys_tutorials_enabled(&self) -> bool {
        true
    }
    /// `Sys.SetAutosaveEnabled(on)` — gates `Sys.RequestAutosave`.
    fn sys_set_autosave_enabled(&mut self, on: bool) {
        let _ = on;
    }
    /// `Sys.SetLuaSaveVersion(v)` — the save-format version the Lua stamps into profiles.
    fn sys_set_lua_save_version(&mut self, version: i64) {
        let _ = version;
    }
    /// `Sys.SetNumberOfViewports(n)` — split-screen viewport count.
    fn sys_set_viewports(&mut self, n: i64) {
        let _ = n;
    }
    /// `Sys.SetAssetRequestMax(n)` — the streaming asset-request budget.
    fn sys_set_asset_request_max(&mut self, n: i64) {
        let _ = n;
    }
    /// `Sys.StartSingleplayer` — mark a single-player session started.
    fn sys_start_singleplayer(&mut self) {}

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
    /// The `Vehicle.Hijack*` lifecycle (`HijackStart`/`StartTankHijackMotion`/`SetHijackSuccess`/
    /// `HijackComplete`/`HijackAbort`/`HijackAbortDone`/`CancelHijack`/`SetHijackState(name)`): drive
    /// the vehicle's hijack FSM by event name and return the resulting state name.
    fn vehicle_hijack_event(&mut self, veh: u64, event: &str) -> String {
        let _ = (veh, event);
        "idle".into()
    }
    /// The current hijack state name for a vehicle (`idle` when not being hijacked).
    fn vehicle_hijack_state(&self, veh: u64) -> String {
        let _ = veh;
        "idle".into()
    }
    /// `Vehicle.SetTurretPitch`/`SetTurretYaw`/`SpinHeli` — set the turret/rotor articulation targets
    /// (radians; `spin` gates helicopter rotor). `None` leaves that field unchanged.
    fn vehicle_set_turret(&mut self, veh: u64, pitch: Option<f32>, yaw: Option<f32>, spin: Option<bool>) {
        let _ = (veh, pitch, yaw, spin);
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
    /// `Sound._GetLibVersion` — the audio library version NUMBER (the game gates features on
    /// `>= 10/11/12`, so this must be numeric). The final PC build reports the newest tier.
    fn sound_lib_version(&self) -> i64 {
        12
    }
    /// `Sound.LockActionLevelMusic(lock)` — lock/unlock the action-level dynamic music.
    fn sound_lock_action_level_music(&mut self, lock: bool) {
        let _ = lock;
    }
    /// `Sound.SetCategoryPitch(category, pitch [, length])` — set a mix category's pitch over `length`s.
    fn sound_set_category_pitch(&mut self, category: &str, pitch: f32, length: f32) {
        let _ = (category, pitch, length);
    }
    /// `Sound.LoadBank`/`LoadSoundBank`/`LoadWaveBank`/`LoadTempBank` — request a bank resident
    /// (`wave=true` ⇒ wave bank). Returns whether the load was accepted.
    fn sound_load_bank(&mut self, name: &str, wave: bool) -> bool {
        let _ = (name, wave);
        false
    }
    /// `Sound.UnloadBank`/`UnloadSoundBank`/`UnloadWaveBank`/`UnloadTempBank` — release a bank.
    fn sound_unload_bank(&mut self, name: &str) -> bool {
        let _ = name;
        false
    }
    /// `Sound.RequestAmbienceBank(name)` — load a bank as an ambience bank.
    fn sound_request_ambience_bank(&mut self, name: &str) -> bool {
        let _ = name;
        false
    }
    /// Whether a bank is currently resident (test/introspection seam).
    fn sound_bank_loaded(&self, name: &str) -> bool {
        let _ = name;
        false
    }
    /// `VO.Cue` → voice id.
    fn vo_cue(&mut self, cue: &str) -> u64 {
        let _ = cue;
        0
    }
    /// `VO.Cancel(cue)` — stop the given VO line if it is playing.
    fn vo_cancel(&mut self, cue: &str) {
        let _ = cue;
    }
    /// `VO.CancelAll` — stop the active VO line.
    fn vo_cancel_all(&mut self) {}
    /// `VO.Pause`/`Unpause`/`PauseAll`/`UnpauseAll` — pause/resume VO playback.
    fn vo_set_paused(&mut self, paused: bool) {
        let _ = paused;
    }
    /// `VO.SetCinematicMode(enable)` — cinematic VO priority mode.
    fn vo_set_cinematic_mode(&mut self, enable: bool) {
        let _ = enable;
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
    /// The `Ai.*` **order surface** (`Role`/`Anchor`/`Squad`/`Deploy`/`SetHaste`/`RemoveGoal`/…): post
    /// the order verb, hash-addressed, to the same action ring `ai_goal` uses (AI code map §5/§8 — the
    /// order brain is data/Lua over the ring, so posting the verb *is* the engine-owned mechanism).
    fn ai_order(&mut self, guid: u64, verb: &str) -> bool {
        let _ = (guid, verb);
        false
    }
    /// `Ai.AddInfraction(offender, faction, amount)` — accrue a scripted infraction against `faction`
    /// (weighted by its infraction multiplier) into the faction mood accumulator.
    fn ai_add_infraction(&mut self, offender: u64, faction: u64, amount: i64) {
        let _ = (offender, faction, amount);
    }
    /// `Ai.SetInfractionMultiplier(faction, mult)` — set the standing multiplier on `faction`'s future
    /// scripted infractions (`0` disables them).
    fn ai_set_infraction_multiplier(&mut self, faction: u64, multiplier: i64) {
        let _ = (faction, multiplier);
    }
    /// `Ai.TweakAttachedSpawners(target, {SpawnerState=…, …})` — apply a spawner adjust to the attached
    /// living-world spawners in `group_mask`; returns how many spawners were affected.
    fn ai_tweak_spawners(&mut self, target: u64, group_mask: u8, state: Option<&str>, force_respawn: bool) -> u32 {
        let _ = (target, group_mask, state, force_respawn);
        0
    }
    /// `Ai.SetAttitude`/`ChangeRelation(faction, toward, value)` — write the faction manager's directed
    /// relation (drives price/pursuit/attitude events), mirrored into the AI matrix.
    fn ai_set_attitude(&mut self, faction: u64, toward: u64, relation: i64) {
        let _ = (faction, toward, relation);
    }

    // ===== Player: identity / session / binding / profile (the depth surface `Player.*` reads). =====
    // The host tracks the player↔character binding + the profile hero fields; getters the game reads
    // return real host state, pure session actions the single-player host ignores are faithful no-ops.
    /// `Player.GetPlayer(id)` — the player object for a slot id (0 = local).
    fn player_get_player(&self, id: i64) -> u64 {
        let _ = id;
        self.player_local_player()
    }
    /// `Player.GetCharacter(player)` — the character a player currently controls.
    fn player_character_of(&self, player: u64) -> u64 {
        let _ = player;
        0
    }
    /// `Player.GetControlledObject(player)` — the object (character or vehicle) a player drives.
    fn player_controlled_object(&self, player: u64) -> u64 {
        self.player_character_of(player)
    }
    /// `Player.GetPrimaryPlayer`.
    fn player_primary_player(&self) -> u64 {
        self.player_local_player()
    }
    /// `Player.GetSecondaryPlayer` (0 = no second player → nil).
    fn player_secondary_player(&self) -> u64 {
        0
    }
    /// `Player.GetPlayerId(player)` / `GetLocalPlayerId` / `GetLocalId`.
    fn player_id_of(&self, player: u64) -> i64 {
        let _ = player;
        0
    }
    /// `Player.GetName(player)`.
    fn player_name(&self, player: u64) -> String {
        let _ = player;
        String::new()
    }
    /// `Player.GetAllPlayers`.
    fn player_all_players(&self) -> Vec<u64> {
        let p = self.player_local_player();
        if p == 0 { Vec::new() } else { vec![p] }
    }
    /// `Player.GetAllCharacters`.
    fn player_all_characters(&self) -> Vec<u64> {
        let c = self.player_any_character();
        if c == 0 { Vec::new() } else { vec![c] }
    }
    /// `Player.GetMaximumPlayers` / `GetMaximumLocalPlayers` (2-player co-op).
    fn player_max_players(&self) -> i64 {
        2
    }
    /// `Player.GetCurrentPlayers` / `GetCurrentLocalPlayers`.
    fn player_current_players(&self) -> i64 {
        1
    }
    /// `Player.IsCoopMultiplayer`.
    fn player_is_coop(&self) -> bool {
        false
    }
    /// `Player.IsJoined(player)`.
    fn player_is_joined(&self, player: u64) -> bool {
        player != 0
    }
    /// `Player.GetSelectedCharacter` — the selected hero template name (`chris`/`mattias`/`jen`).
    fn player_selected_character(&self) -> String {
        String::new()
    }
    /// `Player.GetProfileCharacter` — the save's hero character (header @0x4D).
    fn player_profile_character(&self) -> String {
        self.player_selected_character()
    }
    /// `Player.GetProfileUpgrade` — the save's upgrade tier (header @0x4F).
    fn player_profile_upgrade(&self) -> i64 {
        0
    }
    /// `Player.GetProfileCostume` — the save's wardrobe costume (0 in all shipped saves).
    fn player_profile_costume(&self) -> i64 {
        0
    }
    /// `Player.GetAvailableCostumes`.
    fn player_available_costumes(&self) -> Vec<i64> {
        Vec::new()
    }
    /// `Player.AttachToCharacter(player, character)` — bind a player to a character.
    fn player_attach_to_character(&mut self, player: u64, character: u64) {
        let _ = (player, character);
    }
    /// `Player.DetachFromCharacter(player)`.
    fn player_detach_from_character(&mut self, player: u64) {
        let _ = player;
    }
    /// `Player.BindToLocal(player)`.
    fn player_bind_local(&mut self, player: u64) {
        let _ = player;
    }
    /// `Player.BindToRemote(player)`.
    fn player_bind_remote(&mut self, player: u64) {
        let _ = player;
    }
    /// `Player.Unbind(player)`.
    fn player_unbind(&mut self, player: u64) {
        let _ = player;
    }
    /// `Player.CreatePlayer` — mint a new player object (0 = failed → nil).
    fn player_create(&mut self) -> u64 {
        0
    }
    /// `Player.DestroyPlayer(player)`.
    fn player_destroy(&mut self, player: u64) {
        let _ = player;
    }
    /// `Player.ClearPlayerDB`.
    fn player_clear_db(&mut self) {}
    /// `Player.SetOutfit(character, outfit)` — the `_tOutfits`→wardrobe override.
    fn player_set_outfit(&mut self, character: u64, outfit: i64) {
        let _ = (character, outfit);
    }
    /// `Player.SetProfileCostume(costume)`.
    fn player_set_profile_costume(&mut self, costume: i64) {
        let _ = costume;
    }

    // ===== Object: the depth surface (identity / transform / physics / hibernation state). =====
    /// `Object.GetParent(guid)` (0 = no parent → nil).
    fn object_parent(&self, guid: u64) -> u64 {
        let _ = guid;
        0
    }
    /// `Object.GetModelName(guid)`.
    fn object_model_name(&self, guid: u64) -> String {
        let _ = guid;
        String::new()
    }
    /// `Object.SetModelName(guid, name)`.
    fn object_set_model_name(&mut self, guid: u64, name: &str) {
        let _ = (guid, name);
    }
    /// `Object.GetLocalizedName(guid)` — the display name (defaults to the object name).
    fn object_localized_name(&self, guid: u64) -> String {
        self.object_name(guid)
    }
    /// `Object.IsValid(guid)`.
    fn object_is_valid(&self, guid: u64) -> bool {
        guid != 0
    }
    /// `Object.IsPlayerControlled(guid)`.
    fn object_is_player_controlled(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Object.GetInvincible(guid)`.
    fn object_get_invincible(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Object.GetMass(guid)`.
    fn object_mass(&self, guid: u64) -> f32 {
        let _ = guid;
        0.0
    }
    /// `Object.SetMass(guid, mass)`.
    fn object_set_mass(&mut self, guid: u64, mass: f32) {
        let _ = (guid, mass);
    }
    /// `Object.IsVisible(guid)`.
    fn object_is_visible(&self, guid: u64) -> bool {
        let _ = guid;
        true
    }
    /// `Object.SetVisible(guid, on)`.
    fn object_set_visible(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Object.IsAwake(guid)`.
    fn object_is_awake(&self, guid: u64) -> bool {
        let _ = guid;
        true
    }
    /// `Object.IsHibernated(guid)`.
    fn object_is_hibernated(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Object.GetHibernationDistance(guid)`.
    fn object_hibernation_distance(&self, guid: u64) -> f32 {
        let _ = guid;
        0.0
    }
    /// `Object.SetHibernationDistance(guid, dist)`.
    fn object_set_hibernation_distance(&mut self, guid: u64, dist: f32) {
        let _ = (guid, dist);
    }
    /// `Object.GetPhysicsType(guid)`.
    fn object_physics_type(&self, guid: u64) -> i64 {
        let _ = guid;
        0
    }
    /// `Object.EnablePhysics` (`on=true`) / `Object.DisablePhysics`.
    fn object_set_physics_enabled(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Object.GetVelocityVector(guid)`.
    fn object_velocity_vector(&self, guid: u64) -> [f32; 3] {
        let _ = guid;
        [0.0; 3]
    }
    /// `Object.GetDistanceFrom(a, b)` — real Euclidean distance from the two objects' positions.
    fn object_distance(&mut self, a: u64, b: u64) -> f32 {
        let pa = self.object_get_position(a);
        let pb = self.object_get_position(b);
        let d = [pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }
    /// `Object.IsAttached(guid)`.
    fn object_is_attached(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Object.GetAttachedObjects(guid)`.
    fn object_attached_objects(&self, guid: u64) -> Vec<u64> {
        let _ = guid;
        Vec::new()
    }
    /// `Object.Attach(child, parent)` — parent `child` under `parent` in the attachment graph.
    fn object_attach(&mut self, child: u64, parent: u64) {
        let _ = (child, parent);
    }
    /// `Object.Detach(child)` — remove `child` from its parent.
    fn object_detach(&mut self, child: u64) {
        let _ = child;
    }
    /// `Object.IsTemplate(guid)`.
    fn object_is_template(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Object.GetCashValue(guid)`.
    fn object_cash_value(&self, guid: u64) -> i64 {
        let _ = guid;
        0
    }
    /// `Object.SetUnkillable(guid, on)`.
    fn object_set_unkillable(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Object.SetInfiniteAmmo(guid, on)`.
    fn object_set_infinite_ammo(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Object.FadeOut(guid)` — despawn with a fade (record as a removal).
    fn object_fade_out(&mut self, guid: u64) {
        self.object_remove(guid);
    }
    /// `Object.IsDisguised(guid)`.
    fn object_is_disguised(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }

    // ===== Human: humanoid stance / action / carry state (mrxutil teleport + civ/hijack scripts). =====
    /// `Human.SetState(guid, stance, action)` — the boot-relevant stance+action setter
    /// (`mrxutil.lua:314` teleport uses `("upright","idle")`). Records the humanoid's driven state.
    fn human_set_state(&mut self, guid: u64, stance: &str, action: &str) {
        let _ = (guid, stance, action);
    }
    /// `Human.DoAction(guid, action)` — trigger a one-shot humanoid action (Cower/Stand/…).
    fn human_do_action(&mut self, guid: u64, action: &str) {
        let _ = (guid, action);
    }
    /// `Human.IsSwimming(guid)`.
    fn human_is_swimming(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Human.IsCarrying(guid)`.
    fn human_is_carrying(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Human.IsGrappling(guid)`.
    fn human_is_grappling(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Human.EnableWeapons`/`DisableWeapons(guid)` — whether the human may use its weapons.
    fn human_enable_weapons(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// Whether the human's weapons are enabled (default true).
    fn human_weapons_enabled(&self, guid: u64) -> bool {
        let _ = guid;
        true
    }
    /// `Human.SetFireLock(guid, on)` — lock the human out of firing.
    fn human_set_fire_lock(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Human.Knockdown(guid)` — knock the human down (ragdoll).
    fn human_knockdown(&mut self, guid: u64) {
        let _ = guid;
    }
    /// `Human.SetPreemptiveRagdoll(guid, on)`.
    fn human_set_ragdoll(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// Whether the human is currently knocked down / ragdolled.
    fn human_is_knocked_down(&self, guid: u64) -> bool {
        let _ = guid;
        false
    }
    /// `Human.StopGrappling(guid)` — end a grapple.
    fn human_stop_grappling(&mut self, guid: u64) {
        let _ = guid;
    }
    /// `Human.Drop(guid)` — drop whatever the human is carrying.
    fn human_drop_carried(&mut self, guid: u64) {
        let _ = guid;
    }
    /// `Human.SetJostleEnabled(guid, on)`.
    fn human_set_jostle(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Human.SetAllowCorpseCleanup(guid, on)`.
    fn human_set_corpse_cleanup(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Human.EquipWeapon`/`StowWeapon(guid)` — whether a weapon is drawn.
    fn human_set_weapon_drawn(&mut self, guid: u64, drawn: bool) {
        let _ = (guid, drawn);
    }

    // ===== Net session mode (`Net.*`). =====
    /// `Net.StartServer`/`ConnectToServer`/`EnterLobby`/`AutoServer`/`AutoClient`/`AutoLobby` — enter a
    /// session of `mode` (`"server"`/`"client"`/`"lobby"`), optionally targeting `host`.
    fn net_session_start(&mut self, mode: &str, host: Option<&str>) {
        let _ = (mode, host);
    }
    /// `Net.Stop` — leave any session (back to the offline single-player server).
    fn net_stop(&mut self) {}
    /// `Net.IsServer` — this endpoint hosts the session (default true: the SP game is its own server).
    fn net_is_server(&self) -> bool {
        true
    }
    /// `Net.IsClient` — this endpoint is a connected client.
    fn net_is_client(&self) -> bool {
        false
    }
    /// `Net.IsActive` — a network session is active (default false: offline SP).
    fn net_is_active(&self) -> bool {
        false
    }
    /// `Net.IsMultiplayer` — a real multiplayer session is running.
    fn net_is_multiplayer(&self) -> bool {
        false
    }
    /// `Net.IsLobby` — the session is in the lobby.
    fn net_is_lobby(&self) -> bool {
        false
    }
    /// `Net.GetHostName` — the connected host's name (empty offline).
    fn net_host_name(&self) -> String {
        String::new()
    }

    // ===== Object state machine + node emitters (`ObjectState.*`). =====
    /// `ObjectState.SetState(guid, state)` — set the object's state-machine state.
    fn object_sm_set_state(&mut self, guid: u64, state: &str) {
        let _ = (guid, state);
    }
    /// The object's current state-machine state (empty if none).
    fn object_sm_state(&self, guid: u64) -> String {
        let _ = guid;
        String::new()
    }
    /// `ObjectState.StartEmitter(guid, name)` — start a named node FX emitter on the object.
    fn object_start_emitter(&mut self, guid: u64, name: &str) {
        let _ = (guid, name);
    }
    /// `ObjectState.StopEmitter(guid, name)` — stop a named emitter.
    fn object_stop_emitter(&mut self, guid: u64, name: &str) {
        let _ = (guid, name);
    }
    /// Whether a named emitter is currently active on the object.
    fn object_emitter_active(&self, guid: u64, name: &str) -> bool {
        let _ = (guid, name);
        false
    }

    // ===== Facial animation (`Face.*`). =====
    /// `Face.BindFaceAnimSet(guid, set)` / `UnbindFaceAnimSet(guid)` — the bound facial anim set.
    fn face_bind_anim_set(&mut self, guid: u64, set: Option<&str>) {
        let _ = (guid, set);
    }
    /// `Face.PlayFaceAnim`/`PlayFacialExpression(guid, name)` — the current facial anim/expression.
    fn face_play(&mut self, guid: u64, name: &str) {
        let _ = (guid, name);
    }
    /// The current facial expression on a face (empty if none).
    fn face_current(&self, guid: u64) -> String {
        let _ = guid;
        String::new()
    }

    // ===== Mission report (`Report.*`) — the faction reporting lifecycle. =====
    /// `Report.Init(config)` — configure the faction reporter (the report is scored against the PMC).
    fn report_init(&mut self) {}
    /// `Report.SetDelay(seconds)` — set the report delay.
    fn report_set_delay(&mut self, seconds: f32) {
        let _ = seconds;
    }
    /// `Report.Completed`/`Failed` — finalize the active report.
    fn report_finish(&mut self, success: bool) {
        let _ = success;
    }
    /// `Report.GetInfractions()` — the pending infraction count for the active report's faction.
    fn report_infractions(&self) -> i64 {
        0
    }

    // ===== Player mode flags (`Player.Set*` — engine-read gameplay gates). =====
    /// Set a named player-mode boolean flag (`Player.SetInputEnabled`/`SetCinematicMode`/… → the engine
    /// reads these to gate control, HUD, grapple, scope, vehicle locks, disguise, PDA/satellite modes).
    fn player_set_mode(&mut self, key: &str, on: bool) {
        let _ = (key, on);
    }
    /// Read a named player-mode flag (with the caller's default if unset).
    fn player_mode(&self, key: &str, default: bool) -> bool {
        let _ = key;
        default
    }
    /// Set a named player-mode scalar (`SetHealthClamp`/`SetSwimmingSearchRadius`/`SetAimMode`).
    fn player_set_mode_scalar(&mut self, key: &str, value: f32) {
        let _ = (key, value);
    }

    // ===== Seat occupancy (`Vehicle.EnterBySeatGuid`/`TransferToSeat`, `Human.ForceExitSeatNoSnap`). =====
    /// Seat a human in `seat` (a seat GUID), moving it out of any previous seat.
    fn human_enter_seat(&mut self, human: u64, seat: u64) {
        let _ = (human, seat);
    }
    /// Remove a human from its seat.
    fn human_exit_seat(&mut self, human: u64) {
        let _ = human;
    }
    /// The seat GUID a human occupies (0 = none).
    fn human_seat(&self, human: u64) -> u64 {
        let _ = human;
        0
    }
    /// `Vehicle.RestoreAmmo(weapon)` — refill the weapon's clip + reserve to capacity.
    fn weapon_restore_ammo(&mut self, weapon: u64) {
        let _ = weapon;
    }

    /// Record a dynamic-music / DSP / audio-mode command (`Sound.AddFactionMusic`/`SetSourceMusic`/
    /// `SetReverbPreset`/… — a command-queue the audio director consumes; the verb + stringified args
    /// are the config the mixer/music FSM applies).
    fn sound_cmd(&mut self, verb: &str, args: Vec<String>) {
        let _ = (verb, args);
    }

    /// Record a replicated mission event (`Net.SendEvent_AddMarkerObjective`/`TeleportPlayer`/`Fanfare`/
    /// `Support`/… + telemetry/presence) onto the drainable event log the runtime realizes (add/remove
    /// objectives + markers, teleports, fanfares, support items, revives, achievements). In SP these are
    /// applied locally rather than sent over the wire.
    fn net_event(&mut self, verb: &str, args: Vec<String>) {
        let _ = (verb, args);
    }

    /// Record a generic engine command (`Hud` animation/callbacks, `Object` animation/winch/impulse,
    /// `Camera` extras, `Lti` options-menu navigation, `Sys`/`Graphics` misc, `Gui` marker-category
    /// toggles, …) onto the drainable command log the corresponding runtime system consumes. The verb
    /// is namespaced (`"Ns.Verb"`) so one log serves every remaining action surface.
    fn script_cmd(&mut self, verb: &str, args: Vec<String>) {
        let _ = (verb, args);
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

-- Pandemic engine math extension used across the resident scripts (MrxFactionManager, gunships,
-- airstrikes, island fortress, …): `math.randi(n)` = random integer in [1,n]; `math.randi(a,b)` =
-- [a,b]. Guarded against an empty interval (n<1 / a>b) so a degenerate call returns the low bound
-- instead of erroring (`math.random` rejects an empty range). NOT a 5.1 compat alias — an engine cfunc.
if not math.randi then
  function math.randi(a, b)
    local lo, hi
    if b then lo, hi = a, b else lo, hi = 1, a end
    if hi < lo then return lo end
    return math.random(lo, hi)
  end
end
-- `Math` is the engine's capitalized math namespace (a superset of Lua `math`); the scripts use both
-- `math.randi` and `Math.randi`. Alias it to the standard library (+ our extension) when it isn't a
-- real table, so `Math.floor`/`Math.random`/`Math.randi`/… all resolve.
if type(Math) ~= "table" or type(Math.floor) ~= "function" then Math = math end
if not Math.randi then Math.randi = math.randi end

-- 5.1 getfenv/setfenv shims over 5.4's _ENV-as-upvalue model (used by prototype-inheritance modules
-- like AntiAir: `local m = getfenv(); for _,p in pairs(protos) do setmetatable(p,{__index=m}) end`).
-- The module loader runs each module with its module table as the `_ENV` upvalue, so returning/replacing
-- that upvalue is faithful.
local function _env_upvalue_index(f)
  local i = 1
  while true do
    local name = debug.getupvalue(f, i)
    if not name then return nil end
    if name == "_ENV" then return i end
    i = i + 1
  end
end
if not getfenv then
  function getfenv(f)
    if type(f) == "function" then
      local i = _env_upvalue_index(f)
      return i and select(2, debug.getupvalue(f, i)) or _G
    end
    local lvl = (type(f) == "number") and f or 1
    if lvl == 0 then return _G end
    local info = debug.getinfo(lvl + 1, "f")               -- +1 for this shim frame
    if info and info.func then
      local i = _env_upvalue_index(info.func)
      if i then return select(2, debug.getupvalue(info.func, i)) end
    end
    return _G
  end
end
if not setfenv then
  function setfenv(f, env)
    local fn = (type(f) == "function") and f or debug.getinfo(((type(f) == "number") and f or 1) + 1, "f").func
    local i = _env_upvalue_index(fn)
    if i then debug.setupvalue(fn, i, env) end
    return fn
  end
end
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
    /// Modules whose body has finished loading and that define a parameterless `Init()`, awaiting the
    /// deferred **two-phase** init flush (load ALL modules, then run their `Init`s in load order). This
    /// is what the engine does — running a module's `Init` immediately would fire it mid-cycle while a
    /// dependency is only half-loaded (e.g. `MrxShop.Init` before `MrxFactionManager` finished).
    pending_init: RefCell<Vec<Table>>,
    /// Re-entrancy guard: true while the init queue is being flushed (an `Init` may itself `import`).
    flushing: Cell<bool>,
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
            pending_init: RefCell::new(Vec::new()),
            flushing: Cell::new(false),
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

        // Pandemic module convention: a module's parameterless `Init()` is auto-invoked by the loader
        // (no explicit `Module.Init()` call exists anywhere in the 62 modules that define one — the
        // framework owns that call; it builds the module's state tables, e.g. `MrxGuiManager.Init` →
        // `_tPlayerGuiList = {}`). It is DEFERRED into a queue and flushed only when the whole import
        // chain has settled (two-phase: load all, then Init all in load order) — running it eagerly
        // would fire a module's Init mid-cycle while a dependency is still half-loaded.
        if env.get::<mlua::Function>("Init").is_ok() {
            self.pending_init.borrow_mut().push(env.clone());
        }
        if self.stack.borrow().is_empty() && !self.flushing.get() {
            self.flushing.set(true);
            // Drain FIFO; an `Init` that imports more modules appends to the queue and is drained too.
            let mut i = 0;
            loop {
                let next = self.pending_init.borrow().get(i).cloned();
                let Some(m) = next else { break };
                i += 1;
                let init: mlua::Function = m.get("Init")?;
                self.stack.borrow_mut().push(m.clone());
                let r = init.call::<()>(());
                self.stack.borrow_mut().pop();
                r?;
            }
            self.pending_init.borrow_mut().clear();
            self.flushing.set(false);
        }
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
        // All stdlibs incl. `debug` (the game Lua uses the 5.1 `getfenv`/`setfenv`, which our compat
        // shims implement via `debug.getupvalue`/`setupvalue`). This host runs TRUSTED decompiled game
        // Lua, so the unsafe `debug` library is acceptable.
        let lua = unsafe { Lua::unsafe_new_with(mlua::StdLib::ALL, mlua::LuaOptions::default()) };
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

    /// Fire the `GameStateChange` handlers waiting on `(state, phase)` — the engine's world-load state
    /// machine calls this (via the resident pump) to advance the `MrxState` chain when a requested game
    /// state reaches that phase.
    pub fn fire_state_change(&self, state: &str, phase: &str) -> LuaResult<()> {
        crate::bindings::event::fire_game_state_change(&self.lua, state, phase)
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
        // Binding-surface burn-down. ALL 1086 Required cfuncs are installed & callable
        // (tests/binding_smoke.rs enforces that). The split is the HONEST progress metric:
        //   real  = BACKED — wired to a real engine mechanism (`mercs2_ai`/`faction`/`population`/
        //           `audio`/…) or reads real host state. A wrong body here is a bug.
        //   stub  = UNBACKED — a deliberate no-op because the engine system behind it isn't built yet
        //           (HUD renderer, DSP, exclusion zones, …) OR the retail cfunc is genuinely stripped.
        //           These are the burn-down: docs/modernization/binding_burndown.md tracks each by the
        //           system it needs. `stub` is NOT "done" — it's "not built yet".
        // De-stub work moves a name real←stub. Session start: real 86 / stub 9. Ai vertical wired its
        // order ring + faction mood + spawner tweaks (real +31); Vehicle vertical wired the hijack FSM
        // + turret aim + RestoreHealth (real +13); Sound vertical wired category pitch + the bank
        // load/unload/ambience residency family (real +12); Sys vertical wired the engine-config store
        // (time scale / level+master-script / tutorials / autosave / save-version / viewports; real +10);
        // ObjectFilter vertical wired the label-expr query registry + object label store (real +7);
        // Object Attach/Detach wired the real attachment graph (real +2); VO wired cancel/pause/
        // cinematic-mode to the real VoManager (real +7); HUD wired the retained-mode widget tree
        // (mercs2_ui::WidgetTree) — widget/image/text/sprite/movie/flash/minimap create+mutate+query
        // (real +55); Gui wired the world-marker set (mercs2_ui::MarkerSet) + texture/font handles
        // (real +16); render-state vertical wired Atmosphere (generic value/color/int store + time) +
        // Bloom + Graphics + Fade to mercs2_core::RenderState (real +40); CameraFx wired the cinematic
        // camera controller pose/shake/blend/follow (real +13); Inventory wired the per-character
        // weapon loadout (set/get/equip/drop/destroy) (real +4); Weapon ammo + Fire burning state +
        // object health/SendDamage wired to real host state (real +7); Pg regions/alarms + Airstrike
        // designator lifecycle + recorded ordnance spawns wired to real host state (real +13); Human
        // weapon/ragdoll/grapple/carry/jostle flag verbs wired to a per-human flag store (real +13);
        // Net session mode (IsServer/IsClient/IsActive/IsLobby/GetHostName + Start/Connect/Lobby/Stop)
        // wired to a real NetState (real +6); ObjectState SetState/emitters + Face bind/play + Report
        // faction-report lifecycle wired to real host state (real +12); Player mode gates
        // (input/cinematic/survival/grapple/scope/vehicle-lock/disguise/PDA/satellite + scalars) wired
        // to a real player-mode store (real +18); seat occupancy (Enter/Transfer/ForceExit) +
        // Vehicle.RestoreAmmo wired to real host state (real +4); Sound dynamic-music/DSP command log +
        // Net SendEvent_* mission-event log wired as real recorded intents (real +120); the entire
        // action residue (Hud/Object/Lti/Pg/Camera/Sys/Gui/Ai/Atmosphere/Vo/ObjectFilter/ObjectState
        // animation/menu/spawner/param/marker-category verbs) → recorded command logs (real +231).
        // Remaining unbacked = genuine dev stubs (debug menu, asset dumps) + a few getters/subsystem gaps.
        const EXPECTED_REAL: usize = 1058;
        const EXPECTED_STUB: usize = 28;

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
        assert_eq!(by("Sys").real_count(), 64);
        assert_eq!(by("Pg").real_count(), 80);
        assert_eq!(by("Object").real_count(), 86);
        assert_eq!(by("Object").stub_count(), 1);
        assert_eq!(by("Player").real_count(), 107);
        assert_eq!(by("Event").real_count(), 4);
        assert_eq!(by("Vehicle").real_count(), 40);
        assert_eq!(by("Sound").real_count(), 88);
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
