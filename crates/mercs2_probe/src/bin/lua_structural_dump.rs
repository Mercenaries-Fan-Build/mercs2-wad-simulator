//! Dev bin: emit a canonical JSON **structural signature** of a Lua 5.1 bytecode file.
//!
//! Mercenaries 2 ships the same Lua on three hosts with three on-disk encodings:
//!   * PC Retail   — LE, `lua_Number` = f32, debug info kept.
//!   * Xbox 360    — BE, `lua_Number` = f32, debug info stripped.
//!   * PS3 (base)  — behind an uncracked keystream; inferred byte-identical to Xbox.
//! A textual diff of the raw bytes is useless (endian + stripped debug flip almost every byte).
//! A **structural** diff — constants, globals, call targets, child-proto layout — is what tells us
//! whether the two platforms were compiled from the same source, so this dumps exactly that.
//!
//! Rust-native parser, deliberately: `mercs2_luac`'s vendored `lundump.c` is wired to accept **only**
//! the LE/float dialect (and would reject Xbox BE outright at `LoadHeader`), and the FFI surface
//! does not expose `Proto*` internals even if the header matched. Rolling our own parser against the
//! same `lundump.c` spec is the smaller change than re-plumbing the vendored C to speak two
//! endiannesses *and* grow a Proto introspection API.
//!
//!   cargo run -p mercs2_probe --bin lua_structural_dump -- <path.luac>
//!
//! Prints a stable JSON tree to stdout. Diff two with any JSON diff — or just `diff`:
//!   diff <(lua_structural_dump pc.luac) <(lua_structural_dump xbox.luac)

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process;

use serde_json::{json, Map, Value as J};

/// A parse error. Message carries the byte offset so a corrupt chunk points somewhere useful.
#[derive(Debug)]
struct Err_(String);
impl std::fmt::Display for Err_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Err_ {}
type R<T> = std::result::Result<T, Err_>;

/// A byte-cursor that reads either LE or BE, int/size_t/float at header-declared widths.
///
/// Mirrors `lundump.c`'s `LoadByte`/`LoadInt`/`LoadNumber`/`LoadString` one-for-one.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    endian: Endian,
    int_size: usize,
    size_t_size: usize,
    instr_size: usize,
    number_size: usize,
    is_integral: bool,
}

#[derive(Copy, Clone, PartialEq)]
enum Endian { Le, Be }

impl Endian {
    fn as_str(self) -> &'static str {
        match self { Endian::Le => "little", Endian::Be => "big" }
    }
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> R<Reader<'a>> {
        if buf.len() < 12 || &buf[..4] != b"\x1bLua" {
            return Err(Err_("not a Lua chunk (missing \\x1bLua signature)".into()));
        }
        if buf[4] != 0x51 {
            return Err(Err_(format!("expected Lua 5.1 (version 0x51), got 0x{:02X}", buf[4])));
        }
        if buf[5] != 0x00 {
            return Err(Err_(format!("expected standard format (0), got {}", buf[5])));
        }
        let endian = match buf[6] { 1 => Endian::Le, 0 => Endian::Be, b => return Err(Err_(format!("bad endian byte {b}"))) };
        let int_size = buf[7] as usize;
        let size_t_size = buf[8] as usize;
        let instr_size = buf[9] as usize;
        let number_size = buf[10] as usize;
        let is_integral = buf[11] != 0;
        // Mercs2 ships int=4, size_t=4, instr=4, number=4-float on BOTH PC and Xbox. We decode anything
        // self-consistent so a mismatch flags LOUDLY (per the no-silent-fallback mandate) rather than
        // being papered over.
        if ![4, 8].contains(&int_size)      { return Err(Err_(format!("unsupported sizeof(int)={int_size}"))); }
        if ![4, 8].contains(&size_t_size)   { return Err(Err_(format!("unsupported sizeof(size_t)={size_t_size}"))); }
        if instr_size != 4                   { return Err(Err_(format!("unsupported sizeof(Instruction)={instr_size}"))); }
        if ![4, 8].contains(&number_size)   { return Err(Err_(format!("unsupported sizeof(lua_Number)={number_size}"))); }
        Ok(Reader { buf, pos: 12, endian, int_size, size_t_size, instr_size, number_size, is_integral })
    }

    fn remaining(&self) -> usize { self.buf.len().saturating_sub(self.pos) }

    fn need(&self, n: usize) -> R<()> {
        if self.remaining() < n {
            return Err(Err_(format!("unexpected end at offset {} (want {}, have {})", self.pos, n, self.remaining())));
        }
        Ok(())
    }

    fn read_bytes(&mut self, n: usize) -> R<&'a [u8]> {
        self.need(n)?;
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn byte(&mut self) -> R<u8> { Ok(self.read_bytes(1)?[0]) }

    /// Read `n` bytes as an unsigned integer under the chunk's endian. `n` ∈ {1,2,4,8}.
    fn uint(&mut self, n: usize) -> R<u64> {
        let b = self.read_bytes(n)?;
        let mut v = 0u64;
        match self.endian {
            Endian::Le => for i in 0..n { v |= (b[i] as u64) << (8 * i); },
            Endian::Be => for i in 0..n { v = (v << 8) | b[i] as u64; },
        }
        Ok(v)
    }

    /// `LoadInt` — read a (signed) int; `lundump.c` rejects negative, so we do too.
    fn int(&mut self) -> R<i64> {
        let n = self.int_size;
        let u = self.uint(n)?;
        // Sign-extend from n*8 bits.
        let bits = n as u32 * 8;
        let signed = if bits < 64 {
            let sign = 1u64 << (bits - 1);
            if u & sign != 0 { (u | (!0u64 << bits)) as i64 } else { u as i64 }
        } else { u as i64 };
        if signed < 0 { return Err(Err_(format!("negative int at offset {}", self.pos - n))); }
        Ok(signed)
    }

    fn size_t(&mut self) -> R<u64> { self.uint(self.size_t_size) }

    /// `LoadString` — size32 (really size_t) then that many bytes INCLUDING the trailing NUL.
    /// A zero size means the string is absent (NULL in C); we return `None`.
    fn string(&mut self) -> R<Option<String>> {
        let sz = self.size_t()? as usize;
        if sz == 0 { return Ok(None); }
        let bytes = self.read_bytes(sz)?;
        // Drop the trailing NUL `lundump` would strip.
        let end = if bytes.last() == Some(&0) { bytes.len() - 1 } else { bytes.len() };
        Ok(Some(String::from_utf8_lossy(&bytes[..end]).into_owned()))
    }

    /// `LoadNumber` under the chunk's declared `lua_Number` width + endian + integral flag.
    fn number(&mut self) -> R<f64> {
        let b = self.read_bytes(self.number_size)?.to_vec();
        if self.is_integral {
            // Integer `lua_Number` (never used by Mercs2, but spec-complete).
            let mut v = 0u64;
            match self.endian {
                Endian::Le => for i in 0..self.number_size { v |= (b[i] as u64) << (8 * i); },
                Endian::Be => for i in 0..self.number_size { v = (v << 8) | b[i] as u64; },
            }
            return Ok(v as i64 as f64);
        }
        match (self.number_size, self.endian) {
            (4, Endian::Le) => Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64),
            (4, Endian::Be) => Ok(f32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64),
            (8, Endian::Le) => Ok(f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])),
            (8, Endian::Be) => Ok(f64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])),
            _ => Err(Err_(format!("unsupported lua_Number width {}", self.number_size))),
        }
    }
}

// ─── Lua 5.1 opcode set (ORDER matches `lopcodes.h`) ─────────────────────────────────────────

const OP_MOVE: u8 = 0;
const OP_LOADK: u8 = 1;
const OP_LOADBOOL: u8 = 2;
const OP_LOADNIL: u8 = 3;
const OP_GETUPVAL: u8 = 4;
const OP_GETGLOBAL: u8 = 5;
const OP_GETTABLE: u8 = 6;
const OP_SETGLOBAL: u8 = 7;
const OP_SETUPVAL: u8 = 8;
const OP_SETTABLE: u8 = 9;
const OP_NEWTABLE: u8 = 10;
const OP_SELF: u8 = 11;
const OP_CALL: u8 = 28;
const OP_TAILCALL: u8 = 29;
const OP_CLOSURE: u8 = 36;

const OPNAMES: &[&str] = &[
    "MOVE","LOADK","LOADBOOL","LOADNIL","GETUPVAL","GETGLOBAL","GETTABLE","SETGLOBAL","SETUPVAL",
    "SETTABLE","NEWTABLE","SELF","ADD","SUB","MUL","DIV","MOD","POW","UNM","NOT","LEN","CONCAT",
    "JMP","EQ","LT","LE","TEST","TESTSET","CALL","TAILCALL","RETURN","FORLOOP","FORPREP",
    "TFORLOOP","SETLIST","CLOSE","CLOSURE","VARARG",
];

/// Instruction fields per `lopcodes.h`: op in bits 0-5, A in 6-13, C in 14-22, B in 23-31,
/// Bx is C||B (18 bits). ALWAYS little-endian relative to the instruction *word* — the chunk's
/// endian byte determines how the 4 bytes assemble into a u32, not how the bits lay out inside it.
fn op_of(instr: u32) -> u8 { (instr & 0x3F) as u8 }
fn arg_a(instr: u32) -> u32 { (instr >> 6) & 0xFF }
fn arg_b(instr: u32) -> u32 { (instr >> 23) & 0x1FF }
fn arg_c(instr: u32) -> u32 { (instr >> 14) & 0x1FF }
fn arg_bx(instr: u32) -> u32 { (instr >> 14) & 0x3FFFF }
fn is_k(x: u32) -> bool { x & 0x100 != 0 }
fn index_k(x: u32) -> u32 { x & 0xFF }

// ─── parsed shapes ───────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
enum Konst { Nil, Bool(bool), Number(f64), String(String) }

impl Konst {
    fn to_json(&self) -> J {
        match self {
            Konst::Nil        => json!({ "type": "nil" }),
            Konst::Bool(b)    => json!({ "type": "bool",   "value": b }),
            Konst::Number(n)  => json!({ "type": "number", "value": n }),
            Konst::String(s)  => json!({ "type": "string", "value": s }),
        }
    }
    /// Short, printable form for use inside call_targets etc.
    fn short(&self) -> String {
        match self {
            Konst::Nil        => "nil".into(),
            Konst::Bool(b)    => b.to_string(),
            Konst::Number(n)  => format!("{n}"),
            Konst::String(s)  => s.clone(),
        }
    }
}

struct Proto {
    source_name: Option<String>,
    line_defined: i64,
    last_line_defined: i64,
    num_upvalues: u8,
    num_params: u8,
    is_vararg: u8,
    max_stack: u8,
    code: Vec<u32>,
    constants: Vec<Konst>,
    protos: Vec<Proto>,
    upvalue_names: Vec<Option<String>>,
}

fn load_function(r: &mut Reader) -> R<Proto> {
    let source_name = r.string()?;
    let line_defined = r.int()?;
    let last_line_defined = r.int()?;
    let num_upvalues = r.byte()?;
    let num_params   = r.byte()?;
    let is_vararg    = r.byte()?;
    let max_stack    = r.byte()?;

    // Code
    let n_code = r.int()? as usize;
    let mut code = Vec::with_capacity(n_code);
    for _ in 0..n_code {
        // An Instruction is read as sizeof(Instruction)=4 bytes under the chunk's endian.
        code.push(r.uint(r.instr_size)? as u32);
    }

    // Constants
    let n_k = r.int()? as usize;
    let mut constants = Vec::with_capacity(n_k);
    for _ in 0..n_k {
        let t = r.byte()?;
        constants.push(match t {
            0 => Konst::Nil,                       // LUA_TNIL
            1 => Konst::Bool(r.byte()? != 0),      // LUA_TBOOLEAN
            3 => Konst::Number(r.number()?),       // LUA_TNUMBER
            4 => Konst::String(r.string()?.unwrap_or_default()), // LUA_TSTRING
            other => return Err(Err_(format!("bad constant tag {other} at offset {}", r.pos - 1))),
        });
    }

    // Nested protos
    let n_p = r.int()? as usize;
    let mut protos = Vec::with_capacity(n_p);
    for _ in 0..n_p { protos.push(load_function(r)?); }

    // Debug: lineinfo (ints), locvars (string + 2 ints), upvalue names (strings).
    // Each sub-count is zero on a stripped chunk, so this transparently handles --strip.
    let n_lineinfo = r.int()? as usize;
    for _ in 0..n_lineinfo { let _ = r.int()?; }
    let n_locvars = r.int()? as usize;
    for _ in 0..n_locvars { let _ = r.string()?; let _ = r.int()?; let _ = r.int()?; }
    let n_upnames = r.int()? as usize;
    let mut upvalue_names = Vec::with_capacity(n_upnames);
    for _ in 0..n_upnames { upvalue_names.push(r.string()?); }

    Ok(Proto {
        source_name, line_defined, last_line_defined,
        num_upvalues, num_params, is_vararg, max_stack,
        code, constants, protos, upvalue_names,
    })
}

// ─── structural signature ────────────────────────────────────────────────────────────────────

/// What the proto reads from `_G`, in program order, with `GET`/`SET` disambiguated.
fn globals_accessed(p: &Proto) -> Vec<J> {
    let mut out = Vec::new();
    for &i in &p.code {
        let op = op_of(i);
        if op == OP_GETGLOBAL || op == OP_SETGLOBAL {
            let bx = arg_bx(i) as usize;
            let name = match p.constants.get(bx) {
                Some(Konst::String(s)) => s.clone(),
                Some(k) => format!("<{}:{}>", type_of(k), k.short()),
                None => format!("<K[{bx}] out of range>"),
            };
            out.push(json!({
                "op": if op == OP_GETGLOBAL { "GETGLOBAL" } else { "SETGLOBAL" },
                "name": name,
            }));
        }
    }
    out
}

/// A rough but useful call-flow signature: at each `CALL`/`TAILCALL`, what populated the callee
/// slot `R(A)` LAST? If it was a `GETGLOBAL`, we have the global name. If it was a `SELF`, we have
/// the method name. If it was `GETUPVAL`, we name the upvalue. Otherwise "local" and the register.
///
/// This is a one-pass peephole, deliberately: a proper backward dataflow adds a lot of code to
/// catch calls through locals, which this signature doesn't claim to resolve anyway.
fn call_targets(p: &Proto) -> Vec<J> {
    let mut out = Vec::new();
    for (pc, &i) in p.code.iter().enumerate() {
        let op = op_of(i);
        if op != OP_CALL && op != OP_TAILCALL { continue; }
        let a = arg_a(i);
        let (kind, name) = resolve_callee(p, pc, a);
        out.push(json!({ "kind": kind, "name": name }));
    }
    out
}

/// Walk back from `pc` looking for the instruction that most recently wrote register `a`.
/// Scans the whole proto rather than guessing a window — a basic block isn't a block boundary we
/// have cheap access to, and getting the signature right beats getting it fast on a chunk this size.
fn resolve_callee(p: &Proto, call_pc: usize, a: u32) -> (&'static str, String) {
    for pc in (0..call_pc).rev() {
        let i = p.code[pc];
        let op = op_of(i);
        if op == OP_GETGLOBAL && arg_a(i) == a {
            let bx = arg_bx(i) as usize;
            return ("global", const_name(p, bx));
        }
        if op == OP_SELF && arg_a(i) == a {
            // SELF emits R(A+1)=obj; R(A)=obj[RK(C)]. The method NAME is RK(C).
            let c = arg_c(i);
            return ("method", rk_name(p, c));
        }
        if op == OP_GETUPVAL && arg_a(i) == a {
            let b = arg_b(i) as usize;
            let n = p.upvalue_names.get(b).and_then(|x| x.clone()).unwrap_or_else(|| format!("U[{b}]"));
            return ("upvalue", n);
        }
        if op == OP_MOVE && arg_a(i) == a {
            // Keep walking from the source register (bounded; a cycle just means we fall off).
            return resolve_callee(p, pc, arg_b(i));
        }
        if op == OP_CLOSURE && arg_a(i) == a {
            return ("closure", format!("proto[{}]", arg_bx(i)));
        }
        // Any OTHER op that writes R(A) with A == a is a non-trivial computed call target.
        if writes_register(op, i) == Some(a) { return ("local", format!("R[{a}]@pc{pc}:{}", opname(op))); }
    }
    ("local", format!("R[{a}]"))
}

fn const_name(p: &Proto, k: usize) -> String {
    match p.constants.get(k) {
        Some(Konst::String(s)) => s.clone(),
        Some(other) => format!("<{}:{}>", type_of(other), other.short()),
        None => format!("<K[{k}] out of range>"),
    }
}

fn rk_name(p: &Proto, rk: u32) -> String {
    if is_k(rk) { const_name(p, index_k(rk) as usize) } else { format!("R[{rk}]") }
}

/// Which register does this instruction write (if any)? Only the ones we care about for callee
/// resolution — a complete table lives in `lopcodes.c` and we don't need it here.
fn writes_register(op: u8, i: u32) -> Option<u32> {
    match op {
        OP_MOVE | OP_LOADK | OP_LOADBOOL | OP_LOADNIL | OP_GETUPVAL | OP_GETGLOBAL
        | OP_GETTABLE | OP_NEWTABLE | OP_SELF | OP_CLOSURE
        | 12..=21     // ADD..CONCAT all write R(A)
        | OP_CALL | OP_TAILCALL     // CALL writes starting at R(A)
        | 37                        // VARARG
        => Some(arg_a(i)),
        _ => None,
    }
}

fn opname(op: u8) -> &'static str {
    OPNAMES.get(op as usize).copied().unwrap_or("??")
}

fn type_of(k: &Konst) -> &'static str {
    match k { Konst::Nil => "nil", Konst::Bool(_) => "bool", Konst::Number(_) => "number", Konst::String(_) => "string" }
}

fn proto_to_json(p: &Proto) -> J {
    let mut m = Map::new();
    m.insert("source_name".into(), J::from(p.source_name.clone()));
    m.insert("line_defined".into(), J::from(p.line_defined));
    m.insert("last_line_defined".into(), J::from(p.last_line_defined));
    m.insert("num_params".into(), J::from(p.num_params));
    m.insert("is_vararg".into(), J::from(p.is_vararg));
    m.insert("max_stack".into(), J::from(p.max_stack));
    m.insert("num_upvalues".into(), J::from(p.num_upvalues));
    m.insert("num_instructions".into(), J::from(p.code.len()));
    m.insert("constants".into(), J::Array(p.constants.iter().map(Konst::to_json).collect()));
    m.insert("globals_accessed".into(), J::Array(globals_accessed(p)));
    m.insert("call_targets".into(), J::Array(call_targets(p)));
    m.insert("child_protos".into(), J::Array(p.protos.iter().map(proto_to_json).collect()));
    J::Object(m)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 || args[1] == "-h" || args[1] == "--help" {
        eprintln!("usage: lua_structural_dump <path.luac>");
        process::exit(2);
    }
    let path = &args[1];
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => { eprintln!("read {path}: {e}"); process::exit(1); }
    };
    let mut r = match Reader::new(&bytes) {
        Ok(r) => r,
        Err(e) => { eprintln!("{path}: {e}"); process::exit(1); }
    };
    let header = json!({
        "signature": "\\x1bLua",
        "version": format!("0x{:02X}", bytes[4]),
        "format": bytes[5],
        "endian": r.endian.as_str(),
        "int_size": r.int_size,
        "size_t_size": r.size_t_size,
        "instr_size": r.instr_size,
        "number_size": r.number_size,
        "is_integral": r.is_integral,
    });
    let top = match load_function(&mut r) {
        Ok(p) => p,
        Err(e) => { eprintln!("{path}: parse: {e}"); process::exit(1); }
    };
    let out = json!({ "path": path, "header": header, "main": proto_to_json(&top) });
    let s = serde_json::to_string_pretty(&out).expect("serialize");
    // One final newline so `diff` doesn't flag "no newline at EOF" on the LAST line.
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(s.as_bytes());
    let _ = lock.write_all(b"\n");
}
