//! Render types + GPU helper glue shared by the wgpu render paths.
//!
//! These moved out of the engine binary so the streaming-world render (`game_world`) and the
//! `scene` renderer live in the library and can be driven in-process by `mercs2_game` (the game exe
//! calls `game_world::run_game_world` directly instead of shelling out to a separate engine binary).
//!
//! Nothing here changes behaviour — it is a faithful relocation of `main.rs`'s render helpers,
//! the `TexMap`/`ClipAnim`/`LoadedModel`/`LoadProgress` types, and the depth format.

use crate::mesh::{self, Vertex};

/// Depth attachment format used by every render pass in the engine.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Model-hash → decoded texture bytes, keyed for the scene's per-material bind groups.
pub type TexMap = std::collections::HashMap<u32, mercs2_formats::texture::TextureData>;

/// Re-export so render/game-world call sites can name `DrawGroup` alongside the other render types.
pub use crate::mesh::DrawGroup;

/// Upload a decoded DXT/BC texture (mip 0) and return its view. Returns None if the resident
/// data is too short (partial/streamed texture) so the caller can fall back.
pub fn make_bc_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    td: &mercs2_formats::texture::TextureData,
    srgb: bool,
) -> Option<wgpu::TextureView> {
    use mercs2_formats::texture::TexFormat;
    // Diffuse is sRGB (gamma); normal/spec maps are linear data.
    let (format, block_bytes) = match (td.format, srgb) {
        (TexFormat::Bc1, true) => (wgpu::TextureFormat::Bc1RgbaUnormSrgb, 8u32),
        (TexFormat::Bc1, false) => (wgpu::TextureFormat::Bc1RgbaUnorm, 8u32),
        (TexFormat::Bc3, true) => (wgpu::TextureFormat::Bc3RgbaUnormSrgb, 16u32),
        (TexFormat::Bc3, false) => (wgpu::TextureFormat::Bc3RgbaUnorm, 16u32),
    };
    if td.width == 0 || td.height == 0 {
        return None;
    }
    let mip_bytes = |lvl: u32| -> usize {
        let w = (td.width >> lvl).max(1);
        let h = (td.height >> lvl).max(1);
        (((w + 3) / 4) * block_bytes * ((h + 3) / 4)) as usize
    };
    // wgpu validates `copy_size.width % block_width == 0` UNCONDITIONALLY (wgpu-core
    // `validate_texture_copy_range`) — there is NO exemption for a mip whose entire extent is already
    // narrower than one block, and rounding the copy extent UP instead trips the "exceeds texture
    // extent" check. So a BC chain can only be uploaded down to its last 4-aligned level; writing the
    // 2×1 / 1×1 tail is an instant validation PANIC, not a recoverable error. Those levels carry no
    // visible detail, so capping the chain there costs nothing.
    let block_aligned = |lvl: u32| -> bool {
        let w = (td.width >> lvl).max(1);
        let h = (td.height >> lvl).max(1);
        w % 4 == 0 && h % 4 == 0
    };
    // A mip 0 that is not itself 4-aligned cannot be uploaded at all. Report it as unavailable so the
    // caller falls back to the white/flat default instead of taking the process down.
    if !block_aligned(0) {
        return None;
    }
    let mip0_need = mip_bytes(0);
    // When the FULL chain (mip 0 first) is present — a hi-res texture assembled from the streaming LOD
    // blocks, or a small fully-resident texture — upload EVERY mip level so the sampler mips down and
    // doesn't shimmer at distance/grazing angles. Otherwise these are STREAMED textures that only
    // shipped the resident low-res TAIL (high mips not fetched) — upload the largest resident level so
    // they show textured (low-res) instead of white.
    if td.mip0.len() >= mip0_need && td.all_mips.len() >= mip0_need {
        // Count whole mip levels present in all_mips from level 0.
        let mut levels = 0u32;
        let mut acc = 0usize;
        for l in 0..td.mip_count.max(1) {
            let mb = mip_bytes(l);
            if mb > 0 && acc + mb <= td.all_mips.len() && block_aligned(l) {
                acc += mb;
                levels += 1;
            } else {
                break;
            }
        }
        let levels = levels.max(1);
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("diffuse"),
            size: wgpu::Extent3d { width: td.width, height: td.height, depth_or_array_layers: 1 },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut off = 0usize;
        for l in 0..levels {
            let w = (td.width >> l).max(1);
            let h = (td.height >> l).max(1);
            let bw = (w + 3) / 4;
            let bh = (h + 3) / 4;
            let sz = (bw * block_bytes * bh) as usize;
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &tex,
                    mip_level: l,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &td.all_mips[off..off + sz],
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bw * block_bytes),
                    rows_per_image: Some(bh),
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            off += sz;
        }
        return Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
    }

    // Resident-tail-only fallback: build from the largest resident mip level (low-res, single level).
    let avail = td.all_mips.len();
    let mut chosen = None;
    for l in 1..td.mip_count.max(1) {
        let tail: usize = (l..td.mip_count).map(mip_bytes).sum();
        if tail > 0 && tail == avail {
            chosen = Some(((td.width >> l).max(1), (td.height >> l).max(1), mip_bytes(l)));
            break;
        }
    }
    let (base_w, base_h, base_bytes) = match chosen {
        // Same block-alignment rule as the full-chain branch above: an unaligned resident level is
        // unuploadable, so fall back rather than panic.
        Some((w, h, sz)) if avail >= sz && w % 4 == 0 && h % 4 == 0 => (w, h, sz),
        _ => return None,
    };
    let blocks_wide = (base_w + 3) / 4;
    let blocks_high = (base_h + 3) / 4;
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("diffuse"),
        size: wgpu::Extent3d { width: base_w, height: base_h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &td.all_mips[..base_bytes],
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(blocks_wide * block_bytes),
            rows_per_image: Some(blocks_high),
        },
        wgpu::Extent3d { width: base_w, height: base_h, depth_or_array_layers: 1 },
    );
    Some(tex.create_view(&wgpu::TextureViewDescriptor::default()))
}

/// The retail pixel shaders' samplers, read from `shader3.bin` in the game's data folder, beside
/// `vz_wad`. Every pixel shader a retail model material names declares the same samplers in
/// `shader3Low.bin` (`tests/material_shaders.rs`), so the ShaderLevel choice of store does not change
/// what a slot binds.
pub fn pixel_samplers(vz_wad: &std::path::Path) -> Result<mercs2_formats::shader_registrations::PixelSamplers, String> {
    let path = vz_wad.with_file_name("shader3.bin");
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let store = mercs2_formats::shader3::Store::parse(bytes).map_err(|e| format!("{}: {e:?}", path.display()))?;
    mercs2_formats::shader_registrations::PixelSamplers::from_store(&store, false)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// What a material's slot 1 is to its pixel shader: the sampler the shader declares at stage 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot1Role {
    /// The shader samples nothing at stage 1 (e.g. `PgDiffFP`).
    None,
    /// `specularMap`: the highlight mask.
    Specular,
    /// `emissiveMap`: added after lighting, times the material's emissive colour (`PgDiffEmis*`).
    Emissive,
}

/// The textures one draw binds and what its slot 1 is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialSlots {
    pub diffuse: Option<u32>,
    pub normal: Option<u32>,
    pub slot1: Option<u32>,
    pub slot1_role: Slot1Role,
    /// For a blended material ([`mercs2_formats::texture::material_blended`]), what its pixel shader
    /// writes to `oC0.w`: the source alpha the blend reads. `None` for an opaque draw.
    pub output_alpha: Option<mercs2_formats::shader_registrations::OutputAlpha>,
}

/// The textures a draw binds, by what its pixel shader samples. `FUN_00858320` binds material slot
/// `i` to texture stage `i` (`FUN_00484280(i, …)`, `0x858390`), so a slot's meaning is the sampler the
/// shader declares at that stage: `diffuseMap` at 0, `specularMap` or `emissiveMap` at 1, `normalMap`
/// at 2 for every retail model material (`tests/material_shaders.rs`). A slot the shader does not
/// sample binds nothing; any other sampler name is an error, as is a key the registry does not
/// register. A draw with no material (`shader_key` 0: the hand-built terrain, imports, debug draws)
/// binds its own `diffuse`/`specular`/`normal` and draws opaque. A blended material whose shader's
/// output alpha is not one `OutputAlpha` names is an error.
pub fn material_slots(
    d: &mesh::DrawGroup,
    samplers: &mercs2_formats::shader_registrations::PixelSamplers,
) -> Result<MaterialSlots, String> {
    if d.shader_key == 0 {
        if mercs2_formats::texture::material_blended(d.material_flags) {
            return Err(format!("a draw with no pixel shader has blended flags 0x{:04X}", d.material_flags));
        }
        return Ok(MaterialSlots {
            diffuse: d.diffuse,
            normal: d.normal,
            slot1: d.specular,
            slot1_role: if d.specular.is_some() { Slot1Role::Specular } else { Slot1Role::None },
            output_alpha: None,
        });
    }
    let slot = |i: usize| d.textures.get(i).copied().filter(|&h| h != 0);
    let role = |i: u8| samplers.sampler_at(d.shader_key, i);
    let key = d.shader_key;
    let diffuse = match role(0)? {
        Some("diffuseMap") => slot(0),
        None => None,
        Some(other) => return Err(format!("pixel shader 0x{key:08X}: stage 0 is `{other}`, not a material role")),
    };
    let (slot1, slot1_role) = match role(1)? {
        Some("specularMap") => (slot(1), Slot1Role::Specular),
        Some("emissiveMap") => (slot(1), Slot1Role::Emissive),
        None => (None, Slot1Role::None),
        Some(other) => return Err(format!("pixel shader 0x{key:08X}: stage 1 is `{other}`, not a material role")),
    };
    let normal = match role(2)? {
        Some("normalMap") => slot(2),
        None => None,
        Some(other) => return Err(format!("pixel shader 0x{key:08X}: stage 2 is `{other}`, not a material role")),
    };
    // Ground cover that is not a cutout draws blended past its fade start whatever its flags say
    // (`FUN_004a5830`).
    let fades = d.scrub.is_some() && !mercs2_formats::texture::material_alpha_tested(d.material_flags);
    let output_alpha = if mercs2_formats::texture::material_blended(d.material_flags) || fades {
        Some(samplers.output_alpha(key)?)
    } else {
        None
    };
    Ok(MaterialSlots { diffuse, normal, slot1, slot1_role, output_alpha })
}

/// A material's uniform (shader `MaterialParams`): `alpha` = (the reference alpha a pixel must
/// exceed, or -1 for a material that is not alpha-tested; the blended material's output alpha
/// (`OutputAlpha::code`), or -1 for an opaque draw; the slot-1 role: 0 none, 1 specular,
/// 2 emissive; unused), then `materialData` c93..c98 as the draw uploads it raw. Retail tests
/// `diffuse.a × c98.w > ALPHAREF/255` with `ALPHAFUNC` GREATER, and only for a material whose flags
/// carry the alpha-test bit (`mercs2_formats::texture::material_alpha_tested`).
pub fn material_params(
    device: &wgpu::Device,
    material_flags: u16,
    material_data: &[[f32; 4]; 6],
    slot1_role: Slot1Role,
    output_alpha: Option<mercs2_formats::shader_registrations::OutputAlpha>,
) -> wgpu::Buffer {
    params_buffer(device, material_alpha(material_flags, slot1_role, output_alpha, 0.0), material_data, [[0.0; 4]; GROUND_ROWS])
}

/// `MaterialParams.alpha` of a draw, `kind` its `w`: 0 a model, 1 terrain ground, 2 ground cover.
fn material_alpha(
    material_flags: u16,
    slot1_role: Slot1Role,
    output_alpha: Option<mercs2_formats::shader_registrations::OutputAlpha>,
    kind: f32,
) -> [f32; 4] {
    use mercs2_formats::texture::{material_alpha_tested, MATERIAL_ALPHA_REF};
    let alpha_ref = if material_alpha_tested(material_flags) { MATERIAL_ALPHA_REF as f32 / 255.0 } else { -1.0 };
    let role = match slot1_role {
        Slot1Role::None => 0.0,
        Slot1Role::Specular => 1.0,
        Slot1Role::Emissive => 2.0,
    };
    [alpha_ref, output_alpha.map_or(-1.0, |a| a.code() as f32), role, kind]
}

/// The rows of `MaterialParams.ground`.
const GROUND_ROWS: usize = 11;

/// A ground cover draw's uniform: [`material_params`] with `alpha.w` = 2 and, in `ground` row 0, its
/// fade (the depth alpha reaches 0, 1 / its range). `PgScrub*VP` writes
/// `COLOR0.w = dp4_sat(world, AlphaVec)`, and `AlphaVec` (`FUN_004a5480`) is the view depth plane
/// `(alpha_zero − depth) / range`, so `shader.wgsl` reads the same value from a pixel's own depth. Its
/// pixel shaders multiply that into their output alpha (`PgScrubLightFP`: `tex.a × COLOR0.w × c98.w`),
/// which the alpha test compares.
pub fn scrub_params(
    device: &wgpu::Device,
    d: &mesh::DrawGroup,
    depths: mesh::ScrubDepths,
    slot1_role: Slot1Role,
    output_alpha: Option<mercs2_formats::shader_registrations::OutputAlpha>,
) -> wgpu::Buffer {
    let mut rows = [[0.0; 4]; GROUND_ROWS];
    rows[0] = [depths.alpha_zero, 1.0 / (depths.cull - depths.alpha_zero), 0.0, 0.0];
    params_buffer(device, material_alpha(d.material_flags, slot1_role, output_alpha, 2.0), &d.material_data, rows)
}

/// A terrain ground draw's uniform: `alpha.w` = 1, and in `ground` the constants of its
/// `PgTerrainMeshVP*`, which `shader.wgsl` reads per pixel: rows 0..8 = each layer's two projection
/// rows, row 8 = (cell centre x, z, UV scale u, v), row 9 = the `FadeDistances` it uses, (global
/// start, 1 / global range, detail start, 1 / detail range), row 10 = (1 / detail tile, 1 when the
/// material has a detail map).
pub fn ground_params(device: &wgpu::Device, material_data: &[[f32; 4]; 6], g: &mesh::TerrainGround) -> wgpu::Buffer {
    let f = g.fade;
    let mut rows = [[0.0; 4]; GROUND_ROWS];
    for (i, uv) in g.layer_uv.iter().enumerate() {
        rows[2 * i] = uv[0];
        rows[2 * i + 1] = uv[1];
    }
    rows[8] = [g.centre[0], g.centre[1], g.uv_scale[0], g.uv_scale[1]];
    rows[9] = [
        f.global_start,
        1.0 / (f.global_end - f.global_start),
        f.detail_start,
        1.0 / (f.detail_end - f.detail_start),
    ];
    rows[10] = [1.0 / f.detail_size, if g.detail.is_some() { 1.0 } else { 0.0 }, 0.0, 0.0];
    params_buffer(device, [-1.0, -1.0, 0.0, 1.0], material_data, rows)
}

fn params_buffer(
    device: &wgpu::Device,
    alpha: [f32; 4],
    material_data: &[[f32; 4]; 6],
    ground: [[f32; 4]; GROUND_ROWS],
) -> wgpu::Buffer {
    let mut data = [0f32; 28 + 4 * GROUND_ROWS];
    data[..4].copy_from_slice(&alpha);
    for (i, reg) in material_data.iter().enumerate() {
        data[4 + i * 4..8 + i * 4].copy_from_slice(reg);
    }
    for (i, reg) in ground.iter().enumerate() {
        data[28 + i * 4..32 + i * 4].copy_from_slice(reg);
    }
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("material params"),
        size: std::mem::size_of_val(&data) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });
    buf.slice(..).get_mapped_range_mut().copy_from_slice(bytemuck::cast_slice(&data));
    buf.unmap();
    buf
}

/// The cull mode of a blended material's draw. `FUN_00858320` sets `CULLMODE` NONE for a two-sided
/// material (runtime bit `0x80`, on-disk `0x010`) and otherwise the value at
/// `*(PTR_00dfc2fc) + 0x3fd0`, which reads 2 (`D3DCULL_CW`) in the v1.0 capture
/// `updated-chris-exports/20261021_chris-jacobs-flying-winch.dmp`; no instruction writing it was found.
/// Retail applies that default to every one-sided material, opaque ones included, so it is the cull
/// that keeps a model's outward faces: `Back`, in renderers whose front face is the outward one.
pub fn material_cull(material_flags: u16) -> Option<wgpu::Face> {
    if mercs2_formats::texture::runtime_material_flags(material_flags) & 0x80 != 0 {
        None
    } else {
        Some(wgpu::Face::Back)
    }
}

/// The wgpu blend state for a D3D9 one ([`mercs2_formats::texture::material_blend_state`]): the
/// colour blends with the material's op and factors. D3D9 would blend the backbuffer's alpha with the
/// same factors, but retail never displays that alpha, and the targets here are read back with it (the
/// `--render` PNGs, the Workshop portrait), so the destination alpha is kept as it is: blending it
/// would punch see-through holes where glass or a rotor disc draws. A D3D value outside the material
/// tables is an error.
pub fn wgpu_blend(b: mercs2_formats::texture::MaterialBlend) -> Result<wgpu::BlendState, String> {
    let factor = |f: u32| match f {
        2 => Ok(wgpu::BlendFactor::One),
        5 => Ok(wgpu::BlendFactor::SrcAlpha),
        6 => Ok(wgpu::BlendFactor::OneMinusSrcAlpha),
        7 => Ok(wgpu::BlendFactor::DstAlpha),
        other => Err(format!("D3DBLEND {other} is not a material blend factor")),
    };
    let operation = match b.op {
        1 => wgpu::BlendOperation::Add,
        2 => wgpu::BlendOperation::Subtract,
        3 => wgpu::BlendOperation::ReverseSubtract,
        other => return Err(format!("D3DBLENDOP {other} is not a material blend op")),
    };
    let color = wgpu::BlendComponent { src_factor: factor(b.src)?, dst_factor: factor(b.dest)?, operation };
    let alpha = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    Ok(wgpu::BlendState { color, alpha })
}

/// A material's bind group. `views` = diffuse, normal, specular (`_sm`, MTRL slot 1; black = no
/// highlight), then the three further textures only a terrain ground draw reads (`shader.wgsl`
/// bindings 5, 6 and 7).
pub fn make_tex_bind(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    views: [&wgpu::TextureView; 6],
    params: &wgpu::Buffer,
) -> wgpu::BindGroup {
    fn view(binding: u32, v: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::TextureView(v) }
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material bind"),
        layout: bgl,
        entries: &[
            // binding 4 = the material's alpha-test parameters ([`material_params`]).
            wgpu::BindGroupEntry { binding: 4, resource: params.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            view(0, views[0]),
            view(1, views[1]),
            view(3, views[2]),
            view(5, views[3]),
            view(6, views[4]),
            view(7, views[5]),
        ],
    })
}

/// A 1×1 flat tangent-space normal (0,0,1) for the DXT5nm read (X in alpha, Y in green): alpha=128
/// and green=128 give X=Y=0 → Z=1. Linear. Fallback for materials without a normal map.
pub fn make_flat_normal_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("flat normal"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[128u8, 128, 255, 128], // green=128 (Y=0), alpha=128 (X=0) -> flat (0,0,1)
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// A 1×1 black (linear) RGBA texture view — fallback specular/gloss map: a group without an `_sm`
/// map samples black here, so its Blinn-Phong specular term is zeroed out (matte).
pub fn make_black_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("black spec"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm, // linear (spec mask is data, not color)
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[0u8, 0, 0, 0],
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// A 1×1 white RGBA texture view — fallback for groups with no/partial diffuse.
pub fn make_white_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("white"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8, 255, 255, 255],
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

pub fn make_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Staged load-progress counter shared between the background loader and the render thread:
/// the loader calls `step("name")` after each stage; the loading screen reads `fraction()` to
/// fill the plate's progress bar. Adding a stage = one `step` call + bump the `new(N)` total
/// (future: entity placement, PMC spawn setup, act/stage overlays).
pub struct LoadProgress {
    current: std::sync::atomic::AtomicU32,
    total: std::sync::atomic::AtomicU32,
    t0: std::time::Instant,
}

impl LoadProgress {
    pub fn new(total: u32) -> Self {
        LoadProgress {
            current: std::sync::atomic::AtomicU32::new(0),
            total: std::sync::atomic::AtomicU32::new(total.max(1)),
            t0: std::time::Instant::now(),
        }
    }
    /// Mark a named stage complete (call AFTER the stage's work) and log it.
    pub fn step(&self, name: &str) {
        use std::sync::atomic::Ordering;
        let k = self.current.fetch_add(1, Ordering::Relaxed) + 1;
        let n = self.total.load(Ordering::Relaxed);
        println!("[load] stage {k}/{n}: {name} (+{:.1}s)", self.t0.elapsed().as_secs_f64());
    }
    /// Completed fraction 0..1 (the bar's target; the render loop eases toward it).
    pub fn fraction(&self) -> f32 {
        use std::sync::atomic::Ordering;
        self.current.load(Ordering::Relaxed) as f32 / self.total.load(Ordering::Relaxed) as f32
    }
}

/// A decoded animation clip bound to a model's HIER, ready to drive `pose::animate_locals`.
pub struct ClipAnim {
    pub clip: mercs2_formats::anim::AnimClip,
    /// track index -> HIER bone index (None = track's bone absent from this model).
    pub track_to_hier: Vec<Option<usize>>,
    /// number of transform tracks (the rest are float tracks, not bone transforms).
    pub num_transform_tracks: usize,
    pub name_hash: u32,
}

/// One dynamic point light, in the layout the forward shader's uniform array expects.
///
/// Positions/radii are in the SAME world space as the entity `Transform`s the scene renders (native
/// game metres, LH +Y up). The game harvests these from `LightObject` COMPs via
/// [`mercs2_formats::placement::light_inventory`] and hands them to [`crate::scene::Scene::set_lights`];
/// the scene uploads the N nearest to the camera each frame. `std140`-friendly: two `vec4`s = 32 B.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    /// `xyz` = world position, `w` = attenuation radius (metres; ≤0 disables the light).
    pub pos_radius: [f32; 4],
    /// `rgb` = linear color, `w` = intensity scalar.
    pub color_intensity: [f32; 4],
}

impl GpuLight {
    /// Build a point light from a world position, linear color, intensity and radius.
    pub fn point(pos: [f32; 3], color: [f32; 3], intensity: f32, radius: f32) -> Self {
        GpuLight {
            pos_radius: [pos[0], pos[1], pos[2], radius],
            color_intensity: [color[0], color[1], color[2], intensity],
        }
    }
}

/// Maximum lights uploaded per frame — the fixed size of the shader's uniform light array. The scene
/// keeps the full harvested set and uploads the nearest `MAX_LIGHTS` to the camera each frame.
pub const MAX_LIGHTS: usize = 32;

/// A model loaded from the WAD, ready to hand to the scene (GPU) + asset store (CPU).
pub struct LoadedModel {
    pub hash: u32,
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<mesh::DrawGroup>,
    pub textures: TexMap,
    pub skin: mesh::SkinData,
    pub clips: Vec<ClipAnim>,
    /// The model's destruction state machine, if it ships one. Parsed by [`crate::model::Model`] on
    /// load and otherwise discarded — carried here so the runtime can feed
    /// [`crate::destruction::DestructionStore`] without re-opening the WAD. `None` for terrain,
    /// markers and the many props that are simply indestructible.
    pub machine: Option<mercs2_formats::orchestrator::StateMachine>,
    /// HIER nodes, needed alongside `machine` because `SHOW`/`HIDE` walk parent links to flip whole
    /// subtrees. Empty when there is no machine to drive them.
    pub hier: Vec<mercs2_formats::orchestrator::HierNode>,
}
