//! `mercs2_decal` — Decals (scoreboard row 6).
//!
//! **Code map:** `docs/reverse_engineer/decal_code_map.md`.
//! **Scaffold** — the Wave-2 owner fills this crate with the faithful decal mechanism (decaltable
//! loader + decal instance pool / lifetime / fade management), depending only on `mercs2_core` +
//! `mercs2_formats`. Render (the decal draw pass) is a seam against `mercs2_engine`.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
    }
}
