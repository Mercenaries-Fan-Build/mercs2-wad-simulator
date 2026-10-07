//! FX cluster: the resident `fxdict` (`INFO` + `DICT`, the sprite rectangles of the `vfx` atlas) and
//! the per-effect UCFX tree.
//!
//! Both are **PC little-endian on-disk** forms. The spec lives in the notes repo:
//! `docs/effect_container_format.md` (the effect tree), `docs/ucfx_tree_container.md` (the
//! container), `docs/fxdict_format.md` (the dictionary).
//!
//! # The effect asset (type `0x5608BD5A`, ASET type id 29)
//!
//! An effect is ONE UCFX tree rooted at `EFCT`:
//!
//! ```text
//! EFCT (18 B: 9 × u16, computed — see EffectContainer::efct_words)
//! ├─ EMTR (u16 = number of GEOM children)
//! │  └─ GEOM × n            u32 k + k × 13 f32   (emitter shapes)
//! ├─ EMIT (marker)          ┐ one pair per emitter
//! │  ├─ TRFM (64 B 4×4)     │
//! │  │  └─ ATRB × 9         │ posx posy posz rotx roty rotz sclx scly sclz
//! │  └─ GEOM (4 B, opt.)    │ u16 shape index, u16 sampled record count
//! ├─ PTYP (u32 flags)       │
//! │  ├─ ATRB × 19           │ fixed hash order (PTYP_ATTRIBUTES_BEFORE_COLR)
//! │  ├─ COLR (800 B)        │ 100 × {u8×4, binary16, u16 0}
//! │  ├─ ATRB × 13           │ fixed hash order (PTYP_ATTRIBUTES_AFTER_COLR)
//! │  └─ TEXT                ┘ u32 n + n × u32 fxdict frame key
//! └─ FRCE × k               u32 kind hash + kind parameters
//!    └─ ATRB × (7 + kind extras)
//! ATRB (12 B) {u32 hash, u32 flags, u32|f32 value} → optional ANIM (u32 = key count) → AKEY × n
//! AKEY (8 B)  {f32 time, f32 value}
//! ```
//!
//! Every loader address below is in `mercs2_unpacked.exe`: the effect driver `FUN_00491920`
//! (EFCT/EMTR/FRCE/PTYP/EMIT dispatch), the PTYP-child reader `FUN_00492af0` (ATRB/TEXT/COLR), the
//! EMIT walker `FUN_0048cc30` (TRFM/GEOM) and the TRFM-channel reader `FUN_00493150`.
//!
//! [`parse_effect_container`] and [`write_effect_container`] are exact inverses: every one of the 314
//! retail effects re-encodes byte-for-byte (`tests/effect_retail_roundtrip.rs`). Neither side
//! carries a best-effort path — anything outside the measured format is an error that says what.

use crate::ucfx::{parse_ucfx_tree, write_ucfx_tree, UcfxNode};

fn read_u16_le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn read_u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn read_f32_le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn read_vec3(b: &[u8], o: usize) -> [f32; 3] {
    [read_f32_le(b, o), read_f32_le(b, o + 4), read_f32_le(b, o + 8)]
}
fn put_f32s(out: &mut Vec<u8>, v: &[f32]) {
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

// ------------------------------------------------------------------------------------------------
// fxdict (INFO + DICT) — the sprite rectangles of the `vfx` atlas, one per frame key.
// ------------------------------------------------------------------------------------------------

/// On-disk DICT record stride (630 × 20 = 12600 bytes in retail, zero slack).
pub const DICT_RECORD_BYTES: usize = 20;
/// Retail fxdict record count (`resident_P000_Q3`).
pub const DICT_RETAIL_COUNT: usize = 630;

/// One fxdict record (20 bytes on disk): the rectangle of one sprite frame in the `vfx` atlas
/// `0x89E211AF`, in atlas-normalised units.
///
/// `v` is measured from the BOTTOM of the atlas: the record's top edge is at `1 − v − h` from the
/// top. The loader `FUN_00491320` expands each record to 32 bytes, `(key, u, 1 − v − h, w, h)`, and
/// the lookup `FUN_00491510` binary-searches the keys with a signed `i32` compare, so the records
/// are sorted by `key as i32` and a key appears once. A key the search misses draws `(0, 0, 1, 1)`,
/// the whole atlas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxRect {
    /// The frame key: the hash an effect's `TEXT` names.
    pub key: u32,
    /// Left edge.
    pub u: f32,
    /// Bottom edge, measured up from the atlas's bottom.
    pub v: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl FxRect {
    /// The top edge measured down from the atlas's top: `1 − v − h`, the value the loader stores.
    pub fn top(&self) -> f32 {
        1.0 - self.v - self.h
    }
}

/// Parse the fxdict from its container `INFO` (`u32 entry_count`) and `DICT` body
/// (`entry_count × 20` bytes). Returns one [`FxRect`] per record, in file order.
///
/// Faithful to the loader: the count comes from INFO, records are read at a fixed 20-byte stride.
/// Trailing bytes past `count × 20` are ignored (the engine only walks `count`).
pub fn parse_fxdict(info: &[u8], dict: &[u8]) -> Result<Vec<FxRect>, String> {
    if info.len() < 4 {
        return Err(format!("fxdict INFO too short: {} bytes (need 4)", info.len()));
    }
    let count = read_u32_le(info, 0) as usize;
    let need = count
        .checked_mul(DICT_RECORD_BYTES)
        .ok_or_else(|| format!("fxdict count {count} overflows"))?;
    if dict.len() < need {
        return Err(format!(
            "fxdict DICT {} bytes < {need} needed ({count} × {DICT_RECORD_BYTES})",
            dict.len()
        ));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = i * DICT_RECORD_BYTES;
        out.push(FxRect {
            key: read_u32_le(dict, o),
            u: read_f32_le(dict, o + 4),
            v: read_f32_le(dict, o + 8),
            w: read_f32_le(dict, o + 12),
            h: read_f32_le(dict, o + 16),
        });
    }
    Ok(out)
}

pub fn write_fxrect(r: &FxRect) -> [u8; DICT_RECORD_BYTES] {
    let mut out = [0u8; DICT_RECORD_BYTES];
    out[0..4].copy_from_slice(&r.key.to_le_bytes());
    out[4..8].copy_from_slice(&r.u.to_le_bytes());
    out[8..12].copy_from_slice(&r.v.to_le_bytes());
    out[12..16].copy_from_slice(&r.w.to_le_bytes());
    out[16..20].copy_from_slice(&r.h.to_le_bytes());
    out
}

pub fn write_fxdict_dict(records: &[FxRect]) -> Vec<u8> {
    let mut out = Vec::with_capacity(records.len() * DICT_RECORD_BYTES);
    for r in records {
        out.extend_from_slice(&write_fxrect(r));
    }
    out
}

pub fn write_fxdict_info(count: u32) -> [u8; 4] {
    count.to_le_bytes()
}

/// Sort records the way the lookup `FUN_00491510` searches them: by `key as i32`. Two records of
/// one key are an error, naming the key: the search would find either.
pub fn sort_fxdict(records: &mut [FxRect]) -> Result<(), String> {
    records.sort_by_key(|r| r.key as i32);
    if let Some(pair) = records.windows(2).find(|p| p[0].key == p[1].key) {
        return Err(format!("fxdict carries key 0x{:08X} twice", pair[0].key));
    }
    Ok(())
}

/// The resident fxdict container: two top-level leaves, `INFO` then `DICT`.
pub fn write_fxdict_container(records: &[FxRect]) -> Vec<u8> {
    write_ucfx_tree(&[
        UcfxNode::leaf(*b"INFO", write_fxdict_info(records.len() as u32).to_vec()),
        UcfxNode::leaf(*b"DICT", write_fxdict_dict(records)),
    ])
}

/// Parse a whole fxdict container. Strict: exactly `INFO` (4 B) then `DICT` (`count × 20` B), no
/// children, no slack — the shape [`write_fxdict_container`] writes and the retail singleton has.
pub fn parse_fxdict_container(container: &[u8]) -> Result<Vec<FxRect>, String> {
    let roots = parse_ucfx_tree(container)?;
    let [info, dict] = roots.as_slice() else {
        return Err(format!("fxdict container has {} top-level rows, not INFO + DICT", roots.len()));
    };
    for (n, tag) in [(info, b"INFO"), (dict, b"DICT")] {
        if &n.tag != tag || !n.children.is_empty() || n.body.is_none() {
            return Err(format!("fxdict row '{}' is not a leaf '{}'", n.tag_str(), String::from_utf8_lossy(tag)));
        }
    }
    let info_b = info.body.as_deref().unwrap_or_default();
    let dict_b = dict.body.as_deref().unwrap_or_default();
    if info_b.len() != 4 {
        return Err(format!("fxdict INFO is {} bytes, not 4", info_b.len()));
    }
    let count = read_u32_le(info_b, 0) as usize;
    if dict_b.len() != count * DICT_RECORD_BYTES {
        return Err(format!(
            "fxdict DICT is {} bytes, not {count} × {DICT_RECORD_BYTES}",
            dict_b.len()
        ));
    }
    parse_fxdict(info_b, dict_b)
}

// ------------------------------------------------------------------------------------------------
// Effect container — sizes and constants.
// ------------------------------------------------------------------------------------------------

/// `EFCT` word 1, read and skipped by `FUN_00491920`; 0x0226 in all 314 retail effects.
pub const EFCT_MAGIC: u16 = 0x0226;
/// `EFCT` body: nine u16 words.
pub const EFCT_BYTES: usize = 18;
/// `TRFM` body: a 4×4 f32 matrix.
pub const TRFM_BYTES: usize = 64;
/// `ATRB` body: `{u32 hash, u32 flags, u32|f32 value}`.
pub const ATRB_BYTES: usize = 12;
/// `AKEY` body: `{f32 time, f32 value}`.
pub const AKEY_BYTES: usize = 8;
/// f32s in one emitter-shape record (`EMTR/GEOM`): the loader allocates `k × 0x34` and copies 13
/// words each (`FUN_00491920`, EMTR arm).
pub const SHAPE_RECORD_FLOATS: usize = 13;
/// Keys in a `COLR` body.
pub const COLR_KEYS: usize = 100;
/// Bytes per `COLR` key.
pub const COLR_KEY_BYTES: usize = 8;
/// `COLR` body: `FUN_00492af0` copies exactly 800 bytes (`vtable+0x14(dst, 800, 0)`).
pub const COLR_BYTES: usize = COLR_KEYS * COLR_KEY_BYTES;
/// Stream-table words the loader reserves for a `COLR` (`*desc = 200`).
pub const COLR_STREAM_WORDS: u16 = 200;
/// Stream-table words `EFCT[8]` reserves for a `TEXT` in every retail effect.
pub const TEXT_STREAM_WORDS: u16 = 200;
/// Samples a resampled curve is expanded to (`FUN_00493150`, `*desc = 100`).
pub const RESAMPLED_CURVE_WORDS: u16 = 100;

/// `ATRB` flag bits. Bits 7-10 are what `FUN_00493150` packs as `b7<<3 | b10<<2 | b8<<1 | b9`.
pub mod atrb_flag {
    /// The value word is an f32 (clear: a u32). Derived from [`super::AtrbValue`].
    pub const FLOAT: u32 = 1 << 0;
    /// Authored option bit 7 (meaning unproven).
    pub const BIT7: u32 = 1 << 7;
    /// Resample the curve to 100 samples (`FUN_00493150`: packed bit 1 → the 100-sample branch).
    pub const RESAMPLE: u32 = 1 << 8;
    /// Authored option bit 9 (meaning unproven).
    pub const BIT9: u32 = 1 << 9;
    /// The attribute owns an `ANIM` curve. Derived from [`super::Atrb::curve`]; set on all 1,880
    /// retail ATRBs that have an ANIM child and on no other.
    pub const CURVE: u32 = 1 << 10;
    /// The authored option bits — the only bits not derived from the value and the curve.
    pub const OPTIONS: u32 = BIT7 | RESAMPLE | BIT9;
    /// Every bit any retail ATRB sets.
    pub const KNOWN: u32 = FLOAT | OPTIONS | CURVE;
}

// ------------------------------------------------------------------------------------------------
// Attributes (ATRB → ANIM → AKEY).
// ------------------------------------------------------------------------------------------------

/// An `ATRB` value word.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AtrbValue {
    F32(f32),
    U32(u32),
}

/// One `AKEY`: a curve key. Retail times run 0..100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimKey {
    pub time: f32,
    pub value: f32,
}

/// One `ATRB`, with its optional `ANIM` curve.
///
/// `flags` is the authored flag word. Bits 0 and 10 are DERIVED (value type, curve presence) and
/// bits 7/8/9 are authored options; [`Atrb::validate`] rejects a word that disagrees with the value
/// or the curve rather than writing it.
#[derive(Debug, Clone, PartialEq)]
pub struct Atrb {
    pub hash: u32,
    pub flags: u32,
    pub value: AtrbValue,
    pub curve: Option<Vec<AnimKey>>,
}

impl Atrb {
    /// An f32 attribute with no curve and no option bits.
    pub fn f32(hash: u32, value: f32) -> Self {
        Atrb { hash, flags: atrb_flag::FLOAT, value: AtrbValue::F32(value), curve: None }
    }
    /// A u32 attribute (a hash, an enum, a count).
    pub fn u32(hash: u32, value: u32) -> Self {
        Atrb { hash, flags: 0, value: AtrbValue::U32(value), curve: None }
    }
    /// Attach a curve (sets [`atrb_flag::CURVE`]).
    pub fn with_curve(mut self, keys: Vec<AnimKey>) -> Self {
        self.flags |= atrb_flag::CURVE;
        self.curve = Some(keys);
        self
    }
    /// OR authored option bits into the flag word ([`Atrb::validate`] rejects non-option bits).
    pub fn with_options(mut self, bits: u32) -> Self {
        self.flags |= bits;
        self
    }

    /// The flag word implied by the value, the curve and the authored option bits.
    pub fn derived_flags(&self) -> u32 {
        let mut f = self.flags & atrb_flag::OPTIONS;
        if matches!(self.value, AtrbValue::F32(_)) {
            f |= atrb_flag::FLOAT;
        }
        if self.curve.is_some() {
            f |= atrb_flag::CURVE;
        }
        f
    }

    /// Reject a flag word that disagrees with the value type or the curve, unknown flag bits,
    /// options or a curve on a u32 value (no retail attribute has either), and an empty curve.
    pub fn validate(&self) -> Result<(), String> {
        let unknown = self.flags & !atrb_flag::KNOWN;
        if unknown != 0 {
            return Err(format!(
                "ATRB 0x{:08X}: flag bits 0x{unknown:08X} are not ones any retail ATRB sets",
                self.hash
            ));
        }
        if self.flags != self.derived_flags() {
            return Err(format!(
                "ATRB 0x{:08X}: authored flags 0x{:08X} disagree with the value/curve (expected 0x{:08X}: \
                 bit 0 = f32 value, bit 10 = has curve)",
                self.hash,
                self.flags,
                self.derived_flags()
            ));
        }
        if let AtrbValue::U32(_) = self.value {
            if self.flags != 0 {
                return Err(format!(
                    "ATRB 0x{:08X}: a u32 attribute carries flags 0x{:08X}; no retail u32 attribute has options or a curve",
                    self.hash, self.flags
                ));
            }
        }
        if let Some(keys) = &self.curve {
            if keys.is_empty() {
                return Err(format!("ATRB 0x{:08X}: an ANIM curve needs at least one key", self.hash));
            }
        }
        Ok(())
    }

    fn body(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(ATRB_BYTES);
        b.extend_from_slice(&self.hash.to_le_bytes());
        b.extend_from_slice(&self.flags.to_le_bytes());
        match self.value {
            AtrbValue::F32(v) => b.extend_from_slice(&v.to_le_bytes()),
            AtrbValue::U32(v) => b.extend_from_slice(&v.to_le_bytes()),
        }
        b
    }

    fn to_node(&self) -> UcfxNode {
        let children = match &self.curve {
            None => Vec::new(),
            Some(keys) => vec![UcfxNode::with_children(
                *b"ANIM",
                (keys.len() as u32).to_le_bytes().to_vec(),
                keys.iter()
                    .map(|k| {
                        let mut b = Vec::with_capacity(AKEY_BYTES);
                        put_f32s(&mut b, &[k.time, k.value]);
                        UcfxNode::leaf(*b"AKEY", b)
                    })
                    .collect(),
            )],
        };
        UcfxNode::with_children(*b"ATRB", self.body(), children)
    }

    fn from_node(n: &UcfxNode) -> Result<Atrb, String> {
        let b = leaf_body(n, b"ATRB", Some(ATRB_BYTES))?;
        let hash = read_u32_le(b, 0);
        let flags = read_u32_le(b, 4);
        let value = if flags & atrb_flag::FLOAT != 0 {
            AtrbValue::F32(read_f32_le(b, 8))
        } else {
            AtrbValue::U32(read_u32_le(b, 8))
        };
        let curve = match n.children.as_slice() {
            [] => None,
            [anim] => {
                let ab = leaf_body(anim, b"ANIM", Some(4))?;
                let declared = read_u32_le(ab, 0) as usize;
                if declared != anim.children.len() {
                    return Err(format!(
                        "ATRB 0x{hash:08X}: ANIM declares {declared} keys but has {} AKEY rows",
                        anim.children.len()
                    ));
                }
                let mut keys = Vec::with_capacity(declared);
                for k in &anim.children {
                    let kb = leaf_body(k, b"AKEY", Some(AKEY_BYTES))?;
                    no_children(k)?;
                    keys.push(AnimKey { time: read_f32_le(kb, 0), value: read_f32_le(kb, 4) });
                }
                Some(keys)
            }
            more => {
                return Err(format!("ATRB 0x{hash:08X} has {} children; at most one ANIM", more.len()));
            }
        };
        let a = Atrb { hash, flags, value, curve };
        a.validate()?;
        Ok(a)
    }
}

/// The value type an attribute position takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    F32,
    U32,
}

/// What the loader does with a curve on an attribute position — which decides how the curve is
/// counted in `EFCT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveUse {
    /// A TRFM channel (`FUN_00493150`): a linear table entry of `2 × keys` words, or of 100 words
    /// when [`atrb_flag::RESAMPLE`] is set.
    Channel,
    /// Stored as `2 × keys` words in the linear-curve table (`EFCT[3]`/`[4]`). PTYP positions that
    /// dispatch to the handler at `0x024EEC10`, and the FRCE positions retail puts curves on.
    Linear,
    /// Resampled to 100 words in the stream table (`EFCT[7]`/`[8]`) — PTYP positions that dispatch
    /// to the handler at `0x024E2380` (the size family), whatever the flag bits.
    Resampled,
    /// Carried in the file but not counted in `EFCT` — PTYP positions that dispatch to the handler
    /// at `0x024EEBE0` (the `…var` family). Retail has two such curves (on `speedvar`).
    Unconsumed,
    /// No retail effect carries a curve here and the loader's handling of one is not decoded, so
    /// the writer refuses a curve at this position.
    Refused,
}

/// One attribute position: its hash, recovered name (FNV inversion of `pandemic_hash_m2`) when one
/// is known, value type, and curve handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttrDef {
    pub hash: u32,
    pub name: Option<&'static str>,
    pub kind: ValueKind,
    pub curve: CurveUse,
}

const fn def(hash: u32, name: Option<&'static str>, kind: ValueKind, curve: CurveUse) -> AttrDef {
    AttrDef { hash, name, kind, curve }
}
use CurveUse::{Channel, Linear, Refused, Resampled, Unconsumed};
use ValueKind::{F32, U32};

/// The nine `TRFM` channels, in file order. All dispatch through `FUN_0048cc30` → `FUN_00493150`.
pub const TRFM_CHANNELS: [AttrDef; 9] = [
    def(0x7D1117EB, Some("posx"), F32, Channel),
    def(0x9B0F088E, Some("posy"), F32, Channel),
    def(0xFD165E99, Some("posz"), F32, Channel),
    def(0xF6B55F38, Some("rotx"), F32, Channel),
    def(0x20B7DFED, Some("roty"), F32, Channel),
    def(0x9EB05782, Some("rotz"), F32, Channel),
    def(0xC5574C5F, Some("sclx"), F32, Channel),
    def(0xE3553D02, Some("scly"), F32, Channel),
    def(0x655CC56D, Some("sclz"), F32, Channel),
];

/// The 19 `PTYP` attributes before `COLR`, in file order (820/820 retail PTYPs).
pub const PTYP_ATTRIBUTES_BEFORE_COLR: [AttrDef; 19] = [
    def(0x1DE5C824, Some("name"), U32, Refused),
    def(0x8C3654C2, Some("size"), F32, Resampled),
    def(0xC1008E25, Some("sizevar"), F32, Refused),
    def(0xC3AEB321, Some("mass"), F32, Refused),
    def(0x47035D90, Some("massvar"), F32, Refused),
    def(0xD3AE67AF, Some("life"), F32, Linear),
    def(0xC7CFE6AA, Some("lifevar"), F32, Unconsumed),
    def(0x497A1895, None, F32, Resampled),
    def(0x1792B524, None, F32, Refused),
    def(0x10831673, None, F32, Refused),
    def(0xB6197EFE, None, F32, Refused),
    def(0x6CF0BE14, None, F32, Refused),
    def(0xBE968D3B, None, F32, Refused),
    def(0xC558C9D8, None, F32, Refused),
    def(0xD80DF37F, None, F32, Refused),
    def(0x720F22F8, None, F32, Refused),
    def(0x4712719F, None, F32, Refused),
    def(0xB3DBB6C0, None, F32, Refused),
    def(0x4C49B137, None, F32, Refused),
];

/// The 13 `PTYP` attributes after `COLR`, in file order (820/820 retail PTYPs).
pub const PTYP_ATTRIBUTES_AFTER_COLR: [AttrDef; 13] = [
    def(0xC3592BB7, None, U32, Refused),
    def(0xEEE1A341, Some("scale"), U32, Refused),
    def(0x062F0D37, Some("rate"), F32, Linear),
    def(0x70653182, Some("ratevar"), F32, Refused),
    def(0x437F66EC, Some("spread"), F32, Linear),
    def(0xB8A95DE3, Some("spreadvar"), F32, Unconsumed),
    def(0x15BA509E, Some("speed"), F32, Linear),
    def(0x9BE62E41, Some("speedvar"), F32, Unconsumed),
    def(0xB4247BF3, Some("inheritvel"), F32, Linear),
    def(0xB15ABB7E, Some("inheritvelvar"), F32, Unconsumed),
    def(0x35ACBAB7, None, F32, Resampled),
    def(0x1B878602, None, F32, Refused),
    def(0x270C9E9D, Some("emissiondir"), U32, Refused),
];

/// The seven attributes every `FRCE` starts with, in file order.
pub const FRCE_COMMON_ATTRIBUTES: [AttrDef; 7] = [
    def(0x3DC3D9DF, Some("ampl"), F32, Linear),
    def(0x7D1117EB, Some("posx"), F32, Linear),
    def(0x9B0F088E, Some("posy"), F32, Linear),
    def(0xFD165E99, Some("posz"), F32, Linear),
    def(0xF6B55F38, Some("rotx"), F32, Refused),
    def(0x20B7DFED, Some("roty"), F32, Refused),
    def(0x9EB05782, Some("rotz"), F32, Refused),
];
/// `drag`'s extra attribute.
pub const FRCE_DRAG_ATTRIBUTES: [AttrDef; 1] = [def(0x2F68CD8F, None, F32, Refused)];
/// `attractor`'s extra attributes.
pub const FRCE_ATTRACTOR_ATTRIBUTES: [AttrDef; 3] = [
    def(0x201B5A86, Some("local"), U32, Refused),
    def(0xE0686A68, Some("range"), F32, Refused),
    def(0xC2783A55, Some("decay"), F32, Refused),
];
/// `vortex`'s extra attributes.
pub const FRCE_VORTEX_ATTRIBUTES: [AttrDef; 4] = [
    def(0x201B5A86, Some("local"), U32, Refused),
    def(0xE0686A68, Some("range"), F32, Refused),
    def(0xC2783A55, Some("decay"), F32, Refused),
    def(0xB46F1F0C, Some("radial"), F32, Refused),
];

/// The name recovered for an attribute hash, from every position table.
pub fn attribute_name(hash: u32) -> Option<&'static str> {
    TRFM_CHANNELS
        .iter()
        .chain(PTYP_ATTRIBUTES_BEFORE_COLR.iter())
        .chain(PTYP_ATTRIBUTES_AFTER_COLR.iter())
        .chain(FRCE_COMMON_ATTRIBUTES.iter())
        .chain(FRCE_DRAG_ATTRIBUTES.iter())
        .chain(FRCE_VORTEX_ATTRIBUTES.iter())
        .find(|d| d.hash == hash)
        .and_then(|d| d.name)
}

/// Check a run of attributes against its position table: same count, same hashes in the same
/// order, the table's value type, a curve only where the table allows one.
fn check_positions(what: &str, attrs: &[Atrb], defs: &[&[AttrDef]]) -> Result<(), String> {
    let defs: Vec<&AttrDef> = defs.iter().flat_map(|d| d.iter()).collect();
    if attrs.len() != defs.len() {
        return Err(format!("{what}: {} attributes, the format has {}", attrs.len(), defs.len()));
    }
    for (i, (a, d)) in attrs.iter().zip(defs).enumerate() {
        let label = d.name.map(str::to_string).unwrap_or_else(|| format!("0x{:08X}", d.hash));
        if a.hash != d.hash {
            return Err(format!(
                "{what}: attribute {i} is 0x{:08X}; position {i} is {label} (0x{:08X})",
                a.hash, d.hash
            ));
        }
        a.validate()?;
        let kind_ok = matches!(
            (a.value, d.kind),
            (AtrbValue::F32(_), ValueKind::F32) | (AtrbValue::U32(_), ValueKind::U32)
        );
        if !kind_ok {
            return Err(format!("{what}: {label} takes a {:?} value, got {:?}", d.kind, a.value));
        }
        if a.curve.is_some() && d.curve == CurveUse::Refused {
            return Err(format!(
                "{what}: {label} carries a curve; no retail effect has one there and the loader's \
                 handling of it is not decoded"
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// COLR / TEXT.
// ------------------------------------------------------------------------------------------------

/// One `COLR` key (8 bytes on disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColrKey {
    /// Four colour bytes, in file order. The channel order is not proven; the retail keys read as
    /// three equal-ish bytes plus a fourth that fades to 0 over the 100 keys.
    pub colour: [u8; 4],
    /// A binary16 bit pattern (`0x3C00` = 1.0 and `0xBC00` = -1.0 are both in retail). Its role is
    /// not proven; it is carried verbatim.
    pub half_bits: u16,
}

/// `COLR` — 100 keys spread over the particle's life. The trailing u16 of each key is 0 in all
/// 82,000 retail keys; the reader rejects anything else and the writer writes 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colr {
    pub keys: [ColrKey; COLR_KEYS],
}

impl Colr {
    /// Every key the same.
    pub fn uniform(colour: [u8; 4], half_bits: u16) -> Self {
        Colr { keys: [ColrKey { colour, half_bits }; COLR_KEYS] }
    }

    /// Build from a function of normalised age `t` in 0..=1 (key `i` is at `i / 99`).
    pub fn from_fn(mut f: impl FnMut(f32) -> ([u8; 4], u16)) -> Self {
        let mut keys = [ColrKey { colour: [0; 4], half_bits: 0 }; COLR_KEYS];
        for (i, k) in keys.iter_mut().enumerate() {
            let (colour, half_bits) = f(i as f32 / (COLR_KEYS - 1) as f32);
            *k = ColrKey { colour, half_bits };
        }
        Colr { keys }
    }

    /// The four colour bytes at normalised age `t` (0 = spawn, 1 = death), linearly interpolated
    /// between the two nearest keys and scaled to 0..1. Channel order as stored (unproven).
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let scaled = t.clamp(0.0, 1.0) * (COLR_KEYS - 1) as f32;
        let i0 = scaled.floor() as usize;
        let i1 = (i0 + 1).min(COLR_KEYS - 1);
        let f = scaled - i0 as f32;
        let (a, b) = (self.keys[i0].colour, self.keys[i1].colour);
        let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f) / 255.0;
        [lerp(a[0], b[0]), lerp(a[1], b[1]), lerp(a[2], b[2]), lerp(a[3], b[3])]
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(COLR_BYTES);
        for k in &self.keys {
            b.extend_from_slice(&k.colour);
            b.extend_from_slice(&k.half_bits.to_le_bytes());
            b.extend_from_slice(&0u16.to_le_bytes());
        }
        b
    }

    pub fn from_bytes(b: &[u8]) -> Result<Colr, String> {
        if b.len() != COLR_BYTES {
            return Err(format!("COLR is {} bytes, not {COLR_BYTES}", b.len()));
        }
        let mut keys = [ColrKey { colour: [0; 4], half_bits: 0 }; COLR_KEYS];
        for (i, k) in keys.iter_mut().enumerate() {
            let o = i * COLR_KEY_BYTES;
            let tail = read_u16_le(b, o + 6);
            if tail != 0 {
                return Err(format!("COLR key {i}: trailing u16 is 0x{tail:04X}, not 0"));
            }
            *k = ColrKey { colour: [b[o], b[o + 1], b[o + 2], b[o + 3]], half_bits: read_u16_le(b, o + 4) };
        }
        Ok(Colr { keys })
    }
}

/// `TEXT` — the sprite frames: `u32 n` then `n` frame keys, each the key of an fxdict record
/// ([`FxRect`]), a rectangle of the `vfx` atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub frames: Vec<u32>,
}

impl Text {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(4 + 4 * self.frames.len());
        b.extend_from_slice(&(self.frames.len() as u32).to_le_bytes());
        for h in &self.frames {
            b.extend_from_slice(&h.to_le_bytes());
        }
        b
    }

    pub fn from_bytes(b: &[u8]) -> Result<Text, String> {
        if b.len() < 4 {
            return Err(format!("TEXT is {} bytes; needs the u32 frame count", b.len()));
        }
        let n = read_u32_le(b, 0) as usize;
        if b.len() != 4 + 4 * n {
            return Err(format!("TEXT declares {n} frames but is {} bytes (not 4 + 4·{n})", b.len()));
        }
        Ok(Text { frames: (0..n).map(|i| read_u32_le(b, 4 + 4 * i)).collect() })
    }
}

// ------------------------------------------------------------------------------------------------
// FRCE.
// ------------------------------------------------------------------------------------------------

/// `pandemic_hash_m2("gravity")`.
pub const FORCE_GRAVITY: u32 = 0x14BD1BBD;
/// `pandemic_hash_m2("drag")`.
pub const FORCE_DRAG: u32 = 0xED791C4B;
/// `pandemic_hash_m2("wind")`.
pub const FORCE_WIND: u32 = 0xC9F7A9D7;
/// `pandemic_hash_m2("attractor")`.
pub const FORCE_ATTRACTOR: u32 = 0xC235456B;
/// `pandemic_hash_m2("vortex")`.
pub const FORCE_VORTEX: u32 = 0xF4D85A49;

/// A force and its parameters, read by the `FRCE` arm of `FUN_00491920`. Field names that describe a
/// meaning (`magnitude`, `direction`) are INFERRED from the retail values; the rest are named by the
/// runtime offset the loader writes them to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ForceKind {
    /// Mode 1: f32 → `+0x12C`, vec3 → `+0x110`.
    Gravity { magnitude: f32, direction: [f32; 3] },
    /// Mode 2: f32 → `+0x12C`.
    Drag { magnitude: f32 },
    /// Mode 0: f32 → `+0x12C`, vec3 → `+0x110`; also sets effect flag `+0x94 |= 2`.
    Wind { magnitude: f32, direction: [f32; 3] },
    /// Mode 3: f32 → `+0x12C`, u32 (read as `!= 0`) → `+0x13C`, f32 → `+0x130`, f32 → `+0x134`,
    /// vec3 → `+0x104`.
    Attractor { magnitude: f32, flag_13c: u32, param_130: f32, param_134: f32, vector_104: [f32; 3] },
    /// Mode 4: as attractor plus f32 → `+0x138`, then three vec3s → `+0x104`, `+0x110`, `+0x11C`
    /// (the loader builds a basis from the cross product of the last two).
    Vortex {
        magnitude: f32,
        flag_13c: u32,
        param_130: f32,
        param_134: f32,
        param_138: f32,
        vector_104: [f32; 3],
        vector_110: [f32; 3],
        vector_11c: [f32; 3],
    },
}

impl ForceKind {
    pub fn hash(&self) -> u32 {
        match self {
            ForceKind::Gravity { .. } => FORCE_GRAVITY,
            ForceKind::Drag { .. } => FORCE_DRAG,
            ForceKind::Wind { .. } => FORCE_WIND,
            ForceKind::Attractor { .. } => FORCE_ATTRACTOR,
            ForceKind::Vortex { .. } => FORCE_VORTEX,
        }
    }

    /// The `FRCE` body size for this kind (8 / 20 / 20 / 32 / 60).
    pub fn body_len(&self) -> usize {
        match self {
            ForceKind::Drag { .. } => 8,
            ForceKind::Gravity { .. } | ForceKind::Wind { .. } => 20,
            ForceKind::Attractor { .. } => 32,
            ForceKind::Vortex { .. } => 60,
        }
    }

    /// The attributes after the common seven, for this kind.
    pub fn extra_attributes(&self) -> &'static [AttrDef] {
        match self {
            ForceKind::Gravity { .. } | ForceKind::Wind { .. } => &[],
            ForceKind::Drag { .. } => &FRCE_DRAG_ATTRIBUTES,
            ForceKind::Attractor { .. } => &FRCE_ATTRACTOR_ATTRIBUTES,
            ForceKind::Vortex { .. } => &FRCE_VORTEX_ATTRIBUTES,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.body_len());
        b.extend_from_slice(&self.hash().to_le_bytes());
        match *self {
            ForceKind::Gravity { magnitude, direction } | ForceKind::Wind { magnitude, direction } => {
                put_f32s(&mut b, &[magnitude]);
                put_f32s(&mut b, &direction);
            }
            ForceKind::Drag { magnitude } => put_f32s(&mut b, &[magnitude]),
            ForceKind::Attractor { magnitude, flag_13c, param_130, param_134, vector_104 } => {
                put_f32s(&mut b, &[magnitude]);
                b.extend_from_slice(&flag_13c.to_le_bytes());
                put_f32s(&mut b, &[param_130, param_134]);
                put_f32s(&mut b, &vector_104);
            }
            ForceKind::Vortex {
                magnitude,
                flag_13c,
                param_130,
                param_134,
                param_138,
                vector_104,
                vector_110,
                vector_11c,
            } => {
                put_f32s(&mut b, &[magnitude]);
                b.extend_from_slice(&flag_13c.to_le_bytes());
                put_f32s(&mut b, &[param_130, param_134, param_138]);
                put_f32s(&mut b, &vector_104);
                put_f32s(&mut b, &vector_110);
                put_f32s(&mut b, &vector_11c);
            }
        }
        b
    }

    pub fn from_bytes(b: &[u8]) -> Result<ForceKind, String> {
        if b.len() < 4 {
            return Err(format!("FRCE is {} bytes; needs the u32 kind hash", b.len()));
        }
        let hash = read_u32_le(b, 0);
        let f = |o: usize| read_f32_le(b, o);
        let (kind, need) = match hash {
            FORCE_GRAVITY | FORCE_WIND if b.len() == 20 => {
                let (magnitude, direction) = (f(4), read_vec3(b, 8));
                let k = if hash == FORCE_GRAVITY {
                    ForceKind::Gravity { magnitude, direction }
                } else {
                    ForceKind::Wind { magnitude, direction }
                };
                (k, 20)
            }
            FORCE_DRAG if b.len() == 8 => (ForceKind::Drag { magnitude: f(4) }, 8),
            FORCE_ATTRACTOR if b.len() == 32 => (
                ForceKind::Attractor {
                    magnitude: f(4),
                    flag_13c: read_u32_le(b, 8),
                    param_130: f(12),
                    param_134: f(16),
                    vector_104: read_vec3(b, 20),
                },
                32,
            ),
            FORCE_VORTEX if b.len() == 60 => (
                ForceKind::Vortex {
                    magnitude: f(4),
                    flag_13c: read_u32_le(b, 8),
                    param_130: f(12),
                    param_134: f(16),
                    param_138: f(20),
                    vector_104: read_vec3(b, 24),
                    vector_110: read_vec3(b, 36),
                    vector_11c: read_vec3(b, 48),
                },
                60,
            ),
            FORCE_GRAVITY | FORCE_WIND | FORCE_DRAG | FORCE_ATTRACTOR | FORCE_VORTEX => {
                return Err(format!("FRCE kind 0x{hash:08X} with a {}-byte body (wrong size for the kind)", b.len()));
            }
            _ => return Err(format!("FRCE kind 0x{hash:08X} is not one the loader dispatches")),
        };
        debug_assert_eq!(need, b.len());
        Ok(kind)
    }
}

/// One `FRCE`: the typed force and its attribute children.
#[derive(Debug, Clone, PartialEq)]
pub struct Force {
    pub kind: ForceKind,
    /// [`FRCE_COMMON_ATTRIBUTES`] then [`ForceKind::extra_attributes`], in that order.
    pub attributes: Vec<Atrb>,
}

// ------------------------------------------------------------------------------------------------
// Emitters.
// ------------------------------------------------------------------------------------------------

/// One `EMTR/GEOM`: a table of 13-f32 shape records, referenced by [`EmitterGeom::shape_index`].
///
/// A record is a triangle the emitter spawns particles on. `FUN_00488770` reads three of its
/// vectors: the vertex `P` (floats 4–6), and the edges `A` (7–9) and `B` (10–12); a particle starts
/// at `P + u·A + v·B` with `u` uniform in `[0, 1)` and `v` uniform in `[0, 1 − u)`, and the vector at
/// floats 1–3 is copied out beside it. In all 13,148 retail records floats 1–3 are a unit vector
/// along `±(A × B)` and float 0 is `|A × B|`; `FUN_00488770` does not read float 0.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitterShape {
    pub records: Vec<[f32; SHAPE_RECORD_FLOATS]>,
}

/// An emitter's `GEOM` (4 bytes). `FUN_0048cc30` stores `shapes[shape_index]` at `+0x04` of the
/// EMIT record and `word_00`, sign-extended, at `+0x00`. `FUN_0048ae80` draws each particle's record
/// as `random % word_00` (an unsigned divide) from the table at `+0x04`: `word_00` is the number
/// of records the emitter samples, the record count of its shape in all 811 retail `GEOM`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitterGeom {
    pub shape_index: u16,
    pub word_00: u16,
}

/// The largest `GEOM` record count the engine samples within its table: `FUN_0048cc30` stores the
/// word as `(int)(short)`, so a word of `0x8000` or above is a negative count, and the unsigned
/// divide in `FUN_0048ae80` then yields an index past the table.
pub const GEOM_MAX_SAMPLED_RECORDS: u16 = 0x7FFF;

/// `PTYP` and its children.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleType {
    /// Bit 0 → emitter `+0x205`, bit 1 → emitter `+0x206` (`FUN_00491920`). No other bit is read.
    pub flags: u32,
    /// [`PTYP_ATTRIBUTES_BEFORE_COLR`] then [`PTYP_ATTRIBUTES_AFTER_COLR`] — 32, in that order.
    pub attributes: Vec<Atrb>,
    pub colr: Colr,
    pub text: Text,
}

/// One emitter: an `EMIT` marker (TRFM + channels + optional GEOM) and the `PTYP` that follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Emitter {
    /// The `TRFM` 4×4, rows as stored.
    pub transform: [[f32; 4]; 4],
    /// The nine [`TRFM_CHANNELS`], in order.
    pub channels: Vec<Atrb>,
    /// 811 of 820 retail emitters have one; [`EffectContainer::check_emitter_shapes`] says when an
    /// emitter may go without.
    pub geom: Option<EmitterGeom>,
    pub particle: ParticleType,
}

/// A whole effect.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectContainer {
    pub shapes: Vec<EmitterShape>,
    pub emitters: Vec<Emitter>,
    pub forces: Vec<Force>,
}

/// PTYP flag bits the loader reads.
pub const PTYP_KNOWN_FLAGS: u32 = 0b11;

impl EffectContainer {
    /// Validate everything the writer relies on, naming the first violation: the node rules
    /// ([`Self::validate_nodes`]), then the emitter-shape rules ([`Self::check_emitter_shapes`]).
    pub fn validate(&self) -> Result<(), String> {
        self.validate_nodes()?;
        self.check_emitter_shapes()
    }

    /// The node rules: at least one emitter and one shape, every attribute run against its position
    /// table, the PTYP flags and the TEXT frames.
    pub fn validate_nodes(&self) -> Result<(), String> {
        if self.emitters.is_empty() {
            return Err("an effect needs at least one emitter (EFCT[0] = PTYP count)".into());
        }
        if self.shapes.is_empty() {
            return Err("an effect needs at least one EMTR shape (every retail EMTR has one)".into());
        }
        for (i, e) in self.emitters.iter().enumerate() {
            let w = format!("emitter {i}");
            check_positions(&format!("{w} TRFM"), &e.channels, &[&TRFM_CHANNELS])?;
            let p = &e.particle;
            if p.flags & !PTYP_KNOWN_FLAGS != 0 {
                return Err(format!("{w}: PTYP flags 0x{:08X} set bits the loader never reads", p.flags));
            }
            check_positions(
                &format!("{w} PTYP"),
                &p.attributes,
                &[&PTYP_ATTRIBUTES_BEFORE_COLR, &PTYP_ATTRIBUTES_AFTER_COLR],
            )?;
            let n = p.text.frames.len();
            if n == 0 {
                return Err(format!("{w}: TEXT has no frames (the loader reads one regardless)"));
            }
            // With PTYP bit 1 the loader reserves 2·n stream words for TEXT; EFCT[8] reserves 200.
            if p.flags & 2 != 0 && 2 * n > TEXT_STREAM_WORDS as usize {
                return Err(format!(
                    "{w}: PTYP bit 1 with {n} TEXT frames needs {} stream words; EFCT reserves {TEXT_STREAM_WORDS}",
                    2 * n
                ));
            }
        }
        for (i, f) in self.forces.iter().enumerate() {
            check_positions(
                &format!("force {i}"),
                &f.attributes,
                &[&FRCE_COMMON_ATTRIBUTES, f.kind.extra_attributes()],
            )?;
        }
        Ok(())
    }

    /// The emitter-shape rules: every emitter the engine can spawn a particle from has a shape table
    /// it samples within.
    ///
    /// A spawned effect starts in mode 0 (`FUN_00488d70` zeroes the instance's `+0x770`), and in
    /// mode 0 `FUN_0048ae80` draws each new particle's spawn record as `random % count` from the
    /// table its `GEOM` set ([`EmitterGeom`]). So:
    ///
    /// * a `GEOM` names a shape of the `EMTR` table;
    /// * its `word_00` is at least 1 (0 divides by zero), at most the named shape's record count
    ///   (beyond it the engine reads past the table), and at most [`GEOM_MAX_SAMPLED_RECORDS`];
    /// * an emitter without `GEOM` has a count of 0, so it divides by zero on its first particle; it
    ///   is accepted only when its `rate` can spawn none: `FUN_0048f4f0` adds
    ///   `max(rate + (1 − 2u)·ratevar, 0)` (`u` uniform in `[0, 1)`, `FUN_00490960`) to the emitter's
    ///   spawn count each frame, so `rate` is a constant at or below 0 (no curve) and `ratevar` is 0.
    ///   The 9 retail emitters without `GEOM` are of this kind. Every template that starts such an
    ///   effect also has a zero per-distance factor (`RedEffectComponent` `0x62C7746E`), the other
    ///   term of the spawn count; that is the template's rule, checked where templates are.
    pub fn check_emitter_shapes(&self) -> Result<(), String> {
        let rate = PTYP_ATTRIBUTES_AFTER_COLR[2].hash;
        let ratevar = PTYP_ATTRIBUTES_AFTER_COLR[3].hash;
        for (i, e) in self.emitters.iter().enumerate() {
            let w = format!("emitter {i}");
            match e.geom {
                Some(g) => {
                    let s = g.shape_index as usize;
                    let Some(shape) = self.shapes.get(s) else {
                        return Err(format!(
                            "{w}: GEOM shape index {s} but EMTR has {} shapes; the engine would take the \
                             emitter's shape table from past the EMTR table",
                            self.shapes.len()
                        ));
                    };
                    let n = shape.records.len();
                    let k = g.word_00;
                    if n == 0 {
                        return Err(format!(
                            "{w}: GEOM names shape {s}, which has no records; the engine picks every \
                             particle's spawn record from the shape, as random % count, and a shape \
                             without records has none to pick"
                        ));
                    }
                    if k == 0 {
                        return Err(format!(
                            "{w}: GEOM samples 0 records of shape {s}; the engine picks every particle's \
                             spawn record as random % count, so a count of 0 divides by zero. Give the \
                             shape's record count, {n}"
                        ));
                    }
                    if k as usize > n {
                        return Err(format!(
                            "{w}: GEOM samples {k} records of shape {s}, which has {n}; the engine picks \
                             each spawn record as random % {k} and reads past the shape's records"
                        ));
                    }
                    if k > GEOM_MAX_SAMPLED_RECORDS {
                        return Err(format!(
                            "{w}: GEOM samples {k} records; the engine reads the count as a signed 16-bit \
                             number, so a count above {GEOM_MAX_SAMPLED_RECORDS} is negative and picks \
                             records past the shape"
                        ));
                    }
                }
                None => {
                    let at = |h: u32| e.particle.attributes.iter().find(|a| a.hash == h);
                    let spawns_none = match (at(rate), at(ratevar)) {
                        (Some(r), Some(v)) => {
                            r.curve.is_none()
                                && matches!(r.value, AtrbValue::F32(x) if x <= 0.0)
                                && v.value == AtrbValue::F32(0.0)
                        }
                        _ => false,
                    };
                    if !spawns_none {
                        return Err(format!(
                            "{w} has no GEOM, so its shape table has 0 records, and the engine divides by \
                             zero picking the spawn record of its first particle (random % 0). Give it a \
                             GEOM naming a shape. An emitter without GEOM spawns no particle only with a \
                             constant rate at or below 0 and a ratevar of 0"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// The nine `EFCT` words, computed:
    ///
    /// | word | value |
    /// |---|---|
    /// | 0 | emitters (PTYP count) |
    /// | 1 | [`EFCT_MAGIC`] |
    /// | 2 | forces (FRCE count) |
    /// | 3 | linear-table curves: TRFM channel curves, PTYP [`CurveUse::Linear`] curves, FRCE curves |
    /// | 4 | their words: `2 × keys` each, or 100 for a TRFM curve with [`atrb_flag::RESAMPLE`] |
    /// | 5, 6 | 0 |
    /// | 7 | stream-table entries: COLR + TEXT per emitter + PTYP [`CurveUse::Resampled`] curves |
    /// | 8 | their words: 200 per COLR, 200 per TEXT, 100 per resampled curve |
    ///
    /// The loader reads the words as three `(count, words)` table reservations (`FUN_00491920`),
    /// and fills tables 1 and 3 from `FUN_00493150` and `FUN_00492af0`. Equal to the stored words
    /// in 314/314 retail effects. The TRFM-resample branch has no retail instance; that it lands in
    /// the linear table follows from `FUN_00493150` using one table pointer for both branches.
    pub fn efct_words(&self) -> Result<[u16; 9], String> {
        let mut lin = 0usize;
        let mut lin_words = 0usize;
        let mut stream = 0usize;
        let mut stream_words = 0usize;
        for e in &self.emitters {
            for a in &e.channels {
                if let Some(keys) = &a.curve {
                    lin += 1;
                    lin_words += if a.flags & atrb_flag::RESAMPLE != 0 {
                        RESAMPLED_CURVE_WORDS as usize
                    } else {
                        2 * keys.len()
                    };
                }
            }
            stream += 2;
            stream_words += (COLR_STREAM_WORDS + TEXT_STREAM_WORDS) as usize;
            let defs = PTYP_ATTRIBUTES_BEFORE_COLR.iter().chain(PTYP_ATTRIBUTES_AFTER_COLR.iter());
            for (a, d) in e.particle.attributes.iter().zip(defs) {
                if let Some(keys) = &a.curve {
                    match d.curve {
                        CurveUse::Linear => {
                            lin += 1;
                            lin_words += 2 * keys.len();
                        }
                        CurveUse::Resampled => {
                            stream += 1;
                            stream_words += RESAMPLED_CURVE_WORDS as usize;
                        }
                        CurveUse::Unconsumed => {}
                        CurveUse::Channel | CurveUse::Refused => {
                            return Err(format!("PTYP attribute 0x{:08X} carries a curve it cannot", a.hash));
                        }
                    }
                }
            }
        }
        for f in &self.forces {
            for a in &f.attributes {
                if let Some(keys) = &a.curve {
                    lin += 1;
                    lin_words += 2 * keys.len();
                }
            }
        }
        let w = |v: usize, what: &str| -> Result<u16, String> {
            if v > i16::MAX as usize {
                Err(format!("EFCT {what} = {v} exceeds the loader's signed-16-bit read"))
            } else {
                Ok(v as u16)
            }
        };
        Ok([
            w(self.emitters.len(), "emitter count")?,
            EFCT_MAGIC,
            w(self.forces.len(), "force count")?,
            w(lin, "linear-curve count")?,
            w(lin_words, "linear-curve words")?,
            0,
            0,
            w(stream, "stream-table count")?,
            w(stream_words, "stream-table words")?,
        ])
    }

    /// Build the UCFX tree (validates first).
    pub fn to_tree(&self) -> Result<UcfxNode, String> {
        self.validate()?;
        let efct: Vec<u8> = self.efct_words()?.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut kids = Vec::with_capacity(1 + 2 * self.emitters.len() + self.forces.len());

        let shapes = self
            .shapes
            .iter()
            .map(|s| {
                let mut b = Vec::with_capacity(4 + 4 * SHAPE_RECORD_FLOATS * s.records.len());
                b.extend_from_slice(&(s.records.len() as u32).to_le_bytes());
                for r in &s.records {
                    put_f32s(&mut b, r);
                }
                UcfxNode::leaf(*b"GEOM", b)
            })
            .collect();
        kids.push(UcfxNode::with_children(
            *b"EMTR",
            (self.shapes.len() as u16).to_le_bytes().to_vec(),
            shapes,
        ));

        for e in &self.emitters {
            let mut trfm = Vec::with_capacity(TRFM_BYTES);
            for row in &e.transform {
                put_f32s(&mut trfm, row);
            }
            let mut emit = vec![UcfxNode::with_children(
                *b"TRFM",
                trfm,
                e.channels.iter().map(Atrb::to_node).collect(),
            )];
            if let Some(g) = e.geom {
                let mut b = g.shape_index.to_le_bytes().to_vec();
                b.extend_from_slice(&g.word_00.to_le_bytes());
                emit.push(UcfxNode::leaf(*b"GEOM", b));
            }
            kids.push(UcfxNode::marker(*b"EMIT", emit));

            let p = &e.particle;
            let split = PTYP_ATTRIBUTES_BEFORE_COLR.len();
            let mut pk: Vec<UcfxNode> = p.attributes[..split].iter().map(Atrb::to_node).collect();
            pk.push(UcfxNode::leaf(*b"COLR", p.colr.to_bytes()));
            pk.extend(p.attributes[split..].iter().map(Atrb::to_node));
            pk.push(UcfxNode::leaf(*b"TEXT", p.text.to_bytes()));
            kids.push(UcfxNode::with_children(*b"PTYP", p.flags.to_le_bytes().to_vec(), pk));
        }

        for f in &self.forces {
            kids.push(UcfxNode::with_children(
                *b"FRCE",
                f.kind.to_bytes(),
                f.attributes.iter().map(Atrb::to_node).collect(),
            ));
        }
        Ok(UcfxNode::with_children(*b"EFCT", efct, kids))
    }
}

/// Encode an effect as its UCFX container (validated, EFCT computed, CSUM appended).
pub fn write_effect_container(effect: &EffectContainer) -> Result<Vec<u8>, String> {
    Ok(write_ucfx_tree(&[effect.to_tree()?]))
}

fn leaf_body<'a>(n: &'a UcfxNode, tag: &[u8; 4], len: Option<usize>) -> Result<&'a [u8], String> {
    if &n.tag != tag {
        return Err(format!("expected '{}', found '{}'", String::from_utf8_lossy(tag), n.tag_str()));
    }
    let b = n
        .body
        .as_deref()
        .ok_or_else(|| format!("'{}' is a marker row; it must own a body", n.tag_str()))?;
    if let Some(l) = len {
        if b.len() != l {
            return Err(format!("'{}' is {} bytes, not {l}", n.tag_str(), b.len()));
        }
    }
    Ok(b)
}

fn no_children(n: &UcfxNode) -> Result<(), String> {
    if n.children.is_empty() {
        Ok(())
    } else {
        Err(format!("'{}' has {} children; it is a leaf", n.tag_str(), n.children.len()))
    }
}

fn atrbs(nodes: &[UcfxNode]) -> Result<Vec<Atrb>, String> {
    nodes.iter().map(Atrb::from_node).collect()
}

/// Parse an effect container. Strict: the tree must be exactly the shape in the module docs, every
/// attribute run must match its position table, and the stored `EFCT` must equal
/// [`EffectContainer::efct_words`].
pub fn parse_effect_container(container: &[u8]) -> Result<EffectContainer, String> {
    let roots = parse_ucfx_tree(container)?;
    let [root] = roots.as_slice() else {
        return Err(format!("effect container has {} top-level rows, not one EFCT", roots.len()));
    };
    let efct = leaf_body(root, b"EFCT", Some(EFCT_BYTES))?;
    let stored: Vec<u16> = (0..9).map(|i| read_u16_le(efct, 2 * i)).collect();
    let mut kids = root.children.iter().peekable();

    let emtr = kids.next().ok_or("EFCT has no children")?;
    let eb = leaf_body(emtr, b"EMTR", Some(2))?;
    let declared = read_u16_le(eb, 0) as usize;
    if declared != emtr.children.len() {
        return Err(format!("EMTR declares {declared} shapes but has {} GEOM rows", emtr.children.len()));
    }
    let mut shapes = Vec::with_capacity(declared);
    for g in &emtr.children {
        let gb = leaf_body(g, b"GEOM", None)?;
        no_children(g)?;
        if gb.len() < 4 {
            return Err(format!("EMTR GEOM is {} bytes; needs the u32 record count", gb.len()));
        }
        let k = read_u32_le(gb, 0) as usize;
        if gb.len() != 4 + 4 * SHAPE_RECORD_FLOATS * k {
            return Err(format!("EMTR GEOM declares {k} records but is {} bytes", gb.len()));
        }
        let records = (0..k)
            .map(|r| {
                let mut rec = [0f32; SHAPE_RECORD_FLOATS];
                for (j, v) in rec.iter_mut().enumerate() {
                    *v = read_f32_le(gb, 4 + 4 * (r * SHAPE_RECORD_FLOATS + j));
                }
                rec
            })
            .collect();
        shapes.push(EmitterShape { records });
    }

    let mut emitters = Vec::new();
    while kids.peek().is_some_and(|n| &n.tag == b"EMIT") {
        let emit = kids.next().expect("peeked");
        if emit.body.is_some() {
            return Err("EMIT must be a marker row".into());
        }
        let (trfm_n, geom_n) = match emit.children.as_slice() {
            [t] => (t, None),
            [t, g] => (t, Some(g)),
            other => return Err(format!("EMIT has {} children; TRFM and an optional GEOM", other.len())),
        };
        let tb = leaf_body(trfm_n, b"TRFM", Some(TRFM_BYTES))?;
        let mut transform = [[0f32; 4]; 4];
        for (r, row) in transform.iter_mut().enumerate() {
            for (c, v) in row.iter_mut().enumerate() {
                *v = read_f32_le(tb, 4 * (4 * r + c));
            }
        }
        let channels = atrbs(&trfm_n.children)?;
        let geom = match geom_n {
            None => None,
            Some(g) => {
                let gb = leaf_body(g, b"GEOM", Some(4))?;
                no_children(g)?;
                Some(EmitterGeom { shape_index: read_u16_le(gb, 0), word_00: read_u16_le(gb, 2) })
            }
        };

        let ptyp = kids.next().ok_or("EMIT is not followed by a PTYP")?;
        let pb = leaf_body(ptyp, b"PTYP", Some(4))?;
        let split = PTYP_ATTRIBUTES_BEFORE_COLR.len();
        let total = split + PTYP_ATTRIBUTES_AFTER_COLR.len();
        let pk = &ptyp.children;
        if pk.len() != total + 2 {
            return Err(format!("PTYP has {} children, not {total} ATRB + COLR + TEXT", pk.len()));
        }
        let colr_n = &pk[split];
        let text_n = &pk[total + 1];
        let colr = Colr::from_bytes(leaf_body(colr_n, b"COLR", None)?)?;
        no_children(colr_n)?;
        let text = Text::from_bytes(leaf_body(text_n, b"TEXT", None)?)?;
        no_children(text_n)?;
        let mut attributes = atrbs(&pk[..split])?;
        attributes.extend(atrbs(&pk[split + 1..total + 1])?);
        emitters.push(Emitter {
            transform,
            channels,
            geom,
            particle: ParticleType { flags: read_u32_le(pb, 0), attributes, colr, text },
        });
    }

    let mut forces = Vec::new();
    for f in kids {
        let fb = leaf_body(f, b"FRCE", None)?;
        forces.push(Force { kind: ForceKind::from_bytes(fb)?, attributes: atrbs(&f.children)? });
    }

    let effect = EffectContainer { shapes, emitters, forces };
    effect.validate()?;
    let computed = effect.efct_words()?;
    if stored != computed {
        return Err(format!("stored EFCT {stored:?} != computed {computed:?}"));
    }
    Ok(effect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::pandemic_hash_m2;
    use crate::ucfx::read_ucfx_rows;

    fn le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    fn rect_bytes(key: u32, u: f32, v: f32, w: f32, h: f32) -> Vec<u8> {
        let mut b = le(key).to_vec();
        for f in [u, v, w, h] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b
    }

    #[test]
    fn a_record_reads_as_key_u_v_w_h_and_its_top_is_one_minus_v_minus_h() {
        let info = 1u32.to_le_bytes();
        let dict = rect_bytes(0xAABBCCDD, 0.75, 0.5, 0.125, 0.25);
        let records = parse_fxdict(&info, &dict).unwrap();
        assert_eq!(records, vec![FxRect { key: 0xAABBCCDD, u: 0.75, v: 0.5, w: 0.125, h: 0.25 }]);
        assert_eq!(records[0].top(), 0.25);
    }

    #[test]
    fn the_retail_count_reads_from_a_zeroed_dict() {
        let info = (DICT_RETAIL_COUNT as u32).to_le_bytes();
        let dict = vec![0u8; DICT_RETAIL_COUNT * DICT_RECORD_BYTES];
        let records = parse_fxdict(&info, &dict).unwrap();
        assert_eq!(records.len(), 630);
        assert_eq!(dict.len(), 12600);
    }

    #[test]
    fn fxdict_ignores_trailing_slack() {
        let info = 2u32.to_le_bytes();
        let dict = vec![0u8; 2 * DICT_RECORD_BYTES + 7];
        assert_eq!(parse_fxdict(&info, &dict).unwrap().len(), 2);
    }

    #[test]
    fn fxdict_rejects_short_inputs() {
        assert!(parse_fxdict(&[0, 0], &[]).is_err());
        let info = 3u32.to_le_bytes();
        assert!(parse_fxdict(&info, &[0u8; 40]).is_err());
    }

    #[test]
    fn a_record_writes_and_reads_back() {
        let r = FxRect { key: 0xDEADBEEF, u: 0.25, v: 0.5, w: 0.03125, h: 0.0625 };
        let bytes = write_fxrect(&r);
        assert_eq!(bytes.to_vec(), rect_bytes(0xDEADBEEF, 0.25, 0.5, 0.03125, 0.0625));
        let back = parse_fxdict(&1u32.to_le_bytes(), &bytes).unwrap();
        assert_eq!(back, vec![r]);
    }

    #[test]
    fn records_sort_by_signed_key_and_a_repeated_key_is_refused() {
        let at = |key: u32| FxRect { key, u: 0.0, v: 0.0, w: 0.0, h: 0.0 };
        let mut records = vec![at(0x0000_0002), at(0x8000_0000), at(0xFFFF_FFFF), at(0x7FFF_FFFF), at(0x0000_0001)];
        sort_fxdict(&mut records).unwrap();
        let keys: Vec<u32> = records.iter().map(|r| r.key).collect();
        assert_eq!(keys, vec![0x8000_0000, 0xFFFF_FFFF, 0x0000_0001, 0x0000_0002, 0x7FFF_FFFF]);
        let mut twice = vec![at(5), at(9), at(5)];
        assert_eq!(sort_fxdict(&mut twice).unwrap_err(), "fxdict carries key 0x00000005 twice");
    }

    #[test]
    fn fxdict_container_is_info_then_dict_and_round_trips() {
        let records: Vec<FxRect> = (0..8)
            .map(|i| FxRect { key: 0x1000 + i, u: i as f32 / 8.0, v: 0.5, w: 1.0 / 8.0, h: 1.0 / 16.0 })
            .collect();
        let c = write_fxdict_container(&records);
        assert!(crate::ucfx::verify_ucfx_container(&c, "fxd", 0).is_none());
        let rows = read_ucfx_rows(&c).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((&rows[0].tag, rows[0].rel_off, rows[0].size, rows[0].x2, rows[0].x3), (b"INFO", 0, 4, 1, 0));
        assert_eq!((&rows[1].tag, rows[1].rel_off, rows[1].size, rows[1].x2, rows[1].x3), (b"DICT", 4, 160, 0, 0));
        assert_eq!(parse_fxdict_container(&c).unwrap(), records);
    }

    // ---- effect -------------------------------------------------------------------------------

    fn channels() -> Vec<Atrb> {
        TRFM_CHANNELS
            .iter()
            .enumerate()
            .map(|(i, d)| Atrb::f32(d.hash, if i >= 6 { 1.0 } else { 0.0 }))
            .collect()
    }

    fn force_attrs(kind: &ForceKind) -> Vec<Atrb> {
        FRCE_COMMON_ATTRIBUTES
            .iter()
            .chain(kind.extra_attributes())
            .map(|d| match d.kind {
                ValueKind::F32 => Atrb::f32(d.hash, 0.0),
                ValueKind::U32 => Atrb::u32(d.hash, 0),
            })
            .collect()
    }

    /// Every attribute position given an explicit authored value.
    fn ptyp_attrs(set: &[(u32, Atrb)]) -> Vec<Atrb> {
        PTYP_ATTRIBUTES_BEFORE_COLR
            .iter()
            .chain(PTYP_ATTRIBUTES_AFTER_COLR.iter())
            .map(|d| match set.iter().find(|(h, _)| *h == d.hash) {
                Some((_, a)) => a.clone(),
                None => match d.kind {
                    ValueKind::F32 => Atrb::f32(d.hash, 0.0),
                    ValueKind::U32 => Atrb::u32(d.hash, 0),
                },
            })
            .collect()
    }

    const MAGENTA: [u8; 4] = [0xFF, 0x00, 0xFF, 0xFF];

    /// A one-emitter burst: magenta over its whole life, one texture, `life` ≈ 1.0 s, `spread`
    /// 180° (the widest retail value; a half-angle of 180° covers the sphere — INFERRED).
    fn magenta_burst(texture: u32) -> EffectContainer {
        let h = pandemic_hash_m2;
        let set = [
            (h("name"), Atrb::u32(h("name"), h("magenta_burst"))),
            (h("size"), Atrb::f32(h("size"), 0.5)),
            (h("life"), Atrb::f32(h("life"), 1.0)),
            (h("rate"), Atrb::f32(h("rate"), 100.0)),
            (h("spread"), Atrb::f32(h("spread"), 180.0)),
            (h("speed"), Atrb::f32(h("speed"), 3.0)),
            (h("scale"), Atrb::u32(h("scale"), 1)),
            (0xC3592BB7, Atrb::u32(0xC3592BB7, 2)),
        ];
        EffectContainer {
            shapes: vec![EmitterShape { records: vec![[0.0; SHAPE_RECORD_FLOATS]] }],
            emitters: vec![Emitter {
                transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
                channels: channels(),
                geom: Some(EmitterGeom { shape_index: 0, word_00: 1 }),
                particle: ParticleType {
                    flags: 1,
                    attributes: ptyp_attrs(&set),
                    colr: Colr::uniform(MAGENTA, 0x3C00),
                    text: Text { frames: vec![texture] },
                },
            }],
            forces: vec![],
        }
    }

    /// Independent tree check over raw rows: `x3` = the rows of the subtree, `x2` = siblings after.
    fn check_tree_rules(rows: &[crate::ucfx::UcfxRow]) {
        fn level(rows: &[crate::ucfx::UcfxRow], start: usize, end: usize) {
            let mut starts = Vec::new();
            let mut i = start;
            while i < end {
                starts.push(i);
                i += rows[i].x3 as usize + 1;
            }
            assert_eq!(i, end, "subtree overruns its parent");
            for (k, &s) in starts.iter().enumerate() {
                assert_eq!(rows[s].x2 as usize, starts.len() - 1 - k, "row {s} x2");
                level(rows, s + 1, s + 1 + rows[s].x3 as usize);
            }
        }
        level(rows, 0, rows.len());
    }

    #[test]
    fn magenta_burst_writes_reparses_and_follows_the_tree_rules() {
        let texture = 0xB73157C0;
        let fx = magenta_burst(texture);
        let bytes = write_effect_container(&fx).unwrap();
        assert!(crate::ucfx::verify_ucfx_container(&bytes, "magenta", crate::types::TYPE_HASH_EFFECT).is_none());
        let back = parse_effect_container(&bytes).unwrap();
        assert_eq!(back, fx);
        assert_eq!(write_effect_container(&back).unwrap(), bytes);

        let rows = read_ucfx_rows(&bytes).unwrap();
        check_tree_rules(&rows);
        let tags: Vec<&[u8; 4]> = rows.iter().map(|r| &r.tag).collect();
        // EFCT, EMTR, GEOM, EMIT, TRFM, 9 ATRB, GEOM, PTYP, 19 ATRB, COLR, 13 ATRB, TEXT.
        assert_eq!(rows.len(), 3 + 1 + 1 + 9 + 1 + 1 + 19 + 1 + 13 + 1);
        assert_eq!((tags[0], rows[0].x2, rows[0].x3), (b"EFCT", 0, rows.len() as u32 - 1));
        assert_eq!((tags[1], rows[1].x2, rows[1].x3), (b"EMTR", 2, 1));
        assert_eq!((tags[3], rows[3].rel_off, rows[3].size, rows[3].x2, rows[3].x3), (b"EMIT", 0xFFFF_FFFF, 0, 1, 11));
        let ptyp = rows.iter().position(|r| &r.tag == b"PTYP").unwrap();
        assert_eq!((rows[ptyp].x2, rows[ptyp].x3), (0, 34));
        let last = rows.last().unwrap();
        assert_eq!((&last.tag, last.x2, last.x3), (b"TEXT", 0, 0));

        let p = &back.emitters[0].particle;
        assert!(p.colr.keys.iter().all(|k| k.colour == MAGENTA));
        assert_eq!(p.text.frames, vec![texture]);
        let life = p.attributes.iter().find(|a| a.hash == pandemic_hash_m2("life")).unwrap();
        assert_eq!(life.value, AtrbValue::F32(1.0));
        let spread = p.attributes.iter().find(|a| a.hash == pandemic_hash_m2("spread")).unwrap();
        assert_eq!(spread.value, AtrbValue::F32(180.0));
        // One emitter, no curves: EFCT = [1, magic, 0, 0, 0, 0, 0, 2 (COLR+TEXT), 400].
        assert_eq!(back.efct_words().unwrap(), [1, EFCT_MAGIC, 0, 0, 0, 0, 0, 2, 400]);
    }

    #[test]
    fn colr_is_800_bytes_of_100_keys() {
        let c = Colr::from_fn(|t| ([(t * 255.0) as u8, 1, 2, 3], 0x3C00));
        let b = c.to_bytes();
        assert_eq!(b.len(), 800);
        assert_eq!(&b[8..16], &[2, 1, 2, 3, 0x00, 0x3C, 0, 0]);
        assert_eq!(Colr::from_bytes(&b).unwrap(), c);
        assert!(Colr::from_bytes(&b[..200]).is_err());
        let mut bad = b.clone();
        bad[7] = 1;
        assert!(Colr::from_bytes(&bad).unwrap_err().contains("trailing"));
        assert!((c.sample(1.0)[0] - 1.0).abs() < 1e-6);
        assert_eq!(c.sample(0.0)[0], 0.0);
    }

    #[test]
    fn each_force_kind_has_its_measured_size_and_hash() {
        let v = [1.0, 2.0, 3.0];
        let kinds = [
            (ForceKind::Gravity { magnitude: 17.0, direction: [0.0, -1.0, 0.0] }, 20, "gravity"),
            (ForceKind::Drag { magnitude: 0.5 }, 8, "drag"),
            (ForceKind::Wind { magnitude: 2.0, direction: v }, 20, "wind"),
            (
                ForceKind::Attractor { magnitude: 1.0, flag_13c: 1, param_130: 2.0, param_134: 3.0, vector_104: v },
                32,
                "attractor",
            ),
            (
                ForceKind::Vortex {
                    magnitude: 1.0,
                    flag_13c: 0,
                    param_130: 2.0,
                    param_134: 3.0,
                    param_138: 4.0,
                    vector_104: v,
                    vector_110: [0.0, 1.0, 0.0],
                    vector_11c: [1.0, 0.0, 0.0],
                },
                60,
                "vortex",
            ),
        ];
        for (k, len, name) in kinds {
            assert_eq!(k.hash(), pandemic_hash_m2(name), "{name}");
            let b = k.to_bytes();
            assert_eq!(b.len(), len, "{name}");
            assert_eq!(k.body_len(), len);
            assert_eq!(ForceKind::from_bytes(&b).unwrap(), k);
            assert!(ForceKind::from_bytes(&b[..len - 4]).is_err(), "{name} short body");
        }
        assert!(ForceKind::from_bytes(b"GRAV\0\0\0\0").unwrap_err().contains("not one the loader"));
    }

    #[test]
    fn forces_write_with_their_attributes_and_count_in_efct() {
        let mut fx = magenta_burst(1);
        for kind in [ForceKind::Gravity { magnitude: 9.8, direction: [0.0, -1.0, 0.0] }, ForceKind::Drag { magnitude: 0.3 }] {
            fx.forces.push(Force { attributes: force_attrs(&kind), kind });
        }
        // A linear curve on the force's `ampl` (retail puts curves there).
        fx.forces[0].attributes[0] = Atrb::f32(pandemic_hash_m2("ampl"), 1.0)
            .with_curve(vec![AnimKey { time: 0.0, value: 1.0 }, AnimKey { time: 100.0, value: 0.0 }]);
        let bytes = write_effect_container(&fx).unwrap();
        let back = parse_effect_container(&bytes).unwrap();
        assert_eq!(back, fx);
        assert_eq!(back.efct_words().unwrap(), [1, EFCT_MAGIC, 2, 1, 4, 0, 0, 2, 400]);
    }

    #[test]
    fn curves_count_by_the_handler_their_position_dispatches_to() {
        let mut fx = magenta_burst(1);
        let keys = |n: usize| (0..n).map(|i| AnimKey { time: i as f32, value: 0.0 }).collect::<Vec<_>>();
        let set = |fx: &mut EffectContainer, name: &str, a: Atrb| {
            let at = fx.emitters[0].particle.attributes.iter().position(|x| x.hash == pandemic_hash_m2(name)).unwrap();
            fx.emitters[0].particle.attributes[at] = a;
        };
        // size → resampled (stream table, 100 words) whatever the flags.
        set(&mut fx, "size", Atrb::f32(pandemic_hash_m2("size"), 1.0).with_curve(keys(3)));
        // life → linear table, 2 × keys.
        set(&mut fx, "life", Atrb::f32(pandemic_hash_m2("life"), 1.0).with_curve(keys(4)));
        // speedvar → carried, not counted.
        set(&mut fx, "speedvar", Atrb::f32(pandemic_hash_m2("speedvar"), 1.0).with_curve(keys(2)));
        // A TRFM channel curve: linear; with RESAMPLE it reserves 100 words in the same table.
        fx.emitters[0].channels[0] = Atrb::f32(TRFM_CHANNELS[0].hash, 0.0).with_curve(keys(2));
        fx.emitters[0].channels[1] =
            Atrb::f32(TRFM_CHANNELS[1].hash, 0.0).with_options(atrb_flag::RESAMPLE).with_curve(keys(2));
        assert_eq!(fx.efct_words().unwrap(), [1, EFCT_MAGIC, 0, 3, 8 + 4 + 100, 0, 0, 3, 500]);
        let bytes = write_effect_container(&fx).unwrap();
        assert_eq!(parse_effect_container(&bytes).unwrap(), fx);
    }

    #[test]
    fn authored_flags_that_disagree_are_rejected() {
        let h = pandemic_hash_m2("life");
        // Curve bit without a curve.
        let a = Atrb { hash: h, flags: atrb_flag::FLOAT | atrb_flag::CURVE, value: AtrbValue::F32(1.0), curve: None };
        assert!(a.validate().unwrap_err().contains("disagree"));
        // Float bit on a u32 value.
        let a = Atrb { hash: h, flags: atrb_flag::FLOAT, value: AtrbValue::U32(1), curve: None };
        assert!(a.validate().unwrap_err().contains("disagree"));
        // A curve without the curve bit.
        let a = Atrb { hash: h, flags: atrb_flag::FLOAT, value: AtrbValue::F32(1.0), curve: Some(vec![AnimKey { time: 0.0, value: 0.0 }]) };
        assert!(a.validate().unwrap_err().contains("disagree"));
        // A bit no retail ATRB sets.
        assert!(Atrb::f32(h, 1.0).with_options(1 << 3).validate().unwrap_err().contains("not ones"));
        // An empty curve.
        assert!(Atrb::f32(h, 1.0).with_curve(vec![]).validate().unwrap_err().contains("at least one key"));

        // Through the writer: a disagreeing word anywhere refuses the whole effect.
        let mut fx = magenta_burst(1);
        fx.emitters[0].particle.attributes[5].flags |= atrb_flag::CURVE;
        assert!(write_effect_container(&fx).unwrap_err().contains("disagree"));
    }

    #[test]
    fn positions_value_kinds_and_refused_curves_are_enforced() {
        let mut fx = magenta_burst(1);
        fx.emitters[0].particle.attributes.swap(1, 2);
        assert!(write_effect_container(&fx).unwrap_err().contains("position 1"));

        let mut fx = magenta_burst(1);
        fx.emitters[0].particle.attributes[0] = Atrb::f32(pandemic_hash_m2("name"), 1.0);
        assert!(write_effect_container(&fx).unwrap_err().contains("takes a U32"));

        let mut fx = magenta_burst(1);
        let at = fx.emitters[0].particle.attributes.iter().position(|a| a.hash == pandemic_hash_m2("mass")).unwrap();
        fx.emitters[0].particle.attributes[at] =
            Atrb::f32(pandemic_hash_m2("mass"), 1.0).with_curve(vec![AnimKey { time: 0.0, value: 1.0 }]);
        assert!(write_effect_container(&fx).unwrap_err().contains("not decoded"));

        let mut fx = magenta_burst(1);
        fx.emitters[0].geom = Some(EmitterGeom { shape_index: 1, word_00: 0 });
        assert!(write_effect_container(&fx).unwrap_err().contains("shape index"));

        let mut fx = magenta_burst(1);
        fx.emitters[0].particle.flags = 4;
        assert!(write_effect_container(&fx).unwrap_err().contains("never reads"));

        let mut fx = magenta_burst(1);
        fx.emitters[0].particle.text.frames.clear();
        assert!(write_effect_container(&fx).unwrap_err().contains("no frames"));

        let mut fx = magenta_burst(1);
        fx.emitters[0].particle.flags = 3;
        fx.emitters[0].particle.text.frames = vec![7; 101];
        assert!(write_effect_container(&fx).unwrap_err().contains("stream words"));
    }

    #[test]
    fn an_emitter_needs_a_shape_table_it_samples_within() {
        let refused = |edit: &dyn Fn(&mut EffectContainer)| {
            let mut fx = magenta_burst(1);
            edit(&mut fx);
            assert!(fx.validate_nodes().is_ok());
            let e = fx.check_emitter_shapes().unwrap_err();
            assert_eq!(write_effect_container(&fx).unwrap_err(), e);
            e
        };
        let e = refused(&|fx| fx.emitters[0].geom = Some(EmitterGeom { shape_index: 1, word_00: 1 }));
        assert!(e.contains("shape index 1"), "{e}");
        let e = refused(&|fx| fx.emitters[0].geom = Some(EmitterGeom { shape_index: 0, word_00: 0 }));
        assert!(e.contains("samples 0 records") && e.contains("divides by zero"), "{e}");
        let e = refused(&|fx| fx.emitters[0].geom = Some(EmitterGeom { shape_index: 0, word_00: 2 }));
        assert!(e.contains("samples 2 records of shape 0, which has 1"), "{e}");
        let e = refused(&|fx| fx.shapes[0].records.clear());
        assert!(e.contains("has no records"), "{e}");
        let e = refused(&|fx| {
            fx.shapes[0].records = vec![[0.0; SHAPE_RECORD_FLOATS]; 0x8000];
            fx.emitters[0].geom = Some(EmitterGeom { shape_index: 0, word_00: 0x8000 });
        });
        assert!(e.contains("signed 16-bit"), "{e}");
        // `magenta_burst` has rate 100: without GEOM it spawns from a 0-record table.
        let e = refused(&|fx| fx.emitters[0].geom = None);
        assert!(e.contains("has no GEOM") && e.contains("divides by zero"), "{e}");

        let at = |fx: &EffectContainer, name: &str| {
            fx.emitters[0].particle.attributes.iter().position(|a| a.hash == pandemic_hash_m2(name)).unwrap()
        };
        let without_geom = |rate: Atrb, ratevar: f32| {
            let mut fx = magenta_burst(1);
            fx.emitters[0].geom = None;
            let (r, v) = (at(&fx, "rate"), at(&fx, "ratevar"));
            fx.emitters[0].particle.attributes[r] = rate;
            fx.emitters[0].particle.attributes[v] = Atrb::f32(pandemic_hash_m2("ratevar"), ratevar);
            fx
        };
        let rate = |v: f32| Atrb::f32(pandemic_hash_m2("rate"), v);
        // A constant rate at or below 0 with ratevar 0 spawns nothing: accepted, as retail's 9.
        for r in [0.0, -1.0] {
            let fx = without_geom(rate(r), 0.0);
            assert_eq!(parse_effect_container(&write_effect_container(&fx).unwrap()).unwrap(), fx);
        }
        assert!(without_geom(rate(0.0), 1.0).check_emitter_shapes().unwrap_err().contains("has no GEOM"));
        let curve = rate(0.0).with_curve(vec![AnimKey { time: 0.0, value: 0.0 }, AnimKey { time: 100.0, value: 5.0 }]);
        assert!(without_geom(curve, 0.0).check_emitter_shapes().unwrap_err().contains("has no GEOM"));
        // A count below the shape's record count samples the first records only.
        let mut fx = magenta_burst(1);
        fx.shapes[0].records.push([1.0; SHAPE_RECORD_FLOATS]);
        assert!(fx.check_emitter_shapes().is_ok());
    }

    #[test]
    fn a_tampered_efct_is_refused_on_parse() {
        let bytes = write_effect_container(&magenta_burst(1)).unwrap();
        let rows = read_ucfx_rows(&bytes).unwrap();
        let data = 20 + 20 * rows.len();
        let mut bad = bytes.clone();
        bad[data + 16] ^= 1; // EFCT word 8
        let at = bad.len() - 8;
        let sum = crate::crc32::crc32_mercs2(&bad[..at]);
        bad[at + 4..].copy_from_slice(&sum.to_le_bytes());
        assert!(parse_effect_container(&bad).unwrap_err().contains("stored EFCT"));
    }

    #[test]
    fn every_recovered_name_hashes_to_its_position() {
        for d in TRFM_CHANNELS
            .iter()
            .chain(PTYP_ATTRIBUTES_BEFORE_COLR.iter())
            .chain(PTYP_ATTRIBUTES_AFTER_COLR.iter())
            .chain(FRCE_COMMON_ATTRIBUTES.iter())
            .chain(FRCE_DRAG_ATTRIBUTES.iter())
            .chain(FRCE_ATTRACTOR_ATTRIBUTES.iter())
            .chain(FRCE_VORTEX_ATTRIBUTES.iter())
        {
            if let Some(n) = d.name {
                assert_eq!(pandemic_hash_m2(n), d.hash, "{n}");
            }
        }
        assert_eq!(attribute_name(0x9BE62E41), Some("speedvar"));
        assert_eq!(attribute_name(0x10831673), None);
    }
}
