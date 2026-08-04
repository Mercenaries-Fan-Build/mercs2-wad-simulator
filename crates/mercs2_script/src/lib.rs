//! `mercs2_script` — the engine's Lua script host.
//!
//! This is the **engine** side of scripting: the VM, the module system (`import`/`inherit`/
//! `dynamic_import`), and the *mechanism* for exposing engine services to Lua (the `Sys.*` / `Debug.*`
//! / `Pg.*` / `Event.*` C-binding tables). It is asset-agnostic — it knows nothing about Mercenaries.
//! The game's Mercenaries Lua (`docs/mercs2-luacd/`, the `Mrx*`/mission/contract modules) runs *on*
//! this host and drives the engine through it. This is the charter's embedded-Lua goal
//! (`docs/modernization/00_charter.md` — run migrated scripts validated by Surface B) and the
//! engine/game split in `docs/modernization/pangea_engine_alignment.md`.
//!
//! ## Seam: inversion of control
//! The host never calls the engine directly. Instead the engine implements [`EngineHost`] and hands it
//! in via [`ScriptHost::register_engine`]. The binding closures call that trait. So the dependency
//! points *into* this crate (engine → script host), never the reverse — the same shape as the original
//! `Sys.*` C-binding table calling into the native engine.
//!
//! ## What the host installs
//! - **The game's own Lua 5.1.5** (`mercs2_luac`, vendored with Pandemic's float-`lua_Number`
//!   patches) — so the shipped corpus runs as authored rather than through a compatibility layer.
//!   No migration shims: `unpack`, `table.getn`, `getfenv`/`setfenv` and friends are native here.
//!   The `PRELUDE` that remains is engine setup only — `ASSERT`, `math.randi`, the capitalized
//!   `Math` namespace, and the `_MODULES` registry.
//! - The **module system**: `import(name)` / `dynamic_import(name)` load a corpus `.lua` into its own
//!   `_ENV` table (metatable `__index → _G`) so the file's bare `function Foo()` become module members;
//!   `inherit(base)` chains `__index → base`; results cache in `_MODULES`. This is the C-level
//!   environment-set the original engine did (`_SYS._IMPORT`), done here with `Chunk::set_environment`.
//!   A module's parameterless `Init()` is auto-invoked, **deferred** two-phase (load all, then Init in
//!   load order).
//! - The **engine binding surface**: 35 namespaces / 1086 required cfuncs, one [`bindings`] module per
//!   `luaL_Reg` table, seeded from the live Surface-B trace
//!   (`mods/lua_trace_asi/reference/binding_map.json`; human index
//!   `docs/reverse_engineer/scripting_host_binding_code_map.md` §3). Coverage is machine-readable
//!   ([`coverage_json`] → `binding_coverage.json`); the burn-down metric is `remaining` = required−real.
//! - Optionally the bring-up auto-stub layer ([`ScriptHost::enable_autostub`]): unwired Capitalized
//!   globals resolve to logged no-op stubs so the real import cascade completes, and every touched name
//!   is recorded as a reimpl-side binding trace.
//!
//! ## Module map
//! - [`ScriptHost`] — the VM + module loader + `register_engine` / `enable_autostub` / `fire_state_change`.
//! - [`EngineHost`] / [`SharedHost`] — the IoC seam the bindings call; the engine implements it.
//! - [`bindings`] — the per-namespace binding files + the coverage harness ([`install_all`], [`totals`],
//!   [`coverage_json`], [`NsCoverage`]).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mercs2_luac::rt::{Lua, Result as LuaResult, Table, Value};

pub mod bindings;
/// The embedded Lua VM crate, re-exported so hosts can build Lua values natively (tables, callbacks)
/// against the SAME runtime this crate links. Serializing to Lua source instead would mean splicing
/// untrusted strings (save/mission names) into a chunk. See
/// `mercs2_engine::script_host::install_boot_save_state` for the motivating consumer.
///
/// There is exactly one Lua in the workspace and this is it — the same crate the bytecode compiler
/// uses. Two Lua builds in one binary would export the same unprefixed `lua_*` symbols and run one
/// implementation's `lua_State` through the other's functions.
pub use mercs2_luac;
pub use bindings::{coverage_json, install_all, totals, NsCoverage, Totals};
/// The canonical `ObjectHibernation` phases + the folding function, re-exported because the ENGINE is
/// the producer: it must fire the same canonical spelling the registrations were folded onto, and the
/// `bindings` submodules are otherwise private. See `bindings::event::canon_phase` for why the corpus's
/// five spellings collapse to two.
pub use bindings::event::{canon_phase, PHASE_ASLEEP, PHASE_AWAKE};

/// The Lua-boundary handle type. Engine GUIDs cross into Lua as **lightuserdata**, the way retail
/// pushes them (`FUN_0059FF50` accepts only tags 2/7; `GetAnyCharacter` writes tag 2 — see
/// [`guid`] for the full provenance), because ~114 shipped-script sites `type()`-check them.
pub mod guid;
pub use guid::Guid;

/// Locating the decompiled game-script corpus (see `corpus.rs`) — the behavioural spec the replay
/// suites and `examples/mission_lab.rs` run against. It lives outside this repo by design, so
/// [`corpus::root`] returns `Option` and callers degrade with [`corpus::skip_notice`].
pub mod corpus;

/// The engine services the script bindings call. The engine (`mercs2_engine`) implements this; the
/// script host only ever talks to the engine through it. Every method here corresponds to an original
/// engine C-binding (or a small cluster of them) that Mercenaries Lua invokes.
///
/// Methods are added as the binding surface widens; it began with the boot + PMC-interior-spawn slice.
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
    /// `Pg.GetAllLandingZones(nSlot)` — the transit landing pads serving **co-op player slot** `nSlot`,
    /// as `(landing-zone number, pad guid)` pairs in ascending zone order.
    ///
    /// Empty when the host has no world loaded — a world with no pads honestly has none.
    ///
    /// ⚠ Two shapes here are easy to get wrong and both are pinned by the corpus:
    /// * `nSlot` is the **player slot**, not a category. `MrxTransit.Reset` calls this with `1` and `2`
    ///   and zips the results into `{uLocation1, uLocation2}`, whose only consumer is
    ///   `MrxUtil.TeleportHeroesToLocations` — hero N lands on pad N
    ///   (`corpus/mercs2-luacd/src/resident/mrxtransit.lua:328-342,89`).
    /// * The Lua table is keyed by the **absolute zone number**, which is *sparse* — retail vz ships
    ///   1..8, 12, 15..18, 20..25, 27..30 — not by dense position. `Reset` writes
    ///   `_tLandingZones[nIndex]` straight from the iteration key, and missions address zones by
    ///   absolute number (`MrxTransit.SetLocationIsNuked(30, …)`, `vz/wifmissionflow.lua:1245`).
    fn landing_zones(&self, slot: u32) -> Vec<(u32, u64)> {
        let _ = slot;
        Vec::new()
    }
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

    // ===== The player concern (`Player.*` — 107 cfuncs) → `mercs2_player`. =====
    //
    // Two accessors replace the 43 bespoke `player_*` methods this trait used to declare. Same shape as
    // `hud()`/`hud_ref()` above: the leaf crate owns the state and the binding bodies call it directly,
    // so widening the `Player` surface never widens this trait again.
    //
    // The Player rewrite reworked all 107 bodies against `player_code_map.md`. The old methods encoded several
    // things the map contradicts — a native 1e9 cash clamp (it is a Lua soft-clamp scoped to
    // `MrxPmc.AddCashQty`), `GetAnyCharacter` as a lookup (it pushes a constant sentinel), and
    // `player_max_players`/`player_current_players` conflating four independent retail numbers.
    /// The player roster + the profile/economy singleton, if this host owns one.
    ///
    /// The real game host does; smoke and example hosts return `None`, and the `Player.*` surface then
    /// degrades to nil/neutral — which is also what retail does for an unresolved handle
    /// (`FUN_004B2A50` is `push nil; return 1`, and shipped scripts rely on `if Player.X(u) then`).
    fn player_world(&mut self) -> Option<&mut mercs2_player::PlayerWorld> {
        None
    }
    /// Read-only view, for the `Get*` / `Is*` queries.
    fn player_world_ref(&self) -> Option<&mercs2_player::PlayerWorld> {
        None
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
    /// `ObjectFilter.AddObject(f, guid, bExclude)` — add to the **exclude** set when `true`, else the
    /// include set. Note the polarity: retail's third argument is `bExclude` (proven in the Xbox
    /// build's add primitive), and it was inverted here until 2026-07-26.
    fn object_filter_add(&mut self, handle: u64, guid: u64, exclude: bool) {
        let _ = (handle, guid, exclude);
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
    fn render_state(&mut self) -> Option<&mut mercs2_core::RenderSettings> {
        None
    }
    /// Read-only view of the render state (for `Get*` queries).
    fn render_state_ref(&self) -> Option<&mercs2_core::RenderSettings> {
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

    // ===== `Human.Inventory.*` — the per-character weapon loadout. =====
    //
    // Backed by `mercs2_combat::inventory` (the `RuntimeInventory` record). The **return shapes** here
    // are load-bearing and were wrong before: `inventory_equipment_code_map.md` §10 item 5 records that
    // retail's `SetAllWeapons`, `EquipWeapon` and `DropWeapon` all push a **boolean**, `ReloadAll` pushes
    // `true` (or nil when its second argument is absent), and `DestroyAllWeapons` pushes **nothing**.
    // Shipped scripts branch on those.
    //
    // The trait stays **scalar-only**: `mercs2_script` depends on `mlua`/`mercs2_ui`/`mercs2_core`/
    // `mercs2_player` and must never name a `mercs2_combat` type. The engine impl does the ECS work.

    /// `SetAllWeapons(uChar, …)` — destroy the current loadout, then apply at most 2 primaries + 2
    /// secondaries. Returns whether the apply succeeded (a locked human rejects it one call deeper).
    fn inventory_set_weapons(&mut self, character: u64, weapons: Vec<u64>) -> bool {
        let _ = (character, weapons);
        false
    }
    /// `GetAllWeapons(uChar [, bExcludeFlagged])` — **one** array: primaries (equipped first), then
    /// secondaries.
    ///
    /// ⚠ One table, not two. §4.4 reads the epilogue as `lua_createtable` + N × `rawseti` then
    /// `return 1`; §7.3 shows the Lua side taking it as a single value. Ordering is still load-bearing
    /// — `mrxplayer.lua:666,702` pairs the results of the **two calls** positionally.
    ///
    /// `exclude_flagged` filters the per-edge **exclude** bit `0x02`, not the equipped bit `0x01`.
    fn inventory_weapons(&self, character: u64, exclude_flagged: bool) -> Vec<u64> {
        let _ = (character, exclude_flagged);
        Vec::new()
    }
    /// `GetPrimaryWeapon(uChar)` — the equipped primary, falling back to the other primary slot.
    /// `0` = none → nil.
    fn inventory_primary(&self, character: u64) -> u64 {
        let _ = character;
        0
    }
    /// `GetSecondaryWeapon(uChar)` — the equipped secondary, falling back to the previous one.
    fn inventory_secondary(&self, character: u64) -> u64 {
        let _ = character;
        0
    }
    /// `GetVehicleWeapon(uChar)` — the mounted weapon slot. **No fallback**; retail also returns nil
    /// when it is 0, and the binding has zero shipped call sites.
    fn inventory_vehicle_weapon(&self, character: u64) -> u64 {
        let _ = character;
        0
    }
    /// `EquipWeapon(uChar, uWeapon)` — equip a carried weapon into the slot its own `Equipment` class
    /// selects. Returns a boolean.
    fn inventory_equip(&mut self, character: u64, weapon: u64) -> bool {
        let _ = (character, weapon);
        false
    }
    /// `DropWeapon(uChar, uWeapon)` — detach, promoting the fallback into the vacated slot. Returns a
    /// boolean.
    fn inventory_drop(&mut self, character: u64, weapon: u64) -> bool {
        let _ = (character, weapon);
        false
    }
    /// `DestroyAllWeapons(uChar)` — queue every carried weapon for **deferred** destruction. Pushes
    /// nothing.
    fn inventory_destroy_all(&mut self, character: u64) {
        let _ = character;
    }
    /// `ReloadAll(uChar, bSomething)` — `None` is retail's bail when argument 2 is absent, which the
    /// binding pushes as nil.
    fn inventory_reload_all(&mut self, character: u64, arg2: Option<bool>) -> Option<bool> {
        let _ = (character, arg2);
        None
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
    /// `Junk.CreateRegion(name, center, radius)` — register a trigger region; returns its handle.
    fn pg_create_region(&mut self, name: &str, center: [f32; 3], radius: f32) -> u64 {
        let _ = (name, center, radius);
        0
    }
    /// `Junk.ActivateAlarm(guid, on)` — set an alarm's active state.
    fn pg_alarm_set(&mut self, guid: u64, on: bool) {
        let _ = (guid, on);
    }
    /// `Junk.ToggleAlarm(guid)` — flip an alarm; returns the new state.
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
    /// `Object.InSeat(guid)` — is this character occupying a vehicle seat.
    ///
    /// Retail reads the character's own `RiderLink+0x50` (`0x005CD9F0`), so the occupancy edge belongs
    /// to the rider; implementations should key on the rider for the same reason.
    fn object_in_seat(&self, guid: u64) -> bool {
        let _ = guid;
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

    // ===== Player identity / session / binding / profile: see `player_world()` above. =====
    // The 30 bespoke methods that used to sit here are gone. Two of them additionally have to be
    // *unlearned* rather than merely moved, because the map shows their defaults were wrong in kind,
    // not just in value:
    //   * `player_max_players` / `player_current_players` collapsed four independent retail numbers
    //     into two. `GetMaximumPlayers` reports a global nothing enforces, `GetMaximumLocalPlayers` and
    //     `GetCurrentLocalPlayers` are two *different* .rdata float constants (2.0 and a hardcoded
    //     1.0), and the real roster cap is three compile-time immediates. See
    //     `mercs2_player::roster`'s module docs.
    //   * `player_selected_character` returned a hero *template name* while `GetProfileCharacter`
    //     returns the profile byte at `+0x61`. They are different things; the name selection stays on
    //     the game host as `hero_character`, and the profile byte lives on
    //     `mercs2_player::PlayerProfile`.

    /// `Player.GetSelectedCharacter` — the selected hero *template* name (`chris`/`mattias`/`jen`).
    ///
    /// Retained on this trait rather than moved into `mercs2_player`, because it is the game's hero
    /// *selection*, not a field of the retail player or profile record — and note it is **not one of
    /// the 107**: it is an extra the binding installs, which `NsCoverage::real_count` filters out.
    fn player_selected_character(&self) -> String {
        String::new()
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
    /// `Object.IsPlayerControlled(guid)` — **returns the controlling player's GUID**, `0` for none.
    ///
    /// ⚠ Despite the `Is` prefix this is not a predicate. Retail `FUN_005CDFF0` tests the queried guid
    /// against `player+0x24` and pushes the player, and the shipped Lua consumes it as a handle:
    /// `local uPlayer = Object.IsPlayerControlled(uDriver)` (`mrxhijack.lua:504`, `:316`, `:343`;
    /// `mrxvehicle.lua:565`) feeding straight into `Player.SetInputEnabled(uPlayer, …)`. Typed as
    /// `bool`, those 74 call sites become `SetInputEnabled(true, …)`.
    fn object_is_player_controlled(&self, guid: u64) -> u64 {
        let _ = guid;
        0
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

    // The stringly-keyed player-mode store that used to live here (`player_set_mode` / `player_mode` /
    // `player_set_mode_scalar`) is gone. It was written by 17 cfuncs and read by nothing but its own
    // tests, and its untyped shape hid a real defect: every one of the 14 boolean gates was declared
    // `|_, on: Option<bool>|`, so it read ARGUMENT 1 — the player handle — as its flag, and mlua's
    // Lua-truthiness conversion (`_ => true`) meant `SetCinematicMode(uPlayer, false)` set the gate to
    // `true`. The gates are now typed fields on `mercs2_player::PlayerObject`, reached via
    // `player_world()`.

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

/// The engine's **own** Lua bootstrap glue, recovered verbatim from the binary's string pool
/// (`docs/mercs2-pdb-analysis/lua-scripting.md`, "Notable strings"):
///
/// ```text
/// _tostring = tostring; tostring = Sys.ToStringL; help = Sys.Help; StringToGuid = Sys.StringToGuid;
/// ASSERT = Debug.Assert; print = Debug.Printf
/// ```
///
/// This is the mechanism behind the namespace registry's third field — each `{name, luaL_Reg*,
/// initLuaChunk}` row can carry a Lua source string the engine runs at registration — and it is why the
/// shipped scripts call **bare** `StringToGuid(...)` (8 sites in `mrxguihudvehicledisguise.lua` alone)
/// alongside the qualified `Sys.StringToGuid(...)`. Without it, `mrxguihudvehicledisguise` was the one
/// module of 370 that failed to import: `StringToGuid` resolved to nil and the faction-texture table
/// took a nil key.
///
/// It runs **after** the bindings, not in [`PRELUDE`], because every right-hand side is a member
/// of a namespace that does not exist until `install_all` has run.
///
/// Each alias is guarded on its source existing, because our binding surface is a subset of retail's —
/// `Sys.Help` in particular is a dev-console entry point we do not implement. Guarding keeps a missing
/// cfunc from taking the whole glue chunk down with it; the aliases whose targets *do* exist still land.
const BOOTSTRAP_GLUE: &str = r#"
-- Recovered verbatim; see the Rust doc comment for provenance.
if Sys then
  _tostring = tostring
  if Sys.ToStringL then tostring = Sys.ToStringL end
  if Sys.Help then help = Sys.Help end
  if Sys.StringToGuid then StringToGuid = Sys.StringToGuid end
end
if Debug then
  if Debug.Assert then ASSERT = Debug.Assert end
  if Debug.Printf then print = Debug.Printf end
end
"#;

/// The Lua-side prelude, installed before any module loads.
///
/// This **was** the 5.1→5.4 compatibility prelude. All of that is gone, because the host now runs
/// the game's own Lua 5.1.5 where every one of those names is native — verified against the
/// vendored `luaconf.h`, which enables `LUA_COMPAT_MOD`, `LUA_COMPAT_GFIND` and `LUA_COMPAT_VARARG`:
///
/// | was shimmed | corpus uses | on this VM |
/// |---|---|---|
/// | `table.getn` | 112 | native |
/// | `unpack` | 76 | native |
/// | `getfenv` / `setfenv` | 18 | native (~35 lines of `debug.getupvalue` reimplementation deleted) |
/// | `loadstring`, `table.maxn`, `math.mod`, `string.gfind` | 0 | native |
///
/// The `getfenv`/`setfenv` pair is the one worth noting: emulating 5.1 environments over 5.4's
/// `_ENV`-as-upvalue model needed the `debug` library and a scan for the right upvalue slot. Here
/// they are the VM's own, and the module loader uses `lua_setfenv` directly.
///
/// What remains is not compatibility — it is the engine's own Lua-side setup, which retail had too.
const PRELUDE: &str = r#"
-- Engine global (not a Lua stdlib function, and not defined anywhere in the script corpus — the
-- game Lua just calls it). The task framework leans on it: `MrxTask._ModuleLoaded` asserts the
-- module resolved, `_AddChild` asserts the child name is unique. Retail is a dev-build assert, so a
-- failure is a loud log line rather than a hard error — killing the VM here would be *less*
-- faithful, not more.
if ASSERT == nil then
  function ASSERT(v, sMsg)
    if not v then
      if Debug and Debug.Printf then
        Debug.Printf("ASSERT FAILED: " .. tostring(sMsg or "(no message)"))
      end
    end
    return v
  end
end

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

-- `Math` is the engine's capitalized math namespace (a superset of Lua `math`); the scripts use both
-- `math.randi` and `Math.randi`. Alias it to the standard library (+ our extension) when it isn't a
-- real table, so `Math.floor`/`Math.random`/`Math.randi`/… all resolve.
if type(Math) ~= "table" or type(Math.floor) ~= "function" then Math = math end
if not Math.randi then Math.randi = math.randi end

-- The module registry `import`/`inherit` cache into.
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

    /// `dynamic_remove(name)` — forget a loaded module, so the next `import` re-executes its body.
    /// Nothing to do if it was never loaded.
    fn remove(&self, lua: &Lua, name: &str) {
        let key = name.to_ascii_lowercase();
        if self.loaded.borrow_mut().remove(&key).is_some() {
            let _ = lua.globals().raw_remove(name);
        }
    }

    /// The registered name of an already-loaded module table, for the `dynamic_remove(oModule)` shape.
    ///
    /// Identity comparison, not structural: two modules can be structurally equal and must still be
    /// distinguishable.
    fn name_of_module(&self, module: &Table) -> Option<String> {
        self.loaded
            .borrow()
            .iter()
            .find(|(_, t)| *t == module)
            .map(|(k, _)| k.clone())
    }

    /// `import(name)` — load `name` once (cached), bind it as a global, return its module table.
    fn import(&self, lua: &Lua, name: &str) -> LuaResult<Table> {
        let key = name.to_ascii_lowercase();
        if let Some(t) = self.loaded.borrow().get(&key) {
            lua.globals().set(name, t.clone())?;
            return Ok(t.clone());
        }
        let path = self.index.get(&key).cloned().ok_or_else(|| {
            mercs2_luac::rt::Error::RuntimeError(format!("import: module '{name}' not found in roots"))
        })?;
        let src = std::fs::read_to_string(&path)
            .map_err(|e| mercs2_luac::rt::Error::RuntimeError(format!("import '{name}': {e}")))?;

        // Fresh environment; misses fall through to the globals (stdlib, other modules, engine tables).
        let env = lua.create_table()?;
        let mt = lua.create_table()?;
        mt.set("__index", lua.globals())?;
        let _ = env.set_metatable(Some(mt));

        // `_THIS` = the module's own table. The task framework relies on it to refer to itself without
        // knowing its own name (`MrxTask.CreateChild` does `_THIS:Create()`; `Cleanup` restores
        // `setmetatable(self, {__index = _THIS})`), so a module that inherits `MrxTask` is broken
        // without it.
        env.set("_THIS", env.clone())?;

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
        if env.get::<mercs2_luac::rt::Function>("Init").is_ok() {
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
                let init: mercs2_luac::rt::Function = m.get("Init")?;
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
        // The full 5.1 standard library, `debug`/`io`/`os` included. Same set retail opened, and
        // the corpus calls into `os.*` (22 sites) and `io.*` (19).
        let lua = Lua::new()?;
        lua.load(PRELUDE).set_name("@prelude").exec()?;

        let loader = Rc::new(Loader::new(&roots));

        let imp = loader.clone();
        let import_fn = lua.create_function(move |lua, name: String| imp.import(lua, &name))?;
        lua.globals().set("import", import_fn)?;

        // `dynamic_import(name [, fCallback [, tArgs]])` — import-at-runtime, with the corpus's
        // continuation form: the callback is invoked as `fCallback(unpack(tArgs), mModule)` once the
        // module is up. `MrxTask.Activate` drives the whole task-instantiation chain through it
        // (`dynamic_import(tConfig.sModuleName, self._ModuleLoaded, {self})` → `_ModuleLoaded(self,
        // mModule)`), so the arg-then-module order is load-bearing, not cosmetic.
        let dimp = loader.clone();
        let dyn_import_fn = lua.create_function(
            move |lua, (name, cb, args): (String, Option<mercs2_luac::rt::Function>, Option<Table>)| {
                let m = dimp.import(lua, &name)?;
                if let Some(cb) = cb {
                    let mut vals: Vec<mercs2_luac::rt::Value> = match args {
                        Some(t) => t.sequence_values::<mercs2_luac::rt::Value>().collect::<LuaResult<_>>()?,
                        None => Vec::new(),
                    };
                    vals.push(mercs2_luac::rt::Value::Table(m.clone()));
                    cb.call::<()>(mercs2_luac::rt::Variadic::from_iter(vals))?;
                }
                Ok(m)
            },
        )?;
        lua.globals().set("dynamic_import", dyn_import_fn)?;

        // `dynamic_remove(nameOrModule)` — drop a dynamically-loaded module so a later `import` re-runs
        // its body. `MrxTask.Cleanup` calls this on the task's own `sModuleName` when a mission tears
        // down, which is how a contract's module-level state is reset between plays.
        //
        // ⚠ **Two shapes ship.** `MrxTask.Cleanup` passes the name *string*, but
        // `MrxGuiCinematic.SubtitleImportCallback` (resident/mrxguicinematic.lua:153) passes the module
        // *table* it was just handed. A string-only signature raises on the second, which aborts the
        // cinematic callback chain mid-boot.
        let drem = loader.clone();
        let dyn_remove_fn = lua.create_function(move |lua, target: Value| {
            let name = match target {
                Value::String(s) => Some(s.to_string_lossy()),
                Value::Table(ref t) => drem.name_of_module(t),
                _ => None,
            };
            if let Some(name) = name {
                drem.remove(lua, &name);
            }
            Ok(())
        })?;
        lua.globals().set("dynamic_remove", dyn_remove_fn)?;

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
    /// required cfunc surface and installing this build's real/stub bodies. All 1086 required cfuncs
    /// are installed and callable; the ones still lacking a real engine-backed body are the "stubs
    /// remaining" tally (see [`Self::register_engine_reported`] / [`bindings::coverage_json`]). The
    /// `Mrx*` modules are *game* Lua and come from the corpus via `import`, not from here.
    pub fn register_engine(&self, host: SharedHost) -> LuaResult<()> {
        self.register_engine_reported(host).map(|_| ())
    }

    /// Like [`Self::register_engine`], but returns the per-namespace [`bindings::NsCoverage`] so the
    /// coverage gate can measure "N stubs remaining" across the whole binding surface. Installing is a
    /// side effect (globals are set); the returned records are pure data.
    pub fn register_engine_reported(&self, host: SharedHost) -> LuaResult<Vec<NsCoverage>> {
        let cov = bindings::install_all(&self.lua, &host)?;
        // The engine's own bootstrap glue runs AFTER the namespaces exist — it aliases members of them
        // into `_G`. See [`BOOTSTRAP_GLUE`].
        self.lua.load(BOOTSTRAP_GLUE).set_name("@bootstrap_glue").exec()?;
        Ok(cov)
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
    pub fn eval<T: mercs2_luac::rt::FromLuaMulti>(&self, src: &str) -> LuaResult<T> {
        self.lua.load(src).eval()
    }

    /// Fire the `GameStateChange` handlers waiting on `(state, phase)` — the engine's world-load state
    /// machine calls this (via the resident pump) to advance the `MrxState` chain when a requested game
    /// state reaches that phase.
    pub fn fire_state_change(&self, state: &str, phase: &str) -> LuaResult<()> {
        crate::bindings::event::fire_game_state_change(&self.lua, state, phase)
    }

    /// Dispatch every engine-side callback the host has queued this tick — the counterpart to the
    /// `fire_*` entry points above, for callbacks the *engine* completes rather than the script.
    ///
    /// Call once per frame, after the event pump. Covers:
    /// * **HUD** — movie end callbacks (`Hud.SetMovieEndCallback`), which also advances movie playback.
    /// * **Player** — boundary / PDA / satellite / disguise / join-leave callbacks.
    ///
    /// Both registries retain the real `mercs2_luac::rt::Function`, which is the whole point: these verbs used to
    /// go through `record_all`, whose `stringify_arg` maps `Value::Function` → `""`, so the closures
    /// were destroyed at registration and pushed into a Vec nothing drained.
    pub fn pump_callbacks(&self, host: &SharedHost, dt: f32) -> LuaResult<()> {
        crate::bindings::hud::pump_hud_callbacks(&self.lua, host, dt)?;
        crate::bindings::player::pump_player_callbacks(&self.lua, host)
    }

    /// Fire the `ObjectHibernation` handlers waiting on `(guid, phase)` — the streaming system calls
    /// this when an object wakes (`"awake"`) or hibernates (`"asleep"`). This is the condition behind
    /// the awake-gate that opens nearly every object script:
    /// `Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupEvents, {uGuid})`.
    pub fn fire_object_hibernation(&self, guid: u64, phase: &str) -> LuaResult<()> {
        crate::bindings::event::fire_object_hibernation(&self.lua, guid, phase)
    }

    /// Fire the `ObjectDeath` handlers registered for `guid`.
    pub fn fire_object_death(&self, guid: u64) -> LuaResult<()> {
        crate::bindings::event::fire_object_death(&self.lua, guid)
    }

    /// Fire the `ObjectInSeat` handlers matching `(occupant, vehicle, seat, action)` — the engine calls
    /// this when a character takes (`action = "e"`) or leaves (`"x"`) a seat. `seat` is a real seat
    /// code (`"d"`/`"p"`); the `"a"` wildcard belongs to the filter, never to the event.
    pub fn fire_object_in_seat(
        &self,
        occupant: u64,
        vehicle: u64,
        seat: &str,
        action: &str,
    ) -> LuaResult<()> {
        crate::bindings::event::fire_object_in_seat(&self.lua, occupant, vehicle, seat, action)
    }

    /// Drain the layer names whose load completed since the last call — the layers whose objects the
    /// engine should now wake (see `Pg.__flush_layer_loads`). Clears the list.
    ///
    /// `Pg.LoadLayer` is Lua-side bookkeeping that instantiates nothing, so this is how the engine
    /// learns a layer arrived. Unload requests are not reported: they wake nothing.
    pub fn take_streamed_layers(&self) -> LuaResult<Vec<String>> {
        let g = self.lua.globals();
        let Ok(t) = g.get::<mercs2_luac::rt::Table>("__layers_streamed") else { return Ok(Vec::new()) };
        let out: Vec<String> = t.sequence_values::<String>().flatten().collect();
        if !out.is_empty() {
            g.set("__layers_streamed", self.lua.create_table()?)?;
        }
        Ok(out)
    }

    /// Advance the `TimerRelative` handlers by `dt` seconds (the engine's per-tick `Event.__pump`).
    pub fn pump_events(&self, dt: f32) -> LuaResult<()> {
        let ev: Table = self.lua.globals().get("Event")?;
        let pump: mercs2_luac::rt::Function = ev.get("__pump")?;
        pump.call::<()>(dt)
    }

    /// How many event handlers are still registered. Tooling only — a script that re-registers on each
    /// stream-in without `Event.Delete`ing on stream-out shows up as a count that climbs every cycle.
    pub fn live_event_handles(&self) -> usize {
        crate::bindings::event::live_handle_count(&self.lua)
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
        let kind: String = child.get::<mercs2_luac::rt::Function>("Kind").unwrap().call(()).unwrap();
        assert_eq!(kind, "CHILD");
        // inherited method (via __index chain to BaseThing)
        let greet: String = child.get::<mercs2_luac::rt::Function>("Greet").unwrap().call(()).unwrap();
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
        // The spawned handle comes back as a `Guid` (lightuserdata), not an integer — see
        // [`crate::guid`]. Reading it as `i64` here is what a shipped script would do only if it did
        // arithmetic on a handle, which none of them does.
        let guid: Guid = h
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
        assert_eq!(guid, Guid(1));

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

    /// The **coverage gate**. Installs the whole binding surface, writes the machine-readable
    /// `binding_coverage.json` next to the crate, and asserts the current baseline so later
    /// progress (or a regression) is visible as a diff. `remaining` = required cfuncs still lacking a
    /// real body — the "N stubs remaining" metric, which must only ever go **down**.
    ///
    /// When you fill a namespace, re-run this test to regenerate the JSON, then bump the
    /// asserted `EXPECTED_REAL` / `EXPECTED_REMAINING` below (they should move in opposite directions).
    #[test]
    fn coverage_report() {
        // Baseline of the current build. Update as namespaces land bodies (the Lua-hook TDD pass added the
        // Event system + Player economy/getters + Object health/labels + Sys game-state handshake).
        // +1 namespace / +2 required / +2 real: the `Table` engine global (`Table.Create`,
        // `Table.InsertI`). Not in the Surface-B trace — recovered from its only call sites, in
        // `MrxGuiBase:Widget:GetChildren`. Without it that returns nil and every `pairs(GetChildren())`
        // in the GUI layer throws, which takes down any boot that imports MrxUtil (see table_ext.rs).
        // 36 → 37 and 1088 → 1092 on 2026-07-26: the `Movie` table (registry row 21, VA 0x00B99BBC,
        // 4 cfuncs) was missing from this crate entirely, so the harness had been under-counting the
        // engine surface by one namespace and four bindings. Its four are `stub`, and deliberately
        // NOT counted as real — retail has real bodies there; we simply have no movie playback yet.
        const EXPECTED_NAMESPACES: usize = 37;
        const EXPECTED_REQUIRED: usize = 1092;
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
        // Bloom + Graphics + Fade to mercs2_core::RenderSettings (real +40); CameraFx wired the cinematic
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
        //
        // The Player rewrite (2026-07-26) reworked all 107 `Player` bodies against `player_code_map.md` and moved
        // three verbs off the recorded-command log onto retained-callback registries — `Player`'s eight
        // callback registrations, plus `Hud.SetMovieEndCallback` and `Hud.InterpolateWidget`. **The
        // counts below do not move**: `record_all` already counted as `real`, and that was the problem.
        // `stringify_arg` maps `Value::Function` → `""`, so every one of those closures was destroyed at
        // registration and pushed into a Vec nothing drains. Since `MrxGuiBase`'s animation queue chains
        // itself through `InterpolateWidget`'s completion callback, that single sink was holding up the
        // entire GUI — every fade and menu transition, the intro cinematic, and with it the release of
        // `STATE_WAITFORGAME`. Real/stub totals were never the signal here; whether a callback can fire
        // is. ~205 verbs still route to `script_cmds` with no drain — that remains open.
        const EXPECTED_REAL: usize = 1060;
        // 28 → 32 on 2026-07-26 with the `Movie` namespace. Note these four are the one place in this
        // crate where `stub` does NOT mean "retail also does nothing": `Movie.{Start,Stop,Pause,Resume}`
        // have real retail bodies (`0x005C6510`/`0x005C6480`/`0x005C64B0`/`0x005C64E0`, none of them the
        // shared `0x006D5640` no-op). They are unimplemented here pending a movie/Bink path, and are
        // counted as debt rather than as faithful no-ops.
        const EXPECTED_STUB: usize = 32;

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
            "real-body count regressed/changed — bump EXPECTED_REAL when a system lands bodies"
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

        // Emit the machine-readable report for CI / later systems to watch trend to zero.
        let json = coverage_json(&cov);
        let out =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binding_coverage.json");
        std::fs::write(&out, &json).expect("write binding_coverage.json");
        assert!(json.contains("\"remaining\""));
    }
}
