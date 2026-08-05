//! `shaderforge` — inspect `shader3.bin` and perform the M4 instancing splice.
//!
//! Subcommands:
//!   list       <shader3.bin>              inventory (id / offset / size / kind)
//!   static-vs  <shader3.bin>              the non-skinned `objectData` VS = splice targets
//!   consts     <shader3.bin> <rec>        CTAB constants of one record
//!   splice     <shader3.bin> <rec> [out]  redirect objectData(World) → instance stream, verify
//!   bundle     <shader3.bin> <out.bin>    R0 test bundle: id+orig+spliced for every target VS,
//!                                         consumed by the in-game `shader_accept_probe.asi`
//!   disasm     <shader3.bin> <rec>        disassemble one record's SM3.0 body (recovery surface)
//!   roles      <shader3.bin> [role]       classify every record by CTAB signature; `role` filters
//!                                         (e.g. `roles shader3.bin fullscreen` = sky/post candidates)
//!
//! The splice is the CTAB-driven operand redirect documented in
//! `docs/reverse_engineer/density_render_instancing_design.md` §2.1: it moves the
//! per-object World matrix off constant registers onto four per-instance vertex
//! inputs, leaving the shared ViewProj (`viewContextData`) and all math untouched.

use mercs2_formats::shader3::{
    classify_role, disassemble, parse_ctab, splice_instanced_world, verify_splice, ShaderKind,
    ShaderRole, Store,
};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let r = match args.get(1).map(String::as_str) {
        Some("list") => cmd_list(&args),
        Some("static-vs") => cmd_static_vs(&args),
        Some("consts") => cmd_consts(&args),
        Some("splice") => cmd_splice(&args),
        Some("bundle") => cmd_bundle(&args),
        Some("disasm") => cmd_disasm(&args),
        Some("roles") => cmd_roles(&args),
        _ => {
            eprintln!(
                "usage: shaderforge <list|static-vs|consts|splice|bundle|disasm|roles> <shader3.bin> [rec|role|out] [out]"
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

/// R0 in-game test bundle. Little-endian throughout:
///   magic u32 = "R0SB"  |  version u32 = 1  |  count u32
///   count × { id:u32, object_data_reg:u16, world_regs:u16, input_base:u8,
///             texcoord_base:u8, _pad:u16, orig_len:u32, spliced_len:u32,
///             orig[orig_len], spliced[spliced_len], pad-to-4 }
/// The probe ASI feeds `orig` (control) and `spliced` to `CreateVertexShader`.
fn cmd_bundle(args: &[String]) -> R {
    let store = load(args)?;
    let out = args.get(3).ok_or("missing output path")?;
    let mut buf = Vec::new();
    buf.extend_from_slice(b"R0SB");
    buf.extend_from_slice(&1u32.to_le_bytes());
    let count_pos = buf.len();
    buf.extend_from_slice(&0u32.to_le_bytes()); // count, backfilled
    let mut count = 0u32;
    let mut skipped = 0u32;
    for r in &store.records {
        if !matches!(r.kind, ShaderKind::Vertex) {
            continue;
        }
        let blob = store.blob(r);
        let (_c, consts) = parse_ctab(blob)?;
        let names: Vec<&str> = consts.iter().map(|c| c.name.as_str()).collect();
        if !names.contains(&"objectData") || names.contains(&"BoneMatrixArray") {
            continue;
        }
        let (spliced, rep) = match splice_instanced_world(blob, None, None) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("  skip id=0x{:08x}: {e}", r.id);
                skipped += 1;
                continue;
            }
        };
        verify_splice(&spliced, &rep)?;
        buf.extend_from_slice(&r.id.to_le_bytes());
        buf.extend_from_slice(&rep.object_data_reg.to_le_bytes());
        buf.extend_from_slice(&(rep.world_regs as u16).to_le_bytes());
        buf.push(rep.input_base as u8);
        buf.push(rep.texcoord_base as u8);
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(spliced.len() as u32).to_le_bytes());
        buf.extend_from_slice(blob);
        buf.extend_from_slice(&spliced);
        while buf.len() % 4 != 0 {
            buf.push(0);
        }
        count += 1;
    }
    buf[count_pos..count_pos + 4].copy_from_slice(&count.to_le_bytes());
    std::fs::write(out, &buf)?;
    println!("wrote {out}: {count} target VS (orig+spliced), {skipped} skipped, {} bytes", buf.len());
    Ok(())
}

fn role_str(r: ShaderRole) -> &'static str {
    match r {
        ShaderRole::SkinnedMeshVs => "skinned-vs",
        ShaderRole::StaticMeshVs => "static-vs",
        ShaderRole::FullscreenVs => "fullscreen-vs",
        ShaderRole::OtherVs => "other-vs",
        ShaderRole::DecalPs => "decal-ps",
        ShaderRole::SkyPs => "sky-ps",
        ShaderRole::TexturedPs => "textured-ps",
        ShaderRole::PlainPs => "plain-ps",
    }
}

/// Disassemble one record's SM3.0 body — the recovery surface a WGSL translation reads off.
fn cmd_disasm(args: &[String]) -> R {
    let store = load(args)?;
    let idx: usize = args.get(3).ok_or("missing rec index")?.parse()?;
    let r = store.records.get(idx).ok_or("rec index out of range")?;
    let blob = store.blob(r);
    let (creator, consts) = parse_ctab(blob)?;
    let role = classify_role(&r.kind, &consts);
    println!(
        "rec{idx} id=0x{:08x} {} size={} role={}",
        r.id,
        kind_str(&r.kind),
        r.blob_size,
        role_str(role)
    );
    println!("creator: {creator}");
    println!("consts:  {}", fmt_consts(&consts));
    println!("--- SM3.0 disassembly ---");
    for line in disassemble(blob)? {
        println!("  {line}");
    }
    Ok(())
}

/// Classify every record by CTAB signature (the only static identity handle: `id != FNV(name)`).
/// With an optional `role` argument, list only matching records; else print a role histogram + the
/// W5 sky/decal candidate buckets. NOTE: buckets are candidate classes, not proven `Pg*` names —
/// mapping a member to its exact name is confirm-live (the `%0x1200` id→blob table).
fn cmd_roles(args: &[String]) -> R {
    let store = load(args)?;
    let filter = args.get(3).map(|s| s.to_ascii_lowercase());
    let mut hist: std::collections::BTreeMap<&'static str, u32> = Default::default();
    let mut listed = 0u32;
    for (i, r) in store.records.iter().enumerate() {
        let blob = store.blob(r);
        let (_c, consts) = parse_ctab(blob)?;
        let role = classify_role(&r.kind, &consts);
        let rs = role_str(role);
        *hist.entry(rs).or_default() += 1;
        if let Some(f) = &filter {
            if rs.contains(f.as_str()) || (f == "w5" && role.is_w5_candidate()) {
                let names: Vec<&str> = consts.iter().map(|c| c.name.as_str()).collect();
                println!("  rec{i:<3} id=0x{:08x} {} :: {}", r.id, rs, names.join(", "));
                listed += 1;
            }
        }
    }
    if filter.is_some() {
        println!("{listed} records matched");
    } else {
        println!("role histogram ({} records):", store.records.len());
        for (k, v) in &hist {
            println!("  {k:<14} {v}");
        }
        println!("(buckets are CTAB-signature candidates, NOT proven Pg* names — id->name is confirm-live)");
    }
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
    println!(
        "  {} const-read operands redirected; {} input->temp copies inserted (1-input-per-instr rule)",
        report.redirects.len(),
        report.input_copies
    );
    println!("  size {} -> {} bytes (+{} for 4 input dcls)", blob.len(), spliced.len(), spliced.len() - blob.len());
    if let Some(out) = args.get(4) {
        std::fs::write(out, &spliced)?;
        println!("  wrote {out}");
    } else {
        println!("  (pass an output path to write the spliced vs_3_0 blob)");
    }
    Ok(())
}
