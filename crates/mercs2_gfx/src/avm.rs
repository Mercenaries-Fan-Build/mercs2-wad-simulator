//! ActionScript 2: the AVM1 bytecode the movies carry.
//!
//! The opcode set is the 49 the retail movies use, the same set `GASActionBuffer::Execute`
//! (`FUN_0076ab40`) reads for them; any other opcode stops the script with an error, so a movie never
//! runs on with part of its logic missing. Identifiers are case sensitive (every movie is SWF 8).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use mercs2_formats::gfx_def::Matrix;

use crate::stage::{Child, ClipId, Item, Movie, Node};

pub type Text = Rc<str>;

#[derive(Clone)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Str(Text),
    Object(Obj),
    Clip(ClipId),
    /// A text field or other non-clip child: its parent and depth.
    Field(ClipId, i32),
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Undefined => write!(f, "undefined"),
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Object(_) => write!(f, "[object]"),
            Value::Clip(id) => write!(f, "[clip {id}]"),
            Value::Field(id, d) => write!(f, "[field {id}:{d}]"),
        }
    }
}

pub type Native = fn(&mut Player, &Value, &[Value]) -> Result<Value, String>;

/// A function written in ActionScript.
pub struct ScriptFn {
    code: Rc<Vec<u8>>,
    body: std::ops::Range<usize>,
    pool: Rc<Vec<Text>>,
    params: Vec<(u8, Text)>,
    /// `DefineFunction2` flags; `None` for a plain `DefineFunction`.
    flags: Option<u16>,
    registers: u8,
    /// The enclosing activations, innermost last.
    scope: Vec<Obj>,
    clip: ClipId,
}

pub enum Kind {
    Plain,
    Array(Vec<Value>),
    Script(Rc<ScriptFn>),
    Native(Native),
    /// Milliseconds since the epoch.
    Date(f64),
}

pub struct ObjectData {
    pub props: HashMap<Text, Value>,
    /// Keys in the order they were first set, for `for … in`.
    pub order: Vec<Text>,
    pub proto: Option<Obj>,
    pub kind: Kind,
}

#[derive(Clone)]
pub struct Obj(pub Rc<RefCell<ObjectData>>);

impl Obj {
    pub fn new(proto: Option<Obj>, kind: Kind) -> Obj {
        Obj(Rc::new(RefCell::new(ObjectData { props: HashMap::new(), order: Vec::new(), proto, kind })))
    }

    pub fn set(&self, key: &str, value: Value) {
        let mut data = self.0.borrow_mut();
        let key: Text = key.into();
        if data.props.insert(key.clone(), value).is_none() {
            data.order.push(key);
        }
    }

    fn own(&self, key: &str) -> Option<Value> {
        self.0.borrow().props.get(key).cloned()
    }

    /// `key` here or up the prototype chain.
    pub fn get(&self, key: &str) -> Option<Value> {
        let mut at = Some(self.clone());
        while let Some(o) = at {
            if let Some(v) = o.own(key) {
                return Some(v);
            }
            at = o.0.borrow().proto.clone();
        }
        None
    }

    fn delete(&self, key: &str) -> bool {
        let mut data = self.0.borrow_mut();
        let had = data.props.remove(key).is_some();
        data.order.retain(|k| &**k != key);
        had
    }

    fn same(&self, other: &Obj) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

pub fn to_number(v: &Value) -> f64 {
    match v {
        Value::Undefined | Value::Null => f64::NAN,
        Value::Bool(b) => *b as i32 as f64,
        Value::Number(n) => *n,
        Value::Str(s) => {
            let t = s.trim();
            if t.is_empty() {
                return f64::NAN;
            }
            match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                Some(hex) => i64::from_str_radix(hex, 16).map_or(f64::NAN, |n| n as f64),
                None => t.parse().unwrap_or(f64::NAN),
            }
        }
        _ => f64::NAN,
    }
}

pub fn to_bool(v: &Value) -> bool {
    match v {
        Value::Undefined | Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        Value::Str(s) => !s.is_empty(),
        _ => true,
    }
}

fn number_text(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        if n > 0.0 { "Infinity".into() } else { "-Infinity".into() }
    } else if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn int32(n: f64) -> i32 {
    if n.is_finite() {
        n.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
    } else {
        0
    }
}

/// A timer `setInterval` made.
struct Interval {
    id: u32,
    func: Value,
    this: Value,
    args: Vec<Value>,
    period: f64,
    due: f64,
}

/// One running function or frame script.
struct Activation {
    code: Rc<Vec<u8>>,
    pc: usize,
    end: usize,
    pool: Rc<Vec<Text>>,
    registers: Vec<Value>,
    /// Scope objects, outermost first; the last holds this call's locals.
    scope: Vec<Obj>,
    /// `true` when the last scope is a function's locals and not the clip's own variables.
    function: bool,
    this: Value,
    clip: ClipId,
}

const MAX_CALL_DEPTH: u32 = 256;
const MAX_ACTIONS: u64 = 20_000_000;

pub struct Player {
    pub movie: Movie,
    pub global: Obj,
    object_proto: Obj,
    function_proto: Obj,
    array_proto: Obj,
    string_proto: Obj,
    clip_proto: Obj,
    field_proto: Obj,
    date_proto: Obj,
    clip_vars: HashMap<ClipId, Obj>,
    field_vars: HashMap<(ClipId, i32), Obj>,
    intervals: Vec<Interval>,
    next_interval: u32,
    /// Milliseconds of movie time.
    pub now: f64,
    pub fscommands: Vec<(String, String)>,
    pub traces: Vec<String>,
    /// Methods scripts called that do not exist here, for the gate to report.
    pub missing: BTreeSet<String>,
    /// Keys held, by Flash key code.
    pub keys: BTreeSet<u32>,
    last_key: u32,
    last_ascii: u32,
    /// Listeners of `Key` and `Mouse`, the objects to send `onKeyDown` and the like.
    key_listeners: Vec<Value>,
    stage_listeners: Vec<Value>,
    pub mouse: [f32; 2],
    depth: u32,
    budget: u64,
    random: u64,
}

impl Player {
    pub fn new(movie: Movie) -> Result<Player, String> {
        let object_proto = Obj::new(None, Kind::Plain);
        let mk = || Obj::new(Some(object_proto.clone()), Kind::Plain);
        let function_proto = mk();
        let array_proto = mk();
        let string_proto = mk();
        let clip_proto = mk();
        let field_proto = mk();
        let date_proto = mk();
        let global = mk();
        let mut player = Player {
            movie,
            global,
            object_proto,
            function_proto,
            array_proto,
            string_proto,
            clip_proto,
            field_proto,
            date_proto,
            clip_vars: HashMap::new(),
            field_vars: HashMap::new(),
            intervals: Vec::new(),
            next_interval: 1,
            now: 0.0,
            fscommands: Vec::new(),
            traces: Vec::new(),
            missing: BTreeSet::new(),
            keys: BTreeSet::new(),
            last_key: 0,
            last_ascii: 0,
            key_listeners: Vec::new(),
            stage_listeners: Vec::new(),
            mouse: [0.0; 2],
            depth: 0,
            budget: MAX_ACTIONS,
            random: 0x2545_F491_4F6C_DD1D,
        };
        crate::builtins::install(&mut player);
        player.run_queued()?;
        Ok(player)
    }

    pub fn new_object(&self) -> Obj {
        Obj::new(Some(self.object_proto.clone()), Kind::Plain)
    }

    pub fn new_array(&self, items: Vec<Value>) -> Value {
        Value::Object(Obj::new(Some(self.array_proto.clone()), Kind::Array(items)))
    }

    pub fn native(&self, f: Native) -> Value {
        Value::Object(Obj::new(Some(self.function_proto.clone()), Kind::Native(f)))
    }

    pub fn protos(&self) -> [&Obj; 7] {
        [&self.object_proto, &self.function_proto, &self.array_proto, &self.string_proto, &self.clip_proto, &self.field_proto, &self.date_proto]
    }

    pub fn date_object(&self, ms: f64) -> Value {
        Value::Object(Obj::new(Some(self.date_proto.clone()), Kind::Date(ms)))
    }

    /// A pseudo random number below `limit`, for the `Random` opcode and `Math.random`.
    pub fn random_below(&mut self, limit: u32) -> u32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        if limit == 0 {
            0
        } else {
            (self.random % limit as u64) as u32
        }
    }

    pub fn to_text(&mut self, v: &Value) -> Result<Text, String> {
        Ok(match v {
            Value::Undefined => "undefined".into(),
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string().into(),
            Value::Number(n) => number_text(*n).into(),
            Value::Str(s) => s.clone(),
            Value::Clip(id) => self.clip_path(*id)?.into(),
            Value::Field(..) => "[object TextField]".into(),
            Value::Object(o) => {
                let array = matches!(o.0.borrow().kind, Kind::Array(_));
                if array {
                    let items = match &o.0.borrow().kind {
                        Kind::Array(items) => items.clone(),
                        _ => Vec::new(),
                    };
                    let mut parts = Vec::with_capacity(items.len());
                    for item in &items {
                        parts.push(self.to_text(item)?.to_string());
                    }
                    parts.join(",").into()
                } else if let Some(f) = o.get("toString") {
                    let r = self.call(&f, v, &[])?;
                    if matches!(r, Value::Object(_)) { "[object Object]".into() } else { self.to_text(&r)? }
                } else {
                    "[object Object]".into()
                }
            }
        })
    }

    fn clip_path(&self, clip: ClipId) -> Result<String, String> {
        match self.movie.child_of(clip)? {
            None => Ok("_level0".into()),
            Some((parent, depth)) => {
                let name = self.movie.clip(parent)?.children[&depth].name.clone().unwrap_or_default();
                Ok(format!("{}.{name}", self.clip_path(parent)?))
            }
        }
    }

    pub fn vars(&mut self, clip: ClipId) -> Obj {
        if let Some(o) = self.clip_vars.get(&clip) {
            return o.clone();
        }
        let o = Obj::new(Some(self.clip_proto.clone()), Kind::Plain);
        self.clip_vars.insert(clip, o.clone());
        o
    }

    /// The variables a script keeps on a text field.
    pub fn field_vars_of(&mut self, parent: ClipId, depth: i32) -> Obj {
        if let Some(o) = self.field_vars.get(&(parent, depth)) {
            return o.clone();
        }
        let o = Obj::new(Some(self.field_proto.clone()), Kind::Plain);
        self.field_vars.insert((parent, depth), o.clone());
        o
    }

    pub fn field(&self, parent: ClipId, depth: i32) -> Result<&Child, String> {
        self.movie.clip(parent)?.children.get(&depth).ok_or_else(|| format!("no object at depth {depth}"))
    }

    fn field_mut(&mut self, parent: ClipId, depth: i32) -> Result<&mut Child, String> {
        self.movie.clip_mut(parent)?.children.get_mut(&depth).ok_or_else(|| format!("no object at depth {depth}"))
    }

    /// The child object of the clip that holds `target` — its transform lives in the parent's list.
    fn child_slot(&self, target: &Value) -> Result<Option<(ClipId, i32)>, String> {
        Ok(match target {
            Value::Clip(id) => self.movie.child_of(*id)?,
            Value::Field(p, d) => Some((*p, *d)),
            _ => None,
        })
    }

    fn display_get(&mut self, target: &Value, name: &str) -> Result<Option<Value>, String> {
        let slot = self.child_slot(target)?;
        let child = match slot {
            Some((p, d)) => Some(self.field(p, d)?.clone()),
            None => None,
        };
        let matrix = child.as_ref().map_or(Matrix::IDENTITY, |c| c.matrix);
        let scale = |a: f32, b: f32| ((a * a + b * b).sqrt() * 100.0) as f64;
        Ok(Some(match name {
            "_x" => Value::Number(matrix.tx as f64 / 20.0),
            "_y" => Value::Number(matrix.ty as f64 / 20.0),
            "_xscale" => Value::Number(scale(matrix.a, matrix.b)),
            "_yscale" => Value::Number(scale(matrix.c, matrix.d)),
            "_rotation" => Value::Number((matrix.b as f64).atan2(matrix.a as f64).to_degrees()),
            "_alpha" => Value::Number(child.as_ref().map_or(100.0, |c| c.cxform.mul[3] as f64 * 100.0)),
            "_visible" => Value::Bool(child.as_ref().is_none_or(|c| c.visible)),
            "_name" => Value::Str(child.and_then(|c| c.name).unwrap_or_default().into()),
            "_width" | "_height" => {
                let bounds = self.bounds(target)?;
                let wide = name == "_width";
                let [xmin, xmax, ymin, ymax] = bounds;
                let (lo, hi) = if wide { (xmin, xmax) } else { (ymin, ymax) };
                Value::Number(((hi - lo).max(0.0)) as f64 / 20.0)
            }
            "_parent" => match slot {
                Some((p, _)) => Value::Clip(p),
                None => Value::Undefined,
            },
            "_root" => Value::Clip(self.movie.root),
            "_global" => Value::Object(self.global.clone()),
            "_level0" => Value::Clip(self.movie.root),
            "_xmouse" | "_ymouse" => {
                let at = self.global_to_local(target)?.apply(self.mouse[0] * 20.0, self.mouse[1] * 20.0);
                Value::Number(at[(name == "_ymouse") as usize] as f64 / 20.0)
            }
            "_currentframe" | "_totalframes" | "_framesloaded" => {
                let Value::Clip(id) = target else { return Ok(None) };
                match name {
                    "_currentframe" => Value::Number(self.movie.clip(*id)?.frame as f64 + 1.0),
                    _ => Value::Number(self.movie.frame_count(*id)? as f64),
                }
            }
            "_target" => {
                let Value::Clip(id) = target else { return Ok(None) };
                let path = self.clip_path(*id)?;
                Value::Str(path.trim_start_matches("_level0").replace('.', "/").into())
            }
            _ => return Ok(None),
        }))
    }

    fn set_display(&mut self, target: &Value, name: &str, value: &Value) -> Result<bool, String> {
        let number = to_number(value);
        if !matches!(name, "_x" | "_y" | "_xscale" | "_yscale" | "_rotation" | "_alpha" | "_visible" | "_width" | "_height" | "_name") {
            return Ok(false);
        }
        let Some((parent, depth)) = self.child_slot(target)? else { return Ok(true) };
        if name == "_width" || name == "_height" {
            let [xmin, xmax, ymin, ymax] = self.bounds(target)?;
            let (span, wide) = if name == "_width" { (xmax - xmin, true) } else { (ymax - ymin, false) };
            let child = self.field_mut(parent, depth)?;
            let m = &mut child.matrix;
            if span > 0.0 {
                let k = number as f32 * 20.0 / span;
                if wide {
                    m.a *= k;
                    m.b *= k;
                } else {
                    m.c *= k;
                    m.d *= k;
                }
            }
            child.scripted = true;
            return Ok(true);
        }
        if name == "_name" {
            let text = self.to_text(value)?.to_string();
            self.field_mut(parent, depth)?.name = Some(text);
            return Ok(true);
        }
        let child = self.field_mut(parent, depth)?;
        let m = &mut child.matrix;
        match name {
            "_x" => m.tx = number as f32 * 20.0,
            "_y" => m.ty = number as f32 * 20.0,
            "_xscale" | "_yscale" => {
                let (a, b) = if name == "_xscale" { (m.a, m.b) } else { (m.c, m.d) };
                let now = (a * a + b * b).sqrt();
                let want = number as f32 / 100.0;
                let (x, y) = if now == 0.0 { (want, 0.0) } else { (a / now * want, b / now * want) };
                if name == "_xscale" {
                    m.a = x;
                    m.b = y;
                } else {
                    // A zero scale keeps its axis: `c` is the skew term, `d` the scale.
                    if now == 0.0 {
                        m.d = want;
                    } else {
                        m.c = x;
                        m.d = y;
                    }
                }
            }
            "_rotation" => {
                let sx = (m.a * m.a + m.b * m.b).sqrt();
                let sy = (m.c * m.c + m.d * m.d).sqrt();
                let (s, c) = (number as f32).to_radians().sin_cos();
                m.a = c * sx;
                m.b = s * sx;
                m.c = -s * sy;
                m.d = c * sy;
            }
            "_alpha" => child.cxform.mul[3] = number as f32 / 100.0,
            "_visible" => child.visible = to_bool(value),
            _ => {}
        }
        child.scripted = true;
        Ok(true)
    }

    /// Bounds of an object in its parent's space, twips, `[xmin, xmax, ymin, ymax]`.
    pub fn bounds(&mut self, target: &Value) -> Result<[f32; 4], String> {
        let (matrix, items) = match target {
            Value::Clip(id) => {
                let slot = self.movie.child_of(*id)?;
                let matrix = match slot {
                    Some((p, d)) => self.field(p, d)?.matrix,
                    None => Matrix::IDENTITY,
                };
                (matrix, self.movie.draw_of(*id)?)
            }
            Value::Field(p, d) => {
                let child = self.field(*p, *d)?.clone();
                match &child.node {
                    Node::Text(_) => {
                        let bounds = match self.movie.def.characters.get(&child.character) {
                            Some(mercs2_formats::gfx_def::Character::EditText(t)) => t.bounds,
                            _ => [0; 4],
                        };
                        let m = child.matrix;
                        let corners = [(bounds[0], bounds[2]), (bounds[1], bounds[3])].map(|(x, y)| m.apply(x as f32, y as f32));
                        return Ok([corners[0][0], corners[1][0], corners[0][1], corners[1][1]]);
                    }
                    _ => (child.matrix, Vec::new()),
                }
            }
            _ => return Ok([0.0; 4]),
        };
        let mut out = [f32::MAX, f32::MIN, f32::MAX, f32::MIN];
        for item in &items {
            if let Item::Draw(d) = item {
                for part in d.parts.iter() {
                    for [x, y] in &part.vertices {
                        let [px, py] = matrix.then(&d.matrix).apply(*x, *y);
                        out[0] = out[0].min(px);
                        out[1] = out[1].max(px);
                        out[2] = out[2].min(py);
                        out[3] = out[3].max(py);
                    }
                }
            }
        }
        if out[0] > out[1] {
            let (x, y) = (matrix.tx, matrix.ty);
            return Ok([x, x, y, y]);
        }
        Ok(out)
    }

    fn global_matrix(&self, target: &Value) -> Result<Matrix, String> {
        let mut out = Matrix::IDENTITY;
        let mut at = target.clone();
        while let Some((parent, depth)) = self.child_slot(&at)? {
            out = self.field(parent, depth)?.matrix.then(&out);
            at = Value::Clip(parent);
        }
        Ok(out)
    }

    fn global_to_local(&self, target: &Value) -> Result<Matrix, String> {
        self.global_matrix(target)?.inverse().ok_or_else(|| "a clip with a singular matrix".to_string())
    }

    /// A stage point in `target`'s space, in pixels.
    pub fn stage_to_local(&self, target: &Value, x: f32, y: f32) -> Result<[f32; 2], String> {
        let [lx, ly] = self.global_to_local(target)?.apply(x * 20.0, y * 20.0);
        Ok([lx / 20.0, ly / 20.0])
    }

    /// `target.name`, in the order Flash looks: display properties, named children, variables.
    pub fn get_member(&mut self, target: &Value, name: &str) -> Result<Value, String> {
        match target {
            Value::Clip(id) => {
                if let Some(v) = self.display_get(target, name)? {
                    return Ok(v);
                }
                if let Some(v) = self.vars(*id).own(name) {
                    return Ok(v);
                }
                if let Some((depth, child)) = self.movie.find_child(*id, name)? {
                    return Ok(match child.node {
                        Node::Clip(c) => Value::Clip(c),
                        _ => Value::Field(*id, depth),
                    });
                }
                Ok(self.vars(*id).get(name).unwrap_or(Value::Undefined))
            }
            Value::Field(p, d) => {
                if let Some(v) = self.display_get(target, name)? {
                    return Ok(v);
                }
                crate::builtins::field_get(self, *p, *d, name)
            }
            Value::Object(o) => {
                if name == "length" {
                    if let Kind::Array(items) = &o.0.borrow().kind {
                        return Ok(Value::Number(items.len() as f64));
                    }
                }
                if let Kind::Array(items) = &o.0.borrow().kind {
                    if let Ok(i) = name.parse::<usize>() {
                        return Ok(items.get(i).cloned().unwrap_or(Value::Undefined));
                    }
                }
                if name == "__proto__" {
                    return Ok(o.0.borrow().proto.clone().map_or(Value::Undefined, Value::Object));
                }
                Ok(o.get(name).unwrap_or(Value::Undefined))
            }
            Value::Str(s) => {
                if name == "length" {
                    return Ok(Value::Number(s.encode_utf16().count() as f64));
                }
                Ok(self.string_proto.get(name).unwrap_or(Value::Undefined))
            }
            Value::Number(_) | Value::Bool(_) => Ok(self.object_proto.get(name).unwrap_or(Value::Undefined)),
            Value::Undefined | Value::Null => Ok(Value::Undefined),
        }
    }

    pub fn set_member(&mut self, target: &Value, name: &str, value: Value) -> Result<(), String> {
        match target {
            Value::Clip(id) => {
                if !self.set_display(target, name, &value)? {
                    self.vars(*id).set(name, value);
                }
            }
            Value::Field(p, d) => {
                if !self.set_display(target, name, &value)? {
                    crate::builtins::field_set(self, *p, *d, name, value)?;
                }
            }
            Value::Object(o) => {
                if let Kind::Array(items) = &mut o.0.borrow_mut().kind {
                    if name == "length" {
                        items.resize(to_number(&value).max(0.0) as usize, Value::Undefined);
                        return Ok(());
                    }
                    if let Ok(i) = name.parse::<usize>() {
                        if i >= items.len() {
                            items.resize(i + 1, Value::Undefined);
                        }
                        items[i] = value;
                        return Ok(());
                    }
                }
                o.set(name, value);
            }
            // A property set on a primitive goes nowhere in Flash.
            _ => {}
        }
        Ok(())
    }

    /// Resolve a `/a/b:c` or `a.b.c` path to the value it names.
    fn resolve_path(&mut self, frame: &Activation, path: &str) -> Result<Value, String> {
        if let Some(rest) = path.strip_prefix('/') {
            let mut at = Value::Clip(self.movie.root);
            for part in rest.split(['/', ':', '.']).filter(|p| !p.is_empty()) {
                at = self.get_member(&at, part)?;
            }
            return Ok(at);
        }
        let mut parts = path.split(['.', ':', '/']);
        let first = parts.next().unwrap_or("");
        let mut at = self.lookup(frame, first)?;
        for part in parts {
            if part.is_empty() {
                continue;
            }
            at = if part == ".." {
                self.get_member(&at, "_parent")?
            } else {
                self.get_member(&at, part)?
            };
        }
        Ok(at)
    }

    /// A bare identifier through the scope chain: locals, the clip, then globals.
    fn lookup(&mut self, frame: &Activation, name: &str) -> Result<Value, String> {
        match name {
            "this" => return Ok(frame.this.clone()),
            "_global" => return Ok(Value::Object(self.global.clone())),
            "_root" | "_level0" => return Ok(Value::Clip(self.movie.root)),
            _ => {}
        }
        for scope in frame.scope.iter().rev() {
            if let Some(v) = scope.own(name) {
                return Ok(v);
            }
        }
        let clip = Value::Clip(frame.clip);
        if self.movie.exists(frame.clip) {
            let v = self.get_member(&clip, name)?;
            if !matches!(v, Value::Undefined) {
                return Ok(v);
            }
        }
        Ok(self.global.get(name).unwrap_or(Value::Undefined))
    }

    /// Set a bare identifier where it already lives, else on the clip.
    fn assign(&mut self, frame: &Activation, name: &str, value: Value) -> Result<(), String> {
        for scope in frame.scope.iter().rev() {
            if scope.own(name).is_some() {
                scope.set(name, value);
                return Ok(());
            }
        }
        self.set_member(&Value::Clip(frame.clip), name, value)
    }

    /// Set a path: everything before the last separator names the object.
    fn assign_path(&mut self, frame: &Activation, path: &str, value: Value) -> Result<(), String> {
        match path.rfind(['.', ':', '/']) {
            Some(at) if at > 0 || path.starts_with('/') => {
                let owner = if at == 0 { Value::Clip(self.movie.root) } else { self.resolve_path(frame, &path[..at])? };
                self.set_member(&owner, &path[at + 1..], value)
            }
            _ => self.assign(frame, path, value),
        }
    }

    pub fn call(&mut self, func: &Value, this: &Value, args: &[Value]) -> Result<Value, String> {
        let Value::Object(o) = func else { return Ok(Value::Undefined) };
        let script = match &o.0.borrow().kind {
            Kind::Native(f) => Err(*f),
            Kind::Script(s) => Ok(s.clone()),
            _ => return Ok(Value::Undefined),
        };
        match script {
            Err(native) => native(self, this, args),
            Ok(script) => self.call_script(o, &script, this, args),
        }
    }

    fn call_script(&mut self, func: &Obj, script: &Rc<ScriptFn>, this: &Value, args: &[Value]) -> Result<Value, String> {
        if self.depth >= MAX_CALL_DEPTH {
            return Err("script recursion too deep".into());
        }
        let locals = self.new_object();
        let mut scope = script.scope.clone();
        scope.push(locals.clone());
        let this = if matches!(this, Value::Undefined) { Value::Clip(script.clip) } else { this.clone() };
        let mut frame = Activation {
            code: script.code.clone(),
            pc: script.body.start,
            end: script.body.end,
            pool: script.pool.clone(),
            registers: vec![Value::Undefined; script.registers.max(4) as usize],
            scope,
            function: true,
            this: this.clone(),
            clip: script.clip,
        };
        let arguments = self.new_array(args.to_vec());
        if let Value::Object(a) = &arguments {
            a.set("callee", Value::Object(func.clone()));
        }
        match script.flags {
            None => {
                locals.set("arguments", arguments);
                for (i, (_, name)) in script.params.iter().enumerate() {
                    locals.set(name, args.get(i).cloned().unwrap_or(Value::Undefined));
                }
            }
            Some(flags) => {
                let mut r = 1;
                let parent = self.get_member(&Value::Clip(script.clip), "_parent").unwrap_or(Value::Undefined);
                let root = Value::Clip(self.movie.root);
                let global = Value::Object(self.global.clone());
                // The register-preload order of `DefineFunction2`.
                let preload = [
                    (0x0001u16, this.clone()),
                    (0x0004, arguments.clone()),
                    (0x0010, Value::Undefined),
                    (0x0040, root),
                    (0x0080, parent),
                    (0x0100, global),
                ];
                for (bit, value) in preload {
                    if flags & bit != 0 {
                        frame.registers[r] = value;
                        r += 1;
                    }
                }
                if flags & 0x0002 == 0 {
                    locals.set("this", this.clone());
                }
                if flags & 0x0008 == 0 {
                    locals.set("arguments", arguments);
                }
                for (i, (reg, name)) in script.params.iter().enumerate() {
                    let value = args.get(i).cloned().unwrap_or(Value::Undefined);
                    if *reg == 0 {
                        locals.set(name, value);
                    } else {
                        frame.registers[*reg as usize] = value;
                    }
                }
            }
        }
        self.depth += 1;
        let result = self.run(&mut frame);
        self.depth -= 1;
        result
    }

    /// Run a frame's bytecode with `clip` as its target.
    pub fn run_action(&mut self, clip: ClipId, code: Rc<Vec<u8>>) -> Result<(), String> {
        let end = code.len();
        let mut frame = Activation {
            code,
            pc: 0,
            end,
            pool: Rc::new(Vec::new()),
            registers: vec![Value::Undefined; 4],
            scope: Vec::new(),
            function: false,
            this: Value::Clip(clip),
            clip,
        };
        self.run(&mut frame)?;
        Ok(())
    }

    /// Run the scripts the timelines queued, oldest first.
    pub fn run_queued(&mut self) -> Result<(), String> {
        while let Some(action) = self.movie.actions.pop_front() {
            if self.movie.exists(action.clip) {
                self.budget = MAX_ACTIONS;
                self.run_action(action.clip, Rc::new(action.code))?;
            }
        }
        Ok(())
    }

    fn pop(stack: &mut Vec<Value>) -> Value {
        stack.pop().unwrap_or(Value::Undefined)
    }

    /// The zero-based frame a goto target names, on `clip`.
    pub fn frame_of(&mut self, clip: ClipId, target: &Value) -> Result<Option<u16>, String> {
        Ok(match target {
            Value::Number(n) => Some((n.max(1.0) as u16).saturating_sub(1)),
            other => {
                let text = self.to_text(other)?;
                match text.parse::<f64>() {
                    Ok(n) => Some((n.max(1.0) as u16).saturating_sub(1)),
                    Err(_) => self.movie.label(clip, &text)?,
                }
            }
        })
    }

    fn call_method(&mut self, target: &Value, name: &str, args: &[Value]) -> Result<Value, String> {
        let func = self.get_member(target, name)?;
        if !matches!(func, Value::Object(_)) {
            let on = match target {
                Value::Clip(_) => "MovieClip",
                Value::Field(..) => "TextField",
                Value::Str(_) => "String",
                Value::Object(_) => "Object",
                _ => "undefined",
            };
            if on != "undefined" {
                self.missing.insert(format!("{on}.{name}"));
            }
            return Ok(Value::Undefined);
        }
        self.call(&func, target, args)
    }

    /// Make a function from its header; `at` is where its body starts. Returns it and the body's end.
    fn define_function(&mut self, frame: &Activation, body: &[u8], two: bool, at: usize) -> Result<(Value, usize), String> {
        let mut p = 0;
        let string = |p: &mut usize| -> Result<Text, String> {
            let rest = body.get(*p..).ok_or("eof in a function header")?;
            let len = rest.iter().position(|b| *b == 0).ok_or("unterminated function name")?;
            let out: Text = String::from_utf8_lossy(&rest[..len]).as_ref().into();
            *p += len + 1;
            Ok(out)
        };
        let word = |p: &mut usize| -> Result<u16, String> {
            let b = body.get(*p..*p + 2).ok_or("eof in a function header")?;
            *p += 2;
            Ok(u16::from_le_bytes([b[0], b[1]]))
        };
        let name = string(&mut p)?;
        let count = word(&mut p)?;
        let (mut registers, mut flags) = (4u8, None);
        if two {
            registers = *body.get(p).ok_or("eof in a function header")?;
            p += 1;
            flags = Some(word(&mut p)?);
        }
        let mut params = Vec::new();
        for _ in 0..count {
            let reg = if two {
                p += 1;
                body[p - 1]
            } else {
                0
            };
            params.push((reg, string(&mut p)?));
        }
        let size = word(&mut p)? as usize;
        let script = ScriptFn {
            code: frame.code.clone(),
            body: at..at + size,
            pool: frame.pool.clone(),
            params,
            flags,
            registers: registers.max(1),
            scope: frame.scope.clone(),
            clip: frame.clip,
        };
        let func = Obj::new(Some(self.function_proto.clone()), Kind::Script(Rc::new(script)));
        let proto = self.new_object();
        proto.set("constructor", Value::Object(func.clone()));
        func.set("prototype", Value::Object(proto));
        let value = Value::Object(func);
        if !name.is_empty() {
            if frame.function {
                frame.scope.last().expect("a function has locals").set(&name, value.clone());
            } else {
                self.set_member(&Value::Clip(frame.clip), &name, value.clone())?;
            }
        }
        Ok((value, at + size))
    }

    fn push_values(&mut self, frame: &Activation, body: &[u8], stack: &mut Vec<Value>) -> Result<(), String> {
        let mut p = 0;
        while p < body.len() {
            let kind = body[p];
            p += 1;
            let bytes = |n: usize, p: &mut usize| -> Result<&[u8], String> {
                let out = body.get(*p..*p + n).ok_or("eof in a Push")?;
                *p += n;
                Ok(out)
            };
            stack.push(match kind {
                0 => {
                    let rest = &body[p..];
                    let len = rest.iter().position(|b| *b == 0).ok_or("unterminated Push string")?;
                    let s: Text = String::from_utf8_lossy(&rest[..len]).as_ref().into();
                    p += len + 1;
                    Value::Str(s)
                }
                1 => {
                    let b = bytes(4, &mut p)?;
                    Value::Number(f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64)
                }
                2 => Value::Null,
                3 => Value::Undefined,
                4 => {
                    let r = bytes(1, &mut p)?[0] as usize;
                    frame.registers.get(r).cloned().unwrap_or(Value::Undefined)
                }
                5 => Value::Bool(bytes(1, &mut p)?[0] != 0),
                6 => {
                    // Doubles are stored with their two words swapped.
                    let b = bytes(8, &mut p)?;
                    Value::Number(f64::from_le_bytes([b[4], b[5], b[6], b[7], b[0], b[1], b[2], b[3]]))
                }
                7 => {
                    let b = bytes(4, &mut p)?;
                    Value::Number(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64)
                }
                8 => {
                    let i = bytes(1, &mut p)?[0] as usize;
                    Value::Str(frame.pool.get(i).cloned().ok_or("constant pool index out of range")?)
                }
                9 => {
                    let b = bytes(2, &mut p)?;
                    let i = u16::from_le_bytes([b[0], b[1]]) as usize;
                    Value::Str(frame.pool.get(i).cloned().ok_or("constant pool index out of range")?)
                }
                other => return Err(format!("Push type {other}")),
            });
        }
        Ok(())
    }

    fn equals(&mut self, a: &Value, b: &Value) -> Result<bool, String> {
        Ok(match (a, b) {
            (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
            (Value::Number(x), Value::Number(y)) => x == y,
            (Value::Str(x), Value::Str(y)) => x == y,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Object(x), Value::Object(y)) => x.same(y),
            (Value::Clip(x), Value::Clip(y)) => x == y,
            (Value::Field(x, c), Value::Field(y, d)) => x == y && c == d,
            (Value::Number(_) | Value::Str(_) | Value::Bool(_), Value::Number(_) | Value::Str(_) | Value::Bool(_)) => to_number(a) == to_number(b),
            _ => false,
        })
    }

    fn strict_equals(&mut self, a: &Value, b: &Value) -> Result<bool, String> {
        Ok(match (a, b) {
            (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
            _ if std::mem::discriminant(a) != std::mem::discriminant(b) => false,
            _ => self.equals(a, b)?,
        })
    }

    fn less(&mut self, a: &Value, b: &Value) -> Result<Value, String> {
        if let (Value::Str(x), Value::Str(y)) = (a, b) {
            return Ok(Value::Bool(x < y));
        }
        let (x, y) = (to_number(a), to_number(b));
        Ok(if x.is_nan() || y.is_nan() { Value::Undefined } else { Value::Bool(x < y) })
    }

    fn fscommand(&mut self, url: &str, target: &str) -> bool {
        match url.strip_prefix("FSCommand:") {
            Some(command) => {
                self.fscommands.push((command.to_string(), target.to_string()));
                true
            }
            None => false,
        }
    }

    fn type_of(v: &Value) -> &'static str {
        match v {
            Value::Undefined => "undefined",
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Clip(_) => "movieclip",
            Value::Field(..) => "object",
            Value::Object(o) => match o.0.borrow().kind {
                Kind::Script(_) | Kind::Native(_) => "function",
                _ => "object",
            },
        }
    }

    fn construct(&mut self, func: &Value, args: &[Value]) -> Result<Value, String> {
        let Value::Object(f) = func else { return Ok(Value::Undefined) };
        let proto = match f.get("prototype") {
            Some(Value::Object(p)) => p,
            _ => self.object_proto.clone(),
        };
        let this = Value::Object(Obj::new(Some(proto), Kind::Plain));
        let made = self.call(func, &this, args)?;
        // A native constructor makes its own object.
        Ok(if matches!(made, Value::Object(_)) { made } else { this })
    }

    fn run(&mut self, frame: &mut Activation) -> Result<Value, String> {
        let code = frame.code.clone();
        let mut stack: Vec<Value> = Vec::new();
        while frame.pc < frame.end {
            self.budget = self.budget.checked_sub(1).ok_or("a script ran past the action budget")?;
            let at = frame.pc;
            let op = code[at];
            frame.pc += 1;
            if op == 0 {
                break;
            }
            let body: &[u8] = if op >= 0x80 {
                let len = u16::from_le_bytes([code[frame.pc], code[frame.pc + 1]]) as usize;
                frame.pc += 2 + len;
                code.get(frame.pc - len..frame.pc).ok_or("an action overruns its block")?
            } else {
                &[]
            };
            match op {
                0x06 => self.movie.clip_mut(frame.clip)?.playing = true,
                0x07 => self.movie.clip_mut(frame.clip)?.playing = false,
                0x0B => {
                    let b = to_number(&Self::pop(&mut stack));
                    let a = to_number(&Self::pop(&mut stack));
                    stack.push(Value::Number(a - b));
                }
                0x0C => {
                    let b = to_number(&Self::pop(&mut stack));
                    let a = to_number(&Self::pop(&mut stack));
                    stack.push(Value::Number(a * b));
                }
                0x0D => {
                    let b = to_number(&Self::pop(&mut stack));
                    let a = to_number(&Self::pop(&mut stack));
                    stack.push(Value::Number(a / b));
                }
                0x12 => {
                    let v = Self::pop(&mut stack);
                    stack.push(Value::Bool(!to_bool(&v)));
                }
                0x17 => {
                    Self::pop(&mut stack);
                }
                0x1C => {
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    let v = if name.contains(['.', ':', '/']) { self.resolve_path(frame, &name)? } else { self.lookup(frame, &name)? };
                    stack.push(v);
                }
                0x1D => {
                    let value = Self::pop(&mut stack);
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    if name.contains(['.', ':', '/']) {
                        self.assign_path(frame, &name, value)?;
                    } else {
                        self.assign(frame, &name, value)?;
                    }
                }
                0x22 => {
                    let index = to_number(&Self::pop(&mut stack)) as usize;
                    let target = Self::pop(&mut stack);
                    let target = self.path_target(frame, &target)?;
                    const NAMES: [&str; 22] = [
                        "_x", "_y", "_xscale", "_yscale", "_currentframe", "_totalframes", "_alpha", "_visible", "_width", "_height",
                        "_rotation", "_target", "_framesloaded", "_name", "_droptarget", "_url", "_highquality", "_focusrect",
                        "_soundbuftime", "_quality", "_xmouse", "_ymouse",
                    ];
                    let name = NAMES.get(index).ok_or_else(|| format!("GetProperty index {index}"))?;
                    stack.push(self.get_member(&target, name)?);
                }
                0x26 => {
                    let v = Self::pop(&mut stack);
                    let text = self.to_text(&v)?;
                    self.traces.push(text.to_string());
                }
                0x30 => {
                    let limit = to_number(&Self::pop(&mut stack));
                    let n = self.random_below(limit.max(0.0) as u32);
                    stack.push(Value::Number(n as f64));
                }
                0x3A => {
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    let target = Self::pop(&mut stack);
                    let removed = match &target {
                        Value::Object(o) => o.delete(&name),
                        Value::Clip(id) => self.vars(*id).delete(&name),
                        _ => false,
                    };
                    stack.push(Value::Bool(removed));
                }
                0x3C => {
                    let value = Self::pop(&mut stack);
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    if frame.function {
                        frame.scope.last().expect("a function has locals").set(&name, value);
                    } else {
                        self.set_member(&Value::Clip(frame.clip), &name, value)?;
                    }
                }
                0x3D => {
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    let count = to_number(&Self::pop(&mut stack)) as usize;
                    let args: Vec<Value> = (0..count).map(|_| Self::pop(&mut stack)).collect();
                    let func = if name.contains(['.', ':', '/']) { self.resolve_path(frame, &name)? } else { self.lookup(frame, &name)? };
                    if !matches!(func, Value::Object(_)) {
                        self.missing.insert(format!("function {name}"));
                    }
                    let this = Value::Clip(frame.clip);
                    stack.push(self.call(&func, &this, &args)?);
                }
                0x3E => return Ok(Self::pop(&mut stack)),
                0x3F => {
                    let b = to_number(&Self::pop(&mut stack));
                    let a = to_number(&Self::pop(&mut stack));
                    stack.push(Value::Number(a % b));
                }
                0x40 => {
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    let count = to_number(&Self::pop(&mut stack)) as usize;
                    let args: Vec<Value> = (0..count).map(|_| Self::pop(&mut stack)).collect();
                    let func = self.lookup(frame, &name)?;
                    if !matches!(func, Value::Object(_)) {
                        self.missing.insert(format!("class {name}"));
                    }
                    stack.push(self.construct(&func, &args)?);
                }
                0x41 => {
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    if frame.function {
                        let locals = frame.scope.last().expect("a function has locals");
                        if locals.own(&name).is_none() {
                            locals.set(&name, Value::Undefined);
                        }
                    }
                }
                0x42 => {
                    let count = to_number(&Self::pop(&mut stack)) as usize;
                    let items: Vec<Value> = (0..count).map(|_| Self::pop(&mut stack)).collect();
                    let array = self.new_array(items);
                    stack.push(array);
                }
                0x43 => {
                    let count = to_number(&Self::pop(&mut stack)) as usize;
                    let object = self.new_object();
                    for _ in 0..count {
                        let value = Self::pop(&mut stack);
                        let name = Self::pop(&mut stack);
                        object.set(&self.to_text(&name)?, value);
                    }
                    stack.push(Value::Object(object));
                }
                0x44 => {
                    let v = Self::pop(&mut stack);
                    stack.push(Value::Str(Self::type_of(&v).into()));
                }
                0x47 => {
                    let b = Self::pop(&mut stack);
                    let a = Self::pop(&mut stack);
                    if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
                        let mut text = self.to_text(&a)?.to_string();
                        text.push_str(&self.to_text(&b)?);
                        stack.push(Value::Str(text.into()));
                    } else {
                        stack.push(Value::Number(to_number(&a) + to_number(&b)));
                    }
                }
                0x48 => {
                    let b = Self::pop(&mut stack);
                    let a = Self::pop(&mut stack);
                    stack.push(self.less(&a, &b)?);
                }
                0x49 => {
                    let b = Self::pop(&mut stack);
                    let a = Self::pop(&mut stack);
                    stack.push(Value::Bool(self.equals(&a, &b)?));
                }
                0x4A => {
                    let v = Self::pop(&mut stack);
                    stack.push(Value::Number(to_number(&v)));
                }
                0x4B => {
                    let v = Self::pop(&mut stack);
                    stack.push(Value::Str(self.to_text(&v)?));
                }
                0x4C => {
                    let v = stack.last().cloned().unwrap_or(Value::Undefined);
                    stack.push(v);
                }
                0x4E => {
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    let target = Self::pop(&mut stack);
                    stack.push(self.get_member(&target, &name)?);
                }
                0x4F => {
                    let value = Self::pop(&mut stack);
                    let name = Self::pop(&mut stack);
                    let name = self.to_text(&name)?;
                    let target = Self::pop(&mut stack);
                    self.set_member(&target, &name, value)?;
                }
                0x50 => {
                    let n = to_number(&Self::pop(&mut stack));
                    stack.push(Value::Number(n + 1.0));
                }
                0x51 => {
                    let n = to_number(&Self::pop(&mut stack));
                    stack.push(Value::Number(n - 1.0));
                }
                0x52 => {
                    let name = Self::pop(&mut stack);
                    let target = Self::pop(&mut stack);
                    let count = to_number(&Self::pop(&mut stack)) as usize;
                    let args: Vec<Value> = (0..count).map(|_| Self::pop(&mut stack)).collect();
                    let result = match &name {
                        Value::Undefined => self.call(&target, &Value::Undefined, &args)?,
                        Value::Str(s) if s.is_empty() => self.call(&target, &Value::Undefined, &args)?,
                        _ => {
                            let name = self.to_text(&name)?;
                            self.call_method(&target, &name, &args)?
                        }
                    };
                    stack.push(result);
                }
                0x55 => {
                    let target = Self::pop(&mut stack);
                    stack.push(Value::Undefined);
                    let keys: Vec<Text> = match &target {
                        Value::Object(o) => {
                            let data = o.0.borrow();
                            match &data.kind {
                                Kind::Array(items) => (0..items.len()).map(|i| i.to_string().as_str().into()).collect(),
                                _ => data.order.clone(),
                            }
                        }
                        Value::Clip(id) => self.vars(*id).0.borrow().order.clone(),
                        _ => Vec::new(),
                    };
                    for key in keys.into_iter().rev() {
                        stack.push(Value::Str(key));
                    }
                }
                0x61 => {
                    let b = int32(to_number(&Self::pop(&mut stack)));
                    let a = int32(to_number(&Self::pop(&mut stack)));
                    stack.push(Value::Number((a | b) as f64));
                }
                0x66 => {
                    let b = Self::pop(&mut stack);
                    let a = Self::pop(&mut stack);
                    stack.push(Value::Bool(self.strict_equals(&a, &b)?));
                }
                0x67 => {
                    let b = Self::pop(&mut stack);
                    let a = Self::pop(&mut stack);
                    stack.push(self.less(&b, &a)?);
                }
                0x81 => {
                    let frame_no = u16::from_le_bytes([body[0], body[1]]);
                    self.movie.goto(frame.clip, frame_no)?;
                    self.movie.clip_mut(frame.clip)?.playing = false;
                }
                0x83 => {
                    let mut parts = body.split(|b| *b == 0);
                    let url = String::from_utf8_lossy(parts.next().unwrap_or(&[])).into_owned();
                    let target = String::from_utf8_lossy(parts.next().unwrap_or(&[])).into_owned();
                    if !self.fscommand(&url, &target) {
                        return Err(format!("GetURL {url} {target}"));
                    }
                }
                0x87 => {
                    let r = body[0] as usize;
                    let v = stack.last().cloned().unwrap_or(Value::Undefined);
                    *frame.registers.get_mut(r).ok_or("StoreRegister out of range")? = v;
                }
                0x88 => {
                    let n = u16::from_le_bytes([body[0], body[1]]) as usize;
                    let mut pool = Vec::with_capacity(n);
                    let mut rest = &body[2..];
                    for _ in 0..n {
                        let len = rest.iter().position(|b| *b == 0).ok_or("unterminated pool string")?;
                        pool.push(String::from_utf8_lossy(&rest[..len]).as_ref().into());
                        rest = &rest[len + 1..];
                    }
                    frame.pool = Rc::new(pool);
                }
                0x8C => {
                    let len = body.iter().position(|b| *b == 0).unwrap_or(body.len());
                    let label = String::from_utf8_lossy(&body[..len]).into_owned();
                    if let Some(target) = self.movie.label(frame.clip, &label)? {
                        self.movie.goto(frame.clip, target)?;
                        self.movie.clip_mut(frame.clip)?.playing = false;
                    }
                }
                0x8E | 0x9B => {
                    let (func, end) = self.define_function(frame, body, op == 0x8E, frame.pc)?;
                    frame.pc = end;
                    let named = body.first().is_some_and(|b| *b != 0);
                    if !named {
                        stack.push(func);
                    }
                }
                0x96 => self.push_values(frame, body, &mut stack)?,
                0x99 => {
                    let offset = i16::from_le_bytes([body[0], body[1]]) as isize;
                    frame.pc = (frame.pc as isize + offset) as usize;
                }
                0x9A => {
                    let target = Self::pop(&mut stack);
                    let url = Self::pop(&mut stack);
                    let (url, target) = (self.to_text(&url)?, self.to_text(&target)?);
                    if !self.fscommand(&url, &target) {
                        return Err(format!("GetURL2 {url} {target}"));
                    }
                }
                0x9D => {
                    let offset = i16::from_le_bytes([body[0], body[1]]) as isize;
                    if to_bool(&Self::pop(&mut stack)) {
                        frame.pc = (frame.pc as isize + offset) as usize;
                    }
                }
                other => return Err(format!("opcode 0x{other:02X} at {at}")),
            }
        }
        Ok(Value::Undefined)
    }

    /// What a `GetProperty` target string or value names.
    fn path_target(&mut self, frame: &Activation, target: &Value) -> Result<Value, String> {
        match target {
            Value::Str(s) if s.is_empty() => Ok(Value::Clip(frame.clip)),
            Value::Str(s) => {
                let s = s.clone();
                self.resolve_path(frame, &s)
            }
            other => Ok(other.clone()),
        }
    }

    fn interval_due(&mut self) -> Result<(), String> {
        let due: Vec<u32> = self.intervals.iter().filter(|i| i.due <= self.now).map(|i| i.id).collect();
        for id in due {
            let Some(at) = self.intervals.iter().position(|i| i.id == id) else { continue };
            let (func, this, args) = {
                let interval = &mut self.intervals[at];
                interval.due = self.now + interval.period.max(1.0);
                (interval.func.clone(), interval.this.clone(), interval.args.clone())
            };
            self.budget = MAX_ACTIONS;
            self.call(&func, &this, &args)?;
            self.run_queued()?;
        }
        Ok(())
    }

    pub fn set_interval(&mut self, func: Value, this: Value, period: f64, args: Vec<Value>) -> u32 {
        let id = self.next_interval;
        self.next_interval += 1;
        self.intervals.push(Interval { id, func, this, args, period, due: self.now + period });
        id
    }

    pub fn clear_interval(&mut self, id: u32) {
        self.intervals.retain(|i| i.id != id);
    }

    pub fn listeners(&mut self, stage: bool) -> &mut Vec<Value> {
        if stage {
            &mut self.stage_listeners
        } else {
            &mut self.key_listeners
        }
    }

    pub fn last_key(&self) -> (u32, u32) {
        (self.last_key, self.last_ascii)
    }

    /// Every live clip, parents first.
    fn all_clips(&self) -> Result<Vec<ClipId>, String> {
        let mut out = vec![self.movie.root];
        let mut i = 0;
        while i < out.len() {
            for child in self.movie.clip(out[i])?.children.values() {
                if let Node::Clip(id) = child.node {
                    out.push(id);
                }
            }
            i += 1;
        }
        Ok(out)
    }

    /// Send a clip's `name` handler, when it has one.
    fn clip_event(&mut self, clip: ClipId, name: &str) -> Result<(), String> {
        if !self.movie.exists(clip) {
            return Ok(());
        }
        let handler = self.vars(clip).own(name);
        if let Some(f) = handler {
            self.budget = MAX_ACTIONS;
            self.call(&f, &Value::Clip(clip), &[])?;
        }
        Ok(())
    }

    /// One frame: `dt` milliseconds of movie time.
    pub fn advance(&mut self, dt: f64) -> Result<(), String> {
        self.now += dt;
        self.interval_due()?;
        self.movie.advance()?;
        self.run_queued()?;
        for clip in self.all_clips()? {
            self.clip_event(clip, "onEnterFrame")?;
            self.run_queued()?;
        }
        Ok(())
    }

    fn notify(&mut self, listeners: Vec<Value>, name: &str) -> Result<(), String> {
        for listener in listeners {
            let f = self.get_member(&listener, name)?;
            if matches!(f, Value::Object(_)) {
                self.budget = MAX_ACTIONS;
                self.call(&f, &listener, &[])?;
                self.run_queued()?;
            }
        }
        Ok(())
    }

    /// A key went down: `code` is the Flash key code.
    pub fn key_down(&mut self, code: u32, ascii: u32) -> Result<(), String> {
        self.last_key = code;
        self.last_ascii = ascii;
        self.keys.insert(code);
        self.notify(self.key_listeners.clone(), "onKeyDown")
    }

    pub fn key_up(&mut self, code: u32) -> Result<(), String> {
        self.last_key = code;
        self.keys.remove(&code);
        self.notify(self.key_listeners.clone(), "onKeyUp")
    }

    /// Call the function a dotted path names, the way the host's `Invoke` does.
    pub fn invoke(&mut self, path: &str, args: &[Value]) -> Result<Value, String> {
        let root = self.movie.root;
        let frame = Activation {
            code: Rc::new(Vec::new()),
            pc: 0,
            end: 0,
            pool: Rc::new(Vec::new()),
            registers: Vec::new(),
            scope: Vec::new(),
            function: false,
            this: Value::Clip(root),
            clip: root,
        };
        let (owner, name) = match path.rfind('.') {
            Some(at) => (self.resolve_path(&frame, &path[..at])?, &path[at + 1..]),
            None => (Value::Clip(root), path),
        };
        self.budget = MAX_ACTIONS;
        let result = self.call_method(&owner, name, args)?;
        self.run_queued()?;
        Ok(result)
    }

    /// Whether a function can be called by this path, as the host's `IsAvailable`.
    pub fn available(&mut self, path: &str) -> Result<bool, String> {
        let root = self.movie.root;
        let frame = Activation {
            code: Rc::new(Vec::new()),
            pc: 0,
            end: 0,
            pool: Rc::new(Vec::new()),
            registers: Vec::new(),
            scope: Vec::new(),
            function: false,
            this: Value::Clip(root),
            clip: root,
        };
        let (owner, name) = match path.rfind('.') {
            Some(at) => (self.resolve_path(&frame, &path[..at])?, &path[at + 1..]),
            None => (Value::Clip(root), path),
        };
        Ok(!matches!(self.get_member(&owner, name)?, Value::Undefined))
    }

    /// Set a variable by path, as the host's `SetVariable`.
    pub fn set_variable(&mut self, path: &str, value: Value) -> Result<(), String> {
        let root = self.movie.root;
        let frame = Activation {
            code: Rc::new(Vec::new()),
            pc: 0,
            end: 0,
            pool: Rc::new(Vec::new()),
            registers: Vec::new(),
            scope: Vec::new(),
            function: false,
            this: Value::Clip(root),
            clip: root,
        };
        self.assign_path(&frame, path, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::gfx_def::{Character, Control, MovieDef, Place, Timeline};

    /// A little assembler: one `Push` of each value, then the bare opcodes.
    fn push_str(code: &mut Vec<u8>, s: &str) {
        let body = [&[0u8][..], s.as_bytes(), &[0]].concat();
        code.push(0x96);
        code.extend((body.len() as u16).to_le_bytes());
        code.extend(body);
    }

    fn push_num(code: &mut Vec<u8>, n: i32) {
        code.extend([0x96, 5, 0, 7]);
        code.extend(n.to_le_bytes());
    }

    fn player(frames: Vec<Vec<Control>>) -> Player {
        let mut def = MovieDef::default();
        def.stage = [0, 25600, 0, 14400];
        def.characters.insert(2, Character::Sprite(Timeline { declared_frames: 1, frames: vec![vec![]], ..Timeline::default() }));
        def.exports.insert("thing".into(), 2);
        def.timeline.frames = frames;
        def.timeline.labels.insert("there".into(), 1);
        Player::new(Movie::new(Rc::new(def)).expect("movie")).expect("player")
    }

    fn set_var(code: &mut Vec<u8>, name: &str, value: impl FnOnce(&mut Vec<u8>)) {
        push_str(code, name);
        value(code);
        code.push(0x1D);
    }

    fn get(p: &mut Player, name: &str) -> Value {
        p.get_member(&Value::Clip(0), name).expect("get")
    }

    fn num(v: Value) -> f64 {
        match v {
            Value::Number(n) => n,
            other => panic!("{other:?} is not a number"),
        }
    }

    #[test]
    fn a_frame_script_sets_a_variable() {
        let mut code = Vec::new();
        set_var(&mut code, "answer", |c| {
            push_num(c, 6);
            push_num(c, 7);
            c.push(0x0C);
        });
        let mut p = player(vec![vec![Control::Action(code)]]);
        assert_eq!(num(get(&mut p, "answer")), 42.0);
    }

    #[test]
    fn a_listener_added_twice_is_held_once() {
        let mut p = player(vec![vec![]]);
        let key = p.global.get("Key").unwrap();
        let add = p.get_member(&key, "addListener").unwrap();
        let root = Value::Clip(p.movie.root);
        p.call(&add, &key, std::slice::from_ref(&root)).unwrap();
        p.call(&add, &key, std::slice::from_ref(&root)).unwrap();
        assert_eq!(p.listeners(false).len(), 1);
    }

    #[test]
    fn add2_concatenates_when_either_side_is_a_string() {
        let mut code = Vec::new();
        set_var(&mut code, "a", |c| {
            push_str(c, "x");
            push_num(c, 1);
            c.push(0x47);
        });
        let mut p = player(vec![vec![Control::Action(code)]]);
        assert!(matches!(get(&mut p, "a"), Value::Str(s) if &*s == "x1"));
    }

    #[test]
    fn a_branch_jumps_by_its_offset() {
        let mut code = Vec::new();
        code.extend([0x96, 2, 0, 5, 1]); // push true
        code.extend([0x9D, 2, 0, 0, 0]); // if, offset filled in below
        let skip = code.len();
        let mut skipped = Vec::new();
        set_var(&mut skipped, "wrong", |c| push_num(c, 1));
        code[skip - 2..skip].copy_from_slice(&(skipped.len() as i16).to_le_bytes());
        code.extend(skipped);
        set_var(&mut code, "right", |c| push_num(c, 1));
        let mut p = player(vec![vec![Control::Action(code)]]);
        assert!(matches!(get(&mut p, "wrong"), Value::Undefined));
        assert_eq!(num(get(&mut p, "right")), 1.0);
    }

    #[test]
    fn a_defined_function_takes_its_arguments() {
        // function double(n) { return n * 2; }
        let mut body = Vec::new();
        push_str(&mut body, "n");
        body.push(0x1C);
        push_num(&mut body, 2);
        body.extend([0x0C, 0x3E]);
        let mut header = b"double\0".to_vec();
        header.extend(1u16.to_le_bytes());
        header.extend(b"n\0");
        header.extend((body.len() as u16).to_le_bytes());
        let mut code = vec![0x9B];
        code.extend((header.len() as u16).to_le_bytes());
        code.extend(header);
        code.extend(body);
        set_var(&mut code, "out", |c| {
            push_num(c, 21);
            push_num(c, 1);
            push_str(c, "double");
            c.push(0x3D);
        });
        let mut p = player(vec![vec![Control::Action(code)]]);
        assert_eq!(num(get(&mut p, "out")), 42.0);
    }

    #[test]
    fn stop_holds_the_timeline_and_goto_label_stops_there() {
        let mut code = Vec::new();
        code.push(0x07);
        let mut p = player(vec![vec![Control::Action(code)], vec![]]);
        p.advance(16.0).expect("advance");
        assert_eq!(p.movie.clip(0).expect("root").frame, 0);
        let mut label = vec![0x8C];
        let name = b"there\0";
        label.extend((name.len() as u16).to_le_bytes());
        label.extend(name);
        p.run_action(0, Rc::new(label)).expect("run");
        assert_eq!(p.movie.clip(0).expect("root").frame, 1);
        assert!(!p.movie.clip(0).expect("root").playing);
    }

    #[test]
    fn an_fscommand_reaches_the_host() {
        let mut code = vec![0x83];
        let body = b"FSCommand:hello\0world\0";
        code.extend((body.len() as u16).to_le_bytes());
        code.extend(body);
        let p = player(vec![vec![Control::Action(code)]]);
        assert_eq!(p.fscommands, [("hello".to_string(), "world".to_string())]);
    }

    #[test]
    fn attach_movie_makes_a_named_clip_above_the_timeline() {
        let mut p = player(vec![vec![]]);
        let made = p.call_method(&Value::Clip(0), "attachMovie", &[Value::Str("thing".into()), Value::Str("one".into()), Value::Number(3.0)]).expect("call");
        let Value::Clip(id) = made else { panic!("a clip") };
        assert!(matches!(get(&mut p, "one"), Value::Clip(c) if c == id));
        assert_eq!(num(p.call_method(&Value::Clip(0), "getNextHighestDepth", &[]).expect("call")), 4.0);
    }

    #[test]
    fn display_properties_write_through_to_the_child() {
        let place = Place { depth: 1, character: Some(2), name: Some("kid".into()), ..Place::default() };
        let mut p = player(vec![vec![Control::Place(place)]]);
        let kid = get(&mut p, "kid");
        p.set_member(&kid, "_x", Value::Number(12.5)).expect("set");
        p.set_member(&kid, "_alpha", Value::Number(50.0)).expect("set");
        p.set_member(&kid, "_visible", Value::Bool(false)).expect("set");
        assert_eq!(num(p.get_member(&kid, "_x").expect("get")), 12.5);
        assert_eq!(num(p.get_member(&kid, "_alpha").expect("get")), 50.0);
        assert!(matches!(p.get_member(&kid, "_visible").expect("get"), Value::Bool(false)));
        assert!(matches!(p.get_member(&kid, "_parent").expect("get"), Value::Clip(0)));
    }

    #[test]
    fn an_interval_fires_when_its_time_comes() {
        let mut p = player(vec![vec![]]);
        fn tick(p: &mut Player, _: &Value, _: &[Value]) -> Result<Value, String> {
            p.traces.push("tick".into());
            Ok(Value::Undefined)
        }
        let f = p.native(tick);
        p.set_interval(f, Value::Undefined, 100.0, Vec::new());
        p.advance(50.0).expect("advance");
        assert!(p.traces.is_empty());
        p.advance(60.0).expect("advance");
        assert_eq!(p.traces, ["tick"]);
    }

    #[test]
    fn an_unknown_opcode_stops_the_script() {
        let mut p = player(vec![vec![]]);
        assert!(p.run_action(0, Rc::new(vec![0x2A])).is_err());
    }

    #[test]
    fn a_runaway_loop_stops_at_the_budget() {
        let mut p = player(vec![vec![]]);
        let code = vec![0x99, 2, 0, 0xFB, 0xFF]; // jump to itself
        assert!(p.run_action(0, Rc::new(code)).is_err());
    }

    #[test]
    fn a_method_nobody_wrote_is_reported() {
        let mut p = player(vec![vec![]]);
        p.call_method(&Value::Clip(0), "nope", &[]).expect("call");
        assert!(p.missing.contains("MovieClip.nope"));
    }

    #[test]
    fn number_text_is_flash_shaped() {
        assert_eq!(number_text(5.0), "5");
        assert_eq!(number_text(0.5), "0.5");
        assert_eq!(number_text(f64::NAN), "NaN");
    }
}
