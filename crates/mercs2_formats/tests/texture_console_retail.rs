//! The Xbox bake's textures, converted, against PC retail.
//!
//! Every converted texture must hold what the PC reader takes from it: a resident rung with mips, each
//! rung's `BODY` exactly its masked mips, and no rung with nothing to give. Each platform compressed its
//! own blocks, so bytes are only comparable where both bakes kept the same DXT5 data. There every tail
//! mip has to match byte for byte, which pins the tail layout, and full mips have to match in alpha,
//! which pins the surface layout.
//!
//! Game-gated: built by the `retail` feature, reads the `vz_wad` and `xbox_vz_wad` of the repo-root
//! `.mercs2-local.toml`, and fails if one is absent.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use mercs2_formats::be_to_le::texture::{archive_textures, rung_of_path, texture_chunks, TextureTable};
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block};
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths::{local_config_vz_wad, local_config_xbox_vz_wad};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::texsize::linear_mip_chain_size;
use mercs2_formats::texture::texture_body;
use mercs2_formats::texture_encode::decode_dxt_surface;
use mercs2_formats::types::TYPE_HASH_TEXTURE;
use mercs2_formats::ucfx::block_chunk_spans;

fn block_size(fourcc: &[u8; 4]) -> usize {
    if fourcc == b"DXT1" {
        8
    } else {
        16
    }
}

#[test]
fn every_xbox_texture_converts_to_what_the_pc_reader_takes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pc = local_config_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));
    let xbox = local_config_xbox_vz_wad(root).unwrap_or_else(|e| panic!("{e}"));

    let mut f = File::open(&pc).unwrap();
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).unwrap();
    let mut twins: HashMap<(u32, String), Vec<u8>> = HashMap::new();
    for b in 0..ar.indx.len() {
        let path = ar.paths[b].to_ascii_lowercase();
        if !path.contains("_p000_") {
            continue;
        }
        let dec = decompress_block(&mut f, &ar.indx, b as u16).unwrap();
        for (e, r) in block_chunk_spans(&dec).unwrap() {
            if e.type_hash == TYPE_HASH_TEXTURE {
                twins.insert((e.name_hash, path.clone()), texture_body(&dec[r]).unwrap());
            }
        }
    }

    let doh = std::fs::read(&xbox).unwrap();
    let (_, rows) = parse_be_ffcs(&doh).unwrap();
    let indx = rows.iter().find(|r| r.tag == "INDX").unwrap();
    let entries = parse_be_indx(&doh, indx.offset as usize, indx.meta as usize);
    let row = rows.iter().find(|r| r.tag == "PTHS").unwrap();
    let paths = parse_be_pths(&doh, row.offset as usize, row.meta as usize);
    let table = TextureTable::from_doh(&doh, &entries, &paths, &archive_textures(&pc).unwrap()).unwrap();

    let (mut chunks, mut dropped) = (0usize, 0usize);
    let (mut tail_mips, mut full_mips, mut other_alpha) = (0usize, 0usize, 0usize);
    for (i, e) in entries.iter().enumerate() {
        let be = read_be_block(&doh, e).unwrap();
        for (hash, chunk) in texture_chunks(&be).unwrap() {
            if table.is_empty_stub(hash) {
                dropped += 1;
                continue;
            }
            let rung = rung_of_path(&paths[i]).unwrap();
            let converted = table.convert_chunk(hash, rung, chunk).unwrap_or_else(|err| panic!("0x{hash:08X}: {err}"));
            let tex = &table.textures[&hash];
            let masks = tex.pc_masks();
            assert_ne!(masks[0], 0, "0x{hash:08X} has no resident mips");
            assert_ne!(masks[rung], 0, "0x{hash:08X} rung {rung} has a chunk and no mask");
            chunks += 1;
            let format = tex.pc_format().unwrap();
            if !matches!(&format, b"DXT1" | b"DXT5") {
                continue;
            }
            let body = texture_body(&converted).unwrap();
            let faces = if tex.kind == 2 { 6 } else { 1 };
            let want: usize = (0..tex.mips)
                .filter(|m| masks[rung] >> m & 1 != 0)
                .map(|m| (tex.width >> m).max(1).div_ceil(4) * (tex.height >> m).max(1).div_ceil(4) * block_size(&format))
                .sum::<usize>()
                * faces;
            assert_eq!(body.len(), want, "0x{hash:08X} rung {rung}");
            assert!(body.len() <= linear_mip_chain_size(tex.width, tex.height, &format, tex.mips) * faces);

            // Same data on both sides: a DXT5 texture whose resident rung has the same split.
            if rung != 0 || tex.format != 0x54 || tex.flags != 0 || faces != 1 {
                continue;
            }
            let path = paths[i].to_ascii_lowercase();
            let Some(theirs) = twins.get(&(hash, path)).filter(|t| t.len() == body.len()) else { continue };
            let mut pos = 0;
            for mip in (0..tex.mips).filter(|m| masks[0] >> m & 1 != 0) {
                let (w, h) = ((tex.width >> mip).max(1), (tex.height >> mip).max(1));
                let (wb, hb) = (w.div_ceil(4), h.div_ceil(4));
                let end = pos + wb * hb * 16;
                let (ours, pc) = (&body[pos..end], &theirs[pos..end]);
                pos = end;
                if w.min(h) < 32 {
                    tail_mips += 1;
                    assert_eq!(ours, pc, "0x{hash:08X} tail mip {mip}");
                } else {
                    // PC clears the colour under fully transparent texels, so alpha is what compares.
                    let (a, b) = (decode_dxt_surface(b"DXT5", wb, hb, ours).unwrap(), decode_dxt_surface(b"DXT5", wb, hb, pc).unwrap());
                    full_mips += 1;
                    other_alpha += usize::from(a.iter().zip(&b).any(|(p, q)| p[3] != q[3]));
                }
            }
        }
    }
    eprintln!("{chunks} chunks converted, {dropped} left to the base game, {tail_mips} DXT5 tail mips equal PC, {other_alpha} of {full_mips} full mips differ in alpha");
    assert_eq!(chunks, 36_757);
    assert_eq!(dropped, 90);
    assert_eq!(tail_mips, 5_218);
    assert_eq!(full_mips, 4_753);
    // 0xF74D6682, 0xC891E027 and 0xB82AFCE8, by a mean of 0.10, 3.76 and 0.12 of 255.
    assert_eq!(other_alpha, 3);
}
