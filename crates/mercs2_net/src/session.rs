//! Session / player-slot state — the host/client role model and the recovered **max-2** slot table
//! (networking code map §5, AI code map §2.2).
//!
//! The player-count gate is `FUN_006cdac0` (Xbox `FUN_82590e28`): *"active slots, **max 2**"* (AI code
//! map §2.2 table). Mercenaries 2 co-op is a **two-player** title — the session holds at most two
//! occupied slots, one of which is the local player. Role (host vs client) is the `Net.IsServer` /
//! `Net.IsClient` gate every replicated op checks (§Appendix A). The transport that fills these slots
//! (the Winsock peer mesh `FUN_009cf970`/`FUN_009cfa10`, the FESL/Theater lobby) is a *replace* layer
//! (§8/§10); this is the deterministic in-memory session model the replication logic reads.

/// The recovered active-slot ceiling — `FUN_006cdac0` returns active slots, **max 2** (co-op is a
/// two-player title). The player-count gate that decides whether there is a peer to replicate to.
pub const MAX_PLAYERS: usize = 2;

/// The session role — the `Net.IsServer` / `Net.IsClient` gate (§Appendix A). Every host-authoritative
/// op (`NetSafe*` senders) checks this before it emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The host: authoritative simulator; the only role that replicates to clients (§2, §10).
    Host,
    /// A client: applies host pushes (`NetClient*`), never authoritative.
    Client,
}

/// The in-memory session — role plus the max-2 player-slot table. Slot 0 is the local player; when
/// hosting, the local player is the authoritative host in slot 0.
#[derive(Clone, Debug)]
pub struct Session {
    role: Role,
    /// Up to [`MAX_PLAYERS`] occupied slots, each holding a player id (guid). `None` = empty slot.
    slots: [Option<u32>; MAX_PLAYERS],
}

impl Session {
    /// Start a session as **host** with the local player id in slot 0 (a solo host — one active slot
    /// until a client joins). This is the `Net.StartServer` state.
    pub fn host(local_player: u32) -> Session {
        Session { role: Role::Host, slots: [Some(local_player), None] }
    }

    /// Start a session as **client** — slot 0 is the remote host, slot 1 the local player. This is the
    /// post-join `Net.ConnectToServer` state (a client is never alone; the host it joined fills slot 0).
    pub fn client(host_player: u32, local_player: u32) -> Session {
        Session { role: Role::Client, slots: [Some(host_player), Some(local_player)] }
    }

    /// The session role (`Net.IsServer`/`Net.IsClient`).
    pub fn role(&self) -> Role {
        self.role
    }

    /// Whether this peer is the host (`Net.IsServer`).
    pub fn is_host(&self) -> bool {
        self.role == Role::Host
    }

    /// Whether this peer is a client (`Net.IsClient`).
    pub fn is_client(&self) -> bool {
        self.role == Role::Client
    }

    /// The number of active slots — the `FUN_006cdac0` player count (0..[`MAX_PLAYERS`]).
    pub fn player_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// The player id in each occupied slot, in slot order.
    pub fn players(&self) -> impl Iterator<Item = u32> + '_ {
        self.slots.iter().filter_map(|s| *s)
    }

    /// A client joins — occupy the first free slot with `player`. Returns the slot index, or `None` if
    /// the session is already at the max-2 cap or the player is already present (the gate never seats a
    /// third peer). Faithful to the max-2 active-slot ceiling.
    pub fn join(&mut self, player: u32) -> Option<usize> {
        if self.slots.iter().any(|s| *s == Some(player)) {
            return None;
        }
        let idx = self.slots.iter().position(|s| s.is_none())?;
        self.slots[idx] = Some(player);
        Some(idx)
    }

    /// A player leaves — free its slot. Returns whether the player was present.
    pub fn leave(&mut self, player: u32) -> bool {
        if let Some(slot) = self.slots.iter_mut().find(|s| **s == Some(player)) {
            *slot = None;
            true
        } else {
            false
        }
    }
}

/// The **host gate** — the local-vs-wire replicate predicate, modeled faithfully (§2, AI code map
/// §2.2). A locally-posted action becomes a wire message **only** when this peer is the host *and*
/// there is a client to send to. `DirectAction` local-posts, then replicates *if hosting*
/// (`FUN_0056aa70`); the player-count gate (`FUN_006cdac0`, max 2) supplies the "is there a peer"
/// half — a solo host (player_count 1) has no client, so it does not marshal to the wire.
///
/// `is_host` = `Net.IsServer`; `player_count` = active slots (0..2). Replicate iff host with ≥ 2
/// active slots — the second slot being the client mirror.
pub fn should_replicate(is_host: bool, player_count: usize) -> bool {
    is_host && player_count > 1 && player_count <= MAX_PLAYERS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_starts_solo_then_seats_one_client() {
        let mut s = Session::host(0x1000);
        assert!(s.is_host());
        assert_eq!(s.player_count(), 1, "solo host = one active slot");
        assert_eq!(s.join(0x2000), Some(1));
        assert_eq!(s.player_count(), 2);
    }

    #[test]
    fn slot_table_caps_at_two() {
        let mut s = Session::host(1);
        assert_eq!(s.join(2), Some(1));
        assert_eq!(s.join(3), None, "no third slot — max 2");
        assert_eq!(s.player_count(), MAX_PLAYERS);
    }

    #[test]
    fn join_is_idempotent_per_player() {
        let mut s = Session::host(1);
        assert_eq!(s.join(2), Some(1));
        assert_eq!(s.join(2), None, "same player is not seated twice");
    }

    #[test]
    fn leave_frees_a_slot_for_a_new_join() {
        let mut s = Session::host(1);
        s.join(2);
        assert!(s.leave(2));
        assert!(!s.leave(2), "leaving twice reports absent");
        assert_eq!(s.player_count(), 1);
        assert_eq!(s.join(3), Some(1), "freed slot is reusable");
    }

    #[test]
    fn client_session_is_never_alone() {
        let s = Session::client(0x1000, 0x2000);
        assert!(s.is_client());
        assert_eq!(s.player_count(), 2);
        assert_eq!(s.players().collect::<Vec<_>>(), vec![0x1000, 0x2000]);
    }

    #[test]
    fn should_replicate_truth_table() {
        // host with a client → wire
        assert!(should_replicate(true, 2));
        // solo host → no peer → local only
        assert!(!should_replicate(true, 1));
        // client never replicates (never authoritative)
        assert!(!should_replicate(false, 2));
        assert!(!should_replicate(false, 1));
        // never beyond max-2
        assert!(!should_replicate(true, 3));
    }
}
