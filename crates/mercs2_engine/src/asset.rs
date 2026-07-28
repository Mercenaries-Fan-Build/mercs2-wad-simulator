//! `AssetSource` — the engine's cohesive WAD asset layer: a base archive plus an ordered stack of
//! patch/overlay WADs, resolved last-writer-wins.
//!
//! This is the game's own patch mechanism (`data/vz-patch.wad`, the online-restore + DLC-port overlay)
//! made first-class: open `vz.wad`, then any overlays *on top*, and every resolver walks the stack in
//! REVERSE (last overlay first, base last) so a later archive's asset shadows an earlier one's — exactly
//! the retail "last-opened wins" rule. Promoted from the workshop's private `WadStack` so the game and
//! the workshop share one implementation instead of each opening `vz.wad` ad hoc.
//!
//! NOTE — two distinct "overlay" vocabularies, do not conflate: THIS overlay = patch-WAD *file* stacking
//! (whole archives layered on top). The `overlays` argument to `game_world::load_streaming_world_data`
//! and `worldutil::add_overlay_to_catalog` is a DIFFERENT thing — `vz_state` layer *blocks inside one
//! wad* folded into the streaming catalog. `AssetSource` is the file-stacking one.

use crate::registry::{AssetRegistry, RegistryStats};
use crate::wad::{self, Wad};
use mercs2_formats::texture::TextureData;

/// One canonical mount slot. The retail WAD manager (`FUN_004BE0A0`, `0x0149FDA0`) constructs
/// **seven** reader objects and mounts them in this fixed order; the mount state machine
/// `FUN_004BFAF0` dispatches one slot per state pair, and teardown closes them in exact reverse.
/// Because the readers always open as a group and claim array slots in order,
/// **slot index == mount order == resolution rank**
/// (`docs/fixpack/wad_duplicate_inventory.md` §B.1–B.3, evidence: `proven`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MountSlot {
    /// 0 — `Loading.wad`. Boot archive: the minimum needed to draw a loading screen.
    Loading,
    /// 1 — `loading-patch.wad`. ⚠ Ranks BELOW the level and language WADs despite its name, so they
    /// can override it (§B.3).
    LoadingPatch,
    /// 2 — `<level>.wad`, the basename from `[0x014A259C]`: **`shell.wad` OR `vz.wad`, never both**.
    /// One slot, one reader; the front-end path writes `"shell"` and the level path writes the level
    /// name, with a close-all → reopen-all cycle between them (§B.5). This is why 106 of the 112
    /// cross-WAD duplicates are the shell↔vz front-end kit: it is baked twice precisely because the
    /// two archives are never co-resident.
    Level,
    /// 3 — `<level>-patch.wad`. Where `vz-patch.wad` lands: immediately above its own base.
    LevelPatch,
    /// 4 — the config-gated extra WAD (full path from `[0x014A25DC]`). Absent in retail.
    Extra,
    /// 5 — the language WAD, opened as `%s\%s.wad` with the lowercase name `english`. Outranks the
    /// level WAD, which is exactly how the six localized main-menu textures shadow their base copies.
    Language,
    /// 6 — `english-patch.wad`. Top of the stack; beats everything.
    LanguagePatch,
}

/// The canonical mount order — index in this array IS the retail slot number (§B.2).
pub const MOUNT_ORDER: [MountSlot; 7] = [
    MountSlot::Loading,
    MountSlot::LoadingPatch,
    MountSlot::Level,
    MountSlot::LevelPatch,
    MountSlot::Extra,
    MountSlot::Language,
    MountSlot::LanguagePatch,
];

/// The engine's mounted WAD stack, in retail mount order. Resolution walks it **in reverse**
/// (highest mounted index first), so the last-mounted archive that owns a key wins — the
/// `RedVirtualDisk` reverse search read out of `FUN_00875E80`/`FUN_00876150` (§B.3, `proven`).
///
/// Absent archives are simply not mounted; the stack stays dense and ordered, matching the retail
/// slot-claim loop, which compacts into the first free array entry.
pub struct AssetSource {
    /// Mounted archives in MOUNT ORDER. Resolution walks this in reverse.
    wads: Vec<Wad>,
    labels: Vec<String>,
    /// Which canonical slot each mounted archive occupies. Parallel to `wads`.
    slots: Vec<MountSlot>,
    /// Index of [`MountSlot::Level`] within `wads` — what [`base`](Self::base) returns.
    ///
    /// ⚠ NOT 0. `Loading.wad` mounts below the level WAD, so the level is no longer the first
    /// element. Every `base()`/`base_mut()` caller means "the level archive" (terrain, world index,
    /// placements), and repointing them at `Loading.wad` would silently load an 8-block boot archive
    /// instead of the 11,370-block world.
    level: usize,
    /// The level WAD path — used to resolve sibling archives without a second ad-hoc open.
    base_path: String,
    /// Block residency + the global hash-keyed chunk registry — the retail asset layer. See
    /// `registry.rs`: the WAD stack above is last-wins *file* resolution; registry insert is
    /// first-wins. Both rules are live at once, exactly as retail composes them.
    registry: AssetRegistry,
}

impl AssetSource {
    /// Mount the level WAD plus each extra overlay, in that order only — the pre-stack shape, kept for
    /// callers that genuinely want just one archive (tools, fixtures). **The game boot should use
    /// [`discover`](Self::discover)**, which mounts the full retail seven-slot stack.
    ///
    /// An overlay that fails to open is logged and skipped (a missing patch must not brick the game).
    /// Fails only if the level WAD itself won't open.
    pub fn open(base: &str, overlays: &[String]) -> Result<AssetSource, String> {
        let mut wads = vec![wad::open(base)?];
        let mut labels = vec![base.to_string()];
        let mut slots = vec![MountSlot::Level];
        for o in overlays {
            match wad::open(o) {
                Ok(w) => {
                    println!("[asset] overlay: {o}");
                    wads.push(w);
                    labels.push(o.clone());
                    slots.push(MountSlot::LevelPatch);
                }
                Err(e) => println!("[asset] overlay {o}: {e} (skipped)"),
            }
        }
        Ok(AssetSource {
            wads,
            labels,
            slots,
            level: 0,
            base_path: base.to_string(),
            registry: AssetRegistry::default(),
        })
    }

    /// Mount the **full retail WAD stack** around the level archive at `base`, in the canonical order
    /// of [`MOUNT_ORDER`] (`wad_duplicate_inventory.md` §B.2, `proven`):
    ///
    /// ```text
    /// 0  Loading.wad          3  <level>-patch.wad     6  english-patch.wad
    /// 1  loading-patch.wad    4  <extra>.wad
    /// 2  <level>.wad          5  english.wad
    /// ```
    ///
    /// Every sibling is resolved next to `base`, which is how retail builds them too — one directory,
    /// `%s\<name>.wad`. Missing archives are skipped, not fatal: retail soft-fails each open, and a
    /// stack of one still boots. `extra_overlays` mount in the `Extra` slot (4), which is where retail's
    /// config-gated archive goes — above the level patch, below the language WAD.
    ///
    /// Only the level WAD is required. Note the resulting rank order means **`english.wad` beats
    /// `vz.wad`** — the reason the localized main-menu art is visible at all — and that
    /// `loading-patch.wad` is *below* both, despite its name.
    pub fn discover(base: &str, extra_overlays: &[String]) -> Result<AssetSource, String> {
        let mut wads = Vec::new();
        let mut labels = Vec::new();
        let mut slots = Vec::new();
        let mut level = 0usize;

        // Each entry: (slot, resolved path, required). Order here IS the mount order.
        let mut plan: Vec<(MountSlot, String)> = Vec::new();
        let mut push = |plan: &mut Vec<_>, slot, name: &str| {
            if let Some(p) = sibling_ci(base, name) {
                plan.push((slot, p));
            }
        };
        push(&mut plan, MountSlot::Loading, "Loading.wad");
        push(&mut plan, MountSlot::LoadingPatch, "loading-patch.wad");
        plan.push((MountSlot::Level, base.to_string()));
        push(&mut plan, MountSlot::LevelPatch, &patch_name_for(base));
        for e in extra_overlays {
            plan.push((MountSlot::Extra, e.clone()));
        }
        // The language slot is opened as the LOWERCASE `english`; the shipped file is `English.wad`.
        // Retail relies on Windows case-insensitivity here, which is a live portability trap on a
        // case-sensitive filesystem (Proton/Wine) — `sibling_ci` resolves it either way.
        push(&mut plan, MountSlot::Language, "english.wad");
        push(&mut plan, MountSlot::LanguagePatch, "english-patch.wad");

        for (slot, path) in plan {
            let required = slot == MountSlot::Level;
            match wad::open(&path) {
                Ok(w) => {
                    if slot == MountSlot::Level {
                        level = wads.len();
                    }
                    println!("[asset] mount {}: {slot:?} <- {path}", wads.len());
                    wads.push(w);
                    labels.push(path);
                    slots.push(slot);
                }
                Err(e) if required => return Err(e),
                Err(e) => println!("[asset] {slot:?} {path}: {e} (skipped)"),
            }
        }
        println!(
            "[asset] stack: {} archive(s), resolution is last-wins (top = {:?})",
            wads.len(),
            slots.last()
        );
        Ok(AssetSource { wads, labels, slots, level, base_path: base.to_string(), registry: AssetRegistry::default() })
    }

    /// The canonical slot each mounted archive occupies, in mount order.
    pub fn slots(&self) -> &[MountSlot] {
        &self.slots
    }

    /// Index of the archive mounted in `slot`, if any.
    pub fn index_of(&self, slot: MountSlot) -> Option<usize> {
        self.slots.iter().position(|s| *s == slot)
    }

    /// The base WAD path (for sibling-archive resolution).
    pub fn base_path(&self) -> &str {
        &self.base_path
    }

    /// The **level** archive (`vz.wad`), read-only. This is slot 2, not index 0 — `Loading.wad` mounts
    /// beneath it.
    pub fn base(&self) -> &Wad {
        &self.wads[self.level]
    }

    /// The level archive, mutable — for level-only loader code (terrain, world index) that predates the
    /// stack and reads only `vz.wad`. Overlay-sensitive asset lookups must go through the `extract_*`
    /// resolvers instead so higher-ranked archives win.
    pub fn base_mut(&mut self) -> &mut Wad {
        let i = self.level;
        &mut self.wads[i]
    }

    /// Number of archives in the stack (base + overlays).
    pub fn len(&self) -> usize {
        self.wads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.wads.is_empty()
    }

    /// Short provenance tag for source index `src` (base = "", overlays = "+<file stem>").
    pub fn tag(&self, src: usize) -> String {
        if src == 0 || src >= self.labels.len() {
            return String::new();
        }
        let stem = std::path::Path::new(&self.labels[src])
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("overlay");
        format!("+{stem}")
    }

    /// Resolve a chunk through the residency registry: `(type_hash, name_hash)` → bytes, streaming the
    /// owning block in on demand and registering *every* chunk it carries. Returns `None` when the hash
    /// is in no open archive.
    ///
    /// This is the seam the retail engine actually has. Resolving one asset makes its whole block
    /// resident, so its block-mates are registered too and later lookups for them are free — which is
    /// how a model in block 3350 binds textures that live in blocks 2976/2977.
    pub fn resolve(&mut self, type_hash: u32, name_hash: u32) -> Option<Vec<u8>> {
        let AssetSource { wads, registry, .. } = self;
        let c = registry.resolve(wads, type_hash, name_hash)?;
        registry.slice(c).map(<[u8]>::to_vec)
    }

    /// Residency + registry counters (resident blocks, registered chunks, first-wins shadowed, evicted).
    pub fn registry_stats(&self) -> RegistryStats {
        self.registry.stats()
    }

    /// Model container by hash.
    pub fn extract_container(&mut self, hash: u32) -> Result<Vec<u8>, String> {
        self.resolve(wad::MODEL_TYPE_HASH, hash)
            .ok_or_else(|| format!("0x{hash:08X}: no model chunk in any open wad"))
    }

    /// A typed CHDR-class container (terrainmesh / watermap / wavebank / sounddb) by hash.
    pub fn extract_container_typed(&mut self, hash: u32, chunk_type: u32) -> Result<Vec<u8>, String> {
        self.resolve(chunk_type, hash)
            .ok_or_else(|| format!("0x{hash:08X}: no 0x{chunk_type:08X} chunk in any open wad"))
    }

    /// The container of a **singleton** asset class, found by `chunk_type` alone — `watermap`
    /// (`0x4D7D30C4`), `materialtable`, and the other ASET `type_id 0` classes.
    ///
    /// These have no addressable name: the ASET row's `asset_hash` names the resident group that holds
    /// the chunk, and the chunk's own `name_hash` is an unrelated authored hash, so
    /// [`extract_container_typed`](Self::extract_container_typed) can never find one. Returns the
    /// chunk's `name_hash` alongside its bytes for logging/diagnostics.
    pub fn extract_singleton(&mut self, chunk_type: u32) -> Option<(u32, Vec<u8>)> {
        let AssetSource { wads, registry, .. } = self;
        let (name_hash, c) = registry.resolve_singleton(wads, chunk_type)?;
        registry.slice(c).map(|b| (name_hash, b.to_vec()))
    }

    /// Resident-mip texture (fast path — model loads) by hash — last wins.
    ///
    /// NOT routed through the registry: a texture's mip chain is spread across the finer-LOD blocks of
    /// its own c3 cell subtree and must be *assembled*, not picked (see `wad::extract_texture_hires`).
    /// The registry's one-cell-per-hash rule is the right model for that — retail's pool holds a single
    /// cell per texture and mips accumulate into it — but assembling into a registry cell is the next
    /// step, not this one.
    pub fn extract_texture(&mut self, hash: u32) -> Result<TextureData, String> {
        let mut last = format!("0x{hash:08X}: not in any open wad");
        for i in (0..self.wads.len()).rev() {
            match wad::extract_texture(&mut self.wads[i], hash) {
                Ok(t) => return Ok(t),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Full streamed (hi-res assembled) texture when available, resident otherwise — last wins.
    pub fn extract_texture_hires(&mut self, hash: u32) -> Result<TextureData, String> {
        let mut last = format!("0x{hash:08X}: not in any open wad");
        for i in (0..self.wads.len()).rev() {
            let w = &mut self.wads[i];
            match wad::extract_texture_hires(w, hash).or_else(|_| wad::extract_texture(w, hash)) {
                Ok(t) => return Ok(t),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// The real loading-screen plate from the sibling `shell.wad` (next to the base `vz.wad`). Folds the
    /// one-off `shell.wad` open into the asset layer instead of a scattered ad-hoc call.
    pub fn loading_plate(&self) -> Result<TextureData, String> {
        wad::shell_loading_plate(&self.base_path)
    }
}

/// The standard patch-WAD path for a base: `vz-patch.wad` alongside `vz.wad`. Kept separate so the
/// discovery contract is unit-testable without a real archive on disk.
fn patch_sibling(base: &str) -> std::path::PathBuf {
    std::path::Path::new(base).with_file_name(patch_name_for(base))
}

/// `<level>-patch.wad` for a level WAD path — retail builds it from the same basename that opened the
/// level (`%s\%s-patch.wad`, name from `[0x014A259C]`), so a `shell.wad` mount looks for
/// `shell-patch.wad`, not `vz-patch.wad`.
fn patch_name_for(base: &str) -> String {
    let stem = std::path::Path::new(base)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vz");
    format!("{stem}-patch.wad")
}

/// Resolve `name` as a sibling of `base`, **case-insensitively**, returning `None` if absent.
///
/// Retail opens the language slot as lowercase `english.wad` and the boot slot as `loading.wad`, while
/// the shipped files are `English.wad` and `Loading.wad`. On Windows that works by accident of
/// case-insensitive filesystem matching; on a case-sensitive one (Proton/Wine with a case-sensitive
/// prefix, or Linux) the exact-name open silently fails and the archive is never mounted — losing the
/// localized art with no error. Matching on a lowercased comparison reproduces the Windows behaviour
/// everywhere. Called out explicitly in `wad_duplicate_inventory.md` §B.2.
fn sibling_ci(base: &str, name: &str) -> Option<String> {
    let dir = std::path::Path::new(base).parent()?;
    let exact = dir.join(name);
    if exact.exists() {
        return Some(exact.to_string_lossy().into_owned());
    }
    let want = name.to_ascii_lowercase();
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let p = e.path();
        let f = p.file_name()?.to_str()?;
        (f.to_ascii_lowercase() == want).then(|| p.to_string_lossy().into_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a stack shell with only labels populated — enough to exercise the pure provenance/ordering
    /// logic without opening real WADs (which the ignored integration probes cover).
    fn labeled(labels: &[&str]) -> AssetSource {
        AssetSource {
            wads: Vec::new(),
            slots: vec![MountSlot::Level; labels.len()],
            level: 0,
            labels: labels.iter().map(|s| s.to_string()).collect(),
            base_path: labels.first().copied().unwrap_or_default().to_string(),
            registry: AssetRegistry::default(),
        }
    }

    #[test]
    fn base_has_no_tag_overlays_are_stemmed() {
        let a = labeled(&["data/vz.wad", "data/vz-patch.wad", "mods/foo.wad"]);
        assert_eq!(a.tag(0), ""); // base carries no provenance marker
        assert_eq!(a.tag(1), "+vz-patch"); // overlay tagged by file stem
        assert_eq!(a.tag(2), "+foo");
        assert_eq!(a.tag(99), ""); // out of range is inert, never panics
    }

    #[test]
    fn discover_looks_for_vz_patch_next_to_the_base() {
        // The patch drop is resolved as a sibling of the base wad, whatever the base directory is.
        assert!(patch_sibling("C:/game/data/vz.wad").ends_with("vz-patch.wad"));
        assert_eq!(
            patch_sibling("C:/game/data/vz.wad").parent(),
            std::path::Path::new("C:/game/data/vz.wad").parent()
        );
    }

    /// The patch name follows the basename that opened the slot, so a `shell.wad` mount looks for
    /// `shell-patch.wad` — retail builds both from the same `[0x014A259C]` string.
    #[test]
    fn patch_name_follows_the_level_basename() {
        assert_eq!(patch_name_for("data/vz.wad"), "vz-patch.wad");
        assert_eq!(patch_name_for("data/shell.wad"), "shell-patch.wad");
    }

    /// `MOUNT_ORDER` is the retail slot order, and rank is positional: anything later in the array
    /// outranks anything earlier, because resolution walks the mounted stack backwards.
    #[test]
    fn mount_order_matches_the_retail_slots() {
        assert_eq!(MOUNT_ORDER.len(), 7, "the WAD manager constructs exactly seven readers");
        assert_eq!(MOUNT_ORDER[0], MountSlot::Loading);
        assert_eq!(MOUNT_ORDER[2], MountSlot::Level);
        assert_eq!(MOUNT_ORDER[5], MountSlot::Language);
        let rank = |s: MountSlot| MOUNT_ORDER.iter().position(|x| *x == s).unwrap();
        // The two consequences §B.3 calls out explicitly, as assertions.
        assert!(rank(MountSlot::Language) > rank(MountSlot::Level), "English.wad beats the level WAD");
        assert!(
            rank(MountSlot::LoadingPatch) < rank(MountSlot::Level),
            "loading-patch.wad ranks BELOW the level WAD, despite its name"
        );
        assert!(rank(MountSlot::LevelPatch) == rank(MountSlot::Level) + 1, "a patch sits directly above its base");
    }

    /// Case-insensitive sibling resolution: retail opens lowercase `english.wad`/`loading.wad` while the
    /// shipped files are `English.wad`/`Loading.wad`. Without this, a case-sensitive filesystem mounts
    /// neither and silently loses the localized art.
    #[test]
    fn sibling_resolves_regardless_of_case() {
        let dir = std::env::temp_dir().join(format!("mercs2_asset_ci_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("vz.wad");
        std::fs::write(&base, b"x").unwrap();
        std::fs::write(dir.join("English.wad"), b"x").unwrap();
        let base = base.to_string_lossy().into_owned();

        // Asked for lowercase, shipped as capitalized — must still resolve.
        assert!(sibling_ci(&base, "english.wad").is_some(), "english.wad must find English.wad");
        assert!(sibling_ci(&base, "English.wad").is_some(), "exact match still works");
        assert!(sibling_ci(&base, "nope.wad").is_none(), "a genuinely absent archive stays absent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Integration: mount the real shipped stack and assert the documented order + that `base()` still
    /// means the LEVEL archive rather than `Loading.wad`, which now sits beneath it.
    ///
    /// ```text
    /// cargo test -p mercs2_engine --lib real_wad_stack -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn real_wad_stack_mounts_in_the_documented_order() {
        let Some(path) = wad::resolve_vz_wad(None) else {
            return eprintln!("[skip] vz.wad not resolvable");
        };
        let src = AssetSource::discover(&path, &[]).expect("mount the stack");
        println!("[stack] {:?}", src.slots());

        // Mounted slots must be a subsequence of the canonical order — never reordered.
        let ranks: Vec<usize> =
            src.slots().iter().map(|s| MOUNT_ORDER.iter().position(|x| x == s).unwrap()).collect();
        assert!(ranks.windows(2).all(|w| w[0] < w[1]), "mount order must be strictly ascending: {ranks:?}");

        // The level archive is the world, not the 8-block boot archive.
        let level = src.index_of(MountSlot::Level).expect("the level WAD is required");
        assert_eq!(level, src.level, "base() must follow the Level slot");
        assert!(
            wad::block_paths(src.base()).len() > 1000,
            "base() must be the level archive (11,370 blocks), not Loading.wad (8)"
        );

        // English.wad ships next to vz.wad in a retail install; if it mounted, it must outrank the level.
        if let Some(lang) = src.index_of(MountSlot::Language) {
            assert!(lang > level, "English.wad must outrank the level WAD (§B.3)");
        }
    }
}
