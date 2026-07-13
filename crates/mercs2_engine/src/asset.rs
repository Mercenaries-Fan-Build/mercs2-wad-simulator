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

/// A base WAD plus an ordered stack of overlay/patch WADs. `wads[0]` is the base; `wads[1..]` are
/// overlays in load order. Resolution walks the stack in reverse (last wins).
pub struct AssetSource {
    wads: Vec<Wad>,
    labels: Vec<String>,
    /// The base WAD path — used to resolve sibling archives (e.g. `shell.wad`) without a second ad-hoc
    /// open scattered through the game.
    base_path: String,
    /// Block residency + the global hash-keyed chunk registry — the retail asset layer. See
    /// `registry.rs`: the WAD stack above is last-wins *file* resolution; registry insert is
    /// first-wins. Both rules are live at once, exactly as retail composes them.
    registry: AssetRegistry,
}

impl AssetSource {
    /// Open `base` plus each overlay in load order. An overlay that fails to open is logged and skipped
    /// (a missing patch must not brick the game). Fails only if the base itself won't open.
    pub fn open(base: &str, overlays: &[String]) -> Result<AssetSource, String> {
        let mut wads = vec![wad::open(base)?];
        let mut labels = vec![base.to_string()];
        for o in overlays {
            match wad::open(o) {
                Ok(w) => {
                    println!("[asset] overlay: {o}");
                    wads.push(w);
                    labels.push(o.clone());
                }
                Err(e) => println!("[asset] overlay {o}: {e} (skipped)"),
            }
        }
        Ok(AssetSource {
            wads,
            labels,
            base_path: base.to_string(),
            registry: AssetRegistry::default(),
        })
    }

    /// Open `base` and auto-include the sibling `vz-patch.wad` overlay if it exists next to it — the
    /// game's standard patch drop. Any additional overlays are appended after the auto-discovered one.
    pub fn discover(base: &str, extra_overlays: &[String]) -> Result<AssetSource, String> {
        let mut overlays = Vec::new();
        let sibling = patch_sibling(base);
        if sibling.exists() {
            overlays.push(sibling.to_string_lossy().into_owned());
        }
        overlays.extend_from_slice(extra_overlays);
        AssetSource::open(base, &overlays)
    }

    /// The base WAD path (for sibling-archive resolution).
    pub fn base_path(&self) -> &str {
        &self.base_path
    }

    /// The base archive, read-only.
    pub fn base(&self) -> &Wad {
        &self.wads[0]
    }

    /// The base archive, mutable — for base-only loader code (terrain, world index) that predates the
    /// stack and reads only `vz.wad`. Overlay-sensitive asset lookups must go through the `extract_*`
    /// resolvers instead so patches win.
    pub fn base_mut(&mut self) -> &mut Wad {
        &mut self.wads[0]
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
    std::path::Path::new(base).with_file_name("vz-patch.wad")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a stack shell with only labels populated — enough to exercise the pure provenance/ordering
    /// logic without opening real WADs (which the ignored integration probes cover).
    fn labeled(labels: &[&str]) -> AssetSource {
        AssetSource {
            wads: Vec::new(),
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
}
