# lua_lab — learn this API by breaking it

You've read the docs. You know `Event.Create` exists and that objects have a lifecycle.
And then you wrote something reasonable and **nothing happened**, and nothing told you why.

That's not you being slow. That's this API. Almost every mistake here fails *silently*:
no crash, no warning, no error line. The engine just quietly does nothing, and you're left
staring at correct-looking code.

This lab makes the silence visible. It runs your script against a miniature engine and prints
the timeline — including every call the engine threw on the floor, and the reason.

## Run it

From `tools/wad_simulator`:

```bash
cargo run -p mercs2_script --example lua_lab -- \
    crates/mercs2_script/examples/lessons/01_the_engine_calls_you.lua
```

You'll get a timeline and a scorecard. Work through the files in order. Each one is a script
that looks right and isn't; each ends in a `STATION` comment telling you what to change. Change
it, run it again, watch the scorecard move.

The lab streams in **two** objects that share your script, wakes them, kills one, streams the
block out, and streams it back in — because most real script bugs only surface on that second
pass.

| file | the thing nobody tells you |
|---|---|
| `01_the_engine_calls_you.lua` | There's no `main()`. The engine calls **you**, by exact function name. One wrong capital letter = your script is permanently, silently dead. |
| `02_asleep_at_the_wheel.lua` | **Your object is ASLEEP when `OnActivate` runs.** Every AI call you make there is discarded. This is the big one — it's why every real script opens with that `Event.ObjectHibernation` line you thought was boilerplate. |
| `03_the_double_fire.lua` | The world *streams*. Drive away and back and your `OnActivate` runs **again**. Didn't clean up? Now everything fires twice. |
| `04_you_are_holding_a_number.lua` | `uGuid` is not an object. It's an integer. A typo'd name gives you `0`, and every call on `0` silently does nothing. |
| `05_one_script_many_objects.lua` | Your script isn't "the guard" — it's **every** guard. They share your globals, and both `OnActivate`s run before either object wakes. That's why the callback args table `{uGuid}` exists. |
| `06_the_shape.lua` | All of it, correct. Now go break it on purpose and predict what the report will say. |

## How to read the output

```
t=0.00  │ engine   block streams in — 'Outpost_Guard' is guid 7, iArg=1, state=ASLEEP
t=0.00  │ engine   → OnActivate(7, 0, 1)
t=0.00  │ script → Ai.Goal(7, "defend")        ✗ DROPPED — object is ASLEEP
t=0.60  │ engine   guid 7 → AWAKE
```

`engine` lines are things happening *to* you. `script` lines are things *you* asked for.
`✓` landed. `✗` means the engine ignored you — exactly as the real game would, without telling you.

The scorecard at the bottom scores five independent things, one per lesson. A red line for a
lesson you haven't reached yet is expected. Fix them in order.

## The one idea

If you take nothing else from this: **you are not writing a program that runs. You are
registering to be called back, into a world that is not ready yet, about an object that
might not be there — and there are several of it.** Every strange line in a real Mercenaries
script is defending against one of those facts.

Once that clicks, go read a real one: [`docs/mercs2-luacd/src/resident/soldier.lua`](../../../../../../docs/mercs2-luacd/src/resident/soldier.lua)
— infantry that drop ammo when they die. It opens with `local tEvents = tEvents or {}` and an
`OnDeath(uGuid)`, and by now you know exactly why both of those are there.

## Then: the other half

Object scripts are only half the API. Missions, contracts and objectives are a different world —
`self`, `inherit("MrxTaskContract")`, and a framework that does your teardown for you if you let
it. That's [`mission_lab`](../mission_lessons/README.md), next door.

And when you move from scripting to **assets** — models, skins, textures, WADs — the failures get
even stranger (a model that loads fine and crashes only when you *look* at it). Those are catalogued
in [`docs/modding/field_guide.md`](../../../../../../docs/modding/field_guide.md).
