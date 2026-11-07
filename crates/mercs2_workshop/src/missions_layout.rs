//! Where the Missions canvas puts each node that has no position yet ([`layout`]), and how readable a
//! layout is as numbers ([`score`]), which the layout is gated on over every canvas of the corpus
//! (`docs/modernization/workshop_pages/missions.md`, Canvas layout). Two passes, so wires run short and
//! level:
//!
//! * **Feeders** (data nodes: no exec pins) sit left of the node they feed, each with its output pin
//!   level with the input it feeds ([`Rules::feeder_drop`] below it), a feeder's own feeders further left,
//!   the feeders of one node stacked down in pin order. The node they feed is pushed right to make room,
//!   so no feeder wire runs backward.
//! * **The exec chain** runs left to right, each statement's exec input level with the exec output it
//!   continues from. A statement with blocks (a branch's arms, a loop's body, a callback, including a
//!   callback held by one of its feeders) ends its row: its first block runs on a row
//!   [`Rules::first_arm_rise`] above the output it hangs from, the others stack below in port order, and the
//!   code after the statement continues on a row below them all, right of the statement. Every block is
//!   laid out whole and placed as a box, so no two boxes overlap.
//! * What no chain reaches (statements left unwired by an edit, loose values) goes in one row below
//!   everything.
//!
//! Nothing wraps: a long function is wide, and the canvas pans.

use std::collections::{HashMap, HashSet};

use egui::{Pos2, Rect, Vec2};

use crate::blueprint::{Graph, NodeId, ENTRY};
use crate::missions::row_y;
#[cfg(test)]
use crate::missions::{pin_at, PinRef};

/// The layout's spacing and placement rules, in canvas units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// Between one statement (with its feeders) and the next on a row.
    pub gap_x: f32,
    /// Between boxes stacked one under another (a statement's blocks, and the code after them).
    pub gap_y: f32,
    /// Between a feeder and the node it feeds.
    pub feeder_gap_x: f32,
    /// Between feeders stacked one under another.
    pub feeder_gap_y: f32,
    /// How far a feeder's output pin sits below the input it feeds (0: the wire is level).
    pub feeder_drop: f32,
    /// How far a statement's first block (a branch's `then`, a loop's body) rises above the exec output it
    /// hangs from.
    pub first_arm_rise: f32,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { gap_x: 56.0, gap_y: 36.0, feeder_gap_x: 40.0, feeder_gap_y: 18.0, feeder_drop: 0.0, first_arm_rise: 52.0 }
    }
}

/// Nodes placed relative to one corner, with the box they fill.
#[derive(Default)]
struct Boxed {
    pos: HashMap<NodeId, Pos2>,
    bounds: Option<Rect>,
}

impl Boxed {
    fn put(&mut self, id: NodeId, at: Pos2, size: Vec2) {
        self.pos.insert(id, at);
        let r = Rect::from_min_size(at, size);
        self.bounds = Some(self.bounds.map_or(r, |b| b.union(r)));
    }

    fn bounds(&self) -> Rect {
        self.bounds.unwrap_or(Rect::ZERO)
    }

    fn shift(mut self, by: Vec2) -> Boxed {
        for p in self.pos.values_mut() {
            *p += by;
        }
        self.bounds = self.bounds.map(|b| b.translate(by));
        self
    }

    fn take(&mut self, other: Boxed) {
        self.pos.extend(other.pos);
        if let Some(r) = other.bounds {
            self.bounds = Some(self.bounds.map_or(r, |b| b.union(r)));
        }
    }
}

struct Layout<'a> {
    g: &'a Graph,
    sizes: &'a [Vec2],
    rules: Rules,
    placed: HashSet<NodeId>,
}

impl Layout<'_> {
    /// Claim `id` for placement; `false` when it is placed already (a cycle, or a value fed to two pins).
    fn claim(&mut self, id: NodeId) -> bool {
        self.placed.insert(id)
    }

    /// The data nodes feeding `id`, in input-pin order.
    fn feeders(&self, id: NodeId) -> Vec<(usize, NodeId)> {
        let mut f: Vec<(usize, NodeId)> = self
            .g
            .data
            .iter()
            .filter(|w| w.to.0 == id && w.from.0 != ENTRY && !self.g.nodes[w.from.0].exec_in)
            .map(|w| (w.to.1, w.from.0))
            .collect();
        f.sort();
        f
    }

    /// Place the feeders of `id` (at `at`) into `out`, left of it; returns the feeders placed, outermost
    /// first, so their callbacks can be laid out.
    fn feed(&mut self, id: NodeId, at: Pos2, out: &mut Boxed) -> Vec<NodeId> {
        let r = self.rules;
        let n = &self.g.nodes[id];
        let mut placed = Vec::new();
        let mut below: Option<f32> = None;
        for (pin, f) in self.feeders(id) {
            if !self.claim(f) {
                continue;
            }
            let size = self.sizes[f];
            let mut sub = Boxed::default();
            sub.put(f, Pos2::ZERO, size);
            let inner = self.feed(f, Pos2::ZERO, &mut sub);
            let pin_y = at.y + row_y(n.exec_in as usize + pin);
            let mut y = pin_y - row_y(self.g.nodes[f].exec_outs.len()) + r.feeder_drop;
            if let Some(b) = below {
                y = y.max(b + r.feeder_gap_y - sub.bounds().min.y);
            }
            let x = at.x - r.feeder_gap_x - size.x;
            let sub = sub.shift(Vec2::new(x, y));
            below = Some(sub.bounds().max.y);
            out.take(sub);
            placed.push(f);
            placed.extend(inner);
        }
        placed
    }

    /// Lay out the chain that starts at `first`, its first node's corner at the origin.
    fn chain(&mut self, first: NodeId) -> Boxed {
        let r = self.rules;
        let mut out = Boxed::default();
        let (mut x, mut y) = (0.0_f32, 0.0_f32);
        let mut cur = Some(first);
        let mut head = true;
        while let Some(id) = cur {
            if !self.claim(id) {
                break;
            }
            let size = self.sizes[id];
            let mut nb = Boxed::default();
            nb.put(id, Pos2::ZERO, size);
            let feeders = self.feed(id, Pos2::ZERO, &mut nb);
            // The chain's first node sits at the origin (its feeders reach left of it); every later one
            // leaves room for its feeders after the node before it.
            let nx = if head { 0.0 } else { x - nb.bounds().min.x };
            let nb = nb.shift(Vec2::new(nx, y));
            let at = nb.pos[&id];
            out.take(nb);
            head = false;

            let n = &self.g.nodes[id];
            // The entry's body continues its row; every other exec output but `next` is a block.
            let next_out = if id == ENTRY { Some(0) } else { n.next };
            let mut arms: Vec<(NodeId, usize)> =
                (0..n.exec_outs.len()).filter(|&k| Some(k) != next_out).map(|k| (id, k)).collect();
            for &f in &feeders {
                arms.extend((0..self.g.nodes[f].exec_outs.len()).map(|k| (f, k)));
            }
            arms.retain(|&(from, k)| self.g.exec_target(from, k).is_some_and(|t| !self.placed.contains(&t)));
            let next = next_out.and_then(|k| self.g.exec_target(id, k)).filter(|t| !self.placed.contains(t));
            let right = at.x + size.x + r.gap_x;
            if arms.is_empty() {
                let Some(t) = next else { break };
                // Level: the next statement's exec input at the height of the output it continues from.
                y += row_y(next_out.expect("a target has an output")) - row_y(0);
                x = right;
                cur = Some(t);
                continue;
            }
            let mut bottom = at.y + size.y;
            for (i, &(from, k)) in arms.iter().enumerate() {
                let Some(t) = self.g.exec_target(from, k).filter(|t| !self.placed.contains(t)) else { continue };
                let arm = self.chain(t);
                let ab = arm.bounds();
                let pin_y = out.pos[&from].y + row_y(k);
                let mut ay = pin_y - row_y(0) - if i == 0 { r.first_arm_rise } else { 0.0 };
                if i > 0 {
                    ay = ay.max(bottom + r.gap_y - ab.min.y);
                }
                let arm = arm.shift(Vec2::new(right - ab.min.x, ay));
                bottom = bottom.max(arm.bounds().max.y);
                out.take(arm);
            }
            if let Some(t) = next {
                // The code after the statement: a row of its own below the blocks, right of the statement.
                let rest = self.chain(t);
                let rb = rest.bounds();
                let rest = rest.shift(Vec2::new(right - rb.min.x, bottom + r.gap_y - rb.min.y));
                out.take(rest);
            }
            break;
        }
        out
    }
}

/// Positions for every node of `g` (node `sizes` in canvas units) by `rules`.
pub fn layout(g: &Graph, sizes: &[Vec2], rules: &Rules) -> HashMap<NodeId, Pos2> {
    let mut l = Layout { g, sizes, rules: *rules, placed: HashSet::new() };
    let mut all = l.chain(ENTRY);
    // What no chain reaches, in one row underneath: statements with their feeders and blocks, then values.
    let mut x = all.bounds().min.x;
    let y = all.bounds().max.y + rules.gap_y * 2.0;
    for id in (0..g.nodes.len()).filter(|i| g.nodes[*i].exec_in).chain((0..g.nodes.len()).filter(|i| !g.nodes[*i].exec_in)) {
        if l.placed.contains(&id) {
            continue;
        }
        let b = l.chain(id);
        let bb = b.bounds();
        let b = b.shift(Vec2::new(x - bb.min.x, y - bb.min.y));
        x = b.bounds().max.x + rules.gap_x;
        all.take(b);
    }
    all.pos
}

/// What a layout does to one canvas's wires and nodes (the tests' gate; the page does not score).
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Score {
    pub wires: usize,
    /// The sum of each wire's straight length, pin to pin.
    pub length: f32,
    /// The sum of each wire's vertical drift, pin to pin.
    pub drift: f32,
    /// Wires that end left of where they start (drawn as a loop back).
    pub backward: usize,
    /// Pairs of wires whose straight segments cross, neither sharing a node with the other.
    pub crossings: usize,
    /// Pairs of nodes whose boxes overlap.
    pub overlaps: usize,
    pub width: f32,
    pub height: f32,
    /// Wire count and vertical drift by kind: [exec wires, feeder wires (from a data node), wires from a
    /// statement's or the entry's outputs (locals, loop variables, parameters)].
    pub kinds: [(usize, f32); 3],
}

#[cfg(test)]
impl Score {
    /// Sum `o` into this one (width and height take the larger).
    pub fn add(&mut self, o: &Score) {
        self.wires += o.wires;
        self.length += o.length;
        self.drift += o.drift;
        self.backward += o.backward;
        self.crossings += o.crossings;
        self.overlaps += o.overlaps;
        self.width = self.width.max(o.width);
        self.height = self.height.max(o.height);
        for (k, (n, d)) in o.kinds.iter().enumerate() {
            self.kinds[k].0 += n;
            self.kinds[k].1 += d;
        }
    }
}

/// Which of [`Score::kinds`] a wire from `from` is: 0 exec, 1 feeder, 2 a statement's or the entry's output.
#[cfg(test)]
fn kind(g: &Graph, from: NodeId, exec: bool) -> usize {
    match (exec, from != crate::blueprint::ENTRY && !g.nodes[from].exec_in) {
        (true, _) => 0,
        (false, true) => 1,
        (false, false) => 2,
    }
}

/// Every wire of `g` as (from node, to node, from pin centre, to pin centre, its [`kind`]), at `pos`.
#[cfg(test)]
fn wires(g: &Graph, sizes: &[Vec2], pos: &HashMap<NodeId, Pos2>) -> Vec<(NodeId, NodeId, Pos2, Pos2, usize)> {
    let at = |id: NodeId, pin: PinRef| pin_at(&g.nodes[id], pos[&id], sizes[id], pin);
    let exec = g.exec.iter().map(|w| {
        (w.from.0, w.to, at(w.from.0, PinRef::ExecOut(w.from.0, w.from.1)), at(w.to, PinRef::ExecIn(w.to)), kind(g, w.from.0, true))
    });
    let data = g.data.iter().map(|w| {
        let (a, b) = (at(w.from.0, PinRef::Out(w.from.0, w.from.1)), at(w.to.0, PinRef::In(w.to.0, w.to.1)));
        (w.from.0, w.to.0, a, b, kind(g, w.from.0, false))
    });
    exec.chain(data).collect()
}

/// Whether segments a–b and c–d cross (touching at an end does not count).
#[cfg(test)]
fn cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let side = |p: Pos2, q: Pos2, r: Pos2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2, d3, d4) = (side(c, d, a), side(c, d, b), side(a, b, c), side(a, b, d));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// The score of `pos` (every node of `g` placed) with node `sizes`.
#[cfg(test)]
pub fn score(g: &Graph, sizes: &[Vec2], pos: &HashMap<NodeId, Pos2>) -> Score {
    let w = wires(g, sizes, pos);
    let mut s = Score { wires: w.len(), ..Score::default() };
    for &(_, _, a, b, k) in &w {
        s.length += a.distance(b);
        s.drift += (b.y - a.y).abs();
        s.kinds[k].0 += 1;
        s.kinds[k].1 += (b.y - a.y).abs();
        if b.x < a.x {
            s.backward += 1;
        }
    }
    // Wires are drawn as curves; crossings are counted on the straight pin-to-pin segments.
    for (i, &(f1, t1, a, b, _)) in w.iter().enumerate() {
        for &(f2, t2, c, d, _) in &w[i + 1..] {
            let shared = f1 == f2 || f1 == t2 || t1 == f2 || t1 == t2;
            if !shared && cross(a, b, c, d) {
                s.crossings += 1;
            }
        }
    }
    let rects: Vec<Rect> = (0..g.nodes.len()).map(|i| Rect::from_min_size(pos[&i], sizes[i])).collect();
    for (i, r) in rects.iter().enumerate() {
        s.overlaps += rects[i + 1..].iter().filter(|o| o.shrink(0.5).intersects(r.shrink(0.5))).count();
    }
    if let Some(b) = rects.iter().copied().reduce(|a, b| a.union(b)) {
        s.width = b.width();
        s.height = b.height();
    }
    s
}
