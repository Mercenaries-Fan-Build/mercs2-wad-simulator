//! Edits to the action blocks of a GFx movie, every other byte of the movie copied unchanged.
//! [`read_actions`] / [`write_actions`], [`push_values`] / [`push_payload`] and [`pool`] / [`pool_payload`]
//! read a block's records, a `Push`'s values and a `ConstantPool`'s strings as stored and write them back
//! to the same bytes (every block of every movie: the workshop's `every_action_block_writes_back`).
//! [`Program`] holds a block's records with every branch target and body end as a record index, so
//! records can be inserted anywhere and the block encoded again with every position recomputed (the
//! same gate checks every block encodes back to its bytes).
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

use crate::gfx_stream::{Tag, TagBody, TagStream};
use crate::gfx_tags;

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

const ACTION_CONSTANT_POOL: u8 = 0x88;
const ACTION_PUSH: u8 = 0x96;

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

/// One action record as written: its code and, for a code of `0x80` or more, its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub code: u8,
    pub payload: Vec<u8>,
}

/// A block's action records through its `End` (code 0), and the bytes after that `End`.
pub fn read_actions(block: &[u8]) -> Result<(Vec<Action>, Vec<u8>), String> {
    let acts = actions(block)?;
    let read: usize = acts.iter().map(|(c, _, p)| record_len(*c, p)).sum();
    Ok((acts.into_iter().map(|(code, _, payload)| Action { code, payload: payload.to_vec() }).collect(), block[read..].to_vec()))
}

/// The records and the bytes after them, as [`read_actions`] reads them.
pub fn write_actions(actions: &[Action], after: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for a in actions {
        out.push(a.code);
        if a.code >= 0x80 {
            let len = u16::try_from(a.payload.len()).map_err(|_| format!("action {:#x}: a {}-byte payload past 65535", a.code, a.payload.len()))?;
            out.extend(len.to_le_bytes());
            out.extend(&a.payload);
        } else if !a.payload.is_empty() {
            return Err(format!("action {:#x} is below 0x80 and has no payload to carry {} bytes", a.code, a.payload.len()));
        }
    }
    out.extend(after);
    Ok(out)
}

/// One value of a `Push`, as stored: numbers keep their bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushValue {
    /// Type 0.
    Str(Vec<u8>),
    /// Type 1: an `f32`'s bits.
    Float(u32),
    /// Type 2.
    Null,
    /// Type 3.
    Undefined,
    /// Type 4.
    Register(u8),
    /// Type 5.
    Bool(u8),
    /// Type 6: SWF's double, its two 32-bit halves swapped, as stored.
    Double([u8; 8]),
    /// Type 7.
    Int(u32),
    /// Type 8: a constant-pool index in one byte.
    Constant8(u8),
    /// Type 9: a constant-pool index in two bytes.
    Constant16(u16),
}

/// The values of a `Push` payload.
pub fn push_values(payload: &[u8]) -> Result<Vec<PushValue>, String> {
    let mut out = Vec::new();
    let mut off = 0;
    while off < payload.len() {
        let ty = payload[off];
        off += 1;
        let take = |off: &mut usize, n: usize| -> Result<&[u8], String> {
            let b = payload.get(*off..*off + n).ok_or_else(|| format!("Push value type {ty} overruns its action"))?;
            *off += n;
            Ok(b)
        };
        out.push(match ty {
            0 => {
                let end = payload[off..].iter().position(|&b| b == 0).ok_or("Push string has no terminator")?;
                let s = payload[off..off + end].to_vec();
                off += end + 1;
                PushValue::Str(s)
            }
            1 => PushValue::Float(u32::from_le_bytes(take(&mut off, 4)?.try_into().expect("4 bytes"))),
            2 => PushValue::Null,
            3 => PushValue::Undefined,
            4 => PushValue::Register(take(&mut off, 1)?[0]),
            5 => PushValue::Bool(take(&mut off, 1)?[0]),
            6 => PushValue::Double(take(&mut off, 8)?.try_into().expect("8 bytes")),
            7 => PushValue::Int(u32::from_le_bytes(take(&mut off, 4)?.try_into().expect("4 bytes"))),
            8 => PushValue::Constant8(take(&mut off, 1)?[0]),
            9 => PushValue::Constant16(u16::from_le_bytes(take(&mut off, 2)?.try_into().expect("2 bytes"))),
            _ => return Err(format!("Push value type {ty} is not an SWF 8 type")),
        });
    }
    Ok(out)
}

/// The payload of a `Push` of `values`.
pub fn push_payload(values: &[PushValue]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for v in values {
        match v {
            PushValue::Str(s) => {
                if s.contains(&0) {
                    return Err(format!("a pushed string {:?} holds a zero byte", String::from_utf8_lossy(s)));
                }
                out.push(0);
                out.extend(s);
                out.push(0);
            }
            PushValue::Float(bits) => {
                out.push(1);
                out.extend(bits.to_le_bytes());
            }
            PushValue::Null => out.push(2),
            PushValue::Undefined => out.push(3),
            PushValue::Register(r) => out.extend([4, *r]),
            PushValue::Bool(b) => out.extend([5, *b]),
            PushValue::Double(d) => {
                out.push(6);
                out.extend(d);
            }
            PushValue::Int(i) => {
                out.push(7);
                out.extend(i.to_le_bytes());
            }
            PushValue::Constant8(i) => out.extend([8, *i]),
            PushValue::Constant16(i) => {
                out.push(9);
                out.extend(i.to_le_bytes());
            }
        }
    }
    Ok(out)
}

/// The strings of a `ConstantPool` payload.
pub fn pool(payload: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    Ok(pool_strings(payload)?.into_iter().map(<[u8]>::to_vec).collect())
}

/// The payload of a `ConstantPool` of `strings`.
pub fn pool_payload(strings: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let count = u16::try_from(strings.len()).map_err(|_| format!("{} pool strings do not fit a u16 count", strings.len()))?;
    let mut out = count.to_le_bytes().to_vec();
    for s in strings {
        if s.contains(&0) {
            return Err(format!("a pool string {:?} holds a zero byte", String::from_utf8_lossy(s)));
        }
        out.extend(s);
        out.push(0);
    }
    Ok(out)
}

/// One record of a [`Program`]. Its payload is kept as stored except the bytes that hold a position (a
/// `Jump`/`If` offset, a function's code size, a `With` size, a `Try`'s three sizes): those are zero in
/// the program and written from `to` by [`Program::encode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub code: u8,
    pub payload: Vec<u8>,
    /// Record indices: a `Jump`/`If`'s target; the end of a function's or a `With`'s body; the ends of a
    /// `Try`'s try, catch and finally blocks. The block's end is `ops.len()`.
    pub to: Vec<usize>,
}

/// A block's records with every position held as a record index, so records can be inserted, removed
/// or replaced anywhere and [`Program::encode`] lays the block out again with every branch and size
/// recomputed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub ops: Vec<Op>,
    /// The bytes after `End`, kept verbatim.
    pub after: Vec<u8>,
}

/// The byte offsets of the size fields of a `Try` payload: try, catch, finally.
const TRY_SIZES: [usize; 3] = [1, 3, 5];

impl Program {
    /// `block` read as records; every position must land on a record's first byte or the block's end.
    pub fn decode(block: &[u8]) -> Result<Program, String> {
        let acts = actions(block)?;
        let read: usize = acts.iter().map(|(c, _, p)| record_len(*c, p)).sum();
        let index_of = |pos: isize, what: &str, start: usize| -> Result<usize, String> {
            if pos == read as isize {
                return Ok(acts.len());
            }
            usize::try_from(pos)
                .ok()
                .and_then(|p| acts.binary_search_by_key(&p, |(_, s, _)| *s).ok())
                .ok_or_else(|| format!("the {what} of the record at {start:#x} lands at {pos:#x}, not on a record"))
        };
        let mut ops = Vec::with_capacity(acts.len());
        for (code, start, payload) in &acts {
            let next = start + record_len(*code, payload);
            let u16_at = |at: usize| -> Result<usize, String> {
                payload
                    .get(at..at + 2)
                    .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
                    .ok_or_else(|| format!("the record {code:#x} at {start:#x} is too short for its sizes"))
            };
            let to = match *code {
                ACTION_JUMP | ACTION_IF => vec![index_of(branch_target(*start, payload)?, "branch", *start)?],
                ACTION_DEFINE_FUNCTION | ACTION_DEFINE_FUNCTION2 => {
                    let (_, end) = function_body(*start, payload)?;
                    vec![index_of(end as isize, "function end", *start)?]
                }
                ACTION_WITH => vec![index_of((next + u16_at(0)?) as isize, "With end", *start)?],
                ACTION_TRY => {
                    let mut end = next;
                    let mut ends = Vec::with_capacity(3);
                    for at in TRY_SIZES {
                        end += u16_at(at)?;
                        ends.push(index_of(end as isize, "Try block end", *start)?);
                    }
                    ends
                }
                _ => Vec::new(),
            };
            // The position bytes are written by `encode` from `to`; held as zero, two programs that
            // differ only in layout compare equal.
            let mut payload = payload.to_vec();
            let n = payload.len();
            let zero: &[usize] = match *code {
                ACTION_JUMP | ACTION_IF | ACTION_WITH => &[0],
                ACTION_DEFINE_FUNCTION | ACTION_DEFINE_FUNCTION2 => &[n - 2],
                ACTION_TRY => &TRY_SIZES,
                _ => &[],
            };
            for &at in zero {
                payload[at..at + 2].fill(0);
            }
            ops.push(Op { code: *code, payload, to });
        }
        Ok(Program { ops, after: block[read..].to_vec() })
    }

    /// The block's bytes: every record laid out in order, every position written from its `to`.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut at = Vec::with_capacity(self.ops.len() + 1);
        let mut pos = 0usize;
        for op in &self.ops {
            at.push(pos);
            pos += record_len(op.code, &op.payload);
        }
        at.push(pos);
        let place = |i: usize, to: usize| -> Result<usize, String> {
            at.get(to).copied().ok_or_else(|| format!("record {i} points at record {to}, past the block's {} records", self.ops.len()))
        };
        let mut out = Vec::with_capacity(pos + self.after.len());
        for (i, op) in self.ops.iter().enumerate() {
            let next = at[i + 1];
            let mut payload = op.payload.clone();
            let size = |from: usize, to: usize| -> Result<[u8; 2], String> {
                let n = to.checked_sub(from).ok_or_else(|| format!("record {i}: a block that ends before it starts"))?;
                Ok(u16::try_from(n).map_err(|_| format!("record {i}: a {n}-byte block past 65535"))?.to_le_bytes())
            };
            let want = match op.code {
                ACTION_JUMP | ACTION_IF | ACTION_DEFINE_FUNCTION | ACTION_DEFINE_FUNCTION2 | ACTION_WITH => 1,
                ACTION_TRY => 3,
                _ => 0,
            };
            if op.to.len() != want {
                return Err(format!("record {i} ({:#x}) has {} positions, not {want}", op.code, op.to.len()));
            }
            match op.code {
                ACTION_JUMP | ACTION_IF => {
                    if payload.len() != 2 {
                        return Err(format!("record {i}: a branch with a {}-byte payload, not 2", payload.len()));
                    }
                    let delta = place(i, op.to[0])? as isize - next as isize;
                    let off = i16::try_from(delta).map_err(|_| format!("record {i}: a branch of {delta} bytes past 16 bits"))?;
                    payload.copy_from_slice(&off.to_le_bytes());
                }
                ACTION_DEFINE_FUNCTION | ACTION_DEFINE_FUNCTION2 => {
                    let n = payload.len();
                    if n < 2 {
                        return Err(format!("record {i}: a function with no code size"));
                    }
                    let s = size(next, place(i, op.to[0])?)?;
                    payload[n - 2..].copy_from_slice(&s);
                }
                ACTION_WITH => {
                    let s = size(next, place(i, op.to[0])?)?;
                    payload.get_mut(0..2).ok_or_else(|| format!("record {i}: a With with no size"))?.copy_from_slice(&s);
                }
                ACTION_TRY => {
                    let mut from = next;
                    for (k, at_field) in TRY_SIZES.into_iter().enumerate() {
                        let end = place(i, op.to[k])?;
                        let s = size(from, end)?;
                        payload.get_mut(at_field..at_field + 2).ok_or_else(|| format!("record {i}: a Try too short for its sizes"))?.copy_from_slice(&s);
                        from = end;
                    }
                }
                _ => {}
            }
            out.push(op.code);
            if op.code >= 0x80 {
                let len = u16::try_from(payload.len()).map_err(|_| format!("record {i}: a {}-byte payload past 65535", payload.len()))?;
                out.extend(len.to_le_bytes());
                out.extend(&payload);
            } else if !payload.is_empty() {
                return Err(format!("record {i} ({:#x}) is below 0x80 and cannot carry a payload", op.code));
            }
        }
        out.extend(&self.after);
        Ok(out)
    }

    /// Inserts `new` before record `at`, so a position that named record `at` or later now names the same
    /// record. A position held by `new` is relative to `new` itself (0 = its first record, `new.len()` =
    /// the record after it) unless it is listed in `outside`, which keeps absolute positions of the
    /// program as it was before the insert.
    pub fn insert(&mut self, at: usize, new: Vec<Op>, outside: &[(usize, usize)]) -> Result<(), String> {
        if at > self.ops.len() {
            return Err(format!("insert at record {at}, past the block's {} records", self.ops.len()));
        }
        let n = new.len();
        for op in &mut self.ops {
            for t in &mut op.to {
                if *t >= at {
                    *t += n;
                }
            }
        }
        let mut new = new;
        for (k, op) in new.iter_mut().enumerate() {
            for (j, t) in op.to.iter_mut().enumerate() {
                if outside.contains(&(k, j)) {
                    if *t > self.ops.len() {
                        return Err(format!("inserted record {k} points at record {t}, past the block"));
                    }
                    if *t >= at {
                        *t += n;
                    }
                } else {
                    if *t > n {
                        return Err(format!("inserted record {k} points at its record {t}, past the {n} inserted"));
                    }
                    *t += at;
                }
            }
        }
        self.ops.splice(at..at, new);
        Ok(())
    }

    /// Records `from..to` replaced by `new`. A position elsewhere that named record `from` now names the
    /// first new record; one that named a later record in the range is refused. Positions held by `new`
    /// follow [`Program::insert`]'s rule: relative to `new` unless listed in `outside`, which are positions
    /// of the program as it was.
    pub fn replace(&mut self, from: usize, to: usize, new: Vec<Op>, outside: &[(usize, usize)]) -> Result<(), String> {
        if from > to || to > self.ops.len() {
            return Err(format!("replace records {from}..{to} of a block of {}", self.ops.len()));
        }
        let n = new.len();
        let gone = to - from;
        let map = |t: usize, who: &str| -> Result<usize, String> {
            if t <= from {
                Ok(t)
            } else if t < to {
                Err(format!("{who} points at record {t}, inside the replaced records {from}..{to}"))
            } else {
                Ok(t - gone + n)
            }
        };
        for (i, op) in self.ops.iter_mut().enumerate() {
            if (from..to).contains(&i) {
                continue;
            }
            for t in &mut op.to {
                *t = map(*t, &format!("record {i}"))?;
            }
        }
        let mut new = new;
        for (k, op) in new.iter_mut().enumerate() {
            for (j, t) in op.to.iter_mut().enumerate() {
                if outside.contains(&(k, j)) {
                    *t = map(*t, &format!("new record {k}"))?;
                } else if *t > n {
                    return Err(format!("new record {k} points at its record {t}, past the {n} new"));
                } else {
                    *t += from;
                }
            }
        }
        self.ops.splice(from..to, new);
        Ok(())
    }

    /// Records `from..to` copied for [`Program::insert`] or [`Program::replace`]: a position inside the
    /// range (or at its end) becomes relative to it, any other stays absolute and is listed in the second
    /// value.
    pub fn clone_range(&self, from: usize, to: usize) -> Result<(Vec<Op>, Vec<(usize, usize)>), String> {
        let ops = self.ops.get(from..to).ok_or_else(|| format!("records {from}..{to} of a block of {}", self.ops.len()))?;
        let mut outside = Vec::new();
        let mut out = Vec::with_capacity(ops.len());
        for (k, op) in ops.iter().enumerate() {
            let mut op = op.clone();
            for (j, t) in op.to.iter_mut().enumerate() {
                if (from..=to).contains(t) {
                    *t -= from;
                } else {
                    outside.push((k, j));
                }
            }
            out.push(op);
        }
        Ok((out, outside))
    }

    /// The strings of the block's `ConstantPool`, which must be its first record when it has one.
    pub fn pool(&self) -> Result<Vec<Vec<u8>>, String> {
        let mut pools = self.ops.iter().enumerate().filter(|(_, o)| o.code == ACTION_CONSTANT_POOL);
        match (pools.next(), pools.next()) {
            (None, _) => Ok(Vec::new()),
            (Some((0, op)), None) => pool(&op.payload),
            (Some((i, _)), None) => Err(format!("the ConstantPool is record {i}, not the block's first")),
            (Some(_), Some(_)) => Err("the block has more than one ConstantPool".into()),
        }
    }

    /// Edits record `i`, a `Push`, through its values with pool strings resolved: `edit` sees each value
    /// as [`Value`] and may change it. A value `edit` leaves equal is written as it was stored (a pool
    /// reference stays one); a changed string is pushed inline.
    pub fn edit_push(&mut self, i: usize, pool: &[Vec<u8>], edit: &mut dyn FnMut(&mut [Value])) -> Result<bool, String> {
        let op = self.ops.get(i).ok_or_else(|| format!("no record {i}"))?;
        if op.code != ACTION_PUSH {
            return Err(format!("record {i} is {:#x}, not a Push", op.code));
        }
        let stored = push_values(&op.payload)?;
        let before = stored.iter().map(|v| Value::of(v, pool)).collect::<Result<Vec<_>, _>>()?;
        let mut after = before.clone();
        edit(&mut after);
        if after.len() != before.len() {
            return Err(format!("record {i}: an edit that changes how many values the Push has"));
        }
        if after == before {
            return Ok(false);
        }
        let written = stored
            .iter()
            .zip(before.iter().zip(&after))
            .map(|(s, (b, a))| if a == b { Ok(s.clone()) } else { a.stored(s) })
            .collect::<Result<Vec<_>, String>>()?;
        self.ops[i].payload = push_payload(&written)?;
        Ok(true)
    }

    /// The values of record `i`, a `Push`, with pool strings resolved.
    pub fn push_at(&self, i: usize, pool: &[Vec<u8>]) -> Result<Vec<Value>, String> {
        let op = self.ops.get(i).ok_or_else(|| format!("no record {i}"))?;
        if op.code != ACTION_PUSH {
            return Err(format!("record {i} is {:#x}, not a Push", op.code));
        }
        push_values(&op.payload)?.iter().map(|v| Value::of(v, pool)).collect()
    }

    /// Every `switch` on `var` the compiler wrote: `<load var>; StoreRegister rK; Push v1; StrictEquals;
    /// If; (Push rK, v; StrictEquals; If)*; Jump default`.
    pub fn switches(&self, var: &str, pool: &[Vec<u8>]) -> Result<Vec<Switch>, String> {
        let mut out = Vec::new();
        for i in 2..self.ops.len() {
            if self.ops[i].code != ACTION_STORE_REGISTER || !matches!(self.ops[i - 1].code, ACTION_GET_MEMBER | ACTION_GET_VARIABLE) {
                continue;
            }
            if self.ops[i - 2].code != ACTION_PUSH || self.push_at(i - 2, pool)?.last() != Some(&Value::Str(var.as_bytes().to_vec())) {
                continue;
            }
            let reg = *self.ops[i].payload.first().ok_or_else(|| format!("record {i}: StoreRegister with no register"))?;
            let mut cases = Vec::new();
            let mut k = i + 1;
            loop {
                let (Some(push), Some(eq), Some(branch)) = (self.ops.get(k), self.ops.get(k + 1), self.ops.get(k + 2)) else { break };
                if push.code != ACTION_PUSH || eq.code != ACTION_STRICT_EQUALS || branch.code != ACTION_IF {
                    break;
                }
                let values = self.push_at(k, pool)?;
                let value = match (cases.is_empty(), &values[..]) {
                    (true, [v]) => v.number(),
                    (_, [Value::Register(r), v]) if *r == reg => v.number(),
                    _ => None,
                };
                let Some(value) = value else { break };
                cases.push(Case { value, dispatch: k, body: branch.to[0] });
                k += 3;
            }
            if cases.is_empty() || self.ops.get(k).map(|o| o.code) != Some(ACTION_JUMP) {
                continue;
            }
            out.push(Switch { reg, cases, default: k, end: self.ops[k].to[0] });
        }
        Ok(out)
    }

    /// Adds case `value` to switch `s`, its body `body` (positions relative to it unless listed in
    /// `outside`, which are positions of the program before this call) placed at the switch's default
    /// target, so every case and the default still reach what they reached. Returns where the body starts.
    pub fn add_case(&mut self, s: &Switch, value: &Value, body: Vec<Op>, outside: &[(usize, usize)]) -> Result<usize, String> {
        let anchor = s.cases.last().ok_or("a switch with no case")?;
        // The new value is stored as the last case's is.
        let like = push_values(&self.ops[anchor.dispatch].payload)?.last().cloned().ok_or("an empty case Push")?;
        let at = s.end;
        self.insert(at, body, outside)?;
        let dispatch = vec![
            Op { code: ACTION_PUSH, payload: push_payload(&[PushValue::Register(s.reg), value.stored(&like)?])?, to: vec![] },
            Op { code: ACTION_STRICT_EQUALS, payload: vec![], to: vec![] },
            Op { code: ACTION_IF, payload: vec![0, 0], to: vec![at] },
        ];
        let n = dispatch.len();
        self.insert(s.default, dispatch, &[(2, 0)])?;
        Ok(at + n)
    }
}

/// One `case` of a [`Switch`].
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    pub value: f64,
    /// The record of its `Push` in the dispatch.
    pub dispatch: usize,
    /// Where its body starts.
    pub body: usize,
}

/// A compiled `switch`: its cases in dispatch order, the default `Jump` and where that lands.
#[derive(Debug, Clone, PartialEq)]
pub struct Switch {
    pub reg: u8,
    pub cases: Vec<Case>,
    pub default: usize,
    pub end: usize,
}

impl Switch {
    /// The case whose value is `value`.
    pub fn case(&self, value: f64) -> Result<&Case, String> {
        self.cases.iter().find(|c| c.value == value).ok_or_else(|| format!("the switch has no case {value}"))
    }

    /// Where case `value`'s body ends: the next body that starts after it, or the switch's end.
    pub fn body_end(&self, value: f64) -> Result<usize, String> {
        let start = self.case(value)?.body;
        Ok(self.cases.iter().map(|c| c.body).filter(|&b| b > start).chain(std::iter::once(self.end)).min().unwrap_or(self.end))
    }
}

/// A `Push` value with its pool reference resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(Vec<u8>),
    Number(f64),
    Null,
    Undefined,
    Register(u8),
    Bool(bool),
}

/// How a number is stored, so an edited one is written the way the original was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberKind {
    Int,
    Double,
    Float,
}

impl Value {
    fn of(v: &PushValue, pool: &[Vec<u8>]) -> Result<Value, String> {
        let constant = |i: usize| pool.get(i).cloned().map(Value::Str).ok_or_else(|| format!("a Push of constant {i}, past the pool's {}", pool.len()));
        Ok(match v {
            PushValue::Str(s) => Value::Str(s.clone()),
            PushValue::Float(bits) => Value::Number(f64::from(f32::from_bits(*bits))),
            PushValue::Null => Value::Null,
            PushValue::Undefined => Value::Undefined,
            PushValue::Register(r) => Value::Register(*r),
            PushValue::Bool(b) => Value::Bool(*b != 0),
            PushValue::Double(d) => Value::Number(f64::from_le_bytes([d[4], d[5], d[6], d[7], d[0], d[1], d[2], d[3]])),
            PushValue::Int(i) => Value::Number(f64::from(*i as i32)),
            PushValue::Constant8(i) => constant(usize::from(*i))?,
            PushValue::Constant16(i) => constant(usize::from(*i))?,
        })
    }

    /// The number, when this is one.
    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// This value as stored in place of `like`: a number keeps `like`'s storage, a string is inline.
    fn stored(&self, like: &impl StoredLike) -> Result<PushValue, String> {
        Ok(match self {
            Value::Str(s) => PushValue::Str(s.clone()),
            Value::Number(n) => match like.kind() {
                NumberKind::Int if n.fract() == 0.0 && *n >= f64::from(i32::MIN) && *n <= f64::from(i32::MAX) => PushValue::Int(*n as i32 as u32),
                NumberKind::Float => PushValue::Float((*n as f32).to_bits()),
                _ => {
                    let b = n.to_le_bytes();
                    PushValue::Double([b[4], b[5], b[6], b[7], b[0], b[1], b[2], b[3]])
                }
            },
            Value::Null => PushValue::Null,
            Value::Undefined => PushValue::Undefined,
            Value::Register(r) => PushValue::Register(*r),
            Value::Bool(b) => PushValue::Bool(u8::from(*b)),
        })
    }
}

/// What decides how an edited number is stored.
trait StoredLike {
    fn kind(&self) -> NumberKind;
}

impl StoredLike for PushValue {
    fn kind(&self) -> NumberKind {
        match self {
            PushValue::Double(_) => NumberKind::Double,
            PushValue::Float(_) => NumberKind::Float,
            _ => NumberKind::Int,
        }
    }
}

/// `block` with `from` replaced by `to` in its leading `ConstantPool`, or `None` when the pool does not
/// hold `from`.
fn swap_in_block(block: &[u8], from: &str, to: &str) -> Result<Option<Vec<u8>>, String> {
    let acts = actions(block)?;
    for (code, start, payload) in &acts {
        if *code == ACTION_PUSH && push_values(payload)?.iter().any(|v| matches!(v, PushValue::Str(s) if s == from.as_bytes())) {
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

const ACTION_GET_VARIABLE: u8 = 0x1c;
const ACTION_GET_MEMBER: u8 = 0x4e;
const ACTION_STRICT_EQUALS: u8 = 0x66;
const ACTION_STORE_REGISTER: u8 = 0x87;
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

/// Applies `edit` to the named `DoAction`s of a tag list; `in_sprite` is the sprite id when the list is a
/// sprite's.
fn rewrite_tags(
    tags: &mut [Tag],
    in_sprite: Option<u16>,
    targets: &[Target],
    edit: &mut BlockEdit<'_>,
    done: &mut Vec<(Target, usize)>,
) -> Result<(), String> {
    let mut frame = 0u32;
    for tag in tags {
        match &mut tag.body {
            TagBody::Raw(body) if tag.code == gfx_tags::DO_ACTION => {
                let here = match in_sprite {
                    None => Target::Root(frame),
                    Some(id) => Target::Sprite(id, frame),
                };
                if targets.contains(&here) {
                    if let Some(new) = edit(here, body)? {
                        done.push((here, new.len()));
                        *body = new;
                    }
                }
            }
            TagBody::Sprite { id, tags, .. } if in_sprite.is_none() => {
                if targets.iter().any(|t| matches!(t, Target::Sprite(s, _) if s == id)) {
                    rewrite_tags(tags, Some(*id), targets, edit, done)?;
                }
            }
            _ => {}
        }
        if tag.code == gfx_tags::SHOW_FRAME {
            frame += 1;
        }
    }
    Ok(())
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
/// and the blocks `edit` replaced with their new lengths. Read and written by [`TagStream`]: tag, sprite and
/// file lengths follow, a tag keeps its header form, and bytes after `End` stay, uncounted.
fn rewrite_movie(data: &[u8], targets: &[Target], edit: &mut BlockEdit<'_>) -> Result<(Vec<u8>, Vec<(Target, usize)>), String> {
    let mut stream = TagStream::parse(data)?;
    let mut done = Vec::new();
    rewrite_tags(&mut stream.tags, None, targets, edit, &mut done)?;
    Ok((stream.file()?, done))
}

/// Whether the edited movie `out` still decodes, as every edit's result must.
pub fn check_decodes(out: &[u8]) -> Result<(), String> {
    crate::gfx_def::MovieDef::read(out)
        .map(|_| ())
        .map_err(|e| format!("the rewritten movie does not decode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    const TAG_END: u16 = 0;
    const TAG_SHOW_FRAME: u16 = 1;
    const TAG_DO_ACTION: u16 = 12;
    const TAG_DEFINE_SPRITE: u16 = 39;

    fn write_tag(out: &mut Vec<u8>, code: u16, long: bool, body: &[u8]) -> Result<(), String> {
        if long || body.len() >= 0x3f {
            out.extend_from_slice(&((code << 6) | 0x3f).to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        } else {
            out.extend_from_slice(&((code << 6) | body.len() as u16).to_le_bytes());
        }
        out.extend_from_slice(body);
        Ok(())
    }

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

    /// A block's records, every Push value type and a pool write back to their bytes.
    #[test]
    fn records_values_and_pools_write_back() {
        let b = block(&["Platform", "PS3"]);
        let (acts, after) = read_actions(&b).unwrap();
        assert!(after.is_empty());
        assert_eq!(write_actions(&acts, &after).unwrap(), b);
        let mut tail = b.clone();
        tail.extend([0xAA, 0xBB]);
        let (acts, after) = read_actions(&tail).unwrap();
        assert_eq!(after, [0xAA, 0xBB], "bytes after End are kept");
        assert_eq!(write_actions(&acts, &after).unwrap(), tail);
        let push = [0u8, b'a', 0, 1, 0, 0, 0x80, 0x3f, 2, 3, 4, 7, 5, 1, 6, 1, 2, 3, 4, 5, 6, 7, 8, 7, 9, 0, 0, 0, 8, 3, 9, 1, 2];
        let values = push_values(&push).unwrap();
        assert_eq!(values.len(), 10, "one of each type");
        assert_eq!(push_payload(&values).unwrap(), push);
        let strings = super::pool(&acts[0].payload).unwrap();
        assert_eq!(strings, [b"Platform".to_vec(), b"PS3".to_vec()]);
        assert_eq!(pool_payload(&strings).unwrap(), acts[0].payload);
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
        assert!(e.contains("through End") && e.contains("5 bytes follow End"), "{e}");
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

    /// A block with a forward `If`, a backward `Jump`, a `DefineFunction2` whose body holds a `Push`, and
    /// bytes after `End`.
    fn branchy() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend([ACTION_PUSH, 2, 0, 5, 1]); // 0: Push true
        b.extend([ACTION_IF, 2, 0, 1, 0]); // 5: If over the Pop
        b.push(0x17); // 10: Pop
        // 11: DefineFunction2 "f", 0 params, 1 register, flags 0, size 5 (the Push below)
        b.extend([ACTION_DEFINE_FUNCTION2, 9, 0, b'f', 0, 0, 0, 1, 0, 0, 5, 0]);
        b.extend([ACTION_PUSH, 2, 0, 5, 0]); // 23: Push false
        b.extend([ACTION_JUMP, 2, 0]); // 28: Jump back to the Pop at 10
        b.extend((10i16 - 33).to_le_bytes());
        b.push(0); // 33: End
        b.extend([0xaa, 0xbb]);
        b
    }

    #[test]
    fn a_program_encodes_back_to_its_block() {
        let block = branchy();
        let p = Program::decode(&block).unwrap();
        assert_eq!(p.ops.len(), 7);
        assert_eq!(p.ops[1].to, vec![3], "the If lands on the DefineFunction2");
        assert_eq!(p.ops[3].to, vec![5], "the function's body is the Push");
        assert_eq!(p.ops[5].to, vec![2], "the Jump lands on the Pop");
        assert_eq!(p.after, vec![0xaa, 0xbb]);
        assert_eq!(p.encode().unwrap(), block);
    }

    #[test]
    fn an_insert_moves_every_position_after_it() {
        let mut p = Program::decode(&branchy()).unwrap();
        // Two records inside the function body, before its Push: a Pop, and a Jump to the body's end.
        let new = vec![
            Op { code: 0x17, payload: vec![], to: vec![] },
            Op { code: ACTION_JUMP, payload: vec![0, 0], to: vec![2] },
        ];
        p.insert(4, new, &[]).unwrap();
        assert_eq!(p.ops[3].to, vec![7], "the function's body now ends after the moved Push");
        assert_eq!(p.ops[5].to, vec![6], "the inserted Jump lands on the record after the insert");
        assert_eq!(p.ops[7].to, vec![2], "the Jump before the insert still lands on the Pop");
        let out = p.encode().unwrap();
        let again = Program::decode(&out).unwrap();
        assert_eq!(again, p, "the encoded block reads back as the edited program");
        assert_eq!(&out[out.len() - 2..], &[0xaa, 0xbb]);
        // The function's code size covers the Pop, the Jump and the Push: 1 + 5 + 5.
        assert_eq!(&out[21..23], &11u16.to_le_bytes());
    }

    #[test]
    fn a_position_off_a_record_is_refused() {
        let mut block = branchy();
        block[8] = 2; // the If now lands one byte into the DefineFunction2
        let e = Program::decode(&block).unwrap_err();
        assert!(e.contains("not on a record"), "{e}");
    }
}
