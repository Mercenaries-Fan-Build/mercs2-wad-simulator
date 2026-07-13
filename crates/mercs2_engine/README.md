# mercs2_engine

The native 64-bit engine of the Mercenaries 2 modernization — a Rust/wgpu reimplementation that runs
the retail game's own data (see `docs/modernization/00_charter.md`).

## What it is

A **pure library** (no binary, no `main`, no argument parsing). The engine is asset-agnostic
machinery; the game exe (`mercs2_game`) and the tooling (`mercs2_probe`) depend on it and configure
it. It provides:

- **An application shell** — one `winit` event loop + wgpu device/surface, `Time`, the fixed-step
  tick, raw input plumbing, background-load polling and the loading screen. A `Game` implementor
  supplies only *policy* through hooks (`config` / `spawn_loader` / `setup` / `update` /
  `fixed_update` / `render_prep` / `ui`). Both boots — the dev free-fly and the TPS game — run on
  this single loop (`app`).
- **A streaming world** — `game_world::run_game_world` owns the background WAD loader and the
  per-frame streaming executor: it loads/unloads c3 cells and terrain tiles and wakes/hibernates
  `ModelName` props by proximity.
- **The asset layer** — `AssetSource` (a base `vz.wad` plus an ordered stack of patch/overlay WADs,
  last-writer-wins) over a block-residency `AssetRegistry` whose hash-keyed chunk tables are
  first-wins, mirroring retail. Cross-block model assembly, texture extraction and high-mip
  streaming live here.
- **A render path** — a named-node render graph, a shadow-depth pass, a forward color pass with sky,
  an HDR + bloom post chain, a translucent water-surface node, a CPU billboard particle pass, and a
  2D UI/text overlay. Shaders are in `src/*.wgsl`.
- **The Lua host + simulation cluster** — `script_host` implements `mercs2_script`'s `EngineHost`
  seam; `spawn` resolves a template to an ECS archetype; `runtime`/`gameplay` tick the fleet
  subsystems (physics / vehicle / combat / audio) over the `mercs2_core` ECS `World`.

The engine also **owns and re-exports** every mechanism crate, so the game can depend on the engine
alone: `mercs2_engine::{ai, anim, audio, combat, decal, faction, physics, population, script,
vehicle}`, plus `widgets` (`mercs2_ui`, the retained HUD widget tree) and `water_sim` (`mercs2_water`,
the watermap/swim data crate) — renamed to avoid clashing with the engine's own `ui` and `water`
render modules.

## Where it comes from

The engine consumes the **original game's data** (WADs) through `mercs2_formats`; the retail exe and
its decompilation remain the oracle/spec, not the shipping artifact. The provenance the source
itself records:

| Subsystem | Oracle |
|---|---|
| `render_graph` — per-viewport pass order | `FUN_00466d40`, the one shared scene-pass driver (`docs/reverse_engineer/render_core_code_map.md` §5/§11, `water_code_map.md` §2, `shadow_code_map.md` §4) |
| `render_state` — the per-segment draw gate | `FUN_004722a0` (blob-shadow pass) / `FUN_00472a50` (main pass); `view_state` recomputed per frame from camera distance via `FUN_00470740` → `FUN_0047724e`. `docs/modernization/model_render_gate_spec.md` |
| `registry` — chunk registry insert is **first-wins** | `FUN_004cc130`; the probe `FUN_008242b0` is `slot = key % size` with an 8-way unrolled linear scan (verified live) |
| `camera` — mode-based rig + presets | `docs/reverse_engineer/camera_code_map.md`; `HumanCameraModifier` schema defaults from `FUN_0065eaf0`, `CameraCarPreset` from `FUN_0065e1d0`; on-foot offset from a live x32dbg capture; near/far from the game's own Lua (`Graphics.Camera.SetNearFar(0, 0.3, 500, 0)` in `wifpmcinterior.lua`) |
| `water` — the water-surface node | the exe's `FUN_00487540` / `FUN_00487dd0` slot |
| `model` — cross-block LOD assembly | the `<model>_P00N_Q(3-N)` block chain; only the resident block ships `HIER`/`SEGM`/`MTRL`/`SWIT` |
| `input` — action bindings | the retail `Mercs2.ini` (`[Actions1]`, `[Actions2]`, `[Mouse]`, `[Controller (XBOX 360 For Windows)]`) |
| `wad` — install discovery | `HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames` → `Install Dir` + `data\vz.wad` |

The engine/game boundary rule is `docs/modernization/pangea_engine_alignment.md` §6: **mechanism →
engine; selection / content / tunables → game.**

## Usage

This is a library — there is nothing to `cargo run`. Boot the streaming world in-process and use the
`populate` hook to spawn your own entities once the base geometry has loaded:

```rust
use mercs2_engine::{game_world, wad};

fn main() {
    // Find the retail install (Windows registry), or pass a path.
    let vz = wad::registry_vz_wad().expect("vz.wad not found");

    pollster::block_on(game_world::run_game_world(
        vz,
        Some([3794.0, 451.0, -3911.0]), // spawn; None = default exterior bird's-eye
        Vec::new(),                     // vz_state overlay layer names
        |_world: &mut mercs2_core::World,
         _scene: &mut mercs2_engine::scene::Scene,
         _wad: &mut wad::Wad| {
            // GAME hook: base geometry is loaded — spawn the player, interiors, ...
        },
    ));
}
```

Opening the asset stack directly (base + the sibling `vz-patch.wad`, auto-discovered):

```rust
use mercs2_engine::asset::AssetSource;

let mut src = AssetSource::discover("C:/.../data/vz.wad", &[])?;
let container = src.extract_container(0x9FCAE910)?; // oc_veh_helicopter_md500
let tex = src.extract_texture_hires(0x22101D86)?;
# Ok::<(), String>(())
```

For a full game rather than the free-fly boot, implement `app::Game` and call `app::run(game)` — the
engine owns the window, loop and render; you supply the config, the loader, and the per-frame policy.

## Modules

| Module | Owns |
|---|---|
| `app` | The application shell: winit event loop, `Game` trait, `GameConfig`, `Ctx`, fixed step, loading screen. |
| `asset` | `AssetSource` — base WAD + ordered overlay/patch stack, resolved last-wins. |
| `camera` | Mode-based camera rig (`CameraMode` → `CameraPreset`) + boom-collision math. |
| `diag` | Headless, render-agnostic diagnostic/export subcommands consumed by `mercs2_probe`. |
| `game_world` | `run_game_world` + `StreamingWorld`: the streaming executor and its render-coupled WAD loaders. |
| `gameplay` | `GameplaySystems` — the fleet subsystems (physics/vehicle/combat/audio) wired into the fixed tick. |
| `input` | Data-driven action/binding layer from `Mercs2.ini`; keyboard, mouse and `gilrs` gamepad. |
| `mesh` | UCFX container → indexed geometry (`Vertex`, `BoneRig`). |
| `model` | Cross-block model assembly over the `_P00N_Q` LOD chain. |
| `particles` | CPU billboard particle system driven by `fxdict` effect templates. |
| `player` | `PlayerController` — third-person locomotion, collide-and-slide, ground snap, clip FSM. |
| `pose` | Skinning-palette recomposition from a `BoneRig` + local transforms. |
| `post` | HDR target + bright-pass → gaussian bloom → tone-map chain (fallible; degrades to plain forward). |
| `registry` | `AssetRegistry` — block residency + hash-keyed chunk tables (first-wins). |
| `render` | Shared render types + wgpu helper glue (`LoadedModel`, `ClipAnim`, `TexMap`, `LoadProgress`). |
| `render_graph` | The named-node scene pass order recovered from `FUN_00466d40`. |
| `render_state` | Per-object render state + the three-clause per-segment draw gate. |
| `runtime` | `GameRuntime` — realizes script spawn intents and ticks the fleet; no GPU state. |
| `scene` | The multi-entity wgpu `Scene` renderer over the ECS `World`. |
| `script_host` | `GameScriptHost` — the engine's `EngineHost` impl; records `Pg.Spawn` intents. |
| `spawn` | `SpawnResolver` — spawn template → ECS archetype (plain prop vs. full fleet entity). |
| `ui` | 2D overlay pass: screen-space quads + monospace text (`ui.wgsl`). |
| `wad` | FFCS/`vz.wad` access: open, block decompress, ASET/container extraction, textures. |
| `water` | `WaterNode` — the translucent water-surface `RenderNode`. |
| `worldutil` | Render-agnostic world/asset helpers: `HeightMap`, the streaming decision catalog, reverse-hash utils. |

## Notes / gotchas

- **Two different "overlay" vocabularies — do not conflate.** `AssetSource`'s overlays are whole patch
  **WAD files** stacked on top of the base. The `overlays` argument to `run_game_world` /
  `load_streaming_world_data` / `worldutil::add_overlay_to_catalog` is a different thing: `vz_state`
  layer **blocks inside one wad**, folded into the streaming catalog.
- **The two resolution rules run at once, in opposite directions.** Which *block* becomes resident for
  a hash is resolved by walking the WAD stack in reverse (**last** overlay wins). Once a block is
  resident, its chunks register into the global tables **first**-wins. That is exactly how retail
  composes them.
- **Deliberate divergence from retail:** retail never evicts registry entries and stores no owning
  block id, so a stale reference resolves to a null sentinel and is dereferenced unchecked (the AV at
  `0x47AA5C`). This engine tracks the owning block, evicts its chunks with it, and returns `None`.
- **LOD rung and destruction state are orthogonal axes** of the draw gate, not one mechanism. Getting
  this backwards put an intact helicopter and its wreck on screen simultaneously.
- **A model is not one container.** Resolving a fine LOD rung against its own (absent) `SEGM` is what
  made every vehicle render as a low-poly proxy; the resident block's `SEGM` is the master table for
  the whole chain. `tests/model_assembly.rs` fails if single-container loading ever returns.
- **The render graph is mostly seams.** Canonical passes that are not yet implemented render *nothing*
  (they are not faked), so executing the full `SCENE_ORDER` reduces to the engine's current command
  sequence.
- **UI font:** a system monospace TTF is rasterized with `fontdue` at `UiPass` creation
  (`MERCS2_UI_FONT` overrides the search); when none is found, the `font8x8` bitmap set is baked
  instead, so the overlay always works.
- `Post::new` returns `Option` — on failure the caller renders straight to the swapchain rather than
  breaking.
- **Tests:** `shader_validation` parses + validates every WGSL with `naga` (no GPU needed);
  `registry_wad_probe` is `#[ignore]`d because it needs the retail install —
  `cargo test -p mercs2_engine --test registry_wad_probe -- --ignored --nocapture`.
