//! The retail game's running Lua functions read out of its memory: a `Proto` tree as
//! [`luachunk::Proto`](crate::luachunk::Proto) (what `lua_debug::stamp` hashes), a function's
//! [`FunctionKey`], and the Lua frames on a state's call stack.
//!
//! Lua 5.1.2, 32-bit, float `lua_Number`. The `Proto` offsets are proven from pc-v10's own chunk loader,
//! which writes each field it reads: LoadFunction `0x00870F00` (`source` +0x20, `linedefined` +0x3C,
//! `lastlinedefined` +0x40, `nups`/`numparams`/`is_vararg`/`maxstacksize` +0x48..+0x4B, and
//! `luaG_checkcode(p, p->sizecode)` reading +0x2C), LoadCode `0x008707F0` (`code` +0x0C, `sizecode`
//! +0x2C), LoadConstants `0x008708D0` (`k` +0x08 as 8-byte TValues, `sizek` +0x28, nested `p` +0x10,
//! `sizep` +0x34). `lineinfo` +0x14 and `sizelineinfo` +0x30 are what traceexec `0x00865BA0` reads.
//! The call stack: `luaD_callhook` `0x00868290` computes a frame's index as `(ci - L->base_ci) / 24`;
//! a frame's function is the TValue at `ci->func` (+0x04); its saved pc is `ci->savedpc` (+0x0C), and
//! the running frame's is `L->savedpc` (+0x18), which traceexec stores one instruction past the one
//! executing. A Lua closure's `Proto` is at +0x10, and its `isC` byte at +0x06.
//!
//! Every read goes through [`Mem`]; an unreadable or implausible structure is an error naming the
//! address, never a fault.

use crate::lua_linemap::FunctionKey;
use crate::lua_stack::{Mem, LUA_TFUNCTION, OFF_BASE_CI, OFF_CI, TSTRING_DATA, TSTRING_LEN};
use crate::luachunk::{Constant, Proto};

pub const PROTO_K: u32 = 0x08;
pub const PROTO_CODE: u32 = 0x0C;
pub const PROTO_P: u32 = 0x10;
pub const PROTO_LINEINFO: u32 = 0x14;
pub const PROTO_SOURCE: u32 = 0x20;
pub const PROTO_SIZEK: u32 = 0x28;
pub const PROTO_SIZECODE: u32 = 0x2C;
pub const PROTO_SIZELINEINFO: u32 = 0x30;
pub const PROTO_SIZEP: u32 = 0x34;
pub const PROTO_LINEDEFINED: u32 = 0x3C;
pub const PROTO_LASTLINEDEFINED: u32 = 0x40;
pub const PROTO_NUPS: u32 = 0x48;
/// `nups`, `numparams`, `is_vararg`, `maxstacksize`: the last of the four bytes.
pub const PROTO_SIZE: u32 = 0x4C;

pub const CI_FUNC: u32 = 0x04;
pub const CI_SAVEDPC: u32 = 0x0C;
pub const CI_SIZE: u32 = 0x18;
/// `L->savedpc`, the running frame's.
pub const OFF_SAVEDPC: u32 = 0x18;
pub const CLOSURE_ISC: u32 = 0x06;
pub const CLOSURE_PROTO: u32 = 0x10;

/// More instructions, constants or nested functions than any shipped function has by orders of
/// magnitude: a count past it means the address is not a `Proto`.
const MAX_COUNT: u32 = 1 << 20;
/// Deeper than any call stack the game builds (`LUAI_MAXCALLS` is 20000; the corpus nests far less).
const MAX_FRAMES: u32 = 20_000;

fn need<M: Mem>(m: &M, addr: u32, len: u32, what: &str) -> Result<(), String> {
    if len == 0 || (addr != 0 && m.readable_span(addr) >= len) {
        Ok(())
    } else {
        Err(format!("{what} at {addr:#x} ({len} bytes) is not readable"))
    }
}

fn count<M: Mem>(m: &M, p: u32, off: u32, what: &str) -> Result<u32, String> {
    let n = m.read_u32(p + off);
    if n > MAX_COUNT {
        return Err(format!("Proto {p:#x}: {what} {n} is implausible"));
    }
    Ok(n)
}

/// A `TString`'s bytes.
pub fn read_tstring<M: Mem>(m: &M, ts: u32) -> Result<Vec<u8>, String> {
    need(m, ts, TSTRING_DATA, "TString")?;
    let len = m.read_u32(ts + TSTRING_LEN);
    if len > MAX_COUNT {
        return Err(format!("TString {ts:#x}: length {len} is implausible"));
    }
    need(m, ts + TSTRING_DATA, len, "TString data")?;
    Ok((0..len).map(|i| m.read_u8(ts + TSTRING_DATA + i)).collect())
}

/// The function's key: its line-defined fields and instruction count.
pub fn function_key<M: Mem>(m: &M, p: u32) -> Result<FunctionKey, String> {
    need(m, p, PROTO_SIZE, "Proto")?;
    Ok(FunctionKey {
        line_defined: m.read_u32(p + PROTO_LINEDEFINED),
        last_line_defined: m.read_u32(p + PROTO_LASTLINEDEFINED),
        code_len: count(m, p, PROTO_SIZECODE, "sizecode")?,
    })
}

/// The chunk name the function was loaded under (`Proto.source`).
pub fn source<M: Mem>(m: &M, p: u32) -> Result<Vec<u8>, String> {
    need(m, p, PROTO_SIZE, "Proto")?;
    read_tstring(m, m.read_u32(p + PROTO_SOURCE))
}

fn words<M: Mem>(m: &M, at: u32, n: u32, what: &str) -> Result<Vec<u32>, String> {
    need(m, at, n * 4, what)?;
    Ok((0..n).map(|i| m.read_u32(at + i * 4)).collect())
}

/// The whole function tree at `p`, as the loader built it: everything [`crate::lua_debug::stamp`] reads,
/// plus `source`, the line-defined fields and the line info. Local and upvalue names are not read.
pub fn read_proto<M: Mem>(m: &M, p: u32) -> Result<Proto, String> {
    read_at(m, p, 0)
}

fn read_at<M: Mem>(m: &M, p: u32, depth: u32) -> Result<Proto, String> {
    if depth > 200 {
        return Err(format!("Proto {p:#x}: nested over 200 deep"));
    }
    let key = function_key(m, p)?;
    let sizek = count(m, p, PROTO_SIZEK, "sizek")?;
    let sizep = count(m, p, PROTO_SIZEP, "sizep")?;
    let sizeline = count(m, p, PROTO_SIZELINEINFO, "sizelineinfo")?;
    let code = words(m, m.read_u32(p + PROTO_CODE), key.code_len, "code")?;
    let k = m.read_u32(p + PROTO_K);
    need(m, k, sizek * 8, "constants")?;
    let mut constants = Vec::with_capacity(sizek as usize);
    for i in 0..sizek {
        let (value, tt) = (m.read_u32(k + i * 8), m.read_u32(k + i * 8 + 4));
        constants.push(match tt {
            0 => Constant::Nil,
            // LoadConstants stores the chunk's byte sign-extended: any non-zero byte is true.
            1 => Constant::Boolean(value != 0),
            3 => Constant::Number(value),
            4 => Constant::String(read_tstring(m, value)?),
            other => return Err(format!("Proto {p:#x}: constant {i} has type tag {other}")),
        });
    }
    let line_info = if sizeline == 0 { Vec::new() } else { words(m, m.read_u32(p + PROTO_LINEINFO), sizeline, "lineinfo")? };
    let children = words(m, m.read_u32(p + PROTO_P), sizep, "nested functions")?;
    let protos = children.into_iter().map(|c| read_at(m, c, depth + 1)).collect::<Result<Vec<_>, _>>()?;
    Ok(Proto {
        source: Some(source(m, p)?),
        line_defined: key.line_defined,
        last_line_defined: key.last_line_defined,
        nups: m.read_u8(p + PROTO_NUPS),
        num_params: m.read_u8(p + PROTO_NUPS + 1),
        is_vararg: m.read_u8(p + PROTO_NUPS + 2),
        max_stack: m.read_u8(p + PROTO_NUPS + 3),
        code,
        constants,
        protos,
        line_info,
        loc_vars: Vec::new(),
        upvalues: Vec::new(),
    })
}

/// One Lua frame on the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The frame's index in the call-info stack (`ci - base_ci`), what the hook reports as `i_ci`.
    pub depth: u32,
    pub proto: u32,
    /// 0-based index of the instruction the frame is executing (for a frame below the top, its call).
    pub pc: u32,
}

/// The `Proto` of the function in the frame at `ci`, or `None` for a C function or a non-function.
pub fn frame_proto<M: Mem>(m: &M, ci: u32) -> Result<Option<u32>, String> {
    need(m, ci, CI_SIZE, "CallInfo")?;
    let func = m.read_u32(ci + CI_FUNC);
    need(m, func, 8, "CallInfo func")?;
    if m.read_u32(func + 4) != LUA_TFUNCTION {
        return Ok(None);
    }
    let cl = m.read_u32(func);
    need(m, cl, CLOSURE_PROTO + 4, "closure")?;
    if m.read_u8(cl + CLOSURE_ISC) != 0 {
        return Ok(None);
    }
    Ok(Some(m.read_u32(cl + CLOSURE_PROTO)))
}

/// The instruction index a saved pc names: one instruction before it, as traceexec computes.
pub fn pc_index<M: Mem>(m: &M, proto: u32, savedpc: u32) -> Result<u32, String> {
    need(m, proto, PROTO_SIZE, "Proto")?;
    let code = m.read_u32(proto + PROTO_CODE);
    let len = m.read_u32(proto + PROTO_SIZECODE);
    if savedpc <= code || !(savedpc - code).is_multiple_of(4) || (savedpc - code) / 4 > len {
        return Err(format!("saved pc {savedpc:#x} is not inside Proto {proto:#x}'s code ({code:#x}, {len} instructions)"));
    }
    Ok((savedpc - code) / 4 - 1)
}

/// Every Lua frame of state `l`, base first. The running frame's pc comes from `L->savedpc`, the others'
/// from their `ci->savedpc`.
pub fn lua_frames<M: Mem>(m: &M, l: u32) -> Result<Vec<Frame>, String> {
    need(m, l, OFF_SAVEDPC + 4, "lua_State")?;
    let (base, top) = (m.read_u32(l + OFF_BASE_CI), m.read_u32(l + OFF_CI));
    if top < base || (top - base) % CI_SIZE != 0 || (top - base) / CI_SIZE > MAX_FRAMES {
        return Err(format!("lua_State {l:#x}: ci {top:#x} is not a frame above base_ci {base:#x}"));
    }
    let mut out = Vec::new();
    for depth in 0..=(top - base) / CI_SIZE {
        let ci = base + depth * CI_SIZE;
        let Some(proto) = frame_proto(m, ci)? else { continue };
        let savedpc = if ci == top { m.read_u32(l + OFF_SAVEDPC) } else { m.read_u32(ci + CI_SAVEDPC) };
        out.push(Frame { depth, proto, pc: pc_index(m, proto, savedpc)? });
    }
    Ok(out)
}

/// Retail memory laid out the way the game builds it, for tests.
#[cfg(test)]
pub(crate) mod layout {
    use super::*;
    use crate::lua_stack::fake::FakeMem;

    /// Map `bytes` at `*next` (leaving a gap after them) and return where.
    pub fn alloc(m: &mut FakeMem, next: &mut u32, bytes: Vec<u8>) -> u32 {
        let at = *next;
        *next += (bytes.len() as u32 + 0x10 + 0xF) & !0xF;
        m.map(at, bytes);
        at
    }

    pub fn tstring(m: &mut FakeMem, next: &mut u32, s: &[u8]) -> u32 {
        let mut b = vec![0u8; TSTRING_DATA as usize];
        b[TSTRING_LEN as usize..TSTRING_LEN as usize + 4].copy_from_slice(&(s.len() as u32).to_le_bytes());
        b.extend_from_slice(s);
        b.push(0);
        alloc(m, next, b)
    }

    pub fn le(w: &[u32]) -> Vec<u8> {
        w.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// A closure: `isC` and its `Proto` (Lua) or function (C) at +0x10.
    pub fn closure(m: &mut FakeMem, next: &mut u32, is_c: bool, target: u32) -> u32 {
        let mut b = vec![0u8; (CLOSURE_PROTO + 4) as usize];
        b[CLOSURE_ISC as usize] = u8::from(is_c);
        b[CLOSURE_PROTO as usize..].copy_from_slice(&target.to_le_bytes());
        alloc(m, next, b)
    }

    /// Lay `p` out in `m` the way the retail loader does, from `*next` upward; returns its address.
    pub fn lay(m: &mut FakeMem, next: &mut u32, p: &Proto) -> u32 {
        let code = alloc(m, next, le(&p.code));
        let lines = alloc(m, next, le(&p.line_info));
        let mut k = Vec::new();
        for c in &p.constants {
            let (v, tt) = match c {
                Constant::Nil => (0, 0),
                // A `true` the loader read as byte 0x01 sign-extends to 1; 0xFF would be 0xFFFFFFFF.
                Constant::Boolean(b) => (if *b { 0xFFFF_FFFF } else { 0 }, 1),
                Constant::Number(bits) => (*bits, 3),
                Constant::String(s) => (tstring(m, next, s), 4),
            };
            k.extend_from_slice(&[v, tt]);
        }
        let k = alloc(m, next, le(&k));
        let children: Vec<u32> = p.protos.iter().map(|c| lay(m, next, c)).collect();
        let children = alloc(m, next, le(&children));
        let source = tstring(m, next, p.source.as_deref().unwrap_or(b"=?"));
        let mut b = vec![0u8; PROTO_SIZE as usize];
        let mut put = |o: u32, v: u32| b[o as usize..o as usize + 4].copy_from_slice(&v.to_le_bytes());
        put(PROTO_K, k);
        put(PROTO_CODE, code);
        put(PROTO_P, children);
        put(PROTO_LINEINFO, lines);
        put(PROTO_SOURCE, source);
        put(PROTO_SIZEK, p.constants.len() as u32);
        put(PROTO_SIZECODE, p.code.len() as u32);
        put(PROTO_SIZELINEINFO, p.line_info.len() as u32);
        put(PROTO_SIZEP, p.protos.len() as u32);
        put(PROTO_LINEDEFINED, p.line_defined);
        put(PROTO_LASTLINEDEFINED, p.last_line_defined);
        b[PROTO_NUPS as usize..PROTO_SIZE as usize].copy_from_slice(&[p.nups, p.num_params, p.is_vararg, p.max_stack]);
        alloc(m, next, b)
    }
}

#[cfg(test)]
mod tests {
    use super::layout::*;
    use super::*;
    use crate::lua_debug::stamp;
    use crate::lua_stack::fake::FakeMem;

    fn sample() -> Proto {
        let child = Proto {
            source: Some(b"@gurcon002".to_vec()),
            line_defined: 12,
            last_line_defined: 20,
            nups: 1,
            num_params: 2,
            is_vararg: 0,
            max_stack: 5,
            code: vec![0x0000_0001, 0x0080_001E],
            constants: vec![Constant::Number(0x3F80_0000), Constant::Boolean(false)],
            protos: Vec::new(),
            line_info: vec![13, 14],
            loc_vars: Vec::new(),
            upvalues: Vec::new(),
        };
        Proto {
            source: Some(b"@gurcon002".to_vec()),
            line_defined: 0,
            last_line_defined: 0,
            nups: 0,
            num_params: 0,
            is_vararg: 2,
            max_stack: 2,
            code: vec![0x0000_0024, 0x0000_0007, 0x0080_001E],
            constants: vec![Constant::String(b"Start\xFC".to_vec()), Constant::Nil, Constant::Boolean(true)],
            protos: vec![child],
            line_info: vec![1, 1, 21],
            loc_vars: Vec::new(),
            upvalues: Vec::new(),
        }
    }

    #[test]
    fn a_tree_laid_out_as_the_loader_does_reads_back_with_its_stamp() {
        let (mut m, mut next) = (FakeMem::default(), 0x0010_0000);
        let p = sample();
        let at = lay(&mut m, &mut next, &p);
        let back = read_proto(&m, at).unwrap();
        assert_eq!(back, p);
        assert_eq!(stamp(&back), stamp(&p));
        assert_eq!(function_key(&m, at).unwrap(), FunctionKey { line_defined: 0, last_line_defined: 0, code_len: 3 });
        assert_eq!(source(&m, at).unwrap(), b"@gurcon002");
    }

    #[test]
    fn a_bad_tree_is_an_error_not_a_fault() {
        let (mut m, mut next) = (FakeMem::default(), 0x0010_0000);
        let at = lay(&mut m, &mut next, &sample());
        assert!(read_proto(&m, 0x0900_0000).unwrap_err().contains("not readable"));
        // Point `code` at nothing.
        let mut broken = FakeMem::default();
        let mut n2 = 0x0010_0000;
        let at2 = lay(&mut broken, &mut n2, &sample());
        let mut bytes: Vec<u8> = (0..PROTO_SIZE).map(|i| broken.read_u8(at2 + i)).collect();
        bytes[PROTO_CODE as usize..PROTO_CODE as usize + 4].copy_from_slice(&0x0800_0000u32.to_le_bytes());
        broken.map(at2, bytes);
        assert!(read_proto(&broken, at2).unwrap_err().contains("code"));
        let _ = at;
    }

    /// A state whose call stack is: base frame (C), a Lua frame of `outer` called at instruction 1, and
    /// the running Lua frame of `inner` executing instruction 1.
    #[test]
    fn the_frames_of_a_state_name_their_function_and_instruction() {
        let (mut m, mut next) = (FakeMem::default(), 0x0010_0000);
        let p = sample();
        let outer = lay(&mut m, &mut next, &p);
        let inner = lay(&mut m, &mut next, &p.protos[0]);
        let code = |m: &FakeMem, p: u32| m.read_u32(p + PROTO_CODE);
        let (l, cis, stack) = (0x0050_0000u32, 0x0060_0000u32, 0x0070_0000u32);
        // Closures: a C one, then Lua ones for outer and inner.
        let closure = |isc: u8, proto: u32| {
            let mut b = vec![0u8; (CLOSURE_PROTO + 4) as usize];
            b[CLOSURE_ISC as usize] = isc;
            b[CLOSURE_PROTO as usize..].copy_from_slice(&proto.to_le_bytes());
            b
        };
        m.map(0x0080_0000, closure(1, 0));
        m.map(0x0080_1000, closure(0, outer));
        m.map(0x0080_2000, closure(0, inner));
        let mut s = Vec::new();
        for cl in [0x0080_0000u32, 0x0080_1000, 0x0080_2000] {
            s.extend_from_slice(&cl.to_le_bytes());
            s.extend_from_slice(&LUA_TFUNCTION.to_le_bytes());
        }
        m.map(stack, s);
        let mut ci = vec![0u8; 3 * CI_SIZE as usize];
        for d in 0..3u32 {
            let o = (d * CI_SIZE) as usize;
            ci[o + CI_FUNC as usize..o + CI_FUNC as usize + 4].copy_from_slice(&(stack + d * 8).to_le_bytes());
        }
        let outer_saved = code(&m, outer) + 2 * 4;
        ci[(CI_SIZE + CI_SAVEDPC) as usize..(CI_SIZE + CI_SAVEDPC) as usize + 4].copy_from_slice(&outer_saved.to_le_bytes());
        m.map(cis, ci);
        let mut state = vec![0u8; 0x40];
        state[OFF_CI as usize..OFF_CI as usize + 4].copy_from_slice(&(cis + 2 * CI_SIZE).to_le_bytes());
        state[OFF_BASE_CI as usize..OFF_BASE_CI as usize + 4].copy_from_slice(&cis.to_le_bytes());
        state[OFF_SAVEDPC as usize..OFF_SAVEDPC as usize + 4].copy_from_slice(&(code(&m, inner) + 2 * 4).to_le_bytes());
        m.map(l, state);
        assert_eq!(
            lua_frames(&m, l).unwrap(),
            vec![Frame { depth: 1, proto: outer, pc: 1 }, Frame { depth: 2, proto: inner, pc: 1 }]
        );
    }

    #[test]
    fn a_saved_pc_outside_the_code_is_an_error() {
        let (mut m, mut next) = (FakeMem::default(), 0x0010_0000);
        let p = lay(&mut m, &mut next, &sample());
        let code = m.read_u32(p + PROTO_CODE);
        assert_eq!(pc_index(&m, p, code + 4).unwrap(), 0);
        assert_eq!(pc_index(&m, p, code + 12).unwrap(), 2);
        assert!(pc_index(&m, p, code).is_err());
        assert!(pc_index(&m, p, code + 16).is_err());
        assert!(pc_index(&m, p, code + 6).is_err());
    }
}
