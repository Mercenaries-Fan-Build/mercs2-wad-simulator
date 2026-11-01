//! The game addresses pmc_bb reads or writes, found in the game image without per-build addresses, in
//! the form of [`crate::lua_sites::Pattern`]: `call rel32` targets and absolute addresses are `??`, and
//! every site matches exactly once in `.text` or resolution fails, naming each site's match count.
//!
//! pmc_bb is C and links nothing from this workspace, and it cannot lean on `m2-sdk.dll`: it has to report
//! on a game that has no SDK installed, and on an SDK that failed to load. So this table is the one
//! definition and [`c_header`] writes it out as `pmc_sites.gen.h` for pmc_bb's own small scanner, the way
//! the load ladder reaches C. `mercs2_probe --bin pmc_sites_header` prints the header.
//!
//! Each site gives one of two things ([`Yields`]): a 32-bit address read from an instruction's operand, or
//! a function's start with its length. Roles, from the disassembly of `mercs2_unpacked.exe` (v1.0 code) and
//! `genuine_patched_unpacked.exe` (v1.1 code):
//!
//! | site | v1.0 | v1.1 | what it is |
//! |---|---|---|---|
//! | [`Site::EntityVtable`] | `0x00BDB410` | `0x00BDB348` | the Entity vtable, from the `mov [edi], vtable` in `Entity_Destruct` |
//! | [`Site::PrmgKeys`] | `0x0197DE48` | `0x0197CE48` | the render-resource registry's 256 keys, from `FUN_00478270` |
//! | [`Site::PrmgHandles`] | `0x0197DA48` | `0x0197CA48` | its 256 handles; the `lea` beside the sentinel at handles - 4 |
//! | [`Site::SpawnWalk`] | `0x004AB0D0` | `0x004AB0D0` | the model-segment walk holding the `0x4AB26B` fault |
//! | [`Site::PrmgDrawA`] | `0x0047A6C0` | `0x0047A7A0` | the first PRMG draw function |
//! | [`Site::PrmgDrawB`] | `0x0047AA20` | `0x0047AB00` | the second PRMG draw function |
//! | [`Site::TextureFree`] | `0x0116A7CC` | `0x011697CC` | the texture free lists, from the lookup helper |
//! | [`Site::TextureInUse`] | `0x01174FE8` | `0x01173FE8` | the in-use texture lists, from the same helper |
//!
//! The index and vertex buffer lookups are byte-identical functions, so no signature tells them apart.
//! Their four list heads follow the in-use texture lists at fixed offsets ([`PoolLists`]), as the operands
//! of those functions show on both builds.

use crate::exe_image::Image;
use crate::lua_sites::Pattern;

/// The resolved sites, in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    EntityVtable,
    PrmgKeys,
    PrmgHandles,
    SpawnWalk,
    PrmgDrawA,
    PrmgDrawB,
    TextureFree,
    TextureInUse,
}

/// What a site's match gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Yields {
    /// The little-endian `u32` this many bytes into the match.
    Operand(usize),
    /// The match's own address, a function of this many bytes.
    Function(u32),
}

impl Site {
    pub const ALL: [Site; 8] = [
        Site::EntityVtable,
        Site::PrmgKeys,
        Site::PrmgHandles,
        Site::SpawnWalk,
        Site::PrmgDrawA,
        Site::PrmgDrawB,
        Site::TextureFree,
        Site::TextureInUse,
    ];

    /// The name in pmc_bb's log and the suffix of its `PMC_SITE_` constant.
    pub fn name(self) -> &'static str {
        match self {
            Site::EntityVtable => "entity_vtable",
            Site::PrmgKeys => "prmg_keys",
            Site::PrmgHandles => "prmg_handles",
            Site::SpawnWalk => "spawn_walk",
            Site::PrmgDrawA => "prmg_draw_a",
            Site::PrmgDrawB => "prmg_draw_b",
            Site::TextureFree => "texture_free",
            Site::TextureInUse => "texture_in_use",
        }
    }

    pub fn yields(self) -> Yields {
        match self {
            Site::EntityVtable => Yields::Operand(14),
            Site::PrmgKeys => Yields::Operand(17),
            Site::PrmgHandles => Yields::Operand(19),
            // The same length on both builds, to each function's last `ret`.
            Site::SpawnWalk => Yields::Function(0x2aa),
            Site::PrmgDrawA => Yields::Function(0x198),
            Site::PrmgDrawB => Yields::Function(0x1b4),
            Site::TextureFree => Yields::Operand(50),
            Site::TextureInUse => Yields::Operand(18),
        }
    }

    /// The byte signature, as space-separated hex with `??` wildcards.
    pub fn pattern(self) -> &'static str {
        match self {
            // push esi; push edi; mov edi,ecx; mov eax,[edi+0x184]; test eax,eax;
            // mov [edi], vtable; mov [edi+0x10], ..; mov [edi+0x64], ..; je; ...
            Site::EntityVtable => {
                "56 57 8b f9 8b 87 84 01 00 00 85 c0 c7 07 ?? ?? ?? ?? c7 47 10 ?? ?? ?? ?? c7 47 64 ?? ?? ?? ?? 74 38 \
                 8b 08 8b 50 04 89 51 04"
            }
            // ...; push 0x100; mov esi, keys; movss [ebp+0x1a4],xmm0; call <find>; add esp,0xc; test eax,eax; jge
            Site::PrmgKeys => {
                "85 a0 01 00 00 f3 0f 10 44 24 60 68 00 01 00 00 be ?? ?? ?? ?? f3 0f 11 85 a4 01 00 00 e8 ?? ?? ?? ?? \
                 83 c4 0c 85 c0 7d 07"
            }
            // add esp,0xc; test eax,eax; jge; mov eax, sentinel; jmp; lea eax,[eax*4+handles]; mov eax,[eax]; ...
            Site::PrmgHandles => {
                "3a 00 83 c4 0c 85 c0 7d 07 b8 ?? ?? ?? ?? eb 07 8d 04 85 ?? ?? ?? ?? 8b 00 8b 4c 24 30 68 00 01 00 00 \
                 be ?? ?? ?? ?? 89 45 00"
            }
            // sub esp,0x34; cmp dword [hash],0; push ebx,ebp,esi; mov esi,ecx; push edi; hash the name once; ...
            Site::SpawnWalk => {
                "83 ec 34 83 3d ?? ?? ?? ?? 00 53 55 56 8b f1 57 89 74 24 1c 75 0f ba ?? ?? ?? ?? e8 ?? ?? ?? ?? a3 ?? \
                 ?? ?? ?? a1 ?? ?? ?? ?? 0f b7 88 92 2b 00 00"
            }
            // The two draw functions: the same loop over the batch, with record+0 and record+4 respectively.
            Site::PrmgDrawA => {
                "83 ec 0c 55 8b 6c 24 14 33 c0 39 45 04 89 44 24 08 0f 8e ?? ?? ?? ?? 53 56 89 44 24 0c 57 8b ff 8b 7d \
                 08 03 7c 24 10 8b 45 14 85 c0 8b 1f 89 44 24 18 74 64 8b 75 10 85 f6 74 5d 83 bb fc 00 00 00 00"
            }
            Site::PrmgDrawB => {
                "83 ec 0c 57 8b 7c 24 14 33 c0 39 47 04 89 44 24 0c 0f 8e ?? ?? ?? ?? 53 55 89 44 24 0c 56 8b ff 8b 6f \
                 08 03 6c 24 10 8b 47 14 85 c0 8b 5d 04 89 44 24 14 74 64 8b 77 10 85 f6 74 5d 83 bb fc 00 00 00 00"
            }
            // The texture lookup: exponent of the width, format bucket, then mov eax, free lists
            Site::TextureFree => {
                "f3 0f 2a 44 24 0c f3 0f 11 44 24 0c 8b 4c 24 0c c1 e9 17 81 e1 ff 00 00 00 83 e9 7f 3d c8 00 00 00 0f \
                 b7 d1 73 05 0f b7 c0 eb 03 c1 e8 19 0f bf c8 a1 ?? ?? ?? ?? 53 0f bf d2 03 c9 55 03 c9 8b 04 01 03 d2 \
                 56 03 d2 57"
            }
            // The node leaving the free list: or dword [eax], 0x10000000; mov esi, in-use lists; ...
            Site::TextureInUse => {
                "70 10 89 37 8b 70 04 81 08 00 00 00 10 89 70 04 8b 35 ?? ?? ?? ?? 89 58 10 89 58 14 8b 0c 31 03 ca 8b \
                 11 3b d3 74 06 89 50 10"
            }
        }
    }
}

/// Offsets of the buffer list heads from [`Site::TextureInUse`], read from the operands of the IB and VB
/// lookups on both builds.
pub const IB_IN_USE_OFF: u32 = 0x4;
pub const VB_IN_USE_OFF: u32 = 0x8;
pub const IB_FREE_OFF: u32 = 0xc;
pub const VB_FREE_OFF: u32 = 0x10;

/// Every pool list head, by resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolLists {
    pub texture_free: u32,
    pub texture_in_use: u32,
    pub ib_in_use: u32,
    pub vb_in_use: u32,
    pub ib_free: u32,
    pub vb_free: u32,
}

/// Where every site was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PmcSites {
    value: [u32; Site::ALL.len()],
}

impl PmcSites {
    /// The address a site gives: the operand it names, or the function's start.
    pub fn value(&self, s: Site) -> u32 {
        self.value[s as usize]
    }

    pub fn pool_lists(&self) -> PoolLists {
        let in_use = self.value(Site::TextureInUse);
        PoolLists {
            texture_free: self.value(Site::TextureFree),
            texture_in_use: in_use,
            ib_in_use: in_use + IB_IN_USE_OFF,
            vb_in_use: in_use + VB_IN_USE_OFF,
            ib_free: in_use + IB_FREE_OFF,
            vb_free: in_use + VB_FREE_OFF,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PmcSitesError {
    NoSection(&'static str),
    /// Each site's match count, when any is not exactly one.
    Matches(Vec<(Site, usize)>),
}

impl std::fmt::Display for PmcSitesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PmcSitesError::NoSection(s) => write!(f, "the image has no {s} section"),
            PmcSitesError::Matches(m) => write!(
                f,
                "pmc_bb sites did not resolve to exactly one match each: {}",
                m.iter().map(|(s, n)| format!("{}={n} match(es)", s.name())).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

impl std::error::Error for PmcSitesError {}

/// Resolve every site in `img`.
pub fn resolve(img: &Image) -> Result<PmcSites, PmcSitesError> {
    let text = img.section(".text").ok_or(PmcSitesError::NoSection(".text"))?;
    let hits: Vec<Vec<usize>> = Site::ALL
        .iter()
        .map(|s| Pattern::parse(s.pattern()).expect("built-in pattern parses").find_all(text.bytes))
        .collect();
    if !hits.iter().all(|h| h.len() == 1) {
        return Err(PmcSitesError::Matches(Site::ALL.iter().zip(&hits).map(|(s, h)| (*s, h.len())).collect()));
    }
    let mut value = [0u32; Site::ALL.len()];
    for (s, h) in Site::ALL.iter().zip(&hits) {
        let at = h[0];
        value[*s as usize] = match s.yields() {
            Yields::Function(_) => text.va + at as u32,
            Yields::Operand(off) => {
                let b = &text.bytes[at + off..at + off + 4];
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            }
        };
    }
    Ok(PmcSites { value })
}

/// `pmc_sites.gen.h`: the table for pmc_bb's C scanner.
pub fn c_header() -> String {
    let mut h = String::from(
        "/* pmc_sites.gen.h -- GENERATED from mercs2_formats::pmc_sites by\n \
         * `mercs2_probe --bin pmc_sites_header`. Do not edit by hand. */\n\
         #ifndef PMC_SITES_GEN_H\n#define PMC_SITES_GEN_H\n\n",
    );
    h.push_str(&format!("#define PMC_SITE_COUNT {}\n\n", Site::ALL.len()));
    for (i, s) in Site::ALL.iter().enumerate() {
        h.push_str(&format!("#define PMC_SITE_{} {i}\n", s.name().to_ascii_uppercase()));
    }
    h.push_str(&format!(
        "\n#define PMC_POOL_IB_IN_USE_OFF {IB_IN_USE_OFF:#x}\n#define PMC_POOL_VB_IN_USE_OFF {VB_IN_USE_OFF:#x}\n\
         #define PMC_POOL_IB_FREE_OFF {IB_FREE_OFF:#x}\n#define PMC_POOL_VB_FREE_OFF {VB_FREE_OFF:#x}\n\n"
    ));
    h.push_str(
        "/* operand_at >= 0: the site is the 32-bit value that far into the match.\n \
         * operand_at < 0: the site is the match itself, a function of function_len bytes. */\n\
         typedef struct {\n    const char *name;\n    const char *pattern;\n    int operand_at;\n    \
         unsigned function_len;\n} PmcSiteDef;\n\n\
         static const PmcSiteDef PMC_SITE_DEFS[PMC_SITE_COUNT] = {\n",
    );
    for s in Site::ALL {
        let (at, len) = match s.yields() {
            Yields::Operand(off) => (off as i64, 0),
            Yields::Function(len) => (-1, len),
        };
        let pattern = s.pattern().split_ascii_whitespace().collect::<Vec<_>>().join(" ");
        h.push_str(&format!("    {{ \"{}\", \"{pattern}\", {at}, {len:#x} }},\n", s.name()));
    }
    h.push_str("};\n\n#endif /* PMC_SITES_GEN_H */\n");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe_image::synth;

    #[test]
    fn every_built_in_pattern_parses_with_its_operand_wildcarded() {
        for s in Site::ALL {
            let toks: Vec<&str> = s.pattern().split_whitespace().collect();
            Pattern::parse(s.pattern()).unwrap_or_else(|e| panic!("{}: {e:?}", s.name()));
            if let Yields::Operand(off) = s.yields() {
                assert_eq!(&toks[off..off + 4], ["??", "??", "??", "??"], "{}", s.name());
            }
        }
    }

    #[test]
    fn an_image_without_the_code_reports_every_site() {
        let file = synth::pe(0x400000, &[(".text", 0x1000, vec![0xcc; 64]), (".rdata", 0x2000, vec![0; 64])]);
        let img = Image::from_file(&file).unwrap();
        let e = resolve(&img).unwrap_err();
        let PmcSitesError::Matches(m) = &e else { panic!("{e}") };
        assert_eq!(m.len(), Site::ALL.len());
        assert!(e.to_string().contains("entity_vtable=0 match(es)"), "{e}");
    }

    /// A `.text` holding exactly one match of every site, each with its wildcards filled by `fill`.
    fn text_with_every_site(fill: u8) -> (Vec<u8>, Vec<usize>) {
        let mut text = vec![0xcc; 8];
        let mut at = Vec::new();
        for s in Site::ALL {
            at.push(text.len());
            for tok in s.pattern().split_whitespace() {
                text.push(if tok == "??" { fill } else { u8::from_str_radix(tok, 16).unwrap() });
            }
            text.extend([0xcc; 8]);
        }
        (text, at)
    }

    #[test]
    fn operands_and_functions_are_read_where_the_table_says() {
        let (text, at) = text_with_every_site(0x11);
        let file = synth::pe(0x400000, &[(".text", 0x1000, text)]);
        let s = resolve(&Image::from_file(&file).unwrap()).unwrap();
        assert_eq!(s.value(Site::EntityVtable), 0x1111_1111);
        assert_eq!(s.value(Site::SpawnWalk), 0x401000 + at[Site::SpawnWalk as usize] as u32);
        let lists = s.pool_lists();
        assert_eq!(lists.vb_free, 0x1111_1111 + 0x10);
        assert_eq!(lists.texture_free, 0x1111_1111);
    }

    #[test]
    fn a_site_matching_twice_fails() {
        let (mut text, _) = text_with_every_site(0x11);
        let again = text.clone();
        text.extend(again);
        let file = synth::pe(0x400000, &[(".text", 0x1000, text)]);
        let e = resolve(&Image::from_file(&file).unwrap()).unwrap_err();
        assert!(e.to_string().contains("texture_free=2 match(es)"), "{e}");
    }

    #[test]
    fn the_c_header_carries_every_site_on_one_line() {
        let h = c_header();
        assert!(h.contains(&format!("#define PMC_SITE_COUNT {}", Site::ALL.len())));
        for (i, s) in Site::ALL.iter().enumerate() {
            assert!(h.contains(&format!("#define PMC_SITE_{} {i}\n", s.name().to_ascii_uppercase())));
            let line = h.lines().find(|l| l.contains(&format!("\"{}\",", s.name()))).expect(s.name());
            assert!(!line.contains("  ?? "), "the pattern is one normalised line: {line}");
        }
    }
}
