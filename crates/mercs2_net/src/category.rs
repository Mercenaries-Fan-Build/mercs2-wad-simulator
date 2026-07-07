//! Replication categories — the per-object property-sync descriptor `NetCategoryInfo`
//! (`FUN_00644510`, networking code map §3).
//!
//! Which per-object properties are network-synced is a **stream-type descriptor** registered like
//! every other ECS component descriptor. `FUN_00644510` (159 B, one-shot init) installs it with the
//! shared registrar template (read first-hand, §3):
//!
//! ```c
//! _DAT_017bebc4 = 0x100;                              // pool size 0x100
//! PTR_PTR_017bebb8 = &PTR_CopyFromStream_00bc1f40;    // WAD stream-deserialize vtable
//! _DAT_017bebe4 = 0x9e3779b9;                         // golden-ratio hash seed
//! PTR_s_NetCategoryInfo_017bebf4 = "NetCategoryInfo";
//! ```
//!
//! The descriptor backs the recovered category set: one **primary** class plus eight **sub-cats**,
//! each an independently-serialized typed sub-stream (§3) — the engine syncs `NetSubCatHealth` /
//! `NetSubCatInventory` / … separately, not one monolithic snapshot. This granularity is the reason
//! the model replicates node/health/importance rather than full transforms (GDC 2008: both peers
//! simulate destruction locally with periodic corrections).
//!
//! **Honest boundary:** the numeric category **nibble** each sub-cat occupies in the packet header
//! (`hdr >> 4`, message §2.1) is the data table *behind* `FUN_00644510` and is **not recovered** — the
//! Xbox `.rdata` gives the names, not their nibble indices. So this enum names the recovered
//! categories and their synced property, but does **not** fabricate a nibble mapping.

/// Pool size the `NetCategoryInfo` descriptor presizes (`_DAT_017bebc4 = 0x100`, §3).
pub const CATEGORY_POOL_SIZE: u32 = 0x100;

/// The golden-ratio hash seed the shared descriptor registrar uses (`_DAT_017bebe4 = 0x9e3779b9`, §3).
pub const CATEGORY_HASH_SEED: u32 = 0x9e37_79b9;

/// The recovered replication categories (§3). `Primary` is the object's replication class;
/// the eight `SubCat*` are the independently-synced typed sub-streams.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetCategory {
    /// `NetPrimaryCategory` — the primary replication class of the object.
    Primary,
    /// `NetSubCatFriendOrFoe` — faction / hostility relation.
    FriendOrFoe,
    /// `NetSubCatIsImportant` — "important object" flag (prioritized sync).
    IsImportant,
    /// `NetSubCatInventory` — held items / weapons.
    Inventory,
    /// `NetSubCatSeatLink` — vehicle-seat occupancy linkage.
    SeatLink,
    /// `NetSubCatPoweredGate` — powered gate / door state.
    PoweredGate,
    /// `NetSubCatNodeHealth` — destructible node health.
    NodeHealth,
    /// `NetSubCatHealth` — actor health.
    Health,
}

impl NetCategory {
    /// Every recovered category, in the `.rdata` declaration order (§3 table).
    pub const ALL: [NetCategory; 8] = [
        NetCategory::Primary,
        NetCategory::FriendOrFoe,
        NetCategory::IsImportant,
        NetCategory::Inventory,
        NetCategory::SeatLink,
        NetCategory::PoweredGate,
        NetCategory::NodeHealth,
        NetCategory::Health,
    ];

    /// The engine symbol name for this category (the Xbox `.rdata` string it registers under, §3).
    pub fn symbol(self) -> &'static str {
        match self {
            NetCategory::Primary => "NetPrimaryCategory",
            NetCategory::FriendOrFoe => "NetSubCatFriendOrFoe",
            NetCategory::IsImportant => "NetSubCatIsImportant",
            NetCategory::Inventory => "NetSubCatInventory",
            NetCategory::SeatLink => "NetSubCatSeatLink",
            NetCategory::PoweredGate => "NetSubCatPoweredGate",
            NetCategory::NodeHealth => "NetSubCatNodeHealth",
            NetCategory::Health => "NetSubCatHealth",
        }
    }

    /// The 32-bit name-hash the descriptor registrar addresses this category by (`pandemic_hash_m2`
    /// of the symbol — the same hashing the seed/`CopyFromStream` template uses for every descriptor).
    pub fn name_hash(self) -> u32 {
        mercs2_formats::hash::pandemic_hash_m2(self.symbol())
    }
}

/// The two channel tokens the category streams ride (`NetCommand` / `NetNotify`, rdata `0x0013a1c` /
/// `0x0013a28`, §3) — a command mutates host-authoritative state; a notify is a client-facing push.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetChannel {
    /// `NetCommand` — host-authoritative mutation.
    Command,
    /// `NetNotify` — client-facing notification.
    Notify,
}

impl NetChannel {
    pub fn symbol(self) -> &'static str {
        match self {
            NetChannel::Command => "NetCommand",
            NetChannel::Notify => "NetNotify",
        }
    }

    pub fn name_hash(self) -> u32 {
        mercs2_formats::hash::pandemic_hash_m2(self.symbol())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_categories_with_distinct_hashes() {
        assert_eq!(NetCategory::ALL.len(), 8);
        let mut hashes: Vec<u32> = NetCategory::ALL.iter().map(|c| c.name_hash()).collect();
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(hashes.len(), 8, "each category symbol hashes distinctly");
    }

    #[test]
    fn recovered_registrar_constants() {
        assert_eq!(CATEGORY_POOL_SIZE, 0x100);
        assert_eq!(CATEGORY_HASH_SEED, 0x9e37_79b9);
    }

    #[test]
    fn channel_symbols() {
        assert_eq!(NetChannel::Command.symbol(), "NetCommand");
        assert_ne!(NetChannel::Command.name_hash(), NetChannel::Notify.name_hash());
    }
}
