//! `vehicle-tuning` — the physics record of every vehicle template, field by field.
//!
//! A vehicle template of the `worldentity` carries one physics record: `_CarPhysicsV2`,
//! `_TankPhysics`, `_BoatPhysics`, `_HelicopterPhysics` (a flown helicopter also carries
//! `_HelicopterPhysicsAi`) or `_JetPhysics`. The class's `schm` gives each field a type, an offset
//! and `pandemic_hash_m2` of its name; the names are read from the rainbow table the `rainbow_table`
//! key of `.mercs2-local.toml` names, and a field it has no name for is headed by its hash.
//!
//! `--tsv <file>` (default: this repository's `docs/data/vehicle_tuning.tsv`) is the table per model:
//! one row for each distinct set of values the templates of a model give the fields `--fields` lists
//! (default [`SUMMARY_FIELDS`]), with the class, how many templates give them and the shortest of
//! those templates' names. A cell is empty where the class has no such field. A template with no
//! `ModelName` has an empty `model`.
//!
//! `--out <dir>` also writes one `<class>.tsv` per class: a row per template (its key, `Name`,
//! `ModelName` hash and that model's catalog name), a column per schema field in schema order, a
//! `Vec3` as three columns. Templates that share a record repeat its values.
//!
//! Only the archive named is read: no patch overlay is mounted.
//!
//! Exit 0 = every record of the six classes decoded; 1 = a record did not (listed); 2 = usage or
//! setup.
//!
//! ```text
//! mercs2_probe vehicle-tuning [--tsv <file>] [--out <dir>] [--fields A,B,...] [--names <names.json>] [--wad <path>]
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

use mercs2_engine::worldutil::TemplateRecords;
use mercs2_quartermaster::names::{NameTable, PRODUCTION_NAMES};

use crate::we_tables::{self, cells, field_label, heads};

/// The physics classes a vehicle template carries, as `mercs2_engine::game_world::unit_archetype`
/// reads them.
pub const CLASSES: [&str; 6] =
    ["_CarPhysicsV2", "_TankPhysics", "_BoatPhysics", "_HelicopterPhysics", "_HelicopterPhysicsAi", "_JetPhysics"];

/// The fields of the per-model table when `--fields` is not given.
pub const SUMMARY_FIELDS: [&str; 12] = [
    "Mass",
    "MaxSpeed",
    "ReverseMaxSpeed",
    "MaxSpeedReverse",
    "EngineTorque",
    "EngineReverseTorque",
    "MaxFwdSpeed",
    "FwdSpeedAcc",
    "MaxRightSpeed",
    "RightSpeedAcc",
    "MaxElevationSpeed",
    "MaxYawSpeed",
];

const DEFAULT_TSV: [&str; 2] = ["docs", "data/vehicle_tuning.tsv"];

fn setup_fail(msg: String) -> i32 {
    eprintln!("vehicle-tuning: {msg}");
    2
}

/// The cells of `summary` out of one record's row: the cell under the head of that name, or empty
/// where the class has no such column.
fn pick(summary: &[String], head: &[String], row: &[String]) -> Vec<String> {
    summary
        .iter()
        .map(|s| head.iter().position(|h| h == s).map(|i| row[i].clone()).unwrap_or_default())
        .collect()
}

/// A model's rows of the per-model table: each distinct set of summary values, with how many
/// templates give it and the shortest of their names (the first in order among equals).
#[derive(Default)]
struct ModelRows(BTreeMap<Vec<String>, (usize, String)>);

impl ModelRows {
    fn count(&mut self, values: Vec<String>, template: &str) {
        let slot = self.0.entry(values).or_default();
        slot.0 += 1;
        if slot.1.is_empty() || (template.len(), template) < (slot.1.len(), slot.1.as_str()) {
            slot.1 = template.to_string();
        }
    }
}

pub fn run(
    wadpath: &str,
    tsv: Option<String>,
    out: Option<String>,
    fields: Option<String>,
    names_arg: Option<String>,
) -> i32 {
    let tsv_path: PathBuf = match tsv {
        Some(p) => PathBuf::from(p),
        None => mercs2_formats::game_paths::repo_root().join(DEFAULT_TSV[0]).join(DEFAULT_TSV[1]),
    };
    let out = out.map(PathBuf::from);
    if let Some(dir) = &out {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return setup_fail(format!("create {}: {e}", dir.display()));
        }
    }
    let summary: Vec<String> = match fields {
        Some(f) => f.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        None => SUMMARY_FIELDS.iter().map(|s| s.to_string()).collect(),
    };
    if summary.is_empty() {
        return setup_fail("--fields names no field".into());
    }
    let names_path: PathBuf = match names_arg {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(PRODUCTION_NAMES),
    };
    let names = match NameTable::load(&names_path) {
        Ok(n) => n,
        Err(e) => return setup_fail(e.to_string()),
    };

    let we = match we_tables::load(wadpath) {
        Ok(w) => w,
        Err(e) => return setup_fail(e),
    };
    let records = match TemplateRecords::from_worldentity(&we) {
        Ok(r) => r,
        Err(e) => return setup_fail(format!("template records: {e}")),
    };
    let template_names: HashMap<u32, &str> = records.names.iter().map(|(k, n)| (*k, n.as_str())).collect();
    let schema_hashes: BTreeSet<u32> = CLASSES
        .iter()
        .flat_map(|class| we.groups_of(class))
        .flat_map(|c| c.schema.fields.iter().map(|f| f.name_hash))
        .collect();
    let field_names = match we_tables::field_names(&schema_hashes) {
        Ok(n) => n,
        Err(e) => return setup_fail(e),
    };

    let mut failures: Vec<String> = Vec::new();
    let mut seen_fields: BTreeSet<String> = BTreeSet::new();
    // (class, model name, model hash) -> that model's rows.
    let mut by_model: Vec<(&str, BTreeMap<(String, u32), ModelRows>)> = Vec::new();
    for class in CLASSES {
        let groups: Vec<_> = we.groups_of(class).collect();
        if groups.is_empty() {
            return setup_fail(format!("the worldentity has no {class} class"));
        }
        let mut class_tsv = match &out {
            None => None,
            Some(dir) => {
                let path = dir.join(format!("{class}.tsv"));
                match std::fs::File::create(&path) {
                    Ok(f) => Some((std::io::BufWriter::new(f), path)),
                    Err(e) => return setup_fail(format!("create {}: {e}", path.display())),
                }
            }
        };
        let mut models: BTreeMap<(String, u32), ModelRows> = BTreeMap::new();
        let (mut n_records, mut n_templates) = (0usize, 0usize);
        let mut wrote_head = false;
        for c in &groups {
            for r in &c.records {
                n_records += 1;
                let values = match c.decode(&r.payload) {
                    Ok(v) => v,
                    Err(e) => {
                        failures.push(format!("{class}: record of 0x{:08X}: {e}", r.keys.first().copied().unwrap_or(0)));
                        continue;
                    }
                };
                let (mut head, mut row): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
                for (f, v) in c.schema.fields.iter().zip(&values) {
                    let cs = cells(f.field_type, v);
                    head.extend(heads(&field_label(&field_names, f.name_hash), cs.len()));
                    row.extend(cs);
                }
                seen_fields.extend(head.iter().cloned());
                if let Some((w, path)) = &mut class_tsv {
                    if !wrote_head {
                        if let Err(e) = writeln!(w, "key\ttemplate\tmodel_hash\tmodel\t{}", head.join("\t")) {
                            return setup_fail(format!("write {}: {e}", path.display()));
                        }
                    }
                }
                wrote_head = true;
                let picked = pick(&summary, &head, &row);
                for &key in &r.keys {
                    n_templates += 1;
                    let template = template_names.get(&key).map(|s| s.to_string()).unwrap_or_else(|| format!("0x{key:08X}"));
                    let model = records.models.get(&key).copied();
                    let model_hash = model.map(|m| format!("0x{m:08X}")).unwrap_or_default();
                    let model_name = model.and_then(|m| names.reverse(m)).unwrap_or("").to_string();
                    if let Some((w, path)) = &mut class_tsv {
                        if let Err(e) = writeln!(w, "0x{key:08X}\t{template}\t{model_hash}\t{model_name}\t{}", row.join("\t")) {
                            return setup_fail(format!("write {}: {e}", path.display()));
                        }
                    }
                    models.entry((model_name, model.unwrap_or(0))).or_default().count(picked.clone(), &template);
                }
            }
        }
        if let Some((mut w, path)) = class_tsv {
            if let Err(e) = w.flush() {
                return setup_fail(format!("write {}: {e}", path.display()));
            }
        }
        println!("{class}: {n_records} record(s), {n_templates} template(s), {} model(s)", models.len());
        by_model.push((class, models));
    }
    if let Some(unknown) = summary.iter().find(|s| !seen_fields.contains(*s)) {
        return setup_fail(format!("--fields: no vehicle physics class has a field {unknown:?}"));
    }

    let mut text = format!("class\tmodel\tmodel_hash\ttemplates\tshortest_template\t{}\n", summary.join("\t"));
    for (class, models) in &by_model {
        for ((model_name, model), rows) in models {
            let hash = if *model == 0 { String::new() } else { format!("0x{model:08X}") };
            for (values, (n, template)) in &rows.0 {
                text.push_str(&format!("{class}\t{model_name}\t{hash}\t{n}\t{template}\t{}\n", values.join("\t")));
            }
        }
    }
    if let Err(e) = std::fs::write(&tsv_path, text) {
        return setup_fail(format!("write {}: {e}", tsv_path.display()));
    }
    println!("wrote {}", tsv_path.display());

    if failures.is_empty() {
        return 0;
    }
    eprintln!("vehicle-tuning: {} record(s) did not decode:", failures.len());
    for f in &failures {
        eprintln!("  {f}");
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_the_class_lacks_is_an_empty_cell() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let summary = s(&["Mass", "MaxSpeed", "MaxFwdSpeed"]);
        let head = s(&["Mass", "MaxElevationSpeed", "MaxFwdSpeed"]);
        let row = s(&["6000", "10", "30"]);
        assert_eq!(pick(&summary, &head, &row), ["6000", "", "30"]);
    }

    #[test]
    fn a_models_row_counts_its_templates_and_keeps_the_shortest_name() {
        let mut rows = ModelRows::default();
        let v = vec!["2000".to_string(), "105".to_string()];
        rows.count(v.clone(), "Valiant (Driver)");
        rows.count(v.clone(), "Valiant");
        rows.count(v.clone(), "Valiant (Driver) (Civ poor male)");
        rows.count(vec!["2000".to_string(), "95".to_string()], "Valiant (4door)");
        assert_eq!(rows.0[&v], (3, "Valiant".to_string()));
        assert_eq!(rows.0.len(), 2);
    }
}
