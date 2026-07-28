# Corpus stand-ins

Modules the shipped game had that the **370-of-382** decompile does not include. This root is
registered *after* `corpus/mercs2-luacd/src` (see `corpus::roots`), so a module that later decompiles
automatically shadows its stand-in here — at which point the stand-in should be deleted.

Nothing here invents game content. A stand-in exists only where a module's *absence* is a hard error
rather than a missing detail, and it supplies the minimum shape the caller type-checks.

## `Subtitles_*` — 35 files

The whole subtitle-localization family is missing from the decompile (`find … -iname 'subtitles_*'`
over the corpus returns nothing). The shipped game obviously had them, so their absence is a gap in
our data, not in the engine.

They cannot simply be skipped. `MrxGuiCinematic.ShowMovie` (`resident/mrxguicinematic.lua:72-91`)
will not play a subtitled movie until it has the subtitle table:

```lua
if bSubtitles and not tSubtitles then
  dynamic_import("Subtitles_" .. sFile, SubtitleImportCallback, {...})
  return
end
```

and `SubtitleImportCallback` (`:146-156`) re-enters `ShowMovie` with whatever it got. So the module
must resolve **and** expose a `SubtitleData` **table**, or the pair loops forever. Raising instead
aborts the caller's callback chain and strands `MrxState` part-way through a transition — which is
exactly what stalled the boot at `GameStateChange(WaitForStreaming, exit)`.

Each file is `SubtitleData = {}`. Empty is correct: `type(tSubtitleData) == "table"` is the only
thing the caller checks, and an empty table means "this movie has no subtitle lines" rather than
inventing dialogue we do not have.

### Why 35

Movie ids are built as `sMovie = "<id>_" .. sHeroLetter` (e.g. `vz/wifmissionflow.lua:65`), where the
letter is the hero identity from `MrxUtil.GetCharacterIdentity` — **M**attias, **J**ennifer, or
**C**hris. That is why a Chris playthrough requests `Subtitles_01_AOA_C` and a Mattias one requests
`Subtitles_01_AOA_M`: same cinematic, different per-hero track.

Eleven cinematics take a hero letter — `01_AOA`, `02_AOB`, `06_YNH`, `07_RHE`, `08_RME`, `09_RJE`,
`10_BRV`, `12_CAR`, `13_AVI`, `14_CVI`, `15_ACK` — giving 11 x 3 = 33, plus the two that ship with a
fixed suffix and no per-hero variant, `11_SR1_S` and `11_SR2_S`. 35 in total.

The id list is every distinct `sMovie = "…"` in the corpus:

```
grep -rhoE 'sMovie *= *"[^"]+"' corpus/mercs2-luacd/src --include='*.lua' | sort -u
```

If that command ever yields an id not covered here, the boot will fail loudly on it — script errors
are fatal (`mercs2_engine::script_host::lua_fatal`), so a new movie cannot silently half-play.
