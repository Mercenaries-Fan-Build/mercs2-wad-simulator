//! Builds m2-sdk.dll: the version from `M2_VERSION` (the release passes the tag without its `v`; an
//! untagged build is 0.0.0, a version no release carries), the vendored MinHook, the export list and
//! import library from `m2-sdk.def` with `load_ladder.gen.h` beside it, and the msvcrt-based MinGW
//! libraries fetched by `scripts/fetch_mingw_libs.sh`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn version() -> (String, u32) {
    println!("cargo:rerun-if-env-changed=M2_VERSION");
    let text = env::var("M2_VERSION").unwrap_or_else(|_| "0.0.0".to_owned());
    let parts: Vec<u32> = text
        .split('.')
        .map(|p| p.parse().unwrap_or_else(|_| panic!("M2_VERSION={text:?} is not MAJOR.MINOR.PATCH")))
        .collect();
    let [major, minor, patch] = parts[..] else { panic!("M2_VERSION={text:?} is not MAJOR.MINOR.PATCH") };
    assert!(minor < 100 && patch < 100, "M2_VERSION={text:?}: m2_version_num packs MINOR and PATCH into two digits each");
    (text, major * 10000 + minor * 100 + patch)
}

fn main() {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    assert!(
        os == "windows" && arch == "x86",
        "m2-sdk.dll is a 32-bit Windows DLL: build it with --target i686-pc-windows-gnu (got {arch}-{os})"
    );

    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    let (text, num) = version();
    fs::write(
        out.join("version.rs"),
        format!("pub const NUM: i32 = {num};\npub const STRING: &core::ffi::CStr = c\"{text}\";\n"),
    )
    .unwrap();

    let minhook = dir.join("minhook");
    let sources = ["src/hook.c", "src/buffer.c", "src/trampoline.c", "src/hde/hde32.c"];
    for s in sources {
        println!("cargo:rerun-if-changed={}", minhook.join(s).display());
    }
    cc::Build::new()
        .include(minhook.join("include"))
        .files(sources.iter().map(|s| minhook.join(s)))
        .opt_level(2)
        .warnings(true)
        .extra_warnings(false)
        .compile("minhook");

    let mingw = dir.join("toolchain/i686-mingw");
    let stamp = mingw.join("VERSION");
    println!("cargo:rerun-if-changed={}", stamp.display());
    let rustc = env::var("RUSTC").unwrap();
    let want = String::from_utf8(Command::new(&rustc).arg("-V").output().unwrap().stdout).unwrap();
    let have = fs::read_to_string(&stamp).unwrap_or_else(|e| {
        panic!("{}: {e}; run scripts/fetch_mingw_libs.sh", stamp.display())
    });
    assert_eq!(have.trim(), want.trim(), "{} is for another rustc; run scripts/fetch_mingw_libs.sh", stamp.display());
    println!("cargo:rustc-link-search=native={}", mingw.display());

    println!("cargo:rerun-if-env-changed=M2_IMPLIB");
    let implib = env::var("M2_IMPLIB").map(PathBuf::from).unwrap_or_else(|_| out.join("libm2-sdk.dll.a"));
    println!("cargo:rerun-if-changed={}", implib.display());
    let def = dir.join("m2-sdk.def");
    println!("cargo:rerun-if-changed={}", def.display());
    println!("cargo:rustc-cdylib-link-arg={}", def.display());
    println!("cargo:rustc-cdylib-link-arg=-Wl,--out-implib,{}", implib.display());

    let header = implib.with_file_name("load_ladder.gen.h");
    println!("cargo:rerun-if-changed={}", header.display());
    fs::write(&header, mercs2_formats::load_ladder::c_header()).unwrap();
}
