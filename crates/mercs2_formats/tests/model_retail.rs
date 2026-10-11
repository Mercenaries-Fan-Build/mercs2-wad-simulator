//! Retail gate for [`mercs2_formats::model`]: every static/skinned model container in `vz.wad`, and
//! every `GEOM`-only LOD-block container of `MESH`/`SKIN` sub-objects, decodes and encodes back to
//! its own bytes; every other model container is refused naming the chunk; and the decoded models
//! hold the census invariants the codec's fields are documented by.
//!
//! In scope: a top level whose chunks are all in {INFO, HIER, MTRL, BSHP, SEGM, PHY2, GEOM} and
//! whose `GEOM` sub-objects are all `MESH` or `SKIN`. The scope is read from the raw descriptor
//! rows, independently of the codec.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::model::{
    DeclType, DeclUsage, Geom, HierNode, LodBlock, Model, PrimRecord, SubObject, Trailer, VertexStream,
};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_MODEL;
use mercs2_formats::ucfx::{read_ucfx_rows, walk_decompressed_block, UcfxRow};

/// Every model container of the retail `vz.wad`: (block, name hash, container bytes).
fn retail_model_containers() -> Vec<(usize, u32, Vec<u8>)> {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut f = std::fs::File::open(&wad).expect("open vz.wad");
    let size = f.metadata().expect("stat vz.wad").len();
    let archive = load_ffcs_archive(&mut f, size).expect("read FFCS tables");
    let mut out = Vec::new();
    for block in 0..archive.indx.len() {
        let dec = decompress_block(&mut f, &archive.indx, block as u16).expect("decompress block");
        let (parsed, _) = walk_decompressed_block(&dec, "block");
        for (entry, c) in parsed.entries.iter().zip(parsed.containers.iter()) {
            if entry.type_hash == TYPE_HASH_MODEL {
                out.push((block, entry.name_hash, c.to_vec()));
            }
        }
    }
    out
}

const TOP_LEVEL: [&[u8; 4]; 7] = [b"INFO", b"HIER", b"MTRL", b"BSHP", b"SEGM", b"PHY2", b"GEOM"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// A full model container in scope.
    Resident,
    /// A `GEOM`-only container in scope.
    LodBlock,
    /// Outside the codec.
    Out,
}

/// The row indices of the children of row `parent` (`None` = the top level), walked `i + x3 + 1`.
fn children(rows: &[UcfxRow], parent: Option<usize>) -> Vec<usize> {
    let (mut i, end) = match parent {
        None => (0, rows.len()),
        Some(p) => (p + 1, p + 1 + rows[p].x3 as usize),
    };
    let mut out = Vec::new();
    while i < end {
        out.push(i);
        i += 1 + rows[i].x3 as usize;
    }
    out
}

/// The scope of a container, the tags that put it out of scope, and whether its top level is
/// `GEOM` alone.
fn scope(container: &[u8]) -> (Scope, Vec<String>, bool) {
    let rows = read_ucfx_rows(container).unwrap_or_else(|e| panic!("retail model container: {e}"));
    let tag = |i: usize| String::from_utf8_lossy(&rows[i].tag).into_owned();
    let top = children(&rows, None);
    let mut outside: Vec<String> =
        top.iter().filter(|&&i| !TOP_LEVEL.contains(&&rows[i].tag)).map(|&i| tag(i)).collect();
    let geom: Vec<usize> = top.iter().copied().filter(|&i| &rows[i].tag == b"GEOM").collect();
    assert_eq!(geom.len(), 1, "every retail model container has one GEOM");
    outside.extend(
        children(&rows, Some(geom[0]))
            .into_iter()
            .skip(2)
            .filter(|&i| &rows[i].tag != b"MESH" && &rows[i].tag != b"SKIN")
            .map(tag),
    );
    let geom_only = top.len() == 1;
    let sc = if !outside.is_empty() {
        Scope::Out
    } else if geom_only {
        Scope::LodBlock
    } else {
        Scope::Resident
    };
    (sc, outside, geom_only)
}

/// One `PRMG` group of either kind.
struct GroupView<'a> {
    stream: &'a VertexStream,
    strip: &'a [u16],
    /// Each record, and whether it is in list B.
    prims: Vec<(&'a PrimRecord, bool)>,
    skinned: bool,
}

fn groups(g: &Geom) -> Vec<GroupView<'_>> {
    fn view<'a>(
        stream: &'a VertexStream,
        strip: &'a [u16],
        a: &'a [PrimRecord],
        b: &'a [PrimRecord],
        skinned: bool,
    ) -> GroupView<'a> {
        let prims = a.iter().map(|p| (p, false)).chain(b.iter().map(|p| (p, true))).collect();
        GroupView { stream, strip, prims, skinned }
    }
    let mut out = Vec::new();
    for s in &g.subs {
        match s {
            SubObject::Mesh { groups } => {
                out.extend(groups.iter().map(|x| view(&x.stream, &x.strip, &x.list_a, &x.list_b, false)))
            }
            SubObject::Skin { groups } => {
                out.extend(groups.iter().map(|x| view(&x.stream, &x.strip, &x.list_a, &x.list_b, true)))
            }
        }
    }
    out
}

/// Walk a first-child/next-sibling chain starting at `first`; `None` if it revisits a node or
/// leaves the node list.
fn chain(hier: &[HierNode], first: Option<u16>) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    let mut at = first;
    while let Some(i) = at {
        let i = i as usize;
        if i >= hier.len() || out.contains(&i) {
            return None;
        }
        out.push(i);
        at = hier[i].next_sibling;
    }
    Some(out)
}

#[test]
fn every_retail_static_and_skinned_model_round_trips_and_holds_the_census_invariants() {
    let all = retail_model_containers();
    let mut failures = Vec::new();
    let mut residents: Vec<(String, u32, Model)> = Vec::new();
    let mut blocks: HashMap<u32, Vec<Geom>> = HashMap::new();
    let (mut out_of_scope, mut lod_blocks, mut zeroed) = (0usize, 0usize, Vec::new());

    for (block, hash, c) in &all {
        let at = format!("block {block} container {hash:#010X}");
        let (sc, outside, geom_only) = scope(c);
        match sc {
            Scope::Out => {
                out_of_scope += 1;
                let err = if geom_only { LodBlock::decode(c).err() } else { Model::decode(c).err() };
                match err {
                    None => failures.push(format!("{at}: out of scope ({}) and decoded", outside.join(", "))),
                    Some(e) if !outside.iter().any(|t| e.contains(t.as_str())) => {
                        failures.push(format!("{at}: refused without naming {}: {e}", outside.join(", ")))
                    }
                    Some(_) => {}
                }
            }
            Scope::Resident => match Model::decode(c) {
                Err(e) => failures.push(format!("{at}: {e}")),
                Ok(m) => match m.encode() {
                    Ok(b) if &b == c => residents.push((at, *hash, m)),
                    Ok(_) => failures.push(format!("{at}: encodes to different bytes")),
                    Err(e) => failures.push(format!("{at}: encode: {e}")),
                },
            },
            Scope::LodBlock => {
                lod_blocks += 1;
                match LodBlock::decode(c) {
                    Err(e) => failures.push(format!("{at}: {e}")),
                    Ok(l) => match l.encode() {
                        Ok(b) if &b == c => {
                            if l.trailer == Trailer::Zeroed {
                                zeroed.push(at);
                            }
                            blocks.entry(*hash).or_default().push(l.geom);
                        }
                        Ok(_) => failures.push(format!("{at}: encodes to different bytes")),
                        Err(e) => failures.push(format!("{at}: encode: {e}")),
                    },
                }
            }
        }
    }

    let mut skinned_vertices = 0usize;
    let mut prim_records = 0usize;
    // List-B records whose min/max/unique words declare the whole vertex range (0, vcount − 1,
    // vcount) while their strip spans less. The draw passes min/max as MinIndex / NumVertices, and
    // that range contains the strip.
    let mut full_range_b = Vec::new();
    for (at, hash, m) in &residents {
        let mut bad = |what: String| failures.push(format!("{at}: {what}"));
        let rungs: Vec<&Geom> = std::iter::once(&m.geom).chain(blocks.get(hash).into_iter().flatten()).collect();

        let max_slot = rungs.iter().flat_map(|g| g.indx.iter()).copied().max();
        if max_slot.map(|s| s as u32 + 1) != Some(m.info.slot_count) {
            bad(format!("INFO +0x2C is {} and the max INDX over all blocks is {max_slot:?}", m.info.slot_count));
        }
        let masks = m.segm.iter().fold(0u8, |acc, r| acc | r.lod_mask);
        if m.info.lod_count != 8 - masks.leading_zeros() {
            bad(format!("INFO +0x34 is {} and the SEGM masks OR to 0x{masks:02X}", m.info.lod_count));
        }
        if m.info.fade_sharpness != 5.0 {
            bad(format!("INFO +0x3C is {}", m.info.fade_sharpness));
        }

        // Each node's child chain, and the root chain from the first root, enumerate exactly the
        // nodes the parent field gives, once each.
        let first_root = m.hier.iter().position(|n| n.parent.is_none()).map(|r| r as u16);
        for owner in std::iter::once(None).chain((0..m.hier.len()).map(Some)) {
            let (first, want): (Option<u16>, BTreeSet<usize>) = match owner {
                None => (first_root, (0..m.hier.len()).filter(|&c| m.hier[c].parent.is_none()).collect()),
                Some(i) => (
                    m.hier[i].first_child,
                    (0..m.hier.len()).filter(|&c| m.hier[c].parent == Some(i as u16)).collect(),
                ),
            };
            match chain(&m.hier, first) {
                Some(got) if got.len() == want.len() && got.iter().copied().collect::<BTreeSet<_>>() == want => {}
                got => bad(format!("HIER {owner:?} chain {got:?}; the parents give {want:?}")),
            }
        }

        for g in &rungs {
            for view in groups(g) {
                let stream = view.stream;
                if view.skinned {
                    let Some(w) = stream.element(DeclUsage::BlendWeight, 0) else {
                        bad("a SKIN group without BLENDWEIGHT0".into());
                        continue;
                    };
                    if w.ty != DeclType::UByte4N {
                        bad(format!("BLENDWEIGHT0 is {:?}", w.ty));
                        continue;
                    }
                    let (o, stride) = (w.offset as usize, stream.stride());
                    for v in 0..stream.vertex_count() {
                        skinned_vertices += 1;
                        let sum: u32 = stream.data[v * stride + o..v * stride + o + 4].iter().map(|&b| b as u32).sum();
                        if sum != 255 {
                            bad(format!("skinned vertex {v} weights sum to {sum}"));
                            break;
                        }
                    }
                }
                for (p, in_b) in &view.prims {
                    prim_records += 1;
                    let (s, e) = (p.start as usize, p.start as usize + p.prims as usize + 2);
                    let Some(run) = view.strip.get(s..e) else {
                        bad(format!("PRMT {p:?} runs past the {}-index strip", view.strip.len()));
                        continue;
                    };
                    let (lo, hi) = (*run.iter().min().unwrap(), *run.iter().max().unwrap());
                    let vcount = stream.vertex_count();
                    let declares_full_range = (p.min_index as usize, p.max_index as usize, p.unique_count as usize)
                        == (0, vcount.wrapping_sub(1), vcount);
                    if (p.min_index, p.max_index) != (lo, hi) && *in_b && declares_full_range && hi <= p.max_index {
                        full_range_b.push(format!("{at} {p:?}"));
                    } else if (p.min_index, p.max_index) != (lo, hi) {
                        bad(format!(
                            "PRMT {p:?} (list {}) declares {}..={} over {} vertices; its strip spans {lo}..={hi}",
                            if *in_b { "B" } else { "A" },
                            p.min_index,
                            p.max_index,
                            stream.vertex_count()
                        ));
                    }
                }
            }
        }
    }

    eprintln!(
        "model containers {}: resident in scope {}, LOD blocks in scope {lod_blocks} (zero trailer: {zeroed:?}), \
         out of scope {out_of_scope}",
        all.len(),
        residents.len(),
    );
    eprintln!("skinned vertices {skinned_vertices}; PRMT records {prim_records}; full-range list-B records {full_range_b:?}");
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    assert_eq!(residents.len(), 455);
    assert_eq!(full_range_b.len(), 1, "full-range list-B records: {full_range_b:?}");
}
