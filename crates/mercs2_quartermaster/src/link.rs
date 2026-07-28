//! The Lua linker — the thing that lets two script mods coexist.
//!
//! Script entries load from the **block**, not per-hash, so editing one script means re-emitting all
//! 114. Under one-overlay-per-Shipment plus last-mounted-wins, two Shipments that each ship a
//! finished `scripts_vz` do not merge and do not error: the later one wins and the earlier one's Lua
//! vanishes — model, wardrobe row and all — with nothing reported. That single failure is why
//! `patch_lua` declares a *mutation* instead of shipping a block, and why this module exists.
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
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
}

/// Find a script's decompiled source under the corpus.
///
/// Searched in `vz`, `resident`, `shell`, then the stub directory — `vz/<name>.lua` is the mapping
/// that matters for `scripts_vz`, and the others are there so the same helper serves the resident
/// block later without growing a second lookup.
pub fn base_source_path(corpus_root: &Path, target: &str) -> Result<PathBuf, Vec<PathBuf>> {
    let mut tried = Vec::new();
    for sub in ["vz", "resident", "shell"] {
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

/// Concatenate the base source with every mutation's append, in a deterministic order.
///
/// **Ordered by Shipment name, not install order.** Two installs with the same set of Shipments must
/// produce byte-identical output, or verify-by-hash means nothing and a user's saved state can shift
/// under them between deploys. Load order decides who *wins* a conflict; it does not get to decide
/// the bytes of a merge that has no conflict.
pub fn linked_source(base: &str, mutations: &[&ScriptMutation]) -> (String, Vec<String>) {
    let mut ordered: Vec<&&ScriptMutation> = mutations.iter().collect();
    ordered.sort_by(|a, b| a.shipment.cmp(&b.shipment));

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
    (out, contributors)
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
    format!(
        "table.insert(_tOutfits.{wearer}, {{ Name = {}, Model = {}, PlayerVisibleName = {} }})\n",
        lua_string(slug),
        lua_string(model),
        lua_string(display),
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
pub fn link_into(
    block: &mut ScriptsBlock,
    corpus_root: &Path,
    mutations: &[ScriptMutation],
) -> Result<Vec<LinkedScript>, LinkError> {
    // Group by target so each script is compiled ONCE with all its appends, which is the entire
    // point — compiling per Shipment would mean the last one wins again, just more slowly.
    let mut by_target: BTreeMap<&str, Vec<&ScriptMutation>> = BTreeMap::new();
    for m in mutations {
        by_target.entry(m.target.as_str()).or_default().push(m);
    }

    let mut linked = Vec::new();
    for (target, group) in by_target {
        let idx = block
            .find_by_name(target)
            .ok_or_else(|| LinkError::UnknownScript {
                target: target.to_string(),
                shipment: group[0].shipment.clone(),
            })?;
        let source_path =
            base_source_path(corpus_root, target).map_err(|tried| LinkError::NoBaseSource {
                target: target.to_string(),
                tried,
            })?;
        let base = std::fs::read_to_string(&source_path).map_err(|e| LinkError::Compile {
            target: target.to_string(),
            message: format!("reading base source {}: {e}", source_path.display()),
        })?;

        let (mut source, contributors) = linked_source(&base, &group);
        // Emitted once, AFTER every Shipment's append — see `derived_epilogue`. Putting it here
        // rather than in each Shipment is what makes "exactly once" structural.
        if let Some(epilogue) = derived_epilogue(target) {
            source.push_str(&epilogue);
        }
        // BARE chunk name — see the module note. `@name.lua` produces a chunk 5 bytes off retail.
        let bytecode = mercs2_luac::compile(&source, target).map_err(|e| LinkError::Compile {
            target: target.to_string(),
            message: e,
        })?;
        block
            .replace_lua(idx, &bytecode)
            .map_err(|e| LinkError::Splice {
                target: target.to_string(),
                message: e,
            })?;

        linked.push(LinkedScript {
            target: target.to_string(),
            contributors,
            base_source_bytes: base.len(),
            linked_source_bytes: source.len(),
            bytecode_bytes: bytecode.len(),
        });
    }

    // The block must still verify after every splice. `replace_lua` recomputes each container's
    // CSUM, so a failure here means the block itself was left inconsistent.
    block
        .verify_csums()
        .map_err(|e| LinkError::Block(format!("CSUMs after linking: {e}")))?;
    Ok(linked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mutation(shipment: &str, target: &str, append: &str) -> ScriptMutation {
        ScriptMutation {
            shipment: shipment.into(),
            target: target.into(),
            append: append.into(),
        }
    }

    /// The ordering property the whole design rests on: same set of Shipments, same bytes, whatever
    /// order they arrive in.
    #[test]
    fn concatenation_order_is_deterministic_regardless_of_input_order() {
        let a = mutation("aaa-mod", "s", "-- A\n");
        let b = mutation("zzz-mod", "s", "-- Z\n");
        let (one, c1) = linked_source("base\n", &[&a, &b]);
        let (two, c2) = linked_source("base\n", &[&b, &a]);
        assert_eq!(one, two, "install order must not change the linked source");
        assert_eq!(c1, c2);
        assert_eq!(c1, vec!["aaa-mod", "zzz-mod"]);
        // Both appends must survive — this is the annihilation the linker exists to prevent.
        assert!(one.contains("-- A") && one.contains("-- Z"), "{one}");
    }

    /// A base script with no trailing newline must not weld into the first append.
    #[test]
    fn a_base_without_a_trailing_newline_is_separated() {
        let m = mutation("mod", "s", "print('x')\n");
        let (src, _) = linked_source("local t = 1", &[&m]);
        assert!(src.starts_with("local t = 1\n"), "{src}");
        assert!(
            !src.contains("local t = 1--"),
            "appended source welded onto the base: {src}"
        );
    }

    #[test]
    fn each_append_is_attributed_in_the_source() {
        let m = mutation("sean-devlin", "s", "-- outfit\n");
        let (src, _) = linked_source("base\n", &[&m]);
        assert!(src.contains("appended by Shipment: sean-devlin"), "{src}");
    }
}
