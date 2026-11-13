//! What the game's executables say about a script's functions, compiled in: which ones the engine calls
//! by name, and which names are engine bindings.
//!
//! * `data/engine_lua_calls.tsv` — every call each catalogued image's code makes to a Lua function by name
//!   (`exemap lua-calls`, `mercs2_exemap::luacalls`): the module's `pandemic_hash_m2` (or `(object)`: the
//!   function runs in the script of the world object the call passes; `(computed)`: the module is chosen
//!   at run time; `(unread)`: the decode could not read it, and the call counts for no module), and the
//!   name. A row whose name is `(computed)` or `(unread)` names no function.
//! * `data/pc-v10.luareg.tsv` — the Lua C-function registration tables of the pc-v10 image (`exemap
//!   luareg`, sha256 `886524f8…`): a binding is a `Namespace.name` row with a namespace.
//!
//! * `data/binding_args.tsv` — the arguments each binding reads, by position and type (`exemap
//!   binding-args` over x360-retail, whose bindings read their arguments through helpers with explicit
//!   stack positions and no SecuROM in the way).
//! * `data/binding_names.tsv` — names for those arguments, traced from what the code does with each
//!   value, each with its confidence, review status and evidence.
//!
//! The files are read on first use; a malformed row is a panic naming the file and line, since the data
//! is compiled in and its tests read it.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Where the engine runs a function it calls by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallModule {
    /// The module whose name hashes to this.
    Hash(u32),
    /// The script of the world object the call passes.
    Object,
    /// A module the call chooses at run time.
    Computed,
    /// A module the decode could not read: such a call names no function's module, so it counts for none.
    Unread,
}

/// One call to a Lua function by name, in one image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineCall {
    pub image: String,
    /// The Ghidra function the call is in.
    pub function: Option<u32>,
    /// The call instruction, when it was read from the bytes.
    pub site: Option<u32>,
    pub module: CallModule,
    /// The function's name; `None` when the call computes it or it could not be read.
    pub name: Option<String>,
}

pub struct EngineData {
    pub calls: Vec<EngineCall>,
    bindings: HashSet<(String, String)>,
    /// Lua path → the arguments it reads ([`ArgRead`]).
    args: HashMap<String, Vec<ArgRead>>,
    /// (Lua path, position) → its name.
    names: HashMap<(String, i64), ArgName>,
}

/// One argument a binding reads (`data/binding_args.tsv`, `exemap binding-args` over x360-retail).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgRead {
    /// 1-based; `None` when the code works it out (a loop, a position chosen at run time).
    pub position: Option<i64>,
    pub reads: String,
    /// Positions it reads: a vector3 takes 3. A callback with an optional data table counts 1.
    pub slots: i64,
    pub reader: u32,
    /// Another read takes the same position as something else: the binding shifted the stack between them.
    pub conflict: bool,
}

/// A name given to a binding's argument (`data/binding_names.tsv`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgName {
    pub name: String,
    /// `proven` or `inferred`.
    pub confidence: String,
    /// `proposed` or `accepted`.
    pub status: String,
    pub evidence: String,
}

const CALLS: &str = include_str!("../data/engine_lua_calls.tsv");
const LUAREG: &str = include_str!("../data/pc-v10.luareg.tsv");
const ARGS: &str = include_str!("../data/binding_args.tsv");
const NAMES: &str = include_str!("../data/binding_names.tsv");
const ARGS_HEADER: &str = "binding\tfunction\tposition\treads\tslots\treader\tvia\tconflict";
const NAMES_HEADER: &str = "binding\tposition\tname\tconfidence\tstatus\tevidence";

/// The lines after a file's `#` lines, which must open with `header`: (line number, fields).
fn rows<'t>(text: &'t str, header: &str, width: usize) -> Result<Vec<(usize, Vec<&'t str>)>, String> {
    let mut lines = text.lines().enumerate().skip_while(|(_, l)| l.starts_with('#'));
    match lines.next() {
        Some((_, h)) if h == header => {}
        other => return Err(format!("the first line after the # lines is {:?}, not {header:?}", other.map(|(_, l)| l))),
    }
    lines
        .map(|(i, l)| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() != width || f.iter().any(|x| x.is_empty()) {
                return Err(format!("line {}: {} fields, expected {width} non-empty", i + 1, f.len()));
            }
            Ok((i + 1, f))
        })
        .collect()
}

/// Parse a binding-args file.
pub fn parse_args(text: &str) -> Result<HashMap<String, Vec<ArgRead>>, String> {
    let mut out: HashMap<String, Vec<ArgRead>> = HashMap::new();
    for (n, f) in rows(text, ARGS_HEADER, 8)? {
        let at = |m: String| format!("line {n}: {m}");
        let entry = out.entry(f[0].to_string()).or_default();
        if f[2] == "-" {
            continue; // a binding that reads no argument
        }
        let position = match f[2] {
            "computed" => None,
            p => Some(p.parse().map_err(|_| at(format!("position {p:?}")))?),
        };
        let slots = match f[4] {
            "1..2" => 1,
            s => s.parse().map_err(|_| at(format!("slots {s:?}")))?,
        };
        let reader = hex(f[5]).ok_or_else(|| at(format!("reader {:?}", f[5])))?;
        let read = ArgRead { position, reads: f[3].to_string(), slots, reader, conflict: f[7] == "yes" };
        if !entry.contains(&read) {
            entry.push(read);
        }
    }
    Ok(out)
}

/// Parse a binding-names file.
pub fn parse_names(text: &str) -> Result<HashMap<(String, i64), ArgName>, String> {
    let mut out = HashMap::new();
    for (n, f) in rows(text, NAMES_HEADER, 6)? {
        let at = |m: String| format!("line {n}: {m}");
        let position: i64 = f[1].parse().ok().filter(|p| *p > 0).ok_or_else(|| at(format!("position {:?}", f[1])))?;
        if !matches!(f[3], "proven" | "inferred") {
            return Err(at(format!("confidence {:?} is not proven or inferred", f[3])));
        }
        if !matches!(f[4], "proposed" | "accepted") {
            return Err(at(format!("status {:?} is not proposed or accepted", f[4])));
        }
        let name = ArgName { name: f[2].into(), confidence: f[3].into(), status: f[4].into(), evidence: f[5].into() };
        if out.insert((f[0].to_string(), position), name).is_some() {
            return Err(at(format!("a second name for {} position {position}", f[0])));
        }
    }
    Ok(out)
}
const CALLS_HEADER: &str = "image_id\tfunction\tsite\tmodule\tname";
const LUAREG_HEADER: &str = "table\tnamespace\tindex\tname\tfunction";

fn hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

fn optional_hex(s: &str) -> Result<Option<u32>, String> {
    match s {
        "-" => Ok(None),
        _ => hex(s).map(Some).ok_or_else(|| format!("{s:?} is not - or a 0x address")),
    }
}

/// Parse the calls file.
pub fn parse_calls(text: &str) -> Result<Vec<EngineCall>, String> {
    let mut lines = text.lines().enumerate().skip_while(|(_, l)| l.starts_with('#'));
    match lines.next() {
        Some((_, h)) if h == CALLS_HEADER => {}
        other => return Err(format!("the first line after the # lines is {:?}, not {CALLS_HEADER:?}", other.map(|(_, l)| l))),
    }
    lines
        .map(|(i, l)| {
            let at = |m: String| format!("line {}: {m}", i + 1);
            let f: Vec<&str> = l.split('\t').collect();
            let [image, function, site, module, name] = f[..] else { return Err(at(format!("{} fields, expected 5", f.len()))) };
            if image.is_empty() || name.is_empty() {
                return Err(at("an empty field".into()));
            }
            let module = match module {
                "(object)" => CallModule::Object,
                "(computed)" => CallModule::Computed,
                "(unread)" => CallModule::Unread,
                h => CallModule::Hash(hex(h).ok_or_else(|| at(format!("module {h:?} is not a 0x hash or a (marker)")))?),
            };
            let name = match name {
                "(computed)" | "(unread)" => None,
                n if n.starts_with('(') => return Err(at(format!("unknown marker {n:?}"))),
                n => Some(n.to_string()),
            };
            Ok(EngineCall { image: image.to_string(), function: optional_hex(function).map_err(at)?, site: optional_hex(site).map_err(at)?, module, name })
        })
        .collect()
}

/// Parse a luareg export: the `(namespace, name)` of every row with a namespace.
pub fn parse_luareg(text: &str) -> Result<HashSet<(String, String)>, String> {
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, h)) if h == LUAREG_HEADER => {}
        other => return Err(format!("the first line is {:?}, not {LUAREG_HEADER:?}", other.map(|(_, l)| l))),
    }
    let mut out = HashSet::new();
    for (i, l) in lines {
        let f: Vec<&str> = l.split('\t').collect();
        let [_, namespace, _, name, _] = f[..] else { return Err(format!("line {}: {} fields, expected 5", i + 1, f.len())) };
        if !namespace.is_empty() {
            out.insert((namespace.to_string(), name.to_string()));
        }
    }
    Ok(out)
}

/// The compiled-in data, read once.
pub fn engine() -> &'static EngineData {
    static DATA: OnceLock<EngineData> = OnceLock::new();
    DATA.get_or_init(|| EngineData {
        calls: parse_calls(CALLS).unwrap_or_else(|e| panic!("data/engine_lua_calls.tsv {e}")),
        bindings: parse_luareg(LUAREG).unwrap_or_else(|e| panic!("data/pc-v10.luareg.tsv {e}")),
        args: parse_args(ARGS).unwrap_or_else(|e| panic!("data/binding_args.tsv {e}")),
        names: parse_names(NAMES).unwrap_or_else(|e| panic!("data/binding_names.tsv {e}")),
    })
}

impl EngineData {
    /// The calls that run `name` in the module named `module` (a file stem; the hash folds case), or in a
    /// world object's script, or in a module chosen at run time. A call whose module was not read is none.
    pub fn calls_of<'d>(&'d self, module: &str, name: &'d str) -> impl Iterator<Item = &'d EngineCall> + 'd {
        let hash = mercs2_formats::hash::pandemic_hash_m2(module);
        self.calls.iter().filter(move |c| {
            c.name.as_deref() == Some(name)
                && match c.module {
                    CallModule::Hash(h) => h == hash,
                    CallModule::Object | CallModule::Computed => true,
                    CallModule::Unread => false,
                }
        })
    }

    /// Whether `namespace.name` is an engine binding.
    pub fn is_binding(&self, namespace: &str, name: &str) -> bool {
        self.bindings.contains(&(namespace.to_string(), name.to_string()))
    }

    /// The type of the argument at `position` of binding `path`, from the reads that cover it (a vector
    /// read covers as many positions as it reads): `integer`, `vector3 2/3`. Reads the binding makes after
    /// shifting the stack, and computed positions, type nothing.
    fn arg_type(&self, path: &str, position: i64) -> Option<String> {
        let reads = self.args.get(path)?;
        let covering = |r: &&ArgRead| !r.conflict && r.position.is_some_and(|p| p <= position && position < p + r.slots);
        let pick = reads.iter().filter(covering).find(|r| r.reads != "any").or_else(|| reads.iter().find(covering))?;
        let p = pick.position.expect("covering reads have a position");
        Some(if pick.slots > 1 { format!("{} {}/{}", pick.reads, position - p + 1, pick.slots) } else { pick.reads.clone() })
    }

    /// The pin labels of binding `path`'s arguments, position 1 first: each one's name when it has one,
    /// else `arg N · type`, else `arg N`. `None` for a path that is not a binding the data covers.
    pub fn params(&self, path: &str) -> Option<Vec<String>> {
        let reads = self.args.get(path);
        let named = self.names.keys().filter(|(b, _)| b == path).map(|(_, p)| *p).max().unwrap_or(0);
        if reads.is_none() && named == 0 {
            return None;
        }
        let read = reads
            .into_iter()
            .flatten()
            .filter(|r| !r.conflict)
            .filter_map(|r| r.position.map(|p| p + r.slots - 1))
            .max()
            .unwrap_or(0);
        let width = read.max(named);
        Some(
            (1..=width)
                .map(|p| match (self.names.get(&(path.to_string(), p)), self.arg_type(path, p)) {
                    (Some(n), _) => n.name.clone(),
                    (None, Some(t)) => format!("arg {p} · {t}"),
                    (None, None) => format!("arg {p}"),
                })
                .collect(),
        )
    }

    /// What is known about argument `position` of binding `path`: its name's confidence, review status
    /// and evidence, and the type its reader reads.
    pub fn param_note(&self, path: &str, position: i64) -> Option<String> {
        let name = self.names.get(&(path.to_string(), position));
        let ty = self.arg_type(path, position);
        if name.is_none() && ty.is_none() {
            return None;
        }
        let mut lines = Vec::new();
        if let Some(n) = name {
            lines.push(format!("{} ({}, {}): {}", n.name, n.confidence, n.status, n.evidence));
        }
        if let Some(t) = ty {
            lines.push(format!("read as {t} by the binding (x360-retail)"));
        }
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compiled_in_data_reads_and_names_what_the_briefing_proves() {
        let e = engine();
        let images: HashSet<&str> = e.calls.iter().map(|c| c.image.as_str()).collect();
        assert_eq!(images.len(), 10, "every catalogued image");
        // pc-v10 0x6f5c95: ecx = "NetSafePlayCheapCinematic", eax = pandemic_hash_m2("MrxBriefing")
        let pc: Vec<&EngineCall> = e.calls_of("mrxbriefing", "NetSafePlayCheapCinematic").filter(|c| c.image == "pc-v10").collect();
        assert_eq!(pc.len(), 1);
        assert_eq!((pc[0].site, pc[0].module), (Some(0x006f_5c95), CallModule::Hash(0x51ee_8f14)));
        assert_eq!(e.calls_of("mrxbriefing", "_JobRequest").count(), 0, "a callback the engine does not name");
        // NetEventCallback's module comes from the net event
        assert!(e.calls_of("mrxbriefing", "NetEventCallback").all(|c| c.module == CallModule::Computed));
        assert!(e.calls_of("mine", "OnActivate").any(|c| c.module == CallModule::Object));
        assert!(e.is_binding("Event", "Create") && e.is_binding("Event", "CreatePersistent"));
        assert!(!e.is_binding("MrxGui", "DisplayDialogBox"));
    }

    #[test]
    fn a_binding_s_pins_take_their_names_else_the_type_its_reader_reads() {
        let e = engine();
        let p = |path: &str| e.params(path).unwrap_or_else(|| panic!("{path}"));
        // SetColorValue("uiAmbientColor", 128, 128, 128, 255): a named key, four integers not yet named
        assert_eq!(p("Graphics.Atmosphere.SetColorValue"), ["sValueName", "arg 2 · integer", "arg 3 · integer", "arg 4 · integer", "arg 5 · integer"]);
        assert_eq!(p("Graphics.Atmosphere.End"), ["nInterpolateTime"]);
        // a vector3 read names its three positions
        assert_eq!(p("Graphics.Atmosphere.SetAmbientColor"), ["nRed", "nGreen", "nBlue"]);
        assert_eq!(p("Graphics.Atmosphere.SetLightModifier"), ["arg 1 · vector3 1/3", "arg 2 · vector3 2/3", "arg 3 · vector3 3/3"]);
        assert_eq!(p("Graphics.Atmosphere.SetSky"), ["sSky"], "named from its call sites; no executable reads it");
        assert!(e.params("Graphics.Atmosphere.NoSuchBinding").is_none());
        let note = e.param_note("Graphics.Atmosphere.End", 1).expect("a note");
        assert!(note.contains("fInterpolateTime") && note.contains("proven, proposed") && note.contains("read as number"), "{note}");
    }

    #[test]
    fn the_names_file_refuses_an_unknown_confidence_or_status_and_a_second_name() {
        let h = NAMES_HEADER;
        assert!(parse_names(&format!("{h}\nA.B\t1\tn\tlikely\tproposed\te\n")).unwrap_err().contains("confidence"));
        assert!(parse_names(&format!("{h}\nA.B\t1\tn\tproven\tdone\te\n")).unwrap_err().contains("status"));
        assert!(parse_names(&format!("{h}\nA.B\t1\tn\tproven\tproposed\te\nA.B\t1\tm\tproven\tproposed\te\n")).unwrap_err().contains("second name"));
        assert!(parse_names(&format!("{h}\nA.B\t0\tn\tproven\tproposed\te\n")).unwrap_err().contains("position"));
    }

    #[test]
    fn a_malformed_row_is_an_error_naming_its_line() {
        let e = parse_calls(&format!("# x\n{CALLS_HEADER}\npc-v10\t-\t0x1\tnot-hex\tF\n")).unwrap_err();
        assert!(e.contains("line 3") && e.contains("not a 0x hash"), "{e}");
        let e = parse_calls(&format!("{CALLS_HEADER}\npc-v10\t-\t0x1\n")).unwrap_err();
        assert!(e.contains("3 fields"), "{e}");
        assert!(parse_calls("image_id\tsite\n").is_err());
        let e = parse_luareg("table\tnamespace\n").unwrap_err();
        assert!(e.contains("first line"), "{e}");
    }
}
