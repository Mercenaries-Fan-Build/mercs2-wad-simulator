-- Loaded at shell.boot, in the front end's VM. Logs that its body ran, then every call of the front
-- end's shell and menu hooks.
local QM = _G._QM
print("[gate] qm_gate_shell: body ran")
local fired = {}
local hooks = {
  "shell.open", "menu.open", "shell.close", "shell.sound_exit", "shell.play",
  "shell.lobby.enter", "shell.lobby.exit", "shell.friends_lobby.enter", "shell.friends_lobby.exit",
  "shell.online", "shell.join.lan", "shell.join.friends", "shell.join.quickmatch", "shell.join.optimatch",
}
for i = 1, #hooks do
  local id = hooks[i]
  QM.on(id, function(...)
    fired[id] = (fired[id] or 0) + 1
    print("[gate] handler " .. id .. ": call " .. fired[id] .. ", " .. select("#", ...) .. " argument(s)")
  end, "qm-live-gate")
end
