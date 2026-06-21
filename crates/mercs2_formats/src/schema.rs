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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_type_from_code_all_valid() {
        assert_eq!(SchemaFieldType::from_code(1), Some(SchemaFieldType::Bit));
        assert_eq!(SchemaFieldType::from_code(2), Some(SchemaFieldType::U8));
        assert_eq!(SchemaFieldType::from_code(4), Some(SchemaFieldType::U16));
        assert_eq!(SchemaFieldType::from_code(5), Some(SchemaFieldType::F32));
        assert_eq!(SchemaFieldType::from_code(6), Some(SchemaFieldType::U32));
        assert_eq!(SchemaFieldType::from_code(7), Some(SchemaFieldType::Ref));
        assert_eq!(SchemaFieldType::from_code(8), Some(SchemaFieldType::StringRef));
        assert_eq!(SchemaFieldType::from_code(9), Some(SchemaFieldType::Flags));
        assert_eq!(SchemaFieldType::from_code(10), Some(SchemaFieldType::Vec3));
        assert_eq!(SchemaFieldType::from_code(11), Some(SchemaFieldType::Blob32));
    }

    #[test]
    fn field_type_from_code_invalid() {
        assert_eq!(SchemaFieldType::from_code(0), None);
        assert_eq!(SchemaFieldType::from_code(3), None);
        assert_eq!(SchemaFieldType::from_code(12), None);
        assert_eq!(SchemaFieldType::from_code(0xFFFFFFFF), None);
    }

    #[test]
    fn field_type_byte_width() {
        assert_eq!(SchemaFieldType::Bit.byte_width(), 0);
        assert_eq!(SchemaFieldType::U8.byte_width(), 1);
        assert_eq!(SchemaFieldType::U16.byte_width(), 2);
        assert_eq!(SchemaFieldType::F32.byte_width(), 4);
        assert_eq!(SchemaFieldType::U32.byte_width(), 4);
        assert_eq!(SchemaFieldType::Ref.byte_width(), 4);
        assert_eq!(SchemaFieldType::StringRef.byte_width(), 4);
        assert_eq!(SchemaFieldType::Flags.byte_width(), 4);
        assert_eq!(SchemaFieldType::Vec3.byte_width(), 12);
        assert_eq!(SchemaFieldType::Blob32.byte_width(), 32);
    }

    #[test]
    fn field_type_needs_swap() {
        assert!(!SchemaFieldType::Bit.needs_swap());
        assert!(!SchemaFieldType::U8.needs_swap());
        assert!(SchemaFieldType::U16.needs_swap());
        assert!(SchemaFieldType::F32.needs_swap());
        assert!(SchemaFieldType::U32.needs_swap());
        assert!(SchemaFieldType::Vec3.needs_swap());
        assert!(SchemaFieldType::Blob32.needs_swap());
    }

    #[test]
    fn field_type_swap_unit() {
        assert_eq!(SchemaFieldType::U16.swap_unit(), 2);
        assert_eq!(SchemaFieldType::F32.swap_unit(), 4);
        assert_eq!(SchemaFieldType::U32.swap_unit(), 4);
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
        let body = [
            1, 0, 0, 0, // n_fields = 1
            100, 0, 0, 0, // payload_stride = 100
            // One field: type=6 (U32), name_hash=0x12345678, unk=0, offset=(10 << 16) | 5
            6, 0, 0, 0, // type_code = 6 (U32)
            0x78, 0x56, 0x34, 0x12, // name_hash = 0x12345678
            0, 0, 0, 0, // unk
            5, 0, 10, 0, // raw_offset = (10 << 16) | 5
        ];
        let schema = ComponentSchema::from_schm_body(&body, false).unwrap();
        assert_eq!(schema.payload_stride, 100);
        assert_eq!(schema.fields.len(), 1);
        let field = &schema.fields[0];
        assert_eq!(field.field_type, SchemaFieldType::U32);
        assert_eq!(field.name_hash, 0x12345678);
        assert_eq!(field.byte_offset, 10);
        assert_eq!(field.flags, 5);
    }
}
