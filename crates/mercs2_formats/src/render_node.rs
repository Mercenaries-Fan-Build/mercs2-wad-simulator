//! A model render node's bounding sphere when its entity's activation creates it — what the hibernation
//! grid re-cells the entity with (`docs/reverse_engineer/hibernation_grid.md` §2).
//!
//! - A `TinyGeometryObject` entity gets node class `0xBAB160` (`FUN_00477D10`): the model's `INFO`
//!   box and the sphere the `INFO` reader `FUN_00470D60` derives from it, never recomputed.
//! - Any other model gets class `0xBAB0F0` (`FUN_00471FB0`): `FUN_004737F0` (`0x66984F`) recomputes the
//!   box from the activation pose — the union, over every enabled HIER node, of the node's two box
//!   corners through its bone matrix — and the sphere from that box (`0x473D25..0x473DCE`).

use std::collections::BTreeMap;

use crate::orchestrator::{self, HierNode};
use crate::placement::LightObject;
use crate::worldentity::{Payload, WorldEntity};

/// A sphere: model-space centre and radius.
pub type Sphere = ([f32; 3], f32);

type M4 = [[f32; 4]; 4];

/// The `INFO` sphere (`FUN_00470D60`, `0x470DE5..0x470E87`): centre = (min + max)·0.5, radius =
/// 0.5·sqrt((dz² + dy²) + dx²) evaluated in double precision and stored as f32.
pub fn info_sphere(container: &[u8]) -> Result<Sphere, String> {
    let h = crate::model_cubeize::parse_model_header(container).ok_or("no INFO header")?;
    let (lo, hi) = (h.aabb_min, h.aabb_max);
    let centre = [0, 1, 2].map(|i| ((f64::from(lo[i]) + f64::from(hi[i])) * 0.5) as f32);
    let d = [0, 1, 2].map(|i| f64::from(hi[i] - lo[i]));
    Ok((centre, (((d[2] * d[2] + d[1] * d[1]) + d[0] * d[0]).sqrt() * 0.5) as f32))
}

/// The posed node's sphere at activation for a model without `TinyGeometryObject`, given its node
/// enable table (`+0x2A8`, one flag per HIER node).
pub fn posed_sphere(container: &[u8], enable: &[bool]) -> Result<Sphere, String> {
    let hier = orchestrator::parse_hier(container);
    if enable.len() != hier.len() {
        return Err(format!("{} enable flags for {} HIER nodes", enable.len(), hier.len()));
    }
    let bone = activation_bones(&hier, &skeleton_flags(container, hier.len())?)?;
    let (mut lo, mut hi) = ([65536f32; 3], [-65536f32; 3]);
    for (i, h) in hier.iter().enumerate().filter(|(i, _)| enable[*i]) {
        for c in [h.bbox_min, h.bbox_max] {
            let p = transform_coord(c, &bone[i]);
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    Ok(box_sphere(lo, hi))
}

/// The range and cone scales a `LightAnimation` applies once when its listener completes
/// (`0x66D860` → `FUN_004A7A50` → `0x4A80DA`), before the activation's SetTransform: record `+0x00`
/// range scale, `+0x08` cone scale. Without a LightAnimation both are 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightScales {
    pub range: f32,
    pub cone: f32,
}

impl Default for LightScales {
    fn default() -> Self {
        LightScales { range: 1.0, cone: 1.0 }
    }
}

/// The light render node's sphere (LightObject listener complete `0x66D630`), centre 0:
/// - point (type 2, class `0xBACA38`, `FUN_004A7120`): radius = max(range·s, 0.01), range = `+0x18`;
/// - spot (type 3, class `0xBACA80`, `FUN_004A7660`): L = max(range·s, 0.01), cone = clamp(`+0x24`·c,
///   0, π), w = L·tan(cone/2) (double `tan`, stored f32), radius = sqrt(L² + w² + w²) (x87).
pub fn light_sphere(light: &LightObject, scales: LightScales) -> Result<Sphere, String> {
    let range = (light.params[2] * scales.range).max(0.01);
    match light.light_type {
        LightObject::TYPE_POINT => Ok(([0.0; 3], range)),
        LightObject::TYPE_SPOT => {
            let cone = (light.params[5] * scales.cone).clamp(0.0, std::f32::consts::PI);
            let t = f64::from(cone * 0.5).tan() as f32;
            let w = range * t;
            let (l, w) = (f64::from(range), f64::from(w));
            Ok(([0.0; 3], (l * l + w * w + w * w).sqrt() as f32))
        }
        t => Err(format!("LightObject type {t} has no render node")),
    }
}

/// Every worldentity template's `LightObject`, by template key.
pub fn template_lights(we: &WorldEntity) -> Result<BTreeMap<u32, LightObject>, String> {
    let mut out = BTreeMap::new();
    for c in we.groups_of("LightObject") {
        for r in &c.records {
            let Payload::Fixed(b) = &r.payload else {
                return Err(format!("LightObject: a record of keys {:08X?} is not fixed-size", r.keys));
            };
            if b.len() != LightObject::PAYLOAD_STRIDE {
                return Err(format!("LightObject: keys {:08X?}: {} payload bytes", r.keys, b.len()));
            }
            for &k in &r.keys {
                if out.insert(k, LightObject::from_payload(b)).is_some() {
                    return Err(format!("LightObject: template 0x{k:08X} has two records"));
                }
            }
        }
    }
    Ok(out)
}

/// Every worldentity template's `LightAnimation` scales, by template key. A negative scale makes retail
/// draw a random one (`FUN_004A7A50`, LCG `[0xDFCBAC]`), which is not modelled: an error.
pub fn template_light_scales(we: &WorldEntity) -> Result<BTreeMap<u32, LightScales>, String> {
    let mut out = BTreeMap::new();
    for c in we.groups_of("LightAnimation") {
        for r in &c.records {
            let Payload::Fixed(b) = &r.payload else {
                return Err(format!("LightAnimation: a record of keys {:08X?} is not fixed-size", r.keys));
            };
            if b.len() < 12 {
                return Err(format!("LightAnimation: keys {:08X?}: {} payload bytes", r.keys, b.len()));
            }
            let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
            let s = LightScales { range: f(0), cone: f(8) };
            if s.range < 0.0 || s.cone < 0.0 {
                return Err(format!("LightAnimation: keys {:08X?}: a random scale ({s:?}) is not modelled", r.keys));
            }
            for &k in &r.keys {
                if out.insert(k, s).is_some() {
                    return Err(format!("LightAnimation: template 0x{k:08X} has two records"));
                }
            }
        }
    }
    Ok(out)
}

/// The posed node's enable table at activation (`+0x2A8`, one flag per HIER node), for an entity with
/// no saved destruction state: the model's state machine run from `InitState`
/// ([`orchestrator::activation_node_enable`]). A model without a state machine keeps every node
/// enabled.
pub fn activation_enable(container: &[u8]) -> Result<Vec<bool>, String> {
    let hier = orchestrator::parse_hier(container);
    match orchestrator::parse_state_machine(container) {
        Some(sm) => orchestrator::activation_node_enable(&sm, &hier),
        None => Ok(vec![true; hier.len()]),
    }
}

/// `0x473D25..0x473DCE`: centre = (max + min)·0.5 in f32; radius = sqrt of the f32
/// ((dz·dz + dy·dy) + dx·dx), times 0.5.
pub fn box_sphere(lo: [f32; 3], hi: [f32; 3]) -> Sphere {
    let centre = [0, 1, 2].map(|i| (hi[i] + lo[i]) * 0.5);
    let d = [0, 1, 2].map(|i| hi[i] - lo[i]);
    (centre, ((d[2] * d[2] + d[1] * d[1]) + d[0] * d[0]).sqrt() * 0.5)
}

/// HIER `+4` per node: the skeleton flag, as the node constructor `FUN_004723B0` counts it
/// (`0x247F59D..0x247F5D0`). Any value other than 0 or 1 is an error.
fn skeleton_flags(container: &[u8], n: usize) -> Result<Vec<bool>, String> {
    let rows = crate::ucfx::read_ucfx_rows(container)?;
    let data = u32::from_le_bytes([container[4], container[5], container[6], container[7]]) as usize;
    let row = rows.iter().find(|r| &r.tag == b"HIER" && r.rel_off != u32::MAX).ok_or("no HIER")?;
    if row.size as usize != 176 * n {
        return Err(format!("HIER is {} bytes for {n} nodes", row.size));
    }
    let base = data + row.rel_off as usize;
    (0..n)
        .map(|i| match u16::from_le_bytes([container[base + 176 * i + 4], container[base + 176 * i + 5]]) {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(format!("HIER node {i} +4 = {v}")),
        })
        .collect()
}

/// The bone matrices `FUN_004737F0` builds at activation (`0x47383D..0x4738FB`, root 0): the skeleton
/// prefix (`nanim` nodes with the skeleton flag) takes the reference pose — rotation and translation,
/// scale dropped (`FUN_0068FBC0`) — here the HIER rest chain with rows 0..2 normalised; every later
/// node is its HIER local × its parent's bone.
fn activation_bones(hier: &[HierNode], skeleton: &[bool]) -> Result<Vec<M4>, String> {
    let n = hier.len();
    let nanim = skeleton.iter().take_while(|&&f| f).count();
    if skeleton[nanim..].iter().any(|&f| f) {
        return Err("the skeleton flags are not a prefix of the HIER".into());
    }
    let mut rest: Vec<M4> = Vec::with_capacity(n);
    for h in hier {
        let l = m4(&h.local);
        rest.push(match h.parent {
            Some(p) => matrix_multiply(&l, &rest[p]),
            None => l,
        });
    }
    let mut bone = vec![[[0f32; 4]; 4]; n];
    for i in 0..nanim {
        let w = rest[i];
        let mut m = w;
        for r in 0..3 {
            let s = (w[r][0] * w[r][0] + w[r][1] * w[r][1] + w[r][2] * w[r][2]).sqrt();
            for c in 0..3 {
                m[r][c] = w[r][c] / s;
            }
            m[r][3] = 0.0;
        }
        m[3][3] = 1.0;
        bone[i] = m;
    }
    for i in nanim..n {
        let p = hier[i].parent.ok_or_else(|| format!("HIER node {i} past the skeleton has no parent"))?;
        bone[i] = matrix_multiply(&m4(&hier[i].local), &bone[p]);
    }
    Ok(bone)
}

fn m4(l: &[f32; 16]) -> M4 {
    [[l[0], l[1], l[2], l[3]], [l[4], l[5], l[6], l[7]], [l[8], l[9], l[10], l[11]], [l[12], l[13], l[14], l[15]]]
}

/// `D3DXMatrixMultiply` with d3dx9_36's SSE association: rows 0 and 3 `(a0B0 + a1B1) + (a2B2 + a3B3)`,
/// rows 1 and 2 `((a0B0 + a1B1) + a2B2) + a3B3`.
fn matrix_multiply(a: &M4, b: &M4) -> M4 {
    let mut o = [[0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            let (p0, p1, p2, p3) = (a[i][0] * b[0][j], a[i][1] * b[1][j], a[i][2] * b[2][j], a[i][3] * b[3][j]);
            o[i][j] = if i == 0 || i == 3 { (p0 + p1) + (p2 + p3) } else { ((p0 + p1) + p2) + p3 };
        }
    }
    o
}

/// `D3DXVec3TransformCoord` for an affine matrix (w = 1): `(y·M1 + x·M0) + (z·M2 + M3)`. Retail then
/// scales by a Newton step on `RCPPS(1.0)`, whose value is CPU-specific; the exact reciprocal (1) is
/// used here — it moves box bits by at most an ulp and no captured grid row.
fn transform_coord(v: [f32; 3], m: &M4) -> [f32; 3] {
    let r = |j: usize| (v[1] * m[1][j] + v[0] * m[0][j]) + (v[2] * m[2][j] + m[3][j]);
    [r(0), r(1), r(2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_sphere_is_the_half_diagonal() {
        let (c, r) = box_sphere([-1.0, 0.0, -2.0], [1.0, 2.0, 2.0]);
        assert_eq!(c, [0.0, 1.0, 0.0]);
        assert_eq!(r, (4.0f32 * 4.0 + 2.0 * 2.0 + 2.0 * 2.0).sqrt() * 0.5);
    }

    /// Point: the range (`+0x18`, params[2]) times the range scale, at least 0.01. Spot: the cone's
    /// bounding radius from the range and tan(cone/2). Another type has no node.
    #[test]
    fn light_spheres() {
        let mut l = LightObject { light_type: LightObject::TYPE_POINT, color: [1.0; 3], params: [0.0; 9] };
        l.params[2] = 7.5;
        assert_eq!(light_sphere(&l, LightScales::default()).unwrap(), ([0.0; 3], 7.5));
        assert_eq!(light_sphere(&l, LightScales { range: 0.9, cone: 1.0 }).unwrap().1, 7.5 * 0.9);
        l.params[2] = 0.0;
        assert_eq!(light_sphere(&l, LightScales::default()).unwrap().1, 0.01);
        l.light_type = LightObject::TYPE_SPOT;
        l.params[2] = 10.0;
        l.params[5] = std::f32::consts::FRAC_PI_2;
        let w = 10.0 * (std::f64::consts::FRAC_PI_4).tan();
        let r = light_sphere(&l, LightScales::default()).unwrap().1;
        assert!((f64::from(r) - (100.0 + 2.0 * w * w).sqrt()).abs() < 1e-4, "{r}");
        l.light_type = 7;
        assert!(light_sphere(&l, LightScales::default()).is_err());
    }

    #[test]
    fn matrix_multiply_by_identity_is_the_matrix() {
        let id = m4(&[1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]);
        let a = m4(&[0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1., 0., 3., 4., 5., 1.]);
        assert_eq!(matrix_multiply(&a, &id), a);
        assert_eq!(transform_coord([1.0, 0.0, 0.0], &a), [3.0, 5.0, 5.0]);
    }
}
