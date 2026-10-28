//! `we-template` — the retail `worldentity` read through `mercs2_formats::worldentity`, resolved by name
//! through the WAD stack (`AssetSource`, patch overlays included).
//!
//! - `we-template <name|0xKEY>…`: every component record each template carries (its `Name` resolves to
//!   every key that bears it).
//! - `we-template --class <Class>`: every record of a component class, with its keys.
//! - `we-template --uses <0xU32>`: every record whose payload holds that u32 at a 4-aligned offset.
//! - `we-template --enums`: every enum table of the container, value by value.
//!
//! `--class` also prints the class schema (offset, type, field name from the rainbow table).
//!
//! Payloads print as their u32 words (`Fixed`), the name (`Name`), or the variant.

use mercs2_engine::asset::AssetSource;
use mercs2_formats::worldentity::{Payload, WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};

pub enum Query {
    Templates(Vec<String>),
    Class(String),
    Uses(u32),
    Enums,
}

fn payload_str(p: &Payload) -> String {
    match p {
        Payload::Fixed(b) => b
            .chunks(4)
            .map(|c| match c {
                [a, b, c, d] => format!("{:08X}", u32::from_le_bytes([*a, *b, *c, *d])),
                tail => tail.iter().map(|x| format!("{x:02X}")).collect::<String>(),
            })
            .collect::<Vec<_>>()
            .join(" "),
        Payload::Name { name, flag } => format!("{name:?} flag {flag}"),
        Payload::PointLocation { text, .. } => format!("point {text:?}"),
        Payload::PackedU16(v) => format!("u16 {v:#06X}"),
    }
}

pub fn we_template(wadpath: &str, q: Query) -> Result<(), String> {
    let mut assets = AssetSource::discover(wadpath, &[])?;
    let bytes = assets
        .resolve(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)
        .ok_or("worldentity: in no open wad")?;
    let we = WorldEntity::parse(&bytes).map_err(|e| format!("worldentity: {e}"))?;
    let names = we.names()?;
    let name_of = |k: u32| -> String {
        names
            .iter()
            .find(|(_, ks)| ks.contains(&k))
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| "?".into())
    };
    match q {
        Query::Templates(wanted) => {
            for w in wanted {
                let keys: Vec<u32> = match w.strip_prefix("0x").map(|h| u32::from_str_radix(h, 16)) {
                    Some(Ok(k)) => vec![k],
                    Some(Err(e)) => return Err(format!("{w:?}: {e}")),
                    None => names
                        .iter()
                        .filter(|(n, _)| n.eq_ignore_ascii_case(&w))
                        .flat_map(|(_, ks)| ks.iter().copied())
                        .collect(),
                };
                if keys.is_empty() {
                    return Err(format!("{w:?}: no template Name"));
                }
                for k in keys {
                    println!("template 0x{k:08X} {:?}", name_of(k));
                    for c in &we.components {
                        for r in c.records.iter().filter(|r| r.keys.contains(&k)) {
                            println!("  {:<28} {}", c.class, payload_str(&r.payload));
                        }
                    }
                }
            }
        }
        Query::Class(class) => {
            let mut any = false;
            let table = mercs2_formats::game_paths::local_config_rainbow_table(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))?;
            for c in we.groups_of(&class) {
                any = true;
                let hashes = c.schema.fields.iter().map(|f| f.name_hash).collect();
                let field_names = mercs2_engine::worldutil::rainbow_names(&table, &hashes)?;
                println!("{class} version {} payload {} bytes, schema:", c.version, c.schema.payload_stride);
                for f in &c.schema.fields {
                    let bits = if f.bit_width == 0 { String::new() } else { format!(" bits {}+{}", f.bit_start, f.bit_width) };
                    let name = field_names.get(&f.name_hash).map(String::as_str).unwrap_or("?");
                    println!("  +0x{:02X}{bits}  {:?}  0x{:08X} {name}", f.byte_offset, f.field_type, f.name_hash);
                }
                for r in &c.records {
                    let ks: Vec<String> = r.keys.iter().map(|&k| format!("0x{k:08X} {:?}", name_of(k))).collect();
                    println!("{}  <- {}", payload_str(&r.payload), ks.join(", "));
                }
            }
            if !any {
                return Err(format!("no component class {class:?}"));
            }
        }
        Query::Enums => {
            for e in &we.enums {
                let vals: Vec<String> = e.values.iter().map(|v| format!("{}={}", v.value, v.name)).collect();
                println!("{}: {}", e.name, vals.join(" "));
            }
        }
        Query::Uses(v) => {
            for c in &we.components {
                for r in &c.records {
                    let Payload::Fixed(b) = &r.payload else { continue };
                    if b.chunks_exact(4).any(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]) == v) {
                        let ks: Vec<String> = r.keys.iter().map(|&k| format!("0x{k:08X} {:?}", name_of(k))).collect();
                        println!("{:<28} {}  <- {}", c.class, payload_str(&r.payload), ks.join(", "));
                    }
                }
            }
        }
    }
    Ok(())
}
