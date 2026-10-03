//! The `worldentity` container (type `0x5647C35D`): the world's templates, as component records.
//!
//! The retail game ships one, `0x50075B3B` in `blocks\VZ\resident_P000_Q3.block`. Its loader is the
//! CHDR dispatcher `FUN_00654940` (`mercs2_unpacked.exe`), reached from the block dispatch
//! `FUN_004646B0`. The format is specified in `docs/worldentity_container_format.md`; this module
//! is its codec:
//!
//! * [`WorldEntity::parse`] reads every chunk into a typed model and refuses anything the writer
//!   would not reproduce, so a successful parse re-writes to the identical bytes
//!   ([`WorldEntity::write`]).
//! * Every component record decodes into typed field values ([`Component::decode`]) and encodes
//!   back from them ([`Component::encode`]).
//! * [`WorldEntity::append_template`] adds one template whose every component, field and value is
//!   declared by the caller. Nothing is copied from another template.
//!
//! The container is a UCFX tree (`ucfx::parse_ucfx_tree`), top level in this order:
//!
//! ```text
//! CHDR   8 B   { i16, i16 stride gate, u32 flags }
//! enum         the enum tables
//! UNIQ         [u32 n][n × u32 key] — every template key, ascending
//! COMP ×N      marker; children info, schm, data — one component class each
//! flgt         [u32 n][n × (u32 class hash, cstring class name)][u32 trailer]
//! flgs         [u32 n][n × (u32 key, 32-byte class-membership bitset)]
//! ```

use crate::hash::pandemic_hash_m2;
use crate::schema::{ComponentSchema, FieldValue, SchemaField, SchemaFieldType};
use crate::ucfx::{parse_ucfx_tree, write_ucfx_tree, UcfxNode};

/// Type hash of a `worldentity` container (`pandemic_hash_m2("worldentity")`).
pub const WORLDENTITY_TYPE_HASH: u32 = crate::types::TYPE_HASH_WORLD_ENTITY_DATA;

/// Name hash of the retail `worldentity` asset in `blocks\VZ\resident_P000_Q3.block`.
pub const RETAIL_WORLDENTITY_NAME_HASH: u32 = 0x5007_5B3B;

/// Longest string a native string reader accepts: `FUN_00825DC0` reads into a 0x80-byte buffer,
/// terminator included.
pub const MAX_STRING_BYTES: usize = 0x7F;

/// Bytes in one `flgs` bitset (the `Flags` container's record stride, 32).
pub const FLAGS_BYTES: usize = 32;

/// Bit 31 of a key marks a template: the template lookup `FUN_00672F70` returns only owners whose
/// key is negative as an `i32`.
pub const TEMPLATE_KEY_BIT: u32 = 0x8000_0000;

type Res<T> = Result<T, String>;

// ------------------------------------------------------------------------------------------------
// Little-endian cursor
// ------------------------------------------------------------------------------------------------

struct Cursor<'a> {
    b: &'a [u8],
    pos: usize,
    what: &'a str,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8], what: &'a str) -> Self {
        Cursor { b, pos: 0, what }
    }
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(|| {
            format!(
                "{}: needs {n} byte(s) at +{} but the body is {} bytes",
                self.what,
                self.pos,
                self.b.len()
            )
        })?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn i16(&mut self) -> Res<i16> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Res<u32> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn cstring(&mut self) -> Res<String> {
        let rest = &self.b[self.pos..];
        let nul = rest.iter().position(|&c| c == 0).ok_or_else(|| {
            format!("{}: string at +{} has no terminator", self.what, self.pos)
        })?;
        if nul > MAX_STRING_BYTES {
            return Err(format!(
                "{}: string at +{} is {nul} bytes; the engine's reader holds {MAX_STRING_BYTES}",
                self.what, self.pos
            ));
        }
        let s = std::str::from_utf8(&rest[..nul])
            .map_err(|e| format!("{}: string at +{} is not UTF-8: {e}", self.what, self.pos))?
            .to_string();
        self.pos += nul + 1;
        Ok(s)
    }
    fn end(&self) -> Res<()> {
        if self.pos == self.b.len() {
            Ok(())
        } else {
            Err(format!(
                "{}: {} trailing byte(s) after +{}",
                self.what,
                self.b.len() - self.pos,
                self.pos
            ))
        }
    }
}

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn push_cstring(out: &mut Vec<u8>, s: &str, what: &str) -> Res<()> {
    check_string(s, what)?;
    out.extend_from_slice(s.as_bytes());
    out.push(0);
    Ok(())
}

fn check_string(s: &str, what: &str) -> Res<()> {
    if s.as_bytes().contains(&0) {
        return Err(format!("{what}: {s:?} contains a NUL byte"));
    }
    if s.len() > MAX_STRING_BYTES {
        return Err(format!(
            "{what}: {s:?} is {} bytes; the engine's string reader holds {MAX_STRING_BYTES}",
            s.len()
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// CHDR, enum, UNIQ, flgt, flgs
// ------------------------------------------------------------------------------------------------

/// The `CHDR` chunk, as `FUN_00654940` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chdr {
    /// `i16 @+0`, written to `[0x0117607C]`.
    pub field0: i16,
    /// `i16 @+2`, written to `[0x01176078]`, the Transform record-stride gate.
    pub stride_gate: i16,
    /// `u32 @+4`. Bit 0 set: the `data` chunks are grouped records (`[u32 n][n keys][payload]`) and
    /// the UNIQ pass installs a default hibernation for keys without `HibernationControl`. Bit 2
    /// set: `Name` and class names are stored as hashes instead of strings.
    pub flags: u32,
}

/// CHDR flag bit 0: grouped `data` records.
pub const CHDR_GROUPED: u32 = 1;
/// CHDR flag bit 2: names stored as hashes (`DAT_01176053`).
pub const CHDR_NAMES_AS_HASHES: u32 = 4;

impl Chdr {
    fn parse(b: &[u8]) -> Res<Self> {
        let mut c = Cursor::new(b, "CHDR");
        let h = Chdr { field0: c.i16()?, stride_gate: c.i16()?, flags: c.u32()? };
        c.end()?;
        if h.flags & CHDR_GROUPED == 0 {
            return Err(format!(
                "CHDR flags 0x{:08X}: bit 0 is clear, so the data chunks would be one record per \
                 key; that layout is not the one this codec decodes",
                h.flags
            ));
        }
        if h.flags & CHDR_NAMES_AS_HASHES != 0 {
            return Err(format!(
                "CHDR flags 0x{:08X}: bit 2 stores names as hashes; that layout is not decoded",
                h.flags
            ));
        }
        if h.flags & !(CHDR_GROUPED) != 0 {
            return Err(format!(
                "CHDR flags 0x{:08X}: bits other than 0 are set and their effect is not decoded",
                h.flags
            ));
        }
        Ok(h)
    }
    fn write(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(8);
        o.extend_from_slice(&self.field0.to_le_bytes());
        o.extend_from_slice(&self.stride_gate.to_le_bytes());
        push_u32(&mut o, self.flags);
        o
    }
}

/// One enum table of the `enum` chunk: the enum's name and its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumTable {
    pub name: String,
    pub values: Vec<EnumValue>,
}

/// One enum value: its name and the integer it stands for. On disk each name is followed by its
/// `pandemic_hash_m2`, which the codec derives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumValue {
    pub name: String,
    pub value: u32,
}

fn parse_enums(b: &[u8]) -> Res<Vec<EnumTable>> {
    let mut c = Cursor::new(b, "enum");
    let n = c.u32()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let name = c.cstring()?;
        let h = c.u32()?;
        expect_hash("enum table", &name, h)?;
        let k = c.u32()?;
        let mut values = Vec::new();
        for _ in 0..k {
            let vn = c.cstring()?;
            let vh = c.u32()?;
            expect_hash("enum value", &vn, vh)?;
            values.push(EnumValue { name: vn, value: c.u32()? });
        }
        out.push(EnumTable { name, values });
    }
    c.end()?;
    Ok(out)
}

fn write_enums(t: &[EnumTable]) -> Res<Vec<u8>> {
    let mut o = Vec::new();
    push_u32(&mut o, t.len() as u32);
    for e in t {
        push_cstring(&mut o, &e.name, "enum table name")?;
        push_u32(&mut o, pandemic_hash_m2(&e.name));
        push_u32(&mut o, e.values.len() as u32);
        for v in &e.values {
            push_cstring(&mut o, &v.name, "enum value name")?;
            push_u32(&mut o, pandemic_hash_m2(&v.name));
            push_u32(&mut o, v.value);
        }
    }
    Ok(o)
}

fn expect_hash(what: &str, name: &str, stored: u32) -> Res<()> {
    let h = pandemic_hash_m2(name);
    if h != stored {
        return Err(format!(
            "{what} {name:?}: stored hash 0x{stored:08X} is not pandemic_hash_m2 of the name \
             (0x{h:08X})"
        ));
    }
    Ok(())
}

fn parse_u32_list(b: &[u8], what: &str) -> Res<Vec<u32>> {
    let mut c = Cursor::new(b, what);
    let n = c.u32()?;
    let v = (0..n).map(|_| c.u32()).collect::<Res<Vec<u32>>>()?;
    c.end()?;
    Ok(v)
}

fn write_u32_list(v: &[u32]) -> Vec<u8> {
    let mut o = Vec::with_capacity(4 + 4 * v.len());
    push_u32(&mut o, v.len() as u32);
    for &x in v {
        push_u32(&mut o, x);
    }
    o
}

/// The `flgt` chunk: the component classes in `flgs` bit order. Bit `i` of an entity's `flgs`
/// bitset is set exactly when the entity has a record in the class `classes[i]`. `FUN_00654940`
/// reads only its count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagTable {
    /// Class names, in bit order. On disk each is preceded by its `pandemic_hash_m2`.
    pub classes: Vec<String>,
    /// The u32 after the last entry. Its meaning is not established; it is carried as read.
    pub trailer: u32,
}

impl FlagTable {
    fn parse(b: &[u8]) -> Res<Self> {
        let mut c = Cursor::new(b, "flgt");
        let n = c.u32()?;
        let mut classes = Vec::new();
        for _ in 0..n {
            let h = c.u32()?;
            let name = c.cstring()?;
            expect_hash("flgt class", &name, h)?;
            classes.push(name);
        }
        let trailer = c.u32()?;
        c.end()?;
        if n as usize > FLAGS_BYTES * 8 {
            return Err(format!("flgt lists {n} classes; a flgs bitset holds {}", FLAGS_BYTES * 8));
        }
        Ok(FlagTable { classes, trailer })
    }
    fn write(&self) -> Res<Vec<u8>> {
        let mut o = Vec::new();
        push_u32(&mut o, self.classes.len() as u32);
        for c in &self.classes {
            push_u32(&mut o, pandemic_hash_m2(c));
            push_cstring(&mut o, c, "flgt class")?;
        }
        push_u32(&mut o, self.trailer);
        Ok(o)
    }
    /// The bit index of a class, if the table lists it.
    pub fn bit_of(&self, class: &str) -> Option<usize> {
        self.classes.iter().position(|c| c == class)
    }
}

/// One `flgs` record: an entity and the `flgt` bits set for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityFlags {
    pub key: u32,
    /// Set bits, ascending; each indexes [`FlagTable::classes`].
    pub bits: Vec<u16>,
}

fn parse_flags(b: &[u8], table: &FlagTable) -> Res<Vec<EntityFlags>> {
    let mut c = Cursor::new(b, "flgs");
    let n = c.u32()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let key = c.u32()?;
        let set = c.take(FLAGS_BYTES)?;
        let mut bits = Vec::new();
        for i in 0..FLAGS_BYTES * 8 {
            if set[i / 8] >> (i % 8) & 1 == 1 {
                if i >= table.classes.len() {
                    return Err(format!(
                        "flgs key 0x{key:08X}: bit {i} is set but flgt lists {} classes",
                        table.classes.len()
                    ));
                }
                bits.push(i as u16);
            }
        }
        out.push(EntityFlags { key, bits });
    }
    c.end()?;
    Ok(out)
}

fn write_flags(f: &[EntityFlags]) -> Vec<u8> {
    let mut o = Vec::with_capacity(4 + f.len() * (4 + FLAGS_BYTES));
    push_u32(&mut o, f.len() as u32);
    for e in f {
        push_u32(&mut o, e.key);
        let mut set = [0u8; FLAGS_BYTES];
        for &b in &e.bits {
            set[b as usize / 8] |= 1 << (b % 8);
        }
        o.extend_from_slice(&set);
    }
    o
}

// ------------------------------------------------------------------------------------------------
// Components
// ------------------------------------------------------------------------------------------------

/// How a class's record payload is laid out on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `payload_stride` bytes; each schm field at its byte offset, bit fields inside their unit.
    Fixed,
    /// `Name` (native reader `FUN_006569B0`): `[cstring name][u8]`.
    Name,
    /// `PointLocation` (native reader `FUN_00656AD0`): `[32 bytes][cstring]`.
    PointLocation,
    /// `NetCategoryInfo` (native reader `FUN_0063D750`): one u16; every schm field is a bit field
    /// of it at its bit start and width.
    PackedU16,
}

/// The classes whose native deserializer reads a layout other than the schm's fixed record, with
/// the version that selects the native reader. `FUN_00654940` compares the `info` version with the
/// one each class registers (`FUN_0064EE60` and its siblings: tables `0x017C0B58` native,
/// `0x017C0B80` version, `0x017C0B6C` schm-driven) and uses the native reader when they match.
const NATIVE_LAYOUTS: &[(&str, u32, Layout)] = &[
    ("Name", 1, Layout::Name),
    ("PointLocation", 0x50, Layout::PointLocation),
    ("NetCategoryInfo", 1, Layout::PackedU16),
];

/// A typed field value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A field the schema types ([`SchemaFieldType`] + bit field).
    Field(FieldValue),
    /// A string the native reader reads inline (`Name`'s name, `PointLocation`'s text).
    Text(String),
}

/// A record's payload as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    Fixed(Vec<u8>),
    Name { name: String, flag: u8 },
    PointLocation { blob: [u8; 32], text: String },
    PackedU16(u16),
}

/// One grouped record: the keys sharing it and its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub keys: Vec<u32>,
    pub payload: Payload,
}

/// One `COMP` group: a component class, its schema and its records.
#[derive(Debug, Clone)]
pub struct Component {
    pub class: String,
    /// `info` version word, compared with the engine class's registered version.
    pub version: u32,
    /// `info` flags bit 0: each record's first u32 is also the container's secondary key.
    pub keyed: bool,
    pub schema: ComponentSchema,
    pub layout: Layout,
    pub records: Vec<Record>,
}

fn layout_for(class: &str, version: u32, schema: &ComponentSchema) -> Res<Layout> {
    if let Some(&(_, v, layout)) = NATIVE_LAYOUTS.iter().find(|(c, _, _)| *c == class) {
        if v != version {
            return Err(format!(
                "{class}: info version {version} is not the native reader's {v}, so the engine \
                 would read it through its schm-driven reader, whose layout is not decoded"
            ));
        }
        check_native_schema(class, layout, schema)?;
        return Ok(layout);
    }
    check_fixed_schema(class, schema)?;
    Ok(Layout::Fixed)
}

fn field_desc(f: &SchemaField) -> String {
    format!(
        "field 0x{:08X} ({:?} @{} bits {}+{})",
        f.name_hash, f.field_type, f.byte_offset, f.bit_start, f.bit_width
    )
}

fn check_native_schema(class: &str, layout: Layout, s: &ComponentSchema) -> Res<()> {
    let shape: Vec<(SchemaFieldType, u16, u8)> =
        s.fields.iter().map(|f| (f.field_type, f.byte_offset, f.bit_width)).collect();
    let ok = match layout {
        Layout::Name => shape == [(SchemaFieldType::StringRef, 0, 0), (SchemaFieldType::Byte, 4, 0)],
        Layout::PointLocation => {
            shape == [(SchemaFieldType::Blob32, 0, 0), (SchemaFieldType::Hash, 32, 0)]
        }
        Layout::PackedU16 => {
            let mut used = 0u32;
            s.fields.iter().all(|f| {
                let w = f.bit_width as u32;
                let st = f.bit_start as u32;
                let m = if w == 0 || st + w > 16 { u32::MAX } else { ((1u32 << w) - 1) << st };
                let fits = f.field_type == SchemaFieldType::U16 && m != u32::MAX && used & m == 0;
                used |= m;
                fits
            }) && !s.fields.is_empty()
        }
        Layout::Fixed => unreachable!("native layouts only"),
    };
    if !ok {
        return Err(format!(
            "{class}: schm {shape:?} is not the shape its native reader ({layout:?}) is decoded for"
        ));
    }
    Ok(())
}

fn check_fixed_schema(class: &str, s: &ComponentSchema) -> Res<()> {
    let stride = s.payload_stride as usize;
    // Full-width fields own their bytes; bit fields share a unit with bit fields of the same type
    // and offset whose bits do not overlap.
    let mut owner: Vec<Option<usize>> = vec![None; stride];
    let mut units: Vec<(u16, SchemaFieldType, u32)> = Vec::new();
    for (i, f) in s.fields.iter().enumerate() {
        if f.field_type == SchemaFieldType::StringRef {
            return Err(format!(
                "{class}: {} is an inline string outside the Name class; its layout is not decoded",
                field_desc(f)
            ));
        }
        let w = f.field_type.byte_width();
        let o = f.byte_offset as usize;
        if o + w > stride {
            return Err(format!(
                "{class}: {} runs past the {stride}-byte payload",
                field_desc(f)
            ));
        }
        if f.bit_width > 0 {
            let bits = (w * 8) as u32;
            if !matches!(
                f.field_type,
                SchemaFieldType::Byte
                    | SchemaFieldType::U8
                    | SchemaFieldType::Short
                    | SchemaFieldType::U16
                    | SchemaFieldType::Int
                    | SchemaFieldType::Hash
                    | SchemaFieldType::Enum
            ) || f.bit_start as u32 + f.bit_width as u32 > bits
            {
                return Err(format!("{class}: {} is not a bit field of its unit", field_desc(f)));
            }
            let m = (((1u64 << f.bit_width) - 1) << f.bit_start) as u32;
            if let Some(u) = units.iter_mut().find(|u| u.0 == f.byte_offset) {
                if u.1 != f.field_type || u.2 & m != 0 {
                    return Err(format!(
                        "{class}: {} overlaps another bit field of its unit",
                        field_desc(f)
                    ));
                }
                u.2 |= m;
                continue;
            }
            units.push((f.byte_offset, f.field_type, m));
        }
        for b in &mut owner[o..o + w] {
            if b.is_some() {
                return Err(format!("{class}: {} overlaps another field", field_desc(f)));
            }
            *b = Some(i);
        }
    }
    Ok(())
}

impl Component {
    fn parse(info: &[u8], schm: &[u8], data: &[u8]) -> Res<Self> {
        let mut c = Cursor::new(info, "info");
        let class = c.cstring()?;
        let type_hash = c.u32()?;
        expect_hash("component class", &class, type_hash)?;
        let version = c.u32()?;
        let count = c.u32()?;
        let flags = c.u32()?;
        c.end()?;
        if flags > 1 {
            return Err(format!(
                "{class}: info flags 0x{flags:08X}; only bit 0 (keyed records) is decoded"
            ));
        }
        let schema = ComponentSchema::from_schm_body(schm, false)
            .ok_or_else(|| format!("{class}: schm does not parse"))?;
        if schema.to_schm_body() != schm {
            return Err(format!("{class}: schm does not re-write to its own bytes"));
        }
        let layout = layout_for(&class, version, &schema)?;
        let what = format!("{class} data");
        let mut d = Cursor::new(data, &what);
        let mut records = Vec::with_capacity(count as usize);
        for r in 0..count {
            let n = d.u32()?;
            if n == 0 {
                return Err(format!("{class}: record {r} has no keys"));
            }
            let keys = (0..n).map(|_| d.u32()).collect::<Res<Vec<u32>>>()?;
            let payload = match layout {
                Layout::Fixed => Payload::Fixed(d.take(schema.payload_stride as usize)?.to_vec()),
                Layout::Name => {
                    let name = d.cstring()?;
                    Payload::Name { name, flag: d.u8()? }
                }
                Layout::PointLocation => {
                    let mut blob = [0u8; 32];
                    blob.copy_from_slice(d.take(32)?);
                    Payload::PointLocation { blob, text: d.cstring()? }
                }
                Layout::PackedU16 => Payload::PackedU16(d.u16()?),
            };
            records.push(Record { keys, payload });
        }
        d.end()?;
        let comp = Component { class, version, keyed: flags == 1, schema, layout, records };
        for (i, r) in comp.records.iter().enumerate() {
            let v = comp.decode(&r.payload).map_err(|e| format!("record {i}: {e}"))?;
            if comp.encode(&v)? != r.payload {
                return Err(format!(
                    "{}: record {i} does not re-encode from its typed values (a bit outside \
                     every declared field is set)",
                    comp.class
                ));
            }
        }
        Ok(comp)
    }

    fn type_hash(&self) -> u32 {
        pandemic_hash_m2(&self.class)
    }

    fn write(&self) -> Res<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let mut info = Vec::new();
        push_cstring(&mut info, &self.class, "class name")?;
        push_u32(&mut info, self.type_hash());
        push_u32(&mut info, self.version);
        push_u32(&mut info, self.records.len() as u32);
        push_u32(&mut info, self.keyed as u32);
        let mut data = Vec::new();
        for r in &self.records {
            if r.keys.is_empty() {
                return Err(format!("{}: a record has no keys", self.class));
            }
            push_u32(&mut data, r.keys.len() as u32);
            for &k in &r.keys {
                push_u32(&mut data, k);
            }
            match (&r.payload, self.layout) {
                (Payload::Fixed(b), Layout::Fixed) => {
                    if b.len() != self.schema.payload_stride as usize {
                        return Err(format!(
                            "{}: payload is {} bytes, the schm stride is {}",
                            self.class,
                            b.len(),
                            self.schema.payload_stride
                        ));
                    }
                    data.extend_from_slice(b);
                }
                (Payload::Name { name, flag }, Layout::Name) => {
                    push_cstring(&mut data, name, "Name")?;
                    data.push(*flag);
                }
                (Payload::PointLocation { blob, text }, Layout::PointLocation) => {
                    data.extend_from_slice(blob);
                    push_cstring(&mut data, text, "PointLocation text")?;
                }
                (Payload::PackedU16(v), Layout::PackedU16) => data.extend_from_slice(&v.to_le_bytes()),
                (p, l) => {
                    return Err(format!("{}: a {p:?} payload in a {l:?} class", self.class));
                }
            }
        }
        Ok((info, self.schema.to_schm_body(), data))
    }

    /// Every key with a record in this class.
    pub fn keys(&self) -> impl Iterator<Item = u32> + '_ {
        self.records.iter().flat_map(|r| r.keys.iter().copied())
    }

    /// Decode a payload into one typed value per schm field, in schm order.
    pub fn decode(&self, p: &Payload) -> Res<Vec<Value>> {
        match (self.layout, p) {
            (Layout::Fixed, Payload::Fixed(b)) => self
                .schema
                .fields
                .iter()
                .map(|f| {
                    ComponentSchema::read_field(f, b).map(Value::Field).ok_or_else(|| {
                        format!("{}: {} runs past the payload", self.class, field_desc(f))
                    })
                })
                .collect(),
            (Layout::Name, Payload::Name { name, flag }) => {
                Ok(vec![Value::Text(name.clone()), Value::Field(FieldValue::U8(*flag))])
            }
            (Layout::PointLocation, Payload::PointLocation { blob, text }) => {
                let mut a = [0f32; 8];
                for (i, s) in a.iter_mut().enumerate() {
                    *s = f32::from_bits(u32::from_le_bytes([
                        blob[4 * i],
                        blob[4 * i + 1],
                        blob[4 * i + 2],
                        blob[4 * i + 3],
                    ]));
                }
                Ok(vec![Value::Field(FieldValue::Blob32(a)), Value::Text(text.clone())])
            }
            (Layout::PackedU16, Payload::PackedU16(v)) => Ok(self
                .schema
                .fields
                .iter()
                .map(|f| {
                    let m = (1u32 << f.bit_width) - 1;
                    Value::Field(FieldValue::Bits((*v as u32 >> f.bit_start) & m))
                })
                .collect()),
            (l, p) => Err(format!("{}: a {p:?} payload in a {l:?} class", self.class)),
        }
    }

    /// Encode one typed value per schm field (schm order) into a payload. Every field must be
    /// given, each with the value kind its type and bit field take; bytes no field covers are 0
    /// (every such byte of every retail record is 0).
    pub fn encode(&self, values: &[Value]) -> Res<Payload> {
        if values.len() != self.schema.fields.len() {
            return Err(format!(
                "{}: {} value(s) for {} field(s)",
                self.class,
                values.len(),
                self.schema.fields.len()
            ));
        }
        let bad = |f: &SchemaField, v: &Value| {
            format!("{}: {} cannot hold {v:?}", self.class, field_desc(f))
        };
        match self.layout {
            Layout::Fixed => {
                let mut buf = vec![0u8; self.schema.payload_stride as usize];
                for (f, v) in self.schema.fields.iter().zip(values) {
                    let Value::Field(fv) = v else { return Err(bad(f, v)) };
                    write_field(f, fv, &mut buf).map_err(|e| format!("{}: {e}", self.class))?;
                }
                Ok(Payload::Fixed(buf))
            }
            Layout::Name => match (&values[0], &values[1]) {
                (Value::Text(name), Value::Field(FieldValue::U8(flag))) => {
                    check_string(name, "Name")?;
                    Ok(Payload::Name { name: name.clone(), flag: *flag })
                }
                _ => Err(format!("{}: takes [text, u8], got {values:?}", self.class)),
            },
            Layout::PointLocation => match (&values[0], &values[1]) {
                (Value::Field(FieldValue::Blob32(a)), Value::Text(text)) => {
                    check_string(text, "PointLocation text")?;
                    let mut blob = [0u8; 32];
                    for (i, s) in a.iter().enumerate() {
                        blob[4 * i..4 * i + 4].copy_from_slice(&s.to_bits().to_le_bytes());
                    }
                    Ok(Payload::PointLocation { blob, text: text.clone() })
                }
                _ => Err(format!("{}: takes [blob32, text], got {values:?}", self.class)),
            },
            Layout::PackedU16 => {
                let mut u = 0u32;
                for (f, v) in self.schema.fields.iter().zip(values) {
                    let Value::Field(FieldValue::Bits(x)) = v else { return Err(bad(f, v)) };
                    if *x >> f.bit_width != 0 {
                        return Err(format!(
                            "{}: {} is {} bits wide; {x} does not fit",
                            self.class,
                            field_desc(f),
                            f.bit_width
                        ));
                    }
                    u |= x << f.bit_start;
                }
                Ok(Payload::PackedU16(u as u16))
            }
        }
    }
}

/// Write one typed value at its field's place in a fixed payload.
fn write_field(f: &SchemaField, v: &FieldValue, buf: &mut [u8]) -> Res<()> {
    let o = f.byte_offset as usize;
    let mismatch = || format!("{} cannot hold {v:?}", field_desc(f));
    if f.bit_width > 0 {
        let FieldValue::Bits(x) = v else { return Err(mismatch()) };
        if (*x as u64) >> f.bit_width != 0 {
            return Err(format!("{} is {} bits wide; {x} does not fit", field_desc(f), f.bit_width));
        }
        let w = f.field_type.byte_width();
        let mut unit = 0u64;
        for i in 0..w {
            unit |= (buf[o + i] as u64) << (8 * i);
        }
        unit |= (*x as u64) << f.bit_start;
        for i in 0..w {
            buf[o + i] = (unit >> (8 * i)) as u8;
        }
        return Ok(());
    }
    use SchemaFieldType as T;
    match (f.field_type, v) {
        (T::Byte | T::U8, FieldValue::U8(x)) => buf[o] = *x,
        (T::Short | T::U16, FieldValue::U16(x)) => buf[o..o + 2].copy_from_slice(&x.to_le_bytes()),
        (T::Int | T::Hash | T::Enum, FieldValue::U32(x)) => {
            buf[o..o + 4].copy_from_slice(&x.to_le_bytes())
        }
        (T::F32, FieldValue::F32(x)) => buf[o..o + 4].copy_from_slice(&x.to_bits().to_le_bytes()),
        (T::Vec3, FieldValue::Vec3(a)) => {
            for (i, s) in a.iter().enumerate() {
                buf[o + 4 * i..o + 4 * i + 4].copy_from_slice(&s.to_bits().to_le_bytes());
            }
        }
        (T::Blob32, FieldValue::Blob32(a)) => {
            for (i, s) in a.iter().enumerate() {
                buf[o + 4 * i..o + 4 * i + 4].copy_from_slice(&s.to_bits().to_le_bytes());
            }
        }
        _ => return Err(mismatch()),
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// The container
// ------------------------------------------------------------------------------------------------

/// A parsed `worldentity` container.
#[derive(Debug, Clone)]
pub struct WorldEntity {
    pub header: Chdr,
    pub enums: Vec<EnumTable>,
    /// `UNIQ`: every template key, ascending.
    pub instances: Vec<u32>,
    pub components: Vec<Component>,
    pub flag_table: FlagTable,
    /// `flgs`: one record per key whose bitset is not empty, ascending by key.
    pub flags: Vec<EntityFlags>,
}

/// One component of a template being appended: its class and one value per schm field.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentDecl {
    pub class: String,
    pub values: Vec<Value>,
}

/// A template to append: its name, key and every component, each fully declared.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateDecl {
    pub name: String,
    pub key: u32,
    /// The `Name` record's flag byte. `FUN_006569B0` registers a template's name regardless of it.
    pub name_flag: u8,
    pub components: Vec<ComponentDecl>,
}

/// The key a template's name derives: `0x8` in the top nibble (the level's templates), the low 28
/// bits of `pandemic_hash_m2(name)` below it.
pub fn derived_template_key(name: &str) -> u32 {
    0x8000_0000 | (pandemic_hash_m2(name) & 0x0FFF_FFFF)
}

impl WorldEntity {
    /// Parse a container. Strict: anything [`Self::write`] would not reproduce is an error.
    pub fn parse(container: &[u8]) -> Res<Self> {
        let roots = parse_ucfx_tree(container)?;
        let tags: Vec<String> = roots.iter().map(UcfxNode::tag_str).collect();
        let body = |i: usize, tag: &[u8; 4]| -> Res<&[u8]> {
            let n = roots.get(i).ok_or_else(|| format!("top-level row {i} missing, want {tag:?}"))?;
            if &n.tag != tag || !n.children.is_empty() {
                return Err(format!(
                    "top-level row {i} is '{}' with {} children; the layout is CHDR, enum, UNIQ, \
                     COMP…, flgt, flgs (rows: {tags:?})",
                    n.tag_str(),
                    n.children.len()
                ));
            }
            n.body.as_deref().ok_or_else(|| format!("top-level row {i} '{}' is a marker", n.tag_str()))
        };
        let header = Chdr::parse(body(0, b"CHDR")?)?;
        let enums = parse_enums(body(1, b"enum")?)?;
        let instances = parse_u32_list(body(2, b"UNIQ")?, "UNIQ")?;
        let n = roots.len();
        if n < 5 {
            return Err(format!("{n} top-level rows; the layout needs at least 5 ({tags:?})"));
        }
        let mut components = Vec::new();
        for (i, node) in roots[3..n - 2].iter().enumerate() {
            let kids: Vec<&[u8; 4]> = node.children.iter().map(|c| &c.tag).collect();
            if &node.tag != b"COMP"
                || node.body.is_some()
                || kids != [b"info", b"schm", b"data"]
                || node.children.iter().any(|c| c.body.is_none() || !c.children.is_empty())
            {
                return Err(format!(
                    "top-level row {} is '{}' with children {:?}; a component is a COMP marker \
                     with info, schm, data",
                    i + 3,
                    node.tag_str(),
                    node.children.iter().map(UcfxNode::tag_str).collect::<Vec<_>>()
                ));
            }
            let b = |k: usize| node.children[k].body.as_deref().unwrap_or_default();
            components.push(
                Component::parse(b(0), b(1), b(2)).map_err(|e| format!("COMP {i}: {e}"))?,
            );
        }
        let flag_table = FlagTable::parse(body(n - 2, b"flgt")?)?;
        let flags = parse_flags(body(n - 1, b"flgs")?, &flag_table)?;
        Ok(WorldEntity { header, enums, instances, components, flag_table, flags })
    }

    /// Write the container.
    pub fn write(&self) -> Res<Vec<u8>> {
        let mut roots = vec![
            UcfxNode::leaf(*b"CHDR", self.header.write()),
            UcfxNode::leaf(*b"enum", write_enums(&self.enums)?),
            UcfxNode::leaf(*b"UNIQ", write_u32_list(&self.instances)),
        ];
        for c in &self.components {
            let (info, schm, data) = c.write()?;
            roots.push(UcfxNode::marker(
                *b"COMP",
                vec![
                    UcfxNode::leaf(*b"info", info),
                    UcfxNode::leaf(*b"schm", schm),
                    UcfxNode::leaf(*b"data", data),
                ],
            ));
        }
        roots.push(UcfxNode::leaf(*b"flgt", self.flag_table.write()?));
        roots.push(UcfxNode::leaf(*b"flgs", write_flags(&self.flags)));
        Ok(write_ucfx_tree(&roots))
    }

    /// The component groups of a class (retail `LightObject` has two).
    pub fn groups_of<'a>(&'a self, class: &'a str) -> impl Iterator<Item = &'a Component> + 'a {
        self.components.iter().filter(move |c| c.class == class)
    }

    /// The `Name` group.
    pub fn name_group(&self) -> Res<&Component> {
        let mut it = self.components.iter().filter(|c| c.layout == Layout::Name);
        match (it.next(), it.next()) {
            (Some(c), None) => Ok(c),
            (None, _) => Err("the container has no Name component".into()),
            _ => Err("the container has more than one Name component".into()),
        }
    }

    /// Every template name with its keys.
    pub fn names(&self) -> Res<Vec<(String, &[u32])>> {
        Ok(self
            .name_group()?
            .records
            .iter()
            .filter_map(|r| match &r.payload {
                Payload::Name { name, .. } => Some((name.clone(), r.keys.as_slice())),
                _ => None,
            })
            .collect())
    }

    /// Every key the container uses anywhere (UNIQ, any record, flgs).
    pub fn all_keys(&self) -> std::collections::BTreeSet<u32> {
        let mut s: std::collections::BTreeSet<u32> = self.instances.iter().copied().collect();
        for c in &self.components {
            s.extend(c.keys());
        }
        s.extend(self.flags.iter().map(|f| f.key));
        s
    }

    /// The `flgs` bits the components give a key: one per class with a record for it that the
    /// `flgt` table lists.
    pub fn derived_flag_bits(&self, key: u32) -> Vec<u16> {
        let mut bits: Vec<u16> = self
            .components
            .iter()
            .filter(|c| c.keys().any(|k| k == key))
            .filter_map(|c| self.flag_table.bit_of(&c.class).map(|b| b as u16))
            .collect();
        bits.sort_unstable();
        bits.dedup();
        bits
    }

    /// Append one template. Its key must carry the template bit and be unused, its name must not
    /// hash like an existing name, every class must exist exactly once in the container (and not be
    /// `Name`), and every value must be declared. The template gets a `Name` record, one new record
    /// per component at the end of its class, its key in `UNIQ`, and the `flgs` bits its classes
    /// derive.
    pub fn append_template(&mut self, t: &TemplateDecl) -> Res<()> {
        if t.key & TEMPLATE_KEY_BIT == 0 {
            return Err(format!(
                "template key 0x{:08X} lacks bit 31; the template lookup returns only negative keys",
                t.key
            ));
        }
        if self.all_keys().contains(&t.key) {
            return Err(format!("template key 0x{:08X} is already used in the container", t.key));
        }
        check_string(&t.name, "template name")?;
        if t.name.is_empty() {
            return Err("the template name is empty".into());
        }
        let h = pandemic_hash_m2(&t.name);
        if let Some((n, _)) = self.names()?.into_iter().find(|(n, _)| pandemic_hash_m2(n) == h) {
            return Err(format!(
                "template name {:?} hashes to 0x{h:08X}, the same as the existing name {n:?}",
                t.name
            ));
        }
        if !self.instances.windows(2).all(|w| w[0] < w[1]) {
            return Err("UNIQ is not strictly ascending, so there is no place to insert into".into());
        }
        if !self.flags.windows(2).all(|w| w[0].key < w[1].key) {
            return Err("flgs is not strictly ascending by key".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut planned = Vec::new();
        for d in &t.components {
            if !seen.insert(d.class.as_str()) {
                return Err(format!("class {} is declared twice", d.class));
            }
            let idx: Vec<usize> = self
                .components
                .iter()
                .enumerate()
                .filter(|(_, c)| c.class == d.class)
                .map(|(i, _)| i)
                .collect();
            let i = match idx.as_slice() {
                [i] => *i,
                [] => return Err(format!("class {} has no component group in the container", d.class)),
                _ => {
                    return Err(format!(
                        "class {} has {} component groups; which one a new record belongs in is \
                         not established",
                        d.class,
                        idx.len()
                    ))
                }
            };
            if self.components[i].layout == Layout::Name {
                return Err("Name is written from the template name, not declared".into());
            }
            let payload = self.components[i].encode(&d.values)?;
            planned.push((i, payload));
        }
        let name_idx = self
            .components
            .iter()
            .position(|c| c.layout == Layout::Name)
            .ok_or("the container has no Name component")?;
        for (i, payload) in planned {
            self.components[i].records.push(Record { keys: vec![t.key], payload });
        }
        self.components[name_idx].records.push(Record {
            keys: vec![t.key],
            payload: Payload::Name { name: t.name.clone(), flag: t.name_flag },
        });
        let at = self.instances.partition_point(|&k| k < t.key);
        self.instances.insert(at, t.key);
        let bits = self.derived_flag_bits(t.key);
        if !bits.is_empty() {
            let at = self.flags.partition_point(|f| f.key < t.key);
            self.flags.insert(at, EntityFlags { key: t.key, bits });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(code: u32, hash: u32, off: u16, start: u8, width: u8) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&code.to_le_bytes());
        e.extend_from_slice(&hash.to_le_bytes());
        e.extend_from_slice(&0u32.to_le_bytes());
        e.extend_from_slice(&off.to_le_bytes());
        e.push(start);
        e.push(width);
        e
    }

    fn schm(stride: u32, fields: &[Vec<u8>]) -> Vec<u8> {
        let mut s = Vec::new();
        s.extend_from_slice(&(fields.len() as u32).to_le_bytes());
        s.extend_from_slice(&stride.to_le_bytes());
        for f in fields {
            s.extend_from_slice(f);
        }
        s
    }

    fn info(class: &str, version: u32, count: u32, keyed: u32) -> Vec<u8> {
        let mut i = class.as_bytes().to_vec();
        i.push(0);
        for v in [pandemic_hash_m2(class), version, count, keyed] {
            i.extend_from_slice(&v.to_le_bytes());
        }
        i
    }

    fn comp(class: &str, version: u32, count: u32, schm: Vec<u8>, data: Vec<u8>) -> UcfxNode {
        UcfxNode::marker(
            *b"COMP",
            vec![
                UcfxNode::leaf(*b"info", info(class, version, count, 0)),
                UcfxNode::leaf(*b"schm", schm),
                UcfxNode::leaf(*b"data", data),
            ],
        )
    }

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A small container in the retail shape: Health (f32 + three 1-bit fields), Name, and a flgt
    /// listing Health.
    fn sample() -> Vec<u8> {
        let health_schm = schm(
            8,
            &[field(7, 0xD122_919D, 0, 0, 0), field(5, 1, 4, 0, 1), field(5, 2, 4, 1, 1), field(5, 3, 4, 2, 1)],
        );
        let mut health = le(&[2, 0x8000_0002, 0x8000_0005]);
        health.extend_from_slice(&100f32.to_bits().to_le_bytes());
        health.extend_from_slice(&5u32.to_le_bytes());
        let name_schm = schm(5, &[field(8, 0x1DE5_C824, 0, 0, 0), field(1, 0x12AF_A0B8, 4, 0, 0)]);
        let mut names = le(&[1, 0x8000_0002]);
        names.extend_from_slice(b"box\0\0");
        names.extend_from_slice(&le(&[1, 0x8000_0005]));
        names.extend_from_slice(b"crate\0\x01");
        let mut enums = le(&[1]);
        enums.extend_from_slice(b"BoolEnum\0");
        enums.extend_from_slice(&le(&[pandemic_hash_m2("BoolEnum"), 1]));
        enums.extend_from_slice(b"True\0");
        enums.extend_from_slice(&le(&[pandemic_hash_m2("True"), 1]));
        let mut flgt = le(&[1, pandemic_hash_m2("Health")]);
        flgt.extend_from_slice(b"Health\0");
        flgt.extend_from_slice(&le(&[0xE9DA_BB4A]));
        let mut flgs = le(&[2, 0x8000_0002]);
        flgs.extend_from_slice(&[1; 1]);
        flgs.extend_from_slice(&[0; 31]);
        flgs.extend_from_slice(&le(&[0x8000_0005]));
        flgs.extend_from_slice(&[1; 1]);
        flgs.extend_from_slice(&[0; 31]);
        let mut chdr = vec![0, 0, 0x33, 0];
        chdr.extend_from_slice(&le(&[1]));
        write_ucfx_tree(&[
            UcfxNode::leaf(*b"CHDR", chdr),
            UcfxNode::leaf(*b"enum", enums),
            UcfxNode::leaf(*b"UNIQ", le(&[2, 0x8000_0002, 0x8000_0005])),
            comp("Health", 0x51, 1, health_schm, health),
            comp("Name", 1, 2, name_schm, names),
            UcfxNode::leaf(*b"flgt", flgt),
            UcfxNode::leaf(*b"flgs", flgs),
        ])
    }

    #[test]
    fn a_container_round_trips_and_decodes_typed() {
        let c = sample();
        let we = WorldEntity::parse(&c).unwrap();
        assert_eq!(we.write().unwrap(), c);
        assert_eq!(we.instances, vec![0x8000_0002, 0x8000_0005]);
        assert_eq!(we.flags[0].bits, vec![0]);
        assert_eq!(we.derived_flag_bits(0x8000_0005), vec![0]);
        let h = &we.components[0];
        assert_eq!(h.layout, Layout::Fixed);
        let v = h.decode(&h.records[0].payload).unwrap();
        assert_eq!(
            v,
            vec![
                Value::Field(FieldValue::F32(100.0)),
                Value::Field(FieldValue::Bits(1)),
                Value::Field(FieldValue::Bits(0)),
                Value::Field(FieldValue::Bits(1)),
            ]
        );
        assert_eq!(we.names().unwrap()[1].0, "crate");
    }

    #[test]
    fn append_adds_name_records_uniq_and_flags() {
        let mut we = WorldEntity::parse(&sample()).unwrap();
        let key = derived_template_key("qm_gate_c4");
        assert_eq!(key >> 28, 8);
        let t = TemplateDecl {
            name: "qm_gate_c4".into(),
            key,
            name_flag: 1,
            components: vec![ComponentDecl {
                class: "Health".into(),
                values: vec![
                    Value::Field(FieldValue::F32(50.0)),
                    Value::Field(FieldValue::Bits(0)),
                    Value::Field(FieldValue::Bits(1)),
                    Value::Field(FieldValue::Bits(0)),
                ],
            }],
        };
        we.append_template(&t).unwrap();
        let bytes = we.write().unwrap();
        let back = WorldEntity::parse(&bytes).unwrap();
        assert!(back.instances.contains(&key));
        assert!(back.instances.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(back.components[0].records.last().unwrap().keys, vec![key]);
        assert_eq!(
            back.components[0].records.last().unwrap().payload,
            Payload::Fixed([&50f32.to_bits().to_le_bytes()[..], &[2, 0, 0, 0]].concat())
        );
        assert!(back.names().unwrap().iter().any(|(n, k)| n == "qm_gate_c4" && *k == [key]));
        assert_eq!(back.flags.iter().find(|f| f.key == key).unwrap().bits, vec![0]);
        // A second append under the same key or name is refused.
        let e = we.append_template(&t).unwrap_err();
        assert!(e.contains("already used"), "{e}");
        let mut t2 = t.clone();
        t2.key = 0x8000_0077;
        t2.name = "QM_GATE_C4".into();
        assert!(we.append_template(&t2).unwrap_err().contains("same as the existing name"));
    }

    #[test]
    fn append_refuses_undeclared_or_mistyped_values_and_unknown_classes() {
        let mut we = WorldEntity::parse(&sample()).unwrap();
        let mut t = TemplateDecl {
            name: "x".into(),
            key: 0x8000_0010,
            name_flag: 0,
            components: vec![ComponentDecl {
                class: "Health".into(),
                values: vec![Value::Field(FieldValue::F32(1.0))],
            }],
        };
        assert!(we.append_template(&t).unwrap_err().contains("1 value(s) for 4 field(s)"));
        t.components[0].values = vec![
            Value::Field(FieldValue::U32(1)),
            Value::Field(FieldValue::Bits(0)),
            Value::Field(FieldValue::Bits(0)),
            Value::Field(FieldValue::Bits(0)),
        ];
        assert!(we.append_template(&t).unwrap_err().contains("cannot hold"));
        t.components[0].values[0] = Value::Field(FieldValue::F32(1.0));
        t.components[0].values[1] = Value::Field(FieldValue::Bits(2));
        assert!(we.append_template(&t).unwrap_err().contains("does not fit"));
        t.components[0].class = "Nope".into();
        assert!(we.append_template(&t).unwrap_err().contains("no component group"));
        t.key = 0x0000_0010;
        assert!(we.append_template(&t).unwrap_err().contains("bit 31"));
    }

    #[test]
    fn parse_refuses_a_set_bit_outside_every_field() {
        let c = sample();
        let mut we = WorldEntity::parse(&c).unwrap();
        // Bit 3 of Health's flag word belongs to no field.
        if let Payload::Fixed(b) = &mut we.components[0].records[0].payload {
            b[4] |= 8;
        }
        let bytes = we.write().unwrap();
        let e = WorldEntity::parse(&bytes).unwrap_err();
        assert!(e.contains("does not re-encode"), "{e}");
    }

    #[test]
    fn packed_u16_layout_reads_bit_fields_of_one_u16() {
        let s = schm(
            16,
            &[field(4, 1, 0, 0, 4), field(4, 2, 2, 4, 1), field(4, 3, 4, 5, 1)],
        );
        let mut data = le(&[1, 0x8000_0002]);
        data.extend_from_slice(&0x0029u16.to_le_bytes());
        let c = Component::parse(&info("NetCategoryInfo", 1, 1, 0), &s, &data).unwrap();
        assert_eq!(c.layout, Layout::PackedU16);
        assert_eq!(
            c.decode(&c.records[0].payload).unwrap(),
            vec![
                Value::Field(FieldValue::Bits(9)),
                Value::Field(FieldValue::Bits(0)),
                Value::Field(FieldValue::Bits(1)),
            ]
        );
        // A different version would select the schm-driven reader: refused.
        assert!(Component::parse(&info("NetCategoryInfo", 2, 1, 0), &s, &data).is_err());
    }
}
