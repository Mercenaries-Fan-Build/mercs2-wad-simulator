//! Recognising an MSVC scalar deleting destructor by its code: the function that tests bit 0 of its
//! flags argument (the "delete the storage too" bit) before returning.
//!
//! * **PowerPC** (Xbox 360): the flags arrive in `r4`. The register is followed through copies
//!   (`mr rA,rS` = `or rA,rS,rS`), loses copies a `bl` clobbers (`r3`–`r12`) or another
//!   instruction overwrites, and the test is `rlwinm rX,<holder>,0,31,31`.
//! * **x86** (PC, `__thiscall`): the flags are the first stack argument, `[esp+4]` on entry. The
//!   stack depth is tracked through `push`/`pop`, `sub/add esp,imm` and the bytes a called function
//!   pops with `ret N`, and an `ebp` frame (`mov ebp,esp`) is followed; the test is `test`/`and`
//!   of that argument with 1, directly or through a register loaded from it.
//!
//! Decoding is linear from the entry to the first return, at most 512 instructions.

use iced_x86::{Decoder, DecoderOptions, Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register};

use crate::mem::Mem;

/// Whether the function at `f` tests bit 0 of its flags argument.
pub(crate) fn tests_delete_flag(mem: &Mem<'_>, f: u32) -> bool {
    if mem.big {
        ppc(mem, f)
    } else {
        x86(mem, f)
    }
}

fn ppc(mem: &Mem<'_>, f: u32) -> bool {
    let mut holders: u32 = 1 << 4;
    for i in 0..512u32 {
        let Some(w) = mem.word(f + 4 * i) else { return false };
        if w == 0x4e80_0020 {
            return false; // blr
        }
        let op = w >> 26;
        let rs = (w >> 21) & 31;
        let ra = (w >> 16) & 31;
        match op {
            21 => {
                // rlwinm rA,rS,SH,MB,ME
                let (sh, mb, me) = ((w >> 11) & 31, (w >> 6) & 31, (w >> 1) & 31);
                if holders & (1 << rs) != 0 && sh == 0 && mb == 31 && me == 31 {
                    return true;
                }
                holders &= !(1 << ra);
            }
            31 if (w >> 1) & 0x3ff == 444 => {
                // or rA,rS,rB (mr when rS == rB)
                let rb = (w >> 11) & 31;
                if rs == rb && holders & (1 << rs) != 0 {
                    holders |= 1 << ra;
                } else {
                    holders &= !(1 << ra);
                }
            }
            18 if w & 1 == 1 => holders &= !0x1ff8, // bl: r3..r12 are volatile
            // D-form instructions whose target register is bits 21..25.
            7 | 8 | 12 | 13 | 14 | 15 | 32 | 33 | 34 | 35 | 40 | 41 | 42 | 43 | 46 | 58 => holders &= !(1 << rs),
            // D-form logical immediates write rA.
            24 | 25 | 26 | 27 | 28 | 29 | 20 | 23 => holders &= !(1 << ra),
            _ => {}
        }
        if holders == 0 {
            return false;
        }
    }
    false
}

/// The bytes the function at `f` pops on return (`ret N`), 0 for a plain `ret`.
fn x86_ret_pop(mem: &Mem<'_>, f: u32) -> Option<u32> {
    let off = mem.off(f)?;
    let bytes = mem.d.get(off..(off + 0x4000).min(mem.d.len()))?;
    let mut dec = Decoder::with_ip(32, bytes, f as u64, DecoderOptions::NONE);
    let mut ins = Instruction::default();
    for _ in 0..4096 {
        if !dec.can_decode() {
            return None;
        }
        dec.decode_out(&mut ins);
        if ins.mnemonic() == Mnemonic::Ret {
            return Some(if ins.op_count() > 0 { ins.immediate16() as u32 } else { 0 });
        }
        if ins.is_invalid() || ins.mnemonic() == Mnemonic::Int3 {
            return None;
        }
    }
    None
}

fn x86(mem: &Mem<'_>, f: u32) -> bool {
    let Some(off) = mem.off(f) else { return false };
    let Some(bytes) = mem.d.get(off..(off + 0x2000).min(mem.d.len())) else { return false };
    let mut dec = Decoder::with_ip(32, bytes, f as u64, DecoderOptions::NONE);
    let mut info = InstructionInfoFactory::new();
    let mut ins = Instruction::default();
    // Bytes pushed since entry; the flags argument is at [esp + 4 + depth].
    let mut depth: i64 = 0;
    let mut frame: Option<i64> = None;
    let mut holders: Vec<Register> = Vec::new();
    let is_arg = |ins: &Instruction, depth: i64, frame: Option<i64>| -> bool {
        if ins.memory_index() != Register::None {
            return false;
        }
        let disp = ins.memory_displacement32() as i32 as i64;
        match ins.memory_base() {
            Register::ESP => disp == 4 + depth,
            Register::EBP => frame.is_some_and(|d0| disp == 4 + d0),
            _ => false,
        }
    };
    for _ in 0..512 {
        if !dec.can_decode() {
            return false;
        }
        dec.decode_out(&mut ins);
        if ins.is_invalid() {
            return false;
        }
        match ins.mnemonic() {
            Mnemonic::Ret | Mnemonic::Int3 | Mnemonic::Jmp => return false,
            Mnemonic::Test | Mnemonic::And => {
                let imm_one = ins.op_count() == 2 && ins.op1_kind() != OpKind::Register && ins.op1_kind() != OpKind::Memory && ins.immediate(1) == 1;
                if imm_one {
                    if ins.op0_kind() == OpKind::Memory && is_arg(&ins, depth, frame) {
                        return true;
                    }
                    if ins.op0_kind() == OpKind::Register && holders.contains(&ins.op0_register().full_register32()) {
                        return true;
                    }
                }
            }
            Mnemonic::Mov | Mnemonic::Movzx if ins.op0_kind() == OpKind::Register && ins.op1_kind() == OpKind::Memory && is_arg(&ins, depth, frame) => {
                let r = ins.op0_register().full_register32();
                if !holders.contains(&r) {
                    holders.push(r);
                }
                continue;
            }
            Mnemonic::Mov if ins.op0_register() == Register::EBP && ins.op1_register() == Register::ESP => {
                frame = Some(depth);
                continue;
            }
            _ => {}
        }
        // Stack depth.
        match ins.mnemonic() {
            Mnemonic::Push => depth += 4,
            Mnemonic::Pop => depth -= 4,
            Mnemonic::Sub if ins.op0_register() == Register::ESP && ins.op1_kind() != OpKind::Register => depth += ins.immediate(1) as i64,
            Mnemonic::Add if ins.op0_register() == Register::ESP && ins.op1_kind() != OpKind::Register => depth -= ins.immediate(1) as i64,
            Mnemonic::Call => {
                if ins.op0_kind() == OpKind::NearBranch32 {
                    match x86_ret_pop(mem, ins.near_branch32()) {
                        Some(n) => depth -= n as i64,
                        None => return false,
                    }
                } else {
                    // An indirect call's callee is unknown, so the stack depth after it is too.
                    return false;
                }
                holders.retain(|r| !matches!(r, Register::EAX | Register::ECX | Register::EDX));
                continue;
            }
            _ => {}
        }
        // Drop holders the instruction overwrites.
        let used = info.info(&ins);
        for u in used.used_registers() {
            if matches!(u.access(), OpAccess::Write | OpAccess::CondWrite | OpAccess::ReadWrite | OpAccess::ReadCondWrite) {
                let r = u.register().full_register32();
                holders.retain(|h| *h != r);
            }
        }
    }
    false
}
