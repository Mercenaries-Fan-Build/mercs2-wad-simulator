//! The Lua host's sites, the shader sites and the profiler sites resolve exactly once, at the addresses read from the
//! disassembly, in the reference executables: the v1.0-code builds `v10-securom-unwrap`,
//! `v10-drmfree-laa` and `dump-v10-pmcbb-unpacked` (one layout) and the v1.1 dump
//! `dump-v11-securom-unpacked` (code at other addresses).
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. Each executable is
//! found by its catalogued sha256 in the canonical tree of PC builds that the repo-root
//! `.mercs2-local.toml` key `canonical_dir` names.

use std::path::{Path, PathBuf};

use mercs2_exemap::canonical::{build_path, builds_by_sha};
use mercs2_exemap::Catalogue;
use mercs2_formats::exe_image::Image;
use mercs2_formats::game_paths::local_config_canonical_dir;
use mercs2_formats::lua_sites::{resolve, LuaSites, Site};

/// The builds under test, each with whether it runs the v1.1 code.
const BUILDS: [(&str, bool); 4] = [
    ("v10-securom-unwrap", false),
    ("v10-drmfree-laa", false),
    ("dump-v10-pmcbb-unpacked", false),
    ("dump-v11-securom-unpacked", true),
];

/// The bytes of every build in [`BUILDS`], in order.
fn images() -> Vec<(&'static str, bool, Vec<u8>)> {
    let c = Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"));
    let dir: PathBuf = local_config_canonical_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"));
    let files = builds_by_sha(&c, &dir).unwrap_or_else(|e| panic!("{e}"));
    BUILDS
        .iter()
        .map(|&(id, v11)| {
            let path = build_path(&c, &files, &dir, id).unwrap_or_else(|e| panic!("{e}"));
            (id, v11, std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())))
        })
        .collect()
}

fn sites(id: &str, bytes: &[u8]) -> LuaSites {
    let img = Image::from_file(bytes).unwrap_or_else(|e| panic!("{id}: {e}"));
    resolve(&img).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// `(site, v1.0 VA, v1.1 VA)`, read with objdump; the by-name VAs equal lua_bridge v0.5.4's
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
fn every_site_resolves_in_every_reference_build() {
    for (id, v11, bytes) in images() {
        let s = sites(id, &bytes);
        for (site, va, va11) in EXPECTED {
            assert_eq!(s.va(site), if v11 { va11 } else { va }, "{id} {}", site.name());
        }
        assert_eq!(s.host(), if v11 { 0x14a4da0 } else { 0x14a5da0 }, "{id} host");
    }
}

/// The v1.1 dump's family vtables, in family order, from its `.rdata` (the structural scan).
const V11_VTABLES: [u32; 45] = [
    0xbe88cc, 0xbaadd0, 0xbe84a4, 0xbac878, 0xbab148, 0xbe8938, 0xbab598, 0xbab580, 0xbaaedc, 0xbac604, 0xbaade8, 0xbac304,
    0xbaada0, 0xbaae84, 0xbaaf8c, 0xbac400, 0xbad148, 0xbad160, 0xbad118, 0xbad130, 0xbaae5c, 0xbab600, 0xbaadb8, 0xbab974,
    0xbac158, 0xbab95c, 0xbe88e4, 0xbac860, 0xbac848, 0xbab130, 0xbab568, 0xbac5ec, 0xbac2ec, 0xbac6a0, 0xbaaf74, 0xbac3e8,
    0xbac438, 0xbad100, 0xbad0e8, 0xbac96c, 0xbab5e8, 0xbab618, 0xbaccc8, 0xbac140, 0xbab944,
];

#[test]
fn shader_sites_resolve_in_every_reference_build() {
    use mercs2_formats::shader_registry::family;
    use mercs2_formats::shader_sites::resolve as resolve_shaders;
    for (id, v11, bytes) in images() {
        let s = resolve_shaders(&Image::from_file(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"))).unwrap_or_else(|e| panic!("{id}: {e}"));
        let (shift, pool, level) = if v11 { (0x270, 0x1976a38, 0xdfb345) } else { (0, 0x1977a38, 0xdfc345) };
        assert_eq!(s.register_all, 0x84f130 + shift, "{id}");
        assert_eq!(s.register, 0x85ac90 + shift, "{id}");
        assert_eq!(s.ps_base_ctor, 0x85ace0 + shift, "{id}");
        assert_eq!(s.vs_base_ctor, 0x85ade0 + shift, "{id}");
        assert_eq!((s.ps_count, s.ps_keys, s.vs_count, s.vs_keys), (pool, pool + 0x2008, pool + 0x6008, pool + 0x6410), "{id}");
        assert_eq!(s.shader_level, level, "{id}");
        for f in 0..45 {
            let want = if v11 { V11_VTABLES[f] } else { family(f as i32).unwrap().vtable };
            assert_eq!(s.vtable[f], want, "{id} family {f}");
        }
    }
}

/// `(site, v1.0 VA, v1.1 VA)`, read with objdump.
const PROFILER_EXPECTED: [(mercs2_formats::profiler_sites::Site, u32, u32); 5] = [
    (mercs2_formats::profiler_sites::Site::RunFrame, 0x630ef0, 0x630e10),
    (mercs2_formats::profiler_sites::Site::BindDevice, 0x749060, 0x749270),
    (mercs2_formats::profiler_sites::Site::JobRegister, 0x84b230, 0x84b4a0),
    (mercs2_formats::profiler_sites::Site::JobWait, 0x8765c0, 0x876810),
    (mercs2_formats::profiler_sites::Site::CellsChangeRing, 0x735aef, 0x735cef),
];

#[test]
fn profiler_sites_resolve_in_every_reference_build() {
    use mercs2_formats::profiler_sites::resolve as resolve_profiler;
    for (id, v11, bytes) in images() {
        let s = resolve_profiler(&Image::from_file(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"))).unwrap_or_else(|e| panic!("{id}: {e}"));
        for (site, va, va11) in PROFILER_EXPECTED {
            assert_eq!(s.va(site), if v11 { va11 } else { va }, "{id} {}", site.name());
        }
        // `mov eax, [RenderSystem]` in BindDevice, read with objdump.
        assert_eq!(s.render_system(), if v11 { 0x1175288 } else { 0x1176288 }, "{id} RenderSystem global");
        // `mov eax, [count]` and the two `mov [eax*8+table], ecx` in JobRegister, read with objdump.
        let jobs = s.jobs();
        assert_eq!(jobs.count, if v11 { 0x117563c } else { 0x117663c }, "{id} jobtype count");
        assert_eq!(jobs.handlers, if v11 { [0x19f804c, 0x19f8178] } else { [0x19f904c, 0x19f9178] }, "{id} handler tables");
        // The counter and ring in CellsChangeRing, read with objdump.
        let ring = s.cells_change();
        assert_eq!(ring.counter, if v11 { 0xdd4788 } else { 0xdd5788 }, "{id} cells change counter");
        assert_eq!(ring.ring, if v11 { 0xdd4790 } else { 0xdd5790 }, "{id} cells change ring");
    }
}
