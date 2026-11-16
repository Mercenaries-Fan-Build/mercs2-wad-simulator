//! Inspect's UI tab: a Scaleform movie playing in the viewport, run by the engine's own player
//! (`mercs2_engine::gfx`) and drawn by its Flash pass (`mercs2_engine::gfx_pass`) on the window's
//! device, into a texture the GUI shows.
//!
//! The movie runs its own timeline here. The values its host hands it in the game (its owner's Lua,
//! the exe's own calls, a parent movie) are not given yet, so a field the host fills reads `undefined`;
//! the movie card lists every read that found nothing and the functions a host can call.

use std::collections::HashMap;
use std::rc::Rc;

use mercs2_engine::gfx::avm::{FieldInfo, Player, Value};
use mercs2_engine::gfx::stage::Movie;
use mercs2_engine::gfx::text::FontLib;
use mercs2_engine::gfx_pass::{GfxImages, GfxPass};
use mercs2_engine::wad;
use mercs2_formats::gfx::GfxMovie;
use mercs2_formats::gfx_def::{Control, Matrix, MovieDef, Place, Timeline};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::texture::TextureData;
use mercs2_formats::types::TYPE_HASH_CFX_PACK;

use crate::app::WadStack;
use crate::gui::theme;

/// The font movie every language but Russian reads (`FUN_0060E9C0`); the workshop shows English.
const FONTS: &str = "fonts_enext";
/// The string table the movies' `[0x…]` keys read.
const LANGUAGE: &str = "english";
/// The pass writes gamma-encoded colour, as it does into the surface's non-sRGB view in the front end
/// (`Scene::prepare_front_end`), and the GUI samples a native texture in this format as gamma
/// (`egui_wgpu::Renderer::register_native_texture`): one texture serves both.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// What the movie draws over: light gray, `#C8C8C8`, under which the movies' translucent dark panels
/// read. Opaque, so the frame's alpha stays 1 for the GUI, which takes textures as premultiplied.
const GROUND: wgpu::Color = wgpu::Color { r: 200.0 / 255.0, g: 200.0 / 255.0, b: 200.0 / 255.0, a: 1.0 };

/// What the movies read besides themselves: the font library and the string table, with the names the
/// `lua` scripts give its keys (`textpage::lua_key_names`).
pub struct Shared {
    fonts: Rc<FontLib>,
    strings: Rc<HashMap<u32, String>>,
    key_names: Rc<HashMap<u32, String>>,
    /// How many scripts the key names were read from.
    pub scripts: usize,
}

impl Shared {
    pub fn read(w: &mut WadStack, lua: &[String]) -> Result<Shared, String> {
        let fonts = Rc::new(FontLib::from_movie(&movie_anywhere(w, FONTS)?)?);
        let strings = w.string_table(LANGUAGE)?;
        let keys: std::collections::HashSet<u32> = strings.keys().copied().collect();
        let key_names = Rc::new(crate::textpage::lua_key_names(lua, &keys));
        Ok(Shared { fonts, strings: Rc::new(strings), key_names, scripts: lua.len() })
    }
}

/// A movie by name from the first source that holds one: the stack last-opened first, then the side
/// WADs (`fonts_enext` ships in `Loading.wad`).
fn movie_anywhere(w: &mut WadStack, name: &str) -> Result<MovieDef, String> {
    let hash = pandemic_hash_m2(name);
    let mut tried = Vec::new();
    for src in w.resolution_order() {
        let holds = w.source_mut(src).is_some_and(|x| wad::aset_types(x, hash).iter().any(|(ty, _, _)| *ty == mercs2_formats::types::TYPE_ID_CFX_PACK));
        if holds {
            return movie_def(w, src, hash, name);
        }
        tried.push(w.source_path(src).to_string());
    }
    Err(format!("no movie {name} (0x{hash:08X}) in {}", tried.join(", ")))
}

/// The movie file `hash` as source `src` holds it.
fn movie_bytes(w: &mut WadStack, src: usize, hash: u32, label: &str) -> Result<Vec<u8>, String> {
    let x = w.source_mut(src).ok_or_else(|| format!("no source {src}"))?;
    let container = wad::extract_container_typed(x, hash, TYPE_HASH_CFX_PACK)?;
    mercs2_formats::gfx::cfx_pack_movie(&container, label)
}

/// The movie `hash` as source `src` holds it, decoded.
fn movie_def(w: &mut WadStack, src: usize, hash: u32, label: &str) -> Result<MovieDef, String> {
    let bytes = movie_bytes(w, src, hash, label)?;
    MovieDef::decode(&GfxMovie::parse(&bytes).map_err(|e| format!("{label}: {e}"))?).map_err(|e| format!("{label}: {e}"))
}

/// A movie's external images, by the file name its image tags carry, resolved like any texture.
struct StackImages<'a>(&'a mut WadStack);

impl GfxImages for StackImages<'_> {
    fn image(&mut self, file_name: &str) -> Result<TextureData, String> {
        let stem = file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem);
        self.0.texture_best(pandemic_hash_m2(stem)).map_err(|e| format!("image {file_name}: {e}"))
    }
}

/// The texture a movie's frame is drawn into, and the GUI's handle on it.
struct Target {
    size: [u32; 2],
    draw: wgpu::TextureView,
    stencil: wgpu::TextureView,
    id: egui::TextureId,
}

pub struct MovieView {
    pub hash: u32,
    pub label: String,
    /// The file of the WAD the movie was read from.
    pub wad: String,
    /// The movie as shipped, and what plays: it, or one of its exports alone.
    whole: Rc<MovieDef>,
    def: Rc<MovieDef>,
    /// The export playing alone, if one is.
    pub isolated: Option<String>,
    /// [`drawable_exports`] of the whole movie.
    pub exports: Vec<String>,
    player: Player,
    fonts: Rc<FontLib>,
    strings: Rc<HashMap<u32, String>>,
    key_names: Rc<HashMap<u32, String>>,
    pub playing: bool,
    /// Seconds run toward the next movie frame.
    clock: f32,
    /// Movie frames run since the movie (re)started.
    pub frames_run: u64,
    /// The movie's `trace` lines and FSCommands, oldest first, since it (re)started.
    pub traces: Vec<String>,
    pub fscommands: Vec<(String, String)>,
    /// The functions the root timeline holds after the last frame, with their parameter names: what a
    /// host can call by name.
    pub entry_points: Vec<(String, Vec<String>)>,
    /// The text fields on stage after the last frame.
    pub fields: Vec<FieldInfo>,
    /// The named clips on stage after the last frame, by path.
    pub clips: Vec<(usize, String)>,
    /// The clip drawn alone, by path, while the whole movie runs.
    pub focus: Option<String>,
    /// The inspector's edit buffers: a field's text by its path, a function's arguments by its name.
    edits: HashMap<String, String>,
    args: HashMap<String, Vec<String>>,
    /// The last call of each function the inspector made that did not go through, by name.
    call_errors: HashMap<String, String>,
    /// The calls made into the movie as it loads, in order, arguments as typed.
    pub on_load: Vec<(String, Vec<String>)>,
    /// What stopped the movie: a player error, or the frame not drawing.
    pub error: Option<String>,
    pass: Option<GfxPass>,
    target: Option<Target>,
    /// The viewport's size in pixels at the last layout; the frame is drawn to fit it.
    want: [u32; 2],
}

#[derive(Clone, Debug)]
pub enum Act {
    Play,
    Pause,
    /// Run one movie frame.
    Step,
    Restart,
    /// `gotoAndPlay` a frame label of a clip's timeline (the root's or the focused clip's).
    GotoLabel { clip: usize, label: String },
    /// `gotoAndStop` a clip's timeline at a zero-based frame.
    GotoFrame { clip: usize, frame: u16 },
    /// Set a text field's text, as a script's `field.text = …` does.
    SetText { parent: usize, depth: i32, text: String },
    /// Call a root function with the arguments as typed ([`parse_arg`]), as a host's `Invoke` does.
    Call { name: String, args: Vec<String> },
    /// Add a call to the ones made when the movie loads, and load it again.
    OnLoad { name: String, args: Vec<String> },
    /// Take an on-load call away, by its place in the list, and load the movie again.
    RemoveOnLoad(usize),
    /// Play one exported clip alone ([`isolated`]).
    Isolate(String),
    /// Back to the whole movie.
    Whole,
    /// Draw one named clip alone, by path, while the whole movie runs; `None` draws the stage.
    Focus(Option<String>),
}

/// `whole` with its root timeline replaced by one placement of the export `name` at the stage's
/// centre, under its export name, as `attachMovie` would put it on an empty stage: the clip runs its
/// own timeline and scripts; whatever the root defined is not there.
pub fn isolated(whole: &MovieDef, name: &str) -> Result<MovieDef, String> {
    let &id = whole.exports.get(name).ok_or_else(|| format!("no export {name}"))?;
    let mut def = whole.clone();
    let center = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: (def.stage[0] + def.stage[1]) as f32 / 2.0,
        ty: (def.stage[2] + def.stage[3]) as f32 / 2.0,
    };
    def.timeline = Timeline {
        declared_frames: 1,
        frames: vec![vec![Control::Place(Place { depth: 1, character: Some(id), matrix: Some(center), name: Some(name.to_string()), ..Place::default() })]],
        labels: Default::default(),
    };
    Ok(def)
}

/// The exports of `def` that draw: clips, shapes and text fields.
pub fn drawable_exports(def: &MovieDef) -> Vec<String> {
    use mercs2_formats::gfx_def::Character as C;
    def.exports
        .iter()
        .filter(|(_, id)| matches!(def.characters.get(id), Some(C::Sprite(_) | C::Shape(_) | C::Morph(_) | C::EditText(_))))
        .map(|(name, _)| name.clone())
        .collect()
}

/// A timeline as the strip draws it ([`MovieView::timeline`]).
#[derive(Clone, Debug)]
pub struct TimelineView {
    pub clip: usize,
    /// The clip's path, `_level0` for the root.
    pub whose: String,
    pub frames: u16,
    /// Zero-based.
    pub at: u16,
    /// Its labels by frame, earliest first.
    pub labels: Vec<(String, u16)>,
}

impl TimelineView {
    /// The sections the labels cut the timeline into: `(label, first frame, frame past the last)`, a
    /// stretch before the first label with no label.
    pub fn sections(&self) -> Vec<(Option<&str>, u16, u16)> {
        let mut out = Vec::new();
        let first = self.labels.first().map_or(self.frames, |(_, f)| *f);
        if first > 0 {
            out.push((None, 0, first));
        }
        for (i, (name, start)) in self.labels.iter().enumerate() {
            let end = self.labels[i + 1..].iter().map(|(_, f)| *f).find(|f| f > start).unwrap_or(self.frames);
            out.push((Some(name.as_str()), *start, end.max(*start + 1)));
        }
        out
    }
}

/// An argument as typed: a number, `true`, `false`, `null`, `undefined`, or text in double quotes.
pub fn parse_arg(typed: &str) -> Result<Value, String> {
    let t = typed.trim();
    match t {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        "null" => return Ok(Value::Null),
        "undefined" => return Ok(Value::Undefined),
        _ => {}
    }
    if let Some(inner) = t.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Ok(Value::Str(inner.into()));
    }
    t.parse::<f64>().map(Value::Number).map_err(|_| format!("{typed:?} is not a number, true, false, null, undefined or \"text\""))
}

impl MovieView {
    /// Open the movie `hash` from source `src`.
    pub fn open(w: &mut WadStack, shared: &Shared, hash: u32, label: String, src: usize) -> Result<MovieView, String> {
        let def = Rc::new(movie_def(w, src, hash, &label)?);
        let wad = std::path::Path::new(w.source_path(src)).file_name().map_or_else(|| format!("source {src}"), |f| f.to_string_lossy().into_owned());
        let player = new_player(&def, &shared.fonts, &shared.strings)?;
        let exports = drawable_exports(&def);
        let mut v = MovieView {
            hash,
            label,
            wad,
            whole: def.clone(),
            def,
            isolated: None,
            exports,
            player,
            fonts: shared.fonts.clone(),
            strings: shared.strings.clone(),
            key_names: shared.key_names.clone(),
            playing: true,
            clock: 0.0,
            frames_run: 0,
            traces: Vec::new(),
            fscommands: Vec::new(),
            entry_points: Vec::new(),
            fields: Vec::new(),
            clips: Vec::new(),
            focus: None,
            edits: HashMap::new(),
            args: HashMap::new(),
            call_errors: HashMap::new(),
            on_load: Vec::new(),
            error: None,
            pass: None,
            target: None,
            want: [0, 0],
        };
        v.collect();
        Ok(v)
    }

    /// The stage in pixels.
    pub fn stage(&self) -> [f32; 2] {
        [(self.def.stage[1] - self.def.stage[0]) as f32 / 20.0, (self.def.stage[3] - self.def.stage[2]) as f32 / 20.0]
    }

    /// The root timeline's frame (1-based, as Flash counts) and its frame count.
    pub fn frame(&self) -> (u32, u32) {
        let at = self.player.movie.clip(self.player.movie.root).map_or(0, |c| c.frame as u32 + 1);
        (at, self.def.timeline.frames.len() as u32)
    }

    /// The timeline the strip shows: the focused clip's, else the root's.
    pub fn timeline(&self) -> Option<TimelineView> {
        let movie = &self.player.movie;
        let (clip, line, whose) = match &self.focus {
            Some(path) => {
                let &(id, _) = self.clips.iter().find(|(_, p)| p == path)?;
                let character = movie.clip(id).ok()?.character?;
                let mercs2_formats::gfx_def::Character::Sprite(line) = self.def.characters.get(&character)? else { return None };
                (id, line, path.clone())
            }
            None => (movie.root, &self.def.timeline, "_level0".to_string()),
        };
        let state = movie.clip(clip).ok()?;
        let mut labels: Vec<(String, u16)> = line.labels.iter().map(|(n, f)| (n.clone(), *f)).collect();
        labels.sort_by_key(|(_, f)| *f);
        Some(TimelineView { clip, whose, frames: line.frames.len().max(1) as u16, at: state.frame, labels })
    }

    pub fn missing(&self) -> &std::collections::BTreeSet<String> {
        &self.player.missing
    }

    pub fn on_undefined(&self) -> &std::collections::BTreeSet<String> {
        &self.player.on_undefined
    }

    pub fn log(&self) -> &[String] {
        &self.player.log
    }

    /// Reads nothing defined, by where they were looked for, with how often: the inputs no host gave.
    pub fn unresolved(&self) -> &std::collections::BTreeMap<String, u32> {
        &self.player.unresolved
    }

    fn fail(&mut self, e: String) {
        self.playing = false;
        self.error = Some(e);
    }

    /// Run one movie frame and keep what it said.
    fn step_once(&mut self) {
        let period = 1.0 / self.def.frame_rate.max(1.0) as f64;
        match self.player.advance(period * 1000.0) {
            Ok(()) => {
                self.frames_run += 1;
                self.collect();
            }
            Err(e) => self.fail(format!("frame {}: {e}", self.frames_run + 1)),
        }
    }

    /// Keep what the movie said and read what is on stage, after a frame or a call into it.
    fn collect(&mut self) {
        self.traces.append(&mut self.player.traces);
        self.fscommands.append(&mut self.player.fscommands);
        self.entry_points = self.player.root_functions();
        match self.player.text_fields() {
            Ok(f) => self.fields = f,
            Err(e) => self.fail(format!("text fields: {e}")),
        }
        match self.player.named_clips() {
            Ok(c) => self.clips = c,
            Err(e) => self.fail(format!("clips: {e}")),
        }
    }

    /// Advance by `dt` seconds of wall time, at the movie's own frame rate. One movie frame at most
    /// per call: a slow window frame slows the movie rather than skipping its frames.
    pub fn tick(&mut self, dt: f32) {
        if !self.playing {
            return;
        }
        let period = 1.0 / self.def.frame_rate.max(1.0);
        self.clock += dt;
        if self.clock >= period {
            self.clock = (self.clock - period).min(period);
            self.step_once();
        }
    }

    pub fn apply(&mut self, act: Act) {
        match act {
            Act::Play if self.error.is_none() => self.playing = true,
            Act::Play => {}
            Act::Pause => self.playing = false,
            Act::Step => {
                self.playing = false;
                if self.error.is_none() {
                    self.step_once();
                }
            }
            Act::Restart => self.restart(),
            Act::OnLoad { name, args } => match args.iter().map(|a| parse_arg(a)).collect::<Result<Vec<Value>, String>>() {
                Err(e) => {
                    self.call_errors.insert(name, e);
                }
                Ok(_) => {
                    self.call_errors.remove(&name);
                    self.on_load.push((name, args));
                    self.restart();
                }
            },
            Act::Isolate(name) => match isolated(&self.whole, &name) {
                Ok(def) => {
                    self.def = Rc::new(def);
                    self.isolated = Some(name);
                    self.focus = None;
                    self.restart();
                }
                Err(e) => self.fail(e),
            },
            Act::Focus(path) => self.focus = path,
            Act::Whole => {
                self.def = self.whole.clone();
                self.isolated = None;
                self.focus = None;
                self.restart();
            }
            Act::RemoveOnLoad(index) => {
                if index < self.on_load.len() {
                    self.on_load.remove(index);
                    self.restart();
                }
            }
            Act::GotoLabel { clip, label } => {
                if let Err(e) = self.player.invoke_on(clip, "gotoAndPlay", &[Value::Str(label.as_str().into())]) {
                    self.fail(format!("gotoAndPlay({label}): {e}"));
                }
                self.collect();
            }
            Act::GotoFrame { clip, frame } => {
                if let Err(e) = self.player.invoke_on(clip, "gotoAndStop", &[Value::Number(frame as f64 + 1.0)]) {
                    self.fail(format!("gotoAndStop({}): {e}", frame + 1));
                }
                self.collect();
            }
            Act::SetText { parent, depth, text } => {
                if let Err(e) = self.player.set_member(&Value::Field(parent, depth), "text", Value::Str(text.as_str().into())) {
                    self.fail(format!("set text: {e}"));
                }
                self.collect();
            }
            Act::Call { name, args } => {
                let values: Result<Vec<Value>, String> = args.iter().map(|a| parse_arg(a)).collect();
                match values {
                    Err(e) => {
                        self.call_errors.insert(name, e);
                    }
                    Ok(values) => {
                        self.call_errors.remove(&name);
                        if let Err(e) = self.player.invoke(&name, &values) {
                            self.fail(format!("{name}: {e}"));
                        }
                        self.collect();
                    }
                }
            }
        }
    }

    /// Load the movie again and make the on-load calls before its first frame, as the exe makes its
    /// calls into a movie it has just loaded.
    fn restart(&mut self) {
        self.frames_run = 0;
        self.clock = 0.0;
        self.traces.clear();
        self.fscommands.clear();
        self.error = None;
        self.playing = true;
        match new_player(&self.def, &self.fonts, &self.strings) {
            Ok(p) => self.player = p,
            Err(e) => return self.fail(format!("restart: {e}")),
        }
        if let Err(e) = self.make_on_load_calls() {
            return self.fail(e);
        }
        self.collect();
    }

    fn make_on_load_calls(&mut self) -> Result<(), String> {
        for (name, args) in self.on_load.clone() {
            let values = args.iter().map(|a| parse_arg(a)).collect::<Result<Vec<Value>, String>>().map_err(|e| format!("{name}: {e}"))?;
            self.player.invoke(&name, &values).map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(())
    }

    /// The size the frame is drawn at: the stage, scaled to fit the viewport whole.
    fn fit(&self) -> Option<[u32; 2]> {
        let stage = self.stage();
        let [w, h] = self.want;
        if w == 0 || h == 0 || stage[0] <= 0.0 || stage[1] <= 0.0 {
            return None;
        }
        let scale = (w as f32 / stage[0]).min(h as f32 / stage[1]);
        Some([((stage[0] * scale).round() as u32).max(1), ((stage[1] * scale).round() as u32).max(1)])
    }

    /// Draw the current frame into the texture the GUI shows, submitted on `queue` ahead of the
    /// window's frame. Nothing is drawn before the viewport has been laid out once.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, gui: &mut crate::gui::Gui, w: &mut WadStack) {
        if self.error.is_some() {
            return;
        }
        let Some(size) = self.fit() else { return };
        if let Err(e) = self.draw(device, queue, gui, w, size) {
            self.fail(format!("draw: {e}"));
        }
    }

    fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, gui: &mut crate::gui::Gui, w: &mut WadStack, size: [u32; 2]) -> Result<(), String> {
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("movie frame"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let stencil = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("movie stencil"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: GfxPass::depth_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let draw = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let id = match self.target.take() {
                Some(old) => {
                    gui.update_native(device, &draw, old.id);
                    old.id
                }
                None => gui.register_native(device, &draw),
            };
            self.target = Some(Target { size, draw, stencil: stencil.create_view(&wgpu::TextureViewDescriptor::default()), id });
        }
        self.prepare(device, queue, w)?;
        let pass = self.pass.as_ref().expect("prepare makes the pass");
        let target = self.target.as_ref().expect("made above");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("movie frame") });
        {
            let mut rp = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("movie pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.draw,
                        depth_slice: None,
                        resolve_target: None,
                        // The movies are overlays drawn over the game with transparency, and the
                        // movie's own background colour is not painted in the game either; an opaque
                        // ground they read against stands in for the game.
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(GROUND), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &target.stencil,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                        stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Discard }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            pass.record_frame(&mut rp, 0);
        }
        queue.submit([encoder.finish()]);
        Ok(())
    }

    /// Build the current frame's geometry, reading every image it draws.
    fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, w: &mut WadStack) -> Result<(), String> {
        // A focused clip draws alone; nothing, while the movie has it off stage.
        let items = match &self.focus {
            Some(path) => match self.clips.iter().find(|(_, p)| p == path) {
                Some(&(id, _)) => self.player.movie.draw_only(id)?,
                None => Vec::new(),
            },
            None => self.player.movie.draw()?,
        };
        let pass = self.pass.get_or_insert_with(|| GfxPass::new(device, queue, FORMAT));
        pass.begin();
        pass.prepare(device, queue, &self.def, &items, &mut StackImages(w))
    }

    /// Give the GUI's handle back.
    pub fn close(self, gui: &mut crate::gui::Gui) {
        if let Some(t) = self.target {
            gui.free_native(t.id);
        }
    }
}

fn new_player(def: &Rc<MovieDef>, fonts: &Rc<FontLib>, strings: &Rc<HashMap<u32, String>>) -> Result<Player, String> {
    let mut movie = Movie::new(def.clone())?;
    movie.font_lib = Some(fonts.clone());
    movie.strings = Some(strings.clone());
    Player::new(movie)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A label runs to the next label's frame, the last to the end; frames before the first label are
    /// a section with no label; a label at the same frame as the next still gets a frame.
    #[test]
    fn labels_cut_the_timeline_into_sections() {
        let tl = TimelineView {
            clip: 0,
            whose: "_level0".into(),
            frames: 30,
            at: 0,
            labels: vec![("a".into(), 5), ("b".into(), 12), ("c".into(), 12), ("d".into(), 20)],
        };
        assert_eq!(
            tl.sections(),
            vec![(None, 0, 5), (Some("a"), 5, 12), (Some("b"), 12, 20), (Some("c"), 12, 20), (Some("d"), 20, 30)]
        );
    }

    #[test]
    fn an_argument_is_a_number_a_literal_or_quoted_text() {
        assert!(matches!(parse_arg(" 2.5 "), Ok(Value::Number(n)) if n == 2.5));
        assert!(matches!(parse_arg("true"), Ok(Value::Bool(true))));
        assert!(matches!(parse_arg("undefined"), Ok(Value::Undefined)));
        assert!(matches!(parse_arg("\"Mattias\""), Ok(Value::Str(s)) if &*s == "Mattias"));
        assert!(parse_arg("Mattias").is_err());
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;
    use crate::index::{AssetIndex, Kind};

    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("no adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("movie test"),
            required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
            ..Default::default()
        }))
        .expect("no device")
    }

    /// Every movie of the UI tab writes back to the body it was read from (compared inflated: the
    /// loader only inflates, and deflate output is the compressor's), and its header's length is that
    /// body's — over the archive alone and over the stack a default launch opens.
    #[test]
    fn every_movie_writes_back_as_it_was_read() {
        use mercs2_formats::gfx_stream::{inflate, TagStream};
        let vz = crate::app::retail_vz_wad();
        for overlays in [Vec::new(), crate::app::overlays_for(&vz, true, &[])] {
            let mut w = WadStack::open(&vz, &overlays).expect("open retail vz.wad");
            let index = AssetIndex::build(&w.wads, &w.side, crate::index::load_all_names().expect("names"));
            let rows: Vec<_> = index.rows(Kind::UiGfx).to_vec();
            assert!(!rows.is_empty(), "the UI tab lists no rows");
            let mut failed = Vec::new();
            for row in &rows {
                let label = row.label();
                let result = (|| -> Result<(), String> {
                    let file = movie_bytes(&mut w, row.src, row.hash, &label)?;
                    let body = inflate(&file)?;
                    let stream = TagStream::parse(&file)?;
                    if stream.body() != body {
                        return Err("the written body differs".into());
                    }
                    if stream.file()?[..8] != file[..8] {
                        return Err(format!("the written header {:?} is not the read one {:?}", &stream.file()?[..8], &file[..8]));
                    }
                    if stream.compressed() && body.len() % 4096 != 0 {
                        return Err(format!("a {}-byte deflated body, not a multiple of 4096", body.len()));
                    }
                    if TagStream::parse(&stream.file()?)?.body() != body {
                        return Err("the written file reads back to another body".into());
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    failed.push(format!("{label} (0x{:08X}, {}): {e}", row.hash, w.source_path(row.src)));
                }
            }
            println!("{overlays:?}: {} movies, {} failed", rows.len(), failed.len());
            assert!(failed.is_empty(), "{} of {} movies failed:\n{}", failed.len(), rows.len(), failed.join("\n"));
        }
    }

    /// An on-load call reaches the movie before its first frame, as the exe's does:
    /// `pause_menu.profileName(name)` (named in the profile manager's table at pc-v10 `0x00d121b0`)
    /// stores the name, `InitializePause` puts it on the profile line; a text edit sets a field.
    #[test]
    fn an_on_load_call_and_a_text_edit_change_what_is_on_stage() {
        let vz = crate::app::retail_vz_wad();
        let mut w = WadStack::open(&vz, &[]).expect("open retail vz.wad");
        let shared = Shared::read(&mut w, &[]).expect("fonts and strings");
        let hash = pandemic_hash_m2("pause_menu");
        let mut v = MovieView::open(&mut w, &shared, hash, "pause_menu".into(), 0).expect("pause_menu");
        assert!(v.entry_points.iter().any(|(n, p)| n == "profileName" && p.len() == 1), "{:?}", v.entry_points);
        v.apply(Act::OnLoad { name: "profileName".into(), args: vec!["\"Mattias\"".into()] });
        for _ in 0..10 {
            v.step_once();
        }
        assert_eq!(v.error, None);
        let line = v.fields.iter().find(|f| f.path.ends_with("player_info_mc.profileName_txt")).expect("the profile line is on stage");
        assert_eq!(line.text, "Mattias");
        let (parent, depth) = (line.parent, line.depth);
        v.apply(Act::SetText { parent, depth, text: "Chris".into() });
        assert_eq!(v.fields.iter().find(|f| f.path.ends_with("profileName_txt")).map(|f| f.text.as_str()), Some("Chris"));
        v.apply(Act::Call { name: "profileName".into(), args: vec!["Mattias".into()] });
        assert!(v.call_errors.contains_key("profileName"), "an unquoted word is not an argument");
    }

    /// Every row of the UI tab, over the stack a default launch opens (`vz-patch.wad` beside the
    /// archive when there is one), opens from the WAD it lists and plays its root timeline through
    /// once, each frame built with every image it draws. A row that does not is named with its error.
    #[test]
    fn every_ui_row_opens_and_draws_every_frame() {
        let vz = crate::app::retail_vz_wad();
        let overlays = crate::app::overlays_for(&vz, true, &[]);
        println!("stack: {vz} + {overlays:?}");
        let mut w = WadStack::open(&vz, &overlays).expect("open retail vz.wad");
        let index = AssetIndex::build(&w.wads, &w.side, crate::index::load_all_names().expect("names"));
        let rows: Vec<_> = index.rows(Kind::UiGfx).to_vec();
        assert!(!rows.is_empty(), "the UI tab lists no rows");
        let shared = Shared::read(&mut w, &[]).expect("fonts and strings");
        let (device, queue) = device();
        let mut failed = Vec::new();
        for row in &rows {
            let label = row.label();
            let result = (|| -> Result<MovieView, String> {
                let mut v = MovieView::open(&mut w, &shared, row.hash, label.clone(), row.src)?;
                let frames = v.def.timeline.frames.len().max(1);
                for _ in 0..frames {
                    v.prepare(&device, &queue, &mut w)?;
                    v.step_once();
                    if let Some(e) = &v.error {
                        return Err(e.clone());
                    }
                }
                // Each drawable export alone, ten frames.
                for export in v.exports.clone() {
                    v.apply(Act::Isolate(export.clone()));
                    for _ in 0..10 {
                        v.prepare(&device, &queue, &mut w).map_err(|e| format!("{export}: {e}"))?;
                        v.step_once();
                        if let Some(e) = &v.error {
                            return Err(format!("{export}: {e}"));
                        }
                    }
                }
                v.apply(Act::Whole);
                Ok(v)
            })();
            match result {
                Ok(v) => {
                    println!("{label}: {} frames, {} reads found nothing, {} entry points", v.frames_run, v.unresolved().len(), v.entry_points.len());
                    for (at, n) in v.unresolved() {
                        println!("  read {at} x{n}");
                    }
                    let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
                    for c in v.def.characters.values() {
                        use mercs2_formats::gfx_def::Character as C;
                        let k = match c {
                            C::Shape(_) => "shape",
                            C::Morph(_) => "morph",
                            C::Sprite(_) => "sprite",
                            C::EditText(_) => "text",
                            C::Font(_) => "font",
                            C::Image(_) => "image",
                        };
                        *kinds.entry(k).or_default() += 1;
                    }
                    println!("  characters {kinds:?} exports {}", v.def.exports.len());
                    println!("  drawable exports {:?}", v.exports);
                    for (url, id, name) in &v.def.imports {
                        println!("  import {url} {name} (char {id})");
                    }
                    for (f, params) in &v.entry_points {
                        println!("  entry {f}({})", params.join(", "));
                    }
                }
                Err(e) => failed.push(format!("{label} (0x{:08X}, {}): {e}", row.hash, w.source_path(row.src))),
            }
        }
        println!("{} rows, {} failed", rows.len(), failed.len());
        assert!(failed.is_empty(), "{} of {} rows failed:\n{}", failed.len(), rows.len(), failed.join("\n"));
    }
}

/// The viewport: the transport over the movie's frame, letterboxed in what is left.
pub fn center(root: &mut egui::Ui, v: &mut MovieView) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::G0).inner_margin(egui::Margin::symmetric(16, 12)))
        .show(root, |ui| {
            ui.horizontal(|ui| {
                let title = match (&v.isolated, &v.focus) {
                    (Some(export), _) => format!("MOVIE \u{b7} {} \u{203a} {export}", v.label),
                    (None, Some(path)) => format!("MOVIE \u{b7} {} \u{203a} {path}", v.label),
                    (None, None) => format!("MOVIE \u{b7} {}", v.label),
                };
                ui.label(theme::disp_text(title.to_uppercase(), theme::style_title(), theme::TX));
                let (at, of) = v.frame();
                ui.label(egui::RichText::new(format!("frame {at} / {of} \u{b7} {} fps \u{b7} {}", v.def.frame_rate, v.wad)).color(theme::DIM));
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if v.playing {
                    if ui.button("Pause").clicked() {
                        acts.push(Act::Pause);
                    }
                } else if ui.add_enabled(v.error.is_none(), egui::Button::new("Play")).clicked() {
                    acts.push(Act::Play);
                }
                if ui.add_enabled(v.error.is_none(), egui::Button::new("Step")).clicked() {
                    acts.push(Act::Step);
                }
                if ui.button("Restart").clicked() {
                    acts.push(Act::Restart);
                }
            });
            if let Some(tl) = v.timeline() {
                ui.add_space(6.0);
                acts.extend(timeline_strip(ui, &tl, v.error.is_none()));
            }
            if let Some(e) = &v.error {
                ui.label(egui::RichText::new(e).color(theme::BAD));
            }
            ui.add_space(8.0);
            let area = ui.available_rect_before_wrap();
            let ppp = ui.ctx().pixels_per_point();
            v.want = [(area.width() * ppp).max(0.0) as u32, (area.height() * ppp).max(0.0) as u32];
            if let (Some(t), Some(size)) = (&v.target, v.fit()) {
                let shown = egui::vec2(size[0] as f32, size[1] as f32) / ppp;
                let rect = egui::Rect::from_center_size(area.center(), shown);
                ui.painter_at(area).image(t.id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            }
        });
    acts
}

/// A timeline as a strip: its sections (label to label) on top, named where the name fits, the one
/// the playhead is in lit; a frame ruler under them with the playhead. A section click goes to its
/// label and plays; a click or drag on the ruler goes to that frame and stops.
fn timeline_strip(ui: &mut egui::Ui, tl: &TimelineView, live: bool) -> Option<Act> {
    const SECTIONS_H: f32 = 22.0;
    const RULER_H: f32 = 12.0;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, SECTIONS_H + RULER_H), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let n = tl.frames.max(1) as f32;
    let x_of = |f: f32| rect.left() + f / n * rect.width();
    let sections_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), SECTIONS_H));
    let ruler = egui::Rect::from_min_max(egui::pos2(rect.left(), sections_rect.bottom()), rect.max);
    let font = theme::font(&egui::TextStyle::Small);
    let mut act = None;
    painter.rect_filled(sections_rect, 2.0, theme::G1);
    for (i, (name, start, end)) in tl.sections().into_iter().enumerate() {
        let r = egui::Rect::from_min_max(egui::pos2(x_of(start as f32), sections_rect.top()), egui::pos2(x_of(end as f32), sections_rect.bottom()));
        let here = (start..end).contains(&tl.at);
        let fill = match (here, name.is_some(), i % 2) {
            (true, _, _) => theme::BRASS_SOFT,
            (false, false, _) => theme::G0,
            (false, true, 0) => theme::G2,
            _ => theme::G3,
        };
        painter.rect_filled(r.shrink2(egui::vec2(0.5, 0.0)), 0.0, fill);
        painter.vline(r.left(), sections_rect.y_range(), egui::Stroke::new(1.0, if here { theme::BRASS } else { theme::LINE2 }));
        let Some(name) = name else { continue };
        let galley = painter.layout_no_wrap(name.to_string(), font.clone(), if here { theme::BRASS } else { theme::TX });
        if galley.size().x + 8.0 <= r.width() {
            painter.galley(egui::pos2(r.left() + 4.0, r.center().y - galley.size().y / 2.0), galley, theme::TX);
        }
        let response = ui
            .interact(r, ui.make_persistent_id(("timeline_section", &tl.whose, i)), egui::Sense::click())
            .on_hover_text(format!("{name} \u{b7} frames {}\u{2013}{}", start + 1, end));
        if response.clicked() && live {
            act = Some(Act::GotoLabel { clip: tl.clip, label: name.to_string() });
        }
    }
    painter.rect_filled(ruler, 0.0, theme::G0);
    for (_, f) in &tl.labels {
        painter.vline(x_of(*f as f32), ruler.y_range(), egui::Stroke::new(1.0, theme::LINE2));
    }
    let head = x_of(tl.at as f32 + 0.5);
    painter.vline(head, rect.y_range(), egui::Stroke::new(2.0, theme::BRASS));
    let response = ui.interact(ruler, ui.make_persistent_id(("timeline_ruler", &tl.whose)), egui::Sense::click_and_drag());
    if let Some(p) = response.interact_pointer_pos().filter(|_| live && (response.clicked() || response.dragged())) {
        let frame = (((p.x - rect.left()) / rect.width() * n).floor() as i64).clamp(0, tl.frames as i64 - 1) as u16;
        if frame != tl.at {
            act = Some(Act::GotoFrame { clip: tl.clip, frame });
        }
    }
    act
}

/// The right column: what the movie is, and what it said while it ran.
pub fn inspector(ui: &mut egui::Ui, v: &mut MovieView, names: &HashMap<u32, String>) -> Vec<Act> {
    let mut acts = Vec::new();
    let stage = v.stage();
    let (at, of) = v.frame();
    theme::card(ui, "Movie", Some(&v.label), |ui| {
        theme::kv(ui, "stage", egui::RichText::new(format!("{} \u{d7} {} px", stage[0], stage[1])));
        theme::kv(ui, "frame rate", egui::RichText::new(format!("{} fps", v.def.frame_rate)));
        theme::kv(ui, "frame", egui::RichText::new(format!("{at} / {of} \u{b7} {} run", v.frames_run)));
        theme::kv(ui, "labels", egui::RichText::new(v.def.timeline.labels.len().to_string()));
        theme::kv(ui, "exports", egui::RichText::new(v.def.exports.len().to_string()));
        theme::kv(ui, "hash", egui::RichText::new(format!("0x{:08X}", v.hash)).monospace());
        theme::kv(ui, "read from", egui::RichText::new(&v.wad));
    });
    let lines = |ui: &mut egui::Ui, title: &str, rows: Vec<String>| {
        theme::section(ui, title, Some(&rows.len().to_string()), !rows.is_empty(), |ui| {
            let row_h = 18.0;
            theme::scroll_vertical().id_salt(title).max_height(220.0).show_rows(ui, row_h, rows.len(), |ui, range| {
                for r in &rows[range] {
                    ui.label(egui::RichText::new(r).monospace());
                }
            });
        });
    };
    // The shown timeline's labels, each the section it starts; the playhead's lit.
    if let Some(tl) = v.timeline() {
        let sections = tl.sections();
        let named = sections.iter().filter(|(n, _, _)| n.is_some()).count();
        theme::section(ui, &format!("Labels \u{b7} {}", tl.whose), Some(&named.to_string()), named > 0, |ui| {
            theme::scroll_vertical().id_salt("movie_labels").max_height(320.0).show(ui, |ui| {
                for (name, start, end) in &sections {
                    let Some(name) = name else { continue };
                    let here = (*start..*end).contains(&tl.at);
                    let text = egui::RichText::new(format!("{name}  {}\u{2013}{}", start + 1, end)).monospace();
                    if ui.selectable_label(here, text).clicked() && v.error.is_none() {
                        acts.push(Act::GotoLabel { clip: tl.clip, label: name.to_string() });
                    }
                }
            });
        });
    }
    // The named clips on stage: one draws alone while the whole movie runs.
    let clips = v.clips.clone();
    theme::section(ui, "Clips on stage", Some(&clips.len().to_string()), false, |ui| {
        if ui.add_enabled(v.focus.is_some(), egui::Button::new("Whole stage")).clicked() {
            acts.push(Act::Focus(None));
        }
        theme::scroll_vertical().id_salt("movie_clips").max_height(320.0).show(ui, |ui| {
            for (_, path) in &clips {
                let on = v.focus.as_deref() == Some(path.as_str());
                if ui.selectable_label(on, egui::RichText::new(path).monospace()).clicked() && !on {
                    acts.push(Act::Focus(Some(path.clone())));
                }
            }
        });
    });
    // The movie's exported clips: one plays alone on the stage, or the whole movie again.
    let exports = v.exports.clone();
    theme::section(ui, "Exports", Some(&exports.len().to_string()), false, |ui| {
        if ui.add_enabled(v.isolated.is_some(), egui::Button::new("Whole movie")).clicked() {
            acts.push(Act::Whole);
        }
        theme::scroll_vertical().id_salt("movie_exports").max_height(320.0).show(ui, |ui| {
            for name in &exports {
                let on = v.isolated.as_deref() == Some(name.as_str());
                if ui.selectable_label(on, egui::RichText::new(name).monospace()).clicked() && !on {
                    acts.push(Act::Isolate(name.clone()));
                }
            }
        });
    });
    // The calls made as the movie loads.
    theme::section(ui, "On load", Some(&v.on_load.len().to_string()), !v.on_load.is_empty(), |ui| {
        for (index, (name, args)) in v.on_load.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{name}({})", args.join(", "))).monospace());
                if ui.small_button("Remove").clicked() {
                    acts.push(Act::RemoveOnLoad(index));
                }
            });
        }
    });
    // Every text field on stage, or in the focused clip: where its text comes from, and the text,
    // which an edit sets.
    let fields: Vec<FieldInfo> = match &v.focus {
        Some(path) => v.fields.iter().filter(|f| f.path.starts_with(&format!("{path}."))).cloned().collect(),
        None => v.fields.clone(),
    };
    theme::section(ui, "Text fields", Some(&fields.len().to_string()), !fields.is_empty(), |ui| {
        theme::scroll_vertical().id_salt("movie_fields").max_height(420.0).show(ui, |ui| {
            for f in &fields {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&f.path).monospace().color(theme::DIM));
                let from = if f.text == f.initial { "movie" } else { "script" };
                theme::kv(ui, "set by", egui::RichText::new(from));
                if !f.variable.is_empty() {
                    theme::kv(ui, "variable", egui::RichText::new(&f.variable).monospace());
                }
                for (key, english) in string_keys(&f.text, &v.strings) {
                    let name = v.key_names.get(&key).or_else(|| names.get(&key)).map_or_else(|| format!("0x{key:08X}"), |n| format!("{n} \u{b7} 0x{key:08X}"));
                    let shown = english.unwrap_or_default();
                    theme::kv(ui, &format!("english \u{b7} {name}"), egui::RichText::new(shown));
                }
                let id = ui.make_persistent_id(("movie_field", &f.path));
                let editing = ui.memory(|m| m.has_focus(id));
                let buffer = v.edits.entry(f.path.clone()).or_default();
                if !editing {
                    buffer.clone_from(&f.text);
                }
                let response = ui.add(egui::TextEdit::singleline(buffer).id(id).desired_width(f32::INFINITY));
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    acts.push(Act::SetText { parent: f.parent, depth: f.depth, text: buffer.clone() });
                }
            }
        });
    });
    // The root's functions: what a host calls, each with its arguments to type and call it with.
    let entries = v.entry_points.clone();
    theme::section(ui, "Host entry points", Some(&entries.len().to_string()), false, |ui| {
        theme::scroll_vertical().id_salt("movie_entries").max_height(420.0).show(ui, |ui| {
            for (name, params) in &entries {
                egui::CollapsingHeader::new(egui::RichText::new(format!("{name}({})", params.join(", "))).monospace())
                    .id_salt(("movie_entry", name))
                    .show(ui, |ui| {
                        let args = v.args.entry(name.clone()).or_insert_with(|| vec![String::new(); params.len()]);
                        for (param, typed) in params.iter().zip(args.iter_mut()) {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(param).monospace());
                                ui.add(egui::TextEdit::singleline(typed).desired_width(f32::INFINITY));
                            });
                        }
                        ui.horizontal(|ui| {
                            if ui.add_enabled(v.error.is_none(), egui::Button::new("Call")).clicked() {
                                acts.push(Act::Call { name: name.clone(), args: args.clone() });
                            }
                            if ui.button("On load").clicked() {
                                acts.push(Act::OnLoad { name: name.clone(), args: args.clone() });
                            }
                        });
                        if let Some(e) = v.call_errors.get(name) {
                            ui.label(egui::RichText::new(e).color(theme::BAD));
                        }
                    });
            }
        });
    });
    lines(ui, "Reads that found nothing", v.unresolved().iter().map(|(at, n)| format!("{at}  \u{d7}{n}")).collect());
    lines(ui, "Traces", v.traces.clone());
    lines(ui, "FSCommands", v.fscommands.iter().map(|(c, a)| format!("{c}({a})")).collect());
    lines(ui, "Called on undefined", v.on_undefined().iter().cloned().collect());
    lines(ui, "Unknown methods", v.missing().iter().cloned().collect());
    lines(ui, "Movie log", v.log().to_vec());
    acts
}

/// Every `[0x…]` string key `text` reads, nested ones included, each with the string table's text for
/// it, found by the same walk that draws the field (`mercs2_gfx::text::translate`).
fn string_keys(text: &str, strings: &HashMap<u32, String>) -> Vec<(u32, Option<String>)> {
    let seen = std::cell::RefCell::new(Vec::new());
    mercs2_engine::gfx::text::translate(text, &|key| {
        let found = strings.get(&key).cloned();
        seen.borrow_mut().push((key, found.clone()));
        found
    });
    seen.into_inner()
}
