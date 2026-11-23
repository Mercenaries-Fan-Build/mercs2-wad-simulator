# `corpus/` — the decompiled game-script corpus, vendored

The Lua the shipped game actually runs. It is the **behavioural spec** for the engine binding surface:
real call arities, real argument shapes, the clamps and control flow the bindings must satisfy. The
replay suites and `cargo run -p mercs2_script --example mission_lab` execute it directly.

## Why it is vendored

Two reasons, and the second is why it lives *here* specifically.

**Determinism.** The alternative — discovering a sibling checkout at a known-relative path — meant the
tests silently degraded to "0 [lua] lines" on any machine laid out differently, which is exactly how
`boot_flow_runs_real_game_lua` came to report a boot regression when the real problem was that it had
never found the scripts.

**It has to reach users.** `cargo package` only collects files under a crate's own directory, so a
workspace-root location would resolve for us and for nobody who depends on `mercs2_script` from the
registry. Inside the package, `CARGO_MANIFEST_DIR` points at the extracted `.crate` and downstream
consumers get the corpus with the dependency — no checkout of ours, no network, no configuration.

Cost is small: the whole package is **682 KiB compressed** (Lua is text and compresses ~6:1), against
crates.io's 10 MiB limit.

Locate it in code with `mercs2_script::corpus::root()` / `::roots()` / `::dlc_root()`. **Do not
hardcode a path to it.**

## Do not move this above `crates/`

It was briefly at the workspace root. That keeps it out of published crates, which is precisely the
failure: registry users then have no corpus and every corpus-driven path silently no-ops for them.

## Provenance

Carried forward from the corpus's own README so the origin is not lost in the copy.

| | |
|---|---|
| Source WAD | `game-files/vz.wad` — **byte-identical** to the game's deployed `data/vz.wad` (size `2565537792`, qsha256 head+tail `502e290f…863696f`) |
| Verified | Loads cleanly to 100% (`loadprobe` REACHED-WORLD). **Base game, no DLC** — no `dlccon*`/blitz/arena; base contracts (pmccon/gurcon/chicon) all present |
| Blocks | `blocks\VZ\resident_P000_Q3.block` (idx 3185), `scripts_vz_P000_Q3.block` (idx 3197), `shell.wad` block 17; `vz_guilayouts/` from `blocks\VZ\guilayouts_P000_Q3.block` |
| Decompiler | `unluac` on `lua51-mercs2` float bytecode |
| Coverage | **370 / 382**. The 12 failures are the empty `all_*` stubs (`all_weapons`, `all_vehicles`, …) plus 2 goto-heavy scripts — no content lost |
| Complete corpus | The research repo's later pass over every Lua chunk PC ships (commit `93719c0`, 2026-10-02, "complete-corpus Lua forensics") added `vz_missions/`, `vz_subtitles/`, `vz_hijacks/`, `english/`, `loading/` and 12 more `resident/` modules, and the PS3 DLC01 Lua in `mercs2-dlc-luacd/` |

## Layout

`mercs2-luacd/src/` and `mercs2-dlc-luacd/src/` are byte-identical copies of the research repo's
`docs/mercs2-luacd/src/` and `docs/mercs2-dlc-luacd/src/` (`diff -rq` clean).

- `mercs2-luacd/src/resident/` (240) — `Mrx*` engine/library modules + world-entity scripts
- `mercs2-luacd/src/vz/` (114) — contracts, jobs, tutorials, WIF data tables
- `mercs2-luacd/src/vz_missions/` (222) — the per-mission scripts
- `mercs2-luacd/src/vz_subtitles/` (36) — the `Subtitles_*` tables `MrxGuiCinematic.ShowMovie` imports,
  and `technov`
- `mercs2-luacd/src/vz_hijacks/` (29) — the vehicle hijack scripts
- `mercs2-luacd/src/shell/` (28) — front-end menu / shell GUI
- `mercs2-luacd/src/vz_guilayouts/` (4) — the HUD, binoculars, satellite and PDA layouts
  `MrxGuiManager.CreateGui` imports
- `mercs2-luacd/src/english/` (1), `mercs2-luacd/src/loading/` (1)
- `mercs2-dlc-luacd/src/` — the console DLC levels' Lua, one folder per level: `dlc01/` (36),
  `dlctest01/` (3). Not a module root of the retail script host ([`corpus::roots`] is the retail
  corpus alone); read through `corpus::dlc_root()`.

The category references that accompany this corpus (`01_support_economy_delivery.md` …
`08_audio_presentation.md`) were not copied — they are documentation, not test input, and live with the
code maps in the research repo.
