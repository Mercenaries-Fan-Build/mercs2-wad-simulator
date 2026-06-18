/// ECS COMP schema field type codes, reverse-engineered from schm entries.
/// See docs/schm_type_codes.md for derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SchemaFieldType {
    /// Type 1: Sub-byte bit field. No swap needed.
    Bit = 1,
    /// Type 2: Single byte (u8). No swap needed.
    U8 = 2,
    /// Type 4: Two bytes (u16). Swap 2 bytes.
    U16 = 4,
    /// Type 5: Four bytes float (f32). Swap 4 bytes.
    F32 = 5,
    /// Type 6: Four bytes unsigned (u32/hash). Swap 4 bytes.
    U32 = 6,
    /// Type 7: Four bytes reference (u32). Swap 4 bytes.
    Ref = 7,
    /// Type 8: Four bytes string reference. Swap 4 bytes.
    StringRef = 8,
    /// Type 9: Four bytes flags/bitfield stored as u32. Swap 4 bytes.
    Flags = 9,
    /// Type 10: 12 bytes Vec3 (3 × f32). Swap as 3 × 4 bytes.
    Vec3 = 10,
    /// Type 11: 32 bytes composite (8 × f32, e.g. Transform pos+quat blob). Swap as 8 × 4 bytes.
    Blob32 = 11,
}

impl SchemaFieldType {
    /// Try to parse a raw type code from schm entry.
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::Bit),
            2 => Some(Self::U8),
            4 => Some(Self::U16),
            5 => Some(Self::F32),
            6 => Some(Self::U32),
            7 => Some(Self::Ref),
            8 => Some(Self::StringRef),
            9 => Some(Self::Flags),
            10 => Some(Self::Vec3),
            11 => Some(Self::Blob32),
            _ => None,
        }
    }

    /// Total byte width of this field type.
    pub fn byte_width(&self) -> usize {
        match self {
            Self::Bit => 0,
            Self::U8 => 1,
            Self::U16 => 2,
            Self::F32 | Self::U32 | Self::Ref | Self::StringRef | Self::Flags => 4,
            Self::Vec3 => 12,
            Self::Blob32 => 32,
        }
    }

    /// Whether this field needs byte-swapping for BE→LE conversion.
    pub fn needs_swap(&self) -> bool {
        match self {
            Self::Bit | Self::U8 => false,
            _ => true,
        }
    }

    /// The atomic swap unit size (bytes per swap operation).
    /// Vec3 and Blob32 are swapped as multiple 4-byte units.
    pub fn swap_unit(&self) -> usize {
        match self {
            Self::Bit | Self::U8 => 0,
            Self::U16 => 2,
            _ => 4,
        }
    }

    /// Number of swap operations needed for this field.
    pub fn swap_count(&self) -> usize {
        match self {
            Self::Bit | Self::U8 => 0,
            Self::U16 => 1,
            Self::F32 | Self::U32 | Self::Ref | Self::StringRef | Self::Flags => 1,
            Self::Vec3 => 3,
            Self::Blob32 => 8,
        }
    }
}

/// A parsed schema field entry from a `schm` chunk.
#[derive(Debug, Clone)]
pub struct SchemaField {
    pub field_type: SchemaFieldType,
    pub name_hash: u32,
    pub flags: u16,
    pub byte_offset: u16,
}

/// Parsed schema for an ECS component.
#[derive(Debug, Clone)]
pub struct ComponentSchema {
    pub payload_stride: u32,
    pub fields: Vec<SchemaField>,
}

impl ComponentSchema {
    /// Parse a schm chunk body into a ComponentSchema.
    /// Reads n_fields and payload_stride from the header, then field entries.
    pub fn from_schm_body(body: &[u8], big_endian: bool) -> Option<Self> {
        if body.len() < 8 {
            return None;
        }

        let (n_fields, payload_stride) = if big_endian {
            (
                u32::from_be_bytes([body[0], body[1], body[2], body[3]]),
                u32::from_be_bytes([body[4], body[5], body[6], body[7]]),
            )
        } else {
            (
                u32::from_le_bytes([body[0], body[1], body[2], body[3]]),
                u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
            )
        };

        if n_fields > 200 || (8 + n_fields as usize * 16) > body.len() {
            return None;
        }

        let mut fields = Vec::with_capacity(n_fields as usize);
        for i in 0..n_fields as usize {
            let off = 8 + i * 16;
            let (type_code, name_hash, _unk, raw_offset) = if big_endian {
                (
                    u32::from_be_bytes([body[off], body[off+1], body[off+2], body[off+3]]),
                    u32::from_be_bytes([body[off+4], body[off+5], body[off+6], body[off+7]]),
                    u32::from_be_bytes([body[off+8], body[off+9], body[off+10], body[off+11]]),
                    u32::from_be_bytes([body[off+12], body[off+13], body[off+14], body[off+15]]),
                )
            } else {
                (
                    u32::from_le_bytes([body[off], body[off+1], body[off+2], body[off+3]]),
                    u32::from_le_bytes([body[off+4], body[off+5], body[off+6], body[off+7]]),
                    u32::from_le_bytes([body[off+8], body[off+9], body[off+10], body[off+11]]),
                    u32::from_le_bytes([body[off+12], body[off+13], body[off+14], body[off+15]]),
                )
            };

            let byte_offset = ((raw_offset >> 16) & 0xFFFF) as u16;
            let flags = (raw_offset & 0xFFFF) as u16;

            let field_type = match SchemaFieldType::from_code(type_code) {
                Some(ft) => ft,
                None => return None,
            };

            fields.push(SchemaField {
                field_type,
                name_hash,
                flags,
                byte_offset,
            });
        }

        Some(ComponentSchema {
            payload_stride,
            fields,
        })
    }
}
