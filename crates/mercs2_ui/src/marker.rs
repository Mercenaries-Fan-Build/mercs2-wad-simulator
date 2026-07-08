//! World-space **HUD markers** behind the `Gui._Marker*` surface: blips/tripwires/discs/3D markers
//! that track a world location (or follow an object GUID), with color/scale/pulse state. The engine
//! owns the marker set; the HUD renderer projects each to screen. This is that owned state.

use std::collections::HashMap;

/// Marker shape (`_MarkerAdd` / `_MarkerAddTripwire` / `_MarkerAddDisc` / `_MarkerAdd3D`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    Blip,
    Tripwire,
    Disc,
    ThreeD,
    Objective,
}

/// A single HUD marker.
#[derive(Clone, Debug)]
pub struct Marker {
    pub kind: MarkerKind,
    pub location: [f32; 3],
    pub color: [f32; 4],
    pub scale: f32,
    /// The object GUID this marker tracks (`_MarkerSetFollowGuid`); 0 = pinned to `location`.
    pub follow_guid: u64,
    pub pulsing: bool,
}

impl Marker {
    fn new(kind: MarkerKind) -> Self {
        Marker { kind, location: [0.0; 3], color: [1.0; 4], scale: 1.0, follow_guid: 0, pulsing: false }
    }
}

/// The HUD marker registry (`Gui._Marker*` + `Gui.AddObjective`).
#[derive(Default)]
pub struct MarkerSet {
    markers: HashMap<u64, Marker>,
    next: u64,
    /// `_MarkerSetBlipLimit` — max simultaneous on-screen blips (0 = unlimited).
    pub blip_limit: u32,
}

impl MarkerSet {
    pub fn new() -> Self {
        MarkerSet { markers: HashMap::new(), next: 1, blip_limit: 0 }
    }

    pub fn len(&self) -> usize {
        self.markers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.markers.is_empty()
    }

    /// Add a marker of `kind`, returning its handle.
    pub fn add(&mut self, kind: MarkerKind) -> u64 {
        let id = self.next;
        self.next += 1;
        self.markers.insert(id, Marker::new(kind));
        id
    }

    /// `_MarkerRemove` / `MinimapRemoveObjective`.
    pub fn remove(&mut self, id: u64) {
        self.markers.remove(&id);
    }

    pub fn get(&self, id: u64) -> Option<&Marker> {
        self.markers.get(&id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Marker> {
        self.markers.get_mut(&id)
    }

    pub fn set_location(&mut self, id: u64, loc: [f32; 3]) {
        if let Some(m) = self.markers.get_mut(&id) {
            m.location = loc;
        }
    }

    pub fn set_color(&mut self, id: u64, color: [f32; 4]) {
        if let Some(m) = self.markers.get_mut(&id) {
            m.color = color;
        }
    }

    pub fn set_scale(&mut self, id: u64, scale: f32) {
        if let Some(m) = self.markers.get_mut(&id) {
            m.scale = scale;
        }
    }

    pub fn set_follow(&mut self, id: u64, guid: u64) {
        if let Some(m) = self.markers.get_mut(&id) {
            m.follow_guid = guid;
        }
    }

    pub fn set_pulsing(&mut self, id: u64, on: bool) {
        if let Some(m) = self.markers.get_mut(&id) {
            m.pulsing = on;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_configure_remove() {
        let mut s = MarkerSet::new();
        let m = s.add(MarkerKind::Objective);
        assert_ne!(m, 0);
        s.set_location(m, [10.0, 0.0, 20.0]);
        s.set_follow(m, 0x1000);
        s.set_pulsing(m, true);
        let mk = s.get(m).unwrap();
        assert_eq!(mk.location, [10.0, 0.0, 20.0]);
        assert_eq!(mk.follow_guid, 0x1000);
        assert!(mk.pulsing);
        s.remove(m);
        assert!(s.get(m).is_none());
    }
}
