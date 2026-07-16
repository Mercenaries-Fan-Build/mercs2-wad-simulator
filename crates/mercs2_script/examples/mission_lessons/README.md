# mission_lab — contracts, missions and objectives

Object scripts (see [`lua_lab`](../lessons/README.md)) are loose functions the engine calls by name.
Missions are a completely different animal, and if you carry the object-script habits over, you will
write code that fights the framework instead of using it.

Here you are writing a **class**. You get `self`. You inherit from a real base class that runs a state
machine, tracks your events, and tears everything down for you — *if* you cooperate with it, and
silently not if you don't.

## This is not a mock

The lab loads the **actual decompiled `MrxTask`** from the shipped game
([`docs/mercs2-luacd/src/resident/mrxtask.lua`](../../../../../../docs/mercs2-luacd/src/resident/mrxtask.lua))
and drives it through its real entry points:

```
MrxTask.Create(m) → Configure(tConfig) → Activate()
   → dynamic_import(sModuleName, self._ModuleLoaded, {self})
   → _ModuleLoaded → setmetatable(self, {__index = your module}) → _tEvents = {}
   → PreLoadAssets → LoadAssets → AssetsLoaded → Activated(self)     ← YOUR CODE
...
Complete()/Cancel() → Cleanup() → Event.Delete(every handle in self._tEvents)
                                → MarkForRemoval(tConfig.tLayers)
                                → cascade Cleanup() into every child objective
```

Importing it boots the real resident framework — sound banks, the shop, the GUI layer, ~240 lines of
engine chatter (muted, and counted, at the top of every run). The only thing shadowed is one module
whose auto-`Init()` would boot the whole front-end; see [`_engine_stubs/`](_engine_stubs/) for why
that's safe here.

## Run it

From `tools/wad_simulator` — note the lesson is a **module name**, not a path (that's how the game
imports):

```bash
cargo run -p mercs2_script --example mission_lab -- 01_you_forgot_the_parent
```

| file | the thing nobody tells you |
|---|---|
| `01_you_forgot_the_parent.lua` | Lua has no `super`. Defining `Activated` **replaces** the parent's — so your task never enters the state machine, `Cleanup` decides there's nothing to tear down, and your events outlive the mission. One missing line, three symptoms. |
| `02_the_event_that_outlived_the_mission.lua` | `self:_CreateEvent` is `Event.Create` plus one `table.insert`. That one line is the difference between the framework reclaiming your event and it firing forever into a world that moved on. |
| `03_the_shape.lua` | A correct contract. Note that it contains **zero teardown code**. |

## How to read the output

```
t=0.00 │ MrxTask Dynamically imported module 03_the_shape
t=0.00 │ MrxTask _SetState "MyContract" active
t=0.00 │ script  contract started
t=3.00 │ MrxTask Cleaning up MyContract
t=6.00 │ engine  — the mission is over —
```

`MrxTask` lines are **the framework narrating itself** — those are real `Debug.Printf`s from the
shipped base class, not something this lab invented. When lesson 1 goes wrong, the framework tells you
exactly why, in its own words:

```
t=2.00 │ MrxTask Not destroying task "MyContract" (Reason: latent)
```

Anything printing *after* "the mission is over" is an event that escaped.

## The one idea

**The framework will clean up everything it knows about. Your only job is to make sure it knows.**
Call your parent, and create everything through `self`. Do that and you write no teardown code at all.

## Then: the real thing

[`docs/mercs2-luacd/src/vz/pircon002.lua`](../../../../../../docs/mercs2-luacd/src/vz/pircon002.lua)
— the contract where you escort a truck full of parrots. `inherit("MrxTaskContract")`,
`MrxTaskContract.Activated(self)` as line one, `self:CreateChild{...}` objectives with `[Token]`
descriptions, `self:_CreateEvent(Event.ObjectProximity, ...)`. You can read it now.
