//! Sound categories: per-category volume/pitch, timed fades, and ref-counted master ducking.
//!
//! **Oracle (audio_code_map.md §5, luacd 08_audio_presentation §3):**
//! * `Sound.SetCategoryVolume/Pitch` shims `FUN_005e12f0`/`FUN_005e1390` → impl `FUN_00607960`: a
//!   **double-buffered pending list, max 10 applied per frame**.
//! * `Sound.FadeCategoryDown/Up` — timed category fades with enter/exit lengths
//!   (`MrxSoundCategories.SetFadeCategory(mode, cat, level, enter, exit)`).
//! * `Sound.SetMasterVolume` shim `FUN_005e4240` → `FUN_0082f590` → engine vcall `+0x4C`
//!   (`StartMasterFade`). Ducking (`DuckMasterVolume`/`UnduckMasterVolume`) is **ref-counted** in Lua
//!   (`MrxSoundCategories`).
//!
//! The named categories the game uses (`sfx`, `vo`, `music`, `chatter`, `non_ui`, …) are declared in
//! `MrxSoundBootstrap`. Here a category is keyed by its m2 name-hash so the surface is data-driven —
//! no hard-coded enum.

use std::collections::HashMap;

use mercs2_formats::hash::pandemic_hash_m2;

/// Max category param changes applied per frame (`FUN_00607960` double-buffered pending list).
pub const MAX_CATEGORY_APPLIES_PER_FRAME: usize = 10;

/// A timed linear fade of a scalar (volume or pitch) toward a target.
#[derive(Clone, Copy, Debug)]
pub struct Fade {
    pub current: f32,
    pub target: f32,
    /// Seconds to traverse the full 0→1 range (rate is derived from this).
    pub length: f32,
}

impl Fade {
    fn stable(v: f32) -> Fade {
        Fade {
            current: v,
            target: v,
            length: 0.0,
        }
    }

    /// Retarget over `length` seconds (0 = snap).
    pub fn to(&mut self, target: f32, length: f32) {
        self.target = target;
        self.length = length.max(0.0);
        if self.length == 0.0 {
            self.current = target;
        }
    }

    /// Advance the fade by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        if self.current == self.target {
            return;
        }
        if self.length <= 0.0 {
            self.current = self.target;
            return;
        }
        let step = dt / self.length; // fraction of the full range per tick
        if self.current < self.target {
            self.current = (self.current + step).min(self.target);
        } else {
            self.current = (self.current - step).max(self.target);
        }
    }
}

/// One category's live state.
#[derive(Clone, Copy, Debug)]
pub struct Category {
    pub volume: Fade,
    pub pitch: Fade,
}

impl Default for Category {
    fn default() -> Self {
        Category {
            volume: Fade::stable(1.0),
            pitch: Fade::stable(1.0),
        }
    }
}

/// A queued category change, applied ≤10/frame (`FUN_00607960`).
#[derive(Clone, Copy, Debug)]
enum PendingKind {
    Volume,
    Pitch,
}
#[derive(Clone, Copy, Debug)]
struct Pending {
    cat: u32,
    kind: PendingKind,
    target: f32,
    length: f32,
}

/// The category mixer state: master volume + duck ref-count + per-category fades.
#[derive(Clone, Debug)]
pub struct Categories {
    master: Fade,
    /// Ref-counted master duck (`DuckMasterVolume`/`UnduckMasterVolume`). While `> 0`, master is
    /// pulled toward `duck_level`.
    duck_refs: u32,
    duck_level: f32,
    cats: HashMap<u32, Category>,
    pending: Vec<Pending>,
}

impl Default for Categories {
    fn default() -> Self {
        Categories {
            master: Fade::stable(1.0),
            duck_refs: 0,
            duck_level: 0.0,
            cats: HashMap::new(),
            pending: Vec::new(),
        }
    }
}

impl Categories {
    /// Look up (creating on first use) a category by name-hash.
    fn entry(&mut self, cat: u32) -> &mut Category {
        self.cats.entry(cat).or_default()
    }

    /// `Sound.SetCategoryVolume` — queue a category volume change (applied ≤10/frame on [`tick`]).
    pub fn set_category_volume(&mut self, cat: u32, volume: f32, length: f32) {
        self.pending.push(Pending {
            cat,
            kind: PendingKind::Volume,
            target: volume,
            length,
        });
    }

    /// `Sound.SetCategoryPitch` — queue a category pitch change.
    pub fn set_category_pitch(&mut self, cat: u32, pitch: f32, length: f32) {
        self.pending.push(Pending {
            cat,
            kind: PendingKind::Pitch,
            target: pitch,
            length,
        });
    }

    /// `Sound.FadeCategoryDown` — fade a category's volume down to `level` over `length`.
    pub fn fade_category_down(&mut self, cat: u32, level: f32, length: f32) {
        self.entry(cat).volume.to(level, length);
    }

    /// `Sound.FadeCategoryUp` — restore a category's volume to `level` (usually 1.0) over `length`.
    pub fn fade_category_up(&mut self, cat: u32, level: f32, length: f32) {
        self.entry(cat).volume.to(level, length);
    }

    /// Current (post-fade) linear volume of a category (1.0 if never set).
    pub fn category_volume(&self, cat: u32) -> f32 {
        self.cats.get(&cat).map(|c| c.volume.current).unwrap_or(1.0)
    }

    /// Current pitch multiplier of a category.
    pub fn category_pitch(&self, cat: u32) -> f32 {
        self.cats.get(&cat).map(|c| c.pitch.current).unwrap_or(1.0)
    }

    /// `Sound.SetMasterVolume` → `StartMasterFade` (engine vcall `+0x4C`): fade master to `volume`.
    pub fn set_master_volume(&mut self, volume: f32, length: f32) {
        self.master.to(volume, length);
    }

    /// Current master volume (with any active duck folded in).
    pub fn master_volume(&self) -> f32 {
        self.master.current
    }

    /// `DuckMasterVolume(length)` — push a ref-counted master duck toward `duck_level` (default 0).
    pub fn duck_master(&mut self, level: f32, length: f32) {
        self.duck_refs += 1;
        self.duck_level = level;
        if self.duck_refs == 1 {
            self.master.to(level, length);
        }
    }

    /// `UnduckMasterVolume(length)` — pop a duck ref; restore to 1.0 when the last is released.
    pub fn unduck_master(&mut self, length: f32) {
        if self.duck_refs > 0 {
            self.duck_refs -= 1;
        }
        if self.duck_refs == 0 {
            self.master.to(1.0, length);
        }
    }

    /// Advance all fades and drain up to [`MAX_CATEGORY_APPLIES_PER_FRAME`] pending param changes.
    pub fn tick(&mut self, dt: f32) {
        // Apply ≤10 pending changes this frame; the rest wait (double-buffer, FUN_00607960).
        let n = self.pending.len().min(MAX_CATEGORY_APPLIES_PER_FRAME);
        for p in self.pending.drain(..n).collect::<Vec<_>>() {
            let c = self.entry(p.cat);
            match p.kind {
                PendingKind::Volume => c.volume.to(p.target, p.length),
                PendingKind::Pitch => c.pitch.to(p.target, p.length),
            }
        }
        self.master.tick(dt);
        for c in self.cats.values_mut() {
            c.volume.tick(dt);
            c.pitch.tick(dt);
        }
    }

    /// Effective linear gain for a voice in category `cat`: `master * category_volume`.
    pub fn effective_gain(&self, cat: u32) -> f32 {
        self.master_volume() * self.category_volume(cat)
    }
}

/// Hash a category name to its id (convenience for callers using names rather than pre-hashed ids).
pub fn category_id(name: &str) -> u32 {
    pandemic_hash_m2(name)
}
