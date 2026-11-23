# Destruction orchestrator chunk format — the ENGINE's state machine (from the decomp)

**Source:** `FUN_004cf340 @0x004cf340` in `output/_ghidra/mercs2_unpacked.exe_decomp.txt`
(L103473–103725) — found by grepping the chunk-tag constant `0x54495753` ("SWIT"); it is the
exe's ONLY consumer of that tag. Recovered 2026-07-05 after the workshop's heuristic
destruction classifier mislabeled `al_veh_truck_hmmwv_avenger` (39/51 groups "break").

## Why this matters

`mercs2_formats::orchestrator::classify` GUESSES which SWIT sibling subtree is intact vs
break ("break root = most SWIT descendants", validated on one crate). The engine does not
guess: destruction is a **named-state machine** parsed from a dedicated chunk family, with
explicit per-state membership and enter/exit action lists. The heuristic can be (and is, for
vehicles) wrong; this format replaces it.

## Chunk family (walked by `FUN_004cf340` over a UCFX container subtree)

Tags (LE u32 constants as they appear in the decomp):

| Tag | u32 | Role |
|---|---|---|
| `INFO` | 0x4F464E49 | header: `[u32 skipped, u32 switch_count → this+0xC, u32 node_count → this+4]` |
| `NODE` | 0x45444F4E | one switch node: `[u32 name_hash, u32 state_count]`; starts a 0xC-byte node record `{name, state_count, states_ptr}`; states = `state_count × 0x14`-byte records |
| `STAT` | 0x54415453 | one NAMED STATE of the current node: `[u32 state_name_hash]` → record `{name, enter_count, enter_ptr, exit_count, exit_ptr}` (0x14 B, lists zeroed) |
| `CHDR` | 0x52444843 | selects the current state's list: `[u32 which, u32 count]` — `which` is `0x9DA97065` = **m2("Enter")** or `0xDB41017D` = **m2("Exit")** (rainbow-confirmed); allocates `count × u32` |
| `CEXE` | 0x45584543 | fills the selected Enter/Exit list with `count × u32` entries |
| `SWIT` | 0x54495753 | `switch_count × u32` — the per-switch-slot table (this+0x10/this+0x14) |

Final packing (single allocation, in order): `[SWIT u32s][node records ×0xC][state records
×0x14][enter/exit u32 lists]` — offsets rebased to the allocation.

## Interpretation (to be validated on real containers)

- A destructible model's orchestrator defines N switch **nodes**; each node has named
  **states** (expect hashes reversing to `pristine`/`destroyed`/`damaged##`-style names —
  vehicles have staged damage, hence 7–8 SEGM tiers on the destroyer/HMMWV).
- Each state's **Enter/Exit lists** carry u32 hashes — candidates: HIER node hashes to
  show/hide, effect/sound hooks, or command ids. Resolving a real model's lists against its
  HIER hashes + the rainbow table decides this.
- The geometry-side flat `SWIT` hash list our Rust reads is only the *participant set*; the
  authoritative state→membership mapping lives HERE.

## VALIDATED on retail (2026-07-05) — parser: `mercs2_formats::orchestrator::parse_state_machine`

Parsed clean on both test vehicles (family lives INSIDE the model container, as a container row
whose immediate children are the INFO/NODE/STAT/CHDR/CEXE/SWIT leaves — the engine's `u3`
descendant-count sibling-skip reproduced):

- `al_veh_truck_hmmwv_avenger` 0xAC990539: 25 switch slots, 11 nodes, ~8 states each.
- `al_veh_boat_destroyer` 0xE54047D5: 59 switch slots, 47 nodes.

**Enter/Exit are COMMAND SCRIPTS**, token grammar (decoder: `orchestrator::decode_script`):
`0x1 <arg>` push arg · `0x2 <command>` invoke with pushed args · `0x3` end.

Resolved command vocabulary (rainbow table + the Jul-08 DEVKIT string dump
`output/jul08_prototype/mercs2_xenon_p.pe_full_strings.txt` — 57k authored strings the retail
exe strips; now merged into the workshop name pack):

| hash | command | args (observed) |
|---|---|---|
| — | `SHOW` / `Hide` | (node) |
| — | `SetState` | (target_state, node) |
| 0xE1142510 | `SetStateOnMsg` | (target_state, message, node) — the TRANSITION rule |
| — | `StartEmitter` / `StopEmitter` | (effect, node) — real FX names (`global_particle_fire_carhood`, `fx_Explosion_Large`, exhaust on `hp_fx_exhaust_a/b`) |
| 0xC6E8AFA8 | `CreateObject` | (…, PropTemplate, …, node) — break-away prop spawn |
| 0x207C1CC7 | `KillObjectsLinkedToHP` | (node) — HP = hardpoint (community "hp" finding confirmed) |
| 0x842AE03E | `DisableConstraint` | (node) |
| 0xC20AB66F | `SetRootNode` | (node) |
| 0xCFF8BCA4 | `StartAnim` | (anim?, ?, node) |
| 0xECADCE57 | `SetNodePhysicsModelKeyframed` | (node) |
| — | `KILL(SELF)`, `Antenna`, `PropTemplate` | literals/args |

Still unresolved: `0xB4DBE473` (command, destroyer), the STATE name hashes (global vocabulary,
below), and the MESSAGE hashes `0x1ED7AD78` / `0x3D0D4C99` / `0xC6507EE1` (damage/impact event
names — `SetStateOnMsg` listens for them). Tried and missed: exe symbol strings, common damage
words, `labels`/`driver`/`faction` (user candidates), `hp` (= hardpoint, not a state).

**State-name hashes are a GLOBAL vocabulary** (identical across both vehicles):
`0x0ACE072A` (init: immediately SetState→0xACB51200), `0xACB51200` (the SHOW/pristine-like
state), `0x92791EBB` (the wreck state: fires + explosion + prop spawns + KILL SELF),
`0xCA261E5B` (terminal/empty), `0x5D308F4F`, `0x5A6E8927`, `0x381BE6A4`, `0xCE603754`,
`0xA530B827`, `0x1D5575A1`, `0x7687DF41`. Not yet reversed (common words tried and missed) —
crack once, named everywhere. Candidate sources: exe SetState string handling, COMP schema
defaults, DLC/mission Lua.

Workshop: `--states <model>` dumps all of this decoded; the Details panel shows a read-only
"Engine state machine" section per preview.

## Validation / implementation plan

1. Locate this chunk family in a real destructible's block (the HMMWV `0xAC990539` and
   destroyer `0xE54047D5` are the test pair — the HMMWV is the case the heuristic gets
   wrong). It likely lives in the ORCHESTRATOR container that `destruction_extract` already
   distinguishes from geometry containers.
2. Parse + reverse every state name hash via the rainbow table; cross-reference Enter/Exit
   entries against the model's HIER node hashes.
3. Replace `orchestrator::classify`'s guess with the parsed state machine; workshop's
   Destruction panel then lists REAL state names and switches membership exactly as the
   engine does. (Until then the panel is labeled a heuristic.)
4. Cross-check against SEGM `state_mask` tiers — expect states ↔ tier bits to correspond
   (the mask selects the geometry tier; the orchestrator drives which mask is active).
