//! The running movie: a tree of clips, each a timeline over a depth-sorted display list.
//!
//! A timeline object's depth is the tag's less [`TIMELINE_DEPTH`], so everything a script attaches at
//! depth 0 and up sits above the timeline, as `getDepth` reports in Flash.
//! A backward goto replays the frames up to its target to learn what should be on stage, and keeps an
//! object that is still wanted at the same depth with the same character, so a clip's own state
//! survives, as `GFxSprite`'s goto does.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;

use mercs2_formats::gfx_def::{Character, Control, Cxform, EditText, Matrix, MovieDef, Place, Timeline};

use crate::tess::{tessellate, tessellate_morph, Part};
use crate::text::FontLib;

pub type ClipId = usize;

/// What a timeline depth is offset by.
pub const TIMELINE_DEPTH: i32 = 16384;

fn tag_depth(depth: u16) -> i32 {
    depth as i32 - TIMELINE_DEPTH
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Clip(ClipId),
    Shape,
    Morph,
    Text(String),
}

/// One object in a display list.
#[derive(Clone, Debug, PartialEq)]
pub struct Child {
    pub character: u16,
    pub matrix: Matrix,
    pub cxform: Cxform,
    pub ratio: u16,
    pub name: Option<String>,
    pub clip_depth: u16,
    pub blend: u8,
    pub visible: bool,
    /// Script has moved it, so the timeline leaves its transform alone.
    pub scripted: bool,
    /// Placed by a timeline, so a goto may take it away.
    pub timeline: bool,
    pub node: Node,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    /// The sprite character; `None` for the root.
    pub character: Option<u16>,
    pub parent: Option<ClipId>,
    /// Zero-based current frame.
    pub frame: u16,
    pub playing: bool,
    pub children: BTreeMap<i32, Child>,
}

/// A script to run: the clip it belongs to and its bytecode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub clip: ClipId,
    pub code: Vec<u8>,
}

/// One entry of the draw list, flat and back to front.
#[derive(Clone, Debug)]
pub enum Item {
    Draw(Draw),
    /// What follows up to [`Item::MaskEnd`] writes the mask rather than color.
    MaskBegin,
    MaskEnd,
    /// The mask stops applying; its draws repeat here so a stencil can be taken back down.
    MaskPop(Vec<Draw>),
}

#[derive(Clone, Debug)]
pub struct Draw {
    pub parts: Rc<Vec<Part>>,
    /// Twips to stage pixels.
    pub matrix: Matrix,
    pub cxform: Cxform,
    pub blend: u8,
}

pub struct Movie {
    pub def: Rc<MovieDef>,
    clips: Vec<Option<Clip>>,
    pub root: ClipId,
    pub actions: VecDeque<Action>,
    meshes: HashMap<(u16, u16), Rc<Vec<Part>>>,
    local_fonts: HashMap<u16, Rc<FontLib>>,
    /// What `gfxfontlib.swf` resolves to: retail's `FUN_0060e9c0` picks the language's font movie.
    pub font_lib: Option<Rc<FontLib>>,
    /// The string table `[0x…]` keys read from.
    pub strings: Option<Rc<HashMap<u32, String>>>,
    /// A field wanted a font nothing supplied.
    pub fontless: bool,
}

/// Twips to pixels, the root transform.
const STAGE: Matrix = Matrix { a: 0.05, b: 0.0, c: 0.0, d: 0.05, tx: 0.0, ty: 0.0 };

impl Movie {
    pub fn new(def: Rc<MovieDef>) -> Result<Movie, String> {
        let root = Clip { character: None, parent: None, frame: 0, playing: true, children: BTreeMap::new() };
        let local_fonts = FontLib::all_in(&def)?.into_iter().map(|(id, font)| (id, Rc::new(font))).collect();
        let mut movie = Movie {
            def,
            clips: vec![Some(root)],
            root: 0,
            actions: VecDeque::new(),
            meshes: HashMap::new(),
            local_fonts,
            font_lib: None,
            strings: None,
            fontless: false,
        };
        movie.apply_frame(0, 0, true)?;
        Ok(movie)
    }

    pub fn clip(&self, id: ClipId) -> Result<&Clip, String> {
        self.clips.get(id).and_then(Option::as_ref).ok_or_else(|| format!("clip {id} is gone"))
    }

    pub fn clip_mut(&mut self, id: ClipId) -> Result<&mut Clip, String> {
        self.clips.get_mut(id).and_then(Option::as_mut).ok_or_else(|| format!("clip {id} is gone"))
    }

    pub fn exists(&self, id: ClipId) -> bool {
        self.clips.get(id).is_some_and(Option::is_some)
    }

    /// The timeline of `clip`, held through the definition so the clip can change while it is read.
    fn timeline(&self, clip: ClipId) -> Result<Rc<MovieDef>, String> {
        self.clip(clip)?;
        Ok(self.def.clone())
    }

    fn line<'a>(def: &'a MovieDef, character: Option<u16>) -> Result<&'a Timeline, String> {
        match character {
            None => Ok(&def.timeline),
            Some(id) => match def.characters.get(&id) {
                Some(Character::Sprite(line)) => Ok(line),
                _ => Err(format!("character {id} is not a sprite")),
            },
        }
    }

    /// The frames of a clip's timeline.
    pub fn frame_count(&self, clip: ClipId) -> Result<u16, String> {
        Ok(Self::line(&self.def, self.clip(clip)?.character)?.frames.len() as u16)
    }

    pub fn label(&self, clip: ClipId, label: &str) -> Result<Option<u16>, String> {
        let line = Self::line(&self.def, self.clip(clip)?.character)?;
        // Frame labels are case insensitive in AVM1.
        Ok(line.labels.iter().find(|(name, _)| name.eq_ignore_ascii_case(label)).map(|(_, f)| *f))
    }

    fn new_child(&mut self, parent: ClipId, depth: i32, place: &Place, character: u16) -> Result<(), String> {
        let def = self.def.clone();
        let node = match def.characters.get(&character) {
            Some(Character::Shape(_)) => Node::Shape,
            Some(Character::Morph(_)) => Node::Morph,
            Some(Character::Sprite(_)) => {
                let id = self.clips.len();
                self.clips.push(Some(Clip {
                    character: Some(character),
                    parent: Some(parent),
                    frame: 0,
                    playing: true,
                    children: BTreeMap::new(),
                }));
                self.apply_frame(id, 0, true)?;
                Node::Clip(id)
            }
            Some(Character::EditText(t)) => Node::Text(t.text.clone()),
            Some(other) => return Err(format!("character {character} cannot be placed: {other:?}")),
            None => return Err(format!("no character {character}")),
        };
        self.remove(parent, depth)?;
        let child = Child {
            character,
            matrix: place.matrix.unwrap_or(Matrix::IDENTITY),
            cxform: place.cxform.unwrap_or(Cxform::IDENTITY),
            ratio: place.ratio.unwrap_or(0),
            name: place.name.clone(),
            clip_depth: place.clip_depth.unwrap_or(0),
            blend: place.blend_mode.unwrap_or(0),
            visible: true,
            scripted: false,
            timeline: true,
            node,
        };
        self.clip_mut(parent)?.children.insert(depth, child);
        Ok(())
    }

    /// The child of `clip` called `name`.
    pub fn find_child(&self, clip: ClipId, name: &str) -> Result<Option<(i32, &Child)>, String> {
        Ok(self.clip(clip)?.children.iter().find(|(_, c)| c.name.as_deref() == Some(name)).map(|(d, c)| (*d, c)))
    }

    /// `attachMovie`: a new child of an exported symbol.
    pub fn attach(&mut self, parent: ClipId, symbol: &str, name: &str, depth: i32) -> Result<Option<ClipId>, String> {
        let Some(character) = self.def.exports.get(symbol).copied() else { return Ok(None) };
        self.script_child(parent, character, name, depth, Matrix::IDENTITY, Cxform::IDENTITY)
    }

    /// A child placed by script rather than by a timeline.
    pub fn script_child(
        &mut self,
        parent: ClipId,
        character: u16,
        name: &str,
        depth: i32,
        matrix: Matrix,
        cxform: Cxform,
    ) -> Result<Option<ClipId>, String> {
        let place = Place { character: Some(character), matrix: Some(matrix), cxform: Some(cxform), name: Some(name.to_string()), ..Place::default() };
        self.new_child(parent, depth, &place, character)?;
        let child = self.clip_mut(parent)?.children.get_mut(&depth).ok_or("the new child is missing")?;
        child.timeline = false;
        Ok(match child.node {
            Node::Clip(id) => Some(id),
            _ => None,
        })
    }

    /// The clip's depth and place in its parent.
    pub fn child_of(&self, clip: ClipId) -> Result<Option<(ClipId, i32)>, String> {
        let Some(parent) = self.clip(clip)?.parent else { return Ok(None) };
        let found = self.clip(parent)?.children.iter().find(|(_, c)| c.node == Node::Clip(clip)).map(|(d, _)| (parent, *d));
        Ok(found)
    }

    /// Take the object at `depth` off a display list, with everything under it.
    pub fn remove(&mut self, parent: ClipId, depth: i32) -> Result<(), String> {
        if let Some(child) = self.clip_mut(parent)?.children.remove(&depth) {
            if let Node::Clip(id) = child.node {
                self.destroy(id)?;
            }
        }
        Ok(())
    }

    fn destroy(&mut self, id: ClipId) -> Result<(), String> {
        let clip = self.clips.get_mut(id).and_then(Option::take).ok_or_else(|| format!("clip {id} is gone"))?;
        for child in clip.children.values() {
            if let Node::Clip(inner) = child.node {
                self.destroy(inner)?;
            }
        }
        Ok(())
    }

    fn update_child(&mut self, parent: ClipId, depth: i32, place: &Place) -> Result<(), String> {
        let Some(child) = self.clip_mut(parent)?.children.get_mut(&depth) else {
            return Err(format!("a move at empty depth {depth}"));
        };
        if !child.scripted {
            if let Some(m) = place.matrix {
                child.matrix = m;
            }
            if let Some(c) = place.cxform {
                child.cxform = c;
            }
        }
        if let Some(r) = place.ratio {
            child.ratio = r;
        }
        if let Some(n) = &place.name {
            child.name = Some(n.clone());
        }
        if let Some(c) = place.clip_depth {
            child.clip_depth = c;
        }
        if let Some(b) = place.blend_mode {
            child.blend = b;
        }
        Ok(())
    }

    fn place(&mut self, clip: ClipId, place: &Place) -> Result<(), String> {
        let depth = tag_depth(place.depth);
        let same = self.clip(clip)?.children.get(&depth).map(|c| c.character);
        match place.character {
            // A place of the character already there is a move, keeping its state.
            Some(character) if !place.is_move || same != Some(character) => self.new_child(clip, depth, place, character),
            _ => self.update_child(clip, depth, place),
        }
    }

    fn apply_frame(&mut self, clip: ClipId, frame: u16, actions: bool) -> Result<(), String> {
        let def = self.timeline(clip)?;
        let line = Self::line(&def, self.clip(clip)?.character)?;
        let Some(tags) = line.frames.get(frame as usize) else { return Ok(()) };
        for tag in tags {
            match tag {
                Control::Place(place) => self.place(clip, place)?,
                Control::Remove { depth } => self.remove(clip, tag_depth(*depth))?,
                Control::Action(code) if actions => self.actions.push_back(Action { clip, code: code.clone() }),
                Control::Action(_) => {}
            }
        }
        Ok(())
    }

    /// Move a clip's playhead to a zero-based frame.
    pub fn goto(&mut self, clip: ClipId, frame: u16) -> Result<(), String> {
        let last = self.frame_count(clip)?.saturating_sub(1);
        let target = frame.min(last);
        let current = self.clip(clip)?.frame;
        if target == current {
            return Ok(());
        }
        if target > current {
            for f in current + 1..=target {
                self.apply_frame(clip, f, f == target)?;
            }
        } else {
            self.rewind(clip, target)?;
        }
        self.clip_mut(clip)?.frame = target;
        Ok(())
    }

    /// Build what frame `target` shows by replaying from the first frame, then make the list match.
    fn rewind(&mut self, clip: ClipId, target: u16) -> Result<(), String> {
        let def = self.def.clone();
        let line = Self::line(&def, self.clip(clip)?.character)?;
        let mut wanted: BTreeMap<i32, Place> = BTreeMap::new();
        for tags in &line.frames[..=target as usize] {
            for tag in tags {
                match tag {
                    Control::Place(p) => {
                        let depth = tag_depth(p.depth);
                        match wanted.get_mut(&depth) {
                            Some(have) if p.is_move && (p.character.is_none() || p.character == have.character) => {
                                have.matrix = p.matrix.or(have.matrix);
                                have.cxform = p.cxform.or(have.cxform);
                                have.ratio = p.ratio.or(have.ratio);
                                have.name = p.name.clone().or(have.name.take());
                                have.clip_depth = p.clip_depth.or(have.clip_depth);
                                have.blend_mode = p.blend_mode.or(have.blend_mode);
                            }
                            _ => {
                                wanted.insert(depth, p.clone());
                            }
                        }
                    }
                    Control::Remove { depth } => {
                        wanted.remove(&tag_depth(*depth));
                    }
                    Control::Action(_) => {}
                }
            }
        }
        let gone: Vec<i32> = self
            .clip(clip)?
            .children
            .iter()
            .filter(|(depth, c)| c.timeline && wanted.get(depth).and_then(|p| p.character) != Some(c.character))
            .map(|(depth, _)| *depth)
            .collect();
        for depth in gone {
            self.remove(clip, depth)?;
        }
        for (depth, place) in &wanted {
            if self.clip(clip)?.children.contains_key(depth) {
                // The whole state at the target, so a field the tags leave unset goes back to its default.
                let state = Place {
                    matrix: Some(place.matrix.unwrap_or(Matrix::IDENTITY)),
                    cxform: Some(place.cxform.unwrap_or(Cxform::IDENTITY)),
                    ratio: Some(place.ratio.unwrap_or(0)),
                    ..place.clone()
                };
                self.update_child(clip, *depth, &state)?;
            } else {
                let character = place.character.ok_or_else(|| format!("nothing placed at depth {depth}"))?;
                self.new_child(clip, *depth, place, character)?;
            }
        }
        let tags = &line.frames[target as usize];
        for tag in tags {
            if let Control::Action(code) = tag {
                self.actions.push_back(Action { clip, code: code.clone() });
            }
        }
        Ok(())
    }

    fn step(&mut self, clip: ClipId) -> Result<(), String> {
        let existing: Vec<ClipId> = self
            .clip(clip)?
            .children
            .values()
            .filter_map(|c| match c.node {
                Node::Clip(id) => Some(id),
                _ => None,
            })
            .collect();
        if self.clip(clip)?.playing {
            let frames = self.frame_count(clip)?;
            let current = self.clip(clip)?.frame;
            if frames > 1 {
                if current + 1 < frames {
                    self.goto(clip, current + 1)?;
                } else {
                    self.goto(clip, 0)?;
                }
            }
        }
        for id in existing {
            if self.exists(id) {
                self.step(id)?;
            }
        }
        Ok(())
    }

    /// One frame of the movie.
    pub fn advance(&mut self) -> Result<(), String> {
        self.step(self.root)
    }

    fn parts(&mut self, character: u16, ratio: u16) -> Result<Rc<Vec<Part>>, String> {
        let key = match self.def.characters.get(&character) {
            Some(Character::Morph(_)) => (character, ratio),
            _ => (character, 0),
        };
        if let Some(parts) = self.meshes.get(&key) {
            return Ok(parts.clone());
        }
        let parts = Rc::new(match self.def.characters.get(&character) {
            Some(Character::Shape(s)) => tessellate(s).map_err(|e| format!("shape {character}: {e}"))?,
            Some(Character::Morph(m)) => tessellate_morph(m, ratio).map_err(|e| format!("morph {character}: {e}"))?,
            other => return Err(format!("character {character} has no shape: {other:?}")),
        });
        self.meshes.insert(key, parts.clone());
        Ok(parts)
    }

    fn draw_child(&mut self, child: &Child, matrix: Matrix, cxform: Cxform, blend: u8, out: &mut Vec<Item>) -> Result<(), String> {
        if !child.visible {
            return Ok(());
        }
        let matrix = matrix.then(&child.matrix);
        let cxform = cxform.then(&child.cxform);
        // A clip's blend mode reaches what it holds; 0 and 1 are both "normal".
        let blend = if child.blend > 1 { child.blend } else { blend };
        match &child.node {
            Node::Shape | Node::Morph => {
                let parts = self.parts(child.character, child.ratio)?;
                out.push(Item::Draw(Draw { parts, matrix, cxform, blend }));
            }
            Node::Clip(id) => self.draw_clip(*id, matrix, cxform, blend, out)?,
            Node::Text(text) => {
                if let Some(part) = self.text_part(child.character, text)? {
                    out.push(Item::Draw(Draw { parts: Rc::new(vec![part]), matrix, cxform, blend }));
                }
            }
        }
        Ok(())
    }

    /// The font a field draws with: its own when the movie textured it, else the library.
    pub fn font_of(&self, field: &EditText) -> Result<Option<Rc<FontLib>>, String> {
        if let Some(local) = field.font.and_then(|id| self.local_fonts.get(&id)) {
            return Ok(Some(local.clone()));
        }
        Ok(self.font_lib.clone())
    }

    fn text_part(&mut self, character: u16, text: &str) -> Result<Option<Part>, String> {
        let def = self.def.clone();
        let Some(Character::EditText(field)) = def.characters.get(&character) else {
            return Err(format!("character {character} is not a text field"));
        };
        let Some(font) = self.font_of(field)? else {
            self.fontless = true;
            return Ok(None);
        };
        let shown = match &self.strings {
            Some(strings) => crate::text::translate(text, &|key| strings.get(&key).cloned()),
            None => text.to_string(),
        };
        Ok(crate::text::layout(field, &shown, &font))
    }

    fn draw_clip(&mut self, clip: ClipId, matrix: Matrix, cxform: Cxform, blend: u8, out: &mut Vec<Item>) -> Result<(), String> {
        let children: Vec<(i32, Child)> = self.clip(clip)?.children.iter().map(|(d, c)| (*d, c.clone())).collect();
        // Masks in force: the depth each ends at and its draws, to take back down.
        let mut masks: Vec<(i32, Vec<Draw>)> = Vec::new();
        for (depth, child) in &children {
            while masks.last().is_some_and(|(end, _)| depth > end) {
                let (_, draws) = masks.pop().expect("checked");
                out.push(Item::MaskPop(draws));
            }
            if child.clip_depth != 0 {
                let mut mask = Vec::new();
                self.draw_child(child, matrix, cxform, blend, &mut mask)?;
                let draws: Vec<Draw> = mask
                    .into_iter()
                    .filter_map(|item| match item {
                        Item::Draw(d) => Some(d),
                        _ => None,
                    })
                    .collect();
                out.push(Item::MaskBegin);
                out.extend(draws.iter().cloned().map(Item::Draw));
                out.push(Item::MaskEnd);
                masks.push((tag_depth(child.clip_depth), draws));
            } else {
                self.draw_child(child, matrix, cxform, blend, out)?;
            }
        }
        while let Some((_, draws)) = masks.pop() {
            out.push(Item::MaskPop(draws));
        }
        Ok(())
    }

    /// What a clip draws in its own space, in twips.
    pub fn draw_of(&mut self, clip: ClipId) -> Result<Vec<Item>, String> {
        let mut out = Vec::new();
        self.draw_clip(clip, Matrix::IDENTITY, Cxform::IDENTITY, 0, &mut out)?;
        Ok(out)
    }

    /// The frame as a flat draw list, back to front, in stage pixels.
    pub fn draw(&mut self) -> Result<Vec<Item>, String> {
        let mut out = Vec::new();
        self.draw_clip(self.root, STAGE, Cxform::IDENTITY, 0, &mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::gfx_def::{Fill, Record, Shape};

    fn square() -> Character {
        Character::Shape(Shape {
            bounds: [0, 100, 0, 100],
            fills: vec![Fill::Solid([1, 2, 3, 255])],
            lines: Vec::new(),
            records: vec![
                Record::Style { move_to: Some((0, 0)), fill0: None, fill1: Some(1), line: None, styles: None },
                Record::Line { dx: 100, dy: 0 },
                Record::Line { dx: 0, dy: 100 },
                Record::Line { dx: -100, dy: 0 },
                Record::Line { dx: 0, dy: -100 },
            ],
        })
    }

    fn put(depth: u16, character: u16) -> Control {
        Control::Place(Place { depth, character: Some(character), ..Place::default() })
    }

    fn slide(depth: u16, x: f32) -> Control {
        Control::Place(Place {
            depth,
            is_move: true,
            matrix: Some(Matrix { tx: x, ..Matrix::IDENTITY }),
            ..Place::default()
        })
    }

    fn movie(frames: Vec<Vec<Control>>) -> Movie {
        let mut def = MovieDef::default();
        def.characters.insert(1, square());
        def.characters.insert(
            2,
            Character::Sprite(Timeline {
                declared_frames: 3,
                frames: vec![vec![put(1, 1)], vec![], vec![Control::Remove { depth: 1 }]],
                labels: BTreeMap::new(),
            }),
        );
        def.timeline.labels.insert("Later".into(), 1);
        def.timeline.frames = frames;
        Movie::new(Rc::new(def)).expect("movie")
    }

    fn draws(m: &mut Movie) -> usize {
        m.draw().expect("draw").iter().filter(|i| matches!(i, Item::Draw(_))).count()
    }

    #[test]
    fn a_move_keeps_a_placed_object_and_changes_its_transform() {
        let mut m = movie(vec![vec![put(1, 1)], vec![slide(1, 40.0)]]);
        m.advance().expect("advance");
        assert_eq!(m.clip(m.root).expect("root").children[&(1 - TIMELINE_DEPTH)].matrix.tx, 40.0);
    }

    #[test]
    fn a_timeline_loops_to_its_first_frame() {
        let mut m = movie(vec![vec![put(1, 1)], vec![slide(1, 40.0)]]);
        m.advance().expect("advance");
        m.advance().expect("advance");
        let root = m.clip(m.root).expect("root");
        assert_eq!(root.frame, 0);
        assert_eq!(root.children[&(1 - TIMELINE_DEPTH)].matrix.tx, 0.0);
    }

    #[test]
    fn a_rewind_takes_off_what_the_later_frame_added() {
        let mut m = movie(vec![vec![put(1, 1)], vec![put(2, 1)], vec![]]);
        m.goto(m.root, 1).expect("goto");
        assert_eq!(draws(&mut m), 2);
        m.goto(m.root, 0).expect("goto");
        assert_eq!(draws(&mut m), 1);
    }

    #[test]
    fn a_rewind_keeps_a_clip_that_is_still_wanted() {
        let mut m = movie(vec![vec![put(1, 2)], vec![], vec![]]);
        let id = match m.clip(m.root).expect("root").children[&(1 - TIMELINE_DEPTH)].node {
            Node::Clip(id) => id,
            _ => panic!("a clip"),
        };
        m.goto(id, 1).expect("goto");
        m.goto(m.root, 2).expect("goto");
        m.goto(m.root, 0).expect("goto");
        assert_eq!(m.clip(id).expect("kept").frame, 1);
    }

    #[test]
    fn removing_a_clip_removes_its_children() {
        let mut m = movie(vec![vec![put(1, 2)], vec![Control::Remove { depth: 1 }]]);
        let id = match m.clip(m.root).expect("root").children[&(1 - TIMELINE_DEPTH)].node {
            Node::Clip(id) => id,
            _ => panic!("a clip"),
        };
        m.goto(m.root, 1).expect("goto");
        assert!(!m.exists(id));
    }

    #[test]
    fn a_new_clip_waits_a_frame_before_it_advances() {
        let mut m = movie(vec![vec![], vec![put(1, 2)]]);
        m.advance().expect("advance");
        let id = match m.clip(m.root).expect("root").children[&(1 - TIMELINE_DEPTH)].node {
            Node::Clip(id) => id,
            _ => panic!("a clip"),
        };
        assert_eq!(m.clip(id).expect("clip").frame, 0);
        m.clip_mut(m.root).expect("root").playing = false;
        m.advance().expect("advance");
        assert_eq!(m.clip(id).expect("clip").frame, 1);
    }

    #[test]
    fn a_script_moved_object_ignores_the_timeline() {
        let mut m = movie(vec![vec![put(1, 1)], vec![slide(1, 40.0)]]);
        let depth = 1 - TIMELINE_DEPTH;
        m.clip_mut(m.root).expect("root").children.get_mut(&depth).expect("child").scripted = true;
        m.advance().expect("advance");
        assert_eq!(m.clip(m.root).expect("root").children[&depth].matrix.tx, 0.0);
    }

    #[test]
    fn a_mask_wraps_the_depths_it_covers() {
        let mask = Control::Place(Place { depth: 1, character: Some(1), clip_depth: Some(2), ..Place::default() });
        let mut m = movie(vec![vec![mask, put(2, 1), put(3, 1)]]);
        let kinds: Vec<&str> = m
            .draw()
            .expect("draw")
            .iter()
            .map(|i| match i {
                Item::Draw(_) => "draw",
                Item::MaskBegin => "begin",
                Item::MaskEnd => "end",
                Item::MaskPop(_) => "pop",
            })
            .collect();
        assert_eq!(kinds, ["begin", "draw", "end", "draw", "pop", "draw"]);
    }

    #[test]
    fn a_clips_blend_mode_reaches_what_it_holds() {
        let place = Control::Place(Place { depth: 1, character: Some(2), blend_mode: Some(3), ..Place::default() });
        let mut m = movie(vec![vec![place]]);
        let items = m.draw().expect("draw");
        let Item::Draw(d) = &items[0] else { panic!("a draw") };
        assert_eq!(d.blend, 3);
    }

    #[test]
    fn labels_ignore_case() {
        let m = movie(vec![vec![], vec![]]);
        assert_eq!(m.label(m.root, "later").expect("label"), Some(1));
    }

    #[test]
    fn a_nested_transform_reaches_stage_pixels() {
        let mut m = movie(vec![vec![Control::Place(Place {
            depth: 1,
            character: Some(1),
            matrix: Some(Matrix { tx: 200.0, ..Matrix::IDENTITY }),
            ..Place::default()
        })]]);
        let items = m.draw().expect("draw");
        let Item::Draw(d) = &items[0] else { panic!("a draw") };
        assert_eq!(d.matrix.apply(0.0, 0.0), [10.0, 0.0]);
    }
}
