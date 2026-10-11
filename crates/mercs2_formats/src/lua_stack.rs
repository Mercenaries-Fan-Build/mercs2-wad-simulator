//! Argument reads off the game's Lua 5.1.2 stack (32-bit, float `lua_Number`), behind m2-sdk.dll's
//! `m2_lua_nargs`, `m2_lua_arg_*` and `m2_lua_join_strings` (`m2/m2_luastack.h`).
//!
//! TValue is 8 bytes `{u32 value; u32 tt}`. Every dereference goes through [`Mem`], which the DLL backs
//! with `VirtualQuery`, so a caller that is not a Lua C function reads as an invalid stack, not a fault.

pub const OFF_TOP: u32 = 0x08;
pub const OFF_BASE: u32 = 0x0C;
pub const OFF_CI: u32 = 0x14;
pub const OFF_STACK_LAST: u32 = 0x1C;
pub const OFF_STACK: u32 = 0x20;
pub const CI_OFF_BASE: u32 = 0x00;
pub const CI_OFF_FUNC: u32 = 0x04;

pub const TVALUE_SIZE: u32 = 8;
pub const TVALUE_TT: u32 = 4;
/// String bytes start at TString + 16.
pub const TSTRING_DATA: u32 = 16;
pub const MAX_ARGS: u32 = 32;

pub const LUA_TNIL: u32 = 0;
pub const LUA_TBOOLEAN: u32 = 1;
pub const LUA_TNUMBER: u32 = 3;
pub const LUA_TSTRING: u32 = 4;

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
        let mut l = vec![0u8; 0x48];
        let mut put = |o: u32, v: u32| l[o as usize..o as usize + 4].copy_from_slice(&v.to_le_bytes());
        put(OFF_TOP, top);
        put(OFF_BASE, if base_ok { base } else { 0 });
        put(OFF_CI, ci);
        put(OFF_STACK_LAST, STACK + 64 * TVALUE_SIZE);
        put(OFF_STACK, STACK);
        put(0x40 + CI_OFF_BASE, base);
        put(0x40 + CI_OFF_FUNC, STACK);
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
