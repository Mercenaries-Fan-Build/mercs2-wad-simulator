//! The ActionScript library: what the retail movies call, nothing they do not. A script reaching for
//! anything else lands in [`Player::missing`], which the gate reports.

use std::rc::Rc;

use mercs2_formats::gfx_def::Character;

use crate::avm::{to_number, Kind, Listeners, Native, Obj, Player, Text, Value};
use crate::stage::Node;

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}

fn method(p: &Player, on: &Obj, name: &str, f: Native) {
    on.set(name, p.native(f));
}

fn items_of(v: &Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Object(o) => match &o.0.borrow().kind {
            Kind::Array(items) => Ok(items.clone()),
            _ => Err("an Array method on something else".into()),
        },
        _ => Err("an Array method on something else".into()),
    }
}

fn with_items<T>(v: &Value, f: impl FnOnce(&mut Vec<Value>) -> T) -> Result<T, String> {
    match v {
        Value::Object(o) => match &mut o.0.borrow_mut().kind {
            Kind::Array(items) => Ok(f(items)),
            _ => Err("an Array method on something else".into()),
        },
        _ => Err("an Array method on something else".into()),
    }
}

/// A slice position: negative counts from the end, clamped to the length.
fn position(v: &Value, len: usize, otherwise: usize) -> usize {
    match v {
        Value::Undefined => otherwise,
        _ => {
            let n = to_number(v);
            if n.is_nan() {
                0
            } else if n < 0.0 {
                (len as f64 + n).max(0.0) as usize
            } else {
                (n as usize).min(len)
            }
        }
    }
}

fn array_join(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let sep = match arg(args, 0) {
        Value::Undefined => Text::from(","),
        other => p.to_text(&other)?,
    };
    let mut parts = Vec::new();
    for item in items_of(this)? {
        parts.push(p.to_text(&item)?.to_string());
    }
    Ok(Value::Str(parts.join(&sep).into()))
}

fn array_push(_: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let n = with_items(this, |items| {
        items.extend_from_slice(args);
        items.len()
    })?;
    Ok(Value::Number(n as f64))
}

fn array_unshift(_: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let n = with_items(this, |items| {
        items.splice(0..0, args.iter().cloned());
        items.len()
    })?;
    Ok(Value::Number(n as f64))
}

fn array_slice(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let items = items_of(this)?;
    let from = position(&arg(args, 0), items.len(), 0);
    let to = position(&arg(args, 1), items.len(), items.len());
    Ok(p.new_array(if from < to { items[from..to].to_vec() } else { Vec::new() }))
}

fn array_splice(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let len = items_of(this)?.len();
    let from = position(&arg(args, 0), len, 0);
    let count = match arg(args, 1) {
        Value::Undefined => len - from,
        other => (to_number(&other).max(0.0) as usize).min(len - from),
    };
    let inserted: Vec<Value> = args.iter().skip(2).cloned().collect();
    let removed: Vec<Value> = with_items(this, |items| items.splice(from..from + count, inserted).collect())?;
    Ok(p.new_array(removed))
}

/// `sortOn(name, options)`: option 1 ignores case, 2 descends, 16 compares numbers.
fn array_sort_on(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let name = p.to_text(&arg(args, 0))?;
    let options = to_number(&arg(args, 1));
    let options = if options.is_nan() { 0 } else { options as u32 };
    let mut keyed = Vec::new();
    for item in items_of(this)? {
        keyed.push((p.get_member(&item, &name)?, item));
    }
    let mut texts = Vec::with_capacity(keyed.len());
    for (key, _) in &keyed {
        let t = p.to_text(key)?.to_string();
        texts.push(if options & 1 != 0 { t.to_lowercase() } else { t });
    }
    let mut order: Vec<usize> = (0..keyed.len()).collect();
    order.sort_by(|a, b| {
        let by = if options & 16 != 0 {
            to_number(&keyed[*a].0).partial_cmp(&to_number(&keyed[*b].0)).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            texts[*a].cmp(&texts[*b])
        };
        if options & 2 != 0 { by.reverse() } else { by }
    });
    let sorted: Vec<Value> = order.into_iter().map(|i| keyed[i].1.clone()).collect();
    with_items(this, |items| *items = sorted)?;
    Ok(this.clone())
}

fn make_array(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    if let [Value::Number(n)] = args {
        return Ok(p.new_array(vec![Value::Undefined; n.max(0.0) as usize]));
    }
    Ok(p.new_array(args.to_vec()))
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn string_char_at(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let text = p.to_text(this)?;
    let units = utf16(&text);
    let i = to_number(&arg(args, 0));
    Ok(Value::Str(match units.get(if i >= 0.0 { i as usize } else { usize::MAX }) {
        Some(u) => String::from_utf16_lossy(&[*u]).as_str().into(),
        None => "".into(),
    }))
}

fn string_substring(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let text = p.to_text(this)?;
    let units = utf16(&text);
    let clamp = |v: &Value, otherwise: usize| match v {
        Value::Undefined => otherwise,
        other => (to_number(other).max(0.0) as usize).min(units.len()),
    };
    let a = clamp(&arg(args, 0), 0);
    let b = clamp(&arg(args, 1), units.len());
    Ok(Value::Str(String::from_utf16_lossy(&units[a.min(b)..a.max(b)]).as_str().into()))
}

fn string_split(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let text = p.to_text(this)?;
    let items = match arg(args, 0) {
        Value::Undefined => vec![Value::Str(text)],
        sep => {
            let sep = p.to_text(&sep)?;
            if sep.is_empty() {
                text.chars().map(|c| Value::Str(c.to_string().as_str().into())).collect()
            } else {
                text.split(&*sep).map(|s| Value::Str(s.into())).collect()
            }
        }
    };
    Ok(p.new_array(items))
}

fn string_slice(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let text = p.to_text(this)?;
    let units = utf16(&text);
    let from = position(&arg(args, 0), units.len(), 0);
    let to = position(&arg(args, 1), units.len(), units.len());
    Ok(Value::Str(if from < to { String::from_utf16_lossy(&units[from..to]).as_str().into() } else { "".into() }))
}

fn string_index_of(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let text = p.to_text(this)?;
    let needle = p.to_text(&arg(args, 0))?;
    Ok(Value::Number(text.find(&*needle).map_or(-1.0, |i| text[..i].encode_utf16().count() as f64)))
}

fn string_upper(p: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Str(p.to_text(this)?.to_uppercase().as_str().into()))
}

fn string_lower(p: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Str(p.to_text(this)?.to_lowercase().as_str().into()))
}

fn make_string(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Str(if args.is_empty() { "".into() } else { p.to_text(&args[0])? }))
}

fn make_object(p: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(match this {
        Value::Object(_) => this.clone(),
        _ => Value::Object(p.new_object()),
    })
}

fn object_to_string(_: &mut Player, _: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Str("[object Object]".into()))
}

fn function_call(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let rest: Vec<Value> = args.iter().skip(1).cloned().collect();
    p.call(this, &arg(args, 0), &rest)
}

fn math_floor(_: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(to_number(&arg(args, 0)).floor()))
}

fn math_ceil(_: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(to_number(&arg(args, 0)).ceil()))
}

fn math_round(_: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number((to_number(&arg(args, 0)) + 0.5).floor()))
}

fn math_abs(_: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(to_number(&arg(args, 0)).abs()))
}

fn math_max(_: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(args.iter().map(to_number).fold(f64::NEG_INFINITY, f64::max)))
}

fn math_min(_: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(args.iter().map(to_number).fold(f64::INFINITY, f64::min)))
}

fn math_random(p: &mut Player, _: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(p.random_below(1 << 30) as f64 / (1u32 << 30) as f64))
}

fn parse_int(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    let text = p.to_text(&arg(args, 0))?;
    let radix = match arg(args, 1) {
        Value::Undefined => 10,
        other => to_number(&other) as u32,
    };
    let t = text.trim();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, t.strip_prefix('+').unwrap_or(t)),
    };
    let end = digits.find(|c: char| !c.is_digit(radix)).unwrap_or(digits.len());
    Ok(Value::Number(match i64::from_str_radix(&digits[..end], radix) {
        Ok(n) => sign * n as f64,
        Err(_) => f64::NAN,
    }))
}

fn is_nan(_: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Bool(to_number(&arg(args, 0)).is_nan()))
}

fn set_interval(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    // `setInterval(function, ms, …)` or `setInterval(object, "method", ms, …)`.
    let (func, this, rest) = match args {
        [Value::Object(o), Value::Str(name), rest @ ..] => {
            let func = p.get_member(&Value::Object(o.clone()), name)?;
            (func, Value::Object(o.clone()), rest)
        }
        [Value::Clip(c), Value::Str(name), rest @ ..] => (p.get_member(&Value::Clip(*c), name)?, Value::Clip(*c), rest),
        [func, rest @ ..] => (func.clone(), Value::Undefined, rest),
        [] => return Ok(Value::Undefined),
    };
    let period = to_number(&arg(rest, 0));
    let id = p.set_interval(func, this, period, rest.iter().skip(1).cloned().collect());
    Ok(Value::Number(id as f64))
}

fn clear_interval(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    p.clear_interval(to_number(&arg(args, 0)) as u32);
    Ok(Value::Undefined)
}

fn clip_of(this: &Value) -> Result<usize, String> {
    match this {
        Value::Clip(id) => Ok(*id),
        other => Err(format!("a MovieClip method on {other:?}")),
    }
}

fn clip_play(p: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    p.movie.clip_mut(clip_of(this)?)?.playing = true;
    Ok(Value::Undefined)
}

fn clip_stop(p: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    p.movie.clip_mut(clip_of(this)?)?.playing = false;
    Ok(Value::Undefined)
}

fn goto(p: &mut Player, this: &Value, args: &[Value], playing: bool) -> Result<Value, String> {
    let clip = clip_of(this)?;
    if let Some(frame) = p.frame_of(clip, &arg(args, 0))? {
        p.movie.goto(clip, frame)?;
        p.movie.clip_mut(clip)?.playing = playing;
    }
    Ok(Value::Undefined)
}

fn clip_goto_play(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    goto(p, this, args, true)
}

fn clip_goto_stop(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    goto(p, this, args, false)
}

fn clip_attach(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let clip = clip_of(this)?;
    let symbol = p.to_text(&arg(args, 0))?;
    let name = p.to_text(&arg(args, 1))?;
    let depth = to_number(&arg(args, 2)) as i32;
    let made = p.movie.attach(clip, &symbol, &name, depth)?;
    if let (Some(id), Value::Object(init)) = (made, arg(args, 3)) {
        let keys = init.0.borrow().order.clone();
        for key in keys {
            let value = init.get(&key).unwrap_or(Value::Undefined);
            p.set_member(&Value::Clip(id), &key, value)?;
        }
    }
    p.run_queued()?;
    Ok(made.map_or(Value::Undefined, Value::Clip))
}

fn clip_duplicate(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let clip = clip_of(this)?;
    let Some((parent, depth)) = p.movie.child_of(clip)? else { return Ok(Value::Undefined) };
    let child = p.field(parent, depth)?.clone();
    let name = p.to_text(&arg(args, 0))?;
    let at = to_number(&arg(args, 1)) as i32;
    let made = p.movie.script_child(parent, child.character, &name, at, child.matrix, child.cxform)?;
    p.run_queued()?;
    Ok(made.map_or(Value::Undefined, Value::Clip))
}

fn clip_remove(p: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    let clip = clip_of(this)?;
    if let Some((parent, depth)) = p.movie.child_of(clip)? {
        p.movie.remove(parent, depth)?;
    }
    Ok(Value::Undefined)
}

fn clip_next_depth(p: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    let highest = p.movie.clip(clip_of(this)?)?.children.keys().next_back().copied().unwrap_or(-1);
    Ok(Value::Number((highest.max(-1) + 1) as f64))
}

/// `hitTest(x, y, shapeFlag)` or `hitTest(other)`, by bounds: the movies pass no shape flag.
fn clip_hit_test(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let bounds = |p: &mut Player, v: &Value| -> Result<[f32; 4], String> {
        let parent = p.get_member(v, "_parent")?;
        let [xmin, xmax, ymin, ymax] = p.bounds(v)?;
        let [ax, ay] = local_to_stage(p, &parent, xmin / 20.0, ymin / 20.0)?;
        let [bx, by] = local_to_stage(p, &parent, xmax / 20.0, ymax / 20.0)?;
        Ok([ax.min(bx), ax.max(bx), ay.min(by), ay.max(by)])
    };
    let mine = bounds(p, this)?;
    Ok(Value::Bool(match args {
        [other] => {
            let theirs = bounds(p, other)?;
            mine[0] <= theirs[1] && theirs[0] <= mine[1] && mine[2] <= theirs[3] && theirs[2] <= mine[3]
        }
        [x, y, ..] => {
            let (x, y) = (to_number(x) as f32, to_number(y) as f32);
            mine[0] <= x && x <= mine[1] && mine[2] <= y && y <= mine[3]
        }
        [] => false,
    }))
}

fn local_to_stage(p: &mut Player, clip: &Value, x: f32, y: f32) -> Result<[f32; 2], String> {
    let origin = p.stage_to_local(clip, 0.0, 0.0)?;
    let unit_x = p.stage_to_local(clip, 1.0, 0.0)?;
    let unit_y = p.stage_to_local(clip, 0.0, 1.0)?;
    // The inverse of the affine map those three points pin down.
    let (a, b) = (unit_x[0] - origin[0], unit_x[1] - origin[1]);
    let (c, d) = (unit_y[0] - origin[0], unit_y[1] - origin[1]);
    let det = a * d - b * c;
    if det == 0.0 {
        return Ok([x, y]);
    }
    let (dx, dy) = (x - origin[0], y - origin[1]);
    Ok([(d * dx - c * dy) / det, (a * dy - b * dx) / det])
}

fn key_is_down(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    Ok(Value::Bool(p.keys.contains(&(to_number(&arg(args, 0)) as u32))))
}

fn key_get_code(p: &mut Player, _: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(p.last_key().0 as f64))
}

fn key_get_ascii(p: &mut Player, _: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(p.last_key().1 as f64))
}

/// A listener is held once however often it is added, or a frame script that re-runs doubles its keys.
fn add_listener(p: &mut Player, of: Listeners, who: Value) -> Result<Value, String> {
    remove_listener(p, of, &who)?;
    p.listeners(of).push(who);
    Ok(Value::Undefined)
}

fn key_add_listener(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    add_listener(p, Listeners::Key, arg(args, 0))
}

fn key_remove_listener(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    remove_listener(p, Listeners::Key, &arg(args, 0))
}

fn stage_add_listener(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    add_listener(p, Listeners::Stage, arg(args, 0))
}

fn stage_remove_listener(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    remove_listener(p, Listeners::Stage, &arg(args, 0))
}

fn mouse_add_listener(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    add_listener(p, Listeners::Mouse, arg(args, 0))
}

fn mouse_remove_listener(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    remove_listener(p, Listeners::Mouse, &arg(args, 0))
}

/// `Mouse.show()` / `Mouse.hide()`: the cursor is the host's, so these answer and change nothing here.
fn mouse_visibility(_: &mut Player, _: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Undefined)
}

fn remove_listener(p: &mut Player, of: Listeners, who: &Value) -> Result<Value, String> {
    let same = |a: &Value, b: &Value| match (a, b) {
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(&x.0, &y.0),
        (Value::Clip(x), Value::Clip(y)) => x == y,
        _ => false,
    };
    let list = p.listeners(of);
    let before = list.len();
    list.retain(|l| !same(l, who));
    Ok(Value::Bool(list.len() != before))
}

fn make_date(p: &mut Player, _: &Value, args: &[Value]) -> Result<Value, String> {
    let ms = match args.first() {
        Some(v) => to_number(v),
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("the clock is before the epoch: {e}"))?
            .as_millis() as f64,
    };
    Ok(p.date_object(ms))
}

/// Civil date of a time in UTC: `(year, zero-based month, day of month)`.
fn civil(ms: f64) -> (i64, u32, u32) {
    let days = (ms / 86_400_000.0).floor() as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + (month <= 2) as i64;
    (year, month - 1, day)
}

fn date_ms(this: &Value) -> Result<f64, String> {
    match this {
        Value::Object(o) => match o.0.borrow().kind {
            Kind::Date(ms) => Ok(ms),
            _ => Err("a Date method on something else".into()),
        },
        _ => Err("a Date method on something else".into()),
    }
}

fn date_year(_: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(civil(date_ms(this)?).0 as f64))
}

fn date_month(_: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(civil(date_ms(this)?).1 as f64))
}

fn date_day(_: &mut Player, this: &Value, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(civil(date_ms(this)?).2 as f64))
}

fn make_text_format(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    const NAMES: [&str; 13] = [
        "font", "size", "color", "bold", "italic", "underline", "url", "target", "align", "leftMargin", "rightMargin", "indent", "leading",
    ];
    let object = match this {
        Value::Object(o) => o.clone(),
        _ => p.new_object(),
    };
    for (name, value) in NAMES.iter().zip(args) {
        object.set(name, value.clone());
    }
    Ok(Value::Object(object))
}

fn field_slot(this: &Value) -> Result<(usize, i32), String> {
    match this {
        Value::Field(p, d) => Ok((*p, *d)),
        other => Err(format!("a TextField method on {other:?}")),
    }
}

/// `setTextFormat`: a field's whole text takes the format; [`field_get`] reads it back.
fn field_set_format(p: &mut Player, this: &Value, args: &[Value]) -> Result<Value, String> {
    let (parent, depth) = field_slot(this)?;
    let vars = p.field_vars_of(parent, depth);
    vars.set("__format", args.last().cloned().unwrap_or(Value::Undefined));
    Ok(Value::Undefined)
}

pub fn field_get(p: &mut Player, parent: usize, depth: i32, name: &str) -> Result<Value, String> {
    let child = p.field(parent, depth)?.clone();
    let def = match p.movie.def.characters.get(&child.character) {
        Some(Character::EditText(_)) => Some(p.movie.edit_text(&child)?),
        _ => None,
    };
    Ok(match (name, &child.node) {
        ("text" | "htmlText", Node::Text(text)) => Value::Str(text.as_str().into()),
        ("length", Node::Text(text)) => Value::Number(text.encode_utf16().count() as f64),
        ("multiline", _) => Value::Bool(def.as_ref().is_some_and(|d| d.multiline)),
        ("wordWrap", _) => Value::Bool(def.as_ref().is_some_and(|d| d.word_wrap)),
        ("html", _) => Value::Bool(def.as_ref().is_some_and(|d| d.html)),
        ("variable", _) => Value::Str(def.map_or(String::new(), |d| d.variable).as_str().into()),
        ("textWidth" | "textHeight", Node::Text(text)) => {
            let field = def.ok_or("a text field without its definition")?;
            let Some(font) = p.movie.font_of(&field)? else {
                p.movie.fontless = true;
                return Ok(Value::Number(0.0));
            };
            let [w, h] = crate::text::measure(&field, text, &font);
            Value::Number((if name == "textWidth" { w } else { h }) as f64 / 20.0)
        }
        _ => {
            let vars = p.field_vars_of(parent, depth);
            vars.get(name).unwrap_or(Value::Undefined)
        }
    })
}

pub fn field_set(p: &mut Player, parent: usize, depth: i32, name: &str, value: Value) -> Result<(), String> {
    match name {
        "text" | "htmlText" => {
            let text = p.to_text(&value)?.to_string();
            let child = p.movie.clip_mut(parent)?.children.get_mut(&depth).ok_or("the field is gone")?;
            child.node = Node::Text(text);
            fit_field(p, parent, depth)?;
        }
        "autoSize" => {
            p.field_vars_of(parent, depth).set(name, value);
            fit_field(p, parent, depth)?;
        }
        _ => p.field_vars_of(parent, depth).set(name, value),
    }
    Ok(())
}

/// A field whose `autoSize` is `"left"`, `"center"`, `"right"` or `true` (left) takes its text's
/// size plus the gutters, keeping that edge or its center where it was; one that wraps keeps its
/// width.
fn fit_field(p: &mut Player, parent: usize, depth: i32) -> Result<(), String> {
    let anchor = match p.field_vars_of(parent, depth).get("autoSize") {
        Some(Value::Bool(true)) => "left".to_string(),
        Some(v @ Value::Str(_)) => p.to_text(&v)?.to_string(),
        _ => return Ok(()),
    };
    let child = p.field(parent, depth)?.clone();
    let Node::Text(text) = &child.node else { return Ok(()) };
    let field = p.movie.edit_text(&child)?;
    let Some(font) = p.movie.font_of(&field)? else {
        p.movie.fontless = true;
        return Ok(());
    };
    let [w, h] = crate::text::measure(&field, text, &font);
    let gutters = crate::text::GUTTER * 2.0;
    let [mut xmin, mut xmax, ymin, _] = field.bounds;
    if !field.word_wrap {
        let width = (w + gutters).round() as i32;
        match anchor.as_str() {
            "left" => xmax = xmin + width,
            "center" => {
                let middle = (xmin + xmax) / 2;
                xmin = middle - width / 2;
                xmax = xmin + width;
            }
            "right" => xmin = xmax - width,
            _ => return Ok(()),
        }
    } else if !matches!(anchor.as_str(), "left" | "center" | "right") {
        return Ok(());
    }
    let ymax = ymin + (h + gutters).round() as i32;
    p.field_mut(parent, depth)?.bounds = Some([xmin, xmax, ymin, ymax]);
    Ok(())
}

/// Fill the global object and the prototypes.
pub fn install(p: &mut Player) {
    let [object_proto, function_proto, array_proto, string_proto, clip_proto, field_proto, date_proto] = p.protos().map(Obj::clone);
    let global = p.global.clone();

    method(p, &object_proto, "toString", object_to_string);
    method(p, &function_proto, "call", function_call);

    for (name, f) in [
        ("join", array_join as Native),
        ("push", array_push),
        ("unshift", array_unshift),
        ("slice", array_slice),
        ("splice", array_splice),
        ("sortOn", array_sort_on),
    ] {
        method(p, &array_proto, name, f);
    }
    for (name, f) in [
        ("charAt", string_char_at as Native),
        ("substring", string_substring),
        ("slice", string_slice),
        ("split", string_split),
        ("indexOf", string_index_of),
        ("toUpperCase", string_upper),
        ("toLowerCase", string_lower),
    ] {
        method(p, &string_proto, name, f);
    }
    for (name, f) in [
        ("play", clip_play as Native),
        ("stop", clip_stop),
        ("gotoAndPlay", clip_goto_play),
        ("gotoAndStop", clip_goto_stop),
        ("attachMovie", clip_attach),
        ("duplicateMovieClip", clip_duplicate),
        ("removeMovieClip", clip_remove),
        ("getNextHighestDepth", clip_next_depth),
        ("hitTest", clip_hit_test),
    ] {
        method(p, &clip_proto, name, f);
    }
    method(p, &field_proto, "setTextFormat", field_set_format);
    for (name, f) in [("getFullYear", date_year as Native), ("getMonth", date_month), ("getDate", date_day)] {
        method(p, &date_proto, name, f);
    }

    let constructor = |p: &Player, name: &str, f: Native, proto: Option<&Obj>| {
        let func = p.native(f);
        if let (Value::Object(o), Some(proto)) = (&func, proto) {
            o.set("prototype", Value::Object(proto.clone()));
        }
        global.set(name, func);
    };
    constructor(p, "Object", make_object, Some(&object_proto));
    constructor(p, "Array", make_array, Some(&array_proto));
    constructor(p, "String", make_string, Some(&string_proto));
    constructor(p, "Date", make_date, Some(&date_proto));
    constructor(p, "TextFormat", make_text_format, None);

    let math = p.new_object();
    for (name, f) in [
        ("floor", math_floor as Native),
        ("ceil", math_ceil),
        ("round", math_round),
        ("abs", math_abs),
        ("max", math_max),
        ("min", math_min),
        ("random", math_random),
    ] {
        method(p, &math, name, f);
    }
    math.set("PI", Value::Number(std::f64::consts::PI));
    global.set("Math", Value::Object(math));

    let key = p.new_object();
    for (name, f) in [
        ("isDown", key_is_down as Native),
        ("getCode", key_get_code),
        ("getAscii", key_get_ascii),
        ("addListener", key_add_listener),
        ("removeListener", key_remove_listener),
    ] {
        method(p, &key, name, f);
    }
    for (name, code) in
        [("BACKSPACE", 8), ("TAB", 9), ("ENTER", 13), ("SHIFT", 16), ("CONTROL", 17), ("ESCAPE", 27), ("SPACE", 32), ("LEFT", 37), ("UP", 38), ("RIGHT", 39), ("DOWN", 40)]
    {
        key.set(name, Value::Number(code as f64));
    }
    global.set("Key", Value::Object(key));

    let stage = p.new_object();
    let rect = p.movie.def.stage;
    stage.set("width", Value::Number((rect[1] - rect[0]) as f64 / 20.0));
    stage.set("height", Value::Number((rect[3] - rect[2]) as f64 / 20.0));
    method(p, &stage, "addListener", stage_add_listener);
    method(p, &stage, "removeListener", stage_remove_listener);
    global.set("Stage", Value::Object(stage));

    let mouse = p.new_object();
    for (name, f) in [
        ("addListener", mouse_add_listener as Native),
        ("removeListener", mouse_remove_listener),
        ("show", mouse_visibility),
        ("hide", mouse_visibility),
    ] {
        method(p, &mouse, name, f);
    }
    global.set("Mouse", Value::Object(mouse));

    for (name, f) in [
        ("isNaN", is_nan as Native),
        ("parseInt", parse_int),
        ("setInterval", set_interval),
        ("clearInterval", clear_interval),
    ] {
        method(p, &global, name, f);
    }
}
