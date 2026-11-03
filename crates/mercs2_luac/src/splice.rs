//! Run one chunk at the end of another, without touching the first's code.
//!
//! A `patch_lua` appends source to a script's source and compiles the two together. A script whose
//! source cannot be compiled has no such route: unluac decompiles a stripped chunk's control flow to
//! `goto`, which Lua 5.1 refuses, and that is 9 of the 12 retail scripts the DLC overrides. So
//! [`append_chunk`] takes both as bytecode. The appended chunk's main function becomes a new child of
//! the base's, and two instructions run it just before the base's final `RETURN`:
//!
//! ```text
//! CLOSURE  r, child
//! CALL     r, 1, 1
//! ```
//!
//! `OP_CLOSURE` gives a closure its creator's environment, so the appended code sees the module the base
//! runs in. `r` is one past the base's stack, so no live register moves. Instructions go in at the index
//! of the `RETURN`, so a jump to that `RETURN` lands on the `CLOSURE` and the appended code runs on that
//! path too. An early `return` skips it, as it skips source written after it.
//!
//! The base's locals are out of reach, as they are to a stripped chunk's reader anyway: they have no
//! names. A name the appended code uses that the base holds in a local resolves as a global.

use crate::{check_header, check_loads, CompileError, MERCS2_LUAQ_HEADER};

const OP_CALL: u32 = 28;
const OP_RETURN: u32 = 30;
const OP_CLOSURE: u32 = 36;
/// `MAXSTACK`, which `luaG_checkcode` holds a function to.
const MAX_STACK: u8 = 250;

fn bad(what: impl Into<String>) -> CompileError {
    CompileError::Internal(format!("a chunk to splice is malformed: {}", what.into()))
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn skip(&mut self, n: usize) -> Result<(), CompileError> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len()).ok_or_else(|| bad(format!("{n} bytes at {} run past the end", self.at)))?;
        self.at = end;
        Ok(())
    }

    fn int(&mut self) -> Result<usize, CompileError> {
        let at = self.at;
        self.skip(4)?;
        Ok(u32::from_le_bytes([self.b[at], self.b[at + 1], self.b[at + 2], self.b[at + 3]]) as usize)
    }

    fn string(&mut self) -> Result<(), CompileError> {
        let n = self.int()?;
        self.skip(n)
    }

    fn function(&mut self) -> Result<(), CompileError> {
        self.string()?; // source
        self.skip(8 + 4)?; // line defined, last line defined, four u8 fields
        let code = self.int()?;
        self.skip(code * 4)?;
        self.constants()?;
        for _ in 0..self.int()? {
            self.function()?;
        }
        let lines = self.int()?;
        self.skip(lines * 4)?;
        self.debug()
    }

    fn constants(&mut self) -> Result<(), CompileError> {
        for _ in 0..self.int()? {
            let at = self.at;
            self.skip(1)?;
            match self.b[at] {
                0 => {}
                1 => self.skip(1)?,
                3 => self.skip(4)?,
                4 => self.string()?,
                t => return Err(bad(format!("constant type {t}"))),
            }
        }
        Ok(())
    }

    /// Local variables and upvalue names, after `lineinfo`.
    fn debug(&mut self) -> Result<(), CompileError> {
        for _ in 0..self.int()? {
            self.string()?;
            self.skip(8)?;
        }
        for _ in 0..self.int()? {
            self.string()?;
        }
        Ok(())
    }
}

fn put_int(out: &mut Vec<u8>, v: usize) {
    out.extend_from_slice(&(v as u32).to_le_bytes());
}

/// Run `appended` at the end of `base`, both whole chunks in the game's dialect. See the module docs.
/// A stripped `base` has no source name, and takes `name`: a script container's own name must equal
/// its chunk's.
pub fn append_chunk(base: &[u8], appended: &[u8], name: &str) -> Result<Vec<u8>, CompileError> {
    check_header(base)?;
    check_header(appended)?;
    let head = MERCS2_LUAQ_HEADER.len();

    let mut r = Reader { b: base, at: head };
    r.string()?;
    let lines_at = r.at;
    r.skip(8)?;
    let flags_at = r.at;
    r.skip(4)?;
    let code_count_at = r.at;
    let code_count = r.int()?;
    let code_at = r.at;
    r.skip(code_count * 4)?;
    let constants_at = r.at;
    r.constants()?;
    let protos_count_at = r.at;
    let protos = r.int()?;
    for _ in 0..protos {
        r.function()?;
    }
    let protos_end = r.at;
    let line_count = r.int()?;
    let lines = r.at;
    r.skip(line_count * 4)?;
    let debug_at = r.at;
    r.debug()?;
    if r.at != base.len() {
        return Err(bad(format!("{} bytes follow the main function", base.len() - r.at)));
    }

    let word = |i: usize| {
        let o = code_at + i * 4;
        u32::from_le_bytes([base[o], base[o + 1], base[o + 2], base[o + 3]])
    };
    let last = code_count.checked_sub(1).ok_or_else(|| bad("the main function has no code"))?;
    if word(last) & 0x3F != OP_RETURN {
        return Err(bad("the main function does not end in RETURN"));
    }
    let stack = base[flags_at + 3];
    if stack >= MAX_STACK {
        return Err(CompileError::Internal(format!("the main function uses the whole stack ({stack} registers), so nothing can run after it")));
    }
    if protos >= 1 << 18 {
        return Err(CompileError::Internal(format!("the main function has {protos} functions, past CLOSURE's index")));
    }

    let r_a = u32::from(stack) << 6;
    let closure = OP_CLOSURE | r_a | (protos as u32) << 14;
    let call = OP_CALL | r_a | 1 << 23 | 1 << 14;

    let mut out = Vec::with_capacity(base.len() + appended.len() + 16);
    if lines_at == head + 4 {
        out.extend_from_slice(&base[..head]);
        put_int(&mut out, name.len() + 1);
        out.extend_from_slice(name.as_bytes());
        out.push(0);
    } else {
        out.extend_from_slice(&base[..lines_at]);
    }
    out.extend_from_slice(&base[lines_at..flags_at + 3]);
    out.push(stack + 1);
    debug_assert_eq!(flags_at + 4, code_count_at);
    put_int(&mut out, code_count + 2);
    out.extend_from_slice(&base[code_at..code_at + last * 4]);
    out.extend_from_slice(&closure.to_le_bytes());
    out.extend_from_slice(&call.to_le_bytes());
    out.extend_from_slice(&base[code_at + last * 4..constants_at]);
    out.extend_from_slice(&base[constants_at..protos_count_at]);
    put_int(&mut out, protos + 1);
    out.extend_from_slice(&base[protos_count_at + 4..protos_end]);
    out.extend_from_slice(&appended[head..]);
    if line_count == 0 {
        put_int(&mut out, 0);
    } else {
        if line_count != code_count {
            return Err(bad(format!("{line_count} line records for {code_count} instructions")));
        }
        let line = &base[lines + last * 4..lines + last * 4 + 4];
        put_int(&mut out, line_count + 2);
        out.extend_from_slice(&base[lines..lines + last * 4]);
        out.extend_from_slice(line);
        out.extend_from_slice(line);
        out.extend_from_slice(&base[lines + last * 4..debug_at]);
    }
    out.extend_from_slice(&moved_locals(base, debug_at, last)?);
    check_loads(&out)?;
    Ok(out)
}

/// The debug tail with each local's scope that reaches the moved `RETURN` still ending at it.
fn moved_locals(base: &[u8], at: usize, last: usize) -> Result<Vec<u8>, CompileError> {
    let mut r = Reader { b: base, at };
    let count = r.int()?;
    let mut out = base[at..r.at].to_vec();
    for _ in 0..count {
        let start = r.at;
        r.string()?;
        r.skip(4)?;
        out.extend_from_slice(&base[start..r.at]);
        let end_pc = r.int()?;
        put_int(&mut out, if end_pc >= last { end_pc + 2 } else { end_pc });
    }
    out.extend_from_slice(&base[r.at..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile;
    use crate::sys::{lua_close, lua_getfield, lua_pcall, lua_settop, lua_tonumber, luaL_loadbuffer, luaL_newstate, LUA_GLOBALSINDEX};
    use std::os::raw::c_char;

    /// Run a chunk in a fresh state and read the number global `name`.
    fn run(chunk: &[u8], name: &str) -> Result<f32, String> {
        let key = std::ffi::CString::new(name).unwrap();
        // SAFETY: a state of our own, closed before returning; the buffers outlive the calls.
        unsafe {
            let l = luaL_newstate();
            let status = luaL_loadbuffer(l, chunk.as_ptr() as *const c_char, chunk.len(), c"t".as_ptr());
            let status = if status == 0 { lua_pcall(l, 0, 0, 0) } else { status };
            if status != 0 {
                lua_close(l);
                return Err(format!("status {status}"));
            }
            lua_getfield(l, LUA_GLOBALSINDEX, key.as_ptr());
            let v = lua_tonumber(l, -1);
            lua_settop(l, 0);
            lua_close(l);
            Ok(v as f32)
        }
    }

    fn spliced(base: &str, appended: &str) -> Vec<u8> {
        append_chunk(&compile(base, "base").unwrap(), &compile(appended, "x").unwrap(), "base").unwrap()
    }

    #[test]
    fn the_appended_chunk_runs_after_the_base_and_sees_its_globals() {
        let chunk = spliced("x = 2", "y = x + 1");
        assert_eq!(run(&chunk, "x"), Ok(2.0));
        assert_eq!(run(&chunk, "y"), Ok(3.0));
    }

    #[test]
    fn the_base_keeps_its_functions_and_the_appended_chunk_can_wrap_them() {
        let chunk = spliced(
            "function f() return 1 end\nfunction g() return f() + 1 end",
            "local prev = f\nfunction f() return prev() + 10 end\ny = g()",
        );
        assert_eq!(run(&chunk, "y"), Ok(12.0));
    }

    #[test]
    fn it_runs_on_a_path_that_jumps_to_the_last_return() {
        let chunk = spliced("x = 1\nif x == 1 then y = 5 end", "z = y + 1");
        assert_eq!(run(&chunk, "z"), Ok(6.0));
    }

    #[test]
    fn the_bases_locals_stay_live_at_the_end_of_its_chunk() {
        let chunk = spliced("local a, b, c = 1, 2, 3\nfunction f() return a + b + c end", "y = f()");
        assert_eq!(run(&chunk, "y"), Ok(6.0));
    }

    #[test]
    fn a_stripped_base_splices_as_a_debug_base_does() {
        // A debug-less base: `lua_dump` with strip is not reachable from `compile`, so strip by hand.
        let chunk = compile("x = 2", "b").unwrap();
        let mut r = Reader { b: &chunk, at: 12 };
        r.string().unwrap();
        let after_source = r.at;
        r.skip(12).unwrap();
        let code = r.int().unwrap();
        r.skip(code * 4).unwrap();
        r.constants().unwrap();
        assert_eq!(r.int().unwrap(), 0, "no children");
        let body_end = r.at;
        let mut stripped = chunk[..12].to_vec();
        put_int(&mut stripped, 0);
        stripped.extend_from_slice(&chunk[after_source..body_end]);
        stripped.extend_from_slice(&[0u8; 12]);
        let out = append_chunk(&stripped, &compile("y = x + 1", "a").unwrap(), "mine").unwrap();
        assert_eq!(run(&out, "y"), Ok(3.0));
        assert_eq!(&out[12..16], &5u32.to_le_bytes(), "the source name takes the entry's");
        assert_eq!(&out[16..21], b"mine\0");
    }

    #[test]
    fn two_splices_run_in_order() {
        let once = spliced("x = 1", "x = x * 2");
        let twice = append_chunk(&once, &compile("x = x + 5", "t").unwrap(), "base").unwrap();
        assert_eq!(run(&twice, "x"), Ok(7.0));
    }

    #[test]
    fn a_chunk_of_another_dialect_or_truncated_is_refused() {
        let ok = compile("x = 1", "a").unwrap();
        assert!(append_chunk(b"not lua", &ok, "a").is_err());
        assert!(append_chunk(&ok[..ok.len() - 3], &ok, "a").is_err());
        let mut extra = ok.clone();
        extra.push(0);
        assert!(append_chunk(&extra, &ok, "a").is_err());
    }
}
