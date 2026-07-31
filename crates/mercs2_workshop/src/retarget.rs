//! The Skeleton workbench's retarget, re-exported from the library.
//!
//! # Why this is a shim
//!
//! The classifier, the per-convention bone tables and [`Retarget::joint_table`] moved to
//! `mercs2_formats::retarget`. They decide which target bone every source bone maps onto, which is
//! the *lowering* decision — and while they lived in this binary crate, `mercs2_quartermaster` had
//! no way to reach them. That is why a Shipment's `retarget:` was rejected as unsupported: the
//! Workshop could preview a remap nothing else could reproduce.
//!
//! Preview == shipped only holds if there is one mapper. There is now.
//!
//! What stayed here is genuinely preview-only: GPU-side rebind matrices and an animation `BoneRig`.
//! Those pull `glam` and `mercs2_engine`, neither of which belongs in the format layer, and neither
//! is on the path from a `.glb` to a WAD block. Both bodies are the originals, moved verbatim —
//! they are load-bearing enough that paraphrasing them from their own doc comments produced a
//! materially different matrix order on the first attempt.

pub use mercs2_formats::retarget::*;

/// The preview-only half of the retarget, kept as an extension so existing `rt.method(..)` call
/// sites read unchanged.
pub trait RetargetPreview {
    fn rebind_matrices(&self, table: &[usize], tgt_bind: &[glam::Mat4]) -> Vec<glam::Mat4>;
    fn animation_rig(&self, table: &[usize], target_hashes: &[u32])
        -> Vec<mercs2_engine::mesh::BoneRig>;
}

impl RetargetPreview for Retarget {
    /// The per-source-bone rebind matrices that re-anchor a foreign mesh onto the target skeleton.
    ///
    /// A **hybrid**, because the two bone classes want different treatment (established by
    /// rendering):
    ///
    /// - **Deform bones** compose frames: `rebind[s] = tgt_bind[t] · S(uscale) · src_ibm[s]`.
    /// - **Gear/cosmetic bones and anything mapped onto an ATTACHMENT point** (`bone_attach_*`) use
    ///   POSITION + SCALE only, keeping the source orientation: `rebind[s] = T(p_t) · S · T(−p_s)`.
    ///   An attach point's frame is unrelated to the gear bone's frame, so frame composition rotates
    ///   the (often long, off-centre) sling/pouch geometry by a garbage delta and flings it across
    ///   the hips — the exploded gear. Snapping by position parks it at the mount without that
    ///   rotation.
    ///
    /// `S(uscale)` absorbs the source→target size difference (inches→metres for a CoD import).
    /// Returns one matrix per source bone, or an empty vec when neither source IBMs nor positions
    /// are available (the caller skips the rebind).
    fn rebind_matrices(&self, table: &[usize], tgt_bind: &[glam::Mat4]) -> Vec<glam::Mat4> {
        use glam::{Mat4, Vec3};
        let n = self.source_bones.len();
        let has_ibm = self.source_ibm.len() == n;
        let has_pos = self.source_pos.len() == n && !self.source_pos.is_empty();
        if tgt_bind.is_empty() || (!has_ibm && !has_pos) {
            return Vec::new();
        }
        let scale_m = Mat4::from_scale(Vec3::splat(self.unit_scale()));
        let is_attach = |t: usize| {
            self.target_bones
                .get(t)
                .map(|nm| nm.to_ascii_lowercase().contains("attach"))
                .unwrap_or(false)
        };
        // Position + scale, no rotation — for gear/attach bones.
        let snap = |s: usize, tb: &Mat4| -> Mat4 {
            let p_s = Vec3::from(self.source_pos.get(s).copied().unwrap_or([0.0; 3]));
            Mat4::from_translation(tb.w_axis.truncate()) * scale_m * Mat4::from_translation(-p_s)
        };
        (0..n)
            .map(|s| {
                let t = table.get(s).copied().unwrap_or(0).min(tgt_bind.len() - 1);
                let tb = tgt_bind[t];
                let gear = is_attach(t) || is_aux_source(&self.source_bones[s]);
                if gear && has_pos {
                    snap(s, &tb)
                } else if has_ibm {
                    tb * scale_m * Mat4::from_cols_array_2d(&self.source_ibm[s])
                } else {
                    snap(s, &tb)
                }
            })
            .collect()
    }

    /// Build the imported character's OWN skeleton as a `BoneRig` array, **relabeled** with the
    /// target's bone-name hashes so the target's animation clips bind to it — the non-destructive
    /// retarget. The mesh and its skin weights are left untouched; only the bone *identities*
    /// change, so the target's clips (which are keyed by bone-name hash) drive the imported bones
    /// while the character keeps its own bind pose, proportions and off-body gear. A source bone
    /// whose target is out of range keeps hash 0 (no clip binds it → it stays at bind).
    ///
    /// Built from the glTF inverse-bind matrices (`source_ibm`, column-major / column-vector) and
    /// the `source_parents` chain. `target_hashes[t]` is the target HIER bone-name hash for target
    /// index `t`. Returns an empty vec if the import carried no skeleton.
    fn animation_rig(
        &self,
        table: &[usize],
        target_hashes: &[u32],
    ) -> Vec<mercs2_engine::mesh::BoneRig> {
        use glam::Mat4;
        use mercs2_engine::mesh::BoneRig;
        let n = self.source_bones.len();
        if self.source_ibm.len() != n || n == 0 {
            return Vec::new();
        }
        // Column-vector world/inv-bind per source joint. glTF IBM is already column-major, so
        // `from_cols_array_2d` reads it directly; its inverse is the bind world matrix.
        let inv_g: Vec<Mat4> = self.source_ibm.iter().map(Mat4::from_cols_array_2d).collect();
        let world_g: Vec<Mat4> = inv_g.iter().map(|m| m.inverse()).collect();
        (0..n)
            .map(|j| {
                let parent = self.source_parents.get(j).copied().unwrap_or(-1);
                // LOCAL (column-vector) = world_parent^-1 · world_bone; root local = world.
                let local_g = if parent >= 0 && (parent as usize) < n {
                    world_g[parent as usize].inverse() * world_g[j]
                } else {
                    world_g[j]
                };
                // The engine stores matrices as the row-vector form of the column-vector glam
                // matrix, which is exactly its `to_cols_array_2d()` (see the mesh loader's
                // round-trip).
                let name_hash = table
                    .get(j)
                    .and_then(|&t| target_hashes.get(t))
                    .copied()
                    .unwrap_or(0);
                BoneRig {
                    parent,
                    name_hash,
                    world_bind: world_g[j].to_cols_array_2d(),
                    inv_bind: self.source_ibm[j],
                    local_bind: local_g.to_cols_array_2d(),
                }
            })
            .collect()
    }
}
