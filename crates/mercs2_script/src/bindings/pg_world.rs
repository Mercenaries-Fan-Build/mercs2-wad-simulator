//! `Junk` engine binding namespace — luaL_Reg table VA 0x00b99e28, 24 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! **This is not a second `Pg` table.** The namespace registry at `0x00DFD478` (31 rows × 12 bytes)
//! names this table **`Junk`** at row 19; `Pg` is row 2 and is a different table (`0x00B99328`,
//! modelled by `pg.rs`). The two share no cfunc names — the overlap is empty. The file keeps its
//! `pg_world` name for churn reasons only; the global it installs is `Junk`, which is what the
//! shipped scripts call (`Junk.FormatTime` / `IsInstallable` / `DrawPath`, never `Pg.*` for these).
//! `CreateRegion` in particular belongs here and **not** to `Pg` — several docs had it under `Pg`.
//!
//! World/spawn/asset-tooling surface. Only one entry here is pure: `FormatTime(seconds)` — a
//! seconds→`"M:SS"` formatter —
//! which gets a real body. Every other cfunc needs world, asset-DB, or install-manager state that has
//! no `EngineHost` seam yet, so they are faithful `b.stub`s: the dev/diagnostic dumps (`DumpAssets`,
//! `DumpTextures`, `DumpMemory`, `DumpStats`, …) mirror retail's return-0 dev stubs, and the
//! spawn/region/alarm/install cfuncs no-op with sensible defaults (spawns → guid 0, `IsInstallable`
//! → false, `Search` → empty table) so callers don't fault on a nil. A later world/asset silo backs
//! these once the host exposes the streaming + asset DB.

use mlua::{Lua, MultiValue, Result as LuaResult, Value};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
/// Registry row 19 (`0x00DFD55C`) names this table **`Junk`** — not `PgWorld`, and not part of `Pg`.
/// The game calls it `Junk.*` (12 sites: `IsInstallable`, `InstallToHDD`, `UseExistingInstall`,
/// `FormatTime`, `ToggleAlarm`, `DrawPath`). Merging it into `Pg` also zeroed its `corpus_calls`
/// census, which keyed on `Pg.<name>`. Corrected 2026-07-26.
pub const NAMESPACE: &str = "Junk";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Junk";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99e28;

pub const REQUIRED: &[Required] = &[
    Required { name: "SpawnHomingProjectile", corpus_calls: 0 },
    Required { name: "CreateRegion", corpus_calls: 0 },
    Required { name: "Subdue", corpus_calls: 0 },
    Required { name: "GetModelBBoxExtents", corpus_calls: 1 },
    Required { name: "SpawnWithModel", corpus_calls: 0 },
    Required { name: "FormatTime", corpus_calls: 2 },
    Required { name: "DrawPath", corpus_calls: 1 },
    Required { name: "IsInstallable", corpus_calls: 4 },
    Required { name: "InstallToHDD", corpus_calls: 2 },
    Required { name: "UseExistingInstall", corpus_calls: 2 },
    Required { name: "Search", corpus_calls: 0 },
    Required { name: "DumpAssets", corpus_calls: 0 },
    Required { name: "DumpAssetsDiff", corpus_calls: 0 },
    Required { name: "DumpTextures", corpus_calls: 0 },
    Required { name: "DumpAssetMemory", corpus_calls: 0 },
    Required { name: "DumpMemory", corpus_calls: 0 },
    Required { name: "LoadScript", corpus_calls: 0 },
    Required { name: "LoadFunctions", corpus_calls: 0 },
    Required { name: "LoadData", corpus_calls: 0 },
    Required { name: "DescribeGuid", corpus_calls: 0 },
    Required { name: "SetQGrey", corpus_calls: 0 },
    Required { name: "ActivateAlarm", corpus_calls: 0 },
    Required { name: "ToggleAlarm", corpus_calls: 1 },
    Required { name: "DumpStats", corpus_calls: 0 },
];

/// `FormatTime` is pure and gets a real body; the world/asset/install cfuncs are faithful stubs.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // FormatTime(seconds [, useTenths]) -> "M:SS" (or "M:SS.t" with tenths). Pure clock formatting.
    b.real(
        "FormatTime",
        lua.create_function(|_, (seconds, use_tenths): (f64, Option<bool>)| {
            let secs = seconds.max(0.0);
            let mins = (secs / 60.0).floor() as i64;
            let rem = secs - (mins * 60) as f64;
            Ok(if use_tenths.unwrap_or(false) {
                let whole = rem.floor() as i64;
                let tenths = ((rem - whole as f64) * 10.0).floor() as i64;
                format!("{mins}:{whole:02}.{tenths}")
            } else {
                format!("{mins}:{:02}", rem.floor() as i64)
            })
        })?,
    )?;

    // Value-returning stubs: sensible defaults so callers never fault on a nil where they read a
    // result. Not engine-backed yet, so counted as stubs.
    b.stub(
        "IsInstallable",
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;
    b.stub(
        "GetModelBBoxExtents",
        lua.create_function(|_, _: MultiValue| Ok((0.0f32, 0.0f32, 0.0f32)))?,
    )?;
    b.stub(
        "DescribeGuid",
        lua.create_function(|_, _: MultiValue| Ok(String::new()))?,
    )?;
    b.stub(
        "Search",
        lua.create_function(|lua, _: MultiValue| lua.create_table())?,
    )?;
    // Spawns return a guid; 0 = "no object" until the world seam exists.
    b.stub(
        "SpawnHomingProjectile",
        lua.create_function(|_, _: MultiValue| Ok(0i64))?,
    )?;
    b.stub(
        "SpawnWithModel",
        lua.create_function(|_, _: MultiValue| Ok(0i64))?,
    )?;
    // CreateRegion(name, x, y, z, radius) → the real trigger-region registry; returns the handle.
    let h = host.clone();
    b.real("CreateRegion", lua.create_function(move |_, (name, x, y, z, radius): (String, f32, f32, f32, Option<f32>)| {
        Ok(h.borrow_mut().pg_create_region(&name, [x, y, z], radius.unwrap_or(0.0)) as i64)
    })?)?;

    // Alarms → the real alarm state.
    let h = host.clone();
    b.real("ActivateAlarm", lua.create_function(move |_, (guid, on): (i64, Option<bool>)| {
        h.borrow_mut().pg_alarm_set(guid as u64, on.unwrap_or(true));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("ToggleAlarm", lua.create_function(move |_, guid: i64| Ok(h.borrow_mut().pg_alarm_toggle(guid as u64)))?)?;

    // Install-manager + script/data loaders + misc actions → recorded Pg commands the world/install
    // runtime drains.
    super::record_all(&mut b, lua, host, "Pg", &[
        "Subdue", "DrawPath", "InstallToHDD", "UseExistingInstall", "LoadScript", "LoadFunctions",
        "LoadData", "SetQGrey",
    ])?;
    // Genuine retail dev-dump stubs (stripped/no-op on the PC build).
    for name in ["DumpAssets", "DumpAssetsDiff", "DumpTextures", "DumpAssetMemory", "DumpMemory", "DumpStats"] {
        b.stub(name, lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
    }

    // `Pg` is a SHARED global: `pg.rs` installs the core surface (Spawn / GetGuidByName / …) earlier,
    // and `install_global` below replaces the global table. Copy the existing `Pg` entries into ours
    // first so both coexist (no name overlap) — otherwise this clobbers Pg.GetGuidByName/Spawn.
    if let Ok(existing) = lua.globals().get::<mlua::Table>(GLOBAL) {
        for pair in existing.pairs::<String, mlua::Function>() {
            let (k, f) = pair?;
            b.extra(&k, f)?;
        }
    }

    b.install_global(GLOBAL)
}
