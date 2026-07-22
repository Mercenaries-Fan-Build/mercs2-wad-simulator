//! Headless offscreen render of a skinned mesh to a PNG — a self-service visual check for the
//! retarget/rebind (no window, no surface; renders straight to a texture and reads it back). Flat
//! N·L shading is enough to judge silhouette, pose and left/right placement. Mirrors the engine's
//! final clip-X flip so left/right match what the interactive workshop shows on screen.

use glam::{Mat4, Vec3};

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
}

const WGSL: &str = r#"
struct Cam { mvp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> cam: Cam;
@group(1) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;
@group(2) @binding(0) var t_diffuse: texture_2d<f32>;
@group(2) @binding(1) var s_diffuse: sampler;
struct VIn {
  @location(0) pos: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) joints: vec4<u32>,
  @location(3) weights: vec4<f32>,
  @location(4) uv: vec2<f32>,
};
struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) n: vec3<f32>,
  @location(1) uv: vec2<f32>,
};
@vertex fn vs(in: VIn) -> VOut {
  var wsum = in.weights.x + in.weights.y + in.weights.z + in.weights.w;
  if (wsum <= 0.0) { wsum = 1.0; }
  var js = array<u32,4>(in.joints.x, in.joints.y, in.joints.z, in.joints.w);
  var ws = array<f32,4>(in.weights.x, in.weights.y, in.weights.z, in.weights.w);
  var p = vec4<f32>(0.0);
  var nn = vec3<f32>(0.0);
  for (var k = 0; k < 4; k = k + 1) {
    let w = ws[k] / wsum;
    if (w <= 0.0) { continue; }
    let m = bones[js[k]];
    p += w * (m * vec4<f32>(in.pos, 1.0));
    nn += w * (mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz) * in.normal);
  }
  var o: VOut;
  o.clip = cam.mvp * vec4<f32>(p.xyz, 1.0);
  o.n = nn;
  o.uv = in.uv;
  return o;
}
@fragment fn fs(in: VOut) -> @location(0) vec4<f32> {
  let n = normalize(in.n);
  let l1 = max(dot(n, normalize(vec3<f32>(0.4, 0.7, 0.6))), 0.0);
  let l2 = max(dot(n, normalize(vec3<f32>(-0.5, 0.2, -0.7))), 0.0) * 0.4;
  let d = clamp(l1 + l2 + 0.22, 0.0, 1.0);
  // Untextured ranges bind a 1x1 white texel, so this one path serves both and there is no second
  // pipeline to keep in step.
  let albedo = textureSample(t_diffuse, s_diffuse, in.uv).rgb;
  return vec4<f32>(albedo * d, 1.0);
}
"#;

/// Render `verts`/`indices` with the given `palette` (row-major skin matrices) from several angles;
/// write one PNG per angle as `<out_prefix>_<angle>.png`. `bbox` frames the camera.
pub fn render(
    verts: &[SV],
    indices: &[u32],
    palette: &[[[f32; 4]; 4]],
    bbox: ([f32; 3], [f32; 3]),
    out_prefix: &str,
    draws: &[DrawTex],
) {
    let (w, h) = (720u32, 960u32);
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("no gpu adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: wgpu::Limits::default() },
        None,
    ))
    .expect("device");

    let vbuf = create_init(&device, wgpu::BufferUsages::VERTEX, bytemuck::cast_slice(verts));
    let ibuf = create_init(&device, wgpu::BufferUsages::INDEX, bytemuck::cast_slice(indices));
    let pal_flat: Vec<f32> = palette.iter().flat_map(|m| m.iter().flat_map(|r| r.iter().copied())).collect();
    let pbuf = create_init(&device, wgpu::BufferUsages::STORAGE, bytemuck::cast_slice(&pal_flat));
    let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None, size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(WGSL.into()) });
    let cam_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
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
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // Upload each range's diffuse once. A 1x1 white texel stands in for an untextured range so the
    // shader has a single path.
    let upload = |w: u32, h: u32, rgba: &[u8]| -> wgpu::BindGroup {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Unorm, not Srgb: the colour target is Rgba8Unorm, so an sRGB view would linearise on
            // read with nothing converting back and every texture would render dark.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = tex.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &tex_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        })
    };
    let white = upload(1, 1, &[255u8, 255, 255, 255]);
    let tex_binds: Vec<Option<wgpu::BindGroup>> = draws
        .iter()
        .map(|d| d.diffuse.as_ref().map(|(w, h, rgba)| upload(*w, *h, rgba)))
        .collect();

    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&cam_bgl, &bone_bgl, &tex_bgl], push_constant_ranges: &[] });

    let vlayout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<SV>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 1 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32x4, offset: 32, shader_location: 2 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 3 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 64, shader_location: 4 },
        ],
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pl),
        vertex: wgpu::VertexState { module: &shader, entry_point: "vs", buffers: &[vlayout], compilation_options: Default::default() },
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs", targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())], compilation_options: Default::default() }),
        multiview: None,
    });

    let color = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
    let color_v = color.create_view(&Default::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
    let depth_v = depth.create_view(&Default::default());

    let center = Vec3::from((Vec3::from(bbox.0) + Vec3::from(bbox.1)) * 0.5);
    let radius = ((Vec3::from(bbox.1) - Vec3::from(bbox.0)).length() * 0.5).max(0.1);
    // Engine mirrors clip X on screen (LH asset space -> wgpu). Match it so left/right read the same.
    let flip_x = Mat4::from_cols_array(&[-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    let proj = Mat4::perspective_rh(45f32.to_radians(), w as f32 / h as f32, 0.05, radius * 12.0);
    let angles: [(&str, Vec3); 3] = [
        ("front", Vec3::new(0.0, 0.15, 1.0)),
        ("side", Vec3::new(1.0, 0.15, 0.15)),
        ("threeq", Vec3::new(0.8, 0.2, 0.8)),
    ];
    // Row-bytes must be aligned to 256 for the readback copy.
    let bpr = ((w * 4 + 255) / 256) * 256;
    let read_buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

    for (name, dir) in angles {
        let eye = center + dir.normalize() * radius * 3.0;
        let view = Mat4::look_at_rh(eye, center, Vec3::Y);
        let mvp = flip_x * proj * view;
        queue.write_buffer(&cam_buf, 0, bytemuck::cast_slice(&mvp.to_cols_array()));

        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &color_v, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.12, a: 1.0 }), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth_v, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                ..Default::default()
            });
            rp.set_pipeline(&pipeline);
            rp.set_bind_group(0, &cam_bind, &[]);
            rp.set_bind_group(1, &bone_bind, &[]);
            rp.set_vertex_buffer(0, vbuf.slice(..));
            rp.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
            for (i, d) in draws.iter().enumerate() {
                rp.set_bind_group(2, tex_binds[i].as_ref().unwrap_or(&white), &[]);
                let end = (d.index_start + d.index_count).min(indices.len() as u32);
                if end > d.index_start {
                    rp.draw_indexed(d.index_start..end, 0, 0..1);
                }
            }
        }
        enc.copy_texture_to_buffer(
            wgpu::ImageCopyTexture { texture: &color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::ImageCopyBuffer { buffer: &read_buf, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([enc.finish()]);

        let slice = read_buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        // Un-pad rows.
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h {
            let s = (row * bpr) as usize;
            rgba.extend_from_slice(&data[s..s + (w * 4) as usize]);
        }
        drop(data);
        read_buf.unmap();

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
