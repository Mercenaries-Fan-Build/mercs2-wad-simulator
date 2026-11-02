//! The Live page's camera: local to the workshop, never sent to the game.
//!
//! All of it is math over the pose samples, so every mode is testable with no window. Positions are the
//! game's left-handed Y-up metres; `Object.GetYaw` turns about +Y from +Z (`Quat::from_rotation_y`, as
//! `StreamingWorld::spawn` builds a spawn's facing).

use glam::{Mat4, Vec2, Vec3, Vec4};

/// The renderer's vertical field of view, as `Scene::render_with` builds it.
const FOV_Y: f32 = 45.0;
pub const NEAR: f32 = 0.3;
pub const FAR: f32 = 8000.0;
/// The mode's default distance at 100% zoom.
const ORBIT_DIST: f32 = 13.0;
const CHASE_DIST: f32 = 9.0;
const CHASE_HEIGHT: f32 = 3.0;
const TOP_HEIGHT: f32 = 60.0;
/// A chase drag's offset decays by this much a second.
const SETTLE: f32 = 1.5;
/// Where a character's camera looks, above the feet the game reports.
const FOCUS_UP: f32 = 1.6;

/// The camera modes of the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    FreeOrbit,
    ChaseHero,
    TopDown,
    MirrorInGame,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::FreeOrbit, Mode::ChaseHero, Mode::TopDown, Mode::MirrorInGame];

    pub fn label(self) -> &'static str {
        match self {
            Mode::FreeOrbit => "Free orbit",
            Mode::ChaseHero => "Chase hero",
            Mode::TopDown => "Top-down",
            Mode::MirrorInGame => "Mirror in-game",
        }
    }
}

/// What the camera is following this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Follow {
    pub pos: Vec3,
    /// Degrees.
    pub yaw: f32,
}

/// The game camera, as the bridge reads it: its eye comes from the native read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameCamera {
    pub eye: Vec3,
    /// Degrees, as `Camera.GetYaw`.
    pub yaw: f32,
    /// Degrees, as `Camera.GetPitch`.
    pub pitch: f32,
}

/// Which way yaw `deg` faces, about +Y from +Z.
pub fn facing(deg: f32) -> Vec3 {
    let r = deg.to_radians();
    Vec3::new(r.sin(), 0.0, r.cos())
}

/// One frame's view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub eye: Vec3,
    pub target: Vec3,
}

impl View {
    pub fn matrix(&self) -> Mat4 {
        let up = if (self.target - self.eye).normalize_or_zero().y.abs() > 0.999 { Vec3::Z } else { Vec3::Y };
        Mat4::look_at_lh(self.eye, self.target, up)
    }

    /// The renderer's view-projection, with the image centre moved `shift` in clip units.
    pub fn view_proj(&self, aspect: f32, shift: Vec2) -> Mat4 {
        Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) * off_axis(aspect, NEAR, FAR, shift) * self.matrix()
    }
}

/// `Mat4::perspective_lh` with its centre moved `shift` in clip units, so what is centred lands at the
/// middle of the visible area between the side panels, not of the whole window.
pub fn off_axis(aspect: f32, near: f32, far: f32, shift: Vec2) -> Mat4 {
    let mut p = Mat4::perspective_lh(FOV_Y.to_radians(), aspect, near, far);
    p.z_axis.x = shift.x;
    p.z_axis.y = shift.y;
    p
}

/// The clip-space move that puts the window's centre at the centre of `visible`. The renderer mirrors
/// clip X after this matrix, so X is negated here.
pub fn centre_shift(screen: egui::Vec2, visible: egui::Rect) -> Vec2 {
    let c = visible.center();
    Vec2::new(1.0 - 2.0 * c.x / screen.x, 1.0 - 2.0 * c.y / screen.y)
}

/// `p` on a screen `size` points big, with its clip depth; `None` behind the camera.
pub fn project(vp: Mat4, p: Vec3, size: egui::Vec2) -> Option<(egui::Pos2, f32)> {
    let c: Vec4 = vp * p.extend(1.0);
    (c.w > NEAR).then(|| {
        let (x, y) = (c.x / c.w, c.y / c.w);
        (egui::pos2((x + 1.0) * 0.5 * size.x, (1.0 - y) * 0.5 * size.y), c.w)
    })
}

/// The camera's state.
#[derive(Debug, Clone)]
pub struct Camera {
    pub mode: Mode,
    /// Orbit angles, radians.
    pub yaw: f32,
    pub pitch: f32,
    /// 1.0 = the mode's default distance.
    pub zoom: f32,
    /// A free orbit's fixed centre, once a pan has let go of the lock.
    pub pan_target: Vec3,
    /// A chase's drag, decaying back to behind the hero.
    chase_offset: Vec2,
}

impl Default for Camera {
    fn default() -> Camera {
        Camera { mode: Mode::FreeOrbit, yaw: 0.6, pitch: -0.35, zoom: 1.0, pan_target: Vec3::ZERO, chase_offset: Vec2::ZERO }
    }
}

impl Camera {
    pub fn drag(&mut self, dx: f32, dy: f32) {
        match self.mode {
            Mode::FreeOrbit => {
                self.yaw += dx * 0.008;
                self.pitch = (self.pitch - dy * 0.008).clamp(-1.5, 1.5);
            }
            Mode::ChaseHero => self.chase_offset += Vec2::new(dx, dy) * 0.008,
            Mode::TopDown | Mode::MirrorInGame => {}
        }
    }

    /// A shift-drag moves the centre, from where it is now.
    pub fn pan(&mut self, from: Vec3, dx: f32, dy: f32) {
        let fwd = orbit_dir(self.yaw, self.pitch);
        let right = Vec3::Y.cross(fwd).normalize();
        let up = fwd.cross(right).normalize();
        let k = self.distance() * 0.0016;
        self.pan_target = from + right * (dx * k) - up * (dy * k);
    }

    pub fn wheel(&mut self, lines: f32) {
        self.zoom = (self.zoom * 0.9f32.powf(lines)).clamp(0.05, 40.0);
    }

    /// The `−` / `+` buttons.
    pub fn step_zoom(&mut self, closer: bool) {
        self.zoom = if closer { (self.zoom / 1.25).max(0.05) } else { (self.zoom * 1.25).min(40.0) };
    }

    pub fn distance(&self) -> f32 {
        ORBIT_DIST * self.zoom
    }

    /// Yaw in degrees for the readout, 0..360.
    pub fn readout_yaw(&self) -> f32 {
        self.yaw.to_degrees().rem_euclid(360.0)
    }

    /// This frame's view. `lock` is what a free orbit is centred on, when anything; `hero` what the
    /// other modes follow. A mode with nothing to follow holds `pan_target`.
    pub fn view(&mut self, dt: f32, lock: Option<Follow>, hero: Option<Follow>, game: Option<GameCamera>) -> View {
        self.chase_offset *= (1.0 - SETTLE * dt).clamp(0.0, 1.0);
        let focus = |f: Follow| f.pos + Vec3::Y * FOCUS_UP;
        let centre = |f: Option<Follow>, pan: Vec3| f.map_or(pan, focus);
        match self.mode {
            Mode::FreeOrbit => {
                let target = centre(lock, self.pan_target);
                View { eye: target - orbit_dir(self.yaw, self.pitch) * self.distance(), target }
            }
            Mode::ChaseHero => {
                let Some(h) = hero else {
                    return View { eye: self.pan_target - Vec3::Z * ORBIT_DIST, target: self.pan_target };
                };
                let back = -facing(h.yaw + self.chase_offset.x.to_degrees());
                let target = focus(h);
                let up = CHASE_HEIGHT * self.zoom + self.chase_offset.y * CHASE_DIST;
                View { eye: target + back * (CHASE_DIST * self.zoom) + Vec3::Y * up, target }
            }
            Mode::TopDown => {
                let target = centre(hero, self.pan_target);
                View { eye: target + Vec3::Y * (TOP_HEIGHT * self.zoom), target }
            }
            Mode::MirrorInGame => match game {
                Some(g) => {
                    let (y, p) = (g.yaw.to_radians(), g.pitch.to_radians());
                    let dir = Vec3::new(y.sin() * p.cos(), p.sin(), y.cos() * p.cos());
                    View { eye: g.eye, target: g.eye + dir }
                }
                None => {
                    let target = centre(hero, self.pan_target);
                    View { eye: target - orbit_dir(self.yaw, self.pitch) * self.distance(), target }
                }
            },
        }
    }
}

/// The orbit's forward vector, the convention of the workshop's other pages.
fn orbit_dir(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos())
}

/// A sample at `t`, 0..=1 between `a` and `b`: the shortest way round for an angle.
pub fn lerp_follow(a: Follow, b: Follow, t: f32) -> Follow {
    let t = t.clamp(0.0, 1.0);
    let d = (b.yaw - a.yaw + 180.0).rem_euclid(360.0) - 180.0;
    Follow { pos: a.pos.lerp(b.pos, t), yaw: a.yaw + d * t }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERO: Follow = Follow { pos: Vec3::new(2635.0, -14.0, -999.0), yaw: 90.0 };

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-2
    }

    #[test]
    fn yaw_faces_about_y_from_z() {
        assert!(near(facing(0.0), Vec3::Z));
        assert!(near(facing(90.0), Vec3::X));
    }

    #[test]
    fn a_free_orbit_locked_on_the_hero_follows_it() {
        let mut c = Camera::default();
        let a = c.view(0.0, Some(HERO), Some(HERO), None);
        let moved = Follow { pos: HERO.pos + Vec3::new(10.0, 0.0, 0.0), ..HERO };
        let b = c.view(0.0, Some(moved), Some(moved), None);
        assert!(near(b.target - a.target, Vec3::new(10.0, 0.0, 0.0)));
        assert!(near(b.eye - a.eye, Vec3::new(10.0, 0.0, 0.0)), "the angle holds, the camera travels");
    }

    #[test]
    fn a_free_orbit_with_no_lock_stays_put() {
        let mut c = Camera { pan_target: Vec3::new(1.0, 2.0, 3.0), ..Camera::default() };
        assert_eq!(c.view(0.0, None, Some(HERO), None).target, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn a_chase_sits_behind_the_hero_and_turns_with_it() {
        let mut c = Camera { mode: Mode::ChaseHero, ..Camera::default() };
        let v = c.view(0.0, None, Some(HERO), None);
        assert!(v.eye.x < v.target.x, "the hero faces +X, so the camera is on -X");
        assert!(v.eye.y > v.target.y);
        let turned = c.view(0.0, None, Some(Follow { yaw: -90.0, ..HERO }), None);
        assert!(turned.eye.x > turned.target.x);
    }

    #[test]
    fn a_chase_drag_settles_back() {
        let mut c = Camera { mode: Mode::ChaseHero, ..Camera::default() };
        let rest = c.view(0.0, None, Some(HERO), None);
        c.drag(200.0, 0.0);
        let swung = c.view(0.0, None, Some(HERO), None);
        assert!(!near(swung.eye, rest.eye));
        for _ in 0..600 {
            c.view(1.0 / 60.0, None, Some(HERO), None);
        }
        assert!((c.view(0.0, None, Some(HERO), None).eye - rest.eye).length() < 0.01);
    }

    #[test]
    fn top_down_looks_straight_down_with_a_usable_matrix() {
        let mut c = Camera { mode: Mode::TopDown, ..Camera::default() };
        let v = c.view(0.0, None, Some(HERO), None);
        assert!(near(Vec3::new(v.eye.x, 0.0, v.eye.z), Vec3::new(v.target.x, 0.0, v.target.z)));
        assert!(v.matrix().is_finite(), "straight down must not collapse the up vector");
        c.wheel(-3.0);
        assert!(c.view(0.0, None, Some(HERO), None).eye.y > v.eye.y, "the wheel changes the height");
    }

    #[test]
    fn mirror_uses_the_game_camera_when_it_has_one() {
        let mut c = Camera { mode: Mode::MirrorInGame, ..Camera::default() };
        let eye = Vec3::new(1.0, 2.0, 3.0);
        let v = c.view(0.0, None, Some(HERO), Some(GameCamera { eye, yaw: 90.0, pitch: 0.0 }));
        assert!(near(v.eye, eye));
        assert!(near(v.target - eye, Vec3::X));
    }

    #[test]
    fn zoom_scales_the_distance_and_the_buttons_undo_each_other() {
        let mut c = Camera::default();
        let d = c.distance();
        c.step_zoom(false);
        assert!(c.distance() > d);
        c.step_zoom(true);
        assert!((c.distance() - d).abs() < 1e-3);
    }

    #[test]
    fn the_shortest_way_round() {
        let a = Follow { pos: Vec3::ZERO, yaw: 170.0 };
        let b = Follow { pos: Vec3::X, yaw: -170.0 };
        let m = lerp_follow(a, b, 0.5);
        assert!((m.yaw.rem_euclid(360.0) - 180.0).abs() < 1e-3, "{}", m.yaw);
        assert!(near(m.pos, Vec3::new(0.5, 0.0, 0.0)));
    }

    /// ★ The hero lands at the centre of the visible rect, between the side panels, not of the window.
    #[test]
    fn the_target_lands_at_the_centre_of_the_visible_rect() {
        let screen = egui::vec2(1440.0, 842.0);
        let visible = egui::Rect::from_min_max(egui::pos2(312.0, 79.0), egui::pos2(1114.0, 817.0));
        let v = View { eye: Vec3::new(0.0, 5.0, -10.0), target: Vec3::ZERO };
        let vp = v.view_proj(screen.x / screen.y, centre_shift(screen, visible));
        let (at, _) = project(vp, v.target, screen).expect("in front");
        assert!((at - visible.center()).length() < 0.5, "{at:?} vs {:?}", visible.center());
    }

    #[test]
    fn with_no_panels_nothing_shifts() {
        let screen = egui::vec2(1000.0, 600.0);
        let all = egui::Rect::from_min_size(egui::Pos2::ZERO, screen);
        assert_eq!(centre_shift(screen, all), Vec2::ZERO);
    }
}
