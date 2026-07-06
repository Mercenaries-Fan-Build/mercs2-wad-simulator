//! 2D UI overlay pass — screen-space quads + monospace text (`ui.wgsl`).
//!
//! ENGINE MECHANISM ONLY (pangea_engine_alignment §6: framework = engine, content = game): the
//! engine knows how to rasterize rects and monospace glyph runs over the swapchain; WHAT the menu
//! says and how it flows is game policy (`mercs2_game::menu`). This is the first brick of the
//! `PgGui`/RedCanvas/RedFont layer — the authentic path (Scaleform `shell.gfx` + the game's own
//! fonts) can replace the glyph source later without changing the call sites.
//!
//! GLYPH SOURCE: at `UiPass` creation a system monospace TTF (`MERCS2_UI_FONT` override, else
//! Cascadia Mono / Consolas / Courier / DejaVu) is rasterized with `fontdue` into an
//! anti-aliased R8 atlas sampled with LINEAR filtering — clean rounded characters at any text
//! scale, fractional included. When no system font exists, the public-domain `font8x8` 8x8
//! bitmap set is baked instead (nearest-sampled), so the overlay always works.
//!
//! Layout is unchanged either way: the atlas is a 16x8 grid of ASCII 0..127 cells and a text run
//! advances `GLYPH * scale` px per character (callers lay out from `Scene::size`). Cell 1
//! (unused control char) is reserved ALL-WHITE so solid rects sample it — one pipeline, one bind
//! group, one instanced draw for the whole overlay. Coordinates are surface pixels, origin
//! top-left.

use wgpu::util::DeviceExt;

/// One staged quad: `rect` px (x,y,w,h), `uv` (u0,v0,u1,v1) into the atlas, straight-alpha color.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiInst {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
}

/// Layout advance per character per unit text scale (the layout unit every caller assumes).
pub const GLYPH: f32 = 8.0;
/// Atlas cell reserved as all-white (ASCII 0x01, never drawn as a glyph) — solid-rect UV target.
const WHITE_CELL: u8 = 0x01;

/// A baked glyph atlas + the metrics `text` needs to place its quads.
struct GlyphAtlas {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    /// Atlas cell size in atlas px (16x8 grid of ASCII cells).
    cell_w: f32,
    cell_h: f32,
    /// Screen-space glyph quad size / placement offsets, per unit text scale. The quad may be
    /// taller than the 8px layout cell (real fonts hang descenders below the baseline).
    quad_w: f32,
    quad_h: f32,
    x_off: f32,
    y_off: f32,
    /// Anti-aliased atlas → linear sampling; 1-bit bitmap atlas → nearest (keeps it crisp).
    smooth: bool,
}

/// The guaranteed fallback: the 8x8 `font8x8` bitmap set, exactly as the original UI pass baked it.
fn build_bitmap_atlas() -> GlyphAtlas {
    const AW: u32 = 16 * 8;
    const AH: u32 = 8 * 8;
    let mut pixels = vec![0u8; (AW * AH) as usize];
    for ch in 0u8..128 {
        let glyph: [u8; 8] = font8x8::legacy::BASIC_LEGACY[ch as usize];
        let (gx, gy) = ((ch as u32 % 16) * 8, (ch as u32 / 16) * 8);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8u32 {
                // font8x8 rows are LSB-left bitmasks.
                let on = if ch == WHITE_CELL { true } else { bits & (1 << col) != 0 };
                pixels[((gy + row as u32) * AW + gx + col) as usize] = if on { 0xFF } else { 0 };
            }
        }
    }
    GlyphAtlas {
        pixels,
        width: AW,
        height: AH,
        cell_w: 8.0,
        cell_h: 8.0,
        quad_w: 8.0,
        quad_h: 8.0,
        x_off: 0.0,
        y_off: 0.0,
        smooth: false,
    }
}

/// Locate a standard monospace TTF: `MERCS2_UI_FONT` (any path) first, then the usual system
/// fonts on Windows and Linux/SteamOS. Returns the file bytes.
fn find_ui_font() -> Option<(std::path::PathBuf, Vec<u8>)> {
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("MERCS2_UI_FONT") {
        cands.push(p.into());
    }
    if let Ok(windir) = std::env::var("WINDIR") {
        let fonts = std::path::Path::new(&windir).join("Fonts");
        for f in ["CascadiaMono.ttf", "consola.ttf", "cour.ttf", "lucon.ttf"] {
            cands.push(fonts.join(f));
        }
    }
    for f in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    ] {
        cands.push(f.into());
    }
    cands
        .into_iter()
        .find_map(|p| std::fs::read(&p).ok().map(|b| (p, b)))
}

/// Rasterize ASCII 33..127 of a system monospace TTF into an anti-aliased 16x8-grid atlas.
/// Returns None (→ bitmap fallback) if no font is found or it fails to parse.
fn build_ttf_atlas() -> Option<GlyphAtlas> {
    // Rasterization size (px per em) — the atlas resolution. Text draws smaller than this at the
    // usual scales, and the linear-filtered downscale is what makes the glyphs read cleanly.
    const EM: f32 = 40.0;
    let (path, bytes) = find_ui_font()?;
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()?;
    let lm = font.horizontal_line_metrics(EM)?;
    let ascent = lm.ascent.ceil() + 1.0; // + top pad px
    let descent = (-lm.descent).ceil() + 2.0; // + AA pad px
    let cell_h = (ascent + descent) as u32;
    let adv = font.metrics('M', EM).advance_width;
    let cell_w = adv.ceil() as u32 + 2;
    let (aw, ah) = (16 * cell_w, 8 * cell_h);
    let mut pixels = vec![0u8; (aw * ah) as usize];
    for ch in 0u8..128 {
        let (gx, gy) = ((ch as u32 % 16) * cell_w, (ch as u32 / 16) * cell_h);
        if ch == WHITE_CELL {
            for r in 0..cell_h {
                for c in 0..cell_w {
                    pixels[((gy + r) * aw + gx + c) as usize] = 0xFF;
                }
            }
            continue;
        }
        if ch < 33 {
            continue; // controls + space stay empty
        }
        let (m, cov) = font.rasterize(ch as char, EM);
        let x0 = 1 + m.xmin.max(0);
        let y0 = ascent as i32 - m.ymin - m.height as i32;
        for r in 0..m.height {
            let ay = y0 + r as i32;
            if ay < 0 || ay >= cell_h as i32 {
                continue;
            }
            for c in 0..m.width {
                let ax = x0 + c as i32;
                if ax < 0 || ax >= cell_w as i32 {
                    continue;
                }
                let dst = &mut pixels[((gy + ay as u32) * aw + gx + ax as u32) as usize];
                *dst = (*dst).max(cov[r * m.width + c]);
            }
        }
    }
    // Screen metrics per unit text scale: the font's ascent maps to 8.6px so cap height lands
    // where the 8x8 bitmap's caps did (callers assume ~8px visual height per unit scale); the
    // baseline sits at 7.6px, descenders hang ~2px below the layout cell into the row gap.
    let k = 8.6 / ascent;
    eprintln!("[ui] glyph atlas: {} ({}x{} px cells @ {EM}px em)", path.display(), cell_w, cell_h);
    Some(GlyphAtlas {
        pixels,
        width: aw,
        height: ah,
        cell_w: cell_w as f32,
        cell_h: cell_h as f32,
        quad_w: cell_w as f32 * k,
        quad_h: cell_h as f32 * k,
        x_off: (GLYPH - adv * k) * 0.5,
        y_off: 7.6 - ascent * k,
        smooth: true,
    })
}

pub struct UiPass {
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    screen_buf: wgpu::Buffer,
    inst_buf: wgpu::Buffer,
    inst_cap: usize,
    /// Quads staged this frame (drained by `flush`). Public staging happens via `rect`/`text`.
    staged: Vec<UiInst>,
    // Glyph-atlas geometry (see `GlyphAtlas`).
    cell_w: f32,
    cell_h: f32,
    atlas_w: f32,
    atlas_h: f32,
    quad_w: f32,
    quad_h: f32,
    x_off: f32,
    y_off: f32,
}

impl UiPass {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> UiPass {
        // ── Bake the glyph atlas: system monospace TTF, else the 8x8 bitmap fallback. ──
        let ga = build_ttf_atlas().unwrap_or_else(build_bitmap_atlas);
        let atlas = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("ui font atlas"),
                size: wgpu::Extent3d { width: ga.width, height: ga.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &ga.pixels,
        );
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        // AA atlas → linear (smooth downscale); bitmap atlas → nearest (crisp at integer scales).
        let filter = if ga.smooth { wgpu::FilterMode::Linear } else { wgpu::FilterMode::Nearest };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui sampler"),
            mag_filter: filter,
            min_filter: filter,
            ..Default::default()
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let screen_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui screen uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui bind"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: screen_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui pipeline layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiInst>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            // 4-vertex strip per instance (see ui.wgsl vs_main).
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            // Overlay: drawn inside a pass which has a depth attachment — keep the depth state
            // present but never test/write (the overlay always wins).
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::render::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let inst_cap = 1024;
        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui instances"),
            size: (inst_cap * std::mem::size_of::<UiInst>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        UiPass {
            pipeline,
            bind,
            screen_buf,
            inst_buf,
            inst_cap,
            staged: Vec::new(),
            cell_w: ga.cell_w,
            cell_h: ga.cell_h,
            atlas_w: ga.width as f32,
            atlas_h: ga.height as f32,
            quad_w: ga.quad_w,
            quad_h: ga.quad_h,
            x_off: ga.x_off,
            y_off: ga.y_off,
        }
    }

    /// Stage a solid rect (surface px, origin top-left, straight-alpha color).
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        // Sample the centre of the reserved all-white cell.
        let gx = (WHITE_CELL as f32 % 16.0) * self.cell_w;
        let gy = (WHITE_CELL / 16) as f32 * self.cell_h;
        let u = (gx + self.cell_w * 0.5) / self.atlas_w;
        let v = (gy + self.cell_h * 0.5) / self.atlas_h;
        self.staged.push(UiInst { rect: [x, y, w, h], uv: [u, v, u, v], color });
    }

    /// Stage a monospace text run at `scale`x the 8px layout cell (advance = 8*scale px per
    /// character). Non-ASCII bytes render as '?'. Returns the run's pixel width. The glyph quads
    /// themselves may extend slightly above/below the layout cell (real-font descenders).
    pub fn text(&mut self, x: f32, y: f32, scale: f32, color: [f32; 4], s: &str) -> f32 {
        let cell = GLYPH * scale;
        let mut cx = x;
        for b in s.bytes() {
            let ch = if b < 128 && b != WHITE_CELL { b } else { b'?' };
            if ch != b' ' {
                let gx = (ch as f32 % 16.0) * self.cell_w;
                let gy = (ch / 16) as f32 * self.cell_h;
                self.staged.push(UiInst {
                    rect: [
                        cx + self.x_off * scale,
                        y + self.y_off * scale,
                        self.quad_w * scale,
                        self.quad_h * scale,
                    ],
                    uv: [
                        gx / self.atlas_w,
                        gy / self.atlas_h,
                        (gx + self.cell_w) / self.atlas_w,
                        (gy + self.cell_h) / self.atlas_h,
                    ],
                    color,
                });
            }
            cx += cell;
        }
        cx - x
    }

    /// Pixel width of `s` at `scale` (monospace: len * 8 * scale) — for centring, without staging.
    pub fn text_width(s: &str, scale: f32) -> f32 {
        s.len() as f32 * GLYPH * scale
    }

    pub fn has_staged(&self) -> bool {
        !self.staged.is_empty()
    }

    /// Phase 1 (BEFORE `begin_render_pass`): upload this frame's staged quads and clear the stage.
    /// Returns the instance count to hand to `draw` (0 = nothing staged, skip `draw`).
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, surface_w: u32, surface_h: u32) -> u32 {
        if self.staged.is_empty() {
            return 0;
        }
        if self.staged.len() > self.inst_cap {
            self.inst_cap = self.staged.len().next_power_of_two();
            self.inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ui instances"),
                size: (self.inst_cap * std::mem::size_of::<UiInst>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.screen_buf, 0, bytemuck::cast_slice(&[surface_w as f32, surface_h as f32, 0.0, 0.0]));
        queue.write_buffer(&self.inst_buf, 0, bytemuck::cast_slice(&self.staged));
        let n = self.staged.len() as u32;
        self.staged.clear();
        n
    }

    /// Phase 2 (INSIDE the render pass, after the background draws): record the instanced overlay
    /// draw for the `count` quads uploaded by `prepare` this frame.
    pub fn draw<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, count: u32) {
        if count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.inst_buf.slice(..));
        pass.draw(0..4, 0..count);
    }
}
