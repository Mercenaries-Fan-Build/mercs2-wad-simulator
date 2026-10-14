//! The per-function index of one Lua corpus.
//!
//! [`index`] takes the WAD files of one `corpora.tsv` corpus — each identified by sha256 against
//! `wads.tsv`, every WAD of the corpus present exactly once — and writes one [`FnRow`] per Lua
//! function in every script container ([`crate::wadscripts`]), decoded with [`crate::luachunk`].
//!
//! A function's place in its script is its `path`: `main` for the chunk's main function, then
//! `/<i>` per nesting level (`main/3/0` is the first function defined inside the main function's
//! fourth). Its fingerprints are platform-neutral:
//!
//! * `code_hash` — SHA-256 (first 16 hex digits) of its instruction words, each written
//!   little-endian, so the same bytecode hashes the same on PC and console;
//! * `constants_hash` — the same over its constants: a type byte (0 nil, 1 boolean, 3 number,
//!   4 string), then the boolean byte, the number's float bits little-endian, or the string's
//!   length (u32 little-endian) and bytes.
//!
//! Line numbers are those the chunk records (0 when its debug information is stripped).
//!
//! `lookups` lists, in order of first use and without repeats, every `Namespace.Name` the function
//! reads the way Lua 5.1 compiles that expression: `GETGLOBAL r, "Namespace"` (opcode 5)
//! immediately followed by `GETTABLE r, r, "Name"` (opcode 6) with a string-constant key. A lookup
//! may be a function about to be called or any other field (an event id, a constant, a sub-table);
//! lookups compiled any other way are not listed.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mercs2_formats::ffcs::Endian;
use sha2::{Digest, Sha256};

use crate::corpus::{file_sha256, Corpora};
use crate::luachunk::{decode, decode_info, encode, Constant, Proto};
use crate::wadscripts::scripts;
use crate::Error;

pub const HEADER: &str = "corpus\twad\tscript\tname_hash\tpath\tline_defined\tlast_line_defined\tnum_params\tis_vararg\tnups\tmax_stack\tcode_len\tconstants\tfunctions\tcode_hash\tconstants_hash\tdebug\tlookups";

const OP_GETGLOBAL: u32 = 5;
const OP_GETTABLE: u32 = 6;

/// One function of a corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnRow {
    pub corpus: String,
    /// The WAD's name in `wads.tsv`.
    pub wad: String,
    pub script: String,
    pub name_hash: u32,
    pub path: String,
    pub line_defined: u32,
    pub last_line_defined: u32,
    pub num_params: u8,
    pub is_vararg: u8,
    pub nups: u8,
    pub max_stack: u8,
    pub code_len: usize,
    pub constants: usize,
    /// Functions defined directly inside this one.
    pub functions: usize,
    pub code_hash: String,
    pub constants_hash: String,
    /// Whether this function carries line info, local or upvalue names, or a source name.
    pub debug: bool,
    /// `Namespace.Name` lookups, in order of first use (see the module docs).
    pub lookups: Vec<String>,
}

impl FnRow {
    pub fn tsv(&self) -> String {
        format!(
            "{}\t{}\t{}\t0x{:08x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.corpus,
            self.wad,
            self.script,
            self.name_hash,
            self.path,
            self.line_defined,
            self.last_line_defined,
            self.num_params,
            self.is_vararg,
            self.nups,
            self.max_stack,
            self.code_len,
            self.constants,
            self.functions,
            self.code_hash,
            self.constants_hash,
            if self.debug { "yes" } else { "stripped" },
            self.lookups.join(";")
        )
    }
}

/// The function's `Namespace.Name` lookups (see the module docs).
pub fn lookups(p: &Proto) -> Vec<String> {
    let k = |i: u32| match p.constants.get(i as usize) {
        Some(Constant::String(s)) => Some(String::from_utf8_lossy(s).into_owned()),
        _ => None,
    };
    let mut out: Vec<String> = Vec::new();
    for w in p.code.windows(2) {
        let (g, t) = (w[0], w[1]);
        if g & 0x3f != OP_GETGLOBAL || t & 0x3f != OP_GETTABLE {
            continue;
        }
        let reg = (g >> 6) & 0xff;
        let (ta, tb, tc) = ((t >> 6) & 0xff, (t >> 23) & 0x1ff, (t >> 14) & 0x1ff);
        if ta != reg || tb != reg || tc & 0x100 == 0 {
            continue;
        }
        if let (Some(ns), Some(name)) = (k(g >> 14), k(tc & 0xff)) {
            let lookup = format!("{ns}.{name}");
            if !out.contains(&lookup) {
                out.push(lookup);
            }
        }
    }
    out
}

fn short(h: Sha256) -> String {
    format!("{:x}", h.finalize())[..16].to_string()
}

pub fn code_hash(p: &Proto) -> String {
    let mut h = Sha256::new();
    for i in &p.code {
        h.update(i.to_le_bytes());
    }
    short(h)
}

pub fn constants_hash(p: &Proto) -> String {
    let mut h = Sha256::new();
    for c in &p.constants {
        match c {
            Constant::Nil => h.update([0u8]),
            Constant::Boolean(v) => h.update([1u8, *v as u8]),
            Constant::Number(bits) => {
                h.update([3u8]);
                h.update(bits.to_le_bytes());
            }
            Constant::String(s) => {
                h.update([4u8]);
                h.update((s.len() as u32).to_le_bytes());
                h.update(s);
            }
        }
    }
    short(h)
}

fn rows(p: &Proto, path: String, base: &FnRow, out: &mut Vec<FnRow>) {
    out.push(FnRow {
        path: path.clone(),
        line_defined: p.line_defined,
        last_line_defined: p.last_line_defined,
        num_params: p.num_params,
        is_vararg: p.is_vararg,
        nups: p.nups,
        max_stack: p.max_stack,
        code_len: p.code.len(),
        constants: p.constants.len(),
        functions: p.protos.len(),
        code_hash: code_hash(p),
        constants_hash: constants_hash(p),
        debug: p.source.is_some() || !p.line_info.is_empty() || !p.loc_vars.is_empty() || !p.upvalues.is_empty(),
        lookups: lookups(p),
        ..base.clone()
    });
    for (i, q) in p.protos.iter().enumerate() {
        rows(q, format!("{path}/{i}"), base, out);
    }
}

/// Every function of corpus `corpus_id`, from its WAD files `wads`.
pub fn index(corpora: &Corpora, corpus_id: &str, wads: &[PathBuf]) -> Result<Vec<FnRow>, Error> {
    let corpus = corpora
        .corpus(corpus_id)
        .ok_or_else(|| Error::new(format!("corpora.tsv has no corpus {corpus_id}")))?;
    let want_endian = if corpus.platform == "pc" { Endian::Little } else { Endian::Big };
    let catalogued: Vec<_> = corpora.wads_of(corpus_id).collect();
    let mut given: BTreeMap<String, (PathBuf, String)> = BTreeMap::new();
    for path in wads {
        let sha = file_sha256(path)?;
        let w = catalogued.iter().find(|w| w.sha256 == sha).ok_or_else(|| {
            Error::new(format!("{} (sha256 {sha}) is not a WAD of corpus {corpus_id}", path.display()))
        })?;
        if let Some((prev, _)) = given.insert(w.name.clone(), (path.clone(), sha)) {
            return Err(Error::new(format!(
                "{} and {} are both corpus {corpus_id}'s {}",
                prev.display(),
                path.display(),
                w.name
            )));
        }
    }
    let missing: Vec<&str> = catalogued.iter().filter(|w| !given.contains_key(&w.name)).map(|w| w.name.as_str()).collect();
    if !missing.is_empty() {
        return Err(Error::new(format!("corpus {corpus_id}: no file given for {}", missing.join(", "))));
    }
    let mut out = Vec::new();
    for (name, (path, _)) in &given {
        let ws = scripts(path)?;
        if ws.endian != want_endian {
            return Err(Error::new(format!(
                "{}: a {:?} WAD in {} corpus {corpus_id}",
                path.display(),
                ws.endian,
                corpus.platform
            )));
        }
        for s in &ws.scripts {
            let what = format!("{} block {} entry {} (0x{:08x})", path.display(), s.block, s.entry, s.name_hash);
            let info = s.chunk(b"INFO").ok_or_else(|| Error::new(format!("{what}: no INFO descriptor")))?;
            let info = decode_info(info, ws.endian).map_err(|e| Error::new(format!("{what}: {e}")))?;
            let binn = s.chunk(b"BINN").ok_or_else(|| Error::new(format!("{what}: no BINN descriptor")))?;
            let chunk = decode(binn).map_err(|e| Error::new(format!("{what}: {e}")))?;
            if encode(&chunk) != binn {
                return Err(Error::new(format!("{what}: the decoded chunk does not re-encode to its bytes")));
            }
            let base = FnRow {
                corpus: corpus_id.to_string(),
                wad: name.clone(),
                script: info.name,
                name_hash: s.name_hash,
                path: String::new(),
                line_defined: 0,
                last_line_defined: 0,
                num_params: 0,
                is_vararg: 0,
                nups: 0,
                max_stack: 0,
                code_len: 0,
                constants: 0,
                functions: 0,
                code_hash: String::new(),
                constants_hash: String::new(),
                debug: false,
                lookups: Vec::new(),
            };
            rows(&chunk.main, "main".into(), &base, &mut out);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::luachunk::LocVar;

    fn proto(code: Vec<u32>, constants: Vec<Constant>, protos: Vec<Proto>) -> Proto {
        Proto {
            source: None,
            line_defined: 0,
            last_line_defined: 0,
            nups: 0,
            num_params: 0,
            is_vararg: 2,
            max_stack: 2,
            code,
            constants,
            protos,
            line_info: vec![],
            loc_vars: vec![],
            upvalues: vec![],
        }
    }

    #[test]
    fn hashes_ignore_debug_information_and_paths_follow_nesting() {
        let mut debug = proto(vec![0x24, 0x80001e], vec![Constant::String(b"x".to_vec())], vec![]);
        let stripped = debug.clone();
        debug.line_info = vec![1, 2];
        debug.loc_vars = vec![LocVar { name: b"a".to_vec(), start_pc: 0, end_pc: 1 }];
        assert_eq!(code_hash(&debug), code_hash(&stripped));
        assert_eq!(constants_hash(&debug), constants_hash(&stripped));
        assert_ne!(constants_hash(&stripped), constants_hash(&proto(vec![0x24, 0x80001e], vec![Constant::Number(0)], vec![])));

        let tree = proto(vec![0x1e], vec![], vec![proto(vec![0x1e], vec![], vec![proto(vec![0x1e], vec![], vec![])]), stripped]);
        let base = FnRow {
            corpus: "c".into(),
            wad: "w".into(),
            script: "s".into(),
            name_hash: 1,
            path: String::new(),
            line_defined: 0,
            last_line_defined: 0,
            num_params: 0,
            is_vararg: 0,
            nups: 0,
            max_stack: 0,
            code_len: 0,
            constants: 0,
            functions: 0,
            code_hash: String::new(),
            constants_hash: String::new(),
            debug: false,
            lookups: Vec::new(),
        };
        let mut out = Vec::new();
        rows(&tree, "main".into(), &base, &mut out);
        let paths: Vec<&str> = out.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["main", "main/0", "main/0/0", "main/1"]);
        assert_eq!(out[0].functions, 2);
        assert!(!out[3].debug);
    }

    #[test]
    fn lookups_are_adjacent_getglobal_gettable_pairs_with_a_constant_key() {
        // K: 0 "Ns", 1 "Fn", 2 "Other".
        let getglobal = |a: u32, bx: u32| OP_GETGLOBAL | (a << 6) | (bx << 14);
        let gettable = |a: u32, b: u32, c: u32| OP_GETTABLE | (a << 6) | (c << 14) | (b << 23);
        let k = vec![
            Constant::String(b"Ns".to_vec()),
            Constant::String(b"Fn".to_vec()),
            Constant::String(b"Other".to_vec()),
        ];
        let code = vec![
            getglobal(0, 0),
            gettable(0, 0, 0x101), // Ns.Fn
            getglobal(1, 0),
            gettable(1, 1, 0x101), // Ns.Fn again: listed once
            getglobal(2, 0),
            gettable(2, 2, 3),     // key from a register, not a constant
            getglobal(3, 0),
            gettable(4, 3, 0x102), // result in another register: not this pattern
            getglobal(5, 0),
            0x1e,
            gettable(5, 5, 0x102), // not adjacent
            getglobal(6, 0),
            gettable(6, 6, 0x102), // Ns.Other
        ];
        assert_eq!(lookups(&proto(code, k, vec![])), ["Ns.Fn", "Ns.Other"]);
    }
}
