//! Xbox 360 (big-endian) Lua 5.1 bytecode → PC (little-endian) conversion.
//!
//! NOT a byte-swap. Uses unluac's bytecode-level disassemble/assemble round-trip
//! (flip `.endianness BIG`→`LITTLE`), which is bytecode-faithful and robust —
//! mirroring how `audio.rs` shells to `ffmpeg`. A fragile field-by-field endian
//! swap of Lua bytecode has repeatedly caused corruption; this regenerates the
//! chunk structurally via the assembler. Decompile-to-source is NOT used (unluac
//! emits Lua 5.2+ `goto`/labels on complex control flow that the 5.1 compiler
//! rejects) — the bytecode-level disassemble/assemble avoids that.
//!
//! Toolchain: a `java` executable and the unluac jar, both passed in by the caller as a
//! [`LuaTools`].

use std::path::PathBuf;
use std::process::Command;

const LUAQ_SIG: &[u8; 4] = b"\x1bLua";

struct ScopeGuard(PathBuf);
impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn swap32(b: &mut [u8], o: usize) {
    b[o..o + 4].reverse();
}
fn swap16(b: &mut [u8], o: usize) {
    b[o..o + 2].reverse();
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// The external programs that round-trip console Lua bytecode: the `java` executable and the
/// unluac jar it runs (`java -jar <unluac> --disassemble` / `--assemble`). The caller supplies
/// both paths; [`convert_binn_be_to_le`] runs exactly these.
#[derive(Debug, Clone)]
pub struct LuaTools {
    /// The `java` executable.
    pub java: PathBuf,
    /// The unluac jar.
    pub unluac: PathBuf,
}

fn flip_endianness(listing: &[u8]) -> Option<Vec<u8>> {
    for (from, to) in [
        (&b".endianness\tBIG"[..], &b".endianness\tLITTLE"[..]),
        (&b".endianness BIG"[..], &b".endianness LITTLE"[..]),
    ] {
        if let Some(p) = find_sub(listing, from) {
            let mut out = Vec::with_capacity(listing.len() + 8);
            out.extend_from_slice(&listing[..p]);
            out.extend_from_slice(to);
            out.extend_from_slice(&listing[p + from.len()..]);
            return Some(out);
        }
    }
    None
}

/// Round-trip one standalone BE LuaQ bytecode chunk to PC LE via unluac.
fn luaq_be_to_le(be_luaq: &[u8], tools: &LuaTools) -> Result<Vec<u8>, String> {
    if be_luaq.len() < 12 || &be_luaq[0..4] != LUAQ_SIG {
        return Err("not a LuaQ chunk (missing \\x1bLua signature)".into());
    }
    if !tools.java.is_file() {
        return Err(format!("java executable {} is not a file", tools.java.display()));
    }
    if !tools.unluac.is_file() {
        return Err(format!("unluac jar {} is not a file", tools.unluac.display()));
    }
    let java = &tools.java;
    let jar = &tools.unluac;

    let dir = std::env::temp_dir().join(format!(
        "mercs2_lua_{}_{}",
        std::process::id(),
        be_luaq.len()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _g = ScopeGuard(dir.clone());
    let be_path = dir.join("be.luac");
    let luas_path = dir.join("chunk.luas");
    let le_path = dir.join("le.luac");
    std::fs::write(&be_path, be_luaq).map_err(|e| e.to_string())?;

    let dis = Command::new(&java)
        .arg("-jar").arg(&jar).arg("--disassemble").arg(&be_path)
        .output()
        .map_err(|e| format!("java/unluac launch failed: {e}"))?;
    if !dis.status.success() {
        return Err(format!(
            "unluac --disassemble failed: {}",
            String::from_utf8_lossy(&dis.stderr).chars().take(300).collect::<String>()
        ));
    }
    let listing =
        flip_endianness(&dis.stdout).ok_or("no `.endianness BIG` directive in disassembly")?;
    std::fs::write(&luas_path, &listing).map_err(|e| e.to_string())?;

    let asm = Command::new(&java)
        .arg("-jar").arg(&jar).arg("--assemble").arg(&luas_path)
        .arg("--output").arg(&le_path)
        .output()
        .map_err(|e| format!("java/unluac launch failed: {e}"))?;
    if !asm.status.success() || !le_path.is_file() {
        return Err(format!(
            "unluac --assemble failed: {}",
            String::from_utf8_lossy(&asm.stderr).chars().take(300).collect::<String>()
        ));
    }
    let le = std::fs::read(&le_path).map_err(|e| e.to_string())?;
    if le.len() < 12 || &le[0..4] != LUAQ_SIG || le[6] != 1 {
        return Err(format!(
            "assembled chunk is not PC-LE LuaQ (header {:02x?})",
            &le[..le.len().min(12)]
        ));
    }
    Ok(le)
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

/// Convert a BINN chunk body (Lua bytecode container, or a script-reference
/// record) from Xbox BE to PC LE. The LuaQ bytecode is converted via the unluac
/// round-trip; the BINN framing/metadata is per-field swapped. A script-ref
/// (no LuaQ) just byte-swaps its leading u32. Mirrors
/// `tools/ucfx_be_to_le.py::_convert_lua_be_to_le`. Only a body holding LuaQ bytecode runs
/// `tools`.
pub fn convert_binn_be_to_le(be: &[u8], tools: &LuaTools) -> Result<Vec<u8>, String> {
    let scan_end = be.len().saturating_sub(4).min(256);
    let mut luaq_off = None;
    for s in 0..scan_end {
        if &be[s..s + 4] == LUAQ_SIG {
            luaq_off = Some(s);
            break;
        }
    }
    let luaq_off = match luaq_off {
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
    let le_luaq = luaq_be_to_le(&be[luaq_off..], tools)?;
    head.extend_from_slice(&le_luaq);
    Ok(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_paths::{local_config_java_binary, local_config_unluac_jar};
    use std::path::Path;

    /// The `java_binary` and `unluac_jar` keys of `.mercs2-local.toml`. Panics naming the missing
    /// key when either is absent.
    fn local_tools() -> LuaTools {
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        LuaTools {
            java: local_config_java_binary(start).unwrap_or_else(|e| panic!("{e}")),
            unluac: local_config_unluac_jar(start).unwrap_or_else(|e| panic!("{e}")),
        }
    }

    /// Tool paths that do not exist. A body without a LuaQ signature returns from
    /// `convert_binn_be_to_le` before `luaq_be_to_le` checks or runs the tools.
    fn absent_tools() -> LuaTools {
        LuaTools {
            java: PathBuf::from("absent/java"),
            unluac: PathBuf::from("absent/unluac.jar"),
        }
    }

    #[test]
    fn test_empty_binn_body() {
        let empty = vec![];
        let result = convert_binn_be_to_le(&empty, &absent_tools());
        assert_eq!(result, Ok(Vec::new()));
    }

    #[test]
    fn test_short_binn_body() {
        let short = vec![0u8; 2];
        let result = convert_binn_be_to_le(&short, &absent_tools());
        assert_eq!(result, Ok(short));
    }

    #[test]
    fn test_luaq_signature_detection() {
        // Body with Lua signature: starts with junk, then \x1bLua
        let mut data = vec![0u8; 100];
        // Insert Lua signature at offset 50
        data[50..54].copy_from_slice(b"\x1bLua");
        // This runs the bytecode through unluac with the configured tools.
        let result = convert_binn_be_to_le(&data, &local_tools());
        // The result is not checked; the junk chunk's conversion outcome is unluac's.
        let _ = result;
    }

    #[test]
    fn test_no_luaq_signature_skips_java() {
        // Body without Lua signature: only the leading u32 is byte-swapped, no tool runs.
        let mut data = vec![0u8; 100];
        data[0..4].copy_from_slice(&[1, 2, 3, 4]);
        let output = convert_binn_be_to_le(&data, &absent_tools())
            .expect("a body without a LuaQ signature must convert without the tools");
        let mut want = data.clone();
        want[0..4].copy_from_slice(&[4, 3, 2, 1]);
        assert_eq!(output, want);
    }

    #[test]
    fn a_luaq_body_with_a_missing_tool_names_the_path() {
        let mut data = vec![0u8; 100];
        data[0..4].copy_from_slice(LUAQ_SIG);
        let e = convert_binn_be_to_le(&data, &absent_tools()).unwrap_err();
        assert!(e.contains("absent/java"), "{e}");
        let tools = LuaTools { unluac: PathBuf::from("absent/unluac.jar"), ..local_tools() };
        let e = convert_binn_be_to_le(&data, &tools).unwrap_err();
        assert!(e.contains("absent/unluac.jar"), "{e}");
    }
}
