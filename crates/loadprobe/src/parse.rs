//! Parser for `pmc_blackbox.log` lines.
//!
//! Format (see `tools/pmc_blackbox/pmc_blackbox.c` `pmc_log`):
//!   `[HH:MM:SS.mmm] [source] message`
//! Lua lines (`tools/pmc_blackbox/lua_log_hook.c`) carry a trailing `  @script:line`
//! and `[world]` echoes are prefixed `>>> `. A few lines (continuation hexdumps) do
//! not start with a timestamp; we keep them as `source == "raw"` attached in order.

/// One parsed log line. `script`/`line`/`world_echo`/`lineno` are retained for
/// completeness and potential consumers (JSON, future call-site reporting) even
/// where the current text dump doesn't print them.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct LogLine {
    /// Monotonic milliseconds (midnight-wrap corrected, see `parse_log`).
    pub ts_ms: u64,
    /// Original `HH:MM:SS.mmm` text (empty for `raw` continuation lines).
    pub raw_ts: String,
    /// Source tag without brackets: `lua`, `pool`, `crash`, ... or `raw`.
    pub source: String,
    /// Message with the `@script:line` suffix and leading `>>> ` stripped.
    pub msg: String,
    /// Lua caller script (without the `@`), if present.
    pub script: Option<String>,
    /// Lua caller line, if present.
    pub line: Option<u32>,
    /// True when the original line was a `>>> `-prefixed `[world]` echo.
    pub world_echo: bool,
    /// 1-based line number in the file.
    pub lineno: usize,
}

impl LogLine {
    /// A signal-prefixed Lua marker (starts with one of the given prefixes).
    pub fn signal_prefix<'a>(&self, prefixes: &[&'a str]) -> Option<&'a str> {
        let m = self.msg.trim_start();
        prefixes.iter().copied().find(|p| m.starts_with(p))
    }
}

/// Parse `HH:MM:SS.mmm` into milliseconds-since-midnight. Returns None on bad shape.
fn parse_ts(s: &str) -> Option<u64> {
    // s like "21:02:43.033"
    let (hms, ms) = s.split_once('.')?;
    let mut it = hms.split(':');
    let h: u64 = it.next()?.parse().ok()?;
    let m: u64 = it.next()?.parse().ok()?;
    let sec: u64 = it.next()?.parse().ok()?;
    let mil: u64 = ms.parse().ok()?;
    if it.next().is_some() || h > 23 || m > 59 || sec > 59 || mil > 999 {
        return None;
    }
    Some(((h * 3600 + m * 60 + sec) * 1000) + mil)
}

/// Split a `  @script:line` suffix off a Lua message. Returns (msg, script, line).
fn split_script(msg: &str) -> (String, Option<String>, Option<u32>) {
    // The hook emits "  @name:123" or "  @name"; find the LAST "  @".
    if let Some(at) = msg.rfind("  @") {
        let body = &msg[at + 3..];
        // Guard: only treat as a script tag if it looks like name[:line] with no spaces.
        if !body.is_empty() && !body.contains(' ') {
            let (name, line) = match body.rsplit_once(':') {
                Some((n, l)) if l.chars().all(|c| c.is_ascii_digit()) && !l.is_empty() => {
                    (n.to_string(), l.parse::<u32>().ok())
                }
                _ => (body.to_string(), None),
            };
            return (msg[..at].to_string(), Some(name), line);
        }
    }
    (msg.to_string(), None, None)
}

/// Parse a full log file into ordered `LogLine`s, correcting a single midnight wrap.
pub fn parse_log(text: &str) -> Vec<LogLine> {
    let mut out = Vec::new();
    let mut day_offset: u64 = 0;
    let mut prev_raw: u64 = 0;
    let mut last_ts_ms: u64 = 0;

    for (i, line) in text.lines().enumerate() {
        let lineno = i + 1;
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }

        // Expect "[ts] [source] rest". A non-matching line is a continuation.
        let parsed = parse_prefix(line);
        match parsed {
            Some((ts_str, source, rest)) => {
                let raw_ms = match parse_ts(ts_str) {
                    Some(v) => v,
                    None => {
                        push_raw(&mut out, line, last_ts_ms, lineno);
                        continue;
                    }
                };
                // Midnight wrap: if the raw ms goes backwards by > 1h, assume a new day.
                if raw_ms + 3_600_000 < prev_raw {
                    day_offset += 86_400_000;
                }
                prev_raw = raw_ms;
                let ts_ms = raw_ms + day_offset;
                last_ts_ms = ts_ms;

                let world_echo = rest.starts_with(">>> ");
                let body = if world_echo { &rest[4..] } else { rest };
                let (msg, script, line_no) = split_script(body);
                out.push(LogLine {
                    ts_ms,
                    raw_ts: ts_str.to_string(),
                    source: source.to_string(),
                    msg,
                    script,
                    line: line_no,
                    world_echo,
                    lineno,
                });
            }
            None => push_raw(&mut out, line, last_ts_ms, lineno),
        }
    }
    out
}

fn push_raw(out: &mut Vec<LogLine>, line: &str, ts_ms: u64, lineno: usize) {
    out.push(LogLine {
        ts_ms,
        raw_ts: String::new(),
        source: "raw".to_string(),
        msg: line.to_string(),
        script: None,
        line: None,
        world_echo: false,
        lineno,
    });
}

/// Split a line into (ts, source, rest) where it begins `[ts] [source] rest`.
fn parse_prefix(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.strip_prefix('[')?;
    let (ts, after) = rest.split_once(']')?;
    let after = after.strip_prefix(' ')?;
    let after = after.strip_prefix('[')?;
    let (source, rest) = after.split_once(']')?;
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    Some((ts, source, rest))
}
