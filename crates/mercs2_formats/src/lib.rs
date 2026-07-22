//! Asset-format layer for the Mercenaries 2 modernization project: readers, writers and authoring
//! tools for the game's WAD/sges/UCFX container stack and everything inside it.
//!
//! No rendering, no simulation, no I/O policy — just format code, reverse-engineered from the
//! retail PC build and its console siblings. The one dependency is `flate2`.
//!
//! # Getting bytes out of a WAD
//!
//! The archive path is always the same three steps: read the FFCS tables, inflate the block an
//! asset lives in, then walk the UCFX containers inside it.
//!
//! ```no_run
//! use std::fs::File;
//! use mercs2_formats::{ffcs, hash, texture};
//!
//! let mut f = File::open("vz.wad")?;
//! let size = f.metadata()?.len();
//! let archive = ffcs::load_ffcs_archive(&mut f, size)?;   // INDX + ASET + PTHS + endian
//!
//! let name = hash::pandemic_hash_m2("pmc_hum_mattias_v2_dm");
//! let tex = texture::extract_texture(&mut f, &archive, name)?;   // raw DXT/BC, wgpu-ready
//! println!("{}x{} {:?}, {} mips", tex.width, tex.height, tex.format, tex.mip_count);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! For anything the `extract_*` helpers don't cover, do it by hand:
//! [`sges::decompress_block`] → [`ucfx::walk_decompressed_block`] →
//! [`ucfx::get_container_by_type_hash`] → the decoder for that asset type.
//!
//! # Module map
//!
//! **Archive / container**
//! - [`ffcs`] — FFCS WAD header + `INDX`/`ASET`/`PTHS` tables (LE on the PC bake, BE on console).
//! - [`sges`] — segmented-deflate block decompression/compression; whole-block and head-only reads.
//! - [`ucfx`] — UCFX descriptor-tree walk, chunk-body extraction, container `CSUM` verification.
//! - [`chunk_validate`], [`tags`], [`tag_registry`] — chunk-layout validators; the tag enum; the
//!   registry of all 232 FourCCs the engine dispatches on.
//! - [`types`], [`aset_type_ids`] — ASET `type_id` / UCFX `type_hash` constants and the map between.
//! - [`hash`], [`crc32`] — Pandemic FNV-1a name hashing; the `CSUM` CRC-32 (init 0, no final XOR).
//! - [`schema`], [`safe_slice`] — `COMP`/`schm` reflection records; bounds-checked byte buffer.
//!
//! **Assets**
//! - [`texture`], [`texsize`] — `MTRL` materials, per-group material binding, texture containers →
//!   raw DXT/BC (incl. hi-res mip assembly); the shared mip-chain sizing rule.
//! - [`skeleton`], [`havok`], [`anim`], [`animgroup`], [`anim_select`] — `HIER` rest pose; the
//!   little-endian Havok 5.5 packfile reader (`PHY2` collision); `hkaAnimation` clip decode; the
//!   `animation` block; the engine's data-driven clip picker.
//! - [`terrain`], [`placement`], [`world_index`], [`world`] — low-res terrain; `layers_static`
//!   placements; the Layer-1 world block index that feeds streaming; world spatial constants.
//! - [`orchestrator`], [`fxdict`], [`atmosphere`], [`gfx`] — destruction state machines; FX
//!   dictionaries; the `Graphics.Atmosphere.*` sky/HDR parameter model; Scaleform GFx/SWF.
//! - [`save`], [`save_write`] — the PC `.profile` save (13,404 bytes, zlib Lua payload at `0x468`).
//!
//! **Authoring / write side**
//! - [`model_build`] — author a static model container from scratch (no donor).
//! - [`model_inject`], [`model_cubeize`] — conform novel geometry into a *real* donor container.
//!   The engine rejects a hand-built container's decl/material/shader bindings (`0x004CC064`), so
//!   the donor path is the one that ships.
//! - [`mannequin`], [`retarget`] — procedural humanoid mesh; foreign-rig retargeting.
//! - [`placement_build`], [`patch_wad`] — append a new world placement; serialize a `vz-patch.wad`.
//! - [`dlc_input`], [`dlc_stfs`] — big-endian Xbox 360 DLC readers; STFS container + RAR extraction.
//!
//! # Gotchas
//!
//! - **No coordinate conversion is applied.** [`anim`] returns Havok values verbatim (right-handed,
//!   +Y up, metres). Game space is left-handed, +Y up — the RH→LH conversion belongs to the
//!   integrator, not this crate.
//! - **One Havok packfile walker.** [`anim`] reuses [`havok::parse_packfile_raw`] for the
//!   section/classname/fixup pass and only adds the animation classes. Do not re-implement it.
//! - See `README.md`, `SAVE_FORMAT.md`, and the deferred-work list in `DEFERRED.md`.
//!
//! Binaries in `src/bin/` (probes, injectors, forges) and the examples in `examples/` drive these
//! modules from the command line; `tests/wavelet_*.rs` gate the animation decoder against a live
//! x32dbg capture.

pub mod anim;
pub mod anim_select;
pub mod animgroup;
pub mod atmosphere;
pub mod aset_type_ids;
pub mod char_skin;
pub mod chunk_validate;
pub mod crc32;
pub mod dlc_input;
pub mod dlc_stfs;
pub mod ffcs;
pub mod fxdict;
pub mod gfx;
pub mod hash;
pub mod havok;
pub mod model_build;
pub mod model_cubeize;
pub mod model_inject;
pub mod orchestrator;
pub mod patch_wad;
pub mod placement;
pub mod placement_build;
pub mod safe_slice;
pub mod save;
pub mod save_write;
pub mod schema;
pub mod scripts_block;
pub mod sges;
pub mod skeleton;
pub mod stringdb;
pub mod tag_registry;
pub mod tags;
pub mod terrain;
pub mod texsize;
pub mod texture;
pub mod texture_encode;
pub mod types;
pub mod ucfx;
pub mod world;
pub mod world_index;
