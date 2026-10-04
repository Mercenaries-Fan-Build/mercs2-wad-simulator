//! Xbox 360 BE → PC LE UCFX block converter. Moved out of the `ucfx_byteswap` crate so every
//! game-format codec lives under one roof. The old `ucfx_byteswap` crate is now a thin shim that
//! re-exports everything here for backward compat with existing callers (dlc_port, wad_simulator,
//! ucfx_byteswap's own CLI and tests).
//!
//! One already-decompressed UCFX block in, one converted block out. Not a blind u32 sweep: entry
//! table + descriptor tables are re-emitted LE, ECS component bodies (Layer / WorldEntityData /
//! GuidMap) are swapped at schema-declared field widths, and several embedded payloads are
//! re-encoded rather than swapped — Havok packfiles (section-aware), GPU-tiled DXT textures
//! (untile + rebuilt INFO), wavebanks (Xbox-ADPCM / XMA → PC IMA), Lua BINN bytecode (unluac
//! round-trip), mesh vertex declarations (Xbox 12-byte → 8-byte D3DVERTEXELEMENT9), and
//! terrainmesh (widen + de-strip). Sizes and CSUM trailers are recomputed on the way out.

pub mod aset;
pub mod audio;
pub mod convert;
pub mod havok;
pub mod lua;
pub mod ps3_native;
pub mod report;
pub mod validate;
