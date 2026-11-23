# Editing a model across its LOD chain

How `qm` re-emits a shipped model it edits (`edit_state_machine`, `replace_phy2`) so that every LOD rung
the model's ASET row names ships with it.

## The problem

A model is a chain of up to four blocks: `_P000` (coarsest; holds `HIER`/`SEGM`/`MTRL`/`PHY2` and the
destruction machine) and finer rungs `_P001`..`_P003` holding geometry only. Both model edits emit the
edited `_P000` container as one single-entry block:

- `edit_state_machine` reads the model through `GameStack::model_container_for_edit`, which accepts only a
  row whose `_P001` slot is the sentinel (`is_primary()`). That admits 252 of the 1,311 models with a
  machine, none of which has a finer rung; the other 1,059 are refused.
- `replace_phy2` reads through `container_for_asset` and writes its row with every rung sentinelled, so a
  multi-rung model ships its coarsest tier alone (the destroyer: 3,037 of its 143,266 near triangles).

## What the engine and retail say

| Fact | Evidence |
|---|---|
| A rung is fetched by the block index in its ASET slot. The row reads as four u16 `[_P003, _P002, _P001, _P000]`; `_P00N` is slot `3-N`. No path is read in the request chain. | pc-v10 `FUN_00875760` → `FUN_008759c0` (`node+0x58` block, `node+0x5a` slot); capture `23fe7f38/pmc` nodes `0x0148B874`, `0x0148AEB4`, `0x01486E1C` |
| Rungs install coarsest first, and a rung block must hold the model's `(hash, model)` entry exactly once. | consumer `0x4b1160` rung gate (`render_core_code_map.md` §8) |
| The INDX tier byte (`packed_field` bits 24–27) is the node's eviction class: copied to `node+0x18`; a request of tier t may evict only resident nodes of tier > t; completions finish in tier order. | `FUN_00872e60`, `FUN_00873e80`, `FUN_008738f0`; in-memory INDX == disk INDX on all 11,370 rows, tier bits included |
| Every model rung holds its entry once, tier == N, path ends `_P00N_Q(3-N)`, no gaps, machine only in `_P000`. 4,407 rungs: 3,007 `_P000`, 1,236 `_P001`, 164 `_P002`, 0 `_P003`. 686 finer-rung blocks also hold other assets. | `mercs2_probe model-chains <vz.wad> <out.tsv>` (exit 0 = all hold) |
| Each rung entry's third word (`field_c`) is the byte sum of the model's containers in the rungs before it (`_P000` = 0). | same sweep: 3,007 / 1,236 / 164 hold, 0 do not |
| The rung install loop reads each entry's name, type and size (`+0`, `+4`, `+0xc`), never `+8`. Other readers of the word (the resident-block reader, per-type handlers) are not checked. | pc-v10 `FUN_00875d80` decomp; pc-v11 `0x875fd0` disassembly. The match graph has no console counterpart for it. |

## Design

### Reader — `GameStack::model_chain(hash)`

Finds the model's row in the top-most WAD of the stack that has one (no `is_primary` filter) and reads
every rung the row names **from that same WAD**, because the row's block indices number that WAD's blocks.
Per rung it returns the container bytes, the source block's `packed_field` and its path. It fails, naming
the rung and block, when a rung block lacks the model's entry or holds it twice, or when a rung follows a
sentinel slot. It also fails when that WAD's row names fewer rungs than base `vz.wad`'s row for the same
model (an earlier build that dropped rungs), naming both chains and the WAD the shorter one came from.
`model_container_for_edit` and `ModelEditInputs` are replaced by it.

### Emitter — `emit_model_chain(hash, chain, edited_p000)`

One single-entry block per rung:

- `_P000` carries the edited container; the finer rungs carry their containers byte-for-byte.
- Each entry's `field_c` is recomputed from the emitted chain: the byte sum of the containers before it.
- Each block's tier byte is copied from its source block's `packed_field`, not computed from `N`.
- Paths: `_P000` keeps `blocks\VZ\mod_<hash>.block`; rung N is `blocks\VZ\mod_<hash>_P00N_Q<3-N>.block`,
  so no block shadows a retail path.
- Only `_P000` carries an ASET row. Its rungs name the group's own blocks by group-local source indices
  `0..n`; the rung blocks carry no row, as in retail.

### Assembly — `Lowering::Chain`

A chain is a self-contained group. `build_patch_wad_multi` maps rungs through a source-index → patch-index
map, so two edited models whose finer rungs come from the same retail block (686 such blocks) would both
claim that index; the map keeps one, and the other model's row would name a block without its entry — the
rung gate spins, a stream livelock. So each chain is numbered locally and `build` renumbers it with
`patch_wad::rebase_after` past every block collected before it, earlier chains included. The existing
`level_asset` rebase then runs after the chains.

### Callers

- `edit_state_machine` and `replace_phy2` lower to a chain; their log lines name each rung carried.
- `qm extract-states` and `qm extract-tiny` read `_P000` from the chain (`extract-tiny` output unchanged).
- Workshop: `statemachine::shipped` and the collision editor (`collision.rs`) read through the chain, so
  the Depot can show what an edit carries.

## Witnesses

All WAD-dependent; a run counts only when it prints no `SKIPPING`.

- `mercs2_probe model-chains` on retail exits 0.
- The destroyer (`al_veh_boat_destroyer`, 3 rungs): `edit_state_machine` and `replace_phy2` builds read
  back with a row naming three carried blocks, `_P001`/`_P002` entries byte-identical to retail, tiers
  0/1/2, `field_c` sums matching the emitted lengths, `aset_refcheck` clean, no rung sentinelled.
- Two models sharing a finer-rung block in one Shipment: each row's every rung resolves to a block holding
  that model's entry.
- `read_patch_wad` → `build_patch_wad_multi` round trip of such a WAD keeps every chain (the merge path).
- The workshop's `the_destroyer_machine_reads_as_the_build_reads_it` passes.

## Related docs

The engine and retail facts above are in the parent repo's `docs/aset_format.md` (§The LOD chain lives in
the ASET row), `docs/format_reference.md` §2 (`INDX`), `docs/reverse_engineer/render_core_code_map.md` §8,
and `docs/modding/field_guide.md` Trap 8; `docs/modding/manifest_format.md` §`edit_state_machine` states
the contract this design implements.
