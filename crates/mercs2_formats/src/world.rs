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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_center() {
        assert!(is_valid_position(0.0, 0.0, 0.0));
    }

    #[test]
    fn position_boundaries_min() {
        assert!(is_valid_position(WORLD_X_MIN, WORLD_Y_MIN, WORLD_Z_MIN));
        assert!(is_valid_position(-5000.0, -500.0, -5000.0));
    }

    #[test]
    fn position_boundaries_max() {
        assert!(is_valid_position(WORLD_X_MAX, WORLD_Y_MAX, WORLD_Z_MAX));
        assert!(is_valid_position(5000.0, 1000.0, 5000.0));
    }

    #[test]
    fn position_out_of_bounds_x() {
        assert!(!is_valid_position(-5001.0, 0.0, 0.0));
        assert!(!is_valid_position(5001.0, 0.0, 0.0));
    }

    #[test]
    fn position_out_of_bounds_y() {
        assert!(!is_valid_position(0.0, -501.0, 0.0));
        assert!(!is_valid_position(0.0, 1001.0, 0.0));
    }

    #[test]
    fn position_out_of_bounds_z() {
        assert!(!is_valid_position(0.0, 0.0, -5001.0));
        assert!(!is_valid_position(0.0, 0.0, 5001.0));
    }

    #[test]
    fn position_nan() {
        assert!(!is_valid_position(f32::NAN, 0.0, 0.0));
        assert!(!is_valid_position(0.0, f32::NAN, 0.0));
        assert!(!is_valid_position(0.0, 0.0, f32::NAN));
    }

    #[test]
    fn position_infinity() {
        assert!(!is_valid_position(f32::INFINITY, 0.0, 0.0));
        assert!(!is_valid_position(f32::NEG_INFINITY, 0.0, 0.0));
        assert!(!is_valid_position(0.0, f32::INFINITY, 0.0));
    }

    #[test]
    fn quaternion_identity() {
        assert!(is_valid_quaternion(0.0, 0.0, 0.0, 1.0));
    }

    #[test]
    fn quaternion_valid_unit() {
        // (0, 0, sin(π/4), cos(π/4)) ≈ (0, 0, 0.707, 0.707)
        let s = std::f32::consts::FRAC_1_SQRT_2;
        assert!(is_valid_quaternion(0.0, 0.0, s, s));
    }

    #[test]
    fn quaternion_near_unit() {
        // magnitude ≈ 1.05 (within tolerance)
        let val = 0.5250;
        assert!(is_valid_quaternion(val, val, val, val));
    }

    #[test]
    fn quaternion_outside_tolerance() {
        // magnitude > 1.1 (outside tolerance)
        assert!(!is_valid_quaternion(0.6, 0.6, 0.6, 0.6));
    }

    #[test]
    fn quaternion_nan() {
        assert!(!is_valid_quaternion(f32::NAN, 0.0, 0.0, 1.0));
        assert!(!is_valid_quaternion(0.0, f32::NAN, 0.0, 1.0));
        assert!(!is_valid_quaternion(0.0, 0.0, f32::NAN, 1.0));
        assert!(!is_valid_quaternion(0.0, 0.0, 0.0, f32::NAN));
    }

    #[test]
    fn quaternion_infinity() {
        assert!(!is_valid_quaternion(f32::INFINITY, 0.0, 0.0, 1.0));
        assert!(!is_valid_quaternion(0.0, 0.0, 0.0, f32::NEG_INFINITY));
    }

    #[test]
    fn quaternion_zero() {
        // All zeros: magnitude = 0 (outside tolerance)
        assert!(!is_valid_quaternion(0.0, 0.0, 0.0, 0.0));
    }
}
