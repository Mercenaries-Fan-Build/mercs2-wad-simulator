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
            if mb > 0 && acc + mb <= td.all_mips.len() {
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
        Some((w, h, sz)) if avail >= sz => (w, h, sz),
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

pub fn make_tex_bind(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    diffuse: &wgpu::TextureView,
    normal: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material bind"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(diffuse),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(normal),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
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

/// A model loaded from the WAD, ready to hand to the scene (GPU) + asset store (CPU).
pub struct LoadedModel {
    pub hash: u32,
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<mesh::DrawGroup>,
    pub textures: TexMap,
    pub skin: mesh::SkinData,
    pub clips: Vec<ClipAnim>,
}
