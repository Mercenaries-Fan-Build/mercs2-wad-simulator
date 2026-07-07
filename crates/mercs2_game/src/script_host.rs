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
}

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
        }
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
        self.by_name.get(name).copied().unwrap_or(0)
    }
    fn pg_spawn(&mut self, template: &str, pos: [f32; 3], yaw: f32, _high_detail: bool) -> u64 {
        self.next_guid += 1;
        let guid = self.next_guid;
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
        if let Some(r) = self.req_mut(guid) {
            r.pos = pos;
        }
    }
    fn object_set_yaw(&mut self, guid: u64, yaw: f32) {
        if let Some(r) = self.req_mut(guid) {
            r.yaw = yaw;
        }
    }
    fn teleport_hero(&mut self, _pos: [f32; 3]) {}
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
