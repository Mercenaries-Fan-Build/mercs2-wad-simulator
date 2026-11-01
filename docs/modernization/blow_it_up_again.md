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

`dlc_port` writes into the Shipment's gitignored `src/level/`, and `qm build` places three things: the
converted WAD as `data/dlc01-patch.wad`, its 7 streams in `data/Audios`, and a copy of `vz.wad`, made at
deploy, as `data/dlc01.wad`. The engine opens those two WADs and no more for a level, and the DLC does
not run alone: the PS3 archive lacks 25 of the 32 modules `dlc01.lua` imports, and on PS3's `IsDLC` path
the archive mounts above level `vz` (`FUN_00382e48`). That the retail menu path on console does the same
was not read to the end.

## What is measured

`dlc_port` over the retail Xbox package (`game-files/Blow it up Again DLC/45410828/…`), 2026-11-01:

| Check | Result |
|---|---|
| Blocks | 2,196 of 2,196 convert; a block that does not fails the run. Converting says nothing about the bytes being right: only a join to PC retail does (`mercs2_probe ecs-compare`, `chunk-compare`) |
| ECS containers | `worldentity` and every console layer are compact (`docs/worldentity_container_format.md` §7). PS3 retail, converted, equals PC retail: 220,085 of 220,085 records, guidmap byte-identical. The DLC's `worldentity` is identical on Xbox, PS3 and the Nov 25 prototype, and its 6,284 template keys and name hashes all match the named Nov 6 prototype |
| Materials and part tables | Model `MTRL` 3,007 of 3,007 and `MIXR` 34 of 34 equal PC retail |
| Template names | 158 templates are new in the DLC, all named from the Nov 6 prototype |
| Models | DLC models load and draw in `--render`. `dlc_outskirt_interior_hq` draws all 39 groups with its outer shell blown out to white. All 21 of its diffuse textures match PS3, so the fault is how a material is drawn, in the Workshop or a shader; which one was not read |
| Textures | Each rung converts to its own mips, as `INFO`'s masks declare (`docs/format_reference.md` §4.3). All 1,268 DLC textures are converted; 7 have no pixels on Xbox, are in the base game, and leave their stubs out, so a `dlc01.wad` mounted alone lacks them until step 3's closure. Decoded against PS3 DLC, DXT5 is identical at the 99th percentile, DXT1 differs by codec noise, and the 9 normal maps PS3 keeps in DXT1 match on X within 3.3 of 255 and Y within 7.5; PC twins agree over 88,448 mips of the Xbox retail bake. `dlc_al_veh_tank_m1a3` and `dlc_pmc_veh_truck_armoredmonster` draw fully textured in `--render` |
| ASET rungs | 1,045 rows chain to their finer rungs, derived from which blocks hold each asset, and every rung of all 5,334 rows agrees with the Xbox row, which `dlc_port` now enforces. Before this `_P001` was blanked and `_P002`/`_P003` kept Xbox block indices wherever they happened to fall in range |
| Layers | 20 layers, 8,312 entities, 3.2 km square (`layer --catalog --overlay`). `dlc01.lua` asks for 14. `dlc01_SpeedCity_scrub` and `dlc01_SpeedCity_foliage` are in no build we hold |
| Streamed wavebanks | 7, `vo_stream_dlctest.<language>` ×6 and `dlctest_streaming`, 4,096 clips, with a 32-byte `.pws` name slot and records at `+0x38`. PC reads the offset from the bank (`audio_code_map.md` §11.5). `dlc_port` re-encodes their XMA2 streams to the IMA a retail `.pws` holds and writes them to `Audios/` beside the WAD. Decoded, our IMA matches ffmpeg's XMA decode at a median correlation of 0.9996 at lag 0, the lowest 0.9977, over 120 clips. Against PS3 the 30 clips sampled are the same take: envelopes correlate at 0.985, and waveforms at 0.71 in 8,192-sample windows and 0.34 over a whole clip, so the two bakes are not sample-identical and why was not read |
| ASET rows | 5,334 routed, 7 dropped with their pixel-less stubs; `aset_refcheck` passes, every one of 3,499 rungs holding its asset once |
| `wad_simulator` | All 9 wavebanks and every asset type consume with 0 issues. Exit 1 from the one flag notes already rule out, the `ActionTable` row count of 1,036 |
| `.pws` files | The 7 June-built files in `output/data/Audios` are nibble-swapped XMA, not audio. That PC's `Sound.OpenStreamFile` path finds the new ones is unread: `dlctest01_soundbootstrap.lua` opens them under `Net.DLCGetMountPoint()` |
| Workshop | The picker lists `vz` and `dlc01`. `dlc01` lists 312 models, 1,261 textures, 2,609 anims, 4 effects, 18 audio assets, 3 UI, 7 text and 20 layers, no FaceFX. A model, material animation, effect, string table, layer and sound each open from the converted WAD |
| Audio tab | 11 banks, 4,126 lines. All 681 English lines, the 10 streamed and the 27 embedded `dlctest_resident` clips export with sound in them. English peaks reach full scale on 400, and on 59 of 120 PS3 twins sampled, against 1 of 300 retail `vo_Chris` lines, so the level is in the DLC's master. A line has no speaker or caption, and no cue reads it yet |
| Effects | 314 of 314 retail effects convert to PC's bytes, 51,593 chunks. An emitter's `GEOM` is two u16s, which the u32 sweep traded |
| Shaders | 2 of 21 keys are not registered on PC, `0x3973c300` among them, on 219 of 1,020 materials (`docs/_dlc01_shader_triage.md`) |
| Live | `dlc01` boots by name, then hangs in `WAITFORSTREAMING` with `dlc01.wad` mounted alone, about 26k base assets absent. Which asset the "1 outstanding asset" is was not read. Every WAD of that run carried a damaged `worldentity`, compact layers and guidmap, and predates these banks converting |

## The converter worklist

`mercs2_probe chunk-compare <xbox-vz.wad> <out.tsv> --vz-wad <vz.wad> --java … --unluac …` converts every
Xbox retail chunk and compares each row with its PC twin, joined by name and type. `--only <type>`
limits a re-run. A difference is either a converter gap or a real difference between the bakes, and
each row below has to be read to say which. Of 11,087 blocks `wpn_sniperrifle` is refused, whose
wavebank fails ffmpeg's decode, and so is each streamed bank, whose `.pws` this tool is not given.

| Rows differing, of those on both sides | Chunk |
|---|---|
| Every one | `lineregion` `data` 625, `anim_state_machine` `AINF` 394, `sequencetable` `ITEM` 102 and `SINF` 51, `soundbank` 75, `sounddb` 76, `facefxanimationset` 86, `facefxactor` 31, `font` `INFO` and `CHAR` 6 |
| Most | `wavebank` `data` 94 of 94 (sizes differ), `PHY2` 998 of 1,564 |
| Some | `animation` `data` 1,435 of 4,232, `model` `INFO` 230 of 3,007, `binary` 14 of 14, `chatter` 6 of 22, `cfx_pack` 14 of 64 |
| Unread | mesh `STRM`, `IBUF`, `PRMT`; `script` `BINN` |

Texture bodies differ because each platform compressed its own blocks, so `chunk-compare` cannot judge
them. `mercs2_probe tex-compare <xbox archive> <out.tsv> --vz-wad <vz.wad> [--ps3 <ps3 archive>]` decodes each
converted mip against its twin instead. Three things remain unread there: 15 Xbox DXT1 textures without the
`0x10` flag are DXT5 on PC and differ from it the way an unconverted normal map does; the full mips of `0x47`
textures differ by a mean of 50 of 255; and 3 of 4,753 full DXT5 mips differ in alpha, by a mean of at most
3.76. Another 123 keep their alpha and differ under fully transparent texels, where PC stores black and Xbox
keeps the colour; their visible colour differs by a median of 4.7 and at most 10.9.

## Order of work

Each step ends with something the user can look at.

1. **Every block converts.** The streamed wavebank layout. Test: 2,196 of 2,196, 5,341 rows, 0
   sentinelled, `a_dlc_streamed_bank_converts_with_its_long_name`.
2. **Every byte reads.**
   - `worldentity` with names as hashes, proven by a parse that re-encodes byte for byte.
   - `mercs2_probe layer --catalog` over the DLC, with the 11 static and 3 default layers of `dlc01.lua`.
   - The Workshop opens a level WAD: every Inspect tab lists the DLC beside the base game, in a level
     picker, and an effect, model, clip, sound and string plays from the converted WAD.
   - The DLC's Xbox-side audio plays in the Audio tab, with its streams re-encoded for PC.
3. **The Shipment exists.** P5 `add_level`, and `shipments/blow-it-up-again/` passes `qm lint` and
   `qm build`. What a script's `DEPS` or a model's `MTRL` names that neither the level nor the game
   holds is a build error naming it: 1,123 references, 70 of them only in the base. What Lua asks for by
   a name it computes is not visible to a build, which is why the level sits over a whole copy of the
   base rather than a computed subset of it.
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

- Whether the wedge was the base assets, the audio or both; step 8 separates them.
- How a player starts the level. `Sys.StartSingleplayer("dlc01", "dlc01")` boots it through the Lua
  bridge; nothing in the Shipment calls it yet.
- `Net.DLCGetMountPoint` is not in the PC exe, and `dlctest01_soundbootstrap.lua:188` calls it to open the
  streams. Whether `dlc01` reaches that module was not read: `dlc01.lua` imports `MrxSoundBootstrap`.
- What `Pg.LoadLayer` (`0x005D4C80`) does with a layer in no WAD, and so whether the two absent
  SpeedCity layers can hold `_tStaticLayers` open.
- Whether the real rungs stream in the game. The scaled resident chain they replace was added to end
  a hang (`dlc01_dlccon002_roads`) on WADs with a damaged `worldentity`, and no read of the decomp finds a
  way for a texture's `INFO` or `BODY` to hold a stream node open, only to crash or corrupt. Step 8 tests it.
- Whether the rung gate was the old hang. The ring consumer re-queues a rung that arrives before the one
  above it (`render_core_code_map.md` §8), and the recorded livelock sat in that branch. The last WAD
  before this rung work names 10 rungs on 6 rows in blocks without the asset, past a blanked `_P001`, which
  the engine should not reach. The new WAD names none, and `aset_refcheck` now refuses any that does.
- The PS3 package's textures: their 25-byte `INFO` is read by `tex-compare` but `TextureTable` refuses it,
  so `dlc_port` on a PS3 input stops at its first texture. Before this it wrote them unconverted.
- `inflate` writes `0xHHHHHHHH` for a name it does not know, so an inflated container must not be
  written back: step 6 edits go through the compact bytes.
- What sets bit 1 of `DAT_00DF6B90` on PC. Under CHDR bit 2 a template registers its name hash only
  when it is set, so `Pg.Spawn` by name decides on it.
- `dlc_pc_port_status.md` and `dlc_arena_loading_analysis.md` still describe the overlay route.
- The PS3 package skips 472 blocks; this plan uses the Xbox package, which holds the same names.
