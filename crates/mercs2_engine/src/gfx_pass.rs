//! Draws a [`mercs2_gfx`] movie's draw list.
//!
//! Every vertex of a frame is transformed here into one stream, so a batch is only what changes GPU
//! state: the texture, the blend mode and where the stencil stands. Masks are the stencil: a mask's
//! shapes raise it, the draws it covers test against the raised level, and the shapes draw again to
//! lower it.

use std::collections::HashMap;

use mercs2_formats::gfx_def::{Character, GradientKind, MovieDef};
use mercs2_formats::texture::TextureData;
use mercs2_gfx::stage::{Draw, Item};
use mercs2_gfx::tess::Paint;
use wgpu::util::DeviceExt;

const DEPTH_STENCIL: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    mul: [f32; 4],
    add: [f32; 4],
    kind: [f32; 3],
}

/// The textures a movie names, by the file name in its image tags.
pub trait GfxImages {
    fn image(&mut self, file_name: &str) -> Result<TextureData, String>;
}

/// External images from the WADs a front end mounts, found by scanning their blocks: `shell.wad`'s
/// aliases make its ASET rows point at the wrong entry for these.
pub struct WadImages {
    wads: Vec<crate::wad::Wad>,
    index: HashMap<u32, (usize, u16)>,
    movies: HashMap<u32, (usize, u16)>,
    scripts: HashMap<u32, Vec<u8>>,
}

impl WadImages {
    pub fn open(paths: &[String]) -> Result<WadImages, String> {
        let mut wads = Vec::new();
        let mut index = HashMap::new();
        let mut movies = HashMap::new();
        let mut scripts = HashMap::new();
        for (w, path) in paths.iter().enumerate() {
            let mut wad = crate::wad::open(path)?;
            for block in 0..wad.block_count() {
                let bytes = crate::wad::decompress_block_index(&mut wad, block)?;
                let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&bytes);
                let mut at = 4 + count as usize * 16;
                for e in &entries {
                    let end = at + e.chunk_size as usize;
                    if e.type_hash == mercs2_formats::types::TYPE_HASH_TEXTURE {
                        index.entry(e.name_hash).or_insert((w, block));
                    } else if e.type_hash == mercs2_formats::types::TYPE_HASH_CFX_PACK {
                        movies.entry(e.name_hash).or_insert((w, block));
                    } else if e.type_hash == mercs2_formats::types::TYPE_HASH_SCRIPT && !scripts.contains_key(&e.name_hash) {
                        let container = bytes.get(at..end).ok_or_else(|| format!("a script overruns block {block} of {path}"))?;
                        let lay = mercs2_formats::scripts_block::parse_container(container).map_err(|e| format!("a script in {path}: {e}"))?;
                        scripts.insert(e.name_hash, container[lay.luaq_off..lay.csum_off].to_vec());
                    }
                    at = end;
                }
            }
            wads.push(wad);
        }
        Ok(WadImages { wads, index, movies, scripts })
    }

    /// Hand over the scripts these WADs hold: each module's LuaQ chunk, by its name hash.
    pub fn take_scripts(&mut self) -> HashMap<u32, Vec<u8>> {
        std::mem::take(&mut self.scripts)
    }

    /// A movie's file bytes, by its name without the extension.
    pub fn movie(&mut self, stem: &str) -> Result<Vec<u8>, String> {
        let hash = mercs2_formats::hash::pandemic_hash_m2(stem);
        let (w, block) = *self.movies.get(&hash).ok_or_else(|| format!("no movie {stem} (0x{hash:08X})"))?;
        let bytes = crate::wad::decompress_block_index(&mut self.wads[w], block)?;
        let (count, entries) = mercs2_formats::ucfx::parse_block_entry_table(&bytes);
        let mut at = 4 + count as usize * 16;
        for e in &entries {
            let end = at + e.chunk_size as usize;
            if e.type_hash == mercs2_formats::types::TYPE_HASH_CFX_PACK && e.name_hash == hash {
                let chunk = bytes.get(at..end).ok_or_else(|| format!("movie {stem} overruns its block"))?;
                return mercs2_formats::gfx::cfx_pack_movie(chunk, stem);
            }
            at = end;
        }
        Err(format!("block {block} lost movie {stem}"))
    }

    /// The `language` string tables of these WADs, by key hash: what a movie's `[0x…]` keys read.
    pub fn strings(&mut self, language: &str) -> Result<HashMap<u32, String>, String> {
        use mercs2_formats::ucfx::{extract_chunk_body, parse_block_entry_table};
        let mut out = HashMap::new();
        for wad in &mut self.wads {
            for block in 0..wad.block_count() {
                if !wad.block_path(block).to_ascii_lowercase().contains(language) {
                    continue;
                }
                let bytes = crate::wad::decompress_block_index(wad, block)?;
                let (count, entries) = parse_block_entry_table(&bytes);
                let mut at = 4 + count as usize * 16;
                for e in &entries {
                    let end = at + e.chunk_size as usize;
                    if e.type_hash == mercs2_formats::types::TYPE_HASH_STRINGDB {
                        let chunk = bytes.get(at..end).ok_or("a string table overruns its block")?;
                        let keys = extract_chunk_body(chunk, b"KEYS").ok_or("a string table with no KEYS")?;
                        let text = extract_chunk_body(chunk, b"STRS").ok_or("a string table with no STRS")?;
                        for row in mercs2_formats::stringdb::parse(&keys, &text)?.entries {
                            out.entry(row.key_hash).or_insert(row.text);
                        }
                    }
                    at = end;
                }
            }
        }
        Ok(out)
    }
}

impl GfxImages for WadImages {
    fn image(&mut self, file_name: &str) -> Result<TextureData, String> {
        let stem = file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem);
        let hash = mercs2_formats::hash::pandemic_hash_m2(stem);
        let (w, block) = *self.index.get(&hash).ok_or_else(|| format!("no texture {stem} (0x{hash:08X})"))?;
        crate::wad::tex_from_block(&mut self.wads[w], block, hash).ok_or_else(|| format!("texture {stem} did not decode"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Stencil {
    /// Draw color where the stencil holds the current level.
    Test,
    /// Raise the stencil where the mask's shapes fall.
    Raise,
    /// Lower it again.
    Lower,
}

/// Which texture a batch binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Source {
    White,
    Ramp(usize),
    Image { index: usize, clamp: bool, smooth: bool },
    Atlas(usize),
}

struct Batch {
    source: Source,
    blend: u8,
    stencil: Stencil,
    level: u32,
    indices: std::ops::Range<u32>,
}

/// What a frame leaves behind to record.
struct Frame {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    batches: Vec<Batch>,
}

/// The inputs of a frame's build, and the geometry it grows.
struct Build<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    def: &'a MovieDef,
    images: &'a mut dyn GfxImages,
    stage: [f32; 2],
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    batches: Vec<Batch>,
}

pub struct GfxPass {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    pipelines: HashMap<(u8, Stencil), wgpu::RenderPipeline>,
    samplers: HashMap<(bool, bool), wgpu::Sampler>,
    /// Each image once, by its file name: movies share them.
    views: Vec<wgpu::TextureView>,
    view_index: HashMap<String, usize>,
    /// Stage size of each image, to turn pixels into texture coordinates.
    sizes: Vec<[f32; 2]>,
    ramps: Vec<wgpu::TextureView>,
    ramp_index: HashMap<Vec<u8>, usize>,
    atlases: Vec<wgpu::TextureView>,
    atlas_index: HashMap<String, usize>,
    white: wgpu::TextureView,
    bindings: HashMap<Source, wgpu::BindGroup>,
    /// One entry per movie drawn this frame, back to front.
    frames: Vec<Frame>,
}

/// The wgpu blend of a SWF blend mode.
fn blend_state(mode: u8) -> Result<wgpu::BlendState, String> {
    use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
    let over = C { src_factor: F::One, dst_factor: F::OneMinusSrcAlpha, operation: O::Add };
    let color = match mode {
        0 | 1 => C { src_factor: F::SrcAlpha, dst_factor: F::OneMinusSrcAlpha, operation: O::Add },
        3 => C { src_factor: F::Dst, dst_factor: F::OneMinusSrcAlpha, operation: O::Add },
        4 => C { src_factor: F::One, dst_factor: F::OneMinusSrc, operation: O::Add },
        8 => C { src_factor: F::SrcAlpha, dst_factor: F::One, operation: O::Add },
        9 => C { src_factor: F::SrcAlpha, dst_factor: F::One, operation: O::ReverseSubtract },
        other => return Err(format!("blend mode {other} has no pipeline")),
    };
    Ok(wgpu::BlendState { color, alpha: over })
}

fn upload_rgba(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) -> wgpu::TextureView {
    device
        .create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("gfx texture"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        )
        .create_view(&wgpu::TextureViewDescriptor::default())
}

impl GfxPass {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> GfxPass {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gfx bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gfx pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        GfxPass {
            format,
            shader: device.create_shader_module(wgpu::include_wgsl!("gfx.wgsl")),
            white: upload_rgba(device, queue, 1, 1, &[255; 4]),
            layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            samplers: HashMap::new(),
            views: Vec::new(),
            view_index: HashMap::new(),
            sizes: Vec::new(),
            ramps: Vec::new(),
            ramp_index: HashMap::new(),
            atlases: Vec::new(),
            atlas_index: HashMap::new(),
            bindings: HashMap::new(),
            frames: Vec::new(),
        }
    }

    fn pipeline(&mut self, device: &wgpu::Device, blend: u8, stencil: Stencil) -> Result<(), String> {
        if self.pipelines.contains_key(&(blend, stencil)) {
            return Ok(());
        }
        let face = |compare, pass_op| wgpu::StencilFaceState {
            compare,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op,
        };
        let (front, write_mask) = match stencil {
            Stencil::Test => (face(wgpu::CompareFunction::Equal, wgpu::StencilOperation::Keep), wgpu::ColorWrites::ALL),
            Stencil::Raise => (face(wgpu::CompareFunction::Equal, wgpu::StencilOperation::IncrementClamp), wgpu::ColorWrites::empty()),
            Stencil::Lower => (face(wgpu::CompareFunction::Equal, wgpu::StencilOperation::DecrementClamp), wgpu::ColorWrites::empty()),
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gfx pipeline"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Float32x3],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState { format: self.format, blend: Some(blend_state(blend)?), write_mask })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_STENCIL,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState { front, back: front, read_mask: 0xFF, write_mask: 0xFF },
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert((blend, stencil), pipeline);
        Ok(())
    }

    fn sampler(&mut self, device: &wgpu::Device, clamp: bool, smooth: bool) {
        self.samplers.entry((clamp, smooth)).or_insert_with(|| {
            let address = if clamp { wgpu::AddressMode::ClampToEdge } else { wgpu::AddressMode::Repeat };
            let filter = if smooth { wgpu::FilterMode::Linear } else { wgpu::FilterMode::Nearest };
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("gfx sampler"),
                address_mode_u: address,
                address_mode_v: address,
                mag_filter: filter,
                min_filter: filter,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        });
    }

    fn binding(&mut self, device: &wgpu::Device, source: Source) -> Result<(), String> {
        if self.bindings.contains_key(&source) {
            return Ok(());
        }
        let (clamp, smooth) = match source {
            Source::Image { clamp, smooth, .. } => (clamp, smooth),
            _ => (true, true),
        };
        self.sampler(device, clamp, smooth);
        let view = match source {
            Source::White => &self.white,
            Source::Ramp(i) => &self.ramps[i],
            Source::Atlas(i) => &self.atlases[i],
            Source::Image { index, .. } => &self.views[index],
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gfx bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.samplers[&(clamp, smooth)]) },
            ],
        });
        self.bindings.insert(source, group);
        Ok(())
    }

    fn ramp(&mut self, build: &Build, ramp: &[[u8; 4]]) -> usize {
        let bytes: Vec<u8> = ramp.iter().flatten().copied().collect();
        if let Some(i) = self.ramp_index.get(&bytes) {
            return *i;
        }
        let i = self.ramps.len();
        self.ramps.push(upload_rgba(build.device, build.queue, ramp.len() as u32, 1, &bytes));
        self.ramp_index.insert(bytes, i);
        i
    }

    fn atlas(&mut self, build: &mut Build, file_name: &str) -> Result<usize, String> {
        if let Some(i) = self.atlas_index.get(file_name) {
            return Ok(*i);
        }
        let texture = build.images.image(file_name)?;
        let view = crate::render::make_bc_view(build.device, build.queue, &texture, false).ok_or_else(|| format!("{file_name} did not upload"))?;
        let i = self.atlases.len();
        self.atlases.push(view);
        self.atlas_index.insert(file_name.to_string(), i);
        Ok(i)
    }

    fn load_image(&mut self, build: &mut Build, id: u16) -> Result<usize, String> {
        let Some(Character::Image(image)) = build.def.characters.get(&id) else {
            return Err(format!("bitmap fill names character {id}, which is not an image"));
        };
        if let Some(index) = self.view_index.get(&image.file_name) {
            return Ok(*index);
        }
        let texture = build.images.image(&image.file_name)?;
        let view = crate::render::make_bc_view(build.device, build.queue, &texture, false)
            .ok_or_else(|| format!("{} did not upload", image.file_name))?;
        let index = self.views.len();
        self.views.push(view);
        self.sizes.push([image.width as f32, image.height as f32]);
        self.view_index.insert(image.file_name.clone(), index);
        Ok(index)
    }

    /// Forget the last frame's movies, before [`Self::prepare`]ing this frame's.
    pub fn begin(&mut self) {
        self.frames.clear();
    }

    /// Add a movie's frame on top of the ones prepared since [`Self::begin`]; its stage fills the target.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        def: &MovieDef,
        items: &[Item],
        images: &mut dyn GfxImages,
    ) -> Result<(), String> {
        let stage = [(def.stage[1] - def.stage[0]) as f32 / 20.0, (def.stage[3] - def.stage[2]) as f32 / 20.0];
        let mut build = Build { device, queue, def, images, stage, vertices: Vec::new(), indices: Vec::new(), batches: Vec::new() };
        let mut level = 0u32;
        let mut raising = false;
        for item in items {
            match item {
                Item::MaskBegin => raising = true,
                Item::MaskEnd => {
                    raising = false;
                    level += 1;
                }
                Item::MaskPop(draws) => {
                    for draw in draws {
                        self.push_draw(&mut build, draw, Stencil::Lower, level)?;
                    }
                    level = level.checked_sub(1).ok_or("a mask popped with none in force")?;
                }
                Item::Draw(draw) => {
                    let stencil = if raising { Stencil::Raise } else { Stencil::Test };
                    self.push_draw(&mut build, draw, stencil, level)?;
                }
            }
        }
        let Build { vertices, indices, batches, .. } = build;
        for batch in &batches {
            self.pipeline(device, batch.blend, batch.stencil)?;
            self.binding(device, batch.source)?;
        }
        self.frames.push(Frame {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gfx vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gfx indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            batches,
        });
        Ok(())
    }

    fn push_draw(&mut self, build: &mut Build, draw: &Draw, stencil: Stencil, level: u32) -> Result<(), String> {
        for part in draw.parts.iter() {
            let (source, color, kind, uv_of): (Source, [f32; 4], [f32; 3], Box<dyn Fn(usize, f32, f32) -> [f32; 2]>) = match &part.paint {
                Paint::Solid(c) => (Source::White, c.map(|v| v as f32 / 255.0), [0.0; 3], Box::new(|_, _, _| [0.0; 2])),
                Paint::Glyphs { atlas, color, uvs } => (
                    Source::Atlas(self.atlas(build, atlas)?),
                    color.map(|v| v as f32 / 255.0),
                    [1.0, 0.0, 0.0],
                    Box::new(|i, _, _| uvs[i]),
                ),
                Paint::Gradient { ramp, kind, to_unit, spread, focal } => {
                    let mode = match kind {
                        GradientKind::Linear => 2.0,
                        GradientKind::Radial => 3.0,
                        GradientKind::Focal => 4.0,
                    };
                    let matrix = *to_unit;
                    (
                        Source::Ramp(self.ramp(build, ramp)),
                        [1.0; 4],
                        [mode, *spread as f32, *focal],
                        Box::new(move |_, x, y| matrix.apply(x, y)),
                    )
                }
                Paint::Bitmap { id, to_pixels, clamp, smooth } => {
                    let index = self.load_image(build, *id)?;
                    let [w, h] = self.sizes[index];
                    let matrix = *to_pixels;
                    (
                        Source::Image { index, clamp: *clamp, smooth: *smooth },
                        [1.0; 4],
                        [1.0, 0.0, 0.0],
                        Box::new(move |_, x, y| {
                            let [u, v] = matrix.apply(x, y);
                            [u / w, v / h]
                        }),
                    )
                }
            };
            let base = build.vertices.len() as u32;
            let mul: [f32; 4] = std::array::from_fn(|k| color[k] * draw.cxform.mul[k]);
            for (i, [x, y]) in part.vertices.iter().enumerate() {
                let [px, py] = draw.matrix.apply(*x, *y);
                build.vertices.push(Vertex {
                    position: [px / build.stage[0] * 2.0 - 1.0, 1.0 - py / build.stage[1] * 2.0],
                    uv: uv_of(i, *x, *y),
                    mul,
                    add: draw.cxform.add,
                    kind,
                });
            }
            let start = build.indices.len() as u32;
            build.indices.extend(part.indices.iter().map(|i| i + base));
            let end = build.indices.len() as u32;
            match build.batches.last_mut() {
                Some(b) if b.source == source && b.blend == draw.blend && b.stencil == stencil && b.level == level && b.indices.end == start => {
                    b.indices.end = end;
                }
                _ => build.batches.push(Batch { source, blend: draw.blend, stencil, level, indices: start..end }),
            }
        }
        Ok(())
    }

    /// Record the prepared frames. The pass needs a stencil attachment of [`Self::depth_format`].
    pub fn record<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>) {
        for frame in &self.frames {
            if frame.batches.is_empty() {
                continue;
            }
            pass.set_vertex_buffer(0, frame.vertices.slice(..));
            pass.set_index_buffer(frame.indices.slice(..), wgpu::IndexFormat::Uint32);
            for batch in &frame.batches {
                pass.set_pipeline(&self.pipelines[&(batch.blend, batch.stencil)]);
                pass.set_bind_group(0, &self.bindings[&batch.source], &[]);
                pass.set_stencil_reference(batch.level);
                pass.draw_indexed(batch.indices.clone(), 0, 0..1);
            }
        }
    }

    pub fn depth_format() -> wgpu::TextureFormat {
        DEPTH_STENCIL
    }
}

/// Render one draw list off screen to RGBA, on a device of its own.
pub fn render_to_rgba(draws: &[crate::flash::FlashDraw], images: &mut dyn GfxImages, size: [u32; 2], clear: [f64; 4]) -> Result<Vec<u8>, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .map_err(|e| format!("no adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("gfx offscreen"),
        required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        ..Default::default()
    }))
    .map_err(|e| format!("no device: {e}"))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gfx target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let stencil = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gfx stencil"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_STENCIL,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let stencil_view = stencil.create_view(&wgpu::TextureViewDescriptor::default());
    let mut gfx = GfxPass::new(&device, &queue, format);
    gfx.begin();
    for draw in draws {
        gfx.prepare(&device, &queue, &draw.def, &draw.items, images)?;
    }
    let row = (size[0] * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gfx readback"),
        size: row as u64 * size[1] as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gfx offscreen") });
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gfx pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0], g: clear[1], b: clear[2], a: clear[3] }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &stencil_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Discard }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        gfx.record(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(size[1]) },
        },
        extent,
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("wait for the readback: {e}"))?;
    let data = slice.get_mapped_range().map_err(|e| format!("map the readback: {e}"))?;
    let mut out = Vec::with_capacity((size[0] * size[1] * 4) as usize);
    for y in 0..size[1] as usize {
        let start = y * row as usize;
        out.extend_from_slice(&data[start..start + size[0] as usize * 4]);
    }
    Ok(out)
}
