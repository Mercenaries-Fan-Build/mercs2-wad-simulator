//! qm's edit of the front end's SHELL movie, which makes the Extras ▸ Downloadable Content page list the
//! installed levels on PC and start the one chosen ([`crate::link::extras_page_source`] fills the page).
//!
//! The edit is data, `data/shell_extras.tsv`, applied in its order to the player's own SHELL, read out of
//! `shell.wad`: the retail movie is never shipped, and a Shipment never ships a SHELL of its own for this. The
//! movie read must be retail's (its sha256), and the movie written must be the reviewed result (its sha256):
//! a SHELL qm did not write the edit against fails the link instead of shipping a movie nobody has seen.

use mercs2_formats::gfx_actions::{check_decodes, parse_patches, patch_actions, swap_constant, Target};
use sha2::{Digest, Sha256};

/// SHELL's asset name, which `mrxguishell` opens (`GetShellGfxFilename` → `shell.gfx`).
pub const SHELL: &str = "SHELL";

/// The sha256 of retail SHELL as `shell.wad` carries it (the movie, not its `cfx_pack` container).
pub const RETAIL_SHELL_SHA256: &str = "ca06957d6184d6f460b9549844fed999779f53d1ca825d86c13436269015c851";

/// The sha256 of SHELL with [`RECIPE`] applied: the movie played through Extras ▸ Downloadable Content into
/// `dlc01` live on 2026-11-20.
pub const EDITED_SHELL_SHA256: &str = "7a35577c6393d9c5c000db119a5ed7cd4d0e4d3bf1ff7fc25c5df4b9827afa1e";

/// The edit, one operation per line: `swap <targets> <from> <to>` or `patch <target> <find> <replace> <count>`.
pub const RECIPE: &str = include_str!("../data/shell_extras.tsv");

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// `retail` with [`RECIPE`] applied. `retail` must be retail SHELL, and the result is checked against
/// [`EDITED_SHELL_SHA256`].
pub fn edit(retail: &[u8]) -> Result<Vec<u8>, String> {
    let got = sha256_hex(retail);
    if got != RETAIL_SHELL_SHA256 {
        return Err(format!(
            "shell.wad's SHELL has sha256 {got}; qm's Extras edit is written against retail's {RETAIL_SHELL_SHA256}"
        ));
    }
    let mut movie = retail.to_vec();
    let mut patches = String::new();
    // Patch rows apply together, after the swaps before them, in the recipe's order.
    let flush = |movie: &mut Vec<u8>, patches: &mut String| -> Result<(), String> {
        if patches.is_empty() {
            return Ok(());
        }
        let (out, _) = patch_actions(movie, &parse_patches(patches)?)?;
        *movie = out;
        patches.clear();
        Ok(())
    };
    for (n, line) in RECIPE.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fail = |e: String| format!("data/shell_extras.tsv line {}: {e}", n + 1);
        match line.split_once('\t') {
            Some(("swap", rest)) => {
                flush(&mut movie, &mut patches).map_err(fail)?;
                let f: Vec<&str> = rest.split('\t').collect();
                let [targets, from, to] = f[..] else {
                    return Err(fail(format!("swap has {} fields, not 3 (targets, from, to)", f.len())));
                };
                let targets = targets.split(',').map(Target::parse).collect::<Result<Vec<_>, _>>().map_err(fail)?;
                movie = swap_constant(&movie, &targets, from, to).map_err(fail)?.0;
            }
            Some(("patch", rest)) => {
                patches.push_str(rest);
                patches.push('\n');
            }
            _ => return Err(fail(format!("{line:?} is neither a swap nor a patch row"))),
        }
    }
    flush(&mut movie, &mut patches)?;
    check_decodes(&movie)?;
    let got = sha256_hex(&movie);
    if got != EDITED_SHELL_SHA256 {
        return Err(format!("qm's Extras edit of SHELL came out {got}, not the reviewed {EDITED_SHELL_SHA256}"));
    }
    Ok(movie)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_movie_that_is_not_retail_shell_is_refused() {
        let e = edit(b"CFX\x08\x00\x00\x00\x00").unwrap_err();
        assert!(e.contains("qm's Extras edit is written against retail's"), "{e}");
    }

    #[test]
    fn the_recipe_reads_as_swaps_and_patches() {
        let rows: Vec<&str> = RECIPE.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect();
        assert!(rows.iter().all(|l| l.starts_with("swap\t") || l.starts_with("patch\t")), "{rows:?}");
        let patches: String = rows.iter().filter_map(|l| l.strip_prefix("patch\t")).map(|l| format!("{l}\n")).collect();
        assert_eq!(parse_patches(&patches).unwrap().len(), 4);
    }
}
