//! `dump-render` — what a v1.0 capture's last frame rendered with: the live camera, the frame time, the
//! atmosphere's wind, the `WindMatrix` the ground cover draws read, and the tree species batches.
//!
//! The wind block is found from the draw itself: `FUN_004a5830` writes a constant-set record per
//! ground cover mesh into the command buffer `DAT_00ff46c0`, `{2, handle, pointer, count}` for
//! `AlphaVec`, `WindMatrix` and `WindParams`, so the one set of 4 between two sets of 1 names it. The
//! batches are `DAT_01175a30` (stride `0x710`, count `DAT_00e90734`).

use crate::dump::{Capture, Mem};

const VIEW: u32 = 0x00DF_C2F8;
const VIEW_STRIDE: u32 = 0xE80;
const FRAME_TIME: u32 = 0x0117_6048;
const ATMOSPHERE: u32 = 0x00E7_ADFC;
const COMMANDS: u32 = 0x00FF_46C0;
const COMMANDS_BYTES: usize = 0x5_0000;
const BATCHES: u32 = 0x0117_5A30;
const BATCH_COUNT: u32 = 0x00E9_0734;
const BATCH_STRIDE: u32 = 0x710;
/// One row of a batch's collection table (`FUN_004a32f0`'s swap-remove).
const INSTANCE_STRIDE: u32 = 0x1C;

fn f32_at(mem: &Mem, va: u32) -> Result<f32, String> {
    Ok(f32::from_bits(mem.u32(va)?))
}

fn floats<const N: usize>(mem: &Mem, va: u32) -> Result<[f32; N], String> {
    let mut out = [0.0; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = f32_at(mem, va + 4 * i as u32)?;
    }
    Ok(out)
}

/// Every `WindMatrix` pointer the command buffer holds, most frequent first.
fn wind_pointers(mem: &Mem) -> Result<Vec<(u32, usize)>, String> {
    let words: Vec<u32> = mem
        .read(mem.u32(COMMANDS)?, COMMANDS_BYTES)?
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let mut found: std::collections::BTreeMap<u32, usize> = Default::default();
    for w in words.windows(12) {
        if w[0] == 2 && w[3] == 1 && w[4] == 2 && w[7] == 4 && w[8] == 2 && w[11] == 1 {
            *found.entry(w[6]).or_default() += 1;
        }
    }
    let mut out: Vec<(u32, usize)> = found.into_iter().collect();
    out.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    Ok(out)
}

fn print_camera(mem: &Mem) -> Result<(), String> {
    let base = mem.u32(VIEW)?;
    println!("view index {}", mem.u16(base + 0x2B92)?);
    for slot in 0..4 {
        let cam = base + slot * VIEW_STRIDE;
        println!(
            "viewport {slot}: enabled {} eye {:?} forward {:?} proj {:?}",
            mem.u8(cam + 0x3C)?,
            floats::<3>(mem, cam + 0xB20)?,
            floats::<3>(mem, cam + 0xB10)?,
            floats::<4>(mem, cam + 0xB30 + 0x20)?,
        );
    }
    Ok(())
}

fn print_wind(mem: &Mem) -> Result<(), String> {
    println!("frame time {}", f32_at(mem, FRAME_TIME)?);
    let atmo = mem.chase(ATMOSPHERE, &[0x104, 0])?;
    println!(
        "atmosphere 0x{atmo:08X}: time {} speed {} intensity {} modifier {} wind direction {:?} wind speed {}",
        f32_at(mem, atmo + 0x10)?,
        f32_at(mem, atmo + 0x14)?,
        f32_at(mem, atmo + 0xE0)?,
        f32_at(mem, atmo + 0x124)?,
        floats::<2>(mem, atmo + 0x17C)?,
        f32_at(mem, atmo + 0x184)?,
    );
    let pointers = wind_pointers(mem)?;
    if pointers.is_empty() {
        println!("no ground cover draw in the command buffer");
    }
    for (p, n) in pointers.iter().take(3) {
        let (head, m) = (p - 0x20, floats::<16>(mem, *p)?);
        println!("WindMatrix 0x{p:08X} ({n} draws): block head {:?}", floats::<4>(mem, head)?);
        println!("  row0 {:?}\n  row3 {:?}", &m[..4], &m[12..]);
    }
    Ok(())
}

fn print_batches(mem: &Mem) -> Result<(), String> {
    let (base, count) = (mem.u32(BATCHES)?, mem.u32(BATCH_COUNT)?);
    println!("{count} tree batches at 0x{base:08X}");
    let mut total = 0usize;
    for i in 0..count {
        let b = base + i * BATCH_STRIDE;
        let (table, rows) = (mem.u32(b + 0x708)?, mem.u16(b + 0x70C)? as u32);
        total += rows as usize;
        println!(
            "batch {i:2}: template 0x{:08X} instances {:5} range {} rows {rows}",
            mem.u32(b + 0x6D0)?,
            mem.u32(b)?,
            f32_at(mem, b + 0x6D4)?,
        );
        for r in 0..rows.min(4) {
            let row = table + r * INSTANCE_STRIDE;
            println!("  id 0x{:08X} at {:?}", mem.u32(row + 0x10)?, floats::<3>(mem, row)?);
        }
    }
    println!("{total} rows");
    Ok(())
}

/// `dump-model`: the runtime model `M` of `hash` (`M+0x08` the model type hash) with its LOD fields, and
/// every render object `OBJ` drawing it (`OBJ+0x1e0` = `M`): flags, rung and `view_state`.
pub fn dump_model(capture: &str, hash: u32) -> Result<(), String> {
    const MODEL_TYPE: u32 = 0x5B72_4250;
    let cap = Capture::open(std::path::Path::new(capture))?;
    let mem = cap.mem()?;
    for (at, _) in mem.find_u32(&[hash]) {
        for m in [at.wrapping_sub(4), at.wrapping_sub(12), at] {
            if mem.u32(m + 8).ok() != Some(MODEL_TYPE) {
                continue;
            }
            println!(
                "M 0x{m:08X}: maxLOD {} minLOD {} distance {}",
                mem.u8(m + 0x7C)?,
                mem.u8(m + 0x80)?,
                f32_at(&mem, m + 0x84)?
            );
            for (p, _) in mem.find_u32(&[m]) {
                let obj = p.wrapping_sub(0x1E0);
                let (Ok(flags), Ok(state), Ok(rung)) = (mem.u16(obj + 0x12), mem.u16(obj + 0x352), mem.u8(obj + 0x354)) else { continue };
                println!("  OBJ 0x{obj:08X}: flags 0x{flags:04X} cross-fade {} view_state 0x{state:04X} rung {rung}", flags >> 9 & 1);
            }
        }
    }
    Ok(())
}

/// `dump-near`: every `SceneObject` within `radius` of `at`, nearest first: key, position, template.
pub fn dump_near(capture: &str, at: [f32; 3], radius: f32) -> Result<(), String> {
    let cap = Capture::open(std::path::Path::new(capture))?;
    let mem = cap.mem()?;
    let table = mem.container_named("SceneObject")?;
    let mut near = Vec::new();
    for r in mem.records(&table)? {
        let p = floats::<3>(&mem, r.va)?;
        let d = (0..3).map(|k| (p[k] - at[k]).powi(2)).sum::<f32>().sqrt();
        if d <= radius {
            near.push((d, r.key, p, mem.u32(r.va + 0x14)?));
        }
    }
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!("[dump-near] {} objects within {radius} of {at:?}", near.len());
    for (d, key, p, template) in near {
        println!("{d:8.1} m  0x{key:08X} at {p:?} template 0x{template:08X}");
    }
    Ok(())
}

/// `dump-entity`: every live component record of `key`, across all containers.
pub fn dump_entity(capture: &str, key: u32) -> Result<(), String> {
    let cap = Capture::open(std::path::Path::new(capture))?;
    let mem = cap.mem()?;
    println!("[dump-entity] 0x{key:08X} in {}", cap.path.display());
    for c in mem.containers()? {
        let Ok(records) = mem.records(&c) else {
            println!("{}: unreadable", c.name);
            continue;
        };
        for r in records.iter().filter(|r| r.key == key) {
            println!("{} (stride {}) @0x{:08X}", c.name, c.stride, r.va);
            print!("{}", crate::dump::hexdump(r.va, &mem.read(r.va, c.stride as usize)?));
        }
    }
    Ok(())
}

pub fn dump_render(capture: &str) -> Result<(), String> {
    let cap = Capture::open(std::path::Path::new(capture))?;
    let mem = cap.mem()?;
    println!("[dump-render] {} (build {})", cap.path.display(), cap.build_id);
    print_camera(&mem)?;
    print_wind(&mem)?;
    print_batches(&mem)
}
