# Retail `.profile` save fixtures

The eight real Mercenaries 2 (PC) save files the save reader (`src/save.rs`) and writer
(`src/save_write.rs`) are reversed against. Reached via
`mercs2_formats::game_paths::save_fixtures()`, which derives from `CARGO_MANIFEST_DIR` — never a
hardcoded path.

## Why they are committed

They were previously read from one developer's `C:/Users/Shadow/Documents/My Games/Mercenaries 2/SaveGames`.
That had two failure modes, both bad:

* **`src/save.rs`** — the loader called `.unwrap_or_else(|e| panic!(...))`, so on any other machine
  eight tests failed with "No such file or directory" rather than testing anything.
* **`src/save_write.rs`** — the loader used `.ok()`, so every write-side test skipped silently and
  reported **green while asserting nothing**. The module documents the `ProfileHash` derivation as
  "verified byte-exact against all 8 retail `.profile` files"; that verification had not actually
  executed anywhere but the machine the path names.

A test that can only run on one computer is not a test. At 13,404 bytes each the whole set is 128 KiB,
so vendoring costs nothing and makes every claim about the format continuously checked. (This is not
an option for `vz.wad`, at 2.5 GiB — WAD-dependent tests still skip when it is absent.)

## The set

Listed in `mercs2_formats::game_paths::SAVE_FIXTURES` — the one authoritative list, which
`save::tests::the_fixture_set_is_complete_and_fully_covered` checks against this directory in **both**
directions, so a file can be neither added-but-unexercised nor deleted-but-listed.

| File | Hero | Contract | Flow | Notes |
|---|---|---|---|---|
| `Chris Jacobs_6A499ED6.profile` | Chris (2) | `VzaCon001` | 2 | **Earliest state: before the player owns the PMC.** See below. |
| `auto_6A499D08.profile` | Chris (2) | `VzaCon001` | 2 | autosave of the same pre-ownership state |
| `auto_6A0BE454.profile` | Mattias (1) | `PmcCon001` | 3 | first PMC contract |
| `auto_6A447BF8.profile` | Jen (3) | `PmcCon001` | 3 | the primary target file most single-file assertions use |
| `auto_634304EA.profile` | Mattias (1) | `OilCon003` | 15 | mid-game |
| `Mattias Nilsson_63430745.profile` | Mattias (1) | `OilCon001` | 15 | mid-game, named slot |
| `Mattias Nilsson_6A0E523C.profile` | Mattias (1) | `PmcJob001` | 63 | endgame, upgrade tier 3 |
| `_______ ________48EFABFB.profile` | Mattias (1) | `PmcJob001` | 63 | endgame; **non-ASCII slot name**, which is why it is here — it exercises the UTF-16LE `save_name` path |

"Flow" is the length of the `SaveState` flow chain (completed contracts). Together these span the
whole progression, all three heroes, and upgrade tiers 0 and 3, which is what
`the_set_spans_the_progression_and_flow_chains_grow_with_it` and `every_hero_is_represented` assert.

### The Chris saves are the pre-open-world state

Per the repository owner: this save is from **before the PMC is owned by the player**. They have beaten
the intro mission and have not yet progressed to the open world.

That is not derivable from the bytes — the flow chain is exactly `["Start", "VzaCon001"]`, and knowing
that `VzaCon001` *is* the intro (rather than the first of many completed contracts) is what makes it
meaningful. It gives the set a genuine floor: "no progression yet", distinct from "one contract done".

It is also the save state the engine boot path currently exercises. `VzaCon001.StandardSetup`
(`vz/vzacon001.lua:78`) is the mission whose `ObjectHibernation` gate on `VzaCon001_StartingBoat` the
world-load state machine waits on, so this fixture corresponds to exactly the point the boot reaches.

Every file is exactly `PROFILE_SIZE` (13,404) bytes: a packed binary header, then a zlib stream at
`0x468`. See `SAVE_FORMAT.md` and the module docs for the field grounding (FACT vs INFERRED).

## Provenance

Real player profiles from the repository owner's own installation, contributed specifically so these
tests run everywhere. They are save data — player progress — not game assets redistributed from the
retail disc.
