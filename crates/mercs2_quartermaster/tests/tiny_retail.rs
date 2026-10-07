//! `add_tiny_geometry` against the retail game: the TINY shaders' slot read, every retail stand-in
//! decomposed and rebuilt, a stand-in built and read back, and the rules that need the game.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml` and the shader stores beside it, and fails if they are absent.

mod common {
    pub mod build;
}

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use common::build::{scratch, shipment};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::model_inject::read_f16_le;
use mercs2_formats::placement_build::{read_layer_records, remove_entity, FLGS_TINY_GEOMETRY_OBJECT};
use mercs2_formats::tiny_model::{self as tm, TinyModel};
use mercs2_quartermaster::tiny::{self, World};
use mercs2_quartermaster::{build, lint, GameStack};

fn vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"))
}

fn retail_game() -> GameStack {
    GameStack::open(&[vz_wad()]).unwrap_or_else(|e| panic!("the game stack: {e}"))
}

fn unhalf(h: u16) -> f32 {
    read_f16_le(&h.to_le_bytes(), 0)
}

// ── the shaders' slot read ──────────────────────────────────────────────────────────────────────

/// One operand of a disassembled instruction: register file, index, relative addressing, swizzle
/// and modifiers.
struct Operand {
    file: char,
    index: usize,
    relative: bool,
    swizzle: [usize; 4],
    negate: bool,
    abs: bool,
}

fn operand(text: &str) -> Operand {
    let (negate, t) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (base, swz) = match t.rsplit_once('.') {
        Some((b, s)) if s.chars().all(|c| "xyzw".contains(c)) => (b, s),
        _ => (t, "xyzw"),
    };
    let abs = base.ends_with("_abs");
    let base = base.trim_end_matches("_abs");
    let relative = base.contains("[a0.x]");
    let base = base.replace("[a0.x]", "");
    let file = base.chars().next().expect("a register");
    let index = base[1..].parse().unwrap_or_else(|_| panic!("register {text}"));
    let comps: Vec<usize> = swz.chars().map(|c| "xyzw".find(c).unwrap()).collect();
    let swizzle = std::array::from_fn(|k| comps[k.min(comps.len() - 1)]);
    Operand { file, index, relative, swizzle, negate, abs }
}

/// Run a TINY vertex shader's disassembly up to its first `dp4` for one vertex at `slot`, with
/// `ObjectIDScaleArray` holding `states` (slot `s` is component `s mod 4` of `c[s / 4]`). Returns
/// the keep factor in `r0.x`: 1 keeps the vertex, 0 collapses it.
fn keep(disasm: &str, slot: usize, states: &[f32]) -> f32 {
    let mut c: BTreeMap<usize, [f32; 4]> = BTreeMap::new();
    for (k, chunk) in states.chunks(4).enumerate() {
        let mut v = [0.0; 4];
        v[..chunk.len()].copy_from_slice(chunk);
        c.insert(k, v);
    }
    let mut r = [[0.0f32; 4]; 8];
    let v0 = [1.0, 2.0, 3.0, slot as f32];
    let mut a0 = 0usize;
    let mut skipping: Vec<bool> = Vec::new();
    let read = |o: &Operand, r: &[[f32; 4]; 8], c: &BTreeMap<usize, [f32; 4]>, a0: usize| -> [f32; 4] {
        let reg = match o.file {
            'r' => r[o.index],
            'v' => {
                assert_eq!(o.index, 0, "only POSITION is read before the keep factor");
                v0
            }
            'c' => {
                let i = if o.relative { o.index + a0 } else { o.index };
                *c.get(&i).unwrap_or(&[0.0; 4])
            }
            f => panic!("register file {f}"),
        };
        std::array::from_fn(|k| {
            let mut x = reg[o.swizzle[k]];
            if o.abs {
                x = x.abs();
            }
            if o.negate {
                x = -x;
            }
            x
        })
    };
    for line in disasm.lines().map(str::trim) {
        let (op, args) = line.split_once(' ').unwrap_or((line, ""));
        let args: Vec<&str> = args.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        match op {
            "else" => {
                let top = skipping.pop().expect("an if");
                skipping.push(!top);
                continue;
            }
            "endif" => {
                skipping.pop().expect("an if");
                continue;
            }
            _ => {}
        }
        if skipping.iter().any(|&s| s) {
            if op.starts_with("if") {
                skipping.push(true);
            }
            continue;
        }
        match op {
            "def" => {
                let i: usize = args[0][1..].parse().unwrap();
                c.insert(i, std::array::from_fn(|k| args[1 + k].parse().unwrap()));
            }
            "dp4" => return r[0][0],
            "if_ne" => {
                let a = read(&operand(args[0]), &r, &c, a0)[0];
                let b = read(&operand(args[1]), &r, &c, a0)[0];
                skipping.push(a == b);
            }
            "mova" => {
                let x = read(&operand(args[1]), &r, &c, a0)[0];
                a0 = x.round() as usize;
            }
            "mul" | "add" | "mad" | "frc" | "slt" | "sge" | "rcp" | "mov" | "lrp" => {
                let d = operand(args[0]);
                let mask: Vec<usize> = match args[0].split_once('.') {
                    Some((_, m)) => m.chars().map(|ch| "xyzw".find(ch).unwrap()).collect(),
                    None => vec![0, 1, 2, 3],
                };
                let s: Vec<[f32; 4]> = args[1..].iter().map(|a| read(&operand(a), &r, &c, a0)).collect();
                let out: [f32; 4] = std::array::from_fn(|k| match op {
                    "mul" => s[0][k] * s[1][k],
                    "add" => s[0][k] + s[1][k],
                    "mad" => s[0][k] * s[1][k] + s[2][k],
                    "frc" => s[0][k] - s[0][k].floor(),
                    "slt" => (s[0][k] < s[1][k]) as u8 as f32,
                    "sge" => (s[0][k] >= s[1][k]) as u8 as f32,
                    "rcp" => 1.0 / s[0][0],
                    "mov" => s[0][k],
                    "lrp" => s[0][k] * s[1][k] + (1.0 - s[0][k]) * s[2][k],
                    _ => unreachable!(),
                });
                assert_eq!(d.file, 'r', "{line}");
                for &k in &mask {
                    r[d.index][k] = out[k];
                }
            }
            o if o.starts_with("dcl") || o.starts_with('.') || o.starts_with("vs_") => {}
            other => panic!("the keep factor reaches an unhandled instruction {other}: {line}"),
        }
    }
    panic!("no dp4 in the shader")
}

/// ★ The TINY shaders keep a vertex at slot `w` while its object's state is the role's (1 intact,
/// 3 ruined) for `w ≢ 3 (mod 4)`, and read slot `w ≡ 3` as `2·S[w] − S[w−2]`: run over every
/// combination of the four states of a register, in both stores.
#[test]
fn the_tiny_shaders_read_component_three_as_twice_w_minus_y() {
    let data = vz_wad().parent().unwrap().to_path_buf();
    let mut checked = 0;
    for (file, low) in [("shader3.bin", false), ("shader3Low.bin", true)] {
        let store = mercs2_formats::shader3::Store::parse(std::fs::read(data.join(file)).unwrap()).unwrap();
        for (stem, target) in [
            ("PgMeshTinyVP", 1.0f32),
            ("PgMeshTinyVP_Ruin", 3.0),
            ("PgMeshTinyShadowVP", 1.0),
            ("PgMeshTinyShadowVP_Ruin", 3.0),
            ("PgMeshTinyShadowVPTex", 1.0),
            ("PgMeshTinyShadowVP_RuinTex", 3.0),
        ] {
            let id = mercs2_formats::shader3::store_id(stem, low).unwrap();
            let rec = store.records.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{stem} in {file}"));
            let text = mercs2_formats::sm3asm::disassemble(store.blob(rec)).unwrap();
            let consts = mercs2_formats::shader3::parse_ctab(store.blob(rec)).unwrap().1;
            let scale = consts.iter().find(|c| c.name == "ObjectIDScaleArray").expect("ObjectIDScaleArray");
            assert_eq!((scale.register_index, scale.register_count), (0, tiny::SCALE_REGISTERS as u16), "{stem}");
            for slot in 0..8usize {
                for combo in 0..256usize {
                    let mut states = [0.0f32; 8];
                    for k in 0..4 {
                        states[(slot / 4) * 4 + k] = ((combo >> (2 * k)) & 3) as f32 + 1.0;
                    }
                    let got = keep(&text, slot, &states);
                    let read = if slot % 4 == 3 { 2.0 * states[slot] - states[slot - 2] } else { states[slot] };
                    let want = (read == target) as u8 as f32;
                    assert_eq!(got, want, "{stem} ({file}) slot {slot} states {:?}", &states[(slot / 4) * 4..(slot / 4) * 4 + 4]);
                    checked += 1;
                }
            }
            // and the read is wrong for slot 3: drawn intact beside a hidden neighbour, it is dropped
            let mut states = [1.0, 2.0, 1.0, target];
            states[1] = if target == 1.0 { 2.0 } else { 4.0 };
            assert_eq!(keep(&text, 3, &states), 0.0, "{stem}: slot 3 shows only when slot 1 agrees");
        }
    }
    assert_eq!(checked, 2 * 6 * 8 * 256);
}

// ── retail stand-ins ────────────────────────────────────────────────────────────────────────────

struct Retail {
    /// Every TINY container by model hash.
    models: BTreeMap<u32, Vec<u8>>,
    layers: BTreeMap<u32, Vec<u8>>,
    world: World,
}

fn retail() -> &'static Retail {
    static R: OnceLock<Retail> = OnceLock::new();
    R.get_or_init(|| {
        let wad = vz_wad();
        let mut f = std::fs::File::open(&wad).unwrap();
        let size = f.metadata().unwrap().len();
        let archive = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).unwrap();
        let mut models = BTreeMap::new();
        for block in 0..archive.indx.len() {
            let dec = mercs2_formats::sges::decompress_block(&mut f, &archive.indx, block as u16).unwrap();
            let (parsed, _) = mercs2_formats::ucfx::walk_decompressed_block(&dec, "block");
            for (e, c) in parsed.entries.iter().zip(parsed.containers) {
                if e.type_hash == mercs2_formats::types::TYPE_HASH_MODEL && tm::is_tiny_container(&c) {
                    models.insert(e.name_hash, c);
                }
            }
        }
        let layers = retail_game().layer_containers().unwrap();
        let world = World::read(&layers).unwrap();
        Retail { models, layers, world }
    })
}

/// `container` with the `Transform` tail of entity `key` set to zero.
fn zero_tail(container: &[u8], key: u32) -> Vec<u8> {
    let mut roots = mercs2_formats::ucfx::parse_ucfx_tree(container).unwrap();
    for comp in roots.iter_mut().filter(|n| &n.tag == b"COMP") {
        if !comp.children[0].body.as_deref().is_some_and(|b| b.starts_with(b"Transform\0")) {
            continue;
        }
        let data = comp.children.iter_mut().find(|n| &n.tag == b"data").unwrap().body.as_mut().unwrap();
        for rec in data.chunks_exact_mut(42) {
            if u32::from_le_bytes(rec[0..4].try_into().unwrap()) == key {
                rec[36..42].fill(0);
            }
        }
    }
    mercs2_formats::ucfx::write_ucfx_tree(&roots)
}

/// One f16 step at `x`.
fn f16_step(x: f32) -> f32 {
    let e = x.abs().max(f32::MIN_POSITIVE).log2().floor().max(-14.0);
    2f32.powf(e - 10.0)
}

fn near(a: &[f32], b: &[f32], steps: f32) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= steps * f16_step(x.abs().max(y.abs())))
}

/// `rebuilt` with every bound (the model's, each node's, each group's centre, radius and box) and
/// the slot list and every `POSITION.w` taken from `retail`: what remains is everything the
/// manifest and glTF carry.
fn with_retail_derived(rebuilt: &TinyModel, retail: &TinyModel) -> TinyModel {
    let mut m = rebuilt.clone();
    m.info.bbox_min = retail.info.bbox_min;
    m.info.bbox_max = retail.info.bbox_max;
    m.slots = retail.slots.clone();
    for (n, r) in m.nodes.iter_mut().zip(&retail.nodes) {
        n.bbox_min = r.bbox_min;
        n.bbox_max = r.bbox_max;
    }
    for (s, rs) in m.sub_objects.iter_mut().zip(&retail.sub_objects) {
        for (g, rg) in s.groups.iter_mut().zip(&rs.groups) {
            g.center = rg.center;
            g.radius = rg.radius;
            g.bbox_min = rg.bbox_min;
            g.bbox_max = rg.bbox_max;
            for (v, rv) in g.vertices.iter_mut().zip(&rg.vertices) {
                v.position[3] = rv.position[3];
            }
        }
    }
    m
}

/// ★ The north star: every retail stand-in, taken apart into an `add_tiny_geometry` contribution
/// and glTF and built back, is its own container but for what the build derives — the bounds,
/// and, where the slot list holds a slot ≡ 3 (mod 4), the slots — and its layer is its own layer
/// but for the placement's `Transform` tail.
#[test]
fn every_retail_stand_in_decomposes_and_rebuilds() {
    let r = retail();
    let dir = scratch("tiny_decompose");
    let mut failures = Vec::new();
    let (mut total, mut same_slots, mut repeats, mut max_steps) = (0usize, 0usize, 0usize, 0.0f32);
    for &(layer, key, cell) in &r.world.stand_ins {
        total += 1;
        let at = format!("layer 0x{layer:08X} stand-in 0x{key:08X}");
        let (row, col) = cell.unwrap_or_else(|| panic!("{at} is outside the grid"));
        let layer_c = &r.layers[&layer];
        let records = read_layer_records(layer_c).unwrap();
        let model_hash = records.models.iter().find(|m| m.0 == key).unwrap().1;
        let Some(container) = r.models.get(&model_hash) else {
            failures.push(format!("{at}: its model 0x{model_hash:08X} is no TINY container"));
            continue;
        };
        let retail_model = TinyModel::decode(container).unwrap();
        let d = match tiny::decompose(&retail_model, row, col, &|_| None) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("{at}: decompose: {e}"));
                continue;
            }
        };
        let glb = dir.join(format!("{key:08x}.glb"));
        std::fs::write(&glb, &d.glb).unwrap();
        let source = tiny::read_source(&glb).unwrap();
        let plan = match tiny::plan_slots(&d.objects, &|g| r.world.placed.contains_key(&g) || g == key) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{at}: slots: {e}"));
                continue;
            }
        };
        let rebuilt = match tiny::build_model(&source, row, col, &plan, &tiny::resolve_materials(&source).unwrap()) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{at}: build: {e}"));
                continue;
            }
        };
        // everything the manifest and glTF carry comes back byte for byte
        if with_retail_derived(&rebuilt, &retail_model).encode() != *container {
            failures.push(format!("{at}: differs beyond the bounds and the slots"));
            continue;
        }
        // the slots: each object keeps its GUID; its slot moves only off a slot ≡ 3
        let objects: Vec<u32> = plan.slot_of.iter().map(|&s| rebuilt.slots[s as usize]).collect();
        assert_eq!(objects, retail_model.slots, "{at}");
        if rebuilt.slots.windows(2).any(|w| w[0] == w[1]) {
            repeats += 1;
        }
        if rebuilt.slots == retail_model.slots {
            same_slots += 1;
        } else if retail_model.slots.len() <= 3 {
            failures.push(format!("{at}: the slots moved with no slot ≡ 3 to leave"));
        }
        for (s, rs) in rebuilt.sub_objects.iter().zip(&retail_model.sub_objects) {
            for (g, rg) in s.groups.iter().zip(&rs.groups) {
                for (v, rv) in g.vertices.iter().zip(&rg.vertices) {
                    let w = unhalf(rv.position[3]) as usize;
                    assert_eq!(unhalf(v.position[3]) as u16, plan.slot_of[w], "{at}");
                }
                // the bounds: the retail ones come from the positions before they were halved
                let pairs = [(&g.bbox_min[..], &rg.bbox_min[..]), (&g.bbox_max[..], &rg.bbox_max[..])];
                for (a, b) in pairs {
                    if !near(a, b, 1.0) {
                        failures.push(format!("{at}: group bounds {a:?} vs {b:?}"));
                    }
                    for (x, y) in a.iter().zip(b) {
                        max_steps = max_steps.max((x - y).abs() / f16_step(x.abs().max(y.abs())));
                    }
                }
            }
        }
        for (n, rn) in rebuilt.nodes.iter().zip(&retail_model.nodes) {
            if !near(&n.bbox_min, &rn.bbox_min, 1.0) || !near(&n.bbox_max, &rn.bbox_max, 1.0) {
                failures.push(format!("{at}: node bounds {:?}..{:?} vs {:?}..{:?}", n.bbox_min, n.bbox_max, rn.bbox_min, rn.bbox_max));
            }
        }
        if !near(&rebuilt.info.bbox_min, &retail_model.info.bbox_min, 1.0) || !near(&rebuilt.info.bbox_max, &retail_model.info.bbox_max, 1.0) {
            failures.push(format!("{at}: model bounds differ by more than one f16 step"));
        }
        // the placement: out of the layer and back
        let base = remove_entity(layer_c, key).unwrap();
        let back = tiny::layer_with_placement(&base, key, row, col, model_hash).unwrap();
        if back != zero_tail(layer_c, key) {
            failures.push(format!("{at}: the layer differs beyond the placement's Transform tail"));
        }
    }
    eprintln!(
        "{total} stand-ins: {same_slots} keep their slot list, {} move objects off slots ≡ 3 ({repeats} with a \
         repeated GUID); bounds within {max_steps:.3} f16 steps",
        total - same_slots
    );
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    assert_eq!(total, 1208);
}

// ── a stand-in built and read back ──────────────────────────────────────────────────────────────

const LAYER: &str = "vz_state_mar_city_pristine";

/// A cell of [`LAYER`] with no stand-in that holds at least two of the layer's placements, and two
/// of them.
fn free_cell() -> (u32, u32, [u32; 2]) {
    let r = retail();
    let layer = pandemic_hash_m2(LAYER);
    let mut by_cell: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (&key, at) in &r.world.placed {
        for (l, p) in at {
            if *l == layer {
                if let Some(c) = tiny::cell_of(*p) {
                    by_cell.entry(c).or_default().push(key);
                }
            }
        }
    }
    let taken: Vec<(u32, u32)> = r.world.stand_ins.iter().filter(|s| s.0 == layer).filter_map(|s| s.2).collect();
    let ((row, col), keys) = by_cell
        .into_iter()
        .find(|(c, k)| k.len() >= 2 && !taken.contains(c))
        .expect("a free cell of the layer with two placements");
    (row, col, [keys[0], keys[1]])
}

/// A key no placement of the game uses.
fn free_key() -> u32 {
    (0x00F0_0000u32..).find(|k| !retail().world.placed.contains_key(k)).unwrap()
}

/// A texture the game ships: the first retail stand-in material's.
fn a_texture() -> u32 {
    let m = TinyModel::decode(retail().models.values().next().unwrap()).unwrap();
    m.materials[0].textures[0]
}

/// A `.gltf` whose primitives are `(role, slot)` triangles near the origin, drawing one opaque
/// material with `texture`.
fn stand_in_gltf(dir: &Path, prims: &[(&str, u8)], texture: u32) {
    let mut bin: Vec<u8> = Vec::new();
    for c in [0.0f32, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 4.0, 2.0] {
        bin.extend_from_slice(&c.to_le_bytes());
    }
    for _ in 0..3 {
        for c in [0.0f32, 1.0, 0.0] {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    for c in [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0] {
        bin.extend_from_slice(&c.to_le_bytes());
    }
    let mut views = vec![
        r#"{"buffer":0,"byteOffset":0,"byteLength":36}"#.to_string(),
        r#"{"buffer":0,"byteOffset":36,"byteLength":36}"#.to_string(),
        r#"{"buffer":0,"byteOffset":72,"byteLength":24}"#.to_string(),
    ];
    let mut accessors = vec![
        r#"{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[4,4,2]}"#.to_string(),
        r#"{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}"#.to_string(),
        r#"{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}"#.to_string(),
    ];
    let mut out = Vec::new();
    for (role, slot) in prims {
        let at = bin.len();
        bin.extend_from_slice(&[*slot, *slot, *slot, 0]);
        views.push(format!(r#"{{"buffer":0,"byteOffset":{at},"byteLength":3}}"#));
        accessors.push(format!(r#"{{"bufferView":{},"componentType":5121,"count":3,"type":"SCALAR"}}"#, views.len() - 1));
        out.push(format!(
            r#"{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2,"_TINY_SLOT":{}}},"mode":4,"material":0,"extras":{{"tiny_role":"{role}"}}}}"#,
            accessors.len() - 1
        ));
    }
    std::fs::write(dir.join("t.bin"), &bin).unwrap();
    std::fs::write(
        dir.join("t.gltf"),
        format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
"meshes":[{{"primitives":[{}]}}],"materials":[{{"alphaMode":"OPAQUE","extras":{{"texture":"0x{texture:08X}","pixel_shader":"PgDiffFP"}}}}],
"buffers":[{{"uri":"t.bin","byteLength":{}}}],"bufferViews":[{}],"accessors":[{}]}}"#,
            out.join(","),
            bin.len(),
            views.join(","),
            accessors.join(",")
        ),
    )
    .unwrap();
}

fn contribution(row: u32, col: u32, key: u32, objects: &[String]) -> String {
    let objects = objects.iter().map(|o| format!("\"{o}\"")).collect::<Vec<_>>().join(", ");
    format!(
        "  - kind: add_tiny_geometry\n    layer: {LAYER}\n    cell: {{ row: {row}, col: {col} }}\n    key: {key}\n    \
         objects: [{objects}]\n    model: src/t.gltf\n"
    )
}

/// ★ A stand-in for a free cell of a retail layer builds, and the built WAD holds its model (the
/// slot list, the roles, the shaders, the material) and its placement in the layer.
#[test]
fn an_add_tiny_geometry_build_decodes_back() {
    let (row, col, objs) = free_cell();
    let key = free_key();
    let texture = a_texture();
    let dir = scratch("tiny_build");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    stand_in_gltf(&dir.join("src"), &[("intact", 0), ("intact", 1), ("ruined", 1)], texture);
    let objects = [format!("0x{:08X}", objs[1]), format!("0x{:08X}", objs[0])];
    let s = shipment(&dir, &contribution(row, col, key, &objects));
    let mut game = retail_game();
    let report = build::build(&s, Some(&mut game), None, None, None, None).expect("the stand-in builds");

    let on_disk = std::fs::read(report.wad.as_ref().expect("a WAD")).unwrap();
    let contents = mercs2_formats::patch_wad::read_patch_wad(&on_disk).unwrap();
    let model_name = tiny::model_name(LAYER, row, col, key);
    let model_hash = pandemic_hash_m2(&model_name);
    let layer_hash = pandemic_hash_m2(LAYER);
    let (mut model, mut layer) = (None, None);
    for block in &contents.blocks {
        let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).unwrap();
        if let Some((_, c)) = mercs2_formats::placement_build::find_entry(&dec, model_hash, mercs2_formats::types::TYPE_HASH_MODEL) {
            model = Some(TinyModel::decode(c).unwrap());
        }
        if let Some((_, c)) = mercs2_formats::placement_build::find_entry(&dec, layer_hash, mercs2_formats::types::TYPE_HASH_LAYER) {
            layer = Some(read_layer_records(c).unwrap());
        }
    }
    let m = model.expect("the model block");
    let mut sorted = objs;
    sorted.sort();
    assert_eq!(m.slots, sorted.to_vec());
    assert_eq!(m.nodes.iter().map(|n| n.name_hash).collect::<Vec<_>>(), [pandemic_hash_m2("pristine"), pandemic_hash_m2("ruin")]);
    let shaders: Vec<(u32, u32)> =
        m.sub_objects.iter().flat_map(|s| s.groups.iter().map(|g| (g.vertex_shader, g.shadow_vertex_shader))).collect();
    assert_eq!(
        shaders,
        [
            (pandemic_hash_m2("PgMeshTinyVP"), pandemic_hash_m2("PgMeshTinyShadowVP")),
            (pandemic_hash_m2("PgMeshTinyVP"), pandemic_hash_m2("PgMeshTinyShadowVP")),
            (pandemic_hash_m2("PgMeshTinyVP_Ruin"), pandemic_hash_m2("PgMeshTinyShadowVP_Ruin")),
        ]
    );
    // objects[0] is the larger GUID: slot 1; objects[1] slot 0
    let w = |s: usize, g: usize| unhalf(m.sub_objects[s].groups[g].vertices[0].position[3]);
    assert_eq!((w(0, 0), w(0, 1), w(1, 0)), (1.0, 0.0, 0.0));
    assert_eq!(m.materials[0].textures, vec![texture]);
    assert_eq!(m.materials[0].pixel_shader, pandemic_hash_m2("PgDiffFP"));
    assert_eq!(m.materials[0].name_hash, pandemic_hash_m2(&tm::material_name(row, col, false)));

    let l = layer.expect("the layer overlay");
    let name = l.names.iter().find(|n| n.0 == key).expect("the placement's Name").1.clone();
    assert_eq!(name, format!("{} 0x{key:08x}", tiny::placement_name(row, col)));
    assert_eq!(l.models.iter().find(|m| m.0 == key).map(|m| m.1), Some(model_hash));
    let t = l.transforms.iter().find(|t| t.0 == key).expect("the placement's Transform");
    assert_eq!(t.1, tiny::cell_centre(row, col));
    assert_eq!(t.2, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(l.flags.iter().find(|f| f.0 == key).map(|f| f.1), Some(FLGS_TINY_GEOMETRY_OBJECT));
}

// ── the rules that need the game ────────────────────────────────────────────────────────────────

fn game_codes(contributions: &str) -> Vec<&'static str> {
    let m = mercs2_quartermaster::from_str(
        &format!("format: 2\nshipment: {{ name: s, version: 1.0.0, target: retail }}\ncontributions:\n{contributions}"),
        mercs2_quartermaster::Format::Yaml,
    )
    .unwrap();
    lint::game_checks(&m, &mut retail_game()).iter().map(|d| d.rule.code).collect()
}

#[test]
fn the_game_rules_are_quiet_on_a_sound_stand_in() {
    let (row, col, objs) = free_cell();
    let codes = game_codes(&contribution(row, col, free_key(), &[format!("0x{:08X}", objs[0])]));
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn m0245_fires_on_an_object_outside_the_cell_or_not_placed() {
    let (row, col, objs) = free_cell();
    let codes = game_codes(&contribution(row, (col + 1) % tiny::GRID, free_key(), &[format!("0x{:08X}", objs[0])]));
    assert!(codes.contains(&"M0245"), "{codes:?}");
    let codes = game_codes(&contribution(row, col, free_key(), &["no_such_placement".into()]));
    assert_eq!(codes, vec!["M0245"]);
}

#[test]
fn m0247_and_m0250_fire_on_a_retail_stand_ins_cell_and_key() {
    let r = retail();
    let layer = pandemic_hash_m2(LAYER);
    let &(_, key, cell) = r.world.stand_ins.iter().find(|s| s.0 == layer).expect("a stand-in of the layer");
    let (row, col) = cell.unwrap();
    let records = read_layer_records(&r.layers[&layer]).unwrap();
    let model = TinyModel::decode(&r.models[&records.models.iter().find(|m| m.0 == key).unwrap().1]).unwrap();
    let objects: Vec<String> = model.slots.iter().map(|g| format!("0x{g:08X}")).collect();
    let codes = game_codes(&contribution(row, col, key, &objects));
    assert!(codes.contains(&"M0247") && codes.contains(&"M0250"), "{codes:?}");
}

#[test]
fn m0241_fires_on_a_name_and_a_guid_of_one_object() {
    let r = retail();
    let layer = pandemic_hash_m2(LAYER);
    let (row, col, _) = free_cell();
    let (name, guid) = r.world.names[&layer]
        .iter()
        .filter(|(_, k)| k.len() == 1)
        .map(|(n, k)| (n.clone(), k[0]))
        .find(|(_, g)| r.world.placed[g].iter().any(|(l, p)| *l == layer && tiny::cell_of(*p) == Some((row, col))))
        .or_else(|| r.world.names[&layer].iter().filter(|(_, k)| k.len() == 1).map(|(n, k)| (n.clone(), k[0])).next())
        .unwrap();
    let codes = game_codes(&contribution(row, col, free_key(), &[name, format!("0x{guid:08X}")]));
    assert!(codes.contains(&"M0241"), "{codes:?}");
}

#[test]
fn m0246_fires_past_1400_stand_ins() {
    let r = retail();
    assert_eq!(r.world.stand_ins.len(), 1208);
    let (row, col, objs) = free_cell();
    let many = |n: u32| -> String {
        (0..n).map(|k| contribution(row, col, 0x00F1_0000 + k, &[format!("0x{:08X}", objs[0])])).collect()
    };
    assert!(game_codes(&many(193)).contains(&"M0246"));
    assert!(!game_codes(&many(192)).contains(&"M0246"));
}

/// `add_model` onto a TINY host refuses a slot ≡ 3 (mod 4) and takes the slot below it.
#[test]
fn m0248_refuses_a_tiny_host_slot_three_mod_four() {
    let r = retail();
    // a retail stand-in whose list holds four objects or more, with an intact first group
    let (&hash, _) = r
        .models
        .iter()
        .find(|(_, c)| {
            let m = TinyModel::decode(c).unwrap();
            m.slots.len() >= 4 && m.nodes[0].name_hash == pandemic_hash_m2("pristine")
        })
        .unwrap();
    let mut game = retail_game();
    for (slot, ok) in [(3u8, false), (2u8, true)] {
        let dir = scratch(&format!("tiny_host_{slot}"));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        stand_in_gltf(&dir.join("src"), &[("intact", slot)], a_texture());
        let s = shipment(
            &dir,
            &format!("  - kind: add_model\n    name: qm_test_tiny_host\n    model: src/t.gltf\n    donor: \"0x{hash:08X}\"\n    group: 0\n"),
        );
        let result = build::build(&s, Some(&mut game), None, None, None, None);
        if ok {
            result.unwrap_or_else(|e| panic!("slot {slot}: {e}"));
        } else {
            let e = result.unwrap_err().to_string();
            assert!(e.contains("M0248"), "{e}");
        }
    }
}
