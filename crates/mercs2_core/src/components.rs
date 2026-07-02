//! Core components. These are plain data — the systems (animation, render, physics, …) read and
//! write them. They deliberately mirror the original engine's model: an entity is a bag of
//! reflection-addressable components. Here we hand-type the hot-path components the sim actually
//! simulates; the long tail of the 220 native reflection classes will hang off a hash-keyed blob
//! component later, so Lua/ObjectScript can touch any of them the way the game does.

use glam::{Mat4, Quat, Vec3};

/// World transform in **canonical game space: left-handed, +Y up, +Z north, +X east**
/// (see docs/coordinate_systems.md — this is identical to the game's own basis, so the
/// asset-load transform is the identity). Stored as TRS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat, // xyzw
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn from_translation(t: Vec3) -> Self {
        Self {
            translation: t,
            ..Self::IDENTITY
        }
    }

    /// The 4x4 model matrix (scale, then rotate, then translate) in game space.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// Reference to a model asset by its WAD hash — the geometry + rig this entity renders as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelRef {
    pub model: u32,
}

/// Playback state for a bound animation clip. The animation system advances `time` each fixed
/// tick and samples the clip into a [`SkinPalette`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimState {
    pub clip: u32,
    pub time: f32,
    pub speed: f32,
    /// Crossfade source: the clip that was playing before the last switch. While `blend < 1`
    /// the animation system samples it too and blends toward `clip`.
    pub prev_clip: u32,
    /// Playback time within `prev_clip` (advances and wraps on its own duration during a fade).
    pub prev_time: f32,
    /// Crossfade progress 0..1 (weight of `clip` vs `prev_clip`); 1.0 = no fade active.
    pub blend: f32,
    pub playing: bool,
}

impl Default for AnimState {
    fn default() -> Self {
        Self {
            clip: 0,
            time: 0.0,
            speed: 1.0,
            prev_clip: 0,
            prev_time: 0.0,
            blend: 1.0,
            playing: false,
        }
    }
}

impl AnimState {
    /// A clip that starts playing from t=0 at normal speed.
    pub fn playing(clip: u32) -> Self {
        Self {
            clip,
            playing: true,
            ..Self::default()
        }
    }
}

/// The skinning palette: one bone matrix per bone (row-major, as the shader consumes it). This is
/// the hand-off between the sim spine (which fills it) and the render system (which uploads it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinPalette {
    pub mats: Vec<[[f32; 4]; 4]>,
}
