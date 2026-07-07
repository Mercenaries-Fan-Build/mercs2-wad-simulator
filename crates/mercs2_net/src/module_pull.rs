//! Join-time module pull — `SynchNetImportModule` / `NetSynchImportModule` (networking code map §4).
//!
//! When a client joins mid-session it must receive the host's authoritative **Lua module state** (the
//! assembled world / mission script tables) before any event for that module can be delivered. The
//! Xbox body is read first-hand (§4):
//!
//! ```c
//! undefined8 SynchNetImportModule(param_1, param_2) {          // Xbox @825ce918
//!   iVar1 = FUN_8241bb78(0x762c8f61);                          // the import-module registry record
//!   if ((iVar1 != 0) && (0 < *(int*)(iVar1 + 0x40))) {         // registry exists & populated
//!     iVar1 = FUN_8241bb78(param_2);                            // is THIS module already synced?
//!     if ((iVar1 != 0) && (0 < *(int*)(iVar1 + 0x40))) return 1;// yes → deliver
//!     FUN_82315658(auStack_20, param_2);                        // no → push module hash (handle arg)
//!     FUN_82420690(auStack_20, 0x762c8f61, ...);               // emit the pull event
//!   }
//! }
//! ```
//!
//! `NetEventCallback` calls this **first** (message §2.1 line (a)), so an inbound event for a
//! not-yet-synced module **triggers a pull before the event fires** — the client requests the host's
//! module state, the host ships it, and only then does the queued event deliver. The pulled module is
//! the host's authoritative script-table snapshot for that channel (`"MrxFactionManager"`,
//! `"WifPmcInterior"`, … — the channels the Lua `Net.SendCustomEvent(...)` calls name).

use crate::message::{NetArg, NetMessage};
use std::collections::HashSet;

/// The import-module registry hash the gate keys on (`FUN_8241bb78(0x762c8f61)`, §4). Also the
/// name-hash the emitted pull event carries (`FUN_82420690(frame, 0x762c8f61, …)`).
pub const IMPORT_MODULE_REGISTRY_HASH: u32 = 0x762c_8f61;

/// The join-time module-sync state — which module hashes this peer has already pulled/synced. Backs
/// the `FUN_8241bb78(moduleHash) → +0x40 count` "is this module populated" check (§4), modeled as a
/// synced-set: a module is "synced" once its state has been received.
#[derive(Default, Clone, Debug)]
pub struct ModulePullState {
    synced: HashSet<u32>,
}

impl ModulePullState {
    pub fn new() -> ModulePullState {
        ModulePullState::default()
    }

    /// Mark a module hash as synced (its authoritative snapshot has been received/applied).
    pub fn mark_synced(&mut self, module_hash: u32) {
        self.synced.insert(module_hash);
    }

    /// Whether the module's state has been synced (the `+0x40 > 0` populated check, §4).
    pub fn is_synced(&self, module_hash: u32) -> bool {
        self.synced.contains(&module_hash)
    }

    /// The gate `SynchNetImportModule` runs **before** delivering an inbound event (§2.1 line (a)):
    /// if the event's module is already synced, return `None` (deliver immediately); otherwise return
    /// the **pull request** event — name-hash `IMPORT_MODULE_REGISTRY_HASH`, one `Handle(module_hash)`
    /// arg — that the client emits to ask the host for that module's state. Faithful to the Xbox body:
    /// synced → `return 1` (deliver), unsynced → push module hash + emit pull.
    pub fn gate_inbound(&self, module_hash: u32) -> Option<NetMessage> {
        if self.is_synced(module_hash) {
            None
        } else {
            Some(pull_request(module_hash))
        }
    }
}

/// Build the module-pull request event the gate emits for an unsynced module (§4): the
/// `NetSynchImportModule` event carrying the requested module hash as a single handle arg. Category 0
/// — the recovered emit `FUN_82420690(frame, 0x762c8f61, …, 1,0,1,0)` pushes exactly one arg.
pub fn pull_request(module_hash: u32) -> NetMessage {
    NetMessage::new(IMPORT_MODULE_REGISTRY_HASH, 0, vec![NetArg::Handle(module_hash)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::hash::pandemic_hash_m2;

    #[test]
    fn unsynced_module_gates_to_a_pull_request() {
        let state = ModulePullState::new();
        let module = pandemic_hash_m2("MrxFactionManager");
        let req = state.gate_inbound(module).expect("unsynced module must pull first");
        assert_eq!(req.name_hash, IMPORT_MODULE_REGISTRY_HASH);
        assert_eq!(req.args, vec![NetArg::Handle(module)]);
    }

    #[test]
    fn synced_module_delivers_without_a_pull() {
        let mut state = ModulePullState::new();
        let module = pandemic_hash_m2("WifPmcInterior");
        state.mark_synced(module);
        assert!(state.gate_inbound(module).is_none(), "synced → deliver, no pull");
    }

    #[test]
    fn pull_request_survives_the_wire() {
        let module = 0x1234_5678;
        let bytes = pull_request(module).marshal().unwrap();
        let back = NetMessage::unmarshal(&bytes).unwrap();
        assert_eq!(back.name_hash, IMPORT_MODULE_REGISTRY_HASH);
        assert_eq!(back.args, vec![NetArg::Handle(module)]);
    }
}
