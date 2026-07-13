# mercs2_player

The player concern of the Mercenaries 2 reimplementation — cash/fuel economy, the player-controller, and character/disguise — carved out as its own crate. **Currently a Wave-1 scaffold: it contains no subsystem logic yet.**

## What it is

`mercs2_player` is the reserved home for everything behind the `Player` Lua namespace:

- **Economy** — cash and fuel (`Player.GetCash`/`SetCash`, `GetFuel`/`SetFuel`/`AddFuel`, `FuelCapacity`).
- **Player-controller** — the locally-driven character.
- **Character / disguise** — `Player.GetPrimaryCharacter`, `Player.VehicleDisguise`.

As of this commit the crate ships **no public API**. `src/lib.rs` is the crate-level doc plus one
`scaffold_links` test that constructs a `mercs2_core::Time` to prove the dependency edge resolves.
Nothing in the workspace depends on it yet.

The crate exists so that the Wave-1 owner can implement this subsystem without write-colliding on
`mercs2_engine` / `mercs2_game`. Per the carve rules it depends **only** on `mercs2_core` (ECS /
events / time, plus the `PhysicsQuery` seam) and `mercs2_formats`, and never on another leaf crate.

## Where it comes from

Provenance as recorded in the crate's own header:

- **Silo 17** of `docs/modernization/reimplementation_parallelization_plan.md` §3. It got its own
  crate because `Player` is the second-highest-traffic Lua namespace (107 call sites) and spans both
  economy and the player-controller, so folding it into the vehicle or faction silo would bloat them.
- **Scoreboard row:** none — the concern is cross-cutting. The economy state lives on the singleton
  at `[0x1176054]` in the retail PC exe.
- **Code map:** `docs/reverse_engineer/save_serialize_code_map.md` (economy / profile singleton),
  plus the money/fuel datatype notes: money and fuel are a **signed i32** on `[0x1176054]`, with the
  1-billion cap applied as a Lua-side soft-clamp rather than by the native type.

The Wave-1 pass is specified to implement the subsystem against those code maps with the exe as the
oracle and **zero stubbed Lua**.

## Usage

There is no public API to call yet. The crate is a workspace member; it builds and tests:

```sh
cargo test -p mercs2_player
```

which runs the single `scaffold_links` test.

## Notes / gotchas

- **This is a scaffold.** Do not document or depend on behaviour it does not have. If you are the
  Wave-1 owner, implement here rather than in `mercs2_engine`/`mercs2_game` — that separation is the
  entire reason the crate exists.
- **Dependency rule:** `mercs2_core` + `mercs2_formats` only. Adding another leaf crate as a
  dependency breaks the parallelization plan's carve rules (plan §4).
- **Open ownership question:** `Human.Inventory` (`SetAllWeapons`, `SetReserveAmmo` — the player
  loadout) is a candidate to be co-owned here rather than by `mercs2_combat`. The crate header flags
  this as undecided, to be settled when the silo starts.
