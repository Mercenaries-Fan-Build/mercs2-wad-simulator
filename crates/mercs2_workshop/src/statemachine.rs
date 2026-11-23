//! The destruction-states editor's model of an `edit_state_machine` item
//! (`docs/modernization/workshop_pages/depot.md`, board `ShipmentDestruction`).
//!
//! The item's `states:` file is qm's own format (`mercs2_quartermaster::states`): read with its
//! `parse`, written with its `extract`. A state's Enter and Exit scripts are `u32` token streams;
//! the editor shows them as [`Line`]s, one per instruction, and every edit is checked to give back
//! exactly the tokens it shows ([`set_script`]). What `qm build` would say about the file needs the
//! machine the game ships, read with the call the build uses ([`shipped`]); [`findings`] runs the
//! build's own checks against it.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::orchestrator::{self as orch, HierNode, StateMachine};
use mercs2_quartermaster::names::NameTable;

/// One instruction of a command script. The grammar is `FUN_004D1B00`'s: `1 X` pushes `X`, `2 X`
/// runs command `X` on what was pushed, `3` ends the script. Any other token, or a `1`/`2` with no
/// operand after it, is kept as it is (`Raw`), so no script loses a token by being shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Push(u32),
    Call(u32),
    End,
    Raw(u32),
}

/// A token stream as lines. [`tokens`] gives the same stream back.
pub fn lines(tokens: &[u32]) -> Vec<Line> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            1 if i + 1 < tokens.len() => {
                out.push(Line::Push(tokens[i + 1]));
                i += 2;
            }
            2 if i + 1 < tokens.len() => {
                out.push(Line::Call(tokens[i + 1]));
                i += 2;
            }
            3 => {
                out.push(Line::End);
                i += 1;
            }
            v => {
                out.push(Line::Raw(v));
                i += 1;
            }
        }
    }
    out
}

pub fn tokens(lines: &[Line]) -> Vec<u32> {
    let mut out = Vec::new();
    for l in lines {
        match *l {
            Line::Push(v) => out.extend([1, v]),
            Line::Call(v) => out.extend([2, v]),
            Line::End => out.push(3),
            Line::Raw(v) => out.push(v),
        }
    }
    out
}

/// A token as the manifest spells one: a whole number, a bare `0xHHHHHHHH`, or a name to hash.
pub(crate) fn word(w: &str) -> u32 {
    if !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(n) = w.parse::<u32>() {
            return n;
        }
    }
    mercs2_quartermaster::manifest::bare_hash(w).unwrap_or_else(|| pandemic_hash_m2(w))
}

/// Read a line as typed: `push <token>`, `end`, `raw <token>`, or a command (`SHOW`, `SetState`,
/// a bare hash). A raw `1`, `2` or `3` is refused: it would read back as an instruction.
pub fn parse_line(text: &str) -> Result<Line, String> {
    let mut parts = text.split_whitespace();
    let Some(first) = parts.next() else { return Err("the line is empty".into()) };
    let rest: Vec<&str> = parts.collect();
    let one = |what: &str| -> Result<u32, String> {
        match rest.as_slice() {
            [w] => Ok(word(w)),
            [] => Err(format!("{what} needs a value: a name, a 0xHHHHHHHH hash or a number")),
            _ => Err(format!("{what} takes one value, not {}", rest.len())),
        }
    };
    match first.to_ascii_lowercase().as_str() {
        "push" => one("push").map(Line::Push),
        "end" if rest.is_empty() => Ok(Line::End),
        "end" => Err("end takes no value".into()),
        "raw" => match one("raw")? {
            v @ 1..=3 => Err(format!("raw {v} would read back as an instruction; write push, a command or end")),
            v => Ok(Line::Raw(v)),
        },
        _ if rest.is_empty() => Ok(Line::Call(word(first))),
        _ => Err(format!("a command takes no value on its line; push its arguments first (`push {}`)", rest[0])),
    }
}

/// The line as [`parse_line`] reads it back, with `name` spelling each token.
pub fn line_text(line: Line, name: impl Fn(u32) -> String) -> String {
    match line {
        Line::Push(v) => format!("push {}", name(v)),
        Line::Call(v) => name(v),
        Line::End => "end".into(),
        Line::Raw(v) => format!("raw {}", name(v)),
    }
}

/// Spell a token so [`word`] reads it back as the same value: its name when that re-hashes to it,
/// a small value as a number, otherwise a bare hash.
pub fn spell(v: u32, name: Option<&str>) -> String {
    match name {
        Some(n) if pandemic_hash_m2(n) == v && !n.contains(char::is_whitespace) => n.to_string(),
        _ if v < 256 => v.to_string(),
        _ => format!("0x{v:08X}"),
    }
}

/// What the game ships for a model: its state machine, its HIER nodes, the `_P000` container the
/// build edits, and how many LOD rungs the build carries with it.
pub struct Shipped {
    pub machine: StateMachine,
    pub hier: Vec<HierNode>,
    pub container: Vec<u8>,
    pub rungs: usize,
}

/// Read `target`'s machine from the game stack the way `qm build` does (`GameStack::model_chain`).
pub fn shipped(wads: &[PathBuf], target: &str) -> Result<Shipped, String> {
    let hash = mercs2_quartermaster::manifest::asset_hash(target);
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let chain = game
        .model_chain(hash)
        .map_err(|e| format!("{target}: {e}"))?
        .ok_or_else(|| format!("{target} (0x{hash:08X}) is not a model in the game stack"))?;
    let rungs = chain.rungs.len();
    let container = chain.rungs.into_iter().next().map(|r| r.container).ok_or_else(|| format!("{target}: empty chain"))?;
    let machine = orch::parse_state_machine(&container)
        .ok_or_else(|| format!("{target} carries no destruction state machine"))?;
    let hier = orch::parse_hier(&container);
    Ok(Shipped { machine, hier, container, rungs })
}

/// One thing `qm build` would say about the edited machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub error: bool,
    pub code: &'static str,
    pub message: String,
}

/// The build's checks on `edited` against what ships: M0193 for a state outside the vocabulary
/// and the model, the container rewrite `serialize_state_machine` does, and a run of the machine
/// into its default states (a `SetState` loop is an error there, as in the engine's 64-level cap).
pub fn findings(edited: &StateMachine, shipped: &Shipped) -> Vec<Finding> {
    let mut out = Vec::new();
    let base: BTreeSet<u32> = shipped.machine.nodes.iter().flat_map(|n| n.states.iter().map(|s| s.name_hash)).collect();
    for node in &edited.nodes {
        for st in &node.states {
            if !base.contains(&st.name_hash) && !orch::is_known_state(st.name_hash) {
                out.push(Finding {
                    error: false,
                    code: mercs2_quartermaster::lint::M0193_STATE_OFF_VOCABULARY.code,
                    message: format!(
                        "state 0x{:08X} of node 0x{:08X} is not a known destruction state and is not in the \
                         model: SetState never reaches it",
                        st.name_hash, node.name_hash
                    ),
                });
            }
        }
    }
    if let Err(e) = orch::serialize_state_machine(&shipped.container, edited) {
        out.push(Finding { error: true, code: "build", message: format!("the container rewrite refuses it: {e}") });
    }
    let chosen: Vec<usize> = edited.nodes.iter().map(orch::default_state_index).collect();
    if let Err(e) = orch::machine_run(edited, &shipped.hier, &chosen) {
        out.push(Finding { error: true, code: "run", message: format!("running the machine: {e}") });
    }
    out
}

/// Every string scalar of a YAML text, by the hash it names: the spellings an author used, so a
/// rewrite keeps them where the name table has none.
fn spellings(text: &str) -> HashMap<u32, String> {
    fn walk(v: &serde_norway::Value, out: &mut HashMap<u32, String>) {
        match v {
            serde_norway::Value::String(s) => {
                out.entry(pandemic_hash_m2(s)).or_insert_with(|| s.clone());
            }
            serde_norway::Value::Sequence(items) => items.iter().for_each(|i| walk(i, out)),
            serde_norway::Value::Mapping(m) => m.values().for_each(|i| walk(i, out)),
            serde_norway::Value::Tagged(t) => walk(&t.value, out),
            _ => {}
        }
    }
    let mut out = HashMap::new();
    if let Ok(v) = serde_norway::from_str::<serde_norway::Value>(text) {
        walk(&v, &mut out);
    }
    out
}

/// `machine` as the text of a `states:` file. A token reads as the name the file being replaced
/// (`prior`) spelt it with, else the name table's, else hex; `extract` keeps a name only when it
/// re-hashes to the token.
pub fn to_yaml(machine: &StateMachine, prior: &str, names: Option<&NameTable>) -> String {
    let own = spellings(prior);
    mercs2_quartermaster::states::extract(machine, |h| {
        own.get(&h).cloned().or_else(|| names.and_then(|n| n.reverse(h)).map(String::from))
    })
}

/// Read the `states:` file at `path`.
pub fn read(path: &Path) -> Result<(String, StateMachine), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sm = mercs2_quartermaster::states::parse(&text)?;
    Ok((text, sm))
}

/// Which script of a state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    Enter,
    Exit,
}

/// Replace the `script` of state `state` of node `node` with `lines`. Refused when the tokens would
/// read back as other lines: a raw `1` or `2` followed by a line takes that line's first token as
/// its operand.
pub fn set_script(sm: &mut StateMachine, node: usize, state: usize, script: Script, lines_: &[Line]) -> Result<(), String> {
    let t = tokens(lines_);
    if lines(&t) != lines_ {
        return Err("these lines do not read back as themselves: a raw 1 or 2 takes the next line as its value".into());
    }
    let st = sm
        .nodes
        .get_mut(node)
        .and_then(|n| n.states.get_mut(state))
        .ok_or_else(|| format!("the machine has no state {state} on node {node}"))?;
    match script {
        Script::Enter => st.enter = t,
        Script::Exit => st.exit = t,
    }
    Ok(())
}

/// What a token is in this machine, for the note beside a line: `self` for the node's own hash, a
/// state, another switch node, a HIER node (`@`), a damage message, or its name.
pub fn annotate(v: u32, node: u32, sm: &StateMachine, hier: &[HierNode], names: Option<&NameTable>) -> String {
    if v < 256 {
        return String::new();
    }
    let name = names.and_then(|n| n.reverse(v));
    if v == node {
        return "self".into();
    }
    if let Some(s) = orch::state_name(v) {
        return s.into();
    }
    if orch::is_known_state(v) {
        return "state".into();
    }
    if sm.nodes.iter().any(|n| n.name_hash == v) {
        return format!("node {}", name.unwrap_or("")).trim_end().into();
    }
    if hier.iter().any(|h| h.hash == v) {
        return format!("@{}", name.unwrap_or("HIER node"));
    }
    let (minor, terminal) = orch::damage_messages(sm);
    if terminal.contains(&v) {
        return format!("{} (death message)", name.unwrap_or("message")).trim_start().into();
    }
    if minor.contains(&v) {
        return format!("{} (damage message)", name.unwrap_or("message")).trim_start().into();
    }
    name.map(String::from).unwrap_or_default()
}

/// The states a node can be named: the global vocabulary's named members, then the model's own
/// states outside it, each once.
pub fn state_choices(shipped: &StateMachine) -> Vec<u32> {
    let mut out: Vec<u32> = orch::STATE_VOCABULARY.iter().filter(|(_, n)| !n.is_empty()).map(|(h, _)| *h).collect();
    for s in shipped.nodes.iter().flat_map(|n| n.states.iter()) {
        if !out.contains(&s.name_hash) {
            out.push(s.name_hash);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_reads_as_lines_and_back() {
        let t = vec![1, 0xAAAA_0000, 2, pandemic_hash_m2("SHOW"), 3];
        let l = lines(&t);
        assert_eq!(l, vec![Line::Push(0xAAAA_0000), Line::Call(pandemic_hash_m2("SHOW")), Line::End]);
        assert_eq!(tokens(&l), t);
    }

    #[test]
    fn stray_tokens_survive_as_raw_lines() {
        for t in [vec![7, 1], vec![2], vec![0x1234_5678, 3, 1, 9, 2]] {
            assert_eq!(tokens(&lines(&t)), t, "{t:?}");
        }
        assert_eq!(lines(&[2]), vec![Line::Raw(2)]);
    }

    #[test]
    fn typed_lines_read_as_the_manifest_reads_tokens() {
        assert_eq!(parse_line("push 0x6BCB7C64"), Ok(Line::Push(0x6BCB_7C64)));
        assert_eq!(parse_line("push DestroyedState"), Ok(Line::Push(pandemic_hash_m2("DestroyedState"))));
        assert_eq!(parse_line("push 5"), Ok(Line::Push(5)));
        assert_eq!(parse_line("SHOW"), Ok(Line::Call(pandemic_hash_m2("show"))));
        assert_eq!(parse_line("  end "), Ok(Line::End));
        assert_eq!(parse_line("raw 0x10"), Ok(Line::Raw(0x10)));
        assert!(parse_line("raw 2").is_err());
        assert!(parse_line("push").is_err());
        assert!(parse_line("SHOW 0x1").is_err());
        assert!(parse_line("").is_err());
    }

    #[test]
    fn a_line_spelt_for_editing_reads_back() {
        let name = |v: u32| spell(v, (v == pandemic_hash_m2("SHOW")).then_some("SHOW"));
        for l in [Line::Push(0xDEAD_BEEF), Line::Push(4), Line::Call(pandemic_hash_m2("SHOW")), Line::End, Line::Raw(0x99)] {
            assert_eq!(parse_line(&line_text(l, name)), Ok(l));
        }
        // A name that does not re-hash to the token is not used.
        assert_eq!(spell(0x1234_5678, Some("not_it")), "0x12345678");
    }

    #[test]
    fn an_edit_that_would_read_back_differently_is_refused() {
        let mut sm = StateMachine {
            switch_slots: vec![],
            nodes: vec![orch::SwitchNodeDef { name_hash: 9, states: vec![orch::StateDef::default()] }],
        };
        assert!(set_script(&mut sm, 0, 0, Script::Enter, &[Line::Raw(1), Line::End]).is_err());
        set_script(&mut sm, 0, 0, Script::Exit, &[Line::Push(0x1000), Line::End]).unwrap();
        assert_eq!(sm.nodes[0].states[0].exit, vec![1, 0x1000, 3]);
        assert!(set_script(&mut sm, 0, 1, Script::Exit, &[]).is_err());
    }

    /// A name the author typed that no table knows survives a rewrite of the file.
    #[test]
    fn a_rewrite_keeps_the_authors_names() {
        let yaml = "nodes:\n- name: my_node\n  states:\n  - name: PristineState\n    enter: [1, my_part, 2, SHOW, 3]\n";
        let sm = mercs2_quartermaster::states::parse(yaml).unwrap();
        let out = to_yaml(&sm, yaml, None);
        assert!(out.contains("my_node") && out.contains("my_part") && out.contains("SHOW"), "{out}");
        let back = mercs2_quartermaster::states::parse(&out).unwrap();
        assert_eq!(back.nodes[0].states[0].enter, sm.nodes[0].states[0].enter);
    }

    /// The destroyer's machine, read as the build reads it: the numbers the board's footer and node
    /// list show, and every script of every node as lines that give back its tokens.
    #[cfg(feature = "retail")]
    #[test]
    fn the_destroyer_machine_reads_as_the_build_reads_it() {
        let wad = PathBuf::from(crate::app::retail_vz_wad());
        let s = shipped(&[wad], "al_veh_boat_destroyer").expect("the destroyer has a machine");
        let m = &s.machine;
        let states: usize = m.nodes.iter().map(|n| n.states.len()).sum();
        let scripts = m.nodes.iter().flat_map(|n| n.states.iter()).map(|s| (!s.enter.is_empty()) as usize + (!s.exit.is_empty()) as usize).sum::<usize>();
        eprintln!(
            "destroyer machine: {} nodes, {states} states, {scripts} non-empty scripts, {} SWIT slots, {} HIER nodes, node 0 = 0x{:08X} with {} states",
            m.nodes.len(),
            m.switch_slots.len(),
            s.hier.len(),
            m.nodes[0].name_hash,
            m.nodes[0].states.len()
        );
        for n in &m.nodes {
            for st in &n.states {
                assert_eq!(tokens(&lines(&st.enter)), st.enter);
                assert_eq!(tokens(&lines(&st.exit)), st.exit);
            }
        }
        assert!(findings(m, &s).is_empty(), "{:?}", findings(m, &s));
        // Extracted, rewritten, and read back, it is the same machine.
        let yaml = to_yaml(m, "", None);
        let back = mercs2_quartermaster::states::parse(&yaml).unwrap();
        assert_eq!(back.nodes.len(), m.nodes.len());
        for (a, b) in back.nodes.iter().zip(&m.nodes) {
            assert_eq!(a.name_hash, b.name_hash);
            for (x, y) in a.states.iter().zip(&b.states) {
                assert_eq!((x.name_hash, &x.enter, &x.exit), (y.name_hash, &y.enter, &y.exit));
            }
        }
    }

    /// Every destruction machine in the retail archive, every script: lines give back the tokens.
    #[cfg(feature = "retail")]
    #[test]
    fn every_retail_script_reads_back_through_its_lines() {
        use mercs2_formats::ffcs::load_ffcs_archive;
        use mercs2_formats::sges::decompress_block;
        use mercs2_formats::types::TYPE_ID_MODEL;
        use mercs2_formats::ucfx::parse_block_entry_table;
        let wad = crate::app::retail_vz_wad();
        let mut file = std::fs::File::open(&wad).expect("open vz.wad");
        let size = file.metadata().expect("stat").len();
        let archive = load_ffcs_archive(&mut file, size).expect("read FFCS");
        let mut blocks: BTreeSet<u16> = BTreeSet::new();
        for a in archive.aset.iter().filter(|a| a.type_id == TYPE_ID_MODEL) {
            blocks.extend(a.lod_chain().into_iter().filter(|&b| b != 0xFFFF));
        }
        let (mut machines, mut scripts, mut raw) = (0usize, 0usize, 0usize);
        for bi in blocks {
            let dec = decompress_block(&mut file, &archive.indx, bi).unwrap_or_else(|e| panic!("block {bi}: {e}"));
            let (_n, entries) = parse_block_entry_table(&dec);
            let mut pos = 4 + entries.len() * 16;
            for e in &entries {
                let end = pos + e.chunk_size as usize;
                let container = &dec[pos..end];
                pos = end;
                let Some(sm) = orch::parse_state_machine(container) else { continue };
                machines += 1;
                for st in sm.nodes.iter().flat_map(|n| n.states.iter()) {
                    for t in [&st.enter, &st.exit] {
                        let l = lines(t);
                        assert_eq!(&tokens(&l), t, "block {bi} 0x{:08X}", e.name_hash);
                        scripts += 1;
                        raw += l.iter().filter(|x| matches!(x, Line::Raw(_))).count();
                    }
                }
            }
        }
        eprintln!("{machines} machines, {scripts} scripts read back through their lines; {raw} raw lines");
        assert!(machines > 1000, "expected ~1311 destructibles, read {machines}");
    }
}
