-- ═══ MISSION LESSON 2 ═══  self:_CreateEvent, not Event.Create.
--
-- In the object-script lab you learned to keep every event handle and delete it yourself. Here you
-- will learn the opposite lesson: in a task, DON'T. The framework already does it — but only for
-- events it knows about, and it only knows about the ones you made through `self`.
--
-- This contract calls the parent correctly. It completes correctly. And it leaves a timer running in
-- the world forever.
--
--     cargo run -p mercs2_script --example mission_lab -- 02_the_event_that_outlived_the_mission
--
-- Watch the lines AFTER "mission ends". The mission is over. The task is completed. And something is
-- still talking.

inherit("MrxTask")


function Activated(self)
	MrxTask.Activated(self)

	Debug.Printf("contract started — escort the cargo")

	-- A raw Event.Create. It works! It fires! And the framework has never heard of it.
	Event.CreatePersistent(Event.TimerRelative, {1.0}, CheckCargo, {self})

	-- This one goes through self. Note what's different: nothing, except the `self:_` prefix.
	self:_CreateEvent(Event.TimerRelative, {3.0}, Finish, {self})
end


function CheckCargo(self)
	Debug.Printf("...checking the cargo is still intact")
end


function Finish(self)
	Debug.Printf("cargo delivered")
	self:Complete()
end


-- ─── STATION 2 ────────────────────────────────────────────────────────────────
--
-- Here is `_CreateEvent`, in full, from the real base class:
--
--     function _CreateEvent(self, nEventId, tEventArgs, fCallback, tCallbackArgs)
--         local uHandle = Event.Create(nEventId, tEventArgs, fCallback, tCallbackArgs)
--         table.insert(self._tEvents, uHandle)      ← the entire difference
--         return uHandle
--     end
--
-- That's it. It calls the same `Event.Create` you would have called, and then it writes the handle
-- into the task's own list. And here is the payoff, from `Cleanup`:
--
--     if self._tEvents then
--         for i, uHandle in pairs(self._tEvents) do
--             Event.Delete(uHandle)
--         end
--     end
--
-- So the framework will delete every event you created through `self` — when the mission completes,
-- when it's cancelled, when it's cleaned up, and recursively for every child objective. You write no
-- teardown code at all. That is the deal the task framework offers you.
--
-- Break the deal by calling `Event.Create` directly, and that handle is in nobody's list. It
-- survives `Complete()`. It survives the mission. It fires into a world where the cargo it's checking
-- was despawned twenty minutes ago — and if its callback touches a stale guid or a nil field, you get
-- a mystery error long after the mission that caused it is gone from your mind.
--
-- YOUR MOVE: change the raw one.
--
--     self:_CreatePersistentEvent(Event.TimerRelative, {1.0}, CheckCargo, {self})
--       ^^^^^ ^                       ^^^^^^^^^^^^^^^
--       |     |                       (there's a persistent variant too — same deal)
--       |     the framework's version
--       your task
--
-- Run it again: zero handlers survive, and the chatter stops when the mission does. You deleted
-- nothing by hand.
--
-- RULE OF THUMB: inside a task, if you're typing `Event.` you're probably making a mistake. Type
-- `self:_` instead. (The corpus agrees: 714 `_CreateEvent` calls vs 660 raw `Event.Create` — and most
-- of those raw ones are in object scripts, which have no `self` to hang them on.)
-- ──────────────────────────────────────────────────────────────────────────────
