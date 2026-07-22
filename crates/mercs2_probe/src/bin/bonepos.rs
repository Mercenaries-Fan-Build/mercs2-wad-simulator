use mercs2_formats::skeleton::Skeleton;
fn main(){
    let a:Vec<String>=std::env::args().collect();
    let b=std::fs::read(&a[1]).unwrap();
    let s=Skeleton::from_block(&b).unwrap();
    // Arm chain = bones far from the body midline at torso height.
    let mut v:Vec<(f32,usize,[f32;3],i32)> = s.bones.iter().map(|x|{let p=x.bind_pos();(p[0].abs(),x.index,p,x.parent)}).collect();
    v.sort_by(|p,q| q.0.partial_cmp(&p.0).unwrap());
    let _=&v;
    // Walk from a hand up to the root: that IS the arm chain, no name table needed.
    let start: usize = a.get(2).and_then(|x|x.parse().ok()).unwrap_or(66);
    let mut cur = start as i32;
    let mut chain = Vec::new();
    while cur >= 0 {
        let b=&s.bones[cur as usize];
        chain.push((b.index, b.parent, b.bind_pos()));
        cur = b.parent;
    }
    chain.reverse();
    println!("chain root -> bone {start}:");
    for (i,par,p) in chain {
        println!("  bone {i:>3} parent {par:>3}  [{:>7.3}, {:>6.3}, {:>7.3}]",p[0],p[1],p[2]);
    }
}
