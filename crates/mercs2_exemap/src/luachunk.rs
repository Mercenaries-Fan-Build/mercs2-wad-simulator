//! Lua 5.1 precompiled chunks as the game stores them, in either byte order, and the INFO record
//! that names a script container.
//!
//! A chunk opens with a 12-byte header:
//!
//! ```text
//! 1b 4c 75 61   "\x1bLua"
//! 51            version 5.1
//! 00            format 0
//! 01 | 00       little-endian (PC) | big-endian (Xbox 360, PS3)
//! 04 04 04 04   sizeof int, size_t, Instruction, lua_Number (a 4-byte float)
//! 00            lua_Number is not integral
//! ```
//!
//! then the main function, each function laid out as Lua 5.1's `ldump.c` writes it (every integer
//! in the header's byte order):
//!
//! ```text
//! string source            size_t length counting the NUL, then the bytes; length 0 = none
//! int    line_defined, last_line_defined
//! u8     nups, num_params, is_vararg, max_stack
//! int n, n × u32           code
//! int n, n × constant      u8 type: 0 nil | 1 boolean (u8) | 3 number (f32) | 4 string
//! int n, n × function      nested functions, recursively
//! int n, n × int           line info
//! int n, n × { string name, int start_pc, int end_pc }   local variables
//! int n, n × string        upvalue names
//! ```
//!
//! [`decode`] consumes a chunk exactly — trailing bytes, an unknown constant type or an opcode
//! outside Lua 5.1's 38 are errors — and [`encode`] writes a [`Chunk`] back byte for byte.
//!
//! The INFO record ([`Info`]) is `u8 5`, a u16 name length, the name, a NUL, a u16 and a u8, the
//! u16s in the WAD's byte order.

use mercs2_formats::ffcs::Endian;

use crate::Error;

/// The 12-byte header with the endianness byte at [`ENDIAN_AT`].
const HEADER: [u8; 12] = [0x1b, 0x4c, 0x75, 0x61, 0x51, 0x00, 0x00, 0x04, 0x04, 0x04, 0x04, 0x00];
const ENDIAN_AT: usize = 6;
/// Lua 5.1 defines opcodes 0..=37.
const NUM_OPCODES: u32 = 38;

/// A constant. A number keeps its float bits, so equality is exact.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Constant {
    Nil,
    Boolean(bool),
    Number(u32),
    String(Vec<u8>),
}

/// A local variable's debug record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocVar {
    pub name: Vec<u8>,
    pub start_pc: u32,
    pub end_pc: u32,
}

/// One function prototype.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proto {
    /// `None` when the chunk carries no source name (every console chunk).
    pub source: Option<Vec<u8>>,
    pub line_defined: u32,
    pub last_line_defined: u32,
    pub nups: u8,
    pub num_params: u8,
    pub is_vararg: u8,
    pub max_stack: u8,
    pub code: Vec<u32>,
    pub constants: Vec<Constant>,
    pub protos: Vec<Proto>,
    pub line_info: Vec<u32>,
    pub loc_vars: Vec<LocVar>,
    pub upvalues: Vec<Vec<u8>>,
}

/// A decoded chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub endian: Endian,
    pub main: Proto,
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    e: Endian,
}

impl<'a> Reader<'a> {
    fn err(&self, what: &str) -> Error {
        Error::new(format!("Lua chunk: {what} at 0x{:x} of {} bytes", self.pos, self.b.len()))
    }

    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8], Error> {
        if self.pos + n > self.b.len() {
            return Err(self.err(&format!("{what} ({n} bytes) runs past the end")));
        }
        let s = &self.b[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self, what: &str) -> Result<u8, Error> {
        Ok(self.take(1, what)?[0])
    }

    fn u32(&mut self, what: &str) -> Result<u32, Error> {
        let s = self.take(4, what)?;
        Ok(self.e.u32(s, 0))
    }

    /// A count, bounded by the bytes left (each element is at least `min_size` bytes).
    fn count(&mut self, min_size: usize, what: &str) -> Result<usize, Error> {
        let n = self.u32(what)? as usize;
        if n.saturating_mul(min_size) > self.b.len() - self.pos {
            return Err(self.err(&format!("{what} count {n} exceeds the bytes left")));
        }
        Ok(n)
    }

    fn string(&mut self, what: &str) -> Result<Option<Vec<u8>>, Error> {
        let n = self.u32(what)? as usize;
        if n == 0 {
            return Ok(None);
        }
        let s = self.take(n, what)?;
        if s[n - 1] != 0 {
            return Err(self.err(&format!("{what} does not end in NUL")));
        }
        Ok(Some(s[..n - 1].to_vec()))
    }

    fn named(&mut self, what: &str) -> Result<Vec<u8>, Error> {
        self.string(what)?.ok_or_else(|| self.err(&format!("{what} is absent")))
    }

    fn proto(&mut self) -> Result<Proto, Error> {
        let source = self.string("source")?;
        let line_defined = self.u32("line_defined")?;
        let last_line_defined = self.u32("last_line_defined")?;
        let nups = self.u8("nups")?;
        let num_params = self.u8("num_params")?;
        let is_vararg = self.u8("is_vararg")?;
        let max_stack = self.u8("max_stack")?;
        let n = self.count(4, "code")?;
        let mut code = Vec::with_capacity(n);
        for _ in 0..n {
            let i = self.u32("instruction")?;
            if i & 0x3f >= NUM_OPCODES {
                return Err(self.err(&format!("opcode {} is not a Lua 5.1 opcode", i & 0x3f)));
            }
            code.push(i);
        }
        let n = self.count(1, "constants")?;
        let mut constants = Vec::with_capacity(n);
        for _ in 0..n {
            constants.push(match self.u8("constant type")? {
                0 => Constant::Nil,
                1 => Constant::Boolean(match self.u8("boolean")? {
                    0 => false,
                    1 => true,
                    v => return Err(self.err(&format!("boolean constant {v}"))),
                }),
                3 => Constant::Number(self.u32("number")?),
                4 => Constant::String(self.named("string constant")?),
                t => return Err(self.err(&format!("constant type {t}"))),
            });
        }
        let n = self.count(4, "functions")?;
        let mut protos = Vec::with_capacity(n);
        for _ in 0..n {
            protos.push(self.proto()?);
        }
        let n = self.count(4, "line info")?;
        let mut line_info = Vec::with_capacity(n);
        for _ in 0..n {
            line_info.push(self.u32("line")?);
        }
        let n = self.count(12, "local variables")?;
        let mut loc_vars = Vec::with_capacity(n);
        for _ in 0..n {
            loc_vars.push(LocVar {
                name: self.named("local name")?,
                start_pc: self.u32("start_pc")?,
                end_pc: self.u32("end_pc")?,
            });
        }
        let n = self.count(4, "upvalue names")?;
        let mut upvalues = Vec::with_capacity(n);
        for _ in 0..n {
            upvalues.push(self.named("upvalue name")?);
        }
        Ok(Proto {
            source,
            line_defined,
            last_line_defined,
            nups,
            num_params,
            is_vararg,
            max_stack,
            code,
            constants,
            protos,
            line_info,
            loc_vars,
            upvalues,
        })
    }
}

/// Decode a whole chunk; every byte must belong to it.
pub fn decode(b: &[u8]) -> Result<Chunk, Error> {
    if b.len() < HEADER.len() {
        return Err(Error::new(format!("Lua chunk: {} bytes, shorter than its header", b.len())));
    }
    let e = match b[ENDIAN_AT] {
        1 => Endian::Little,
        0 => Endian::Big,
        v => return Err(Error::new(format!("Lua chunk: endianness byte {v}"))),
    };
    let mut want = HEADER;
    want[ENDIAN_AT] = b[ENDIAN_AT];
    if b[..HEADER.len()] != want {
        return Err(Error::new(format!(
            "Lua chunk: header {:02x?}, expected {want:02x?}",
            &b[..HEADER.len()]
        )));
    }
    let mut r = Reader { b, pos: HEADER.len(), e };
    let main = r.proto()?;
    if r.pos != b.len() {
        return Err(r.err(&format!("{} trailing bytes after the main function", b.len() - r.pos)));
    }
    Ok(Chunk { endian: e, main })
}

struct Writer {
    out: Vec<u8>,
    e: Endian,
}

impl Writer {
    fn u32(&mut self, v: u32) {
        match self.e {
            Endian::Little => self.out.extend_from_slice(&v.to_le_bytes()),
            Endian::Big => self.out.extend_from_slice(&v.to_be_bytes()),
        }
    }

    fn string(&mut self, s: Option<&[u8]>) {
        match s {
            None => self.u32(0),
            Some(s) => {
                self.u32(s.len() as u32 + 1);
                self.out.extend_from_slice(s);
                self.out.push(0);
            }
        }
    }

    fn proto(&mut self, p: &Proto) {
        self.string(p.source.as_deref());
        self.u32(p.line_defined);
        self.u32(p.last_line_defined);
        self.out.extend_from_slice(&[p.nups, p.num_params, p.is_vararg, p.max_stack]);
        self.u32(p.code.len() as u32);
        for &i in &p.code {
            self.u32(i);
        }
        self.u32(p.constants.len() as u32);
        for c in &p.constants {
            match c {
                Constant::Nil => self.out.push(0),
                Constant::Boolean(v) => self.out.extend_from_slice(&[1, *v as u8]),
                Constant::Number(bits) => {
                    self.out.push(3);
                    self.u32(*bits);
                }
                Constant::String(s) => {
                    self.out.push(4);
                    self.string(Some(s));
                }
            }
        }
        self.u32(p.protos.len() as u32);
        for q in &p.protos {
            self.proto(q);
        }
        self.u32(p.line_info.len() as u32);
        for &l in &p.line_info {
            self.u32(l);
        }
        self.u32(p.loc_vars.len() as u32);
        for v in &p.loc_vars {
            self.string(Some(&v.name));
            self.u32(v.start_pc);
            self.u32(v.end_pc);
        }
        self.u32(p.upvalues.len() as u32);
        for u in &p.upvalues {
            self.string(Some(u));
        }
    }
}

/// Write `c` as a chunk in its byte order.
pub fn encode(c: &Chunk) -> Vec<u8> {
    let mut w = Writer { out: HEADER.to_vec(), e: c.endian };
    w.out[ENDIAN_AT] = if c.endian == Endian::Little { 1 } else { 0 };
    w.proto(&c.main);
    w.out
}

/// A script container's INFO record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub name: String,
    /// The u16 and u8 after the name's NUL, kept as stored.
    pub tail_u16: u16,
    pub tail_u8: u8,
}

/// Decode an INFO record in byte order `e`.
pub fn decode_info(b: &[u8], e: Endian) -> Result<Info, Error> {
    if b.len() < 3 || b[0] != 5 {
        return Err(Error::new(format!("INFO: {:02x?} does not open with version 5", &b[..b.len().min(3)])));
    }
    let len = match e {
        Endian::Little => u16::from_le_bytes([b[1], b[2]]),
        Endian::Big => u16::from_be_bytes([b[1], b[2]]),
    } as usize;
    if b.len() != 3 + len + 4 {
        return Err(Error::new(format!("INFO: {} bytes for a {len}-byte name, expected {}", b.len(), 3 + len + 4)));
    }
    let name = &b[3..3 + len];
    if name.contains(&0) || b[3 + len] != 0 {
        return Err(Error::new(format!("INFO: name {name:02x?} is not a NUL-terminated string")));
    }
    let name = String::from_utf8(name.to_vec()).map_err(|_| Error::new(format!("INFO: name {name:02x?} is not UTF-8")))?;
    let t = 3 + len + 1;
    let tail_u16 = match e {
        Endian::Little => u16::from_le_bytes([b[t], b[t + 1]]),
        Endian::Big => u16::from_be_bytes([b[t], b[t + 1]]),
    };
    Ok(Info { name, tail_u16, tail_u8: b[t + 2] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(e: Endian, source: Option<&[u8]>) -> Chunk {
        let inner = Proto {
            source: None,
            line_defined: 3,
            last_line_defined: 5,
            nups: 1,
            num_params: 2,
            is_vararg: 0,
            max_stack: 4,
            code: vec![0x0000_001e],
            constants: vec![],
            protos: vec![],
            line_info: vec![5],
            loc_vars: vec![LocVar { name: b"a".to_vec(), start_pc: 0, end_pc: 1 }],
            upvalues: vec![b"up".to_vec()],
        };
        Chunk {
            endian: e,
            main: Proto {
                source: source.map(<[u8]>::to_vec),
                line_defined: 0,
                last_line_defined: 0,
                nups: 0,
                num_params: 0,
                is_vararg: 2,
                max_stack: 2,
                code: vec![0x0000_0024, 0x0080_001e],
                constants: vec![
                    Constant::Nil,
                    Constant::Boolean(true),
                    Constant::Number(1.5f32.to_bits()),
                    Constant::String(b"CallWithOptionalArgs".to_vec()),
                ],
                protos: vec![inner],
                line_info: vec![1, 2],
                loc_vars: vec![],
                upvalues: vec![],
            },
        }
    }

    #[test]
    fn chunks_round_trip_in_both_byte_orders() {
        for (e, src) in [(Endian::Little, Some(&b"mrxutil_shell"[..])), (Endian::Big, None)] {
            let c = sample(e, src);
            let bytes = encode(&c);
            assert_eq!(bytes[ENDIAN_AT], if e == Endian::Little { 1 } else { 0 });
            assert_eq!(decode(&bytes).unwrap(), c, "{e:?}");
        }
    }

    #[test]
    fn malformed_chunks_are_errors() {
        let good = encode(&sample(Endian::Little, None));
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(decode(&trailing).unwrap_err().to_string().contains("trailing bytes"));
        assert!(decode(&good[..good.len() - 1]).unwrap_err().to_string().contains("runs past the end"));
        let mut header = good.clone();
        header[10] = 8;
        assert!(decode(&header).unwrap_err().to_string().contains("header"));
        let mut op = good.clone();
        // The main function's first instruction follows header 12 + source 4 + lines 8 + 4 bytes
        // + code count 4.
        op[32] = 0x3f;
        assert!(decode(&op).unwrap_err().to_string().contains("not a Lua 5.1 opcode"));
    }

    #[test]
    fn info_records_decode_in_both_byte_orders() {
        let pc = b"\x05\x0d\x00mrxutil_shell\x00\x02\x00\x00";
        let x360 = b"\x05\x00\x0dmrxutil_shell\x00\x00\x02\x00";
        let want = Info { name: "mrxutil_shell".into(), tail_u16: 2, tail_u8: 0 };
        assert_eq!(decode_info(pc, Endian::Little).unwrap(), want);
        assert_eq!(decode_info(x360, Endian::Big).unwrap(), want);
        assert!(decode_info(pc, Endian::Big).unwrap_err().to_string().contains("expected"));
    }
}
