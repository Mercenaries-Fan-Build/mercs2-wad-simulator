/// ECS COMP schema field type codes — the `+0` word of a 16-byte `schm` field entry.
///
/// The meaning of each code is read from the engine's per-field stream readers in
/// `mercs2_unpacked.exe`, which look a field up by name hash in the class's `schm`, seek to its
/// byte offset and then dispatch on this code:
///
/// * `FUN_00656210` (int reader) and `FUN_00656320` (float reader): codes 1 and 2 read one byte
///   (`char`), 3 and 4 read two bytes (`short`), 5, 6 and 9 read a 4-byte `int`, and 7 reads an
///   `f32` (the int reader converts it, the float reader passes it through).
/// * `FUN_00656720` (enum reader): code 9 reads a u32 and resolves it through the enum-name table
///   (`FUN_00655ea0`); codes 1–6 read as above.
/// * `FUN_00656610` (vec3 reader): code 10 reads three dwords.
/// * `FUN_0065644a` (transform reader): code 11 reads eight dwords.
/// * Code 8 is the `Name` class's string field; its native deserializer `FUN_006569B0` reads an
///   inline NUL-terminated string.
///
/// The engine reads codes 1 and 2 identically, and 3 and 4 identically; what distinguishes the two
/// members of each pair is not established, so each keeps its own variant.
///
/// Retail check (PC `vz.wad`, `worldentity`): `Health`'s code-7 field holds `100.0f`, and its three
/// code-5 fields are bits 0, 1 and 2 of one u32 (see [`SchemaField::bit_width`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SchemaFieldType {
    /// Code 1: one byte.
    Byte = 1,
    /// Code 2: one byte.
    U8 = 2,
    /// Code 3: two bytes.
    Short = 3,
    /// Code 4: two bytes.
    U16 = 4,
    /// Code 5: a 4-byte integer.
    Int = 5,
    /// Code 6: a 4-byte word holding a `pandemic_hash_m2` name hash or a raw value.
    Hash = 6,
    /// Code 7: `f32`.
    F32 = 7,
    /// Code 8: an inline NUL-terminated string (`Name`).
    StringRef = 8,
    /// Code 9: a 4-byte word the enum reader resolves as the hash of an enum value name.
    Enum = 9,
    /// Code 10: three `f32`.
    Vec3 = 10,
    /// Code 11: eight dwords (32 bytes).
    Blob32 = 11,
}

impl SchemaFieldType {
    /// Parse a raw type code from a schm entry.
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::Byte),
            2 => Some(Self::U8),
            3 => Some(Self::Short),
            4 => Some(Self::U16),
            5 => Some(Self::Int),
            6 => Some(Self::Hash),
            7 => Some(Self::F32),
            8 => Some(Self::StringRef),
            9 => Some(Self::Enum),
            10 => Some(Self::Vec3),
            11 => Some(Self::Blob32),
            _ => None,
        }
    }

    /// Byte width of the storage unit the engine reads for this code.
    pub fn byte_width(&self) -> usize {
        match self {
            Self::Byte | Self::U8 => 1,
            Self::Short | Self::U16 => 2,
            Self::Int | Self::Hash | Self::F32 | Self::StringRef | Self::Enum => 4,
            Self::Vec3 => 12,
            Self::Blob32 => 32,
        }
    }

    /// Whether this field needs byte-swapping for BE→LE conversion.
    pub fn needs_swap(&self) -> bool {
        !matches!(self, Self::Byte | Self::U8)
    }

    /// The atomic swap unit size (bytes per swap operation).
    /// Vec3 and Blob32 are swapped as multiple 4-byte units.
    pub fn swap_unit(&self) -> usize {
        match self {
            Self::Byte | Self::U8 => 0,
            Self::Short | Self::U16 => 2,
            _ => 4,
        }
    }

    /// Number of swap operations needed for this field.
    pub fn swap_count(&self) -> usize {
        match self {
            Self::Byte | Self::U8 => 0,
            Self::Short | Self::U16 => 1,
            Self::Int | Self::Hash | Self::F32 | Self::StringRef | Self::Enum => 1,
            Self::Vec3 => 3,
            Self::Blob32 => 8,
        }
    }
}

/// A parsed schema field entry from a `schm` chunk (16 bytes on disk):
///
/// ```text
///   +0  u32 type_code      (SchemaFieldType)
///   +4  u32 name_hash      (pandemic_hash_m2 of the field name)
///   +8  u32 unk            (0 in every retail record observed)
///   +12 offset_word        { u16 byte_offset ; u8 bit_start ; u8 bit_width }
/// ```
///
/// **`byte_offset` is the LOW 16 bits of the offset word** — verified against retail
/// vz.wad: Transform 32,36,38,…,50 · HibernationControl 0,2,3,4,5,5 · Road 0,4,8,12,16,28 ·
/// RoadIntersection 0,4,…,120 (all monotonic, matching type widths). This is the field's
/// location inside the deserialized payload record.
///
/// The two high bytes place a **bit field** inside the storage unit at `byte_offset`: `bit_start`
/// is its lowest bit and `bit_width` its width; `bit_width == 0` means the field is the whole unit.
/// Retail: `HibernationControl` packs two 1-bit fields in byte 5 (starts 0 and 1), `Health` three
/// 1-bit fields in the u32 at 4, `HumanInventory` fields of widths 1/7/8/8/8 at starts 0/1/8/16/24
/// in the u32 at 20. Over every bit-field unit of the retail `worldentity`, no bit outside the
/// declared fields is set. See `docs/spatial_hash_crash_analysis.md` (the RCA that established the
/// low-16 convention) and `docs/schm_type_codes.md`.
#[derive(Debug, Clone)]
pub struct SchemaField {
    pub field_type: SchemaFieldType,
    pub name_hash: u32,
    /// Location of the field's storage unit inside the payload record (LOW 16 bits of the offset
    /// word).
    pub byte_offset: u16,
    /// offset_word[2]: the lowest bit of a bit field inside its storage unit.
    pub bit_start: u8,
    /// offset_word[3]: the width of a bit field in bits; 0 for a field that is its whole unit.
    pub bit_width: u8,
    /// The schm entry's `+8` word — 0 in all observed retail data; retained for fidelity.
    pub unk: u32,
}

/// Parsed schema for an ECS component (`schm` chunk = `[u32 n_fields][u32 payload_stride][fields…]`).
#[derive(Debug, Clone)]
pub struct ComponentSchema {
    /// Serialized payload size in bytes = the descriptor stride the exe sets at `desc+0x24`
    /// (e.g. HibernationControl = 6). The on-disk `data` record is `[u32 entity_key][payload]`,
    /// so the record stride is [`Self::record_stride`].
    pub payload_stride: u32,
    pub fields: Vec<SchemaField>,
}

impl ComponentSchema {
    /// Parse a schm chunk body into a ComponentSchema.
    /// Reads n_fields and payload_stride from the header, then field entries.
    ///
    /// `big_endian` = the source is an unconverted Xbox 360 block; retail PC vz.wad is `false`.
    /// The `byte_offset` u16 lives in the offset word's first two bytes in the source endianness
    /// (BE result is byte-identical to the historical `raw>>16`, so the BE→LE converter is
    /// unaffected); the LOW-16 read is the fix for LE/retail data (see [`SchemaField`]).
    pub fn from_schm_body(body: &[u8], big_endian: bool) -> Option<Self> {
        if body.len() < 8 {
            return None;
        }

        let rd_u32 = |o: usize| -> u32 {
            let b = [body[o], body[o + 1], body[o + 2], body[o + 3]];
            if big_endian {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            }
        };
        let rd_u16 = |o: usize| -> u16 {
            let b = [body[o], body[o + 1]];
            if big_endian {
                u16::from_be_bytes(b)
            } else {
                u16::from_le_bytes(b)
            }
        };

        let n_fields = rd_u32(0);
        let payload_stride = rd_u32(4);

        if n_fields > 200 || (8 + n_fields as usize * 16) > body.len() {
            return None;
        }

        let mut fields = Vec::with_capacity(n_fields as usize);
        for i in 0..n_fields as usize {
            let off = 8 + i * 16;
            let type_code = rd_u32(off);
            let name_hash = rd_u32(off + 4);
            let unk = rd_u32(off + 8);
            // offset_word = { u16 byte_offset ; u8 bit_start ; u8 bit_width }
            let byte_offset = rd_u16(off + 12);
            let bit_start = body[off + 14];
            let bit_width = body[off + 15];

            let field_type = SchemaFieldType::from_code(type_code)?;

            fields.push(SchemaField {
                field_type,
                name_hash,
                byte_offset,
                bit_start,
                bit_width,
                unk,
            });
        }

        Some(ComponentSchema {
            payload_stride,
            fields,
        })
    }

    /// Write this schema as a PC (little-endian) `schm` body — the inverse of
    /// [`Self::from_schm_body`] with `big_endian = false`.
    pub fn to_schm_body(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + 16 * self.fields.len());
        out.extend_from_slice(&(self.fields.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.payload_stride.to_le_bytes());
        for f in &self.fields {
            out.extend_from_slice(&(f.field_type as u32).to_le_bytes());
            out.extend_from_slice(&f.name_hash.to_le_bytes());
            out.extend_from_slice(&f.unk.to_le_bytes());
            out.extend_from_slice(&f.byte_offset.to_le_bytes());
            out.push(f.bit_start);
            out.push(f.bit_width);
        }
        out
    }

    /// Read one field of a payload record (PC/LE) at its byte offset, applying its bit field.
    /// `None` if the field runs past the payload.
    pub fn read_field(field: &SchemaField, payload: &[u8]) -> Option<FieldValue> {
        read_field(field, payload)
    }

    /// On-disk `data`-record stride = `4` (the leading `u32` entity key) + [`Self::payload_stride`].
    /// Verified against retail vz.wad: HibernationControl 4+6=10, ModelName 4+4=8, Road 4+40=44.
    pub fn record_stride(&self) -> usize {
        4 + self.payload_stride as usize
    }

    /// True if any field is an inline variable-length string ([`SchemaFieldType::StringRef`],
    /// e.g. `Name`). Such components do **not** use a fixed record stride, so
    /// [`Self::deserialize_records`] cannot iterate them; the exe deserializes them with a
    /// hand-written per-class reader (`Name`/`ModelName` are special-cased in the BE→LE converter
    /// for the same reason). ModelName is *not* variable — its `ModelName`/`0x5b724250` field is a
    /// plain `u32` hash despite the name.
    pub fn is_variable_length(&self) -> bool {
        self.fields
            .iter()
            .any(|f| f.field_type == SchemaFieldType::StringRef)
    }

    /// Deserialize a component's `data` chunk into per-entity records — the faithful analog of the
    /// exe's `CopyFromStream`, driven entirely by this schema. Each on-disk record is
    /// `[u32 entity_key][payload_stride bytes]`; every field is read at
    /// `payload + byte_offset` with the width implied by its [`SchemaFieldType`].
    ///
    /// Returns `None` when the schema is [variable-length](Self::is_variable_length) or when the
    /// data length is not a whole multiple of the record stride (so callers never silently accept a
    /// mis-sized `data` body). `payload_stride == 0` (a pure-sentinel component) yields `Some(vec![])`.
    pub fn deserialize_records(&self, data: &[u8]) -> Option<Vec<ComponentRecord>> {
        if self.is_variable_length() {
            return None;
        }
        if self.payload_stride == 0 {
            return Some(Vec::new());
        }
        let stride = self.record_stride();
        if data.is_empty() || data.len() % stride != 0 {
            return None;
        }
        let mut records = Vec::with_capacity(data.len() / stride);
        for rec in data.chunks_exact(stride) {
            let entity_key = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
            let payload = &rec[4..];
            let mut fields = Vec::with_capacity(self.fields.len());
            for f in &self.fields {
                if let Some(v) = read_field(f, payload) {
                    fields.push((f.name_hash, v));
                }
            }
            records.push(ComponentRecord { entity_key, fields });
        }
        Some(records)
    }
}

/// A single deserialized field value, typed by the schema's [`SchemaFieldType`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldValue {
    /// A bit field ([`SchemaField::bit_width`] > 0): its bits, shifted down to bit 0.
    Bits(u32),
    /// Codes 1/2 — one byte.
    U8(u8),
    /// Codes 3/4 — two bytes.
    U16(u16),
    /// Code 7 — `f32`.
    F32(f32),
    /// Codes 5/6/8/9 — a 32-bit word. Kept raw; the distinction is the field's
    /// [`SchemaFieldType`].
    U32(u32),
    /// Code 10 — `[f32; 3]`.
    Vec3([f32; 3]),
    /// Code 11 — `[f32; 8]` (e.g. a Transform pos+quat blob).
    Blob32([f32; 8]),
}

/// One entity's deserialized component instance: its key plus `(field-name-hash, value)` pairs in
/// schema order.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRecord {
    pub entity_key: u32,
    pub fields: Vec<(u32, FieldValue)>,
}

impl ComponentRecord {
    /// Look up a field value by its name-hash (`pandemic_hash_m2(field_name)`).
    pub fn get(&self, name_hash: u32) -> Option<FieldValue> {
        self.fields
            .iter()
            .find(|(h, _)| *h == name_hash)
            .map(|(_, v)| *v)
    }
}

fn rd_u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn rd_u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(o)?,
        *b.get(o + 1)?,
        *b.get(o + 2)?,
        *b.get(o + 3)?,
    ]))
}
fn rd_f32le(b: &[u8], o: usize) -> Option<f32> {
    Some(f32::from_bits(rd_u32le(b, o)?))
}

/// Read one field from a payload record slice (PC/LE). Returns `None` if it would run past the end.
fn read_field(f: &SchemaField, payload: &[u8]) -> Option<FieldValue> {
    let o = f.byte_offset as usize;
    if f.bit_width > 0 {
        let w = f.field_type.byte_width();
        let mut unit = 0u64;
        for i in 0..w.min(4) {
            unit |= (*payload.get(o + i)? as u64) << (8 * i);
        }
        let mask = (1u64 << f.bit_width) - 1;
        return Some(FieldValue::Bits(((unit >> f.bit_start) & mask) as u32));
    }
    Some(match f.field_type {
        SchemaFieldType::Byte | SchemaFieldType::U8 => FieldValue::U8(*payload.get(o)?),
        SchemaFieldType::Short | SchemaFieldType::U16 => FieldValue::U16(rd_u16le(payload, o)?),
        SchemaFieldType::F32 => FieldValue::F32(rd_f32le(payload, o)?),
        SchemaFieldType::Int
        | SchemaFieldType::Hash
        | SchemaFieldType::StringRef
        | SchemaFieldType::Enum => FieldValue::U32(rd_u32le(payload, o)?),
        SchemaFieldType::Vec3 => FieldValue::Vec3([
            rd_f32le(payload, o)?,
            rd_f32le(payload, o + 4)?,
            rd_f32le(payload, o + 8)?,
        ]),
        SchemaFieldType::Blob32 => {
            let mut a = [0f32; 8];
            for (i, s) in a.iter_mut().enumerate() {
                *s = rd_f32le(payload, o + i * 4)?;
            }
            FieldValue::Blob32(a)
        }
    })
}

// ---------------------------------------------------------------------------
// COMP-group walker — the `FUN_00654940` `COMP` arm in Rust: group a UCFX
// container's descriptor table into {info, schm, data} triples so a schema can
// be married to its data (and to the class name/type-hash from `info`).
// ---------------------------------------------------------------------------

/// One `COMP` group inside a UCFX container: the component's `info` (name + type-hash), its
/// field `schm`, and its packed `data`. Mirrors the exe's COMP subtree (`info`/`schm`/`data`).
#[derive(Debug, Clone)]
pub struct CompGroup {
    /// ASCII class name from the `info` body, when present (`[name\0][u32 type_hash]…`).
    pub name: Option<String>,
    /// The component type-hash (`pandemic_hash_m2(name)`) read from the `info` body: the u32 that
    /// follows the name (ASCII form) or the leading u32 (compact 16-byte form).
    pub type_hash: Option<u32>,
    pub info: Vec<u8>,
    pub schm: Option<Vec<u8>>,
    pub data: Option<Vec<u8>>,
}

impl CompGroup {
    /// Parse this group's `schm` into a [`ComponentSchema`] (PC/LE).
    pub fn schema(&self) -> Option<ComponentSchema> {
        ComponentSchema::from_schm_body(self.schm.as_deref()?, false)
    }
}

/// Walk a UCFX container's 20-byte descriptor table into its `COMP` groups. A `COMP` group starts
/// at a `COMP` sentinel row (`row_u0 == 0xFFFF_FFFF`) and runs to the next sentinel, collecting the
/// `info`/`schm`/`data` child bodies. Non-COMP top-level descriptors are ignored. Returns an empty
/// vec for non-UCFX / malformed input (never panics on bounds).
pub fn parse_comp_groups(container: &[u8]) -> Vec<CompGroup> {
    let mut out = Vec::new();
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return out;
    }
    let rd = |o: usize| -> u32 {
        u32::from_le_bytes([
            container[o],
            container[o + 1],
            container[o + 2],
            container[o + 3],
        ])
    };
    let data_area_off = rd(4) as usize;
    let n_desc = rd(16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return out;
    }
    let data_start = if data_area_off > 0 { data_area_off } else { 8 };
    let body = |u0: usize, sz: usize| -> Option<Vec<u8>> {
        if u0 == 0xFFFF_FFFF {
            return None;
        }
        let s = data_start.checked_add(u0)?;
        let e = s.checked_add(sz)?;
        (e <= container.len()).then(|| container[s..e].to_vec())
    };

    let mut i = 0;
    while i < n_desc {
        let ro = 20 + i * 20;
        let tag = &container[ro..ro + 4];
        let u0 = rd(ro + 4) as usize;
        if tag == b"COMP" && u0 == 0xFFFF_FFFF {
            let mut group = CompGroup {
                name: None,
                type_hash: None,
                info: Vec::new(),
                schm: None,
                data: None,
            };
            i += 1;
            while i < n_desc {
                let ro2 = 20 + i * 20;
                let t2 = &container[ro2..ro2 + 4];
                let u02 = rd(ro2 + 4) as usize;
                if u02 == 0xFFFF_FFFF {
                    break; // next sentinel ends the group
                }
                let sz2 = rd(ro2 + 8) as usize;
                match t2 {
                    b"info" => {
                        if let Some(b) = body(u02, sz2) {
                            parse_info(&b, &mut group);
                            group.info = b;
                        }
                    }
                    b"schm" => group.schm = body(u02, sz2),
                    b"data" => group.data = body(u02, sz2),
                    _ => {}
                }
                i += 1;
            }
            out.push(group);
        } else {
            i += 1;
        }
    }
    out
}

/// Parse a COMP `info` body: `[name\0][u32 type_hash]…` (ASCII) or `[u32 type_hash]…` (compact).
fn parse_info(info: &[u8], group: &mut CompGroup) {
    if let Some(nul) = info.iter().position(|&x| x == 0) {
        if nul > 0 && info[..nul].iter().all(|&x| (32..127).contains(&x)) {
            group.name = Some(String::from_utf8_lossy(&info[..nul]).into_owned());
            group.type_hash = rd_u32le(info, nul + 1);
            return;
        }
    }
    // Compact binary form: leading u32 is the type-hash.
    group.type_hash = rd_u32le(info, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_type_from_code_all_valid() {
        let all = [
            (1, SchemaFieldType::Byte),
            (2, SchemaFieldType::U8),
            (3, SchemaFieldType::Short),
            (4, SchemaFieldType::U16),
            (5, SchemaFieldType::Int),
            (6, SchemaFieldType::Hash),
            (7, SchemaFieldType::F32),
            (8, SchemaFieldType::StringRef),
            (9, SchemaFieldType::Enum),
            (10, SchemaFieldType::Vec3),
            (11, SchemaFieldType::Blob32),
        ];
        for (code, t) in all {
            assert_eq!(SchemaFieldType::from_code(code), Some(t));
            assert_eq!(t as u32, code);
        }
    }

    #[test]
    fn field_type_from_code_invalid() {
        assert_eq!(SchemaFieldType::from_code(0), None);
        assert_eq!(SchemaFieldType::from_code(12), None);
        assert_eq!(SchemaFieldType::from_code(0xFFFFFFFF), None);
    }

    /// The widths the engine's readers consume (`FUN_00656210`: 1/2 → 1 byte, 3/4 → 2, 5/6/9 → 4,
    /// 7 → f32; `FUN_00656610` → 12; `FUN_0065644a` → 32).
    #[test]
    fn field_type_byte_width() {
        assert_eq!(SchemaFieldType::Byte.byte_width(), 1);
        assert_eq!(SchemaFieldType::U8.byte_width(), 1);
        assert_eq!(SchemaFieldType::Short.byte_width(), 2);
        assert_eq!(SchemaFieldType::U16.byte_width(), 2);
        assert_eq!(SchemaFieldType::Int.byte_width(), 4);
        assert_eq!(SchemaFieldType::Hash.byte_width(), 4);
        assert_eq!(SchemaFieldType::F32.byte_width(), 4);
        assert_eq!(SchemaFieldType::StringRef.byte_width(), 4);
        assert_eq!(SchemaFieldType::Enum.byte_width(), 4);
        assert_eq!(SchemaFieldType::Vec3.byte_width(), 12);
        assert_eq!(SchemaFieldType::Blob32.byte_width(), 32);
    }

    #[test]
    fn field_type_needs_swap() {
        assert!(!SchemaFieldType::Byte.needs_swap());
        assert!(!SchemaFieldType::U8.needs_swap());
        assert!(SchemaFieldType::Short.needs_swap());
        assert!(SchemaFieldType::U16.needs_swap());
        assert!(SchemaFieldType::F32.needs_swap());
        assert!(SchemaFieldType::Hash.needs_swap());
        assert!(SchemaFieldType::Vec3.needs_swap());
        assert!(SchemaFieldType::Blob32.needs_swap());
    }

    #[test]
    fn field_type_swap_unit() {
        assert_eq!(SchemaFieldType::Short.swap_unit(), 2);
        assert_eq!(SchemaFieldType::U16.swap_unit(), 2);
        assert_eq!(SchemaFieldType::F32.swap_unit(), 4);
        assert_eq!(SchemaFieldType::Int.swap_unit(), 4);
        assert_eq!(SchemaFieldType::Vec3.swap_unit(), 4);
    }

    #[test]
    fn field_type_swap_count() {
        assert_eq!(SchemaFieldType::U16.swap_count(), 1);
        assert_eq!(SchemaFieldType::F32.swap_count(), 1);
        assert_eq!(SchemaFieldType::Vec3.swap_count(), 3);
        assert_eq!(SchemaFieldType::Blob32.swap_count(), 8);
    }

    #[test]
    fn to_schm_body_inverts_from_schm_body() {
        let mut body = vec![2u8, 0, 0, 0, 8, 0, 0, 0];
        body.extend_from_slice(&[7, 0, 0, 0, 0x9d, 0x91, 0x22, 0xd1, 0, 0, 0, 0, 0, 0, 0, 0]);
        body.extend_from_slice(&[5, 0, 0, 0, 0x72, 0xfd, 0xa0, 0x8c, 0, 0, 0, 0, 4, 0, 2, 1]);
        let s = ComponentSchema::from_schm_body(&body, false).unwrap();
        assert_eq!(s.fields[1].bit_start, 2);
        assert_eq!(s.fields[1].bit_width, 1);
        assert_eq!(s.to_schm_body(), body);
    }

    #[test]
    fn from_schm_body_too_small() {
        assert!(ComponentSchema::from_schm_body(&[], false).is_none());
        assert!(ComponentSchema::from_schm_body(&[0, 0, 0], false).is_none());
    }

    #[test]
    fn from_schm_body_zero_fields() {
        let body = [
            0, 0, 0, 0, // n_fields = 0
            100, 0, 0, 0, // payload_stride = 100
        ];
        let schema = ComponentSchema::from_schm_body(&body, false);
        assert!(schema.is_some());
        let schema_unwrapped = schema.unwrap();
        assert_eq!(schema_unwrapped.payload_stride, 100);
        assert_eq!(schema_unwrapped.fields.len(), 0);
    }

    #[test]
    fn from_schm_body_too_many_fields() {
        let mut body = vec![
            200, 0, 0, 0, // n_fields = 200 (max boundary)
            100, 0, 0, 0, // payload_stride = 100
        ];
        // Add 200 * 16 = 3200 bytes of field data
        body.resize(8 + 201 * 16, 0);
        assert!(ComponentSchema::from_schm_body(&body, false).is_none());
    }

    #[test]
    fn from_schm_body_insufficient_data() {
        let body = [
            1, 0, 0, 0, // n_fields = 1
            100, 0, 0, 0, // payload_stride = 100
        ];
        // Only 8 bytes, but need 8 + 16 = 24
        assert!(ComponentSchema::from_schm_body(&body, false).is_none());
    }

    #[test]
    fn from_schm_body_le() {
        // Retail PC (LE): offset_word = { u16 byte_offset ; u8 bit_start ; u8 bit_width }.
        // byte_offset is the LOW 16 bits — bytes [10,0] => 10; bit_start=5, bit_width=1.
        let body = [
            1, 0, 0, 0, // n_fields = 1
            100, 0, 0, 0, // payload_stride = 100
            6, 0, 0, 0, // type_code = 6 (Hash)
            0x78, 0x56, 0x34, 0x12, // name_hash = 0x12345678
            0, 0, 0, 0, // unk
            10, 0, 5, 1, // offset_word: byte_offset=10, bit_start=5, bit_width=1
        ];
        let schema = ComponentSchema::from_schm_body(&body, false).unwrap();
        assert_eq!(schema.payload_stride, 100);
        assert_eq!(schema.fields.len(), 1);
        let field = &schema.fields[0];
        assert_eq!(field.field_type, SchemaFieldType::Hash);
        assert_eq!(field.name_hash, 0x12345678);
        assert_eq!(field.byte_offset, 10);
        assert_eq!(field.bit_start, 5);
        assert_eq!(field.bit_width, 1);
    }

    /// The offset word's byte_offset lives in the source endianness's first two bytes: BE reads
    /// the *same* two bytes big-endian (the historical `raw>>16` result), LE reads them
    /// little-endian. Same on-disk bytes `[0x00,0x24,bit,meta]` => BE 0x0024=36, LE 0x2400.
    #[test]
    fn from_schm_body_byte_offset_is_endian_aware() {
        // Header (n_fields=1, stride=8) and the type/name/unk words are read in the source
        // endianness, so build them accordingly.
        let mk = |be: bool, ow: [u8; 4]| {
            let w = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
            let mut b = Vec::new();
            b.extend_from_slice(&w(1)); // n_fields
            b.extend_from_slice(&w(8)); // payload_stride
            b.extend_from_slice(&w(6)); // type_code = 6 (Hash)
            b.extend_from_slice(&w(0)); // name_hash
            b.extend_from_slice(&w(0)); // unk
            b.extend_from_slice(&ow); // offset_word (raw bytes)
            b
        };
        // BE source: byte_offset in the two bytes read big-endian → 0x00,0x24 = 0x0024 = 36.
        let be = ComponentSchema::from_schm_body(&mk(true, [0x00, 0x24, 0, 0]), true).unwrap();
        assert_eq!(be.fields[0].byte_offset, 0x0024);
        // LE source: same two bytes read little-endian → 0x24,0x00 = 0x0024 = 36.
        let le = ComponentSchema::from_schm_body(&mk(false, [0x24, 0x00, 0, 0]), false).unwrap();
        assert_eq!(le.fields[0].byte_offset, 0x0024);
    }

    /// Build a synthetic HibernationControl-shaped schm (the exact retail field layout) and
    /// deserialize one `[u32 key][payload:6]` record. Grounds the deserializer without the WAD.
    #[test]
    fn deserialize_hibernation_shaped_record() {
        // fields: u16@0, u8@2, u8@3, u8@4, 1-bit@5 start 0, 1-bit@5 start 1; payload_stride = 6.
        let mut schm = vec![6u8, 0, 0, 0, /*stride*/ 6, 0, 0, 0];
        let field = |tc: u8, nh: u32, off: u16, start: u8, width: u8| {
            let mut e = vec![tc, 0, 0, 0];
            e.extend_from_slice(&nh.to_le_bytes());
            e.extend_from_slice(&0u32.to_le_bytes());
            e.extend_from_slice(&off.to_le_bytes());
            e.push(start);
            e.push(width);
            e
        };
        schm.extend(field(4, 0xAAAA_0000, 0, 0, 0)); // u16 dist0
        schm.extend(field(2, 0xAAAA_0001, 2, 0, 0)); // u8 dist1
        schm.extend(field(2, 0xAAAA_0002, 3, 0, 0)); // u8 dist2
        schm.extend(field(2, 0xAAAA_0003, 4, 0, 0)); // u8 dist3
        schm.extend(field(1, 0xAAAA_0004, 5, 0, 1)); // bit 0 of byte 5
        schm.extend(field(1, 0xAAAA_0005, 5, 1, 1)); // bit 1 of byte 5

        let s = ComponentSchema::from_schm_body(&schm, false).unwrap();
        assert_eq!(s.payload_stride, 6);
        assert_eq!(s.record_stride(), 10);
        assert!(!s.is_variable_length());

        // One record: key=0x12345678, payload = [dist0=590(LE), 160, 60, 20, 0b10]
        let mut data = 0x1234_5678u32.to_le_bytes().to_vec();
        data.extend_from_slice(&[0x4e, 0x02, 160, 60, 20, 0b0000_0010]);
        let recs = s.deserialize_records(&data).unwrap();
        assert_eq!(recs.len(), 1);
        let r = &recs[0];
        assert_eq!(r.entity_key, 0x1234_5678);
        assert_eq!(r.get(0xAAAA_0000), Some(FieldValue::U16(590)));
        assert_eq!(r.get(0xAAAA_0001), Some(FieldValue::U8(160)));
        assert_eq!(r.get(0xAAAA_0003), Some(FieldValue::U8(20)));
        assert_eq!(r.get(0xAAAA_0004), Some(FieldValue::Bits(0))); // bit 0 of 0b10
        assert_eq!(r.get(0xAAAA_0005), Some(FieldValue::Bits(1))); // bit 1 of 0b10
    }

    #[test]
    fn deserialize_rejects_missized_and_variable() {
        // Fixed schema, but data length not a multiple of the record stride → None.
        let schm = vec![0u8, 0, 0, 0, 4, 0, 0, 0]; // 0 fields, stride 4 → record 8
        let s = ComponentSchema::from_schm_body(&schm, false).unwrap();
        assert!(s.deserialize_records(&[0u8; 7]).is_none());
        assert_eq!(s.deserialize_records(&[0u8; 8]).unwrap().len(), 1);

        // A StringRef field marks the schema variable-length (Name) → deserialize_records None.
        let mut v = vec![1u8, 0, 0, 0, 4, 0, 0, 0];
        v.extend_from_slice(&[8, 0, 0, 0]); // type 8 = StringRef
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&[0, 0, 0, 0]);
        let sv = ComponentSchema::from_schm_body(&v, false).unwrap();
        assert!(sv.is_variable_length());
        assert!(sv.deserialize_records(&[0u8; 16]).is_none());
    }

    #[test]
    fn comp_group_walker_synthetic() {
        // Build a minimal UCFX container with one COMP group {info, schm, data}.
        // Header: "UCFX", data_area_off, unk, unk, n_desc; then 20B descriptor rows; then bodies.
        let name = b"ModelName\0";
        let info: Vec<u8> = name
            .iter()
            .copied()
            .chain(0x5b72_4250u32.to_le_bytes()) // type_hash right after the name
            .collect();
        // schm: 1 field, type6 @0, stride 4.
        let mut schm = vec![1u8, 0, 0, 0, 4, 0, 0, 0];
        schm.extend_from_slice(&[6, 0, 0, 0]);
        schm.extend_from_slice(&0x5b72_4250u32.to_le_bytes());
        schm.extend_from_slice(&0u32.to_le_bytes());
        schm.extend_from_slice(&[0, 0, 0, 0]);
        // data: one record [key=0x143b10][hash=0xdad8a613]
        let mut data = 0x0014_3b10u32.to_le_bytes().to_vec();
        data.extend_from_slice(&0xdad8_a613u32.to_le_bytes());

        let n_desc = 4u32; // COMP sentinel + info + schm + data
        let table_end = 20 + n_desc as usize * 20;
        let mut c = Vec::new();
        c.extend_from_slice(b"UCFX");
        c.extend_from_slice(&(table_end as u32).to_le_bytes()); // data_area_off
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&n_desc.to_le_bytes());
        let push_row = |c: &mut Vec<u8>, tag: &[u8; 4], u0: u32, sz: u32| {
            c.extend_from_slice(tag);
            c.extend_from_slice(&u0.to_le_bytes());
            c.extend_from_slice(&sz.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
        };
        // Bodies are concatenated after data_area_off; row_u0 is each body's offset from there.
        let info_off = 0u32;
        let schm_off = info.len() as u32;
        let data_off = (info.len() + schm.len()) as u32;
        push_row(&mut c, b"COMP", 0xFFFF_FFFF, 0);
        push_row(&mut c, b"info", info_off, info.len() as u32);
        push_row(&mut c, b"schm", schm_off, schm.len() as u32);
        push_row(&mut c, b"data", data_off, data.len() as u32);
        c.extend_from_slice(&info);
        c.extend_from_slice(&schm);
        c.extend_from_slice(&data);

        let groups = parse_comp_groups(&c);
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.name.as_deref(), Some("ModelName"));
        assert_eq!(g.type_hash, Some(0x5b72_4250));
        let s = g.schema().unwrap();
        let recs = s.deserialize_records(g.data.as_ref().unwrap()).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].entity_key, 0x0014_3b10);
        assert_eq!(recs[0].get(0x5b72_4250), Some(FieldValue::U32(0xdad8_a613)));
    }

    #[cfg(feature = "retail")]
    mod retail {
        use super::*;

        // -----------------------------------------------------------------------
        // Live end-to-end test against retail vz.wad. Game-gated: built by the `retail`
        // feature, reads the vz.wad named by the repo-root .mercs2-local.toml, and fails
        // if it is absent.
        // Walks real blocks, finds representative components (HibernationControl,
        // ModelName, FactionMarker, Road, Transform), and deserializes them through
        // the schema, asserting concrete field values / invariants.
        // -----------------------------------------------------------------------
        #[test]
        fn live_deserialize_representative_components_if_wad_present() {
            use crate::ffcs::load_ffcs_archive;
            use crate::sges::decompress_block;

            let path =
                crate::game_paths::local_config_vz_wad(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
                    .unwrap_or_else(|e| panic!("{e}"));
            let mut f = std::fs::File::open(&path)
                .unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
            let size = f.metadata().unwrap().len();
            let arch = load_ffcs_archive(&mut f, size).expect("ffcs");

            // Collect the first COMP group (with schm+data) for each target class name.
            let targets = [
                "HibernationControl",
                "ModelName",
                "FactionMarker",
                "Road",
                "Transform",
            ];
            let mut found: std::collections::HashMap<String, CompGroup> =
                std::collections::HashMap::new();

            'outer: for bi in 0..arch.indx.len() {
                if found.len() == targets.len() {
                    break;
                }
                let dec = decompress_block(&mut f, &arch.indx, bi as u16)
                    .unwrap_or_else(|e| panic!("decompress block {bi}: {e}"));
                assert!(dec.len() >= 4, "block {bi}: {} bytes, too short for an entry count", dec.len());
                let count = u32::from_le_bytes([dec[0], dec[1], dec[2], dec[3]]) as usize;
                let mut pos = 4 + count * 16;
                for ei in 0..count {
                    let base = 4 + ei * 16;
                    assert!(
                        base + 16 <= dec.len(),
                        "block {bi}: entry {ei} of {count} runs past the {}-byte block",
                        dec.len()
                    );
                    let chunk_size = u32::from_le_bytes([
                        dec[base + 12],
                        dec[base + 13],
                        dec[base + 14],
                        dec[base + 15],
                    ]) as usize;
                    assert!(
                        pos + chunk_size <= dec.len(),
                        "block {bi}: entry {ei} container ({chunk_size} bytes at {pos}) runs past the {}-byte block",
                        dec.len()
                    );
                    let container = &dec[pos..pos + chunk_size];
                    pos += chunk_size;
                    for g in parse_comp_groups(container) {
                        if let Some(name) = g.name.clone() {
                            if targets.contains(&name.as_str())
                                && g.schm.is_some()
                                && g.data.as_ref().is_some_and(|d| !d.is_empty())
                                && !found.contains_key(&name)
                            {
                                found.insert(name, g);
                                if found.len() == targets.len() {
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }

            // HibernationControl: stride 6, fields u16@0,u8@2,u8@3,u8@4,bit@5,bit@5 (world_streaming spec).
            if let Some(g) = found.get("HibernationControl") {
                let s = g.schema().expect("hib schema");
                assert_eq!(
                    s.payload_stride, 6,
                    "HibernationControl descriptor stride is 6"
                );
                assert_eq!(s.record_stride(), 10);
                assert_eq!(s.fields.len(), 6);
                assert_eq!(s.fields[0].field_type, SchemaFieldType::U16);
                assert_eq!(s.fields[0].byte_offset, 0);
                assert_eq!(s.fields[4].field_type, SchemaFieldType::Byte);
                assert_eq!((s.fields[4].bit_width, s.fields[5].bit_width), (1, 1));
                assert_eq!(s.fields[4].byte_offset, 5);
                assert_eq!(s.fields[5].byte_offset, 5);
                assert_ne!(
                    s.fields[4].bit_start, s.fields[5].bit_start,
                    "two bits packed in byte 5"
                );
                let recs = s
                    .deserialize_records(g.data.as_ref().unwrap())
                    .expect("hib records");
                assert!(!recs.is_empty());
                // Every record must yield all 6 typed fields.
                for r in &recs {
                    assert_eq!(r.fields.len(), 6);
                    assert!(matches!(
                        r.get(s.fields[0].name_hash),
                        Some(FieldValue::U16(_))
                    ));
                    assert!(matches!(
                        r.get(s.fields[4].name_hash),
                        Some(FieldValue::Bits(_))
                    ));
                }
            } else {
                panic!("HibernationControl not found in retail vz.wad");
            }

            // ModelName: single u32 hash field named 0x5b724250 (== Model), stride 4.
            if let Some(g) = found.get("ModelName") {
                let s = g.schema().expect("modelname schema");
                assert_eq!(s.payload_stride, 4);
                assert_eq!(s.fields.len(), 1);
                assert_eq!(s.fields[0].field_type, SchemaFieldType::Hash);
                assert_eq!(s.fields[0].name_hash, 0x5b72_4250);
                let recs = s
                    .deserialize_records(g.data.as_ref().unwrap())
                    .expect("modelname records");
                assert!(!recs.is_empty());
                // Each record's model hash is a non-zero u32.
                for r in &recs {
                    match r.get(0x5b72_4250) {
                        Some(FieldValue::U32(h)) => assert_ne!(h, 0),
                        other => panic!("expected model hash u32, got {other:?}"),
                    }
                }
            } else {
                panic!("ModelName not found in retail vz.wad");
            }

            // FactionMarker: single u32 field, stride 4 (matches the descriptor stride in the code map).
            if let Some(g) = found.get("FactionMarker") {
                let s = g.schema().expect("factionmarker schema");
                assert_eq!(s.payload_stride, 4, "FactionMarker descriptor stride is 4");
                assert_eq!(s.fields.len(), 1);
                let recs = s
                    .deserialize_records(g.data.as_ref().unwrap())
                    .expect("faction records");
                assert!(!recs.is_empty());
                assert!(matches!(
                    recs[0].get(s.fields[0].name_hash),
                    Some(FieldValue::U32(_))
                ));
            } else {
                panic!("FactionMarker not found in retail vz.wad");
            }

            // Road: 4×u32 + 2×vec3, stride 40; the vec3 fields must decode to finite floats.
            if let Some(g) = found.get("Road") {
                let s = g.schema().expect("road schema");
                assert_eq!(s.payload_stride, 40, "Road stride 40 (4×u32 + 2×vec3)");
                assert_eq!(
                    s.fields
                        .iter()
                        .filter(|f| f.field_type == SchemaFieldType::Vec3)
                        .count(),
                    2
                );
                let recs = s
                    .deserialize_records(g.data.as_ref().unwrap())
                    .expect("road records");
                assert!(!recs.is_empty());
                for f in s
                    .fields
                    .iter()
                    .filter(|f| f.field_type == SchemaFieldType::Vec3)
                {
                    if let Some(FieldValue::Vec3(v)) = recs[0].get(f.name_hash) {
                        assert!(v.iter().all(|c| c.is_finite()), "road vec3 finite");
                    } else {
                        panic!("road vec3 field missing");
                    }
                }
            } else {
                panic!("Road not found in retail vz.wad");
            }

            // Transform: type11 blob@0 (32B) + code-5 int@32 + 8×u16@36..50 — assert the SCHEMA layout only.
            // Transform's on-disk `data` record is written by a special CHDR-gated builder (0x0063D7C0),
            // not the generic [key][payload] path, so its record stride is validated live (confirm-live),
            // not asserted here.
            if let Some(g) = found.get("Transform") {
                let s = g.schema().expect("transform schema");
                assert_eq!(s.fields[0].field_type, SchemaFieldType::Blob32);
                assert_eq!(s.fields[0].byte_offset, 0);
                assert_eq!(s.fields[1].field_type, SchemaFieldType::Int);
                assert_eq!(s.fields[1].byte_offset, 32);
                // The u16 tail is strictly monotonically increasing by 2 (36,38,…) — proves LOW-16 offsets.
                let u16s: Vec<u16> = s
                    .fields
                    .iter()
                    .filter(|f| f.field_type == SchemaFieldType::U16)
                    .map(|f| f.byte_offset)
                    .collect();
                for w in u16s.windows(2) {
                    assert_eq!(w[1] - w[0], 2, "Transform u16 fields are 2 bytes apart");
                }
            } else {
                panic!("Transform not found in retail vz.wad");
            }
        }
    }
}
