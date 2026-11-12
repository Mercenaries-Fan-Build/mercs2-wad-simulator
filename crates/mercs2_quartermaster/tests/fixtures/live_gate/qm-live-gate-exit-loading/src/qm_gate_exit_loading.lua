-- Loaded at world.scripts_loaded, on the loading screen. Requests Exiting there, the way qm's error
-- policy does after a no-dialog failure.
print("[gate] qm_gate_exit_loading: requesting Exiting from the loading screen")
Sys.RequestGameState("Exiting")
print("[gate] qm_gate_exit_loading: RequestGameState returned")
