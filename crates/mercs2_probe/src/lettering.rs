//! `lettering-draft`: the first drawing of the main-menu stencil lettering as SVG, one file per letter,
//! built from the parts retail's `shell_MAINMENU_text` is made of (stems, bowl halves, quarter arms,
//! dashes, diagonal bands) at the proportions measured from it (`qm::label_art`). The files are the
//! source from then on: people refine them in an SVG editor; this only writes the starting point.
//!
//! Geometry is in texels of retail's lettering: the cap height is 27, the cap top y = 0. A piece is a set
//! of polygons filled even-odd (an outline and its holes); a letter is pieces drawn over each other. A
//! stencil gap is a cut: every piece is clipped by straight lines, so halves mirror exactly and cuts are
//! square. Curves are polylines of [`ARC_STEPS`] segments a quarter circle.

use std::fmt::Write as _;

/// Cap height, in texels.
pub const CAP: f64 = 27.0;
/// Room in every letter's SVG above the cap (for accents) and below the baseline (for a cedilla), in
/// texels; `qm::label_art` reads the same.
pub const ABOVE: f64 = 10.0;
pub const BELOW: f64 = 6.0;
/// SVG units per texel: the cap height is 1000 units.
pub const UNIT: f64 = 1000.0 / CAP;
const ARC_STEPS: usize = 12;

/// The measured proportions, in texels.
const STEM: f64 = 9.6;
const WALL: f64 = 4.5;
const GAP: f64 = 1.3;
const OUTER_R: f64 = 7.0;
const INNER_R: f64 = 2.0;

type Pt = (f64, f64);
type Poly = Vec<Pt>;
/// An outline and its holes, filled even-odd.
type Piece = Vec<Poly>;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Poly {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// A rectangle with corner radii `[top-left, top-right, bottom-right, bottom-left]`, clockwise.
fn rrect(x0: f64, y0: f64, x1: f64, y1: f64, r: [f64; 4]) -> Poly {
    let mut out = Vec::new();
    let corner = |out: &mut Poly, cx: f64, cy: f64, rad: f64, start: f64| {
        if rad <= 0.0 {
            out.push((cx + rad * start.cos(), cy + rad * start.sin()));
            return;
        }
        for k in 0..=ARC_STEPS {
            let a = start + std::f64::consts::FRAC_PI_2 * k as f64 / ARC_STEPS as f64;
            out.push((cx + rad * a.cos(), cy + rad * a.sin()));
        }
    };
    use std::f64::consts::PI;
    // Top-left, top-right, bottom-right, bottom-left; angles in screen space (y down).
    if r[0] > 0.0 { corner(&mut out, x0 + r[0], y0 + r[0], r[0], PI) } else { out.push((x0, y0)) }
    if r[1] > 0.0 { corner(&mut out, x1 - r[1], y0 + r[1], r[1], -PI / 2.0) } else { out.push((x1, y0)) }
    if r[2] > 0.0 { corner(&mut out, x1 - r[2], y1 - r[2], r[2], 0.0) } else { out.push((x1, y1)) }
    if r[3] > 0.0 { corner(&mut out, x0 + r[3], y1 - r[3], r[3], PI / 2.0) } else { out.push((x0, y1)) }
    out
}

/// A closed polygon through `corners`, each rounded by an arc of its radius tangent to both edges at it
/// (radius 0 keeps the corner sharp; a radius too big for its edges is shrunk to fit).
fn filleted(corners: &[(Pt, f64)]) -> Poly {
    let n = corners.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (prev, (p, r), next) = (corners[(i + n - 1) % n].0, corners[i], corners[(i + 1) % n].0);
        if r <= 0.0 {
            out.push(p);
            continue;
        }
        let unit = |a: Pt, b: Pt| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l = (dx * dx + dy * dy).sqrt();
            ((dx / l, dy / l), l)
        };
        let ((u1, l1), (u2, l2)) = (unit(p, prev), unit(p, next));
        // The angle between the two edges at the corner, and how far from it the arc touches each edge.
        let cos = (u1.0 * u2.0 + u1.1 * u2.1).clamp(-1.0, 1.0);
        let half = cos.acos() / 2.0;
        if half.abs() < 1e-6 || (std::f64::consts::FRAC_PI_2 - half).abs() < 1e-6 {
            out.push(p);
            continue;
        }
        let along = (r / half.tan()).min(l1 / 2.0).min(l2 / 2.0);
        let r = along * half.tan();
        let (a, b) = ((p.0 + u1.0 * along, p.1 + u1.1 * along), (p.0 + u2.0 * along, p.1 + u2.1 * along));
        // The arc's centre lies on the bisector, r / sin(half) from the corner.
        let bis = ((u1.0 + u2.0) / 2.0, (u1.1 + u2.1) / 2.0);
        let bl = (bis.0 * bis.0 + bis.1 * bis.1).sqrt();
        let c = (p.0 + bis.0 / bl * r / half.sin(), p.1 + bis.1 / bl * r / half.sin());
        let (sa, ea) = ((a.1 - c.1).atan2(a.0 - c.0), (b.1 - c.1).atan2(b.0 - c.0));
        let mut sweep = ea - sa;
        while sweep > std::f64::consts::PI {
            sweep -= 2.0 * std::f64::consts::PI;
        }
        while sweep < -std::f64::consts::PI {
            sweep += 2.0 * std::f64::consts::PI;
        }
        for k in 0..=ARC_STEPS {
            let t = sa + sweep * k as f64 / ARC_STEPS as f64;
            out.push((c.0 + r * t.cos(), c.1 + r * t.sin()));
        }
    }
    out
}

/// A straight band from `top` (its left and right x at y = `y0`) to `bottom` (at y = `y1`).
fn band(top: (f64, f64), bottom: (f64, f64), y0: f64, y1: f64) -> Poly {
    vec![(top.0, y0), (top.1, y0), (bottom.1, y1), (bottom.0, y1)]
}

/// A half-plane: the points where `a·x + b·y <= c`.
#[derive(Clone, Copy)]
struct Half {
    a: f64,
    b: f64,
    c: f64,
}

fn left_of(x: f64) -> Half {
    Half { a: 1.0, b: 0.0, c: x }
}
fn right_of(x: f64) -> Half {
    Half { a: -1.0, b: 0.0, c: -x }
}
fn above(y: f64) -> Half {
    Half { a: 0.0, b: 1.0, c: y }
}
fn below(y: f64) -> Half {
    Half { a: 0.0, b: -1.0, c: -y }
}
/// The side of the line through `p` and `q` that is to the left walking from `p` to `q` (screen space,
/// y down), moved `off` texels away from the line.
fn side_of(p: Pt, q: Pt, off: f64) -> Half {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let len = (dx * dx + dy * dy).sqrt();
    let (a, b) = (dy / len, -dx / len);
    Half { a, b, c: a * p.0 + b * p.1 - off }
}

/// `poly` clipped to every half-plane of `halves` (Sutherland–Hodgman).
fn clip(poly: &Poly, halves: &[Half]) -> Poly {
    let mut out = poly.clone();
    for h in halves {
        let inside = |p: Pt| h.a * p.0 + h.b * p.1 <= h.c + 1e-9;
        let cross = |p: Pt, q: Pt| {
            let (fp, fq) = (h.a * p.0 + h.b * p.1 - h.c, h.a * q.0 + h.b * q.1 - h.c);
            let t = fp / (fp - fq);
            (p.0 + t * (q.0 - p.0), p.1 + t * (q.1 - p.1))
        };
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (p, q) = (input[i], input[(i + 1) % input.len()]);
            match (inside(p), inside(q)) {
                (true, true) => out.push(q),
                (true, false) => out.push(cross(p, q)),
                (false, true) => {
                    out.push(cross(p, q));
                    out.push(q);
                }
                (false, false) => {}
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Every polygon of `piece` clipped to `halves` (the holes too, so even-odd still cuts them out).
fn clip_piece(piece: &Piece, halves: &[Half]) -> Piece {
    piece.iter().map(|p| clip(p, halves)).filter(|p| p.len() >= 3).collect()
}

/// A ring: `outer` with `inner` cut out.
fn ring(outer: Poly, inner: Poly) -> Piece {
    vec![outer, inner]
}

/// The O ring of a letter `w` wide, its top and bottom walls [`WALL`], side walls [`STEM`].
fn o_ring(x0: f64, w: f64) -> Piece {
    ring(rrect(x0, 0.0, x0 + w, CAP, [OUTER_R; 4]), rrect(x0 + STEM + 0.4, WALL, x0 + w - STEM - 0.4, CAP - WALL, [INNER_R; 4]))
}

/// One drafted letter: its pieces and its advance width, in texels.
pub struct Draft {
    pub ch: char,
    pub width: f64,
    pub pieces: Vec<Piece>,
}

fn d(ch: char, width: f64, pieces: Vec<Piece>) -> Draft {
    Draft { ch, width, pieces: pieces.into_iter().filter(|p| !p.is_empty()).collect() }
}

fn solid(p: Poly) -> Piece {
    vec![p]
}

/// The main-menu capitals.
pub fn capitals() -> Vec<Draft> {
    let mid = |w: f64| w / 2.0;
    let mut out = Vec::new();
    // Stems and dashes.
    out.push(d('I', 10.0, vec![solid(rect(0.0, 0.0, 10.0, CAP))]));
    let dash = |y0: f64, y1: f64| solid(rect(10.0 + GAP, y0, 22.0, y1));
    let (top, middle, bottom) = (dash(0.0, 4.3), dash(11.3, 15.6), dash(22.2, CAP));
    out.push(d('E', 22.0, vec![solid(rect(0.0, 0.0, 10.0, CAP)), top.clone(), middle.clone(), bottom.clone()]));
    out.push(d('F', 22.0, vec![solid(rect(0.0, 0.0, 10.0, CAP)), top, middle]));
    out.push(d('L', 22.0, vec![solid(rect(0.0, 0.0, 10.0, CAP)), bottom]));
    out.push(d('T', 27.0, vec![solid(rect(0.0, 0.0, 27.0, 4.3)), solid(rect(8.5, 4.3 + GAP, 18.5, CAP))]));
    out.push(d('H', 29.0, vec![solid(rect(0.0, 0.0, 9.5, CAP)), solid(rect(19.5, 0.0, 29.0, CAP)), solid(rect(9.5 + GAP, 11.3, 20.0, 15.7))]));
    // Bowls: O, U, C, G, D, P, R, B, J, Q.
    let w = 28.0;
    let o = o_ring(0.0, w);
    let left = |p: &Piece| clip_piece(p, &[left_of(mid(w) - GAP / 2.0)]);
    let right = |p: &Piece| clip_piece(p, &[right_of(mid(w) + GAP / 2.0)]);
    out.push(d('O', w, vec![left(&o), right(&o)]));
    let u = ring(rrect(0.0, 0.0, w, CAP, [0.0, 0.0, OUTER_R, OUTER_R]), rrect(STEM + 0.4, -10.0, w - STEM - 0.4, CAP - WALL, [0.0, 0.0, INNER_R, INNER_R]));
    out.push(d('U', w, vec![clip_piece(&u, &[left_of(mid(w) - GAP / 2.0), below(0.0)]), clip_piece(&u, &[right_of(mid(w) + GAP / 2.0), below(0.0)])]));
    let arm_top = clip_piece(&o, &[right_of(mid(w) + GAP / 2.0), above(9.8)]);
    let arm_bottom = clip_piece(&o, &[right_of(mid(w) + GAP / 2.0), below(17.2)]);
    out.push(d('C', w, vec![left(&o), arm_top, arm_bottom]));
    // G is a texel narrower than O (27).
    let gw = 27.0;
    let go = o_ring(0.0, gw);
    let g_split = mid(gw);
    let g_low = clip_piece(&go, &[right_of(g_split + GAP / 2.0), below(11.3)]);
    out.push(d(
        'G',
        gw,
        vec![
            clip_piece(&go, &[left_of(g_split - GAP / 2.0)]),
            clip_piece(&go, &[right_of(g_split + GAP / 2.0), above(9.8)]),
            g_low,
            solid(rect(13.2, 11.3, gw - STEM, 15.8)),
        ],
    ));
    let bowl = |x1: f64, y0: f64, y1: f64, inner: (f64, f64, f64, f64)| {
        clip_piece(&ring(rrect(0.0, y0, x1, y1, [0.0, OUTER_R, OUTER_R, 0.0]), rrect(inner.0, inner.1, inner.2, inner.3, [INNER_R; 4])), &[right_of(10.0 + GAP)])
    };
    out.push(d('D', w, vec![solid(rect(0.0, 0.0, 10.0, CAP)), bowl(w, 0.0, CAP, (10.0, WALL, w - STEM - 0.4, CAP - WALL))]));
    // P's stem is 9 wide and its bowl 15 tall; R's stem is 10 (retail).
    let p_bowl = clip_piece(&ring(rrect(0.0, 0.0, 26.0, 15.0, [0.0, OUTER_R, OUTER_R, 0.0]), rrect(10.0, 4.3, 17.0, 10.7, [INNER_R; 4])), &[right_of(9.0 + GAP)]);
    out.push(d('P', 26.0, vec![solid(rect(0.0, 0.0, 9.0, CAP)), p_bowl]));
    let r_bowl = bowl(26.0, 0.0, 15.6, (10.0, 4.3, 17.0, 11.3));
    out.push(d('R', 27.0, vec![solid(rect(0.0, 0.0, 9.6, CAP)), r_bowl, clip_piece(&solid(band((15.6, 25.0), (17.6, 27.0), 13.0, CAP)), &[right_of(10.0 + GAP)])]));
    out.push(d('B', 27.0, vec![solid(rect(0.0, 0.0, 9.6, CAP)), bowl(25.0, 0.0, 15.6, (10.0, 4.3, 16.0, 11.3)), bowl(27.0, 11.4, CAP, (10.0, 15.7, 17.5, CAP - WALL))]));
    out.push(d('J', 16.0, vec![solid(rrect(6.0, 0.0, 16.0, CAP, [0.0, 0.0, 6.0, 0.0])), solid(rrect(0.0, 21.8, 16.0, CAP, [0.0, 0.0, 6.0, 0.0]))]));
    // Diagonals: N, M, W, V, A, X, K, Y, Z.
    // N's diagonal: its left edge through (2.2, 3) falling 0.85 texels right per texel down, 8.7 wide.
    let n_left = |y: f64| 2.2 + 0.85 * (y - 3.0);
    let n_band = band((n_left(0.0), n_left(0.0) + 8.7), (n_left(CAP), n_left(CAP) + 8.7), 0.0, CAP);
    out.push(d('N', 29.0, vec![solid(rect(0.0, 0.0, 9.3, CAP)), solid(rect(20.0, 0.0, 29.0, CAP)), clip_piece(&solid(n_band), &[right_of(9.3 + GAP), left_of(20.0 - GAP)])]));
    // M: retail's V strokes are 7.5 wide and meet the stems with almost no gap (0.4).
    let m_gap = 0.4;
    let m_left = band((5.5, 13.0), (16.5, 24.0), 0.0, CAP);
    let m_right = band((38.0 - 13.0, 38.0 - 5.5), (38.0 - 24.0, 38.0 - 16.5), 0.0, CAP);
    let m_between = [right_of(10.0 + m_gap), left_of(28.5 - m_gap)];
    out.push(d('M', 38.0, vec![solid(rect(0.0, 0.0, 10.0, CAP)), solid(rect(28.5, 0.0, 38.0, CAP)), clip_piece(&solid(m_left), &m_between), clip_piece(&solid(m_right), &m_between)]));
    // V, read off retail's (French NOUVELLE): the left stroke whole to the bottom, the right stroke cut a
    // stencil gap short of the left stroke's inner edge (from (9, 0) to (18, 27)), as W's strokes meet.
    let v_left = band((0.0, 9.0), (10.0, 18.0), 0.0, CAP);
    let v_right = band((20.0, 29.0), (10.5, 19.5), 0.0, CAP);
    out.push(d('V', 29.0, vec![solid(v_left), clip_piece(&solid(v_right), &[side_of((18.0, CAP), (9.0, 0.0), GAP)])]));
    // W, read off retail's: four strokes about 8 wide; the inner two join in a 6-wide peak at x 18–24. The
    // stencil cuts follow the strokes: on the left the inner stroke stops a gap short of the outer one, on
    // the right the outer stroke stops a gap short of the inner one.
    let s1 = band((0.0, 8.0), (8.0, 16.0), 0.0, CAP);
    let s2 = band((16.5, 24.0), (8.5, 16.0), 0.0, CAP);
    let s3 = band((18.0, 25.5), (27.0, 35.0), 0.0, CAP);
    let s4 = band((36.0, 44.0), (27.5, 35.5), 0.0, CAP);
    // `side_of` walking a stroke's right edge from bottom to top keeps what lies right of it.
    let past_s1 = side_of((16.0, CAP), (8.0, 0.0), GAP);
    let past_s3 = side_of((35.0, CAP), (25.5, 0.0), GAP);
    out.push(d('W', 44.0, vec![solid(s1), clip_piece(&solid(s2), &[past_s1]), solid(s3), clip_piece(&solid(s4), &[past_s3])]));
    // A: two legs under a flat top; the crossbar joins the right leg, a gap from the left one.
    let a_outer = vec![(9.0, 0.0), (21.0, 0.0), (30.0, CAP), (20.3, CAP), (18.6, 21.4), (11.4, 21.4), (9.7, CAP), (0.0, CAP)];
    let a_counter = vec![(15.0, 8.5), (17.8, 16.9), (12.2, 16.9)];
    let a_gap = vec![(11.7, 16.9), (12.9, 16.9), (11.3, 21.4), (10.1, 21.4)];
    out.push(d('A', 30.0, vec![vec![a_outer, a_counter, a_gap]]));
    // X: one full falling band 10 wide; the rising band in two arms, each cut a gap away from the falling
    // band's nearer edge (its left edge from (0,0) to (21,27), its right edge from (10,0) to (31,27)).
    let fall = band((0.0, 10.0), (21.0, 31.0), 0.0, CAP);
    let rise = band((21.0, 31.0), (0.0, 10.0), 0.0, CAP);
    let lower_left = side_of((0.0, 0.0), (21.0, CAP), GAP);
    let upper_right = side_of((31.0, CAP), (10.0, 0.0), GAP);
    out.push(d('X', 31.0, vec![solid(fall.clone()), clip_piece(&solid(rise.clone()), &[lower_left]), clip_piece(&solid(rise.clone()), &[upper_right])]));
    // Y: the mirrored X without its lower-right leg: the rising band whole, and the falling band's upper-left
    // arm, cut a gap away from the rising band's left edge (from (22,0) to (0,27)).
    let mirror = |p: &Poly, w: f64| p.iter().map(|&(x, y)| (w - x, y)).collect::<Poly>();
    let upper_left = side_of((21.0, 0.0), (0.0, CAP), GAP);
    out.push(d('Y', 31.0, vec![solid(mirror(&fall, 31.0)), clip_piece(&solid(mirror(&rise, 31.0)), &[upper_left])]));
    // K: a stem and the two arms of an X's right half.
    let k_up = band((17.5, 27.0), (9.0, 18.5), 0.0, 14.0);
    let k_down = band((9.0, 18.5), (17.5, 27.0), 13.0, CAP);
    out.push(d('K', 27.0, vec![solid(rect(0.0, 0.0, 9.6, CAP)), clip_piece(&solid(k_up), &[right_of(9.6 + GAP), above(13.5 - GAP / 2.0)]), clip_piece(&solid(k_down), &[right_of(9.6 + GAP), below(13.5 + GAP / 2.0)])]));
    // Z, read off retail's (German FORTSETZEN): a top bar 5 tall from x 1, a bottom bar 6 tall, and between
    // them, a row clear of each, a diagonal 11 wide from x 14–25 at the top to 2–11 at the bottom.
    let z_diag = band((14.0, 25.0), (1.5, 12.5), 6.0, CAP - 6.0 - 0.8);
    out.push(d('Z', 26.0, vec![solid(rect(1.0, 0.0, 26.0, 5.0)), solid(rect(0.0, CAP - 6.0, 26.0, CAP)), solid(z_diag)]));
    // S, read off retail's: one spine outline — the top bar's left half, round the top-left corner, down the
    // left wall, along a shallow band (14 wide across, falling 1 for every 2 across, centred on the letter:
    // lower edge x = 7 + 2(y − 13.5), upper x = 21 + 2(y − 13.5)) and on round the bottom-right — every joint
    // filleted; the top-right and bottom-left ends are terminals cut off at the centre. The letter is the same
    // turned half round about (14, 13.5).
    let (cut_l, cut_r) = (mid(w) - GAP / 2.0, mid(w) + GAP / 2.0);
    let (in_l, in_r) = (STEM + 0.4, w - STEM - 0.4);
    let lower = |x: f64| 13.5 + (x - 7.0) / 2.0;
    let upper = |x: f64| 13.5 + (x - 21.0) / 2.0;
    let spine = filleted(&[
        ((cut_l, 0.0), 0.0),
        ((0.0, 0.0), OUTER_R),
        ((0.0, lower(0.0)), 6.0),
        ((in_r, lower(in_r)), 4.0),
        ((in_r, CAP - WALL), INNER_R),
        ((cut_r, CAP - WALL), 0.0),
        ((cut_r, CAP), 0.0),
        ((w, CAP), OUTER_R),
        ((w, upper(w)), 6.0),
        ((in_l, upper(in_l)), 4.0),
        ((in_l, WALL), INNER_R),
        ((cut_l, WALL), 0.0),
    ]);
    let top_end = filleted(&[((cut_r, 0.0), 0.0), ((w, 0.0), OUTER_R), ((w, 8.5), 0.0), ((in_r, 8.5), 0.0), ((in_r, WALL), 3.5), ((cut_r, WALL), 0.0)]);
    let bottom_end = filleted(&[((cut_l, CAP), 0.0), ((0.0, CAP), OUTER_R), ((0.0, CAP - 8.5), 0.0), ((in_l, CAP - 8.5), 0.0), ((in_l, CAP - WALL), 3.5), ((cut_l, CAP - WALL), 0.0)]);
    out.push(d('S', w, vec![solid(spine), solid(top_end), solid(bottom_end)]));
    // Q, the user's design (Figma, 2026-11-22): O's two halves, and over the bottom-right quarter a K-style leg
    // moved down and right — from (14.34, 16.22)–(22.64, 16.22) across its top to (20.94, 29.07)–(30.44, 29.07)
    // across its bottom, the leg's left side upright from y 16.22 to 18.20. The O is cut away along the leg's
    // outline and the leg drawn inset by a gap of 0.84 texels (the design's 62-unit stroke masked to the leg's
    // inside): the stencil cut between them.
    let leg: Poly = vec![(22.64, 16.22), (30.44, 29.07), (20.94, 29.07), (14.34, 18.20), (14.34, 16.22)];
    let q_gap = 31.0 / UNIT;
    let mut q = Vec::new();
    for half in [left(&o), right(&o)] {
        q.extend(outside(&leg).iter().map(|h| clip_piece(&half, &[*h])));
    }
    q.push(solid(clip(&leg, &inset(&leg, q_gap))));
    out.push(d('Q', 30.44, q));
    out
}

/// The half-planes holding convex polygon `p`, one per edge, each moved `by` texels inward.
fn inset(p: &Poly, by: f64) -> Vec<Half> {
    let n = p.len() as f64;
    let c = (p.iter().map(|q| q.0).sum::<f64>() / n, p.iter().map(|q| q.1).sum::<f64>() / n);
    (0..p.len())
        .map(|i| {
            let h = side_of(p[i], p[(i + 1) % p.len()], 0.0);
            // The side holding the centroid is the inside.
            let h = if h.a * c.0 + h.b * c.1 <= h.c { h } else { Half { a: -h.a, b: -h.b, c: -h.c } };
            Half { a: h.a, b: h.b, c: h.c - by }
        })
        .collect()
}

/// The half-planes beyond each edge of convex polygon `p`: together they cover everything outside it.
fn outside(p: &Poly) -> Vec<Half> {
    inset(p, 0.0).into_iter().map(|h| Half { a: -h.a, b: -h.b, c: -h.c }).collect()
}

/// The marks accented capitals carry, drawn about x = 0: above the cap they sit on y < 0 (their bottom
/// [`MARK_GAP`] above the cap line), the cedilla below the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Acute,
    Grave,
    Circumflex,
    Diaeresis,
    Tilde,
    Cedilla,
}

/// The gap between the cap line and a mark above it, and a mark's height, in texels.
const MARK_GAP: f64 = 2.0;
const MARK_H: f64 = 5.0;

fn mark(m: Mark) -> Vec<Piece> {
    let (top, bottom) = (-(MARK_GAP + MARK_H), -MARK_GAP);
    match m {
        // A bar slanting up to the right, 4 wide across.
        Mark::Acute => vec![solid(band((-0.5, 3.5), (-3.0, 1.0), top, bottom))],
        Mark::Grave => vec![solid(band((-3.5, 0.5), (-1.0, 3.0), top, bottom))],
        // Two short slashes meeting at a stencil gap.
        Mark::Circumflex => vec![
            solid(band((-2.8, -GAP / 2.0), (-6.3, -3.3), top, bottom)),
            solid(band((GAP / 2.0, 2.8), (3.3, 6.3), top, bottom)),
        ],
        // Two square dots.
        Mark::Diaeresis => vec![solid(rect(-6.0, bottom - 4.3, -2.0, bottom)), solid(rect(2.0, bottom - 4.3, 6.0, bottom))],
        // A wave 13 long and 3.2 thick: high on the left, low on the right.
        Mark::Tilde => {
            let mid_y = (top + bottom) / 2.0;
            let steps = 26;
            let centre = |i: usize| {
                let x = -6.5 + 13.0 * i as f64 / steps as f64;
                (x, mid_y + 1.2 * (x * std::f64::consts::PI / 6.5).sin())
            };
            let mut poly: Poly = (0..=steps).map(|i| centre(i)).map(|(x, y)| (x, y - 1.6)).collect();
            poly.extend((0..=steps).rev().map(|i| centre(i)).map(|(x, y)| (x, y + 1.6)));
            vec![solid(poly)]
        }
        // A hook under the letter: down from the baseline, then left.
        Mark::Cedilla => vec![solid(filleted(&[
            ((-1.0, CAP + 1.2), 0.0),
            ((2.2, CAP + 1.2), 0.0),
            ((2.2, CAP + 5.5), 1.5),
            ((-3.5, CAP + 5.5), 0.0),
            ((-3.5, CAP + 3.3), 0.0),
            ((-1.0, CAP + 3.3), 0.0),
        ]))],
    }
}

fn translate(piece: &Piece, dx: f64, dy: f64) -> Piece {
    piece.iter().map(|p| p.iter().map(|&(x, y)| (x + dx, y + dy)).collect()).collect()
}

/// The accented capitals of the game's languages (French, German, Spanish, Italian) and Portuguese, as
/// base letter and mark; ß becomes SS when a label is uppercased, so it needs no letter.
const ACCENTED: &[(char, char, Mark)] = &[
    ('À', 'A', Mark::Grave),
    ('Á', 'A', Mark::Acute),
    ('Â', 'A', Mark::Circumflex),
    ('Ã', 'A', Mark::Tilde),
    ('Ä', 'A', Mark::Diaeresis),
    ('Ç', 'C', Mark::Cedilla),
    ('È', 'E', Mark::Grave),
    ('É', 'E', Mark::Acute),
    ('Ê', 'E', Mark::Circumflex),
    ('Ë', 'E', Mark::Diaeresis),
    ('Ì', 'I', Mark::Grave),
    ('Í', 'I', Mark::Acute),
    ('Î', 'I', Mark::Circumflex),
    ('Ï', 'I', Mark::Diaeresis),
    ('Ñ', 'N', Mark::Tilde),
    ('Ò', 'O', Mark::Grave),
    ('Ó', 'O', Mark::Acute),
    ('Ô', 'O', Mark::Circumflex),
    ('Õ', 'O', Mark::Tilde),
    ('Ö', 'O', Mark::Diaeresis),
    ('Ù', 'U', Mark::Grave),
    ('Ú', 'U', Mark::Acute),
    ('Û', 'U', Mark::Circumflex),
    ('Ü', 'U', Mark::Diaeresis),
    ('Ÿ', 'Y', Mark::Diaeresis),
];

/// The accented capitals, each its base letter with its mark centred over it, and Œ (O's left half and E).
pub fn accented(capitals: &[Draft]) -> Result<Vec<Draft>, String> {
    let base = |ch: char| capitals.iter().find(|d| d.ch == ch).ok_or_else(|| format!("no capital {ch}"));
    let mut out = Vec::new();
    for &(ch, b, m) in ACCENTED {
        let b = base(b)?;
        // Centred on the letter; on A, on its apex (the letter is symmetric about its middle).
        let x = b.width / 2.0;
        let mut pieces = b.pieces.clone();
        pieces.extend(mark(m).iter().map(|p| translate(p, x, 0.0)));
        out.push(Draft { ch, width: b.width, pieces });
    }
    let o = base('O')?;
    let e = base('E')?;
    let mut oe: Vec<Piece> = o.pieces.first().cloned().into_iter().collect();
    oe.extend(e.pieces.iter().map(|p| translate(p, 14.0 + GAP / 2.0, 0.0)));
    out.push(Draft { ch: 'Œ', width: 14.0 + GAP / 2.0 + e.width, pieces: oe });
    Ok(out)
}

/// The SVG of `draft`: one even-odd `path` per piece, the cap from y = 0 to 1000, with [`ABOVE`] texels of
/// room above it and [`BELOW`] below.
pub fn svg(draft: &Draft) -> String {
    let w = draft.width * UNIT;
    let (top, height) = (-ABOVE * UNIT, (ABOVE + CAP + BELOW) * UNIT);
    let mut out = String::new();
    let _ = writeln!(out, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 {top:.3} {w:.3} {height:.3}" width="{w:.3}" height="{height:.3}">"#);
    let _ = writeln!(out, "  <!-- U+{:04X} {}: the main-menu stencil, drafted from the parts of retail's shell_MAINMENU_text. The cap is y 0 to 1000 (27 texels on the menu); the view has 10 texels' room above it and 6 below. -->", draft.ch as u32, draft.ch);
    for piece in &draft.pieces {
        let mut path = String::new();
        for poly in piece {
            for (i, (x, y)) in poly.iter().enumerate() {
                let _ = write!(path, "{}{:.1} {:.1} ", if i == 0 { "M" } else { "L" }, x * UNIT, y * UNIT);
            }
            path.push_str("Z ");
        }
        let _ = writeln!(out, r#"  <path fill="black" fill-rule="evenodd" d="{}"/>"#, path.trim_end());
    }
    out.push_str("</svg>\n");
    out
}

/// `mercs2_probe lettering-draft <out_dir>`: write every drafted capital as `<out_dir>/U+XXXX.svg`.
pub fn run(args: &[String]) -> Result<(), String> {
    let [dir] = args else { return Err("usage: mercs2_probe lettering-draft <out_dir>".into()) };
    // Drafts overwrite: once a letter is edited by hand, it is the source and this is not run over it.
    std::fs::create_dir_all(dir).map_err(|e| format!("{dir}: {e}"))?;
    let mut all = capitals();
    all.extend(accented(&all)?);
    for draft in &all {
        let path = format!("{dir}/U+{:04X}.svg", draft.ch as u32);
        std::fs::write(&path, svg(draft)).map_err(|e| format!("write {path}: {e}"))?;
    }
    println!("wrote {} letters to {dir}: {}", all.len(), all.iter().map(|d| d.ch).collect::<String>());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clip_keeps_the_inside_of_a_half_plane() {
        let c = clip(&rect(0.0, 0.0, 10.0, 10.0), &[left_of(4.0)]);
        let xs: Vec<f64> = c.iter().map(|p| p.0).collect();
        assert!(xs.iter().all(|&x| x <= 4.0 + 1e-9) && xs.contains(&4.0), "{c:?}");
    }

    #[test]
    fn every_capital_is_drafted_once() {
        let all = capitals();
        let mut chars: Vec<char> = all.iter().map(|d| d.ch).collect();
        chars.sort();
        assert_eq!(chars.iter().collect::<String>(), "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    }
}
