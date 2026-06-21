//! Bounds-checked byte buffer — models engine pointer dereferences.

use std::fmt;

#[derive(Debug, Clone)]
pub struct AccessViolation {
    pub context: String,
    pub offset: usize,
    pub size: usize,
    pub buffer_len: usize,
}

impl fmt::Display for AccessViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: read {} bytes at offset 0x{:X} (buffer len 0x{:X})",
            self.context, self.size, self.offset, self.buffer_len
        )
    }
}

pub type AccessResult<T> = Result<T, AccessViolation>;

#[derive(Clone)]
pub struct SafeSlice {
    data: Vec<u8>,
    label: String,
}

impl SafeSlice {
    pub fn new(data: Vec<u8>, label: impl Into<String>) -> Self {
        Self {
            data,
            label: label.into(),
        }
    }

    pub fn from_slice(data: &[u8], label: impl Into<String>) -> Self {
        Self::new(data.to_vec(), label)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    fn ctx(&self, field: &str) -> String {
        format!("{}:{}", self.label, field)
    }

    pub fn read_u8(&self, offset: usize, field: &str) -> AccessResult<u8> {
        if offset >= self.data.len() {
            return Err(AccessViolation {
                context: self.ctx(field),
                offset,
                size: 1,
                buffer_len: self.data.len(),
            });
        }
        Ok(self.data[offset])
    }

    pub fn read_u16_le(&self, offset: usize, field: &str) -> AccessResult<u16> {
        if offset + 2 > self.data.len() {
            return Err(AccessViolation {
                context: self.ctx(field),
                offset,
                size: 2,
                buffer_len: self.data.len(),
            });
        }
        Ok(u16::from_le_bytes([self.data[offset], self.data[offset + 1]]))
    }

    pub fn read_u32_le(&self, offset: usize, field: &str) -> AccessResult<u32> {
        if offset + 4 > self.data.len() {
            return Err(AccessViolation {
                context: self.ctx(field),
                offset,
                size: 4,
                buffer_len: self.data.len(),
            });
        }
        Ok(u32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ]))
    }

    pub fn read_f32_le(&self, offset: usize, field: &str) -> AccessResult<f32> {
        Ok(f32::from_bits(self.read_u32_le(offset, field)?))
    }

    /// Engine pointer follow: `base + offset` for `size` bytes.
    pub fn slice(&self, offset: usize, size: usize, field: &str) -> AccessResult<SafeSlice> {
        if size == 0 {
            return Ok(SafeSlice::new(Vec::new(), format!("{}[0..0]", self.label)));
        }
        if offset > self.data.len() || offset.saturating_add(size) > self.data.len() {
            return Err(AccessViolation {
                context: self.ctx(field),
                offset,
                size,
                buffer_len: self.data.len(),
            });
        }
        Ok(SafeSlice::new(
            self.data[offset..offset + size].to_vec(),
            format!("{}+0x{:X}[{}]", self.label, offset, size),
        ))
    }

    pub fn subslice(&self, offset: usize, end: usize, field: &str) -> AccessResult<SafeSlice> {
        if end < offset {
            return Err(AccessViolation {
                context: format!("{}: end < start", self.ctx(field)),
                offset,
                size: end.saturating_sub(offset),
                buffer_len: self.data.len(),
            });
        }
        self.slice(offset, end - offset, field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_properties() {
        let data = vec![1, 2, 3, 4, 5];
        let ss = SafeSlice::new(data, "test");
        assert_eq!(ss.len(), 5);
        assert!(!ss.is_empty());
        assert_eq!(ss.label(), "test");
    }

    #[test]
    fn from_slice() {
        let data = [10u8, 20, 30];
        let ss = SafeSlice::from_slice(&data, "slice_test");
        assert_eq!(ss.len(), 3);
        assert_eq!(ss.as_bytes(), &data);
    }

    #[test]
    fn read_u8_valid() {
        let ss = SafeSlice::from_slice(&[42u8], "u8_test");
        assert_eq!(ss.read_u8(0, "field").unwrap(), 42);
    }

    #[test]
    fn read_u8_out_of_bounds() {
        let ss = SafeSlice::from_slice(&[42u8], "u8_test");
        assert!(ss.read_u8(1, "field").is_err());
        let err = ss.read_u8(1, "field").unwrap_err();
        assert_eq!(err.offset, 1);
        assert_eq!(err.size, 1);
    }

    #[test]
    fn read_u16_le_valid() {
        let ss = SafeSlice::from_slice(&[0x34, 0x12], "u16_test");
        assert_eq!(ss.read_u16_le(0, "field").unwrap(), 0x1234);
    }

    #[test]
    fn read_u16_le_partial() {
        let ss = SafeSlice::from_slice(&[0x34], "u16_test");
        assert!(ss.read_u16_le(0, "field").is_err());
    }

    #[test]
    fn read_u16_le_out_of_bounds() {
        let ss = SafeSlice::from_slice(&[0x12, 0x34], "u16_test");
        assert!(ss.read_u16_le(1, "field").is_err());
    }

    #[test]
    fn read_u32_le_valid() {
        let ss = SafeSlice::from_slice(&[0x78, 0x56, 0x34, 0x12], "u32_test");
        assert_eq!(ss.read_u32_le(0, "field").unwrap(), 0x12345678);
    }

    #[test]
    fn read_u32_le_out_of_bounds() {
        let ss = SafeSlice::from_slice(&[0x12, 0x34, 0x56], "u32_test");
        assert!(ss.read_u32_le(0, "field").is_err());
    }

    #[test]
    fn read_f32_le_valid() {
        let bits = 0x41200000u32; // 10.0 in IEEE 754
        let ss = SafeSlice::from_slice(&bits.to_le_bytes(), "f32_test");
        let f = ss.read_f32_le(0, "field").unwrap();
        assert!((f - 10.0).abs() < 0.001);
    }

    #[test]
    fn read_f32_le_out_of_bounds() {
        let ss = SafeSlice::from_slice(&[0, 0], "f32_test");
        assert!(ss.read_f32_le(0, "field").is_err());
    }

    #[test]
    fn slice_valid() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let ss = SafeSlice::new(data, "slice_test");
        let sub = ss.slice(2, 3, "subfield").unwrap();
        assert_eq!(sub.len(), 3);
        assert_eq!(sub.as_bytes(), &[3, 4, 5]);
    }

    #[test]
    fn slice_zero_size() {
        let data = vec![1, 2, 3];
        let ss = SafeSlice::new(data, "slice_test");
        let sub = ss.slice(1, 0, "empty").unwrap();
        assert_eq!(sub.len(), 0);
        assert!(sub.is_empty());
    }

    #[test]
    fn slice_out_of_bounds() {
        let data = vec![1, 2, 3];
        let ss = SafeSlice::new(data, "slice_test");
        assert!(ss.slice(2, 2, "field").is_err());
        assert!(ss.slice(5, 1, "field").is_err());
    }

    #[test]
    fn slice_overflow() {
        let data = vec![1, 2, 3];
        let ss = SafeSlice::new(data, "slice_test");
        assert!(ss.slice(1, usize::MAX, "field").is_err());
    }

    #[test]
    fn subslice_valid() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let ss = SafeSlice::new(data, "subslice_test");
        let sub = ss.subslice(2, 5, "range").unwrap();
        assert_eq!(sub.as_bytes(), &[3, 4, 5]);
    }

    #[test]
    fn subslice_invalid_range() {
        let data = vec![1, 2, 3];
        let ss = SafeSlice::new(data, "subslice_test");
        assert!(ss.subslice(5, 2, "field").is_err());
    }

    #[test]
    fn subslice_equal_range() {
        let data = vec![1, 2, 3, 4];
        let ss = SafeSlice::new(data, "subslice_test");
        let sub = ss.subslice(2, 2, "equal").unwrap();
        assert_eq!(sub.len(), 0);
    }

    #[test]
    fn empty_buffer() {
        let ss = SafeSlice::new(vec![], "empty");
        assert!(ss.is_empty());
        assert_eq!(ss.len(), 0);
        assert!(ss.read_u8(0, "field").is_err());
    }

    #[test]
    fn access_violation_display() {
        let av = AccessViolation {
            context: "test:field".to_string(),
            offset: 0x100,
            size: 4,
            buffer_len: 0x50,
        };
        let display = format!("{}", av);
        assert!(display.contains("test:field"));
        assert!(display.contains("0x100"));
        assert!(display.contains("0x50"));
    }
}
