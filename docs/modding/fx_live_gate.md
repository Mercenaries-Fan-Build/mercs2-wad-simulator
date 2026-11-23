# FX live gate: effects and templates through `qm link`

Three parts: what the live runs of 2026-10-04 showed (§1), the retest of `add_fx` and `replace_fx`
as `qm` ships them (§2), and the sprites the fixtures add and draw (§3), with the outcomes of its
first run (§3.5) and its second (§3.6). The container formats are in
[`../effect_container_format.md`](../effect_container_format.md) and
[`../worldentity_container_format.md`](../worldentity_container_format.md); the kinds are in
[`manifest_format.md`](manifest_format.md#add_fx).

---

## 1. Outcomes, 2026-10-04

The game ran under Wine on macOS, installed through Modkit, driven over the Lua bridge
(`tools/lua_repl.py` in the Ess repo). Four archives were used, each a `raw` Shipment:

| Archive | What it shipped |
|---|---|
| G0 | the retail `worldentity` `0x50075B3B`, re-written by the codec (identical bytes), as a one-entry block |
| G1 | the same container with the template `qm_gate_c4` appended under its derived key `0x8D9E11CB` (`0x8` + the low 28 bits of `pandemic_hash_m2("qm_gate_c4")` = `0xAD9E11CB`) |
| FX-A | the whole effects block (314 effects, 46 models), `global_explosion_c4` recoloured magenta |
| FX-B | the magenta `global_explosion_c4` alone, as a one-entry block |

`qm_gate_c4` was declared field by field through the template form with the values of the retail C4
template `global_particle_explosion_c4` (`0x80008028`), so a working spawn of it is a C4 explosion.

### Run 1: G0 + FX-A

- A placed C4 charge exploded magenta.
- `Pg.Spawn("global_particle_explosion_c4")` exploded magenta.
- `ObjectState.StartEmitter` on a Monster Truck's `hp_fx_exhaust_a` with the C4 template fired it.
- "Humvee (Cargo)" spawned as a prop without physics.
- `Pg.GetGuidByName`: the retail C4 template returned `80008028`; a name no template has returned
  `nil`.

### Run 2: G1 + FX-B

- `Pg.GetGuidByName("qm_gate_c4")` returned `8D9E11CB`.
- Spawned templates, `qm_gate_c4` and the retail C4 alike, played their sound with no visual.
- A placed C4 charge exploded with the retail look, not magenta.

### Run 3: G1 + FX-A

- `Pg.Spawn("qm_gate_c4")` showed the C4 explosion.
- `ObjectState.StartEmitter` on the Monster Truck's `hp_fx_exhaust_a` with `qm_gate_c4` fired it.

### What the runs settle

| Question | Answer |
|---|---|
| Is a `worldentity` shipped in a patch block processed? | Yes: G1's appended template resolved by name (Run 2) and spawned (Run 3). |
| Does a template key derived from its name work? | Yes: `0x8D9E11CB` resolved, spawned, and fired from `StartEmitter`. |
| Does `StartEmitter` resolve a new template like a retail one? | Yes (Run 3). |
| Does an effect ship as the whole effects block? | Yes: the whole block recoloured the C4 explosion (Run 1, Run 3). |
| Does an effect ship as its own one-entry block? | No: with FX-B, spawned effects lost their visuals and the charge kept the retail look (Run 2). That the lone-effect block caused it is SPECULATIVE; the cause was not traced. |

`qm` ships effects only as the whole effects block at its own path, and templates only inside the
resident block's `worldentity`, both merged across the installed set by `qm link`
([`manifest_format.md`](manifest_format.md#effects-and-templates-are-merged)).

---

## 2. Retest: `qm-fx-a` and `qm-fx-b`

The two Shipments are fixtures of the qm workspace, `crates/mercs2_quartermaster/tests/fixtures/fx/`:

| Shipment | Contributions |
|---|---|
| `qm-fx-a` | `add_fx_sprite` `qm_fx_a_ring`; `add_fx` `qm_fx_cyan_burst` (a cyan one-emitter burst drawing the ring) with template `qm_cyan_burst`; `replace_fx` of `global_explosion_c4`, every emitter magenta |
| `qm-fx-b` | `add_fx_sprite` `qm_fx_b_star`; `add_fx` `qm_fx_green_burst` (a green one-emitter burst drawing the star) with template `qm_green_burst`; `replace_fx` of the effect the template `global_particle_fire_carhood` starts, every emitter yellow |

Each template carries the retail C4 template's components (`EffectTemplate`, `HibernationControl`,
`RedEffectComponent`, `SoundEffect`), its `RedEffectComponent` naming the Shipment's own effect, so a
spawn of it plays the C4 sound with the Shipment's burst. Each burst is authored: one emitter
drawing the Shipment's sprite (§3), with `size` 0.6, `life` 1.5, `rate` 30, `speed` 4 and `spread`
30, 100 colour keys whose fourth byte fades from 255 to 0, and one gravity force.

| Template | Derived key |
|---|---|
| `qm_cyan_burst` | `0x899E7D78` |
| `qm_green_burst` | `0x8E53AD44` |

### 2.1 Install

With Modkit's `headless_deploy` (`src-tauri/examples/headless_deploy.rs`), while the game, its Wine
prefix and the Modkit app are closed:

1. `snapshot --real --game-root <R> --out <S>.json`, where `<R>` is the game folder.
2. `install --real --game-root <R> --qm <qm> --rows <rows>.json --shipment <fixtures>/qm-fx-a`,
   where `<rows>.json` is the installed set to keep (the Lua bridge Shipments the session needs) and
   `<fixtures>` is `crates/mercs2_quartermaster/tests/fixtures/fx`.
3. `install` again with `--shipment <fixtures>/qm-fx-b` and the rows of step 2 plus `qm-fx-a`.

Each install's report lists every Shipment's outcome. All must be **Applied**, with no claim
conflict. The final `vz-patch.wad` carries one `blocks\VZ\effects_P000_Q3.block` and one
`blocks\VZ\resident_P000_Q3.block`, both from the link group.

### 2.2 In the game

The user launches the game, loads the save in the open world and stands in the open. Each command is
`python tools/lua_repl.py --code '<code>'` from the Ess repo; record each printed result.

```lua
-- names: each new template resolves under its derived key; a missing name is nil
return tostring(Pg.GetGuidByName("qm_cyan_burst")) .. " | " .. tostring(Pg.GetGuidByName("qm_green_burst")) .. " | " .. tostring(Pg.GetGuidByName("qm_fx_no_such_template"))
-- expect: 899E7D78 | 8E53AD44 | nil

-- spawns, 15 m from the player: cyan, green, magenta, yellow
local x,y,z = Object.GetPosition(Player.GetLocalCharacter()); return tostring(Pg.Spawn("qm_cyan_burst", x+15, y, z))
local x,y,z = Object.GetPosition(Player.GetLocalCharacter()); return tostring(Pg.Spawn("qm_green_burst", x+15, y, z))
local x,y,z = Object.GetPosition(Player.GetLocalCharacter()); return tostring(Pg.Spawn("global_particle_explosion_c4", x+15, y, z))
local x,y,z = Object.GetPosition(Player.GetLocalCharacter()); return tostring(Pg.Spawn("global_particle_fire_carhood", x+15, y, z))
```

`Pg.Spawn` takes only names `GetGuidByName` resolved: what a spawn of a name no template has does
is not recorded, and an empty name crashes the engine (Ess `src/49_ui_menu.lua:79`).

Then place and detonate a C4 charge: it explodes **magenta**.

Then the emitter:

```lua
-- a Monster Truck (template "Monster Truck", 0x80006C99) to emit from
local x,y,z = Object.GetPosition(Player.GetLocalCharacter()); qmTruck = Pg.Spawn("Monster Truck", x+12, y, z); return tostring(qmTruck)
-- (wait a second: a fresh spawn's hardpoints read nil for ~0.3 s)
return tostring(Object.GetHardpointPosition(qmTruck, String.GetHash("hp_fx_exhaust_a")))
ObjectState.StartEmitter(qmTruck, String.GetHash("hp_fx_exhaust_a"), String.GetHash("qm_cyan_burst")); return "sent"
```

The cyan burst plays at the exhaust.

| Observation | Meaning |
|---|---|
| both names resolve to their keys, the missing one is `nil` | `qm link`'s resident block carries both Shipments' templates |
| cyan and green bursts | each added effect is in the effects block and its template starts it |
| magenta C4, spawned and placed | `qm-fx-a`'s edit of a game effect, named directly |
| yellow car-hood fire | `qm-fx-b`'s edit, reached through a template |
| cyan burst at the exhaust | `StartEmitter` resolves an added template |

### 2.3 With a script Shipment

Uninstall both (§2.4), then repeat §2.1 with `/Volumes/Projects/mercs2-unofficial-patch` in the rows
and `qm-fx-a`, `qm-fx-b` installed after it. Every outcome is **Applied**. The resident block in the
final `vz-patch.wad` is link's and carries the unofficial patch's linked `mrxplayer` and both
templates; `headless_deploy blocks --wad <R>/data/vz-patch.wad --overlay <link WAD>` reports each of
the link WAD's blocks as byte-identical in the final WAD. Then run §2.2 again.

### 2.4 Uninstall

`uninstall --real --game-root <R> --qm <qm> --rows <rows>.json` with the rows of the set before the
fixtures, then `verify --real --game-root <R> --snapshot <S>.json`: the game folder matches the
snapshot.

### 2.5 Reporting

For each command, the printed result; for each colour, what was seen; for any crash, the last
command. Record each install's outcomes and the sha256 of the final `vz-patch.wad`.

---

## 3. Sprites: `qm-fx-a` and `qm-fx-b` draw their own

Each fixture adds a sprite and its burst draws it:

| Shipment | Sprite | Burst |
|---|---|---|
| `qm-fx-a` | `qm_fx_a_ring`, a 64² white ring (alpha `clamp(1 − abs(r − 24) / 4)`, `r` the distance from the centre) | `qm_fx_cyan_burst`, template `qm_cyan_burst` |
| `qm-fx-b` | `qm_fx_b_star`, a 64² white five-pointed star (alpha `clamp(R(t) − r)`, `R(t) = 10 + 18·abs(cos(5t/2))`) | `qm_fx_green_burst`, template `qm_green_burst` |

`qm link` draws both into the free square of the `vfx` atlas, the 512² at (1536, 0): the ring at
(1536, 0) and the star at (1600, 0), both 64², whatever the load order. The fxdict carries 632
records, the game's 630 and the two sprites'
([`manifest_format.md`](manifest_format.md#sprites-and-the-vfx-atlas-are-merged)).

### 3.1 Install

As §2.1, with the game, its Wine prefix and the Modkit app closed: `snapshot`, then `install` of
`qm-fx-a`, then `install` of `qm-fx-b` with `qm-fx-a` in the rows. Every outcome is **Applied**, with
no claim conflict. The final `vz-patch.wad` carries one `blocks\VZ\resident_P000_Q3.block`, from the
link group, with the fxdict and the atlas in it.

### 3.2 In the game

The user launches the game, loads the save in the open world and stands in the open. Each command is
`python tools/lua_repl.py --code '<code>'` from the Ess repo.

```lua
local x,y,z = Object.GetPosition(Player.GetLocalCharacter()); return tostring(Pg.Spawn("qm_cyan_burst", x+15, y, z))
local x,y,z = Object.GetPosition(Player.GetLocalCharacter()); return tostring(Pg.Spawn("qm_green_burst", x+15, y, z))
```

| Seen | Meaning |
|---|---|
| cyan rings; green stars | the sprites work: the fxdict and the atlas are link's |
| particles each showing a mosaic of the whole atlas | the fxdict was not loaded: a frame the lookup misses draws `(0, 0, 1, 1)`, the whole atlas |
| the burst's sound and nothing else | the atlas body was not loaded: the frames' rectangles lie in the game's free square, whose texels have alpha 0 |

Then spawn `global_particle_explosion_c4` and `global_particle_fire_carhood` as in §2.2, and place and
detonate a C4 charge: the C4 explosion is magenta and the car-hood fire yellow, each with **its
retail shapes**. The game's 630 rectangles and the atlas outside the free square are the game's.

### 3.3 Uninstall

As §2.4: `uninstall` with the rows of the set before the fixtures, then `verify` against the
snapshot: the game folder matches it.

### 3.4 Reporting

For each spawn, what was seen and which row of the table it matches; for any crash, the last
command. Record each install's outcomes and the sha256 of the final `vz-patch.wad`.

### 3.5 Outcome, 2026-10-06

The game ran under Wine on macOS. `qm-fx-a` and `qm-fx-b` were installed through `headless_deploy`,
with `qm` built at `99994025`, whose fixtures gave each burst's one emitter `geom: none` (with `rate`
30) and one shape of a single record of 13 zeros.

- `Pg.GetGuidByName("qm_cyan_burst")` returned `899E7D78` and `Pg.GetGuidByName("qm_green_burst")`
  returned `8E53AD44`: both templates registered.
- `Pg.Spawn("qm_cyan_burst", x, y + 1, z + 15)` crashed the game. `pmc_blackbox.log`:

  ```text
  VEH EXCEPTION C0000094 INT_DIVIDE_BY_ZERO @ EIP=0048AFF6
  EAX=7EC00771 ECX=00000771 EDX=00000000 EBX=03C09968 ESP=03C039C0 EBP=03C03BA0 ESI=00000000 EDI=1F5C63F0
  stk+060 = 0048FE0D
  [EDI=1F5C63F0] 00000000 00000000 00000000 00000000 3F800000 ...
  ```

The sprite rows of §3.2 were not reached.

**Cause.** `EDI` is the emitter's runtime record, whose `+0x00` is the `GEOM` count and `+0x04` the
shape's records; without a `GEOM` both are 0, and the spawn picks each particle's record as
`random % count` with `div esi` at `0x0048AFF6`; `0x0048FE0D` is the return into `FUN_0048f900`.
PROVEN by the disassembly and the dump
([`../effect_container_format.md`](../effect_container_format.md) §2.1).

**The next run.** `qm` refuses an emitter the engine cannot spawn from as **M0309**
([`manifest_format.md`](manifest_format.md#m0309)). The fixtures' emitters spawn on shapes of their
own: `qm_fx_cyan_burst` on the eight faces of an octahedron of radius 0.25 about the effect's origin
(`geom: { shape: 0, word: 8 }`), `qm_fx_green_burst` on a 0.5 × 0.5 square in the `y = 0` plane, two
triangles (`geom: { shape: 0, word: 2 }`). The next run repeats §3.1–§3.4 with them.

### 3.6 Outcome, 2026-10-07

The game ran under Wine on macOS, with `qm-fx-a` and `qm-fx-b` installed with the fixtures of §3.5,
whose emitters spawn on shapes of their own.

- `Pg.GetGuidByName("qm_cyan_burst")` returned `899E7D78` and `Pg.GetGuidByName("qm_green_burst")`
  returned `8E53AD44`: both templates registered.
- `Pg.Spawn("qm_cyan_burst", …)` drew a fountain of ring sprites, gold. Its colour keys, authored
  red 0, green 255, blue 255, were stored `00 FF FF`.
- `Pg.Spawn("qm_green_burst", …)` drew green five-point star sprites (keys stored `00 FF 00`).
- `global_particle_fire_carhood`, recoloured red 255, green 255, blue 0 (yellow), drew cyan (keys
  stored `FF FF 00`). The C4, recoloured magenta (`FF 00 FF`), drew magenta.

**Conclusion.** A `COLR` key stores its colour blue, green, red, alpha. PROVEN by the gold rings
and the cyan car-hood fire, each the red/blue swap of what its bytes give read red-first; green and
magenta are the same under that swap
([`../effect_container_format.md`](../effect_container_format.md) §6). `qm` takes red, green, blue,
alpha from the effect form and the edits and writes them blue, green, red, alpha. The sprite packing,
the linked fxdict and the atlas work end to end in the game: each burst drew its own sprite, the
ring and the star, from the free square of the `vfx` atlas.

### 3.7 Outcome, 2026-10-07 (channel order applied)

The game ran under Wine on macOS, with `qm-fx-a` and `qm-fx-b` built by a `qm` that writes `COLR`
keys blue, green, red, alpha (`vz-patch.wad` sha256 `58c7965e…`).

- `Pg.Spawn("qm_cyan_burst", …)` drew a fountain of cyan ring sprites.
- `Pg.Spawn("global_particle_fire_carhood", …)`, recoloured red 255, green 255, blue 0, drew a
  yellow fire.

**Conclusion.** The effect form and the edits produce the colours they name. `add_fx`,
`replace_fx`, `add_fx_sprite` and the linked templates, fxdict and atlas draw as authored in the
game.

### 3.8 Outcome, 2026-10-07 (with a script Shipment, §2.3)

The game ran under Wine on macOS. Into the game folder, through `headless_deploy install --real`, in
order: m2-sdk 0.2.0, the unofficial patch, `qm-fx-a`, `qm-fx-b`. Every outcome was **Applied** under
`qm-link:scripts`, with no claim conflict. The final `vz-patch.wad` sha256 was `0a8236e9…`, the same
as the scratch install of the same set.

- Decoding the final `vz-patch.wad`: link's resident block carries `mrxplayer` with
  `_QmGetClipAmmo` and `_QmRestoreAmmo` in its constants, and the templates `qm_cyan_burst` and
  `qm_green_burst`; the effects block carries `qm_fx_cyan_burst` and `qm_fx_green_burst`.
- `scripts/unofficial_patch.log` read `m2 0.2.0`, 4 armed, 0 failed. `pmc_blackbox.log` loaded every
  ASI and `vz-patch.wad` `0a8236e9…`.
- `Pg.GetGuidByName("qm_cyan_burst")` returned `899E7D78` and `Pg.GetGuidByName("qm_green_burst")`
  returned `8E53AD44`.
- `Pg.Spawn("qm_cyan_burst", …)` drew cyan rings; `Pg.Spawn("global_particle_fire_carhood", …)` drew
  a yellow fire.
- `_QmGetClipAmmo` and `_QmRestoreAmmo` were not reached from `lua_repl`: both read `nil` in its
  globals, as does the game's own `SaveSingleton` from `mrxplayer`. UNTESTED in the game.

`uninstall` with no Shipments removed `m2-sdk.dll`, `scripts/unofficial_patch.asi`,
`scripts/unofficial_patch.ini` and `vz-patch.wad`; `restore-wad --file vz-patch.58c7965e48ffeabb.wad`
put back the `vz-patch.wad` of before the run. `verify` then matched the snapshot except the files the
run itself wrote: `d3d.log`, `pmc_blackbox.log`, `scripts/lua_bridge_DEV.log`,
`scripts/lua_loader_printf.log` and `scripts/unofficial_patch.log`.

**Conclusion.** `add_fx`, `replace_fx`, `add_fx_sprite` and the templates draw as authored alongside a
Shipment that links resident scripts: link's one resident block carries both.
