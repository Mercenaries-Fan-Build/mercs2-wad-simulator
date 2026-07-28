//! Engine-accurate WAD consumption simulator — as a library.
//!
//! Everything here was reachable only from `main.rs` until now. That was costly in a way worth
//! recording: these are the checks that have caught real structural defects (a bare UCFX container
//! where a block table was needed, an ASET row that named a `_P001` rung which did not exist), and
//! a linter that wanted to run them ahead of shipping a mod could not call a single one. Sibling
//! binaries in `src/bin/` had already resorted to `#[path = "../names.rs"] mod names;` to get at
//! this code, which is the shape the problem takes when a crate has no `[lib]`.
//!
//! The binary is unchanged in behaviour: `main.rs` now consumes these modules from here rather than
//! declaring them itself, so there is one copy compiled once.
//!
//! ## What a caller most likely wants
//!
//! - [`texture::texture_buffer_too_small`] — a texture BODY shorter than its dimension-derived mip
//!   chain, which makes the streaming worker over-read and hangs the world load. Its two
//!   false-positive gates are retail-verified; retail has 9,562 legitimately short *streamed*
//!   bodies, so do not reimplement this predicate.
//! - [`chunk_invariants::validate_chunk_invariants`] — per-chunk record alignment and minimum
//!   sizes, each derived from a disassembled handler.
//! - [`aset_validate::run_aset_hash_validation`] — does every ASET row's hash actually live in the
//!   block it claims? Classifies verified / misrouted / true-ghost.
//! - [`simulate`] — the whole consumption pass, and the fatal-verdict terms that decide exit code.

pub mod action_table;
pub mod animation;
pub mod aset_validate;
pub mod audio;
pub mod blocks;
pub mod chunk_invariants;
pub mod consume;
pub mod material;
pub mod model;
pub mod names;
pub mod overlay;
pub mod placement;
pub mod progress;
pub mod pws;
pub mod resident;
pub mod script;
pub mod simulate;
pub mod texture;
