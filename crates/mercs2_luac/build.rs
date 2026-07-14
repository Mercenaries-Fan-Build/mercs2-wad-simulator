//! Compile the vendored Mercenaries-2 flavour of Lua 5.1.5 into this crate.
//!
//! `vendor/` is upstream Lua 5.1.5 with the three patches from
//! `tools/lua51-mercs2/patches/` already applied:
//!
//! 1. `luaconf.h`  — `lua_Number` is `float` (4-byte single precision), not `double`.
//! 2. `ldump.c`    — string lengths are dumped as `uint32_t`, not native `size_t`.
//! 3. `lundump.c`  — string lengths are read as 4 bytes, and the chunk header hard-codes
//!                   `sizeof(size_t) = 4`.
//!
//! (2) and (3) are what let a **64-bit** host emit bytecode the 32-bit game accepts. The
//! shipped `tools/lua51-mercs2` build sidesteps them by compiling with a 32-bit mingw gcc;
//! we cannot, because the Rust host is x86_64 — so the patches are load-bearing here.
//!
//! Target header: `1b 4c 75 61 51 00 01 04 04 04 04 00`.

fn main() {
    // Every core/lib translation unit except the three that carry a `main()`
    // (`lua.c`, `luac.c`, `print.c`) — we want the library, not the CLIs.
    const UNITS: &[&str] = &[
        "lapi", "lauxlib", "lbaselib", "lcode", "ldblib", "ldebug", "ldo", "ldump", "lfunc",
        "lgc", "linit", "liolib", "llex", "lmathlib", "lmem", "loadlib", "lobject", "lopcodes",
        "loslib", "lparser", "lstate", "lstring", "lstrlib", "ltable", "ltablib", "ltm",
        "lundump", "lvm", "lzio",
    ];

    let mut build = cc::Build::new();
    build.include("vendor").warnings(false);

    for unit in UNITS {
        build.file(format!("vendor/{unit}.c"));
        println!("cargo:rerun-if-changed=vendor/{unit}.c");
    }
    println!("cargo:rerun-if-changed=vendor/luaconf.h");
    println!("cargo:rerun-if-changed=build.rs");

    // Lua's own recommended platform defines. Without LUA_USE_* it still builds (ANSI C),
    // which is all we need — we only ever parse + dump, never run a script.
    #[cfg(unix)]
    build.define("LUA_USE_POSIX", None);

    build.compile("mercs2lua");
}
