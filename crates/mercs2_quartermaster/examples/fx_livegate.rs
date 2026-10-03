//! fx_livegate — builds the Shipments for the FX live test on Windows.
//!
//! Reads the game only through the repo-root `.mercs2-local.toml` (`vz_wad`). Writes one Shipment
//! directory per archive under OUT, each a `raw` contribution Modkit installs (plus `add_texture`
//! for the FX archives), builds each with qm's own `build::build`, and checks the result:
//!
//! | Archive | Payload |
//! |---|---|
//! | `g0` | a one-entry block: the retail worldentity `0x50075B3B`, re-written by the codec (identical bytes) |
//! | `g1` | the same, with template `qm_gate_c4` appended under the key its name derives |
//! | `g2` | the same template under the highest `0x8` key + 1 |
//! | `fx_a` | the whole effects block, `global_explosion_c4` recoloured magenta, frames on the disc texture |
//! | `fx_b` | the same magenta `global_explosion_c4` as a one-entry block |
//!
//! `qm_gate_c4` is declared through the template author form ([`QM_GATE_C4`]); every value is
//! typed by hand. Each archive gets `lint::artifact_checks` on its built blocks, a decode-back of
//! its payload out of the built overlay, and a sha256 of every file.
//!
//! Usage: `cargo run -p mercs2_quartermaster --example fx_livegate -- <OUT>`

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::{load_ffcs_archive, FfcsArchive};
use mercs2_formats::fxdict::{parse_effect_container, write_effect_container};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::patch_wad::read_patch_wad;
use mercs2_formats::sges::{decompress_block, decompress_sges};
use mercs2_formats::types::TYPE_HASH_EFFECT;
use mercs2_formats::ucfx::parse_block_entry_table;
use mercs2_formats::worldentity::{
    derived_template_key, Payload, WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH,
};
use mercs2_quartermaster::lint::{self, Severity};
use mercs2_quartermaster::template::{self, TemplateForm};
use mercs2_quartermaster::{build, discover, Format, GameStack};
use sha2::{Digest, Sha256};

/// The test template, declared field by field. Its values are the ones the retail C4 template
/// `global_particle_explosion_c4` (`0x80008028`) holds, typed by hand, so that a spawn of it should
/// look like a C4 explosion.
const QM_GATE_C4: &str = r#"
name: qm_gate_c4
name_flag: 1
components:
  EffectTemplate:
    "0xB9D95A23": "0x00000000"
  HibernationControl:
    "0xCBE8ED58": 500
    "0x74E63261": 160
    "0xDEA888CE": 60
    "0x2332033F": 20
    "0x3CE51772": true
    "0x3F1DA641": false
  RedEffectComponent:
    name: global_explosion_c4
    "0x4D7D459B": 1.0
    "0x62C7746E": 0.0
    "0xE8DABAE6": 1.0
    "0x216E8465": 10.0
    "0x3902F594": 0.0
    "0xE351CA81": "0x00000001"
    "0xB9BA2DFE": 0.0
    "0xF88C32BA": "0x00000001"
    "0x95323A93": "0x00000000"
    "0xE8764CF8": "0x00000000"
    "0xA87B6266": -2.0
    "0x4E4CCD85": 10.0
    "0x87519019": 30
  SoundEffect:
    "0x2EB62242": "0x56B83982"
    "0xC43322C3": 0
    "0x29F15442": 1
    "0x4E97DE03": 1.0
    "0x14A67FA6": "0x00000000"
    "0xD932985B": 0
    "0x11957817": 50.0
"#;

const C4_TEMPLATE_KEY: u32 = 0x8000_8028;
const C4_EFFECT: &str = "global_explosion_c4";
const DISC_TEXTURE: &str = "qm_gate_magenta_disc";

type Res<T> = Result<T, String>;

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The decompressed block holding `asset`, by its ASET row, and that block's index and path.
fn block_of(f: &mut File, archive: &FfcsArchive, asset: u32) -> Res<(u16, String, Vec<u8>)> {
    let rows: Vec<_> = archive.aset.iter().filter(|a| a.asset_hash == asset).collect();
    let [row] = rows.as_slice() else {
        return Err(format!("{} ASET rows name 0x{asset:08X}; expected one", rows.len()));
    };
    let bi = row.block_index();
    let dec = decompress_block(f, &archive.indx, bi).map_err(|e| format!("block {bi}: {e}"))?;
    Ok((bi, archive.paths[bi as usize].clone(), dec))
}

/// One block entry: `(name_hash, type_hash, field_c, container)`.
type Entry = (u32, u32, u32, Vec<u8>);

fn entries(block: &[u8]) -> Res<Vec<Entry>> {
    let (n, rows) = parse_block_entry_table(block);
    if rows.len() != n as usize {
        return Err(format!("block declares {n} entries, {} rows fit", rows.len()));
    }
    let mut pos = 4 + 16 * rows.len();
    let mut out = Vec::new();
    for r in &rows {
        let end = pos + r.chunk_size as usize;
        if end > block.len() {
            return Err(format!("entry 0x{:08X} overruns the block", r.name_hash));
        }
        out.push((r.name_hash, r.type_hash, r.field_c, block[pos..end].to_vec()));
        pos = end;
    }
    if pos != block.len() {
        return Err(format!("{} bytes follow the last entry", block.len() - pos));
    }
    Ok(out)
}

fn block_from(entries: &[Entry]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (n, t, c, body) in entries {
        for w in [*n, *t, *c, body.len() as u32] {
            b.extend_from_slice(&w.to_le_bytes());
        }
    }
    for (_, _, _, body) in entries {
        b.extend_from_slice(body);
    }
    b
}

/// A 64×64 RGBA disc: magenta, opaque inside radius 24, fading to transparent at 31.
fn magenta_disc_png() -> Res<Vec<u8>> {
    let mut px = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let d = ((x as f32 - 31.5).powi(2) + (y as f32 - 31.5).powi(2)).sqrt();
            let a = if d <= 24.0 {
                255.0
            } else if d >= 31.0 {
                0.0
            } else {
                255.0 * (31.0 - d) / 7.0
            };
            px.extend_from_slice(&[255, 0, 255, a as u8]);
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 64, 64);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&px).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Recolour every emitter of an effect magenta (colour bytes 0..2 = FF 00 FF, byte 3 kept) and
/// point every TEXT frame at the disc texture.
fn magenta(effect: &[u8], disc: u32) -> Res<Vec<u8>> {
    let mut fx = parse_effect_container(effect)?;
    for e in &mut fx.emitters {
        for k in e.particle.colr.keys.iter_mut() {
            k.colour = [0xFF, 0x00, 0xFF, k.colour[3]];
        }
        for f in e.particle.text.frames.iter_mut() {
            *f = disc;
        }
    }
    write_effect_container(&fx)
}

fn check_magenta(effect: &[u8], disc: u32, original: &[u8]) -> Res<()> {
    let fx = parse_effect_container(effect)?;
    let base = parse_effect_container(original)?;
    if fx.emitters.len() != base.emitters.len() || fx.forces != base.forces || fx.shapes != base.shapes {
        return Err("the effect's emitters, forces or shapes changed".into());
    }
    for (e, b) in fx.emitters.iter().zip(&base.emitters) {
        for (k, kb) in e.particle.colr.keys.iter().zip(b.particle.colr.keys.iter()) {
            if k.colour != [0xFF, 0x00, 0xFF, kb.colour[3]] || k.half_bits != kb.half_bits {
                return Err(format!("a COLR key reads {:?}", k.colour));
            }
        }
        if e.particle.text.frames.len() != b.particle.text.frames.len()
            || e.particle.text.frames.iter().any(|&f| f != disc)
        {
            return Err("TEXT frames are not all the disc".into());
        }
        if e.particle.attributes != b.particle.attributes
            || e.particle.flags != b.particle.flags
            || e.channels != b.channels
            || e.transform != b.transform
            || e.geom != b.geom
        {
            return Err("something other than COLR and TEXT changed".into());
        }
    }
    Ok(())
}

struct Archive {
    id: &'static str,
    title: String,
    payload: Vec<u8>,
    touches: Vec<u32>,
    texture: bool,
}

fn manifest(a: &Archive) -> String {
    let touches: Vec<String> = a.touches.iter().map(|h| format!("\"0x{h:08X}\"")).collect();
    let mut m = format!(
        "format: 2\n\nshipment:\n  name: qm-fx-livegate-{id}\n  title: \"FX live test {id}\"\n  \
         version: 0.0.1\n  target: retail\n  description: >-\n    {title}\n\ncontributions:\n",
        id = a.id.replace('_', "-"),
        title = a.title
    );
    if a.texture {
        m.push_str(&format!(
            "  - kind: add_texture\n    name: {DISC_TEXTURE}\n    image: src/{DISC_TEXTURE}.png\n"
        ));
    }
    m.push_str(&format!(
        "  - kind: raw\n    description: \"{}\"\n    payload: src/payload.block\n    target_layer: data\n    \
         touches: [{}]\n",
        a.title,
        touches.join(", ")
    ));
    m
}

/// What each archive's payload must decode back to.
struct Expect<'a> {
    base: &'a WorldEntity,
    form: &'a TemplateForm,
    g1: u32,
    g2: u32,
    c4: u32,
    disc: u32,
    original: &'a [u8],
}

impl Expect<'_> {
    fn check(&self, id: &str, block: &[u8]) -> Res<()> {
        let es = entries(block)?;
        match id {
            "g0" | "g1" | "g2" => {
                let [(n, t, _, c)] = es.as_slice() else {
                    return Err(format!("{id}: {} entries", es.len()));
                };
                if (*n, *t) != (RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH) {
                    return Err(format!("{id}: the entry is 0x{n:08X}/0x{t:08X}"));
                }
                match id {
                    "g0" if *c != self.base.write()? => {
                        Err("g0 is not the retail worldentity".into())
                    }
                    "g0" => Ok(()),
                    "g1" => check_template(c, self.form, self.g1),
                    _ => check_template(c, self.form, self.g2),
                }
            }
            "fx_b" => {
                let [(n, _, _, c)] = es.as_slice() else {
                    return Err(format!("{id}: {} entries", es.len()));
                };
                if *n != self.c4 {
                    return Err(format!("{id}: the entry is 0x{n:08X}"));
                }
                check_magenta(c, self.disc, self.original)
            }
            "fx_a" => {
                let c = &es.iter().find(|e| e.0 == self.c4).ok_or("fx_a: no C4 effect")?.3;
                check_magenta(c, self.disc, self.original)
            }
            _ => Err(format!("unknown archive {id}")),
        }
    }
}

/// The appended template reads back: its name resolves to `key`, its records equal the retail C4
/// template's class by class, and its flgs bits equal the C4 template's.
fn check_template(bytes: &[u8], form: &TemplateForm, key: u32) -> Res<()> {
    let we = WorldEntity::parse(bytes)?;
    let names = we.names()?;
    let keys: Vec<&[u32]> = names.iter().filter(|(n, _)| *n == form.name).map(|(_, k)| *k).collect();
    if keys != [[key].as_slice()] {
        return Err(format!("{} resolves to {keys:?}, not 0x{key:08X}", form.name));
    }
    if !we.instances.contains(&key) {
        return Err("the key is not in UNIQ".into());
    }
    let records = |k: u32| -> Vec<(String, Payload)> {
        we.components
            .iter()
            .filter(|c| c.class != "Name")
            .flat_map(|c| {
                c.records
                    .iter()
                    .filter(move |r| r.keys.contains(&k))
                    .map(move |r| (c.class.clone(), r.payload.clone()))
            })
            .collect()
    };
    let (mine, c4) = (records(key), records(C4_TEMPLATE_KEY));
    if mine != c4 {
        let diff: Vec<String> = mine
            .iter()
            .zip(&c4)
            .filter(|(a, b)| a != b)
            .map(|((class, a), (_, b))| {
                let comp = &we.components[we.append_group(class).unwrap_or(0)];
                format!("{class}: {:?} vs C4 {:?}", comp.decode(a), comp.decode(b))
            })
            .collect();
        return Err(format!(
            "qm_gate_c4's records differ from the C4 template's ({} vs {} records): {}",
            mine.len(),
            c4.len(),
            diff.join("; ")
        ));
    }
    let bits = |k: u32| we.flags.iter().find(|f| f.key == k).map(|f| f.bits.clone());
    if bits(key).is_none() || bits(key) != bits(C4_TEMPLATE_KEY) {
        return Err("qm_gate_c4's flgs bits differ from the C4 template's".into());
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("fx_livegate: {e}");
        std::process::exit(1);
    }
}

fn run() -> Res<()> {
    let out: PathBuf = std::env::args_os().nth(1).map(PathBuf::from).ok_or("usage: fx_livegate <OUT>")?;
    let vz = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let mut f = File::open(&vz).map_err(|e| format!("open {}: {e}", vz.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let archive = load_ffcs_archive(&mut f, size).map_err(|e| e.to_string())?;

    // ---- the worldentity ---------------------------------------------------------------------
    let (rbi, rpath, resident) = block_of(&mut f, &archive, RETAIL_WORLDENTITY_NAME_HASH)?;
    let rentries = entries(&resident)?;
    let we_row = rentries
        .iter()
        .find(|e| e.0 == RETAIL_WORLDENTITY_NAME_HASH && e.1 == WORLDENTITY_TYPE_HASH)
        .ok_or("no worldentity entry in its block")?;
    let we_bytes = we_row.3.clone();
    println!(
        "worldentity 0x{RETAIL_WORLDENTITY_NAME_HASH:08X}: block {rbi} {rpath}, {} bytes, sha256 {}",
        we_bytes.len(),
        sha256(&we_bytes)
    );
    let base = WorldEntity::parse(&we_bytes)?;
    if base.write()? != we_bytes {
        return Err("the codec does not re-write the retail worldentity byte for byte".into());
    }

    let form = template::from_str(QM_GATE_C4, Format::Yaml)?;
    let g1_key = derived_template_key(&form.name);
    let g2_key = base.all_keys().into_iter().filter(|k| k >> 28 == 8).max().ok_or("no 0x8 key")? + 1;
    println!(
        "qm_gate_c4: name hash 0x{:08X}; g1 key 0x{g1_key:08X} (derived); g2 key 0x{g2_key:08X} \
         (highest 0x8 key + 1)",
        pandemic_hash_m2(&form.name)
    );
    let with_template = |key: u32| -> Res<Vec<u8>> {
        let mut we = base.clone();
        let decl = form.lower(&we, key)?;
        we.append_template(&decl)?;
        let bytes = we.write()?;
        check_template(&bytes, &form, key)?;
        Ok(bytes)
    };
    let one = |container: Vec<u8>| {
        block_from(&[(RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH, we_row.2, container)])
    };

    // ---- the effect --------------------------------------------------------------------------
    let c4 = pandemic_hash_m2(C4_EFFECT);
    let (ebi, epath, effects) = block_of(&mut f, &archive, c4)?;
    let mut eentries = entries(&effects)?;
    let disc = pandemic_hash_m2(DISC_TEXTURE);
    let at = eentries
        .iter()
        .position(|e| e.0 == c4 && e.1 == TYPE_HASH_EFFECT)
        .ok_or("no global_explosion_c4 effect in its block")?;
    let original = eentries[at].3.clone();
    let recoloured = magenta(&original, disc)?;
    check_magenta(&recoloured, disc, &original)?;
    println!(
        "effects: block {ebi} {epath}, {} entries; {C4_EFFECT} 0x{c4:08X} {} -> {} bytes; disc \
         texture {DISC_TEXTURE} 0x{disc:08X}",
        eentries.len(),
        original.len(),
        recoloured.len()
    );
    let fx_b = block_from(&[(c4, TYPE_HASH_EFFECT, eentries[at].2, recoloured.clone())]);
    eentries[at].3 = recoloured;
    let fx_touches: Vec<u32> = eentries.iter().map(|e| e.0).collect();
    let fx_a = block_from(&eentries);

    let archives = vec![
        Archive {
            id: "g0",
            title: "the retail worldentity 0x50075B3B, re-written unchanged, as a one-entry block".into(),
            payload: one(base.write()?),
            touches: vec![RETAIL_WORLDENTITY_NAME_HASH],
            texture: false,
        },
        Archive {
            id: "g1",
            title: format!(
                "the retail worldentity with template qm_gate_c4 appended under the derived key 0x{g1_key:08X}"
            ),
            payload: one(with_template(g1_key)?),
            touches: vec![RETAIL_WORLDENTITY_NAME_HASH],
            texture: false,
        },
        Archive {
            id: "g2",
            title: format!(
                "the retail worldentity with template qm_gate_c4 appended under the key 0x{g2_key:08X}, the highest 0x8 key + 1"
            ),
            payload: one(with_template(g2_key)?),
            touches: vec![RETAIL_WORLDENTITY_NAME_HASH],
            texture: false,
        },
        Archive {
            id: "fx_a",
            title: "the whole effects block with global_explosion_c4 magenta".into(),
            payload: fx_a,
            touches: fx_touches,
            texture: true,
        },
        Archive {
            id: "fx_b",
            title: "global_explosion_c4 magenta as a one-entry block".into(),
            payload: fx_b,
            touches: vec![c4],
            texture: true,
        },
    ];
    let expect = Expect { base: &base, form: &form, g1: g1_key, g2: g2_key, c4, disc, original: &original };

    let mut game = GameStack::open(std::slice::from_ref(&vz)).map_err(|e| e.to_string())?;
    let disc_png = magenta_disc_png()?;
    let mut report = String::new();
    for a in &archives {
        let dir = out.join(a.id);
        if dir.exists() {
            return Err(format!("{} exists; remove it first", dir.display()));
        }
        std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("manifest.yaml"), manifest(a)).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("src/payload.block"), &a.payload).map_err(|e| e.to_string())?;
        if a.texture {
            std::fs::write(dir.join(format!("src/{DISC_TEXTURE}.png")), &disc_png)
                .map_err(|e| e.to_string())?;
        }
        let s = discover::open(&dir).map_err(|e| format!("{}: {e:?}", a.id))?;
        let built = dir.join("build");
        let rep = build::build(&s, Some(&mut game), None, Some(&built), None, None)
            .map_err(|e| format!("{}: build: {e:?}", a.id))?;
        for d in &rep.diagnostics {
            println!("  {} lint {:?} {}: {}", a.id, d.severity, d.rule.code, d.message);
        }
        let wad_path = rep.wad.ok_or_else(|| format!("{}: the build produced no overlay", a.id))?;
        let wad = std::fs::read(&wad_path).map_err(|e| e.to_string())?;
        let contents = read_patch_wad(&wad)?;
        let findings = lint::artifact_checks(&contents.blocks);
        for d in &findings {
            println!("  {} artifact {:?} {}: {}", a.id, d.severity, d.rule.code, d.message);
        }
        if findings.iter().any(|d| d.severity >= Severity::Error) {
            return Err(format!("{}: artifact_checks found an error", a.id));
        }
        // Decode back: the raw block out of the built overlay is the payload, byte for byte, its
        // rows are by-hash rows for exactly the touched hashes, and its payload reads back typed.
        let raw = contents
            .blocks
            .iter()
            .find(|b| b.aset_entries.iter().any(|e| e.asset_hash == a.touches[0]))
            .ok_or_else(|| format!("{}: no built block carries 0x{:08X}", a.id, a.touches[0]))?;
        let dec = decompress_sges(&raw.compressed_data)?;
        if dec != a.payload {
            return Err(format!("{}: the built block is not the payload", a.id));
        }
        let mut rows: Vec<u32> = raw.aset_entries.iter().map(|e| e.asset_hash).collect();
        rows.sort_unstable();
        let mut want = a.touches.clone();
        want.sort_unstable();
        if rows != want || raw.aset_entries.iter().any(|e| e.u32_2 & 0xFFFF != 0xFFFF) {
            return Err(format!("{}: the built rows are not by-hash rows for the touched hashes", a.id));
        }
        expect.check(a.id, &dec)?;
        let mut files: BTreeMap<String, String> = BTreeMap::new();
        for rel in ["manifest.yaml", "src/payload.block"] {
            let bytes = std::fs::read(dir.join(rel)).map_err(|e| e.to_string())?;
            files.insert(rel.into(), sha256(&bytes));
        }
        if a.texture {
            files.insert(format!("src/{DISC_TEXTURE}.png"), sha256(&disc_png));
        }
        let mut type_ids: Vec<u32> = raw.aset_entries.iter().map(|e| e.u32_3).collect();
        type_ids.sort_unstable();
        type_ids.dedup();
        let line = format!(
            "{}: {}\n  payload {} bytes; built block {} ({} by-hash rows, type ids {:?}); overlay {} sha256 {}\n{}",
            a.id,
            dir.display(),
            a.payload.len(),
            raw.path_string,
            raw.aset_entries.len(),
            type_ids,
            wad_path.display(),
            sha256(&wad),
            files.iter().map(|(k, v)| format!("  {k} sha256 {v}\n")).collect::<String>()
        );
        print!("{line}");
        report.push_str(&line);
    }
    std::fs::write(out.join("fx_livegate_report.txt"), report).map_err(|e| e.to_string())?;
    Ok(())
}
