-- ═══ LESSON 2 ═══  Your object is ASLEEP when OnActivate runs.
--
-- This is the one that makes people scream at the computer.
--
-- The name is spelled right. The API calls are real. The arguments are correct. You
-- did exactly what the reference doc said. Run it:
--
--     cargo run -p mercs2_script --example lua_lab -- \
--         crates/mercs2_script/examples/lessons/02_asleep_at_the_wheel.lua
--
-- Every AI call comes back ✗ DROPPED. Nothing you wrote happened. The game would not
-- have told you this. The game would just have a guard standing there doing nothing,
-- forever, and you would have spent two hours re-reading your Ai.Goal arguments.


function OnActivate(uGuid)
	Debug.Printf("setting up guard " .. uGuid)

	-- The obvious code. The engine throws every line of it away.
	Ai.Goal(uGuid, "defend")
	Ai.SetState(uGuid, "alert", true)
end


-- ─── STATION 2 ────────────────────────────────────────────────────────────────
--
-- Here is the thing no reference doc ever says out loud:
--
--     OnActivate does not mean "your object is ready".
--     It means "your object's DATA has streamed in".
--
-- The object exists. It has a guid. It is not awake. It has no AI, no physics, no
-- animation — the engine has not finished bringing it to life, and it will not do so
-- on the same frame. Anything you push into it right now lands on the floor. Silently.
-- Ai.Goal even returns a boolean saying so, and nobody has ever checked it.
--
-- So OnActivate's real job is not to set your object up. It is to ASK TO BE TOLD when
-- the object is finally awake. That request is an event:
--
--     Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupGuard, {uGuid})
--       │            │                        │       │        │           │
--       │            │                        │       │        │           └─ args passed to it
--       │            │                        │       │        └───────────── call THIS when it happens
--       │            │                        │       └────────────────────── ...reaches this phase
--       │            │                        └────────────────────────────── when THIS object...
--       │            └─────────────────────────────────────────────────────── ...of this kind
--       └──────────────────────────────────────────────────────────────────── register a one-shot event
--
-- This is why every single object script in the shipped game opens with a line that
-- looks like that. It is not ceremony. It is not a style convention someone imposed.
-- It is the load-bearing wall, and now you know what it is holding up.
--
-- YOUR MOVE: restructure the above into the two-step shape.
--
--     function OnActivate(uGuid)
--         Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupGuard, {uGuid})
--     end
--
--     function SetupGuard(uGuid)
--         Debug.Printf("setting up guard " .. uGuid)
--         Ai.Goal(uGuid, "defend")
--         Ai.SetState(uGuid, "alert", true)
--     end
--
-- Run it. Watch the timeline: OnActivate fires at t=0.00 and does almost nothing;
-- the engine wakes the object at t=0.60; your setup runs THERE, and lands.
--
-- Note that SetupGuard is not a magic name. The engine has never heard of it. YOU
-- handed the engine that function, so the engine calls it back. That is the only
-- kind of function you actually control in this API.
-- ──────────────────────────────────────────────────────────────────────────────
