//! Xbox 360 BE → PC LE UCFX block converter, exposed as a library so the
//! `dlc_port` driver can call `convert::convert_block` directly (no subprocess).
//! The `ucfx_byteswap` binary (`main.rs`) is a thin CLI over these modules.

pub mod aset;
pub mod audio;
pub mod convert;
pub mod havok;
pub mod lua;
pub mod report;
pub mod validate;
