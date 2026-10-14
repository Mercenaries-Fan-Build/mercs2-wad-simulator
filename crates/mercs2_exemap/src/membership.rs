//! Layout membership: compare a build with its layout reference and require every difference to be
//! a catalogued overlay.
//!
//! The comparison produces *runs* — maximal ranges of differing bytes — in four passes:
//!
//! 1. **Headers.** RVA `[0, SizeOfHeaders)` of both images, byte for byte. The two must declare the
//!    same `SizeOfHeaders` and `ImageBase`.
//! 2. **Reference sections.** For each reference section, its whole mapped extent against the build's
//!    image at the same RVAs ([`crate::image`] defines the image). A byte the build does not map is a
//!    difference; a run never mixes mapped and unmapped bytes.
//! 3. **Build-only extents.** Build RVAs that the reference maps nowhere, labelled with the build's
//!    section name; every such byte is a difference.
//! 4. **File slack.** File offsets that either file leaves outside its headers and raw section data,
//!    compared by file offset: a byte differs when the other file lacks it or holds another value.
//!
//! Runs are labelled `(headers)`, a section name, or `(slack)`; header and section runs are in VA
//! space (`ImageBase + RVA`), slack runs in file-offset space.
//!
//! Each run must fall inside exactly one overlay row of the build's set with the same label and
//! space. A run in no row, a run crossing a row boundary, a row with no runs, and a row whose range,
//! run count, differing-byte count or sha256 disagrees with what the bytes give are all errors.

use std::ops::Range;

use sha2::{Digest, Sha256};

use crate::catalogue::{Catalogue, Kind, Overlay, Space};
use crate::image::Image;
use crate::Error;

/// Label of the header pseudo-section.
pub const HEADERS: &str = "(headers)";
/// Label of the file-slack pseudo-section.
pub const SLACK: &str = "(slack)";

/// One maximal range of differing bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub section: String,
    pub space: Space,
    pub start: u64,
    pub end: u64,
    /// `false` when the build does not hold these bytes at all.
    pub mapped: bool,
}

fn push_diff_runs(reference: &[u8], build: &[u8], base: u64, section: &str, out: &mut Vec<Run>) {
    debug_assert_eq!(reference.len(), build.len());
    let mut open: Option<usize> = None;
    for k in 0..reference.len() {
        let differs = reference[k] != build[k];
        match (differs, open) {
            (true, None) => open = Some(k),
            (false, Some(s)) => {
                out.push(Run { section: section.to_string(), space: Space::Va, start: base + s as u64, end: base + k as u64, mapped: true });
                open = None;
            }
            _ => {}
        }
    }
    if let Some(s) = open {
        out.push(Run {
            section: section.to_string(),
            space: Space::Va,
            start: base + s as u64,
            end: base + reference.len() as u64,
            mapped: true,
        });
    }
}

fn merge(mut ranges: Vec<Range<u64>>) -> Vec<Range<u64>> {
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<Range<u64>> = Vec::new();
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// Every run of differing bytes between `build` and `reference`, in pass order.
pub fn runs(reference: &Image, build: &Image) -> Result<Vec<Run>, Error> {
    let (rp, bp) = (&reference.pe, &build.pe);
    if bp.size_of_headers != rp.size_of_headers {
        return Err(Error::new(format!(
            "SizeOfHeaders differs: build 0x{:x}, reference 0x{:x}",
            bp.size_of_headers, rp.size_of_headers
        )));
    }
    if bp.image_base != rp.image_base {
        return Err(Error::new(format!(
            "ImageBase differs: build 0x{:x}, reference 0x{:x}",
            bp.image_base, rp.image_base
        )));
    }
    let base = rp.image_base as u64;
    let mut out = Vec::new();

    let soh = rp.size_of_headers as usize;
    push_diff_runs(&reference.bytes()[..soh], &build.bytes()[..soh], base, HEADERS, &mut out);

    for (i, s) in rp.sections.iter().enumerate() {
        let extent = reference.section_extent(i);
        let want = reference.mapped(extent.clone())?;
        for piece in build.view(extent.clone()) {
            let at = (piece.rva.start - extent.start) as usize;
            let len = (piece.rva.end - piece.rva.start) as usize;
            match piece.bytes {
                None => out.push(Run {
                    section: s.name.clone(),
                    space: Space::Va,
                    start: base + piece.rva.start as u64,
                    end: base + piece.rva.end as u64,
                    mapped: false,
                }),
                Some(have) => push_diff_runs(&want[at..at + len], &have, base + piece.rva.start as u64, &s.name, &mut out),
            }
        }
    }

    for (i, s) in bp.sections.iter().enumerate() {
        for piece in reference.view(build.section_extent(i)) {
            if piece.bytes.is_none() {
                out.push(Run {
                    section: s.name.clone(),
                    space: Space::Va,
                    start: base + piece.rva.start as u64,
                    end: base + piece.rva.end as u64,
                    mapped: true,
                });
            }
        }
    }

    let (r, b) = (reference.bytes(), build.bytes());
    for range in merge(reference.slack().iter().chain(build.slack()).cloned().collect()) {
        let mut open: Option<(u64, bool)> = None;
        for o in range.clone() {
            let mapped = (o as usize) < b.len();
            let differs = !mapped || o as usize >= r.len() || b[o as usize] != r[o as usize];
            let state = differs.then_some(mapped);
            if let Some((s, m)) = open {
                if state != Some(m) {
                    out.push(Run { section: SLACK.to_string(), space: Space::File, start: s, end: o, mapped: m });
                    open = None;
                }
            }
            if open.is_none() {
                if let Some(m) = state {
                    open = Some((o, m));
                }
            }
        }
        if let Some((s, m)) = open {
            out.push(Run { section: SLACK.to_string(), space: Space::File, start: s, end: range.end, mapped: m });
        }
    }
    Ok(out)
}

/// sha256 of the build's bytes over an overlay range, lowercase hex.
fn range_sha256(build: &Image, space: Space, start: u64, end: u64) -> Result<String, Error> {
    let bytes = match space {
        Space::File => build
            .bytes()
            .get(start as usize..end as usize)
            .ok_or_else(|| Error::new(format!("file range 0x{start:x}..0x{end:x} is past the end of the build")))?
            .to_vec(),
        Space::Va => {
            let base = build.pe.image_base as u64;
            if start < base || end - base > u32::MAX as u64 {
                return Err(Error::new(format!("VA range 0x{start:x}..0x{end:x} is outside the image")));
            }
            build.mapped((start - base) as u32..(end - base) as u32)?
        }
    };
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn describe(r: &Run) -> String {
    format!(
        "{} {} 0x{:x}..0x{:x} ({} bytes{})",
        r.section,
        r.space.as_str(),
        r.start,
        r.end,
        r.end - r.start,
        if r.mapped { "" } else { ", unmapped in the build" }
    )
}

/// Check `build` against `reference` using `catalogue`, returning the build's overlay rows.
pub fn check<'c>(catalogue: &'c Catalogue, build: &[u8], reference: &[u8]) -> Result<Vec<&'c Overlay>, Error> {
    let b = crate::identify_in(catalogue, build)?;
    let r = crate::identify_in(catalogue, reference)?;
    let layout = catalogue
        .layout(&b.layout)
        .ok_or_else(|| Error::new(format!("build {} names layout {}, which is not catalogued", b.id, b.layout)))?;
    if r.sha256 != layout.reference_sha256 {
        return Err(Error::new(format!(
            "{} is not the reference of layout {} (build {}); the reference is sha256 {}",
            r.id, layout.id, b.id, layout.reference_sha256
        )));
    }

    let ri = Image::parse(reference)?;
    let bi = Image::parse(build)?;
    let (rp, bp) = (&ri.pe, &bi.pe);
    let table: Vec<_> = layout
        .sections
        .iter()
        .map(|s| (s.name.as_str(), s.rva, s.virtual_size, s.raw_offset, s.raw_size))
        .collect();
    let parsed: Vec<_> = rp
        .sections
        .iter()
        .map(|s| (s.name.as_str(), s.vaddr, s.vsize, s.raw_ptr, s.raw_size))
        .collect();
    if table != parsed
        || rp.timestamp != layout.pe_timestamp
        || rp.image_base != layout.image_base
        || rp.section_alignment != layout.section_alignment
        || rp.size_of_headers != layout.size_of_headers
    {
        return Err(Error::new(format!("the reference's PE headers disagree with layouts.tsv for layout {}", layout.id)));
    }
    if bp.timestamp != b.pe_timestamp {
        return Err(Error::new(format!(
            "build {} has PE timestamp 0x{:08x}, builds.tsv says 0x{:08x}",
            b.id, bp.timestamp, b.pe_timestamp
        )));
    }

    let found = runs(&ri, &bi)?;
    let rows: Vec<&Overlay> = match &b.overlay_set {
        None => Vec::new(),
        Some(set) => catalogue.overlay_set(set),
    };

    let mut assigned: Vec<Vec<&Run>> = vec![Vec::new(); rows.len()];
    let mut unexpected: Vec<String> = Vec::new();
    for run in &found {
        let inside = rows.iter().position(|o| {
            o.section == run.section && o.space == run.space && o.start <= run.start && run.end <= o.end
        });
        match inside {
            Some(i) => assigned[i].push(run),
            None => {
                let straddle = rows
                    .iter()
                    .find(|o| o.space == run.space && o.start < run.end && run.start < o.end);
                unexpected.push(match straddle {
                    Some(o) => format!("{} crosses the boundary of the {} overlay 0x{:x}..0x{:x}", describe(run), o.kind.as_str(), o.start, o.end),
                    None => describe(run),
                });
            }
        }
    }
    if !unexpected.is_empty() {
        let shown: Vec<&str> = unexpected.iter().take(20).map(String::as_str).collect();
        return Err(Error::new(format!(
            "build {} has {} difference(s) from layout {} that no overlay lists:\n  {}{}",
            b.id,
            unexpected.len(),
            layout.id,
            shown.join("\n  "),
            if unexpected.len() > shown.len() { "\n  …" } else { "" }
        )));
    }

    for (o, rs) in rows.iter().zip(&assigned) {
        let label = format!("{} {} overlay {} 0x{:x}..0x{:x}", b.id, o.kind.as_str(), o.section, o.start, o.end);
        let (first, last) = match (rs.first(), rs.last()) {
            (Some(f), Some(l)) => (f, l),
            _ => return Err(Error::new(format!("{label}: the bytes show no difference there"))),
        };
        if first.start != o.start || last.end != o.end {
            return Err(Error::new(format!(
                "{label}: the differences span 0x{:x}..0x{:x}",
                first.start, last.end
            )));
        }
        let n = rs.len() as u64;
        let bytes: u64 = rs.iter().map(|r| r.end - r.start).sum();
        if n != o.runs || bytes != o.diff_bytes {
            return Err(Error::new(format!(
                "{label}: {n} run(s) / {bytes} differing bytes, the table says {} / {}",
                o.runs, o.diff_bytes
            )));
        }
        let mapped = rs.iter().all(|r| r.mapped);
        let unmapped = rs.iter().all(|r| !r.mapped);
        match (o.kind, mapped, unmapped) {
            (Kind::Unmapped, _, true) => {}
            (Kind::Unmapped, _, false) => return Err(Error::new(format!("{label}: the build maps bytes inside it"))),
            (_, true, _) => {
                let got = range_sha256(&bi, o.space, o.start, o.end)?;
                if Some(&got) != o.sha256.as_ref() {
                    return Err(Error::new(format!(
                        "{label}: sha256 {got}, the table says {}",
                        o.sha256.as_deref().unwrap_or("-")
                    )));
                }
            }
            (_, false, _) => return Err(Error::new(format!("{label}: the build does not map all of it"))),
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::synth::{pe, Sec};

    fn sha(b: &[u8]) -> String {
        format!("{:x}", Sha256::digest(b))
    }
    fn md5(b: &[u8]) -> String {
        use md5::Md5;
        format!("{:x}", Md5::digest(b))
    }

    fn reference() -> Vec<u8> {
        pe(
            0x11,
            &[
                Sec { name: ".text", rva: 0x1000, virtual_size: 0x100, raw_offset: 0x200, raw: (0..=255u8).collect() },
                Sec { name: ".data", rva: 0x2000, virtual_size: 0x100, raw_offset: 0x300, raw: vec![7; 0x100] },
            ],
            b"",
        )
    }

    /// The reference with `.text` bytes 0x10..0x14 replaced and an 8-byte tail.
    fn member() -> Vec<u8> {
        let mut d = reference();
        d[0x210..0x214].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        d.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        d
    }

    fn catalogue(reference: &[u8], member: &[u8], overlays: &str) -> Catalogue {
        let layouts = format!(
            "layout_id\tpe_timestamp\treference_sha256\timage_base\tsection_alignment\tsize_of_headers\tsection\trva\tvirtual_size\traw_offset\traw_size\n\
             L\t0x00000011\t{r}\t0x00400000\t0x1000\t0x200\t.text\t0x00001000\t0x00000100\t0x00000200\t0x00000100\n\
             L\t0x00000011\t{r}\t0x00400000\t0x1000\t0x200\t.data\t0x00002000\t0x00000100\t0x00000300\t0x00000100\n",
            r = sha(reference)
        );
        let builds = format!(
            "build_id\tsha256\tmd5\tsize\tpe_timestamp\tlayout\tcode_version\trecipe_from\trecipe_step\toverlay_set\n\
             member\t{}\t{}\t{}\t0x00000011\tL\t1.0\t-\t-\tmember\n\
             ref\t{}\t{}\t{}\t0x00000011\tL\t1.0\t-\t-\t-\n",
            sha(member),
            md5(member),
            member.len(),
            sha(reference),
            md5(reference),
            reference.len()
        );
        Catalogue::parse(&builds, &layouts, overlays, "alias\tbuild_id\treason\n").unwrap()
    }

    fn overlays(text_sha: &str, tail_sha: &str) -> String {
        format!(
            "overlay_set\tsection\tspace\tstart\tend\tkind\truns\tdiff_bytes\tsha256\tdetail\n\
             member\t.text\tva\t0x00401010\t0x00401014\tlineage\t1\t4\t{text_sha}\t-\n\
             member\t(slack)\tfile\t0x00000400\t0x00000408\tfile-padding\t1\t8\t{tail_sha}\t-\n"
        )
    }

    #[test]
    fn identical_images_have_no_runs() {
        let r = reference();
        let p = Image::parse(&r).unwrap();
        assert!(runs(&p, &p).unwrap().is_empty());
    }

    #[test]
    fn runs_cover_changed_bytes_tail_and_unmapped_sections() {
        let (r, m) = (reference(), member());
        let got = runs(&Image::parse(&r).unwrap(), &Image::parse(&m).unwrap()).unwrap();
        assert_eq!(
            got,
            vec![
                Run { section: ".text".into(), space: Space::Va, start: 0x401010, end: 0x401014, mapped: true },
                Run { section: SLACK.into(), space: Space::File, start: 0x400, end: 0x408, mapped: true },
            ]
        );

        let short = pe(0x11, &[Sec { name: ".text", rva: 0x1000, virtual_size: 0x100, raw_offset: 0x200, raw: (0..=255u8).collect() }], b"");
        let got = runs(&Image::parse(&r).unwrap(), &Image::parse(&short).unwrap()).unwrap();
        assert!(got.iter().any(|x| x.section == ".data" && !x.mapped && x.start == 0x402000 && x.end == 0x403000), "{got:?}");
        assert!(got.iter().any(|x| x.section == HEADERS), "NumberOfSections differs: {got:?}");
    }

    #[test]
    fn a_member_whose_differences_are_all_listed_passes() {
        let (r, m) = (reference(), member());
        let text = sha(&[0xde, 0xad, 0xbe, 0xef]);
        let tail = sha(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let c = catalogue(&r, &m, &overlays(&text, &tail));
        let rows = check(&c, &m, &r).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(check(&c, &r, &r).unwrap().is_empty(), "the reference is a member of its own layout");
    }

    #[test]
    fn an_unlisted_difference_fails() {
        let (r, m) = (reference(), member());
        let tail = sha(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let text = sha(&[0xde, 0xad, 0xbe, 0xef]);
        let only_tail = overlays(&text, &tail).lines().filter(|l| !l.contains(".text")).collect::<Vec<_>>().join("\n") + "\n";
        let c = catalogue(&r, &m, &only_tail);
        let e = check(&c, &m, &r).unwrap_err().to_string();
        assert!(e.contains("no overlay lists") && e.contains(".text va 0x401010..0x401014"), "{e}");
    }

    #[test]
    fn a_wrong_hash_or_a_wrong_reference_fails() {
        let (r, m) = (reference(), member());
        let tail = sha(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let c = catalogue(&r, &m, &overlays(&"0".repeat(64), &tail));
        let e = check(&c, &m, &r).unwrap_err().to_string();
        assert!(e.contains("sha256"), "{e}");

        let text = sha(&[0xde, 0xad, 0xbe, 0xef]);
        let c = catalogue(&r, &m, &overlays(&text, &tail));
        let e = check(&c, &m, &m).unwrap_err().to_string();
        assert!(e.contains("is not the reference"), "{e}");
    }

    #[test]
    fn a_listed_overlay_the_bytes_do_not_show_fails() {
        let (r, m) = (reference(), member());
        let rows = format!(
            "overlay_set\tsection\tspace\tstart\tend\tkind\truns\tdiff_bytes\tsha256\tdetail\n\
             member\t.text\tva\t0x00401010\t0x00401014\tlineage\t1\t4\t{}\t-\n\
             member\t.data\tva\t0x00402000\t0x00402004\tlineage\t1\t4\t{}\t-\n\
             member\t(slack)\tfile\t0x00000400\t0x00000408\tfile-padding\t1\t8\t{}\t-\n",
            sha(&[0xde, 0xad, 0xbe, 0xef]),
            sha(&[7; 4]),
            sha(&[1, 2, 3, 4, 5, 6, 7, 8])
        );
        let c = catalogue(&r, &m, &rows);
        let e = check(&c, &m, &r).unwrap_err().to_string();
        assert!(e.contains("show no difference"), "{e}");
    }

    #[test]
    fn a_run_crossing_an_overlay_boundary_fails() {
        let (r, m) = (reference(), member());
        let tail = sha(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let narrow = format!(
            "overlay_set\tsection\tspace\tstart\tend\tkind\truns\tdiff_bytes\tsha256\tdetail\n\
             member\t.text\tva\t0x00401010\t0x00401012\tlineage\t1\t2\t{}\t-\n\
             member\t(slack)\tfile\t0x00000400\t0x00000408\tfile-padding\t1\t8\t{tail}\t-\n",
            sha(&[0xde, 0xad])
        );
        let c = catalogue(&r, &m, &narrow);
        let e = check(&c, &m, &r).unwrap_err().to_string();
        assert!(e.contains("crosses the boundary"), "{e}");
    }
}
