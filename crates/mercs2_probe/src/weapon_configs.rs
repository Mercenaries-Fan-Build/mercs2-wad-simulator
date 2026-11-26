//! `weapon-configs` — every weapon template of the `worldentity`, and every template it reaches.
//!
//! A weapon is a template with a `WeaponProjectileBase` record (a gun, a launcher, a mounted
//! weapon) or a `WeaponThrown` record (a grenade, C4, a designator). Its other records are the rest
//! of its configuration (`WeaponScatter`, `WeaponHint`, `WeaponUI`, `WeaponEffects`, `Equipment`,
//! `Pickup`, …), and a `Hash` field of a record can hold another template's key: `AmmoTemplate` is
//! the projectile the weapon fires, that projectile's `SpawnOnDeath` the explosion it leaves. The
//! export follows every such key from every weapon, so a reached template is listed whether or not
//! its class is a weapon class.
//!
//! Written into `--dir <dir>` (default: this repository's `docs/data/`):
//!
//! * `weapon_tuning.tsv` — a row per weapon: its key, `Name`, model, the template its
//!   `AmmoTemplate` names (`ammo`), the `Explosive` template that ammo spawns through
//!   `SpawnOnDeath.SpawnGuid` or `TimerResponse.SpawnGuid` (`blast`), then the fields of [`SUMMARY`]:
//!   `Class.Field` is the weapon's own, `ammo.Class.Field` its ammo's, `blast.Class.Field` its
//!   blast's. A cell is empty when the template has no record of the class, and holds its values
//!   separated by ` | ` when it has several.
//! * `weapon_fields.tsv` — a row per field of every record of every weapon and reached template:
//!   `template_key`, `template`, `reached_from` (`weapon`, or the key and `Class.Field` that first
//!   named the template, breadth first from the weapons in key order), `class`, `record` (0-based,
//!   among the template's records of that class), `field`, `type`, `value`, `value_name`. A `Vec3`
//!   or blob value is its numbers separated by spaces. `value_name` is the `Name` of the template a
//!   `Hash` value is the key of, else the catalog name that hashes to it, else empty.
//! * `worldentity_enums.tsv` — the container's enum tables (`enum`, `value`, `member`). An `Enum`
//!   field's value is written as its number: the container does not say which table a field reads.
//!
//! Field names are read from the rainbow table the `rainbow_table` key of `.mercs2-local.toml`
//! names; a field it has no name for is written as its hash. Only the archive named is read: no
//! patch overlay is mounted.
//!
//! Exit 0 = every record decoded; 1 = a record did not (listed); 2 = usage or setup.
//!
//! ```text
//! mercs2_probe weapon-configs [--dir <dir>] [--names <names.json>] [--wad <path>]
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};

use mercs2_engine::worldutil::TemplateRecords;
use mercs2_formats::schema::{FieldValue, SchemaField, SchemaFieldType};
use mercs2_formats::worldentity::{Value, WorldEntity};
use mercs2_quartermaster::names::{NameTable, PRODUCTION_NAMES};

use crate::we_tables::{self, cells, field_label};

/// A template with a record of one of these is a weapon.
pub const WEAPON_CLASSES: [&str; 2] = ["WeaponProjectileBase", "WeaponThrown"];

/// The record that names a template; its text is the `template` column, not a field row.
const NAME_CLASS: &str = "Name";

const SUMMARY_FILE: &str = "weapon_tuning.tsv";
const FIELDS_FILE: &str = "weapon_fields.tsv";
const ENUMS_FILE: &str = "worldentity_enums.tsv";

/// The field of a weapon class that holds the key of the template the weapon fires or throws.
const AMMO_FIELD: &str = "AmmoTemplate";
/// The fields of an ammo template that hold the key of a template it spawns: when it dies, and when
/// its timer runs out.
const SPAWN_FIELDS: [(&str, &str); 2] = [("SpawnOnDeath", "SpawnGuid"), ("TimerResponse", "SpawnGuid")];
/// A spawned template with a record of this class is the ammo's blast.
const BLAST_CLASS: &str = "Explosive";
/// Between the values of a cell that has several (two records of a class, two ammo templates).
const SEVERAL: &str = " | ";

/// Whose record a column of `weapon_tuning.tsv` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// The weapon template itself.
    Weapon,
    /// The template its [`AMMO_FIELD`] names.
    Ammo,
    /// The [`BLAST_CLASS`] template its ammo spawns ([`SPAWN_FIELDS`]).
    Blast,
}

/// The columns of `weapon_tuning.tsv` after the names: a role, a class and a field of it.
const SUMMARY: [(Role, &str, &str); 38] = [
    (Role::Weapon, "WeaponProjectileBase", "FireType"),
    (Role::Weapon, "WeaponProjectileBase", "SpecialCaseType"),
    (Role::Weapon, "WeaponProjectileBase", "RateOfFire"),
    (Role::Weapon, "WeaponProjectileBase", "iClipSize"),
    (Role::Weapon, "WeaponProjectileBase", "MaxAmmoReserve"),
    (Role::Weapon, "WeaponProjectileBase", "MaxAmmoReserveModifier"),
    (Role::Weapon, "WeaponProjectileBase", "iBulletsPerShot"),
    (Role::Weapon, "WeaponProjectileBase", "iRoundsPerReload"),
    (Role::Weapon, "WeaponThrown", "MaxAmmoReserve"),
    (Role::Weapon, "WeaponThrown", "MaxAmmoReserveModifier"),
    (Role::Weapon, "WeaponScatter", "ScatterMin"),
    (Role::Weapon, "WeaponScatter", "ScatterMax"),
    (Role::Weapon, "WeaponScatter", "ScatterPerShot"),
    (Role::Weapon, "WeaponScatter", "ScatterAimModeMin"),
    (Role::Weapon, "WeaponScatter", "ScatterAimModeMax"),
    (Role::Weapon, "WeaponHint", "MinFireRange"),
    (Role::Weapon, "WeaponHint", "FireRange"),
    (Role::Weapon, "HomingWeapon", "TYPE"),
    (Role::Weapon, "HomingWeapon", "LockOnTime"),
    (Role::Weapon, "HomingWeapon", "LockOnMaxDistance"),
    (Role::Weapon, "WeaponRecoilVehicle", "PhysicalRecoil"),
    (Role::Weapon, "WeaponScope", "MaxZoomLevel"),
    (Role::Weapon, "Equipment", "EquipmentClass"),
    (Role::Ammo, "ProjectilePhysics", "DAMAGE"),
    (Role::Ammo, "ProjectilePhysics", "DamageMinimum"),
    (Role::Ammo, "ProjectilePhysics", "DamageDropoffStart"),
    (Role::Ammo, "ProjectilePhysics", "DamageDropoffStop"),
    (Role::Ammo, "ProjectilePhysics", "HeroMultiplier"),
    (Role::Ammo, "ProjectilePhysics", "GravityScale"),
    (Role::Ammo, "ControllerVelocity", "Velocity"),
    (Role::Ammo, "ControllerVelocity", "MaxVelocity"),
    (Role::Ammo, "ControllerVelocity", "Accel"),
    (Role::Ammo, "HomingProjectile", "TurnSpeed"),
    (Role::Ammo, "TriggerOnTimer", "TIME"),
    (Role::Blast, "Explosive", "Radius"),
    (Role::Blast, "Explosive", "DAMAGE"),
    (Role::Blast, "Explosive", "MinDamage"),
    (Role::Blast, "Explosive", "MaxForce"),
];

/// The head of a `weapon_tuning.tsv` column: `Class.Field`, behind `ammo.` or `blast.` for a field
/// of those templates.
fn column_head(role: Role, class: &str, field: &str) -> String {
    match role {
        Role::Weapon => format!("{class}.{field}"),
        Role::Ammo => format!("ammo.{class}.{field}"),
        Role::Blast => format!("blast.{class}.{field}"),
    }
}

/// One value a template gives a field.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shown {
    value: String,
    /// The name a `Hash` value stands for; empty when it has none.
    value_name: String,
    /// The value, when it is the key of a template of the container.
    template: Option<u32>,
}

/// A `weapon_tuning.tsv` cell: each value by its name when it has one, several joined by
/// [`SEVERAL`], empty for none.
fn cell(shown: &[Shown]) -> String {
    shown
        .iter()
        .map(|s| if s.value_name.is_empty() { s.value.as_str() } else { s.value_name.as_str() })
        .collect::<Vec<_>>()
        .join(SEVERAL)
}

fn setup_fail(msg: String) -> i32 {
    eprintln!("weapon-configs: {msg}");
    2
}

/// How a template entered the export.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Reached {
    Weapon,
    /// The template whose record named this one, and that record's class and field hash.
    From { key: u32, class: String, field: u32 },
}

/// The templates the weapons reach: each weapon, then every template a `Hash` field of an exported
/// record holds the key of, breadth first from the weapons in key order. `refs(key)` lists the
/// `(class, field hash, value)` of a template's `Hash` fields; a value is followed when `is_key`
/// says the container has a template of that key.
fn reach(
    weapons: &BTreeSet<u32>,
    refs: impl Fn(u32) -> Vec<(String, u32, u32)>,
    is_key: impl Fn(u32) -> bool,
) -> BTreeMap<u32, Reached> {
    let mut out: BTreeMap<u32, Reached> = weapons.iter().map(|&k| (k, Reached::Weapon)).collect();
    let mut queue: VecDeque<u32> = weapons.iter().copied().collect();
    while let Some(key) = queue.pop_front() {
        for (class, field, value) in refs(key) {
            if value == key || !is_key(value) || out.contains_key(&value) {
                continue;
            }
            out.insert(value, Reached::From { key, class, field });
            queue.push_back(value);
        }
    }
    out
}

/// Every record of the container by the template keys that hold it: `(component, record)` indices
/// in container order.
fn records_by_key(we: &WorldEntity) -> HashMap<u32, Vec<(usize, usize)>> {
    let mut index: HashMap<u32, Vec<(usize, usize)>> = HashMap::new();
    for (ci, c) in we.components.iter().enumerate() {
        for (ri, r) in c.records.iter().enumerate() {
            for &k in &r.keys {
                index.entry(k).or_default().push((ci, ri));
            }
        }
    }
    index
}

pub fn run(wadpath: &str, dir: Option<String>, names_arg: Option<String>) -> i32 {
    let dir: PathBuf = match dir {
        Some(d) => PathBuf::from(d),
        None => mercs2_formats::game_paths::repo_root().join("docs").join("data"),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return setup_fail(format!("create {}: {e}", dir.display()));
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
    let index = records_by_key(&we);

    // Every record a template holds, decoded once.
    let mut failures: Vec<String> = Vec::new();
    let mut decoded: HashMap<(usize, usize), Vec<Value>> = HashMap::new();
    for (ci, c) in we.components.iter().enumerate() {
        if c.class == NAME_CLASS {
            continue;
        }
        for (ri, r) in c.records.iter().enumerate() {
            match c.decode(&r.payload) {
                Ok(v) => {
                    decoded.insert((ci, ri), v);
                }
                Err(e) => failures.push(format!("{}: record of 0x{:08X}: {e}", c.class, r.keys.first().copied().unwrap_or(0))),
            }
        }
    }

    let mut weapons: BTreeSet<u32> = BTreeSet::new();
    for class in WEAPON_CLASSES {
        let mut any = false;
        for c in we.groups_of(class) {
            any = true;
            weapons.extend(c.keys());
        }
        if !any {
            return setup_fail(format!("the worldentity has no {class} class"));
        }
    }
    let refs = |key: u32| -> Vec<(String, u32, u32)> {
        let mut out = Vec::new();
        for &(ci, ri) in index.get(&key).map(Vec::as_slice).unwrap_or(&[]) {
            let c = &we.components[ci];
            let Some(values) = decoded.get(&(ci, ri)) else { continue };
            for (f, v) in c.schema.fields.iter().zip(values) {
                if let (SchemaFieldType::Hash, Value::Field(FieldValue::U32(w))) = (f.field_type, v) {
                    out.push((c.class.clone(), f.name_hash, *w));
                }
            }
        }
        out
    };
    let reached = reach(&weapons, refs, |k| index.contains_key(&k));

    let schema_hashes: BTreeSet<u32> = reached
        .keys()
        .flat_map(|k| index.get(k).into_iter().flatten())
        .flat_map(|&(ci, _)| we.components[ci].schema.fields.iter().map(|f| f.name_hash))
        .chain(reached.values().filter_map(|r| match r {
            Reached::From { field, .. } => Some(*field),
            Reached::Weapon => None,
        }))
        .collect();
    let field_names = match we_tables::field_names(&schema_hashes) {
        Ok(n) => n,
        Err(e) => return setup_fail(e),
    };
    let template_label =
        |k: u32| template_names.get(&k).map(|s| s.to_string()).unwrap_or_else(|| format!("0x{k:08X}"));
    // A field's value as written, and the name a `Hash` value stands for (empty when it has none).
    let show = |f: &SchemaField, v: &Value| -> (String, String) {
        let value_name = match (f.field_type, v) {
            (SchemaFieldType::Hash, Value::Field(FieldValue::U32(w))) if *w != 0 => {
                if index.contains_key(w) {
                    template_label(*w)
                } else {
                    names.reverse(*w).unwrap_or("").to_string()
                }
            }
            _ => String::new(),
        };
        (cells(f.field_type, v).join(" "), value_name)
    };
    // What a template gives `class.field`, over its records of that class in order.
    let field_values = |key: u32, class: &str, field: &str| -> Vec<Shown> {
        let mut out = Vec::new();
        for &(ci, ri) in index.get(&key).map(Vec::as_slice).unwrap_or(&[]) {
            let c = &we.components[ci];
            if c.class != class {
                continue;
            }
            let Some(values) = decoded.get(&(ci, ri)) else { continue };
            for (f, v) in c.schema.fields.iter().zip(values) {
                if field_label(&field_names, f.name_hash) != field {
                    continue;
                }
                let (value, value_name) = show(f, v);
                let template = match (f.field_type, v) {
                    (SchemaFieldType::Hash, Value::Field(FieldValue::U32(w))) if index.contains_key(w) => Some(*w),
                    _ => None,
                };
                out.push(Shown { value, value_name, template });
            }
        }
        out
    };
    let has_class = |key: u32, class: &str| {
        index.get(&key).into_iter().flatten().any(|&(ci, _)| we.components[ci].class == class)
    };
    for (class, field) in SUMMARY.iter().map(|&(_, c, f)| (c, f)).chain(WEAPON_CLASSES.map(|c| (c, AMMO_FIELD))).chain(SPAWN_FIELDS) {
        let known = we
            .groups_of(class)
            .any(|c| c.schema.fields.iter().any(|f| field_label(&field_names, f.name_hash) == field));
        if !known {
            return setup_fail(format!("{class} has no field the rainbow table names {field:?}"));
        }
    }

    let mut text = String::from("template_key\ttemplate\treached_from\tclass\trecord\tfield\ttype\tvalue\tvalue_name\n");
    let mut n_rows = 0usize;
    for (&key, how) in &reached {
        let from = match how {
            Reached::Weapon => "weapon".to_string(),
            Reached::From { key, class, field } => format!("0x{key:08X} {class}.{}", field_label(&field_names, *field)),
        };
        let mut nth: HashMap<usize, usize> = HashMap::new();
        for &(ci, ri) in index.get(&key).map(Vec::as_slice).unwrap_or(&[]) {
            let c = &we.components[ci];
            let Some(values) = decoded.get(&(ci, ri)) else { continue };
            let record = *nth.entry(ci).and_modify(|n| *n += 1).or_insert(0);
            for (f, v) in c.schema.fields.iter().zip(values) {
                let (value, value_name) = show(f, v);
                let bits = if f.bit_width == 0 { String::new() } else { format!(":{}", f.bit_width) };
                text.push_str(&format!(
                    "0x{key:08X}\t{}\t{from}\t{}\t{record}\t{}\t{:?}{bits}\t{value}\t{value_name}\n",
                    template_label(key),
                    c.class,
                    field_label(&field_names, f.name_hash),
                    f.field_type,
                ));
                n_rows += 1;
            }
        }
    }
    let fields_path = dir.join(FIELDS_FILE);
    if let Err(e) = std::fs::write(&fields_path, text) {
        return setup_fail(format!("write {}: {e}", fields_path.display()));
    }

    // The table per weapon: its own fields, its ammo's, and its blast's.
    let templates_of = |shown: Vec<Shown>| -> Vec<u32> {
        let mut keys: Vec<u32> = Vec::new();
        for k in shown.into_iter().filter_map(|s| s.template) {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        keys
    };
    let mut summary = String::from("weapon_key\tweapon\tmodel\tammo\tblast");
    for &(role, class, field) in &SUMMARY {
        summary.push('\t');
        summary.push_str(&column_head(role, class, field));
    }
    summary.push('\n');
    for &key in &weapons {
        let ammo = templates_of(WEAPON_CLASSES.iter().flat_map(|c| field_values(key, c, AMMO_FIELD)).collect());
        let blast: Vec<u32> = templates_of(
            ammo.iter().flat_map(|&a| SPAWN_FIELDS.iter().flat_map(move |&(c, f)| field_values(a, c, f))).collect(),
        )
        .into_iter()
        .filter(|&k| has_class(k, BLAST_CLASS))
        .collect();
        let model = match records.models.get(&key) {
            Some(&m) => names.reverse(m).map(str::to_string).unwrap_or_else(|| format!("0x{m:08X}")),
            None => String::new(),
        };
        let labels = |keys: &[u32]| keys.iter().map(|&k| template_label(k)).collect::<Vec<_>>().join(SEVERAL);
        summary.push_str(&format!("0x{key:08X}\t{}\t{model}\t{}\t{}", template_label(key), labels(&ammo), labels(&blast)));
        for &(role, class, field) in &SUMMARY {
            let of: &[u32] = match role {
                Role::Weapon => std::slice::from_ref(&key),
                Role::Ammo => &ammo,
                Role::Blast => &blast,
            };
            let shown: Vec<Shown> = of.iter().flat_map(|&k| field_values(k, class, field)).collect();
            summary.push('\t');
            summary.push_str(&cell(&shown));
        }
        summary.push('\n');
    }
    let summary_path = dir.join(SUMMARY_FILE);
    if let Err(e) = std::fs::write(&summary_path, summary) {
        return setup_fail(format!("write {}: {e}", summary_path.display()));
    }

    let mut enums = String::from("enum\tvalue\tmember\n");
    for e in &we.enums {
        for v in &e.values {
            enums.push_str(&format!("{}\t{}\t{}\n", e.name, v.value, v.name));
        }
    }
    let enums_path = dir.join(ENUMS_FILE);
    if let Err(e) = std::fs::write(&enums_path, enums) {
        return setup_fail(format!("write {}: {e}", enums_path.display()));
    }

    let n_reached = reached.values().filter(|r| **r != Reached::Weapon).count();
    println!("{} weapon template(s), {n_reached} template(s) reached from them", weapons.len());
    println!("wrote {} ({} weapons)", summary_path.display(), weapons.len());
    println!("wrote {} ({n_rows} field rows)", fields_path.display());
    println!("wrote {} ({} enum tables)", enums_path.display(), we.enums.len());

    if failures.is_empty() {
        return 0;
    }
    eprintln!("weapon-configs: {} record(s) did not decode:", failures.len());
    for f in &failures {
        eprintln!("  {f}");
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weapon_reaches_its_ammo_and_what_the_ammo_spawns() {
        // 1 = rifle -> 2 = bullet -> 3 = blast; 4 = a template nothing names; 9 is not a template.
        let weapons: BTreeSet<u32> = [1].into();
        let refs = |k: u32| -> Vec<(String, u32, u32)> {
            match k {
                1 => vec![("WeaponProjectileBase".into(), 0xC5A7_D745, 2), ("WeaponBarrel".into(), 0xC780_F3A8, 9)],
                2 => vec![("SpawnOnDeath".into(), 0x7461_C0CD, 3), ("SpawnOnDeath".into(), 0x1DE5_C824, 1)],
                _ => vec![],
            }
        };
        let got = reach(&weapons, refs, |k| (1..=4).contains(&k));
        assert_eq!(got.keys().copied().collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(got[&1], Reached::Weapon);
        assert_eq!(got[&2], Reached::From { key: 1, class: "WeaponProjectileBase".into(), field: 0xC5A7_D745 });
        assert_eq!(got[&3], Reached::From { key: 2, class: "SpawnOnDeath".into(), field: 0x7461_C0CD });
    }

    #[test]
    fn a_cell_prefers_a_name_and_joins_several() {
        let s = |value: &str, name: &str| Shown { value: value.into(), value_name: name.into(), template: None };
        assert_eq!(cell(&[]), "");
        assert_eq!(cell(&[s("450", "")]), "450");
        assert_eq!(cell(&[s("0x8000A5F2", "Explosion (Handheld RPG)")]), "Explosion (Handheld RPG)");
        assert_eq!(cell(&[s("5", ""), s("15", "")]), "5 | 15");
    }

    #[test]
    fn a_column_is_headed_by_whose_record_it_reads() {
        assert_eq!(column_head(Role::Weapon, "WeaponProjectileBase", "iClipSize"), "WeaponProjectileBase.iClipSize");
        assert_eq!(column_head(Role::Ammo, "ProjectilePhysics", "DAMAGE"), "ammo.ProjectilePhysics.DAMAGE");
        assert_eq!(column_head(Role::Blast, "Explosive", "Radius"), "blast.Explosive.Radius");
    }

    #[test]
    fn a_template_two_weapons_reach_is_listed_once_from_the_first() {
        let weapons: BTreeSet<u32> = [5, 6].into();
        let refs = |k: u32| -> Vec<(String, u32, u32)> {
            match k {
                5 | 6 => vec![("WeaponProjectileBase".into(), 0xC5A7_D745, 7)],
                _ => vec![],
            }
        };
        let got = reach(&weapons, refs, |k| (5..=7).contains(&k));
        assert_eq!(got.len(), 3);
        assert_eq!(got[&7], Reached::From { key: 5, class: "WeaponProjectileBase".into(), field: 0xC5A7_D745 });
    }
}
