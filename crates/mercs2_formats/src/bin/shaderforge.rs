//! `shaderforge` — inspect `shader3.bin` and perform the M4 instancing splice.
//!
//! Subcommands:
//!   list       <shader3.bin>              inventory (id / offset / size / kind)
//!   static-vs  <shader3.bin>              the non-skinned `objectData` VS = splice targets
//!   consts     <shader3.bin> <rec>        CTAB constants of one record
//!   splice     <shader3.bin> <rec> [out]  redirect objectData(World) → instance stream, verify
//!
//! The splice is the CTAB-driven operand redirect documented in
//! `docs/reverse_engineer/density_render_instancing_design.md` §2.1: it moves the
//! per-object World matrix off constant registers onto four per-instance vertex
//! inputs, leaving the shared ViewProj (`viewContextData`) and all math untouched.

use mercs2_formats::shader3::{
    parse_ctab, splice_instanced_world, verify_splice, ShaderKind, Store,
};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let r = match args.get(1).map(String::as_str) {
        Some("list") => cmd_list(&args),
        Some("static-vs") => cmd_static_vs(&args),
        Some("consts") => cmd_consts(&args),
        Some("splice") => cmd_splice(&args),
        _ => {
            eprintln!(
                "usage: shaderforge <list|static-vs|consts|splice> <shader3.bin> [rec] [out]"
            );
            return ExitCode::from(2);
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type R = Result<(), Box<dyn std::error::Error>>;

fn load(args: &[String]) -> Result<Store, Box<dyn std::error::Error>> {
    let path = args.get(2).ok_or("missing shader3.bin path")?;
    Ok(Store::parse(std::fs::read(path)?)?)
}

fn kind_str(k: &ShaderKind) -> &'static str {
    match k {
        ShaderKind::Vertex => "vs",
        ShaderKind::Pixel => "ps",
    }
}

fn cmd_list(args: &[String]) -> R {
    let store = load(args)?;
    let vs = store.records.iter().filter(|r| matches!(r.kind, ShaderKind::Vertex)).count();
    println!(
        "{} records: {} vs_3_0 + {} ps_3_0",
        store.records.len(),
        vs,
        store.records.len() - vs
    );
    for (i, r) in store.records.iter().enumerate() {
        println!(
            "  rec{i:<3} id=0x{:08x} off=0x{:06x} size={:<5} {}",
            r.id,
            r.blob_off,
            r.blob_size,
            kind_str(&r.kind)
        );
    }
    Ok(())
}

fn fmt_consts(consts: &[mercs2_formats::shader3::Constant]) -> String {
    let set = |s: u16| ["b", "i", "c", "s"].get(s as usize).copied().unwrap_or("?");
    consts
        .iter()
        .map(|c| {
            format!(
                "{}[{}{}:{}]",
                c.name,
                set(c.register_set),
                c.register_index,
                c.register_index + c.register_count.saturating_sub(1)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn cmd_static_vs(args: &[String]) -> R {
    let store = load(args)?;
    let mut n = 0;
    for (i, r) in store.records.iter().enumerate() {
        if !matches!(r.kind, ShaderKind::Vertex) {
            continue;
        }
        let (_c, consts) = parse_ctab(store.blob(r))?;
        let names: Vec<&str> = consts.iter().map(|c| c.name.as_str()).collect();
        if names.contains(&"objectData") && !names.contains(&"BoneMatrixArray") {
            let od = consts.iter().find(|c| c.name == "objectData").unwrap();
            println!(
                "  rec{i:<3} id=0x{:08x} size={:<5} objectData@c{} :: {}",
                r.id,
                r.blob_size,
                od.register_index,
                fmt_consts(&consts)
            );
            n += 1;
        }
    }
    println!("{n} non-skinned objectData VS (splice targets)");
    Ok(())
}

fn cmd_consts(args: &[String]) -> R {
    let store = load(args)?;
    let idx: usize = args.get(3).ok_or("missing rec index")?.parse()?;
    let r = store.records.get(idx).ok_or("rec index out of range")?;
    let (creator, consts) = parse_ctab(store.blob(r))?;
    println!("rec{idx} id=0x{:08x} {} size={}", r.id, kind_str(&r.kind), r.blob_size);
    println!("creator: {creator}");
    println!("consts:  {}", fmt_consts(&consts));
    Ok(())
}

fn cmd_splice(args: &[String]) -> R {
    let store = load(args)?;
    let idx: usize = args.get(3).ok_or("missing rec index")?.parse()?;
    let r = store.records.get(idx).ok_or("rec index out of range")?;
    if !matches!(r.kind, ShaderKind::Vertex) {
        return Err("record is not a vertex shader".into());
    }
    let blob = store.blob(r);
    let (spliced, report) = splice_instanced_world(blob, None, None)?;
    verify_splice(&spliced, &report)?;
    let last = report.world_regs - 1;
    println!("rec{idx} id=0x{:08x}: SPLICED + VERIFIED", r.id);
    println!(
        "  objectData(World, {}x4) c{}..c{}  ->  instance inputs v{}..v{} (dcl_texcoord{}..{})",
        report.world_regs,
        report.object_data_reg,
        report.object_data_reg as u32 + last,
        report.input_base,
        report.input_base + last,
        report.texcoord_base,
        report.texcoord_base + last
    );
    println!("  {} const-read operands redirected", report.redirects.len());
    println!("  size {} -> {} bytes (+{} for 4 input dcls)", blob.len(), spliced.len(), spliced.len() - blob.len());
    if let Some(out) = args.get(4) {
        std::fs::write(out, &spliced)?;
        println!("  wrote {out}");
    } else {
        println!("  (pass an output path to write the spliced vs_3_0 blob)");
    }
    Ok(())
}
