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

/// Unwrap a 20-byte-header block to its bare `UCFX` container, which is what the readers take.
fn container_of(block: &[u8]) -> &[u8] {
    if block.len() > 4 && &block[0..4] == b"UCFX" {
        return block;
    }
    if block.len() > 20 {
        let n = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
        if 20 + n <= block.len() && &block[20..24] == b"UCFX" {
            return &block[20..20 + n];
        }
    }
    block
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
    // In-repo first, so the test is runnable by someone who is not this machine. `find_upward` was
    // written for exactly this and then left unused — the resolver went straight to `~/Downloads`
    // while the skip message named `game-files/new-models`, a directory nothing ever searched.
    for rel in [
        "game-files/new-models/RuMerc1.glb",
        "crates/mercs2_formats/tests/fixtures/char_source.glb",
    ] {
        if let Some(p) = find_upward(rel) {
            return Some(p);
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        for rel in ["Downloads/RuMerc1.glb", "Downloads/hmmm.glb"] {
            let p = PathBuf::from(&home).join(rel);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // A skip has to name every path it actually tried, or it is indistinguishable from a pass.
    eprintln!(
        "SKIPPING: no rigged character model. Tried $MERCS2_TEST_CHAR_GLB, \
         <repo>/game-files/new-models/RuMerc1.glb, \
         crates/mercs2_formats/tests/fixtures/char_source.glb, ~/Downloads/RuMerc1.glb and \
         ~/Downloads/hmmm.glb."
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

    // ------------------------------------------------------------------ the emitted block
    //
    // Everything above describes the lowering's own bookkeeping. None of it reads the bytes that
    // actually ship, which is why a block binding the whole body to a 64x128 `teeth` texture and
    // losing three of the donor's four LOD tiers passed this file unchallenged. Re-parse the
    // output with the same reader the engine's behaviour is modelled on.
    use mercs2_formats::model_cubeize::read_model_meshes;
    use mercs2_formats::texture::group_prmt_material_indices;

    let donor_c = container_of(&donor);
    let out_c = container_of(&out.block);
    let donor_mats = group_prmt_material_indices(donor_c);
    let out_mats = group_prmt_material_indices(out_c);
    let out_meshes = read_model_meshes(out_c).expect("re-read the emitted block");

    for &h in &out.hosts {
        // PRMT field 0 is the MATERIAL INDEX. The injector wrote a constant 6 here, so every host
        // named donor MTRL record 6 whatever the donor's own group used.
        let want = donor_mats.get(h).and_then(|v| v.first()).copied();
        let got = out_mats.get(h).and_then(|v| v.first()).copied();
        // Printed, and asserted present: comparing two `None`s would pass this check while proving
        // nothing, which is the failure mode the whole block exists to avoid.
        eprintln!("[char-lowering] host {h}: donor mat {want:?} -> emitted mat {got:?}");
        assert!(
            want.is_some(),
            "donor group {h} has no PRMT material index to compare against"
        );
        assert_eq!(
            got, want,
            "host group {h} must keep the donor's own material index, got {got:?} want {want:?}"
        );

        let m = out_meshes
            .iter()
            .find(|m| m.group_index == h)
            .unwrap_or_else(|| panic!("host group {h} missing from the emitted block"));

        // Every other group is neutralised, so a host left on the donor's tier disappears the
        // moment the camera crosses into a rung nothing draws at.
        assert_eq!(
            m.state_mask, 0x7F,
            "host group {h} must draw at every LOD tier (donor tiers are neutralised around it)"
        );

        // The limits, in the units they were measured in.
        assert!(
            m.distinct_bones <= mercs2_formats::char_skin::build::MAX_GROUP_BONES,
            "host group {h} weights {} bones, above the measured retail ceiling of {}",
            m.distinct_bones,
            mercs2_formats::char_skin::build::MAX_GROUP_BONES
        );
        assert!(
            m.palette_slots <= mercs2_formats::char_skin::build::MAX_PALETTE_SLOTS,
            "host group {h} declares {} palette slots, past the engine reader's {}",
            m.palette_slots,
            mercs2_formats::char_skin::build::MAX_PALETTE_SLOTS
        );
        assert!(
            (1..=8).contains(&m.range_count),
            "host group {h} range_count {} is outside the field's own 1..=8",
            m.range_count
        );
        assert!(m.prmt_draw > 0, "host group {h} must actually draw");
    }

    // And the converse: nothing but the hosts draws, or the import ships wearing the donor's
    // leftover head and gear.
    for m in &out_meshes {
        if !out.hosts.contains(&m.group_index) {
            assert_eq!(
                m.prmt_draw, 0,
                "non-host group {} must be neutralised",
                m.group_index
            );
        }
    }

    assert_eq!(
        out.stats.unbound_segs.len(),
        out.hosts.len(),
        "one SEGM row must be unbound per host, got {:?} for hosts {:?}",
        out.stats.unbound_segs,
        out.hosts
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
