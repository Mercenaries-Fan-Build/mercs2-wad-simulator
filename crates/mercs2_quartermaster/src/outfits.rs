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
//!
//! [`tails`] turns a level's lists into the Lua the link splices onto the wardrobe, the player script
//! and the master ([`OutfitTails`]): the added rows, always listed; the map, installed with
//! `SetCharacterMap`; the worn row's identity saved beside the costume byte and found again on load,
//! so a set that changed between save and load restores the right outfit or the hero's own model,
//! never a neighbour; and a row's chatter set on the join and on a wardrobe change.

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
    /// Whether the wardrobe's own `_SelectOutfit` lists a row marked `bAlwaysVisible` past the unlock
    /// count (dlc01's does, `dlc01_pmcinterior.lua:2055-2068`; vz's does not), found by running it
    /// ([`read_lists_always_visible`]).
    pub lists_always_visible: bool,
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

/// Runs the wardrobe's `_SelectOutfit` for Mattias at an unlock count of 2, wearing row 1, with a
/// `bAlwaysVisible` row appended past every retail row, and reports which rows it offered. The count
/// rule (`i <= count + 1`, not row 2) must offer row 3; whether the appended row is offered is the
/// answer.
const PROBE_ALWAYS_VISIBLE: &str = r#"
local sHero = ...
local tListed = {}
Player = {
  GetPrimaryCharacter = function() return 1 end,
  GetSecondaryCharacter = function() return 2 end,
  GetProfileCostume = function() return 0 end,
  GetAvailableCostumes = function() return 2 end,
  GetLocalPlayer = function() return 1 end,
}
Net = {
  IsServer = function() return false end,
  IsClient = function() return false end,
  HasPlayerUnlockedCode = function() return false end,
  SendCustomEvent = function() end,
}
MrxUtil = { GetCharacterIdentity = function() return sHero end }
LTILibName = { ChangeShellState = function() end }
MrxGui = { DisplayDialogBox = function() return true end }
MrxMultiPageMenu = {
  Reset = function() end,
  Display = function() end,
  AddOption = function(sText, fCallback, tArgs)
    if type(tArgs) == "table" and tArgs[2] ~= nil then
      tListed[#tListed + 1] = tArgs[2]
    end
  end,
}
_nAvailableCostumes = 2
assert(type(_SelectOutfit) == "function", "the wardrobe script defines no _SelectOutfit")
local tRows = _tOutfits[sHero]
local iProbe = #tRows + 1
tRows[iProbe] = { Name = "qm:probe", Model = "qm:probe", PlayerVisibleName = "qm:probe", bAlwaysVisible = true }
_SelectOutfit(1)
local bProbe, bCounted = false, false
for _, i in ipairs(tListed) do
  if i == iProbe then bProbe = true end
  if i == 3 then bCounted = true end
end
assert(bCounted, "_SelectOutfit at an unlock count of 2 did not offer row 3, so it does not follow the retail count rule")
return bProbe
"#;

/// Whether the wardrobe chunk's `_SelectOutfit` lists a `bAlwaysVisible` row past the unlock count
/// ([`PROBE_ALWAYS_VISIBLE`]).
fn read_lists_always_visible(luaq: &[u8]) -> Result<bool, String> {
    let lua = run_module(luaq, WARDROBE_SCRIPT)?;
    lua.load(PROBE_ALWAYS_VISIBLE)
        .set_name("=probe _SelectOutfit")
        .call::<bool>("mattias".to_string())
        .map_err(lua_err("running the wardrobe's _SelectOutfit"))
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
        let lists_always_visible = read_lists_always_visible(wardrobe_luaq)?;
        Ok(RetailLevel { wardrobe, map, lists_always_visible })
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

/// The chatter set names qm knows, with their evidence (`data/chatter_sets.tsv`).
const CHATTER_SETS_TSV: &str = include_str!("../data/chatter_sets.tsv");

/// The component class whose one field is a template's chatter set, as `pandemic_hash_m2(name)`.
pub const CHATTER_SET_CLASS: &str = "ChatterSet";

/// Every chatter set name of `data/chatter_sets.tsv`, in file order.
pub fn chatter_sets() -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut header = false;
    for (i, line) in CHATTER_SETS_TSV.lines().enumerate() {
        if line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if !header {
            if cols != ["name", "evidence"] {
                return Err(format!("data/chatter_sets.tsv line {}: the header is {cols:?}, not [name, evidence]", i + 1));
            }
            header = true;
            continue;
        }
        let [name, evidence] = cols.as_slice() else {
            return Err(format!("data/chatter_sets.tsv line {}: {} columns, not 2", i + 1, cols.len()));
        };
        if name.is_empty() || evidence.is_empty() {
            return Err(format!("data/chatter_sets.tsv line {}: an empty field", i + 1));
        }
        if out.iter().any(|n: &String| m2(n) == m2(name)) {
            return Err(format!("data/chatter_sets.tsv line {}: {name} is listed twice", i + 1));
        }
        out.push(name.to_string());
    }
    Ok(out)
}

/// Each wardrobe hero's chatter set, named: the `ChatterSet` of every template its map entry can
/// answer (the base and each upgrade template), which must be one value, and the
/// `data/chatter_sets.tsv` name that hashes to it. A template the worldentity does not hold once, a
/// template with no `ChatterSet` or more than one, templates of one hero that disagree, and a value no
/// listed name hashes to are errors.
pub fn hero_chatter(
    we: &mercs2_formats::worldentity::WorldEntity,
    retail: &RetailLevel,
) -> Result<BTreeMap<String, String>, String> {
    use mercs2_formats::schema::FieldValue;
    use mercs2_formats::worldentity::{Value as Field, TEMPLATE_KEY_BIT};
    let names = we.name_hashes()?;
    let sets = chatter_sets()?;
    let mut out = BTreeMap::new();
    for (hero, e) in &retail.map {
        let mut values = BTreeSet::new();
        for t in std::iter::once(&e.base).chain(&e.templates) {
            let h = m2(t);
            let keys: Vec<u32> = names
                .iter()
                .filter(|(n, _)| *n == h)
                .flat_map(|(_, k)| k.iter().copied().filter(|k| k & TEMPLATE_KEY_BIT != 0))
                .collect();
            let [key] = keys.as_slice() else {
                return Err(format!("{hero}: the worldentity holds {} templates named {t:?}, not one", keys.len()));
            };
            let mut found = Vec::new();
            for c in we.groups_of(CHATTER_SET_CLASS) {
                for r in c.records.iter().filter(|r| r.keys.contains(key)) {
                    match c.decode(&r.payload)?.as_slice() {
                        [Field::Field(FieldValue::U32(v))] => found.push(*v),
                        other => return Err(format!("template {t:?}: its {CHATTER_SET_CLASS} decodes as {other:?}, not one 32-bit field")),
                    }
                }
            }
            let [v] = found.as_slice() else {
                return Err(format!("template {t:?} has {} {CHATTER_SET_CLASS} records, not one", found.len()));
            };
            values.insert(*v);
        }
        let value = match values.iter().collect::<Vec<_>>().as_slice() {
            [v] => **v,
            vs => return Err(format!("{hero}: its templates hold different chatter sets {vs:08X?}")),
        };
        let name = sets.iter().find(|n| m2(n) == value).ok_or_else(|| {
            format!("{hero}: chatter set 0x{value:08X} is no name data/chatter_sets.tsv lists")
        })?;
        out.insert(hero.clone(), name.clone());
    }
    Ok(out)
}

/// The hook id qm's outfit code names in a failure and in the link's check of the functions it wraps.
pub const OUTFITS_ID: &str = "outfits";

/// The Lua a level's outfits splice onto each host, each a [`crate::loader::Tail`] that starts with
/// qm's prologue ([`crate::loader::prologue`]), so it runs whether or not the link splices qm's hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutfitTails {
    /// Onto [`WARDROBE_SCRIPT`]: appends the added rows to `_tOutfits`, refusing a wardrobe whose
    /// lists are not the ones generated from; in a level whose `_SelectOutfit` does not list
    /// `bAlwaysVisible` rows, replaces it with retail's rule plus that one; saves the worn row's identity
    /// (`tQmOutfit`) with `SaveSingleton`; and, when a row has a chatter set, sets it on a wardrobe
    /// change (`_CompleteChangeOutfit`) and on a co-op partner's (`NetEventCallback`).
    pub wardrobe: crate::loader::Tail,
    /// Onto [`PLAYER_SCRIPT`]: installs the generated map with `SetCharacterMap` and, when a row has a
    /// chatter set, sets it on the join (`OnPlayerJoined`).
    pub player: crate::loader::Tail,
    /// Onto the level's master ([`crate::hooks::MASTER_HOST`], or a level's own master): before
    /// `LoadSingleton` runs, sets the costume byte to the saved `tQmOutfit`'s position in the generated
    /// list, or 0 (the hero's own model) when that outfit is gone.
    pub master: crate::loader::Tail,
}

fn s(text: &str) -> String {
    crate::loader::lua_string(text)
}

/// The guard every tail's own code runs under: an error goes to `QM.fail`.
const GUARD: &str = "local function guard(f, ...)\n  local ok, err = pcall(f, ...)\n  if not ok then\n    QM.fail(\"Quartermaster\", \"outfits\", err)\n  end\n  return ok\nend\n";

/// `target`, which must be a function of the host, under the name `original`; a host where it is not a
/// function fails and the tail stops.
fn take_original(host: &str, target: &str) -> String {
    format!(
        "local original = {target}\nif type(original) ~= \"function\" then\n  QM.fail(\"Quartermaster\", \"outfits\", {where_} .. \" is not a function when qm's outfits are spliced\")\n  return\nend\n",
        where_ = s(&format!("{host}.{target}"))
    )
}

/// The vz wardrobe's `_SelectOutfit` (`wifpmcinterior.lua:1390-1446`), with two changes: a row marked
/// `bAlwaysVisible` is offered whatever the unlock count, and a hero with such a row opens the menu where
/// retail shows the custom-outfit tutorial dialog instead (count ≤ 1 and no unlock code).
const SELECT_OUTFIT: &str = r#"_SelectOutfit = function(uGuid)
    if uGuid == Player.GetPrimaryCharacter() then
      _WardrobeOpen = true
    end
    if Net.IsServer() then
      _SetCustomOutfitMarker(false)
    end
    if Net.IsServer() and uGuid == Player.GetSecondaryCharacter() then
      Net.SendCustomEvent("WifPmcInterior", NETEVENT_CHANGEOUTFIT, {})
      return
    end
    local sHero = MrxUtil.GetCharacterIdentity(uGuid)
    local tOutfits = _tOutfits[sHero]
    local nCurrentOutfit = Player.GetProfileCostume()
    if nCurrentOutfit == nil or nCurrentOutfit == 0 then
      nCurrentOutfit = 0
    end
    nCurrentOutfit = nCurrentOutfit + 1
    local nAvailableOutfits = 0
    if Net.IsClient() then
      nAvailableOutfits = Player.GetAvailableCostumes()
    else
      nAvailableOutfits = GetAvailableCostumes()
    end
    if nAvailableOutfits == nil or nAvailableOutfits == 0 then
      nAvailableOutfits = 0
    end
    local bAlwaysVisible = false
    for _, tOutfit in ipairs(tOutfits) do
      if tOutfit.bAlwaysVisible then
        bAlwaysVisible = true
      end
    end
    if nAvailableOutfits <= 1 and not Net.HasPlayerUnlockedCode() and not bAlwaysVisible then
      LTILibName.ChangeShellState(true)
      _CostumeDialogBox = MrxGui.DisplayDialogBox(Player.GetLocalPlayer(), "[Tutorial.CustomOutfit]", {
        "[Generic.Continue]"
      }, 1, _CloseCostumeDialog, {}, 48, 36, "left", "bottom", false, 1)
      return
    end
    nAvailableOutfits = nAvailableOutfits + 1
    MrxMultiPageMenu.Reset()
    if Net.HasPlayerUnlockedCode() and nCurrentOutfit ~= 2 then
      MrxMultiPageMenu.AddOption(tOutfits[2].PlayerVisibleName, _ChangeOutfit, {uGuid, 2})
    end
    for i, tOutfit in ipairs(tOutfits) do
      if i ~= nCurrentOutfit and (tOutfit.bAlwaysVisible or (i ~= 2 and nAvailableOutfits >= i)) then
        MrxMultiPageMenu.AddOption(tOutfit.PlayerVisibleName, _ChangeOutfit, {uGuid, i})
      end
    end
    if Net.IsServer() then
      MrxMultiPageMenu.AddOption("[Generic.Cancel]", _SetCustomOutfitMarker, {true}, true, true)
    elseif Net.IsClient() then
      MrxMultiPageMenu.AddOption("[Generic.Cancel]", Net.SendCustomEvent, {
        "WifPmcInterior",
        NETEVENT_NOTIFYOUTFITCHANGE,
        {0, 0}
      }, true, true)
    end
    MrxMultiPageMenu.Display("[Generic.ChooseOutfit]:")
  end
"#;

/// The wardrobe's chatter handling: before a change, a character qm gave a chatter set gets its hero's
/// own back; after it, a row with a chatter set gives it.
const WARDROBE_CHATTER: &str = r#"local function ChatterBefore(uGuid)
  if uGuid ~= nil and QM.outfit_chatter[uGuid] then
    local sHero = MrxUtil.GetCharacterIdentity(uGuid)
    local sSet = tHeroChatter[sHero]
    if sSet == nil then
      error("hero " .. tostring(sHero) .. " has no chatter set to return to")
    end
    QM.outfit_chatter[uGuid] = nil
    Human.SetChatterSet(uGuid, sSet)
  end
end
local function ChatterAfter(uGuid, iRow)
  if uGuid == nil then
    return
  end
  local tRows = _tOutfits[MrxUtil.GetCharacterIdentity(uGuid)]
  local tRow = tRows ~= nil and tRows[iRow] or nil
  if tRow ~= nil and tRow.sQmChatter ~= nil then
    Human.SetChatterSet(uGuid, tRow.sQmChatter)
    QM.outfit_chatter[uGuid] = true
  end
end
do
  local original = _CompleteChangeOutfit
  if type(original) ~= "function" then
    QM.fail("Quartermaster", "outfits", "wifpmcinterior._CompleteChangeOutfit is not a function when qm's outfits are spliced")
    return
  end
  _CompleteChangeOutfit = function(uGuid, iIndex, ...)
    guard(ChatterBefore, uGuid)
    local n, r = QM.pack(original(uGuid, iIndex, ...))
    guard(ChatterAfter, uGuid, iIndex)
    return unpack(r, 1, n)
  end
end
do
  local original = NetEventCallback
  if type(original) ~= "function" then
    QM.fail("Quartermaster", "outfits", "wifpmcinterior.NetEventCallback is not a function when qm's outfits are spliced")
    return
  end
  NetEventCallback = function(nEventType, tArgs, ...)
    local uGuid = nil
    if nEventType == NETEVENT_NOTIFYOUTFITCHANGE and type(tArgs) == "table" then
      if tArgs[1] == 1 then
        uGuid = Player.GetPrimaryCharacter()
      elseif tArgs[1] == 2 then
        uGuid = Player.GetSecondaryCharacter()
      end
    end
    if uGuid ~= nil then
      guard(ChatterBefore, uGuid)
    end
    local n, r = QM.pack(original(nEventType, tArgs, ...))
    if uGuid ~= nil then
      guard(ChatterAfter, uGuid, tArgs[2])
    end
    return unpack(r, 1, n)
  end
end
"#;

/// `SaveSingleton`'s table gains `tQmOutfit`: the hero and the identity of the row the costume byte
/// names. A byte past the hero's list (no wardrobe row) saves none, and the load keeps the byte.
const WARDROBE_SAVE: &str = r#"do
  local original = SaveSingleton
  if type(original) ~= "function" then
    QM.fail("Quartermaster", "outfits", "wifpmcinterior.SaveSingleton is not a function when qm's outfits are spliced")
    return
  end
  SaveSingleton = function(...)
    local n, r = QM.pack(original(...))
    guard(function()
      local tSave = r[1]
      if type(tSave) ~= "table" then
        error("WifPmcInterior.SaveSingleton returned a " .. type(tSave) .. ", not a table")
      end
      local sHero = MrxUtil.GetCharacterIdentity(Player.GetLocalCharacter())
      local tRows = _tOutfits[sHero]
      if type(tRows) ~= "table" then
        error("the local character " .. tostring(sHero) .. " has no wardrobe list")
      end
      local tRow = tRows[(Player.GetProfileCostume() or 0) + 1]
      if tRow ~= nil then
        tSave.tQmOutfit = { sHero = sHero, sShipment = tRow.sQmShipment, sName = tRow.Name }
      end
    end)
    return unpack(r, 1, n)
  end
end
"#;

/// The master's `LoadSingleton`, before its body: the saved `tQmOutfit` found in the generated list sets
/// the costume byte to its position, one no longer listed sets 0; a save without it keeps the byte. The
/// host's own save path only (retail's guard, `xQ!L.lua:635`); a co-op client restores from the byte.
const MASTER_LOAD: &str = r#"do
  local original = LoadSingleton
  if type(original) ~= "function" then
    QM.fail("Quartermaster", "outfits", "the master's LoadSingleton is not a function when qm's outfits are spliced")
    return
  end
  LoadSingleton = function(tSaveData, ...)
    guard(function()
      if Net.IsMultiplayer() and not Net.IsServer() then
        return
      end
      if type(tSaveData) ~= "table" or type(tSaveData.tPmcInteriorData) ~= "table" then
        return
      end
      local tSaved = tSaveData.tPmcInteriorData.tQmOutfit
      if tSaved == nil then
        return
      end
      local tRows = tLists[tSaved.sHero]
      if tRows == nil then
        error("the save's outfit names hero " .. tostring(tSaved.sHero) .. ", which has no wardrobe list")
      end
      local nCostume = 0
      for i = 1, #tRows do
        if tRows[i].sShipment == tSaved.sShipment and tRows[i].sName == tSaved.sName then
          nCostume = i - 1
          break
        end
      end
      Player.SetProfileCostume(nCostume)
      print("[qm] outfit restore: " .. tSaved.sHero .. " " .. tostring(tSaved.sShipment) .. "/" .. tostring(tSaved.sName) .. " -> costume " .. nCostume)
    end)
    return original(tSaveData, ...)
  end
end
"#;

/// The join's chatter: a restored row with a chatter set gives it to the character the join made.
const PLAYER_CHATTER: &str = r#"do
  local original = OnPlayerJoined
  if type(original) ~= "function" then
    QM.fail("Quartermaster", "outfits", "mrxplayer.OnPlayerJoined is not a function when qm's outfits are spliced")
    return
  end
  OnPlayerJoined = function(iPlayerId, sPlayerName, tCharacterConfig, ...)
    local n, r = QM.pack(original(iPlayerId, sPlayerName, tCharacterConfig, ...))
    guard(function()
      if type(tCharacterConfig) ~= "table" or tCharacterConfig.iIndex == nil or tCharacterConfig.iCostume == nil then
        return
      end
      local tHero = tChatter[tCharacterConfig.iIndex]
      local sSet = tHero ~= nil and tHero[tCharacterConfig.iCostume] or nil
      if sSet ~= nil then
        local uChar = Player.GetCharacter(Player.GetPlayer(iPlayerId))
        Human.SetChatterSet(uChar, sSet)
        QM.outfit_chatter[uChar] = true
        print("[qm] outfit chatter: player " .. tostring(iPlayerId) .. " " .. sSet)
      end
    end)
    return unpack(r, 1, n)
  end
end
"#;

fn lua_list(items: &[String]) -> String {
    items.iter().map(|i| s(i)).collect::<Vec<_>>().join(", ")
}

fn wrap_tail(body: String, wraps: &[&str]) -> crate::loader::Tail {
    crate::loader::Tail {
        source: format!(
            "(function()\n{}-- [Quartermaster] outfits (generated by qm link; do not edit).\nQM.outfit_chatter = QM.outfit_chatter or {{}}\n{GUARD}{body}end)()\n",
            crate::loader::prologue(crate::manifest::LoadSession::Gameplay)
        ),
        wraps: wraps.iter().map(|t| (t.to_string(), vec![OUTFITS_ID.to_string()])).collect(),
    }
}

/// The tails of `lists` ([`OutfitTails`]); `None` when the level's lists add no row, which leaves every
/// retail script as it ships. `hero_chatter` ([`hero_chatter`]) is required when a row has a chatter set:
/// it is what a change away from that row returns the hero to.
pub fn tails(
    lists: &LevelLists,
    retail: &RetailLevel,
    hero_chatter: Option<&BTreeMap<String, String>>,
) -> Result<Option<OutfitTails>, String> {
    if lists.heroes.iter().all(|h| h.rows.iter().all(|r| r.listing == Listing::Retail)) {
        return Ok(None);
    }
    let chatter = lists.heroes.iter().any(|h| h.rows.iter().any(|r| r.chatter.is_some()));
    let hero_sets = match (chatter, hero_chatter) {
        (true, Some(c)) => Some(c),
        (true, None) => return Err(format!("{}: an outfit has a chatter set, and the heroes' own sets were not read", lists.level)),
        (false, _) => None,
    };

    // The wardrobe: the generated rows after the retail ones, refusing lists other than those read.
    let mut w = String::new();
    w.push_str("local tRetail = {\n");
    for h in &lists.heroes {
        w.push_str(&format!("  {} = {},\n", h.hero, retail.wardrobe[&h.hero].len()));
    }
    w.push_str("}\nlocal tAdded = {\n");
    for h in &lists.heroes {
        w.push_str(&format!("  {} = {{\n", h.hero));
        for r in h.rows.iter().filter(|r| r.listing == Listing::Always) {
            let Identity::Added { shipment, slug } = &r.identity else {
                return Err(format!("{}: an always-listed row of {} is a retail row", lists.level, h.hero));
            };
            w.push_str(&format!(
                "    {{ Name = {}, Model = {}, PlayerVisibleName = {}, bAlwaysVisible = true, sQmShipment = {}{} }},\n",
                s(slug),
                s(&r.model),
                s(&r.display),
                s(shipment),
                r.chatter.as_deref().map(|c| format!(", sQmChatter = {}", s(c))).unwrap_or_default()
            ));
        }
        w.push_str("  },\n");
    }
    w.push_str("}\n");
    w.push_str(&format!(
        "if not guard(function()\n  for _, sHero in ipairs({{ {} }}) do\n    local tRows = _tOutfits[sHero]\n    \
         if type(tRows) ~= \"table\" or #tRows ~= tRetail[sHero] then\n      \
         error(\"_tOutfits.\" .. sHero .. \" has \" .. (type(tRows) == \"table\" and #tRows or type(tRows)) .. \" rows, and qm generated its list from \" .. tRetail[sHero])\n    \
         end\n    for i = 1, #tAdded[sHero] do\n      tRows[#tRows + 1] = tAdded[sHero][i]\n    end\n  end\nend) then\n  return\nend\n",
        lua_list(&lists.heroes.iter().map(|h| h.hero.clone()).collect::<Vec<_>>())
    ));
    let mut wardrobe_wraps = vec!["SaveSingleton"];
    if !retail.lists_always_visible {
        w.push_str(&format!("do\n  {}  {SELECT_OUTFIT}end\n", take_original(WARDROBE_SCRIPT, "_SelectOutfit").replace('\n', "\n  ")));
        wardrobe_wraps.push("_SelectOutfit");
    }
    if let Some(sets) = hero_sets {
        w.push_str("local tHeroChatter = {\n");
        for h in &lists.heroes {
            let set = sets.get(&h.hero).ok_or_else(|| format!("{}: no chatter set was read for {}", lists.level, h.hero))?;
            w.push_str(&format!("  {} = {},\n", h.hero, s(set)));
        }
        w.push_str("}\n");
        w.push_str(WARDROBE_CHATTER);
        wardrobe_wraps.extend(["_CompleteChangeOutfit", "NetEventCallback"]);
    }
    w.push_str(WARDROBE_SAVE);

    // The player script: the generated map, and the join's chatter.
    let mut p = String::from("local tMap = {\n");
    for h in &lists.heroes {
        p.push_str(&format!(
            "  {{ base = {}, templates = {{ {} }}, models = {{ {} }} }},\n",
            s(&h.map.base),
            lua_list(&h.map.templates),
            lua_list(&h.map.models)
        ));
    }
    p.push_str(&format!(
        "}}\nif SetCharacterMap(tMap) ~= true then\n  QM.fail(\"Quartermaster\", \"outfits\", {} .. \".SetCharacterMap refused the generated map\")\n  return\nend\n",
        s(PLAYER_SCRIPT)
    ));
    let mut player_wraps = vec!["SetCharacterMap"];
    if hero_sets.is_some() {
        p.push_str("local tChatter = {\n");
        for (i, h) in lists.heroes.iter().enumerate() {
            let by_byte: Vec<String> = h
                .rows
                .iter()
                .enumerate()
                .filter_map(|(k, r)| r.chatter.as_ref().map(|c| format!("[{k}] = {}", s(c))))
                .collect();
            p.push_str(&format!("  [{}] = {{ {} }},\n", i + 1, by_byte.join(", ")));
        }
        p.push_str("}\n");
        p.push_str(PLAYER_CHATTER);
        player_wraps.push("OnPlayerJoined");
    }

    // The master: the saved identity back to a position.
    let mut m = String::from("local tLists = {\n");
    for h in &lists.heroes {
        m.push_str(&format!("  {} = {{\n", h.hero));
        for r in &h.rows {
            match &r.identity {
                Identity::Retail { name } => m.push_str(&format!("    {{ sName = {} }},\n", s(name))),
                Identity::Added { shipment, slug } => {
                    m.push_str(&format!("    {{ sShipment = {}, sName = {} }},\n", s(shipment), s(slug)))
                }
            }
        }
        m.push_str("  },\n");
    }
    m.push_str("}\n");
    m.push_str(MASTER_LOAD);

    Ok(Some(OutfitTails {
        wardrobe: wrap_tail(w, &wardrobe_wraps),
        player: wrap_tail(p, &player_wraps),
        master: wrap_tail(m, &["LoadSingleton"]),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wardrobe chunk with retail's shape: three Mattias rows, two Chris rows, three Jennifer rows,
    /// and the functions qm's tail wraps. `_SelectOutfit` follows vz's count rule, and with `dlc`
    /// also dlc01's `bAlwaysVisible` rule.
    fn wardrobe_of(dlc: bool) -> Vec<u8> {
        let always = if dlc { "t.bAlwaysVisible or " } else { "" };
        mercs2_luac::compile(
            &format!(
                r#"
import("MrxGui")
_tOutfits = {{
  chris = {{ {{ Name = "Original", Model = "c0", PlayerVisibleName = "[c0]" }},
            {{ Name = "Vacation", Model = "c1", PlayerVisibleName = "[c1]" }} }},
  jennifer = {{ {{ Name = "Original", Model = "j0", PlayerVisibleName = "[j0]" }},
               {{ Name = "Rebel", Model = "j1", PlayerVisibleName = "[j1]" }},
               {{ Name = "Tactical", Model = "j2", PlayerVisibleName = "[j2]" }} }},
  mattias = {{ {{ Name = "Original", Model = "m0", PlayerVisibleName = "[m0]" }},
              {{ Name = "Suit", Model = "m1", PlayerVisibleName = "[m1]" }},
              {{ Name = "Chicken", Model = "m2", PlayerVisibleName = "[m2]" }} }},
}}
function GetAvailableCostumes()
  return _nAvailableCostumes or 1
end
function _SelectOutfit(uGuid)
  local tOutfits = _tOutfits[MrxUtil.GetCharacterIdentity(uGuid)]
  local nCurrent = (Player.GetProfileCostume() or 0) + 1
  local n = GetAvailableCostumes() + 1
  MrxMultiPageMenu.Reset()
  for i, t in ipairs(tOutfits) do
    if i ~= nCurrent and ({always}(i ~= 2 and n >= i)) then
      MrxMultiPageMenu.AddOption(t.PlayerVisibleName, _ChangeOutfit, {{uGuid, i}})
    end
  end
end
function _CompleteChangeOutfit(uGuid, iIndex)
  Player.SetProfileCostume(iIndex - 1)
  Player.SetOutfit(uGuid, _tOutfits[MrxUtil.GetCharacterIdentity(uGuid)][iIndex].Model)
end
function NetEventCallback(nEventType, tArgs)
end
function SaveSingleton()
  return {{ bUnlocked = true }}
end
"#
            ),
            "wifpmcinterior",
        )
        .unwrap()
    }

    fn wardrobe() -> Vec<u8> {
        wardrobe_of(false)
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
function OnPlayerJoined(iPlayerId, sPlayerName, tCharacterConfig, bLocalPlayer, iLocalId)
  if bLocalPlayer then
    tCharacterConfig.iCostume = Player.GetProfileCostume()
  end
  local sTemplate, sModel = GetTemplateAndModelName(tCharacterConfig)
  tJoined = {{ sTemplate, sModel }}
end
"#
            ),
            "mrxplayer",
        )
        .unwrap()
    }

    const MAP: &str = r#"{
  { base = "mattias", templates = { "mu1", "mu2", "mu3" }, models = { "m1", "m2", "mx", "my" } },
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
        assert_eq!(r.restore(1, 4), Some(Restore::Model("my")));
        assert_eq!(r.restore(1, 5), Some(Restore::Unmapped));
        assert_eq!(r.restore(3, 2), Some(Restore::Model("j2")));
        assert_eq!(r.restore(0, 1), None);
        assert_eq!(r.restore(4, 1), None);
    }

    #[test]
    fn a_map_that_does_not_restore_its_wardrobe_is_refused() {
        let bad = MAP.replace(r#"models = { "m1", "m2", "mx", "my" }"#, r#"models = { "mx", "m1", "m2" }"#);
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
        assert_eq!(models, ["m0", "m1", "m2", "pmc_hum_obama", "coat"]);
        assert_eq!(m.map.models, ["m1", "m2", "pmc_hum_obama", "coat", "mx", "my"]);
        for (k, model) in m.map.models.iter().enumerate().take(m.rows.len() - 1) {
            assert_eq!(model, &m.rows[k + 1].model, "map entry {} is row {}", k + 1, k + 2);
        }
        assert_eq!(m.rows[0].listing, Listing::Retail);
        assert_eq!(m.rows[3].listing, Listing::Always);
        assert_eq!(m.rows[3].identity, Identity::Added { shipment: "biua".into(), slug: "Obama".into() });
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
        assert_eq!((rows("vz", &o), rows("dlc01", &o)), (4, 3), "no levels: the main campaign alone");
        o.levels = Some(vec!["dlc01".into()]);
        assert_eq!((rows("vz", &o), rows("dlc01", &o)), (3, 4));
        o.levels = Some(vec!["vz".into(), "dlc01".into()]);
        assert_eq!((rows("vz", &o), rows("dlc01", &o)), (4, 4));
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

    /// The engine and module stubs the tails' runtime tests run against. Calls that matter are recorded
    /// in `tCalls` as `name arg…`; the costume byte is the global `nCostume`; `sHero` is the identity
    /// `MrxUtil.GetCharacterIdentity` answers; the menu's offered rows go to `tListed`.
    const STUBS: &str = r##"
tCalls = {}
local function rec(name, ret)
  return function(...)
    local t = { name }
    for i = 1, select("#", ...) do
      t[#t + 1] = tostring((select(i, ...)))
    end
    tCalls[#tCalls + 1] = table.concat(t, " ")
    return ret
  end
end
nCostume = 0
sHero = "mattias"
NETEVENT_NOTIFYOUTFITCHANGE = 2
Player = {
  GetPrimaryCharacter = function() return 101 end,
  GetSecondaryCharacter = function() return 102 end,
  GetLocalCharacter = function() return 101 end,
  GetLocalPlayer = function() return 1 end,
  GetProfileCostume = function() return nCostume end,
  SetProfileCostume = function(n) nCostume = n end,
  SetOutfit = rec("SetOutfit"),
  GetPlayer = function(i) return 500 + i end,
  GetCharacter = function(u) return u - 399 end,
}
Net = {
  IsServer = function() return false end,
  IsClient = function() return false end,
  IsMultiplayer = function() return false end,
  HasPlayerUnlockedCode = function() return false end,
  SendCustomEvent = rec("SendCustomEvent"),
}
MrxUtil = { GetCharacterIdentity = function() return sHero end }
Human = { SetChatterSet = rec("SetChatterSet") }
Sys = { RequestGameState = rec("RequestGameState") }
LTILibName = { ChangeShellState = function() end }
MrxGui = { DisplayDialogBox = rec("DisplayDialogBox", 1) }
tListed = {}
MrxMultiPageMenu = {
  Reset = function() tListed = {} end,
  Display = function() end,
  AddOption = function(sText, fCallback, tArgs)
    if type(tArgs) == "table" and tArgs[2] ~= nil then
      tListed[#tListed + 1] = tArgs[2]
    end
  end,
}
"##;

    const MASTER: &str = "function LoadSingleton(tSaveData)\n  tLoaded = tSaveData\nend\n";

    fn spliced(base: &[u8], tail: &crate::loader::Tail, name: &str) -> Vec<u8> {
        let appended = mercs2_luac::compile(&tail.source, name).unwrap_or_else(|e| panic!("{name}'s tail: {e}\n{}", tail.source));
        mercs2_luac::splice::append_chunk(base, &appended, name).unwrap()
    }

    /// A VM with the stubs, `import` stubbed, and each chunk run in order.
    fn run(chunks: &[(Vec<u8>, &str)]) -> Lua {
        let lua = Lua::new().unwrap();
        lua.load(STUBS).set_name("=stubs").exec().unwrap();
        lua.load("function import() end").exec().unwrap();
        for (c, name) in chunks {
            let f: Function = lua.load(c).set_name(format!("={name}")).into_function().unwrap();
            f.call::<()>(()).unwrap_or_else(|e| panic!("running {name}: {e}"));
        }
        lua
    }

    fn eval<T: mercs2_luac::rt::FromLuaMulti>(lua: &Lua, code: &str) -> T {
        lua.load(code).set_name("=test").eval::<T>().unwrap_or_else(|e| panic!("{code}: {e}"))
    }

    fn calls(lua: &Lua) -> Vec<String> {
        let t: Table = lua.globals().raw_get("tCalls").unwrap();
        t.sequence_values::<String>().collect::<Result<_, _>>().unwrap()
    }

    fn sets() -> BTreeMap<String, String> {
        [("mattias", "Mattias_Peter01"), ("chris", "Chris_Phil01"), ("jennifer", "Jen_Jen01")]
            .into_iter()
            .map(|(h, s)| (h.to_string(), s.to_string()))
            .collect()
    }

    /// Obama for every hero (with its chatter set), a Coat for Mattias alone.
    fn the_set() -> [AddedOutfit; 2] {
        let mut obama = added("biua", "Obama", "pmc_hum_obama", &["mattias", "chris", "jen"]);
        obama.chatter = Some("Obama_Obama01".into());
        [obama, added("other", "Coat", "coat", &["mattias"])]
    }

    fn linked(dlc: bool) -> (Lua, OutfitTails) {
        let w = wardrobe_of(dlc);
        let r = RetailLevel::read(&w, &player(MAP)).unwrap();
        let t = tails(&generate("vz", &r, &the_set()).unwrap(), &r, Some(&sets())).unwrap().expect("rows were added");
        let master = mercs2_luac::compile(MASTER, "vz").unwrap();
        let lua = run(&[
            (spliced(&w, &t.wardrobe, "wifpmcinterior"), "wifpmcinterior"),
            (spliced(&player(MAP), &t.player, "mrxplayer"), "mrxplayer"),
            (spliced(&master, &t.master, "vz"), "vz"),
        ]);
        (lua, t)
    }

    #[test]
    fn whether_select_outfit_lists_always_visible_rows_is_read_by_running_it() {
        assert!(!RetailLevel::read(&wardrobe_of(false), &player(MAP)).unwrap().lists_always_visible);
        assert!(RetailLevel::read(&wardrobe_of(true), &player(MAP)).unwrap().lists_always_visible);
    }

    #[test]
    fn a_level_the_set_adds_no_row_to_gets_no_tails() {
        let r = retail();
        assert_eq!(tails(&generate("vz", &r, &[]).unwrap(), &r, None), Ok(None));
        let mut o = added("x", "A", "a", &["chris"]);
        o.levels = Some(vec!["dlc01".into()]);
        assert_eq!(tails(&generate("vz", &r, &[o]).unwrap(), &r, None), Ok(None));
    }

    #[test]
    fn a_chatter_row_needs_the_heroes_own_sets() {
        let r = retail();
        let e = tails(&generate("vz", &r, &the_set()).unwrap(), &r, None).unwrap_err();
        assert!(e.contains("the heroes' own sets were not read"), "{e}");
        let mut no_jen = sets();
        no_jen.remove("jennifer");
        let e = tails(&generate("vz", &r, &the_set()).unwrap(), &r, Some(&no_jen)).unwrap_err();
        assert!(e.contains("no chatter set was read for jennifer"), "{e}");
    }

    /// The wardrobe gains the added rows after the retail ones; the player script's map restores them;
    /// every function a tail wraps is listed for the link's check.
    #[test]
    fn the_tails_append_the_rows_and_install_the_map() {
        let (lua, t) = linked(false);
        assert_eq!(eval::<i64>(&lua, "return #_tOutfits.mattias"), 5);
        assert_eq!(eval::<String>(&lua, "return _tOutfits.mattias[4].Name .. ' ' .. _tOutfits.mattias[4].sQmShipment"), "Obama biua");
        assert!(eval::<bool>(&lua, "return _tOutfits.mattias[5].bAlwaysVisible and _tOutfits.mattias[5].sQmChatter == nil"));
        assert_eq!(eval::<i64>(&lua, "return #_tOutfits.jennifer"), 4);
        let (template, model): (String, String) = eval(&lua, "return GetTemplateAndModelName({ iIndex = 1, iCostume = 3 })");
        assert_eq!((template.as_str(), model.as_str()), ("mattias", "pmc_hum_obama"));
        assert_eq!(eval::<String>(&lua, "local _, m = GetTemplateAndModelName({ iIndex = 1, iCostume = 5 }) return m"), "mx");
        assert_eq!(eval::<String>(&lua, "local _, m = GetTemplateAndModelName({ iIndex = 3, iCostume = 3 }) return m"), "pmc_hum_obama");
        assert!(calls(&lua).is_empty(), "{:?}", calls(&lua));
        let targets = |tail: &crate::loader::Tail| tail.wraps.iter().map(|(t, _)| t.clone()).collect::<Vec<String>>();
        assert_eq!(targets(&t.wardrobe), ["SaveSingleton", "_SelectOutfit", "_CompleteChangeOutfit", "NetEventCallback"]);
        assert_eq!(targets(&t.player), ["SetCharacterMap", "OnPlayerJoined"]);
        assert_eq!(targets(&t.master), ["LoadSingleton"]);
    }

    /// vz: an added row is offered at any unlock count, and opens the menu where retail shows the
    /// tutorial dialog; retail rows keep the count rule.
    #[test]
    fn added_rows_are_always_offered() {
        let (lua, _) = linked(false);
        let offered = |hero: &str, count: i64, costume: i64| -> Vec<i64> {
            eval::<Table>(&lua, &format!("sHero = {hero:?} _nAvailableCostumes = {count} nCostume = {costume} _SelectOutfit(101) return tListed"))
                .sequence_values::<i64>()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(offered("mattias", 1, 0), [4, 5], "count 1: no retail row but the worn one, every added row");
        assert_eq!(offered("mattias", 3, 0), [3, 4, 5]);
        assert_eq!(offered("mattias", 3, 3), [1, 3, 5], "the worn added row is not offered again");
        assert_eq!(offered("chris", 1, 0), [3]);
        assert!(!calls(&lua).iter().any(|c| c.starts_with("DisplayDialogBox")), "{:?}", calls(&lua));
    }

    /// dlc01's own `_SelectOutfit` lists `bAlwaysVisible` rows, so the tail leaves it.
    #[test]
    fn a_wardrobe_that_lists_always_visible_rows_keeps_its_select_outfit() {
        let (lua, t) = linked(true);
        assert!(!t.wardrobe.wraps.iter().any(|(f, _)| f == "_SelectOutfit"));
        assert!(!t.wardrobe.source.contains("_SelectOutfit = function"));
        let listed: Vec<i64> = eval::<Table>(&lua, "_nAvailableCostumes = 1 _SelectOutfit(101) return tListed")
            .sequence_values::<i64>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(listed, [4, 5]);
    }

    /// The save names the worn row; the load puts the byte back where that row now is, at 0 when it is
    /// gone, and leaves a retail save's byte alone.
    #[test]
    fn the_save_remembers_which_outfit_and_the_load_finds_it() {
        let (lua, _) = linked(false);
        let saved: String = eval(
            &lua,
            "nCostume = 3 local t = SaveSingleton() return t.tQmOutfit.sHero .. ' ' .. t.tQmOutfit.sShipment .. ' ' .. t.tQmOutfit.sName .. ' ' .. tostring(t.bUnlocked)",
        );
        assert_eq!(saved, "mattias biua Obama true");
        assert_eq!(eval::<String>(&lua, "nCostume = 1 return SaveSingleton().tQmOutfit.sName"), "Suit");
        assert!(eval::<bool>(&lua, "nCostume = 1 return SaveSingleton().tQmOutfit.sShipment == nil"));
        let load = |saved: &str| -> i64 {
            eval(&lua, &format!("nCostume = 2 tLoaded = nil LoadSingleton({{ tPmcInteriorData = {saved} }}) assert(tLoaded ~= nil) return nCostume"))
        };
        assert_eq!(load("{ tQmOutfit = { sHero = 'mattias', sShipment = 'other', sName = 'Coat' } }"), 4);
        assert_eq!(load("{ tQmOutfit = { sHero = 'chris', sShipment = 'biua', sName = 'Obama' } }"), 2);
        assert_eq!(load("{ tQmOutfit = { sHero = 'mattias', sName = 'Chicken' } }"), 2);
        assert_eq!(load("{ tQmOutfit = { sHero = 'mattias', sShipment = 'gone', sName = 'Cape' } }"), 0, "gone: the hero's own model");
        assert_eq!(load("{}"), 2, "a retail save keeps its byte");
        assert!(calls(&lua).is_empty(), "{:?}", calls(&lua));
    }

    /// The join and a wardrobe change give a chatter row's set; a change away from it gives the hero's
    /// own back, and so does a co-op partner's change.
    #[test]
    fn a_chatter_row_sets_its_chatter_and_leaving_it_returns_the_heroes() {
        let (lua, _) = linked(false);
        eval::<()>(&lua, "nCostume = 3 OnPlayerJoined(0, 'p', { iIndex = 1 }, true, 0)");
        assert_eq!(calls(&lua), ["SetChatterSet 101 Obama_Obama01"]);
        assert_eq!(eval::<String>(&lua, "return tJoined[2]"), "pmc_hum_obama");
        eval::<()>(&lua, "tCalls = {} _CompleteChangeOutfit(101, 5)");
        assert_eq!(calls(&lua), ["SetChatterSet 101 Mattias_Peter01", "SetOutfit 101 coat"]);
        eval::<()>(&lua, "tCalls = {} _CompleteChangeOutfit(101, 2)");
        assert_eq!(calls(&lua), ["SetOutfit 101 m1"], "no qm set to return from");
        eval::<()>(&lua, "tCalls = {} _CompleteChangeOutfit(101, 4)");
        assert_eq!(calls(&lua), ["SetOutfit 101 pmc_hum_obama", "SetChatterSet 101 Obama_Obama01"]);
        eval::<()>(&lua, "tCalls = {} NetEventCallback(NETEVENT_NOTIFYOUTFITCHANGE, { 1, 1 })");
        assert_eq!(calls(&lua), ["SetChatterSet 101 Mattias_Peter01"]);
        eval::<()>(&lua, "tCalls = {} NetEventCallback(NETEVENT_NOTIFYOUTFITCHANGE, { 2, 4 })");
        assert_eq!(calls(&lua), ["SetChatterSet 102 Obama_Obama01"]);
    }

    /// A wardrobe whose lists are not the ones the tail was generated from fails, rather than putting
    /// the added rows where the map does not restore them.
    #[test]
    fn a_wardrobe_other_than_the_one_read_fails() {
        let r = retail();
        let t = tails(&generate("vz", &r, &[added("x", "Coat", "coat", &["mattias"])]).unwrap(), &r, None).unwrap().unwrap();
        // Mattias has a fourth row the read did not see.
        let other = mercs2_luac::compile(
            "_tOutfits = { mattias = { {}, {}, {}, {} }, chris = { {}, {} }, jennifer = { {}, {}, {} } }\n\
             function SaveSingleton() return {} end\nfunction _SelectOutfit() end\n",
            "wifpmcinterior",
        )
        .unwrap();
        let lua = run(&[(spliced(&other, &t.wardrobe, "wifpmcinterior"), "wifpmcinterior")]);
        assert_eq!(calls(&lua), ["RequestGameState Exiting"]);
        assert_eq!(eval::<i64>(&lua, "return #_tOutfits.mattias"), 4, "no row added");
    }

    /// An author's display string cannot leave its Lua literal: the tail compiles, and the text is the
    /// row's `PlayerVisibleName`, as data.
    #[test]
    fn a_hostile_display_string_stays_a_string() {
        const HOSTILE: &str = "evil\" ) end print(\"pwned\\";
        let mut o = added("x", "S", "m", &["mattias"]);
        o.display = HOSTILE.into();
        let r = retail();
        let t = tails(&generate("vz", &r, &[o]).unwrap(), &r, None).unwrap().unwrap();
        let lua = run(&[(spliced(&wardrobe(), &t.wardrobe, "wifpmcinterior"), "wifpmcinterior")]);
        assert_eq!(eval::<String>(&lua, "return _tOutfits.mattias[4].PlayerVisibleName"), HOSTILE);
    }

    #[test]
    fn the_chatter_set_catalogue_parses() {
        assert_eq!(chatter_sets().unwrap(), ["Mattias_Peter01", "Chris_Phil01", "Jen_Jen01", "Obama_Obama01", "Palin_Palin01"]);
    }

    #[test]
    fn bad_wearers_are_refused() {
        let r = retail();
        assert!(generate("vz", &r, &[added("x", "A", "a", &[])]).unwrap_err().contains("no wearer"));
        assert!(generate("vz", &r, &[added("x", "A", "a", &["eva"])]).unwrap_err().contains("\"eva\" is not a hero"));
        assert!(generate("vz", &r, &[added("x", "A", "a", &["jen", "jennifer"])]).unwrap_err().contains("names jennifer twice"));
    }
}
