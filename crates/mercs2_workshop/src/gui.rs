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
    /// The window — the integration must deliver `PlatformOutput.cursor_icon` to it (egui only
    /// EMITS the desired cursor; nothing changes it otherwise). Drives the hand cursor over buttons.
    window: Arc<Window>,
    /// The cursor egui last requested, so we only call `set_cursor` when it changes.
    cursor: egui::CursorIcon,
}

impl Gui {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Arc<Window>) -> Gui {
        let ctx = egui::Context::default();
        // The workshop's "field-workbench" identity: warm gunmetal, Bahnschrift stencil headings,
        // brass = live/selected, hazard-orange = irreversible. See theme.rs values below.
        theme::install(&ctx);
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
            window: window.clone(),
            cursor: egui::CursorIcon::Default,
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
        // Deliver the cursor: egui sets IBeam over text, resize cursors over panel splitters, etc.;
        // where it leaves Default but the pointer is over an interactive widget, show a hand so
        // clickable elements read as clickable. (`wants_pointer_input()` alone is too broad — it is
        // true over empty panel background — so gate on egui NOT already asking for a cursor AND a
        // widget wanting the click.)
        let mut want = out.platform_output.cursor_icon;
        if want == egui::CursorIcon::Default
            && self.ctx.wants_pointer_input()
            && !self.ctx.wants_keyboard_input()
        {
            want = egui::CursorIcon::PointingHand;
        }
        if want != self.cursor {
            self.cursor = want;
            self.window.set_cursor_icon(to_winit_cursor(want));
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

/// The workshop's visual system — one place for the palette + type + spacing so every panel reads
/// as one tool. Colours and roles mirror the approved redesign mockup: warm gunmetal neutrals,
/// **brass** for what's live/selected, **hazard-orange** reserved for the irreversible.
#[allow(dead_code)] // the palette is a complete token set; not every token is wired yet
pub mod theme {
    use egui::{Color32, FontFamily, FontId, Rounding, Stroke, TextStyle};

    // ── palette (warm gunmetal / painted metal) ──
    pub const G0: Color32 = Color32::from_rgb(0x12, 0x13, 0x16); // app ground
    pub const G1: Color32 = Color32::from_rgb(0x1a, 0x1c, 0x20); // panels
    pub const G2: Color32 = Color32::from_rgb(0x22, 0x25, 0x2b); // cards / inputs
    pub const G3: Color32 = Color32::from_rgb(0x2b, 0x2f, 0x37); // raised / hover
    pub const LINE: Color32 = Color32::from_rgb(0x33, 0x37, 0x3f);
    pub const LINE2: Color32 = Color32::from_rgb(0x42, 0x47, 0x4f);
    pub const TX: Color32 = Color32::from_rgb(0xdc, 0xd8, 0xce); // warm neutral text
    pub const DIM: Color32 = Color32::from_rgb(0x9a, 0x95, 0x8a);
    pub const FAINT: Color32 = Color32::from_rgb(0x67, 0x63, 0x5a);
    // semantic accents
    pub const BRASS: Color32 = Color32::from_rgb(0xe6, 0xb2, 0x3c); // live / selected
    pub const BRASS_DK: Color32 = Color32::from_rgb(0xa6, 0x7c, 0x22);
    pub const BRASS_SOFT: Color32 = Color32::from_rgb(0x35, 0x30, 0x1c); // brass @ ~12% over G1
    pub const HAZARD: Color32 = Color32::from_rgb(0xe8, 0x76, 0x3a); // irreversible only
    pub const HAZARD_SOFT: Color32 = Color32::from_rgb(0x34, 0x24, 0x1a);
    pub const GOOD: Color32 = Color32::from_rgb(0x8f, 0xbf, 0x4f);
    // Unreal-style vector-field axis strips.
    pub const AXIS_X: Color32 = Color32::from_rgb(0xc8, 0x55, 0x4e);
    pub const AXIS_Y: Color32 = Color32::from_rgb(0x7c, 0xaa, 0x46);
    pub const AXIS_Z: Color32 = Color32::from_rgb(0x4f, 0x86, 0xc6);
    pub const GOOD_SOFT: Color32 = Color32::from_rgb(0x20, 0x2a, 0x17); // green @ ~12% over the ground
    pub const GOOD_DK: Color32 = Color32::from_rgb(0x53, 0x73, 0x2c);
    pub const BAD: Color32 = Color32::from_rgb(0xd5, 0x60, 0x4c);
    pub const INFO: Color32 = Color32::from_rgb(0x63, 0xa6, 0xcf);

    /// The condensed industrial display family (Bahnschrift, shipped on Windows). Falls back to the
    /// proportional stack when absent so `FontFamily::Name("disp")` always resolves.
    pub fn disp() -> FontFamily {
        FontFamily::Name("disp".into())
    }

    fn load_font(defs: &mut egui::FontDefinitions, key: &str, paths: &[&str]) -> bool {
        for p in paths {
            if let Ok(bytes) = std::fs::read(p) {
                defs.font_data.insert(key.to_owned(), egui::FontData::from_owned(bytes));
                return true;
            }
        }
        false
    }

    pub fn install(ctx: &egui::Context) {
        // ── fonts ──
        let mut fonts = egui::FontDefinitions::default();
        // Body: prefer Segoe UI (the native Windows UI face) ahead of egui's default proportional.
        if load_font(&mut fonts, "segoe", &["C:/Windows/Fonts/segoeui.ttf"]) {
            fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".to_owned());
        }
        // Display: Bahnschrift for the stencil eyebrows / headings.
        let disp_key = if load_font(&mut fonts, "disp_ttf", &["C:/Windows/Fonts/bahnschrift.ttf"]) {
            // Bahnschrift sits high in its line box vs Segoe — nudge the baseline down so disp labels
            // and body/mono text vertically centre together (visible on the command bar).
            if let Some(fd) = fonts.font_data.get_mut("disp_ttf") {
                fd.tweak.y_offset_factor = 0.09;
            }
            vec!["disp_ttf".to_owned()]
        } else {
            // No Bahnschrift: alias the display family to the proportional stack.
            fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default()
        };
        fonts.families.insert(FontFamily::Name("disp".into()), disp_key);
        ctx.set_fonts(fonts);

        // ── type scale + visuals ──
        let mut style = (*ctx.style()).clone();
        let disp = FontFamily::Name("disp".into());
        style.text_styles.insert(TextStyle::Heading, FontId::new(18.0, disp.clone()));
        style.text_styles.insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace));

        let mut v = egui::Visuals::dark();
        v.panel_fill = G1;
        v.window_fill = G2;
        v.window_stroke = Stroke::new(1.0, LINE2);
        v.extreme_bg_color = G0;
        v.faint_bg_color = G2;
        v.override_text_color = Some(TX);
        v.hyperlink_color = BRASS;
        v.selection.bg_fill = BRASS_SOFT;
        v.selection.stroke = Stroke::new(1.0, BRASS);
        v.window_rounding = Rounding::same(7.0);
        let round = Rounding::same(5.0);
        // widgets
        v.widgets.noninteractive.bg_fill = G1;
        v.widgets.noninteractive.weak_bg_fill = G1;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, DIM);
        v.widgets.noninteractive.rounding = round;
        v.widgets.inactive.bg_fill = G2;
        v.widgets.inactive.weak_bg_fill = G2;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TX);
        v.widgets.inactive.rounding = round;
        v.widgets.hovered.bg_fill = G3;
        v.widgets.hovered.weak_bg_fill = G3;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, LINE2);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, TX);
        v.widgets.hovered.rounding = round;
        v.widgets.active.bg_fill = G3;
        v.widgets.active.weak_bg_fill = G3;
        v.widgets.active.bg_stroke = Stroke::new(1.0, BRASS_DK);
        v.widgets.active.fg_stroke = Stroke::new(1.0, BRASS);
        v.widgets.active.rounding = round;
        v.widgets.open.bg_fill = G2;
        v.widgets.open.weak_bg_fill = G2;
        v.widgets.open.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.open.rounding = round;
        style.visuals = v;

        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(9.0, 4.0);
        style.spacing.window_margin = egui::Margin::same(10.0);
        style.spacing.menu_margin = egui::Margin::same(6.0);
        ctx.set_style(style);
    }

    /// A HUD chip drawn over the viewport (Orbit / clip position / legend). `on` = lit brass;
    /// `dot` paints a small status square before the label. Non-interactive (status only).
    pub fn chip(ui: &mut egui::Ui, label: &str, on: bool, dot: Option<Color32>) {
        let (fg, bg, stroke) = if on {
            (BRASS, BRASS_SOFT, BRASS_DK)
        } else {
            (DIM, Color32::from_rgba_unmultiplied(14, 16, 20, 205), LINE)
        };
        egui::Frame::none()
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, stroke))
            .rounding(egui::Rounding::same(3.0))
            .inner_margin(egui::Margin::symmetric(9.0, 4.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    if let Some(c) = dot {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, egui::Rounding::same(1.0), c);
                    }
                    ui.label(disp_text(label.to_uppercase(), 9.5, fg));
                });
            });
    }

    /// A stencil eyebrow label (Bahnschrift, uppercased, dim) — the section-header voice.
    pub fn eyebrow(ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add(egui::Label::new(
            egui::RichText::new(text.to_uppercase())
                .family(disp())
                .size(11.0)
                .color(DIM),
        ))
    }

    /// Display-family rich text at a chosen size/colour (headings, chips, titles).
    pub fn disp_text(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
        egui::RichText::new(text.into()).family(disp()).size(size).color(color)
    }

    /// A framed inspector card: a rounded panel with a stencil eyebrow header (brass tick + title +
    /// optional right-aligned badge) and the body below. This is the defining inspector element.
    pub fn card<R>(
        ui: &mut egui::Ui,
        title: &str,
        badge: Option<&str>,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        egui::Frame::none()
            .fill(G2)
            .stroke(egui::Stroke::new(1.0, LINE))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::symmetric(11.0, 9.0))
            .outer_margin(egui::Margin { bottom: 10.0, ..Default::default() })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, egui::Rounding::ZERO, BRASS_DK);
                    ui.add_space(3.0);
                    ui.label(disp_text(title.to_uppercase(), 11.0, DIM));
                    if let Some(b) = badge {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(b).monospace().size(10.0).color(FAINT));
                        });
                    }
                });
                ui.add_space(3.0);
                ui.separator();
                ui.add_space(5.0);
                add(ui)
            })
            .inner
    }

    /// A framed panel with no eyebrow header — wraps a collapsible section so it reads as a card
    /// alongside the `card()`s while keeping its own collapse control.
    pub fn panel<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        egui::Frame::none()
            .fill(G2)
            .stroke(egui::Stroke::new(1.0, LINE))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::symmetric(10.0, 7.0))
            .outer_margin(egui::Margin { bottom: 10.0, ..Default::default() })
            .show(ui, add)
            .inner
    }

    /// A COLLAPSIBLE framed inspector section: the `card()` look (rounded panel, brass-tick stencil
    /// eyebrow + optional badge) but with a persistent expand/collapse toggle. The body has NO inner
    /// scroll area — an open section shows its full content and the single outer inspector scroll
    /// handles the length, so nothing is squeezed into a tiny sub-window. `title` must be STATIC (it
    /// is the persistence key); put dynamic counts in `badge`.
    pub fn section(
        ui: &mut egui::Ui,
        title: &str,
        badge: Option<&str>,
        default_open: bool,
        add: impl FnOnce(&mut egui::Ui),
    ) {
        egui::Frame::none()
            .fill(G2)
            .stroke(egui::Stroke::new(1.0, LINE))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::symmetric(11.0, 8.0))
            .outer_margin(egui::Margin { bottom: 10.0, ..Default::default() })
            .show(ui, |ui| {
                let id = ui.make_persistent_id(("sect", title));
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open)
                    .show_header(ui, |ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, egui::Rounding::ZERO, BRASS_DK);
                        ui.add_space(3.0);
                        ui.label(disp_text(title.to_uppercase(), 11.0, DIM));
                        if let Some(b) = badge {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(b).monospace().size(10.0).color(FAINT));
                            });
                        }
                    })
                    .body(|ui| {
                        ui.add_space(5.0);
                        add(ui);
                    });
            });
    }

    /// A full-width framed, clickable row (the LOD-tier / segment / clip chip). `fill`/`border` carry
    /// the state colour (green = drawn/passing, brass = selected, neutral = idle). Returns the row's
    /// click response; add the row's columns inside `add`.
    pub fn row_chip<R>(
        ui: &mut egui::Ui,
        fill: egui::Color32,
        border: egui::Color32,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::Response {
        let ir = egui::Frame::none()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, border))
            .rounding(egui::Rounding::same(5.0))
            .inner_margin(egui::Margin::symmetric(9.0, 5.0))
            .outer_margin(egui::Margin { bottom: 4.0, ..Default::default() })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_width(ui.available_width());
                    add(ui);
                });
            });
        ir.response.interact(egui::Sense::click())
    }

    /// A small rounded toggle pill (destruction states, filters). Brass when `on`, dim when off.
    pub fn pill(ui: &mut egui::Ui, label: &str, on: bool) -> egui::Response {
        let (fill, stroke, txt) = if on { (BRASS_SOFT, BRASS_DK, BRASS) } else { (G0, LINE, DIM) };
        egui::Frame::none()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke))
            .rounding(egui::Rounding::same(4.0))
            .inner_margin(egui::Margin::symmetric(8.0, 3.0))
            .outer_margin(egui::Margin { right: 4.0, bottom: 4.0, ..Default::default() })
            .show(ui, |ui| {
                ui.label(disp_text(label, 10.0, txt));
            })
            .response
            .interact(egui::Sense::click())
    }

    /// A key → value row inside a card body: dim label left, tabular mono value right-aligned.
    pub fn kv(ui: &mut egui::Ui, key: &str, value: egui::RichText) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(key).color(DIM).size(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(value.monospace().size(11.5));
            });
        });
    }

    /// A 26×26 LOD/state bit chip. Returns whether it was clicked.
    pub fn bit_chip(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
        let (fill, stroke, txt) =
            if on { (BRASS_SOFT, BRASS_DK, BRASS) } else { (G0, LINE, FAINT) };
        let p = ui.painter();
        p.rect(rect, egui::Rounding::same(4.0), fill, egui::Stroke::new(1.0, stroke));
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(11.0),
            txt,
        );
        resp.clicked()
    }

    /// A single Unreal-style scrub cell: a flat inset field with a coloured axis strip on the left
    /// edge, drag-to-scrub anywhere on it, click-to-type. `strip` = the axis colour (X/Y/Z), or
    /// `FAINT` for a plain scalar. Returns the `DragValue` response so callers can test `.changed()`.
    /// `total_w` is the WHOLE cell footprint; the inner drag value is sized to fill what's left after
    /// the axis strip + margins, so callers pass a stretched width and the field fills it.
    fn scrub_cell(
        ui: &mut egui::Ui,
        strip: Color32,
        value: &mut f32,
        speed: f32,
        total_w: f32,
    ) -> egui::Response {
        // A fixed-width cell painted by hand so the value can be LEFT-aligned in tabular monospace
        // (UE style) — every column's digits then start at the same x. add_sized would centre them.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, 22.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect(rect, egui::Rounding::same(3.0), G0, egui::Stroke::new(1.0, LINE));
        p.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + 2.0),
                egui::vec2(3.0, rect.height() - 4.0),
            ),
            egui::Rounding { nw: 2.0, sw: 2.0, ne: 0.0, se: 0.0 },
            strip,
        );
        let inner = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 9.0, rect.top()),
            egui::pos2(rect.right() - 4.0, rect.bottom()),
        );
        ui.allocate_ui_at_rect(inner, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // Flatten the DragValue chrome so it reads as a field, not a button.
                let w = ui.visuals_mut();
                w.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                w.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                w.widgets.hovered.weak_bg_fill = Color32::TRANSPARENT;
                w.widgets.hovered.bg_stroke = egui::Stroke::NONE;
                w.widgets.active.weak_bg_fill = Color32::TRANSPARENT;
                w.widgets.active.bg_stroke = egui::Stroke::NONE;
                ui.style_mut().override_font_id = Some(egui::FontId::monospace(11.0));
                ui.add(
                    egui::DragValue::new(value)
                        .speed(speed)
                        // Fixed 3 decimals + a leading space for non-negatives, so in monospace the
                        // sign column and decimal points line up down every column (no ragged X).
                        .custom_formatter(|n, _| {
                            if n < 0.0 {
                                format!("{n:.3}")
                            } else {
                                format!(" {n:.3}")
                            }
                        })
                        .custom_parser(|s| s.trim().parse::<f64>().ok()),
                )
            })
            .inner
        })
        .inner
    }

    /// The shared label-column width (a UE-style splitter): the caption takes the left HALF of the
    /// row, the value fields share the right half, so all fields start at the same x panel-wide.
    pub fn field_label_w(avail: f32) -> f32 {
        (avail * 0.5).max(60.0)
    }

    /// A left-aligned caption cell of the shared width; truncates long text (full text on hover).
    fn field_label(ui: &mut egui::Ui, label: &str) {
        let lw = field_label_w(ui.available_width());
        ui.allocate_ui_with_layout(
            egui::vec2(lw, 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                // Force the column to the full splitter width even for short captions, so every
                // row's value fields line up (allocate_ui advances by content size otherwise).
                ui.set_min_width(lw);
                ui.add(
                    egui::Label::new(disp_text(label.to_uppercase(), 9.5, DIM))
                        .truncate()
                        .selectable(false),
                )
                .on_hover_text(label);
            },
        );
    }

    /// An Unreal Details-panel vector row: a left-aligned caption in the shared column, then X/Y/Z
    /// scrub cells (red/green/blue) that STRETCH to fill the value column. Empty label = cells only.
    pub fn vec3_field(ui: &mut egui::Ui, label: &str, v: &mut [f32; 3], speed: f32) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if !label.is_empty() {
                field_label(ui, label);
            }
            let cell = ((ui.available_width() - 12.0) / 3.0).max(30.0);
            changed |= scrub_cell(ui, AXIS_X, &mut v[0], speed, cell).changed();
            changed |= scrub_cell(ui, AXIS_Y, &mut v[1], speed, cell).changed();
            changed |= scrub_cell(ui, AXIS_Z, &mut v[2], speed, cell).changed();
        });
        changed
    }

    /// A column-header row matching `vec3_field`'s geometry: a caption in the splitter column, then
    /// X / Y / Z headers centred over the three value columns, axis-coloured. Pair with striped
    /// rows to read as a table.
    pub fn vec3_header(ui: &mut egui::Ui, label: &str) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let lw = field_label_w(ui.available_width());
            ui.allocate_ui_with_layout(
                egui::vec2(lw, 14.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(lw);
                    ui.label(disp_text(label.to_uppercase(), 8.5, FAINT));
                },
            );
            let cell = ((ui.available_width() - 12.0) / 3.0).max(30.0);
            for (t, c) in [("X", AXIS_X), ("Y", AXIS_Y), ("Z", AXIS_Z)] {
                ui.allocate_ui_with_layout(
                    egui::vec2(cell, 14.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(cell);
                        // Match the value's left inset (strip + gap) so the header sits over the digits.
                        ui.add_space(9.0);
                        ui.label(disp_text(t, 9.0, c));
                    },
                );
            }
        });
    }

    /// Subtle band fill for alternating table rows (zebra striping). `odd` rows get the band.
    pub fn row_stripe(odd: bool) -> Color32 {
        if odd {
            Color32::from_rgb(0x1b, 0x1d, 0x22)
        } else {
            Color32::TRANSPARENT
        }
    }

    /// A single-value Details row (scale, angle, …): caption + one neutral field that fills the
    /// value column.
    pub fn scalar_field(ui: &mut egui::Ui, label: &str, value: &mut f32, speed: f32) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if !label.is_empty() {
                field_label(ui, label);
            }
            let cell = (ui.available_width() - 4.0).max(40.0);
            changed = scrub_cell(ui, FAINT, value, speed, cell).changed();
        });
        changed
    }

    /// A filled brass "go" button (Place / Merge / Apply). Dimmed when disabled.
    pub fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
        let fg = if enabled { Color32::from_rgb(0x1c, 0x16, 0x06) } else { FAINT };
        let bg = if enabled { BRASS } else { G2 };
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(label).color(fg).strong())
                .fill(bg)
                .stroke(egui::Stroke::new(1.0, if enabled { BRASS } else { LINE })),
        )
    }

    /// A hazard-orange "irreversible" button (Publish / Clear / Delete).
    pub fn danger_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
        ui.add_enabled(
            enabled,
            egui::Button::new(disp_text(label.to_uppercase(), 12.0, HAZARD))
                .fill(HAZARD_SOFT)
                .stroke(egui::Stroke::new(1.0, HAZARD)),
        )
    }

    /// The activity-rail glyphs.
    #[derive(Clone, Copy)]
    pub enum RailIcon {
        Inspect,
        Sandbox,
        Mods,
        Skeleton,
        Log,
    }

    fn paint_icon(p: &egui::Painter, icon: RailIcon, c: egui::Pos2, col: Color32) {
        use egui::vec2;
        let s = egui::Stroke::new(1.5, col);
        match icon {
            RailIcon::Inspect => {
                p.circle_stroke(c + vec2(-1.5, -1.5), 5.0, s);
                p.line_segment([c + vec2(1.8, 1.8), c + vec2(6.0, 6.0)], s);
            }
            RailIcon::Sandbox => {
                let r = 6.0;
                let top = c + vec2(0.0, -r);
                let l = c + vec2(-r, -r * 0.4);
                let rr = c + vec2(r, -r * 0.4);
                let bl = c + vec2(-r, r * 0.5);
                let br = c + vec2(r, r * 0.5);
                let bot = c + vec2(0.0, r);
                for seg in [[top, l], [top, rr], [l, bl], [rr, br], [bl, bot], [br, bot], [l, rr], [top, bot]] {
                    p.line_segment(seg, s);
                }
            }
            RailIcon::Mods => {
                p.circle_stroke(c + vec2(-3.5, -3.5), 3.0, s);
                p.line_segment([c + vec2(-1.2, -1.2), c + vec2(6.0, 6.0)], s);
                p.line_segment([c + vec2(6.0, 6.0), c + vec2(6.5, 3.5)], s);
            }
            RailIcon::Skeleton => {
                p.circle_stroke(c + vec2(-4.0, -4.0), 2.2, s);
                p.circle_stroke(c + vec2(4.0, 4.0), 2.2, s);
                p.line_segment([c + vec2(-2.6, -2.6), c + vec2(2.6, 2.6)], s);
            }
            RailIcon::Log => {
                p.circle_stroke(c, 6.0, s);
                p.line_segment([c, c + vec2(0.0, -3.5)], s);
                p.line_segment([c, c + vec2(2.8, 1.5)], s);
            }
        }
    }

    /// One activity-rail entry: index + icon + label, with a brass left-bar + soft fill when active.
    /// Returns whether it was clicked.
    pub fn rail_item(ui: &mut egui::Ui, index: Option<usize>, label: &str, icon: RailIcon, on: bool) -> bool {
        let w = ui.available_width();
        // Square cell: height == width, so the icon+label block reads as a tidy square button.
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, w), egui::Sense::click());
        let col = if on { BRASS } else if resp.hovered() { DIM } else { FAINT };
        let p = ui.painter();
        if on {
            p.rect_filled(rect, egui::Rounding::ZERO, BRASS_SOFT);
            let bar = egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + 6.0),
                egui::vec2(3.0, rect.height() - 12.0),
            );
            p.rect_filled(bar, egui::Rounding { ne: 2.0, se: 2.0, ..Default::default() }, BRASS);
        } else if resp.hovered() {
            p.rect_filled(rect, egui::Rounding::ZERO, Color32::from_rgba_unmultiplied(255, 255, 255, 6));
        }
        if let Some(i) = index {
            p.text(
                rect.left_top() + egui::vec2(7.0, 5.0),
                egui::Align2::LEFT_TOP,
                format!("{i:02}"),
                egui::FontId::monospace(8.0),
                if on { BRASS } else { FAINT },
            );
        }
        let cx = rect.center().x;
        // Icon above the centre, label below — the pair centred within the square cell.
        paint_icon(p, icon, egui::pos2(cx, rect.center().y - 8.0), col);
        p.text(
            egui::pos2(cx, rect.center().y + 15.0),
            egui::Align2::CENTER_CENTER,
            label.to_uppercase(),
            egui::FontId::new(8.5, disp()),
            col,
        );
        resp.clicked()
    }

    /// A small square status dot + label (the "READY" pill in the status bar).
    pub fn status_dot(ui: &mut egui::Ui, label: &str, color: Color32) {
        let (r, _) = ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
        ui.painter().rect_filled(r, egui::Rounding::same(1.0), color);
        ui.add_space(2.0);
        ui.label(disp_text(label.to_uppercase(), 9.5, color));
    }

    /// The command-bar diamond brand mark (a filled brass rhombus with a dark inner cut — the game's
    /// spade-skull emblem stand-in).
    pub fn brand_mark(ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
        // Align the diamond's centre to the brand text's optical (cap-height) centre. Measured: the
        // allocated box centres ~1.5px below the caps, so lift the diamond by that much. (Painted
        // shapes get no font metrics, so this can't come from the layout.)
        let c = rect.center() - egui::vec2(0.0, 1.5);
        let diamond = |r: f32| {
            vec![
                c + egui::vec2(0.0, -r),
                c + egui::vec2(r, 0.0),
                c + egui::vec2(0.0, r),
                c + egui::vec2(-r, 0.0),
            ]
        };
        let p = ui.painter();
        // Outer filled diamond, then a gunmetal inner diamond → a bold brass ring emblem.
        p.add(egui::Shape::convex_polygon(diamond(11.0), BRASS, egui::Stroke::NONE));
        p.add(egui::Shape::convex_polygon(diamond(7.0), G0, egui::Stroke::NONE));
        p.add(egui::Shape::convex_polygon(diamond(3.2), BRASS, egui::Stroke::NONE));
    }
}

/// egui → winit cursor icon (the subset the tool produces; anything else falls back to the arrow).
fn to_winit_cursor(c: egui::CursorIcon) -> winit::window::CursorIcon {
    use egui::CursorIcon as E;
    use winit::window::CursorIcon as W;
    match c {
        E::PointingHand => W::Pointer,
        E::Text | E::VerticalText => W::Text,
        E::Crosshair => W::Crosshair,
        E::Move => W::Move,
        E::Grab => W::Grab,
        E::Grabbing => W::Grabbing,
        E::NotAllowed | E::NoDrop => W::NotAllowed,
        E::Wait => W::Wait,
        E::Progress => W::Progress,
        E::Help => W::Help,
        E::ResizeHorizontal | E::ResizeEast | E::ResizeWest => W::EwResize,
        E::ResizeVertical | E::ResizeNorth | E::ResizeSouth => W::NsResize,
        E::ResizeNeSw | E::ResizeNorthEast | E::ResizeSouthWest => W::NeswResize,
        E::ResizeNwSe | E::ResizeNorthWest | E::ResizeSouthEast => W::NwseResize,
        E::ResizeColumn => W::ColResize,
        E::ResizeRow => W::RowResize,
        _ => W::Default,
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
