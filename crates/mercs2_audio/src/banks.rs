//! Sound/wave bank load state machine.
//!
//! **Oracle (audio_code_map.md §4.3):**
//! * `UpdateLoads` **`FUN_00601dd0`** — a **65-slot (`0x41 × 0x1c`) bank-load state machine** on the
//!   bank manager `DAT_01175f9c`.
//! * `FUN_00602880` bank-slot release/unload (on `soundbank` `0x9F8BCA10` it re-requests the sounddb block).
//! * `FUN_00603110` soundbank/wavebank **async load completion** (Chunk_GetEntryReader → Chunk_Alloc → fixups).
//! * Lua: `Sound.LoadSoundBank` (`FUN_005e2630`), `Sound.LoadWaveBank` (`FUN_005e26b0`),
//!   `Sound.LoadBankWithCallback`/`UnloadBankWithCallback` — both to `FUN_006026c0`, and the Lua
//!   loader (`MrxSoundBanks`) throttles to **64 in-flight** requests.
//!
//! Bank *file* I/O goes through the WAD streaming manager on PC (`FUN_00872f80` etc.), which this
//! crate does not own; here the state machine is faithful (request → loading → loaded/unloaded, with
//! a completion callback) and [`BankManager::complete_load`]/[`complete_unload`] stand in for the
//! async completion the streaming silo will drive.

use std::collections::HashMap;

/// Bank-load slot count (`FUN_00601dd0`: `0x41` slots).
pub const BANK_SLOTS: usize = 0x41; // 65

/// Max in-flight bank requests (`MrxSoundBanks.MAX_SUBMITTED`, luacd §2).
pub const MAX_SUBMITTED: usize = 64;

/// Bank kind — distinct m2 asset types (audio_code_map.md §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankKind {
    /// `soundbank` 0x9F8BCA10 — cue/metadata bank.
    Sound,
    /// `wavebank` 0xF753F6D0 — the PCM waves.
    Wave,
    /// A temp bank (`Sound.LoadTempBank`).
    Temp,
    /// An ambience bank (`Sound.RequestAmbienceBank`, lib ≥ 12).
    Ambience,
}

impl BankKind {
    /// The m2 asset-type hash for this kind.
    pub fn type_hash(&self) -> u32 {
        match self {
            BankKind::Sound => 0x9F8B_CA10,
            BankKind::Wave => 0xF753_F6D0,
            BankKind::Temp => 0x9F8B_CA10,
            BankKind::Ambience => 0x9F8B_CA10,
        }
    }
}

/// Per-slot load state (`FUN_00601dd0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadState {
    /// Request queued, not yet submitted to streaming.
    Requested,
    /// Submitted; awaiting async completion (`FUN_00603110`).
    Loading,
    /// Resident.
    Loaded,
    /// Unload requested; awaiting release (`FUN_00602880`).
    Unloading,
}

/// A callback id the caller registers; fired when a batch completes (`LoadBankWithCallback`).
pub type CallbackId = u32;

/// One bank slot.
#[derive(Clone, Debug)]
pub struct BankSlot {
    pub name: String,
    pub kind: BankKind,
    pub state: LoadState,
    pub callback: Option<CallbackId>,
}

/// The bank manager: the 65-slot load state machine + an outstanding-request counter.
#[derive(Clone, Debug, Default)]
pub struct BankManager {
    slots: HashMap<String, BankSlot>,
    submitted: usize,
    /// Completed callbacks, drained by the caller (VM fires the Lua `funcBatchComplete`).
    fired: Vec<CallbackId>,
}

impl BankManager {
    /// Empty manager.
    pub fn new() -> BankManager {
        BankManager::default()
    }

    /// `Sound.LoadSoundBank` / `LoadWaveBank` / `LoadBankWithCallback`: request a bank load. Honours
    /// the 64-in-flight throttle and the 65-slot capacity; a rejected request returns `false`.
    pub fn load(&mut self, name: &str, kind: BankKind, callback: Option<CallbackId>) -> bool {
        if self.slots.len() >= BANK_SLOTS && !self.slots.contains_key(name) {
            return false; // slot table full
        }
        if self.submitted >= MAX_SUBMITTED {
            return false; // throttled — the Lua loader will retry next frame
        }
        // Re-loading an already-resident bank is a no-op success.
        if matches!(
            self.slots.get(name).map(|s| s.state),
            Some(LoadState::Loaded) | Some(LoadState::Loading)
        ) {
            return true;
        }
        self.submitted += 1;
        self.slots.insert(
            name.to_string(),
            BankSlot {
                name: name.to_string(),
                kind,
                state: LoadState::Requested,
                callback,
            },
        );
        true
    }

    /// Advance `Requested` → `Loading` for submitted banks (`UpdateLoads` submit pass).
    pub fn tick(&mut self) {
        for s in self.slots.values_mut() {
            if s.state == LoadState::Requested {
                s.state = LoadState::Loading;
            }
        }
    }

    /// Async completion (`FUN_00603110`): a `Loading` bank becomes `Loaded`; fires its callback.
    pub fn complete_load(&mut self, name: &str) {
        if let Some(s) = self.slots.get_mut(name) {
            if s.state == LoadState::Loading || s.state == LoadState::Requested {
                s.state = LoadState::Loaded;
                self.submitted = self.submitted.saturating_sub(1);
                if let Some(cb) = s.callback.take() {
                    self.fired.push(cb);
                }
            }
        }
    }

    /// `Sound.UnloadSoundBank`/`UnloadBankWithCallback`: request an unload (`FUN_00602880`).
    pub fn unload(&mut self, name: &str, callback: Option<CallbackId>) -> bool {
        if let Some(s) = self.slots.get_mut(name) {
            s.state = LoadState::Unloading;
            s.callback = callback;
            true
        } else {
            false
        }
    }

    /// Release completion for an `Unloading` bank — removes the slot, fires the callback.
    pub fn complete_unload(&mut self, name: &str) {
        if let Some(s) = self.slots.remove(name) {
            if let Some(cb) = s.callback {
                self.fired.push(cb);
            }
        }
    }

    /// Is a bank resident?
    pub fn is_loaded(&self, name: &str) -> bool {
        matches!(self.slots.get(name).map(|s| s.state), Some(LoadState::Loaded))
    }

    /// Outstanding (submitted, not yet complete) request count.
    pub fn outstanding(&self) -> usize {
        self.submitted
    }

    /// Number of occupied slots.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Drain the callbacks that fired since the last drain (the VM invokes their Lua closures).
    pub fn drain_callbacks(&mut self) -> Vec<CallbackId> {
        std::mem::take(&mut self.fired)
    }
}
