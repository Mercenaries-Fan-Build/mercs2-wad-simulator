//! Read the retail game's memory out of a live-process capture (`game-files/memory-dumps/`).
//!
//! Two capture layouts exist, and [`Capture::open`] takes either:
//! * a **folder** `<build>/<state>/` holding `process.dmp`, `SHA256SUMS.txt` and `capture.log`
//!   (`exe_sha256=`, looked up in `builds.tsv`);
//! * a **`.dmp` file** `<yyyymmdd>_<label>.dmp` beside a `MANIFEST.md` whose table row for `<label>`
//!   ends in the dump's sha256 (no exe sha is recorded for these).
//!
//! Either way the dump's bytes must match their recorded sha256, and the game code in the dump must be
//! v1.0 — the only code whose addresses are the Ghidra decomp's — proven from the dump's own image by
//! the three-address byte test of `docs/reverse_engineer/pc_exe_build_fingerprints.md`. [`Mem`] then
//! reads virtual addresses out of the dump's `Memory64List`; an address the dump did not capture is an
//! error, never zeros.

use std::path::{Path, PathBuf};

use minidump::{Minidump, MinidumpMemory64List, MmapMinidump};
use sha2::{Digest, Sha256};

/// The v1.0 game code's first 12 bytes at the three VAs where v1.0 and v1.1 differ most
/// (`pc_exe_build_fingerprints.md` "The three code groups"), read from the `image.bin` of all four
/// v1.0 captures (`ada55455`, `2ae4a2e0`, `23fe7f38`, `958eb227`), which agree byte for byte. The v1.1
/// captures (`a1532b4c`, `7a348847`) start `55 8b ec 83 e4 f8` at all three.
const V10_CODE: [(u32, [u8; 12]); 3] = [
    (0x005C_37F0, [0x53, 0x56, 0x57, 0x8d, 0x84, 0x24, 0xf8, 0x02, 0x00, 0x00, 0x50, 0x8b]),
    (0x0068_3CE0, [0x4d, 0x48, 0x5f, 0x83, 0xe1, 0xe5, 0x83, 0xc9, 0x04, 0x5e, 0x89, 0x4d]),
    (0x006A_5970, [0x85, 0xc0, 0x74, 0x09, 0x8b, 0x10, 0x8b, 0xc8, 0x8b, 0x42, 0x10, 0xff]),
];

/// A verified capture: its minidump, mapped, and what identified its build.
pub struct Capture {
    /// The capture folder, or the `.dmp` file.
    pub path: PathBuf,
    /// The `builds.tsv` id when the capture records its exe, else how v1.0 was established.
    pub build_id: String,
    pub dump: MmapMinidump,
}

impl Capture {
    /// Open a capture folder or a manifest-listed `.dmp` (see the module docs).
    pub fn open(path: &Path) -> Result<Capture, String> {
        let (dmp, build_id) = if path.is_dir() { Self::folder(path)? } else { Self::manifest_listed(path)? };
        let dump = Minidump::read_path(&dmp).map_err(|e| format!("{}: {e}", dmp.display()))?;
        let cap = Capture { path: path.to_path_buf(), build_id, dump };
        let mem = cap.mem()?;
        for (va, want) in V10_CODE {
            let got = mem.read(va, want.len())?;
            if got != want {
                return Err(format!(
                    "{}: bytes at 0x{va:08X} are {} — not v1.0 game code; only v1.0 shares the decomp's addresses",
                    path.display(),
                    got.iter().map(|b| format!("{b:02x}")).collect::<String>()
                ));
            }
        }
        drop(mem);
        Ok(cap)
    }

    /// A `<build>/<state>/` folder: `process.dmp` against `SHA256SUMS.txt`, the exe against `builds.tsv`.
    fn folder(dir: &Path) -> Result<(PathBuf, String), String> {
        let dmp = dir.join("process.dmp");
        let sums = read_text(&dir.join("SHA256SUMS.txt"))?;
        let want = sums
            .lines()
            .find_map(|l| {
                let (sha, name) = l.split_once(char::is_whitespace)?;
                (name.trim() == "process.dmp").then(|| sha.trim().to_ascii_lowercase())
            })
            .ok_or_else(|| format!("{}: SHA256SUMS.txt lists no process.dmp", dir.display()))?;
        check_sha(&dmp, &want, "SHA256SUMS.txt")?;

        let log = read_text(&dir.join("capture.log"))?;
        let exe_sha = log
            .lines()
            .find_map(|l| l.trim().strip_prefix("exe_sha256="))
            .map(|s| s.trim().to_ascii_lowercase())
            .ok_or_else(|| format!("{}: capture.log has no exe_sha256= line", dir.display()))?;
        let catalogue = mercs2_exemap::Catalogue::embedded().map_err(|e| e.to_string())?;
        let build = catalogue
            .build_by_sha(&exe_sha)
            .ok_or_else(|| format!("{}: exe sha256 {exe_sha} is not in builds.tsv", dir.display()))?;
        if build.code_version != mercs2_exemap::CodeVersion::V1_0 {
            return Err(format!(
                "{}: build {} runs {} code; only v1.0 captures share the decomp's addresses",
                dir.display(),
                build.id,
                build.code_version.as_str()
            ));
        }
        Ok((dmp, build.id.clone()))
    }

    /// A `<yyyymmdd>_<label>.dmp`: its sha256 from the `MANIFEST.md` row whose label column is `<label>`.
    fn manifest_listed(dmp: &Path) -> Result<(PathBuf, String), String> {
        let stem = dmp.file_stem().and_then(|s| s.to_str()).ok_or_else(|| format!("{}: no file name", dmp.display()))?;
        let label = stem
            .split_once('_')
            .map(|(_, l)| l)
            .ok_or_else(|| format!("{}: name is not <yyyymmdd>_<label>.dmp", dmp.display()))?;
        let manifest = dmp.with_file_name("MANIFEST.md");
        let text = read_text(&manifest)?;
        let want = text
            .lines()
            .find_map(|l| {
                let cells: Vec<&str> = l.split('|').map(str::trim).filter(|c| !c.is_empty()).collect();
                (cells.get(1) == Some(&label)).then(|| cells.last().map(|s| s.to_ascii_lowercase()))?
            })
            .ok_or_else(|| format!("{}: no row for label {label:?}", manifest.display()))?;
        check_sha(dmp, &want, "MANIFEST.md")?;
        Ok((dmp.to_path_buf(), "v1.0 by code bytes (manifest capture, exe unrecorded)".into()))
    }

    /// The dump's captured memory.
    pub fn mem(&self) -> Result<Mem<'_>, String> {
        let list = self
            .dump
            .get_stream::<MinidumpMemory64List>()
            .map_err(|e| format!("{}: no Memory64List stream ({e})", self.path.display()))?;
        Ok(Mem { list })
    }
}

fn check_sha(file: &Path, want: &str, source: &str) -> Result<(), String> {
    let got = sha256_file(file)?;
    if got != want {
        return Err(format!("{}: sha256 {got} does not match {source} ({want})", file.display()));
    }
    Ok(())
}

/// The virtual memory a minidump captured.
pub struct Mem<'a> {
    list: MinidumpMemory64List<'a>,
}

impl Mem<'_> {
    /// `len` bytes at `va`, spanning adjacent captured regions. Any uncaptured byte is an error.
    pub fn read(&self, va: u32, len: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        let mut at = va as u64;
        let end = va as u64 + len as u64;
        while at < end {
            let r = self
                .list
                .memory_at_address(at)
                .ok_or_else(|| format!("0x{at:08X} was not captured (reading {len} bytes at 0x{va:08X})"))?;
            let off = (at - r.base_address) as usize;
            let take = ((end - at) as usize).min(r.bytes.len() - off);
            out.extend_from_slice(&r.bytes[off..off + take]);
            at += take as u64;
        }
        Ok(out)
    }

    /// Every 4-byte-aligned address in captured memory holding one of `values` (little-endian u32).
    pub fn find_u32(&self, values: &[u32]) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for r in self.list.by_addr() {
            for (i, w) in r.bytes.chunks_exact(4).enumerate() {
                let v = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
                if values.contains(&v) {
                    out.push(((r.base_address as u32).wrapping_add(i as u32 * 4), v));
                }
            }
        }
        out
    }

    /// Every 4-byte-aligned address where three consecutive f32 lie within `tol` of `v`.
    pub fn find_vec3(&self, v: [f32; 3], tol: f32) -> Vec<u32> {
        let mut out = Vec::new();
        for r in self.list.by_addr() {
            let f: Vec<f32> = r.bytes.chunks_exact(4).map(|w| f32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
            for i in 0..f.len().saturating_sub(2) {
                if (f[i] - v[0]).abs() <= tol && (f[i + 1] - v[1]).abs() <= tol && (f[i + 2] - v[2]).abs() <= tol {
                    out.push((r.base_address as u32).wrapping_add(i as u32 * 4));
                }
            }
        }
        out
    }

    pub fn u8(&self, va: u32) -> Result<u8, String> {
        Ok(self.read(va, 1)?[0])
    }

    pub fn u16(&self, va: u32) -> Result<u16, String> {
        let b = self.read(va, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&self, va: u32) -> Result<u32, String> {
        let b = self.read(va, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// The NUL-terminated string at `va`, at most `max` bytes. No terminator within `max` is an error.
    pub fn cstr(&self, va: u32, max: usize) -> Result<String, String> {
        let mut s = Vec::new();
        for i in 0..max as u32 {
            match self.u8(va + i)? {
                0 => return Ok(String::from_utf8_lossy(&s).into_owned()),
                c => s.push(c),
            }
        }
        Err(format!("no NUL within {max} bytes at 0x{va:08X}"))
    }

    /// Follow a pointer chain: `[[[base] + o0] + o1] …`. The last offset is added, not dereferenced,
    /// so `chase(PTR, &[0x61])` is the ADDRESS of field `+0x61` of the object `PTR` points at.
    pub fn chase(&self, base: u32, offsets: &[u32]) -> Result<u32, String> {
        let mut at = self.u32(base)?;
        for (i, &o) in offsets.iter().enumerate() {
            if i + 1 == offsets.len() {
                return Ok(at.wrapping_add(o));
            }
            at = self.u32(at.wrapping_add(o))?;
        }
        Ok(at)
    }
}

/// The two container arrays (`object_entity_core_code_map.md` §1.1): 102 of the `0x80` class from
/// `0x00DF6B88`, 232 of the `0x50` class from `0x017BBF58`.
const ARRAY_80: (u32, u32) = (0x00DF_6B88, 102);
const ARRAY_50: (u32, u32) = (0x017B_BF58, 232);

/// The two container storage classes (§1.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Two hash indexes; record key = dense index; owner GUID at `keys[+0x44][index]`.
    C80,
    /// One hash index; the record lives at its bucket; `+0x34[bucket]` holds the key.
    C50,
}

/// One live component container.
#[derive(Clone, Debug)]
pub struct Container {
    pub va: u32,
    pub class: Class,
    /// The component name `[vtable+0x34]` returns (`mov eax, <char*>; ret`).
    pub name: String,
    pub type_id: u16,
    /// Live record count (`0x80` class `+0x18`; the `0x50` class's count is not documented, so it is
    /// the number of occupied buckets found).
    pub count: u32,
    pub stride: u16,
    pub shift: u8,
    pub per_page: u32,
}

/// One live record: its key (owner GUID for the `0x80` class) and where its bytes are.
#[derive(Clone, Copy, Debug)]
pub struct Record {
    pub key: u32,
    pub va: u32,
}

impl Mem<'_> {
    /// The component name of the container at `va`, if it carries the container fingerprint:
    /// `[vtable+0x34]` is `B8 <imm32> C3` naming an ASCII string, and for the `0x80` class both hash
    /// indexes' owner back-pointers (`+0x4C`, `+0x68`) read back `va`. (`u16 +0x04 == 0xFFFF` holds in
    /// the boot-time image only; in-world captures carry other values there.)
    fn container_name(&self, va: u32, class: Class) -> Option<String> {
        if class == Class::C80 && (self.u32(va + 0x4C).ok()? != va || self.u32(va + 0x68).ok()? != va) {
            return None;
        }
        let acc = self.u32(self.u32(va).ok()? + 0x34).ok()?;
        let code = self.read(acc, 6).ok()?;
        if code[0] != 0xB8 || code[5] != 0xC3 {
            return None;
        }
        let name = self.cstr(u32::from_le_bytes([code[1], code[2], code[3], code[4]]), 128).ok()?;
        (!name.is_empty() && name.bytes().all(|b| b.is_ascii_graphic())).then_some(name)
    }

    /// Every container in both arrays, found by fingerprint. Exactly 102 + 232 or an error.
    pub fn containers(&self) -> Result<Vec<Container>, String> {
        let mut out = Vec::new();
        for (class, (base, want), span) in [(Class::C80, ARRAY_80, 0x80 * 103), (Class::C50, ARRAY_50, 0x50 * 232)] {
            let mut found = 0;
            let mut va = base;
            while va < base + span {
                if let Some(name) = self.container_name(va, class) {
                    out.push(self.container(va, class, name)?);
                    found += 1;
                }
                va += 4;
            }
            if found != want {
                return Err(format!("{class:?} array at 0x{base:08X}: {found} containers, expected {want}"));
            }
        }
        Ok(out)
    }

    fn container(&self, va: u32, class: Class, name: String) -> Result<Container, String> {
        let per_page = match class {
            Class::C80 => self.u32(va + 0x20)?,
            Class::C50 => self.u32(va + 0x28)?,
        };
        let shift = self.u8(va + 0x26)?;
        if per_page != 1u32 << (shift & 31) {
            return Err(format!(
                "{name} @0x{va:08X} ({class:?}): records-per-page {per_page} != 2^shift {shift} — wrong class layout"
            ));
        }
        let mut c = Container {
            va,
            class,
            name,
            type_id: self.u16(va + 6)?,
            count: 0,
            stride: self.u16(va + 0x24)?,
            shift,
            per_page,
        };
        c.count = match class {
            Class::C80 => self.u32(va + 0x18)?,
            Class::C50 => self.records(&c)?.len() as u32,
        };
        Ok(c)
    }

    /// The container whose component name is `name` (case-insensitive).
    pub fn container_named(&self, name: &str) -> Result<Container, String> {
        self.containers()?
            .into_iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("no container named {name:?}"))
    }

    /// Every live record of `c`, walked bucket by bucket the way the resolvers read them
    /// (`FUN_00648D80`/`FUN_00423DC0` for `0x80`, `FUN_0064A090`/`FUN_005857E0` for `0x50`).
    pub fn records(&self, c: &Container) -> Result<Vec<Record>, String> {
        let mut out = Vec::new();
        let (buckets, slots, pages) = match c.class {
            Class::C80 => (self.u32(c.va + 0x3C)?, self.u32(c.va + 0x48)?, self.u32(c.va + 0x70)?),
            Class::C50 => (self.u32(c.va + 0x20)?, self.u32(c.va + 0x34)?, self.u32(c.va + 0x38)?),
        };
        if buckets == 0 {
            return Ok(out);
        }
        let table = self.read(slots, buckets as usize * 4)?;
        for b in 0..buckets {
            let i = b as usize * 4;
            let v = u32::from_le_bytes([table[i], table[i + 1], table[i + 2], table[i + 3]]);
            if v == u32::MAX {
                continue;
            }
            // 0x80: the bucket holds the dense record index; 0x50: the record sits at the bucket.
            let (index, key) = match c.class {
                Class::C80 => (v, self.u32(self.u32(c.va + 0x44)? + v * 4)?),
                Class::C50 => (b, v),
            };
            let page = self.u32(pages + (index >> c.shift) * 4)?;
            let va = (index & (c.per_page - 1)) * c.stride as u32 + page;
            out.push(Record { key, va });
        }
        if c.class == Class::C80 && out.len() as u32 != c.count {
            return Err(format!("{}: walked {} records, live count says {}", c.name, out.len(), c.count));
        }
        Ok(out)
    }
}

/// A 16-bytes-per-row hex dump of `bytes` at `va`. Each dword is also shown as an f32 when it reads as
/// a plausible float (`0`, or magnitude 1e-4..1e7), else `.`.
pub fn hexdump(va: u32, bytes: &[u8]) -> String {
    let mut s = String::new();
    for (i, row) in bytes.chunks(16).enumerate() {
        let hexs: Vec<String> = row.iter().map(|b| format!("{b:02x}")).collect();
        let words: Vec<String> = row
            .chunks_exact(4)
            .map(|w| {
                let f = f32::from_le_bytes([w[0], w[1], w[2], w[3]]);
                if f == 0.0 || (f.is_finite() && (1e-4..1e7).contains(&f.abs())) {
                    format!("{f:>12.5}")
                } else {
                    format!("{:>12}", ".")
                }
            })
            .collect();
        s.push_str(&format!("0x{:08X}  {:<47}  {}\n", va + (i * 16) as u32, hexs.join(" "), words.join(" ")));
    }
    s
}

/// The v1.0 in-world captures, each paired with the save fixture the game had loaded (matched by its
/// header: hero `@0x4D`, cash `@0x18`, fuel `@0x1C`).
#[cfg(all(test, feature = "retail"))]
mod tests {
    use super::*;

    const PROFILE_PTR: u32 = 0x0117_6054; // PTR_01176054, the profile/economy singleton

    fn dumps_dir() -> PathBuf {
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        mercs2_formats::game_paths::local_config_memory_dumps_dir(start).unwrap_or_else(|e| panic!("{e}"))
    }

    fn fixture(name: &str) -> Vec<u8> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mercs2_formats/fixtures/saves").join(name);
        std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    /// The singleton's hero (`+0x61`), cash (`+0x2C`) and fuel (`+0x30`) equal the loaded save's header
    /// bytes in every v1.0 PMC capture — the reader resolves the decomp's addresses in live memory.
    #[test]
    fn profile_singleton_matches_the_loaded_save() {
        let root = dumps_dir();
        for (capture, save) in [
            ("ada55455_disc/pmc", "auto_6A447BF8.profile"),
            ("2ae4a2e0_nodrm-v3/pmc", "auto_6A447BF8.profile"),
            ("23fe7f38_cracked-cruise/pmc", "auto_6A0BE454.profile"),
            ("958eb227_cracked-pmcbb/pmc", "auto_6A0BE454.profile"),
        ] {
            let cap = Capture::open(&root.join(capture)).unwrap_or_else(|e| panic!("{e}"));
            let mem = cap.mem().unwrap();
            let p = mem.u32(PROFILE_PTR).unwrap();
            let hdr = fixture(save);
            assert_eq!(mem.u8(p + 0x61).unwrap(), hdr[0x4D], "{capture}: hero");
            assert_eq!(mem.read(p + 0x2C, 4).unwrap(), hdr[0x18..0x1C], "{capture}: cash");
            assert_eq!(mem.read(p + 0x30, 4).unwrap(), hdr[0x1C..0x20], "{capture}: fuel");
        }
    }

    /// Both container arrays are found whole (102 + 232, each self-checked `per_page == 2^shift`), every
    /// `0x80` walk returns its live count (checked inside `records`), and the hero the `Players` record
    /// names is in the PMC interior, Upright and idle.
    #[test]
    fn container_census_and_hero_state() {
        let cap = Capture::open(&dumps_dir().join("ada55455_disc/pmc")).unwrap_or_else(|e| panic!("{e}"));
        let mem = cap.mem().unwrap();
        let all = mem.containers().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(all.len(), 334);
        for c in all.iter().filter(|c| c.class == Class::C80) {
            mem.records(c).unwrap_or_else(|e| panic!("{e}"));
        }
        let players = mem.container_named("Players").unwrap();
        let p1 = mem.records(&players).unwrap().into_iter().find(|r| r.key == 0x4000_0014).expect("player 1");
        let hero = mem.u32(mem.u32(p1.va).unwrap() + 0x20).unwrap();
        let hsm = mem.container_named("HumanStateMachine").unwrap();
        let rec = mem.records(&hsm).unwrap().into_iter().find(|r| r.key == hero).expect("hero state machine");
        let sm = mem.u32(rec.va).unwrap();
        assert_eq!(mem.u32(sm + 4).unwrap(), 0x12C0_7B18, "Stance Upright");
        assert_eq!(mem.u32(sm + 8).unwrap(), 0xB4DA_003B, "Action idle");
        let rso = mem.container_named("RuntimeSceneObject").unwrap();
        let at = mem.records(&rso).unwrap().into_iter().find(|r| r.key == hero).expect("hero scene object");
        let f = |va: u32| f32::from_bits(mem.u32(va).unwrap());
        let pos = [f(at.va), f(at.va + 4), f(at.va + 8)];
        assert!((pos[0] - 3794.04).abs() < 0.01 && (pos[1] - 450.81).abs() < 0.01 && (pos[2] + 3911.03).abs() < 0.01, "{pos:?}");
    }

    /// The manifest-listed capture of the end-game Mattias save (`Mattias Nilsson_6A0E523C`, worn in the
    /// "Jacket" outfit): the singleton's `+0x63` is the costume and `+0x62` the upgrade, which puts them at
    /// header `0x4F` / `0x4E` (singleton offset − 0x14, like hero `+0x61` ↔ `0x4D`). Costume 3 is
    /// `mrxplayer.lua` `_tCharacterMap[1].models[3]` = `pmc_hum_mattias_v4`, the Jacket.
    #[test]
    fn costume_is_singleton_0x63_and_header_0x4f() {
        let dmp = dumps_dir().join("updated-chris-exports/20261021_mattias-nilson-jacket-skin.dmp");
        let cap = Capture::open(&dmp).unwrap_or_else(|e| panic!("{e}"));
        let mem = cap.mem().unwrap();
        let p = mem.u32(PROFILE_PTR).unwrap();
        let hdr = fixture("Mattias Nilsson_6A0E523C.profile");
        assert_eq!(mem.read(p + 0x2C, 4).unwrap(), hdr[0x18..0x1C], "cash: the fixture is the loaded save");
        assert_eq!(mem.read(p + 0x30, 4).unwrap(), hdr[0x1C..0x20], "fuel: the fixture is the loaded save");
        assert_eq!(mem.u8(p + 0x61).unwrap(), hdr[0x4D], "hero");
        assert_eq!(mem.u8(p + 0x62).unwrap(), 0, "upgrade tier");
        assert_eq!(hdr[0x4E], 0, "header upgrade byte");
        assert_eq!(mem.u8(p + 0x63).unwrap(), 3, "costume = Jacket");
        assert_eq!(hdr[0x4F], 3, "header costume byte");
    }

    /// An undriven node rests at its `+16` chain, NOT at its `+80` bind. In the same capture the
    /// render object of model `0x1FBFBB4B` (resident in the PMC; `OBJ = 0x1EB7F800`, `OBJ+0x1E0` = its
    /// model resource `0x1F22FE00`) keeps a 73-entry array of row-major model-space 4×4 node matrices
    /// at `OBJ+0x2B8` = `0x293A21E0`. Every one equals the `+16` chain; `+80` matches only the 8 nodes
    /// that own geometry. The rest are SEGM instance nodes, whose `+80` is a copy of their source's:
    /// resting at `+80` stacks every instance on its source (all 13 of a tank's other road wheels on
    /// one hub). `mercs2_mesh`'s rest palette `SkinData::bones` = `invBind · (+16 chain)` reproduces
    /// these matrices, so `skin_bind · bones[b]` must equal the live one.
    #[test]
    fn retail_node_matrices_are_the_plus16_chain() {
        const NODES: u32 = 0x293A_21E0;
        let cap = Capture::open(&dumps_dir().join("23fe7f38_cracked-cruise/pmc")).unwrap_or_else(|e| panic!("{e}"));
        let mem = cap.mem().unwrap();
        assert_eq!(mem.u32(0x1EB7_F800 + 0x1E0).unwrap(), 0x1F22_FE00, "OBJ+0x1E0 is the model resource");
        assert_eq!(mem.u32(0x1F22_FE00 + 0x04).unwrap(), 0x1FBF_BB4B, "the model resource names the model");
        assert_eq!(mem.u32(0x1EB7_F800 + 0x2B8).unwrap(), NODES, "OBJ+0x2B8 holds the node matrices");
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        let wad = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        let mut w = mercs2_engine::wad::open(wad.to_str().unwrap()).unwrap_or_else(|e| panic!("{e}"));
        let (m, _, _) = mercs2_engine::game_world::load_model_by_hash(&mut w, 0x1FBF_BB4B).expect("0x1FBFBB4B");
        assert_eq!(m.skin.rig.len(), 73);
        let f = |va: u32| f32::from_bits(mem.u32(va).unwrap());
        let mut bind_matches = 0;
        for (b, bone) in m.skin.rig.iter().enumerate() {
            let rest = mercs2_formats::skeleton::mat4_mul(&bone.world_bind, &m.skin.bones[b]);
            let live = |r: usize, c: usize| f(NODES + 64 * b as u32 + 16 * r as u32 + 4 * c as u32);
            let d = (0..4).flat_map(|r| (0..4).map(move |c| (r, c))).map(|(r, c)| (rest[r][c] - live(r, c)).abs()).fold(0.0f32, f32::max);
            assert!(d < 1e-3, "node #{b} 0x{:08X}: rest pose is {d:.4} off retail's live node matrix", bone.name_hash);
            let db = (0..4).flat_map(|r| (0..4).map(move |c| (r, c))).map(|(r, c)| (bone.world_bind[r][c] - live(r, c)).abs()).fold(0.0f32, f32::max);
            if db < 1e-3 {
                bind_matches += 1;
            }
        }
        assert_eq!(bind_matches, 8, "the +80 bind matches only the geometry-owning nodes");
    }

    /// The engine skins with retail's inverse-binds. In the `23fe7f38_cracked-cruise/pmc` capture
    /// (Mattias, base model `pmc_hum_mattias`, idle in the PMC) retail's skin palette for him is a
    /// 116-entry array of transposed 4×3 matrices at `0x293A3420` (the vertex-shader constant layout),
    /// and his model-space pose a 116-entry array of row-major 4×4 at `0x293A5FA0` — found by
    /// scanning for runs of rigid transforms; both start with three identities at (0, −0.06, 0).
    /// `palette = invBind · pose`, so `palette · pose⁻¹` is the inverse-bind retail actually used.
    /// It must equal `mercs2_mesh`'s `BoneRig::inv_bind` for every bone. The bound 0.03 covers the
    /// one-frame lag between the two arrays (worst residual 0.024, at the hands); it rejects the
    /// `+16`-chain inverse on every bone where that differs (by 0.09–1.5 on #3, #5–#8, #22–#24,
    /// #61, #83 — including the ancestor-copy records).
    #[test]
    fn retail_skin_matches_the_live_palette() {
        const PALETTE: u32 = 0x293A_3420;
        const POSE: u32 = 0x293A_5FA0;
        let cap = Capture::open(&dumps_dir().join("23fe7f38_cracked-cruise/pmc")).unwrap_or_else(|e| panic!("{e}"));
        let mem = cap.mem().unwrap();
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        let wad = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        let mut w = mercs2_engine::wad::open(wad.to_str().unwrap()).unwrap_or_else(|e| panic!("{e}"));
        let (m, _, _) = mercs2_engine::game_world::load_model_by_hash(&mut w, 0x0BBA_3066).expect("pmc_hum_mattias");
        let rig = &m.skin.rig;
        assert_eq!(rig.len(), 116);
        let f = |va: u32| f32::from_bits(mem.u32(va).unwrap());
        for (b, bone) in rig.iter().enumerate() {
            // Transposed 4×3: three float4 rows (R column | T component) → row-vector 4×4.
            let e = |r: u32, c: u32| f(PALETTE + 48 * b as u32 + 16 * r + 4 * c);
            let mut pal = [[0.0f32; 4]; 4];
            for i in 0..3 {
                for j in 0..3 {
                    pal[i][j] = e(j as u32, i as u32);
                }
                pal[3][i] = e(i as u32, 3);
            }
            pal[3][3] = 1.0;
            let mut pose = [[0.0f32; 4]; 4];
            for (i, row) in pose.iter_mut().enumerate() {
                for (j, v) in row.iter_mut().enumerate() {
                    *v = f(POSE + 64 * b as u32 + 16 * i as u32 + 4 * j as u32);
                }
            }
            let solved = mercs2_formats::skeleton::mat4_mul(&pal, &mercs2_formats::skeleton::affine_inverse(&pose));
            let d = (0..4).flat_map(|r| (0..4).map(move |c| (r, c))).map(|(r, c)| (solved[r][c] - bone.inv_bind[r][c]).abs()).fold(0.0f32, f32::max);
            assert!(d < 0.03, "bone #{b} 0x{:08X}: engine inv_bind is {d:.4} off retail's live palette", bone.name_hash);
        }
    }

    /// A v1.1 capture is refused (its addresses are not the decomp's), and an uncaptured address is an
    /// error rather than zeros.
    #[test]
    fn refuses_v11_and_uncaptured_memory() {
        let root = dumps_dir();
        let e = Capture::open(&root.join("a1532b4c_signed/pmc")).err().expect("v1.1 capture opened");
        assert!(e.contains("runs 1.1 code"), "{e}");
        let cap = Capture::open(&root.join("ada55455_disc/pmc")).unwrap_or_else(|e| panic!("{e}"));
        let e = cap.mem().unwrap().u32(0x10).unwrap_err();
        assert!(e.contains("was not captured"), "{e}");
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(s.trim_start_matches('\u{feff}').to_string())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
