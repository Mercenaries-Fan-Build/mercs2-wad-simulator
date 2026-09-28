//! Builder test fixtures shared by `build.rs` and `build_retail.rs`.

use mercs2_quartermaster::discover;
use std::path::{Path, PathBuf};

pub fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qm_build_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

pub fn shipment(dir: &Path, contributions: &str) -> discover::LoadedShipment {
    std::fs::write(
        dir.join("manifest.yaml"),
        format!(
            "format: 2
shipment: {{ name: test-shipment, version: 1.0.0, target: retail }}
contributions:
{contributions}"
        ),
    )
    .expect("write manifest");
    discover::open(dir).expect("open shipment")
}

/// A 1x1 PNG — enough to exist, deliberately the wrong size for any real target.
pub fn fake_png() -> Vec<u8> {
    solid_png(1, 1)
}

pub fn solid_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let data = vec![0x80u8; (width * height * 4) as usize];
        writer.write_image_data(&data).unwrap();
    }
    out
}

pub fn raw_shipment(
    dir: &Path,
    payload: &[u8],
    touches: &str,
    layer: &str,
) -> discover::LoadedShipment {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/state.block"), payload).unwrap();
    shipment(
        dir,
        &format!(
            "  - kind: raw\n    description: hand-built block\n    payload: src/state.block\n\
             \x20   target_layer: {layer}\n    touches: [{touches}]\n"
        ),
    )
}

/// A small, valid, uncompressed `GFX` movie: an AVM1 `DoAction` and a `GFx_ExporterInfo`, which is
/// the shape a GFx 2.x authoring tool emits.
///
/// Synthetic on purpose. A checked-in `.gfx` would be an opaque blob in the tree, and the property
/// under test is that whatever bytes go in come out again unchanged — which a fixture nobody can
/// read makes harder to believe, not easier.
pub fn tiny_gfx_movie() -> Vec<u8> {
    let mut body = Vec::new();
    body.push(0); // stage RECT: nbits = 0 in the top 5 bits, so the bounds are empty
    body.extend_from_slice(&(30u16 << 8).to_le_bytes()); // 30 fps
    body.extend_from_slice(&1u16.to_le_bytes()); // one frame
    let mut tag = |code: u16, b: &[u8]| {
        assert!(b.len() < 0x3F);
        body.extend_from_slice(&((code << 6) | b.len() as u16).to_le_bytes());
        body.extend_from_slice(b);
    };
    tag(1000, &[0x07, 0x02, 0x00, 0x00]); // GFx_ExporterInfo
    tag(12, &[0x00]); // DoAction: a bare End-of-actions
    tag(1, &[]); // ShowFrame
    tag(0, &[]); // End
    let mut file = Vec::new();
    file.extend_from_slice(b"GFX");
    file.push(8);
    file.extend_from_slice(&((8 + body.len()) as u32).to_le_bytes());
    file.extend_from_slice(&body);
    file
}

/// Pull the `cfx_pack` container back out of an emitted WAD and return `(name_hash, movie bytes)`.
pub fn read_back_movie(wad: &[u8]) -> (u32, Vec<u8>) {
    let contents = mercs2_formats::patch_wad::read_patch_wad(wad).expect("re-read the WAD");
    assert_eq!(contents.blocks.len(), 1);
    let block = &contents.blocks[0];

    // (1) The ASET row must be PRIMARY — low-16 `0xFFFF`. Any other value names a `_P001` rung one
    // level finer, and a movie has no LOD chain for such a rung to be, so it would dangle. `0x0000`
    // in particular is the M0001 HANG rather than "no rung".
    let row = &block.aset_entries[0];
    assert_eq!(
        row.u32_2 & 0xFFFF,
        0xFFFF,
        "a new movie must register as primary, not as a dangling LOD rung"
    );
    // `patch_wad::AsetEntry` names its words positionally; `u32_3` is the type id the reader side
    // dispatches on. It is what picks the loader, so a movie must resolve to the GFx one.
    assert_eq!(
        row.u32_3,
        mercs2_formats::types::TYPE_ID_CFX_PACK,
        "the row's type id is what picks the loader; a movie must dispatch to the GFx one"
    );

    // (2) A patch block is `[entry table][containers…]`, NOT a bare container. Handing over a raw
    // container makes the loader read the `UCFX` magic as an entry-table field: the WAD hashes fine
    // and is structurally nonsense.
    let decompressed = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&decompressed);
    assert_eq!(count, 1, "expected a single-entry block table");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].type_hash,
        mercs2_formats::types::TYPE_HASH_CFX_PACK
    );
    assert_eq!(
        &decompressed[20..24],
        b"UCFX",
        "the container must start AFTER the 20-byte entry table"
    );
    assert_eq!(
        entries[0].name_hash, row.asset_hash,
        "the row and the block must be talking about the same asset, or the lookup resolves to a \
         block that does not contain it"
    );

    let container = &decompressed[20..];
    let movie = mercs2_formats::ucfx::extract_chunk_body(container, b"data")
        .expect("the container must carry a `data` leaf");
    (entries[0].name_hash, movie)
}

/// A real Havok 5.5 clip, shared with `mercs2_formats`' own tests.
pub const ANIM_CLIP: &[u8] = include_bytes!("../../../mercs2_formats/tests/fixtures/anim_ks750_le.bin");

pub fn anim_trnm(count: u32) -> Vec<u8> {
    let mut t = count.to_le_bytes().to_vec();
    t.extend_from_slice(&0u32.to_le_bytes());
    for k in 0..count {
        t.extend_from_slice(&(0x2000 + k).to_le_bytes());
    }
    t
}

pub fn anim_tracks() -> u32 {
    mercs2_formats::animgroup::read_clip_header(ANIM_CLIP)
        .expect("fixture carries an hkaAnimation")
        .num_transform_tracks
}

/// Read the one animation block back out of a built WAD: its ASET row and its container's chunks.
pub fn read_back_animation(wad: &[u8]) -> (mercs2_formats::patch_wad::AsetEntry, u32, Vec<mercs2_formats::anim_container::Chunk>) {
    use mercs2_formats::types::{TYPE_HASH_ANIMATION, TYPE_ID_ANIMATION};
    let contents = mercs2_formats::patch_wad::read_patch_wad(wad).expect("re-read the WAD");
    assert_eq!(contents.blocks.len(), 1);
    let block = &contents.blocks[0];
    assert_eq!(block.aset_entries.len(), 1);
    let row = block.aset_entries[0].clone();
    assert_eq!(row.u32_1, 0xFFFF_FFFF, "no LOD rungs");
    assert_eq!(row.u32_2 & 0xFFFF, 0xFFFF, "primary");
    assert_eq!(row.u32_3, TYPE_ID_ANIMATION);
    let dec = mercs2_formats::sges::decompress_sges(&block.compressed_data).expect("sges");
    let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&dec);
    assert_eq!(count, 1);
    assert_eq!(entries[0].type_hash, TYPE_HASH_ANIMATION);
    assert_eq!(entries[0].name_hash, row.asset_hash);
    let chunks = mercs2_formats::anim_container::parse_container(&dec[20..])
        .expect("the container is the strict retail shape");
    (row, entries[0].name_hash, chunks)
}
