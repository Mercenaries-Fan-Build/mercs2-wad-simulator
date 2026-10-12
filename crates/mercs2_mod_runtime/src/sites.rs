//! The Lua host's code sites in the running game, resolved once per process from the mapped image
//! (`mercs2_formats::lua_sites`). SecuROM decrypts `.text` after the image is mapped, so a resolution that
//! finds nothing yet is retried until [`WAIT`] runs out.

use crate::rt_log;
use mercs2_formats::exe_image::{Image, parse_headers};
use mercs2_formats::lua_sites::{LuaSites, resolve};
use mercs2_formats::shader_sites::{self, ShaderSites};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;

/// How long a caller that may wait does wait for the sites to resolve.
pub(crate) const WAIT: Duration = Duration::from_secs(30);
const RETRY: Duration = Duration::from_millis(50);
/// The headers fit in the first page of the image.
const HEADER_BYTES: usize = 0x1000;

static SITES: OnceLock<LuaSites> = OnceLock::new();
static SHADERS: OnceLock<ShaderSites> = OnceLock::new();

fn mapped() -> Result<Image<'static>, String> {
    let base = unsafe { GetModuleHandleA(core::ptr::null()) } as *const u8;
    if base.is_null() {
        return Err("GetModuleHandleA(NULL) returned no module".into());
    }
    let header = unsafe { core::slice::from_raw_parts(base, HEADER_BYTES) };
    let (image_base, headers) = parse_headers(header).map_err(|e| format!("the game's headers: {e}"))?;
    Ok(unsafe { Image::from_mapped(base, image_base, headers) })
}

fn resolve_mapped() -> Result<LuaSites, String> {
    resolve(&mapped()?).map_err(|e| e.to_string())
}

/// The shader registry's sites, resolving them now when that has not been done; never waits (the add
/// functions run in a mod's `DllMain`).
pub(crate) fn shaders_now() -> Result<&'static ShaderSites, String> {
    if let Some(s) = SHADERS.get() {
        return Ok(s);
    }
    let s = shader_sites::resolve(&mapped()?).map_err(|e| e.to_string())?;
    let s = SHADERS.get_or_init(|| s);
    rt_log::line(&format!(
        "shader sites: RegisterAll {:#x}, Register {:#x}, base ctors {:#x} / {:#x}, pool {:#x}, ShaderLevel {:#x}, 45 family vtables",
        s.register_all, s.register, s.ps_base_ctor, s.vs_base_ctor, s.ps_count, s.shader_level
    ));
    Ok(s)
}

fn publish(s: LuaSites) -> &'static LuaSites {
    let s = SITES.get_or_init(|| s);
    for site in mercs2_formats::lua_sites::Site::ALL {
        rt_log::line(&format!("site {} -> {:#x}", site.name(), s.va(site)));
    }
    rt_log::line(&format!("script host at {:#x}", s.host()));
    s
}

/// The sites, resolving them now when that has not been done; `Err` names what did not resolve.
pub(crate) fn now() -> Result<&'static LuaSites, String> {
    if let Some(s) = SITES.get() {
        return Ok(s);
    }
    resolve_mapped().map(publish)
}

/// The sites, retrying until they resolve or [`WAIT`] runs out. Never call it under the loader lock.
pub(crate) fn wait() -> Result<&'static LuaSites, String> {
    let deadline = Instant::now() + WAIT;
    loop {
        match now() {
            Ok(s) => return Ok(s),
            Err(e) if Instant::now() >= deadline => {
                return Err(format!("the game's Lua sites did not resolve within {}s: {e}", WAIT.as_secs()));
            }
            Err(_) => std::thread::sleep(RETRY),
        }
    }
}
