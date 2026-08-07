# Faithful-Tier-1 — unplugged-seam inventory

Status snapshot (branch `faithful-tier1`, 2026-08-04). Result of a 3-way read-only sweep for
mechanisms that are BUILT but not PLUGGED into the live boot/tick path. "Verified fine" items were
checked and are driven correctly — this list is deliberately honest, not alarmist.

## Root cause: host↔runtime instance split
Several subsystems exist as TWO instances — one on the script host (`GameScriptHost`/`EngineHost`,
where Lua writes land) and one on `GameRuntime` (which ticks/loads). Writes and ticks hit different
objects. Fixing this once unblocks three gaps:
- **AI** — `Ai.SetRelation` → `script_host.ai`; the perceiving `runtime.ai` is never fed → nothing
  hostile. (`script_host.rs:130` vs `runtime.rs:39/84/144`.)
- **Population** — `Ai.TweakAttachedSpawners`/lane toggles → host's *unloaded* `PopulationWorld`; the
  loaded one (`runtime.rs:49`) is never tweaked. (`script_host.rs:139/468/1104`.)
- **Faction** — `pursuit_level` lives only on the host (`script_host.rs:136`); nothing reads it.

## Tier 1 — blocks a populated, interactive mission  [IN PROGRESS]
1. **Spawn-list / `template=0`** — `SimpleSpawner::update` emits literal 0 (`population/spawner.rs:171`);
   no `SkirmishSpawnList` slot→template resolution. Corpus: runtime-seeded by `FUN_004d742c`
   (`docs/reverse_engineer/render_distance_and_density_levers.md#14-15`), COMP `0xafba5846` reader
   `FUN_0065bf00` (`docs/mercs2-ecs/02_ai_perception_population.md#27`).
2. **Spawn→Character path** — `tick_population` calls hash-only `resolver.spawn` (`runtime.rs:183`),
   never `spawn_named`/`spawn_character` → bare Prop, no AI/Health/anim bundle.
3. **`SpawnResolver.by_template` never populated** (`spawn.rs:123`, `runtime.rs:83`).
4. **`animation_system` has no caller** (`anim/lib.rs:51`) → spawned NPCs never animate.

## Tier 2 — combat & "heat" feel
5. **Vehicle wreckage** — vehicles get `Health` but no `Destructible`/machine → destruction FSM skips
   them (`destruction/lib.rs:155`); 0-HP → no wreck. Explosions damage but never shed parts.
6. **Dynamic music silent** — `MusicStateMachine` crossfades but no deck is routed to a mixer voice
   (`audio/music.rs:216`, `engine.rs:404`) → `Sound.TransitionMusic` is silent.
7. **Faction heat → music not wired** — `SetActionLevelsMusic` etc. record into a never-drained
   `sound_cmds` Vec (`bindings/sound.rs:266`); `pursuit_level` never calls `transition_music`.
8. **Spawner faction/timing channel unloaded** (`worldutil.rs:765`) — all `SpawnFaction::Vz`/1 Hz.
9. **No on-screen HUD** — `Mercs2Game` never implements `Game::ui`; widget tree + markers
   (`script_host.rs:176/178`) maintained, never drawn.

## Tier 3 — stored-but-not-applied (polish)
- Audio: `.pws` streams silent (`engine.rs:451`), EAX/reverb recorded-not-applied, Doppler computed
  but not fed to voice pitch (`spatial.rs:154`), surround = stereo pan law, `LockActionLevelMusic` dead.
- Render no-op seams (`scene.rs:2243-2254`): ZOpaque z-prepass, FadingTrees, Mirror, water
  Reflection/Wake/Occlusion; shadows are one cascade not the 4-cascade atlas.
- `Ai.SetSpawnList`/lane toggles are Nil stubs (`bindings/ai.rs:273-286`).

## Known approximations (recorded, not silently wrong)
Debris drops when its model isn't resident (`world.rs:2113`); emitters at object origin not the
hardpoint; per-instance vehicle HP / camera boom / weapon overrides are confirm-live numbers.

## Verified FINE (driven + fed)
Decals, sky, water surface, glow cards, particles, all combat-FX producer→consumer loops; physics /
combat / prop-destruction / water / resident-audio / death-ragdoll; population loader + identity fix.
Note: some `render_graph.rs` `is_seam()`/DEFERRED docs are stale (Blob + water surface do render).
