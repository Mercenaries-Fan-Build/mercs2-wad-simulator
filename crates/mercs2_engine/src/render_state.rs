//! Per-object render state and the engine's per-segment draw gate.
//!
//! The retail gate lives in `FUN_004722a0` / `FUN_00472a50` (the second is the main pass; the first
//! is the blob-shadow pass). Per SEGM record, stride 4, in the array at `M+0x50` where
//! `M = *(OBJ+0x1e0)`:
//!
//! ```text
//! draw(seg) iff renderable[seg.seg_id] != 0                              // (1) geometry exists
//!             && (OBJ.view_state@0x352 & seg.lod_mask@+3) != 0           // (2) LOD-rung overlap
//!             && (seg.node@+0 < 0 || OBJ.node_enable@0x2a0[seg.node])    // (3) destruction SHOW/Hide
//! ```
//!
//! Clause (2) is an **ANY-bit** overlap (`& != 0`), verified in the live image. Clause (3) is the
//! destruction state machine's SHOW/Hide table. **They are orthogonal axes** — LOD rung × destruction
//! state — not the same mechanism, and clause (3) is what hides a wreck. Getting this backwards is
//! what put an intact helicopter and its wreckage on screen at once.
//!
//! `view_state` is NOT a constant. It is recomputed every frame from camera distance
//! (`FUN_00470740` → `FUN_0047724e`). Our old hardcoded `active_bit = 0x01` happened to mean
//! "LOD rung 0", which is why close-up previews looked mostly right.
//!
//! See `docs/modernization/model_render_gate_spec.md`.

/// Per-object, per-frame render state. Lives on the ENTITY, never on the model — two instances of the
/// same model must be able to sit at different LOD rungs and different damage states.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderState {
    /// The LOD rung `n` this object is at, from [`lod_rung`].
    pub lod: u8,
    /// `OBJ+0x352`: the bitmask of rungs currently accepted, from [`view_state`]. Clause (2).
    pub view_state: u8,
    /// `OBJ+0x2a0`: one byte per HIER node, written by the destruction state machine. Clause (3).
    /// Empty means "no table" — every node passes, matching a null pointer at `+0x2a0`.
    pub node_enable: Vec<bool>,
}

impl RenderState {
    /// An object with no destruction machine sitting at LOD rung 0 — the pristine close-up default.
    pub fn rung0(node_count: usize) -> RenderState {
        RenderState { lod: 0, view_state: 0x01, node_enable: vec![true; node_count] }
    }

    /// Clause (3): a segment's node must be enabled. A negative node means "no node" → always visible
    /// (`*psVar1 < 0 ||` in the decomp). An empty table means no table → always visible.
    pub fn node_visible(&self, node: i16) -> bool {
        if node < 0 || self.node_enable.is_empty() {
            return true;
        }
        // Out of range reads as visible rather than panicking: retail indexes unchecked, and a model
        // whose SEGM references a node past the table is a data bug, not a reason to drop geometry.
        self.node_enable.get(node as usize).copied().unwrap_or(true)
    }

    /// Clauses (2) AND (3) — the whole gate, minus clause (1) which is "the group exists at all".
    pub fn segment_visible(&self, lod_mask: u8, node: i16) -> bool {
        (self.view_state & lod_mask) != 0 && self.node_visible(node)
    }
}

/// `FUN_00470740` — pick the LOD rung `n` from camera distance.
///
/// ```text
/// fVar4 = dist / (model->scale@0x84 * k);
/// cVar2 = (char)((uint)fVar4 >> 0x17) + -0x7e;      // f32 exponent field - 126
/// if (cVar2 < model[0x80]) cVar2 = model[0x80];     // clamp to minLOD
/// else if (model[0x7c]-1 < cVar2) cVar2 = model[0x7c]-1;
/// ```
///
/// The exponent trick is `floor(log2(x)) + 1` for normalized positive `x`: each doubling of distance
/// advances one rung. `max_lod` is the model's rung COUNT, so the top index is `max_lod - 1`.
pub fn lod_rung(dist: f32, scale: f32, k: f32, min_lod: u8, max_lod: u8) -> u8 {
    let denom = scale * k;
    // Retail divides unguarded; a zero/degenerate denominator here would produce inf/NaN whose
    // exponent bits are 0xFF -> rung 129. Clamp to the closest rung instead of inventing one.
    if !denom.is_finite() || denom <= 0.0 || !dist.is_finite() || dist <= 0.0 {
        return min_lod;
    }
    let ratio = dist / denom;
    let exp = ((ratio.to_bits() >> 23) as i32) - 0x7e;
    let top = max_lod.saturating_sub(1);
    exp.clamp(min_lod as i32, top.max(min_lod) as i32) as u8
}

/// `FUN_0047724e` — compose `view_state` from the rung.
///
/// ```text
/// *(short*)(OBJ+0x34c) = 1 << (n     & 0x1f);
/// *(short*)(OBJ+0x350) = 1 << (n + 1 & 0x1f);
/// *(short*)(OBJ+0x34e) = (minLOD < n) ? 1 << (n-1 & 0x1f) : 0;   // zero at the bottom rung
///
/// if ((OBJ[0x12] >> 9 & 1) == 0)  OBJ[0x352] = OBJ[0x34c];                       // single bit
/// else                            OBJ[0x352] = OBJ[0x34e]|OBJ[0x34c]|OBJ[0x350]; // 3-rung spread
/// ```
///
/// `cross_fade` is bit 9 of the object flags word at `+0x12`. The engine keeps the terms in `u16`s
/// and ANDs a byte, so rung 7's `n+1` term (`0x100`) simply contributes nothing to the mask.
pub fn view_state(n: u8, min_lod: u8, cross_fade: bool) -> u8 {
    let bit = |i: u32| -> u16 { 1u16 << (i & 0x1f) };
    let b_n = bit(n as u32);
    if !cross_fade {
        return b_n as u8;
    }
    let b_next = bit(n as u32 + 1);
    let b_prev = if min_lod < n { bit(n as u32 - 1) } else { 0 };
    (b_prev | b_n | b_next) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_is_any_bit_not_all_bits() {
        // The decomp is `(view_state & mask) != 0`. A mask-0x03 segment (present at rungs 0 and 1)
        // DOES draw at rung 0. The "all-bits" reading would drop it.
        let rs = RenderState::rung0(0);
        assert!(rs.segment_visible(0x03, -1));
        assert!(rs.segment_visible(0x01, -1));
        assert!(rs.segment_visible(0x7F, -1), "present-at-every-rung segments always draw");
        assert!(!rs.segment_visible(0x02, -1), "rung-1-only segment is absent at rung 0");
        assert!(!rs.segment_visible(0x70, -1), "rungs 4-6 only");
    }

    #[test]
    fn a_zero_mask_overlaps_nothing_and_never_draws() {
        // Under ANY-bit a zero mask can never overlap. (The legacy builder special-cased mask==0 as
        // always-on; that quirk does not exist in the engine.)
        let rs = RenderState::rung0(0);
        assert!(!rs.segment_visible(0x00, -1));
    }

    #[test]
    fn negative_node_is_always_visible_even_when_disabled_neighbours_are_not() {
        let rs = RenderState { lod: 0, view_state: 0x01, node_enable: vec![false, false] };
        assert!(rs.segment_visible(0x01, -1), "node < 0 skips clause 3");
        assert!(!rs.segment_visible(0x01, 0), "node 0 is disabled");
    }

    #[test]
    fn an_empty_table_means_no_table_so_everything_passes_clause_3() {
        // A model with no destruction machine has a null +0x2a0. Every node passes.
        let rs = RenderState::rung0(0);
        assert!(rs.node_visible(5));
        assert!(rs.segment_visible(0x01, 5));
    }

    #[test]
    fn clause_3_hides_the_wreck_independently_of_lod() {
        // The two axes are orthogonal: the wreck node is absent at EVERY rung, the body at none.
        let mut rs = RenderState { lod: 0, view_state: 0x7F, node_enable: vec![true, false] };
        assert!(rs.segment_visible(0x7F, 0), "body: node enabled");
        assert!(!rs.segment_visible(0x7F, 1), "wreck: node disabled, at every rung");
        rs.view_state = view_state(3, 0, false);
        assert!(rs.segment_visible(0x08, 0));
        assert!(!rs.segment_visible(0x08, 1), "still hidden three rungs out");
    }

    #[test]
    fn out_of_range_node_reads_as_visible_rather_than_panicking() {
        let rs = RenderState { lod: 0, view_state: 0x01, node_enable: vec![true] };
        assert!(rs.node_visible(99));
    }

    #[test]
    fn view_state_single_bit_vs_cross_fade_spread() {
        assert_eq!(view_state(0, 0, false), 0x01);
        assert_eq!(view_state(3, 0, false), 0x08);
        // Bottom rung: the n-1 term is zeroed (minLOD == n), so only {n, n+1}.
        assert_eq!(view_state(0, 0, true), 0x03);
        // Middle rung: {n-1, n, n+1}.
        assert_eq!(view_state(3, 0, true), 0x1C);
        // Rung 7's n+1 term is 0x100 — kept in a u16 by the engine, contributes nothing to the byte.
        assert_eq!(view_state(7, 0, true), 0xC0);
    }

    #[test]
    fn cross_fade_at_rung_0_would_resurrect_the_hair_overdraw() {
        // Why the spread matters: mattias's hair tiers are masks 1/2/4/8. At rung 0 with cross-fade
        // ON, view_state = 0x03 draws BOTH the rung-0 and rung-1 hair. That is the triple-hair bug,
        // and it is why `view_state` cannot be assumed to be a single bit.
        let single = RenderState { lod: 0, view_state: view_state(0, 0, false), node_enable: vec![] };
        let spread = RenderState { lod: 0, view_state: view_state(0, 0, true), node_enable: vec![] };
        assert!(!single.segment_visible(0x02, -1));
        assert!(spread.segment_visible(0x02, -1));
    }

    #[test]
    fn lod_rung_advances_one_step_per_doubling_of_distance() {
        // exponent(x) - 126 == floor(log2 x) + 1
        let (min, max) = (0u8, 8u8);
        assert_eq!(lod_rung(1.0, 1.0, 1.0, min, max), 1);
        assert_eq!(lod_rung(2.0, 1.0, 1.0, min, max), 2);
        assert_eq!(lod_rung(4.0, 1.0, 1.0, min, max), 3);
        assert_eq!(lod_rung(8.0, 1.0, 1.0, min, max), 4);
        // Scale divides the distance: a 2x bigger model stays a rung closer.
        assert_eq!(lod_rung(8.0, 2.0, 1.0, min, max), 3);
    }

    #[test]
    fn lod_rung_clamps_to_the_models_own_min_and_max() {
        assert_eq!(lod_rung(0.001, 1.0, 1.0, 2, 8), 2, "near clamps up to minLOD");
        assert_eq!(lod_rung(1.0e30, 1.0, 1.0, 0, 4), 3, "far clamps to maxLOD-1");
        // A degenerate model (max_lod = 0) must not underflow to rung 255.
        assert_eq!(lod_rung(10.0, 1.0, 1.0, 0, 0), 0);
    }

    #[test]
    fn lod_rung_is_defined_for_degenerate_inputs() {
        // Retail divides unguarded; inf/NaN exponents would yield rung 129.
        assert_eq!(lod_rung(10.0, 0.0, 1.0, 1, 8), 1);
        assert_eq!(lod_rung(f32::NAN, 1.0, 1.0, 1, 8), 1);
        assert_eq!(lod_rung(0.0, 1.0, 1.0, 1, 8), 1);
        assert_eq!(lod_rung(-5.0, 1.0, 1.0, 1, 8), 1);
    }
}
