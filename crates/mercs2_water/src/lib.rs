//! `mercs2_water` — Water (scoreboard row 7).
//!
//! **Code map:** `docs/reverse_engineer/water_code_map.md`.
//! **Scaffold** — the Wave-2 owner fills this crate with the faithful water mechanism (watermap query /
//! water height, TPS swim-state FSM, buoyancy from the `WaterDrag*` tunables), depending only on
//! `mercs2_core` + `mercs2_formats`. Render (the water pass) is a seam against `mercs2_engine`.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
    }
}
