//! `model-names` — does each model's catalog name agree with the model itself?
//!
//! A model chunk carries no name, only `pandemic_hash_m2(name)`. The catalog name of a model is
//! whatever string of the name table hashes to it, and a 32-bit match is all that ties the two: a
//! string made by a grammar sweep can hash to a model it has nothing to do with. The Workshop files a
//! model under its name's `_veh_<class>_` token, so a wrong name moves the model out of its category.
//!
//! This reads, for every model of the archive:
//!
//! * the textures its own `MTRL`s cite, over every block's entry table (`wad::for_each_chunk`), so a
//!   model container with no ASET row is seen too;
//! * the `worldentity` templates whose `ModelName` is the model (`mercs2_formats::mixr::kit_templates`).
//!
//! A **content name** of a model is a string taken from those that hashes to the model: a template
//! `Name` with its leading `_` removed (a placed prop's template is named after its model), or an
//! order-preserving `_`-token subsequence of a cited texture's name (`X_dm` names `X`). Each model
//! then gets one verdict:
//!
//! * `witnessed` — the catalog name is a content name, or a cited texture's name starts with it;
//! * `contradicted` — the model has content names and the catalog name is none of them: two strings
//!   hash to the model and its own data spells the other one;
//! * `family` — neither, and the name shares its first two or more `_` tokens with a cited texture;
//! * `unwitnessed` — nothing of the model spells the name;
//! * `unnamed` — the table has no name for the hash.
//!
//! `archetypes` lists what the model's templates are by the engine's own rule
//! (`mercs2_engine::worldutil::TemplateRecords::archetypes`: the physics record a template carries):
//! `Car`, `Tank`, `Boat`, `Helicopter`, `Jet` or `Character`. A template with none of those is a prop
//! and adds nothing. The report sets that against the `_veh_<class>_` token of the catalog name.
//!
//! The name table is the workspace's `crates/mercs2_workshop/data/production_names.json` (`--names`
//! reads another file of the same shape). Texture names come from the same table.
//!
//! Exit 0 = every block, MTRL and template read, the block walk and the ASET table list the same
//! models, and no name is contradicted; 1 = any of those failed (listed); 2 = usage or setup.
//!
//! ```text
//! mercs2_probe model-names [--names <names.json>] [--tsv <out.tsv>] [--wad <path>]
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use mercs2_engine::registry::AssetRegistry;
use mercs2_engine::spawn::Archetype;
use mercs2_engine::wad;
use mercs2_engine::worldutil::TemplateRecords;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::mixr::kit_templates;
use mercs2_formats::texture::{parse_mtrl, MtrlSource};
use mercs2_formats::worldentity::{WorldEntity, WORLDENTITY_TYPE_HASH};
use mercs2_quartermaster::names::{NameTable, PRODUCTION_NAMES};

/// How an archetype prints: the vehicle's class, or `Character`. A prop prints nothing.
fn archetype_label(a: Archetype) -> Option<String> {
    match a {
        Archetype::Prop => None,
        Archetype::Vehicle(c) => Some(format!("{c:?}")),
        Archetype::Character => Some("Character".into()),
    }
}

/// The class token of a `<faction>_veh_<class>_<name>` name, lower-case.
fn veh_token(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let token = lower.split("_veh_").nth(1)?.split('_').next().unwrap_or("");
    Some(token.to_string())
}

/// Longest texture name, in `_` tokens, whose subsequences are enumerated (2^N candidates).
const MAX_TOKENS: usize = 12;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Verdict {
    Contradicted,
    Unwitnessed,
    Unnamed,
    Family,
    Witnessed,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Verdict::Contradicted => "contradicted",
            Verdict::Unwitnessed => "unwitnessed",
            Verdict::Unnamed => "unnamed",
            Verdict::Family => "family",
            Verdict::Witnessed => "witnessed",
        }
    }
}

struct Row {
    hash: u32,
    name: Option<String>,
    verdict: Verdict,
    /// Strings from the model's own templates and textures that hash to it.
    content_names: Vec<String>,
    /// Leading `_` tokens the name shares with its closest cited texture.
    shared_tokens: usize,
    /// What the model's templates are ([`archetype_label`]), props left out.
    archetypes: BTreeSet<String>,
    templates: Vec<String>,
    textures: Vec<String>,
}

fn tokens(s: &str) -> Vec<String> {
    s.to_ascii_lowercase().split('_').filter(|t| !t.is_empty()).map(str::to_string).collect()
}

/// Every order-preserving subsequence of `name`'s `_` tokens, two tokens or longer, that hashes to
/// `target`.
fn subsequences_hashing_to(name: &str, target: u32, out: &mut BTreeSet<String>) {
    let toks = tokens(name);
    let n = toks.len().min(MAX_TOKENS);
    for mask in 1u32..(1 << n) {
        if mask.count_ones() < 2 {
            continue;
        }
        let cand: Vec<&str> = (0..n).filter(|i| mask & (1 << i) != 0).map(|i| toks[i].as_str()).collect();
        let cand = cand.join("_");
        if pandemic_hash_m2(&cand) == target {
            out.insert(cand);
        }
    }
}

fn judge(name: Option<&str>, content_names: &[String], textures: &[String]) -> (Verdict, usize) {
    let Some(name) = name else { return (Verdict::Unnamed, 0) };
    let lower = name.to_ascii_lowercase();
    let name_toks = tokens(name);
    let shared = textures
        .iter()
        .map(|t| tokens(t).iter().zip(&name_toks).take_while(|(a, b)| a == b).count())
        .max()
        .unwrap_or(0);
    let spelled = content_names.iter().any(|c| *c == lower)
        || textures.iter().any(|t| t.to_ascii_lowercase().starts_with(&lower));
    let verdict = if spelled {
        Verdict::Witnessed
    } else if !content_names.is_empty() {
        Verdict::Contradicted
    } else if shared >= 2 {
        Verdict::Family
    } else {
        Verdict::Unwitnessed
    };
    (verdict, shared)
}

fn setup_fail(msg: String) -> i32 {
    eprintln!("model-names: {msg}");
    2
}

pub fn run(wadpath: &str, names_arg: Option<String>, tsv: Option<String>) -> i32 {
    let names_path: PathBuf = match names_arg {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(PRODUCTION_NAMES),
    };
    let names = match NameTable::load(&names_path) {
        Ok(n) => n,
        Err(e) => return setup_fail(e.to_string()),
    };
    eprintln!("[model-names] {} names from {}", names.len(), names_path.display());

    let mut wads = match wad::open(wadpath) {
        Ok(w) => vec![w],
        Err(e) => return setup_fail(format!("cannot open {wadpath}: {e}")),
    };
    let aset: BTreeSet<u32> = wad::model_list_all(&wads[0]).into_iter().map(|(h, _)| h).collect();

    // Every model chunk of every block, with the textures its MTRLs cite.
    let mut cited: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let block_failures = wad::for_each_chunk(&mut wads[0], |c| {
        if c.entry.type_hash != wad::MODEL_TYPE_HASH {
            return;
        }
        match parse_mtrl(c.bytes, MtrlSource::Model) {
            Ok(mats) => {
                let slot = cited.entry(c.entry.name_hash).or_default();
                for m in &mats {
                    slot.extend(m.textures.iter().copied().filter(|t| *t != 0 && *t != 0xFFFF_FFFF));
                }
            }
            Err(e) => failures.push(format!("block {} {}: model 0x{:08X} MTRL: {e}", c.block, c.path, c.entry.name_hash)),
        }
    });
    failures.extend(block_failures);
    let walked: BTreeSet<u32> = cited.keys().copied().collect();

    // The templates of each model, and the template keys that carry a vehicle component.
    let mut registry = AssetRegistry::default();
    let Some((_, we_ref)) = registry.resolve_singleton(&mut wads, WORLDENTITY_TYPE_HASH) else {
        return setup_fail("no worldentity chunk in the wad".into());
    };
    let Some(bytes) = registry.slice(we_ref) else {
        return setup_fail("worldentity block evicted before read".into());
    };
    let we = match WorldEntity::parse(bytes) {
        Ok(w) => w,
        Err(e) => return setup_fail(format!("worldentity: {e}")),
    };
    let templates = match kit_templates(&we) {
        Ok(t) => t,
        Err(e) => return setup_fail(format!("templates: {e}")),
    };
    let records = match TemplateRecords::from_worldentity(&we) {
        Ok(r) => r,
        Err(e) => return setup_fail(format!("template records: {e}")),
    };
    if records.archetypes.is_empty() {
        return setup_fail("no template of the worldentity carries an archetype physics record".into());
    }
    let mut templates_of: BTreeMap<u32, Vec<(String, Option<String>)>> = BTreeMap::new();
    for t in &templates {
        let archetype = records.archetypes.get(&t.key).copied().and_then(archetype_label);
        templates_of.entry(t.model).or_default().push((t.label(), archetype));
    }

    let mut rows: Vec<Row> = Vec::new();
    for &hash in aset.union(&walked) {
        let name = names.reverse(hash).map(str::to_string);
        let textures: Vec<String> = cited
            .get(&hash)
            .into_iter()
            .flatten()
            .filter_map(|t| names.reverse(*t))
            .map(str::to_string)
            .collect();
        let ts = templates_of.get(&hash).map(Vec::as_slice).unwrap_or(&[]);
        let mut content: BTreeSet<String> = BTreeSet::new();
        for (t, _) in ts {
            let stem = t.trim_start_matches('_');
            if pandemic_hash_m2(stem) == hash {
                content.insert(stem.to_ascii_lowercase());
            }
        }
        for t in &textures {
            subsequences_hashing_to(t, hash, &mut content);
        }
        let content_names: Vec<String> = content.into_iter().collect();
        let (verdict, shared_tokens) = judge(name.as_deref(), &content_names, &textures);
        rows.push(Row {
            hash,
            name,
            verdict,
            content_names,
            shared_tokens,
            archetypes: ts.iter().filter_map(|(_, a)| a.clone()).collect(),
            templates: ts.iter().map(|(t, _)| t.clone()).collect(),
            textures,
        });
    }
    rows.sort_by(|a, b| a.verdict.cmp(&b.verdict).then(a.name.cmp(&b.name)).then(a.hash.cmp(&b.hash)));

    // ---- report ----
    let only_walked: Vec<u32> = walked.difference(&aset).copied().collect();
    let only_aset: Vec<u32> = aset.difference(&walked).copied().collect();
    println!("models in the ASET table: {}   model chunks found by walking every block: {}", aset.len(), walked.len());
    println!("  in a block but not in ASET: {}   in ASET but in no block: {}", only_walked.len(), only_aset.len());
    for h in &only_walked {
        println!("    0x{h:08X} has a chunk and no ASET row");
    }
    for h in &only_aset {
        println!("    0x{h:08X} has an ASET row and no chunk");
    }

    let count = |v: Verdict| rows.iter().filter(|r| r.verdict == v).count();
    println!("\nverdicts over {} models:", rows.len());
    for v in [Verdict::Witnessed, Verdict::Family, Verdict::Unwitnessed, Verdict::Contradicted, Verdict::Unnamed] {
        println!("  {:13} {}", v.label(), count(v));
    }

    let line = |r: &Row| {
        format!(
            "  0x{:08X}  {}{}\n      templates: {}\n      textures:  {}",
            r.hash,
            r.name.as_deref().unwrap_or("(unnamed)"),
            if r.content_names.is_empty() { String::new() } else { format!("\n      its own data spells: {}", r.content_names.join(", ")) },
            if r.templates.is_empty() { "-".into() } else { r.templates.join("; ") },
            if r.textures.is_empty() { "-".into() } else { r.textures.join(" ") },
        )
    };
    println!("\ncontradicted names ({}):", count(Verdict::Contradicted));
    for r in rows.iter().filter(|r| r.verdict == Verdict::Contradicted) {
        println!("{}", line(r));
    }
    // What the templates say a model is, against the `_veh_<class>_` token of its name.
    let token_of = |r: &Row| r.name.as_deref().and_then(veh_token);
    let mut cross: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in rows.iter().filter(|r| !r.archetypes.is_empty() || token_of(r).is_some()) {
        let a = if r.archetypes.is_empty() { "-".to_string() } else { r.archetypes.iter().cloned().collect::<Vec<_>>().join("+") };
        *cross.entry((a, token_of(r).unwrap_or_else(|| "-".into()))).or_default() += 1;
    }
    println!("\ntemplate archetype x `_veh_` name token (models; `-` = none):");
    for ((a, t), n) in &cross {
        println!("  {a:22} {t:14} {n}");
    }
    let several: Vec<&Row> = rows.iter().filter(|r| r.archetypes.len() > 1).collect();
    println!("\nmodels whose templates are of several archetypes ({}):", several.len());
    for r in &several {
        println!("{}", line(r));
    }
    let is_vehicle = |r: &Row| r.archetypes.iter().any(|a| a != "Character");
    let hidden: Vec<&Row> = rows.iter().filter(|r| is_vehicle(r) && token_of(r).is_none()).collect();
    println!("\nmodels of a vehicle template whose name has no `_veh_` token ({}):", hidden.len());
    for r in &hidden {
        println!("{}", line(r));
    }
    let token_only: Vec<&Row> = rows.iter().filter(|r| !is_vehicle(r) && token_of(r).is_some()).collect();
    println!("\nmodels with a `_veh_` name token that no vehicle template names ({}):", token_only.len());
    for r in &token_only {
        println!("{}", line(r));
    }
    println!("\nunnamed models ({}):", count(Verdict::Unnamed));
    for r in rows.iter().filter(|r| r.verdict == Verdict::Unnamed) {
        println!("{}", line(r));
    }
    println!("\nunwitnessed names ({}):", count(Verdict::Unwitnessed));
    for r in rows.iter().filter(|r| r.verdict == Verdict::Unwitnessed) {
        println!("{}", line(r));
    }

    if let Some(p) = &tsv {
        let mut f = match std::fs::File::create(p) {
            Ok(f) => f,
            Err(e) => return setup_fail(format!("create {p}: {e}")),
        };
        let mut text = String::from("model\tverdict\tname\tcontent_names\tshared_tokens\tarchetypes\ttemplates\ttextures\n");
        for r in &rows {
            text.push_str(&format!(
                "0x{:08X}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                r.hash,
                r.verdict.label(),
                r.name.as_deref().unwrap_or(""),
                r.content_names.join(","),
                r.shared_tokens,
                r.archetypes.iter().cloned().collect::<Vec<_>>().join("+"),
                r.templates.join("; "),
                r.textures.join(" "),
            ));
        }
        if let Err(e) = f.write_all(text.as_bytes()) {
            return setup_fail(format!("write {p}: {e}"));
        }
        println!("\nwrote {p}");
    }

    if !failures.is_empty() {
        eprintln!("\n{} item(s) could not be read:", failures.len());
        for f in &failures {
            eprintln!("  {f}");
        }
    }
    let clean = failures.is_empty() && only_walked.is_empty() && only_aset.is_empty() && count(Verdict::Contradicted) == 0;
    if clean {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn texture_base_is_a_content_name() {
        // `pr_veh_car_t300_m60` is not a subsequence of its texture `pr_veh_car_t300_dm`, so the
        // pirate technical gets no content name from textures; the F150 does.
        let mut out = BTreeSet::new();
        subsequences_hashing_to("global_veh_fordf150_wheel_nm", pandemic_hash_m2("global_veh_fordf150"), &mut out);
        assert_eq!(out.into_iter().collect::<Vec<_>>(), strs(&["global_veh_fordf150"]));
    }

    #[test]
    fn a_name_its_textures_start_with_is_witnessed() {
        let (v, shared) = judge(Some("global_veh_fordf150"), &[], &strs(&["global_veh_fordf150_nm"]));
        assert!(v == Verdict::Witnessed);
        assert_eq!(shared, 3);
    }

    #[test]
    fn a_name_the_models_own_template_overrules_is_contradicted() {
        // Retail 0x42247B7F: the table's truck-damage string and the template's foliage name both hash to it.
        let h = pandemic_hash_m2("jungle_env_junglewallfoliage04");
        assert_eq!(h, pandemic_hash_m2("vz_veh_truck_m38_cab116_damage_nk"));
        let (v, _) = judge(
            Some("vz_veh_truck_m38_cab116_damage_nk"),
            &strs(&["jungle_env_junglewallfoliage04"]),
            &strs(&["jungle_env_plantlarge01"]),
        );
        assert!(v == Verdict::Contradicted);
    }

    #[test]
    fn a_name_nothing_spells_is_family_or_unwitnessed() {
        let texes = strs(&["pr_veh_car_t300_dm", "global_veh_civilianruin"]);
        assert!(judge(Some("pr_veh_car_t300_m60"), &[], &texes).0 == Verdict::Family);
        assert!(judge(Some("proutpost_bld_hq_ruin_lod.propattach07_ruin_lod1"), &[], &texes).0 == Verdict::Unwitnessed);
        assert!(judge(None, &[], &texes).0 == Verdict::Unnamed);
    }
}
