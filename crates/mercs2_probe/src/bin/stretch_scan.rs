//! Find vertices prone to STRETCH under animation: those whose weighted bones sit far apart, so a
//! pose that separates the bones stretches the vertex between them. Reports the worst, grouped by
//! body region (by Y and bone), to localize a "limb becomes a foot long" defect.
//!   stretch_scan <donor.block> <injected.block>
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let inj = std::fs::read(&a[2]).expect("inj");
    let skel = Skeleton::from_block(&std::fs::read(&a[1]).expect("donor")).expect("skel");
    let bp: Vec<[f32; 3]> = skel.bones.iter().map(|b| b.bind_pos()).collect();
    let n = u32::from_le_bytes(inj[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&inj[20..20 + n]).expect("meshes");

    let mut rows: Vec<(f32, [f32; 3], Vec<(u32, f32)>)> = Vec::new();
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() || m.tris.is_empty() { continue; }
        for i in 0..m.positions.len() {
            let tot: f32 = (0..4).map(|c| m.weights[i][c] as f32).sum();
            if tot <= 0.0 { continue; }
            // bones with real weight
            let bones: Vec<(u32, f32)> = (0..4).filter(|&c| m.weights[i][c] as f32 / tot > 0.10)
                .map(|c| (m.joints[i][c] as u32, m.weights[i][c] as f32 / tot)).collect();
            // max pairwise distance between weighted bones = stretch potential
            let mut spread = 0.0f32;
            for x in 0..bones.len() { for y in x+1..bones.len() {
                if let (Some(p), Some(q)) = (bp.get(bones[x].0 as usize), bp.get(bones[y].0 as usize)) {
                    let d = ((p[0]-q[0]).powi(2)+(p[1]-q[1]).powi(2)+(p[2]-q[2]).powi(2)).sqrt();
                    spread = spread.max(d);
                }
            }}
            rows.push((spread, m.positions[i], bones));
        }
    }
    rows.sort_by(|a,b| b.0.partial_cmp(&a.0).unwrap());
    let total = rows.len();
    let over = |t: f32| rows.iter().filter(|r| r.0 > t).count();
    println!("{} skinned verts. bone-spread (max dist between a vertex's weighted bones):", total);
    for t in [0.10, 0.15, 0.20, 0.30, 0.40] {
        println!("  > {:.2} m: {} verts ({:.2}%)", t, over(t), 100.0*over(t) as f32/total as f32);
    }
    println!("\n  worst 16 (spread, pos, weighted bones):");
    for (sp, p, bones) in rows.iter().take(16) {
        let bs: Vec<String> = bones.iter().map(|(b,w)| format!("{b}:{:.2}", w)).collect();
        println!("    spread {:.3} m at [{:.2},{:.2},{:.2}]  bones {}", sp, p[0],p[1],p[2], bs.join(" "));
    }
}
