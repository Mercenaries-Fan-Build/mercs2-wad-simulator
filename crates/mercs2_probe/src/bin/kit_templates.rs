//! Which world templates wear a given model, and — for a character kit — which outfits each can show.
//!
//! Reads the resident `worldentity` (type `0x5647C35D`, resolved as the singleton it is through
//! `mercs2_engine::registry`) through `mercs2_formats::worldentity` and its templates through `mercs2_formats::mixr::kit_templates` (`ModelName`, `Name`, `ModelMixerProfile`).
//!
//! * default: each queried model's templates by `Name`;
//! * `--components`: every component record of each of those templates;
//! * `--profiles`: the model's `MIXR` (`mercs2_formats::mixr`) — each part's exclusions, each
//!   profile's allowed parts and outfits, and the templates that pick it;
//! * `--all-kits`: `--profiles` over every ASET model whose own container carries a `MIXR`, with a
//!   count of templates whose `ModelMixerProfile` names no profile of their kit;
//! * `--factions`: every template with a `ModelName`, one tab-separated line each —
//!   `model hash · FactionMarker hash (or -) · template name` — to join against model names.
//!
//! Exit 0 = everything read; 1 = the worldentity, a model container or a MIXR could not be read; 2 = usage.
//!
//! ```text
//! cargo run -p mercs2_probe --bin kit_templates -- 0x4D4F42B5 [--profiles] [--components] [--wad path]
//! cargo run -p mercs2_probe --bin kit_templates -- --all-kits [--wad path]
//! cargo run -p mercs2_probe --bin kit_templates -- --factions [--wad path]
//! ```

use mercs2_engine::registry::AssetRegistry;
use mercs2_engine::wad;
use mercs2_engine::worldutil::hash_or_name;
use mercs2_formats::mixr::{kit_templates, KitTemplate, Mixr};
use mercs2_formats::worldentity::{WorldEntity, WORLDENTITY_TYPE_HASH};
use std::collections::BTreeMap;

fn fail(msg: String) -> ! {
    eprintln!("kit_templates: {msg}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wad_arg = args.iter().position(|a| a == "--wad").and_then(|i| args.get(i + 1)).cloned();
    let all_kits = args.iter().any(|a| a == "--all-kits");
    let factions = args.iter().any(|a| a == "--factions");
    let profiles = all_kits || args.iter().any(|a| a == "--profiles");
    let show_components = args.iter().any(|a| a == "--components");
    let mut targets: Vec<u32> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && !(*i > 0 && args[i - 1] == "--wad"))
        .map(|(_, a)| {
            hash_or_name(a).unwrap_or_else(|e| {
                eprintln!("kit_templates: {e}");
                std::process::exit(2);
            })
        })
        .collect();
    if targets.is_empty() && !all_kits && !factions {
        eprintln!("usage: kit_templates <model name|0xHASH>... | --all-kits | --factions  [--components] [--profiles] [--wad <path>]");
        std::process::exit(2);
    }

    let wadpath = wad::resolve_vz_wad(wad_arg.as_deref()).unwrap_or_else(|| {
        eprintln!("kit_templates: no vz.wad found (pass --wad, or write .mercs2-local.toml)");
        std::process::exit(2);
    });
    let mut wads = vec![wad::open(&wadpath).unwrap_or_else(|e| fail(format!("cannot open {wadpath}: {e}")))];

    // `--all-kits`: every ASET model whose own container (the block its row names — finer LOD rungs
    // are streamed GEOM blocks with other framing) carries a MIXR.
    if all_kits {
        let mut models: Vec<u32> = wad::model_list_all(&wads[0]).into_iter().map(|(h, _)| h).collect();
        models.sort_unstable();
        models.dedup();
        for &h in &models {
            let c = wad::extract_container(&mut wads[0], h).unwrap_or_else(|e| fail(format!("model 0x{h:08X}: {e}")));
            if Mixr::find(&c).unwrap_or_else(|e| fail(format!("model 0x{h:08X} MIXR: {e}"))).is_some() {
                targets.push(h);
            }
        }
        eprintln!("[kit_templates] {} of {} model(s) carry a MIXR", targets.len(), models.len());
    }

    // The worldentity chunk: the singleton of its type, made resident the way the game does.
    let mut registry = AssetRegistry::default();
    let (_, we_ref) = registry
        .resolve_singleton(&mut wads, WORLDENTITY_TYPE_HASH)
        .unwrap_or_else(|| fail("no worldentity chunk in the wad".into()));
    let we_block = we_ref.block;
    let bytes = registry.slice(we_ref).unwrap_or_else(|| fail("worldentity block evicted before read".into()));
    let we = WorldEntity::parse(bytes).unwrap_or_else(|e| fail(format!("worldentity: {e}")));
    let templates = kit_templates(&we).unwrap_or_else(|e| fail(format!("templates: {e}")));
    eprintln!(
        "[kit_templates] worldentity in block {we_block}: {} components, {} templates with a ModelName",
        we.components.len(),
        templates.len()
    );
    if factions {
        for t in &templates {
            let f = t.faction.map(|f| format!("0x{f:08X}")).unwrap_or_else(|| "-".into());
            println!("0x{:08X}\t{f}\t{}", t.model, t.label());
        }
        return;
    }
    let mut of_model: BTreeMap<u32, Vec<&KitTemplate>> = BTreeMap::new();
    for t in &templates {
        of_model.entry(t.model).or_default().push(t);
    }

    let (mut templates_seen, mut templates_unmatched) = (0usize, 0usize);
    for m in &targets {
        let ts = of_model.get(m).cloned().unwrap_or_default();
        println!("\n=== model 0x{m:08X}: {} template(s)", ts.len());
        for t in &ts {
            println!("  entity 0x{:08X}  name {:?}", t.key, t.name);
            if show_components {
                for c in &we.components {
                    for r in c.records.iter().filter(|r| r.keys.contains(&t.key)) {
                        println!("    {:28} {:?}", c.class, r.payload);
                    }
                }
            }
        }
        if !profiles {
            continue;
        }
        let c = wad::extract_container(&mut wads[0], *m).unwrap_or_else(|e| fail(format!("model 0x{m:08X}: {e}")));
        let Some(mx) = Mixr::find(&c).unwrap_or_else(|e| fail(format!("model 0x{m:08X} MIXR: {e}"))) else {
            println!("  (no MIXR)");
            continue;
        };
        println!("  MIXR: {} parts, {} profiles", mx.parts.len(), mx.profiles.len());
        for (i, h) in mx.parts.iter().enumerate() {
            let excl: Vec<usize> = (0..mx.parts.len()).filter(|&j| j != i && !mx.compatible[i][j]).collect();
            println!("    part {i:2} node 0x{h:08X}  exclusive with {excl:?}");
        }
        for (pi, p) in mx.profiles.iter().enumerate() {
            let allowed: Vec<usize> = (0..p.allowed.len()).filter(|&i| p.allowed[i]).collect();
            let users: Vec<String> = ts.iter().filter(|t| t.profile == p.hash).map(|t| t.label()).collect();
            let outfits = mx.outfits(pi);
            println!(
                "    profile {pi} 0x{:08X}: allows {allowed:?}; {} outfit(s); templates {users:?}",
                p.hash,
                outfits.len()
            );
            for (k, o) in outfits.iter().enumerate() {
                println!("      {}", mx.outfit_line(k + 1, *o));
            }
        }
        for t in &ts {
            templates_seen += 1;
            if let Err(e) = mx.template_outfits(t) {
                templates_unmatched += 1;
                println!("    entity 0x{:08X}: {e}", t.key);
            }
        }
    }
    if all_kits {
        println!(
            "\n{} kit model(s); {templates_seen} template(s), {templates_unmatched} without a profile of their kit",
            targets.len()
        );
    }
}
