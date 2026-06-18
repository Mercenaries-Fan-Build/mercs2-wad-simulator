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
