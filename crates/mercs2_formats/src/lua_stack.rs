//! Argument reads off the game's Lua 5.1.2 stack (32-bit, float `lua_Number`), behind m2-sdk.dll's
//! `m2_lua_nargs`, `m2_lua_arg_*` and `m2_lua_join_strings` (`m2/m2_luastack.h`), and the stack-index
//! checks behind `m2_lua_ref` and `m2_lua_pcall` (`m2/m2_lua.h`).
//!
//! TValue is 8 bytes `{u32 value; u32 tt}`. Every dereference goes through [`Mem`], which the DLL backs
//! with `VirtualQuery`, so a caller that is not a Lua C function reads as an invalid stack, not a fault.

pub const OFF_TOP: u32 = 0x08;
pub const OFF_BASE: u32 = 0x0C;
pub const OFF_CI: u32 = 0x14;
pub const OFF_STACK_LAST: u32 = 0x1C;
pub const OFF_STACK: u32 = 0x20;
/// `L->base_ci`: `L->ci` equals it while no function runs on the state.
pub const OFF_BASE_CI: u32 = 0x28;
pub const CI_OFF_BASE: u32 = 0x00;
pub const CI_OFF_FUNC: u32 = 0x04;
/// `ci->top`, the highest slot the running function may fill: `lua_pcall` (pc-v10 0x0085DF50) raises
/// `[[L+0x14]+8]` to `L->top` after a `LUA_MULTRET` call (`adjustresults`).
pub const CI_OFF_TOP: u32 = 0x08;
/// `LUA_MULTRET`: a call that keeps every result.
pub const LUA_MULTRET: i32 = -1;

pub const TVALUE_SIZE: u32 = 8;
pub const TVALUE_TT: u32 = 4;
/// String bytes start at TString + 16.
pub const TSTRING_DATA: u32 = 16;
/// The string's length (`luaS_newlstr` compares `[ts+0xC]` with the length it is asked for).
pub const TSTRING_LEN: u32 = 0x0C;
/// `lua_State.l_G`, the global state.
pub const OFF_GLOBAL: u32 = 0x10;
pub const MAX_ARGS: u32 = 32;

pub const LUA_TNIL: u32 = 0;
pub const LUA_TBOOLEAN: u32 = 1;
pub const LUA_TLIGHTUSERDATA: u32 = 2;
pub const LUA_TNUMBER: u32 = 3;
/// A full userdata; its payload starts [`UDATA_PAYLOAD`] bytes into the `Udata`.
pub const LUA_TUSERDATA: u32 = 7;
pub const UDATA_PAYLOAD: u32 = 0x18;
pub const LUA_TSTRING: u32 = 4;
pub const LUA_TTABLE: u32 = 5;
pub const LUA_TFUNCTION: u32 = 6;
/// A `lua_State`'s own type tag, at `+4` (`tt` of its GC header).
pub const LUA_TTHREAD: u32 = 8;
/// `LUA_REGISTRYINDEX`; the pseudo-indices are this and below (`cmp ebp, 0xFFFFD8F0` in `luaL_ref`).
pub const LUA_REGISTRYINDEX: i32 = -10000;

/// Process memory as the readers see it.
pub trait Mem {
    /// Bytes readable from `addr` to the end of its committed, readable region; 0 when none are.
    fn readable_span(&self, addr: u32) -> u32;
    fn read_u32(&self, addr: u32) -> u32;
    fn read_u8(&self, addr: u32) -> u8;
}

fn readable<M: Mem>(m: &M, addr: u32, n: u32) -> bool {
    n != 0 && addr != 0 && m.readable_span(addr) >= n
}

fn in_stack(p: u32, stack: u32, last: u32) -> bool {
    p != 0 && stack != 0 && last != 0 && stack <= last && p >= stack && p <= last
}

/// The frame of a Lua C function: its first argument and how many there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub base: u32,
    pub nargs: u32,
}

/// `base` falls back to `ci->func + 1`, then `ci->base`, while `L->base` is out of the stack.
pub fn resolve<M: Mem>(m: &M, l: u32) -> Option<Frame> {
    if l < 0x10000 || l & 3 != 0 {
        return None;
    }
    if !readable(m, l + OFF_STACK, 12) {
        return None;
    }
    let top = m.read_u32(l + OFF_TOP);
    let mut base = m.read_u32(l + OFF_BASE);
    let last = m.read_u32(l + OFF_STACK_LAST);
    let stack = m.read_u32(l + OFF_STACK);
    if !in_stack(stack, stack, last) || !in_stack(top, stack, last) {
        return None;
    }
    if !in_stack(base, stack, last) && readable(m, l + OFF_CI, 4) {
        let ci = m.read_u32(l + OFF_CI);
        if ci != 0 && readable(m, ci + CI_OFF_FUNC, 4) {
            let func = m.read_u32(ci + CI_OFF_FUNC);
            if in_stack(func, stack, last) {
                base = func.wrapping_add(TVALUE_SIZE);
            } else if readable(m, ci + CI_OFF_BASE, 4) {
                let cb = m.read_u32(ci + CI_OFF_BASE);
                if in_stack(cb, stack, last) {
                    base = cb;
                }
            }
        }
    }
    if !in_stack(base, stack, last) || top < base {
        return None;
    }
    if base & 3 != 0 || top & 3 != 0 {
        return None;
    }
    if !(base - stack).is_multiple_of(TVALUE_SIZE) || !(top - stack).is_multiple_of(TVALUE_SIZE) {
        return None;
    }
    let nargs = (top - base) / TVALUE_SIZE;
    if nargs == 0 || nargs > MAX_ARGS || !readable(m, base, nargs * TVALUE_SIZE) {
        return None;
    }
    Some(Frame { base, nargs })
}

fn arg_at<M: Mem>(m: &M, f: Frame, idx0: i32) -> Option<(u32, u32)> {
    let i = u32::try_from(idx0).ok().filter(|i| *i < f.nargs)?;
    let at = f.base + i * TVALUE_SIZE;
    Some((m.read_u32(at), m.read_u32(at + TVALUE_TT)))
}

/// Copy a TString's bytes into `out` (NUL-terminated), up to its first NUL, `out.len() - 1` bytes, or the
/// end of its readable region. Returns the bytes copied.
pub fn copy_tstring<M: Mem>(m: &M, ts: u32, out: &mut [u8]) -> usize {
    if ts == 0 || out.len() <= 1 {
        return 0;
    }
    if m.readable_span(ts) < TSTRING_DATA + 4 {
        return 0;
    }
    let data = ts + TSTRING_DATA;
    let span = m.readable_span(data) as usize;
    if span == 0 {
        return 0;
    }
    let limit = (out.len() - 1).min(span);
    let mut n = 0;
    while n < limit {
        let b = m.read_u8(data + n as u32);
        if b == 0 {
            break;
        }
        out[n] = b;
        n += 1;
    }
    out[n] = 0;
    n
}

/// `m2_lua_nargs`: the argument count, or -1 for an invalid stack (zero arguments included).
pub fn nargs<M: Mem>(m: &M, l: u32) -> i32 {
    resolve(m, l).map_or(-1, |f| f.nargs as i32)
}

/// `m2_lua_arg_type`: the type tag, or -1.
pub fn arg_type<M: Mem>(m: &M, l: u32, idx0: i32) -> i32 {
    resolve(m, l).and_then(|f| arg_at(m, f, idx0)).map_or(-1, |(_, tt)| tt as i32)
}

/// `m2_lua_arg_number`: the float bits widened to f64, when the argument is a number.
pub fn arg_number<M: Mem>(m: &M, l: u32, idx0: i32) -> Option<f64> {
    let (v, tt) = resolve(m, l).and_then(|f| arg_at(m, f, idx0))?;
    (tt == LUA_TNUMBER).then(|| f64::from(f32::from_bits(v)))
}

/// `m2_lua_arg_bool`: `Some(truthiness)` when the argument exists; only nil and false are false.
pub fn arg_bool<M: Mem>(m: &M, l: u32, idx0: i32) -> Option<bool> {
    let (v, tt) = resolve(m, l).and_then(|f| arg_at(m, f, idx0))?;
    Some(match tt {
        LUA_TNIL => false,
        LUA_TBOOLEAN => v != 0,
        _ => true,
    })
}

/// `m2_lua_arg_object`: an object argument as the game's own bindings read it (PC v1.1 `FUN_0059FF40`): a
/// light userdata's value (the object's guid), or a full userdata's payload address; `None` for any other type.
pub fn arg_object<M: Mem>(m: &M, l: u32, idx0: i32) -> Option<u32> {
    match resolve(m, l).and_then(|f| arg_at(m, f, idx0))? {
        (v, LUA_TLIGHTUSERDATA) => Some(v),
        (v, LUA_TUSERDATA) => Some(v.wrapping_add(UDATA_PAYLOAD)),
        _ => None,
    }
}

/// `m2_lua_arg_string`: copies a string argument into `out`; returns its length, 0 otherwise.
pub fn arg_string<M: Mem>(m: &M, l: u32, idx0: i32, out: &mut [u8]) -> i32 {
    if let Some(first) = out.first_mut() {
        *first = 0;
    }
    match resolve(m, l).and_then(|f| arg_at(m, f, idx0)) {
        Some((v, LUA_TSTRING)) => copy_tstring(m, v, out) as i32,
        _ => 0,
    }
}

/// `m2_lua_join_strings`: every string argument, tab-separated, into `out`. Returns how many were joined,
/// or -1 for an invalid stack.
pub fn join_strings<M: Mem>(m: &M, l: u32, out: &mut [u8]) -> i32 {
    let out_max = out.len() as i32;
    if let Some(first) = out.first_mut() {
        *first = 0;
    }
    let Some(f) = resolve(m, l) else { return -1 };
    let mut pos: i32 = 0;
    let mut count = 0;
    for i in 0..f.nargs {
        if pos >= out_max - 2 {
            break;
        }
        let (v, tt) = arg_at(m, f, i as i32).expect("index below nargs");
        if tt != LUA_TSTRING {
            continue;
        }
        if count > 0 && pos < out_max - 1 {
            out[pos as usize] = b'\t';
            pos += 1;
        }
        pos += copy_tstring(m, v, &mut out[pos as usize..]) as i32;
        count += 1;
    }
    if out_max > 0 {
        out[pos.min(out_max - 1) as usize] = 0;
    }
    count
}

/// `L->top` and `L->base`, when both are readable, aligned and ordered.
fn top_base<M: Mem>(m: &M, l: u32) -> Result<(u32, u32), String> {
    if l < 0x10000 || l & 3 != 0 || !readable(m, l + OFF_TOP, 8) {
        return Err(format!("lua_State {l:#x} is not readable"));
    }
    let (top, base) = (m.read_u32(l + OFF_TOP), m.read_u32(l + OFF_BASE));
    if top < base || !(top - base).is_multiple_of(TVALUE_SIZE) {
        return Err(format!("lua_State {l:#x} has top {top:#x} and base {base:#x}, not a frame"));
    }
    Ok((top, base))
}

/// The address of the stack slot that the Lua 5.1 stack index `idx` names, as `index2adr` resolves a
/// real index: `idx > 0` counts from `L->base` (1 is the first value of the running function's frame),
/// `idx < 0` from `L->top` (-1 is the top value). 0, a pseudo-index ([`LUA_REGISTRYINDEX`] and below)
/// and an index outside `base..top` name no slot and are errors.
pub fn stack_slot<M: Mem>(m: &M, l: u32, idx: i32) -> Result<u32, String> {
    let (top, base) = top_base(m, l)?;
    let depth = (top - base) / TVALUE_SIZE;
    if idx > 0 {
        let i = idx as u32 - 1;
        if i >= depth {
            return Err(format!("stack index {idx} is above the top of a frame of {depth} value(s)"));
        }
        Ok(base + i * TVALUE_SIZE)
    } else if idx < 0 && idx > LUA_REGISTRYINDEX {
        let back = idx.unsigned_abs();
        if back > depth {
            return Err(format!("stack index {idx} is below the base of a frame of {depth} value(s)"));
        }
        Ok(top - back * TVALUE_SIZE)
    } else if idx == 0 {
        Err("stack index 0 names no value".into())
    } else {
        Err(format!("pseudo-index {idx} is not a stack slot"))
    }
}

/// How many values the running function's frame holds (`L->top - L->base`).
pub fn frame_depth<M: Mem>(m: &M, l: u32) -> Result<u32, String> {
    top_base(m, l).map(|(top, base)| (top - base) / TVALUE_SIZE)
}

/// Whether `n` more values fit on the stack without growing it: `lua_checkstack`'s test,
/// `L->stack_last - L->top > n` slots.
pub fn has_room<M: Mem>(m: &M, l: u32, n: u32) -> Result<bool, String> {
    let (top, _) = top_base(m, l)?;
    if !readable(m, l + OFF_STACK_LAST, 4) {
        return Err(format!("lua_State {l:#x} is not readable"));
    }
    let last = m.read_u32(l + OFF_STACK_LAST);
    Ok(last > top && (last - top) / TVALUE_SIZE > n)
}

/// The slot of the function a `lua_pcall(L, nargs, nresults)` would call, when the call is one Lua 5.1
/// allows: `nargs >= 0`, the frame holds the function and its `nargs` arguments, and `nresults` is
/// [`LUA_MULTRET`] or passes Lua's `checkresults` (`ci->top - top >= nresults - nargs` in slots, lapi.c;
/// compiled out of the game). `luaD_poscall` (pc-v10 0x00868850) writes `ci->nresults` values from the
/// function's slot up and fills with nil past the results it got, so a larger `nresults` writes past the
/// stack. `ci->top` is also the slot the garbage collector keeps when it shrinks the stack during the
/// call; the results must also end at or below `L->stack_last`. `LUA_MULTRET` is allowed: its results are
/// the callee's own, copied down.
pub fn pcall_callee<M: Mem>(m: &M, l: u32, nargs: i32, nresults: i32) -> Result<u32, String> {
    if nargs < 0 {
        return Err(format!("nargs {nargs} is negative"));
    }
    if nresults < LUA_MULTRET {
        return Err(format!("nresults {nresults} is below LUA_MULTRET (-1)"));
    }
    let (top, base) = top_base(m, l)?;
    let depth = (top - base) / TVALUE_SIZE;
    if depth < nargs as u32 + 1 {
        return Err(format!("the frame holds {depth} value(s), fewer than the function and its {nargs} argument(s)"));
    }
    let callee = top - (nargs as u32 + 1) * TVALUE_SIZE;
    if nresults == LUA_MULTRET {
        return Ok(callee);
    }
    if !readable(m, l + OFF_CI, 4) || !readable(m, l + OFF_STACK_LAST, 4) {
        return Err(format!("lua_State {l:#x} is not readable"));
    }
    let ci = m.read_u32(l + OFF_CI);
    if !readable(m, ci + CI_OFF_TOP, 4) {
        return Err(format!("lua_State {l:#x} has an unreadable CallInfo {ci:#x}"));
    }
    let (ci_top, last) = (m.read_u32(ci + CI_OFF_TOP), m.read_u32(l + OFF_STACK_LAST));
    let slots = |a: u32, b: u32| (i64::from(a) - i64::from(b)) / i64::from(TVALUE_SIZE);
    let fits_ci = slots(ci_top, top) >= i64::from(nresults) - i64::from(nargs);
    let fits_stack = i64::from(callee) + i64::from(nresults) * i64::from(TVALUE_SIZE) <= i64::from(last);
    if !fits_ci || !fits_stack {
        return Err(format!(
            "nresults {nresults} does not fit: ci->top {ci_top:#x} is {} slot(s) above top {top:#x} (Lua allows nresults - nargs \
             up to that), and stack_last {last:#x} is {} slot(s) above the function's slot",
            slots(ci_top, top),
            slots(last, callee)
        ));
    }
    Ok(callee)
}

#[cfg(test)]
pub mod fake {
    use super::Mem;
    use std::collections::BTreeMap;

    /// A sparse address space of readable regions.
    #[derive(Default)]
    pub struct FakeMem {
        regions: BTreeMap<u32, Vec<u8>>,
    }

    impl FakeMem {
        pub fn map(&mut self, base: u32, bytes: Vec<u8>) {
            self.regions.insert(base, bytes);
        }
        fn region(&self, addr: u32) -> Option<(u32, &Vec<u8>)> {
            let (&base, bytes) = self.regions.range(..=addr).next_back()?;
            (addr - base < bytes.len() as u32).then_some((base, bytes))
        }
    }

    impl Mem for FakeMem {
        fn readable_span(&self, addr: u32) -> u32 {
            self.region(addr).map_or(0, |(base, b)| b.len() as u32 - (addr - base))
        }
        fn read_u32(&self, addr: u32) -> u32 {
            let (base, b) = self.region(addr).expect("read of unmapped memory");
            let o = (addr - base) as usize;
            u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
        }
        fn read_u8(&self, addr: u32) -> u8 {
            let (base, b) = self.region(addr).expect("read of unmapped memory");
            b[(addr - base) as usize]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeMem;
    use super::*;

    const L: u32 = 0x0010_0000;
    const STACK: u32 = 0x0020_0000;
    const STRINGS: u32 = 0x0030_0000;

    fn tstring(text: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; TSTRING_DATA as usize];
        b.extend_from_slice(text);
        b.push(0);
        b
    }

    /// A lua_State whose frame holds `args` (value, tt) starting at stack slot 1; `base_ok` false puts
    /// L->base outside the stack so the ci->func fallback is used.
    fn state(args: &[(u32, u32)], base_ok: bool) -> FakeMem {
        let mut m = FakeMem::default();
        let base = STACK + TVALUE_SIZE;
        let top = base + args.len() as u32 * TVALUE_SIZE;
        let ci = L + 0x40;
        let mut l = vec![0u8; 0x58];
        let mut put = |o: u32, v: u32| l[o as usize..o as usize + 4].copy_from_slice(&v.to_le_bytes());
        put(OFF_TOP, top);
        put(OFF_BASE, if base_ok { base } else { 0 });
        put(OFF_CI, ci);
        put(OFF_STACK_LAST, STACK + 64 * TVALUE_SIZE);
        put(OFF_STACK, STACK);
        put(0x40 + CI_OFF_BASE, base);
        put(0x40 + CI_OFF_FUNC, STACK);
        // A C function's ci->top: LUA_MINSTACK (20) slots above its base.
        put(0x40 + CI_OFF_TOP, base + 20 * TVALUE_SIZE);
        m.map(L, l);
        let mut s = vec![0u8; 65 * TVALUE_SIZE as usize];
        for (i, (v, tt)) in args.iter().enumerate() {
            let o = (i + 1) * 8;
            s[o..o + 4].copy_from_slice(&v.to_le_bytes());
            s[o + 4..o + 8].copy_from_slice(&tt.to_le_bytes());
        }
        m.map(STACK, s);
        m
    }

    #[test]
    fn reads_each_type() {
        let mut m = state(
            &[
                (STRINGS, LUA_TSTRING),
                (1.5f32.to_bits(), LUA_TNUMBER),
                (0, LUA_TBOOLEAN),
                (0, LUA_TNIL),
                (STRINGS + 0x100, LUA_TSTRING),
            ],
            true,
        );
        m.map(STRINGS, tstring(b"hello"));
        m.map(STRINGS + 0x100, tstring(b"world"));
        assert_eq!(nargs(&m, L), 5);
        assert_eq!(arg_type(&m, L, 1), LUA_TNUMBER as i32);
        assert_eq!(arg_type(&m, L, 5), -1);
        assert_eq!(arg_number(&m, L, 1), Some(1.5));
        assert_eq!(arg_number(&m, L, 0), None);
        assert_eq!(arg_bool(&m, L, 2), Some(false));
        assert_eq!(arg_bool(&m, L, 3), Some(false));
        assert_eq!(arg_bool(&m, L, 1), Some(true));
        assert_eq!(arg_bool(&m, L, 9), None);
        let mut out = [0xAAu8; 16];
        assert_eq!(arg_string(&m, L, 0, &mut out), 5);
        assert_eq!(&out[..6], b"hello\0");
        let mut out = [0xAAu8; 32];
        assert_eq!(join_strings(&m, L, &mut out), 2);
        assert_eq!(&out[..12], b"hello\tworld\0");
    }

    #[test]
    fn an_object_argument_reads_as_the_game_reads_it() {
        let m = state(&[(0x4000_0915, LUA_TLIGHTUSERDATA), (0x0050_0000, LUA_TUSERDATA), (1.5f32.to_bits(), LUA_TNUMBER)], true);
        assert_eq!(arg_object(&m, L, 0), Some(0x4000_0915), "a light userdata is the object's guid");
        assert_eq!(arg_object(&m, L, 1), Some(0x0050_0018), "a full userdata is its payload address");
        assert_eq!(arg_object(&m, L, 2), None, "a number is not an object");
        assert_eq!(arg_object(&m, L, 3), None, "an absent argument is not an object");
    }

    #[test]
    fn truncation_and_small_buffers() {
        let mut m = state(&[(STRINGS, LUA_TSTRING), (STRINGS + 0x100, LUA_TSTRING)], true);
        m.map(STRINGS, tstring(b"hello"));
        m.map(STRINGS + 0x100, tstring(b"world"));
        let mut out = [0xAAu8; 4];
        assert_eq!(arg_string(&m, L, 0, &mut out), 3);
        assert_eq!(&out, b"hel\0");
        let mut out = [0xAAu8; 8];
        assert_eq!(join_strings(&m, L, &mut out), 2);
        assert_eq!(&out, b"hello\tw\0");
        let mut out = [0xAAu8; 1];
        assert_eq!(arg_string(&m, L, 0, &mut out), 0);
        assert_eq!(out[0], 0);
    }

    #[test]
    fn base_falls_back_to_ci_func() {
        let mut m = state(&[(STRINGS, LUA_TSTRING)], false);
        m.map(STRINGS, tstring(b"x"));
        assert_eq!(nargs(&m, L), 1);
    }

    #[test]
    fn a_stack_index_names_a_slot_of_the_frame_or_is_refused() {
        let m = state(&[(1, LUA_TNUMBER), (2, LUA_TNUMBER), (3, LUA_TNUMBER)], true);
        let base = STACK + TVALUE_SIZE;
        assert_eq!(stack_slot(&m, L, 1), Ok(base), "1 is the first value of the frame");
        assert_eq!(stack_slot(&m, L, 3), Ok(base + 2 * TVALUE_SIZE));
        assert_eq!(stack_slot(&m, L, -1), Ok(base + 2 * TVALUE_SIZE), "-1 is the top value");
        assert_eq!(stack_slot(&m, L, -3), Ok(base));
        assert!(stack_slot(&m, L, 4).unwrap_err().contains("above the top"));
        assert!(stack_slot(&m, L, -4).unwrap_err().contains("below the base"));
        assert!(stack_slot(&m, L, 0).unwrap_err().contains("index 0"));
        assert!(stack_slot(&m, L, LUA_REGISTRYINDEX).unwrap_err().contains("pseudo-index -10000"));
        assert!(stack_slot(&m, L, -10002).unwrap_err().contains("pseudo-index"), "LUA_GLOBALSINDEX");
        assert!(stack_slot(&m, L, -9999).unwrap_err().contains("below the base"), "the last real index");
        assert!(stack_slot(&FakeMem::default(), L, 1).unwrap_err().contains("not readable"));
        assert!(stack_slot(&m, L + 2, 1).unwrap_err().contains("not readable"), "a misaligned state");
        assert_eq!(frame_depth(&m, L), Ok(3));
        assert_eq!(frame_depth(&state(&[], true), L), Ok(0), "an empty frame is a frame");
    }

    #[test]
    fn room_is_lua_checkstack_s_test() {
        // stack_last is slot 64; three values from slot 1 leave top at slot 4: 60 slots to stack_last.
        let m = state(&[(1, LUA_TNUMBER), (2, LUA_TNUMBER), (3, LUA_TNUMBER)], true);
        assert_eq!(has_room(&m, L, 59), Ok(true));
        assert_eq!(has_room(&m, L, 60), Ok(false), "lua_checkstack grows when stack_last - top <= n");
        assert!(has_room(&FakeMem::default(), L, 1).is_err());
    }

    /// The ASan reproduction of the review: a C function's frame with LUA_MINSTACK room, `lua_pcall` with
    /// nresults 10 returned, nresults 1000 wrote past the stack in `luaD_poscall`.
    #[test]
    fn pcall_results_must_end_at_or_below_ci_top() {
        let m = state(&[(0, LUA_TFUNCTION)], true);
        let callee = STACK + TVALUE_SIZE;
        // top is slot 2 and ci->top slot 21: checkresults allows nresults - nargs up to 19.
        assert_eq!(pcall_callee(&m, L, 0, 10), Ok(callee));
        assert_eq!(pcall_callee(&m, L, 0, 19), Ok(callee), "ci->top - top == nresults - nargs passes checkresults");
        assert!(pcall_callee(&m, L, 0, 20).unwrap_err().contains("is 19 slot(s) above top"));
        assert!(pcall_callee(&m, L, 0, 1000).unwrap_err().contains("nresults 1000 does not fit"));
        assert!(pcall_callee(&m, L, 0, i32::MAX).unwrap_err().contains("does not fit"));
        assert_eq!(pcall_callee(&m, L, 0, LUA_MULTRET), Ok(callee), "LUA_MULTRET keeps the callee's own results");
        assert!(pcall_callee(&m, L, 0, -2).unwrap_err().contains("below LUA_MULTRET"));
        assert!(pcall_callee(&m, L, -1, 0).unwrap_err().contains("negative"));
        assert!(pcall_callee(&m, L, 1, 0).unwrap_err().contains("fewer than the function and its 1 argument"));
        let with_args = state(&[(0, LUA_TFUNCTION), (1, LUA_TNUMBER), (2, LUA_TNUMBER)], true);
        assert_eq!(pcall_callee(&with_args, L, 2, 0), Ok(callee), "the function sits below its arguments");
        // top is slot 4, ci->top slot 21: 17 slots, so with 2 arguments up to 19 results.
        assert_eq!(pcall_callee(&with_args, L, 2, 19), Ok(callee), "the arguments' slots count toward the results");
        assert!(pcall_callee(&with_args, L, 2, 20).unwrap_err().contains("is 17 slot(s) above top"));
        assert!(pcall_callee(&FakeMem::default(), L, 0, 0).is_err());
    }

    #[test]
    fn pcall_results_must_also_end_at_or_below_stack_last() {
        // ci->top (slot 100) above stack_last (slot 64): stack_last bounds the results.
        let mut m = state(&[(0, LUA_TFUNCTION)], true);
        let mut l = vec![0u8; 0x58];
        let mut put = |o: u32, v: u32| l[o as usize..o as usize + 4].copy_from_slice(&v.to_le_bytes());
        put(OFF_TOP, STACK + 2 * TVALUE_SIZE);
        put(OFF_BASE, STACK + TVALUE_SIZE);
        put(OFF_CI, L + 0x40);
        put(OFF_STACK_LAST, STACK + 64 * TVALUE_SIZE);
        put(OFF_STACK, STACK);
        put(0x40 + CI_OFF_TOP, STACK + 100 * TVALUE_SIZE);
        m.map(L, l);
        assert_eq!(pcall_callee(&m, L, 0, 63), Ok(STACK + TVALUE_SIZE));
        assert!(pcall_callee(&m, L, 0, 64).unwrap_err().contains("is 63 slot(s) above the function's slot"));
    }

    #[test]
    fn invalid_states_read_as_minus_one() {
        let m = state(&[], true);
        assert_eq!(nargs(&m, L), -1, "zero arguments is an invalid frame");
        assert_eq!(join_strings(&m, L, &mut [0u8; 8]), -1);
        assert_eq!(nargs(&m, 0x1234), -1);
        assert_eq!(nargs(&m, L + 1), -1);
        assert_eq!(nargs(&FakeMem::default(), L), -1);
        let many: Vec<(u32, u32)> = (0..33).map(|_| (0, LUA_TNIL)).collect();
        assert_eq!(nargs(&state(&many, true), L), -1);
    }
}
