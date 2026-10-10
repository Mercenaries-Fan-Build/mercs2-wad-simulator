//! PS3 PPU executable imports: the PRX import stubs of a PS3 ELF.
//!
//! A PS3 executable calls a system-library function through a 32-byte **code stub**, reached by
//! `bl`. The stub loads a 4-byte **slot** that the loader fills with the function's descriptor and
//! jumps through it. Its eight words are
//!
//! ```text
//! 39800000  li     r12,0
//! 658cHHHH  oris   r12,r12,HHHH
//! 818cLLLL  lwz    r12,LLLL(r12)     slot = HHHH << 16 + sign-extended LLLL
//! f8410028  std    r2,40(r1)
//! 800c0000  lwz    r0,0(r12)
//! 804c0004  lwz    r2,4(r12)
//! 7c0903a6  mtctr  r0
//! 4e800420  bctr
//! ```
//!
//! Which function a slot stands for is the import table's. The program header of type
//! `0x60000002` holds `sys_process_prx_param`: `{u32 size = 0x28, u32 magic = 0x1b434cec, u32
//! version = 2, u32, u32 libent_start, u32 libent_end, u32 libstub_start, u32 libstub_end, u16,
//! u16, u32}`. The `libstub` range is an array of 0x2c-byte library records `{u8 size = 0x2c, u8,
//! u16 version, u16 attributes, u16 function_count, u16 variable_count, u16 tls_count, u8[4], u32
//! name, u32 fnids, u32 slots, u32 variable_nids, u32 variable_stubs, u32, u32}`: `name` points at
//! the library's NUL-terminated name, `fnids` at `function_count` 4-byte function NIDs (FNIDs) and
//! `slots` at as many slots, the FNID at index *k* naming the function whose slot is at index *k*.
//!
//! [`prx_imports`] reads every record and, for each function, the stub the slot's initial word
//! points at. It fails on anything but exactly one `0x60000002` header holding a version-2 param
//! of size 0x28 with the magic, a `libstub` range that is not whole 0x2c-byte records, a record
//! whose `size` is not 0x2c, a library with variable or TLS imports (`variable_count` or
//! `tls_count` not 0: those are not read), a repeated slot, a stub outside every executable
//! section, a stub that is not the eight words above, and a stub that loads a slot other than the
//! one pointing at it. The stubs must then fill one executable section exactly, ascending at 32
//! bytes apart, so every 32-byte step of that section is the stub of exactly one record.

use crate::starts::{elf_sections, elf_segments, SHF_EXECINSTR};
use crate::Error;

/// Program header type of `sys_process_prx_param`.
pub const PT_PRX_PARAM: u32 = 0x6000_0002;
const PRX_PARAM_MAGIC: u32 = 0x1b43_4cec;
const PRX_PARAM_VERSION: u32 = 2;
const PRX_PARAM_SIZE: u64 = 0x28;
const LIBSTUB_SIZE: u32 = 0x2c;
const PT_LOAD: u32 = 1;
const EM_PPC64: u16 = 21;

/// Size of a PRX import code stub.
pub const STUB_SIZE: usize = 32;

/// One function import of a PS3 ELF: its code stub, the slot the stub loads, and the library and
/// FNID the import table gives that slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrxImport {
    pub stub: u32,
    pub slot: u32,
    pub library: String,
    pub fnid: u32,
    /// The stub's bytes.
    pub bytes: [u8; STUB_SIZE],
}

/// The header of a PS3 `<id>.imports.tsv` export.
pub const PRX_IMPORTS_HEADER: &str = "address\tkind\tlibrary\tfnid\tstub";

/// The slot a stub's eight words load, or `None` when they are not the stub form.
pub fn decode_stub(w: &[u32; 8]) -> Option<u32> {
    const TAIL: [u32; 5] = [0xf841_0028, 0x800c_0000, 0x804c_0004, 0x7c09_03a6, 0x4e80_0420];
    if w[0] != 0x3980_0000 || w[1] >> 16 != 0x658c || w[2] >> 16 != 0x818c || w[3..] != TAIL {
        return None;
    }
    let hi = (w[1] & 0xffff) << 16;
    let lo = (w[2] & 0xffff) as u16 as i16 as i32 as u32;
    Some(hi.wrapping_add(lo))
}

/// Every function import of the PS3 ELF `elf`, ascending by stub address.
pub fn prx_imports(elf: &[u8]) -> Result<Vec<PrxImport>, Error> {
    if elf.get(0..4) != Some(b"\x7fELF") {
        return Err(Error::new("not an ELF file"));
    }
    let machine = elf.get(0x12..0x14).map(|b| u16::from_be_bytes([b[0], b[1]]));
    if machine != Some(EM_PPC64) {
        return Err(Error::new(format!("ELF machine is {machine:?}, not PowerPC64 ({EM_PPC64})")));
    }
    let segs = elf_segments(elf)?;
    let (_, secs) = elf_sections(elf)?;
    let off = |va: u32, len: usize, what: &str| -> Result<usize, Error> {
        let va = va as u64;
        segs.iter()
            .filter(|s| s.kind == PT_LOAD && va >= s.vaddr && va + len as u64 <= s.vaddr + s.filesz)
            .map(|s| (s.offset + (va - s.vaddr)) as usize)
            .next()
            .filter(|o| o + len <= elf.len())
            .ok_or_else(|| Error::new(format!("{what} 0x{va:08x} ({len} bytes) is not in a loaded part of the file")))
    };
    let word = |va: u32, what: &str| -> Result<u32, Error> {
        let o = off(va, 4, what)?;
        Ok(u32::from_be_bytes(elf[o..o + 4].try_into().unwrap()))
    };

    let params: Vec<_> = segs.iter().filter(|s| s.kind == PT_PRX_PARAM).collect();
    if params.len() != 1 {
        return Err(Error::new(format!(
            "ELF has {} program headers of type 0x{PT_PRX_PARAM:08x} (sys_process_prx_param), not one",
            params.len()
        )));
    }
    let p = params[0];
    let po = p.offset as usize;
    let param = elf
        .get(po..po + PRX_PARAM_SIZE as usize)
        .ok_or_else(|| Error::new(format!("sys_process_prx_param at file 0x{po:x} runs past the file")))?;
    let pw = |i: usize| u32::from_be_bytes(param[4 * i..4 * i + 4].try_into().unwrap());
    let (size, magic, version) = (pw(0), pw(1), pw(2));
    if p.filesz != PRX_PARAM_SIZE || size as u64 != PRX_PARAM_SIZE || magic != PRX_PARAM_MAGIC || version != PRX_PARAM_VERSION {
        return Err(Error::new(format!(
            "sys_process_prx_param has header size 0x{:x}, size 0x{size:x}, magic 0x{magic:08x}, version {version}; \
             the layout read is size 0x{PRX_PARAM_SIZE:x}, magic 0x{PRX_PARAM_MAGIC:08x}, version {PRX_PARAM_VERSION}",
            p.filesz
        )));
    }
    let (stubs_start, stubs_end) = (pw(6), pw(7));
    if stubs_end < stubs_start || (stubs_end - stubs_start) % LIBSTUB_SIZE != 0 {
        return Err(Error::new(format!(
            "the libstub range 0x{stubs_start:08x}..0x{stubs_end:08x} is not whole 0x{LIBSTUB_SIZE:x}-byte records"
        )));
    }

    let mut out: Vec<PrxImport> = Vec::new();
    let mut slots = std::collections::BTreeSet::new();
    let mut rec = stubs_start;
    while rec < stubs_end {
        let what = format!("library record 0x{rec:08x}");
        let ro = off(rec, LIBSTUB_SIZE as usize, &what)?;
        let r = &elf[ro..ro + LIBSTUB_SIZE as usize];
        if r[0] as u32 != LIBSTUB_SIZE {
            return Err(Error::new(format!("{what} has size 0x{:x}, not 0x{LIBSTUB_SIZE:x}", r[0])));
        }
        let h = |o: usize| u16::from_be_bytes([r[o], r[o + 1]]);
        let w = |o: usize| u32::from_be_bytes(r[o..o + 4].try_into().unwrap());
        let (functions, variables, tls) = (h(6), h(8), h(10));
        let (name_va, fnids, slot_array) = (w(16), w(20), w(24));
        let no = off(name_va, 1, &format!("{what} name"))?;
        let len = elf[no..].iter().position(|&b| b == 0).ok_or_else(|| Error::new(format!("{what}: its name is not NUL-terminated")))?;
        let name = &elf[no..no + len];
        if name.is_empty() || !name.iter().all(|b| b.is_ascii_graphic()) {
            return Err(Error::new(format!("{what}: its name {name:?} is not printable ASCII without spaces")));
        }
        let library = String::from_utf8(name.to_vec()).unwrap();
        if variables != 0 || tls != 0 {
            return Err(Error::new(format!(
                "{what} ({library}) imports {variables} variables and {tls} TLS variables; only function imports are read"
            )));
        }
        for k in 0..functions as u32 {
            let fnid = word(fnids + 4 * k, &format!("{library} FNID {k}"))?;
            let slot = slot_array + 4 * k;
            if !slots.insert(slot) {
                return Err(Error::new(format!("{library} FNID {fnid:08x}: slot 0x{slot:08x} belongs to another import too")));
            }
            let stub = word(slot, &format!("{library} FNID {fnid:08x} slot"))?;
            let in_code = secs
                .iter()
                .any(|s| s.flags & SHF_EXECINSTR != 0 && (stub as u64) >= s.addr && stub as u64 + STUB_SIZE as u64 <= s.addr + s.size);
            if !in_code {
                return Err(Error::new(format!(
                    "{library} FNID {fnid:08x}: slot 0x{slot:08x} points at 0x{stub:08x}, which is not 32 bytes inside an executable section"
                )));
            }
            let so = off(stub, STUB_SIZE, &format!("{library} FNID {fnid:08x} stub"))?;
            let bytes: [u8; STUB_SIZE] = elf[so..so + STUB_SIZE].try_into().unwrap();
            let words: [u32; 8] = std::array::from_fn(|i| u32::from_be_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap()));
            let loads = decode_stub(&words).ok_or_else(|| {
                Error::new(format!(
                    "{library} FNID {fnid:08x}: the stub at 0x{stub:08x} is {}, not li r12,0; oris; lwz; std r2,40(r1); lwz r0; lwz r2; mtctr r0; bctr",
                    words.iter().map(|w| format!("{w:08x}")).collect::<Vec<_>>().join(" ")
                ))
            })?;
            if loads != slot {
                return Err(Error::new(format!(
                    "{library} FNID {fnid:08x}: the stub at 0x{stub:08x} loads slot 0x{loads:08x}, not its own slot 0x{slot:08x}"
                )));
            }
            out.push(PrxImport { stub, slot, library: library.clone(), fnid, bytes });
        }
        rec += LIBSTUB_SIZE;
    }
    if out.is_empty() {
        return Err(Error::new("the import table lists no function"));
    }
    out.sort_by_key(|i| i.stub);
    let first = out[0].stub as u64;
    let sec = secs
        .iter()
        .find(|s| s.flags & SHF_EXECINSTR != 0 && first >= s.addr && first < s.addr + s.size)
        .ok_or_else(|| Error::new(format!("the first stub 0x{first:08x} is in no executable section")))?;
    for (i, imp) in out.iter().enumerate() {
        let want = sec.addr + (i * STUB_SIZE) as u64;
        if imp.stub as u64 != want {
            return Err(Error::new(format!(
                "the stubs do not fill the executable section 0x{:08x}..0x{:08x}: stub {i} is at 0x{:08x}, not 0x{want:08x}",
                sec.addr,
                sec.addr + sec.size,
                imp.stub
            )));
        }
    }
    if sec.addr + (out.len() * STUB_SIZE) as u64 != sec.addr + sec.size {
        return Err(Error::new(format!(
            "the {} stubs end at 0x{:08x}, short of the end of their executable section 0x{:08x}..0x{:08x}",
            out.len(),
            sec.addr + (out.len() * STUB_SIZE) as u64,
            sec.addr,
            sec.addr + sec.size
        )));
    }
    Ok(out)
}

/// The `<id>.imports.tsv` text of `imports`: [`PRX_IMPORTS_HEADER`], then one row per import in
/// the order given: the stub address (`0x%08x`), `function`, the library, the FNID (8 lowercase hex
/// digits) and the stub's bytes (lowercase hex; its length is the stub size).
pub fn prx_imports_tsv(imports: &[PrxImport]) -> String {
    let mut s = format!("{PRX_IMPORTS_HEADER}\n");
    for i in imports {
        let bytes: String = i.bytes.iter().map(|b| format!("{b:02x}")).collect();
        s.push_str(&format!("0x{:08x}\tfunction\t{}\t{:08x}\t{bytes}\n", i.stub, i.library, i.fnid));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: u64 = 0x1_0000; // code section: two stubs
    const RO: u64 = 0x2_0000; // read-only data: prx_param, records, names, FNIDs
    const DATA: u64 = 0x3_0000; // slots

    fn stub_words(slot: u32) -> [u32; 8] {
        let hi = (slot.wrapping_add(0x8000) >> 16) & 0xffff;
        let lo = slot & 0xffff;
        [0x3980_0000, 0x658c_0000 | hi, 0x818c_0000 | lo, 0xf841_0028, 0x800c_0000, 0x804c_0004, 0x7c09_03a6, 0x4e80_0420]
    }

    /// A PS3 ELF: one code section of `stubs` (each 32 bytes, its slot), one PT_LOAD per region,
    /// the prx_param header, and one library record per `libs` entry (name, [(fnid, slot)]).
    /// `edit` changes the file before it is returned.
    fn elf(libs: &[(&str, &[(u32, u32)])], stubs: &[(u32, [u32; 8])], edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let (text_off, ro_off, data_off) = (0x1000usize, 0x2000usize, 0x3000usize);
        let mut d = vec![0u8; 0x4000];
        let put32 = |d: &mut Vec<u8>, o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_be_bytes());
        let put16 = |d: &mut Vec<u8>, o: usize, v: u16| d[o..o + 2].copy_from_slice(&v.to_be_bytes());
        let put64 = |d: &mut Vec<u8>, o: usize, v: u64| d[o..o + 8].copy_from_slice(&v.to_be_bytes());
        d[0..4].copy_from_slice(b"\x7fELF");
        d[4] = 2;
        d[5] = 2;
        put16(&mut d, 0x12, EM_PPC64);
        // program headers at 0x40: three PT_LOAD and the prx_param
        put64(&mut d, 0x20, 0x40);
        put16(&mut d, 0x36, 56);
        put16(&mut d, 0x38, 4);
        let ph = |d: &mut Vec<u8>, i: usize, kind: u32, off: usize, va: u64, size: u64| {
            let o = 0x40 + i * 56;
            d[o..o + 4].copy_from_slice(&kind.to_be_bytes());
            d[o + 8..o + 16].copy_from_slice(&(off as u64).to_be_bytes());
            d[o + 16..o + 24].copy_from_slice(&va.to_be_bytes());
            d[o + 32..o + 40].copy_from_slice(&size.to_be_bytes());
        };
        ph(&mut d, 0, PT_LOAD, text_off, TEXT, 0x1000);
        ph(&mut d, 1, PT_LOAD, ro_off, RO, 0x1000);
        ph(&mut d, 2, PT_LOAD, data_off, DATA, 0x1000);
        ph(&mut d, 3, PT_PRX_PARAM, ro_off, RO, PRX_PARAM_SIZE);
        // section headers at 0x200: [null, code]
        put64(&mut d, 0x28, 0x200);
        put16(&mut d, 0x3a, 64);
        put16(&mut d, 0x3c, 2);
        let sh = 0x200 + 64;
        put32(&mut d, sh + 4, 1);
        put64(&mut d, sh + 8, 0x6);
        put64(&mut d, sh + 16, TEXT);
        put64(&mut d, sh + 24, text_off as u64);
        put64(&mut d, sh + 32, (stubs.len() * STUB_SIZE) as u64);
        for (i, (_, w)) in stubs.iter().enumerate() {
            for (k, x) in w.iter().enumerate() {
                put32(&mut d, text_off + i * STUB_SIZE + 4 * k, *x);
            }
        }
        // prx_param, then records from RO + 0x100, names from RO + 0x400, FNIDs from RO + 0x600
        let records = RO as u32 + 0x100;
        put32(&mut d, ro_off, PRX_PARAM_SIZE as u32);
        put32(&mut d, ro_off + 4, PRX_PARAM_MAGIC);
        put32(&mut d, ro_off + 8, PRX_PARAM_VERSION);
        put32(&mut d, ro_off + 24, records);
        put32(&mut d, ro_off + 28, records + LIBSTUB_SIZE * libs.len() as u32);
        let (mut name_at, mut fnid_at) = (0x400usize, 0x600usize);
        for (li, (name, fns)) in libs.iter().enumerate() {
            let r = ro_off + 0x100 + li * LIBSTUB_SIZE as usize;
            d[r] = LIBSTUB_SIZE as u8;
            put16(&mut d, r + 6, fns.len() as u16);
            d[ro_off + name_at..ro_off + name_at + name.len()].copy_from_slice(name.as_bytes());
            put32(&mut d, r + 16, RO as u32 + name_at as u32);
            name_at += name.len() + 1;
            put32(&mut d, r + 20, RO as u32 + fnid_at as u32);
            put32(&mut d, r + 24, fns.first().map(|f| f.1).unwrap_or(DATA as u32));
            for (fnid, _) in fns.iter() {
                put32(&mut d, ro_off + fnid_at, *fnid);
                fnid_at += 4;
            }
        }
        for (i, (slot, _)) in stubs.iter().enumerate() {
            put32(&mut d, data_off + (*slot as u64 - DATA) as usize, TEXT as u32 + (i * STUB_SIZE) as u32);
        }
        edit(&mut d);
        d
    }

    const S0: u32 = DATA as u32 + 0x10;
    const S1: u32 = DATA as u32 + 0x14;
    const S2: u32 = DATA as u32 + 0x40;
    const LIBS: &[(&str, &[(u32, u32)])] = &[("sys_fs", &[(0x2cb5_1f0d, S0), (0x4d5f_f8e2, S1)]), ("cellSysmodule", &[(0x3226_7a31, S2)])];

    fn good_stubs() -> Vec<(u32, [u32; 8])> {
        // stub order differs from record order
        vec![(S2, stub_words(S2)), (S0, stub_words(S0)), (S1, stub_words(S1))]
    }

    #[test]
    fn decode_stub_sign_extends_the_low_half() {
        // ps3-v100's first stub: oris r12,r12,0xfb; lwz r12,0x4f8c(r12)
        let w = [0x3980_0000, 0x658c_00fb, 0x818c_4f8c, 0xf841_0028, 0x800c_0000, 0x804c_0004, 0x7c09_03a6, 0x4e80_0420];
        assert_eq!(decode_stub(&w), Some(0x00fb_4f8c));
        // a negative displacement: 0xfb0000 - 0x7074
        let mut n = w;
        n[2] = 0x818c_8f8c;
        assert_eq!(decode_stub(&n), Some(0x00fa_8f8c));
        assert_eq!(decode_stub(&stub_words(0x00fa_8f8c)), Some(0x00fa_8f8c));
        let mut bad = w;
        bad[0] = 0x3980_0001;
        assert_eq!(decode_stub(&bad), None);
        let mut bad = w;
        bad[7] = 0x4e80_0020;
        assert_eq!(decode_stub(&bad), None);
    }

    #[test]
    fn every_function_import_is_read_with_its_stub_ascending() {
        let imps = prx_imports(&elf(LIBS, &good_stubs(), |_| {})).unwrap();
        let rows: Vec<(u32, u32, &str, u32)> = imps.iter().map(|i| (i.stub, i.slot, i.library.as_str(), i.fnid)).collect();
        assert_eq!(
            rows,
            vec![
                (0x1_0000, S2, "cellSysmodule", 0x3226_7a31),
                (0x1_0020, S0, "sys_fs", 0x2cb5_1f0d),
                (0x1_0040, S1, "sys_fs", 0x4d5f_f8e2),
            ]
        );
        let tsv = prx_imports_tsv(&imps[..1]);
        let stub: String = stub_words(S2).iter().map(|w| format!("{w:08x}")).collect();
        assert_eq!(tsv, format!("address\tkind\tlibrary\tfnid\tstub\n0x00010000\tfunction\tcellSysmodule\t32267a31\t{stub}\n"));
    }

    #[test]
    fn a_stub_that_is_not_the_stub_form_is_an_error() {
        let mut s = good_stubs();
        s[1].1[3] = 0x6000_0000;
        let e = prx_imports(&elf(LIBS, &s, |_| {})).unwrap_err().to_string();
        assert!(e.contains("sys_fs FNID 2cb51f0d") && e.contains("0x00010020") && e.contains("not li r12,0"), "{e}");
    }

    #[test]
    fn a_stub_loading_another_slot_is_an_error() {
        let mut s = good_stubs();
        s[1].1 = stub_words(S1);
        let e = prx_imports(&elf(LIBS, &s, |_| {})).unwrap_err().to_string();
        assert!(e.contains("loads slot 0x00030014, not its own slot 0x00030010"), "{e}");
    }

    #[test]
    fn variable_imports_are_an_error() {
        let e = prx_imports(&elf(LIBS, &good_stubs(), |d| d[0x2100 + 9] = 1)).unwrap_err().to_string();
        assert!(e.contains("sys_fs") && e.contains("imports 1 variables"), "{e}");
        let e = prx_imports(&elf(LIBS, &good_stubs(), |d| d[0x2100 + 11] = 2)).unwrap_err().to_string();
        assert!(e.contains("2 TLS variables"), "{e}");
    }

    #[test]
    fn a_slot_listed_twice_is_an_error() {
        let libs: &[(&str, &[(u32, u32)])] = &[("sys_fs", &[(0x2cb5_1f0d, S0), (0x4d5f_f8e2, S1)]), ("cellSysmodule", &[(0x3226_7a31, S0)])];
        let e = prx_imports(&elf(libs, &good_stubs(), |_| {})).unwrap_err().to_string();
        assert!(e.contains("slot 0x00030010 belongs to another import"), "{e}");
    }

    #[test]
    fn stubs_that_do_not_fill_their_section_are_an_error() {
        // a fourth 32-byte step in the section that no record's slot points at
        let mut s = good_stubs();
        s.push((DATA as u32 + 0x80, stub_words(DATA as u32 + 0x80)));
        let e = prx_imports(&elf(LIBS, &s, |_| {})).unwrap_err().to_string();
        assert!(e.contains("short of the end of their executable section"), "{e}");
    }

    #[test]
    fn a_wrong_prx_param_is_an_error() {
        let e = prx_imports(&elf(LIBS, &good_stubs(), |d| d[0x2004] = 0)).unwrap_err().to_string();
        assert!(e.contains("magic"), "{e}");
        let e = prx_imports(&elf(LIBS, &good_stubs(), |d| d[0x200b] = 3)).unwrap_err().to_string();
        assert!(e.contains("version 3"), "{e}");
        // the prx_param header turned into a second PT_LOAD
        let e = prx_imports(&elf(LIBS, &good_stubs(), |d| d[0x40 + 3 * 56..0x40 + 3 * 56 + 4].copy_from_slice(&PT_LOAD.to_be_bytes())))
            .unwrap_err()
            .to_string();
        assert!(e.contains("0 program headers of type 0x60000002"), "{e}");
    }

    #[test]
    fn a_record_of_the_wrong_size_is_an_error() {
        let e = prx_imports(&elf(LIBS, &good_stubs(), |d| d[0x2100 + 0x2c] = 0x28)).unwrap_err().to_string();
        assert!(e.contains("has size 0x28, not 0x2c"), "{e}");
    }
}
