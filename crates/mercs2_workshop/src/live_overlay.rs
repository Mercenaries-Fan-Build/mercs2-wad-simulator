//! The Live page's overlays that come from the world's data: patrol paths, trigger regions, spawners and
//! sound emitters of the layers the game has loaded.
//!
//! A layer block is a large read, so a worker opens the archive for itself and keeps what it found per
//! layer: a new layer costs one read, and the frame loop never waits. The geometry is
//! `docs/placement_data_format.md` §2.6: a `Path` names a `path` asset whose points go through the entity's
//! transform, a `LineRegion` names a `lineregion` asset, local XZ, with the entity's X and Z added once.

use crate::app::WadStack;
use glam::{Quat, Vec3};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::placement::{component_keys, component_words, placed_entities};
use mercs2_formats::types::{TYPE_HASH_LAYER, TYPE_HASH_PATH};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

/// `lineregion`'s asset type (`types.rs`).
const TYPE_HASH_LINEREGION: u32 = 0x6310_807F;
/// The classes `layerview` marks as sound.
const SOUND_CLASSES: [&str; 2] = ["SoundEffect", "SoundAmbience"];
const SPAWNER_CLASS: &str = "PopulationSimpleSpawner";

/// What the overlays draw, in world metres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    pub paths: Vec<Vec<Vec3>>,
    pub regions: Vec<Vec<Vec3>>,
    pub spawners: Vec<Vec3>,
    pub sounds: Vec<Vec3>,
}

impl Marks {
    fn extend(&mut self, other: &Marks) {
        self.paths.extend_from_slice(&other.paths);
        self.regions.extend_from_slice(&other.regions);
        self.spawners.extend_from_slice(&other.spawners);
        self.sounds.extend_from_slice(&other.sounds);
    }
}

/// `path` asset: `u16 2, u16 1, u32 n, n × vec3`.
fn path_points(body: &[u8]) -> Result<Vec<Vec3>, String> {
    let f = |at: usize| f32::from_le_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
    if body.len() < 8 {
        return Err(format!("a path of {} bytes", body.len()));
    }
    let n = u32::from_le_bytes([body[4], body[5], body[6], body[7]]) as usize;
    if body.len() < 8 + n * 12 {
        return Err(format!("a path of {n} points in {} bytes", body.len()));
    }
    Ok((0..n).map(|i| Vec3::new(f(8 + i * 12), f(12 + i * 12), f(16 + i * 12))).collect())
}

/// `lineregion` asset: `u16 2, u16 n, u32 0, n × (x, z)`.
fn region_points(body: &[u8]) -> Result<Vec<[f32; 2]>, String> {
    let f = |at: usize| f32::from_le_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
    if body.len() < 8 {
        return Err(format!("a region of {} bytes", body.len()));
    }
    let n = u16::from_le_bytes([body[2], body[3]]) as usize;
    if body.len() < 8 + n * 8 {
        return Err(format!("a region of {n} points in {} bytes", body.len()));
    }
    Ok((0..n).map(|i| [f(8 + i * 8), f(12 + i * 8)]).collect())
}

/// The chunk an asset's data sits in.
fn asset_body(w: &mut WadStack, ty: u32, hash: u32) -> Result<Vec<u8>, String> {
    let c = w.typed_container(ty, hash)?;
    mercs2_formats::ucfx::extract_chunk_body(&c, b"data").ok_or_else(|| format!("0x{hash:08X}: no data chunk"))
}

/// One layer's marks.
fn read_layer(w: &mut WadStack, name: &str) -> Result<Marks, String> {
    let block = match w.typed_container(TYPE_HASH_LAYER, pandemic_hash_m2(name)) {
        Ok(b) => b,
        // A listed layer the archive does not carry has no marks; `_AddRequest` culls the same way.
        Err(_) => return Ok(Marks::default()),
    };
    let entities: HashMap<u32, ([f32; 3], [f32; 4])> = placed_entities(&block)?.into_iter().map(|e| (e.key, (e.pos, e.quat))).collect();
    let at = |k: &u32| entities.get(k).copied();
    let mut m = Marks::default();
    for k in component_keys(&block, SPAWNER_CLASS)? {
        if let Some((p, _)) = at(&k) {
            m.spawners.push(Vec3::from(p));
        }
    }
    for class in SOUND_CLASSES {
        for k in component_keys(&block, class)? {
            if let Some((p, _)) = at(&k) {
                m.sounds.push(Vec3::from(p));
            }
        }
    }
    for (k, asset) in component_words(&block, "Path")? {
        let Some((p, q)) = at(&k) else { continue };
        let (pos, rot) = (Vec3::from(p), Quat::from_array(q));
        let points = path_points(&asset_body(w, TYPE_HASH_PATH, asset)?)?;
        m.paths.push(points.into_iter().map(|v| pos + rot * v).collect());
    }
    for (k, asset) in component_words(&block, "LineRegion")? {
        let Some((p, _)) = at(&k) else { continue };
        let points = region_points(&asset_body(w, TYPE_HASH_LINEREGION, asset)?)?;
        m.regions.push(points.into_iter().map(|[x, z]| Vec3::new(p[0] + x, p[1], p[2] + z)).collect());
    }
    Ok(m)
}

/// The worker's handle.
pub struct Overlays {
    tx: Sender<Vec<String>>,
    shared: Arc<Mutex<(Marks, Vec<String>)>>,
    asked: HashSet<String>,
}

impl Overlays {
    /// Start the worker over the archive at `base` with `overlays`.
    pub fn spawn(base: String, overlays: Vec<String>) -> Overlays {
        let (tx, rx) = channel::<Vec<String>>();
        let shared: Arc<Mutex<(Marks, Vec<String>)>> = Arc::default();
        let out = shared.clone();
        std::thread::Builder::new()
            .name("live-overlay".into())
            .spawn(move || {
                let mut w = match WadStack::open(&base, &overlays) {
                    Ok(w) => w,
                    Err(e) => {
                        out.lock().expect("the overlay marks").1.push(format!("overlays: {e}"));
                        return;
                    }
                };
                let mut done: HashMap<String, Marks> = HashMap::new();
                while let Ok(mut layers) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        layers = newer;
                    }
                    for name in &layers {
                        if done.contains_key(name) {
                            continue;
                        }
                        match read_layer(&mut w, name) {
                            Ok(m) => {
                                done.insert(name.clone(), m);
                            }
                            Err(e) => {
                                out.lock().expect("the overlay marks").1.push(format!("layer {name}: {e}"));
                                done.insert(name.clone(), Marks::default());
                            }
                        }
                        let mut marks = Marks::default();
                        for l in &layers {
                            if let Some(m) = done.get(l) {
                                marks.extend(m);
                            }
                        }
                        out.lock().expect("the overlay marks").0 = marks;
                    }
                }
            })
            .expect("spawn the overlay worker");
        Overlays { tx, shared, asked: HashSet::new() }
    }

    /// The layers now loaded: reads what it has not read, and forgets the rest's marks.
    pub fn set_layers(&mut self, layers: Vec<String>) {
        let wanted: HashSet<String> = layers.iter().cloned().collect();
        if wanted != self.asked {
            self.asked = wanted;
            let _ = self.tx.send(layers);
        }
    }

    pub fn marks(&self) -> Marks {
        self.shared.lock().expect("the overlay marks").0.clone()
    }

    pub fn errors(&self) -> Vec<String> {
        self.shared.lock().expect("the overlay marks").1.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    #[test]
    fn a_path_reads_its_points() {
        let mut b = vec![2, 0, 1, 0, 2, 0, 0, 0];
        b.extend(le(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
        assert_eq!(path_points(&b).unwrap(), [Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0)]);
    }

    #[test]
    fn a_region_reads_its_points() {
        let mut b = vec![2, 0, 2, 0, 0, 0, 0, 0];
        b.extend(le(&[1.0, 2.0, 3.0, 4.0]));
        assert_eq!(region_points(&b).unwrap(), [[1.0, 2.0], [3.0, 4.0]]);
    }

    #[test]
    fn a_cut_asset_is_an_error() {
        assert!(path_points(&[2, 0, 1, 0, 9, 0, 0, 0]).is_err());
        assert!(region_points(&[2, 0]).is_err());
    }
}
