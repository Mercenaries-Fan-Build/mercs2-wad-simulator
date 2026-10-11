//! m2-sdk.dll, the one shared layer every Mercenaries 2 ASI imports. It owns the process-wide state that
//! only works as a single instance: MinHook, the log-stream subscription, the load ladder, the shader
//! queue and the per-module log files. The exports and their behaviour are the C ABI in `m2/*.h`.
//!
//! There is no `DllMain`: every facility initialises on its first call, and the DLL keeps its thread
//! notifications because the Rust runtime's thread-local destructors run on them.

#[cfg(not(all(windows, target_arch = "x86")))]
compile_error!("m2-sdk.dll is a 32-bit Windows DLL: build it with --target i686-pc-windows-gnu");

mod hook;
mod ini;
mod loadtrigger;
mod log;
mod loghook;
mod luastack;
mod shader;
mod version;

