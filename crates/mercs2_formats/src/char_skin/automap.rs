//! Generic source-rig → Mercs2 84-bone auto-mapper.
//!
//! Faithful hand-port of `mercs2-mesher/src/automap.js` (itself a port of
//! `tools/model_import_test/automap.py`). The JS uses regexes with lookahead/lookbehind;
//! Rust's `regex` crate has no lookaround and `mercs2_formats` is deliberately dep-free, so
//! the patterns are re-implemented as explicit matchers. Parity is guaranteed by the
//! `automap_parity_*` tests, which assert byte-identical output against the mesher's five
//! committed `expect_automap_*.json` fixtures (19 / 83 / 94 / 119 / 266-joint rigs).
//!
//! Bone indices below are GLOBAL HIER node indices — identical to our
//! [`crate::skeleton::Skeleton`] bone ordering (verified: `export_bundle` emits the joint
//! list as the identity mapping over the HIER-ordered rig).

use std::collections::HashMap;

// ---- target-bone tables (HIER indices into the 84-bone NPC skeleton) -------------------

/// role → HIER for the single (non-sided) core bones.
fn core(role: &str) -> Option<u32> {
    Some(match role {
        "hips" => 3,
        "spine1" => 14,
        "spine2" => 15,
        "chest" => 16,
        "neck" => 20,
        "head" => 21,
        "jaw" => 38,
        "tongue" => 39,
        "browcenter" => 37,
        _ => return None,
    })
}

/// role → `[left_hier, right_hier]`.
fn sided(role: &str) -> Option<[u32; 2]> {
    Some(match role {
        "thigh" => [6, 10],
        "calf" => [7, 11],
        "foot" => [8, 12],
        "toe" => [9, 13],
        "clavicle" => [42, 63],
        "upperarm" => [43, 64],
        "forearm" => [44, 65],
        "forearmroll" => [45, 66],
        "hand" => [46, 67],
        "eye" => [34, 33],
        "eyelid" => [29, 28],
        "brow" => [31, 30],
        "cheek" => [35, 36],
        "nose" => [23, 22],
        _ => return None,
    })
}

/// finger role → `[left_base, right_base]`; +0/+1/+2 per segment.
fn finger_base(role: &str) -> Option<[u32; 2]> {
    Some(match role {
        "index" => [48, 69],
        "middle" => [51, 72],
        "pinky" => [54, 75],
        "ring" => [57, 78],
        "thumb" => [60, 81],
        _ => return None,
    })
}

/// Ordered finger roles (first match wins) — mirrors `Object.keys(FINGERS)`.
const FINGER_ROLES: [&str; 5] = ["index", "middle", "pinky", "ring", "thumb"];

/// Roles that BLOCK the generic numbered-limb-chain fallback (a specific segment matched).
const CHAIN_BLOCKERS: [&str; 8] = [
    "toe", "foot", "calf", "thigh", "forearm", "upperarm", "hand", "clavicle",
];

// ---- classification result -------------------------------------------------------------

/// What a source joint name classifies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cls {
    /// A direct HIER index.
    Hier(u32),
    /// A deferred chain marker resolved by a depth-ordered ladder.
    Spine,
    Neck,
    LegL,
    LegR,
    ArmL,
    ArmR,
    /// No Mercs2 equivalent (helper / unrecognised).
    None,
}

// ---- normalisation + side --------------------------------------------------------------

/// `name.toLowerCase().replace(/[^a-z0-9]+/g, '_')` — collapse each non-alnum run to one `_`.
pub fn norm(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_run = false;
    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('_');
            in_run = true;
        }
    }
    out
}

/// Does `x` sit at `n[i..]` bounded by `(^|_)` before and `(_|\d|$)` after? (token match)
fn token_bounded(n: &str, x: &str) -> bool {
    let b = n.as_bytes();
    let xb = x.as_bytes();
    let mut i = 0;
    while i + xb.len() <= b.len() {
        if &b[i..i + xb.len()] == xb {
            let before_ok = i == 0 || b[i - 1] == b'_';
            let j = i + xb.len();
            let after_ok = j == b.len() || b[j] == b'_' || b[j].is_ascii_digit();
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// `sideOf(n)` — port of the JS side detector.
pub fn side_of(n: &str) -> Option<char> {
    for alt in ["l", "left", "lf"] {
        if token_bounded(n, alt) {
            return Some('L');
        }
    }
    for alt in ["r", "right", "rt"] {
        if token_bounded(n, alt) {
            return Some('R');
        }
    }
    if n.contains("left") {
        return Some('L');
    }
    if n.contains("right") {
        return Some('R');
    }
    // side glued onto the role token: eyeL_bone, browOutR_bone, handl.
    // /(eye|eyelid|lid|brow|cheek|nose|hand|foot|arm|leg|thigh|calf|toe)(l|r)(?![a-z])/
    const ROLES: [&str; 13] = [
        "eye", "eyelid", "lid", "brow", "cheek", "nose", "hand", "foot", "arm", "leg",
        "thigh", "calf", "toe",
    ];
    let b = n.as_bytes();
    for pos in 0..b.len() {
        for role in ROLES {
            let rb = role.as_bytes();
            if pos + rb.len() < b.len() && &b[pos..pos + rb.len()] == rb {
                let j = pos + rb.len();
                let sc = b[j];
                if sc == b'l' || sc == b'r' {
                    let next_ok = j + 1 == b.len() || !b[j + 1].is_ascii_lowercase();
                    if next_ok {
                        return Some(if sc == b'l' { 'L' } else { 'R' });
                    }
                }
            }
        }
    }
    None
}

/// Segment index 0..2 for a finger bone — the digits FOLLOWING the finger keyword
/// (`IndexFinger01_L` → 0), never the trailing joint ordinal.
fn finger_seg(n: &str, role: &str) -> u32 {
    let keys: &[&str] = if role == "pinky" {
        &["pinky", "little"]
    } else {
        std::slice::from_ref(&role)
    };
    // leftmost keyword occurrence, then the first digit run after it.
    let mut best: Option<usize> = None;
    for k in keys {
        if let Some(p) = n.find(k) {
            let after = p + k.len();
            best = Some(best.map_or(after, |b| b.min(after)));
        }
    }
    let Some(mut i) = best else { return 0 };
    let b = n.as_bytes();
    while i < b.len() && !b[i].is_ascii_digit() {
        i += 1;
    }
    if i >= b.len() {
        return 0;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let v: i64 = n[start..i].parse().unwrap_or(1);
    (v - 1).clamp(0, 2) as u32
}

// ---- individual regex-pattern ports ----------------------------------------------------

/// `ball(?!oon)` — "ball" not followed by "oon".
fn ball_not_oon(n: &str) -> bool {
    let b = n.as_bytes();
    let mut i = 0;
    while i + 4 <= b.len() {
        if &b[i..i + 4] == b"ball" && !n[i + 4..].starts_with("oon") {
            return true;
        }
        i += 1;
    }
    false
}

/// `root(?!joint)` — "root" not followed by "joint".
fn root_not_joint(n: &str) -> bool {
    let mut from = 0;
    while let Some(p) = n[from..].find("root") {
        let idx = from + p;
        if !n[idx + 4..].starts_with("joint") {
            return true;
        }
        from = idx + 4;
    }
    false
}

/// A twist bone is a pure HELPER unless it is a FOREARM twist, which carries the forearm's skin and
/// must map to the forearm — otherwise that geometry is left un-reposed and the arm skews.
///
/// The old form matched the regex `twist(?!.*forearm)` literally, checking only the text AFTER
/// "twist". That is order-dependent, and Unreal names forearm twists `ForeArmTwist01` with the limb
/// BEFORE "twist", so they slipped through as helpers, dropped the forearm skin, and zig-zagged the
/// arm at bind. Check the WHOLE name instead: any twist that names the forearm (`forearm` /
/// `lowerarm`) is a forearm twist, not a helper, whichever side of "twist" the token sits on.
fn twist_not_forearm(n: &str) -> bool {
    n.contains("twist") && !n.contains("forearm") && !n.contains("lowerarm")
}

/// `hip(?=.*\d)` — "hip" followed somewhere later by a digit.
fn hip_then_digit(n: &str) -> bool {
    let mut from = 0;
    while let Some(p) = n[from..].find("hip") {
        let idx = from + p;
        if n[idx + 3..].bytes().any(|c| c.is_ascii_digit()) {
            return true;
        }
        from = idx + 3;
    }
    false
}

/// `PREFIX _? \d* _? (l|r)? $` anchored to the string end (used by leg / arm chain patterns).
/// `no_fore` rejects a `PREFIX` immediately preceded by "fore" (`(?<!fore)arm…`).
fn limb_end(n: &str, prefix: &str, no_fore: bool) -> bool {
    let b = n.as_bytes();
    let pb = prefix.as_bytes();
    let mut i = 0;
    while i + pb.len() <= b.len() {
        if &b[i..i + pb.len()] == pb {
            let fore_ok = !no_fore || i < 4 || &n[i - 4..i] != "fore";
            if fore_ok {
                let mut j = i + pb.len();
                // _?
                if j < b.len() && b[j] == b'_' {
                    j += 1;
                }
                // \d*
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                // _?
                if j < b.len() && b[j] == b'_' {
                    j += 1;
                }
                // (l|r)?
                if j < b.len() && (b[j] == b'l' || b[j] == b'r') {
                    j += 1;
                }
                if j == b.len() {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// `contains any of` helper.
fn any(n: &str, subs: &[&str]) -> bool {
    subs.iter().any(|s| n.contains(s))
}

/// Does the role's ROLE_PATTERN match `n`? (only the roles used as chain-blockers plus the
/// ones consulted by `classify`'s main loop are needed; this is the single source of truth.)
fn role_matches(role: &str, n: &str) -> bool {
    match role {
        "toe" => n.contains("toe") || ball_not_oon(n),
        "foot" => any(n, &["foot", "ankle"]),
        "calf" => {
            any(n, &["calf", "shin", "lowerleg", "lowleg"]) || limb_end(n, "leg", false)
        }
        "thigh" => any(n, &["thigh", "upleg", "upperleg", "femur"]) || hip_then_digit(n),
        "clavicle" => any(n, &["clavicle", "collar", "shoulder"]),
        "forearmroll" => any(n, &["forearmtwist", "forearmroll", "lowerarmtwist"]),
        "forearm" => any(n, &["forearm", "lowerarm", "elbow"]),
        "upperarm" => {
            any(n, &["upperarm", "bicep", "humerus"]) || limb_end(n, "arm", true)
        }
        "hand" => any(n, &["hand", "wrist", "palm"]),
        "head" => any(n, &["head", "skull"]),
        "neck" => n.contains("neck"),
        "jaw" => any(n, &["jaw", "chin"]),
        "tongue" => n.contains("tongue"),
        "eyelid" => any(n, &["eyelid", "lid"]),
        "brow" => n.contains("brow"),
        "eye" => any(n, &["eye", "pupil"]),
        "cheek" => n.contains("cheek"),
        "nose" => any(n, &["nose", "nostril"]),
        "chest" => any(n, &["chest", "ribcage", "upperchest", "thorax"]),
        "spine" => any(n, &["spine", "torso", "abdomen", "waist"]),
        "hips" => any(n, &["hips", "pelvis", "cog"]) || root_not_joint(n),
        _ => false,
    }
}

/// ROLE_PATTERNS order — specific before generic (first match wins).
const ROLE_ORDER: [&str; 21] = [
    "toe", "foot", "calf", "thigh", "clavicle", "forearmroll", "forearm", "upperarm",
    "hand", "head", "neck", "jaw", "tongue", "eyelid", "brow", "eye", "cheek", "nose",
    "chest", "spine", "hips",
];

/// `HELPER` regex — pure rig helpers with no Mercs2 counterpart.
/// `/muscle|lookat|twist(?!.*forearm)|camera|objectoffset|gunbone|ik|pole|target|scale/`
fn is_helper(n: &str) -> bool {
    any(
        n,
        &[
            "muscle",
            "lookat",
            "camera",
            "objectoffset",
            "gunbone",
            "ik",
            "pole",
            "target",
            "scale",
        ],
    ) || twist_not_forearm(n)
}

/// Classify one joint name → `(class, reason)`. Faithful port of `classify()`.
pub fn classify(name: &str) -> (Cls, String) {
    let n = norm(name);
    if is_helper(&n) {
        return (Cls::None, "helper".into());
    }
    let s = side_of(&n);
    // fingers first (specific)
    for role in FINGER_ROLES {
        let hit = if role == "pinky" {
            n.contains("pinky") || n.contains("little")
        } else {
            n.contains(role)
        };
        if hit {
            let Some(side) = s else {
                return (Cls::None, format!("{role} without a side"));
            };
            let base = finger_base(role).unwrap()[if side == 'L' { 0 } else { 1 }];
            let seg = finger_seg(&n, role);
            return (Cls::Hier(base + seg), format!("{role}{}.{side}", seg + 1));
        }
    }
    // generic NUMBERED limb chains — only if no specific segment already matched.
    let blocked = CHAIN_BLOCKERS.iter().any(|r| role_matches(r, &n));
    if !blocked {
        if token_bounded(&n, "neck") {
            return (Cls::Neck, "neck chain".into());
        }
        if token_bounded(&n, "leg") {
            if let Some(side) = s {
                return (
                    if side == 'L' { Cls::LegL } else { Cls::LegR },
                    "leg chain".into(),
                );
            }
        }
        if token_bounded(&n, "arm") {
            if let Some(side) = s {
                return (
                    if side == 'L' { Cls::ArmL } else { Cls::ArmR },
                    "arm chain".into(),
                );
            }
        }
    }
    for role in ROLE_ORDER {
        if role_matches(role, &n) {
            if role == "spine" {
                return (Cls::Spine, "spine(chain)".into());
            }
            if let Some(pair) = sided(role) {
                let Some(side) = s else {
                    return (Cls::None, format!("{role} without a side"));
                };
                return (Cls::Hier(pair[if side == 'L' { 0 } else { 1 }]), format!("{role}.{side}"));
            }
            if let Some(h) = core(role) {
                return (Cls::Hier(h), role.to_string());
            }
        }
    }
    (Cls::None, "unrecognised".into())
}

// ---- automap ---------------------------------------------------------------------------

/// Source rig as plain data — the caller (a glTF adapter) fills these; keeps `char_skin`
/// free of any glTF dependency. `joint_nodes[j]` is the node index of joint `j`;
/// `node_parent[node]` is its parent node (-1 = root); `node_name[node]` its name.
pub struct Rig<'a> {
    pub joint_nodes: &'a [usize],
    pub node_parent: &'a [i32],
    pub node_name: &'a [String],
}

/// Output of [`automap`]: direct + inherited joint→HIER maps, plus per-joint reason.
pub struct AutoMap {
    /// joint index → its node name (mirrors `am.names`).
    pub names: Vec<String>,
    /// direct mappings (joint → HIER).
    pub mapped: HashMap<usize, u32>,
    /// inherited-from-nearest-mapped-ancestor mappings.
    pub inherited: HashMap<usize, u32>,
    /// human-readable reason per joint (for logging).
    pub why: HashMap<usize, String>,
}

impl<'a> Rig<'a> {
    fn depth(&self, node: usize) -> i32 {
        let mut d = 0;
        let mut cur = node as i32;
        while cur >= 0 {
            let p = self.node_parent.get(cur as usize).copied().unwrap_or(-1);
            if p < 0 {
                break;
            }
            cur = p;
            d += 1;
        }
        d
    }
    fn name_of(&self, node: usize) -> String {
        self.node_name.get(node).cloned().unwrap_or_default()
    }
}

/// Map a source rig onto the 84-bone Mercs2 skeleton. Faithful port of `automap()`.
/// Does this joint set use the Call of Duty naming dialect? Keyed on its two signature roots,
/// which no other rig we map ships: the `tag_origin` export tag and the `j_mainroot` pelvis.
pub fn is_cod_rig(names: &[String]) -> bool {
    names.iter().any(|n| {
        let l = n.to_ascii_lowercase();
        l == "tag_origin" || l == "j_mainroot"
    })
}

/// Rewrite the CoD side tokens `le`/`ri` to the `l`/`r` that [`side_of`] already understands,
/// underscore-token-wise so `j_clavicle_le` -> `j_clavicle_l` but `j_spinelower` is untouched.
pub fn normalize_cod_sides(name: &str) -> String {
    name.split('_')
        .map(|t| match t.to_ascii_lowercase().as_str() {
            "le" => "l",
            "ri" => "r",
            _ => t,
        })
        .collect::<Vec<_>>()
        .join("_")
}

pub fn automap(rig: &Rig) -> AutoMap {
    let joints = rig.joint_nodes;
    let names: Vec<String> = joints.iter().map(|&n| rig.name_of(n)).collect();
    // node → joint index
    let jidx: HashMap<usize, usize> = joints.iter().enumerate().map(|(i, &n)| (n, i)).collect();

    // Dialect pre-pass. `side_of` recognises l/left/lf and r/right/rt; the Call of Duty rig
    // instead sides its joints `_le`/`_ri` (j_clavicle_le, j_hip_ri). Widening `side_of` to
    // accept those globally is NOT safe -- it re-sides joints on the crosby and vietnam parity
    // fixtures and breaks their pinned output. So detect the dialect from the rig as a whole and
    // rewrite only its side tokens, leaving every other rig's classification byte-identical.
    let cod = is_cod_rig(&names);
    let raw: Vec<(Cls, String)> = names
        .iter()
        .map(|nm| classify(&if cod { normalize_cod_sides(nm) } else { nm.clone() }))
        .collect();
    let mut mapped: HashMap<usize, u32> = HashMap::new();
    let mut why: HashMap<usize, String> = HashMap::new();

    // depth-ordered ladder assignment
    let ladder_assign = |members: &mut Vec<usize>,
                         rungs: &[u32],
                         label: &str,
                         mapped: &mut HashMap<usize, u32>,
                         why: &mut HashMap<usize, String>| {
        members.sort_by_key(|&i| rig.depth(joints[i]));
        let n = members.len();
        for (k, &i) in members.iter().enumerate() {
            let r = if n <= rungs.len() {
                rungs[k]
            } else {
                rungs[((k * rungs.len()) / n).min(rungs.len() - 1)]
            };
            mapped.insert(i, r);
            why.insert(i, format!("{label}[{}/{}]", k + 1, n));
        }
    };

    let pick = |tag: Cls| -> Vec<usize> {
        (0..raw.len()).filter(|&i| raw[i].0 == tag).collect()
    };

    // A spine-chain root that PARENTS the leg chains is really the pelvis, not spine1.
    let mut spine = pick(Cls::Spine);
    let legs: Vec<usize> = (0..raw.len())
        .filter(|&i| matches!(raw[i].0, Cls::LegL | Cls::LegR))
        .collect();
    if !spine.is_empty() && !legs.is_empty() {
        spine.sort_by_key(|&i| rig.depth(joints[i]));
        let leg_parents: std::collections::HashSet<i32> = legs
            .iter()
            .map(|&i| rig.node_parent.get(joints[i]).copied().unwrap_or(-1))
            .collect();
        if leg_parents.contains(&(joints[spine[0]] as i32)) {
            mapped.insert(spine[0], 3);
            why.insert(spine[0], "hips (spine root parents the legs)".into());
            spine.remove(0);
        }
    }
    ladder_assign(&mut spine, &[14, 15, 16], "spine", &mut mapped, &mut why);
    ladder_assign(&mut pick(Cls::Neck), &[20, 21], "neck", &mut mapped, &mut why);
    ladder_assign(&mut pick(Cls::LegL), &[6, 7, 8, 9], "leg.L", &mut mapped, &mut why);
    ladder_assign(&mut pick(Cls::LegR), &[10, 11, 12, 13], "leg.R", &mut mapped, &mut why);
    ladder_assign(&mut pick(Cls::ArmL), &[43, 44, 46], "arm.L", &mut mapped, &mut why);
    ladder_assign(&mut pick(Cls::ArmR), &[64, 65, 67], "arm.R", &mut mapped, &mut why);

    for i in 0..raw.len() {
        if let Cls::Hier(h) = raw[i].0 {
            if !mapped.contains_key(&i) {
                mapped.insert(i, h);
                why.insert(i, raw[i].1.clone());
            }
        }
    }

    // unmapped → nearest mapped ancestor (walk NODE parents)
    let mut inherited: HashMap<usize, u32> = HashMap::new();
    for i in 0..joints.len() {
        if mapped.contains_key(&i) {
            continue;
        }
        let mut cur = joints[i] as i32;
        loop {
            let p = rig.node_parent.get(cur as usize).copied().unwrap_or(-1);
            if p < 0 {
                break;
            }
            cur = p;
            if let Some(&j) = jidx.get(&(cur as usize)) {
                if let Some(&h) = mapped.get(&j) {
                    inherited.insert(i, h);
                    break;
                }
            }
        }
    }

    AutoMap {
        names,
        mapped,
        inherited,
        why,
    }
}

/// Origin of a joint's final mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Auto,
    Inherited,
    Manual,
    Dropped,
}

/// Merge automap output with manual overrides. `overrides` maps source joint → `Some(hier)`
/// or `None` (force-drop). Direct mappings win over inherited. Port of `applyOverrides`.
pub fn apply_overrides(
    am: &AutoMap,
    overrides: &HashMap<usize, Option<u32>>,
) -> (HashMap<usize, u32>, HashMap<usize, Origin>) {
    let mut full: HashMap<usize, u32> = HashMap::new();
    let mut origin: HashMap<usize, Origin> = HashMap::new();
    for (&k, &v) in &am.inherited {
        full.insert(k, v);
        origin.insert(k, Origin::Inherited);
    }
    for (&k, &v) in &am.mapped {
        full.insert(k, v); // mapped wins over inherited
        origin.insert(k, Origin::Auto);
    }
    for (&k, &v) in overrides {
        match v {
            None => {
                full.remove(&k);
                origin.insert(k, Origin::Dropped);
            }
            Some(h) => {
                full.insert(k, h);
                origin.insert(k, Origin::Manual);
            }
        }
    }
    (full, origin)
}
