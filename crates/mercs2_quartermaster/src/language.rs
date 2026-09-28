//! What `add_language` forks from the game for a new language `<name>`, besides its string table.
//!
//! * **Fonts.** A language's fonts are `<language>_18` and `<language>_20`: `english_18` and
//!   `english_20` sit beside the english string table in `shell.wad`'s `english_P000_Q3` block, with
//!   their atlases `english_18_main` and `english_20_main`. A font names its atlas only by name hash, in
//!   its `MTRL` chunk (`Mtrl_Parse`, `FUN_00858790`, acquires `{hash, texture}` through
//!   `FUN_00873f20`); a texture's `NAME` chunk is copied into a buffer nothing reads
//!   (`FUN_00750a30`). So `<base>_18` / `<base>_20` fork to `<name>_18` / `<name>_20` with the one
//!   `MTRL` reference to `<base>_18_main` / `<base>_20_main` repointed, and the atlases ship
//!   unchanged under `<name>_18_main` / `<name>_20_main`.
//! * **Voice-over tables.** Retail Lua loads a `vo_*` bank as `<bank>.<language>`
//!   (`_GetLocalizedName`, `mrxsoundbanks.lua:80-87`), and every audio table of `English.wad` is
//!   registered under `m2("<bank>.english")` with the table's own bank hash `m2("<bank>")` inside.
//!   Each soundbank, sounddb and streamed wavebank of `English.wad` is shipped again under
//!   `m2("<bank>.<name>")`, computed from the bank hash it carries
//!   ([`mercs2_formats::hash::pandemic_hash_m2_extend`]); the streamed `vo_stream` wavebank's
//!   waves play from `Audios\vo_stream.<name>.pws`, which [`VO_STREAM_FROM`] is copied to. An
//!   embedded wavebank carries audio and is not shipped.

use mercs2_audio::sounddb::ASSET_TYPE_SOUNDDB;
use mercs2_audio::wave::WavebankFile;
use mercs2_formats::hash::{pandemic_hash_m2 as m2, pandemic_hash_m2_extend};
use mercs2_formats::types::{
    TYPE_HASH_FONT, TYPE_HASH_SOUNDBANK, TYPE_HASH_TEXTURE, TYPE_HASH_WAVEBANK, TYPE_ID_FONT, TYPE_ID_SOUNDBANK,
    TYPE_ID_TEXTURE, TYPE_ID_WAVEBANK,
};
use mercs2_formats::ucfx::{extract_data_chunk, parse_ucfx_tree, write_ucfx_tree};

use crate::game::GameStack;
use crate::sound::{sibling_wad, TYPE_ID_SOUNDDB};

/// The English voice stream, relative to the game folder.
pub const VO_STREAM_FROM: &str = "data/Audios/vo_stream.english.pws";

/// The copy of the English voice stream a language `name` plays its voice-over from.
pub fn vo_stream_to(name: &str) -> String {
    format!("data/Audios/vo_stream.{name}.pws")
}

/// One asset to ship: its name hash, type hash, ASET type id and UCFX container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name_hash: u32,
    pub type_hash: u32,
    pub type_id: u32,
    pub container: Vec<u8>,
}

/// The sizes of the fonts a language forks: `<base>_18` and `<base>_20`.
pub const FONT_SIZES: [&str; 2] = ["18", "20"];

/// `(font, atlas)` names for `language`: `<language>_18` / `<language>_18_main`, then `_20`.
pub fn font_names(language: &str) -> [(String, String); 2] {
    FONT_SIZES.map(|size| (format!("{language}_{size}"), format!("{language}_{size}_main")))
}

/// Every copy of `(hash, type)` in the game's carriers — the stack and `shell.wad` beside it — which
/// must agree byte for byte. `None` when no carrier has it.
fn carried(
    game: &mut GameStack,
    shell: &mut GameStack,
    hash: u32,
    type_hash: u32,
    type_id: u32,
    what: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut found: Option<Vec<u8>> = None;
    for stack in [game, shell] {
        if !stack.has_asset(hash, type_id) {
            continue;
        }
        let c = stack
            .container_for_asset(hash, type_hash, type_id)
            .ok_or_else(|| format!("{what} (0x{hash:08X}) has an ASET row but its block does not read"))?;
        match &found {
            Some(prev) if *prev != c => {
                return Err(format!(
                    "{what} (0x{hash:08X}) differs between vz.wad's stack and shell.wad, so there is \
                     no one copy to fork"
                ))
            }
            _ => found = Some(c),
        }
    }
    Ok(found)
}

/// The `shell.wad` beside the stack's `vz.wad`, opened on its own.
pub fn open_shell(game: &GameStack) -> Result<GameStack, String> {
    let vz = game.paths().first().map(|p| p.to_path_buf()).ok_or("the game stack is empty")?;
    GameStack::open(&[sibling_wad(&vz, "shell.wad")?]).map_err(|e| e.to_string())
}

/// Each font or atlas of `base` that no carrier has (M0219); empty when all four are there.
pub fn font_problems(game: &mut GameStack, base: &str) -> Result<Vec<String>, String> {
    let mut shell = open_shell(game)?;
    let mut out = Vec::new();
    for (font, atlas) in font_names(base) {
        for (name, type_hash, type_id, what) in [
            (&font, TYPE_HASH_FONT, TYPE_ID_FONT, "font"),
            (&atlas, TYPE_HASH_TEXTURE, TYPE_ID_TEXTURE, "font atlas"),
        ] {
            if carried(game, &mut shell, m2(name), type_hash, type_id, what)?.is_none() {
                out.push(format!("{what} {name:?} (0x{:08X}) is in neither vz.wad's stack nor shell.wad", m2(name)));
            }
        }
    }
    Ok(out)
}

/// Replace the one reference to `from` in a font container's `MTRL` chunk with `to`.
pub fn repoint_font(container: &[u8], from: u32, to: u32) -> Result<Vec<u8>, String> {
    let mut tree = parse_ucfx_tree(container)?;
    if write_ucfx_tree(&tree) != container {
        return Err("the font container does not re-serialize byte for byte".into());
    }
    let mut hits = 0usize;
    for node in tree.iter_mut().filter(|n| &n.tag == b"MTRL") {
        let body = node.body.as_mut().ok_or("the font's MTRL row owns no bytes")?;
        let at: Vec<usize> = (0..body.len().saturating_sub(3))
            .filter(|&i| u32::from_le_bytes([body[i], body[i + 1], body[i + 2], body[i + 3]]) == from)
            .collect();
        for i in &at {
            body[*i..*i + 4].copy_from_slice(&to.to_le_bytes());
        }
        hits += at.len();
    }
    if hits != 1 {
        return Err(format!("the font's MTRL references 0x{from:08X} {hits} times; one reference is repointed"));
    }
    Ok(write_ucfx_tree(&tree))
}

/// The fonts and atlases of `name`, forked from `base`'s.
pub fn fork_fonts(game: &mut GameStack, base: &str, name: &str) -> Result<Vec<Asset>, String> {
    let mut shell = open_shell(game)?;
    let mut out = Vec::new();
    for ((base_font, base_atlas), (font, atlas)) in font_names(base).into_iter().zip(font_names(name)) {
        let font_c = carried(game, &mut shell, m2(&base_font), TYPE_HASH_FONT, TYPE_ID_FONT, "font")?
            .ok_or_else(|| format!("[M0219] font {base_font:?} is in neither vz.wad's stack nor shell.wad"))?;
        let atlas_c = carried(game, &mut shell, m2(&base_atlas), TYPE_HASH_TEXTURE, TYPE_ID_TEXTURE, "font atlas")?
            .ok_or_else(|| format!("[M0219] font atlas {base_atlas:?} is in neither vz.wad's stack nor shell.wad"))?;
        let forked = repoint_font(&font_c, m2(&base_atlas), m2(&atlas)).map_err(|e| format!("font {base_font:?}: {e}"))?;
        out.push(Asset { name_hash: m2(&font), type_hash: TYPE_HASH_FONT, type_id: TYPE_ID_FONT, container: forked });
        out.push(Asset { name_hash: m2(&atlas), type_hash: TYPE_HASH_TEXTURE, type_id: TYPE_ID_TEXTURE, container: atlas_c });
    }
    Ok(out)
}

/// Every soundbank, sounddb and streamed wavebank of `English.wad` (beside the stack's `vz.wad`),
/// re-keyed from `m2("<bank>.english")` to `m2("<bank>.<name>")`. An entry whose name is not the
/// `.english` extension of the bank hash it carries is an error.
pub fn fork_vo_tables(game: &GameStack, name: &str) -> Result<Vec<Asset>, String> {
    let vz = game.paths().first().map(|p| p.to_path_buf()).ok_or("the game stack is empty")?;
    let mut english = GameStack::open(&[sibling_wad(&vz, "english.wad")?]).map_err(|e| e.to_string())?;
    let suffix = format!(".{name}");
    let mut out = Vec::new();
    for (type_id, type_hash) in [
        (TYPE_ID_SOUNDBANK, TYPE_HASH_SOUNDBANK),
        (TYPE_ID_SOUNDDB, ASSET_TYPE_SOUNDDB),
        (TYPE_ID_WAVEBANK, TYPE_HASH_WAVEBANK),
    ] {
        for hash in english.asset_hashes(type_id) {
            let container = english
                .container_for_asset(hash, type_hash, type_id)
                .ok_or_else(|| format!("English.wad table 0x{hash:08X} has an ASET row but its block does not read"))?;
            let body = extract_data_chunk(&container)
                .ok_or_else(|| format!("English.wad table 0x{hash:08X}: its container has no data chunk"))?;
            if body.len() < 8 {
                return Err(format!("English.wad table 0x{hash:08X} is {} bytes", body.len()));
            }
            let bank = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            if pandemic_hash_m2_extend(bank, ".english") != hash {
                return Err(format!(
                    "English.wad table 0x{hash:08X} carries bank hash 0x{bank:08X}, and is not named \
                     <bank>.english — it cannot be re-keyed for {name:?}"
                ));
            }
            if type_id == TYPE_ID_WAVEBANK {
                let wb = WavebankFile::parse(&body).map_err(|e| format!("English.wad wavebank 0x{hash:08X}: {e}"))?;
                if wb.stream_name.is_none() {
                    continue;
                }
            }
            out.push(Asset { name_hash: pandemic_hash_m2_extend(bank, &suffix), type_hash, type_id, container });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::ucfx::UcfxNode;

    #[test]
    fn font_names_follow_the_language() {
        assert_eq!(
            font_names("polski"),
            [
                ("polski_18".to_string(), "polski_18_main".to_string()),
                ("polski_20".to_string(), "polski_20_main".to_string())
            ]
        );
        assert_eq!(m2("english_18"), 0x093F_42E5);
        assert_eq!(m2("english_20"), 0x13C2_6ABC);
        assert_eq!(m2("english_18_main"), 0x6C3B_162F);
        assert_eq!(m2("english_20_main"), 0x40BA_4038);
    }

    fn font(mtrl: Vec<u8>) -> Vec<u8> {
        write_ucfx_tree(&[
            UcfxNode::leaf(*b"INFO", vec![1, 2, 3, 4]),
            UcfxNode::leaf(*b"CHAR", vec![9; 12]),
            UcfxNode::leaf(*b"MTRL", mtrl),
        ])
    }

    #[test]
    fn repointing_changes_the_one_atlas_reference_and_nothing_else() {
        let mut mtrl = Vec::new();
        for tex in [0x6C3B_162Fu32, 0x1111_1111, 0x2222_2222] {
            mtrl.extend_from_slice(&1u32.to_le_bytes());
            mtrl.extend_from_slice(&tex.to_le_bytes());
            mtrl.extend_from_slice(&0x7CCB_EC4Eu32.to_le_bytes());
        }
        let c = font(mtrl.clone());
        let out = repoint_font(&c, 0x6C3B_162F, 0xABCD_EF01).expect("repoints");
        let tree = parse_ucfx_tree(&out).unwrap();
        let mut expected = mtrl;
        expected[4..8].copy_from_slice(&0xABCD_EF01u32.to_le_bytes());
        assert_eq!(tree[2].body.as_deref(), Some(&expected[..]));
        assert_eq!(tree[0].body.as_deref(), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(tree[1].body.as_deref(), Some(&[9u8; 12][..]));
    }

    #[test]
    fn a_font_without_exactly_one_reference_is_refused() {
        let none = font(vec![0; 12]);
        assert!(repoint_font(&none, 0x6C3B_162F, 1).unwrap_err().contains("0 times"));
        let mut two = Vec::new();
        two.extend_from_slice(&0x6C3B_162Fu32.to_le_bytes());
        two.extend_from_slice(&0x6C3B_162Fu32.to_le_bytes());
        assert!(repoint_font(&font(two), 0x6C3B_162F, 1).unwrap_err().contains("2 times"));
    }

    #[test]
    fn the_voice_stream_copy_is_named_for_the_language() {
        assert_eq!(VO_STREAM_FROM, "data/Audios/vo_stream.english.pws");
        assert_eq!(vo_stream_to("polski"), "data/Audios/vo_stream.polski.pws");
    }
}

/// One block of whole containers: `[count][count × {name, type, 0, size}][containers]`, the layout
/// `mercs2_formats::ucfx::walk_decompressed_block` reads, with one primary ASET row per asset. None
/// of these assets has an LOD chain, so both rung halves carry the sentinel.
pub fn assets_block(path: String, assets: &[Asset]) -> Result<mercs2_formats::patch_wad::PatchBlock, String> {
    let mut raw = Vec::new();
    raw.extend_from_slice(&(assets.len() as u32).to_le_bytes());
    for a in assets {
        raw.extend_from_slice(&a.name_hash.to_le_bytes());
        raw.extend_from_slice(&a.type_hash.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        raw.extend_from_slice(&(a.container.len() as u32).to_le_bytes());
    }
    for a in assets {
        raw.extend_from_slice(&a.container);
    }
    let aset = assets
        .iter()
        .map(|a| mercs2_formats::patch_wad::AsetEntry::new(a.name_hash, 0xFFFF_FFFF, 0x0000_FFFF, a.type_id))
        .collect();
    mercs2_formats::patch_wad::PatchBlock::from_decompressed(&raw, path, aset, None)
}
