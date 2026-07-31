//! End-to-end: a rigged `.glb` + a retail donor → an injected character block.
//!
//! Everything under `char_lower` compiled long before it ran. This is the test that says the
//! skinned lowering actually produces a block, because "it builds" is not the claim — the claim is
//! that a Shipment can ship a character.
//!
//! Needs the retail install and a rigged source model; skips loudly without either.

use std::path::{Path, PathBuf};

/// Walk up looking for `rel` itself, not for a marker directory.
///
/// Marker-matching was wrong here: `tools/wad_simulator/` has its own `game-files/`, so a search for
/// that name stopped at the nearest ancestor and reported the fixture missing when it was two levels
/// further up. Testing the full candidate at each level cannot make that mistake.
fn find_upward(rel: &str) -> Option<PathBuf> {
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let candidate = d.join(rel);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

fn vz_wad() -> Option<PathBuf> {
    if let Some(p) = mercs2_formats::game_paths::vz_wad_from_env() {
        return Some(p);
    }
    mercs2_formats::game_paths::wad_from_local_config(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// A rigged humanoid that is not on the game's own skeleton — the cross-rig case the whole
/// `retarget:` path exists for.
/// A rigged humanoid to lower. `MERCS2_TEST_CHAR_GLB` overrides; otherwise the working models this
/// project keeps outside the repo (they are third-party assets we do not vendor).
///
/// It must be a real humanoid rig. The first candidate tried here was
/// `game-files/new-models/.../fsb_operator.glb`, which carries **3 joints** — not a character rig at
/// all, and the lowering rejected it with "0 mapped joints have a usable inverse-bind matrix". That
/// is the reader working correctly, not a fixture that merely needs coaxing.
fn source_glb() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MERCS2_TEST_CHAR_GLB") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
        eprintln!("SKIPPING: MERCS2_TEST_CHAR_GLB is set but {} is not a file", p.display());
        return None;
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    for rel in ["Downloads/RuMerc1.glb", "Downloads/hmmm.glb"] {
        let p = PathBuf::from(&home).join(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    // A skip has to name what was missing, or it is indistinguishable from a pass.
    eprintln!(
        "SKIPPING: no rigged character model. Set MERCS2_TEST_CHAR_GLB, or place one at \
         ~/Downloads/RuMerc1.glb"
    );
    None
}

#[test]
fn a_rigged_glb_lowers_onto_a_retail_donor() {
    let Some(wad) = vz_wad() else {
        return eprintln!(
            "SKIPPING: no vz.wad discovered. Run `scripts/find-vz-wad.sh --write` or set MERCS2_GAME_DIR."
        );
    };
    let Some(glb_path) = source_glb() else {
        return eprintln!("SKIPPING: no rigged source model under game-files/new-models");
    };

    let glb = mercs2_formats::char_import::load_char_glb(&glb_path).expect("read the rigged glb");
    assert!(!glb.joint_nodes.is_empty(), "source must carry a skin");
    assert!(!glb.positions.is_empty(), "source must carry geometry");
    eprintln!(
        "[char-lowering] source: {} joints, {} verts, {} tris, {} parts",
        glb.joint_nodes.len(),
        glb.positions.len(),
        glb.tris.len(),
        glb.parts.len()
    );

    // `pmc_hum_mattias` is a hero donor — the host an outfit must use, since the wardrobe drives it
    // with the hero's own clips.
    let donor_hash = mercs2_formats::hash::pandemic_hash_m2("pmc_hum_mattias");
    let donor = mercs2_formats::donor::donor_block(&[wad], donor_hash).expect("resolve the donor");

    let opts = mercs2_formats::char_lower::LowerOpts::default();
    let out = mercs2_formats::char_lower::character_into_donor(
        &donor,
        &glb,
        mercs2_formats::hash::pandemic_hash_m2("pmc_hum_test_operator"),
        &opts,
    )
    .expect("lower the character onto the donor");

    eprintln!(
        "[char-lowering] hosts {:?}, {} verts, {} tris | {}",
        out.hosts, out.stats.vertex_count, out.stats.triangle_count, out.transfer
    );

    assert!(!out.block.is_empty(), "a block must come out");
    assert!(out.stats.vertex_count > 0, "the injected group must have geometry");
    assert!(out.stats.triangle_count > 0, "the injected group must have triangles");

    // The shipped skinning format: palette-relative BLENDINDICES need the INFO(56) range table to
    // expand back to global indices at load. No ranges means the reader cannot decode the group.
    assert!(!out.skin.ranges.is_empty(), "a character group must carry its bone-palette ranges");

    // Two bytes of joints + two of weights per vertex, times four influences.
    assert_eq!(
        out.skin.skin_bytes.len(),
        out.skin.stats.verts * 8,
        "skin bytes must be 8 per vertex (4 indices + 4 weights)"
    );
}

/// The lowering must refuse a donor it cannot fit rather than emitting a block that renders wrong.
#[test]
fn a_mesh_that_cannot_fit_any_group_is_a_loud_error() {
    let Some(wad) = vz_wad() else {
        return eprintln!("SKIPPING: no vz.wad discovered");
    };
    let Some(glb_path) = source_glb() else {
        return eprintln!("SKIPPING: no rigged source model");
    };
    let mut glb = mercs2_formats::char_import::load_char_glb(&glb_path).expect("read glb");

    // Inflate the triangle count past any donor group's budget by duplicating the index stream.
    let base = glb.tris.clone();
    for _ in 0..12 {
        glb.tris.extend_from_slice(&base);
    }

    let donor_hash = mercs2_formats::hash::pandemic_hash_m2("pmc_hum_mattias");
    let donor = mercs2_formats::donor::donor_block(&[wad], donor_hash).expect("donor");

    let err = match mercs2_formats::char_lower::character_into_donor(
        &donor,
        &glb,
        mercs2_formats::hash::pandemic_hash_m2("pmc_hum_too_big"),
        &mercs2_formats::char_lower::LowerOpts::default(),
    ) {
        Err(e) => e,
        Ok(_) => panic!("an oversized mesh must be refused, not silently injected"),
    };
    assert!(
        err.contains("decimate") || err.contains("fits"),
        "the error should tell the author what to do, got: {err}"
    );
}
