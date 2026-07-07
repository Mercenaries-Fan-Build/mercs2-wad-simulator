# mercs2_core — deferred improvements

Non-blocking improvements intentionally left for a later silo. Each is tagged
`[faithful-blocker: no]` — i.e. omitting it does NOT make the current behaviour less faithful to the
exe oracle; it is scope/quality, not correctness. Faithful blockers (things the exe does that we do
NOT yet, needed for parity) belong in the code maps' confirm-live lists, not here.

## Streaming / region cache / prop-LOD (Wave-0 S5)

- **Region catalog wiring** `[faithful-blocker: no]` — `streaming::RegionCache` (the CacheIn/CacheOut
  decision layer) is built + unit-tested and delegated from `StreamingManager`, but no real regions
  are registered yet. They come from the `SphereRegion`/`CircleRegion`/`LineRegion` +
  `PopulationDensity` COMPs (descriptors `FUN_00641e10`/`FUN_004d60e0`), whose field-schema parsers
  live in `mercs2_formats` (out of S5's edit scope). When that parser exists, feed the rects +
  priorities into `mgr.add_region(...)` in `worldutil::build_streaming_catalog`. Not a fidelity gap
  today: without a population system there is nothing to cache in/out.

- **Full `PopulationSystem`** `[faithful-blocker: no]` — the ambient spawner families
  (Window/NoModel/Hardpoint/Path), the death check/compute pair, density decay, and the spawn-queue
  drain (`population_spawner_code_map.md` §4/§6, scoreboard row 24). The region cache added here is
  the streaming-side slice; the spawner half is a distinct, larger silo.

- **Configurable crowd / lump sizes** `[faithful-blocker: no]` — expose the ambient-density budgets
  (DensityUpdate 10/10/2/2, DeathCheck 20/frame) as tunables once the population system exists. Pure
  ergonomics; the exe's values are fixed constants.

- **Global LOD-budget thresholds from live capture** `[faithful-blocker: partial]` — the
  `GlobalLodGovernor` (engine `FUN_0084ae70`) mechanics (tier 0..3, immediate climb, banded +
  step-rate-limited drop, broadcast-on-change) are faithful, but the three concrete pressure
  thresholds (`DAT_00ce8dd8`/`PTR_DAT_00ce8ddc`/`PTR_FUN_00ce8de0`) + band (`DAT_00ce8dd4`) are not
  read from the decomp — the normalized defaults are a stand-in. Reading them (x32dbg, read-only
  while PAUSED) tightens fidelity; the composition (global tier = coarseness floor on the per-entity
  `HibernationControl` tier) is a reasoned choice pending live confirmation of the broadcast
  application.

- **Kept-ring capacity 8 vs 64** `[faithful-blocker: no]` — `RegionCacheConfig::kept_ring` defaults
  to the PC build's 8-slot ring (`DAT_00ed55d4`, cursor `& 7`); the Xbox build keeps 64. The
  discrepancy is an open confirm-live item (population_spawner_code_map §5). Configurable already; the
  default matches the PC oracle.
