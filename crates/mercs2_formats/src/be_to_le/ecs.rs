//! ECS containers (`worldentity`, layers), big-endian to little-endian, at the widths they are read.
//!
//! CHDR bit 2 (`FUN_00654940`, `DAT_01176053`) picks the form of a container. Clear, it describes
//! itself: named `info`, `schm`, `enum`, and `Name` records hold strings. Set, as in every console
//! `worldentity`, it holds neither `schm` nor `enum`: `info` is `[u32 hash][u32 version][u32 count]
//! [u32 flags]`, `Name` records hold a u32 hash, and the layouts are the ones compiled into the exe
//! (native table `0x017C0B58`, keyed by that hash). The PC bake writes the same layouts down as
//! `schm`, so [`ComponentSchemas`] reads them from the player's `vz.wad`.
//!
//! The form comes from CHDR and nothing else. A compact `info` is not told from a named one by its
//! bytes: `EB A0 9B 1A` is valid UTF-8 and is the hash of `AiSkill`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::Path;

use super::convert::{
    convert_chdr_body_inplace, convert_ecs_container, convert_enum_body_inplace, swap_schm_body_inplace,
    swap_u16, swap_u32, swap_u32_array, Descriptor,
};
use super::report::SchemaCoverageReport;
use crate::dlc_input::{parse_be_ffcs, parse_be_indx, read_be_block};
use crate::dlc_stfs::load_stfs_or_doh;
use crate::ffcs::{load_ffcs_archive, read_u32_be, read_u32_le};
use crate::hash::pandemic_hash_m2;
use crate::schema::{ComponentSchema, SchemaField};
use crate::sges::decompress_block;
use crate::tags::ChunkTag;
use crate::types::{
    TYPE_HASH_GUIDMAP, TYPE_HASH_LAYER, TYPE_HASH_WORLD_ENTITY_DATA, TYPE_ID_LAYER, TYPE_ID_WORLD_ENTITY_DATA,
};
use crate::ucfx::{block_chunk_spans, parse_ucfx_tree, write_ucfx_tree, UcfxNode};
use crate::worldentity::{layout_for, Layout, CHDR_GROUPED, CHDR_NAMES_AS_HASHES, FLAGS_BYTES};

/// Size of a compact `info`.
const COMPACT_INFO_BYTES: usize = 16;

/// The class of a container's names.
const NAME_CLASS: &str = "Name";
/// The class whose records hold a stride of their own: 42 bytes when the CHDR gate is at least
/// `0x2A`, else 40 (`0x0063D7C0`), whatever its `schm` declares.
const TRANSFORM_CLASS: &str = "Transform";
/// The lowest CHDR gate value that gives Transform its 6-byte tail.
const TRANSFORM_GATE: i16 = 0x2A;

/// A class as the PC bake describes it.
#[derive(Debug, Clone)]
pub struct ClassSchema {
    pub name: String,
    pub schema: ComponentSchema,
}

/// The layouts a compact container leaves to the exe, by class hash and `info` version, with what
/// else the PC bake still writes down: its `enum` tables and the strings of its `Name` records.
#[derive(Debug, Default)]
pub struct ComponentSchemas {
    classes: HashMap<(u32, u32), ClassSchema>,
    enums: Vec<u8>,
    names: HashMap<u32, String>,
}

/// Whether a little-endian ECS container is compact, by its first row, CHDR.
pub fn is_compact(container: &[u8]) -> bool {
    let (dao, is_chdr) = match (container.get(4..8), container.get(20..24)) {
        (Some(d), Some(t)) => (u32::from_le_bytes([d[0], d[1], d[2], d[3]]) as usize, t == b"CHDR"),
        _ => return false,
    };
    let size = container.get(28..32).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    is_chdr && size == 8 && container.get(dao + 4..dao + 8).is_some_and(|f| f[0] as u32 & CHDR_NAMES_AS_HASHES != 0)
}

impl ComponentSchemas {
    /// Every class of a PC `vz.wad`: its `worldentity` and all its layers.
    pub fn from_vz_wad(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let size = file.metadata().map_err(|e| format!("{}: {e}", path.display()))?.len();
        let archive =
            load_ffcs_archive(&mut file, size).map_err(|e| format!("{}: {e}", path.display()))?;
        let blocks: BTreeSet<u16> = archive
            .aset
            .iter()
            .filter(|e| e.type_id == TYPE_ID_LAYER || e.type_id == TYPE_ID_WORLD_ENTITY_DATA)
            .map(|e| e.block_index())
            .collect();
        let mut out = Self::default();
        let mut worldentities = 0usize;
        for b in blocks {
            let dec = decompress_block(&mut file, &archive.indx, b).map_err(|e| format!("block {b}: {e}"))?;
            for (entry, range) in block_chunk_spans(&dec).map_err(|e| format!("block {b}: {e}"))? {
                if entry.type_hash == TYPE_HASH_WORLD_ENTITY_DATA {
                    worldentities += 1;
                } else if entry.type_hash != TYPE_HASH_LAYER {
                    continue;
                }
                if is_compact(&dec[range.clone()]) {
                    return Err(format!("{}: a compact bake has no schm to read", path.display()));
                }
                out.add_container(&dec[range])
                    .map_err(|e| format!("block {b} 0x{:08X}: {e}", entry.name_hash))?;
            }
        }
        if worldentities == 0 {
            return Err(format!("{}: no worldentity, so this is not a PC vz.wad", path.display()));
        }
        Ok(out)
    }

    /// Add every `COMP` group of a little-endian named container, with its `enum` table and the
    /// strings of its `Name` records. A class seen twice at one version must declare the same bytes.
    pub fn add_container(&mut self, container: &[u8]) -> Result<(), String> {
        let tree = parse_ucfx_tree(container)?;
        let chdr = tree.iter().find(|n| &n.tag == b"CHDR").and_then(|n| n.body.as_deref()).ok_or("no CHDR")?;
        let (flags, gate) = (read_u32_le(chdr, 4), i16::from_le_bytes([chdr[2], chdr[3]]));
        if let Some(e) = tree.iter().find(|n| &n.tag == b"enum").and_then(|n| n.body.as_deref()) {
            self.enums = e.to_vec();
        }
        for node in tree.iter().filter(|n| &n.tag == b"COMP") {
            let body = |tag: &[u8; 4]| {
                node.children
                    .iter()
                    .find(|c| &c.tag == tag)
                    .and_then(|c| c.body.as_deref())
                    .ok_or_else(|| format!("a COMP group has no {:?}", String::from_utf8_lossy(tag)))
            };
            let info = body(b"info")?;
            let nul = info.iter().position(|&b| b == 0).ok_or("info has no name")?;
            let name = std::str::from_utf8(&info[..nul]).map_err(|e| format!("info name: {e}"))?;
            if info.len() != nul + 1 + COMPACT_INFO_BYTES {
                return Err(format!("{name}: info is {} bytes", info.len()));
            }
            let hash = read_u32_le(info, nul + 1);
            let version = read_u32_le(info, nul + 5);
            let schm = body(b"schm")?;
            let schema = ComponentSchema::from_schm_body(schm, false)
                .ok_or_else(|| format!("{name}: schm does not parse"))?;
            if name == NAME_CLASS {
                let data = body(b"data")?;
                let shape = GroupShape::new(name, version, &schema, read_u32_le(info, nul + 9), flags, gate)?;
                for r in record_spans(data, false, &shape)? {
                    let p = &data[r.payload];
                    let s = std::str::from_utf8(&p[..p.len() - 2]).map_err(|e| format!("Name: {e}"))?;
                    self.names.entry(pandemic_hash_m2(s)).or_insert_with(|| s.to_string());
                }
            }
            match self.classes.get(&(hash, version)) {
                Some(have) if have.schema.to_schm_body() != schema.to_schm_body() => {
                    return Err(format!("{name} v{version}: two different schm"));
                }
                Some(_) => {}
                None => {
                    self.classes.insert((hash, version), ClassSchema { name: name.to_string(), schema });
                }
            }
        }
        Ok(())
    }

    /// Add names known from elsewhere, each under its hash. A name the PC bake wrote stays.
    pub fn add_names(&mut self, names: impl IntoIterator<Item = String>) {
        for n in names {
            self.names.entry(pandemic_hash_m2(&n)).or_insert(n);
        }
    }

    /// A compact little-endian container in the named form: `info` with its class name, `schm`, the
    /// PC `enum` table, and each `Name` record's string, or `0xHHHHHHHH` when no string for its hash
    /// is known. A reader of the named form reads it unchanged; the game reads the compact bytes.
    pub fn inflate(&self, container: &[u8]) -> Result<Vec<u8>, String> {
        let mut roots = Vec::new();
        let tree = parse_ucfx_tree(container)?;
        let chdr = tree.iter().find(|n| &n.tag == b"CHDR").and_then(|n| n.body.as_deref()).ok_or("no CHDR")?;
        let (flags, gate) = (read_u32_le(chdr, 4), i16::from_le_bytes([chdr[2], chdr[3]]));
        for node in tree {
            match &node.tag {
                b"CHDR" => {
                    let mut b = node.body.clone().ok_or("CHDR has no body")?;
                    b[4..8].copy_from_slice(&(flags & !CHDR_NAMES_AS_HASHES).to_le_bytes());
                    roots.push(UcfxNode::leaf(*b"CHDR", b));
                    roots.push(UcfxNode::leaf(*b"enum", self.enums.clone()));
                }
                b"COMP" => {
                    let body = |tag: &[u8; 4]| {
                        node.children.iter().find(|c| &c.tag == tag).and_then(|c| c.body.as_deref())
                    };
                    let info = body(b"info").ok_or("COMP has no info")?;
                    let data = body(b"data").ok_or("COMP has no data")?;
                    let (hash, version, count) = (read_u32_le(info, 0), read_u32_le(info, 4), read_u32_le(info, 8));
                    let class = self.get(hash, version).ok_or_else(|| format!("class 0x{hash:08X} v{version}: no schm"))?;
                    let mut named = class.name.as_bytes().to_vec();
                    named.push(0);
                    named.extend_from_slice(info);
                    let shape = GroupShape::new(&class.name, version, &class.schema, count, flags, gate)?;
                    let data = match shape.payload {
                        RecordPayload::NameHash => self.named_records(data, &shape)?,
                        _ => data.to_vec(),
                    };
                    roots.push(UcfxNode::marker(
                        *b"COMP",
                        vec![
                            UcfxNode::leaf(*b"info", named),
                            UcfxNode::leaf(*b"schm", class.schema.to_schm_body()),
                            UcfxNode::leaf(*b"data", data),
                        ],
                    ));
                }
                _ => roots.push(node),
            }
        }
        Ok(write_ucfx_tree(&roots))
    }

    /// `Name` records of hashes as records of strings.
    fn named_records(&self, data: &[u8], shape: &GroupShape) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(data.len() * 3);
        let mut at = 0;
        for r in record_spans(data, false, shape)? {
            out.extend_from_slice(&data[at..r.payload.start]);
            let h = read_u32_le(data, r.payload.start);
            match self.names.get(&h) {
                Some(s) => out.extend_from_slice(s.as_bytes()),
                None => out.extend_from_slice(format!("0x{h:08X}").as_bytes()),
            }
            out.extend_from_slice(&[0, 0]);
            at = r.payload.end;
        }
        Ok(out)
    }

    /// The class at a hash and `info` version.
    pub fn get(&self, hash: u32, version: u32) -> Option<&ClassSchema> {
        self.classes.get(&(hash, version))
    }

    /// The name of a class at any version.
    pub fn name_of(&self, hash: u32) -> Option<&str> {
        self.classes.iter().find(|((h, _), _)| *h == hash).map(|(_, c)| c.name.as_str())
    }

    /// How many (class, version) pairs are held.
    pub fn len(&self) -> usize {
        self.classes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }
}

/// How one record's payload is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordPayload {
    /// `payload_stride` bytes, each `schm` field at its offset.
    Fixed,
    /// `[cstring name][u8]`.
    NameString,
    /// `[u32 hash]`.
    NameHash,
    /// `[32 bytes][cstring]`.
    PointLocation,
    /// One u16 holding every field as bits.
    PackedU16,
    /// Nine u32 words, then a u16 when the record has the tail.
    Transform { tail: bool },
}

/// The shape of a group's `data`.
#[derive(Debug, Clone, Copy)]
pub struct GroupShape<'a> {
    pub class: &'a str,
    pub schema: &'a ComponentSchema,
    pub count: u32,
    /// CHDR bit 0: `[u32 n][n keys]` rather than one key.
    pub grouped: bool,
    pub payload: RecordPayload,
}

impl<'a> GroupShape<'a> {
    /// Read a group's shape from its class, `info` version and the container's CHDR.
    pub fn new(
        class: &'a str,
        version: u32,
        schema: &'a ComponentSchema,
        count: u32,
        chdr_flags: u32,
        gate: i16,
    ) -> Result<Self, String> {
        let hashed = chdr_flags & CHDR_NAMES_AS_HASHES != 0;
        let payload = if class == TRANSFORM_CLASS {
            RecordPayload::Transform { tail: gate >= TRANSFORM_GATE }
        } else {
            match layout_for(class, version, schema)? {
                Layout::Fixed => RecordPayload::Fixed,
                Layout::Name if hashed => RecordPayload::NameHash,
                Layout::Name => RecordPayload::NameString,
                Layout::PointLocation => RecordPayload::PointLocation,
                Layout::PackedU16 => RecordPayload::PackedU16,
            }
        };
        Ok(GroupShape { class, schema, count, grouped: chdr_flags & CHDR_GROUPED != 0, payload })
    }
}

/// Where one record's parts sit in a `data` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSpan {
    /// The key words, without a grouped record's leading count.
    pub keys: Range<usize>,
    pub payload: Range<usize>,
}

fn cstring_end(data: &[u8], from: usize, class: &str) -> Result<usize, String> {
    let rest = data.get(from..).ok_or_else(|| format!("{class}: record runs past the data"))?;
    let nul = rest.iter().position(|&b| b == 0).ok_or_else(|| format!("{class}: unterminated string"))?;
    Ok(from + nul + 1)
}

/// Walk a `data` body into records, which must consume it exactly.
pub fn record_spans(data: &[u8], big_endian: bool, g: &GroupShape) -> Result<Vec<RecordSpan>, String> {
    let read = if big_endian { read_u32_be } else { read_u32_le };
    let class = g.class;
    let need = |at: usize| -> Result<usize, String> {
        if at > data.len() {
            return Err(format!(
                "{class}: a record ends at {at}, the {} records' data is {} bytes",
                g.count,
                data.len()
            ));
        }
        Ok(at)
    };
    let mut out = Vec::with_capacity(g.count as usize);
    let mut p = 0usize;
    for _ in 0..g.count {
        let keys = if g.grouped {
            need(p + 4)?;
            let n = read(data, p) as usize;
            if n == 0 {
                return Err(format!("{class}: record at {p} has no keys"));
            }
            need(p + 4 + n * 4)?;
            p + 4..p + 4 + n * 4
        } else {
            need(p + 4)?;
            p..p + 4
        };
        p = keys.end;
        let end = match g.payload {
            RecordPayload::Fixed => need(p + g.schema.payload_stride as usize)?,
            RecordPayload::NameString => need(cstring_end(data, p, class)? + 1)?,
            RecordPayload::NameHash => need(p + 4)?,
            RecordPayload::PointLocation => cstring_end(data, need(p + 32)?, class)?,
            RecordPayload::PackedU16 => need(p + 2)?,
            RecordPayload::Transform { tail } => need(p + if tail { 38 } else { 36 })?,
        };
        out.push(RecordSpan { keys, payload: p..end });
        p = end;
    }
    if p != data.len() {
        return Err(format!("{class}: {} records end at {p}, data is {} bytes", g.count, data.len()));
    }
    Ok(out)
}

/// One class's records in a container: its name, and each key's payload. A `Name` record's payload
/// is its hash in both forms, so a compact container compares with a named one.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ClassRecords {
    pub class: String,
    pub by_key: HashMap<u32, Vec<u8>>,
}

/// A little-endian ECS container of either form, read into records.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ContainerRecords {
    pub flags: u32,
    pub gate: i16,
    pub classes: BTreeMap<u32, ClassRecords>,
    /// Every row outside a `COMP` group, by tag.
    pub rows: BTreeMap<String, Vec<u8>>,
    /// The strings of a named container's `Name` records, by key.
    pub names: HashMap<u32, String>,
}

/// Read a little-endian ECS container of either form. A compact one needs `schemas`.
pub fn read_records(container: &[u8], schemas: &ComponentSchemas) -> Result<ContainerRecords, String> {
    let mut out = ContainerRecords::default();
    for node in parse_ucfx_tree(container)? {
        if &node.tag == b"CHDR" {
            let b = node.body.as_deref().ok_or("CHDR has no body")?;
            out.gate = i16::from_le_bytes([b[2], b[3]]);
            out.flags = read_u32_le(b, 4);
        }
        if &node.tag != b"COMP" {
            if let Some(b) = &node.body {
                out.rows.insert(node.tag_str(), b.clone());
            }
            continue;
        }
        let body = |tag: &[u8; 4]| node.children.iter().find(|c| &c.tag == tag).and_then(|c| c.body.as_deref());
        let info = body(b"info").ok_or("COMP has no info")?;
        let data = body(b"data").ok_or("COMP has no data")?;
        let (name, hash, at, schema) = match body(b"schm") {
            Some(s) => {
                let nul = info.iter().position(|&b| b == 0).ok_or("info has no name")?;
                let name = String::from_utf8_lossy(&info[..nul]).into_owned();
                let schema = ComponentSchema::from_schm_body(s, false).ok_or_else(|| format!("{name}: schm"))?;
                (name, read_u32_le(info, nul + 1), nul + 5, schema)
            }
            None => {
                let (hash, version) = (read_u32_le(info, 0), read_u32_le(info, 4));
                let c = schemas.get(hash, version).ok_or_else(|| format!("class 0x{hash:08X} v{version}: no schm"))?;
                (c.name.clone(), hash, 4, c.schema.clone())
            }
        };
        let (version, count) = (read_u32_le(info, at), read_u32_le(info, at + 4));
        let shape = GroupShape::new(&name, version, &schema, count, out.flags, out.gate)?;
        let spans = record_spans(data, false, &shape)?;
        let class = out.classes.entry(hash).or_default();
        for r in spans {
            let p = &data[r.payload];
            let name = match shape.payload {
                RecordPayload::NameString => {
                    Some(std::str::from_utf8(&p[..p.len() - 2]).map_err(|e| format!("Name: {e}"))?)
                }
                _ => None,
            };
            let value = name.map_or_else(|| p.to_vec(), |s| pandemic_hash_m2(s).to_le_bytes().to_vec());
            for k in data[r.keys].chunks_exact(4) {
                let k = u32::from_le_bytes([k[0], k[1], k[2], k[3]]);
                class.by_key.insert(k, value.clone());
                if let Some(s) = name {
                    out.names.insert(k, s.to_string());
                }
            }
        }
        class.class = name;
    }
    Ok(out)
}

/// Every ECS container of an archive, little-endian, by (name, type): a PC `FFCS` archive as stored,
/// a big-endian one or an Xbox package converted.
pub fn ecs_containers(path: &Path, schemas: &ComponentSchemas) -> Result<BTreeMap<(u32, u32), Vec<u8>>, String> {
    const ECS_TYPES: [u32; 3] = [TYPE_HASH_WORLD_ENTITY_DATA, TYPE_HASH_LAYER, TYPE_HASH_GUIDMAP];
    let at = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut file = File::open(path).map_err(at)?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).map_err(at)?;
    let mut out = BTreeMap::new();
    if &magic == b"FFCS" {
        let size = file.metadata().map_err(at)?.len();
        let archive = load_ffcs_archive(&mut file, size).map_err(|e| format!("{}: {e}", path.display()))?;
        for b in 0..archive.indx.len() {
            let dec = decompress_block(&mut file, &archive.indx, b as u16).map_err(|e| format!("block {b}: {e}"))?;
            for (e, r) in block_chunk_spans(&dec).map_err(|e| format!("block {b}: {e}"))? {
                if ECS_TYPES.contains(&e.type_hash) {
                    out.insert((e.name_hash, e.type_hash), dec[r].to_vec());
                }
            }
        }
        return Ok(out);
    }
    let (doh, _) = load_stfs_or_doh(path)?;
    let (_, rows) = parse_be_ffcs(&doh)?;
    let indx = rows.iter().find(|r| r.tag == "INDX").ok_or("no INDX")?;
    for (bi, e) in parse_be_indx(&doh, indx.offset as usize, indx.meta as usize).iter().enumerate() {
        let b = read_be_block(&doh, e).map_err(|err| format!("block {bi}: {err}"))?;
        let n = read_u32_be(&b, 0) as usize;
        let mut off = 4 + n * 16;
        for i in 0..n {
            let row = 4 + i * 16;
            let (name, ty, size) = (read_u32_be(&b, row), read_u32_be(&b, row + 4), read_u32_be(&b, row + 12) as usize);
            let c = b.get(off..off + size).ok_or_else(|| format!("block {bi} entry {i} runs past the block"))?;
            off += size;
            if ECS_TYPES.contains(&ty) {
                let le = convert_ecs_container(c, ty, Some(schemas)).map_err(|err| format!("block {bi} 0x{name:08X}: {err}"))?;
                out.insert((name, ty), le);
            }
        }
    }
    Ok(out)
}

/// Rewrite the u16 of a [`RecordPayload::PackedU16`]. `schm` declares the PC's bit positions. A field
/// at bit `s` of width `w` sits at bit `16 - s - w` on console, and keeps its value. A set bit no
/// field declares fails.
fn mirror_bit_unit(unit: &mut [u8], fields: &[&SchemaField], class: &str) -> Result<(), String> {
    let v = unit.iter().fold(0u32, |v, &b| v << 8 | b as u32);
    let bits = unit.len() as u32 * 8;
    let (mut out, mut used) = (0u32, 0u32);
    for f in fields {
        let (start, width) = (f.bit_start as u32, f.bit_width as u32);
        let mask = ((1u64 << width) - 1) as u32;
        let console_start = bits - start - width;
        out |= ((v >> console_start) & mask) << start;
        used |= mask << console_start;
    }
    if v & !used != 0 {
        return Err(format!("{class}: unit 0x{v:08X} sets bits no schm field declares"));
    }
    for (i, b) in unit.iter_mut().enumerate() {
        *b = (out >> (8 * i)) as u8;
    }
    Ok(())
}

/// Swap one fixed payload field by field, and rewrite each unit of bit fields once.
fn swap_fixed_payload(p: &mut [u8], schema: &ComponentSchema, class: &str) -> Result<(), String> {
    let mut units: BTreeMap<u16, Vec<&SchemaField>> = BTreeMap::new();
    for f in &schema.fields {
        if f.bit_width > 0 {
            units.entry(f.byte_offset).or_default().push(f);
            continue;
        }
        if !f.field_type.needs_swap() {
            continue;
        }
        let unit = f.field_type.swap_unit();
        for i in 0..f.field_type.swap_count() {
            let at = f.byte_offset as usize + i * unit;
            match unit {
                2 => swap_u16(p, at),
                _ => swap_u32(p, at),
            }
        }
    }
    for (offset, fields) in units {
        let at = offset as usize;
        reverse_bit_unit(&mut p[at..at + fields[0].field_type.byte_width()], &fields, class)?;
    }
    Ok(())
}

/// Rewrite one unit of a fixed record's bit fields. A console bake stores the whole unit
/// bit-reversed against the PC's, so a multi-bit field's value reverses as well: `WeaponHint`'s
/// 2-bit fields hold 1 on console where PC holds 2. A set bit no field declares fails.
fn reverse_bit_unit(unit: &mut [u8], fields: &[&SchemaField], class: &str) -> Result<(), String> {
    let v = unit.iter().fold(0u32, |v, &b| v << 8 | b as u32);
    let bits = unit.len() as u32 * 8;
    let out = v.reverse_bits() >> (32 - bits);
    let declared = fields.iter().fold(0u32, |m, f| m | (((1u64 << f.bit_width) - 1) as u32) << f.bit_start);
    if out & !declared != 0 {
        return Err(format!("{class}: unit 0x{v:08X} sets bits no schm field declares"));
    }
    for (i, b) in unit.iter_mut().enumerate() {
        *b = (out >> (8 * i)) as u8;
    }
    Ok(())
}

/// Swap a whole `data` body in place.
fn swap_data(data: &mut [u8], g: &GroupShape) -> Result<(), String> {
    for r in record_spans(data, true, g)? {
        let head = if g.grouped { r.keys.start - 4 } else { r.keys.start };
        swap_u32_array(&mut data[head..r.keys.end]);
        let payload = &mut data[r.payload.clone()];
        match g.payload {
            RecordPayload::Fixed => swap_fixed_payload(payload, g.schema, g.class)?,
            RecordPayload::NameString => {}
            RecordPayload::NameHash => swap_u32(payload, 0),
            RecordPayload::PointLocation => swap_u32_array(&mut payload[..32]),
            RecordPayload::PackedU16 => {
                let fields: Vec<&SchemaField> = g.schema.fields.iter().collect();
                mirror_bit_unit(&mut payload[..2], &fields, g.class)?;
            }
            RecordPayload::Transform { tail } => {
                swap_u32_array(&mut payload[..36]);
                if tail {
                    swap_u16(payload, 36);
                }
            }
        }
    }
    Ok(())
}

/// `flgt`: one u32 in a layer; in a `worldentity`, `[u32 n][n × (u32 hash, cstring class)][u32]`.
fn swap_flgt(b: &mut [u8]) -> Result<(), String> {
    if b.len() < 4 {
        return Err(format!("flgt is {} bytes", b.len()));
    }
    if b.len() == 4 {
        swap_u32(b, 0);
        return Ok(());
    }
    let n = read_u32_be(b, 0) as usize;
    swap_u32(b, 0);
    let mut p = 4;
    for i in 0..n {
        if p + 4 > b.len() {
            return Err(format!("flgt entry {i} of {n} runs past its {} bytes", b.len()));
        }
        swap_u32(b, p);
        p = cstring_end(b, p + 4, "flgt")?;
    }
    swap_u32_array(&mut b[p..]);
    if !(b.len() - p).is_multiple_of(4) {
        return Err(format!("flgt: {} bytes after its {n} entries", b.len() - p));
    }
    Ok(())
}

/// `flgs`: `[u32 n][n × (u32 key, u64 words of bitset)]`.
fn swap_flgs(b: &mut [u8]) -> Result<(), String> {
    if b.len() < 4 {
        return Err(format!("flgs is {} bytes", b.len()));
    }
    let n = read_u32_be(b, 0) as usize;
    if b.len() != 4 + n * (4 + FLAGS_BYTES) {
        return Err(format!("flgs is {} bytes for {n} records", b.len()));
    }
    swap_u32(b, 0);
    for rec in b[4..].chunks_exact_mut(4 + FLAGS_BYTES) {
        swap_u32(rec, 0);
        rec[4..].chunks_exact_mut(8).for_each(<[u8]>::reverse);
    }
    Ok(())
}

/// A guidmap's `CHDR` body, as `PgGuidMap`'s reader takes it (PS3 `FUN_001EDDB0`): `{ u16 version,
/// u16, u32 a, u32 b, u8, u8, u16, u32 }`, then `a` u32, `a` u8, `b` u32, `b` u16.
fn swap_guidmap(b: &mut [u8]) -> Result<(), String> {
    const HEADER: usize = 20;
    if b.len() < HEADER {
        return Err(format!("guidmap is {} bytes", b.len()));
    }
    let (a, n) = (read_u32_be(b, 4) as usize, read_u32_be(b, 8) as usize);
    if b.len() != HEADER + 5 * a + 6 * n {
        return Err(format!("guidmap is {} bytes for {a} and {n} entries", b.len()));
    }
    swap_u16(b, 0);
    swap_u16(b, 2);
    swap_u32(b, 4);
    swap_u32(b, 8);
    swap_u16(b, 14);
    swap_u32(b, 16);
    swap_u32_array(&mut b[HEADER..HEADER + 4 * a]);
    let words = HEADER + 5 * a;
    swap_u32_array(&mut b[words..words + 4 * n]);
    b[words + 4 * n..].chunks_exact_mut(2).for_each(<[u8]>::reverse);
    Ok(())
}

/// Swap a big-endian guidmap's data area: one `CHDR` row holding the whole map.
pub(super) fn convert_guidmap_body(data: &mut [u8], rows: &[Descriptor]) -> Result<(), String> {
    match rows {
        [row] if row.tag == ChunkTag::Chdr => swap_guidmap(body_mut(data, row)?),
        _ => Err(format!("a guidmap holds one CHDR row, this one has {} rows", rows.len())),
    }
}

fn body_mut<'a>(data: &'a mut [u8], d: &Descriptor) -> Result<&'a mut [u8], String> {
    let start = d.row_u0 as usize;
    data.get_mut(start..start + d.body_size as usize)
        .ok_or_else(|| format!("row {:?} runs past the data area", d.tag))
}

/// Swap one `COMP` group: its rows are `info`, `data` in the compact form and `info`, `schm`, `data`
/// in the named one.
fn swap_group(
    data: &mut [u8],
    kids: &[Descriptor],
    chdr_flags: u32,
    gate: i16,
    schemas: Option<&ComponentSchemas>,
    report: Option<&mut SchemaCoverageReport>,
) -> Result<(), String> {
    let tags: Vec<ChunkTag> = kids.iter().map(|d| d.tag).collect();
    let compact = chdr_flags & CHDR_NAMES_AS_HASHES != 0;
    let want: &[ChunkTag] = if compact {
        &[ChunkTag::Info, ChunkTag::Data]
    } else {
        &[ChunkTag::Info, ChunkTag::Schm, ChunkTag::Data]
    };
    if tags != want {
        return Err(format!("CHDR flags 0x{chdr_flags:08X} want a COMP of {want:?}, this one is {tags:?}"));
    }
    let mut report = report;
    let data_row = &kids[kids.len() - 1];
    if compact {
        let info = body_mut(data, &kids[0])?;
        if info.len() != COMPACT_INFO_BYTES {
            return Err(format!("compact info is {} bytes", info.len()));
        }
        swap_u32_array(info);
        let (hash, version) = (read_u32_le(info, 0), read_u32_le(info, 4));
        let count = read_u32_le(info, 8);
        let table = schemas.ok_or_else(|| {
            format!("class 0x{hash:08X}: a compact container needs the schemas of a PC vz.wad")
        })?;
        let class = table.get(hash, version).ok_or_else(|| match table.name_of(hash) {
            Some(n) => format!("{n}: info version {version} has no schm in the PC bake"),
            None => format!("class 0x{hash:08X}: no schm in the PC bake"),
        })?;
        if let Some(r) = report.as_deref_mut() {
            class.schema.fields.iter().for_each(|f| r.record_field(f.field_type as u32));
        }
        let g = GroupShape::new(&class.name, version, &class.schema, count, chdr_flags, gate)?;
        return swap_data(body_mut(data, data_row)?, &g);
    }
    let info = body_mut(data, &kids[0])?;
    let nul = info.iter().position(|&b| b == 0).ok_or("named info has no name")?;
    let name = String::from_utf8(info[..nul].to_vec()).map_err(|e| format!("info name: {e}"))?;
    if info.len() != nul + 1 + COMPACT_INFO_BYTES {
        return Err(format!("{name}: named info is {} bytes", info.len()));
    }
    swap_u32_array(&mut info[nul + 1..]);
    let (version, count) = (read_u32_le(info, nul + 5), read_u32_le(info, nul + 9));
    let schm = body_mut(data, &kids[1])?;
    let schema = ComponentSchema::from_schm_body(schm, true).ok_or_else(|| format!("{name}: schm does not parse"))?;
    swap_schm_body_inplace(schm);
    if let Some(r) = report {
        schema.fields.iter().for_each(|f| r.record_field(f.field_type as u32));
    }
    let g = GroupShape::new(&name, version, &schema, count, chdr_flags, gate)?;
    swap_data(body_mut(data, data_row)?, &g)
}

/// Swap every body of a big-endian ECS container's data area. Rows are walked by their child
/// counts; a tag this does not know fails the container.
pub(super) fn convert_ecs_bodies(
    data: &mut [u8],
    rows: &[Descriptor],
    schemas: Option<&ComponentSchemas>,
    mut report: Option<&mut SchemaCoverageReport>,
) -> Result<(), String> {
    let mut chdr: Option<(u32, i16)> = None;
    let mut i = 0;
    while i < rows.len() {
        let row = &rows[i];
        match row.tag {
            ChunkTag::Chdr => {
                let b = body_mut(data, row)?;
                convert_chdr_body_inplace(b);
                if b.len() < 8 {
                    return Err(format!("CHDR is {} bytes", b.len()));
                }
                chdr = Some((read_u32_le(b, 4), i16::from_le_bytes([b[2], b[3]])));
            }
            ChunkTag::Enum => convert_enum_body_inplace(body_mut(data, row)?),
            ChunkTag::Uniq => swap_u32_array(body_mut(data, row)?),
            ChunkTag::Flgt => swap_flgt(body_mut(data, row)?)?,
            ChunkTag::Flgs => swap_flgs(body_mut(data, row)?)?,
            ChunkTag::Comp => {
                let (flags, gate) = chdr.ok_or("a COMP group before CHDR")?;
                let kids = rows
                    .get(i + 1..i + 1 + row.row_u4 as usize)
                    .ok_or_else(|| format!("COMP row {i} claims {} children", row.row_u4))?;
                swap_group(data, kids, flags, gate, schemas, report.as_deref_mut())
                    .map_err(|e| format!("COMP row {i}: {e}"))?;
                i += kids.len();
            }
            other => return Err(format!("row {i} {other:?} is not an ECS container's")),
        }
        i += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SchemaFieldType;

    fn be32(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    fn schema(fields: &[(SchemaFieldType, u16, u8, u8)], stride: u32) -> ComponentSchema {
        ComponentSchema {
            payload_stride: stride,
            fields: fields
                .iter()
                .enumerate()
                .map(|(i, &(field_type, byte_offset, bit_start, bit_width))| SchemaField {
                    field_type,
                    name_hash: i as u32,
                    byte_offset,
                    bit_start,
                    bit_width,
                    unk: 0,
                })
                .collect(),
        }
    }

    fn descriptor(tag: ChunkTag, row_u0: usize, body_size: usize, children: u32) -> Descriptor {
        Descriptor {
            tag,
            tag_bytes_le: tag.as_bytes(),
            row_u0: row_u0 as u32,
            body_size: body_size as u32,
            row_u3: 0,
            row_u4: children,
        }
    }

    /// `[u32 n][n keys][u8 a][u8 b][u16 c][u32 bits]`: a u16 between two bytes, then a u32 holding
    /// two bit fields.
    fn mixed() -> ComponentSchema {
        schema(
            &[
                (SchemaFieldType::Byte, 0, 0, 0),
                (SchemaFieldType::U8, 1, 0, 0),
                (SchemaFieldType::U16, 2, 0, 0),
                (SchemaFieldType::Int, 4, 0, 1),
                (SchemaFieldType::Int, 4, 1, 1),
            ],
            8,
        )
    }

    fn table(hash: u32, name: &str, version: u32, s: ComponentSchema) -> ComponentSchemas {
        let mut t = ComponentSchemas::default();
        t.classes.insert((hash, version), ClassSchema { name: name.to_string(), schema: s });
        t
    }

    /// A compact container: CHDR, one COMP, flgt, flgs, in the row order of a console bake.
    fn compact(hash: u32, count: u32, flags: u32, rows_data: &[u8]) -> (Vec<u8>, Vec<Descriptor>) {
        let mut d = vec![0u8, 0, 0, 0x33];
        d.extend_from_slice(&be32(flags));
        let info_at = d.len();
        for w in [hash, 2, count, 0] {
            d.extend_from_slice(&be32(w));
        }
        let data_at = d.len();
        d.extend_from_slice(rows_data);
        let flgt_at = d.len();
        d.extend_from_slice(&be32(1));
        d.extend_from_slice(&be32(hash));
        d.extend_from_slice(b"Ai\0");
        d.extend_from_slice(&be32(7));
        let flgs_at = d.len();
        d.extend_from_slice(&be32(1));
        d.extend_from_slice(&be32(0x8000_0001));
        d.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
        d.extend_from_slice(&[0; 24]);
        let end = d.len();
        let rows = vec![
            descriptor(ChunkTag::Chdr, 0, 8, 0),
            descriptor(ChunkTag::Comp, 0xFFFF_FFFF, 0, 2),
            descriptor(ChunkTag::Info, info_at, 16, 0),
            descriptor(ChunkTag::Data, data_at, flgt_at - data_at, 0),
            descriptor(ChunkTag::Flgt, flgt_at, flgs_at - flgt_at, 0),
            descriptor(ChunkTag::Flgs, flgs_at, end - flgs_at, 0),
        ];
        (d, rows)
    }

    #[test]
    fn a_hash_that_reads_as_text_is_still_a_hash() {
        // `EB A0 9B 1A` is valid UTF-8 and the word after it starts with a NUL.
        let hash = 0xEBA0_9B1A;
        let mut rec = Vec::new();
        rec.extend_from_slice(&be32(1));
        rec.extend_from_slice(&be32(0x8000_0001));
        rec.extend_from_slice(&[9, 8, 0x12, 0x34, 0x40, 0, 0, 0]);
        let (mut d, rows) = compact(hash, 1, 5, &rec);
        convert_ecs_bodies(&mut d, &rows, Some(&table(hash, "AiSkill", 2, mixed())), None).unwrap();
        assert_eq!(read_u32_le(&d, 8), hash);
        assert_eq!(read_u32_le(&d, 12), 2);
        assert_eq!(read_u32_le(&d, 16), 1);
    }

    #[test]
    fn a_fields_width_decides_its_swap_and_a_shared_unit_swaps_once() {
        let mut rec = Vec::new();
        rec.extend_from_slice(&be32(2));
        rec.extend_from_slice(&be32(0x8000_0001));
        rec.extend_from_slice(&be32(0x8000_0002));
        rec.extend_from_slice(&[9, 8, 0x12, 0x34, 0x40, 0, 0, 0]);
        let (mut d, rows) = compact(0x1111_2222, 1, 5, &rec);
        convert_ecs_bodies(&mut d, &rows, Some(&table(0x1111_2222, "Mixed", 2, mixed())), None).unwrap();
        let at = rows[3].row_u0 as usize;
        assert_eq!(read_u32_le(&d, at), 2);
        assert_eq!(read_u32_le(&d, at + 4), 0x8000_0001);
        assert_eq!(read_u32_le(&d, at + 8), 0x8000_0002);
        assert_eq!(&d[at + 12..at + 14], &[9, 8]);
        assert_eq!(u16::from_le_bytes([d[at + 14], d[at + 15]]), 0x1234);
        assert_eq!(read_u32_le(&d, at + 16), 2);
    }

    #[test]
    fn a_fixed_records_bit_unit_is_stored_reversed() {
        // Retail pairs, Xbox bytes against PC bytes. HibernationControl's two bits of byte 5.
        let s = schema(&[(SchemaFieldType::Byte, 0, 0, 1), (SchemaFieldType::Byte, 0, 1, 1)], 1);
        let mut p = [0x80];
        swap_fixed_payload(&mut p, &s, "HibernationControl").unwrap();
        assert_eq!(p, [0x01]);
        // Health's third bit of a u32.
        let s = schema(
            &[
                (SchemaFieldType::Int, 0, 0, 1),
                (SchemaFieldType::Int, 0, 1, 1),
                (SchemaFieldType::Int, 0, 2, 1),
            ],
            4,
        );
        let mut p = [0x20, 0, 0, 0];
        swap_fixed_payload(&mut p, &s, "Health").unwrap();
        assert_eq!(p, [0x04, 0, 0, 0]);
        // WeaponHint's four 2-bit fields, the first holding 2 on PC.
        let s = schema(
            &[
                (SchemaFieldType::Int, 0, 0, 2),
                (SchemaFieldType::Int, 0, 2, 2),
                (SchemaFieldType::Int, 0, 4, 2),
                (SchemaFieldType::Int, 0, 6, 2),
            ],
            4,
        );
        let mut p = [0xBF, 0, 0, 0];
        swap_fixed_payload(&mut p, &s, "WeaponHint").unwrap();
        assert_eq!(p, [0xFD, 0, 0, 0]);
    }

    #[test]
    fn net_category_info_keeps_its_field_values() {
        // Retail pair: Xbox `1C 20`, PC `31 04`. Field 0 is 4 bits and holds 1 on both.
        let widths = [(0, 4), (4, 1), (5, 1), (6, 1), (7, 1), (8, 1), (9, 1), (10, 1)];
        let fields: Vec<_> = widths.iter().map(|&(s, w)| (SchemaFieldType::U16, 0, s, w)).collect();
        let s = schema(&fields, 2);
        let refs: Vec<&SchemaField> = s.fields.iter().collect();
        let mut p = [0x1C, 0x20];
        mirror_bit_unit(&mut p, &refs, "NetCategoryInfo").unwrap();
        assert_eq!(p, [0x31, 0x04]);
    }

    #[test]
    fn a_bit_no_field_declares_fails() {
        let s = schema(&[(SchemaFieldType::Byte, 0, 0, 1)], 1);
        let e = swap_fixed_payload(&mut [0x01], &s, "Hibernation").unwrap_err();
        assert!(e.contains("Hibernation: unit 0x00000001"), "{e}");
    }

    #[test]
    fn the_rows_after_the_last_group_are_swapped() {
        let mut rec = Vec::new();
        rec.extend_from_slice(&be32(1));
        rec.extend_from_slice(&be32(0x8000_0001));
        rec.extend_from_slice(&[0; 8]);
        let (mut d, rows) = compact(0x1111_2222, 1, 5, &rec);
        convert_ecs_bodies(&mut d, &rows, Some(&table(0x1111_2222, "Mixed", 2, mixed())), None).unwrap();
        let flgt = rows[4].row_u0 as usize;
        assert_eq!(read_u32_le(&d, flgt), 1);
        assert_eq!(read_u32_le(&d, flgt + 4), 0x1111_2222);
        assert_eq!(&d[flgt + 8..flgt + 11], b"Ai\0");
        assert_eq!(read_u32_le(&d, flgt + 11), 7);
        let flgs = rows[5].row_u0 as usize;
        assert_eq!(read_u32_le(&d, flgs), 1);
        assert_eq!(read_u32_le(&d, flgs + 4), 0x8000_0001);
        // Retail: a bitset is u64 words, so bit 0 of a class sits in the eighth console byte.
        assert_eq!(d[flgs + 8], 1);
    }

    #[test]
    fn a_name_record_under_bit_2_is_one_hash() {
        let s = schema(&[(SchemaFieldType::StringRef, 0, 0, 0), (SchemaFieldType::Byte, 4, 0, 0)], 5);
        let mut rec = Vec::new();
        rec.extend_from_slice(&be32(1));
        rec.extend_from_slice(&be32(0x8000_0001));
        rec.extend_from_slice(&be32(0xCAFE_F00D));
        let g = GroupShape::new("Name", 1, &s, 1, 5, 0x33).unwrap();
        assert_eq!(g.payload, RecordPayload::NameHash);
        swap_data(&mut rec, &g).unwrap();
        assert_eq!(read_u32_le(&rec, 8), 0xCAFE_F00D);
    }

    #[test]
    fn a_class_the_pc_bake_lacks_fails_naming_it() {
        let (mut d, rows) = compact(0x1111_2222, 0, 5, &[]);
        let e = convert_ecs_bodies(&mut d, &rows, Some(&ComponentSchemas::default()), None).unwrap_err();
        assert!(e.contains("0x11112222"), "{e}");
        let t = table(0x1111_2222, "Mixed", 3, mixed());
        let (mut d, rows) = compact(0x1111_2222, 0, 5, &[]);
        let e = convert_ecs_bodies(&mut d, &rows, Some(&t), None).unwrap_err();
        assert!(e.contains("Mixed: info version 2"), "{e}");
        let (mut d, rows) = compact(0x1111_2222, 0, 5, &[]);
        let e = convert_ecs_bodies(&mut d, &rows, None, None).unwrap_err();
        assert!(e.contains("schemas of a PC vz.wad"), "{e}");
    }

    #[test]
    fn data_a_records_walk_does_not_consume_fails() {
        let mut rec = Vec::new();
        rec.extend_from_slice(&be32(1));
        rec.extend_from_slice(&be32(0x8000_0001));
        rec.extend_from_slice(&[0; 9]);
        let (mut d, rows) = compact(0x1111_2222, 1, 5, &rec);
        let e = convert_ecs_bodies(&mut d, &rows, Some(&table(0x1111_2222, "Mixed", 2, mixed())), None).unwrap_err();
        assert!(e.contains("Mixed: 1 records end at 16"), "{e}");
    }

    #[test]
    fn a_form_the_chdr_does_not_declare_fails() {
        let (mut d, rows) = compact(0x1111_2222, 0, 1, &[]);
        let e = convert_ecs_bodies(&mut d, &rows, None, None).unwrap_err();
        assert!(e.contains("want a COMP of"), "{e}");
    }

    #[test]
    fn a_guidmap_swaps_at_its_readers_widths() {
        let mut b = Vec::new();
        b.extend_from_slice(&8u16.to_be_bytes());
        b.extend_from_slice(&[0, 0]);
        b.extend_from_slice(&be32(2));
        b.extend_from_slice(&be32(1));
        b.extend_from_slice(&[0x0D, 0x10, 0, 0]);
        b.extend_from_slice(&be32(0xD27C_C641));
        b.extend_from_slice(&be32(0));
        b.extend_from_slice(&be32(0x8000_0002));
        b.extend_from_slice(&[7, 9]);
        b.extend_from_slice(&be32(0xAABB_CCDD));
        b.extend_from_slice(&0x0A7Fu16.to_be_bytes());
        let row = descriptor(ChunkTag::Chdr, 0, b.len(), 0);
        convert_guidmap_body(&mut b, &[row]).unwrap();
        assert_eq!(u16::from_le_bytes([b[0], b[1]]), 8);
        assert_eq!(read_u32_le(&b, 4), 2);
        assert_eq!(read_u32_le(&b, 8), 1);
        assert_eq!(&b[12..14], &[0x0D, 0x10]);
        assert_eq!(read_u32_le(&b, 16), 0xD27C_C641);
        assert_eq!(read_u32_le(&b, 24), 0x8000_0002);
        assert_eq!(&b[28..30], &[7, 9]);
        assert_eq!(read_u32_le(&b, 30), 0xAABB_CCDD);
        assert_eq!(u16::from_le_bytes([b[34], b[35]]), 0x0A7F);
    }

    #[test]
    fn a_guidmap_of_the_wrong_size_fails() {
        let mut b = vec![0u8; 21];
        b[7] = 0;
        let row = descriptor(ChunkTag::Chdr, 0, 21, 0);
        let e = convert_guidmap_body(&mut b, &[row]).unwrap_err();
        assert!(e.contains("guidmap is 21 bytes"), "{e}");
    }

    #[test]
    fn transform_takes_its_stride_from_the_gate() {
        let s = schema(&[(SchemaFieldType::Blob32, 0, 0, 0)], 52);
        let g = GroupShape::new("Transform", 80, &s, 1, 2, 0x38).unwrap();
        assert_eq!(g.payload, RecordPayload::Transform { tail: true });
        let g = GroupShape::new("Transform", 80, &s, 1, 2, 0x20).unwrap();
        assert_eq!(g.payload, RecordPayload::Transform { tail: false });
    }
}
