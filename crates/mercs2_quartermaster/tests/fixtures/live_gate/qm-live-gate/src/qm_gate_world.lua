-- Loaded at world.scripts_loaded. Logs that its body ran, then every call of the gameplay hooks the
-- gate generates, with how many arguments the hook passed and how many times it has fired in this VM.
local QM = _G._QM
print("[gate] qm_gate_world: body ran")
local fired = {}
local hooks = { "world.load", "world.begin", "world.retry", "world.sound_exit", "save.complete", "hq.enter", "cash.changed" }
for i = 1, #hooks do
  local id = hooks[i]
  QM.on(id, function(...)
    fired[id] = (fired[id] or 0) + 1
    print("[gate] handler " .. id .. ": call " .. fired[id] .. ", " .. select("#", ...) .. " argument(s)")
  end, "qm-live-gate")
end
