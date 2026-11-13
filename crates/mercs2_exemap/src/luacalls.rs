//! Every call an image's code makes to a Lua script function by name, with the module and the name
//! it passes: the functions the engine calls in a script, which the script's own text never shows
//! being called.
//!
//! **Callers** (`data/lua_callers.tsv`, [`Callers`]): per image, the function through which its code
//! calls a Lua function by name, and where each call passes the function-name string and the
//! module's `pandemic_hash_m2` — an x86 register loaded before the call (`ecx`, `eax`), or the 0-based
//! argument of the call as the image's Ghidra decomp writes it (`arg2`). The table opens with `#`
//! comment lines; no field is empty. A PC image's row names registers, a console image's arguments.
//!
//! **x86** ([`x86_calls`]). Every `E8 rel32` in an executable section whose target is the caller is
//! a site; the PC decomp drops the register arguments, so the bytes are read instead. Each site is
//! decoded (iced-x86) from every start [`BACK_MIN`]`..=`[`BACK_MAX`] bytes before it; a start whose
//! instructions land exactly on the site is a landing start. With fewer than [`MIN_LANDING`] landing
//! starts the bytes are not a call the code makes, and the site's values are `unread`. Otherwise the
//! site's window is the run of instructions every landing start decodes alike, ending at the site, cut
//! after its last instruction that is not straight-line (a branch, call or return: the block the call
//! is in). A register's value is the immediate of the block's last write to it when that write is
//! `mov reg, imm32`; `computed` when that write is anything else, or when the block starts without
//! writing it (the value comes from before the block); `unread` when the window ends before the
//! block's start without a write.
//!
//! **PowerPC** ([`ppc_calls`]). The decomp keeps the register arguments, so each call is read from
//! the text of the function that makes it (the caller's record in the functions export lists every
//! calling function). An argument is a hex literal (a 64-bit sign-extended literal is its low word),
//! a dereference `*(T *)(base ± K)` of the TOC or of a local holding a word loaded through the TOC
//! (PS3, `mercs2_exemap::hashsites` evaluates both), or a local assigned such a load; anything else
//! is `computed`. A function's decomp must hold as many calls as the export lists sites in it. The
//! raw scan of every `bl` to the caller must find every listed site; a `bl` in no listed calling
//! function (a site Ghidra put in no function) is a row with both values `unread`.
//!
//! A name value must address printable text; one that does not is an error naming the call, since
//! the callers row is then wrong.
//!
//! Besides `call`, the x86 scan finds `push <ret>; push <caller>; ret` whose `<ret>` is code after it:
//! the call SecuROM's moved code makes (with junk bytes between the `ret` and the return address).
//! A caller with no call site is an error: its row is wrong.
//!
//! **Output** ([`to_tsv`]): `#` lines naming each image (sha256, caller, sites found), the header
//! [`CALLS_HEADER`], then one row per call: image, calling function (`-` for a site in no Ghidra
//! function), site (`-` for a call read from the decomp), the module hash (`0x%08x`) and the name.
//! A value that is not known is `(computed)` or `(unread)`; a module is `(object)` when the call
//! runs the function in the script of the object it passes.

use std::collections::{BTreeMap, HashMap};

use iced_x86::{Decoder, DecoderOptions, FlowControl, Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register};
use serde::Deserialize;

use crate::hashsites::{assigned_local, literal_u32, toc_load, tokenize, ImageMemory, Memory, Tok};
use crate::images::{Images, Platform};
use crate::mem::Mem;
use crate::{commented_table_rows, Error};

pub const CALLERS_HEADER: &str = "image_id\tva\tname\tmodule\tevidence";
pub const CALLS_HEADER: &str = "image_id\tfunction\tsite\tmodule\tname";

/// Where a call passes a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Passed {
    /// An x86 register, loaded before the call.
    Register(Register),
    /// The x86 stack argument pushed `k`-th last before the call (`stack0` is the last push).
    Stack(usize),
    /// The 0-based argument of the call in the decomp.
    Arg(usize),
    /// The module only: the call passes an object, and the function runs in that object's script.
    Object,
}

/// One row of `data/lua_callers.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    pub image_id: String,
    pub va: u32,
    pub name: Passed,
    pub module: Passed,
    pub evidence: String,
}

/// The callers table.
#[derive(Clone, Debug)]
pub struct Callers {
    pub rows: Vec<Caller>,
}

fn register(s: &str) -> Option<Register> {
    Some(match s {
        "eax" => Register::EAX,
        "ecx" => Register::ECX,
        "edx" => Register::EDX,
        "ebx" => Register::EBX,
        "esi" => Register::ESI,
        "edi" => Register::EDI,
        _ => return None,
    })
}

impl Callers {
    /// The table compiled into this crate, checked against the image table.
    pub fn embedded(images: &Images) -> Result<Callers, Error> {
        Callers::parse(include_str!("../data/lua_callers.tsv"), images)
    }

    pub fn parse(text: &str, images: &Images) -> Result<Callers, Error> {
        let mut rows = Vec::new();
        for (n, f) in commented_table_rows("lua_callers.tsv", CALLERS_HEADER, text)? {
            let at = |m: String| Error::new(format!("lua_callers.tsv line {n}: {m}"));
            let image = images.image(f[0]).ok_or_else(|| at(format!("image {:?} is not in images.tsv", f[0])))?;
            let va = f[1]
                .strip_prefix("0x")
                .filter(|h| h.len() == 8)
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .ok_or_else(|| at(format!("va {:?} is not 0x and eight hex digits", f[1])))?;
            let index = |s: &str, prefix: &str| s.strip_prefix(prefix).and_then(|k| k.parse::<usize>().ok());
            let from = |s: &str| -> Result<Passed, Error> {
                match (image.platform, s) {
                    (_, "object") => Ok(Passed::Object),
                    (Platform::Pc, _) => register(s)
                        .map(Passed::Register)
                        .or_else(|| index(s, "stack").map(Passed::Stack))
                        .ok_or_else(|| at(format!("{s:?} is not an x86 register or stackN (a PC image's calls are read from the bytes)"))),
                    _ => index(s, "arg").map(Passed::Arg).ok_or_else(|| at(format!("{s:?} is not argN (a console image's calls are read from the decomp)"))),
                }
            };
            let (name, module) = (from(f[2])?, from(f[3])?);
            if name == Passed::Object {
                return Err(at("the name is a string the call passes, not an object".into()));
            }
            if rows.iter().any(|r: &Caller| r.image_id == f[0] && r.va == va) {
                return Err(at(format!("a second row for image {} caller {}", f[0], f[1])));
            }
            rows.push(Caller { image_id: f[0].to_string(), va, name, module, evidence: f[4].to_string() });
        }
        Ok(Callers { rows })
    }

    /// The callers of one image.
    pub fn of<'s>(&'s self, image_id: &'s str) -> impl Iterator<Item = &'s Caller> + 's {
        self.rows.iter().filter(move |r| r.image_id == image_id)
    }
}

/// A value a call passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Val<T> {
    Known(T),
    /// Not a constant where the call is made.
    Computed,
    /// The bytes or the decomp could not be read for it.
    Unread,
    /// A module only: the script of the object the call passes.
    Object,
}

/// One call to a Lua function by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The Ghidra function the call is in; `None` for a site in no function.
    pub function: Option<u32>,
    /// The call instruction; `None` for a call read from the decomp.
    pub site: Option<u32>,
    pub module: Val<u32>,
    pub name: Val<String>,
}

/// One image's calls.
#[derive(Clone, Debug)]
pub struct ImageCalls {
    pub image_id: String,
    pub sha256: String,
    pub caller: u32,
    /// Call instructions the raw scan found.
    pub sites: usize,
    pub calls: Vec<Call>,
}

/// The function record fields read here.
#[derive(Deserialize)]
struct Record {
    kind: String,
    va: Option<String>,
    name: Option<String>,
    size: Option<u32>,
    decomp: Option<String>,
    callers: Option<Vec<CallerRef>>,
}

#[derive(Deserialize)]
struct CallerRef {
    site: String,
    function: Option<String>,
}

fn hex(s: &str) -> Result<u32, Error> {
    s.strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .ok_or_else(|| Error::new(format!("{s:?} is not a 0x address")))
}

/// One function of a functions export.
struct FnRecord {
    size: u32,
    name: String,
    decomp: Option<String>,
    /// (call site, the function holding it; `None` for a site in no function).
    callers: Vec<(u32, Option<u32>)>,
}

/// The functions export of one image, by address.
pub struct Functions {
    by_va: BTreeMap<u32, FnRecord>,
}

impl Functions {
    /// Read a functions export (`what` names it in errors).
    pub fn parse(text: &str, what: &str) -> Result<Functions, Error> {
        let mut by_va = BTreeMap::new();
        for (i, line) in text.lines().enumerate() {
            let r: Record = serde_json::from_str(line).map_err(|e| Error::new(format!("{what} line {}: {e}", i + 1)))?;
            if r.kind != "function" {
                continue;
            }
            let at = |m: &str| Error::new(format!("{what} line {}: a function record {m}", i + 1));
            let va = hex(r.va.as_deref().ok_or_else(|| at("has no va"))?)?;
            let mut callers = Vec::new();
            for c in r.callers.unwrap_or_default() {
                callers.push((hex(&c.site)?, c.function.as_deref().map(hex).transpose()?));
            }
            let size = r.size.ok_or_else(|| at("has no size"))?;
            let name = r.name.ok_or_else(|| at("has no name"))?;
            by_va.insert(va, FnRecord { size, name, decomp: r.decomp, callers });
        }
        Ok(Functions { by_va })
    }

    /// The decomp of the function at `va`, when the export has one.
    pub(crate) fn decomp(&self, va: u32) -> Option<&str> {
        self.by_va.get(&va)?.decomp.as_deref()
    }

    /// Whether the export has a function at `va`.
    pub(crate) fn has(&self, va: u32) -> bool {
        self.by_va.contains_key(&va)
    }

    /// The function whose body holds `va`.
    fn holding(&self, va: u32) -> Option<u32> {
        let (&f, r) = self.by_va.range(..=va).next_back()?;
        (va < f + r.size).then_some(f)
    }
}

// ---------------------------------------------------------------------------------------------
// x86

pub const BACK_MIN: u32 = 64;
pub const BACK_MAX: u32 = 96;
pub const MIN_LANDING: usize = 3;

/// Every `E8 rel32` in an executable range whose target is `target`.
fn e8_sites(mem: &Mem<'_>, target: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for r in mem.ranges.iter().filter(|r| r.exec) {
        let len = (r.hi - r.lo) as usize;
        let bytes = &mem.d[r.off..r.off + len];
        for o in 0..len.saturating_sub(4) {
            if bytes[o] == 0xE8 {
                let rel = i32::from_le_bytes(bytes[o + 1..o + 5].try_into().expect("four bytes"));
                let site = r.lo + o as u32;
                if (site as i64 + 5 + rel as i64) as u32 == target {
                    out.push(site);
                }
            }
        }
    }
    out
}

/// Every `push <ret>; push <target>; ret` in an executable range whose `<ret>` is code after it: a
/// call SecuROM's moved code makes without `call` (it pads junk bytes between the `ret` and the
/// return address, so the return is not the next byte). The site is the first `push`.
fn push_ret_sites(mem: &Mem<'_>, target: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let t = target.to_le_bytes();
    for r in mem.ranges.iter().filter(|r| r.exec) {
        let len = (r.hi - r.lo) as usize;
        let bytes = &mem.d[r.off..r.off + len];
        for o in 0..len.saturating_sub(10) {
            if bytes[o] == 0x68 && bytes[o + 5] == 0x68 && bytes[o + 6..o + 10] == t && bytes[o + 10] == 0xC3 {
                let ret = u32::from_le_bytes(bytes[o + 1..o + 5].try_into().expect("four bytes"));
                let site = r.lo + o as u32;
                if ret >= site + 11 && mem.exec(ret) {
                    out.push(site);
                }
            }
        }
    }
    out
}

/// The instructions from `start` that end exactly at `site` (the call itself excluded), or `None`
/// when the decode steps over it.
fn decode_to(mem: &Mem<'_>, start: u32, site: u32) -> Option<Vec<Instruction>> {
    let off = mem.off(start)?;
    let bytes = mem.d.get(off..off + (site - start) as usize + 5)?;
    let mut dec = Decoder::with_ip(32, bytes, start as u64, DecoderOptions::NONE);
    let mut out = Vec::new();
    let mut ins = Instruction::default();
    loop {
        let ip = dec.ip() as u32;
        if ip == site {
            return Some(out);
        }
        if ip > site || !dec.can_decode() {
            return None;
        }
        dec.decode_out(&mut ins);
        if ins.is_invalid() {
            return None;
        }
        out.push(ins);
    }
}

/// The site's window (see the module documentation) and whether it reaches the block's start.
fn window(mem: &Mem<'_>, site: u32) -> Option<(Vec<Instruction>, bool)> {
    let landing: Vec<Vec<Instruction>> = (BACK_MIN..=BACK_MAX)
        .filter_map(|b| site.checked_sub(b))
        .filter_map(|start| decode_to(mem, start, site))
        .collect();
    if landing.len() < MIN_LANDING {
        return None;
    }
    // the common suffix: walk back while every landing decode has the same instruction there
    let shortest = landing.iter().map(Vec::len).min().unwrap_or(0);
    let mut k = 0;
    while k < shortest {
        let ip = landing[0][landing[0].len() - 1 - k].ip32();
        if landing.iter().any(|l| l[l.len() - 1 - k].ip32() != ip) {
            break;
        }
        k += 1;
    }
    let common = &landing[0][landing[0].len() - k..];
    match common.iter().rposition(|i| i.flow_control() != FlowControl::Next) {
        Some(p) => Some((common[p + 1..].to_vec(), true)),
        None => Some((common.to_vec(), false)),
    }
}

/// The value of `reg` at the end of `block`.
fn reg_value(block: &[Instruction], reaches_start: bool, reg: Register) -> Val<u32> {
    let mut info = InstructionInfoFactory::new();
    for ins in block.iter().rev() {
        let writes = info.info(ins).used_registers().iter().any(|u| {
            u.register().full_register32() == reg
                && matches!(u.access(), OpAccess::Write | OpAccess::CondWrite | OpAccess::ReadWrite | OpAccess::ReadCondWrite)
        });
        if writes {
            let imm = ins.mnemonic() == Mnemonic::Mov
                && ins.op0_kind() == OpKind::Register
                && ins.op0_register() == reg
                && ins.op1_kind() == OpKind::Immediate32;
            return if imm { Val::Known(ins.immediate32()) } else { Val::Computed };
        }
    }
    if reaches_start {
        Val::Computed
    } else {
        Val::Unread
    }
}

/// The value the `k`-th last push of `block` puts on the stack: its immediate for `push imm32`.
fn stack_value(block: &[Instruction], reaches_start: bool, k: usize) -> Val<u32> {
    match block.iter().rev().filter(|i| i.mnemonic() == Mnemonic::Push).nth(k) {
        Some(p) if matches!(p.op0_kind(), OpKind::Immediate32 | OpKind::Immediate8to32) => Val::Known(p.immediate32()),
        Some(_) => Val::Computed,
        None if reaches_start => Val::Computed,
        None => Val::Unread,
    }
}

fn x86_value(block: &[Instruction], reaches_start: bool, p: &Passed) -> Result<Val<u32>, Error> {
    Ok(match p {
        Passed::Register(r) => reg_value(block, reaches_start, *r),
        Passed::Stack(k) => stack_value(block, reaches_start, *k),
        Passed::Object => Val::Object,
        Passed::Arg(_) => return Err(Error::new("an x86 caller passes its values in registers or on the stack, not as decomp arguments")),
    })
}

/// The name a call passes, from the address it passes; `at` names the call in the error.
fn named(v: Val<u32>, mem: &dyn Memory, at: impl Fn() -> String) -> Result<Val<String>, Error> {
    Ok(match v {
        Val::Known(a) => Val::Known(
            mem.text(a)
                .ok_or_else(|| Error::new(format!("{}: the name passed is 0x{a:08x}, which addresses no printable text", at())))?,
        ),
        Val::Computed => Val::Computed,
        Val::Unread => Val::Unread,
        Val::Object => return Err(Error::new(format!("{}: a name is a string, not an object", at()))),
    })
}

/// The calls of an x86 image to `caller`.
pub fn x86_calls(image: &[u8], functions: &Functions, caller: &Caller) -> Result<(usize, Vec<Call>), Error> {
    let mem = Mem::of(image)?;
    let im = ImageMemory::of(image)?;
    let mut sites = e8_sites(&mem, caller.va);
    sites.extend(push_ret_sites(&mem, caller.va));
    sites.sort_unstable();
    if sites.is_empty() {
        return Err(Error::new(format!("{}: nothing calls 0x{:08x}; the callers row is wrong", caller.image_id, caller.va)));
    }
    let mut calls = Vec::new();
    for &site in &sites {
        let (module, name) = match window(&mem, site) {
            None => (Val::Unread, Val::Unread),
            Some((block, start)) => {
                let module = x86_value(&block, start, &caller.module)?;
                let name = named(x86_value(&block, start, &caller.name)?, &im, || format!("{} site 0x{site:08x}", caller.image_id))?;
                (module, name)
            }
        };
        calls.push(Call { function: functions.holding(site), site: Some(site), module, name });
    }
    Ok((sites.len(), calls))
}

// ---------------------------------------------------------------------------------------------
// PowerPC

/// Every `bl` (opcode 18, AA 0, LK 1) in an executable range whose target is `target`.
fn bl_sites(mem: &Mem<'_>, target: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for r in mem.ranges.iter().filter(|r| r.exec) {
        let n = (r.hi - r.lo) / 4;
        for k in 0..n {
            let site = r.lo + 4 * k;
            let Some(w) = mem.word_at(r.off + 4 * k as usize) else { continue };
            if w >> 26 == 18 && w & 3 == 1 {
                let li = ((w & 0x03ff_fffc) as i32) << 6 >> 6;
                if (site as i64 + li as i64) as u32 == target {
                    out.push(site);
                }
            }
        }
    }
    out
}

/// The tokens of each argument of the call whose `(` is at `open`: (start, end) per argument.
pub(crate) fn call_args(toks: &[Tok], open: usize) -> Option<Vec<(usize, usize)>> {
    let mut depth = 0i32;
    let mut start = open + 1;
    let mut out = Vec::new();
    for (j, t) in toks.iter().enumerate().skip(open + 1) {
        match t {
            Tok::Punct("(") | Tok::Punct("[") => depth += 1,
            Tok::Punct(")") if depth == 0 => {
                if j > start {
                    out.push((start, j));
                }
                return Some(out);
            }
            Tok::Punct(")") | Tok::Punct("]") => depth -= 1,
            Tok::Punct(",") if depth == 0 => {
                out.push((start, j));
                start = j + 1;
            }
            _ => {}
        }
    }
    None
}

/// The constant an argument `toks[s..e]` is, if it is one (see the module documentation).
fn arg_value(toks: &[Tok], s: usize, e: usize, toc: Option<u32>, held: &HashMap<String, u32>, mem: &dyn Memory) -> Val<u32> {
    match &toks[s..e] {
        [Tok::Num(Some(v), true)] => literal_u32(*v, false).map_or(Val::Computed, Val::Known),
        [Tok::Punct("-"), Tok::Num(Some(v), true)] => literal_u32(*v, true).map_or(Val::Computed, Val::Known),
        [Tok::Ident(local)] => held.get(local).copied().map_or(Val::Computed, Val::Known),
        [Tok::Punct("*"), .., Tok::Punct(")")] => {
            // `*(T *)(base ± K)`: the base is the identifier after the last `(`
            let Some(open) = (s..e).rev().find(|&j| matches!(toks[j], Tok::Punct("("))) else { return Val::Computed };
            match toc_load(toks, open + 1, toc, held, mem) {
                Some(w) if (open + 1..e).filter(|&j| matches!(toks[j], Tok::Punct(")"))).count() == 1 => Val::Known(w),
                _ => Val::Computed,
            }
        }
        _ => Val::Computed,
    }
}

/// One call a decomp writes: the module it passes, and the address of the name.
#[derive(Debug, PartialEq, Eq)]
struct DecompCall {
    module: Val<u32>,
    name: Val<u32>,
}

/// The calls a function's decomp makes to `callee`, in text order.
fn decomp_calls(decomp: &str, va: u32, callee: &str, caller: &Caller, mem: &dyn Memory) -> Result<Vec<DecompCall>, Error> {
    let arg = |p: &Passed| match p {
        Passed::Arg(k) => Ok(Some(*k)),
        Passed::Object => Ok(None),
        _ => Err(Error::new(format!("{}: a console image's caller is read from the decomp's arguments", caller.image_id))),
    };
    let (name_arg, module_arg) = (arg(&caller.name)?, arg(&caller.module)?);
    let toks = tokenize(decomp);
    let toc = mem.toc(va);
    let mut held: HashMap<String, u32> = HashMap::new();
    let mut out = Vec::new();
    for i in 0..toks.len() {
        let Tok::Ident(w) = &toks[i] else { continue };
        if let Some(word) = toc_load(&toks, i, toc, &held, mem) {
            if let Some(local) = assigned_local(&toks, i) {
                held.insert(local, word);
            }
        }
        if w == callee && matches!(toks.get(i + 1), Some(Tok::Punct("("))) {
            let args = call_args(&toks, i + 1)
                .ok_or_else(|| Error::new(format!("{} 0x{va:08x}: a call to {callee} whose parentheses do not close", caller.image_id)))?;
            let get = |k: Option<usize>| match k {
                None => Val::Object,
                Some(k) => args.get(k).map_or(Val::Unread, |&(s, e)| arg_value(&toks, s, e, toc, &held, mem)),
            };
            out.push(DecompCall { module: get(module_arg), name: get(name_arg) });
        }
    }
    Ok(out)
}

/// The calls of a PowerPC image to `caller`.
pub fn ppc_calls(image: &[u8], functions: &Functions, caller: &Caller) -> Result<(usize, Vec<Call>), Error> {
    let mem = Mem::of(image)?;
    let im = ImageMemory::of(image)?;
    let FnRecord { name: callee, callers: listed, .. } = functions
        .by_va
        .get(&caller.va)
        .ok_or_else(|| Error::new(format!("{}: the functions export has no function at 0x{:08x}", caller.image_id, caller.va)))?;
    let scanned = bl_sites(&mem, caller.va);
    if scanned.is_empty() {
        return Err(Error::new(format!("{}: no bl calls 0x{:08x}; the callers row is wrong", caller.image_id, caller.va)));
    }
    let missing: Vec<String> = listed.iter().filter(|(s, _)| !scanned.contains(s)).map(|(s, _)| format!("0x{s:08x}")).collect();
    if !missing.is_empty() {
        return Err(Error::new(format!(
            "{}: the export lists call sites to 0x{:08x} that are no bl to it: {}",
            caller.image_id,
            caller.va,
            missing.join(", ")
        )));
    }
    let mut per_fn: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, f) in listed {
        if let Some(f) = f {
            *per_fn.entry(*f).or_default() += 1;
        }
    }
    let mut calls = Vec::new();
    for (&f, &n) in &per_fn {
        let record = functions.by_va.get(&f).ok_or_else(|| {
            Error::new(format!("{}: calling function 0x{f:08x} has no record in the functions export", caller.image_id))
        })?;
        let decomp = record
            .decomp
            .as_deref()
            .ok_or_else(|| Error::new(format!("{}: calling function 0x{f:08x} has no decomp", caller.image_id)))?;
        let found = decomp_calls(decomp, f, callee, caller, &im)?;
        if found.len() != n {
            return Err(Error::new(format!(
                "{} 0x{f:08x}: the export lists {n} calls to {callee} in it, its decomp writes {}",
                caller.image_id,
                found.len()
            )));
        }
        for c in found {
            let name = named(c.name, &im, || format!("{} 0x{f:08x}: a call to {callee}", caller.image_id))?;
            calls.push(Call { function: Some(f), site: None, module: c.module, name });
        }
    }
    // bl sites in no calling function the export lists
    for &s in &scanned {
        if !listed.iter().any(|(site, f)| *site == s && f.is_some()) {
            calls.push(Call { function: functions.holding(s), site: Some(s), module: Val::Unread, name: Val::Unread });
        }
    }
    Ok((scanned.len(), calls))
}

// ---------------------------------------------------------------------------------------------
// Output

fn val_text<T>(v: &Val<T>, known: impl Fn(&T) -> String) -> String {
    match v {
        Val::Known(t) => known(t),
        Val::Computed => "(computed)".into(),
        Val::Unread => "(unread)".into(),
        Val::Object => "(object)".into(),
    }
}

/// The calls file (see the module documentation).
pub fn to_tsv(images: &[ImageCalls]) -> String {
    let mut out = String::from(
        "# Every call each image's code makes to a Lua script function by name, written by\n\
         # `exemap lua-calls` (mercs2_exemap::luacalls) from each image file below and its Ghidra functions\n\
         # export; the by-name callers are mercs2_exemap's data/lua_callers.tsv.\n",
    );
    for im in images {
        let unread = im.calls.iter().filter(|c| c.name == Val::Unread).count();
        out.push_str(&format!(
            "# image {} sha256 {} caller 0x{:08x}: {} call sites, {} rows, {} unread\n",
            im.image_id,
            im.sha256,
            im.caller,
            im.sites,
            im.calls.len(),
            unread
        ));
    }
    out.push_str(CALLS_HEADER);
    out.push('\n');
    for im in images {
        for c in &im.calls {
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                im.image_id,
                c.function.map_or("-".into(), |f| format!("0x{f:08x}")),
                c.site.map_or("-".into(), |s| format!("0x{s:08x}")),
                val_text(&c.module, |h| format!("0x{h:08x}")),
                val_text(&c.name, |n| n.clone()),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Range;

    /// Code at 0x1000, then data at 0x8000.
    fn x86_mem<'a>(d: &'a [u8], code_len: usize) -> Mem<'a> {
        Mem {
            d,
            big: false,
            ranges: vec![
                Range { lo: 0x1000, hi: 0x1000 + code_len as u32, off: 0, exec: true },
                Range { lo: 0x8000, hi: 0x8000 + (d.len() - code_len) as u32, off: code_len, exec: false },
            ],
            opd: None,
        }
    }

    /// 100 bytes of `nop`, then `mov ecx, 0x8000` / `mov eax, 0x51ee8f14` / `call 0x2000`, then
    /// a call whose module comes from a register and whose block starts with a jump.
    fn code() -> Vec<u8> {
        let mut c = vec![0x90u8; 100];
        let call = |c: &mut Vec<u8>, target: u32| {
            let site = 0x1000 + c.len() as u32;
            c.push(0xE8);
            c.extend_from_slice(&((target as i64 - site as i64 - 5) as i32).to_le_bytes());
        };
        c.push(0xB9);
        c.extend_from_slice(&0x8000u32.to_le_bytes()); // mov ecx, 0x8000
        c.push(0xB8);
        c.extend_from_slice(&0x51ee_8f14u32.to_le_bytes()); // mov eax, 0x51ee8f14
        call(&mut c, 0x2000);
        c.extend_from_slice(&[0x90; 80]);
        c.extend_from_slice(&[0xEB, 0x00]); // jmp +0: the block starts after it
        c.extend_from_slice(&[0x8B, 0xC6]); // mov eax, esi
        c.push(0xB9);
        c.extend_from_slice(&0x8010u32.to_le_bytes()); // mov ecx, 0x8010
        call(&mut c, 0x2000);
        c.extend_from_slice(&[0x90; 8]);
        c
    }

    #[test]
    fn an_x86_call_reads_its_name_and_module_from_the_block_before_it() {
        let code = code();
        let mut data = b"NetSafePlayCheapCinematic\0".to_vec();
        data.resize(0x10, 0);
        data.extend_from_slice(b"SafeEnter\0");
        let d = [code.as_slice(), data.as_slice()].concat();
        let m = x86_mem(&d, code.len());
        let sites = e8_sites(&m, 0x2000);
        assert_eq!(sites, vec![0x1000 + 110, 0x1000 + 115 + 80 + 2 + 2 + 5]);
        let (b, start) = window(&m, sites[0]).expect("decodes");
        assert!(!start, "straight-line nops reach back past the window");
        assert_eq!(reg_value(&b, start, Register::EAX), Val::Known(0x51ee_8f14));
        assert_eq!(reg_value(&b, start, Register::ECX), Val::Known(0x8000));
        assert_eq!(reg_value(&b, start, Register::EDX), Val::Unread);
        let (b, start) = window(&m, sites[1]).expect("decodes");
        assert!(start, "the jump starts the block");
        assert_eq!(reg_value(&b, start, Register::EAX), Val::Computed, "mov eax, esi");
        assert_eq!(reg_value(&b, start, Register::ECX), Val::Known(0x8010));
        assert_eq!(reg_value(&b, start, Register::EDX), Val::Computed, "not written in the block");
    }

    #[test]
    fn a_push_push_ret_call_with_junk_before_its_return_is_a_site_and_reads_its_pushes() {
        let mut c = vec![0x90u8; 100];
        c.extend_from_slice(&[0x6A, 0x02]); // push 2
        c.push(0x68);
        c.extend_from_slice(&0x8000u32.to_le_bytes()); // push 0x8000: the name, pushed last
        let site = 0x1000 + c.len() as u32;
        c.push(0x68);
        c.extend_from_slice(&(site + 11 + 3).to_le_bytes()); // push the return, past 3 junk bytes
        c.push(0x68);
        c.extend_from_slice(&0x2000u32.to_le_bytes()); // push the caller
        c.push(0xC3);
        c.extend_from_slice(&[0xD6, 0xD6, 0xD6, 0x90, 0x90]);
        // the same shape whose return points out of the code is not a call
        c.extend_from_slice(&[0x68, 0, 0, 0, 0x7f, 0x68, 0, 0x20, 0, 0, 0xC3]);
        let mut d = c.clone();
        d.extend_from_slice(b"OnUse\0");
        let m = x86_mem(&d, c.len());
        assert_eq!(push_ret_sites(&m, 0x2000), vec![site]);
        let (b, start) = window(&m, site).expect("decodes");
        assert_eq!(stack_value(&b, start, 0), Val::Known(0x8000));
        assert_eq!(stack_value(&b, start, 1), Val::Known(2));
        assert_eq!(stack_value(&b, start, 2), Val::Unread, "no third push in the window");
    }

    #[test]
    fn a_ppc_decomp_call_reads_literals_and_toc_loads() {
        struct M;
        impl Memory for M {
            fn word(&self, va: u32) -> Option<u32> {
                match va {
                    0x1_0000 => Some(0x2_0000), // the TOC slot holds the table's address
                    0x2_0010 => Some(0x3_0000), // the table holds the name's address
                    _ => None,
                }
            }
            fn text(&self, va: u32) -> Option<String> {
                (va == 0x3_0000).then(|| "NetSafeSetStarter".into())
            }
            fn mapped(&self, _: u32) -> bool {
                true
            }
            fn toc(&self, va: u32) -> Option<u32> {
                (va == 0x500).then_some(0x1_8000)
            }
            fn data_word(&self, _: u32) -> Option<u32> {
                None
            }
            fn data_words(&self) -> Box<dyn Iterator<Item = (u32, u32)> + '_> {
                Box::new(std::iter::empty())
            }
        }
        let decomp = "void f(void) {\n  uVar13 = (ulong)*(uint *)(unaff_r2 + -0x8000);\n  \
                      FUN_005035c0(uVar11,0x51ee8f14,*(undefined4 *)(uVar13 + 0x10),3,0,1,0);\n  \
                      FUN_005035c0(uVar11,*param_1,0xffffffff82000000,2,0,1,0);\n  FUN_005035c0(uVar11);\n}";
        let c = Caller { image_id: "t".into(), va: 0x5035c0, name: Passed::Arg(2), module: Passed::Arg(1), evidence: "-".into() };
        let calls = decomp_calls(decomp, 0x500, "FUN_005035c0", &c, &M).expect("reads");
        assert_eq!(
            calls,
            vec![
                DecompCall { module: Val::Known(0x51ee_8f14), name: Val::Known(0x3_0000) },
                DecompCall { module: Val::Computed, name: Val::Known(0x8200_0000) },
                DecompCall { module: Val::Unread, name: Val::Unread },
            ]
        );
    }

    #[test]
    fn the_embedded_callers_table_parses_against_the_image_table() {
        let catalogue = crate::Catalogue::embedded().expect("catalogue");
        let images = Images::embedded(&catalogue).expect("images");
        let c = Callers::embedded(&images).expect("callers");
        let pc: Vec<&Caller> = c.of("pc-v10").collect();
        assert_eq!(pc[0].va, 0x005a_0cc0);
        assert_eq!(pc[0].name, Passed::Register(Register::ECX));
        assert_eq!(c.of("x360-retail").next().map(|r| r.module.clone()), Some(Passed::Arg(1)));
        let e = Callers::parse(&format!("{CALLERS_HEADER}\npc-v10\t0x005a0cc0\targ2\teax\tx\n"), &images).unwrap_err().to_string();
        assert!(e.contains("not an x86 register"), "{e}");
        let e = Callers::parse(&format!("{CALLERS_HEADER}\nnope\t0x005a0cc0\tecx\teax\tx\n"), &images).unwrap_err().to_string();
        assert!(e.contains("not in images.tsv"), "{e}");
    }
}
