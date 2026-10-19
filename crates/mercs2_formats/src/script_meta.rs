//! A script container's INFO and DEPS chunks, derived from its compiled Lua the way the retail
//! script compiler derived them.
//!
//! Every script container holds `INFO`, then `DEPS` when it has one, then `BINN` (the LuaQ chunk).
//! The engine reads the first two when it loads the script resource (`FUN_0059cf90`):
//!
//! ```text
//! INFO  u8 5, u16 name length, name, NUL, u16 globals, u8 inherits
//! DEPS  u8 count, count × u32 pandemic_hash_m2(module name)
//! ```
//!
//! * **name** — the bare script name, lowercase, which the engine keys `_MODULES[name]` and the
//!   module env's `[1]` by, and which `dynamic_import` uses to find the importing script. Retail's
//!   name always equals the chunk's own source name.
//! * **DEPS** — one entry per call `import("x")` / `inherit("x")` with a literal name, in the order
//!   the calls appear, duplicates kept. A script with no such call has no DEPS chunk at all. The
//!   engine loads every DEPS entry before it runs the script, so this is what makes a module's
//!   imports exist when its body runs. `dynamic_import` is never listed.
//! * **inherits** — how many of those calls are `inherit`; the engine presizes the env's inherit
//!   list (`env[2]`) with it and creates that list only when it is non-zero.
//! * **globals** — how many globals the script's top-level code leaves non-nil; the engine presizes
//!   the module env with it. Computed by running the main function abstractly ([`count_globals`]).
//!
//! These rules reproduce the INFO and DEPS chunks of every retail script byte for byte; the
//! `script_meta_retail` test holds them to that.

use crate::hash::pandemic_hash_m2;
use crate::luachunk::{decode, Chunk, Constant, Proto};
use crate::ffcs::Endian;

/// The INFO record's version byte.
const INFO_VERSION: u8 = 5;
/// DEPS stores its count in one byte.
pub const MAX_DEPS: usize = 255;

const OP_MOVE: u32 = 0;
const OP_LOADK: u32 = 1;
const OP_LOADBOOL: u32 = 2;
const OP_LOADNIL: u32 = 3;
const OP_GETUPVAL: u32 = 4;
const OP_GETGLOBAL: u32 = 5;
const OP_GETTABLE: u32 = 6;
const OP_SETGLOBAL: u32 = 7;
const OP_SETUPVAL: u32 = 8;
const OP_SETTABLE: u32 = 9;
const OP_NEWTABLE: u32 = 10;
const OP_SELF: u32 = 11;
const OP_ADD: u32 = 12;
const OP_LEN: u32 = 20;
const OP_NOT: u32 = 19;
const OP_UNM: u32 = 18;
const OP_CONCAT: u32 = 21;
const OP_JMP: u32 = 22;
const OP_EQ: u32 = 23;
const OP_LT: u32 = 24;
const OP_LE: u32 = 25;
const OP_TEST: u32 = 26;
const OP_TESTSET: u32 = 27;
const OP_CALL: u32 = 28;
const OP_TAILCALL: u32 = 29;
const OP_RETURN: u32 = 30;
const OP_FORLOOP: u32 = 31;
const OP_FORPREP: u32 = 32;
const OP_TFORLOOP: u32 = 33;
const OP_SETLIST: u32 = 34;
const OP_CLOSE: u32 = 35;
const OP_CLOSURE: u32 = 36;
const OP_VARARG: u32 = 37;

fn op(i: u32) -> u32 {
    i & 0x3f
}
fn a(i: u32) -> usize {
    ((i >> 6) & 0xff) as usize
}
fn b(i: u32) -> u32 {
    (i >> 23) & 0x1ff
}
fn c(i: u32) -> u32 {
    (i >> 14) & 0x1ff
}
fn bx(i: u32) -> usize {
    (i >> 14) as usize
}
fn sbx(i: u32) -> i64 {
    (i >> 14) as i64 - 131_071
}

/// One DEPS entry: the module name as the source spells it, and its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dep {
    pub name: String,
    pub hash: u32,
    pub inherit: bool,
}

/// Everything INFO and DEPS say about one script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptMeta {
    pub name: String,
    pub globals: u16,
    pub inherits: u8,
    pub deps: Vec<Dep>,
}

impl ScriptMeta {
    /// The INFO chunk body.
    pub fn info_body(&self) -> Vec<u8> {
        let mut out = vec![INFO_VERSION];
        out.extend_from_slice(&(self.name.len() as u16).to_le_bytes());
        out.extend_from_slice(self.name.as_bytes());
        out.push(0);
        out.extend_from_slice(&self.globals.to_le_bytes());
        out.push(self.inherits);
        out
    }

    /// The DEPS chunk body, or `None` when the script imports nothing (retail then has no DEPS chunk).
    pub fn deps_body(&self) -> Option<Vec<u8>> {
        if self.deps.is_empty() {
            return None;
        }
        let mut out = vec![self.deps.len() as u8];
        for d in &self.deps {
            out.extend_from_slice(&d.hash.to_le_bytes());
        }
        Some(out)
    }
}

/// Derive `name`'s INFO and DEPS from its compiled chunk `luaq`.
///
/// Errors, each naming the script: `name` is empty or not lowercase; the chunk does not decode, is
/// not little-endian, or names a different source; more than [`MAX_DEPS`] imports or 255 inherits;
/// or the globals count cannot be computed ([`count_globals`]).
pub fn derive(name: &str, luaq: &[u8]) -> Result<ScriptMeta, String> {
    if name.is_empty() {
        return Err("a script needs a name".into());
    }
    if name != name.to_ascii_lowercase() || name.contains('\0') || name.len() > u16::MAX as usize {
        return Err(format!(
            "script name {name:?} must be lowercase ASCII: the engine keys `_MODULES` by it exactly as written"
        ));
    }
    let chunk = decode(luaq).map_err(|e| format!("script {name}: {e}"))?;
    derive_from_chunk(name, &chunk)
}

/// [`derive`] for an already decoded chunk.
pub fn derive_from_chunk(name: &str, chunk: &Chunk) -> Result<ScriptMeta, String> {
    if chunk.endian != Endian::Little {
        return Err(format!("script {name}: a PC script container holds a little-endian chunk"));
    }
    match &chunk.main.source {
        Some(s) if s.as_slice() == name.as_bytes() => {}
        other => {
            return Err(format!(
                "script {name}: the chunk's source name is {:?}, not {name:?}; compile it with the bare script name as its chunk name",
                other.as_ref().map(|s| String::from_utf8_lossy(s).into_owned())
            ))
        }
    }
    let mut deps = Vec::new();
    collect_deps(&chunk.main, &mut deps);
    if deps.len() > MAX_DEPS {
        return Err(format!(
            "script {name}: {} literal import/inherit calls; DEPS holds at most {MAX_DEPS}",
            deps.len()
        ));
    }
    let inherits = deps.iter().filter(|d| d.inherit).count();
    let globals = count_globals(&chunk.main).map_err(|e| format!("script {name}: {e}"))?;
    Ok(ScriptMeta { name: name.to_string(), globals, inherits: inherits as u8, deps })
}

/// What a register is known to hold while looking for `import("x")` / `inherit("x")`.
#[derive(Clone)]
enum Tracked {
    Global(Vec<u8>),
    Str(Vec<u8>),
}

/// Every call `import(<literal>)` / `inherit(<literal>)` in `p` and the functions nested in it, in
/// source order: a nested function's calls come at the `CLOSURE` that creates it.
///
/// The pattern is the code Lua 5.1 emits for such a call — `GETGLOBAL R(a) "import"`,
/// `LOADK R(a+1) "x"`, `CALL R(a)` with exactly one argument (B = 2) — which is the same for
/// `import "x"` without parentheses.
fn collect_deps(p: &Proto, out: &mut Vec<Dep>) {
    let mut regs: Vec<Option<Tracked>> = vec![None; 512];
    let str_k = |i: usize| match p.constants.get(i) {
        Some(Constant::String(s)) => Some(s.clone()),
        _ => None,
    };
    let mut pc = 0usize;
    while pc < p.code.len() {
        let i = p.code[pc];
        pc += 1;
        let ra = a(i);
        match op(i) {
            OP_GETGLOBAL => regs[ra] = str_k(bx(i)).map(Tracked::Global),
            OP_LOADK => regs[ra] = str_k(bx(i)).map(Tracked::Str),
            OP_CALL | OP_TAILCALL => {
                if let (Some(Tracked::Global(f)), Some(Tracked::Str(arg))) = (&regs[ra], &regs[ra + 1]) {
                    let inherit = f.as_slice() == b"inherit";
                    if (inherit || f.as_slice() == b"import") && b(i) == 2 {
                        let name = String::from_utf8_lossy(arg).into_owned();
                        out.push(Dep { hash: pandemic_hash_m2(&name), name, inherit });
                    }
                }
                regs[ra..].iter_mut().for_each(|r| *r = None);
            }
            OP_CLOSURE => {
                regs[ra] = None;
                if let Some(child) = p.protos.get(bx(i)) {
                    collect_deps(child, out);
                    // The closure's upvalue pseudo-instructions are not executed.
                    pc += child.nups as usize;
                }
            }
            OP_LOADNIL => {
                for r in ra..=(b(i) as usize).max(ra) {
                    regs[r] = None;
                }
            }
            OP_SETLIST => {
                // C = 0 stores the real count in the next code word, which is not an instruction.
                if c(i) == 0 {
                    pc += 1;
                }
            }
            // These write no register.
            OP_SETGLOBAL | OP_SETUPVAL | OP_SETTABLE | OP_JMP | OP_EQ | OP_LT | OP_LE | OP_TEST
            | OP_RETURN | OP_CLOSE => {}
            _ => regs[ra] = None,
        }
    }
}

/// A value while running a main function abstractly.
#[derive(Clone, Debug, PartialEq)]
enum Val {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Vec<u8>),
    /// A table this function built.
    Table,
    /// A function this function built.
    Closure,
    /// Anything the engine or another module provides: never nil, never compared.
    Unknown,
}

impl Val {
    fn truthy(&self) -> bool {
        !matches!(self, Val::Nil | Val::Bool(false))
    }
}

/// Step limit for [`count_globals`], far above any retail main function.
const MAX_STEPS: usize = 1_000_000;

/// How many distinct globals the main function `p` leaves non-nil when it finishes.
///
/// Runs the code abstractly: constants, tables, closures and booleans keep their real values; every
/// global the script has not set, every table field and every upvalue is an unknown non-nil value;
/// every call returns nil; branches follow Lua's truthiness. Running stops at the first `RETURN` or
/// `TAILCALL`.
///
/// When the outcome depends on a value the model does not know — comparing an unknown value, a
/// numeric `for` whose bounds are not constants, calling or indexing nil — the count is not
/// computable and this returns an error naming the instruction, rather than a guess.
pub fn count_globals(p: &Proto) -> Result<u16, String> {
    let mut reg: Vec<Val> = vec![Val::Nil; 512];
    let mut env: Vec<(Vec<u8>, Val)> = Vec::new();
    let k = |i: usize| -> Result<Val, String> {
        Ok(match p.constants.get(i) {
            Some(Constant::Nil) => Val::Nil,
            Some(Constant::Boolean(v)) => Val::Bool(*v),
            Some(Constant::Number(bits)) => Val::Num(f32::from_bits(*bits) as f64),
            Some(Constant::String(s)) => Val::Str(s.clone()),
            None => return Err(format!("constant {i} does not exist")),
        })
    };
    let undecidable = |what: &str, pc: usize| -> String {
        match p.line_info.get(pc) {
            Some(line) => format!("cannot compute the INFO globals count: {what} at line {line} (main-function instruction {pc})"),
            None => format!("cannot compute the INFO globals count: {what} at main-function instruction {pc}"),
        }
    };
    let mut pc = 0usize;
    let mut steps = 0usize;
    while pc < p.code.len() {
        steps += 1;
        if steps > MAX_STEPS {
            return Err(undecidable("the top-level code does not finish within the step limit", pc));
        }
        let at = pc;
        let i = p.code[pc];
        pc += 1;
        let ra = a(i);
        let rk = |x: u32, reg: &Vec<Val>| -> Result<Val, String> {
            if x >= 256 {
                k((x - 256) as usize)
            } else {
                Ok(reg[x as usize].clone())
            }
        };
        match op(i) {
            OP_MOVE => reg[ra] = reg[b(i) as usize].clone(),
            OP_LOADK => reg[ra] = k(bx(i))?,
            OP_LOADBOOL => {
                reg[ra] = Val::Bool(b(i) != 0);
                if c(i) != 0 {
                    pc += 1;
                }
            }
            OP_LOADNIL => {
                for r in ra..=(b(i) as usize).max(ra) {
                    reg[r] = Val::Nil;
                }
            }
            OP_GETUPVAL => reg[ra] = Val::Unknown,
            OP_GETGLOBAL => {
                let Val::Str(name) = k(bx(i))? else {
                    return Err(undecidable("a global named by a non-string constant", at));
                };
                reg[ra] = env.iter().rev().find(|(n, _)| *n == name).map(|(_, v)| v.clone()).unwrap_or(Val::Unknown);
            }
            OP_GETTABLE => {
                if reg[b(i) as usize] == Val::Nil {
                    return Err(undecidable("indexing nil", at));
                }
                reg[ra] = Val::Unknown;
            }
            OP_SETGLOBAL => {
                let Val::Str(name) = k(bx(i))? else {
                    return Err(undecidable("a global named by a non-string constant", at));
                };
                let v = reg[ra].clone();
                match env.iter_mut().find(|(n, _)| *n == name) {
                    Some(slot) => slot.1 = v,
                    None => env.push((name, v)),
                }
            }
            OP_SETUPVAL | OP_SETTABLE | OP_CLOSE => {}
            OP_SETLIST => {
                if c(i) == 0 {
                    pc += 1;
                }
            }
            OP_NEWTABLE => reg[ra] = Val::Table,
            OP_SELF => {
                let obj = reg[b(i) as usize].clone();
                if obj == Val::Nil {
                    return Err(undecidable("a method call on nil", at));
                }
                reg[ra + 1] = obj;
                reg[ra] = Val::Unknown;
            }
            x if (OP_ADD..=OP_UNM).contains(&x) || x == OP_LEN => reg[ra] = Val::Num(0.0),
            OP_NOT => reg[ra] = Val::Bool(!reg[b(i) as usize].truthy()),
            OP_CONCAT => reg[ra] = Val::Str(Vec::new()),
            OP_JMP => pc = (pc as i64 + sbx(i)) as usize,
            OP_EQ | OP_LT | OP_LE => {
                let (x, y) = (rk(b(i), &reg)?, rk(c(i), &reg)?);
                let known = |v: &Val| !matches!(v, Val::Unknown | Val::Table | Val::Closure);
                if !known(&x) || !known(&y) {
                    return Err(undecidable("a comparison involving a value the script does not set itself", at));
                }
                let res = match (op(i), &x, &y) {
                    (OP_EQ, _, _) => x == y,
                    (OP_LT, Val::Num(p), Val::Num(q)) => p < q,
                    (OP_LE, Val::Num(p), Val::Num(q)) => p <= q,
                    (OP_LT, Val::Str(p), Val::Str(q)) => p < q,
                    (OP_LE, Val::Str(p), Val::Str(q)) => p <= q,
                    _ => return Err(undecidable("an ordering comparison of mixed types", at)),
                };
                if res != (ra != 0) {
                    pc += 1;
                }
            }
            OP_TEST => {
                if reg[ra].truthy() != (c(i) != 0) {
                    pc += 1;
                }
            }
            OP_TESTSET => {
                let v = reg[b(i) as usize].clone();
                if v.truthy() == (c(i) != 0) {
                    reg[ra] = v;
                } else {
                    pc += 1;
                }
            }
            OP_CALL => {
                if reg[ra] == Val::Nil {
                    return Err(undecidable("calling nil", at));
                }
                let results = (c(i) as usize).saturating_sub(1).max(1);
                for r in ra..ra + results {
                    reg[r] = Val::Nil;
                }
            }
            OP_TAILCALL | OP_RETURN => break,
            OP_FORPREP => {
                let (Val::Num(init), Val::Num(_), Val::Num(step)) = (&reg[ra], &reg[ra + 1], &reg[ra + 2]) else {
                    return Err(undecidable("a numeric for loop whose bounds the script does not set itself", at));
                };
                reg[ra] = Val::Num(init - step);
                pc = (pc as i64 + sbx(i)) as usize;
            }
            OP_FORLOOP => {
                let (Val::Num(idx), Val::Num(limit), Val::Num(step)) = (&reg[ra], &reg[ra + 1], &reg[ra + 2]) else {
                    return Err(undecidable("a numeric for loop whose bounds the script does not set itself", at));
                };
                let (idx, limit, step) = (*idx, *limit, *step);
                let next = idx + step;
                reg[ra] = Val::Num(next);
                if (step > 0.0 && next <= limit) || (step <= 0.0 && next >= limit) {
                    pc = (pc as i64 + sbx(i)) as usize;
                    reg[ra + 3] = Val::Num(next);
                }
            }
            OP_TFORLOOP => {
                // The iterator is a call, and calls return nil: the loop ends.
                reg[ra + 3] = Val::Nil;
                pc += 1;
            }
            OP_CLOSURE => {
                let nups = p.protos.get(bx(i)).map(|q| q.nups as usize).ok_or_else(|| {
                    undecidable("a closure of a function that does not exist", at)
                })?;
                reg[ra] = Val::Closure;
                pc += nups;
            }
            OP_VARARG => {
                let n = (b(i) as usize).saturating_sub(1).max(1);
                for r in ra..ra + n {
                    reg[r] = Val::Nil;
                }
            }
            other => return Err(undecidable(&format!("opcode {other}"), at)),
        }
    }
    let n = env.iter().filter(|(_, v)| *v != Val::Nil).count();
    u16::try_from(n).map_err(|_| format!("{n} globals; INFO holds at most {}", u16::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::luachunk::encode;

    /// `iABC` / `iABx` / `iAsBx` instruction words.
    fn abc(op: u32, a: u32, b: u32, c: u32) -> u32 {
        op | (a << 6) | (c << 14) | (b << 23)
    }
    fn abx(op: u32, a: u32, bx: u32) -> u32 {
        op | (a << 6) | (bx << 14)
    }
    fn asbx(op: u32, a: u32, sbx: i64) -> u32 {
        op | (a << 6) | (((sbx + 131_071) as u32) << 14)
    }

    fn proto(code: Vec<u32>, constants: Vec<Constant>, protos: Vec<Proto>) -> Proto {
        Proto {
            source: None,
            line_defined: 0,
            last_line_defined: 0,
            nups: 0,
            num_params: 0,
            is_vararg: 2,
            max_stack: 8,
            code,
            constants,
            protos,
            line_info: vec![],
            loc_vars: vec![],
            upvalues: vec![],
        }
    }

    fn s(v: &str) -> Constant {
        Constant::String(v.as_bytes().to_vec())
    }

    fn chunk(name: &str, mut main: Proto) -> Vec<u8> {
        main.source = Some(name.as_bytes().to_vec());
        encode(&Chunk { endian: Endian::Little, main })
    }

    /// `import("MrxUtil")  inherit("MrxTask")  import("MrxUtil")  function f() import("MrxPmc") end
    ///  x = 1  y = nil  z = {}`
    fn sample() -> Vec<u8> {
        let inner = proto(
            vec![abx(OP_GETGLOBAL, 0, 0), abx(OP_LOADK, 1, 1), abc(OP_CALL, 0, 2, 1), abc(OP_RETURN, 0, 1, 0)],
            vec![s("import"), s("MrxPmc")],
            vec![],
        );
        let main = proto(
            vec![
                abx(OP_GETGLOBAL, 0, 0),
                abx(OP_LOADK, 1, 1),
                abc(OP_CALL, 0, 2, 1),
                abx(OP_GETGLOBAL, 0, 2),
                abx(OP_LOADK, 1, 3),
                abc(OP_CALL, 0, 2, 1),
                abx(OP_GETGLOBAL, 0, 0),
                abx(OP_LOADK, 1, 1),
                abc(OP_CALL, 0, 2, 1),
                abx(OP_CLOSURE, 0, 0),
                abx(OP_SETGLOBAL, 0, 4),
                abx(OP_LOADK, 0, 5),
                abx(OP_SETGLOBAL, 0, 6),
                abc(OP_LOADNIL, 0, 0, 0),
                abx(OP_SETGLOBAL, 0, 7),
                abc(OP_NEWTABLE, 0, 0, 0),
                abx(OP_SETGLOBAL, 0, 8),
                abc(OP_RETURN, 0, 1, 0),
            ],
            vec![
                s("import"),
                s("MrxUtil"),
                s("inherit"),
                s("MrxTask"),
                s("f"),
                Constant::Number(1.0f32.to_bits()),
                s("x"),
                s("y"),
                s("z"),
            ],
            vec![inner],
        );
        chunk("sample", main)
    }

    #[test]
    fn deps_are_literal_calls_in_source_order_with_duplicates() {
        let m = derive("sample", &sample()).unwrap();
        let names: Vec<&str> = m.deps.iter().map(|d| d.name.as_str()).collect();
        // The nested function's import comes at its CLOSURE, after the three top-level calls.
        assert_eq!(names, ["MrxUtil", "MrxTask", "MrxUtil", "MrxPmc"]);
        assert_eq!(m.deps[1].hash, pandemic_hash_m2("MrxTask"));
        assert_eq!(m.inherits, 1);
    }

    #[test]
    fn globals_count_only_non_nil_top_level_globals() {
        // f (closure), x (1) and z ({}) are non-nil; y is nil.
        assert_eq!(derive("sample", &sample()).unwrap().globals, 3);
    }

    #[test]
    fn bodies_have_the_retail_layout() {
        let m = derive("sample", &sample()).unwrap();
        let info = m.info_body();
        assert_eq!(info[0], 5);
        assert_eq!(u16::from_le_bytes([info[1], info[2]]), 6);
        assert_eq!(&info[3..9], b"sample");
        assert_eq!(&info[9..], &[0, 3, 0, 1]);
        let deps = m.deps_body().unwrap();
        assert_eq!(deps[0], 4);
        assert_eq!(deps.len(), 1 + 4 * 4);
        assert_eq!(&deps[1..5], &pandemic_hash_m2("MrxUtil").to_le_bytes());
    }

    #[test]
    fn a_script_without_imports_has_no_deps_chunk() {
        let main = proto(vec![abc(OP_RETURN, 0, 1, 0)], vec![], vec![]);
        let m = derive("plain", &chunk("plain", main)).unwrap();
        assert!(m.deps.is_empty());
        assert_eq!(m.deps_body(), None);
        assert_eq!(m.globals, 0);
    }

    #[test]
    fn a_call_with_two_arguments_or_a_variable_name_is_not_a_dep() {
        // import("a", false) and import(v) (v a global) are not literal one-argument calls.
        let main = proto(
            vec![
                abx(OP_GETGLOBAL, 0, 0),
                abx(OP_LOADK, 1, 1),
                abc(OP_LOADBOOL, 2, 0, 0),
                abc(OP_CALL, 0, 3, 1),
                abx(OP_GETGLOBAL, 0, 0),
                abx(OP_GETGLOBAL, 1, 2),
                abc(OP_CALL, 0, 2, 1),
                abc(OP_RETURN, 0, 1, 0),
            ],
            vec![s("import"), s("a"), s("v")],
            vec![],
        );
        assert!(derive("dyn", &chunk("dyn", main)).unwrap().deps.is_empty());
    }

    #[test]
    fn a_loop_over_unknown_bounds_is_an_error_not_a_guess() {
        // for i = 1, #t do ... end  with t unknown: the limit is LEN of an unknown, which the
        // model folds to 0, so make the bound a global instead.
        let main = proto(
            vec![
                abx(OP_LOADK, 0, 0),
                abx(OP_GETGLOBAL, 1, 1),
                abx(OP_LOADK, 2, 0),
                asbx(OP_FORPREP, 0, 0),
                asbx(OP_FORLOOP, 0, -1),
                abc(OP_RETURN, 0, 1, 0),
            ],
            vec![Constant::Number(1.0f32.to_bits()), s("n")],
            vec![],
        );
        let e = derive("loop", &chunk("loop", main)).unwrap_err();
        assert!(e.contains("numeric for loop") && e.contains("script loop"), "{e}");
    }

    #[test]
    fn names_are_checked() {
        assert!(derive("Sample", &sample()).unwrap_err().contains("lowercase"));
        assert!(derive("other", &sample()).unwrap_err().contains("source name"));
        assert!(derive("", &sample()).is_err());
    }
}
