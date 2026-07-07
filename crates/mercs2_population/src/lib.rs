//! `mercs2_population` — Population / spawners (scoreboard row 24).
//!
//! **Code map:** `docs/reverse_engineer/population_spawner_code_map.md`.
//! **Scaffold** — the Wave-2 owner fills this crate with the faithful spawner/density mechanism
//! (SkirmishSpawnList selection, PopulationDensity crowd/traffic budgets, dynamic-road flow), depending
//! only on `mercs2_core` + `mercs2_formats`. Feeds AI actors through the game's spawn resolver (a seam).

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
    }
}
