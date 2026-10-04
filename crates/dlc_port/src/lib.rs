//! Library surface of the `dlc_port` crate.
//!
//! The crate's primary artifact is the `dlc_port` binary; this library exists
//! solely to expose the modules that have no inherent coupling to the CLI
//! driver (the Xbox-DOH side oracle lookup machinery) so integration tests
//! under `tests/` can drive them directly. Nothing in `main.rs` depends on
//! this library — the binary links the module source in-tree with `mod
//! xbox_doh_oracle;` as before.

pub mod xbox_doh_oracle;
