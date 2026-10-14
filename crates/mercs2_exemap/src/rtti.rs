//! Virtual tables recovered from an executable's own C++ run-time type information, read from the
//! image bytes.
//!
//! **MSVC** (PC x86 PE, little-endian; Xbox 360 PowerPC PE, big-endian; 32-bit pointers):
//!
//! * a type descriptor is `{pVFTable, spare, name}` with `name` = `.?AV…@@` or `.?AU…@@`;
//! * a complete object locator is `{signature = 0, offset, cdOffset, pTypeDescriptor,
//!   pClassHierarchyDescriptor}`, and is accepted only when its hierarchy descriptor (`signature =
//!   0`, `numBaseClasses` 1..=63) lists the same type descriptor as its first base class;
//! * a vtable is preceded by a pointer to its locator; its slots are the following words that point
//!   into executable sections.
//!
//! **GCC / Itanium** (PS3 PPU ELF, big-endian, 32-bit pointers):
//!
//! * the vptrs of class typeinfo objects are the address points (vtable + 8) of the vtables whose
//!   typeinfo is the image's own `__cxxabiv1::__class_type_info`, `__si_class_type_info` or
//!   `__vmi_class_type_info`;
//! * a typeinfo object is `{vptr, name}` with one of those vptrs and a name that parses as an
//!   Itanium nested or plain class name (`N3Foo3BarE`, `3Bar`);
//! * a vtable is `{offset_to_top (0 or negative), typeinfo, slots…}`; a slot is the address of an
//!   `.opd` function descriptor (resolved to the descriptor's code address) or a direct pointer into
//!   an executable section.
//!
//! An MSVC vtable's destructor slot is the one slot whose function tests the deleting-destructor
//! flag ([`crate::dtor`]); the Itanium ABI gives the same destructor two consecutive slots
//! (complete-object, then deleting), so an Itanium vtable of the same class has one slot more.
//!
//! [`Vtable::class`] is the qualified class name both ABIs agree on (`Foo::Bar`), present when the
//! decorated name is a plain or nested class name; template and operator names keep only their raw
//! form.

use crate::mem::Mem;
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abi {
    Msvc,
    Itanium,
}

impl Abi {
    pub fn as_str(self) -> &'static str {
        match self {
            Abi::Msvc => "msvc",
            Abi::Itanium => "itanium",
        }
    }
}

/// One recovered vtable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vtable {
    pub abi: Abi,
    /// The decorated name as stored in the image (`.?AVBar@Foo@@`, `N3Foo3BarE`).
    pub raw_name: String,
    /// `Foo::Bar`, when the decorated name is a plain or nested class name.
    pub class: Option<String>,
    /// Address of the first slot.
    pub va: u32,
    /// Offset of the subobject this vtable serves within the complete object (MSVC locator
    /// `offset`; Itanium `-offset_to_top`).
    pub subobject_offset: i64,
    /// Function entry address of each slot, in slot order.
    pub slots: Vec<u32>,
    /// MSVC: the one slot whose function tests the deleting-destructor flag
    /// ([`crate::dtor`]), when exactly one does. Itanium: `None`.
    pub deleting_destructor: Option<usize>,
}

/// The vtables of a PE (MSVC) or PS3 ELF (Itanium) image.
pub fn vtables(image: &[u8]) -> Result<Vec<Vtable>, Error> {
    if image.starts_with(b"MZ") {
        msvc(image)
    } else if image.starts_with(b"\x7fELF") {
        itanium(image)
    } else {
        Err(Error::new("not a PE (MZ) or ELF image"))
    }
}

/// `Foo::Bar` from `.?AVBar@Foo@@`; `None` for template, operator and other special names.
fn msvc_class(raw: &str) -> Option<String> {
    let body = raw.strip_prefix(".?AV").or_else(|| raw.strip_prefix(".?AU"))?.strip_suffix("@@")?;
    if body.is_empty() || body.contains('?') || body.contains('$') {
        return None;
    }
    let parts: Vec<&str> = body.split('@').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    Some(parts.into_iter().rev().collect::<Vec<_>>().join("::"))
}

fn msvc(image: &[u8]) -> Result<Vec<Vtable>, Error> {
    let mem = Mem::of(image)?;
    // Type descriptors: `.?A[UV]…@@` names in the data ranges; the descriptor starts 8 bytes before.
    let mut tds = std::collections::BTreeMap::new();
    for r in mem.ranges.iter().filter(|r| !r.exec) {
        let bytes = &image[r.off..r.off + (r.hi - r.lo) as usize];
        let mut i = 0;
        while i + 4 < bytes.len() {
            if &bytes[i..i + 3] == b".?A" && (bytes[i + 3] == b'V' || bytes[i + 3] == b'U') {
                if let Some(end) = bytes[i..(i + 512).min(bytes.len())].iter().position(|&b| b == 0) {
                    let name = &bytes[i..i + end];
                    if name.ends_with(b"@@") && name.iter().all(|b| b.is_ascii_graphic()) && i >= 8 {
                        tds.insert(r.lo + i as u32 - 8, String::from_utf8_lossy(name).into_owned());
                    }
                    i += end;
                    continue;
                }
            }
            i += 1;
        }
    }
    if tds.is_empty() {
        return Err(Error::new("no MSVC type descriptor names (.?AV…@@) in the image"));
    }
    // Complete object locators: a word equal to a type descriptor at locator + 12.
    let mut cols = std::collections::BTreeMap::new();
    for (va, w) in mem.data_words() {
        if !tds.contains_key(&w) || va < 12 {
            continue;
        }
        let col = va - 12;
        let ok = (|| {
            if mem.word(col)? != 0 {
                return None;
            }
            let chd = mem.word(col + 16)?;
            if mem.word(chd)? != 0 {
                return None;
            }
            let n = mem.word(chd + 8)?;
            if !(1..64).contains(&n) {
                return None;
            }
            let first = mem.word(mem.word(chd + 12)?)?;
            (mem.word(first)? == w).then_some(mem.word(col + 4)?)
        })();
        if let Some(offset) = ok {
            cols.insert(col, (w, offset));
        }
    }
    let mut out = Vec::new();
    let mut flag_cache: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
    for (va, w) in mem.data_words() {
        let Some(&(td, offset)) = cols.get(&w) else { continue };
        let first = va + 4;
        let mut slots = Vec::new();
        let mut s = first;
        while let Some(f) = mem.word(s) {
            if !mem.exec(f) {
                break;
            }
            slots.push(f);
            s += 4;
        }
        if slots.is_empty() {
            continue;
        }
        let raw = tds[&td].clone();
        let flagged: Vec<usize> = slots
            .iter()
            .enumerate()
            .filter(|(_, f)| *flag_cache.entry(**f).or_insert_with(|| crate::dtor::tests_delete_flag(&mem, **f)))
            .map(|(i, _)| i)
            .collect();
        out.push(Vtable {
            abi: Abi::Msvc,
            class: msvc_class(&raw),
            raw_name: raw,
            va: first,
            subobject_offset: offset as i64,
            slots,
            deleting_destructor: (flagged.len() == 1).then(|| flagged[0]),
        });
    }
    Ok(out)
}

/// The components of an Itanium plain (`3Bar`) or nested (`N3Foo3BarE`) class name.
fn itanium_parts(raw: &[u8]) -> Option<Vec<String>> {
    let body = match raw.first()? {
        b'N' => raw.strip_prefix(b"N")?.strip_suffix(b"E")?,
        b'0'..=b'9' => raw,
        _ => return None,
    };
    let mut parts = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let start = i;
        while i < body.len() && body[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
        let n: usize = std::str::from_utf8(&body[start..i]).ok()?.parse().ok()?;
        let id = body.get(i..i + n)?;
        if n == 0 || !(id[0].is_ascii_alphabetic() || id[0] == b'_') || !id.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_') {
            return None;
        }
        parts.push(String::from_utf8_lossy(id).into_owned());
        i += n;
    }
    (!parts.is_empty()).then_some(parts)
}

const CXXABI_TYPEINFOS: [&str; 3] = [
    "N10__cxxabiv117__class_type_infoE",
    "N10__cxxabiv120__si_class_type_infoE",
    "N10__cxxabiv121__vmi_class_type_infoE",
];

fn itanium(image: &[u8]) -> Result<Vec<Vtable>, Error> {
    let mem = Mem::of(image)?;

    // Class-name strings: NUL-preceded, NUL-terminated, parsing as an Itanium class name.
    let mut names = std::collections::BTreeMap::new();
    for r in mem.ranges.iter().filter(|r| !r.exec) {
        let bytes = &image[r.off..r.off + (r.hi - r.lo) as usize];
        for i in 1..bytes.len() {
            if bytes[i - 1] != 0 || !(bytes[i] == b'N' || bytes[i].is_ascii_digit()) {
                continue;
            }
            let Some(end) = bytes[i..(i + 512).min(bytes.len())].iter().position(|&b| b == 0) else { continue };
            let raw = &bytes[i..i + end];
            if let Some(parts) = itanium_parts(raw) {
                names.insert(r.lo + i as u32, (String::from_utf8_lossy(raw).into_owned(), parts));
            }
        }
    }

    // Typeinfo objects of every name (any vptr), then the cxxabi ones' vtable address points.
    let mut by_name: std::collections::BTreeMap<u32, Vec<(u32, u32)>> = Default::default();
    for (va, w) in mem.data_words() {
        if names.contains_key(&w) && va >= 4 {
            if let Some(vptr) = mem.word(va - 4) {
                by_name.entry(w).or_default().push((va - 4, vptr));
            }
        }
    }
    let mut abi_typeinfo = std::collections::BTreeSet::new();
    for want in CXXABI_TYPEINFOS {
        for (name_va, (raw, _)) in &names {
            if raw == want {
                for (ti, _) in by_name.get(name_va).into_iter().flatten() {
                    abi_typeinfo.insert(*ti);
                }
            }
        }
    }
    if abi_typeinfo.is_empty() {
        return Err(Error::new("no __cxxabiv1 class typeinfo objects in the image"));
    }
    let mut class_vptrs = std::collections::BTreeSet::new();
    for (va, w) in mem.data_words() {
        if abi_typeinfo.contains(&w) && va >= 4 && mem.word(va - 4) == Some(0) {
            class_vptrs.insert(va + 4);
        }
    }
    if class_vptrs.is_empty() {
        return Err(Error::new("no vtable of a __cxxabiv1 class typeinfo type in the image"));
    }

    let mut typeinfos = std::collections::BTreeMap::new();
    for (name_va, objs) in &by_name {
        for &(ti, vptr) in objs {
            if class_vptrs.contains(&vptr) {
                typeinfos.insert(ti, *name_va);
            }
        }
    }

    let mut out = Vec::new();
    for (va, w) in mem.data_words() {
        let Some(&name_va) = typeinfos.get(&w) else { continue };
        let Some(ott) = mem.word(va.wrapping_sub(4)).map(|x| x as i32) else { continue };
        if va < 4 || ott > 0 || ott < -0x1_0000 {
            continue;
        }
        let first = va + 4;
        let mut slots = Vec::new();
        let mut s = first;
        while let Some(code) = mem.word(s).and_then(|p| mem.function(p)) {
            slots.push(code);
            s += 4;
        }
        if slots.is_empty() {
            continue;
        }
        let (raw, parts) = &names[&name_va];
        out.push(Vtable {
            abi: Abi::Itanium,
            raw_name: raw.clone(),
            class: Some(parts.join("::")),
            va: first,
            subobject_offset: -(ott as i64),
            slots,
            deleting_destructor: None,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starts::tests::pe_image;
    use crate::xbox::MACHINE_XENON;

    #[test]
    fn msvc_names_normalise_to_qualified_class_names() {
        assert_eq!(msvc_class(".?AVhkBaseObject@@").as_deref(), Some("hkBaseObject"));
        assert_eq!(msvc_class(".?AVBar@Foo@@").as_deref(), Some("Foo::Bar"));
        assert_eq!(msvc_class(".?AV?$vector@H@std@@"), None);
        assert_eq!(msvc_class(".?AUs@@").as_deref(), Some("s"));
    }

    #[test]
    fn itanium_names_parse_plain_and_nested() {
        assert_eq!(itanium_parts(b"12hkBaseObject"), Some(vec!["hkBaseObject".to_string()]));
        assert_eq!(itanium_parts(b"N3Foo3BarE"), Some(vec!["Foo".into(), "Bar".into()]));
        assert_eq!(itanium_parts(b"N3Foo3BarE").map(|p| p.join("::")).as_deref(), Some("Foo::Bar"));
        assert_eq!(itanium_parts(b"3Fo"), None);
        assert_eq!(itanium_parts(b"5Hello2"), None);
        assert_eq!(itanium_parts(b"hello"), None);
    }

    /// An MSVC image: a type descriptor, a hierarchy descriptor naming it first, a locator, and a
    /// two-slot vtable followed by a non-code word.
    fn msvc_image(machine: u16, big: bool) -> Vec<u8> {
        let base = 0x0040_0000u32;
        let w = |v: u32| if big { v.to_be_bytes() } else { v.to_le_bytes() };
        let rdata = 0x1000u32;
        let mut data = vec![0u8; 0x200];
        let td = base + rdata;
        data[8..8 + 16].copy_from_slice(b".?AVBar@Foo@@\0\0\0");
        let bcd = td + 0x40; // base class descriptor: first word = type descriptor
        data[0x40..0x44].copy_from_slice(&w(td));
        let bca = td + 0x60; // base class array
        data[0x60..0x64].copy_from_slice(&w(bcd));
        let chd = td + 0x80; // {signature 0, attributes, numBaseClasses 1, pBaseClassArray}
        data[0x88..0x8c].copy_from_slice(&w(1));
        data[0x8c..0x90].copy_from_slice(&w(bca));
        let col = td + 0xa0; // {0, offset 8, cdOffset, td, chd}
        data[0xa4..0xa8].copy_from_slice(&w(8));
        data[0xac..0xb0].copy_from_slice(&w(td));
        data[0xb0..0xb4].copy_from_slice(&w(chd));
        data[0xc0..0xc4].copy_from_slice(&w(col)); // meta pointer, vtable at +0xc4
        data[0xc4..0xc8].copy_from_slice(&w(base + 0x2010));
        data[0xc8..0xcc].copy_from_slice(&w(base + 0x2020));
        data[0xcc..0xd0].copy_from_slice(&w(td)); // not code: ends the vtable
        pe_image(
            machine,
            base,
            &[(".rdata", rdata, 0x200, 0x4000_0040, &data), (".text", 0x2000, 0x100, 0x6000_0020, &[])],
        )
    }

    #[test]
    fn msvc_vtables_are_found_in_both_byte_orders() {
        for (machine, big) in [(0x14c, false), (MACHINE_XENON, true)] {
            let v = vtables(&msvc_image(machine, big)).unwrap();
            assert_eq!(v.len(), 1, "machine 0x{machine:x}");
            assert_eq!(v[0].class.as_deref(), Some("Foo::Bar"));
            assert_eq!(v[0].subobject_offset, 8);
            assert_eq!(v[0].slots, vec![0x0040_2010, 0x0040_2020]);
            assert_eq!(v[0].va, 0x0040_1000 + 0xc4);
        }
    }

    #[test]
    fn a_locator_whose_hierarchy_does_not_name_it_first_is_rejected() {
        let mut img = msvc_image(0x14c, false);
        // Point the base class descriptor's type descriptor somewhere else.
        let off = 0x1000 + 0x40;
        img[off..off + 4].copy_from_slice(&0x0040_1100u32.to_le_bytes());
        assert!(vtables(&img).unwrap().is_empty());
    }
}
