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

The Shipment holds components, never a WAD. A WAD shipped whole would shut every other Shipment out of the
level, because `vz-patch.wad` is not mounted there. The DLC's bytes stay out of the repo, and out of any
published Shipment: `dlc_port --shipment` writes one PC-form file per asset rung into the gitignored
`src/level/`, and one `level_asset` row per asset into the manifest.

`qm build` compiles the rows into `data/dlc01-patch.wad`, and places the 7 streams in `data/Audios` and a
copy of `vz.wad`, made at deploy, as `data/dlc01.wad`. The engine opens those two WADs and no more for a
level, and the DLC does not run alone: the PS3 archive lacks 25 of the 32 modules `dlc01.lua` imports, and
on PS3's `IsDLC` path the archive mounts above level `vz` (`FUN_00382e48`). That the retail menu path on
console does the same was not read to the end.

One `level_asset` kind carries every type, where the plan first named a kind per type. A row is the
asset's own PC bytes, so a single kind loses nothing, and typed sources can replace a row type by type as
each gets an editor.

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
| Shaders | The level names 29 pixel and 32 vertex shaders, all registered on PC, which `qm build` now checks. `docs/_dlc01_shader_triage.md`'s two missing keys came from a June WAD: its `0xf931343a` is `PgDiffFP`, `0x343AF931`, with the halves swapped |
| Live | Built from the PS3 package over a `vz.wad` copy, `dlc01` reaches `GlobalExit` on retail PC (`loadprobe` 100%, no fault): `Sys.SetLevelName("dlc01"); Sys.SetMasterScriptName("dlc01"); Sys.StartSingleplayer("dlc01", "dlc01")` sent through the Lua bridge after `Shell music started`, the hero placed at (316.7, 8.1, −171.1). Each crash on the way was a converter bug in the table below. With `dlc01.wad` mounted alone the earlier build hung in `WAITFORSTREAMING`, about 26k base assets absent |

## What the DLC adds, repeats and overrides

`mercs2_probe dlc-census <package> <xbox-vz.wad> <out.tsv>` joins each of the package's 7,795 chunks to Xbox
retail by (name, type). Both sides come from one toolchain, so nothing is converted. It reads chunks, not ASET
rows, so it does not say which rung a chunk sits on.

| Class | Chunks | What they are |
|---|---|---|
| New name | 2,239 | 908 textures, 212 models, 18 animations, 4 effects, 20 layers, 81 `terrainmesh`, 81 `lowresterrain`, 98 `scrub`, 721 `path`, 31 `lineregion`, 24 scripts, 7 string tables, 31 sound tables, 2 `chatter`, 1 `level` |
| Identical bytes | 2,392 | 1,417 animations, 828 textures, 82 models, 61 `path`, 3 `animationtable`, 1 `material_params` |
| Same name, other bytes | 3,164 | Below |

| Same name, other bytes | Chunks | What differs |
|---|---|---|
| Texture | 1,929 | `BODY` at one size, except one row that also differs in `INFO`. Decoded against PC, the worst mip of any of these names differs by at most 25.25 of 255, the same ceiling as the identical textures. 790 of 791 names were compared |
| Animation | 1,174 | At most 12 bytes of `data`, a median of 6. Which bytes was not read |
| Model | 38 | `INFO` and `PHY2`, a median of 96 bytes, at one size. The same values repeat across models. What they are was not read |
| Script | 12 | `mrxguihudradar`, `wifpmcinterior`, `mrxguidialogbox`, `mrxguipda`, `mrxtankbuster`, `mrxsoundbootstrap`, `wifmissionflow`, `mrxguipausescreen`, `hero`, `wifstarterdata`, `mrxbriefing`, `mrxplayer` |
| `animationtable` | 3 | `INFO` and `VALU`, each DLC table larger, `VehicleAnimationLookup` among them |
| `cfx_pack` | 3 | One in `topbar`'s block, two in `minimap`'s |
| `watermap` | 1 | 61% of its bytes, at one size |
| `foliage` | 1 | 15,074 bytes against 254,690 |
| `guidmap` | 1 | 59,099 bytes against 388,807 |
| `worldentity` | 1 | See below |
| `sounddb` | 1 | 8 of 236 bytes |

`ecs-compare` over the package and Xbox retail: the DLC's `worldentity` adds 2,772 records, the 158 templates,
removes none, and differs from the base in 12 of the 87,520 records both hold. Those 12 are tank and camera
tuning, 2 model names, 2 link targets, a light, 2 weapons and a `TerrainFade`.

The terrain rows have new names. `low_res_terrain` is a block path, and the DLC's block holds 81 `lowresterrain`
chunks and no chunk of a base key.

## What the level carries

A component is kept when its bytes are the DLC's own, and left to the copy of `vz.wad` when the base already
holds it. Every chunk with a base key also has a PC twin, 5,556 of 5,556. The call is per key, so a dropped asset
keeps the base's whole rung chain.

| Group | Verdict | Evidence |
|---|---|---|
| 1,641 new keys | Carried | New on both platforms |
| 23 real overrides | Carried | Above |
| Identical on either platform | Left to the base | Bytes |
| Textures, same name | Left to the base | Decoded against PC, at most 25.25 of 255, the ceiling of the identical textures. 218 keys also match on one platform |
| Animations, same name | Left to the base | `clip-compare`: all 2,591 clips with a base twin decode to the same transforms as PC at every key frame. The 15 others are new |
| 24 models, other bytes on both platforms | Left to the base | Xbox differs in `PHY2`'s packfile stamp and in group vertex shader keys, all registered, with the geometry identical. PS3's `INFO` equals its base, and only its `IBUF` differs, which was not read further |

## The source is the PS3 package

The PS3 version is the preferred source. Both versions disassemble, and the Shipment is reassembled from what
they yield, taking a component from PS3 first and using Xbox as a further source and a check.

`dlc-census` over the decrypted PS3 archive and `ps3-VZ.WAD`: the PS3 and Xbox archives hold the same 5,341 of
5,341 (name, type) keys, the same 1,641 keys are new on both, and the same 23 assets are real overrides. 1,084
keys are `same` on one platform and `changed` on the other, so a difference on one side alone does not show the
DLC changed that asset.

| PS3 input | Now |
|---|---|
| The package | `dlc_port --package` reads the `.pkg`, its archive and its 7 MP3 `.pws` directly |
| Textures, 913 chunks | From PS3 |
| Scripts, layers, `worldentity`, `watermap`, `foliage`, effects, strings, sound, `path`, `lineregion`, animations | From PS3. Layer and entity records equal Xbox's, 104,852 of 104,852; the 15 animations that differ in bytes decode the same; the `guidmap` differs in one word and its `CSUM` |
| Models, 212 chunks | From PS3 (`be_to_le::ps3_model`, [`ps3_model_format.md`](../reverse_engineer/ps3_model_format.md)). `model-compare` over the retail game: 2,988 of 3,007 models equal PC in triangles, materials, UVs, winding, list B, weights and bones; 16 differ in list B alone; 3 are refused, for forms the DLC does not hold. Normals and tangents are close, not equal: PS3 keeps 10 bits and no length |
| Terrain, 162 chunks | From PS3. Against Xbox, all 81 low-res cells have the same 83,224 triangles; all 81 high-res cells have the same 2,326,120 main-pass triangles, no winding differs, and about 21,000 shadow-pass triangles differ on each side, a mesh each platform baked for itself. Five PS3 vertex shader keys map to PC's by a table |
| Collision | From PS3. Its packfile objects equal Xbox's, but its MOPP codes are chunked for the SPUs with an opcode PC does not have, so each is recomputed from its mesh, which must yield the source's keys |
| `scrub`, 98 chunks | From PS3 (`be_to_le::ps3_scrub`). `scrub-compare` over retail: all 1,026 equal PC in every row, triangle and translation. Rotations are PS3's, up to 10° from PC's: instances reuse a shared table, and PC's are not in the data. Xbox holds the exact ones |

A compact `decl` is 4-byte `(type, offset, usage, components)` rows, one table per stream, which is why a
lone `03 00 08 02` looked ambiguous: it is the UV stream of a model whose other attributes are in `SSPU`.

Measuring this found converter bugs in the Xbox route too, all fixed:

| Bug | Before | After, retail against PC |
|---|---|---|
| A collider's wrapper vertex pools swept as u32, exchanging each pair of u16s | Every collision vertex scrambled | Wrapper equal |
| `WpMeshShape16` swapped only its first subpart, and counted triangles as u16s | Most of each index array scrambled | Objects equal |
| `hkpMoppCode` `m_buildType`, one byte, swapped as a u32 | Value lost | Equal |
| A low-res terrain draw kept its count when restarts were stitched | The end of each cell not drawn | 83,224 triangles of 83,224 |
| A model's `INFO +64` swapped as a u32, not two u16s | 230 of 3,007 `INFO` rows differ | 73 differ, not read further |
| A chunk's table word kept from the console | Wrong once sizes change | Computed: all 1,236 model and 9,562 texture chains follow the rule |
| `foliage`'s `tree` body swept as u32; a species' lone flag byte puts its materials off the grid (`be_to_le::foliage`) | Each material's flags and texture count traded; the game crashed in `Mtrl_Parse` at `0x00859028` | 1 of 1 equal |
| `watermap`'s `watr` body swept as u32 past its heights (`be_to_le::watermap`) | Masks scrambled, the footer off its u16 grid | 1 of 1 equal |
| A `stringdb`'s `STRS` left big-endian | Its heap read as UTF-16LE: wrong text and count; the game crashed at `0x04888CFB` | Count and heap swapped; Xbox and PC carry different text, so no byte twin; all 3 retail tables pass the count check |
| A terrain draw group's `INFO` swept as u32, though its pass groups are `u32` + 4 × `u16` | Every group's ranges transposed; `FUN_004AAAB0` drew past the group's records and crashed at `0x004AADBA` | 0 → 4,465 of 8,859 equal; the other 4,394 are the groups whose `PRMT` PC baked with other counts |
| A `lineregion`'s `data` header swept as u32, though it is four u16s (`docs/format_reference.md` §14.1) | Kind and point count traded, so the loader `0x0045E8A0` read every region as 2 points; `Speed Boundary` (`0x6743DF71`, 21 points) enclosed nothing and `DlcCon001` started out of bounds | 0 → 625 of 625 equal |
| A `soundbank`'s and a `sounddb`'s `data` swept as u32, though their counts and indices are u16 pairs and their flags bytes (`audio_code_map.md` §11.12; `be_to_le::audio::convert_soundbank`, `convert_sounddb`) | The DLC's VO sounddb read 0 cues at `+0x08`, the bound of FindCue's search (`FUN_0083c610`), so FindCue found no DLC guid; in the game every DLC `VO.Cue` ended in `sVoState=cancel` about 60 ms later, and that the miss is what reports `cancel` was not read in the PC code; every single-track cue read group 0; the DLC's global sounddb read its 19 categories as cues | Xbox `vz.wad` through `convert_block` (`chunk-compare --only`, 3 blocks refused for their wavebanks): 0 of 66 sounddbs and 0 of 65 soundbanks equal. Xbox and PS3 `vz.wad` through the converters (`sound_console_retail`): 77 of 77 sounddbs equal; 36 of 76 soundbanks, the other 40 differing only in 127 unread cue `+0x16` words and 34 indices into `English.wad`'s `vo_stream`, each bake's own |

`PHY2` across retail: 398 of 400 terrain cells and 1,560 of 1,564 models now equal PC, where 0 and 566 did,
but for 2 padding bytes after an odd vertex pool, which differ between platforms.

## The converter worklist

`mercs2_probe chunk-compare <xbox-vz.wad> <out.tsv> --vz-wad <vz.wad> --java … --unluac …` converts every
Xbox retail chunk and compares each row with its PC twin, joined by name and type. `--only <type>`
limits a re-run. A difference is either a converter gap or a real difference between the bakes, and
each row below has to be read to say which. Of 11,087 blocks `wpn_sniperrifle` is refused, whose
wavebank fails ffmpeg's decode, and so is each streamed bank, whose `.pws` this tool is not given.

| Rows differing, of those on both sides | Chunk |
|---|---|
| Every one | `anim_state_machine` `AINF` 394, `sequencetable` `ITEM` 102 and `SINF` 51, `soundbank` 75, `sounddb` 76, `facefxanimationset` 86, `facefxactor` 31, `font` `INFO` and `CHAR` 6 |
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
3. **The Shipment exists.** `shipments/blow-it-up-again/` compiles from components: 1,664 assets, 1,311
   blocks, 123 MB. What a script's `DEPS` or a model's `MTRL` names that neither the level nor the game
   holds is a build error naming it: 620 references, 359 of them only in the base. What Lua asks for by
   a name it computes is not visible to a build, which is why the level sits over a whole copy of the
   base rather than a computed subset of it.
   - Other Shipments reach the level: `qm link` compiles its whole patch against the DLC's scripts, and
     Modkit merges every overlay under it (`level_patch`). 9 of the DLC's 12 overriding scripts do not
     recompile from their decompile, so a mod's `patch_lua` is spliced into their bytecode
     (`mercs2_luac::splice`), which leaves the DLC's code byte-exact.
   - The whole DLC converts from the PS3 package alone: 7,770 chunks, none refused.
4. **The level loads in our own engine.**
   - The Level page opens `dlc01` as a level: terrain, entities, layer states.
   - `mercs2_autoplay` runs `dlc01.lua` on the script host, so the load's every asset request and
     layer call is a line we can read.
   - The `_pathfinding` layers are read, for what they say about AI movement in the base game.
5. **Shaders.** Draw the DLC's materials in the Workshop. Every key is registered on PC, so nothing needs
   `add_shader` unless a material draws wrong.
6. **Editing, base game and DLC alike.** P1, P3, P4a, then P2, then P4b, each through a Depot draft,
   exact layer lookup with the first. Each proposal's in-game test (U1, U2 and the rest of
   `level_editor.md`) runs before its edit is offered.
7. **Coverage.** A table of every chunk tag and asset type across both WADs: which can be viewed, which
   inspected, which edited. Anything short of all three is a row of work.
8. **The game.** Deploy by sha256, boot `dlc01`, score the run with `loadprobe`, gate each mission.
   Every failure goes back to the step whose test should have caught it, as a new test there.

## Engine functions PC never had

The DLC's Lua calls engine functions that the PS3 title update v1.03 (`ps3-v103-na`) added and PC v1.1 never
registered (diff `output/_exemap_exports/pc-v11.luareg.tsv` against `ps3-v103-na.luareg.tsv`; v1.03 adds
exactly 12 over `ps3-v100`). On PC an unguarded call raises a Lua error the engine swallows without a log line:
`Net.DLCGetMountPoint` stops `MrxSoundBootstrap.LoadBanks` on its first line, so no game or VO bank loads,
`VO.Cue` never calls back, and accepting a hub contract waits forever on `Mattias.Yes03`. The Xbox dev
"profile" build (`x360-preview-profile`) registers 27 more that no other image has. All 39 are ported into
the Shipment's own ASI, built from `shipments/blow-it-up-again/client/` like the other Shipments' plugins
(Rust, `i686-pc-windows-gnu`, imports `m2-sdk.dll`), declared as a `native_hook` with `touches` and
`signature_guard` per exe build.

| Binding | Source VA | What the PC port is |
|---|---|---|
| `Net.DLCGetMountPoint()` → string | ps3 `0x5370B8` | The folder the DLC's streams are opened under; PS3 returns `<gamedata>/BLUS30056_DLC/USRDIR/DLC/DLC01` |
| `Net.GetUserName()` → string | ps3 `0x537030` | The online persona; the account name `multiplayer-restored` logged in with |
| `Net.LeaderboardGetScore(board, cb[, self])` | ps3 `0x537BA0` | Async: `cb(board, rank, score)`, then a chained 30-row query to the friends slot; board `DlcCon001`–`004` |
| `Net.LeaderboardPushScore(board, score[, cb[, self]])` | ps3 `0x537D98` | Async push of the player's cash; `cb(status)`; skipped when cheats were used |
| `Net.LeaderboardGetFriends(board, cb[, self])` | ps3 `0x5379A8` | Rows `cb(name, str2, a, b, score, false)`, then `cb("", "", 0, 0, 0, true)`; no shipped Lua calls it |
| `Gui.SetMapCorrectionOffset(x, z, scale)` / `Gui.GetMapCorrectionOffset()` | ps3 `0x522C38` / `0x522108` | Three ASI-owned values (35, 40, 1.0) plus hooks on the five PC routines that hard-code `+35/+40`: minimap `0x6201A0`, GPS line `0x620AA0`, radar blip `0x621380`, co-op midpoint `0x621620`, PDA click `0x6A0DD0`; reset where PgSysGUI slot 7 resets them |
| `Gui._MarkerAddDiscDLC` + `Marker.AddDiscDLC` | ps3 `0x523ED0` | `_MarkerAddDisc` (pc `0x5B38C0`) with its creator's flag byte `(AL & 0x80) \| 0x26` and `400` for `100`; the alias line PS3's embedded Gui chunk adds |
| `Human.SetChatterSet(obj, name)` | ps3 `0x5300F8` | The PC `ChatterSet` component (descriptor `0x017BC498`; lookup `0x00587D80`, add `0x0064A570` as `0x0065BD30` calls it) |
| `Object.GetInfiniteAmmo(obj)` → bool | ps3 `0x544530` | Presence of the `CheatInfiniteAmmo` component `0x00DF8B10` |
| `Pg.SetCheatsUsed(bool)` / `Pg.WereCheatsUsed()` | ps3 `0x554B30` / `0x550B50` | ASI-owned flag; PS3 gates achievements, autosave, online session data and the leaderboard push on it |
| `Graphics.Debug.*` (20 toggles) | x360p `0x82432940`… | One flags word; PC has neither the word nor the gates, so each toggle with a reader is a hook on the PC code it gates. Six have no reader even in the profile build |
| `Graphics.Debug.VSync`, `.Prim` | x360p `0x824E7D58` | The profile build's own return-0 stub |
| `Graphics.Debug.ObjectCapping`, `.Shader` | x360p `0x82431BE0`, `0x82433350` | A value nothing reads; a flat-colour override in the material bind |
| `Ai.Debug`, `Ai.Chatter` | x360p `0x82426378`, `0x82426F08` | Perception-arc debug draw; chatter counters PC's chatter manager has no room for |
| `Sys.GetIgnoreLayers()` → table | x360p `0x8245FCA8` | The comma-split `IgnoreLayers` config value; PC's own `mrxlayermanager.lua:113` already calls it when it exists |

The reverse engineering behind each row is in the parent repo's `docs/reverse_engineer/dlc_engine_bindings.md`,
`dlc_cheats_gate.md` and `x360_profile_only_bindings.md`.

Leaderboards run over `multiplayer-restored` (a `load.requires` of this Shipment) and `mp-server`: new
`mercs2_mp_api` messages, a server table, and a small versioned C API that `multiplayer_restored.asi`
exports (leaderboard push, score, range and friends, user name, online state). This ASI resolves it at start
and refuses to start when it is absent or of another version.

Decided with the user:

- The DLC's streams install under `DLC\dlc01\Data\Audios\` and `Net.DLCGetMountPoint` returns `DLC\dlc01`,
  the PS3 layout; `data\Audios` is never written. This moves `add_level`'s `streams` destination.
- `Ai.Debug` ports what it was written to do, the perception-arc debug draw for one AI or all, not the
  dev build's broken check.
- `Sys.GetIgnoreLayers` reads a required `IgnoreLayers` key from the Shipment's own `.ini`; an empty value
  returns `{""}`, which skips nothing.
- Every exe build the other plugins support, addresses resolved by signature; a mismatch stops the game
  with a message.
- The cheats flag gates what it gates in PS3 v1.03: achievements, autosave, online session data and the
  leaderboard push.

## Venezuela gets the DLC's vehicles and skins

A `level_asset` reaches only its level, so the base world gets the DLC's assets through a new kind,
`world_asset`: one asset's PC-container bytes into the Venezuela overlay, refused for any name `vz.wad` already
holds, composed by `qm link` across Shipments; a template merges into the `worldentity` the way `add_fx`'s do.
`dlc_port` writes the rows.

- Skins: `pmc_hum_obama`, `pmc_hum_sarah`, `pmc_hum_mattias_v5` with their textures, and the chatter sets
  `Obama_Obama01` and `Palin_Palin01`, as `world_asset`; then `add_outfit` rows naming the shipped models:
  Obama and Palin for Mattias, Jennifer and Chris, GrandpaMattias for Mattias. Their `[SHELL.Costumes.*]`
  strings are checked against the base stringdb, and an `edit_stringdb` adds any that are absent.
- Vehicles: every DLC vehicle template not in `vz.wad`, with its models, textures and collision, as
  `world_asset`; then a `patch_lua` append to `mrxsupportdata` (catalog entry) and `mrxrewarddata` (reward row
  `{id, "Pmc"}`), the store recipe proven in game, so it lists in Eva's shop.

## Order of work, the ASI

Each step ends with something to look at in the game.

1. **`client/` builds and loads.** `Net.DLCGetMountPoint` and the merge of a function into an existing engine
   table (`luaL_register` reuses a global table in Lua 5.1; proven here, live). Test: the level loads 57
   banks, and accepting a hub contract leaves the hub.
2. **The DLC's own calls.** `Marker.AddDiscDLC` (contract 1's target disc), `Human.SetChatterSet`,
   `Object.GetInfiniteAmmo`, the cheats flag and its gates, the map correction and its five hooks.
3. **Leaderboards.** `mercs2_mp_api`, `mp-server`, `multiplayer-restored`, then the three `Net.Leaderboard*`
   and `Net.GetUserName`, against a local `mp-server`.
4. **The Xbox dev bindings.** `Sys.GetIgnoreLayers` first, since a shipped PC script reaches it.
5. **`world_asset`**, then the skins in the wardrobe, then the vehicles in Eva's shop.
6. **A test that fails the build** when the DLC's Lua names an engine function neither PC v1.1 nor the ASI
   registers.

## Not settled

- Whether the wedge was the base assets, the audio or both; step 8 separates them.
- How a player starts the level. `Sys.StartSingleplayer("dlc01", "dlc01")` boots it through the Lua
  bridge; nothing in the Shipment calls it yet.
- Why the hub briefing draws the lower half of the screen black from a straight horizontal line through
  the middle, cutting the starter off at the chest while the hero in front draws over it. The briefing sets
  the same depth of field and field of view as the base game's (`0, 0.3, 4, 10, 0.5`, 55°), so it is not
  the request; the camera's or the actors' placement in the hub, or the effect itself on this level, is
  unread.
- Why the player is outside `DlcCon001`'s `Speed Boundary` from the moment the contract starts. The white
  and grey noise over the mission area is the game's own out-of-bounds satellite static
  (`WifVzBoundary.BoundaryCallback`, which also plays `Fiona-None-Freeplay-None-01`, logged at mission
  start); the contract cancels 16 s later. The converter traded each region's kind and point count (bug
  table above), so the game read `Speed Boundary`, entity `0x00156B7C` in `DLC01_DlcCon001` naming
  `lineregion` `0x6743DF71`, as 2 points. Read as 21 points and moved by the entity's (x, z), it spans x
  −1525 to −285, z −1439 to −240, and holds `BombCar` at (−1187.6, −1147.7), the car `SetupVehicle`
  puts the player in; the hub point (316.7, −171.1) is outside it. `SetupVehicle` calls
  `Vehicle.Enter(car, player, "d", true)` before `AssetsLoaded`, and `MrxTaskContract.AssetsLoaded` defers
  `Activated`, which adds the boundary, to the next global exit; that `Vehicle.Enter` moves the player at
  once was not read.
- Whether DLC voice cues play once their sound tables convert field by field (bug table: the word
  swap left the DLC's VO sounddb with 0 cues, and every DLC `VO.Cue`, `Fiona-In-Mission-Contract-Dlc02-05`
  and `-18` among them, ended in `sVoState=cancel` about 60 ms later). Converted, both cues resolve
  offline to their group, wave and record (`0x34AAF50C` → group 62 → wave 62, 29,808 bytes at
  5,424,608 of `vo_stream_dlctest.english.pws`), and the stream opens from `DLC\dlc01\Data\Audios`. The
  PC path from a FindCue miss to the `cancel` state was not read past `FUN_00515300`.
- Two DLC soundbanks (`0x353FBAAF`, `0x35D82199`) play waves of base wavebanks (`0xEB61D6E1`,
  `0x7871F925`, `0x0873D14E`, `0xDD4573C5`, `0x4111ECDA`). Retail Xbox and PS3 soundbanks index
  `vz.wad`'s own wavebanks as PC does, and differ only for `English.wad`'s `vo_stream`, so these indices
  hold if the DLC was baked against the same `vz.wad` wavebanks; that was not compared.
- Who calls PgSysGUI slot 7 (the map correction's reset), and what the disc creator's flag bits and its
  `400` mean.
- Whether PS3 saves the cheats flag (byte `+0x5E` of the profile block) and clears it for a new game.
- The PC hook points for `Graphics.Debug` Filters, Occlusion, ZPass, Models, Reflections, TerrainMesh,
  LightOcclusion, RenderLights, RenderLightBounds and Shader, and for the chatter counters.
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
