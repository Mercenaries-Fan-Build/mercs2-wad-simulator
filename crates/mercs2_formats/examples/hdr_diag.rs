use mercs2_formats::ffcs::{load_ffcs_archive, parse_ffcs_header, FFCS_HEADER_SIZE};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
fn main() {
    let wad = std::env::args().nth(1).unwrap();
    let mut file = File::open(&wad).unwrap();
    let size = file.metadata().unwrap().len();
    let mut hdr = [0u8; FFCS_HEADER_SIZE];
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_exact(&mut hdr).unwrap();
    let rows = parse_ffcs_header(&hdr).unwrap();
    println!("FFCS chunks ({}):", rows.len());
    for r in &rows {
        println!("  {:?} offset=0x{:X} meta={}", std::str::from_utf8(&r.tag).unwrap_or("?"), r.offset, r.meta);
    }
    let a = load_ffcs_archive(&mut file, size).unwrap();
    println!("indx entries={} aset entries={}", a.indx.len(), a.aset.len());
    // Count type_ids
    use std::collections::BTreeMap;
    let mut m: BTreeMap<u32, usize> = BTreeMap::new();
    for e in &a.aset { *m.entry(e.type_id).or_default() += 1; }
    println!("type_id histogram: {:?}", m);
}
