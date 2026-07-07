//! `Math` engine binding namespace — luaL_Reg table VA 0x00b99be8, 17 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! This namespace is **pure math** — every cfunc is computed from its arguments with no engine state,
//! so it is backed with real bodies. Semantics are pinned from the decompiled game Lua call sites:
//! - `Normalize(x,y,z)` → unit 3-vector (3 returns); zero-length → `(0,0,0)`
//!   (`mrxgunship.lua:26`, `mrxcruisemissile.lua:36`).
//! - `Length(x,y,z)` → `sqrt(x²+y²+z²)` (`mrxfuelairbomb.lua:55`, `mrxoilcon002delivery.lua:132`).
//! - `CrossProduct(ax,ay,az,bx,by,bz)` → cross product (3 returns).
//! - `randi(n)` → int in `[1,n]`; `randi(a,b)` → int in `[a,b]` (used as 1-based table indices,
//!   `allcon002.lua:922`, and coin tosses `gurcon002.lua:679`).
//! - `randf(a,b)` → float in `[a,b)`; `randf(n)` → `[0,n)` (`fueltank.lua:7`, `islandfortress.lua:72`).
//! - `PolarToRect(angleDeg, radius)` → `(radius·cos, radius·sin)`, angle in **degrees**
//!   (`mrxguisatellite.lua:559`, `Math.PolarToRect(-nTheta + 90, nRadius)`).
//! - `GetXZHeading(dx,dy,dz)` → yaw about +Y in the XZ plane = `atan2(dx, dz)` (radians), fed straight
//!   to `Object.SetYaw` (`mrxchicon001rescue.lua:55-56`).
//!
//! `randi`/`randf` draw from a deterministic seeded LCG owned by this table (the reimpl forbids
//! nondeterministic RNG); every boot replays the same sequence.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, Result as LuaResult, Variadic};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Math";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Math";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99be8;

pub const REQUIRED: &[Required] = &[
    Required { name: "abs", corpus_calls: 4 },
    Required { name: "floor", corpus_calls: 14 },
    Required { name: "ceil", corpus_calls: 3 },
    Required { name: "round", corpus_calls: 0 },
    Required { name: "max", corpus_calls: 9 },
    Required { name: "min", corpus_calls: 8 },
    Required { name: "exp", corpus_calls: 0 },
    Required { name: "pow", corpus_calls: 0 },
    Required { name: "deg", corpus_calls: 0 },
    Required { name: "rad", corpus_calls: 0 },
    Required { name: "randi", corpus_calls: 28 },
    Required { name: "randf", corpus_calls: 10 },
    Required { name: "GetXZHeading", corpus_calls: 10 },
    Required { name: "Normalize", corpus_calls: 32 },
    Required { name: "CrossProduct", corpus_calls: 0 },
    Required { name: "Length", corpus_calls: 5 },
    Required { name: "PolarToRect", corpus_calls: 1 },
];

/// One step of a deterministic 64-bit LCG (Knuth MMIX constants). Returns the new state; callers take
/// the high bits for uniformity.
fn lcg_next(state: &RefCell<u64>) -> u64 {
    let mut s = state.borrow_mut();
    *s = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *s
}

/// Uniform f64 in `[0,1)` from the LCG (top 53 bits → mantissa).
fn lcg_f64(state: &RefCell<u64>) -> f64 {
    (lcg_next(state) >> 11) as f64 / (1u64 << 53) as f64
}

/// All 17 cfuncs are pure — every one gets a real body. `randi`/`randf` share a deterministic LCG
/// owned by this closure set.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Seeded so the RNG sequence is identical every boot (reimpl determinism mandate).
    let rng: Rc<RefCell<u64>> = Rc::new(RefCell::new(0x2545F4914F6CDD1D));

    // --- scalar helpers (game-side Math, distinct from Lua stdlib `math`) ---
    b.real("abs", lua.create_function(|_, x: f64| Ok(x.abs()))?)?;
    b.real("floor", lua.create_function(|_, x: f64| Ok(x.floor()))?)?;
    b.real("ceil", lua.create_function(|_, x: f64| Ok(x.ceil()))?)?;
    // round-half-up (floor(x+0.5)), the classic game convention.
    b.real("round", lua.create_function(|_, x: f64| Ok((x + 0.5).floor()))?)?;
    b.real(
        "max",
        lua.create_function(|_, v: Variadic<f64>| {
            Ok(v.into_iter().fold(f64::NEG_INFINITY, f64::max))
        })?,
    )?;
    b.real(
        "min",
        lua.create_function(|_, v: Variadic<f64>| {
            Ok(v.into_iter().fold(f64::INFINITY, f64::min))
        })?,
    )?;
    b.real("exp", lua.create_function(|_, x: f64| Ok(x.exp()))?)?;
    b.real("pow", lua.create_function(|_, (x, y): (f64, f64)| Ok(x.powf(y)))?)?;
    b.real("deg", lua.create_function(|_, x: f64| Ok(x.to_degrees()))?)?;
    b.real("rad", lua.create_function(|_, x: f64| Ok(x.to_radians()))?)?;

    // --- deterministic RNG ---
    let r = rng.clone();
    b.real(
        "randi",
        // randi(n) -> [1,n]; randi(a,b) -> [a,b] (inclusive; used as 1-based table indices).
        lua.create_function(move |_, args: Variadic<i64>| {
            let (lo, hi) = match args.len() {
                0 => (1, 1),
                1 => (1, args[0]),
                _ => (args[0], args[1]),
            };
            if hi <= lo {
                return Ok(lo);
            }
            let span = (hi - lo + 1) as u64;
            Ok(lo + (lcg_next(&r) >> 11).wrapping_rem(span) as i64)
        })?,
    )?;
    let r = rng.clone();
    b.real(
        "randf",
        // randf(a,b) -> [a,b); randf(n) -> [0,n); randf() -> [0,1).
        lua.create_function(move |_, args: Variadic<f64>| {
            let (lo, hi) = match args.len() {
                0 => (0.0, 1.0),
                1 => (0.0, args[0]),
                _ => (args[0], args[1]),
            };
            Ok(lo + lcg_f64(&r) * (hi - lo))
        })?,
    )?;

    // --- vector math ---
    b.real(
        "Length",
        lua.create_function(|_, (x, y, z): (f64, f64, f64)| Ok((x * x + y * y + z * z).sqrt()))?,
    )?;
    b.real(
        "Normalize",
        lua.create_function(|_, (x, y, z): (f64, f64, f64)| {
            let len = (x * x + y * y + z * z).sqrt();
            if len > 0.0 {
                Ok((x / len, y / len, z / len))
            } else {
                Ok((0.0, 0.0, 0.0))
            }
        })?,
    )?;
    b.real(
        "CrossProduct",
        lua.create_function(
            |_, (ax, ay, az, bx, by, bz): (f64, f64, f64, f64, f64, f64)| {
                Ok((ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx))
            },
        )?,
    )?;
    // Yaw about +Y from an XZ-plane delta; result flows straight into Object.SetYaw (radians).
    b.real(
        "GetXZHeading",
        lua.create_function(|_, (dx, _dy, dz): (f64, f64, f64)| Ok(dx.atan2(dz)))?,
    )?;
    // Polar (angle in DEGREES, radius) -> rectangular (x, y).
    b.real(
        "PolarToRect",
        lua.create_function(|_, (angle_deg, radius): (f64, f64)| {
            let a = angle_deg.to_radians();
            Ok((radius * a.cos(), radius * a.sin()))
        })?,
    )?;

    b.install_global(GLOBAL)
}
