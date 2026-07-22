//! Small dense linear algebra — a faithful f64 port of `mercs2-mesher/src/mat.js`.
//!
//! Deliberately dependency-free (no glam): the mesher's outputs are held to byte-exact
//! parity with the Python that produced two in-game-confirmed characters, so this keeps
//! the exact f64 numerics and the exact Rodrigues/`lstsq` behaviour rather than routing
//! through glam's f32 quaternion path. Matrices are plain arrays; 4×4s are ROW-MAJOR
//! (glTF ships column-major — convert on read, once). Vectors are `[x, y, z]`.

pub type V3 = [f64; 3];

#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
pub fn len(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
#[inline]
pub fn norm(a: V3) -> V3 {
    let l = len(a);
    if l < 1e-12 {
        [0.0, 0.0, 0.0]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

pub const IDENT4: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
pub const EYE3: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Row-major 4×4 multiply.
pub fn mul4(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut o = [0.0f64; 16];
    for r in 0..4 {
        for c in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[r * 4 + k] * b[k * 4 + c];
            }
            o[r * 4 + c] = s;
        }
    }
    o
}

/// Translation column of a row-major 4×4.
#[inline]
pub fn origin_of(m: &[f64; 16]) -> V3 {
    [m[3], m[7], m[11]]
}

/// General 4×4 inverse, Gauss–Jordan with partial pivoting. Returns None if singular.
/// General (not the fast affine form) because inverse-bind matrices legitimately carry
/// non-uniform scale and shear.
pub fn inv4(m: &[f64; 16]) -> Option<[f64; 16]> {
    // augmented [m | I], 4×8
    let mut a = [[0.0f64; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            a[r][c] = m[r * 4 + c];
        }
        a[r][4 + r] = 1.0;
    }
    for c in 0..4 {
        let mut piv = c;
        for r in (c + 1)..4 {
            if a[r][c].abs() > a[piv][c].abs() {
                piv = r;
            }
        }
        if a[piv][c].abs() < 1e-14 {
            return None;
        }
        a.swap(c, piv);
        let d = a[c][c];
        for k in 0..8 {
            a[c][k] /= d;
        }
        for r in 0..4 {
            if r == c {
                continue;
            }
            let f = a[r][c];
            if f == 0.0 {
                continue;
            }
            for k in 0..8 {
                a[r][k] -= f * a[c][k];
            }
        }
    }
    let mut o = [0.0f64; 16];
    for r in 0..4 {
        for k in 0..4 {
            o[r * 4 + k] = a[r][k + 4];
        }
    }
    Some(o)
}

/// A fitted affine: 4 rows of 3 (column-per-output), i.e. `T[row][out]`, matching the
/// `lstsq` return shape used by `apply_fit`. Row 3 is the translation.
pub type Fit = [[f64; 3]; 4];

/// Apply a fitted 4×3 affine (from `lstsq` on `[x,y,z,1]`) to a point.
#[inline]
pub fn apply_fit(t: &Fit, p: V3) -> V3 {
    [
        p[0] * t[0][0] + p[1] * t[1][0] + p[2] * t[2][0] + t[3][0],
        p[0] * t[0][1] + p[1] * t[1][1] + p[2] * t[2][1] + t[3][1],
        p[0] * t[0][2] + p[1] * t[1][2] + p[2] * t[2][2] + t[3][2],
    ]
}

pub struct LstsqResult {
    /// `m × k` solution (row-per-input-column).
    pub x: Vec<Vec<f64>>,
    pub resid_mean: f64,
    pub resid_max: f64,
}

/// Least-squares solve of `A x = B` via normal equations `(AᵀA)x = AᵀB`.
/// `a` is n×m rows, `b` is n×k rows. Faithful port of `mat.js::lstsq`.
pub fn lstsq(a: &[Vec<f64>], b: &[Vec<f64>]) -> Result<LstsqResult, String> {
    let n = a.len();
    let m = a[0].len();
    let k = b[0].len();
    let mut ata = vec![vec![0.0f64; m]; m];
    let mut atb = vec![vec![0.0f64; k]; m];
    for i in 0..n {
        let ai = &a[i];
        let bi = &b[i];
        for r in 0..m {
            let v = ai[r];
            if v == 0.0 {
                continue;
            }
            for c in 0..m {
                ata[r][c] += v * ai[c];
            }
            for c in 0..k {
                atb[r][c] += v * bi[c];
            }
        }
    }
    // Gauss–Jordan on [AtA | AtB]
    let mut aug = vec![vec![0.0f64; m + k]; m];
    for r in 0..m {
        aug[r][..m].copy_from_slice(&ata[r]);
        aug[r][m..].copy_from_slice(&atb[r]);
    }
    for c in 0..m {
        let mut piv = c;
        for r in (c + 1)..m {
            if aug[r][c].abs() > aug[piv][c].abs() {
                piv = r;
            }
        }
        if aug[piv][c].abs() < 1e-12 {
            return Err("lstsq: singular normal matrix (degenerate point set?)".into());
        }
        aug.swap(c, piv);
        let d = aug[c][c];
        for j in 0..(m + k) {
            aug[c][j] /= d;
        }
        for r in 0..m {
            if r == c {
                continue;
            }
            let f = aug[r][c];
            if f == 0.0 {
                continue;
            }
            for j in 0..(m + k) {
                aug[r][j] -= f * aug[c][j];
            }
        }
    }
    let x: Vec<Vec<f64>> = (0..m).map(|r| aug[r][m..].to_vec()).collect();
    let mut sum = 0.0;
    let mut max = 0.0f64;
    for i in 0..n {
        let mut d2 = 0.0;
        for c in 0..k {
            let mut p = 0.0;
            for r in 0..m {
                p += a[i][r] * x[r][c];
            }
            let e = p - b[i][c];
            d2 += e * e;
        }
        let d = d2.sqrt();
        sum += d;
        if d > max {
            max = d;
        }
    }
    Ok(LstsqResult {
        x,
        resid_mean: sum / n as f64,
        resid_max: max,
    })
}

/// Row-major 3×3 multiply.
pub fn mul3(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut o = [0.0f64; 9];
    for r in 0..3 {
        for c in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += a[r * 3 + k] * b[k * 3 + c];
            }
            o[r * 3 + c] = s;
        }
    }
    o
}

/// Apply a row-major 3×3 to a vector.
#[inline]
pub fn apply3(m: &[f64; 9], p: V3) -> V3 {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2],
        m[3] * p[0] + m[4] * p[1] + m[5] * p[2],
        m[6] * p[0] + m[7] * p[1] + m[8] * p[2],
    ]
}

/// Shortest-arc rotation (3×3 row-major) taking unit vector `a` onto unit `b` (Rodrigues).
/// Faithful port of `mat.js::alignRot` — kept literal (not glam `from_rotation_arc`) for parity.
pub fn align_rot(a: V3, b: V3) -> [f64; 9] {
    let v = cross(a, b);
    let c = dot(a, b);
    if c > 0.999999 {
        return EYE3;
    }
    if c < -0.999999 {
        let axis = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let w = norm(cross(a, axis));
        let kk = mul3(
            &[0.0, -w[2], w[1], w[2], 0.0, -w[0], -w[1], w[0], 0.0],
            &[0.0, -w[2], w[1], w[2], 0.0, -w[0], -w[1], w[0], 0.0],
        );
        return [
            1.0 + 2.0 * kk[0],
            2.0 * kk[1],
            2.0 * kk[2],
            2.0 * kk[3],
            1.0 + 2.0 * kk[4],
            2.0 * kk[5],
            2.0 * kk[6],
            2.0 * kk[7],
            1.0 + 2.0 * kk[8],
        ];
    }
    let kmat = [0.0, -v[2], v[1], v[2], 0.0, -v[0], -v[1], v[0], 0.0];
    let kk = mul3(&kmat, &kmat);
    let f = 1.0 / (1.0 + c);
    let mut o = [0.0f64; 9];
    for i in 0..9 {
        let ident = if i % 4 == 0 { 1.0 } else { 0.0 };
        o[i] = ident + kmat[i] + kk[i] * f;
    }
    o
}

/// Transpose of a row-major 3×3.
#[inline]
pub fn transpose3(m: &[f64; 9]) -> [f64; 9] {
    [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]]
}

/// Determinant of a row-major 3×3.
#[inline]
pub fn det3(m: &[f64; 9]) -> f64 {
    m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6])
}

/// Eigen-decomposition of a SYMMETRIC row-major 3×3 by cyclic Jacobi rotations.
/// Returns `(eigenvalues, V)` where `V` is row-major with eigenvectors in its COLUMNS
/// (`A = V diag(w) Vᵀ`). Always converges for a symmetric input.
pub fn sym_eigen3(a_in: &[f64; 9]) -> ([f64; 3], [f64; 9]) {
    let mut a = *a_in;
    let mut v = EYE3;
    for _ in 0..24 {
        // largest off-diagonal magnitude
        let (mut p, mut q, mut best) = (0usize, 1usize, 0.0f64);
        for (r, c) in [(0usize, 1usize), (0, 2), (1, 2)] {
            let m = a[r * 3 + c].abs();
            if m > best {
                best = m;
                p = r;
                q = c;
            }
        }
        if best < 1e-18 {
            break;
        }
        let (app, aqq, apq) = (a[p * 3 + p], a[q * 3 + q], a[p * 3 + q]);
        let theta = 0.5 * (aqq - app) / apq;
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        // A <- Jᵀ A J, V <- V J
        let mut na = a;
        for k in 0..3 {
            let akp = a[k * 3 + p];
            let akq = a[k * 3 + q];
            na[k * 3 + p] = c * akp - s * akq;
            na[k * 3 + q] = s * akp + c * akq;
        }
        let mut ma = na;
        for k in 0..3 {
            let apk = na[p * 3 + k];
            let aqk = na[q * 3 + k];
            ma[p * 3 + k] = c * apk - s * aqk;
            ma[q * 3 + k] = s * apk + c * aqk;
        }
        a = ma;
        a[p * 3 + q] = 0.0;
        a[q * 3 + p] = 0.0;
        let mut nv = v;
        for k in 0..3 {
            let vkp = v[k * 3 + p];
            let vkq = v[k * 3 + q];
            nv[k * 3 + p] = c * vkp - s * vkq;
            nv[k * 3 + q] = s * vkp + c * vkq;
        }
        v = nv;
    }
    ([a[0], a[4], a[8]], v)
}

/// The PROPER rotation (det = +1) that best aligns `h` in the Kabsch sense — the orthogonal
/// polar factor of `h`, i.e. `argmax_R tr(Rᵀ h)` over `SO(3)`. Robust to a rank-deficient `h`
/// (2-point or 1-point correspondence sets): the null directions are filled by Gram–Schmidt,
/// which is arbitrary-but-deterministic there, so callers must constrain those cases
/// themselves. Returns `(R, singular_values_desc)`.
pub fn kabsch_rot(h: &[f64; 9]) -> ([f64; 9], [f64; 3]) {
    // SVD of h via the eigen-decomposition of hᵀh: h = U S Vᵀ, hᵀh = V S² Vᵀ.
    let ht = transpose3(h);
    let hth = mul3(&ht, h);
    let (w, vmat) = sym_eigen3(&hth);
    let mut order = [0usize, 1, 2];
    order.sort_by(|&i, &j| w[j].partial_cmp(&w[i]).unwrap_or(std::cmp::Ordering::Equal));
    let col = |m: &[f64; 9], c: usize| -> V3 { [m[c], m[3 + c], m[6 + c]] };
    let mut vc = [[0.0f64; 3]; 3];
    let mut uc = [[0.0f64; 3]; 3];
    let mut sig = [0.0f64; 3];
    let scale_ref = w[order[0]].max(0.0).sqrt().max(1e-300);
    for (k, &o) in order.iter().enumerate() {
        vc[k] = col(&vmat, o);
        sig[k] = w[o].max(0.0).sqrt();
        let hv = apply3(h, vc[k]);
        uc[k] = if sig[k] > 1e-9 * scale_ref {
            [hv[0] / sig[k], hv[1] / sig[k], hv[2] / sig[k]]
        } else {
            [0.0, 0.0, 0.0]
        };
    }
    // Gram–Schmidt fill for null directions (keeps U orthonormal and right-handed).
    for k in 0..3 {
        if len(uc[k]) > 0.5 {
            continue;
        }
        uc[k] = match k {
            2 => cross(uc[0], uc[1]),
            1 => {
                let seed = if uc[0][0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
                norm(cross(uc[0], seed))
            }
            _ => [1.0, 0.0, 0.0],
        };
        if len(uc[k]) < 0.5 {
            uc[k] = [0.0, 0.0, 1.0];
        }
        uc[k] = norm(uc[k]);
    }
    // R = U diag(1,1,d) Vᵀ  (row-major: R[r][c] = Σ_k u_k[r] * dk * v_k[c])
    let build = |d: f64| -> [f64; 9] {
        let dk = [1.0, 1.0, d];
        let mut r = [0.0f64; 9];
        for row in 0..3 {
            for c in 0..3 {
                r[row * 3 + c] = (0..3).map(|k| uc[k][row] * dk[k] * vc[k][c]).sum();
            }
        }
        r
    };
    let mut r = build(1.0);
    if det3(&r) < 0.0 {
        r = build(-1.0);
        sig[2] = -sig[2];
    }
    (r, sig)
}

/// A fitted similarity: uniform scale · rotation + translation.
#[derive(Debug, Clone, Copy)]
pub struct Sim {
    /// scale · rotation, row-major 3×3.
    pub sr: [f64; 9],
    pub t: V3,
    pub scale: f64,
    /// rank of the correspondence set (3 = fully determined, 2 = one free twist, …).
    pub rank: usize,
}

impl Sim {
    pub const IDENTITY: Sim = Sim { sr: EYE3, t: [0.0; 3], scale: 1.0, rank: 3 };
    #[inline]
    pub fn apply(&self, p: V3) -> V3 {
        let q = apply3(&self.sr, p);
        [q[0] + self.t[0], q[1] + self.t[1], q[2] + self.t[2]]
    }
}

/// Weighted **Umeyama** similarity fit `src → dst` — the least-squares-optimal uniform
/// scale, rotation and translation. Unlike polar-decomposing the fitted general affine, this
/// is the true minimiser: the polar factor of an ANISOTROPIC affine is not the optimal
/// rotation, and the mean of its singular values is not the optimal scale.
///
/// `pairs` is `(src, dst, weight)`. Returns `None` for an empty/zero-weight set.
pub fn fit_similarity_weighted(pairs: &[(V3, V3, f64)]) -> Option<Sim> {
    let wsum: f64 = pairs.iter().map(|p| p.2).sum();
    if pairs.is_empty() || wsum <= 0.0 {
        return None;
    }
    let mut ms = [0.0f64; 3];
    let mut md = [0.0f64; 3];
    for &(s, d, w) in pairs {
        for c in 0..3 {
            ms[c] += w * s[c] / wsum;
            md[c] += w * d[c] / wsum;
        }
    }
    // h[r][c] = Σ w (d-md)[r] (s-ms)[c]  →  argmax_R tr(Rᵀ h) is the Kabsch rotation.
    let mut h = [0.0f64; 9];
    let mut var = 0.0f64;
    for &(s, d, w) in pairs {
        let sc = sub(s, ms);
        let dc = sub(d, md);
        for r in 0..3 {
            for c in 0..3 {
                h[r * 3 + c] += w * dc[r] * sc[c];
            }
        }
        var += w * dot(sc, sc);
    }
    let (r, sig) = kabsch_rot(&h);
    let rank = sig.iter().filter(|&&s| s > 1e-9 * sig[0].abs().max(1e-300)).count();
    // optimal scale: tr(Rᵀ h) / Σ w |s-ms|²
    let trace: f64 = (0..3).map(|i| sig[i]).sum();
    let scale = if var > 1e-30 { trace / var } else { 1.0 };
    let scale = if scale.is_finite() && scale > 1e-12 { scale } else { 1.0 };
    let sr: [f64; 9] = std::array::from_fn(|i| r[i] * scale);
    let t = sub(md, apply3(&sr, ms));
    Some(Sim { sr, t, scale, rank })
}

/// Rotation angle of a row-major 3×3 in degrees.
#[inline]
pub fn rot_angle_deg(m: &[f64; 9]) -> f64 {
    (((m[0] + m[4] + m[8] - 1.0) / 2.0).clamp(-1.0, 1.0)).acos() * 180.0 / std::f64::consts::PI
}

/// `numpy.allclose(a, b)` with numpy's default `rtol=1e-5` and a caller-set `atol`.
pub fn allclose(a: &[f64], b: &[f64], atol: f64) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(&v, &w)| (v - w).abs() <= atol + 1e-5 * w.abs())
}

/// Inverse of a row-major 3×3 by adjugate/determinant. `None` when singular.
///
/// Needed for normals: the source→container fit is a general affine, and a normal transforms by the
/// inverse-transpose of the point map, not by the map itself. Skipping that is only harmless when
/// the map is a similarity — and this one is measurably not (3.0× anisotropy on 50 Cent → mattias).
pub fn inv3(m: &[f64; 9]) -> Option<[f64; 9]> {
    let (a, b, c) = (m[0], m[1], m[2]);
    let (d, e, f) = (m[3], m[4], m[5]);
    let (g, h, i) = (m[6], m[7], m[8]);
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-18 {
        return None;
    }
    let r = 1.0 / det;
    Some([
        (e * i - f * h) * r,
        (c * h - b * i) * r,
        (b * f - c * e) * r,
        (f * g - d * i) * r,
        (a * i - c * g) * r,
        (c * d - a * f) * r,
        (d * h - e * g) * r,
        (b * g - a * h) * r,
        (a * e - b * d) * r,
    ])
}
