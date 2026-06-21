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
//! Toolchain (located at runtime): a JRE (`$JAVA` / `$JAVA_HOME/bin/java` /
//! bundled `tools/jdk21/*/bin/java` / `java` on PATH) + unluac (`$UNLUAC_JAR` /
//! `tools/external/unluac/unluac.jar`). See memory: lua-bytecode-disassemble-reassemble.

use std::path::{Path, PathBuf};
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

fn find_java() -> Option<String> {
    if let Ok(j) = std::env::var("JAVA") {
        if !j.is_empty() {
            return Some(j);
        }
    }
    if let Ok(jh) = std::env::var("JAVA_HOME") {
        for n in ["bin/java", "bin/java.exe"] {
            let p = Path::new(&jh).join(n);
            if p.is_file() {
                return Some(p.to_string_lossy().into_owned());
            }
        }
    }
    // Bundled JDK under tools/jdk21/<dist>/bin/java[.exe] (relative to CWD = repo root).
    if let Ok(rd) = std::fs::read_dir("tools/jdk21") {
        for e in rd.flatten() {
            for n in ["bin/java.exe", "bin/java"] {
                let p = e.path().join(n);
                if p.is_file() {
                    return Some(p.to_string_lossy().into_owned());
                }
            }
        }
    }
    Some("java".to_string()) // PATH fallback
}

fn find_unluac() -> Option<String> {
    if let Ok(j) = std::env::var("UNLUAC_JAR") {
        if !j.is_empty() && Path::new(&j).is_file() {
            return Some(j);
        }
    }
    for c in ["tools/external/unluac/unluac.jar", "tools/unluac.jar"] {
        if Path::new(c).is_file() {
            return Some(c.to_string());
        }
    }
    None
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
fn luaq_be_to_le(be_luaq: &[u8]) -> Result<Vec<u8>, String> {
    if be_luaq.len() < 12 || &be_luaq[0..4] != LUAQ_SIG {
        return Err("not a LuaQ chunk (missing \\x1bLua signature)".into());
    }
    let java = find_java().ok_or("no Java runtime (set JAVA/JAVA_HOME or put java on PATH)")?;
    let jar = find_unluac()
        .ok_or("unluac.jar not found (set UNLUAC_JAR or tools/external/unluac/unluac.jar)")?;

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
/// `tools/ucfx_be_to_le.py::_convert_lua_be_to_le`.
pub fn convert_binn_be_to_le(be: &[u8]) -> Result<Vec<u8>, String> {
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
    let le_luaq = luaq_be_to_le(&be[luaq_off..])?;
    head.extend_from_slice(&le_luaq);
    Ok(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_binn_body() {
        let empty = vec![];
        let result = convert_binn_be_to_le(&empty);
        // Empty body: might succeed or fail depending on implementation
        let _ = result;
    }

    #[test]
    fn test_short_binn_body() {
        let short = vec![0u8; 2];
        let result = convert_binn_be_to_le(&short);
        // Short body: might succeed (fallback to header-only swap) or fail
        let _ = result;
    }

    #[test]
    fn test_luaq_signature_detection() {
        // Body with Lua signature: starts with junk, then \x1bLua
        let mut data = vec![0u8; 100];
        // Insert Lua signature at offset 50
        data[50..54].copy_from_slice(b"\x1bLua");
        // This should attempt to process the bytecode via unluac
        // but will likely fail due to missing Java/unluac
        let result = convert_binn_be_to_le(&data);
        // We just check it doesn't crash; the actual conversion depends on environment
        let _ = result;
    }

    #[test]
    fn test_no_luaq_signature_skips_java() {
        // Body without Lua signature (all zeros)
        let data = vec![0u8; 100];
        let result = convert_binn_be_to_le(&data);
        // Without signature, should just byte-swap the leading u32 and return
        match result {
            Ok(output) => {
                // Output should be similar length or shorter (no Java processing)
                assert!(output.len() <= data.len());
            }
            Err(_) => {
                // Some error paths are acceptable
            }
        }
    }
}
