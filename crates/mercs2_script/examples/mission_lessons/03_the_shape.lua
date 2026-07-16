-- ═══ MISSION LESSON 3 ═══  The shape of a contract.
--
--     cargo run -p mercs2_script --example mission_lab -- 03_the_shape
--
-- Clean verdict. Notice how little code this is, and in particular notice that there is NO teardown
-- code anywhere in this file. No Event.Delete. No cleanup loop. The framework does all of it, because
-- everything was created through `self`.
--
-- A real contract inherits `MrxTaskContract` rather than `MrxTask` — that adds the PDA entry, the
-- cash reward, the faction hit, the VO, the briefing. But the bones are exactly these, and
-- MrxTaskContract is itself just MrxTaskMission → MrxTask. Same rules, more furniture.

inherit("MrxTask")


function Activated(self)
	-- RULE 1: your parent, first, always. It's what puts you in the state machine, and everything
	-- else (Cleanup, the mission timer, the state callbacks) hangs off that.
	MrxTask.Activated(self)

	Debug.Printf("contract started")

	-- RULE 2: objectives are CHILDREN, not code. You describe them; the framework runs them, shows
	-- them in the PDA, and tears them down with you.
	--
	-- RULE 3: player-visible text is a [Token], resolved from the string table. A bare English
	-- sentence here would render as that literal sentence in every language the game ships.
	self:CreateChild({
		sName = "DestroyConvoy",
		sDspShortDesc = "[MyContract.Objectives.001]",
	})

	-- RULE 4: every event through `self`. Both variants exist; both get reclaimed.
	self:_CreatePersistentEvent(Event.TimerRelative, {1.0}, CheckConvoy, {self})
	self:_CreateEvent(Event.TimerRelative, {3.0}, Finish, {self})
end


function CheckConvoy(self)
	Debug.Printf("...convoy still moving")
end


function Finish(self)
	Debug.Printf("convoy destroyed")

	-- Complete() → Cleanup() → deletes every event in self._tEvents, MarkForRemoval's the layers in
	-- tConfig.tLayers, and cascades Cleanup into every child objective. Cancel() does the same.
	self:Complete()
end


-- ─── STATION 3 ────────────────────────────────────────────────────────────────
--
-- Break it and predict the report first:
--
--   * Delete the `MrxTask.Activated(self)` line.
--     → the task never goes Active... so why does the EVENT LEAK too? (Cleanup's first line is
--       `if not self:IsLatent()`. Follow it through.)
--
--   * Change `self:_CreatePersistentEvent` to `Event.CreatePersistent`.
--     → how many handlers outlive the mission, and what keeps printing after "mission ends"?
--
--   * Change `sDspShortDesc` to "Destroy the convoy".
--     → the lint fires. Ask yourself what a French player would see.
--
--   * Override `Cleanup(self)` and DON'T call `MrxTask.Cleanup(self)`.
--     → predict which of the framework's five teardown jobs stop happening. (Events, layers,
--       children, the parent's child-list entry, the timer.) This is lesson 1's bug wearing a
--       different hat, and it's why every real contract's Cleanup ends with a parent call.
--
-- WHEN YOU'RE READY FOR THE REAL THING: docs/mercs2-luacd/src/vz/pircon002.lua — "Devil's Due", the
-- one where you escort a truck full of parrots. It is this exact shape: `inherit("MrxTaskContract")`,
-- `MrxTaskContract.Activated(self)` on line one of Activated, `self:CreateChild{...}` objectives with
-- `[Token]` descriptions, and `self:_CreateEvent(Event.ObjectProximity, ...)`. You can read it now.
-- ──────────────────────────────────────────────────────────────────────────────
