//! `mercs2_autoplay` — run the shipped game Lua headless, the way a player's session runs it.
//!
//! The game's progression, missions and objectives are all Lua (`docs/mercs2-luacd`). This crate runs
//! that Lua on the engine's own script host ([`mercs2_engine::script_host`]) against the real world
//! data from `vz.wad`, with no renderer. It is the base for walking every logical path the scripts
//! can take and recording every call they make.
//!
//! * [`boot`] — boot a new game through the real `vz` master script, exactly the path `mercs2_game`
//!   takes, and refuse to continue unless the world load reaches `GlobalExit`.
//! * [`tasks`] — read the live `MrxTask` tree (every mission, briefing and objective node and its
//!   state) straight out of the running Lua.
//! * [`trace`] — record every call the Lua makes (into the engine, across modules, into unbuilt engine
//!   names) as NDJSON, from before the first game module loads.
//!
//! Nothing here re-implements engine behaviour: the waits the scripts park on (movies, layer streaming,
//! state enter/exit) are already resolved by the engine host's pump; this crate only drives it.

pub mod boot;
pub mod tasks;
pub mod trace;
