//! Inspect's VFX tab (`docs/modernization/workshop_pages/inspect_tabs.md`, VFX): an effect on the
//! pedestal, run by the retail sim (`mercs2_engine::fx_sim`) and drawn from the `vfx` atlas.
//!
//! [`VfxView::open`] starts the effect and works out how long it runs and how much room it takes, by
//! running a copy of it. It repeats when it ends. A template that starts the effect decides how long
//! it emits and how many particles a metre of travel adds, so the card lists every such template.

use crate::gui::theme;
use glam::Vec3;
use mercs2_engine::fx_sim::{EffectSim, Starter};
use mercs2_engine::particles::EmitterId;
use mercs2_engine::scene::Scene;
use mercs2_formats::fxdict::{attribute_name, Atrb, AtrbValue, EffectContainer, ForceKind, FxRect};
use mercs2_formats::schema::FieldValue;
use mercs2_formats::worldentity::{Value, WorldEntity};
use std::collections::HashMap;

/// The class naming the effect a template starts, and its fields by offset (`FUN_00488d70`).
const RED_EFFECT: &str = "RedEffectComponent";
const NAME_OFFSET: u16 = 0;
const PER_METRE_OFFSET: u16 = 8;
const DURATION_OFFSET: u16 = 0x10;
const DURATION_VAR_OFFSET: u16 = 0x14;
const HAS_DURATION_OFFSET: u16 = 0x18;

/// The scrub's step: one update at the 60 Hz the curves count in.
const SEEK_STEP: f32 = 1.0 / 60.0;
/// How far ahead [`measure`] looks, and the scrub's length for an effect that does not end.
const MEASURE_SECONDS: f32 = 10.0;
/// The room a sprite takes beyond its centre, as a share of its edge.
const SPRITE_REACH: f32 = 0.7;
/// The instance's path when it travels, either side of the pedestal.
const TRAVEL_REACH: f32 = 20.0;

/// The room a slider's value box takes beside its rail.
const SLIDER_VALUE_WIDTH: f32 = 96.0;
const COLR_STRIP_HEIGHT: f32 = 14.0;
const FRAME_THUMB: f32 = 40.0;
const CURVE_HEIGHT: f32 = 36.0;

/// One template that starts an effect.
#[derive(Debug, Clone, PartialEq)]
pub struct StartedBy {
    pub template: String,
    pub starter: Starter,
    /// The duration's `±`.
    pub duration_var: f32,
}

fn number(values: &[Value], at: usize) -> Option<f32> {
    match values.get(at) {
        Some(Value::Field(FieldValue::F32(x))) => Some(*x),
        Some(Value::Field(FieldValue::U32(x))) => Some(*x as f32),
        Some(Value::Field(FieldValue::Bits(x))) => Some(*x as f32),
        _ => None,
    }
}

/// Every template that starts an effect, by the effect's hash, in template order. `names` are the
/// templates' `Name`s by key.
pub fn starters(we: &WorldEntity, names: &HashMap<u32, String>) -> Result<HashMap<u32, Vec<StartedBy>>, String> {
    let mut out: HashMap<u32, Vec<StartedBy>> = HashMap::new();
    for c in we.groups_of(RED_EFFECT) {
        let at = |offset: u16| {
            c.schema.fields.iter().position(|f| f.byte_offset == offset).ok_or_else(|| format!("{RED_EFFECT} has no field at +0x{offset:02X}"))
        };
        let (name, per_metre, duration, var, has) =
            (at(NAME_OFFSET)?, at(PER_METRE_OFFSET)?, at(DURATION_OFFSET)?, at(DURATION_VAR_OFFSET)?, at(HAS_DURATION_OFFSET)?);
        for r in &c.records {
            let v = c.decode(&r.payload)?;
            let Some(Value::Field(FieldValue::U32(effect))) = v.get(name) else {
                return Err(format!("{RED_EFFECT}.name of keys {:08X?} reads {:?}", r.keys, v.get(name)));
            };
            let read = |i: usize| number(&v, i).ok_or_else(|| format!("{RED_EFFECT} field {i} of keys {:08X?} reads {:?}", r.keys, v.get(i)));
            let timed = read(has)? != 0.0;
            let starter = Starter { duration: if timed { read(duration)? } else { 0.0 }, per_metre: read(per_metre)? };
            let duration_var = if timed { read(var)? } else { 0.0 };
            for k in &r.keys {
                let template = names.get(k).cloned().unwrap_or_else(|| format!("0x{k:08X}"));
                out.entry(*effect).or_default().push(StartedBy { template, starter, duration_var });
            }
        }
    }
    Ok(out)
}

/// What a run of an effect shows: when it ends and the room it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    /// Seconds to its end, or [`MEASURE_SECONDS`] for one that goes on.
    pub span: f32,
    pub centre: Vec3,
    pub radius: f32,
}

/// Run a copy of `sim` to its end, or for [`MEASURE_SECONDS`].
pub fn measure(sim: &EffectSim) -> Measure {
    let mut run = sim.clone();
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    while run.time() < MEASURE_SECONDS && !run.is_finished() {
        run.step(SEEK_STEP);
        run.sprites(Vec3::X, Vec3::Y, Vec3::Z, |s| {
            let reach = Vec3::splat(s.size.abs() * SPRITE_REACH);
            lo = lo.min(Vec3::from(s.pos) - reach);
            hi = hi.max(Vec3::from(s.pos) + reach);
        });
    }
    let span = run.time();
    if lo.x > hi.x {
        return Measure { span, centre: sim.position(), radius: 1.0 };
    }
    Measure { span, centre: (lo + hi) * 0.5, radius: ((hi - lo).length() * 0.5).max(0.5) }
}

/// One action of the transport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VfxAct {
    Restart,
    Pause,
    Seek(f32),
    Speed(f32),
    /// The instance's travel, metres a second.
    Travel(f32),
    Starter(Option<usize>),
}

/// The open effect.
pub struct VfxView {
    pub hash: u32,
    pub label: String,
    pub effect: EffectContainer,
    pub started_by: Vec<StartedBy>,
    /// The template whose [`Starter`] it runs under; `None` runs it with none.
    pub chosen: Option<usize>,
    pub id: EmitterId,
    pub paused: bool,
    pub speed: f32,
    pub travel: f32,
    /// Where the instance is along its path, and which way it is going.
    along: f32,
    heading: f32,
    pub measure: Measure,
    /// The clock and live particles as of this frame.
    pub time: f32,
    pub live: usize,
}

impl VfxView {
    /// Start `effect` at the origin.
    pub fn open(hash: u32, label: String, effect: EffectContainer, started_by: Vec<StartedBy>, rects: &HashMap<u32, FxRect>, scene: &mut Scene) -> Result<VfxView, String> {
        let chosen = (!started_by.is_empty()).then_some(0);
        let starter = chosen.map_or(Starter::default(), |i| started_by[i].starter);
        let sim = EffectSim::new(&effect, rects, starter, Vec3::ZERO, glam::Quat::IDENTITY, 1)?;
        let m = measure(&sim);
        let id = scene.fx_start_effect(&effect, starter, [0.0; 3], true)?;
        Ok(VfxView { hash, label, effect, started_by, chosen, id, paused: false, speed: 1.0, travel: 0.0, along: 0.0, heading: 1.0, measure: m, time: 0.0, live: 0 })
    }

    /// The camera that frames it: target and distance.
    pub fn framing(&self) -> (Vec3, f32) {
        (self.measure.centre, (self.measure.radius * 3.0).clamp(1.5, 15000.0))
    }

    /// Advance what the transport drives and read the clock.
    pub fn frame(&mut self, dt: f32, scene: &mut Scene) {
        let Some(e) = scene.fx_effect_mut(self.id) else { return };
        if self.travel != 0.0 && !self.paused {
            self.along += self.heading * self.travel * dt * self.speed;
            if self.along.abs() > TRAVEL_REACH {
                self.along = self.along.clamp(-TRAVEL_REACH, TRAVEL_REACH);
                self.heading = -self.heading;
            }
        }
        if self.travel == 0.0 {
            self.along = 0.0;
        }
        e.sim.set_position(Vec3::new(self.along, 0.0, 0.0));
        e.paused = self.paused;
        e.speed = self.speed;
        self.time = e.sim.time();
        self.live = e.sim.live_particles();
    }

    pub fn act(&mut self, act: VfxAct, rects: &HashMap<u32, FxRect>, scene: &mut Scene) -> Result<(), String> {
        match act {
            VfxAct::Restart => {
                if let Some(e) = scene.fx_effect_mut(self.id) {
                    e.sim.restart();
                }
                self.paused = false;
            }
            VfxAct::Pause => self.paused = !self.paused,
            VfxAct::Seek(t) => {
                if let Some(e) = scene.fx_effect_mut(self.id) {
                    e.sim.seek(t, SEEK_STEP);
                }
                self.paused = true;
            }
            VfxAct::Speed(s) => self.speed = s,
            VfxAct::Travel(v) => self.travel = v,
            VfxAct::Starter(i) => {
                self.chosen = i;
                let starter = i.map_or(Starter::default(), |i| self.started_by[i].starter);
                scene.fx_remove_effect(self.id);
                let sim = EffectSim::new(&self.effect, rects, starter, Vec3::ZERO, glam::Quat::IDENTITY, 1)?;
                self.measure = measure(&sim);
                self.id = scene.fx_start_effect(&self.effect, starter, [0.0; 3], true)?;
            }
        }
        Ok(())
    }

    pub fn close(self, scene: &mut Scene) {
        scene.fx_remove_effect(self.id);
    }
}

fn value_text(a: &Atrb) -> String {
    match a.value {
        AtrbValue::F32(v) => format!("{v:.3}"),
        AtrbValue::U32(v) => v.to_string(),
    }
}

fn attribute_label(hash: u32) -> String {
    attribute_name(hash).map_or_else(|| format!("0x{hash:08X}"), str::to_string)
}

/// One attribute: its name and value, and its curve plotted when it has one.
fn attribute_row(ui: &mut egui::Ui, a: &Atrb) {
    theme::kv(ui, &attribute_label(a.hash), egui::RichText::new(value_text(a)).small());
    if let Some(keys) = a.curve.as_ref().filter(|k| k.len() > 1) {
        let (x0, x1) = (keys[0].time, keys[keys.len() - 1].time);
        theme::curve(ui, CURVE_HEIGHT, x0, x1, x0, |x| {
            match keys.iter().position(|k| k.time > x) {
                None => keys[keys.len() - 1].value,
                Some(0) => keys[0].value,
                Some(i) => {
                    let (a, b) = (keys[i - 1], keys[i]);
                    a.value + (b.value - a.value) * ((x - a.time) / (b.time - a.time))
                }
            }
        });
    }
}

fn colr_strip(ui: &mut egui::Ui, e: &mercs2_formats::fxdict::Emitter) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), COLR_STRIP_HEIGHT), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let keys = &e.particle.colr.keys;
    let w = rect.width() / keys.len() as f32;
    for (i, k) in keys.iter().enumerate() {
        let [r, g, b, a] = k.rgba;
        let x = rect.left() + i as f32 * w;
        p.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w + 0.5, rect.height())),
            0.0,
            egui::Color32::from_rgba_unmultiplied(r, g, b, a.max(24)),
        );
    }
}

/// The emitter's sprite frames from the atlas.
fn frame_thumbs(ui: &mut egui::Ui, e: &mercs2_formats::fxdict::Emitter, rects: &HashMap<u32, FxRect>, atlas: Option<&egui::TextureHandle>) {
    let Some(atlas) = atlas else { return };
    ui.horizontal_wrapped(|ui| {
        for k in &e.particle.text.frames {
            let uv = match rects.get(k) {
                Some(r) => egui::Rect::from_min_size(egui::pos2(r.u, r.top()), egui::vec2(r.w, r.h)),
                None => egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            };
            let image = egui::Image::new(atlas).uv(uv).fit_to_exact_size(egui::vec2(FRAME_THUMB, FRAME_THUMB));
            ui.add(image).on_hover_text(format!("0x{k:08X}"));
        }
    });
}

/// The effect's inspector.
pub fn inspector(ui: &mut egui::Ui, view: &VfxView, rects: &HashMap<u32, FxRect>, atlas: Option<&egui::TextureHandle>) -> Vec<VfxAct> {
    let mut acts = Vec::new();
    theme::card(ui, "Effect", Some(&format!("0x{:08X}", view.hash)), |ui| {
        ui.add(egui::Label::new(egui::RichText::new(&view.label).color(theme::TX)).truncate());
        theme::kv(ui, "emitters", egui::RichText::new(view.effect.emitters.len().to_string()));
        theme::kv(ui, "forces", egui::RichText::new(view.effect.forces.len().to_string()));
        theme::kv(ui, "particles", egui::RichText::new(view.live.to_string()));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Restart").clicked() {
                acts.push(VfxAct::Restart);
            }
            if ui.button(if view.paused { "Play" } else { "Pause" }).clicked() {
                acts.push(VfxAct::Pause);
            }
        });
        let width = (ui.available_width() - SLIDER_VALUE_WIDTH).max(0.0);
        let mut t = view.time.min(view.measure.span);
        let scrub = egui::Slider::new(&mut t, 0.0..=view.measure.span).suffix(" s").fixed_decimals(2);
        if theme::scrub(ui, width, scrub).changed() {
            acts.push(VfxAct::Seek(t));
        }
        let mut speed = view.speed;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("speed").color(theme::DIM));
            for (s, text) in [(0.25, "¼×"), (0.5, "½×"), (1.0, "1×"), (2.0, "2×")] {
                if ui.selectable_label(speed == s, text).clicked() {
                    speed = s;
                }
            }
        });
        if speed != view.speed {
            acts.push(VfxAct::Speed(speed));
        }
        let mut travel = view.travel;
        ui.label(egui::RichText::new("travel").color(theme::DIM));
        if theme::scrub(ui, width, egui::Slider::new(&mut travel, 0.0..=30.0).suffix(" m/s")).changed() {
            acts.push(VfxAct::Travel(travel));
        }
    });
    if !view.started_by.is_empty() {
        theme::card(ui, "Started by", Some(&view.started_by.len().to_string()), |ui| {
            theme::scroll_vertical().id_salt("vfx_started_by").max_height(120.0).auto_shrink([false, true]).show(ui, |ui| {
                for (i, s) in view.started_by.iter().enumerate() {
                    let text = egui::RichText::new(&s.template).small();
                    if ui.selectable_label(view.chosen == Some(i), text).clicked() {
                        acts.push(VfxAct::Starter(Some(i)));
                    }
                }
            });
            if let Some(s) = view.chosen.map(|i| &view.started_by[i]) {
                ui.add_space(4.0);
                let duration = if s.starter.duration > 0.0 { format!("{:.2} ± {:.2} s", s.starter.duration, s.duration_var) } else { "none".to_string() };
                theme::kv(ui, "duration", egui::RichText::new(duration));
                theme::kv(ui, "per metre", egui::RichText::new(format!("{:.2}", s.starter.per_metre)));
            }
        });
    }
    for (i, e) in view.effect.emitters.iter().enumerate() {
        theme::card(ui, &format!("Emitter {i}"), None, |ui| {
            colr_strip(ui, e);
            ui.add_space(4.0);
            frame_thumbs(ui, e, rects, atlas);
            let records = e.geom.and_then(|g| view.effect.shapes.get(g.shape_index as usize)).map_or(0, |s| s.records.len());
            let area: f32 = e.geom.and_then(|g| view.effect.shapes.get(g.shape_index as usize)).map_or(0.0, |s| s.records.iter().map(|r| r[0]).sum());
            theme::kv(ui, "spawn shape", egui::RichText::new(format!("{records} triangles, {area:.2} m²")));
            let half = (e.particle.colr.keys[0].half_bits & 0x8000) != 0;
            theme::kv(ui, "blend", egui::RichText::new(if half { "adds" } else { "alpha" }));
            theme::kv(ui, "facing", egui::RichText::new(if e.particle.flags & 1 != 0 { "velocity" } else { "camera" }));
            theme::kv(ui, "frames", egui::RichText::new(if e.particle.flags & 2 != 0 { "flipbook" } else { "over life" }));
            egui::CollapsingHeader::new("Attributes").id_salt(("vfx_attributes", i)).show(ui, |ui| {
                for a in e.particle.attributes.iter().chain(e.channels.iter()) {
                    attribute_row(ui, a);
                }
            });
        });
    }
    if !view.effect.forces.is_empty() {
        theme::card(ui, "Forces", Some(&view.effect.forces.len().to_string()), |ui| {
            for (i, f) in view.effect.forces.iter().enumerate() {
                let kind = match f.kind {
                    ForceKind::Gravity { .. } => "Gravity",
                    ForceKind::Drag { .. } => "Drag",
                    ForceKind::Wind { .. } => "Wind",
                    ForceKind::Attractor { .. } => "Attractor",
                    ForceKind::Vortex { .. } => "Vortex",
                };
                egui::CollapsingHeader::new(kind).id_salt(("vfx_force", i)).show(ui, |ui| {
                    for a in &f.attributes {
                        attribute_row(ui, a);
                    }
                });
            }
        });
    }
    acts
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::fxdict::{
        AttrDef, Colr, Emitter, EmitterGeom, EmitterShape, ParticleType, Text, ValueKind, PTYP_ATTRIBUTES_AFTER_COLR, PTYP_ATTRIBUTES_BEFORE_COLR,
        TRFM_CHANNELS,
    };

    fn zero(d: &AttrDef) -> Atrb {
        match d.kind {
            ValueKind::F32 => Atrb::f32(d.hash, 0.0),
            ValueKind::U32 => Atrb::u32(d.hash, 0),
        }
    }

    /// A one-emitter effect with `rate`, `life` and `size`, its particles rising at 2 m/s.
    fn effect(rate: f32, life: f32, size: f32) -> EffectContainer {
        let mut attributes: Vec<Atrb> = PTYP_ATTRIBUTES_BEFORE_COLR.iter().chain(PTYP_ATTRIBUTES_AFTER_COLR.iter()).map(zero).collect();
        for (name, v) in [("rate", rate), ("life", life), ("size", size), ("mass", 1.0), ("speed", 2.0)] {
            let h = mercs2_formats::hash::pandemic_hash_m2(name);
            attributes.iter_mut().find(|a| a.hash == h).expect("an attribute").value = AtrbValue::F32(v);
        }
        let mut record = [0.0; 13];
        record[2] = 1.0;
        let scl: [u32; 3] = [0xC557_4C5F, 0xE355_3D02, 0x655C_C56D];
        EffectContainer {
            shapes: vec![EmitterShape { records: vec![record] }],
            emitters: vec![Emitter {
                transform: [[0.0; 4]; 4],
                channels: TRFM_CHANNELS.iter().map(|d| if scl.contains(&d.hash) { Atrb::f32(d.hash, 1.0) } else { zero(d) }).collect(),
                geom: Some(EmitterGeom { shape_index: 0, word_00: 1 }),
                particle: ParticleType { flags: 0, attributes, colr: Colr::uniform([255; 4], 0x3C00), text: Text { frames: vec![1] } },
            }],
            forces: vec![],
        }
    }

    fn sim(e: &EffectContainer) -> EffectSim {
        EffectSim::new(e, &HashMap::new(), Starter::default(), Vec3::ZERO, glam::Quat::IDENTITY, 1).unwrap()
    }

    /// A held rate goes on, so the scrub runs the look-ahead; the column rises above the origin.
    #[test]
    fn a_measure_holds_where_the_particles_go() {
        let m = measure(&sim(&effect(50.0, 1.0, 1.0)));
        assert_eq!(m.span.round(), MEASURE_SECONDS);
        assert!(m.centre.y > 0.5 && m.radius > 1.0, "{m:?}");
    }

    /// An effect that emits nothing still frames, and ends on the idle rule.
    #[test]
    fn a_silent_effect_measures_to_its_idle_end() {
        let m = measure(&sim(&effect(0.0, 1.0, 1.0)));
        assert!((m.span - 3.0).abs() < 0.1, "{m:?}");
        assert_eq!(m.centre, Vec3::ZERO);
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;
    use crate::app::WadStack;
    use mercs2_engine::game_world::{load_effect, load_fx_atlas};
    use mercs2_engine::wad;
    use mercs2_formats::types::TYPE_ID_EFFECT;

    fn stack() -> WadStack {
        let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        WadStack::open(path.to_str().expect("a UTF-8 path"), &[]).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Templates name retail effects, with the durations and per-metre rates they run under.
    #[test]
    fn templates_start_effects_with_their_durations() {
        let mut w = stack();
        let t = crate::layerview::Templates::read(&mut w).unwrap_or_else(|e| panic!("{e}"));
        let s = starters(&t.we, &t.names).unwrap_or_else(|e| panic!("{e}"));
        let all: Vec<&StartedBy> = s.values().flatten().collect();
        assert!(s.get(&0x41B4_326E).is_some_and(|v| !v.is_empty()), "global_explosion_c4 has a template");
        println!("[vfx] {} templates start {} effects; {} timed, {} per metre", all.len(), s.len(), all.iter().filter(|x| x.starter.duration > 0.0).count(), all.iter().filter(|x| x.starter.per_metre != 0.0).count());
        assert!(all.iter().any(|x| x.starter.per_metre != 0.0));
        assert!(all.iter().any(|x| x.starter.duration > 0.0));
    }

    /// Every retail effect measures to a span and a finite room, and reports what the rows need:
    /// how many have no name, and the C4's spread.
    #[test]
    fn every_retail_effect_measures() {
        let mut w = stack();
        let base = &mut w.wads[0];
        let (_, rects) = load_fx_atlas(base).unwrap_or_else(|e| panic!("{e}"));
        let rects: HashMap<u32, FxRect> = rects.into_iter().map(|r| (r.key, r)).collect();
        let hashes: Vec<u32> = wad::all_asets(base).into_iter().filter(|(_, ty, _)| *ty == TYPE_ID_EFFECT).map(|(h, _, _)| h).collect();
        for h in &hashes {
            let fx = load_effect(base, *h).unwrap_or_else(|e| panic!("{e}")).expect("an effect");
            let s = EffectSim::new(&fx, &rects, Starter::default(), Vec3::ZERO, glam::Quat::IDENTITY, 1).unwrap_or_else(|e| panic!("0x{h:08X}: {e}"));
            let m = measure(&s);
            assert!(m.radius.is_finite() && m.centre.is_finite() && m.span > 0.0, "0x{h:08X}: {m:?}");
        }
        assert_eq!(hashes.len(), 314);
    }
}
