//! Ignored probe: verify the static watermap against the real installed `vz.wad`. Two things it
//! pins that a synthetic fixture cannot:
//!
//! 1. **The lookup resolves at all.** `watr` is a singleton — its ASET row is named for the resident
//!    group that holds it, and the chunk carries an unrelated authored name hash — so the name-keyed
//!    resolvers never find it. Asking for `pandemic_hash_m2("watermap")` as a *name* (that value is
//!    the TYPE hash) is the miss that made the boot print "no watermap in WAD (swim disabled)" on an
//!    archive that plainly ships one. Only `AssetSource::extract_singleton` reaches it.
//!
//! 2. **Layer 0 and Layer 1 are aligned.** With the header read as 36 bytes instead of 44, two header
//!    floats are consumed as height samples and the whole field slides two cells against the wet
//!    mask. Nothing errors; the census still looks right; 4,681 coastline cells (7% of the map) just
//!    end up on the wrong side of the waterline. The invariant below — a cell is wet **iff** its
//!    height is not the dry sentinel — is what catches that, and it holds for all 66,049 cells.
//!
//! ```text
//! cargo test -p mercs2_probe --test watermap_wad_probe -- --nocapture
//! ```

use mercs2_engine::asset::AssetSource;
use mercs2_engine::water_sim::{Watermap, CELL_SIZE_M, GRID_DIM, HEIGHT_MIN_M, OPEN_WATER_SURFACE_M};
use mercs2_formats::types::TYPE_HASH_WATERMAP;

/// Retail `watr` payload size, and the census the corrected parse yields.
const RETAIL_WATR_LEN: usize = 495_669;
const RETAIL_WET_CELLS: usize = 38_078;

#[test]
fn watermap_resolves_by_type_and_parses_aligned_from_vz_wad() {
    let Some(path) = mercs2_engine::wad::resolve_vz_wad(None) else {
        return eprintln!(
            "SKIPPING: no vz.wad discovered. Run `scripts/find-vz-wad.sh --write` or set MERCS2_GAME_DIR."
        );
    };
    let mut assets = AssetSource::discover(&path, &[]).expect("mount the WAD stack");

    // (1) The type hash is `pandemic_hash_m2("watermap")` — a TYPE, never an asset name.
    assert_eq!(mercs2_formats::hash::pandemic_hash_m2("watermap"), TYPE_HASH_WATERMAP);
    assert!(
        assets.extract_container_typed(TYPE_HASH_WATERMAP, TYPE_HASH_WATERMAP).is_err(),
        "the type hash is not an asset name — a name-keyed lookup must not accidentally work"
    );

    let (name_hash, container) =
        assets.extract_singleton(TYPE_HASH_WATERMAP).expect("watermap singleton in vz.wad");
    println!("watermap chunk 0x{name_hash:08X}, container {} B", container.len());

    let watr = mercs2_formats::ucfx::extract_chunk_body(&container, b"watr").expect("watr chunk");
    assert_eq!(watr.len(), RETAIL_WATR_LEN, "retail watr payload size");

    let wm = Watermap::from_watr_bytes(&watr).expect("parse watr");
    assert_eq!((wm.width(), wm.height()), (GRID_DIM, GRID_DIM));
    assert_eq!(wm.cell_size(), CELL_SIZE_M);

    // (2) The alignment invariant, cell for cell. This is the assertion that fails on a 36-byte
    // header — with 4,681 offenders, all of them shoreline.
    let mut misaligned = 0usize;
    for iz in 0..wm.height() {
        for ix in 0..wm.width() {
            let x = wm.origin_x + ix as f32 * wm.cell_size();
            let z = wm.origin_z + iz as f32 * wm.cell_size();
            let s = wm.sample(x, z);
            if s.is_water != (s.surface_height != HEIGHT_MIN_M) {
                misaligned += 1;
            }
        }
    }
    assert_eq!(misaligned, 0, "wet mask and height sentinel must agree on every cell");

    let wet = wm.wet_cell_count();
    println!("census: {wet} wet / {} dry", wm.width() * wm.height() - wet);
    assert_eq!(wet, RETAIL_WET_CELLS, "retail wet-cell census");

    // The open sea is at the recovered plateau, not Y=0 — the calibration the swim FSM depends on.
    for (x, z) in [(3000.0, 3000.0), (-3000.0, 3000.0), (3000.0, -3000.0)] {
        let s = wm.sample(x, z);
        assert!(s.is_water, "({x}, {z}) is open sea");
        assert!(
            (s.surface_height - OPEN_WATER_SURFACE_M).abs() < 0.01,
            "({x}, {z}) surface {} != plateau {OPEN_WATER_SURFACE_M}",
            s.surface_height
        );
    }

    // A renderable surface actually comes out of it, and every quad sits over water the query agrees
    // is water — the drawn shoreline and the swimmable shoreline are the same shoreline.
    let (pos, idx) = wm.surface_mesh();
    assert_eq!(pos.len(), wet * 4, "one quad per wet cell");
    assert_eq!(idx.len(), wet * 6);
    println!("surface mesh: {} verts / {} tris", pos.len(), idx.len() / 3);
    for p in pos.iter().step_by(997) {
        let (cx, cz) = (p[0], p[2]);
        // Pull the corner a hair toward its own cell centre before querying.
        let s = wm.sample(cx * 0.999, cz * 0.999);
        assert!(s.is_water, "mesh corner ({cx}, {cz}) draws water the query calls dry");
    }
}
