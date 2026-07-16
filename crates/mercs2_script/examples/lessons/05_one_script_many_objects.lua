-- ═══ LESSON 5 ═══  Your script is not "the guard". It is EVERY guard.
--
-- Here is the assumption that quietly wrecks scripts, and no doc states it:
--
--     ONE script serves MANY objects.
--
-- You didn't write a script for *an* outpost guard. You wrote the script that runs for the
-- guard, AND the sniper, AND the other forty placements of that type across the map. They
-- all share your globals. All of them.
--
-- Worse, look at the order the engine does things in:
--
--     OnActivate(guard)     ← both fire...
--     OnActivate(sniper)    ← ...before EITHER wakes up
--     guard wakes  → your callback runs
--     sniper wakes → your callback runs
--
-- So by the time the first callback runs, any global you set in `OnActivate` is holding the
-- LAST object's value. Both callbacks think they're the sniper.
--
-- Run it. The guard wakes up and gets absolutely nothing; the sniper gets set up twice.
--
--     cargo run -p mercs2_script --example lua_lab -- \
--         crates/mercs2_script/examples/lessons/05_one_script_many_objects.lua


function OnActivate(uGuid, uRuntimeOwner, iArg)
	-- A global. There is exactly ONE of these, no matter how many guards exist.
	uMe = uGuid

	-- Note the missing 4th argument: no `{uGuid}`. The callback gets no arguments,
	-- so it has no way to know which object it is for — except the global. Which is wrong.
	Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupGuard)
end

function SetupGuard()
	Ai.Goal(uMe, "defend")
	Ai.SetState(uMe, "alert", true)
end

function OnDeactivate(uGuid)
	-- (leaving this incomplete on purpose — one bug per lesson)
end


-- ─── STATION 5 ────────────────────────────────────────────────────────────────
--
-- The per-object report shows it plainly: one object woke up and received NOTHING, while its
-- sibling got served twice. That's not a race or a fluke. It's arithmetic: one global, two
-- objects, second write wins.
--
-- In the real game this is the bug where "the mine only works if there's exactly one on the
-- map", or "whichever turret spawned last is the only one that shoots".
--
-- The API already solved this for you, and it's the argument you've been copying without
-- knowing why. `Event.Create` takes a FOURTH parameter: a table of arguments handed back to
-- your callback when it fires.
--
--     Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupGuard, {uGuid})
--                                                                          ^^^^^^^
--                                             this is how the callback learns WHICH object
--
-- That's it. That's what `{uGuid}` is for. It is per-object identity, carried through time,
-- from the moment you registered to the moment the engine calls you back.
--
-- YOUR MOVE:
--
--   1. Pass the identity through, and take it as a parameter instead of reading a global:
--
--          function OnActivate(uGuid, uRuntimeOwner, iArg)
--              Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupGuard, {uGuid})
--          end
--
--          function SetupGuard(uGuid)          -- <— now it knows who it is
--              Ai.Goal(uGuid, "defend")
--              Ai.SetState(uGuid, "alert", true)
--          end
--
--      Run it. Both objects get served. Delete the `uMe` global entirely.
--
--   2. Now the same rule for anything you need to REMEMBER per object. You can't use a plain
--      global — you need one slot per guid. That's why every real script has a table like this:
--
--          tEvents = {}
--          tEvents[uGuid] = Event.Create(...)
--
--      A global that is a TABLE KEYED BY GUID is fine. A global holding one object's value is not.
--
--   3. While you're here — meet the third parameter, `iArg`. The lab streams in two placements
--      with different `iArg`s (1 and 3). It's an integer the level editor baked into each
--      placement, and it's how one script becomes many behaviours: `antiair` uses it for tier
--      1–4, `mine` uses it to pick trigger mode. Try giving them different goals:
--
--          local sGoal = "defend"
--          if iArg >= 3 then sGoal = "snipe" end
--
--      ...but notice you'll have to carry `iArg` through the event's argument table too —
--      `{uGuid, iArg}` — for the same reason as before. Everything per-object travels in that
--      table or it doesn't travel at all.
-- ──────────────────────────────────────────────────────────────────────────────
