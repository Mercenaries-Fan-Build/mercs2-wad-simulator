//! How m2-sdk.dll's `m2_lua_exec` reports a chunk's outcome: the result text, in the format lua_bridge
//! clients read (`[ok]\t<v1>\t<v2>…`, `[runtime] <v1>…`, `[compile] "<message>"`), and which stack slots
//! hold the status and the results.
//!
//! The executor calls the game's `luaB_pcall` directly, so when the call returns, `L->base` is back at the
//! base of the frame the executor runs on top of; `lua_insert(L, 1)` then puts the status boolean in that
//! frame's slot 0 and moves every slot above it up by one. The executor's slot 0 therefore holds the outer
//! frame's last slot and the chunk's results start at slot 1: in game runs that slot held a table (lua_bridge
//! v0.5.0, 8 executions on the cracked retail exe; the status was 0 for exactly the failing chunk) and, run
//! from inside `type(s)`, the string `s` (m2-sdk 0.4.0, 2026-10-11). [`outcome`] applies that, and keeps the
//! stock layout (the status in slot 0) for a frame without a saved status.

use crate::lua_stack::{LUA_TBOOLEAN, LUA_TFUNCTION, LUA_TNIL, LUA_TNUMBER, LUA_TSTRING, LUA_TTABLE, Mem, TSTRING_DATA, TSTRING_LEN};

/// The largest chunk the executor runs, in bytes.
pub const MAX_CHUNK: usize = 1 << 20;
/// The most result values formatted.
pub const MAX_RESULTS: usize = 16;
/// A string longer than this is reported as implausible rather than read.
const MAX_STRING: u32 = 16 * 1024 * 1024;

/// `%g` as msvcrt's `printf` writes it: six significant digits, trailing zeros removed, scientific
/// notation with a sign and at least three exponent digits when the exponent is below -4 or above 5;
/// `1.#INF`, `-1.#INF`, `1.#QNAN` and `-1.#IND` for infinities and NaNs.
pub fn fmt_g(x: f64) -> String {
    if x.is_infinite() {
        return if x < 0.0 { "-1.#INF".into() } else { "1.#INF".into() };
    }
    if x.is_nan() {
        return if x.is_sign_negative() { "-1.#IND".into() } else { "1.#QNAN".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let sci = format!("{x:.5e}");
    let (mant, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let exp: i32 = exp.parse().expect("{:e} exponent is an integer");
    let strip = |s: &str| -> String {
        if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_owned() } else { s.to_owned() }
    };
    if !(-4..6).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:03}", strip(mant), exp.abs())
    } else {
        let decimals = (5 - exp).max(0) as usize;
        strip(&format!("{x:.decimals$}"))
    }
}

/// Append the text of the value in the stack slot at `slot` (a `TValue`: value at +0, tag at +4).
pub fn format_value<M: Mem>(m: &M, slot: u32, out: &mut Vec<u8>) {
    let v = m.read_u32(slot);
    let tt = m.read_u32(slot + 4);
    match tt {
        LUA_TNIL => out.extend_from_slice(b"nil"),
        LUA_TBOOLEAN => out.extend_from_slice(if v != 0 { b"true" } else { b"false" }),
        LUA_TNUMBER => out.extend_from_slice(fmt_g(f64::from(f32::from_bits(v))).as_bytes()),
        LUA_TSTRING => {
            if m.readable_span(v) < 0x14 {
                out.extend_from_slice(b"<string: gc unreadable>");
                return;
            }
            let len = m.read_u32(v + TSTRING_LEN);
            if len > MAX_STRING {
                out.extend_from_slice(format!("<string: implausible len={len}>").as_bytes());
                return;
            }
            let data = v + TSTRING_DATA;
            if m.readable_span(data) < len + 1 {
                out.extend_from_slice(b"<string: data unreadable>");
                return;
            }
            out.push(b'"');
            out.extend((0..len).map(|i| m.read_u8(data + i)));
            out.push(b'"');
        }
        LUA_TTABLE => out.extend_from_slice(b"<table>"),
        LUA_TFUNCTION => out.extend_from_slice(b"<function>"),
        _ => out.extend_from_slice(format!("<tt={tt} val={v:08X}>").as_bytes()),
    }
}

/// What a finished `pcall` produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub succeeded: bool,
    /// The first result slot to report (1 when slot 0 is the status or the table artifact).
    pub first_result: usize,
}

/// `saved_status`: the `(tag, value)` the outer frame's slot 0 held right after the call, when the executor
/// saved that frame. `slot0`: the `(tag, value)` of the executor frame's slot 0, when there is a result.
pub fn outcome(saved_status: Option<(u32, u32)>, slot0: Option<(u32, u32)>) -> Outcome {
    if let Some((LUA_TBOOLEAN, v)) = saved_status {
        return Outcome { succeeded: v != 0, first_result: usize::from(slot0.is_some()) };
    }
    let mut status = None;
    let mut skip = false;
    if let Some((tt, v)) = slot0 {
        if tt == LUA_TTABLE || tt == LUA_TFUNCTION {
            skip = true;
        } else if status.is_none() && tt == LUA_TBOOLEAN {
            status = Some(v != 0);
            skip = true;
        } else if status.is_none() && tt == LUA_TNUMBER {
            status = Some(f32::from_bits(v) != 0.0);
            skip = true;
        }
    }
    Outcome { succeeded: status.unwrap_or(false), first_result: usize::from(skip) }
}

/// The result line: `[ok]` then each result after a tab, or `[runtime]` then each after a space.
pub fn render<M: Mem>(m: &M, o: Outcome, base: u32, slots: usize) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(if o.succeeded { b"[ok]" } else { b"[runtime]" });
    for i in o.first_result..slots.min(MAX_RESULTS) {
        out.push(if o.succeeded { b'\t' } else { b' ' });
        format_value(m, base + i as u32 * 8, &mut out);
    }
    out
}

/// The compile-error line: `[compile] ` and the message slot's text.
pub fn render_compile_error<M: Mem>(m: &M, message_slot: u32) -> Vec<u8> {
    let mut out = b"[compile] ".to_vec();
    format_value(m, message_slot, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua_stack::fake::FakeMem;

    #[test]
    fn g_matches_msvcrt() {
        for (x, want) in [
            (0.0, "0"),
            (1.0, "1"),
            (1.5, "1.5"),
            (-2.25, "-2.25"),
            (100000.0, "100000"),
            (1000000.0, "1e+006"),
            (123456789.0, "1.23457e+008"),
            (0.0001, "0.0001"),
            (0.00001, "1e-005"),
            (3.14159265, "3.14159"),
            (2.5e-10, "2.5e-010"),
            (1e100, "1e+100"),
            (999999.5, "1e+006"),
        ] {
            assert_eq!(fmt_g(x), want, "{x}");
        }
        assert_eq!(fmt_g(f64::INFINITY), "1.#INF");
        assert_eq!(fmt_g(f64::NEG_INFINITY), "-1.#INF");
        assert_eq!(fmt_g(-f64::NAN), "-1.#IND");
        assert_eq!(fmt_g(f64::from(0.1f32)), "0.1");
    }

    fn stack(values: &[(u32, u32)]) -> FakeMem {
        let mut m = FakeMem::default();
        let mut b = Vec::new();
        for (v, tt) in values {
            b.extend_from_slice(&v.to_le_bytes());
            b.extend_from_slice(&tt.to_le_bytes());
        }
        m.map(0x1000, b);
        let mut ts = vec![0u8; 16];
        ts[12..16].copy_from_slice(&5u32.to_le_bytes());
        ts.extend_from_slice(b"hello\0");
        m.map(0x2000, ts);
        m
    }

    #[test]
    fn values_format_like_the_bridge() {
        let m = stack(&[(0, LUA_TNIL), (1, LUA_TBOOLEAN), (2.5f32.to_bits(), LUA_TNUMBER), (0x2000, LUA_TSTRING), (7, LUA_TTABLE), (0x1234, 9)]);
        let o = Outcome { succeeded: true, first_result: 0 };
        assert_eq!(String::from_utf8(render(&m, o, 0x1000, 6)).unwrap(), "[ok]\tnil\ttrue\t2.5\t\"hello\"\t<table>\t<tt=9 val=00001234>");
        let o = Outcome { succeeded: false, first_result: 3 };
        assert_eq!(String::from_utf8(render(&m, o, 0x1000, 4)).unwrap(), "[runtime] \"hello\"");
        assert_eq!(String::from_utf8(render_compile_error(&m, 0x1000 + 3 * 8)).unwrap(), "[compile] \"hello\"");
        let bad = stack(&[(0x9000, LUA_TSTRING)]);
        assert_eq!(String::from_utf8(render(&bad, Outcome { succeeded: true, first_result: 0 }, 0x1000, 1)).unwrap(), "[ok]\t<string: gc unreadable>");
    }

    #[test]
    fn the_status_comes_from_the_outer_frame_and_the_table_artifact_is_skipped() {
        // The game's layout: status in the outer frame, a table in slot 0.
        assert_eq!(outcome(Some((LUA_TBOOLEAN, 1)), Some((LUA_TTABLE, 0))), Outcome { succeeded: true, first_result: 1 });
        assert_eq!(outcome(Some((LUA_TBOOLEAN, 0)), Some((LUA_TTABLE, 0))), Outcome { succeeded: false, first_result: 1 });
        // The stock layout: the status is slot 0.
        assert_eq!(outcome(None, Some((LUA_TBOOLEAN, 1))), Outcome { succeeded: true, first_result: 1 });
        assert_eq!(outcome(None, Some((LUA_TNUMBER, 0))), Outcome { succeeded: false, first_result: 1 });
        // Whatever the outer frame's last slot held is skipped: here the string `type(s)` was called with.
        assert_eq!(outcome(Some((LUA_TBOOLEAN, 1)), Some((LUA_TSTRING, 0))), Outcome { succeeded: true, first_result: 1 });
        assert_eq!(outcome(Some((LUA_TBOOLEAN, 0)), Some((LUA_TNIL, 0))), Outcome { succeeded: false, first_result: 1 });
        assert_eq!(outcome(Some((LUA_TBOOLEAN, 1)), None), Outcome { succeeded: true, first_result: 0 });
        // Nothing known: reported as a runtime failure.
        assert_eq!(outcome(None, None), Outcome { succeeded: false, first_result: 0 });
    }
}
