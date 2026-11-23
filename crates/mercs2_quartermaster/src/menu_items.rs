//! qm's edit of a retail menu movie that adds the Shipments' menu items (`add_menu_item`) to its pages.
//!
//! A page is described by `data/menus.toml`: its items in the order Up and Down walk them, and where its
//! scripts are. Retail pages are hand-written: each script that knows the items is a `switch` on the
//! page's index variable with one case per item, every move between items names both items, and every
//! button is a fixed instance that follows a fixed bullet clip. An item is added after an existing one by
//! doing what the page's author would have done:
//!
//! - the items below it move one bullet and one hit clip down: the bullets and hit clips are renamed one
//!   slot down, top first, and the last item takes the next spare bullet and a new hit clip;
//! - a new button (a copy of the page's button sprite, its label index changed) is placed and bound to
//!   its bullet;
//! - every switch on the index gets a case for it, copied from the anchor's with the anchor's button,
//!   hit clip and help index replaced by its own;
//! - in the key handler, the anchor's Down and the next item's Up lead to it, its Up leads to the anchor,
//!   its Down is the anchor's old Down, and its Enter sends FSCommand `qmMenuItem` with the item's event;
//! - its label and help line are a new label-array entry, set before every platform's labels;
//! - its label art is its button's text clips, copies of the template's whose shapes read the item's
//!   slot of a texture qm adds beside the retail one ([`LabelArt`]); [`label_texture`] sets the labels
//!   into it in the lettering, one copy per language.
//!
//! Every edit is made on the movie's own records (`gfx_actions::Program`) and tags (`gfx_edit`), found by
//! what they do, never by byte offset. A page whose scripts do not have the shape these steps expect is
//! refused with what was not found.

use crate::label_art;
use mercs2_formats::gfx_actions::{Op, Program, PushValue, Switch, Value};
use mercs2_formats::gfx_edit::{self, Line, Track};
use mercs2_formats::gfx_stream::{TagBody, TagStream};
use serde::Deserialize;

/// The catalogue, `data/menus.toml`.
pub const CATALOGUE: &str = include_str!("../data/menus.toml");

/// The FSCommand an added item's Enter sends; its argument is the item's event.
pub const ITEM_COMMAND: &str = "qmMenuItem";

const PUSH: u8 = 0x96;
const POP: u8 = 0x17;
const GET_VARIABLE: u8 = 0x1c;
const SET_VARIABLE: u8 = 0x1d;
const CALL_FUNCTION: u8 = 0x3d;
const GET_MEMBER: u8 = 0x4e;
const SET_MEMBER: u8 = 0x4f;
const EQUALS2: u8 = 0x49;
const NOT: u8 = 0x12;
const IF: u8 = 0x9d;
const JUMP: u8 = 0x99;
const GET_URL2: u8 = 0x9a;
const DEFINE_FUNCTION: u8 = 0x9b;
const DEFINE_FUNCTION2: u8 = 0x8e;

const KEY_UP: u32 = 38;
const KEY_DOWN: u32 = 40;
const KEY_ENTER: u32 = 13;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalogue {
    page: Vec<Page>,
}

/// One page of the catalogue.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: String,
    pub movie: String,
    index: String,
    container: u16,
    button_template: u16,
    label_frame: usize,
    template_label: u32,
    labels: String,
    first_label: u32,
    /// The texture the added items' labels are set into.
    pub label_art: String,
    button_depths: [u16; 2],
    rollover_frame: usize,
    rollover_depths: [u16; 2],
    spare_bullets: Vec<u16>,
    scripts: Scripts,
    #[serde(rename = "item")]
    items: Vec<RetailItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scripts {
    keys: Block,
    mouse: Block,
    leave: Block,
    labels: Block,
    bind: Block,
    off: Vec<Block>,
    highlight: Vec<Block>,
}

/// A `DoAction`: the `block`th of `frame` of the root, or of `sprite`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    sprite: Option<u16>,
    frame: usize,
    block: usize,
}

impl Block {
    fn line(&self) -> Line {
        self.sprite.map_or(Line::Root, Line::Sprite)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetailItem {
    key: String,
    index: u32,
    button: String,
    help: u32,
    rollover: String,
    bullet: u16,
}

/// The catalogue's pages.
pub fn pages() -> Result<Vec<Page>, String> {
    let c: Catalogue = toml::from_str(CATALOGUE).map_err(|e| format!("data/menus.toml: {e}"))?;
    Ok(c.page)
}

/// The page `id` of the catalogue.
pub fn page(id: &str) -> Result<Page, String> {
    pages()?.into_iter().find(|p| p.id == id).ok_or_else(|| format!("no menu page {id:?} in data/menus.toml"))
}

impl Page {
    /// How many items can be added to the page: one per spare bullet below its last item, which the
    /// items below an added one move down onto.
    pub fn capacity(&self) -> usize {
        self.spare_bullets.len()
    }
}

/// One item to add: its label's stringdb key (`label`), the key of its help line (none: an empty line),
/// the event its Enter sends, and the label key of the item it goes after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewItem {
    pub label: String,
    pub help: Option<String>,
    pub event: String,
    pub after: String,
}

/// An item of the page as it is being built.
#[derive(Debug, Clone)]
struct Item {
    key: u32,
    index: u32,
    button: String,
    help: u32,
    rollover: String,
    /// The name of the bullet its button follows.
    bullet: String,
}

/// One insert: the items as they are, the anchor's position and the item.
struct Insert<'a> {
    live: &'a [Item],
    /// The bullet depths of `live`, in order, then the spare bullet the last item moves onto.
    bullets: &'a [u16],
    p: usize,
    item: &'a Item,
}

impl Insert<'_> {
    fn anchor(&self) -> &Item {
        &self.live[self.p]
    }

    fn next(&self) -> Option<&Item> {
        self.live.get(self.p + 1)
    }
}

/// A string key's hash, as the string tables key it ([`mercs2_formats::stringdb::key_hash`]: a bracketed
/// or bare name, or `0x…` the hash itself).
pub fn key_hash(key: &str) -> u32 {
    mercs2_formats::stringdb::key_hash(key)
}

/// The text a label array holds for stringdb key `key`: the engine shows the string of `[0x<hash>]`.
fn key_text(key: &str) -> Vec<u8> {
    format!("[0x{:08x}]", key_hash(key)).into_bytes()
}

/// `movie` with `items` added to `page`, in order: each goes after the item `after` names, which may be
/// one added before it. Also where their labels go on the texture the movie now reads beside the retail
/// one, for [`label_texture`].
pub fn add_items(page: &Page, movie: &[u8], items: &[NewItem]) -> Result<(Vec<u8>, LabelArt), String> {
    let mut s = TagStream::parse(movie)?;
    check_page(page, &s)?;
    let def = mercs2_formats::gfx_def::MovieDef::decode(&s)?;
    let art = read_label_art(page, &s, &def, items.len())?;
    let image = next_id(&s)?;
    let side = |v: usize| u16::try_from(v).map_err(|_| format!("{}: a label texture side of {v} texels", page.id));
    gfx_edit::add_external_image(&mut s, art.image, image, side(art.size[0])?, side(art.size[1])?, &art.name)?;
    let inner = Line::Sprite(page.container);
    let mut live = Vec::with_capacity(page.items.len() + items.len());
    for i in &page.items {
        let bullet = track_at(&s, inner, i.bullet, 0)?.name.ok_or_else(|| format!("{}: the bullet at depth {} has no name", page.id, i.bullet))?;
        if !binds(&s, page, &i.button, &bullet)? {
            return Err(format!("{}: {} is not bound to {bullet}", page.id, i.button));
        }
        live.push(Item { key: key_hash(&i.key), index: i.index, button: i.button.clone(), help: i.help, rollover: i.rollover.clone(), bullet });
    }
    let mut bullets: Vec<u16> = page.items.iter().map(|i| i.bullet).collect();
    let mut spare = page.spare_bullets.iter();
    for (k, new) in items.iter().enumerate() {
        let fail = |e: String| format!("{}: the item {} after {}: {e}", page.id, new.label, new.after);
        let after = key_hash(&new.after);
        let p = live.iter().position(|i| i.key == after).ok_or_else(|| fail(format!("{} is no item of the page", new.after)))?;
        if live.iter().any(|i| i.key == key_hash(&new.label)) {
            return Err(fail("the page already has an item with that label".into()));
        }
        let k = u16::try_from(k).map_err(|_| fail("too many items".into()))?;
        let index = live.iter().map(|i| i.index).max().unwrap_or(0) + 1;
        let button = format!("qm_item{index}_btn");
        let item = Item {
            key: key_hash(&new.label),
            index,
            bullet: format!("qm_item{index}bullet_mc"),
            button,
            help: page.first_label + u32::from(k),
            rollover: format!("qm_item{index}_rollover"),
        };
        bullets.push(*spare.next().ok_or_else(|| fail("no spare bullet is left below the last item".into()))?);
        let at = Insert { live: &live, bullets: &bullets, p, item: &item };
        let button = layout(page, &mut s, &at).map_err(fail)?;
        label_clips(&mut s, button, &art, image, usize::from(k)).map_err(fail)?;
        scripts(page, &mut s, &at, new).map_err(fail)?;
        // The items below took the next bullet's name down: each now follows the bullet it was given.
        live.insert(p + 1, item);
    }
    let out = s.file()?;
    mercs2_formats::gfx_actions::check_decodes(&out)?;
    Ok((out, art))
}

/// What the template button's text clips show of the page's retail label texture, and where the added
/// items' labels go on the texture qm adds beside it (the page's `label_art`): label `k` in slot `k`, its
/// dark state (the off clip) in rows `slot(k, 0)..`, its bright state (the over clip) in `slot(k, 1)..`.
#[derive(Debug, Clone)]
pub struct LabelArt {
    /// The retail texture the template's shapes read: its external image's name and size.
    pub retail: String,
    pub retail_size: [usize; 2],
    /// The added texture's name and size, and how many labels it holds.
    pub name: String,
    pub size: [usize; 2],
    pub labels: usize,
    /// The label keys of the retail items whose buttons read the retail texture, in the order of their
    /// labels on it.
    pub retail_keys: Vec<u32>,
    /// The retail external image's character id.
    image: u16,
    /// The template's off clip, then its over clip.
    windows: [Window; 2],
}

/// A text clip of a button: the bitmap shape it shows at `depth` from frame `first`, and the texels of
/// the texture that shape shows.
#[derive(Debug, Clone, Copy)]
struct Window {
    clip: u16,
    depth: u16,
    first: usize,
    shape: u16,
    image: u16,
    top: usize,
    rows: usize,
    left: usize,
    cols: usize,
    /// The fill's twips per texel row.
    twips_per_row: f64,
}

impl LabelArt {
    /// The rows a slot takes: the taller window.
    fn slot_rows(&self) -> usize {
        self.windows[0].rows.max(self.windows[1].rows)
    }

    /// The first row of label `k`'s window in `state` (0 the off clip, 1 the over clip).
    fn slot(&self, k: usize, state: usize) -> usize {
        (2 * k + state) * self.slot_rows()
    }
}

/// The off and over text clips of button sprite `button`: the `text_mc` it places in frame 0 and the one
/// it places in the page's label frame.
fn windows(page: &Page, s: &TagStream, def: &mercs2_formats::gfx_def::MovieDef, button: u16) -> Result<[Window; 2], String> {
    let text: Vec<Track> = gfx_edit::tracks(gfx_edit::timeline(s, Line::Sprite(button))?)?.into_iter().filter(|t| t.name.as_deref() == Some("text_mc")).collect();
    let clip = |frame: usize| text.iter().find(|t| t.first == frame).map(|t| t.character).ok_or_else(|| format!("button sprite {button} places no text_mc in frame {frame}"));
    Ok([window(s, def, clip(0)?)?, window(s, def, clip(page.label_frame)?)?])
}

/// Text clip `clip`'s one shape, which must have one bitmap fill, and the texels it shows.
fn window(s: &TagStream, def: &mercs2_formats::gfx_def::MovieDef, clip: u16) -> Result<Window, String> {
    use mercs2_formats::gfx_def::Character;
    let shapes: Vec<Track> = gfx_edit::tracks(gfx_edit::timeline(s, Line::Sprite(clip))?)?
        .into_iter()
        .filter(|t| matches!(def.characters.get(&t.character), Some(Character::Shape(_))))
        .collect();
    let [t] = &shapes[..] else { return Err(format!("text clip {clip} shows {} shapes, not one", shapes.len())) };
    let shape = gfx_edit::bitmap_shape(s, t.character)?;
    let m = shape.fill.matrix;
    if m.rotate.is_some() {
        return Err(format!("shape {}'s fill is rotated", shape.id));
    }
    let [sx, sy] = m.scale.ok_or_else(|| format!("shape {}'s fill is not scaled", shape.id))?.values.map(|v| f64::from(v) / 65536.0);
    if sx <= 0.0 || sy <= 0.0 {
        return Err(format!("shape {}'s fill is mirrored or flat", shape.id));
    }
    let [tx, ty] = m.translate.values.map(f64::from);
    let [x0, x1, y0, y1] = shape.bounds.map(f64::from);
    let (top, bottom, left, right) = (((y0 - ty) / sy).floor(), ((y1 - ty) / sy).ceil(), ((x0 - tx) / sx).floor(), ((x1 - tx) / sx).ceil());
    if top < 0.0 || left < 0.0 {
        return Err(format!("shape {} shows past its texture's top or left", shape.id));
    }
    Ok(Window {
        clip,
        depth: t.depth,
        first: t.first,
        shape: shape.id,
        image: shape.fill.image,
        top: top as usize,
        rows: (bottom - top) as usize,
        left: left as usize,
        cols: (right - left) as usize,
        twips_per_row: sy,
    })
}

/// The page's label art, for `labels` added items: the template's windows on the retail texture, and the
/// retail items reading that texture in the order of their labels on it.
fn read_label_art(page: &Page, s: &TagStream, def: &mercs2_formats::gfx_def::MovieDef, labels: usize) -> Result<LabelArt, String> {
    let fail = |e: String| format!("{}: label art: {e}", page.id);
    let template = windows(page, s, def, page.button_template).map_err(fail)?;
    let image = template[0].image;
    if template[1].image != image {
        return Err(fail(format!("the template's text clips read images {} and {}", image, template[1].image)));
    }
    if template[0].top >= template[1].top {
        return Err(fail("the template's off window is not above its over window (dark band, then bright)".into()));
    }
    let retail = gfx_edit::external_image(s, image).map_err(fail)?;
    let retail_size = [usize::from(retail.width), usize::from(retail.height)];
    for w in &template {
        if w.left + w.cols > retail_size[0] || w.top + w.rows > retail_size[1] {
            return Err(fail(format!("shape {} shows past its {}x{} texture", w.shape, retail_size[0], retail_size[1])));
        }
    }
    let inner = gfx_edit::tracks(gfx_edit::timeline(s, Line::Sprite(page.container))?)?;
    let mut on_texture = Vec::new();
    for i in &page.items {
        let button = inner
            .iter()
            .find(|t| t.first == 0 && t.name.as_deref() == Some(i.button.as_str()))
            .ok_or_else(|| fail(format!("sprite {} places no {} in frame 0", page.container, i.button)))?;
        let w = windows(page, s, def, button.character).map_err(|e| fail(format!("{}: {e}", i.button)))?;
        if w[0].image == image {
            on_texture.push((w[0].top, key_hash(&i.key)));
        }
    }
    on_texture.sort_unstable();
    let mut art = LabelArt {
        retail: String::from_utf8(retail.export_name.clone()).map_err(|_| fail(format!("image {image}'s name is not UTF-8")))?,
        retail_size,
        name: page.label_art.clone(),
        size: [retail_size[0], 0],
        labels,
        retail_keys: on_texture.into_iter().map(|(_, k)| k).collect(),
        image,
        windows: template,
    };
    art.size[1] = art.slot(labels, 0).max(4).next_power_of_two();
    Ok(art)
}

/// The new button sprite `button` (a copy of the template) shows label `k` of the added texture `image`:
/// each of its text clips is a copy of the template's whose shape is a copy reading `image`, its fill
/// moved by whole rows from the template's window to the label's slot.
fn label_clips(s: &mut TagStream, button: u16, art: &LabelArt, image: u16, k: usize) -> Result<(), String> {
    let line = Line::Sprite(button);
    for (state, w) in art.windows.iter().enumerate() {
        let clip = next_id(s)?;
        gfx_edit::clone_sprite(s, w.clip, clip)?;
        let shape = next_id(s)?;
        let rows = w.top as f64 - art.slot(k, state) as f64;
        let dy = twips(rows * w.twips_per_row)?;
        let mut moved = Ok(());
        gfx_edit::clone_bitmap_shape(s, w.shape, shape, |f| {
            f.image = image;
            match f.matrix.translate.values[1].checked_add(dy) {
                Some(y) => f.matrix.translate.values[1] = y,
                None => moved = Err(format!("shape {}'s fill moved past i32 twips", w.shape)),
            }
        })?;
        moved?;
        gfx_edit::recharacter(s, Line::Sprite(clip), w.depth, w.first, shape)?;
        let uses: Vec<Track> = gfx_edit::tracks(gfx_edit::timeline(s, line)?)?.into_iter().filter(|t| t.character == w.clip).collect();
        if uses.is_empty() {
            return Err(format!("button sprite {button} places no text clip {}", w.clip));
        }
        for t in uses {
            gfx_edit::recharacter(s, line, t.depth, t.first, clip)?;
        }
    }
    Ok(())
}

/// Label texture `img` (from [`label_texture`]) as a patch block of its own named `art.name`, in BC3: the
/// labels' ring fades out in alpha.
pub fn label_texture_block(art: &LabelArt, img: &label_art::Image) -> Result<mercs2_formats::patch_wad::PatchBlock, String> {
    use mercs2_formats::texture_encode::{encode_bc3, mip_chain};
    if [img.width, img.height] != art.size {
        return Err(format!("a {}x{} image for the {}x{} {}", img.width, img.height, art.size[0], art.size[1], art.name));
    }
    let px: Vec<f32> = img.px.iter().flat_map(|t| t.map(f32::from)).collect();
    let body = mip_chain(img.width, img.height, 4, &px, encode_bc3);
    Ok(crate::build::resident_texture_block(&art.name, img.width, img.height, mercs2_formats::texture::TexFormat::Bc3, &body)?.0)
}

/// The added items' labels `texts` (in the order the items were added) set in the lettering into the
/// texture `art` names. `retail` is the player's copy of the retail label texture in the texts' language
/// and `retail_text` gives each retail label's text (by key) in it: the grain and the outline are
/// measured from them, and each label is centred where the template's is, its cap on the template's cap
/// line. Labels are set in capitals; a character with no letter, or a label wider than the button shows,
/// is refused, naming it.
pub fn label_texture(art: &LabelArt, retail: &label_art::Image, retail_text: &dyn Fn(u32) -> Option<String>, texts: &[String]) -> Result<label_art::Image, String> {
    use label_art::{cap_line, grain_columns, lines, measure_outline, set_label, State, OUTLINE_REACH, RETAIL_SETTING};
    if texts.len() != art.labels {
        return Err(format!("{} labels for a texture of {}", texts.len(), art.labels));
    }
    if [retail.width, retail.height] != art.retail_size {
        return Err(format!("{} is {}x{}; the movie reads it as {}x{}", art.retail, retail.width, retail.height, art.retail_size[0], art.retail_size[1]));
    }
    let retail_texts: Vec<String> = art.retail_keys.iter().map(|k| retail_text(*k).ok_or_else(|| format!("no text for the retail label 0x{k:08x}"))).collect::<Result<_, _>>()?;
    let refs: Vec<&str> = retail_texts.iter().map(String::as_str).collect();
    let lines = lines(retail, &refs).map_err(|e| format!("{}: {e}", art.retail))?;
    let [off, over] = art.windows;
    let template = lines
        .iter()
        .find(|l| l.dark.0 >= off.top && l.dark.1 <= off.top + off.rows && l.bright.0 >= over.top && l.bright.1 <= over.top + over.rows)
        .ok_or_else(|| format!("no label of {} lies in the template's windows", art.retail))?;
    let (first, last) = (template.letters.first().ok_or("a template label of no letters")?, template.letters.last().ok_or("a template label of no letters")?);
    let centre = (first.x0 + last.x1) as f64 / 2.0;
    let cap = cap_line(template);
    let caps = [cap, cap + (template.bright.0 - template.dark.0)];
    let grain = grain_columns(&lines);
    let outlines = (measure_outline(retail, &lines, State::Dark), measure_outline(retail, &lines, State::Bright));
    let glyph = |ch: char| label_art::letter(ch);
    let mut out = label_art::Image::new(art.size[0], art.size[1]);
    for (k, text) in texts.iter().enumerate() {
        let fail = |e: String| format!("label {text:?}: {e}");
        let set = set_label(&text.to_uppercase(), &glyph, &grain, (&outlines.0, &outlines.1), RETAIL_SETTING).map_err(fail)?;
        for (state, (img, w)) in [(&set.dark, off), (&set.bright, over)].into_iter().enumerate() {
            let slot = art.slot(k, state);
            let y = (slot + caps[state]).checked_sub(w.top + set.cap_row).ok_or_else(|| fail("its marks reach above its slot".into()))?;
            let x = (centre - img.width as f64 / 2.0).round();
            let letters = img.width - 2 * OUTLINE_REACH;
            if x < w.left as f64 || x as usize + img.width > w.left + w.cols {
                return Err(fail(format!("its letters are {letters} texels wide; the button shows {}", w.cols - 2 * OUTLINE_REACH)));
            }
            if y < slot || y + img.height > slot + art.slot_rows() {
                return Err(fail(format!("it is {} texels high; its slot is {}", img.height, art.slot_rows())));
            }
            for py in 0..img.height {
                for px in 0..img.width {
                    out.set(x as usize + px, y + py, img.at(px, py));
                }
            }
        }
    }
    Ok(out)
}

/// The page's items and clips are where the catalogue says.
fn check_page(page: &Page, s: &TagStream) -> Result<(), String> {
    let inner = gfx_edit::tracks(gfx_edit::timeline(s, Line::Sprite(page.container))?)?;
    let root = gfx_edit::tracks(gfx_edit::timeline(s, Line::Root)?)?;
    for i in &page.items {
        if !inner.iter().any(|t| t.first == 0 && t.name.as_deref() == Some(i.button.as_str())) {
            return Err(format!("{}: sprite {} places no {} in frame 0", page.id, page.container, i.button));
        }
        if !root.iter().any(|t| t.on_stage(page.rollover_frame) && t.name.as_deref() == Some(i.rollover.as_str())) {
            return Err(format!("{}: root frame {} has no {}", page.id, page.rollover_frame, i.rollover));
        }
    }
    for d in &page.spare_bullets {
        if !inner.iter().any(|t| t.first == 0 && t.depth == *d && t.name.is_none()) {
            return Err(format!("{}: sprite {} has no unnamed bullet at depth {d}", page.id, page.container));
        }
    }
    Ok(())
}

/// The track at `depth` on stage in `frame` of `line`.
fn track_at(s: &TagStream, line: Line, depth: u16, frame: usize) -> Result<Track, String> {
    gfx_edit::tracks(gfx_edit::timeline(s, line)?)?
        .into_iter()
        .find(|t| t.depth == depth && t.on_stage(frame))
        .ok_or_else(|| format!("nothing at depth {depth} in frame {frame} of {line:?}"))
}

/// The depth of the instance named `name` on stage in `frame` of `line`.
fn depth_of(s: &TagStream, line: Line, name: &str, frame: usize) -> Result<u16, String> {
    gfx_edit::tracks(gfx_edit::timeline(s, line)?)?
        .into_iter()
        .find(|t| t.on_stage(frame) && t.name.as_deref() == Some(name))
        .map(|t| t.depth)
        .ok_or_else(|| format!("no {name} in frame {frame} of {line:?}"))
}

/// The first depth from `start`, by `step`, that no instance of `line` holds while the one at `depth` in
/// `frame` is on stage.
fn free_depth(s: &TagStream, line: Line, depth: u16, frame: usize, start: u16, step: u16) -> Result<u16, String> {
    let all = gfx_edit::tracks(gfx_edit::timeline(s, line)?)?;
    let me = all.iter().find(|t| t.depth == depth && t.on_stage(frame)).ok_or_else(|| format!("nothing at depth {depth} in frame {frame} of {line:?}"))?;
    let (first, removed) = (me.first, me.removed.unwrap_or(usize::MAX));
    let mut d = start;
    loop {
        if !all.iter().any(|t| t.depth == d && t.first < removed && first < t.removed.unwrap_or(usize::MAX)) {
            return Ok(d);
        }
        d = d.checked_add(step.max(1)).ok_or_else(|| format!("no depth from {start} up is free in {line:?}"))?;
    }
}

/// The translation of the tag that places the instance at `depth` in `frame` of `line`.
fn placed_at(s: &TagStream, line: Line, depth: u16, frame: usize) -> Result<[f64; 2], String> {
    let t = track_at(s, line, depth, frame)?;
    let tags = gfx_edit::timeline(s, line)?;
    let tag = &tags[t.tags[0]];
    let TagBody::Raw(body) = &tag.body else { return Err("a placement that is a sprite".into()) };
    match mercs2_formats::gfx_tags::decode(tag.code, body)? {
        Some(mercs2_formats::gfx_tags::Typed::Place(p)) => {
            let m = p.matrix.ok_or_else(|| format!("depth {depth} of {line:?} is placed with no matrix"))?;
            Ok([f64::from(m.translate.values[0]), f64::from(m.translate.values[1])])
        }
        _ => Err(format!("tag {} of {line:?} is not a placement", t.tags[0])),
    }
}

/// How far a retail hit clip may sit from where the fitted map puts it.
const FIT_TWIPS: f64 = 100.0;

/// The least-squares affine map from each bullet's position to its hit clip's. It must place every retail
/// hit clip to within [`FIT_TWIPS`]: the hit clips sit where the bullets do, through the page's own
/// placement of the bullets' sprite.
fn fit(pairs: &[([f64; 2], [f64; 2])]) -> Result<[[f64; 3]; 2], String> {
    if pairs.len() < 3 {
        return Err(format!("{} bullet/hit-clip pairs, too few to place a hit clip by", pairs.len()));
    }
    let mut a = [[0.0f64; 3]; 3];
    let mut y = [[0.0f64; 3]; 2];
    for (b, r) in pairs {
        let v = [b[0], b[1], 1.0];
        for i in 0..3 {
            for j in 0..3 {
                a[i][j] += v[i] * v[j];
            }
            y[0][i] += v[i] * r[0];
            y[1][i] += v[i] * r[1];
        }
    }
    let solve = |rhs: [f64; 3]| -> Result<[f64; 3], String> {
        let mut m = a;
        let mut r = rhs;
        for k in 0..3 {
            let pivot = (k..3).max_by(|&i, &j| m[i][k].abs().total_cmp(&m[j][k].abs())).unwrap_or(k);
            m.swap(k, pivot);
            r.swap(k, pivot);
            if m[k][k].abs() < 1e-9 {
                return Err("the bullets are on one line; no map fits".into());
            }
            for i in k + 1..3 {
                let f = m[i][k] / m[k][k];
                for j in k..3 {
                    m[i][j] -= f * m[k][j];
                }
                r[i] -= f * r[k];
            }
        }
        let mut x = [0.0; 3];
        for i in (0..3).rev() {
            x[i] = (r[i] - (i + 1..3).map(|j| m[i][j] * x[j]).sum::<f64>()) / m[i][i];
        }
        Ok(x)
    };
    let map = [solve(y[0])?, solve(y[1])?];
    for (b, r) in pairs {
        let got = apply(&map, *b);
        let off = (got[0] - r[0]).hypot(got[1] - r[1]);
        if off > FIT_TWIPS {
            return Err(format!("the bullet-to-hit-clip map misses a retail hit clip by {off:.0} twips"));
        }
    }
    Ok(map)
}

fn apply(map: &[[f64; 3]; 2], b: [f64; 2]) -> [f64; 2] {
    [map[0][0] * b[0] + map[0][1] * b[1] + map[0][2], map[1][0] * b[0] + map[1][1] * b[1] + map[1][2]]
}

fn twips(v: f64) -> Result<i32, String> {
    if v.abs() < 1e9 {
        Ok(v.round() as i32)
    } else {
        Err(format!("an offset of {v} twips"))
    }
}

/// Moves the items below the anchor one bullet and one hit clip down, and places the new button: its
/// sprite's id.
fn layout(page: &Page, s: &mut TagStream, at: &Insert<'_>) -> Result<u16, String> {
    let inner = Line::Sprite(page.container);
    let rf = page.rollover_frame;
    let (live, item, p) = (at.live, at.item, at.p);
    let next_bullet = *at.bullets.last().ok_or("no bullets")?;
    // The new last hit clip: a copy of the last one, where the fitted map puts the next bullet's.
    let mut pairs = Vec::new();
    for (i, it) in live.iter().enumerate() {
        pairs.push((placed_at(s, inner, at.bullets[i], 0)?, placed_at(s, Line::Root, depth_of(s, Line::Root, &it.rollover, rf)?, rf)?));
    }
    let map = fit(&pairs)?;
    let last_clip = depth_of(s, Line::Root, &live.last().ok_or("a page with no item")?.rollover, rf)?;
    let to = apply(&map, placed_at(s, inner, next_bullet, 0)?);
    let from = placed_at(s, Line::Root, last_clip, rf)?;
    let new_clip = free_depth(s, Line::Root, last_clip, rf, page.rollover_depths[0], page.rollover_depths[1])?;
    let temp = format!("{}_new", item.rollover);
    gfx_edit::copy(s, Line::Root, last_clip, rf, new_clip, Some(&temp), twips(to[0] - from[0])?, twips(to[1] - from[1])?)?;
    // From the anchor's next item down, each bullet and hit clip takes the name of the one above it, top
    // first; the topmost takes the new item's.
    let mut bullet_names = vec![item.bullet.clone()];
    bullet_names.extend(live[p + 1..].iter().map(|i| i.bullet.clone()));
    for (d, name) in at.bullets[p + 1..].iter().zip(&bullet_names) {
        gfx_edit::rename(s, inner, *d, 0, name)?;
    }
    let mut clips: Vec<u16> = live[p + 1..].iter().map(|i| depth_of(s, Line::Root, &i.rollover, rf)).collect::<Result<_, _>>()?;
    clips.push(new_clip);
    let mut clip_names = vec![item.rollover.clone()];
    clip_names.extend(live[p + 1..].iter().map(|i| i.rollover.clone()));
    for (d, name) in clips.iter().zip(&clip_names) {
        gfx_edit::rename(s, Line::Root, *d, rf, name)?;
    }
    // The button: a copy of the template sprite that reads its own label, placed like the anchor's.
    let id = next_id(s)?;
    gfx_edit::clone_sprite(s, page.button_template, id)?;
    edit(s, Line::Sprite(id), page.label_frame, 0, |prog, pool| {
        if number_pushes(prog, pool, 0, prog.ops.len(), f64::from(page.template_label))? != 1 {
            return Err(format!("button sprite {}'s frame {} does not push {} once", page.button_template, page.label_frame, page.template_label));
        }
        set_numbers(prog, pool, 0, prog.ops.len(), f64::from(page.template_label), f64::from(item.help))
    })?;
    let anchor_button = depth_of(s, inner, &at.anchor().button, 0)?;
    let depth = free_depth(s, inner, anchor_button, 0, page.button_depths[0], page.button_depths[1])?;
    gfx_edit::copy(s, inner, anchor_button, 0, depth, Some(&item.button), 0, 0)?;
    gfx_edit::recharacter(s, inner, depth, 0, id)?;
    Ok(id)
}

/// The next character id above every one the movie takes.
fn next_id(s: &TagStream) -> Result<u16, String> {
    gfx_edit::character_ids(s)?.last().copied().unwrap_or(0).checked_add(1).ok_or_else(|| "no character id is free".to_string())
}

/// Edits the `block`th `DoAction` of `frame` of `line` as a program.
fn edit(s: &mut TagStream, line: Line, frame: usize, block: usize, f: impl FnOnce(&mut Program, &[Vec<u8>]) -> Result<(), String>) -> Result<(), String> {
    let mut blocks = gfx_edit::do_actions_mut(s, line, frame)?;
    let n = blocks.len();
    let body = blocks.get_mut(block).ok_or_else(|| format!("{line:?} frame {frame} has {n} DoAction(s), not a #{block}"))?;
    let mut p = Program::decode(body)?;
    let pool = p.pool()?;
    f(&mut p, &pool).map_err(|e| format!("{line:?} frame {frame} DoAction #{block}: {e}"))?;
    **body = p.encode()?;
    Ok(())
}

fn edit_block(s: &mut TagStream, b: &Block, f: impl FnOnce(&mut Program, &[Vec<u8>]) -> Result<(), String>) -> Result<(), String> {
    edit(s, b.line(), b.frame, b.block, f)
}

fn str_value(s: &str) -> Value {
    Value::Str(s.as_bytes().to_vec())
}

/// Whether record `i` is a `Push` of exactly `values`.
fn pushes(p: &Program, pool: &[Vec<u8>], i: usize, values: &[Value]) -> bool {
    p.ops.get(i).is_some_and(|o| o.code == PUSH) && p.push_at(i, pool).is_ok_and(|v| v == values)
}

/// How many `Push` values in records `from..to` are the number `n`.
fn number_pushes(p: &Program, pool: &[Vec<u8>], from: usize, to: usize, n: f64) -> Result<usize, String> {
    let mut count = 0;
    for i in from..to {
        if p.ops[i].code == PUSH {
            count += p.push_at(i, pool)?.iter().filter(|v| v.number() == Some(n)).count();
        }
    }
    Ok(count)
}

/// Every `Push` value in records `from..to` that is the number `a` becomes `b`.
fn set_numbers(p: &mut Program, pool: &[Vec<u8>], from: usize, to: usize, a: f64, b: f64) -> Result<(), String> {
    for i in from..to {
        if p.ops[i].code == PUSH {
            p.edit_push(i, pool, &mut |vs| {
                for v in vs.iter_mut().filter(|v| v.number() == Some(a)) {
                    *v = Value::Number(b);
                }
            })?;
        }
    }
    Ok(())
}

/// Every pushed string `a` in records `from..to` becomes `b`.
fn set_strings(p: &mut Program, pool: &[Vec<u8>], from: usize, to: usize, a: &str, b: &str) -> Result<(), String> {
    for i in from..to {
        if p.ops[i].code == PUSH {
            p.edit_push(i, pool, &mut |vs| {
                for v in vs.iter_mut().filter(|v| **v == str_value(a)) {
                    *v = str_value(b);
                }
            })?;
        }
    }
    Ok(())
}

/// Whether record `i` is a `Push` whose last values are `2, "automaticScrolling"`: a help-bar call, whose
/// earlier values are help indices.
fn is_help_call(p: &Program, pool: &[Vec<u8>], i: usize) -> Result<bool, String> {
    if p.ops[i].code != PUSH {
        return Ok(false);
    }
    let v = p.push_at(i, pool)?;
    Ok(v.len() >= 3 && v[v.len() - 1] == str_value("automaticScrolling") && v[v.len() - 2].number() == Some(2.0))
}

/// In every help-bar call in records `from..to`, help index `a` becomes `b`.
fn set_help(p: &mut Program, pool: &[Vec<u8>], from: usize, to: usize, a: u32, b: u32) -> Result<(), String> {
    for i in from..to {
        if is_help_call(p, pool, i)? {
            p.edit_push(i, pool, &mut |vs| {
                let n = vs.len();
                for v in vs[..n - 2].iter_mut().filter(|v| v.number() == Some(f64::from(a))) {
                    *v = Value::Number(f64::from(b));
                }
            })?;
        }
    }
    Ok(())
}

/// A switch on the page's index, and case `index`'s body in it.
struct Case {
    switch: Switch,
    body: usize,
    body_end: usize,
}

/// Every switch on `var` in `p` with a case `index`, in record order.
fn cases(p: &Program, pool: &[Vec<u8>], var: &str, index: u32) -> Result<Vec<Case>, String> {
    let mut out = Vec::new();
    for s in p.switches(var, pool)? {
        let Ok(c) = s.case(f64::from(index)) else { continue };
        let body = c.body;
        let body_end = s.body_end(f64::from(index))?;
        out.push(Case { switch: s, body, body_end });
    }
    Ok(out)
}

/// Adds `item`'s case to the switch of `c`: a copy of the anchor's body with the anchor's button, hit
/// clip and help index replaced by the item's.
fn add_case_like(p: &mut Program, pool: &[Vec<u8>], c: &Case, anchor: &Item, item: &Item) -> Result<(), String> {
    let (body, outside) = p.clone_range(c.body, c.body_end)?;
    let n = body.len();
    let start = p.add_case(&c.switch, &Value::Number(f64::from(item.index)), body, &outside)?;
    set_strings(p, pool, start, start + n, &anchor.button, &item.button)?;
    set_strings(p, pool, start, start + n, &anchor.rollover, &item.rollover)?;
    set_help(p, pool, start, start + n, anchor.help, item.help)
}

/// Adds `item`'s case to every switch on the page's index in block `b`; how many.
fn add_cases(page: &Page, s: &mut TagStream, b: &Block, anchor: &Item, item: &Item) -> Result<usize, String> {
    let mut added = 0;
    loop {
        let mut did = false;
        edit_block(s, b, |p, pool| {
            if let Some(c) = cases(p, pool, &page.index, anchor.index)?.into_iter().find(|c| c.switch.case(f64::from(item.index)).is_err()) {
                add_case_like(p, pool, &c, anchor, item)?;
                did = true;
            }
            Ok(())
        })?;
        if !did {
            return Ok(added);
        }
        added += 1;
    }
}

/// The records of one move between two items: the `Push`es of the button left (`_out`), of the button
/// entered (`_over`), of the index set and of the help-bar call.
struct Move {
    out: usize,
    over: usize,
    index: usize,
    help: usize,
}

/// What a move is set to: the button left and entered, the index set, the help line left and shown.
struct MoveTo<'a> {
    out: &'a str,
    over: &'a str,
    index: u32,
    left: u32,
    shown: u32,
}

/// The `Push` of the button name after a `_out`/`_over` push at `i`: `Push "<button>"; GetMember; Push
/// "gotoAndPlay"` within the next few records.
fn button_after(p: &Program, pool: &[Vec<u8>], i: usize, to: usize) -> Option<usize> {
    (i + 1..(i + 8).min(to)).find(|&j| {
        p.ops[j].code == PUSH && p.push_at(j, pool).is_ok_and(|v| v.len() == 1) && p.ops.get(j + 1).is_some_and(|o| o.code == GET_MEMBER) && pushes(p, pool, j + 2, &[str_value("gotoAndPlay")])
    })
}

/// The moves in records `from..to`: an `_out`, then an `_over`, then the index set and the help-bar call
/// in either order.
fn moves(p: &Program, pool: &[Vec<u8>], var: &str, from: usize, to: usize) -> Result<Vec<Move>, String> {
    let mut out = Vec::new();
    let (mut left, mut entered, mut index, mut help) = (None, None, None, None);
    for i in from..to {
        if p.ops[i].code != PUSH {
            continue;
        }
        let v = p.push_at(i, pool)?;
        if v.first() == Some(&str_value("_out")) {
            (left, entered, index, help) = (button_after(p, pool, i, to), None, None, None);
        } else if v.first() == Some(&str_value("_over")) && left.is_some() {
            entered = button_after(p, pool, i, to);
        } else if entered.is_some() && v.len() == 2 && v[0] == str_value(var) && v[1].number().is_some() && p.ops.get(i + 1).is_some_and(|o| o.code == SET_VARIABLE) {
            index = Some(i);
        } else if entered.is_some() && v.len() == 4 && is_help_call(p, pool, i)? {
            help = Some(i);
        }
        if let (Some(o), Some(e), Some(x), Some(h)) = (left, entered, index, help) {
            out.push(Move { out: o, over: e, index: x, help: h });
            (left, entered, index, help) = (None, None, None, None);
        }
    }
    Ok(out)
}

fn set_move(p: &mut Program, pool: &[Vec<u8>], m: &Move, to: &MoveTo<'_>) -> Result<(), String> {
    p.edit_push(m.out, pool, &mut |v| v[0] = str_value(to.out))?;
    p.edit_push(m.over, pool, &mut |v| v[0] = str_value(to.over))?;
    p.edit_push(m.index, pool, &mut |v| v[1] = Value::Number(f64::from(to.index)))?;
    p.edit_push(m.help, pool, &mut |v| {
        v[0] = Value::Number(f64::from(to.left));
        v[1] = Value::Number(f64::from(to.shown));
    })?;
    Ok(())
}

/// The case `index` of the key handler's switch on the page's index.
fn key_case(page: &Page, p: &Program, pool: &[Vec<u8>], index: u32) -> Result<Case, String> {
    cases(p, pool, &page.index, index)?.into_iter().next().ok_or_else(|| format!("the key handler has no switch on {} with a case {index}", page.index))
}

/// Key `code`'s branch of the keyCode switch at the start of `c`'s body, without its closing `Jump`.
fn key_branch(p: &Program, pool: &[Vec<u8>], c: &Case, code: u32) -> Result<(usize, usize), String> {
    let sw = p
        .switches("keyCode", pool)?
        .into_iter()
        .find(|s| s.cases.first().is_some_and(|k| (c.body..c.body_end).contains(&k.dispatch)))
        .ok_or("the case has no switch on keyCode")?;
    let start = sw.case(f64::from(code))?.body;
    let end = sw.body_end(f64::from(code))?;
    match p.ops.get(end.wrapping_sub(1)).map(|o| o.code) {
        Some(JUMP) if end > start => Ok((start, end - 1)),
        _ if end == start => Ok((start, start)),
        other => Err(format!("key {code}'s branch ends in {other:x?}, not a break")),
    }
}

/// The records an added item's Enter runs: the page's select sound, then FSCommand `qmMenuItem <event>`.
fn enter_records(event: &str) -> Result<Vec<Op>, String> {
    let push = |vs: &[PushValue]| -> Result<Op, String> { Ok(Op { code: PUSH, payload: mercs2_formats::gfx_actions::push_payload(vs)?, to: vec![] }) };
    let bare = |code: u8| Op { code, payload: vec![], to: vec![] };
    Ok(vec![
        push(&[PushValue::Int(0), PushValue::Str(b"soundSelect".to_vec())])?,
        bare(CALL_FUNCTION),
        bare(POP),
        push(&[PushValue::Str(format!("FSCommand:{ITEM_COMMAND}").into_bytes()), PushValue::Str(event.as_bytes().to_vec())])?,
        Op { code: GET_URL2, payload: vec![0], to: vec![] },
    ])
}

/// A copy of the statements of a move into the anchor (the next item's Up, or the item above's Down
/// when the anchor is last), positions as [`Program::clone_range`] gives them.
fn move_template(page: &Page, p: &Program, pool: &[Vec<u8>], at: &Insert<'_>) -> Result<(Vec<Op>, Vec<(usize, usize)>), String> {
    let anchor = at.anchor();
    let (from, to) = match at.next() {
        Some(n) => key_branch(p, pool, &key_case(page, p, pool, n.index)?, KEY_UP)?,
        None => {
            let above = at.p.checked_sub(1).and_then(|i| at.live.get(i)).ok_or("a page of one item")?;
            key_branch(p, pool, &key_case(page, p, pool, above.index)?, KEY_DOWN)?
        }
    };
    let m = moves(p, pool, &page.index, from, to)?
        .into_iter()
        .find(|m| pushes(p, pool, m.over, &[str_value(&anchor.button)]))
        .ok_or("no move into the anchor to copy")?;
    let first = (from..=m.out).rev().find(|&i| p.push_at(i, pool).is_ok_and(|v| v.first() == Some(&str_value("_out")))).ok_or("a move with no _out")?;
    let last = m.index.max(m.help);
    let end = (last..to).find(|&i| matches!(p.ops[i].code, POP | SET_VARIABLE)).ok_or("a move with no end")?;
    p.clone_range(first, end + 1)
}

/// The key handler: the item's case, and the moves into and out of it.
fn keys(page: &Page, s: &mut TagStream, at: &Insert<'_>, new: &NewItem) -> Result<(), String> {
    let (anchor, item) = (at.anchor(), at.item);
    edit_block(s, &page.scripts.keys, |p, pool| {
        let (template, outside) = move_template(page, p, pool, at)?;
        let n = template.len();
        let one_move = |p: &Program, from: usize| -> Result<Move, String> {
            let mut m = moves(p, pool, &page.index, from, from + n)?;
            match m.len() {
                1 => Ok(m.remove(0)),
                k => Err(format!("the copied move reads as {k} moves")),
            }
        };
        // The item's case: the anchor's, renamed.
        add_case_like(p, pool, &key_case(page, p, pool, anchor.index)?, anchor, item)?;
        // Its Up leads to the anchor.
        let (up, up_end) = key_branch(p, pool, &key_case(page, p, pool, item.index)?, KEY_UP)?;
        p.replace(up, up_end, template.clone(), &outside)?;
        let m = one_move(p, up)?;
        set_move(p, pool, &m, &MoveTo { out: &item.button, over: &anchor.button, index: anchor.index, left: item.help, shown: anchor.help })?;
        // Its Enter sends its event.
        let (enter, enter_end) = key_branch(p, pool, &key_case(page, p, pool, item.index)?, KEY_ENTER)?;
        p.replace(enter, enter_end, enter_records(&new.event)?, &[])?;
        // The anchor's Down leads to it.
        let (down, down_end) = key_branch(p, pool, &key_case(page, p, pool, anchor.index)?, KEY_DOWN)?;
        p.replace(down, down_end, template, &outside)?;
        let m = one_move(p, down)?;
        set_move(p, pool, &m, &MoveTo { out: &anchor.button, over: &item.button, index: item.index, left: anchor.help, shown: item.help })?;
        // The next item's Up leads to it.
        if let Some(nx) = at.next() {
            let (up, up_end) = key_branch(p, pool, &key_case(page, p, pool, nx.index)?, KEY_UP)?;
            let into: Vec<Move> = moves(p, pool, &page.index, up, up_end)?.into_iter().filter(|m| pushes(p, pool, m.over, &[str_value(&anchor.button)])).collect();
            if into.is_empty() {
                return Err(format!("{}'s Up does not lead to {}", nx.button, anchor.button));
            }
            for m in &into {
                set_move(p, pool, m, &MoveTo { out: &nx.button, over: &item.button, index: item.index, left: nx.help, shown: item.help })?;
            }
        }
        Ok(())
    })
}

/// The hit clip's handlers, copied from the anchor's, and doButton's help rule for the item's index.
fn mouse(page: &Page, s: &mut TagStream, at: &Insert<'_>) -> Result<(), String> {
    let (anchor, item) = (at.anchor(), at.item);
    edit_block(s, &page.scripts.mouse, |p, pool| {
        // `<rollover>.<event> = function () { … }`: Push "<rollover>"; GetVariable; Push "<event>";
        // DefineFunction…; SetMember.
        let mut copied = 0;
        let mut i = 0;
        while i + 3 < p.ops.len() {
            let handler = pushes(p, pool, i, &[str_value(&anchor.rollover)])
                && p.ops[i + 1].code == GET_VARIABLE
                && p.ops[i + 2].code == PUSH
                && matches!(p.ops[i + 3].code, DEFINE_FUNCTION | DEFINE_FUNCTION2);
            if !handler {
                i += 1;
                continue;
            }
            let end = p.ops[i + 3].to[0];
            if p.ops.get(end).map(|o| o.code) != Some(SET_MEMBER) {
                return Err(format!("{}'s handler at record {i} is not assigned", anchor.rollover));
            }
            let (copy, outside) = p.clone_range(i, end + 1)?;
            let n = copy.len();
            p.insert(end + 1, copy, &outside)?;
            set_strings(p, pool, end + 1, end + 1 + n, &anchor.rollover, &item.rollover)?;
            for j in end + 1..end + 1 + n {
                // `_root.<index> = <anchor's index>` in its onRollOver.
                if pushes(p, pool, j, &[Value::Register(1), str_value(&page.index), Value::Number(f64::from(anchor.index))]) {
                    p.edit_push(j, pool, &mut |v| v[2] = Value::Number(f64::from(item.index)))?;
                }
            }
            copied += 1;
            i = end + 1 + n;
        }
        if copied == 0 {
            return Err(format!("no handler of {} to copy", anchor.rollover));
        }
        // doButton: `if (index == v) help(h) else if … else help(19 + index)`. A test of the item's index,
        // copied from the first test, goes first.
        let first = (0..p.ops.len().saturating_sub(5))
            .find(|&i| {
                pushes(p, pool, i, &[Value::Register(1), str_value(&page.index)])
                    && p.ops[i + 1].code == GET_MEMBER
                    && p.ops[i + 2].code == PUSH
                    && p.ops[i + 3].code == EQUALS2
                    && p.ops[i + 4].code == NOT
                    && p.ops[i + 5].code == IF
            })
            .ok_or("no help rule on the index (doButton)")?;
        let value = p.push_at(first + 2, pool)?.first().and_then(Value::number).ok_or("a help test with no number")?;
        let tested = at.live.iter().find(|i| f64::from(i.index) == value).ok_or_else(|| format!("doButton tests index {value}, which no item has"))?;
        let (copy, outside) = p.clone_range(first, p.ops[first + 5].to[0])?;
        let n = copy.len();
        p.replace(first, first, copy, &outside)?;
        set_numbers(p, pool, first + 2, first + 3, value, f64::from(item.index))?;
        set_help(p, pool, first, first + n, tested.help, item.help)
    })
}

/// The item's binding to its bullet, its `_off`s, and its label and help line.
fn bind_off_label(page: &Page, s: &mut TagStream, at: &Insert<'_>, new: &NewItem) -> Result<(), String> {
    let (anchor, item) = (at.anchor(), at.item);
    edit_block(s, &page.scripts.bind, |p, pool| {
        // Two statements per button: `<button>._x = <bullet>._x` and `._y`, each ending in SetMember.
        let starts: Vec<usize> = (0..p.ops.len()).filter(|&i| pushes(p, pool, i, &[Value::Register(1), str_value(&anchor.button)])).collect();
        let (Some(&from), Some(&last)) = (starts.first(), starts.last()) else { return Err(format!("no binding of {}", anchor.button)) };
        let to = (last..p.ops.len()).find(|&j| p.ops[j].code == SET_MEMBER).ok_or("a binding with no SetMember")? + 1;
        let (copy, outside) = p.clone_range(from, to)?;
        let n = copy.len();
        p.insert(to, copy, &outside)?;
        set_strings(p, pool, to, to + n, &anchor.button, &item.button)?;
        set_strings(p, pool, to, to + n, &anchor.bullet, &item.bullet)
    })?;
    for b in &page.scripts.off {
        edit_block(s, b, |p, pool| {
            let name = (0..p.ops.len()).find(|&i| pushes(p, pool, i, &[str_value(&anchor.button)])).ok_or_else(|| format!("no _off of {}", anchor.button))?;
            let from = (0..name).rev().find(|&i| p.push_at(i, pool).is_ok_and(|v| v.first() == Some(&str_value("_off")))).ok_or("an _off with no start")?;
            let to = (name..p.ops.len()).find(|&i| p.ops[i].code == POP).ok_or("an _off with no Pop")? + 1;
            let (copy, outside) = p.clone_range(from, to)?;
            let n = copy.len();
            p.insert(to, copy, &outside)?;
            set_strings(p, pool, to, to + n, &anchor.button, &item.button)
        })?;
    }
    edit_block(s, &page.scripts.labels, |p, pool| {
        let f = (0..p.ops.len())
            .find(|&i| matches!(p.ops[i].code, DEFINE_FUNCTION | DEFINE_FUNCTION2) && p.ops[i].payload.starts_with(b"updateTextStrings\0"))
            .ok_or("no updateTextStrings")?;
        // `<labels>[help] = [caption, help line]`: Push "<labels>"; GetVariable; Push i, help, caption, 2,
        // "Array"; NewObject; SetMember.
        let entry = (f..p.ops[f].to[0])
            .find(|&i| p.ops[i].code == PUSH && p.push_at(i, pool).is_ok_and(|v| v.len() == 5 && v[0].number() == Some(f64::from(anchor.help)) && v[4] == str_value("Array")))
            .ok_or_else(|| format!("updateTextStrings sets no {}[{}]", page.labels, anchor.help))?;
        if entry < 2 || !pushes(p, pool, entry - 2, &[str_value(&page.labels)]) || p.ops.get(entry + 2).map(|o| o.code) != Some(SET_MEMBER) {
            return Err(format!("{}[{}] is not set as `{}[i] = [caption, help]`", page.labels, anchor.help, page.labels));
        }
        let (copy, outside) = p.clone_range(entry - 2, entry + 3)?;
        p.insert(f + 1, copy, &outside)?;
        let help = new.help.as_deref().map(key_text).unwrap_or_default();
        let caption = key_text(&new.label);
        p.edit_push(f + 3, pool, &mut |v| {
            v[0] = Value::Number(f64::from(item.help));
            v[1] = Value::Str(help.clone());
            v[2] = Value::Str(caption.clone());
        })?;
        Ok(())
    })
}

fn scripts(page: &Page, s: &mut TagStream, at: &Insert<'_>, new: &NewItem) -> Result<(), String> {
    keys(page, s, at, new)?;
    // Every other switch on the index: the mouse's, the leaving one, the highlights.
    let mut blocks = vec![page.scripts.mouse, page.scripts.leave];
    blocks.extend(page.scripts.highlight.iter().copied());
    for b in &blocks {
        if add_cases(page, s, b, at.anchor(), at.item)? == 0 {
            return Err(format!("{:?} frame {} DoAction #{}: no switch on {} to add to", b.line(), b.frame, b.block, page.index));
        }
    }
    mouse(page, s, at)?;
    bind_off_label(page, s, at, new)
}

/// Whether the page's binding script binds `button` to the bullet `bullet`: `Push r1, "<button>";
/// GetMember; Push "_x", r1, "<bullet>"`.
fn binds(s: &TagStream, page: &Page, button: &str, bullet: &str) -> Result<bool, String> {
    let blocks = gfx_edit::do_actions(s, page.scripts.bind.line(), page.scripts.bind.frame)?;
    let body = blocks.get(page.scripts.bind.block).ok_or("no binding script")?;
    let p = Program::decode(body)?;
    let pool = p.pool()?;
    for i in 0..p.ops.len() {
        if pushes(&p, &pool, i, &[Value::Register(1), str_value(button)]) && p.ops.get(i + 2).is_some_and(|o| o.code == PUSH) && p.push_at(i + 2, &pool)?.last() == Some(&str_value(bullet)) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_reads() {
        let main = page("shell.main").expect("data/menus.toml");
        assert_eq!(main.items.len(), 6);
        assert_eq!(key_hash(&main.items[3].key), 0x385f518f, "SHELL.MainMenu.Options is the hash SHELL pushes");
        assert_eq!(key_hash(&main.items[0].key), 0x1324df48);
        assert_eq!(key_hash(&main.items[4].key), 0xaba15838, "a key with no name known is its hash");
    }

    #[test]
    fn a_fit_reproduces_an_affine_map() {
        let map = [[0.9, 0.2, 100.0], [-0.1, 1.1, -50.0]];
        let pairs: Vec<_> = [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [7.0, 3.0]].iter().map(|b| (*b, apply(&map, *b))).collect();
        let got = fit(&pairs).expect("fit");
        for (b, r) in &pairs {
            let g = apply(&got, *b);
            assert!((g[0] - r[0]).abs() < 1e-6 && (g[1] - r[1]).abs() < 1e-6);
        }
    }

    #[test]
    fn a_far_hit_clip_is_refused() {
        let mut pairs: Vec<_> = [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [7.0, 3.0]].iter().map(|b| (*b, *b)).collect();
        pairs[3].1 = [500.0, 500.0];
        assert!(fit(&pairs).unwrap_err().contains("misses a retail hit clip"));
    }
}
