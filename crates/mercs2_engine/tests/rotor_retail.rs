//! The rotor blade/blur swap (`PgRotorBoneController`, `mercs2_vehicle::rotor`) against the retail WAD
//! and a live capture of the game.
//!
//! Game-gated: built by the `retail` feature, reads the retail `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails when it is absent.

use mercs2_engine::model::Model;
use mercs2_engine::vehicle::rotor::RotorController;
use mercs2_engine::wad;
use mercs2_formats::orchestrator as orch;
use mercs2_formats::rotor::{model_rotors, rotor_records};
use mercs2_formats::worldentity::{WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};

fn open_vz_wad() -> wad::Wad {
    let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
    let path = path.to_str().unwrap_or_else(|| panic!("vz.wad path is not UTF-8: {}", path.display())).to_string();
    wad::open(&path).unwrap_or_else(|e| panic!("open {path}: {e}"))
}

fn worldentity(w: &mut wad::Wad) -> WorldEntity {
    let c = wad::extract_container_typed(w, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH)
        .unwrap_or_else(|e| panic!("worldentity: {e}"));
    WorldEntity::parse(&c).unwrap_or_else(|e| panic!("worldentity: {e}"))
}

/// The node-enable table at `OBJ+0x2A0` of the two `global_veh_uh1huey` render objects in
/// `game-files/memory-dumps/updated-chris-exports/20261021_chris-jacobs-flying-winch.dmp`, one byte per
/// HIER node: OBJ `0x1E1ACEC0` (table `0x1CBB1B60`), whose HelicopterPhysics `+0x3C` reads 1.0, and OBJ
/// `0x1E9F8000` (table `0x1CBB1D20`), whose reads 0.0.
const DUMP_TABLE_SPEED_1: &str = "111111010010001100011011000000000000000111000000000000000000000000000000011111111111111111111011001110000000000011000000000000000000000000000000";
const DUMP_TABLE_SPEED_0: &str = "111111100010001100010111000000000000000111000000000000000000000000000000011111111111111111111011001110000000000011000000000000000000000000000000";

fn bits(s: &str) -> Vec<bool> {
    s.bytes().map(|b| b == b'1').collect()
}

/// `SceneObject+0x14` of both live Hueys: the template `UH1 Transport (GR)`.
const LIVE_HUEY_TEMPLATE: u32 = 0x8000_8205;

/// The machine of `global_veh_uh1huey` run with its live template's `OSMStateParameter` records, and
/// its two rotors over it, reproduces both live tables node for node: speed 1 the flown Huey's, speed
/// 0 the parked one's. The two differ in the rotors' blur and blade nodes alone.
#[test]
fn the_huey_template_machine_and_rotors_reproduce_the_live_tables() {
    let mut w = open_vz_wad();
    let hash = mercs2_formats::hash::pandemic_hash_m2("global_veh_uh1huey");
    let m = Model::load(&mut w, hash).expect("huey loads");
    let sm = m.machine.as_ref().expect("the huey has a state machine");
    let we = worldentity(&mut w);
    let hashes: Vec<u32> = m.hier.iter().map(|n| n.hash).collect();
    let rotors = model_rotors(&we, hash, &hashes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(rotors.len(), 2, "main and tail rotor: {rotors:08X?}");
    let template = orch::template_states(&we, &[LIVE_HUEY_TEMPLATE]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(template.len(), 14, "UH1 Transport (GR) picks a state for each of the 14 kit switches");

    let table_at = |speed: f32| {
        let chosen: Vec<usize> = sm.nodes.iter().map(|n| orch::default_state_index_in(n, &template)).collect();
        let mut records = orch::machine_run_in(sm, &m.hier, &chosen, &template).unwrap_or_else(|e| panic!("{e}")).records;
        for r in &rotors {
            let mut c = RotorController::new(r);
            let shows = c.update(speed, 1.0 / 60.0, false);
            c.record(shows, &m.hier, &mut records).unwrap_or_else(|e| panic!("{e}"));
        }
        records.table(&m.hier).unwrap_or_else(|e| panic!("{e}"))
    };
    let (ours1, ours0) = (table_at(1.0), table_at(0.0));
    let (live1, live0) = (bits(DUMP_TABLE_SPEED_1), bits(DUMP_TABLE_SPEED_0));
    assert_eq!(ours1.len(), live1.len(), "one byte per HIER node");

    let differ = |a: &[bool], b: &[bool]| -> Vec<usize> { (0..a.len()).filter(|&i| a[i] != b[i]).collect() };
    let rotor_nodes: Vec<usize> = ["bone_tailbluroff", "bone_tailbluron", "bone_bluron", "bone_bluroff"]
        .iter()
        .map(|n| {
            let h = mercs2_formats::hash::pandemic_hash_m2(n);
            m.hier.iter().find(|x| x.hash == h).unwrap_or_else(|| panic!("{n}")).index
        })
        .collect();
    assert_eq!(differ(&live1, &live0), rotor_nodes, "the live tables differ in the rotor nodes alone");
    assert_eq!(differ(&ours1, &live1), Vec::<usize>::new(), "speed 1 against the flown Huey");
    assert_eq!(differ(&ours0, &live0), Vec::<usize>::new(), "speed 0 against the parked Huey");
}

/// A switch whose `InitState` targets the template state starts, with no template records, in
/// `PristineState`, so its `SetStateOnMsg(DestroyMsg)` moves it on. Started in `InitState`, the Huey's
/// 14 kit switches never left it, and a state entered on destruction never fired its enter commands.
/// (The kits vanish from a wreck either way: they hang under the intact body the wreck hides.)
#[test]
fn a_destroyed_huey_moves_its_kit_switches() {
    let mut w = open_vz_wad();
    let hash = mercs2_formats::hash::pandemic_hash_m2("global_veh_uh1huey");
    let m = Model::load(&mut w, hash).expect("huey loads");
    let sm = m.machine.as_ref().expect("the huey has a state machine");
    let we = worldentity(&mut w);
    let kits = orch::template_states(&we, &[LIVE_HUEY_TEMPLATE]).unwrap_or_else(|e| panic!("{e}"));
    let wrecked = orch::node_states_for_health(sm, 0.0, 0.99);
    for &kit in kits.keys() {
        let (i, sw) = sm.nodes.iter().enumerate().find(|(_, n)| n.name_hash == kit).unwrap_or_else(|| panic!("0x{kit:08X}"));
        assert_eq!(sw.states[orch::default_state_index(sw)].name_hash, orch::STATE_PRISTINE, "kit 0x{kit:08X} starts pristine");
        assert_eq!(sw.states[wrecked[i]].name_hash, 0x46F7_9D30, "kit 0x{kit:08X} on DestroyMsg");
    }
}

/// Every state a template's `OSMStateParameter` records pick, in every model the template names, is
/// one its switch node lists — except where it is not, and then the node stays hidden: `0x004D4010`
/// records an unlisted state and runs no script, so a `SWIT` participant is never shown.
/// `al_veh_truck_hmmwv_armored` is the case: each weapon variant sends the other weapon's mount switch
/// to a state hashed as the switch node itself, which the node does not list.
#[test]
fn template_states_are_listed_or_leave_their_node_hidden() {
    let mut w = open_vz_wad();
    let we = worldentity(&mut w);
    let templates = mercs2_formats::mixr::kit_templates(&we).unwrap_or_else(|e| panic!("{e}"));
    let (mut listed, mut unlisted, mut shown) = (0usize, 0usize, Vec::new());
    let mut models: std::collections::HashMap<u32, Option<(orch::StateMachine, Vec<orch::HierNode>)>> =
        std::collections::HashMap::new();
    for t in &templates {
        let states = orch::template_states(&we, &[t.key]).unwrap_or_else(|e| panic!("{e}"));
        if states.is_empty() {
            continue;
        }
        let model = models.entry(t.model).or_insert_with(|| {
            let c = wad::extract_container(&mut w, t.model).ok()?;
            Some((orch::parse_state_machine(&c)?, orch::parse_hier(&c)))
        });
        let Some((sm, hier)) = model else { continue };
        let chosen: Vec<usize> = sm.nodes.iter().map(|n| orch::default_state_index_in(n, &states)).collect();
        let table = orch::machine_run_in(sm, hier, &chosen, &states)
            .and_then(|r| r.records.table(hier))
            .unwrap_or_else(|e| panic!("template 0x{:08X}: {e}", t.key));
        for (&node, &state) in &states {
            let Some(sw) = sm.nodes.iter().find(|n| n.name_hash == node) else { continue };
            if sw.states.iter().any(|s| s.name_hash == state) {
                listed += 1;
                continue;
            }
            unlisted += 1;
            let at = hier.iter().find(|n| n.hash == node).map(|n| n.index);
            if at.is_none_or(|i| table[i]) {
                shown.push(format!("template 0x{:08X} model 0x{:08X} node 0x{node:08X} state 0x{state:08X}", t.key, t.model));
            }
        }
    }
    println!("template states: {listed} listed by their switch node, {unlisted} not");
    assert!(listed > 0);
    assert!(shown.is_empty(), "an unlisted template state left its node shown:\n{}", shown.join("\n"));
}

/// The helicopters are named by templates carrying `_HelicopterPhysics` — the physics whose rotor
/// speed their controllers read — through the same template keys their `ModelName` records carry.
#[test]
fn helicopter_models_are_named_by_helicopter_templates() {
    let mut w = open_vz_wad();
    let we = worldentity(&mut w);
    let heli = mercs2_formats::rotor::helicopter_keys(&we);
    let templates = mercs2_formats::mixr::kit_templates(&we).unwrap_or_else(|e| panic!("{e}"));
    for name in ["ch_veh_helicopter_ka29b", "global_veh_uh1huey"] {
        let model = mercs2_formats::hash::pandemic_hash_m2(name);
        assert!(templates.iter().any(|t| t.model == model && heli.contains(&t.key)), "{name}");
    }
}

/// A Ka-29B rotor turned on its real rig (`+80` skin binds, `+16` rest palette): the hub stays put and
/// every node under it keeps its height along the hub's own Y axis and its distance from that axis,
/// while actually moving.
#[test]
fn a_rotor_turns_its_subtree_about_the_hubs_own_axis_on_the_real_rig() {
    use mercs2_formats::skeleton::mat4_mul;
    let mut w = open_vz_wad();
    let hash = mercs2_formats::hash::pandemic_hash_m2("ch_veh_helicopter_ka29b");
    let m = Model::load(&mut w, hash).expect("ka29b loads");
    let skin = m.flatten().3.skin_data();
    assert_eq!(skin.rig.len(), m.hier.len(), "one rig bone per HIER node");
    let we = worldentity(&mut w);
    let hashes: Vec<u32> = m.hier.iter().map(|n| n.hash).collect();
    let rotors = model_rotors(&we, hash, &hashes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(rotors.len(), 2, "upper and lower rotor");
    let world = |pal: &[[[f32; 4]; 4]], b: usize| mat4_mul(&skin.rig[b].world_bind, &pal[b]);
    for r in &rotors {
        let hub = m.hier.iter().find(|n| n.hash == r.hub).unwrap().index;
        let mut subtree = vec![hub];
        for n in &m.hier {
            if n.parent.is_some_and(|q| subtree.contains(&q)) {
                subtree.push(n.index);
            }
        }
        let rest = skin.bones.clone();
        let mut pal = rest.clone();
        mercs2_engine::vehicle::rotor::spin_subtree(&mut pal, &skin.rig[hub].world_bind, &subtree, 1.0).unwrap();
        let hw = world(&rest, hub);
        let (o, y) = ([hw[3][0], hw[3][1], hw[3][2]], [hw[1][0], hw[1][1], hw[1][2]]);
        let along = |p: [f32; 3]| (0..3).map(|i| (p[i] - o[i]) * y[i]).sum::<f32>();
        let off_axis = |p: [f32; 3]| {
            let a = along(p);
            (0..3).map(|i| (p[i] - o[i] - a * y[i]).powi(2)).sum::<f32>().sqrt()
        };
        let spun_hub = world(&pal, hub);
        assert!((0..3).all(|i| (spun_hub[3][i] - o[i]).abs() < 1e-4), "hub 0x{:08X} moved", r.hub);
        let mut moved = 0;
        for &b in &subtree[1..] {
            let (p0, p1) = (world(&rest, b)[3], world(&pal, b)[3]);
            let (p0, p1) = ([p0[0], p0[1], p0[2]], [p1[0], p1[1], p1[2]]);
            assert!((along(p0) - along(p1)).abs() < 1e-3, "node {b}: height along the hub axis");
            assert!((off_axis(p0) - off_axis(p1)).abs() < 1e-3, "node {b}: distance from the hub axis");
            moved += usize::from((0..3).any(|i| (p0[i] - p1[i]).abs() > 1e-2));
        }
        assert!(moved > 0, "hub 0x{:08X}: no node of its subtree moved", r.hub);
    }
}

/// Every model a template with `Rotor` records names: its rotors resolve without two records for one
/// hub, and every rotor whose hub the model has also has the blur and blade nodes it names.
#[test]
fn every_rotor_model_resolves_its_rotors() {
    let mut w = open_vz_wad();
    let we = worldentity(&mut w);
    let rotor_keys: std::collections::BTreeSet<u32> =
        rotor_records(&we).unwrap_or_else(|e| panic!("{e}")).into_iter().flat_map(|(k, _)| k).collect();
    let models: std::collections::BTreeSet<u32> = mercs2_formats::mixr::kit_templates(&we)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .filter(|t| rotor_keys.contains(&t.key))
        .map(|t| t.model)
        .collect();
    let (mut checked, mut absent, mut with_rotors) = (0, 0, 0);
    let mut missing = Vec::new();
    for model in models {
        let Ok(c) = wad::extract_container(&mut w, model) else {
            absent += 1;
            continue;
        };
        let hier = orch::parse_hier(&c);
        let hashes: Vec<u32> = hier.iter().map(|n| n.hash).collect();
        let rotors = model_rotors(&we, model, &hashes).unwrap_or_else(|e| panic!("{e}"));
        checked += 1;
        with_rotors += usize::from(!rotors.is_empty());
        for r in rotors {
            for node in [r.blur_on, r.blur_off].into_iter().filter(|&h| h != 0) {
                if !hashes.contains(&node) {
                    missing.push(format!("model 0x{model:08X} hub 0x{:08X}: node 0x{node:08X}", r.hub));
                }
            }
        }
    }
    println!("rotor models: {checked} read, {with_rotors} with rotors, {absent} not in vz.wad");
    assert!(with_rotors >= 15, "{with_rotors} models with rotors");
    assert!(missing.is_empty(), "rotor nodes the model lacks:\n{}", missing.join("\n"));
}
