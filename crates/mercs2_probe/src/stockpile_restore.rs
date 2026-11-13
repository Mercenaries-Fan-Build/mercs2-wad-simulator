//! `stockpile-restore` — does a WAD's `wifpmcinterior` script restore the HQ stockpile mark a save
//! holds (BUG-015, `docs/fixpack/bug_register.md`)?
//!
//! ```text
//! mercs2_probe stockpile-restore <profile> --expect restored|dropped [--wad <wad>]
//! ```
//!
//! The `wifpmcinterior` script asset (`m2("wifpmcinterior")`, type `0x42498680`) is read from the
//! WAD and its LuaQ run in the game's exact Lua 5.1.5 VM (`mercs2_luac`), with `import` the only
//! global stubbed. Then, as the save path does:
//!
//! 1. `LoadSingleton({ tStockpileQty = <the save's tPmcInteriorData.tStockpileQty> })` — what
//!    `WifPmcInterior.LoadSingleton` receives on load (`xQ!L.lua:799`), with no `bUnlocked` and no
//!    `tIntroduced`, so nothing but the stockpile branch runs.
//! 2. `_UpdateStockpile(false)`, the HQ's consumer, with the player holding nothing
//!    (`MrxPmc.GetCashQty` / `GetSupportQty` return 0, `Net.IsServer` false). Each category is shown
//!    at the larger of the live quantity and the mark; `_SetStockpileCategoryQty` is replaced by a
//!    recorder, so no pile mesh is touched.
//!
//! The outcome is `restored` when every category is shown at its saved mark, `dropped` when every
//! category is shown at the live 0. The save must hold a mark for every category of the module's
//! `_tStockpile` and at least one non-zero mark, or the two outcomes cannot be told apart and the run
//! fails. Saved and shown values are compared as the VM holds them (`f32` numbers), the way the game
//! itself reads the save text.
//!
//! Exit 0: the outcome equals `--expect`. 1: it does not. 2: could not run (usage, a missing asset, a
//! save without a usable mark, a module that does not have the retail shape).

use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::save_lua;
use mercs2_formats::scripts_block::parse_container;
use mercs2_formats::types::TYPE_HASH_SCRIPT;
use mercs2_luac::rt::{Function, Lua, Table, Value};

/// The script asset this probe runs.
pub const MODULE: &str = "wifpmcinterior";

const USAGE: &str =
    "stockpile-restore: usage: mercs2_probe stockpile-restore <profile> --expect restored|dropped [--wad <wad>]";

/// What the HQ shows after a load.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// Every category at its saved mark.
    Restored,
    /// Every category at the live quantity (0).
    Dropped,
}

impl Outcome {
    fn parse(s: &str) -> Option<Outcome> {
        match s {
            "restored" => Some(Outcome::Restored),
            "dropped" => Some(Outcome::Dropped),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Outcome::Restored => "restored",
            Outcome::Dropped => "dropped",
        }
    }
}

/// A run's result: the outcome and what each category was saved at and shown at, by name.
#[derive(Debug)]
pub struct Report {
    pub outcome: Outcome,
    /// `(category, saved mark, shown)`, sorted by category, both numbers as the VM holds them.
    pub rows: Vec<(String, f32, f32)>,
    /// How many `import` calls the module made at load (stubbed, recorded only).
    pub imports: u32,
}

/// The `tPmcInteriorData.tStockpileQty` table of a `.profile`, by category. Errors when the save has
/// none, or when an entry is not a string-keyed number.
pub fn saved_mark(profile: &[u8]) -> Result<Vec<(String, f64)>, String> {
    let p = mercs2_formats::save::parse(profile)?;
    let lua = p.decompress_lua()?;
    let text = std::str::from_utf8(&lua).map_err(|e| format!("save payload is not UTF-8: {e}"))?;
    let root = save_lua::parse_chunk(text).map_err(|e| format!("save payload: {e}"))?;
    let root = root.as_table().map_err(|e| format!("save payload: {e}"))?;
    let interior = root
        .get("tPmcInteriorData")
        .ok_or("the save has no tPmcInteriorData")?
        .as_table()
        .map_err(|e| format!("tPmcInteriorData: {e}"))?;
    let qty = interior
        .get("tStockpileQty")
        .ok_or("the save's tPmcInteriorData has no tStockpileQty")?
        .as_table()
        .map_err(|e| format!("tPmcInteriorData.tStockpileQty: {e}"))?;
    let mut out = Vec::new();
    for e in qty.iter_entries() {
        let key = match &e.key {
            Some(save_lua::Value::String(s)) => s.value.clone(),
            other => return Err(format!("tStockpileQty has a non-string key {other:?}")),
        };
        let save_lua::Value::Number(n) = &e.value else {
            return Err(format!("tStockpileQty.{key} is a {}, not a number", e.value.kind()));
        };
        out.push((key, n.value));
    }
    Ok(out)
}

/// The LuaQ of the `wifpmcinterior` script asset in the WAD at `wadpath`.
pub fn module_luaq(wadpath: &str) -> Result<Vec<u8>, String> {
    let mut w = wad::open(wadpath)?;
    let c = wad::extract_container_typed(&mut w, pandemic_hash_m2(MODULE), TYPE_HASH_SCRIPT)
        .map_err(|e| format!("{MODULE} (script 0x{TYPE_HASH_SCRIPT:08X}) in {wadpath}: {e}"))?;
    let l = parse_container(&c).map_err(|e| format!("{MODULE} container: {e}"))?;
    let end = l.luaq_off + l.luaq_len;
    if end > c.len() {
        return Err(format!("{MODULE} container: LuaQ +0x{:X}..+0x{end:X} overruns {} bytes", l.luaq_off, c.len()));
    }
    Ok(c[l.luaq_off..end].to_vec())
}

const DRIVER: &str = r#"
local tSaved = ...
local nImports = 0
function import() nImports = nImports + 1 end
__stockpile_module()
assert(type(LoadSingleton) == "function", "the module defines no LoadSingleton")
assert(type(_UpdateStockpile) == "function", "the module defines no _UpdateStockpile")
assert(type(_tStockpile) == "table", "the module defines no _tStockpile table")
assert(type(_tStockpileQty) == "table" and next(_tStockpileQty) == nil,
  "_tStockpileQty is not an empty table at module load")
for sName in pairs(tSaved) do
  assert(_tStockpile[sName] ~= nil, "the save holds a mark for " .. sName .. ", which _tStockpile has no category for")
end
for sName in pairs(_tStockpile) do
  assert(tSaved[sName] ~= nil, "the save holds no mark for the _tStockpile category " .. sName)
end
local tArg = {}
for sName, n in pairs(tSaved) do tArg[sName] = n end
LoadSingleton({ tStockpileQty = tArg })
MrxPmc = { GetCashQty = function() return 0 end, GetSupportQty = function() return 0 end }
Net = { IsServer = function() return false end }
local tShown = {}
_SetStockpileCategoryQty = function(sName, nQty)
  assert(tShown[sName] == nil, "category " .. sName .. " shown twice")
  tShown[sName] = nQty
end
_UpdateStockpile(false)
return tShown, nImports
"#;

/// Run `luaq` (a `wifpmcinterior` chunk) against `saved` and classify what the HQ shows.
pub fn run_module(luaq: &[u8], saved: &[(String, f64)]) -> Result<Report, String> {
    let lua = Lua::new().map_err(|e| format!("Lua::new: {e:?}"))?;
    let module: Function = lua
        .load(luaq)
        .set_name(format!("={MODULE}"))
        .into_function()
        .map_err(|e| format!("load {MODULE}: {e:?}"))?;
    lua.globals().set("__stockpile_module", module).map_err(|e| format!("{e:?}"))?;
    let t = lua.create_table().map_err(|e| format!("{e:?}"))?;
    for (k, v) in saved {
        if t.raw_get::<Value>(k.as_str()).map_err(|e| format!("{e:?}"))?.is_truthy() {
            return Err(format!("the save holds two marks for {k}"));
        }
        t.raw_set(k.as_str(), *v as f32).map_err(|e| format!("{e:?}"))?;
    }
    let (shown, imports): (Table, f32) =
        lua.load(DRIVER).set_name("=stockpile-restore").call(t.clone()).map_err(|e| format!("driver: {e:?}"))?;
    let mut rows = Vec::new();
    for pair in t.pairs::<String, Value>() {
        let (k, v) = pair.map_err(|e| format!("{e:?}"))?;
        let saved = v.as_f32().ok_or_else(|| format!("saved mark {k} is a {}", v.type_name()))?;
        let s: Value = shown.raw_get(k.as_str()).map_err(|e| format!("{e:?}"))?;
        let s = s.as_f32().ok_or_else(|| format!("category {k} shown as a {}, not a number", s.type_name()))?;
        rows.push((k, saved, s));
    }
    if shown.pairs::<Value, Value>().count() != rows.len() {
        return Err(format!("{} categories shown for {} saved marks", shown.pairs::<Value, Value>().count(), rows.len()));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Report { outcome: classify(&rows)?, rows, imports: imports as u32 })
}

/// `Restored` when every category is shown at its mark, `Dropped` when every category is shown at 0.
fn classify(rows: &[(String, f32, f32)]) -> Result<Outcome, String> {
    if rows.iter().all(|r| r.1 == 0.0) {
        return Err("every saved mark is 0: restored and dropped look the same; use a save with a non-zero mark".into());
    }
    if rows.iter().all(|r| r.2 == r.1) {
        Ok(Outcome::Restored)
    } else if rows.iter().all(|r| r.2 == 0.0) {
        Ok(Outcome::Dropped)
    } else {
        let odd: Vec<String> =
            rows.iter().filter(|r| r.2 != r.1 && r.2 != 0.0).map(|r| format!("{} saved {} shown {}", r.0, r.1, r.2)).collect();
        let mixed = if odd.is_empty() { "some categories at the mark, others at 0".to_string() } else { odd.join(", ") };
        Err(format!("the HQ shows neither the saved mark nor the live quantities: {mixed}"))
    }
}

/// The subcommand: exit 0 when the outcome equals `--expect`, 1 when it does not, 2 when it could not run.
pub fn main(wadpath: &str, args: &[String]) -> i32 {
    let expect = match args.iter().position(|a| a == "--expect").and_then(|i| args.get(i + 1)) {
        Some(e) => match Outcome::parse(e) {
            Some(o) => o,
            None => {
                eprintln!("stockpile-restore: --expect {e:?} is not restored or dropped\n{USAGE}");
                return 2;
            }
        },
        None => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    let mut positional = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if a == "--expect" || a == "--wad" {
            skip = true;
        } else {
            positional.push(a);
        }
    }
    let [profile] = positional.as_slice() else {
        eprintln!("{USAGE}");
        return 2;
    };
    let run = || -> Result<Report, String> {
        let bytes = std::fs::read(profile).map_err(|e| format!("read {profile}: {e}"))?;
        let saved = saved_mark(&bytes).map_err(|e| format!("{profile}: {e}"))?;
        run_module(&module_luaq(wadpath)?, &saved)
    };
    match run() {
        Ok(r) => {
            for (k, saved, shown) in &r.rows {
                // Through f64: f32's shortest Display rounds 342479104 to 342479100.
                println!("  {k:<16} saved {:>12}  shown {:>12}", f64::from(*saved), f64::from(*shown));
            }
            println!(
                "[stockpile-restore] {wadpath}: {MODULE} ran ({} imports stubbed); the HQ mark is {} (expected {})",
                r.imports,
                r.outcome.as_str(),
                expect.as_str()
            );
            i32::from(r.outcome != expect)
        }
        Err(e) => {
            eprintln!("stockpile-restore: {e}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module with retail's shape: `_tStockpile`, an empty `_tStockpileQty`, `LoadSingleton`
    /// guarded by `guard`, and `_UpdateStockpile` showing each category at max(live, mark).
    fn module(guard: &str) -> Vec<u8> {
        let src = format!(
            r#"
import("MrxPmc")
_tStockpileQty = {{}}
_tStockpile = {{ money = {{1000}}, artillery = {{1, 5, 9}}, moab = {{1}} }}
function LoadSingleton(tSaveData)
  if tSaveData.{guard} then _tStockpileQty = tSaveData.tStockpileQty end
end
function _UpdateStockpile(bClientJoined)
  for sName in pairs(_tStockpile) do
    local nQty = sName == "money" and MrxPmc.GetCashQty() or MrxPmc.GetSupportQty(sName)
    local nOld = _tStockpileQty[sName] or -1
    if nQty > nOld then _tStockpileQty[sName] = nQty else nQty = nOld end
    if bClientJoined == false then _SetStockpileCategoryQty(sName, nQty) end
  end
end
"#
        );
        mercs2_luac::compile(&src, MODULE).unwrap()
    }

    fn saved(rows: &[(&str, f64)]) -> Vec<(String, f64)> {
        rows.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    const MARK: &[(&str, f64)] = &[("money", 342479104.0), ("artillery", 7.0), ("moab", 0.0)];

    #[test]
    fn the_retail_guard_drops_the_mark() {
        let r = run_module(&module("tSupportQty"), &saved(MARK)).unwrap();
        assert_eq!(r.outcome, Outcome::Dropped);
        assert_eq!(r.imports, 1);
        assert!(r.rows.iter().all(|row| row.2 == 0.0), "{:?}", r.rows);
    }

    #[test]
    fn the_fixed_guard_restores_the_mark() {
        let r = run_module(&module("tStockpileQty"), &saved(MARK)).unwrap();
        assert_eq!(r.outcome, Outcome::Restored);
        let money = r.rows.iter().find(|row| row.0 == "money").unwrap();
        assert_eq!((money.1, money.2), (342479104.0, 342479104.0));
    }

    #[test]
    fn an_all_zero_mark_cannot_tell_the_outcomes_apart() {
        let e = run_module(&module("tStockpileQty"), &saved(&[("money", 0.0), ("artillery", 0.0), ("moab", 0.0)]))
            .unwrap_err();
        assert!(e.contains("every saved mark is 0"), "{e}");
    }

    #[test]
    fn a_mark_missing_a_category_fails() {
        let e = run_module(&module("tStockpileQty"), &saved(&[("money", 5.0), ("artillery", 1.0)])).unwrap_err();
        assert!(e.contains("no mark for the _tStockpile category moab"), "{e}");
    }

    #[test]
    fn a_mark_for_an_unknown_category_fails() {
        let mut s = saved(MARK);
        s.push(("tankbuster".into(), 3.0));
        let e = run_module(&module("tStockpileQty"), &s).unwrap_err();
        assert!(e.contains("tankbuster, which _tStockpile has no category for"), "{e}");
    }

    #[test]
    fn a_partly_restored_mark_is_neither_outcome() {
        let rows = vec![("a".to_string(), 4.0, 4.0), ("b".to_string(), 3.0, 0.0)];
        let e = classify(&rows).unwrap_err();
        assert!(e.contains("some categories at the mark, others at 0"), "{e}");
    }

    /// The `Mattias Nilsson_6A0E523C` fixture's mark reads as the 13 string-keyed numbers
    /// BUG-015.md quotes.
    #[test]
    fn the_fixture_mark_reads_from_the_save() {
        let path = mercs2_formats::game_paths::save_fixtures().join("Mattias Nilsson_6A0E523C.profile");
        let m = saved_mark(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(m.len(), 13);
        let get = |k: &str| m.iter().find(|r| r.0 == k).unwrap().1;
        assert_eq!(get("tankbuster"), 94.0);
        assert_eq!(get("money"), 342479104.0);
    }
}

/// Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
/// `.mercs2-local.toml`, and fails when it is absent. `cargo xtask retail-test`.
#[cfg(all(test, feature = "retail"))]
mod retail {
    use super::*;

    /// Retail `wifpmcinterior` reproduces BUG-015: the fixture's mark is dropped on load.
    #[test]
    fn retail_wifpmcinterior_drops_the_saved_mark() {
        let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let wad = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        let path = mercs2_formats::game_paths::save_fixtures().join("Mattias Nilsson_6A0E523C.profile");
        let saved = saved_mark(&std::fs::read(&path).unwrap()).unwrap();
        let r = run_module(&module_luaq(&wad.to_string_lossy()).unwrap(), &saved).unwrap();
        assert_eq!(r.outcome, Outcome::Dropped, "{:?}", r.rows);
        assert_eq!(r.rows.len(), 13);
    }
}
