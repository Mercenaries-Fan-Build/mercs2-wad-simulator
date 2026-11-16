//! Edits to a movie's timelines, made on its [`TagStream`] and written as retail's exporter writes tags
//! ([`gfx_tags::exporter_width`], [`gfx_tags::exporter_long_header`]).
//!
//! A timeline's [`Track`]s are its instances: what is placed at a depth, from the frame it is placed in
//! until the frame that removes it, with every tag that places, moves or removes it. An edit names an
//! instance by its depth and a frame it is on stage in.

use crate::gfx_stream::{Tag, TagBody, TagStream};
use crate::gfx_tags::{self, MatrixBits, PlaceTag, Typed};

/// The root timeline, or a `DefineSprite`'s by its character id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Root,
    Sprite(u16),
}

/// The tags of timeline `line`.
pub fn timeline(stream: &TagStream, line: Line) -> Result<&[Tag], String> {
    match line {
        Line::Root => Ok(&stream.tags),
        Line::Sprite(id) => stream
            .tags
            .iter()
            .find_map(|t| match &t.body {
                TagBody::Sprite { id: i, tags, .. } if *i == id => Some(tags.as_slice()),
                _ => None,
            })
            .ok_or_else(|| format!("no sprite {id}")),
    }
}

fn timeline_mut(stream: &mut TagStream, line: Line) -> Result<&mut Vec<Tag>, String> {
    match line {
        Line::Root => Ok(&mut stream.tags),
        Line::Sprite(id) => stream
            .tags
            .iter_mut()
            .find_map(|t| match &mut t.body {
                TagBody::Sprite { id: i, tags, .. } if *i == id => Some(tags),
                _ => None,
            })
            .ok_or_else(|| format!("no sprite {id}")),
    }
}

/// One instance of a timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub depth: u16,
    /// The zero-based frame it is placed in.
    pub first: usize,
    /// The frame whose `RemoveObject2` takes it off; `None` when it stays to the end.
    pub removed: Option<usize>,
    /// The character it shows last.
    pub character: u16,
    /// The instance name it carries last.
    pub name: Option<String>,
    /// The indices, in the timeline's tags, of every tag that places, moves or removes it.
    pub tags: Vec<usize>,
}

impl Track {
    /// Whether it is on stage in `frame`.
    pub fn on_stage(&self, frame: usize) -> bool {
        frame >= self.first && self.removed.is_none_or(|r| frame < r)
    }

    fn overlaps(&self, other: &Track) -> bool {
        self.first < other.removed.unwrap_or(usize::MAX) && other.first < self.removed.unwrap_or(usize::MAX)
    }
}

fn typed(tag: &Tag) -> Result<Option<Typed>, String> {
    match &tag.body {
        TagBody::Raw(body) => gfx_tags::decode(tag.code, body),
        TagBody::Sprite { .. } => Ok(None),
    }
}

fn text(name: &[u8]) -> Result<String, String> {
    String::from_utf8(name.to_vec()).map_err(|_| format!("an instance name that is not UTF-8: {:?}", String::from_utf8_lossy(name)))
}

/// Every instance of a timeline, in the order they are placed.
pub fn tracks(tags: &[Tag]) -> Result<Vec<Track>, String> {
    let mut out: Vec<Track> = Vec::new();
    let mut live: std::collections::HashMap<u16, usize> = std::collections::HashMap::new();
    let mut frame = 0;
    for (i, tag) in tags.iter().enumerate() {
        match typed(tag)? {
            Some(Typed::ShowFrame) => frame += 1,
            Some(Typed::Place(p)) => {
                let at = live.get(&p.depth).copied();
                match (p.is_move, at) {
                    (true, Some(t)) => {
                        let track = &mut out[t];
                        track.tags.push(i);
                        if let Some(c) = p.character {
                            track.character = c;
                        }
                        if let Some(n) = &p.name {
                            track.name = Some(text(n)?);
                        }
                    }
                    (false, Some(_)) => return Err(format!("tag #{i}: a new placement at depth {}, which is already taken", p.depth)),
                    (_, None) => {
                        let character = p.character.ok_or_else(|| format!("tag #{i}: a move at depth {}, which holds nothing", p.depth))?;
                        live.insert(p.depth, out.len());
                        out.push(Track { depth: p.depth, first: frame, removed: None, character, name: p.name.as_deref().map(text).transpose()?, tags: vec![i] });
                    }
                }
            }
            Some(Typed::Remove { depth }) => {
                let t = live.remove(&depth).ok_or_else(|| format!("tag #{i}: a removal at depth {depth}, which holds nothing"))?;
                out[t].removed = Some(frame);
                out[t].tags.push(i);
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The instance at `depth` on stage in `frame`.
fn find(tracks: &[Track], depth: u16, frame: usize) -> Result<&Track, String> {
    tracks.iter().find(|t| t.depth == depth && t.on_stage(frame)).ok_or_else(|| format!("nothing at depth {depth} in frame {frame}"))
}

/// Replace tag `i` with `t`, written as the exporter writes it.
fn put(tags: &mut [Tag], i: usize, t: &Typed) -> Result<(), String> {
    tags[i] = written(t)?;
    Ok(())
}

fn written(t: &Typed) -> Result<Tag, String> {
    let t = t.exported()?;
    let body = t.encode()?;
    let long = gfx_tags::exporter_long_header(t.code(), Some(&t)).ok_or_else(|| format!("no header rule for tag {}", t.code()))?;
    Ok(Tag { code: t.code(), long: long && body.len() < 0x3F, body: TagBody::Raw(body) })
}

/// `p`'s matrix moved by `(dx, dy)` twips; a placement with no matrix (`first`) gets one.
fn shifted(p: &mut PlaceTag, first: bool, dx: i32, dy: i32) -> Result<(), String> {
    let add = |v: i32, d: i32| v.checked_add(d).ok_or_else(|| format!("depth {}: a translation past i32", p.depth));
    p.matrix = match p.matrix {
        Some(m) => Some(MatrixBits::exported(m.scale.map(|b| b.values), m.rotate.map(|b| b.values), [add(m.translate.values[0], dx)?, add(m.translate.values[1], dy)?])?),
        None if first && (dx, dy) != (0, 0) => Some(MatrixBits::exported(None, None, [dx, dy])?),
        None => None,
    };
    Ok(())
}

/// Move the instance at `depth` on stage in `frame` by `(dx, dy)` twips over its whole lifetime.
pub fn offset(stream: &mut TagStream, line: Line, depth: u16, frame: usize, dx: i32, dy: i32) -> Result<(), String> {
    let tags = timeline_mut(stream, line)?;
    let track = find(&tracks(tags)?, depth, frame)?.clone();
    for (k, &i) in track.tags.iter().enumerate() {
        let Some(Typed::Place(mut p)) = typed(&tags[i])? else { continue };
        shifted(&mut p, k == 0, dx, dy)?;
        put(tags, i, &Typed::Place(p))?;
    }
    Ok(())
}

/// Copy the instance at `depth` on stage in `frame` to `new_depth`, `(dx, dy)` twips away, for the
/// same lifetime. Each copied tag goes in right after the one it copies. A named instance's copy takes
/// `new_name`; the copy of a mask (`clip_depth`) is refused, since what it masks is a depth range.
pub fn copy(stream: &mut TagStream, line: Line, depth: u16, frame: usize, new_depth: u16, new_name: Option<&str>, dx: i32, dy: i32) -> Result<(), String> {
    let tags = timeline_mut(stream, line)?;
    let all = tracks(tags)?;
    let track = find(&all, depth, frame)?.clone();
    if let Some(taken) = all.iter().find(|t| t.depth == new_depth && t.overlaps(&track)) {
        return Err(format!("depth {new_depth} holds {} from frame {} while the copy would be on stage", taken.name.as_deref().unwrap_or("an instance"), taken.first));
    }
    match (&track.name, new_name) {
        (Some(n), None) => return Err(format!("{n} is named, so its copy needs a name of its own")),
        (None, Some(n)) => return Err(format!("the instance at depth {depth} has no name to give {n} in place of")),
        (_, Some(n)) if all.iter().any(|t| t.name.as_deref() == Some(n)) => return Err(format!("{n} already names an instance of this timeline")),
        _ => {}
    }
    for (k, &i) in track.tags.iter().enumerate().rev() {
        let copied = match typed(&tags[i])? {
            Some(Typed::Place(mut p)) => {
                if p.clip_depth.is_some() {
                    return Err(format!("the instance at depth {depth} is a mask; copying one is not built"));
                }
                p.depth = new_depth;
                if p.name.is_some() {
                    p.name = new_name.map(|n| n.as_bytes().to_vec());
                }
                shifted(&mut p, k == 0, dx, dy)?;
                Typed::Place(p)
            }
            Some(Typed::Remove { .. }) => Typed::Remove { depth: new_depth },
            other => return Err(format!("tag #{i} of the track is {other:?}")),
        };
        tags.insert(i + 1, written(&copied)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(depth: u16, character: Option<u16>, is_move: bool, name: Option<&str>, tx: Option<i32>) -> Tag {
        let matrix = tx.map(|x| MatrixBits::exported(None, None, [x, 0]).expect("matrix"));
        written(&Typed::Place(PlaceTag { depth, character, is_move, name: name.map(|n| n.as_bytes().to_vec()), matrix, ..PlaceTag::default() })).expect("tag")
    }

    fn show() -> Tag {
        written(&Typed::ShowFrame).expect("tag")
    }

    /// Frame 0 places `a` at depth 1 and an unnamed instance at 2; frame 1 moves `a`; frame 2 removes it.
    fn stream() -> TagStream {
        let tags = vec![
            place(1, Some(5), false, Some("a"), Some(100)),
            place(2, Some(6), false, None, None),
            show(),
            place(1, None, true, None, Some(200)),
            show(),
            written(&Typed::Remove { depth: 1 }).expect("tag"),
            show(),
            Tag { code: crate::gfx_stream::END, long: false, body: TagBody::Raw(Vec::new()) },
        ];
        let mut body = vec![0u8, 0x00, 0x1E, 3, 0]; // empty RECT, 30 fps, 3 frames
        crate::gfx_stream::write_tags(&tags, &mut body);
        TagStream::parse_body(&body).expect("stream")
    }

    fn tx(stream: &TagStream, i: usize) -> i32 {
        let Some(Typed::Place(p)) = typed(&stream.tags[i]).expect("decode") else { panic!("tag {i} is a placement") };
        p.matrix.map_or(0, |m| m.translate.values[0])
    }

    #[test]
    fn tracks_follow_an_instance_from_placement_to_removal() {
        let t = tracks(&stream().tags).expect("tracks");
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].depth, t[0].first, t[0].removed, t[0].name.as_deref(), &t[0].tags[..]), (1, 0, Some(2), Some("a"), &[0, 3, 5][..]));
        assert_eq!((t[1].depth, t[1].removed), (2, None));
        assert!(t[0].on_stage(1) && !t[0].on_stage(2));
    }

    #[test]
    fn an_offset_moves_every_placement_of_the_instance() {
        let mut s = stream();
        offset(&mut s, Line::Root, 1, 1, 40, 0).expect("offset");
        assert_eq!((tx(&s, 0), tx(&s, 3)), (140, 240));
        offset(&mut s, Line::Root, 2, 0, 20, 0).expect("offset the matrixless one");
        assert_eq!(tx(&s, 1), 20, "a placement with no matrix gets one");
        assert!(offset(&mut s, Line::Root, 1, 2, 1, 0).is_err(), "depth 1 is empty in frame 2");
    }

    #[test]
    fn a_copy_runs_beside_its_original() {
        let mut s = stream();
        copy(&mut s, Line::Root, 1, 0, 9, Some("b"), 0, 0).expect("copy");
        let t = tracks(&s.tags).expect("tracks");
        let b = t.iter().find(|t| t.depth == 9).expect("the copy");
        assert_eq!((b.first, b.removed, b.name.as_deref()), (0, Some(2), Some("b")));
        assert_eq!(b.tags, [1, 5, 8], "each copied tag right after its original");
        assert!(copy(&mut s, Line::Root, 1, 0, 2, Some("c"), 0, 0).is_err(), "depth 2 is taken");
        assert!(copy(&mut s, Line::Root, 1, 0, 10, Some("b"), 0, 0).is_err(), "b is taken");
        assert!(copy(&mut s, Line::Root, 1, 0, 10, None, 0, 0).is_err(), "a named instance's copy needs a name");
    }
}
