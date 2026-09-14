//! Model UCFX container editors, moved out of wad_builder. Each submodule mutates one aspect of
//! a shipped model container in place (MTRL / reskin static→SKIN / mesh unwrap / vertex layout)
//! and recomputes the container CSUM.

pub mod mtrl;
pub mod reskin;
pub mod unwrap;
pub mod vertex;
