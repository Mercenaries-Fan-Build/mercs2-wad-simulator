//! `mercs2_net` — Networking — Keystone-B replication, session/transport, FESL.
//!
//! **Silo 16** (`docs/modernization/reimplementation_parallelization_plan.md` §3).
//! **Scoreboard row(s):** 28.
//! **Code map:** `docs/reverse_engineer/networking_code_map.md`.
//! **Owned Lua namespace(s):** `Net`.
//! Reference impl: the online-restore mod (coopserver/tlsterm); depends on the event bus (Keystone B).
//!
//! **WAVE-1 SILO — scaffold only.** No subsystem logic lives here yet: this crate exists so the
//! Wave-1 owner can fill it without write-colliding on `mercs2_engine`/`mercs2_game` (the carve
//! rules, plan §4). It depends only on `mercs2_core` (ECS/events/time + the `PhysicsQuery` seam)
//! and `mercs2_formats`; it never depends on another leaf crate. The Wave-1 pass implements the
//! subsystem against the code map above with the exe as the oracle and zero stubbed Lua.

#[cfg(test)]
mod tests {
    /// The scaffold links and its `mercs2_core` dependency resolves. Replaced by real tests in Wave 1.
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
        assert_eq!(2 + 2, 4);
    }
}
