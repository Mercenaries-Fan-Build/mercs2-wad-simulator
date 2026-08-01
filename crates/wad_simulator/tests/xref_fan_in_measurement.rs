//! F7 measurement: how SHARED are textures, really?
//!
//! M0006 ("a `replace_texture` target is shared by several materials — collateral reskin") was
//! parked with the note that M0009 ("no primary ASET row") may already cover it. This measures the
//! thing that decides it: the fan-in map the simulator now keeps (referenced hash → the distinct
//! assets that reference it). A hash with fan-in > 1 is shared; replacing it reskins every sharer.
//!
//! The measurement is the point — it runs against retail so the shape is observed, not assumed, and
//! reports the distribution so the M0006-vs-M0009 decision rests on numbers. It also guards the
//! mechanism the fix depends on: that the map keeps EVERY referrer (the old code kept only the
//! first, which would report a fan-in of 1 for everything and make the measurement meaningless).

use std::path::Path;

use wad_simulator::simulate::run_simulate;

fn vz_wad() -> Option<std::path::PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
}

#[test]
fn texture_fan_in_is_measured_over_retail() {
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    let report = run_simulate(Some(&wad), None).expect("simulate retail");

    // The map must be populated and must keep MULTIPLE referrers — the whole point of the change.
    assert!(!report.xref_fan_in.is_empty(), "no cross-references were recorded");
    let shared: Vec<(&u32, &Vec<String>)> =
        report.xref_fan_in.iter().filter(|(_, v)| v.len() > 1).collect();
    let max_fan = report.xref_fan_in.values().map(|v| v.len()).max().unwrap_or(0);

    eprintln!("\n════ TEXTURE / ASSET FAN-IN (F7) ════");
    eprintln!("distinct referenced hashes : {}", report.xref_fan_in.len());
    eprintln!("shared (fan-in > 1)        : {}", shared.len());
    eprintln!("max fan-in                 : {max_fan}");
    // A few of the most-shared hashes, so the M0006 case is concrete rather than a count.
    let mut top: Vec<(&u32, usize)> =
        report.xref_fan_in.iter().map(|(h, v)| (h, v.len())).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (h, n) in top.iter().take(8) {
        eprintln!("  0x{:08X} referenced by {n} distinct assets", **h);
    }
    eprintln!("═════════════════════════════════════\n");

    // The mechanism guard: retail HAS shared references, so the fan-in map is doing real work. If
    // this ever reads 0, the collector has regressed to first-referrer-only and M0006 cannot be
    // measured at all.
    assert!(
        max_fan > 1,
        "expected some shared references in retail — the fan-in collector may have regressed"
    );
}
