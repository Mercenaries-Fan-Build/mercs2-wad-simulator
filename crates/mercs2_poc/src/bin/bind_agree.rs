//! Does this model's HIER stored inverse-bind (+80) agree with its chained local matrices (+16)?
//!
//! The engine skins `v' = W_b(t) * invBind_b * v`, so the mesh MUST be authored in the pose that
//! `invBind` inverts. char_skin conforms geometry onto `Skeleton::bind_pos()` (+80). Where +80
//! disagrees with the +16 chain, the two candidate "bind poses" differ — geometry conformed to one
//! and skinned by the other looks plausible at rest and tears as soon as bones move.
//!
//!   bind_agree <block.bin>

use mercs2_formats::skeleton::Skeleton;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let block = std::fs::read(&a[1]).expect("read block");
    let skel = Skeleton::from_block(&block).expect("skeleton");

    let mut disagree = 0usize;
    let mut worst = (0usize, 0.0f64);
    for (i, b) in skel.bones.iter().enumerate() {
        // chained-+16 world translation vs stored-+80 bind translation
        let w = [b.world[3][0] as f64, b.world[3][1] as f64, b.world[3][2] as f64];
        let p = b.bind_pos();
        let p = [p[0] as f64, p[1] as f64, p[2] as f64];
        let d = ((w[0] - p[0]).powi(2) + (w[1] - p[1]).powi(2) + (w[2] - p[2]).powi(2)).sqrt();
        if d > 1e-4 {
            disagree += 1;
            println!(
                "  bone {i:3} hash 0x{:08X}  off {:.4} m   default[{:.3},{:.3},{:.3}] bind[{:.3},{:.3},{:.3}]",
                b.name_hash, d, w[0], w[1], w[2], p[0], p[1], p[2]
            );
            if d > worst.1 {
                worst = (i, d);
            }
        }
    }
    println!("{}: {} bones", a[1], skel.bones.len());
    println!("  +80 vs chained +16 disagree: {disagree} / {}", skel.bones.len());
    if disagree > 0 {
        println!(
            "  worst: bone {} off by {:.4} m  -> the two candidate bind poses are NOT the same",
            worst.0, worst.1
        );
    } else {
        println!("  identical — this model's stored bind IS its default pose");
    }
}
