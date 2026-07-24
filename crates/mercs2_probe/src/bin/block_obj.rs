//! Dump a model block's skinned mesh to OBJ (positions only) so it can be measured against a
//! conformed import with the same yardstick. Comparing an import to the target BONES only proves it
//! tracks the skeleton; comparing it to the donor's own MESH is what shows a shape difference.
use mercs2_formats::model_cubeize::read_model_meshes;
use std::io::Write;
fn main() {
    let mut a = std::env::args().skip(1);
    let blk_path = a.next().expect("usage: block_obj <block.bin> <out.obj>");
    let out = a.next().expect("usage: block_obj <block.bin> <out.obj>");
    let block = std::fs::read(&blk_path).expect("read block");
    let ucfx_len = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&block[20..20 + ucfx_len]).expect("meshes");
    let mut f = std::io::BufWriter::new(std::fs::File::create(&out).expect("create"));
    let mut n = 0usize;
    let mut tris = 0usize;
    let mut base = 0usize;
    let mut faces: Vec<[usize; 3]> = Vec::new();
    for m in &meshes {
        for p in &m.positions {
            writeln!(f, "v {} {} {}", p[0], p[1], p[2]).unwrap();
            n += 1;
        }
        for t in &m.tris {
            faces.push([base + t[0] as usize + 1, base + t[1] as usize + 1, base + t[2] as usize + 1]);
            tris += 1;
        }
        base += m.positions.len();
    }
    for t in &faces {
        writeln!(f, "f {} {} {}", t[0], t[1], t[2]).unwrap();
    }
    println!("wrote {out}: {n} verts, {tris} tris from {} mesh(es)", meshes.len());
}
