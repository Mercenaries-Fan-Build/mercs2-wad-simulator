//! Retail gates for [`mercs2_formats::scrub`]: every ground-cover container in `vz.wad`, the frame its
//! placement puts it in, and the PMC HQ pyramid edit carrying its ground cover along.
//!
//! Needs the game: set `MERCS2_GAME_DIR`. Without it every test prints `SKIPPING` and returns.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::OnceLock;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::placement::{load_scrub_placements, load_terrain_tiles, ScrubPlacement};
use mercs2_formats::scrub::{ScrubPack, TYPE_HASH};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::terrainmesh::{self, sphere_of, TerrainCell};
use mercs2_formats::types::TYPE_ID_TERRAIN_MESH;
use mercs2_formats::ucfx::walk_decompressed_block;

/// ASET `type_id` of `scrub`.
const TYPE_ID_SCRUB: u32 = 12;
const HQ_CELL: u32 = 0xA241_BC0C;

fn pyramid(x: f32, z: f32) -> f32 {
    let d = (x + 120.0).abs().max((z + 120.0).abs());
    40.0 * (1.0 - d / 60.0).max(0.0)
}

struct Retail {
    /// Every scrub container by hash, with its block path.
    scrubs: BTreeMap<u32, (String, Vec<u8>)>,
    /// Every terrain cell container by hash.
    cells: HashMap<u32, Vec<u8>>,
    /// Every terrain tile's world position, by terrainmesh hash.
    tiles: HashMap<u32, [f32; 3]>,
    /// Every `ScrubObject` placement, with the layer it is in.
    placements: Vec<(String, ScrubPlacement)>,
}

/// Read once per test binary.
fn retail() -> Option<&'static Retail> {
    static RETAIL: OnceLock<Option<Retail>> = OnceLock::new();
    RETAIL
        .get_or_init(|| {
            let wad = mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))?;
            let mut f = std::fs::File::open(&wad).expect("open vz.wad");
            let size = f.metadata().expect("stat vz.wad").len();
            let archive = load_ffcs_archive(&mut f, size).expect("read FFCS tables");
            let mut wanted: BTreeMap<u16, Vec<(u32, u32)>> = BTreeMap::new();
            for row in archive.aset.iter() {
                let th = match row.type_id {
                    TYPE_ID_SCRUB => TYPE_HASH,
                    TYPE_ID_TERRAIN_MESH => terrainmesh::TYPE_HASH,
                    _ => continue,
                };
                wanted
                    .entry(row.block_index())
                    .or_default()
                    .push((row.asset_hash, th));
            }
            let mut r = Retail {
                scrubs: BTreeMap::new(),
                cells: HashMap::new(),
                tiles: HashMap::new(),
                placements: Vec::new(),
            };
            for (&block, rows) in &wanted {
                let dec = decompress_block(&mut f, &archive.indx, block).expect("decompress");
                let (parsed, issues) = walk_decompressed_block(&dec, "block");
                assert!(
                    issues.is_empty(),
                    "block {block}: {:?}",
                    issues.iter().map(|i| &i.detail).collect::<Vec<_>>()
                );
                let path = archive
                    .paths
                    .get(block as usize)
                    .cloned()
                    .unwrap_or_default();
                for &(hash, th) in rows {
                    let i = parsed
                        .entries
                        .iter()
                        .position(|e| e.name_hash == hash && e.type_hash == th)
                        .unwrap_or_else(|| panic!("{hash:#010X} not in its block {path}"));
                    let c = parsed.containers[i].clone();
                    if th == TYPE_HASH {
                        r.scrubs.insert(hash, (path.clone(), c));
                    } else {
                        r.cells.insert(hash, c);
                    }
                }
            }
            for (i, path) in archive.paths.iter().enumerate() {
                let p = path.to_lowercase();
                if !(p.contains("layers_static") || p.contains("vz_state")) {
                    continue;
                }
                let dec =
                    decompress_block(&mut f, &archive.indx, i as u16).expect("decompress layer");
                for t in load_terrain_tiles(&dec) {
                    r.tiles.insert(t.terrainmesh_hash, t.pos);
                }
                let placed = load_scrub_placements(&dec).unwrap_or_else(|e| panic!("{path}: {e}"));
                r.placements
                    .extend(placed.into_iter().map(|s| (path.clone(), s)));
            }
            Some(r)
        })
        .as_ref()
}

macro_rules! retail_or_skip {
    () => {
        match retail() {
            Some(r) => r,
            None => {
                eprintln!(
                    "SKIPPING {}: no vz.wad (set MERCS2_GAME_DIR)",
                    module_path!()
                );
                return;
            }
        }
    };
}

/// The terrain cell a scrub placement covers: the tile at the very same world position.
fn cell_at(r: &Retail, pos: [f32; 3]) -> Option<u32> {
    r.tiles.iter().find(|(_, p)| **p == pos).map(|(h, _)| *h)
}

#[test]
fn every_retail_scrub_decodes_and_re_encodes_byte_identically() {
    let r = retail_or_skip!();
    let (mut empty, mut patches, mut instances) = (0, 0usize, 0usize);
    let (mut worst_box, mut worst_centre, mut worst_radius) = (0f32, 0f32, 0f32);
    for (hash, (path, bytes)) in &r.scrubs {
        let pack =
            ScrubPack::decode(bytes).unwrap_or_else(|e| panic!("{hash:#010X} ({path}): {e}"));
        let again = pack.encode().unwrap();
        assert!(again == *bytes, "{hash:#010X}: re-encode differs");
        assert_eq!(
            ScrubPack::decode(&again).unwrap(),
            pack,
            "{hash:#010X}: typed decode not reflexive"
        );
        let ScrubPack::Pack(s) = &pack else {
            empty += 1;
            continue;
        };
        instances += s.instances.len();
        for (p, patch) in s.patches.iter().enumerate() {
            patches += 1;
            let bounds = s.patch_bounds(p).unwrap();
            for a in 0..3 {
                worst_box = worst_box
                    .max((bounds.min[a] - patch.aabb.min[a]).abs())
                    .max((bounds.max[a] - patch.aabb.max[a]).abs());
            }
            let (c, rad) = sphere_of(&patch.aabb);
            for (a, ca) in c.iter().enumerate() {
                worst_centre = worst_centre.max((ca - patch.sphere_centre[a]).abs());
            }
            worst_radius = worst_radius.max((rad - patch.sphere_radius).abs());
        }
    }
    eprintln!(
        "{} scrub containers re-encode byte-identically ({empty} empty); {patches} patches, {instances} instances",
        r.scrubs.len()
    );
    eprintln!(
        "patch box vs transformed meshes: max Δ {worst_box} m; sphere vs sphere_of(box): centre Δ {worst_centre:e}, radius Δ {worst_radius:e}"
    );
    assert_eq!(r.scrubs.len(), 1026);
    assert!(
        worst_centre < 1e-4 && worst_radius < 1e-3,
        "patch spheres are not the box's centre + half-diagonal"
    );
    assert!(
        worst_box < 0.05,
        "patch boxes are not the union of the transformed meshes"
    );
}

/// A `ScrubObject` sits at exactly its cell's `TerrainObject` position with no rotation, and its instances
/// stand on that cell's render surface, cell-local — the frame [`ScrubPack::follow_ground`] relies on.
#[test]
fn every_placed_scrub_sits_on_the_cell_at_its_placement() {
    let r = retail_or_skip!();
    let mut placed: HashMap<u32, Vec<u32>> = HashMap::new();
    let (mut offsets, mut outside) = (Vec::new(), 0usize);
    let mut absent = Vec::new();
    let mut checked: HashMap<u32, u32> = HashMap::new();
    for (layer, s) in &r.placements {
        assert_eq!(
            s.quat,
            [0.0, 0.0, 0.0, 1.0],
            "{layer}: ScrubObject {:08X} is rotated",
            s.key
        );
        let cell = cell_at(r, s.pos).unwrap_or_else(|| {
            panic!(
                "{layer}: scrub {:08X} at {:?} is not at a cell centre",
                s.scrub_hash, s.pos
            )
        });
        placed.entry(s.scrub_hash).or_default().push(cell);
        if let Some(prev) = checked.insert(s.scrub_hash, cell) {
            assert_eq!(
                prev, cell,
                "scrub {:08X} is placed over two cells",
                s.scrub_hash
            );
            continue;
        }
        let Some((_, bytes)) = r.scrubs.get(&s.scrub_hash) else {
            absent.push(format!("{:08X} ({layer})", s.scrub_hash));
            continue;
        };
        let pack = ScrubPack::decode(bytes).unwrap();
        let o = pack
            .ground_offsets(&TerrainCell::decode(&r.cells[&cell]).unwrap())
            .unwrap_or_else(|e| panic!("scrub {:08X} on {cell:#010X}: {e}", s.scrub_hash));
        for v in o {
            match v {
                Some(v) => offsets.push(v),
                None => outside += 1,
            }
        }
    }
    let unplaced: Vec<String> = r
        .scrubs
        .iter()
        .filter(|(h, (_, b))| {
            !placed.contains_key(h) && ScrubPack::decode(b).unwrap() != ScrubPack::Empty
        })
        .map(|(h, (p, _))| format!("{h:08X} ({p})"))
        .collect();
    offsets.sort_by(|a, b| a.total_cmp(b));
    let q = |f: f64| offsets[((offsets.len() - 1) as f64 * f) as usize];
    let within = offsets.iter().filter(|o| o.abs() <= 0.25).count();
    eprintln!(
        "{} ScrubObject placements over {} distinct scrubs present in vz.wad; {} instances on their cell's surface, {outside} just past its edge; \
         height above it: p01 {:.3} p05 {:.3} median {:.3} p95 {:.3} p99 {:.3}; {within} ({:.1}%) within 25 cm",
        r.placements.len(),
        checked.len() - absent.len(),
        offsets.len(),
        q(0.01),
        q(0.05),
        q(0.5),
        q(0.95),
        q(0.99),
        100.0 * within as f64 / offsets.len() as f64
    );
    eprintln!("non-empty containers with no ScrubObject placement: {unplaced:?}");
    eprintln!(
        "ScrubObject records naming a scrub vz.wad does not carry: {} {absent:?}",
        absent.len()
    );
    assert!(
        q(0.5).abs() < 0.05,
        "instances do not sit on their cell's surface"
    );
    assert!(unplaced.is_empty(), "unplaced ground cover: {unplaced:?}");
}

/// The pyramid on the PMC HQ cell carries every ground-cover container placed over that cell.
#[test]
fn hq_ground_cover_follows_the_pyramid() {
    let r = retail_or_skip!();
    let before = TerrainCell::decode(&r.cells[&HQ_CELL]).unwrap();
    let mut after = before.clone();
    after.displace(pyramid).unwrap();
    let hq_pos = r.tiles[&HQ_CELL];
    let mut hashes: Vec<u32> = r
        .placements
        .iter()
        .filter(|(_, s)| s.pos == hq_pos)
        .map(|(_, s)| s.scrub_hash)
        .collect();
    hashes.sort_unstable();
    hashes.dedup();
    let (mut moved, mut rebounded, mut outside) = (0usize, 0usize, 0usize);
    let mut drift = Vec::new();
    for hash in &hashes {
        let original = ScrubPack::decode(&r.scrubs[hash].1).unwrap();
        let mut pack = original.clone();
        let rep = pack
            .follow_ground(&before, &after)
            .unwrap_or_else(|e| panic!("{hash:#010X}: {e}"));
        moved += rep.moved_instances;
        rebounded += rep.rebounded_patches;
        outside += rep.outside_footprint;
        // Height above the (re-selected) surface before vs after. Differences come from f16 rounding at
        // the new height and from overlapping draw groups whose triangulations of the pyramid differ.
        let (old, new) = (
            original.ground_offsets(&before).unwrap(),
            pack.ground_offsets(&after).unwrap(),
        );
        for (o, n) in old.iter().zip(&new) {
            if let (Some(o), Some(n)) = (o, n) {
                drift.push((o - n).abs());
            }
        }
        let bytes = pack.encode().unwrap();
        assert_eq!(
            ScrubPack::decode(&bytes).unwrap(),
            pack,
            "{hash:#010X}: edited container re-parses"
        );
        if let ScrubPack::Pack(s) = &pack {
            for p in 0..s.patches.len() {
                let b = s.patch_bounds(p).unwrap();
                for a in 0..3 {
                    assert!(
                        s.patches[p].aabb.min[a] <= b.min[a] + 0.05
                            && s.patches[p].aabb.max[a] >= b.max[a] - 0.05,
                        "{hash:#010X} patch {p}: box does not cover its instances"
                    );
                }
            }
        }
    }
    drift.sort_by(|a, b| a.total_cmp(b));
    let q = |f: f64| drift[((drift.len() - 1) as f64 * f) as usize];
    eprintln!(
        "HQ cell: {} ground-cover containers placed over it; {moved} instances moved, {rebounded} patches re-bounded, \
         {outside} past the edge left alone; |Δ height above ground|: median {:.4} p99 {:.3} max {:.3} m",
        hashes.len(),
        q(0.5),
        q(0.99),
        q(1.0)
    );
    assert!(moved > 0, "nothing in the pyramid footprint moved");
    assert!(
        q(0.5) <= 0.01,
        "instances did not keep their height above the ground"
    );
}
