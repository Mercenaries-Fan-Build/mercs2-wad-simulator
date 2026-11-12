//! Probe: print templates as qm's template form (JSON), from the game's worldentity or a level's.
//!
//!   cargo run -p mercs2_probe --bin template_form -- --game <vz.wad> [--level <worldentity ucfx>] NAME...
//!
//! Each NAME is a template name, its key found by the name's hash, or a key `0xHHHHHHHH` the container
//! has. With `--level`, the templates are
//! read from that container (a console level's compact one is read by the class layouts of `--game`),
//! else from `vz.wad`'s resident worldentity. A name the container has no template of, or more than
//! one, is an error. Exit 0 on success, 2 on any error.

use std::path::PathBuf;
use std::process::ExitCode;

use mercs2_formats::be_to_le::ecs::ComponentSchemas;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::worldentity::{Payload, WorldEntity, TEMPLATE_KEY_BIT};
use mercs2_quartermaster::template::TemplateForm;
use mercs2_quartermaster::GameStack;

fn run() -> Result<(), String> {
    let mut it = std::env::args().skip(1);
    let (mut game, mut level, mut names) = (None, None, Vec::new());
    while let Some(a) = it.next() {
        match a.as_str() {
            "--game" => game = Some(PathBuf::from(it.next().ok_or("--game needs a value")?)),
            "--level" => level = Some(PathBuf::from(it.next().ok_or("--level needs a value")?)),
            _ if a.starts_with("--") => return Err(format!("unknown option {a}")),
            _ => names.push(a),
        }
    }
    let game = game.ok_or("--game is required")?;
    if names.is_empty() {
        return Err("name at least one template".into());
    }
    let mut we = match &level {
        Some(path) => {
            let schemas = ComponentSchemas::from_vz_wad(&game)?;
            let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            WorldEntity::parse_compact(&raw, &schemas).map_err(|e| format!("{}: {e}", path.display()))?
        }
        None => {
            let mut stack = GameStack::open(&[game.clone()]).map_err(|e| format!("{}: {e:?}", game.display()))?;
            mercs2_quartermaster::fx::GameFx::read(&mut stack)?.worldentity
        }
    };
    for name in &names {
        let h = pandemic_hash_m2(name);
        let as_key = name.strip_prefix("0x").and_then(|k| u32::from_str_radix(k, 16).ok());
        let keys: Vec<u32> = match as_key {
            Some(k) if we.instances.contains(&k) => vec![k],
            _ => we
                .name_hashes()?
                .into_iter()
                .filter(|(n, _)| *n == h)
                .flat_map(|(_, k)| k.iter().copied().filter(|k| k & TEMPLATE_KEY_BIT != 0).collect::<Vec<_>>())
                .collect(),
        };
        let [key] = keys.as_slice() else {
            return Err(format!("{name:?} (0x{h:08X}): the container has {} templates of that name", keys.len()));
        };
        // A compact container names templates by hash; the form takes the name.
        for comp in we.components.iter_mut().filter(|c| c.class == "Name") {
            for r in comp.records.iter_mut().filter(|r| r.keys.contains(key)) {
                if matches!(r.payload, Payload::NameHash(_)) {
                    r.payload = Payload::Name { name: name.clone(), flag: 0 };
                }
            }
        }
        let form = TemplateForm::express(&we, *key)?;
        let json = serde_json::to_string_pretty(&form).map_err(|e| e.to_string())?;
        println!("// {name} — key 0x{key:08X}\n{json}");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("template_form: {e}");
            ExitCode::from(2)
        }
    }
}
