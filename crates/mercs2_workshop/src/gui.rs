//! egui host for the workshop: a hand-rolled winit-0.29 → egui event bridge plus the egui-wgpu
//! paint path, rendered through the engine's `Scene` overlay hook (`render_with` /
//! `render_menu_with`). Hand-rolled because `egui-winit` 0.28 targets winit 0.30 while the
//! engine is on 0.29 — the bridge below is the ~10% of it this tool needs.

use std::sync::Arc;

use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

pub struct Gui {
    pub ctx: egui::Context,
    renderer: egui_wgpu::Renderer,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    pointer: egui::Pos2,
    ppp: f32,
    size: [u32; 2],
    /// Output of the last `run` (painted by `paint` inside the overlay hook).
    jobs: Vec<egui::ClippedPrimitive>,
    tex_delta: egui::TexturesDelta,
    /// Wall-clock epoch for `RawInput.time`. egui does NOT read the clock itself — with
    /// `time: None` it counts frames at an assumed 60 fps, so on a fast-rendering app a normal
    /// 150 ms click "lasts" several egui-seconds and gets voided by the 0.8 s click limit
    /// (diagnosed from a live trace: 9 clean press/release pairs, only a fast tap clicked).
    start: std::time::Instant,
    /// OS clipboard (lazy): egui only EMITS copied text via `PlatformOutput`; the integration
    /// must deliver it — this is what makes the context menus' "Copy …" actions real.
    clipboard: Option<arboard::Clipboard>,
}

impl Gui {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Arc<Window>) -> Gui {
        let ctx = egui::Context::default();
        // Dark theme tuned toward the shell's gold-on-dark.
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(148, 110, 34);
        style.visuals.hyperlink_color = egui::Color32::from_rgb(242, 189, 64);
        ctx.set_style(style);
        let size = window.inner_size();
        Gui {
            ctx,
            renderer: egui_wgpu::Renderer::new(device, format, None, 1),
            events: Vec::new(),
            modifiers: egui::Modifiers::default(),
            pointer: egui::Pos2::ZERO,
            ppp: window.scale_factor() as f32,
            size: [size.width, size.height],
            jobs: Vec::new(),
            tex_delta: egui::TexturesDelta::default(),
            start: std::time::Instant::now(),
            clipboard: None,
        }
    }

    /// Feed a winit event. Returns true when egui CONSUMED it (pointer over a panel, text into a
    /// widget) — the caller should then skip its own camera/shortcut handling.
    pub fn on_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::Resized(s) => {
                self.size = [s.width, s.height];
                false
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ppp = *scale_factor as f32;
                false
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.modifiers = egui::Modifiers {
                    alt: s.alt_key(),
                    ctrl: s.control_key(),
                    shift: s.shift_key(),
                    mac_cmd: false,
                    command: s.control_key(),
                };
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = egui::pos2(position.x as f32 / self.ppp, position.y as f32 / self.ppp);
                self.events.push(egui::Event::PointerMoved(self.pointer));
                self.ctx.is_using_pointer()
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => egui::PointerButton::Primary,
                    MouseButton::Right => egui::PointerButton::Secondary,
                    MouseButton::Middle => egui::PointerButton::Middle,
                    _ => return false,
                };
                let pressed = *state == ElementState::Pressed;
                self.events.push(egui::Event::PointerButton {
                    pos: self.pointer,
                    button,
                    pressed,
                    modifiers: self.modifiers,
                });
                // Hover-based: `wants_pointer_input` alone misses the PRESS (its any-down state
                // is a frame behind), which would start a camera drag under the panel.
                self.ctx.is_pointer_over_area() || self.ctx.wants_pointer_input()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (unit, d) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (egui::MouseWheelUnit::Line, egui::vec2(*x, *y))
                    }
                    MouseScrollDelta::PixelDelta(p) => (
                        egui::MouseWheelUnit::Point,
                        egui::vec2(p.x as f32 / self.ppp, p.y as f32 / self.ppp),
                    ),
                };
                self.events.push(egui::Event::MouseWheel { unit, delta: d, modifiers: self.modifiers });
                self.ctx.is_pointer_over_area() || self.ctx.wants_pointer_input()
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state, text, repeat, .. },
                ..
            } => {
                if let Some(key) = map_key(*code) {
                    self.events.push(egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: *state == ElementState::Pressed,
                        repeat: *repeat,
                        modifiers: self.modifiers,
                    });
                }
                if *state == ElementState::Pressed && self.ctx.wants_keyboard_input() {
                    if let Some(t) = text {
                        let printable: String =
                            t.chars().filter(|c| !c.is_control()).collect();
                        if !printable.is_empty() {
                            self.events.push(egui::Event::Text(printable));
                        }
                    }
                }
                self.ctx.wants_keyboard_input()
            }
            _ => false,
        }
    }

    /// Run one GUI frame: `build` lays out the panels; the paint jobs are stashed for `paint`.
    pub fn run(&mut self, build: impl FnOnce(&egui::Context)) {
        let screen =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(self.size[0] as f32, self.size[1] as f32) / self.ppp);
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(self.start.elapsed().as_secs_f64()),
            modifiers: self.modifiers,
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        };
        raw.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point =
            Some(self.ppp);
        let out = self.ctx.run(raw, |ctx| build(ctx));
        // Deliver copy actions (context menus, Ctrl+C in text fields) to the OS clipboard.
        if !out.platform_output.copied_text.is_empty() {
            if self.clipboard.is_none() {
                self.clipboard = arboard::Clipboard::new()
                    .map_err(|e| eprintln!("[gui] clipboard unavailable: {e}"))
                    .ok();
            }
            if let Some(cb) = &mut self.clipboard {
                if let Err(e) = cb.set_text(out.platform_output.copied_text.clone()) {
                    eprintln!("[gui] clipboard write failed: {e}");
                }
            }
        }
        self.jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        self.tex_delta = out.textures_delta;
    }

    /// Paint the last `run` inside the engine's overlay hook (own render pass on the swapchain).
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        for (id, delta) in &self.tex_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }
        let desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: self.ppp,
        };
        self.renderer.update_buffers(device, queue, encoder, &self.jobs, &desc);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.renderer.render(&mut pass, &self.jobs, &desc);
        }
        for id in &self.tex_delta.free {
            self.renderer.free_texture(id);
        }
        self.tex_delta = egui::TexturesDelta::default();
    }
}

/// winit → egui key map (the subset the inspector uses; unmapped keys still reach the app's own
/// shortcut handler).
fn map_key(code: KeyCode) -> Option<egui::Key> {
    use egui::Key as K;
    Some(match code {
        KeyCode::ArrowUp => K::ArrowUp,
        KeyCode::ArrowDown => K::ArrowDown,
        KeyCode::ArrowLeft => K::ArrowLeft,
        KeyCode::ArrowRight => K::ArrowRight,
        KeyCode::Enter | KeyCode::NumpadEnter => K::Enter,
        KeyCode::Escape => K::Escape,
        KeyCode::Tab => K::Tab,
        KeyCode::Backspace => K::Backspace,
        KeyCode::Delete => K::Delete,
        KeyCode::Space => K::Space,
        KeyCode::Home => K::Home,
        KeyCode::End => K::End,
        KeyCode::PageUp => K::PageUp,
        KeyCode::PageDown => K::PageDown,
        KeyCode::KeyA => K::A,
        KeyCode::KeyC => K::C,
        KeyCode::KeyV => K::V,
        KeyCode::KeyX => K::X,
        KeyCode::KeyZ => K::Z,
        _ => return None,
    })
}
