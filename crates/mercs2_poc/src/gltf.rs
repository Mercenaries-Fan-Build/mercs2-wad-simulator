//! Adapter onto the library's skinned-glTF reader.
//!
//! This was a 436-line hand-rolled `serde_json` GLB parser — a **second** implementation of what
//! `mercs2_workshop::import` already did, because both crates are binaries and neither could reach
//! the other. Predictably they drifted: only the Workshop's grew the rigid bone-parented-part pass,
//! so every one of these CLIs silently dropped a character's eyes, teeth and equipment packs.
//!
//! It also read GLB only, and only the accessor subset Blender happens to emit.
//!
//! Both copies are gone. `mercs2_formats::char_import` is the one reader, and the twelve
//! `src/bin/*` tools keep their `#[path = "../gltf.rs"] mod gltf;` line unchanged.

pub use mercs2_formats::char_skin::CharGlbData;

/// `&str` front door, matching what the `src/bin` tools pass straight from `std::env::args`.
pub fn load_char_glb(path: &str) -> Result<CharGlbData, String> {
    mercs2_formats::char_import::load_char_glb(std::path::Path::new(path))
}
