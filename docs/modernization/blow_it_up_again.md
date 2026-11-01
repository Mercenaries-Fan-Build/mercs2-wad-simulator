# Blow It Up Again — every piece of the DLC, in the Workshop first

The goal, from the user: every piece of data in the base game and the DLC can be viewed, inspected and
edited. The DLC works in the Workshop before it works in the game, because the game is opaque about
what goes wrong and our own readers are not. The DLC is carried by a Shipment,
`shipments/blow-it-up-again/`, built up piece by piece.

The DLC is a level, not an overlay: `dlc01.lua` is a peer of `vz`, and the engine opens `data\dlc01.wad`
by name (memory `dlc-level-boot-and-replacement-architecture`).

Approved manifest changes: P1, P2, P3, P4a, P4b and P5 of
[`workshop_pages/level_editor.md`](workshop_pages/level_editor.md).

## What the Shipment holds

The DLC's bytes stay out of the repo, and out of any published Shipment. The Shipment holds the recipe
and the player's copy supplies the content: `dlc_port` converts the player's own package into a
gitignored folder the manifest builds from. A file we author, such as a master-script change or a
ported shader, is tracked.

The folder starts when a manifest for it can pass `qm lint`, which needs step 3's `add_level`.

## What is measured

`dlc_port` over the retail Xbox package (`game-files/Blow it up Again DLC/45410828/…`), 2026-11-01:

| Check | Result |
|---|---|
| Blocks | 2,196 of 2,196 convert; a block that does not fails the run |
| Streamed wavebanks | 7, `vo_stream_dlctest.<language>` ×6 and `dlctest_streaming`, with a 32-byte `.pws` name slot and records at `+0x38`. PC reads the offset from the bank (`audio_code_map.md` §11.5) |
| ASET rows | 5,341 routed, 0 dropped; `aset_refcheck` passes |
| `wad_simulator` | All 9 wavebanks and every asset type consume with 0 issues. Exit 1 from the one flag notes already rule out, the `ActionTable` row count of 1,036 |
| `.pws` files | Not converted by `dlc_port`. 7 June-built PC files in `output/data/Audios` pass the simulator's audit; the Rust path is `transcode_pws_xbox_to_pc`. That the game finds them under the bank's name is unread |
| Reader | `WorldEntity::parse` refuses the DLC's `worldentity`: CHDR flags 5, bit 2 stores names as hashes, so `mercs2_probe layer` cannot read one DLC layer |
| Shaders | 2 of 21 keys are not registered on PC, `0x3973c300` among them, on 219 of 1,020 materials (`docs/_dlc01_shader_triage.md`) |
| Live | `dlc01` boots by name, then hangs in `WAITFORSTREAMING` with `dlc01.wad` mounted alone, about 26k base assets absent. Which asset the "1 outstanding asset" is was not read. Every WAD of that run predates these banks converting, or carried them in an older form (SPECULATIVE) |

## Order of work

Each step ends with something the user can look at.

1. **Every block converts.** The streamed wavebank layout. Test: 2,196 of 2,196, 5,341 rows, 0
   sentinelled, `a_dlc_streamed_bank_converts_with_its_long_name`.
2. **Every byte reads.**
   - `worldentity` with names as hashes, proven by a parse that re-encodes byte for byte.
   - `mercs2_probe layer --catalog` over the DLC, with the 11 static and 3 default layers of `dlc01.lua`.
   - The Workshop opens a level WAD: every Inspect tab lists the DLC beside the base game, in a level
     picker, and an effect, model, clip, sound and string plays from the converted WAD.
   - The DLC's Xbox-side audio plays in the Audio tab.
3. **The Shipment exists.** P5 `add_level`, then `shipments/blow-it-up-again/` passes `qm lint`.
   `qm build` writes a self-contained `dlc01.wad`: DLC blocks plus exactly the base assets their
   references reach (`aset_closure`, `aset_refcheck`), so a missing asset is a build error naming it.
4. **The level loads in our own engine.**
   - The Level page opens `dlc01` as a level: terrain, entities, layer states.
   - `mercs2_autoplay` runs `dlc01.lua` on the script host, so the load's every asset request and
     layer call is a line we can read.
   - The `_pathfinding` layers are read, for what they say about AI movement in the base game.
5. **Shaders.** Name the 2 unregistered keys and draw their 219 materials in the Workshop. Port them to
   PC through `add_shader`, from the PS3 or Xbox programs.
6. **Editing, base game and DLC alike.** P1, P3, P4a, then P2, then P4b, each through a Depot draft,
   exact layer lookup with the first. Each proposal's in-game test (U1, U2 and the rest of
   `level_editor.md`) runs before its edit is offered.
7. **Coverage.** A table of every chunk tag and asset type across both WADs: which can be viewed, which
   inspected, which edited. Anything short of all three is a row of work.
8. **The game.** Deploy by sha256, boot `dlc01`, score the run with `loadprobe`, gate each mission.
   Every failure goes back to the step whose test should have caught it, as a new test there.

## Not settled

- Whether the wedge is the base assets, the audio or both; steps 3 and 8 separate them.
- `dlc_pc_port_status.md` and `dlc_arena_loading_analysis.md` still describe the overlay route.
- The PS3 package skips 472 blocks; this plan uses the Xbox package, which holds the same names.
