//! Write side of the Havok-5.5.0-r1 little-endian (Win32) packfile — the serializer half of the
//! native animation encoder, so a clip can be rebuilt into a native wavelet packfile WITHOUT the
//! Havok Content Tools DLL.
//!
//! The format is fully pinned (the reverse of [`crate::havok::parse_packfile_raw`]): a proven-good
//! re-encoder (AssetCc2 5.5) reproduces a real Mercs2 anim packfile byte-for-byte bar ONE mantissa
//! byte of `m_duration`. So this module has essentially zero layout freedom — the gate is a byte
//! diff against the oracle. All multi-byte integers are little-endian.
//!
//! Header layout (`hkPackfileHeader`, 0x00..0x40), field offsets:
//!   0x00 magic[8]=57E0E057 10C0C010 · 0x08 userTag=0 · 0x0C fileVersion=5 ·
//!   0x10 layoutRules[4]=04 01 00 01 (ptr=4, LE=1, reusePad=0, emptyBase=1 — the `--rules4101`) ·
//!   0x14 numSections=3 · 0x18 contentsSectionIndex=2 (__data__) · 0x1C contentsSectionOffset=0 ·
//!   0x20 contentsClassNameSectionIndex=0 · 0x24 contentsClassNameSectionOffset=<root cnoff> ·
//!   0x28 contentsVersion[16]="Havok-5.5.0-r1\0"+0xFF fill · 0x38 pad[8]=0xFF.

/// The little-endian Havok packfile magic (two words, each word-palindromic so a byteswap of the
/// header word survives).
pub const HAVOK_MAGIC_LE: [u8; 8] = [0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10];

/// The layout-rules quad for Win32 LE — `--rules4101`: 4-byte pointer, little-endian, no
/// reuse-padding optimization, empty-base-class optimization on. These four bytes ARE the contract
/// the retail loader's cross-platform guard checks; changing them makes the game reject the file.
pub const LAYOUT_RULES_WIN32_LE: [u8; 4] = [0x04, 0x01, 0x00, 0x01];

pub const CONTENTS_VERSION: &str = "Havok-5.5.0-r1";

/// Serialize the 0x40-byte packfile header. `cn_root_off` is the classnames-body-relative offset of
/// the ROOT object's class-name string (for an anim packfile the root is `hkaAnimationContainer`).
pub fn write_packfile_header(cn_root_off: u32) -> [u8; 0x40] {
    let mut h = [0u8; 0x40];
    h[0x00..0x08].copy_from_slice(&HAVOK_MAGIC_LE);
    // 0x08 userTag = 0 (already zero)
    h[0x0C..0x10].copy_from_slice(&5u32.to_le_bytes()); // fileVersion
    h[0x10..0x14].copy_from_slice(&LAYOUT_RULES_WIN32_LE);
    h[0x14..0x18].copy_from_slice(&3u32.to_le_bytes()); // numSections
    h[0x18..0x1C].copy_from_slice(&2u32.to_le_bytes()); // contentsSectionIndex (__data__)
    // 0x1C contentsSectionOffset = 0 (already zero)
    // 0x20 contentsClassNameSectionIndex = 0 (already zero)
    h[0x24..0x28].copy_from_slice(&cn_root_off.to_le_bytes());
    // 0x28..0x40 contentsVersion + pad: the version string, nul, then 0xFF to the end.
    let v = CONTENTS_VERSION.as_bytes();
    h[0x28..0x28 + v.len()].copy_from_slice(v);
    h[0x28 + v.len()] = 0; // nul terminator
    for b in &mut h[0x28 + v.len() + 1..0x40] {
        *b = 0xFF;
    }
    h
}

/// Serialize the `__classnames__` section body. Each class is a record
/// `[signature u32 LE][0x09 separator][name ASCII][0x00]`, packed with no inter-record alignment;
/// then a `0xFFFFFFFF` terminator and `0xFF` padding to a 16-byte boundary (the section end is
/// 16-aligned). Returns `(body, name_offsets)` where `name_offsets[i]` is the body-relative offset
/// of record `i`'s NAME string (record start + 5) — the identity the header `cnoff` and every
/// virtual fixup reference.
pub fn write_classnames(classes: &[(u32, &str)]) -> (Vec<u8>, Vec<u32>) {
    let mut body = Vec::new();
    let mut name_offsets = Vec::with_capacity(classes.len());
    for &(sig, name) in classes {
        body.extend_from_slice(&sig.to_le_bytes());
        body.push(0x09);
        name_offsets.push(body.len() as u32); // the NAME string offset
        body.extend_from_slice(name.as_bytes());
        body.push(0x00);
    }
    body.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    while body.len() % 16 != 0 {
        body.push(0xFF);
    }
    (body, name_offsets)
}

/// One 48-byte `hkPackfileSectionHeader`: a 19-byte tag, a `0xFF` marker, then seven body-relative
/// u32 offsets (local/global/virtual/exports/imports fixups + endOffset), preceded by the absolute
/// body start. When a section has no fixups, all six offset fields equal `end`.
#[allow(clippy::too_many_arguments)]
pub fn write_section_header(
    tag: &str,
    abs: u32,
    local: u32,
    global: u32,
    virt: u32,
    exports: u32,
    imports: u32,
    end: u32,
) -> [u8; 48] {
    let mut h = [0u8; 48];
    let t = tag.as_bytes();
    h[..t.len().min(19)].copy_from_slice(&t[..t.len().min(19)]);
    h[19] = 0xFF; // constant marker at tag+19
    h[20..24].copy_from_slice(&abs.to_le_bytes());
    h[24..28].copy_from_slice(&local.to_le_bytes());
    h[28..32].copy_from_slice(&global.to_le_bytes());
    h[32..36].copy_from_slice(&virt.to_le_bytes());
    h[36..40].copy_from_slice(&exports.to_le_bytes());
    h[40..44].copy_from_slice(&imports.to_le_bytes());
    h[44..48].copy_from_slice(&end.to_le_bytes());
    h
}

/// The `__data__` section's object storage plus its fixup lists — everything the assembler needs to
/// lay out the tail of the packfile. `body` is the object region (objects + array/buffer storage),
/// already padded to a 16-byte boundary (it becomes the local-fixup offset). Fixups are as in the
/// [`write_*_fixups`] functions and MUST be sorted ascending by `src`.
pub struct DataSection {
    pub body: Vec<u8>,
    pub local: Vec<(u32, u32)>,
    pub global: Vec<(u32, u32, u32)>,
    pub virt: Vec<(u32, u32, u32)>,
}

/// Assemble a whole Havok 5.5 LE packfile from the classnames, the root class-name offset, and the
/// `__data__` section. Computes every section offset per the pinned format (`__classnames__` at
/// 0xD0; empty `__types__`; `__data__` right after; fixup tables at the tail) and emits the bytes.
/// `__classnames__` starts at 0xD0 because the header (0x40) + three 48-byte section headers (0x90)
/// land the classnames body there.
pub fn write_packfile(classes: &[(u32, &str)], cn_root_off: u32, data: &DataSection) -> Vec<u8> {
    let (cn_body, _) = write_classnames(classes);
    const CN_ABS: u32 = 0xD0;
    let cn_end = cn_body.len() as u32;
    let data_abs = CN_ABS + cn_end; // __types__ is empty, so it shares this abs

    let local = write_local_fixups(&data.local);
    let global = write_global_fixups(&data.global);
    let virt = write_virtual_fixups(&data.virt);
    let lf = data.body.len() as u32;
    let gf = lf + local.len() as u32;
    let vf = gf + global.len() as u32;
    let end = vf + virt.len() as u32; // exports = imports = end (both empty)

    let mut out = Vec::with_capacity((data_abs + end) as usize);
    out.extend_from_slice(&write_packfile_header(cn_root_off));
    out.extend_from_slice(&write_section_header(
        "__classnames__",
        CN_ABS,
        cn_end,
        cn_end,
        cn_end,
        cn_end,
        cn_end,
        cn_end,
    ));
    out.extend_from_slice(&write_section_header("__types__", data_abs, 0, 0, 0, 0, 0, 0));
    out.extend_from_slice(&write_section_header(
        "__data__", data_abs, lf, gf, vf, end, end, end,
    ));
    out.extend_from_slice(&cn_body);
    // __types__ has no body.
    out.extend_from_slice(&data.body);
    out.extend_from_slice(&local);
    out.extend_from_slice(&global);
    out.extend_from_slice(&virt);
    out
}

/// Pad a fixup table to a 16-byte boundary with `0xFF` (the `0xFFFFFFFF` terminator/fill). A table
/// that already fills its 16-slot exactly (e.g. the 2-entry local table) gets no extra bytes.
fn pad16_ff(b: &mut Vec<u8>) {
    while b.len() % 16 != 0 {
        b.push(0xFF);
    }
}

/// Local fixups — `{u32 src, u32 dst}`: relocate a pointer field to another offset WITHIN the same
/// `__data__` section. Emit sorted ascending by `src`.
pub fn write_local_fixups(fixups: &[(u32, u32)]) -> Vec<u8> {
    let mut b = Vec::new();
    for &(src, dst) in fixups {
        b.extend_from_slice(&src.to_le_bytes());
        b.extend_from_slice(&dst.to_le_bytes());
    }
    pad16_ff(&mut b);
    b
}

/// Global fixups — `{u32 src, u32 sec, u32 dst}`: relocate an OBJECT pointer to `section[sec] + dst`
/// (`sec = 2` = `__data__` for intra-file refs). Emit sorted ascending by `src`.
pub fn write_global_fixups(fixups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut b = Vec::new();
    for &(src, sec, dst) in fixups {
        b.extend_from_slice(&src.to_le_bytes());
        b.extend_from_slice(&sec.to_le_bytes());
        b.extend_from_slice(&dst.to_le_bytes());
    }
    pad16_ff(&mut b);
    b
}

/// Virtual fixups — `{u32 src, u32 sec, u32 cnoff}`: bind the object at `src` to its class
/// (`sec` = classnames section index 0, `cnoff` = the body-relative name offset). Emit sorted
/// ascending by `src`.
pub fn write_virtual_fixups(fixups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut b = Vec::new();
    for &(src, sec, cnoff) in fixups {
        b.extend_from_slice(&src.to_le_bytes());
        b.extend_from_slice(&sec.to_le_bytes());
        b.extend_from_slice(&cnoff.to_le_bytes());
    }
    pad16_ff(&mut b);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse the `__classnames__` records out of a packfile body region (for the round-trip gate).
    fn parse_classnames(body: &[u8]) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        let mut p = 0;
        while p + 4 <= body.len() {
            let sig = u32::from_le_bytes([body[p], body[p + 1], body[p + 2], body[p + 3]]);
            if sig == 0xFFFF_FFFF {
                break;
            }
            p += 4;
            assert_eq!(body[p], 0x09, "record separator");
            p += 1;
            let start = p;
            while p < body.len() && body[p] != 0 {
                p += 1;
            }
            out.push((sig, String::from_utf8_lossy(&body[start..p]).into_owned()));
            p += 1; // nul
        }
        out
    }

    /// The written header must match the ACTUAL proven-in-retail oracle bytes (the real Mercs2
    /// `orig8720.bin` packfile header), not a transcription. Root class = `hkaAnimationContainer`,
    /// whose classnames-body offset in this packfile is 0x272.
    #[test]
    fn header_matches_the_oracle() {
        let oracle: &[u8] = include_bytes!("../tests/fixtures/havok_anim_orig8720.bin");
        let got = write_packfile_header(0x272);
        assert_eq!(
            &got[..],
            &oracle[..0x40],
            "packfile header must match the retail packfile byte-for-byte"
        );
    }

    /// The `__classnames__` emitter must reproduce the real section byte-for-byte: parse the 30
    /// records out of the retail packfile, re-emit them, and diff. Also checks the name-offset map
    /// (record 27 = `hkaAnimationContainer` at 0x272, the header's root cnoff).
    #[test]
    fn classnames_round_trip_matches_oracle() {
        let oracle: &[u8] = include_bytes!("../tests/fixtures/havok_anim_orig8720.bin");
        let cn = &oracle[0xD0..0x390]; // __classnames__ body (abs 0xD0, end 0x2C0)
        let classes = parse_classnames(cn);
        assert_eq!(classes.len(), 30, "an anim packfile references 30 classes");
        let refs: Vec<(u32, &str)> = classes.iter().map(|(s, n)| (*s, n.as_str())).collect();
        let (body, name_offsets) = write_classnames(&refs);
        assert_eq!(body.as_slice(), cn, "__classnames__ must re-emit byte-for-byte");

        // hkaAnimationContainer is the root; its name offset must be 0x272 (the header cnoff).
        let container = classes
            .iter()
            .position(|(_, n)| n == "hkaAnimationContainer")
            .expect("container class present");
        assert_eq!(name_offsets[container], 0x272);
    }

    /// The three section headers must match the oracle's table [0x40..0xD0) byte-for-byte.
    #[test]
    fn section_headers_match_the_oracle() {
        let oracle: &[u8] = include_bytes!("../tests/fixtures/havok_anim_orig8720.bin");
        let cn = write_section_header("__classnames__", 0xD0, 0x2C0, 0x2C0, 0x2C0, 0x2C0, 0x2C0, 0x2C0);
        let ty = write_section_header("__types__", 0x390, 0, 0, 0, 0, 0, 0);
        let da = write_section_header("__data__", 0x390, 0x1E40, 0x1E50, 0x1E60, 0x1E80, 0x1E80, 0x1E80);
        assert_eq!(&cn[..], &oracle[0x40..0x70], "__classnames__ header");
        assert_eq!(&ty[..], &oracle[0x70..0xA0], "__types__ header");
        assert_eq!(&da[..], &oracle[0xA0..0xD0], "__data__ header");
    }

    /// The three fixup tables must match the oracle byte-for-byte. `__data__` abs = 0x390, so the
    /// tables live at file 0x21D0 (local), 0x21E0 (global), 0x21F0 (virtual).
    #[test]
    fn fixup_tables_match_the_oracle() {
        let oracle: &[u8] = include_bytes!("../tests/fixtures/havok_anim_orig8720.bin");
        // local: container animations ptr @+0x08→+0x30 ; wavelet dataBuffer ptr @+0x98→+0xA0
        let local = write_local_fixups(&[(0x08, 0x30), (0x98, 0xA0)]);
        assert_eq!(local.as_slice(), &oracle[0x21D0..0x21E0], "local fixups");
        // global: animations[0] T* @+0x30 → __data__(sec 2)+0x40 (the wavelet object)
        let global = write_global_fixups(&[(0x30, 2, 0x40)]);
        assert_eq!(global.as_slice(), &oracle[0x21E0..0x21F0], "global fixups");
        // virtual: container @+0x00 → cnoff 0x272 ; wavelet @+0x40 → cnoff 0x1B8
        let virt = write_virtual_fixups(&[(0x00, 0, 0x272), (0x40, 0, 0x1B8)]);
        assert_eq!(virt.as_slice(), &oracle[0x21F0..0x2210], "virtual fixups");
    }

    /// THE CAPSTONE: the whole-packfile assembler must reproduce the retail packfile EXACTLY when
    /// fed the oracle's own `__data__` object region. This validates every offset computation in
    /// `write_packfile` (section abs/end, fixup-table placement, total size) end-to-end. From-source
    /// object emission is the only remaining piece; this proves the container assembly is correct.
    #[test]
    fn whole_packfile_assembly_reproduces_the_oracle() {
        let oracle: &[u8] = include_bytes!("../tests/fixtures/havok_anim_orig8720.bin");
        let classes: Vec<(u32, String)> = parse_classnames(&oracle[0xD0..0x390]);
        let refs: Vec<(u32, &str)> = classes.iter().map(|(s, n)| (*s, n.as_str())).collect();
        let data = DataSection {
            body: oracle[0x390..0x21D0].to_vec(), // __data__ object region, verbatim
            local: vec![(0x08, 0x30), (0x98, 0xA0)],
            global: vec![(0x30, 2, 0x40)],
            virt: vec![(0x00, 0, 0x272), (0x40, 0, 0x1B8)],
        };
        let got = write_packfile(&refs, 0x272, &data);
        assert_eq!(got.len(), oracle.len(), "packfile length");
        let diffs: Vec<usize> = (0..oracle.len()).filter(|&i| got[i] != oracle[i]).collect();
        assert!(
            diffs.is_empty(),
            "assembler must reproduce the retail packfile byte-for-byte; diffs at {diffs:?}"
        );
    }
}
