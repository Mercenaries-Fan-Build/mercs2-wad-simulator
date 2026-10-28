//! Boot a new game headless, on the real world data.
//!
//! This is the same sequence `mercs2_game` runs at world setup (`mercs2_game/src/world.rs`, the block
//! that ends in `run_boot_flow`), minus everything that draws:
//!
//! 1. Build the streaming world from `vz.wad` over the script host's entity-key registry, with no
//!    layer loaded. Every placed entity, name and landing pad the scripts address arrives through the
//!    layer requests the scripts make (`Sys.RequiredAsset` for `vz_base`, `Pg.LoadLayer` for the rest).
//! 2. Start the resident script host over the vendored Lua corpus, install the call recorder
//!    ([`crate::trace`]) while no game module is loaded yet, and run the vanilla boot flow, which
//!    imports the `vz` master script and services the world-load state machine until it settles. Between
//!    pumps the world performs the layer requests and, once the hero exists, steps headless around it,
//!    so the hibernation grid activates what the scripts wait on (the opening boat).

use std::cell::RefCell;
use std::fmt;
use std::path::Path;
use std::rc::Rc;

use mercs2_engine::game_world::{self, StreamingWorld};
use mercs2_engine::script::ScriptHost;
use mercs2_engine::script_host::{self, GameScriptHost};
use mercs2_engine::{asset, render, worldutil};

use crate::trace::Tracer;

/// The three playable heroes. The identity label decides branches in the shipped Lua — e.g.
/// `wifmissionflow.lua:31` picks every cinematic by the hero's first letter (`01_AOA_M/J/C`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Hero {
    Mattias,
    Jennifer,
    Chris,
}

impl Hero {
    pub const ALL: [Hero; 3] = [Hero::Mattias, Hero::Jennifer, Hero::Chris];

    /// The hero template / identity label the engine host accepts (`hero_identity_label`,
    /// `mercs2_engine::script_host`).
    pub fn as_str(self) -> &'static str {
        match self {
            Hero::Mattias => "mattias",
            Hero::Jennifer => "jennifer",
            Hero::Chris => "chris",
        }
    }

    pub fn parse(s: &str) -> Result<Hero, String> {
        match s.to_ascii_lowercase().as_str() {
            "mattias" => Ok(Hero::Mattias),
            "jennifer" | "jen" => Ok(Hero::Jennifer),
            "chris" => Ok(Hero::Chris),
            other => Err(format!("unknown hero '{other}' (expected mattias, jennifer or chris)")),
        }
    }
}

/// A booted game: the Lua VM, the engine host it calls into, the streaming world the host's entity
/// keys live in, and the recorder that has seen every call since before the first game module loaded.
pub struct Session {
    pub sh: ScriptHost,
    pub host: Rc<RefCell<GameScriptHost>>,
    /// Attached to `host` for `Pg.Spawn`.
    pub stream: Rc<RefCell<StreamingWorld>>,
    pub hero: Hero,
    pub trace: Tracer,
}

/// Why a boot did not produce a playable session.
#[derive(Debug)]
pub enum BootError {
    /// `vz.wad` could not be opened or its streaming world could not be built.
    Wad(String),
    /// The resident script host could not start (no Lua corpus, or the binding surface failed).
    ScriptHost,
    /// The call recorder could not be installed or its output could not be written.
    Trace(String),
    /// The boot flow ran but the world-load state machine never reached `GlobalExit`.
    DidNotSettle { layers_loaded: bool, world_load_complete: bool, lua_log_lines: usize },
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootError::Wad(e) => write!(f, "vz.wad: {e}"),
            BootError::ScriptHost => write!(f, "the resident script host did not start (see the [script] lines above)"),
            BootError::Trace(e) => write!(f, "call trace: {e}"),
            BootError::DidNotSettle { layers_loaded, world_load_complete, lua_log_lines } => write!(
                f,
                "the boot flow settled without finishing the world load: every layer request fulfilled = \
                 {layers_loaded}, GlobalExit reached = {world_load_complete}, after {lua_log_lines} [lua] lines"
            ),
        }
    }
}

impl std::error::Error for BootError {}

/// Boot a new game as `hero` from the archive at `vz_wad`, recording every call to `trace_out`.
/// Returns only once the world load has reached `GlobalExit`; anything short of that is a
/// [`BootError`].
///
/// A Lua error anywhere in the boot is fatal inside the engine's `run_boot_flow` (it panics, by
/// design: a script that errors mid-transition strands the state machine), and so is a world step
/// that fails.
pub fn boot_new_game(vz_wad: &Path, hero: Hero, trace_out: &Path) -> Result<Session, BootError> {
    let path = vz_wad
        .to_str()
        .ok_or_else(|| BootError::Wad(format!("path is not UTF-8: {}", vz_wad.display())))?;
    let mut assets = asset::AssetSource::discover(path, &[]).map_err(BootError::Wad)?;
    let templates = worldutil::TemplateRecords::load(&mut assets).map_err(BootError::Wad)?;
    let data = game_world::load_streaming_world_data(
        path,
        mercs2_core::streaming::StreamingConfig::default(),
        templates,
        &render::LoadProgress::new(8),
    )
    .map_err(BootError::Wad)?;

    let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
    let stream = Rc::new(RefCell::new(data.into_streaming_world(host.borrow().guids()).1));
    host.borrow_mut().attach_stream(stream.clone());

    let sh = script_host::resident_script_host(host.clone()).ok_or(BootError::ScriptHost)?;
    let trace = Tracer::install(&sh, trace_out).map_err(BootError::Trace)?;
    host.borrow_mut().set_boot_context(hero.as_str());
    script_host::run_boot_flow(&sh, &host, hero.as_str(), &mut || {
        let viewer = host.borrow().hero_position();
        let mut stream = stream.borrow_mut();
        match viewer {
            Some(v) => stream.step_headless(v, &|_| false),
            None => stream.apply_layer_ops(&|_| false),
        }
        .unwrap_or_else(|e| panic!("[stream] {e}"));
        stream.settled()
    });
    trace.finish().map_err(BootError::Trace)?;

    let (layers_loaded, world_load_complete, lua_log_lines) = {
        let h = host.borrow();
        (h.world_layers_loaded, h.world_load_complete, h.lua_log_lines)
    };
    if !(layers_loaded && world_load_complete) {
        return Err(BootError::DidNotSettle { layers_loaded, world_load_complete, lua_log_lines });
    }
    Ok(Session { sh, host, stream, hero, trace })
}
