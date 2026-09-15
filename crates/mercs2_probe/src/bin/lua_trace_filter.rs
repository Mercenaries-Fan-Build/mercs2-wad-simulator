//! `lua_trace_filter` — read `lua_trace.ndjson` emitted by `mods/lua_trace_asi.asi`, filter by
//! function name and/or caller Lua source, print matching lines to stdout.
//!
//! # Why this exists
//!
//! `lua_trace_asi` writes one JSON object per Lua→engine cfunc call. The file is a live-run
//! oracle — Surface B of the modernization regression harness — but at ~1200 bindings and
//! thousands of call sites per session, the raw stream is unreadable. A Shipment author or a
//! reimpl reviewer wants to see **just the calls their mission fires**, or **just the calls
//! into a particular namespace**.
//!
//! With `caller_src` (`@wifpmcinterior:214` style) landing in the extended `lua_trace_asi`,
//! `--caller-src <substr>` filters by the script that made the call — a modder can pass the
//! stem of their `add_script` module and see exactly what their mission touched.
//!
//! # Usage
//!
//! ```text
//! # every Pg.Spawn call:
//! mercs2_probe lua_trace_filter --fn Pg.Spawn < lua_trace.ndjson
//!
//! # everything the FioDef001 mission fired:
//! lua_trace_filter --caller-src fiodef001 --input lua_trace.ndjson
//!
//! # narrow to Ai.* calls made by wifpmcinterior:
//! lua_trace_filter --fn Ai. --caller-src wifpmcinterior
//! ```
//!
//! # What it does NOT do (yet)
//!
//! - Colour output / pretty-print. Records go out verbatim; wrap in `jq` if you want them
//!   formatted. Keeping the tool boringly line-oriented means it composes with `head`, `tail`,
//!   `sort`, `wc`, etc. without surprise.
//! - Multiple pattern-file input (`grep -f`). One `--fn` + one `--caller-src` is the surface;
//!   more complex predicates belong in a downstream script.

use serde_json::Value;
use std::io::{BufRead, BufWriter, Write};

fn main() -> std::io::Result<()> {
    let cli = parse_args();

    let stdin;
    let file;
    let reader: Box<dyn BufRead> = match &cli.input {
        Some(p) => {
            file = std::fs::File::open(p)?;
            Box::new(std::io::BufReader::new(file))
        }
        None => {
            stdin = std::io::stdin();
            Box::new(stdin.lock())
        }
    };

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut kept = 0usize;
    let mut total = 0usize;
    for line in reader.lines() {
        let line = line?;
        total += 1;
        if line.trim().is_empty() {
            continue;
        }
        // Parse to Value: robust against schema drift (new fields the ASI may add later) and
        // against pathological args arrays. A record that does not parse is a `_log`/`warn`
        // sidecar or a corrupt line — we pass it through only when no filters are set.
        let parsed: Option<Value> = serde_json::from_str(&line).ok();
        if keep(parsed.as_ref(), &cli) {
            kept += 1;
            out.write_all(line.as_bytes())?;
            out.write_all(b"\n")?;
        }
    }
    if cli.stats {
        eprintln!("lua_trace_filter: kept {kept} / {total} records");
    }
    Ok(())
}

/// Keep this record iff every declared filter matches. Filter matching is substring-based;
/// case-insensitive; empty-filter matches everything.
fn keep(rec: Option<&Value>, cli: &Cli) -> bool {
    // A line that failed to parse: pass through only when no filters are active. Filters
    // without a parsed record cannot match, so this preserves `_log`/`warn` sidecars in the
    // unfiltered case (which is the default).
    let Some(v) = rec else {
        return cli.fn_pattern.is_none() && cli.src_pattern.is_none() && cli.argc.is_none();
    };
    if let Some(pat) = &cli.fn_pattern {
        let fn_name = v.get("fn").and_then(|s| s.as_str()).unwrap_or("");
        if !fn_name.to_ascii_lowercase().contains(pat) {
            return false;
        }
    }
    if let Some(pat) = &cli.src_pattern {
        let src = v.get("caller_src").and_then(|s| s.as_str()).unwrap_or("");
        if !src.to_ascii_lowercase().contains(pat) {
            return false;
        }
    }
    if let Some(argc) = cli.argc {
        let n = v.get("argc").and_then(|n| n.as_i64()).unwrap_or(i64::MIN);
        if n != argc {
            return false;
        }
    }
    true
}

/// The tool's whole CLI surface. Substring filters (lowercased on input) are all AND-composed:
/// a record matches when every declared filter matches.
struct Cli {
    input: Option<std::path::PathBuf>,
    fn_pattern: Option<String>,
    src_pattern: Option<String>,
    argc: Option<i64>,
    stats: bool,
}

/// Hand-parsed to match the `emmylua-gen` style — clap for four flags is overkill and adds a
/// dep the crate does not otherwise need.
fn parse_args() -> Cli {
    let mut cli = Cli {
        input: None,
        fn_pattern: None,
        src_pattern: None,
        argc: None,
        stats: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--input" | "-i" => cli.input = args.next().map(std::path::PathBuf::from),
            "--fn" => cli.fn_pattern = args.next().map(|s| s.to_ascii_lowercase()),
            "--caller-src" | "--src" => {
                cli.src_pattern = args.next().map(|s| s.to_ascii_lowercase())
            }
            "--argc" => cli.argc = args.next().and_then(|s| s.parse().ok()),
            "--stats" => cli.stats = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => {
                if let Some(rest) = other.strip_prefix("--input=") {
                    cli.input = Some(std::path::PathBuf::from(rest));
                } else if let Some(rest) = other.strip_prefix("--fn=") {
                    cli.fn_pattern = Some(rest.to_ascii_lowercase());
                } else if let Some(rest) = other.strip_prefix("--caller-src=") {
                    cli.src_pattern = Some(rest.to_ascii_lowercase());
                } else if let Some(rest) = other.strip_prefix("--src=") {
                    cli.src_pattern = Some(rest.to_ascii_lowercase());
                } else if let Some(rest) = other.strip_prefix("--argc=") {
                    cli.argc = rest.parse().ok();
                }
                // Silently ignore unknown args.
            }
        }
    }
    cli
}

fn print_help() {
    let help = "\
lua_trace_filter — filter lua_trace.ndjson records by function or caller source.

Usage: lua_trace_filter [OPTIONS]

Options:
  -i, --input <PATH>         Read from PATH instead of stdin.
      --fn <SUBSTR>          Keep records whose `fn` contains SUBSTR (case-insensitive).
      --caller-src <SUBSTR>  Keep records whose `caller_src` contains SUBSTR.
      --src <SUBSTR>         Alias for --caller-src.
      --argc <N>             Keep records whose `argc` == N exactly.
      --stats                Print `kept / total` count to stderr on exit.
  -h, --help                 Print this help.

Filters compose with AND; no filters keeps every record. Non-JSON lines (`_log`/`warn` sidecars)
are passed through only when no filters are set.
";
    print!("{help}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cli(fn_p: Option<&str>, src_p: Option<&str>, argc: Option<i64>) -> Cli {
        Cli {
            input: None,
            fn_pattern: fn_p.map(|s| s.to_ascii_lowercase()),
            src_pattern: src_p.map(|s| s.to_ascii_lowercase()),
            argc,
            stats: false,
        }
    }

    fn v(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn empty_filters_keep_everything() {
        let cli = make_cli(None, None, None);
        assert!(keep(Some(&v(r#"{"fn":"Pg.Spawn"}"#)), &cli));
        assert!(keep(Some(&v(r#"{"fn":"Ai.Goal","caller_src":"@x:1"}"#)), &cli));
    }

    #[test]
    fn fn_filter_is_case_insensitive_substring() {
        let cli = make_cli(Some("pg.spawn"), None, None);
        assert!(keep(Some(&v(r#"{"fn":"Pg.Spawn"}"#)), &cli));
        assert!(keep(Some(&v(r#"{"fn":"Pg.SpawnDelayed"}"#)), &cli));
        assert!(!keep(Some(&v(r#"{"fn":"Ai.Goal"}"#)), &cli));
    }

    #[test]
    fn caller_src_filter_matches_the_field_and_only_it() {
        let cli = make_cli(None, Some("wifpmcinterior"), None);
        assert!(keep(
            Some(&v(r#"{"fn":"Pg.Spawn","caller_src":"@wifpmcinterior:214"}"#)),
            &cli
        ));
        // The pattern must not accidentally match against `fn` — this rec has the pattern
        // ONLY in `fn`, not `caller_src`, and must be dropped.
        assert!(!keep(
            Some(&v(r#"{"fn":"wifpmcinterior.Fn","caller_src":"@other:1"}"#)),
            &cli
        ));
    }

    #[test]
    fn filters_compose_with_and() {
        let cli = make_cli(Some("pg."), Some("fio"), None);
        assert!(keep(
            Some(&v(r#"{"fn":"Pg.Spawn","caller_src":"@fiodef001:5"}"#)),
            &cli
        ));
        // fn matches, src does not:
        assert!(!keep(
            Some(&v(r#"{"fn":"Pg.Spawn","caller_src":"@vzacon:5"}"#)),
            &cli
        ));
        // src matches, fn does not:
        assert!(!keep(
            Some(&v(r#"{"fn":"Ai.Goal","caller_src":"@fiodef001:5"}"#)),
            &cli
        ));
    }

    #[test]
    fn argc_filter_is_exact() {
        let cli = make_cli(None, None, Some(4));
        assert!(keep(Some(&v(r#"{"fn":"Pg.Spawn","argc":4}"#)), &cli));
        assert!(!keep(Some(&v(r#"{"fn":"Pg.Spawn","argc":5}"#)), &cli));
        // A record with no argc field never matches a numeric argc filter.
        assert!(!keep(Some(&v(r#"{"fn":"Pg.Spawn"}"#)), &cli));
    }

    #[test]
    fn non_json_lines_pass_through_when_no_filters_and_get_dropped_otherwise() {
        let no_filter = make_cli(None, None, None);
        assert!(keep(None, &no_filter));
        let with_filter = make_cli(Some("pg"), None, None);
        assert!(!keep(None, &with_filter));
    }

    #[test]
    fn missing_caller_src_field_is_treated_as_empty_string() {
        // A record without `caller_src` matches only when the filter's pattern is empty; a
        // non-empty pattern must reject it (contains-check against "" returns true only for
        // "", which the parser stores as `Some("")`; here the pattern is a real substring so
        // the record drops).
        let cli = make_cli(None, Some("fiodef"), None);
        assert!(!keep(Some(&v(r#"{"fn":"Pg.Spawn"}"#)), &cli));
    }
}
