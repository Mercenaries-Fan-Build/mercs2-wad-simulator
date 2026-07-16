-- ═══ LESSON 3 ═══  It works. Then you drive away, come back, and it does everything twice.
--
-- This script is correct. Lesson 1's name is right. Lesson 2's awake-gate is there.
-- Run it and the first half of the timeline is clean — the guard sets up, the guard
-- shouts on a timer, everything lands.
--
--     cargo run -p mercs2_script --example lua_lab -- \
--         crates/mercs2_script/examples/lessons/03_the_double_fire.lua
--
-- Then keep reading the timeline past t=3.5, where the player drives away and comes
-- back. Look at the shout. Look at the leaked-handler count in the report.


function OnActivate(uGuid)
	Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupGuard, {uGuid})
end

function SetupGuard(uGuid)
	Ai.Goal(uGuid, "defend")

	-- Shout every second, forever.
	Event.CreatePersistent(Event.TimerRelative, {1.0}, Shout, {uGuid})
end

function Shout(uGuid)
	Debug.Printf("guard " .. uGuid .. ": area secure")
end


-- ─── STATION 3 ────────────────────────────────────────────────────────────────
--
-- The world is not a place your object is born into once and lives in forever. It
-- streams. Walk 300m away and your object hibernates and its block unloads. Walk back
-- and it ALL HAPPENS AGAIN: OnActivate, awake, SetupGuard. Same guid. Second time.
--
-- Which means SetupGuard registered a second shout timer. The first one never went
-- anywhere — you never told anyone to throw it away. Now the guard shouts twice a
-- second. Drive away and back again: three times. This is the bug that shows up as
-- "the VO line plays twice", "the pickup spawns twice", "the trigger fires twice",
-- and it is impossible to reason about if you think of your script as running once.
--
-- Every Event.Create hands you back a HANDLE. It is a number. It is the only proof
-- the event exists, and the only way to get rid of it:
--
--     e = Event.Create(...)     -- e is now a handle
--     e = Event.Delete(e)       -- Delete returns nil, so this also clears your variable
--
-- That `e = Event.Delete(e)` idiom is everywhere in the shipped scripts for exactly
-- that reason: it kills the event and blanks the variable in one line, so you can't
-- accidentally double-delete a stale handle.
--
-- And the place you do it is OnDeactivate — the hook you have been ignoring, whose
-- entire purpose is "your object is leaving; put your toys away."
--
--     OnActivate    →  set things up
--     OnDeactivate  →  tear down EXACTLY what OnActivate set up
--
-- If those two are not mirror images of each other, you have this bug.
--
-- YOUR MOVE:
--
--   1. Keep the handles. A table keyed by guid is the standard shape, because one
--      script serves many objects:
--
--          tEvents = {}
--
--          function SetupGuard(uGuid)
--              Ai.Goal(uGuid, "defend")
--              tEvents[uGuid] = Event.CreatePersistent(
--                  Event.TimerRelative, {1.0}, Shout, {uGuid})
--          end
--
--   2. Give the awake-gate event the same treatment — it is an event too, and on a
--      second stream-in you will register a second one.
--
--   3. Write OnDeactivate and Delete them all:
--
--          function OnDeactivate(uGuid)
--              tEvents[uGuid] = Event.Delete(tEvents[uGuid])
--          end
--
-- Run it again. The report's leaked-handler count should go to zero, and the guard
-- should shout at a sane rate for the whole timeline.
-- ──────────────────────────────────────────────────────────────────────────────
