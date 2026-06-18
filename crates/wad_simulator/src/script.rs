//! Script UCFX consumption (LuaQ / BINN).

use crate::consume::ConsumeResult;
use mercs2_formats::ucfx::extract_chunk_body;

const LUAQ_MAGIC: &[u8] = b"\x1BLua";
const BINN_MAGIC: &[u8; 4] = b"BINN";

// Lua 5.1 header sizeof block: [sizeof int, sizeof size_t, sizeof Instruction,
// sizeof lua_Number, integral-flag]. The mercs2 Lua fork (and retail pc-game-vz.wad,
// verified: BINN bodies are `\x1BLuaQ\x00\x01\x04\x04\x04\x04\x00…`) uses a 32-bit
// build with FLOAT lua_Number, so lua_Number is 4 bytes, not the desktop default 8.
// (The old `8` was never exercised — scripts ship payload under the BINN chunk, and
// the previous "no data chunk" early-return skipped this check entirely.)
const EXPECTED_SIZEOF_FIELDS: [u8; 5] = [4, 4, 4, 4, 0];
const MAX_REASONABLE_SIZE: u32 = 100_000;

pub fn consume_script(container: &[u8], data_body: Option<&[u8]>, label: &str) -> ConsumeResult {
    let mut issues = Vec::new();
    let mut structural_violations = 0u32;
    // Script containers ship their LuaQ payload under the `BINN` chunk tag, not
    // `data` — verified against retail pc-game-vz.wad (402/402 sampled script
    // containers are (INFO, BINN) or (INFO, DEPS, BINN), BINN body = `\x1BLuaQ`
    // bytecode). The old "no data chunk" error here was a false positive that ALSO
    // skipped payload validation entirely; falling back to the BINN chunk turns the
    // LuaQ checks (version/endian/sizeof/proto walk) on for these scripts.
    let binn_owned;
    let body: &[u8] = match data_body {
        Some(b) => b,
        None => match extract_chunk_body(container, b"BINN") {
            Some(b) => {
                binn_owned = b;
                &binn_owned
            }
            None => {
                issues.push(format!("{label}: no script payload chunk (data or BINN)"));
                return ConsumeResult {
                    consumed: true,
                    issues,
                    structural_violations: 1,
                    ..Default::default()
                };
            }
        },
    };

    if body.len() >= 5 && body.starts_with(LUAQ_MAGIC) {
        if body.len() >= 12 {
            let version = body[4];
            if version != 0x51 {
                issues.push(format!("{label}: Lua bytecode version 0x{version:02X} (expected 0x51)"));
                structural_violations += 1;
            }

            let endian_flag = body[6];
            if endian_flag != 1 {
                issues.push(format!("{label}: LuaQ endianness flag {endian_flag} (expected 1 = LE)"));
                structural_violations += 1;
            }

            if body[7..12] != EXPECTED_SIZEOF_FIELDS {
                issues.push(format!(
                    "{label}: LuaQ sizeof fields {:?} (expected {:?})",
                    &body[7..12],
                    EXPECTED_SIZEOF_FIELDS
                ));
                structural_violations += 1;
            }

            if let Err(e) = walk_lua_proto(body, 12) {
                issues.push(format!("{label}: LuaQ proto walk failed: {e}"));
                structural_violations += 1;
            }
        }
    } else if body.len() >= 4 && &body[0..4] == BINN_MAGIC {
        // BINN script container — OK
    } else if body.len() < 8 {
        issues.push(format!("{label}: data too small for LuaQ/BINN"));
    } else {
        issues.push(format!(
            "{label}: unknown script header {:?}",
            &body[..body.len().min(8)]
        ));
    }

    ConsumeResult {
        consumed: true,
        issues,
        structural_violations,
        ..Default::default()
    }
}

fn read_u32(data: &[u8], off: usize) -> Option<u32> {
    if off + 4 > data.len() {
        return None;
    }
    Some(u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]))
}

/// Recursively walk a Lua 5.1 proto structure, returning the offset after the proto.
fn walk_lua_proto(data: &[u8], start: usize) -> Result<usize, String> {
    let mut pos = start;

    // source_name: size_t(4) + bytes
    let name_len = read_u32(data, pos).ok_or("overrun at source_name len")? as usize;
    pos += 4;
    pos = pos.checked_add(name_len).ok_or("overrun at source_name data")?;
    if pos > data.len() {
        return Err("overrun after source_name".into());
    }

    // linedefined, lastlinedefined: 2 x int(4)
    pos = pos.checked_add(8).ok_or("overrun at linedefined")?;
    if pos > data.len() {
        return Err("overrun after linedefined".into());
    }

    // nups, numparams, is_vararg, maxstacksize: 4 x u8
    pos = pos.checked_add(4).ok_or("overrun at params")?;
    if pos > data.len() {
        return Err("overrun after params".into());
    }

    // sizecode (int) + instructions (sizecode * 4)
    let sizecode = read_u32(data, pos).ok_or("overrun at sizecode")?;
    if sizecode > MAX_REASONABLE_SIZE {
        return Err(format!("sizecode {sizecode} implausible"));
    }
    pos += 4;
    pos = pos.checked_add(sizecode as usize * 4).ok_or("overrun at instructions")?;
    if pos > data.len() {
        return Err("overrun after instructions".into());
    }

    // sizek (int) + constants
    let sizek = read_u32(data, pos).ok_or("overrun at sizek")?;
    if sizek > MAX_REASONABLE_SIZE {
        return Err(format!("sizek {sizek} implausible"));
    }
    pos += 4;
    for _ in 0..sizek {
        if pos >= data.len() {
            return Err("overrun in constants".into());
        }
        let type_byte = data[pos];
        pos += 1;
        match type_byte {
            0 => {} // nil
            1 => {
                // bool: 1 byte
                pos += 1;
                if pos > data.len() {
                    return Err("overrun in bool constant".into());
                }
            }
            3 => {
                // number: sizeof lua_Number = 4 (mercs2 Lua uses FLOAT, per the
                // header's [4,4,4,4,0] sizeof block — verified retail + DLC). The
                // old `8` (desktop double) ran the walk 4 bytes off after the first
                // number constant, so subsequent string bytes were misread as type
                // tags ("unknown constant type 110/115/…").
                pos += 4;
                if pos > data.len() {
                    return Err("overrun in number constant".into());
                }
            }
            4 => {
                // string: size_t(4) + bytes
                let slen = read_u32(data, pos).ok_or("overrun at string constant len")? as usize;
                pos += 4;
                pos = pos.checked_add(slen).ok_or("overrun at string constant data")?;
                if pos > data.len() {
                    return Err("overrun after string constant".into());
                }
            }
            _ => {
                return Err(format!("unknown constant type {type_byte}"));
            }
        }
    }

    // sizep (int) + sub-protos (recurse)
    let sizep = read_u32(data, pos).ok_or("overrun at sizep")?;
    if sizep > MAX_REASONABLE_SIZE {
        return Err(format!("sizep {sizep} implausible"));
    }
    pos += 4;
    for _ in 0..sizep {
        pos = walk_lua_proto(data, pos)?;
    }

    // sizelineinfo (int) + lineinfo (sizelineinfo * 4)
    let sizelineinfo = read_u32(data, pos).ok_or("overrun at sizelineinfo")?;
    if sizelineinfo > MAX_REASONABLE_SIZE {
        return Err(format!("sizelineinfo {sizelineinfo} implausible"));
    }
    pos += 4;
    pos = pos.checked_add(sizelineinfo as usize * 4).ok_or("overrun at lineinfo")?;
    if pos > data.len() {
        return Err("overrun after lineinfo".into());
    }

    // sizelocvars (int) + locvars (each: string + startpc(int) + endpc(int))
    let sizelocvars = read_u32(data, pos).ok_or("overrun at sizelocvars")?;
    if sizelocvars > MAX_REASONABLE_SIZE {
        return Err(format!("sizelocvars {sizelocvars} implausible"));
    }
    pos += 4;
    for _ in 0..sizelocvars {
        let vname_len = read_u32(data, pos).ok_or("overrun at locvar name len")? as usize;
        pos += 4;
        pos = pos.checked_add(vname_len).ok_or("overrun at locvar name")?;
        if pos > data.len() {
            return Err("overrun after locvar name".into());
        }
        pos += 8; // startpc + endpc
        if pos > data.len() {
            return Err("overrun after locvar pcs".into());
        }
    }

    // sizeupvalues (int) + upvalues (each: string = size_t + bytes)
    let sizeupvalues = read_u32(data, pos).ok_or("overrun at sizeupvalues")?;
    if sizeupvalues > MAX_REASONABLE_SIZE {
        return Err(format!("sizeupvalues {sizeupvalues} implausible"));
    }
    pos += 4;
    for _ in 0..sizeupvalues {
        let uname_len = read_u32(data, pos).ok_or("overrun at upvalue name len")? as usize;
        pos += 4;
        pos = pos.checked_add(uname_len).ok_or("overrun at upvalue name")?;
        if pos > data.len() {
            return Err("overrun after upvalue name".into());
        }
    }

    Ok(pos)
}
