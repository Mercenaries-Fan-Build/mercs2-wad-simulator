//! `splice-emulate`: run a SecuROM splice through a decrypted memory dump of the game and report what
//! the code behind it did — the method `docs/reverse_engineer/audio_code_map.md` §10 uses.
//!
//! A splice replaces game code with a jump into SecuROM, whose VM runs the stolen instructions and
//! comes back to game code. The dump (`output/_ghidra/securom_dump/image.bin`, a capture's
//! `image.bin`; raw = virtual layout from `0x400000`) holds the VM decrypted, so running it from the
//! splice with every register and the return address set to sentinels shows, by what it read, wrote,
//! called and left in the registers, what the hidden code is.

use std::collections::BTreeMap;
use std::path::Path;

use unicorn_engine::{Arch, HookType, MemType, Mode, Prot, RegisterX86, Unicorn};

/// Where the dump maps.
const IMAGE_BASE: u64 = 0x0040_0000;
/// The game's own code and data, below SecuROM's sections (`Stext` starts at `0x01648000` or later
/// in every catalogued layout): reads and writes here are what the report is about.
const GAME_END: u64 = 0x0164_0000;
/// Game code, where a transfer out of SecuROM lands when the VM calls back into the game.
const TEXT: (u64, u64) = (0x0040_1000, 0x00B0_5000);
/// The import table, searched to name the import an unmapped jump went to.
const IAT: (u64, u64) = (0x00B0_5000, 0x00B0_6000);
const STACK_BASE: u64 = 0x0F00_0000;
const STACK_SIZE: u64 = 0x0010_0000;
/// The initial stack pointer, and the return address it holds: reaching it means the code returned.
const STACK_TOP: u64 = STACK_BASE + 0x000F_0000;
const RETURN_SENTINEL: u64 = 0x0EE0_0000;
/// Sentinel register values: each points at its own unmapped region, so an access through it shows
/// which register it came from (`[0xA0000110]` is `[eax+0x110]`).
const SENTINELS: [(RegisterX86, &str, u64); 7] = [
    (RegisterX86::EAX, "eax", 0xA000_0000),
    (RegisterX86::ECX, "ecx", 0xC000_0000),
    (RegisterX86::EDX, "edx", 0xD000_0000),
    (RegisterX86::EBX, "ebx", 0xB000_0000),
    (RegisterX86::EBP, "ebp", 0x5300_0000),
    (RegisterX86::ESI, "esi", 0x5100_0000),
    (RegisterX86::EDI, "edi", 0x5200_0000),
];
const LOG_LIMIT: usize = 400;

#[derive(Default)]
struct State {
    accesses: Vec<String>,
    /// Game-code addresses entered from SecuROM, with how often.
    into_game: BTreeMap<u64, u32>,
    last_block_in_game: bool,
    stop: Option<String>,
    mapped_on_demand: Vec<u64>,
}

fn register(name: &str) -> Result<RegisterX86, String> {
    SENTINELS.iter().find(|(_, n, _)| *n == name).map(|(r, _, _)| *r).ok_or_else(|| format!("no register {name:?}"))
}

fn number(s: &str) -> Result<u64, String> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => s.parse(),
    }
    .map_err(|e| format!("{s:?}: {e}"))
}

fn in_game(address: u64) -> bool {
    (IMAGE_BASE..GAME_END).contains(&address)
}

fn watched(address: u64) -> bool {
    in_game(address) || address >= 0x5000_0000 || (STACK_TOP..STACK_BASE + STACK_SIZE).contains(&address)
}

/// `args`: `<image.bin> <start VA> [--reg name=value]... [--arg value]... [--byte address=value]...
/// [--dword address=value]... [--steps N]`; `--byte`/`--dword` set image memory before the run.
pub fn run(args: &[String]) -> Result<(), String> {
    let (Some(image_path), Some(start)) = (args.first(), args.get(1)) else {
        return Err("usage: splice-emulate <image.bin> <start VA> [--reg name=value]... [--arg value]... [--byte address=value]... [--dword address=value]... [--steps N]".into());
    };
    let start = number(start)?;
    let mut overrides = Vec::new();
    let mut stack_args = Vec::new();
    let mut steps = 2_000_000usize;
    let mut pokes: Vec<(u64, Vec<u8>)> = Vec::new();
    let mut rest = args[2..].iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let poke = |value: &str| -> Result<(u64, u64), String> {
            let (address, v) = value.split_once('=').ok_or_else(|| format!("{flag} {value}: not address=value"))?;
            Ok((number(address)?, number(v)?))
        };
        match flag.as_str() {
            "--reg" => {
                let (name, v) = value.split_once('=').ok_or_else(|| format!("--reg {value}: not name=value"))?;
                overrides.push((register(name)?, number(v)?));
            }
            "--arg" => stack_args.push(number(value)? as u32),
            "--steps" => steps = number(value)? as usize,
            "--byte" => {
                let (address, v) = poke(value)?;
                pokes.push((address, vec![v as u8]));
            }
            "--dword" => {
                let (address, v) = poke(value)?;
                pokes.push((address, (v as u32).to_le_bytes().to_vec()));
            }
            other => return Err(format!("no option {other}")),
        }
    }
    let mut image = std::fs::read(Path::new(image_path)).map_err(|e| format!("read {image_path}: {e}"))?;
    for (address, bytes) in &pokes {
        let at = address.checked_sub(IMAGE_BASE).filter(|at| (*at as usize) + bytes.len() <= image.len()).ok_or_else(|| format!("0x{address:08X} is outside the image"))?;
        image[at as usize..at as usize + bytes.len()].copy_from_slice(bytes);
    }
    let report = emulate(&image, start, &overrides, &stack_args, steps)?;
    print!("{report}");
    Ok(())
}

fn emulate(image: &[u8], start: u64, overrides: &[(RegisterX86, u64)], stack_args: &[u32], steps: usize) -> Result<String, String> {
    fn fail(what: &'static str) -> impl Fn(unicorn_engine::uc_error) -> String {
        move |e| format!("{what}: {e:?}")
    }
    let mut emu = Unicorn::new_with_data(Arch::X86, Mode::MODE_32, State::default()).map_err(fail("unicorn"))?;
    let size = (image.len() as u64).next_multiple_of(0x1000);
    emu.mem_map(IMAGE_BASE, size, Prot::ALL).map_err(fail("map the image"))?;
    emu.mem_write(IMAGE_BASE, image).map_err(fail("load the image"))?;
    emu.mem_map(STACK_BASE, STACK_SIZE, Prot::ALL).map_err(fail("map the stack"))?;
    let mut stack = (RETURN_SENTINEL as u32).to_le_bytes().to_vec();
    for a in stack_args {
        stack.extend(a.to_le_bytes());
    }
    emu.mem_write(STACK_TOP, &stack).map_err(fail("write the stack"))?;
    let mut initial = BTreeMap::new();
    for (reg, name, value) in SENTINELS {
        let value = overrides.iter().find(|(r, _)| *r == reg).map_or(value, |(_, v)| *v);
        emu.reg_write(reg, value).map_err(fail("set a register"))?;
        initial.insert(name, value);
    }
    emu.reg_write(RegisterX86::ESP, STACK_TOP).map_err(fail("set esp"))?;

    // Touches of unmapped memory: a return to the sentinel ends the run; a jump elsewhere is an
    // import (named by its IAT slot) or a wild jump, and ends it too; data gets a zero page.
    emu.add_mem_hook(HookType::MEM_UNMAPPED, 1, 0, |uc, kind, address, size, _value| {
        if kind == MemType::FETCH_UNMAPPED {
            let reason = if address == RETURN_SENTINEL {
                "returned to the caller".to_string()
            } else {
                let slot = iat_slot(uc, address as u32);
                let esp = uc.reg_read(RegisterX86::ESP).unwrap_or(0);
                let words = uc.mem_read_as_vec(esp, 20).unwrap_or_default();
                let args: Vec<String> = words.chunks(4).skip(1).map(|w| format!("0x{:08X}", u32::from_le_bytes([w[0], w[1], w[2], w[3]]))).collect();
                match slot {
                    Some(slot) => format!("called the import at 0x{address:08X} (IAT slot 0x{slot:08X}); stack args {}", args.join(" ")),
                    None => format!("jumped to unmapped 0x{address:08X}; stack {}", args.join(" ")),
                }
            };
            uc.get_data_mut().stop = Some(reason);
            return false;
        }
        let page = address & !0xFFF;
        let pages = ((address + size as u64 - 1) & !0xFFF) - page + 0x1000;
        uc.get_data_mut().mapped_on_demand.push(page);
        uc.mem_map(page, pages, Prot::ALL).is_ok()
    })
    .map_err(fail("hook unmapped memory"))?;

    emu.add_mem_hook(HookType::MEM_READ | HookType::MEM_WRITE, 1, 0, |uc, kind, address, size, value| {
        if !watched(address) {
            return true;
        }
        let eip = uc.reg_read(RegisterX86::EIP).unwrap_or(0);
        let state = uc.get_data_mut();
        if state.accesses.len() < LOG_LIMIT {
            let line = match kind {
                MemType::WRITE => format!("write [0x{address:08X}] {size}B = 0x{:X}  (eip 0x{eip:08X})", value as u64 & mask(size)),
                _ => format!("read  [0x{address:08X}] {size}B  (eip 0x{eip:08X})"),
            };
            state.accesses.push(line);
        }
        true
    })
    .map_err(fail("hook memory"))?;

    emu.add_block_hook(1, 0, |uc, address, _size| {
        let game = (TEXT.0..TEXT.1).contains(&address);
        let state = uc.get_data_mut();
        if game && !state.last_block_in_game {
            *state.into_game.entry(address).or_default() += 1;
        }
        state.last_block_in_game = game;
    })
    .map_err(fail("hook blocks"))?;

    let result = emu.emu_start(start, 0, 0, steps);
    let mut out = String::new();
    let eip = emu.reg_read(RegisterX86::EIP).unwrap_or(0);
    let stop = emu.get_data().stop.clone();
    out.push_str(&format!("start 0x{start:08X}\n"));
    out.push_str(&match (stop, result) {
        (Some(reason), _) => format!("stopped: {reason}\n"),
        (None, Ok(())) => format!("stopped: {steps} instructions ran out at eip 0x{eip:08X}\n"),
        (None, Err(e)) => format!("stopped: {e:?} at eip 0x{eip:08X}\n"),
    });
    out.push_str("registers that changed:\n");
    for (reg, name, _) in SENTINELS {
        let now = emu.reg_read(reg).unwrap_or(0);
        if now != initial[name] {
            out.push_str(&format!("  {name} 0x{:08X} -> 0x{now:08X}\n", initial[name]));
        }
    }
    let esp = emu.reg_read(RegisterX86::ESP).unwrap_or(0);
    out.push_str(&format!("  esp 0x{STACK_TOP:08X} -> 0x{esp:08X} ({:+})\n", esp as i64 - STACK_TOP as i64));
    let state = emu.get_data();
    out.push_str(&format!("game code entered from SecuROM ({}):\n", state.into_game.len()));
    for (address, count) in &state.into_game {
        out.push_str(&format!("  0x{address:08X} x{count}\n"));
    }
    out.push_str(&format!("reads and writes outside SecuROM ({}{}):\n", state.accesses.len(), if state.accesses.len() >= LOG_LIMIT { ", first ones" } else { "" }));
    for line in &state.accesses {
        out.push_str(&format!("  {line}\n"));
    }
    if !state.mapped_on_demand.is_empty() {
        let pages: Vec<String> = state.mapped_on_demand.iter().map(|p| format!("0x{p:08X}")).collect();
        out.push_str(&format!("pages mapped on first touch: {}\n", pages.join(" ")));
    }
    Ok(out)
}

fn mask(size: usize) -> u64 {
    if size >= 8 {
        u64::MAX
    } else {
        (1u64 << (size * 8)) - 1
    }
}

/// The IAT slot holding `target`, if one does.
fn iat_slot(uc: &Unicorn<State>, target: u32) -> Option<u64> {
    let table = uc.mem_read_as_vec(IAT.0, (IAT.1 - IAT.0) as usize).ok()?;
    table.chunks(4).position(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]) == target).map(|i| IAT.0 + i as u64 * 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in image: at the start, `mov eax, [0x00401100]; mov [0x00401104], eax; ret` reads one
    /// game global, writes another and returns.
    #[test]
    fn a_short_routine_reports_its_read_write_and_return() {
        let mut image = vec![0u8; 0x2000];
        let code = [0xA1, 0x00, 0x11, 0x40, 0x00, 0xA3, 0x04, 0x11, 0x40, 0x00, 0xC3];
        image[0x1000..0x1000 + code.len()].copy_from_slice(&code);
        image[0x1100..0x1104].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        let report = emulate(&image, 0x0040_1000, &[], &[], 100).expect("emulate");
        assert!(report.contains("stopped: returned to the caller"), "{report}");
        assert!(report.contains("eax 0xA0000000 -> 0x12345678"), "{report}");
        assert!(report.contains("read  [0x00401100] 4B"), "{report}");
        assert!(report.contains("write [0x00401104] 4B = 0x12345678"), "{report}");
    }
}
