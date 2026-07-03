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

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

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
    /// `Pg.GetGuidByName` — resolve a placed-object name to its runtime GUID (0 if unknown).
    fn guid_by_name(&mut self, name: &str) -> u64;
    /// The bottom-out of `MrxUtil.SpawnActor(template, name, {vPosition, nRotation, …})`: instantiate
    /// an actor template (e.g. the `HqInterior` room shell) at a world position. Returns its GUID.
    fn spawn_actor(&mut self, template: &str, name: &str, pos: [f32; 3], rot_deg: f32) -> u64;
    /// `MrxUtil._TeleportHero` — move the player to a world position.
    fn teleport_hero(&mut self, pos: [f32; 3]);
    /// The bottom-out of `MrxLayerManager.Add({..})`: request a set of `vz_state_*` world-state layers.
    fn add_layers(&mut self, layers: &[String]);
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
    /// Phase 1 installs the confirmed C-binding tables the boot + PMC-interior slice touches. The
    /// `Mrx*` modules are *game* Lua and come from the corpus via `import`, not from here.
    pub fn register_engine(&self, host: SharedHost) -> LuaResult<()> {
        let g = self.lua.globals();

        // ---- Debug.* (the [lua] log stream) ----
        let debug = self.lua.create_table()?;
        let h = host.clone();
        let printf = self.lua.create_function(move |lua, args: MultiValue| {
            let s = args
                .iter()
                .next()
                .and_then(|v| lua.coerce_string(v.clone()).ok().flatten())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            h.borrow_mut().log("lua", &s);
            Ok(())
        })?;
        debug.set("Printf", printf.clone())?;
        debug.set("Print", printf)?;
        g.set("Debug", debug)?;

        // ---- Sys.* (engine/level queries) ----
        let sys = self.lua.create_table()?;
        let h = host.clone();
        sys.set(
            "GetLevelName",
            self.lua
                .create_function(move |_, ()| Ok(h.borrow().get_level_name()))?,
        )?;
        let h = host.clone();
        sys.set(
            "GetMasterScriptName",
            self.lua
                .create_function(move |_, ()| Ok(h.borrow().get_level_name()))?,
        )?;
        let h = host.clone();
        sys.set(
            "StartWithResources",
            self.lua
                .create_function(move |_, ()| Ok(h.borrow().start_with_resources()))?,
        )?;
        g.set("Sys", sys)?;

        // ---- Pg.* (name → GUID) ----
        let pg = self.lua.create_table()?;
        let h = host.clone();
        pg.set(
            "GetGuidByName",
            self.lua.create_function(move |_, name: String| {
                Ok(h.borrow_mut().guid_by_name(&name) as i64)
            })?,
        )?;
        g.set("Pg", pg)?;

        // ---- Event.* (constants + a Create stub so boot scripts register without erroring) ----
        let event = self.lua.create_table()?;
        for (i, k) in [
            "ObjectHibernation",
            "TimerRelative",
            "TimerAbsolute",
            "ObjectDeath",
            "ObjectProximity",
            "Boundary",
            "ObjectPhysicsEvent",
        ]
        .iter()
        .enumerate()
        {
            event.set(*k, (i + 1) as i64)?;
        }
        // Create(kind, params, fn, args) -> opaque handle. Phase 1 does not run the event loop yet;
        // it returns a distinct integer so scripts can store/compare handles.
        let counter = Rc::new(RefCell::new(0i64));
        let c = counter.clone();
        event.set(
            "Create",
            self.lua.create_function(move |_, _: MultiValue| {
                let mut n = c.borrow_mut();
                *n += 1;
                Ok(*n)
            })?,
        )?;
        g.set("Event", event)?;

        // ---- _Engine.* (PROVISIONAL seam for actor/layer spawning) ----
        // These back MrxUtil.SpawnActor / _TeleportHero / MrxLayerManager.Add. They are exposed under
        // `_Engine` until each is bound to its real C-binding-table name (pending mrxutil.lua ×
        // binding_map.json). The seam itself — EngineHost — is final; only the Lua name is provisional.
        let engine = self.lua.create_table()?;
        let h = host.clone();
        engine.set(
            "SpawnActor",
            self.lua.create_function(
                move |_, (template, name, params): (String, String, Option<Table>)| {
                    let (pos, rot) = params
                        .as_ref()
                        .map(actor_pos_rot)
                        .unwrap_or(([0.0; 3], 0.0));
                    Ok(h.borrow_mut().spawn_actor(&template, &name, pos, rot) as i64)
                },
            )?,
        )?;
        let h = host.clone();
        engine.set(
            "TeleportHero",
            self.lua.create_function(move |_, params: Table| {
                let (pos, _) = actor_pos_rot(&params);
                h.borrow_mut().teleport_hero(pos);
                Ok(())
            })?,
        )?;
        let h = host.clone();
        engine.set(
            "AddLayers",
            self.lua.create_function(move |_, layers: Table| {
                let names: Vec<String> = layers.sequence_values::<String>().flatten().collect();
                h.borrow_mut().add_layers(&names);
                Ok(())
            })?,
        )?;
        g.set("_Engine", engine)?;

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

/// Pull `vPosition` (`{x,y,z}` or `{[1],[2],[3]}`) and `nRotation` (degrees) out of a SpawnActor param
/// table. Missing fields default to origin / 0.
fn actor_pos_rot(params: &Table) -> ([f32; 3], f32) {
    let pos = params
        .get::<Table>("vPosition")
        .ok()
        .map(|t| {
            [
                t.get::<f32>(1).unwrap_or(0.0),
                t.get::<f32>(2).unwrap_or(0.0),
                t.get::<f32>(3).unwrap_or(0.0),
            ]
        })
        .unwrap_or([0.0; 3]);
    let rot = params.get::<f32>("nRotation").unwrap_or(0.0);
    (pos, rot)
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
        spawns: Vec<(String, [f32; 3], f32)>,
        layers: Vec<String>,
        teleports: Vec<[f32; 3]>,
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
        fn guid_by_name(&mut self, name: &str) -> u64 {
            // deterministic fake GUID so tests can assert routing
            name.bytes().fold(0u64, |a, b| a.wrapping_mul(131).wrapping_add(b as u64))
        }
        fn spawn_actor(&mut self, template: &str, _name: &str, pos: [f32; 3], rot: f32) -> u64 {
            self.spawns.push((template.to_string(), pos, rot));
            self.spawns.len() as u64
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
    fn engine_bindings_route_to_host() {
        let host = Rc::new(RefCell::new(RecordingHost::default()));
        let h = ScriptHost::bare().unwrap();
        h.register_engine(host.clone()).unwrap();

        // Debug.Printf -> host.log ; Sys.GetLevelName -> host
        let lvl: String = h
            .eval("Debug.Printf(\"gui loaded\"); return Sys.GetLevelName()")
            .unwrap();
        assert_eq!(lvl, "vz");

        // Pg.GetGuidByName routes and returns a nonzero integer
        let guid: i64 = h.eval("return Pg.GetGuidByName(\"HqInterior\")").unwrap();
        assert_ne!(guid, 0);

        // The provisional actor seam: SpawnActor + AddLayers reach the host with parsed args.
        h.exec(
            "_Engine.SpawnActor(\"HqInterior\", \"HqInterior\", { vPosition = {3750, 450, -3840}, nRotation = 0 })\n\
             _Engine.AddLayers({\"vz_state_pmcinterior\", \"vz_state_pmcinterior_jet\"})",
            "@slice",
        )
        .unwrap();

        let hb = host.borrow();
        assert_eq!(hb.logs, vec!["gui loaded".to_string()]);
        assert_eq!(hb.spawns.len(), 1);
        assert_eq!(hb.spawns[0].0, "HqInterior");
        assert_eq!(hb.spawns[0].1, [3750.0, 450.0, -3840.0]);
        assert_eq!(hb.layers.len(), 2);
    }
}
