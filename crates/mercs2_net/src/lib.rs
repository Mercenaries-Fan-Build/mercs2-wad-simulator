//! `mercs2_net` — Networking / replication (row 28), Layer-1 the reimpl target.
//!
//! **Silo 16** (`docs/modernization/reimplementation_parallelization_plan.md` §3).
//! **Scoreboard row(s):** 28.
//! **Code map:** `docs/reverse_engineer/networking_code_map.md`, with `event_bus_code_map.md` §4
//! (the shared bus the wire branch rides) and `ai_code_map.md` §2.2 (the `DirectAction` replicate gate).
//! **Owned Lua namespace(s):** `Net`.
//!
//! This crate reimplements **Layer 1 only — the game-side `Net*` replication / RPC layer** on the
//! recovered Keystone-B bus (networking map §10). Layers 2/3 (the Winsock peer mesh, the FESL/EA
//! services, the OpenSSL-0.9.8 TLS) are dead transports the map marks *replace-don't-port* (§8) and
//! the online-restore mod already stands in for; this crate does **not** depend on or reimplement them.
//!
//! What the engine *owns* here — and what this crate supplies — is the deterministic replication +
//! session mechanism:
//!
//! - [`message`] — the recovered wire record: a 32-bit name-hash + a typed-TLV arg stream (≤ 7),
//!   marshalled/unmarshalled at the modeled boundary (`NetEventCallback` §2.1, `FUN_005a0cc0` §2.2).
//! - [`session`] — the host/client role model + the recovered **max-2** player-slot table
//!   (`FUN_006cdac0`), and [`session::should_replicate`], the **host gate**.
//! - [`replicate`] — the local-vs-wire branch of the marshal core: local-post always, wire-marshal
//!   when the host gate fires (mirrors the AI bus's `DirectAction` → replicate-if-hosting).
//! - [`category`] — the `NetCategoryInfo` property-sync descriptor: the primary class + 8 `NetSubCat*`.
//! - [`module_pull`] — the join-time module pull (`SynchNetImportModule`, gate hash `0x762c8f61`).
//!
//! **Honest boundaries (what the map marks unrecovered/virtualized and this crate does NOT fabricate):**
//! the local-vs-wire *predicate* and the *encode/emit* steps of `FUN_005a0cc0` are SecuROM-virtualized
//! VM residue (`thunk_FUN_02ee0000` / `_02935000` / `_024f28e0`), read live — so [`message`] models the
//! *marshal boundary* (a self-consistent, round-trippable byte form preserving every recovered field),
//! not the exact virtualized on-wire bytes; the numeric category-nibble ↔ `NetSubCat*` mapping (data
//! behind `FUN_00644510`) is not recovered; the transport, matchmaking, and TLS/FESL wire are Layer-2/3
//! replace targets that live in the mod, not here.

pub mod category;
pub mod message;
pub mod module_pull;
pub mod replicate;
pub mod session;

pub use category::{NetCategory, NetChannel, CATEGORY_HASH_SEED, CATEGORY_POOL_SIZE};
pub use message::{NetArg, NetMessage, WireError, FRAME_SLOT_CAP, MAX_ARGS};
pub use module_pull::{pull_request, ModulePullState, IMPORT_MODULE_REGISTRY_HASH};
pub use replicate::{deliver, frame_has_room, route, Dispatch};
pub use session::{should_replicate, Role, Session, MAX_PLAYERS};

use mercs2_formats::hash::pandemic_hash_m2;

/// The host-owned networking mechanism — the session + the join-time module-sync state, the two pieces
/// the `Net.*` Lua surface drives through the game's `EngineHost` seam. `Net.SendCustomEvent` /
/// `Net.SendEvent_*` build messages and route them through the host gate here; the receive path
/// re-drives them onto the local bus after the module-pull gate.
///
/// This bundles the world-global net state the script host holds. Per-object replicated properties
/// (health / inventory / node-health via the [`NetCategory`] sub-cats) live on ECS components in the
/// `World`; this is the session-level spine that decides *what leaves the host and what a client pulls*.
pub struct NetWorld {
    /// The session role + max-2 player-slot state (`Net.IsServer`/`IsClient`, `FUN_006cdac0`).
    pub session: Session,
    /// Which host modules this peer has synced (the join-time pull gate, `SynchNetImportModule`).
    pub modules: ModulePullState,
}

impl NetWorld {
    /// Boot as **host** with the local player id (the `Net.StartServer` state).
    pub fn host(local_player: u32) -> NetWorld {
        NetWorld { session: Session::host(local_player), modules: ModulePullState::new() }
    }

    /// Boot as **client** joined to `host_player` (the post-`Net.ConnectToServer` state). A joining
    /// client has synced nothing yet — every channel's first inbound event triggers a module pull (§4).
    pub fn client(host_player: u32, local_player: u32) -> NetWorld {
        NetWorld { session: Session::client(host_player, local_player), modules: ModulePullState::new() }
    }

    /// `Net.SendCustomEvent(channel, id, {args})` — build the per-mission RPC and route it through the
    /// host gate (§Appendix A). The event name-hash is `pandemic_hash_m2(channel)` (the channel name,
    /// e.g. `"MrxFactionManager"`); the `NETEVENT_*` id rides as the first `Int` arg, then the caller's
    /// args — so the receiver reconstructs `NetEventCallback(id, tArgs)` (§4).
    ///
    /// Returns [`Dispatch::Wire`] with the marshalled bytes when hosting a client (replicated), else
    /// [`Dispatch::LocalOnly`]. Faithful to `DirectAction`: local-post always, wire only when the host
    /// gate fires.
    pub fn send_custom_event(
        &self,
        channel: &str,
        event_id: i32,
        args: &[NetArg],
    ) -> Result<Dispatch, WireError> {
        let mut all = Vec::with_capacity(args.len() + 1);
        all.push(NetArg::Int(event_id));
        all.extend_from_slice(args);
        let msg = NetMessage::new(pandemic_hash_m2(channel), 0, all);
        route(&self.session, &msg)
    }

    /// Receive an inbound wire packet (`NetEventCallback`, §2.1). Runs the **module-pull gate first**
    /// (§2.1 line (a)): the packet's channel/module hash is the event name-hash — if this peer has not
    /// synced that module, delivery is deferred and the module-**pull request** is returned in
    /// [`Received::PullFirst`]; the caller emits it to the host, applies the module state, calls
    /// [`NetWorld::mark_module_synced`], then re-delivers. If already synced, the decoded message is
    /// returned in [`Received::Deliver`] to re-drive the local bus.
    pub fn receive(&self, bytes: &[u8]) -> Result<Received, WireError> {
        let msg = deliver(bytes)?;
        match self.modules.gate_inbound(msg.name_hash) {
            Some(pull) => Ok(Received::PullFirst { pull, deferred: msg }),
            None => Ok(Received::Deliver(msg)),
        }
    }

    /// Mark a channel/module as synced once its host snapshot has been applied (drives the pull gate).
    pub fn mark_module_synced(&mut self, module_hash: u32) {
        self.modules.mark_synced(module_hash);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `Net.SendCustomEvent` surface roundtrips through `NetWorld`: a host with a client marshals
    /// the RPC to the wire with the channel-hash name and the `NETEVENT_*` id as the first arg; the
    /// client decodes it back byte-for-byte.
    #[test]
    fn host_send_custom_event_replicates_and_decodes() {
        let mut host = NetWorld::host(0x1000);
        host.session.join(0x2000);

        let channel = "MrxFactionManager";
        let dispatch = host
            .send_custom_event(channel, 42, &[NetArg::Handle(0xBEEF)])
            .unwrap();

        let bytes = match dispatch {
            Dispatch::Wire(b) => b,
            Dispatch::LocalOnly => panic!("host + client must replicate the custom event"),
        };

        let decoded = deliver(&bytes).unwrap();
        assert_eq!(decoded.name_hash, pandemic_hash_m2(channel));
        assert_eq!(decoded.args[0], NetArg::Int(42));
        assert_eq!(decoded.args[1], NetArg::Handle(0xBEEF));
    }

    /// A solo host keeps the event local (no client to replicate to).
    #[test]
    fn solo_host_custom_event_is_local() {
        let host = NetWorld::host(0x1000);
        assert_eq!(
            host.send_custom_event("WifPmcInterior", 1, &[]).unwrap(),
            Dispatch::LocalOnly
        );
    }

    /// A joining client's first packet on an unsynced channel gates to a module pull, then delivers
    /// once the module is marked synced — the full join-time module-pull sequence (§4).
    #[test]
    fn joining_client_pulls_module_before_delivering() {
        let mut client = NetWorld::client(0x1000, 0x2000);
        let channel = "MrxBriefing";

        // The host marshals an event on the channel.
        let mut host = NetWorld::host(0x1000);
        host.session.join(0x2000);
        let bytes = match host.send_custom_event(channel, 7, &[]).unwrap() {
            Dispatch::Wire(b) => b,
            Dispatch::LocalOnly => unreachable!(),
        };

        // Client has synced nothing → gate returns a pull request for this channel.
        match client.receive(&bytes).unwrap() {
            Received::PullFirst { pull, deferred } => {
                assert_eq!(pull.name_hash, IMPORT_MODULE_REGISTRY_HASH);
                assert_eq!(pull.args, vec![NetArg::Handle(pandemic_hash_m2(channel))]);
                assert_eq!(deferred.args[0], NetArg::Int(7));
            }
            Received::Deliver(_) => panic!("first packet on an unsynced module must pull first"),
        }

        // After applying the host's module snapshot, the packet delivers.
        client.mark_module_synced(pandemic_hash_m2(channel));
        match client.receive(&bytes).unwrap() {
            Received::Deliver(msg) => assert_eq!(msg.args[0], NetArg::Int(7)),
            Received::PullFirst { .. } => panic!("synced module must deliver"),
        }
    }

    /// Sanity: `mercs2_core` links (kept from the scaffold so the dependency stays exercised).
    #[test]
    fn core_dependency_links() {
        let _ = mercs2_core::Time::new(60.0);
    }
}

/// The result of [`NetWorld::receive`] — either deliver the decoded message to the local bus, or pull
/// the host's module state first (the join-time gate, §4).
#[derive(Clone, Debug, PartialEq)]
pub enum Received {
    /// The module is synced — re-drive this message onto the local bus (`NetEventCallback` → shared bus).
    Deliver(NetMessage),
    /// The message's module is unsynced — emit `pull` to the host, apply the returned snapshot, mark
    /// the module synced, then re-deliver `deferred`.
    PullFirst {
        /// The `NetSynchImportModule` pull request to send to the host.
        pull: NetMessage,
        /// The inbound message held back until the module state arrives.
        deferred: NetMessage,
    },
}
