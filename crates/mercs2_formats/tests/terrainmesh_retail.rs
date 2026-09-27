//! Retail gates for [`mercs2_formats::terrainmesh`]: every one of the 400 hi-res terrain cells in
//! `vz.wad`, and an edit of the cell under the PMC HQ.
//!
//! Needs the game: set `MERCS2_GAME_DIR` (install root, its `data` folder, or `vz.wad` itself). Without
//! it every test here prints `SKIPPING` and returns.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::placement::{load_placements, load_terrain_tiles};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::terrainmesh::{destrip, sphere_of, stripify, TerrainCell, TYPE_HASH};
use mercs2_formats::types::TYPE_ID_TERRAIN_MESH;
use mercs2_formats::ucfx::walk_decompressed_block;

/// The cell under the PMC HQ (2647, 10, -951): `Terrain_r07_c16`, block `c31411_P000_Q3`.
const HQ_CELL: u32 = 0xA241_BC0C;
/// Pyramid: apex 40 m above the ground, square base of half-width 60 m, centred at cell-local (-120, -120).
const APEX: [f32; 2] = [-120.0, -120.0];
const HALF_WIDTH: f32 = 60.0;
const HEIGHT: f32 = 40.0;

fn pyramid(x: f32, z: f32) -> f32 {
    let d = (x - APEX[0]).abs().max((z - APEX[1]).abs());
    HEIGHT * (1.0 - d / HALF_WIDTH).max(0.0)
}

fn vz_wad() -> Option<PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
}

struct Retail {
    /// `(cell hash, block path, container bytes)` for every ASET `terrainmesh` row.
    cells: Vec<(u32, String, Vec<u8>)>,
    /// Every placement block: `(block path, decompressed bytes)`.
    layers: Vec<(String, Vec<u8>)>,
}

/// Read once per test binary and shared: 400 cell blocks plus every placement layer.
fn retail() -> Option<&'static Retail> {
    static RETAIL: OnceLock<Option<Retail>> = OnceLock::new();
    RETAIL
        .get_or_init(|| {
            let wad = vz_wad()?;
            let mut f = std::fs::File::open(&wad).expect("open vz.wad");
            let size = f.metadata().expect("stat vz.wad").len();
            let archive = load_ffcs_archive(&mut f, size).expect("read FFCS tables");
            let mut cells = Vec::new();
            for row in archive
                .aset
                .iter()
                .filter(|r| r.type_id == TYPE_ID_TERRAIN_MESH)
            {
                let block = row.block_index();
                let dec =
                    decompress_block(&mut f, &archive.indx, block).expect("decompress cell block");
                let (parsed, issues) = walk_decompressed_block(&dec, "cell");
                assert!(
                    issues.is_empty(),
                    "block {block}: {:?}",
                    issues.iter().map(|i| &i.detail).collect::<Vec<_>>()
                );
                let i = parsed
                    .entries
                    .iter()
                    .position(|e| e.name_hash == row.asset_hash && e.type_hash == TYPE_HASH)
                    .unwrap_or_else(|| {
                        panic!("cell {:#010X} is not in its block {block}", row.asset_hash)
                    });
                let path = archive
                    .paths
                    .get(block as usize)
                    .cloned()
                    .unwrap_or_default();
                cells.push((row.asset_hash, path, parsed.containers[i].clone()));
            }
            let mut layers = Vec::new();
            for (i, path) in archive.paths.iter().enumerate() {
                let p = path.to_lowercase();
                if p.contains("layers_static") || p.contains("vz_state") {
                    let dec = decompress_block(&mut f, &archive.indx, i as u16)
                        .expect("decompress layer");
                    layers.push((path.clone(), dec));
                }
            }
            Some(Retail { cells, layers })
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

/// The ground decl: `POSITION·D3DCOLOR·NORMAL`, 20 bytes.
fn is_ground(p: &mercs2_formats::terrainmesh::Prmg) -> bool {
    let d: Vec<(u16, u8, u8)> = p.decl.iter().map(|e| (e.offset, e.ty, e.usage)).collect();
    d == [(0, 16, 0), (8, 4, 10), (12, 16, 3)]
}

fn hq(r: &Retail) -> TerrainCell {
    let (_, _, bytes) = r
        .cells
        .iter()
        .find(|c| c.0 == HQ_CELL)
        .expect("the HQ cell is in vz.wad");
    TerrainCell::decode(bytes).expect("decode the HQ cell")
}

/// World centre of each terrainmesh, from the `TerrainObject` placements.
fn cell_centres(r: &Retail) -> HashMap<u32, [f32; 3]> {
    let mut out = HashMap::new();
    for (_, block) in &r.layers {
        for t in load_terrain_tiles(block) {
            assert_eq!(
                t.quat,
                [0.0, 0.0, 0.0, 1.0],
                "terrain tile {:#010X} is rotated",
                t.terrainmesh_hash
            );
            out.insert(t.terrainmesh_hash, t.pos);
        }
    }
    out
}

#[test]
fn every_retail_cell_decodes_and_re_encodes_byte_identically() {
    let r = retail_or_skip!();
    assert_eq!(r.cells.len(), 400, "ASET terrainmesh rows");
    let mut decls: HashMap<Vec<(u16, u8, u8)>, usize> = HashMap::new();
    let (mut patches, mut worst_centre, mut worst_radius) = (0usize, 0f32, 0f32);
    for (hash, path, bytes) in &r.cells {
        let cell =
            TerrainCell::decode(bytes).unwrap_or_else(|e| panic!("{hash:#010X} ({path}): {e}"));
        let again = cell
            .encode()
            .unwrap_or_else(|e| panic!("{hash:#010X}: encode: {e}"));
        assert!(
            again == *bytes,
            "{hash:#010X} ({path}): re-encode differs ({} vs {} bytes)",
            again.len(),
            bytes.len()
        );
        // Typed equality too: a float field holding a NaN bit pattern would round-trip its bytes but
        // compare unequal to itself.
        assert!(
            TerrainCell::decode(&again).unwrap() == cell,
            "{hash:#010X}: typed decode is not reflexive"
        );

        assert_eq!(cell.geoms.len(), 16, "{hash:#010X}: patch count");
        assert_eq!(
            cell.phy2.prefix,
            [
                0x39,
                *hash,
                2,
                1,
                1,
                0,
                0,
                cell.phy2.prefix[7],
                cell.phy2.prefix[8],
                0,
                0,
                0
            ],
            "{hash:#010X}: PHY2 prefix"
        );
        // The root box is the union of the offset patch boxes, exactly.
        let mut u = mercs2_formats::terrainmesh::Aabb {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        };
        let mut offsets: Vec<(i32, i32)> = cell
            .geoms
            .iter()
            .map(|g| (g.poff[0] as i32, g.poff[2] as i32))
            .collect();
        offsets.sort_unstable();
        let grid = [-150, -50, 50, 150];
        let want: Vec<(i32, i32)> = grid
            .iter()
            .flat_map(|&x| grid.iter().map(move |&z| (x, z)))
            .collect();
        assert_eq!(offsets, want, "{hash:#010X}: patch offsets");
        for g in &cell.geoms {
            patches += 1;
            assert_eq!(
                g.prmgs.iter().filter(|p| is_ground(p)).count(),
                1,
                "{hash:#010X}: a patch without exactly one ground draw group"
            );
            let (c, rad) = sphere_of(&g.aabb);
            for (k, ck) in c.iter().enumerate() {
                worst_centre = worst_centre.max((ck - g.sphere_centre[k]).abs());
                u.min[k] = u.min[k].min(g.aabb.min[k] + g.poff[k]);
                u.max[k] = u.max[k].max(g.aabb.max[k] + g.poff[k]);
            }
            worst_radius = worst_radius.max((rad - g.sphere_radius).abs());
            assert_eq!(g.poff[1], 0.0);
            for p in &g.prmgs {
                for (i, pg) in p.pass_groups.iter().enumerate() {
                    assert_eq!(
                        (pg.alt_draw_count, pg.index as usize),
                        (1, i),
                        "{hash:#010X}: pass group"
                    );
                }
                *decls
                    .entry(p.decl.iter().map(|e| (e.offset, e.ty, e.usage)).collect())
                    .or_default() += 1;
            }
        }
        assert_eq!(
            u, cell.bounds,
            "{hash:#010X}: root box is not the union of the patch boxes"
        );
    }
    eprintln!("400/400 cells re-encode byte-identically; {patches} patches");
    eprintln!("GEOM sphere vs sphere_of(aabb): max centre Δ {worst_centre:e}, max radius Δ {worst_radius:e}");
    for (d, n) in &decls {
        eprintln!("decl {d:?}: {n} draw groups");
    }
    assert!(
        worst_centre < 1e-4 && worst_radius < 1e-3,
        "GEOM spheres are not the box's centre + half-diagonal"
    );
}

#[test]
fn every_retail_draw_strip_survives_stripify() {
    let r = retail_or_skip!();
    let (mut draws, mut tris, mut bounded) = (0usize, 0usize, 0usize);
    for (hash, _, bytes) in &r.cells {
        let cell = TerrainCell::decode(bytes).unwrap();
        for g in &cell.geoms {
            for p in &g.prmgs {
                for d in p.draws.iter().chain(&p.alt_draws) {
                    let range = p.draw_range(d).unwrap();
                    let used = &p.indices[range.clone()];
                    let (lo, hi) = (used.iter().min().unwrap(), used.iter().max().unwrap());
                    if (*lo, *hi) == (d.min_index, d.max_index) {
                        bounded += 1;
                    }
                    let list = destrip(&p.indices[range]);
                    let strip = stripify(&list).unwrap_or_else(|e| panic!("{hash:#010X}: {e}"));
                    let canon = |t: [u16; 3]| {
                        let k = (0..3).min_by_key(|&i| t[i]).unwrap();
                        [t[k], t[(k + 1) % 3], t[(k + 2) % 3]]
                    };
                    let mut want: Vec<_> = list.iter().map(|&t| canon(t)).collect();
                    let mut got: Vec<_> = destrip(&strip).into_iter().map(canon).collect();
                    want.sort_unstable();
                    got.sort_unstable();
                    assert_eq!(
                        got, want,
                        "{hash:#010X}: a draw's triangles or windings changed"
                    );
                    draws += 1;
                    tris += list.len();
                }
            }
        }
    }
    eprintln!(
        "stripify ∘ destrip: {draws} retail draws, {tris} triangles, all preserved with winding; \
         {bounded} draws carry their range's exact min/max index"
    );
}

#[test]
fn retail_normals_follow_the_draw_winding() {
    let r = retail_or_skip!();
    let cell = hq(r);
    let mut errs = Vec::new();
    for g in &cell.geoms {
        for p in g.prmgs.iter().filter(|p| is_ground(p)) {
            let mut acc = vec![[0f64; 3]; p.vertex_count()];
            for t in p.triangles().unwrap() {
                let [a, b, c] = t.map(|v| p.position(v as usize).map(|x| x as f64));
                let (u, w) = (
                    [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                    [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
                );
                let f = [
                    u[1] * w[2] - u[2] * w[1],
                    u[2] * w[0] - u[0] * w[2],
                    u[0] * w[1] - u[1] * w[0],
                ];
                for v in t {
                    for k in 0..3 {
                        acc[v as usize][k] += f[k];
                    }
                }
            }
            for (i, s) in acc.iter().enumerate() {
                let n = p.normal(i).map(|x| x as f64);
                let (ls, ln) = (
                    (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt(),
                    (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt(),
                );
                if ls > 0.0 && ln > 0.0 {
                    let cos = (s[0] * n[0] + s[1] * n[1] + s[2] * n[2]) / (ls * ln);
                    errs.push(cos.clamp(-1.0, 1.0).acos().to_degrees());
                }
            }
        }
    }
    errs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = errs[errs.len() / 2];
    let p90 = errs[errs.len() * 9 / 10];
    eprintln!(
        "ground normals vs (b−a)×(c−a): {} vertices, median {median:.2}°, p90 {p90:.2}°",
        errs.len()
    );
    assert!(
        median < 1.5,
        "the recompute convention does not reproduce the retail normals"
    );
}

/// A cell's edge is its neighbours' edge: ground vertices on the shared line sit at the same
/// along-edge positions with the same height, to within one f16 step (the two cells quantize the same
/// source height independently). A few samples exist on one side only — retail T-junctions. This is why
/// [`TerrainCell::displace`] refuses to move an edge vertex.
#[test]
fn retail_cell_edges_are_shared_with_the_neighbour() {
    let r = retail_or_skip!();
    let centres = cell_centres(r);
    let hq_centre = centres[&HQ_CELL];
    let cell = hq(r);
    let (lo, hi) = cell.edge_extent().unwrap();
    assert_eq!(
        (lo, hi),
        ([-200.0, -200.0], [200.0, 200.0]),
        "the HQ cell spans ±200 m"
    );
    let edge_points = |c: &TerrainCell, centre: [f32; 3]| {
        let mut out = Vec::new();
        for (g, geom) in c.geoms.iter().enumerate() {
            for (p, prmg) in geom.prmgs.iter().enumerate() {
                if !is_ground(prmg) {
                    continue;
                }
                for i in 0..prmg.vertex_count() {
                    let v = c.cell_position(g, p, i);
                    out.push([v[0] + centre[0], v[1] + centre[1], v[2] + centre[2]]);
                }
            }
        }
        out
    };
    let mine = edge_points(&cell, hq_centre);
    let mut checked = 0;
    for (hash, _, bytes) in &r.cells {
        let c = centres[hash];
        let (dx, dz) = (c[0] - hq_centre[0], c[2] - hq_centre[2]);
        let (axis, at) = if dz == 0.0 && dx.abs() == 400.0 {
            (0, hq_centre[0] + dx / 2.0)
        } else if dx == 0.0 && dz.abs() == 400.0 {
            (2, hq_centre[2] + dz / 2.0)
        } else {
            continue;
        };
        let theirs = edge_points(&TerrainCell::decode(bytes).unwrap(), c);
        let along = 2 - axis;
        let a: HashMap<u32, f32> = mine
            .iter()
            .filter(|v| v[axis] == at)
            .map(|v| (v[along].to_bits(), v[1]))
            .collect();
        let b: HashMap<u32, f32> = theirs
            .iter()
            .filter(|v| v[axis] == at)
            .map(|v| (v[along].to_bits(), v[1]))
            .collect();
        assert!(
            !a.is_empty() && !b.is_empty(),
            "no ground vertex on the edge at {at}"
        );
        let mut common = 0;
        for (k, ya) in &a {
            if let Some(yb) = b.get(k) {
                // One f16 step at this height: 2^(exponent − 10).
                let ulp = 2f32
                    .powi(ya.abs().max(yb.abs()).max(f32::MIN_POSITIVE).log2().floor() as i32 - 10);
                assert!(
                    (ya - yb).abs() <= ulp,
                    "{hash:#010X}: edge heights {ya} vs {yb} differ by more than one f16 step"
                );
                common += 1;
            }
        }
        eprintln!(
            "edge with {hash:#010X}: {common} shared samples; {} only in the HQ cell, {} only in the neighbour",
            a.len() - common,
            b.len() - common
        );
        assert!(
            common * 10 >= a.len().max(b.len()) * 9,
            "{hash:#010X}: under 90% of the edge samples are shared"
        );
        checked += 1;
    }
    assert_eq!(
        checked, 4,
        "found {checked} of the HQ cell's four neighbours"
    );
}

#[test]
fn pyramid_on_the_pmc_hq_cell() {
    let r = retail_or_skip!();
    let centre = cell_centres(r)[&HQ_CELL];
    assert_eq!(
        centre,
        [2600.0, 0.0, -1000.0],
        "Terrain_r07_c16's placement"
    );
    let (_, path, original) = r.cells.iter().find(|c| c.0 == HQ_CELL).unwrap();
    assert!(
        path.to_lowercase().ends_with("c31411_p000_q3.block"),
        "{path}"
    );

    let before = TerrainCell::decode(original).unwrap();
    let mut cell = before.clone();
    let report = cell.displace(pyramid).unwrap();
    eprintln!("pyramid: {report:?}");

    // Every vertex moved by the pyramid's height at its (x, z), to f16 precision; the apex vertex +40 m.
    let (mut apex, mut checked) = (None, 0usize);
    for (g, (a, b)) in before.geoms.iter().zip(&cell.geoms).enumerate() {
        for (p, (pa, pb)) in a.prmgs.iter().zip(&b.prmgs).enumerate() {
            for i in 0..pa.vertex_count() {
                let c = before.cell_position(g, p, i);
                let (y0, y1) = (pa.position(i)[1], pb.position(i)[1]);
                let want = pyramid(c[0], c[2]);
                let step = f32::EPSILON.max((y0 + want).abs() / 1024.0);
                assert!(
                    (y1 - y0 - want).abs() <= step,
                    "vertex at ({}, {}) moved {} not {want}",
                    c[0],
                    c[2],
                    y1 - y0
                );
                assert_eq!(cell.cell_position(g, p, i)[0], c[0]);
                assert_eq!(cell.cell_position(g, p, i)[2], c[2]);
                if c[0] == APEX[0] && c[2] == APEX[1] {
                    apex = Some((y0, y1));
                }
                checked += 1;
            }
        }
    }
    let (base, top) = apex.expect("a vertex sits exactly under the apex");
    eprintln!(
        "apex (world {}, {}): ground {base} → {top} (Δ {})",
        centre[0] + APEX[0],
        centre[2] + APEX[1],
        top - base
    );
    assert!(
        (top - base - HEIGHT).abs() <= 0.0625,
        "apex rose {} m",
        top - base
    );

    // Seam: nothing on the cell edge moved or was re-lit.
    let (lo, hi) = before.edge_extent().unwrap();
    for (g, (a, b)) in before.geoms.iter().zip(&cell.geoms).enumerate() {
        for (p, (pa, pb)) in a.prmgs.iter().zip(&b.prmgs).enumerate() {
            let s = pa.stride as usize;
            for i in 0..pa.vertex_count() {
                let c = before.cell_position(g, p, i);
                if c[0] == lo[0] || c[0] == hi[0] || c[2] == lo[1] || c[2] == hi[1] {
                    assert_eq!(
                        pa.vertices[i * s..(i + 1) * s],
                        pb.vertices[i * s..(i + 1) * s],
                        "edge vertex changed"
                    );
                }
            }
        }
    }

    // The bounds follow the new ground.
    assert!(
        cell.bounds.max[1] >= top,
        "root box top {} below the apex {top}",
        cell.bounds.max[1]
    );

    // The edited container re-parses to the same cell.
    let bytes = cell.encode().unwrap();
    assert!(mercs2_formats::ucfx::verify_ucfx_container(&bytes, "hq", TYPE_HASH).is_none());
    assert_eq!(TerrainCell::decode(&bytes).unwrap(), cell);
    eprintln!(
        "checked {checked} vertices; edited container {} bytes",
        bytes.len()
    );
}

#[test]
fn rebuilt_hq_collision_parses_and_never_misses_an_edited_triangle() {
    let r = retail_or_skip!();
    let mut cell = hq(r);
    cell.displace(pyramid).unwrap();
    cell.rebuild_collision(HQ_CELL).unwrap();
    let bytes = cell.encode().unwrap();
    let back = TerrainCell::decode(&bytes).unwrap();
    let body: Vec<u8> = back
        .phy2
        .prefix
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .chain(back.phy2.payload.iter().copied())
        .collect();
    let packfile = mercs2_formats::havok::parse_phy2_body(&body).expect("rebuilt PHY2 re-parses");
    eprintln!(
        "rebuilt PHY2: {} bytes, prefix {:?}, classes {:?}",
        body.len(),
        back.phy2.prefix,
        packfile.class_counts
    );

    let soups = back.collision_soups().unwrap();
    let mopps = mercs2_formats::mopp::extract_mopp_with_info(&body);
    assert_eq!(mopps.len(), soups.len());
    let (mut queries, mut candidates) = (0usize, 0usize);
    for (patch, ((code, info), (tris, verts))) in mopps.iter().zip(&soups).enumerate() {
        for (k, t) in tris.iter().enumerate() {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for &v in t {
                for a in 0..3 {
                    lo[a] = lo[a].min(verts[v as usize][a]);
                    hi[a] = hi[a].max(verts[v as usize][a]);
                }
            }
            let got = mercs2_formats::mopp::query_aabb(code, info, lo, hi);
            assert!(
                got.contains(&(k as u32)),
                "patch {patch}: the MOPP misses triangle {k}"
            );
            queries += 1;
            candidates += got.len();
        }
    }
    eprintln!(
        "MOPP no-miss: {queries} triangle queries, {:.1} candidates each",
        candidates as f64 / queries as f64
    );
    // The collider carries the raised apex.
    let apex = soups
        .iter()
        .flat_map(|(_, v)| v.iter())
        .find(|v| v[0] == APEX[0] && v[2] == APEX[1])
        .expect("a collision vertex under the apex");
    let before = hq(r);
    let ground = before
        .geoms
        .iter()
        .enumerate()
        .flat_map(|(g, geom)| {
            (0..geom.prmgs.len())
                .flat_map(move |p| (0..geom.prmgs[p].vertex_count()).map(move |i| (g, p, i)))
        })
        .map(|(g, p, i)| before.cell_position(g, p, i))
        .find(|v| v[0] == APEX[0] && v[2] == APEX[1])
        .unwrap();
    eprintln!("collider apex y {} (ground was {})", apex[1], ground[1]);
    assert!(
        (apex[1] - ground[1] - HEIGHT).abs() <= 0.0625,
        "collider apex rose {} m",
        apex[1] - ground[1]
    );
}

/// What stands inside the pyramid's footprint, and the largest clear square in the cell. A report, not a
/// gate: it prints what it finds.
#[test]
fn placements_in_the_pyramid_footprint() {
    let r = retail_or_skip!();
    let centre = cell_centres(r)[&HQ_CELL];
    let (ax, az) = (centre[0] + APEX[0], centre[2] + APEX[1]);
    let mut in_cell = Vec::new();
    for (path, block) in &r.layers {
        // `load_placements` errs only on a layer with no Transform records — nothing placed in it.
        let Ok(placed) = load_placements(block) else {
            continue;
        };
        for p in placed {
            if (p.pos[0] - centre[0]).abs() <= 200.0 && (p.pos[2] - centre[2]).abs() <= 200.0 {
                in_cell.push((path.clone(), p));
            }
        }
    }
    assert!(!in_cell.is_empty(), "no placement inside the HQ cell");
    let inside: Vec<_> = in_cell
        .iter()
        .filter(|(_, p)| (p.pos[0] - ax).abs() <= HALF_WIDTH && (p.pos[2] - az).abs() <= HALF_WIDTH)
        .collect();
    eprintln!(
        "{} placements in the cell; {} inside the footprint x {}..{}, z {}..{}:",
        in_cell.len(),
        inside.len(),
        ax - HALF_WIDTH,
        ax + HALF_WIDTH,
        az - HALF_WIDTH,
        az + HALF_WIDTH
    );
    for (path, p) in &inside {
        eprintln!(
            "  {:08X} {:<32} {:?}  [{path}]",
            p.key,
            p.name.as_deref().unwrap_or("-"),
            p.pos
        );
    }
    // Largest axis-aligned clear square fully inside the cell, centres on a 1 m grid.
    let local: Vec<(f32, f32)> = in_cell
        .iter()
        .map(|(_, p)| (p.pos[0] - centre[0], p.pos[2] - centre[2]))
        .collect();
    let mut best = (0f32, 0f32, 0f32);
    for ix in -200..=200 {
        for iz in -200..=200 {
            let (x, z) = (ix as f32, iz as f32);
            let mut h = 200.0 - x.abs().max(z.abs());
            for &(px, pz) in &local {
                h = h.min((px - x).abs().max((pz - z).abs()));
            }
            if h > best.0 {
                best = (h, x, z);
            }
        }
    }
    eprintln!(
        "largest clear square: half-width {} m at cell-local ({}, {}) = world ({}, {})",
        best.0,
        best.1,
        best.2,
        centre[0] + best.1,
        centre[2] + best.2
    );
}
