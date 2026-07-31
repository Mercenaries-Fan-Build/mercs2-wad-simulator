//! Tests for the safe layer.
//!
//! Weighted toward what fails *silently*: the `longjmp`/panic boundary, stack balance, ref
//! lifetimes, and the f32 numeric model. A binding layer that leaks a stack slot per call works
//! fine in a unit test and dies after an hour of gameplay.

use super::*;
use crate::sys;

/// Stack depth, for the balance assertions. A layer that returns the right answer while growing
/// the stack every call is broken in the way that takes longest to find.
fn top(lua: &Lua) -> i32 {
    unsafe { sys::lua_gettop(lua.state()) }
}

/// The balance guard must actually fire. Every entry point carries one, and 38 passing tests
/// prove nothing if the assertion can't trip — so unbalance the stack deliberately and check.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "left the Lua stack")]
fn the_balance_guard_detects_a_leaked_slot() {
    let lua = Lua::new().unwrap();
    let _balance = Balance::new(&lua, "deliberate leak");
    // SAFETY: pushing one value; the guard is expected to catch that we never pop it.
    unsafe {
        lua.reserve(1);
        sys::lua_pushnil(lua.state());
    }
}

#[test]
fn evaluates_and_returns_a_number() {
    let lua = Lua::new().unwrap();
    let n: f32 = lua.load("return 6 * 7").eval().unwrap();
    assert_eq!(n, 42.0);
    assert_eq!(top(&lua), 0, "stack must be balanced");
}

#[test]
fn globals_round_trip() {
    let lua = Lua::new().unwrap();
    lua.globals().set("greeting", "hola").unwrap();
    let s: String = lua.load("return greeting").eval().unwrap();
    assert_eq!(s, "hola");
    let via_get: String = lua.globals().get("greeting").unwrap();
    assert_eq!(via_get, "hola");
    assert_eq!(top(&lua), 0);
}

#[test]
fn rust_function_is_callable_from_lua() {
    let lua = Lua::new().unwrap();
    let f = lua.create_function(|_, (a, b): (f32, f32)| Ok(a + b)).unwrap();
    lua.globals().set("add", f).unwrap();
    let n: f32 = lua.load("return add(2, 3)").eval().unwrap();
    assert_eq!(n, 5.0);
    assert_eq!(top(&lua), 0);
}

/// Trailing arguments the caller omitted must arrive as `None`, because the game's Lua calls
/// engine functions with optional tails constantly.
#[test]
fn missing_trailing_arguments_decode_as_none() {
    let lua = Lua::new().unwrap();
    let f = lua
        .create_function(|_, (a, b): (String, Option<String>)| {
            Ok(format!("{a}/{}", b.unwrap_or_else(|| "default".into())))
        })
        .unwrap();
    lua.globals().set("j", f).unwrap();
    assert_eq!(
        lua.load("return j('x')").eval::<String>().unwrap(),
        "x/default"
    );
    assert_eq!(
        lua.load("return j('x', 'y')").eval::<String>().unwrap(),
        "x/y"
    );
}

/// A binding returning `Err` must surface as a catchable Lua error, not a process abort.
#[test]
fn binding_error_becomes_a_lua_error() {
    let lua = Lua::new().unwrap();
    let f = lua
        .create_function(|_, ()| -> Result<()> { Err(Error::RuntimeError("nope".into())) })
        .unwrap();
    lua.globals().set("boom", f).unwrap();

    let (ok, msg): (bool, String) = lua.load("return pcall(boom)").call(()).unwrap();
    assert!(!ok, "pcall must report failure");
    assert!(msg.contains("nope"), "message must survive: {msg}");
    assert_eq!(top(&lua), 0);
}

/// The hazard the trampoline exists for: a Rust `panic!` must be converted at the boundary, never
/// unwound into C. If this regresses the process aborts rather than failing the test.
#[test]
fn panic_in_a_binding_is_converted_not_unwound() {
    let lua = Lua::new().unwrap();
    let f = lua
        .create_function(|_, ()| -> Result<()> { panic!("deliberate") })
        .unwrap();
    lua.globals().set("panicky", f).unwrap();

    let (ok, msg): (bool, String) = lua.load("return pcall(panicky)").call(()).unwrap();
    assert!(!ok);
    assert!(msg.contains("deliberate"), "panic payload must reach Lua: {msg}");
    // The VM must still be usable afterwards.
    assert_eq!(lua.load("return 1 + 1").eval::<f32>().unwrap(), 2.0);
}

/// An error raised *by Lua* and caught by our `pcall` wrapper must not leave the stack dirty —
/// this is the leak that only shows up after thousands of failed calls.
#[test]
fn repeated_errors_do_not_grow_the_stack() {
    let lua = Lua::new().unwrap();
    let base = top(&lua);
    for _ in 0..200 {
        let e = lua.load("error('x')").exec().unwrap_err();
        assert!(matches!(e, Error::RuntimeError(_)));
    }
    assert_eq!(top(&lua), base, "each failed call must clean up after itself");
}

#[test]
fn repeated_successful_calls_do_not_grow_the_stack() {
    let lua = Lua::new().unwrap();
    let f = lua.create_function(|_, n: f32| Ok(n * 2.0)).unwrap();
    lua.globals().set("dbl", f).unwrap();
    let base = top(&lua);
    for i in 0..200 {
        let n: f32 = lua.load("return dbl(21)").eval().unwrap();
        assert_eq!(n, 42.0, "iteration {i}");
    }
    assert_eq!(top(&lua), base);
}

/// `set_environment` is the module system's primitive — a chunk's bare `function Foo()` must land
/// in the supplied table and NOT in globals. This is `lua_setfenv`, native in 5.1.
#[test]
fn set_environment_scopes_a_chunks_globals() {
    let lua = Lua::new().unwrap();
    let env = lua.create_table().unwrap();
    // Fall through to the real globals, exactly as the module loader does.
    let mt = lua.create_table().unwrap();
    mt.set("__index", lua.globals()).unwrap();
    env.set_metatable(Some(mt)).unwrap();

    lua.load("function Init() return 7 end  MODULE_LOCAL = 1")
        .set_environment(env.clone())
        .exec()
        .unwrap();

    assert!(env.get::<Function>("Init").is_ok(), "Init lands in the module env");
    assert!(
        matches!(lua.globals().get::<Value>("MODULE_LOCAL").unwrap(), Value::Nil),
        "the module must not leak into globals"
    );
    let seven: f32 = env.get::<Function>("Init").unwrap().call(()).unwrap();
    assert_eq!(seven, 7.0);
}

/// `__index` fallthrough must actually resolve, and clearing the metatable must undo it. This is
/// the mechanism behind both `import` (env → `_G`) and `inherit(base)` (module → base module).
#[test]
fn metatable_index_chains_and_clears() {
    let lua = Lua::new().unwrap();
    let base = lua.create_table().unwrap();
    base.set("Shared", "from-base").unwrap();

    let derived = lua.create_table().unwrap();
    let mt = lua.create_table().unwrap();
    mt.set("__index", base).unwrap();
    derived.set_metatable(Some(mt)).unwrap();

    assert_eq!(derived.get::<String>("Shared").unwrap(), "from-base");
    // `raw_get` must NOT see through the chain — that distinction is why both exist.
    assert!(matches!(derived.raw_get::<Value>("Shared").unwrap(), Value::Nil));
    assert!(derived.get_metatable().is_some());

    derived.set_metatable(None).unwrap();
    assert!(derived.get_metatable().is_none());
    assert!(matches!(derived.get::<Value>("Shared").unwrap(), Value::Nil));
    assert_eq!(top(&lua), 0);
}

#[test]
fn create_string_interns_bytes() {
    let lua = Lua::new().unwrap();
    let s = lua.create_string("pmc_hum_mattias").unwrap();
    assert_eq!(s.to_string_lossy(), "pmc_hum_mattias");
    // Non-UTF-8 must survive: Lua strings are byte strings.
    let raw = lua.create_string([0xffu8, 0x00, 0x41]).unwrap();
    assert_eq!(raw.as_bytes(), vec![0xff, 0x00, 0x41]);
    assert_eq!(top(&lua), 0);
}

/// Handles must cross losslessly at full pointer width — the reason GUIDs are lightuserdata and
/// not numbers.
#[test]
fn lightuserdata_survives_a_round_trip_through_lua() {
    let lua = Lua::new().unwrap();
    let p = LightUserData(0xDEAD_BEEF_1234_5678u64 as usize as *mut std::ffi::c_void);
    let f = lua.create_function(move |_, h: LightUserData| Ok(h)).unwrap();
    lua.globals().set("echo", f).unwrap();
    lua.globals().set("handle", p).unwrap();
    let back: LightUserData = lua.load("return echo(handle)").eval().unwrap();
    assert_eq!(back, p);
}

/// The f32 model, pinned. Retail's `lua_Number` was float, so this loss IS the game's arithmetic —
/// the test exists so nobody "fixes" it to f64 later.
#[test]
fn numbers_are_single_precision_by_design() {
    let lua = Lua::new().unwrap();
    // 2^24+1 is the first integer f32 cannot represent.
    let n: f64 = lua.load("return 16777217").eval().unwrap();
    assert_eq!(n, 16777216.0, "f32 rounds 2^24+1 down — this is retail behaviour");

    // The ~1e9 cash clamp the script host mentions: representable, but not to the unit.
    let cash: f64 = lua.load("return 1000000001").eval().unwrap();
    assert_ne!(cash, 1000000001.0, "beyond 2^24 the unit digit is not preserved");
}

#[test]
fn tables_read_write_and_enumerate() {
    let lua = Lua::new().unwrap();
    let t: Table = lua.load("return {10, 20, 30, name = 'kit'}").eval().unwrap();
    assert_eq!(t.len(), 3);
    assert_eq!(t.get::<String>("name").unwrap(), "kit");

    let seq: Vec<f32> = t.sequence_values::<f32>().collect::<Result<_>>().unwrap();
    assert_eq!(seq, vec![10.0, 20.0, 30.0]);

    t.set("added", true).unwrap();
    assert!(t.get::<bool>("added").unwrap());

    let pairs = t.pairs::<Value, Value>().collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(pairs.len(), 5, "3 array + name + added");
    assert_eq!(top(&lua), 0);
}

/// `Variadic` forwards an argument vector plus a trailing value — the `dynamic_import` shape,
/// `fCallback(unpack(tArgs), mModule)`.
#[test]
fn variadic_forwards_arguments_in_order() {
    let lua = Lua::new().unwrap();
    let f: Function = lua
        .load("return function(...) local t = {...} return #t, t[1], t[3] end")
        .eval()
        .unwrap();
    let vals: Vec<Value> = vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)];
    let (count, first, third): (f32, f32, f32) =
        f.call(Variadic::from_iter(vals)).unwrap();
    assert_eq!((count, first, third), (3.0, 1.0, 3.0));
}

/// A handle must keep the VM alive on its own, or a `Table` outliving its `Lua` dangles.
#[test]
fn a_table_outlives_the_lua_handle_it_came_from() {
    let t = {
        let lua = Lua::new().unwrap();
        lua.load("return {v = 99}").eval::<Table>().unwrap()
    };
    assert_eq!(t.get::<f32>("v").unwrap(), 99.0);
}

/// Registry refs must be released, or every value ever seen leaks for the VM's lifetime.
#[test]
fn dropped_handles_release_their_registry_slots() {
    let lua = Lua::new().unwrap();
    let before = registry_len(&lua);
    for _ in 0..500 {
        let t: Table = lua.load("return {}").eval().unwrap();
        let _ = t.len();
    }
    let after = registry_len(&lua);
    assert!(
        after <= before + 8,
        "registry grew {before} → {after}; refs are not being unref'd"
    );
}

/// Count the registry's array part — a proxy for outstanding `luaL_ref` slots.
fn registry_len(lua: &Lua) -> usize {
    unsafe {
        sys::lua_checkstack(lua.state(), 1);
        sys::lua_pushvalue(lua.state(), sys::LUA_REGISTRYINDEX);
        let n = sys::lua_objlen(lua.state(), -1);
        sys::lua_pop(lua.state(), 1);
        n
    }
}

/// Errors from a syntactically invalid chunk must be classified, and must name the chunk.
#[test]
fn syntax_errors_are_reported_with_the_chunk_name() {
    let lua = Lua::new().unwrap();
    let e = lua
        .load("function oops(")
        .set_name("@wifpmcinterior")
        .exec()
        .unwrap_err();
    match e {
        Error::SyntaxError(m) => assert!(m.contains("wifpmcinterior"), "{m}"),
        other => panic!("expected a syntax error, got {other:?}"),
    }
}

/// The 5.1 natives that replace the 5.4 compat prelude, exercised through the safe layer.
#[test]
fn lua_51_natives_the_corpus_uses_are_reachable() {
    let lua = Lua::new().unwrap();
    let n: f32 = lua.load("return table.getn({1,2,3,4})").eval().unwrap();
    assert_eq!(n, 4.0, "table.getn — 112 corpus uses");

    let (a, b): (f32, f32) = lua.load("return unpack({8, 9})").call(()).unwrap();
    assert_eq!((a, b), (8.0, 9.0), "unpack — 76 corpus uses");

    // getfenv/setfenv (18 uses) and the implicit `arg` table, together.
    let v: f32 = lua
        .load("local function f(...) return arg.n end return f(1,2,3)")
        .eval()
        .unwrap();
    assert_eq!(v, 3.0, "implicit vararg `arg` table");
}

/// Values this layer does not model must still round-trip untouched rather than becoming nil.
#[test]
fn unmodelled_values_round_trip_losslessly() {
    let lua = Lua::new().unwrap();
    let f = lua.create_function(|_, v: Value| Ok(v)).unwrap();
    lua.globals().set("echo", f).unwrap();
    let same: bool = lua
        .load("local co = coroutine.create(function() end) return echo(co) == co")
        .eval()
        .unwrap();
    assert!(same, "a thread must survive a Rust binding unchanged");
}
