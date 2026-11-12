//! The wardrobe lists qm owns: one list per level per hero, and the restore map that goes with it.
//!
//! The wardrobe (`_tOutfits`, the `wifpmcinterior` script) saves the picked row `i` as the profile's
//! costume byte `i - 1` (`wifpmcinterior.lua:1472`, `Player.SetProfileCostume(iIndex - 1)`). The join
//! restores that byte through the player script's own map (`mrxplayer.lua:155-173`, `:179`):
//! `models[byte]` is the model put on, and byte 0 (`models[0]`, nil) leaves the hero's base model. So
//! a map entry `k` must be the model of wardrobe row `k + 1`. The map is a local of the player script;
//! the only way to change it is the player script's `SetCharacterMap(t)` (`mrxplayer.lua:746`), which
//! replaces it whole.
//!
//! Both levels hold the two scripts under the same asset names: the DLC ships `dlc01_pmcinterior`
//! as `wifpmcinterior` (`0x3787604B`) and `dlc01_player` as `mrxplayer` (`0x8D35CF13`), so one reader
//! serves both. Their lists are read by running the shipped bytecode in the game's Lua 5.1.5
//! ([`RetailLevel::read`]), never from decompiled source. The DLC's bytecode carries no debug
//! information, so the map is read through the public `GetTemplateAndModelName`, not its upvalue.
//!
//! [`generate`] builds, from a level's retail lists and the outfits the installed Shipments add, each
//! hero's list and map: retail rows in retail order, then the added rows in the order given (the load
//! plan's Shipment order, each Shipment's rows in manifest order). Nobody picks a position, and the
//! same set gives the same lists on every machine. The map is every list row's model after the first,
//! then the retail map's models that no retail wardrobe row offers, in retail order (the DLC's
//! `dlc01_player` lays its map out the same way by hand).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::scripts_block::parse_container;
use mercs2_formats::types::{TYPE_HASH_SCRIPT, TYPE_ID_SCRIPT};
use mercs2_luac::rt::{Function, Lua, Table, Value};

use crate::manifest::wearer_table_key;

/// The wardrobe's hero keys (`_tOutfits.<key>`), in the order output is written.
pub const HEROES: [&str; 3] = ["mattias", "chris", "jennifer"];

/// The script asset that holds `_tOutfits`, in every level.
pub const WARDROBE_SCRIPT: &str = "wifpmcinterior";

/// The script asset that holds the character map, in every level.
pub const PLAYER_SCRIPT: &str = "mrxplayer";

/// The level an added outfit joins when it names none: the main campaign (user, 2026-11-12).
pub const DEFAULT_LEVEL: &str = "vz";

/// The profile's hero, upgrade and costume are bytes (save header `0x4D`, `0x4E`, `0x4F`;
/// `save_serialize_code_map.md` §4), so every index the join can pass the map is in `0..=255`.
const PROFILE_BYTE_MAX: u32 = 255;

/// One `_tOutfits` row as the shipped wardrobe holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetailRow {
    /// `Name`: the row's key among that hero's rows.
    pub name: String,
    /// `Model`: what `Player.SetOutfit` receives.
    pub model: String,
    /// `PlayerVisibleName`: what the wardrobe shows.
    pub display: String,
}

/// One hero's entry of the player script's character map, as `GetTemplateAndModelName` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterEntry {
    /// The template when no upgrade is given, and the wardrobe key it maps to (via
    /// [`wearer_table_key`]: the map's third base is `jen`, the wardrobe's key `jennifer`).
    pub base: String,
    /// `templates[u]` for `u = 1..`: the answer for each upgrade up to the last one that is not the
    /// base. A position whose answer is the base holds the base, which answers the same.
    pub templates: Vec<String>,
    /// `models[k]` for `k = 1..`.
    pub models: Vec<String>,
}

/// A level's wardrobe and character map, read from its shipped scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetailLevel {
    /// `_tOutfits`, by hero key.
    pub wardrobe: BTreeMap<String, Vec<RetailRow>>,
    /// The character map by hero index (`iIndex - 1`), each with the wardrobe key it serves.
    pub map: Vec<(String, CharacterEntry)>,
}

/// An outfit a Shipment adds, as the generator takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedOutfit {
    /// The Shipment's name: with `slug`, the row's identity.
    pub shipment: String,
    pub slug: String,
    pub model: String,
    pub display: String,
    /// The heroes it is offered to, as wardrobe keys or their spellings (`jen`).
    pub wearers: Vec<String>,
    /// The levels whose wardrobes it joins; `None` is the main campaign alone ([`DEFAULT_LEVEL`]):
    /// a level such as the DLC's ships its own wardrobe, and an outfit reaches it only by naming it.
    pub levels: Option<Vec<String>>,
    /// The chatter set the join applies when this outfit is restored.
    pub chatter: Option<String>,
}

/// Which outfit a row is, independent of where it sits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Identity {
    /// A retail row, by its `Name` (unique among one hero's retail rows).
    Retail { name: String },
    /// An added row, by its Shipment and slug.
    Added { shipment: String, slug: String },
}

/// Who decides whether the wardrobe lists a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    /// The level's own rule (vz: the unlock count; dlc01: the count or the row's `bAlwaysVisible`).
    Retail,
    /// Always listed: every qm-added outfit is unlocked.
    Always,
}

/// One row of a generated list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub identity: Identity,
    pub model: String,
    pub display: String,
    pub listing: Listing,
    pub chatter: Option<String>,
}

/// One hero's generated list and the map entry that restores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroList {
    /// The wardrobe key.
    pub hero: String,
    /// Row `i` saves costume byte `i - 1`.
    pub rows: Vec<Row>,
    /// `models[k - 1]` is row `k + 1`'s model, then the retail restore-only models.
    pub map: CharacterEntry,
}

/// A level's generated lists, in the map's hero-index order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelLists {
    pub level: String,
    pub heroes: Vec<HeroList>,
}

/// The LuaQ chunk of a script asset's `UCFX` container.
pub fn luaq_of(container: &[u8]) -> Result<Vec<u8>, String> {
    let l = parse_container(container)?;
    let end = l.luaq_off + l.luaq_len;
    if end > container.len() {
        return Err(format!("LuaQ +0x{:X}..+0x{end:X} overruns the {}-byte container", l.luaq_off, container.len()));
    }
    Ok(container[l.luaq_off..end].to_vec())
}

fn lua_err(context: &str) -> impl Fn(mercs2_luac::rt::Error) -> String + '_ {
    move |e| format!("{context}: {e}")
}

/// Run a script chunk at load in a fresh VM, `import` stubbed: what it defines is then in the globals.
fn run_module(luaq: &[u8], name: &str) -> Result<Lua, String> {
    let lua = Lua::new().map_err(lua_err("Lua::new"))?;
    let f: Function =
        lua.load(luaq).set_name(format!("={name}")).into_function().map_err(lua_err(&format!("load {name}")))?;
    lua.load("function import() end").set_name("=import stub").exec().map_err(lua_err("stub import"))?;
    f.call::<()>(()).map_err(lua_err(&format!("run {name}")))?;
    Ok(lua)
}

fn string_field(t: &Table, key: &str, ctx: &str) -> Result<String, String> {
    match t.raw_get::<Value>(key).map_err(lua_err(ctx))? {
        Value::String(s) => Ok(s.to_string_lossy()),
        v => Err(format!("{ctx}.{key} is a {}, not a string", v.type_name())),
    }
}

/// `_tOutfits` of a wardrobe chunk.
fn read_wardrobe(luaq: &[u8]) -> Result<BTreeMap<String, Vec<RetailRow>>, String> {
    let lua = run_module(luaq, "wifpmcinterior")?;
    let outfits = match lua.globals().raw_get::<Value>("_tOutfits").map_err(lua_err("_tOutfits"))? {
        Value::Table(t) => t,
        v => return Err(format!("the wardrobe script leaves _tOutfits a {}, not a table", v.type_name())),
    };
    let mut out = BTreeMap::new();
    for pair in outfits.pairs::<Value, Value>() {
        let (k, v) = pair.map_err(lua_err("_tOutfits"))?;
        let hero = k.as_str().ok_or_else(|| format!("_tOutfits has a {} key", k.type_name()))?;
        if !HEROES.contains(&hero.as_str()) {
            return Err(format!("_tOutfits.{hero} is not a wardrobe hero ({HEROES:?})"));
        }
        let Value::Table(rows) = v else {
            return Err(format!("_tOutfits.{hero} is a {}, not a table", v.type_name()));
        };
        let n = rows.len();
        if rows.pairs::<Value, Value>().count() != n {
            return Err(format!("_tOutfits.{hero} is not a sequence: {n} rows by length, more keys by pairs"));
        }
        let mut list = Vec::with_capacity(n);
        for i in 1..=n {
            let ctx = format!("_tOutfits.{hero}[{i}]");
            let row: Table = rows.raw_get(i as i64).map_err(lua_err(&ctx))?;
            let r = RetailRow {
                name: string_field(&row, "Name", &ctx)?,
                model: string_field(&row, "Model", &ctx)?,
                display: string_field(&row, "PlayerVisibleName", &ctx)?,
            };
            if list.iter().any(|o: &RetailRow| o.name == r.name) {
                return Err(format!("{ctx}: Name {:?} is already a row of _tOutfits.{hero}", r.name));
            }
            list.push(r);
        }
        if list.is_empty() {
            return Err(format!("_tOutfits.{hero} has no rows"));
        }
        out.insert(hero, list);
    }
    for h in HEROES {
        if !out.contains_key(h) {
            return Err(format!("_tOutfits has no {h} list"));
        }
    }
    Ok(out)
}

const READ_MAP: &str = r#"
local nMax = ...
assert(type(GetTemplateAndModelName) == "function", "the player script defines no GetTemplateAndModelName")
assert(type(SetCharacterMap) == "function", "the player script defines no SetCharacterMap")
assert(GetTemplateAndModelName({iIndex = 0}) == nil, "hero index 0 has a map entry")
local tMap, bEnded = {}, false
for h = 1, nMax do
  local sBase, sNoModel = GetTemplateAndModelName({iIndex = h})
  if sBase == nil then
    bEnded = true
  else
    assert(not bEnded, "hero index " .. h .. " has a map entry after an index that has none")
    assert(type(sBase) == "string", "hero index " .. h .. "'s base is a " .. type(sBase))
    assert(sNoModel == nil, "hero index " .. h .. " names a model with no costume")
    assert(GetTemplateAndModelName({iIndex = h, iUpgrade = 0}) == sBase, "hero index " .. h .. " has an upgrade 0 template")
    local _, sModel0 = GetTemplateAndModelName({iIndex = h, iCostume = 0})
    assert(sModel0 == nil, "hero index " .. h .. "'s costume 0 names a model")
    local tTemplates, tModels, bModelsEnded = {}, {}, false
    for u = 1, nMax do
      tTemplates[u] = GetTemplateAndModelName({iIndex = h, iUpgrade = u})
    end
    for k = 1, nMax do
      local _, sModel = GetTemplateAndModelName({iIndex = h, iCostume = k})
      if sModel == nil then
        bModelsEnded = true
      else
        assert(not bModelsEnded, "hero index " .. h .. " has costume " .. k .. " after a costume with no model")
        tModels[k] = sModel
      end
    end
    tMap[h] = { base = sBase, templates = tTemplates, models = tModels }
  end
end
-- The route qm installs its map by: SetCharacterMap must replace the map GetTemplateAndModelName reads.
local tSentinel = { { base = "qm:base", templates = { "qm:template" }, models = { "qm:model" } } }
assert(SetCharacterMap(tSentinel) ~= false, "SetCharacterMap refused a map")
local sT, sM = GetTemplateAndModelName({iIndex = 1, iUpgrade = 1, iCostume = 1})
assert(sT == "qm:template" and sM == "qm:model" and GetTemplateAndModelName({iIndex = 2}) == nil,
  "SetCharacterMap does not replace the map GetTemplateAndModelName reads")
return tMap
"#;

/// The character map of a player chunk, through `GetTemplateAndModelName`, and proof that its
/// `SetCharacterMap` replaces that map.
fn read_map(luaq: &[u8]) -> Result<Vec<CharacterEntry>, String> {
    let lua = run_module(luaq, "mrxplayer")?;
    let map: Table = lua
        .load(READ_MAP)
        .set_name("=read character map")
        .call(PROFILE_BYTE_MAX as f32)
        .map_err(lua_err("the player script's character map"))?;
    let mut out = Vec::new();
    for h in 1..=map.len() {
        let ctx = format!("character map [{h}]");
        let e: Table = map.raw_get(h as i64).map_err(lua_err(&ctx))?;
        let base = string_field(&e, "base", &ctx)?;
        let answers: Table = e.raw_get("templates").map_err(lua_err(&ctx))?;
        let mut templates = Vec::new();
        for u in 1..=PROFILE_BYTE_MAX {
            match answers.raw_get::<Value>(u).map_err(lua_err(&ctx))? {
                Value::String(s) => templates.push(s.to_string_lossy()),
                v => return Err(format!("{ctx}: upgrade {u}'s template is a {}, not a string", v.type_name())),
            }
        }
        while templates.last() == Some(&base) {
            templates.pop();
        }
        let models: Vec<String> = e
            .raw_get::<Table>("models")
            .map_err(lua_err(&ctx))?
            .sequence_values::<String>()
            .collect::<Result<_, _>>()
            .map_err(lua_err(&format!("{ctx}.models")))?;
        out.push(CharacterEntry { base, templates, models });
    }
    Ok(out)
}

impl RetailLevel {
    /// Read a level's lists from its wardrobe (`wifpmcinterior`) and player (`mrxplayer`) chunks.
    ///
    /// Refuses a level whose map does not restore its own wardrobe: each hero's map entry `k` must be
    /// that hero's wardrobe row `k + 1`, and every wardrobe hero must have exactly one map entry.
    pub fn read(wardrobe_luaq: &[u8], player_luaq: &[u8]) -> Result<RetailLevel, String> {
        let wardrobe = read_wardrobe(wardrobe_luaq)?;
        let entries = read_map(player_luaq)?;
        let mut map = Vec::new();
        for (i, e) in entries.into_iter().enumerate() {
            let hero = wearer_table_key(&e.base).ok_or_else(|| {
                format!("character map [{}]: base {:?} is not a wardrobe hero", i + 1, e.base)
            })?;
            if map.iter().any(|(h, _): &(String, CharacterEntry)| h == hero) {
                return Err(format!("character map [{}]: a second entry for {hero}", i + 1));
            }
            let rows = &wardrobe[hero];
            for (k, row) in rows.iter().enumerate().skip(1) {
                if e.models.get(k - 1) != Some(&row.model) {
                    return Err(format!(
                        "character map [{}] ({hero}): models[{k}] is {:?}, but wardrobe row {} is {:?}; \
                         the saved costume would restore the wrong outfit",
                        i + 1,
                        e.models.get(k - 1),
                        k + 1,
                        row.model
                    ));
                }
            }
            map.push((hero.to_string(), e));
        }
        for h in HEROES {
            if !map.iter().any(|(m, _)| m == h) {
                return Err(format!("the character map has no entry for the wardrobe hero {h}"));
            }
        }
        Ok(RetailLevel { wardrobe, map })
    }

    /// The models of `hero`'s map that no wardrobe row offers, in map order.
    pub fn restore_only(&self, hero: &str) -> &[String] {
        let rows = self.wardrobe[hero].len();
        let (_, e) = self.map.iter().find(|(h, _)| h == hero).expect("RetailLevel::read checked every hero");
        &e.models[rows - 1..]
    }

    /// What the join puts on the hero of index `i_index` (the profile's hero byte, `1..`) for the
    /// costume byte `costume`, by `GetTemplateAndModelName`'s rule; `None` when the map has no such
    /// hero.
    pub fn restore(&self, i_index: u8, costume: u8) -> Option<Restore<'_>> {
        let (_, e) = self.map.get(usize::from(i_index).checked_sub(1)?)?;
        Some(match costume {
            0 => Restore::Base,
            c => match e.models.get(usize::from(c) - 1) {
                Some(m) => Restore::Model(m),
                None => Restore::Unmapped,
            },
        })
    }
}

/// What the join does with a costume byte ([`RetailLevel::restore`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore<'a> {
    /// Byte 0: no `SetOutfit`, the hero keeps its template's model.
    Base,
    /// `models[byte]`: the join puts this model on.
    Model(&'a str),
    /// Past the map: `models[byte]` is nil, so the join puts nothing on, exactly as for byte 0.
    Unmapped,
}

/// The vz level's retail lists, read from the `vz.wad` at `vz_wad` alone.
pub fn read_vz(vz_wad: &Path) -> Result<RetailLevel, String> {
    let mut stack = crate::game::GameStack::open(&[vz_wad.to_path_buf()]).map_err(|e| e.to_string())?;
    let mut chunk = |name: &str| -> Result<Vec<u8>, String> {
        let c = stack
            .container_for_asset(m2(name), TYPE_HASH_SCRIPT, TYPE_ID_SCRIPT)
            .ok_or_else(|| format!("{}: no {name} script", vz_wad.display()))?;
        luaq_of(&c).map_err(|e| format!("{}: {name}: {e}", vz_wad.display()))
    };
    let wardrobe = chunk(WARDROBE_SCRIPT)?;
    let player = chunk(PLAYER_SCRIPT)?;
    RetailLevel::read(&wardrobe, &player)
}

/// Build `level`'s lists from its retail lists and the `added` outfits, in the order given.
///
/// Refuses an added outfit with no hero, a hero twice, a name that is not a hero, a level twice or an
/// empty level list; a slug equal to a retail row's `Name` of a hero it is offered to; and the same
/// `(shipment, slug)` twice for one hero.
pub fn generate(level: &str, retail: &RetailLevel, added: &[AddedOutfit]) -> Result<LevelLists, String> {
    let mut heroes = Vec::new();
    for (hero, entry) in &retail.map {
        let mut rows: Vec<Row> = retail.wardrobe[hero]
            .iter()
            .map(|r| Row {
                identity: Identity::Retail { name: r.name.clone() },
                model: r.model.clone(),
                display: r.display.clone(),
                listing: Listing::Retail,
                chatter: None,
            })
            .collect();
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        for a in added {
            if !joins(a, level)? || !offered_to(a, hero)? {
                continue;
            }
            let who = format!("{} outfit {:?}", a.shipment, a.slug);
            if retail.wardrobe[hero].iter().any(|r| r.name == a.slug) {
                return Err(format!("{who}: {hero} already has a retail outfit named {:?} in {level}", a.slug));
            }
            if !seen.insert((a.shipment.clone(), a.slug.clone())) {
                return Err(format!("{who}: offered to {hero} twice in {level}"));
            }
            rows.push(Row {
                identity: Identity::Added { shipment: a.shipment.clone(), slug: a.slug.clone() },
                model: a.model.clone(),
                display: a.display.clone(),
                listing: Listing::Always,
                chatter: a.chatter.clone(),
            });
        }
        let mut models: Vec<String> = rows[1..].iter().map(|r| r.model.clone()).collect();
        models.extend(retail.restore_only(hero).iter().cloned());
        heroes.push(HeroList {
            hero: hero.clone(),
            rows,
            map: CharacterEntry { base: entry.base.clone(), templates: entry.templates.clone(), models },
        });
    }
    Ok(LevelLists { level: level.to_string(), heroes })
}

/// Whether `a` joins `level`'s wardrobe. Errors on an empty or repeating level list.
fn joins(a: &AddedOutfit, level: &str) -> Result<bool, String> {
    let Some(levels) = &a.levels else {
        return Ok(level == DEFAULT_LEVEL);
    };
    if levels.is_empty() {
        return Err(format!("{} outfit {:?}: levels is empty", a.shipment, a.slug));
    }
    let set: BTreeSet<&String> = levels.iter().collect();
    if set.len() != levels.len() {
        return Err(format!("{} outfit {:?}: levels {levels:?} names a level twice", a.shipment, a.slug));
    }
    Ok(levels.iter().any(|l| l == level))
}

/// Whether `a` is offered to the wardrobe hero `hero`. Errors on an empty, unknown or repeating hero.
fn offered_to(a: &AddedOutfit, hero: &str) -> Result<bool, String> {
    if a.wearers.is_empty() {
        return Err(format!("{} outfit {:?}: no wearer", a.shipment, a.slug));
    }
    let mut keys = BTreeSet::new();
    for w in &a.wearers {
        let k = wearer_table_key(w)
            .ok_or_else(|| format!("{} outfit {:?}: wearer {w:?} is not a hero ({HEROES:?})", a.shipment, a.slug))?;
        if !keys.insert(k) {
            return Err(format!("{} outfit {:?}: wearer {w:?} names {k} twice", a.shipment, a.slug));
        }
    }
    Ok(keys.contains(hero))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wardrobe chunk with retail's shape: two Mattias rows, two Chris rows, three Jennifer rows.
    fn wardrobe() -> Vec<u8> {
        mercs2_luac::compile(
            r#"
import("MrxGui")
_tOutfits = {
  chris = { { Name = "Original", Model = "c0", PlayerVisibleName = "[c0]" },
            { Name = "Vacation", Model = "c1", PlayerVisibleName = "[c1]" } },
  jennifer = { { Name = "Original", Model = "j0", PlayerVisibleName = "[j0]" },
               { Name = "Rebel", Model = "j1", PlayerVisibleName = "[j1]" },
               { Name = "Tactical", Model = "j2", PlayerVisibleName = "[j2]" } },
  mattias = { { Name = "Original", Model = "m0", PlayerVisibleName = "[m0]" },
              { Name = "Suit", Model = "m1", PlayerVisibleName = "[m1]" } },
}
"#,
            "wifpmcinterior",
        )
        .unwrap()
    }

    /// A player chunk with retail's map shape and its `GetTemplateAndModelName` / `SetCharacterMap`.
    fn player(map: &str) -> Vec<u8> {
        mercs2_luac::compile(
            &format!(
                r#"
import("MrxCoop")
local _tCharacterMap = {map}
function GetTemplateAndModelName(t)
  if type(t) ~= "table" then return t end
  local iIndex = t.iIndex
  if not iIndex then return end
  local e = _tCharacterMap[iIndex]
  if not e then return end
  local sTemplate = t.iUpgrade and e.templates and e.templates[t.iUpgrade] or e.base
  local sModel = t.iCostume and e.models and e.models[t.iCostume] or nil
  return sTemplate, sModel
end
function SetCharacterMap(t)
  if t == nil then return false end
  _tCharacterMap = t
  return true
end
"#
            ),
            "mrxplayer",
        )
        .unwrap()
    }

    const MAP: &str = r#"{
  { base = "mattias", templates = { "mu1", "mu2", "mu3" }, models = { "m1", "mx", "my" } },
  { base = "chris", templates = { "cu1", "cu2", "cu3" }, models = { "c1", "cx" } },
  { base = "jen", templates = { "ju1", "ju2", "ju3" }, models = { "j1", "j2" } },
}"#;

    fn retail() -> RetailLevel {
        RetailLevel::read(&wardrobe(), &player(MAP)).unwrap()
    }

    fn added(shipment: &str, slug: &str, model: &str, wearers: &[&str]) -> AddedOutfit {
        AddedOutfit {
            shipment: shipment.into(),
            slug: slug.into(),
            model: model.into(),
            display: format!("[{slug}]"),
            wearers: wearers.iter().map(|w| w.to_string()).collect(),
            levels: None,
            chatter: None,
        }
    }

    fn hero<'a>(l: &'a LevelLists, h: &str) -> &'a HeroList {
        l.heroes.iter().find(|x| x.hero == h).unwrap()
    }

    #[test]
    fn the_retail_lists_read_from_the_chunks() {
        let r = retail();
        assert_eq!(r.wardrobe["jennifer"].len(), 3);
        assert_eq!(r.wardrobe["mattias"][1], RetailRow { name: "Suit".into(), model: "m1".into(), display: "[m1]".into() });
        let heroes: Vec<&str> = r.map.iter().map(|(h, _)| h.as_str()).collect();
        assert_eq!(heroes, ["mattias", "chris", "jennifer"], "map order, jen joined to jennifer");
        assert_eq!(r.map[2].1.base, "jen");
        assert_eq!(r.map[0].1.templates, ["mu1", "mu2", "mu3"]);
        assert_eq!(r.restore_only("mattias"), ["mx", "my"]);
        assert_eq!(r.restore_only("jennifer"), Vec::<String>::new());
    }

    #[test]
    fn restore_follows_get_template_and_model_name() {
        let r = retail();
        assert_eq!(r.restore(1, 0), Some(Restore::Base));
        assert_eq!(r.restore(1, 1), Some(Restore::Model("m1")));
        assert_eq!(r.restore(1, 3), Some(Restore::Model("my")));
        assert_eq!(r.restore(1, 4), Some(Restore::Unmapped));
        assert_eq!(r.restore(3, 2), Some(Restore::Model("j2")));
        assert_eq!(r.restore(0, 1), None);
        assert_eq!(r.restore(4, 1), None);
    }

    #[test]
    fn a_map_that_does_not_restore_its_wardrobe_is_refused() {
        let bad = MAP.replace(r#"models = { "m1", "mx", "my" }"#, r#"models = { "mx", "m1" }"#);
        let e = RetailLevel::read(&wardrobe(), &player(&bad)).unwrap_err();
        assert!(e.contains("models[1] is Some(\"mx\"), but wardrobe row 2 is \"m1\""), "{e}");
    }

    #[test]
    fn a_set_character_map_that_does_not_replace_the_map_is_refused() {
        let p =mercs2_luac::compile(
            &format!(
                "local _tCharacterMap = {MAP}\n\
                 function GetTemplateAndModelName(t)\n\
                   local e = _tCharacterMap[t.iIndex]; if not e then return end\n\
                   return t.iUpgrade and e.templates[t.iUpgrade] or e.base, t.iCostume and e.models[t.iCostume] or nil\n\
                 end\n\
                 function SetCharacterMap(t) return true end\n"
            ),
            "mrxplayer",
        )
        .unwrap();
        let e = RetailLevel::read(&wardrobe(), &p).unwrap_err();
        assert!(e.contains("SetCharacterMap does not replace the map"), "{e}");
    }

    #[test]
    fn a_wardrobe_missing_a_hero_is_refused() {
        let w = mercs2_luac::compile(
            r#"_tOutfits = { chris = { { Name = "Original", Model = "c0", PlayerVisibleName = "x" } },
                            mattias = { { Name = "Original", Model = "m0", PlayerVisibleName = "x" } } }"#,
            "wifpmcinterior",
        )
        .unwrap();
        assert_eq!(read_wardrobe(&w).unwrap_err(), "_tOutfits has no jennifer list");
    }

    /// Retail rows keep their order, added rows follow in the order given, and map entry `k` is row
    /// `k + 1`'s model with the retail restore-only models after them.
    #[test]
    fn added_rows_follow_the_retail_rows_and_the_map_follows_the_rows() {
        let r = retail();
        let l = generate(
            "vz",
            &r,
            &[added("biua", "Obama", "pmc_hum_obama", &["mattias", "chris", "jen"]), added("other", "Coat", "coat", &["mattias"])],
        )
        .unwrap();
        let m = hero(&l, "mattias");
        let models: Vec<&str> = m.rows.iter().map(|r| r.model.as_str()).collect();
        assert_eq!(models, ["m0", "m1", "pmc_hum_obama", "coat"]);
        assert_eq!(m.map.models, ["m1", "pmc_hum_obama", "coat", "mx", "my"]);
        for (k, model) in m.map.models.iter().enumerate().take(m.rows.len() - 1) {
            assert_eq!(model, &m.rows[k + 1].model, "map entry {} is row {}", k + 1, k + 2);
        }
        assert_eq!(m.rows[0].listing, Listing::Retail);
        assert_eq!(m.rows[2].listing, Listing::Always);
        assert_eq!(m.rows[2].identity, Identity::Added { shipment: "biua".into(), slug: "Obama".into() });
        let j = hero(&l, "jennifer");
        assert_eq!(j.rows.len(), 4, "the universal outfit joins jennifer through `jen`");
        assert_eq!(j.map.models, ["j1", "j2", "pmc_hum_obama"]);
        assert_eq!(j.map.templates, ["ju1", "ju2", "ju3"]);
        assert_eq!(l.heroes.iter().map(|h| h.hero.as_str()).collect::<Vec<_>>(), ["mattias", "chris", "jennifer"]);
    }

    #[test]
    fn the_same_set_gives_the_same_lists() {
        let a = [added("a", "One", "one", &["chris"]), added("b", "Two", "two", &["chris", "mattias"])];
        let r1 = RetailLevel::read(&wardrobe(), &player(MAP)).unwrap();
        let r2 = RetailLevel::read(&wardrobe(), &player(MAP)).unwrap();
        assert_eq!(format!("{:?}", generate("vz", &r1, &a).unwrap()), format!("{:?}", generate("vz", &r2, &a).unwrap()));
    }

    #[test]
    fn levels_scope_an_outfit() {
        let mut o = added("biua", "Gramps", "pmc_hum_mattias_v5", &["mattias"]);
        let r = retail();
        let rows = |level: &str, o: &AddedOutfit| hero(&generate(level, &r, &[o.clone()]).unwrap(), "mattias").rows.len();
        assert_eq!((rows("vz", &o), rows("dlc01", &o)), (3, 2), "no levels: the main campaign alone");
        o.levels = Some(vec!["dlc01".into()]);
        assert_eq!((rows("vz", &o), rows("dlc01", &o)), (2, 3));
        o.levels = Some(vec!["vz".into(), "dlc01".into()]);
        assert_eq!((rows("vz", &o), rows("dlc01", &o)), (3, 3));
        o.levels = Some(vec![]);
        assert!(generate("vz", &r, &[o.clone()]).unwrap_err().contains("levels is empty"));
        o.levels = Some(vec!["vz".into(), "vz".into()]);
        assert!(generate("vz", &r, &[o]).unwrap_err().contains("names a level twice"));
    }

    #[test]
    fn a_slug_equal_to_a_retail_name_is_refused() {
        let e = generate("vz", &retail(), &[added("x", "Suit", "suit2", &["mattias"])]).unwrap_err();
        assert!(e.contains("mattias already has a retail outfit named \"Suit\""), "{e}");
        // Another hero's retail name is free: Rebel is Jennifer's.
        assert!(generate("vz", &retail(), &[added("x", "Rebel", "r", &["mattias"])]).is_ok());
    }

    #[test]
    fn one_shipment_offering_a_slug_twice_to_a_hero_is_refused_and_two_shipments_may_share_one() {
        let e = generate("vz", &retail(), &[added("x", "Coat", "a", &["chris"]), added("x", "Coat", "b", &["chris"])])
            .unwrap_err();
        assert!(e.contains("offered to chris twice"), "{e}");
        let l = generate("vz", &retail(), &[added("x", "Coat", "a", &["chris"]), added("y", "Coat", "b", &["chris"])]).unwrap();
        assert_eq!(hero(&l, "chris").rows.len(), 4);
    }

    #[test]
    fn bad_wearers_are_refused() {
        let r = retail();
        assert!(generate("vz", &r, &[added("x", "A", "a", &[])]).unwrap_err().contains("no wearer"));
        assert!(generate("vz", &r, &[added("x", "A", "a", &["eva"])]).unwrap_err().contains("\"eva\" is not a hero"));
        assert!(generate("vz", &r, &[added("x", "A", "a", &["jen", "jennifer"])]).unwrap_err().contains("names jennifer twice"));
    }
}
