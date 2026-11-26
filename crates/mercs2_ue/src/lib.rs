//! Mercenaries 2 game data for Unreal Engine.
//!
//! A static library with a C interface (`include/mercs2_ue.h`) that an Unreal plugin links. It opens
//! the game's archives and hands out models, terrain cells, textures and layer placements already in
//! Unreal's space, so the plugin parses no game format and converts no axis.
//!
//! * [`space`] — game space to Unreal space: the only place an axis, a unit or a winding changes.
//! * [`data`] — the engine's decoders, their output posed at rest and converted.
//! * [`ffi`] — the C interface.

pub mod atmosphere;
pub mod data;
pub mod ffi;
pub mod space;
