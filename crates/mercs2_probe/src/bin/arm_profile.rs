//! Weight profile ALONG an arm chain: where does each bone hand off to the next, donor vs import?
//!
//! A clean elbow needs the weight to cross over from the upper-arm bone to the forearm bone in a
//! short zone AT the joint. If the import's crossover sits at a different point along the arm than
//! the donor's, the mesh creases in the wrong place and the limb reads as a broken bone under
//! animation. Centroid-of-a-bone's-domain (bind_outliers) is a proxy for this and a poor one; this
//! measures the handoff directly.
//!
//! Vertices are projected onto the straightened arm axis (clavicle -> ... -> hand) and binned by arc
//! length. Per bin, the mean weight on each chain bone is printed for donor and import side by side.
//!
//!   arm_profile <donor.block> <injected.block> <bone_a,bone_b,...>   (e.g. 55,57,58,60)
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;

fn verts(block: &[u8]) -> Vec<([f64; 3], Vec<(u32, f64)>)> {
    let n = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&block[20..20 + n]).expect("meshes");
    let mut out = Vec::new();
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() || m.tris.is_empty() {
            continue;
        }
        for i in 0..m.positions.len() {
            let tot: f64 = (0..4).map(|c| m.weights[i][c] as f64).sum();
            if tot <= 0.0 {
                continue;
            }
            let infl = (0..4).filter(|&c| m.weights[i][c] > 0).map(|c| (m.joints[i][c] as u32, m.weights[i][c] as f64 / tot)).collect();
            let p = m.positions[i];
            out.push(([p[0] as f64, p[1] as f64, p[2] as f64], infl));
        }
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let chain: Vec<u32> = a[3].split(',').filter_map(|s| s.parse().ok()).collect();
    let donor = std::fs::read(&a[1]).expect("donor");
    let inj = std::fs::read(&a[2]).expect("inj");
    let skel = Skeleton::from_block(&donor).expect("skel");
    let bp: Vec<[f64; 3]> = skel.bones.iter().map(|b| { let p = b.bind_pos(); [p[0] as f64, p[1] as f64, p[2] as f64] }).collect();

    // Cumulative arc length of each chain joint, and a projector: nearest point on the polyline.
    let jp: Vec<[f64; 3]> = chain.iter().map(|&b| bp[b as usize]).collect();
    let mut arc = vec![0.0f64];
    for i in 1..jp.len() {
        let d = (0..3).map(|k| (jp[i][k] - jp[i - 1][k]).powi(2)).sum::<f64>().sqrt();
        arc.push(arc[i - 1] + d);
    }
    let total = *arc.last().unwrap();
    let project = |p: [f64; 3]| -> (f64, f64) {
        let (mut bs, mut bd) = (0.0, f64::MAX);
        for i in 0..jp.len() - 1 {
            let (u, v) = (jp[i], jp[i + 1]);
            let e = [v[0] - u[0], v[1] - u[1], v[2] - u[2]];
            let el = e[0] * e[0] + e[1] * e[1] + e[2] * e[2];
            let t = if el > 0.0 { (((p[0]-u[0])*e[0]+(p[1]-u[1])*e[1]+(p[2]-u[2])*e[2]) / el).clamp(0.0, 1.0) } else { 0.0 };
            let c = [u[0]+t*e[0], u[1]+t*e[1], u[2]+t*e[2]];
            let d = (0..3).map(|k| (p[k]-c[k]).powi(2)).sum::<f64>().sqrt();
            if d < bd { bd = d; bs = arc[i] + t * (arc[i+1]-arc[i]); }
        }
        (bs, bd)
    };

    const NB: usize = 12;
    let radius = 0.14; // stay within the arm, not the torso
    let tally = |vs: &[([f64;3],Vec<(u32,f64)>)]| {
        let mut sum = vec![vec![0.0f64; chain.len()]; NB];
        let mut cnt = vec![0.0f64; NB];
        for (p,infl) in vs {
            let (s,d) = project(*p);
            if d > radius { continue; }
            let bin = ((s/total*NB as f64).floor() as usize).min(NB-1);
            cnt[bin]+=1.0;
            for (ci,&cb) in chain.iter().enumerate() {
                let w: f64 = infl.iter().filter(|(b,_)| *b==cb).map(|(_,w)| *w).sum();
                sum[bin][ci]+=w;
            }
        }
        (sum,cnt)
    };
    let (sd,cd)=tally(&verts(&donor));
    let (si,ci)=tally(&verts(&inj));

    println!("arm chain {:?}, arc {total:.3} m, {NB} bins from clavicle(0) to hand(1)", chain);
    print!("  bin  arc   ");
    for b in &chain { print!(" b{:<3}(d/i)   ", b); }
    println!();
    for bin in 0..NB {
        print!("  {bin:>2}  {:.3} ", bin as f64/NB as f64*total);
        for ci_ in 0..chain.len() {
            let d = if cd[bin]>0.0 { sd[bin][ci_]/cd[bin] } else { 0.0 };
            let i = if ci[bin]>0.0 { si[bin][ci_]/ci[bin] } else { 0.0 };
            let mark = if (d-i).abs()>0.15 {"*"} else {" "};
            print!(" {:.2}/{:.2}{}  ", d, i, mark);
        }
        println!();
    }
}
