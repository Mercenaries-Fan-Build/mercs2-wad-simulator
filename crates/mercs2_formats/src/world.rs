//! Game world spatial constants for validation.

pub const WORLD_X_MIN: f32 = -5000.0;
pub const WORLD_X_MAX: f32 = 5000.0;
pub const WORLD_Y_MIN: f32 = -500.0;
pub const WORLD_Y_MAX: f32 = 1000.0;
pub const WORLD_Z_MIN: f32 = -5000.0;
pub const WORLD_Z_MAX: f32 = 5000.0;

/// Check if a position float is valid for the spatial hash table.
/// Returns false for NaN, Inf, or out-of-world-bounds values —
/// any of which would overflow cvttss2si and corrupt the hash table.
#[inline]
pub fn is_valid_position(x: f32, y: f32, z: f32) -> bool {
    x.is_finite()
        && y.is_finite()
        && z.is_finite()
        && x >= WORLD_X_MIN
        && x <= WORLD_X_MAX
        && y >= WORLD_Y_MIN
        && y <= WORLD_Y_MAX
        && z >= WORLD_Z_MIN
        && z <= WORLD_Z_MAX
}

/// Check if a quaternion is valid (finite components, approximately unit length).
#[inline]
pub fn is_valid_quaternion(qx: f32, qy: f32, qz: f32, qw: f32) -> bool {
    if !qx.is_finite() || !qy.is_finite() || !qz.is_finite() || !qw.is_finite() {
        return false;
    }
    let mag_sq = qx * qx + qy * qy + qz * qz + qw * qw;
    (0.81..=1.21).contains(&mag_sq) // 0.9^2 .. 1.1^2
}
