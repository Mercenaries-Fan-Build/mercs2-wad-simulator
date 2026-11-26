//! The C interface `include/mercs2_ue.h` declares.
//!
//! Every object is an opaque pointer the caller frees with its own `_free`/`_close` function. A call
//! that fails returns null (or 0 where it returns a count) and leaves a message for
//! [`m2ue_last_error`]. A panic inside a call is caught at the boundary and reported the same way.
//! Returned buffers belong to their object and live until it is freed.

use crate::data::{self, Game, Mesh, Placement, Section, Texture, Vertex};
use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_error(message: String) {
    // A message with an interior NUL cannot be a C string; its NULs are replaced, not the message dropped.
    let c = CString::new(message.replace('\0', "\u{FFFD}")).expect("NULs replaced above");
    LAST_ERROR.with(|e| *e.borrow_mut() = c);
}

/// Run `f`, turning an `Err` or a panic into `None` with the message kept for [`m2ue_last_error`].
fn guard<T>(f: impl FnOnce() -> Result<T, String>) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => Some(v),
        Ok(Err(e)) => {
            set_error(e);
            None
        }
        Err(p) => {
            let what = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned());
            set_error(format!("panic: {}", what.unwrap_or_else(|| "(no message)".into())));
            None
        }
    }
}

fn boxed<T>(v: Option<T>) -> *mut T {
    v.map_or(std::ptr::null_mut(), |v| Box::into_raw(Box::new(v)))
}

/// `p` as a shared reference, or an error naming `what` when it is null.
///
/// # Safety
/// A non-null `p` must be a live object this library returned.
unsafe fn object<'a, T>(p: *const T, what: &str) -> Result<&'a T, String> {
    p.as_ref().ok_or_else(|| format!("{what} is null"))
}

/// Hand out `slice` as pointer + count. An empty slice is a null pointer with a count of 0.
///
/// # Safety
/// `count` must be null or writable.
unsafe fn span<T>(slice: &[T], count: *mut usize) -> *const T {
    if !count.is_null() {
        *count = slice.len();
    }
    if slice.is_empty() {
        std::ptr::null()
    } else {
        slice.as_ptr()
    }
}

/// The message of the last call that failed on this thread, valid until the next call on it. Empty
/// when none has failed.
#[no_mangle]
pub extern "C" fn m2ue_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ptr())
}

/// `pandemic_hash_m2` of a NUL-terminated UTF-8 name. 0 with an error when `name` is null or not UTF-8.
///
/// # Safety
/// `name` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn m2ue_hash(name: *const c_char) -> u32 {
    guard(|| {
        if name.is_null() {
            return Err("name is null".into());
        }
        let s = CStr::from_ptr(name).to_str().map_err(|e| format!("name is not UTF-8: {e}"))?;
        Ok(data::hash(s))
    })
    .unwrap_or(0)
}

/// Convert a point from the game's space (three floats, metres) to Unreal's (three floats,
/// centimetres): [`crate::space::position`], for a position that comes from the game's scripts rather
/// than from this library. Returns 1, or 0 with an error.
///
/// # Safety
/// `game_metres` must be null or readable for 3 floats; `unreal` null or writable for 3 floats.
#[no_mangle]
pub unsafe extern "C" fn m2ue_position(game_metres: *const f32, unreal: *mut f32) -> u32 {
    guard(|| {
        if game_metres.is_null() || unreal.is_null() {
            return Err("game_metres or unreal is null".into());
        }
        let p = crate::space::position([*game_metres, *game_metres.add(1), *game_metres.add(2)]);
        std::ptr::copy_nonoverlapping(p.as_ptr(), unreal, 3);
        Ok(1)
    })
    .unwrap_or(0)
}

/// Convert a length from the game's metres to Unreal's centimetres: [`crate::space::length`].
#[no_mangle]
pub extern "C" fn m2ue_length(metres: f32) -> f32 {
    crate::space::length(metres)
}

/// 1 when the level archive ships a model under this asset hash, 0 when it does not (a placed entity
/// can name a model the game does not ship, and draws nothing for it), 0 with an error for a null game.
///
/// # Safety
/// `game` must be null or a live [`m2ue_open`] pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_model_exists(game: *const Game, hash: u32) -> u32 {
    guard(|| Ok(u32::from(object(game, "game")?.has_model(hash)))).unwrap_or(0)
}

/// Open the game from the path of its level archive (`vz.wad`), NUL-terminated UTF-8. The archives
/// beside it are mounted with it.
///
/// # Safety
/// `level_wad` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn m2ue_open(level_wad: *const c_char) -> *mut Game {
    boxed(guard(|| {
        if level_wad.is_null() {
            return Err("level_wad is null".into());
        }
        let path = CStr::from_ptr(level_wad).to_str().map_err(|e| format!("level_wad is not UTF-8: {e}"))?;
        Game::open(path)
    }))
}

/// # Safety
/// `game` must be null or a pointer [`m2ue_open`] returned, not yet closed.
#[no_mangle]
pub unsafe extern "C" fn m2ue_close(game: *mut Game) {
    if !game.is_null() {
        drop(Box::from_raw(game));
    }
}

/// A model by asset hash: every LOD block of its chain in one vertex and index buffer, posed at rest.
///
/// # Safety
/// `game` must be null or a live [`m2ue_open`] pointer, used by one thread at a time.
#[no_mangle]
pub unsafe extern "C" fn m2ue_model_load(game: *mut Game, hash: u32) -> *mut Mesh {
    boxed(guard(|| game.as_mut().ok_or("game is null")?.model(hash)))
}

/// A hi-res terrain cell by terrainmesh hash, cell-local.
///
/// # Safety
/// As [`m2ue_model_load`].
#[no_mangle]
pub unsafe extern "C" fn m2ue_terrain_cell_load(game: *mut Game, hash: u32) -> *mut Mesh {
    boxed(guard(|| game.as_mut().ok_or("game is null")?.terrain_cell(hash)))
}

/// A TINY stand-in by its model hash, relative to its placement: one section per world object, state
/// and material, each naming the object it stands in for.
///
/// # Safety
/// As [`m2ue_model_load`].
#[no_mangle]
pub unsafe extern "C" fn m2ue_stand_in_load(game: *mut Game, hash: u32) -> *mut Mesh {
    boxed(guard(|| game.as_mut().ok_or("game is null")?.stand_in(hash)))
}

/// # Safety
/// `mesh` must be null or a live mesh pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_mesh_free(mesh: *mut Mesh) {
    if !mesh.is_null() {
        drop(Box::from_raw(mesh));
    }
}

/// # Safety
/// `mesh` must be null or a live mesh pointer; `count` null or writable.
#[no_mangle]
pub unsafe extern "C" fn m2ue_mesh_vertices(mesh: *const Mesh, count: *mut usize) -> *const Vertex {
    match guard(|| object(mesh, "mesh")) {
        Some(m) => span(&m.vertices, count),
        None => span(&[], count),
    }
}

/// The triangle list: three indices per triangle.
///
/// # Safety
/// As [`m2ue_mesh_vertices`].
#[no_mangle]
pub unsafe extern "C" fn m2ue_mesh_indices(mesh: *const Mesh, count: *mut usize) -> *const u32 {
    match guard(|| object(mesh, "mesh")) {
        Some(m) => span(&m.indices, count),
        None => span(&[], count),
    }
}

/// # Safety
/// As [`m2ue_mesh_vertices`].
#[no_mangle]
pub unsafe extern "C" fn m2ue_mesh_sections(mesh: *const Mesh, count: *mut usize) -> *const Section {
    match guard(|| object(mesh, "mesh")) {
        Some(m) => span(&m.sections, count),
        None => span(&[], count),
    }
}

/// LOD tiers the model declares; 1 for a terrain cell; 0 with an error for a null mesh.
///
/// # Safety
/// `mesh` must be null or a live mesh pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_mesh_lod_count(mesh: *const Mesh) -> u32 {
    guard(|| object(mesh, "mesh")).map_or(0, |m| m.lod_count)
}

/// The distance from the main camera, in Unreal units, at which the game starts drawing tier `tier`
/// (1 or above) in place of the tier before it ([`Mesh::lod_switch_distance`]). Returns -1 with an
/// error for a null mesh, tier 0, or a tier the mesh does not have.
///
/// # Safety
/// `mesh` must be null or a live mesh pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_mesh_lod_switch_distance(mesh: *const Mesh, tier: u32) -> f32 {
    guard(|| object(mesh, "mesh")?.lod_switch_distance(tier)).unwrap_or(-1.0)
}

/// Write the mesh's bounds (three floats each). Returns 1, or 0 with an error.
///
/// # Safety
/// `mesh` must be null or a live mesh pointer; `min` and `max` each null or writable for 3 floats.
#[no_mangle]
pub unsafe extern "C" fn m2ue_mesh_bounds(mesh: *const Mesh, min: *mut f32, max: *mut f32) -> u32 {
    guard(|| {
        let m = object(mesh, "mesh")?;
        if min.is_null() || max.is_null() {
            return Err("min or max is null".into());
        }
        std::ptr::copy_nonoverlapping(m.bounds_min.as_ptr(), min, 3);
        std::ptr::copy_nonoverlapping(m.bounds_max.as_ptr(), max, 3);
        Ok(1)
    })
    .unwrap_or(0)
}

/// A texture by asset hash, with its streamed mips where the archives ship them.
///
/// # Safety
/// As [`m2ue_model_load`].
#[no_mangle]
pub unsafe extern "C" fn m2ue_texture_load(game: *mut Game, hash: u32) -> *mut Texture {
    boxed(guard(|| game.as_mut().ok_or("game is null")?.texture(hash)))
}

/// # Safety
/// `texture` must be null or a live texture pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_texture_free(texture: *mut Texture) {
    if !texture.is_null() {
        drop(Box::from_raw(texture));
    }
}

/// What a texture is: its size, block compression and mip count.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureInfo {
    pub width: u32,
    pub height: u32,
    /// `M2UE_TEXTURE_BC1` or `M2UE_TEXTURE_BC3`.
    pub format: u32,
    pub mip_count: u32,
}

/// Write the texture's description. Returns 1, or 0 with an error.
///
/// # Safety
/// `texture` must be null or a live texture pointer; `info` null or writable.
#[no_mangle]
pub unsafe extern "C" fn m2ue_texture_info(texture: *const Texture, info: *mut TextureInfo) -> u32 {
    guard(|| {
        let t = object(texture, "texture")?;
        if info.is_null() {
            return Err("info is null".into());
        }
        *info = TextureInfo { width: t.width, height: t.height, format: t.format, mip_count: t.mip_count };
        Ok(1)
    })
    .unwrap_or(0)
}

/// The whole mip chain, largest mip first, each mip's blocks in row order.
///
/// # Safety
/// `texture` must be null or a live texture pointer; `size` null or writable.
#[no_mangle]
pub unsafe extern "C" fn m2ue_texture_data(texture: *const Texture, size: *mut usize) -> *const u8 {
    match guard(|| object(texture, "texture")) {
        Some(t) => span(&t.data, size),
        None => span(&[], size),
    }
}

/// The largest mip decoded to 8-bit RGBA: `width · height · 4` bytes, row-major from the top row.
///
/// # Safety
/// As [`m2ue_texture_data`].
#[no_mangle]
pub unsafe extern "C" fn m2ue_texture_rgba(texture: *const Texture, size: *mut usize) -> *const u8 {
    match guard(|| object(texture, "texture")) {
        Some(t) => span(&t.rgba, size),
        None => span(&[], size),
    }
}

/// The layers of the mounted archives: their records, and the block name of each as a C string.
pub struct LayerList {
    infos: Vec<data::LayerInfo>,
    blocks: Vec<CString>,
}

/// Every layer the mounted archives carry, ascending by name hash.
///
/// # Safety
/// `game` must be null or a live [`m2ue_open`] pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_layers(game: *const Game) -> *mut LayerList {
    boxed(guard(|| {
        let layers = object(game, "game")?.layers()?;
        let blocks = layers
            .iter()
            .map(|l| CString::new(l.block.as_str()).map_err(|_| format!("layer 0x{:08X}: its block name holds a NUL", l.info.hash)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LayerList { infos: layers.iter().map(|l| l.info).collect(), blocks })
    }))
}

/// # Safety
/// `list` must be null or a live list pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_layers_free(list: *mut LayerList) {
    if !list.is_null() {
        drop(Box::from_raw(list));
    }
}

/// # Safety
/// `list` must be null or a live list pointer; `count` null or writable.
#[no_mangle]
pub unsafe extern "C" fn m2ue_layers_infos(list: *const LayerList, count: *mut usize) -> *const data::LayerInfo {
    match guard(|| object(list, "list")) {
        Some(l) => span(&l.infos, count),
        None => span(&[], count),
    }
}

/// The base name of the block that ships layer `index` of the list (`layers_static`,
/// `vz_state_pmcinterior`), NUL-terminated; null with an error when `index` is past the list.
///
/// # Safety
/// `list` must be null or a live list pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_layers_block(list: *const LayerList, index: usize) -> *const c_char {
    guard(|| {
        let l = object(list, "list")?;
        l.blocks.get(index).map(|b| b.as_ptr()).ok_or_else(|| format!("layer index {index} of {}", l.blocks.len()))
    })
    .unwrap_or(std::ptr::null())
}

/// One layer's placed entities.
pub struct Layer(Vec<Placement>);

/// Every placed entity of a layer, by layer name hash, in data order.
///
/// # Safety
/// As [`m2ue_model_load`].
#[no_mangle]
pub unsafe extern "C" fn m2ue_layer_load(game: *mut Game, hash: u32) -> *mut Layer {
    boxed(guard(|| game.as_mut().ok_or("game is null")?.layer(hash).map(Layer)))
}

/// # Safety
/// `layer` must be null or a live layer pointer.
#[no_mangle]
pub unsafe extern "C" fn m2ue_layer_free(layer: *mut Layer) {
    if !layer.is_null() {
        drop(Box::from_raw(layer));
    }
}

/// # Safety
/// `layer` must be null or a live layer pointer; `count` null or writable.
#[no_mangle]
pub unsafe extern "C" fn m2ue_layer_placements(layer: *const Layer, count: *mut usize) -> *const Placement {
    match guard(|| object(layer, "layer")) {
        Some(l) => span(&l.0, count),
        None => span(&[], count),
    }
}

/// The world layers a save restores, as C strings.
pub struct SaveLayers(Vec<CString>);

/// Read the world layers a save file (`*.profile`) restores: its `tLayerData`, in the save's order.
/// `profile` is the file's path, NUL-terminated UTF-8.
///
/// # Safety
/// `profile` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn m2ue_save_layers(profile: *const c_char) -> *mut SaveLayers {
    boxed(guard(|| {
        if profile.is_null() {
            return Err("profile is null".into());
        }
        let path = CStr::from_ptr(profile).to_str().map_err(|e| format!("profile is not UTF-8: {e}"))?;
        let names = data::save_layers(std::path::Path::new(path))?;
        names
            .into_iter()
            .map(|n| CString::new(n).map_err(|e| format!("{path}: a layer name holds a NUL ({e})")))
            .collect::<Result<Vec<_>, _>>()
            .map(SaveLayers)
    }))
}

/// # Safety
/// `layers` must be null or a live pointer [`m2ue_save_layers`] returned.
#[no_mangle]
pub unsafe extern "C" fn m2ue_save_layers_free(layers: *mut SaveLayers) {
    if !layers.is_null() {
        drop(Box::from_raw(layers));
    }
}

/// How many layers the save restores; 0 with an error for a null pointer.
///
/// # Safety
/// `layers` must be null or a live pointer [`m2ue_save_layers`] returned.
#[no_mangle]
pub unsafe extern "C" fn m2ue_save_layers_count(layers: *const SaveLayers) -> usize {
    guard(|| object(layers, "layers")).map_or(0, |l| l.0.len())
}

/// The name of layer `index`, NUL-terminated, as the save stores it (lower case); null with an error
/// when `index` is past the list.
///
/// # Safety
/// `layers` must be null or a live pointer [`m2ue_save_layers`] returned.
#[no_mangle]
pub unsafe extern "C" fn m2ue_save_layers_name(layers: *const SaveLayers, index: usize) -> *const c_char {
    guard(|| {
        let l = object(layers, "layers")?;
        l.0.get(index).map(|n| n.as_ptr()).ok_or_else(|| format!("save layer index {index} of {}", l.0.len()))
    })
    .unwrap_or(std::ptr::null())
}

/// Write the light of the atmosphere setting named `setting` (NUL-terminated UTF-8; `default` is a
/// region's until a script changes it) that applies at a point of the game's space (three floats,
/// metres): [`Game::atmosphere_at`]. Returns 1, or 0 with an error.
///
/// # Safety
/// `game` must be null or a live pointer [`m2ue_open`] returned; `game_position` null or readable
/// for three floats; `setting` null or a NUL-terminated string; `out` null or writable for one
/// [`crate::atmosphere::Atmosphere`].
#[no_mangle]
pub unsafe extern "C" fn m2ue_atmosphere_at(game: *mut Game, game_position: *const f32, setting: *const c_char, out: *mut crate::atmosphere::Atmosphere) -> u32 {
    guard(|| {
        let game = game.as_mut().ok_or("game is null")?;
        if game_position.is_null() || setting.is_null() || out.is_null() {
            return Err("game_position, setting or out is null".into());
        }
        let setting = CStr::from_ptr(setting).to_str().map_err(|e| format!("setting is not UTF-8: {e}"))?;
        *out = game.atmosphere_at(std::ptr::read_unaligned(game_position.cast::<[f32; 3]>()), setting)?;
        Ok(1)
    })
    .unwrap_or(0)
}

// The header pins these sizes with `_Static_assert`; a record that changes here must change there.
const _: () = assert!(std::mem::size_of::<crate::atmosphere::Atmosphere>() == 120);
const _: () = assert!(std::mem::offset_of!(crate::atmosphere::Atmosphere, ambient) == 40);
const _: () = assert!(std::mem::size_of::<Vertex>() == 72);
const _: () = assert!(std::mem::size_of::<data::Ground>() == 164);
const _: () = assert!(std::mem::size_of::<Section>() == 344);
const _: () = assert!(std::mem::offset_of!(Section, diffuse_map) == 28);
const _: () = assert!(std::mem::size_of::<Placement>() == 128);
const _: () = assert!(std::mem::offset_of!(Placement, archetype) == 124);
const _: () = assert!(std::mem::size_of::<TextureInfo>() == 16);
const _: () = assert!(std::mem::size_of::<data::LayerInfo>() == 8);
const _: () = assert!(std::mem::offset_of!(Section, textures) == 44);
const _: () = assert!(std::mem::offset_of!(Section, ground) == 180);
const _: () = assert!(std::mem::offset_of!(Placement, wake_radius) == 120);
const _: () = assert!(std::mem::offset_of!(Placement, position) == 28);
const _: () = assert!(std::mem::offset_of!(Placement, light_kind) == 76);

#[cfg(test)]
mod tests {
    use super::*;

    fn last_error() -> String {
        unsafe { CStr::from_ptr(m2ue_last_error()) }.to_str().unwrap().to_string()
    }

    #[test]
    fn a_name_hashes_as_the_formats_crate_hashes_it() {
        let name = CString::new("pmcoutpost_interior_hq").unwrap();
        assert_eq!(unsafe { m2ue_hash(name.as_ptr()) }, mercs2_formats::hash::pandemic_hash_m2("pmcoutpost_interior_hq"));
    }

    #[test]
    fn a_script_position_converts_as_the_library_converts_its_own() {
        let (game, mut unreal) = ([3750.0f32, 450.0, -3840.0], [0.0f32; 3]);
        assert_eq!(unsafe { m2ue_position(game.as_ptr(), unreal.as_mut_ptr()) }, 1);
        assert_eq!(unreal, crate::space::position(game));
        assert_eq!(unsafe { m2ue_position(std::ptr::null(), unreal.as_mut_ptr()) }, 0);
    }

    /// A vendored retail save: its layers are the lower-case names `MrxLayerManager.SaveSingleton` wrote.
    #[test]
    fn a_save_lists_the_layers_it_restores() {
        let path = mercs2_formats::game_paths::save_fixtures().join("Mattias Nilsson_6A0E523C.profile");
        let c = CString::new(path.to_str().unwrap()).unwrap();
        let layers = unsafe { m2ue_save_layers(c.as_ptr()) };
        assert!(!layers.is_null(), "{}", last_error());
        let n = unsafe { m2ue_save_layers_count(layers) };
        let names: Vec<String> = (0..n).map(|i| unsafe { CStr::from_ptr(m2ue_save_layers_name(layers, i)) }.to_str().unwrap().to_string()).collect();
        assert_eq!(names, data::save_layers(&path).unwrap());
        assert!(n > 0 && names.iter().all(|l| *l == l.to_lowercase()), "{names:?}");
        assert!(names.iter().any(|l| l.starts_with("vz_state_")), "{names:?}");
        assert!(unsafe { m2ue_save_layers_name(layers, n) }.is_null());
        unsafe { m2ue_save_layers_free(layers) };
        let missing = CString::new("/nonexistent/x.profile").unwrap();
        assert!(unsafe { m2ue_save_layers(missing.as_ptr()) }.is_null());
        assert!(last_error().contains("/nonexistent/x.profile"), "{}", last_error());
    }

    #[test]
    fn a_missing_archive_is_null_with_a_message() {
        let path = CString::new("/nonexistent/vz.wad").unwrap();
        let game = unsafe { m2ue_open(path.as_ptr()) };
        assert!(game.is_null());
        assert!(last_error().contains("/nonexistent/vz.wad"), "{}", last_error());
    }

    #[test]
    fn an_atmosphere_without_a_game_is_an_error() {
        let mut a = crate::atmosphere::Atmosphere { time: 0.0, toward_sun: [0.0; 3], light_color: [0.0; 3], rim_color: [0.0; 3], ambient: [[0.0; 3]; 6], mie_g: 0.0, entity: 0 };
        let (at, name) = ([0.0f32; 3], CString::new("default").unwrap());
        unsafe {
            assert_eq!(m2ue_atmosphere_at(std::ptr::null_mut(), at.as_ptr(), name.as_ptr(), &mut a), 0);
            assert_eq!(last_error(), "game is null");
        }
    }

    #[test]
    fn null_arguments_are_errors_not_crashes() {
        unsafe {
            assert!(m2ue_open(std::ptr::null()).is_null());
            assert_eq!(last_error(), "level_wad is null");
            assert!(m2ue_model_load(std::ptr::null_mut(), 1).is_null());
            assert_eq!(last_error(), "game is null");
            let mut n = 7usize;
            assert!(m2ue_mesh_vertices(std::ptr::null(), &mut n).is_null());
            assert_eq!((n, last_error().as_str()), (0, "mesh is null"));
            assert_eq!(m2ue_mesh_lod_count(std::ptr::null()), 0);
            assert_eq!(m2ue_mesh_lod_switch_distance(std::ptr::null(), 1), -1.0);
            assert_eq!(last_error(), "mesh is null");
            assert_eq!(m2ue_hash(std::ptr::null()), 0);
            m2ue_close(std::ptr::null_mut());
            m2ue_mesh_free(std::ptr::null_mut());
            m2ue_texture_free(std::ptr::null_mut());
            m2ue_layer_free(std::ptr::null_mut());
            m2ue_layers_free(std::ptr::null_mut());
        }
    }

    #[test]
    fn a_panic_is_reported_not_unwound_across_the_boundary() {
        let got: Option<()> = guard(|| panic!("decoder bug"));
        assert!(got.is_none());
        assert_eq!(last_error(), "panic: decoder bug");
    }

    #[test]
    fn an_empty_buffer_is_a_null_pointer_with_no_elements() {
        let mut n = 9usize;
        let p = unsafe { span::<u32>(&[], &mut n) };
        assert!(p.is_null());
        assert_eq!(n, 0);
    }
}
