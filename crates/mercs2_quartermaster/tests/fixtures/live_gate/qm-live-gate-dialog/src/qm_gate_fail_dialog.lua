-- Loaded at world.scripts_loaded. Its world.begin handler fails on purpose.
local QM = _G._QM
print("[gate] qm_gate_fail_dialog: body ran")
QM.on("world.begin", function()
  error("deliberate failure, to show the error dialog")
end, "qm-live-gate-dialog")
