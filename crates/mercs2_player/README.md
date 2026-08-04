# mercs2_player

The **player concern** of the Mercenaries 2 reimplementation: the per-slot player object, the
persistent profile/economy singleton, possession, disguise, the play-area fence, PDA/satellite mode, and
on-foot locomotion.

## What it is

The engine behind the `Player` Lua namespace (107 cfuncs, `luaL_Reg` table `0x00B98FC0` — the
second-highest-traffic namespace in the game). Reconstructed against
`docs/reverse_engineer/player_code_map.md`, which has **all 107 cfunc bodies read** and whose §10 is a
requirements list written for this crate.

The one thing to understand before reading anything else:

> **A player is not a character, and there are two objects, not one.**

|  | `PlayerObject` | `PlayerProfile` |
|---|---|---|
| retail | the ≥`0x465`-byte record in the `Players` container `0x00DF9B90` | the singleton `[0x01176054]` |
| cardinality | one per occupied slot, **≤2** | exactly **one**, shared by the co-op pair |
| holds | slot, viewport, character link, control source, reticle, locks, modes | cash, fuel, capacity, character, upgrade, costume |
| lifetime | the session | persisted to `.profile` |

Merging them is the most consequential mistake available here: cash is not per-player, and a viewport is
not persistent.

Subsystems:

- **Roster** — the `Players` container as a *scan keyed on `+0x2C`*, capped at 2.
- **Profile/economy** — cash/fuel/capacity/character/upgrade/costume, signed `i32`, with the autosave
  dirty flag.
- **Possession** — the `player+0x20` link, the control-source clear, the disguise seed.
- **Boundary** — the play-area fence; mutating it is server-authoritative.
- **PDA + satellite** — the callback-driven modal UI.
- **Disguise** — *two* mechanisms wearing four similar names.
- **Locomotion** — the on-foot controller (a declared stand-in; see Notes).

**`Human.Inventory` is not owned here.** That question is settled:
`inventory_equipment_code_map.md` §10 assigns it to `mercs2_combat`, because the state lives on the
*character* entity (`RuntimeInventory`) and the player object carries no weapon field at all. This crate
owns possession and the profile, and reaches weapons through the character GUID.

## Where it comes from

| Reimpl item | Oracle |
|---|---|
| `Players` container, self-named | `[[0x00DF9B90]+0x34] = FUN_00647BA0` → `"Players"` |
| Slot resolve, cap-before-scan | `FUN_006CDAF0` (`cmp dword [esp+4],1 / ja`), 241 call sites |
| Joined count | `FUN_006CDAC0` (`cmp esi,2 / jl`, counts `+0x30 != -1`) |
| Local-slot resolve | `FUN_006CD960` (rejects local `>= 2`) — the **third** compile-time cap |
| Possession write | `FUN_006A4060` `0x006A422E: mov [ebx+0x20], eax` |
| Control-source clear / disguise seed | `0x006A4279` / `0x006A4314` |
| Character→player resolve | `FUN_006CDB70` (**M** — a VM stub, named behaviourally) |
| Profile singleton offsets | the six profile/economy cfunc bodies |
| Autosave gate | `FUN_00614540` `0x00614891` + `0x00614897` |
| Cheat flags, named by hash | `FUN_004C2C20`; each verified against `pandemic_hash_m2` |
| `Net.IsClient` (boundary authority) | `DAT_00DFBD77`, named by five consecutive `Net` accessors over five consecutive bytes |
| PDA widget-type hash | `0x005BA646 cmp dword [ecx+0x450], 0xfa62754e` = `pandemic_hash_m2("PDA")` |
| ECS components | registrars `FUN_00640410` / `FUN_006413F0` / `FUN_00643D50` / `FUN_00643A40` |

## Usage

```rust
use mercs2_player::{CheatFlags, PlayerWorld, possession};

let mut w = PlayerWorld::single_player();          // one joined local player in slot 0
possession::attach_to_character(&mut w.roster, 0, 0xC0FFEE, CheatFlags::default());

// Possession is a field on the player, not a component on the character.
assert_eq!(w.roster.get(0).unwrap().character, 0xC0FFEE);
let player_guid = w.roster.get(0).unwrap().guid;
assert_eq!(w.roster.by_character(0xC0FFEE).map(|p| p.guid), Some(player_guid));

// The profile is ONE global, shared across the co-op pair.
w.profile.set_cash(75_000, false);
w.roster.create(1);                                 // a slot INDEX, not a guid — and idempotent
assert_eq!(w.profile.cash, 75_000);                 // no second wallet for a second player

// ...and `SetCash` does not arm the autosave. That is retail's bug, reproduced on purpose.
assert!(!w.profile.autosave_due());
```

## Modules

- **`object`** — `PlayerObject` + `BoundaryState` / `ReticleState` / `PdaMapMode` / `SatelliteScan` /
  `SurvivalState` / `PlayerDisguise`. Every field carries its offset **and** the VA of the instruction it
  was read from.
- **`roster`** — `PlayerRoster`, and the four independent player counts retail reports.
- **`profile`** — `PlayerProfile`, `NON_DIRTYING_SETTERS`.
- **`possession`** — `attach_to_character` / `detach_from_character` / the bind trio, `CheatFlags`.
- **`boundary`** — `Boundary`, `BoundarySet`, `NetAuthority`.
- **`pda`** — map mode (nine arguments) + satellite scan.
- **`disguise`** — `DisguiseRequest` and the two-mechanism split.
- **`callbacks`** — `CallbackRegistry`: opaque ids the script layer keys retained Lua functions on.
- **`components`** — the four reflection components + the six `Controller*` probe order.
- **`locomotion`** — `PlayerController`, `LocomotionInput`.
- **`system`** — `player_roster_system` (retail's passes A and B).

## Notes / gotchas

1. **The roster is a scan, not an array.** Retail matches `+0x2C`; nothing may assume
   `records[i].slot == i`.
2. **Four counts, four different answers.** `ROSTER_CAP` (3 compile-time immediates) ≠
   `GetMaximumPlayers` (a global nothing enforces) ≠ `GetMaximumLocalPlayers` (`2.0`, an `.rdata`
   constant) ≠ `GetCurrentLocalPlayers` (**always `1.0`, regardless of state**).
3. **`GetAnyCharacter` does no lookup.** It pushes `ANY_CHARACTER_SENTINEL` (`0xF0000000`); the *host*
   resolves it. 223 call sites break if the resolver is missing.
4. **Four cfuncs take a CHARACTER handle**, not a player handle: `IsBoundaryDeath`, `SetWaitForInGame`,
   `VehicleDisguise`, `GetVehicleDisguiseState`. They resolve through `FUN_006CDB70`. Typing them as
   player handles fails *silently*.
5. **`CreatePlayer`/`DestroyPlayer`/the attach-bind family take a SLOT INDEX**, not a GUID
   (`mrxplayer.lua:117,587`). And `create` is **idempotent and does not join** — `MrxPlayer.Init` loops
   it over every slot against an already-populated roster.
6. **Disguise is two mechanisms.** `Set/GetVehicleDisguise` read/write a *global* gate byte
   (`[0x01176106]`) and do no lookup; `VehicleDisguise`/`GetVehicleDisguiseState` are *per-player* and
   arrive as named tables whose `Player =` key is a character guid.
7. **Boundary mutation is server-authoritative.** On a client, `AddBoundary`/`RemoveBoundary`/
   `RemoveAllBoundary` push `false` and do nothing. Single-player is *not* a client.
8. **Container capacity/shift are runtime state.** `ControllerPlayer` registers `0x100` and the dump
   reads `0x60`. Registrar constants are initial values, never contracts — only stride is asserted.
9. **`locomotion` is a declared stand-in.** Retail is `hkpCharacterProxy` + a 5-state
   `hkpCharacterContext` (`physics_code_map.md`); every constant here is gameplay-derived from human
   scale, not read from the exe. It reads the world through `mercs2_core::LocomotionQuery` so the
   Havok-faithful version can replace it behind the same seam.
10. **A failed handle lookup returns nil and does not raise** (`FUN_004B2A50` is `push nil; return 1`).
    Shipped scripts rely on `if Player.X(u) then`, so erroring on a bad handle breaks working Lua.

## Dependency rule

`mercs2_core` + `mercs2_formats` only — never another leaf crate (plan §4). The two edges that would
otherwise be needed go through `mercs2_core` traits instead: `LocomotionQuery` for the controller's world
reads, and `PlayerWorld::set_control_source` as the seam the seat/ride system pushes through.

Deliberately unimplemented items, and the shipped bugs reproduced on purpose, are in
[`DEFERRED.md`](DEFERRED.md).
