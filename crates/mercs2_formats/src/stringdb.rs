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
        (Some(b), Some(l)) => Some(if b >= l {
            (Endian::Be, b as u32)
        } else {
            (Endian::Le, l as u32)
        }),
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
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
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
            return Err(format!(
                "SYEK entry {k}: offset {offset} past heap ({})",
                heap.len()
            ));
        }
        entries.push(StringEntry {
            key_hash,
            offset,
            text: read_utf16(heap, offset as usize, tend),
        });
    }

    Ok(StringDb {
        entries,
        endian: kend,
        declared_code_units: declared,
        heap_bytes: heap.len(),
    })
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

/// Extract a named chunk's body range `(start, len)` from a `UCFX` container. Little-endian
/// descriptor table, the layout the whole container family shares.
fn chunk_range(container: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let le = |o: usize| u32::from_le_bytes([container[o], container[o + 1], container[o + 2], container[o + 3]]) as usize;
    let data_area = le(4);
    let n = le(16);
    for i in 0..n {
        let row = 20 + i * 20;
        if row + 20 > container.len() || &container[row..row + 4] != tag {
            continue;
        }
        let rel = le(row + 4);
        if rel == 0xFFFF_FFFF {
            return None; // a nested container, not a body
        }
        let start = data_area + rel;
        let size = le(row + 8);
        if start + size <= container.len() {
            return Some((start, size));
        }
    }
    None
}

/// Apply text edits to a stringdb `UCFX` container and return the rebuilt container.
///
/// This is what a `edit_stringdb` contribution lowers through, and it is the SAME writer the fix
/// pack proved against retail (`wad_builder::stringdb_patch`) — promoted out of that binary so the
/// Quartermaster can reach it, since Plan 05 §C names the binary-only crates as the recurring tax.
///
/// `edits` maps a bracket key (`"[DlcCon001.Title]"`) — hashed the way the engine does — to its new
/// text. A key not present is a hard error and is NAMED: a silently-dropped correction is worse than
/// a failed build, and it is the exact failure the SYEK/KEYS tag confusion once produced ("0
/// containers checked" against a WAD that plainly had six).
///
/// Arbitrary-length edits are supported: the heap is rebuilt and the `KEYS`/`STRS` descriptors are
/// re-pointed, then the trailing `CSUM` is re-stamped. Retail's shared-string dedupe is preserved by
/// `build`, which only shares a heap slot when the offset AND the text still match.
pub fn edit_container(
    container: &[u8],
    edits: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<u8>, String> {
    let (ks, kl) = chunk_range(container, b"KEYS").ok_or("container has no KEYS chunk")?;
    let (ss, sl) = chunk_range(container, b"STRS").ok_or("container has no STRS chunk")?;
    let mut db = parse(&container[ks..ks + kl], &container[ss..ss + sl])?;

    for (key, text) in edits {
        // A bare `0xHHHHHHHH` IS the key hash; anything else is a bracket key, hashed the way the
        // engine does. Same rule as `manifest::asset_hash`, so an author who only has the hash from
        // a dump can still edit — the reverse of a bracket key is not always known.
        let hit = match key
            .trim()
            .strip_prefix("0x")
            .or_else(|| key.trim().strip_prefix("0X"))
            .filter(|h| h.len() <= 8 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .and_then(|h| u32::from_str_radix(h, 16).ok())
        {
            Some(h) => db.set_by_hash(h, text),
            None => db.set_by_name(key, text),
        };
        if !hit {
            return Err(format!(
                "{key} is not a key in this string table — check the spelling; the engine hashes \
                 the bracket key verbatim, so an unknown key resolves to a lookup that simply misses"
            ));
        }
    }

    let (new_keys, new_strs) = build(&db);
    rebuild_container(container, &[(*b"KEYS", &new_keys), (*b"STRS", &new_strs)])
}

/// Rebuild a `UCFX` container substituting new bodies for some chunks, re-laying out everything
/// after them and re-stamping the trailing `CSUM`.
///
/// Relies on the measured invariant that the family's chunk bodies are **contiguous** — no
/// alignment padding — so the rebuild is header, bodies in shipped order (with substitutions), then
/// the 8-byte `CSUM` trailer. A container with padding between bodies is REFUSED rather than
/// guessed at: reproducing an unknown padding scheme would emit a container that validates and is
/// subtly wrong, which is the failure mode this whole codec exists to avoid.
fn rebuild_container(container: &[u8], replacements: &[([u8; 4], &[u8])]) -> Result<Vec<u8>, String> {
    if container.len() < 28 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let le = |o: usize| u32::from_le_bytes([container[o], container[o + 1], container[o + 2], container[o + 3]]);
    let base = le(4) as usize;
    let n = le(16) as usize;

    // (row, body_start, size), body descriptors only, in file order.
    let mut bodies: Vec<(usize, usize, usize)> = Vec::new();
    for i in 0..n {
        let row = 20 + i * 20;
        if row + 20 > container.len() {
            return Err(format!("descriptor {i} runs past the container"));
        }
        let rel = le(row + 4);
        if rel == 0xFFFF_FFFF {
            continue; // nested container, no body
        }
        let start = base + rel as usize;
        let size = le(row + 8) as usize;
        if start + size > container.len() {
            return Err(format!("descriptor {i} body runs past the container"));
        }
        bodies.push((row, start, size));
    }
    bodies.sort_by_key(|b| b.1);

    let mut cursor = base;
    for (_, start, size) in &bodies {
        if *start != cursor {
            return Err(format!(
                "a chunk starts at 0x{start:X} but the previous body ended at 0x{cursor:X} — this \
                 container has padding between bodies, which this rebuild does not reproduce. \
                 Refusing rather than emitting a container that validates and is wrong."
            ));
        }
        cursor += size;
    }
    if container.len() != cursor + 8 {
        return Err(format!(
            "container is {} B but bodies end at 0x{cursor:X} + an 8 B CSUM trailer; unexpected tail",
            container.len()
        ));
    }

    let mut out = Vec::with_capacity(container.len());
    out.extend_from_slice(&container[..base]);
    let mut repoint: Vec<(usize, u32, u32)> = Vec::new(); // (row, new_rel, new_size)
    // Descriptor rows are in table order; bodies were sorted by position. Re-emit in FILE order so
    // the original layout is preserved for every chunk the caller did not touch.
    let mut file_order = bodies.clone();
    file_order.sort_by_key(|b| b.0); // by row = table order
    // But bytes must be written in POSITION order to stay contiguous; retail's table order matches
    // its position order for this family, so a single ordering suffices — assert it.
    for (a, b) in bodies.iter().zip(file_order.iter()) {
        if a.0 != b.0 {
            return Err("descriptor table order differs from body position order; unsupported".into());
        }
    }
    for (row, start, size) in &bodies {
        let rel = (out.len() - base) as u32;
        let body: &[u8] = replacements
            .iter()
            .find(|(t, _)| container[*row..*row + 4] == *t)
            .map(|(_, b)| *b)
            .unwrap_or(&container[*start..*start + *size]);
        out.extend_from_slice(body);
        repoint.push((*row, rel, body.len() as u32));
    }
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&[0u8; 4]);
    for (row, rel, size) in repoint {
        out[row + 4..row + 8].copy_from_slice(&rel.to_le_bytes());
        out[row + 8..row + 12].copy_from_slice(&size.to_le_bytes());
    }

    // Re-stamp CSUM (JAMCRC over everything before the 8-byte trailer).
    let n = out.len();
    let crc = crate::crc32::crc32_mercs2(&out[..n - 8]);
    out[n - 4..].copy_from_slice(&crc.to_le_bytes());
    Ok(out)
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
                    StringEntry {
                        key_hash: *h,
                        offset: off,
                        text: t.to_string(),
                    }
                })
                .collect()
        };
        let heap_bytes = entries
            .iter()
            .map(|e| (e.text.encode_utf16().count() + 1) * 2)
            .sum();
        build(&StringDb {
            entries,
            endian,
            declared_code_units: 0,
            heap_bytes,
        })
    }

    #[test]
    fn roundtrip_le() {
        let (syek, srts) = synth(
            Endian::Le,
            &[(0xAABB_CCDD, "Hello"), (0x1234_5678, "World!")],
        );
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
        assert_eq!(
            r2.len(),
            srts.len(),
            "shared string must not be duplicated into the heap"
        );
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
        assert_eq!(
            db2.entries[1].text, "tail",
            "the following string must survive re-pointing"
        );
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
        assert_eq!(
            back.entries[1].text, "Continue",
            "the co-located key must be untouched"
        );
        assert_ne!(
            back.entries[0].offset, back.entries[1].offset,
            "they must no longer share"
        );
    }

    #[test]
    fn missing_key_reports_failure() {
        let (syek, srts) = synth(Endian::Le, &[(1, "x")]);
        let mut db = parse(&syek, &srts).expect("parse");
        assert!(
            !db.set_by_hash(0xDEAD, "y"),
            "a fix aimed at a missing key must not silently pass"
        );
    }
}

#[cfg(test)]
mod container_tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Build a tiny INFO/KEYS/STRS container the way retail lays one out, so the re-splice can be
    /// tested without a game install. Two keys, one shared string, to exercise the dedupe path.
    fn tiny_container() -> Vec<u8> {
        // Two entries, both pointing at heap offset 0 ("Hi\0"): a shared string.
        let mut keys = Vec::new();
        keys.extend_from_slice(&2u32.to_le_bytes());
        keys.extend_from_slice(&0xAAAA_0001u32.to_le_bytes());
        keys.extend_from_slice(&0u32.to_le_bytes());
        keys.extend_from_slice(&0xAAAA_0002u32.to_le_bytes());
        keys.extend_from_slice(&0u32.to_le_bytes());
        let mut strs = Vec::new();
        let heap: Vec<u16> = "Hi".encode_utf16().chain([0]).collect();
        strs.extend_from_slice(&(heap.len() as u32).to_le_bytes());
        for u in &heap {
            strs.extend_from_slice(&u.to_le_bytes());
        }
        let info = vec![0u8; 8];

        // UCFX: header (20) + 3 descriptors (20 each) + bodies + CSUM.
        let data_area = 20 + 3 * 20;
        let mut c = Vec::new();
        c.extend_from_slice(b"UCFX");
        c.extend_from_slice(&(data_area as u32).to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&3u32.to_le_bytes());
        let mut rel = 0u32;
        for (tag, body) in [(b"INFO", &info), (b"KEYS", &keys), (b"STRS", &strs)] {
            c.extend_from_slice(tag);
            c.extend_from_slice(&rel.to_le_bytes());
            c.extend_from_slice(&(body.len() as u32).to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
            rel += body.len() as u32;
        }
        c.extend_from_slice(&info);
        c.extend_from_slice(&keys);
        c.extend_from_slice(&strs);
        let crc = crate::crc32::crc32_mercs2(&c);
        c.extend_from_slice(b"CSUM");
        c.extend_from_slice(&crc.to_le_bytes());
        c
    }

    #[test]
    fn a_noop_edit_rebuilds_byte_identical() {
        let c = tiny_container();
        // An edit that sets a key to the text it already has is a no-op; the container must come
        // back byte-for-byte, which is what makes any post-edit difference attributable to the edit.
        let mut edits = BTreeMap::new();
        edits.insert("0xAAAA0001".to_string(), "Hi".to_string());
        let out = edit_container(&c, &edits).expect("edit");
        assert_eq!(out, c, "a no-op edit must reproduce the container exactly");
    }

    #[test]
    fn a_longer_edit_grows_the_container_and_reparses() {
        let c = tiny_container();
        let mut edits = BTreeMap::new();
        edits.insert("0xAAAA0001".to_string(), "Hello there".to_string());
        let out = edit_container(&c, &edits).expect("edit");
        assert!(out.len() > c.len(), "a longer string must grow the container");

        // Re-extract and re-parse: the edited key changed, the OTHER key is intact, and — the trap
        // the codec exists to avoid — the shared neighbour was NOT dragged along.
        let (ks, kl) = chunk_range(&out, b"KEYS").unwrap();
        let (ss, sl) = chunk_range(&out, b"STRS").unwrap();
        let db = parse(&out[ks..ks + kl], &out[ss..ss + sl]).unwrap();
        let by = |h: u32| db.entries.iter().find(|e| e.key_hash == h).map(|e| e.text.as_str());
        assert_eq!(by(0xAAAA_0001), Some("Hello there"));
        assert_eq!(by(0xAAAA_0002), Some("Hi"), "the shared neighbour must be untouched");
    }

    #[test]
    fn an_unknown_key_is_named_not_silently_dropped() {
        let c = tiny_container();
        let mut edits = BTreeMap::new();
        edits.insert("[No.Such.Key]".to_string(), "x".to_string());
        let err = edit_container(&c, &edits).expect_err("must reject an absent key");
        assert!(err.contains("[No.Such.Key]"), "the refusal must name the key: {err}");
    }
}
