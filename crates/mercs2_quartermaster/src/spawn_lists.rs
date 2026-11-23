//! `set_spawn_lists`: the spawn lists a shipped mission script sets, and the Lua that sets them again.
//!
//! The shipped Lua names a spawn list in two calls: `Ai.TweakAttachedSpawnersInGroup(guid, group,
//! {… SpawnList = "…"})` for one group of a building's spawners and `Ai.TweakAttachedSpawners(guid,
//! {… SpawnList = "…"})` for all of them. A `set_spawn_lists` row is written as Lua that wraps the
//! top-level function holding the call ([`wrapper`]): the wrapper runs the shipped body, then sets
//! the new list on the same building and group, and is linked onto the script as a `patch_lua`
//! append is. The wrapper reaches the building by its name, so only a call whose building is a
//! plain name that the function never declares as a parameter or local can be set ([`calls`]).

use full_moon::ast::{
    Call, Expression, Field, FunctionArgs, FunctionBody, FunctionCall, GenericFor, Index, LocalAssignment,
    LocalFunction, NumericFor, Parameter, Prefix, Suffix,
};
use full_moon::node::Node;
use full_moon::tokenizer::TokenType;
use full_moon::visitors::Visitor;

use crate::lua_source::{FnDef, Script};
use crate::manifest::SpawnListSet;

/// The two engine calls that set a spawn list.
pub const IN_GROUP: &str = "TweakAttachedSpawnersInGroup";
pub const ALL_GROUPS: &str = "TweakAttachedSpawners";

/// One call in a script that sets a literal spawn list.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnCall {
    /// The top-level function holding the call, as written (`SetupDangerousObjBuildings`).
    pub function: String,
    /// The building argument, as written (`TallCommBuild`).
    pub building: String,
    /// The group for `TweakAttachedSpawnersInGroup`; `None` for `TweakAttachedSpawners` (all groups).
    pub group: Option<String>,
    /// The list the script sets.
    pub list: String,
    /// 1-based line of the `SpawnList` value.
    pub line: u32,
    /// Whether a wrapper can set this call's list again, or why not.
    pub editable: Result<(), String>,
}

impl SpawnCall {
    pub fn key(&self) -> (&str, &str, Option<&str>) {
        (&self.function, &self.building, self.group.as_deref())
    }
}

fn row_key(s: &SpawnListSet) -> (&str, &str, Option<&str>) {
    (&s.function, &s.building, s.group.as_deref())
}

/// Every spawn-list call in `src` that sets a literal list, in source order.
pub fn calls(src: &str) -> Result<Vec<SpawnCall>, String> {
    let script = Script::parse(src)?;
    let fns = script.functions();

    struct Find(Vec<(usize, String, Option<String>, String, u32)>);
    impl Visitor for Find {
        fn visit_function_call(&mut self, c: &FunctionCall) {
            if let Some(found) = spawn_call(c) {
                self.0.push(found);
            }
        }
    }
    let mut find = Find(Vec::new());
    find.visit_ast(script.ast());

    let mut out = Vec::new();
    for (at, building, group, list, line) in find.0 {
        let Some(f) = fns.iter().find(|f| f.span.contains(&at)) else { continue };
        let editable = reachable(&building, f, src);
        out.push(SpawnCall { function: f.name.clone(), building, group, list, line, editable });
    }
    Ok(out)
}

/// `(byte offset, building, group, list, line)` when `c` is a spawn-list call with a literal list.
fn spawn_call(c: &FunctionCall) -> Option<(usize, String, Option<String>, String, u32)> {
    let Prefix::Name(ai) = c.prefix() else { return None };
    if ai.token().to_string() != "Ai" {
        return None;
    }
    let mut s = c.suffixes();
    let (Some(Suffix::Index(Index::Dot { name, .. })), Some(Suffix::Call(Call::AnonymousCall(args))), None) =
        (s.next(), s.next(), s.next())
    else {
        return None;
    };
    let method = name.token().to_string();
    let FunctionArgs::Parentheses { arguments, .. } = &**args else { return None };
    let a: Vec<&Expression> = arguments.iter().collect();
    let (building, group, table) = match (method.as_str(), a.as_slice()) {
        (IN_GROUP, [b, Expression::String(g), Expression::TableConstructor(t)]) => {
            (b, Some(string_literal(g.token().token_type())?), t)
        }
        (ALL_GROUPS, [b, Expression::TableConstructor(t)]) => (b, None, t),
        _ => return None,
    };
    let (list, line) = table.fields().iter().find_map(|f| match f {
        Field::NameKey { key, value, .. } if key.token().to_string() == "SpawnList" => match &**value {
            Expression::String(v) => {
                Some((string_literal(v.token().token_type())?, v.token().start_position().line() as u32))
            }
            _ => None,
        },
        _ => None,
    })?;
    let at = c.start_position()?.bytes();
    Some((at, building.to_string().trim().to_string(), group, list, line))
}

fn string_literal(t: &TokenType) -> Option<String> {
    match t {
        TokenType::StringLiteral { literal, .. } => Some(literal.to_string()),
        _ => None,
    }
}

fn is_name(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Can a wrapper of the function `f` reach `building` after the shipped body ran? Only when the
/// name is a global: not a parameter of `f`, and nothing in `f` declares it (a `local`, a `local
/// function`, a `for` variable, or a parameter of a function inside `f`).
fn reachable(building: &str, f: &FnDef, src: &str) -> Result<(), String> {
    if !is_name(building) {
        return Err(format!("{building} is not a plain name a wrapper can read"));
    }
    if f.params.iter().any(|p| p == building) {
        return Err(format!("{building} is a parameter of {}; a wrapper cannot read it", f.name));
    }
    let def = Script::parse(&src[f.span.clone()])?;
    struct Locals(Vec<String>);
    impl Visitor for Locals {
        fn visit_local_assignment(&mut self, a: &LocalAssignment) {
            self.0.extend(a.names().iter().map(|n| n.token().to_string()));
        }
        fn visit_local_function(&mut self, l: &LocalFunction) {
            self.0.push(l.name().token().to_string());
        }
        fn visit_numeric_for(&mut self, n: &NumericFor) {
            self.0.push(n.index_variable().token().to_string());
        }
        fn visit_generic_for(&mut self, g: &GenericFor) {
            self.0.extend(g.names().iter().map(|n| n.token().to_string()));
        }
        fn visit_function_body(&mut self, b: &FunctionBody) {
            self.0.extend(b.parameters().iter().filter_map(|p| match p {
                Parameter::Name(t) => Some(t.token().to_string()),
                _ => None,
            }));
        }
    }
    let mut locals = Locals(Vec::new());
    locals.visit_ast(def.ast());
    if locals.0.iter().any(|n| n == building) {
        return Err(format!("{building} is a local of {}; a wrapper cannot read it", f.name));
    }
    Ok(())
}

/// The local the wrapper keeps the shipped function in.
fn saved_name(function: &str) -> String {
    format!("_{}", function.replace(['.', ':'], "_"))
}

/// The Lua that sets each row: one wrapper per function, in the order the rows name them. Empty for
/// no rows.
pub fn wrapper(sets: &[SpawnListSet]) -> String {
    let mut functions: Vec<&str> = Vec::new();
    for c in sets {
        if !functions.contains(&c.function.as_str()) {
            functions.push(&c.function);
        }
    }
    let mut out = String::new();
    for f in functions {
        let saved = saved_name(f);
        let shipped = f.replacen(':', ".", 1);
        let call = if f.contains(':') { format!("{saved}(self, ...)") } else { format!("{saved}(...)") };
        out.push_str(&format!("local {saved} = {shipped}\nfunction {f}(...)\n  {call}\n"));
        for c in sets.iter().filter(|c| c.function == f) {
            match &c.group {
                Some(g) => out.push_str(&format!(
                    "  Ai.{IN_GROUP}({}, {}, {{\n    SpawnList = {}\n  }})\n",
                    c.building,
                    lua_string(g),
                    lua_string(&c.list)
                )),
                None => out.push_str(&format!(
                    "  Ai.{ALL_GROUPS}({}, {{\n    SpawnList = {}\n  }})\n",
                    c.building,
                    lua_string(&c.list)
                )),
            }
        }
        out.push_str("end\n");
    }
    out
}

/// `s` as a Lua string literal.
fn lua_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `sets` with the row for `call` set to `list`, or removed when `list` is the one the script ships.
pub fn set(sets: &[SpawnListSet], call: &SpawnCall, list: &str) -> Vec<SpawnListSet> {
    let mut out: Vec<SpawnListSet> = sets.to_vec();
    let at = out.iter().position(|s| row_key(s) == call.key());
    match (at, list == call.list) {
        (Some(i), true) => {
            out.remove(i);
        }
        (Some(i), false) => out[i].list = list.to_string(),
        (None, true) => {}
        (None, false) => out.push(SpawnListSet {
            function: call.function.clone(),
            building: call.building.clone(),
            group: call.group.clone(),
            list: list.to_string(),
        }),
    }
    out
}

/// Every literal spawn list the shipped Lua sets, sorted: the lists a row may name.
pub fn list_names<'a>(sources: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for src in sources {
        if !src.contains("SpawnList") {
            continue;
        }
        let Ok(found) = calls(src) else { continue };
        for c in found {
            if !names.contains(&c.list) {
                names.push(c.list);
            }
        }
    }
    names.sort();
    names
}

/// [`list_names`] over every `.lua` under each of `dirs` of `corpus_root`.
pub fn corpus_list_names(corpus_root: &std::path::Path, dirs: &[&str]) -> Result<Vec<String>, String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for e in entries {
            let p = e.map_err(|e| format!("{}: {e}", dir.display()))?.path();
            if p.is_dir() {
                walk(&p, out)?;
            } else if p.extension().is_some_and(|x| x == "lua") {
                out.push(std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?);
            }
        }
        Ok(())
    }
    let mut sources = Vec::new();
    for d in dirs {
        let dir = corpus_root.join(d);
        if dir.is_dir() {
            walk(&dir, &mut sources)?;
        }
    }
    if sources.is_empty() {
        return Err(format!("no shipped script under {} ({})", corpus_root.display(), dirs.join(", ")));
    }
    Ok(list_names(sources.iter().map(String::as_str)))
}

/// What is wrong with the rows on their own, each as `lists[i]: …`.
pub fn row_problems(sets: &[SpawnListSet]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, s) in sets.iter().enumerate() {
        let function_ok = !s.function.is_empty()
            && s.function.split(['.', ':']).all(is_name)
            && s.function.matches(':').count() <= 1
            && s.function.find(':').is_none_or(|c| !s.function[c..].contains('.'));
        if !function_ok {
            out.push(format!("lists[{i}]: function {:?} is not a function name the script can define", s.function));
        }
        if !is_name(&s.building) {
            out.push(format!("lists[{i}]: building {:?} is not a plain name a wrapper can read", s.building));
        }
        if s.group.as_deref().is_some_and(str::is_empty) {
            out.push(format!("lists[{i}]: group is empty; leave it out to set every group"));
        }
        if s.list.is_empty() {
            out.push(format!("lists[{i}]: list is empty"));
        }
        if let Some(j) = sets[..i].iter().position(|t| row_key(t) == row_key(s)) {
            out.push(format!("lists[{i}]: the same call as lists[{j}]"));
        }
    }
    out
}

/// What is wrong with the rows against the shipped script `source` (`path` names it) and the lists
/// the shipped Lua sets anywhere (`names`), each as `lists[i]: …`.
pub fn source_problems(sets: &[SpawnListSet], path: &str, source: &str, names: &[String]) -> Vec<String> {
    let found = match calls(source) {
        Ok(c) => c,
        Err(e) => return vec![format!("{path} does not parse: {e}")],
    };
    let mut out = Vec::new();
    for (i, s) in sets.iter().enumerate() {
        let where_ = match &s.group {
            Some(g) => format!("{IN_GROUP}({}, \"{g}\", …)", s.building),
            None => format!("{ALL_GROUPS}({}, …)", s.building),
        };
        match found.iter().find(|c| c.key() == row_key(s)) {
            None => out.push(format!("lists[{i}]: {path} has no {where_} call that sets a list in {}", s.function)),
            Some(c) => {
                if let Err(e) = &c.editable {
                    out.push(format!("lists[{i}]: {e}"));
                }
            }
        }
        if !names.contains(&s.list) {
            out.push(format!(
                "lists[{i}]: no shipped script sets the list {:?}; the lists the game's Lua sets are {}",
                s.list,
                names.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two call shapes the shipped Lua uses, a reachable building and three that are not.
    const SCRIPT: &str = r#"
function SetupBuildings()
  local SpawnDelay = 20
  Tower = Pg.GetGuidByName("Tower")
  Ai.TweakAttachedSpawnersInGroup(Tower, "Rooftop", {
    SpawnList = "Spawnlist (A)"
  })
  Ai.TweakAttachedSpawners(Tower, {SpawnerState = "on", SpawnList = "Spawnlist (B)"})
  Ai.TweakAttachedSpawnersInGroup(Tower, "Ground", {SpawnerState = "off"})
end

function ActivateBuilding(self, sName)
  local uGuid = Pg.GetGuidByName(sName)
  Ai.TweakAttachedSpawners(uGuid, {SpawnList = "Spawnlist (C)"})
  Ai.TweakAttachedSpawners(sName, {SpawnList = "Spawnlist (C)"})
end

function EachBuilding(tAll)
  for _, uBuilding in ipairs(tAll) do
    Ai.TweakAttachedSpawners(uBuilding, {SpawnList = "Spawnlist (C)"})
  end
end
"#;

    fn row(function: &str, building: &str, group: Option<&str>, list: &str) -> SpawnListSet {
        SpawnListSet { function: function.into(), building: building.into(), group: group.map(Into::into), list: list.into() }
    }

    #[test]
    fn calls_finds_both_shapes_with_their_function_group_list_and_line() {
        let c = calls(SCRIPT).unwrap();
        assert_eq!(c.len(), 5, "{c:#?}");
        assert_eq!(c[0].key(), ("SetupBuildings", "Tower", Some("Rooftop")));
        assert_eq!((c[0].list.as_str(), c[0].line), ("Spawnlist (A)", 6));
        assert_eq!(c[1].key(), ("SetupBuildings", "Tower", None));
        assert_eq!(c[1].list, "Spawnlist (B)");
        assert!(c[0].editable.is_ok() && c[1].editable.is_ok());
        assert!(c[2].editable.as_ref().unwrap_err().contains("local"));
        assert!(c[3].editable.as_ref().unwrap_err().contains("parameter"));
    }

    /// The line scan this replaced read `for _, uBuilding in …` as no declaration, so a wrapper
    /// would have read a global `uBuilding` that is nil.
    #[test]
    fn a_for_loop_variable_is_a_local_a_wrapper_cannot_read() {
        let c = calls(SCRIPT).unwrap();
        assert_eq!(c[4].key(), ("EachBuilding", "uBuilding", None));
        assert_eq!(c[4].editable, Err("uBuilding is a local of EachBuilding; a wrapper cannot read it".into()));
    }

    #[test]
    fn the_wrapper_sets_each_row_after_the_shipped_body_and_compiles() {
        let c = calls(SCRIPT).unwrap();
        let sets = set(&set(&[], &c[0], "Spawnlist (Elite)"), &c[1], "Spawnlist (A)");
        let text = wrapper(&sets);
        assert_eq!(
            text,
            "local _SetupBuildings = SetupBuildings\nfunction SetupBuildings(...)\n  _SetupBuildings(...)\n  Ai.TweakAttachedSpawnersInGroup(Tower, \"Rooftop\", {\n    SpawnList = \"Spawnlist (Elite)\"\n  })\n  Ai.TweakAttachedSpawners(Tower, {\n    SpawnList = \"Spawnlist (A)\"\n  })\nend\n"
        );
        mercs2_luac::compile(&text, "@wrapper").expect("the wrapper is Lua 5.1");
    }

    #[test]
    fn setting_a_call_back_to_its_shipped_list_removes_the_row() {
        let c = calls(SCRIPT).unwrap();
        let sets = set(&[], &c[0], "Spawnlist (Elite)");
        assert_eq!(sets, [row("SetupBuildings", "Tower", Some("Rooftop"), "Spawnlist (Elite)")]);
        assert_eq!(set(&sets, &c[0], "Spawnlist (B)")[0].list, "Spawnlist (B)");
        assert!(set(&sets, &c[0], "Spawnlist (A)").is_empty());
        assert_eq!(wrapper(&[]), "");
    }

    #[test]
    fn a_method_wrapper_passes_self() {
        let text = wrapper(&[row("Mod:Start", "B", None, "L")]);
        assert!(text.starts_with("local _Mod_Start = Mod.Start\nfunction Mod:Start(...)\n  _Mod_Start(self, ...)\n"), "{text}");
        mercs2_luac::compile(&text, "@wrapper").expect("the wrapper is Lua 5.1");
    }

    #[test]
    fn list_names_are_every_literal_list_once_sorted() {
        assert_eq!(list_names([SCRIPT, "x = 1"].into_iter()), ["Spawnlist (A)", "Spawnlist (B)", "Spawnlist (C)"]);
    }

    #[test]
    fn row_problems_name_a_bad_name_an_empty_part_and_a_repeated_call() {
        let rows = [
            row("SetupBuildings", "Tower", Some("Rooftop"), "Spawnlist (A)"),
            row("Setup Buildings", "t.x", Some(""), ""),
            row("SetupBuildings", "Tower", Some("Rooftop"), "Spawnlist (B)"),
            row("Mod:Start", "Tower", None, "Spawnlist (B)"),
        ];
        assert_eq!(
            row_problems(&rows),
            [
                "lists[1]: function \"Setup Buildings\" is not a function name the script can define",
                "lists[1]: building \"t.x\" is not a plain name a wrapper can read",
                "lists[1]: group is empty; leave it out to set every group",
                "lists[1]: list is empty",
                "lists[2]: the same call as lists[0]",
            ]
        );
    }

    #[test]
    fn source_problems_name_a_missing_call_an_unreachable_building_and_an_unknown_list() {
        let names = list_names([SCRIPT].into_iter());
        let rows = [
            row("SetupBuildings", "Tower", Some("Rooftop"), "Spawnlist (B)"),
            row("SetupBuildings", "Tower", Some("Balcony"), "Spawnlist (B)"),
            row("ActivateBuilding", "sName", None, "Spawnlist (B)"),
            row("SetupBuildings", "Tower", None, "Spawnlist (Z)"),
        ];
        assert_eq!(
            source_problems(&rows, "lua/vz/x.lua", SCRIPT, &names),
            [
                "lists[1]: lua/vz/x.lua has no TweakAttachedSpawnersInGroup(Tower, \"Balcony\", …) call that sets a list in SetupBuildings",
                "lists[2]: sName is a parameter of ActivateBuilding; a wrapper cannot read it",
                "lists[3]: no shipped script sets the list \"Spawnlist (Z)\"; the lists the game's Lua sets are \"Spawnlist (A)\", \"Spawnlist (B)\", \"Spawnlist (C)\"",
            ]
        );
    }
}
