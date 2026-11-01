//! `ecs-survey` — every ECS container of a big-endian archive, as stored.
//!
//! Writes a row per `worldentity`, layer and guidmap container: its CHDR flags, how many of its
//! `COMP` groups keep a `schm`, and its row tags. `--raw <dir>` also writes each `worldentity`
//! container unconverted.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use mercs2_formats::types::{TYPE_HASH_GUIDMAP, TYPE_HASH_LAYER, TYPE_HASH_WORLD_ENTITY_DATA};

fn be32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn type_label(t: u32) -> Option<&'static str> {
    match t {
        TYPE_HASH_WORLD_ENTITY_DATA => Some("worldentity"),
        TYPE_HASH_LAYER => Some("layer"),
        TYPE_HASH_GUIDMAP => Some("guidmap"),
        _ => None,
    }
}

#[derive(Default)]
struct Shape {
    chdr: Option<u32>,
    comps: usize,
    compact: usize,
    schm: usize,
    tags: BTreeSet<String>,
}

/// Read one big-endian UCFX container's rows. A `COMP` group's `info` is compact at 16 bytes with
/// no `schm` beside it; the bytes are not otherwise interpreted.
fn shape(c: &[u8]) -> Result<Shape, String> {
    if c.len() < 20 || &c[..4] != b"XFCU" {
        return Err(format!("not a big-endian UCFX ({} bytes)", c.len()));
    }
    let dao = be32(c, 4) as usize;
    let n = be32(c, 16) as usize;
    let mut s = Shape::default();
    let rows: Vec<([u8; 4], u32, u32)> = (0..n)
        .map(|i| {
            let o = 20 + i * 20;
            ([c[o + 3], c[o + 2], c[o + 1], c[o]], be32(c, o + 4), be32(c, o + 8))
        })
        .collect();
    for (i, (tag, off, _)) in rows.iter().enumerate() {
        s.tags.insert(String::from_utf8_lossy(tag).into_owned());
        match tag {
            b"CHDR" => s.chdr = Some(be32(c, dao + *off as usize + 4)),
            b"schm" => s.schm += 1,
            b"COMP" => {
                s.comps += 1;
                if rows.get(i + 2).is_some_and(|(t, _, _)| t != b"schm") {
                    s.compact += 1;
                }
            }
            _ => {}
        }
    }
    Ok(s)
}

pub fn ecs_survey(path: &Path, out: &Path, raw: Option<&Path>) -> Result<(), String> {
    let (doh, src) = load_stfs_or_doh(path)?;
    let (_, rows) = parse_be_ffcs(&doh)?;
    let chunk = |t: &str| rows.iter().find(|r| r.tag == t).ok_or(format!("{t}: no such chunk"));
    let indx_row = chunk("INDX")?;
    let indx = parse_be_indx(&doh, indx_row.offset as usize, indx_row.meta as usize);
    let pths = chunk("PTHS").map(|r| parse_be_pths(&doh, r.offset as usize, r.meta as usize))?;
    println!("{src}: {} blocks", indx.len());
    let mut tsv = String::from("block\tpath\tname\ttype\tchdr_flags\tcomps\tcompact\tschm\ttags\n");
    for (bi, e) in indx.iter().enumerate() {
        let path = pths.get(bi).map_or("", String::as_str);
        let b = read_be_block(&doh, e).map_err(|err| format!("[{bi}] {path}: {err}"))?;
        let n = be32(&b, 0) as usize;
        let mut off = 4 + n * 16;
        for i in 0..n {
            let row = 4 + i * 16;
            let (name, ty, size) = (be32(&b, row), be32(&b, row + 4), be32(&b, row + 12) as usize);
            let c = b.get(off..off + size).ok_or_else(|| format!("[{bi}] {path}: entry {i} runs past the block"))?;
            off += size;
            let Some(label) = type_label(ty) else { continue };
            let s = shape(c).map_err(|err| format!("[{bi}] {path}: {err}"))?;
            let chdr = s.chdr.map_or("-".into(), |f| format!("{f}"));
            let tags = s.tags.iter().map(String::as_str).collect::<Vec<_>>().join(",");
            writeln!(tsv, "{bi}\t{path}\t0x{name:08X}\t{label}\t{chdr}\t{}\t{}\t{}\t{tags}", s.comps, s.compact, s.schm)
                .expect("a String takes the row");
            if let (Some(dir), TYPE_HASH_WORLD_ENTITY_DATA) = (raw, ty) {
                let p = dir.join(format!("worldentity_{bi}_{name:08X}.bin"));
                std::fs::write(&p, c).map_err(|err| format!("write {}: {err}", p.display()))?;
            }
        }
    }
    std::fs::write(out, tsv).map_err(|err| format!("write {}: {err}", out.display()))
}
