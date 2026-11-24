//! Hand-painted charts and scaled drawings: a mapping from data units to pixels, and the few marks
//! a bench draws with it (axes with ticks, polylines, dashed reference lines, labels). Every label
//! is in the type scale's 11 px monospace.

use crate::gui::theme;
use egui::{Align2, Color32, Painter, Pos2, Rect, Stroke};

/// A rectangle of the screen and the data range it shows. Y grows upward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plot {
    pub area: Rect,
    x: (f32, f32),
    y: (f32, f32),
}

/// The space a chart keeps around its plot area for tick labels: left, right, top, bottom.
pub const CHART_MARGIN: (f32, f32, f32, f32) = (44.0, 10.0, 12.0, 20.0);

impl Plot {
    /// `area` shows `x.0..=x.1` left to right and `y.0..=y.1` bottom to top, each axis filling it.
    pub fn new(area: Rect, x: (f32, f32), y: (f32, f32)) -> Plot {
        Plot { area, x, y }
    }

    /// A chart's plot inside `outer`, less [`CHART_MARGIN`].
    pub fn chart(outer: Rect, x: (f32, f32), y: (f32, f32)) -> Plot {
        let (l, r, t, b) = CHART_MARGIN;
        Plot::new(Rect::from_min_max(outer.min + egui::vec2(l, t), outer.max - egui::vec2(r, b)), x, y)
    }

    /// A drawing to scale: one pixel size per unit on both axes, the largest that fits `outer`,
    /// the data ranges centred in it.
    pub fn to_scale(outer: Rect, x: (f32, f32), y: (f32, f32)) -> Plot {
        let (w, h) = ((x.1 - x.0).abs().max(f32::EPSILON), (y.1 - y.0).abs().max(f32::EPSILON));
        let per_unit = (outer.width() / w).min(outer.height() / h);
        let size = egui::vec2(w * per_unit, h * per_unit);
        Plot::new(Rect::from_center_size(outer.center(), size), x, y)
    }

    /// A drawing at a given scale: `per_unit` pixels per unit on both axes, centred in `outer`.
    pub fn at_scale(outer: Rect, x: (f32, f32), y: (f32, f32), per_unit: f32) -> Plot {
        let size = egui::vec2((x.1 - x.0).abs() * per_unit, (y.1 - y.0).abs() * per_unit);
        Plot::new(Rect::from_center_size(outer.center(), size), x, y)
    }

    /// Pixels per data unit along X.
    pub fn per_unit(&self) -> f32 {
        self.area.width() / (self.x.1 - self.x.0)
    }

    pub fn x(&self, x: f32) -> f32 {
        self.area.left() + (x - self.x.0) / (self.x.1 - self.x.0) * self.area.width()
    }

    pub fn y(&self, y: f32) -> f32 {
        self.area.bottom() - (y - self.y.0) / (self.y.1 - self.y.0) * self.area.height()
    }

    pub fn at(&self, x: f32, y: f32) -> Pos2 {
        egui::pos2(self.x(x), self.y(y))
    }
}

/// The multiples of `step` from `lo` to `hi`, both included when they are multiples.
pub fn ticks(lo: f32, hi: f32, step: f32) -> Vec<f32> {
    let first = (lo / step).ceil() as i64;
    let last = (hi / step + 1e-4).floor() as i64;
    (first..=last).map(|i| i as f32 * step).collect()
}

/// A number with thousands separators and no trailing zeros, for a tick or a value label.
pub fn number(v: f32) -> String {
    let text = format!("{:.3}", v.abs());
    let text = text.trim_end_matches('0').trim_end_matches('.');
    let (whole, part) = text.split_once('.').unwrap_or((text, ""));
    let mut out = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if !part.is_empty() {
        out.push('.');
        out.push_str(part);
    }
    if v < 0.0 && out != "0" { format!("−{out}") } else { out }
}

/// A label in the chart voice.
pub fn label(p: &Painter, pos: Pos2, align: Align2, text: impl ToString, color: Color32) {
    p.text(pos, align, text, theme::font(&egui::TextStyle::Monospace), color);
}

/// Grid lines and tick labels for the given values, then the two axis lines.
pub fn axes(p: &Painter, plot: &Plot, x_ticks: &[(f32, String)], y_ticks: &[(f32, String)]) {
    let grid = Stroke::new(1.0, theme::G2);
    let a = plot.area;
    for (v, text) in y_ticks {
        let y = plot.y(*v);
        p.line_segment([egui::pos2(a.left(), y), egui::pos2(a.right(), y)], grid);
        label(p, egui::pos2(a.left() - 4.0, y), Align2::RIGHT_CENTER, text, theme::DIM);
    }
    for (v, text) in x_ticks {
        let x = plot.x(*v);
        p.line_segment([egui::pos2(x, a.top()), egui::pos2(x, a.bottom())], grid);
        label(p, egui::pos2(x, a.bottom() + 4.0), Align2::CENTER_TOP, text, theme::DIM);
    }
    let axis = Stroke::new(1.0, theme::LINE2);
    p.line_segment([a.left_top(), a.left_bottom()], axis);
    p.line_segment([a.left_bottom(), a.right_bottom()], axis);
}

/// A polyline through data points.
pub fn line(p: &Painter, plot: &Plot, points: &[(f32, f32)], stroke: Stroke) {
    p.add(egui::Shape::line(points.iter().map(|&(x, y)| plot.at(x, y)).collect(), stroke));
}

/// A dashed line across the plot at a Y value.
pub fn dashed_across(p: &Painter, plot: &Plot, y: f32, color: Color32) {
    let y = plot.y(y);
    let ends = [egui::pos2(plot.area.left(), y), egui::pos2(plot.area.right(), y)];
    p.extend(egui::Shape::dashed_line(&ends, Stroke::new(1.0, color), 2.0, 4.0));
}

/// A dashed line up the plot at an X value.
pub fn dashed_up(p: &Painter, plot: &Plot, x: f32, color: Color32) {
    let x = plot.x(x);
    let ends = [egui::pos2(x, plot.area.top()), egui::pos2(x, plot.area.bottom())];
    p.extend(egui::Shape::dashed_line(&ends, Stroke::new(1.0, color), 4.0, 3.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f32, h: f32) -> Rect {
        Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(w, h))
    }

    #[test]
    fn a_plot_puts_its_low_corner_bottom_left() {
        let plot = Plot::new(rect(200.0, 100.0), (0.0, 100.0), (0.0, 8000.0));
        assert_eq!(plot.at(0.0, 0.0), egui::pos2(10.0, 120.0));
        assert_eq!(plot.at(100.0, 8000.0), egui::pos2(210.0, 20.0));
        assert_eq!(plot.at(50.0, 4000.0), egui::pos2(110.0, 70.0));
    }

    #[test]
    fn a_drawing_to_scale_keeps_one_scale_and_fits() {
        // 9.5 m long and 4.5 m tall in a 400 × 100 strip: the height binds.
        let plot = Plot::to_scale(rect(400.0, 100.0), (4.9, -4.6), (0.0, 4.5));
        assert!((plot.per_unit().abs() - 100.0 / 4.5).abs() < 1e-3);
        assert!((plot.area.height() - 100.0).abs() < 1e-3);
        // the front (largest z) is on the left
        assert!(plot.x(4.07) < plot.x(-3.76));
        // a 2 m and a 14 m model both stay inside
        for length in [2.0_f32, 14.0] {
            let p = Plot::to_scale(rect(400.0, 100.0), (length / 2.0, -length / 2.0), (0.0, 3.0));
            assert!(rect(400.0, 100.0).expand(0.01).contains_rect(p.area), "{length}");
        }
    }

    #[test]
    fn ticks_are_the_multiples_inside_the_range() {
        assert_eq!(ticks(0.0, 100.0, 20.0), [0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        assert_eq!(ticks(0.0, 0.9, 0.2), [0.0, 0.2, 0.4, 0.6, 0.8]);
        assert_eq!(ticks(1000.0, 8000.0, 2000.0), [2000.0, 4000.0, 6000.0, 8000.0]);
    }

    #[test]
    fn numbers_group_thousands_and_drop_trailing_zeros() {
        assert_eq!(number(27000.0), "27,000");
        assert_eq!(number(4297.2), "4,297.2");
        assert_eq!(number(0.115), "0.115");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(-3.1), "−3.1");
        assert_eq!(number(2.0), "2");
        assert_eq!(number(0.0), "0");
    }
}
