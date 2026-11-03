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
use mercs2_formats::gfx_def::{Character, MovieDef};
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

/// `gfx-decode`: decode every movie under `dir` to its [`MovieDef`]; any failure fails the run.
pub fn gfx_decode(dir: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    collect_gfx(dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!("no .gfx/.swf files under {}", dir.display()));
    }
    let mut failed = 0;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for path in &files {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        match GfxMovie::parse(&bytes).and_then(|movie| MovieDef::decode(&movie)) {
            Ok(def) => {
                for c in def.characters.values() {
                    let kind = match c {
                        Character::Shape(_) => "shape",
                        Character::Morph(_) => "morph",
                        Character::Sprite(_) => "sprite",
                        Character::EditText(_) => "edit text",
                        Character::Font(_) => "font",
                        Character::Image(_) => "image",
                    };
                    *counts.entry(kind).or_default() += 1;
                }
            }
            Err(e) => {
                failed += 1;
                println!("{}: {e}", path.display());
            }
        }
    }
    println!("{} movies, {failed} failed, characters {counts:?}", files.len());
    if failed != 0 {
        return Err(format!("{failed} movies did not decode"));
    }
    Ok(())
}

/// `gfx-run`: play every movie under `dir` for `frames` frames, then report script failures and the
/// library calls the scripts reached for that do not exist.
pub fn gfx_run(dir: &Path, frames: u32) -> Result<(), String> {
    use mercs2_engine::gfx::avm::Player;
    use mercs2_engine::gfx::stage::Movie;

    let mut files = Vec::new();
    collect_gfx(dir, &mut files)?;
    files.sort();
    let mut failed = 0;
    let mut commands: BTreeMap<String, usize> = BTreeMap::new();
    let mut missing: BTreeMap<String, usize> = BTreeMap::new();
    for path in &files {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let def = std::rc::Rc::new(GfxMovie::parse(&bytes).and_then(|m| MovieDef::decode(&m))?);
        let frame_ms = 1000.0 / def.frame_rate.max(1.0) as f64;
        let run = (|| -> Result<(Vec<(String, String)>, Vec<String>), String> {
            let mut player = Player::new(Movie::new(def.clone())?)?;
            for _ in 1..frames {
                player.advance(frame_ms)?;
            }
            player.movie.draw()?;
            Ok((player.fscommands.clone(), player.missing.iter().cloned().collect()))
        })();
        match run {
            Ok((fs, gaps)) => {
                for (c, _) in fs {
                    *commands.entry(c).or_default() += 1;
                }
                for g in gaps {
                    *missing.entry(g).or_default() += 1;
                }
            }
            Err(e) => {
                failed += 1;
                println!("{}: {e}", path.display());
            }
        }
    }
    println!("{} movies, {failed} failed", files.len());
    println!("fscommands {commands:?}");
    for (name, count) in &missing {
        println!("missing in {count:>3}: {name}");
    }
    if failed != 0 {
        return Err(format!("{failed} movies failed"));
    }
    Ok(())
}

/// `gfx-render`: play `movie` for `frames` frames and write the last as a PNG. Images come from `wads`.
pub fn gfx_render(movie: &Path, frames: u32, png_path: &Path, wads: &[String], fonts: Option<&Path>, label: Option<&str>) -> Result<(), String> {
    use mercs2_engine::gfx::avm::{Player, Value};
    use mercs2_engine::gfx::stage::Movie;
    use mercs2_engine::gfx::text::FontLib;
    use mercs2_engine::gfx_pass::{render_to_rgba, WadImages};

    let load = |path: &Path| -> Result<MovieDef, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        GfxMovie::parse(&bytes).and_then(|m| MovieDef::decode(&m))
    };
    let def = std::rc::Rc::new(load(movie)?);
    let mut images = WadImages::open(wads)?;
    let mut stage = Movie::new(def.clone())?;
    if let Some(path) = fonts {
        stage.font_lib = Some(std::rc::Rc::new(FontLib::from_movie(&load(path)?)?));
    }
    stage.strings = Some(std::rc::Rc::new(images.strings("english")?));
    let mut player = Player::new(stage)?;
    let frame_ms = 1000.0 / def.frame_rate.max(1.0) as f64;
    for i in 1..frames {
        player.advance(frame_ms)?;
        if let (Some(label), 4) = (label, i) {
            player.invoke("gotoAndPlay", &[Value::Str(label.into())])?;
        }
    }
    println!("traces {:?}", player.traces);
    println!("fscommands {:?}", player.fscommands);
    println!("missing {:?}", player.missing);
    let items = player.movie.draw()?;
    for item in &items {
        if let mercs2_engine::gfx::stage::Item::Draw(d) = item {
            for part in d.parts.iter() {
                let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                for [x, y] in &part.vertices {
                    let p = d.matrix.apply(*x, *y);
                    for k in 0..2 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                let paint = match &part.paint {
                    mercs2_engine::gfx::tess::Paint::Solid(c) => format!("solid {c:?}"),
                    mercs2_engine::gfx::tess::Paint::Gradient { kind, .. } => format!("gradient {kind:?}"),
                    mercs2_engine::gfx::tess::Paint::Bitmap { id, .. } => format!("bitmap {id}"),
                    mercs2_engine::gfx::tess::Paint::Glyphs { atlas, color, .. } => format!("text {atlas} {color:?}"),
                };
                println!("{lo:.0?}..{hi:.0?} {paint} blend {} mul {:.2?} add {:.2?}", d.blend, d.cxform.mul, d.cxform.add);
            }
        } else {
            println!("{}", match item {
                mercs2_engine::gfx::stage::Item::MaskBegin => "mask begin",
                mercs2_engine::gfx::stage::Item::MaskEnd => "mask end",
                _ => "mask pop",
            });
        }
    }
    let size = [(def.stage[1] - def.stage[0]) as u32 / 20, (def.stage[3] - def.stage[2]) as u32 / 20];
    // The movie's own background is not painted: `MrxGuiShell` puts a Bink widget behind the shell's
    // Flash widget, which could not show through it.
    let count = items.len();
    let draw = mercs2_engine::flash::FlashDraw { def, items };
    let layers = [mercs2_engine::scene::FrontLayer::Flash(0)];
    let rgba = render_to_rgba(std::slice::from_ref(&draw), &mut images, &[], &layers, size, [0.0, 0.0, 0.0, 1.0])?;
    write_png(png_path, size, &rgba)?;
    println!("{}: {count} items, {}x{}", png_path.display(), size[0], size[1]);
    Ok(())
}

/// Print a block of action bytes, one action a line, with pool strings written in.
fn print_actions(code: &[u8], out: &mut String) {
    fn zstr(b: &[u8]) -> (String, usize) {
        let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
        (String::from_utf8_lossy(&b[..end]).into_owned(), end + 1)
    }
    let op_name = |op: u8| match op {
        0x04 => "NextFrame", 0x05 => "PrevFrame", 0x06 => "Play", 0x07 => "Stop", 0x0A => "Add", 0x0B => "Subtract",
        0x0C => "Multiply", 0x0D => "Divide", 0x0E => "Equals", 0x0F => "Less", 0x10 => "And", 0x11 => "Or", 0x12 => "Not",
        0x17 => "Pop", 0x18 => "ToInteger", 0x1C => "GetVariable", 0x1D => "SetVariable", 0x20 => "SetTarget2",
        0x21 => "StringAdd", 0x22 => "GetProperty", 0x23 => "SetProperty", 0x26 => "Trace", 0x30 => "RandomNumber",
        0x3A => "Delete", 0x3B => "Delete2", 0x3C => "DefineLocal", 0x3D => "CallFunction", 0x3E => "Return", 0x3F => "Modulo",
        0x40 => "NewObject", 0x41 => "DefineLocal2", 0x42 => "InitArray", 0x43 => "InitObject", 0x44 => "TypeOf",
        0x46 => "Enumerate", 0x47 => "Add2", 0x48 => "Less2", 0x49 => "Equals2", 0x4A => "ToNumber", 0x4B => "ToString",
        0x4C => "PushDuplicate", 0x4E => "GetMember", 0x4F => "SetMember", 0x50 => "Increment", 0x51 => "Decrement",
        0x52 => "CallMethod", 0x53 => "NewMethod", 0x60 => "BitAnd", 0x61 => "BitOr", 0x62 => "BitXor", 0x66 => "StrictEquals",
        0x67 => "Greater", 0x81 => "GotoFrame", 0x83 => "GetURL", 0x87 => "StoreRegister", 0x88 => "ConstantPool",
        0x8C => "GotoLabel", 0x8E => "DefineFunction2", 0x94 => "With", 0x96 => "Push", 0x99 => "Jump", 0x9A => "GetURL2",
        0x9B => "DefineFunction", 0x9D => "If", 0x9E => "Call", 0x9F => "GotoFrame2",
        _ => "?",
    };
    let mut pool: Vec<String> = Vec::new();
    let mut at = 0;
    while at < code.len() {
        let op = code[at];
        at += 1;
        if op == 0 {
            out.push_str("  End\n");
            continue;
        }
        let mut body: &[u8] = &[];
        if op >= 0x80 {
            let Some(len) = code.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) else { return };
            at += 2;
            body = code.get(at..at + len).unwrap_or(&[]);
            at += len;
        }
        let arg = match op {
            0x88 => {
                pool.clear();
                let mut p = 2;
                for _ in 0..u16::from_le_bytes([body[0], body[1]]) {
                    let (s, n) = zstr(&body[p..]);
                    p += n;
                    pool.push(s);
                }
                format!("{} strings", pool.len())
            }
            0x96 => {
                let mut p = 0;
                let mut parts = Vec::new();
                while p < body.len() {
                    let kind = body[p];
                    p += 1;
                    match kind {
                        0 => {
                            let (s, n) = zstr(&body[p..]);
                            p += n;
                            parts.push(format!("{s:?}"));
                        }
                        1 => {
                            parts.push(f32::from_le_bytes(body[p..p + 4].try_into().unwrap_or_default()).to_string());
                            p += 4;
                        }
                        2 => parts.push("null".into()),
                        3 => parts.push("undefined".into()),
                        4 => {
                            parts.push(format!("r{}", body[p]));
                            p += 1;
                        }
                        5 => {
                            parts.push((body[p] != 0).to_string());
                            p += 1;
                        }
                        6 => {
                            let b = &body[p..p + 8];
                            parts.push(f64::from_le_bytes([b[4], b[5], b[6], b[7], b[0], b[1], b[2], b[3]]).to_string());
                            p += 8;
                        }
                        7 => {
                            parts.push(i32::from_le_bytes(body[p..p + 4].try_into().unwrap_or_default()).to_string());
                            p += 4;
                        }
                        8 => {
                            parts.push(format!("{:?}", pool.get(body[p] as usize).map_or("?", |s| s)));
                            p += 1;
                        }
                        9 => {
                            parts.push(format!("{:?}", pool.get(u16::from_le_bytes([body[p], body[p + 1]]) as usize).map_or("?", |s| s)));
                            p += 2;
                        }
                        other => {
                            parts.push(format!("<kind {other}>"));
                            break;
                        }
                    }
                }
                parts.join(", ")
            }
            0x8C => zstr(body).0,
            0x9B | 0x8E => format!("FUNCTION {}", zstr(body).0),
            0x83 => {
                let (url, n) = zstr(body);
                format!("{url} {}", zstr(&body[n..]).0)
            }
            0x99 | 0x9D => i16::from_le_bytes([body[0], body[1]]).to_string(),
            0x81 => u16::from_le_bytes([body[0], body[1]]).to_string(),
            0x87 => format!("r{}", body[0]),
            _ => String::new(),
        };
        out.push_str(&format!("  {} {arg}\n", op_name(op)));
    }
}

/// Write every action block of a movie as text, each headed by where it lives.
pub fn gfx_actions(movie: &Path, out_path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(movie).map_err(|e| format!("read {}: {e}", movie.display()))?;
    let def = GfxMovie::parse(&bytes).and_then(|m| MovieDef::decode(&m))?;
    let mut out = String::new();
    let mut timeline = |name: String, t: &mercs2_formats::gfx_def::Timeline| {
        for (index, frame) in t.frames.iter().enumerate() {
            for control in frame {
                match control {
                    mercs2_formats::gfx_def::Control::Action(code) => {
                        out.push_str(&format!("== {name} frame {index}\n"));
                        print_actions(code, &mut out);
                    }
                    mercs2_formats::gfx_def::Control::Place(place) => {
                        for action in &place.clip_actions {
                            out.push_str(&format!("== {name} frame {index} depth {} clip event {:#x}\n", place.depth, action.events));
                            print_actions(&action.code, &mut out);
                        }
                    }
                    mercs2_formats::gfx_def::Control::Remove { .. } => {}
                }
            }
        }
    };
    timeline("root".into(), &def.timeline);
    let mut ids: Vec<u16> = def.characters.keys().copied().collect();
    ids.sort();
    for id in ids {
        if let Some(Character::Sprite(sprite)) = def.characters.get(&id) {
            timeline(format!("sprite {id}"), sprite);
        }
    }
    std::fs::write(out_path, out).map_err(|e| format!("write {}: {e}", out_path.display()))
}

/// Write a movie's display lists as text: every timeline's frames with their places (depth, move or
/// new, character, name, ratio, clip depth), removes, scripts and labels.
pub fn gfx_tags(movie: &Path, out_path: &Path) -> Result<(), String> {
    use mercs2_formats::gfx_def::{Control, Timeline};
    let bytes = std::fs::read(movie).map_err(|e| format!("read {}: {e}", movie.display()))?;
    let def = GfxMovie::parse(&bytes).and_then(|m| MovieDef::decode(&m))?;
    let mut out = String::new();
    let mut timeline = |name: String, t: &Timeline| {
        out.push_str(&format!("== {name}: {} frames (declares {})\n", t.frames.len(), t.declared_frames));
        for (label, frame) in &t.labels {
            out.push_str(&format!("  label {label:?} -> frame {frame}\n"));
        }
        for (index, frame) in t.frames.iter().enumerate() {
            for control in frame {
                match control {
                    Control::Place(p) => out.push_str(&format!(
                        "  frame {index}: {} depth {} character {:?} name {:?} ratio {:?} clip_depth {:?} events {}\n",
                        if p.is_move { "move" } else { "place" },
                        p.depth,
                        p.character,
                        p.name,
                        p.ratio,
                        p.clip_depth,
                        p.clip_actions.len()
                    )),
                    Control::Remove { depth } => out.push_str(&format!("  frame {index}: remove depth {depth}\n")),
                    Control::Action(code) => out.push_str(&format!("  frame {index}: script ({} bytes)\n", code.len())),
                }
            }
        }
    };
    timeline("root".into(), &def.timeline);
    let mut ids: Vec<u16> = def.characters.keys().copied().collect();
    ids.sort();
    for id in ids {
        if let Some(Character::Sprite(sprite)) = def.characters.get(&id) {
            timeline(format!("sprite {id}"), sprite);
        }
    }
    std::fs::write(out_path, out).map_err(|e| format!("write {}: {e}", out_path.display()))
}

/// Decode one audio track of a movie to 16-bit PCM and, given another decode of it (ffmpeg's, as a
/// black box), say how far apart they are.
pub fn bink_audio(movie: &Path, track: usize, out: &Path, against: Option<&Path>) -> Result<(), String> {
    let bytes = std::fs::read(movie).map_err(|e| format!("read {}: {e}", movie.display()))?;
    let m = mercs2_bink::Movie::parse(&bytes)?;
    let info = *m.tracks.get(track).ok_or_else(|| format!("track {track} of {}", m.tracks.len()))?;
    let mut decoder = mercs2_bink::AudioDecoder::new(&info)?;
    let mut pcm: Vec<i16> = Vec::new();
    for frame in 0..m.frame_count {
        let (records, _) = m.split(frame)?;
        pcm.extend(decoder.decode_record(records[track]).map_err(|e| format!("frame {frame}: {e}"))?);
    }
    let raw: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(out, &raw).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("track {track} id {} {} Hz {} ch: {} samples -> {}", info.id, info.rate(), info.channels(), pcm.len(), out.display());
    if let Some(path) = against {
        let other: Vec<i16> = std::fs::read(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        let common = pcm.len().min(other.len());
        let diffs: Vec<i32> = (0..common).map(|i| (pcm[i] as i32 - other[i] as i32).abs()).collect();
        let max = diffs.iter().copied().max().unwrap_or(0);
        let differing = diffs.iter().filter(|d| **d != 0).count();
        println!("against {}: {} samples there, {common} compared, {differing} differ, max |difference| {max}", path.display(), other.len());
    }
    Ok(())
}

/// Decode the first `frames` video frames of a movie and compare each frame's Y, U and V with the
/// matching frame of a raw `yuv420p` file (ffmpeg's decode, as a black box). Writes nothing; fails
/// when any plane differs.
pub fn bink_video(movie: &Path, frames: u32, against: &Path) -> Result<(), String> {
    use std::io::Read;
    let bytes = std::fs::read(movie).map_err(|e| format!("read {}: {e}", movie.display()))?;
    let m = mercs2_bink::Movie::parse(&bytes)?;
    if frames > m.frame_count {
        return Err(format!("{frames} frames asked for, the movie has {}", m.frame_count));
    }
    let (w, h) = (m.width as usize, m.height as usize);
    let chroma = w.div_ceil(2) * h.div_ceil(2);
    let frame_size = w * h + 2 * chroma;
    let reference_len = std::fs::metadata(against).map_err(|e| format!("stat {}: {e}", against.display()))?.len();
    if reference_len < frames as u64 * frame_size as u64 {
        return Err(format!("{} holds {reference_len} bytes, fewer than {frames} frames of {frame_size}", against.display()));
    }
    let mut reference = std::io::BufReader::new(std::fs::File::open(against).map_err(|e| format!("open {}: {e}", against.display()))?);
    let mut expected = vec![0u8; frame_size];
    let mut decoder = mercs2_bink::VideoDecoder::new(&m)?;
    let mut decode_time = std::time::Duration::ZERO;
    let mut first_diff: Option<String> = None;
    let mut differing = 0u32;
    for i in 0..frames {
        reference.read_exact(&mut expected).map_err(|e| format!("read frame {i} of {}: {e}", against.display()))?;
        let started = std::time::Instant::now();
        let frame = decoder.decode(&m, i)?;
        decode_time += started.elapsed();
        let planes = [("Y", &frame.y, &expected[..w * h]), ("U", &frame.u, &expected[w * h..w * h + chroma]), ("V", &frame.v, &expected[w * h + chroma..])];
        let mut line = format!("frame {i}:");
        let mut equal = true;
        for (name, ours, theirs) in planes {
            match ours.iter().zip(theirs.iter()).position(|(a, b)| a != b) {
                None => line.push_str(&format!(" {name} equal")),
                Some(at) => {
                    equal = false;
                    let differ = ours.iter().zip(theirs.iter()).filter(|(a, b)| a != b).count();
                    line.push_str(&format!(" {name} DIFFERS ({differ} bytes, first at offset {at}: ours {} reference {})", ours[at], theirs[at]));
                    if first_diff.is_none() {
                        let stride = if name == "Y" { w } else { w.div_ceil(2) };
                        first_diff = Some(format!("frame {i} plane {name} offset {at} (row {}, column {})", at / stride, at % stride));
                    }
                }
            }
        }
        if !equal {
            differing += 1;
        }
        println!("{line}");
    }
    let secs = decode_time.as_secs_f64();
    println!(
        "{frames} frames {w}x{h} decoded in {secs:.3} s ({:.1} frames/s; movie rate {:.3} frames/s); {differing} frames differ",
        frames as f64 / secs,
        m.fps.0 as f64 / m.fps.1 as f64
    );
    match first_diff {
        None => Ok(()),
        Some(at) => Err(format!("{differing} of {frames} frames differ; first difference: {at}")),
    }
}

/// Decode a movie up to `index` and write that frame as RGBA, converted as the movie widget converts
/// it; prints the mean time a frame takes to decode and to convert.
pub fn bink_frame(movie: &Path, index: u32, out: &Path) -> Result<(), String> {
    let bytes = std::fs::read(movie).map_err(|e| format!("read {}: {e}", movie.display()))?;
    let m = mercs2_bink::Movie::parse(&bytes)?;
    if index >= m.frame_count {
        return Err(format!("frame {index} asked for, the movie has {}", m.frame_count));
    }
    let mut decoder = mercs2_bink::VideoDecoder::new(&m)?;
    let (mut decode, mut convert) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
    let mut rgba = Vec::new();
    for i in 0..=index {
        let started = std::time::Instant::now();
        let frame = decoder.decode(&m, i)?;
        decode += started.elapsed();
        let started = std::time::Instant::now();
        rgba = mercs2_bink::rgb::to_rgba(frame);
        convert += started.elapsed();
    }
    let n = (index + 1) as f64;
    println!(
        "{} frames {}x{}: decode {:.3} ms/frame, convert {:.3} ms/frame",
        index + 1,
        m.width,
        m.height,
        decode.as_secs_f64() * 1000.0 / n,
        convert.as_secs_f64() * 1000.0 / n
    );
    write_png(out, [m.width, m.height], &rgba)
}

fn write_png(path: &Path, size: [u32; 2], rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size[0], size[1]);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut w| w.write_image_data(rgba)).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Boot the shipped shell headless on a `screen` of that many pixels and walk it: each step is
/// `wait:<frames>`, `down:<button>`, `up:<button>`, `mouse:<x>,<y>,<buttons>` or `call:<name>,<args>`,
/// at 60 frames a second. The final frame goes to `png_path`.
pub fn gfx_shell(png_path: &Path, screen: [u32; 2], steps: &[String], wads: &[String]) -> Result<(), String> {
    use mercs2_engine::gfx_pass::render_to_rgba;
    use mercs2_engine::shell::Shell;

    const DT: f32 = 1.0 / 60.0;
    let saves = vec![mercs2_engine::shell::ShellSave {
        path: PathBuf::from("probe.profile"),
        date: "05:41:11 | 07/23/2008".into(),
        character: 1,
        cash: 58174568,
        fuel: 240,
        play_time: "13:33:33".into(),
        contract: "PirCon003".into(),
    }];
    let base = wads.last().ok_or("no WAD to boot the shell from")?;
    let mut shell = Shell::open(wads, saves, base)?;
    let screen_px = [screen[0] as f32, screen[1] as f32];
    // No window here: a stand-in display with the common 4:3 and 16:9 modes and the screen's.
    let modes = [(640, 480, 60), (800, 600, 60), (1024, 768, 60), (1280, 720, 60), (1280, 960, 60), (1600, 900, 60), (1920, 1080, 60), (screen[0], screen[1], 60)];
    let device = mercs2_engine::options::Device { vendor: 0, device: 0, aa_levels: 4, high_shaders: true };
    println!("stand-in display: modes {modes:?}, adapter {device:?}");
    shell.attach_display(&modes, device)?;
    let mut frame = 0u32;
    let mut pointer = mercs2_engine::flash::Pointer::default();
    // The stand-in pad's inputs held each frame, by joystick vocabulary index.
    let mut held: Vec<u8> = Vec::new();
    for step in steps {
        let (verb, value) = step.split_once(':').ok_or_else(|| format!("step {step:?} is not verb:value"))?;
        let number = || value.parse::<i64>().map_err(|e| format!("step {step:?}: {e}"));
        let frames = match verb {
            "wait" => number()?,
            "down" | "up" => {
                shell.button(number()?, verb == "down")?;
                1
            }
            // `mouse:<x>,<y>,<buttons>`: the pointer across the viewport, 0..1 each way.
            "mouse" => {
                let parts: Vec<&str> = value.split(',').collect();
                let [x, y, b] = parts[..] else { return Err(format!("step {step:?} is not mouse:x,y,buttons")) };
                let f = |s: &str| s.parse::<f32>().map_err(|e| format!("step {step:?}: {e}"));
                pointer = mercs2_engine::flash::Pointer { at: [f(x)?, f(y)?], buttons: b.parse().map_err(|e| format!("step {step:?}: {e}"))? };
                1
            }
            "call" => {
                let mut parts = value.split(',');
                let name = parts.next().ok_or("a call with no name")?;
                let args = parts.map(|a| a.parse::<f64>().map_err(|e| format!("step {step:?}: {e}"))).collect::<Result<Vec<_>, _>>()?;
                println!("{value} -> {}", shell.call_movies(name, &args)?);
                1
            }
            // `key:<id>`: a key or mouse button down by its id in retail's key table (hex with `0x`).
            "key" => {
                let id = match value.strip_prefix("0x") {
                    Some(hex) => u16::from_str_radix(hex, 16),
                    None => value.parse::<u16>(),
                }
                .map_err(|e| format!("step {step:?}: {e}"))?;
                println!("key {id:#x} taken by a rebind: {}", shell.key_down(id)?);
                1
            }
            // `pad:<vendor>,<product>,<name>`: a stand-in pad attached (ids in hex), its section `<name>`.
            "pad" => {
                let mut parts = value.splitn(3, ',');
                let hex = |s: Option<&str>| u16::from_str_radix(s.ok_or(format!("step {step:?} is not pad:vendor,product,name"))?, 16).map_err(|e| format!("step {step:?}: {e}"));
                let pad = mercs2_engine::controls::Pad { vendor: hex(parts.next())?, product: hex(parts.next())?, name: parts.next().ok_or(format!("step {step:?} has no name"))?.to_string() };
                shell.attach_pads(&[pad])?;
                0
            }
            // `held:<input>,...`: the stand-in pad's inputs held from now on (empty for none).
            "held" => {
                held = value.split(',').filter(|s| !s.is_empty()).map(|s| s.parse::<u8>().map_err(|e| format!("step {step:?}: {e}"))).collect::<Result<_, _>>()?;
                1
            }
            // `get:<path>`: print a value of every live movie, read by dotted path from its root.
            "get" => {
                println!("{value} = {}", shell.get_movies(value)?);
                0
            }
            other => return Err(format!("no step verb {other:?}")),
        };
        for _ in 0..frames {
            frame += 1;
            shell.pad_frame(&held)?;
            if let Some(exit) = shell.tick(DT, pointer, screen_px).map_err(|e| format!("frame {frame}: {e}"))? {
                println!("frame {frame}: the shell exits: {exit:?}");
                return Ok(());
            }
        }
        println!("frame {frame}: after {step}: {:?}", shell.describe());
    }
    // What building the draw lists costs a frame, as the game builds them every frame.
    const TIMED: u32 = 30;
    let started = std::time::Instant::now();
    for _ in 0..TIMED {
        shell.draws()?;
    }
    println!("building the draw lists: {:.3} ms/frame", started.elapsed().as_secs_f64() * 1000.0 / TIMED as f64);
    let front = shell.draws()?;
    println!("{} Flash and {} Bink movies on screen", front.flash.len(), front.movies.len());
    if front.layers.is_empty() {
        return Err("no movie on screen".into());
    }
    let rgba = render_to_rgba(&front.flash, front.images, &front.movies, &front.layers, screen, [0.0, 0.0, 0.0, 1.0])?;
    write_png(png_path, screen, &rgba)
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
