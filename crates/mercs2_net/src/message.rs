//! The replicated event/RPC message — the "32-bit name-hash + typed-TLV" record and its marshal
//! boundary (networking code map §2.1).
//!
//! The on-wire packet is recovered first-hand from the **Xbox receive decoder** `NetEventCallback`
//! (`@825d3ce8`) — the one place the serialized record is unpacked field-by-field (§2.1):
//!
//! ```c
//! FUN_822ef440(frame, *(byte*)(param_1+8) >> 4);            // (c) CATEGORY nibble = hdr >> 4
//! switch(*(undefined1*)((int)param_1 + uVar3 + 4)) {        //     per-arg TYPE TAG
//!   case 0: /* string/guid */  case 1: /* int */
//!   case 2: /* float */        case 3: /* handle/guid */ }
//! while (uVar3 < (*(byte*)(param_1+8) >> 1 & 7));           //     ARGC = hdr >> 1 & 7  (cap 7)
//! FUN_82420690(frame, *param_1, ...);                       // (d) dispatch on EVENT_HASH = *param_1
//! ```
//!
//! Recovered record shape (§2.1 table):
//!
//! | Field           | Bytes      | Meaning                                                        |
//! |-----------------|------------|---------------------------------------------------------------|
//! | `record[0..3]`  | u32        | **event name-hash** (`pandemic_hash_m2` of the event/channel) |
//! | `record[+4..7]` | 4× u8      | per-arg **type tags** (0 str/guid, 1 int, 2 float, 3 handle)   |
//! | `record[+8]`    | u8         | **header**: `>>4` = category nibble; `>>1 & 7` = argc (max 7)  |
//! | `record[+0xc…]` | argc× u32  | argument payload words (interpreted by the type tag)          |
//!
//! **Honest boundary — the exact PC on-wire byte encoding is confirm-live, NOT recovered.** The PC
//! marshal core `FUN_005a0cc0` builds the packet body through the SecuROM-virtualized VM residue
//! `thunk_FUN_02935000` (encode) → `thunk_FUN_024f28e0` (emit) (§2.2, §9), so the precise byte order,
//! the `+0xc` payload padding, and whether high-argc records still pack four inline tag bytes are an
//! x32dbg confirm-live item. What is authoritative and modeled here is the **logical record**: a
//! 32-bit name-hash, a category nibble, and an argc-capped stream of typed args over the recovered
//! 4-type set. [`NetMessage::marshal`] emits a **self-consistent, round-trippable** byte form that
//! preserves every recovered field; it is the *marshal boundary*, deliberately not a fabricated claim
//! about the virtualized wire bytes.

/// The recovered argument-count ceiling — the header packs argc in `>>1 & 7`, so **7 args max** per
/// event (§2.1). Matches the in-memory event bus (Keystone B: argc ≤ 7).
pub const MAX_ARGS: usize = 7;

/// The event-frame slot budget the marshal core reserves against — `FUN_005a0cc0`'s capacity check
/// `(end - cursor) >> 3` (free 8-byte slots) is byte-identical to the Xbox marshal `FUN_82878c50`
/// whose bound is `0x801` (§2.2). ~2048 8-byte slots; the reserve refuses once the frame is full.
pub const FRAME_SLOT_CAP: usize = 0x801;

/// One typed argument — the four recovered per-arg **type tags** from the Xbox decoder switch (§2.1).
/// Each carries a single 32-bit payload word (the record's `argc× u32` payload region); the tag byte
/// selects how the word is interpreted, exactly as `NetEventCallback`'s `switch` does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetArg {
    /// Tag `0` — a string/guid reference, carried as its 32-bit id word (strings ride as hashes/guids).
    Str(u32),
    /// Tag `1` — a signed integer (e.g. a `NETEVENT_*` id or count).
    Int(i32),
    /// Tag `2` — a 32-bit float.
    Float(f32),
    /// Tag `3` — an engine object handle / guid word.
    Handle(u32),
}

impl NetArg {
    /// The recovered type-tag byte for this arg (`NetEventCallback`'s `case 0..3`).
    pub fn tag(self) -> u8 {
        match self {
            NetArg::Str(_) => 0,
            NetArg::Int(_) => 1,
            NetArg::Float(_) => 2,
            NetArg::Handle(_) => 3,
        }
    }

    /// The 32-bit payload word for this arg (the record's payload region stores one word per arg).
    /// Float is bit-cast (the decoder reads `(float)param_1[..]`); int is two's-complement bits.
    pub fn payload_word(self) -> u32 {
        match self {
            NetArg::Str(w) | NetArg::Handle(w) => w,
            NetArg::Int(i) => i as u32,
            NetArg::Float(f) => f.to_bits(),
        }
    }

    /// Rebuild an arg from a `(tag, word)` pair — the receive-side interpretation (§2.1 switch).
    /// Returns `None` for an unrecovered tag (>3): the decoder only defines cases 0..3.
    pub fn from_tag_word(tag: u8, word: u32) -> Option<NetArg> {
        match tag {
            0 => Some(NetArg::Str(word)),
            1 => Some(NetArg::Int(word as i32)),
            2 => Some(NetArg::Float(f32::from_bits(word))),
            3 => Some(NetArg::Handle(word)),
            _ => None,
        }
    }
}

/// A replicated event/RPC message — the logical form of the recovered record: a 32-bit name-hash, a
/// 4-bit category/target nibble, and up to [`MAX_ARGS`] typed args. This is what `Net.SendCustomEvent`
/// / `Net.SendEvent_*` build and what the receive side re-drives onto the local bus.
#[derive(Clone, Debug, PartialEq)]
pub struct NetMessage {
    /// The event name-hash (`record[0..3]`) — `pandemic_hash_m2` of the channel/event name.
    pub name_hash: u32,
    /// The category/target nibble (`hdr >> 4`, 0..15). The numeric nibble↔`NetSubCat*` mapping is the
    /// data table behind `FUN_00644510` and is **not recovered**; this is the raw recovered field.
    pub category: u8,
    /// The typed argument stream (argc = `hdr >> 1 & 7`, capped at [`MAX_ARGS`]).
    pub args: Vec<NetArg>,
}

/// A marshal/unmarshal failure at the modeled boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// More than [`MAX_ARGS`] args — the header's 3-bit argc field cannot represent it (§2.1).
    TooManyArgs(usize),
    /// The byte buffer is shorter than the header + declared argc payload requires.
    Truncated,
    /// A payload arg carried a type tag outside the recovered 0..3 set.
    BadTag(u8),
}

impl NetMessage {
    /// Build a message with the name pre-hashed, a category nibble, and typed args.
    pub fn new(name_hash: u32, category: u8, args: Vec<NetArg>) -> NetMessage {
        NetMessage { name_hash, category: category & 0x0f, args }
    }

    /// The recovered header byte: `(category << 4) | (argc << 1)` (§2.1; bit0 is spare). Argc is the
    /// live arg count masked to the header's 3-bit field.
    pub fn header(&self) -> u8 {
        let argc = (self.args.len() as u8) & 0x07;
        ((self.category & 0x0f) << 4) | (argc << 1)
    }

    /// Marshal to the wire byte form — the **marshal boundary** (see module docs; the true virtualized
    /// PC encoding is confirm-live). Layout preserves every recovered field and round-trips through
    /// [`NetMessage::unmarshal`]:
    ///
    /// `[name_hash u32 LE][header u8]` then, per arg, `[tag u8][payload_word u32 LE]`.
    ///
    /// This keeps each arg's tag adjacent to its word so the record is self-describing for any argc up
    /// to 7 — sidestepping the unrecovered fixed 4-tag / `+0xc` padding detail rather than inventing it.
    /// Returns `Err(TooManyArgs)` if the arg count exceeds the header's representable ceiling.
    pub fn marshal(&self) -> Result<Vec<u8>, WireError> {
        if self.args.len() > MAX_ARGS {
            return Err(WireError::TooManyArgs(self.args.len()));
        }
        let mut out = Vec::with_capacity(5 + self.args.len() * 5);
        out.extend_from_slice(&self.name_hash.to_le_bytes());
        out.push(self.header());
        for arg in &self.args {
            out.push(arg.tag());
            out.extend_from_slice(&arg.payload_word().to_le_bytes());
        }
        Ok(out)
    }

    /// Rebuild a message from the wire byte form — the receive-side decode (`NetEventCallback`'s
    /// unpack, modeled against [`NetMessage::marshal`]'s boundary). Reads the header's argc, then that
    /// many `[tag][word]` pairs, interpreting each tag through the recovered 0..3 switch.
    pub fn unmarshal(bytes: &[u8]) -> Result<NetMessage, WireError> {
        if bytes.len() < 5 {
            return Err(WireError::Truncated);
        }
        let name_hash = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let header = bytes[4];
        let category = header >> 4;
        let argc = ((header >> 1) & 0x07) as usize;
        let mut args = Vec::with_capacity(argc);
        let mut off = 5;
        for _ in 0..argc {
            if off + 5 > bytes.len() {
                return Err(WireError::Truncated);
            }
            let tag = bytes[off];
            let word = u32::from_le_bytes([
                bytes[off + 1],
                bytes[off + 2],
                bytes[off + 3],
                bytes[off + 4],
            ]);
            args.push(NetArg::from_tag_word(tag, word).ok_or(WireError::BadTag(tag))?);
            off += 5;
        }
        Ok(NetMessage { name_hash, category, args })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::hash::pandemic_hash_m2;

    #[test]
    fn header_packs_category_and_argc() {
        let m = NetMessage::new(0, 0x5, vec![NetArg::Int(1), NetArg::Float(2.0)]);
        // category 0x5 in high nibble, argc 2 in >>1&7  => (5<<4)|(2<<1) = 0x54
        assert_eq!(m.header(), 0x54);
        assert_eq!(m.header() >> 4, 0x5);
        assert_eq!((m.header() >> 1) & 7, 2);
    }

    #[test]
    fn marshal_roundtrips_all_arg_types() {
        let m = NetMessage::new(
            pandemic_hash_m2("MrxFactionManager"),
            0x3,
            vec![
                NetArg::Int(-320369524),
                NetArg::Float(1.5),
                NetArg::Handle(0xDEAD_BEEF),
                NetArg::Str(0x51ee_8f14),
            ],
        );
        let bytes = m.marshal().expect("marshal");
        let back = NetMessage::unmarshal(&bytes).expect("unmarshal");
        assert_eq!(m, back);
    }

    #[test]
    fn argc_ceiling_is_seven() {
        let mut args = vec![NetArg::Int(0); MAX_ARGS];
        assert!(NetMessage::new(1, 0, args.clone()).marshal().is_ok());
        args.push(NetArg::Int(0)); // 8th
        assert_eq!(
            NetMessage::new(1, 0, args).marshal(),
            Err(WireError::TooManyArgs(8))
        );
    }

    #[test]
    fn unmarshal_rejects_truncated_and_bad_tag() {
        assert_eq!(NetMessage::unmarshal(&[0, 0, 0]), Err(WireError::Truncated));
        // header claims 1 arg but payload is cut short
        assert_eq!(
            NetMessage::unmarshal(&[1, 0, 0, 0, 0x02]),
            Err(WireError::Truncated)
        );
        // header claims 1 arg, tag byte 9 is outside the recovered 0..3 set
        assert_eq!(
            NetMessage::unmarshal(&[1, 0, 0, 0, 0x02, 9, 0, 0, 0, 0]),
            Err(WireError::BadTag(9))
        );
    }

    #[test]
    fn float_arg_is_bit_exact() {
        let m = NetMessage::new(7, 0, vec![NetArg::Float(3.14159)]);
        let back = NetMessage::unmarshal(&m.marshal().unwrap()).unwrap();
        assert_eq!(back.args[0], NetArg::Float(3.14159));
    }
}
