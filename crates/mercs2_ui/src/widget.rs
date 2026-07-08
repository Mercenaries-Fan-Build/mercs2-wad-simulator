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

/// A single widget node.
#[derive(Clone, Debug)]
pub struct Widget {
    pub kind: WidgetKind,
    pub location: [f32; 2],
    pub corrected_location: [f32; 2],
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
}

impl Widget {
    fn new(kind: WidgetKind, z: i32) -> Self {
        Widget {
            kind,
            location: [0.0, 0.0],
            corrected_location: [0.0, 0.0],
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
}

impl WidgetTree {
    pub fn new() -> Self {
        WidgetTree { widgets: HashMap::new(), next: 1, z_top: 0 }
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
}
