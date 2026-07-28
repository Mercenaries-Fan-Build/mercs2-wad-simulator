//! Lengthen a character's LEG BONES in its HIER, so the in-game result can settle whether the engine
//! takes bone lengths from the MODEL or from the CLIP.
//!
//! `bindtrans` measured that clips carry a per-bone translation track which is a constant copy of the
//! stock bind offsets. That makes two engine readings indistinguishable on every shipped asset, and
//! only a re-proportioned rig can tell them apart:
//!
//!   (A) lengths come from the model's own HIER  -> the character is simply TALLER, cleanly stretched.
//!   (B) lengths come from the clip              -> `WorldPose` stays stock while `InvBind` is long,
//!                                                  so `Skin != I` and the mesh is DRAGGED / TORN.
//!
//! The mesh is deliberately NOT touched. Under (A) linear-blend skinning follows the longer chain on
//! its own; under (B) the mismatch shows as breakage. Taller-and-clean vs torn is the whole readout.
//!
//! Edits are IN PLACE (no chunk changes size), so every offset in the container stays valid and only
//! the trailing CSUM is recomputed.
//!
//!   stretchrig <in.ucfx> <out.ucfx> [--scale 1.35]
//!
//! Input is a bare UCFX container as written by `mercs2_workshop --export-bundle` under `raw/`.
//! Ship the result with:
//!   mercs2_smuggler --source-wad vz.wad --extra-only --inject-extra "0xMODEL:19:<out.ucfx>" -o test.wad

use mercs2_formats::skeleton::{affine_inverse, mat3_det};

const STRIDE: usize = 176;
/// The CHILD of each segment we lengthen: scaling a bone's own local translation lengthens the
/// segment running from its PARENT to it. Shin -> lengthens the thigh; Foot -> lengthens the shin.
const TARGETS: [(u32, &str); 4] = [
    (0xA76C_9842, "Bone_LShin (lengthens L thigh)"),
    (0x0163_705C, "Bone_RShin (lengthens R thigh)"),
    (0x1226_F58D, "Bone_LFootBone1 (lengthens L shin)"),
    (0x3120_671B, "Bone_RFootBone1 (lengthens R shin)"),
];

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn rd_f32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
/// Row-major 4x4 at `o` (the convention `skeleton.rs` uses: translation in row 3).
fn rd_mat(b: &[u8], o: usize) -> [[f32; 4]; 4] {
    let mut m = [[0.0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            m[r][c] = rd_f32(b, o + (r * 4 + c) * 4);
        }
    }
    m
}
fn wr_mat(b: &mut [u8], o: usize, m: &[[f32; 4]; 4]) {
    for r in 0..4 {
        for c in 0..4 {
            b[o + (r * 4 + c) * 4..o + (r * 4 + c) * 4 + 4].copy_from_slice(&m[r][c].to_le_bytes());
        }
    }
}
fn matmul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut o = [[0.0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            o[r][c] = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    o
}
fn tr(m: &[[f32; 4]; 4]) -> [f32; 3] {
    [m[3][0], m[3][1], m[3][2]]
}
fn d3(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: stretchrig <in.ucfx> <out.ucfx> [--scale 1.35]");
        std::process::exit(2);
    }
    let scale: f32 = a
        .iter()
        .position(|x| x == "--scale")
        .and_then(|i| a.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.35);

    let mut u = std::fs::read(&a[1]).expect("read input");
    assert_eq!(&u[0..4], b"UCFX", "input is not a UCFX container");
    let data_off = rd_u32(&u, 4) as usize;
    let ndesc = rd_u32(&u, 16) as usize;

    // Locate the HIER leaf exactly as `Skeleton::from_block` does.
    let mut hier = None;
    for i in 0..ndesc {
        let ro = 20 + i * 20;
        if &u[ro..ro + 4] == b"HIER" {
            let u0 = rd_u32(&u, ro + 4);
            let size = rd_u32(&u, ro + 8) as usize;
            if u0 != 0xFFFF_FFFF && size >= STRIDE {
                hier = Some((data_off + u0 as usize, size / STRIDE));
                break;
            }
        }
    }
    let (base, n) = hier.expect("no HIER leaf chunk");
    println!("HIER: {n} nodes at +{base}, scale {scale}");

    let hash: Vec<u32> = (0..n).map(|r| rd_u32(&u, base + r * STRIDE)).collect();
    let parent: Vec<i32> = (0..n)
        .map(|r| {
            let p = u16::from_le_bytes([u[base + r * STRIDE + 8], u[base + r * STRIDE + 9]]);
            if p == 0xFFFF { -1 } else { p as i32 }
        })
        .collect();
    let world_of = |u: &[u8]| -> Vec<[[f32; 4]; 4]> {
        let mut w = vec![[[0.0f32; 4]; 4]; n];
        for r in 0..n {
            let l = rd_mat(u, base + r * STRIDE + 16);
            w[r] = if parent[r] < 0 { l } else { matmul(&l, &w[parent[r] as usize]) };
        }
        w
    };
    let before = world_of(&u);

    // --- scale the chosen bones' LOCAL translation in place ---
    let mut touched = Vec::new();
    for (h, label) in TARGETS {
        let Some(r) = (0..n).find(|&r| hash[r] == h) else {
            eprintln!("  !! bone 0x{h:08X} ({label}) not in this rig — skipped");
            continue;
        };
        let o = base + r * STRIDE + 16;
        let mut m = rd_mat(&u, o);
        let was = tr(&m);
        for c in 0..3 {
            m[3][c] *= scale;
        }
        wr_mat(&mut u, o, &m);
        touched.push(r);
        println!(
            "  node {r:>3} 0x{h:08X} {label}: |t| {:.1} mm -> {:.1} mm",
            (was[0] * was[0] + was[1] * was[1] + was[2] * was[2]).sqrt() * 1000.0,
            (m[3][0] * m[3][0] + m[3][1] * m[3][1] + m[3][2] * m[3][2]).sqrt() * 1000.0
        );
    }
    assert!(!touched.is_empty(), "no target bones found — wrong model?");

    // Legs hang DOWN from the hips, so lengthening them alone drives the feet below the origin and
    // the character stands knee-deep in the terrain — which would read as "broken" under either
    // hypothesis and waste the test. Raise the whole body by exactly the leg-length delta so the feet
    // return to where they were and the character is simply TALLER, standing correctly.
    //
    // The lift goes on `Bone_Root`, NOT `Bone_Hips`: measured on this rig, `Bone_Hips` and
    // `bone_spine1` are SIBLINGS under `Bone_Root` — the hips parent only the leg chains and the
    // torso hangs off spine1. Lifting the hips therefore raises the legs out of the body and leaves
    // the torso behind; only `Bone_Root` carries both.
    {
        let mid = world_of(&u);
        let find = |h: u32| (0..n).find(|&r| hash[r] == h);
        if let (Some(t), Some(s), Some(f), Some(root)) =
            (find(0x7685_3D12), find(0xA76C_9842), find(0x1226_F58D), find(0xFAEF_B386))
        {
            let leg_before = d3(tr(&before[t]), tr(&before[s])) + d3(tr(&before[s]), tr(&before[f]));
            let leg_after = d3(tr(&mid[t]), tr(&mid[s])) + d3(tr(&mid[s]), tr(&mid[f]));
            let lift = leg_after - leg_before;
            let o = base + root * STRIDE + 16;
            let mut m = rd_mat(&u, o);
            m[3][1] += lift;
            wr_mat(&mut u, o, &m);
            println!("  node {root:>3} 0xFAEFB386 Bone_Root: raised {:.1} mm to re-plant the feet", lift * 1000.0);
        }
    }

    // --- rewrite +80 inverse-bind for EVERY bone whose world transform moved ---
    //
    // A descendant of a lengthened bone moves even though its own local matrix is untouched, so the
    // set to fix is "world changed", not "we edited it". Leaving a stale inverse-bind here would
    // itself produce the tearing this test is trying to detect — the experiment has to be clean.
    let after = world_of(&u);
    let mut fixed = 0usize;
    for r in 0..n {
        if d3(tr(&before[r]), tr(&after[r])) < 1e-6 {
            continue;
        }
        let o = base + r * STRIDE + 80;
        let old = rd_mat(&u, o);
        if mat3_det(&old).abs() <= 1e-12 {
            continue; // never carried a real inverse-bind; leave it as it was
        }
        wr_mat(&mut u, o, &affine_inverse(&after[r]));
        fixed += 1;
    }
    println!("inverse-bind (+80) rewritten for {fixed} bones whose world transform moved");

    // --- report the resulting proportions ---
    let find = |h: u32| (0..n).find(|&r| hash[r] == h);
    if let (Some(t), Some(s), Some(f)) = (find(0x7685_3D12), find(0xA76C_9842), find(0x1226_F58D)) {
        let leg_b = d3(tr(&before[t]), tr(&before[s])) + d3(tr(&before[s]), tr(&before[f]));
        let leg_a = d3(tr(&after[t]), tr(&after[s])) + d3(tr(&after[s]), tr(&after[f]));
        println!("LEG (thigh+shin): {:.3} m -> {:.3} m", leg_b, leg_a);
    }
    if let Some(h) = find(0x705C_4508) {
        println!("head bone height: {:.3} m -> {:.3} m", tr(&before[h])[1], tr(&after[h])[1]);
    }
    let foot_drop = (0..n)
        .map(|r| tr(&after[r])[1])
        .fold(f32::INFINITY, f32::min);
    println!("lowest bone now at y = {foot_drop:.3} m (negative = below the origin plane)");

    // --- CSUM over everything up to the trailing tag ---
    let tag = u.windows(4).rposition(|w| w == b"CSUM").expect("no CSUM trailer");
    let csum = mercs2_formats::crc32::crc32_mercs2(&u[..tag]);
    u[tag + 4..tag + 8].copy_from_slice(&csum.to_le_bytes());
    println!("CSUM recomputed: 0x{csum:08X}");

    std::fs::write(&a[2], &u).expect("write output");
    println!("wrote {} ({} bytes)", a[2], u.len());
}
