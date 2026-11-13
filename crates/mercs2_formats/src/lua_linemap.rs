//! Retail Lua positions translated into the decompiled corpus's lines, instruction for instruction.
//!
//! The shipped chunks keep the original author's line numbers; the decompiled corpus (`unluac`'s
//! text, which the workshop's Blueprint lifts and our script host runs) renumbers them, so
//! `mrxmissionflow:201` in a retail trace is `mrxmissionflow.lua:132` in the corpus. Compiling the
//! corpus text gives a chunk whose code, constants and nested functions equal retail's and whose line
//! info alone differs (`mercs2_luac/tests/parity_retail.rs`). Where that holds, instruction *i* of a
//! function is the same instruction in both chunks, and pairing the two `line_info` arrays by index is
//! an exact translation: nothing is matched by text or guessed.
//!
//! [`LineMap::build`] walks retail's function tree and ours together. A function that differs in
//! anything but line info is kept as unmapped, with the first difference as its reason: a position in
//! it is an error carrying that reason, and the rest of the module still maps. A tree whose functions
//! cannot be paired at all (a different number of nested functions) refuses the module.
//!
//! [`pair_modules`] pairs retail's scripts with the corpus's files: by name, then, for a file whose
//! name no script has (the corpus names a few by their content: `xQ!L.lua` is retail's `vz`), by
//! identical code.
//!
//! A retail position names its function the way a live hook can: [`FunctionKey`] (line defined, last
//! line defined, instruction count, from the running `Proto`) plus a 0-based instruction index. A
//! debug-stripped retail chunk keeps both line-defined fields and its code, so its positions translate
//! through our compile's lines even though it carries none of its own.

use crate::luachunk::Proto;
use std::collections::HashMap;

/// The blocks the corpus was decompiled from (`crates/mercs2_script/corpus/README.md`, Provenance):
/// (WAD beside `vz.wad`, PTHS path, corpus bank).
pub const CORPUS_SOURCES: [(&str, &str, &str); 3] = [
    ("vz.wad", r"blocks\VZ\resident_P000_Q3.block", "resident"),
    ("vz.wad", r"blocks\VZ\scripts_vz_P000_Q3.block", "vz"),
    ("shell.wad", r"blocks\Shell\resident_P000_Q3.block", "shell"),
];

/// One script as shipped in a corpus bank: the retail side of a module's [`LineMap`].
#[derive(Clone, Debug)]
pub struct Shipped {
    /// The name its container's INFO gives it, which [`pair_modules`] pairs corpus files by.
    pub name: String,
    pub main: Proto,
}

impl Shipped {
    /// The name a live hook reports for the script's functions: its chunk's source name
    /// (`lua_debug::game_text`), which is not always [`Shipped::name`].
    pub fn module(&self) -> Result<String, String> {
        self.main
            .source
            .as_deref()
            .map(crate::lua_debug::game_text)
            .ok_or_else(|| format!("the shipped script {} carries no source name", self.name))
    }
}

/// Every script shipped in corpus bank `bank` (a [`CORPUS_SOURCES`] bank), read from the WAD beside
/// `vz_wad`, in block order.
pub fn shipped_scripts(vz_wad: &std::path::Path, bank: &str) -> Result<Vec<Shipped>, String> {
    let (wad, path, _) = CORPUS_SOURCES
        .iter()
        .find(|(_, _, b)| *b == bank)
        .ok_or_else(|| format!("no corpus bank `{bank}` (the banks are resident, vz and shell)"))?;
    let wad = vz_wad.with_file_name(wad);
    let mut file = std::fs::File::open(&wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = file.metadata().map_err(|e| format!("{}: {e}", wad.display()))?.len();
    let archive = crate::ffcs::load_ffcs_archive(&mut file, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let idx = archive
        .paths
        .iter()
        .position(|p| p.eq_ignore_ascii_case(path))
        .ok_or_else(|| format!("{}: no block `{path}` in PTHS", wad.display()))?;
    let bytes = crate::sges::decompress_block(&mut file, &archive.indx, idx as u16).map_err(|e| format!("{path}: {e}"))?;
    let block = crate::scripts_block::ScriptsBlock::parse(&bytes)?;
    let mut out = Vec::new();
    for (i, e) in block.entries.iter().enumerate().filter(|(_, e)| e.type_hash == crate::types::TYPE_HASH_SCRIPT) {
        let name = crate::scripts_block::container_info(&e.bytes).map_err(|err| format!("{path} entry {i}: {err}"))?.name;
        let chunk = crate::luachunk::decode(&block.extract_lua(i)?).map_err(|err| format!("{name} in {}: {err}", wad.display()))?;
        out.push(Shipped { name, main: chunk.main });
    }
    Ok(out)
}

/// The shipped script that corpus file `file` of a bank was decompiled from, given our compile of it
/// (`ours`) and the name of every corpus file of the bank (`files`): the script of its name, else the one
/// script with its code that no corpus file is named after, as [`pair_modules`] pairs them.
pub fn shipped_for<'a>(scripts: &'a [Shipped], file: &str, ours: &Proto, files: &[&str]) -> Result<&'a Shipped, String> {
    if let Some(s) = scripts.iter().find(|s| s.name.eq_ignore_ascii_case(file)) {
        return Ok(s);
    }
    let same: Vec<&Shipped> = scripts
        .iter()
        .filter(|s| !files.iter().any(|f| f.eq_ignore_ascii_case(&s.name)) && same_code(&s.main, ours))
        .collect();
    match same[..] {
        [one] => Ok(one),
        [] => Err(format!("no shipped script has the name or the code of corpus file {file}")),
        _ => Err(format!(
            "corpus file {file} has the code of {} shipped scripts ({}), so nothing says which it is",
            same.len(),
            same.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// A module that cannot be mapped, or a position the map does not hold: what and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// How a running function names itself: its retail `Proto`'s line-defined fields and code length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionKey {
    pub line_defined: u32,
    pub last_line_defined: u32,
    pub code_len: u32,
}

/// One function: where it sits in the tree, and each instruction's line in both line spaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionLines {
    /// Child indices from the main function (`[]` is the main function, `[3, 0]` its fourth child's
    /// first child).
    pub path: Vec<u32>,
    pub key: FunctionKey,
    /// The function's first line in the corpus text (`0` for the main function).
    pub line_defined: u32,
    /// The corpus line of each instruction.
    pub lines: Vec<u32>,
    /// Retail's line of each instruction; empty when retail's chunk is debug-stripped.
    pub retail_lines: Vec<u32>,
    /// Why this function does not map (its first difference from retail), or `None` when it does.
    pub unmapped: Option<String>,
}

/// Every function of one module, both line spaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineMap {
    pub functions: Vec<FunctionLines>,
    by_key: HashMap<FunctionKey, Vec<usize>>,
}

impl LineMap {
    /// Pair `retail`'s main function with `ours` (our compile of the corpus text, with line info).
    pub fn build(retail: &Proto, ours: &Proto) -> Result<LineMap, Error> {
        let mut functions = Vec::new();
        walk(retail, ours, &mut Vec::new(), &mut functions)?;
        let mut by_key: HashMap<FunctionKey, Vec<usize>> = HashMap::new();
        for (i, f) in functions.iter().enumerate() {
            by_key.entry(f.key).or_default().push(i);
        }
        Ok(LineMap { functions, by_key })
    }

    /// The functions that do not map, with why.
    pub fn unmapped(&self) -> impl Iterator<Item = &FunctionLines> {
        self.functions.iter().filter(|f| f.unmapped.is_some())
    }

    /// The function a retail hook reports. Two functions may share a key (two closures written on one
    /// line); that is only an answer when their corpus lines are the same too. A function that does
    /// not map is an error carrying its reason.
    pub fn function(&self, key: FunctionKey) -> Result<&FunctionLines, Error> {
        let hits = self.by_key.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        let (&first, rest) = hits.split_first().ok_or_else(|| {
            Error(format!(
                "no function defined at lines {}-{} with {} instructions",
                key.line_defined, key.last_line_defined, key.code_len
            ))
        })?;
        let f = &self.functions[first];
        if let Some(&bad) = hits.iter().find(|&&i| self.functions[i].unmapped.is_some()) {
            let g = &self.functions[bad];
            return Err(Error(format!("function {:?} does not map onto the corpus lines: {}", g.path, g.unmapped.as_deref().unwrap_or(""))));
        }
        if let Some(&other) = rest.iter().find(|&&i| self.functions[i].lines != f.lines) {
            return Err(Error(format!(
                "functions {:?} and {:?} are both defined at lines {}-{} with {} instructions and their \
                 corpus lines differ, so the position cannot be told apart",
                f.path, self.functions[other].path, key.line_defined, key.last_line_defined, key.code_len
            )));
        }
        Ok(f)
    }

    /// The corpus line of instruction `pc` (0-based) of the function a retail hook reports.
    pub fn line(&self, key: FunctionKey, pc: u32) -> Result<u32, Error> {
        let f = self.function(key)?;
        f.lines.get(pc as usize).copied().ok_or_else(|| {
            Error(format!("instruction {pc} is past the end of function {:?} ({} instructions)", f.path, f.lines.len()))
        })
    }

    /// Every retail position on corpus line `line`: where a breakpoint on that line must stop.
    /// Functions that do not map hold none.
    pub fn positions(&self, line: u32) -> Vec<(FunctionKey, u32)> {
        self.functions
            .iter()
            .filter(|f| f.unmapped.is_none())
            .flat_map(|f| f.lines.iter().enumerate().filter(move |(_, &l)| l == line).map(move |(pc, _)| (f.key, pc as u32)))
            .collect()
    }
}

/// The first way `retail` and `ours` differ in anything but line info, ignoring nested functions.
fn difference(retail: &Proto, ours: &Proto) -> Option<String> {
    let shape = |p: &Proto| (p.nups, p.num_params, p.is_vararg, p.max_stack);
    if shape(retail) != shape(ours) {
        return Some(format!(
            "upvalues, parameters, vararg or stack differ (retail {:?}, ours {:?})",
            shape(retail),
            shape(ours)
        ));
    }
    if let Some(pc) = (0..retail.code.len().max(ours.code.len())).find(|&i| retail.code.get(i) != ours.code.get(i)) {
        return Some(format!(
            "instruction {pc} differs (retail {:08x?}, ours {:08x?}; {} vs {} instructions)",
            retail.code.get(pc),
            ours.code.get(pc),
            retail.code.len(),
            ours.code.len()
        ));
    }
    if let Some(k) = (0..retail.constants.len().max(ours.constants.len())).find(|&i| retail.constants.get(i) != ours.constants.get(i)) {
        let show = |c: Option<&crate::luachunk::Constant>| match c {
            Some(crate::luachunk::Constant::String(s)) => format!("{:?}", String::from_utf8_lossy(s)),
            other => format!("{other:?}"),
        };
        return Some(format!("constant {k} differs (retail {}, ours {})", show(retail.constants.get(k)), show(ours.constants.get(k))));
    }
    None
}

/// Whether two chunks are the same program: every function equal in everything but line info.
pub fn same_code(retail: &Proto, ours: &Proto) -> bool {
    difference(retail, ours).is_none()
        && retail.protos.len() == ours.protos.len()
        && retail.protos.iter().zip(&ours.protos).all(|(r, o)| same_code(r, o))
}

fn walk(retail: &Proto, ours: &Proto, path: &mut Vec<u32>, out: &mut Vec<FunctionLines>) -> Result<(), Error> {
    let at = || {
        format!(
            "function {path:?} (retail lines {}-{}, corpus lines {}-{})",
            retail.line_defined, retail.last_line_defined, ours.line_defined, ours.last_line_defined
        )
    };
    if retail.protos.len() != ours.protos.len() {
        return Err(Error(format!("{}: {} nested functions in retail, {} in ours", at(), retail.protos.len(), ours.protos.len())));
    }
    if ours.line_info.len() != ours.code.len() {
        return Err(Error(format!(
            "{}: our compile has {} line records for {} instructions; compile the corpus text with line info",
            at(),
            ours.line_info.len(),
            ours.code.len()
        )));
    }
    if !retail.line_info.is_empty() && retail.line_info.len() != retail.code.len() {
        return Err(Error(format!("{}: retail has {} line records for {} instructions", at(), retail.line_info.len(), retail.code.len())));
    }
    out.push(FunctionLines {
        path: path.clone(),
        key: FunctionKey {
            line_defined: retail.line_defined,
            last_line_defined: retail.last_line_defined,
            code_len: retail.code.len() as u32,
        },
        line_defined: ours.line_defined,
        lines: ours.line_info.clone(),
        retail_lines: retail.line_info.clone(),
        unmapped: difference(retail, ours).map(|why| format!("{}: {why}", at())),
    });
    for (i, (r, o)) in retail.protos.iter().zip(&ours.protos).enumerate() {
        path.push(i as u32);
        walk(r, o, path, out)?;
        path.pop();
    }
    Ok(())
}

/// Retail's scripts paired with the corpus's files, and what could not be paired.
#[derive(Debug, Default)]
pub struct Pairing {
    /// (retail script, corpus file), each index into the slices given to [`pair_modules`].
    pub pairs: Vec<(usize, usize)>,
    /// Corpus files no retail script has the name or the code of.
    pub unpaired_files: Vec<usize>,
    /// Retail scripts with neither a file of their name nor one of their code.
    pub unpaired_scripts: Vec<usize>,
}

/// Pair `scripts` (retail: name, main function) with `files` (corpus: name, our compile's main
/// function). Names pair first, ignoring ASCII case. A file left over pairs with the one left-over
/// script whose code is the same as its; two such scripts is an error naming both, since nothing then
/// says which the file is.
pub fn pair_modules(scripts: &[(&str, &Proto)], files: &[(&str, &Proto)]) -> Result<Pairing, Error> {
    let mut out = Pairing::default();
    let mut taken = vec![false; scripts.len()];
    let mut left = Vec::new();
    for (f, (name, _)) in files.iter().enumerate() {
        match scripts.iter().position(|(s, _)| s.eq_ignore_ascii_case(name)) {
            Some(s) => {
                taken[s] = true;
                out.pairs.push((s, f));
            }
            None => left.push(f),
        }
    }
    for f in left {
        let same: Vec<usize> = (0..scripts.len()).filter(|&s| !taken[s] && same_code(scripts[s].1, files[f].1)).collect();
        match same.as_slice() {
            [] => out.unpaired_files.push(f),
            [s] => {
                taken[*s] = true;
                out.pairs.push((*s, f));
            }
            many => {
                return Err(Error(format!(
                    "corpus file `{}` has the code of {} retail scripts ({}), so nothing says which it is",
                    files[f].0,
                    many.len(),
                    many.iter().map(|&s| scripts[s].0).collect::<Vec<_>>().join(", ")
                )))
            }
        }
    }
    out.unpaired_scripts = (0..scripts.len()).filter(|&s| !taken[s]).collect();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::luachunk::Constant;

    fn proto(line_defined: u32, last: u32, code: Vec<u32>, lines: Vec<u32>, protos: Vec<Proto>) -> Proto {
        Proto {
            source: None,
            line_defined,
            last_line_defined: last,
            nups: 0,
            num_params: 0,
            is_vararg: 0,
            max_stack: 2,
            code,
            constants: vec![Constant::String(b"Ai".to_vec())],
            protos,
            line_info: lines,
            loc_vars: vec![],
            upvalues: vec![],
        }
    }

    /// Retail: main at 0, two children. Ours: the same code with every line moved.
    fn pair() -> (Proto, Proto) {
        let retail = proto(
            0,
            0,
            vec![0x24, 0x1e],
            vec![10, 10],
            vec![proto(500, 503, vec![0x05, 0x1c, 0x1e], vec![501, 502, 503], vec![]), proto(530, 530, vec![0x1e], vec![530], vec![])],
        );
        let ours = proto(
            0,
            0,
            vec![0x24, 0x1e],
            vec![4, 4],
            vec![proto(62, 64, vec![0x05, 0x1c, 0x1e], vec![63, 63, 64], vec![]), proto(70, 70, vec![0x1e], vec![70], vec![])],
        );
        (retail, ours)
    }

    #[test]
    fn instruction_i_carries_its_line_across() {
        let (retail, ours) = pair();
        let m = LineMap::build(&retail, &ours).unwrap();
        let key = FunctionKey { line_defined: 500, last_line_defined: 503, code_len: 3 };
        assert_eq!(m.line(key, 0).unwrap(), 63);
        assert_eq!(m.line(key, 2).unwrap(), 64);
        assert_eq!(m.function(key).unwrap().path, vec![0]);
        assert_eq!(m.function(key).unwrap().retail_lines, vec![501, 502, 503]);
        assert!(m.line(key, 3).unwrap_err().to_string().contains("past the end"));
    }

    #[test]
    fn a_corpus_line_lists_every_position_to_stop_at() {
        let (retail, ours) = pair();
        let m = LineMap::build(&retail, &ours).unwrap();
        let key = FunctionKey { line_defined: 500, last_line_defined: 503, code_len: 3 };
        assert_eq!(m.positions(63), vec![(key, 0), (key, 1)]);
        assert!(m.positions(99).is_empty());
    }

    #[test]
    fn a_stripped_retail_chunk_still_maps() {
        let (mut retail, ours) = pair();
        retail.line_info.clear();
        retail.protos[0].line_info.clear();
        retail.protos[1].line_info.clear();
        let m = LineMap::build(&retail, &ours).unwrap();
        assert_eq!(m.line(FunctionKey { line_defined: 530, last_line_defined: 530, code_len: 1 }, 0).unwrap(), 70);
    }

    #[test]
    fn a_function_that_differs_is_unmapped_and_the_rest_still_maps() {
        let (retail, mut ours) = pair();
        ours.protos[0].code[1] = 0x1d;
        let m = LineMap::build(&retail, &ours).unwrap();
        let bad = FunctionKey { line_defined: 500, last_line_defined: 503, code_len: 3 };
        let e = m.line(bad, 0).unwrap_err().to_string();
        assert!(e.contains("function [0]") && e.contains("instruction 1 differs"), "{e}");
        assert!(m.positions(63).is_empty(), "an unmapped function holds no breakpoint position");
        assert_eq!(m.line(FunctionKey { line_defined: 530, last_line_defined: 530, code_len: 1 }, 0).unwrap(), 70);
        assert_eq!(m.unmapped().map(|f| f.path.clone()).collect::<Vec<_>>(), vec![vec![0]]);

        let (retail, mut ours) = pair();
        ours.protos[1].constants[0] = Constant::String(b"Pg".to_vec());
        let m = LineMap::build(&retail, &ours).unwrap();
        let e = m.line(FunctionKey { line_defined: 530, last_line_defined: 530, code_len: 1 }, 0).unwrap_err().to_string();
        assert!(e.contains(r#"constant 0 differs (retail "Ai", ours "Pg")"#), "{e}");
    }

    #[test]
    fn a_tree_that_cannot_be_paired_refuses_the_module() {
        let (retail, mut ours) = pair();
        ours.protos.pop();
        assert!(LineMap::build(&retail, &ours).unwrap_err().to_string().contains("2 nested functions in retail, 1 in ours"));

        let (retail, mut ours) = pair();
        ours.protos[0].line_info.clear();
        assert!(LineMap::build(&retail, &ours).unwrap_err().to_string().contains("compile the corpus text with line info"));
    }

    #[test]
    fn modules_pair_by_name_then_by_identical_code() {
        let (retail, ours) = pair();
        let other = proto(0, 0, vec![0x1e], vec![1], vec![]);
        let scripts = [("vz", &retail), ("MrxTask", &other), ("c4", &other)];
        let files = [("mrxtask", &other), ("xQ!L", &ours), ("Multi", &proto(0, 0, vec![0x1d], vec![1], vec![]))];
        let p = pair_modules(&scripts, &files).unwrap();
        assert_eq!(p.pairs, vec![(1, 0), (0, 1)]);
        assert_eq!(p.unpaired_files, vec![2]);
        assert_eq!(p.unpaired_scripts, vec![2]);

        // Two left-over scripts with the file's code: nothing says which one it is.
        let scripts = [("a", &other), ("b", &other)];
        let e = pair_modules(&scripts, &[("Init", &other)]).unwrap_err().to_string();
        assert!(e.contains("`Init` has the code of 2 retail scripts (a, b)"), "{e}");
    }

    #[test]
    fn one_corpus_file_finds_its_shipped_script_by_name_then_by_code() {
        let (retail, ours) = pair();
        let other = proto(0, 0, vec![0x1e], vec![1], vec![]);
        let named = |name: &str, main: &Proto, source: &[u8]| Shipped {
            name: name.into(),
            main: Proto { source: Some(source.to_vec()), ..main.clone() },
        };
        let scripts = [named("vz", &retail, b"xQ!L"), named("MrxTask", &other, b"MrxTask"), named("c4", &other, b"Init")];
        let files = ["mrxtask", "xQ!L", "Init"];
        let by_name = shipped_for(&scripts, "mrxtask", &other, &files).unwrap();
        assert_eq!((by_name.name.as_str(), by_name.module().unwrap().as_str()), ("MrxTask", "MrxTask"));
        // `xQ!L` is named after no script: the one script of its code no file is named after.
        let by_code = shipped_for(&scripts, "xQ!L", &ours, &files).unwrap();
        assert_eq!((by_code.name.as_str(), by_code.module().unwrap().as_str()), ("vz", "xQ!L"));
        // `Init` has `MrxTask`'s code too, but a file is named after that one, so only `c4` is left.
        assert_eq!(shipped_for(&scripts, "Init", &other, &files).unwrap().name, "c4");

        let e = shipped_for(&scripts, "Multi", &proto(0, 0, vec![0x1d], vec![1], vec![]), &files).unwrap_err();
        assert!(e.contains("no shipped script has the name or the code of corpus file Multi"), "{e}");
        let twins = [named("a", &other, b"a"), named("b", &other, b"b")];
        let e = shipped_for(&twins, "Init", &other, &["Init"]).unwrap_err();
        assert!(e.contains("Init has the code of 2 shipped scripts (a, b)"), "{e}");
        let e = Shipped { name: "x".into(), main: other }.module().unwrap_err();
        assert!(e.contains("x carries no source name"), "{e}");
    }

    #[test]
    fn same_key_functions_answer_only_when_their_lines_agree() {
        let twin = |lines| proto(530, 530, vec![0x1e], lines, vec![]);
        let retail = proto(0, 0, vec![0x24, 0x24, 0x1e], vec![1, 1, 1], vec![twin(vec![530]), twin(vec![530])]);
        let key = FunctionKey { line_defined: 530, last_line_defined: 530, code_len: 1 };

        let ours = proto(0, 0, vec![0x24, 0x24, 0x1e], vec![1, 1, 1], vec![twin(vec![70]), twin(vec![70])]);
        assert_eq!(LineMap::build(&retail, &ours).unwrap().line(key, 0).unwrap(), 70);

        let ours = proto(0, 0, vec![0x24, 0x24, 0x1e], vec![1, 1, 1], vec![twin(vec![70]), twin(vec![71])]);
        let e = LineMap::build(&retail, &ours).unwrap().line(key, 0).unwrap_err().to_string();
        assert!(e.contains("cannot be told apart"), "{e}");
    }

    #[test]
    fn an_unknown_function_is_an_error() {
        let (retail, ours) = pair();
        let m = LineMap::build(&retail, &ours).unwrap();
        let e = m.line(FunctionKey { line_defined: 1, last_line_defined: 2, code_len: 3 }, 0).unwrap_err().to_string();
        assert!(e.contains("no function defined at lines 1-2"), "{e}");
    }
}
