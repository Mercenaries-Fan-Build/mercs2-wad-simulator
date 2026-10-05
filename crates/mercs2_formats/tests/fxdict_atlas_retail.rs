//! The retail `fxdict` `0x86BF6C5B` and the `vfx` atlas `0x89E211AF` it indexes, both in the
//! resident block.
//!
//! * The fxdict container re-writes byte for byte; its keys are sorted by `i32` and unique.
//! * Every record is a whole-pixel rectangle of the 2048² atlas, inside it, and no two overlap.
//! * Read with `v` from the bottom (top edge `1 − v − h`), the rectangles cover the atlas's alpha:
//!   one rectangle covers no texel with alpha, and eight texels outside every rectangle carry alpha
//!   1. Read with `v` from the top, 46 rectangles cover none and 219,200 texels fall outside.
//! * The 512² square at (1536, 0) is under no rectangle and transparent at mips 0–5 and 9; at mips
//!   6–8 a few of its texels carry alpha 1 or 2.
//! * The atlas is a 2048² DXT5 of 10 mips named `vfx`, re-writes through `replace_body`, and no
//!   `MTRL` in `vz.wad` names it.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::fs::File;
use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::fxdict::{parse_fxdict_container, sort_fxdict, write_fxdict_container, FxRect};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::texture::{parse_mtrl, parse_texture_container, replace_body, texture_name, MtrlSource, TexFormat};
use mercs2_formats::texture_encode::decode_bc3_block;
use mercs2_formats::types::{
    TYPE_HASH_FONT, TYPE_HASH_FX_DICTIONARY, TYPE_HASH_LOWRES_TERRAIN, TYPE_HASH_MODEL, TYPE_HASH_TERRAIN_MESH,
    TYPE_HASH_TEXTURE,
};
use mercs2_formats::ucfx::{parse_block_entry_table, read_ucfx_rows, walk_decompressed_block};

const FXDICT: u32 = 0x86BF_6C5B;
const VFX: u32 = 0x89E2_11AF;
const SIZE: usize = 2048;

fn wad() -> std::path::PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"))
}

/// The fxdict and atlas containers, out of the block the atlas's ASET row names.
fn retail() -> (Vec<u8>, Vec<u8>) {
    let mut f = File::open(wad()).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS");
    let rows: Vec<_> = archive.aset.iter().filter(|a| a.asset_hash == VFX).collect();
    assert_eq!(rows.len(), 1, "one ASET row names the atlas");
    let bi = rows[0].block_index();
    assert_eq!(bi, 3185);
    assert!(archive.paths[bi as usize].ends_with(r"\VZ\resident_P000_Q3.block"), "{}", archive.paths[bi as usize]);
    let dec = decompress_block(&mut f, &archive.indx, bi).expect("decompress");
    let (_, entries) = parse_block_entry_table(&dec);
    let mut pos = 4 + 16 * entries.len();
    let (mut fxdict, mut atlas) = (Vec::new(), Vec::new());
    for e in &entries {
        let end = pos + e.chunk_size as usize;
        if e.name_hash == FXDICT && e.type_hash == TYPE_HASH_FX_DICTIONARY {
            fxdict.push(dec[pos..end].to_vec());
        }
        if e.name_hash == VFX && e.type_hash == TYPE_HASH_TEXTURE {
            atlas.push(dec[pos..end].to_vec());
        }
        pos = end;
    }
    assert_eq!((fxdict.len(), atlas.len()), (1, 1), "the resident block carries one fxdict and one atlas");
    (fxdict.pop().unwrap(), atlas.pop().unwrap())
}

/// The furthest a retail field lies from a whole pixel: each is written to six decimals, so it is
/// within 0.5e-6 × 2048 = 0.001 pixel of one.
const PIXEL_SLACK: f64 = 0.5e-6 * SIZE as f64;

/// A record as pixels `(x, y_top, w, h)`, with the top edge at `SIZE − v − h` or, with `from_top`,
/// at `v`. Every stored field must be a whole number of pixels, to within [`PIXEL_SLACK`].
fn pixels(r: &FxRect, from_top: bool) -> (usize, usize, usize, usize) {
    let px = |f: f32, what: &str| {
        let p = f as f64 * SIZE as f64;
        assert!((p - p.round()).abs() <= PIXEL_SLACK, "0x{:08X}: {what} {f} is {p} pixels, not a whole pixel", r.key);
        assert!(p.round() >= 0.0, "0x{:08X}: {what} is negative", r.key);
        p.round() as usize
    };
    let (u, v, w, h) = (px(r.u, "u"), px(r.v, "v"), px(r.w, "w"), px(r.h, "h"));
    assert!(v + h <= SIZE, "0x{:08X}: v + h is past the atlas", r.key);
    let top = if from_top { v } else { SIZE - v - h };
    (u, top, w, h)
}

/// Mip level `level` of a DXT5 body of a `SIZE`² texture, decoded to RGBA8.
fn decode_level(body: &[u8], level: usize) -> (usize, Vec<[u8; 4]>) {
    let offset: usize = (0..level).map(|l| (SIZE >> l) * (SIZE >> l)).sum();
    let n = SIZE >> level;
    let bw = n / 4;
    let mut out = vec![[0u8; 4]; n * n];
    for by in 0..bw {
        for bx in 0..bw {
            let o = offset + (by * bw + bx) * 16;
            let texels = decode_bc3_block(&body[o..o + 16]);
            for ty in 0..4 {
                for tx in 0..4 {
                    out[(by * 4 + ty) * n + bx * 4 + tx] = texels[ty * 4 + tx];
                }
            }
        }
    }
    (n, out)
}

#[test]
fn the_fxdict_re_writes_and_its_keys_are_signed_sorted_and_unique() {
    let (fxdict, _) = retail();
    let records = parse_fxdict_container(&fxdict).expect("the fxdict parses");
    assert_eq!(records.len(), 630);
    assert_eq!(write_fxdict_container(&records), fxdict, "the fxdict re-writes byte for byte");
    let mut sorted = records.clone();
    sort_fxdict(&mut sorted).expect("no key repeats");
    assert_eq!(sorted, records, "the records are in i32 key order");
    let unsigned = records.windows(2).all(|p| p[0].key < p[1].key);
    assert!(!unsigned, "the order is the signed one: some key with the top bit set comes first");
}

#[test]
fn every_record_is_a_whole_pixel_rectangle_inside_the_atlas_and_none_overlap() {
    let (fxdict, _) = retail();
    let records = parse_fxdict_container(&fxdict).unwrap();
    let rects: Vec<(u32, (usize, usize, usize, usize))> = records.iter().map(|r| (r.key, pixels(r, false))).collect();
    for (key, (x, y, w, h)) in &rects {
        assert!(*w > 0 && *h > 0, "0x{key:08X} is empty");
        assert!(x + w <= SIZE && y + h <= SIZE, "0x{key:08X} leaves the atlas");
    }
    for (i, (ka, (ax, ay, aw, ah))) in rects.iter().enumerate() {
        for (kb, (bx, by, bw, bh)) in &rects[i + 1..] {
            let apart = ax + aw <= *bx || bx + bw <= *ax || ay + ah <= *by || by + bh <= *ay;
            assert!(apart, "0x{ka:08X} and 0x{kb:08X} overlap");
        }
    }
}

/// Over mip 0: the keys of the records that cover no texel with alpha, and the texels outside every
/// record that carry alpha, as `(x, y, alpha)`.
fn coverage(records: &[FxRect], mip0: &[[u8; 4]], from_top: bool) -> (Vec<u32>, Vec<(usize, usize, u8)>) {
    let mut covered = vec![false; SIZE * SIZE];
    let mut empty = Vec::new();
    for r in records {
        let (x, y, w, h) = pixels(r, from_top);
        let (x1, y1) = ((x + w).min(SIZE), (y + h).min(SIZE));
        let mut any = false;
        for yy in y..y1 {
            for xx in x..x1 {
                covered[yy * SIZE + xx] = true;
                any |= mip0[yy * SIZE + xx][3] > 0;
            }
        }
        if !any {
            empty.push(r.key);
        }
    }
    let stray = (0..SIZE * SIZE)
        .filter(|&i| !covered[i] && mip0[i][3] > 0)
        .map(|i| (i % SIZE, i / SIZE, mip0[i][3]))
        .collect();
    (empty, stray)
}

#[test]
fn read_with_v_from_the_bottom_the_records_cover_the_atlas_alpha() {
    let (fxdict, atlas) = retail();
    let records = parse_fxdict_container(&fxdict).unwrap();
    let tex = parse_texture_container(&atlas).unwrap();
    let (_, mip0) = decode_level(&tex.all_mips, 0);
    let (empty, stray) = coverage(&records, &mip0, false);
    assert_eq!(empty, vec![0xDC5C_5324], "the one record over transparent texels, at (832, 512) 64²");
    let want: Vec<(usize, usize, u8)> = [(1344, 0), (1345, 0), (1346, 0), (1347, 0), (1352, 8), (1353, 8), (1354, 8), (1355, 8)]
        .into_iter()
        .map(|(x, y)| (x, y, 1))
        .collect();
    assert_eq!(stray, want);
    let (empty, stray) = coverage(&records, &mip0, true);
    assert_eq!((empty.len(), stray.len()), (46, 219_200), "read with v from the top");
}

#[test]
fn the_square_at_1536_0_of_side_512_is_under_no_record_and_transparent_at_mip_0() {
    let (fxdict, atlas) = retail();
    let records = parse_fxdict_container(&fxdict).unwrap();
    let (sx, sy, s) = (1536usize, 0usize, 512usize);
    for r in &records {
        let (x, y, w, h) = pixels(r, false);
        let apart = x + w <= sx || sx + s <= x || y + h <= sy || sy + s <= y;
        assert!(apart, "0x{:08X} lies over the square", r.key);
    }
    let tex = parse_texture_container(&atlas).unwrap();
    // Per mip: (the largest alpha in the square, how many of its texels carry alpha).
    let mut per_level = Vec::new();
    for level in 0..tex.mip_count as usize {
        let (n, texels) = decode_level(&tex.all_mips, level);
        let (x0, y0, side) = (sx >> level, sy >> level, (s >> level).max(1));
        let (mut max, mut lit) = (0u8, 0usize);
        for y in y0..y0 + side {
            for x in x0..x0 + side {
                let a = texels[y * n + x][3];
                max = max.max(a);
                lit += usize::from(a > 0);
            }
        }
        per_level.push((max, lit));
    }
    assert_eq!(per_level, vec![(0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (1, 4), (2, 1), (1, 4), (0, 0)]);
}

#[test]
fn the_atlas_is_a_2048_dxt5_of_10_mips_named_vfx_and_re_writes_through_replace_body() {
    let (_, atlas) = retail();
    assert_eq!(pandemic_hash_m2("vfx"), VFX);
    assert_eq!(texture_name(&atlas).as_deref(), Some("vfx"));
    let tex = parse_texture_container(&atlas).unwrap();
    assert_eq!((tex.width, tex.height, tex.format, tex.mip_count), (2048, 2048, TexFormat::Bc3, 10));
    assert_eq!(tex.all_mips.len(), 5_592_400);
    assert_eq!(replace_body(&atlas, &tex.all_mips).unwrap(), atlas);
}

#[test]
fn no_mtrl_in_vz_wad_names_the_atlas() {
    let mut f = File::open(wad()).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS");
    let mut materials = 0usize;
    let mut naming = Vec::new();
    for block in 0..archive.indx.len() {
        let dec = decompress_block(&mut f, &archive.indx, block as u16).expect("decompress block");
        let (parsed, _) = walk_decompressed_block(&dec, "block");
        for (entry, c) in parsed.entries.iter().zip(parsed.containers.iter()) {
            let Ok(rows) = read_ucfx_rows(c) else { continue };
            if !rows.iter().any(|r| &r.tag == b"MTRL") {
                continue;
            }
            let source = match entry.type_hash {
                TYPE_HASH_MODEL => MtrlSource::Model,
                TYPE_HASH_TERRAIN_MESH => MtrlSource::TerrainMesh,
                TYPE_HASH_FONT => MtrlSource::Font,
                TYPE_HASH_LOWRES_TERRAIN => MtrlSource::LowResTerrain,
                0x600B_904E => MtrlSource::Scrub,
                other => panic!("block {block}: 0x{:08X} of type 0x{other:08X} has an MTRL", entry.name_hash),
            };
            for m in parse_mtrl(c, source).unwrap_or_else(|e| panic!("block {block}: {e}")) {
                materials += 1;
                if m.textures.contains(&VFX) {
                    naming.push(format!("block {block}: 0x{:08X}", entry.name_hash));
                }
            }
        }
    }
    eprintln!("{materials} materials read");
    assert!(materials > 0);
    assert!(naming.is_empty(), "MTRLs name the atlas: {naming:?}");
}
