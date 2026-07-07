//! Impact-output channel — the **producer side** of the hit-FX pipeline.
//!
//! Every resolved combat hit (a hitscan bullet striking a surface, a projectile's direct impact, an
//! explosion detonating) emits an [`Impact`] record. Downstream presentation silos — the **decal**
//! table (`decaltable 0x3B0AABF8`: bullet holes / explosion marks / blood splatter) and the **PgFX
//! particle** system (impact sprays / scorch puffs) — consume these to spawn their world FX. This crate
//! only *produces* the records; the decal/particle consumers are wired in the game layer (no
//! leaf→leaf edge, carve rule §4).
//!
//! The three [`ImpactKind`] variants are exactly the combat-produced entries the `decaltable`
//! definition enumerates (`docs/type_hash_registry.md` §`0x3B0AABF8`): bullet holes, explosion marks,
//! and blood splatter. Tyre-track / burnout decals exist in the same table but are produced by the
//! vehicle silo, not combat, so they are intentionally out of scope here.
//!
//! # Surface-normal derivation (honesty boundary)
//! The combat silo does not know a struck surface's material, and only the physics `RayHit` carries a
//! true geometric normal. So:
//! - **Hitscan / projectile surface & body hits** use the physics `RayHit.normal` when it is a valid
//!   unit-ish vector; if the query returns a degenerate (zero) normal, we fall back to the **negated
//!   shot/projectile travel direction** (a face pointing back at the shooter — the conventional
//!   bullet-decal orientation).
//! - **Explosions** have no single surface (a radial blast), so the normal is a fixed **world-up**
//!   (`+Y`, canonical game space) — an FX-orientation convention, not a measured surface.
//! `// CONFIRM-LIVE:` the exe's decal/particle placement reads the actual struck triangle's normal and
//! material id from the physics contact; that contact data lands with the physics silo (`DEFERRED.md`).

use glam::Vec3;

/// What kind of impact FX a resolved hit should produce — the combat-produced subset of the
/// `decaltable` (`0x3B0AABF8`) categories. A consumer maps each variant to its decal/particle template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImpactKind {
    /// A round struck world geometry or a non-character body — a **bullet hole** decal + spark/dust.
    Bullet,
    /// A blast detonated — an **explosion mark** (scorch) decal + fireball/smoke.
    Explosion,
    /// A round struck a character (a [`crate::components::Health`]-bearing entity) — a **blood
    /// splatter** decal + blood spray.
    Blood,
}

/// One resolved-hit impact event: where the FX goes, how it is oriented, and which template to use.
/// Accumulated on the combat system and drained each frame by the game layer (see
/// [`crate::WeaponSystem::take_impacts`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impact {
    /// World-space contact point where the FX is spawned.
    pub position: Vec3,
    /// Unit surface normal at the contact (FX facing). See the module docs for how it is derived.
    pub normal: Vec3,
    /// Which decal/particle family to spawn.
    pub kind: ImpactKind,
}

impl Impact {
    /// A surface/body hit (bullet hole or blood, depending on `is_character`). `surface_normal` is the
    /// physics contact normal if available; `travel_dir` is the shot/projectile unit heading, used to
    /// derive a facing when the contact normal is degenerate.
    pub fn from_hit(point: Vec3, surface_normal: Vec3, travel_dir: Vec3, is_character: bool) -> Self {
        let normal = normalize_or_back(surface_normal, travel_dir);
        Impact {
            position: point,
            normal,
            kind: if is_character { ImpactKind::Blood } else { ImpactKind::Bullet },
        }
    }

    /// An explosion detonation at `center`. A radial blast has no single surface, so the FX faces
    /// world-up (`+Y`, canonical game space) — see the module docs.
    pub fn explosion(center: Vec3) -> Self {
        Impact {
            position: center,
            normal: Vec3::Y,
            kind: ImpactKind::Explosion,
        }
    }
}

/// Return `surface_normal` normalized if it is a usable direction, else a unit facing back along the
/// travel direction (`-travel_dir`). Guarantees a finite, roughly-unit normal for the consumer.
fn normalize_or_back(surface_normal: Vec3, travel_dir: Vec3) -> Vec3 {
    if surface_normal.length_squared() > 1e-8 {
        surface_normal.normalize()
    } else {
        (-travel_dir).normalize_or_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_hit_is_blood_surface_hit_is_bullet() {
        let b = Impact::from_hit(Vec3::new(1.0, 2.0, 3.0), Vec3::Y, Vec3::Z, true);
        assert_eq!(b.kind, ImpactKind::Blood);
        assert_eq!(b.position, Vec3::new(1.0, 2.0, 3.0));
        let s = Impact::from_hit(Vec3::ZERO, Vec3::Y, Vec3::Z, false);
        assert_eq!(s.kind, ImpactKind::Bullet);
    }

    #[test]
    fn degenerate_normal_falls_back_to_reverse_travel() {
        // Physics returned a zero normal → face back along the shot direction (+Z travel ⇒ -Z facing).
        let i = Impact::from_hit(Vec3::ZERO, Vec3::ZERO, Vec3::Z, false);
        assert!((i.normal - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);
    }

    #[test]
    fn valid_normal_is_kept_and_normalized() {
        let i = Impact::from_hit(Vec3::ZERO, Vec3::new(0.0, 4.0, 0.0), Vec3::Z, false);
        assert!((i.normal - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn explosion_faces_world_up() {
        let e = Impact::explosion(Vec3::new(5.0, 0.0, 5.0));
        assert_eq!(e.kind, ImpactKind::Explosion);
        assert_eq!(e.normal, Vec3::Y);
        assert_eq!(e.position, Vec3::new(5.0, 0.0, 5.0));
    }
}
