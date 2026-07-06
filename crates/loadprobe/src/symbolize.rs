//! symbolize — turn `module+0xRVA` tokens in a `[crash]` block into `function+0xN`.
//!
//! pmc_bb's crash handler emits every faulting site as `module.dll+0xOFFSET` (see
//! tools/pmc_blackbox/crash_handler.c). That is unambiguous but still cryptic —
//! you have to own a symbol map to know *which function* +0x2251 is. This pass
//! closes that gap offline (never in the crash path):
//!
//!   * our own modules (`lua_trace.asi`, other un-stripped `.asi`/`.dll`) carry a
//!     COFF symbol table — parsed here directly, no external `nm`/crate needed;
//!   * `Mercenaries2.exe` frames resolve against the curated VA→name map in
//!     scripts/mercs2_annotations.json (sparse, so far matches are dropped).
//!
//! A line like `... EIP=71482251 (lua_trace.asi+0x2251) ...` gains `= Record+0xA1`.

use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

fn u16le(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o], b[o + 1]]) }
fn u32le(b: &[u8], o: usize) -> u32 { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }

/// A module's function symbols, sorted ascending by the key used to query it
/// (RVA for a loaded module, absolute VA for the exe annotation map).
pub struct SymTable {
    ents: Vec<(u32, String)>,
}

impl SymTable {
    /// Nearest symbol at or below `key`, if within `cap` bytes. Returns
    /// "name" (exact) or "name+0xN".
    fn nearest(&self, key: u32, cap: u32) -> Option<String> {
        let idx = match self.ents.binary_search_by(|e| e.0.cmp(&key)) {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };
        let (base, name) = &self.ents[idx];
        let delta = key - base;
        if delta > cap { return None; }
        Some(if delta == 0 { name.clone() } else { format!("{}+0x{:X}", name, delta) })
    }
}

/// Clean a MinGW COFF symbol name: drop the leading cdecl `_` and any trailing
/// `@N` stdcall decoration so `_DllMain@12` reads as `DllMain`.
fn clean_name(raw: &str) -> String {
    let s = raw.strip_prefix('_').unwrap_or(raw);
    match s.find('@') { Some(i) => s[..i].to_string(), None => s.to_string() }
}

/// Read a COFF symbol's name (8-byte inline, or an offset into the string table).
fn read_sym_name(data: &[u8], ent: usize, strtab: usize) -> Option<String> {
    let n = &data[ent..ent + 8];
    if n[0] == 0 && n[1] == 0 && n[2] == 0 && n[3] == 0 {
        let start = strtab + u32le(data, ent + 4) as usize;
        let mut end = start;
        while end < data.len() && data[end] != 0 { end += 1; }
        if start > data.len() { return None; }
        std::str::from_utf8(&data[start..end]).ok().map(|s| s.to_string())
    } else {
        let mut end = 0;
        while end < 8 && n[end] != 0 { end += 1; }
        std::str::from_utf8(&n[..end]).ok().map(|s| s.to_string())
    }
}

/// Parse the COFF symbol table of a PE32 image into an RVA-sorted table of the
/// functions living in code sections. Returns None if the image is stripped or
/// unparseable.
pub fn parse_pe_symbols(data: &[u8]) -> Option<SymTable> {
    if data.len() < 0x40 || &data[0..2] != b"MZ" { return None; }
    let pe = u32le(data, 0x3C) as usize;
    if pe + 24 > data.len() || &data[pe..pe + 4] != b"PE\0\0" { return None; }
    let coff = pe + 4;
    let num_sections = u16le(data, coff + 2) as usize;
    let ptr_symtab = u32le(data, coff + 8) as usize;
    let num_syms = u32le(data, coff + 12) as usize;
    let opt_size = u16le(data, coff + 16) as usize;
    if ptr_symtab == 0 || num_syms == 0 { return None; }   // stripped

    // section table: capture VirtualAddress + whether the section holds code
    let sec_base = coff + 20 + opt_size;
    let mut sec_va = Vec::with_capacity(num_sections);
    let mut sec_code = Vec::with_capacity(num_sections);
    for i in 0..num_sections {
        let s = sec_base + i * 40;
        if s + 40 > data.len() { break; }
        sec_va.push(u32le(data, s + 12));
        let chars = u32le(data, s + 36);
        // IMAGE_SCN_CNT_CODE (0x20) | IMAGE_SCN_MEM_EXECUTE (0x2000_0000)
        sec_code.push(chars & 0x2000_0020 != 0);
    }

    let strtab = ptr_symtab + num_syms * 18;
    let mut ents: Vec<(u32, String)> = Vec::new();
    let mut i = 0usize;
    while i < num_syms {
        let e = ptr_symtab + i * 18;
        if e + 18 > data.len() { break; }
        let value = u32le(data, e + 8);
        let secnum = u16le(data, e + 12) as i16;   // 1-based; <=0 = absolute/debug
        let storage = data[e + 16];
        let naux = data[e + 17] as usize;
        // Function = external(2)/static(3) storage class in a code section.
        if secnum >= 1 && (storage == 2 || storage == 3) {
            let sidx = secnum as usize - 1;
            if sidx < sec_va.len() && *sec_code.get(sidx).unwrap_or(&false) {
                if let Some(raw) = read_sym_name(data, e, strtab) {
                    let name = clean_name(&raw);
                    if !name.is_empty() && !raw.starts_with('.') {
                        ents.push((sec_va[sidx] + value, name));
                    }
                }
            }
        }
        i += 1 + naux;
    }
    if ents.is_empty() { return None; }
    ents.sort_by_key(|e| e.0);
    ents.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    Some(SymTable { ents })
}

/// Load `scripts/mercs2_annotations.json`'s `"0x<VA>" -> {name}` entries into a
/// VA-sorted table (queried by absolute exe VA).
fn load_exe_symbols(path: &PathBuf) -> Option<SymTable> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let obj = v.as_object()?;
    let mut ents: Vec<(u32, String)> = Vec::new();
    for (k, val) in obj {
        if !k.starts_with("0x") { continue; }
        let Ok(va) = u32::from_str_radix(&k[2..], 16) else { continue };
        let name = val.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if !name.is_empty() { ents.push((va, name.to_string())); }
    }
    if ents.is_empty() { return None; }
    ents.sort_by_key(|e| e.0);
    Some(SymTable { ents })
}

/// Resolves `module+offset` tokens found in crash-log lines.
pub struct Symbolizer {
    exe: Option<SymTable>,
    search_dirs: Vec<PathBuf>,
    cache: HashMap<String, Option<SymTable>>,   // filename(lower) -> parsed table
}

/// A near-symbol match is only meaningful within a plausible function span; a
/// far "nearest" entry (sparse exe map) is dropped rather than mislabel.
const EXE_CAP: u32 = 0x4000;
const MOD_CAP: u32 = 0x8000;
const EXE_IMAGE_BASE: u32 = 0x0040_0000;

impl Symbolizer {
    pub fn new(exe_symbols: &PathBuf, search_dirs: Vec<PathBuf>) -> Self {
        Symbolizer { exe: load_exe_symbols(exe_symbols), search_dirs, cache: HashMap::new() }
    }

    /// True if this symbolizer has *any* source to resolve against.
    pub fn has_any_source(&mut self) -> bool {
        self.exe.is_some() || self.search_dirs.iter().any(|d| d.exists())
    }

    fn load_module(&self, fname: &str) -> Option<SymTable> {
        for dir in &self.search_dirs {
            let p = dir.join(fname);
            if let Ok(bytes) = std::fs::read(&p) {
                if let Some(t) = parse_pe_symbols(&bytes) { return Some(t); }
            }
        }
        None
    }

    /// Resolve a single `module`(lowercased) + `off` to "func+0xN", or None.
    fn lookup(&mut self, module: &str, off: u32) -> Option<String> {
        if module.ends_with(".exe") {
            let va = EXE_IMAGE_BASE.checked_add(off)?;
            return self.exe.as_ref()?.nearest(va, EXE_CAP);
        }
        if module.ends_with(".asi") || module.ends_with(".dll") {
            if !self.cache.contains_key(module) {
                let t = self.load_module(module);
                self.cache.insert(module.to_string(), t);
            }
            return self.cache.get(module)?.as_ref()?.nearest(off, MOD_CAP);
        }
        None
    }

    /// Append `= func+0xN` to a crash line for its first resolvable
    /// `module+0xhex` token. Lines with no such token are returned unchanged.
    pub fn rewrite_line(&mut self, line: &str) -> String {
        let mut from = 0;
        while let Some(rel) = line[from..].find("+0x") {
            let p = from + rel;
            let hs = p + 3;
            let he = hs + line[hs..].bytes().take_while(|c| c.is_ascii_hexdigit()).count();
            if he == hs { from = hs; continue; }
            let off = u32::from_str_radix(&line[hs..he], 16).ok();
            // module name = the identifier run immediately before the '+'
            let ms = line[..p]
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'))
                .map(|i| i + 1)
                .unwrap_or(0);
            let module = line[ms..p].to_lowercase();
            if let (Some(off), true) = (off, module.contains('.')) {
                if let Some(res) = self.lookup(&module, off) {
                    return format!("{}   = {}", line, res);
                }
            }
            from = he;
        }
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tbl(ents: &[(u32, &str)]) -> SymTable {
        SymTable { ents: ents.iter().map(|(a, n)| (*a, n.to_string())).collect() }
    }

    #[test]
    fn nearest_below_and_exact() {
        let t = tbl(&[(0x21b0, "Record"), (0x2420, "DllMain"), (0x14a0, "SharedDetour")]);
        // (unsorted input is fine for these hand-picked queries because nearest()
        //  relies on sorted order — sort first as the real loader does)
        let mut e = t.ents.clone(); e.sort_by_key(|x| x.0);
        let t = SymTable { ents: e };
        assert_eq!(t.nearest(0x2251, MOD_CAP).as_deref(), Some("Record+0xA1"));
        assert_eq!(t.nearest(0x21b0, MOD_CAP).as_deref(), Some("Record"));
        assert_eq!(t.nearest(0x14ac, MOD_CAP).as_deref(), Some("SharedDetour+0xC"));
    }

    #[test]
    fn nearest_respects_cap_and_floor() {
        let t = tbl(&[(0x1000, "A"), (0x2000, "B")]);
        assert_eq!(t.nearest(0x0500, MOD_CAP), None);          // below first symbol
        assert_eq!(t.nearest(0x2000 + MOD_CAP + 1, MOD_CAP), None); // beyond cap of nearest (B)
    }

    #[test]
    fn rewrite_appends_for_module_token() {
        let mut s = Symbolizer { exe: None, search_dirs: vec![], cache: HashMap::new() };
        s.cache.insert("lua_trace.asi".into(), Some(tbl(&[(0x21b0, "Record"), (0x14a0, "SharedDetour")])));
        let out = s.rewrite_line("  stk+050 = 714814AC  lua_trace.asi+0x14AC");
        assert!(out.ends_with("= SharedDetour+0xC"), "got: {out}");
        // no module token -> unchanged
        let plain = "  -> faulting pointer = ECX(000C73AD) + 0x8";
        assert_eq!(s.rewrite_line(plain), plain);
    }
}
