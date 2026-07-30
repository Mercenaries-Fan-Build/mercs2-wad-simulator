//! `loadprobe` as a LIBRARY, alongside the `loadprobe` binary.
//!
//! # Why this file exists
//!
//! Version 1.0 of this crate exposed a library; 2.0 shipped **bin-only** (no `[lib]`, no
//! `lib.rs`). That silently broke every downstream consumer, because a bin-only crate cannot be
//! linked: `use loadprobe::{parse, report, sha256};` fails with *"use of unresolved module or
//! unlinked crate"* even though the version resolves and downloads perfectly. It is a
//! particularly quiet regression — `cargo build` gets as far as compiling the dependent's own
//! source before failing, so it reads as the dependent's bug rather than a missing target.
//!
//! Found via the modkit, which consumes `parse`, `report` and `sha256` to score
//! `pmc_blackbox.log` in its UI and to hash deployed artifacts.
//!
//! The binary keeps its own private `mod` declarations, so nothing about its behaviour changes —
//! this adds a target, it does not restructure the crate. `phases` and `symbolize` are exported
//! too because `report` and `parse` are only useful with the phase vocabulary they classify
//! against, and because both are already reachable via `crate::` from the modules above.

pub mod parse;
pub mod phases;
pub mod report;
pub mod sha256;
pub mod symbolize;
