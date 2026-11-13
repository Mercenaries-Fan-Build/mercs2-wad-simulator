//! The arguments each Lua binding of an image reads: which Lua stack position, read as what.
//!
//! **Readers** (`data/lua_readers.tsv`, [`Readers`]): per image, the functions a binding reads an
//! argument with — the engine's argument helpers and the Lua API calls a binding makes itself — each
//! with the 0-based argument of its call that holds the stack position, how many consecutive positions
//! it reads, and what it takes. The table opens with `#` comment lines; no field is empty.
//!
//! **Reads** ([`binding_reads`]): the positions of the binding's **full form**, the call that gives
//! every argument it reads. A binding's decomp is tokenized ([`crate::hashsites`]) and read in text
//! order. A local holds a constant when a statement `local = <expr>` gives it one: an integer literal,
//! a sum or difference of constants (casts dropped), or a reader's call, whose value is the number of
//! positions it reads — what a reader returns when it succeeds (a callback: two, its function and its
//! args table). The branches of an `if` / `else if` / `else` chain are alternatives, each starting from
//! the constants before the chain. When the chain's `if` tests whether a reader succeeded (`0 < x` or
//! `x < 1` of a local a reader's call gave, or a local holding such a test; the test may end a comma
//! expression, `(x = FUN_…(…), x < 1)`, and be one term of an `||` chain of failure tests or an `&&`
//! chain of success tests, `VO.Cancel`'s `if (absent || (x = …, x < 1))`), the full form takes the
//! way it succeeded: a binding with an optional argument reads the next one at a position that depends
//! on whether it was given (`Airstrike.Flyby`: a name, a handle, or neither first). After any other
//! chain a local holds a constant only when every way through it that goes on agrees (past it too,
//! when there is no `else`; a branch with its own `return` or `goto` does not go on), and the reads it
//! places are `computed` otherwise. Every call of a reader records a read at the
//! position its index argument evaluates to; an index that does not evaluate, and every read inside a
//! loop's body (`do {`, `while (…) {`, `for (…) {`: one call that reads a new position each pass), is
//! `computed`, and a negative one (relative to the top of the stack, a
//! value the binding pushed itself) is not an argument and is not recorded. Repeated reads of one
//! position with one reader count once.
//!
//! A binding that hands its Lua state on — a call whose first argument is its own first parameter,
//! `param_1`, to a function that is not a reader (`Event.Create` calls a shared implementation with a
//! mode flag) — is followed into that function and the ones it hands the state on to, [`DELEGATE_DEPTH`]
//! calls deep; those reads carry the function they were made in (`via`). A shared implementation's reads
//! are those of all its modes.
//!
//! **Output** ([`to_tsv`]): `#` lines naming the image and the readers, the header [`ARGS_HEADER`],
//! then one row per read in text order — binding (its Lua path), function, position (`computed` when
//! unknown), what it reads, its positions, the reader, the function it was made in (`-`: the binding),
//! and `yes` when another read takes the same position as something else (`any` aside): the binding
//! moved values on the stack between them (`Pg.LoadAsset` reads its callback after shifting its two
//! names off), which the text does not show — and for a binding that reads no argument, one row with
//! position `-` and `(none)`.

use std::collections::{HashMap, HashSet};

use crate::hashsites::{tokenize, Tok};
use crate::luacalls::{call_args, Functions};
use crate::{commented_table_rows, Error};

pub const READERS_HEADER: &str = "image_id\tva\tindex\tslots\treads\tevidence";
pub const ARGS_HEADER: &str = "binding\tfunction\tposition\treads\tslots\treader\tvia\tconflict";

/// How many positions a reader reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slots {
    Fixed(i64),
    /// One, and the next when it holds a table (a callback and its data).
    OneOrTwo,
}

impl Slots {
    fn text(self) -> String {
        match self {
            Slots::Fixed(n) => n.to_string(),
            Slots::OneOrTwo => "1..2".into(),
        }
    }
}

/// Where a reader finds the stack position it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    /// The 0-based argument of its call holds it.
    Arg(usize),
    /// Always this position, whatever the call passes.
    Position(i64),
}

/// One row of `data/lua_readers.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reader {
    pub image_id: String,
    pub va: u32,
    pub index: At,
    pub slots: Slots,
    pub reads: String,
    pub evidence: String,
}

#[derive(Clone, Debug)]
pub struct Readers {
    pub rows: Vec<Reader>,
}

impl Readers {
    pub fn embedded() -> Result<Readers, Error> {
        Readers::parse(include_str!("../data/lua_readers.tsv"))
    }

    pub fn parse(text: &str) -> Result<Readers, Error> {
        let mut rows: Vec<Reader> = Vec::new();
        for (n, f) in commented_table_rows("lua_readers.tsv", READERS_HEADER, text)? {
            let at = |m: String| Error::new(format!("lua_readers.tsv line {n}: {m}"));
            let va = f[1]
                .strip_prefix("0x")
                .filter(|h| h.len() == 8)
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .ok_or_else(|| at(format!("va {:?} is not 0x and eight hex digits", f[1])))?;
            let index = match f[2].strip_prefix('@') {
                Some(p) => At::Position(p.parse().ok().filter(|n: &i64| *n > 0).ok_or_else(|| at(format!("index {:?} is not @ and a position", f[2])))?),
                None => At::Arg(f[2].parse().map_err(|_| at(format!("index {:?} is not a number or @position", f[2])))?),
            };
            let slots = match f[3] {
                "1..2" => Slots::OneOrTwo,
                s => Slots::Fixed(s.parse().ok().filter(|n: &i64| *n > 0).ok_or_else(|| at(format!("slots {s:?} is not a positive number or 1..2")))?),
            };
            if rows.iter().any(|r| r.image_id == f[0] && r.va == va) {
                return Err(at(format!("a second row for {} {}", f[0], f[1])));
            }
            rows.push(Reader { image_id: f[0].into(), va, index, slots, reads: f[4].into(), evidence: f[5].into() });
        }
        Ok(Readers { rows })
    }

    pub fn of<'s>(&'s self, image_id: &'s str) -> impl Iterator<Item = &'s Reader> + 's {
        self.rows.iter().filter(move |r| r.image_id == image_id)
    }
}

/// One argument read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    /// The Lua stack position (1-based); `None` when the index is computed.
    pub position: Option<i64>,
    pub reads: String,
    pub slots: Slots,
    pub reader: u32,
    /// The function the binding hands its Lua state to that makes the read, when the binding does not
    /// make it itself.
    pub via: Option<u32>,
}

/// How many calls deep a binding is followed through the functions it hands its Lua state to.
pub const DELEGATE_DEPTH: usize = 2;

/// The functions a decomp calls with its own first parameter (`param_1`, a binding's Lua state) as their
/// first argument, other than the readers: where a binding that delegates reads its arguments.
fn delegates(decomp: &str, readers: &HashMap<u32, &Reader>) -> Vec<u32> {
    let toks = tokenize(decomp);
    let mut out = Vec::new();
    for i in 0..toks.len() {
        let Tok::Ident(w) = &toks[i] else { continue };
        let Some(f) = fun_va(w).filter(|f| !readers.contains_key(f)) else { continue };
        if !matches!(toks.get(i + 1), Some(Tok::Punct("("))) {
            continue;
        }
        let Some(args) = call_args(&toks, i + 1) else { continue };
        if matches!(args.first().map(|&(s, e)| &toks[s..e]), Some([Tok::Ident(p)]) if p == "param_1") && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// The value of `toks` as an integer: literals (a 64-bit literal whose high word is all ones is
/// negative), locals in `env`, `+`, `-`, parentheses and casts.
fn eval(toks: &[Tok], env: &HashMap<String, i64>) -> Option<i64> {
    fn expr(t: &[Tok], i: &mut usize, env: &HashMap<String, i64>) -> Option<i64> {
        let mut v = term(t, i, env)?;
        while let Some(Tok::Punct(p)) = t.get(*i) {
            match *p {
                "+" => {
                    *i += 1;
                    v += term(t, i, env)?;
                }
                "-" => {
                    *i += 1;
                    v -= term(t, i, env)?;
                }
                _ => break,
            }
        }
        Some(v)
    }
    fn term(t: &[Tok], i: &mut usize, env: &HashMap<String, i64>) -> Option<i64> {
        match t.get(*i)? {
            Tok::Num(Some(v), _) => {
                *i += 1;
                Some(if *v >> 32 == 0xffff_ffff { *v as u32 as i32 as i64 } else { *v as i64 })
            }
            Tok::Ident(w) => {
                *i += 1;
                env.get(w).copied()
            }
            Tok::Punct("-") => {
                *i += 1;
                Some(-term(t, i, env)?)
            }
            Tok::Punct("(") => {
                // a cast: `(` type words and `*`s `)` before an operand
                let close = (*i + 1..t.len()).find(|&k| matches!(t[k], Tok::Punct(")")))?;
                let cast = close > *i + 1
                    && t[*i + 1..close].iter().all(|x| matches!(x, Tok::Ident(w) if !env.contains_key(w)) || matches!(x, Tok::Punct("*")))
                    && close + 1 < t.len();
                if cast {
                    *i = close + 1;
                    return term(t, i, env);
                }
                *i += 1;
                let v = expr(t, i, env)?;
                matches!(t.get(*i), Some(Tok::Punct(")"))).then(|| *i += 1)?;
                Some(v)
            }
            _ => None,
        }
    }
    let mut i = 0;
    let v = expr(toks, &mut i, env)?;
    (i == toks.len()).then_some(v)
}

/// The end (exclusive) of the expression starting at `start`: the first `;`, `,` or unmatched `)` at
/// its depth.
fn expr_end(toks: &[Tok], start: usize) -> usize {
    let mut depth = 0i32;
    for (k, t) in toks.iter().enumerate().skip(start) {
        match t {
            Tok::Punct("(") | Tok::Punct("[") => depth += 1,
            Tok::Punct(")") | Tok::Punct("]") if depth == 0 => return k,
            Tok::Punct(")") | Tok::Punct("]") => depth -= 1,
            Tok::Punct(";") | Tok::Punct(",") if depth == 0 => return k,
            _ => {}
        }
    }
    toks.len()
}

/// The word before the `(` that the `)` at `close` closes (`if`, `while`, `for`, a function's name),
/// with its index.
fn head_word(toks: &[Tok], close: usize) -> Option<(&str, usize)> {
    let mut depth = 0i32;
    let mut k = close;
    loop {
        match &toks[k] {
            Tok::Punct(")") => depth += 1,
            Tok::Punct("(") => {
                depth -= 1;
                if depth == 0 {
                    let j = k.checked_sub(1)?;
                    return match &toks[j] {
                        Tok::Ident(w) => Some((w.as_str(), j)),
                        _ => None,
                    };
                }
            }
            _ => {}
        }
        if k == 0 {
            return None;
        }
        k -= 1;
    }
}

/// What a `{` opens.
enum Block {
    /// A loop's body (`do {`, `while (…) {`, `for (…) {`).
    Loop,
    /// One branch of an `if` chain; `last` for its `else {`, `exits` once it has a `return` or `goto` of
    /// its own (it does not go on past the chain).
    Branch { last: bool, exits: bool },
    Other,
}

/// An `if` / `else if` / `else` chain being read.
struct Chain {
    /// The constants before it.
    entry: HashMap<String, i64>,
    /// Each branch's constants at its end so far, in order; `None` for one that does not go on.
    ends: Vec<Option<HashMap<String, i64>>>,
    /// When its `if` tests a reader's success: whether its first branch is the way it succeeded.
    succeeds: Option<bool>,
}

/// The type words of a cast `(int)` the decomp puts on a reader's result.
const CAST_TYPES: [&str; 8] = ["int", "uint", "long", "ulong", "longlong", "ulonglong", "short", "ushort"];

/// Whether `cond` tests a reader's success: `Some(true)` when it holds as the reader succeeded
/// (`0 < x`), `Some(false)` when it holds as it failed (`x < 1`), for a local `x` a reader's call gave
/// (`read`), or a local holding such a test (`tests`), or its negation. The test may be the last of a
/// comma expression (`(x = FUN_…(…), x < 1)`), and one term of an `||` chain of a failure test (the
/// chain is false only as the reader succeeded) or of an `&&` chain of a success test (true only as it
/// succeeded).
fn reader_test(cond: &[Tok], read: &HashSet<String>, tests: &HashMap<String, bool>) -> Option<bool> {
    let mut t: Vec<&Tok> = Vec::new();
    let mut k = 0;
    while k < cond.len() {
        let cast = matches!(cond[k], Tok::Punct("("))
            && matches!(cond.get(k + 1), Some(Tok::Ident(w)) if CAST_TYPES.contains(&w.as_str()))
            && matches!(cond.get(k + 2), Some(Tok::Punct(")")));
        if cast {
            k += 3;
            continue;
        }
        t.push(&cond[k]);
        k += 1;
    }
    let t = unwrap_parens(&t);
    let ors = split_top(t, "||");
    let ands = split_top(t, "&&");
    let (terms, way) = match (ors.len(), ands.len()) {
        (1, 1) => return one_test(t, read, tests),
        (_, 1) => (ors, false),
        (1, _) => (ands, true),
        _ => return None,
    };
    let found: Vec<bool> = terms.iter().filter_map(|term| one_test(term, read, tests)).collect();
    (found.contains(&way) && !found.contains(&!way)).then_some(way)
}

/// [`reader_test`] of one term.
fn one_test(term: &[&Tok], read: &HashSet<String>, tests: &HashMap<String, bool>) -> Option<bool> {
    let parts = split_top(unwrap_parens(term), ",");
    match unwrap_parens(parts.last().expect("split_top gives a part")) {
        [Tok::Ident(x), Tok::Punct("<"), Tok::Num(Some(1), _)] if read.contains(x) => Some(false),
        [Tok::Num(Some(0), _), Tok::Punct("<"), Tok::Ident(x)] if read.contains(x) => Some(true),
        [Tok::Ident(b)] => tests.get(b).copied(),
        [Tok::Punct("!"), Tok::Ident(b)] => tests.get(b).map(|v| !v),
        _ => None,
    }
}

/// `t` without the parentheses around the whole of it.
fn unwrap_parens<'a, 'b>(mut t: &'a [&'b Tok]) -> &'a [&'b Tok] {
    while t.len() > 2 && matches!(t[0], Tok::Punct("(")) && split_top(&t[1..], ")").first().map(|p| p.len()) == Some(t.len() - 2) {
        t = &t[1..t.len() - 1];
    }
    t
}

/// `t` split at its `sep` tokens outside parentheses and brackets (a `)` or `]` past the start closes
/// nothing of `t`, so it splits as `sep` when that is `)`).
fn split_top<'a, 'b>(t: &'a [&'b Tok], sep: &str) -> Vec<&'a [&'b Tok]> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (k, x) in t.iter().enumerate() {
        match x {
            Tok::Punct(p) if depth == 0 && *p == sep => {
                out.push(&t[start..k]);
                start = k + 1;
            }
            Tok::Punct("(") | Tok::Punct("[") => depth += 1,
            Tok::Punct(")") | Tok::Punct("]") => depth -= 1,
            _ => {}
        }
    }
    out.push(&t[start..]);
    out
}

/// The constants every one of `ends` agrees on.
fn agreed(mut ends: Vec<HashMap<String, i64>>) -> HashMap<String, i64> {
    let Some(mut out) = ends.pop() else { return HashMap::new() };
    out.retain(|k, v| ends.iter().all(|e| e.get(k) == Some(v)));
    out
}

fn fun_va(w: &str) -> Option<u32> {
    w.strip_prefix("FUN_").and_then(|h| u32::from_str_radix(h, 16).ok())
}

/// The reads of one binding's decomp (see the module documentation).
pub fn binding_reads(decomp: &str, readers: &HashMap<u32, &Reader>) -> Vec<Read> {
    let toks = tokenize(decomp);
    let mut env: HashMap<String, i64> = HashMap::new();
    let mut out: Vec<Read> = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut chains: Vec<Chain> = Vec::new();
    // the locals a reader's call last gave, and those last given a test of one's success
    let mut read: HashSet<String> = HashSet::new();
    let mut tests: HashMap<String, bool> = HashMap::new();
    for i in 0..toks.len() {
        match &toks[i] {
            Tok::Punct("{") => {
                let prev = i.checked_sub(1).map(|j| &toks[j]);
                let head = if matches!(prev, Some(Tok::Punct(")"))) { head_word(&toks, i - 1) } else { None };
                let block = match (prev, head) {
                    (Some(Tok::Ident(w)), _) if w == "do" => Block::Loop,
                    (_, Some(("while" | "for", _))) => Block::Loop,
                    (_, Some(("if", k))) => {
                        // `else if (…) {` goes on with the chain its `else` continues
                        if !matches!(k.checked_sub(1).map(|j| &toks[j]), Some(Tok::Ident(w)) if w == "else") {
                            let succeeds = reader_test(&toks[k + 2..i - 1], &read, &tests);
                            chains.push(Chain { entry: env.clone(), ends: Vec::new(), succeeds });
                        }
                        Block::Branch { last: false, exits: false }
                    }
                    (Some(Tok::Ident(w)), _) if w == "else" => Block::Branch { last: true, exits: false },
                    _ => Block::Other,
                };
                blocks.push(block);
                continue;
            }
            Tok::Punct("}") => {
                // A branch starts from the constants before its chain. After the chain: the way a
                // reader succeeded, when its `if` tests that; else a local keeps a constant only when
                // every way through it that goes on — and past it, without an `else` — agrees.
                if let Some(Block::Branch { last, exits }) = blocks.pop() {
                    let chain = chains.last_mut().expect("a branch opened its chain");
                    let end = std::mem::replace(&mut env, chain.entry.clone());
                    chain.ends.push((!exits).then_some(end));
                    if last || !matches!(toks.get(i + 1), Some(Tok::Ident(w)) if w == "else") {
                        let Chain { entry, mut ends, succeeds } = chains.pop().expect("a branch opened its chain");
                        let past = (!last).then_some(entry);
                        let ways: Vec<HashMap<String, i64>> = match succeeds {
                            Some(true) if ends[0].is_some() => vec![ends.swap_remove(0).expect("checked")],
                            Some(false) => ends.into_iter().skip(1).flatten().chain(past).collect(),
                            _ => ends.into_iter().flatten().chain(past).collect(),
                        };
                        env = agreed(ways);
                    }
                }
                continue;
            }
            _ => {}
        }
        let Tok::Ident(w) = &toks[i] else { continue };
        if w == "return" || w == "goto" {
            if let Some(Block::Branch { exits, .. }) = blocks.last_mut() {
                *exits = true;
            }
        }
        // `local = <expr>`: the local holds the expression's constant, or nothing
        if matches!(toks.get(i + 1), Some(Tok::Punct("="))) && !matches!(i.checked_sub(1).map(|j| &toks[j]), Some(Tok::Punct("*")) | Some(Tok::Punct("."))) {
            let end = expr_end(&toks, i + 2);
            let rhs = &toks[i + 2..end];
            let reader_call = matches!(rhs, [Tok::Ident(f), Tok::Punct("("), .., Tok::Punct(")")] if fun_va(f).is_some_and(|v| readers.contains_key(&v)));
            let value = match rhs {
                [Tok::Ident(f), ..] if reader_call => match readers[&fun_va(f).expect("checked")].slots {
                    Slots::Fixed(n) => Some(n),
                    Slots::OneOrTwo => Some(2),
                },
                _ => eval(rhs, &env),
            };
            if reader_call {
                read.insert(w.clone());
            } else {
                read.remove(w);
            }
            match reader_test(rhs, &read, &tests) {
                Some(t) => {
                    tests.insert(w.clone(), t);
                }
                None => {
                    tests.remove(w);
                }
            }
            match value {
                Some(v) => {
                    env.insert(w.clone(), v);
                }
                None => {
                    env.remove(w);
                }
            }
        }
        let Some(r) = fun_va(w).and_then(|v| readers.get(&v)) else { continue };
        if !matches!(toks.get(i + 1), Some(Tok::Punct("("))) {
            continue;
        }
        let Some(args) = call_args(&toks, i + 1) else { continue };
        // In a loop the position changes from pass to pass: the text alone does not give it.
        let looped = blocks.iter().any(|b| matches!(b, Block::Loop));
        let position = match r.index {
            At::Position(p) => Some(p),
            At::Arg(_) if looped => None,
            At::Arg(k) => args.get(k).and_then(|&(s, e)| eval(&toks[s..e], &env)),
        };
        if position.is_some_and(|p| p <= 0) {
            continue;
        }
        let read = Read { position, reads: r.reads.clone(), slots: r.slots, reader: r.va, via: None };
        if !out.contains(&read) {
            out.push(read);
        }
    }
    out
}

/// The reads of every binding of an image: (Lua path, function, reads), by Lua path.
pub fn image_reads(
    luareg: &std::collections::BTreeMap<(String, String), std::collections::BTreeSet<u64>>,
    functions: &Functions,
    readers: &[&Reader],
) -> Result<Vec<(String, u32, Vec<Read>)>, Error> {
    let by_va: HashMap<u32, &Reader> = readers.iter().map(|r| (r.va, *r)).collect();
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for ((namespace, name), fns) in luareg {
        if namespace.is_empty() {
            continue;
        }
        for &f in fns {
            let f = f as u32;
            let path = format!("{namespace}.{name}");
            match functions.decomp(f) {
                Some(d) => {
                    let mut reads = binding_reads(d, &by_va);
                    // the functions it hands its Lua state to, breadth first
                    let mut seen = vec![f];
                    let mut level = delegates(d, &by_va);
                    for _ in 0..DELEGATE_DEPTH {
                        let mut next = Vec::new();
                        for g in level {
                            if seen.contains(&g) {
                                continue;
                            }
                            seen.push(g);
                            let Some(dd) = functions.decomp(g) else { continue };
                            for mut r in binding_reads(dd, &by_va) {
                                r.via = Some(g);
                                if !reads.iter().any(|x| x.position == r.position && x.reads == r.reads && x.reader == r.reader) {
                                    reads.push(r);
                                }
                            }
                            next.extend(delegates(dd, &by_va));
                        }
                        level = next;
                    }
                    out.push((path, f, reads));
                }
                None if functions.has(f) => missing.push(format!("{path} 0x{f:08x} (no decomp)")),
                None => missing.push(format!("{path} 0x{f:08x} (no function)")),
            }
        }
    }
    if !missing.is_empty() {
        return Err(Error::new(format!("{} bindings cannot be read: {}", missing.len(), missing.join(", "))));
    }
    Ok(out)
}

/// Whether another read of the binding reads `r`'s position as something else (`any` excepted): the
/// binding moved values on the stack between the reads (a removal, an insertion), which the text does
/// not show, so the positions cannot both be its arguments'.
fn conflicts(reads: &[Read], r: &Read) -> bool {
    r.position.is_some() && r.reads != "any" && reads.iter().any(|x| x.position == r.position && x.reads != r.reads && x.reads != "any")
}

/// The output file (see the module documentation).
pub fn to_tsv(image_id: &str, sha256: &str, readers: &[&Reader], bindings: &[(String, u32, Vec<Read>)]) -> String {
    let mut out = String::from(
        "# The arguments each Lua binding reads, written by `exemap binding-args` (mercs2_exemap::bindingargs)\n\
         # from the image's luareg export and Ghidra functions export, through mercs2_exemap's data/lua_readers.tsv.\n",
    );
    let read = bindings.iter().filter(|(_, _, r)| !r.is_empty()).count();
    let computed = bindings.iter().filter(|(_, _, r)| r.iter().any(|x| x.position.is_none())).count();
    out.push_str(&format!(
        "# image {image_id} sha256 {sha256}: {} bindings, {read} read arguments, {computed} with a computed position; {} readers\n",
        bindings.len(),
        readers.len()
    ));
    out.push_str(ARGS_HEADER);
    out.push('\n');
    for (path, f, reads) in bindings {
        if reads.is_empty() {
            out.push_str(&format!("{path}\t0x{f:08x}\t-\t(none)\t-\t-\t-\t-\n"));
        }
        for r in reads {
            out.push_str(&format!(
                "{path}\t0x{f:08x}\t{}\t{}\t{}\t0x{:08x}\t{}\t{}\n",
                r.position.map_or("computed".into(), |p| p.to_string()),
                r.reads,
                r.slots.text(),
                r.reader,
                r.via.map_or("-".into(), |v| format!("0x{v:08x}")),
                if conflicts(reads, r) { "yes" } else { "-" }
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readers() -> Vec<Reader> {
        let r = |va, index, slots, reads: &str| Reader { image_id: "t".into(), va, index, slots, reads: reads.into(), evidence: "-".into() };
        vec![
            r(0x823084a8, At::Arg(1), Slots::Fixed(1), "string"),
            r(0x82308320, At::Arg(1), Slots::Fixed(1), "integer"),
            r(0x82308620, At::Arg(1), Slots::Fixed(3), "vector3"),
            r(0x82308e90, At::Arg(2), Slots::OneOrTwo, "callback"),
            r(0x82308390, At::Position(1), Slots::Fixed(1), "string"),
        ]
    }

    #[test]
    fn positions_follow_the_slots_each_reader_takes() {
        let rs = readers();
        let by: HashMap<u32, &Reader> = rs.iter().map(|r| (r.va, r)).collect();
        // SetColorValue's shape: a string, then four integers each after the one before.
        let decomp = "undefined8 f(undefined4 param_1) {\n  iVar1 = FUN_823084a8(local_40,1,&local_2c);\n  if (0 < iVar1) {\n    \
                      iVar4 = FUN_82308320(local_40,(long)(iVar1 + 1),auStack_30);\n    if (0 < iVar4) {\n      iVar4 = iVar1 + 1 + iVar4;\n      \
                      iVar3 = FUN_82308320(local_40,(long)iVar4,auStack_34);\n      iVar2 = FUN_82308620(local_40,iVar4 + iVar3,auStack_38);\n      \
                      FUN_82308320(local_40,0xffffffffffffffff,x);\n      FUN_82308e90(local_40,&cb,iVar4 + iVar3 + iVar2);\n    }\n  }\n}";
        let reads = binding_reads(decomp, &by);
        let got: Vec<(Option<i64>, &str)> = reads.iter().map(|r| (r.position, r.reads.as_str())).collect();
        assert_eq!(got, vec![(Some(1), "string"), (Some(2), "integer"), (Some(3), "integer"), (Some(4), "vector3"), (Some(7), "callback")]);
    }

    #[test]
    fn the_branches_of_an_if_are_alternatives_and_agree_or_leave_the_position_unknown() {
        let rs = readers();
        let by: HashMap<u32, &Reader> = rs.iter().map(|r| (r.va, r)).collect();
        // SetWidgetColor's shape (x360 0x8231ce78): optional integers whose `if` and `else` both step
        // the next position by one, then a chain whose branches disagree.
        let decomp = "void f(void) {\n  lVar6 = FUN_823084a8(&s,1,a);\n  lVar7 = FUN_82308320(&s,lVar6 + 1,b);\n  \
                      if (bVar1) {\n    lVar7 = lVar6 + 2;\n  }\n  else {\n    lVar7 = lVar7 + lVar6 + 1;\n  }\n  \
                      if (lVar7 < 1) {\n    lVar7 = 0;\n    FUN_82206f20(&s);\n    return;\n  }\n  \
                      lVar6 = FUN_82308320(&s,lVar7,c);\n  if (x) {\n    lVar6 = lVar7 + 1;\n  }\n  else {\n    lVar6 = lVar6 + lVar7;\n  }\n  \
                      FUN_82308320(&s,lVar6,d);\n  if (y) {\n    lVar6 = 9;\n  }\n  else if (z) {\n    lVar6 = 9;\n  }\n  \
                      FUN_82308320(&s,lVar6,e);\n  if (w) {\n    lVar6 = 1;\n  }\n  FUN_823084a8(&s,lVar6,g);\n}";
        let reads = binding_reads(decomp, &by);
        let got: Vec<(Option<i64>, &str)> = reads.iter().map(|r| (r.position, r.reads.as_str())).collect();
        assert_eq!(
            got,
            vec![(Some(1), "string"), (Some(2), "integer"), (Some(3), "integer"), (Some(4), "integer"), (None, "integer"), (None, "string")],
            "both branches give 3 (the branch that returns does not go on), then 4; an `else if` with no `else` and an `if` alone leave a way past that disagrees"
        );
    }

    #[test]
    fn the_full_form_takes_the_way_each_reader_succeeded() {
        let rs = readers();
        let by: HashMap<u32, &Reader> = rs.iter().map(|r| (r.va, r)).collect();
        // Airstrike.Flyby's shape (x360 0x823158d0): a string or else an integer first, or neither;
        // then EquipDesignator's (0x823156e0): a callback with its args table, skipped two when absent;
        // then SetWidgetColor's test held in a local.
        let decomp = "void f(void) {\n  lVar6 = 1;\n  lVar1 = FUN_823084a8(&s,1,a);\n  if ((int)lVar1 < 1) {\n    \
                      lVar1 = FUN_82308320(&s,1,b);\n    if ((int)lVar1 < 1) {\n      x = 0;\n    }\n    else {\n      lVar6 = lVar1 + 1;\n    }\n  }\n  \
                      else {\n    lVar6 = lVar1 + 1;\n  }\n  lVar2 = FUN_82308320(&s,lVar6,c);\n  if (0 < (int)lVar2) {\n    \
                      lVar2 = lVar2 + lVar6;\n    lVar1 = FUN_82308e90(&s,&cb,lVar2);\n    if ((int)lVar1 < 1) {\n      lVar1 = lVar2 + 2;\n    }\n    \
                      else {\n      lVar1 = lVar1 + lVar2;\n    }\n    lVar7 = FUN_82308320(&s,lVar1,d);\n    bVar1 = (int)lVar7 < 1;\n    \
                      if (bVar1) {\n      lVar7 = 9;\n    }\n    else {\n      lVar7 = lVar7 + lVar1;\n    }\n    FUN_823084a8(&s,lVar7,e);\n  }\n}";
        let reads = binding_reads(decomp, &by);
        let got: Vec<(Option<i64>, &str)> = reads.iter().map(|r| (r.position, r.reads.as_str())).collect();
        assert_eq!(
            got,
            vec![(Some(1), "string"), (Some(1), "integer"), (Some(2), "integer"), (Some(3), "callback"), (Some(5), "integer"), (Some(6), "string")],
            "the string given, the callback with its table, the integer given"
        );
    }

    #[test]
    fn a_reader_test_in_an_or_or_and_chain_takes_the_way_it_succeeded_and_a_mixed_one_does_not() {
        let rs = readers();
        let by: HashMap<u32, &Reader> = rs.iter().map(|r| (r.va, r)).collect();
        // VO.Cancel's shape (x360 0x82349230): an optional first argument read inside an `||` chain;
        // then a success test in an `&&` chain without an `else`; then a chain testing both ways.
        let decomp = "void f(void) {\n  lVar9 = 1;\n  cVar5 = FUN_82251f58(&s,1,0);\n  \
                      if ((cVar5 == '\\0') || (lVar2 = FUN_823084a8(&s,1,a), (int)lVar2 < 1)) {\n    u = 0;\n  }\n  \
                      else {\n    lVar9 = lVar2 + 1;\n  }\n  lVar2 = FUN_82308320(&s,lVar9,b);\n  \
                      if ((c != 0) && (0 < (int)lVar2)) {\n    lVar9 = lVar2 + lVar9;\n  }\n  FUN_82308320(&s,lVar9,c);\n  \
                      lVar3 = FUN_823084a8(&s,lVar9,d);\n  if ((int)lVar3 < 1 || 0 < (int)lVar3) {\n    lVar9 = 7;\n  }\n  \
                      else {\n    lVar9 = 8;\n  }\n  FUN_82308320(&s,lVar9,e);\n}";
        let reads = binding_reads(decomp, &by);
        let got: Vec<(Option<i64>, &str)> = reads.iter().map(|r| (r.position, r.reads.as_str())).collect();
        assert_eq!(
            got,
            vec![(Some(1), "string"), (Some(2), "integer"), (Some(3), "integer"), (Some(3), "string"), (None, "integer")],
            "the `||` chain is false and the `&&` chain true as their readers succeed; a chain testing both ways gives neither"
        );
    }

    #[test]
    fn a_read_in_a_loop_is_computed_and_a_top_relative_one_is_no_argument() {
        let rs = readers();
        let by: HashMap<u32, &Reader> = rs.iter().map(|r| (r.va, r)).collect();
        // SetAmbientCube's shape: one reader called once per pass.
        let decomp = "void f(void) {\n  iVar5 = 1;\n  do {\n    iVar3 = FUN_82308620(&param_1,iVar5,&local_c);\n    iVar5 = iVar5 + iVar3;\n  } while (x);\n  \
                      while (y) {\n    FUN_82308320(p,2,z);\n  }\n  FUN_823084a8(p,-1,w);\n  FUN_82308320(p,5,v);\n  FUN_82308390(p,9,u);\n}";
        let reads = binding_reads(decomp, &by);
        let got: Vec<(Option<i64>, &str)> = reads.iter().map(|r| (r.position, r.reads.as_str())).collect();
        assert_eq!(got, vec![(None, "vector3"), (None, "integer"), (Some(5), "integer"), (Some(1), "string")], "a fixed reader reads its own position");
    }

    #[test]
    fn a_binding_that_hands_its_state_on_reads_through_that_function_and_a_shifted_position_conflicts() {
        let rs = readers();
        let refs: Vec<&Reader> = rs.iter().collect();
        // Event.Create's shape: the binding calls a shared implementation with its state and a mode.
        let record = |va: &str, decomp: &str| {
            serde_json::json!({"kind": "function", "va": va, "name": "f", "size": 16, "decomp": decomp, "callers": []}).to_string()
        };
        let jsonl = [
            record("0x00001000", "void f(undefined8 param_1) {\n  FUN_00002000(param_1,0);\n  FUN_00003000(local_10,1);\n}"),
            record(
                "0x00002000",
                "void g(undefined8 param_1,undefined8 param_2) {\n  iVar1 = FUN_82308320(&s,1,x);\n  FUN_823084a8(&s,iVar1 + 1,y);\n  FUN_82308620(&s,1,z);\n}",
            ),
            record("0x00003000", "void h(void) {\n  FUN_823084a8(&s,5,y);\n}"),
        ]
        .join("\n");
        let fns = Functions::parse(&jsonl, "test").expect("parses");
        let mut reg = std::collections::BTreeMap::new();
        reg.insert(("Event".to_string(), "Create".to_string()), std::collections::BTreeSet::from([0x1000u64]));
        let got = image_reads(&reg, &fns, &refs).expect("reads");
        let reads = &got[0].2;
        let rows: Vec<(Option<i64>, &str, Option<u32>, bool)> =
            reads.iter().map(|r| (r.position, r.reads.as_str(), r.via, conflicts(reads, r))).collect();
        assert_eq!(
            rows,
            vec![
                (Some(1), "integer", Some(0x2000), true),
                (Some(2), "string", Some(0x2000), false),
                (Some(1), "vector3", Some(0x2000), true),
            ],
            "followed into 0x2000, not into 0x3000 (handed no state); position 1 read two ways"
        );
    }

    #[test]
    fn the_embedded_readers_table_parses() {
        let r = Readers::embedded().expect("parses");
        assert!(r.of("x360-retail").any(|x| x.va == 0x823082b8 && x.reads == "number"));
        let e = Readers::parse(&format!("{READERS_HEADER}\nt\t0x00000001\t1\t0\tnumber\tx\n")).unwrap_err().to_string();
        assert!(e.contains("not a positive number"), "{e}");
    }
}
