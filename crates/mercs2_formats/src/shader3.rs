//! `shader3.bin` — the PC retail Mercenaries 2 compiled-shader store, plus the
//! CTAB-driven `vs_3_0` **instancing splice** (density upgrade / M4, piece 1).
//!
//! ## Container (proven offline, cross-checked against loader `FUN_0085b3f0`)
//! ```text
//! [u32 count][count × Record{ id:u32, blob_off:u32, blob_size:u32, kind:u32 }][blobs]
//! ```
//! `kind` 1 = `vs_3_0` (blob begins `0xfffe0300`), 0 = `ps_3_0` (`0xffff0300`).
//! Retail `shader3.bin` = 556 records (151 VS + 405 PS). The engine copies each
//! blob into one 0x8000-byte scratch buffer (so [`MAX_BLOB`] is a hard cap), hands
//! it to `CreateVertexShader`/`CreatePixelShader`, and files the D3D handle by `id`
//! into a `%0x1200` open-addressed table (`FUN_0085b810`, [`TABLE_SLOTS`]).
//!
//! ## Record ids
//! `id = pandemic_hash_m2(stem + "_3.sho")` in `shader3.bin` and
//! `pandemic_hash_m2(stem + "_3l.sho")` in `shader3Low.bin`, where `stem` is the
//! registered `.sho` file name without `.sho` (`FUN_0085b6f0` cuts four characters
//! and appends the suffix). See [`store_id`]. A lookup probes from `id % 0x1200` and
//! takes the first match, so when two records share an id the first loaded wins.
//!
//! ## Writing a store
//! [`StoreBuilder`] rewrites a store in the retail layout (blobs 16-byte aligned,
//! zero padding, file padded to 16, record order = blob order), replaces one record
//! in place or appends one, and refuses anything the loader cannot take.
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

pub use crate::sm3asm::{END_TOKEN, PS_3_0, VS_3_0};
use crate::hash::pandemic_hash_m2;
use crate::sm3asm::Sm3Error;

const COMMENT_OPCODE: u16 = 0xfffe;

/// Largest blob the loader accepts: `FUN_0085b3f0` `memcpy`s every record into one 0x8000-byte
/// scratch buffer with no size check.
pub const MAX_BLOB: usize = 0x8000;
/// Slots in the engine's id → shader table (`FUN_0085b810`, `id % 0x1200`, linear probe). The
/// insert probe never gives up, so every store loaded together must hold fewer records than this.
pub const TABLE_SLOTS: usize = 0x1200;
/// The retail PC shader stores, as shipped in the game's `data` folder.
pub const RETAIL_STORES: [&str; 6] = [
    "shader3.bin",
    "shader3Low.bin",
    "shaderVT.bin",
    "shaderVTLow.bin",
    "shaderR2VB.bin",
    "shaderR2VBLow.bin",
];
/// Blob alignment (and whole-file padding) in the retail stores.
const STORE_ALIGN: usize = 16;
const OP_DCL: u16 = 0x1f;
const OP_DEF: u16 = 0x51;
const OP_DEFI: u16 = 0x30;
const OP_DEFB: u16 = 0x2f;

// D3DSHADER_PARAM_REGISTER_TYPE (subset)
pub const REG_TEMP: u32 = 0;
pub const REG_INPUT: u32 = 1;
pub const REG_CONST: u32 = 2;

const PARAM_TOKEN_BIT: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// The SM3 codec refused the blob (malformed, or not exactly expressible).
    Sm3(Sm3Error),
    /// A blob larger than the loader's 0x8000-byte scratch buffer.
    BlobTooLarge { size: usize },
    /// A blob whose version token does not match the record kind.
    VersionMismatch { kind: ShaderKind, found: u32 },
    /// `replace_in_place` of an id the store does not hold.
    MissingTarget(u32),
    /// The id already exists: `store` is `None` for this store, `Some(i)` for `others[i]`.
    IdCollision { id: u32, store: Option<usize> },
    /// The stores loaded together would hold this many records; the table has [`TABLE_SLOTS`].
    TooManyRecords { total: usize },
    /// A `store_id` stem that is empty or still ends in `.sho`.
    BadStem(String),
}

impl From<Sm3Error> for Error {
    fn from(e: Sm3Error) -> Self {
        Error::Sm3(e)
    }
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
            Error::Sm3(e) => write!(f, "SM3 bytecode: {e}"),
            Error::BlobTooLarge { size } => {
                write!(f, "blob is {size} bytes; the loader copies every blob into a {MAX_BLOB}-byte buffer")
            }
            Error::VersionMismatch { kind, found } => {
                let want = match kind {
                    ShaderKind::Vertex => VS_3_0,
                    ShaderKind::Pixel => PS_3_0,
                };
                write!(f, "{kind:?} record needs version token 0x{want:08x}, the blob starts 0x{found:08x}")
            }
            Error::MissingTarget(id) => write!(f, "no record with id 0x{id:08x} to replace"),
            Error::IdCollision { id, store: None } => {
                write!(f, "id 0x{id:08x} is already in this store; the first loaded record would win")
            }
            Error::IdCollision { id, store: Some(i) } => write!(
                f,
                "id 0x{id:08x} is also in the other store #{i}; which record the engine binds depends on load order"
            ),
            Error::TooManyRecords { total } => write!(
                f,
                "{total} records across the stores loaded together; the engine's id table has {TABLE_SLOTS} slots and must keep one free"
            ),
            Error::BadStem(s) => write!(f, "store_id stem {s:?} must be non-empty and exclude the .sho extension"),
        }
    }
}
impl std::error::Error for Error {}

fn rd_u32(b: &[u8], off: usize) -> Result<u32, Error> {
    let s = b.get(off..off + 4).ok_or(Error::Truncated("u32"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
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

/// Read the constant table (`CTAB`) a `.sho` blob carries in its first `CTAB` comment block.
/// Returns (creator, constants). Decoding is [`crate::sm3asm::Ctab::decode`].
pub fn parse_ctab(blob: &[u8]) -> Result<(String, Vec<Constant>), Error> {
    let ctab = crate::sm3asm::find_ctab(blob)?.ok_or(Error::NoCtab)?;
    let consts = ctab
        .constants
        .iter()
        .map(|c| Constant {
            name: c.name.clone(),
            register_set: c.register_set,
            register_index: c.register_index,
            register_count: c.register_count,
        })
        .collect();
    Ok((ctab.creator, consts))
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
    // Accept either shader version token (`vs_3_0`/`ps_3_0`): the token stream grammar (comments,
    // dcl/def, instruction param counts) is identical. VS-only callers (the splice) independently
    // require `objectData`, so a PS blob can never reach them.
    if tokens.is_empty() || (tokens[0] != VS_3_0 && tokens[0] != PS_3_0) {
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
    /// `mov temp,input` copies inserted so no instruction reads two input registers.
    pub input_copies: u32,
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

    // 3a. vs_3_0 allows AT MOST ONE input register (v#) read per instruction (proven live: a splice
    //     that made an instruction read two inputs was rejected D3DERR_INVALIDCALL, 43/60). Since the
    //     objectData redirect turns a const read into an input read, any instruction that already
    //     reads an input register (position/normal/tangent) alongside objectData would then read two.
    //     Find those pre-existing inputs; we copy each to a fresh temp at the top and read the temp
    //     there instead. Also track the highest temp in use, to allocate above it.
    let mut conflict_inputs: Vec<u32> = Vec::new();
    let mut max_temp: i64 = -1;
    for ins in &instrs {
        // Skip dcl/def first: their trailing tokens are semantics / float immediates, NOT register
        // operands — decoding them as registers would pollute max_temp (a def's float bits can look
        // like a temp with a huge index).
        if matches!(ins.opcode, OP_DCL | OP_DEF | OP_DEFI | OP_DEFB) {
            continue;
        }
        for p in 0..ins.nparams {
            let t = tokens[ins.at + 1 + p];
            if regtype(t) == REG_TEMP {
                max_temp = max_temp.max(regnum(t) as i64);
            }
        }
        let mut od_here = 0u32;
        let mut inputs_here: Vec<u32> = Vec::new();
        for p in 1..ins.nparams {
            let t = tokens[ins.at + 1 + p];
            match regtype(t) {
                REG_CONST if (o..o + nregs).contains(&regnum(t)) => {
                    // objectData is a fixed block, never relatively addressed; refuse if it is.
                    if t & (1 << 13) != 0 {
                        return Err(Error::Structure(
                            "relative addressing on objectData register — unsupported",
                        ));
                    }
                    od_here += 1;
                }
                REG_INPUT => inputs_here.push(regnum(t)),
                _ => {}
            }
        }
        if od_here >= 1 {
            // Two objectData rows in one instruction → both become inputs, unsplittable this way.
            if od_here >= 2 {
                return Err(Error::Structure(
                    "instruction reads two objectData registers — unsupported",
                ));
            }
            for v in inputs_here {
                if !conflict_inputs.contains(&v) {
                    conflict_inputs.push(v);
                }
            }
        }
    }
    conflict_inputs.sort_unstable();
    let temp_base = (max_temp + 1) as u32;
    if temp_base as usize + conflict_inputs.len() > 32 {
        return Err(Error::Structure("out of temp registers for input copies"));
    }
    // input register -> its private temp copy
    let temp_map: Vec<(u32, u32)> = conflict_inputs
        .iter()
        .enumerate()
        .map(|(i, &v)| (v, temp_base + i as u32))
        .collect();

    // 3b. rewrite operands: objectData const -> new instance input; a conflicting input -> its temp.
    let mut redirects = Vec::new();
    for ins in &instrs {
        if matches!(ins.opcode, OP_DCL | OP_DEF | OP_DEFI | OP_DEFB) {
            continue;
        }
        for p in 1..ins.nparams {
            let idx = ins.at + 1 + p;
            let tok = tokens[idx];
            match regtype(tok) {
                REG_CONST if (o..o + nregs).contains(&regnum(tok)) => {
                    let newnum = vbase + (regnum(tok) - o);
                    redirects.push((idx, regnum(tok), newnum));
                    tokens[idx] = set_reg(tok, REG_INPUT, newnum);
                }
                REG_INPUT => {
                    if let Some(&(_, t)) = temp_map.iter().find(|(v, _)| *v == regnum(tok)) {
                        tokens[idx] = set_reg(tok, REG_TEMP, t);
                    }
                }
                _ => {}
            }
        }
    }
    if redirects.is_empty() {
        return Err(Error::Structure("objectData declared but never read"));
    }

    // 4. header insertions at the first executable instruction: the instance-input dcls, then a
    //    `mov temp, input` copy for each conflicting input (these run before any transform).
    let mut header_ins: Vec<u32> = Vec::with_capacity(nregs as usize * 3 + temp_map.len() * 3);
    for k in 0..nregs {
        header_ins.push(0x1f | (2 << 24)); // dcl, length 2
        // semantic token: TEXCOORD(5) | usageindex<<16, WITH the param-token bit (0x80000000).
        // Omitting that bit makes the D3D9 runtime reject the shader D3DERR_INVALIDCALL.
        header_ins.push(PARAM_TOKEN_BIT | 5 | ((tbase + k) << 16));
        header_ins.push(set_reg(PARAM_TOKEN_BIT | (0xf << 16), REG_INPUT, vbase + k));
    }
    for &(vx, t) in &temp_map {
        header_ins.push(0x01 | (2 << 24)); // mov, length 2
        header_ins.push(set_reg(PARAM_TOKEN_BIT | (0xf << 16), REG_TEMP, t)); // dst temp, full mask
        header_ins.push(set_reg(PARAM_TOKEN_BIT | (0xe4 << 16), REG_INPUT, vx)); // src input .xyzw
    }
    tokens.splice(header_end..header_end, header_ins);

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
            input_copies: temp_map.len() as u32,
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
    if crate::sm3asm::find_ctab(spliced)?.is_none() {
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
    // The rule the live D3D9 runtime enforces (proven at R0): at most ONE input register read per
    // instruction. Verify no executable instruction sources two distinct v# — this is what the
    // temp-copy pass exists to guarantee, and the offline gate that now catches its absence.
    for ins in &instrs {
        if matches!(ins.opcode, OP_DCL | OP_DEF | OP_DEFI | OP_DEFB) {
            continue;
        }
        let mut seen: Vec<u32> = Vec::new();
        for p in 1..ins.nparams {
            let t = tokens[ins.at + 1 + p];
            if regtype(t) == REG_INPUT && !seen.contains(&regnum(t)) {
                seen.push(regnum(t));
            }
        }
        if seen.len() > 1 {
            return Err(Error::Structure("instruction reads more than one input register"));
        }
    }
    Ok(())
}

// ── SM3.0 disassembly + CTAB-signature role classification (shader recovery) ─────────────────────
//
// `shader3.bin` is the PC retail store of **already-compiled D3D9 SM3.0 bytecode** (`vs_3_0`/`ps_3_0`),
// NOT Xenon microcode (that is the Xbox `.updb`/`ucode` path). Recovery toward WGSL therefore means:
// disassemble the SM3 token stream, and identify WHICH logical `Pg*` shader a record is.
//
// **Identity:** a record's `id` is `pandemic_hash_m2(<stem>_3.sho)` (`<stem>_3l.sho` in the Low store)
// — see [`store_id`]. The registry `FUN_0084f130` pairs each logical name with its `.sho` file name, so
// a record is named by hashing the registered `.sho` names. Every blob also keeps its intact CTAB →
// constant **names + register layout**, which [`classify_role`] reads as a constant *signature*.

/// Disassemble one `vs_3_0`/`ps_3_0` blob, one statement per line. The text is exact: [`crate::sm3asm::assemble`]
/// turns it back into the same bytes (CTAB included). This is the recovery surface a WGSL
/// translation reads off.
pub fn disassemble(blob: &[u8]) -> Result<Vec<String>, Error> {
    Ok(crate::sm3asm::disassemble(blob)?.lines().map(str::to_string).collect())
}

/// A role bucket for a shader record, assigned from its CTAB constant **signature**. These are
/// *candidate* classes, not `Pg*` names: e.g. a VS whose only constants are the view/projection block
/// and carries no `objectData`/`BoneMatrixArray` is a **fullscreen/far-plane** shader — the class the
/// sky, sun, moon, cloud and post-process shaders all fall into. A record's exact name comes from its
/// id instead (see [`store_id`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderRole {
    /// VS reading `BoneMatrixArray` — a skinned character/vehicle mesh vertex shader.
    SkinnedMeshVs,
    /// VS reading `objectData` (World) but no bones — a static-mesh vertex shader (the splice targets).
    StaticMeshVs,
    /// VS with neither `objectData` nor bones — a fullscreen / far-plane VS (sky/sun/moon/cloud/post
    /// candidates all live here; the sky draws a far-plane quad, post a fullscreen tri).
    FullscreenVs,
    /// Any other VS (special geometry paths).
    OtherVs,
    /// PS binding a `decalNormal` or `decalParam` sampler/const — a **decal** pixel-shader candidate.
    DecalPs,
    /// PS whose constant signature carries scattering/atmosphere params — a **sky/atmosphere** PS
    /// candidate (`beta`/`scatter`/`inscatter`/`sun`/`atmos`/`sky` named constants).
    SkyPs,
    /// PS binding one or more samplers with no lighting/scatter signature — a generic textured PS
    /// (mesh material / post-process; distinguishing those two is confirm-live).
    TexturedPs,
    /// PS with no sampler and no recognised signature (solid-colour / math-only).
    PlainPs,
}

impl ShaderRole {
    /// Whether this role is one of the sky/decal candidate buckets W5 cares about.
    pub fn is_w5_candidate(self) -> bool {
        matches!(self, ShaderRole::FullscreenVs | ShaderRole::DecalPs | ShaderRole::SkyPs)
    }
}

/// Classify a record into a [`ShaderRole`] from its kind + CTAB constant signature. This is the
/// static recovery step that narrows "which record could be `PgSky*`/`PgDecal*`" without a name map.
pub fn classify_role(kind: &ShaderKind, consts: &[Constant]) -> ShaderRole {
    let has = |n: &str| consts.iter().any(|c| c.name == n);
    let name_has = |needle: &str| {
        consts.iter().any(|c| c.name.to_ascii_lowercase().contains(needle))
    };
    match kind {
        ShaderKind::Vertex => {
            if has("BoneMatrixArray") {
                ShaderRole::SkinnedMeshVs
            } else if has("objectData") {
                ShaderRole::StaticMeshVs
            } else if consts.is_empty()
                || consts.iter().all(|c| c.register_set == 2 /* c */ && c.register_count <= 4)
            {
                // Only float-const scalars/vectors (view block etc.), no per-object/bone matrix → the
                // fullscreen/far-plane family (sky/sun/moon/cloud/post).
                ShaderRole::FullscreenVs
            } else {
                ShaderRole::OtherVs
            }
        }
        ShaderKind::Pixel => {
            let samplers = consts.iter().filter(|c| c.register_set == 3 /* s */).count();
            if name_has("decal") {
                ShaderRole::DecalPs
            } else if name_has("beta") || name_has("scatter") || name_has("inscatter")
                || name_has("atmos") || name_has("sky") || name_has("henyey")
            {
                ShaderRole::SkyPs
            } else if samplers > 0 {
                ShaderRole::TexturedPs
            } else {
                ShaderRole::PlainPs
            }
        }
    }
}

// ── store writer ─────────────────────────────────────────────────────────────────────────────────

/// The record id the engine files a registered `.sho` under: `pandemic_hash_m2(stem + "_3.sho")`, or
/// `stem + "_3l.sho"` for the Low store. `stem` is the registered `.sho` file name without `.sho`
/// (`PgMeshVP.sho` → `PgMeshVP`); `FUN_0085b6f0` cuts those four characters and appends the suffix.
pub fn store_id(stem: &str, low: bool) -> Result<u32, Error> {
    if stem.is_empty() || stem.to_ascii_lowercase().ends_with(".sho") {
        return Err(Error::BadStem(stem.to_string()));
    }
    let suffix = if low { "_3l.sho" } else { "_3.sho" };
    Ok(pandemic_hash_m2(&format!("{stem}{suffix}")))
}

/// One record of a store being written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreEntry {
    pub id: u32,
    pub kind: ShaderKind,
    pub blob: Vec<u8>,
}

/// Refuse a blob the loader cannot take for a record of `kind`.
fn check_blob(kind: ShaderKind, blob: &[u8]) -> Result<(), Error> {
    if blob.len() > MAX_BLOB {
        return Err(Error::BlobTooLarge { size: blob.len() });
    }
    if blob.len() < 8 || !blob.len().is_multiple_of(4) {
        return Err(Error::Structure("blob is not a whole token stream of at least version + end"));
    }
    let first = rd_u32(blob, 0)?;
    let want = match kind {
        ShaderKind::Vertex => VS_3_0,
        ShaderKind::Pixel => PS_3_0,
    };
    if first != want {
        return Err(Error::VersionMismatch { kind, found: first });
    }
    if rd_u32(blob, blob.len() - 4)? != END_TOKEN {
        return Err(Error::Structure("blob does not end with the end token"));
    }
    Ok(())
}

fn align_up(n: usize) -> usize {
    n.div_ceil(STORE_ALIGN) * STORE_ALIGN
}

/// Rebuilds a shader store in the retail layout. The `others` a mutation takes are every other
/// store the engine loads alongside this one: their ids share the engine's one id table.
/// `FUN_0084f130` always loads `shader3.bin` and `shader3Low.bin`, then either `shaderVT.bin` +
/// `shaderVTLow.bin` (caps bit 2) or `shaderR2VB.bin` + `shaderR2VBLow.bin` (bit 3), never both.
#[derive(Debug, Clone, Default)]
pub struct StoreBuilder {
    entries: Vec<StoreEntry>,
}

impl StoreBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every record of `store`, in record order, blobs copied verbatim.
    pub fn from_store(store: &Store) -> Self {
        let entries = store
            .records
            .iter()
            .map(|r| StoreEntry { id: r.id, kind: r.kind, blob: store.blob(r).to_vec() })
            .collect();
        StoreBuilder { entries }
    }

    pub fn entries(&self) -> &[StoreEntry] {
        &self.entries
    }

    fn check_others(id: u32, others: &[&Store]) -> Result<(), Error> {
        match others.iter().position(|s| s.records.iter().any(|r| r.id == id)) {
            Some(i) => Err(Error::IdCollision { id, store: Some(i) }),
            None => Ok(()),
        }
    }

    /// Swap the blob of the one record with `id`, keeping its position and kind.
    pub fn replace_in_place(&mut self, id: u32, blob: Vec<u8>, others: &[&Store]) -> Result<(), Error> {
        let hits: Vec<usize> = (0..self.entries.len()).filter(|&i| self.entries[i].id == id).collect();
        let at = match hits[..] {
            [] => return Err(Error::MissingTarget(id)),
            [one] => one,
            _ => return Err(Error::IdCollision { id, store: None }),
        };
        Self::check_others(id, others)?;
        check_blob(self.entries[at].kind, &blob)?;
        self.entries[at].blob = blob;
        Ok(())
    }

    /// Append a new record.
    pub fn add(&mut self, id: u32, kind: ShaderKind, blob: Vec<u8>, others: &[&Store]) -> Result<(), Error> {
        if self.entries.iter().any(|e| e.id == id) {
            return Err(Error::IdCollision { id, store: None });
        }
        Self::check_others(id, others)?;
        check_blob(kind, &blob)?;
        let total = self.entries.len() + 1 + others.iter().map(|s| s.records.len()).sum::<usize>();
        if total >= TABLE_SLOTS {
            return Err(Error::TooManyRecords { total });
        }
        self.entries.push(StoreEntry { id, kind, blob });
        Ok(())
    }

    /// Serialize: `[count][records][blobs]`, each blob at a 16-byte-aligned offset in record order,
    /// zero padding between blobs, the file padded to 16.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let n = self.entries.len();
        if n == 0 {
            return Err(Error::BadCount(0));
        }
        if n >= TABLE_SLOTS {
            return Err(Error::TooManyRecords { total: n });
        }
        let mut offsets = Vec::with_capacity(n);
        let mut at = align_up(4 + 16 * n);
        for e in &self.entries {
            offsets.push(at);
            at = align_up(at + e.blob.len());
        }
        let mut out = vec![0u8; at];
        out[0..4].copy_from_slice(&(n as u32).to_le_bytes());
        for (i, (e, &off)) in self.entries.iter().zip(&offsets).enumerate() {
            let kind: u32 = match e.kind {
                ShaderKind::Vertex => 1,
                ShaderKind::Pixel => 0,
            };
            let r = 4 + 16 * i;
            for (k, v) in [e.id, off as u32, e.blob.len() as u32, kind].iter().enumerate() {
                out[r + 4 * k..r + 4 * k + 4].copy_from_slice(&v.to_le_bytes());
            }
            out[off..off + e.blob.len()].copy_from_slice(&e.blob);
        }
        Ok(out)
    }
}

/// A retail store for a game-gated test, read from the `data` folder that holds the `vz.wad` named by
/// the repo-root `.mercs2-local.toml`. Built by the `retail` feature; fails if the config, the
/// archive, or the store is absent.
#[cfg(all(test, feature = "retail"))]
pub(crate) fn retail_store_for_test(name: &str) -> Store {
    let vz = crate::game_paths::local_config_vz_wad(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let data = vz
        .parent()
        .unwrap_or_else(|| panic!("{} has no parent folder", vz.display()));
    let path = data.join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Store::parse(bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
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
        // real dcl semantic tokens carry the param bit (0x80000000); mirror that here.
        let dcl = |usage: u32, ty: u32, num: u32| -> [u32; 3] {
            [0x1f | (2 << 24), PARAM_TOKEN_BIT | usage, set_reg(PARAM_TOKEN_BIT | (0xf << 16), ty, num)]
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

    /// A CTAB declaring viewContextData[c0:3] + objectData[c4:7] as float4x4, laid out by the
    /// same encoder the assembler uses.
    fn build_ctab() -> Vec<u8> {
        use crate::sm3asm::{Ctab, CtabConstant, CtabType};
        let matrix = CtabType { class: 3, ty: 3, rows: 4, columns: 4, elements: 1, members: vec![] };
        let constant = |name: &str, register_index: u16| CtabConstant {
            name: name.into(),
            register_set: 2,
            register_index,
            register_count: 4,
            reserved: 0,
            ty: matrix.clone(),
        };
        Ctab {
            creator: "test".into(),
            target: "vs_3_0".into(),
            flags: 0,
            constants: vec![constant("viewContextData", 0), constant("objectData", 4)],
        }
        .encode(VS_3_0)
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
    fn spliced_dcl_semantic_tokens_carry_param_bit() {
        // Regression: the dcl semantic token MUST have bit 31 set, or the D3D9 runtime rejects the
        // shader with D3DERR_INVALIDCALL (caught live at R0, not by structural re-parse).
        let blob = build_min_vs();
        let (spliced, _rep) = splice_instanced_world(&blob, None, None).unwrap();
        let tokens: Vec<u32> = spliced
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let mut i = 1;
        let mut checked = 0;
        while i < tokens.len() {
            let tok = tokens[i];
            if tok == END_TOKEN {
                break;
            }
            let op = (tok & 0xffff) as u16;
            if op == COMMENT_OPCODE {
                i += 1 + ((tok >> 16) & 0x7fff) as usize;
                continue;
            }
            let n = ((tok >> 24) & 0xf) as usize;
            if op == OP_DCL {
                let semantic = tokens[i + 1];
                assert!(
                    semantic & PARAM_TOKEN_BIT != 0,
                    "dcl semantic token 0x{semantic:08x} missing param bit"
                );
                checked += 1;
            }
            i += 1 + n;
        }
        assert!(checked >= 4, "expected the 4 inserted input dcls");
    }

    #[test]
    fn disassembles_the_min_vs_body() {
        let blob = build_min_vs();
        let asm = disassemble(&blob).unwrap();
        assert_eq!(asm[0], "vs_3_0");
        // The synthetic body is 2 dcls + 8 dp4; the CTAB comment must be skipped, not disassembled.
        assert!(asm.iter().any(|l| l.starts_with("dcl_position")), "dcl decoded: {asm:?}");
        assert!(asm.iter().filter(|l| l.starts_with("dp4 ")).count() == 8, "8 dp4: {asm:?}");
        // operand decode: the World reads are c4..c7, the ViewProj reads c0..c3.
        assert!(asm.iter().any(|l| l.contains("c4")), "objectData read present: {asm:?}");
    }

    #[test]
    fn classifies_static_mesh_vs_by_signature() {
        let blob = build_min_vs();
        let (_c, consts) = parse_ctab(&blob).unwrap();
        // objectData present, no BoneMatrixArray → static-mesh VS (a splice target).
        assert_eq!(classify_role(&ShaderKind::Vertex, &consts), ShaderRole::StaticMeshVs);
    }

    #[test]
    fn classifies_decal_and_sky_ps_by_constant_names() {
        let mk = |name: &str, set: u16| Constant {
            name: name.to_string(),
            register_set: set,
            register_index: 0,
            register_count: 1,
        };
        // a PS binding decalNormal → decal candidate.
        assert_eq!(
            classify_role(&ShaderKind::Pixel, &[mk("decalNormal", 3)]),
            ShaderRole::DecalPs
        );
        // a PS with a scattering const → sky candidate.
        assert_eq!(
            classify_role(&ShaderKind::Pixel, &[mk("betaRay", 2)]),
            ShaderRole::SkyPs
        );
        // a plain textured PS (one sampler, no signature).
        assert_eq!(
            classify_role(&ShaderKind::Pixel, &[mk("diffuseMap", 3)]),
            ShaderRole::TexturedPs
        );
    }

    // ── store writer ──

    fn magenta_ps() -> Vec<u8> {
        crate::sm3asm::assemble("ps_3_0\ndef c0, 1, 0, 1, 1\nmov oC0, c0").unwrap()
    }
    fn passthrough_vs() -> Vec<u8> {
        crate::sm3asm::assemble("vs_3_0\ndcl_position v0\ndcl_position o0\nmov o0, v0").unwrap()
    }
    fn small_store(ids: &[u32]) -> Store {
        let mut b = StoreBuilder::new();
        for &id in ids {
            b.add(id, ShaderKind::Pixel, magenta_ps(), &[]).unwrap();
        }
        Store::parse(b.to_bytes().unwrap()).unwrap()
    }

    #[test]
    fn store_id_matches_the_known_records() {
        assert_eq!(store_id("PgMeshVP", false).unwrap(), 0x2af7_398f);
        assert_eq!(store_id("PgSkyFP", false).unwrap(), 0xc91c_0187);
        assert_eq!(store_id("PgMeshVP", true).unwrap(), 0x9b0d_5961);
        assert_eq!(store_id("PgSkyFP", true).unwrap(), 0xa759_fdb9);
        assert!(matches!(store_id("", false), Err(Error::BadStem(_))));
        assert!(matches!(store_id("PgMeshVP.sho", false), Err(Error::BadStem(_))));
        // Materials name their pixel shader by the logical name, a different key space.
        assert_eq!(pandemic_hash_m2("PgDiffSpecNormFP"), 0xcaef_e1fe);
        assert_eq!(pandemic_hash_m2("PgDiffSpecReflNormAmbOccRimFP"), 0x322f_cd56);
    }

    #[test]
    fn store_writer_refusals() {
        let other = small_store(&[0x1111_1111]);
        let mut b = StoreBuilder::from_store(&small_store(&[0xaaaa_aaaa, 0xbbbb_bbbb]));

        // blob larger than the loader's 0x8000 scratch buffer
        let mut big = magenta_ps();
        let end = big.len() - 4;
        big.splice(end..end, std::iter::repeat_n(0u8, MAX_BLOB)); // pads before the end token
        assert!(matches!(b.add(0x2, ShaderKind::Pixel, big.clone(), &[]), Err(Error::BlobTooLarge { .. })));
        assert!(matches!(b.replace_in_place(0xaaaa_aaaa, big, &[]), Err(Error::BlobTooLarge { .. })));
        // version token vs kind
        assert!(matches!(b.add(0x2, ShaderKind::Vertex, magenta_ps(), &[]), Err(Error::VersionMismatch { .. })));
        assert!(matches!(b.add(0x2, ShaderKind::Pixel, passthrough_vs(), &[]), Err(Error::VersionMismatch { .. })));
        assert!(matches!(b.replace_in_place(0xaaaa_aaaa, passthrough_vs(), &[]), Err(Error::VersionMismatch { .. })));
        // missing replace target
        assert!(matches!(b.replace_in_place(0x3, magenta_ps(), &[]), Err(Error::MissingTarget(0x3))));
        // collisions: this store, another store
        assert!(matches!(b.add(0xaaaa_aaaa, ShaderKind::Pixel, magenta_ps(), &[]), Err(Error::IdCollision { store: None, .. })));
        assert!(matches!(
            b.add(0x1111_1111, ShaderKind::Pixel, magenta_ps(), &[&other]),
            Err(Error::IdCollision { store: Some(0), .. })
        ));
        let shared = small_store(&[0xaaaa_aaaa]);
        assert!(matches!(
            b.replace_in_place(0xaaaa_aaaa, magenta_ps(), &[&other, &shared]),
            Err(Error::IdCollision { store: Some(1), .. })
        ));
        // the engine's id table: every store loaded together must stay under 0x1200 records
        let crowd_ids: Vec<u32> = (0..(TABLE_SLOTS as u32 - 3)).map(|i| 0x5000_0000 + i).collect();
        let crowd = small_store(&crowd_ids);
        assert!(matches!(
            b.add(0x2, ShaderKind::Pixel, magenta_ps(), &[&crowd]),
            Err(Error::TooManyRecords { total }) if total == TABLE_SLOTS
        ));
        // …and one fewer is fine
        let crowd = small_store(&crowd_ids[1..]);
        b.add(0x2, ShaderKind::Pixel, magenta_ps(), &[&crowd]).unwrap();
        // nothing refused above changed the store
        assert_eq!(b.entries().iter().map(|e| e.id).collect::<Vec<_>>(), [0xaaaa_aaaa, 0xbbbb_bbbb, 0x2]);
    }

    /// Game-gated: built by the `retail` feature, reads the stores beside the `vz.wad` named by the
    /// repo-root `.mercs2-local.toml`, and fails if they are absent.
    #[cfg(feature = "retail")]
    mod retail {
        use super::*;

        #[test]
        fn parse_real_store_if_present() {
            let store = retail_store_for_test("shader3.bin");
            let vs = store.records.iter().filter(|r| matches!(r.kind, ShaderKind::Vertex)).count();
            let ps = store.records.len() - vs;
            assert_eq!(store.records.len(), 556);
            assert_eq!((vs, ps), (151, 405));
            // every VS blob must carry a CTAB and parse; and every record (VS+PS) must disassemble +
            // classify (the recovery surface must not choke on any real retail blob).
            for r in &store.records {
                let blob = store.blob(r);
                let (_c, consts) = parse_ctab(blob).unwrap();
                let asm = disassemble(blob).unwrap();
                assert!(asm.len() >= 2, "a real shader disassembles to >=1 instruction");
                let _role = classify_role(&r.kind, &consts);
                if matches!(r.kind, ShaderKind::Vertex) {
                    assert_eq!(u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]), VS_3_0);
                }
            }
        }

        #[test]
        fn store_ids_are_present_in_the_retail_stores() {
            let (high, low) = (retail_store_for_test("shader3.bin"), retail_store_for_test("shader3Low.bin"));
            let has = |s: &Store, id: u32, kind: ShaderKind| s.records.iter().any(|r| r.id == id && r.kind == kind);
            for (stem, kind) in [("PgMeshVP", ShaderKind::Vertex), ("PgSkyFP", ShaderKind::Pixel), ("PgBlurHFP", ShaderKind::Pixel)] {
                assert!(has(&high, store_id(stem, false).unwrap(), kind), "{stem} in shader3.bin");
                assert!(has(&low, store_id(stem, true).unwrap(), kind), "{stem} in shader3Low.bin");
                assert!(!has(&high, store_id(stem, true).unwrap(), kind), "{stem} Low id not in shader3.bin");
            }
        }

        #[test]
        fn retail_stores_rewrite_byte_identically() {
            for name in RETAIL_STORES {
                let store = retail_store_for_test(name);
                let out = StoreBuilder::from_store(&store).to_bytes().unwrap();
                assert!(out == store.bytes, "{name}: rewrite differs from retail");
            }
        }

        #[test]
        fn replace_changes_exactly_one_record() {
            let store = retail_store_for_test("shader3.bin");
            let id = store_id("PgSkyFP", false).unwrap();
            let mut b = StoreBuilder::from_store(&store);
            b.replace_in_place(id, magenta_ps(), &[]).unwrap();
            let out = Store::parse(b.to_bytes().unwrap()).unwrap();
            assert_eq!(out.records.len(), store.records.len());
            let mut changed = 0;
            for (old, new) in store.records.iter().zip(&out.records) {
                assert_eq!((old.id, old.kind), (new.id, new.kind), "record order and kinds are kept");
                assert_eq!(new.blob_off as usize % 16, 0);
                if new.id == id {
                    assert_eq!(out.blob(new), &magenta_ps()[..]);
                    changed += 1;
                } else {
                    assert_eq!(out.blob(new), store.blob(old), "record 0x{:08x} must be untouched", old.id);
                }
            }
            assert_eq!(changed, 1);
            assert_eq!(out.bytes.len() % 16, 0);
        }

        #[test]
        fn add_appends_a_record() {
            let store = retail_store_for_test("shader3.bin");
            let id = store_id("SmMagentaFP", false).unwrap();
            let mut b = StoreBuilder::from_store(&store);
            b.add(id, ShaderKind::Pixel, magenta_ps(), &[]).unwrap();
            let out = Store::parse(b.to_bytes().unwrap()).unwrap();
            assert_eq!(out.records.len(), store.records.len() + 1);
            let last = out.records.last().unwrap();
            assert_eq!((last.id, last.kind), (id, ShaderKind::Pixel));
            assert_eq!(out.blob(last), &magenta_ps()[..]);
            let prev = &out.records[out.records.len() - 2];
            assert_eq!(
                last.blob_off as usize,
                ((prev.blob_off + prev.blob_size) as usize).div_ceil(16) * 16,
                "appended at the next 16-byte boundary"
            );
            for (old, new) in store.records.iter().zip(&out.records) {
                assert_eq!((old.id, old.kind), (new.id, new.kind));
                assert_eq!(out.blob(new), store.blob(old));
            }
            assert_eq!(out.bytes.len() % 16, 0);
        }
    }
}
