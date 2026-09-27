//! `sm3asm` — a Direct3D 9 **Shader Model 3** (`vs_3_0` / `ps_3_0`) assembler and the matching exact
//! disassembler, including the `CTAB` constant-table comment block.
//!
//! The two directions are one codec: [`disassemble`] turns a token stream into text that
//! [`assemble`] turns back into the **identical bytes**. That round trip is proven over every
//! record of every retail shader store (`shader3.bin`, `shader3Low.bin`, `shaderVT*.bin`,
//! `shaderR2VB*.bin`) by the game-gated tests at the bottom of this file. Anything the disassembler
//! cannot express exactly is a hard error, never a lossy rendering.
//!
//! # Token stream (D3D9 SM3)
//! ```text
//! version  fffe0300 (vs_3_0) | ffff0300 (ps_3_0)
//! comment  [0xfffe | len<<16] [len words]            (first word = FourCC, e.g. "CTAB", "DBUG")
//! instr    [opcode | control<<16 | nparams<<24 | predicated<<28] [nparams param tokens]
//! end      0000ffff
//! ```
//! A parameter token: register number bits 0-10, register type split over bits 28-30 (low 3 bits)
//! and 11-12 (high 2 bits), relative addressing bit 13 (the address token follows), bit 31 always
//! set. Destinations carry the write mask in bits 16-19, result modifiers (`_sat`=1, `_pp`=2,
//! `_centroid`=4) in bits 20-23 and the shift scale in bits 24-27. Sources carry the swizzle in
//! bits 16-23 and the source modifier in bits 24-27. A predicated instruction's predicate token
//! follows its destination token (D3D9 token-format rule; no retail shader is predicated).
//!
//! # CTAB layout (as the retail HLSL compiler writes it)
//! `[header 28 B][ConstantInfo × n, 20 B each]`, then for each constant in order: its name, then its
//! type (a struct's member names and member types first, then the member array, then the struct's
//! own TypeInfo), then the target string, then the creator string. Every string is emitted once
//! (a repeat reuses the first offset); every 16-byte TypeInfo is emitted once (a repeat reuses the
//! first offset); an array (member array or TypeInfo) is aligned to 4 bytes with `0xab` filler, and
//! the block ends on a 4-byte boundary with `0xab` filler. This rule reproduces all 1,023 retail
//! CTABs byte for byte.
//!
//! # Text syntax
//! One statement per line (`;` also separates statements; `//` starts a comment):
//! ```text
//! ps_3_0
//! .ctab creator="Microsoft (R) HLSL Shader Compiler 9.19.949.2111" target="ps_3_0" flags=0x20000110
//! .const diffuseMap s0 1 reserved=2 : object sampler2d 1x1 [1]
//! .const pointLights c36 24 : struct void 1x12 [8] { position: vector float 1x4 [1], color: vector float 1x4 [1], params: vector float 1x4 [1] }
//! .endctab
//! .comment 4442554701000000                  // any other comment block: its payload bytes in hex
//! dcl_texcoord1_centroid v1.xy
//! dcl_2d s0
//! def c0, 1, 0, 1, 1
//! texldp_pp r0, v1, s0
//! mad_sat oC0.xyz, -r0_abs.x, c0.yxzw, 1-r1.xyz
//! if_lt r0.x, c0.y
//! endif
//! ```
//! Sources: `-` negate, `1-` complement, `!` not, `_abs`/`_bias`/`_bx2`/`_x2`/`_dz`/`_dw` suffixes,
//! swizzles with trailing repeats dropped (`.xyz` = `.xyzz`). Relative addressing is written
//! `c10[a0.x]` / `o3[aL]`. Instruction suffixes: `_sat`, `_pp`, `_centroid`, shifts `_x2`…`_d8`,
//! comparisons `if_lt`/`break_ge`/`setp_eq`; `texldp`/`texldb` are the projected/biased `texld`.
//! A predicated instruction is prefixed `(p0.x)` or `(!p0.x)`.
//! When a source has no `.ctab` block the assembler emits a CTAB with creator
//! [`DEFAULT_CREATOR`], the profile as target, flags 0 and no constants, right after the version
//! token.

use std::fmt;

pub const VS_3_0: u32 = 0xfffe_0300;
pub const PS_3_0: u32 = 0xffff_0300;
pub const END_TOKEN: u32 = 0x0000_ffff;
const OP_COMMENT: u32 = 0xfffe;
const OP_DCL: u16 = 0x1f;
const OP_DEF: u16 = 0x51;
const OP_DEFI: u16 = 0x30;
const OP_DEFB: u16 = 0x2f;
const OP_TEXLD: u16 = 0x42;
const PARAM_BIT: u32 = 0x8000_0000;
const REL_BIT: u32 = 1 << 13;
/// The FourCC that opens a constant-table comment block.
pub const CTAB_FOURCC: [u8; 4] = *b"CTAB";
/// Creator string of a CTAB this assembler writes when the source has no `.ctab creator=`.
pub const DEFAULT_CREATOR: &str = "mercs2_formats sm3asm";
/// Size of the CTAB header (`D3DXSHADER_CONSTANTTABLE`).
const CTAB_HEADER: usize = 28;
/// Longest comment block the 15-bit length field can describe, in words.
const MAX_COMMENT_WORDS: usize = 0x7fff;

/// Why a text or bytecode could not be converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sm3Error {
    /// The assembly source is wrong at this 1-based line.
    Source { line: usize, msg: String },
    /// The bytecode is malformed, or holds something the text form cannot express exactly, at this
    /// token index.
    Bytecode { token: usize, msg: String },
}

impl fmt::Display for Sm3Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sm3Error::Source { line, msg } => write!(f, "line {line}: {msg}"),
            Sm3Error::Bytecode { token, msg } => write!(f, "token {token}: {msg}"),
        }
    }
}
impl std::error::Error for Sm3Error {}

fn src_err(line: usize, msg: impl Into<String>) -> Sm3Error {
    Sm3Error::Source { line, msg: msg.into() }
}
fn tok_err(token: usize, msg: impl Into<String>) -> Sm3Error {
    Sm3Error::Bytecode { token, msg: msg.into() }
}

/// Split little-endian bytes into tokens. The length must be a whole number of tokens.
pub fn to_tokens(blob: &[u8]) -> Result<Vec<u32>, Sm3Error> {
    if !blob.len().is_multiple_of(4) {
        return Err(tok_err(blob.len() / 4, "blob length is not a whole number of 4-byte tokens"));
    }
    Ok(blob.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

/// Join tokens into little-endian bytes.
pub fn to_bytes(tokens: &[u32]) -> Vec<u8> {
    tokens.iter().flat_map(|t| t.to_le_bytes()).collect()
}

/// The profile name of a version token.
pub fn profile_name(version: u32) -> Option<&'static str> {
    match version {
        VS_3_0 => Some("vs_3_0"),
        PS_3_0 => Some("ps_3_0"),
        _ => None,
    }
}

/// The version token of a profile name.
pub fn profile_version(name: &str) -> Option<u32> {
    match name {
        "vs_3_0" => Some(VS_3_0),
        "ps_3_0" => Some(PS_3_0),
        _ => None,
    }
}

// ── opcodes ──────────────────────────────────────────────────────────────────────────────────────

/// One SM3 opcode: its text name, whether it writes a destination, and how many sources it reads.
/// `cmp` marks the three opcodes whose control field is a comparison (`if_lt`, `break_ge`, `setp_eq`).
struct Op {
    code: u16,
    name: &'static str,
    dst: bool,
    srcs: usize,
    cmp: bool,
}

const fn op(code: u16, name: &'static str, dst: bool, srcs: usize) -> Op {
    Op { code, name, dst, srcs, cmp: false }
}
const fn cmp_op(code: u16, name: &'static str, dst: bool, srcs: usize) -> Op {
    Op { code, name, dst, srcs, cmp: true }
}

/// Every opcode valid in `vs_3_0` / `ps_3_0` apart from `dcl`/`def`/`defi`/`defb`, which have their
/// own operand forms. The ps_1_x texture-addressing opcodes (0x40-0x57 except `texkill`/`texld`)
/// are not SM3 and are absent on purpose.
const OPS: &[Op] = &[
    op(0x00, "nop", false, 0),
    op(0x01, "mov", true, 1),
    op(0x02, "add", true, 2),
    op(0x03, "sub", true, 2),
    op(0x04, "mad", true, 3),
    op(0x05, "mul", true, 2),
    op(0x06, "rcp", true, 1),
    op(0x07, "rsq", true, 1),
    op(0x08, "dp3", true, 2),
    op(0x09, "dp4", true, 2),
    op(0x0a, "min", true, 2),
    op(0x0b, "max", true, 2),
    op(0x0c, "slt", true, 2),
    op(0x0d, "sge", true, 2),
    op(0x0e, "exp", true, 1),
    op(0x0f, "log", true, 1),
    op(0x10, "lit", true, 1),
    op(0x11, "dst", true, 2),
    op(0x12, "lrp", true, 3),
    op(0x13, "frc", true, 1),
    op(0x14, "m4x4", true, 2),
    op(0x15, "m4x3", true, 2),
    op(0x16, "m3x4", true, 2),
    op(0x17, "m3x3", true, 2),
    op(0x18, "m3x2", true, 2),
    op(0x19, "call", false, 1),
    op(0x1a, "callnz", false, 2),
    op(0x1b, "loop", false, 2),
    op(0x1c, "ret", false, 0),
    op(0x1d, "endloop", false, 0),
    op(0x1e, "label", false, 1),
    op(0x20, "pow", true, 2),
    op(0x21, "crs", true, 2),
    op(0x22, "sgn", true, 3),
    op(0x23, "abs", true, 1),
    op(0x24, "nrm", true, 1),
    op(0x25, "sincos", true, 1),
    op(0x26, "rep", false, 1),
    op(0x27, "endrep", false, 0),
    op(0x28, "if", false, 1),
    cmp_op(0x29, "if", false, 2),
    op(0x2a, "else", false, 0),
    op(0x2b, "endif", false, 0),
    op(0x2c, "break", false, 0),
    cmp_op(0x2d, "break", false, 2),
    op(0x2e, "mova", true, 1),
    op(0x41, "texkill", true, 0),
    op(0x42, "texld", true, 2),
    op(0x58, "cmp", true, 3),
    op(0x5a, "dp2add", true, 3),
    op(0x5b, "dsx", true, 1),
    op(0x5c, "dsy", true, 1),
    op(0x5d, "texldd", true, 4),
    cmp_op(0x5e, "setp", true, 2),
    op(0x5f, "texldl", true, 2),
    op(0x60, "breakp", false, 1),
];

fn op_by_code(code: u16) -> Option<&'static Op> {
    OPS.iter().find(|o| o.code == code)
}
fn op_by_name(name: &str, cmp: bool) -> Option<&'static Op> {
    OPS.iter().find(|o| o.name == name && o.cmp == cmp)
}

/// `D3DSHADER_COMPARISON`, 1..=6.
const COMPARISONS: [&str; 6] = ["gt", "eq", "ge", "lt", "ne", "le"];
/// `texld` control values 0..=2: plain, projected, biased.
const TEXLD_NAMES: [&str; 3] = ["texld", "texldp", "texldb"];

/// `D3DDECLUSAGE`, 0..=13.
const USAGES: [&str; 14] = [
    "position", "blendweight", "blendindices", "normal", "psize", "texcoord", "tangent",
    "binormal", "tessfactor", "positiont", "color", "fog", "depth", "sample",
];
/// `D3DSAMPLER_TEXTURE_TYPE` values a sampler `dcl` may name.
const SAMPLER_TYPES: [(u32, &str); 3] = [(2, "2d"), (3, "cube"), (4, "volume")];

// ── registers ────────────────────────────────────────────────────────────────────────────────────

const RT_SAMPLER: u32 = 10;
const RT_MISC: u32 = 17;

fn regtype(tok: u32) -> u32 {
    ((tok >> 28) & 0x7) | ((tok >> 8) & 0x18)
}

fn encode_reg(ty: u32, num: u32) -> u32 {
    ((ty & 0x7) << 28) | (((ty >> 3) & 0x3) << 11) | (num & 0x7ff)
}

/// Registers whose name carries no number (the number field must be the listed value).
const NAMED_REGS: [(&str, u32, u32); 7] = [
    ("oPos", 4, 0),
    ("oFog", 4, 1),
    ("oPts", 4, 2),
    ("oDepth", 9, 0),
    ("vPos", RT_MISC, 0),
    ("vFace", RT_MISC, 1),
    ("aL", 15, 0),
];
/// Numbered register-file prefixes, longest first so `oD`/`oC` win over `o`.
const NUMBERED_REGS: [(&str, u32); 12] = [
    ("oD", 5),
    ("oC", 8),
    ("o", 6),
    ("r", 0),
    ("v", 1),
    ("c", 2),
    ("a", 3),
    ("i", 7),
    ("s", RT_SAMPLER),
    ("b", 14),
    ("l", 18),
    ("p", 19),
];

fn reg_name(ty: u32, num: u32) -> Result<String, String> {
    if let Some((name, _, _)) = NAMED_REGS.iter().find(|(_, t, n)| *t == ty && *n == num) {
        return Ok((*name).to_string());
    }
    if NAMED_REGS.iter().any(|(_, t, _)| *t == ty) {
        return Err(format!("register type {ty} has no register number {num}"));
    }
    match NUMBERED_REGS.iter().find(|(_, t)| *t == ty) {
        Some((prefix, _)) => Ok(format!("{prefix}{num}")),
        None => Err(format!("register type {ty} is not a Shader Model 3 register")),
    }
}

/// Parse a register name at the start of `s`; returns (type, number, rest).
fn parse_reg(s: &str) -> Result<(u32, u32, &str), String> {
    for (name, ty, num) in NAMED_REGS {
        if let Some(rest) = s.strip_prefix(name) {
            if !rest.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                return Ok((ty, num, rest));
            }
        }
    }
    for (prefix, ty) in NUMBERED_REGS {
        if let Some(rest) = s.strip_prefix(prefix) {
            let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            if digits == 0 {
                continue;
            }
            let num: u32 = rest[..digits].parse().map_err(|_| format!("bad register number in {s:?}"))?;
            if num > 0x7ff {
                return Err(format!("register number {num} exceeds 2047"));
            }
            return Ok((ty, num, &rest[digits..]));
        }
    }
    Err(format!("unknown register {s:?}"))
}

const COMPONENTS: [char; 4] = ['x', 'y', 'z', 'w'];

/// A swizzle in its shortest exact text: identity prints nothing; trailing repeats of the last
/// component are dropped (`.xyzz` → `.xyz`, `.xxxx` → `.x`), which [`parse_swizzle`] restores.
fn swizzle_text(sw: u32) -> String {
    let c: Vec<u32> = (0..4).map(|i| (sw >> (2 * i)) & 3).collect();
    if c == [0, 1, 2, 3] {
        return String::new();
    }
    let mut n = 4;
    while n > 1 && c[n - 1] == c[n - 2] {
        n -= 1;
    }
    let mut s = String::from(".");
    for &k in &c[..n] {
        s.push(COMPONENTS[k as usize]);
    }
    s
}

fn parse_swizzle(s: &str) -> Result<u32, String> {
    let comps: Vec<u32> = s
        .chars()
        .map(|ch| COMPONENTS.iter().position(|&c| c == ch).map(|p| p as u32))
        .collect::<Option<_>>()
        .ok_or_else(|| format!("bad swizzle .{s}"))?;
    if comps.is_empty() || comps.len() > 4 {
        return Err(format!("bad swizzle .{s}"));
    }
    let last = *comps.last().unwrap_or(&0);
    Ok((0..4).fold(0, |acc, i| acc | (comps.get(i).copied().unwrap_or(last) << (2 * i))))
}

fn mask_text(mask: u32) -> Result<String, String> {
    match mask {
        0 => Err("destination write mask is empty".into()),
        0xf => Ok(String::new()),
        m => {
            let mut s = String::from(".");
            for (i, c) in COMPONENTS.iter().enumerate() {
                if m & (1 << i) != 0 {
                    s.push(*c);
                }
            }
            Ok(s)
        }
    }
}

fn parse_mask(s: &str) -> Result<u32, String> {
    let mut mask = 0u32;
    let mut last: i32 = -1;
    for ch in s.chars() {
        let p = COMPONENTS.iter().position(|&c| c == ch).ok_or_else(|| format!("bad write mask .{s}"))? as i32;
        if p <= last {
            return Err(format!("write mask .{s} must list components once, in xyzw order"));
        }
        last = p;
        mask |= 1 << p;
    }
    if mask == 0 {
        return Err("empty write mask".into());
    }
    Ok(mask)
}

// ── operand disassembly ──────────────────────────────────────────────────────────────────────────

/// Reads parameter tokens of one instruction; `base` is the token index of the first parameter.
struct Params<'a> {
    toks: &'a [u32],
    base: usize,
    k: usize,
}

impl Params<'_> {
    fn next(&mut self, what: &str) -> Result<u32, Sm3Error> {
        let t = *self
            .toks
            .get(self.k)
            .ok_or_else(|| tok_err(self.base + self.k, format!("instruction is missing its {what} token")))?;
        if t & PARAM_BIT == 0 {
            return Err(tok_err(self.base + self.k, format!("{what} token 0x{t:08x} lacks bit 31")));
        }
        self.k += 1;
        Ok(t)
    }
    fn at(&self) -> usize {
        self.base + self.k
    }
}

/// The relative-address token after a register with bit 13: `a0.x` or `aL`.
fn dis_rel(p: &mut Params) -> Result<String, Sm3Error> {
    let at = p.at();
    let t = p.next("relative-address")?;
    if t & (REL_BIT | 0xc000 | 0x0f00_0000) != 0 {
        return Err(tok_err(at, format!("relative-address token 0x{t:08x} carries bits the text cannot express")));
    }
    let name = reg_name(regtype(t), t & 0x7ff).map_err(|m| tok_err(at, m))?;
    Ok(format!("{name}{}", swizzle_text((t >> 16) & 0xff)))
}

/// A register (+ its relative address) as `c10[a0.x]`.
fn dis_reg(p: &mut Params, t: u32, at: usize) -> Result<String, Sm3Error> {
    let mut s = reg_name(regtype(t), t & 0x7ff).map_err(|m| tok_err(at, m))?;
    if t & REL_BIT != 0 {
        s.push('[');
        s.push_str(&dis_rel(p)?);
        s.push(']');
    }
    Ok(s)
}

fn dis_src(p: &mut Params) -> Result<String, Sm3Error> {
    let at = p.at();
    let t = p.next("source")?;
    if t & 0xc000 != 0 {
        return Err(tok_err(at, format!("source token 0x{t:08x} sets reserved bits 14-15")));
    }
    let reg = dis_reg(p, t, at)?;
    let (pre, post) = match (t >> 24) & 0xf {
        0 => ("", ""),
        1 => ("-", ""),
        2 => ("", "_bias"),
        3 => ("-", "_bias"),
        4 => ("", "_bx2"),
        5 => ("-", "_bx2"),
        6 => ("1-", ""),
        7 => ("", "_x2"),
        8 => ("-", "_x2"),
        9 => ("", "_dz"),
        10 => ("", "_dw"),
        11 => ("", "_abs"),
        12 => ("-", "_abs"),
        13 => ("!", ""),
        m => return Err(tok_err(at, format!("unknown source modifier {m}"))),
    };
    Ok(format!("{pre}{reg}{post}{}", swizzle_text((t >> 16) & 0xff)))
}

/// A destination: its text, plus the instruction suffixes its modifiers and shift print as.
fn dis_dst(p: &mut Params) -> Result<(String, String), Sm3Error> {
    let at = p.at();
    let t = p.next("destination")?;
    if t & 0xc000 != 0 {
        return Err(tok_err(at, format!("destination token 0x{t:08x} sets reserved bits 14-15")));
    }
    let reg = dis_reg(p, t, at)?;
    let mask = mask_text((t >> 16) & 0xf).map_err(|m| tok_err(at, m))?;
    let mut suffix = String::new();
    match (t >> 24) & 0xf {
        0 => {}
        1 => suffix.push_str("_x2"),
        2 => suffix.push_str("_x4"),
        3 => suffix.push_str("_x8"),
        13 => suffix.push_str("_d8"),
        14 => suffix.push_str("_d4"),
        15 => suffix.push_str("_d2"),
        s => return Err(tok_err(at, format!("unknown destination shift {s}"))),
    }
    let mods = (t >> 20) & 0xf;
    if mods & 0x8 != 0 {
        return Err(tok_err(at, format!("unknown destination modifier bits 0x{mods:x}")));
    }
    for (bit, name) in [(1, "_sat"), (2, "_pp"), (4, "_centroid")] {
        if mods & bit != 0 {
            suffix.push_str(name);
        }
    }
    Ok((format!("{reg}{mask}"), suffix))
}

/// Shortest decimal that parses back to exactly these bits. Non-finite values have no exact decimal
/// form, so they are refused.
fn float_text(bits: u32, at: usize) -> Result<String, Sm3Error> {
    let f = f32::from_bits(bits);
    if !f.is_finite() {
        return Err(tok_err(at, format!("def immediate 0x{bits:08x} is not a finite float")));
    }
    Ok(format!("{f}"))
}

fn dis_dcl(p: &mut Params, at_usage: usize) -> Result<String, Sm3Error> {
    let usage = p.next("dcl usage")?;
    if usage & !(PARAM_BIT | 0x1f | (0xf << 16) | (0xf << 27)) != 0 {
        return Err(tok_err(at_usage, format!("dcl usage token 0x{usage:08x} sets reserved bits")));
    }
    let dst_type = regtype(*p.toks.get(p.k).ok_or_else(|| tok_err(p.at(), "dcl is missing its register"))?);
    let (dst, suffix) = dis_dst(p)?;
    let u = usage & 0x1f;
    let idx = (usage >> 16) & 0xf;
    let tex = (usage >> 27) & 0xf;
    if dst_type == RT_SAMPLER {
        if u != 0 || idx != 0 {
            return Err(tok_err(at_usage, "sampler dcl carries a usage"));
        }
        let name = SAMPLER_TYPES
            .iter()
            .find(|(v, _)| *v == tex)
            .map(|(_, n)| *n)
            .ok_or_else(|| tok_err(at_usage, format!("sampler dcl texture type {tex} is not 2d/cube/volume")))?;
        return Ok(format!("dcl_{name}{suffix} {dst}"));
    }
    if tex != 0 {
        return Err(tok_err(at_usage, "non-sampler dcl carries a texture type"));
    }
    if dst_type == RT_MISC && u == 0 && idx == 0 {
        return Ok(format!("dcl{suffix} {dst}"));
    }
    let uname = USAGES
        .get(u as usize)
        .ok_or_else(|| tok_err(at_usage, format!("unknown dcl usage {u}")))?;
    let index = if idx == 0 { String::new() } else { idx.to_string() };
    Ok(format!("dcl_{uname}{index}{suffix} {dst}"))
}

/// One instruction starting at `tokens[i]` (not a comment/end); returns its text.
fn dis_instr(tokens: &[u32], i: usize) -> Result<(String, usize), Sm3Error> {
    let ins = tokens[i];
    if ins & 0xe000_0000 != 0 {
        return Err(tok_err(i, format!("instruction token 0x{ins:08x} sets bits 29-31")));
    }
    let code = (ins & 0xffff) as u16;
    let control = (ins >> 16) & 0xff;
    let len = ((ins >> 24) & 0xf) as usize;
    let predicated = ins & (1 << 28) != 0;
    let end = i + 1 + len;
    if end > tokens.len() {
        return Err(tok_err(i, "instruction runs past the end of the blob"));
    }
    let mut p = Params { toks: &tokens[i + 1..end], base: i + 1, k: 0 };
    let special = matches!(code, OP_DCL | OP_DEF | OP_DEFI | OP_DEFB);
    if special && (control != 0 || predicated) {
        return Err(tok_err(i, "dcl/def carries a control value or predicate"));
    }
    let text = match code {
        OP_DCL => dis_dcl(&mut p, i + 1)?,
        OP_DEF | OP_DEFI | OP_DEFB => {
            let (dst, suffix) = dis_dst(&mut p)?;
            if !suffix.is_empty() {
                return Err(tok_err(i + 1, "def destination carries modifiers"));
            }
            let mut vals = Vec::new();
            let n = if code == OP_DEFB { 1 } else { 4 };
            for _ in 0..n {
                let at = p.at();
                let raw = *p.toks.get(p.k).ok_or_else(|| tok_err(at, "def is missing an immediate"))?;
                p.k += 1;
                vals.push(match code {
                    OP_DEF => float_text(raw, at)?,
                    OP_DEFI => (raw as i32).to_string(),
                    _ => match raw {
                        0 => "false".into(),
                        1 => "true".into(),
                        v => return Err(tok_err(at, format!("defb immediate {v} is not 0/1"))),
                    },
                });
            }
            let name = match code {
                OP_DEF => "def",
                OP_DEFI => "defi",
                _ => "defb",
            };
            format!("{name} {dst}, {}", vals.join(", "))
        }
        _ => {
            let op = op_by_code(code).ok_or_else(|| tok_err(i, format!("opcode 0x{code:02x} is not Shader Model 3")))?;
            let mut name = if op.cmp {
                let c = COMPARISONS
                    .get((control as usize).wrapping_sub(1))
                    .ok_or_else(|| tok_err(i, format!("comparison {control} is not 1..=6")))?;
                format!("{}_{c}", op.name)
            } else if code == OP_TEXLD {
                TEXLD_NAMES
                    .get(control as usize)
                    .ok_or_else(|| tok_err(i, format!("texld control {control} is not 0..=2")))?
                    .to_string()
            } else if control != 0 {
                return Err(tok_err(i, format!("{} carries control value {control}", op.name)));
            } else {
                op.name.to_string()
            };
            let mut operands = Vec::new();
            if op.dst {
                let (dst, suffix) = dis_dst(&mut p)?;
                name.push_str(&suffix);
                operands.push(dst);
            }
            let pred = if predicated { Some(dis_src(&mut p)?) } else { None };
            for _ in 0..op.srcs {
                operands.push(dis_src(&mut p)?);
            }
            let body = if operands.is_empty() { name } else { format!("{name} {}", operands.join(", ")) };
            match pred {
                Some(pr) => format!("({pr}) {body}"),
                None => body,
            }
        }
    };
    if p.k != len {
        return Err(tok_err(i, format!("instruction declares {len} parameter tokens but its operands use {}", p.k)));
    }
    Ok((text, end))
}

// ── CTAB ─────────────────────────────────────────────────────────────────────────────────────────

/// `D3DXPARAMETER_CLASS`, 0..=5.
const CLASSES: [&str; 6] = ["scalar", "vector", "matrix_rows", "matrix_columns", "object", "struct"];
/// `D3DXPARAMETER_TYPE`, 0..=19.
const PARAM_TYPES: [&str; 20] = [
    "void", "bool", "int", "float", "string", "texture", "texture1d", "texture2d", "texture3d",
    "texturecube", "sampler", "sampler1d", "sampler2d", "sampler3d", "samplercube", "pixelshader",
    "vertexshader", "pixelfragment", "vertexfragment", "unsupported",
];
/// `D3DXREGISTER_SET`, 0..=3.
const REGISTER_SETS: [char; 4] = ['b', 'i', 'c', 's'];

/// A `D3DXSHADER_TYPEINFO`, with its struct members resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtabType {
    pub class: u16,
    pub ty: u16,
    pub rows: u16,
    pub columns: u16,
    pub elements: u16,
    pub members: Vec<(String, CtabType)>,
}

/// A `D3DXSHADER_CONSTANTINFO`. `reserved` is kept verbatim: the retail compiler leaves non-zero
/// values there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtabConstant {
    pub name: String,
    pub register_set: u16,
    pub register_index: u16,
    pub register_count: u16,
    pub reserved: u16,
    pub ty: CtabType,
}

/// A decoded constant table. The header's version field is the shader's version token and is not
/// stored here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ctab {
    pub creator: String,
    pub target: String,
    pub flags: u32,
    pub constants: Vec<CtabConstant>,
}

fn rd32(b: &[u8], off: usize) -> Result<u32, String> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| format!("CTAB read at {off} past its end ({})", b.len()))
}
fn rd16(b: &[u8], off: usize) -> Result<u16, String> {
    b.get(off..off + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| format!("CTAB read at {off} past its end ({})", b.len()))
}
fn rd_cstr(b: &[u8], off: usize) -> Result<String, String> {
    let tail = b.get(off..).ok_or_else(|| format!("CTAB string offset {off} past its end"))?;
    let n = tail.iter().position(|&c| c == 0).ok_or_else(|| format!("CTAB string at {off} is unterminated"))?;
    String::from_utf8(tail[..n].to_vec()).map_err(|_| format!("CTAB string at {off} is not UTF-8"))
}

impl Ctab {
    /// Decode the bytes after the `CTAB` FourCC. `version` is the shader's version token, which the
    /// header must repeat.
    pub fn decode(b: &[u8], version: u32) -> Result<Ctab, String> {
        let size = rd32(b, 0)? as usize;
        if size != CTAB_HEADER {
            return Err(format!("CTAB header size {size}, expected {CTAB_HEADER}"));
        }
        let creator = rd_cstr(b, rd32(b, 4)? as usize)?;
        let v = rd32(b, 8)?;
        if v != version {
            return Err(format!("CTAB version 0x{v:08x} differs from the shader version 0x{version:08x}"));
        }
        let n = rd32(b, 12)? as usize;
        let info = rd32(b, 16)? as usize;
        let flags = rd32(b, 20)?;
        let target = rd_cstr(b, rd32(b, 24)? as usize)?;
        let want_info = if n == 0 { 0 } else { CTAB_HEADER };
        if info != want_info {
            return Err(format!("CTAB ConstantInfo offset {info}, expected {want_info} for {n} constants"));
        }
        let mut constants = Vec::with_capacity(n);
        for j in 0..n {
            let o = info + 20 * j;
            let default_value = rd32(b, o + 16)?;
            if default_value != 0 {
                return Err(format!("CTAB constant {j} has a default value; none of the retail shaders do and it is not supported"));
            }
            constants.push(CtabConstant {
                name: rd_cstr(b, rd32(b, o)? as usize)?,
                register_set: rd16(b, o + 4)?,
                register_index: rd16(b, o + 6)?,
                register_count: rd16(b, o + 8)?,
                reserved: rd16(b, o + 10)?,
                ty: Self::decode_type(b, rd32(b, o + 12)? as usize, 0)?,
            });
        }
        Ok(Ctab { creator, target, flags, constants })
    }

    fn decode_type(b: &[u8], off: usize, depth: usize) -> Result<CtabType, String> {
        if depth > 32 {
            return Err("CTAB struct nesting deeper than 32 (a cycle?)".into());
        }
        let nmembers = rd16(b, off + 10)? as usize;
        let member_info = rd32(b, off + 12)? as usize;
        let mut members = Vec::with_capacity(nmembers);
        for m in 0..nmembers {
            let o = member_info + 8 * m;
            let name = rd_cstr(b, rd32(b, o)? as usize)?;
            members.push((name, Self::decode_type(b, rd32(b, o + 4)? as usize, depth + 1)?));
        }
        Ok(CtabType {
            class: rd16(b, off)?,
            ty: rd16(b, off + 2)?,
            rows: rd16(b, off + 4)?,
            columns: rd16(b, off + 6)?,
            elements: rd16(b, off + 8)?,
            members,
        })
    }

    /// Encode to the bytes that follow the `CTAB` FourCC, laid out as the retail compiler does (see
    /// the module docs), padded to 4 bytes with `0xab`.
    pub fn encode(&self, version: u32) -> Vec<u8> {
        struct W {
            b: Vec<u8>,
            strings: Vec<(String, u32)>,
            types: Vec<([u8; 16], u32)>,
        }
        impl W {
            fn align(&mut self) {
                while !self.b.len().is_multiple_of(4) {
                    self.b.push(0xab);
                }
            }
            fn string(&mut self, s: &str) -> u32 {
                if let Some((_, o)) = self.strings.iter().find(|(x, _)| x == s) {
                    return *o;
                }
                let o = self.b.len() as u32;
                self.b.extend_from_slice(s.as_bytes());
                self.b.push(0);
                self.strings.push((s.to_string(), o));
                o
            }
            fn ty(&mut self, t: &CtabType) -> u32 {
                let mut member_info = 0u32;
                if !t.members.is_empty() {
                    let mut entries = Vec::with_capacity(t.members.len());
                    for (name, mt) in &t.members {
                        let n = self.string(name);
                        let o = self.ty(mt);
                        entries.push((n, o));
                    }
                    self.align();
                    member_info = self.b.len() as u32;
                    for (n, o) in entries {
                        self.b.extend_from_slice(&n.to_le_bytes());
                        self.b.extend_from_slice(&o.to_le_bytes());
                    }
                }
                let mut info = [0u8; 16];
                for (k, v) in [t.class, t.ty, t.rows, t.columns, t.elements, t.members.len() as u16]
                    .iter()
                    .enumerate()
                {
                    info[2 * k..2 * k + 2].copy_from_slice(&v.to_le_bytes());
                }
                info[12..16].copy_from_slice(&member_info.to_le_bytes());
                if let Some((_, o)) = self.types.iter().find(|(x, _)| *x == info) {
                    return *o;
                }
                self.align();
                let o = self.b.len() as u32;
                self.b.extend_from_slice(&info);
                self.types.push((info, o));
                o
            }
        }
        let n = self.constants.len();
        let mut w = W { b: vec![0; CTAB_HEADER + 20 * n], strings: Vec::new(), types: Vec::new() };
        let mut infos = Vec::with_capacity(n);
        for c in &self.constants {
            let name = w.string(&c.name);
            let ty = w.ty(&c.ty);
            infos.push((name, ty));
        }
        let target = w.string(&self.target);
        let creator = w.string(&self.creator);
        w.align();
        let header = [
            CTAB_HEADER as u32,
            creator,
            version,
            n as u32,
            if n == 0 { 0 } else { CTAB_HEADER as u32 },
            self.flags,
            target,
        ];
        for (k, v) in header.iter().enumerate() {
            w.b[4 * k..4 * k + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (j, (c, (name, ty))) in self.constants.iter().zip(infos).enumerate() {
            let o = CTAB_HEADER + 20 * j;
            w.b[o..o + 4].copy_from_slice(&name.to_le_bytes());
            for (k, v) in [c.register_set, c.register_index, c.register_count, c.reserved].iter().enumerate() {
                w.b[o + 4 + 2 * k..o + 6 + 2 * k].copy_from_slice(&v.to_le_bytes());
            }
            w.b[o + 12..o + 16].copy_from_slice(&ty.to_le_bytes());
            // DefaultValue stays 0.
        }
        w.b
    }

    /// The comment-block tokens carrying this table: `[0xfffe | len<<16]["CTAB"][table words]`.
    pub fn comment_tokens(&self, version: u32) -> Result<Vec<u32>, String> {
        let bytes = self.encode(version);
        let words = 1 + bytes.len() / 4;
        if words > MAX_COMMENT_WORDS {
            return Err(format!("CTAB needs {words} comment words, the limit is {MAX_COMMENT_WORDS}"));
        }
        let mut t = vec![OP_COMMENT | ((words as u32) << 16), u32::from_le_bytes(CTAB_FOURCC)];
        t.extend(bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])));
        Ok(t)
    }

    /// The `.ctab` … `.endctab` text block.
    fn text(&self) -> Result<Vec<String>, String> {
        let mut out = vec![format!(
            ".ctab creator={} target={} flags=0x{:08x}",
            quote(&self.creator)?,
            quote(&self.target)?,
            self.flags
        )];
        for c in &self.constants {
            check_name(&c.name)?;
            let set = REGISTER_SETS
                .get(c.register_set as usize)
                .ok_or_else(|| format!("constant {} has register set {}", c.name, c.register_set))?;
            let reserved = if c.reserved == 0 { String::new() } else { format!(" reserved={}", c.reserved) };
            out.push(format!(
                ".const {} {set}{} {}{reserved} : {}",
                c.name,
                c.register_index,
                c.register_count,
                type_text(&c.ty)?
            ));
        }
        out.push(".endctab".into());
        Ok(out)
    }
}

fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
        return Err(format!("CTAB name {name:?} is not an identifier the text form can carry"));
    }
    Ok(())
}

fn quote(s: &str) -> Result<String, String> {
    let mut q = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => q.push_str("\\\""),
            '\\' => q.push_str("\\\\"),
            c if (' '..='~').contains(&c) => q.push(c),
            c => return Err(format!("CTAB string {s:?} holds non-printable {c:?}")),
        }
    }
    q.push('"');
    Ok(q)
}

fn type_text(t: &CtabType) -> Result<String, String> {
    let class = CLASSES.get(t.class as usize).ok_or_else(|| format!("CTAB type class {}", t.class))?;
    let ty = PARAM_TYPES.get(t.ty as usize).ok_or_else(|| format!("CTAB parameter type {}", t.ty))?;
    let mut s = format!("{class} {ty} {}x{} [{}]", t.rows, t.columns, t.elements);
    if !t.members.is_empty() {
        let mut parts = Vec::with_capacity(t.members.len());
        for (name, mt) in &t.members {
            check_name(name)?;
            parts.push(format!("{name}: {}", type_text(mt)?));
        }
        s.push_str(&format!(" {{ {} }}", parts.join(", ")));
    }
    Ok(s)
}

// ── disassembly ──────────────────────────────────────────────────────────────────────────────────

/// Disassemble a `vs_3_0`/`ps_3_0` blob to text that [`assemble`] turns back into the same bytes.
/// Hard-fails on anything malformed or not exactly expressible.
pub fn disassemble(blob: &[u8]) -> Result<String, Sm3Error> {
    let tokens = to_tokens(blob)?;
    let version = *tokens.first().ok_or_else(|| tok_err(0, "empty blob"))?;
    let profile = profile_name(version).ok_or_else(|| tok_err(0, format!("version token 0x{version:08x} is not vs_3_0/ps_3_0")))?;
    let mut lines = vec![profile.to_string()];
    let mut i = 1;
    loop {
        let tok = *tokens.get(i).ok_or_else(|| tok_err(i, "blob has no end token"))?;
        if tok == END_TOKEN {
            if i + 1 != tokens.len() {
                return Err(tok_err(i + 1, "data after the end token"));
            }
            break;
        }
        if tok & 0xffff == OP_COMMENT {
            if tok & 0x8000_0000 != 0 {
                return Err(tok_err(i, "comment token sets bit 31"));
            }
            let len = ((tok >> 16) & 0x7fff) as usize;
            let payload = tokens.get(i + 1..i + 1 + len).ok_or_else(|| tok_err(i, "comment runs past the end"))?;
            let bytes = to_bytes(payload);
            if bytes.starts_with(&CTAB_FOURCC) {
                let ctab = Ctab::decode(&bytes[4..], version).map_err(|m| tok_err(i, m))?;
                let again = ctab.comment_tokens(version).map_err(|m| tok_err(i, m))?;
                if again[..] != tokens[i..i + 1 + len] {
                    return Err(tok_err(i, "CTAB is not laid out as the retail compiler lays it out; re-encoding would change its bytes"));
                }
                lines.extend(ctab.text().map_err(|m| tok_err(i, m))?);
            } else {
                let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                lines.push(format!(".comment {hex}").trim_end().to_string());
            }
            i += 1 + len;
            continue;
        }
        let (text, next) = dis_instr(&tokens, i)?;
        lines.push(text);
        i = next;
    }
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out)
}

// ── assembly ─────────────────────────────────────────────────────────────────────────────────────

/// Split source into (line number, statement) pairs: `//` starts a comment, `;` separates
/// statements, neither counts inside a quoted string.
fn statements(src: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in src.lines().enumerate() {
        let mut cur = String::new();
        let mut quoted = false;
        let mut escaped = false;
        let chars: Vec<char> = line.chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            if quoted {
                cur.push(c);
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    quoted = false;
                }
            } else if c == '"' {
                quoted = true;
                cur.push(c);
            } else if c == '/' && chars.get(k + 1) == Some(&'/') {
                break;
            } else if c == ';' {
                out.push((n + 1, std::mem::take(&mut cur)));
            } else {
                cur.push(c);
            }
            k += 1;
        }
        out.push((n + 1, cur));
    }
    out.into_iter()
        .map(|(n, s)| (n, s.trim().to_string()))
        .filter(|(_, s)| !s.is_empty())
        .collect()
}

/// Parse `-c10[a0.x]_abs.xy` into its source token (plus relative-address token).
fn asm_src(s: &str) -> Result<Vec<u32>, String> {
    let (prefix, body) = if let Some(r) = s.strip_prefix("1-") {
        ("1-", r)
    } else if let Some(r) = s.strip_prefix('-') {
        ("-", r)
    } else if let Some(r) = s.strip_prefix('!') {
        ("!", r)
    } else {
        ("", s)
    };
    let (ty, num, rest) = parse_reg(body)?;
    let (rel, rest) = parse_rel(rest)?;
    let (suffix, rest) = match rest.strip_prefix('_') {
        Some(r) => {
            let n = r.find('.').unwrap_or(r.len());
            (&r[..n], &r[n..])
        }
        None => ("", rest),
    };
    let swizzle = match rest.strip_prefix('.') {
        Some(sw) => parse_swizzle(sw)?,
        None if rest.is_empty() => 0xe4,
        None => return Err(format!("unexpected {rest:?} in source {s:?}")),
    };
    let modifier = match (prefix, suffix) {
        ("", "") => 0,
        ("-", "") => 1,
        ("", "bias") => 2,
        ("-", "bias") => 3,
        ("", "bx2") => 4,
        ("-", "bx2") => 5,
        ("1-", "") => 6,
        ("", "x2") => 7,
        ("-", "x2") => 8,
        ("", "dz") => 9,
        ("", "dw") => 10,
        ("", "abs") => 11,
        ("-", "abs") => 12,
        ("!", "") => 13,
        _ => return Err(format!("unknown source modifier {prefix}…_{suffix} in {s:?}")),
    };
    let mut t = vec![PARAM_BIT | encode_reg(ty, num) | (swizzle << 16) | (modifier << 24)];
    if let Some(r) = rel {
        t[0] |= REL_BIT;
        t.push(r);
    }
    Ok(t)
}

/// An optional `[a0.x]` / `[aL]` right after a register; returns its token and the rest.
fn parse_rel(s: &str) -> Result<(Option<u32>, &str), String> {
    let Some(r) = s.strip_prefix('[') else {
        return Ok((None, s));
    };
    let close = r.find(']').ok_or_else(|| format!("unclosed [ in {s:?}"))?;
    let inner = &r[..close];
    let (ty, num, rest) = parse_reg(inner)?;
    let swizzle = match rest.strip_prefix('.') {
        Some(sw) => parse_swizzle(sw)?,
        None if rest.is_empty() => 0xe4,
        None => return Err(format!("bad relative address [{inner}]")),
    };
    Ok((Some(PARAM_BIT | encode_reg(ty, num) | (swizzle << 16)), &r[close + 1..]))
}

/// Parse `o3[aL].xy` into its destination token (plus relative-address token); `mods` is the
/// `(modifier bits, shift)` taken from the instruction suffixes.
fn asm_dst(s: &str, mods: (u32, u32)) -> Result<Vec<u32>, String> {
    let (ty, num, rest) = parse_reg(s)?;
    let (rel, rest) = parse_rel(rest)?;
    let mask = match rest.strip_prefix('.') {
        Some(m) => parse_mask(m)?,
        None if rest.is_empty() => 0xf,
        None => return Err(format!("unexpected {rest:?} in destination {s:?}")),
    };
    let mut t = vec![PARAM_BIT | encode_reg(ty, num) | (mask << 16) | (mods.0 << 20) | (mods.1 << 24)];
    if let Some(r) = rel {
        t[0] |= REL_BIT;
        t.push(r);
    }
    Ok(t)
}

/// Fold the `_sat`/`_pp`/`_centroid`/shift suffixes into (modifier bits, shift).
fn dst_mods(suffixes: &[&str]) -> Result<(u32, u32), String> {
    let (mut mods, mut shift) = (0u32, None);
    for s in suffixes {
        let (bit, sh) = match *s {
            "sat" => (1, None),
            "pp" => (2, None),
            "centroid" => (4, None),
            "x2" => (0, Some(1)),
            "x4" => (0, Some(2)),
            "x8" => (0, Some(3)),
            "d8" => (0, Some(13)),
            "d4" => (0, Some(14)),
            "d2" => (0, Some(15)),
            other => return Err(format!("unknown instruction suffix _{other}")),
        };
        if bit != 0 {
            if mods & bit != 0 {
                return Err(format!("suffix _{s} given twice"));
            }
            mods |= bit;
        }
        if let Some(v) = sh {
            if shift.is_some() {
                return Err("two shift suffixes".into());
            }
            shift = Some(v);
        }
    }
    Ok((mods, shift.unwrap_or(0)))
}

fn split_operands(s: &str) -> Vec<&str> {
    if s.trim().is_empty() {
        Vec::new()
    } else {
        s.split(',').map(str::trim).collect()
    }
}

fn instr_token(code: u16, control: u32, params: usize, predicated: bool) -> Result<u32, String> {
    if params > 15 {
        return Err(format!("{params} parameter tokens exceed the 4-bit length field"));
    }
    Ok(code as u32 | (control << 16) | ((params as u32) << 24) | if predicated { 1 << 28 } else { 0 })
}

fn asm_instr(stmt: &str) -> Result<Vec<u32>, String> {
    let (pred, stmt) = match stmt.strip_prefix('(') {
        Some(r) => {
            let close = r.find(')').ok_or("unclosed predicate (")?;
            (Some(asm_src(r[..close].trim())?), r[close + 1..].trim())
        }
        None => (None, stmt),
    };
    let (mnemonic, rest) = match stmt.find(char::is_whitespace) {
        Some(n) => (&stmt[..n], stmt[n..].trim()),
        None => (stmt, ""),
    };
    let operands = split_operands(rest);
    let parts: Vec<&str> = mnemonic.split('_').collect();
    let base = parts[0];

    if matches!(base, "dcl" | "def" | "defi" | "defb") && pred.is_some() {
        return Err(format!("{base} cannot be predicated"));
    }
    match base {
        "dcl" => {
            let mut usage_tok = PARAM_BIT;
            let mut rest_parts = &parts[1..];
            if let Some(first) = parts.get(1) {
                if let Some((v, _)) = SAMPLER_TYPES.iter().find(|(_, n)| n == first) {
                    usage_tok |= v << 27;
                    rest_parts = &parts[2..];
                } else {
                    let digits = first.len() - first.trim_end_matches(|c: char| c.is_ascii_digit()).len();
                    let name = &first[..first.len() - digits];
                    if let Some(u) = USAGES.iter().position(|u| *u == name) {
                        let idx: u32 = if digits == 0 { 0 } else { first[name.len()..].parse().map_err(|_| "bad usage index")? };
                        if idx > 15 {
                            return Err(format!("usage index {idx} exceeds 15"));
                        }
                        usage_tok |= u as u32 | (idx << 16);
                        rest_parts = &parts[2..];
                    }
                }
            }
            let mods = dst_mods(rest_parts)?;
            if operands.len() != 1 {
                return Err("dcl takes one register".into());
            }
            let dst = asm_dst(operands[0], mods)?;
            let is_sampler = regtype(dst[0]) == RT_SAMPLER;
            let has_texture_type = (usage_tok >> 27) & 0xf != 0;
            if is_sampler != has_texture_type {
                return Err("a sampler dcl needs dcl_2d/dcl_cube/dcl_volume, and only a sampler takes one".into());
            }
            let mut t = vec![instr_token(OP_DCL, 0, 1 + dst.len(), false)?, usage_tok];
            t.extend(dst);
            Ok(t)
        }
        "def" | "defi" | "defb" => {
            if parts.len() != 1 {
                return Err(format!("{base} takes no suffixes"));
            }
            let (code, n) = match base {
                "def" => (OP_DEF, 4),
                "defi" => (OP_DEFI, 4),
                _ => (OP_DEFB, 1),
            };
            if operands.len() != 1 + n {
                return Err(format!("{base} takes a register and {n} value(s)"));
            }
            let dst = asm_dst(operands[0], (0, 0))?;
            if dst.len() != 1 {
                return Err(format!("{base} register cannot be relatively addressed"));
            }
            let mut t = vec![instr_token(code, 0, 1 + n, false)?, dst[0]];
            for v in &operands[1..] {
                t.push(match code {
                    OP_DEF => {
                        let f: f32 = v.parse().map_err(|_| format!("bad float {v:?}"))?;
                        if !f.is_finite() {
                            return Err(format!("def value {v:?} is not finite"));
                        }
                        f.to_bits()
                    }
                    OP_DEFI => v.parse::<i32>().map_err(|_| format!("bad integer {v:?}"))? as u32,
                    _ => match *v {
                        "true" => 1,
                        "false" => 0,
                        _ => return Err(format!("defb value {v:?} is not true/false")),
                    },
                });
            }
            Ok(t)
        }
        _ => {
            let (op, control, suffixes) = if let Some(c) = parts.get(1).and_then(|p| COMPARISONS.iter().position(|c| c == p)) {
                let op = op_by_name(base, true).ok_or_else(|| format!("{base} takes no comparison"))?;
                (op, c as u32 + 1, &parts[2..])
            } else if let Some(c) = TEXLD_NAMES.iter().position(|n| *n == base) {
                (op_by_code(OP_TEXLD).ok_or("texld missing from the opcode table")?, c as u32, &parts[1..])
            } else {
                let op = op_by_name(base, false).ok_or_else(|| {
                    if op_by_name(base, true).is_some() {
                        format!("{base} needs a comparison suffix (_gt/_eq/_ge/_lt/_ne/_le)")
                    } else {
                        format!("unknown instruction {base:?}")
                    }
                })?;
                (op, 0, &parts[1..])
            };
            let mods = dst_mods(suffixes)?;
            if !op.dst && mods != (0, 0) {
                return Err(format!("{mnemonic}: modifiers need a destination"));
            }
            let want = op.srcs + usize::from(op.dst);
            if operands.len() != want {
                return Err(format!("{mnemonic} takes {want} operand(s), got {}", operands.len()));
            }
            let mut params = Vec::new();
            let mut ops = operands.iter();
            if op.dst {
                params.extend(asm_dst(ops.next().ok_or("missing destination")?, mods)?);
            }
            let predicated = pred.is_some();
            if let Some(p) = pred {
                params.extend(p);
            }
            for s in ops {
                params.extend(asm_src(s)?);
            }
            let mut t = vec![instr_token(op.code, control, params.len(), predicated)?];
            t.extend(params);
            Ok(t)
        }
    }
}

/// Parse the attributes of a `.ctab` line (`creator="…" target="…" flags=0x…`).
fn parse_ctab_header(s: &str, default_target: &str) -> Result<Ctab, String> {
    let mut ctab = Ctab {
        creator: DEFAULT_CREATOR.into(),
        target: default_target.into(),
        flags: 0,
        constants: Vec::new(),
    };
    let chars: Vec<char> = s.chars().collect();
    let mut k = 0;
    let mut seen: Vec<String> = Vec::new();
    while k < chars.len() {
        if chars[k].is_whitespace() {
            k += 1;
            continue;
        }
        let start = k;
        while k < chars.len() && chars[k] != '=' && !chars[k].is_whitespace() {
            k += 1;
        }
        let key: String = chars[start..k].iter().collect();
        if chars.get(k) != Some(&'=') {
            return Err(format!(".ctab attribute {key:?} needs =value"));
        }
        k += 1;
        let value = if chars.get(k) == Some(&'"') {
            k += 1;
            let mut v = String::new();
            loop {
                match chars.get(k) {
                    None => return Err("unterminated string".into()),
                    Some('"') => {
                        k += 1;
                        break;
                    }
                    Some('\\') => {
                        v.push(*chars.get(k + 1).ok_or("dangling \\")?);
                        k += 2;
                    }
                    Some(c) => {
                        v.push(*c);
                        k += 1;
                    }
                }
            }
            v
        } else {
            let start = k;
            while k < chars.len() && !chars[k].is_whitespace() {
                k += 1;
            }
            chars[start..k].iter().collect()
        };
        if seen.contains(&key) {
            return Err(format!(".ctab attribute {key} given twice"));
        }
        seen.push(key.clone());
        match key.as_str() {
            "creator" => ctab.creator = value,
            "target" => ctab.target = value,
            "flags" => ctab.flags = parse_u32(&value)?,
            other => return Err(format!("unknown .ctab attribute {other:?}")),
        }
    }
    Ok(ctab)
}

fn parse_u32(v: &str) -> Result<u32, String> {
    match v.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16),
        None => v.parse(),
    }
    .map_err(|_| format!("bad number {v:?}"))
}

fn parse_u16(v: &str, what: &str) -> Result<u16, String> {
    v.parse().map_err(|_| format!("bad {what} {v:?}"))
}

/// A tiny cursor for the recursive type grammar.
struct TypeCursor<'a> {
    s: &'a str,
}

impl<'a> TypeCursor<'a> {
    fn skip_ws(&mut self) {
        self.s = self.s.trim_start();
    }
    fn word(&mut self) -> Result<&'a str, String> {
        self.skip_ws();
        let n = self
            .s
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
            .unwrap_or(self.s.len());
        if n == 0 {
            return Err(format!("expected a word at {:?}", self.s));
        }
        let (w, rest) = self.s.split_at(n);
        self.s = rest;
        Ok(w)
    }
    fn eat(&mut self, c: char) -> bool {
        self.skip_ws();
        match self.s.strip_prefix(c) {
            Some(r) => {
                self.s = r;
                true
            }
            None => false,
        }
    }
    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(format!("expected {c:?} at {:?}", self.s))
        }
    }
    fn ty(&mut self) -> Result<CtabType, String> {
        let class = self.word()?;
        let class = CLASSES.iter().position(|c| *c == class).ok_or_else(|| format!("unknown type class {class:?}"))?;
        let ty = self.word()?;
        let ty = PARAM_TYPES.iter().position(|t| *t == ty).ok_or_else(|| format!("unknown parameter type {ty:?}"))?;
        let dims = self.word()?;
        let (r, c) = dims.split_once('x').ok_or_else(|| format!("expected RxC, got {dims:?}"))?;
        let rows = parse_u16(r, "rows")?;
        let columns = parse_u16(c, "columns")?;
        self.expect('[')?;
        let elements = parse_u16(self.word()?, "elements")?;
        self.expect(']')?;
        let mut members = Vec::new();
        if self.eat('{') {
            loop {
                let name = self.word()?.to_string();
                self.expect(':')?;
                members.push((name, self.ty()?));
                if self.eat('}') {
                    break;
                }
                self.expect(',')?;
            }
            if members.is_empty() {
                return Err("empty struct member list".into());
            }
        }
        Ok(CtabType { class: class as u16, ty: ty as u16, rows, columns, elements, members })
    }
}

/// Parse `.const <name> <set><index> <count> [reserved=N] : <type>`.
fn parse_const(s: &str) -> Result<CtabConstant, String> {
    let (head, ty) = s.split_once(':').ok_or(".const needs ': <type>'")?;
    let words: Vec<&str> = head.split_whitespace().collect();
    if words.len() < 3 || words.len() > 4 {
        return Err(".const takes <name> <register> <count> [reserved=N] : <type>".into());
    }
    let name = words[0].to_string();
    check_name(&name)?;
    let set_char = words[1].chars().next().ok_or("missing register")?;
    let register_set = REGISTER_SETS
        .iter()
        .position(|c| *c == set_char)
        .ok_or_else(|| format!("register set {set_char:?} is not b/i/c/s"))? as u16;
    let register_index = parse_u16(&words[1][1..], "register index")?;
    let register_count = parse_u16(words[2], "register count")?;
    let reserved = match words.get(3) {
        Some(w) => parse_u16(w.strip_prefix("reserved=").ok_or_else(|| format!("unexpected {w:?}"))?, "reserved")?,
        None => 0,
    };
    let mut cur = TypeCursor { s: ty };
    let ty = cur.ty()?;
    if !cur.s.trim().is_empty() {
        return Err(format!("trailing text after the type: {:?}", cur.s));
    }
    Ok(CtabConstant { name, register_set, register_index, register_count, reserved, ty })
}

fn parse_hex_words(s: &str) -> Result<Vec<u32>, String> {
    let s: String = s.split_whitespace().collect();
    if !s.len().is_multiple_of(8) {
        return Err(".comment payload must be whole 4-byte words of hex".into());
    }
    let bytes: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|k| u8::from_str_radix(&s[k..k + 2], 16).map_err(|_| format!("bad hex {:?}", &s[k..k + 2])))
        .collect::<Result<_, _>>()?;
    Ok(bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

/// Assemble SM3 text into a blob. The first statement names the profile (`vs_3_0` / `ps_3_0`).
pub fn assemble(src: &str) -> Result<Vec<u8>, Sm3Error> {
    let stmts = statements(src);
    let (first_line, profile) = stmts.first().ok_or_else(|| src_err(1, "empty source; expected vs_3_0 or ps_3_0"))?;
    let version = profile_version(profile)
        .ok_or_else(|| src_err(*first_line, format!("expected vs_3_0 or ps_3_0 first, got {profile:?}")))?;
    let mut body: Vec<u32> = Vec::new();
    let mut have_ctab = false;
    let mut open_ctab: Option<(usize, Ctab)> = None;
    for (line, stmt) in &stmts[1..] {
        let line = *line;
        let wrap = |m: String| src_err(line, m);
        if let Some((_, ctab)) = open_ctab.as_mut() {
            if stmt == ".endctab" {
                let (_, ctab) = open_ctab.take().ok_or_else(|| wrap("no open .ctab".into()))?;
                body.extend(ctab.comment_tokens(version).map_err(wrap)?);
                have_ctab = true;
            } else if let Some(rest) = stmt.strip_prefix(".const ") {
                ctab.constants.push(parse_const(rest).map_err(wrap)?);
            } else {
                return Err(wrap(format!("expected .const or .endctab inside .ctab, got {stmt:?}")));
            }
            continue;
        }
        if stmt == ".ctab" || stmt.starts_with(".ctab ") {
            let ctab = parse_ctab_header(&stmt[5..], profile).map_err(wrap)?;
            open_ctab = Some((line, ctab));
        } else if stmt == ".comment" || stmt.starts_with(".comment ") {
            let words = parse_hex_words(&stmt[8..]).map_err(wrap)?;
            if words.len() > MAX_COMMENT_WORDS {
                return Err(wrap("comment exceeds 32767 words".into()));
            }
            if words.first() == Some(&u32::from_le_bytes(CTAB_FOURCC)) {
                return Err(wrap("a CTAB comment must be written as a .ctab block".into()));
            }
            body.push(OP_COMMENT | ((words.len() as u32) << 16));
            body.extend(words);
        } else if stmt.starts_with('.') {
            return Err(wrap(format!("unknown directive {stmt:?}")));
        } else if profile_version(stmt).is_some() {
            return Err(wrap("a second version statement".into()));
        } else {
            body.extend(asm_instr(stmt).map_err(wrap)?);
        }
    }
    if let Some((line, _)) = open_ctab {
        return Err(src_err(line, ".ctab without .endctab"));
    }
    let mut tokens = vec![version];
    if !have_ctab {
        let ctab = Ctab {
            creator: DEFAULT_CREATOR.into(),
            target: profile.clone(),
            flags: 0,
            constants: Vec::new(),
        };
        tokens.extend(ctab.comment_tokens(version).map_err(|m| src_err(*first_line, m))?);
    }
    tokens.extend(body);
    tokens.push(END_TOKEN);
    Ok(to_bytes(&tokens))
}

/// Find the first `CTAB` comment of a blob and decode it.
pub fn find_ctab(blob: &[u8]) -> Result<Option<Ctab>, Sm3Error> {
    let tokens = to_tokens(blob)?;
    let version = *tokens.first().ok_or_else(|| tok_err(0, "empty blob"))?;
    let mut i = 1;
    while let Some(&tok) = tokens.get(i) {
        if tok == END_TOKEN {
            break;
        }
        if tok & 0xffff == OP_COMMENT {
            let len = ((tok >> 16) & 0x7fff) as usize;
            let payload = tokens.get(i + 1..i + 1 + len).ok_or_else(|| tok_err(i, "comment runs past the end"))?;
            let bytes = to_bytes(payload);
            if bytes.starts_with(&CTAB_FOURCC) {
                return Ctab::decode(&bytes[4..], version).map(Some).map_err(|m| tok_err(i, m));
            }
            i += 1 + len;
        } else {
            i += 1 + ((tok >> 24) & 0xf) as usize;
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader3::{retail_store_for_test, RETAIL_STORES};

    fn words(blob: &[u8]) -> Vec<u32> {
        to_tokens(blob).unwrap()
    }

    /// The instruction tokens after the CTAB comment (which sits right after the version token).
    fn after_ctab(t: &[u32]) -> &[u32] {
        assert_eq!(t[1] & 0xffff, OP_COMMENT, "CTAB comment follows the version token");
        &t[2 + ((t[1] >> 16) & 0x7fff) as usize..]
    }

    #[test]
    fn magenta_pixel_shader() {
        let blob = assemble("ps_3_0\ndef c0, 1, 0, 1, 1\nmov oC0, c0\n").unwrap();
        let t = words(&blob);
        assert_eq!(t[0], 0xffff_0300);
        assert_eq!(
            after_ctab(&t),
            &[
                0x0500_0051, 0xa00f_0000, 0x3f80_0000, 0x0000_0000, 0x3f80_0000, 0x3f80_0000,
                0x0200_0001, 0x800f_0800, 0xa0e4_0000, 0x0000_ffff
            ]
        );
        // The CTAB: no constants, target = the profile, creator = this assembler.
        let ctab = find_ctab(&blob).unwrap().unwrap();
        assert_eq!(ctab.target, "ps_3_0");
        assert_eq!(ctab.creator, DEFAULT_CREATOR);
        assert!(ctab.constants.is_empty());
        // The same program on one line with `;` separators assembles identically.
        assert_eq!(assemble("ps_3_0; def c0, 1, 0, 1, 1; mov oC0, c0").unwrap(), blob);
        // And it round-trips.
        assert_eq!(assemble(&disassemble(&blob).unwrap()).unwrap(), blob);
    }

    #[test]
    fn empty_ctab_header_matches_the_retail_zero_constant_form() {
        // Retail zero-constant tables carry ConstantInfo = 0 (8 records); ours must too.
        let ctab = Ctab { creator: "x".into(), target: "vs_3_0".into(), flags: 0, constants: vec![] };
        let b = ctab.encode(VS_3_0);
        assert_eq!(rd32(&b, 12).unwrap(), 0);
        assert_eq!(rd32(&b, 16).unwrap(), 0);
        assert_eq!(b.len() % 4, 0);
        assert_eq!(Ctab::decode(&b, VS_3_0).unwrap(), ctab);
    }

    #[test]
    fn operand_forms_round_trip() {
        let src = "\
vs_3_0
.ctab creator=\"t \\\"q\\\"\" target=\"vs_3_0\" flags=0x00000001
.const m c4 4 reserved=7 : matrix_columns float 4x4 [1]
.const s c8 3 : struct void 1x12 [1] { a: vector float 1x4 [1], b: matrix_rows float 2x4 [1] }
.const t s0 1 : object sampler2d 1x1 [1]
.endctab
.comment 4442554701020304
dcl_position v0
dcl_texcoord3 o3.xy
dcl_2d s0
defi i0, 4, -1, 1, 0
defb b1, true
def c20, 0.5, -0, 1e-30, 3.4028235e38
mova a0.x, v0.x
mad_sat_pp r0.xyz, -c10[a0.x]_abs.x, v0.yxzw, 1-r1.xyz
mul_x2 r1.w, r0_bias.xy, r0_bx2
if b1
if_lt r0.x, c0.y
break_ge r0.x, c0.z
rep i0
endrep
else
endif
endif
(!p0.x) add r2, r0, r1
setp_ne p0, r0, r1
texldl r3, v0, s0
sincos r4.xy, r0.x
m4x4 o3[aL], v0, c4
nrm r5.xyz, r0
";
        let blob = assemble(src).unwrap();
        let text = disassemble(&blob).unwrap();
        assert_eq!(assemble(&text).unwrap(), blob, "text:\n{text}");
        // Spot-check encodings, walking instruction boundaries.
        let t = words(&blob);
        let mut starts = Vec::new();
        let mut i = 1;
        while t[i] != END_TOKEN {
            if t[i] & 0xffff == OP_COMMENT {
                i += 1 + ((t[i] >> 16) & 0x7fff) as usize;
            } else {
                starts.push(i);
                i += 1 + ((t[i] >> 24) & 0xf) as usize;
            }
        }
        let find = |code: u32| *starts.iter().find(|&&k| t[k] & 0xffff == code).expect("opcode present");
        let mova = find(0x2e);
        assert_eq!(t[mova], 0x0200_002e);
        assert_eq!(t[mova + 1], 0x8001_0000 | (3 << 28), "a0.x");
        let mad = find(0x04);
        assert_eq!(t[mad] >> 24, 5, "mad with a relative source has 5 param tokens");
        assert_eq!(t[mad + 1] & 0x00f0_0000, 0x0030_0000, "_sat_pp");
        assert_eq!(t[mad + 2] & (REL_BIT | 0x0f00_0000), REL_BIT | (12 << 24), "-..._abs relative");
        assert_eq!(t[mad + 3], 0xb000_0000, "relative token a0.x");
        let ifc = find(0x29);
        assert_eq!((t[ifc] >> 16) & 0xff, 4, "lt = 4");
        let add = find(0x02);
        assert_ne!(t[add] & (1 << 28), 0, "predicated");
        assert_eq!(regtype(t[add + 2]), 19, "predicate token follows the destination");
        assert_eq!((t[add + 2] >> 24) & 0xf, 13, "! is the not modifier");
    }

    #[test]
    fn refusals() {
        for (src, why) in [
            ("", "empty"),
            ("vs_2_0\nmov r0, r1", "profile"),
            ("ps_3_0\nmov r0", "operand count"),
            ("ps_3_0\nfoo r0, r1", "unknown opcode"),
            ("ps_3_0\nif r0.x, c0", "if without comparison takes one operand"),
            ("ps_3_0\nsetp p0, r0, r1", "setp without comparison"),
            ("ps_3_0\nmov r0.yx, r1", "mask out of order"),
            ("ps_3_0\nmov r0, r1.q", "bad swizzle"),
            ("ps_3_0\nendif_sat", "modifier without destination"),
            ("ps_3_0\ndcl_texcoord s0", "sampler with usage"),
            ("ps_3_0\ndcl_2d v0", "texture type on an input"),
            ("ps_3_0\ndef c0, 1, 2, 3", "def arity"),
            ("ps_3_0\ndef c0, 1, 2, 3, inf", "non-finite def"),
            ("ps_3_0\n.ctab\n.const x c0 1 : vector float 1x4 [1]", "unterminated ctab"),
            ("ps_3_0\n.comment 435441420000", "partial comment word"),
            ("ps_3_0\n.comment 43544142", "a CTAB written as a raw comment"),
            ("ps_3_0\nmov r0, c2048", "register number"),
        ] {
            assert!(assemble(src).is_err(), "{why}: {src:?} should be refused");
        }
    }

    #[test]
    fn disassembler_refuses_what_it_cannot_express() {
        let base = assemble("ps_3_0\nmov oC0, c0").unwrap();
        let t = words(&base);
        let mov = t.len() - 4;
        assert_eq!(t[mov], 0x0200_0001);
        let bad = |edit: &dyn Fn(&mut Vec<u32>)| {
            let mut t = t.clone();
            edit(&mut t);
            disassemble(&to_bytes(&t)).is_err()
        };
        assert!(bad(&|t| t[mov + 1] |= 0x4000), "reserved dst bits");
        assert!(bad(&|t| t[mov + 2] |= 0x0e00_0000), "unknown source modifier 14");
        assert!(bad(&|t| t[mov] |= 0x0001_0000), "control on mov");
        assert!(bad(&|t| t[mov] = 0x0300_0001), "length disagrees with operands");
        assert!(bad(&|t| t.push(0)), "data after end");
        assert!(bad(&|t| {
            t.pop();
        }), "no end token");
        assert!(bad(&|t| t[mov] = 0x0200_0045), "not an SM3 opcode");
    }

    /// Every retail record of every store: `assemble(disassemble(blob)) == blob`, byte for byte,
    /// CTAB included (the disassembler carries the CTAB as a structured `.ctab` block and the
    /// assembler re-lays it out).
    #[test]
    fn retail_round_trip_is_byte_identical() {
        let mut total = 0usize;
        let mut failures = Vec::new();
        let mut present = 0;
        for name in RETAIL_STORES {
            let Some(store) = retail_store_for_test(name) else { continue };
            present += 1;
            let mut ok = 0;
            for (i, r) in store.records.iter().enumerate() {
                let blob = store.blob(r);
                let result = disassemble(blob).and_then(|text| assemble(&text));
                match result {
                    Ok(b) if b == blob => ok += 1,
                    Ok(_) => failures.push(format!("{name} rec{i}: bytes differ")),
                    Err(e) => failures.push(format!("{name} rec{i}: {e}")),
                }
            }
            eprintln!("round trip {name}: {ok}/{} byte-identical", store.records.len());
            total += store.records.len();
        }
        if present == 0 {
            return;
        }
        assert_eq!(present, RETAIL_STORES.len(), "every retail store must be present once any is");
        assert!(failures.is_empty(), "{} of {total} records failed:\n{}", failures.len(), failures.join("\n"));
    }
}
