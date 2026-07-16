-- ═══ LESSON 4 ═══  You are not holding an object. You are holding a number.
--
-- uGuid is not a reference. It is not a pointer. It is not an object with methods on
-- it. It is an integer ID, and the engine will happily accept a completely bogus one
-- and do nothing with it.
--
-- Run this and look at what Pg.GetGuidByName gives back:
--
--     cargo run -p mercs2_script --example lua_lab -- \
--         crates/mercs2_script/examples/lessons/04_you_are_holding_a_number.lua


tEvents = {}

function OnActivate(uGuid, uRuntimeOwner, iArg)
	tEvents[uGuid] = Event.Create(
		Event.ObjectHibernation, {uGuid, "awake"}, SetupGuard, {uGuid})
end

function SetupGuard(uGuid)
	-- Find the tower this guard is supposed to stand on.
	local uTower = Pg.GetGuidByName("Outpost_Gaurd")     -- <— the typo is deliberate

	Object.SetPosition(uTower, 100.0, 0.0, 100.0)
	Ai.Goal(uTower, "defend")
end

function OnDeactivate(uGuid)
	-- Lesson 3's teardown, done properly — so the only bug left in this file is the guid.
	tEvents[uGuid] = Event.Delete(tEvents[uGuid])
end


-- ─── STATION 4 ────────────────────────────────────────────────────────────────
--
-- `Pg.GetGuidByName` on a name that doesn't exist returns **0**. Not nil. Not an
-- error. Zero. And zero is a perfectly acceptable guid to pass to every Object.* and
-- Ai.* call in the API — they will take it, do nothing, and return.
--
-- So a single typo in a string turns your whole script into an elaborate no-op, and
-- the only symptom is that the world doesn't change. `if not uTower then` won't save
-- you either — 0 is truthy in Lua. Only `if uTower == 0 then` catches it.
--
-- The same trap, one step later: guids go STALE. When the object dies, its guid keeps
-- its numeric value but stops referring to anything you can act on. If you stashed it
-- in a global at setup and a timer keeps poking it, every poke is silently dropped —
-- which is why `Object.IsAlive(uGuid)` shows up 139 times in the shipped scripts.
--
-- YOUR MOVE:
--
--   1. Fix the typo — "Outpost_Guard". Watch the calls start landing.
--
--   2. Now put the typo BACK, and make the script defend itself instead:
--
--          uTower = Pg.GetGuidByName("Outpost_Gaurd")
--          if uTower == 0 then
--              Debug.Printf("no tower by that name — check the spelling")
--              return
--          end
--
--      This is the habit worth building. Anywhere a guid enters your script from the
--      outside — a name lookup, a spawn, an event argument — it is a number that might
--      be junk, and you are the only one who is ever going to check.
--
--   3. Then go and read 05_the_shape.lua, which is what all four lessons add up to.
-- ──────────────────────────────────────────────────────────────────────────────
