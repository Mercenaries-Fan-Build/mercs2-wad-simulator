//! The code and data m2-sdk.dll's shader registry uses, found in the game image without per-build
//! addresses (as [`crate::lua_sites`] does for the Lua host). Everything resolves exactly once or
//! resolution fails, naming what did not.
//!
//! * Four functions by byte signature over `.text`: `FUN_0084f130` (builds the registry), `FUN_0085ac90`
//!   (registers one record), `FUN_0085ace0` / `FUN_0085ade0` (the pixel / vertex record base constructors;
//!   the pixel one is `mov eax, ecx` followed by the vertex one's body, so the vertex signature also
//!   matches two bytes into it and that match is excluded).
//! * The shader pool from its static initializer `FUN_0085ab00`, which zeroes the pixel count, the pixel
//!   keys, the vertex count and the vertex keys; checked against `RegisterAll`'s two pushes of the pool
//!   and the fixed layout (keys at +0x2008, vertex count at +0x6008, vertex keys at +0x6410).
//! * ShaderLevel from `RegisterAll`'s `cmp byte [x], 0; je` instructions, which must all name one byte.
//! * The 45 family vtables from `.rdata`: the 6-slot vtables whose slot 1 is the register function and
//!   slot 2 a stage's load handler (the base constructors' vtables give the two handlers). There are 45 in
//!   every reference image, in the same address order, so they map to families by rank ([`RANK_ORDER`]);
//!   each rank's stage and the two base families' vtables are checked.

use crate::exe_image::{Image, Section};
use crate::lua_sites::Pattern;
use crate::shader_registry::{FAMILY_COUNT, FAMILY_VERTEX, Stage, family};

const REGISTER_ALL: &str = "55 8b ec 83 e4 f8 53 56 68 ?? ?? ?? ?? b8 ?? ?? ?? ?? 8b d9 e8 ?? ?? ?? ?? 68 ?? ?? ?? ?? \
                            b8 ?? ?? ?? ?? e8 ?? ?? ?? ?? a1 ?? ?? ?? ?? 8b 80 e4";
const REGISTER: &str = "8b 54 24 04 56 57 8b f1 e8 ?? ?? ?? ?? 89 46 04 8b 44 24 14 8d 7e 0b 89 86 8c 00 00 00 8b 44 \
                        24 10 8b d7 2b d0 8a 08 88 0c 02 83 c0 01 84 c9 75";
const PS_CTOR: &str = "8b c1 33 c9 66 c7 40 08 ff ff c6 40 0a 01 89 88 90 00 00 00 c7 00 ?? ?? ?? ?? 89 88 94 00 00 00";
const VS_CTOR: &str = "33 c9 66 c7 40 08 ff ff c6 40 0a 01 89 88 90 00 00 00 c7 00 ?? ?? ?? ?? 89 88 94 00 00 00";
const POOL_INIT: &str = "33 d2 57 33 c0 89 15 ?? ?? ?? ?? 89 15 ?? ?? ?? ?? b9 00 08 00 00 bf ?? ?? ?? ?? f3 ab 89 15 ?? \
                         ?? ?? ?? 89 15 ?? ?? ?? ?? b9 00 01 00 00 bf ?? ?? ?? ?? f3 ab";

/// `push <pool>` operands in [`REGISTER_ALL`].
const ALL_POOL_AT: [usize; 2] = [9, 26];
/// The `mov [eax], <vtable>` operands of the constructors.
const PS_VTABLE_AT: usize = 22;
const VS_VTABLE_AT: usize = 20;
/// Operands of [`POOL_INIT`]: pixel count, pixel keys, vertex count, vertex keys.
const POOL_AT: [usize; 4] = [13, 23, 37, 47];
const PS_KEYS_OFF: u32 = 0x2008;
const VS_COUNT_OFF: u32 = 0x6008;
const VS_KEYS_OFF: u32 = 0x6410;
/// `RegisterAll` ends at its first `ret 4` followed by `int3` padding.
const REGISTER_ALL_END: [u8; 4] = [0xc2, 0x04, 0x00, 0xcc];

/// The families in ascending vtable address, as in every reference image.
pub const RANK_ORDER: [i32; FAMILY_COUNT] = [
    12, 22, 1, 10, 20, 13, 8, 34, 14, 29, 4, 30, 7, 6, 40, 21, 41, 44, 25, 23, 43, 24, 32, 11, 35, 15, 36, 31, 9, 33, 28,
    27, 3, 39, 42, 38, 37, 18, 19, 16, 17, 2, 0, 26, 5,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderSites {
    pub register_all: u32,
    pub register: u32,
    pub ps_base_ctor: u32,
    pub vs_base_ctor: u32,
    pub ps_count: u32,
    pub ps_keys: u32,
    pub vs_count: u32,
    pub vs_keys: u32,
    pub shader_level: u32,
    /// Indexed by `m2_shader_family`.
    pub vtable: [u32; FAMILY_COUNT],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaderSitesError {
    NoSection(&'static str),
    Matches(Vec<(&'static str, usize)>),
    Pool(String),
    ShaderLevel(Vec<u32>),
    Families(String),
}

impl std::fmt::Display for ShaderSitesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShaderSitesError::NoSection(s) => write!(f, "the image has no {s} section"),
            ShaderSitesError::Matches(m) => write!(
                f,
                "shader sites did not resolve to exactly one match each: {}",
                m.iter().map(|(n, c)| format!("{n}={c} match(es)")).collect::<Vec<_>>().join(", ")
            ),
            ShaderSitesError::Pool(why) => write!(f, "the shader pool: {why}"),
            ShaderSitesError::ShaderLevel(v) => {
                write!(f, "RegisterAll's ShaderLevel tests name {} byte(s), not one: {v:#x?}", v.len())
            }
            ShaderSitesError::Families(why) => write!(f, "the shader family vtables: {why}"),
        }
    }
}

impl std::error::Error for ShaderSitesError {}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn find(text: &Section, p: &str) -> Vec<usize> {
    Pattern::parse(p).expect("built-in pattern parses").find_all(text.bytes)
}

/// The 6-slot vtables in `rdata` whose slot 1 is `register` and slot 2 one of the two loaders, in address
/// order, each with its stage.
fn family_vtables(rdata: &Section, register: u32, ps_load: u32, vs_load: u32) -> Vec<(u32, Stage)> {
    let mut out = Vec::new();
    for at in (0..rdata.bytes.len().saturating_sub(24)).step_by(4) {
        if u32_at(rdata.bytes, at + 4) != register {
            continue;
        }
        let load = u32_at(rdata.bytes, at + 8);
        let stage = if load == ps_load {
            Stage::Pixel
        } else if load == vs_load {
            Stage::Vertex
        } else {
            continue;
        };
        out.push((rdata.va + at as u32, stage));
    }
    out
}

pub fn resolve(img: &Image) -> Result<ShaderSites, ShaderSitesError> {
    let text = img.section(".text").ok_or(ShaderSitesError::NoSection(".text"))?;
    let rdata = img.section(".rdata").ok_or(ShaderSitesError::NoSection(".rdata"))?;

    let all = find(text, REGISTER_ALL);
    let reg = find(text, REGISTER);
    let ps = find(text, PS_CTOR);
    let pool = find(text, POOL_INIT);
    let vs: Vec<usize> = find(text, VS_CTOR).into_iter().filter(|&o| ps.len() != 1 || o != ps[0] + 2).collect();
    let counts =
        [("RegisterAll", all.len()), ("Register", reg.len()), ("PsBaseCtor", ps.len()), ("VsBaseCtor", vs.len()), ("PoolInit", pool.len())];
    if counts.iter().any(|(_, n)| *n != 1) {
        return Err(ShaderSitesError::Matches(counts.to_vec()));
    }
    let va = |o: usize| text.va + o as u32;
    let (all, reg, ps, vs, pool) = (all[0], reg[0], ps[0], vs[0], pool[0]);

    let [ps_count, ps_keys, vs_count, vs_keys] = POOL_AT.map(|o| u32_at(text.bytes, pool + o));
    let pushes = ALL_POOL_AT.map(|o| u32_at(text.bytes, all + o));
    if pushes != [ps_count, ps_count] {
        return Err(ShaderSitesError::Pool(format!("RegisterAll pushes {pushes:#x?}, the initializer zeroes {ps_count:#x}")));
    }
    if [ps_keys, vs_count, vs_keys] != [ps_count + PS_KEYS_OFF, ps_count + VS_COUNT_OFF, ps_count + VS_KEYS_OFF] {
        return Err(ShaderSitesError::Pool(format!(
            "pool {ps_count:#x}: keys {ps_keys:#x}, vertex count {vs_count:#x}, vertex keys {vs_keys:#x} are not at \
             +{PS_KEYS_OFF:#x}, +{VS_COUNT_OFF:#x}, +{VS_KEYS_OFF:#x}"
        )));
    }

    let end = text.bytes[all..]
        .windows(REGISTER_ALL_END.len())
        .position(|w| w == REGISTER_ALL_END)
        .map_or(text.bytes.len(), |e| all + e + 3);
    let level_test = Pattern::parse("80 3d ?? ?? ?? ?? 00 0f 84").expect("built-in pattern parses");
    let mut levels: Vec<u32> = level_test.find_all(&text.bytes[all..end]).iter().map(|&o| u32_at(text.bytes, all + o + 2)).collect();
    levels.sort_unstable();
    levels.dedup();
    if levels.len() != 1 {
        return Err(ShaderSitesError::ShaderLevel(levels));
    }

    let register = va(reg);
    let ps_vt = u32_at(text.bytes, ps + PS_VTABLE_AT);
    let vs_vt = u32_at(text.bytes, vs + VS_VTABLE_AT);
    let (Some(ps_load), Some(vs_load)) = (img.u32_at(ps_vt + 8), img.u32_at(vs_vt + 8)) else {
        return Err(ShaderSitesError::Families(format!("the base vtables {ps_vt:#x} / {vs_vt:#x} are not in the image")));
    };
    let found = family_vtables(rdata, register, ps_load, vs_load);
    if found.len() != FAMILY_COUNT {
        return Err(ShaderSitesError::Families(format!("{} vtables have the register function and a loader, not {FAMILY_COUNT}", found.len())));
    }
    let mut vtable = [0u32; FAMILY_COUNT];
    for (rank, (&fam, &(vt, stage))) in RANK_ORDER.iter().zip(&found).enumerate() {
        let want = family(fam).expect("RANK_ORDER holds family ids").stage;
        if stage != want {
            return Err(ShaderSitesError::Families(format!("rank {rank} ({vt:#x}) is a {stage:?} vtable; family {fam} is {want:?}")));
        }
        vtable[fam as usize] = vt;
    }
    if vtable[0] != ps_vt || vtable[FAMILY_VERTEX as usize] != vs_vt {
        return Err(ShaderSitesError::Families(format!(
            "the base families map to {:#x} / {:#x}; the constructors store {ps_vt:#x} / {vs_vt:#x}",
            vtable[0], vtable[FAMILY_VERTEX as usize]
        )));
    }

    Ok(ShaderSites {
        register_all: va(all),
        register,
        ps_base_ctor: va(ps),
        vs_base_ctor: va(vs),
        ps_count,
        ps_keys,
        vs_count,
        vs_keys,
        shader_level: levels[0],
        vtable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe_image::synth;

    #[test]
    fn rank_order_is_every_family_once() {
        let mut seen = RANK_ORDER.to_vec();
        seen.sort_unstable();
        assert_eq!(seen, (0..FAMILY_COUNT as i32).collect::<Vec<_>>());
        // The family table's own vtables (the unpacked exe's) ascend in this order.
        let vts: Vec<u32> = RANK_ORDER.iter().map(|&f| family(f).unwrap().vtable).collect();
        assert!(vts.windows(2).all(|w| w[0] < w[1]), "{vts:#x?}");
    }

    #[test]
    fn patterns_parse_and_operands_are_wildcards() {
        for p in [REGISTER_ALL, REGISTER, PS_CTOR, VS_CTOR, POOL_INIT] {
            Pattern::parse(p).unwrap();
        }
        let toks = |p: &str| p.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        for (p, offs) in [(REGISTER_ALL, &ALL_POOL_AT[..]), (POOL_INIT, &POOL_AT[..]), (PS_CTOR, &[PS_VTABLE_AT][..]), (VS_CTOR, &[VS_VTABLE_AT][..])] {
            let t = toks(p);
            for &o in offs {
                assert_eq!(t[o..o + 4], ["??"; 4], "{p} at {o}");
            }
        }
    }

    #[test]
    fn an_image_without_the_code_names_every_site() {
        let file = synth::pe(0x400000, &[(".text", 0x1000, vec![0xcc; 64]), (".rdata", 0x2000, vec![0; 64])]);
        let e = resolve(&Image::from_file(&file).unwrap()).unwrap_err();
        assert!(e.to_string().contains("RegisterAll=0 match(es)"), "{e}");
        assert!(e.to_string().contains("PoolInit=0 match(es)"), "{e}");
    }
}
