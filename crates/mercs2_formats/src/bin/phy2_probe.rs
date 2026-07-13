//! phy2_probe — dump the collision shapes in a model container's `PHY2` chunk.
//!
//! `PHY2` = `[u32 header][Havok 5.5 packfile][trailing engine wrapper]` (docs/format_reference.md
//! §15.2). Conforming a novel model into a donor leaves the DONOR's collision hull behind — so if
//! the new model is a different SIZE, the visible vehicle and the thing bullets hit disagree.
//!
//! Usage:  phy2_probe <container.ucfx>

use mercs2_formats::havok::{parse_phy2_body, Shape};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        eprintln!("usage: phy2_probe <container.ucfx>");
        std::process::exit(2);
    }
    let ucfx = std::fs::read(&args[0]).expect("read container");
    let data_off = u32::from_le_bytes(ucfx[4..8].try_into().unwrap()) as usize;
    let ndesc = u32::from_le_bytes(ucfx[16..20].try_into().unwrap()) as usize;
    let mut found = false;
    for i in 0..ndesc {
        let r = 20 + i * 20;
        let tag = &ucfx[r..r + 4];
        let u0 = u32::from_le_bytes(ucfx[r + 4..r + 8].try_into().unwrap());
        let size = u32::from_le_bytes(ucfx[r + 8..r + 12].try_into().unwrap()) as usize;
        if tag != b"PHY2" || u0 == 0xFFFF_FFFF {
            continue;
        }
        found = true;
        let body = &ucfx[data_off + u0 as usize..data_off + u0 as usize + size];
        println!("PHY2 chunk: {size} bytes");
        match parse_phy2_body(body) {
            Ok(pf) => {
                println!("  havok {}   packfile {} bytes", pf.version, pf.size);
                for (k, v) in &pf.class_counts {
                    println!("    {k:32} x{v}");
                }
                let mut mn = [f32::MAX; 3];
                let mut mx = [f32::MIN; 3];
                let mut nv = 0usize;
                for (h, hull) in pf.hulls().enumerate() {
                    let (mut a, mut b) = ([f32::MAX; 3], [f32::MIN; 3]);
                    for v in &hull.vertices {
                        for k in 0..3 {
                            a[k] = a[k].min(v[k]);
                            b[k] = b[k].max(v[k]);
                            mn[k] = mn[k].min(v[k]);
                            mx[k] = mx[k].max(v[k]);
                        }
                    }
                    nv += hull.vertices.len();
                    println!(
                        "  hull {h:2}: {:4} verts {:3} planes  bbox [{:6.2},{:6.2},{:6.2}]..[{:6.2},{:6.2},{:6.2}]",
                        hull.vertices.len(), hull.planes.len(), a[0], a[1], a[2], b[0], b[1], b[2]
                    );
                }
                for s in &pf.shapes {
                    match s {
                        Shape::Box { half_extents } => println!("  box half-extents {half_extents:?}"),
                        Shape::Mopp => println!("  MOPP BV-tree (static non-convex mesh)"),
                        Shape::Mesh => println!("  WpMeshShape16 (16-bit indexed collision mesh)"),
                        Shape::Other(n) => println!("  undecoded shape: {n}"),
                        Shape::Convex(_) => {}
                    }
                }
                if nv > 0 {
                    println!(
                        "  COLLISION EXTENT: [{:.2},{:.2},{:.2}]..[{:.2},{:.2},{:.2}]  ({:.2} x {:.2} x {:.2} m, {nv} verts)",
                        mn[0], mn[1], mn[2], mx[0], mx[1], mx[2],
                        mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2]
                    );
                }
            }
            Err(e) => println!("  parse failed: {e}"),
        }
    }
    if !found {
        println!("no PHY2 chunk in this container");
    }
}
