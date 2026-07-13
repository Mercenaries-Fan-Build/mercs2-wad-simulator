//! The engine-owned application shell: one winit event loop + a `Game` hook trait.
//!
//! Today two near-verbatim frame loops exist — `game_world::run_game_world` (dev free-fly) and
//! `mercs2_game::world::run_scene_world_loading` (the TPS game). Their window/cursor/mouse/`LayerStack`/
//! fixed-step/loading/render bodies are copies. This module is the single home for that machinery: the
//! engine owns the window, event loop, raw input plumbing, `Time`, the `LayerStack`, the background-load
//! polling + loading-screen render, the shared `World`, and the per-frame render; a `Game` implementor
//! supplies only POLICY (its config, its loaded data type, camera, sim, menu/HUD) through the hooks.
//!
//! Boundary rule (`docs/modernization/pangea_engine_alignment.md`): mechanism → engine; selection /
//! content / tunables → game. `Game::setup` generalizes `run_game_world`'s one-shot `populate` closure;
//! the per-frame stanzas become `update` / `fixed_update` / `render_prep` / `ui`. Anything whose *type*
//! is game-specific — the loaded world data, an optional `StreamingWorld`, the asset handles — is owned
//! by the `Game` (via the associated `LoadData` type and its own fields), so the engine loop stays fully
//! generic over both the free-fly and TPS boots.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use mercs2_core::frame::{LayerStack, LayerTransition, LAYER_GAME};
use mercs2_core::glam::Mat4;
use mercs2_core::glam::Vec3;
use mercs2_core::{Time, World};
use winit::event::{DeviceEvent, ElementState, Event, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowBuilder};

use crate::input::{Bindings, Gamepad, Input};
use crate::render::LoadProgress;
use crate::scene::Scene;

/// The loading layer sits one below the game layer (the recovered 0→4 climb, `FUN_004c15e0`): the
/// background loader runs while it is active, then loader-completion raises the target to `LAYER_GAME`.
const LAYER_LOADING: usize = LAYER_GAME - 1;
/// The shell-menu (frontend) layer sits one below loading: a boot with a menu starts here, renders its
/// menu each frame, and raises the target to loading once the player picks a save.
const LAYER_MENU: usize = LAYER_GAME - 2;

/// What the shell menu wants the engine loop to do after a frame on the menu layer.
pub enum MenuOutcome {
    /// Keep showing the menu (the game rendered it this frame).
    Stay,
    /// A save was picked — the game has stored its selection; the engine spawns the loader and climbs.
    StartLoad,
    /// Quit the game.
    Exit,
}

/// Static configuration the engine reads ONCE, before it opens the window. Replaces the positional
/// argument soup of `run_scene_world_loading` and the inline fog/sun/plate setup: the game declares
/// *what* world it wants; the engine performs the *how* of standing it up.
pub struct GameConfig {
    /// Window title bar text.
    pub title: String,
    /// Initial inner size, logical pixels.
    pub size: (f64, f64),
    /// Grab + hide the cursor on boot (world play); leave free while a menu is up.
    pub grab_cursor: bool,
    /// Distance fog: (`rgb`, density, start). Interior vs exterior differ — a game choice.
    pub fog: ([f32; 3], f32, f32),
    /// Directional key light (`azimuth`, `elevation`); `None` = no outdoor sun (interior).
    pub sun: Option<(f32, f32)>,
    /// Sky/atmosphere tunables (game content; engine renders them). `None` leaves the scene's default
    /// atmosphere untouched (the TPS boot relies on fog-only, no explicit atmosphere).
    pub atmosphere: Option<mercs2_formats::atmosphere::Atmosphere>,
    /// Path to the base `vz.wad`; the engine resolves the `shell.wad` loading plate as a sibling. `None`
    /// = spinner only.
    pub loading_plate_wad: Option<String>,
    /// Number of load stages for the progress bar.
    pub load_stages: u32,
    /// The action→key/pad binding table (data-driven from `Mercs2.ini`). `Bindings::default()` is fine
    /// for boots that read raw keys (the dev free-fly cam).
    pub bindings: Bindings,
}

/// The camera a `Game::update` produces for this frame. `pos` is exposed so a game that owns streaming
/// can step it around the camera; `view` + near/far drive `Scene::set_view`.
pub struct Camera {
    pub view: Mat4,
    pub pos: Vec3,
    pub near: f32,
    pub far: f32,
}

/// Everything a hook may touch this frame, lent by the engine per phase.
///
/// `world` is the shared HANDLE (`&Rc<RefCell<World>>`), never a live outstanding borrow: hooks take
/// their own narrow `world.borrow()/borrow_mut()` and drop it before the next statement. This lets a
/// `Game` keep its own `Rc<RefCell<World>>` clone (e.g. inside its script host) without ever colliding
/// with an engine-held borrow. Do NOT change the hooks to take `&mut World` — a live `&mut World` across
/// a re-borrowing hook reintroduces the "already borrowed" panic against that clone.
pub struct Ctx<'a> {
    pub world: &'a Rc<RefCell<World>>,
    pub scene: &'a mut Scene,
    pub input: &'a Input<'a>,
    /// Keys that transitioned to pressed THIS frame (rising edges) — for menu nav + the Tab cam toggle.
    pub pressed: &'a std::collections::HashSet<KeyCode>,
    /// This frame's selected-source mouse delta in pixels (dual-source raw/absolute already resolved).
    /// The game applies its own sensitivity/inversion.
    pub mouse_delta: (f32, f32),
    pub time: &'a Time,
    pub window: &'a Window,
    /// Real (variable) delta seconds this frame — for camera/look smoothing in `update`.
    pub dt: f32,
}

/// A game is content + per-frame policy over the engine's generic loop. The engine calls, in order per
/// rendered frame once the world is up: `setup` (once, on load completion), then each frame `update` →
/// `fixed_update` × N → `render_prep` → `ui` → render.
pub trait Game {
    /// The game-specific result of the background load (e.g. `StreamingWorldData` or `WorldData`). Owned
    /// by the game; the engine only ferries it from the loader thread to `setup`.
    type LoadData: Send + 'static;

    /// Declared once, before the window opens.
    fn config(&self) -> GameConfig;

    /// Does this boot begin on the shell-menu layer (default: no — load immediately)? A menu boot shows
    /// `menu` each frame until it returns `StartLoad`.
    fn starts_at_menu(&self) -> bool {
        false
    }

    /// Called each frame while on the menu layer: the game handles nav (via `ctx.pressed` +
    /// `ctx.input`), draws + renders its shell, and returns whether to stay / start loading / quit. On
    /// `StartLoad` the game has stored its save selection so `spawn_loader` knows what to load. Only
    /// invoked when `starts_at_menu` is true; the default is unused.
    fn menu(&mut self, _ctx: &mut Ctx) -> MenuOutcome {
        MenuOutcome::StartLoad
    }

    /// Spawn the background loader thread and return the channel its result lands on. The engine polls
    /// it while on the loading layer, rendering the plate/spinner off `progress`. For a menu boot it is
    /// called on `StartLoad`, so it may read a selection the game stored during `menu`.
    fn spawn_loader(&self, progress: Arc<LoadProgress>) -> Receiver<Result<Self::LoadData, String>>;

    /// The world finished loading — realize the loaded `data` into the live World/Scene (base terrain,
    /// player, PMC interior, mission objects, lights, and — for a streaming boot — build the game's own
    /// `StreamingWorld`). Generalizes `run_game_world`'s one-shot `populate` closure.
    fn setup(&mut self, ctx: &mut Ctx, data: Self::LoadData);

    /// Variable-rate, once per rendered frame, BEFORE the fixed steps: drain input onto the camera rig +
    /// player look, run locomotion / streaming step, and return the view.
    fn update(&mut self, ctx: &mut Ctx) -> Camera;

    /// One fixed simulation step; the engine calls it `steps` times at `ctx.time.fixed_dt`.
    fn fixed_update(&mut self, ctx: &mut Ctx);

    /// After the fixed steps: drain game→engine per-frame FX/render intents. Default no-op.
    fn render_prep(&mut self, _ctx: &mut Ctx) {}

    /// The 2D pass over the engine `ui` overlay (shell menu + HUD). Default no-op.
    fn ui(&mut self, _ctx: &mut Ctx) {}
}

/// The single engine-owned event loop. Opens the window from `game.config()`, drives the background
/// loader + loading screen, and once the world is realized runs the fixed-step frame spine, calling the
/// game's hooks. This is the one home for the window/cursor/mouse/`LayerStack`/`Time`/render machinery
/// that `run_game_world` and `run_scene_world_loading` each used to duplicate.
pub async fn run<G: Game + 'static>(mut game: G) {
    fn grab_cursor(window: &Window) {
        if let Err(e) = window
            .set_cursor_grab(CursorGrabMode::Confined)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))
        {
            eprintln!("[app] cursor grab unavailable ({e}); arrow keys still steer");
        }
        window.set_cursor_visible(false);
    }

    let cfg = game.config();
    let starts_at_menu = game.starts_at_menu();

    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(cfg.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(cfg.size.0, cfg.size.1))
            .build(&event_loop)
            .expect("window"),
    );
    // A menu boot keeps the cursor free/visible until a save is picked; grab happens on `StartLoad`.
    if cfg.grab_cursor && !starts_at_menu {
        grab_cursor(&window);
    }

    let mut scene = Scene::new(window.clone()).await;
    scene.set_fog(cfg.fog.0, cfg.fog.1, cfg.fog.2);
    if let Some((az, el)) = cfg.sun {
        scene.set_sun(az, el);
    }
    if let Some(atmos) = cfg.atmosphere.clone() {
        scene.set_atmosphere(atmos);
    }
    if let Some(wadpath) = &cfg.loading_plate_wad {
        match crate::wad::shell_loading_plate(wadpath) {
            Ok(td) => scene.set_loading_art(&td),
            Err(e) => eprintln!("[app] loading art unavailable ({e}); spinner only"),
        }
    }

    let progress = Arc::new(LoadProgress::new(cfg.load_stages));
    // A menu boot defers the loader until the player picks a save (`StartLoad`); a direct boot starts it
    // now. `None` = not yet spawned.
    let mut rx: Option<Receiver<Result<G::LoadData, String>>> =
        (!starts_at_menu).then(|| game.spawn_loader(progress.clone()));

    // The ECS World is the single source of truth, a shared single-threaded handle so a game's script
    // host can hold its own clone (see `Ctx`).
    let world = Rc::new(RefCell::new(World::new()));

    let bindings = cfg.bindings;
    let grab_on_load = cfg.grab_cursor;
    let mut gamepad = Gamepad::new();
    let mut held: std::collections::HashSet<KeyCode> = std::collections::HashSet::new();
    let mut prev_held: std::collections::HashSet<KeyCode> = std::collections::HashSet::new();
    let mut mouse_btns: std::collections::HashSet<MouseButton> = std::collections::HashSet::new();

    let mut time = Time::new(60.0);
    let mut layers = LayerStack::at(if starts_at_menu { LAYER_MENU } else { LAYER_LOADING });
    let mut pending: Option<G::LoadData> = None;

    let mut load_start = std::time::Instant::now();
    let mut bar_shown = 0.0f32;
    let mut bar_last_t = 0.0f32;
    let mut last = std::time::Instant::now();
    // Dual-source mouse accumulators: CursorMoved (absolute, recentred) vs DeviceEvent raw delta. A
    // Shadow-PC-style absolute raw stream is detected and ignored in favour of the recentre path.
    let mut mouse_acc: (f32, f32) = (0.0, 0.0);
    let mut mouse_raw_acc: (f32, f32) = (0.0, 0.0);
    let mut mouse_src: u8 = 0;
    let mut mouse_sane_events: u32 = 0;

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { window_id, event } if window_id == scene.window.id() => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::KeyboardInput {
                    event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. },
                    ..
                } => match (code, state) {
                    // Escape quits IN-WORLD; on the menu layer it falls through to a press edge the shell
                    // reads as Back (so it doesn't quit the game from the menu).
                    (KeyCode::Escape, ElementState::Pressed) if layers.active() != LAYER_MENU => elwt.exit(),
                    (c, ElementState::Pressed) => {
                        held.insert(c);
                    }
                    (c, ElementState::Released) => {
                        held.remove(&c);
                    }
                },
                WindowEvent::MouseInput { button, state, .. } => match state {
                    ElementState::Pressed => {
                        mouse_btns.insert(button);
                    }
                    ElementState::Released => {
                        mouse_btns.remove(&button);
                    }
                },
                WindowEvent::Resized(size) => scene.resize(size),
                WindowEvent::CursorMoved { position, .. } => {
                    // On the menu layer the cursor stays free/visible — no look-accumulate, no recentre.
                    if layers.active() == LAYER_MENU {
                        return;
                    }
                    let (cx, cy) = (scene.size.width as f64 / 2.0, scene.size.height as f64 / 2.0);
                    mouse_acc.0 += (position.x - cx) as f32;
                    mouse_acc.1 += (position.y - cy) as f32;
                    let _ = scene.window.set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                }
                WindowEvent::RedrawRequested => {
                    let now = std::time::Instant::now();
                    let real_dt = (now - last).as_secs_f32().min(0.1);
                    last = now;
                    // Rising key edges this frame (menu nav + Tab toggle) + poll the gamepad up front so
                    // both the menu and the game layers see fresh input.
                    let pressed: std::collections::HashSet<KeyCode> = held.difference(&prev_held).copied().collect();
                    prev_held = held.clone();
                    gamepad.update();

                    // MENU layer: the shell owns the frame — nav + draw + render live in the game. On
                    // StartLoad, spawn the loader (reading the game's stored selection), grab the cursor,
                    // reset the loading clock, and climb.
                    if layers.active() == LAYER_MENU {
                        let input = Input { bindings: &bindings, keys: &held, mouse: &mouse_btns, gamepad: &gamepad };
                        let mut ctx = Ctx { world: &world, scene: &mut scene, input: &input, pressed: &pressed, mouse_delta: (0.0, 0.0), time: &time, window: &window, dt: real_dt };
                        match game.menu(&mut ctx) {
                            MenuOutcome::Stay => return,
                            MenuOutcome::Exit => {
                                elwt.exit();
                                return;
                            }
                            MenuOutcome::StartLoad => {
                                rx = Some(game.spawn_loader(progress.clone()));
                                if grab_on_load {
                                    grab_cursor(&window);
                                }
                                load_start = std::time::Instant::now();
                                // Actually LEAVE the menu layer this frame — `set_target` alone only moves
                                // the target, so without climbing here `active()` stays LAYER_MENU and the
                                // menu block re-runs forever (the loader completes but the LOADING poll +
                                // GAME realize below never run — the world loads yet the screen stays on
                                // the shell). Advance to LOADING so next frame drives the loading screen.
                                layers.set_target(LAYER_LOADING);
                                while !layers.settled() {
                                    let _ = layers.advance();
                                }
                                return;
                            }
                        }
                    }

                    // LOADING layer: poll the background loader; on completion raise the target.
                    if layers.active() == LAYER_LOADING {
                        if let Some(rx) = rx.as_ref() {
                            match rx.try_recv() {
                                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    eprintln!("[app] loader thread died");
                                    elwt.exit();
                                    return;
                                }
                                Ok(Err(e)) => {
                                    eprintln!("[app] load failed: {e}");
                                    elwt.exit();
                                    return;
                                }
                                Ok(Ok(data)) => {
                                    pending = Some(data);
                                    layers.set_target(LAYER_GAME);
                                }
                            }
                        }
                    }
                    // Climb the layer stack; realize the world exactly once on entering the GAME layer.
                    while !layers.settled() {
                        if let Some(LayerTransition::Ascending(LAYER_GAME)) = layers.advance() {
                            let Some(data) = pending.take() else { continue };
                            let input = Input { bindings: &bindings, keys: &held, mouse: &mouse_btns, gamepad: &gamepad };
                            let mut ctx = Ctx { world: &world, scene: &mut scene, input: &input, pressed: &pressed, mouse_delta: (0.0, 0.0), time: &time, window: &window, dt: real_dt };
                            game.setup(&mut ctx, data);
                        }
                    }

                    // While still loading, render the plate/spinner and stop here.
                    if layers.active() != LAYER_GAME {
                        let t = load_start.elapsed().as_secs_f32();
                        let dt = (t - bar_last_t).max(0.0);
                        bar_last_t = t;
                        bar_shown += (progress.fraction() - bar_shown) * (1.0 - (-6.0 * dt).exp());
                        match scene.render_loading(t, bar_shown) {
                            Ok(()) => {}
                            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => scene.resize(scene.size),
                            Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                            Err(e) => eprintln!("surface error: {e:?}"),
                        }
                        return;
                    }

                    // GAME layer: fixed-step spine + the game hooks.
                    let steps = time.advance_frame(real_dt);
                    // Resolve this frame's mouse delta from the selected source, then clear both.
                    let mouse_delta = if mouse_src == 1 { mouse_raw_acc } else { mouse_acc };
                    mouse_acc = (0.0, 0.0);
                    mouse_raw_acc = (0.0, 0.0);

                    let input = Input { bindings: &bindings, keys: &held, mouse: &mouse_btns, gamepad: &gamepad };
                    let mut ctx = Ctx { world: &world, scene: &mut scene, input: &input, pressed: &pressed, mouse_delta, time: &time, window: &window, dt: real_dt };
                    let cam = game.update(&mut ctx);
                    for _ in 0..steps {
                        game.fixed_update(&mut ctx);
                    }
                    game.render_prep(&mut ctx);
                    game.ui(&mut ctx);

                    scene.set_view(cam.view, cam.near, cam.far);
                    match scene.render(&world.borrow()) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => scene.resize(scene.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                        Err(e) => eprintln!("surface error: {e:?}"),
                    }
                }
                _ => {}
            },
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                let (dx, dy) = (delta.0 as f32, delta.1 as f32);
                if mouse_src != 2 {
                    if dx.abs() > 2000.0 || dy.abs() > 2000.0 {
                        mouse_src = 2;
                        eprintln!("[app] absolute-coordinate raw input detected -> cursor-recentre mode");
                    } else {
                        mouse_raw_acc.0 += dx;
                        mouse_raw_acc.1 += dy;
                        if mouse_src == 0 && (dx != 0.0 || dy != 0.0) {
                            mouse_sane_events += 1;
                            if mouse_sane_events >= 10 {
                                mouse_src = 1;
                            }
                        }
                    }
                }
            }
            Event::AboutToWait => scene.window.request_redraw(),
            _ => {}
        })
        .expect("event loop run");
}
