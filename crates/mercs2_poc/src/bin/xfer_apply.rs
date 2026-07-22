//! Apply donor weight transfer to a conformed import and write the injected block.
//!
//! Experiment, measured rather than argued: does sampling the SHIPPED donor's skinning reach the
//! retail multi-influence distribution (82.8% / 93.3%) where inheriting the source rig's own
//! weights reaches only 14.6%?
//!
//!   xfer_apply <model.glb> <donor.block> <out.bin> [--name 0xHASH] [--group N] [-k N]

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::build::build_palette_ranges;
use mercs2_formats::char_skin::transfer::{
    adjacency, clamp_to_donor_reach, smooth_weights, transfer_weights_pruned, vertex_normals,
    DonorSample, TransferOpts,
};
use mercs2_formats::char_skin::{build_character, TargetSkeleton};
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::model_inject::{
    inject_character_into_donor_block, inject_character_multi_into_donor_block, ExternalMesh,
    MtrlRepoint,
};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn flag<'a>(a: &'a [String], name: &str) -> Option<&'a str> {
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).map(|s| s.as_str())
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("usage: xfer_apply <model.glb> <donor.block> <out.bin> [--name 0xHASH] [--group N] [-k N]");
        std::process::exit(2);
    }
    let (glb_path, donor_path, out_path) = (&a[1], &a[2], &a[3]);
    let name = flag(&a, "--name")
        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0xDFDF_5B5D);
    // Comma-separated ordinals route through the MULTI-group path, which gives each host group its
    // OWN INFO(56) palette. That is what keeps a 70-bone transfer inside the format: one whole-model
    // palette needs 76 slots (retail max measured 48), three per-group palettes need far fewer each.
    let groups: Vec<usize> = flag(&a, "--group")
        .unwrap_or("3")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let k: usize = flag(&a, "-k").and_then(|s| s.parse().ok()).unwrap_or(4);
    let prune: f64 = flag(&a, "--prune").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    // Smoothing passes over the target's own mesh connectivity. Sampling is per-point, so isolated
    // vertices can land on a neighbour body part; averaging over mesh edges removes those without
    // touching boundaries where a whole run of vertices agrees.
    let smooth: usize = flag(&a, "--smooth").and_then(|s| s.parse().ok()).unwrap_or(2);
    let lambda: f64 = flag(&a, "--lambda").and_then(|s| s.parse().ok()).unwrap_or(0.5);
    // Allowance for the import legitimately standing a little proud of the donor surface. 0 disables
    // the reach clamp entirely.
    let reach: f64 = flag(&a, "--reach").and_then(|s| s.parse().ok()).unwrap_or(1.15);
    // MTRL record index per host group, parallel to --group. Each part needs its OWN record or its
    // textures cannot differ from its neighbours'; the injector otherwise hardcodes record 6 for
    // every group. Defaults are three records of pmc_hum_mattias whose nine texture slots are
    // pairwise distinct, which is what lets a global-value-scan repoint address them independently.
    let part_materials: Vec<u32> = flag(&a, "--materials")
        .unwrap_or("1,12,3")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    // from:to texture repoints, comma separated, e.g. 0xFAF2CF03:0x1234ABCD
    let repoints: Vec<MtrlRepoint> = flag(&a, "--repoint")
        .map(|s| {
            s.split(',')
                .filter_map(|p| {
                    let (f, t) = p.split_once(':')?;
                    Some(MtrlRepoint {
                        from: u32::from_str_radix(f.trim().trim_start_matches("0x"), 16).ok()?,
                        to: u32::from_str_radix(t.trim().trim_start_matches("0x"), 16).ok()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let glb = gltf::load_char_glb(glb_path).expect("glb");
    let donor_block = std::fs::read(donor_path).expect("donor");
    let skel = Skeleton::from_block(&donor_block).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);
    let mut cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");

    // Donor surface samples: every skinned vertex the shipped model has, with the GLOBAL bones the
    // reader expands its per-group palette to. All groups — group 3 alone is only the torso, and a
    // partial source would leave limbs sampling from nothing.
    let ucfx_len = u32::from_le_bytes(donor_block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&donor_block[20..20 + ucfx_len]).expect("donor meshes");
    let mut donor: Vec<DonorSample> = Vec::new();
    let mut flipped = 0usize;
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        // Derive this mesh's normals from its own triangles, then check the derived field against
        // the stored one: a de-stripped IBUF can come out with the opposite winding, and a globally
        // flipped normal field would make every compatibility test fail identically (silently
        // degrading to plain nearest-neighbour rather than erroring). Measure it instead.
        let mpos: Vec<[f64; 3]> =
            m.positions.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
        let mut dn = vertex_normals(&mpos, &m.tris);
        if !m.normals.is_empty() {
            let agree: f64 = (0..dn.len().min(m.normals.len()))
                .map(|i| {
                    let s = m.normals[i];
                    dn[i][0] * s[0] as f64 + dn[i][1] * s[1] as f64 + dn[i][2] * s[2] as f64
                })
                .sum();
            if agree < 0.0 {
                for x in dn.iter_mut() {
                    for axis in 0..3 {
                        x[axis] = -x[axis];
                    }
                }
                flipped += 1;
            }
        }
        for i in 0..m.positions.len() {
            let mut infl = Vec::new();
            let tot: f64 = (0..4).map(|c| m.weights[i][c] as f64).sum();
            if tot <= 0.0 {
                continue;
            }
            for c in 0..4 {
                let w = m.weights[i][c] as f64;
                if w > 0.0 {
                    infl.push((m.joints[i][c] as u32, w / tot));
                }
            }
            donor.push(DonorSample {
                pos: mpos[i],
                normal: dn.get(i).copied().unwrap_or([0.0; 3]),
                infl,
            });
        }
    }
    println!("donor samples: {} ({} meshes re-wound to match stored normals)", donor.len(), flipped);

    // Optional OBJ dumps of the SOURCE mesh and the CONFORMED mesh, in their own spaces. The
    // conform fits foreign geometry onto the target skeleton's proportions, which is a real change
    // of shape and the one step nothing so far has actually looked at: every check to date compared
    // the conformed mesh against the DONOR, never against the model it is supposed to be.
    if let Some(dir) = flag(&a, "--dump-obj") {
        let write = |name: &str, pts: &[[f64; 3]], tris: &[[u32; 3]]| {
            let mut o = String::new();
            for p in pts {
                o.push_str(&format!("v {:.6} {:.6} {:.6}
", p[0], p[1], p[2]));
            }
            for t in tris {
                o.push_str(&format!("f {} {} {}
", t[0] + 1, t[1] + 1, t[2] + 1));
            }
            let path = format!("{dir}/{name}.obj");
            std::fs::write(&path, o).expect("write obj");
            println!("  dumped {path} ({} verts, {} tris)", pts.len(), tris.len());
        };
        // Normalise the source to the conformed model's height so the two are comparable as SHAPES
        // rather than as scales - the glTF is in centimetres and the container is in metres.
        let ylo = glb.positions.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
        let yhi = glb.positions.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
        let clo = cs.posed.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
        let chi = cs.posed.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
        let sc = (chi - clo) / (yhi - ylo).max(1e-9);
        let src: Vec<[f64; 3]> = glb
            .positions
            .iter()
            .map(|p| [p[0] * sc, (p[1] - ylo) * sc + clo, p[2] * sc])
            .collect();
        write("src_scaled", &src, &glb.tris);
        write("conformed", &cs.posed, &glb.tris);
    }

    // Permanent guard on the normal field. Conforming re-poses the geometry, so the SOURCE glTF's
    // normals stop describing the surface — measured here at mean dot -0.02 with 91.5% of vertices
    // worse than 0.7, i.e. a normal field with no relation to the shape it was lighting. The bug was
    // invisible for a long time because nothing renders normals directly; it shows up only as a
    // model that looks subtly wrong under light. `CharSkin::nrm` carries them through the same
    // transform as the positions, which scores 0.94 — the residual being the mesh's real hard edges,
    // which a geometric derivation cannot reproduce and this preserves.
    {
        let conf: Vec<[f64; 3]> =
            cs.pos.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
        let geo = vertex_normals(&conf, &glb.tris);
        let n = geo.len().min(cs.nrm.len());
        let mut sum = 0.0f64;
        let mut bad = 0usize;
        for i in 0..n {
            let c = cs.nrm[i];
            let d = geo[i][0] * c[0] as f64 + geo[i][1] * c[1] as f64 + geo[i][2] * c[2] as f64;
            sum += d;
            if d < 0.7 {
                bad += 1;
            }
        }
        println!(
            "normals: conformed field agrees with conformed geometry at mean dot {:.4}, {} of {}              below 0.7 ({:.1}%)",
            sum / n as f64, bad, n, 100.0 * bad as f64 / n as f64
        );
    }

    // Target normals in the SAME space as the sampled positions. `cs.posed` is the conformed mesh,
    // so its normals must be derived from it — the source GLB's own normals are in the pre-conform
    // space and would be rotated relative to the donor.
    let tnorm = vertex_normals(&cs.posed, &glb.tris);
    let mut t = transfer_weights_pruned(
        &donor,
        &cs.posed,
        target.height,
        &TransferOpts { k, min_weight: prune, target_normals: &tnorm, exclude_radius: 0.0 },
    );
    if reach > 0.0 {
        let bone_pos: Vec<[f64; 3]> = skel
            .bones
            .iter()
            .map(|b| {
                let p = b.bind_pos();
                [p[0] as f64, p[1] as f64, p[2] as f64]
            })
            .collect();
        let n = clamp_to_donor_reach(&mut t.per_vertex, &cs.posed, &donor, &bone_pos, 0.99, reach);
        println!("reach clamp (margin {reach}): trimmed {n} vertices");
    }
    if smooth > 0 {
        let adj = adjacency(cs.posed.len(), &glb.tris);
        smooth_weights(&mut t.per_vertex, &adj, smooth, lambda);
        println!("smoothed: {smooth} pass(es), lambda {lambda}");
    }
    println!(
        "transfer: k={k}  median nearest {:.4} m ({:.1}% of height)  far {} ({:.1}%)",
        t.median_dist, 100.0 * t.median_dist / target.height, t.far,
        100.0 * t.far as f64 / cs.posed.len() as f64
    );

    // Rebuild the palette from the bones the transfer actually used, then re-encode skin bytes.
    let mut used: Vec<u32> = t.per_vertex.iter().flatten().map(|x| x.0).collect();
    used.sort_unstable();
    used.dedup();
    let (ranges32, slot_of, slots) = build_palette_ranges(&used);
    println!("transferred bones: {}  palette: {} slots / {} runs", used.len(), slots, ranges32.len());

    let nv = cs.posed.len();
    let mut skin = vec![0u8; nv * 8];
    let mut multi = 0usize;
    for (vi, infl) in t.per_vertex.iter().enumerate() {
        // quantise to 255 with the residual on the largest fractional part (same policy as build.rs)
        let scaled: Vec<(u8, f64)> = infl
            .iter()
            .filter_map(|(b, w)| slot_of.get(b).map(|&s| (s, 255.0 * w)))
            .collect();
        let mut q: Vec<(u8, i64)> = scaled.iter().map(|&(s, x)| (s, x.floor() as i64)).collect();
        let rem = 255 - q.iter().map(|p| p.1).sum::<i64>();
        let mut order: Vec<usize> = (0..scaled.len()).collect();
        order.sort_by(|&x, &y| {
            let fy = scaled[y].1 - scaled[y].1.floor();
            let fx = scaled[x].1 - scaled[x].1.floor();
            fy.partial_cmp(&fx).unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in 0..rem.max(0) as usize {
            if q.is_empty() { break; }
            let idx = order[i % q.len()];
            q[idx].1 += 1;
        }
        if q.iter().filter(|p| p.1 > 0).count() > 1 {
            multi += 1;
        }
        for (i, (s, w)) in q.iter().take(4).enumerate() {
            skin[vi * 8 + i] = *s;
            skin[vi * 8 + 4 + i] = (*w).clamp(0, 255) as u8;
        }
    }
    println!(
        "multi-influence AFTER transfer: {:.1}% ({} of {}) — shipped is 82.8% / 93.3%",
        100.0 * multi as f64 / nv as f64, multi, nv
    );

    cs.skin_bytes = skin;
    cs.ranges = ranges32.iter().map(|&(b, c)| (b as u16, c as u16)).collect();

    // For the MULTI path `joints` must be GLOBAL bone indices: the injector derives each group's
    // palette from the bones that group actually uses. For the single path it must be palette
    // SLOTS, because the whole-model palette is supplied alongside. Same bytes, different meaning.
    let global_joints: Vec<[u8; 4]> = t
        .per_vertex
        .iter()
        .map(|infl| {
            let mut o = [0u8; 4];
            for (i, (b, _)) in infl.iter().take(4).enumerate() {
                o[i] = *b as u8;
            }
            o
        })
        .collect();
    let weights: Vec<[u8; 4]> = (0..nv)
        .map(|i| {
            [
                cs.skin_bytes[i * 8 + 4],
                cs.skin_bytes[i * 8 + 5],
                cs.skin_bytes[i * 8 + 6],
                cs.skin_bytes[i * 8 + 7],
            ]
        })
        .collect();
    let slot_joints: Vec<[u8; 4]> = (0..nv)
        .map(|i| {
            [
                cs.skin_bytes[i * 8],
                cs.skin_bytes[i * 8 + 1],
                cs.skin_bytes[i * 8 + 2],
                cs.skin_bytes[i * 8 + 3],
            ]
        })
        .collect();

    // FAITHFUL PARTITION: sub-object first, then contiguous bone span.
    //
    // Retail authors a character as many small draw groups (shipped mattias: 22, bone counts
    // 2/9/2/48/27/6/2/4/13/48/...). Crucially their bone sets are near-CONTIGUOUS in HIER index:
    // group 3 packs 48 bones into 48 slots over 5 runs, chris 45 into 45 over 7 — zero gap
    // bridging. A group is a body REGION, and HIER indices are hierarchical, so a region is an
    // index range.
    //
    // Partitioning only by source primitive is too coarse: 50 Cent's "body" is one primitive
    // spanning the whole body, needing 47 scattered bones -> 8 runs -> 51 slots (+4 bridged).
    // So order triangles by (part, dominant bone) and cut into groups along that order: each
    // group then covers one part and a contiguous bone span, which is the retail shape.
    let dom_of = |tri: &[u32; 3]| -> u32 {
        let mut best = (0u8, u32::MAX);
        for &v in tri {
            let vi = v as usize;
            for c in 0..4 {
                let w = cs.skin_bytes[vi * 8 + 4 + c];
                if w > best.0 {
                    best = (w, global_joints[vi][c] as u32);
                }
            }
        }
        best.1
    };
    let mut part_of = vec![0usize; glb.tris.len()];
    for (pi, part) in glb.parts.iter().enumerate() {
        for t in part.tri_start..(part.tri_start + part.tri_count).min(part_of.len()) {
            part_of[t] = pi;
        }
    }
    let mut order: Vec<usize> = (0..glb.tris.len()).collect();
    order.sort_by_key(|&i| (part_of[i], dom_of(&glb.tris[i])));
    let tris: Vec<[u32; 3]> = order.iter().map(|&i| glb.tris[i]).collect();

    // ONE SOURCE PART PER HOST GROUP.
    //
    // A draw group carries exactly one material, so a group spanning two source parts cannot be
    // textured: whichever material it names is wrong for one of them. The previous split cut the
    // triangle order into EQUAL chunks, which straddles part boundaries by construction (parts here
    // are 9173/6191/320 triangles against a 3921 chunk), and that is why the import could never
    // wear its own textures no matter what was packed.
    //
    // Splitting by part instead is also how retail authors a character - one sub-object, one
    // material - so this is the faithful shape as well as the necessary one.
    let nparts = glb.parts.len();
    if nparts > groups.len() {
        eprintln!(
            "partition: {nparts} source parts but only {} host groups; each group carries ONE              material, so pass at least {nparts}",
            groups.len()
        );
        std::process::exit(2);
    }

    // Allocate host groups to parts, then split each part's triangles across ITS OWN groups.
    //
    // A part may need more than one group even though it has one material: the palette cap is on
    // BONES, not triangles, and 50 Cent's head part alone reaches 44 bones / 50 slots against the
    // 48 the game ships. Splitting a part across several groups is fine -- they simply share a
    // material -- whereas merging two parts into one group is not, because the group could then
    // only name one of their materials. So: a group never spans parts; a part may span groups.
    //
    // Slots are handed out in proportion to triangle count (largest remainder, minimum one each),
    // which puts the extra groups where the geometry and therefore the bones actually are.
    let mut alloc = vec![1usize; nparts];
    let mut spare = groups.len() - nparts;
    if spare > 0 {
        let total: f64 = glb.parts.iter().map(|p| p.tri_count as f64).sum::<f64>().max(1.0);
        let mut want: Vec<(f64, usize)> = glb
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| (p.tri_count as f64 / total * spare as f64, i))
            .collect();
        // whole shares first, then the largest remainders
        for (w, i) in want.iter() {
            let take = (w.floor() as usize).min(spare);
            alloc[*i] += take;
            spare -= take;
        }
        want.sort_by(|a, b| {
            (b.0 - b.0.floor()).partial_cmp(&(a.0 - a.0.floor())).unwrap_or(std::cmp::Ordering::Equal)
        });
        for (_, i) in want {
            if spare == 0 {
                break;
            }
            alloc[i] += 1;
            spare -= 1;
        }
    }

    // slot index -> which part it carries, and each part's first slot
    let mut slot_part: Vec<usize> = Vec::with_capacity(groups.len());
    for (pi, &n) in alloc.iter().enumerate() {
        for _ in 0..n {
            slot_part.push(pi);
        }
    }
    let mut first_slot = vec![0usize; nparts];
    for (si, &pi) in slot_part.iter().enumerate() {
        if slot_part[..si].iter().all(|&q| q != pi) {
            first_slot[pi] = si;
        }
    }
    // Within a part, spread its triangles evenly over its own slots.
    let mut seen = vec![0usize; nparts];
    let tri_group: Vec<usize> = order
        .iter()
        .map(|&i| {
            let pi = part_of[i];
            let n = alloc[pi].max(1);
            let per = (glb.parts[pi].tri_count + n - 1) / n.max(1);
            let k = (seen[pi] / per.max(1)).min(n - 1);
            seen[pi] += 1;
            first_slot[pi] + k
        })
        .collect();

    if part_materials.len() < nparts {
        eprintln!(
            "--materials gave {} record(s) for {nparts} source parts; each part needs its own              MTRL record or its textures cannot differ from its neighbours'",
            part_materials.len()
        );
        std::process::exit(2);
    }
    // One MTRL record per PART, expanded to per-slot for the injector.
    let materials: Vec<u32> = slot_part.iter().map(|&pi| part_materials[pi]).collect();
    for (si, &pi) in slot_part.iter().enumerate() {
        let n = tri_group.iter().filter(|&&g| g == si).count();
        println!(
            "partition: host group {} <- part {:?} ({} tris) MTRL {}",
            groups[si], glb.parts[pi].name, n, materials[si]
        );
    }

    let mut mesh = ExternalMesh {
        positions: cs.pos.clone(),
        // CONFORMED normals. The source glTF's field describes the PRE-conform surface and agrees
        // with the conformed geometry at mean dot -0.01, so using it lights the model with a
        // normal field unrelated to its shape.
        normals: if cs.nrm.is_empty() { glb.normals.clone() } else { cs.nrm.clone() },
        uvs: glb.uvs.clone(),
        tris,
        joints: slot_joints,
        weights,
    };

    let (block, target_group, vcount) = if groups.len() == 1 {
        let (b, st) = inject_character_into_donor_block(
            &donor_block, &mesh, &cs.ranges, groups[0], &[], name,
        )
        .expect("inject");
        (b, st.target_group, st.vertex_count)
    } else {
        mesh.joints = global_joints;
        let (b, _audits, st) = inject_character_multi_into_donor_block(
            &donor_block, &mesh, &groups, &repoints, name, true, Some(&tri_group),
            Some(&materials),
        )
        .expect("inject multi");
        for (f, t, n) in &st.mtrl_repoints {
            println!("  repoint 0x{f:08X} -> 0x{t:08X}: {n} occurrence(s)");
            if *n == 0 {
                eprintln!("  WARNING: repoint 0x{f:08X} matched nothing - the material is not in this MTRL chunk");
            }
        }
        (b, st.target_group, st.vertex_count)
    };
    std::fs::write(out_path, &block).expect("write");
    println!(
        "wrote {out_path} ({} bytes): groups {:?} <- {} verts (stats group {}, {} verts)",
        block.len(), groups, nv, target_group, vcount
    );
}
