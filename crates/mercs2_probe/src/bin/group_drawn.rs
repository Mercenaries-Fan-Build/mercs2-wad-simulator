//! Which groups of a model payload does the ENGINE actually draw?
//!
//! Not answerable from the mesh reader: it derives triangle counts from PRMT index spans, which the
//! injector's neutralisation leaves alone — only the draw count at PRMT+8 is zeroed. A block still
//! rendering the donor's leftover geometry therefore looks clean through the reader.
//!
//!   group_drawn <payload-or-block.bin> ...
use mercs2_formats::model_inject::group_draw_report;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    for path in &a[1..] {
        let b = std::fs::read(path).expect("read");
        // Accept either a raw UCFX payload or a 20-byte-header block.
        let p: &[u8] = if b.len() > 4 && &b[0..4] == b"UCFX" {
            &b
        } else {
            let n = u32::from_le_bytes(b[16..20].try_into().unwrap()) as usize;
            &b[20..20 + n]
        };
        let rep = group_draw_report(p).expect("report");
        let drawn: Vec<&(usize, u32, u32)> = rep.iter().filter(|r| r.1 > 0 && r.2 > 0).collect();
        println!("== {path}");
        for (gi, ic, mx) in rep.iter() {
            println!(
                "   group {gi:>2}  ibuf_ic {ic:>6}  max PRMT draw {mx:>6}  {}",
                if *ic == 0 || *mx == 0 { "NOT DRAWN" } else { "drawn" }
            );
        }
        println!("   {} of {} groups DRAW", drawn.len(), rep.len());
    }
}
