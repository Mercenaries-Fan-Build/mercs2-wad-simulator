//! Text fields: the font textures the movies ship, and laying a field's text out into glyph quads.

use std::collections::HashMap;
use std::sync::Arc;

use mercs2_formats::gfx_def::{Character, EditText, FontTexture, Matrix, MovieDef, Rgba, TextureGlyph};

use crate::tess::{Paint, Part};

/// Font units to the em in a `DefineFont3`.
const EM: f32 = 20480.0;
/// Flash's gutter between a field's edge and its text: 2 pixels.
pub(crate) const GUTTER: f32 = 40.0;

/// One `setImageSubstitutions` element as retail's `GFxTextImageDesc` holds it (`FUN_007a81e0`).
/// Lengths are twips: the element's pixels times 20.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageSub {
    /// The text it stands in for, UTF-16, 1 to 15 units.
    pub key: Vec<u16>,
    /// The exported image character.
    pub image: u16,
    /// The image's own size.
    pub natural: [f32; 2],
    /// `width`, `height`: the size it draws at, its own when absent.
    pub size: [f32; 2],
    /// `baseLineX`, `baseLineY`.
    pub base_line: [f32; 2],
}

/// One font with the atlas that holds its glyphs.
#[derive(Clone, Debug)]
pub struct FontLib {
    glyph_of: HashMap<u16, usize>,
    advances: Vec<i16>,
    ascent: f32,
    descent: f32,
    leading: f32,
    atlas: Arc<str>,
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

/// What a line is made of: a character, or one of the field's substitution images.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Token {
    Glyph(char),
    Image(usize),
}

/// A paragraph as tokens. At every position the first image whose key the text there starts with
/// takes the key's characters, which are not drawn (`FUN_007e6290` with `FUN_007e5090`; the compare,
/// `FUN_007e4f90`, is case-sensitive on UTF-16 units).
fn tokens(paragraph: &str, subs: &[ImageSub]) -> Vec<Token> {
    let units: Vec<u16> = paragraph.encode_utf16().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < units.len() {
        if let Some(i) = subs.iter().position(|s| units[at..].starts_with(&s.key)) {
            out.push(Token::Image(i));
            at += subs[i].key.len();
            continue;
        }
        let c = char::decode_utf16(units[at..].iter().copied()).next().map_or('\u{FFFD}', |r| r.unwrap_or('\u{FFFD}'));
        out.push(Token::Glyph(c));
        at += c.len_utf16();
    }
    out
}

/// An image's pen advance: its width and 40 twips (`addss` of the `40.0f` at `0xB9B724`, `0x7E6B7F`).
const IMAGE_GAP: f32 = 40.0;

fn advance(token: Token, font: &FontLib, height: f32, subs: &[ImageSub]) -> f32 {
    match token {
        Token::Glyph(c) => font.advance(c, height),
        Token::Image(i) => subs[i].size[0] + IMAGE_GAP,
    }
}

/// How far an image reaches above and below the baseline. The line keeps the largest of these and the
/// font's own (`FUN_007e6290`: above is `-ty` of the image's matrix, below its height less that).
fn image_extent(sub: &ImageSub) -> [f32; 2] {
    let above = sub.base_line[1] * sub.size[1] / sub.natural[1];
    [above, sub.size[1] - above]
}

/// One laid-out line: its tokens, width and how far it reaches above and below its baseline.
struct Row {
    tokens: Vec<Token>,
    width: f32,
    above: f32,
    below: f32,
}

/// Break `text` into lines that fit `width` twips when `wrap` is set.
fn lines(text: &str, font: &FontLib, height: f32, wrap: Option<f32>, subs: &[ImageSub]) -> Vec<Row> {
    let width_of = |line: &[Token]| {
        let end = line.iter().rposition(|t| *t != Token::Glyph(' ')).map_or(0, |i| i + 1);
        line[..end].iter().map(|t| advance(*t, font, height, subs)).sum::<f32>()
    };
    let row = |line: Vec<Token>| {
        let (mut above, mut below) = (font.ascent / EM * height, font.descent / EM * height);
        for t in &line {
            if let Token::Image(i) = t {
                let [a, b] = image_extent(&subs[*i]);
                above = above.max(a);
                below = below.max(b);
            }
        }
        Row { width: width_of(&line), tokens: line, above, below }
    };
    let mut out = Vec::new();
    for paragraph in text.split(['\n', '\r']) {
        let all = tokens(paragraph, subs);
        let Some(limit) = wrap else {
            out.push(row(all));
            continue;
        };
        let mut line: Vec<Token> = Vec::new();
        for word in all.split_inclusive(|t| *t == Token::Glyph(' ')) {
            let next: Vec<Token> = line.iter().chain(word).copied().collect();
            if !line.is_empty() && width_of(&next) > limit {
                out.push(row(std::mem::replace(&mut line, word.to_vec())));
            } else {
                line = next;
            }
        }
        out.push(row(line));
    }
    out
}

/// `(width, height)` of a field's text in twips, the `textWidth` and `textHeight` it reports.
pub fn measure(field: &EditText, text: &str, font: &FontLib, subs: &[ImageSub]) -> [f32; 2] {
    let height = field.height as f32;
    let wrap = field.word_wrap.then_some((field.bounds[1] - field.bounds[0]) as f32 - GUTTER * 2.0);
    let rows = lines(&plain_text(text, field.html), font, height, wrap, subs);
    let widest = rows.iter().map(|r| r.width).fold(0.0, f32::max);
    let leading = font.leading / EM * height;
    [widest, rows.iter().map(|r| r.above + r.below + leading).sum()]
}

/// The quads of a field's text, in the field's space: the glyphs, then each substitution image.
///
/// An image is drawn through retail's matrix for it (`FUN_007a81e0`: scale `size / natural`, offset
/// `-scale · baseLine`) from the pen on the baseline, which is where the line metrics above put it.
/// The draw itself was not traced, and neither was the filtering, so linear filtering is assumed.
pub fn layout(field: &EditText, text: &str, font: &FontLib, subs: &[ImageSub]) -> Vec<Part> {
    let height = field.height as f32;
    let [xmin, xmax, ymin, _] = field.bounds.map(|v| v as f32);
    let (align, left, right, indent, extra) = field.layout.unwrap_or((0, 0, 0, 0, 0));
    let inner = xmax - xmin - GUTTER * 2.0 - left as f32 - right as f32;
    let wrap = field.word_wrap.then_some(inner);
    let rows = lines(&plain_text(text, field.html), font, height, wrap, subs);
    let (mut vertices, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    let mut images = Vec::new();
    let glyph_scale = height / font.nominal;
    let leading = font.leading / EM * height;
    let mut top = ymin + GUTTER;
    for (index, row) in rows.iter().enumerate() {
        let baseline = top + row.above;
        let start = xmin + GUTTER + left as f32 + if index == 0 { indent as f32 } else { 0.0 };
        let mut pen = match align {
            1 => start + inner - row.width,
            2 => start + (inner - row.width) / 2.0,
            _ => start,
        };
        for token in &row.tokens {
            match *token {
                Token::Glyph(c) => {
                    if let Some(g) = u16::try_from(c as u32).ok().and_then(|code| font.glyph_of.get(&code)).and_then(|i| font.textured.get(i)) {
                        let [u0, v0, u1, v1] = g.uv;
                        let x = |u: f32| pen + (u - g.origin[0]) * font.size[0] * glyph_scale;
                        let y = |v: f32| baseline + (v - g.origin[1]) * font.size[1] * glyph_scale;
                        let base = vertices.len() as u32;
                        vertices.extend([[x(u0), y(v0)], [x(u1), y(v0)], [x(u0), y(v1)], [x(u1), y(v1)]]);
                        uvs.extend([[u0, v0], [u1, v0], [u0, v1], [u1, v1]]);
                        indices.extend([base, base + 1, base + 2, base + 2, base + 1, base + 3]);
                    }
                }
                Token::Image(i) => images.extend(image_part(&subs[i], pen, baseline)),
            }
            pen += advance(*token, font, height, subs);
        }
        top = baseline + row.below + leading + extra as f32;
    }
    let mut out = Vec::new();
    if !vertices.is_empty() {
        let color: Rgba = field.color.unwrap_or([0, 0, 0, 255]);
        out.push(Part { paint: Paint::Glyphs { atlas: font.atlas.clone(), color, uvs }, vertices, indices });
    }
    out.extend(images);
    out
}

/// One substitution image as a quad from the pen at `baseline`; none when a `width` or `height` of 0
/// scales it to nothing.
fn image_part(sub: &ImageSub, pen: f32, baseline: f32) -> Option<Part> {
    let scale = [sub.size[0] / sub.natural[0], sub.size[1] / sub.natural[1]];
    let x0 = pen - scale[0] * sub.base_line[0];
    let y0 = baseline - scale[1] * sub.base_line[1];
    let [x1, y1] = [x0 + sub.size[0], y0 + sub.size[1]];
    // Image pixels to twips: the natural size is the pixels times 20.
    let to_twips = Matrix { a: scale[0] * 20.0, b: 0.0, c: 0.0, d: scale[1] * 20.0, tx: x0, ty: y0 };
    let to_pixels = to_twips.inverse()?;
    Some(Part {
        paint: Paint::Bitmap { id: sub.image, to_pixels, clamp: true, smooth: true },
        vertices: vec![[x0, y0], [x1, y0], [x0, y1], [x1, y1]],
        indices: vec![0, 1, 2, 2, 1, 3],
    })
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
            Some(found) => out.push_str(&fill(&found, &arguments(argument), lookup)),
            None => out.push_str(&tail[..=close]),
        }
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    out
}

/// A key's arguments: split at each `:` outside a nested key, as the joystick page's
/// `[0x603e1c7d:%s:%s:%s]` carries three.
fn arguments(argument: &str) -> Vec<&str> {
    if argument.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (at, c) in argument.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ':' if depth == 0 => {
                out.push(&argument[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(&argument[start..]);
    out
}

/// `format` with each `%s` in turn taking the next argument, translated; one argument fills every `%s`.
fn fill(format: &str, args: &[&str], lookup: &dyn Fn(u32) -> Option<String>) -> String {
    if args.len() <= 1 {
        return format.replace("%s", &translate(args.first().copied().unwrap_or(""), lookup));
    }
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = format;
    while let Some(at) = rest.find("%s") {
        out.push_str(&rest[..at]);
        match args.next() {
            Some(arg) => out.push_str(&translate(arg, lookup)),
            None => out.push_str("%s"),
        }
        rest = &rest[at + 2..];
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
    fn a_key_takes_its_arguments_in_turn() {
        let lookup = |k: u32| match k {
            0xA => Some("The key %s is currently bound to %s, would you like to bind it to %s instead?".to_string()),
            0xB => Some("Button 1".to_string()),
            0xC => Some("Jump".to_string()),
            0xD => Some("Walk".to_string()),
            _ => None,
        };
        assert_eq!(
            translate("[0xA:[0xB]:[0xC]:[0xD]]", &lookup),
            "The key Button 1 is currently bound to Jump, would you like to bind it to Walk instead?"
        );
    }

    #[test]
    fn a_glyph_sits_on_the_baseline_past_the_gutter() {
        let parts = layout(&field(), "a", &font(), &[]);
        // Ascent 0.8 em of 200 twips: the baseline is 40 + 160 down.
        assert_eq!(parts[0].vertices[0], [40.0, 200.0 - 200.0]);
        assert_eq!(parts[0].vertices[3], [240.0, 200.0]);
    }

    #[test]
    fn centered_text_splits_the_spare_width() {
        let mut f = field();
        f.layout = Some((2, 0, 0, 0, 0));
        let parts = layout(&f, "a", &font(), &[]);
        assert_eq!(parts[0].vertices[0][0], 40.0 + (1920.0 - 100.0) / 2.0);
    }

    #[test]
    fn words_wrap_at_the_field_width() {
        let mut f = field();
        f.word_wrap = true;
        f.bounds[1] = 330;
        // Each "aa" is 200 wide; 250 fit.
        let rows = lines("aa aa", &font(), 200.0, Some(250.0), &[]);
        assert_eq!(rows.len(), 2);
        assert_eq!(measure(&f, "aa aa", &font(), &[])[1], 2.0 * 200.0);
    }

    /// A 10 × 10 pixel image drawn 20 × 10 for `[x]`, 9 pixels of it above the baseline.
    fn flag() -> ImageSub {
        ImageSub { key: "[x]".encode_utf16().collect(), image: 7, natural: [200.0, 200.0], size: [400.0, 200.0], base_line: [0.0, 180.0] }
    }

    #[test]
    fn a_key_draws_its_image_in_place_of_its_characters() {
        let parts = layout(&field(), "a[x]a", &font(), &[flag()]);
        assert_eq!(parts.len(), 2, "the glyphs, then the image");
        assert_eq!(parts[0].vertices.len(), 8, "two glyphs: the key's characters are not drawn");
        // The image reaches 180 above the baseline, more than the font's 160, so the baseline drops.
        let baseline = 40.0 + 180.0;
        assert_eq!(parts[1].vertices[0], [40.0 + 100.0, baseline - 180.0]);
        assert_eq!(parts[1].vertices[3], [140.0 + 400.0, baseline + 20.0]);
        // The next glyph starts past the image's width and the 40-twip gap.
        assert_eq!(parts[0].vertices[4][0], 140.0 + 400.0 + 40.0);
        let Paint::Bitmap { id, to_pixels, .. } = &parts[1].paint else { panic!("a bitmap") };
        assert_eq!(*id, 7);
        assert_eq!(to_pixels.apply(540.0, 240.0), [10.0, 10.0], "the far corner is the image's last pixel edge");
    }

    #[test]
    fn a_key_matches_case_and_all_its_units() {
        assert_eq!(tokens("[X]", &[flag()]), [Token::Glyph('['), Token::Glyph('X'), Token::Glyph(']')]);
        assert_eq!(tokens("[x", &[flag()]), [Token::Glyph('['), Token::Glyph('x')]);
        assert_eq!(tokens("a[x]", &[flag()]), [Token::Glyph('a'), Token::Image(0)]);
    }

    #[test]
    fn an_image_line_is_as_tall_as_what_it_holds() {
        // 180 above (more than the font's 160) and 40 below (the font's; the image's is 20).
        assert_eq!(measure(&field(), "[x]", &font(), &[flag()]), [440.0, 220.0]);
    }

    #[test]
    fn an_image_scaled_to_nothing_draws_nothing() {
        let mut sub = flag();
        sub.size[0] = 0.0;
        assert_eq!(layout(&field(), "[x]", &font(), &[sub]).len(), 0);
    }
}
