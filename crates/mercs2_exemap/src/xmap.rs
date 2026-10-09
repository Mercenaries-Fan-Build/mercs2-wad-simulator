//! The cross-image function map: translation of a function address between any two images by
//! composing the pairs of `exemap match` outputs.
//!
//! The input is a directory of `exemap match` outputs. A pairs file has the header
//! `<image_a>\t<image_b>\tevidence` and one row per accepted pair (`0x<a>\t0x<b>\t<evidence,...>`);
//! a conflicts file (header `side\tfunction\tcandidates`) names no images, so it is recognised and
//! not read. Any other file in the directory is an error. Each pairs file is one edge of the match
//! graph, whose nodes are images; an image pair may have only one file, in either orientation.
//!
//! A translation of function `f` from image `X` to image `Y` follows every simple path from `X` to
//! `Y` in the graph, mapping `f` through each edge's pairs in turn. A path either **answers** (every
//! hop has a pair) or **stops** at the first image whose function has no pair with the next image.
//! The translation is:
//!
//! * **agreed** when at least one path answers and every answering path gives the same function;
//! * **contradicted** when two answering paths give different functions — every answer is reported
//!   with the paths that gave it, and none is chosen;
//! * **untranslated** when no path answers, or the graph has no path between the two images.
//!
//! The direct path (the edge between `X` and `Y`, when there is one) is one path among the others,
//! so a direct pair and every composed path that answers are checked against each other.
//! Two functions of `X` that translate to the same function of `Y` along different paths show as
//! a contradiction of that function's translation from `Y` back to `X`, so [`Graph::coverage`],
//! which translates every function in both directions, reports that case too.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::matcher::Evidence;
use crate::Error;

/// The header of an `exemap match` conflicts file.
pub const CONFLICTS_HEADER: &str = "side\tfunction\tcandidates";

/// Parse a `0x`-prefixed hexadecimal function address.
pub fn parse_address(s: &str) -> Result<u32, Error> {
    let h = s.strip_prefix("0x").ok_or_else(|| Error::new(format!("address {s:?} is not 0x-prefixed")))?;
    u32::from_str_radix(h, 16).map_err(|e| Error::new(format!("address {s:?}: {e}")))
}

/// The accepted pairs of one `exemap match` output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pairs {
    pub image_a: String,
    pub image_b: String,
    /// `(a, b)` function pairs.
    pub pairs: Vec<(u32, u32)>,
}

/// Parse a pairs file. `what` names it in errors. Fails on a malformed row, an unknown evidence
/// name, and a function that occurs in two rows on the same side (the matcher accepts only 1:1
/// pairs).
pub fn parse_pairs(text: &str, what: &str) -> Result<Pairs, Error> {
    let mut lines = text.lines();
    let header = lines.next().ok_or_else(|| Error::new(format!("{what}: empty")))?;
    let h: Vec<&str> = header.split('\t').collect();
    if h.len() != 3 || h[2] != "evidence" || h[0].is_empty() || h[1].is_empty() {
        return Err(Error::new(format!("{what}: header {header:?} is not <image_a>\\t<image_b>\\tevidence")));
    }
    if h[0] == h[1] {
        return Err(Error::new(format!("{what}: both columns name image {}", h[0])));
    }
    let mut pairs = Vec::new();
    let (mut seen_a, mut seen_b) = (HashMap::new(), HashMap::new());
    for (i, l) in lines.enumerate() {
        let n = i + 2;
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() != 3 {
            return Err(Error::new(format!("{what} line {n}: {} fields, expected 3", f.len())));
        }
        let a = parse_address(f[0]).map_err(|e| Error::new(format!("{what} line {n}: {e}")))?;
        let b = parse_address(f[1]).map_err(|e| Error::new(format!("{what} line {n}: {e}")))?;
        if f[2].is_empty() {
            return Err(Error::new(format!("{what} line {n}: no evidence")));
        }
        for e in f[2].split(',') {
            if Evidence::parse(e).is_none() {
                return Err(Error::new(format!("{what} line {n}: unknown evidence {e:?}")));
            }
        }
        if let Some(prev) = seen_a.insert(a, n) {
            return Err(Error::new(format!("{what} line {n}: {} 0x{a:08x} is already paired on line {prev}", h[0])));
        }
        if let Some(prev) = seen_b.insert(b, n) {
            return Err(Error::new(format!("{what} line {n}: {} 0x{b:08x} is already paired on line {prev}", h[1])));
        }
        pairs.push((a, b));
    }
    Ok(Pairs { image_a: h[0].to_string(), image_b: h[1].to_string(), pairs })
}

/// The files of a matches directory, by kind.
#[derive(Clone, Debug, Default)]
pub struct MatchesDir {
    pub pairs: Vec<Pairs>,
    /// Conflicts files: recognised by their header, not read further.
    pub conflicts_files: usize,
}

/// Read every file of `dir`: each must be a pairs file whose images are in `images`, or a conflicts
/// file. Anything else (another file, a subdirectory) is an error naming it.
pub fn load_dir(dir: &Path, images: &crate::images::Images) -> Result<MatchesDir, Error> {
    let rd = std::fs::read_dir(dir).map_err(|e| Error::new(format!("read {}: {e}", dir.display())))?;
    let mut paths = Vec::new();
    for ent in rd {
        let ent = ent.map_err(|e| Error::new(format!("read {}: {e}", dir.display())))?;
        paths.push(ent.path());
    }
    paths.sort();
    let mut out = MatchesDir::default();
    for p in paths {
        if !p.is_file() {
            return Err(Error::new(format!("{}: not a file; a matches directory holds only exemap match outputs", p.display())));
        }
        let text = std::fs::read_to_string(&p).map_err(|e| Error::new(format!("read {}: {e}", p.display())))?;
        if text.lines().next() == Some(CONFLICTS_HEADER) {
            out.conflicts_files += 1;
            continue;
        }
        let what = p.display().to_string();
        let pairs = parse_pairs(&text, &what)?;
        for id in [&pairs.image_a, &pairs.image_b] {
            if images.image(id).is_none() {
                return Err(Error::new(format!("{what}: image {id} is not in images.tsv")));
            }
        }
        out.pairs.push(pairs);
    }
    Ok(out)
}

/// What one path gives for one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Walk {
    /// Every hop had a pair; the function in the last image.
    Answer(u32),
    /// The function `function` of image `path[at]` has no pair with image `path[at + 1]`.
    Stops { at: usize, function: u32 },
}

/// The outcome of translating one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The graph has no path between the two images.
    NoPath,
    /// No path answers.
    Untranslated,
    /// Every answering path gives this function.
    Agreed(u32),
    /// Answering paths disagree: each answer with the indices (into [`Translation::paths`]) of the
    /// paths that gave it.
    Contradicted(BTreeMap<u32, Vec<usize>>),
}

/// Every path between two images and what each gives for one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translation {
    pub paths: Vec<(Vec<String>, Walk)>,
}

impl Translation {
    pub fn verdict(&self) -> Verdict {
        if self.paths.is_empty() {
            return Verdict::NoPath;
        }
        let mut answers: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (i, (_, w)) in self.paths.iter().enumerate() {
            if let Walk::Answer(f) = w {
                answers.entry(*f).or_default().push(i);
            }
        }
        match answers.len() {
            0 => Verdict::Untranslated,
            1 => Verdict::Agreed(*answers.keys().next().unwrap()),
            _ => Verdict::Contradicted(answers),
        }
    }

    /// The direct path's walk, when the two images share an edge.
    pub fn direct(&self) -> Option<&Walk> {
        self.paths.iter().find(|(p, _)| p.len() == 2).map(|(_, w)| w)
    }

    /// How many composed paths (two hops or more) answer.
    pub fn composed_answers(&self) -> usize {
        self.paths.iter().filter(|(p, w)| p.len() > 2 && matches!(w, Walk::Answer(_))).count()
    }
}

/// The match graph: images as nodes, each pairs file as an edge usable in both directions.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    adjacent: BTreeMap<String, BTreeSet<String>>,
    /// `(from, to)` -> function of `from` -> function of `to`, both orientations of every edge.
    maps: HashMap<(String, String), HashMap<u32, u32>>,
}

/// One `(image, to)` row of [`Graph::coverage`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoverageRow {
    pub image: String,
    pub to: String,
    /// Functions of `image` that occur in any pairs file.
    pub functions: usize,
    /// Agreed translations.
    pub translated: usize,
    /// Agreed translations the direct edge gives.
    pub direct: usize,
    /// Agreed translations the direct edge gives and at least one composed path gives too.
    pub direct_confirmed: usize,
    /// Agreed translations no direct pair gives (no edge, or the edge has no pair for the function).
    pub composed_only: usize,
    /// Contradicted translations.
    pub contradicted: usize,
    /// Contradicted translations where the direct pair is one of the answers.
    pub contradicted_direct: usize,
    /// Functions no path translates.
    pub untranslated: usize,
}

/// One contradicted translation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contradiction {
    pub image: String,
    pub function: u32,
    pub to: String,
    /// Each answer with the paths that gave it.
    pub answers: BTreeMap<u32, Vec<Vec<String>>>,
}

impl Graph {
    /// Build the graph. An image pair given by two pairs files (in either orientation) is an error.
    pub fn new(sets: &[Pairs]) -> Result<Graph, Error> {
        let mut g = Graph::default();
        for s in sets {
            let (a, b) = (s.image_a.clone(), s.image_b.clone());
            if g.maps.contains_key(&(a.clone(), b.clone())) {
                return Err(Error::new(format!("two pairs files match {a} with {b}")));
            }
            g.maps.insert((a.clone(), b.clone()), s.pairs.iter().copied().collect());
            g.maps.insert((b.clone(), a.clone()), s.pairs.iter().map(|(x, y)| (*y, *x)).collect());
            g.adjacent.entry(a.clone()).or_default().insert(b.clone());
            g.adjacent.entry(b).or_default().insert(a);
        }
        Ok(g)
    }

    /// The images the graph spans.
    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.adjacent.keys().map(String::as_str)
    }

    /// Every simple path from `from` to `to`, shortest first, then by image ids.
    pub fn paths(&self, from: &str, to: &str) -> Vec<Vec<String>> {
        fn dfs(g: &Graph, to: &str, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
            let last = path.last().unwrap().clone();
            if last == to {
                out.push(path.clone());
                return;
            }
            let Some(next) = g.adjacent.get(&last) else { return };
            for n in next {
                if !path.contains(n) {
                    path.push(n.clone());
                    dfs(g, to, path, out);
                    path.pop();
                }
            }
        }
        let mut out = Vec::new();
        if from != to && self.adjacent.contains_key(from) {
            dfs(self, to, &mut vec![from.to_string()], &mut out);
        }
        out.sort_by(|x, y| x.len().cmp(&y.len()).then_with(|| x.cmp(y)));
        out
    }

    fn walk(&self, path: &[String], function: u32) -> Walk {
        let mut f = function;
        for (i, w) in path.windows(2).enumerate() {
            match self.maps[&(w[0].clone(), w[1].clone())].get(&f) {
                Some(g) => f = *g,
                None => return Walk::Stops { at: i, function: f },
            }
        }
        Walk::Answer(f)
    }

    fn translate_along(&self, paths: &[Vec<String>], function: u32) -> Translation {
        Translation { paths: paths.iter().map(|p| (p.clone(), self.walk(p, function))).collect() }
    }

    /// Translate `function` of image `from` to image `to` along every path.
    pub fn translate(&self, from: &str, function: u32, to: &str) -> Translation {
        self.translate_along(&self.paths(from, to), function)
    }

    /// The functions of `image` that occur in any pairs file.
    pub fn functions(&self, image: &str) -> BTreeSet<u32> {
        self.maps.iter().filter(|((a, _), _)| a == image).flat_map(|(_, m)| m.keys().copied()).collect()
    }

    /// Translate every function of every image to every other image.
    pub fn coverage(&self) -> (Vec<CoverageRow>, Vec<Contradiction>) {
        let mut rows = Vec::new();
        let mut contradictions = Vec::new();
        for image in self.images() {
            let functions = self.functions(image);
            for to in self.images().filter(|t| *t != image) {
                let paths = self.paths(image, to);
                let mut r = CoverageRow { image: image.into(), to: to.into(), functions: functions.len(), ..Default::default() };
                for &f in &functions {
                    let t = self.translate_along(&paths, f);
                    let direct = t.direct().and_then(|w| match w {
                        Walk::Answer(g) => Some(*g),
                        Walk::Stops { .. } => None,
                    });
                    match t.verdict() {
                        Verdict::NoPath | Verdict::Untranslated => r.untranslated += 1,
                        Verdict::Agreed(_) => {
                            r.translated += 1;
                            if direct.is_some() {
                                r.direct += 1;
                                if t.composed_answers() > 0 {
                                    r.direct_confirmed += 1;
                                }
                            } else {
                                r.composed_only += 1;
                            }
                        }
                        Verdict::Contradicted(answers) => {
                            r.contradicted += 1;
                            if direct.is_some() {
                                r.contradicted_direct += 1;
                            }
                            contradictions.push(Contradiction {
                                image: image.into(),
                                function: f,
                                to: to.into(),
                                answers: answers
                                    .into_iter()
                                    .map(|(g, idx)| (g, idx.into_iter().map(|i| t.paths[i].0.clone()).collect()))
                                    .collect(),
                            });
                        }
                    }
                }
                rows.push(r);
            }
        }
        (rows, contradictions)
    }
}

/// `a>b>c`: a path as the commands print it.
pub fn path_str(path: &[String]) -> String {
    path.join(">")
}

/// The coverage TSV header.
pub const COVERAGE_HEADER: &str =
    "image\tto\tfunctions\ttranslated\tdirect\tdirect_confirmed\tcomposed_only\tcontradicted\tcontradicted_direct\tuntranslated";

/// The contradictions TSV header.
pub const CONTRADICTIONS_HEADER: &str = "image\tfunction\tto\tanswers";

impl CoverageRow {
    pub fn tsv(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.image,
            self.to,
            self.functions,
            self.translated,
            self.direct,
            self.direct_confirmed,
            self.composed_only,
            self.contradicted,
            self.contradicted_direct,
            self.untranslated
        )
    }
}

impl Contradiction {
    /// `image  0x<function>  to  0x<answer>(path,path) 0x<answer>(path)`.
    pub fn tsv(&self) -> String {
        let answers: Vec<String> = self
            .answers
            .iter()
            .map(|(g, ps)| format!("0x{g:08x}({})", ps.iter().map(|p| path_str(p)).collect::<Vec<_>>().join(",")))
            .collect();
        format!("{}\t0x{:08x}\t{}\t{}", self.image, self.function, self.to, answers.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(a: &str, b: &str, p: &[(u32, u32)]) -> Pairs {
        Pairs { image_a: a.into(), image_b: b.into(), pairs: p.to_vec() }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parse_reads_the_match_output_and_rejects_what_is_not_one() {
        let p = parse_pairs("x\ty\tevidence\n0x00000001\t0x0000000b\tbytes,callee\n", "t").unwrap();
        assert_eq!(p, pairs("x", "y", &[(1, 11)]));
        for bad in [
            "side\tfunction\tcandidates\n",
            "x\tx\tevidence\n",
            "x\ty\tevidence\n0x1\t0xb\n",
            "x\ty\tevidence\n1\t0xb\tbytes\n",
            "x\ty\tevidence\n0x1\t0xb\tguess\n",
            "x\ty\tevidence\n0x1\t0xb\t\n",
            "x\ty\tevidence\n0x1\t0xb\tbytes\n0x1\t0xc\tbytes\n",
            "x\ty\tevidence\n0x1\t0xb\tbytes\n0x2\t0xb\tbytes\n",
        ] {
            assert!(parse_pairs(bad, "t").is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_image_pair_given_twice_is_an_error() {
        let e = Graph::new(&[pairs("x", "y", &[]), pairs("y", "x", &[])]).unwrap_err();
        assert!(e.to_string().contains("two pairs files"), "{e}");
    }

    #[test]
    fn paths_are_every_simple_path_shortest_first() {
        let g = Graph::new(&[pairs("a", "b", &[]), pairs("b", "c", &[]), pairs("a", "c", &[]), pairs("c", "d", &[])]).unwrap();
        assert_eq!(g.paths("a", "c"), vec![s(&["a", "c"]), s(&["a", "b", "c"])]);
        assert_eq!(g.paths("d", "b"), vec![s(&["d", "c", "b"]), s(&["d", "c", "a", "b"])]);
        assert!(g.paths("a", "e").is_empty());
        assert!(g.paths("a", "a").is_empty());
    }

    #[test]
    fn composition_follows_each_hop_in_its_own_orientation() {
        // a -> b given as (a, b); c given as (c, b), so the second hop reads the file backwards.
        let g = Graph::new(&[pairs("a", "b", &[(1, 11), (2, 12)]), pairs("c", "b", &[(21, 11)])]).unwrap();
        let t = g.translate("a", 1, "c");
        assert_eq!(t.paths, vec![(s(&["a", "b", "c"]), Walk::Answer(21))]);
        assert_eq!(t.verdict(), Verdict::Agreed(21));
        assert_eq!(g.translate("c", 21, "a").verdict(), Verdict::Agreed(1));
        // 2 reaches b as 12, which has no pair with c.
        let t = g.translate("a", 2, "c");
        assert_eq!(t.paths[0].1, Walk::Stops { at: 1, function: 12 });
        assert_eq!(t.verdict(), Verdict::Untranslated);
        assert_eq!(g.translate("a", 99, "c").paths[0].1, Walk::Stops { at: 0, function: 99 });
    }

    #[test]
    fn no_path_is_not_the_same_as_no_answer() {
        let g = Graph::new(&[pairs("a", "b", &[(1, 11)]), pairs("c", "d", &[(5, 15)])]).unwrap();
        assert_eq!(g.translate("a", 1, "d").verdict(), Verdict::NoPath);
        assert_eq!(g.translate("a", 2, "b").verdict(), Verdict::Untranslated);
    }

    #[test]
    fn a_direct_pair_and_a_composed_path_that_agree() {
        let g = Graph::new(&[pairs("a", "b", &[(1, 11)]), pairs("b", "c", &[(11, 21)]), pairs("a", "c", &[(1, 21)])]).unwrap();
        let t = g.translate("a", 1, "c");
        assert_eq!(t.verdict(), Verdict::Agreed(21));
        assert_eq!(t.direct(), Some(&Walk::Answer(21)));
        assert_eq!(t.composed_answers(), 1);
    }

    #[test]
    fn a_direct_pair_that_a_composed_path_contradicts() {
        let g = Graph::new(&[pairs("a", "b", &[(1, 11)]), pairs("b", "c", &[(11, 22)]), pairs("a", "c", &[(1, 21)])]).unwrap();
        let t = g.translate("a", 1, "c");
        let mut want = BTreeMap::new();
        want.insert(21, vec![0]);
        want.insert(22, vec![1]);
        assert_eq!(t.verdict(), Verdict::Contradicted(want));
    }

    #[test]
    fn a_path_that_stops_does_not_count_against_one_that_answers() {
        let g = Graph::new(&[pairs("a", "b", &[]), pairs("b", "c", &[(11, 22)]), pairs("a", "c", &[(1, 21)])]).unwrap();
        let t = g.translate("a", 1, "c");
        assert_eq!(t.verdict(), Verdict::Agreed(21));
        assert_eq!(t.composed_answers(), 0);
    }

    #[test]
    fn coverage_counts_every_class_and_lists_contradictions_from_both_sides() {
        // a1 -> c: direct 21, via b 22 (contradicted, direct among the answers).
        // a2 -> c: direct 23, via b 23 (agreed, confirmed).
        // a3 -> c: only via b (composed only).
        // a4 -> c: direct only (agreed, unconfirmed).
        // a5: paired with b only, stops at c.
        let g = Graph::new(&[
            pairs("a", "b", &[(1, 11), (2, 12), (3, 13), (5, 15)]),
            pairs("b", "c", &[(11, 22), (12, 23), (13, 24)]),
            pairs("a", "c", &[(1, 21), (2, 23), (4, 25)]),
        ])
        .unwrap();
        let (rows, cs) = g.coverage();
        let ac = rows.iter().find(|r| r.image == "a" && r.to == "c").unwrap();
        assert_eq!(
            ac,
            &CoverageRow {
                image: "a".into(),
                to: "c".into(),
                functions: 5,
                translated: 3,
                direct: 2,
                direct_confirmed: 1,
                composed_only: 1,
                contradicted: 1,
                contradicted_direct: 1,
                untranslated: 1,
            }
        );
        let a1 = cs.iter().find(|c| c.image == "a" && c.function == 1 && c.to == "c").unwrap();
        assert_eq!(a1.tsv(), "a\t0x00000001\tc\t0x00000015(a>c) 0x00000016(a>b>c)");
        // b11 -> c: direct 22, via a 21 — the same inconsistency seen from b.
        assert!(cs.iter().any(|c| c.image == "b" && c.function == 11 && c.to == "c"));
    }

    #[test]
    fn two_functions_meeting_in_one_shows_as_a_contradiction_on_the_way_back() {
        // a1 -> c21 directly, a2 -> b12 -> c21: both forward translations agree on their own
        // paths, and c21 -> a disagrees (1 directly, 2 via b).
        let g = Graph::new(&[pairs("a", "b", &[(2, 12)]), pairs("b", "c", &[(12, 21)]), pairs("a", "c", &[(1, 21)])]).unwrap();
        assert_eq!(g.translate("a", 1, "c").verdict(), Verdict::Agreed(21));
        assert_eq!(g.translate("a", 2, "c").verdict(), Verdict::Agreed(21));
        let (_, cs) = g.coverage();
        let back: Vec<String> = cs.iter().filter(|c| c.image == "c").map(Contradiction::tsv).collect();
        assert_eq!(back, vec!["c\t0x00000015\ta\t0x00000001(c>a) 0x00000002(c>b>a)".to_string()]);
    }

    #[test]
    fn addresses_must_be_0x_hex() {
        assert_eq!(parse_address("0x8239cd98").unwrap(), 0x8239cd98);
        assert!(parse_address("8239cd98").is_err());
        assert!(parse_address("0xzz").is_err());
    }
}
