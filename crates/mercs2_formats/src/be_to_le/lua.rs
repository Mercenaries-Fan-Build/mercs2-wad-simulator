//! Console (big-endian, Xbox 360 and PS3) Lua 5.1 bytecode → PC (little-endian) conversion.
//!
//! NOT a byte-swap in place. The chunk is decoded whole by [`crate::luachunk`], which reads either
//! byte order and refuses anything it does not consume exactly, its byte order is set to little,
//! and it is encoded again. A field-by-field swap in place is what corrupted chunks before; a
//! structural decode/encode cannot miss a field.
//!
//! This replaced a round-trip through unluac's disassembler and assembler (java). Over every Lua
//! chunk of the Blow It Up Again DLC, 34 on PS3 and the same 34 on Xbox 360, the two produced the
//! same bytes; `mercs2_probe lua-convert-check` repeats that comparison against a package.

use crate::ffcs::Endian;
use crate::luachunk;

const LUAQ_SIG: &[u8; 4] = b"\x1bLua";

fn swap32(b: &mut [u8], o: usize) {
    b[o..o + 4].reverse();
}
fn swap16(b: &mut [u8], o: usize) {
    b[o..o + 2].reverse();
}

/// One standalone big-endian LuaQ chunk, in PC's little-endian order: decoded whole, its byte order
/// set to little, encoded again. A chunk that does not decode exactly, or is already little-endian,
/// is refused.
pub fn luaq_be_to_le(be_luaq: &[u8]) -> Result<Vec<u8>, String> {
    let mut chunk = luachunk::decode(be_luaq).map_err(|e| e.to_string())?;
    if chunk.endian != Endian::Big {
        return Err("the Lua chunk is already little-endian; a console chunk is big-endian".into());
    }
    chunk.endian = Endian::Little;
    Ok(luachunk::encode(&chunk))
}

/// BINN metadata before the LuaQ signature. Ports `_convert_binn_pre_luaq`
/// (tools/ucfx_be_to_le.py): swap the leading u32, the u16 @13, the dep-id u32
/// at `luaq_off-4`, and the u32 dependency-hash array, leaving the NUL-terminated
/// name string. `head` is exactly the `[0, luaq_off)` slice.
fn convert_binn_pre_luaq(head: &mut [u8]) {
    let luaq_off = head.len();
    if luaq_off < 4 {
        return;
    }
    swap32(head, 0);
    if luaq_off >= 15 {
        swap16(head, 13);
    }
    let nul = match head[15.min(luaq_off)..].iter().position(|&b| b == 0) {
        Some(i) => 15.min(luaq_off) + i,
        None => return,
    };
    if nul >= luaq_off {
        return;
    }
    let dep_end = luaq_off - 4;
    swap32(head, dep_end);
    let mut pos = nul + 1;
    if pos < dep_end {
        pos += 1; // dep_count byte
        while pos < dep_end && (dep_end - pos) % 4 != 0 {
            pos += 1;
        }
        while pos + 4 <= dep_end {
            swap32(head, pos);
            pos += 4;
        }
    }
}

/// Where a BINN body's LuaQ bytecode starts: the first `\x1bLua` in its first 256 bytes, after the
/// BINN metadata. `None` for a script-reference record, which holds no bytecode. The bytecode runs
/// to the body's end.
pub fn luaq_offset(body: &[u8]) -> Option<usize> {
    let scan_end = body.len().saturating_sub(4).min(256);
    (0..scan_end).find(|&s| &body[s..s + 4] == LUAQ_SIG)
}

/// Convert a BINN chunk body (Lua bytecode container, or a script-reference
/// record) from console BE to PC LE. The LuaQ bytecode is converted by
/// [`luaq_be_to_le`]; the BINN framing/metadata is per-field swapped. A script-ref
/// (no LuaQ) just byte-swaps its leading u32. Mirrors
/// `tools/ucfx_be_to_le.py::_convert_lua_be_to_le`.
pub fn convert_binn_be_to_le(be: &[u8]) -> Result<Vec<u8>, String> {
    let luaq_off = match luaq_offset(be) {
        None => {
            // Script-reference record: only the leading u32 is byte-swapped.
            let mut out = be.to_vec();
            if out.len() >= 4 {
                swap32(&mut out, 0);
            }
            return Ok(out);
        }
        Some(o) => o,
    };
    let mut head = be[..luaq_off].to_vec();
    if luaq_off > 0 {
        convert_binn_pre_luaq(&mut head);
    }
    let le_luaq = luaq_be_to_le(&be[luaq_off..])?;
    head.extend_from_slice(&le_luaq);
    Ok(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::luachunk::{Chunk, Constant, LocVar, Proto};

    /// A two-function chunk with every constant type, line info, a local and an upvalue.
    fn chunk(endian: Endian) -> Chunk {
        let inner = Proto {
            source: None,
            line_defined: 3,
            last_line_defined: 5,
            nups: 1,
            num_params: 2,
            is_vararg: 0,
            max_stack: 4,
            code: vec![0x0000_001e],
            constants: vec![],
            protos: vec![],
            line_info: vec![5],
            loc_vars: vec![LocVar { name: b"a".to_vec(), start_pc: 0, end_pc: 1 }],
            upvalues: vec![b"up".to_vec()],
        };
        Chunk {
            endian,
            main: Proto {
                source: None,
                line_defined: 0,
                last_line_defined: 0,
                nups: 0,
                num_params: 0,
                is_vararg: 2,
                max_stack: 2,
                code: vec![0x0000_0024, 0x0080_001e],
                constants: vec![
                    Constant::Nil,
                    Constant::Boolean(true),
                    Constant::Number(1.5f32.to_bits()),
                    Constant::String(b"CallWithOptionalArgs".to_vec()),
                ],
                protos: vec![inner],
                line_info: vec![1, 2],
                loc_vars: vec![],
                upvalues: vec![],
            },
        }
    }

    #[test]
    fn test_empty_binn_body() {
        let empty = vec![];
        let result = convert_binn_be_to_le(&empty);
        assert_eq!(result, Ok(Vec::new()));
    }

    #[test]
    fn test_short_binn_body() {
        let short = vec![0u8; 2];
        let result = convert_binn_be_to_le(&short);
        assert_eq!(result, Ok(short));
    }

    #[test]
    fn test_no_luaq_signature_swaps_only_the_leading_word() {
        // Body without Lua signature: a script reference, whose leading u32 alone is byte-swapped.
        let mut data = vec![0u8; 100];
        data[0..4].copy_from_slice(&[1, 2, 3, 4]);
        let output = convert_binn_be_to_le(&data).expect("a body without a LuaQ signature converts");
        let mut want = data.clone();
        want[0..4].copy_from_slice(&[4, 3, 2, 1]);
        assert_eq!(output, want);
    }

    /// A big-endian chunk after BINN metadata comes out as the same chunk little-endian, the
    /// metadata swapped as before.
    #[test]
    fn a_big_endian_chunk_converts_to_the_same_chunk_little_endian() {
        let mut head = vec![0u8; 24];
        head[0..4].copy_from_slice(&[0, 0, 0, 7]);
        head[15..19].copy_from_slice(b"ess\0");
        head[20..24].copy_from_slice(&[0, 0, 0, 9]);
        let mut be = head.clone();
        be.extend_from_slice(&luachunk::encode(&chunk(Endian::Big)));
        assert_eq!(luaq_offset(&be), Some(24));

        let out = convert_binn_be_to_le(&be).expect("converts");
        let mut want_head = head;
        convert_binn_pre_luaq(&mut want_head);
        assert_eq!(&out[..24], &want_head[..]);
        assert_eq!(&out[..4], &[7, 0, 0, 0]);
        assert_eq!(&out[20..24], &[9, 0, 0, 0]);
        assert_eq!(&out[24..], &luachunk::encode(&chunk(Endian::Little))[..]);
        assert_eq!(luachunk::decode(&out[24..]).unwrap(), chunk(Endian::Little));
    }

    /// Bytes after the signature that are not a whole chunk are refused, not passed through.
    #[test]
    fn a_signature_followed_by_junk_is_refused() {
        let mut data = vec![0u8; 100];
        data[50..54].copy_from_slice(LUAQ_SIG);
        assert!(convert_binn_be_to_le(&data).is_err());

        let mut trailing = luachunk::encode(&chunk(Endian::Big));
        trailing.push(0);
        assert!(luaq_be_to_le(&trailing).is_err());
    }

    /// A chunk that is already little-endian is not a console chunk.
    #[test]
    fn a_little_endian_chunk_is_refused() {
        let e = luaq_be_to_le(&luachunk::encode(&chunk(Endian::Little))).unwrap_err();
        assert!(e.contains("already little-endian"), "{e}");
    }
}
