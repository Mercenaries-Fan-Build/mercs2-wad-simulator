//! Xbox 360 BE → PC LE UCFX block converter, exposed as a library so the
//! `dlc_port` driver can call `convert::convert_block` directly (no subprocess).
//! The `ucfx_byteswap` binary (`main.rs`) is a thin CLI over these modules.
//!
//! Input is one **already-decompressed** UCFX block. The conversion is not a
//! blind u32 sweep: the entry table and descriptor tables are re-emitted LE,
//! ECS component bodies (Layer / WorldEntityData / GuidMap) are swapped at their
//! schema-declared field widths, and several embedded payloads are re-encoded
//! rather than swapped — Havok packfiles (section-aware; a flat swap scrambles
//! the ASCII `__classnames__` and the loader then fails to resolve the class),
//! GPU-tiled DXT textures (untile + rebuilt PC `INFO`), wavebanks (Xbox-ADPCM /
//! XMA → PC IMA), Lua `BINN` bytecode (unluac disassemble → flip endianness →
//! reassemble), mesh vertex declarations (Xbox 12-byte elements → 8-byte
//! `D3DVERTEXELEMENT9`), and terrainmesh (vertex widening + index de-strip).
//! Bodies that resize force a container reframe; sizes and `CSUM` trailers are
//! recomputed on the way out.
//!
//! Anything that could not be converted with a typed swap — unknown schema type
//! codes, `schm` parse failures, catch-all u32 fallbacks, registered-but-
//! unvalidated tags — is surfaced through [`report::SchemaCoverageReport`]
//! rather than being converted silently.
//!
//! # Module map
//! - [`convert`] — block-level BE→LE conversion; [`convert::convert_block`] is
//!   the public entry point. Also hosts [`convert::untile_tiled_dxt_body`].
//! - [`validate`] — post-conversion checks on an LE block (entry table, `CSUM`,
//!   descriptor bounds, float NaN/Inf, world envelope, `IBUF` index bounds).
//! - [`havok`] — Havok 5.5.0-r1 (HK550) packfile conversion (`ANIM` / `PHY2`).
//! - [`audio`] — Xbox wavebank → PC IMA ADPCM transcode (XMA path shells to
//!   `ffmpeg`).
//! - [`lua`] — Lua 5.1 `BINN` bytecode BE→LE via an unluac round-trip (needs a
//!   JRE + `unluac.jar`).
//! - [`aset`] — ASET `packed_block_ref` sub-entry recompute.
//! - [`report`] — schema field coverage tracking.
//!
//! # Example
//! ```no_run
//! use ucfx_byteswap::{convert, validate};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let be_block = std::fs::read("block_be.bin")?;
//! let le_block = convert::convert_block(&be_block, false, None)?;
//! for err in validate::validate_converted_block(&le_block) {
//!     eprintln!("{:?}", err);
//! }
//! # Ok(())
//! # }
//! ```

pub mod aset;
pub mod audio;
pub mod convert;
pub mod havok;
pub mod lua;
pub mod report;
pub mod validate;
