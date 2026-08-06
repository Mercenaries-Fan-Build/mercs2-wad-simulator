# Open doors — research decoded, code still on a bootstrap

> ## ★ EMPIRICAL VERDICT (2026-08-05) — this supersedes the doc-cited list below
> The 3-domain audit was DOC-CITED (unreliable). Every candidate was then re-verified by
> PARSING it out of the real `vz.wad`. Real numbers only. Ground-truth open doors:
>
> | Open door | Empirical evidence | Verdict |
> |---|---|---|
> | Static-world `WpMeshShape16`/MOPP collision | 4380 mesh + 4380 MOPP + 4380 MoppCode on disk; decoded to EMPTY markers | **REAL — biggest**; needs 16-bit mesh decode |
> | PHY2 convex hulls → break-piece collision | 18,638 hulls parse (crate 0x81C71C96 → 6 hulls); **zero runtime consumers** (grep-confirmed) | **REAL — cleanest**; parser done, just wire |
> | Terrain per-vertex splat weights | 47/60 tiles carry varied D3DCOLOR weights; `load_terrainmesh_tile` white-outs them | **REAL — visible** |
> | AnimationTransition `0xAB8FE34B` | exists blk 3185 type 0x207359C7, INFO count=497, VALU=497 rows; parses via generic UCFX machinery; no consumer | **REAL** |
> | Spot lights (streaming path) | exactly 14 `Light_spot_large_yellow`; decode fine; only streaming/free-fly path drops them | **REAL but small** |
>
> **REFUTED (doc-only ghosts, deleted):** LightAnimation — **0** such COMPs exist in the WAD.
> Texture high-mip rungs — already implemented (`extract_texture_hires`, 9/12 tank textures up
> to 32×; audit cited a nonexistent fn). Also: the empirical agent falsely called convex hulls
> "consumed/closed" — grep proved zero runtime consumers, so it stays a REAL open door.
>
> **Standard going forward:** a claim is a real open door only if a parser produces the claimed
> structure from `vz.wad`. Doc "SOLVED" citations do not count.

---
(Provisional doc-cited audit below — kept for the file:line pointers; trust the verdict table above.)

Result of a 3-domain audit (2026-08-05): cross-referenced corpus findings marked
proven/decoded/SOLVED against the Rust code, keeping ONLY cases where the research is
genuinely ahead of the code (decoded, consumer often already built, just not wired) — not
items correctly blocked on a live/x32dbg read. The prompt's example (collision from render
tris vs authored PHY2 shapes) is the flagship.

## FLAGSHIP — collision from authored PHY2 Havok shapes (not render-mesh triangles)
The whole collision path derives triangles from the render mesh (`model.rs::extract_local_tris`,
`game_world.rs::placed_tris`); `model.rs:10` marks PHY2 as absent in the consumer. Retail ships
each asset's collision as a `PHY2` Havok 5.5 packfile (heightfield / MOPP mesh / convex hull).
- **Convex hulls (break pieces): fully parsed + tested** (`havok.rs::parse_phy2_body`, test
  `crate_phy2_decodes_six_break_piece_hulls`) — but the ONLY caller is the offline OBJ tool
  (`havok_extract`). **Cleanest untouched-parser case.** → feed to break-piece collision.
- **Terrain heightfield** — retail `hkpSampledHeightFieldShape`. We now use a render-derived
  heightfield (this session) — close, but the authored samples are the faithful source.
- **MOPP / `WpMeshShape16`** (static-world building collision) — `havok.rs:364` decodes these as
  EMPTY markers. Partly a PARSER gap (the 16-bit tree/index layout is the on-disk decode item),
  not pure research-ahead.

## HIGH — clean open doors (decoded AND consumer built; one missing wire)
1. **Spot/cone lights dropped on the streaming path.** `placement.rs` fully decodes `LightObject`
   spot lights (14 `Light_spot_*`, cone_inner/outer, cone_axis — all 14 witnessed); `scene.rs`
   has the full `_sl` cone shader + `set_spot_lights` (tested). But `game_world.rs::into_streaming_world`
   (~1000) harvests only points (spots filtered at ~726). The `mercs2_game::world` static path DOES
   wire them (world.rs:572,583,1878). → call `placed_spot_lights_to_gpu` + `set_spot_lights` on the
   streaming path. `DEFERRED.md:86` ("cone floats not decoded") is STALE. *Visible: 14 searchlights.*
2. **Terrain multi-layer splat blend.** The per-vertex `D3DCOLOR` splat WEIGHTS (≤4 detail layers)
   are decoded; `game_world.rs:80` binds ONE representative layer and `:122` **white-outs the decoded
   weights** (the black-terrain fix this session — a stand-in). → bind all ≤4 layers + blend by the
   per-vertex weights in the terrain shader. *Visible: flat material per region vs authored blend.*
3. **AnimationTransition table** (`0xAB8FE34B`, 497 rows) decoded + parseable by the EXISTING
   `anim_select` `0x207359C7` machinery; `controller.rs:54` uses a fixed `ANIM_BLEND_SEC=0.2` for
   every transition. → add a row accessor, feed `TransitionDuration`/`TransitionType`(snap/via-clip)/
   `TransitionAnimation`. `faithful-blocker: yes`. *Visible: transition pops, no via-clip bridges.*

## MEDIUM
4. **PHY2 convex hulls → break-piece collision** (the flagship's cleanest sub-item, see above).
5. **Hardpoint-attached FX emitters.** Bridge decoded (`FUN_004D28C0(guid, hardpointHash, effectHash)`,
   hardpoint = HIER node-name hash — reimpl already resolves those). Effect templates ARE adopted;
   attachment isn't (`rendering_fx_lighting_gap.md §E ❌`). → pin (hardpoint,effect) to the HIER node.
   *Visible: exhaust/muzzle/jet FX not on their nodes.*
6. **`LightAnimation` tween parse-half.** Descriptor layout decoded (`FUN_00662ee0`, 0x2c); the tween
   runtime is built + tested (`scene.rs::LightAnim`/`set_light_animations`), but no parser feeds it.
   → parse the COMP into `LightAnim` slots (apply math key→amp/freq is VMX = the still-blocked half).
   *Visible: torches/pulsing signage frozen.*
7. **Texture high-mip on-demand upgrade.** Residency descriptor + P000/P001/P002 rungs decoded;
   `wad.rs::extract_full_res` assembles finer mips. No camera-distance-driven swap. *Visible/perf:
   near props/terrain stay coarse.*
8. **Water reflection/surface passes.** Pass chain decoded (`RenderReflections FUN_004677d0`
   mirror-matrix, rated high); `render_graph.rs` registers them as no-op seams. *Visible: no
   reflections/wake; also the floating white/red water slabs are here.*

## PERF — decoded + designed, not built
9. **Hardware instancing.** shader3 container/CTAB/`objectData` splice "fully cracked" + tooled
   (`shader3.rs`/`shaderforge.rs`); the runtime instance-buffer + DIP coalescer is designed, NOT
   built (`density_upgrade_state.md` M4). Props/crowds draw one-per-object. *This is the decoded
   answer to the draw-call cost.*

## Half-open (decoded value, one narrow live confirm)
- Vehicle speed/susp/friction/brake — recovered engine-unit defaults in `tuning.rs`; `default()`
  uses model-scale substitutes; gated on the engine→SI unit factor (`bp 0x0044a970`).
- Destruction damage band 0.5 — threshold global `DAT_00b97744` address located, value not yet decoded.

## Stale docs to correct
- `mercs2_vehicle/DEFERRED.md` ("placeholders, names stripped") — `tuning.rs` already recovered them.
- `mercs2_engine` lighting `DEFERRED.md:86` (spot cone floats "not decoded") — decoded in `placement.rs`.

## Verified NOT gaps (correctly confirm-live or already adopted — do not chase)
Physics integrator/gravity (VMX), ragdoll masses/limits (not in shipped data), combat solver constants
(WildStar algo adopted, constants live), weapon per-weapon overrides (live CopyFromStream), AI
perception/relations (adopted), population density budgets 10/10/2/2 (adopted), region rect extents
(live), water wave amp/freq (VMX), sun/TOD (script-wiring not decode), decal projection params (live),
dynamic music (infra wiring not decode). Already-closed: anim clip playback, full-quat placements,
destruction state machine, effect templates.
