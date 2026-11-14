//! `dlc-census` — every chunk of a big-endian DLC archive against the retail build of the same platform.
//!
//! Both sides are console bytes from one toolchain, so nothing is converted: a chunk is `new` when the
//! base has no chunk of its (name, type), `same` when one of the base's chunks of that key holds the same
//! bytes, and `changed` when the base has the key under other bytes. A `changed` row names the UCFX rows
//! that differ from the closest base chunk of its key, the one differing in the fewest. The rows say what
//! a DLC adds, what it repeats and what it overwrites, which is what a build from components has to carry.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use mercs2_formats::anim::AnimClip;
use mercs2_formats::be_to_le::texture::container_rows;
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;
use mercs2_formats::types::{
    type_name_from_hash, TYPE_HASH_ANIMATION, TYPE_HASH_LOWRES_TERRAIN, TYPE_HASH_MODEL, TYPE_HASH_TERRAIN_MESH,
    TYPE_HASH_TEXTURE as TYPE_TEXTURE,
};
use mercs2_formats::ucfx::{parse_ucfx_tree, UcfxNode};

struct Chunk {
    block: String,
    name: u32,
    ty: u32,
    body: Vec<u8>,
}

fn be32(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| format!("short read at +0x{o:X}"))
}

/// Every chunk of a console archive that `keep` accepts, each with the path of its block. A block is a
/// big-endian `[count]`, 16-byte rows of name, type, a third word and size, then the bodies back to back.
fn archive_chunks(path: &Path, keep: impl Fn(u32, u32) -> bool) -> Result<Vec<Chunk>, String> {
    let (doh, _) = load_stfs_or_doh(path)?;
    let (_, rows) = parse_be_ffcs(&doh)?;
    let indx = rows.iter().find(|r| r.tag == "INDX").ok_or("no INDX")?;
    let entries = parse_be_indx(&doh, indx.offset as usize, indx.meta as usize);
    let paths = rows.iter().find(|r| r.tag == "PTHS").map_or(Vec::new(), |r| parse_be_pths(&doh, r.offset as usize, r.meta as usize));
    let mut out = Vec::new();
    for (bi, e) in entries.iter().enumerate() {
        let b = read_be_block(&doh, e).map_err(|err| format!("{}: block {bi}: {err}", path.display()))?;
        let at = paths.get(bi).cloned().unwrap_or_default();
        let count = be32(&b, 0)? as usize;
        let mut off = 4 + count * 16;
        for i in 0..count {
            let row = 4 + i * 16;
            let (name, ty, size) = (be32(&b, row)?, be32(&b, row + 4)?, be32(&b, row + 12)? as usize);
            let body = b.get(off..off + size).ok_or_else(|| format!("{at}: chunk 0x{name:08X} runs past the block"))?;
            if keep(name, ty) {
                out.push(Chunk { block: at.clone(), name, ty, body: body.to_vec() });
            }
            off += size;
        }
    }
    Ok(out)
}

/// The UCFX rows of a container by tag, a tag repeating under its index. A chunk that is no container
/// is one row of its whole bytes.
fn rows_of(body: &[u8]) -> BTreeMap<String, &[u8]> {
    let mut out = BTreeMap::new();
    let parsed = if body.len() > 20 { container_rows(body).ok() } else { None };
    let fits = |rows: &[([u8; 4], usize, usize)]| rows.iter().all(|(_, o, s)| o + s <= body.len());
    match parsed.filter(|r| fits(r)) {
        Some(rows) => {
            for (i, (tag, off, size)) in rows.iter().enumerate() {
                out.insert(format!("{}#{i}", String::from_utf8_lossy(tag)), &body[*off..off + size]);
            }
        }
        None => {
            out.insert("whole".to_string(), body);
        }
    }
    out
}

/// The tags that differ between two chunks, by tag name, a row only one side has counted as differing.
fn differing(a: &[u8], b: &[u8]) -> Vec<String> {
    let (x, y) = (rows_of(a), rows_of(b));
    let mut out: Vec<String> = x
        .iter()
        .filter(|(k, v)| y.get(*k) != Some(v))
        .map(|(k, _)| k.split('#').next().unwrap_or(k).to_string())
        .chain(y.keys().filter(|k| !x.contains_key(*k)).map(|k| k.split('#').next().unwrap_or(k).to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// How two chunks of one size differ, as tab-separated differing bytes, the mean absolute difference
/// over those bytes, and the first differing offset. Chunks of two sizes have no such row.
fn byte_delta(a: &[u8], b: &[u8]) -> String {
    if a.len() != b.len() {
        return "\t\t".to_string();
    }
    let (mut n, mut sum, mut first) = (0usize, 0u64, None);
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y {
            n += 1;
            sum += u64::from(x.abs_diff(*y));
            first.get_or_insert(i);
        }
    }
    format!("{n}\t{:.1}\t{}", sum as f64 / n.max(1) as f64, first.unwrap_or(0))
}

/// The differing bytes of two chunks of one size, as `offset:ours:base` hex triples, at most `max`.
fn byte_list(a: &[u8], b: &[u8], max: usize) -> String {
    let at = a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).take(max);
    at.map(|(i, (x, y))| format!("{i:X}:{x:02X}:{y:02X}")).collect::<Vec<_>>().join(" ")
}

/// The Havok clips of a PC archive whose names `keep` accepts, by name, each decoded copy.
fn pc_clips(wad: &Path, keep: impl Fn(u32) -> bool) -> Result<HashMap<u32, Vec<AnimClip>>, String> {
    let mut f = std::fs::File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let mut out: HashMap<u32, Vec<AnimClip>> = HashMap::new();
    for b in 0..pc.indx.len() {
        let dec = mercs2_formats::sges::decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("block {b}: {e}"))?;
        for (e, range) in mercs2_formats::ucfx::block_chunk_spans(&dec).map_err(|e| format!("block {b}: {e}"))? {
            if e.type_hash != TYPE_HASH_ANIMATION || !keep(e.name_hash) {
                continue;
            }
            if let Some(data) = mercs2_formats::ucfx::extract_chunk_body(&dec[range], b"data") {
                out.entry(e.name_hash).or_default().push(mercs2_formats::anim::parse_anim(&data).map_err(|err| format!("0x{:08X}: {err}", e.name_hash))?);
            }
        }
    }
    Ok(out)
}

/// The largest difference between two clips over every track of every key frame, per component:
/// rotation, translation and scale. A quaternion and its negation are one rotation, so the rotation
/// takes the nearer sign. A decoded quaternion need not be unit, so an angle from a dot product would
/// report a difference between a clip and itself. `None` when the two differ in shape.
fn pose_delta(a: &AnimClip, b: &AnimClip) -> Option<[f32; 3]> {
    if (a.num_tracks, a.num_frames, a.decoded) != (b.num_tracks, b.num_frames, b.decoded) {
        return None;
    }
    let mut worst = [0f32; 3];
    let last = a.num_frames.max(2) - 1;
    for f in 0..=last {
        let t = a.duration * f as f32 / last as f32;
        for (x, y) in a.sample_local(t).iter().zip(b.sample_local(t)) {
            let plus = x.rotation.iter().zip(y.rotation).map(|(p, q)| (p - q).abs()).fold(0.0, f32::max);
            let minus = x.rotation.iter().zip(y.rotation).map(|(p, q)| (p + q).abs()).fold(0.0, f32::max);
            worst[0] = worst[0].max(plus.min(minus));
            worst[1] = worst[1].max(x.translation.iter().zip(y.translation).map(|(p, q)| (p - q).abs()).fold(0.0, f32::max));
            worst[2] = worst[2].max(x.scale.iter().zip(y.scale).map(|(p, q)| (p - q).abs()).fold(0.0, f32::max));
        }
    }
    Some(worst)
}

/// Every clip of a converted DLC archive against the PC base's, decoded, one row per clip with its
/// largest pose difference. A clip the base lacks is `new`.
pub fn clip_compare(converted: &Path, pc: &Path, out: &Path) -> Result<(), String> {
    let ours = pc_clips(converted, |_| true)?;
    let base = pc_clips(pc, |n| ours.contains_key(&n))?;
    let mut tsv = String::from("name\tverdict\trotation\ttranslation\tscale\n");
    let (mut equal, mut differ, mut shape, mut new) = (0usize, 0usize, 0usize, 0usize);
    for (name, clips) in &ours {
        let Some(twins) = base.get(name) else {
            new += clips.len();
            writeln!(tsv, "0x{name:08X}\tnew\t\t\t").map_err(|e| e.to_string())?;
            continue;
        };
        for c in clips {
            let best = twins.iter().filter_map(|t| pose_delta(c, t)).min_by(|x, y| x[0].total_cmp(&y[0]).then(x[1].total_cmp(&y[1])));
            let Some(d) = best else {
                shape += 1;
                writeln!(tsv, "0x{name:08X}\tshape\t\t\t").map_err(|e| e.to_string())?;
                continue;
            };
            let verdict = if d == [0.0; 3] { equal += 1; "equal" } else { differ += 1; "differ" };
            writeln!(tsv, "0x{name:08X}\t{verdict}\t{}\t{}\t{}", d[0], d[1], d[2]).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{equal} equal, {differ} differ, {shape} of another shape, {new} new");
    Ok(())
}

/// Write every UCFX row of one big-endian chunk to `dir`, one file per row, and print the rows.
pub fn chunk_rows(archive: &Path, name: u32, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (ci, c) in archive_chunks(archive, |n, _| n == name)?.iter().enumerate() {
        println!("chunk {ci}: {} {} bytes in {}", type_name_from_hash(c.ty), c.body.len(), c.block);
        for (i, (tag, off, size)) in container_rows(&c.body)?.iter().enumerate() {
            let t = String::from_utf8_lossy(tag).to_string();
            let body = c.body.get(*off..off + size).ok_or("a row runs past its chunk")?;
            println!("  {i:4} {t} {size} @{off}");
            std::fs::write(dir.join(format!("{ci}_{i:04}_{t}.bin")), body).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// For each type in a PC WAD, how many assets spread over several chunks, and of those how many hold, in
/// each chunk's third table word, the byte sum of the chunks of the rungs before it.
pub fn rung_words(wad: &Path) -> Result<(), String> {
    let mut f = std::fs::File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let mut chains: HashMap<(u32, u32), Vec<(u32, u32)>> = HashMap::new();
    for b in 0..pc.indx.len() {
        let dec = mercs2_formats::sges::decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("block {b}: {e}"))?;
        for (e, range) in mercs2_formats::ucfx::block_chunk_spans(&dec).map_err(|e| format!("block {b}: {e}"))? {
            chains.entry((e.name_hash, e.type_hash)).or_default().push((e.field_c, range.len() as u32));
        }
    }
    let mut tally: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for ((_, ty), mut rows) in chains {
        let t = tally.entry(type_name_from_hash(ty).to_string()).or_default();
        if rows.len() < 2 {
            t.2 += usize::from(rows[0].0 != 0);
            continue;
        }
        rows.sort_unstable();
        t.0 += 1;
        let mut sum = 0u32;
        let cumulative = rows.iter().all(|&(word, size)| {
            let here = word == sum;
            sum += size;
            here
        });
        t.1 += usize::from(cumulative);
    }
    for (ty, (chains, cumulative, lone_nonzero)) in tally {
        if chains > 0 || lone_nonzero > 0 {
            println!("{ty}\t{chains} chains\t{cumulative} cumulative\t{lone_nonzero} lone chunks with a word");
        }
    }
    Ok(())
}

/// One mesh's drawn triangles by the bits of each corner's position and UV, wound, with the copy number.
fn scrub_triangles(m: &mercs2_formats::scrub::ScrubMesh) -> HashMap<([VertexKey; 3], u16), i64> {
    let half = |o: usize| u16::from_le_bytes([m.vertices[o], m.vertices[o + 1]]);
    let stride = m.stride as usize;
    let key = |i: u16| -> VertexKey {
        let o = i as usize * stride;
        ([half(o), half(o + 2), half(o + 4)], [half(o + 8), half(o + 10)])
    };
    let mut out: HashMap<([VertexKey; 3], u16), i64> = HashMap::new();
    for (i, t) in m.indices.windows(3).enumerate() {
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            continue;
        }
        let mut tri = if i % 2 == 0 { [key(t[0]), key(t[1]), key(t[2])] } else { [key(t[0]), key(t[2]), key(t[1])] };
        let low = (0..3).min_by_key(|&k| tri[k]).unwrap_or(0);
        tri.rotate_left(low);
        *out.entry((tri, half(t[0] as usize * stride + 12))).or_default() += 1;
    }
    out
}

/// Convert every scrub of a big-endian archive and compare it with its PC twin: every row the conversion
/// keeps exactly, each mesh's triangles by copy, and each instance's translation. Rotations are PS3's, so
/// only their spread from PC is reported. `out` lists each scrub that differs.
pub fn scrub_compare(archive: &Path, pc: &Path, out: &Path) -> Result<(), String> {
    use mercs2_formats::be_to_le::convert::convert_block;
    use mercs2_formats::scrub::{ScrubPack, TYPE_HASH};
    let twins = pc_chunks(pc, |_, t| t == TYPE_HASH)?;
    let mut tsv = String::from("name\tproblem\n");
    let (mut clean, mut total, mut instances, mut worst) = (0usize, 0usize, 0usize, 0f32);
    for c in archive_chunks(archive, |_, t| t == TYPE_HASH)? {
        total += 1;
        let mut one = Vec::with_capacity(20 + c.body.len());
        one.extend_from_slice(&1u32.to_be_bytes());
        one.extend_from_slice(&c.name.to_be_bytes());
        one.extend_from_slice(&c.ty.to_be_bytes());
        one.extend_from_slice(&0u32.to_be_bytes());
        one.extend_from_slice(&(c.body.len() as u32).to_be_bytes());
        one.extend_from_slice(&c.body);
        let le = convert_block(&one, false, None, None, None).map_err(|e| format!("0x{:08X}: {e}", c.name))?;
        let (_, range) = mercs2_formats::ucfx::block_chunk_spans(&le)?.into_iter().next().ok_or("no chunk")?;
        let ours = ScrubPack::decode(&le[range])?;
        let twin = twins.get(&(c.name, TYPE_HASH)).and_then(|v| v.first()).ok_or_else(|| format!("0x{:08X} has no PC twin", c.name))?;
        let (ours, theirs) = match (ours, ScrubPack::decode(twin)?) {
            (ScrubPack::Pack(a), ScrubPack::Pack(b)) => (a, b),
            (ScrubPack::Empty, ScrubPack::Empty) => {
                clean += 1;
                continue;
            }
            _ => return Err(format!("0x{:08X} is empty on one side", c.name)),
        };
        let mut problems: Vec<String> = Vec::new();
        if ours.patches != theirs.patches {
            problems.push("patches".into());
        }
        if ours.ranges != theirs.ranges {
            problems.push("ranges".into());
        }
        if ours.meshes.len() != theirs.meshes.len() || ours.instances.len() != theirs.instances.len() {
            problems.push("counts".into());
        }
        for (i, (a, b)) in ours.meshes.iter().zip(&theirs.meshes).enumerate() {
            if a.info != b.info || a.material != b.material || a.decl != b.decl || a.stride != b.stride {
                problems.push(format!("mesh {i} rows"));
            }
            if scrub_triangles(a) != scrub_triangles(b) {
                problems.push(format!("mesh {i} triangles"));
            }
        }
        for (i, (a, b)) in ours.instances.iter().zip(&theirs.instances).enumerate() {
            instances += 1;
            if (0..3).any(|r| a.rows[r][3] != b.rows[r][3]) {
                problems.push(format!("instance {i} translation"));
                break;
            }
            let (x, y) = (a.matrix(), b.matrix());
            let len = |m: &[[f32; 4]; 3], c: usize| (0..3).map(|r| m[r][c] * m[r][c]).sum::<f32>().sqrt();
            let (la, lb) = (len(&x, 1), len(&y, 1));
            if la > 0.0 && lb > 0.0 {
                let cos: f32 = (0..3).map(|r| x[r][1] * y[r][1]).sum::<f32>() / (la * lb);
                worst = worst.max(cos.clamp(-1.0, 1.0).acos().to_degrees());
            }
        }
        if problems.is_empty() {
            clean += 1;
        } else {
            let _ = writeln!(tsv, "0x{:08X}\t{}", c.name, problems.join(", "));
        }
    }
    std::fs::write(out, tsv).map_err(|e| e.to_string())?;
    println!("{clean} of {total} scrubs equal but for rotation, {instances} instances, the largest up-axis angle from PC {worst:.2}°");
    Ok(())
}

/// A vertex as the bits of its position and UV.
type VertexKey = ([u16; 3], [u16; 2]);

/// What a model draws, over all its rungs, which each platform splits its own way.
#[derive(Default)]
struct ModelView {
    /// Triangles of list A by material, wound as drawn, with their counts.
    triangles: HashMap<(u32, [VertexKey; 3]), i64>,
    /// Triangles of list B, whichever material each record names.
    shadow: HashSet<[VertexKey; 3]>,
    /// Each skinned vertex with its `(weight, HIER bone)` list, slot order, no empty slots.
    skin: HashSet<(VertexKey, Vec<(u8, u16)>)>,
}

/// One PC group of a model into `view`.
fn view_group(group: &[UcfxNode], view: &mut ModelView) -> Result<(), String> {
    let leaf = |t: &[u8; 4]| group.iter().find(|n| &n.tag == t);
    let info = leaf(b"INFO").and_then(|n| n.body.as_ref()).ok_or("a group has no INFO")?;
    let word = |b: &[u8], o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
    let half = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let strm = leaf(b"STRM").ok_or("a group has no STRM")?;
    let row = |t: &str| strm.children.iter().find(|c| c.tag_str() == t).and_then(|c| c.body.as_ref());
    let (stream_info, decl, data) = (row("info").ok_or("no info")?, row("decl").ok_or("no decl")?, row("data").ok_or("no data")?);
    let stride = word(stream_info, 4);
    let element = |usage: u8| decl.chunks_exact(8).find(|e| e[0] == 0 && e[6] == usage && e[7] == 0).map(|e| half(e, 2) as usize);
    let key = |i: u16| -> VertexKey {
        let o = i as usize * stride;
        let uv = element(5).map_or([0, 0], |at| [half(data, o + at), half(data, o + at + 2)]);
        ([half(data, o), half(data, o + 2), half(data, o + 4)], uv)
    };
    let ibuf = leaf(b"IBUF").ok_or("a group has no IBUF")?;
    let strip = ibuf.children.iter().find(|c| c.tag_str() == "data").and_then(|c| c.body.as_ref()).ok_or("no IBUF data")?;
    let idx: Vec<u16> = strip.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    let prmt = leaf(b"PRMT").and_then(|n| n.body.as_ref()).ok_or("a group has no PRMT")?;
    let (list_a, list_b) = (word(info, 0), word(info, 4));
    for (r, rec) in prmt.chunks_exact(16).enumerate() {
        let (material, start, prims) = (word(rec, 0) as u32, word(rec, 4), half(rec, 8) as usize);
        let run = idx.get(start..start + prims + 2).ok_or_else(|| format!("record {r} of {} past a strip of {}", list_a + list_b, idx.len()))?;
        for (i, t) in run.windows(3).enumerate() {
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                continue;
            }
            let (a, b, c) = (key(t[0]), key(t[1]), key(t[2]));
            if a == b || b == c || a == c {
                continue;
            }
            let mut tri = if i % 2 == 0 { [a, b, c] } else { [a, c, b] };
            let low = (0..3).min_by_key(|&k| tri[k]).unwrap_or(0);
            tri.rotate_left(low);
            if r < list_a {
                *view.triangles.entry((material, tri)).or_default() += 1;
            } else {
                view.shadow.insert(tri);
            }
        }
    }
    if let (Some(slots), Some(weights)) = (element(2), element(1)) {
        let mut palette = Vec::new();
        for k in 0..word(info, 0x14) {
            let base = half(info, 0x18 + 4 * k);
            palette.extend((0..half(info, 0x1A + 4 * k)).map(|b| base.wrapping_add(b)));
        }
        for v in 0..data.len() / stride {
            let o = v * stride;
            let list: Vec<(u8, u16)> = (0..4)
                .filter(|&k| data[o + weights + k] != 0)
                .map(|k| (data[o + weights + k], palette.get(data[o + slots + k] as usize).copied().unwrap_or(0xFFFF)))
                .collect();
            view.skin.insert((key(v as u16), list));
        }
    }
    Ok(())
}

fn view_nodes(nodes: &[UcfxNode], view: &mut ModelView) -> Result<(), String> {
    for n in nodes {
        if &n.tag == b"PRMG" {
            view_group(&n.children, view)?;
        }
        view_nodes(&n.children, view)?;
    }
    Ok(())
}

/// Convert every model of a big-endian archive and compare it with its PC twin, its whole rung chain at
/// once: triangles by material, position, UV and winding, list B's triangles, and each vertex's weights
/// and bones. `out` lists each model that differs.
pub fn model_compare(archive: &Path, pc: &Path, out: &Path) -> Result<(), String> {
    use mercs2_formats::be_to_le::convert::convert_block;
    use mercs2_formats::be_to_le::texture::{BlockTables, TextureTable};
    let chunks = archive_chunks(archive, |_, t| t == TYPE_HASH_MODEL)?;
    let textures = TextureTable::of_models(chunks.iter().map(|c| (c.name, &c.body[..])))?;
    let tables = BlockTables { textures: &textures, rung: None, streams: None };
    let mut ours: HashMap<u32, ModelView> = HashMap::new();
    let mut refused: BTreeMap<u32, String> = BTreeMap::new();
    for c in chunks {
        let mut one = Vec::with_capacity(20 + c.body.len());
        one.extend_from_slice(&1u32.to_be_bytes());
        one.extend_from_slice(&c.name.to_be_bytes());
        one.extend_from_slice(&c.ty.to_be_bytes());
        one.extend_from_slice(&0u32.to_be_bytes());
        one.extend_from_slice(&(c.body.len() as u32).to_be_bytes());
        one.extend_from_slice(&c.body);
        let view = ours.entry(c.name).or_default();
        let done = convert_block(&one, false, None, None, Some(tables)).and_then(|le| {
            let (_, range) = mercs2_formats::ucfx::block_chunk_spans(&le)?.into_iter().next().ok_or("no chunk")?;
            view_nodes(&parse_ucfx_tree(&le[range])?, view)
        });
        if let Err(e) = done {
            refused.insert(c.name, e);
        }
    }
    let twins = pc_chunks(pc, |_, t| t == TYPE_HASH_MODEL)?;
    let mut tsv = String::from("name\ttriangles\tour_triangles\tonly_pc\tonly_ours\tshadow_only_pc\tshadow_only_ours\tskin_only_pc\tskin_only_ours\n");
    let (mut clean, mut total, mut tris) = (0usize, 0usize, 0i64);
    let mut names: Vec<u32> = ours.keys().copied().collect();
    names.sort_unstable();
    for name in names {
        if let Some(e) = refused.get(&name) {
            let _ = writeln!(tsv, "0x{name:08X}\trefused\t{e}");
            total += 1;
            continue;
        }
        let mut twin = ModelView::default();
        for chunk in twins.get(&(name, TYPE_HASH_MODEL)).map_or(&[][..], Vec::as_slice) {
            // A few retail lower rungs end in zeros, not a `CSUM` trailer (`model::Trailer::Zeroed`).
            let mut sealed = chunk.clone();
            if chunk[chunk.len() - 8..].iter().all(|&b| b == 0) {
                sealed.truncate(chunk.len() - 8);
                let sum = mercs2_formats::crc32::crc32_mercs2(&sealed);
                sealed.extend_from_slice(b"CSUM");
                sealed.extend_from_slice(&sum.to_le_bytes());
            }
            view_nodes(&parse_ucfx_tree(&sealed)?, &mut twin)?;
        }
        let view = &ours[&name];
        let only = |a: &HashMap<(u32, [VertexKey; 3]), i64>, b: &HashMap<(u32, [VertexKey; 3]), i64>| {
            a.iter().map(|(k, n)| (n - b.get(k).copied().unwrap_or(0)).max(0)).sum::<i64>()
        };
        let (a, b) = (only(&twin.triangles, &view.triangles), only(&view.triangles, &twin.triangles));
        let (sa, sb) = (twin.shadow.difference(&view.shadow).count(), view.shadow.difference(&twin.shadow).count());
        let (ka, kb) = (twin.skin.difference(&view.skin).count(), view.skin.difference(&twin.skin).count());
        total += 1;
        tris += twin.triangles.values().sum::<i64>();
        if a == 0 && b == 0 && sa == 0 && sb == 0 && ka == 0 && kb == 0 {
            clean += 1;
        } else {
            let (x, y): (i64, i64) = (twin.triangles.values().sum(), view.triangles.values().sum());
            let _ = writeln!(tsv, "0x{name:08X}\t{x}\t{y}\t{a}\t{b}\t{sa}\t{sb}\t{ka}\t{kb}");
        }
    }
    std::fs::write(out, tsv).map_err(|e| e.to_string())?;
    println!("{clean} of {total} models equal, {} refused, {tris} PC triangles", refused.len());
    Ok(())
}

/// Convert the big-endian `PHY2` body at `be` and print how it differs from the little-endian body in
/// `pc`, a body or a container holding one: byte runs by region of the body.
pub fn phy2_diff(be: &Path, pc: &Path) -> Result<(), String> {
    use mercs2_formats::be_to_le::havok::convert_phy2_be_to_le;
    let read = |p: &Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let got = convert_phy2_be_to_le(&read(be)?)?;
    let want = read(pc)?;
    let want = match mercs2_formats::phy2_container::phy2_span_in_container(&want) {
        Some((at, size)) => want[at..at + size].to_vec(),
        None => want,
    };
    println!("converted {} bytes, PC {}", got.len(), want.len());
    let u32le = |b: &[u8], o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
    let wrapper = 48 + u32le(&want, 32);
    let sections = 48 + 64 + 2 * 48 + 20;
    let data = 48 + u32le(&want, sections);
    let fixups = data + u32le(&want, sections + 4);
    for (name, a, b) in [("prefix", 0, 48), ("header", 48, data), ("objects", data, fixups), ("fixups", fixups, wrapper), ("wrapper", wrapper, want.len())] {
        let end = b.min(got.len());
        let diff: Vec<usize> = (a..end).filter(|&i| got[i] != want[i]).collect();
        let first: Vec<String> = diff.iter().take(8).map(|&i| format!("+{}:{:02x}/{:02x}", i - a, got[i], want[i])).collect();
        println!("{name} {a}..{b}: {} bytes differ {}", diff.len(), first.join(" "));
    }
    Ok(())
}

/// The triangles a terrain chunk's draws cover, each as its three positions of raw half-float words,
/// wound as drawn and turned to start at the lowest, so a triangle wound the other way is another
/// triangle. A draw is a strip within the group's `IBUF`.
fn triangles(nodes: &[UcfxNode], out: &mut Vec<[[u16; 3]; 3]>) -> Result<(), String> {
    // A `lowresterrain` holds its one group's rows at the top level, not under a `PRMG`.
    if nodes.iter().any(|n| n.tag_str() == "PRMT") {
        group_triangles(nodes, out)?;
    }
    for n in nodes {
        triangles(&n.children, out)?;
    }
    Ok(())
}

/// The triangles of one group, from its `STRM`, `IBUF` and `PRMT`.
fn group_triangles(group: &[UcfxNode], out: &mut Vec<[[u16; 3]; 3]>) -> Result<(), String> {
    let (mut verts, mut stride, mut strip, mut draws) = (None, 0usize, None, None);
    for c in group {
        let row = |t: &str| c.children.iter().find(|x| x.tag_str() == t).and_then(|x| x.body.as_ref());
        match c.tag_str().as_str() {
            "STRM" => {
                if let (Some(info), Some(data)) = (row("info"), row("data")) {
                    stride = u32::from_le_bytes([info[4], info[5], info[6], info[7]]) as usize;
                    verts = Some(data);
                }
            }
            "IBUF" => strip = row("data"),
            "PRMT" => draws = c.body.as_ref(),
            _ => {}
        }
    }
    let (Some(v), Some(s), Some(d)) = (verts, strip, draws) else { return Ok(()) };
    let pos = |i: u16| -> Option<[u16; 3]> {
        let o = i as usize * stride;
        let w = |k: usize| v.get(o + k..o + k + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        Some([w(0)?, w(2)?, w(4)?])
    };
    let idx: Vec<u16> = s.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    // A terrain draw is 20 bytes: start at +8, triangles at +12 (`docs/terrain_cell_format.md`).
    for rec in d.chunks_exact(20) {
        let start = u32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]) as usize;
        let count = u16::from_le_bytes([rec[12], rec[13]]) as usize;
        let run = idx.get(start..start + count + 2).ok_or_else(|| format!("a draw of {count} triangles at {start} runs past a strip of {}", idx.len()))?;
        for (i, t) in run.windows(3).enumerate() {
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                continue;
            }
            let (Some(a), Some(b), Some(c)) = (pos(t[0]), pos(t[1]), pos(t[2])) else {
                return Err(format!("index past {} vertices", v.len() / stride.max(1)));
            };
            if a == b || b == c || a == c {
                continue;
            }
            let mut tri = if i % 2 == 0 { [a, b, c] } else { [a, c, b] };
            let low = (0..3).min_by_key(|&k| tri[k]).unwrap_or(0);
            tri.rotate_left(low);
            out.push(tri);
        }
    }
    Ok(())
}

/// The summed area of a set of triangles of half-float positions.
fn area(tris: &HashSet<&[[u16; 3]; 3]>) -> f64 {
    let f = |h: u16| -> f64 {
        let (s, e, m) = ((h >> 15) as i32, ((h >> 10) & 31) as i32, (h & 1023) as f64);
        let v = if e == 0 { m * 2f64.powi(-24) } else { (1.0 + m / 1024.0) * 2f64.powi(e - 15) };
        if s == 1 { -v } else { v }
    };
    tris.iter()
        .map(|t| {
            let p = |i: usize| [f(t[i][0]), f(t[i][1]), f(t[i][2])];
            let (a, b, c) = (p(0), p(1), p(2));
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        })
        .sum()
}

/// Every terrain chunk two WADs share, compared as sets of triangles and of points, and by area. A
/// strip's vertex order is each bake's own, so the bytes differ where the surface does not.
pub fn terrain_compare(a: &Path, b: &Path, out: &Path) -> Result<(), String> {
    let wanted = |t: u32| t == TYPE_HASH_TERRAIN_MESH || t == TYPE_HASH_LOWRES_TERRAIN;
    let (left, right) = (pc_chunks(a, |_, t| wanted(t))?, pc_chunks(b, |_, t| wanted(t))?);
    let mut tsv = String::from("type\tname\ta_triangles\tb_triangles\tonly_a\tonly_b\ta_points\tb_points\tpoints_only_a\tpoints_only_b\ta_area\tb_area\n");
    let (mut same, mut differ) = (0usize, 0usize);
    for (key, bodies) in &left {
        let Some(twins) = right.get(key) else { continue };
        for (x, y) in bodies.iter().zip(twins) {
            let (mut p, mut q) = (Vec::new(), Vec::new());
            triangles(&parse_ucfx_tree(x)?, &mut p).map_err(|e| format!("first WAD, 0x{:08X}: {e}", key.0))?;
            triangles(&parse_ucfx_tree(y)?, &mut q).map_err(|e| format!("second WAD, 0x{:08X}: {e}", key.0))?;
            let (s, t): (HashSet<_>, HashSet<_>) = (p.iter().collect(), q.iter().collect());
            let (only_a, only_b) = (s.difference(&t).count(), t.difference(&s).count());
            if only_a + only_b == 0 { same += 1 } else { differ += 1 }
            let points = |l: &[[[u16; 3]; 3]]| -> HashSet<[u16; 3]> { l.iter().flatten().copied().collect() };
            let (u, v) = (points(&p), points(&q));
            writeln!(
                tsv,
                "{}\t0x{:08X}\t{}\t{}\t{only_a}\t{only_b}\t{}\t{}\t{}\t{}\t{}\t{}",
                type_name_from_hash(key.1), key.0, s.len(), t.len(), u.len(), v.len(), u.difference(&v).count(), v.difference(&u).count(), area(&s), area(&t),
            ).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{same} surfaces equal, {differ} differ");
    Ok(())
}

/// `terrain-draw-ranges`: every terrain draw record (`PRMT`, `docs/terrain_cell_format.md`) checked against
/// the strip it draws. The game passes the record's `min`/`max` to `DrawIndexedPrimitive` as `MinVertexIndex`
/// and `NumVertices = min(max - min + 1, vertex count)`; a strip index outside that range, or past the
/// group's vertices, is a draw the GPU is told not to expect.
pub fn terrain_draw_ranges(wad: &Path, out: &Path) -> Result<(), String> {
    fn groups<'a>(nodes: &'a [UcfxNode], out: &mut Vec<&'a [UcfxNode]>) {
        if nodes.iter().any(|n| n.tag_str() == "PRMT") {
            out.push(nodes);
        }
        for n in nodes {
            groups(&n.children, out);
        }
    }
    let wanted = |t: u32| t == TYPE_HASH_TERRAIN_MESH || t == TYPE_HASH_LOWRES_TERRAIN;
    let chunks = pc_chunks(wad, |_, t| wanted(t))?;
    let mut keys: Vec<_> = chunks.keys().copied().collect();
    keys.sort();
    let mut tsv = String::from("type\tname\tgroup\trecord\tstart\tprims\tmin\tmax\tvertices\tused_min\tused_max\tbelow_min\tabove_max\tpast_vertices\n");
    let (mut records, mut bad_records, mut bad_cells) = (0usize, 0usize, 0usize);
    for key in keys {
        for (k, body) in chunks[&key].iter().enumerate() {
            let tree = parse_ucfx_tree(body).map_err(|e| format!("0x{:08X}.{k}: {e}", key.0))?;
            let mut found = Vec::new();
            groups(&tree, &mut found);
            let mut cell_bad = false;
            for (gi, group) in found.iter().enumerate() {
                let (mut verts, mut stride, mut strip, mut draws) = (None, 0usize, None, None);
                for c in group.iter() {
                    let row = |t: &str| c.children.iter().find(|x| x.tag_str() == t).and_then(|x| x.body.as_ref());
                    match c.tag_str().as_str() {
                        "STRM" => {
                            if let (Some(info), Some(data)) = (row("info"), row("data")) {
                                stride = u32::from_le_bytes([info[4], info[5], info[6], info[7]]) as usize;
                                verts = Some(data.len());
                            }
                        }
                        "IBUF" => strip = row("data"),
                        "PRMT" => draws = c.body.as_ref(),
                        _ => {}
                    }
                }
                let (Some(vbytes), Some(s), Some(d)) = (verts, strip, draws) else {
                    return Err(format!("0x{:08X}.{k} group {gi}: a draw group without STRM data, IBUF data or PRMT", key.0));
                };
                if stride == 0 {
                    return Err(format!("0x{:08X}.{k} group {gi}: STRM stride 0", key.0));
                }
                let vcount = vbytes / stride;
                let idx: Vec<u16> = s.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
                for (ri, rec) in d.chunks_exact(20).enumerate() {
                    let start = u32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]) as usize;
                    let prims = u16::from_le_bytes([rec[12], rec[13]]) as usize;
                    let (min, max) = (u16::from_le_bytes([rec[14], rec[15]]), u16::from_le_bytes([rec[16], rec[17]]));
                    if prims == 0 {
                        continue;
                    }
                    records += 1;
                    let run = idx.get(start..start + prims + 2).ok_or_else(|| {
                        format!("0x{:08X}.{k} group {gi} record {ri}: {prims} triangles at {start} run past a strip of {}", key.0, idx.len())
                    })?;
                    let (used_min, used_max) = (*run.iter().min().unwrap_or(&0), *run.iter().max().unwrap_or(&0));
                    let below = run.iter().filter(|&&i| i < min).count();
                    let above = run.iter().filter(|&&i| i > max).count();
                    let past = run.iter().filter(|&&i| i as usize >= vcount).count();
                    if below + above + past > 0 {
                        bad_records += 1;
                        cell_bad = true;
                    }
                    writeln!(
                        tsv,
                        "{}\t0x{:08X}.{k}\t{gi}\t{ri}\t{start}\t{prims}\t{min}\t{max}\t{vcount}\t{used_min}\t{used_max}\t{below}\t{above}\t{past}",
                        type_name_from_hash(key.1),
                        key.0
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            if cell_bad {
                bad_cells += 1;
            }
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{records} draw records; {bad_records} use an index outside their min..max or past their vertices, in {bad_cells} cell(s)");
    Ok(())
}

/// `terrain-position-w`: the terrain vertices' POSITION `w` (the fourth half of the FLOAT16_4 at offset 0),
/// tallied per vertex stream shape (stride and `decl`): the most common values and how many distinct values
/// there are.
pub fn terrain_position_w(wad: &Path, out: &Path) -> Result<(), String> {
    fn groups<'a>(nodes: &'a [UcfxNode], out: &mut Vec<&'a [UcfxNode]>) {
        if nodes.iter().any(|n| n.tag_str() == "STRM") {
            out.push(nodes);
        }
        for n in nodes {
            groups(&n.children, out);
        }
    }
    let chunks = pc_chunks(wad, |_, t| t == TYPE_HASH_TERRAIN_MESH || t == TYPE_HASH_LOWRES_TERRAIN)?;
    let mut tally: BTreeMap<(String, usize, String), HashMap<u16, usize>> = BTreeMap::new();
    for ((_, ty), bodies) in &chunks {
        for body in bodies {
            let tree = parse_ucfx_tree(body).map_err(|e| e.to_string())?;
            let mut found = Vec::new();
            groups(&tree, &mut found);
            for group in found {
                for c in group.iter().filter(|c| c.tag_str() == "STRM") {
                    let row = |t: &str| c.children.iter().find(|x| x.tag_str() == t).and_then(|x| x.body.as_ref());
                    let info = row("info").ok_or("a STRM without info")?;
                    let stride = u32::from_le_bytes([info[4], info[5], info[6], info[7]]) as usize;
                    let decl: String = row("decl").ok_or("a STRM without decl")?.iter().map(|b| format!("{b:02x}")).collect();
                    let data = row("data").ok_or("a STRM without data")?;
                    if stride < 8 {
                        return Err(format!("a STRM of stride {stride}"));
                    }
                    let t = tally.entry((type_name_from_hash(*ty).to_string(), stride, decl)).or_default();
                    for v in data.chunks_exact(stride) {
                        *t.entry(u16::from_le_bytes([v[6], v[7]])).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let mut tsv = String::from("type\tstride\tdecl\tvertices\tdistinct_w\ttop_w\n");
    for ((ty, stride, decl), t) in &tally {
        let total: usize = t.values().sum();
        let mut top: Vec<(&u16, &usize)> = t.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let shown = top.iter().take(6).map(|(w, n)| format!("{}:{n}", half_value(**w))).collect::<Vec<_>>().join(" ");
        writeln!(tsv, "{ty}\t{stride}\t{decl}\t{total}\t{}\t{shown}", t.len()).map_err(|e| e.to_string())?;
        println!("{ty} stride {stride}: {total} vertices, {} distinct w; top {shown}", t.len());
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    Ok(())
}

/// `terrain-alt-strips`: per terrain draw group, the triangles of its alt draw records (the records past
/// `INFO` word 0, `docs/terrain_cell_format.md`) against the triangles of its other records, as sorted vertex-index
/// triples over the group's one vertex stream. A group whose alt triangles are not among the others' draws a
/// surface in that pass the other passes do not.
pub fn terrain_alt_strips(wad: &Path, out: &Path) -> Result<(), String> {
    fn groups<'a>(nodes: &'a [UcfxNode], out: &mut Vec<&'a [UcfxNode]>) {
        if nodes.iter().any(|n| n.tag_str() == "PRMT") {
            out.push(nodes);
        }
        for n in nodes {
            groups(&n.children, out);
        }
    }
    fn tris(idx: &[u16], recs: &[&[u8]]) -> Result<HashSet<[u16; 3]>, String> {
        let mut out = HashSet::new();
        for rec in recs {
            let start = u32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]) as usize;
            let prims = u16::from_le_bytes([rec[12], rec[13]]) as usize;
            if prims == 0 {
                continue;
            }
            let run = idx.get(start..start + prims + 2).ok_or_else(|| format!("{prims} triangles at {start} run past a strip of {}", idx.len()))?;
            for t in run.windows(3) {
                if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                    continue;
                }
                let mut k = [t[0], t[1], t[2]];
                k.sort_unstable();
                out.insert(k);
            }
        }
        Ok(out)
    }
    let chunks = pc_chunks(wad, |_, t| t == TYPE_HASH_TERRAIN_MESH)?;
    let mut keys: Vec<_> = chunks.keys().copied().collect();
    keys.sort();
    let mut tsv = String::from("name\tgroup\tdraws\talt_draws\tdraw_triangles\talt_triangles\talt_not_in_draws\tdraws_not_in_alt\n");
    let (mut groups_n, mut with_alt, mut alt_outside) = (0usize, 0usize, 0usize);
    let (mut alt_total, mut outside_total) = (0usize, 0usize);
    for key in keys {
        for (k, body) in chunks[&key].iter().enumerate() {
            let tree = parse_ucfx_tree(body).map_err(|e| format!("0x{:08X}.{k}: {e}", key.0))?;
            let mut found = Vec::new();
            groups(&tree, &mut found);
            for (gi, group) in found.iter().enumerate() {
                let (mut info, mut strip, mut draws) = (None, None, None);
                for c in group.iter() {
                    match c.tag_str().as_str() {
                        "INFO" => info = c.body.as_ref(),
                        "IBUF" => strip = c.children.iter().find(|x| x.tag_str() == "data").and_then(|x| x.body.as_ref()),
                        "PRMT" => draws = c.body.as_ref(),
                        _ => {}
                    }
                }
                let (Some(info), Some(s), Some(d)) = (info, strip, draws) else {
                    return Err(format!("0x{:08X}.{k} group {gi}: a draw group without INFO, IBUF data or PRMT", key.0));
                };
                if info.len() < 8 {
                    return Err(format!("0x{:08X}.{k} group {gi}: INFO of {} bytes", key.0, info.len()));
                }
                let draw_count = u32::from_le_bytes([info[0], info[1], info[2], info[3]]) as usize;
                let recs: Vec<&[u8]> = d.chunks_exact(20).collect();
                if draw_count > recs.len() {
                    return Err(format!("0x{:08X}.{k} group {gi}: INFO names {draw_count} draws of {} records", key.0, recs.len()));
                }
                let idx: Vec<u16> = s.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
                let at = |e: String| format!("0x{:08X}.{k} group {gi}: {e}", key.0);
                let normal = tris(&idx, &recs[..draw_count]).map_err(at)?;
                let alt = tris(&idx, &recs[draw_count..]).map_err(|e| format!("0x{:08X}.{k} group {gi}: {e}", key.0))?;
                groups_n += 1;
                let outside = alt.difference(&normal).count();
                let missing = normal.difference(&alt).count();
                if !alt.is_empty() {
                    with_alt += 1;
                    alt_total += alt.len();
                    outside_total += outside;
                    if outside > 0 {
                        alt_outside += 1;
                    }
                }
                writeln!(
                    tsv,
                    "0x{:08X}.{k}\t{gi}\t{draw_count}\t{}\t{}\t{}\t{outside}\t{missing}",
                    key.0,
                    recs.len() - draw_count,
                    normal.len(),
                    alt.len()
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!(
        "{groups_n} terrain draw groups, {with_alt} with alt draws; {alt_outside} of those draw alt triangles the other draws do not ({outside_total} of {alt_total} alt triangles)"
    );
    Ok(())
}

/// `terrain-decls`: every terrain draw group's vertex stream shape, tallied: the `STRM` stride and `decl` bytes,
/// and the vertex-shader hashes its draw records name (`PRMT` +0). Two WADs' tallies side by side show a stream
/// layout or shader one has and the other does not.
pub fn terrain_decls(wad: &Path, out: &Path) -> Result<(), String> {
    fn groups<'a>(nodes: &'a [UcfxNode], out: &mut Vec<&'a [UcfxNode]>) {
        if nodes.iter().any(|n| n.tag_str() == "PRMT") {
            out.push(nodes);
        }
        for n in nodes {
            groups(&n.children, out);
        }
    }
    let wanted = |t: u32| t == TYPE_HASH_TERRAIN_MESH || t == TYPE_HASH_LOWRES_TERRAIN;
    let chunks = pc_chunks(wad, |_, t| wanted(t))?;
    let mut shapes: BTreeMap<(String, usize, String, Vec<String>), usize> = BTreeMap::new();
    for ((_, ty), bodies) in &chunks {
        for (k, body) in bodies.iter().enumerate() {
            let tree = parse_ucfx_tree(body).map_err(|e| format!("{}: {e}", k))?;
            let mut found = Vec::new();
            groups(&tree, &mut found);
            for group in found {
                let (mut stride, mut decl, mut shaders) = (None, None, Vec::new());
                for c in group.iter() {
                    let row = |t: &str| c.children.iter().find(|x| x.tag_str() == t).and_then(|x| x.body.as_ref());
                    match c.tag_str().as_str() {
                        "STRM" => {
                            let info = row("info").ok_or("a STRM without info")?;
                            stride = Some(u32::from_le_bytes([info[4], info[5], info[6], info[7]]) as usize);
                            decl = Some(row("decl").ok_or("a STRM without decl")?.iter().map(|b| format!("{b:02x}")).collect::<String>());
                        }
                        "PRMT" => {
                            let d = c.body.as_ref().ok_or("a PRMT without a body")?;
                            let mut s: Vec<String> = d.chunks_exact(20).map(|r| format!("{:08x}", u32::from_le_bytes([r[0], r[1], r[2], r[3]]))).collect();
                            s.sort();
                            s.dedup();
                            shaders = s;
                        }
                        _ => {}
                    }
                }
                let (Some(stride), Some(decl)) = (stride, decl) else { return Err("a draw group without a STRM".into()) };
                *shapes.entry((type_name_from_hash(*ty).to_string(), stride, decl, shaders)).or_insert(0) += 1;
            }
        }
    }
    let mut tsv = String::from("type\tgroups\tstride\tshaders\tdecl\n");
    // Each shape's vertex data against its stride: how many groups' data length is a whole number of
    // `stride`-byte vertices covering the highest index their draws use, and how many would be with 4 bytes more
    // or less per vertex.
    let mut fit: BTreeMap<(String, usize, String), [usize; 3]> = BTreeMap::new();
    for ((_, ty), bodies) in &chunks {
        for body in bodies {
            let tree = parse_ucfx_tree(body).map_err(|e| e.to_string())?;
            let mut found = Vec::new();
            groups(&tree, &mut found);
            for group in found {
                let (mut stride, mut decl, mut data_len, mut strip, mut draws) = (0usize, String::new(), 0usize, None, None);
                for c in group.iter() {
                    let row = |t: &str| c.children.iter().find(|x| x.tag_str() == t).and_then(|x| x.body.as_ref());
                    match c.tag_str().as_str() {
                        "STRM" => {
                            let info = row("info").ok_or("a STRM without info")?;
                            stride = u32::from_le_bytes([info[4], info[5], info[6], info[7]]) as usize;
                            decl = row("decl").ok_or("a STRM without decl")?.iter().map(|b| format!("{b:02x}")).collect();
                            data_len = row("data").ok_or("a STRM without data")?.len();
                        }
                        "IBUF" => strip = row("data"),
                        "PRMT" => draws = c.body.as_ref(),
                        _ => {}
                    }
                }
                let (Some(s), Some(d)) = (strip, draws) else { return Err("a draw group without IBUF data or PRMT".into()) };
                let idx: Vec<u16> = s.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
                let mut used = 0usize;
                for rec in d.chunks_exact(20) {
                    let start = u32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]) as usize;
                    let prims = u16::from_le_bytes([rec[12], rec[13]]) as usize;
                    if prims == 0 {
                        continue;
                    }
                    if let Some(run) = idx.get(start..start + prims + 2) {
                        used = used.max(run.iter().copied().max().unwrap_or(0) as usize + 1);
                    }
                }
                let e = fit.entry((type_name_from_hash(*ty).to_string(), stride, decl)).or_insert([0; 3]);
                for (k, s) in [stride, stride + 4, stride.saturating_sub(4)].into_iter().enumerate() {
                    if s > 0 && data_len % s == 0 && data_len / s >= used {
                        e[k] += 1;
                    }
                }
            }
        }
    }
    let mut fit_lines = Vec::new();
    for ((ty, stride, decl), [as_is, plus4, minus4]) in &fit {
        fit_lines.push(format!("{ty} stride {stride} decl {decl}: data fits stride {stride} in {as_is} group(s), {} in {plus4}, {} in {minus4}", stride + 4, stride.saturating_sub(4)));
    }
    for l in &fit_lines {
        println!("{l}");
    }
    for ((ty, stride, decl, shaders), n) in &shapes {
        writeln!(tsv, "{ty}\t{n}\t{stride}\t{}\t{decl}", shaders.join(",")).map_err(|e| e.to_string())?;
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{} distinct (type, stride, decl, shaders) shapes", shapes.len());
    Ok(())
}

/// One element of a PC vertex `decl` (`D3DVERTEXELEMENT9`, 8 bytes): offset, type, usage.
struct DeclElement {
    offset: usize,
    ty: u8,
    usage: u8,
}

/// The bytes a `D3DDECLTYPE` takes in a vertex.
fn decl_type_size(ty: u8) -> Result<usize, String> {
    Ok(match ty {
        0 | 4 | 5 | 6 | 8 | 9 | 11 | 13 | 14 | 15 => 4,
        1 | 7 | 10 | 12 | 16 => 8,
        2 => 12,
        3 => 16,
        _ => return Err(format!("decl type {ty}")),
    })
}

/// The elements of stream 0 of a PC `decl`, up to its end marker (stream 0xFF).
fn decl_elements(decl: &[u8]) -> Result<Vec<DeclElement>, String> {
    let mut out = Vec::new();
    for row in decl.chunks_exact(8) {
        let stream = u16::from_le_bytes([row[0], row[1]]);
        if stream == 0xFF {
            return Ok(out);
        }
        if stream != 0 {
            return Err(format!("a decl element of stream {stream}"));
        }
        out.push(DeclElement { offset: u16::from_le_bytes([row[2], row[3]]) as usize, ty: row[4], usage: row[6] });
    }
    Err("a decl without its end marker".into())
}

/// `terrain-vertex-compare`: two conversions of one archive's terrain (its PS3 and its Xbox copy), vertex by
/// vertex. A terrain chunk both WADs hold pools each stream shape's vertices (stride and `decl`) by position;
/// a position each side holds once is a pair. Per shape and `decl` element: the pairs whose element bytes are
/// equal, those whose half floats differ by at most 1/64 per component, and for the others the reordering that
/// makes the second side's bytes the first's (each first-side byte's index on the second side), or `other`.
pub fn terrain_vertex_compare(a: &Path, b: &Path, out: &Path) -> Result<(), String> {
    fn streams<'a>(nodes: &'a [UcfxNode], out: &mut Vec<&'a UcfxNode>) {
        for n in nodes {
            if n.tag_str() == "STRM" {
                out.push(n);
            }
            streams(&n.children, out);
        }
    }
    type Pool<'a> = HashMap<(usize, Vec<u8>), HashMap<[u8; 6], Vec<&'a [u8]>>>;
    fn pool<'a>(tree: &'a [UcfxNode]) -> Result<Pool<'a>, String> {
        let mut found = Vec::new();
        streams(tree, &mut found);
        let mut p: Pool<'a> = HashMap::new();
        for s in found {
            let row = |t: &str| s.children.iter().find(|x| x.tag_str() == t).and_then(|x| x.body.as_ref());
            let info = row("info").ok_or("a STRM without info")?;
            let stride = u32::from_le_bytes([info[4], info[5], info[6], info[7]]) as usize;
            let decl = row("decl").ok_or("a STRM without decl")?.clone();
            let data = row("data").ok_or("a STRM without data")?;
            if stride < 8 {
                return Err(format!("a STRM of stride {stride}"));
            }
            let shape = p.entry((stride, decl)).or_default();
            for v in data.chunks_exact(stride) {
                let mut key = [0u8; 6];
                key.copy_from_slice(&v[..6]);
                shape.entry(key).or_default().push(v);
            }
        }
        Ok(p)
    }
    #[derive(Default)]
    struct Tally {
        pairs: usize,
        equal: usize,
        close: usize,
        orders: BTreeMap<String, usize>,
        samples: Vec<String>,
    }
    let wanted = |t: u32| t == TYPE_HASH_TERRAIN_MESH || t == TYPE_HASH_LOWRES_TERRAIN;
    let (left, right) = (pc_chunks(a, |_, t| wanted(t))?, pc_chunks(b, |_, t| wanted(t))?);
    let mut tallies: BTreeMap<(usize, String, usize, u8, u8), Tally> = BTreeMap::new();
    let (mut paired, mut unpaired) = (0usize, 0usize);
    for (key, bodies) in &left {
        let Some(twins) = right.get(key) else { continue };
        for (x, y) in bodies.iter().zip(twins) {
            let (tx, ty) = (parse_ucfx_tree(x)?, parse_ucfx_tree(y)?);
            let (px, py) = (pool(&tx).map_err(|e| format!("first WAD, 0x{:08X}: {e}", key.0))?, pool(&ty).map_err(|e| format!("second WAD, 0x{:08X}: {e}", key.0))?);
            for ((stride, decl), verts) in &px {
                let elements = decl_elements(decl).map_err(|e| format!("0x{:08X}: {e}", key.0))?;
                let decl_hex: String = decl.iter().map(|b| format!("{b:02x}")).collect();
                let twin = py.get(&(*stride, decl.clone()));
                for (pos, vs) in verts {
                    let other = twin.and_then(|t| t.get(pos));
                    let (Some(&[u]), Some(&[w])) = (Some(vs.as_slice()).filter(|v| v.len() == 1), other.map(|o| o.as_slice()).filter(|o| o.len() == 1)) else {
                        unpaired += vs.len();
                        continue;
                    };
                    paired += 1;
                    for e in &elements {
                        let size = decl_type_size(e.ty)?;
                        let (p, q) = (u.get(e.offset..e.offset + size), w.get(e.offset..e.offset + size));
                        let (Some(p), Some(q)) = (p, q) else {
                            return Err(format!("0x{:08X}: an element at +{} of {size} bytes past a stride of {stride}", key.0, e.offset));
                        };
                        let t = tallies.entry((*stride, decl_hex.clone(), e.offset, e.ty, e.usage)).or_default();
                        t.pairs += 1;
                        if p == q {
                            t.equal += 1;
                            continue;
                        }
                        if e.ty == 15 || e.ty == 16 {
                            let halves = |s: &[u8]| s.chunks_exact(2).map(|h| half_value(u16::from_le_bytes([h[0], h[1]]))).collect::<Vec<_>>();
                            if halves(p).iter().zip(halves(q)).all(|(m, n)| (m - n).abs() <= 1.0 / 64.0) {
                                t.close += 1;
                                continue;
                            }
                        }
                        if e.ty == 4 && p.iter().zip(q).all(|(m, n)| m.abs_diff(*n) <= 1) {
                            t.close += 1;
                            continue;
                        }
                        let order = if size == 4 {
                            let mut found = None;
                            'perm: for i in 0..4 {
                                for j in 0..4 {
                                    for k in 0..4 {
                                        for l in 0..4 {
                                            let idx = [i, j, k, l];
                                            let mut seen = [false; 4];
                                            if idx.iter().any(|&n| std::mem::replace(&mut seen[n], true)) {
                                                continue;
                                            }
                                            if (0..4).all(|n| p[n] == q[idx[n]]) {
                                                found = Some(format!("{i}{j}{k}{l}"));
                                                break 'perm;
                                            }
                                        }
                                    }
                                }
                            }
                            found.unwrap_or_else(|| "other".into())
                        } else {
                            "other".into()
                        };
                        if t.samples.len() < 4 {
                            let hex = |s: &[u8]| s.iter().map(|b| format!("{b:02x}")).collect::<String>();
                            t.samples.push(format!("{}/{}", hex(p), hex(q)));
                        }
                        *t.orders.entry(order).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let mut tsv = String::from("stride\tdecl\toffset\ttype\tusage\tpairs\tequal\tclose\tdiffering_by_order\tsamples_first/second\n");
    for ((stride, decl, offset, ty, usage), t) in &tallies {
        let mut orders: Vec<(&String, &usize)> = t.orders.iter().collect();
        orders.sort_by(|m, n| n.1.cmp(m.1));
        let shown = orders.iter().take(6).map(|(o, n)| format!("{o}:{n}")).collect::<Vec<_>>().join(" ");
        let samples = t.samples.join(" ");
        writeln!(tsv, "{stride}\t{decl}\t{offset}\t{ty}\t{usage}\t{}\t{}\t{}\t{shown}\t{samples}", t.pairs, t.equal, t.close).map_err(|e| e.to_string())?;
        println!("stride {stride} +{offset} type {ty} usage {usage}: {} pairs, {} equal, {} close; {shown}; e.g. {samples}", t.pairs, t.equal, t.close);
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{paired} vertices paired by position, {unpaired} not");
    Ok(())
}

/// `terrain-half-tally`: per terrain stream shape (stride and `decl`), the values of one half-float component of
/// the `FLOAT16_4` element of one usage (`component` 0–3), tallied: the most common values and how many distinct
/// values there are.
pub fn terrain_half_tally(wad: &Path, usage: u8, component: usize, out: &Path) -> Result<(), String> {
    fn streams<'a>(nodes: &'a [UcfxNode], out: &mut Vec<&'a UcfxNode>) {
        for n in nodes {
            if n.tag_str() == "STRM" {
                out.push(n);
            }
            streams(&n.children, out);
        }
    }
    if component > 3 {
        return Err(format!("component {component} of a FLOAT16_4"));
    }
    let chunks = pc_chunks(wad, |_, t| t == TYPE_HASH_TERRAIN_MESH || t == TYPE_HASH_LOWRES_TERRAIN)?;
    let mut tally: BTreeMap<(usize, String), HashMap<u16, usize>> = BTreeMap::new();
    for bodies in chunks.values() {
        for body in bodies {
            let tree = parse_ucfx_tree(body).map_err(|e| e.to_string())?;
            let mut found = Vec::new();
            streams(&tree, &mut found);
            for s in found {
                let row = |t: &str| s.children.iter().find(|x| x.tag_str() == t).and_then(|x| x.body.as_ref());
                let info = row("info").ok_or("a STRM without info")?;
                let stride = u32::from_le_bytes([info[4], info[5], info[6], info[7]]) as usize;
                let decl = row("decl").ok_or("a STRM without decl")?;
                let data = row("data").ok_or("a STRM without data")?;
                let Some(e) = decl_elements(decl)?.into_iter().find(|e| e.usage == usage && e.ty == 16) else { continue };
                let at = e.offset + component * 2;
                let t = tally.entry((stride, decl.iter().map(|b| format!("{b:02x}")).collect())).or_default();
                for v in data.chunks_exact(stride) {
                    let h = v.get(at..at + 2).ok_or_else(|| format!("an element at +{at} past a stride of {stride}"))?;
                    *t.entry(u16::from_le_bytes([h[0], h[1]])).or_insert(0) += 1;
                }
            }
        }
    }
    let mut tsv = String::from("stride\tdecl\tvertices\tdistinct\ttop\n");
    for ((stride, decl), t) in &tally {
        let total: usize = t.values().sum();
        let mut top: Vec<(&u16, &usize)> = t.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let shown = top.iter().take(6).map(|(w, n)| format!("{}:{n}", half_value(**w))).collect::<Vec<_>>().join(" ");
        writeln!(tsv, "{stride}\t{decl}\t{total}\t{}\t{shown}", t.len()).map_err(|e| e.to_string())?;
        println!("stride {stride}: {total} vertices, {} distinct; top {shown}", t.len());
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    Ok(())
}

/// One group's vertex streams on PS3 as stored, beside the one PC stream of the same group in a
/// converted WAD, joined by (name, type, the group's index among that chunk's groups, its vertex
/// count). A row counts the groups of that shape.
pub fn decl_table(ps3: &Path, converted: &Path, out: &Path) -> Result<(), String> {
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let chunks = archive_chunks(ps3, |_, t| t == TYPE_HASH_MODEL || t == TYPE_HASH_TERRAIN_MESH || t == mercs2_formats::scrub::TYPE_HASH || t == TYPE_HASH_LOWRES_TERRAIN)?;
    // A scrub chunk aligns its bodies, which the tree reader refuses, so it has no twin column.
    let pc = pc_chunks(converted, |_, t| t == TYPE_HASH_MODEL || t == TYPE_HASH_TERRAIN_MESH || t == TYPE_HASH_LOWRES_TERRAIN)?;
    // Each PC group as (vertex count, stride, decl).
    fn pc_groups(nodes: &[UcfxNode], out: &mut Vec<(u32, u32, Vec<u8>)>) {
        for n in nodes {
            if n.tag_str() == "STRM" {
                let row = |t: &str| n.children.iter().find(|c| c.tag_str() == t).and_then(|c| c.body.clone());
                if let (Some(info), Some(decl)) = (row("info"), row("decl")) {
                    let w = |o: usize| u32::from_le_bytes([info[o], info[o + 1], info[o + 2], info[o + 3]]);
                    out.push((w(8), w(4), decl));
                }
            }
            pc_groups(&n.children, out);
        }
    }
    let mut twins: HashMap<(u32, u32), Vec<(u32, u32, Vec<u8>)>> = HashMap::new();
    for ((name, ty), bodies) in &pc {
        let list = twins.entry((*name, *ty)).or_default();
        for b in bodies {
            pc_groups(&parse_ucfx_tree(b).map_err(|e| format!("0x{name:08X} {}: {e}", type_name_from_hash(*ty)))?, list);
        }
    }
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    for c in &chunks {
        let rows = container_rows(&c.body)?;
        // Streams in row order: each `STRM` marker opens one, each `PRMG` a group.
        let (mut groups, mut strm): (Vec<Vec<(String, u32)>>, Option<(Vec<u8>, u32, u32)>) = (Vec::new(), None);
        let flush = |groups: &mut Vec<Vec<(String, u32)>>, s: &mut Option<(Vec<u8>, u32, u32)>| {
            if let Some((d, stride, n)) = s.take() {
                if let Some(g) = groups.last_mut() {
                    g.push((format!("{}/{stride}", hex(&d)), n));
                }
            }
        };
        let mut info: Option<(u32, u32)> = None;
        for (tag, off, size) in &rows {
            let body = c.body.get(*off..off + size).unwrap_or(&[]);
            match tag {
                b"PRMG" => {
                    flush(&mut groups, &mut strm);
                    groups.push(Vec::new());
                }
                b"STRM" => flush(&mut groups, &mut strm),
                b"info" if body.len() == 12 => info = Some((be32(body, 4)?, be32(body, 8)?)),
                b"decl" => {
                    let (stride, n) = info.take().unwrap_or((0, 0));
                    strm = Some((body.to_vec(), stride, n));
                }
                _ => {}
            }
        }
        flush(&mut groups, &mut strm);
        let empty = Vec::new();
        let list = twins.get(&(c.name, c.ty)).unwrap_or(&empty);
        for g in groups.iter().filter(|g| !g.is_empty()) {
            let n = g[0].1;
            let twin = list.iter().find(|(v, ..)| *v == n).map_or("-".to_string(), |(_, s, d)| format!("{}/{s}", hex(d)));
            let names: Vec<&str> = g.iter().map(|(d, _)| d.as_str()).collect();
            *shapes.entry(format!("{}\t{}\t{twin}", type_name_from_hash(c.ty), names.join(" + "))).or_default() += 1;
        }
    }
    let mut tsv = String::from("groups\ttype\tps3 streams\tpc stream\n");
    let mut rows: Vec<_> = shapes.into_iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in rows {
        writeln!(tsv, "{n}\t{k}").map_err(|e| e.to_string())?;
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))
}

/// The chunks of a little-endian archive that `keep` accepts, by (name, type), bodies in block order.
fn pc_chunks(wad: &Path, keep: impl Fn(u32, u32) -> bool) -> Result<HashMap<(u32, u32), Vec<Vec<u8>>>, String> {
    let mut f = std::fs::File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let mut out: HashMap<(u32, u32), Vec<Vec<u8>>> = HashMap::new();
    for b in 0..pc.indx.len() {
        let dec = mercs2_formats::sges::decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("block {b}: {e}"))?;
        for (e, range) in mercs2_formats::ucfx::block_chunk_spans(&dec).map_err(|e| format!("block {b}: {e}"))? {
            if keep(e.name_hash, e.type_hash) {
                out.entry((e.name_hash, e.type_hash)).or_default().push(dec[range].to_vec());
            }
        }
    }
    Ok(out)
}

fn tree_rows<'a>(nodes: &'a [UcfxNode], prefix: &str, out: &mut BTreeMap<String, Vec<&'a [u8]>>) {
    for n in nodes {
        let path = if prefix.is_empty() { n.tag_str() } else { format!("{prefix}/{}", n.tag_str()) };
        if let Some(b) = &n.body {
            out.entry(path.clone()).or_default().push(b);
        }
        tree_rows(&n.children, &path, out);
    }
}

/// The UCFX rows two little-endian chunks differ in, each as `path:bytes`, the bytes differing where
/// the two bodies are one size and `size` where they are not.
fn row_delta(a: &[u8], b: &[u8]) -> Result<Vec<String>, String> {
    let (x, y) = (parse_ucfx_tree(a)?, parse_ucfx_tree(b)?);
    let (mut p, mut q) = (BTreeMap::new(), BTreeMap::new());
    tree_rows(&x, "", &mut p);
    tree_rows(&y, "", &mut q);
    let mut out = Vec::new();
    for (path, bodies) in &p {
        let twins = q.get(path).map_or(&[][..], |t| t.as_slice());
        for (i, body) in bodies.iter().enumerate() {
            match twins.get(i) {
                Some(t) if t == body => {}
                Some(t) if t.len() == body.len() => {
                    out.push(format!("{path}:{}", body.iter().zip(t.iter()).filter(|(m, n)| m != n).count()));
                }
                _ => out.push(format!("{path}:size")),
            }
        }
    }
    Ok(out)
}

/// Every chunk of a converted archive that the PC base also holds, row by row against the closest PC
/// chunk of its key. `only` limits the run to a type hash.
pub fn pc_diff(converted: &Path, pc: &Path, only: Option<u32>, out: &Path) -> Result<(), String> {
    let ours = pc_chunks(converted, |_, t| only.is_none_or(|o| o == t))?;
    let base = pc_chunks(pc, |n, t| ours.contains_key(&(n, t)))?;
    let mut tsv = String::from("type\tname\tverdict\trows\n");
    let mut totals: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    for (&(name, ty), bodies) in &ours {
        let Some(twins) = base.get(&(name, ty)) else { continue };
        let type_name = type_name_from_hash(ty).to_string();
        for body in bodies {
            let t = totals.entry(type_name.clone()).or_default();
            if twins.iter().any(|b| b == body) {
                t[0] += 1;
                writeln!(tsv, "{type_name}\t0x{name:08X}\tequal\t").map_err(|e| e.to_string())?;
                continue;
            }
            let delta = twins.iter().map(|b| row_delta(body, b)).collect::<Result<Vec<_>, _>>();
            match delta.ok().and_then(|d| d.into_iter().min_by_key(Vec::len)) {
                Some(rows) => {
                    t[1] += 1;
                    writeln!(tsv, "{type_name}\t0x{name:08X}\tdiffer\t{}", rows.join(" ")).map_err(|e| e.to_string())?;
                }
                None => {
                    t[2] += 1;
                    writeln!(tsv, "{type_name}\t0x{name:08X}\tunreadable\t").map_err(|e| e.to_string())?;
                }
            }
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{:<24}{:>8}{:>8}{:>12}", "type", "equal", "differ", "unreadable");
    for (ty, [e, d, u]) in &totals {
        println!("{ty:<24}{e:>8}{d:>8}{u:>12}");
    }
    Ok(())
}

/// Each material's pixel shader in a model container's top-level `MTRL`: `INFO` +0x24 records of
/// `116 + 4·textures` bytes, the texture count at +106, the pixel shader after the textures. A chain's
/// lower rungs carry no `MTRL` and yield nothing.
fn material_pixel_shaders(container: &[u8]) -> Result<Vec<u32>, String> {
    let roots = parse_ucfx_tree(container)?;
    let leaf = |t: &[u8; 4]| roots.iter().find(|n| &n.tag == t).and_then(|n| n.body.as_ref());
    let (Some(info), Some(mtrl)) = (leaf(b"INFO"), leaf(b"MTRL")) else { return Ok(Vec::new()) };
    let count = u32::from_le_bytes([info[0x24], info[0x25], info[0x26], info[0x27]]) as usize;
    let (mut out, mut p) = (Vec::with_capacity(count), 0usize);
    for k in 0..count {
        let r = mtrl.get(p..).filter(|r| r.len() >= 116).ok_or_else(|| format!("MTRL material {k} at +{p} runs past {} bytes", mtrl.len()))?;
        let textures = u16::from_le_bytes([r[106], r[107]]) as usize;
        let at = 108 + 4 * textures;
        let w = r.get(at..at + 4).ok_or_else(|| format!("MTRL material {k} with {textures} textures runs past the leaf"))?;
        out.push(u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
        p += 116 + 4 * textures;
    }
    if p != mtrl.len() {
        return Err(format!("MTRL holds {} bytes and its {count} materials fill {p}", mtrl.len()));
    }
    Ok(out)
}

/// `pixel-shaders`: every material pixel shader the model containers of `dir` name, against the set the
/// models of a PC archive name. A shader no PC model names is one the PC engine may never have loaded.
pub fn pixel_shaders(dir: &Path, pc: &Path) -> Result<(), String> {
    let mut retail: BTreeMap<u32, usize> = BTreeMap::new();
    for bodies in pc_chunks(pc, |_, t| t == TYPE_HASH_MODEL)?.into_values() {
        for mut b in bodies {
            // A few retail lower rungs end in zeros, not a `CSUM` trailer (`model::Trailer::Zeroed`).
            if b[b.len() - 8..].iter().all(|&x| x == 0) {
                b.truncate(b.len() - 8);
                let sum = mercs2_formats::crc32::crc32_mercs2(&b);
                b.extend_from_slice(b"CSUM");
                b.extend_from_slice(&sum.to_le_bytes());
            }
            for ps in material_pixel_shaders(&b)? {
                *retail.entry(ps).or_default() += 1;
            }
        }
    }
    let mut ours: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut files: Vec<_> = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    files.sort_by_key(|f| f.file_name());
    for f in files {
        let p = f.path();
        if !p.extension().is_some_and(|x| x == "ucfx") {
            continue;
        }
        let body = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        for ps in material_pixel_shaders(&body).map_err(|e| format!("{}: {e}", p.display()))? {
            ours.entry(ps).or_default().push(f.file_name().to_string_lossy().into_owned());
        }
    }
    println!("{} pixel shaders across PC's models; {} across {}", retail.len(), ours.len(), dir.display());
    for (ps, users) in &ours {
        let mut models = users.clone();
        models.dedup();
        match retail.get(ps) {
            Some(n) => println!("  0x{ps:08X}: {} material(s) here, {n} on PC", users.len()),
            None => println!("  0x{ps:08X}: {} material(s) here, NONE on PC: {}", users.len(), models.join(" ")),
        }
    }
    Ok(())
}

/// The vertex shader pair (`INFO` +12, +16) of every group of a model's rung chain, in order.
fn group_shaders(nodes: &[UcfxNode], out: &mut Vec<(u32, u32)>) {
    for n in nodes {
        if &n.tag == b"PRMG" {
            if let Some(info) = n.children.iter().find(|c| &c.tag == b"INFO").and_then(|c| c.body.as_ref()) {
                if info.len() >= 20 {
                    out.push((u32::from_le_bytes([info[12], info[13], info[14], info[15]]), u32::from_le_bytes([info[16], info[17], info[18], info[19]])));
                }
            }
        }
        group_shaders(&n.children, out);
    }
}

/// `vs-compare`: every model of a big-endian archive, converted, against its PC twin by the vertex
/// shaders its groups draw with, group by group. Prints each `(ours, PC)` pair that differs with its count,
/// and the models whose group count differs.
pub fn vs_compare(archive: &Path, pc: &Path, out: &Path) -> Result<(), String> {
    use mercs2_formats::be_to_le::convert::convert_block;
    use mercs2_formats::be_to_le::texture::{BlockTables, TextureTable};
    let chunks = archive_chunks(archive, |_, t| t == TYPE_HASH_MODEL)?;
    let textures = TextureTable::of_models(chunks.iter().map(|c| (c.name, &c.body[..])))?;
    let tables = BlockTables { textures: &textures, rung: None, streams: None };
    let mut ours: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    let mut refused: BTreeMap<u32, String> = BTreeMap::new();
    for c in &chunks {
        let mut one = Vec::with_capacity(20 + c.body.len());
        for w in [1u32, c.name, c.ty, 0, c.body.len() as u32] {
            one.extend_from_slice(&w.to_be_bytes());
        }
        one.extend_from_slice(&c.body);
        let list = ours.entry(c.name).or_default();
        let done = convert_block(&one, false, None, None, Some(tables)).and_then(|le| {
            let (_, range) = mercs2_formats::ucfx::block_chunk_spans(&le)?.into_iter().next().ok_or("no chunk")?;
            group_shaders(&parse_ucfx_tree(&le[range])?, list);
            Ok(())
        });
        if let Err(e) = done {
            refused.insert(c.name, e);
        }
    }
    let twins = pc_chunks(pc, |_, t| t == TYPE_HASH_MODEL)?;
    let (mut equal, mut differ, mut shape) = (0usize, 0usize, 0usize);
    let mut pairs: BTreeMap<((u32, u32), (u32, u32)), usize> = BTreeMap::new();
    let mut tsv = String::from("model\tgroups\tpc_groups\tdiffering_groups\n");
    for (name, list) in &ours {
        if refused.contains_key(name) {
            continue;
        }
        let mut theirs = Vec::new();
        for chunk in twins.get(&(*name, TYPE_HASH_MODEL)).map_or(&[][..], Vec::as_slice) {
            let mut sealed = chunk.clone();
            if chunk[chunk.len() - 8..].iter().all(|&b| b == 0) {
                sealed.truncate(chunk.len() - 8);
                let sum = mercs2_formats::crc32::crc32_mercs2(&sealed);
                sealed.extend_from_slice(b"CSUM");
                sealed.extend_from_slice(&sum.to_le_bytes());
            }
            group_shaders(&parse_ucfx_tree(&sealed)?, &mut theirs);
        }
        if theirs.len() != list.len() {
            shape += 1;
            let _ = writeln!(tsv, "0x{name:08X}\t{}\t{}\t-", list.len(), theirs.len());
            continue;
        }
        // Groups are compared as a multiset: a chain may list its groups in another order than PC's.
        let (mut a, mut b) = (list.clone(), theirs.clone());
        a.sort_unstable();
        b.sort_unstable();
        let bad = a.iter().zip(&b).filter(|(x, y)| x != y).count();
        for (x, y) in a.iter().zip(&b).filter(|(x, y)| x != y) {
            *pairs.entry((*x, *y)).or_default() += 1;
        }
        if bad == 0 {
            equal += 1;
        } else {
            differ += 1;
            let _ = writeln!(tsv, "0x{name:08X}\t{}\t{}\t{bad}", list.len(), theirs.len());
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{equal} models draw every group with PC's vertex shaders, {differ} differ, {shape} with another group count, {} refused", refused.len());
    for ((a, b), n) in &pairs {
        println!("  ours (0x{:08X}, 0x{:08X}) PC (0x{:08X}, 0x{:08X}): {n} group(s)", a.0, a.1, b.0, b.1);
    }
    Ok(())
}

/// `type-compare`: every chunk of one type in a big-endian archive, converted alone, against its PC twin
/// byte for byte. Each differing chunk prints its size on both sides and its first differing offset.
pub fn type_compare(archive: &Path, pc: &Path, type_name: &str) -> Result<(), String> {
    use mercs2_formats::be_to_le::convert::convert_block;
    let ty = mercs2_formats::hash::pandemic_hash_m2(type_name);
    if type_name_from_hash(ty) != type_name {
        return Err(format!("{type_name} is not a type this build names (its hash 0x{ty:08X} reads {})", type_name_from_hash(ty)));
    }
    let chunks = archive_chunks(archive, |_, t| t == ty)?;
    let twins = pc_chunks(pc, |_, t| t == ty)?;
    let (mut equal, mut differ, mut alone) = (0usize, 0usize, 0usize);
    for c in &chunks {
        let mut one = Vec::with_capacity(20 + c.body.len());
        for w in [1u32, c.name, c.ty, 0, c.body.len() as u32] {
            one.extend_from_slice(&w.to_be_bytes());
        }
        one.extend_from_slice(&c.body);
        let le = convert_block(&one, false, None, None, None).map_err(|e| format!("0x{:08X}: {e}", c.name))?;
        let (_, range) = mercs2_formats::ucfx::block_chunk_spans(&le)?.into_iter().next().ok_or("no chunk")?;
        let ours = &le[range];
        let Some(theirs) = twins.get(&(c.name, ty)) else {
            alone += 1;
            continue;
        };
        if theirs.iter().any(|t| t[..] == ours[..]) {
            equal += 1;
            continue;
        }
        differ += 1;
        let t = &theirs[0];
        let first = ours.iter().zip(t.iter()).position(|(a, b)| a != b).unwrap_or(ours.len().min(t.len()));
        println!("0x{:08X} {}: ours {} bytes, PC {} bytes, first difference at +0x{first:X}", c.name, c.block, ours.len(), t.len());
    }
    println!("{type_name}: {} chunks, {equal} equal PC, {differ} differ, {alone} without a PC twin", chunks.len());
    Ok(())
}

/// `frame-capture`: a profiler frame capture's world matrices, each a row-major 3×4 transform in a 4×4 (column 3
/// the translation, row 3 `(0, 0, 0, 1)`: the table's identity entries and placed objects read this way), with
/// the abnormal ones listed and the packets that draw with them (a packet's `u16 +0x38` is its matrix index). A
/// draw with an abnormal matrix stretches its triangles across the screen.
pub fn frame_capture(file: &Path, out: &Path) -> Result<(), String> {
    use mercs2_formats::frame_capture::{FrameCapture, Store};
    let c = FrameCapture::decode(&std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?)?;
    println!("frame {} parity {}", c.frame, c.parity);
    for (store, s) in Store::ALL.into_iter().zip(&c.sections) {
        println!("  {store:?}: {} of {} at {:#x}, {} bytes copied", s.used, s.size, s.address, s.bytes.len());
    }
    let matrices = c.matrices();
    let mut tsv = String::from("index\tscale_x\tscale_y\tscale_z\ttx\tty\ttz\trow3\tflags\n");
    let mut flagged: BTreeMap<usize, String> = BTreeMap::new();
    for (i, m) in matrices.iter().enumerate() {
        // The basis vectors are the 3×3's columns; each one's length is that axis's scale.
        let column = |c: usize| (m[c] * m[c] + m[4 + c] * m[4 + c] + m[8 + c] * m[8 + c]).sqrt();
        let scales = [column(0), column(1), column(2)];
        let t = [m[3], m[7], m[11]];
        let row3 = [m[12], m[13], m[14], m[15]];
        let mut why = Vec::new();
        if m.iter().any(|v| !v.is_finite()) {
            why.push("not finite");
        }
        if scales.iter().any(|&s| s > 100.0) {
            why.push("scale over 100");
        }
        if scales.iter().all(|&s| s < 1e-4) {
            why.push("collapsed");
        }
        if row3 != [0.0, 0.0, 0.0, 1.0] {
            why.push("projective row");
        }
        if t.iter().any(|v| v.abs() > 20_000.0) {
            why.push("translation past 20 km");
        }
        let flags = why.join(", ");
        let _ = writeln!(tsv, "{i}\t{}\t{}\t{}\t{}\t{}\t{}\t{row3:?}\t{flags}", scales[0], scales[1], scales[2], t[0], t[1], t[2]);
        if !why.is_empty() {
            flagged.insert(i, flags);
        }
    }
    let packets = &c.section(Store::Packets).bytes;
    let stride = if c.section(Store::Packets).used == 0 { 0 } else { packets.len() / c.section(Store::Packets).used.min(c.section(Store::Packets).size) as usize };
    let mut users: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut past = 0usize;
    if stride > 0 {
        for (p, pk) in packets.chunks_exact(stride).enumerate() {
            let m = u16::from_le_bytes([pk[0x38], pk[0x39]]) as usize;
            if m >= matrices.len() {
                past += 1;
            }
            users.entry(m).or_default().push(p);
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{} matrices, {} flagged; {} packets of {stride} bytes, {past} naming a matrix past the frame's count", matrices.len(), flagged.len(), packets.len() / stride.max(1));
    for (i, why) in &flagged {
        let ps = users.get(i).map_or(Vec::new(), Vec::clone);
        println!("  matrix {i}: {why}; {} packet(s) {:?}", ps.len(), &ps[..ps.len().min(8)]);
        for &p in ps.iter().take(3) {
            let pk = &packets[p * stride..(p + 1) * stride];
            let words: Vec<String> = pk.chunks_exact(4).map(|w| format!("{:08x}", u32::from_le_bytes([w[0], w[1], w[2], w[3]]))).collect();
            println!("    packet {p}: {}", words.join(" "));
        }
    }
    Ok(())
}

/// One group's positions: `STRM` `decl` element of usage 0, as `FLOAT3` (type 2) or `FLOAT16_4` (type 16).
fn group_positions(group: &[UcfxNode]) -> Result<Vec<[f32; 3]>, String> {
    let strm = group.iter().find(|n| &n.tag == b"STRM").ok_or("a group has no STRM")?;
    let row = |t: &str| strm.children.iter().find(|c| c.tag_str() == t).and_then(|c| c.body.as_ref());
    let (info, decl, data) = (row("info").ok_or("no STRM info")?, row("decl").ok_or("no STRM decl")?, row("data").ok_or("no STRM data")?);
    let stride = u32::from_le_bytes([info[4], info[5], info[6], info[7]]) as usize;
    let e = decl.chunks_exact(8).find(|e| e[0] == 0 && e[6] == 0 && e[7] == 0).ok_or("no position element")?;
    let (at, ty) = (u16::from_le_bytes([e[2], e[3]]) as usize, e[4]);
    if stride == 0 || data.len() % stride != 0 {
        return Err(format!("STRM data of {} bytes at stride {stride}", data.len()));
    }
    data.chunks_exact(stride)
        .map(|v| match ty {
            2 => Ok(std::array::from_fn(|k| f32::from_le_bytes([v[at + 4 * k], v[at + 4 * k + 1], v[at + 4 * k + 2], v[at + 4 * k + 3]]))),
            16 => Ok(std::array::from_fn(|k| half_value(u16::from_le_bytes([v[at + 2 * k], v[at + 2 * k + 1]])))),
            t => Err(format!("position type {t}")),
        })
        .collect()
}

/// `model-bounds`: each model container of a directory against its own top-level `INFO` box: how far
/// its groups' vertices reach past the box, and how many are not finite. A vertex thrown away from its
/// model draws a triangle across the scene; it shows here as a reach far past the box.
pub fn model_bounds(dir: &Path, out: &Path) -> Result<(), String> {
    fn groups<'a>(nodes: &'a [UcfxNode], out: &mut Vec<&'a [UcfxNode]>) {
        for n in nodes {
            if &n.tag == b"PRMG" {
                out.push(&n.children);
            }
            groups(&n.children, out);
        }
    }
    // A model's rungs are `<hash>.<rung>.ucfx`; only the chain's top rung carries the INFO box.
    let mut chains: BTreeMap<String, Vec<std::path::PathBuf>> = BTreeMap::new();
    for f in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = f.map_err(|e| e.to_string())?.path();
        if p.extension().is_some_and(|x| x == "ucfx") {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let model = name.split('.').next().unwrap_or_default().to_string();
            chains.entry(model).or_default().push(p);
        }
    }
    let mut tsv = String::from("model\tbox_min\tbox_max\trungs\tgroups\tvertices\tnot_finite\treach_past_box_m\tbox_diagonal_m\n");
    let (mut models, mut flagged) = (0usize, 0usize);
    for (model, mut rungs) in chains {
        rungs.sort();
        let trees = rungs
            .iter()
            .map(|p| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())).and_then(|b| parse_ucfx_tree(&b).map_err(|e| format!("{}: {e}", p.display()))))
            .collect::<Result<Vec<_>, _>>()?;
        let mut boxes = trees.iter().filter_map(|t| t.iter().find(|n| &n.tag == b"INFO").and_then(|n| n.body.as_ref()));
        let info = boxes.next().ok_or_else(|| format!("{model}: no rung carries an INFO"))?;
        if boxes.next().is_some() {
            return Err(format!("{model}: more than one rung carries an INFO"));
        }
        let f32_at = |o: usize| f32::from_le_bytes([info[o], info[o + 1], info[o + 2], info[o + 3]]);
        let (lo, hi) = ([f32_at(4), f32_at(8), f32_at(12)], [f32_at(16), f32_at(20), f32_at(24)]);
        let mut found = Vec::new();
        for t in &trees {
            groups(t, &mut found);
        }
        let (mut vertices, mut bad, mut reach) = (0usize, 0usize, 0f32);
        for g in &found {
            for v in group_positions(g).map_err(|e| format!("{model}: {e}"))? {
                vertices += 1;
                if v.iter().any(|x| !x.is_finite()) {
                    bad += 1;
                    continue;
                }
                for k in 0..3 {
                    reach = reach.max(lo[k] - v[k]).max(v[k] - hi[k]);
                }
            }
        }
        let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
        models += 1;
        if bad > 0 || reach > 1.0 {
            flagged += 1;
        }
        let _ = writeln!(tsv, "{model}\t{lo:?}\t{hi:?}\t{}\t{}\t{vertices}\t{bad}\t{reach:.3}\t{diag:.3}", rungs.len(), found.len());
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("{models} models, {flagged} with a vertex past its box by more than 1 m or not finite");
    Ok(())
}

/// `terrain-bounds`: each `terrainmesh` and `lowresterrain` cell's vertex extents (cell-local), from a PC
/// archive or a directory holding `terrainmesh/` and `lowresterrain/` folders of `.ucfx` containers. One row
/// per cell; the summary prints each type's extent ranges, so a cell drawn somewhere else stands out. A
/// group is any row holding a `STRM`: a `terrainmesh` `PRMG`, or a `lowresterrain` cell's `GEOM` itself.
pub fn terrain_bounds(input: &Path, out: &Path) -> Result<(), String> {
    use mercs2_formats::types::{TYPE_HASH_LOWRES_TERRAIN, TYPE_HASH_TERRAIN_MESH};
    fn groups<'a>(nodes: &'a [UcfxNode], out: &mut Vec<&'a [UcfxNode]>) {
        for n in nodes {
            if n.children.iter().any(|c| &c.tag == b"STRM") {
                out.push(&n.children);
            }
            groups(&n.children, out);
        }
    }
    let mut cells: Vec<(String, String, Vec<u8>)> = Vec::new();
    if input.is_dir() {
        for (ty, sub) in [("terrainmesh", "terrainmesh"), ("lowresterrain", "lowresterrain")] {
            let dir = input.join(sub);
            let mut files: Vec<_> = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
            files.sort_by_key(|f| f.file_name());
            for f in files {
                let body = std::fs::read(f.path()).map_err(|e| format!("{}: {e}", f.path().display()))?;
                cells.push((ty.to_string(), f.file_name().to_string_lossy().into_owned(), body));
            }
        }
    } else {
        for ((name, ty), bodies) in pc_chunks(input, |_, t| t == TYPE_HASH_TERRAIN_MESH || t == TYPE_HASH_LOWRES_TERRAIN)? {
            for (k, b) in bodies.into_iter().enumerate() {
                cells.push((type_name_from_hash(ty).to_string(), format!("0x{name:08X}.{k}"), b));
            }
        }
        cells.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    }
    let mut tsv = String::from("type\tcell\tgroups\tvertices\tmin_x\tmin_y\tmin_z\tmax_x\tmax_y\tmax_z\n");
    let mut ranges: BTreeMap<String, ([f32; 3], [f32; 3], [f32; 3], [f32; 3], usize)> = BTreeMap::new();
    for (ty, cell, body) in &cells {
        let tree = parse_ucfx_tree(body).map_err(|e| format!("{ty} {cell}: {e}"))?;
        let mut found = Vec::new();
        groups(&tree, &mut found);
        let (mut lo, mut hi, mut n) = ([f32::MAX; 3], [f32::MIN; 3], 0usize);
        for g in &found {
            for v in group_positions(g).map_err(|e| format!("{ty} {cell}: {e}"))? {
                n += 1;
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
        }
        let _ = writeln!(tsv, "{ty}\t{cell}\t{}\t{n}\t{}\t{}\t{}\t{}\t{}\t{}", found.len(), lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]);
        if n == 0 {
            continue;
        }
        let r = ranges.entry(ty.clone()).or_insert(([f32::MAX; 3], [f32::MIN; 3], [f32::MAX; 3], [f32::MIN; 3], 0));
        for k in 0..3 {
            r.0[k] = r.0[k].min(lo[k]);
            r.1[k] = r.1[k].max(lo[k]);
            r.2[k] = r.2[k].min(hi[k]);
            r.3[k] = r.3[k].max(hi[k]);
        }
        r.4 += 1;
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    for (ty, (min_lo, max_lo, min_hi, max_hi, n)) in &ranges {
        println!("{ty}: {n} cells; minimum corner from {min_lo:?} to {max_lo:?}; maximum corner from {min_hi:?} to {max_hi:?}");
    }
    Ok(())
}

/// `ucfx-tree`: every row of a little-endian container, with each leaf's bytes read three ways (hex,
/// u16, f32) up to `max` bytes, so a mis-swapped field stands out by its value.
pub fn ucfx_tree(file: &Path, max: usize) -> Result<(), String> {
    fn walk(nodes: &[UcfxNode], depth: usize, max: usize) {
        for n in nodes {
            let pad = "  ".repeat(depth);
            match &n.body {
                Some(b) => {
                    let shown = &b[..b.len().min(max)];
                    let hex: Vec<String> = shown.chunks(4).map(|c| c.iter().map(|x| format!("{x:02x}")).collect()).collect();
                    let u16s: Vec<String> = shown.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]).to_string()).collect();
                    let f32s: Vec<String> = shown.chunks_exact(4).map(|c| format!("{:.6e}", f32::from_le_bytes([c[0], c[1], c[2], c[3]]))).collect();
                    println!("{pad}{} {} bytes", n.tag_str(), b.len());
                    println!("{pad}  hex {}", hex.join(" "));
                    println!("{pad}  u16 {}", u16s.join(" "));
                    println!("{pad}  f32 {}", f32s.join(" "));
                }
                None => {
                    println!("{pad}{} ({} children)", n.tag_str(), n.children.len());
                    walk(&n.children, depth + 1, max);
                }
            }
        }
    }
    let body = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    walk(&parse_ucfx_tree(&body)?, 0, max);
    Ok(())
}

/// `water-at`: a watermap container's water at world `(x, z)` and the 5 × 5 cells around it, with the
/// grid's header and wet height range.
pub fn water_at(file: &Path, x: f32, z: f32) -> Result<(), String> {
    use mercs2_engine::water_sim::watermap::Watermap;
    let body = if file.extension().is_some_and(|x| x.eq_ignore_ascii_case("wad")) {
        let found = pc_chunks(file, |_, t| t == mercs2_formats::types::TYPE_HASH_WATERMAP)?;
        let mut bodies = found.into_values().flatten();
        let one = bodies.next().ok_or("the archive holds no watermap")?;
        if bodies.next().is_some() {
            return Err("the archive holds more than one watermap".into());
        }
        one
    } else {
        std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?
    };
    let watr = mercs2_formats::ucfx::extract_chunk_body(&body, b"watr").ok_or("no watr row")?;
    let m = Watermap::from_watr_bytes(&watr).map_err(|e| format!("{e:?}"))?;
    println!("{} x {} cells of {} m, origin ({}, {}), {} wet, wet heights {:?}", m.width(), m.height(), m.cell_size(), m.origin_x, m.origin_z, m.wet_cell_count(), m.wet_height_range());
    let s = m.sample(x, z);
    println!("at ({x}, {z}): cell {:?}, water {}, surface {}", m.cell_at(x, z), s.is_water, s.surface_height);
    let c = m.cell_size();
    for dz in -2..=2 {
        let row: Vec<String> = (-2..=2)
            .map(|dx| {
                let s = m.sample(x + dx as f32 * c, z + dz as f32 * c);
                format!("{}{:.1}", if s.is_water { "W" } else { "." }, s.surface_height)
            })
            .collect();
        println!("  {}", row.join("  "));
    }
    Ok(())
}

/// A half float's value.
fn half_value(h: u16) -> f32 {
    let (sign, exp, frac) = ((h >> 15) as u32, ((h >> 10) & 0x1F) as i32, (h & 0x3FF) as f32);
    let v = match exp {
        0 => frac * (2f32).powi(-24),
        0x1F => f32::NAN,
        e => (1.0 + frac / 1024.0) * (2f32).powi(e - 15),
    };
    if sign == 1 { -v } else { v }
}

/// One TINY stand-in's slot use: `[slots, triangles, triangles spanning two slots, vertices past the
/// list, vertices at a slot ≡ 3 (mod 4), vertices at no whole slot]`. A slot is a vertex's
/// `POSITION.w`; the shader keeps or drops each vertex by its own slot's state, so a triangle whose
/// vertices name two slots tears when the two states differ.
fn tiny_slot_use(container: &[u8]) -> Result<[usize; 6], String> {
    let m = mercs2_formats::tiny_model::TinyModel::decode(container)?;
    let n = m.slots.len();
    let mut out = [n, 0, 0, 0, 0, 0];
    for g in m.sub_objects.iter().flat_map(|s| &s.groups) {
        let slot = |i: u16| g.vertices.get(i as usize).map(|v| v.position[3]).ok_or_else(|| format!("index {i} past {} vertices", g.vertices.len()));
        for v in &g.vertices {
            let w = half_value(v.position[3]);
            if w.fract() != 0.0 || w < 0.0 {
                out[5] += 1;
            } else if w as usize >= n {
                out[3] += 1;
            } else if w as usize % 4 == 3 {
                out[4] += 1;
            }
        }
        for p in &g.prims {
            let start = p.start_index as usize;
            let run = g.strip.get(start..start + p.prim_count as usize + 2).ok_or("a PRMT record past its strip")?;
            for t in run.windows(3) {
                if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                    continue;
                }
                out[1] += 1;
                let (a, b, c) = (slot(t[0])?, slot(t[1])?, slot(t[2])?);
                if a != b || b != c {
                    out[2] += 1;
                }
            }
        }
    }
    Ok(out)
}

/// `tiny-slots`: every TINY stand-in of each input, how its vertices use the slot list. An input is a
/// PC archive, a directory of `.ucfx` containers, or (after `--be`) a big-endian archive whose models
/// are converted first. `out` has one row per stand-in; the totals print per input.
pub fn tiny_slots(inputs: &[String], out: &Path) -> Result<(), String> {
    use mercs2_formats::be_to_le::convert::convert_block;
    use mercs2_formats::be_to_le::texture::{BlockTables, TextureTable};
    use mercs2_formats::tiny_model::is_tiny_container;
    let mut tsv = String::from("input\tname\tslots\ttriangles\tspanning\tpast_list\tslot_three\tnot_whole\n");
    let mut big_endian = false;
    for input in inputs {
        if input == "--be" {
            big_endian = true;
            continue;
        }
        let path = Path::new(input);
        let mut containers: Vec<(String, Vec<u8>)> = Vec::new();
        if big_endian {
            let chunks = archive_chunks(path, |_, t| t == TYPE_HASH_MODEL)?;
            let textures = TextureTable::of_models(chunks.iter().map(|c| (c.name, &c.body[..])))?;
            let tables = BlockTables { textures: &textures, rung: None, streams: None };
            for c in &chunks {
                let mut one = Vec::with_capacity(20 + c.body.len());
                for w in [1u32, c.name, c.ty, 0, c.body.len() as u32] {
                    one.extend_from_slice(&w.to_be_bytes());
                }
                one.extend_from_slice(&c.body);
                let le = convert_block(&one, false, None, None, Some(tables)).map_err(|e| format!("0x{:08X}: {e}", c.name))?;
                let (_, range) = mercs2_formats::ucfx::block_chunk_spans(&le)?.into_iter().next().ok_or("no chunk")?;
                if is_tiny_container(&le[range.clone()]) {
                    containers.push((format!("0x{:08X}", c.name), le[range].to_vec()));
                }
            }
            big_endian = false;
        } else if path.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
            files.sort_by_key(|f| f.file_name());
            for f in files {
                let p = f.path();
                if p.extension().is_some_and(|x| x == "ucfx") {
                    let body = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                    if is_tiny_container(&body) {
                        containers.push((f.file_name().to_string_lossy().into_owned(), body));
                    }
                }
            }
        } else {
            for ((name, _), bodies) in pc_chunks(path, |_, t| t == TYPE_HASH_MODEL)? {
                for body in bodies.into_iter().filter(|b| is_tiny_container(b)) {
                    containers.push((format!("0x{name:08X}"), body));
                }
            }
            containers.sort_by(|a, b| a.0.cmp(&b.0));
        }
        let mut total = [0usize; 6];
        let mut pixel_shaders: BTreeMap<u32, usize> = BTreeMap::new();
        for (name, body) in &containers {
            let m = mercs2_formats::tiny_model::TinyModel::decode(body).map_err(|e| format!("{input}: {name}: {e}"))?;
            for mat in &m.materials {
                *pixel_shaders.entry(mat.pixel_shader).or_default() += 1;
            }
            let u = tiny_slot_use(body).map_err(|e| format!("{input}: {name}: {e}"))?;
            let _ = writeln!(tsv, "{input}\t{name}\t{}\t{}\t{}\t{}\t{}\t{}", u[0], u[1], u[2], u[3], u[4], u[5]);
            for k in 1..6 {
                total[k] += u[k];
            }
            total[0] += usize::from(u[2] > 0);
        }
        println!(
            "{input}: {} stand-ins, {} with a spanning triangle; {} triangles, {} spanning; vertices: {} past the list, {} at a slot ≡ 3, {} at no whole slot",
            containers.len(), total[0], total[1], total[2], total[3], total[4], total[5]
        );
        let named: Vec<String> = pixel_shaders.iter().map(|(h, n)| format!("0x{h:08X} ×{n}")).collect();
        println!("  material pixel shaders: {}", named.join(", "));
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))
}

/// Every (name, type) of a little-endian PC archive.
fn pc_keys(wad: &Path) -> Result<HashSet<(u32, u32)>, String> {
    let mut f = std::fs::File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let pc = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let mut keys = HashSet::new();
    for b in 0..pc.indx.len() {
        let dec = mercs2_formats::sges::decompress_block(&mut f, &pc.indx, b as u16).map_err(|e| format!("pc block {b}: {e}"))?;
        for (e, _) in mercs2_formats::ucfx::block_chunk_spans(&dec).map_err(|e| format!("pc block {b}: {e}"))? {
            keys.insert((e.name_hash, e.type_hash));
        }
    }
    Ok(keys)
}

pub fn dlc_census(package: &Path, base: &Path, pc: &Path, out: &Path) -> Result<(), String> {
    let chunks = archive_chunks(package, |_, _| true)?;
    let keys: HashSet<(u32, u32)> = chunks.iter().map(|c| (c.name, c.ty)).collect();
    let mut known: HashMap<(u32, u32), Vec<Chunk>> = HashMap::new();
    for c in archive_chunks(base, |n, t| keys.contains(&(n, t)))? {
        known.entry((c.name, c.ty)).or_default().push(c);
    }
    let on_pc = pc_keys(pc)?;
    let mut tsv = String::from("class\ttype\tname\tsize\tbase_sizes\tdiffers\tdiff_bytes\tmean_abs\tfirst_diff\tblock\tbase_block\ton_pc\n");
    let mut totals: BTreeMap<String, [(usize, usize); 3]> = BTreeMap::new();
    let mut bytes_log = String::new();
    for c in &chunks {
        let twins = known.get(&(c.name, c.ty));
        let (class, row, diff, bytes, home) = match twins {
            None => ("new", 0, String::new(), "\t\t".to_string(), String::new()),
            Some(t) => match t.iter().find(|b| b.body == c.body) {
                Some(b) => ("same", 1, String::new(), "\t\t".to_string(), b.block.clone()),
                None => {
                    let best = t
                        .iter()
                        .map(|b| (differing(&c.body, &b.body), b))
                        .min_by_key(|(d, b)| (b.body.len() != c.body.len(), d.len()))
                        .ok_or("a key with no chunks")?;
                    if c.body.len() == best.1.body.len() && c.ty != TYPE_TEXTURE {
                        writeln!(bytes_log, "0x{:08X}\t{}\t{}", c.name, type_name_from_hash(c.ty), byte_list(&c.body, &best.1.body, 16)).map_err(|e| e.to_string())?;
                    }
                    ("changed", 2, best.0.join(","), byte_delta(&c.body, &best.1.body), best.1.block.clone())
                }
            },
        };
        let sizes = twins.map_or(String::new(), |t| t.iter().map(|b| b.body.len().to_string()).collect::<Vec<_>>().join(","));
        let ty = type_name_from_hash(c.ty).to_string();
        let pc_side = on_pc.contains(&(c.name, c.ty));
        writeln!(tsv, "{class}\t{ty}\t0x{:08X}\t{}\t{sizes}\t{diff}\t{bytes}\t{}\t{home}\t{pc_side}", c.name, c.body.len(), c.block).map_err(|e| e.to_string())?;
        let t = &mut totals.entry(ty).or_default()[row];
        t.0 += 1;
        t.1 += c.body.len();
    }
    std::fs::write(out, tsv).map_err(|e| format!("write {}: {e}", out.display()))?;
    let bytes_out = out.with_extension("bytes.tsv");
    std::fs::write(&bytes_out, bytes_log).map_err(|e| format!("write {}: {e}", bytes_out.display()))?;
    println!("{:<28}{:>8}{:>12}{:>8}{:>12}{:>8}{:>12}", "type", "new", "bytes", "same", "bytes", "changed", "bytes");
    for (ty, [n, s, c]) in &totals {
        println!("{ty:<28}{:>8}{:>12}{:>8}{:>12}{:>8}{:>12}", n.0, n.1, s.0, s.1, c.0, c.1);
    }
    println!("{} chunks in the package", chunks.len());
    Ok(())
}
