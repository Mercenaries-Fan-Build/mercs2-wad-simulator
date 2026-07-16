-- ═══ MISSION LESSON 1 ═══  You are inheriting from something that does real work.
--
-- Missions are not object scripts. Here you're writing a CLASS. `inherit("MrxTask")` puts a real,
-- shipped base class underneath you, and that base class runs the state machine, owns the event
-- book-keeping, and drives Cleanup.
--
-- The catch: Lua has no `super`. When you define `Activated`, you don't EXTEND the parent's
-- `Activated` — you REPLACE it. Whatever it was going to do, it now doesn't do.
--
-- Run it:
--
--     cargo run -p mercs2_script --example mission_lab -- 01_you_forgot_the_parent
--
-- The contract starts. The timer fires. It even reports as completed. And yet the report says the
-- task NEVER WENT ACTIVE, and its event outlived the mission. Read on.

inherit("MrxTask")


function Activated(self)
	Debug.Printf("contract started — go blow up the convoy")

	-- Both of these go through `self`, correctly. Watch them survive the mission anyway.
	self:_CreatePersistentEvent(Event.TimerRelative, {1}, Nag, {self})
	self:_CreateEvent(Event.TimerRelative, {2}, Finish, {self})
end


function Nag(self)
	Debug.Printf("...the convoy is getting away")
end


function Finish(self)
	Debug.Printf("convoy destroyed")
	self:Complete()
end


-- ─── STATION 1 ────────────────────────────────────────────────────────────────
--
-- `MrxTask.Activated(self)` is the line you didn't write. Here is what it does, from the real
-- source (docs/mercs2-luacd/src/resident/mrxtask.lua):
--
--     function Activated(self)
--         ...
--         self:_SetState(MrxTaskState._knActive)     ← puts the task in the state machine
--         self:_IssueStateChangeCallbacks()          ← fires tOnActivated hooks
--         ... starts the mission timer from tConfig.nTimeLimit ...
--     end
--
-- Skip it and your task stays LATENT forever. Which sounds abstract until you follow it through:
--
--     Cleanup() starts with `if not self:IsLatent() ...`
--
-- Your task IS latent, so Cleanup does nothing. It doesn't delete your events. It doesn't remove
-- your layers. It doesn't clean up your children. The framework has decided your task never really
-- started, so there is nothing to tear down. That's why the report shows a leaked handler even
-- though you did everything else right: the leak is a SYMPTOM of the missing parent call.
--
-- This is also why the mission timer (`nTimeLimit`) "doesn't work" for people. It's started by the
-- parent Activated. No parent call, no timer.
--
-- YOUR MOVE: call your parent, FIRST, before anything else:
--
--     function Activated(self)
--         MrxTask.Activated(self)          -- <— always the first line
--         Debug.Printf("contract started — go blow up the convoy")
--         self:_CreateEvent(Event.TimerRelative, {2}, Finish, {self})
--     end
--
-- Note it's `MrxTask.Activated(self)` — a DOT, and you pass `self` by hand. `self:Activated()` would
-- call your own Activated again and loop forever. This is what "there is no super in Lua" costs you.
--
-- The same rule applies to every lifecycle method you override: `Complete`, `Cancel`, `Cleanup`,
-- `LoadAssets`. Real contracts call their parent in all of them — go look at any `*con*.lua` in
-- docs/mercs2-luacd/src/vz/ and you'll see `MrxTaskContract.Activated(self)` as line one.
-- ──────────────────────────────────────────────────────────────────────────────
