-- LAB STUB — not game code. Shadows the real `MrxGuiShellBootstrap` for `mission_lab` only.
--
-- Why this exists: `MrxUtil` (which `MrxTask` imports) has `import("MrxGuiShellBootstrap")` on line 3
-- and then never uses it. Harmless in the real game — but our module loader honours the Pandemic
-- convention of auto-running a module's `Init()`, and this module's `Init()` boots the entire SHELL
-- UI (`MrxGuiBase.LoadGUIFile("MrxGuiLoadLayout", ...)` → attract screen → load screen → …).
--
-- So merely importing the task framework drags in the front-end. That's faithful, but it is not what
-- the mission lab is teaching, and the shell's widget layer isn't modelled here. An inert stub keeps
-- the lab on the task framework.
--
-- VERIFIED SAFE FOR THESE LESSONS: nothing in the task chain (MrxTask, MrxTaskState, MrxTimer,
-- MrxGui, MrxLayerManager, MrxUtil) calls a single function on this module — MrxUtil's import is
-- dead. Grep it yourself:
--     grep -rn "MrxGuiShellBootstrap\." docs/mercs2-luacd/src/resident/
--
-- The real module is at docs/mercs2-luacd/src/resident/mrxguishellbootstrap.lua and is worth reading
-- if you ever want to touch the front end.

nPlayersSelected = 1
bNeedsReloading = false
_sSelectedCharacter = false

function GetSelectedCharacter()
	return _sSelectedCharacter
end

function SetSelectedCharacter(sCharacter)
	_sSelectedCharacter = sCharacter
end
