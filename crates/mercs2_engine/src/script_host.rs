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
use std::rc::Rc;

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
}

impl GameScriptHost {
    pub fn new(level: impl Into<String>) -> Self {
        GameScriptHost {
            spawns: Vec::new(),
            by_name: HashMap::new(),
            by_guid: HashMap::new(),
            next_guid: 0x1000_0000, // distinct, non-zero GUID space for script-spawned actors
            level: level.into(),
        }
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
}

/// Boot the PMC interior THROUGH the script host and return the actor-spawn intents the engine must
/// realize. Runs the authentic `MrxUtil.SpawnActor` body (mrxutil.lua:463) for the inanimate
/// `HqInterior` against the real `Pg.Spawn` / `Object.*` bindings.
///
/// This is boot glue standing in for the real `mrxhq`/`WifPmcInterior` module tree (a 6-module import
/// cascade) until that runs end to end — but the control flow is already authentic: the interior
/// spawns because Lua's `Pg.Spawn` asked for it, not a hardcoded engine call.
pub fn run_interior_boot() -> Vec<SpawnRequest> {
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
    // The exact inanimate-HqInterior branch of MrxUtil.SpawnActor, as engine-embedded boot glue.
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
