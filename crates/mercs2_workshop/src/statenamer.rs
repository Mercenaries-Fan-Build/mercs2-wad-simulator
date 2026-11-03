//! Naming the table values a clip is selected by — its Stance, Action and the other cells its
//! label shows as hex — while the clip plays.
//!
//! **Homes.** A cracked name is written to both committed homes of cracked names: the notes repo's
//! `docs/data/aset_discovered_names.json` (`notes_docs_dir` in `.mercs2-local.toml`, searched upward
//! from this crate's checkout) and this crate's `data/aset_discovered_names.json`. The two are kept
//! byte-identical: [`NameHomes::load`] and [`NameHomes::append`] refuse when they differ. Both live in
//! a checkout, so a released binary has no homes and [`StateNamer::open`] says so.
//!
//! **Append** ([`append_entry`]). The name's `pandemic_hash_m2` is its key (`0x%08X`); the entry is
//! added after the last one, in the file's `indent=2` layout, and the file keeps ending in `}` with
//! no newline. A key the file already holds is refused, as is a name that is not printable ASCII.
//! The result is parsed again and every entry checked to hash to its key before anything is written.
//!
//! **Word orders** ([`word_orders`]). The words typed (split on whitespace and commas), joined with
//! nothing and with `_` in the order typed and, up to [`MAX_WORDS`] words, in every other order. The
//! hash folds case, so case does not matter. Only an exact hash match is ever shown ([`matches`]).
//!
//! **Context** ([`Consumers`]). The notes repo's `docs/data/hash_candidates_by_consumer.tsv`
//! (written by `exemap hash-join`): for a value, the consumers that hold it and their known names.
//!
//! **Proposals** ([`Proposals`]). The notes repo's `docs/data/clip_name_proposals.tsv` (written by
//! `mercs2_probe clip-names`): for a clip, the names its table rows crack to, each with its evidence.
//! A proposal is only a candidate until Accept writes it to the homes.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use mercs2_formats::hash::pandemic_hash_m2_bytes;

/// The file both homes hold.
pub const NAMES_FILE: &str = "aset_discovered_names.json";
/// The consumer summary in the notes repo's `docs/data/`.
pub const CONSUMERS_FILE: &str = "hash_candidates_by_consumer.tsv";
/// The clip-name proposals in the notes repo's `docs/data/`.
pub const PROPOSALS_FILE: &str = "clip_name_proposals.tsv";
/// The most words [`word_orders`] permutes (5! × 2 = 240 candidates); more are tried as typed only.
pub const MAX_WORDS: usize = 5;

/// The two committed homes of cracked names.
#[derive(Clone, Debug)]
pub struct NameHomes {
    pub docs: PathBuf,
    pub krate: PathBuf,
}

impl NameHomes {
    /// Both homes, found from the checkout at `start`. An error naming what is missing.
    pub fn locate(start: &Path) -> Result<NameHomes, String> {
        let notes_docs = mercs2_formats::game_paths::local_config_notes_docs_dir(start)?;
        let docs = crate::index::notes_data_file(&notes_docs, NAMES_FILE)?;
        let krate = crate::index::crate_data_file(NAMES_FILE)?;
        Ok(NameHomes { docs, krate })
    }

    /// The text both homes hold; an error when they differ or either cannot be read.
    fn text(&self) -> Result<String, String> {
        let a = std::fs::read_to_string(&self.docs).map_err(|e| format!("{}: {e}", self.docs.display()))?;
        let b = std::fs::read_to_string(&self.krate).map_err(|e| format!("{}: {e}", self.krate.display()))?;
        if a != b {
            return Err(format!(
                "{} and {} differ; they are kept identical, so reconcile them before naming",
                self.docs.display(),
                self.krate.display()
            ));
        }
        Ok(a)
    }

    /// Every name the homes hold, by hash.
    pub fn load(&self) -> Result<HashMap<u32, String>, String> {
        let text = self.text()?;
        parse_names(&text, &self.docs.display().to_string())
    }

    /// Add `name` to both homes; returns its hash.
    pub fn append(&self, name: &str) -> Result<u32, String> {
        let text = self.text()?;
        let hash = pandemic_hash_m2_bytes(name.as_bytes());
        let out = append_entry(&text, hash, name)?;
        for p in [&self.docs, &self.krate] {
            std::fs::write(p, &out).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        Ok(hash)
    }
}

/// Parse a names file: `{"pandemic_hash_m2": {"0x%08X": [name, …]}}`, every name hashing to its key.
/// The first name of each key is kept.
pub fn parse_names(text: &str, what: &str) -> Result<HashMap<u32, String>, String> {
    let root: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("{what}: {e}"))?;
    let obj = root
        .get("pandemic_hash_m2")
        .and_then(|v| v.as_object())
        .ok_or_else(|| format!("{what}: no \"pandemic_hash_m2\" object"))?;
    let mut out = HashMap::new();
    for (k, v) in obj {
        let h = k
            .strip_prefix("0x")
            .filter(|h| h.len() == 8)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| format!("{what}: key {k:?} is not 0x and eight hex digits"))?;
        let names = v.as_array().ok_or_else(|| format!("{what}: {k} is not an array of names"))?;
        for n in names {
            let n = n.as_str().ok_or_else(|| format!("{what}: {k} holds a name that is not a string"))?;
            if pandemic_hash_m2_bytes(n.as_bytes()) != h {
                return Err(format!("{what}: {n:?} does not hash to its key {k}"));
            }
            out.entry(h).or_insert_with(|| n.to_string());
        }
    }
    Ok(out)
}

/// `text` with `name` added under `hash` (see the module documentation).
pub fn append_entry(text: &str, hash: u32, name: &str) -> Result<String, String> {
    if name.is_empty() || !name.bytes().all(|b| (0x20..=0x7e).contains(&b)) || name.contains(['"', '\\']) {
        return Err(format!("{name:?} is not a printable ASCII name without quotes or backslashes"));
    }
    if pandemic_hash_m2_bytes(name.as_bytes()) != hash {
        return Err(format!("{name:?} does not hash to 0x{hash:08X}"));
    }
    let before = parse_names(text, "the names file")?;
    if let Some(n) = before.get(&hash) {
        return Err(format!("0x{hash:08X} is already named {n:?}"));
    }
    const TAIL: &str = "\n    ]\n  }\n}";
    let head = text
        .strip_suffix(TAIL)
        .ok_or_else(|| "the names file does not end with its last entry's `]`, `}` and `}` in the indent=2 layout".to_string())?;
    let out = format!("{head}\n    ],\n    \"0x{hash:08X}\": [\n      \"{name}\"{TAIL}");
    let after = parse_names(&out, "the appended names file")?;
    if after.len() != before.len() + 1 || after.get(&hash).map(String::as_str) != Some(name) {
        return Err(format!("appending 0x{hash:08X} {name:?} did not yield exactly one new entry"));
    }
    Ok(out)
}

/// The typed words as typed and, up to [`MAX_WORDS`], in every other order, joined with nothing and
/// with `_` (see the module documentation).
pub fn word_orders(input: &str) -> Vec<String> {
    let words: Vec<&str> = input.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()).collect();
    let mut out: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut idx: Vec<usize> = (0..words.len()).collect();
    loop {
        for sep in ["", "_"] {
            let s = idx.iter().map(|&i| words[i]).collect::<Vec<_>>().join(sep);
            if !s.is_empty() && seen.insert(s.to_ascii_lowercase()) {
                out.push(s);
            }
        }
        if words.len() > MAX_WORDS || !next_permutation(&mut idx) {
            break;
        }
    }
    out
}

/// Step `v` to its next lexicographic permutation; `false` after the last.
fn next_permutation(v: &mut [usize]) -> bool {
    let Some(i) = (1..v.len()).rev().find(|&i| v[i - 1] < v[i]) else {
        return false;
    };
    let j = (i..v.len()).rev().find(|&j| v[j] > v[i - 1]).expect("v[i] > v[i - 1]");
    v.swap(i - 1, j);
    v[i..].reverse();
    true
}

/// The [`word_orders`] of `input` whose hash is `value`.
pub fn matches(input: &str, value: u32) -> Vec<String> {
    word_orders(input).into_iter().filter(|s| pandemic_hash_m2_bytes(s.as_bytes()) == value).collect()
}

/// One proposed clip name and its evidence, as `mercs2_probe clip-names` wrote it.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub name: String,
    /// `phase · expected chance · stance agrees · family support`.
    pub evidence: String,
}

/// Per clip, its proposed names.
#[derive(Debug, Default)]
pub struct Proposals {
    by_clip: HashMap<u32, Vec<Proposal>>,
}

impl Proposals {
    /// Parse the review TSV (`hash name phase expected_chance stance_agrees family_support …`). An
    /// error when a column is missing, a hash is not `0x`-hex, or a name does not hash to its clip.
    pub fn parse(text: &str, what: &str) -> Result<Proposals, String> {
        let mut lines = text.lines();
        let header = lines.next().ok_or_else(|| format!("{what}: empty"))?;
        let cols: Vec<&str> = header.split('\t').collect();
        let col = |n: &str| cols.iter().position(|c| *c == n).ok_or_else(|| format!("{what}: no column {n:?}"));
        let (c_hash, c_name, c_phase, c_chance, c_stance, c_support) = (
            col("hash")?,
            col("name")?,
            col("phase")?,
            col("expected_chance")?,
            col("stance_agrees")?,
            col("family_support")?,
        );
        let mut by_clip: HashMap<u32, Vec<Proposal>> = HashMap::new();
        for (i, l) in lines.enumerate() {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() != cols.len() {
                return Err(format!("{what} line {}: {} fields, the header has {}", i + 2, f.len(), cols.len()));
            }
            let h = f[c_hash]
                .strip_prefix("0x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .ok_or_else(|| format!("{what} line {}: hash {:?} is not 0x-hex", i + 2, f[c_hash]))?;
            let name = f[c_name];
            if pandemic_hash_m2_bytes(name.as_bytes()) != h {
                return Err(format!("{what} line {}: {name:?} does not hash to 0x{h:08X}", i + 2));
            }
            by_clip.entry(h).or_default().push(Proposal {
                name: name.to_string(),
                evidence: format!(
                    "{} · chance {} · stance {} · support {}",
                    f[c_phase], f[c_chance], f[c_stance], f[c_support]
                ),
            });
        }
        Ok(Proposals { by_clip })
    }

    pub fn of(&self, clip: u32) -> &[Proposal] {
        self.by_clip.get(&clip).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Per value, the consumers the hash-join summary lists it under: `"<consumer> · <known names>"`.
#[derive(Debug, Default)]
pub struct Consumers {
    by_value: HashMap<u32, Vec<String>>,
}

impl Consumers {
    /// Parse the summary TSV (`kind consumer images known known_names uncracked uncracked_values
    /// cracked_by_context sites`).
    pub fn parse(text: &str, what: &str) -> Result<Consumers, String> {
        let mut lines = text.lines();
        let header = lines.next().ok_or_else(|| format!("{what}: empty"))?;
        let cols: Vec<&str> = header.split('\t').collect();
        let col = |n: &str| cols.iter().position(|c| *c == n).ok_or_else(|| format!("{what}: no column {n:?}"));
        let (c_consumer, c_known, c_values) = (col("consumer")?, col("known_names")?, col("uncracked_values")?);
        let mut by_value: HashMap<u32, Vec<String>> = HashMap::new();
        for (i, l) in lines.enumerate() {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() != cols.len() {
                return Err(format!("{what} line {}: {} fields, the header has {}", i + 2, f.len(), cols.len()));
            }
            if f[c_values] == "-" {
                continue;
            }
            for v in f[c_values].split(' ') {
                let h = v
                    .strip_prefix("0x")
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .ok_or_else(|| format!("{what} line {}: value {v:?} is not 0x-hex", i + 2))?;
                by_value.entry(h).or_default().push(format!("{} · {}", f[c_consumer], f[c_known]));
            }
        }
        Ok(Consumers { by_value })
    }

    pub fn of(&self, value: u32) -> &[String] {
        self.by_value.get(&value).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// What the clip panel names with: the homes it writes, the consumer context it shows, and the clip
/// names proposed for review (or why there are none).
pub struct StateNamer {
    pub homes: NameHomes,
    pub consumers: Consumers,
    pub proposals: Result<Proposals, String>,
}

impl StateNamer {
    /// Open from this crate's checkout; returns the namer and the names its homes already hold.
    pub fn open() -> Result<(StateNamer, HashMap<u32, String>), String> {
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        let homes = NameHomes::locate(start)?;
        let names = homes.load()?;
        let notes_docs = mercs2_formats::game_paths::local_config_notes_docs_dir(start)?;
        let tsv = crate::index::notes_data_file(&notes_docs, CONSUMERS_FILE)?;
        let text = std::fs::read_to_string(&tsv).map_err(|e| format!("{}: {e}", tsv.display()))?;
        let consumers = Consumers::parse(&text, &tsv.display().to_string())?;
        let proposals = crate::index::notes_data_file(&notes_docs, PROPOSALS_FILE)
            .map_err(|e| format!("{e}; `mercs2_probe clip-names <that path>` writes it"))
            .and_then(|p| {
                let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                Proposals::parse(&text, &p.display().to_string())
            });
        Ok((StateNamer { homes, consumers, proposals }, names))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "{\n  \"pandemic_hash_m2\": {\n    \"0x9DA97065\": [\n      \"enter\"\n    ]\n  }\n}";

    #[test]
    fn an_appended_entry_keeps_the_layout_and_every_hash() {
        let h = pandemic_hash_m2_bytes(b"exit");
        let out = append_entry(FILE, h, "exit").unwrap();
        assert_eq!(
            out,
            "{\n  \"pandemic_hash_m2\": {\n    \"0x9DA97065\": [\n      \"enter\"\n    ],\n    \"0xDB41017D\": [\n      \"exit\"\n    ]\n  }\n}"
        );
        let names = parse_names(&out, "t").unwrap();
        assert_eq!(names.len(), 2);
        assert_eq!(names[&h], "exit");
    }

    #[test]
    fn a_held_key_a_wrong_hash_and_a_bad_name_are_refused() {
        let enter = pandemic_hash_m2_bytes(b"enter");
        assert!(append_entry(FILE, enter, "ENTER").unwrap_err().contains("already named"));
        assert!(append_entry(FILE, 0x1234_5678, "exit").unwrap_err().contains("does not hash"));
        let q = "a\"b";
        assert!(append_entry(FILE, pandemic_hash_m2_bytes(q.as_bytes()), q).unwrap_err().contains("printable"));
        let trailing = format!("{FILE}\n");
        assert!(append_entry(&trailing, pandemic_hash_m2_bytes(b"exit"), "exit").unwrap_err().contains("layout"));
    }

    #[test]
    fn diverged_homes_are_refused() {
        let dir = std::env::temp_dir().join(format!("statenamer_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let homes = NameHomes { docs: dir.join("a.json"), krate: dir.join("b.json") };
        std::fs::write(&homes.docs, FILE).unwrap();
        std::fs::write(&homes.krate, FILE).unwrap();
        assert_eq!(homes.append("exit").unwrap(), pandemic_hash_m2_bytes(b"exit"));
        assert_eq!(std::fs::read(&homes.docs).unwrap(), std::fs::read(&homes.krate).unwrap());
        std::fs::write(&homes.krate, FILE).unwrap();
        let before = std::fs::read(&homes.docs).unwrap();
        assert!(homes.append("pause").unwrap_err().contains("differ"));
        assert_eq!(std::fs::read(&homes.docs).unwrap(), before, "nothing is written when the homes differ");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn words_are_tried_in_every_order_with_and_without_underscores() {
        let crawl = pandemic_hash_m2_bytes(b"CrawlExitVehicle");
        assert_eq!(matches("exit crawl vehicle", crawl), vec!["crawlexitvehicle".to_string()]);
        let o = word_orders("a b c");
        assert_eq!(o.len(), 12, "{o:?}");
        assert!(o.contains(&"c_a_b".to_string()));
        assert_eq!(word_orders("one"), vec!["one".to_string()]);
        assert!(word_orders("").is_empty());
    }

    /// Past [`MAX_WORDS`], the words are tried as typed only: a clip name of six tokens matches.
    #[test]
    fn more_words_than_permuted_are_tried_as_typed() {
        let fidget = pandemic_hash_m2_bytes(b"player_mattias_bare_upright_fidget01_fb");
        assert_eq!(word_orders("a b c d e f"), vec!["abcdef".to_string(), "a_b_c_d_e_f".to_string()]);
        assert_eq!(
            matches("player mattias bare upright fidget01 fb", fidget),
            vec!["player_mattias_bare_upright_fidget01_fb".to_string()]
        );
    }

    #[test]
    fn proposals_carry_their_evidence_and_refuse_a_wrong_hash() {
        let name = "player_mattias_rifle_upright_run_forward_fb";
        let h = pandemic_hash_m2_bytes(name.as_bytes());
        let head = "hash\tname\tphase\texpected_chance\tstance_agrees\tfamily_support\tambiguous\trows\n";
        let tsv = format!("{head}0x{h:08X}\t{name}\tnarrow\t4.37e-4\tyes\t9\t\tMattias/Rifle@0xF71C4238\n");
        let p = Proposals::parse(&tsv, "t").unwrap();
        assert_eq!(
            p.of(h),
            [Proposal { name: name.to_string(), evidence: "narrow · chance 4.37e-4 · stance yes · support 9".to_string() }]
        );
        assert!(p.of(0x1234_5678).is_empty());
        let wrong = format!("{head}0x12345678\t{name}\tnarrow\t4.37e-4\tyes\t9\t\t-\n");
        assert!(Proposals::parse(&wrong, "t").unwrap_err().contains("does not hash"));
        assert!(Proposals::parse("hash\tname\n", "t").unwrap_err().contains("phase"));
    }

    #[test]
    fn consumers_list_every_uncracked_value_with_its_known_names() {
        let tsv = "kind\tconsumer\timages\tknown\tknown_names\tuncracked\tuncracked_values\tcracked_by_context\tsites\n\
                   group\targ1 pc-v11:0x0068cba0\tpc-v11\t3\tCrawlExitVehicle Idle Knockdown\t1\t0xd0fb9338\t-\tpc-v11:0x0068cba0\n\
                   table\tpc-v11:0x00b9c660 stride 4\tpc-v11\t2\ta b\t0\t-\t-\t-\n";
        let c = Consumers::parse(tsv, "t").unwrap();
        assert_eq!(c.of(0xD0FB_9338), ["arg1 pc-v11:0x0068cba0 · CrawlExitVehicle Idle Knockdown".to_string()]);
        assert!(c.of(0x1234_5678).is_empty());
        assert!(Consumers::parse("kind\tconsumer\n", "t").unwrap_err().contains("known_names"));
    }
}
