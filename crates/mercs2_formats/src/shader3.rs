//! `shader3.bin` — the PC retail Mercenaries 2 compiled-shader store, plus the
//! CTAB-driven `vs_3_0` **instancing splice** (density upgrade / M4, piece 1).
//!
//! ## Container (proven offline, cross-checked against loader `FUN_0085b3f0`)
//! ```text
//! [u32 count][count × Record{ id:u32, blob_off:u32, blob_size:u32, kind:u32 }][blobs]
//! ```
//! `kind` 1 = `vs_3_0` (blob begins `0xfffe0300`), 0 = `ps_3_0` (`0xffff0300`).
//! Retail `game-files/shader3.bin` = 556 records (151 VS + 405 PS). The engine
//! loads each blob into `CreateVertexShader`/`CreatePixelShader` and files the
//! resulting D3D handle by `id` into a `%0x1200` table (`FUN_0085b810`). NOTE:
//! `id` is **not** `FNV(name)` — do not assume a name→blob map here.
//!
//! ## Why this exists — the M4 shader crux, resolved
//! Every static-mesh VS delivers its per-object transform as the constant block
//! **`objectData`** (exactly 4 float4 registers = the World / LocalToWorld matrix;
//! also feeds the normal/tangent basis) and composes it with the shared
//! **`viewContextData`** (ViewProj) *in-shader*:
//! ```text
//! dp4 r0, v0, c[O..O+3]   ; worldPos = position × objectData
//! dp4 o0, r0, c0..c3      ; clipPos  = worldPos × viewContextData
//! ```
//! Hardware instancing therefore needs **zero new math** — only redirect the four
//! `objectData` const reads `c[O..O+3]` to four per-instance vertex inputs, and
//! declare those inputs. `O` is **per-shader** (recovered from the CTAB), not a
//! fixed register. This module performs exactly that operand rewrite + `dcl`
//! insertion, preserving the CTAB (its offsets are self-relative, so it stays
//! valid) and every other instruction.

pub const VS_3_0: u32 = 0xfffe_0300;
pub const PS_3_0: u32 = 0xffff_0300;
const END_TOKEN: u32 = 0x0000_ffff;
const COMMENT_OPCODE: u16 = 0xfffe;
const OP_DCL: u16 = 0x1f;
const OP_DEF: u16 = 0x51;
const OP_DEFI: u16 = 0x30;
const OP_DEFB: u16 = 0x2f;

// D3DSHADER_PARAM_REGISTER_TYPE (subset)
pub const REG_TEMP: u32 = 0;
pub const REG_INPUT: u32 = 1;
pub const REG_CONST: u32 = 2;

const PARAM_TOKEN_BIT: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy)]
pub enum ShaderKind {
    Vertex,
    Pixel,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub id: u32,
    pub blob_off: u32,
    pub blob_size: u32,
    pub kind: ShaderKind,
}

#[derive(Debug, Clone)]
pub struct Constant {
    pub name: String,
    pub register_set: u16, // 0=b 1=i 2=c 3=s
    pub register_index: u16,
    pub register_count: u16,
}

#[derive(Debug)]
pub struct Store {
    pub bytes: Vec<u8>,
    pub records: Vec<Record>,
}

#[derive(Debug)]
pub enum Error {
    Truncated(&'static str),
    BadCount(u32),
    NotVertexShader,
    NoCtab,
    ConstantNotFound(String),
    NotFourRegisters { name: String, count: u16 },
    NoFreeInputs,
    Structure(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Truncated(w) => write!(f, "truncated shader3: {w}"),
            Error::BadCount(c) => write!(f, "implausible record count {c}"),
            Error::NotVertexShader => write!(f, "blob is not a vs_3_0 shader"),
            Error::NoCtab => write!(f, "no CTAB constant table in blob"),
            Error::ConstantNotFound(n) => write!(f, "constant {n:?} not declared by shader"),
            Error::NotFourRegisters { name, count } => {
                write!(f, "constant {name:?} spans {count} registers, expected 3 or 4 (World matrix)")
            }
            Error::NoFreeInputs => write!(f, "no 4 free contiguous input registers/semantics"),
            Error::Structure(w) => write!(f, "malformed shader bytecode: {w}"),
        }
    }
}
impl std::error::Error for Error {}

fn rd_u32(b: &[u8], off: usize) -> Result<u32, Error> {
    let s = b.get(off..off + 4).ok_or(Error::Truncated("u32"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn rd_u16(b: &[u8], off: usize) -> Result<u16, Error> {
    let s = b.get(off..off + 2).ok_or(Error::Truncated("u16"))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

impl Store {
    /// Parse the container table (does not copy blobs; they live in `bytes`).
    pub fn parse(bytes: Vec<u8>) -> Result<Store, Error> {
        let count = rd_u32(&bytes, 0)?;
        // sanity: the table must fit and the count be plausible for this store.
        if count == 0 || count as usize > (bytes.len() / 16) {
            return Err(Error::BadCount(count));
        }
        let mut records = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let base = 4 + i * 16;
            let id = rd_u32(&bytes, base)?;
            let blob_off = rd_u32(&bytes, base + 4)?;
            let blob_size = rd_u32(&bytes, base + 8)?;
            let kind_raw = rd_u32(&bytes, base + 12)?;
            let kind = match kind_raw {
                1 => ShaderKind::Vertex,
                0 => ShaderKind::Pixel,
                other => return Err(Error::Structure(if other > 1 {
                    "record kind not in {0,1}"
                } else {
                    "record kind"
                })),
            };
            if (blob_off as usize).saturating_add(blob_size as usize) > bytes.len() {
                return Err(Error::Truncated("blob out of range"));
            }
            records.push(Record { id, blob_off, blob_size, kind });
        }
        Ok(Store { bytes, records })
    }

    pub fn blob(&self, r: &Record) -> &[u8] {
        &self.bytes[r.blob_off as usize..(r.blob_off + r.blob_size) as usize]
    }
}

/// Read the length-prefixed constant table (`CTAB`) a Pangea `.sho` blob carries
/// verbatim in its first comment block. Returns (creator, constants).
pub fn parse_ctab(blob: &[u8]) -> Result<(String, Vec<Constant>), Error> {
    let p = find_ctab(blob).ok_or(Error::NoCtab)?;
    let base = p + 4; // header begins right after the "CTAB" fourcc
    let creator_off = rd_u32(blob, base + 4)? as usize;
    let constants = rd_u32(blob, base + 12)? as usize;
    let cinfo_off = rd_u32(blob, base + 16)? as usize;
    let creator = cstr(blob, base + creator_off);
    let mut out = Vec::with_capacity(constants);
    for i in 0..constants {
        let o = base + cinfo_off + i * 20;
        let name_off = rd_u32(blob, o)? as usize;
        out.push(Constant {
            name: cstr(blob, base + name_off),
            register_set: rd_u16(blob, o + 4)?,
            register_index: rd_u16(blob, o + 6)?,
            register_count: rd_u16(blob, o + 8)?,
        });
    }
    Ok((creator, out))
}

fn find_ctab(blob: &[u8]) -> Option<usize> {
    blob.windows(4).position(|w| w == b"CTAB")
}

fn cstr(b: &[u8], off: usize) -> String {
    if off >= b.len() {
        return String::new();
    }
    let end = b[off..].iter().position(|&c| c == 0).map(|e| off + e).unwrap_or(b.len());
    String::from_utf8_lossy(&b[off..end]).into_owned()
}

// ── SM3 token stream ────────────────────────────────────────────────────────

fn regtype(tok: u32) -> u32 {
    ((tok >> 28) & 0x7) | ((tok >> 8) & 0x18)
}
fn regnum(tok: u32) -> u32 {
    tok & 0x7ff
}
fn set_reg(tok: u32, ty: u32, num: u32) -> u32 {
    let cleared = tok & !(0x7 << 28) & !(0x3 << 11) & !0x7ff;
    cleared | ((ty & 0x7) << 28) | (((ty >> 3) & 0x3) << 11) | (num & 0x7ff)
}

/// A decoded instruction span over the raw token vector, for walking/rewriting.
struct Instr {
    opcode: u16,
    /// index of the opcode token in the token vector
    at: usize,
    /// number of parameter tokens following the opcode
    nparams: usize,
}

/// Split a shader token vector into (header_end, instructions, end_index).
/// `header_end` is the token index of the first *executable* instruction
/// (past version + comments + dcl/def) — the insertion point for new `dcl`s.
fn walk(tokens: &[u32]) -> Result<(usize, Vec<Instr>), Error> {
    if tokens.is_empty() || tokens[0] != VS_3_0 {
        return Err(Error::NotVertexShader);
    }
    let mut i = 1usize;
    let mut instrs = Vec::new();
    let mut first_exec: Option<usize> = None;
    while i < tokens.len() {
        let tok = tokens[i];
        if tok == END_TOKEN {
            break;
        }
        let opcode = (tok & 0xffff) as u16;
        if opcode == COMMENT_OPCODE {
            let len = ((tok >> 16) & 0x7fff) as usize;
            i += 1 + len;
            continue;
        }
        let nparams = ((tok >> 24) & 0xf) as usize;
        let is_decl = matches!(opcode, OP_DCL | OP_DEF | OP_DEFI | OP_DEFB);
        if !is_decl && first_exec.is_none() {
            first_exec = Some(i);
        }
        instrs.push(Instr { opcode, at: i, nparams });
        i += 1 + nparams;
        if i > tokens.len() {
            return Err(Error::Structure("instruction runs past end"));
        }
    }
    Ok((first_exec.unwrap_or(i), instrs))
}

/// Which input registers (`v#`) and TEXCOORD usage-indices the shader already uses,
/// so the splice can pick four fresh, non-colliding ones.
fn used_inputs(tokens: &[u32], instrs: &[Instr]) -> (Vec<u32>, Vec<u32>) {
    let mut regs = Vec::new();
    let mut texcoord_idx = Vec::new();
    for ins in instrs {
        if ins.opcode == OP_DCL && ins.nparams >= 2 {
            let usage_tok = tokens[ins.at + 1];
            let dst = tokens[ins.at + 2];
            if regtype(dst) == REG_INPUT {
                regs.push(regnum(dst));
                let usage = usage_tok & 0xf;
                if usage == 5 {
                    // D3DDECLUSAGE_TEXCOORD
                    texcoord_idx.push((usage_tok >> 16) & 0xf);
                }
            }
        }
    }
    (regs, texcoord_idx)
}

/// Result of a splice, with an audit trail for the offline verification gate.
#[derive(Debug)]
pub struct SpliceReport {
    pub object_data_reg: u16,
    /// number of World registers streamed (4 = float4x4, 3 = affine float4x3).
    pub world_regs: u32,
    pub input_base: u32,
    pub texcoord_base: u32,
    /// (token index, old const reg, new input reg) for every redirected operand.
    pub redirects: Vec<(usize, u32, u32)>,
}

/// Rewrite one `vs_3_0` blob so the `objectData` (World) matrix is read from a
/// per-instance vertex stream instead of constant registers. Returns the new blob
/// plus a report. The CTAB is preserved verbatim (its self-relative offsets stay
/// valid; the now-unread `objectData` constant uploads become dead but harmless).
///
/// `input_base`/`texcoord_base` may be `None` to auto-pick four fresh, contiguous
/// input registers and TEXCOORD semantic indices that do not collide with the
/// shader's existing inputs — these MUST match the stream-1 vertex declaration.
pub fn splice_instanced_world(
    blob: &[u8],
    input_base: Option<u32>,
    texcoord_base: Option<u32>,
) -> Result<(Vec<u8>, SpliceReport), Error> {
    if blob.len() < 4 || (blob.len() % 4) != 0 {
        return Err(Error::Structure("blob length not a whole number of tokens"));
    }
    let mut tokens: Vec<u32> = blob
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    // 1. locate objectData (the per-object World matrix) from the CTAB.
    let (_creator, consts) = parse_ctab(blob)?;
    let od = consts
        .iter()
        .find(|c| c.name == "objectData")
        .ok_or_else(|| Error::ConstantNotFound("objectData".into()))?;
    // World is a float4x4 (4 regs) or an affine float4x3 (3 regs, implicit w-row).
    if od.register_count != 4 && od.register_count != 3 {
        return Err(Error::NotFourRegisters {
            name: od.name.clone(),
            count: od.register_count,
        });
    }
    let o = od.register_index as u32;
    let nregs = od.register_count as u32;

    // 2. choose four fresh input registers + TEXCOORD usage indices.
    let (header_end, instrs) = walk(&tokens)?;
    let (used_regs, used_tex) = used_inputs(&tokens, &instrs);
    let vbase = match input_base {
        Some(v) => v,
        None => (0u32..=15 - nregs)
            .find(|b| (0..nregs).all(|k| !used_regs.contains(&(b + k))))
            .ok_or(Error::NoFreeInputs)?,
    };
    let tbase = match texcoord_base {
        Some(t) => t,
        None => (0u32..=15 - nregs)
            .find(|b| (0..nregs).all(|k| !used_tex.contains(&(b + k))))
            .ok_or(Error::NoFreeInputs)?,
    };

    // 3. redirect every source operand reading c[O..O+3] → v[vbase..vbase+3].
    //    Source operands are every param token after the first (destination) one,
    //    for executable instructions. dcl/def tokens are skipped (no const reads).
    let mut redirects = Vec::new();
    for ins in &instrs {
        if matches!(ins.opcode, OP_DCL | OP_DEF | OP_DEFI | OP_DEFB) {
            continue;
        }
        // params: [dst, src0, src1, ...]; only sources (index >= 1) read registers.
        for p in 1..ins.nparams {
            let idx = ins.at + 1 + p;
            let tok = tokens[idx];
            if regtype(tok) == REG_CONST {
                let rn = regnum(tok);
                if (o..o + nregs).contains(&rn) {
                    // objectData is a fixed 3/4-reg block; it is never relatively
                    // addressed. If it somehow is, the index is ambiguous — refuse
                    // rather than mis-splice. (Relative addressing ELSEWHERE in the
                    // shader — spline/palette arrays — is fine and left untouched;
                    // its extra address token decodes as regtype ADDR/LOOP, never
                    // CONST, so this per-token scan skips it safely.)
                    if tok & (1 << 13) != 0 {
                        return Err(Error::Structure(
                            "relative addressing on objectData register — unsupported",
                        ));
                    }
                    let newnum = vbase + (rn - o);
                    tokens[idx] = set_reg(tok, REG_INPUT, newnum);
                    redirects.push((idx, rn, newnum));
                }
            }
        }
    }
    if redirects.is_empty() {
        return Err(Error::Structure("objectData declared but never read"));
    }

    // 4. insert the input dcls at the header boundary (dcl_texcoord{tbase..} v{vbase..}).
    let mut dcls = Vec::with_capacity(nregs as usize * 3);
    for k in 0..nregs {
        // dcl opcode token: opcode 0x1f, length 2.
        dcls.push(0x1f | (2 << 24));
        // usage token: TEXCOORD(5) | usageindex<<16.
        dcls.push(5 | ((tbase + k) << 16));
        // dst input register, full write mask, param-token bit set.
        let dst = PARAM_TOKEN_BIT | (0xf << 16);
        dcls.push(set_reg(dst, REG_INPUT, vbase + k));
    }
    tokens.splice(header_end..header_end, dcls);

    // 5. serialize.
    let mut out = Vec::with_capacity(tokens.len() * 4);
    for t in &tokens {
        out.extend_from_slice(&t.to_le_bytes());
    }
    Ok((
        out,
        SpliceReport {
            object_data_reg: o as u16,
            world_regs: nregs,
            input_base: vbase,
            texcoord_base: tbase,
            redirects,
        },
    ))
}

/// Offline structural verification of a spliced blob (the piece-1 gate): it must
/// still parse as a `vs_3_0`, retain its CTAB, read **no** constant register in
/// `[O..O+3]` any more, and declare four new input registers at `vbase`.
pub fn verify_splice(spliced: &[u8], report: &SpliceReport) -> Result<(), Error> {
    let tokens: Vec<u32> = spliced
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    if tokens.first() != Some(&VS_3_0) {
        return Err(Error::NotVertexShader);
    }
    if find_ctab(spliced).is_none() {
        return Err(Error::NoCtab);
    }
    let (_he, instrs) = walk(&tokens)?;
    let o = report.object_data_reg as u32;
    for ins in &instrs {
        if matches!(ins.opcode, OP_DCL | OP_DEF | OP_DEFI | OP_DEFB) {
            continue;
        }
        for p in 1..ins.nparams {
            let tok = tokens[ins.at + 1 + p];
            if regtype(tok) == REG_CONST && (o..o + report.world_regs).contains(&regnum(tok)) {
                return Err(Error::Structure("objectData const read survived the splice"));
            }
        }
    }
    let (used_regs, _used_tex) = used_inputs(&tokens, &instrs);
    for k in 0..report.world_regs {
        if !used_regs.contains(&(report.input_base + k)) {
            return Err(Error::Structure("spliced input register not declared"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A hand-built minimal vs_3_0 mirroring rec419 (the simplest static-mesh VS):
    //   dcl_position v0 ; dcl_position o0
    //   dp4 r0.{xyzw}, v0, c4..c7   (objectData = World at c4)
    //   dp4 o0.{xyzw}, r0, c0..c3   (viewContextData = ViewProj)
    // with a synthetic CTAB declaring objectData[c4:7] + viewContextData[c0:3].
    fn build_min_vs() -> Vec<u8> {
        let mut t: Vec<u32> = Vec::new();
        t.push(VS_3_0);
        // ---- CTAB comment block ----
        let ctab = build_ctab();
        let words = ctab.len() / 4;
        t.push((COMMENT_OPCODE as u32) | (((words + 1) as u32) << 16)); // +1 for fourcc
        t.push(u32::from_le_bytes(*b"CTAB"));
        for c in ctab.chunks_exact(4) {
            t.push(u32::from_le_bytes([c[0], c[1], c[2], c[3]]));
        }
        // ---- dcls ----
        let dcl = |usage: u32, ty: u32, num: u32| -> [u32; 3] {
            [0x1f | (2 << 24), usage, set_reg(PARAM_TOKEN_BIT | (0xf << 16), ty, num)]
        };
        for w in dcl(0, REG_INPUT, 0) { t.push(w); }   // dcl_position v0
        for w in dcl(0, 6, 0) { t.push(w); }           // dcl_position o0 (output type 6)
        // ---- dp4 r0.x..w, v0, c4..c7 ----
        let src = |ty: u32, num: u32| set_reg(PARAM_TOKEN_BIT | (0b11_10_01_00 << 16), ty, num);
        let dst = |ty: u32, num: u32, mask: u32| set_reg(PARAM_TOKEN_BIT | (mask << 16), ty, num);
        for k in 0..4u32 {
            t.push(0x09 | (3 << 24)); // dp4, 3 params
            t.push(dst(REG_TEMP, 0, 1 << k));
            t.push(src(REG_INPUT, 0));
            t.push(src(REG_CONST, 4 + k));
        }
        for k in 0..4u32 {
            t.push(0x09 | (3 << 24));
            t.push(dst(6, 0, 1 << k)); // o0 output
            t.push(src(REG_TEMP, 0));
            t.push(src(REG_CONST, k));
        }
        t.push(END_TOKEN);
        t.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn build_ctab() -> Vec<u8> {
        // header(28) + 2 constinfo(20 each) + name strings.
        let mut b = Vec::new();
        let put = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
        let header = 28u32;
        let cinfo_off = header;
        let names_off = header + 40;
        let name_view = names_off; // "viewContextData\0"
        let name_obj = names_off + 16; // "objectData\0"
        let creator_off = name_obj + 11;
        put(&mut b, header); // Size
        put(&mut b, creator_off); // Creator
        put(&mut b, VS_3_0); // Version
        put(&mut b, 2); // Constants
        put(&mut b, cinfo_off); // ConstantInfo
        put(&mut b, 0); // Flags
        put(&mut b, 0); // Target
        // constinfo[0] = viewContextData c0..3
        put(&mut b, name_view);
        b.extend_from_slice(&2u16.to_le_bytes()); // regset c
        b.extend_from_slice(&0u16.to_le_bytes()); // index 0
        b.extend_from_slice(&4u16.to_le_bytes()); // count 4
        b.extend_from_slice(&0u16.to_le_bytes());
        put(&mut b, 0);
        put(&mut b, 0);
        // constinfo[1] = objectData c4..7
        put(&mut b, name_obj);
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        put(&mut b, 0);
        put(&mut b, 0);
        // names
        b.extend_from_slice(b"viewContextData\0");
        b.extend_from_slice(b"objectData\0");
        b.extend_from_slice(b"x\0"); // creator (placeholder)
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b
    }

    #[test]
    fn ctab_roundtrips_object_data_at_c4() {
        let blob = build_min_vs();
        let (_creator, consts) = parse_ctab(&blob).unwrap();
        let od = consts.iter().find(|c| c.name == "objectData").unwrap();
        assert_eq!(od.register_index, 4);
        assert_eq!(od.register_count, 4);
    }

    #[test]
    fn splice_redirects_world_and_verifies() {
        let blob = build_min_vs();
        let (spliced, report) = splice_instanced_world(&blob, None, None).unwrap();
        // objectData was at c4; four dp4 (position) + four dp3 would read it — here
        // four position dp4 read c4..c7 → 4 redirects.
        assert_eq!(report.object_data_reg, 4);
        assert_eq!(report.redirects.len(), 4);
        // auto-picked inputs must avoid v0 (already used).
        assert!(report.input_base >= 1);
        verify_splice(&spliced, &report).unwrap();
        // viewContextData (c0..c3) reads must SURVIVE (shared ViewProj, untouched).
        let tokens: Vec<u32> = spliced
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let saw_c0 = tokens.iter().any(|&t| {
            (t & PARAM_TOKEN_BIT) != 0 && regtype(t) == REG_CONST && regnum(t) < 4
        });
        assert!(saw_c0, "viewContextData reads must be preserved");
    }

    #[test]
    fn parse_real_store_if_present() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../game-files/shader3.bin"
        );
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("SKIP: {path} not present");
            return;
        };
        let store = Store::parse(bytes).unwrap();
        let vs = store.records.iter().filter(|r| matches!(r.kind, ShaderKind::Vertex)).count();
        let ps = store.records.len() - vs;
        assert_eq!(store.records.len(), 556);
        assert_eq!((vs, ps), (151, 405));
        // every VS blob must carry a CTAB and parse.
        for r in store.records.iter().filter(|r| matches!(r.kind, ShaderKind::Vertex)) {
            let blob = store.blob(r);
            assert_eq!(u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]), VS_3_0);
            parse_ctab(blob).unwrap();
        }
    }
}
