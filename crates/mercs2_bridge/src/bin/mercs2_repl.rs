//! `mercs2_repl` — a headless, scriptable REPL client for the Mercenaries 2 live bridge.
//!
//! This is the W0 "live parity harness" client from the faithful-Tier-1 plan. It speaks the exact
//! `<<<RUN>>>`/`<<<END>>>` loopback protocol on `127.0.0.1:27050`, so the SAME invocation drives:
//!   * the **retail** game running the community `lua-bridge-DEV` ASI, and
//!   * our **reimpl** (`mercs2_game`) which serves the identical protocol via `bridge_host`.
//!
//! That is the whole point: an agent runs one Lua chunk against retail, runs it against the reimpl,
//! and diffs the two answers. Faithful = the answers match. The tkinter `lua_console.py` GUI can't be
//! driven from a script; this can.
//!
//! # Usage
//! ```text
//! mercs2_repl --code 'return 1+1'                     # eval against the default port (27050)
//! mercs2_repl --code 'return Player.GetCash()'        # read live retail state
//! echo 'return Ess.VERSION' | mercs2_repl             # chunk from stdin
//! mercs2_repl --addr 127.0.0.1:27060 --code '...'     # a reimpl bound to an alt port (side-by-side)
//! mercs2_repl --probe                                 # is a bridge listening? exit 0=up / 1=down
//! mercs2_repl --ab 'return Player.GetCash()' \        # A/B: retail vs reimpl in one shot
//!             --retail 127.0.0.1:27050 --reimpl 127.0.0.1:27060
//! ```
//! Exit code: `0` on success (or `--probe` up / `--ab` match), `1` on error / down / mismatch.

use std::io::Read;
use std::process::ExitCode;

use mercs2_bridge::{Bridge, DEFAULT_ADDR};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let addr = flag(&args, "--addr").unwrap_or_else(|| DEFAULT_ADDR.to_string());

    // --probe: connect-and-drop, report up/down via exit code (the launch.py --status equivalent).
    if args.iter().any(|a| a == "--probe") {
        return match Bridge::connect_at(&addr) {
            Ok(_) => {
                println!("[repl] bridge UP at {addr}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                println!("[repl] bridge DOWN at {addr}: {e}");
                ExitCode::FAILURE
            }
        };
    }

    // --ab <chunk>: run the same chunk against retail and reimpl, diff the answers.
    if let Some(chunk) = flag(&args, "--ab") {
        let retail = flag(&args, "--retail").unwrap_or_else(|| DEFAULT_ADDR.to_string());
        let reimpl = flag(&args, "--reimpl").unwrap_or_else(|| "127.0.0.1:27060".to_string());
        return ab(&chunk, &retail, &reimpl);
    }

    // Otherwise: eval one chunk (from --code or stdin) and print the result.
    let chunk = match flag(&args, "--code") {
        Some(c) => c,
        None => read_stdin(),
    };
    if chunk.trim().is_empty() {
        eprintln!("[repl] no chunk given — pass --code '<lua>' or pipe one on stdin (see --help)");
        return ExitCode::FAILURE;
    }

    match eval(&addr, &chunk) {
        Ok(out) => {
            print!("{out}");
            if !out.ends_with('\n') {
                println!();
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[repl] {addr}: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Connect, eval one chunk, return the printed result. A trailing newline is appended to the chunk so
/// a chunk ending in a `--` line comment can't swallow the RUN marker (matches `lua_console.py`).
fn eval(addr: &str, chunk: &str) -> Result<String, mercs2_bridge::BridgeError> {
    let mut bridge = Bridge::connect_at(addr)?;
    bridge.eval(&format!("{}\n", chunk.trim_end_matches('\n')))
}

/// A/B one chunk against two endpoints and diff. Exit 0 iff both answered and the answers are equal.
fn ab(chunk: &str, retail: &str, reimpl: &str) -> ExitCode {
    let r = eval(retail, chunk);
    let m = eval(reimpl, chunk);
    match (&r, &m) {
        (Ok(a), Ok(b)) => {
            let (a, b) = (a.trim_end(), b.trim_end());
            println!("retail [{retail}]: {a}");
            println!("reimpl [{reimpl}]: {b}");
            if a == b {
                println!("[repl] MATCH");
                ExitCode::SUCCESS
            } else {
                println!("[repl] MISMATCH");
                ExitCode::FAILURE
            }
        }
        _ => {
            if let Err(e) = &r {
                eprintln!("[repl] retail [{retail}]: {e}");
            }
            if let Err(e) = &m {
                eprintln!("[repl] reimpl [{reimpl}]: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

fn read_stdin() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}

/// The value after `name`, if present and not itself a flag.
fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .filter(|s| !s.starts_with("--"))
        .cloned()
}

fn print_help() {
    println!(
        "mercs2_repl — headless client for the Mercenaries 2 live bridge (127.0.0.1:27050)\n\n\
         USAGE:\n  \
         mercs2_repl --code '<lua>'            eval a chunk (default addr {DEFAULT_ADDR})\n  \
         mercs2_repl                            eval a chunk read from stdin\n  \
         mercs2_repl --addr HOST:PORT --code .. target a non-default endpoint (e.g. an alt-port reimpl)\n  \
         mercs2_repl --probe                    exit 0 if a bridge is listening, 1 if not\n  \
         mercs2_repl --ab '<lua>' [--retail A] [--reimpl B]   diff retail vs reimpl for one chunk\n\n\
         Same binary talks to retail (lua-bridge-DEV ASI) and the reimpl (bridge_host) — identical protocol."
    );
}
