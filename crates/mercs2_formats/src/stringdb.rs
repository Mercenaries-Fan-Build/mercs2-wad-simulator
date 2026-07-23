//! Stringdb (SYEK / SRTS) codec — read AND write localized string tables.
//!
//! Used by the unofficial fix pack to correct shipped typos and grammar. Read-side exists so we
//! can find a reported string; write-side exists so a correction can be **any length**, which the
//! older equal-length in-place approach (`tools/build_shell_string_patch.py`) could not do.
//!
//! # Layout (retail PC, MEASURED 2026-07-22 — see `docs/format_reference.md` §4.1)
//!
//! `docs/format_reference.md` used to claim these bodies are big-endian on every platform. That is
//! false for the PC build, and building a writer on it would have produced silent garbage. Measured
//! across all six `shell.wad` language blocks and `vz.wad`'s english block:
//!
//! * `SYEK` = `u32 key_count`, then `key_count × (u32 key_hash, u32 byte_offset)` — **little-endian**.
//! * `SRTS` = `u32 total_code_units`, then the heap — **little-endian**. Note the header counts
//!   UTF-16 **code units, not bytes**: `heap_bytes == 2 × header`, exact in all six languages.
//! * Heap strings are NUL-terminated UTF-16**LE**. `SYEK` offsets are **byte** offsets from the
//!   start of the heap (i.e. from SRTS body + 4), not code-unit offsets.
//!
//! Endianness is still *detected* rather than assumed, so this keeps working if a big-endian
//! (Xbox) table is ever fed through it.

use crate::hash::pandemic_hash_m2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Endian {
    Le,
    Be,
}

impl Endian {
    fn u32(self, d: &[u8], off: usize) -> u32 {
        let b = [d[off], d[off + 1], d[off + 2], d[off + 3]];
        match self {
            Endian::Le => u32::from_le_bytes(b),
            Endian::Be => u32::from_be_bytes(b),
        }
    }
    fn u16(self, d: &[u8], off: usize) -> u16 {
        let b = [d[off], d[off + 1]];
        match self {
            Endian::Le => u16::from_le_bytes(b),
            Endian::Be => u16::from_be_bytes(b),
        }
    }
    fn put_u32(self, out: &mut Vec<u8>, v: u32) {
        match self {
            Endian::Le => out.extend_from_slice(&v.to_le_bytes()),
            Endian::Be => out.extend_from_slice(&v.to_be_bytes()),
        }
    }
    fn put_u16(self, out: &mut Vec<u8>, v: u16) {
        match self {
            Endian::Le => out.extend_from_slice(&v.to_le_bytes()),
            Endian::Be => out.extend_from_slice(&v.to_be_bytes()),
        }
    }
}

/// One row of the key table, in the file's original order.
#[derive(Clone, Debug)]
pub struct StringEntry {
    pub key_hash: u32,
    /// Byte offset into the heap as stored on disk. Preserved so an unmodified rebuild can be
    /// proven byte-identical against retail before we trust the writer with real edits.
    pub offset: u32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct StringDb {
    pub entries: Vec<StringEntry>,
    pub endian: Endian,
    /// `SRTS` header value as read (code units). Kept for round-trip fidelity checks.
    pub declared_code_units: u32,
    pub heap_bytes: usize,
}

/// Pick the endianness under which the count is a plausible table size for the buffer.
fn detect(body: &[u8], bytes_per_entry: usize) -> Option<(Endian, u32)> {
    if body.len() < 4 {
        return None;
    }
    let fits = |e: Endian| {
        let n = e.u32(body, 0) as usize;
        n.checked_mul(bytes_per_entry)
            .filter(|need| 4 + need <= body.len())
            .map(|_| n)
    };
    match (fits(Endian::Be), fits(Endian::Le)) {
        // Both readings fit only when one is a small number byte-swapped into a smaller one; the
        // larger count is the tighter — and therefore real — fit.
        (Some(b), Some(l)) => Some(if b >= l { (Endian::Be, b as u32) } else { (Endian::Le, l as u32) }),
        (Some(b), None) => Some((Endian::Be, b as u32)),
        (None, Some(l)) => Some((Endian::Le, l as u32)),
        (None, None) => None,
    }
}

fn read_utf16(heap: &[u8], off: usize, e: Endian) -> String {
    let mut units = Vec::new();
    let mut p = off;
    while p + 2 <= heap.len() {
        let u = e.u16(heap, p);
        p += 2;
        if u == 0 {
            break;
        }
        units.push(u);
    }
    char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')).collect()
}

/// How ASCII-like is this decode? Settles heap endianness independently of the header.
fn ascii_score(heap: &[u8], e: Endian) -> usize {
    (0..heap.len().min(4096) / 2)
        .filter(|i| {
            let u = e.u16(heap, i * 2);
            (0x20..0x7F).contains(&u) || u == 0 || u == 0x0A
        })
        .count()
}

/// Parse a stringdb from its two raw chunk bodies (as returned by `ucfx::extract_chunk_body`).
pub fn parse(syek: &[u8], srts: &[u8]) -> Result<StringDb, String> {
    let (kend, count) = detect(syek, 8).ok_or("SYEK: key_count implausible under BE and LE")?;
    if srts.len() < 4 {
        return Err(format!("SRTS too short ({} bytes)", srts.len()));
    }
    let heap = &srts[4..];
    let tend = if ascii_score(heap, Endian::Le) >= ascii_score(heap, Endian::Be) {
        Endian::Le
    } else {
        Endian::Be
    };
    let declared = kend.u32(srts, 0);

    let mut entries = Vec::with_capacity(count as usize);
    for k in 0..count as usize {
        let base = 4 + k * 8;
        if base + 8 > syek.len() {
            return Err(format!("SYEK truncated at entry {k}"));
        }
        let key_hash = kend.u32(syek, base);
        let offset = kend.u32(syek, base + 4);
        if offset as usize > heap.len() {
            return Err(format!("SYEK entry {k}: offset {offset} past heap ({})", heap.len()));
        }
        entries.push(StringEntry { key_hash, offset, text: read_utf16(heap, offset as usize, tend) });
    }

    Ok(StringDb { entries, endian: kend, declared_code_units: declared, heap_bytes: heap.len() })
}

/// Serialize back to `(SYEK, SRTS)` bodies.
///
/// The heap is rebuilt from the decoded text, laid out in **ascending original offset** order so
/// that an unmodified table reproduces retail byte-for-byte (see `roundtrip_*` tests). Entries
/// sharing an offset in the original — the engine dedupes identical strings — stay shared.
pub fn build(db: &StringDb) -> (Vec<u8>, Vec<u8>) {
    let e = db.endian;

    // Group by original offset so shared strings are emitted once and stay shared.
    let mut order: Vec<usize> = (0..db.entries.len()).collect();
    order.sort_by_key(|&i| (db.entries[i].offset, i));

    let mut heap: Vec<u8> = Vec::with_capacity(db.heap_bytes);
    let mut new_offset = vec![0u32; db.entries.len()];
    let mut prev: Option<(u32, u32, usize)> = None; // (original offset, new offset, entry index)

    for &i in &order {
        let ent = &db.entries[i];
        if let Some((orig, newo, previ)) = prev {
            // Share the already-written copy ONLY if the text is still identical. Comparing
            // offsets alone would be a silent corruption: editing one of two keys that shared an
            // offset in retail would drag the other along with it.
            if ent.offset == orig && db.entries[previ].text == ent.text {
                new_offset[i] = newo;
                continue;
            }
        }
        let at = heap.len() as u32;
        for u in ent.text.encode_utf16() {
            e.put_u16(&mut heap, u);
        }
        e.put_u16(&mut heap, 0);
        new_offset[i] = at;
        prev = Some((ent.offset, at, i));
    }

    let mut syek = Vec::with_capacity(4 + db.entries.len() * 8);
    e.put_u32(&mut syek, db.entries.len() as u32);
    for (i, ent) in db.entries.iter().enumerate() {
        e.put_u32(&mut syek, ent.key_hash);
        e.put_u32(&mut syek, new_offset[i]);
    }

    let mut srts = Vec::with_capacity(4 + heap.len());
    // Header counts UTF-16 code units, not bytes.
    e.put_u32(&mut srts, (heap.len() / 2) as u32);
    srts.extend_from_slice(&heap);

    (syek, srts)
}

impl StringDb {
    /// Replace the text for a key. Returns false if the key is not present — callers should treat
    /// that as a hard error, since a silently-dropped fix is worse than a failed build.
    pub fn set_by_hash(&mut self, key_hash: u32, text: &str) -> bool {
        let mut hit = false;
        for ent in self.entries.iter_mut().filter(|e| e.key_hash == key_hash) {
            ent.text = text.to_string();
            hit = true;
        }
        hit
    }

    /// Replace by key name (e.g. `"[OilCon001.Objectives.001]"`), hashing it the way the engine does.
    pub fn set_by_name(&mut self, key_name: &str, text: &str) -> bool {
        self.set_by_hash(pandemic_hash_m2(key_name), text)
    }

    /// Replace every entry whose current text is exactly `old`. Returns how many changed.
    ///
    /// This is the fix-pack's primary entry point: community reports name a string by the text the
    /// player sees, not by a key. Requiring an exact full-string match keeps it from mangling
    /// unrelated lines that merely contain the phrase.
    pub fn replace_exact_text(&mut self, old: &str, new: &str) -> usize {
        let mut n = 0;
        for ent in self.entries.iter_mut().filter(|e| e.text == old) {
            ent.text = new.to_string();
            n += 1;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth(endian: Endian, rows: &[(u32, &str)]) -> (Vec<u8>, Vec<u8>) {
        let entries: Vec<StringEntry> = {
            let mut heap_len = 0u32;
            rows.iter()
                .map(|(h, t)| {
                    let off = heap_len;
                    heap_len += (t.encode_utf16().count() as u32 + 1) * 2;
                    StringEntry { key_hash: *h, offset: off, text: t.to_string() }
                })
                .collect()
        };
        let heap_bytes = entries.iter().map(|e| (e.text.encode_utf16().count() + 1) * 2).sum();
        build(&StringDb { entries, endian, declared_code_units: 0, heap_bytes })
    }

    #[test]
    fn roundtrip_le() {
        let (syek, srts) = synth(Endian::Le, &[(0xAABB_CCDD, "Hello"), (0x1234_5678, "World!")]);
        let db = parse(&syek, &srts).expect("parse");
        assert_eq!(db.endian, Endian::Le);
        assert_eq!(db.entries.len(), 2);
        assert_eq!(db.entries[0].text, "Hello");
        assert_eq!(db.entries[1].text, "World!");
        // SRTS header is a code-unit count: "Hello\0" + "World!\0" = 6 + 7 = 13 units.
        assert_eq!(db.declared_code_units, 13);
        assert_eq!(db.heap_bytes, 26);
        let (s2, r2) = build(&db);
        assert_eq!((s2, r2), (syek, srts), "rebuild must be byte-identical");
    }

    #[test]
    fn roundtrip_be() {
        let (syek, srts) = synth(Endian::Be, &[(0x0000_0101, "Xbox"), (0x0000_0202, "Table")]);
        let db = parse(&syek, &srts).expect("parse");
        assert_eq!(db.endian, Endian::Be);
        assert_eq!(db.entries[1].text, "Table");
        let (s2, r2) = build(&db);
        assert_eq!((s2, r2), (syek, srts));
    }

    #[test]
    fn shared_offsets_stay_shared() {
        // Two keys pointing at one string — the engine dedupes, and so must we, or the heap grows
        // on every rebuild.
        let mut heap = Vec::new();
        for u in "Same".encode_utf16() {
            heap.extend_from_slice(&u.to_le_bytes());
        }
        heap.extend_from_slice(&0u16.to_le_bytes());
        let mut syek = Vec::new();
        syek.extend_from_slice(&2u32.to_le_bytes());
        syek.extend_from_slice(&1u32.to_le_bytes());
        syek.extend_from_slice(&0u32.to_le_bytes());
        syek.extend_from_slice(&2u32.to_le_bytes());
        syek.extend_from_slice(&0u32.to_le_bytes());
        let mut srts = ((heap.len() / 2) as u32).to_le_bytes().to_vec();
        srts.extend_from_slice(&heap);

        let db = parse(&syek, &srts).expect("parse");
        assert_eq!(db.entries[0].text, "Same");
        assert_eq!(db.entries[1].text, "Same");
        let (s2, r2) = build(&db);
        assert_eq!(r2.len(), srts.len(), "shared string must not be duplicated into the heap");
        assert_eq!((s2, r2), (syek, srts));
    }

    #[test]
    fn longer_replacement_repoints_offsets() {
        let (syek, srts) = synth(Endian::Le, &[(1, "short"), (2, "tail")]);
        let mut db = parse(&syek, &srts).expect("parse");
        assert!(db.set_by_hash(1, "a considerably longer correction"));
        let (s2, r2) = build(&db);
        let db2 = parse(&s2, &r2).expect("reparse");
        assert_eq!(db2.entries[0].text, "a considerably longer correction");
        assert_eq!(db2.entries[1].text, "tail", "the following string must survive re-pointing");
        assert_eq!(db2.declared_code_units as usize * 2, r2.len() - 4);
    }

    /// Regression: editing one of two keys that shared a heap offset must NOT drag the other with
    /// it. Offset-equality alone is not sufficient grounds to share a string.
    #[test]
    fn editing_one_of_a_shared_pair_does_not_affect_the_other() {
        let mut heap = Vec::new();
        for u in "Continue".encode_utf16() {
            heap.extend_from_slice(&u.to_le_bytes());
        }
        heap.extend_from_slice(&0u16.to_le_bytes());
        let mut syek = 2u32.to_le_bytes().to_vec();
        for h in [0xAAAA_u32, 0xBBBB_u32] {
            syek.extend_from_slice(&h.to_le_bytes());
            syek.extend_from_slice(&0u32.to_le_bytes()); // both point at offset 0
        }
        let mut srts = ((heap.len() / 2) as u32).to_le_bytes().to_vec();
        srts.extend_from_slice(&heap);

        let mut db = parse(&syek, &srts).expect("parse");
        assert_eq!(db.entries[0].text, "Continue");
        assert_eq!(db.entries[1].text, "Continue");

        assert!(db.set_by_hash(0xAAAA, "Resume"));
        let (s2, r2) = build(&db);
        let back = parse(&s2, &r2).expect("reparse");
        assert_eq!(back.entries[0].text, "Resume");
        assert_eq!(back.entries[1].text, "Continue", "the co-located key must be untouched");
        assert_ne!(back.entries[0].offset, back.entries[1].offset, "they must no longer share");
    }

    #[test]
    fn missing_key_reports_failure() {
        let (syek, srts) = synth(Endian::Le, &[(1, "x")]);
        let mut db = parse(&syek, &srts).expect("parse");
        assert!(!db.set_by_hash(0xDEAD, "y"), "a fix aimed at a missing key must not silently pass");
    }
}
