//! FX cluster parsers — `fxdict` (DICT) + effect-template key chunks.
//!
//! Ground truth: `docs/ucfx_tag_registry.md` §7 (FX cluster) + `docs/fxdict_format.md`.
//! The engine's container loader is `FUN_00491320` (fxdict) and the effect loader is `0x492AF0`;
//! the per-chunk readers are the addresses noted on each function below. All layouts here are the
//! **PC little-endian on-disk** forms (the Xbox path byteswaps to this before consumption).
//!
//! What is verified vs hypothesised:
//!   * DICT record = 20 bytes `{u32 name_hash, f32, f32, f32, u32 flags}`, 630 records on retail,
//!     zero trailing slack — **verified** (registry §7, `chunk_validate::validate_fxdict_chunks`).
//!   * EMTR = `u16 count + count×4` module refs — **verified** (@0x492402, "reads a u16 count then
//!     count×4 alloc, overflow-guarded").
//!   * COLR = fixed 0xC8 (200-byte) age-sampled gradient record — **verified size** (@0x4930e5,
//!     "stores a fixed 0xC8 record into the effect palette heap"). The *interior* field order of
//!     those 200 bytes is not in the decomp; we model it as 50 RGBA8 stops (50×4 = 200) sampled by
//!     normalised age, which is the natural reading for "sampled by particle age" (**hypothesis**).
//!   * FRCE = `u32 inner_hash` + per-force params — **verified shape** (@0x491c93, "reads a 4-byte
//!     inner hash then sub-dispatches per force type"). The concrete gravity/drag/vortex hash values
//!     are not in the decomp; we classify best-effort (ASCII FourCC + computed pandemic hashes) and
//!     always retain the raw hash + raw params (**force-kind classification = hypothesis**).
//!   * PTYP = 1 flags byte (bit0→+0x205, bit1→+0x206) — **verified** (@0x491ba9).
//!   * POFF = vec3 emitter offset (0xC) — **verified** (@0x4a9cf2).
//!   * TRFM = 16×f32 4×4 row-major matrix — **verified** (FUN_0048cc30, unrolled 16-float read).
//!   * EFCT header: magic `0x0226` @ +2, sub-component count @ +14 — **verified** (loader 0x492AF0;
//!     `spatial_hash_crash_analysis.md`).
//!   * EMIT timing: delegates to FUN_0048cc30; body is float timing data — parsed generically.

use crate::hash::{pandemic_hash, pandemic_hash_m2};

fn read_u16_le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn read_u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn read_f32_le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

// ------------------------------------------------------------------------------------------------
// fxdict (DICT) — the resident 630-record effect-parameter namespace.
// ------------------------------------------------------------------------------------------------

/// On-disk DICT record stride (verified: 630 × 20 = 12600 bytes, zero slack).
pub const DICT_RECORD_BYTES: usize = 20;
/// Retail fxdict record count (`resident_P000_Q3`, 2026-05-30 probe).
pub const DICT_RETAIL_COUNT: usize = 630;

/// One fxdict parameter record (20 bytes on disk).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxParam {
    /// Parameter key hash; shared namespace with effect `TEXT` chunks.
    pub name_hash: u32,
    /// Default scalar value (+0x04).
    pub default: f32,
    /// Second scalar (+0x08) — hypothesised **max** bound.
    pub value_b: f32,
    /// Third scalar (+0x0C) — hypothesised **min** bound (often 1/32).
    pub value_c: f32,
    /// Flags dword (+0x10) — semantics unknown.
    pub flags: u32,
}

/// Parse the fxdict from its container `INFO` (`u32 entry_count`) and `DICT` body
/// (`entry_count × 20` bytes). Returns one [`FxParam`] per record.
///
/// Faithful to the loader: the count comes from INFO, records are read at a fixed 20-byte stride.
/// Trailing bytes past `count × 20` are ignored (the engine only walks `count`).
pub fn parse_fxdict(info: &[u8], dict: &[u8]) -> Result<Vec<FxParam>, String> {
    if info.len() < 4 {
        return Err(format!(
            "fxdict INFO too short: {} bytes (need 4)",
            info.len()
        ));
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
        out.push(FxParam {
            name_hash: read_u32_le(dict, o),
            default: read_f32_le(dict, o + 4),
            value_b: read_f32_le(dict, o + 8),
            value_c: read_f32_le(dict, o + 12),
            flags: read_u32_le(dict, o + 16),
        });
    }
    Ok(out)
}

/// Look up a parameter's default by name hash (linear scan; the engine indexes a hash map but the
/// table is small enough that callers wanting a one-off lookup can use this).
pub fn fxparam_default(params: &[FxParam], name_hash: u32) -> Option<f32> {
    params
        .iter()
        .find(|p| p.name_hash == name_hash)
        .map(|p| p.default)
}

// ------------------------------------------------------------------------------------------------
// Effect template chunks.
// ------------------------------------------------------------------------------------------------

/// `EFCT` header (magic `0x0226` @ byte +2, sub-component count @ byte +14). The engine reads these
/// as u32 words that pack two u16 halves; the count gates the descriptor-array alloc at 0x492AF0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectHeader {
    pub magic: u16,
    pub sub_count: u16,
}

/// Parse the `EFCT` header. Returns `None` if the body is shorter than the count field (+16).
pub fn parse_efct(body: &[u8]) -> Option<EffectHeader> {
    if body.len() < 16 {
        return None;
    }
    Some(EffectHeader {
        magic: read_u16_le(body, 2),
        sub_count: read_u16_le(body, 14),
    })
}

/// `EMTR` — emitter module table: `u16 count` then `count × u32` module refs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EmitterTable {
    pub refs: Vec<u32>,
}

/// Parse `EMTR`. The count is read as u16; each ref is a u32. Refs that would run past the body are
/// truncated (the engine's alloc is overflow-guarded — a short/corrupt table yields fewer refs, not
/// a crash).
pub fn parse_emtr(body: &[u8]) -> EmitterTable {
    if body.len() < 2 {
        return EmitterTable::default();
    }
    let count = read_u16_le(body, 0) as usize;
    let avail = (body.len() - 2) / 4;
    let n = count.min(avail);
    let mut refs = Vec::with_capacity(n);
    for i in 0..n {
        refs.push(read_u32_le(body, 2 + i * 4));
    }
    EmitterTable { refs }
}

/// `EMIT` — emitter timing. The reader delegates to FUN_0048cc30 (the same float-block reader as
/// TRFM); the body is a run of f32 timing values. We expose them raw (their exact roles — spawn
/// delay / burst count / rate — are not pinned in the decomp).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EmitTiming {
    pub floats: Vec<f32>,
}

/// Parse `EMIT` timing floats (as many whole f32 as the body holds).
pub fn parse_emit(body: &[u8]) -> EmitTiming {
    let n = body.len() / 4;
    let mut floats = Vec::with_capacity(n);
    for i in 0..n {
        floats.push(read_f32_le(body, i * 4));
    }
    EmitTiming { floats }
}

/// `POFF` — emitter local offset (vec3). Returns `None` if the body is < 12 bytes.
pub fn parse_poff(body: &[u8]) -> Option<[f32; 3]> {
    if body.len() < 12 {
        return None;
    }
    Some([
        read_f32_le(body, 0),
        read_f32_le(body, 4),
        read_f32_le(body, 8),
    ])
}

/// `TRFM` — 4×4 transform, 16 f32 row-major (D3D convention). Returns `None` if the body is < 64 B.
pub fn parse_trfm(body: &[u8]) -> Option<[[f32; 4]; 4]> {
    if body.len() < 64 {
        return None;
    }
    let mut m = [[0.0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            m[r][c] = read_f32_le(body, (r * 4 + c) * 4);
        }
    }
    Some(m)
}

/// `PTYP` — particle-type flags byte. `bit0` and `bit1` map to engine fields +0x205 / +0x206.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ParticleType {
    pub flags: u8,
}
impl ParticleType {
    /// bit0 → engine +0x205.
    pub fn bit0(&self) -> bool {
        self.flags & 0x01 != 0
    }
    /// bit1 → engine +0x206.
    pub fn bit1(&self) -> bool {
        self.flags & 0x02 != 0
    }
}

/// Parse `PTYP` (single flags byte). Returns `None` on an empty body.
pub fn parse_ptyp(body: &[u8]) -> Option<ParticleType> {
    body.first().map(|&b| ParticleType { flags: b })
}

// --- COLR: fixed 200-byte age gradient -----------------------------------------------------------

/// Byte length of a `COLR` record (fixed `0xC8`).
pub const COLR_BYTES: usize = 0xC8; // 200
/// Number of RGBA8 stops we model the 200-byte record as (50 × 4 = 200).
pub const COLR_STOPS: usize = COLR_BYTES / 4;

/// `COLR` — a fixed 200-byte colour-over-life gradient sampled by particle age.
///
/// The 200 bytes are modelled as [`COLR_STOPS`] RGBA8 stops evenly distributed across normalised
/// age 0..1 (**hypothesis** — see module docs; the decomp pins the *size*, not the field order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorGradient {
    pub stops: [[u8; 4]; COLR_STOPS],
}

impl Default for ColorGradient {
    fn default() -> Self {
        ColorGradient {
            stops: [[255, 255, 255, 255]; COLR_STOPS],
        }
    }
}

impl ColorGradient {
    /// Sample the gradient at normalised age `t` (0 = spawn, 1 = death), linearly interpolating
    /// between the two nearest stops. Returns straight (non-premultiplied) RGBA in 0..1.
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let t = t.clamp(0.0, 1.0);
        let scaled = t * (COLR_STOPS - 1) as f32;
        let i0 = scaled.floor() as usize;
        let i1 = (i0 + 1).min(COLR_STOPS - 1);
        let f = scaled - i0 as f32;
        let a = self.stops[i0];
        let b = self.stops[i1];
        let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f) / 255.0;
        [
            lerp(a[0], b[0]),
            lerp(a[1], b[1]),
            lerp(a[2], b[2]),
            lerp(a[3], b[3]),
        ]
    }
}

/// Parse `COLR`. Requires a full 200-byte body (the engine copies exactly 0xC8 bytes).
pub fn parse_colr(body: &[u8]) -> Option<ColorGradient> {
    if body.len() < COLR_BYTES {
        return None;
    }
    let mut stops = [[0u8; 4]; COLR_STOPS];
    for (i, s) in stops.iter_mut().enumerate() {
        let o = i * 4;
        *s = [body[o], body[o + 1], body[o + 2], body[o + 3]];
    }
    Some(ColorGradient { stops })
}

// --- FRCE: force taxonomy ------------------------------------------------------------------------

/// Best-effort classification of a `FRCE` inner hash. The raw hash is always retained; `kind` is a
/// hypothesis (the concrete gravity/drag/vortex hash constants are not in the decomp).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForceKind {
    Gravity,
    Drag,
    Vortex,
    Wind,
    Unknown,
}

/// One `FRCE` record: a 4-byte inner hash + up to 4 f32 parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Force {
    pub inner_hash: u32,
    pub kind: ForceKind,
    /// Raw parameter floats (as many whole f32 as followed the hash, up to 4).
    pub params: [f32; 4],
    pub param_count: usize,
}

/// Classify a `FRCE` inner hash. Accepts either an ASCII FourCC (e.g. `GRAV`/`DRAG`/`VORT`/`WIND`)
/// or a pandemic name hash (both `pandemic_hash` and `pandemic_hash_m2` of the type name). Any hash
/// that matches neither is [`ForceKind::Unknown`] (the raw value is preserved by the caller).
pub fn classify_force(inner_hash: u32) -> ForceKind {
    // ASCII FourCC form (little-endian tag bytes).
    let tag = inner_hash.to_le_bytes();
    match &tag {
        b"GRAV" | b"grav" => return ForceKind::Gravity,
        b"DRAG" | b"drag" => return ForceKind::Drag,
        b"VORT" | b"vort" => return ForceKind::Vortex,
        b"WIND" | b"wind" => return ForceKind::Wind,
        _ => {}
    }
    // Pandemic name-hash form.
    for (name, kind) in [
        ("gravity", ForceKind::Gravity),
        ("drag", ForceKind::Drag),
        ("vortex", ForceKind::Vortex),
        ("wind", ForceKind::Wind),
    ] {
        if inner_hash == pandemic_hash(name) || inner_hash == pandemic_hash_m2(name) {
            return kind;
        }
    }
    ForceKind::Unknown
}

/// Parse a `FRCE` record: `u32 inner_hash` then up to 4 f32 params. Returns `None` if the body is
/// shorter than the 4-byte hash. Unknown force types are **kept** (classified `Unknown`), not
/// rejected — the registry mandates skip-by-size, never abort.
pub fn parse_frce(body: &[u8]) -> Option<Force> {
    if body.len() < 4 {
        return None;
    }
    let inner_hash = read_u32_le(body, 0);
    let mut params = [0.0f32; 4];
    let avail = (body.len() - 4) / 4;
    let n = avail.min(4);
    for (i, p) in params.iter_mut().enumerate().take(n) {
        *p = read_f32_le(body, 4 + i * 4);
    }
    Some(Force {
        inner_hash,
        kind: classify_force(inner_hash),
        params,
        param_count: n,
    })
}

// ------------------------------------------------------------------------------------------------
// Aggregate effect template.
// ------------------------------------------------------------------------------------------------

/// A parsed effect template — the union of the key chunks the runtime consumes. Assembled by
/// feeding `(fourcc, body)` chunk pairs (from the effect's UCFX container) to
/// [`EffectTemplate::from_chunks`]. Absent chunks stay `None`/empty.
#[derive(Debug, Clone, Default)]
pub struct EffectTemplate {
    pub header: Option<EffectHeader>,
    pub emitters: EmitterTable,
    pub emit: EmitTiming,
    pub gradient: Option<ColorGradient>,
    pub forces: Vec<Force>,
    pub ptype: Option<ParticleType>,
    pub offset: Option<[f32; 3]>,
    pub transform: Option<[[f32; 4]; 4]>,
    pub text_refs: Vec<u32>,
}

impl EffectTemplate {
    /// Assemble a template from an ordered list of `(fourcc, body)` chunk pairs (as produced by the
    /// UCFX container walker). Unknown tags are ignored; repeated `FRCE` chunks accumulate.
    pub fn from_chunks<'a, I>(chunks: I) -> EffectTemplate
    where
        I: IntoIterator<Item = (&'a [u8; 4], &'a [u8])>,
    {
        let mut t = EffectTemplate::default();
        for (tag, body) in chunks {
            match tag {
                b"EFCT" => t.header = parse_efct(body),
                b"EMTR" => t.emitters = parse_emtr(body),
                b"EMIT" => t.emit = parse_emit(body),
                b"COLR" => t.gradient = parse_colr(body),
                b"FRCE" => {
                    if let Some(f) = parse_frce(body) {
                        t.forces.push(f);
                    }
                }
                b"PTYP" => t.ptype = parse_ptyp(body),
                b"POFF" => t.offset = parse_poff(body),
                b"TRFM" => t.transform = parse_trfm(body),
                b"TEXT" => t.text_refs = parse_text(body),
                _ => {}
            }
        }
        t
    }
}

/// `TEXT` — leading `u32` (count/length) then a list of `u32` asset/param hashes. We read the
/// leading word as a count but clamp it to the available body so a byte-length-vs-count ambiguity
/// can't over-read.
pub fn parse_text(body: &[u8]) -> Vec<u32> {
    if body.len() < 4 {
        return Vec::new();
    }
    let stated = read_u32_le(body, 0) as usize;
    let avail = (body.len() - 4) / 4;
    let n = stated.min(avail);
    (0..n).map(|i| read_u32_le(body, 4 + i * 4)).collect()
}

// ------------------------------------------------------------------------------------------------
// Encoders.
// ------------------------------------------------------------------------------------------------

pub fn write_fxparam(p: &FxParam) -> [u8; DICT_RECORD_BYTES] {
    let mut out = [0u8; DICT_RECORD_BYTES];
    out[0..4].copy_from_slice(&p.name_hash.to_le_bytes());
    out[4..8].copy_from_slice(&p.default.to_le_bytes());
    out[8..12].copy_from_slice(&p.value_b.to_le_bytes());
    out[12..16].copy_from_slice(&p.value_c.to_le_bytes());
    out[16..20].copy_from_slice(&p.flags.to_le_bytes());
    out
}

pub fn write_fxdict_dict(params: &[FxParam]) -> Vec<u8> {
    let mut out = Vec::with_capacity(params.len() * DICT_RECORD_BYTES);
    for p in params {
        out.extend_from_slice(&write_fxparam(p));
    }
    out
}

pub fn write_fxdict_info(count: u32) -> [u8; 4] {
    count.to_le_bytes()
}

pub fn write_efct(h: &EffectHeader) -> Vec<u8> {
    let mut body = vec![0u8; 16];
    body[2..4].copy_from_slice(&h.magic.to_le_bytes());
    body[14..16].copy_from_slice(&h.sub_count.to_le_bytes());
    body
}

pub fn write_emtr(t: &EmitterTable) -> Vec<u8> {
    let mut body = Vec::with_capacity(2 + t.refs.len() * 4);
    body.extend_from_slice(&(t.refs.len() as u16).to_le_bytes());
    for r in &t.refs {
        body.extend_from_slice(&r.to_le_bytes());
    }
    body
}

pub fn write_emit(t: &EmitTiming) -> Vec<u8> {
    let mut body = Vec::with_capacity(t.floats.len() * 4);
    for f in &t.floats {
        body.extend_from_slice(&f.to_le_bytes());
    }
    body
}

pub fn write_poff(v: [f32; 3]) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    for x in v {
        body.extend_from_slice(&x.to_le_bytes());
    }
    body
}

pub fn write_trfm(m: &[[f32; 4]; 4]) -> Vec<u8> {
    let mut body = Vec::with_capacity(64);
    for row in m {
        for x in row {
            body.extend_from_slice(&x.to_le_bytes());
        }
    }
    body
}

pub fn write_ptyp(p: ParticleType) -> Vec<u8> {
    vec![p.flags]
}

pub fn write_colr(g: &ColorGradient) -> Vec<u8> {
    let mut body = Vec::with_capacity(COLR_BYTES);
    for s in &g.stops {
        body.extend_from_slice(s);
    }
    body
}

pub fn write_frce(f: &Force) -> Vec<u8> {
    let n = f.param_count.min(4);
    let mut body = Vec::with_capacity(4 + n * 4);
    body.extend_from_slice(&f.inner_hash.to_le_bytes());
    for x in &f.params[..n] {
        body.extend_from_slice(&x.to_le_bytes());
    }
    body
}

pub fn write_text(hashes: &[u32]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + hashes.len() * 4);
    body.extend_from_slice(&(hashes.len() as u32).to_le_bytes());
    for h in hashes {
        body.extend_from_slice(&h.to_le_bytes());
    }
    body
}

impl EffectTemplate {
    pub fn to_chunks(&self) -> Vec<([u8; 4], Vec<u8>)> {
        let mut out: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        if let Some(h) = self.header { out.push((*b"EFCT", write_efct(&h))); }
        if !self.emitters.refs.is_empty() { out.push((*b"EMTR", write_emtr(&self.emitters))); }
        if !self.emit.floats.is_empty() { out.push((*b"EMIT", write_emit(&self.emit))); }
        if let Some(v) = self.offset { out.push((*b"POFF", write_poff(v))); }
        if let Some(m) = &self.transform { out.push((*b"TRFM", write_trfm(m))); }
        if let Some(p) = self.ptype { out.push((*b"PTYP", write_ptyp(p))); }
        if let Some(g) = &self.gradient { out.push((*b"COLR", write_colr(g))); }
        for f in &self.forces { out.push((*b"FRCE", write_frce(f))); }
        if !self.text_refs.is_empty() { out.push((*b"TEXT", write_text(&self.text_refs))); }
        out
    }
}

pub fn write_ucfx_container(chunks: &[([u8; 4], u32, u32, Vec<u8>)]) -> Vec<u8> {
    let n = chunks.len();
    let hdr_bytes = 20;
    let desc_bytes = n * 20;
    let data_off = (hdr_bytes + desc_bytes) as u32;

    let mut data = Vec::new();
    let mut placed: Vec<(u32, u32)> = Vec::with_capacity(n);
    for (_, _, _, body) in chunks {
        while data.len() % 4 != 0 { data.push(0); }
        placed.push((data.len() as u32, body.len() as u32));
        data.extend_from_slice(body);
    }

    let mut c = Vec::with_capacity(20 + desc_bytes + data.len() + 8);
    c.extend_from_slice(b"UCFX");
    for v in [data_off, 0, 0, n as u32] {
        c.extend_from_slice(&v.to_le_bytes());
    }
    for (i, (tag, u2, u3, _)) in chunks.iter().enumerate() {
        let (rel_off, size) = placed[i];
        c.extend_from_slice(tag);
        c.extend_from_slice(&rel_off.to_le_bytes());
        c.extend_from_slice(&size.to_le_bytes());
        c.extend_from_slice(&u2.to_le_bytes());
        c.extend_from_slice(&u3.to_le_bytes());
    }
    c.extend_from_slice(&data);
    let sum = crate::crc32::crc32_mercs2(&c);
    c.extend_from_slice(b"CSUM");
    c.extend_from_slice(&sum.to_le_bytes());
    c
}

pub fn write_effect_container(t: &EffectTemplate) -> Vec<u8> {
    let owned = t.to_chunks();
    let chunks: Vec<([u8; 4], u32, u32, Vec<u8>)> =
        owned.into_iter().map(|(tag, body)| (tag, 0u32, 0u32, body)).collect();
    write_ucfx_container(&chunks)
}

pub fn write_fxdict_container(params: &[FxParam]) -> Vec<u8> {
    let info = write_fxdict_info(params.len() as u32).to_vec();
    let dict = write_fxdict_dict(params);
    write_ucfx_container(&[
        (*b"INFO", 0, 0, info),
        (*b"DICT", 0, 0, dict),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    #[test]
    fn fxdict_single_record() {
        let info = 1u32.to_le_bytes();
        let mut dict = Vec::new();
        dict.extend_from_slice(&le(0xAABBCCDD)); // name_hash
        dict.extend_from_slice(&1.5f32.to_le_bytes()); // default
        dict.extend_from_slice(&0.5f32.to_le_bytes()); // value_b
        dict.extend_from_slice(&0.03125f32.to_le_bytes()); // value_c (1/32)
        dict.extend_from_slice(&le(0x3CF40017)); // flags
        let params = parse_fxdict(&info, &dict).unwrap();
        assert_eq!(params.len(), 1);
        let p = params[0];
        assert_eq!(p.name_hash, 0xAABBCCDD);
        assert_eq!(p.default, 1.5);
        assert_eq!(p.value_b, 0.5);
        assert_eq!(p.value_c, 0.03125);
        assert_eq!(p.flags, 0x3CF40017);
        assert_eq!(fxparam_default(&params, 0xAABBCCDD), Some(1.5));
        assert_eq!(fxparam_default(&params, 0xDEAD), None);
    }

    #[test]
    fn fxdict_retail_shape() {
        // 630 zeroed records = 12600 bytes, exactly as the retail resident block.
        let info = (DICT_RETAIL_COUNT as u32).to_le_bytes();
        let dict = vec![0u8; DICT_RETAIL_COUNT * DICT_RECORD_BYTES];
        let params = parse_fxdict(&info, &dict).unwrap();
        assert_eq!(params.len(), 630);
        assert_eq!(dict.len(), 12600);
    }

    #[test]
    fn fxdict_ignores_trailing_slack() {
        let info = 2u32.to_le_bytes();
        let dict = vec![0u8; 2 * DICT_RECORD_BYTES + 7]; // trailing slack
        assert_eq!(parse_fxdict(&info, &dict).unwrap().len(), 2);
    }

    #[test]
    fn fxdict_rejects_short_inputs() {
        assert!(parse_fxdict(&[0, 0], &[]).is_err());
        let info = 3u32.to_le_bytes();
        assert!(parse_fxdict(&info, &[0u8; 40]).is_err()); // needs 60
    }

    #[test]
    fn emtr_reads_count_then_refs() {
        let mut body = Vec::new();
        body.extend_from_slice(&2u16.to_le_bytes());
        body.extend_from_slice(&le(0x11111111));
        body.extend_from_slice(&le(0x22222222));
        let t = parse_emtr(&body);
        assert_eq!(t.refs, vec![0x11111111, 0x22222222]);
    }

    #[test]
    fn emtr_truncates_overflowing_count() {
        // count says 9 but only room for 1 ref — engine's alloc is overflow-guarded.
        let mut body = Vec::new();
        body.extend_from_slice(&9u16.to_le_bytes());
        body.extend_from_slice(&le(0xCAFEBABE));
        let t = parse_emtr(&body);
        assert_eq!(t.refs, vec![0xCAFEBABE]);
    }

    #[test]
    fn efct_header_offsets() {
        let mut body = vec![0u8; 18];
        body[2..4].copy_from_slice(&0x0226u16.to_le_bytes()); // magic @ +2
        body[14..16].copy_from_slice(&3u16.to_le_bytes()); // sub-count @ +14
        let h = parse_efct(&body).unwrap();
        assert_eq!(h.magic, 0x0226);
        assert_eq!(h.sub_count, 3);
        assert!(parse_efct(&[0u8; 8]).is_none());
    }

    #[test]
    fn poff_and_trfm() {
        let mut poff = Vec::new();
        for v in [1.0f32, 2.0, 3.0] {
            poff.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(parse_poff(&poff), Some([1.0, 2.0, 3.0]));
        assert_eq!(parse_poff(&poff[..8]), None);

        // identity matrix
        let mut trfm = vec![0u8; 64];
        for i in 0..4 {
            trfm[(i * 4 + i) * 4..(i * 4 + i) * 4 + 4].copy_from_slice(&1.0f32.to_le_bytes());
        }
        let m = parse_trfm(&trfm).unwrap();
        assert_eq!(m[0][0], 1.0);
        assert_eq!(m[3][3], 1.0);
        assert_eq!(m[0][1], 0.0);
        assert!(parse_trfm(&trfm[..32]).is_none());
    }

    #[test]
    fn ptyp_flag_bits() {
        assert!(parse_ptyp(&[]).is_none());
        let p = parse_ptyp(&[0x03]).unwrap();
        assert!(p.bit0());
        assert!(p.bit1());
        let p = parse_ptyp(&[0x02]).unwrap();
        assert!(!p.bit0());
        assert!(p.bit1());
    }

    #[test]
    fn colr_gradient_size_and_sampling() {
        assert_eq!(COLR_BYTES, 200);
        assert_eq!(COLR_STOPS, 50);
        assert!(parse_colr(&[0u8; 100]).is_none());
        let mut body = vec![0u8; COLR_BYTES];
        // stop 0 = opaque red, last stop = transparent black.
        body[0..4].copy_from_slice(&[255, 0, 0, 255]);
        let last = (COLR_STOPS - 1) * 4;
        body[last..last + 4].copy_from_slice(&[0, 0, 0, 0]);
        let g = parse_colr(&body).unwrap();
        let s0 = g.sample(0.0);
        assert!((s0[0] - 1.0).abs() < 1e-6 && (s0[3] - 1.0).abs() < 1e-6);
        let s1 = g.sample(1.0);
        assert!(s1[3].abs() < 1e-6); // alpha fades to 0 at death
    }

    #[test]
    fn frce_parse_and_classify() {
        // FourCC "GRAV" + one gravity magnitude.
        let mut body = Vec::new();
        body.extend_from_slice(b"GRAV");
        body.extend_from_slice(&(-9.8f32).to_le_bytes());
        let f = parse_frce(&body).unwrap();
        assert_eq!(f.kind, ForceKind::Gravity);
        assert_eq!(f.param_count, 1);
        assert_eq!(f.params[0], -9.8);

        // pandemic hash of "drag" classifies as Drag.
        let mut body = Vec::new();
        body.extend_from_slice(&pandemic_hash_m2("drag").to_le_bytes());
        body.extend_from_slice(&0.25f32.to_le_bytes());
        assert_eq!(parse_frce(&body).unwrap().kind, ForceKind::Drag);

        // Unknown hash is retained, not rejected.
        let mut body = Vec::new();
        body.extend_from_slice(&le(0xDEADBEEF));
        let f = parse_frce(&body).unwrap();
        assert_eq!(f.kind, ForceKind::Unknown);
        assert_eq!(f.inner_hash, 0xDEADBEEF);
        assert_eq!(f.param_count, 0);

        assert!(parse_frce(&[0, 0]).is_none());
    }

    #[test]
    fn text_refs_clamp_to_body() {
        let mut body = Vec::new();
        body.extend_from_slice(&2u32.to_le_bytes());
        body.extend_from_slice(&le(0x8410A32A));
        body.extend_from_slice(&le(0x00000001));
        assert_eq!(parse_text(&body), vec![0x8410A32A, 0x00000001]);
        // stated count larger than body -> clamp.
        let mut body = Vec::new();
        body.extend_from_slice(&999u32.to_le_bytes());
        body.extend_from_slice(&le(0xAA));
        assert_eq!(parse_text(&body), vec![0xAA]);
    }

    #[test]
    fn effect_template_from_chunks() {
        let mut emtr = Vec::new();
        emtr.extend_from_slice(&1u16.to_le_bytes());
        emtr.extend_from_slice(&le(0x1234));
        let poff = {
            let mut v = Vec::new();
            for x in [0.5f32, 1.0, 1.5] {
                v.extend_from_slice(&x.to_le_bytes());
            }
            v
        };
        let ptyp = [0x01u8];
        let mut frce = Vec::new();
        frce.extend_from_slice(b"DRAG");
        frce.extend_from_slice(&0.1f32.to_le_bytes());
        let chunks: Vec<(&[u8; 4], &[u8])> = vec![
            (b"EMTR", emtr.as_slice()),
            (b"POFF", poff.as_slice()),
            (b"PTYP", ptyp.as_slice()),
            (b"FRCE", frce.as_slice()),
        ];
        let t = EffectTemplate::from_chunks(chunks);
        assert_eq!(t.emitters.refs, vec![0x1234]);
        assert_eq!(t.offset, Some([0.5, 1.0, 1.5]));
        assert_eq!(t.ptype.unwrap().flags, 0x01);
        assert_eq!(t.forces.len(), 1);
        assert_eq!(t.forces[0].kind, ForceKind::Drag);
    }

    #[test]
    fn fxparam_write_roundtrip() {
        let p = FxParam {
            name_hash: 0xDEADBEEF,
            default: 1.25,
            value_b: 3.5,
            value_c: 0.03125,
            flags: 0x12345678,
        };
        let bytes = write_fxparam(&p);
        assert_eq!(bytes.len(), DICT_RECORD_BYTES);
        let info = 1u32.to_le_bytes();
        let dict = bytes.to_vec();
        let back = parse_fxdict(&info, &dict).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0], p);
    }

    #[test]
    fn fxdict_bodies_roundtrip_many() {
        let params: Vec<FxParam> = (0..17)
            .map(|i| FxParam {
                name_hash: pandemic_hash_m2(&format!("param{i}")),
                default: i as f32 * 0.5,
                value_b: 1.0 + i as f32,
                value_c: (i as f32).sqrt(),
                flags: 0xC0DE_0000 | i,
            })
            .collect();
        let info = write_fxdict_info(params.len() as u32);
        let dict = write_fxdict_dict(&params);
        assert_eq!(info.len(), 4);
        assert_eq!(dict.len(), params.len() * DICT_RECORD_BYTES);
        let back = parse_fxdict(&info, &dict).unwrap();
        assert_eq!(back, params);
    }

    #[test]
    fn efct_write_roundtrip() {
        let h = EffectHeader { magic: 0x0226, sub_count: 7 };
        let body = write_efct(&h);
        assert_eq!(body.len(), 16);
        assert_eq!(parse_efct(&body), Some(h));
        assert_eq!(body[0..2], [0, 0]);
        assert_eq!(body[4..14], [0u8; 10]);
    }

    #[test]
    fn emtr_write_roundtrip() {
        let t = EmitterTable { refs: vec![0xAAAA, 0xBBBB, 0xCCCC] };
        let body = write_emtr(&t);
        assert_eq!(&body[0..2], &(3u16).to_le_bytes());
        assert_eq!(body.len(), 2 + 3 * 4);
        assert_eq!(parse_emtr(&body), t);

        let empty = EmitterTable::default();
        let body = write_emtr(&empty);
        assert_eq!(body.len(), 2);
        assert_eq!(parse_emtr(&body), empty);
    }

    #[test]
    fn emit_write_roundtrip() {
        let t = EmitTiming { floats: vec![0.0, 1.5, -3.25, 100.0] };
        let body = write_emit(&t);
        assert_eq!(body.len(), 16);
        assert_eq!(parse_emit(&body), t);
    }

    #[test]
    fn poff_write_roundtrip() {
        let v = [1.5f32, -2.0, 0.25];
        let body = write_poff(v);
        assert_eq!(body.len(), 12);
        assert_eq!(parse_poff(&body), Some(v));
    }

    #[test]
    fn trfm_write_roundtrip() {
        let m = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ];
        let body = write_trfm(&m);
        assert_eq!(body.len(), 64);
        assert_eq!(parse_trfm(&body), Some(m));
    }

    #[test]
    fn ptyp_write_roundtrip() {
        for flags in [0x00u8, 0x01, 0x02, 0x03, 0xFF] {
            let body = write_ptyp(ParticleType { flags });
            assert_eq!(body, vec![flags]);
            assert_eq!(parse_ptyp(&body).unwrap().flags, flags);
        }
    }

    #[test]
    fn colr_write_roundtrip() {
        let mut g = ColorGradient::default();
        for (i, s) in g.stops.iter_mut().enumerate() {
            *s = [i as u8, (i * 2) as u8, (i * 3) as u8, (i * 5) as u8];
        }
        let body = write_colr(&g);
        assert_eq!(body.len(), COLR_BYTES);
        assert_eq!(parse_colr(&body), Some(g));
    }

    #[test]
    fn frce_write_roundtrip() {
        let f = Force {
            inner_hash: u32::from_le_bytes(*b"DRAG"),
            kind: ForceKind::Drag,
            params: [0.25, 0.5, 0.75, 0.0],
            param_count: 3,
        };
        let body = write_frce(&f);
        assert_eq!(body.len(), 4 + 3 * 4);
        let back = parse_frce(&body).unwrap();
        assert_eq!(back.inner_hash, f.inner_hash);
        assert_eq!(back.kind, ForceKind::Drag);
        assert_eq!(back.param_count, 3);
        assert_eq!(&back.params[..3], &[0.25, 0.5, 0.75]);

        let f = Force {
            inner_hash: 0xDEADBEEF,
            kind: ForceKind::Unknown,
            params: [0.0; 4],
            param_count: 0,
        };
        let body = write_frce(&f);
        assert_eq!(body.len(), 4);
        assert_eq!(parse_frce(&body).unwrap().param_count, 0);

        let f = Force {
            inner_hash: u32::from_le_bytes(*b"GRAV"),
            kind: ForceKind::Gravity,
            params: [1.0, 2.0, 3.0, 4.0],
            param_count: 4,
        };
        let body = write_frce(&f);
        assert_eq!(body.len(), 4 + 4 * 4);
        let back = parse_frce(&body).unwrap();
        assert_eq!(back.param_count, 4);
        assert_eq!(back.params, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn text_write_roundtrip() {
        let hashes = vec![0x1111_2222u32, 0x3333_4444, 0x5555_6666];
        let body = write_text(&hashes);
        assert_eq!(body.len(), 4 + hashes.len() * 4);
        assert_eq!(parse_text(&body), hashes);

        let body = write_text(&[]);
        assert_eq!(body.len(), 4);
        assert_eq!(parse_text(&body), Vec::<u32>::new());
    }

    #[test]
    fn effect_template_to_from_chunks_roundtrip() {
        let mut gradient = ColorGradient::default();
        for (i, s) in gradient.stops.iter_mut().enumerate() {
            *s = [255, i as u8, (255 - i) as u8, 128];
        }
        let template = EffectTemplate {
            header: Some(EffectHeader { magic: 0x0226, sub_count: 2 }),
            emitters: EmitterTable { refs: vec![0xAAAA, 0xBBBB] },
            emit: EmitTiming { floats: vec![0.1, 0.2, 0.3] },
            gradient: Some(gradient),
            forces: vec![
                Force {
                    inner_hash: u32::from_le_bytes(*b"GRAV"),
                    kind: ForceKind::Gravity,
                    params: [0.0, -9.8, 0.0, 0.0],
                    param_count: 3,
                },
                Force {
                    inner_hash: u32::from_le_bytes(*b"DRAG"),
                    kind: ForceKind::Drag,
                    params: [0.15, 0.0, 0.0, 0.0],
                    param_count: 1,
                },
            ],
            ptype: Some(ParticleType { flags: 0x03 }),
            offset: Some([0.5, 1.0, 1.5]),
            transform: Some([
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [10.0, 20.0, 30.0, 1.0],
            ]),
            text_refs: vec![0xF00D_BEEF, 0xCAFE_D00D],
        };

        let emitted = template.to_chunks();
        // Round-trip through the from_chunks side.
        let borrowed: Vec<(&[u8; 4], &[u8])> =
            emitted.iter().map(|(tag, body)| (tag, body.as_slice())).collect();
        let back = EffectTemplate::from_chunks(borrowed);

        assert_eq!(back.header, template.header);
        assert_eq!(back.emitters, template.emitters);
        assert_eq!(back.emit, template.emit);
        assert_eq!(back.gradient, template.gradient);
        assert_eq!(back.ptype, template.ptype);
        assert_eq!(back.offset, template.offset);
        assert_eq!(back.transform, template.transform);
        assert_eq!(back.text_refs, template.text_refs);
        assert_eq!(back.forces.len(), template.forces.len());
        for (a, b) in back.forces.iter().zip(template.forces.iter()) {
            assert_eq!(a.inner_hash, b.inner_hash);
            assert_eq!(a.param_count, b.param_count);
            assert_eq!(a.params[..a.param_count], b.params[..b.param_count]);
        }
    }

    #[test]
    fn write_ucfx_container_shape_and_csum() {
        let a = vec![0x11u8, 0x22, 0x33];
        let b = vec![0x44u8, 0x55, 0x66, 0x77, 0x88, 0x99];
        let chunks = vec![
            (*b"AAAA", 0u32, 0u32, a.clone()),
            (*b"BBBB", 0u32, 0u32, b.clone()),
        ];
        let container = write_ucfx_container(&chunks);

        assert_eq!(&container[0..4], b"UCFX");
        let data_off = u32::from_le_bytes(container[4..8].try_into().unwrap());
        let n_desc = u32::from_le_bytes(container[16..20].try_into().unwrap());
        assert_eq!(n_desc, 2);
        assert_eq!(data_off as usize, 20 + 2 * 20);

        let tail = &container[container.len() - 8..];
        assert_eq!(&tail[0..4], b"CSUM");
        let stored = u32::from_le_bytes(tail[4..8].try_into().unwrap());
        let recomputed = crate::crc32::crc32_mercs2(&container[..container.len() - 8]);
        assert_eq!(stored, recomputed);

        let issues = crate::ucfx::verify_ucfx_container(&container, "test", 0);
        assert!(issues.is_none(), "unexpected issues: {issues:?}");

        assert_eq!(crate::ucfx::extract_chunk_body(&container, b"AAAA"), Some(a));
        assert_eq!(crate::ucfx::extract_chunk_body(&container, b"BBBB"), Some(b));
    }

    #[test]
    fn write_effect_container_roundtrip_via_walker() {
        let template = EffectTemplate {
            header: Some(EffectHeader { magic: 0x0226, sub_count: 1 }),
            emitters: EmitterTable { refs: vec![0x9999_AAAA] },
            emit: EmitTiming { floats: vec![0.5, 1.0] },
            gradient: None,
            forces: vec![Force {
                inner_hash: u32::from_le_bytes(*b"GRAV"),
                kind: ForceKind::Gravity,
                params: [0.0, -9.8, 0.0, 0.0],
                param_count: 2,
            }],
            ptype: Some(ParticleType { flags: 0x01 }),
            offset: Some([0.0, 1.0, 2.0]),
            transform: None,
            text_refs: vec![0x1234_5678],
        };
        let container = write_effect_container(&template);
        assert!(crate::ucfx::verify_ucfx_container(&container, "eff", 0).is_none());
        let tags: &[&[u8; 4]] = &[b"EFCT", b"EMTR", b"EMIT", b"POFF", b"PTYP", b"FRCE", b"TEXT"];
        let mut pairs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
        for tag in tags {
            if let Some(body) = crate::ucfx::extract_chunk_body(&container, tag) {
                pairs.push((*tag, body));
            }
        }
        let borrowed: Vec<(&[u8; 4], &[u8])> =
            pairs.iter().map(|(tag, body)| (*tag, body.as_slice())).collect();
        let back = EffectTemplate::from_chunks(borrowed);
        assert_eq!(back.header, template.header);
        assert_eq!(back.emitters, template.emitters);
        assert_eq!(back.emit, template.emit);
        assert_eq!(back.ptype, template.ptype);
        assert_eq!(back.offset, template.offset);
        assert_eq!(back.text_refs, template.text_refs);
        assert_eq!(back.forces.len(), 1);
        assert_eq!(back.forces[0].inner_hash, template.forces[0].inner_hash);
        assert_eq!(back.forces[0].param_count, template.forces[0].param_count);
        assert_eq!(
            back.forces[0].params[..2],
            template.forces[0].params[..2]
        );
    }

    #[test]
    fn write_fxdict_container_roundtrip_via_walker() {
        let params: Vec<FxParam> = (0..8)
            .map(|i| FxParam {
                name_hash: 0x1000 + i,
                default: i as f32,
                value_b: 2.0 * i as f32,
                value_c: 0.5 * i as f32,
                flags: 0xF000_0000 | i,
            })
            .collect();
        let container = write_fxdict_container(&params);
        assert!(crate::ucfx::verify_ucfx_container(&container, "fxd", 0).is_none());
        let info = crate::ucfx::extract_chunk_body(&container, b"INFO").expect("INFO present");
        let dict = crate::ucfx::extract_chunk_body(&container, b"DICT").expect("DICT present");
        assert_eq!(info.len(), 4);
        assert_eq!(dict.len(), params.len() * DICT_RECORD_BYTES);
        let back = parse_fxdict(&info, &dict).unwrap();
        assert_eq!(back, params);
    }
}
