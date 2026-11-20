//! Edits to the action blocks of a GFx movie, every other byte of the movie copied unchanged.
//! [`swap_constant`] replaces one string of the `ConstantPool` in chosen action blocks; [`patch_actions`]
//! finds and replaces whole actions in chosen action blocks, remapping every branch and function size
//! across the changed lengths. `mercs2_probe gfx-swap-constant` and `gfx-patch-actions` run them on a file.
//!
//! A block is named `root:<frame>` (a `DoAction` on the root timeline, frames counted from 0 by
//! `ShowFrame`, as `gfx-actions` and `gfx-tags` number them) or `sprite:<id>:<frame>` (a `DoAction`
//! inside `DefineSprite <id>`). In each named block, every `DoAction` whose `ConstantPool` holds the
//! string is rewritten; a named block with no such `DoAction` is an error.
//!
//! Only the pool entry changes, so every `Push` of that constant index now pushes the new string. The
//! pool must be the block's first action: a shorter or longer pool then moves every later action by the
//! same amount and no relative jump changes. Refused: a pool that is not first, the string pushed
//! inline (a `Push` type-0 string, which this does not rewrite), and the new string already in the pool.
//! The tag, sprite and file lengths are recomputed; a tag keeps the long header form it had. The header's
//! length counts the bytes through `End`; bytes after `End` (retail SHELL has 1285) are kept verbatim
//! and stay uncounted.

use std::io::{Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Root(u32),
    Sprite(u16, u32),
}

impl Target {
    pub fn parse(s: &str) -> Result<Target, String> {
        let parts: Vec<&str> = s.split(':').collect();
        let n = |p: &str| p.parse::<u32>().map_err(|_| format!("target {s:?}: {p:?} is not a number"));
        match parts[..] {
            ["root", f] => Ok(Target::Root(n(f)?)),
            ["sprite", id, f] => Ok(Target::Sprite(
                u16::try_from(n(id)?).map_err(|_| format!("target {s:?}: sprite id above 65535"))?,
                n(f)?,
            )),
            _ => Err(format!("target {s:?}: expected root:<frame> or sprite:<id>:<frame>")),
        }
    }
}

const TAG_END: u16 = 0;
const TAG_SHOW_FRAME: u16 = 1;
const TAG_DO_ACTION: u16 = 12;
const TAG_DEFINE_SPRITE: u16 = 39;
const ACTION_CONSTANT_POOL: u8 = 0x88;
const ACTION_PUSH: u8 = 0x96;

struct Tag<'a> {
    code: u16,
    long: bool,
    body: &'a [u8],
}

/// The tags up to and including `End`, and the offset just past `End` (`data.len()` when there is none).
fn read_tags_to_end(data: &[u8]) -> Result<(Vec<Tag<'_>>, usize), String> {
    let tags = read_tags(data)?;
    let end = tags.iter().map(|t| 2 + if t.long { 4 } else { 0 } + t.body.len()).sum();
    Ok((tags, end))
}

fn read_tags(data: &[u8]) -> Result<Vec<Tag<'_>>, String> {
    let mut tags = Vec::new();
    let mut off = 0;
    while off < data.len() {
        if off + 2 > data.len() {
            return Err(format!("tag header truncated at {off:#x}"));
        }
        let rh = u16::from_le_bytes([data[off], data[off + 1]]);
        off += 2;
        let code = rh >> 6;
        let mut len = usize::from(rh & 0x3f);
        let long = len == 0x3f;
        if long {
            if off + 4 > data.len() {
                return Err(format!("long tag length truncated at {off:#x}"));
            }
            len = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]) as usize;
            off += 4;
        }
        if off + len > data.len() {
            return Err(format!("tag {code} at {off:#x} overruns its stream ({len} bytes)"));
        }
        tags.push(Tag { code, long, body: &data[off..off + len] });
        off += len;
        if code == TAG_END {
            break;
        }
    }
    Ok(tags)
}

fn write_tag(out: &mut Vec<u8>, code: u16, long: bool, body: &[u8]) -> Result<(), String> {
    if long || body.len() >= 0x3f {
        let len = u32::try_from(body.len()).map_err(|_| format!("tag {code} body above 4 GiB"))?;
        out.extend_from_slice(&((code << 6) | 0x3f).to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
    } else {
        out.extend_from_slice(&((code << 6) | body.len() as u16).to_le_bytes());
    }
    out.extend_from_slice(body);
    Ok(())
}

/// The action records of a block as `(code, offset of the record, payload)`.
fn actions(block: &[u8]) -> Result<Vec<(u8, usize, &[u8])>, String> {
    let mut out = Vec::new();
    let mut off = 0;
    while off < block.len() {
        let code = block[off];
        let start = off;
        off += 1;
        if code == 0 {
            out.push((code, start, &block[off..off]));
            break;
        }
        let mut payload: &[u8] = &[];
        if code >= 0x80 {
            if off + 2 > block.len() {
                return Err(format!("action {code:#x} at {start:#x}: length truncated"));
            }
            let len = usize::from(u16::from_le_bytes([block[off], block[off + 1]]));
            off += 2;
            if off + len > block.len() {
                return Err(format!("action {code:#x} at {start:#x}: {len} bytes overrun the block"));
            }
            payload = &block[off..off + len];
            off += len;
        }
        out.push((code, start, payload));
    }
    Ok(out)
}

fn pool_strings(payload: &[u8]) -> Result<Vec<&[u8]>, String> {
    if payload.len() < 2 {
        return Err("ConstantPool shorter than its count".into());
    }
    let count = usize::from(u16::from_le_bytes([payload[0], payload[1]]));
    let mut strings = Vec::with_capacity(count);
    let mut off = 2;
    for i in 0..count {
        let end = payload[off..].iter().position(|&b| b == 0).ok_or(format!("ConstantPool string {i} has no terminator"))?;
        strings.push(&payload[off..off + end]);
        off += end + 1;
    }
    if off != payload.len() {
        return Err(format!("ConstantPool has {} bytes after its {count} strings", payload.len() - off));
    }
    Ok(strings)
}

/// The inline strings of a `Push` payload.
fn push_strings(payload: &[u8]) -> Result<Vec<&[u8]>, String> {
    let mut out = Vec::new();
    let mut off = 0;
    while off < payload.len() {
        let ty = payload[off];
        off += 1;
        let size = match ty {
            0 => {
                let end = payload[off..].iter().position(|&b| b == 0).ok_or("Push string has no terminator")?;
                out.push(&payload[off..off + end]);
                end + 1
            }
            1 | 7 => 4,
            2 | 3 => 0,
            4 | 5 | 8 => 1,
            6 => 8,
            9 => 2,
            _ => return Err(format!("Push value type {ty} is not an SWF 8 type")),
        };
        if off + size > payload.len() {
            return Err(format!("Push value type {ty} overruns its action"));
        }
        off += size;
    }
    Ok(out)
}

/// `block` with `from` replaced by `to` in its leading `ConstantPool`, or `None` when the pool does not
/// hold `from`.
fn swap_in_block(block: &[u8], from: &str, to: &str) -> Result<Option<Vec<u8>>, String> {
    let acts = actions(block)?;
    for (code, start, payload) in &acts {
        if *code == ACTION_PUSH && push_strings(payload)?.contains(&from.as_bytes()) {
            return Err(format!("the block pushes {from:?} inline at {start:#x}; only a pool entry is swapped"));
        }
    }
    let Some(pool_at) = acts.iter().position(|(code, _, _)| *code == ACTION_CONSTANT_POOL) else { return Ok(None) };
    let strings = pool_strings(acts[pool_at].2)?;
    if !strings.contains(&from.as_bytes()) {
        return Ok(None);
    }
    if pool_at != 0 {
        return Err(format!("the ConstantPool holding {from:?} is action {pool_at}, not the block's first"));
    }
    if acts.iter().filter(|(code, _, _)| *code == ACTION_CONSTANT_POOL).count() != 1 {
        return Err("the block has more than one ConstantPool".into());
    }
    if strings.contains(&to.as_bytes()) {
        return Err(format!("the pool already holds {to:?}; two entries would push the same string"));
    }
    let mut payload = (strings.len() as u16).to_le_bytes().to_vec();
    for s in &strings {
        payload.extend_from_slice(if *s == from.as_bytes() { to.as_bytes() } else { s });
        payload.push(0);
    }
    let len = u16::try_from(payload.len()).map_err(|_| "the new ConstantPool is above 65535 bytes".to_string())?;
    let (_, _, old) = acts[0];
    let rest = &block[3 + old.len()..];
    let mut out = vec![ACTION_CONSTANT_POOL];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(rest);
    Ok(Some(out))
}

const ACTION_JUMP: u8 = 0x99;
const ACTION_IF: u8 = 0x9d;
const ACTION_DEFINE_FUNCTION: u8 = 0x9b;
const ACTION_DEFINE_FUNCTION2: u8 = 0x8e;
const ACTION_WITH: u8 = 0x94;
const ACTION_TRY: u8 = 0x8f;

/// One find/replace of whole actions in the `DoAction`s of `target`, which must match `count` times there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPatch {
    pub target: Target,
    pub find: Vec<u8>,
    pub replace: Vec<u8>,
    pub count: usize,
}

/// The byte length of the action record at `start`, from its parsed payload.
fn record_len(code: u8, payload: &[u8]) -> usize {
    if code >= 0x80 { 3 + payload.len() } else { 1 }
}

/// The offset a `Jump` / `If` at `start` lands on.
fn branch_target(start: usize, payload: &[u8]) -> Result<isize, String> {
    let off: [u8; 2] = payload.try_into().map_err(|_| format!("branch at {start:#x} has a {}-byte payload, not 2", payload.len()))?;
    Ok(start as isize + 5 + isize::from(i16::from_le_bytes(off)))
}

/// The body of a `DefineFunction` / `DefineFunction2` at `start`, as `(first byte, end)`: its code size is the
/// payload's last two bytes, and the body follows the record.
fn function_body(start: usize, payload: &[u8]) -> Result<(usize, usize), String> {
    let n = payload.len();
    if n < 2 {
        return Err(format!("function at {start:#x} has no code size"));
    }
    let size = usize::from(u16::from_le_bytes([payload[n - 2], payload[n - 1]]));
    let body = start + 3 + n;
    Ok((body, body + size))
}

/// Every branch of `block` lands on an action's first byte (or the block's end) and every function body
/// ends on one.
fn check_block(block: &[u8]) -> Result<(), String> {
    let acts = actions(block)?;
    let mut bounds: std::collections::BTreeSet<usize> = acts.iter().map(|(_, s, _)| *s).collect();
    bounds.insert(block.len());
    for (code, start, payload) in &acts {
        match *code {
            ACTION_JUMP | ACTION_IF => {
                let t = branch_target(*start, payload)?;
                if t < 0 || !bounds.contains(&(t as usize)) {
                    return Err(format!("the branch at {start:#x} lands at {t:#x}, inside an action"));
                }
            }
            ACTION_DEFINE_FUNCTION | ACTION_DEFINE_FUNCTION2 => {
                let (_, end) = function_body(*start, payload)?;
                if !bounds.contains(&end) {
                    return Err(format!("the function at {start:#x} ends at {end:#x}, inside an action"));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// One match of a patch in a block: the old bytes `[start, end)` and what replaces them.
struct Hit<'a> {
    start: usize,
    end: usize,
    new: &'a [u8],
}

/// `block` with every `(find, replace)` applied, and how many times each matched. A match covers whole
/// actions; every branch and function size is remapped across the changed lengths. Refused: a match that
/// cuts an action, overlaps another, holds a function, `With` or `Try` record or that a branch lands inside;
/// a replacement whose branches leave it or that holds such a record; a block with `With`/`Try` and a match.
fn patch_block(block: &[u8], rules: &[(&[u8], &[u8])]) -> Result<(Vec<u8>, Vec<usize>), String> {
    let acts = actions(block)?;
    let starts: std::collections::BTreeSet<usize> = acts.iter().map(|(_, s, _)| *s).collect();
    let is_bound = |p: usize| starts.contains(&p) || p == block.len();
    let structural = |code: u8| matches!(code, ACTION_DEFINE_FUNCTION | ACTION_DEFINE_FUNCTION2 | ACTION_WITH | ACTION_TRY);
    let mut hits: Vec<Hit<'_>> = Vec::new();
    let mut counts = vec![0usize; rules.len()];
    for (r, (find, replace)) in rules.iter().enumerate() {
        let parsed = actions(find).map_err(|e| format!("find {r}: {e}"))?;
        if find.is_empty() || parsed.iter().map(|(c, _, p)| record_len(*c, p)).sum::<usize>() != find.len() {
            return Err(format!("find {r} is not whole actions"));
        }
        if parsed.iter().any(|(c, _, _)| structural(*c)) {
            return Err(format!("find {r} holds a function, With or Try record"));
        }
        let repl = actions(replace).map_err(|e| format!("replace {r}: {e}"))?;
        if repl.iter().map(|(c, _, p)| record_len(*c, p)).sum::<usize>() != replace.len() {
            return Err(format!("replace {r} is not whole actions"));
        }
        for (code, start, payload) in &repl {
            if structural(*code) || *code == ACTION_CONSTANT_POOL {
                return Err(format!("replace {r} holds a function, With, Try or ConstantPool record"));
            }
            if matches!(*code, ACTION_JUMP | ACTION_IF) {
                let t = branch_target(*start, payload)?;
                if t < 0 || t as usize > replace.len() {
                    return Err(format!("replace {r}: the branch at {start:#x} leaves the replacement"));
                }
            }
        }
        for &s in &starts {
            if block[s..].starts_with(find) && is_bound(s + find.len()) {
                hits.push(Hit { start: s, end: s + find.len(), new: replace });
                counts[r] += 1;
            }
        }
    }
    hits.sort_by_key(|h| h.start);
    if hits.windows(2).any(|w| w[1].start < w[0].end) {
        return Err("two matches overlap".into());
    }
    if hits.is_empty() {
        return Ok((block.to_vec(), counts));
    }
    if acts.iter().any(|(c, _, _)| matches!(*c, ACTION_WITH | ACTION_TRY)) {
        return Err("the block has a With or Try record; its ranges are not remapped".into());
    }
    // Where an old action boundary lands. A match's start stays the start of its replacement, so a branch
    // to it runs the new actions; a position strictly inside a match has no image.
    let new_pos = |p: usize| -> Option<usize> {
        let mut delta = 0isize;
        for h in &hits {
            if p >= h.end {
                delta += h.new.len() as isize - (h.end - h.start) as isize;
            } else if p > h.start {
                return None;
            }
        }
        Some((p as isize + delta) as usize)
    };
    let mut out = Vec::with_capacity(block.len());
    let mut next_hit = 0;
    for (code, start, payload) in &acts {
        if let Some(h) = hits.get(next_hit) {
            if *start == h.start {
                out.extend_from_slice(h.new);
            }
            if *start >= h.start && *start < h.end {
                if *start + record_len(*code, payload) >= h.end {
                    next_hit += 1;
                }
                continue;
            }
        }
        let here = new_pos(*start).ok_or_else(|| format!("action at {start:#x} is inside a match"))?;
        match *code {
            ACTION_JUMP | ACTION_IF => {
                let t = branch_target(*start, payload)?;
                let landed = usize::try_from(t)
                    .ok()
                    .and_then(new_pos)
                    .ok_or_else(|| format!("the branch at {start:#x} lands inside a match ({t:#x})"))?;
                let off = i16::try_from(landed as isize - (here as isize + 5))
                    .map_err(|_| format!("the branch at {start:#x} no longer fits a 16-bit offset"))?;
                out.extend_from_slice(&[*code, 2, 0]);
                out.extend_from_slice(&off.to_le_bytes());
            }
            ACTION_DEFINE_FUNCTION | ACTION_DEFINE_FUNCTION2 => {
                let (body, end) = function_body(*start, payload)?;
                let (nb, ne) = new_pos(body)
                    .zip(new_pos(end))
                    .ok_or_else(|| format!("the function at {start:#x} starts or ends inside a match"))?;
                let size = u16::try_from(ne - nb).map_err(|_| format!("the function at {start:#x} grows past 65535 bytes"))?;
                out.extend_from_slice(&block[*start..*start + 3 + payload.len() - 2]);
                out.extend_from_slice(&size.to_le_bytes());
            }
            _ => out.extend_from_slice(&block[*start..*start + record_len(*code, payload)]),
        }
    }
    check_block(&out)?;
    Ok((out, counts))
}

/// The movie `data` with every patch applied, and the blocks rewritten. Each patch must match exactly its
/// `count` times across the `DoAction`s of its block.
pub fn patch_actions(data: &[u8], patches: &[ActionPatch]) -> Result<(Vec<u8>, Vec<(Target, usize)>), String> {
    let mut targets: Vec<Target> = Vec::new();
    for p in patches {
        if !targets.contains(&p.target) {
            targets.push(p.target);
        }
    }
    let mut seen = vec![0usize; patches.len()];
    let (out, done) = rewrite_movie(data, &targets, &mut |here, block| {
        let mine: Vec<usize> = (0..patches.len()).filter(|&i| patches[i].target == here).collect();
        let rules: Vec<(&[u8], &[u8])> = mine.iter().map(|&i| (patches[i].find.as_slice(), patches[i].replace.as_slice())).collect();
        let (new, counts) = patch_block(block, &rules).map_err(|e| format!("{here:?}: {e}"))?;
        for (k, &i) in mine.iter().enumerate() {
            seen[i] += counts[k];
        }
        Ok(counts.iter().any(|&c| c > 0).then_some(new))
    })?;
    for (i, p) in patches.iter().enumerate() {
        if seen[i] != p.count {
            return Err(format!("patch {i} ({:?}) matched {} time(s), not {}", p.target, seen[i], p.count));
        }
    }
    Ok((out, done))
}

/// Patches from a TSV: `target`, `find`, `replace`, `count`, the byte strings as hex (spaces allowed);
/// `#` lines are comments.
pub fn parse_patches(text: &str) -> Result<Vec<ActionPatch>, String> {
    let hex = |s: &str| -> Result<Vec<u8>, String> {
        let h: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        if h.len() % 2 != 0 {
            return Err(format!("{s:?}: odd number of hex digits"));
        }
        (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| format!("{s:?}: not hex"))).collect()
    };
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let [target, find, replace, count] = f[..] else {
            return Err(format!("line {}: {} fields, not 4 (target, find, replace, count)", n + 1, f.len()));
        };
        out.push(ActionPatch {
            target: Target::parse(target).map_err(|e| format!("line {}: {e}", n + 1))?,
            find: hex(find).map_err(|e| format!("line {}: find {e}", n + 1))?,
            replace: hex(replace).map_err(|e| format!("line {}: replace {e}", n + 1))?,
            count: count.parse().map_err(|_| format!("line {}: count {count:?} is not a number", n + 1))?,
        });
    }
    if out.is_empty() {
        return Err("no patches".into());
    }
    Ok(out)
}

/// What [`rewrite_movie`] does to one `DoAction` body of a named block: `Some` replaces it, `None` keeps it.
type BlockEdit<'f> = dyn FnMut(Target, &[u8]) -> Result<Option<Vec<u8>>, String> + 'f;

/// Rewrites a tag list; `in_sprite` is the sprite id when the list is a sprite's.
fn rewrite_tags(
    tags: &[Tag<'_>],
    in_sprite: Option<u16>,
    targets: &[Target],
    edit: &mut BlockEdit<'_>,
    done: &mut Vec<(Target, usize)>,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut frame = 0u32;
    for tag in tags {
        match tag.code {
            TAG_DO_ACTION => {
                let here = match in_sprite {
                    None => Target::Root(frame),
                    Some(id) => Target::Sprite(id, frame),
                };
                let swapped = if targets.contains(&here) { edit(here, tag.body)? } else { None };
                match swapped {
                    Some(body) => {
                        write_tag(&mut out, tag.code, tag.long, &body)?;
                        done.push((here, body.len()));
                    }
                    None => write_tag(&mut out, tag.code, tag.long, tag.body)?,
                }
            }
            TAG_DEFINE_SPRITE if in_sprite.is_none() => {
                if tag.body.len() < 4 {
                    return Err("DefineSprite shorter than its header".into());
                }
                let id = u16::from_le_bytes([tag.body[0], tag.body[1]]);
                if targets.iter().any(|t| matches!(t, Target::Sprite(s, _) if *s == id)) {
                    let inner = read_tags(&tag.body[4..])?;
                    let mut body = tag.body[..4].to_vec();
                    body.extend_from_slice(&rewrite_tags(&inner, Some(id), targets, edit, done)?);
                    write_tag(&mut out, tag.code, tag.long, &body)?;
                } else {
                    write_tag(&mut out, tag.code, tag.long, tag.body)?;
                }
            }
            _ => write_tag(&mut out, tag.code, tag.long, tag.body)?,
        }
        if tag.code == TAG_SHOW_FRAME {
            frame += 1;
        }
    }
    Ok(out)
}

/// The movie `data` with `from` swapped for `to` in the pools of `targets`, and what was rewritten.
pub fn swap_constant(data: &[u8], targets: &[Target], from: &str, to: &str) -> Result<(Vec<u8>, Vec<(Target, usize)>), String> {
    if from.is_empty() || to.is_empty() || from.contains('\0') || to.contains('\0') {
        return Err("both strings must be non-empty and hold no NUL".into());
    }
    let (out, done) = rewrite_movie(data, targets, &mut |_, block| swap_in_block(block, from, to))?;
    for t in targets {
        if !done.iter().any(|(d, _)| d == t) {
            return Err(format!("{t:?}: no DoAction there has a ConstantPool holding {from:?}"));
        }
    }
    Ok((out, done))
}

/// The movie `data` with `edit` applied to every `DoAction` of the named `targets`, every other byte kept,
/// and the blocks `edit` replaced with their new lengths. Tag, sprite and file lengths follow; a tag keeps
/// its long header form; bytes after `End` stay, uncounted.
fn rewrite_movie(data: &[u8], targets: &[Target], edit: &mut BlockEdit<'_>) -> Result<(Vec<u8>, Vec<(Target, usize)>), String> {
    if data.len() < 8 {
        return Err("too small for a GFx header".into());
    }
    let magic = &data[..3];
    let compressed = match magic {
        b"CFX" | b"CWS" => true,
        b"GFX" | b"FWS" => false,
        _ => return Err(format!("bad magic {:?}", String::from_utf8_lossy(magic))),
    };
    let body = if compressed {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(&data[8..]).read_to_end(&mut out).map_err(|e| format!("zlib inflate: {e}"))?;
        out
    } else {
        data[8..].to_vec()
    };
    // The frame RECT: 5 bits of field width, then four fields of that width, padded to a byte.
    let nbits = usize::from(body[0] >> 3);
    let header_len = (5 + 4 * nbits).div_ceil(8) + 4;
    let (tags, tags_len) = read_tags_to_end(&body[header_len..])?;
    // The header counts the bytes through `End`; retail SHELL carries 1285 more after it, kept verbatim.
    let declared = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if declared != 8 + header_len + tags_len {
        return Err(format!(
            "the header declares {declared} bytes; the stream through End is {} (and {} follow it)",
            8 + header_len + tags_len,
            body.len() - header_len - tags_len
        ));
    }
    let trailer = &body[header_len + tags_len..];
    let mut done = Vec::new();
    let mut new_body = body[..header_len].to_vec();
    new_body.extend_from_slice(&rewrite_tags(&tags, None, targets, edit, &mut done)?);
    let total = u32::try_from(new_body.len() + 8).map_err(|_| "movie above 4 GiB".to_string())?;
    new_body.extend_from_slice(trailer);
    let mut out = data[..4].to_vec();
    out.extend_from_slice(&total.to_le_bytes());
    if compressed {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(&new_body).map_err(|e| format!("zlib deflate: {e}"))?;
        out.extend_from_slice(&z.finish().map_err(|e| format!("zlib deflate: {e}"))?);
    } else {
        out.extend_from_slice(&new_body);
    }
    Ok((out, done))
}

/// Whether the edited movie `out` still decodes, as every edit's result must.
pub fn check_decodes(out: &[u8]) -> Result<(), String> {
    crate::gfx::GfxMovie::parse(out)
        .and_then(|m| crate::gfx_def::MovieDef::decode(&m))
        .map(|_| ())
        .map_err(|e| format!("the rewritten movie does not decode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(strings: &[&str]) -> Vec<u8> {
        let mut p = (strings.len() as u16).to_le_bytes().to_vec();
        for s in strings {
            p.extend_from_slice(s.as_bytes());
            p.push(0);
        }
        let mut a = vec![ACTION_CONSTANT_POOL];
        a.extend_from_slice(&(p.len() as u16).to_le_bytes());
        a.extend_from_slice(&p);
        a
    }

    /// `Platform == <constant 1>`, then a forward jump over one action: the jump stays valid after the
    /// pool changes length.
    fn block(strings: &[&str]) -> Vec<u8> {
        let mut b = pool(strings);
        b.extend_from_slice(&[ACTION_PUSH, 2, 0, 8, 0, 0x1c]); // push constant 0, GetVariable
        b.extend_from_slice(&[ACTION_PUSH, 2, 0, 8, 1, 0x49]); // push constant 1, Equals2
        b.extend_from_slice(&[0x99, 2, 0, 1, 0, 0x17, 0]); // Jump +1, Pop, End
        b
    }

    fn movie(root_action: &[u8], sprite_action: &[u8]) -> Vec<u8> {
        let mut tags = Vec::new();
        write_tag(&mut tags, TAG_SHOW_FRAME, false, &[]).unwrap();
        write_tag(&mut tags, TAG_DO_ACTION, true, root_action).unwrap();
        let mut sprite = 7u16.to_le_bytes().to_vec();
        sprite.extend_from_slice(&1u16.to_le_bytes());
        write_tag(&mut sprite, TAG_DO_ACTION, false, sprite_action).unwrap();
        write_tag(&mut sprite, TAG_SHOW_FRAME, false, &[]).unwrap();
        write_tag(&mut sprite, TAG_END, false, &[]).unwrap();
        write_tag(&mut tags, TAG_DEFINE_SPRITE, true, &sprite).unwrap();
        write_tag(&mut tags, TAG_SHOW_FRAME, false, &[]).unwrap();
        write_tag(&mut tags, TAG_END, false, &[]).unwrap();
        // RECT with 0-bit fields (one byte), frame rate, frame count.
        let mut body = vec![0, 0, 24, 2, 0];
        body.extend_from_slice(&tags);
        let mut out = b"GFX\x08".to_vec();
        out.extend_from_slice(&((body.len() + 8) as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn the_named_pools_change_and_every_other_byte_stays() {
        let shared = block(&["Platform", "PS3"]);
        let input = movie(&shared, &shared);
        let (out, done) = swap_constant(&input, &[Target::Root(1), Target::Sprite(7, 0)], "PS3", "PC").unwrap();
        assert_eq!(done.len(), 2);
        let swapped = block(&["Platform", "PC"]);
        assert_eq!(out, movie(&swapped, &swapped), "only the two pools differ; lengths follow");
        let (_, only_root) = swap_constant(&input, &[Target::Root(1)], "PS3", "PC").unwrap();
        assert_eq!(only_root, vec![(Target::Root(1), swapped.len())]);
    }

    #[test]
    fn a_compressed_movie_round_trips_through_zlib() {
        let shared = block(&["Platform", "PS3"]);
        let plain = movie(&shared, &shared);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&plain[8..]).unwrap();
        let mut cfx = b"CFX\x08".to_vec();
        cfx.extend_from_slice(&plain[4..8]);
        cfx.extend_from_slice(&z.finish().unwrap());
        let (out, _) = swap_constant(&cfx, &[Target::Sprite(7, 0)], "PS3", "PC").unwrap();
        assert_eq!(&out[..3], b"CFX");
        let mut inflated = Vec::new();
        flate2::read::ZlibDecoder::new(&out[8..]).read_to_end(&mut inflated).unwrap();
        let swapped = block(&["Platform", "PC"]);
        assert_eq!(inflated, movie(&shared, &swapped)[8..]);
        assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize, inflated.len() + 8);
    }

    const PUSH_TRUE: [u8; 5] = [ACTION_PUSH, 2, 0, 5, 1];
    const POP: u8 = 0x17;

    fn branch(code: u8, off: i16) -> Vec<u8> {
        let mut b = vec![code, 2, 0];
        b.extend_from_slice(&off.to_le_bytes());
        b
    }

    /// `DefineFunction2 f` (no parameters, 2 registers, `_root` preloaded) over `body`, then `End`.
    fn function_block(body: &[u8]) -> Vec<u8> {
        let mut b = vec![ACTION_DEFINE_FUNCTION2, 9, 0, b'f', 0, 0, 0, 2, 0x40, 0];
        b.extend_from_slice(&(body.len() as u16).to_le_bytes());
        b.extend_from_slice(body);
        b.push(0);
        b
    }

    /// `ButtonClicked`'s shape: an `If` to the call being guarded, a `Jump` over it from the other arm.
    fn guarded_body(jump_to_after: i16, call: &[u8], after: &[u8]) -> Vec<u8> {
        let mut b = PUSH_TRUE.to_vec();
        b.extend(branch(ACTION_IF, 6)); // to the call: 10 + 6 = 16
        b.push(POP);
        b.extend(branch(ACTION_JUMP, jump_to_after)); // over the call
        b.extend_from_slice(call);
        b.extend_from_slice(after);
        b
    }

    #[test]
    fn an_insertion_before_a_branch_target_runs_first_and_everything_else_follows() {
        let call = [POP, POP];
        let block = function_block(&guarded_body(2, &call, &PUSH_TRUE));
        // A guard in front of the call: its own If skips one Pop, and the call stays after it.
        let mut replace = vec![ACTION_PUSH, 2, 0, 5, 0];
        replace.extend(branch(ACTION_IF, 1));
        replace.push(POP);
        replace.extend_from_slice(&call);
        let (out, counts) = patch_block(&block, &[(&call, &replace)]).unwrap();
        assert_eq!(counts, vec![1]);
        // The If still lands on the first byte of the replacement; the Jump over the call grows by the 11
        // inserted bytes; the function's code size grows by 11.
        assert_eq!(out, function_block(&guarded_body(2 + 11, &replace, &PUSH_TRUE)));
    }

    #[test]
    fn a_shorter_label_pulls_a_branch_across_it_closer() {
        let label = |s: &str| {
            let mut a = vec![0x8c];
            a.extend_from_slice(&((s.len() + 1) as u16).to_le_bytes());
            a.extend_from_slice(s.as_bytes());
            a.push(0);
            a
        };
        let block = |l: &[u8]| {
            let mut b = branch(ACTION_JUMP, l.len() as i16);
            b.extend_from_slice(l);
            b.extend([POP, 0]);
            b
        };
        let (old, new) = (label("_newgameagreement"), label("_DLCStart"));
        let input = movie(&block(&old), &block(&old));
        let patches = [ActionPatch { target: Target::Sprite(7, 0), find: old.clone(), replace: new.clone(), count: 1 }];
        let (out, done) = patch_actions(&input, &patches).unwrap();
        assert_eq!(out, movie(&block(&old), &block(&new)), "the root block is not named, so it keeps its label");
        assert_eq!(done, vec![(Target::Sprite(7, 0), block(&new).len())]);
        let wrong = [ActionPatch { count: 2, ..patches[0].clone() }];
        assert!(patch_actions(&input, &wrong).unwrap_err().contains("matched 1 time(s), not 2"));
    }

    #[test]
    fn action_patch_refusals_name_the_reason() {
        let call = [POP, POP];
        let block = function_block(&guarded_body(2, &call, &PUSH_TRUE));
        // A branch landing on the second Pop would land inside the match.
        let mut into_middle = PUSH_TRUE.to_vec();
        into_middle.extend(branch(ACTION_IF, 1));
        into_middle.extend([POP, POP, 0]);
        let e = patch_block(&into_middle, &[(&[POP, POP], &[POP])]).unwrap_err();
        assert!(e.contains("inside a match"), "{e}");
        // A replacement whose branch leaves it.
        let mut escaping = branch(ACTION_JUMP, 40);
        escaping.extend_from_slice(&call);
        let e = patch_block(&block, &[(&call, &escaping)]).unwrap_err();
        assert!(e.contains("leaves the replacement"), "{e}");
        // A replacement carrying a function record.
        let e = patch_block(&block, &[(&call, &function_block(&[])[..12])]).unwrap_err();
        assert!(e.contains("function, With, Try or ConstantPool"), "{e}");
        // Bytes that are not whole actions are not looked for, so they match nowhere.
        let (_, counts) = patch_block(&block, &[(&[5, 1], &[POP])]).unwrap_or((Vec::new(), vec![9]));
        assert_ne!(counts, vec![1]);
        // A With record anywhere: its range is not remapped.
        let mut with = vec![ACTION_WITH, 2, 0, 1, 0, POP];
        with.extend_from_slice(&PUSH_TRUE);
        with.extend([POP, POP, 0]);
        let e = patch_block(&with, &[(&[POP, POP], &[POP])]).unwrap_err();
        assert!(e.contains("With or Try"), "{e}");
        // The same find twice in a row overlaps itself.
        let e = patch_block(&[POP, POP, POP, 0], &[(&[POP, POP], &[POP])]).unwrap_err();
        assert!(e.contains("overlap"), "{e}");
    }

    #[test]
    fn a_patch_table_parses_and_names_its_errors() {
        let t = "# comment\nsprite:694:299\t17 17\t96020005 01 17 17\t3\n";
        let p = parse_patches(t).unwrap();
        assert_eq!(p, vec![ActionPatch { target: Target::Sprite(694, 299), find: vec![POP, POP], replace: vec![0x96, 2, 0, 5, 1, POP, POP], count: 3 }]);
        assert!(parse_patches("root:1\t171\t17\t1\n").unwrap_err().contains("odd number"));
        assert!(parse_patches("root:1\t17\t17\n").unwrap_err().contains("3 fields"));
        assert!(parse_patches("# only\n").unwrap_err().contains("no patches"));
    }

    /// Retail SHELL's header counts the stream through `End`; the bytes after it stay, uncounted.
    #[test]
    fn bytes_after_end_are_kept_and_left_out_of_the_length() {
        let shared = block(&["Platform", "PS3"]);
        let tail = [0x96, 0x02, 0x00, 0x08, 0x30];
        let mut input = movie(&shared, &shared);
        input.extend_from_slice(&tail);
        let (out, _) = swap_constant(&input, &[Target::Root(1)], "PS3", "PC").unwrap();
        let mut want = movie(&block(&["Platform", "PC"]), &shared);
        want.extend_from_slice(&tail);
        assert_eq!(out, want);
        let mut wrong = input.clone();
        wrong[4..8].copy_from_slice(&(input.len() as u32).to_le_bytes());
        let e = swap_constant(&wrong, &[Target::Root(1)], "PS3", "PC").unwrap_err();
        assert!(e.contains("through End") && e.contains("5 follow it"), "{e}");
    }

    #[test]
    fn refusals_name_the_reason() {
        let shared = block(&["Platform", "PS3"]);
        let input = movie(&shared, &shared);
        let e = swap_constant(&input, &[Target::Root(0)], "PS3", "PC").unwrap_err();
        assert!(e.contains("Root(0)") && e.contains("no DoAction"), "{e}");
        let has_to = block(&["PC", "PS3"]);
        let e = swap_constant(&movie(&has_to, &shared), &[Target::Root(1)], "PS3", "PC").unwrap_err();
        assert!(e.contains("already holds"), "{e}");
        let mut inline = block(&["Platform", "PS3"]);
        let end = inline.len() - 1;
        inline.splice(end..end, [ACTION_PUSH, 5, 0, 0, b'P', b'S', b'3', 0]);
        let e = swap_constant(&movie(&inline, &shared), &[Target::Root(1)], "PS3", "PC").unwrap_err();
        assert!(e.contains("inline"), "{e}");
        let mut late = vec![0x17];
        late.extend_from_slice(&block(&["Platform", "PS3"]));
        let e = swap_constant(&movie(&late, &shared), &[Target::Root(1)], "PS3", "PC").unwrap_err();
        assert!(e.contains("not the block's first"), "{e}");
    }
}
