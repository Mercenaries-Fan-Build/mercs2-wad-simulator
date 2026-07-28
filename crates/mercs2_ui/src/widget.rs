//! The retained-mode **widget tree** behind the `Hud.*` Lua surface.
//!
//! Retail drives the HUD through a Scaleform GFx overlay; the `Hud.*` cfuncs are a retained-mode widget
//! API — you `Create*Widget` a node, set its location/color/visibility/anchoring, parent it into the
//! tree, and mutate its per-kind data (image texture, text string, sprite frame, …). The engine owns
//! this widget state; the renderer walks the tree each frame to draw it. This module is that owned
//! state model: a handle registry of [`Widget`] nodes with the full property set the Lua reads back, so
//! `Set*`→`Get*` round-trip for real (a HUD script that hides a widget actually hides it). The GFx
//! *rasterization* (drawing the tree) is a separate render pass; here we own the scene-graph state.

use std::collections::HashMap;

/// Widget node type (`Hud.Create*Widget`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    /// A plain container/group node (`CreateWidget`).
    Container,
    /// A textured quad (`CreateImageWidget`).
    Image,
    /// A text label (`CreateTextWidget`).
    Text,
    /// An animated sprite sheet (`CreateSpriteWidget`).
    Sprite,
    /// A Bink movie surface (`CreateMovieWidget`).
    Movie,
    /// An embedded Scaleform `.swf` (`CreateFlashWidget`).
    Flash,
    /// A minimap surface (`MinimapCreate`).
    Minimap,
}

/// `Image` widget data.
#[derive(Clone, Debug, Default)]
pub struct ImageData {
    pub texture: String,
    pub rotation: f32,
    /// `(u0, v0, u1, v1)` texture coordinates.
    pub tex_coords: [f32; 4],
    pub tiling: bool,
}

/// `Text` widget data.
#[derive(Clone, Debug)]
pub struct TextData {
    pub text: String,
    pub font: String,
    pub wrapping: bool,
    /// 0 = left, 1 = center, 2 = right (`SetTextJustification`).
    pub justification: u8,
    pub scale: f32,
}

impl Default for TextData {
    fn default() -> Self {
        TextData { text: String::new(), font: String::new(), wrapping: false, justification: 0, scale: 1.0 }
    }
}

/// `Sprite` widget data.
#[derive(Clone, Debug, Default)]
pub struct SpriteData {
    pub texture: String,
    pub texture_size: [f32; 2],
    pub frame_size: [f32; 2],
    pub frame: u32,
    pub animating: bool,
}

/// `Movie` widget data.
#[derive(Clone, Debug, Default)]
pub struct MovieData {
    pub file: String,
    pub playing: bool,
    pub frame: u32,
    /// The end-callback registered by `Hud.SetMovieEndCallback` (`0x005BC640`), as an **opaque id**.
    ///
    /// Retail stores a Lua ref, not a closure, and so do we — the `mlua::Function` and its context
    /// arguments are held by the binding layer and keyed on this id, which keeps `mercs2_ui` free of a
    /// Lua dependency. See [`WidgetTree::take_movie_end_fires`] for how it is dispatched.
    pub end_callback: Option<u32>,
    /// Whether this movie's end callback has already been queued, so a still-`playing` movie cannot
    /// fire it twice.
    pub end_fired: bool,
}

/// `Flash` (Scaleform) widget data.
#[derive(Clone, Debug)]
pub struct FlashData {
    pub swf: String,
    pub play_speed: f32,
    pub playing: bool,
}

impl Default for FlashData {
    fn default() -> Self {
        FlashData { swf: String::new(), play_speed: 1.0, playing: true }
    }
}

/// `Minimap` widget data.
#[derive(Clone, Debug, Default)]
pub struct MinimapData {
    pub player_location: [f32; 2],
    pub focus_location: [f32; 2],
    pub rotation: f32,
    pub range: f32,
    pub radius: f32,
    pub owner: u64,
    /// Objective blips: id → world location.
    pub objectives: HashMap<u64, [f32; 3]>,
}

/// An in-flight `InterpolateWidget` animation.
///
/// **This is the spine of the game's whole GUI**, not a cosmetic tween. `MrxGuiBase`'s animation queue
/// (`Widget:AnimateToPoint` → `_HandleAnimationComplete`, `mrxguibase.lua:635-720`) drives every
/// widget move/fade by calling `_GuiInternal.InterpolateWidget(uId, nTime, …, _HandleAnimationComplete,
/// {self}, …)` and **continuing the chain from the completion callback**. If the callback never fires,
/// the queue stalls at its first entry and everything downstream of it — the cinematic fade-in that
/// starts the intro movie, and so the release of `STATE_WAITFORGAME` — never happens.
#[derive(Clone, Debug)]
pub struct WidgetAnim {
    /// Total duration in seconds, as the script requested it.
    pub duration: f32,
    /// Seconds still to run. Reaching `<= 0` snaps to the target and queues the callback.
    pub remaining: f32,
    /// Rect at the moment the animation was issued.
    pub from_location: [f32; 4],
    /// Rect to reach.
    pub to_location: [f32; 4],
    /// Colour at issue time, and the target.
    pub from_color: [f32; 4],
    pub to_color: [f32; 4],
    /// Completion callback, as an **opaque id** — retail stores a Lua ref, and so do we, which keeps
    /// this crate free of a Lua dependency. See [`WidgetTree::take_anim_completions`].
    pub on_complete: Option<u32>,
}

/// The sentinel the game passes for "leave this channel alone" in `InterpolateWidget`'s colour
/// arguments (`mrxguibase.lua:715` defaults each of R/G/B/Translucency to `-4096`).
pub const COLOR_UNCHANGED: f32 = -4096.0;

/// A single widget node.
#[derive(Clone, Debug)]
pub struct Widget {
    pub kind: WidgetKind,
    /// The widget's screen **rect**, `[x1, y1, x2, y2]` — not a point. The game's
    /// `MrxGui.Widget:SetLocation(x1, y1, x2, y2)` passes four coordinates and
    /// `Widget:GetLocation()` destructures four back (`mrxguibase.lua:746/759`); returning fewer
    /// makes callers like `MrxGuiLoadScreen.InitSaveIcon` compute on a nil.
    pub location: [f32; 4],
    /// Safe-area-corrected rect, same `[x1, y1, x2, y2]` layout (`SetWidgetCorrectedLocation`).
    pub corrected_location: [f32; 4],
    /// RGBA in the caller's domain (the game passes D3DCOLOR-style `0..255`); default white/opaque.
    pub color: [f32; 4],
    pub visible: bool,
    pub sleep: bool,
    pub ignores_pause: bool,
    pub highlightable: bool,
    /// Anchoring flag bits (`SetWidgetAnchoring`).
    pub anchoring: u32,
    pub viewport: i32,
    pub fullscreen: bool,
    pub parent: Option<u64>,
    pub children: Vec<u64>,
    /// Draw order (higher = front); `PushWidgetToFront/Back` restamp it.
    pub z: i32,
    pub image: Option<ImageData>,
    pub text: Option<TextData>,
    pub sprite: Option<SpriteData>,
    pub movie: Option<MovieData>,
    pub flash: Option<FlashData>,
    pub minimap: Option<MinimapData>,
    /// The in-flight `InterpolateWidget` animation, if any. See [`WidgetAnim`].
    pub anim: Option<WidgetAnim>,
}

impl Widget {
    fn new(kind: WidgetKind, z: i32) -> Self {
        Widget {
            kind,
            location: [0.0; 4],
            corrected_location: [0.0; 4],
            color: [255.0, 255.0, 255.0, 255.0],
            visible: true,
            sleep: false,
            ignores_pause: false,
            highlightable: false,
            anchoring: 0,
            viewport: 0,
            fullscreen: false,
            parent: None,
            children: Vec::new(),
            z,
            image: matches!(kind, WidgetKind::Image).then(ImageData::default),
            text: matches!(kind, WidgetKind::Text).then(TextData::default),
            sprite: matches!(kind, WidgetKind::Sprite).then(SpriteData::default),
            movie: matches!(kind, WidgetKind::Movie).then(MovieData::default),
            flash: matches!(kind, WidgetKind::Flash).then(FlashData::default),
            minimap: matches!(kind, WidgetKind::Minimap).then(MinimapData::default),
            anim: None,
        }
    }
}

/// The HUD widget registry — the retained-mode scene graph the `Hud.*` surface drives and the renderer
/// walks. Handles are stable, non-zero, unique.
#[derive(Default)]
pub struct WidgetTree {
    widgets: HashMap<u64, Widget>,
    next: u64,
    z_top: i32,
    /// Monotonic id source for retained callbacks. Never reused, so a stale id from a deleted widget
    /// can never be mistaken for a live registration.
    next_callback: u32,
    /// Movie end-callback ids whose movie has finished, awaiting dispatch by the script layer.
    pending_movie_ends: Vec<u32>,
    /// Animation completion ids awaiting dispatch by the script layer.
    pending_anim_completions: Vec<u32>,
}

impl WidgetTree {
    pub fn new() -> Self {
        WidgetTree { widgets: HashMap::new(), next: 1, z_top: 0, ..Default::default() }
    }

    /// Mint an opaque callback id for the script layer to associate with a retained Lua function.
    pub fn mint_callback(&mut self) -> u32 {
        self.next_callback += 1;
        self.next_callback
    }

    /// `Hud.SetMovieEndCallback(uId, fCallback, tData)` — `0x005BC640`.
    ///
    /// Returns `false` if `handle` is not a movie widget. Re-registering replaces, and clears any
    /// already-fired latch so the next play can complete again.
    pub fn set_movie_end_callback(&mut self, handle: u64, callback: Option<u32>) -> bool {
        let Some(m) = self.widgets.get_mut(&handle).and_then(|w| w.movie.as_mut()) else {
            return false;
        };
        m.end_callback = callback;
        m.end_fired = false;
        true
    }

    /// Advance movie playback one frame and queue the end callback of any movie that finished.
    ///
    /// **Headless completion model, and it is a deliberate choice.** There is no Bink decoder here, so
    /// a movie has no frame count to play out. Rather than stall forever — which is what a
    /// never-completing movie does to the game's state machine, since `MrxGuiCinematic`'s end callback
    /// is what releases `STATE_WAITFORGAME` — a playing movie completes on the tick after it starts.
    /// Observably: the cinematic is skipped and gameplay resumes, which is the same outcome as a player
    /// skipping it.
    ///
    /// The frame counter still advances so `Hud.GetMovieCurrentFrameNumber` moves rather than sitting
    /// at 0. When a real decoder lands, this is the one function that changes.
    pub fn tick_movies(&mut self) {
        for w in self.widgets.values_mut() {
            let Some(m) = w.movie.as_mut() else { continue };
            if !m.playing {
                continue;
            }
            m.frame += 1;
            if !m.end_fired {
                m.end_fired = true;
                m.playing = false;
                if let Some(id) = m.end_callback {
                    self.pending_movie_ends.push(id);
                }
            }
        }
    }

    /// Drain the movie end-callbacks awaiting dispatch. The script layer invokes the retained Lua
    /// functions these ids key.
    pub fn take_movie_end_fires(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_movie_ends)
    }

    /// `Hud.InterpolateWidget(uId, nTime, x1, y1, x2, y2, r, g, b, a, fComplete, tData, …)` — start an
    /// animation toward a rect and/or colour, with `on_complete` fired when it finishes.
    ///
    /// A colour channel of [`COLOR_UNCHANGED`] (`-4096`) holds its current value, which is the default
    /// `MrxGuiBase` passes for every channel it does not mean to touch. A `None` rect coordinate
    /// likewise holds — `_HandleAnimationComplete` passes `nil` for `x2`/`y2` when the animation point
    /// maintains the widget's dimensions.
    ///
    /// A zero (or negative) duration completes on the **next tick** rather than instantly: completing
    /// inside the setter would re-enter Lua from a binding, and the animation queue in `mrxguibase.lua`
    /// is explicitly written against a deferred completion.
    ///
    /// Returns `false` if the handle is unknown.
    #[allow(clippy::too_many_arguments)]
    pub fn interpolate(
        &mut self,
        handle: u64,
        duration: f32,
        to_location: [Option<f32>; 4],
        to_color: [f32; 4],
        on_complete: Option<u32>,
    ) -> bool {
        let Some(w) = self.widgets.get_mut(&handle) else { return false };
        let from_location = w.location;
        let from_color = w.color;
        let mut target_loc = from_location;
        for (i, v) in to_location.iter().enumerate() {
            if let Some(v) = v {
                target_loc[i] = *v;
            }
        }
        let mut target_col = from_color;
        for (i, v) in to_color.iter().enumerate() {
            if *v != COLOR_UNCHANGED {
                target_col[i] = *v;
            }
        }
        w.anim = Some(WidgetAnim {
            duration: duration.max(0.0),
            remaining: duration.max(0.0),
            from_location,
            to_location: target_loc,
            from_color,
            to_color: target_col,
            on_complete,
        });
        true
    }

    /// Advance every in-flight animation by `dt`, applying the interpolated values, and queue the
    /// completion callback of each animation that finished.
    ///
    /// Completions are **queued, not invoked** — the script layer drains them via
    /// [`take_anim_completions`](Self::take_anim_completions), so a callback that starts the next
    /// animation in the queue does so on the following tick rather than recursing inside this loop.
    pub fn tick_animations(&mut self, dt: f32) {
        for w in self.widgets.values_mut() {
            let Some(a) = w.anim.as_mut() else { continue };
            a.remaining -= dt;
            if a.remaining > 0.0 && a.duration > 0.0 {
                // Linear blend. Retail's easing curve is not recovered; linear is the honest stand-in
                // and the endpoints — which is all the script observes via GetLocation — are exact.
                let t = 1.0 - (a.remaining / a.duration).clamp(0.0, 1.0);
                for i in 0..4 {
                    w.location[i] = a.from_location[i] + (a.to_location[i] - a.from_location[i]) * t;
                    w.color[i] = a.from_color[i] + (a.to_color[i] - a.from_color[i]) * t;
                }
                continue;
            }
            // Finished: snap to the target exactly, then release the callback.
            w.location = a.to_location;
            w.color = a.to_color;
            let done = a.on_complete;
            w.anim = None;
            if let Some(id) = done {
                self.pending_anim_completions.push(id);
            }
        }
    }

    /// Drain the animation completions awaiting dispatch.
    pub fn take_anim_completions(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_anim_completions)
    }

    /// Number of live widgets.
    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }

    /// `Hud.Create*Widget` — mint a widget of `kind`, returning its handle (top of the draw order).
    pub fn create(&mut self, kind: WidgetKind) -> u64 {
        let handle = self.next;
        self.next += 1;
        self.z_top += 1;
        self.widgets.insert(handle, Widget::new(kind, self.z_top));
        handle
    }

    /// `Hud.DeleteWidget` — remove a widget and detach it from its parent + orphan its children.
    pub fn delete(&mut self, handle: u64) {
        if let Some(w) = self.widgets.remove(&handle) {
            if let Some(p) = w.parent {
                if let Some(parent) = self.widgets.get_mut(&p) {
                    parent.children.retain(|&c| c != handle);
                }
            }
            for c in w.children {
                if let Some(child) = self.widgets.get_mut(&c) {
                    child.parent = None;
                }
            }
        }
    }

    pub fn get(&self, handle: u64) -> Option<&Widget> {
        self.widgets.get(&handle)
    }

    pub fn get_mut(&mut self, handle: u64) -> Option<&mut Widget> {
        self.widgets.get_mut(&handle)
    }

    /// `AddWidgetChild` — parent `child` under `parent` (moving it out of any previous parent).
    pub fn add_child(&mut self, parent: u64, child: u64) {
        if parent == child || !self.widgets.contains_key(&parent) || !self.widgets.contains_key(&child) {
            return;
        }
        // Detach from old parent.
        if let Some(old) = self.widgets.get(&child).and_then(|c| c.parent) {
            if let Some(op) = self.widgets.get_mut(&old) {
                op.children.retain(|&c| c != child);
            }
        }
        self.widgets.get_mut(&child).unwrap().parent = Some(parent);
        let siblings = &mut self.widgets.get_mut(&parent).unwrap().children;
        if !siblings.contains(&child) {
            siblings.push(child);
        }
    }

    /// `RemoveWidgetChild` — unparent `child` from `parent`.
    pub fn remove_child(&mut self, parent: u64, child: u64) {
        if let Some(p) = self.widgets.get_mut(&parent) {
            p.children.retain(|&c| c != child);
        }
        if let Some(c) = self.widgets.get_mut(&child) {
            if c.parent == Some(parent) {
                c.parent = None;
            }
        }
    }

    /// `RemoveAllWidgetChildren`.
    pub fn remove_all_children(&mut self, parent: u64) {
        let kids = self.widgets.get(&parent).map(|p| p.children.clone()).unwrap_or_default();
        for c in &kids {
            if let Some(child) = self.widgets.get_mut(c) {
                child.parent = None;
            }
        }
        if let Some(p) = self.widgets.get_mut(&parent) {
            p.children.clear();
        }
    }

    /// `GetWidgetChildren`.
    pub fn children(&self, parent: u64) -> Vec<u64> {
        self.widgets.get(&parent).map(|p| p.children.clone()).unwrap_or_default()
    }

    /// `PushWidgetToFront` — restamp to the top of the draw order.
    pub fn push_to_front(&mut self, handle: u64) {
        self.z_top += 1;
        let z = self.z_top;
        if let Some(w) = self.widgets.get_mut(&handle) {
            w.z = z;
        }
    }

    /// `PushWidgetToBack` — restamp below everything.
    pub fn push_to_back(&mut self, handle: u64) {
        let min = self.widgets.values().map(|w| w.z).min().unwrap_or(0);
        if let Some(w) = self.widgets.get_mut(&handle) {
            w.z = min - 1;
        }
    }

    /// Live widget handles ordered back-to-front (draw order) — the renderer's walk order.
    pub fn draw_order(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.widgets.keys().copied().collect();
        ids.sort_by_key(|id| self.widgets[id].z);
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_set_get_roundtrip() {
        let mut t = WidgetTree::new();
        let w = t.create(WidgetKind::Text);
        assert_ne!(w, 0);
        // Text data present + settable.
        let td = t.get_mut(w).unwrap().text.as_mut().unwrap();
        td.text = "OBJECTIVE".into();
        assert_eq!(t.get(w).unwrap().text.as_ref().unwrap().text, "OBJECTIVE");
        // visibility roundtrips
        t.get_mut(w).unwrap().visible = false;
        assert!(!t.get(w).unwrap().visible);
    }

    #[test]
    fn parenting_and_deletion() {
        let mut t = WidgetTree::new();
        let root = t.create(WidgetKind::Container);
        let a = t.create(WidgetKind::Image);
        let b = t.create(WidgetKind::Image);
        t.add_child(root, a);
        t.add_child(root, b);
        assert_eq!(t.children(root), vec![a, b]);
        assert_eq!(t.get(a).unwrap().parent, Some(root));

        // Deleting the parent orphans the children.
        t.delete(root);
        assert!(t.get(root).is_none());
        assert_eq!(t.get(a).unwrap().parent, None);

        // Re-parent then remove.
        let root2 = t.create(WidgetKind::Container);
        t.add_child(root2, a);
        t.remove_child(root2, a);
        assert!(t.children(root2).is_empty());
    }

    #[test]
    fn z_order_front_back() {
        let mut t = WidgetTree::new();
        let a = t.create(WidgetKind::Image);
        let b = t.create(WidgetKind::Image);
        // b created after a ⇒ b in front.
        assert_eq!(t.draw_order(), vec![a, b]);
        t.push_to_front(a);
        assert_eq!(t.draw_order(), vec![b, a]);
        t.push_to_back(a);
        assert_eq!(*t.draw_order().first().unwrap(), a);
    }

    /// An animation interpolates toward its target and **fires its completion exactly once**, at the
    /// end. This is the property `MrxGuiBase`'s animation queue is built on: it chains the next
    /// animation from the completion callback, so a completion that never arrives stalls the whole GUI,
    /// and one that arrives twice double-advances the queue.
    #[test]
    fn an_animation_interpolates_then_completes_once() {
        let mut t = WidgetTree::new();
        let w = t.create(WidgetKind::Container);
        let cb = t.mint_callback();
        assert!(t.interpolate(w, 1.0, [Some(100.0), Some(0.0), None, None], [COLOR_UNCHANGED; 4], Some(cb)));

        t.tick_animations(0.5);
        let mid = t.get(w).unwrap().location[0];
        assert!((mid - 50.0).abs() < 1e-3, "halfway through, halfway there; got {mid}");
        assert!(t.take_anim_completions().is_empty(), "not finished yet");

        t.tick_animations(0.5);
        assert_eq!(t.get(w).unwrap().location[0], 100.0, "snaps exactly to the target");
        assert_eq!(t.take_anim_completions(), vec![cb], "fires on completion");

        t.tick_animations(1.0);
        assert!(t.take_anim_completions().is_empty(), "and never again");
    }

    /// A colour channel of [`COLOR_UNCHANGED`] holds — that sentinel is what `MrxGuiBase` passes for
    /// every channel an animation point does not mean to touch, so treating it as a target would fade
    /// every widget to −4096.
    #[test]
    fn the_color_sentinel_holds_a_channel() {
        let mut t = WidgetTree::new();
        let w = t.create(WidgetKind::Container);
        t.get_mut(w).unwrap().color = [10.0, 20.0, 30.0, 40.0];
        // Animate only alpha to 0; RGB must survive.
        t.interpolate(w, 0.0, [None; 4], [COLOR_UNCHANGED, COLOR_UNCHANGED, COLOR_UNCHANGED, 0.0], None);
        t.tick_animations(0.1);
        assert_eq!(t.get(w).unwrap().color, [10.0, 20.0, 30.0, 0.0]);
    }

    /// A zero-duration animation completes on the **next tick**, not inside the setter — completing
    /// synchronously would re-enter Lua from inside a binding.
    #[test]
    fn a_zero_duration_animation_completes_on_the_next_tick() {
        let mut t = WidgetTree::new();
        let w = t.create(WidgetKind::Container);
        let cb = t.mint_callback();
        t.interpolate(w, 0.0, [Some(5.0), None, None, None], [COLOR_UNCHANGED; 4], Some(cb));
        assert!(t.take_anim_completions().is_empty(), "nothing fires during the call itself");
        t.tick_animations(1.0 / 60.0);
        assert_eq!(t.take_anim_completions(), vec![cb]);
        assert_eq!(t.get(w).unwrap().location[0], 5.0);
    }

    /// A playing movie completes and fires its end callback once; `StopMovie` is an explicit cancel and
    /// must **not** produce the same completion signal as watching it through.
    #[test]
    fn a_movie_fires_its_end_callback_once_and_stop_cancels() {
        let mut t = WidgetTree::new();
        let m = t.create(WidgetKind::Movie);
        let cb = t.mint_callback();
        assert!(t.set_movie_end_callback(m, Some(cb)));

        // Not playing yet: nothing happens.
        t.tick_movies();
        assert!(t.take_movie_end_fires().is_empty());

        t.get_mut(m).unwrap().movie.as_mut().unwrap().playing = true;
        t.tick_movies();
        assert_eq!(t.take_movie_end_fires(), vec![cb]);
        t.tick_movies();
        assert!(t.take_movie_end_fires().is_empty(), "exactly once");

        // Replay, then cancel before the tick: no completion.
        {
            let mv = t.get_mut(m).unwrap().movie.as_mut().unwrap();
            mv.playing = true;
            mv.end_fired = false;
        }
        {
            let mv = t.get_mut(m).unwrap().movie.as_mut().unwrap();
            mv.playing = false;
            mv.end_fired = true; // what StopMovie does
        }
        t.tick_movies();
        assert!(t.take_movie_end_fires().is_empty(), "a stopped movie does not report completion");
    }

    /// A callback registered against a non-movie widget is rejected rather than silently accepted, so a
    /// script that targets the wrong handle finds out.
    #[test]
    fn a_movie_callback_needs_a_movie_widget() {
        let mut t = WidgetTree::new();
        let c = t.create(WidgetKind::Container);
        let cb = t.mint_callback();
        assert!(!t.set_movie_end_callback(c, Some(cb)));
    }
}
