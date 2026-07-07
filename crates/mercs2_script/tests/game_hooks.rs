//! Game-hook harness — TDD across the Lua↔engine seam.
//!
//! Each case runs a **real Mercenaries Lua hook pattern** (taken verbatim/near-verbatim from the
//! decompiled corpus `docs/mercs2-luacd`, cited per hook) against the actual `mercs2_script` binding
//! layer + a `HarnessHost` implementing [`mercs2_script::EngineHost`] with real backing state. A hook
//! passes when its engine effect / return matches what the game expects.
//!
//! This is deliberately a **work queue**: many hooks pass (the bodies are filled), some are RED (the
//! binding isn't wired yet — Vehicle/Sound, the condition-kind events). The catalog test tallies
//! pass/fail, writes `hook_coverage.json`, and asserts only a *minimum* pass count so progress is
//! measured and regressions caught while the RED list stays visible as the next work.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use mercs2_script::{EngineHost, ScriptHost};

// ---------------------------------------------------------------------------
//   HarnessHost — a real-enough engine behind the bindings
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Obj {
    name: String,
    alive: bool,
    health: f32,
    max_health: f32,
    labels: HashSet<String>,
    pos: [f32; 3],
    yaw: f32,
}

#[derive(Default)]
struct HarnessHost {
    cash: i64,
    fuel: i64,
    fuel_cap: i64,
    player_char: u64,
    objs: HashMap<u64, Obj>,
    next_guid: u64,
    named: HashMap<String, u64>,
    game_states: Vec<String>,
    autosaves: u32,
    logs: Vec<String>,
    // vehicle seats (stand-in for mercs2_vehicle)
    driver: HashMap<u64, u64>,    // vehicle -> driver rider
    rider_veh: HashMap<u64, u64>, // rider -> vehicle
    // audio (stand-in for mercs2_audio::AudioEngine)
    cues: Vec<String>,
    next_voice: u64,
    music_states: HashSet<String>,
    current_music: String,
    // movement
    velocities: HashMap<u64, f32>,
}

impl HarnessHost {
    /// A host with one player character (guid 1, "Mattias", 100 HP) already in the world — enough for
    /// the Object/Player hooks to have something to act on, like a booted game.
    fn booted() -> Rc<RefCell<Self>> {
        let mut h = HarnessHost { fuel_cap: 100, ..Default::default() };
        let g = h.spawn_obj("Mattias");
        h.player_char = g;
        h.velocities.insert(g, 3.5);
        Rc::new(RefCell::new(h))
    }

    fn spawn_obj(&mut self, name: &str) -> u64 {
        self.next_guid += 1;
        let g = self.next_guid;
        self.objs.insert(
            g,
            Obj {
                name: name.to_string(),
                alive: true,
                health: 100.0,
                max_health: 100.0,
                labels: HashSet::new(),
                pos: [0.0; 3],
                yaw: 0.0,
            },
        );
        if !name.is_empty() {
            self.named.insert(name.to_string(), g);
        }
        g
    }
}

impl EngineHost for HarnessHost {
    fn log(&mut self, _source: &str, msg: &str) {
        self.logs.push(msg.to_string());
    }
    fn get_level_name(&self) -> String {
        "vz".into()
    }
    fn start_with_resources(&self) -> bool {
        false
    }
    fn guid_by_name(&mut self, name: &str) -> u64 {
        self.named.get(name).copied().unwrap_or(0)
    }
    fn pg_spawn(&mut self, template: &str, pos: [f32; 3], yaw: f32, _hi: bool) -> u64 {
        let g = self.spawn_obj(template);
        if let Some(o) = self.objs.get_mut(&g) {
            o.pos = pos;
            o.yaw = yaw;
        }
        g
    }
    fn object_set_name(&mut self, guid: u64, name: &str) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.name = name.to_string();
        }
        self.named.insert(name.to_string(), guid);
    }
    fn object_set_position(&mut self, guid: u64, pos: [f32; 3]) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.pos = pos;
        }
    }
    fn object_set_yaw(&mut self, guid: u64, yaw: f32) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.yaw = yaw;
        }
    }
    fn object_get_position(&mut self, guid: u64) -> [f32; 3] {
        self.objs.get(&guid).map(|o| o.pos).unwrap_or([0.0; 3])
    }
    fn object_get_yaw(&mut self, guid: u64) -> f32 {
        self.objs.get(&guid).map(|o| o.yaw).unwrap_or(0.0)
    }
    fn teleport_hero(&mut self, _pos: [f32; 3]) {}
    fn add_layers(&mut self, _layers: &[String]) {}

    // economy
    fn player_cash(&self) -> i64 {
        self.cash
    }
    fn player_set_cash(&mut self, cash: i64) {
        self.cash = cash;
    }
    fn player_fuel(&self) -> i64 {
        self.fuel
    }
    fn player_set_fuel(&mut self, fuel: i64) {
        self.fuel = fuel;
    }
    fn player_fuel_capacity(&self) -> i64 {
        self.fuel_cap
    }
    fn player_set_fuel_capacity(&mut self, cap: i64) {
        self.fuel_cap = cap;
    }

    // character getters
    fn player_local_player(&self) -> u64 {
        self.player_char
    }
    fn player_any_character(&self) -> u64 {
        self.player_char
    }
    fn player_local_character(&self) -> u64 {
        self.player_char
    }
    fn player_primary_character(&self) -> u64 {
        self.player_char
    }
    fn player_secondary_character(&self) -> u64 {
        0
    }
    fn player_is_local(&self, guid: u64) -> bool {
        guid == self.player_char
    }

    // object health/life/labels
    fn object_health(&self, guid: u64) -> f32 {
        self.objs.get(&guid).map(|o| o.health).unwrap_or(0.0)
    }
    fn object_set_health(&mut self, guid: u64, hp: f32) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.health = hp;
            o.alive = hp > 0.0;
        }
    }
    fn object_max_health(&self, guid: u64) -> f32 {
        self.objs.get(&guid).map(|o| o.max_health).unwrap_or(0.0)
    }
    fn object_is_alive(&self, guid: u64) -> bool {
        self.objs.get(&guid).map(|o| o.alive).unwrap_or(false)
    }
    fn object_kill(&mut self, guid: u64) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.alive = false;
            o.health = 0.0;
        }
    }
    fn object_revive(&mut self, guid: u64) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.alive = true;
            o.health = o.max_health;
        }
    }
    fn object_remove(&mut self, guid: u64) {
        self.objs.remove(&guid);
    }
    fn object_name(&self, guid: u64) -> String {
        self.objs.get(&guid).map(|o| o.name.clone()).unwrap_or_default()
    }
    fn object_add_label(&mut self, guid: u64, label: &str) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.labels.insert(label.to_string());
        }
    }
    fn object_remove_label(&mut self, guid: u64, label: &str) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.labels.remove(label);
        }
    }
    fn object_has_label(&self, guid: u64, label: &str) -> bool {
        self.objs.get(&guid).map(|o| o.labels.contains(label)).unwrap_or(false)
    }
    fn object_set_invincible(&mut self, _guid: u64, _on: bool) {}

    // sys
    fn sys_request_game_state(&mut self, state: &str) {
        self.game_states.push(state.to_string());
    }
    fn sys_request_autosave(&mut self) {
        self.autosaves += 1;
    }

    // vehicle (seat model)
    fn vehicle_driver(&self, veh: u64) -> u64 {
        self.driver.get(&veh).copied().unwrap_or(0)
    }
    fn vehicle_from_rider(&self, rider: u64) -> u64 {
        self.rider_veh.get(&rider).copied().unwrap_or(0)
    }
    fn vehicle_enter(&mut self, veh: u64, rider: u64, seat: &str) -> bool {
        if seat == "d" {
            self.driver.insert(veh, rider);
        }
        self.rider_veh.insert(rider, veh);
        true
    }
    fn vehicle_exit(&mut self, rider: u64) -> bool {
        if let Some(veh) = self.rider_veh.remove(&rider) {
            if self.driver.get(&veh) == Some(&rider) {
                self.driver.remove(&veh);
            }
            true
        } else {
            false
        }
    }
    fn vehicle_usable(&self, veh: u64) -> bool {
        self.objs.contains_key(&veh)
    }

    // audio
    fn sound_cue(&mut self, cue: &str) -> u64 {
        self.next_voice += 1;
        self.cues.push(cue.to_string());
        self.next_voice
    }
    fn sound_add_music_state(&mut self, name: &str) {
        self.music_states.insert(name.to_string());
    }
    fn sound_transition_music(&mut self, state: &str) -> bool {
        self.current_music = state.to_string();
        true
    }
    fn sound_lib_version(&self) -> String {
        "PgAudio-harness".into()
    }

    // movement
    fn object_velocity(&self, guid: u64) -> f32 {
        self.velocities.get(&guid).copied().unwrap_or(0.0)
    }
}

/// Build a booted host + a registered script host. Returns both so a hook can inspect engine state.
fn setup() -> (ScriptHost, Rc<RefCell<HarnessHost>>) {
    let host = HarnessHost::booted();
    let sh = ScriptHost::bare().expect("script host");
    sh.register_engine(host.clone()).expect("register");
    (sh, host)
}

// ---------------------------------------------------------------------------
//   Hook cases — real corpus patterns. Each panics (assert!) on failure.
// ---------------------------------------------------------------------------

// --- Player economy (xQ!L.lua: Player.SetCash/GetCash/SetFuel; mrxmissionflow) ---
fn h_player_cash_roundtrip() {
    let (sh, _h) = setup();
    let c: i64 = sh.eval("Player.SetCash(50000); return Player.GetCash()").unwrap();
    assert_eq!(c, 50000);
}
fn h_player_fuel_add() {
    let (sh, _h) = setup();
    let f: i64 = sh.eval("Player.SetFuel(100); Player.AddFuel(50); return Player.GetFuel()").unwrap();
    assert_eq!(f, 150);
}
fn h_player_any_character_nonnil() {
    let (sh, _h) = setup();
    let ok: bool = sh.eval("return Player.GetAnyCharacter() ~= nil").unwrap();
    assert!(ok, "GetAnyCharacter must return the booted player character, not nil");
}
fn h_player_secondary_is_nil_singleplayer() {
    let (sh, _h) = setup();
    let is_nil: bool = sh.eval("return Player.GetSecondaryCharacter() == nil").unwrap();
    assert!(is_nil);
}

// --- Object life/labels/health (collectable.lua: Object.IsAlive/Kill/HasLabel; enemyblippable) ---
fn h_object_isalive_player() {
    let (sh, _h) = setup();
    let alive: bool = sh.eval("return Object.IsAlive(Player.GetAnyCharacter())").unwrap();
    assert!(alive);
}
fn h_object_kill_then_dead() {
    let (sh, _h) = setup();
    // collectable.lua OnContextAction: Object.Kill(oSelf.uGuid)
    let alive: bool = sh
        .eval("local u = Player.GetAnyCharacter(); Object.Kill(u); return Object.IsAlive(u)")
        .unwrap();
    assert!(!alive, "killed object must not be alive");
}
fn h_object_haslabel_add_remove() {
    let (sh, _h) = setup();
    // collectable.lua: Object.HasLabel(uGuid, "CollectableInvalidated")
    let (a, b): (bool, bool) = sh
        .eval(
            r#"local u = Player.GetAnyCharacter()
               Object.AddLabel(u, "CollectableInvalidated")
               local a = Object.HasLabel(u, "CollectableInvalidated")
               Object.RemoveLabel(u, "CollectableInvalidated")
               local b = Object.HasLabel(u, "CollectableInvalidated")
               return a, b"#,
        )
        .unwrap();
    assert!(a && !b, "label add/remove roundtrip failed: {a} {b}");
}
fn h_object_gethealth() {
    let (sh, _h) = setup();
    let hp: f32 = sh.eval("return Object.GetHealth(Player.GetAnyCharacter())").unwrap();
    assert_eq!(hp, 100.0);
}
fn h_object_setinvincible_reason_arg() {
    let (sh, _h) = setup();
    // mrxguihudmessage.lua: Object.SetInvincible(Player.GetLocalCharacter(), false, "Fanfare")
    sh.exec(
        r#"Object.SetInvincible(Player.GetLocalCharacter(), false, "Fanfare")"#,
        "@setinvincible",
    )
    .unwrap();
}

// --- The MrxUtil.SpawnActor recipe (mrxutil.lua:463-490) ---
fn h_spawnactor_recipe() {
    let (sh, host) = setup();
    let guid: i64 = sh
        .eval(
            r#"local uGuid = Pg.GetGuidByName("HqInterior")
               if not uGuid then uGuid = Pg.Spawn("PmcHqInterior", 0,0,0, 0, false, true) end
               Object.SetName(uGuid, "HqInterior")
               Object.SetPosition(uGuid, 3750, 450, -3840)
               return uGuid"#,
        )
        .unwrap();
    assert!(guid > 0);
    let hb = host.borrow();
    let g = *hb.named.get("HqInterior").expect("named");
    assert_eq!(hb.objs[&g].pos, [3750.0, 450.0, -3840.0]);
}

// --- Event: ScriptEvent (moonpatrol.lua mpPlayerLeft; oilcon020 "PDA Open") ---
fn h_event_scriptevent_fires() {
    let (sh, _h) = setup();
    let fired: bool = sh
        .eval(
            r#"_fired = false
               Event.Create(Event.ScriptEvent, {"mpPlayerLeft"}, function() _fired = true end)
               Event.Post("mpPlayerLeft")
               return _fired"#,
        )
        .unwrap();
    assert!(fired, "ScriptEvent handler did not fire on Post");
}
fn h_event_scriptevent_filter_gates() {
    let (sh, _h) = setup();
    // real filter form: Event.Create(Event.ScriptEvent, {name, function(tData) ... end}, cb, args)
    let (wrong, right): (bool, bool) = sh
        .eval(
            r#"_hits = 0
               Event.CreatePersistent(Event.ScriptEvent,
                 {"pjoin", function(tData) return tData == 7 end},
                 function() _hits = _hits + 1 end)
               Event.Post("pjoin", 3)   -- filtered out
               local wrong = (_hits == 0)
               Event.Post("pjoin", 7)   -- passes
               local right = (_hits == 1)
               return wrong, right"#,
        )
        .unwrap();
    assert!(wrong && right, "filter gating wrong: filtered={wrong} passed={right}");
}
fn h_event_timerrelative_fires_on_pump() {
    let (sh, _h) = setup();
    // oilcon002.lua Cleanup: Event.Create(Event.TimerRelative, {0.75}, cb, {arg})
    let (before, after): (bool, bool) = sh
        .eval(
            r#"_t = false
               Event.Create(Event.TimerRelative, {0.5}, function(x) _t = (x == 42) end, {42})
               Event.__pump(0.3); local before = _t     -- not yet
               Event.__pump(0.3); local after = _t       -- 0.6 >= 0.5 -> fires with arg 42
               return before, after"#,
        )
        .unwrap();
    assert!(!before && after, "timer fired wrong: before={before} after={after}");
}
fn h_event_delete_prevents_fire() {
    let (sh, _h) = setup();
    // moonpatrol.lua OnDeactivate: Event.Delete(e) then the event must not fire
    let fired: bool = sh
        .eval(
            r#"_d = false
               local e = Event.Create(Event.ScriptEvent, {"boom"}, function() _d = true end)
               e = Event.Delete(e)
               Event.Post("boom")
               return _d"#,
        )
        .unwrap();
    assert!(!fired, "deleted event still fired");
}
fn h_event_persistent_fires_twice() {
    let (sh, _h) = setup();
    let hits: i64 = sh
        .eval(
            r#"_n = 0
               Event.CreatePersistent(Event.ScriptEvent, {"tick"}, function() _n = _n + 1 end)
               Event.Post("tick"); Event.Post("tick")
               return _n"#,
        )
        .unwrap();
    assert_eq!(hits, 2, "persistent handler should fire on every Post");
}
fn h_event_create_oneshot_fires_once() {
    let (sh, _h) = setup();
    let hits: i64 = sh
        .eval(
            r#"_m = 0
               Event.Create(Event.ScriptEvent, {"once"}, function() _m = _m + 1 end)
               Event.Post("once"); Event.Post("once")
               return _m"#,
        )
        .unwrap();
    assert_eq!(hits, 1, "one-shot Create handler should fire exactly once");
}

// --- Sys world-load handshake (mrxmissionflow Autosave; boot RequestGameState) ---
fn h_sys_requestgamestate_recorded() {
    let (sh, host) = setup();
    sh.exec(r#"Sys.RequestGameState("WaitForStreaming")"#, "@gs").unwrap();
    assert_eq!(host.borrow().game_states, vec!["WaitForStreaming".to_string()]);
}
fn h_sys_requestautosave() {
    let (sh, host) = setup();
    // mrxmissionflow Autosave(): Sys.RequestAutosave(inMission, lastMission, time, pct)
    sh.exec(r#"Sys.RequestAutosave(true, "none", 12.5, 42)"#, "@as").unwrap();
    assert_eq!(host.borrow().autosaves, 1);
}

// --- Vehicle seats (enemyblippable.lua: Vehicle.GetDriver; moonpatrol Enter/Exit) ---
fn h_vehicle_enter_getdriver_exit() {
    let (sh, _h) = setup();
    let (in_seat, empty): (bool, bool) = sh
        .eval(
            r#"local car = Pg.Spawn("car", 0,0,0, 0, false, true)
               local u = Player.GetAnyCharacter()
               Vehicle.Enter(car, u, "d")
               local a = (Vehicle.GetDriver(car) == u)
               Vehicle.Exit(u)
               local b = (Vehicle.GetDriver(car) == nil)
               return a, b"#,
        )
        .unwrap();
    assert!(in_seat && empty, "Enter/GetDriver/Exit failed: seated={in_seat} emptied={empty}");
}

// --- Audio (mrxguihudmessage / mission scripts: Sound.CueSound; music FSM) ---
fn h_sound_cuesound() {
    let (sh, host) = setup();
    let ok: bool = sh.eval(r#"return Sound.CueSound("ui_confirm") ~= nil"#).unwrap();
    assert!(ok, "CueSound must return a voice id, not nil");
    assert_eq!(host.borrow().cues, vec!["ui_confirm".to_string()]);
}
fn h_sound_music_transition() {
    let (sh, _h) = setup();
    // mrxmusic pattern: Sound.AddMusicState("combat"); Sound.TransitionMusic("combat")
    let ok: bool = sh
        .eval(r#"Sound.AddMusicState("combat"); return Sound.TransitionMusic("combat")"#)
        .unwrap();
    assert!(ok);
}

// --- Object velocity + ObjectDeath condition feed ---
fn h_object_getvelocity() {
    let (sh, _h) = setup();
    let v: f32 = sh.eval("return Object.GetVelocity(Player.GetAnyCharacter())").unwrap();
    assert!((v - 3.5).abs() < 1e-4, "GetVelocity returned {v}");
}
fn h_object_death_fires_event() {
    let (sh, _h) = setup();
    // moonpatrol.lua OnDeath pattern: an ObjectDeath handler fires when the object is killed.
    let dead: bool = sh
        .eval(
            r#"_dead = false
               local u = Player.GetAnyCharacter()
               Event.Create(Event.ObjectDeath, {u}, function() _dead = true end)
               Object.Kill(u)
               return _dead"#,
        )
        .unwrap();
    assert!(dead, "ObjectDeath handler did not fire on Object.Kill");
}

// --- RED (expected fail — the genuine work queue: AI has no body; proximity needs engine feeding) ---
fn h_red_ai_setrelation() {
    let (sh, _h) = setup();
    // oilcon002.lua: Ai.SetRelation(GetGuidByName("VZ"), uHostage, -100) — AI not built yet
    let _: bool = sh.eval("Ai.SetRelation(1, 2, -100); return true").unwrap();
}
fn h_red_ai_goal() {
    let (sh, _h) = setup();
    let _: bool = sh.eval(r#"Ai.Goal(Player.GetAnyCharacter(), "Attack"); return true"#).unwrap();
}
fn h_red_event_proximity_fires() {
    let (sh, _h) = setup();
    // oilcon020.lua: Event.Create(Event.ObjectProximity, {char, guid, "<", 7, ...}, cb) — condition
    // kinds aren't fed by the engine yet, so this callback never fires. RED until wired.
    let fired: bool = sh
        .eval(
            r#"_p = false
               Event.Create(Event.ObjectProximity,
                 {Player.GetAnyCharacter(), Player.GetAnyCharacter(), "<", 7, false, false},
                 function() _p = true end)
               Event.__pump(1.0)
               return _p"#,
        )
        .unwrap();
    assert!(fired, "ObjectProximity condition not fed yet (expected RED)");
}

type HookFn = fn();
const HOOKS: &[(&str, HookFn)] = &[
    ("Player.SetCash/GetCash roundtrip", h_player_cash_roundtrip),
    ("Player.SetFuel/AddFuel/GetFuel", h_player_fuel_add),
    ("Player.GetAnyCharacter non-nil", h_player_any_character_nonnil),
    ("Player.GetSecondaryCharacter nil (SP)", h_player_secondary_is_nil_singleplayer),
    ("Object.IsAlive(player)", h_object_isalive_player),
    ("Object.Kill -> not IsAlive", h_object_kill_then_dead),
    ("Object.AddLabel/HasLabel/RemoveLabel", h_object_haslabel_add_remove),
    ("Object.GetHealth", h_object_gethealth),
    ("Object.SetInvincible(guid,bool,reason)", h_object_setinvincible_reason_arg),
    ("MrxUtil.SpawnActor recipe", h_spawnactor_recipe),
    ("Event.ScriptEvent fires on Post", h_event_scriptevent_fires),
    ("Event.ScriptEvent filter gates", h_event_scriptevent_filter_gates),
    ("Event.TimerRelative fires on pump", h_event_timerrelative_fires_on_pump),
    ("Event.Delete prevents fire", h_event_delete_prevents_fire),
    ("Event.CreatePersistent fires twice", h_event_persistent_fires_twice),
    ("Event.Create one-shot fires once", h_event_create_oneshot_fires_once),
    ("Sys.RequestGameState recorded", h_sys_requestgamestate_recorded),
    ("Sys.RequestAutosave", h_sys_requestautosave),
    ("Vehicle.Enter/GetDriver/Exit", h_vehicle_enter_getdriver_exit),
    ("Sound.CueSound -> voice", h_sound_cuesound),
    ("Sound.AddMusicState/TransitionMusic", h_sound_music_transition),
    ("Object.GetVelocity", h_object_getvelocity),
    ("Event.ObjectDeath fires on Kill", h_object_death_fires_event),
    // --- expected RED (the genuine queue: AI unbuilt; proximity needs engine condition feeding) ---
    ("RED Ai.SetRelation (no AI yet)", h_red_ai_setrelation),
    ("RED Ai.Goal (no AI yet)", h_red_ai_goal),
    ("RED Event.ObjectProximity fires", h_red_event_proximity_fires),
];

/// Minimum green hooks — the TDD floor. Bump this UP as more bindings land; it catches regressions
/// without demanding the RED queue pass. (23 green designed above; floor set just under.)
const EXPECTED_PASS_MIN: usize = 23;

#[test]
fn game_hook_catalog() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {})); // silence per-hook panic spam; we tally them
    let mut pass = 0usize;
    let mut red: Vec<&str> = Vec::new();
    for (name, f) in HOOKS {
        if catch_unwind(AssertUnwindSafe(*f)).is_ok() {
            pass += 1;
        } else {
            red.push(name);
        }
    }
    std::panic::set_hook(prev);

    // Machine-readable behavioral coverage next to the crate (sibling of binding_coverage.json).
    let red_json: Vec<String> = red.iter().map(|n| format!("\"{n}\"")).collect();
    let json = format!(
        "{{\n  \"schema\": \"mercs2_script.hook_coverage/1\",\n  \"note\": \"Behavioral TDD across the Lua<->engine seam: real corpus hook patterns run against the bindings + a HarnessHost. pass = green; red = the work queue. Regenerate: cargo test -p mercs2_script --test game_hooks.\",\n  \"total\": {},\n  \"pass\": {},\n  \"red\": {},\n  \"red_hooks\": [{}]\n}}\n",
        HOOKS.len(),
        pass,
        red.len(),
        red_json.join(", ")
    );
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("hook_coverage.json");
    std::fs::write(&out, &json).expect("write hook_coverage.json");

    eprintln!("[game-hooks] {pass}/{} pass — RED work queue: {red:#?}", HOOKS.len());
    assert!(
        pass >= EXPECTED_PASS_MIN,
        "game-hook green count regressed: {pass}/{} (floor {EXPECTED_PASS_MIN}); red={red:?}",
        HOOKS.len()
    );
}
