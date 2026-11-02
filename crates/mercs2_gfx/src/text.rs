//! Text fields: the font textures the movies ship, and laying a field's text out into glyph quads.

use std::collections::HashMap;
use std::rc::Rc;

use mercs2_formats::gfx_def::{Character, EditText, FontTexture, MovieDef, Rgba, TextureGlyph};

use crate::tess::{Paint, Part};

/// Font units to the em in a `DefineFont3`.
const EM: f32 = 20480.0;
/// Flash's gutter between a field's edge and its text: 2 pixels.
const GUTTER: f32 = 40.0;

/// One font with the atlas that holds its glyphs.
#[derive(Clone, Debug)]
pub struct FontLib {
    glyph_of: HashMap<u16, usize>,
    advances: Vec<i16>,
    ascent: f32,
    descent: f32,
    leading: f32,
    atlas: Rc<str>,
    size: [f32; 2],
    nominal: f32,
    textured: HashMap<usize, TextureGlyph>,
}

impl FontLib {
    /// Every textured font in `def`, by character id.
    pub fn all_in(def: &MovieDef) -> Result<Vec<(u16, FontLib)>, String> {
        let mut out = Vec::new();
        for bytes in &def.font_textures {
            let texture = FontTexture::decode(bytes)?;
            for (id, pairs) in &texture.fonts {
                let Some(Character::Font(tag)) = def.characters.get(id) else {
                    return Err(format!("{} names font {id}, which the movie does not define", texture.file_name));
                };
                let font = tag.decode()?;
                let textured = pairs
                    .iter()
                    .map(|(glyph, slot)| {
                        let at = texture.glyphs.get(*slot as usize).ok_or_else(|| format!("texture glyph {slot} is out of range"))?;
                        Ok((*glyph as usize, *at))
                    })
                    .collect::<Result<HashMap<_, _>, String>>()?;
                out.push((
                    *id,
                    FontLib {
                        glyph_of: font.codes.iter().enumerate().map(|(i, c)| (*c, i)).collect(),
                        advances: font.advances,
                        ascent: font.ascent as f32,
                        descent: font.descent as f32,
                        leading: font.leading as f32,
                        atlas: texture.file_name.as_str().into(),
                        size: [texture.width as f32, texture.height as f32],
                        nominal: texture.nominal as f32,
                        textured,
                    },
                ));
            }
        }
        Ok(out)
    }

    /// The font library a movie holds, as `fonts_enext` holds Gunplay.
    pub fn from_movie(def: &MovieDef) -> Result<FontLib, String> {
        FontLib::all_in(def)?.into_iter().next().map(|(_, font)| font).ok_or_else(|| "a font movie with no textured font".to_string())
    }

    /// The advance of `c` at `height` twips.
    fn advance(&self, c: char, height: f32) -> f32 {
        let Some(glyph) = u16::try_from(c as u32).ok().and_then(|code| self.glyph_of.get(&code)) else { return 0.0 };
        self.advances.get(*glyph).copied().unwrap_or(0) as f32 / EM * height
    }

    fn line_height(&self, height: f32) -> f32 {
        (self.ascent + self.descent + self.leading) / EM * height
    }
}

/// A field's text as display text: markup gone, line breaks kept.
pub fn plain_text(text: &str, html: bool) -> String {
    if !html {
        return text.to_string();
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else { break };
        let tag = rest[open + 1..open + close].to_ascii_lowercase();
        if tag.starts_with("br") || tag == "/p" {
            out.push('\n');
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Break `text` into lines that fit `width` twips when `wrap` is set.
fn lines(text: &str, font: &FontLib, height: f32, wrap: Option<f32>) -> Vec<(String, f32)> {
    let width_of = |s: &str| s.chars().map(|c| font.advance(c, height)).sum::<f32>();
    let mut out = Vec::new();
    for paragraph in text.split(['\n', '\r']) {
        let Some(limit) = wrap else {
            out.push((paragraph.to_string(), width_of(paragraph)));
            continue;
        };
        let mut line = String::new();
        for word in paragraph.split_inclusive(' ') {
            let next = format!("{line}{word}");
            if !line.is_empty() && width_of(next.trim_end()) > limit {
                let width = width_of(line.trim_end());
                out.push((std::mem::take(&mut line), width));
                line = word.to_string();
            } else {
                line = next;
            }
        }
        let width = width_of(line.trim_end());
        out.push((line, width));
    }
    out
}

/// `(width, height)` of a field's text in twips, the `textWidth` and `textHeight` it reports.
pub fn measure(field: &EditText, text: &str, font: &FontLib) -> [f32; 2] {
    let height = field.height as f32;
    let wrap = field.word_wrap.then_some((field.bounds[1] - field.bounds[0]) as f32 - GUTTER * 2.0);
    let rows = lines(&plain_text(text, field.html), font, height, wrap);
    let widest = rows.iter().map(|r| r.1).fold(0.0, f32::max);
    [widest, rows.len() as f32 * font.line_height(height)]
}

/// The quads of a field's text, in the field's space.
pub fn layout(field: &EditText, text: &str, font: &FontLib) -> Option<Part> {
    let height = field.height as f32;
    let [xmin, xmax, ymin, _] = field.bounds.map(|v| v as f32);
    let (align, left, right, indent, extra) = field.layout.unwrap_or((0, 0, 0, 0, 0));
    let inner = xmax - xmin - GUTTER * 2.0 - left as f32 - right as f32;
    let wrap = field.word_wrap.then_some(inner);
    let rows = lines(&plain_text(text, field.html), font, height, wrap);
    let (mut vertices, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    let glyph_scale = height / font.nominal;
    let mut baseline = ymin + GUTTER + font.ascent / EM * height;
    for (row, (line, width)) in rows.iter().enumerate() {
        let start = xmin + GUTTER + left as f32 + if row == 0 { indent as f32 } else { 0.0 };
        let mut pen = match align {
            1 => start + inner - width,
            2 => start + (inner - width) / 2.0,
            _ => start,
        };
        for c in line.chars() {
            if let Some(g) = u16::try_from(c as u32).ok().and_then(|code| font.glyph_of.get(&code)).and_then(|i| font.textured.get(i)) {
                let [u0, v0, u1, v1] = g.uv;
                let x = |u: f32| pen + (u - g.origin[0]) * font.size[0] * glyph_scale;
                let y = |v: f32| baseline + (v - g.origin[1]) * font.size[1] * glyph_scale;
                let base = vertices.len() as u32;
                vertices.extend([[x(u0), y(v0)], [x(u1), y(v0)], [x(u0), y(v1)], [x(u1), y(v1)]]);
                uvs.extend([[u0, v0], [u1, v0], [u0, v1], [u1, v1]]);
                indices.extend([base, base + 1, base + 2, base + 2, base + 1, base + 3]);
            }
            pen += font.advance(c, height);
        }
        baseline += font.line_height(height) + extra as f32;
    }
    if vertices.is_empty() {
        return None;
    }
    let color: Rgba = field.color.unwrap_or([0, 0, 0, 255]);
    Some(Part { paint: Paint::Glyphs { atlas: font.atlas.clone(), color, uvs }, vertices, indices })
}

/// Swap every `[0x…]` string key in `text` through `translate`, leaving a key it does not know.
pub fn translate(text: &str, lookup: &dyn Fn(u32) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("[0x") {
        out.push_str(&rest[..open]);
        let tail = &rest[open..];
        // The bracket that closes this key: a key can hold another as its argument, `[0xA:[0xB]]`.
        let mut depth = 0;
        let close = tail.char_indices().find_map(|(at, c)| {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(at)
        });
        let Some(close) = close else {
            out.push_str(tail);
            return out;
        };
        let inner = &tail[3..close];
        let (key, argument) = inner.split_once(':').unwrap_or((inner, ""));
        match u32::from_str_radix(key, 16).ok().and_then(lookup) {
            Some(found) => out.push_str(&found.replace("%s", &translate(argument, lookup))),
            None => out.push_str(&tail[..=close]),
        }
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> FontLib {
        let glyph = TextureGlyph { uv: [0.0, 0.0, 0.5, 0.5], origin: [0.0, 0.5] };
        FontLib {
            glyph_of: [(b'a' as u16, 0), (b' ' as u16, 1)].into(),
            advances: vec![10240, 10240],
            ascent: 16384.0,
            descent: 4096.0,
            leading: 0.0,
            atlas: "atlas.tga".into(),
            size: [64.0, 64.0],
            nominal: 32.0,
            textured: [(0, glyph)].into(),
        }
    }

    fn field() -> EditText {
        EditText { bounds: [0, 2000, 0, 400], height: 200, ..EditText::default() }
    }

    #[test]
    fn html_markup_goes_and_breaks_stay() {
        assert_eq!(plain_text("<p>a<br>b</p>&amp;", true), "a\nb\n&");
        assert_eq!(plain_text("<b>", false), "<b>");
    }

    #[test]
    fn a_string_key_translates_and_an_unknown_one_stays() {
        let lookup = |k: u32| (k == 0x32E96B39).then(|| "NEW GAME".to_string());
        assert_eq!(translate("[0x32e96b39]", &lookup), "NEW GAME");
        assert_eq!(translate("a [0x1] b", &lookup), "a [0x1] b");
    }

    #[test]
    fn a_key_takes_another_as_its_argument() {
        let lookup = |k: u32| match k {
            0xA => Some("Press %s to start".to_string()),
            0xB => Some("ENTER".to_string()),
            _ => None,
        };
        assert_eq!(translate("[0xA:[0xB]]", &lookup), "Press ENTER to start");
    }

    #[test]
    fn a_glyph_sits_on_the_baseline_past_the_gutter() {
        let part = layout(&field(), "a", &font()).expect("a glyph");
        // Ascent 0.8 em of 200 twips: the baseline is 40 + 160 down.
        assert_eq!(part.vertices[0], [40.0, 200.0 - 200.0]);
        assert_eq!(part.vertices[3], [240.0, 200.0]);
    }

    #[test]
    fn centered_text_splits_the_spare_width() {
        let mut f = field();
        f.layout = Some((2, 0, 0, 0, 0));
        let part = layout(&f, "a", &font()).expect("a glyph");
        assert_eq!(part.vertices[0][0], 40.0 + (1920.0 - 100.0) / 2.0);
    }

    #[test]
    fn words_wrap_at_the_field_width() {
        let mut f = field();
        f.word_wrap = true;
        f.bounds[1] = 330;
        // Each "aa" is 200 wide; 250 fit.
        let rows = lines("aa aa", &font(), 200.0, Some(250.0));
        assert_eq!(rows.len(), 2);
        assert_eq!(measure(&f, "aa aa", &font())[1], 2.0 * 200.0);
    }
}
