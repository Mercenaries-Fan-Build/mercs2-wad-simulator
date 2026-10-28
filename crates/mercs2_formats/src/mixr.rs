//! `MIXR` — a character kit model's part mixer.
//!
//! A kit model (the crowd and faction soldier models: one container holding several heads, bodies
//! and accessories) carries a top-level `MIXR` marker whose children name its swappable **parts**,
//! say which parts may be shown together, and list **profiles** — the part sets a spawn template
//! may draw from. A template picks a profile through its `ModelMixerProfile` component, whose
//! payload is the profile's hash.
//!
//! Layout, as the exe's `MIXR` loader reads it (relocated body at `0x02461CB0` in `.securom`,
//! reached from the model chunk dispatcher's `cmp eax,'MIXR'` at `0x024622E5`):
//!
//! ```text
//! MIXR  marker
//!   INFO  6 bytes   u16 (skipped), u16 part_count -> mixer+0, u16 profile_count -> mixer+4
//!   NAME  part_count × u32         one HIER node hash per part -> mixer+0x14
//!   EXMX  part_count² × u8         row-major; 1 = the two parts may be shown together -> mixer+0x1C
//!   PROF  profile_count × { u32 profile hash -> mixer+0x18 ; part_count × u8 allowed -> mixer+0x20 }
//! ```
//!
//! A part is shown by its node's switch entering `visible` (`0x517FB5D1`): the spawn path at
//! `0x004D03F3` walks the selector's per-part bytes and, for each set byte, sets the part node's
//! state to `visible`. Every draw group of a kit hangs under exactly one part node.
//!
//! [`Mixr::outfits`] is a port of the exe's outfit enumeration — the outer loop at `0x006A1BEC` and
//! the recursive `FUN_00460710` — which builds, per profile, the list of part bitmasks the spawn-time
//! selector chooses from. Which entry a given spawn shows is decided by that selector
//! (`FUN_004609A0`, SecuROM-virtualized); it is not modelled here.

use crate::orchestrator::{default_state_index, HierNode, StateMachine};
use crate::ucfx::{parse_ucfx_tree, UcfxNode};
use crate::ucfx_codec::{exact_len, leaf, marker, u16_at, u32_at};
use crate::worldentity::{Payload, WorldEntity};
use std::collections::BTreeMap;

/// The `visible` switch state (`pandemic_hash_m2("visible")`) a shown part's node enters.
pub const STATE_VISIBLE: u32 = 0x517F_B5D1;

/// One profile: its hash and which parts it allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub hash: u32,
    pub allowed: Vec<bool>,
}

/// A decoded `MIXR`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mixr {
    /// `INFO`'s first u16, which the loader steps over.
    pub info_word: u16,
    /// One HIER node hash per part (`NAME`).
    pub parts: Vec<u32>,
    /// `compatible[a][b]`: parts `a` and `b` may be shown together (`EXMX` byte != 0).
    pub compatible: Vec<Vec<bool>>,
    pub profiles: Vec<Profile>,
}

impl Mixr {
    /// The `MIXR` of a model container, or `None` when it carries none. An error when a `MIXR` is
    /// present but not exactly `[INFO, NAME, EXMX, PROF]` with the lengths its counts imply.
    pub fn find(container: &[u8]) -> Result<Option<Mixr>, String> {
        let roots = parse_ucfx_tree(container)?;
        let found: Vec<&UcfxNode> = roots.iter().filter(|n| &n.tag == b"MIXR").collect();
        match found[..] {
            [] => Ok(None),
            [m] => Mixr::from_node(m).map(Some),
            _ => Err(format!("{} top-level MIXR rows; a model carries at most one", found.len())),
        }
    }

    fn from_node(n: &UcfxNode) -> Result<Mixr, String> {
        let kids = marker(n, b"MIXR", "top level")?;
        let tags: Vec<String> = kids.iter().map(UcfxNode::tag_str).collect();
        if tags != ["INFO", "NAME", "EXMX", "PROF"] {
            return Err(format!("MIXR is [{}]; it is [INFO, NAME, EXMX, PROF]", tags.join(", ")));
        }
        Mixr::from_bodies(
            leaf(&kids[0], b"INFO", "MIXR")?,
            leaf(&kids[1], b"NAME", "MIXR")?,
            leaf(&kids[2], b"EXMX", "MIXR")?,
            leaf(&kids[3], b"PROF", "MIXR")?,
        )
    }

    /// Decode from the four child bodies.
    pub fn from_bodies(info: &[u8], name: &[u8], exmx: &[u8], prof: &[u8]) -> Result<Mixr, String> {
        exact_len(info, 6, "MIXR INFO")?;
        let (info_word, n, p) = (u16_at(info, 0), u16_at(info, 2) as usize, u16_at(info, 4) as usize);
        // The enumerator keeps part sets as `1 << part` in one u32.
        if n > 32 {
            return Err(format!("MIXR INFO counts {n} parts; the exe's part masks hold 32"));
        }
        exact_len(name, 4 * n, &format!("MIXR NAME of {n} parts"))?;
        exact_len(exmx, n * n, &format!("MIXR EXMX of {n}x{n} parts"))?;
        exact_len(prof, p * (4 + n), &format!("MIXR PROF of {p} profiles over {n} parts"))?;
        let parts = (0..n).map(|i| u32_at(name, 4 * i)).collect();
        let compatible = (0..n).map(|a| (0..n).map(|b| exmx[a * n + b] != 0).collect()).collect();
        let profiles = (0..p)
            .map(|k| {
                let r = &prof[k * (4 + n)..(k + 1) * (4 + n)];
                Profile { hash: u32_at(r, 0), allowed: r[4..].iter().map(|&b| b != 0).collect() }
            })
            .collect();
        Ok(Mixr { info_word, parts, compatible, profiles })
    }

    /// The index of the profile with this hash.
    pub fn profile_index(&self, hash: u32) -> Option<usize> {
        self.profiles.iter().position(|p| p.hash == hash)
    }

    /// Every outfit the exe enumerates for profile `profile` (an index into [`Self::profiles`]), as
    /// part bitmasks (`1 << part`), in the order the exe stores them. Empty when the profile allows
    /// no part (the exe makes no call then).
    pub fn outfits(&self, profile: usize) -> Vec<u32> {
        let n = self.parts.len();
        // Outer loop (0x006A1C03): start from every bit set, clear the parts the profile disallows,
        // pivot on the lowest allowed part.
        let mut avail = u32::MAX;
        for (c, &ok) in self.profiles[profile].allowed.iter().enumerate() {
            if !ok {
                avail ^= 1 << c;
            }
        }
        let mut out = Vec::new();
        if let Some(pivot) = (0..n).find(|&c| avail & (1 << c) != 0) {
            self.enumerate(avail, 0, pivot, &mut out);
        }
        out
    }

    /// `FUN_00460710(avail, chosen, pivot, profile, top)`: collect the pivot's exclusion class from
    /// the available parts, then branch on each member.
    fn enumerate(&self, avail: u32, chosen: u32, pivot: usize, out: &mut Vec<u32>) {
        let n = self.parts.len();
        if pivot >= n {
            return;
        }
        let mut avail = avail;
        let mut class: Vec<usize> = Vec::new();
        let mut next: Option<usize> = None;
        for i in 0..n {
            let bit = 1u32 << i;
            if avail & bit == 0 {
                continue;
            }
            if !self.compatible[pivot][i] {
                // A class member is incompatible with the pivot and with every member so far.
                if class.iter().rev().any(|&j| self.compatible[j][i]) {
                    continue;
                }
                class.push(i);
                avail &= !bit;
            } else if next.is_none() {
                next = Some(i);
            }
        }
        if class.is_empty() {
            // 0x004607DC: the pivot's bit is added whether or not it was available.
            out.push(chosen | (1 << pivot));
            return;
        }
        for &c in &class {
            let worn = chosen | (1 << c);
            let mut rest = avail;
            for k in 0..n {
                if !self.compatible[c][k] {
                    rest &= !(1 << k);
                }
            }
            match next {
                Some(p) => self.enumerate(rest, worn, p, out),
                None => out.push(worn),
            }
        }
    }

    /// The parts of an outfit bitmask, as indices.
    pub fn parts_of(&self, outfit: u32) -> Vec<usize> {
        (0..self.parts.len()).filter(|&i| outfit & (1 << i) != 0).collect()
    }

    /// A template's profile index and that profile's outfits, in the exe's order. An error when the
    /// template's `ModelMixerProfile` names no profile of this kit (retail's `OC Soldier (Bench Press)`
    /// carries the allied kit's).
    pub fn template_outfits(&self, t: &KitTemplate) -> Result<(usize, Vec<u32>), String> {
        let pi = self.profile_index(t.profile).ok_or_else(|| {
            format!("template {:?} picks profile 0x{:08X}, which this kit does not carry", t.label(), t.profile)
        })?;
        Ok((pi, self.outfits(pi)))
    }

    /// Outfit number `n` — counted from 1, as `--render-outfit "<template>#n"` takes it — as one
    /// listing line: `#n: parts [..]`.
    pub fn outfit_line(&self, n: usize, outfit: u32) -> String {
        format!("#{n}: parts {:?}", self.parts_of(outfit))
    }

    /// The state index per switch node of `sm` that shows exactly `outfit`: the node's `visible`
    /// state for a worn part, its default state otherwise. Mirrors the spawn loop at `0x004D03F3`,
    /// which looks each worn part's node up in the model's HIER and, when found, sets that node's
    /// state to `visible`; a worn part whose node is not in the HIER is passed over there too.
    /// An error when a worn part's node is in the HIER but no switch node of `sm` gives it a
    /// `visible` state — the exe's setter for that case is not read.
    pub fn outfit_node_states(&self, sm: &StateMachine, hier: &[HierNode], outfit: u32) -> Result<Vec<usize>, String> {
        let mut chosen: Vec<usize> = sm.nodes.iter().map(default_state_index).collect();
        for p in self.parts_of(outfit) {
            let node = self.parts[p];
            if !hier.iter().any(|h| h.hash == node) {
                continue;
            }
            let slot = sm
                .nodes
                .iter()
                .position(|n| n.name_hash == node)
                .ok_or_else(|| format!("part {p} node 0x{node:08X} is in the HIER but no switch node controls it"))?;
            chosen[slot] = sm.nodes[slot]
                .states
                .iter()
                .position(|s| s.name_hash == STATE_VISIBLE)
                .ok_or_else(|| format!("part {p} node 0x{node:08X}'s switch has no visible state"))?;
        }
        Ok(chosen)
    }
}

/// One template of the resident `worldentity` that names a model: its key, `Name`, `ModelName`,
/// `ModelMixerProfile` (0 when it carries none) and `FactionMarker`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitTemplate {
    pub key: u32,
    pub name: Option<String>,
    pub model: u32,
    pub profile: u32,
    /// The template's `FactionMarker`: `pandemic_hash_m2` of the faction's HUD icon name
    /// (`HUD_faction_AN`, `_CH`, `_CV`, `_GR`, `_OC`, `_PR`, `_VZ` in retail). It belongs to the
    /// template, not the model: one kit model is worn by templates of different factions.
    pub faction: Option<u32>,
}

impl KitTemplate {
    /// The template's `Name`, or its key in hex when it has none.
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| format!("0x{:08X}", self.key))
    }
}

/// Every template with a `ModelName`, in key order. An error when a `ModelName`,
/// `ModelMixerProfile` or `FactionMarker` payload is not one u32, or a key carries two different
/// values of one of them.
pub fn kit_templates(we: &WorldEntity) -> Result<Vec<KitTemplate>, String> {
    let u32_values = |class: &str| -> Result<BTreeMap<u32, u32>, String> {
        let mut out: BTreeMap<u32, u32> = BTreeMap::new();
        for c in we.groups_of(class) {
            for r in &c.records {
                let v = match &r.payload {
                    Payload::Fixed(b) if b.len() == 4 => u32_at(b, 0),
                    p => return Err(format!("{class}: payload {p:?} is not one u32")),
                };
                for &k in &r.keys {
                    if let Some(old) = out.insert(k, v) {
                        if old != v {
                            return Err(format!("{class}: key 0x{k:08X} has 0x{old:08X} and 0x{v:08X}"));
                        }
                    }
                }
            }
        }
        Ok(out)
    };
    let models = u32_values("ModelName")?;
    let profiles = u32_values("ModelMixerProfile")?;
    let factions = u32_values("FactionMarker")?;
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    for (n, keys) in we.names()? {
        for &k in keys {
            names.insert(k, n.clone());
        }
    }
    Ok(models
        .iter()
        .map(|(&key, &model)| KitTemplate {
            key,
            name: names.get(&key).cloned(),
            model,
            profile: profiles.get(&key).copied().unwrap_or(0),
            faction: factions.get(&key).copied(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The allied elite kit 0x4D4F42B5's MIXR bodies, copied from retail vz.wad (block c31659).
    // Parts: 0-4 heads, 5/6 upper bodies, 7 lower body, 8/9 and 11 gear, 10 glasses, 12 helmet,
    // 13 a node with no geometry.
    const INFO: [u8; 6] = [0x01, 0x00, 0x0E, 0x00, 0x02, 0x00];
    const NAME: [u32; 14] = [
        0x0B7460D1, 0x720AA71A, 0xCC95CB33, 0xCA70CB6A, 0xEE710F17, 0x43D76F47, 0x19D96BC0, 0x7FAF4D16, 0x641CE36F,
        0x5A1F1248, 0xF822EED0, 0x19AA2340, 0x9D64FBAD, 0x244DDF2E,
    ];
    const EXMX: [[u8; 14]; 14] = [
        [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 1],
        [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1],
        [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0],
    ];
    const JUNGLE: (u32, [u8; 14]) = (0x351013C5, [0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1]);
    const NIGHT_VISION: (u32, [u8; 14]) = (0xF79D3014, [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1]);

    fn elite() -> Mixr {
        let name: Vec<u8> = NAME.iter().flat_map(|h| h.to_le_bytes()).collect();
        let exmx: Vec<u8> = EXMX.iter().flatten().copied().collect();
        let mut prof = Vec::new();
        for (h, flags) in [JUNGLE, NIGHT_VISION] {
            prof.extend_from_slice(&h.to_le_bytes());
            prof.extend_from_slice(&flags);
        }
        Mixr::from_bodies(&INFO, &name, &exmx, &prof).expect("decode")
    }

    #[test]
    fn decodes_the_elite_kit() {
        let m = elite();
        assert_eq!(m.parts.len(), 14);
        assert_eq!(m.profiles.len(), 2);
        assert_eq!(m.profile_index(0xF79D3014), Some(1));
        assert!(m.compatible[10][12] && !m.compatible[11][12] && !m.compatible[3][3]);
    }

    #[test]
    fn jungle_elite_outfits() {
        // One head (3), one of the two upper bodies, the lower body, one of 8/9, glasses, helmet, 13.
        let m = elite();
        let got: Vec<Vec<usize>> = m.outfits(0).iter().map(|&o| m.parts_of(o)).collect();
        assert_eq!(
            got,
            vec![
                vec![3, 5, 7, 8, 10, 12, 13],
                vec![3, 5, 7, 9, 10, 12, 13],
                vec![3, 6, 7, 8, 10, 12, 13],
                vec![3, 6, 7, 9, 10, 12, 13],
            ]
        );
    }

    #[test]
    fn a_template_resolves_to_its_profile_outfits() {
        let m = elite();
        let jungle = KitTemplate { key: 0x8000_6E40, name: Some("Jungle Elite".into()), model: 0x4D4F_42B5, profile: 0x3510_13C5, faction: None };
        let (pi, outfits) = m.template_outfits(&jungle).unwrap();
        assert_eq!((pi, outfits.len()), (0, 4));
        assert_eq!(m.outfit_line(1, outfits[0]), "#1: parts [3, 5, 7, 8, 10, 12, 13]");

        let stray = KitTemplate { key: 0x1234, name: None, model: 0x4D4F_42B5, profile: 0xDEAD_BEEF, faction: None };
        assert_eq!(stray.label(), "0x00001234");
        assert_eq!(
            m.template_outfits(&stray).unwrap_err(),
            "template \"0x00001234\" picks profile 0xDEADBEEF, which this kit does not carry"
        );
    }

    #[test]
    fn night_vision_elite_outfits() {
        // Any of the five heads × two upper bodies × two gear pieces, always with the lower body,
        // the night-vision piece 11 and 13 — and never the glasses (10) or helmet (12).
        let m = elite();
        let got = m.outfits(1);
        assert_eq!(got.len(), 20);
        for o in &got {
            let p = m.parts_of(*o);
            assert_eq!(p.iter().filter(|&&i| i <= 4).count(), 1, "{p:?}");
            assert!(p.contains(&7) && p.contains(&11) && p.contains(&13), "{p:?}");
            assert!(!p.contains(&10) && !p.contains(&12), "{p:?}");
        }
        assert_eq!(m.parts_of(got[0]), vec![0, 5, 7, 8, 11, 13]);
    }

    fn hier_of(hashes: &[u32]) -> Vec<HierNode> {
        hashes
            .iter()
            .enumerate()
            .map(|(index, &hash)| HierNode {
                index,
                hash,
                parent: None,
                local: [0.0; 16],
                bbox_min: [0.0; 3],
                bbox_max: [0.0; 3],
            })
            .collect()
    }

    /// Each kit switch node has the states `[0x0ACE072A, visible]` with no init script (retail elite kit).
    fn kit_machine(nodes: &[u32]) -> StateMachine {
        use crate::orchestrator::{StateDef, SwitchNodeDef};
        StateMachine {
            switch_slots: nodes.to_vec(),
            nodes: nodes
                .iter()
                .map(|&n| SwitchNodeDef {
                    name_hash: n,
                    states: vec![
                        StateDef { name_hash: 0x0ACE_072A, ..Default::default() },
                        StateDef { name_hash: STATE_VISIBLE, ..Default::default() },
                    ],
                })
                .collect(),
        }
    }

    #[test]
    fn outfit_node_states_show_the_worn_parts() {
        let m = elite();
        // The 13 real part nodes have switches and HIER nodes; part 13 (0x244DDF2E) has neither.
        let sm = kit_machine(&NAME[..13]);
        let hier = hier_of(&NAME[..13]);
        let outfit = m.outfits(0)[0]; // [3, 5, 7, 8, 10, 12, 13]
        let chosen = m.outfit_node_states(&sm, &hier, outfit).expect("states");
        let visible: Vec<usize> = (0..chosen.len()).filter(|&i| chosen[i] == 1).collect();
        assert_eq!(visible, vec![3, 5, 7, 8, 10, 12]);
    }

    #[test]
    fn outfit_node_states_refuse_a_part_without_a_switch() {
        let m = elite();
        let sm = kit_machine(&NAME[1..13]); // part 0's node lost its switch
        let hier = hier_of(&NAME[..13]);
        let err = m.outfit_node_states(&sm, &hier, 1 << 0).unwrap_err();
        assert!(err.contains("no switch node"), "{err}");
    }

    #[test]
    fn refuses_a_wrong_length() {
        let err = Mixr::from_bodies(&INFO, &[0; 52], &[0; 196], &[0; 36]).unwrap_err();
        assert!(err.contains("NAME of 14 parts"), "{err}");
    }
}
