//! Progress lines for long-running simulation (always flushed to stderr).

use std::io::Write;

/// Log a progress line to stderr and flush immediately so consoles show activity.
pub fn log(msg: impl AsRef<str>) {
    eprintln!("{}", msg.as_ref());
    let _ = std::io::stderr().flush();
}

/// Log every `interval` invocations when `count` hits a multiple (and when count == 1).
pub fn log_every(count: usize, interval: usize, msg: impl Fn() -> String) {
    if count == 1 || interval > 0 && count % interval == 0 {
        log(msg());
    }
}
