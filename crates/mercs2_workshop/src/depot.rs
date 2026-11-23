//! The Depot: the modder's own items, Draft or Ready, kept in one workspace folder
//! (`docs/modernization/workshop_pages/depot.md`).
//!
//! An item is a folder holding `item.yaml`; its files live under its own `src/`. The contributions in
//! `item.yaml` are the manifest's own form ([`Contribution`]), so an item is checked by the same
//! `qm lint` a Shipment is, with the item folder as the root its `src/` paths resolve against.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use mercs2_quartermaster::lint::{Diagnostic, Severity};
use mercs2_quartermaster::manifest::{Contribution, Load, Manifest, Shipment, Target, FORMAT_VERSION};
use mercs2_quartermaster::names::NameTable;
use serde::{Deserialize, Serialize};

/// The file that makes a folder an item.
pub const ITEM_FILE: &str = "item.yaml";

/// Draft until the modder marks it Ready (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Draft,
    Ready,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Draft => "Draft",
            State::Ready => "Ready",
        }
    }
}

/// `item.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemFile {
    pub state: State,
    #[serde(default)]
    pub contributions: Vec<Contribution>,
}

/// One item found in the workspace.
#[derive(Debug, Clone)]
pub struct Item {
    /// The folder name: the item's name.
    pub name: String,
    /// The item folder.
    pub dir: PathBuf,
    /// The top-level workspace folder it sits in; empty for an item at the workspace root.
    pub folder: String,
    /// The parsed `item.yaml`, or why it did not parse.
    pub file: Result<ItemFile, String>,
    /// The `item.yaml` text [`Self::file`] was read from; `None` when it could not be read. A
    /// rewrite is refused once the file on disk is no longer this text ([`rewrite_item`]).
    pub text: Option<String>,
    /// The newest modification time of anything in the item folder.
    pub modified: Option<SystemTime>,
    pub checks: Checks,
}

impl Item {
    pub fn state(&self) -> Option<State> {
        self.file.as_ref().ok().map(|f| f.state)
    }

    /// The kinds this item writes, in order.
    pub fn kinds(&self) -> Vec<&'static str> {
        match &self.file {
            Ok(f) => f.contributions.iter().map(|c| c.kind()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// The result of checking one item.
#[derive(Debug, Clone, Default)]
pub struct Checks {
    /// `qm lint` over the item's contributions, rooted at the item folder.
    pub lint: Vec<Diagnostic>,
    /// One row per Lua file a `patch_lua`, `replace_lua` or `add_script` names: the path and the
    /// compile result. A file that does not exist is not compiled; lint reports it (M0110).
    pub lua: Vec<(PathBuf, Result<(), String>)>,
}

impl Checks {
    /// Lint errors and failed compiles. A Hang is an error too.
    pub fn errors(&self) -> usize {
        self.lint.iter().filter(|d| d.severity >= Severity::Error).count()
            + self.lua.iter().filter(|(_, r)| r.is_err()).count()
    }

    pub fn warnings(&self) -> usize {
        self.lint.iter().filter(|d| d.severity == Severity::Warning).count()
    }

    /// The finding a row names: the worst lint finding, else the first failed compile.
    pub fn worst(&self) -> Option<(String, String)> {
        let lint = self.lint.iter().max_by_key(|d| d.severity);
        match lint {
            Some(d) if d.severity >= Severity::Warning => {
                Some((d.rule.code.to_string(), d.rule.title.to_string()))
            }
            _ => self.lua.iter().find_map(|(_, r)| r.as_ref().err().map(|e| ("Lua compile".to_string(), e.clone()))),
        }
    }
}

/// Every item in a workspace, and what is wrong with the workspace as a whole.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    pub root: PathBuf,
    pub items: Vec<Item>,
    /// The top-level folders, with how many items each holds, by name.
    pub folders: Vec<(String, usize)>,
    /// Item names held by more than one folder, with every folder holding it. Each is an error: the
    /// items' `src/` files would collide in a Shipment that takes both.
    pub duplicates: Vec<(String, Vec<PathBuf>)>,
}

impl Workspace {
    pub fn count(&self, state: State) -> usize {
        self.items.iter().filter(|i| i.state() == Some(state)).count()
    }

    /// Items with a check error, an unparsable `item.yaml`, or a name another item also has.
    pub fn with_errors(&self) -> usize {
        self.items.iter().filter(|i| self.has_error(i)).count()
    }

    pub fn has_error(&self, item: &Item) -> bool {
        item.file.is_err() || item.checks.errors() > 0 || self.is_duplicate(&item.name)
    }

    pub fn is_duplicate(&self, name: &str) -> bool {
        self.duplicates.iter().any(|(n, _)| n == name)
    }
}

/// Is `root` a usable workspace? It must be an existing, readable folder. Reports what it holds.
pub fn check_root(root: &Path) -> Result<String, String> {
    let ws = scan(root, None)?;
    Ok(format!(
        "{} items \u{b7} {} draft \u{b7} {} ready",
        ws.items.len(),
        ws.count(State::Draft),
        ws.count(State::Ready)
    ))
}

/// Read the whole workspace and check every item.
pub fn scan(root: &Path, names: Option<&NameTable>) -> Result<Workspace, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    let mut dirs = Vec::new();
    find_items(root, &mut dirs)?;
    let mut items: Vec<Item> = dirs.into_iter().map(|d| load_item(root, &d, names)).collect();
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.dir.cmp(&b.dir)));

    let mut duplicates: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for i in &items {
        let same: Vec<PathBuf> = items.iter().filter(|j| j.name == i.name).map(|j| j.dir.clone()).collect();
        if same.len() > 1 && !duplicates.iter().any(|(n, _)| *n == i.name) {
            duplicates.push((i.name.clone(), same));
        }
    }

    let mut folders: Vec<(String, usize)> = Vec::new();
    for entry in read_dir_sorted(root)? {
        if entry.is_dir() && !is_item_dir(&entry) && !is_hidden(&entry) {
            let name = file_name(&entry);
            let n = items.iter().filter(|i| i.folder == name).count();
            folders.push((name, n));
        }
    }
    Ok(Workspace { root: root.to_path_buf(), items, folders, duplicates })
}

/// Read and check one item folder.
pub fn load_item(root: &Path, dir: &Path, names: Option<&NameTable>) -> Item {
    let name = file_name(dir);
    let folder = match dir.strip_prefix(root).ok().and_then(|r| r.components().next()) {
        Some(c) if dir.parent() != Some(root) => c.as_os_str().to_string_lossy().into_owned(),
        _ => String::new(),
    };
    let path = dir.join(ITEM_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()));
    let file = text.as_ref().map_err(String::clone).and_then(|t| parse_item(&path, t));
    let checks = match &file {
        Ok(f) => check(dir, f, names),
        Err(_) => Checks::default(),
    };
    Item { name, dir: dir.to_path_buf(), folder, file, text: text.ok(), modified: newest_mtime(dir), checks }
}

fn parse_item(path: &Path, text: &str) -> Result<ItemFile, String> {
    serde_norway::from_str(text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parse `<dir>/item.yaml`.
#[cfg(test)]
pub fn read_item(dir: &Path) -> Result<ItemFile, String> {
    let path = dir.join(ITEM_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_item(&path, &text)
}

/// Refused unless `item`'s `item.yaml` on disk is still the text it was read from: a write from
/// what was read would lose what changed since.
pub fn unchanged(item: &Item) -> Result<(), String> {
    let path = item.dir.join(ITEM_FILE);
    let now = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if item.text.as_deref() == Some(now.as_str()) {
        Ok(())
    } else {
        Err(format!("{} changed on disk since it was read; it is left as it is", path.display()))
    }
}

/// Write `f` over `item`'s `item.yaml`, once [`unchanged`] holds, rewriting only what changed.
pub fn rewrite_item(item: &Item, f: &ItemFile) -> Result<(), String> {
    unchanged(item)?;
    let path = item.dir.join(ITEM_FILE);
    let prior = item.text.as_deref().ok_or_else(|| format!("{}: it was not read", path.display()))?;
    write_over(&path, prior, f)
}

/// Make the item at `dir` a Draft, rewriting only its `state:`.
pub fn make_draft(dir: &Path) -> Result<(), String> {
    let path = dir.join(ITEM_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut f = parse_item(&path, &text)?;
    f.state = State::Draft;
    write_over(&path, &text, &f)
}

/// Write `f` over `path`, which holds `prior`: only what changed is rewritten, so the rest of the
/// author's text, comments included, stays ([`mercs2_quartermaster::lossless`]).
fn write_over(path: &Path, prior: &str, f: &ItemFile) -> Result<(), String> {
    let text = mercs2_quartermaster::lossless::rewrite(prior, mercs2_quartermaster::Format::Yaml, f, |t| parse_item(path, t), item_text)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn item_text(item: &ItemFile) -> Result<String, String> {
    serde_norway::to_string(item).map_err(|e| e.to_string())
}

/// Write `<dir>/item.yaml`.
pub fn write_item(dir: &Path, item: &ItemFile) -> Result<(), String> {
    let path = dir.join(ITEM_FILE);
    std::fs::write(&path, item_text(item)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// Set an item's state and write it ([`rewrite_item`]). Ready is refused while the item has a
/// check error.
pub fn set_state(item: &Item, state: State) -> Result<(), String> {
    let mut f = item.file.clone()?;
    if state == State::Ready && item.checks.errors() > 0 {
        return Err(format!(
            "{} has {} check error(s); fix them before marking it Ready",
            item.name,
            item.checks.errors()
        ));
    }
    f.state = state;
    rewrite_item(item, &f)
}

/// Make a new Draft item `<root>/<folder>/<name>` holding `contributions`, with an empty `src/`.
/// Refused when an item of that name exists anywhere in the workspace, or the folder exists.
pub fn create_item(
    ws: &Workspace,
    folder: &str,
    name: &str,
    contributions: Vec<Contribution>,
) -> Result<PathBuf, String> {
    valid_name(name)?;
    if ws.items.iter().any(|i| i.name == name) {
        return Err(format!("an item named {name} already exists"));
    }
    let parent = if folder.is_empty() { ws.root.clone() } else { ws.root.join(folder) };
    let dir = parent.join(name);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    std::fs::create_dir_all(dir.join("src")).map_err(|e| format!("{}: {e}", dir.display()))?;
    write_item(&dir, &ItemFile { state: State::Draft, contributions })?;
    Ok(dir)
}

/// An item name is one folder name: not empty, no path separators, not `.`/`..`, not hidden.
pub fn valid_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("an item needs a name".into());
    }
    if name != name.trim() {
        return Err(format!("\u{201c}{name}\u{201d} starts or ends with a space"));
    }
    if name.starts_with('.') {
        return Err(format!("\u{201c}{name}\u{201d} starts with a dot"));
    }
    if name.contains(['/', '\\', ':']) {
        return Err(format!("\u{201c}{name}\u{201d} holds a path separator"));
    }
    Ok(())
}

/// Lint the item's contributions as a manifest rooted at the item folder, and compile its Lua.
pub fn check(dir: &Path, item: &ItemFile, names: Option<&NameTable>) -> Checks {
    let manifest = item_manifest(item);
    let lint = mercs2_quartermaster::lint::lint(&manifest, Some(dir), names);
    let mut lua = Vec::new();
    for c in &item.contributions {
        let path = match c {
            Contribution::PatchLua { append, .. } => append,
            Contribution::ReplaceLua { source, .. } | Contribution::AddScript { source, .. } => source,
            _ => continue,
        };
        let file = dir.join(path);
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let chunk = format!("@{}", path.display());
        let result = mercs2_luac::compile(&text, &chunk).map(|_| ()).map_err(|e| e.to_string());
        lua.push((path.clone(), result));
    }
    Checks { lint, lua }
}

/// The manifest `qm lint` reads for an item: its contributions under a fixed, valid identity. The
/// identity is not the item's; a Shipment gives its own when the item is copied into it.
pub fn item_manifest(item: &ItemFile) -> Manifest {
    Manifest {
        format: FORMAT_VERSION,
        shipment: Shipment {
            name: "depot-item".into(),
            title: None,
            version: "0.0.0".into(),
            authors: Vec::new(),
            description: None,
            target: Target::Retail,
            quartermaster: None,
            license: None,
            homepage: None,
            tags: Vec::new(),
        },
        supersedes: Vec::new(),
        load: Load::default(),
        contributions: item.contributions.clone(),
    }
}

fn is_item_dir(dir: &Path) -> bool {
    dir.join(ITEM_FILE).is_file()
}

/// A dot-folder (`.git`, `.DS_Store`'s kin) is not the modder's content.
fn is_hidden(p: &Path) -> bool {
    file_name(p).starts_with('.')
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        out.push(e.map_err(|e| format!("{}: {e}", dir.display()))?.path());
    }
    out.sort();
    Ok(out)
}

/// Every item folder under `dir`. An item's own folders are part of it and are not searched.
fn find_items(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for p in read_dir_sorted(dir)? {
        if !p.is_dir() || is_hidden(&p) {
            continue;
        }
        if is_item_dir(&p) {
            out.push(p);
        } else {
            find_items(&p, out)?;
        }
    }
    Ok(())
}

fn newest_mtime(dir: &Path) -> Option<SystemTime> {
    let mut newest = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
    let Ok(rd) = std::fs::read_dir(dir) else { return newest };
    for e in rd.flatten() {
        let p = e.path();
        let t = if p.is_dir() { newest_mtime(&p) } else { e.metadata().and_then(|m| m.modified()).ok() };
        if t > newest {
            newest = t;
        }
    }
    newest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("m2ws_depot_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn patch_lua(target: &str, append: &str) -> Contribution {
        Contribution::PatchLua { target: target.into(), append: PathBuf::from(append) }
    }

    #[test]
    fn an_item_round_trips_through_item_yaml() {
        let root = temp("roundtrip");
        let ws = scan(&root, None).unwrap();
        let dir = create_item(&ws, "missions", "gurcon002_spawn_lists", vec![patch_lua("gurcon002", "src/a.lua")])
            .unwrap();
        assert_eq!(dir, root.join("missions").join("gurcon002_spawn_lists"));
        assert!(dir.join("src").is_dir());
        let f = read_item(&dir).unwrap();
        assert_eq!(f.state, State::Draft);
        assert_eq!(f.contributions, vec![patch_lua("gurcon002", "src/a.lua")]);
        let text = std::fs::read_to_string(dir.join(ITEM_FILE)).unwrap();
        assert!(text.contains("state: draft"), "{text}");
        assert!(text.contains("kind: patch_lua"), "{text}");
    }

    #[test]
    fn scan_finds_items_at_any_depth_and_counts_top_level_folders() {
        let root = temp("scan");
        let ws = scan(&root, None).unwrap();
        create_item(&ws, "missions", "a", vec![]).unwrap();
        create_item(&ws, "", "b", vec![]).unwrap();
        std::fs::create_dir_all(root.join("characters").join("deep")).unwrap();
        let ws = scan(&root, None).unwrap();
        create_item(&ws, "characters/deep", "c", vec![]).unwrap();
        std::fs::create_dir_all(root.join(".git").join("x")).unwrap();
        write_item(&root.join(".git").join("x"), &ItemFile { state: State::Draft, contributions: vec![] })
            .unwrap();

        let ws = scan(&root, None).unwrap();
        let names: Vec<(&str, &str)> = ws.items.iter().map(|i| (i.name.as_str(), i.folder.as_str())).collect();
        assert_eq!(names, vec![("a", "missions"), ("b", ""), ("c", "characters")]);
        assert_eq!(ws.folders, vec![("characters".to_string(), 1), ("missions".to_string(), 1)]);
    }

    #[test]
    fn an_unparsable_item_yaml_is_listed_with_its_error() {
        let root = temp("broken");
        let dir = root.join("x");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(ITEM_FILE), "state: maybe\n").unwrap();
        let ws = scan(&root, None).unwrap();
        assert_eq!(ws.items.len(), 1);
        let err = ws.items[0].file.as_ref().unwrap_err();
        assert!(err.contains("item.yaml"), "{err}");
        assert!(ws.has_error(&ws.items[0]));
    }

    #[test]
    fn two_items_with_one_name_are_an_error_naming_both() {
        let root = temp("dupes");
        for f in ["missions", "world"] {
            let d = root.join(f).join("same");
            std::fs::create_dir_all(&d).unwrap();
            write_item(&d, &ItemFile { state: State::Draft, contributions: vec![] }).unwrap();
        }
        let ws = scan(&root, None).unwrap();
        assert_eq!(ws.duplicates.len(), 1);
        assert_eq!(ws.duplicates[0].1.len(), 2);
        assert_eq!(ws.with_errors(), 2);
        assert!(create_item(&ws, "", "same", vec![]).is_err());
    }

    #[test]
    fn a_missing_source_is_m0110_and_blocks_ready() {
        let root = temp("m0110");
        let ws = scan(&root, None).unwrap();
        create_item(&ws, "", "lua", vec![patch_lua("gurcon002", "src/missing.lua")]).unwrap();
        let ws = scan(&root, None).unwrap();
        let item = &ws.items[0];
        assert!(item.checks.lint.iter().any(|d| d.rule.code == "M0110"), "{:?}", item.checks.lint);
        assert!(set_state(item, State::Ready).is_err());
        assert_eq!(read_item(&item.dir).unwrap().state, State::Draft);
    }

    #[test]
    fn a_clean_patch_lua_item_has_no_errors_and_can_be_marked_ready_and_back() {
        let root = temp("clean");
        let ws = scan(&root, None).unwrap();
        let dir = create_item(&ws, "", "lua", vec![patch_lua("gurcon002", "src/a.lua")]).unwrap();
        std::fs::write(dir.join("src/a.lua"), "local x = 1\n").unwrap();
        let ws = scan(&root, None).unwrap();
        let item = &ws.items[0];
        assert_eq!(item.checks.errors(), 0, "{:?} {:?}", item.checks.lint, item.checks.lua);
        assert_eq!(item.checks.lua.len(), 1);
        set_state(item, State::Ready).unwrap();
        let ws = scan(&root, None).unwrap();
        assert_eq!(ws.items[0].state(), Some(State::Ready));
        set_state(&ws.items[0], State::Draft).unwrap();
        assert_eq!(read_item(&dir).unwrap().state, State::Draft);
    }

    /// An `item.yaml` changed on disk after the item was read is not written over: the state change
    /// is refused and the hand edit stays.
    #[test]
    fn a_state_change_is_refused_once_the_file_changed_on_disk() {
        let root = temp("stale");
        let ws = scan(&root, None).unwrap();
        let dir = create_item(&ws, "", "lua", vec![patch_lua("gurcon002", "src/a.lua")]).unwrap();
        std::fs::write(dir.join("src/a.lua"), "local x = 1\n").unwrap();
        let ws = scan(&root, None).unwrap();
        let by_hand = format!("# by hand\n{}", std::fs::read_to_string(dir.join(ITEM_FILE)).unwrap());
        std::fs::write(dir.join(ITEM_FILE), &by_hand).unwrap();
        let e = set_state(&ws.items[0], State::Ready).unwrap_err();
        assert!(e.contains("changed on disk"), "{e}");
        assert_eq!(std::fs::read_to_string(dir.join(ITEM_FILE)).unwrap(), by_hand);
        // Read again, it changes.
        let ws = scan(&root, None).unwrap();
        set_state(&ws.items[0], State::Ready).unwrap();
        assert_eq!(read_item(&dir).unwrap().state, State::Ready);
    }

    /// A hand-written `item.yaml` keeps its comments, spacing and quoting: a state change, and a
    /// duplicate made Draft, rewrite the `state:` value and nothing else.
    #[test]
    fn a_state_change_rewrites_only_the_state_of_a_hand_written_item() {
        let root = temp("byhand");
        let dir = root.join("lua");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.lua"), "local x = 1\n").unwrap();
        let text = "# spawn lists for gurcon002\nstate: draft   # until it is played\n\ncontributions:\n  - kind: patch_lua\n    target: 'gurcon002'\n    append: src/a.lua   # the wrapper\n";
        std::fs::write(dir.join(ITEM_FILE), text).unwrap();
        let ws = scan(&root, None).unwrap();
        set_state(&ws.items[0], State::Ready).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(ITEM_FILE)).unwrap(), text.replace("state: draft", "state: ready"));
        make_draft(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(ITEM_FILE)).unwrap(), text);
    }

    #[test]
    fn a_lua_syntax_error_is_a_check_error() {
        let root = temp("luaerr");
        let ws = scan(&root, None).unwrap();
        let dir = create_item(&ws, "", "lua", vec![patch_lua("gurcon002", "src/a.lua")]).unwrap();
        std::fs::write(dir.join("src/a.lua"), "function (\n").unwrap();
        let ws = scan(&root, None).unwrap();
        let item = &ws.items[0];
        assert!(item.checks.lua[0].1.is_err());
        assert_eq!(item.checks.errors(), 1, "{:?}", item.checks.lint);
    }

    #[test]
    fn names_that_are_not_one_folder_are_refused() {
        for bad in ["", " a", "a/b", "a\\b", ".x", "c:d"] {
            assert!(valid_name(bad).is_err(), "{bad:?}");
        }
        assert!(valid_name("gurcon002_spawn_lists").is_ok());
    }

    #[test]
    fn a_workspace_that_is_not_a_folder_is_refused() {
        let root = temp("notdir");
        let f = root.join("file");
        std::fs::write(&f, "x").unwrap();
        assert!(check_root(&f).is_err());
        assert!(check_root(&root.join("absent")).is_err());
        assert_eq!(check_root(&root).unwrap(), "0 items \u{b7} 0 draft \u{b7} 0 ready");
    }
}
