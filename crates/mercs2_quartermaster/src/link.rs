//! The Lua linker — the thing that lets two script mods coexist.
//!
//! Script entries load from the **block**, not per-hash, so editing one script means re-emitting
//! every script in that block. Under one-overlay-per-Shipment plus last-mounted-wins, two Shipments
//! that each ship a finished `scripts_vz` do not merge and do not error: the later one wins and the
//! earlier one's Lua vanishes — model, wardrobe row and all — with nothing reported. That single
//! failure is why `patch_lua` declares a *mutation* instead of shipping a block, and why this module
//! exists.
//!
//! Targets resolve across every block in [`SCRIPT_BLOCKS`] — `scripts_vz` and `resident` — because
//! the framework modules most worth patching (`mrxplayer`, `mrxguipda`, the `MrxTask*` family) are
//! resident, not `vz`.
//!
//! So linking happens across the **installed set**, not per build:
//!
//! ```text
//!   base script source (vendored corpus)
//!     + each Shipment's `append:` source, in a deterministic order
//!     -> mercs2_luac::compile
//!     -> splice into the block  -> one scripts_vz for everyone
//! ```
//!
//! Our own field guide reached the same conclusion independently, from the modder's side: "N mods
//! union by plain text concatenation, compiled once. That is why exactly one thing must own
//! `scripts_vz`."
//!
//! ## Two facts this depends on, both measured rather than assumed
//!
//! - **The chunk name must be the bare script name** — no `@`, no `.lua`. Retail's LuaQ headers
//!   store it verbatim, and `mercs2_luac/tests/parity.rs` found this by way of all 113 scripts
//!   differing from retail by a constant 5 bytes (`@` + `.lua`) until it was corrected.
//! - **No retail `scripts_vz` container carries metadata after its bytecode**, so `replace_lua`'s
//!   refusal to touch such a container never fires here — surveyed across all 114 in
//!   `mercs2_formats/tests/scripts_block_survey.rs`, which also pins that a no-op splice reproduces
//!   the block byte for byte.
//!
//! ## Path-in, like everything else here
//!
//! The base sources come from the vendored decompiled corpus, taken as a **path**. That is partly
//! the crate's standing discipline and partly forced: the crate that owns the corpus
//! (`mercs2_script`) links a second, incompatible Lua runtime — see the note in `Cargo.toml`.

use mercs2_formats::scripts_block::ScriptsBlock;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Why `name` cannot be a bare chunk name, or `None` when it can.
///
/// Retail's LuaQ headers store the bare script name (the module note above): no `@` prefix and no
/// `.lua` suffix. Lua 5.1 treats a leading `@` (a file name) and a leading `=` (a literal) as sigils
/// when it prints a chunk name, so neither may start one. A name is also one path component, never
/// empty, and carries no NUL. `qm compile-lua` checks the chunk name it compiles under with this.
pub fn chunk_name_refusal(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("it is empty".into());
    }
    if name.starts_with('@') || name.starts_with('=') {
        return Some(format!(
            "it starts with {:?}, which Lua reads as a chunk-name sigil; retail chunk names are the \
             bare script name",
            &name[..1]
        ));
    }
    if name.to_ascii_lowercase().ends_with(".lua") {
        return Some(
            "it ends in `.lua`; retail chunk names are the bare script name, with no extension".into(),
        );
    }
    if name.contains('\0') {
        return Some("it contains a NUL byte".into());
    }
    if name.contains('/') || name.contains('\\') {
        return Some("it contains a path separator; a chunk name is one bare name".into());
    }
    None
}

/// A level WAD whose scripts the linker edits: `vz.wad` (gameplay) or `shell.wad` (the front end).
///
/// The two never share a mount slot (`fixpack/wad_duplicate_inventory.md` §B.5), and each level runs
/// its own Lua VM, so a script is edited per level and one level's edit never reaches the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Vz,
    Shell,
}

impl Level {
    /// The level whose Lua VM runs `session`.
    pub const fn of(session: crate::manifest::LoadSession) -> Level {
        match session {
            crate::manifest::LoadSession::Gameplay => Level::Vz,
            crate::manifest::LoadSession::FrontEnd => Level::Shell,
        }
    }

    /// The level's WAD, for messages.
    pub const fn wad(self) -> &'static str {
        match self {
            Level::Vz => "vz.wad",
            Level::Shell => "shell.wad",
        }
    }
}

/// Every block a `patch_lua` target may live in, as `(PTHS needle, PTHS path)`.
///
/// Game scripts are split across blocks: `scripts_vz` holds the 114 content scripts (contracts,
/// jobs, tutorials) and `resident` holds the ~240 framework modules (`Mrx*`, world-entity scripts)
/// that are always loaded. A fix targeting `mrxplayer` is unreachable without the second.
///
/// Searched in order, so a name present in both resolves to `scripts_vz`. No shipped script name
/// appears in both, and the type-aware lookup makes a cross-type collision impossible; the order is
/// fixed so the outcome stays deterministic if that ever stops being true.
///
/// ⚠ **The resident needle is ANCHORED on purpose, folder included.** `block_by_path` matches a
/// substring over the stack from the top, and `resident_P000_Q3` alone also matches
/// `sound_resident_P000_Q3.block` — a completely different block — while `\resident_P000_Q3.block`
/// also matches `English.wad`'s `blocks\English\resident_P000_Q3.block`, which sits above `vz.wad`
/// in a stack that holds a language WAD. The front end's scripts are in `shell.wad`, which never
/// shares a mount slot with `vz.wad`; they are [`SHELL_SCRIPT_BLOCKS`], read from `shell.wad` and
/// shipped in the shell patch.
pub const SCRIPT_BLOCKS: &[(&str, &str)] = &[
    ("scripts_vz", r"blocks\VZ\scripts_vz_P000_Q3.block"),
    (r"\VZ\resident_P000_Q3.block", r"blocks\VZ\resident_P000_Q3.block"),
];

/// The front end's scripts block in `shell.wad`, as `(PTHS needle, PTHS path)`: the one block of
/// the 28 shell scripts, `MrxSound` among them.
pub const SHELL_SCRIPT_BLOCKS: &[(&str, &str)] =
    &[(r"\Shell\resident_P000_Q3.block", r"blocks\Shell\resident_P000_Q3.block")];

/// The corpus subfolders the `vz.wad` blocks' base sources are searched in, in order.
pub const VZ_SOURCE_DIRS: &[&str] = &["vz", "resident", "shell"];

/// The corpus subfolders the `shell.wad` block's base sources are searched in: the shell's own
/// copies, which differ from the resident ones for some scripts (`mrxgui`).
pub const SHELL_SOURCE_DIRS: &[&str] = &["shell"];

/// One Shipment's declared edit to one script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptMutation {
    /// Which Shipment asked for it — the tie-break for ordering, and what a conflict names.
    pub shipment: String,
    /// Base script, e.g. `wifpmcinterior`.
    pub target: String,
    /// Source text appended after the base. Not bytecode: the whole point is that N appends
    /// concatenate and compile once.
    pub append: String,
}

/// One Shipment's declaration of a NOVEL script module — a whole new `import`-able Lua module
/// that mints its own `scripts_vz` entry and ASET row. Compiled to LuaQ at link time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptAddition {
    pub shipment: String,
    /// The module name (`import("<name>")` / `dynamic_import("<name>")` / `sModuleName`). Bare,
    /// no extension. Hashed with `pandemic_hash_m2` to become the ASET row's key.
    pub name: String,
    /// Full Lua source text to compile.
    pub source: String,
}

/// One Shipment's declaration of a wholesale REPLACE on an existing script's bytecode.
/// Same asset hash, new body; the last-mounted replace wins if two Shipments claim one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptReplacement {
    pub shipment: String,
    /// The shipped script to replace, e.g. `wifpmcinterior`.
    pub target: String,
    /// Full Lua source text that becomes the new bytecode.
    pub source: String,
}

#[derive(Debug)]
pub enum LinkError {
    /// The base script is not in the block being linked.
    UnknownScript {
        target: String,
        shipment: String,
    },
    /// The base script has no source in the corpus, so there is nothing to append to.
    ///
    /// Real and expected for some targets: the corpus covers 370 of 382 scripts, and the gaps are
    /// modules `unluac` could not round-trip. Those are structurally un-linkable, and saying so is
    /// better than emitting a block that silently drops the mod.
    NoBaseSource {
        target: String,
        tried: Vec<PathBuf>,
    },
    Compile {
        target: String,
        message: String,
    },
    Splice {
        target: String,
        message: String,
    },
    /// A contributor that is not in the resolved `order`. Every real contributor comes from a
    /// Shipment in the set, so this is an internal error in whatever built `order`, never an
    /// author's mistake — and never something to paper over with a fallback position.
    NotInOrder {
        shipment: String,
    },
    Block(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::UnknownScript { target, shipment } => write!(
                f,
                "{shipment} patches {target:?}, which is not in this scripts block — check the \
                 spelling; a name that does not exist simply misses"
            ),
            LinkError::NoBaseSource { target, tried } => write!(
                f,
                "no decompiled source for {target:?}, so there is nothing to append to (tried: {}). \
                 The corpus covers 370 of 382 scripts; the gaps are modules the decompiler could not \
                 round-trip, and they cannot be linked",
                tried.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
            LinkError::Compile { target, message } => {
                write!(f, "compiling the linked {target:?}: {message}")
            }
            LinkError::Splice { target, message } => {
                write!(f, "splicing {target:?} back into the block: {message}")
            }
            LinkError::NotInOrder { shipment } => write!(
                f,
                "internal error: {shipment} contributes to the link but is not in the resolved load \
                 order — the order must name every Shipment in the set"
            ),
            LinkError::Block(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// What the link produced, for the build log and for diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedScript {
    pub target: String,
    /// Shipments that contributed, in the order their source was concatenated.
    pub contributors: Vec<String>,
    pub base_source_bytes: usize,
    pub linked_source_bytes: usize,
    pub bytecode_bytes: usize,
    /// Index into the `blocks` slice handed to [`link_into_blocks`] — which block this script was
    /// spliced into, so the caller emits only the blocks that actually changed.
    pub block: usize,
}

/// A literal `import("x")` whose `x` nothing in the link provides (M0209, a warning).
///
/// Resolved means: after every script is linked and every module minted, some loaded block holds a
/// script named `x` — a shipped script, an `add_script` module, a minted support module — or `x`
/// is `qm_modloader`. Anything else will fail at runtime unless something provides it by other
/// means, which the linker cannot see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedImport {
    /// The Shipment whose source carries the import.
    pub shipment: String,
    /// The module name inside the literal.
    pub module: String,
    /// Which source it is in, for the message: `patch_lua append to <target>`, `add_script <name>`,
    /// `replace_lua <target>`, or `support module <name>`.
    pub source: String,
}

impl std::fmt::Display for UnresolvedImport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: import({:?}) in its {} names no script in the linked blocks, no add_script in \
             the set and no module the Quartermaster mints, so it will fail at runtime unless \
             something provides it another way",
            self.shipment, self.module, self.source
        )
    }
}

/// What [`link_into_blocks`] produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkOutput {
    /// Every script linked, replaced or minted.
    pub scripts: Vec<LinkedScript>,
    /// Literal imports nothing provides (M0209), in the order they were found.
    pub unresolved_imports: Vec<UnresolvedImport>,
}

/// Every literal `import("x")` / `import('x')` in `source`: the `x` of each call to the global
/// `import` whose only argument is one short string literal.
///
/// A small Lua 5.1 lexer, so that comments and string contents are never read as code: `--` line
/// and `--[[ ]]` / `--[==[ ]==]` long comments, `"…"` / `'…'` short strings with their escapes, and
/// `[[ ]]` / `[==[ ]==]` long strings. A call counts only when `import` is not a field or method
/// (`t.import(…)`, `t:import(…)`) and is not being defined (`function import(…)`).
///
/// **Not checked, by design:** `dynamic_import(…)`, `import(<expression>)`, a concatenated name
/// (`import("a" .. b)`), the call forms without parentheses (`import "x"`), and module names reached
/// through data such as a task's `sModuleName`. Those are resolved at runtime and the linker cannot
/// see them.
pub fn literal_imports(source: &str) -> Vec<String> {
    let tokens = lua_tokens(source);
    let mut out = Vec::new();
    for i in 0..tokens.len() {
        if tokens[i] != LuaToken::Name("import".into()) {
            continue;
        }
        let before = i.checked_sub(1).map(|p| &tokens[p]);
        let is_field_or_definition = matches!(
            before,
            Some(LuaToken::Punct(".")) | Some(LuaToken::Punct(":"))
        ) || before == Some(&LuaToken::Name("function".into()));
        if is_field_or_definition {
            continue;
        }
        if let (Some(LuaToken::Punct("(")), Some(LuaToken::ShortString(module)), Some(LuaToken::Punct(")"))) =
            (tokens.get(i + 1), tokens.get(i + 2), tokens.get(i + 3))
        {
            out.push(module.clone());
        }
    }
    out
}

/// A Lua token, as far as [`literal_imports`] needs one.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LuaToken {
    Name(String),
    /// A `"…"` or `'…'` string, escapes decoded.
    ShortString(String),
    /// A `[[…]]` string.
    LongString,
    Number,
    Punct(&'static str),
}

/// The level of a long bracket opening at `b[i]` (`[[` is 0, `[==[` is 2), or `None`.
fn long_bracket_level(b: &[u8], i: usize) -> Option<usize> {
    if b.get(i) != Some(&b'[') {
        return None;
    }
    let mut j = i + 1;
    while b.get(j) == Some(&b'=') {
        j += 1;
    }
    (b.get(j) == Some(&b'[')).then_some(j - i - 1)
}

/// The index just past the long bracket close `]=*]` of `level` at or after `from`, or the end of
/// the input when it is unterminated (the compiler reports that; here it only has to stop).
fn long_bracket_end(b: &[u8], from: usize, level: usize) -> usize {
    let mut k = from;
    while k < b.len() {
        if b[k] == b']' {
            let mut j = k + 1;
            while b.get(j) == Some(&b'=') {
                j += 1;
            }
            if j - k - 1 == level && b.get(j) == Some(&b']') {
                return j + 1;
            }
        }
        k += 1;
    }
    b.len()
}

fn lua_tokens(source: &str) -> Vec<LuaToken> {
    const PUNCT: &[&str] = &[
        "...", "..", "==", "~=", "<=", ">=", "(", ")", "{", "}", "[", "]", ";", ":", ",", ".", "+",
        "-", "*", "/", "%", "^", "#", "<", ">", "=",
    ];
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'-' && b.get(i + 1) == Some(&b'-') {
            match long_bracket_level(b, i + 2) {
                Some(level) => i = long_bracket_end(b, i + 2 + level + 2, level),
                None => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                }
            }
        } else if let Some(level) = long_bracket_level(b, i) {
            i = long_bracket_end(b, i + level + 2, level);
            out.push(LuaToken::LongString);
        } else if c == b'"' || c == b'\'' {
            let (text, next) = short_string(b, i + 1, c);
            out.push(LuaToken::ShortString(text));
            i = next;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(LuaToken::Name(source[start..i].to_string()));
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            // A numeral, including `0x1F`, `1e-5` and `.5`: letters, digits, `.`, and a sign right
            // after an exponent marker.
            i += 1;
            while i < b.len() {
                let d = b[i];
                let signed_exponent = (d == b'+' || d == b'-') && matches!(b[i - 1], b'e' | b'E');
                if d.is_ascii_alphanumeric() || d == b'.' || signed_exponent {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(LuaToken::Number);
        } else if let Some(p) = PUNCT.iter().find(|p| b[i..].starts_with(p.as_bytes())) {
            out.push(LuaToken::Punct(p));
            i += p.len();
        } else {
            // A byte Lua itself would reject; the compiler reports it. Stepping over it keeps the
            // scan going without inventing a token. Non-ASCII bytes only occur inside strings and
            // comments in valid Lua 5.1, and those are consumed above.
            i += 1;
        }
    }
    out
}

/// Decode a short string whose body starts at `b[start]` and ends at the unescaped `quote`, and
/// return it with the index just past the closing quote. Lua 5.1's escapes: `\a \b \f \n \r \t
/// \v \\ \" \'`, a backslash-newline, and `\ddd` (up to three decimal digits).
fn short_string(b: &[u8], start: usize, quote: u8) -> (String, usize) {
    let mut bytes = Vec::new();
    let mut i = start;
    while i < b.len() && b[i] != quote && b[i] != b'\n' {
        if b[i] == b'\\' && i + 1 < b.len() {
            let e = b[i + 1];
            i += 2;
            match e {
                b'a' => bytes.push(0x07),
                b'b' => bytes.push(0x08),
                b'f' => bytes.push(0x0C),
                b'n' | b'\n' => bytes.push(b'\n'),
                b'r' => bytes.push(b'\r'),
                b't' => bytes.push(b'\t'),
                b'v' => bytes.push(0x0B),
                d if d.is_ascii_digit() => {
                    let mut value = u32::from(d - b'0');
                    let mut n = 1;
                    while n < 3 && i < b.len() && b[i].is_ascii_digit() {
                        value = value * 10 + u32::from(b[i] - b'0');
                        i += 1;
                        n += 1;
                    }
                    bytes.push(value as u8);
                }
                other => bytes.push(other),
            }
        } else {
            bytes.push(b[i]);
            i += 1;
        }
    }
    (String::from_utf8_lossy(&bytes).into_owned(), (i + 1).min(b.len()))
}

/// One candidate scripts block, with the PTHS path the overlay must carry for it.
///
/// Game scripts live in more than one block and a `patch_lua` target may be in any of them, so the
/// linker takes the set and resolves each target against it rather than being told which block to
/// use.
pub struct TargetBlock<'a> {
    /// The block's own PTHS path, e.g. `blocks\VZ\scripts_vz_P000_Q3.block`. Carried through to the
    /// emitted `PatchBlock` so the overlay shadows the right base block.
    pub path: String,
    pub block: &'a mut ScriptsBlock,
}

/// Find a script's decompiled source under the corpus.
///
/// Searched in `subfolders`, in order, then the stub directory. The `vz.wad` blocks pass
/// [`VZ_SOURCE_DIRS`] and the `shell.wad` block [`SHELL_SOURCE_DIRS`]. On a miss, every path tried
/// comes back.
pub fn base_source_path(corpus_root: &Path, subfolders: &[&str], target: &str) -> Result<PathBuf, Vec<PathBuf>> {
    let mut tried = Vec::new();
    for sub in subfolders {
        let p = corpus_root.join(sub).join(format!("{target}.lua"));
        if p.is_file() {
            return Ok(p);
        }
        tried.push(p);
    }
    // `corpus/stubs` sits beside `corpus/mercs2-luacd/src`, not inside it.
    if let Some(corpus_dir) = corpus_root.parent().and_then(|p| p.parent()) {
        let p = corpus_dir.join("stubs").join(format!("{target}.lua"));
        if p.is_file() {
            return Ok(p);
        }
        tried.push(p);
    }
    Err(tried)
}

/// The name the synthetic mod-loader trampoline contributes under. It is not a Shipment, so it is
/// never in a resolved `order`; it sorts after every real contributor.
pub const MODLOADER_CONTRIBUTOR: &str = "quartermaster-modloader";

/// One ordering edge between two request items (indices into the request): `first` loads before
/// `then`. Built from `requires` alone (a provider before its consumer); nothing else orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderEdge {
    pub first: usize,
    pub then: usize,
}

/// A resolved load order over request items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOrder {
    /// Request indices, in load order. Every index appears exactly once.
    pub order: Vec<usize>,
    /// Per request index: the edge that last delayed the item, when an item with a LARGER request
    /// index loaded before it; otherwise `None`.
    pub held_back_by: Vec<Option<usize>>,
}

/// The load order over `n` request items and the `requires` edges between them.
///
/// A Kahn topological sort. Whenever more than one item is ready, the one with the LOWEST request
/// index goes first, so the request order (the player's list) breaks every tie and declared edges
/// always beat it. The result is a function of the request and its edges alone.
///
/// A cycle has no order: the error carries the edges on each cycle (indices into `edges`), one list
/// per strongly connected tangle, so every one can be named rather than an arbitrary pick made.
pub fn resolve_load_order(n: usize, edges: &[OrderEdge]) -> Result<ResolvedOrder, Vec<Vec<usize>>> {
    let mut indeg = vec![0usize; n];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in edges.iter().enumerate() {
        indeg[e.then] += 1;
        outgoing[e.first].push(i);
    }
    let mut ready: BTreeSet<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut position: Vec<Option<usize>> = vec![None; n];
    while let Some(&next) = ready.iter().next() {
        ready.remove(&next);
        position[next] = Some(order.len());
        order.push(next);
        for &ei in &outgoing[next] {
            let t = edges[ei].then;
            indeg[t] -= 1;
            if indeg[t] == 0 {
                ready.insert(t);
            }
        }
    }
    if order.len() != n {
        return Err(cycle_edges(n, edges, &position));
    }

    let mut held_back_by = vec![None; n];
    for (pos, &x) in order.iter().enumerate() {
        if !order[..pos].iter().any(|&y| y > x) {
            continue;
        }
        // X waited on something: an item with a larger request index went first, which only an
        // incoming edge can cause. Name the edge from its last-emitted predecessor (the lowest-index
        // edge when there are several between the same pair).
        let last = edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.then == x)
            .max_by_key(|(i, e)| (position[e.first], std::cmp::Reverse(*i)))
            .map(|(i, _)| i)
            .expect("an item that is held back has at least one incoming edge");
        held_back_by[x] = Some(last);
    }
    Ok(ResolvedOrder {
        order,
        held_back_by,
    })
}

/// The edges on a cycle, grouped by strongly connected component, among the items the sort could
/// not place (`position[i] == None`).
///
/// An unplaced item is on a cycle or downstream of one; only edges whose ends reach each other are
/// cycle edges. Groups are sorted by their lowest item index, edges within a group by index.
fn cycle_edges(n: usize, edges: &[OrderEdge], position: &[Option<usize>]) -> Vec<Vec<usize>> {
    let stuck: Vec<bool> = (0..n).map(|i| position[i].is_none()).collect();
    // reach[a] = every stuck item reachable from `a` through stuck items.
    let reach: Vec<BTreeSet<usize>> = (0..n)
        .map(|a| {
            let mut seen = BTreeSet::new();
            if !stuck[a] {
                return seen;
            }
            let mut stack = vec![a];
            while let Some(u) = stack.pop() {
                for e in edges.iter().filter(|e| e.first == u && stuck[e.then]) {
                    if seen.insert(e.then) {
                        stack.push(e.then);
                    }
                }
            }
            seen
        })
        .collect();
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, e) in edges.iter().enumerate() {
        if stuck[e.first] && stuck[e.then] && reach[e.then].contains(&e.first) {
            // The component's representative: its lowest item index. Both ends of a cycle edge
            // share one component, so either end names it.
            let rep = (0..n)
                .find(|&m| m == e.first || (reach[m].contains(&e.first) && reach[e.first].contains(&m)))
                .expect("e.first is itself a candidate");
            groups.entry(rep).or_default().push(i);
        }
    }
    groups.into_values().collect()
}

/// An item's position in `order`, for sorting what it contributed.
///
/// The synthetic mod-loader trampoline ([`MODLOADER_CONTRIBUTOR`]) sorts after every real
/// contributor. Any other name missing from `order` is [`LinkError::NotInOrder`].
fn order_rank(order: &[String], shipment: &str) -> Result<usize, LinkError> {
    if shipment == MODLOADER_CONTRIBUTOR {
        return Ok(order.len());
    }
    order
        .iter()
        .position(|n| n == shipment)
        .ok_or_else(|| LinkError::NotInOrder {
            shipment: shipment.to_string(),
        })
}

/// Stable-sort `items` by their contributor's position in `order`, keeping each contributor's own
/// relative order, then by `key` within a contributor.
fn sort_by_order<T, K: Ord>(
    items: &mut Vec<&T>,
    order: &[String],
    shipment: impl Fn(&T) -> &str,
    key: impl Fn(&T) -> K,
) -> Result<(), LinkError> {
    let mut ranked = Vec::with_capacity(items.len());
    for it in items.drain(..) {
        ranked.push((order_rank(order, shipment(it))?, it));
    }
    ranked.sort_by(|(ra, a), (rb, b)| ra.cmp(rb).then_with(|| key(a).cmp(&key(b))));
    items.extend(ranked.into_iter().map(|(_, it)| it));
    Ok(())
}

/// Concatenate the base source with every mutation's append, in the resolved load order.
///
/// `order` is the resolved sequence of Shipment names (the load plan's `order`). Appends follow it;
/// one Shipment's own appends keep their manifest order. The synthetic mod-loader trampoline goes
/// last, and any other contributor missing from `order` is [`LinkError::NotInOrder`].
pub fn linked_source(
    base: &str,
    mutations: &[&ScriptMutation],
    order: &[String],
) -> Result<(String, Vec<String>), LinkError> {
    let mut ordered: Vec<&ScriptMutation> = mutations.to_vec();
    sort_by_order(&mut ordered, order, |m| m.shipment.as_str(), |_| ())?;

    let mut out = String::with_capacity(
        base.len() + ordered.iter().map(|m| m.append.len()).sum::<usize>() + 256,
    );
    out.push_str(base);
    if !base.ends_with('\n') {
        out.push('\n');
    }
    let mut contributors = Vec::new();
    for m in &ordered {
        // Attributed in the source itself: when someone decompiles a linked block to work out why
        // their game behaves oddly, the answer should be readable rather than inferred.
        out.push_str(&format!(
            "\n-- [Quartermaster] appended by Shipment: {}\n",
            m.shipment
        ));
        out.push_str(&m.append);
        if !m.append.ends_with('\n') {
            out.push('\n');
        }
        contributors.push(m.shipment.clone());
    }
    Ok((out, contributors))
}

/// The `_tOutfits` row for one outfit, as source.
///
/// `_tOutfits` is a GLOBAL declared without `local`, so a mod never needs an AST edit — appending a
/// `table.insert` after the base source is enough, and N of them union by plain concatenation. The
/// row's three fields are three distinct strings: `Model` is the asset name `Player.SetOutfit`
/// receives, `Name` is the unlock/tracking key, and `PlayerVisibleName` is what the wardrobe shows.
///
/// **Append only, never insert.** Index 2 is reserved for the unlock-code outfit, and a saved
/// costume is a POSITION into this list — inserting would silently re-dress every existing player.
pub fn outfit_row_append(wearer: &str, slug: &str, model: &str, display: &str) -> String {
    // Normalize to the RUNTIME `_tOutfits` key — the game's third key is `jennifer`, so an outfit
    // authored with the preferred `jen` must land there, not in an empty `_tOutfits.jen` the game
    // never reads. An unknown wearer falls through as-written; M0140 already flags it.
    let key = crate::manifest::wearer_table_key(wearer).unwrap_or(wearer);
    format!(
        "table.insert(_tOutfits.{key}, {{ Name = {}, Model = {}, PlayerVisibleName = {} }})\n",
        lua_string(slug),
        lua_string(model),
        lua_string(display),
    )
}

/// The `tUnlockStatus` table literal for a shop item: `{ <Fac> = 1, … }` when unlocked in every
/// listed vendor, or `{}` when it should ship locked. Locked matters only in Eva's obscured shop
/// (where a locked item is unbuyable); in the five faction shops a locked item is still purchasable.
pub fn shop_unlock_table(shops: &[crate::manifest::ShopVendor], unlocked: bool) -> String {
    if !unlocked {
        return "{}".to_string();
    }
    let mut s = String::from("{ ");
    for (i, v) in shops.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("{} = 1", v.faction_key()));
    }
    s.push_str(" }");
    s
}

/// A SUPPORT-catalog row appended to `mrxsupportdata` for one shop item.
///
/// Wrapped in `do … end` so the `local oSupport` is scoped per item and N appended rows never
/// collide. `tSupportData` is a module-global, so a top-level append lands before `Init()`'s
/// auto-name loop runs — the loop then picks the row up. The item key uses bracket form so an
/// arbitrary author id is safe.
#[allow(clippy::too_many_arguments)]
pub fn shop_support_row_append(
    id: &str,
    name: &str,
    description: &str,
    icon: &str,
    item_type: &str,
    cash_cost: u64,
    fuel_cost: u64,
    max_stock: u32,
    unlock_table: &str,
    module: &str,
    cargo: Option<&str>,
    delivery_vehicle: Option<&str>,
) -> String {
    let mut b = String::new();
    b.push_str("do\n");
    b.push_str(&format!("  local oSupport = {module}:Create()\n"));
    if let Some(c) = cargo {
        b.push_str(&format!("  oSupport:SetCargo({})\n", lua_string(c)));
    }
    if let Some(dv) = delivery_vehicle {
        b.push_str(&format!("  oSupport:SetDeliveryVehicle({})\n", lua_string(dv)));
    }
    b.push_str(&format!(
        "  tSupportData[{}] = {{ sName = {}, sDescription = {}, sIcon = {}, nMaxStock = {}, \
         nCashCost = {}, nFuelCost = {}, oSupport = oSupport, sType = {}, tUnlockStatus = {} }}\n",
        lua_string(id),
        lua_string(name),
        lua_string(description),
        lua_string(icon),
        max_stock,
        cash_cost,
        fuel_cost,
        lua_string(item_type),
        unlock_table,
    ));
    b.push_str("end\n");
    b
}

/// An EQUIPMENT-catalog row appended to `wifequipmentdata`. `_tEquipment` is a module-global; only
/// fuel-tank / grapple `nType`s are ever shopped (the `mrxshop` insert gate), which is what
/// `ntype_const` is restricted to.
pub fn shop_equipment_row_append(
    id: &str,
    name: &str,
    description: &str,
    texture: &str,
    ntype_const: &str,
    cost: u64,
) -> String {
    format!(
        "_tEquipment[{}] = {{ sName = {}, sDescription = {}, sTexture = {}, nType = {}, nCost = {} }}\n",
        lua_string(id),
        lua_string(name),
        lua_string(description),
        lua_string(texture),
        ntype_const,
        cost,
    )
}

/// The `mrxrewarddata` reward rows that SURFACE a shop item — one per vendor faction, because
/// `GetAllPotentialShopItems(f)` matches a row's `sFactionId` against exactly one faction. `field`
/// is `tSupport` (support catalog) or `tEquipment` (equipment catalog).
pub fn shop_reward_append(id: &str, field: &str, shops: &[crate::manifest::ShopVendor]) -> String {
    let mut out = String::new();
    for v in shops {
        let fac = v.faction_key();
        out.push_str(&format!(
            "_tRewards[{}] = {{ sFactionId = {}, {field} = {{ {{ {}, {} }} }} }}\n",
            lua_string(&format!("{id}Reward_{fac}")),
            lua_string(fac),
            lua_string(id),
            lua_string(fac),
        ));
    }
    out
}

/// The name of the Quartermaster mod-loader script — a NEW `scripts_vz` script the linker mints and
/// `add_script`s into the block, distinct from any retail script. Its ASET row (type 35, primary)
/// is emitted automatically by `script_patch_blocks`' new-entry branch, which is the other half of
/// the DLC's own recipe for a new importable script (`dlc_aset_normalize.py`).
///
/// `import` resolves a module by `(hash of its name, script type 0x42498680)` through the typed-asset
/// lookup, with no block or WAD in the key (`_SYS._IMPORT` = `FUN_005AE2D0`, decomp 219707-219761),
/// so a script minted into any mounted block resolves. It lands in the block of its trampoline host
/// `wifpmcinterior`, `scripts_vz`.
pub const QM_MODLOADER_NAME: &str = "qm_modloader";

/// The name of the front end's mod loader — a NEW script the linker mints into `shell.wad`'s
/// scripts block ([`SHELL_SCRIPT_BLOCKS`]) beside its trampoline host `mrxsound`. It loads and
/// unloads every front-end sound bank ([`qm_shell_modloader_source`]).
pub const QM_SHELL_MODLOADER_NAME: &str = "qm_shell_modloader";

/// One `add_ui` registration, resolved to the movie the loader must show. The linker collects these
/// across every Shipment and bakes them into a single generated `qm_modloader` script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiRegistration {
    /// Which Shipment asked for it — the tie-break for the deterministic bake order.
    pub shipment: String,
    /// The `cfx_pack` movie name the FlashWidget plays (`add_movie`'s asset name).
    pub movie: String,
}

/// One `activate_layer` registration, resolved to the layer marks the loader must apply. The linker
/// collects these across every Shipment and bakes them into the same `qm_modloader` script `add_ui`
/// uses, so a UI mod and a layer mod share one load space and one trampoline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerRegistration {
    /// Which Shipment asked for it — the tie-break for the deterministic bake order.
    pub shipment: String,
    /// The layer to `MrxLayerManager.MarkForAddition`.
    pub add: String,
    /// Layers to `MrxLayerManager.MarkForRemoval` first — the overlay(s) this one supersedes.
    pub remove: Vec<String>,
}

/// One novel-behaviour shop item — a `MrxSupport` subclass shipped as source, plus the catalog row
/// that must be DEFERRED into the loader (its `module:Create()` cannot run at resident-load time).
///
/// The linker mints `source` as a new `scripts_vz` script named `module` (like `qm_modloader`
/// itself), then bakes an `_inits` closure that `import`s it and constructs the catalog + reward
/// rows post-world-load. Ordered by `(shipment, id)` for a byte-identical bake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportRegistration {
    pub shipment: String,
    /// Runtime module name — both the minted script name and what the loader `import`s.
    pub module: String,
    /// The novel subclass Lua source, compiled + minted as a new `scripts_vz` script.
    pub source: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    /// `sType` string (already validated to the closed set by the manifest layer).
    pub item_type: String,
    pub cash_cost: u64,
    pub fuel_cost: u64,
    pub max_stock: u32,
    /// The `tUnlockStatus` table literal (from `shop_unlock_table`).
    pub unlock_table: String,
    pub cargo: Option<String>,
    pub delivery_vehicle: Option<String>,
    /// Vendor faction keys (Capitalized) for the deferred reward rows.
    pub shops: Vec<String>,
}

/// One sound bank a loader must load: an `add_sound` bank (its wavebank and soundbank), or the
/// wavebank that carries a sound override's waves (its wavebank only — the overridden soundbank is
/// one retail Lua already loads). A bank is only heard once it is loaded, and retail loads its own
/// banks by name through `MrxSoundBanks.LoadWaveBank` / `LoadSoundBank` (`mrxsoundbootstrap.lua`,
/// `mrxsound.lua`), so the loaders do the same for these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundBankRegistration {
    /// Which Shipment asked for it — the tie-break for the deterministic bake order.
    pub shipment: String,
    /// The bank name `MrxSoundBanks` is given.
    pub bank: String,
    /// Whether the soundbank is loaded as well as the wavebank.
    pub soundbank: bool,
    /// The sessions whose loader loads it: the gameplay loader ([`qm_modloader_source`]) and the
    /// front end's ([`qm_shell_modloader_source`]).
    pub sessions: BTreeSet<crate::manifest::LoadSession>,
}

/// The registrations of `regs` that `session`'s loader loads, in load `order`, then by bank name.
fn session_sounds<'a>(
    regs: &'a [SoundBankRegistration],
    session: crate::manifest::LoadSession,
    order: &[String],
) -> Result<Vec<&'a SoundBankRegistration>, LinkError> {
    let mut out: Vec<&SoundBankRegistration> = regs.iter().filter(|r| r.sessions.contains(&session)).collect();
    sort_by_order(&mut out, order, |r| r.shipment.as_str(), |r| r.bank.clone())?;
    Ok(out)
}

/// The `load_sounds` / `unload_sounds` functions of a loader whose state table is `table`: the ONE
/// generator both loaders use.
///
/// Each bank is loaded with the calls retail makes for its own banks — the wavebank, then, for an
/// `add_sound` bank, the soundbank (`mrxsoundbootstrap.lua:195-217`) — and unloaded with their
/// mirrors, in `regs` order. `table.loaded[bank]` records a load, so a second `load_sounds` in one
/// session loads nothing again and `unload_sounds` unloads only what was loaded. No callback is
/// passed: `MrxSoundBanks` keeps ONE batch callback (`_funcBatchComplete`, `mrxsoundbanks.lua:10-36`)
/// and a new one replaces the caller's, which for the front end is retail's `_StartShellMusic`.
/// `MrxSoundBanks` reports no failure: `_FlagAssetOpComplete` takes no argument
/// (`mrxsoundbanks.lua:141-152`) and the engine's callback carries no flag (`audio_code_map.md`
/// §11.2), so a bank's presence is checked at build time instead.
fn sound_loader_functions(table: &str, regs: &[&SoundBankRegistration]) -> String {
    let mut load = String::new();
    let mut unload = String::new();
    for r in regs {
        let n = lua_string(&r.bank);
        let owner = lua_string(&r.shipment);
        let (sb_load, sb_unload) = if r.soundbank {
            (format!("    MrxSoundBanks.LoadSoundBank({n})\n"), format!("    MrxSoundBanks.UnloadSoundBank({n})\n"))
        } else {
            (String::new(), String::new())
        };
        load.push_str(&format!(
            "  -- {owner}\n  if not {table}.loaded[{n}] then\n    MrxSoundBanks.LoadWaveBank({n})\n{sb_load}    \
             {table}.loaded[{n}] = true\n  end\n"
        ));
        unload.push_str(&format!(
            "  -- {owner}\n  if {table}.loaded[{n}] then\n    MrxSoundBanks.UnloadWaveBank({n})\n{sb_unload}    \
             {table}.loaded[{n}] = nil\n  end\n"
        ));
    }
    format!(
        "{table}.loaded = {table}.loaded or {{}}\n\
         function {table}.load_sounds()\n{load}end\n\
         function {table}.unload_sounds()\n{unload}end\n"
    )
}

/// The whole `qm_modloader` script, baked from every UI, layer, support and gameplay sound
/// registration.
///
/// This is the "expandable load space": the game's scripts stay untouched but for the trampolines
/// ([`qm_trampoline_append`], [`qm_sound_exit_append`]); everything a mod adds lives *here*, in a
/// script Quartermaster owns and re-mints each link. It publishes its state as `_G._QM`, the way
/// retail publishes a table other modules read (`_G.Hud = HudInterface`, `mrxguiinterface.lua:13`):
/// a module's own globals live in its own environment, so the trampolines reach `_QM` through `_G`.
///
/// Sound banks load and unload through `_QM.load_sounds()` / `_QM.unload_sounds()`
/// ([`sound_loader_functions`]), with each bank's state in `_QM.loaded`. `_OnEnter` calls
/// `load_sounds` on every entry and the `ExitGame` trampoline calls `unload_sounds`. The UI, layer
/// and support registrations run once, from `_QM.run()`: `_registered`/`_ran` guards make a
/// re-import or a re-entered interior a no-op, and each of them runs under `pcall`.
///
/// Registrations are baked in the resolved load `order` — the same order [`linked_source`] follows —
/// and within one Shipment by movie / layer / id / bank, so one request bakes byte-identically
/// whatever order the registrations arrive in. A Shipment that requires another is therefore
/// registered after it. A contributor missing from `order` is [`LinkError::NotInOrder`].
pub fn qm_modloader_source(
    regs: &[UiRegistration],
    layers: &[LayerRegistration],
    support: &[SupportRegistration],
    sounds: &[SoundBankRegistration],
    order: &[String],
) -> Result<String, LinkError> {
    let mut inits = String::new();

    let sounds = session_sounds(sounds, crate::manifest::LoadSession::Gameplay, order)?;
    let sound_import = if sounds.is_empty() { "" } else { "import(\"MrxSoundBanks\")\n" };
    let sound_fns = sound_loader_functions("_QM", &sounds);

    let mut ordered: Vec<&UiRegistration> = regs.iter().collect();
    sort_by_order(&mut ordered, order, |r| r.shipment.as_str(), |r| r.movie.clone())?;

    for r in &ordered {
        let n = lua_string(&r.movie);
        // The creation sequence — new → SetSwfFile → Play → SetVisible — is the proven one from
        // decompiled mrxgui.lua (`loadingscreen_standalone`). Existence-checked so a stripped widget
        // table degrades rather than errors; the handle is parked in `_QM.ui[name]` for the author.
        inits.push_str(&format!(
            "  -- {shipment}: {movie}\n  \
             table.insert(_QM._inits, function()\n    \
             local w = FlashWidget:new(); w:SetSwfFile({n})\n    \
             if w.Play then w:Play() end\n    \
             if w.SetVisible then w:SetVisible(true) end\n    \
             _QM.ui[{n}] = w\n  \
             end)\n",
            shipment = r.shipment,
            movie = r.movie,
        ));
    }

    // Layer activations bake into the SAME `_QM._inits` list, so they run under the same once-guard
    // and the same `pcall` as the UI widgets. Each is `MarkForRemoval`(old) then `MarkForAddition`
    // (new) — the vanilla-contract order (remove pristine, add act) so the two never both apply.
    // Ordered by load order, then `add`, for the byte-identical bake. `MarkForAddition`/
    // `MarkForRemoval` are the immediate mark forms (no callback), existence-checked so a stripped
    // table degrades.
    let mut ordered_layers: Vec<&LayerRegistration> = layers.iter().collect();
    sort_by_order(&mut ordered_layers, order, |r| r.shipment.as_str(), |r| r.add.clone())?;
    for r in &ordered_layers {
        let add = lua_string(&r.add);
        let mut body = String::new();
        for rem in &r.remove {
            body.push_str(&format!(
                "      if MrxLayerManager.MarkForRemoval then MrxLayerManager.MarkForRemoval({}) end\n",
                lua_string(rem),
            ));
        }
        body.push_str(&format!(
            "      if MrxLayerManager.MarkForAddition then MrxLayerManager.MarkForAddition({add}) end\n"
        ));
        inits.push_str(&format!(
            "  -- {shipment}: activate {layer}\n  \
             table.insert(_QM._inits, function()\n    \
             if MrxLayerManager then\n{body}    end\n  \
             end)\n",
            shipment = r.shipment,
            layer = r.add,
        ));
    }

    // Novel-behaviour shop items bake into the SAME once-guarded, `pcall`-wrapped `_inits` list. Each
    // imports its minted subclass, constructs `oSupport`, DEFERS the catalog + reward rows (the eager
    // append can't — the module is nil at resident-load), re-applies the `SetSupportName` the
    // `mrxsupportdata.Init` tail loop already ran without, and nulls the never-invalidated
    // `gtAllSupport` cache so the next shop Open rebuilds with the new id. Ordered by load order,
    // then `id`.
    let mut ordered_support: Vec<&SupportRegistration> = support.iter().collect();
    sort_by_order(&mut ordered_support, order, |r| r.shipment.as_str(), |r| r.id.clone())?;
    for r in &ordered_support {
        let mut body = String::new();
        body.push_str(&format!("    import({})\n", lua_string(&r.module)));
        body.push_str(&format!("    local oSupport = {}:Create()\n", r.module));
        if let Some(c) = &r.cargo {
            body.push_str(&format!("    oSupport:SetCargo({})\n", lua_string(c)));
        }
        if let Some(dv) = &r.delivery_vehicle {
            body.push_str(&format!(
                "    oSupport:SetDeliveryVehicle({})\n",
                lua_string(dv)
            ));
        }
        body.push_str(&format!(
            "    if oSupport.SetSupportName then oSupport:SetSupportName({}) end\n",
            lua_string(&r.id)
        ));
        body.push_str(&format!(
            "    MrxSupportData.tSupportData[{}] = {{ sName = {}, sDescription = {}, sIcon = {}, \
             nMaxStock = {}, nCashCost = {}, nFuelCost = {}, oSupport = oSupport, sType = {}, \
             tUnlockStatus = {} }}\n",
            lua_string(&r.id),
            lua_string(&r.name),
            lua_string(&r.description),
            lua_string(&r.icon),
            r.max_stock.min(99),
            r.cash_cost,
            r.fuel_cost,
            lua_string(&r.item_type),
            r.unlock_table,
        ));
        for fac in &r.shops {
            body.push_str(&format!(
                "    MrxRewardData._tRewards[{}] = {{ sFactionId = {}, tSupport = {{ {{ {}, {} }} }} }}\n",
                lua_string(&format!("{}Reward_{}", r.id, fac)),
                lua_string(fac),
                lua_string(&r.id),
                lua_string(fac),
            ));
        }
        body.push_str("    MrxRewardData.gtAllSupport = nil\n");
        inits.push_str(&format!(
            "  -- {shipment}: shop ability {id} ({module})\n  \
             table.insert(_QM._inits, function()\n    \
             if MrxSupportData and MrxRewardData then\n{body}    end\n  \
             end)\n",
            shipment = r.shipment,
            id = r.id,
            module = r.module,
        ));
    }

    Ok(format!(
        "-- {name} — Quartermaster's expandable mod load space (generated; do not hand-edit).\n\
         --\n\
         -- The trampolines import this by name; it publishes _G._QM with load_sounds(), unload_sounds()\n\
         -- and run(). Every modded registration lives here, so the game's scripts carry only the\n\
         -- [Quartermaster] trampolines appended to wifpmcinterior and mrxsoundbootstrap.\n\
         {sound_import}\
         _G._QM = _G._QM or {{}}\n\
         _QM.ui = _QM.ui or {{}}\n\
         {sound_fns}\
         if not _QM._registered then\n\
         \x20 _QM._registered = true\n\
         \x20 _QM._inits = {{}}\n\
         {inits}\
         \x20 function _QM.run()\n\
         \x20   if _QM._ran then return end\n\
         \x20   _QM._ran = true\n\
         \x20   for _, f in ipairs(_QM._inits) do pcall(f) end\n\
         \x20 end\n\
         end\n",
        name = QM_MODLOADER_NAME,
    ))
}

/// The whole `qm_shell_modloader` script: the front end's sound loader, baked from every
/// registration whose sessions include the front end.
///
/// It publishes `_G._QMS` with `load_sounds()` / `unload_sounds()` from the same generator the
/// gameplay loader uses ([`sound_loader_functions`]), in load `order`, then by bank name. The
/// trampoline appended to the front end's `mrxsound` ([`qm_shell_trampoline_append`]) calls them
/// after retail's `EnterShellState` / `ExitShellState`. A contributor missing from `order` is
/// [`LinkError::NotInOrder`].
pub fn qm_shell_modloader_source(sounds: &[SoundBankRegistration], order: &[String]) -> Result<String, LinkError> {
    let sounds = session_sounds(sounds, crate::manifest::LoadSession::FrontEnd, order)?;
    Ok(format!(
        "-- {name} — Quartermaster's front-end sound loader (generated; do not hand-edit).\n\
         --\n\
         -- The [Quartermaster] trampoline appended to MrxSound imports this by name and calls\n\
         -- _QMS.load_sounds() after EnterShellState and _QMS.unload_sounds() after ExitShellState.\n\
         import(\"MrxSoundBanks\")\n\
         _G._QMS = _G._QMS or {{}}\n\
         {fns}",
        name = QM_SHELL_MODLOADER_NAME,
        fns = sound_loader_functions("_QMS", &sounds),
    ))
}

/// The trampoline appended to `wifpmcinterior`.
///
/// It wraps `_OnEnter` (the PMC-interior entry hook, `wifpmcinterior.lua:386`: GUI is fully up by
/// then, it fires every session, and it is file-local so this concatenated append can wrap it — the
/// same property the `_tOutfits` append relies on). The wrapper calls retail's `_OnEnter`, then
/// synchronously `import`s `qm_modloader`, loads its sound banks (each once per session,
/// `_QM.loaded`) and runs its other registrations once (`_QM.run`'s `_ran`). Nothing here grows as
/// mods are added — new mods only enlarge `qm_modloader`.
pub fn qm_trampoline_append() -> String {
    format!(
        "\n-- [Quartermaster] mod-loader trampoline. The expandable load space is {name}, imported by\n\
         -- name; this is the only code wifpmcinterior carries.\n\
         do\n\
         \x20 local _qm_prev_OnEnter = _OnEnter\n\
         \x20 _OnEnter = function(...)\n\
         \x20   _qm_prev_OnEnter(...)\n\
         \x20   import({name_lit})\n\
         \x20   _QM.load_sounds()\n\
         \x20   _QM.run()\n\
         \x20 end\n\
         end\n",
        name = QM_MODLOADER_NAME,
        name_lit = lua_string(QM_MODLOADER_NAME),
    )
}

/// The trampoline appended to the gameplay `mrxsoundbootstrap` when any registration loads a bank in
/// gameplay: `ExitGame` calls retail's (`UnloadBanks`, `mrxsoundbootstrap.lua:188-190`), then
/// imports `qm_modloader` and unloads its banks. `vz`'s `ResetSingleton` calls
/// `MrxSoundBootstrap.ExitGame()` on the way out of the game (`vz/xQ!L.lua:612`).
pub fn qm_sound_exit_append() -> String {
    format!(
        "\n-- [Quartermaster] sound-unload trampoline into {name}.\n\
         do\n\
         \x20 local _qm_prev_ExitGame = ExitGame\n\
         \x20 ExitGame = function(...)\n\
         \x20   _qm_prev_ExitGame(...)\n\
         \x20   import({name_lit})\n\
         \x20   _QM.unload_sounds()\n\
         \x20 end\n\
         end\n",
        name = QM_MODLOADER_NAME,
        name_lit = lua_string(QM_MODLOADER_NAME),
    )
}

/// The trampoline appended to the front end's `mrxsound`: `EnterShellState` and `ExitShellState`
/// (`shell/mrxsound.lua:5-27`, called from `mrxguishell.lua:505` / `:589` and
/// `mrxsoundshellbootstrap.lua:99`) each call retail's first, then import `qm_shell_modloader` and
/// load or unload its banks.
pub fn qm_shell_trampoline_append() -> String {
    format!(
        "\n-- [Quartermaster] front-end sound-loader trampoline into {name}.\n\
         do\n\
         \x20 local _qm_prev_EnterShellState = EnterShellState\n\
         \x20 EnterShellState = function(...)\n\
         \x20   _qm_prev_EnterShellState(...)\n\
         \x20   import({name_lit})\n\
         \x20   _QMS.load_sounds()\n\
         \x20 end\n\
         \x20 local _qm_prev_ExitShellState = ExitShellState\n\
         \x20 ExitShellState = function(...)\n\
         \x20   _qm_prev_ExitShellState(...)\n\
         \x20   import({name_lit})\n\
         \x20   _QMS.unload_sounds()\n\
         \x20 end\n\
         end\n",
        name = QM_SHELL_MODLOADER_NAME,
        name_lit = lua_string(QM_SHELL_MODLOADER_NAME),
    )
}

/// A Lua string literal with quotes and backslashes escaped, so an author-supplied `display:`
/// cannot terminate the string and inject code into the block we compile.
fn lua_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Source the Quartermaster appends ONCE per target, after every Shipment's contribution.
///
/// For the wardrobe this is the availability lift. `GetAvailableCostumes()` returns
/// `_nAvailableCostumes or 1`, and the menu only offers entry `i` when the count is `>= i` — so an
/// appended outfit is in the WAD, in the table, and **unreachable** unless the count grows too.
///
/// It is emitted here rather than by each Shipment precisely because "exactly once" is the
/// property: two Shipments each hard-coding `shipped + 1` produce the same number, the later
/// definition wins, and one outfit stays invisible. Deriving it from the final list length is the
/// only form that survives N contributors.
///
/// Curated per target and empty by default, in the same fail-closed spirit as the merge classes.
pub fn derived_epilogue(target: &str) -> Option<String> {
    match target {
        "wifpmcinterior" => Some(
            "\n-- [Quartermaster] derived: the wardrobe gate is a COUNT, and the menu only offers\n\
             -- entry i when it is >= i. Derived from the final list length so it is correct for\n\
             -- any number of appended outfits.\n\
             function GetAvailableCostumes()\n\
             \x20 local n = 1\n\
             \x20 for _, list in pairs(_tOutfits) do\n\
             \x20   if #list > n then n = #list end\n\
             \x20 end\n\
             \x20 return n\n\
             end\n"
                .to_string(),
        ),
        _ => None,
    }
}

/// Link every mutation into `block`, returning what changed.
///
/// `block` is the base game's `scripts_vz`, already parsed. Mutations targeting the same script are
/// merged; mutations targeting different scripts are independent.
///
/// The single-block convenience case of [`link_into_blocks`]. `order` is the resolved load order of
/// the contributing Shipments' names.
pub fn link_into(
    block: &mut ScriptsBlock,
    corpus_root: &Path,
    mutations: &[ScriptMutation],
    order: &[String],
) -> Result<LinkOutput, LinkError> {
    let mut blocks = [TargetBlock {
        path: String::new(),
        block,
    }];
    link_into_blocks(&mut blocks, corpus_root, mutations, &[], &[], &[], &[], &[], &[], order)
}

/// Compile each target of `mutations` once, its base source (found under `subfolders`,
/// [`base_source_path`]) followed by every append in load `order`, and splice it into whichever of
/// `blocks` carries the target first.
fn link_appends(
    blocks: &mut [TargetBlock<'_>],
    corpus_root: &Path,
    subfolders: &[&str],
    mutations: &[ScriptMutation],
    order: &[String],
) -> Result<Vec<LinkedScript>, LinkError> {
    // Group by target so each script is compiled ONCE with all its appends, which is the entire
    // point — compiling per Shipment would mean the last one wins again, just more slowly.
    let mut by_target: BTreeMap<&str, Vec<&ScriptMutation>> = BTreeMap::new();
    for m in mutations {
        by_target.entry(m.target.as_str()).or_default().push(m);
    }

    let mut linked = Vec::new();
    for (target, group) in by_target {
        // Type-aware lookup: the resident block carries ~240 Lua chunks among ~7,000 entries of
        // other types, so a name-hash-only match could resolve to a texture.
        let (bi, idx) = blocks
            .iter()
            .enumerate()
            .find_map(|(bi, tb)| tb.block.find_script_by_name(target).map(|idx| (bi, idx)))
            .ok_or_else(|| LinkError::UnknownScript {
                target: target.to_string(),
                shipment: group[0].shipment.clone(),
            })?;
        let block = &mut *blocks[bi].block;
        let source_path = base_source_path(corpus_root, subfolders, target).map_err(|tried| LinkError::NoBaseSource {
            target: target.to_string(),
            tried,
        })?;
        let base = std::fs::read_to_string(&source_path).map_err(|e| LinkError::Compile {
            target: target.to_string(),
            message: format!("reading base source {}: {e}", source_path.display()),
        })?;

        let (mut source, contributors) = linked_source(&base, &group, order)?;
        // Emitted once, AFTER every Shipment's append — see `derived_epilogue`. Putting it here
        // rather than in each Shipment is what makes "exactly once" structural.
        if let Some(epilogue) = derived_epilogue(target) {
            source.push_str(&epilogue);
        }
        // BARE chunk name — see the module note. `@name.lua` produces a chunk 5 bytes off retail.
        let bytecode = mercs2_luac::compile(&source, target).map_err(|e| LinkError::Compile {
            target: target.to_string(),
            message: e.to_string(),
        })?;
        block.replace_lua(idx, &bytecode).map_err(|e| LinkError::Splice {
            target: target.to_string(),
            message: e,
        })?;

        linked.push(LinkedScript {
            target: target.to_string(),
            contributors,
            base_source_bytes: base.len(),
            linked_source_bytes: source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }
    Ok(linked)
}

/// Link the front end's sound loader into `shell.wad`'s scripts block ([`SHELL_SCRIPT_BLOCKS`]).
///
/// When any of `sound_regs` loads a bank in the front end, the trampoline
/// ([`qm_shell_trampoline_append`]) is appended to `mrxsound` — its base source from the corpus's
/// `shell/` ([`SHELL_SOURCE_DIRS`]) — and `qm_shell_modloader` ([`qm_shell_modloader_source`]) is
/// minted into the same block, beside it. With no front-end registration, nothing is linked and the
/// output is empty. Every touched block must still CSUM-verify.
pub fn link_front_end(
    blocks: &mut [TargetBlock<'_>],
    corpus_root: &Path,
    sound_regs: &[SoundBankRegistration],
    order: &[String],
) -> Result<LinkOutput, LinkError> {
    let sounds = session_sounds(sound_regs, crate::manifest::LoadSession::FrontEnd, order)?;
    if sounds.is_empty() {
        return Ok(LinkOutput { scripts: Vec::new(), unresolved_imports: Vec::new() });
    }
    let trampoline = ScriptMutation {
        shipment: MODLOADER_CONTRIBUTOR.into(),
        target: "mrxsound".into(),
        append: qm_shell_trampoline_append(),
    };
    let mut linked = link_appends(blocks, corpus_root, SHELL_SOURCE_DIRS, &[trampoline], order)?;
    let bi = linked[0].block;

    let source = qm_shell_modloader_source(sound_regs, order)?;
    let bytecode = mercs2_luac::compile(&source, QM_SHELL_MODLOADER_NAME).map_err(|e| LinkError::Compile {
        target: QM_SHELL_MODLOADER_NAME.to_string(),
        message: e.to_string(),
    })?;
    blocks[bi].block.add_script(QM_SHELL_MODLOADER_NAME, &bytecode).map_err(|m| LinkError::Splice {
        target: QM_SHELL_MODLOADER_NAME.to_string(),
        message: m,
    })?;
    let mut names: Vec<&String> = sounds.iter().map(|r| &r.shipment).collect();
    names.dedup();
    linked.push(LinkedScript {
        target: QM_SHELL_MODLOADER_NAME.to_string(),
        contributors: names.into_iter().cloned().collect(),
        base_source_bytes: 0,
        linked_source_bytes: source.len(),
        bytecode_bytes: bytecode.len(),
        block: bi,
    });
    blocks[bi]
        .block
        .verify_csums()
        .map_err(|e| LinkError::Block(format!("CSUMs after linking {}: {e}", blocks[bi].path)))?;
    Ok(LinkOutput { scripts: linked, unresolved_imports: Vec::new() })
}

/// Link every mutation into whichever of `blocks` actually carries its target script.
///
/// Mutations targeting the same script are merged; mutations targeting different scripts are
/// independent, **including when they land in different blocks**. Blocks are searched in the order
/// given and the first script-typed match wins.
///
/// Only blocks that were spliced come back in the results (via [`LinkedScript::block`]) — a block
/// nothing targeted must not be re-emitted, or the overlay would shadow a base block with a
/// byte-identical copy for no reason.
///
/// `order` is the resolved load order of the contributing Shipments' names, and every ordered
/// decision here follows it: append concatenation, which `replace_lua` wins (the later one), the
/// mint order of `add_script` modules and support subclasses, and the `qm_modloader` bake. A
/// contributor missing from it is [`LinkError::NotInOrder`].
///
/// Once everything is linked and minted, every Shipment-authored Lua source — `patch_lua` appends,
/// `add_script` modules, `replace_lua` sources and support modules — is scanned for literal
/// `import("x")` calls ([`literal_imports`]); the ones nothing provides come back as
/// [`LinkOutput::unresolved_imports`] (M0209). They are warnings: they never fail the link.
pub fn link_into_blocks(
    blocks: &mut [TargetBlock<'_>],
    corpus_root: &Path,
    mutations: &[ScriptMutation],
    ui_regs: &[UiRegistration],
    layer_regs: &[LayerRegistration],
    support_regs: &[SupportRegistration],
    sound_regs: &[SoundBankRegistration],
    additions: &[ScriptAddition],
    replacements: &[ScriptReplacement],
    order: &[String],
) -> Result<LinkOutput, LinkError> {
    // Anything that lives in the load space — a UI widget, a layer activation, a novel support
    // behaviour or a sound bank to load in gameplay — needs the loader minted and the
    // `wifpmcinterior` trampoline installed.
    let gameplay_sounds = sound_regs.iter().any(|r| r.sessions.contains(&crate::manifest::LoadSession::Gameplay));
    let needs_loader = !ui_regs.is_empty() || !layer_regs.is_empty() || !support_regs.is_empty() || gameplay_sounds;
    // Fold the mod-loader trampoline in as a synthetic `wifpmcinterior` mutation when any load-space
    // mod registered — ONE trampoline regardless of how many, so the host never grows with mod
    // count; and, when a bank loads in gameplay, the unload trampoline into `mrxsoundbootstrap`. The
    // expandable part is `qm_modloader`, minted after the base scripts link (below).
    let mut all_mutations: Vec<ScriptMutation> = mutations.to_vec();
    if needs_loader {
        all_mutations.push(ScriptMutation {
            shipment: MODLOADER_CONTRIBUTOR.into(),
            target: "wifpmcinterior".into(),
            append: qm_trampoline_append(),
        });
    }
    if gameplay_sounds {
        all_mutations.push(ScriptMutation {
            shipment: MODLOADER_CONTRIBUTOR.into(),
            target: "mrxsoundbootstrap".into(),
            append: qm_sound_exit_append(),
        });
    }

    let mut linked = link_appends(blocks, corpus_root, VZ_SOURCE_DIRS, &all_mutations, order)?;

    // Apply each `replace_lua` wholesale swap. Same asset hash, new bytecode -- every existing
    // `import(<target>)` call site now returns the new module without rebinding. Applied in load
    // order, so where two replace one target the later Shipment's is what remains.
    let mut in_order: Vec<&ScriptReplacement> = replacements.iter().collect();
    sort_by_order(&mut in_order, order, |r| r.shipment.as_str(), |_| ())?;
    for r in in_order {
        let (bi, idx) = blocks
            .iter()
            .enumerate()
            .find_map(|(bi, tb)| tb.block.find_script_by_name(&r.target).map(|idx| (bi, idx)))
            .ok_or_else(|| LinkError::UnknownScript {
                target: r.target.clone(),
                shipment: r.shipment.clone(),
            })?;
        let bytecode = mercs2_luac::compile(&r.source, &r.target)
            .map_err(|e| LinkError::Compile {
                target: r.target.clone(),
                message: e.to_string(),
            })?;
        blocks[bi]
            .block
            .replace_lua(idx, &bytecode)
            .map_err(|m| LinkError::Splice {
                target: r.target.clone(),
                message: m,
            })?;
        linked.push(LinkedScript {
            target: r.target.clone(),
            contributors: vec![r.shipment.clone()],
            base_source_bytes: 0,
            linked_source_bytes: r.source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }

    // Mint each first-class `add_script` addition. Each one becomes a fresh entry in the
    // `scripts_vz` block with its own primary type-35 ASET row (via `script_patch_blocks`' new-entry
    // branch), so the engine's `import` / `dynamic_import` locates it the same way any shipped
    // script is located. Lives in the `wifpmcinterior`-carrying block beside `qm_modloader`, which
    // keeps every minted script in one discoverable place.
    if !additions.is_empty() {
        let bi = blocks
            .iter()
            .position(|tb| tb.block.find_script_by_name("wifpmcinterior").is_some())
            .ok_or_else(|| LinkError::UnknownScript {
                target: "wifpmcinterior".to_string(),
                shipment: additions[0].shipment.clone(),
            })?;
        let mut additions: Vec<&ScriptAddition> = additions.iter().collect();
        sort_by_order(&mut additions, order, |a| a.shipment.as_str(), |_| ())?;
        for a in additions {
            let bytecode = mercs2_luac::compile(&a.source, &a.name)
                .map_err(|e| LinkError::Compile {
                    target: a.name.clone(),
                    message: e.to_string(),
                })?;
            blocks[bi]
                .block
                .add_script(&a.name, &bytecode)
                .map_err(|m| LinkError::Splice {
                    target: a.name.clone(),
                    message: m,
                })?;
            linked.push(LinkedScript {
                target: a.name.clone(),
                contributors: vec![a.shipment.clone()],
                base_source_bytes: 0,
                linked_source_bytes: a.source.len(),
                bytecode_bytes: bytecode.len(),
                block: bi,
            });
        }
    }

    // Mint the mod loader. It is a NEW `scripts_vz` script — `add_script` appends its container and
    // (via `script_patch_blocks`' new-entry branch) its primary type-35 ASET row, the two halves the
    // DLC's own recipe ships. It goes in the block carrying its trampoline host `wifpmcinterior`.
    if needs_loader {
        let bi = blocks
            .iter()
            .position(|tb| tb.block.find_script_by_name("wifpmcinterior").is_some())
            .ok_or_else(|| LinkError::UnknownScript {
                target: "wifpmcinterior".to_string(),
                shipment: MODLOADER_CONTRIBUTOR.to_string(),
            })?;

        // Mint each NOVEL support subclass as its own new `scripts_vz` script, so the loader can
        // `import` it by name at `_OnEnter`. Same `add_script` mechanism as `qm_modloader` below.
        // Deduped by module name — two items sharing one subclass mint it once.
        let mut minted: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        let mut support_in_order: Vec<&SupportRegistration> = support_regs.iter().collect();
        sort_by_order(&mut support_in_order, order, |r| r.shipment.as_str(), |_| ())?;
        for r in support_in_order {
            if !minted.insert(r.module.as_str()) {
                continue;
            }
            let bytecode =
                mercs2_luac::compile(&r.source, &r.module).map_err(|e| LinkError::Compile {
                    target: r.module.clone(),
                    message: e.to_string(),
                })?;
            blocks[bi]
                .block
                .add_script(&r.module, &bytecode)
                .map_err(|m| LinkError::Splice {
                    target: r.module.clone(),
                    message: m,
                })?;
            linked.push(LinkedScript {
                target: r.module.clone(),
                contributors: vec![r.shipment.clone()],
                base_source_bytes: 0,
                linked_source_bytes: r.source.len(),
                bytecode_bytes: bytecode.len(),
                block: bi,
            });
        }

        let source = qm_modloader_source(ui_regs, layer_regs, support_regs, sound_regs, order)?;
        // BARE chunk name, like every other script here — see the module note.
        let bytecode =
            mercs2_luac::compile(&source, QM_MODLOADER_NAME).map_err(|e| LinkError::Compile {
                target: QM_MODLOADER_NAME.to_string(),
                message: e.to_string(),
            })?;
        blocks[bi]
            .block
            .add_script(QM_MODLOADER_NAME, &bytecode)
            .map_err(|m| LinkError::Splice {
                target: QM_MODLOADER_NAME.to_string(),
                message: m,
            })?;
        let mut names: Vec<&String> = ui_regs
            .iter()
            .map(|r| &r.shipment)
            .chain(layer_regs.iter().map(|r| &r.shipment))
            .chain(support_regs.iter().map(|r| &r.shipment))
            .chain(
                sound_regs
                    .iter()
                    .filter(|r| r.sessions.contains(&crate::manifest::LoadSession::Gameplay))
                    .map(|r| &r.shipment),
            )
            .collect();
        names.sort();
        names.dedup();
        sort_by_order(&mut names, order, |n| n.as_str(), |_| ())?;
        let contributors: Vec<String> = names.into_iter().cloned().collect();
        linked.push(LinkedScript {
            target: QM_MODLOADER_NAME.to_string(),
            contributors,
            base_source_bytes: 0,
            linked_source_bytes: source.len(),
            bytecode_bytes: bytecode.len(),
            block: bi,
        });
    }

    // Every block we touched must still verify. `replace_lua` recomputes each container's CSUM, so
    // a failure here means the block itself was left inconsistent.
    for bi in linked.iter().map(|l| l.block).collect::<std::collections::BTreeSet<_>>() {
        blocks[bi]
            .block
            .verify_csums()
            .map_err(|e| LinkError::Block(format!("CSUMs after linking {}: {e}", blocks[bi].path)))?;
    }

    // M0209. Checked only now, when every addition, support module and `qm_modloader` has been
    // minted into the blocks, so "a script by that name is in a loaded block" covers all of them.
    let mut sources: Vec<(&str, String, &str)> = Vec::new();
    for m in mutations {
        sources.push((&m.shipment, format!("patch_lua append to {}", m.target), &m.append));
    }
    for a in additions {
        sources.push((&a.shipment, format!("add_script {}", a.name), &a.source));
    }
    for r in replacements {
        sources.push((&r.shipment, format!("replace_lua {}", r.target), &r.source));
    }
    for r in support_regs {
        sources.push((&r.shipment, format!("support module {}", r.module), &r.source));
    }
    let mut unresolved_imports: Vec<UnresolvedImport> = Vec::new();
    for (shipment, what, source) in sources {
        for module in literal_imports(source) {
            let provided = module == QM_MODLOADER_NAME
                || blocks
                    .iter()
                    .any(|tb| tb.block.find_script_by_name(&module).is_some());
            let entry = UnresolvedImport {
                shipment: shipment.to_string(),
                module,
                source: what.clone(),
            };
            if !provided && !unresolved_imports.contains(&entry) {
                unresolved_imports.push(entry);
            }
        }
    }
    Ok(LinkOutput {
        scripts: linked,
        unresolved_imports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The literal forms are found; comments, strings, fields, methods, definitions and the
    /// documented-unchecked forms are not.
    #[test]
    fn literal_imports_finds_only_literal_calls_to_the_global_import() {
        let src = r#"
local a = import("ess")
local b = import ( 'ess_names' )
-- import("in_a_line_comment")
--[[ import("in_a_long_comment") ]]
--[==[ import("in_a_level_2_comment") ]]==]
local s = "import(\"in_a_string\")"
local l = [[ import("in_a_long_string") ]]
local c = t.import("a_field")
local d = t:import("a_method")
function import(name) return name end
local e = dynamic_import("dynamic")
local f = import(name)
local g = import("a" .. suffix)
local h = import "no_parens"
local i = import("esc\097ped")
"#;
        assert_eq!(literal_imports(src), vec!["ess", "ess_names", "escaped"]);
    }

    #[test]
    fn literal_imports_survives_numbers_and_operators() {
        let src = "x = 1e-5 + 0x1F .. import('after_concat') ... y = .5; z = a..import(\"b\")\n";
        assert_eq!(literal_imports(src), vec!["after_concat", "b"]);
    }

    fn mutation(shipment: &str, target: &str, append: &str) -> ScriptMutation {
        ScriptMutation {
            shipment: shipment.into(),
            target: target.into(),
            append: append.into(),
        }
    }

    /// ★ The preferred spelling `jen` must reach the RUNTIME key `jennifer`, or the outfit appends
    /// to a `_tOutfits.jen` the game never reads. This is the whole reason the split exists.
    #[test]
    fn jen_appends_to_the_jennifer_table() {
        let row = outfit_row_append("jen", "MyFit", "pmc_hum_jen", "[X]");
        assert!(row.contains("_tOutfits.jennifer"), "jen must normalize to jennifer: {row}");
        assert!(!row.contains("_tOutfits.jen,"), "must not emit the empty jen table: {row}");
        // The literal runtime key still works, and mattias/chris pass straight through.
        assert!(outfit_row_append("jennifer", "F", "m", "d").contains("_tOutfits.jennifer"));
        assert!(outfit_row_append("mattias", "F", "m", "d").contains("_tOutfits.mattias"));
        assert!(outfit_row_append("chris", "F", "m", "d").contains("_tOutfits.chris"));
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// The ordering property the whole design rests on: one resolved order, same bytes, whatever
    /// order the mutations arrive in.
    #[test]
    fn concatenation_follows_the_order_not_the_input_order() {
        let a = mutation("aaa-mod", "s", "-- A\n");
        let b = mutation("zzz-mod", "s", "-- Z\n");
        let order = names(&["aaa-mod", "zzz-mod"]);
        let (one, c1) = linked_source("base\n", &[&a, &b], &order).unwrap();
        let (two, c2) = linked_source("base\n", &[&b, &a], &order).unwrap();
        assert_eq!(one, two, "input order must not change the linked source");
        assert_eq!(c1, c2);
        assert_eq!(c1, vec!["aaa-mod", "zzz-mod"]);
        // Both appends must survive — this is the annihilation the linker exists to prevent.
        assert!(one.contains("-- A") && one.contains("-- Z"), "{one}");
    }

    /// A base script with no trailing newline must not weld into the first append.
    #[test]
    fn a_base_without_a_trailing_newline_is_separated() {
        let m = mutation("mod", "s", "print('x')\n");
        let (src, _) = linked_source("local t = 1", &[&m], &names(&["mod"])).unwrap();
        assert!(src.starts_with("local t = 1\n"), "{src}");
        assert!(
            !src.contains("local t = 1--"),
            "appended source welded onto the base: {src}"
        );
    }

    #[test]
    fn each_append_is_attributed_in_the_source() {
        let m = mutation("sean-devlin", "s", "-- outfit\n");
        let (src, _) = linked_source("base\n", &[&m], &names(&["sean-devlin"])).unwrap();
        assert!(src.contains("appended by Shipment: sean-devlin"), "{src}");
    }

    fn edge(first: usize, then: usize) -> OrderEdge {
        OrderEdge { first, then }
    }

    /// With no edges the order IS the request order, and nothing is held back.
    #[test]
    fn with_no_edges_the_order_is_the_request_order() {
        let r = resolve_load_order(3, &[]).unwrap();
        assert_eq!(r.order, vec![0, 1, 2]);
        assert_eq!(r.held_back_by, vec![None, None, None]);
    }

    /// A consumer listed before its provider waits for it, and records the edge that held it back.
    #[test]
    fn a_requires_edge_beats_request_order() {
        // 0 = ess, 1 = lua-bridge; ess requires lua-bridge.
        let r = resolve_load_order(2, &[edge(1, 0)]).unwrap();
        assert_eq!(r.order, vec![1, 0]);
        assert_eq!(r.held_back_by, vec![Some(0), None]);
    }

    /// The held-back edge is the one from the LAST-emitted predecessor.
    #[test]
    fn held_back_names_the_last_emitted_predecessor() {
        // 0 = consumer of both 1 and 2; 1 and 2 have no edges between them.
        let r = resolve_load_order(3, &[edge(1, 0), edge(2, 0)]).unwrap();
        assert_eq!(r.order, vec![1, 2, 0]);
        assert_eq!(r.held_back_by[0], Some(1), "2 went last, so its edge (index 1) held 0 back");
    }

    /// A cycle has no order. Every edge on it is named, and an edge merely downstream of it is not.
    #[test]
    fn a_cycle_names_every_edge_on_it_and_only_those() {
        // 0 -> 1 -> 2 -> 0 is the cycle; 2 -> 3 hangs off it.
        let edges = [edge(0, 1), edge(1, 2), edge(2, 0), edge(2, 3)];
        let cycles = resolve_load_order(4, &edges).expect_err("a cycle has no order");
        assert_eq!(cycles, vec![vec![0, 1, 2]]);
    }

    /// Two separate cycles are two findings, each with its own edges.
    #[test]
    fn two_cycles_are_reported_separately() {
        let edges = [edge(0, 1), edge(1, 0), edge(2, 3), edge(3, 2)];
        let cycles = resolve_load_order(4, &edges).expect_err("cycles");
        assert_eq!(cycles, vec![vec![0, 1], vec![2, 3]]);
    }

    /// The resolved order actually reorders the appends: `b` ahead of `a` in `order` puts `b`'s source
    /// first, whatever the names sort to.
    #[test]
    fn the_order_reorders_the_appends() {
        let a = mutation("a-mod", "s", "-- A\n");
        let b = mutation("b-mod", "s", "-- B\n");
        let order = names(&["b-mod", "a-mod"]);
        let (forced, contrib) = linked_source("base\n", &[&a, &b], &order).unwrap();
        assert!(forced.find("-- B").unwrap() < forced.find("-- A").unwrap(), "{forced}");
        assert_eq!(contrib, vec!["b-mod", "a-mod"]);
    }

    /// A contributor the order does not name is an internal error, never a guessed position.
    #[test]
    fn a_contributor_missing_from_the_order_is_an_error() {
        let a = mutation("a-mod", "s", "-- A\n");
        match linked_source("base\n", &[&a], &names(&["other"])) {
            Err(LinkError::NotInOrder { shipment }) => assert_eq!(shipment, "a-mod"),
            other => panic!("expected NotInOrder, got {other:?}"),
        }
    }

    /// The synthetic trampoline is not a Shipment: it is never in `order`, and it goes last.
    #[test]
    fn the_modloader_contributor_sorts_last() {
        let t = mutation(MODLOADER_CONTRIBUTOR, "s", "-- T\n");
        let a = mutation("a-mod", "s", "-- A\n");
        let (src, contrib) = linked_source("base\n", &[&t, &a], &names(&["a-mod"])).unwrap();
        assert!(src.find("-- A").unwrap() < src.find("-- T").unwrap(), "{src}");
        assert_eq!(contrib, vec!["a-mod", MODLOADER_CONTRIBUTOR]);
    }

    fn reg(shipment: &str, movie: &str) -> UiRegistration {
        UiRegistration {
            shipment: shipment.into(),
            movie: movie.into(),
        }
    }

    /// The loader defines the global the trampoline depends on, exposes `run`, and enrols each
    /// movie's FlashWidget under the proven creation sequence.
    #[test]
    fn the_mod_loader_defines_qm_and_registers_each_movie() {
        let src = qm_modloader_source(
            &[reg("mod-a", "my_hud"), reg("mod-b", "my_map")],
            &[],
            &[],
            &[],
            &names(&["mod-a", "mod-b"]),
        )
        .unwrap();
        assert!(src.contains("_G._QM = _G._QM or {}"), "must publish the _QM global: {src}");
        assert!(src.contains("function _QM.run()"), "must expose run(): {src}");
        // Each movie enrols via the loadingscreen_standalone sequence.
        for movie in ["my_hud", "my_map"] {
            assert!(src.contains(&format!("w:SetSwfFile(\"{movie}\")")), "missing {movie}: {src}");
        }
        assert_eq!(src.matches("FlashWidget:new()").count(), 2, "one widget per movie: {src}");
        // Guarded so a re-import or a re-entered interior is a no-op, and one bad movie is contained.
        assert!(src.contains("if _QM._ran then return end"), "run must be once-only: {src}");
        assert!(src.contains("pcall(f)"), "each init must be fail-soft: {src}");
    }

    /// Same registrations and order, same bytes, whatever order the registrations arrive in.
    #[test]
    fn the_bake_is_independent_of_input_order() {
        let order = names(&["aaa", "zzz"]);
        let a = qm_modloader_source(&[reg("aaa", "one"), reg("zzz", "two")], &[], &[], &[], &order).unwrap();
        let b = qm_modloader_source(&[reg("zzz", "two"), reg("aaa", "one")], &[], &[], &[], &order).unwrap();
        assert_eq!(a, b, "input order must not change the baked loader");
        assert!(a.find("one").unwrap() < a.find("two").unwrap(), "{a}");
    }

    /// The bake follows the resolved order, not the names. `ess` is ahead of the consumer in
    /// `order` (the consumer requires it), so its registration is baked first even though the
    /// consumer's name sorts earlier.
    #[test]
    fn the_bake_follows_the_resolved_order() {
        let order = names(&["ess", "a-consumer"]);
        let src = qm_modloader_source(
            &[reg("a-consumer", "consumer_hud"), reg("ess", "ess_ui")],
            &[],
            &[],
            &[],
            &order,
        )
        .unwrap();
        assert!(
            src.find("ess_ui").unwrap() < src.find("consumer_hud").unwrap(),
            "ess must be registered before the Shipment that requires it: {src}"
        );
    }

    /// A movie name cannot break out of its Lua string and inject code into the block we compile.
    #[test]
    fn a_movie_name_is_escaped_in_the_bake() {
        let src =
            qm_modloader_source(&[reg("m", "evil\") os.exit() --")], &[], &[], &[], &names(&["m"])).unwrap();
        // The escaped form keeps the payload INSIDE the string literal ...
        assert!(src.contains("evil\\\") os.exit()"), "the embedded quote must be escaped: {src}");
        // ... and the unescaped breakout (a bare `evil") ` that would end the string early) is absent.
        assert!(!src.contains("(\"evil\") os"), "the injection must not close the string: {src}");
    }

    fn layer(shipment: &str, add: &str, remove: &[&str]) -> LayerRegistration {
        LayerRegistration {
            shipment: shipment.into(),
            add: add.into(),
            remove: remove.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// A layer activation bakes MarkForRemoval(old) then MarkForAddition(new) into the same guarded,
    /// pcall-wrapped `_QM._inits` list the UI widgets use — the vanilla contract order.
    #[test]
    fn the_mod_loader_marks_layers_for_addition_and_removal() {
        let src = qm_modloader_source(
            &[],
            &[layer("act-mod", "vz_state_pmccon004_destroyed", &["vz_state_pmccon004_pristine"])],
            &[],
            &[],
            &names(&["act-mod"]),
        )
        .unwrap();
        assert!(src.contains("_G._QM = _G._QM or {}"), "must still publish the _QM global: {src}");
        assert!(
            src.contains("MrxLayerManager.MarkForAddition(\"vz_state_pmccon004_destroyed\")"),
            "must add the new layer: {src}"
        );
        assert!(
            src.contains("MrxLayerManager.MarkForRemoval(\"vz_state_pmccon004_pristine\")"),
            "must remove the superseded layer: {src}"
        );
        // Remove precedes add — the vanilla order so the two never both apply.
        assert!(
            src.find("MarkForRemoval").unwrap() < src.find("MarkForAddition").unwrap(),
            "removal must be baked before addition: {src}"
        );
        // Guarded and fail-soft like the widgets.
        assert!(src.contains("if MrxLayerManager then"), "existence-checked: {src}");
        assert!(src.contains("pcall(f)"), "runs under the shared pcall: {src}");
    }

    /// UI and layer registrations coexist in one loader, and one order bakes byte-identically
    /// whatever order the registrations arrive in.
    #[test]
    fn ui_and_layer_registrations_share_one_deterministic_loader() {
        let order = names(&["aaa", "ui-mod", "zzz"]);
        let a = qm_modloader_source(
            &[reg("ui-mod", "my_hud")],
            &[layer("aaa", "layer_a", &[]), layer("zzz", "layer_z", &[])],
            &[],
            &[],
            &order,
        )
        .unwrap();
        let b = qm_modloader_source(
            &[reg("ui-mod", "my_hud")],
            &[layer("zzz", "layer_z", &[]), layer("aaa", "layer_a", &[])],
            &[],
            &[],
            &order,
        )
        .unwrap();
        assert_eq!(a, b, "input order must not change the baked loader");
        assert!(a.contains("w:SetSwfFile(\"my_hud\")"), "the widget is still baked: {a}");
        assert!(a.find("layer_a").unwrap() < a.find("layer_z").unwrap(), "ordered by load order: {a}");
    }

    /// A layer name cannot break out of its Lua string and inject code into the block we compile.
    #[test]
    fn a_layer_name_is_escaped_in_the_bake() {
        let src = qm_modloader_source(
            &[],
            &[layer("m", "evil\") os.exit() --", &[])],
            &[],
            &[],
            &names(&["m"]),
        )
        .unwrap();
        assert!(src.contains("evil\\\") os.exit()"), "the embedded quote must be escaped: {src}");
        assert!(!src.contains("Addition(\"evil\") os"), "the injection must not close the string: {src}");
    }

    fn sound_in(shipment: &str, bank: &str, soundbank: bool, sessions: &[LoadSession]) -> SoundBankRegistration {
        SoundBankRegistration {
            shipment: shipment.into(),
            bank: bank.into(),
            soundbank,
            sessions: sessions.iter().copied().collect(),
        }
    }

    fn sound(shipment: &str, bank: &str, soundbank: bool) -> SoundBankRegistration {
        sound_in(shipment, bank, soundbank, &[LoadSession::Gameplay])
    }

    use crate::manifest::LoadSession;

    /// The gameplay loader's `load_sounds` loads each bank as the retail load calls (the wavebank,
    /// then, for an added bank, its soundbank) in load order then bank name, whatever order the
    /// registrations arrive in; `unload_sounds` mirrors it; a bank name is escaped like any other;
    /// and a front-end registration is not in it.
    #[test]
    fn the_mod_loader_loads_gameplay_sound_banks_in_load_order() {
        let order = names(&["ess", "b-mod", "a-mod"]);
        let regs = [
            sound("a-mod", "a_sounds", true),
            sound("ess", "qm_ess_ui_hud", false),
            sound("b-mod", "zz_bank", true),
            sound("b-mod", "b_bank", true),
            sound_in("a-mod", "menu_only", true, &[LoadSession::FrontEnd]),
        ];
        let widget = [reg("ess", "ess_ui")];
        let src = qm_modloader_source(&widget, &[], &[], &regs, &order).unwrap();
        let mut reversed = regs.clone();
        reversed.reverse();
        assert_eq!(src, qm_modloader_source(&widget, &[], &[], &reversed, &order).unwrap());

        let at = |needle: &str| src.find(needle).unwrap_or_else(|| panic!("{needle} missing: {src}"));
        assert!(at("LoadWaveBank(\"qm_ess_ui_hud\")") < at("LoadWaveBank(\"b_bank\")"));
        assert!(at("LoadWaveBank(\"b_bank\")") < at("LoadWaveBank(\"zz_bank\")"));
        assert!(at("LoadWaveBank(\"zz_bank\")") < at("LoadWaveBank(\"a_sounds\")"));
        assert!(at("UnloadWaveBank(\"qm_ess_ui_hud\")") < at("UnloadWaveBank(\"b_bank\")"));
        assert!(at("UnloadWaveBank(\"zz_bank\")") < at("UnloadWaveBank(\"a_sounds\")"));
        assert!(at("LoadWaveBank(\"b_bank\")") < at("LoadSoundBank(\"b_bank\")"), "wavebank, then soundbank");
        assert!(!src.contains("LoadSoundBank(\"qm_ess_ui_hud\")"), "an override's wavebank only: {src}");
        assert_eq!(src.matches(".LoadSoundBank(").count(), 3);
        assert_eq!(src.matches(".UnloadSoundBank(").count(), 3);
        assert!(!src.contains("menu_only"), "a front-end bank is the front end's: {src}");
        assert!(at("function _QM.load_sounds()") < at("_QM._inits = {}"), "sounds are outside the once-run inits");
        assert!(src.contains("_QM.loaded[\"b_bank\"] = true"), "per-bank load state: {src}");
        assert!(!src.contains("if MrxSoundBanks"), "no existence check on the sound path: {src}");
        assert!(src.starts_with("-- ") && src.contains("\nimport(\"MrxSoundBanks\")\n"), "imports what it calls: {src}");

        let evil = qm_modloader_source(&[], &[], &[], &[sound("m\nos.exit()", "x\") os.exit() --", true)], &names(&["m\nos.exit()"]))
            .unwrap();
        assert!(evil.contains("x\\\") os.exit()"), "the embedded quote must be escaped: {evil}");
        assert!(!evil.contains("\nos.exit()"), "a Shipment name cannot break out of its comment: {evil}");
    }

    /// Both loaders' sound functions come from ONE generator: the front end's are the gameplay
    /// loader's with the state table renamed, for the same registrations.
    #[test]
    fn both_loaders_share_one_sound_generator() {
        let both = [LoadSession::Gameplay, LoadSession::FrontEnd];
        let regs = [sound_in("a", "qm_a_ui_hud", false, &both), sound_in("a", "added", true, &both)];
        let order = names(&["a"]);
        let gameplay = qm_modloader_source(&[], &[], &[], &regs, &order).unwrap();
        let front = qm_shell_modloader_source(&regs, &order).unwrap();
        let body = |src: &str, table: &str| -> String {
            let start = src.find(&format!("{table}.loaded = ")).expect("the generated functions");
            let end = src[start..].find("\nend\nfunction").map(|i| start + i).expect("load_sounds ends");
            let rest = &src[end + 1..];
            let unload_end = rest.find("\nend\n").expect("unload_sounds ends");
            format!("{}{}", &src[start..end + 1], &rest[..unload_end + 5]).replace(table, "T")
        };
        assert_eq!(body(&gameplay, "_QM"), body(&front, "_QMS"));
    }

    /// The front-end loader: `_G._QMS` with the front-end banks only, in load order then bank name;
    /// it imports `MrxSoundBanks` and carries no `pcall` and no existence check.
    #[test]
    fn the_front_end_loader_loads_front_end_banks_only() {
        let order = names(&["z-first", "a-second"]);
        let regs = [
            sound_in("a-second", "qm_a-second_ui_shell", false, &[LoadSession::FrontEnd]),
            sound_in("z-first", "zz", true, &[LoadSession::FrontEnd, LoadSession::Gameplay]),
            sound_in("z-first", "aa", true, &[LoadSession::FrontEnd]),
            sound_in("z-first", "gameplay_only", true, &[LoadSession::Gameplay]),
        ];
        let src = qm_shell_modloader_source(&regs, &order).unwrap();
        let at = |needle: &str| src.find(needle).unwrap_or_else(|| panic!("{needle} missing: {src}"));
        assert!(at("LoadWaveBank(\"aa\")") < at("LoadWaveBank(\"zz\")"));
        assert!(at("LoadWaveBank(\"zz\")") < at("LoadWaveBank(\"qm_a-second_ui_shell\")"));
        assert!(!src.contains("gameplay_only"), "{src}");
        assert!(src.contains("_G._QMS = _G._QMS or {}"), "{src}");
        assert!(src.contains("import(\"MrxSoundBanks\")"), "{src}");
        assert!(!src.contains("pcall") && !src.contains("if MrxSoundBanks") && !src.contains("if _QMS then"), "{src}");
        mercs2_luac::compile(&src, QM_SHELL_MODLOADER_NAME).expect("the front-end loader compiles");
        assert!(matches!(qm_shell_modloader_source(&regs, &names(&["z-first"])), Err(LinkError::NotInOrder { .. })));
    }

    /// A Lua VM with the module system retail's `_SYS._IMPORT` implements: each module runs in its
    /// own environment (`__index` → `_G`), and `import(m)` loads it once and binds it in the
    /// caller's environment. `MrxSoundBanks` records every call in the global `calls`, as does each
    /// host script's retail function. `sources` maps a module name to its source.
    fn module_vm(sources: &[(&str, String)]) -> mercs2_luac::rt::Lua {
        let lua = mercs2_luac::rt::Lua::new().expect("a Lua VM");
        let t = lua.create_table().unwrap();
        for (name, src) in sources {
            t.set(*name, src.as_str()).unwrap();
        }
        lua.globals().set("__sources", t).unwrap();
        lua.load(
            r#"
calls = {}
local loaded = {}
function import(name)
  local m = loaded[name]
  if not m then
    local src = __sources[name]
    if not src then error("import: no module " .. name) end
    m = setmetatable({}, { __index = _G })
    loaded[name] = m
    local f = assert(loadstring(src, name))
    setfenv(f, m)
    f()
  end
  getfenv(2)[name] = m
  return m
end
"#,
        )
        .exec()
        .expect("the module system");
        lua
    }

    const BANKS: &str = r#"
function LoadWaveBank(n, cb) table.insert(calls, "LoadWaveBank " .. n .. (cb and " +cb" or "")) end
function LoadSoundBank(n, cb) table.insert(calls, "LoadSoundBank " .. n .. (cb and " +cb" or "")) end
function UnloadWaveBank(n, cb) table.insert(calls, "UnloadWaveBank " .. n .. (cb and " +cb" or "")) end
function UnloadSoundBank(n, cb) table.insert(calls, "UnloadSoundBank " .. n .. (cb and " +cb" or "")) end
"#;

    fn calls(lua: &mercs2_luac::rt::Lua) -> Vec<String> {
        let t: mercs2_luac::rt::Table = lua.globals().get("calls").unwrap();
        t.sequence_values::<String>().map(|v| v.unwrap()).collect()
    }

    /// ★ The front-end trampoline, run: `EnterShellState` and `ExitShellState` each call retail's
    /// first, then load or unload the front-end banks in order, each bank once per load; a second
    /// `EnterShellState` loads nothing again, a second `ExitShellState` unloads nothing, and the
    /// next `EnterShellState` loads again. No callback is passed, so retail's batch callback stands.
    #[test]
    fn the_front_end_trampoline_wraps_and_chains_both_functions() {
        let order = names(&["a", "b"]);
        let regs = [
            sound_in("b", "qm_b_ui_hud", false, &[LoadSession::FrontEnd]),
            sound_in("a", "added", true, &[LoadSession::FrontEnd]),
        ];
        let host = format!(
            "function EnterShellState() table.insert(calls, \"retail EnterShellState\") end\n\
             function ExitShellState() table.insert(calls, \"retail ExitShellState\") end\n{}",
            qm_shell_trampoline_append()
        );
        let lua = module_vm(&[
            ("MrxSound", host),
            ("MrxSoundBanks", BANKS.to_string()),
            (QM_SHELL_MODLOADER_NAME, qm_shell_modloader_source(&regs, &order).unwrap()),
        ]);
        lua.load(
            "import(\"MrxSound\")\nMrxSound.EnterShellState()\nMrxSound.EnterShellState()\n\
             MrxSound.ExitShellState()\nMrxSound.ExitShellState()\nMrxSound.EnterShellState()\n",
        )
        .exec()
        .expect("the front end runs");
        assert_eq!(
            calls(&lua),
            [
                "retail EnterShellState",
                "LoadWaveBank added",
                "LoadSoundBank added",
                "LoadWaveBank qm_b_ui_hud",
                "retail EnterShellState",
                "retail ExitShellState",
                "UnloadWaveBank added",
                "UnloadSoundBank added",
                "UnloadWaveBank qm_b_ui_hud",
                "retail ExitShellState",
                "retail EnterShellState",
                "LoadWaveBank added",
                "LoadSoundBank added",
                "LoadWaveBank qm_b_ui_hud",
            ]
        );
        let t = qm_shell_trampoline_append();
        assert!(!t.contains("pcall") && !t.contains("if "), "no guard in the trampoline: {t}");
    }

    /// ★ The gameplay trampolines, run: `_OnEnter` calls retail's, then loads the gameplay banks on
    /// every entry — each bank once — and runs the once-only registrations; `ExitGame` calls
    /// retail's, then unloads what was loaded. `_QM` reaches the trampolines through `_G`, from
    /// modules that each run in their own environment.
    #[test]
    fn the_gameplay_trampolines_load_on_enter_and_unload_after_exit_game() {
        let regs = [sound("a", "qm_a_ui_hud", false), sound("a", "added", true)];
        let interior = format!(
            "function _OnEnter(n) table.insert(calls, \"retail _OnEnter \" .. n) end\n\
             function Enter(n) _OnEnter(n) end\n{}",
            qm_trampoline_append()
        );
        let bootstrap = format!(
            "function ExitGame() table.insert(calls, \"retail ExitGame\") end\n{}",
            qm_sound_exit_append()
        );
        let lua = module_vm(&[
            ("WifPmcInterior", interior),
            ("MrxSoundBootstrap", bootstrap),
            ("MrxSoundBanks", BANKS.to_string()),
            (QM_MODLOADER_NAME, qm_modloader_source(&[], &[], &[], &regs, &names(&["a"])).unwrap()),
        ]);
        lua.load(
            "import(\"WifPmcInterior\")\nimport(\"MrxSoundBootstrap\")\nWifPmcInterior.Enter(1)\n\
             WifPmcInterior.Enter(2)\nMrxSoundBootstrap.ExitGame()\n",
        )
        .exec()
        .expect("the session runs");
        assert_eq!(
            calls(&lua),
            [
                "retail _OnEnter 1",
                "LoadWaveBank added",
                "LoadSoundBank added",
                "LoadWaveBank qm_a_ui_hud",
                "retail _OnEnter 2",
                "retail ExitGame",
                "UnloadWaveBank added",
                "UnloadSoundBank added",
                "UnloadWaveBank qm_a_ui_hud",
            ]
        );
        for t in [qm_trampoline_append(), qm_sound_exit_append()] {
            assert!(!t.contains("pcall") && !t.contains("if "), "no guard in a trampoline: {t}");
        }
    }

    fn support(shipment: &str, id: &str, module: &str) -> SupportRegistration {
        SupportRegistration {
            shipment: shipment.into(),
            module: module.into(),
            source: "-- a novel MrxSupport subclass\n".into(),
            id: id.into(),
            name: format!("[{id}.name]"),
            description: format!("[{id}.desc]"),
            icon: "support_cluster_bomb".into(),
            item_type: "Airstrike".into(),
            cash_cost: 100000,
            fuel_cost: 50,
            max_stock: 8,
            unlock_table: "{ Pmc = 1 }".into(),
            cargo: None,
            delivery_vehicle: None,
            shops: vec!["Pmc".into()],
        }
    }

    /// A novel support behaviour DEFERS its whole catalog row into the loader: import the minted
    /// subclass, construct `oSupport`, write `tSupportData` live, re-apply the name the `Init` tail
    /// loop no longer runs for it, add the reward row, and null the never-invalidated shop cache —
    /// none of which can run at resident-load, which is the entire reason this path exists.
    #[test]
    fn the_mod_loader_defers_a_novel_support_behaviour() {
        let src = qm_modloader_source(
            &[],
            &[],
            &[support("bomb-mod", "ggbomb", "DLC_MrxGreenGoblinBomb")],
            &[],
            &names(&["bomb-mod"]),
        )
        .unwrap();
        assert!(src.contains("import(\"DLC_MrxGreenGoblinBomb\")"), "imports the subclass: {src}");
        assert!(src.contains("DLC_MrxGreenGoblinBomb:Create()"), "constructs oSupport: {src}");
        assert!(
            src.contains("oSupport:SetSupportName(\"ggbomb\")"),
            "re-applies the name Init would have: {src}"
        );
        assert!(
            src.contains("MrxSupportData.tSupportData[\"ggbomb\"]"),
            "defers the catalog row: {src}"
        );
        assert!(
            src.contains("MrxRewardData._tRewards[\"ggbombReward_Pmc\"]"),
            "defers the reward row: {src}"
        );
        assert!(
            src.contains("MrxRewardData.gtAllSupport = nil"),
            "nulls the never-invalidated cache: {src}"
        );
        assert!(src.contains("pcall(f)"), "runs under the shared fail-soft guard: {src}");
    }

    /// The trampoline is exactly what the resident carries: it wraps `_OnEnter`, imports the loader
    /// by name (so `import` hashes it to the ASET row `add_script` mints), and runs it once.
    #[test]
    fn the_trampoline_is_one_self_contained_hook() {
        let t = qm_trampoline_append();
        assert!(t.contains("_OnEnter = function"), "must wrap the entry hook: {t}");
        assert!(t.contains("import(\"qm_modloader\")"), "must import by name: {t}");
        assert!(t.contains("_QM.run()"), "must run the loader: {t}");
        // It calls the previous _OnEnter, so wrapping it never drops the game's own behaviour.
        assert!(t.contains("_qm_prev_OnEnter"), "must chain the prior hook: {t}");
    }
}
