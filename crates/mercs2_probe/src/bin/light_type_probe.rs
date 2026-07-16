//! Dev bin: **empirically pin `LightObject.light_type` + which of the 9 reflected floats carry the
//! cone/spot params.** The engine already implements spot lights (`SpotLightGpu` + the `_sl` shader
//! path) but never populates them, because the reflection field order is positional/unpinned
//! (`placement::LightObject` keeps all 9 floats raw). The WildStar `WSLight` recovery names the
//! parameter SET a Pandemic cone light carries — `ConeColor`, `ConeLength`, `ConeFallOffMin/Max`,
//! `ConeEdgeFade`, `ConeAlphaMultiplier`, `ConeOnly`
//! (`docs/reverse_engineer/saboteur_mercs2_crossval_render_physics.md`) — so we know what to look for.
//!
//! This histograms every real `LightObject` in the world: how many distinct `light_type` values exist,
//! and per type which param slots are ever non-zero (+ their ranges). If a type shows extra non-zero
//! slots the point-type lacks, those slots are the cone params and the type enum is pinned.
//!
//!   cargo run -p mercs2_probe --bin light_type_probe

use std::collections::BTreeMap;

use mercs2_engine::wad;
use mercs2_formats::placement::{light_inventory, PlacedLight};

#[derive(Default)]
struct Stats {
    count: usize,
    /// Per param slot: how many records have it non-zero, and its min/max over those.
    nonzero: [usize; 9],
    min: [f32; 9],
    max: [f32; 9],
    examples: Vec<String>,
}

impl Stats {
    fn add(&mut self, l: &PlacedLight) {
        if self.count == 0 {
            self.min = [f32::INFINITY; 9];
            self.max = [f32::NEG_INFINITY; 9];
        }
        self.count += 1;
        for i in 0..9 {
            let v = l.light.params[i];
            if v != 0.0 && v.is_finite() {
                self.nonzero[i] += 1;
                self.min[i] = self.min[i].min(v);
                self.max[i] = self.max[i].max(v);
            }
        }
        if self.examples.len() < 4 {
            self.examples.push(format!(
                "{:<26} rgb({:.2},{:.2},{:.2}) params {:?}",
                l.name.clone().unwrap_or_else(|| "<unnamed>".into()),
                l.light.color[0], l.light.color[1], l.light.color[2],
                l.light.params.map(|v| (v * 1000.0).round() / 1000.0),
            ));
        }
        if l.light.light_type == 3 {
            // Spot: what does the placement quat aim at? Rotate each candidate local axis by the quat
            // (v + 2*cross(q.xyz, cross(q.xyz,v) + q.w*v)) and report, to pin the cone-axis convention.
            let q = l.quat;
            let rot = |v: [f32; 3]| -> [f32; 3] {
                let u = [q[0], q[1], q[2]];
                let cr = |a: [f32; 3], b: [f32; 3]| [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]];
                let t = cr(u, [cr(u, v)[0] + q[3]*v[0], cr(u, v)[1] + q[3]*v[1], cr(u, v)[2] + q[3]*v[2]]);
                [v[0] + 2.0*t[0], v[1] + 2.0*t[1], v[2] + 2.0*t[2]]
            };
            let f = |v: [f32; 3]| { let d = rot(v); format!("[{:5.2},{:5.2},{:5.2}]", d[0], d[1], d[2]) };
            println!("     SPOT quat {:?} -> -Y {}  +Z {}  +X {}",
                q.map(|v| (v*100.0).round()/100.0),
                f([0.0,-1.0,0.0]), f([0.0,0.0,1.0]), f([1.0,0.0,0.0]));
        }
    }
}

fn main() {
    let mut w = match wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()) {
        Some(w) => w,
        None => {
            eprintln!("could not open vz.wad (registry lookup failed) — is the game installed?");
            return;
        }
    };

    // Scan EVERY block for LightObject records, so we see the whole authored light population
    // (layers_static + all interior/state blocks), not just one area.
    let nblocks = wad::block_paths(&w).len();
    let mut by_type: BTreeMap<u32, Stats> = BTreeMap::new();
    let mut blocks_with_lights = 0usize;
    for blk in 0..nblocks as u16 {
        let Ok(dec) = wad::decompress_block_index(&mut w, blk) else { continue };
        let lights = light_inventory(&dec);
        if lights.is_empty() {
            continue;
        }
        blocks_with_lights += 1;
        for l in &lights {
            by_type.entry(l.light.light_type).or_default().add(l);
        }
    }

    let total: usize = by_type.values().map(|s| s.count).sum();
    println!("== LightObject census: {total} lights across {blocks_with_lights} blocks (of {nblocks}) ==\n");
    for (ty, s) in &by_type {
        println!("-- light_type = {ty}  ({} lights, {:.1}%) --", s.count, 100.0 * s.count as f32 / total.max(1) as f32);
        for i in 0..9 {
            if s.nonzero[i] > 0 {
                println!(
                    "     params[{i}]: non-zero in {:5}/{:<5} ({:5.1}%)  range [{:.3} .. {:.3}]",
                    s.nonzero[i], s.count, 100.0 * s.nonzero[i] as f32 / s.count as f32,
                    s.min[i], s.max[i]
                );
            } else {
                println!("     params[{i}]: always zero");
            }
        }
        for e in &s.examples {
            println!("     e.g. {e}");
        }
        println!();
    }
    println!("READ: a type whose extra slots are non-zero where the majority type's are zero = the");
    println!("spot/cone type; those slots are the ConeLength/FallOff/EdgeFade set WSLight names.");
}
