//! The event-bus-driven replicate hook — the local-vs-wire branch of the marshal core `FUN_005a0cc0`
//! (networking code map §2.2), modeled as the AI bus models `DirectAction` → local-post →
//! replicate-if-hosting.
//!
//! Every PC sender funnels through `FUN_005a0cc0`, whose recovered body branches on a predicate:
//!
//! ```c
//! cVar1 = thunk_FUN_02ee0000(param_2);   // routing predicate: 0 → LOCAL, nonzero → serialize
//! if (cVar1 == '\0') { /* LOCAL: append to the in-memory frame if it has room */ }
//! else { /* REMOTE: encode (thunk_FUN_02935000) → emit (thunk_FUN_024f28e0) */ }
//! ```
//!
//! An inbound packet re-enters the **same shared bus** (`FUN_8241d458`/`FUN_82420690`/`FUN_8256eb28`),
//! so a replicated op re-drives the joining client's local subscribers exactly as a local `Event.Post`
//! would (§2.1). That is the whole shape: **local-post always happens; the wire marshal is the
//! host-gated addendum.**
//!
//! **Honest boundary:** the predicate `thunk_FUN_02ee0000` and the encode/emit steps
//! `thunk_FUN_02935000`/`thunk_FUN_024f28e0` are **SecuROM-virtualized** (VM residue, read live —
//! §2.2/§9). This module models the *decision* (the host gate, [`crate::session::should_replicate`])
//! and the *marshal boundary* ([`NetMessage::marshal`]); it does not fabricate the virtualized encode
//! internals. The `param_2+8`/`+0xc` capacity math (`(end-cursor)>>3` free 8-byte slots, cap
//! [`FRAME_SLOT_CAP`]) is the one piece of the local branch that is byte-recovered, so the frame model
//! honors it.

use crate::message::{NetMessage, WireError, FRAME_SLOT_CAP};
use crate::session::{should_replicate, Session};

/// The outcome of routing a locally-posted message through the marshal core (§2.2). The local subscriber
/// run always happens (it is how both the sender and any inbound packet drive the shared bus); this
/// enum records what the **wire branch** did.
#[derive(Clone, Debug, PartialEq)]
pub enum Dispatch {
    /// The predicate took the LOCAL branch — no host/peer to replicate to. The message stays on the
    /// in-memory bus (solo host, or a client, or a non-replicated event).
    LocalOnly,
    /// The REMOTE branch — the host marshalled the message to these wire bytes for the client
    /// (`thunk_FUN_02935000` encode → `thunk_FUN_024f28e0` emit, modeled at the marshal boundary).
    Wire(Vec<u8>),
}

/// Route a locally-posted message the way `FUN_005a0cc0` does: the message is always available to local
/// subscribers; if the **host gate** ([`should_replicate`]) fires (host with a client), also marshal it
/// to the wire. This is the faithful local-vs-wire branch — the predicate `thunk_FUN_02ee0000` is
/// modeled by the recovered gate (host + peer present), not by the virtualized byte the exe tests.
///
/// Returns [`Dispatch::Wire`] with the marshalled bytes on the remote branch, else
/// [`Dispatch::LocalOnly`]. A marshal error (too many args) surfaces as `Err`.
pub fn route(session: &Session, msg: &NetMessage) -> Result<Dispatch, WireError> {
    if should_replicate(session.is_host(), session.player_count()) {
        Ok(Dispatch::Wire(msg.marshal()?))
    } else {
        Ok(Dispatch::LocalOnly)
    }
}

/// The receive-side counterpart — decode wire bytes back to a message that re-drives the local bus
/// (`NetEventCallback` → shared bus, §2.1). The join-time module-pull gate runs *before* this in
/// [`crate::NetWorld::receive`]; this is the raw unmarshal step.
pub fn deliver(bytes: &[u8]) -> Result<NetMessage, WireError> {
    NetMessage::unmarshal(bytes)
}

/// The in-memory event frame's free-slot check — the byte-recovered local-branch capacity math of
/// `FUN_005a0cc0`: a reserve of `need` 8-byte slots succeeds only while the frame holds ≤
/// [`FRAME_SLOT_CAP`] slots (`(end - cursor) >> 3`, cap `0x801`, §2.2). Modeled as: does appending
/// `need` slots to a frame already holding `used` stay within the budget.
pub fn frame_has_room(used_slots: usize, need: usize) -> bool {
    used_slots + need <= FRAME_SLOT_CAP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{NetArg, NetMessage};

    fn msg() -> NetMessage {
        NetMessage::new(0x51ee_8f14, 0x3, vec![NetArg::Int(1), NetArg::Handle(0xABCD)])
    }

    #[test]
    fn host_with_client_marshals_to_wire() {
        let mut s = Session::host(1);
        s.join(2);
        let m = msg();
        match route(&s, &m).unwrap() {
            Dispatch::Wire(bytes) => assert_eq!(deliver(&bytes).unwrap(), m),
            Dispatch::LocalOnly => panic!("host + client must replicate"),
        }
    }

    #[test]
    fn solo_host_stays_local() {
        let s = Session::host(1);
        assert_eq!(route(&s, &msg()).unwrap(), Dispatch::LocalOnly);
    }

    #[test]
    fn client_never_replicates() {
        let s = Session::client(1, 2);
        assert_eq!(route(&s, &msg()).unwrap(), Dispatch::LocalOnly);
    }

    #[test]
    fn frame_budget_matches_recovered_cap() {
        assert!(frame_has_room(0, FRAME_SLOT_CAP));
        assert!(frame_has_room(FRAME_SLOT_CAP - 1, 1));
        assert!(!frame_has_room(FRAME_SLOT_CAP, 1), "one past the 0x801 cap is refused");
    }
}
