//! Does the donor expose a usable BIND ORIENTATION per bone? `char_skin` section 6c (distal-hand FK)
//! silently does nothing for any bone whose `TargetBone::rot` is `None`, so a donor without bind
//! matrices would make the whole pass a no-op. Reports coverage overall and for the finger chain.
use mercs2_formats::char_skin::TargetSkeleton;
use mercs2_formats::skeleton::Skeleton;
fn main() {
    let p = std::env::args().nth(1).expect("usage: rotcheck <donor.block>");
    let blk = std::fs::read(&p).expect("read");
    let sk = Skeleton::from_block(&blk).expect("skeleton");
    let ts = TargetSkeleton::from_skeleton(&sk);
    let total = ts.bones.len();
    let with = ts.bones.iter().filter(|b| b.rot.is_some()).count();
    let raw_bind = sk.bones.iter().filter(|b| b.bind_world.is_some()).count();
    println!("bones {total}: bind_world present {raw_bind}, TargetBone.rot Some {with}");
    // the hand/finger chain the FK pass targets
    for npc in [46u32, 67, 48, 51, 54, 57, 60, 69, 72, 75, 78, 81] {
        if let Some(h) = ts.index_by_canonical(npc) {
            let b = ts.bones.iter().find(|b| b.i == h).unwrap();
            println!("  canonical {npc:>3} -> hier {h:>3}  rot={}", if b.rot.is_some() { "Some" } else { "NONE" });
        } else {
            println!("  canonical {npc:>3} -> (donor lacks this bone)");
        }
    }
}
