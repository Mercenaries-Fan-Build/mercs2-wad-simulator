//! The INI reader behind m2-sdk.dll's `m2_ini_parse`, `m2_ini_bool` and `m2_ini_int` (`m2/m2_ini.h`).
//!
//! Line-oriented and section-blind: `;` and `#` start a comment anywhere on a line, `[section]` lines are
//! skipped, every other line splits on its first `=` with both sides trimmed of spaces and tabs. Text after
//! an embedded NUL is not read.

/// The largest file `m2_ini_parse` reads; an empty file or a larger one is not read at all.
pub const MAX_FILE: usize = 1 << 20;

fn trim_start(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|&&b| b == b' ' || b == b'\t').count();
    &s[n..]
}

fn trim(s: &[u8]) -> &[u8] {
    let s = trim_start(s);
    let n = s.iter().rev().take_while(|&&b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')).count();
    &s[..s.len() - n]
}

/// Call `kv(key, value)` for every `key = value` line of `text`, in file order.
pub fn parse(text: &[u8], mut kv: impl FnMut(&[u8], &[u8])) {
    let text = match text.iter().position(|&b| b == 0) {
        Some(nul) => &text[..nul],
        None => text,
    };
    for raw in text.split(|&b| b == b'\n') {
        let line = match raw.iter().position(|&b| b == b';' || b == b'#') {
            Some(c) => &raw[..c],
            None => raw,
        };
        let line = trim(line);
        if line.is_empty() || line[0] == b'[' {
            continue;
        }
        if let Some(eq) = line.iter().position(|&b| b == b'=') {
            kv(trim(&line[..eq]), trim(&line[eq + 1..]));
        }
    }
}

/// `"1"`, `"true"`, `"on"`, `"yes"` in any case read as true; everything else is false.
pub fn bool_value(value: &[u8]) -> bool {
    ["1", "true", "on", "yes"].iter().any(|t| value.eq_ignore_ascii_case(t.as_bytes()))
}

/// An optional leading `-`, then decimal digits up to the first non-digit; `fallback` when there are none.
pub fn int_value(value: &[u8], fallback: i32) -> i32 {
    let (neg, digits) = match value.first() {
        Some(b'-') => (true, &value[1..]),
        _ => (false, value),
    };
    let mut v: i32 = 0;
    let mut any = false;
    for &b in digits.iter().take_while(|b| b.is_ascii_digit()) {
        v = v.wrapping_mul(10).wrapping_add(i32::from(b - b'0'));
        any = true;
    }
    if !any {
        return fallback;
    }
    if neg { v.wrapping_neg() } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        parse(text.as_bytes(), |k, v| {
            out.push((String::from_utf8(k.to_vec()).unwrap(), String::from_utf8(v.to_vec()).unwrap()))
        });
        out
    }

    #[test]
    fn lines_comments_sections_and_trimming() {
        let text = "; header\r\n[repl]\r\n  host = 127.0.0.1  \r\nport=27050 ; trailing\r\n\tkey\t=\tvalue # hash\r\nnoequals\r\n=empty key\r\nk=\r\n";
        assert_eq!(
            pairs(text),
            vec![
                ("host".into(), "127.0.0.1".into()),
                ("port".into(), "27050".into()),
                ("key".into(), "value".into()),
                ("".into(), "empty key".into()),
                ("k".into(), "".into()),
            ]
        );
    }

    #[test]
    fn first_equals_splits_and_sections_are_ignored() {
        assert_eq!(pairs("[a]\nx=1=2\n[b]\nx=3"), vec![("x".into(), "1=2".into()), ("x".into(), "3".into())]);
    }

    #[test]
    fn text_after_a_nul_is_not_read() {
        assert_eq!(pairs("a=1\nb=2\0\nc=3\n"), vec![("a".into(), "1".into()), ("b".into(), "2".into())]);
    }

    #[test]
    fn bool_and_int() {
        for t in ["1", "true", "TRUE", "On", "yes"] {
            assert!(bool_value(t.as_bytes()), "{t}");
        }
        for f in ["0", "false", "off", "no", "", "2", " 1"] {
            assert!(!bool_value(f.as_bytes()), "{f:?}");
        }
        assert_eq!(int_value(b"42", 7), 42);
        assert_eq!(int_value(b"-8000", 7), -8000);
        assert_eq!(int_value(b"12abc", 7), 12);
        assert_eq!(int_value(b"abc", 7), 7);
        assert_eq!(int_value(b"-", 7), 7);
        assert_eq!(int_value(b"+5", 7), 7);
        assert_eq!(int_value(b"", 7), 7);
    }
}
