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
// **The identification wall (honest):** the record `id` is NOT `FNV(name)` — proven by hash inversion
// against 344 known names (`density_upgrade_state.md`), and the store carries no name column. The
// `.rdata` registry `FUN_0084f130` names the shaders (`PgSkyFP`, `PgDecalVP`, …) but binds them to
// `.sho` blobs by FNV handle, and the `id → blob` map is a stripped `%0x1200` table. So mapping
// "record N == PgSkyFP" is **confirm-live** (find the `%0x1200` reader, or bp the loader). What IS
// static is every blob's intact CTAB → constant **names + register layout**, which lets us classify a
// record by its constant *signature* (the only static handle on identity) and disassemble its body.

/// SM3.0 register file (the merged 5-bit `D3DSHADER_PARAM_REGISTER_TYPE`). Only the members the
/// disassembler labels are named; the rest fall through to a numbered form.
fn regtype_name(ty: u32) -> &'static str {
    match ty {
        0 => "r",      // temp
        1 => "v",      // input
        2 => "c",      // float const
        3 => "t",      // texture coord (ps) / addr (vs a0)
        4 => "oPos",   // rasterizer out (vs)
        5 => "oD",     // attribute out (vs color)
        6 => "o",      // output (vs texcoord out / ps color pre-SM3 quirk)
        7 => "i",      // int const
        8 => "oC",     // colour out (ps)
        9 => "oDepth", // depth out (ps)
        10 => "s",     // sampler
        15 => "aL",    // loop counter / label
        16 => "p",     // predicate
        17 => "b",     // bool const
        _ => "x",
    }
}

const SWIZ: [char; 4] = ['x', 'y', 'z', 'w'];

/// Format a destination operand token (`reg + write mask`).
fn fmt_dst(tok: u32) -> String {
    let base = format!("{}{}", regtype_name(regtype(tok)), regnum(tok));
    let mask = (tok >> 16) & 0xf;
    if mask == 0xf || mask == 0 {
        return base; // full write (or a form with no mask, e.g. dcl dst)
    }
    let mut s = String::from(".");
    for (i, c) in SWIZ.iter().enumerate() {
        if mask & (1 << i) != 0 {
            s.push(*c);
        }
    }
    base + &s
}

/// Format a source operand token (`reg + swizzle + modifier`), noting relative addressing.
fn fmt_src(tok: u32) -> String {
    let mut base = format!("{}{}", regtype_name(regtype(tok)), regnum(tok));
    if tok & (1 << 13) != 0 {
        base.push_str("[aL]"); // relative-addressed (const/palette array)
    }
    // swizzle: 2 bits per component in bits 16..23.
    let sw = (tok >> 16) & 0xff;
    let swz: Vec<char> = (0..4).map(|i| SWIZ[((sw >> (i * 2)) & 3) as usize]).collect();
    let swizzle = if swz == ['x', 'y', 'z', 'w'] {
        String::new()
    } else if swz[0] == swz[1] && swz[1] == swz[2] && swz[2] == swz[3] {
        format!(".{}", swz[0]) // replicate (e.g. .x)
    } else {
        format!(".{}{}{}{}", swz[0], swz[1], swz[2], swz[3])
    };
    // source modifier in bits 24..27 (0 none, 1 negate, common subset).
    let modn = (tok >> 24) & 0xf;
    let (pre, post) = match modn {
        1 => ("-", ""),   // negate
        2 => ("", "_bias"),
        3 => ("", "_x2"),
        11 => ("", "_abs"),
        _ => ("", ""),
    };
    format!("{pre}{base}{swizzle}{post}")
}

/// Human-readable mnemonic for an SM3 opcode (the subset the retail Pg* shaders use; others print
/// as `op_0xNN`). Enough to read a sky/decal/mesh body and hand-translate it to WGSL.
fn opcode_name(op: u16) -> &'static str {
    match op {
        0x00 => "nop", 0x01 => "mov", 0x02 => "add", 0x03 => "sub", 0x04 => "mad",
        0x05 => "mul", 0x06 => "rcp", 0x07 => "rsq", 0x08 => "dp3", 0x09 => "dp4",
        0x0a => "min", 0x0b => "max", 0x0c => "slt", 0x0d => "sge", 0x0e => "exp",
        0x0f => "log", 0x10 => "lit", 0x11 => "dst", 0x12 => "lrp", 0x13 => "frc",
        0x14 => "m4x4", 0x15 => "m4x3", 0x16 => "m3x4", 0x17 => "m3x3", 0x18 => "m3x2",
        0x19 => "call", 0x1a => "callnz", 0x1b => "loop", 0x1c => "ret", 0x1d => "endloop",
        0x1e => "label", 0x1f => "dcl", 0x20 => "pow", 0x21 => "crs", 0x22 => "sgn",
        0x23 => "abs", 0x24 => "nrm", 0x25 => "sincos", 0x26 => "rep", 0x27 => "endrep",
        0x28 => "if", 0x29 => "ifc", 0x2a => "else", 0x2b => "endif", 0x2c => "break",
        0x2d => "breakc", 0x2e => "mova", 0x2f => "defb", 0x30 => "defi",
        0x40 => "texcoord", 0x41 => "texkill", 0x42 => "texld", 0x43 => "texbem",
        0x48 => "texm3x3pad", 0x4a => "texm3x3tex", 0x51 => "def", 0x58 => "cmp",
        0x5a => "bem", 0x5b => "dp2add", 0x5c => "dsx", 0x5d => "dsy", 0x5e => "texldd",
        0x5f => "setp", 0x60 => "texldl", 0x61 => "breakp",
        _ => "op",
    }
}

/// Disassemble one `vs_3_0`/`ps_3_0` blob to readable SM3 assembly (one instruction per line). This
/// is the recovery surface the WGSL translation reads off; it reuses the WIP token walker
/// ([`walk`]) so it stays in lock-step with the splice's structural model. `dcl`/`def` operands are
/// printed raw (semantic / immediate), executable operands are decoded (reg + mask/swizzle/mod).
pub fn disassemble(blob: &[u8]) -> Result<Vec<String>, Error> {
    if blob.len() < 4 || blob.len() % 4 != 0 {
        return Err(Error::Structure("blob length not a whole number of tokens"));
    }
    let tokens: Vec<u32> = blob
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let version = tokens[0];
    let kind = match version {
        VS_3_0 => "vs_3_0",
        PS_3_0 => "ps_3_0",
        _ => return Err(Error::NotVertexShader),
    };
    let (_he, instrs) = walk(&tokens)?;
    let mut out = vec![format!("{kind}")];
    for ins in &instrs {
        let name = opcode_name(ins.opcode);
        let mnem = if name == "op" { format!("op_0x{:02x}", ins.opcode) } else { name.to_string() };
        if matches!(ins.opcode, OP_DEF | OP_DEFI | OP_DEFB) {
            // def cN, f,f,f,f — dst reg then 4 raw immediate tokens.
            let dst = fmt_dst(tokens[ins.at + 1]);
            let mut vals = Vec::new();
            for p in 1..ins.nparams {
                let raw = tokens[ins.at + 1 + p];
                vals.push(if ins.opcode == OP_DEF {
                    format!("{:.4}", f32::from_bits(raw))
                } else {
                    format!("0x{raw:08x}")
                });
            }
            out.push(format!("{mnem} {dst}, {}", vals.join(", ")));
        } else if ins.opcode == OP_DCL {
            // dcl usage_reg — first token is the usage/semantic, second the register.
            let usage = tokens[ins.at + 1];
            let reg = fmt_dst(tokens[ins.at + 2]);
            let u = usage & 0xf;
            let uidx = (usage >> 16) & 0xf;
            let uname = match u {
                0 => "position", 1 => "blendweight", 2 => "blendindices", 3 => "normal",
                4 => "psize", 5 => "texcoord", 6 => "tangent", 7 => "binormal",
                10 => "color", _ => "usage",
            };
            out.push(format!("dcl_{uname}{uidx} {reg}"));
        } else {
            let mut ops: Vec<String> = Vec::new();
            for p in 0..ins.nparams {
                let tok = tokens[ins.at + 1 + p];
                ops.push(if p == 0 { fmt_dst(tok) } else { fmt_src(tok) });
            }
            out.push(format!("{mnem} {}", ops.join(", ")));
        }
    }
    Ok(out)
}

/// A recovered role bucket for a shader record, assigned from its CTAB constant **signature** — the
/// only static identity handle (record `id != FNV(name)`; see the module note). These are *candidate*
/// classes, not proven `Pg*` names: e.g. a VS whose only constants are the view/projection block and
/// carries no `objectData`/`BoneMatrixArray` is a **fullscreen/far-plane** shader — the class the sky,
/// sun, moon, cloud and post-process shaders all fall into — so it narrows the search but does not by
/// itself say "this is `PgSkyFP`". Mapping a bucket member to its exact name is confirm-live.
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
}
