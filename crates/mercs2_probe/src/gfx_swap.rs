//! `gfx-swap-constant` and `gfx-patch-actions`: [`mercs2_formats::gfx_actions`] on a movie file.

use mercs2_formats::gfx_actions::{check_decodes, parse_patches, patch_actions, swap_constant, Target};
use std::path::Path;

/// `mercs2_probe menu-items <wad[,wad…]> <out shell-patch.wad> <label>|<text>|<help>|<event>|<after>...`:
/// the player's SHELL with the items added to the main menu (`mercs2_quartermaster::menu_items`) and
/// their labels (`text`, in the stack's english table's language) set into the added label texture, both
/// in a shell patch as qm's link writes one; the edited movie beside it as `<out>.gfx` and the texture as
/// `<out>.png`. An empty help is none. The first WAD of the stack is the one the patch mounts over.
/// `--over <patch.wad>` first: the two blocks merged into that patch instead (a block of the same path
/// replaced), for a test over the shell patch a player has installed.
pub fn run_menu_items(args: &[String]) -> Result<(), String> {
    use mercs2_formats::patch_wad::{build_patch_wad_multi, merge_patch_wads, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
    use mercs2_formats::types::{TYPE_HASH_CFX_PACK, TYPE_HASH_STRINGDB, TYPE_ID_CFX_PACK, TYPE_ID_STRINGDB};
    use mercs2_quartermaster::menu_items::{add_items, label_texture, label_texture_block, page, NewItem};
    let (over, args) = match args {
        [flag, path, rest @ ..] if flag == "--over" => (Some(path), rest),
        _ => (None, args),
    };
    let [stack, output, specs @ ..] = args else {
        return Err("usage: mercs2_probe menu-items [--over <patch.wad>] <wad[,wad…]> <out shell-patch.wad> <label>|<text>|<help>|<event>|<after>...".into());
    };
    let (items, texts): (Vec<NewItem>, Vec<String>) = specs
        .iter()
        .map(|s| match s.split('|').collect::<Vec<_>>()[..] {
            [label, text, help, event, after] => Ok((
                NewItem { label: label.into(), help: (!help.is_empty()).then(|| help.to_string()), event: event.into(), after: after.into() },
                text.to_string(),
            )),
            _ => Err(format!("{s:?} is not <label>|<text>|<help>|<event>|<after>")),
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .unzip();
    let hash = mercs2_formats::hash::pandemic_hash_m2("SHELL");
    let mut game = wad_stack(stack)?;
    let container = game.container_for_asset(hash, TYPE_HASH_CFX_PACK, TYPE_ID_CFX_PACK).ok_or_else(|| format!("{stack} holds no SHELL"))?;
    let retail = mercs2_formats::gfx::cfx_pack_movie(&container, "shell.wad SHELL")?;
    let (edited, art) = add_items(&page("shell.main")?, &retail, &items)?;
    let english = mercs2_formats::hash::pandemic_hash_m2("english");
    let table = game.container_for_asset(english, TYPE_HASH_STRINGDB, TYPE_ID_STRINGDB).ok_or_else(|| format!("{stack} holds no english string table"))?;
    let db = mercs2_formats::stringdb::read_container(&table)?;
    let retail_text = |key: u32| db.entries.iter().find(|e| e.key_hash == key).map(|e| e.text.clone());
    let retail_art = mercs2_quartermaster::label_art::read(&mut game, &art.retail)?;
    let texture = label_texture(&art, &retail_art, &retail_text, &texts)?;
    let block = mercs2_formats::gfx::build_cfx_pack_block(hash, &edited);
    let aset = AsetEntry::new(hash, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_CFX_PACK);
    let movie_block = PatchBlock::from_decompressed(&block, format!("blocks\\VZ\\mod_{hash:08x}.block"), vec![aset], None)?;
    let texture_block = label_texture_block(&art, &texture)?;
    let wad = match over {
        Some(path) => merge_patch_wads(&std::fs::read(path).map_err(|e| format!("{path}: {e}"))?, vec![movie_block, texture_block], true)?,
        None => {
            let base = stack.split(',').next().ok_or("an empty WAD stack")?;
            build_patch_wad_multi(&[movie_block, texture_block], mercs2_formats::donor::base_csum(base)?, &FFCS_CERT_BLOB)?
        }
    };
    std::fs::write(output, &wad).map_err(|e| format!("write {output}: {e}"))?;
    let movie = format!("{output}.gfx");
    std::fs::write(&movie, &edited).map_err(|e| format!("write {movie}: {e}"))?;
    let png = format!("{output}.png");
    let rgba: Vec<u8> = texture.px.iter().flatten().copied().collect();
    crate::gfx_golden::write_png(Path::new(&png), [texture.width as u32, texture.height as u32], &rgba)?;
    println!(
        "SHELL {} -> {} bytes, {} item(s); {} {}x{} from {}; wrote {output}, {movie} and {png}",
        retail.len(),
        edited.len(),
        items.len(),
        art.name,
        art.size[0],
        art.size[1],
        art.retail
    );
    Ok(())
}

/// `mercs2_probe label-art <shell.wad> <texture> <alpha> <dark luma> <bright luma>`: the bands of
/// lettering in a texture and, in each, the column runs of any ink, of dark fill and of bright fill,
/// with the gaps between them and each band's luminance histogram (16 bins, texels above `alpha`).
pub fn run_label_art(args: &[String]) -> Result<(), String> {
    use mercs2_quartermaster::label_art::{bands, columns, fill_columns, luma, read, Fill};
    let [shell, name, alpha, dark, bright] = args else {
        return Err("usage: mercs2_probe label-art <shell.wad> <texture> <alpha> <dark luma> <bright luma>".into());
    };
    let num = |s: &String| s.parse::<u8>().map_err(|e| format!("{s:?}: {e}"));
    let (alpha, dark, bright) = (num(alpha)?, num(dark)?, num(bright)?);
    let mut game = wad_stack(shell)?;
    let img = read(&mut game, name)?;
    println!("{name}: {}x{}", img.width, img.height);
    let show = |what: &str, cols: &[(usize, usize)]| {
        let gaps: Vec<usize> = cols.windows(2).map(|w| w[1].0 - w[0].1).collect();
        println!("  {what}: {} runs {cols:?}", cols.len());
        println!("    gaps {gaps:?}");
    };
    for band in bands(&img, alpha) {
        let mut hist = [0usize; 16];
        for y in band.0..band.1 {
            for x in 0..img.width {
                let t = img.at(x, y);
                if t[3] > alpha {
                    hist[usize::from(luma(t) / 16)] += 1;
                }
            }
        }
        println!("rows {}..{} ({} high): luma histogram {hist:?}", band.0, band.1, band.1 - band.0);
        show("any", &columns(&img, band, alpha));
        show("dark", &fill_columns(&img, band, Fill::Dark { alpha, luma: dark }));
        show("bright", &fill_columns(&img, band, Fill::Bright { alpha, luma: bright }));
    }
    Ok(())
}

/// `mercs2_probe label-parts <shell.wad> <texture> <out_dir> <label>...`: the texture's labels (named in
/// band order) taken apart into letters and parts; each state's outline measured; every label rebuilt
/// from its own parts at their own places with the outline regrown, and compared with retail. Writes
/// `<out_dir>/rebuild.png` (per label: retail dark, rebuilt dark, retail bright, rebuilt bright) and
/// `<out_dir>/parts.png` (each part in its own colour).
pub fn run_label_parts(args: &[String]) -> Result<(), String> {
    use mercs2_quartermaster::label_art::{band_image, compose, difference, lines, measure_outline, read, Image, Placed, State};
    let [shell, name, out_dir, texts @ ..] = args else {
        return Err("usage: mercs2_probe label-parts <shell.wad> <texture> <out_dir> <label>...".into());
    };
    let mut game = wad_stack(shell)?;
    let img = read(&mut game, name)?;
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let lines = lines(&img, &texts)?;
    for line in &lines {
        println!("{} (dark rows {}..{}, bright {}..{}):", line.text, line.dark.0, line.dark.1, line.bright.0, line.bright.1);
        for l in &line.letters {
            let boxes: Vec<String> = l.parts.iter().map(|p| format!("{}x{}@{},{}", p.width, p.height, p.x - l.x0, p.y - line.dark.0)).collect();
            println!("  {} cols {}..{} ({} wide): {} part(s) {}", l.ch, l.x0, l.x1, l.x1 - l.x0, l.parts.len(), boxes.join(" "));
            if std::env::var("LABEL_MASKS").is_ok_and(|v| v.contains(l.ch)) {
                for (i, p) in l.parts.iter().enumerate() {
                    println!("    part {i} mask:");
                    for y in 0..p.height {
                        let row: String = (0..p.width).map(|x| if p.mask[y * p.width + x] { '#' } else { '.' }).collect();
                        println!("      {row}");
                    }
                }
            }
        }
        let gaps: Vec<usize> = line.letters.windows(2).map(|w| w[1].x0 - w[0].x1).collect();
        println!("  gaps between letters {gaps:?}");
    }
    let outlines = [(State::Dark, measure_outline(&img, &lines, State::Dark)), (State::Bright, measure_outline(&img, &lines, State::Bright))];
    for (state, o) in &outlines {
        println!("outline {state:?} by distance: {:?}", o.by_distance);
    }
    let mut sheet: Vec<Image> = Vec::new();
    for line in &lines {
        for (state, outline) in &outlines {
            let band = match state {
                State::Dark => line.dark,
                State::Bright => line.bright,
            };
            let h = band.1 - band.0;
            let placed: Vec<Placed<'_>> = line
                .letters
                .iter()
                .flat_map(|l| &l.parts)
                .map(|p| Placed { part: p, x: p.x as isize, y: (p.y - line.dark.0) as isize, mirror_x: false, mirror_y: false })
                .collect();
            let rebuilt = compose(img.width, h, &placed, *state, outline);
            let retail = band_image(&img, band);
            let (mean, off) = difference(&retail, &rebuilt, 32)?;
            println!("{} {state:?}: mean |diff| {mean:.2} per channel, {:.2}% texels off by more than 32", line.text, off * 100.0);
            sheet.push(retail);
            sheet.push(rebuilt);
        }
    }
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{out_dir}: {e}"))?;
    let stack = |images: &[Image], path: &str| -> Result<(), String> {
        let width = images.iter().map(|i| i.width).max().unwrap_or(0);
        let mut rgba = Vec::new();
        let mut height = 0;
        for i in images {
            for y in 0..i.height {
                for x in 0..width {
                    let t = if x < i.width { i.at(x, y) } else { [0; 4] };
                    let a = u32::from(t[3]);
                    for k in 0..3 {
                        rgba.push(((u32::from(t[k]) * a + 200 * (255 - a)) / 255) as u8);
                    }
                    rgba.push(255);
                }
            }
            rgba.extend(std::iter::repeat_n(255u8, width * 4));
            height += i.height + 1;
        }
        crate::gfx_golden::write_png(Path::new(path), [width as u32, height as u32], &rgba)
    };
    stack(&sheet, &format!("{out_dir}/rebuild.png"))?;
    let colours: [[u8; 4]; 6] = [[230, 60, 60, 255], [60, 160, 230, 255], [60, 190, 90, 255], [230, 170, 40, 255], [160, 80, 200, 255], [40, 40, 40, 255]];
    let mut parts_sheet = Vec::new();
    for line in &lines {
        let mut im = Image::new(img.width, line.dark.1 - line.dark.0);
        for l in &line.letters {
            for (k, p) in l.parts.iter().enumerate() {
                for py in 0..p.height {
                    for px in 0..p.width {
                        if p.mask[py * p.width + px] {
                            im.set(p.x + px, p.y - line.dark.0 + py, colours[k % colours.len()]);
                        }
                    }
                }
            }
        }
        parts_sheet.push(im);
    }
    stack(&parts_sheet, &format!("{out_dir}/parts.png"))?;
    println!("wrote {out_dir}/rebuild.png and {out_dir}/parts.png");
    Ok(())
}

/// `mercs2_probe label-letters <shell.wad> <texture> <out.png> <scale> <label>... -- <row>...`: rows of
/// letters set from the texture's parts, in both states with the outline regrown, drawn `scale` times.
/// `=<letters>` is a row of retail letters (their own parts at their own places); `/` starts a row; any
/// other argument is a recipe letter (`label_art::Recipe`) added to the current row.
pub fn run_label_letters(args: &[String]) -> Result<(), String> {
    use mercs2_quartermaster::label_art::{compose, lines, measure_outline, read, recipe_parts, Placed, Recipe, State};
    let split = args.iter().position(|a| a == "--").ok_or("no `--` before the recipes")?;
    let (head, recipes) = (&args[..split], &args[split + 1..]);
    let [shell, name, out, scale, texts @ ..] = head else {
        return Err("usage: mercs2_probe label-letters <shell.wad> <texture> <out.png> <scale> <label>... -- <recipe>...".into());
    };
    let scale: usize = scale.parse().map_err(|e| format!("scale {scale:?}: {e}"))?;
    let mut game = wad_stack(shell)?;
    let img = read(&mut game, name)?;
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let lines = lines(&img, &texts)?;
    let outlines = [(State::Dark, measure_outline(&img, &lines, State::Dark)), (State::Bright, measure_outline(&img, &lines, State::Bright))];
    // Rows of letters: a recipe of the form `=ABC` is the retail letters themselves.
    let mut rows: Vec<Vec<Recipe>> = Vec::new();
    for r in recipes {
        if let Some(letters) = r.strip_prefix('=') {
            let mut row = Vec::new();
            for ch in letters.chars() {
                let l = lines.iter().flat_map(|l| &l.letters).find(|l| l.ch == ch).ok_or_else(|| format!("no retail {ch}"))?;
                let parts = l
                    .parts
                    .iter()
                    .enumerate()
                    .map(|(i, p)| format!("{ch}.{i}@{},{}", p.x - l.x0, p.y - lines.iter().find(|line| line.letters.contains(l)).map_or(0, |line| line.dark.0)))
                    .collect::<Vec<_>>()
                    .join(" ");
                row.push(Recipe::parse(&format!("{ch}={parts}"))?);
            }
            rows.push(row);
            rows.push(Vec::new());
        } else if r == "/" {
            rows.push(Vec::new());
        } else {
            if rows.is_empty() {
                rows.push(Vec::new());
            }
            rows.last_mut().expect("a row").push(Recipe::parse(r)?);
        }
    }
    rows.retain(|r| !r.is_empty());
    const MARGIN: isize = 8;
    const GAP: isize = 5;
    let band_h = lines.iter().map(|l| l.dark.1 - l.dark.0).max().unwrap_or(0) as isize + 2 * MARGIN;
    let mut images = Vec::new();
    for row in &rows {
        let mut placed_parts = Vec::new();
        let mut pen = MARGIN;
        for recipe in row {
            let parts = recipe_parts(&lines, recipe)?;
            let width = parts.iter().map(|(p, r)| r.x + p.width as isize).max().unwrap_or(0);
            for (p, r) in parts {
                placed_parts.push((p, pen + r.x, MARGIN + r.y, r.mirror_x, r.mirror_y));
            }
            pen += width + GAP;
        }
        let width = (pen + MARGIN) as usize;
        for (state, outline) in &outlines {
            let placed: Vec<Placed<'_>> = placed_parts.iter().map(|(p, x, y, mx, my)| Placed { part: p, x: *x, y: *y, mirror_x: *mx, mirror_y: *my }).collect();
            images.push(compose(width, band_h as usize, &placed, *state, outline));
        }
    }
    let width = images.iter().map(|i| i.width).max().unwrap_or(0) * scale;
    let mut rgba = Vec::new();
    let mut height = 0;
    for im in &images {
        for y in 0..im.height * scale {
            for x in 0..width {
                let (sx, sy) = (x / scale, y / scale);
                let t = if sx < im.width { im.at(sx, sy) } else { [0; 4] };
                let a = u32::from(t[3]);
                for k in 0..3 {
                    rgba.push(((u32::from(t[k]) * a + 128 * (255 - a)) / 255) as u8);
                }
                rgba.push(255);
            }
        }
        height += im.height * scale;
    }
    crate::gfx_golden::write_png(Path::new(out), [width as u32, height as u32], &rgba)?;
    println!("wrote {out}: {} row(s) of letters, both states", rows.len());
    Ok(())
}

/// The WADs of a comma-separated list, mounted in order (the last one's assets win).
fn wad_stack(list: &str) -> Result<mercs2_quartermaster::GameStack, String> {
    let paths: Vec<std::path::PathBuf> = list.split(',').map(std::path::PathBuf::from).collect();
    mercs2_quartermaster::GameStack::open(&paths).map_err(|e| e.to_string())
}

/// `mercs2_probe lettering-compare <wad[,wad…]> <texture> <svg_dir> <out.png> <label>...`: every retail
/// letter of the texture's labels (its first copy) against its SVG (`<svg_dir>/U+XXXX.svg`) drawn at the
/// retail cap height: the share of fill they share (intersection over union of the fills, coverage >= 128
/// for the SVG), and `out.png`, 6x, grey where both fill, red where only the SVG does, blue only retail.
pub fn run_lettering_compare(args: &[String]) -> Result<(), String> {
    use mercs2_quartermaster::label_art::{lines, rasterize, read};
    let [shell, name, dir, out, texts @ ..] = args else {
        return Err("usage: mercs2_probe lettering-compare <shell.wad> <texture> <svg_dir> <out.png> <label>...".into());
    };
    let mut game = wad_stack(shell)?;
    let img = read(&mut game, name)?;
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let lines = lines(&img, &texts)?;
    const SCALE: usize = 6;
    const PAD: usize = 3;
    let mut tiles: Vec<(usize, usize, Vec<[u8; 4]>)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut scores = Vec::new();
    for line in &lines {
        // The cap line: the top most letters of the label share (round letters overshoot it).
        let mut tops: Vec<usize> = line.letters.iter().map(|l| l.parts.iter().map(|p| p.y).min().unwrap_or(line.dark.0)).collect();
        tops.sort_unstable();
        let cap_top = tops[tops.len() / 2];
        for l in &line.letters {
            // `LETTERING_ONLY=SWX` limits the comparison to those letters.
            if std::env::var("LETTERING_ONLY").is_ok_and(|only| !only.contains(l.ch)) || !seen.insert(l.ch) {
                continue;
            }
            // The retail letter's fill, from the cap line (an overshoot above it is cut off; below, kept).
            let top = cap_top;
            let bottom = l.parts.iter().map(|p| p.y + p.height).max().unwrap_or(line.dark.1);
            let (rw, rh) = (l.x1 - l.x0, bottom - top);
            let mut retail = vec![false; rw * rh];
            for p in &l.parts {
                for py in 0..p.height {
                    for px in 0..p.width {
                        if p.mask[py * p.width + px] && p.y + py >= top {
                            retail[(p.y + py - top) * rw + (p.x - l.x0 + px)] = true;
                        }
                    }
                }
            }
            let path = format!("{dir}/U+{:04X}.svg", l.ch as u32);
            let svg = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
            // The drawn letter's cap rows only (its marks' room above and below is not retail's to compare).
            let (sw, full_h, full) = rasterize(&svg, 27.0)?;
            let top = mercs2_quartermaster::label_art::SVG_ABOVE;
            let sh = mercs2_quartermaster::label_art::CAP.min(full_h.saturating_sub(top));
            let cov: Vec<u8> = full[top * sw..(top + sh) * sw].to_vec();
            let (w, h) = (rw.max(sw) + 2 * PAD, rh.max(sh) + 2 * PAD);
            let (mut both, mut only_svg, mut only_retail) = (0usize, 0usize, 0usize);
            let mut tile = vec![[255u8, 255, 255, 255]; w * h];
            for y in 0..h {
                for x in 0..w {
                    let r = (x >= PAD && y >= PAD && x - PAD < rw && y - PAD < rh) && retail[(y - PAD) * rw + x - PAD];
                    let s = (x >= PAD && y >= PAD && x - PAD < sw && y - PAD < sh) && cov[(y - PAD) * sw + x - PAD] >= 128;
                    tile[y * w + x] = match (r, s) {
                        (true, true) => {
                            both += 1;
                            [90, 90, 90, 255]
                        }
                        (false, true) => {
                            only_svg += 1;
                            [230, 60, 60, 255]
                        }
                        (true, false) => {
                            only_retail += 1;
                            [60, 110, 230, 255]
                        }
                        (false, false) => [255, 255, 255, 255],
                    };
                }
            }
            let iou = both as f64 / (both + only_svg + only_retail).max(1) as f64;
            println!("{}: retail {rw}x{rh}, svg {sw}x{sh}: shared {:.1}% (svg-only {only_svg}, retail-only {only_retail})", l.ch, iou * 100.0);
            scores.push(iou);
            tiles.push((w, h, tile));
        }
    }
    let mean = scores.iter().sum::<f64>() / scores.len().max(1) as f64;
    println!("{} letters, mean shared fill {:.1}%", scores.len(), mean * 100.0);
    let width: usize = tiles.iter().map(|t| t.0 * SCALE + 4).sum();
    let height = tiles.iter().map(|t| t.1).max().unwrap_or(0) * SCALE;
    let mut rgba = vec![200u8; width * height * 4];
    let mut x0 = 0;
    for (w, h, tile) in &tiles {
        for y in 0..h * SCALE {
            for x in 0..w * SCALE {
                let t = tile[(y / SCALE) * w + x / SCALE];
                let i = (y * width + x0 + x) * 4;
                rgba[i..i + 4].copy_from_slice(&t);
            }
        }
        x0 += w * SCALE + 4;
    }
    crate::gfx_golden::write_png(Path::new(out), [width as u32, height as u32], &rgba)
}

/// `mercs2_probe lettering-label <shell.wad> <texture> <svg_dir> <out_dir> <label>... -- <text>...`: each
/// text set from the SVG letters with retail's grain and outline (taken from the texture's labels), both
/// states, written as `<out_dir>/<TEXT>.png` (dark above bright, over grey). A text that is one of the
/// labels is also stacked over retail's and compared with it, its first letter where retail's is.
pub fn run_lettering_label(args: &[String]) -> Result<(), String> {
    use mercs2_quartermaster::label_art::{band_image, cap_line, difference, grain_columns, lines, measure_outline, read, set_label, Image, State, RETAIL_SETTING};
    let split = args.iter().position(|a| a == "--").ok_or("no `--` before the texts")?;
    let (head, texts) = (&args[..split], &args[split + 1..]);
    let [shell, name, dir, out_dir, labels @ ..] = head else {
        return Err("usage: mercs2_probe lettering-label <shell.wad> <texture> <svg_dir> <out_dir> <label>... -- <text>...".into());
    };
    let mut game = wad_stack(shell)?;
    let img = read(&mut game, name)?;
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let lines = lines(&img, &labels)?;
    let grain = grain_columns(&lines);
    let (dark, bright) = (measure_outline(&img, &lines, State::Dark), measure_outline(&img, &lines, State::Bright));
    println!("{} grain columns from {} labels", grain.len(), lines.len());
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{out_dir}: {e}"))?;
    let glyph = |ch: char| std::fs::read(format!("{dir}/U+{:04X}.svg", ch as u32)).ok();
    // `LETTERING_GRAIN=0.75` sets the grain's strength (1 is retail's columns as they are).
    let strength: f32 = match std::env::var("LETTERING_GRAIN") {
        Ok(v) => v.parse().map_err(|e| format!("LETTERING_GRAIN {v:?}: {e}"))?,
        Err(_) => RETAIL_SETTING.grain,
    };
    let setting = mercs2_quartermaster::label_art::Setting { grain: strength, ..RETAIL_SETTING };
    for text in texts {
        let set = set_label(text, &glyph, &grain, (&dark, &bright), setting)?;
        let (d, b) = (set.dark, set.bright);
        let mut stack: Vec<Image> = vec![d.clone(), b.clone()];
        if let Some(line) = lines.iter().find(|l| l.text == *text) {
            // Ours moved so its first letter starts where retail's does, its cap on retail's cap line.
            let first = line.letters.first().map_or(0, |l| l.x0) as isize;
            let dx = first - mercs2_quartermaster::label_art::OUTLINE_REACH as isize;
            let dy = cap_line(line) as isize - line.dark.0 as isize - set.cap_row as isize;
            for (ours, band) in [(&d, line.dark), (&b, line.bright)] {
                let retail = band_image(&img, band);
                let mut moved = Image::new(retail.width, retail.height);
                for y in 0..ours.height {
                    for x in 0..ours.width {
                        let (tx, ty) = (x as isize + dx, y as isize + dy);
                        if tx >= 0 && ty >= 0 && (tx as usize) < moved.width && (ty as usize) < moved.height {
                            moved.set(tx as usize, ty as usize, ours.at(x, y));
                        }
                    }
                }
                let (mean, off) = difference(&retail, &moved, 32)?;
                println!("{text}: mean |diff| {mean:.2} per channel, {:.2}% texels off by more than 32", off * 100.0);
                stack.push(retail);
                stack.push(moved);
            }
        }
        let slug: String = text.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let slug = format!("{slug}-grain{:.0}", strength * 100.0);
        // At menu size, and magnified 4x without smoothing.
        for (scale, suffix) in [(1usize, ""), (4, "@4x")] {
            let width = stack.iter().map(|i| i.width).max().unwrap_or(0) * scale;
            let mut rgba = Vec::new();
            let mut height = 0;
            for im in &stack {
                for y in 0..im.height * scale {
                    for x in 0..width {
                        let (sx, sy) = (x / scale, y / scale);
                        let t = if sx < im.width { im.at(sx, sy) } else { [0; 4] };
                        let a = u32::from(t[3]);
                        for k in 0..3 {
                            rgba.push(((u32::from(t[k]) * a + 128 * (255 - a)) / 255) as u8);
                        }
                        rgba.push(255);
                    }
                }
                height += im.height * scale;
            }
            crate::gfx_golden::write_png(Path::new(&format!("{out_dir}/{slug}{suffix}.png")), [width as u32, height as u32], &rgba)?;
        }
    }
    println!("wrote {} label(s) to {out_dir}", texts.len());
    Ok(())
}

/// `mercs2_probe label-crops <shell.wad> <texture> <out_dir> <label>...`: reference images for the
/// lettering preview, cut from the player's own texture: each label's two bands (`label-<TEXT>.dark.png`,
/// `.bright.png`) and each letter's first copy (`U+XXXX.dark.png`, `.bright.png`), with the outline
/// around it. They are for looking at locally and are not redistributed.
pub fn run_label_crops(args: &[String]) -> Result<(), String> {
    use mercs2_quartermaster::label_art::{lines, read, Image};
    let [shell, name, dir, texts @ ..] = args else {
        return Err("usage: mercs2_probe label-crops <shell.wad> <texture> <out_dir> <label>...".into());
    };
    let mut game = wad_stack(shell)?;
    let img = read(&mut game, name)?;
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let lines = lines(&img, &texts)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{dir}: {e}"))?;
    let write = |path: String, im: &Image, x0: usize, x1: usize, y0: usize, y1: usize| -> Result<(), String> {
        let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
        for y in y0..y1 {
            for x in x0..x1 {
                rgba.extend(im.at(x, y));
            }
        }
        crate::gfx_golden::write_png(Path::new(&path), [(x1 - x0) as u32, (y1 - y0) as u32], &rgba)
    };
    const MARGIN: usize = 6;
    let mut seen = std::collections::BTreeSet::new();
    let mut count = 0;
    for line in &lines {
        let (x0, x1) = (line.letters.first().map_or(0, |l| l.x0.saturating_sub(MARGIN)), line.letters.last().map_or(img.width, |l| (l.x1 + MARGIN).min(img.width)));
        let slug: String = line.text.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        write(format!("{dir}/label-{slug}.dark.png"), &img, x0, x1, line.dark.0, line.dark.1)?;
        write(format!("{dir}/label-{slug}.bright.png"), &img, x0, x1, line.bright.0, line.bright.1)?;
        for l in &line.letters {
            if !seen.insert(l.ch) {
                continue;
            }
            let (a, b) = (l.x0.saturating_sub(MARGIN), (l.x1 + MARGIN).min(img.width));
            write(format!("{dir}/U+{:04X}.dark.png", l.ch as u32), &img, a, b, line.dark.0, line.dark.1)?;
            write(format!("{dir}/U+{:04X}.bright.png", l.ch as u32), &img, a, b, line.bright.0, line.bright.1)?;
            count += 1;
        }
    }
    println!("wrote {} labels and {count} letters to {dir}", lines.len());
    Ok(())
}

/// `mercs2_probe tex-crop <shell.wad> <texture> <x0> <y0> <x1> <y1> <scale> <out.png>`: a rectangle of a
/// texture, each texel drawn `scale` pixels square over grey, with a 1-pixel grid line every texel.
pub fn run_tex_crop(args: &[String]) -> Result<(), String> {
    let [shell, name, x0, y0, x1, y1, scale, out] = args else {
        return Err("usage: mercs2_probe tex-crop <shell.wad> <texture> <x0> <y0> <x1> <y1> <scale> <out.png>".into());
    };
    let n = |s: &String| s.parse::<usize>().map_err(|e| format!("{s:?}: {e}"));
    let (x0, y0, x1, y1, scale) = (n(x0)?, n(y0)?, n(x1)?, n(y1)?, n(scale)?.max(1));
    let mut game = wad_stack(shell)?;
    let img = mercs2_quartermaster::label_art::read(&mut game, name)?;
    if x1 > img.width || y1 > img.height || x0 >= x1 || y0 >= y1 {
        return Err(format!("{x0},{y0}..{x1},{y1} is not inside {}x{}", img.width, img.height));
    }
    let (w, h) = ((x1 - x0) * scale, (y1 - y0) * scale);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for py in 0..h {
        for px in 0..w {
            let t = img.at(x0 + px / scale, y0 + py / scale);
            let grid = scale >= 4 && (px % scale == 0 || py % scale == 0);
            let a = u32::from(t[3]);
            let bg = if grid { 150u32 } else { 200 };
            for k in 0..3 {
                rgba.push(((u32::from(t[k]) * a + bg * (255 - a)) / 255) as u8);
            }
            rgba.push(255);
        }
    }
    crate::gfx_golden::write_png(Path::new(out), [w as u32, h as u32], &rgba)
}

/// `mercs2_probe font-coverage <font.ttf> <characters>`: which of `characters` the font has no glyph for.
pub fn run_font_coverage(args: &[String]) -> Result<(), String> {
    let [path, chars] = args else {
        return Err("usage: mercs2_probe font-coverage <font.ttf> <characters>".into());
    };
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).map_err(|e| format!("{path}: {e}"))?;
    let wanted: Vec<char> = chars.chars().filter(|c| !c.is_whitespace()).collect();
    let missing: String = wanted.iter().filter(|&&c| font.lookup_glyph_index(c) == 0).collect();
    println!("{path}: {} glyphs; {} of {} characters asked for are missing: {missing}", font.glyph_count(), missing.chars().count(), wanted.len());
    // Which glyphs are composites (built from other glyphs: numberOfContours < 0 in `glyf`).
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    let table = |tag: &[u8; 4]| -> Result<&[u8], String> {
        let n = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        (0..n)
            .map(|i| &bytes[12 + 16 * i..28 + 16 * i])
            .find(|r| &r[..4] == tag)
            .and_then(|r| {
                let off = u32::from_be_bytes([r[8], r[9], r[10], r[11]]) as usize;
                let len = u32::from_be_bytes([r[12], r[13], r[14], r[15]]) as usize;
                bytes.get(off..off + len)
            })
            .ok_or_else(|| format!("{path} has no {} table", String::from_utf8_lossy(tag)))
    };
    let (head, loca, glyf) = match (table(b"head"), table(b"loca"), table(b"glyf")) {
        (Ok(h), Ok(l), Ok(g)) => (h, l, g),
        _ => {
            println!("  no glyf/loca: not a TrueType-outline font");
            return Ok(());
        }
    };
    let long = i16::from_be_bytes([head[50], head[51]]) == 1;
    let offset = |g: usize| -> usize {
        if long {
            u32::from_be_bytes([loca[4 * g], loca[4 * g + 1], loca[4 * g + 2], loca[4 * g + 3]]) as usize
        } else {
            2 * usize::from(u16::from_be_bytes([loca[2 * g], loca[2 * g + 1]]))
        }
    };
    let (mut composite, mut simple) = (String::new(), String::new());
    for &c in &wanted {
        let g = usize::from(font.lookup_glyph_index(c));
        if g == 0 || offset(g) == offset(g + 1) {
            continue;
        }
        let contours = i16::from_be_bytes([glyf[offset(g)], glyf[offset(g) + 1]]);
        if contours < 0 { composite.push(c) } else { simple.push(c) }
    }
    println!("  composite: {composite}");
    println!("  drawn: {simple}");
    Ok(())
}

/// `mercs2_probe font-compare <shell.wad> <texture> <font.ttf> <out.png> <word>...`: each band of the
/// texture's lettering (its dark-fill state: every other band, from the first) above the same word set in
/// the font in black, its capital height scaled to the band's fill height. `word`s name the bands in order.
pub fn run_font_compare(args: &[String]) -> Result<(), String> {
    use mercs2_quartermaster::label_art::{bands, read, Fill};
    let [shell, name, path, out, words @ ..] = args else {
        return Err("usage: mercs2_probe font-compare <shell.wad> <texture> <font.ttf> <out.png> <word>...".into());
    };
    let mut game = wad_stack(shell)?;
    let img = read(&mut game, name)?;
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).map_err(|e| format!("{path}: {e}"))?;
    let all = bands(&img, 128);
    let dark: Vec<(usize, usize)> = all.iter().step_by(2).copied().collect();
    if dark.len() != words.len() {
        return Err(format!("{name} has {} dark bands; {} words given", dark.len(), words.len()));
    }
    let fill = Fill::Dark { alpha: 128, luma: 80 };
    let width = img.width;
    let mut rows: Vec<Vec<[u8; 4]>> = Vec::new();
    let bg = [200u8, 200, 200, 255];
    for (band, word) in dark.iter().zip(words) {
        // The retail band over grey.
        for y in band.0..band.1 {
            rows.push(
                (0..width)
                    .map(|x| {
                        let t = img.at(x, y);
                        let a = u32::from(t[3]);
                        let mix = |c: u8, b: u8| ((u32::from(c) * a + u32::from(b) * (255 - a)) / 255) as u8;
                        [mix(t[0], bg[0]), mix(t[1], bg[1]), mix(t[2], bg[2]), 255]
                    })
                    .collect(),
            );
        }
        // The fill's height: the rows of the band holding dark fill.
        let fill_rows: Vec<usize> = (band.0..band.1).filter(|&y| (0..width).any(|x| fill.holds(img.at(x, y)))).collect();
        let cap = (fill_rows.last().unwrap_or(&band.1) - fill_rows.first().unwrap_or(&band.0) + 1) as f32;
        let h_metrics = font.metrics('H', 100.0);
        let px = 100.0 * cap / h_metrics.height as f32;
        // The word, centred on the band's centre, baseline at the bottom of the fill.
        let glyphs: Vec<(fontdue::Metrics, Vec<u8>)> = word.chars().map(|c| font.rasterize(c, px)).collect();
        let advance: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
        let mut canvas = vec![bg; width * (band.1 - band.0 + 8)];
        let h = band.1 - band.0 + 8;
        let baseline = (cap + 4.0) as i32;
        let mut pen = (width as f32 - advance) / 2.0;
        for (m, bitmap) in &glyphs {
            for gy in 0..m.height {
                for gx in 0..m.width {
                    let x = (pen + m.xmin as f32) as i32 + gx as i32;
                    let y = baseline - m.ymin - m.height as i32 + gy as i32;
                    if x >= 0 && (x as usize) < width && y >= 0 && (y as usize) < h {
                        let a = u32::from(bitmap[gy * m.width + gx]);
                        let p = &mut canvas[y as usize * width + x as usize];
                        for k in 0..3 {
                            p[k] = ((u32::from(p[k]) * (255 - a)) / 255) as u8;
                        }
                    }
                }
            }
            pen += m.advance_width;
        }
        rows.extend(canvas.chunks(width).map(<[[u8; 4]]>::to_vec));
        rows.push(vec![[255, 255, 255, 255]; width]);
        println!("{word}: band {}..{}, fill {cap} px high, font at {px:.1} px", band.0, band.1);
    }
    let rgba: Vec<u8> = rows.iter().flatten().flatten().copied().collect();
    crate::gfx_golden::write_png(Path::new(out), [width as u32, rows.len() as u32], &rgba)
}

/// `mercs2_probe gfx-patch-actions <in.gfx> <out.gfx> <patches.tsv>`
pub fn run_patch(args: &[String]) -> Result<(), String> {
    let [input, output, table] = args else {
        return Err("usage: mercs2_probe gfx-patch-actions <in.gfx> <out.gfx> <patches.tsv>".into());
    };
    let patches = parse_patches(&std::fs::read_to_string(table).map_err(|e| format!("read {table}: {e}"))?)?;
    let data = std::fs::read(input).map_err(|e| format!("read {input}: {e}"))?;
    let (out, done) = patch_actions(&data, &patches)?;
    check_decodes(&out)?;
    std::fs::write(Path::new(output), &out).map_err(|e| format!("write {output}: {e}"))?;
    for (t, len) in &done {
        println!("{t:?}: DoAction now {len} bytes");
    }
    println!("wrote {output} ({} bytes)", out.len());
    Ok(())
}

/// `mercs2_probe gfx-swap-constant <in.gfx> <out.gfx> <from> <to> <target>...`
pub fn run(args: &[String]) -> Result<(), String> {
    let [input, output, from, to, targets @ ..] = args else {
        return Err("usage: mercs2_probe gfx-swap-constant <in.gfx> <out.gfx> <from> <to> <root:<frame>|sprite:<id>:<frame>>...".into());
    };
    if targets.is_empty() {
        return Err("name at least one target block".into());
    }
    let targets = targets.iter().map(|t| Target::parse(t)).collect::<Result<Vec<_>, _>>()?;
    let data = std::fs::read(input).map_err(|e| format!("read {input}: {e}"))?;
    let (out, done) = swap_constant(&data, &targets, from, to)?;
    check_decodes(&out)?;
    std::fs::write(Path::new(output), &out).map_err(|e| format!("write {output}: {e}"))?;
    for (t, len) in &done {
        println!("{t:?}: {from:?} -> {to:?} (DoAction now {len} bytes)");
    }
    println!("wrote {output} ({} bytes)", out.len());
    Ok(())
}
