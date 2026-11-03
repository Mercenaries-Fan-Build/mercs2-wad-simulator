//! The world map image and its mapping, for the Live page's mini-map.
//!
//! The image is `vz_lrterrain`, 8,000 × 8,000 m. The mapping is `level_editor.md`'s, proven by plotting
//! all 42 POI points of `vz_poi_lineregions` on their places.

use crate::gui::theme;
use crate::live_link::{EntityClass, Live, Pose};
use egui::{Color32, RichText};

/// The metres the image covers on each side.
pub const MAP_SIDE: f32 = 8000.0;

/// A world point's place on the image, 0..=1 on each axis.
pub fn to_map(x: f32, z: f32) -> [f32; 2] {
    [(MAP_SIDE / 2.0 - x) / MAP_SIDE, (MAP_SIDE / 2.0 - z) / MAP_SIDE]
}

/// The decoded image, uploaded.
pub struct Map {
    pub texture: egui::TextureHandle,
}

/// The mini-map's card: the map, what is on it, and the retail position.
pub fn card(ui: &mut egui::Ui, map: Option<&Map>, live: &Live, pose: Option<&Pose>) {
    ui.horizontal(|ui| {
        ui.label(theme::disp_text("MINI-MAP", theme::style_eyebrow(), theme::live::FAINT));
        if let Some(w) = &live.world {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{} · 8 × 8 km", w.level)).small().monospace().color(theme::live::FAINT));
            });
        }
    });
    let side = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side * 0.8), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, theme::live::WELL);
    // A window on the map around the hero, so the hero's surroundings read at this size.
    const SPAN: f32 = 0.12;
    let centre = pose.map_or([0.5, 0.5], |p| to_map(p.pos[0], p.pos[2]));
    let uv_min = egui::pos2(centre[0] - SPAN / 2.0, centre[1] - SPAN * 0.4);
    let uv_max = egui::pos2(centre[0] + SPAN / 2.0, centre[1] + SPAN * 0.4);
    if let Some(m) = map {
        painter.image(m.texture.id(), rect, egui::Rect::from_min_max(uv_min, uv_max), Color32::WHITE);
    }
    let place = |x: f32, z: f32| -> egui::Pos2 {
        let m = to_map(x, z);
        rect.min + egui::vec2((m[0] - uv_min.x) / SPAN * rect.width(), (m[1] - uv_min.y) / (SPAN * 0.8) * rect.height())
    };
    if let Some(r) = &live.mission.race {
        for (i, c) in r.points.iter().enumerate() {
            let color = if (i as u32 + 1) == r.at { theme::live::AMBER } else { theme::live::DIM };
            painter.circle_filled(place(c[0], c[2]), 2.5, color);
        }
    }
    for e in live.entities.iter().filter(|e| e.class != EntityClass::Hero) {
        let color = if e.class.is_vehicle() { theme::live::AMBER } else { theme::live::BLUE };
        painter.circle_filled(place(e.pos[0], e.pos[2]), 2.0, color);
    }
    if let Some(p) = pose {
        let at = place(p.pos[0], p.pos[2]);
        let f = crate::live_cam::facing(p.yaw);
        let head = place(p.pos[0] + f.x * 120.0, p.pos[2] + f.z * 120.0);
        painter.line_segment([at, head], egui::Stroke::new(1.5, theme::live::GREEN));
        painter.circle(at, 4.0, theme::live::GREEN, egui::Stroke::new(1.0, theme::live::WELL));
    }
    if let Some(p) = pose {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:.0}, {:.0}, {:.0}", p.pos[0], p.pos[1], p.pos[2])).small().monospace().color(theme::live::FAINT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("yaw {:.0}°", p.yaw.rem_euclid(360.0))).small().monospace().color(theme::live::FAINT));
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mapping_is_the_proven_one() {
        assert_eq!(to_map(4000.0, 4000.0), [0.0, 0.0]);
        assert_eq!(to_map(-4000.0, -4000.0), [1.0, 1.0]);
        assert_eq!(to_map(0.0, 0.0), [0.5, 0.5]);
    }
}
