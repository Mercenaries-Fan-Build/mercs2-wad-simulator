//! **This binary could not be linked before.**
//!
//! It contains both halves of the workspace's Lua at once:
//!
//! * the **runtime** — `mercs2_engine` → `mercs2_script` → the VM the game's scripts execute on;
//! * the **compiler and linker** — `mercs2_quartermaster` → `mercs2_luac`, which compiles Lua source
//!   into the `scripts_vz` bytecode a Shipment ships.
//!
//! Until the host moved off `mlua`, those were two different Lua libraries — mlua's vendored 5.4 and
//! our patched 5.1 — both exporting unprefixed `lua_newstate` / `lua_pcall` / `lua_close`. The
//! linker picked one definition per symbol, so a `lua_State` allocated by one implementation got
//! parsed by the other. The failure was a **SIGSEGV partway through**, not a link error, which is
//! what made it expensive to diagnose (see the module note in `mercs2_luac/tests/parity.rs`).
//!
//! That is why the Workshop could not link the Quartermaster, and why this file exists: it is the
//! regression guard. If a second Lua ever re-enters the dependency graph, this test does not fail
//! politely — it crashes the process, which is the loudest signal available and better than the
//! silent corruption it replaces.

use mercs2_script::ScriptHost;

/// Runtime → compiler → runtime, in one process.
///
/// The ordering is the point. Using the compiler *between* two runtime operations is precisely the
/// interleaving that used to corrupt state: the second runtime call would run on a `lua_State` the
/// other implementation had touched.
#[test]
fn the_runtime_and_the_compiler_coexist_in_one_process() {
    // 1. Runtime.
    let sh = ScriptHost::bare().expect("script host");
    let before: f32 = sh.eval("return 6 * 7").expect("runtime before");
    assert_eq!(before, 42.0);

    // 2. Compiler — the path `mercs2_quartermaster` drives when it links a Shipment's Lua.
    let bytecode = mercs2_luac::compile(
        "table.insert(_tOutfits.mattias, {Name = \"Mechanic\"})",
        "wifpmcinterior",
    )
    .expect("compile");
    assert_eq!(&bytecode[..4], b"\x1bLua");
    assert_eq!(bytecode[10], 4, "float lua_Number — the game's dialect");

    // 3. Runtime again, on the SAME state. This is the call that used to die.
    let after: f32 = sh.eval("return 6 * 7").expect("runtime after the compiler ran");
    assert_eq!(after, 42.0);

    // And the state is still coherent, not merely non-crashing.
    sh.exec("_probe = {}\nfor i = 1, 100 do _probe[i] = tostring(i) end", "@probe")
        .expect("exec after compiling");
    let n: f32 = sh.eval("return table.getn(_probe)").expect("table intact");
    assert_eq!(n, 100.0);
}

/// The Quartermaster is reachable and usable from a binary that also embeds the runtime — the
/// concrete capability the Workshop needs in order to build a Shipment in-process.
#[test]
fn quartermaster_is_usable_alongside_the_runtime() {
    let sh = ScriptHost::bare().expect("script host");

    // A Shipment manifest, parsed by the Quartermaster's own model.
    let manifest_src = r#"
format = 1

[shipment]
name = "one-lua-probe"
version = "1.0.0"
target = "retail"
"#;
    let manifest = mercs2_quartermaster::from_str(manifest_src, mercs2_quartermaster::Format::Toml)
        .expect("parse manifest");
    manifest.validate().expect("valid manifest");
    assert_eq!(manifest.shipment.name, "one-lua-probe");

    // The Lua the Quartermaster would append for a wardrobe outfit, compiled with the same
    // compiler — then the runtime is used again to prove nothing was disturbed.
    let row = mercs2_quartermaster::link::outfit_row_append(
        "mattias",
        "mechanic",
        "pmc_hum_mechanic",
        "Mechanic",
    );
    let compiled = mercs2_luac::compile(&row, "wifpmcinterior").expect("compile the outfit row");
    assert!(compiled.len() > 12);

    let still_alive: bool = sh.eval("return type(unpack) == 'function'").expect("runtime alive");
    assert!(still_alive, "5.1 natives are present and the VM is healthy");
}

/// The two Lua surfaces agree on the dialect, because they are the same library.
///
/// Not a tautology worth skipping: it is the property that lets the Quartermaster compile a chunk
/// the engine can then load, which is what a Shipment's `scripts_vz` block depends on end to end.
#[test]
fn the_compiler_emits_what_the_runtime_loads() {
    let sh = ScriptHost::bare().expect("script host");
    let bytecode = mercs2_luac::compile("return 1 + 1", "probe").expect("compile");

    // Hand the compiler's own output to the runtime and run it.
    let two: f32 = sh
        .lua()
        .load(&bytecode)
        .eval()
        .expect("the runtime must load the compiler's bytecode");
    assert_eq!(two, 2.0);
}
