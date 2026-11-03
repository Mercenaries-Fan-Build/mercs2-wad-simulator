//! Shape records to triangles.
//!
//! A shape's edges carry a fill on each side: `fill0` on the left of travel, `fill1` on the right. A
//! style's region is what those edges enclose, so each style gathers its edges (`fill0`'s reversed),
//! stitches them end to end into closed contours, and those are filled. Strokes run in record order.

use std::collections::HashMap;
use std::rc::Rc;

use lyon_tessellation::math::point;
use lyon_tessellation::path::Path;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use mercs2_formats::gfx_def::{Fill, Gradient, GradientKind, Line, Matrix, Morph, Record, Rgba, Shape};

/// Curve flattening tolerance in twips: a twentieth of a pixel at unit scale.
const TOLERANCE: f32 = 1.0;

/// What a [`Part`] paints with.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Rgba),
    /// 256 stops; `to_unit` maps twips to the gradient square `[-1, 1]`.
    Gradient { ramp: Vec<Rgba>, kind: GradientKind, to_unit: Matrix, spread: u8, focal: f32 },
    /// `to_pixels` maps twips to pixels of the image `id` names.
    Bitmap { id: u16, to_pixels: Matrix, clamp: bool, smooth: bool },
    /// Text: quads into a font texture, one coordinate per vertex.
    Glyphs { atlas: Rc<str>, color: Rgba, uvs: Vec<[f32; 2]> },
}

/// One paint's triangles, in twips.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub paint: Paint,
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Point(i32, i32);

/// A directed edge; `control` for a quadratic curve.
#[derive(Clone, Copy, Debug)]
struct Edge {
    from: Point,
    control: Option<Point>,
    to: Point,
}

impl Edge {
    fn reversed(self) -> Edge {
        Edge { from: self.to, control: self.control, to: self.from }
    }
}

/// Edges of one style table, between `Style` records that replace it.
struct Layer {
    fills: Vec<Fill>,
    lines: Vec<Line>,
    fill_edges: HashMap<u32, Vec<Edge>>,
    line_edges: HashMap<u32, Vec<Edge>>,
    /// Fill and line styles in the order they first appear, which is their paint order.
    fill_order: Vec<u32>,
    line_order: Vec<u32>,
}

impl Layer {
    fn new(fills: Vec<Fill>, lines: Vec<Line>) -> Layer {
        Layer { fills, lines, fill_edges: HashMap::new(), line_edges: HashMap::new(), fill_order: Vec::new(), line_order: Vec::new() }
    }

    fn fill(&mut self, style: u32, edge: Edge) {
        if !self.fill_edges.contains_key(&style) {
            self.fill_order.push(style);
        }
        self.fill_edges.entry(style).or_default().push(edge);
    }

    fn line(&mut self, style: u32, edge: Edge) {
        if !self.line_edges.contains_key(&style) {
            self.line_order.push(style);
        }
        self.line_edges.entry(style).or_default().push(edge);
    }

    fn finish(mut self, parts: &mut Vec<Part>) -> Result<(), String> {
        self.fill_order.sort_unstable();
        for style in &self.fill_order {
            let fill = self.fills.get(*style as usize - 1).ok_or_else(|| format!("fill style {style} is out of range"))?;
            let contours = stitch(&self.fill_edges[style]);
            parts.push(fill_part(fill, &contours)?);
        }
        for style in &self.line_order {
            let line = self.lines.get(*style as usize - 1).ok_or_else(|| format!("line style {style} is out of range"))?;
            let runs = chain(&self.line_edges[style]);
            parts.push(stroke_part(line, &runs)?);
        }
        Ok(())
    }
}

/// Stitch directed edges end to end, each contour closing when it reaches its start.
fn stitch(edges: &[Edge]) -> Vec<Vec<Edge>> {
    let mut from: HashMap<Point, Vec<usize>> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        from.entry(e.from).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut contours = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut contour = vec![edges[first]];
        let start = edges[first].from;
        let mut at = edges[first].to;
        while at != start {
            let Some(next) = from.get(&at).and_then(|list| list.iter().copied().find(|i| !used[*i])) else { break };
            used[next] = true;
            contour.push(edges[next]);
            at = edges[next].to;
        }
        contours.push(contour);
    }
    contours
}

/// Join a stroke's edges into runs, in record order.
fn chain(edges: &[Edge]) -> Vec<Vec<Edge>> {
    let mut runs: Vec<Vec<Edge>> = Vec::new();
    for e in edges {
        match runs.last_mut() {
            Some(run) if run.last().is_some_and(|l| l.to == e.from) => run.push(*e),
            _ => runs.push(vec![*e]),
        }
    }
    runs
}

fn path_of(runs: &[Vec<Edge>], close: bool) -> Path {
    let mut b = Path::builder();
    for run in runs {
        let p = |q: Point| point(q.0 as f32, q.1 as f32);
        b.begin(p(run[0].from));
        for e in run {
            match e.control {
                Some(c) => {
                    b.quadratic_bezier_to(p(c), p(e.to));
                }
                None => {
                    b.line_to(p(e.to));
                }
            }
        }
        b.end(close || run[0].from == run[run.len() - 1].to);
    }
    b.build()
}

fn fill_part(fill: &Fill, contours: &[Vec<Edge>]) -> Result<Part, String> {
    let mut out: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    FillTessellator::new()
        .tessellate_path(
            &path_of(contours, true),
            &FillOptions::tolerance(TOLERANCE),
            &mut BuffersBuilder::new(&mut out, |v: FillVertex| v.position().to_array()),
        )
        .map_err(|e| format!("fill tessellation: {e:?}"))?;
    Ok(Part { paint: paint_of(fill)?, vertices: out.vertices, indices: out.indices })
}

fn stroke_part(line: &Line, runs: &[Vec<Edge>]) -> Result<Part, String> {
    let cap = |c: u8| match c {
        0 => LineCap::Round,
        1 => LineCap::Butt,
        _ => LineCap::Square,
    };
    let join = match line.join {
        0 => LineJoin::Round,
        1 => LineJoin::Bevel,
        _ => LineJoin::MiterClip,
    };
    let options = StrokeOptions::tolerance(TOLERANCE)
        .with_line_width((line.width as f32).max(1.0))
        .with_start_cap(cap(line.start_cap))
        .with_end_cap(cap(line.end_cap))
        .with_line_join(join)
        .with_miter_limit(line.miter.max(1.0));
    let mut out: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    StrokeTessellator::new()
        .tessellate_path(
            &path_of(runs, false),
            &options,
            &mut BuffersBuilder::new(&mut out, |v: StrokeVertex| v.position().to_array()),
        )
        .map_err(|e| format!("stroke tessellation: {e:?}"))?;
    let paint = match &line.fill {
        Some(fill) => paint_of(fill)?,
        None => Paint::Solid(line.color),
    };
    Ok(Part { paint, vertices: out.vertices, indices: out.indices })
}

/// The 256 entries of a gradient's color ramp.
pub fn ramp(stops: &[(u8, Rgba)]) -> Vec<Rgba> {
    (0..=255u32)
        .map(|i| {
            let next = stops.iter().position(|(r, _)| *r as u32 >= i);
            match next {
                None => stops.last().map_or([0; 4], |s| s.1),
                Some(0) => stops[0].1,
                Some(n) => {
                    let (a, b) = (stops[n - 1], stops[n]);
                    let span = (b.0 - a.0) as f32;
                    let t = (i as f32 - a.0 as f32) / span.max(1.0);
                    std::array::from_fn(|k| (a.1[k] as f32 + (b.1[k] as f32 - a.1[k] as f32) * t).round() as u8)
                }
            }
        })
        .collect()
}

fn paint_of(fill: &Fill) -> Result<Paint, String> {
    Ok(match fill {
        Fill::Solid(c) => Paint::Solid(*c),
        Fill::Gradient(Gradient { kind, matrix, spread, stops, focal, .. }) => {
            // The gradient square spans ±16384 twips before its matrix.
            let square = Matrix { a: 16384.0, d: 16384.0, ..Matrix::IDENTITY };
            let to_unit = matrix.then(&square).inverse().ok_or("a gradient with a singular matrix")?;
            Paint::Gradient { ramp: ramp(stops), kind: *kind, to_unit, spread: *spread, focal: *focal }
        }
        Fill::Bitmap { id, matrix, clamp, smooth } => {
            Paint::Bitmap { id: *id, to_pixels: matrix.inverse().ok_or("a bitmap fill with a singular matrix")?, clamp: *clamp, smooth: *smooth }
        }
    })
}

/// A record walker: current point and styles, feeding the layer it builds.
fn walk(records: &[Record], fills: Vec<Fill>, lines: Vec<Line>) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    let mut layer = Layer::new(fills, lines);
    let (mut at, mut fill0, mut fill1, mut line) = (Point(0, 0), 0, 0, 0);
    for record in records {
        match record {
            Record::Style { move_to, fill0: f0, fill1: f1, line: l, styles } => {
                if let Some((fills, lines)) = styles {
                    let done = std::mem::replace(&mut layer, Layer::new(fills.clone(), lines.clone()));
                    done.finish(&mut parts)?;
                    (fill0, fill1, line) = (0, 0, 0);
                }
                if let Some((x, y)) = move_to {
                    at = Point(*x, *y);
                }
                fill0 = f0.unwrap_or(fill0);
                fill1 = f1.unwrap_or(fill1);
                line = l.unwrap_or(line);
            }
            Record::Line { .. } | Record::Curve { .. } => {
                let edge = match record {
                    Record::Line { dx, dy } => Edge { from: at, control: None, to: Point(at.0 + dx, at.1 + dy) },
                    Record::Curve { cx, cy, ax, ay } => {
                        let c = Point(at.0 + cx, at.1 + cy);
                        Edge { from: at, control: Some(c), to: Point(c.0 + ax, c.1 + ay) }
                    }
                    Record::Style { .. } => unreachable!(),
                };
                at = edge.to;
                // An edge with one style on both sides bounds nothing.
                if fill0 != fill1 {
                    if fill0 != 0 {
                        layer.fill(fill0, edge.reversed());
                    }
                    if fill1 != 0 {
                        layer.fill(fill1, edge);
                    }
                }
                if line != 0 {
                    layer.line(line, edge);
                }
            }
        }
    }
    layer.finish(&mut parts)?;
    Ok(parts)
}

pub fn tessellate(shape: &Shape) -> Result<Vec<Part>, String> {
    walk(&shape.records, shape.fills.clone(), shape.lines.clone())
}

fn lerp_i(a: i32, b: i32, t: f32) -> i32 {
    (a as f32 + (b - a) as f32 * t).round() as i32
}

fn lerp_color(a: Rgba, b: Rgba, t: f32) -> Rgba {
    std::array::from_fn(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * t).round() as u8)
}

fn lerp_fill(a: &Fill, b: &Fill, t: f32) -> Result<Fill, String> {
    Ok(match (a, b) {
        (Fill::Solid(x), Fill::Solid(y)) => Fill::Solid(lerp_color(*x, *y, t)),
        (Fill::Gradient(x), Fill::Gradient(y)) => Fill::Gradient(Gradient {
            matrix: x.matrix.lerp(&y.matrix, t),
            stops: x
                .stops
                .iter()
                .zip(&y.stops)
                .map(|(s, e)| (lerp_i(s.0 as i32, e.0 as i32, t) as u8, lerp_color(s.1, e.1, t)))
                .collect(),
            focal: x.focal + (y.focal - x.focal) * t,
            ..x.clone()
        }),
        (Fill::Bitmap { id, matrix: m0, clamp, smooth }, Fill::Bitmap { matrix: m1, .. }) => {
            Fill::Bitmap { id: *id, matrix: m0.lerp(m1, t), clamp: *clamp, smooth: *smooth }
        }
        _ => return Err("a morph whose two fills differ in kind".into()),
    })
}

/// Bring a record list to absolute moves and split curves' shapes so two lists pair up edge for edge.
/// A morph's end list may write an edge the start list writes as a curve as a line, and a style record
/// may sit on one side only.
fn edges_of(records: &[Record]) -> Vec<(Option<Record>, Option<Record>)> {
    let mut out = Vec::new();
    for r in records {
        match r {
            Record::Style { .. } => out.push((Some(r.clone()), None)),
            _ => out.push((None, Some(r.clone()))),
        }
    }
    out
}

fn as_curve(record: &Record) -> Result<(i32, i32, i32, i32), String> {
    match record {
        Record::Curve { cx, cy, ax, ay } => Ok((*cx, *cy, *ax, *ay)),
        // A line as the curve its midpoint controls.
        Record::Line { dx, dy } => Ok((dx / 2, dy / 2, dx - dx / 2, dy - dy / 2)),
        Record::Style { .. } => Err("a style record where a morph wants an edge".into()),
    }
}

/// A morph's shape at `ratio` of 65535.
pub fn tessellate_morph(morph: &Morph, ratio: u16) -> Result<Vec<Part>, String> {
    let t = ratio as f32 / 65535.0;
    let fills = morph.fills.iter().map(|(a, b)| lerp_fill(a, b, t)).collect::<Result<Vec<_>, _>>()?;
    let lines = morph
        .lines
        .iter()
        .map(|(a, b)| {
            Ok(Line {
                width: lerp_i(a.width as i32, b.width as i32, t) as u16,
                color: lerp_color(a.color, b.color, t),
                fill: match (&a.fill, &b.fill) {
                    (Some(x), Some(y)) => Some(lerp_fill(x, y, t)?),
                    _ => None,
                },
                ..a.clone()
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let end_edges: Vec<&Record> = morph.end.iter().filter(|r| !matches!(r, Record::Style { .. })).collect();
    let end_moves: Vec<(i32, i32)> = morph
        .end
        .iter()
        .filter_map(|r| match r {
            Record::Style { move_to, .. } => *move_to,
            _ => None,
        })
        .collect();
    let mut records = Vec::new();
    let mut edge = 0;
    let mut mover = 0;
    for (style, drawn) in edges_of(&morph.start) {
        if let Some(Record::Style { move_to, fill0, fill1, line, styles }) = style {
            let move_to = match move_to {
                Some((x, y)) => {
                    let (ex, ey) = *end_moves.get(mover).ok_or("a morph's end shape has too few moves")?;
                    mover += 1;
                    Some((lerp_i(x, ex, t), lerp_i(y, ey, t)))
                }
                None => None,
            };
            records.push(Record::Style { move_to, fill0, fill1, line, styles });
        }
        if let Some(start) = drawn {
            let end = end_edges.get(edge).ok_or("a morph's end shape has too few edges")?;
            edge += 1;
            let (c0, c1, c2, c3) = as_curve(&start)?;
            let (d0, d1, d2, d3) = as_curve(end)?;
            records.push(Record::Curve {
                cx: lerp_i(c0, d0, t),
                cy: lerp_i(c1, d1, t),
                ax: lerp_i(c2, d2, t),
                ay: lerp_i(c3, d3, t),
            });
        }
    }
    walk(&records, fills, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(fill0: Option<u32>, fill1: Option<u32>) -> Shape {
        Shape {
            bounds: [0, 100, 0, 100],
            fills: vec![Fill::Solid([255, 0, 0, 255])],
            lines: Vec::new(),
            records: vec![
                Record::Style { move_to: Some((0, 0)), fill0, fill1, line: None, styles: None },
                Record::Line { dx: 100, dy: 0 },
                Record::Line { dx: 0, dy: 100 },
                Record::Line { dx: -100, dy: 0 },
                Record::Line { dx: 0, dy: -100 },
            ],
        }
    }

    fn area(part: &Part) -> f32 {
        part.indices
            .chunks_exact(3)
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| part.vertices[i as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
            })
            .sum()
    }

    #[test]
    fn either_side_of_an_edge_fills_the_same_square() {
        for shape in [square(Some(1), None), square(None, Some(1))] {
            let parts = tessellate(&shape).expect("tessellate");
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0].paint, Paint::Solid([255, 0, 0, 255]));
            assert!((area(&parts[0]) - 10_000.0).abs() < 1.0, "{}", area(&parts[0]));
        }
    }

    #[test]
    fn edges_written_out_of_order_still_close() {
        let shape = Shape {
            bounds: [0, 100, 0, 100],
            fills: vec![Fill::Solid([1, 2, 3, 255])],
            lines: Vec::new(),
            records: vec![
                Record::Style { move_to: Some((100, 100)), fill0: None, fill1: Some(1), line: None, styles: None },
                Record::Line { dx: -100, dy: 0 },
                Record::Line { dx: 0, dy: -100 },
                Record::Style { move_to: Some((0, 0)), fill0: None, fill1: None, line: None, styles: None },
                Record::Line { dx: 100, dy: 0 },
                Record::Line { dx: 0, dy: 100 },
            ],
        };
        assert!((area(&tessellate(&shape).expect("tessellate")[0]) - 10_000.0).abs() < 1.0);
    }

    #[test]
    fn an_edge_with_one_style_each_side_draws_nothing() {
        assert!(tessellate(&square(Some(1), Some(1))).expect("tessellate").is_empty());
    }

    #[test]
    fn a_hole_is_cut_out_of_its_fill() {
        let mut shape = square(None, Some(1));
        shape.records.push(Record::Style { move_to: Some((25, 25)), fill0: Some(1), fill1: Some(0), line: None, styles: None });
        for (dx, dy) in [(50, 0), (0, 50), (-50, 0), (0, -50)] {
            shape.records.push(Record::Line { dx, dy });
        }
        assert!((area(&tessellate(&shape).expect("tessellate")[0]) - 7_500.0).abs() < 1.0);
    }

    #[test]
    fn a_stroke_covers_its_width() {
        let shape = Shape {
            bounds: [0, 100, 0, 0],
            fills: Vec::new(),
            lines: vec![Line {
                width: 20,
                color: [9, 9, 9, 255],
                start_cap: 1,
                end_cap: 1,
                join: 0,
                miter: 3.0,
                fill: None,
                no_close: false,
            }],
            records: vec![
                Record::Style { move_to: Some((0, 0)), fill0: None, fill1: None, line: Some(1), styles: None },
                Record::Line { dx: 100, dy: 0 },
            ],
        };
        assert!((area(&tessellate(&shape).expect("tessellate")[0]) - 2_000.0).abs() < 1.0);
    }

    #[test]
    fn a_ramp_holds_its_ends_and_blends_between() {
        let r = ramp(&[(64, [0, 0, 0, 255]), (192, [255, 255, 255, 255])]);
        assert_eq!(r[0], [0, 0, 0, 255]);
        assert_eq!(r[255], [255, 255, 255, 255]);
        assert_eq!(r[128][0], 128);
    }

    #[test]
    fn a_gradient_maps_its_square_to_the_unit_range() {
        let fill = Fill::Gradient(Gradient {
            kind: GradientKind::Linear,
            matrix: Matrix { a: 0.5, tx: 1000.0, ..Matrix::IDENTITY },
            spread: 0,
            interpolation: 0,
            stops: vec![(0, [0; 4]), (255, [255; 4])],
            focal: 0.0,
        });
        let Paint::Gradient { to_unit, .. } = paint_of(&fill).expect("paint") else { panic!("a gradient") };
        assert_eq!(to_unit.apply(1000.0 + 8192.0, 0.0)[0], 1.0);
        assert_eq!(to_unit.apply(1000.0 - 8192.0, 0.0)[0], -1.0);
    }

    #[test]
    fn a_morph_reads_between_its_shapes() {
        let start = square(None, Some(1)).records;
        let end: Vec<Record> = start
            .iter()
            .map(|r| match r {
                Record::Line { dx, dy } => Record::Line { dx: dx * 2, dy: dy * 2 },
                other => other.clone(),
            })
            .collect();
        let fill = Fill::Solid([0, 0, 0, 255]);
        let morph = Morph { fills: vec![(fill.clone(), fill)], lines: Vec::new(), start, end };
        let at = |ratio| area(&tessellate_morph(&morph, ratio).expect("morph")[0]);
        assert!((at(0) - 10_000.0).abs() < 1.0);
        assert!((at(65535) - 40_000.0).abs() < 1.0);
        assert!((at(32768) - 22_500.0).abs() < 200.0, "{}", at(32768));
    }
}
