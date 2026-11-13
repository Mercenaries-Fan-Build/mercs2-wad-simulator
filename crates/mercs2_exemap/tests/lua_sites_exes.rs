//! The Lua host's sites, the shader sites, the profiler sites and pmc_bb's sites resolve exactly once, at the addresses read from the
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

/// The debugger's sites: `luaU_undump` and `luaD_poscall`, read with objdump in the v1.0 dump and found
/// with the same bytes 0x220 later in the v1.1 dump.
#[test]
fn debug_sites_resolve_in_every_reference_build() {
    use mercs2_formats::lua_sites::{resolve_debug, DebugSites};
    for (id, v11, bytes) in images() {
        let s = resolve_debug(&Image::from_file(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"))).unwrap_or_else(|e| panic!("{id}: {e}"));
        let want = if v11 { DebugSites { undump: 0x8713f0, poscall: 0x868a70 } } else { DebugSites { undump: 0x8711d0, poscall: 0x868850 } };
        assert_eq!(s, want, "{id} debug sites");
    }
}

/// The debugger names engine bindings by running `luareg` over the game's executable; every reference build
/// carries the same named registry (62 tables, 31 named, 1,375 entries; the counts of `exemap luareg` over
/// the v1.0 and v1.1 dumps and the shipped v1.1 exe), at its own function addresses.
#[test]
fn every_reference_build_names_its_engine_bindings() {
    use mercs2_exemap::luareg::tables;
    for (id, v11, bytes) in images() {
        let t = tables(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"));
        let named = t.iter().filter(|t| t.namespace.is_some()).count();
        let entries: usize = t.iter().map(|t| t.entries.len()).sum();
        assert_eq!((t.len(), named, entries), (62, 31, 1375), "{id}");
        // `string.byte`, the first row of the first table, read from both dumps' luareg output.
        let byte = t.iter().flat_map(|t| &t.entries).find(|(n, _)| n == "byte").map(|e| e.1);
        assert_eq!(byte, Some(if v11 { 0x8638c0 } else { 0x8636a0 }), "{id}");
    }
}

/// `exemap lua-calls` over the three PC images of images.tsv (the builds pc-v10, pc-v10-drmfree and
/// pc-v11 are): the call sites of each by-name caller, and rows read with objdump. The 124 `call` sites
/// of pc-v10's 0x5a0cc0 equal an objdump pass over every `E8` to it, value for value; the three more are
/// push-push-ret calls in moved code.
#[test]
fn lua_calls_read_every_by_name_call_of_the_pc_images() {
    use mercs2_exemap::luacalls::{x86_calls, Callers, Functions, Val};
    let c = Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"));
    let table = mercs2_exemap::images::Images::embedded(&c).unwrap_or_else(|e| panic!("{e}"));
    let callers = Callers::embedded(&table).unwrap_or_else(|e| panic!("{e}"));
    let none = Functions::parse("", "no export").unwrap_or_else(|e| panic!("{e}"));
    let by_build = |b: &str| table.images.iter().find(|i| i.build_id.as_deref() == Some(b)).map(|i| i.id.clone()).expect(b);
    // (build, sites per caller in lua_callers.tsv order)
    let want: [(&str, [usize; 2]); 3] =
        [("dump-v10-pmcbb-unpacked", [127, 12]), ("v10-drmfree-laa", [127, 12]), ("dump-v11-securom-unpacked", [126, 10])];
    let all = images();
    for (build, counts) in want {
        let bytes = &all.iter().find(|(id, ..)| *id == build).expect(build).2;
        let id = by_build(build);
        let rows: Vec<_> = callers.of(&id).collect();
        assert_eq!(rows.len(), 2, "{id}: the by-name caller and the object dispatcher");
        for (caller, n) in rows.iter().zip(counts) {
            let (sites, calls) = x86_calls(bytes, &none, caller).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!((sites, calls.len()), (n, n), "{id} caller 0x{:08x}", caller.va);
            if id != "pc-v11" {
                let at = |s: u32| calls.iter().find(|k| k.site == Some(s)).unwrap_or_else(|| panic!("{id} site 0x{s:08x}"));
                if caller.va == 0x005a0cc0 {
                    let k = at(0x006f5c95);
                    assert_eq!((&k.module, &k.name), (&Val::Known(0x51ee_8f14), &Val::Known("NetSafePlayCheapCinematic".into())));
                    let k = at(0x006fea91);
                    assert_eq!((&k.module, &k.name), (&Val::Computed, &Val::Known("NetEventCallback".into())));
                    // the object dispatcher's own forward, in moved code whose junk bytes no decode lands through
                    assert_eq!(at(0x006c6f46).name, Val::Unread);
                } else {
                    assert_eq!(at(0x0066c33b).name, Val::Known("OnActivate".into()));
                    assert_eq!(at(0x0066c33b).module, Val::Object);
                    assert_eq!(at(0x028b712a).name, Val::Known("OnUse".into()), "the push-push-ret call");
                }
            }
        }
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

/// `(site, v1.0 value, v1.1 value)`, read from the instruction bytes of both dumps.
const PMC_EXPECTED: [(mercs2_formats::pmc_sites::Site, u32, u32); 8] = [
    (mercs2_formats::pmc_sites::Site::EntityVtable, 0xbdb410, 0xbdb348),
    (mercs2_formats::pmc_sites::Site::PrmgKeys, 0x197de48, 0x197ce48),
    (mercs2_formats::pmc_sites::Site::PrmgHandles, 0x197da48, 0x197ca48),
    (mercs2_formats::pmc_sites::Site::SpawnWalk, 0x4ab0d0, 0x4ab0d0),
    (mercs2_formats::pmc_sites::Site::PrmgDrawA, 0x47a6c0, 0x47a7a0),
    (mercs2_formats::pmc_sites::Site::PrmgDrawB, 0x47aa20, 0x47ab00),
    (mercs2_formats::pmc_sites::Site::TextureFree, 0x116a7cc, 0x11697cc),
    (mercs2_formats::pmc_sites::Site::TextureInUse, 0x1174fe8, 0x1173fe8),
];

#[test]
fn pmc_sites_resolve_in_every_reference_build() {
    use mercs2_formats::pmc_sites::resolve as resolve_pmc;
    for (id, v11, bytes) in images() {
        let s = resolve_pmc(&Image::from_file(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"))).unwrap_or_else(|e| panic!("{id}: {e}"));
        for (site, va, va11) in PMC_EXPECTED {
            assert_eq!(s.value(site), if v11 { va11 } else { va }, "{id} {}", site.name());
        }
        // The operands of the IB and VB lookups, which no signature can tell apart.
        let lists = s.pool_lists();
        assert_eq!(
            (lists.ib_in_use, lists.vb_in_use, lists.ib_free, lists.vb_free),
            if v11 { (0x1173fec, 0x1173ff0, 0x1173ff4, 0x1173ff8) } else { (0x1174fec, 0x1174ff0, 0x1174ff4, 0x1174ff8) },
            "{id}"
        );
    }
}

/// `(site, v1.0 VA, v1.1 VA)`, read with objdump.
const PROFILER_EXPECTED: [(mercs2_formats::profiler_sites::Site, u32, u32); 10] = [
    (mercs2_formats::profiler_sites::Site::RunFrame, 0x630ef0, 0x630e10),
    (mercs2_formats::profiler_sites::Site::BindDevice, 0x749060, 0x749270),
    (mercs2_formats::profiler_sites::Site::JobRegister, 0x84b230, 0x84b4a0),
    (mercs2_formats::profiler_sites::Site::JobWait, 0x8765c0, 0x876810),
    (mercs2_formats::profiler_sites::Site::CellsChangeRing, 0x735aef, 0x735cef),
    (mercs2_formats::profiler_sites::Site::PacketProducer, 0x8546a0, 0x854910),
    (mercs2_formats::profiler_sites::Site::PacketFrameEnd, 0x854b38, 0x854da8),
    (mercs2_formats::profiler_sites::Site::FrameBegin, 0x853fbd, 0x85422d),
    (mercs2_formats::profiler_sites::Site::StoreAlloc, 0x853cee, 0x853f5e),
    (mercs2_formats::profiler_sites::Site::MatrixIndex, 0x477e20, 0x477f00),
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
        // The frame stores, read with Ghidra: the counters the begin-frame zeroes, the sizes the
        // allocator takes, the matrix table's per-parity multiplier.
        let f = s.frame_stores();
        let (packets, list, sidecar, matrices) = if v11 { (0xff36f0, 0xff36d4, 0xff36c8, 0xff36cc) } else { (0xff46f0, 0xff46d4, 0xff46c8, 0xff46cc) };
        let (parity, packet_array, matrix_table, list_base, sidecar_base) =
            if v11 { (0xff364c, 0xff3700, 0x18c61e0, 0xff36c0, 0xff36d0) } else { (0xff464c, 0xff4700, 0x18c71e0, 0xff46c0, 0xff46d0) };
        assert_eq!(
            f,
            mercs2_formats::profiler_sites::FrameStores {
                begin: if v11 { 0x854070 } else { 0x853e00 },
                packets,
                packet_cap: 0x2000,
                list_dwords: list,
                list_bytes: 0x50000,
                sidecar_bytes_used: sidecar,
                sidecar_bytes: 0x20000,
                matrices,
                matrix_cap: 0x1400,
                parity,
                packet_array,
                packet_bytes: 0x58,
                matrix_table,
                list_base,
                sidecar_base,
                // `movzx edx, word [program]` (+0xEF) and `cmp byte [class], 0` / `mov dl, [class]` in the
                // producer, read with Ghidra.
                draw_program: if v11 { 0x1163748 } else { 0x1164748 },
                draw_class: if v11 { 0x116374c } else { 0x116474c },
            },
            "{id} frame stores"
        );
    }
}

#[test]
fn chatter_sites_resolve_in_every_reference_build() {
    use mercs2_formats::chatter_sites::resolve as resolve_chatter;
    for (id, v11, bytes) in images() {
        let s = resolve_chatter(&Image::from_file(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"))).unwrap_or_else(|e| panic!("{id}: {e}"));
        // Read with Ghidra: the ChatterSet registrar's container (FUN_00641020 v1.1 / FUN_00641120 v1.0), the
        // field writer (FUN_0065BD30 v1.1; v1.0 FUN_0065BDC0 is the ChatterSet template of
        // docs/mercs2-ecs/02_ai_perception_population.md), the lookup, the add, the mask setter and its flag byte.
        let (container, writer, lookup, add, mask, flag) = if v11 {
            (0x17bc498, 0x65bd30, 0x587d80, 0x64a570, 0x665500, 0xcfa58a)
        } else {
            (0x17bd498, 0x65bdc0, 0x5857e0, 0x64a600, 0x665590, 0xcfb58a)
        };
        assert_eq!((s.container, s.writer, s.lookup, s.add, s.mask, s.flag), (container, writer, lookup, add, mask, flag), "{id} chatter sites");
    }
}

#[test]
fn tree_sites_resolve_in_every_reference_build() {
    use mercs2_formats::tree_sites::{resolve as resolve_trees, TreeSites};
    for (id, v11, bytes) in images() {
        let s = resolve_trees(&Image::from_file(&bytes).unwrap_or_else(|e| panic!("{id}: {e}"))).unwrap_or_else(|e| panic!("{id}: {e}"));
        // Read with Ghidra: the two loops (`FUN_004A3E10`/`FUN_004A3EA0` in v1.1), the identity they copy
        // from and the copy (`FUN_00836120`), the matrix index and draw record the matrix push writes, the
        // batch count both loops read.
        let want = if v11 {
            TreeSites {
                zpass_loop: 0x4a3ea0,
                shadow_loop: 0x4a3e10,
                identity: 0xdfbbd0,
                matrix_copy: 0x836120,
                matrix_index: 0x1163740,
                draw_record: 0x1163742,
                batches: 0xe8f734,
                matrices: 0xff36cc,
                parity: 0xff364c,
                matrix_table: 0x18c61e0,
                matrix_cap: 0x1400,
            }
        } else {
            TreeSites {
                zpass_loop: 0x4a3e40,
                shadow_loop: 0x4a3db0,
                identity: 0xdfcbd0,
                matrix_copy: 0x835e80,
                matrix_index: 0x1164740,
                draw_record: 0x1164742,
                batches: 0xe90734,
                matrices: 0xff46cc,
                parity: 0xff464c,
                matrix_table: 0x18c71e0,
                matrix_cap: 0x1400,
            }
        };
        assert_eq!(s, want, "{id} tree sites");
    }
}
