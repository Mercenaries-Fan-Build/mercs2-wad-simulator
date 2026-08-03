//! `densify` — add vegetation density to Mercenaries 2 by authoring NOVEL placements the retail
//! engine renders. Densifies EXISTING treed areas with a MIXED canopy+understory scatter, snapped
//! to ground height, avoiding water/roads/buildings. Emits a deployable overlay WAD (does NOT
//! deploy) + a coverage report. Base `vz.wad` stays pristine.
//!
//! Pipeline (the proven working recipe, in-process):
//!   1. Resolve vz.wad, decompress the `layers_static` block (the block whose primary layer ASET
//!      row is `0xB41FC710`).
//!   2. Read existing veg positions (treed areas) + non-veg model footprints (exclusions) from it.
//!   3. Scatter one veg model per plantable candidate: within `--radius` of existing veg, on land,
//!      off-road, gentle slope, clear of building discs, `--spacing` apart, ground-snapped,
//!      deterministic yaw/species (seeded hash of x,z — no wall-clock RNG), capped at `--cap`.
//!   4. Author via `placement_build::append_placements` (template sub 15, layer `vz_densitypatch`
//!      = 0xCEDC9142), gate with `ucfx-check` (0 issues), build the overlay WAD (replace
//!      0xB41FC710 = densified block, add layer 0xCEDC9142), verify it re-parses to 174 layer rows.

mod heightmap;

use heightmap::HeightMap;
use mercs2_engine::wad;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::placement::{entity_key_set, load_placements};
use mercs2_formats::placement_build::{append_placements, NewEntity};
use mercs2_formats::types::TYPE_ID_LAYER;
use mercs2_formats::veg::classify;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The `layers_static` block's PRIMARY layer ASET hash — the block we replace in the overlay.
const BASE_LAYER_HASH: u32 = 0xB41F_C710;
/// Template sub-block index carrying Name/ModelName/Transform COMPs (proven donor).
const TEMPLATE_SUB: usize = 15;
/// Our appended layer's entry-table name = `pandemic_hash_m2("vz_densitypatch")`.
const LAYER_NAME: &str = "vz_densitypatch";
const LAYER_HASH: u32 = 0xCEDC_9142;
/// Entity-key band; skip existing keys and low-byte 0x00/0x01.
const KEY_BAND_START: u32 = 0x00F0_0000;

/// One vegetation species we can plant, with its ModelName hash and weight tier.
struct Species {
    hash: u32,
    name: &'static str,
    tier: Tier,
    weight: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tier {
    Understory,
    Midstory,
    Canopy,
}
impl Tier {
    fn label(self) -> &'static str {
        match self {
            Tier::Understory => "understory",
            Tier::Midstory => "midstory/palm",
            Tier::Canopy => "canopy",
        }
    }
}

/// MIXED palette (hashes from the tree/veg census — all render via ModelName). Weights sum to 30:
/// understory 18 (60%), midstory/palm 9 (30%), canopy 3 (10%); equal within tier.
const SPECIES: &[Species] = &[
    Species { hash: 0x6C2E_7291, name: "plantlarge04",   tier: Tier::Understory, weight: 6 },
    Species { hash: 0x4C36_FE4D, name: "plantmed03",     tier: Tier::Understory, weight: 6 },
    Species { hash: 0xEB60_9C65, name: "bushmedium03",   tier: Tier::Understory, weight: 6 },
    Species { hash: 0x04E1_DED8, name: "palmtreebend02", tier: Tier::Midstory,   weight: 3 },
    Species { hash: 0x0481_7CD7, name: "treesmall01",    tier: Tier::Midstory,   weight: 3 },
    Species { hash: 0x5AB8_A933, name: "treemedium03",   tier: Tier::Midstory,   weight: 3 },
    Species { hash: 0xFF7A_BB3B, name: "largecanopy01",  tier: Tier::Canopy,     weight: 1 },
    Species { hash: 0x40D1_E566, name: "treetall02",     tier: Tier::Canopy,     weight: 1 },
    Species { hash: 0x88D0_9B0C, name: "smallcanopy02",  tier: Tier::Canopy,     weight: 1 },
];

struct Config {
    wad: Option<String>,
    heightmap_dir: PathBuf,
    out_dir: PathBuf,
    radius: f32,
    spacing: f32,
    cap: usize,
    seed: u64,
    max_slope: f32,
    exclude_radius: f32,
    region: Option<[f32; 4]>, // xmin,zmin,xmax,zmax
    // Line-fill mode (runway carpet): dense grid over a strip between two points, no veg-anchor scan.
    line: Option<[f32; 4]>, // x1,z1,x2,z2
    half_width: f32,        // strip half-width (m) each side of the centerline
    model: u32,             // model hash to place (0 = default palm 0x799C0CA2)
    flat_y: Option<f32>,    // ground Y override (flat); falls back to heightmap when absent
}

fn main() {
    if let Err(e) = run() {
        eprintln!("densify: error: {e}");
        std::process::exit(1);
    }
}

fn parse_args() -> Result<Config, String> {
    let mut cfg = Config {
        wad: None,
        heightmap_dir: PathBuf::from(r"C:\Users\Shadow\Downloads\wallys-work\heightmap-data"),
        out_dir: PathBuf::from("output/foliage"),
        radius: 60.0,
        spacing: 6.0,
        cap: 2000,
        seed: 0x5EED_1234,
        max_slope: 1.2,
        exclude_radius: 8.0,
        region: None,
        line: None,
        half_width: 5.0,
        model: 0x799C_0CA2, // global_env_palmtree01
        flat_y: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut next = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--wad" => cfg.wad = Some(next()?),
            "--heightmap" => cfg.heightmap_dir = PathBuf::from(next()?),
            "--out-dir" => cfg.out_dir = PathBuf::from(next()?),
            "--radius" => cfg.radius = next()?.parse().map_err(|_| "bad --radius")?,
            "--spacing" => cfg.spacing = next()?.parse().map_err(|_| "bad --spacing")?,
            "--cap" => cfg.cap = next()?.parse().map_err(|_| "bad --cap")?,
            "--seed" => {
                let s = next()?;
                cfg.seed = s
                    .strip_prefix("0x")
                    .and_then(|h| u64::from_str_radix(h, 16).ok())
                    .or_else(|| s.parse().ok())
                    .ok_or("bad --seed")?;
            }
            "--max-slope" => cfg.max_slope = next()?.parse().map_err(|_| "bad --max-slope")?,
            "--exclude-radius" => {
                cfg.exclude_radius = next()?.parse().map_err(|_| "bad --exclude-radius")?
            }
            "--region" => {
                let v = next()?;
                let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if p.len() != 4 {
                    return Err("--region wants xmin,zmin,xmax,zmax".into());
                }
                cfg.region = Some([p[0], p[1], p[2], p[3]]);
            }
            "--line" => {
                let v = next()?;
                let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if p.len() != 4 {
                    return Err("--line wants x1,z1,x2,z2".into());
                }
                cfg.line = Some([p[0], p[1], p[2], p[3]]);
            }
            "--half-width" => cfg.half_width = next()?.parse().map_err(|_| "bad --half-width")?,
            "--model" => {
                let s = next()?;
                cfg.model = u32::from_str_radix(s.trim_start_matches("0x"), 16)
                    .map_err(|_| "bad --model (hex)")?;
            }
            "--y" => cfg.flat_y = Some(next()?.parse().map_err(|_| "bad --y")?),
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            o => return Err(format!("unknown arg {o}")),
        }
    }
    Ok(cfg)
}

fn print_help() {
    println!(
        "densify — add vegetation density to Mercenaries 2 (novel ground-snapped scatter -> overlay WAD)\n\
         \n\
         USAGE: densify [options]\n\
         \n\
         --radius <m>          plant within this radius of existing veg (default 60)\n\
         --spacing <m>         min spacing between added plants (default 6)\n\
         --cap <n>             max plants added (default 2000)\n\
         --seed <n|0xHEX>      deterministic scatter seed (default 0x5EED1234)\n\
         --max-slope <r>       reject slope > this (rise/run; default 1.2 ~= 50deg)\n\
         --exclude-radius <m>  building/prop exclusion disc radius (default 8)\n\
         --region xmin,zmin,xmax,zmax   limit scatter to one area (world units)\n\
         --wad <path>          override vz.wad discovery\n\
         --heightmap <dir>     Wally heightmap-data dir (heights.bin/tiers.bin)\n\
         --out-dir <dir>       output dir (default output/foliage)\n"
    );
}

fn run() -> Result<(), String> {
    let cfg = parse_args()?;
    assert_eq!(
        pandemic_hash_m2(LAYER_NAME),
        LAYER_HASH,
        "layer name hash drifted from the proven 0xCEDC9142"
    );

    // ── 1. vz.wad + the layers_static block ───────────────────────────────────────────────────
    let wadpath = wad::resolve_vz_wad(cfg.wad.as_deref())
        .ok_or("no vz.wad found — pass --wad <folder-or-file> or set VZ_WAD")?;
    eprintln!("vz.wad: {wadpath}");
    let mut w = wad::open(&wadpath).map_err(|e| format!("open {wadpath}: {e}"))?;

    // Find the block hosting the primary BASE_LAYER_HASH row (== dump-block 29), robustly.
    let block_index = {
        let (ar, _f) = wad::archive_and_file(&mut w);
        ar.aset
            .iter()
            .find(|e| e.asset_hash == BASE_LAYER_HASH && e.is_primary())
            .map(|e| e.block_index())
            .ok_or("no primary ASET row for 0xB41FC710 (layers_static) in vz.wad")?
    };
    let block = wad::decompress_block_index(&mut w, block_index)
        .map_err(|e| format!("decompress block {block_index}: {e}"))?;
    eprintln!(
        "layers_static: block {block_index}, {} bytes decompressed",
        block.len()
    );

    // ── LINE-FILL MODE (runway carpet): dense grid over a strip, no veg-anchor scan ─────────────
    if let Some(line) = cfg.line {
        let hm = HeightMap::load(&cfg.heightmap_dir)?;
        let keys_before = entity_key_set(&block);
        let ents = build_line_entities(&cfg, &hm, line, &keys_before)?;
        let n = ents.len();
        eprintln!(
            "line-fill: {} palms (0x{:08X}) over strip ({:.0},{:.0})-({:.0},{:.0}) ±{} m @ {} m spacing",
            n, cfg.model, line[0], line[1], line[2], line[3], cfg.half_width, cfg.spacing
        );
        let densified = append_placements(&block, TEMPLATE_SUB, &ents, LAYER_HASH)?;
        let (parsed, issues) = mercs2_formats::ucfx::walk_decompressed_block(&densified, "runway");
        println!("[ucfx-check] {} entries; {} UCFX issues", parsed.entry_count, issues.len());
        if !issues.is_empty() {
            return Err(format!("ucfx-check found {} issues — refusing to emit", issues.len()));
        }
        std::fs::create_dir_all(&cfg.out_dir)
            .map_err(|e| format!("mkdir {}: {e}", cfg.out_dir.display()))?;
        let wad_path = cfg.out_dir.join("vz-runway.wad");
        build_overlay(&wadpath, block_index, &densified, &wad_path)?;
        println!(
            "Wrote {} ({} palms). Mount as data/vz-patch.wad (overlay, last-wins).",
            wad_path.display(),
            n
        );
        return Ok(());
    }

    // ── 2. existing veg positions (treed areas) + non-veg footprints (exclusions) ──────────────
    // Use load_placements (GLOBAL Name<->Transform join) and classify by NAME — matching the proven
    // veg_census. In layers_static an entity's Name COMP and Transform COMP can live in DIFFERENT
    // sub-blocks, so the per-sub-block ModelName join (load_model_placements) misses nearly all veg.
    let placements = load_placements(&block)?;
    let mut veg_pts: Vec<(f32, f32)> = Vec::new();
    let mut excl_pts: Vec<(f32, f32)> = Vec::new();
    let mut unnamed = 0usize;
    for p in &placements {
        let xz = (p.pos[0], p.pos[2]);
        match &p.name {
            Some(n) if classify(n.trim_start_matches('_')).is_some() => veg_pts.push(xz),
            Some(_) => excl_pts.push(xz), // named non-veg = building/prop/locator footprint
            None => unnamed += 1,
        }
    }
    eprintln!(
        "existing placements: {} total, {} veg (treed anchors), {} non-veg (exclusion discs), {} unnamed",
        placements.len(),
        veg_pts.len(),
        excl_pts.len(),
        unnamed
    );
    if veg_pts.is_empty() {
        return Err("no existing vegetation found to densify around".into());
    }

    // ── 3. scatter ─────────────────────────────────────────────────────────────────────────────
    let hm = HeightMap::load(&cfg.heightmap_dir)?;
    let scatter = scatter(&cfg, &hm, &veg_pts, &excl_pts);
    eprintln!(
        "scatter: {} plantable candidates, {} accepted (cap {})",
        scatter.candidates_total, scatter.accepted.len(), cfg.cap
    );

    if scatter.accepted.is_empty() {
        return Err("no plantable candidates — loosen --radius/--max-slope or check the region".into());
    }

    // ── 4. author placements ───────────────────────────────────────────────────────────────────
    let keys_before = entity_key_set(&block);
    let ents = build_entities(&cfg, &hm, &scatter, &keys_before)?;
    let n = ents.len();

    let densified = append_placements(&block, TEMPLATE_SUB, &ents, LAYER_HASH)?;

    // Gate A: ucfx-check == 0 issues on the densified block.
    let (parsed, issues) = mercs2_formats::ucfx::walk_decompressed_block(&densified, "densified");
    println!("\n=== GATE A: ucfx-check (densified block) ===");
    println!("[ucfx-check] {} entries; {} UCFX issues", parsed.entry_count, issues.len());
    for iss in issues.iter().take(8) {
        println!("  {}: {}", iss.context, iss.detail);
    }
    if !issues.is_empty() {
        return Err(format!("ucfx-check found {} issues — refusing to emit", issues.len()));
    }

    // Gate B: entity_key_set shows exactly +N unique keys, no collisions.
    let keys_after = entity_key_set(&densified);
    let added: HashSet<u32> = keys_after.difference(&keys_before).copied().collect();
    let collisions: Vec<u32> = ents
        .iter()
        .map(|e| e.key)
        .filter(|k| keys_before.contains(k))
        .collect();
    println!("\n=== GATE B: entity keys ===");
    println!(
        "[keys] before={}, after={}, added={} (want {}), collisions={}",
        keys_before.len(),
        keys_after.len(),
        added.len(),
        n,
        collisions.len()
    );
    if added.len() != n || !collisions.is_empty() {
        return Err("entity-key gate failed (added != N or collisions present)".into());
    }

    // ── 5. build the overlay WAD (replace 0xB41FC710 = densified, add layer 0xCEDC9142) ─────────
    std::fs::create_dir_all(&cfg.out_dir).map_err(|e| format!("mkdir {}: {e}", cfg.out_dir.display()))?;
    let block_path = cfg.out_dir.join("vz-density-layers_static.bin");
    std::fs::write(&block_path, &densified).map_err(|e| format!("write block: {e}"))?;
    let wad_path = cfg.out_dir.join("vz-density-patch.wad");
    build_overlay(&wadpath, block_index, &densified, &wad_path)?;

    // Gate C: overlay re-parses to 174 layer rows incl. 0xCEDC9142.
    println!("\n=== GATE C: overlay re-parse ===");
    let (layer_rows, has_ours, total_rows) = count_overlay_layers(&wad_path)?;
    println!(
        "[overlay] {} total ASET rows; {} layer rows; contains 0xCEDC9142 = {}",
        total_rows, layer_rows, has_ours
    );
    if layer_rows != 174 || !has_ours {
        return Err(format!(
            "overlay gate failed: {layer_rows} layer rows (want 174), ours present = {has_ours}"
        ));
    }

    // ── 6. coverage report ─────────────────────────────────────────────────────────────────────
    coverage_report(&cfg, &scatter, &ents, &block_path, &wad_path, &wadpath, block_index);
    Ok(())
}

// ───────────────────────────────────────── deterministic hash ─────────────────────────────────

/// FNV-1a-64 over the seed + salted, mm-quantized coordinates. Reproducible; no wall-clock RNG.
fn hash_xz(seed: u64, salt: u64, x: f32, z: f32) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed.wrapping_mul(0x1000_0000_01b3);
    let feed = |h: &mut u64, v: u64| {
        for b in v.to_le_bytes() {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(&mut h, salt);
    feed(&mut h, (x * 1000.0).round() as i64 as u64);
    feed(&mut h, (z * 1000.0).round() as i64 as u64);
    h
}

/// Uniform f64 in [0,1) from a hash.
fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

// ───────────────────────────────────────── spatial grid ───────────────────────────────────────

/// Bucketed 2-D point set for O(1) "is there a point within r" queries.
struct Grid {
    bucket: f32,
    cells: HashMap<(i32, i32), Vec<(f32, f32)>>,
}
impl Grid {
    fn new(bucket: f32) -> Grid {
        Grid { bucket, cells: HashMap::new() }
    }
    fn from_points(bucket: f32, pts: &[(f32, f32)]) -> Grid {
        let mut g = Grid::new(bucket);
        for &(x, z) in pts {
            g.insert(x, z);
        }
        g
    }
    #[inline]
    fn key(&self, x: f32, z: f32) -> (i32, i32) {
        ((x / self.bucket).floor() as i32, (z / self.bucket).floor() as i32)
    }
    fn insert(&mut self, x: f32, z: f32) {
        let k = self.key(x, z);
        self.cells.entry(k).or_default().push((x, z));
    }
    fn has_within(&self, x: f32, z: f32, r: f32) -> bool {
        let r2 = r * r;
        let span = (r / self.bucket).ceil() as i32;
        let (kx, kz) = self.key(x, z);
        for dx in -span..=span {
            for dz in -span..=span {
                if let Some(v) = self.cells.get(&(kx + dx, kz + dz)) {
                    for &(px, pz) in v {
                        let (ex, ez) = (px - x, pz - z);
                        if ex * ex + ez * ez <= r2 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

// ───────────────────────────────────────── scatter ────────────────────────────────────────────

struct Accepted {
    x: f32,
    z: f32,
    species: usize, // index into SPECIES
    yaw: f32,
}

#[derive(Default)]
struct Rejections {
    unscanned: usize,
    water: usize,
    road: usize,
    slope: usize,
    building: usize,
    spacing: usize,
    capped: usize,
}

struct Scatter {
    accepted: Vec<Accepted>,
    rej: Rejections,
    candidates_total: usize,
    region: [f32; 4],
}

fn scatter(cfg: &Config, hm: &HeightMap, veg: &[(f32, f32)], excl: &[(f32, f32)]) -> Scatter {
    // Work only over the veg bounding box expanded by radius (nothing outside it can be plantable),
    // intersected with any explicit --region.
    let (mut minx, mut minz, mut maxx, mut maxz) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, z) in veg {
        minx = minx.min(x);
        minz = minz.min(z);
        maxx = maxx.max(x);
        maxz = maxz.max(z);
    }
    minx -= cfg.radius;
    minz -= cfg.radius;
    maxx += cfg.radius;
    maxz += cfg.radius;
    if let Some(r) = cfg.region {
        minx = minx.max(r[0]);
        minz = minz.max(r[1]);
        maxx = maxx.min(r[2]);
        maxz = maxz.min(r[3]);
    }
    let region = [minx, minz, maxx, maxz];

    let veg_grid = Grid::from_points(cfg.radius, veg);
    let excl_grid = Grid::from_points(cfg.exclude_radius.max(1.0), excl);

    // Candidate lattice is FINER than the target spacing and FULLY jittered (jitter ≈ cell),
    // so candidate positions are near-random. The greedy min-spacing accept below (keyed off
    // cfg.spacing, independent of this step) then thins them to a blue-noise / Poisson-disk
    // scatter — organic, not a visible grid. (Was step=spacing, jitter=0.4 → rows.)
    let step = (cfg.spacing * 0.4).max(1.0);
    let jitter = step;
    let mut rej = Rejections::default();

    // Pass 1: gather plantable candidates with a deterministic priority hash.
    let mut cands: Vec<(u64, f32, f32)> = Vec::new();
    let mut gx = minx;
    while gx <= maxx {
        let mut gz = minz;
        while gz <= maxz {
            // deterministic jitter within the lattice cell
            let hx = hash_xz(cfg.seed, 1, gx, gz);
            let hz = hash_xz(cfg.seed, 2, gx, gz);
            let x = gx + (unit(hx) as f32 * 2.0 - 1.0) * jitter;
            let z = gz + (unit(hz) as f32 * 2.0 - 1.0) * jitter;
            gz += step;

            if x < region[0] || x > region[2] || z < region[1] || z > region[3] {
                continue;
            }
            // gate: must be inside a treed area to even be a candidate
            if !veg_grid.has_within(x, z, cfg.radius) {
                continue;
            }
            if hm.is_unscanned(x, z) {
                rej.unscanned += 1;
                continue;
            }
            if hm.is_water(x, z) {
                rej.water += 1;
                continue;
            }
            if hm.is_road(x, z) {
                rej.road += 1;
                continue;
            }
            if hm.slope(x, z) > cfg.max_slope {
                rej.slope += 1;
                continue;
            }
            if excl_grid.has_within(x, z, cfg.exclude_radius) {
                rej.building += 1;
                continue;
            }
            let prio = hash_xz(cfg.seed, 3, x, z);
            cands.push((prio, x, z));
        }
        gx += step;
    }
    let candidates_total = cands.len();

    // Pass 2: greedy accept in deterministic priority order, enforcing min-spacing + cap. Random
    // priority order spreads the cap across the whole plantable set rather than one corner.
    cands.sort_unstable_by_key(|c| c.0);
    let mut added = Grid::new(cfg.spacing);
    let mut accepted: Vec<Accepted> = Vec::new();
    for (_, x, z) in cands {
        if accepted.len() >= cfg.cap {
            rej.capped += 1;
            continue;
        }
        if added.has_within(x, z, cfg.spacing) {
            rej.spacing += 1;
            continue;
        }
        added.insert(x, z);
        let species = pick_species(cfg.seed, x, z);
        let yaw = (unit(hash_xz(cfg.seed, 5, x, z)) as f32) * std::f32::consts::TAU;
        accepted.push(Accepted { x, z, species, yaw });
    }

    Scatter { accepted, rej, candidates_total, region }
}

/// Weighted species pick (understory 60% / midstory 30% / canopy 10%), deterministic by (seed,x,z).
fn pick_species(seed: u64, x: f32, z: f32) -> usize {
    let total: u32 = SPECIES.iter().map(|s| s.weight).sum();
    let mut r = (hash_xz(seed, 4, x, z) % total as u64) as u32;
    for (i, s) in SPECIES.iter().enumerate() {
        if r < s.weight {
            return i;
        }
        r -= s.weight;
    }
    SPECIES.len() - 1
}

// ───────────────────────────────────────── authoring ──────────────────────────────────────────

fn build_entities(
    _cfg: &Config,
    hm: &HeightMap,
    scatter: &Scatter,
    existing: &HashSet<u32>,
) -> Result<Vec<NewEntity>, String> {
    let mut ents = Vec::with_capacity(scatter.accepted.len());
    let mut key = KEY_BAND_START;
    let mut used: HashSet<u32> = HashSet::new();
    let mut next_key = |used: &mut HashSet<u32>| -> u32 {
        loop {
            let k = key;
            key += 1;
            let low = k & 0xFF;
            if low == 0x00 || low == 0x01 {
                continue;
            }
            if existing.contains(&k) || used.contains(&k) {
                continue;
            }
            used.insert(k);
            return k;
        }
    };

    for a in &scatter.accepted {
        let s = &SPECIES[a.species];
        // ground-snap Y (candidates already gated as scanned/land)
        let y = hm.ground_y(a.x, a.z).ok_or("accepted point became unscanned")?;
        // quaternion about Y only: [qx,qy,qz,qw] = [0, sin(yaw/2), 0, cos(yaw/2)]
        let (sy, cy) = (a.yaw * 0.5).sin_cos();
        let quat = [0.0, sy, 0.0, cy];
        let k = next_key(&mut used);
        ents.push(NewEntity {
            key: k,
            model_hash: s.hash,
            pos: [a.x, y, a.z],
            quat,
            name: format!("densify_{}", s.name),
        });
    }
    Ok(ents)
}

/// Line-fill: a dense grid of ONE model over the strip between (x1,z1)-(x2,z2), ±half_width each
/// side of the centerline, at `--spacing` in both axes. Ground-snapped (heightmap `ground_y`, or a
/// flat `--y`), varied Y-only yaw. Collision-free keys in the same band as scatter mode.
fn build_line_entities(
    cfg: &Config,
    hm: &HeightMap,
    line: [f32; 4],
    existing: &HashSet<u32>,
) -> Result<Vec<NewEntity>, String> {
    let (x1, z1, x2, z2) = (line[0], line[1], line[2], line[3]);
    let (dx, dz) = (x2 - x1, z2 - z1);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 1.0 {
        return Err("--line endpoints are the same point".into());
    }
    let (ux, uz) = (dx / len, dz / len); // unit along centerline
    let (perpx, perpz) = (uz, -ux); // unit perpendicular
    let sp = cfg.spacing.max(0.5);

    let mut ents = Vec::new();
    let mut key = KEY_BAND_START;
    let mut used: HashSet<u32> = HashSet::new();
    let mut next_key = |used: &mut HashSet<u32>| -> u32 {
        loop {
            let k = key;
            key += 1;
            let low = k & 0xFF;
            if low == 0x00 || low == 0x01 {
                continue;
            }
            if existing.contains(&k) || used.contains(&k) {
                continue;
            }
            used.insert(k);
            return k;
        }
    };

    let n_along = (len / sp).floor() as i32;
    let n_across = (cfg.half_width / sp).floor() as i32;
    for i in 0..=n_along {
        let t = i as f32 * sp;
        let (cx, cz) = (x1 + ux * t, z1 + uz * t);
        for j in -n_across..=n_across {
            let w = j as f32 * sp;
            let (wx, wz) = (cx + perpx * w, cz + perpz * w);
            let y = cfg.flat_y.or_else(|| hm.ground_y(wx, wz)).unwrap_or(-14.5);
            let yaw = (unit(hash_xz(cfg.seed, 7, wx, wz)) as f32) * std::f32::consts::TAU;
            let (sy, cy) = (yaw * 0.5).sin_cos();
            let k = next_key(&mut used);
            ents.push(NewEntity {
                key: k,
                model_hash: cfg.model,
                pos: [wx, y, wz],
                quat: [0.0, sy, 0.0, cy],
                name: format!("runway_palm_{k:06X}"),
            });
        }
    }
    Ok(ents)
}

// ───────────────────────────────────────── overlay WAD ────────────────────────────────────────

/// Build the overlay WAD: carry every ASET row for `block_index` from the base, append our layer
/// row for 0xCEDC9142, replace the block bytes with the densified block. Mirrors
/// `override_base_blocks --replace 0xB41FC710=<blk> --add-layer 0xCEDC9142`.
fn build_overlay(
    base_wad: &str,
    block_index: u16,
    densified: &[u8],
    out: &Path,
) -> Result<(), String> {
    use std::fs::File;
    let mut f = File::open(base_wad).map_err(|e| format!("open {base_wad}: {e}"))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("parse {base_wad}: {e}"))?;

    let bi = block_index as usize;
    let path = ar.paths.get(bi).cloned().ok_or("block has no path")?;

    let mut aset: Vec<AsetEntry> = ar
        .aset
        .iter()
        .filter(|e| e.block_index() as usize == bi)
        .map(|e| AsetEntry::new(e.asset_hash, e.secondary_ref, e.packed_block_ref, e.type_id))
        .collect();
    let carried = aset.len();
    if aset.iter().any(|e| e.asset_hash == LAYER_HASH) {
        return Err("block already advertises 0xCEDC9142".into());
    }
    // New layer row: primary/single-block (u32_1 = 0xFFFFFFFF, u32_2 low16 = 0xFFFF), type 9.
    aset.push(AsetEntry::new(LAYER_HASH, 0xFFFF_FFFF, 0x0000_FFFF, TYPE_ID_LAYER));

    let tier = ar.indx.get(bi).map(|i| i.packed_field);
    let blk = PatchBlock::from_decompressed(densified, path, aset, tier)?;
    eprintln!(
        "overlay: block {bi} <- densified ({} rows = {} carried + 1 layer, {} pages)",
        blk.aset_entries.len(),
        carried,
        blk.declared_pages()
    );
    let bytes = build_patch_wad_multi(&[blk], 0, None, &FFCS_CERT_BLOB)?;
    std::fs::write(out, &bytes).map_err(|e| format!("write {}: {e}", out.display()))?;
    Ok(())
}

/// Re-parse the overlay WAD; return (layer-type row count, contains-our-hash, total rows).
fn count_overlay_layers(wad_path: &Path) -> Result<(usize, bool, usize), String> {
    use std::fs::File;
    let mut f = File::open(wad_path).map_err(|e| format!("open {}: {e}", wad_path.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("parse overlay: {e}"))?;
    let layer_rows = ar.aset.iter().filter(|e| e.type_id == TYPE_ID_LAYER).count();
    let has_ours = ar.aset.iter().any(|e| e.asset_hash == LAYER_HASH);
    Ok((layer_rows, has_ours, ar.aset.len()))
}

// ───────────────────────────────────────── report ─────────────────────────────────────────────

fn coverage_report(
    cfg: &Config,
    scatter: &Scatter,
    ents: &[NewEntity],
    block_path: &Path,
    wad_path: &Path,
    base_wad: &str,
    block_index: u16,
) {
    let n = ents.len();
    println!("\n================= COVERAGE REPORT =================");
    println!("base vz.wad          : {base_wad} (PRISTINE — not modified)");
    println!("hosted in block      : {block_index} (layers_static, layer 0xB41FC710)");
    println!("overlay WAD          : {}", abspath(wad_path));
    println!("densified block      : {}", abspath(block_path));
    println!(
        "params               : radius={}m spacing={}m cap={} seed=0x{:X} max_slope={} exclude_radius={}m",
        cfg.radius, cfg.spacing, cfg.cap, cfg.seed, cfg.max_slope, cfg.exclude_radius
    );
    if let Some(r) = cfg.region {
        println!("region (requested)   : x[{},{}] z[{},{}]", r[0], r[2], r[1], r[3]);
    }
    println!(
        "scatter region        : x[{:.0},{:.0}] z[{:.0},{:.0}]",
        scatter.region[0], scatter.region[2], scatter.region[1], scatter.region[3]
    );
    println!("\nTOTAL ADDED          : {n}");

    // by species + tier
    let mut by_species: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_tier: BTreeMap<&str, usize> = BTreeMap::new();
    for a in &scatter.accepted {
        let s = &SPECIES[a.species];
        *by_species.entry(s.name).or_default() += 1;
        *by_tier.entry(s.tier.label()).or_default() += 1;
    }
    println!("\nby tier:");
    for (t, c) in &by_tier {
        println!("  {:<14} {:>6}  ({:.1}%)", t, c, 100.0 * *c as f64 / n as f64);
    }
    println!("by species:");
    for (s, c) in &by_species {
        println!("  {:<16} {:>6}  ({:.1}%)", s, c, 100.0 * *c as f64 / n as f64);
    }

    // by quadrant (x WEST-positive, z NORTH-positive)
    let mut quad: BTreeMap<&str, usize> = BTreeMap::new();
    for a in &scatter.accepted {
        let ns = if a.z >= 0.0 { "N" } else { "S" };
        let ew = if a.x >= 0.0 { "W" } else { "E" };
        *quad.entry(match (ns, ew) {
            ("N", "W") => "NW",
            ("N", "E") => "NE",
            ("S", "W") => "SW",
            _ => "SE",
        }).or_default() += 1;
    }
    println!("by world quadrant:");
    for (q, c) in &quad {
        println!("  {:<4} {:>6}", q, c);
    }

    // rejections
    let r = &scatter.rej;
    println!("\ncandidates evaluated (in treed areas): {}", scatter.candidates_total + r.unscanned + r.water + r.road + r.slope + r.building);
    println!("rejected by exclusion:");
    println!("  unscanned      {:>7}", r.unscanned);
    println!("  water          {:>7}", r.water);
    println!("  road           {:>7}", r.road);
    println!("  slope          {:>7}", r.slope);
    println!("  building/prop  {:>7}", r.building);
    println!("  spacing        {:>7}", r.spacing);
    println!("  over cap       {:>7}", r.capped);

    // densest sample coords: bucket accepted at 100 m, pick the 3 densest, emit a representative.
    let cell = 100.0f32;
    let mut buckets: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, a) in scatter.accepted.iter().enumerate() {
        buckets
            .entry(((a.x / cell).floor() as i32, (a.z / cell).floor() as i32))
            .or_default()
            .push(i);
    }
    let mut ranked: Vec<((i32, i32), Vec<usize>)> = buckets.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    println!("\ndense sample coordinates (fly here — x y z, {}m cells):", cell as i32);
    for (bk, idxs) in ranked.iter().take(3) {
        // representative = the entity nearest the bucket centre
        let cx = (bk.0 as f32 + 0.5) * cell;
        let cz = (bk.1 as f32 + 0.5) * cell;
        let best = idxs
            .iter()
            .min_by(|&&i, &&j| {
                let da = dist2(scatter.accepted[i].x, scatter.accepted[i].z, cx, cz);
                let db = dist2(scatter.accepted[j].x, scatter.accepted[j].z, cx, cz);
                da.partial_cmp(&db).unwrap()
            })
            .copied()
            .unwrap();
        let e = &ents[best];
        println!(
            "  {:>8.1} {:>7.1} {:>8.1}   ({} plants in this 100m cell)",
            e.pos[0], e.pos[1], e.pos[2], idxs.len()
        );
    }

    println!("\nNOTE: overlay NOT deployed. To try it, mount {} LAST (last-wins).", abspath(wad_path));
    println!("==================================================");
}

fn dist2(x: f32, z: f32, cx: f32, cz: f32) -> f32 {
    let (dx, dz) = (x - cx, z - cz);
    dx * dx + dz * dz
}

fn abspath(p: &Path) -> String {
    std::fs::canonicalize(p)
        .map(|c| c.display().to_string())
        .unwrap_or_else(|_| p.display().to_string())
}
