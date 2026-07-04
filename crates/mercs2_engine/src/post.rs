//! HDR + bloom post-processing chain for the world render path.
//!
//! The scene (sky + geometry) renders into an offscreen **`Rgba16Float` HDR** target instead of the
//! swapchain; this module then runs the game's post stack — bright-pass → separable gaussian blur
//! (ping-pong, a couple of iterations at quarter res) → composite + ACES/Reinhard tone-map — and
//! presents the result to the swapchain. Driven by the `fBloom*` tunables + adaptive-exposure
//! approximation from [`mercs2_formats::atmosphere::Atmosphere`].
//!
//! Robustness: construction is fallible ([`Post::new`] returns `None`). When it returns `None` the
//! caller keeps rendering the scene straight to the swapchain (no HDR, no post), so the world path
//! degrades to the pre-existing forward render rather than breaking.

use mercs2_formats::atmosphere::Atmosphere;

/// HDR internal format. `Rgba16Float` is a render-attachment + filterable-sample format on every
/// wgpu backend we target, so highlights above 1.0 survive to feed the bloom bright-pass.
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Uniform block shared by all three post fragment programs (see post.wgsl `PostU`). 48 bytes.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PostU {
    threshold: f32,
    contrast_mult: f32,
    contrast_limit: f32,
    amount: f32,
    multiplier: f32,
    exposure: f32,
    tonemap_mode: f32,
    _pad0: f32,
    blur_dir: [f32; 2],
    texel: [f32; 2],
}

/// The per-pass bind groups. Rebuilt whenever the targets are (re)created, because they capture
/// texture views. `u_*` are the group-0 uniform binds (stable buffers); the rest are group-1
/// texture binds.
struct Binds {
    u_main: wgpu::BindGroup,
    u_blur_h: wgpu::BindGroup,
    u_blur_v: wgpu::BindGroup,
    bright_tex: wgpu::BindGroup,    // hdr, hdr
    blur_h_tex: wgpu::BindGroup,    // bloom_a, bloom_a
    blur_v_tex: wgpu::BindGroup,    // bloom_b, bloom_b
    composite_tex: wgpu::BindGroup, // hdr, bloom_a
}

struct Targets {
    _hdr_tex: wgpu::Texture,
    hdr_view: wgpu::TextureView,
    _bloom_a: wgpu::Texture,
    bloom_a_view: wgpu::TextureView,
    _bloom_b: wgpu::Texture,
    bloom_b_view: wgpu::TextureView,
    bloom_size: (u32, u32),
}

pub struct Post {
    targets: Targets,
    binds: Binds,

    sampler: wgpu::Sampler,
    postu_bgl: wgpu::BindGroupLayout,
    tex2_bgl: wgpu::BindGroupLayout,

    pipe_bright: wgpu::RenderPipeline,
    pipe_blur: wgpu::RenderPipeline,
    pipe_composite: wgpu::RenderPipeline,

    u_main: wgpu::Buffer,
    u_blur_h: wgpu::Buffer,
    u_blur_v: wgpu::Buffer,
}

fn make_target(device: &wgpu::Device, w: u32, h: u32, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

fn make_targets(device: &wgpu::Device, w: u32, h: u32) -> Targets {
    let bw = (w / 4).max(1);
    let bh = (h / 4).max(1);
    let (hdr, hdr_view) = make_target(device, w, h, "hdr color");
    let (ba, ba_view) = make_target(device, bw, bh, "bloom a");
    let (bb, bb_view) = make_target(device, bw, bh, "bloom b");
    Targets {
        _hdr_tex: hdr,
        hdr_view,
        _bloom_a: ba,
        bloom_a_view: ba_view,
        _bloom_b: bb,
        bloom_b_view: bb_view,
        bloom_size: (bw, bh),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_binds(
    device: &wgpu::Device,
    postu_bgl: &wgpu::BindGroupLayout,
    tex2_bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    t: &Targets,
    u_main: &wgpu::Buffer,
    u_blur_h: &wgpu::Buffer,
    u_blur_v: &wgpu::Buffer,
) -> Binds {
    let u_bind = |buf: &wgpu::Buffer| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post u bind"),
            layout: postu_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        })
    };
    let tex_bind = |a: &wgpu::TextureView, b: &wgpu::TextureView, label: &str| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: tex2_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(a) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(b) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    };
    Binds {
        u_main: u_bind(u_main),
        u_blur_h: u_bind(u_blur_h),
        u_blur_v: u_bind(u_blur_v),
        bright_tex: tex_bind(&t.hdr_view, &t.hdr_view, "bright tex bind"),
        blur_h_tex: tex_bind(&t.bloom_a_view, &t.bloom_a_view, "blur h tex bind"),
        blur_v_tex: tex_bind(&t.bloom_b_view, &t.bloom_b_view, "blur v tex bind"),
        composite_tex: tex_bind(&t.hdr_view, &t.bloom_a_view, "composite tex bind"),
    }
}

impl Post {
    /// Build the HDR target + bloom chain sized to `(w,h)` presenting to `swap_format`. Returns
    /// `None` if the surface is degenerate so the caller can fall back to direct presentation.
    pub fn new(device: &wgpu::Device, swap_format: wgpu::TextureFormat, w: u32, h: u32) -> Option<Post> {
        if w == 0 || h == 0 {
            return None;
        }
        let targets = make_targets(device, w, h);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let postu_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post uniform bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let tex2_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post tex2 bgl"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("post.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[&postu_bgl, &tex2_bgl],
            push_constant_ranges: &[],
        });
        let make_pipe = |entry: &str, fmt: wgpu::TextureFormat, label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: entry,
                    targets: &[Some(wgpu::ColorTargetState {
                        format: fmt,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            })
        };
        let pipe_bright = make_pipe("fs_bright", HDR_FORMAT, "post bright");
        let pipe_blur = make_pipe("fs_blur", HDR_FORMAT, "post blur");
        let pipe_composite = make_pipe("fs_composite", swap_format, "post composite");

        let mk_u = |label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<PostU>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let u_main = mk_u("post u_main");
        let u_blur_h = mk_u("post u_blur_h");
        let u_blur_v = mk_u("post u_blur_v");

        let binds = build_binds(device, &postu_bgl, &tex2_bgl, &sampler, &targets, &u_main, &u_blur_h, &u_blur_v);

        Some(Post {
            targets,
            binds,
            sampler,
            postu_bgl,
            tex2_bgl,
            pipe_bright,
            pipe_blur,
            pipe_composite,
            u_main,
            u_blur_h,
            u_blur_v,
        })
    }

    /// The HDR view the caller renders the scene (sky + geometry) into.
    pub fn hdr_view(&self) -> &wgpu::TextureView {
        &self.targets.hdr_view
    }

    /// Recreate the HDR + bloom targets for a new surface size.
    pub fn resize(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.targets = make_targets(device, w, h);
        self.binds = build_binds(
            device,
            &self.postu_bgl,
            &self.tex2_bgl,
            &self.sampler,
            &self.targets,
            &self.u_main,
            &self.u_blur_h,
            &self.u_blur_v,
        );
    }

    /// Upload this frame's post parameters from the atmosphere.
    pub fn update(&self, queue: &wgpu::Queue, atmo: &Atmosphere) {
        let b = &atmo.bloom;
        let main = PostU {
            threshold: b.threshold,
            contrast_mult: b.contrast_multiplier,
            contrast_limit: b.contrast_limit,
            amount: b.amount,
            multiplier: b.multiplier,
            exposure: atmo.exposure(),
            tonemap_mode: 0.0, // ACES
            _pad0: 0.0,
            blur_dir: [0.0, 0.0],
            texel: [0.0, 0.0],
        };
        // Blur step = fBloomBlurRadius scaled by one bloom-texel, clamped so a wide radius can't run
        // off the small buffer.
        let (bw, bh) = self.targets.bloom_size;
        let r = b.blur_radius.clamp(0.1, 4.0);
        let tx = r / bw as f32;
        let ty = r / bh as f32;
        let blur_h = PostU { blur_dir: [1.0, 0.0], texel: [tx, ty], ..main };
        let blur_v = PostU { blur_dir: [0.0, 1.0], texel: [tx, ty], ..main };
        queue.write_buffer(&self.u_main, 0, bytemuck::bytes_of(&main));
        queue.write_buffer(&self.u_blur_h, 0, bytemuck::bytes_of(&blur_h));
        queue.write_buffer(&self.u_blur_v, 0, bytemuck::bytes_of(&blur_v));
    }

    /// Run bright-pass → blur (2 iterations) → composite, presenting into `swap_view`. Assumes the
    /// scene has already been rendered into [`Post::hdr_view`].
    pub fn run(&self, encoder: &mut wgpu::CommandEncoder, swap_view: &wgpu::TextureView) {
        // Bright-pass: hdr -> bloom_a.
        self.blit(encoder, &self.targets.bloom_a_view, &self.pipe_bright, &self.binds.u_main, &self.binds.bright_tex, "post bright");
        // Two blur iterations (H then V each), ping-ponging bloom_a <-> bloom_b.
        for _ in 0..2 {
            self.blit(encoder, &self.targets.bloom_b_view, &self.pipe_blur, &self.binds.u_blur_h, &self.binds.blur_h_tex, "post blur h");
            self.blit(encoder, &self.targets.bloom_a_view, &self.pipe_blur, &self.binds.u_blur_v, &self.binds.blur_v_tex, "post blur v");
        }
        // Composite + tonemap: hdr + bloom_a -> swapchain.
        self.blit(encoder, swap_view, &self.pipe_composite, &self.binds.u_main, &self.binds.composite_tex, "post composite");
    }

    fn blit(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        pipe: &wgpu::RenderPipeline,
        u_bind: &wgpu::BindGroup,
        tex_bind: &wgpu::BindGroup,
        label: &str,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, u_bind, &[]);
        pass.set_bind_group(1, tex_bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
