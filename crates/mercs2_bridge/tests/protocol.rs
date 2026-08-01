//! Does the bridge speak the ASI's wire protocol correctly?
//!
//! A mock server stands in for the game: it accepts one connection, reads a request up to the
//! `<<<RUN>>>` terminator, and replies `<result><<<END>>>`. That is the exact framing
//! `lua_bridge_DEV.c` uses, so a round-trip here proves the client half without a running game — the
//! only part a live game would add is that the chunk actually executes.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use mercs2_bridge::{Bridge, BridgeError};

/// A one-shot mock REPL. Returns its bound address and a handle. `respond` maps the received chunk
/// (with the `<<<RUN>>>` marker stripped) to the result body the server sends back before `<<<END>>>`.
fn mock_repl(
    respond: impl Fn(&str) -> String + Send + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let addr = listener.local_addr().unwrap().to_string();
    let handle = thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("accept");
        // Read until we see the RUN terminator.
        let mut buf = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let n = sock.read(&mut chunk).expect("read");
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(9).position(|w| w == b"<<<RUN>>>") {
                let request = String::from_utf8_lossy(&buf[..pos]).into_owned();
                let body = respond(&request);
                sock.write_all(body.as_bytes()).unwrap();
                sock.write_all(b"<<<END>>>").unwrap();
                sock.flush().unwrap();
                return;
            }
        }
    });
    (addr, handle)
}

/// The headline: send a chunk, get its result back, framing handled.
#[test]
fn a_chunk_round_trips_through_the_run_end_framing() {
    // The mock "evaluates" by echoing the chunk uppercased — enough to prove request and response
    // both crossed intact.
    let (addr, handle) = mock_repl(|req| format!("=> {}", req.to_uppercase()));
    let mut bridge =
        Bridge::connect(&addr, Duration::from_millis(0), Duration::from_secs(2)).expect("connect");
    let out = bridge.eval("return 1+1").expect("eval");
    assert_eq!(out, "=> RETURN 1+1", "the result must come back without the END marker");
    drop(bridge);
    handle.join().unwrap();
}

/// The result is returned with the `<<<END>>>` terminator stripped, even when it arrives glued to
/// the marker in one packet.
#[test]
fn the_end_marker_is_stripped_from_the_result() {
    let (addr, handle) = mock_repl(|_| "3".to_string());
    let mut bridge = Bridge::connect_at(&addr).expect("connect");
    let out = bridge.eval("return 1+2").expect("eval");
    assert_eq!(out, "3");
    assert!(!out.contains("<<<END>>>"));
    drop(bridge);
    handle.join().unwrap();
}

/// A dead port fails as `Connect`, promptly — the "game isn't running" case, which must not hang.
#[test]
fn a_closed_port_is_a_connect_error_not_a_hang() {
    // 127.0.0.1:1 is reserved and never listening.
    match Bridge::connect("127.0.0.1:1", Duration::from_millis(0), Duration::from_secs(1)) {
        Err(BridgeError::Connect(_)) => {}
        other => panic!("expected a Connect error, got {other:?}"),
    }
}

/// A server that accepts but never answers must time out, not block forever — a frozen game.
#[test]
fn a_silent_game_times_out() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let handle = thread::spawn(move || {
        let (_sock, _) = listener.accept().unwrap();
        // Hold the connection open, answer nothing, until the client gives up.
        thread::sleep(Duration::from_millis(600));
    });
    let mut bridge =
        Bridge::connect(&addr, Duration::from_millis(0), Duration::from_millis(150)).expect("connect");
    match bridge.eval("hang()") {
        Err(BridgeError::Timeout) => {}
        other => panic!("expected Timeout, got {other:?}"),
    }
    drop(bridge);
    let _ = handle.join();
}
