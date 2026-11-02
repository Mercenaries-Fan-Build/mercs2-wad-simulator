//! Headless offscreen render of a skinned mesh to a PNG — a self-service visual check for the
//! retarget/rebind (no window, no surface; renders straight to a texture and reads it back). Flat
//! N·L shading is enough to judge silhouette, pose and left/right placement. Mirrors the engine's
//! final clip-X flip so left/right match what the interactive workshop shows on screen.
//!
//! Two surfaces:
//! - [`render`] — the one-shot three-angle PNG writer used by the CLI probes in `main.rs`. Builds
//!   a fresh `wgpu::Device` per call, renders front/side/threeq, drops everything.
//! - [`HeroRenderer`] — a stateful renderer the Saves panel keeps across frames: the device +
//!   pipeline + mesh/texture uploads are paid once at `new`, then every orbit tick just rewrites
//!   the camera uniform and reads back the colour buffer. Lets the Workshop drag-orbit the hero at
//!   ~30–60 Hz without rebuilding the whole GPU pipeline each tick. (Not drawing INTO the egui
//!   render pass yet — that's the follow-up; this one still round-trips through CPU + a new
//!   `TextureHandle::set` so the pipeline stays single-threaded against the egui context.)

use std::collections::HashMap;

use glam::{Mat4, Vec3};

/// A spherical-coordinates orbit camera (yaw/pitch around the bbox centre + a radius multiplier).
/// Lives in the Saves panel and is driven by mouse drag + scroll over the hero viewport; passed
/// into [`HeroRenderer::render_rgba`] on every re-render.
///
/// Defaults frame the hero from front-centre, pitched slightly down toward the model's chest —
/// matches the "front" angle the one-shot [`render`] used before orbit existed, so a cold start
/// shows the same framing a static portrait used to.
#[derive(Clone, Copy, Debug)]
pub struct OrbitCam {
    /// Horizontal rotation around the model's up-axis, radians. 0 = facing the hero's front.
    pub yaw: f32,
    /// Vertical tilt in radians, clamped by the caller to roughly `[-π/2 + ε, π/2 − ε]` to avoid
    /// the look-at singularity at the poles. Positive = looking down at the model from above.
    pub pitch: f32,
    /// Distance multiplier against the mesh's bounding-sphere radius. 1.0 = the default
    /// 3× radius the one-shot `render` used; drag-scroll pushes this into `[0.3, 6.0]`.
    pub zoom: f32,
}

impl Default for OrbitCam {
    fn default() -> Self {
        // yaw=0 looks at the hero's front. pitch=0.15 rad (~8.5°) matches the static-portrait's
        // `dir = (0.0, 0.15, 1.0).normalize()` — a modest downward tilt that reads the face.
        OrbitCam { yaw: 0.0, pitch: 0.15, zoom: 1.0 }
    }
}

impl OrbitCam {
    /// Convert to a world-space `eye` position orbiting `center` at `base_radius * 3.0 * zoom`.
    /// The 3× multiplier is the same framing margin the one-shot `render` baked in — kept here so
    /// `OrbitCam::default` with `base_radius = model.radius` lands on the old framing exactly.
    pub fn eye_for(&self, center: Vec3, base_radius: f32) -> Vec3 {
        let dist = (base_radius * 3.0 * self.zoom).max(0.01);
        // Standard spherical→cartesian with yaw around Y, pitch above the XZ plane. Signs chosen
        // so (yaw=0, pitch=0) looks along +Z (model's front, matching the engine's convention).
        let cy = self.pitch.cos();
        let dir = Vec3::new(cy * self.yaw.sin(), self.pitch.sin(), cy * self.yaw.cos());
        center + dir * dist
    }
}

/// Frame size of the one-shot [`render`] PNGs and the size a [`HeroRenderer`] starts at; the
/// Saves viewport resizes its renderer to the on-screen frame via [`HeroRenderer::render_rgba`].
pub const RENDER_W: u32 = 720;
pub const RENDER_H: u32 = 960;

/// One vertex for the shot shader: position, normal, UV, bone indices + weights (LBS, same
/// convention as the engine shader — `bones[j] * v`, palette uploaded row-major).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SV {
    pub pos: [f32; 3],
    pub _p0: f32,
    pub normal: [f32; 3],
    pub _p1: f32,
    pub joints: [u32; 4],
    pub weights: [f32; 4],
    pub uv: [f32; 2],
    pub _p2: [f32; 2],
    pub tangent: [f32; 4],
    /// The vertex COLOR, RGBA (`mesh::Vertex::color`); its alpha feeds the `*AmbOcc*` output alpha.
    pub color: [f32; 4],
}

/// One draw range and the diffuse it binds, already decoded to RGBA8.
///
/// The caller decodes rather than this module, so `shot` stays independent of the texture container
/// and the WAD: `texpng::decode_bc` already resolves the resident-mip-tail case, and duplicating
/// that here is how the two would drift. `None` renders the range with flat shading, which is what
/// an untextured group (or a missing texture) should look like — visibly plain rather than silently
/// white.
pub struct DrawTex {
    pub index_start: u32,
    pub index_count: u32,
    /// `(width, height, rgba8)`.
    pub diffuse: Option<(u32, u32, Vec<u8>)>,
    /// The `_nm` normal map, decoded to RGBA8. DXT5nm: X in ALPHA, Y in GREEN. `None` = flat.
    pub normal: Option<(u32, u32, Vec<u8>)>,
    /// Material slot 1 decoded to RGBA8: the `_sm` specular mask or the emissive map, as `slot1_role`
    /// says (`mercs2_engine::render::material_slots`). `None` = black.
    pub slot1: Option<(u32, u32, Vec<u8>)>,
    pub slot1_role: mercs2_engine::render::Slot1Role,
    /// The material's on-disk flags and `materialData` (`DrawGroup::material_flags` /
    /// `material_data`): the alpha test, the diffuse/ambient scales and the emissive colour, exactly as
    /// the engine's draw uses them. 0 / `mesh::NO_MATERIAL_DATA` for a draw with no material.
    pub material_flags: u16,
    pub material_data: [[f32; 4]; 6],
    /// A blended material's output alpha (`MaterialSlots::output_alpha`); `None` draws opaque.
    pub output_alpha: Option<mercs2_formats::shader_registrations::OutputAlpha>,
}

const WGSL: &str = r#"
struct Cam { mvp: mat4x4<f32>, cam_pos: vec4<f32> };
@group(0) @binding(0) var<uniform> cam: Cam;
@group(1) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;
@group(2) @binding(0) var t_diffuse: texture_2d<f32>;
@group(2) @binding(1) var s_diffuse: sampler;
@group(2) @binding(2) var t_normal: texture_2d<f32>;
@group(2) @binding(3) var t_specular: texture_2d<f32>;
// The material, as shader.wgsl: alpha.x = alpha reference (or -1 = not tested), alpha.y = a blended
// material's output alpha code (or -1 = opaque), alpha.z = slot 1's role (0 nothing, 1 specular,
// 2 emissive), data = materialData c93..c98.
struct MaterialParams { alpha: vec4<f32>, data: array<vec4<f32>, 6> };
@group(2) @binding(4) var<uniform> material: MaterialParams;
struct VIn {
  @location(0) pos: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) joints: vec4<u32>,
  @location(3) weights: vec4<f32>,
  @location(4) uv: vec2<f32>,
  @location(5) tangent: vec4<f32>,
  @location(6) color: vec4<f32>,
};
struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) n: vec3<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) t: vec3<f32>,
  @location(3) b: vec3<f32>,
  @location(4) wpos: vec3<f32>,
  @location(5) color: vec4<f32>,
};
@vertex fn vs(in: VIn) -> VOut {
  var wsum = in.weights.x + in.weights.y + in.weights.z + in.weights.w;
  if (wsum <= 0.0) { wsum = 1.0; }
  var js = array<u32,4>(in.joints.x, in.joints.y, in.joints.z, in.joints.w);
  var ws = array<f32,4>(in.weights.x, in.weights.y, in.weights.z, in.weights.w);
  var p = vec4<f32>(0.0);
  var nn = vec3<f32>(0.0);
  var tt = vec3<f32>(0.0);
  for (var k = 0; k < 4; k = k + 1) {
    let w = ws[k] / wsum;
    if (w <= 0.0) { continue; }
    let m = bones[js[k]];
    let m3 = mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz);
    p += w * (m * vec4<f32>(in.pos, 1.0));
    nn += w * (m3 * in.normal);
    tt += w * (m3 * in.tangent.xyz);
  }
  var o: VOut;
  o.clip = cam.mvp * vec4<f32>(p.xyz, 1.0);
  o.n = nn;
  o.t = tt;
  // bitangent handedness from tangent.w, exactly as the engine shader does
  o.b = cross(nn, tt) * in.tangent.w;
  o.uv = in.uv;
  o.wpos = p.xyz;
  o.color = in.color;
  return o;
}
@fragment fn fs(in: VOut) -> @location(0) vec4<f32> {
  // DXT5nm decode + tangent-space perturbation, byte-for-byte what shader.wgsl does. A 1x1 flat
  // texel (X=0, Y=0) binds where a group has no _nm, so this one path serves both.
  let nsamp = textureSample(t_normal, s_diffuse, in.uv);
  let nx = nsamp.a * 2.0 - 1.0;
  let ny = nsamp.g * 2.0 - 1.0;
  let nz = sqrt(max(1.0 - nx * nx - ny * ny, 0.0));
  let tbn = mat3x3<f32>(normalize(in.t), normalize(in.b), normalize(in.n));
  let N = normalize(tbn * vec3<f32>(nx, ny, nz));
  let dtex = textureSample(t_diffuse, s_diffuse, in.uv);
  if (material.alpha.x >= 0.0 && !(dtex.a * material.data[5].w > material.alpha.x)) { discard; }
  let albedo = dtex.rgb;

  // Lighting matched to shader.wgsl: an ambient floor plus a single
  // sun key light, and Blinn-Phong specular masked by the _sm map. The exterior sun values are
  // baked in here (sun_i ~1, ambient ~0.35) so a character previews as it does outdoors, which is
  // where the arm's harsh specular shows. This is the term that was missing while the arms looked
  // fine in preview and broken in game.
  let sun_dir = normalize(vec3<f32>(0.4, 0.7, -0.5));
  let sun_i = 1.0;
  let ambient = 0.35;
  let spec_power = 48.0;
  let ndl = max(dot(N, sun_dir), 0.0);
  // Slot 1 is the specular mask or the emissive map; materialData c93/c95 scale the direct and ambient
  // light and the emissive map adds `× c96` after lighting, as shader.wgsl does.
  let slot1 = textureSample(t_specular, s_diffuse, in.uv).rgb;
  var spec_mask = vec3<f32>(0.0);
  var emissive = vec3<f32>(0.0);
  if (material.alpha.z > 1.5) {
    emissive = slot1 * material.data[3].rgb;
  } else if (material.alpha.z > 0.5) {
    spec_mask = slot1;
  }
  let V = normalize(cam.cam_pos.xyz - in.wpos);
  var col = albedo * material.data[2].rgb * ambient + albedo * material.data[0].rgb * (sun_i * ndl);
  var spec_light = vec3<f32>(0.0);
  if (ndl > 0.0) {
    let H = normalize(sun_dir + V);
    spec_light = spec_mask * pow(max(dot(N, H), 0.0), spec_power);
  }
  // A blended material's output alpha, as shader.wgsl computes it (`OutputAlpha`).
  var alpha = 1.0;
  if (material.alpha.y >= 0.0) {
    let code = u32(material.alpha.y + 0.5);
    alpha = material.data[5].w;
    if (code >= 1u) { alpha = alpha * dtex.a; }
    if (code == 2u || code == 4u) { alpha = alpha * in.color.a; }
    if (code >= 3u) {
      alpha = clamp(max(alpha, dot(spec_light, vec3<f32>(0.299, 0.587, 0.114))), 0.0, 1.0);
    }
  }
  return vec4<f32>(col + spec_light + emissive, alpha);
}
"#;

/// A blended draw's pipeline key: the material's D3D blend state and cull mode.
type BlendKey = (mercs2_formats::texture::MaterialBlend, Option<wgpu::Face>);

/// One draw: its index range, its bind (the `DrawTex` it came from) and, for a blended material,
/// its pipeline key.
struct ShotDraw {
    start: u32,
    count: u32,
    bind: usize,
    blend: Option<BlendKey>,
}

/// A stateful offscreen hero renderer. One `wgpu::Device` + pipeline + mesh/texture upload per
/// instance, kept across frames so the Workshop's orbit-drag UI pays the ~500 ms setup cost once
/// per hero and ~10–30 ms per subsequent camera tick.
///
/// Call [`HeroRenderer::new`] with the full mesh (verts/indices/palette/bbox/draws); call
/// [`HeroRenderer::render_rgba`] with an [`OrbitCam`] and a pixel size to get back one RGBA frame
/// of that size (the colour/depth targets are rebuilt when the size changes). Dropping the
/// renderer releases every GPU resource.
pub struct HeroRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    blend_pipelines: HashMap<BlendKey, wgpu::RenderPipeline>,
    cam_buf: wgpu::Buffer,
    cam_bind: wgpu::BindGroup,
    bone_bind: wgpu::BindGroup,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    tex_binds: Vec<wgpu::BindGroup>,
    draws: Vec<ShotDraw>,
    color: wgpu::Texture,
    color_v: wgpu::TextureView,
    depth_v: wgpu::TextureView,
    read_buf: wgpu::Buffer,
    bpr: u32,
    size: (u32, u32),
    bbox_center: Vec3,
    bbox_radius: f32,
    proj: Mat4,
    flip_x: Mat4,
}

/// Colour + depth targets, readback buffer, padded row stride and projection for one frame size.
/// Vertical FOV is fixed at 45°, so a wider frame shows more scene sideways at the same framing.
fn size_targets(
    device: &wgpu::Device,
    w: u32,
    h: u32,
    bbox_radius: f32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView, wgpu::Buffer, u32, Mat4) {
    let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let color = device.create_texture(&wgpu::TextureDescriptor { label: None, size: extent, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
    let color_v = color.create_view(&Default::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor { label: None, size: extent, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
    let depth_v = depth.create_view(&Default::default());
    // Row-bytes must be aligned to 256 for the readback copy.
    let bpr = ((w * 4 + 255) / 256) * 256;
    let read_buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let proj = Mat4::perspective_rh(45f32.to_radians(), w as f32 / h as f32, 0.05, bbox_radius * 12.0);
    (color, color_v, depth_v, read_buf, bpr, proj)
}

impl HeroRenderer {
    /// Build the renderer, uploading everything that doesn't change frame-to-frame. The GPU
    /// adapter + device are built with `pollster::block_on` here only; `render_rgba` is also
    /// synchronous but only submits + reads a mapped buffer, no device creation.
    pub fn new(
        verts: &[SV],
        indices: &[u32],
        palette: &[[[f32; 4]; 4]],
        bbox: ([f32; 3], [f32; 3]),
        draws: &[DrawTex],
    ) -> Self {
        let (w, h) = (RENDER_W, RENDER_H);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .expect("no gpu adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .expect("device");

    let vbuf = create_init(&device, wgpu::BufferUsages::VERTEX, bytemuck::cast_slice(verts));
    let ibuf = create_init(&device, wgpu::BufferUsages::INDEX, bytemuck::cast_slice(indices));
    let pal_flat: Vec<f32> = palette.iter().flat_map(|m| m.iter().flat_map(|r| r.iter().copied())).collect();
    let pbuf = create_init(&device, wgpu::BufferUsages::STORAGE, bytemuck::cast_slice(&pal_flat));
    let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None, size: 80, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(WGSL.into()) });
    let cam_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
    });
    let bone_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None }],
    });
    let cam_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &cam_bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: cam_buf.as_entire_binding() }] });
    let bone_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bone_bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: pbuf.as_entire_binding() }] });
    let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // Upload a texture and return its view. Unorm, not Srgb: the colour target is Rgba8Unorm, so an
    // sRGB view would linearise on read with nothing converting back and every texture would render
    // dark. Normal maps in particular MUST be linear-sampled.
    let upload = |w: u32, h: u32, rgba: &[u8]| -> wgpu::TextureView {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        tex.create_view(&Default::default())
    };
    let white = upload(1, 1, &[255u8, 255, 255, 255]);
    // Flat DXT5nm texel: X in alpha, Y in green, both 0.5 -> no perturbation. Bound where a group
    // has no normal map, so the fragment shader has one path.
    let flat_nm = upload(1, 1, &[0u8, 128, 255, 128]);
    let black = upload(1, 1, &[0u8, 0, 0, 255]); // matte fallback where a group has no _sm
    let bind_of = |diff: &wgpu::TextureView, nrm: &wgpu::TextureView, spc: &wgpu::TextureView, d: &DrawTex| -> wgpu::BindGroup {
        let params = mercs2_engine::render::material_params(
            &device, d.material_flags, &d.material_data, d.slot1_role, d.output_alpha,
        );
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &tex_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(diff) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(nrm) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(spc) },
                wgpu::BindGroupEntry { binding: 4, resource: params.as_entire_binding() },
            ],
        })
    };
    let tex_binds: Vec<wgpu::BindGroup> = draws
        .iter()
        .map(|d| {
            let dv = d.diffuse.as_ref().map(|(w, h, r)| upload(*w, *h, r));
            let nv = d.normal.as_ref().map(|(w, h, r)| upload(*w, *h, r));
            let sv = d.slot1.as_ref().map(|(w, h, r)| upload(*w, *h, r));
            bind_of(dv.as_ref().unwrap_or(&white), nv.as_ref().unwrap_or(&flat_nm), sv.as_ref().unwrap_or(&black), d)
        })
        .collect();

    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&cam_bgl), Some(&bone_bgl), Some(&tex_bgl)], immediate_size: 0 });

    let vlayout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<SV>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 1 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32x4, offset: 32, shader_location: 2 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 3 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 64, shader_location: 4 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 80, shader_location: 5 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 96, shader_location: 6 },
        ],
    };
    // The engine's geometry pipelines, as `scene.rs` builds them: opaque draws REPLACE with Z write and
    // no culling; a blended material's draw blends with its own state, Z write off, culled by
    // `render::material_cull`. Outward faces are clockwise on screen under this RH view (the engine's
    // LH one mirrors it): culling `Back` draws a closed model whole, `Front` shows its inside.
    let make_pipeline = |blend: wgpu::BlendState, depth_write: bool, cull: Option<wgpu::Face>| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[Some(vlayout.clone())], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { front_face: wgpu::FrontFace::Cw, cull_mode: cull, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: Some(depth_write), depth_compare: Some(wgpu::CompareFunction::Less), stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let pipeline = make_pipeline(wgpu::BlendState::REPLACE, true, None);
    let draw_list: Vec<ShotDraw> = draws
        .iter()
        .enumerate()
        .map(|(bind, d)| {
            let end = (d.index_start + d.index_count).min(indices.len() as u32);
            let blend = d.output_alpha.map(|_| {
                let rt = mercs2_formats::texture::runtime_material_flags(d.material_flags);
                (mercs2_formats::texture::material_blend_state(rt), mercs2_engine::render::material_cull(d.material_flags))
            });
            ShotDraw { start: d.index_start, count: end.saturating_sub(d.index_start), bind, blend }
        })
        .collect();
    let mut blend_pipelines: HashMap<BlendKey, wgpu::RenderPipeline> = HashMap::new();
    for key in draw_list.iter().filter_map(|d| d.blend) {
        if let std::collections::hash_map::Entry::Vacant(v) = blend_pipelines.entry(key) {
            let state = mercs2_engine::render::wgpu_blend(key.0).unwrap_or_else(|e| panic!("{e}"));
            v.insert(make_pipeline(state, false, key.1));
        }
    }

    let bbox_center = Vec3::from((Vec3::from(bbox.0) + Vec3::from(bbox.1)) * 0.5);
    let bbox_radius = ((Vec3::from(bbox.1) - Vec3::from(bbox.0)).length() * 0.5).max(0.1);
    // Engine mirrors clip X on screen (LH asset space -> wgpu). Match it so left/right read the same.
    let flip_x = Mat4::from_cols_array(&[-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    let (color, color_v, depth_v, read_buf, bpr, proj) = size_targets(&device, w, h, bbox_radius);

    HeroRenderer {
        device,
        queue,
        pipeline,
        blend_pipelines,
        cam_buf,
        cam_bind,
        bone_bind,
        vbuf,
        ibuf,
        tex_binds,
        draws: draw_list,
        color,
        color_v,
        depth_v,
        read_buf,
        bpr,
        size: (w, h),
        bbox_center,
        bbox_radius,
        proj,
        flip_x,
    }
    }

    /// Draw one orbit frame with the given camera at `size` pixels and return `w × h` RGBA8 bytes.
    /// Reuses every GPU resource built in `new`; the colour/depth targets and readback buffer are
    /// rebuilt only when `size` differs from the previous call.
    pub fn render_rgba(&mut self, cam: &OrbitCam, size: (u32, u32)) -> Vec<u8> {
        let (w, h) = (size.0.max(1), size.1.max(1));
        if (w, h) != self.size {
            let (color, color_v, depth_v, read_buf, bpr, proj) =
                size_targets(&self.device, w, h, self.bbox_radius);
            self.color = color;
            self.color_v = color_v;
            self.depth_v = depth_v;
            self.read_buf = read_buf;
            self.bpr = bpr;
            self.proj = proj;
            self.size = (w, h);
        }
        let eye = cam.eye_for(self.bbox_center, self.bbox_radius);
        let view = Mat4::look_at_rh(eye, self.bbox_center, Vec3::Y);
        let mvp = self.flip_x * self.proj * view;
        self.queue.write_buffer(&self.cam_buf, 0, bytemuck::cast_slice(&mvp.to_cols_array()));
        self.queue.write_buffer(&self.cam_buf, 64, bytemuck::cast_slice(&[eye.x, eye.y, eye.z, 1.0f32]));

        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &self.color_v, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.12, a: 1.0 }), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &self.depth_v, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                ..Default::default()
            });
            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, &self.cam_bind, &[]);
            rp.set_bind_group(1, &self.bone_bind, &[]);
            rp.set_vertex_buffer(0, self.vbuf.slice(..));
            rp.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            // Opaque draws, then the blended ones in draw order (`Scene::draw_geometry`).
            for blended in [false, true] {
                for d in self.draws.iter().filter(|d| d.blend.is_some() == blended && d.count > 0) {
                    if let Some(key) = d.blend {
                        rp.set_pipeline(&self.blend_pipelines[&key]);
                    }
                    rp.set_bind_group(2, &self.tex_binds[d.bind], &[]);
                    rp.draw_indexed(d.start..(d.start + d.count), 0, 0..1);
                }
            }
        }
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &self.read_buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.bpr), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);

        let slice = self.read_buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("wait for the readback");
        let data = slice.get_mapped_range().expect("the readback is mapped");
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h {
            let s = (row * self.bpr) as usize;
            rgba.extend_from_slice(&data[s..s + (w * 4) as usize]);
        }
        drop(data);
        self.read_buf.unmap();
        rgba
    }
}

/// Render `verts`/`indices` with the given `palette` (row-major skin matrices) from several angles;
/// write one PNG per angle as `<out_prefix>_<angle>.png`. `bbox` frames the camera.
///
/// Thin wrapper around [`HeroRenderer`]: builds a renderer, re-renders at three canned
/// yaw/pitch combinations, writes a PNG per frame. The one-shot CLI probes in `main.rs` call this.
pub fn render(
    verts: &[SV],
    indices: &[u32],
    palette: &[[[f32; 4]; 4]],
    bbox: ([f32; 3], [f32; 3]),
    out_prefix: &str,
    draws: &[DrawTex],
) {
    let (w, h) = (RENDER_W, RENDER_H);
    let mut r = HeroRenderer::new(verts, indices, palette, bbox, draws);
    // Match the three directions the pre-refactor `render` used: front / side / three-quarter.
    // Converted to (yaw, pitch) with yaw around Y, pitch above the XZ plane. zoom=1 → the old
    // `radius * 3.0` eye distance.
    let angles: [(&str, OrbitCam); 3] = [
        // dir = (0, 0.15, 1).normalize() → yaw=0, pitch=atan2(0.15, 1)
        ("front",  OrbitCam { yaw: 0.0, pitch: 0.14888995, zoom: 1.0 }),
        // dir = (1, 0.15, 0.15).normalize() → yaw=atan2(1, 0.15), pitch=asin(0.15 / len)
        ("side",   OrbitCam { yaw: std::f32::consts::FRAC_PI_2 - 0.14888995, pitch: 0.14858, zoom: 1.0 }),
        // dir = (0.8, 0.2, 0.8).normalize() → yaw=atan2(0.8, 0.8)=π/4, pitch=asin(0.2 / len)
        ("threeq", OrbitCam { yaw: std::f32::consts::FRAC_PI_4, pitch: 0.17408, zoom: 1.0 }),
    ];
    for (name, cam) in angles {
        let rgba = r.render_rgba(&cam, (w, h));
        let path = format!("{out_prefix}_{name}.png");
        let file = std::fs::File::create(&path).expect("create png");
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
        println!("wrote {path}");
    }
}

fn create_init(device: &wgpu::Device, usage: wgpu::BufferUsages, data: &[u8]) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: data, usage })
}

#[cfg(test)]
mod tests {
    use super::{DrawTex, HeroRenderer, OrbitCam, SV, RENDER_H, RENDER_W};

    fn tri() -> (Vec<SV>, Vec<u32>) {
        let v = |x: f32, y: f32| SV {
            pos: [x, y, 0.0],
            _p0: 0.0,
            normal: [0.0, 0.0, 1.0],
            _p1: 0.0,
            joints: [0; 4],
            weights: [1.0, 0.0, 0.0, 0.0],
            uv: [0.0, 0.0],
            _p2: [0.0; 2],
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: [1.0; 4],
        };
        (vec![v(-1.0, -1.0), v(1.0, -1.0), v(0.0, 1.0)], vec![0, 1, 2])
    }

    fn px(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn hero_renderer_resizes_to_the_requested_frame() {
        let (verts, indices) = tri();
        let ident = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let draws = [DrawTex {
            index_start: 0,
            index_count: 3,
            diffuse: None,
            normal: None,
            slot1: None,
            slot1_role: mercs2_engine::render::Slot1Role::None,
            material_flags: 0,
            material_data: mercs2_engine::mesh::NO_MATERIAL_DATA,
            output_alpha: None,
        }];
        let mut r = HeroRenderer::new(&verts, &indices, &[ident], ([-1.0, -1.0, 0.0], [1.0, 1.0, 0.0]), &draws);
        let cam = OrbitCam { yaw: 0.0, pitch: 0.0, zoom: 1.0 };
        let clear = [26u8, 26, 31, 255];
        // Default size, then a wide frame (odd row stride exercises the 256-byte readback padding).
        for (w, h) in [(RENDER_W, RENDER_H), (1001, 360)] {
            let rgba = r.render_rgba(&cam, (w, h));
            assert_eq!(rgba.len(), (w * h * 4) as usize, "frame {w}x{h}");
            for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
                assert_eq!(px(&rgba, w, x, y), clear, "corner ({x},{y}) of {w}x{h} is the clear colour");
            }
            assert_ne!(px(&rgba, w, w / 2, h / 2), clear, "triangle covers the centre of {w}x{h}");
        }
    }
}
