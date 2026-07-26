//! `Movie` engine binding namespace — luaL_Reg table VA 0x00b99bbc, 4 cfuncs.
//!
//! Registry row 21 (`0x00DFD574`) names this table **`Movie`**; it is a first-class namespace, not a
//! sub-table. It was missing from this crate entirely until 2026-07-26, which meant `binding_smoke`
//! could not see it and the coverage harness under-counted the engine surface by four.
//!
//! Retail drives Bink playback (`PgMoviePlayerXenon.cpp` on the console side); the four cfuncs have
//! real bodies at `0x005C6510` / `0x005C6480` / `0x005C64B0` / `0x005C64E0` — none is the shared
//! no-op stub `0x006D5640`.
//!
//! **These are installed as stubs, and that is NOT a faithful no-op.** The distinction matters for
//! the burn-down: a `b.stub` elsewhere in this crate means "retail also does nothing here". Here it
//! means "retail does real work and we have no movie playback yet". When a Bink/movie path lands,
//! these become `b.real` against it. Recorded plainly rather than inflating the backed count.
//!
//! No shipped script calls `Movie.*` — 0 call sites across `docs/mercs2-luacd` (388 files) and
//! `docs/mercs2-dlc-luacd` (77). Playback is driven from the engine and from Scaleform, not Lua, so
//! the zero is expected rather than evidence the table is dead.

use mlua::{Lua, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Movie";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Movie";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99bbc;

pub const REQUIRED: &[Required] = &[
    Required { name: "Start", corpus_calls: 0 },
    Required { name: "Stop", corpus_calls: 0 },
    Required { name: "Pause", corpus_calls: 0 },
    Required { name: "Resume", corpus_calls: 0 },
];

pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let _ = host;
    let mut b = NsBuilder::new(lua)?;

    // Unimplemented, not faithful no-ops — see the module header.
    b.stub("Start", lua.create_function(|_, ()| Ok(()))?)?;
    b.stub("Stop", lua.create_function(|_, ()| Ok(()))?)?;
    b.stub("Pause", lua.create_function(|_, ()| Ok(()))?)?;
    b.stub("Resume", lua.create_function(|_, ()| Ok(()))?)?;

    b.install_global(GLOBAL)
}
