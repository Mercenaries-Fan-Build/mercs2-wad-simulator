//! Reads of the running game for m2-sdk.dll's `M2Live` Lua namespace, over [`Mem`] so they can be tested
//! without a game. Lua has no read of the camera's position or of what is resident; both live here.
//!
//! **The camera.** `Player.GetCamera` pushes `P`, a pointer into the per-player owner array. `*(P + 0x1E0)`
//! is the pose record (`FUN_0070F560`), which holds the eye at `+0x44` (`FUN_0042EE10`).
//!
//! **The asset registry.** The manager (`[slot]`) holds 256 `(typeKey, resolver)` pairs: resolvers at `+8`,
//! keys at `+0x408`. A resolver is `{vtable, count@+4, default@+8, records[N]@+0xC, nameKeys[N]@+0xC+4N}`, and
//! its `N` is only in its lookup code, the function at `vtable+8`. A record is `{vtable, name@+4, type@+8,
//! refs[4]@+0xC, status@+0x14, flags@+0x16}`; status 2 is resident. Blocks count at `manager+0x844`.
//!
//! The engine's own lookup (`0x874370`) writes a flag on the record, so this never calls it; it reads the
//! tables. Every pointer is checked with `readable_span` before use, and anything unexpected is an error
//! that says so, never a zero. Walks are bounded by the tables' own sizes.

use crate::lua_stack::Mem;

/// The instruction that loads the manager pointer: `mov ecx, [manager]` at [`MANAGER_OPERAND`]. One match
/// in both PC exes.
pub const MANAGER_PATTERN: &str = "a8 40 53 55 57 75 ?? 8b 0d ?? ?? ?? ?? 8b a9 60 c3 04 00";
pub const MANAGER_OPERAND: usize = 9;

/// `P + 0x1E0` holds the pose record.
const OWNER_RECORD: u32 = 0x1E0;
const RECORD_EYE: u32 = 0x44;

const SLOTS: u32 = 256;
const RESOLVERS: u32 = 0x8;
const TYPE_KEYS: u32 = 0x408;
const BLOCKS: u32 = 0x844;
const RESOLVER_COUNT: u32 = 0x4;
const RESOLVER_RECORDS: u32 = 0xC;
const RECORD_NAME: u32 = 0x4;
const RECORD_TYPE: u32 = 0x8;
const RECORD_REFS: u32 = 0xC;
const RECORD_STATUS: u32 = 0x14;
/// The lookup code searched for a resolver's capacity.
const LOOKUP_BYTES: u32 = 0x80;

/// What a record's status says.
pub const STATUS_RESIDENT: u16 = 2;

fn need<M: Mem>(m: &M, addr: u32, n: u32, what: &str) -> Result<(), String> {
    if addr >= 0x10000 && m.readable_span(addr) >= n {
        Ok(())
    } else {
        Err(format!("{what} at {addr:#x} is not readable"))
    }
}

fn read_u16<M: Mem>(m: &M, addr: u32) -> u16 {
    u16::from(m.read_u8(addr)) | (u16::from(m.read_u8(addr + 1)) << 8)
}

/// The camera's eye, in world metres. `p` is what `Player.GetCamera` returned.
pub fn camera_eye<M: Mem>(m: &M, p: u32) -> Result<[f32; 3], String> {
    need(m, p + OWNER_RECORD, 4, "the camera owner")?;
    let record = m.read_u32(p + OWNER_RECORD);
    need(m, record + RECORD_EYE, 12, "the camera record")?;
    let eye = [0, 4, 8].map(|o| f32::from_bits(m.read_u32(record + RECORD_EYE + o)));
    if eye.iter().all(|c| c.is_finite() && c.abs() < 1e6) {
        Ok(eye)
    } else {
        Err(format!("the camera record at {record:#x} holds {eye:?}, not a position"))
    }
}

/// The registry's counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Records registered, over every type.
    pub records: u32,
    /// Resident blocks.
    pub blocks: u32,
    /// Type slots in use.
    pub types: u32,
}

fn manager<M: Mem>(m: &M, slot: u32) -> Result<u32, String> {
    need(m, slot, 4, "the manager pointer")?;
    let mgr = m.read_u32(slot);
    need(m, mgr, BLOCKS + 4, "the asset manager")?;
    Ok(mgr)
}

/// The counters, read from the resolvers' own counts: one read per type, cheap enough for a frame.
pub fn stats<M: Mem>(m: &M, slot: u32) -> Result<Stats, String> {
    let mgr = manager(m, slot)?;
    let (mut records, mut types) = (0u32, 0u32);
    for i in 0..SLOTS {
        if m.read_u32(mgr + TYPE_KEYS + i * 4) == 0 {
            continue;
        }
        let res = m.read_u32(mgr + RESOLVERS + i * 4);
        need(m, res, RESOLVER_RECORDS, "a resolver")?;
        records = records.saturating_add(m.read_u32(res + RESOLVER_COUNT));
        types += 1;
    }
    Ok(Stats { records, blocks: m.read_u32(mgr + BLOCKS), types })
}

/// `N` and the offset of `nameKeys`, from the lookup code's `push N; lea esi, [edi + keys]`, in its three
/// encodings. The offset must be `0xC + 4N`, so a resolver of another shape is refused.
fn capacity<M: Mem>(m: &M, res: u32) -> Result<(u32, u32), String> {
    need(m, res, 4, "a resolver")?;
    let vtable = m.read_u32(res);
    need(m, vtable + 8, 4, "a resolver's vtable")?;
    let lookup = m.read_u32(vtable + 8);
    need(m, lookup, LOOKUP_BYTES, "a resolver's lookup code")?;
    let code: Vec<u8> = (0..LOOKUP_BYTES).map(|i| m.read_u8(lookup + i)).collect();
    let u32_at = |i: usize| u32::from_le_bytes([code[i], code[i + 1], code[i + 2], code[i + 3]]);
    for i in 0..code.len() - 11 {
        let (cap, keys) = match (code[i], code[i + 1]) {
            (0x68, _) if code[i + 5..i + 7] == [0x8D, 0xB7] => (u32_at(i + 1), u32_at(i + 7)),
            (0x6A, _) if code[i + 2..i + 4] == [0x8D, 0xB7] => (u32::from(code[i + 1]), u32_at(i + 4)),
            (0x6A, _) if code[i + 2..i + 4] == [0x8D, 0x77] => (u32::from(code[i + 1]), u32::from(code[i + 4])),
            _ => continue,
        };
        if keys == RESOLVER_RECORDS + cap * 4 {
            return Ok((cap, keys));
        }
    }
    Err(format!("the resolver at {res:#x} has no capacity this code can read"))
}

/// One asset's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    /// No record: not registered.
    Absent,
    /// A record: its status and the sum of its references.
    Record { status: u16, refs: u32 },
}

/// The state of `(name, type)`. An unknown type is `Absent`: nothing of it is registered.
pub fn asset<M: Mem>(m: &M, slot: u32, name: u32, ty: u32) -> Result<Asset, String> {
    let mgr = manager(m, slot)?;
    let Some(i) = (0..SLOTS).find(|i| m.read_u32(mgr + TYPE_KEYS + i * 4) == ty) else {
        return Ok(Asset::Absent);
    };
    let res = m.read_u32(mgr + RESOLVERS + i * 4);
    let (cap, keys) = capacity(m, res)?;
    need(m, res + RESOLVER_RECORDS, cap * 4, "a resolver's records")?;
    need(m, res + keys, cap * 4, "a resolver's keys")?;
    let Some(k) = (0..cap).find(|k| m.read_u32(res + keys + k * 4) == name) else {
        return Ok(Asset::Absent);
    };
    let rec = m.read_u32(res + RESOLVER_RECORDS + k * 4);
    need(m, rec, RECORD_STATUS + 4, "a record")?;
    if m.read_u32(rec + RECORD_NAME) != name || m.read_u32(rec + RECORD_TYPE) != ty {
        return Err(format!("the record at {rec:#x} is not {name:#010x}/{ty:#010x}"));
    }
    let refs = (0..4).map(|r| u32::from(read_u16(m, rec + RECORD_REFS + r * 2))).sum();
    Ok(Asset::Record { status: read_u16(m, rec + RECORD_STATUS), refs })
}

/// Parse `name:type,name:type`, hexadecimal: Lua's numbers are 32-bit floats and would round a hash.
pub fn parse_pairs(text: &str) -> Result<Vec<(u32, u32)>, String> {
    let hex = |s: &str| u32::from_str_radix(s, 16).map_err(|_| format!("{s:?} is not a hexadecimal hash"));
    text.split(',')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (n, t) = p.split_once(':').ok_or_else(|| format!("{p:?} is not name:type"))?;
            Ok((hex(n)?, hex(t)?))
        })
        .collect()
}

/// `status:refs,…` in order, `-` for an absent asset: what `M2Live.AssetStatus` returns.
pub fn render_assets(assets: &[Asset]) -> String {
    let parts: Vec<String> = assets
        .iter()
        .map(|a| match a {
            Asset::Absent => "-".to_string(),
            Asset::Record { status, refs } => format!("{status}:{refs}"),
        })
        .collect();
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Memory as regions of bytes.
    #[derive(Default)]
    struct Fake(BTreeMap<u32, Vec<u8>>);

    impl Fake {
        fn region(&self, addr: u32) -> Option<(u32, &Vec<u8>)> {
            self.0.range(..=addr).next_back().filter(|(b, v)| addr < *b + v.len() as u32).map(|(b, v)| (*b, v))
        }

        fn put(&mut self, addr: u32, bytes: Vec<u8>) {
            self.0.insert(addr, bytes);
        }

        fn set32(&mut self, addr: u32, v: u32) {
            let (b, r) = self.0.range_mut(..=addr).next_back().expect("a region");
            let at = (addr - *b) as usize;
            r[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
    }

    impl Mem for Fake {
        fn readable_span(&self, addr: u32) -> u32 {
            self.region(addr).map_or(0, |(b, v)| b + v.len() as u32 - addr)
        }
        fn read_u32(&self, addr: u32) -> u32 {
            u32::from_le_bytes([0, 1, 2, 3].map(|i| self.read_u8(addr + i)))
        }
        fn read_u8(&self, addr: u32) -> u8 {
            let (b, v) = self.region(addr).expect("a read in a mapped region");
            v[(addr - b) as usize]
        }
    }

    const SLOT: u32 = 0x0100_0000;
    const MGR: u32 = 0x0200_0000;
    const RES: u32 = 0x0300_0000;
    const VT: u32 = 0x0400_0000;
    const CODE: u32 = 0x0500_0000;
    const REC: u32 = 0x0600_0000;
    const TY: u32 = 0x980F_0718;
    const NAME: u32 = 0x25C9_8327;

    /// A registry with one type of capacity 0x100 and one record, its lookup code in the 32-bit push form.
    fn registry() -> Fake {
        let mut m = Fake::default();
        m.put(SLOT, vec![0; 4]);
        m.set32(SLOT, MGR);
        m.put(MGR, vec![0; 0x900]);
        m.set32(MGR + TYPE_KEYS, TY);
        m.set32(MGR + RESOLVERS, RES);
        m.set32(MGR + BLOCKS, 653);
        const CAP: u32 = 0x100;
        m.put(RES, vec![0; (0xC + CAP * 8) as usize]);
        m.set32(RES, VT);
        m.set32(RES + RESOLVER_COUNT, 1);
        m.set32(RES + RESOLVER_RECORDS + 7 * 4, REC);
        m.set32(RES + RESOLVER_RECORDS + CAP * 4 + 7 * 4, NAME);
        m.put(VT, vec![0; 12]);
        m.set32(VT + 8, CODE);
        let mut code = vec![0x90u8; LOOKUP_BYTES as usize];
        code[3] = 0x68;
        code[4..8].copy_from_slice(&CAP.to_le_bytes());
        code[8..10].copy_from_slice(&[0x8D, 0xB7]);
        code[10..14].copy_from_slice(&(0xC + CAP * 4).to_le_bytes());
        m.put(CODE, code);
        m.put(REC, vec![0; 0x18]);
        m.set32(REC + RECORD_NAME, NAME);
        m.set32(REC + RECORD_TYPE, TY);
        m.set32(REC + RECORD_REFS, 0x0003_0002);
        m.set32(REC + RECORD_STATUS, u32::from(STATUS_RESIDENT));
        m
    }

    #[test]
    fn the_eye_is_two_hops_from_the_owner() {
        let (p, record) = (0x0700_0000, 0x0800_0000);
        let mut m = Fake::default();
        m.put(p, vec![0; 0x200]);
        m.set32(p + OWNER_RECORD, record);
        m.put(record, vec![0; 0x60]);
        for (i, v) in [2635.0f32, -13.8, -999.0].iter().enumerate() {
            m.set32(record + RECORD_EYE + i as u32 * 4, v.to_bits());
        }
        assert_eq!(camera_eye(&m, p), Ok([2635.0, -13.8, -999.0]));
    }

    #[test]
    fn a_bad_camera_is_an_error_not_a_crash() {
        assert!(camera_eye(&Fake::default(), 0x1234_0000).unwrap_err().contains("not readable"));
        let p = 0x0700_0000;
        let mut m = Fake::default();
        m.put(p, vec![0; 0x200]);
        m.set32(p + OWNER_RECORD, 0x0900_0000);
        assert!(camera_eye(&m, p).unwrap_err().contains("record"), "a dangling record must not be read");
        let record = 0x0800_0000;
        m.set32(p + OWNER_RECORD, record);
        m.put(record, vec![0; 0x60]);
        m.set32(record + RECORD_EYE, f32::NAN.to_bits());
        assert!(camera_eye(&m, p).unwrap_err().contains("not a position"));
    }

    #[test]
    fn stats_read_the_resolvers_counts() {
        assert_eq!(stats(&registry(), SLOT), Ok(Stats { records: 1, blocks: 653, types: 1 }));
    }

    #[test]
    fn stats_never_read_zero_for_a_missing_manager() {
        assert!(stats(&Fake::default(), SLOT).is_err());
    }

    #[test]
    fn an_asset_reads_its_status_and_references() {
        assert_eq!(asset(&registry(), SLOT, NAME, TY), Ok(Asset::Record { status: 2, refs: 5 }));
    }

    #[test]
    fn what_is_not_registered_is_absent() {
        assert_eq!(asset(&registry(), SLOT, NAME + 1, TY), Ok(Asset::Absent));
        assert_eq!(asset(&registry(), SLOT, NAME, TY + 1), Ok(Asset::Absent));
    }

    #[test]
    fn a_record_that_is_not_the_one_asked_for_is_refused() {
        let mut m = registry();
        m.set32(REC + RECORD_NAME, NAME + 1);
        assert!(asset(&m, SLOT, NAME, TY).unwrap_err().contains("is not"));
    }

    #[test]
    fn a_resolver_of_another_shape_is_refused() {
        let mut m = registry();
        m.put(CODE, vec![0x90; LOOKUP_BYTES as usize]);
        assert!(asset(&m, SLOT, NAME, TY).unwrap_err().contains("capacity"));
    }

    #[test]
    fn the_short_encodings_read_their_capacity() {
        for (form, cap_at, keys_len) in [(&[0x8Du8, 0xB7][..], 1, 4usize), (&[0x8D, 0x77][..], 1, 1)] {
            let mut m = Fake::default();
            m.put(RES, vec![0; 16]);
            m.set32(RES, VT);
            m.put(VT, vec![0; 12]);
            m.set32(VT + 8, CODE);
            let cap = 0x20u8;
            let keys = 0xC + u32::from(cap) * 4;
            let mut code = vec![0x90u8; LOOKUP_BYTES as usize];
            code[0] = 0x6A;
            code[cap_at] = cap;
            code[2..4].copy_from_slice(form);
            code[4..4 + keys_len].copy_from_slice(&keys.to_le_bytes()[..keys_len]);
            m.put(CODE, code);
            assert_eq!(capacity(&m, RES), Ok((u32::from(cap), keys)));
        }
    }

    #[test]
    fn pairs_travel_as_text_because_lua_numbers_are_floats() {
        assert_eq!(parse_pairs("25C98327:980F0718,1:2"), Ok(vec![(0x25C98327, 0x980F0718), (1, 2)]));
        assert_eq!(parse_pairs(""), Ok(vec![]));
        assert!(parse_pairs("nothex:1").is_err());
        assert!(parse_pairs("12").is_err());
    }

    #[test]
    fn results_keep_their_order() {
        let got = render_assets(&[Asset::Record { status: 2, refs: 5 }, Asset::Absent]);
        assert_eq!(got, "2:5,-");
    }

    /// The pattern parses, and its operand sits at the `mov ecx, [imm32]` the docs name.
    #[test]
    fn the_manager_pattern_holds_its_operand() {
        let p = crate::lua_sites::Pattern::parse(MANAGER_PATTERN).expect("the pattern parses");
        let mut code = vec![0xA8, 0x40, 0x53, 0x55, 0x57, 0x75, 0x10, 0x8B, 0x0D];
        code.extend_from_slice(&0x0117_5630u32.to_le_bytes());
        code.extend_from_slice(&[0x8B, 0xA9, 0x60, 0xC3, 0x04, 0x00]);
        assert_eq!(p.find_all(&code), [0]);
        assert_eq!(u32::from_le_bytes(code[MANAGER_OPERAND..MANAGER_OPERAND + 4].try_into().unwrap()), 0x0117_5630);
    }
}
