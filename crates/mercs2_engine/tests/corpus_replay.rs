//! Corpus replay — execute **real shipped game scripts** against the **real** `GameScriptHost`.
//!
//! ```sh
//! cargo test -p mercs2_engine --test corpus_replay
//! ```
//!
//! # Why this exists, and why it is here rather than in `mercs2_script`
//!
//! Binding coverage counts prove nothing about behaviour: the `Player` namespace read 107/107 `real`
//! while ~40 bodies returned hardcoded placeholders and 24 more pushed into a Vec nothing drained. What
//! *does* prove something is running the game's own Lua and asserting on engine-observable state.
//!
//! It lives in `mercs2_engine` because the host that matters is `GameScriptHost`. `mercs2_script`'s
//! `tests/game_hooks.rs` runs against a hand-rolled `HarnessHost` that overrides a dozen methods — a
//! replay against that proves things about the fake, not about the engine the game runs on.
//!
//! # Four rules these assertions follow
//!
//! 1. **Never assert "no error".** A module whose every cfunc returns nil raises nothing.
//! 2. **Assert engine-observable state through the host**, not through Lua reading back its own writes.
//! 3. **Assert round trips through a callback** where the contract is a protocol.
//! 4. **Assert arity and shape at the boundary**, so a failure names the cfunc rather than the symptom.
//!
//! The corpus is vendored (`mercs2_script::corpus`), so these run with no configuration.

use std::cell::RefCell;
use std::rc::Rc;

use mercs2_engine::script_host::GameScriptHost;
use mercs2_script::ScriptHost;

/// Build a script host over the vendored corpus, backed by the real `GameScriptHost`.
///
/// Returns `None` (with a printed skip line) when the corpus is absent — a consumer who stripped it
/// should still get a green `cargo test`.

/// Bind a handle to a Lua global as lightuserdata — the way the engine really hands one to a script.
///
/// These are roster guids, minted from `mercs2_core::FIRST_DYNAMIC_GUID` (2^28) upward. They cannot
/// be interpolated into Lua source as numbers: this VM's `lua_Number` is f32, whose steps at 2^28
/// are 32 wide, so the script would receive a *different* handle. `mercs2_script::Guid` refuses to
/// read one out of a number for exactly that reason.
fn set_guid(sh: &ScriptHost, name: &str, g: u64) {
    sh.lua().globals().set(name, mercs2_script::Guid(g)).unwrap();
}

fn replay_host() -> Option<(ScriptHost, Rc<RefCell<GameScriptHost>>)> {
    let roots = mercs2_script::corpus::roots();
    if roots.is_empty() {
        eprintln!("{}", mercs2_script::corpus::skip_notice("corpus-replay"));
        return None;
    }
    let host = Rc::new(RefCell::new(GameScriptHost::new("vz")));
    let sh = ScriptHost::new(roots).expect("lua");
    sh.register_engine(host.clone()).expect("bindings");
    // Bring-up stubs for engine surfaces these modules touch but this replay does not model. Without
    // it an unrelated missing global aborts the import cascade before the module under test loads.
    let stubs: Rc<RefCell<std::collections::BTreeSet<String>>> = Default::default();
    sh.enable_autostub(stubs).expect("autostub");
    Some((sh, host))
}

/// **Target 1 — `resident/mrxpmc.lua`, the economy funnel.**
///
/// Every cash and fuel transaction in the game goes through `MrxPmc`. This asserts the two independent
/// Lua-side clamps *and* that the values land on the real profile singleton — which is the half a
/// coverage count cannot see.
///
/// It also pins the boundary the code map insisted on: the 1-billion cap is **Lua**, not native, so a
/// direct `Player.SetCash` walks straight past it.
#[test]
fn mrxpmc_economy_clamps_and_lands_on_the_profile() {
    let Some((sh, host)) = replay_host() else { return };
    sh.exec("import(\"MrxPmc\")", "@replay").expect("MrxPmc imports");

    // `AddCashQty` clamps the per-delta amount to ±1e9 AND the running total to [0, 1e9] — two
    // separate clamps, and a single-clamp implementation passes only one half of this.
    sh.exec("MrxPmc.AddCashQty(2000000000, false, \"replay\", true)", "@replay").unwrap();
    assert_eq!(
        host.borrow().player().profile.cash,
        1_000_000_000,
        "the per-delta clamp caps the add at 1e9"
    );

    sh.exec("MrxPmc.AddCashQty(-2000000000, false, \"replay\", true)", "@replay").unwrap();
    assert_eq!(host.borrow().player().profile.cash, 0, "the total clamps at zero, not negative");

    // ...and the cap is Lua-side only. `Player.SetCash` is the path `mrxpmc.lua:474,538` itself uses to
    // bypass it, so a native clamp here would make those bypasses unobservable.
    sh.exec("Player.SetCash(2000000000)", "@replay").unwrap();
    assert_eq!(
        host.borrow().player().profile.cash,
        2_000_000_000,
        "SetCash bypasses MrxPmc's cap — the clamp is NOT native"
    );

    // The shipped autosave bug, observable end to end: cash changed, profile not armed to save.
    assert!(
        !host.borrow().player().profile.autosave_due(),
        "SetCash never ORs the dirty flag +0x11 — retail's bug, reproduced"
    );
}

/// **Target 1b — `MrxPmc.SetFuelCapacity`'s validator, and where capacity actually lives.**
///
/// A boolean-returning gate on `[300, 9999]` that a cheat flag bypasses. Asserting the *return value*
/// matters: the shipped callers branch on it.
///
/// **Capacity is Lua-side during play.** `MrxPmc.SetFuelCapacity` writes only its own `_nFuelCapacity`
/// (`mrxpmc.lua:113`) — it never calls `Player.SetFuelCapacity`. The profile field is written once, in
/// `SaveSingleton` (`:503 Player.SetFuelCapacity(GetFuelCapacity())`), and read back in
/// `LoadSingleton` (`:529`). So the engine's `profile.fuel_capacity` is a *save slot*, not the live
/// value, and a reimpl that treats it as authoritative during play is wrong about the ownership.
#[test]
fn mrxpmc_fuel_capacity_is_lua_side_until_the_save_syncs_it() {
    let Some((sh, host)) = replay_host() else { return };
    sh.exec("import(\"MrxPmc\")", "@replay").unwrap();

    let too_small: bool = sh.eval("return MrxPmc.SetFuelCapacity(100)").unwrap();
    assert!(!too_small, "below the 300 floor is rejected, and says so");

    let ok: bool = sh.eval("return MrxPmc.SetFuelCapacity(5000)").unwrap();
    assert!(ok);
    let live: i64 = sh.eval("return MrxPmc.GetFuelCapacity()").unwrap();
    assert_eq!(live, 5000, "the live value is MrxPmc's own");
    assert_eq!(
        host.borrow().player().profile.fuel_capacity,
        0,
        "and it has NOT reached the profile — that only happens at save time"
    );

    // The cheat flag bypasses the range check entirely.
    let cheat: bool = sh.eval("return MrxPmc.SetFuelCapacity(1, true)").unwrap();
    assert!(cheat, "bCheat bypasses the validator");

    // The save-time sync is the one path that writes the profile field.
    sh.exec("MrxPmc.SetFuelCapacity(4200)", "@replay").unwrap();
    sh.exec("Player.SetFuelCapacity(MrxPmc.GetFuelCapacity())", "@replay").unwrap();
    assert_eq!(host.borrow().player().profile.fuel_capacity, 4200, "SaveSingleton's sync");

    // ...and writing it does not arm the autosave: `SetFuelCapacity` is another of the five setters
    // that never OR the dirty flag.
    assert!(!host.borrow().player().profile.autosave_due());

    // Fuel is not natively clamped to capacity either — `mrxpmc.lua:89-90` owns that relationship.
    sh.exec("Player.SetFuel(9999)", "@replay").unwrap();
    assert_eq!(host.borrow().player().profile.fuel, 9999, "no native clamp");
}

/// **Target 2 — `vz/wiftutorialvehicledisguise.lua`, the named-table disguise protocol.**
///
/// This is the script that proves three separate defects are gone at once: the named-table argument
/// shape, a callback that survives registration, and `GetVehicleDisguise` returning a real boolean.
///
/// Its guard is `if not Player.GetVehicleDisguise() then return end` (`:26`), so the whole handler is
/// dead unless the global gate reads back as truthy.
#[test]
fn disguise_protocol_parses_named_tables_and_retains_its_callback() {
    let Some((sh, host)) = replay_host() else { return };

    // The global gate is what the script's own guard tests.
    sh.exec("Player.SetVehicleDisguise(true)", "@replay").unwrap();
    let gate: bool = sh.eval("return Player.GetVehicleDisguise()").unwrap();
    assert!(gate, "the gate must read back as a real boolean, not nil");

    // The named-table shape, with the `Player =` key holding a CHARACTER guid. Passing a player handle
    // here is the silent-failure case the code map warns about.
    let ch = host.borrow().player().roster.local().map(|p| p.character).unwrap_or(0);
    set_guid(&sh, "uCh", ch);
    assert_ne!(ch, 0, "the boot roster possesses the hero");

    sh.exec(
        &format!(
            "_replay_fired = false\n\
             Player.VehicleDisguise({{Player = uCh, Callback = function() _replay_fired = true end}})"
        ),
        "@replay",
    )
    .expect("the named-table form must parse");

    let state: bool =
        sh.eval(&format!("return Player.GetVehicleDisguiseState({{Player = uCh}})")).unwrap();
    assert!(state, "the setter is observable by the getter, as a boolean");

    // `tostring()` on the state is what the shipped script compares against "true"/"false" — pushing an
    // integer here stringifies to "0" and kills both branches at :37/:41.
    let s: String =
        sh.eval(&format!("return tostring(Player.GetVehicleDisguiseState({{Player = uCh}}))")).unwrap();
    assert_eq!(s, "true", "the script compares tostring(...) against \"true\"");

    // `Remove = true` is the teardown form.
    sh.exec(&format!("Player.VehicleDisguise({{Player = uCh, Remove = true}})"), "@replay").unwrap();
    let after: bool =
        sh.eval(&format!("return Player.GetVehicleDisguiseState({{Player = uCh}})")).unwrap();
    assert!(!after, "Remove clears the per-player disguise");

    // The per-player state and the global gate are independent mechanisms.
    let gate: bool = sh.eval("return Player.GetVehicleDisguise()").unwrap();
    assert!(gate, "clearing one player's disguise must not flip the global gate");
}

/// **Target 3 — `resident/mrxsupportdesignatorsatellite.lua`, the PDA arity + callback protocol.**
///
/// The nine-argument engage and the two-argument teardown, plus the exit/cancel callback split.
///
/// ⚠ The *script* cannot run end to end yet: its first line is
/// `if "userdata" ~= type(self.uOwner) then return end` and our GUIDs are Lua integers. That is a
/// separate, workspace-wide change (114 corpus sites type-check on it), so this asserts the **binding
/// contract** the script depends on rather than the script's own completion.
#[test]
fn pda_map_mode_takes_nine_arguments_and_splits_exit_from_cancel() {
    let Some((sh, host)) = replay_host() else { return };
    let owner = host.borrow().player().roster.local().map(|p| p.guid).unwrap_or(0);
    set_guid(&sh, "uOwner", owner);
    assert_ne!(owner, 0);

    // The engage form, verbatim in shape from `mrxsupportdesignatorsatellite.lua:77`.
    sh.exec(
        &format!("Player.SetPDAMapMode(uOwner, true, 10, 25, -5, 120, 3, 7, true)"),
        "@replay",
    )
    .expect("the 9-argument engage must parse");

    {
        let h = host.borrow();
        let pda = h.player().roster.local().unwrap().pda_map;
        assert!(pda.active);
        assert_eq!(pda.centre, [10.0, 25.0, -5.0], "args 3-5 land, not just the mode flag");
        assert_eq!(pda.radius, 120.0, "arg 6");
        assert_eq!((pda.zoom_below, pda.zoom_above), (3.0, 7.0), "args 7-8");
        assert!(pda.minigame, "arg 9");
    }

    // Exit and cancel are DIFFERENT callbacks — a script registers separate handlers and must not
    // receive the wrong one.
    sh.exec(
        &format!(
            "_replay_end = false _replay_cancel = false\n\
             Player.SetPDAMapModeCallback(uOwner, true, function() _replay_end = true end, {{}})\n\
             Player.SetPDAMapModeCancelCallback(uOwner, function() _replay_cancel = true end, {{}})\n\
             Player.RequestPDAMapModeCancel(uOwner)"
        ),
        "@replay",
    )
    .unwrap();

    // The fire is queued, then dispatched by the pump — never re-entered from inside the binding.
    mercs2_engine::script_host::pump_resident(&sh, &host, 1.0 / 60.0);
    let (ended, cancelled): (bool, bool) = sh.eval("return _replay_end, _replay_cancel").unwrap();
    assert!(cancelled, "the cancel callback fired");
    assert!(!ended, "and the exit callback did NOT — they are distinct registrations");

    // The two-argument teardown form.
    sh.exec(&format!("Player.SetPDAMapMode(uOwner, false)"), "@replay")
        .expect("the 2-argument teardown must parse");
    assert!(!host.borrow().player().roster.local().unwrap().pda_map.active);
}

/// **The regression test for the inverted mode gates.**
///
/// `mrxutil.lua:975` calls `Player.SetCinematicMode(uPlayer, false)`. Every gate used to read argument
/// 1 — the player handle — as its boolean, and `mlua`'s Lua-truthiness conversion turned that into
/// `true`. So passing `false` *set* the gate.
#[test]
fn mode_gates_read_argument_two_not_the_handle() {
    let Some((sh, host)) = replay_host() else { return };

    sh.exec(
        "local p = Player.GetLocalPlayer() \
         Player.SetCinematicMode(p, false) \
         Player.SetInputEnabled(p, false)",
        "@replay",
    )
    .unwrap();

    let h = host.borrow();
    let p = h.player().roster.local().unwrap();
    assert!(!p.in_cinematic_mode(), "SetCinematicMode(p, false) must NOT enable cinematic mode");
    assert!(!p.input_enabled, "SetInputEnabled(p, false) must disable input");
}

/// **Sweep — every corpus module imports.**
///
/// `#[ignore]`d because it is slow and because its failure list is a *work queue*, not a gate: the
/// modules that fail are the ones whose engine surface is still missing.
///
/// ```sh
/// cargo test -p mercs2_engine --test corpus_replay -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn every_corpus_module_imports() {
    let Some((sh, _host)) = replay_host() else { return };
    let Some(root) = mercs2_script::corpus::root() else { return };

    let mut ok = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();
    for dir in ["resident", "vz", "shell"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("lua") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            match sh.exec(&format!("import(\"{stem}\")"), "@sweep") {
                Ok(()) => ok += 1,
                Err(err) => {
                    let msg = err.to_string();
                    let first = msg.lines().next().unwrap_or("").to_string();
                    failed.push((stem.to_string(), first));
                }
            }
        }
    }

    println!("\n[corpus-sweep] {ok} imported, {} failed", failed.len());
    for (m, why) in failed.iter().take(40) {
        println!("  {m:<44} {why}");
    }
    if failed.len() > 40 {
        println!("  … and {} more", failed.len() - 40);
    }
    assert!(ok > 0, "no module imported at all — the corpus or the loader is broken");
}
