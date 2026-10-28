//! The profiler's stack samples (`<session>.samples`): what the profiler ASI's sampler records of the game's
//! threads while a session runs, streamed to the capture service (`SAMPLES <session>`, [`crate::profd`])
//! and read offline by `mercs2_probe prof-samples`.
//!
//! Little-endian throughout:
//!
//! ```text
//! header   magic "M2SAMPLE"  u32 version (2)  [u8; 32] sha256 of the game exe file  u32 image base
//!          u64 QueryPerformanceFrequency  u32 sample period (µs)  u32 main thread id
//!          u32 thread count  (u32 thread id, u32 start address) × thread count: the threads sampled
//!          u32 module count  (u32 base, u32 size, u16 name length, name bytes (UTF-8)) × module count:
//!                            the modules loaded when the session started
//!          u32 range count  (u32 start, u32 end) × range count: the image's code sections
//! record × n   u64 QueryPerformanceCounter  u32 tick  u32 frame  u32 thread id  u32 eip  u16 word count
//!              u16 flags  u32 × word count: the stack words, from ESP upward, that lie in a code range
//! ```
//!
//! Every period is one tick: the sampler samples each sampled thread in turn, so the records of one tick
//! show what every thread was doing at (nearly) the same moment. A record's words are candidates, not a
//! call stack: any value on the stack that points into code (a return address, but also a function pointer
//! held in a local). The reader keeps them all; deciding which are return addresses needs the image's bytes
//! and is the offline tool's job. [`FLAG_SCAN_LIMIT`] marks a record whose scan stopped at the scan limit
//! before reaching the stack base; [`FLAG_EXITED`] marks the thread's last record: it had exited by that
//! tick, and the record holds no EIP and no words.

pub const MAGIC: &[u8; 8] = b"M2SAMPLE";
pub const VERSION: u32 = 2;
/// The header up to and including the main thread id.
pub const HEADER_FIXED_LEN: usize = 8 + 4 + 32 + 4 + 8 + 4 + 4;
pub const RECORD_HEAD_LEN: usize = 8 + 4 + 4 + 4 + 4 + 2 + 2;
/// The scan stopped at its byte limit, not at the stack base.
pub const FLAG_SCAN_LIMIT: u16 = 1;
/// The thread had exited by this tick; it has no later record.
pub const FLAG_EXITED: u16 = 2;
/// More code ranges than any PE has sections.
pub const MAX_RANGES: u32 = 96;
/// More threads or modules than a game process holds.
pub const MAX_THREADS: u32 = 4096;
pub const MAX_MODULES: u32 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub tid: u32,
    /// The address the thread started at (`ThreadQuerySetWin32StartAddress`).
    pub start: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub base: u32,
    pub size: u32,
    /// The file name, without its directory.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub exe_sha256: [u8; 32],
    pub image_base: u32,
    pub qpc_frequency: u64,
    pub period_us: u32,
    pub main_thread: u32,
    /// The threads sampled; the main thread is one of them.
    pub threads: Vec<Thread>,
    pub modules: Vec<Module>,
    /// `[start, end)` of each code section.
    pub code: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub qpc: u64,
    pub tick: u32,
    pub frame: u32,
    pub tid: u32,
    pub eip: u32,
    pub flags: u16,
    pub words: Vec<u32>,
}

impl Header {
    /// The encoded header. A module name longer than `u16::MAX` bytes is an error, not a shorter name.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut b = Vec::with_capacity(HEADER_FIXED_LEN + 12 + 8 * self.threads.len() + 8 * self.code.len());
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&VERSION.to_le_bytes());
        b.extend_from_slice(&self.exe_sha256);
        b.extend_from_slice(&self.image_base.to_le_bytes());
        b.extend_from_slice(&self.qpc_frequency.to_le_bytes());
        b.extend_from_slice(&self.period_us.to_le_bytes());
        b.extend_from_slice(&self.main_thread.to_le_bytes());
        b.extend_from_slice(&(self.threads.len() as u32).to_le_bytes());
        for t in &self.threads {
            b.extend_from_slice(&t.tid.to_le_bytes());
            b.extend_from_slice(&t.start.to_le_bytes());
        }
        b.extend_from_slice(&(self.modules.len() as u32).to_le_bytes());
        for m in &self.modules {
            let n = u16::try_from(m.name.len()).map_err(|_| format!("module {:?} has a name longer than {} bytes", m.name, u16::MAX))?;
            b.extend_from_slice(&m.base.to_le_bytes());
            b.extend_from_slice(&m.size.to_le_bytes());
            b.extend_from_slice(&n.to_le_bytes());
            b.extend_from_slice(m.name.as_bytes());
        }
        b.extend_from_slice(&(self.code.len() as u32).to_le_bytes());
        for (s, e) in &self.code {
            b.extend_from_slice(&s.to_le_bytes());
            b.extend_from_slice(&e.to_le_bytes());
        }
        Ok(b)
    }

    /// Whether `va` lies in one of the code ranges.
    pub fn in_code(&self, va: u32) -> bool {
        self.code.iter().any(|&(s, e)| va >= s && va < e)
    }

    /// The module holding `va`.
    pub fn module_of(&self, va: u32) -> Option<&Module> {
        self.modules.iter().find(|m| va >= m.base && va - m.base < m.size)
    }
}

impl Record {
    /// Append the record to `out`. At most `u16::MAX` words are written; the rest are dropped and the
    /// record is flagged [`FLAG_SCAN_LIMIT`].
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        let n = self.words.len().min(usize::from(u16::MAX));
        let flags = if n < self.words.len() { self.flags | FLAG_SCAN_LIMIT } else { self.flags };
        out.extend_from_slice(&self.qpc.to_le_bytes());
        out.extend_from_slice(&self.tick.to_le_bytes());
        out.extend_from_slice(&self.frame.to_le_bytes());
        out.extend_from_slice(&self.tid.to_le_bytes());
        out.extend_from_slice(&self.eip.to_le_bytes());
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        for w in &self.words[..n] {
            out.extend_from_slice(&w.to_le_bytes());
        }
    }

    pub fn exited(&self) -> bool {
        self.flags & FLAG_EXITED != 0
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}
fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

/// Reads the header's counted lists, failing with what ran out.
struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize, what: &str) -> Result<usize, String> {
        if self.b.len() - self.at < n {
            return Err(format!("{} bytes is shorter than the header's {what}", self.b.len()));
        }
        self.at += n;
        Ok(self.at - n)
    }

    fn count(&mut self, what: &str, max: u32) -> Result<usize, String> {
        let at = self.take(4, &format!("{what} count"))?;
        let n = u32_at(self.b, at);
        if n > max {
            return Err(format!("{n} {what}; a header holds at most {max}"));
        }
        Ok(n as usize)
    }
}

/// A whole samples file. A file that ends inside a record, or a record of a thread the header does not
/// list, is an error naming where, not a shorter list.
pub fn decode(bytes: &[u8]) -> Result<(Header, Vec<Record>), String> {
    if bytes.len() < HEADER_FIXED_LEN {
        return Err(format!("{} bytes is shorter than the {HEADER_FIXED_LEN}-byte header", bytes.len()));
    }
    if &bytes[..8] != MAGIC {
        return Err(format!("the magic is {:?}, not \"M2SAMPLE\"", String::from_utf8_lossy(&bytes[..8])));
    }
    let version = u32_at(bytes, 8);
    if version != VERSION {
        return Err(format!("version {version}; this reader reads version {VERSION}"));
    }
    let mut c = Cursor { b: bytes, at: HEADER_FIXED_LEN };
    let nthreads = c.count("threads", MAX_THREADS)?;
    let mut threads = Vec::with_capacity(nthreads);
    for _ in 0..nthreads {
        let at = c.take(8, "threads")?;
        threads.push(Thread { tid: u32_at(bytes, at), start: u32_at(bytes, at + 4) });
    }
    let nmodules = c.count("modules", MAX_MODULES)?;
    let mut modules = Vec::with_capacity(nmodules);
    for i in 0..nmodules {
        let at = c.take(10, "modules")?;
        let len = usize::from(u16_at(bytes, at + 8));
        let name_at = c.take(len, "module names")?;
        let name = std::str::from_utf8(&bytes[name_at..name_at + len]).map_err(|e| format!("module {i}'s name: {e}"))?;
        modules.push(Module { base: u32_at(bytes, at), size: u32_at(bytes, at + 4), name: name.to_owned() });
    }
    let nranges = c.count("code ranges", MAX_RANGES)?;
    let mut code = Vec::with_capacity(nranges);
    for _ in 0..nranges {
        let at = c.take(8, "code ranges")?;
        code.push((u32_at(bytes, at), u32_at(bytes, at + 4)));
    }
    let header = Header {
        exe_sha256: bytes[12..44].try_into().expect("32 bytes"),
        image_base: u32_at(bytes, 44),
        qpc_frequency: u64_at(bytes, 48),
        period_us: u32_at(bytes, 56),
        main_thread: u32_at(bytes, 60),
        threads,
        modules,
        code,
    };
    if !header.threads.iter().any(|t| t.tid == header.main_thread) {
        return Err(format!("the main thread {} is not among the {} threads sampled", header.main_thread, header.threads.len()));
    }
    let mut records = Vec::new();
    let mut at = c.at;
    while at < bytes.len() {
        if bytes.len() - at < RECORD_HEAD_LEN {
            return Err(format!("record {} at byte {at}: the file ends inside its {RECORD_HEAD_LEN}-byte head", records.len()));
        }
        let n = usize::from(u16_at(bytes, at + 24));
        let end = at + RECORD_HEAD_LEN + 4 * n;
        if end > bytes.len() {
            return Err(format!("record {} at byte {at}: {n} words run past the end of the file", records.len()));
        }
        let words = (0..n).map(|i| u32_at(bytes, at + RECORD_HEAD_LEN + 4 * i)).collect();
        let rec = Record {
            qpc: u64_at(bytes, at),
            tick: u32_at(bytes, at + 8),
            frame: u32_at(bytes, at + 12),
            tid: u32_at(bytes, at + 16),
            eip: u32_at(bytes, at + 20),
            flags: u16_at(bytes, at + 26),
            words,
        };
        if !header.threads.iter().any(|t| t.tid == rec.tid) {
            return Err(format!("record {} at byte {at}: thread {} is not among the threads the header lists", records.len(), rec.tid));
        }
        records.push(rec);
        at = end;
    }
    Ok((header, records))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            exe_sha256: [7; 32],
            image_base: 0x40_0000,
            qpc_frequency: 10_000_000,
            period_us: 4000,
            main_thread: 0x5c,
            threads: vec![Thread { tid: 0x5c, start: 0x63_1590 }, Thread { tid: 0x12c, start: 0x87_6400 }],
            modules: vec![
                Module { base: 0x40_0000, size: 0x1d0_0000, name: "Mercenaries2.exe".into() },
                Module { base: 0x7bc0_0000, size: 0x10_0000, name: "ntdll.dll".into() },
            ],
            code: vec![(0x40_1000, 0xb0_5000), (0x1a4_8000, 0x20f_e000)],
        }
    }

    #[test]
    fn a_file_round_trips() {
        let h = header();
        let recs = vec![
            Record { qpc: 1, tick: 0, frame: 10, tid: 0x5c, eip: 0x63_0e10, flags: 0, words: vec![0x4c_9740, 0x63_1938] },
            Record { qpc: 2, tick: 0, frame: 10, tid: 0x12c, eip: 0x7bc0_1234, flags: FLAG_SCAN_LIMIT, words: vec![] },
            Record { qpc: 3, tick: 1, frame: 10, tid: 0x12c, eip: 0, flags: FLAG_EXITED, words: vec![] },
        ];
        let mut b = h.encode().unwrap();
        assert_eq!(b.len(), HEADER_FIXED_LEN + 4 + 16 + 4 + (10 + 16) + (10 + 9) + 4 + 16);
        for r in &recs {
            r.encode_into(&mut b);
        }
        let (h2, r2) = decode(&b).unwrap();
        assert_eq!((h2, r2.clone()), (h, recs));
        assert!(r2[2].exited() && !r2[1].exited());
    }

    #[test]
    fn a_truncated_or_foreign_file_says_where() {
        let h = header();
        let mut b = h.encode().unwrap();
        let hl = b.len();
        Record { qpc: 1, tick: 0, frame: 1, tid: 0x5c, eip: 1, flags: 0, words: vec![1, 2, 3] }.encode_into(&mut b);
        assert!(decode(&b[..b.len() - 2]).unwrap_err().contains("run past the end"));
        assert!(decode(&b[..hl + 5]).unwrap_err().contains("ends inside its"));
        assert!(decode(&b[..hl - 4]).unwrap_err().contains("code ranges"));
        assert!(decode(&b[..HEADER_FIXED_LEN + 10]).unwrap_err().contains("threads"));
        assert!(decode(&b[..HEADER_FIXED_LEN + 4 + 16 + 4 + 12]).unwrap_err().contains("module names"));
        assert!(decode(b"M2SAMPLX").unwrap_err().contains("shorter than"));
        let mut wrong = b.clone();
        wrong[0] = b'X';
        assert!(decode(&wrong).unwrap_err().contains("magic"));
    }

    #[test]
    fn every_record_and_the_main_thread_are_listed_threads() {
        let h = header();
        let mut b = h.encode().unwrap();
        Record { qpc: 1, tick: 0, frame: 1, tid: 0x999, eip: 1, flags: 0, words: vec![] }.encode_into(&mut b);
        assert!(decode(&b).unwrap_err().contains("thread 2457 is not among"));
        let mut unlisted = header();
        unlisted.main_thread = 0x77;
        assert!(decode(&unlisted.encode().unwrap()).unwrap_err().contains("main thread 119"));
    }

    #[test]
    fn code_ranges_and_modules_bound_addresses() {
        let h = header();
        assert!(h.in_code(0x40_1000) && h.in_code(0xb0_4fff) && h.in_code(0x1a4_8000));
        assert!(!h.in_code(0xb0_5000) && !h.in_code(0x40_0fff) && !h.in_code(0x20f_e000));
        assert_eq!(h.module_of(0x7bc0_1234).map(|m| m.name.as_str()), Some("ntdll.dll"));
        assert_eq!(h.module_of(0x7bd0_0000), None);
    }
}
