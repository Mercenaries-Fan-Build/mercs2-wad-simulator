# mercs2_faction

Silo 13 of the Mercenaries 2 reimplementation: the faction / reputation / pursuit ("heat") mechanism —
the combat → infraction → attitude → pursuit loop recovered in `faction_reputation_code_map.md`.

## What it is

The host-side faction mechanism. Hostile acts against a faction accrue into a per-faction **7-key
infraction accumulator**; when that faction "reports" you the accumulator is weighted into a single
**mood**, which becomes a delta on that faction's relation toward the PMC in `[-100, 100]`; the relation
classifies into an **attitude level** (Hostile / Neutral / Friendly) that drives shop prices, the HUD
meter and colour; and a relation that bottoms out at `-100` escalates that faction's **pursuit level**
(`0..=3`), which then auto-decays on a per-level dwell timer.

The whole loop lives behind one type, `FactionWorld`:

* `add_infraction` / `add_special_infraction` / `add_scripted_infraction` — accrual (`Ai.AddInfraction`).
* `report` — weight the accumulator into a mood, apply the relation delta, clear the accumulator,
  escalate pursuit (`MrxFactionManager.Report` → `FinishedReporting`).
* `report_civilian_casualties` — the collateral-damage penalty curve, on the same relation path.
* `set_relation` / `init_relation` / `get_relation` — the manager's own relation model.
* `attitude` / `price_multiplier` / `meter` — the policy readouts.
* `pursuit_level` / `pursuit_state` / `lock_pursuit` / `clear_pursuit_lock` + `tick(dt)` — heat.

It has **two drainable output queues** rather than direct writes: `take_relation_changes()` yields
`RelationChange` intents (a pending `Ai.SetRelation(from, to, value)`, already clamped) and
`take_attitude_events()` yields `AttitudeEvent`s (the `Event.Post("Attitude", …)` payload, emitted only
when a relation change crosses a *level* boundary). This is the carve seam: the relation matrix itself
is owned by `mercs2_ai`, and per the plan's leaf-crate rule this crate does not depend on it — the game
mirrors the intents into `mercs2_ai::RelationMatrix`. The crate keeps its own relation model to compute
deltas, level crossings and pursuit escalation, exactly as `mrxfactionmanager.lua` held `_tFactions[x]`
state *and also* called `Ai.SetRelation`.

It also carries the five faction **reflection components** (`FactionMarker`, `FactionValue`,
`FactionZone`, `RtFactionZone`, `Suspect`) as real `mercs2_core` ECS components, with their recovered m2
class hashes and strides.

Owned Lua surface: `Ai.AddInfraction`, `Ai.Get/SetRelation`, `Ai.SetInfractionMultiplier`,
`Pg.*Pursuit*`, `MrxFactionManager`.

## Where it comes from

Provenance as recorded in the source:

| Reimpl item | Oracle |
| --- | --- |
| the 7×`{id,score}` accumulator + the seven literal key strings | `FUN_005e0720`, the native mood-bridge serializer (code map §2). It walks `puVar2[2*slot]=id`, `puVar2[2*slot+1]=score` and emits under `s_DamagePerson_00bb3d48` … `s_SpecialEvent_00bb994c` |
| mood weights (×3 / ×1 / ×50 / ×25 / ×10 / ×20, `SpecialEvent` = its own `id × score`) + the `≥ -60` mood clamp | `mrxfactionmanager.lua:1212-1219` (`FinishedReporting`) |
| civilian-casualty penalty: `-5000 · 2^(kills/20)`, floored at `-1,000,000` | `mrxfactionmanager.lua:815-823` |
| relation range, attitude bands, price multipliers, HUD colours, `ConvertRelationToMeterValue` | code map §3 / `mrxfactionmanager.lua` (`:9`, `:11`, `:20/:35/:50`, `:632`) |
| HUD meter levels `{0, 25, 50, 75}` | §6, `mrxguihudfactiongauge.lua` |
| the eight faction templates + the mutable-attitude gate + the initial relations | `_tFactions :66`, `CanAttitudeBeMutable :512`, code map §3 |
| pursuit level cap 3, dwell `Pg.SetPursuitLevelTimes(120, 300)`, re-arm `Pg.SetPursuitSeconds(…, 5, …)` | code map §5 (`Setup :367`) |
| `FactionMarker` `0x9b98cb09` (registrar `FUN_00641340`, consumer `FUN_0065c0f0`, stride 4), `FactionValue` `0x8bfc69d6` (`FUN_00641830`/`FUN_0065c7d0`, stride 4), `FactionZone` `0x67267cc1` (`FUN_006414b0`/`FUN_0065c490`, stride 4), `RtFactionZone` `0xa67114c7` (desc base `0x017c05f8`, raw-copy stride `0x1c`) | code map §4 |
| `Suspect` `0x1afc276c` (runtime `FUN_006482b0`, stride `0x20` = 8 factions × dword) | `ai_code_map.md` AI census |

Every component hash is exactly `mercs2_formats::hash::pandemic_hash_m2(class_name)` — asserted in the
`components` module tests. The infraction/attitude/pursuit *policy* is Lua that the code map recovered
verbatim; the native side owned only the accumulate + serialize (§2) and the pursuit level state +
countdown (§5), which is why those are the only things this crate simulates per-frame.

## Usage

Library only — no binaries.

```rust
use mercs2_faction::{Attitude, FactionWorld, InfractionKind, factions};

// Seeded with the recovered §3 initial relations over the eight standard factions.
let mut w = FactionWorld::with_default_relations();
let oil = factions::faction_guid("OC");
assert_eq!(w.attitude(oil), Attitude::Friendly);
assert_eq!(w.price_multiplier(oil), Some(1.0)); // Friendly sells at 1.0x
let _ = w.take_relation_changes();              // drain the seeding intents

// Blow up two of their people and hijack a truck, then let them report it.
w.add_infraction(oil, InfractionKind::DestroyPerson, oil as i32, 2); // 2 x 50
w.add_infraction(oil, InfractionKind::Hijack, oil as i32, 1);        // 1 x 10
w.report(oil);                                                       // mood 110 -> relation -110 -> clamps to -100

assert_eq!(w.attitude(oil), Attitude::Hostile);
assert_eq!(w.price_multiplier(oil), None);      // Hostile will not sell
assert_eq!(w.pursuit_level(oil), 1);            // relation hit -100 -> heat

// The two output queues the game mirrors out.
for c in w.take_relation_changes() {
    // game: mercs2_ai::RelationMatrix.set(c.from, c.to, c.value)
    let _ = (c.from, c.to, c.value);
}
for e in w.take_attitude_events() {
    // game: Event.Post("Attitude", ...) -> HUD / PDA / music
    assert_eq!(e.new_attitude, Attitude::Hostile);
}

// Heat decays on the level's dwell (L1 = 120 s).
w.tick(121.0);
assert_eq!(w.pursuit_level(oil), 0);
```

The reflection components are ordinary `mercs2_core` ECS components:

```rust
use mercs2_core::World;
use mercs2_faction::{FactionMarker, FactionZone, Suspect};

let mut world = World::new();
let e = world.spawn((FactionMarker { faction_id: 3 }, FactionZone { zone_faction_id: 3 }, Suspect::default()));
assert_eq!(world.get::<&FactionMarker>(e).unwrap().faction_id, 3);
```

## Modules

* **(crate root)** — `FactionWorld` (accumulators + relation model + pursuit + the two output queues),
  `RelationChange`, `AttitudeEvent`.
* **`mood`** — the 7-key `InfractionAccumulator` / `InfractionKind` / `InfractionSlot`, the recovered
  mood weights, the `-60` mood clamp, `EMIT_ORDER`, and `civilian_casualty_penalty`.
* **`attitude`** — `Attitude` (band classify, `price_multiplier`, `color`, `label`, `median`),
  `RELATION_MIN/MAX`, the band thresholds, `PURSUIT_ESCALATE_AT`, `relation_to_meter`, `METER_LEVELS`.
* **`pursuit`** — `PursuitState`: level `0..=3`, dwell countdown, `increment` / `settle` / `lock` /
  `clear_lock` / `tick`, plus the recovered `dwell_secs`.
* **`components`** — `FactionMarker` / `FactionValue` / `FactionZone` / `RtFactionZone` / `Suspect` with
  their m2 class hashes and strides.
* **`factions`** — `FACTION_TEMPLATES` (the eight), `FACTION_ABBREVS`, `DYNAMIC_FACTIONS` / `is_dynamic`,
  `faction_guid`, `SELF_RELATION`.

## Notes / gotchas

* **The relation write is not this crate's.** `Ai.SetRelation`'s matrix belongs to `mercs2_ai`; this
  crate only *emits intents*. If nobody drains `take_relation_changes()` the queues grow unboundedly and
  the AI matrix never learns about the change.
* **The mood clamp is asymmetric.** The weighted mood is clamped `≥ -60` *before* it is negated into the
  relation delta, so one favourable report can raise a relation by at most `+60`, while an unfavourable
  one is bounded only by the relation floor (`-100`).
* **`SpecialEvent` has no fixed weight** — its mood term is `id × score`, i.e. the caller-supplied
  multiplier times the amount. `add_scripted_infraction` routes through it, applying the faction's
  standing `Ai.SetInfractionMultiplier` value; a multiplier of `0` drops the infraction entirely (the
  shipped `gurcon002.lua` toggles `0 ↔ 1` around scripted damage windows).
* **Infractions do not passively decay.** `tick(dt)` only advances pursuit countdowns. An accumulator
  persists until a `report` consumes it.
* **Level-3 pursuit has no auto-decay.** `Pg.SetPursuitLevelTimes(120, 300)` supplies dwell times for
  levels 1 and 2 only; no level-3 dwell was recovered, so `dwell_secs(3) == None` and a level-3 heat
  holds until it is explicitly cleared. Confirm-live, not an invented number.
* **`Attitude::Friendly.median()` is confirm-live (±1).** `median(Neutral) = 0` is documented exactly;
  the Friendly median is returned as the band midpoint `(33 + 100) / 2 = 66` because the Lua's precise
  rounding is not pinned in the map. It feeds the default Guerilla/OC starting relations.
* **`RtFactionZone` is an opaque 28-byte blob.** The code map records it only as a `raw-copy 0x1c`
  record; its field layout is unrecovered, so it is carried as `[u8; 28]` rather than invented fields.
* **`faction_guid` is a standalone default.** The engine resolves faction GUIDs via `Pg.GetGuidByName`,
  which may hand back a registry *handle* rather than the raw m2 hash. If the game supplies real GUIDs,
  feed those to `FactionWorld` instead — it is GUID-agnostic (`u32` keys) everywhere else.
* **`set_relation` alone does not escalate pursuit.** Escalation rides the `report` /
  `report_civilian_casualties` paths, matching `FinishedReporting`.
