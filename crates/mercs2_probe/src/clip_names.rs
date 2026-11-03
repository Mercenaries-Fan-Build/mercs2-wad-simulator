//! `clip-names` — name animation clips from the tables that select them.
//!
//! A clip the AnimationLookup lists is named the way its row is keyed. Mattias's own clips are
//! `player_mattias_<equipment>_<stance>_<action>_fb` (`player_mattias_rifle_upright_run_forward_fb`):
//! the row gives the character (`CharacterName`) and the equipment (`PrimaryEquipmentClass`), so
//! only the stance and the action are searched, and each candidate is tested against the clips whose
//! rows give that character and equipment, not against every clip. The shared clips are
//! `<stance>_<action>_fb` (`uprightaim_runforward_leftfoot_fb`).
//!
//! Every word comes from data:
//! * the tokens of the names among every animation table's values and the clips already named,
//!   those ending the way most of them do (`fb`): the transition graph names its states in the clips'
//!   grammar (`upright_walkforward_fb`), which gives the stances (`uprightaim`, `crouch`) and actions;
//! * the names of every animation-table value, split at `_` and at CamelCase (`TransitionUprightAimRight90`
//!   gives `transition`, `upright`, `aim`, `right90`), and the word whose hash IS an equipment value
//!   (`bare` for `0xD20EC345`);
//! * those tokens' compounds split where both halves are words of any source (`walkforward` =
//!   `walk` + `forward`): the halves become words, and a second half two compounds share is a suffix
//!   tried after any word, with `_` and without (`walk_forward`);
//! * the prefix a character's names carry (`player` for `mattias`), from the named clips;
//! * in the second pass only, the words of the jul08 devkit strings and the decompiled Lua (`sprint`,
//!   `shuffle`), with the stances and suffixes the first pass confirmed.
//!
//! Each result carries the expected number of chance matches for its clip (candidates tried against
//! it / 2^32), whether its stance agrees with the stance tables that play its Handle, and how many
//! clips share its stance and action. The clips with no AnimationLookup row (briefings, hijacks,
//! VehicleAnimationLookup seat clips) are counted, not searched: they follow other grammars.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use rayon::prelude::*;

use mercs2_engine::asset::AssetSource;
use mercs2_engine::wad;
use mercs2_formats::anim_select::{
    AnimSelector, AnimTable, ACTIONTABLE, ANIMLOOKUP, NONE_SENTINEL, STANCE_LOCOMOTION_TABLES, TYPE_ANIMTABLE,
    VEHICLE_LOOKUP,
};
use mercs2_formats::game_paths;
use mercs2_formats::hash::pandemic_hash_m2 as m2;

const FNV_OFFSET: u32 = 0x811C_9DC5;
const FNV_PRIME: u32 = 0x0100_0193;

/// `pandemic_hash_m2`'s running state after `s`, from `h`.
fn feed(mut h: u32, s: &[u8]) -> u32 {
    for &b in s {
        h ^= (b | 0x20) as u32;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// `pandemic_hash_m2`'s finalization of a running state.
fn finish(h: u32) -> u32 {
    (h ^ 0x2A).wrapping_mul(FNV_PRIME)
}

/// The words of a name: each `_`/non-alphanumeric piece, and each piece split at CamelCase (digits
/// stay on the word before them: `Right90` → `right90`), lowercased.
pub fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for piece in s.split(|c: char| !c.is_ascii_alphanumeric()).filter(|p| !p.is_empty()) {
        out.push(piece.to_ascii_lowercase());
        let b = piece.as_bytes();
        let mut start = 0;
        for i in 1..b.len() {
            let (p, c) = (b[i - 1], b[i]);
            let next_lower = b.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            let cut = (c.is_ascii_uppercase() && (p.is_ascii_lowercase() || p.is_ascii_digit()))
                || (c.is_ascii_uppercase() && p.is_ascii_uppercase() && next_lower)
                || (c.is_ascii_alphabetic() && p.is_ascii_digit());
            if cut {
                out.push(piece[start..i].to_ascii_lowercase());
                start = i;
            }
        }
        if start > 0 {
            out.push(piece[start..].to_ascii_lowercase());
        }
    }
    out
}

/// Split each compound of `tokens` where both halves (at least 3 bytes) are words of `dictionary`
/// (`walkforward` = `walk` + `forward`). Returns every half, and the suffixes: the second halves at
/// least two tokens split into (`forward` from `walkforward` and `jogforward`), so a one-off split
/// does not become a suffix tried after every word.
pub fn split_compounds(tokens: &BTreeSet<String>, dictionary: &BTreeSet<String>) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut halves = BTreeSet::new();
    let mut tails: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for t in tokens {
        for i in 3..t.len().saturating_sub(2) {
            let (a, b) = t.split_at(i);
            if dictionary.contains(a) && dictionary.contains(b) {
                halves.insert(a.to_string());
                halves.insert(b.to_string());
                tails.entry(b).or_default().insert(t);
            }
        }
    }
    let suffixes = tails.into_iter().filter(|(_, from)| from.len() >= 2).map(|(b, _)| b.to_string()).collect();
    (halves, suffixes)
}

/// The actions tried after a stance: every word, every word with every suffix (`walk_forward` and
/// `walkforward`), and with `digits` every word followed by `01`..`09`.
pub fn phrases(words: &BTreeSet<String>, suffixes: &BTreeSet<String>, digits: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in words {
        out.push(w.clone());
        for s in suffixes {
            out.push(format!("{w}_{s}"));
            out.push(format!("{w}{s}"));
        }
        if digits {
            for d in 1..=9 {
                out.push(format!("{w}{d:02}"));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// One search: every `<head><stance>_<phrase>_<end>` against the clips that share `head`. Returns the
/// hits `(clip, name, stance, phrase)` and the number of candidates tried per head.
fn search(
    heads: &BTreeMap<String, HashSet<u32>>,
    stances: &BTreeSet<String>,
    phrases: &[String],
    ends: &BTreeSet<String>,
) -> (Vec<(u32, String, String, String)>, u64) {
    let per_head = (stances.len() * phrases.len() * ends.len()) as u64;
    let hits = heads
        .par_iter()
        .flat_map_iter(|(head, targets)| {
            let mut found = Vec::new();
            let h0 = feed(FNV_OFFSET, head.as_bytes());
            for s in stances {
                let h1 = feed(feed(h0, s.as_bytes()), b"_");
                for p in phrases {
                    let h2 = feed(feed(h1, p.as_bytes()), b"_");
                    for e in ends {
                        let h = finish(feed(h2, e.as_bytes()));
                        if targets.contains(&h) {
                            found.push((h, format!("{head}{s}_{p}_{e}"), s.clone(), p.clone()));
                        }
                    }
                }
            }
            found
        })
        .collect();
    (hits, per_head)
}

/// A rainbow-format name file (`{"pandemic_hash_m2": {"0xH": [names]}}`) into `pool`, keeping only the
/// names that hash to their key.
fn load_names_json(path: &Path, pool: &mut HashMap<u32, BTreeSet<String>>) -> Result<(), String> {
    let txt = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&txt).map_err(|e| format!("{}: {e}", path.display()))?;
    let map = v["pandemic_hash_m2"].as_object().ok_or_else(|| format!("{}: no `pandemic_hash_m2` object", path.display()))?;
    for (k, names) in map {
        let h = u32::from_str_radix(k.trim_start_matches("0x"), 16).map_err(|e| format!("{}: key {k}: {e}", path.display()))?;
        let names: Vec<&str> = match names {
            serde_json::Value::Array(a) => a.iter().filter_map(|n| n.as_str()).collect(),
            serde_json::Value::String(s) => vec![s.as_str()],
            _ => Vec::new(),
        };
        for n in names {
            if m2(n) == h {
                pool.entry(h).or_default().insert(n.to_string());
            }
        }
    }
    Ok(())
}

/// Every identifier in the `.lua` files under `dir`.
fn lua_identifiers(dir: &Path, out: &mut BTreeSet<String>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            lua_identifiers(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "lua") {
            let txt = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            out.extend(words(&txt));
        }
    }
    Ok(())
}

/// A word worth trying: starts with a letter, 2–20 bytes, alphanumeric.
fn wordlike(w: &str) -> bool {
    (2..=20).contains(&w.len()) && w.as_bytes()[0].is_ascii_alphabetic() && w.bytes().all(|c| c.is_ascii_alphanumeric())
}

/// One cracked clip, for the review file.
struct Hit {
    clip: u32,
    name: String,
    phase: &'static str,
    stance: String,
    phrase: String,
}

pub fn clip_names(wadpath: &str, out: &Path) -> Result<(), String> {
    let start = std::env::current_dir().map_err(|e| format!("current directory: {e}"))?;
    let notes = game_paths::local_config_notes_docs_dir(&start)?;

    // ── the name pool ──
    let mut pool: HashMap<u32, BTreeSet<String>> = HashMap::new();
    load_names_json(&game_paths::local_config_rainbow_table(&start)?, &mut pool)?;
    load_names_json(&notes.join("data/aset_discovered_names.json"), &mut pool)?;
    let jul08 = game_paths::local_config_jul08_strings(&start)?;
    let jul08_txt = std::fs::read_to_string(&jul08).map_err(|e| format!("{}: {e}", jul08.display()))?;
    for line in jul08_txt.lines().map(str::trim).filter(|l| !l.is_empty()) {
        pool.entry(m2(line)).or_default().insert(line.to_string());
    }
    pool.retain(|h, names| {
        names.retain(|n| m2(n) == *h);
        !names.is_empty()
    });
    let name_of = |h: u32| pool.get(&h).and_then(|n| n.iter().next()).cloned();

    // ── the tables ──
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let mut table = |name: u32, what: &str| {
        assets.resolve(TYPE_ANIMTABLE, name).ok_or_else(|| format!("{what} 0x{name:08X}: in no open wad"))
    };
    let lookup = table(ANIMLOOKUP, "AnimationLookup")?;
    let actions = table(ACTIONTABLE, "ActionTable")?;
    let vehicle = table(VEHICLE_LOOKUP, "VehicleAnimationLookup")?;
    let sel = AnimSelector::from_containers(&lookup, Some(&actions))?.with_vehicle_lookup(&vehicle)?;
    let action_table = AnimTable::parse(&actions).map_err(|e| format!("ActionTable: {e}"))?;
    let stance_col = action_table.cols().iter().position(|c| c == "Stance").ok_or("ActionTable: no Stance column")?;
    let rows = sel.all_lookup_rows();
    let shared = sel.shared_character();

    // ── every clip, by what selects it ──
    let mut lookup_clips: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, r) in rows.iter().enumerate() {
        for &c in &r.clips {
            lookup_clips.entry(c).or_default().push(i);
        }
    }
    let vehicle_clips: BTreeSet<u32> = sel.vehicle_clips().iter().map(|v| v.clip).filter(|c| !lookup_clips.contains_key(c)).collect();
    let mut animgroup_clips: BTreeSet<u32> = BTreeSet::new();
    let mut table_names: BTreeSet<u32> = BTreeSet::new();
    let type_id = mercs2_formats::aset_type_ids::type_id_for_type_hash(TYPE_ANIMTABLE)
        .ok_or_else(|| format!("no ASET type id for the animationtable type 0x{TYPE_ANIMTABLE:08X}"))?;
    for p in assets.paths().to_vec() {
        let mut w = wad::open(&p)?;
        table_names.extend(wad::all_asets(&w).into_iter().filter(|&(_, t, _)| t == type_id).map(|(h, _, _)| h));
        for blk in wad::animgroup_blocks(&w) {
            let dec = wad::decompress_block_index(&mut w, blk)?;
            let ag = mercs2_formats::animgroup::parse_animgroup(&dec).map_err(|e| format!("{p}: animgroup block {blk}: {e}"))?;
            animgroup_clips.extend(ag.clips.iter().map(|c| c.name_hash));
        }
    }
    let all_clips: BTreeSet<u32> =
        lookup_clips.keys().copied().chain(vehicle_clips.iter().copied()).chain(animgroup_clips.iter().copied()).collect();
    let named: BTreeMap<u32, String> = all_clips.iter().filter_map(|&c| name_of(c).map(|n| (c, n))).collect();

    // ── the narrow vocabulary ──
    // Every animation table's values (the transition graph names its states in the clips' own
    // grammar: `upright_walkforward_fb`), from every archive of the stack.
    let mut values: BTreeSet<u32> = BTreeSet::new();
    for &h in &table_names {
        let cont = assets.resolve(TYPE_ANIMTABLE, h).ok_or_else(|| format!("animationtable 0x{h:08X}: in no open wad"))?;
        let t = AnimTable::parse(&cont).map_err(|e| format!("animationtable 0x{h:08X}: {e}"))?;
        values.extend(t.defaults().iter().copied());
        values.extend(t.data_rows().flatten().copied());
        values.extend(t.pool().iter().copied());
    }
    let named_values: Vec<String> = values.iter().filter_map(|&v| name_of(v)).collect();
    let mut table_words: BTreeSet<String> = BTreeSet::new();
    for n in &named_values {
        table_words.extend(words(n));
    }
    // The grammar is learned from the `_`-joined names among the table values and the named clips.
    let grammar: Vec<Vec<String>> = named_values
        .iter()
        .chain(named.values())
        .filter(|n| n.contains('_'))
        .map(|n| n.split('_').map(|t| t.to_ascii_lowercase()).collect())
        .collect();
    // Endings: the last token of at least a tenth of those names (`fb`).
    let mut end_counts: BTreeMap<String, usize> = BTreeMap::new();
    for t in &grammar {
        *end_counts.entry(t[t.len() - 1].clone()).or_default() += 1;
    }
    let ends: BTreeSet<String> = end_counts.into_iter().filter(|(_, n)| *n * 10 >= grammar.len()).map(|(e, _)| e).collect();
    // With them, every identifier-shaped name of the pool that ends that way (`crouchaim_…_fb`, whose
    // hashes no open table or animgroup holds).
    let mut known_tokens: Vec<Vec<String>> = grammar.iter().filter(|t| ends.contains(&t[t.len() - 1])).cloned().collect();
    for n in pool.values().flatten() {
        let t: Vec<String> = n.split('_').map(|t| t.to_ascii_lowercase()).collect();
        if t.len() >= 3 && ends.contains(&t[t.len() - 1]) && t.iter().all(|w| wordlike(w) || w.bytes().all(|c| c.is_ascii_digit())) {
            known_tokens.push(t);
        }
    }
    let token_set: BTreeSet<String> = known_tokens.iter().flatten().cloned().collect();
    let mut narrow: BTreeSet<String> = token_set.iter().chain(table_words.iter()).filter(|w| wordlike(w)).cloned().collect();
    // Compounds split where both halves are words of any source (`walkforward` = `walk` + `forward`).
    let mut wide: BTreeSet<String> = words(&jul08_txt).into_iter().filter(|w| wordlike(w)).collect();
    lua_identifiers(&notes.join("mercs2-luacd/src"), &mut wide)?;
    wide.retain(|w| wordlike(w));
    let dictionary: BTreeSet<String> = narrow.union(&wide).cloned().collect();
    let (halves, suffixes) = split_compounds(&token_set, &dictionary);
    narrow.extend(halves);
    wide.retain(|w| !narrow.contains(w));

    // Stances: the ActionTable's Stance names and, to a fixpoint, the name tokens that are a stance's
    // stem or extend one (`crouch` in `crouched`, then `crouchaim`; `uprightaim`). What a token adds to
    // a stance (`aim`) is tried after every stance.
    let stance_names: BTreeSet<String> = action_table
        .data_rows()
        .filter_map(|r| name_of(r[stance_col]))
        .map(|n| n.to_ascii_lowercase())
        .filter(|n| wordlike(n))
        .collect();
    let mut stances: BTreeSet<String> = stance_names.clone();
    loop {
        let before = stances.len();
        for t in &token_set {
            if stances.iter().any(|s| (s.starts_with(t.as_str()) && t.len() >= 4) || t.starts_with(s.as_str())) {
                stances.insert(t.clone());
            }
        }
        if stances.len() == before {
            break;
        }
    }
    let tails: BTreeSet<String> = stances
        .iter()
        .filter(|t| token_set.contains(*t))
        .flat_map(|t| stances.iter().filter_map(move |s| t.strip_prefix(s.as_str()).filter(|x| x.len() >= 3).map(str::to_string)))
        .collect();
    let bases: Vec<String> = stances.iter().cloned().collect();
    for b in &bases {
        for t in &tails {
            stances.insert(format!("{b}{t}"));
        }
    }
    stances.retain(|s| wordlike(s));

    // Prefixes per character: the first token of the named clips whose second token is that character.
    let mut prefixes_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for t in known_tokens.iter().filter(|t| t.len() >= 4) {
        prefixes_of.entry(t[1].clone()).or_default().insert(t[0].clone());
    }
    let all_prefixes: BTreeSet<String> =
        known_tokens.iter().filter(|t| t.len() >= 5 && !stances.contains(&t[0])).map(|t| t[0].clone()).collect();

    // The equipment word of a class value: the narrow word whose hash it is, else its name.
    let equipment_word = |v: u32| -> Option<String> {
        narrow.iter().find(|w| m2(w) == v).cloned().or_else(|| name_of(v).map(|n| n.to_ascii_lowercase()))
    };

    // The characters whose own rows carry each Gender: a shared row keyed by a Gender (`*`/Hero) plays
    // for those characters, and its clips carry one of their names (`player_mattias_rifle_crouch_idle_fb`).
    let mut characters_of_gender: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.character != shared && r.character != NONE_SENTINEL) {
        characters_of_gender.entry(r.gender).or_default().insert(r.character);
    }

    // ── heads: what precedes the stance, per clip ──
    let mut heads: BTreeMap<String, HashSet<u32>> = BTreeMap::new();
    let mut heads_of: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    let mut no_context: BTreeSet<u32> = BTreeSet::new();
    for (&clip, idx) in lookup_clips.iter().filter(|(c, _)| !named.contains_key(c)) {
        let mut hs: BTreeSet<String> = BTreeSet::new();
        hs.insert(String::new()); // the shared grammar: `<stance>_<action>_fb`
        for &i in idx {
            let r = &rows[i];
            let characters: BTreeSet<u32> = if r.character == shared || r.character == NONE_SENTINEL {
                characters_of_gender.get(&r.gender).cloned().unwrap_or_default()
            } else {
                [r.character].into_iter().collect()
            };
            let Some(equip) = equipment_word(r.primary_equipment_class) else {
                continue;
            };
            for who in characters.into_iter().filter_map(name_of) {
                let who = who.to_ascii_lowercase();
                let prefixes = prefixes_of.get(&who).unwrap_or(&all_prefixes);
                for p in prefixes {
                    hs.insert(format!("{p}_{who}_{equip}_"));
                }
            }
        }
        if hs.len() == 1 {
            no_context.insert(clip);
        }
        for h in &hs {
            heads.entry(h.clone()).or_default().insert(clip);
        }
        heads_of.insert(clip, hs);
    }

    // ── pass 1: the narrow vocabulary ──
    let narrow_phrases = phrases(&narrow, &suffixes, true);
    let (hits1, per_head1) = search(&heads, &stances, &narrow_phrases, &ends);
    let mut chance: BTreeMap<u32, f64> = BTreeMap::new();
    for (&c, hs) in &heads_of {
        *chance.entry(c).or_default() += hs.len() as f64 * per_head1 as f64 / 2f64.powi(32);
    }
    let mut hits: Vec<Hit> =
        hits1.into_iter().map(|(clip, name, stance, phrase)| Hit { clip, name, phase: "narrow", stance, phrase }).collect();

    // ── pass 2: the wide words, with the stances and suffixes pass 1 confirmed, on the clips it left ──
    let confirmed_stances: BTreeSet<String> = hits.iter().map(|h| h.stance.clone()).collect();
    let confirmed_suffixes: BTreeSet<String> =
        suffixes.iter().filter(|s| hits.iter().any(|h| h.phrase.ends_with(s.as_str()) && h.phrase != **s)).cloned().collect();
    let cracked: HashSet<u32> = hits.iter().map(|h| h.clip).collect();
    let mut heads2: BTreeMap<String, HashSet<u32>> = BTreeMap::new();
    for (h, t) in &heads {
        let left: HashSet<u32> = t.iter().copied().filter(|c| !cracked.contains(c)).collect();
        if !left.is_empty() {
            heads2.insert(h.clone(), left);
        }
    }
    let wide_phrases = phrases(&wide, &confirmed_suffixes, false);
    let (hits2, per_head2) = search(&heads2, &confirmed_stances, &wide_phrases, &ends);
    for c in heads2.values().flatten() {
        *chance.entry(*c).or_default() += per_head2 as f64 / 2f64.powi(32);
    }
    hits.extend(hits2.into_iter().map(|(clip, name, stance, phrase)| Hit { clip, name, phase: "wide", stance, phrase }));

    // ── evidence per hit ──
    let mut expected: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for (&c, idx) in &lookup_clips {
        for &i in idx {
            let handle = rows[i].handle;
            for (stance, _, words) in STANCE_LOCOMOTION_TABLES.iter() {
                if words.contains(&handle) {
                    if let Some(n) = name_of(*stance) {
                        expected.entry(c).or_default().insert(n.to_ascii_lowercase());
                    }
                }
            }
            for a in sel.handle_actions(handle) {
                if let Some(n) = name_of(a.stance) {
                    expected.entry(c).or_default().insert(n.to_ascii_lowercase());
                }
            }
        }
    }
    let agrees = |c: u32, stance: &str| -> &'static str {
        match expected.get(&c) {
            None => "-",
            Some(e) => {
                let common = |a: &str, b: &str| a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
                if e.iter().any(|x| common(x, stance) >= x.len().min(stance.len()).min(5)) {
                    "yes"
                } else {
                    "no"
                }
            }
        }
    };
    let mut support: HashMap<(String, String), BTreeSet<u32>> = HashMap::new();
    for h in &hits {
        support.entry((h.stance.clone(), h.phrase.clone())).or_default().insert(h.clip);
    }
    let mut per_clip: BTreeMap<u32, usize> = BTreeMap::new();
    for h in &hits {
        *per_clip.entry(h.clip).or_default() += 1;
    }
    let context = |c: u32| -> String {
        let mut s: BTreeSet<String> = BTreeSet::new();
        for &i in &lookup_clips[&c] {
            let r = &rows[i];
            let n = |v: u32| name_of(v).unwrap_or_else(|| format!("0x{v:08X}"));
            s.insert(format!("{}/{}@{}", n(r.character), n(r.primary_equipment_class), n(r.handle)));
        }
        s.into_iter().collect::<Vec<_>>().join(" ")
    };

    hits.sort_by(|a, b| a.name.cmp(&b.name));
    let mut tsv = String::from("hash\tname\tphase\texpected_chance\tstance_agrees\tfamily_support\tambiguous\trows (character/equipment@handle)\n");
    for h in &hits {
        tsv.push_str(&format!(
            "0x{:08X}\t{}\t{}\t{:.2e}\t{}\t{}\t{}\t{}\n",
            h.clip,
            h.name,
            h.phase,
            chance.get(&h.clip).copied().unwrap_or(0.0),
            agrees(h.clip, &h.stance),
            support[&(h.stance.clone(), h.phrase.clone())].len(),
            if per_clip[&h.clip] > 1 { "yes" } else { "" },
            context(h.clip)
        ));
    }
    std::fs::write(out, tsv).map_err(|e| format!("{}: {e}", out.display()))?;

    // ── every clip accounted for ──
    let searched = heads_of.len();
    let cracked_clips: BTreeSet<u32> = hits.iter().map(|h| h.clip).collect();
    let in_no_table = animgroup_clips.iter().filter(|c| !lookup_clips.contains_key(c) && !vehicle_clips.contains(c)).count();
    let expected_total: f64 = chance.values().sum();
    println!("clips: {} in all", all_clips.len());
    println!("  already named:                 {}", named.len());
    println!("  searched (AnimationLookup):    {searched}  ({} with no named character/equipment row: shared grammar only)", no_context.len());
    println!("    cracked:                     {} ({} names; {} narrow, {} wide)", cracked_clips.len(), hits.len(),
        hits.iter().filter(|h| h.phase == "narrow").count(), hits.iter().filter(|h| h.phase == "wide").count());
    println!("    not cracked:                 {}", searched - cracked_clips.len());
    println!("  VehicleAnimationLookup only:   {} (seat grammar, not searched)", vehicle_clips.iter().filter(|c| !named.contains_key(c)).count());
    println!("  in no table:                   {in_no_table} (briefings, hijacks, …: not searched)");
    println!("vocabulary: {} narrow words, {} wide words", narrow.len(), wide.len());
    let list = |s: &BTreeSet<String>| s.iter().cloned().collect::<Vec<_>>().join(" ");
    println!("  suffixes ({}): {}", suffixes.len(), list(&suffixes));
    println!("  stances ({}): {}", stances.len(), list(&stances));
    println!("  ends ({}): {}", ends.len(), list(&ends));
    for (who, p) in &prefixes_of {
        println!("  prefixes of {who}: {}", list(p));
    }
    println!("  every prefix ({}): {}", all_prefixes.len(), list(&all_prefixes));
    println!("candidates per head: pass 1 {per_head1}, pass 2 {per_head2}; expected chance matches over every searched clip: {expected_total:.3}");
    println!("review file -> {}", out.display());
    Ok(())
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    /// On retail, Mattias's locomotion clips crack from their rows alone: walk/jog/run from the
    /// transition graph's words, crouch through the Gender-keyed shared rows, sprint in the wide pass;
    /// and `0x68D7ECBF`, which a free search also matched as a swim clip, comes out once, as its
    /// Upright row says.
    #[test]
    fn mattias_locomotion_cracks_from_the_rows() {
        let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let wad = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        let out = std::env::temp_dir().join(format!("clip_names_{}.tsv", std::process::id()));
        super::clip_names(&wad.to_string_lossy(), &out).unwrap_or_else(|e| panic!("{e}"));
        let tsv = std::fs::read_to_string(&out).unwrap();
        std::fs::remove_file(&out).unwrap();
        let rows: Vec<Vec<&str>> = tsv.lines().skip(1).map(|l| l.split('\t').collect()).collect();
        let names_of = |h: &str| rows.iter().filter(|r| r[0] == h).map(|r| r[1]).collect::<Vec<_>>();
        for (h, n) in [
            ("0xBBA47DC4", "player_mattias_bare_upright_walk_forward_fb"),
            ("0xD23346BC", "player_mattias_rifle_upright_run_forward_fb"),
            ("0x9DC1D11D", "player_mattias_pistol_crouchaim_walk_backward_fb"),
            ("0x35E07B11", "player_mattias_rifle_upright_sprint_forward_fb"),
            ("0x68D7ECBF", "player_mattias_rifle_uprightaim_run_forward_fb"),
        ] {
            assert_eq!(names_of(h), vec![n], "{h}");
        }
        assert!(rows.iter().all(|r| r[4] != "no"), "a hit whose stance disagrees with its rows");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_hash_is_pandemic_hash_m2() {
        for s in ["player_mattias_rifle_upright_run_forward_fb", "Upright", "a"] {
            assert_eq!(finish(feed(FNV_OFFSET, s.as_bytes())), m2(s), "{s}");
        }
    }

    #[test]
    fn words_split_at_underscores_and_camel_case() {
        let w = words("TransitionUprightAimRight90");
        for x in ["transitionuprightaimright90", "transition", "upright", "aim", "right90"] {
            assert!(w.contains(&x.to_string()), "{x} not in {w:?}");
        }
        let w = words("HUD_faction_PMC");
        for x in ["hud", "faction", "pmc"] {
            assert!(w.contains(&x.to_string()), "{x} not in {w:?}");
        }
    }

    /// A compound splits where both halves are dictionary words; a tail two compounds share is a
    /// suffix, one only one compound has is not.
    #[test]
    fn compounds_split_against_the_dictionary() {
        let set = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<BTreeSet<String>>();
        let (halves, suffixes) =
            split_compounds(&set(&["walkforward", "jogforward", "walkstrafe", "section04"]), &set(&["walk", "jog", "forward", "strafe"]));
        assert_eq!(halves, set(&["walk", "jog", "forward", "strafe"]));
        assert_eq!(suffixes, set(&["forward"]));
    }

    /// The search recovers a name built from its own vocabulary and tries exactly the candidates it
    /// counts.
    #[test]
    fn the_search_finds_a_name_from_its_slots() {
        let target = m2("player_mattias_rifle_upright_walk_forward_fb");
        let heads: BTreeMap<String, HashSet<u32>> =
            [("player_mattias_rifle_".to_string(), [target].into_iter().collect())].into_iter().collect();
        let set = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<BTreeSet<String>>();
        let ph = phrases(&set(&["walk", "jog"]), &set(&["forward"]), true);
        let (hits, per_head) = search(&heads, &set(&["upright", "crouch"]), &ph, &set(&["fb"]));
        assert_eq!(per_head, (2 * ph.len()) as u64);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, "player_mattias_rifle_upright_walk_forward_fb");
        assert_eq!((hits[0].2.as_str(), hits[0].3.as_str()), ("upright", "walk_forward"));
    }
}
