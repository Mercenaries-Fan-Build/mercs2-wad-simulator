//! The Lua host's sites resolve exactly once, at the addresses read from the disassembly, in the three
//! reference executables named by `exe_dumps` in `.mercs2-local.toml`: `mercs2_nodrm_v3.exe` and
//! `mercs2_unpacked.exe` (one layout) and `genuine_patched_unpacked.exe` (the retail v1.1 dump, code at
//! other addresses).

use mercs2_formats::exe_image::Image;
use mercs2_formats::game_paths::local_config_exe_dump;
use mercs2_formats::lua_sites::{LuaSites, Site, resolve};

fn sites(name: &str) -> LuaSites {
    let path = local_config_exe_dump(std::path::Path::new(env!("CARGO_MANIFEST_DIR")), name).unwrap_or_else(|e| panic!("{e}"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let img = Image::from_file(&bytes).unwrap();
    resolve(&img).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// `(site, nodrm_v3 / unpacked VA, v1.1 VA)`, read with objdump; the by-name VAs equal lua_bridge v0.5.4's
/// per-build tables for the same images.
const EXPECTED: [(Site, u32, u32); 11] = [
    (Site::RegisterNamespaces, 0x5a2c40, 0x5a2c30),
    (Site::ScriptHostInit, 0x5a1760, 0x5a1750),
    (Site::ScriptHostCtor, 0x5a1677, 0x5a1667),
    (Site::LuaLRegister, 0x85f500, 0x85f720),
    (Site::LuaSNewLStr, 0x86a2b0, 0x86a4d0),
    (Site::LuaGErrorMsg, 0x85eea0, 0x85f0c0),
    (Site::Print, 0x6d5640, 0x6aef90),
    (Site::LuaBType, 0x860c70, 0x860e90),
    (Site::LuaBLoadString, 0x860fc0, 0x8611e0),
    (Site::LuaBPcall, 0x8615f0, 0x861810),
    (Site::CreateTextWidget, 0x5b7d40, 0x5b7d30),
];

#[test]
fn every_site_resolves_in_all_three_images() {
    for (image, v11) in [("mercs2_nodrm_v3.exe", false), ("mercs2_unpacked.exe", false), ("genuine_patched_unpacked.exe", true)] {
        let s = sites(image);
        for (site, va, va11) in EXPECTED {
            assert_eq!(s.va(site), if v11 { va11 } else { va }, "{image} {}", site.name());
        }
        assert_eq!(s.host(), if v11 { 0x14a4da0 } else { 0x14a5da0 }, "{image} host");
    }
}
