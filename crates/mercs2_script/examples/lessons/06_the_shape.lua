-- ═══ LESSON 6 ═══  The shape.
--
-- This is a correct object script. Run it: no dropped calls, no leaked handlers, both objects
-- served on their own terms, clean verdict.
--
--     cargo run -p mercs2_script --example lua_lab -- \
--         crates/mercs2_script/examples/lessons/06_the_shape.lua
--
-- Every real object script in the shipped game has this silhouette. It looked like arbitrary
-- boilerplate before you knew what each line was defending against. Read it again and name the
-- bug each line prevents:
--
--     the hook names are exact                   ← the engine dispatches by string (lesson 1)
--     OnActivate does nothing but arm a gate     ← the object is asleep (lesson 2)
--     handles are kept, keyed by guid            ← you'll need to delete them (lesson 3)
--     guids are checked before use               ← guids are just numbers (lesson 4)
--     identity travels in the args table         ← one script, many objects (lesson 5)
--     OnDeactivate mirrors OnActivate exactly    ← streaming runs it all again (lesson 3)


-- A global that is a TABLE KEYED BY GUID is fine — that's one slot per object.
-- A global holding one object's value is the lesson-5 bug.
tEvents = {}


function OnActivate(uGuid, uRuntimeOwner, iArg)
	tEvents[uGuid] = {}

	-- OnActivate's ONLY job is to ask to be woken up. The object is asleep right now;
	-- anything else you tried here would be silently dropped.
	--
	-- The 4th argument is how this object's identity — and its variant — survive the wait.
	table.insert(tEvents[uGuid], Event.Create(
		Event.ObjectHibernation, {uGuid, "awake"}, SetupGuard, {uGuid, iArg}))
end


function SetupGuard(uGuid, iArg)
	-- The object is real now. Calls land here. But it may have died while we waited,
	-- so the guid still has to earn its keep.
	if not Object.IsAlive(uGuid) then
		return
	end

	-- iArg is the placement variant the level editor baked in. One script, many behaviours.
	local sGoal = "defend"
	if iArg >= 3 then
		sGoal = "snipe"
	end

	Debug.Printf("guard " .. uGuid .. " on post (iArg=" .. iArg .. ", goal=" .. sGoal .. ")")
	Ai.Goal(uGuid, sGoal)
	Ai.SetState(uGuid, "alert", true)

	-- Every event you create, you keep the handle for — in THIS object's slot.
	table.insert(tEvents[uGuid], Event.CreatePersistent(
		Event.TimerRelative, {1.5}, Shout, {uGuid}))
end


function Shout(uGuid)
	-- A guid you stored earlier may be stale by the time the timer fires.
	if not Object.IsAlive(uGuid) then
		return
	end
	Debug.Printf("guard " .. uGuid .. ": area secure")
end


function OnDeath(uGuid)
	Debug.Printf("guard " .. uGuid .. " is down")
	-- The object is dead but the block is still loaded — the events are still armed and still
	-- firing. Death is not teardown. OnDeactivate is teardown.
end


function OnDeactivate(uGuid)
	-- Tear down EXACTLY what you set up, for THIS object. Runs every time it streams out;
	-- OnActivate runs again every time it streams back in.
	for _, e in ipairs(tEvents[uGuid] or {}) do
		Event.Delete(e)
	end
	tEvents[uGuid] = nil
end


-- ─── STATION 6 ────────────────────────────────────────────────────────────────
--
-- Now break it on purpose, one line at a time, and predict the report BEFORE you run:
--
--   * Comment out the `Event.Delete` loop in OnDeactivate.
--     → how many handlers leak? does the shout rate climb?
--
--   * Move `Ai.Goal(uGuid, sGoal)` from SetupGuard up into OnActivate.
--     → does it land, or drop? why?
--
--   * Change `tEvents[uGuid]` to a plain global `eHandle`.
--     → which object gets starved, and which gets served twice?
--
--   * Replace the `{uGuid, iArg}` args table with `{}`, and read a global instead.
--     → same question. This is lesson 5 with a different disguise.
--
--   * Delete the `Object.IsAlive` guard in Shout.
--     → which timeline segment starts producing ✗ DROPPED lines?
--
--   * Rename SetupGuard to setupGuard (but leave Event.Create pointing at SetupGuard).
--     → this one is a real Lua error, not silence. Why is THIS one loud when lesson 1's rename
--       was silent? Because here YOU are the caller: you passed a nil. The engine's dispatch is
--       a name lookup that can quietly miss. That distinction IS the mental model.
--
-- When you can predict all six before hitting enter, you have it.
--
-- Go read a real one: docs/mercs2-luacd/src/resident/soldier.lua — infantry that drop ammo when
-- they die. It opens with `local tEvents = tEvents or {}` and an `OnDeath(uGuid)`, and by now
-- you know exactly why both of those are there.
--
-- Then: object scripts are only HALF the API. Missions and contracts are a different world —
-- objects with `self`, `inherit("MrxTaskContract")`, and a lifecycle of their own. That's the
-- mission_lab, next door.
-- ──────────────────────────────────────────────────────────────────────────────
