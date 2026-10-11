//! `gfx-golden` — turn our extracted retail `.gfx` movies into a **golden reference set**.
//!
//! Walks a directory of `.gfx`/`.swf` movies (the output of `gfx-extract`: `vz.wad`'s movies at the
//! root, `shell/` and `loading/` beside them), parses each via [`mercs2_formats::gfx`], and emits:
//!   * a per-movie inventory (container form, stage size, tag count, feature flags),
//!   * the overall **tag support surface** (which SWF/GFx tags the shipping movies use — the
//!     authoritative "what GFx 2.0.48 actually loads"),
//!   * a **feature → golden-example** index: for each authorable feature (gradient fill, bitmap
//!     fill, embedded/imported fonts, sprites, buttons, edit-text, …) the real movie that best
//!     demonstrates it — so an authoring tool can byte-validate a new emitter against a movie the
//!     game is known to accept.
//!
//! Outputs a human-readable Markdown report and a machine-readable JSON index (for a golden-test
//! harness). Everything is derived from the movies themselves — nothing about the GFx tag set is
//! assumed.
//!
//! Every path is explicit: the movies directory and both outputs. A directory that cannot be read
//! or an output that cannot be written is an error, and a movie that does not parse is recorded in
//! the report AND fails the run, so a broken set is never published as a clean one.
//!
//! Usage:
//!   mercs2_probe gfx-golden <movies_dir> --md <report.md> --json <index.json>

use mercs2_formats::gfx::{tag_name, Features, GfxMovie};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One movie's parsed result (or an error).
struct MovieRow {
    name: String,
    rel: String,
    bytes: usize,
    magic: String,
    stage: [f32; 2],
    frames: u16,
    tag_total: usize,
    feat: Features,
    error: Option<String>,
}

pub fn gfx_golden(dir: &Path, md_out: &Path, json_out: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }

    let mut files = Vec::new();
    collect_gfx(dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!("no .gfx/.swf files under {}", dir.display()));
    }
    eprintln!("gfx-golden: {} movies under {}", files.len(), dir.display());

    let mut rows: Vec<MovieRow> = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(dir)
            .map_err(|e| format!("{} is not under {}: {e}", path.display(), dir.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .ok_or_else(|| format!("{} has no file name", path.display()))?;
        let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        match GfxMovie::parse(&data) {
            Ok(m) => {
                let feat = m.features();
                rows.push(MovieRow {
                    name,
                    rel,
                    bytes: data.len(),
                    magic: String::from_utf8_lossy(&m.magic).into_owned(),
                    stage: m.stage_px(),
                    frames: m.frame_count,
                    tag_total: m.tags.len(),
                    feat,
                    error: None,
                });
            }
            Err(e) => rows.push(err_row(&name, &rel, data.len(), e)),
        }
    }

    let md = render_markdown(&rows, dir);
    let json = render_json(&rows);
    write_out(md_out, &md)?;
    write_out(json_out, &json)?;
    print_summary(&rows);
    eprintln!("gfx-golden: report -> {}", md_out.display());
    eprintln!("gfx-golden: index  -> {}", json_out.display());

    let failed: Vec<&str> = rows.iter().filter(|r| r.error.is_some()).map(|r| r.rel.as_str()).collect();
    if !failed.is_empty() {
        return Err(format!("{} of {} movies did not parse: {}", failed.len(), rows.len(), failed.join(", ")));
    }
    Ok(())
}

fn err_row(name: &str, rel: &str, bytes: usize, e: String) -> MovieRow {
    MovieRow {
        name: name.into(),
        rel: rel.into(),
        bytes,
        magic: "?".into(),
        stage: [0.0, 0.0],
        frames: 0,
        tag_total: 0,
        feat: Features::default(),
        error: Some(e),
    }
}

/// Recursively collect `.gfx`/`.swf` files. An unreadable directory or entry is an error.
fn collect_gfx(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read dir {}: {e}", dir.display()))?;
    for e in entries {
        let p = e.map_err(|e| format!("read dir {}: {e}", dir.display()))?.path();
        if p.is_dir() {
            collect_gfx(&p, out)?;
        } else if matches!(
            p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .as_deref(),
            Some("gfx") | Some("swf")
        ) {
            out.push(p);
        }
    }
    Ok(())
}

// -- feature descriptors (the golden-index axes) --------------------------------------------------

/// `(key, human label, extractor)` — the authorable features the golden set indexes.
type FeatAxis = (&'static str, &'static str, fn(&Features) -> u32);
const AXES: &[FeatAxis] = &[
    (
        "gradient_fill",
        "Gradient fill (shape fill 0x10/0x12/0x13)",
        |f| f.shapes_with_gradient,
    ),
    (
        "focal_gradient",
        "Focal-radial gradient (Flash 8, 0x13)",
        |f| f.shapes_with_focal_gradient,
    ),
    (
        "bitmap_fill",
        "Bitmap fill (shape fill 0x40–0x43, texture-backed)",
        |f| f.shapes_with_bitmap,
    ),
    (
        "embedded_bitmap",
        "Embedded bitmap tag (DefineBits*)",
        |f| f.embedded_bitmaps,
    ),
    ("embedded_font", "Embedded font (DefineFont/2/3)", |f| {
        f.embedded_fonts
    }),
    (
        "import_assets",
        "Imported symbols (ImportAssets/2 — shared-font-lib path)",
        |f| f.imports,
    ),
    ("export_assets", "Exported symbols (ExportAssets)", |f| {
        f.exports
    }),
    ("edit_text", "Dynamic text field (DefineEditText)", |f| {
        f.edit_texts
    }),
    ("button", "Button (DefineButton/2)", |f| f.buttons),
    ("sprite", "Sprite / MovieClip (DefineSprite)", |f| f.sprites),
    ("morph_shape", "Morph shape (DefineMorphShape/2)", |f| {
        f.morph_shapes
    }),
    ("video", "Video (DefineVideoStream)", |f| f.videos),
    (
        "do_action",
        "AVM1/AS2 script block (DoAction/DoInitAction)",
        |f| f.do_action,
    ),
    (
        "do_abc",
        "AS3 bytecode (DoABC) — expected 0 for GFx 2.x",
        |f| f.do_abc,
    ),
    ("gfx_ext_tag", "GFx-extension tag (code ≥ 1000)", |f| {
        f.gfx_ext_tags
    }),
];

// -- rendering ------------------------------------------------------------------------------------

fn print_summary(rows: &[MovieRow]) {
    let ok = rows.iter().filter(|r| r.error.is_none()).count();
    println!("gfx-golden: parsed {ok}/{} movies", rows.len());
    for (key, label, get) in AXES {
        let movies: Vec<&MovieRow> = rows
            .iter()
            .filter(|r| r.error.is_none() && get(&r.feat) > 0)
            .collect();
        let example = movies
            .iter()
            .max_by_key(|r| get(&r.feat))
            .map(|r| format!("{} (×{})", r.name, get(&r.feat)))
            .unwrap_or_else(|| "— NONE in the retail set".into());
        println!(
            "  {:<16} {:>3} movies  golden: {label} = {example}",
            key,
            movies.len()
        );
    }
}

fn render_markdown(rows: &[MovieRow], dir: &Path) -> String {
    let mut s = String::new();
    let ok = rows.iter().filter(|r| r.error.is_none()).count();
    s.push_str("# GFx golden reference set — retail movie feature inventory\n\n");
    s.push_str(&format!(
        "**Generated by** `mercs2_probe gfx-golden` from **{}** movies under `{}` \
         ({ok} parsed). This is the authoritative record of which SWF/GFx features the shipping \
         Mercenaries 2 (**Scaleform GFx 2.0.48 / AVM1**) movies actually use — the golden set an \
         authoring tool validates against. Each feature below names the real movie to byte-diff a \
         new emitter against.\n\n",
        rows.len(),
        dir.display().to_string().replace('\\', "/"),
    ));
    s.push_str("> Method: parse the container (CFX=zlib) + top-level tag stream exactly; parse \
                shape bodies as far as the fill-style array (enough to flag gradient/bitmap fills). \
                Nothing about the GFx tag set is assumed — the tables are discovered from the \
                movies. See the companion spec `gfx_authoring_feature_spec.md` for the encodings.\n\n");

    // 1. Feature -> golden examples.
    s.push_str("## 1. Feature → golden example (what to author against)\n\n");
    s.push_str("| Feature | # movies | Golden example(s) — `movie (count)` |\n|---|---:|---|\n");
    for (_key, label, get) in AXES {
        let mut movies: Vec<(&str, u32)> = rows
            .iter()
            .filter(|r| r.error.is_none())
            .map(|r| (r.name.as_str(), get(&r.feat)))
            .filter(|(_, c)| *c > 0)
            .collect();
        movies.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let examples = if movies.is_empty() {
            "— **not used by any retail movie**".to_string()
        } else {
            movies
                .iter()
                .take(3)
                .map(|(n, c)| format!("`{n}` ({c})"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        s.push_str(&format!("| {label} | {} | {examples} |\n", movies.len()));
    }

    // 2. Tag support surface.
    s.push_str("\n## 2. Tag support surface (every tag the retail set uses)\n\n");
    let mut tag_total: BTreeMap<u16, (u32, u32)> = BTreeMap::new(); // code -> (occurrences, movies)
    for r in rows.iter().filter(|r| r.error.is_none()) {
        for (&code, &n) in &r.feat.tag_counts {
            let e = tag_total.entry(code).or_insert((0, 0));
            e.0 += n;
            e.1 += 1;
        }
    }
    s.push_str("| Tag | Code | Occurrences | Movies |\n|---|---:|---:|---:|\n");
    for (code, (occ, movies)) in &tag_total {
        s.push_str(&format!(
            "| {}{} | {code} | {occ} | {movies} |\n",
            tag_name(*code),
            if tag_name(*code) == "?" {
                " (unrecognised)"
            } else {
                ""
            },
        ));
    }

    // 3. Per-movie inventory.
    s.push_str("\n## 3. Per-movie inventory\n\n");
    s.push_str("| Movie | Form | Stage px | Frames | Tags | Shapes (grad/bmp) | Fonts (emb/imp) | Bmp | Sprite | Btn | Text | AS | GFx-ext | Notes |\n");
    s.push_str("|---|---|---|---:|---:|---|---|---:|---:|---:|---:|---:|---:|---|\n");
    for r in rows {
        if let Some(e) = &r.error {
            s.push_str(&format!(
                "| `{}` | — | — | — | — | — | — | — | — | — | — | — | — | ⚠ parse: {e} |\n",
                r.rel
            ));
            continue;
        }
        let f = &r.feat;
        let note = {
            let mut n = Vec::new();
            if f.do_abc > 0 {
                n.push("AS3!".to_string());
            }
            if f.shapes_parse_incomplete > 0 {
                n.push(format!("{} shape(s) partial", f.shapes_parse_incomplete));
            }
            if f.videos > 0 {
                n.push("video".to_string());
            }
            if f.morph_shapes > 0 {
                n.push("morph".to_string());
            }
            n.join("; ")
        };
        s.push_str(&format!(
            "| `{}` | {} | {:.0}×{:.0} | {} | {} | {}/{}/{} | {}/{} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.rel,
            r.magic,
            r.stage[0],
            r.stage[1],
            r.frames,
            r.tag_total,
            f.shapes,
            f.shapes_with_gradient,
            f.shapes_with_bitmap,
            f.embedded_fonts,
            f.imports,
            f.embedded_bitmaps,
            f.sprites,
            f.buttons,
            f.edit_texts,
            f.do_action,
            f.gfx_ext_tags,
            note,
        ));
    }
    s.push('\n');
    s
}

/// Hand-rolled JSON (no serde dep). A golden-test harness loads this to know which movie exercises
/// each feature and the exact tag histogram per movie.
fn render_json(rows: &[MovieRow]) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut s = String::from("{\n  \"movies\": [\n");
    let mut first = true;
    for r in rows {
        if !first {
            s.push_str(",\n");
        }
        first = false;
        if let Some(e) = &r.error {
            s.push_str(&format!(
                "    {{ \"name\": \"{}\", \"rel\": \"{}\", \"error\": \"{}\" }}",
                esc(&r.name),
                esc(&r.rel),
                esc(e)
            ));
            continue;
        }
        let f = &r.feat;
        let tags: Vec<String> = f
            .tag_counts
            .iter()
            .map(|(c, n)| format!("\"{c}\": {n}"))
            .collect();
        s.push_str(&format!(
            "    {{ \"name\": \"{}\", \"rel\": \"{}\", \"bytes\": {}, \"form\": \"{}\", \
             \"stage\": [{:.1}, {:.1}], \"frames\": {}, \"tag_total\": {}, \
             \"features\": {{ \"shapes\": {}, \"gradient_fill\": {}, \"focal_gradient\": {}, \
             \"bitmap_fill\": {}, \"embedded_bitmap\": {}, \"embedded_font\": {}, \"import_assets\": {}, \
             \"export_assets\": {}, \"edit_text\": {}, \"button\": {}, \"sprite\": {}, \
             \"morph_shape\": {}, \"video\": {}, \"do_action\": {}, \"do_abc\": {}, \
             \"gfx_ext_tag\": {}, \"shapes_partial\": {} }}, \
             \"tags\": {{ {} }} }}",
            esc(&r.name), esc(&r.rel), r.bytes, esc(&r.magic),
            r.stage[0], r.stage[1], r.frames, r.tag_total,
            f.shapes, f.shapes_with_gradient, f.shapes_with_focal_gradient,
            f.shapes_with_bitmap, f.embedded_bitmaps, f.embedded_fonts, f.imports,
            f.exports, f.edit_texts, f.buttons, f.sprites,
            f.morph_shapes, f.videos, f.do_action, f.do_abc,
            f.gfx_ext_tags, f.shapes_parse_incomplete,
            tags.join(", "),
        ));
    }
    s.push_str("\n  ]\n}\n");
    s
}

fn write_out(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, content).map_err(|e| format!("write {}: {e}", path.display()))
}
