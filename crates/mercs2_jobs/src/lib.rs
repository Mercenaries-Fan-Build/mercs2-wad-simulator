//! `mercs2_jobs` — the Pimp job system (scoreboard row 15).
//!
//! **Code map:** `docs/reverse_engineer/pimp_job_system_code_map.md`.
//! **Scaffold** — the Wave-2 owner fills this crate with the faithful worker-pool + job-ring mechanism
//! (the engine's `Pimp*` parallel-work spine: enqueue job → guarded ring → worker drain), depending
//! only on `mercs2_core` + `mercs2_formats`.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
    }
}
