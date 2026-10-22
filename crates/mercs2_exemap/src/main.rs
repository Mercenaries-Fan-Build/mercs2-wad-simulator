//! `exemap` — identify a Mercenaries 2 executable and check it against its code layout.
//!
//! ```text
//! exemap identify <exe>
//! exemap check <exe> --reference <layout-reference-exe>
//! exemap xex <xex> --key retail|devkit --out <pe>
//! exemap ghidra-pe <pe> --out <pe>
//! exemap starts <image> --out <list>
//! exemap derive <image_id> --from <container> --out <image>
//! exemap imports <image_id> --from <container> --exports <dir>
//! exemap rtti <image> --out <tsv>
//! exemap luareg <image> --out <tsv>
//! exemap corpus-derive <corpus_id> <wad> --from <container> --work <dir>
//! exemap lua-scripts <wad> [--dump <dir>]
//! exemap lua-index <corpus_id> --wad <file>... --out <tsv>
//! exemap lua-pair <a.tsv> <b.tsv> --out <tsv>
//! exemap lua-link <index.tsv> <image.luareg.tsv> --out <tsv>
//! exemap match <image_a> <image_b> --exports <dir> --evidence <list> --out <tsv> --conflicts <tsv>
//! exemap translate <image> <address> --to <image> --matches <dir>
//! exemap coverage --matches <dir> --out <tsv> --contradictions <tsv>
//! exemap annotate <image_id> --functions <jsonl> --exports <dir> --matches <dir> --out <jsonl>
//! exemap seeds <image_id> --functions <jsonl> --exports <dir> --out <starts> --names <tsv>
//! ```
//!
//! `annotate` reads the image's Ghidra functions export (`<image_id>.functions.jsonl`), its rtti,
//! luareg and (for an image whose chain unpacks a XEX or a PS3 SELF) imports exports, and the match
//! graph, and writes one JSON line per function: every name with its source, and every agreed
//! translation to another graph image. It fails on a functions file that is truncated, of another
//! image or of other bytes than images.tsv's, and on any contradicted translation, listing each
//! (`mercs2_exemap::annotate`).
//!
//! `imports` rebuilds an images.tsv image and writes its imports to `<dir>/<image_id>.imports.tsv`,
//! which FnMnemonicSig reads as its imports sidecar. The header line names the format:
//!
//! * an image whose derivation chain has one `xex:` step (Xbox 360): header
//!   `address\tkind\tlibrary\tordinal\tname`, one row per record of the XEX's import libraries
//!   optional header (`0x000103FF`), in header order: the record's virtual address (`0x%08x`),
//!   `function` (a type-1 stub) or `variable` (a type-0 slot), the library name, the ordinal
//!   (decimal) and the export's name from `data/xbox_exports.tsv` (Xenia's export tables). Every
//!   record's word in the image must be `type << 24 | library_index << 16 | ordinal` with type 0 or
//!   1 and the library's own header index, and every function stub must end `mtctr r11; bctr`
//!   (`mercs2_exemap::xbox::import_records`); a library and ordinal the table does not list, or a
//!   function record whose export the table lists as a variable, is an error
//!   (`mercs2_exemap::xbox::imports_tsv`);
//! * an image whose chain ends in its one `unself:` step (PS3): header
//!   `address\tkind\tlibrary\tfnid\tname\tstub`, one row per function import of the ELF's PRX
//!   import table, ascending by stub address: the code stub's address (`0x%08x`), `function`, the
//!   library name, the FNID (8 lowercase hex digits), the function's name and the stub's bytes
//!   (lowercase hex, so its length gives the stub size). Every stub must be the 32-byte PRX stub
//!   form and load the slot of exactly one function import, and the stubs must fill their
//!   executable section; a library with variable or TLS imports is an error
//!   (`mercs2_exemap::prx::prx_imports`). The name is the one of `data/ps3_fnid_names.tsv` whose
//!   hash is the FNID, or the module-level NID of `data/ps3_fixed_nids.tsv`; an FNID with neither
//!   is an error listing every unnamed FNID by library (`mercs2_exemap::prx::prx_imports_tsv`).
//!
//! An image whose chain has neither step (every PC image), or both, is an error.
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
    /// Rewrite an Xbox 360 PE image's section headers to its RVA layout.
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
    /// Write <exports>/<image_id>.imports.tsv: one row per XEX import record (address, function or
    /// variable, library, ordinal, name) of an images.tsv image whose chain unpacks a XEX, or one
    /// row per PRX function import (stub address, function, library, FNID, name, stub bytes) of an
    /// image whose chain ends unpacking a PS3 SELF.
    Imports {
        image_id: String,
        /// The first file of the image's chain: a disc image, XEX, SELF or PSN package.
        #[arg(long)]
        from: PathBuf,
        /// The exports directory the file is written to.
        #[arg(long)]
        exports: PathBuf,
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
        /// The first file of the WAD's chain (an archive, package or disc image).
        #[arg(long)]
        from: PathBuf,
        /// Work directory for the files each step writes; the WAD is left in it.
        #[arg(long)]
        work: PathBuf,
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
    /// Translate a function address of one image to another along every path of the match graph
    /// (a directory of `match` outputs); fails when the paths disagree or none answers.
    Translate {
        image: String,
        /// 0x-prefixed hex function address in `image`.
        address: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        matches: PathBuf,
    },
    /// Translate every function of every image in the match graph to every other image: counts per
    /// image pair, and every contradiction; fails when there is a contradiction.
    Coverage {
        #[arg(long)]
        matches: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        contradictions: PathBuf,
    },
    /// Write one JSON line per function of an image's Ghidra functions export: every name it
    /// carries (Ghidra, rtti, luareg, imports) with its source, and its agreed translation to every
    /// other image of the match graph; fails on any contradicted translation.
    Annotate {
        image_id: String,
        /// The image's functions export (<image_id>.functions.jsonl).
        #[arg(long)]
        functions: PathBuf,
        /// The exports directory holding <image_id>.rtti.tsv, .luareg.tsv and .imports.tsv.
        #[arg(long)]
        exports: PathBuf,
        /// A directory of `match` outputs.
        #[arg(long)]
        matches: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Write the seed starts of an image: every rtti or luareg target that is not a function of its
    /// Ghidra functions export, as a CreateFunctions.java start list, with a names TSV for review.
    Seeds {
        image_id: String,
        /// The image's functions export (<image_id>.functions.jsonl).
        #[arg(long)]
        functions: PathBuf,
        /// The exports directory holding <image_id>.rtti.tsv, .luareg.tsv and .imports.tsv.
        #[arg(long)]
        exports: PathBuf,
        /// The start list written: one hex VA per line, no 0x.
        #[arg(long)]
        out: PathBuf,
        /// The names TSV written: va, source, name, detail.
        #[arg(long)]
        names: PathBuf,
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
                    o.detail.as_deref().unwrap_or(mercs2_exemap::NO_VALUE)
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
        Command::Imports { image_id, from, exports } => {
            use mercs2_exemap::images::Step;
            use mercs2_exemap::xbox::{import_records, imports_tsv, ImportKind, XboxExports};
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
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            };
            let xex_steps = image.chain.iter().filter(|l| matches!(l.step, Step::Xex(_))).count();
            let unself_steps = image.chain.iter().filter(|l| matches!(l.step, Step::Unself(_))).count();
            if (xex_steps == 0) == (unself_steps == 0) {
                eprintln!(
                    "error: image {image_id}: its derivation chain has {xex_steps} xex steps and {unself_steps} unself steps; \
                     imports are read from the XEX or from the PS3 SELF's ELF of a chain with one of the two"
                );
                return Err(ExitCode::from(1));
            }
            if unself_steps > 0 {
                use mercs2_exemap::prx::{prx_imports, prx_imports_tsv, FnidNames, STUB_SIZE};
                let names = FnidNames::embedded().map_err(|e| {
                    eprintln!("error: the embedded FNID name tables are invalid: {e}");
                    ExitCode::from(2)
                })?;
                let elf = mercs2_exemap::images::derive_self_elf(image, &mut file).map_err(fail)?;
                let imports = prx_imports(&elf).map_err(|e| fail(mercs2_exemap::Error::new(format!("image {image_id}: {e}"))))?;
                let text = prx_imports_tsv(&imports, &names).map_err(|e| fail(mercs2_exemap::Error::new(format!("image {image_id}: {e}"))))?;
                let out = exports.join(format!("{image_id}.imports.tsv"));
                write(&out, text.as_bytes())?;
                let mut per_lib: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
                for i in &imports {
                    *per_lib.entry(i.library.as_str()).or_default() += 1;
                }
                for (lib, n) in &per_lib {
                    println!("library    {lib}: {n} functions");
                }
                let (lo, hi) = (imports[0].stub, imports[imports.len() - 1].stub + STUB_SIZE as u32);
                println!("stubs      0x{lo:08x}..0x{hi:08x}, {STUB_SIZE} bytes each");
                println!(
                    "imports    {} functions in {} libraries, 0 variables, every FNID named -> {}",
                    imports.len(),
                    per_lib.len(),
                    out.display()
                );
                return Ok(());
            }
            let xbox_exports = XboxExports::embedded().map_err(|e| {
                eprintln!("error: the embedded xbox_exports.tsv is invalid: {e}");
                ExitCode::from(2)
            })?;
            let (bytes, xex) = mercs2_exemap::images::derive_with_xex(image, &mut file).map_err(fail)?;
            let records = import_records(&xex, &bytes).map_err(|e| fail(mercs2_exemap::Error::new(format!("image {image_id}: {e}"))))?;
            let text = imports_tsv(&records, &xbox_exports).map_err(|e| fail(mercs2_exemap::Error::new(format!("image {image_id}: {e}"))))?;
            let out = exports.join(format!("{image_id}.imports.tsv"));
            write(&out, text.as_bytes())?;
            let mut per_lib: std::collections::BTreeMap<&str, (usize, usize)> = std::collections::BTreeMap::new();
            for r in &records {
                let e = per_lib.entry(r.library.as_str()).or_default();
                match r.kind {
                    ImportKind::Function => e.0 += 1,
                    ImportKind::Variable => e.1 += 1,
                }
            }
            for (lib, (f, v)) in &per_lib {
                println!("library    {lib}: {} records ({f} function, {v} variable)", f + v);
            }
            println!("imports    {} records, every one named -> {}", records.len(), out.display());
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
        Command::CorpusDerive { corpus_id, wad, from, work } => {
            use mercs2_exemap::corpus::{derive, Corpora, Env};
            let corpora = Corpora::embedded().map_err(|e| {
                eprintln!("error: the embedded corpus tables are invalid: {e}");
                ExitCode::from(2)
            })?;
            let w = corpora.wads_of(&corpus_id).find(|w| w.name == wad).ok_or_else(|| {
                eprintln!("error: wads.tsv has no {wad} in corpus {corpus_id}");
                ExitCode::from(1)
            })?;
            let path = derive(w, &from, &Env { work }).map_err(|e| {
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
            let anchors = mercs2_exemap::matcher::anchors(&images).map_err(fail)?;
            let anchored = mercs2_exemap::matcher::anchor_pairs(&image_a, &image_b, &anchors);
            let o = match_images(&a, &b, ia.ghidra_language == ib.ghidra_language, &ev, &anchored).map_err(fail)?;
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
            // A sole candidate that is not corroborated: side `uncorroborated`, the image_a function,
            // and its one image_b candidate.
            for p in &o.uncorroborated {
                let es: Vec<&str> = p.evidence.iter().map(|e| e.as_str()).collect();
                ctext.push_str(&format!("uncorroborated\t0x{:08x}\t0x{:08x}({})\n", p.a, p.b, es.join("+")));
            }
            // A rejected candidate: side is the reason, the image_a function, then its image_b
            // function with the rejected evidence and both functions' mhash.
            let mut by_reason = std::collections::BTreeMap::new();
            for r in &o.rejected {
                *by_reason.entry(r.reason.as_str()).or_insert(0usize) += 1;
                let es: Vec<&str> = r.evidence.iter().map(|e| e.as_str()).collect();
                ctext.push_str(&format!(
                    "{}\t0x{:08x}\t0x{:08x}({}) mhash_a={} mhash_b={}\n",
                    r.reason.as_str(),
                    r.a,
                    r.b,
                    es.join("+"),
                    r.mhash_a,
                    r.mhash_b
                ));
            }
            write(&conflicts, ctext.as_bytes())?;
            println!("pairs      {} -> {}", o.pairs.len(), out.display());
            println!("evidence   {:?}", by_kind);
            println!(
                "conflicts  {} (+ {} uncorroborated sole candidates) -> {}",
                o.conflicts.len(),
                o.uncorroborated.len(),
                conflicts.display()
            );
            println!("rejected   {} {:?} -> {}", o.rejected.len(), by_reason, conflicts.display());
            let chk =mercs2_exemap::matcher::check_anchors(&image_a, &image_b, &o, &anchors);
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
        Command::Translate { image, address, to, matches } => {
            use mercs2_exemap::xmap::{load_dir, parse_address, path_str, Graph, Verdict, Walk};
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            };
            let images = mercs2_exemap::images::Images::embedded(&catalogue).map_err(fail)?;
            for id in [&image, &to] {
                if images.image(id).is_none() {
                    eprintln!("error: images.tsv has no image {id}");
                    return Err(ExitCode::from(1));
                }
            }
            if image == to {
                eprintln!("error: {image} is both the source and the target image");
                return Err(ExitCode::from(1));
            }
            let f = parse_address(&address).map_err(fail)?;
            let dir = load_dir(&matches, &images).map_err(fail)?;
            let g = Graph::new(&dir.pairs).map_err(fail)?;
            let t = g.translate(&image, f, &to);
            for (p, w) in &t.paths {
                match w {
                    Walk::Answer(r) => println!("path       {:<70} 0x{r:08x}", path_str(p)),
                    Walk::Stops { at, function } => println!(
                        "path       {:<70} stops: {} 0x{function:08x} has no pair with {}",
                        path_str(p),
                        p[*at],
                        p[*at + 1]
                    ),
                }
            }
            match t.verdict() {
                Verdict::Agreed(r) => {
                    let answering = t.paths.iter().filter(|(_, w)| matches!(w, Walk::Answer(_))).count();
                    println!("{to} 0x{r:08x} ({answering} of {} paths answer, all agree)", t.paths.len());
                }
                Verdict::NoPath => {
                    eprintln!("error: {image} 0x{f:08x} is untranslatable: the match graph has no path from {image} to {to}");
                    return Err(ExitCode::from(1));
                }
                Verdict::Untranslated => {
                    eprintln!("error: {image} 0x{f:08x} is untranslatable: every one of the {} paths to {to} stops", t.paths.len());
                    return Err(ExitCode::from(1));
                }
                Verdict::Contradicted(answers) => {
                    eprintln!("error: {image} 0x{f:08x} -> {to}: the paths disagree");
                    for (r, idx) in &answers {
                        let ps: Vec<String> = idx.iter().map(|i| path_str(&t.paths[*i].0)).collect();
                        eprintln!("  0x{r:08x} via {}", ps.join(", "));
                    }
                    return Err(ExitCode::from(1));
                }
            }
        }
        Command::Coverage { matches, out, contradictions } => {
            use mercs2_exemap::xmap::{load_dir, Graph, CONTRADICTIONS_HEADER, COVERAGE_HEADER};
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            };
            let images = mercs2_exemap::images::Images::embedded(&catalogue).map_err(fail)?;
            let dir = load_dir(&matches, &images).map_err(fail)?;
            let g = Graph::new(&dir.pairs).map_err(fail)?;
            let (rows, cs) = g.coverage();
            let mut text = format!("{COVERAGE_HEADER}\n");
            for r in &rows {
                text.push_str(&r.tsv());
                text.push('\n');
            }
            write(&out, text.as_bytes())?;
            let mut ctext = format!("{CONTRADICTIONS_HEADER}\n");
            for c in &cs {
                ctext.push_str(&c.tsv());
                ctext.push('\n');
            }
            write(&contradictions, ctext.as_bytes())?;
            println!(
                "graph      {} images, {} pairs files ({} conflicts files not read: they name no images)",
                g.images().count(),
                dir.pairs.len(),
                dir.conflicts_files
            );
            for r in &rows {
                println!(
                    "{:<22} -> {:<22} {:>6} functions: {:>6} translated ({:>6} direct, {:>6} confirmed by a composed path, {:>6} composed only), {:>4} contradicted ({:>4} against a direct pair), {:>6} untranslated",
                    r.image, r.to, r.functions, r.translated, r.direct, r.direct_confirmed, r.composed_only, r.contradicted, r.contradicted_direct, r.untranslated
                );
            }
            println!("coverage   -> {}", out.display());
            println!("contradictions {} -> {}", cs.len(), contradictions.display());
            if !cs.is_empty() {
                eprintln!("error: {} translations are contradicted by the match graph", cs.len());
                return Err(ExitCode::from(1));
            }
        }
        Command::Annotate { image_id, functions, exports, matches, out } => {
            use mercs2_exemap::annotate::{annotate, annotated_image, read_functions, sidecar_names};
            use mercs2_exemap::xmap::{load_dir, Graph};
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            };
            let images = mercs2_exemap::images::Images::embedded(&catalogue).map_err(fail)?;
            let image = annotated_image(&images, &image_id).map_err(fail)?;
            let text = std::fs::read_to_string(&functions).map_err(|e| {
                eprintln!("error: cannot read {}: {e}", functions.display());
                ExitCode::from(2)
            })?;
            let file = read_functions(&text, &functions.display().to_string()).map_err(fail)?;
            drop(text);
            let names = sidecar_names(&exports, image).map_err(fail)?;
            let dir = load_dir(&matches, &images).map_err(fail)?;
            let g = Graph::new(&dir.pairs).map_err(fail)?;
            let a = annotate(&images, &image_id, &file, &names, &g).map_err(fail)?;
            write(&out, a.text.as_bytes())?;
            println!(
                "graph      {} images, {} pairs files ({} conflicts files not read: they name no images)",
                g.images().count(),
                dir.pairs.len(),
                dir.conflicts_files
            );
            for (source, (on, off)) in &a.sidecar_rows {
                println!("names      {source:<7} {on} rows name a function, {off} rows name an address that is no function of the functions file");
            }
            for (to, n) in &a.translated {
                println!("{image_id:<22} -> {to:<22} {n:>6} of {} functions translated", a.functions);
            }
            println!("untranslatable {} (no path in the graph)", a.untranslatable_images.join(" "));
            println!("functions  {} -> {}", a.functions, out.display());
        }
        Command::Seeds { image_id, functions, exports, out, names } => {
            use mercs2_exemap::annotate::{annotated_image, read_functions, seeds, sidecar_names};
            let fail = |e: mercs2_exemap::Error| {
                eprintln!("error: {e}");
                ExitCode::from(1)
            };
            let images = mercs2_exemap::images::Images::embedded(&catalogue).map_err(fail)?;
            let image = annotated_image(&images, &image_id).map_err(fail)?;
            let text = std::fs::read_to_string(&functions).map_err(|e| {
                eprintln!("error: cannot read {}: {e}", functions.display());
                ExitCode::from(2)
            })?;
            let file = read_functions(&text, &functions.display().to_string()).map_err(fail)?;
            drop(text);
            let by_va = sidecar_names(&exports, image).map_err(fail)?;
            let s = seeds(&images, &image_id, &file, &by_va).map_err(fail)?;
            write(&out, s.starts.as_bytes())?;
            write(&names, s.names.as_bytes())?;
            println!("seeds      {} rtti/luareg targets that are no function of {} -> {}", s.count, functions.display(), out.display());
            println!("names      -> {}", names.display());
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
