//! `clip-pose` — sample a clip on a model the way the engine poses it
//! (`mercs2_engine::pose::havok_palette_in_place`) and print, per frame, chosen bones' model-space
//! positions and their positions in a reference bone's frame. A pose that holds an object steady in
//! the reference bone's frame (hands on a weapon, relative to the chest) shows as constant rows in
//! the second table; a broken decode or binding shows as drift.
//!
//! Bones are named by their HIER name hash (`0xHASH`).

use mercs2_engine::game_world::{load_clips_for_rig, load_model_by_hash};
use mercs2_engine::wad;
use mercs2_formats::skeleton::{affine_inverse, mat4_mul, transform_point};

pub fn clip_pose(wadpath: &str, model: u32, clip: u32, bones: &[u32], reference: u32, frames: usize) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let (m, _, _) = load_model_by_hash(&mut w, model).ok_or_else(|| format!("model 0x{model:08X} does not load"))?;
    let rig = &m.skin.rig;
    let index = |h: u32| rig.iter().position(|b| b.name_hash == h).ok_or_else(|| format!("bone 0x{h:08X} is not in the model's HIER"));
    let r = index(reference)?;
    let picked: Vec<usize> = bones.iter().map(|&h| index(h)).collect::<Result<_, _>>()?;
    let hier: Vec<u32> = rig.iter().map(|b| b.name_hash).collect();
    let ca = load_clips_for_rig(&mut w, &hier, &[clip])
        .pop()
        .flatten()
        .ok_or_else(|| format!("clip 0x{clip:08X} binds to no animgroup for model 0x{model:08X}"))?;
    println!(
        "model 0x{model:08X} clip 0x{clip:08X}: {} tracks, {:.3}s; reference bone 0x{reference:08X} (#{r})",
        ca.num_transform_tracks, ca.clip.duration
    );
    let frames = frames.max(2);
    let mut model_rows = Vec::new();
    let mut ref_rows = Vec::new();
    for f in 0..frames {
        let t = ca.clip.duration * f as f32 / (frames - 1) as f32;
        let pal = mercs2_engine::pose::havok_palette_in_place(rig, &ca.clip.sample_local(t), &ca.track_to_hier, ca.num_transform_tracks);
        // Row-vector skin: palette[b] = inv_bind[b] · model[b], so model[b] = world_bind[b] · palette[b].
        let model_of = |b: usize| mat4_mul(&rig[b].world_bind, &pal[b]);
        let to_ref = affine_inverse(&model_of(r));
        let mut a = format!("t={t:5.3}");
        let mut b = format!("t={t:5.3}");
        for &i in &picked {
            let p = model_of(i)[3];
            let q = transform_point(&to_ref, [p[0], p[1], p[2]]);
            a += &format!("  [{:+.3},{:+.3},{:+.3}]", p[0], p[1], p[2]);
            b += &format!("  [{:+.3},{:+.3},{:+.3}]", q[0], q[1], q[2]);
        }
        model_rows.push(a);
        ref_rows.push(b);
    }
    let names: Vec<String> = bones.iter().map(|h| format!("0x{h:08X}")).collect();
    println!("model-space positions ({}):", names.join(" "));
    for row in &model_rows {
        println!("  {row}");
    }
    println!("in the reference bone's frame:");
    for row in &ref_rows {
        println!("  {row}");
    }
    Ok(())
}
