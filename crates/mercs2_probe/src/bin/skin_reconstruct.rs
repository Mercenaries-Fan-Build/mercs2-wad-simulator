//! Skin a shipped block exactly as the engine does and find where it deforms wrongly.
//!
//! The engine computes v' = sum_c w_c * (BoneWorld[b_c] * InvBind[b_c]) * v, with InvBind the stored
//! +80 record and b_c the GLOBAL bone the per-group palette expands each skin slot to. This runs that
//! on the block's OWN skeleton + palette, at bind (identity check) and under a synthetic elbow bend,
//! and reports the vertices that move furthest from where the SAME skinning moves the donor's
//! geometry — i.e. where the import deforms unlike retail.
//!
//!   skin_reconstruct <donor.block> <injected.block> [--bend DEG]
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::{mat4_mul, Skeleton};

type M4 = [[f32; 4]; 4];
fn ident() -> M4 { let mut m=[[0.0;4];4]; for i in 0..4 {m[i][i]=1.0;} m }
fn apply(m: &M4, v: [f32;3]) -> [f32;3] {
    [ v[0]*m[0][0]+v[1]*m[1][0]+v[2]*m[2][0]+m[3][0],
      v[0]*m[0][1]+v[1]*m[1][1]+v[2]*m[2][1]+m[3][1],
      v[0]*m[0][2]+v[1]*m[1][2]+v[2]*m[2][2]+m[3][2] ]
}
// row-major 4x4 inverse (general), enough for a rigid+scale bind matrix
fn inv4(m: &M4) -> M4 {
    let a: Vec<f64> = m.iter().flat_map(|r| r.iter().map(|&x| x as f64)).collect();
    let mut inv = [0.0f64; 16];
    let mut aug = a.clone();
    let mut res = { let mut e=[0.0;16]; for i in 0..4 {e[i*4+i]=1.0;} e };
    for col in 0..4 {
        let mut piv = col;
        for r in col+1..4 { if aug[r*4+col].abs() > aug[piv*4+col].abs() { piv=r; } }
        if piv!=col { for k in 0..4 { aug.swap(col*4+k, piv*4+k); res.swap(col*4+k, piv*4+k);} }
        let d = aug[col*4+col]; if d.abs()<1e-12 { return ident(); }
        for k in 0..4 { aug[col*4+k]/=d; res[col*4+k]/=d; }
        for r in 0..4 { if r!=col { let f=aug[r*4+col]; for k in 0..4 { aug[r*4+k]-=f*aug[col*4+k]; res[r*4+k]-=f*res[col*4+k]; } } }
    }
    for i in 0..16 { inv[i]=res[i]; }
    let mut o=[[0.0f32;4];4]; for i in 0..4 { for j in 0..4 { o[i][j]=inv[i*4+j] as f32; } } o
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bend: f32 = a.iter().position(|x| x=="--bend").and_then(|i| a.get(i+1)).and_then(|s| s.parse().ok()).unwrap_or(40.0);
    let inj = std::fs::read(&a[2]).expect("inj");
    let skel = Skeleton::from_block(&std::fs::read(&a[1]).expect("donor")).expect("skel");
    let nb = skel.bones.len();

    // Per bone: rest world (chained +16) and InvBind (inverse of the +80 bind, = inverse of world here).
    let restw: Vec<M4> = skel.bones.iter().map(|b| b.world).collect();
    let bindw: Vec<M4> = skel.bones.iter().map(|b| b.bind_world.unwrap_or(b.world)).collect();
    let invbind: Vec<M4> = bindw.iter().map(inv4).collect();

    // A synthetic pose that bends the arm bones about X at their joint. Everything else stays at rest.
    // Arm chain from the earlier bonepos walk: 57 upper / 58 elbow / 60 hand, and the mirror 79/80/82.
    let arm: std::collections::HashSet<usize> = [57usize,58,60,79,80,82].into_iter().collect();
    let (s,c) = bend.to_radians().sin_cos();
    let mut anim: Vec<M4> = restw.clone();
    for b in 0..nb {
        if arm.contains(&b) {
            // rotate about X around the bone's own rest origin
            let o = [restw[b][3][0], restw[b][3][1], restw[b][3][2]];
            let rot: M4 = [[1.0,0.0,0.0,0.0],[0.0,c,s,0.0],[0.0,-s,c,0.0],[o[0]-(o[1]*0.0+o[2]*0.0),o[1]-(o[1]*c-o[2]*s),o[2]-(o[1]*s+o[2]*c),1.0]];
            anim[b] = mat4_mul(&rot, &restw[b]);
        }
    }
    // Skin matrix per bone.
    let skinm = |world: &[M4]| -> Vec<M4> { (0..nb).map(|b| mat4_mul(&world[b], &invbind[b])).collect() };
    let m_bind = skinm(&restw);
    let m_pose = skinm(&anim);

    let n = u32::from_le_bytes(inj[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&inj[20..20+n]).expect("meshes");

    let (mut bind_err, mut nverts) = (0.0f64, 0usize);
    let mut worst_bind = 0.0f32;
    let mut arm_moved = 0usize; let mut arm_span = (f32::MAX, f32::MIN);
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() || m.tris.is_empty() { continue; }
        for i in 0..m.positions.len() {
            let v = m.positions[i];
            let tot: f32 = (0..4).map(|c| m.weights[i][c] as f32).sum();
            if tot<=0.0 { continue; }
            let skin = |mm: &[M4]| -> [f32;3] {
                let mut acc=[0.0f32;3];
                for c in 0..4 {
                    let w = m.weights[i][c] as f32/tot; if w<=0.0 {continue;}
                    let b = m.joints[i][c] as usize; if b>=nb {continue;}
                    let p = apply(&mm[b], v);
                    for k in 0..3 { acc[k]+=w*p[k]; }
                }
                acc
            };
            let vb = skin(&m_bind);
            let db = ((vb[0]-v[0]).powi(2)+(vb[1]-v[1]).powi(2)+(vb[2]-v[2]).powi(2)).sqrt();
            bind_err += db as f64; nverts+=1; worst_bind=worst_bind.max(db);
            // arm vertices: does the pose move them a sane amount?
            let dom = (0..4).max_by(|&x,&y| m.weights[i][x].cmp(&m.weights[i][y])).unwrap();
            if arm.contains(&(m.joints[i][dom] as usize)) {
                let vp = skin(&m_pose);
                let mv = ((vp[0]-vb[0]).powi(2)+(vp[1]-vb[1]).powi(2)+(vp[2]-vb[2]).powi(2)).sqrt();
                arm_moved+=1; arm_span.0=arm_span.0.min(mv); arm_span.1=arm_span.1.max(mv);
            }
        }
    }
    println!("{}", a[2]);
    println!("  BIND reconstruction error: mean {:.5} m, worst {:.5} m over {} verts", bind_err/nverts as f64, worst_bind, nverts);
    println!("    (should be ~0 -- non-zero means the block does NOT reconstruct its own bind pose)");
    println!("  arm-dominated verts: {arm_moved}, displacement under {bend} deg bend: {:.4}..{:.4} m", arm_span.0, arm_span.1);
}
