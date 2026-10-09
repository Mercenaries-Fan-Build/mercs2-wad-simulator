//! `exemap` — identify a Mercenaries 2 executable and check it against its code layout.
//!
//! ```text
//! exemap identify <exe>
//! exemap check <exe> --reference <layout-reference-exe>
//! exemap xex <xex> --key retail|devkit --out <pe>
//! exemap ghidra-pe <pe> --out <pe>
//! exemap starts <image> --out <list>
//! exemap derive <image_id> --from <container> --out <image>
//! exemap rtti <image> --out <tsv>
//! exemap luareg <image> --out <tsv>
//! exemap corpus-derive <corpus_id> <wad> --from <container> --work <dir> [--wine <wine64>]
//! exemap lua-scripts <wad> [--dump <dir>]
//! exemap lua-index <corpus_id> --wad <file>... --out <tsv>
//! exemap lua-pair <a.tsv> <b.tsv> --out <tsv>
//! exemap lua-link <index.tsv> <image.luareg.tsv> --out <tsv>
//! exemap match <image_a> <image_b> --exports <dir> --evidence <list> --out <tsv> --conflicts <tsv>
//! ```
//!
//! Exit status: 0 on success; 1 on an unknown build, an unexpected difference, or an input the
//! command cannot handle; 2 when a file cannot be read or written.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mercs2_exemap::starts::{function_starts, Source};
use mercs2_exemap::xbox::{ghidra_pe, unpack_xex, XexKey};
use mercs2_exemap::{identify_in, membership, Catalogue};

#[derive(Parser)]
#[command(name = "exemap", version, about = "Identify a Mercenaries 2 executable and check its layout membership")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the build id of an executable, found by its sha256.
    Identify { exe: PathBuf },
    /// Print the build id, its layout, and every overlay that separates it from the layout reference.
    Check {
        exe: PathBuf,
        /// The layout's reference build (builds.tsv names it per layout in layouts.tsv).
        #[arg(long)]
        reference: PathBuf,
    },
    /// Recover the PE image from an Xbox 360 XEX2 file.
    Xex {
        xex: PathBuf,
        /// Key-encryption key that wraps the session key: retail or devkit.
        #[arg(long)]
        key: XexKey,
        #[arg(long)]
        out: PathBuf,
    },
    /// Rewrite an Xbox 360 PE image's section headers to its RVA layout, for Ghidra's PE loader.
    GhidraPe {
        pe: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Write the function starts of an Xbox 360 PE (.pdata) or PS3 ELF (.opd), one hex VA per line.
    Starts {
        image: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Rebuild a console image from its original container (images.tsv chain), checking every sha256.
    Derive {
        image_id: String,
        /// The first file of the image's chain: a disc image, XEX, SELF or PSN package.
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Write the vtables recovered from the image's C++ type information (MSVC or Itanium), one
    /// row per slot.
    Rtti {
        image: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Write the Lua C-function registration tables (luaL_Reg), one row per entry.
    Luareg {
        image: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Rebuild one wads.tsv WAD from its original container, checking every sha256.
    CorpusDerive {
        corpus_id: String,
        wad: String,
        /// The first file of the WAD's chain (an archive, installer or disc image).
        #[arg(long)]
        from: PathBuf,
        /// Work directory for the files each step writes; the WAD is left in it.
        #[arg(long)]
        work: PathBuf,
        /// The wine64 binary, for chains with Wine steps.
        #[arg(long)]
        wine: Option<PathBuf>,
    },
    /// List every Lua script container of a WAD (PC, Xbox 360 or PS3): block, entry, name hash and
    /// each descriptor's tag and size, one row per container.
    LuaScripts {
        wad: PathBuf,
        /// Also write each descriptor body to `<dir>/<block>_<entry>_<name_hash>.<tag>`.
        #[arg(long)]
        dump: Option<PathBuf>,
    },
    /// Write one row per Lua function of a corpora.tsv corpus, from its WAD files (each matched
    /// to wads.tsv by sha256; every WAD of the corpus must be given).
    LuaIndex {
        corpus_id: String,
        #[arg(long = "wad", required = true)]
        wads: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Pair the Lua functions of two lua-index exports by content, one row per pair or unpaired
    /// function, with a count per class.
    LuaPair {
        a: PathBuf,
        b: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Link every Namespace.Name lookup of a lua-index export to the native function the image's
    /// luareg export registers under that name, one row per lookup.
    LuaLink {
        index: PathBuf,
        luareg: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Match the functions of two images.tsv images from their exports (<id>.rtti.tsv,
    /// <id>.luareg.tsv and, for bytes/string/callee evidence, <id>.fnsig.json).
    Match {
        image_a: String,
        image_b: String,
        #[arg(long)]
        exports: PathBuf,
        /// Comma-separated: vtable, lua, bytes, string, callee.
        #[arg(long)]
        evidence: String,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        conflicts: PathBuf,
    },
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), ExitCode> {
    std::fs::write(path, bytes).map_err(|e| {
        eprintln!("error: cannot write {}: {e}", path.display());
        ExitCode::from(2)
    })
}

fn read(path: &Path) -> Result<Vec<u8>, ExitCode> {
    std::fs::read(path).map_err(|e| {
        eprintln!("error: cannot read {}: {e}", path.display());
        ExitCode::from(2)
    })
}

fn run(cli: Cli) -> Result<(), ExitCode> {
    let catalogue = Catalogue::embedded().map_err(|e| {
        eprintln!("error: the embedded catalogue is invalid: {e}");
        ExitCode::from(2)
    })?;
    match cli.command {
        Command::Identify { exe } => {
            let bytes = read(&exe)?;
            let b = identify_in(&catalogue, &bytes).map_err(|e| {
                eprintln!("error: {}: {e}", exe.display());
                ExitCode::from(1)
            })?;
            println!("{}", b.id);
        }
        Command::Check { exe, reference } => {
            let bytes = read(&exe)?;
            let refb = read(&reference)?;
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {}: {e}", exe.display());
                ExitCode::from(1)
            };
            let b = identify_in(&catalogue, &bytes).map_err(fail)?;
            let rows = membership::check(&catalogue, &bytes, &refb).map_err(fail)?;
            println!("build   {}", b.id);
            println!("layout  {}", b.layout);
            println!("overlays {}", rows.len());
            for o in rows {
                println!(
                    "  {:<14} {:<10} {:<4} 0x{:08x}..0x{:08x}  runs {:<7} bytes {:<9} {}",
                    o.kind.as_str(),
                    o.section,
                    o.space.as_str(),
                    o.start,
                    o.end,
                    o.runs,
                    o.diff_bytes,
                    o.detail
                );
            }
        }
        Command::Xex { xex, key, out } => {
            let bytes = read(&xex)?;
            let (pe, info) = unpack_xex(&bytes, key).map_err(|e| {
                eprintln!("error: {}: {e}", xex.display());
                ExitCode::from(1)
            })?;
            write(&out, &pe)?;
            println!("key        {}", key.as_str());
            println!("encrypted  {}", info.encrypted);
            println!("compression {}", info.compression);
            if info.compression == 2 {
                println!("lzx        window 0x{:x}, {} blocks, {} stream bytes", info.window_size, info.blocks, info.stream_len);
            }
            println!("image      {} bytes -> {}", pe.len(), out.display());
            println!("sha256     {}", mercs2_exemap::sha256_hex(&pe));
        }
        Command::GhidraPe { pe, out } => {
            let bytes = read(&pe)?;
            let fixed = ghidra_pe(&bytes).map_err(|e| {
                eprintln!("error: {}: {e}", pe.display());
                ExitCode::from(1)
            })?;
            write(&out, &fixed)?;
            println!("sha256     {}", mercs2_exemap::sha256_hex(&fixed));
        }
        Command::Starts { image, out } => {
            let bytes = read(&image)?;
            let (starts, source) = function_starts(&bytes).map_err(|e| {
                eprintln!("error: {}: {e}", image.display());
                ExitCode::from(1)
            })?;
            let text: String = starts.iter().map(|v| format!("{v:08x}\n")).collect();
            write(&out, text.as_bytes())?;
            match source {
                Source::Pdata { va, records } => println!(".pdata     0x{va:08x}, {records} records"),
                Source::Opd { va, descriptors, toc } => {
                    println!(".opd       0x{va:08x}, {descriptors} descriptors, entry TOC 0x{toc:08x}")
                }
            }
            println!("starts     {} -> {}", starts.len(), out.display());
        }
        Command::Derive { image_id, from, out } => {
            let images = mercs2_exemap::images::Images::embedded(&catalogue).map_err(|e| {
                eprintln!("error: the embedded images table is invalid: {e}");
                ExitCode::from(2)
            })?;
            let image = images.image(&image_id).ok_or_else(|| {
                eprintln!("error: images.tsv has no image {image_id}");
                ExitCode::from(1)
            })?;
            let mut file = std::fs::File::open(&from).map_err(|e| {
                eprintln!("error: cannot read {}: {e}", from.display());
                ExitCode::from(2)
            })?;
            let bytes = mercs2_exemap::images::derive(image, &mut file).map_err(|e| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            })?;
            write(&out, &bytes)?;
            println!("image      {} ({} bytes) -> {}", image.id, bytes.len(), out.display());
            println!("sha256     {}", image.sha256);
        }
        Command::Rtti { image, out } => {
            let bytes = read(&image)?;
            let vts = mercs2_exemap::rtti::vtables(&bytes).map_err(|e| {
                eprintln!("error: {}: {e}", image.display());
                ExitCode::from(1)
            })?;
            let mut text = String::from("abi\traw_name\tclass\tvtable\tsubobject_offset\tslot\tfunction\tdeleting_destructor\n");
            let mut slots = 0usize;
            for v in &vts {
                for (i, f) in v.slots.iter().enumerate() {
                    text.push_str(&format!(
                        "{}\t{}\t{}\t0x{:08x}\t{}\t{}\t0x{:08x}\t{}\n",
                        v.abi.as_str(),
                        v.raw_name,
                        v.class.as_deref().unwrap_or(""),
                        v.va,
                        v.subobject_offset,
                        i,
                        f,
                        v.deleting_destructor.map(|k| k.to_string()).unwrap_or_default()
                    ));
                    slots += 1;
                }
            }
            write(&out, text.as_bytes())?;
            let classes: std::collections::BTreeSet<&str> = vts.iter().map(|v| v.raw_name.as_str()).collect();
            println!("vtables    {} ({} classes, {} slots) -> {}", vts.len(), classes.len(), slots, out.display());
        }
        Command::Luareg { image, out } => {
            let bytes = read(&image)?;
            let tables = mercs2_exemap::luareg::tables(&bytes).map_err(|e| {
                eprintln!("error: {}: {e}", image.display());
                ExitCode::from(1)
            })?;
            let mut text = String::from("table\tnamespace\tindex\tname\tfunction\n");
            let mut entries = 0usize;
            for t in &tables {
                for (i, (name, f)) in t.entries.iter().enumerate() {
                    text.push_str(&format!(
                        "0x{:08x}\t{}\t{}\t{}\t0x{:08x}\n",
                        t.va,
                        t.namespace.as_deref().unwrap_or(""),
                        i,
                        name,
                        f
                    ));
                    entries += 1;
                }
            }
            write(&out, text.as_bytes())?;
            let named = tables.iter().filter(|t| t.namespace.is_some()).count();
            println!("tables     {} ({} named by a registry, {} entries) -> {}", tables.len(), named, entries, out.display());
        }
        Command::CorpusDerive { corpus_id, wad, from, work, wine } => {
            use mercs2_exemap::corpus::{derive, Corpora, Env};
            let corpora = Corpora::embedded().map_err(|e| {
                eprintln!("error: the embedded corpus tables are invalid: {e}");
                ExitCode::from(2)
            })?;
            let w = corpora.wads_of(&corpus_id).find(|w| w.name == wad).ok_or_else(|| {
                eprintln!("error: wads.tsv has no {wad} in corpus {corpus_id}");
                ExitCode::from(1)
            })?;
            let path = derive(w, &from, &Env { wine, work }).map_err(|e| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            })?;
            println!("wad        {corpus_id} {wad} -> {}", path.display());
            println!("sha256     {}", w.sha256);
        }
        Command::LuaScripts { wad, dump } => {
            let ws = mercs2_exemap::wadscripts::scripts(&wad).map_err(|e| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            })?;
            if let Some(dir) = &dump {
                std::fs::create_dir_all(dir).map_err(|e| {
                    eprintln!("error: create {}: {e}", dir.display());
                    ExitCode::from(2)
                })?;
                for s in &ws.scripts {
                    for c in &s.chunks {
                        let name = format!("{}_{}_{:08x}.{}", s.block, s.entry, s.name_hash, String::from_utf8_lossy(&c.tag));
                        write(&dir.join(name), &c.body)?;
                    }
                }
            }
            use mercs2_exemap::luachunk::{decode, decode_info, encode, Proto};
            fn walk(p: &Proto, f: &mut impl FnMut(&Proto)) {
                f(p);
                for q in &p.protos {
                    walk(q, f);
                }
            }
            println!("block\tentry\tname_hash\tname\tfunctions\tinstructions\tdebug\tblock_path\tchunks");
            let (mut functions_total, mut instructions_total) = (0usize, 0usize);
            for s in &ws.scripts {
                let what = format!("block {} entry {} (0x{:08x})", s.block, s.entry, s.name_hash);
                let fail = |m: String| {
                    eprintln!("error: {what}: {m}");
                    ExitCode::from(1)
                };
                let info = s.chunk(b"INFO").ok_or_else(|| fail("no INFO descriptor".into()))?;
                let info = decode_info(info, ws.endian).map_err(|e| fail(e.to_string()))?;
                let binn = s.chunk(b"BINN").ok_or_else(|| fail("no BINN descriptor".into()))?;
                let chunk = decode(binn).map_err(|e| fail(e.to_string()))?;
                if chunk.endian != ws.endian {
                    return Err(fail(format!("a {:?} chunk in a {:?} WAD", chunk.endian, ws.endian)));
                }
                if encode(&chunk) != binn {
                    return Err(fail("the decoded chunk does not re-encode to its bytes".into()));
                }
                let (mut functions, mut instructions, mut debug) = (0usize, 0usize, false);
                walk(&chunk.main, &mut |p| {
                    functions += 1;
                    instructions += p.code.len();
                    debug |= p.source.is_some() || !p.line_info.is_empty() || !p.loc_vars.is_empty() || !p.upvalues.is_empty();
                });
                functions_total += functions;
                instructions_total += instructions;
                let chunks: Vec<String> =
                    s.chunks.iter().map(|c| format!("{}:{}", String::from_utf8_lossy(&c.tag), c.body.len())).collect();
                println!(
                    "{}\t{}\t0x{:08x}\t{}\t{functions}\t{instructions}\t{}\t{}\t{}",
                    s.block,
                    s.entry,
                    s.name_hash,
                    info.name,
                    if debug { "yes" } else { "stripped" },
                    s.block_path,
                    chunks.join(",")
                );
            }
            eprintln!(
                "scripts    {} ({:?}), {functions_total} functions, {instructions_total} instructions, every chunk decoded and re-encoded byte for byte",
                ws.scripts.len(),
                ws.endian
            );
        }
        Command::LuaIndex { corpus_id, wads, out } => {
            use mercs2_exemap::corpus::Corpora;
            use mercs2_exemap::luaindex::{index, HEADER};
            let corpora = Corpora::embedded().map_err(|e| {
                eprintln!("error: the embedded corpus tables are invalid: {e}");
                ExitCode::from(2)
            })?;
            let rows = index(&corpora, &corpus_id, &wads).map_err(|e| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            })?;
            let mut text = String::from(HEADER);
            text.push('\n');
            for r in &rows {
                text.push_str(&r.tsv());
                text.push('\n');
            }
            write(&out, text.as_bytes())?;
            let scripts: std::collections::BTreeSet<(&str, u32)> = rows.iter().map(|r| (r.wad.as_str(), r.name_hash)).collect();
            println!("functions  {} in {} scripts -> {}", rows.len(), scripts.len(), out.display());
        }
        Command::LuaPair { a, b, out } => {
            use mercs2_exemap::luapair::{pair, parse_index, Pairing};
            let load = |p: &Path| {
                let text = std::fs::read_to_string(p).map_err(|e| {
                    eprintln!("error: read {}: {e}", p.display());
                    ExitCode::from(2)
                })?;
                parse_index(&text).map_err(|e| {
                    eprintln!("error: {}: {e}", p.display());
                    ExitCode::from(1)
                })
            };
            let (ra, rb) = (load(&a)?, load(&b)?);
            let pairs = pair(&ra, &rb).map_err(|e| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            })?;
            let mut text = String::from("kind\tscript\tpath_a\tpath_b\n");
            let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for p in &pairs {
                let (kind, script, pa, pb) = match p {
                    Pairing::Pair { script, a, b, class } => (class.name(), script.as_str(), a.as_str(), b.as_str()),
                    Pairing::OnlyA { script, path } => ("only-a", script.as_str(), path.as_str(), ""),
                    Pairing::OnlyB { script, path } => ("only-b", script.as_str(), "", path.as_str()),
                    Pairing::ScriptOnlyA { script } => ("script-only-a", script.as_str(), "", ""),
                    Pairing::ScriptOnlyB { script } => ("script-only-b", script.as_str(), "", ""),
                };
                *counts.entry(kind).or_default() += 1;
                text.push_str(&format!("{kind}\t{script}\t{pa}\t{pb}\n"));
            }
            write(&out, text.as_bytes())?;
            let summary: Vec<String> = counts.iter().map(|(k, n)| format!("{k}={n}")).collect();
            println!("pairs      {} -> {}", summary.join(" "), out.display());
        }
        Command::LuaLink { index, luareg, out } => {
            use mercs2_exemap::lualink::{link, parse_luareg, Link};
            let read = |p: &Path| {
                std::fs::read_to_string(p).map_err(|e| {
                    eprintln!("error: read {}: {e}", p.display());
                    ExitCode::from(2)
                })
            };
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            };
            let rows = mercs2_exemap::luapair::parse_index(&read(&index)?).map_err(fail)?;
            let reg = parse_luareg(&read(&luareg)?).map_err(fail)?;
            let links = link(&rows, &reg).map_err(fail)?;
            let mut text = String::from("script\tpath\tlookup\tstatus\tfunction\n");
            let mut distinct: std::collections::BTreeMap<&str, (&str, String)> = std::collections::BTreeMap::new();
            for l in &links {
                let (status, f) = match &l.link {
                    Link::Resolved(a) => ("resolved", format!("0x{a:08x}")),
                    Link::Ambiguous(v) => ("ambiguous", v.iter().map(|a| format!("0x{a:08x}")).collect::<Vec<_>>().join(";")),
                    Link::UnregisteredInNativeNamespace => ("unregistered-in-native-namespace", String::new()),
                    Link::LuaNamespace => ("lua-namespace", String::new()),
                };
                text.push_str(&format!("{}\t{}\t{}\t{status}\t{f}\n", l.script, l.path, l.lookup));
                distinct.insert(l.lookup.as_str(), (status, f));
            }
            write(&out, text.as_bytes())?;
            let count = |s: &str| distinct.values().filter(|(st, _)| *st == s).count();
            let namespaces: std::collections::BTreeSet<&str> =
                reg.keys().map(|(ns, _)| ns.as_str()).filter(|ns| !ns.is_empty()).collect();
            println!("namespaces {} named by the luareg export", namespaces.len());
            println!(
                "lookups    {} sites, {} distinct: {} resolved, {} ambiguous, {} unregistered in a native namespace, {} in a Lua namespace -> {}",
                links.len(),
                distinct.len(),
                count("resolved"),
                count("ambiguous"),
                count("unregistered-in-native-namespace"),
                count("lua-namespace"),
                out.display()
            );
        }
        Command::Match { image_a, image_b, exports, evidence, out, conflicts } => {
            use mercs2_exemap::matcher::{match_images, Evidence, Exports};
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            };
            let images = mercs2_exemap::images::Images::embedded(&catalogue).map_err(fail)?;
            let lookup = |id: &str| {
                images.image(id).ok_or_else(|| {
                    eprintln!("error: images.tsv has no image {id}");
                    ExitCode::from(1)
                })
            };
            let (ia, ib) = (lookup(&image_a)?, lookup(&image_b)?);
            let mut ev = std::collections::BTreeSet::new();
            for e in evidence.split(',') {
                ev.insert(Evidence::parse(e).ok_or_else(|| {
                    eprintln!("error: unknown evidence {e:?}; expected vtable, lua, bytes, string or callee");
                    ExitCode::from(1)
                })?);
            }
            let sigs = ev.iter().any(|e| matches!(e, Evidence::Bytes | Evidence::String | Evidence::Callee));
            let a = Exports::load(&exports, &image_a, sigs).map_err(fail)?;
            let b = Exports::load(&exports, &image_b, sigs).map_err(fail)?;
            let o = match_images(&a, &b, ia.ghidra_language == ib.ghidra_language, &ev).map_err(fail)?;
            let mut text = format!("{image_a}\t{image_b}\tevidence\n");
            let mut by_kind = std::collections::BTreeMap::new();
            for p in &o.pairs {
                let es: Vec<&str> = p.evidence.iter().map(|e| e.as_str()).collect();
                for e in &es {
                    *by_kind.entry(*e).or_insert(0usize) += 1;
                }
                text.push_str(&format!("0x{:08x}\t0x{:08x}\t{}\n", p.a, p.b, es.join(",")));
            }
            write(&out, text.as_bytes())?;
            let mut ctext = String::from("side\tfunction\tcandidates\n");
            for c in &o.conflicts {
                let cands: Vec<String> = c
                    .candidates
                    .iter()
                    .map(|(f, es)| format!("0x{f:08x}({})", es.iter().map(|e| e.as_str()).collect::<Vec<_>>().join("+")))
                    .collect();
                ctext.push_str(&format!("{}\t0x{:08x}\t{}\n", c.side, c.function, cands.join(" ")));
            }
            write(&conflicts, ctext.as_bytes())?;
            println!("pairs      {} -> {}", o.pairs.len(), out.display());
            println!("evidence   {:?}", by_kind);
            println!("conflicts  {} -> {}", o.conflicts.len(), conflicts.display());
            let anchors = mercs2_exemap::matcher::anchors(&images).map_err(fail)?;
            let chk = mercs2_exemap::matcher::check_anchors(&image_a, &image_b, &o, &anchors);
            println!("anchors    {} agree, {} unmatched, {} contradicted", chk.agreeing, chk.unmatched, chk.contradicted.len());
            if !chk.contradicted.is_empty() {
                for (an, p) in &chk.contradicted {
                    eprintln!(
                        "error: anchor {} 0x{:08x} <-> {} 0x{:08x} ({}) is contradicted by the accepted pair 0x{:08x} <-> 0x{:08x}",
                        an.image_a, an.va_a, an.image_b, an.va_b, an.verified_by, p.a, p.b
                    );
                }
                return Err(ExitCode::from(1));
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}
