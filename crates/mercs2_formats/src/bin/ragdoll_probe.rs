//! ragdoll_probe — TEMPORARY W6 recon: scan the whole retail vz.wad for Havok
//! physics/ragdoll class instances (hkpRigidBody, hkpRagdollConstraintData,
//! hkaRagdollInstance, hkpConstraintInstance, motions, …). Answers: does the
//! retail PC WAD *serialize* ragdoll physics, or is it built procedurally?
//!
//! Usage:  MERCS2_GAME_DIR=<install> cargo run -p mercs2_formats --bin ragdoll_probe

use std::collections::BTreeMap;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::havok::{find_packfiles, parse_packfile_raw, HAVOK_MAGIC};
use mercs2_formats::sges::decompress_block;

fn f32_le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Dump every hkpCapsuleShape in a decompressed block: raw float window + a best-guess
/// (radius, vertexA, vertexB) decode so the layout can be pinned against real limb sizes.
fn dump_capsules(dec: &[u8], extract_fixture: bool) {
    let mut at = 0;
    while let Some(rel) = dec[at..].windows(8).position(|w| w == HAVOK_MAGIC) {
        let off = at + rel;
        let Ok(raw) = parse_packfile_raw(&dec[off..]) else {
            at = off + 8;
            continue;
        };
        let ncaps = raw.vfixups.iter().filter(|(_, c)| c == "hkpCapsuleShape").count();
        if ncaps == 11 && extract_fixture {
            let end = (off + raw.size).min(dec.len());
            let out = "crates/mercs2_formats/tests/fixtures/ragdoll_capsules_le.bin";
            std::fs::create_dir_all("crates/mercs2_formats/tests/fixtures").ok();
            std::fs::write(out, &dec[off..end]).expect("write fixture");
            println!("  >>> wrote fixture {out} ({} bytes, 11 capsules)", end - off);
        }
        for (src, cname) in &raw.vfixups {
            if cname != "hkpCapsuleShape" && cname != "hkpSphereShape" {
                continue;
            }
            if cname == "hkpSphereShape" {
                let obj = off + raw.data_pk + *src;
                print!("  SPHERE @blkoff {obj:#x}: floats[0..8] ");
                for i in 0..8 { print!("{:+.4} ", f32_le(dec, obj + i * 4)); }
                println!();
                continue;
            }
            let obj = off + raw.data_pk + *src;
            print!("  capsule @blkoff {obj:#x}: floats[0..20] ");
            for i in 0..20 {
                print!("{:+.4} ", f32_le(dec, obj + i * 4));
            }
            println!();
            // hint: also show the u32 words for the header region
            print!("                     u32[0..8] ");
            for i in 0..8 {
                print!("{:#010x} ", u32_le(dec, obj + i * 4));
            }
            println!();
        }
        at = off + raw.size.max(8);
    }
}

fn main() {
    use mercs2_formats::hash::{pandemic_hash, pandemic_hash_m2};
    let args: Vec<String> = std::env::args().collect();
    let extract_fixture = args.iter().any(|a| a == "--extract-fixture");
    let bones = [
        "Bone_Hips", "Bone_Chest", "Bone_Head",
        "Bone_LThigh", "Bone_RThigh", "Bone_LShin", "Bone_RShin",
        "Bone_LBicep", "Bone_RBicep", "Bone_LForearm", "Bone_RForearm",
        "Bone_Spine2",
    ];
    let m1: Vec<u32> = bones.iter().map(|b| pandemic_hash(b)).collect();
    let m2: Vec<u32> = bones.iter().map(|b| pandemic_hash_m2(b)).collect();
    if args.iter().any(|a| a == "--hashes") {
        println!("=== bone name-hashes (m1 / m2) ===");
        for (i, b) in bones.iter().enumerate() {
            println!("  {b:16} m1={:#010x}  m2={:#010x}", m1[i], m2[i]);
        }
        return;
    }

    let path = mercs2_formats::game_paths::vz_wad_from_env()
        .or_else(|| {
            mercs2_formats::game_paths::wad_from_local_config(std::path::Path::new("."))
        })
        .expect("set MERCS2_GAME_DIR / VZ_WAD or .mercs2-local.toml");
    let mut f = std::fs::File::open(&path).expect("open vz.wad");
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");
    let nblocks = arch.indx.len();
    eprintln!("vz.wad {} : {} blocks", path.display(), nblocks);

    // Strings that would only appear if ragdoll/physics is serialized.
    let needles: &[&str] = &[
        "hkpRigidBody",
        "hkpRagdollConstraintData",
        "hkpRagdollLimitsData",
        "hkpRagdollMotorConstraintAtom",
        "hkaRagdollInstance",
        "hkpConstraintInstance",
        "hkpConstraintData",
        "hkpBallAndSocketConstraintData",
        "hkpLimitedHingeConstraintData",
        "Motion",
        "hkaSkeletonMapper",
        "hkaSkeleton",
        "hkpConvexVerticesShape",
    ];

    let mut global_classes: BTreeMap<String, u64> = BTreeMap::new();
    let mut str_block_hits: BTreeMap<&str, Vec<u16>> = BTreeMap::new();
    let mut packfile_blocks = 0u64;
    let mut capsule_blocks: Vec<(u16, BTreeMap<String, u32>, bool)> = Vec::new();

    for blk in 0..nblocks {
        let blk = blk as u16;
        let Ok(dec) = decompress_block(&mut f, &arch.indx, blk) else { continue };
        // raw string scan (finds classnames even if the packfile parse stumbles)
        for n in needles {
            if find_sub(&dec, n.as_bytes()).is_some() {
                let e = str_block_hits.entry(n).or_default();
                if e.len() < 12 {
                    e.push(blk);
                }
            }
        }
        let has_hier = find_sub(&dec, b"HIER").is_some();
        // Verify hash variant: does a real character skeleton in this block carry our bone hashes?
        if has_hier {
            if let Ok(sk) = mercs2_formats::skeleton::Skeleton::from_block(&dec) {
                if sk.bones.len() > 40 {
                    let hs: std::collections::HashSet<u32> = sk.bones.iter().map(|b| b.name_hash).collect();
                    let n1 = m1.iter().filter(|h| hs.contains(h)).count();
                    let n2 = m2.iter().filter(|h| hs.contains(h)).count();
                    if n1 > 0 || n2 > 0 {
                        println!("  [skel] block {blk}: {} bones, m1-hits={n1}/12 m2-hits={n2}/12", sk.bones.len());
                    }
                }
            }
        }
        // structured: parse every embedded packfile, tally class instance counts
        let pfs = find_packfiles(&dec);
        if !pfs.is_empty() {
            packfile_blocks += 1;
        }
        let mut blk_caps = 0u32;
        let mut blk_classes: BTreeMap<String, u32> = BTreeMap::new();
        for (_off, pf) in pfs {
            for (name, c) in &pf.class_counts {
                *global_classes.entry(name.clone()).or_insert(0) += *c as u64;
                *blk_classes.entry(name.clone()).or_insert(0) += *c;
                if name == "hkpCapsuleShape" || name == "hkpSphereShape" {
                    blk_caps += *c;
                }
            }
        }
        if blk_caps > 0 {
            capsule_blocks.push((blk, blk_classes.clone(), has_hier));
            if blk_classes.get("hkpCapsuleShape").copied().unwrap_or(0) >= 1
                || blk_classes.get("hkpSphereShape").copied().unwrap_or(0) >= 1 {
                println!("\n--- capsule geometry dump, block {blk} ---");
                dump_capsules(&dec, extract_fixture);
            }
        }
    }

    println!("\n=== blocks bearing hkpCapsuleShape / hkpSphereShape ===");
    for (blk, classes, has_hier) in &capsule_blocks {
        let caps = classes.get("hkpCapsuleShape").copied().unwrap_or(0);
        let sph = classes.get("hkpSphereShape").copied().unwrap_or(0);
        println!("  block {blk:5}  capsules={caps} spheres={sph}  HIER={has_hier}  classes={classes:?}");
    }

    println!("\n=== packfile-bearing blocks: {packfile_blocks} ===");
    println!("\n=== raw classname string hits (block indices) ===");
    for n in needles {
        match str_block_hits.get(n) {
            Some(b) => println!("  {n:34} : {} blocks e.g. {:?}", b.len(), b),
            None => println!("  {n:34} : (none)"),
        }
    }
    println!("\n=== all class-instance counts across every embedded packfile ===");
    for (name, c) in &global_classes {
        println!("  {name:40} x{c}");
    }
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}
