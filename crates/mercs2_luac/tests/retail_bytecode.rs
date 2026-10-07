//! Can this VM **load the bytecode the game shipped**?
//!
//! `tests/parity_retail.rs` proves the compiler *emits* retail's dialect. This proves the other direction:
//! the runtime *reads* it. Together they close the loop — one Lua, and it is the game's.
//!
//! That matters beyond tidiness. The reimpl runs the **decompiled** corpus, so everything it
//! executes inherits the decompiler's fidelity. Loading the shipped chunks directly removes that
//! dependency for anything that does not need readable source. `lundump` reads 4-byte string
//! lengths and a `sizeof(size_t)=4` header, the same dialect the compiler dumps.
//!
//! Executing retail bytecode needs a **runtime**. The workspace links one Lua, this crate's, for
//! both compiling and running (see the module note in `tests/common/mod.rs`).
//!
//! This file holds the hermetic round trips through our own compiler. The loads of the chunks retail
//! shipped are game-gated and live in `retail_bytecode_retail.rs`.

use mercs2_luac::rt::Lua;

/// The round trip, without needing a game install: our own compiler's output must load and run in
/// our own runtime. If the dump and undump halves ever disagree, this fails in the hermetic run,
/// without the game install the retail loads in `retail_bytecode_retail.rs` need.
#[test]
fn our_own_bytecode_loads_and_runs() {
    let bytes = mercs2_luac::compile("return 6 * 7", "roundtrip").expect("compile");
    assert_eq!(&bytes[..4], b"\x1bLua", "compiled to a binary chunk");

    let lua = Lua::new().expect("vm");
    // Loading BYTES, not source — `luaL_loadbuffer` dispatches on the signature.
    let n: f32 = lua.load(&bytes).eval().expect("load + run the compiled chunk");
    assert_eq!(n, 42.0);
}

/// A precompiled chunk and its source must produce the same result, so "we loaded bytecode" is not
/// quietly "we reparsed text".
#[test]
fn bytecode_and_source_agree() {
    let src = "local t = {} for i = 1, 5 do t[i] = i * i end return t[1] + t[2] + t[3] + t[4] + t[5]";
    let lua = Lua::new().expect("vm");

    let from_source: f32 = lua.load(src).eval().expect("source");
    let compiled = mercs2_luac::compile(src, "agree").expect("compile");
    let from_bytecode: f32 = lua.load(&compiled).eval().expect("bytecode");

    assert_eq!(from_source, from_bytecode);
    assert_eq!(from_source, 55.0);
}
