//! Where the retail game ships each Scaleform movie: `data/retail_movies.tsv`, one row per (movie, WAD),
//! written by `mercs2_probe retail-movies` from the vanilla WADs' ASET tables. `add_movie` and `add_ui`
//! go where the movie they name ships, without reading the game: the front end mounts `shell.wad` and
//! `shell-patch.wad`, gameplay `vz.wad` and the overlay, so a replacement of a `shell.wad` movie has to
//! be in the shell patch to be seen.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use mercs2_formats::shader_registrations::tsv_rows;

const RETAIL_MOVIES_TSV: &str = include_str!("../data/retail_movies.tsv");

const HEADER: [&str; 3] = ["hash", "name", "wad"];

/// Where a movie's block goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieRoute {
    /// The overlay (gameplay): a `vz.wad` movie, or a movie retail does not ship.
    Overlay,
    /// The shell patch (front end): a `shell.wad` movie.
    Shell,
    /// Both: a movie both WADs ship.
    ShellAndOverlay,
}

/// Every retail movie hash and the WADs that ship it, lowercased. A malformed table is a build defect of
/// qm itself and panics naming it.
fn table() -> &'static BTreeMap<u32, Vec<String>> {
    static T: OnceLock<BTreeMap<u32, Vec<String>>> = OnceLock::new();
    T.get_or_init(|| parse(RETAIL_MOVIES_TSV).unwrap_or_else(|e| panic!("data/retail_movies.tsv: {e}")))
}

fn parse(text: &str) -> Result<BTreeMap<u32, Vec<String>>, String> {
    let body: String = text.lines().filter(|l| !l.starts_with('#')).map(|l| format!("{l}\n")).collect();
    let mut out: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for f in tsv_rows(&body, &HEADER) {
        let hash = f[0]
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| format!("hash {:?} is not 0x and 8 hex digits", f[0]))?;
        if f[1] != "-" && mercs2_formats::hash::pandemic_hash_m2(f[1]) != hash {
            return Err(format!("{} does not hash to {:?}", f[0], f[1]));
        }
        let wad = f[2].to_ascii_lowercase();
        if !wad.ends_with(".wad") {
            return Err(format!("{}: {:?} is not a WAD file name", f[0], f[2]));
        }
        let wads = out.entry(hash).or_default();
        if wads.contains(&wad) {
            return Err(format!("{} lists {} twice", f[0], f[2]));
        }
        wads.push(wad);
    }
    Ok(out)
}

/// The retail WADs that ship the movie `name`, lowercased; empty for a movie retail does not ship.
pub fn wads_shipping(name: &str) -> &'static [String] {
    table().get(&crate::manifest::asset_hash(name)).map_or(&[], Vec::as_slice)
}

/// Where the block of a movie named `name` goes. A movie retail ships only in a WAD qm writes no patch
/// for (`Loading.wad`) is an error naming it.
pub fn route(name: &str) -> Result<MovieRoute, String> {
    let wads = wads_shipping(name);
    let shell = wads.iter().any(|w| w == "shell.wad");
    let vz = wads.iter().any(|w| w == "vz.wad");
    match (shell, vz) {
        (true, true) => Ok(MovieRoute::ShellAndOverlay),
        (true, false) => Ok(MovieRoute::Shell),
        (false, true) => Ok(MovieRoute::Overlay),
        (false, false) if wads.is_empty() => Ok(MovieRoute::Overlay),
        (false, false) => Err(format!(
            "retail ships the movie {name:?} only in {}, and qm writes no patch for that WAD: a replacement \
             there would never be loaded",
            wads.join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_parses() {
        assert!(table().len() > 60, "{} movies", table().len());
    }

    #[test]
    fn each_movie_goes_where_retail_ships_it() {
        assert_eq!(route("SHELL"), Ok(MovieRoute::Shell), "the front end's menu");
        assert_eq!(route("MINIMAP"), Ok(MovieRoute::Overlay), "a gameplay movie");
        assert_eq!(route("loadingscreen"), Ok(MovieRoute::ShellAndOverlay), "both levels ship it");
        assert_eq!(route("qm_totally_novel_movie"), Ok(MovieRoute::Overlay), "a new movie, loaded from gameplay Lua");
        let e = route("loadingscreen_standalone").unwrap_err();
        assert!(e.contains("loading.wad"), "{e}");
    }

    #[test]
    fn a_malformed_row_is_named() {
        let head = "hash\tname\twad\n";
        assert!(parse(&format!("{head}0x51BFF7B1\tSHELL\tshell.wad\n")).is_ok());
        assert!(parse(&format!("{head}0x51BFF7B1\tNOTSHELL\tshell.wad\n")).unwrap_err().contains("does not hash"));
        assert!(parse(&format!("{head}51BFF7B1\t-\tshell.wad\n")).unwrap_err().contains("not 0x"));
        assert!(parse(&format!("{head}0x51BFF7B1\t-\tshell\n")).unwrap_err().contains("not a WAD"));
        let twice = format!("{head}0x51BFF7B1\t-\tshell.wad\n0x51BFF7B1\t-\tSHELL.WAD\n");
        assert!(parse(&twice).unwrap_err().contains("twice"));
    }
}
