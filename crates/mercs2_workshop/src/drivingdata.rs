//! What the Driving page reads from a level's worldentity and a model's hierarchy: a car template's
//! physics record by field name, where each value sits among every car template, the template's
//! wheels, the trailer templates, and where a model's nodes are.
//!
//! The worldentity schema stores a field's name hash only, so the names are here. They are the
//! strings the PC executable's loaders pass to its field reader: `_CarPhysicsV2` in `FUN_00658f60`,
//! `_CarWheel` at `0x6599c0`.

use crate::drivingmath::{Axles, Gearbox, Steering};
use glam::{Mat4, Quat};
use mercs2_engine::fx_sim::Starter;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::orchestrator::HierNode;
use mercs2_formats::schema::FieldValue;
use mercs2_formats::worldentity::{Component, Record, Value, WorldEntity};
use std::collections::{BTreeMap, HashMap};

pub const CAR_CLASS: &str = "_CarPhysicsV2";
pub const WHEEL_CLASS: &str = "_CarWheel";

/// Every `_CarPhysicsV2` field, in record order.
pub const CAR_FIELDS: [&str; 97] = [
    "Mass",
    "SteerMaxAngle",
    "SteerMaxVisualAngle",
    "SteerSpeed",
    "SteeringInertia",
    "SteerNoInputInertia",
    "SteerDiffSignInertia",
    "LowSpeedSteerSpeed",
    "LowSpeedLimit",
    "HighSpeedSteerLoss",
    "HighSpeedSteerLossBrakeMulti",
    "HighSpeedSteerLossNoThrottleMulti",
    "LinearDampening",
    "AngDampeningX",
    "AngDampeningY",
    "AngDampeningZ",
    "MaxSpeed",
    "EngineTorque",
    "HiSpeedGrip",
    "ReverseMaxSpeed",
    "FrontWheelRadius",
    "FrontWheelSuspStrength",
    "FrontWheelSuspDamp",
    "FrontWheelSuspExpLength",
    "FrontWheelSuspCmpLength",
    "FrontWheelFrictionFwd",
    "FrontWheelFrictionSide",
    "FrontEmergencyBrake",
    "FrontBrake",
    "FrontRev180FrictionMulti",
    "RearWheelRadius",
    "RearWheelSuspStrength",
    "RearWheelSuspDamp",
    "RearWheelSuspExpLength",
    "RearWheelSuspCmpLength",
    "RearWheelFrictionFwd",
    "RearWheelFrictionSide",
    "RearEmergencyBrake",
    "RearBrake",
    "RearRev180FrictionMulti",
    "GearRatioPrimary",
    "GearRatioReverse",
    "GearRatio1",
    "GearRatio2",
    "GearRatio3",
    "GearRatio4",
    "GearRatio5",
    "GearTorqueMultiReverse",
    "GearTorqueMulti1",
    "GearTorqueMulti2",
    "GearTorqueMulti3",
    "GearTorqueMulti4",
    "GearTorqueMulti5",
    "GearMaxRPM",
    "GearMinRPM",
    "GearOptRPM",
    "GearUpshiftRPM",
    "GearDownshiftRPM",
    "GearSlipRPM",
    "TorqueFactorAtMinRPM",
    "torqueFactorAtMaxRPM",
    "ClutchDelayTime",
    "RearToFrontTorqueDistrib",
    "DonutSidePower",
    "DonutBoost",
    "HandBrakeExtraBoost",
    "GravityFactor",
    "ExtraInvisibleSpringLength",
    "SuspSteerSwayAmp",
    "InertiaTensorX",
    "InertiaTensorY",
    "InertiaTensorZ",
    "CenterOfMassOffset",
    "LowSpeedSteerBoost",
    "BikeRollAngVelForMaxRoll",
    "BikeRollAngleAtLoSpeed",
    "BikeRollAngleAtHiSpeed",
    "BikeRollAngleFromInput",
    "BikeRollStabilityStrength",
    "BikeWheeliePitch",
    "BikeWheeliePitchStrengthUp",
    "BikeWheeliePitchStrengthDown",
    "BikeWheeliePitchHiSpeedRatio",
    "BikeWheelieSteerAbility",
    "BikeWheelieSideDamp",
    "BikeExtraDampRoll",
    "BikeExtraDampPitch",
    "BikeExtraDampYaw",
    "BikeAirControlYaw",
    "BikeAirControlPitch",
    "CanCrush",
    "UseIkarugaShape",
    "MaxSlope",
    "PitAttack",
    "PitDefence",
    "PitNearWheelFrictionMulit",
    "PitFarWheelFrictionMulit",
];

/// Every `_CarWheel` field, in record order.
pub const WHEEL_FIELDS: [&str; 5] = ["SuspBone", "VisYawBone", "VisRotBone", "VisPosBone", "Axle"];

/// The `_CarPhysicsV2` fields no code of the PC executable reads. The constructor `FUN_00449460`
/// and the gearbox copy `FUN_0044fc10` give each field's place in the car actor; none of these has
/// a reader among the car actor's functions (`0x437000`–`0x451000`). `torqueFactorAtMaxRPM` is
/// never read by name: its slot is filled from `TorqueFactorAtMinRPM` (`0x6594F8`).
pub const UNREAD: [&str; 9] = [
    "GearTorqueMultiReverse",
    "GearTorqueMulti1",
    "GearTorqueMulti2",
    "GearTorqueMulti3",
    "GearTorqueMulti4",
    "GearTorqueMulti5",
    "torqueFactorAtMaxRPM",
    "SteerMaxVisualAngle",
    "UseIkarugaShape",
];

/// The name of each of a class's schema fields, in schema order. An error for a field whose hash
/// none of `names` has.
fn field_names(comp: &Component, names: &[&'static str]) -> Result<Vec<&'static str>, String> {
    let by_hash: HashMap<u32, &'static str> = names.iter().map(|n| (pandemic_hash_m2(n), *n)).collect();
    comp.schema
        .fields
        .iter()
        .map(|f| {
            by_hash.get(&f.name_hash).copied().ok_or_else(|| {
                format!("{}: schema field 0x{:08X} has no name in the Driving page's field table", comp.class, f.name_hash)
            })
        })
        .collect()
}

/// A record's values by field name.
fn named_values(comp: &Component, r: &Record, names: &[&'static str]) -> Result<BTreeMap<&'static str, FieldValue>, String> {
    let values = comp.decode(&r.payload)?;
    if values.len() != names.len() {
        return Err(format!("{}: a record of {} values against {} schema fields", comp.class, values.len(), names.len()));
    }
    names
        .iter()
        .zip(values)
        .map(|(name, v)| match v {
            Value::Field(v) => Ok((*name, v)),
            Value::Text(_) => Err(format!("{}.{name}: text where a field value is expected", comp.class)),
        })
        .collect()
}

/// One template's `_CarPhysicsV2` record by field name.
#[derive(Clone, Debug, PartialEq)]
pub struct CarFields {
    values: BTreeMap<&'static str, FieldValue>,
}

impl CarFields {
    fn value(&self, name: &str) -> Result<FieldValue, String> {
        self.values.get(name).copied().ok_or_else(|| format!("{CAR_CLASS} has no field {name}"))
    }

    /// A float field.
    pub fn num(&self, name: &str) -> Result<f32, String> {
        match self.value(name)? {
            FieldValue::F32(v) => Ok(v),
            other => Err(format!("{CAR_CLASS}.{name} is {other:?}, not a float")),
        }
    }

    /// A three-float field.
    pub fn vec3(&self, name: &str) -> Result<[f32; 3], String> {
        match self.value(name)? {
            FieldValue::Vec3(v) => Ok(v),
            other => Err(format!("{CAR_CLASS}.{name} is {other:?}, not three floats")),
        }
    }

    /// The gearbox and torque-curve fields.
    pub fn gearbox(&self) -> Result<Gearbox, String> {
        Ok(Gearbox {
            primary: self.num("GearRatioPrimary")?,
            reverse: self.num("GearRatioReverse")?,
            forward: [
                self.num("GearRatio1")?,
                self.num("GearRatio2")?,
                self.num("GearRatio3")?,
                self.num("GearRatio4")?,
                self.num("GearRatio5")?,
            ],
            upshift_rpm: self.num("GearUpshiftRPM")?,
            downshift_rpm: self.num("GearDownshiftRPM")?,
            min_rpm: self.num("GearMinRPM")?,
            opt_rpm: self.num("GearOptRPM")?,
            max_rpm: self.num("GearMaxRPM")?,
            engine_torque: self.num("EngineTorque")?,
            torque_factor: self.num("TorqueFactorAtMinRPM")?,
            rear_share: self.num("RearToFrontTorqueDistrib")?,
            max_speed: self.num("MaxSpeed")?,
        })
    }

    /// The steering-lock fields.
    pub fn steering(&self) -> Result<Steering, String> {
        Ok(Steering {
            max_angle: self.num("SteerMaxAngle")?,
            high_speed_loss: self.num("HighSpeedSteerLoss")?,
            loss_brake_multi: self.num("HighSpeedSteerLossBrakeMulti")?,
            loss_no_throttle_multi: self.num("HighSpeedSteerLossNoThrottleMulti")?,
            max_speed: self.num("MaxSpeed")?,
        })
    }

    /// The wheel counts of a template's axles with the radii of this record.
    pub fn axles(&self, wheels: &[Wheel]) -> Result<Axles, String> {
        Ok(Axles {
            front_wheels: wheels.iter().filter(|w| !w.rear).count(),
            rear_wheels: wheels.iter().filter(|w| w.rear).count(),
            front_radius: self.num("FrontWheelRadius")?,
            rear_radius: self.num("RearWheelRadius")?,
        })
    }
}

/// The records of `class` that `key` shares, with their component.
fn records_of<'a>(we: &'a WorldEntity, class: &'a str, key: u32) -> impl Iterator<Item = (&'a Component, &'a Record)> + 'a {
    we.groups_of(class).flat_map(move |c| c.records.iter().filter(move |r| r.keys.contains(&key)).map(move |r| (c, r)))
}

/// A template's car physics by field name; `None` for a template that has no `_CarPhysicsV2`
/// record. An error for a template with more than one.
pub fn car_fields(we: &WorldEntity, key: u32) -> Result<Option<CarFields>, String> {
    let mut found = records_of(we, CAR_CLASS, key);
    let Some((comp, r)) = found.next() else { return Ok(None) };
    if found.next().is_some() {
        return Err(format!("template 0x{key:08X} has more than one {CAR_CLASS} record"));
    }
    let names = field_names(comp, &CAR_FIELDS)?;
    Ok(Some(CarFields { values: named_values(comp, r, &names)? }))
}

/// One wheel of a template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wheel {
    /// The node the suspension ray starts at.
    pub susp_bone: u32,
    vis_yaw_bone: u32,
    /// `Axle`: 0 is the front axle.
    pub rear: bool,
}

impl Wheel {
    /// The node that turns with the steering: `VisYawBone`, or the suspension node when it is 0
    /// (`FUN_0044df60`: `mov ecx,[eax+4]; test ecx,ecx; jne; mov ecx,[eax]` at `0x44e0a5`).
    pub fn yaw_bone(&self) -> u32 {
        if self.vis_yaw_bone == 0 { self.susp_bone } else { self.vis_yaw_bone }
    }
}

fn word(class: &str, values: &BTreeMap<&'static str, FieldValue>, name: &str) -> Result<u32, String> {
    match values.get(name) {
        Some(FieldValue::U32(v)) | Some(FieldValue::Bits(v)) => Ok(*v),
        Some(FieldValue::U8(v)) => Ok(u32::from(*v)),
        Some(FieldValue::U16(v)) => Ok(u32::from(*v)),
        Some(other) => Err(format!("{class}.{name} is {other:?}, not a word")),
        None => Err(format!("{class} has no field {name}")),
    }
}

/// A template's wheels, in record order.
pub fn wheels(we: &WorldEntity, key: u32) -> Result<Vec<Wheel>, String> {
    records_of(we, WHEEL_CLASS, key)
        .map(|(comp, r)| {
            let values = named_values(comp, r, &field_names(comp, &WHEEL_FIELDS)?)?;
            Ok(Wheel {
                susp_bone: word(WHEEL_CLASS, &values, "SuspBone")?,
                vis_yaw_bone: word(WHEEL_CLASS, &values, "VisYawBone")?,
                rear: word(WHEEL_CLASS, &values, "Axle")? != 0,
            })
        })
        .collect()
}

/// Where each float field's values run across every car template, and which templates are
/// trailers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fleet {
    /// How many templates have a `_CarPhysicsV2` record.
    pub templates: usize,
    range: HashMap<&'static str, (f32, f32)>,
    /// The templates whose `EngineTorque` is 0, by key.
    pub trailers: Vec<u32>,
}

impl Fleet {
    /// One pass over every `_CarPhysicsV2` record of the worldentity.
    pub fn read(we: &WorldEntity) -> Result<Fleet, String> {
        let mut fleet = Fleet::default();
        for comp in we.groups_of(CAR_CLASS) {
            let names = field_names(comp, &CAR_FIELDS)?;
            for r in &comp.records {
                fleet.templates += r.keys.len();
                let values = named_values(comp, r, &names)?;
                for (name, v) in &values {
                    let FieldValue::F32(v) = *v else { continue };
                    let e = fleet.range.entry(name).or_insert((v, v));
                    *e = (e.0.min(v), e.1.max(v));
                }
                if values.get("EngineTorque") == Some(&FieldValue::F32(0.0)) {
                    fleet.trailers.extend(&r.keys);
                }
            }
        }
        fleet.trailers.sort_unstable();
        Ok(fleet)
    }

    /// The lowest and highest value of a float field.
    pub fn range(&self, name: &str) -> Option<(f32, f32)> {
        self.range.get(name).copied()
    }

    /// Where `v` sits between a field's lowest (0) and highest (1) value; `None` for a field every
    /// template gives the same value, or one that is not a float.
    pub fn place(&self, name: &str, v: f32) -> Option<f32> {
        let (lo, hi) = self.range(name)?;
        (hi > lo).then(|| ((v - lo) / (hi - lo)).clamp(0.0, 1.0))
    }
}

/// One record of a template that names a node of its model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeUse {
    pub node: u32,
    pub class: &'static str,
    /// The record's `NAME` hash; 0 for a class with no such field.
    pub name: u32,
    /// `VehiclePart`: its `TYPE` and `PartData`. Zero for the other classes.
    pub kind: u32,
    pub data: u32,
}

/// The classes whose records name a node, with the field that holds it.
const NODE_FIELDS: [(&str, &str); 6] = [
    ("ConnectPoint", "BoneName"),
    ("VehiclePart", "PartName"),
    ("SeatLink", "HardPoint"),
    ("EntranceLink", "HardPoint"),
    ("PhysicalLink", "HardPoint"),
    (WHEEL_CLASS, "SuspBone"),
];

/// A word field of a decoded record, found by its name's hash; `None` when the class has no such
/// field.
fn word_named(comp: &Component, values: &[Value], name: &str) -> Result<Option<u32>, String> {
    let hash = pandemic_hash_m2(name);
    let Some(i) = comp.schema.fields.iter().position(|f| f.name_hash == hash) else { return Ok(None) };
    match values.get(i) {
        Some(Value::Field(FieldValue::U32(v) | FieldValue::Bits(v))) => Ok(Some(*v)),
        other => Err(format!("{}.{name} is {other:?}, not a word", comp.class)),
    }
}

/// Every record of a template that names a node, in the order of [`NODE_FIELDS`] then record order.
pub fn node_uses(we: &WorldEntity, key: u32) -> Result<Vec<NodeUse>, String> {
    let mut out = Vec::new();
    for (class, field) in NODE_FIELDS {
        for (comp, r) in records_of(we, class, key) {
            let values = comp.decode(&r.payload)?;
            let node = word_named(comp, &values, field)?
                .ok_or_else(|| format!("{class} has no field {field}"))?;
            out.push(NodeUse {
                node,
                class,
                name: word_named(comp, &values, "NAME")?.unwrap_or(0),
                kind: word_named(comp, &values, "TYPE")?.unwrap_or(0),
                data: word_named(comp, &values, "PartData")?.unwrap_or(0),
            });
        }
    }
    Ok(out)
}

/// The name of a `VehiclePart` `TYPE` value, from the worldentity's own enum table.
pub fn part_kind(we: &WorldEntity, kind: u32) -> Option<&str> {
    let table = we.enums.iter().find(|e| e.values.iter().any(|v| v.name == "LightBrake"))?;
    table.values.iter().find(|v| v.value == kind).map(|v| v.name.as_str())
}

/// How many records of a class a template has.
pub fn record_count(we: &WorldEntity, class: &str, key: u32) -> usize {
    records_of(we, class, key).count()
}

/// Each of a model's nodes at rest in model space, in list order. `local` is row-major in the
/// row-vector convention, which is `glam`'s column layout, and a node follows its parent in the
/// list.
pub fn node_worlds(hier: &[HierNode]) -> Vec<Mat4> {
    let mut worlds: Vec<Mat4> = Vec::with_capacity(hier.len());
    for n in hier {
        let local = Mat4::from_cols_array(&n.local);
        worlds.push(n.parent.and_then(|p| worlds.get(p).copied()).map_or(local, |p| p * local));
    }
    worlds
}

/// The model-space position of each of a model's nodes at rest, by name hash.
pub fn node_positions(hier: &[HierNode]) -> HashMap<u32, [f32; 3]> {
    hier.iter().zip(node_worlds(hier)).map(|(n, world)| (n.hash, world.w_axis.truncate().to_array())).collect()
}

/// How each of a model's nodes is turned at rest in model space, by name hash.
pub fn node_turns(hier: &[HierNode]) -> HashMap<u32, Quat> {
    hier.iter().zip(node_worlds(hier)).map(|(n, world)| (n.hash, Quat::from_mat4(&world))).collect()
}

/// Which of `models` carry each node: a node's name hash to the models whose hierarchy has it, in
/// the order given.
pub fn models_by_node(w: &mut crate::app::WadStack, models: &[u32]) -> Result<HashMap<u32, Vec<u32>>, String> {
    let mut out: HashMap<u32, Vec<u32>> = HashMap::new();
    for model in models {
        let mut seen: Vec<u32> = mercs2_formats::orchestrator::parse_hier(&w.extract_container(*model)?).iter().map(|n| n.hash).collect();
        seen.sort_unstable();
        seen.dedup();
        for node in seen {
            out.entry(node).or_default().push(*model);
        }
    }
    Ok(out)
}

/// What an exhaust part starts: the effect and how its template starts it. `PartData` is the name
/// hash of a template, and that template's `RedEffectComponent` names the effect.
pub fn started_by_name(starters: &HashMap<u32, Vec<crate::vfxview::StartedBy>>, template: u32) -> Result<(u32, Starter), String> {
    let mut found: Vec<(u32, Starter)> = Vec::new();
    for (effect, list) in starters {
        for s in list.iter().filter(|s| pandemic_hash_m2(&s.template) == template) {
            if !found.contains(&(*effect, s.starter)) {
                found.push((*effect, s.starter));
            }
        }
    }
    match found[..] {
        [one] => Ok(one),
        [] => Err(format!("no template that starts an effect is named by 0x{template:08X}")),
        _ => Err(format!("{} templates named by 0x{template:08X} start different effects", found.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(index: usize, hash: u32, parent: Option<usize>, at: [f32; 3]) -> HierNode {
        let mut local = Mat4::IDENTITY.to_cols_array();
        local[12..15].copy_from_slice(&at);
        HierNode { index, hash, parent, local, bbox_min: [0.0; 3], bbox_max: [0.0; 3] }
    }

    #[test]
    fn a_child_node_sits_at_its_parents_position_plus_its_own() {
        let hier = [node(0, 1, None, [0.0, 0.0, 0.0]), node(1, 2, Some(0), [-0.89, 0.59, 4.07]), node(2, 3, Some(1), [-0.25, 0.0, 0.0])];
        let at = node_positions(&hier);
        assert_eq!(at[&2], [-0.89, 0.59, 4.07]);
        let wheel = at[&3];
        assert!((wheel[0] + 1.14).abs() < 1e-6 && wheel[1] == 0.59 && wheel[2] == 4.07, "{wheel:?}");
    }

    #[test]
    fn every_unread_field_is_a_car_field() {
        for name in UNREAD {
            assert!(CAR_FIELDS.contains(&name), "{name}");
        }
    }

    #[test]
    fn no_two_field_names_share_a_hash() {
        for names in [&CAR_FIELDS[..], &WHEEL_FIELDS[..]] {
            let hashes: std::collections::BTreeSet<u32> = names.iter().map(|n| pandemic_hash_m2(n)).collect();
            assert_eq!(hashes.len(), names.len());
        }
    }

    #[test]
    fn a_fleet_places_a_value_between_its_lowest_and_highest() {
        let mut fleet = Fleet::default();
        fleet.range.insert("Mass", (200.0, 28500.0));
        fleet.range.insert("HighSpeedSteerLossBrakeMulti", (0.5, 0.5));
        assert!((fleet.place("Mass", 27000.0).unwrap() - 0.947).abs() < 1e-3);
        assert_eq!(fleet.place("HighSpeedSteerLossBrakeMulti", 0.5), None);
        assert_eq!(fleet.place("NoSuchField", 1.0), None);
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;
    use crate::app::WadStack;
    use crate::drivingmath;

    /// `Austin (CIV)`, the civilian semi tractor.
    const AUSTIN: u32 = 0x8000_75ED;
    /// `civ_veh_semi_austin`.
    const AUSTIN_MODEL: u32 = 0x926D_9ECB;
    /// `Flatbed Trailer`.
    const FLATBED: u32 = 0x8000_76A6;

    fn retail() -> (WadStack, WorldEntity) {
        let mut w = WadStack::open(&crate::app::retail_vz_wad(), &[]).expect("open retail vz.wad");
        let we = WorldEntity::parse(&w.worldentity().expect("worldentity")).expect("parse worldentity");
        (w, we)
    }

    #[test]
    fn the_austins_physics_reads_by_field_name() {
        let (_, we) = retail();
        let f = car_fields(&we, AUSTIN).unwrap().expect("the Austin is a car");
        assert_eq!(f.num("MaxSpeed").unwrap(), 90.0);
        assert_eq!(f.num("GearRatio1").unwrap(), 3.7);
        assert_eq!(f.num("SteerMaxAngle").unwrap(), 0.8);
        assert_eq!(f.num("Mass").unwrap(), 27000.0);
        assert_eq!(f.vec3("CenterOfMassOffset").unwrap(), [0.0, 0.1, 0.0]);
        assert_eq!(f.values.len(), CAR_FIELDS.len());
    }

    #[test]
    fn a_template_without_car_physics_has_no_car_fields() {
        let (_, we) = retail();
        // `Assault Rifle`
        assert_eq!(car_fields(&we, 0x8000_436F).unwrap(), None);
    }

    #[test]
    fn the_austin_has_two_front_wheels_and_four_rear() {
        let (_, we) = retail();
        let wheels = wheels(&we, AUSTIN).unwrap();
        assert_eq!(wheels.len(), 6);
        assert_eq!(wheels.iter().filter(|w| !w.rear).count(), 2);
        let f = car_fields(&we, AUSTIN).unwrap().unwrap();
        let (g, a) = (f.gearbox().unwrap(), f.axles(&wheels).unwrap());
        assert!((drivingmath::upshift_kmh(&g, &a, 1) - 42.45).abs() < 0.01);
    }

    #[test]
    fn the_fleet_spans_every_car_template() {
        let (_, we) = retail();
        let fleet = Fleet::read(&we).unwrap();
        assert_eq!(fleet.templates, 454);
        assert_eq!(fleet.range("Mass"), Some((200.0, 28500.0)));
        assert_eq!(fleet.range("EngineTorque"), Some((0.0, 1800.0)));
        assert_eq!(fleet.range("CenterOfMassOffset"), None);
    }

    #[test]
    fn the_trailers_are_the_templates_with_no_engine_torque() {
        let (mut w, we) = retail();
        let fleet = Fleet::read(&we).unwrap();
        let names = w.template_names().unwrap();
        let mut trailers: Vec<&str> = fleet.trailers.iter().map(|k| names[k].as_str()).collect();
        trailers.sort_unstable();
        assert_eq!(
            trailers,
            ["Box Trailer", "Civ Fueltrailer", "Civ Fueltrailer_PmcCon018", "Flatbed Trailer", "Fuel Trailer", "Opentop Trailer", "Trailer"]
        );
        assert!(fleet.trailers.contains(&FLATBED));
    }

    #[test]
    fn the_austins_records_name_its_winch_exhaust_and_seat_nodes() {
        let (_, we) = retail();
        let uses = node_uses(&we, AUSTIN).unwrap();
        let of = |class: &str| uses.iter().filter(|u| u.class == class).collect::<Vec<_>>();
        let winch: Vec<u32> = of("ConnectPoint").iter().map(|u| u.node).collect();
        assert_eq!(winch.len(), 2);
        assert!(winch.contains(&pandemic_hash_m2("hp_winch_top")) && winch.contains(&pandemic_hash_m2("hp_winch_bottom")));
        assert_eq!(of("SeatLink").len(), 2);
        assert_eq!(of("EntranceLink").len(), 2);
        assert_eq!(of(WHEEL_CLASS).len(), 6);
        let exhausts: Vec<&NodeUse> =
            of("VehiclePart").into_iter().filter(|u| part_kind(&we, u.kind) == Some("Exhaust")).collect();
        assert_eq!(exhausts.len(), 2);
        assert!(exhausts.iter().all(|u| u.data == 0x06A2_62B1));
        assert!(exhausts.iter().any(|u| u.node == pandemic_hash_m2("hp_fx_exhaust_a")));
        // nothing names the hitch
        assert!(uses.iter().all(|u| u.node != pandemic_hash_m2("hp_trailerhitch")));
        assert_eq!(record_count(&we, "PhysicalLink", AUSTIN), 0);
        assert_eq!(record_count(&we, "ConstraintLink", AUSTIN), 0);
    }

    /// The Austin's exhaust `PartData` is no effect of the archive: it is the name hash of the
    /// template `global_particle_veh_exhaust_car`, whose `RedEffectComponent` names one.
    #[test]
    fn the_austins_exhaust_part_data_names_a_template_that_starts_an_effect() {
        let (mut w, we) = retail();
        const PART_DATA: u32 = 0x06A2_62B1;
        assert_eq!(pandemic_hash_m2("global_particle_veh_exhaust_car"), PART_DATA);
        let base = w.wads.first_mut().unwrap();
        assert!(mercs2_engine::game_world::load_effect(base, PART_DATA).unwrap().is_none());
        let names = w.template_names().unwrap();
        let starters = crate::vfxview::starters(&we, &names).unwrap();
        let (effect, starter) = started_by_name(&starters, PART_DATA).unwrap();
        let base = w.wads.first_mut().unwrap();
        let read = mercs2_engine::game_world::load_effect(base, effect).unwrap().expect("the template's effect is in the archive");
        assert!(!read.emitters.is_empty());
        assert_eq!(starter.duration, 0.0, "{starter:?}");
        assert!(started_by_name(&starters, 0x1234_5678).unwrap_err().contains("12345678"));
    }

    /// The front wheels turn about their yaw node's own up axis, which is the model's: the node
    /// stands upright at rest.
    #[test]
    fn the_austins_steered_nodes_stand_upright() {
        let (mut w, we) = retail();
        let hier = mercs2_formats::orchestrator::parse_hier(&w.model(AUSTIN_MODEL).unwrap().resident);
        let worlds = node_worlds(&hier);
        let front: Vec<Wheel> = wheels(&we, AUSTIN).unwrap().into_iter().filter(|w| !w.rear).collect();
        assert_eq!(front.len(), 2);
        for wheel in front {
            let n = hier.iter().find(|n| n.hash == wheel.yaw_bone()).unwrap_or_else(|| panic!("no node 0x{:08X}", wheel.yaw_bone()));
            let up = worlds[n.index].y_axis.truncate();
            assert!((up - glam::Vec3::Y).length() < 1e-4, "node 0x{:08X} up is {up:?}", n.hash);
        }
    }

    /// The Austin's two exhaust nodes are the tops of its stacks, each tipped 50° back from upright.
    #[test]
    fn the_austins_exhaust_nodes_are_tipped_back() {
        let (mut w, _) = retail();
        let hier = mercs2_formats::orchestrator::parse_hier(&w.model(AUSTIN_MODEL).unwrap().resident);
        let (at, turns) = (node_positions(&hier), node_turns(&hier));
        for name in ["hp_fx_exhaust_a", "hp_fx_exhaust_b"] {
            let node = pandemic_hash_m2(name);
            assert!((at[&node][1] - 4.30).abs() < 0.006, "{name}: {:?}", at[&node]);
            let up = turns[&node] * glam::Vec3::Y;
            assert!((up - glam::Vec3::new(0.0, 0.6428, -0.7660)).length() < 1e-3, "{name}: up is {up:?}");
        }
    }

    #[test]
    fn six_models_carry_the_hitch_node() {
        let (mut w, _) = retail();
        let names = crate::index::load_all_names().unwrap_or_else(|e| panic!("{e}"));
        let index = crate::index::AssetIndex::build(&w.wads, &w.side, names);
        let lens = crate::domain::Domain::Driving;
        let models: Vec<u32> = index.models().iter().filter(|r| lens.model_lens(r)).map(|r| r.hash).collect();
        let t0 = std::time::Instant::now();
        let by_node = models_by_node(&mut w, &models).unwrap();
        eprintln!("{} vehicle models scanned in {:?}", models.len(), t0.elapsed());
        let hitch = &by_node[&pandemic_hash_m2("hp_trailerhitch")];
        assert_eq!(hitch.len(), 6, "{hitch:08X?}");
        assert!(hitch.contains(&AUSTIN_MODEL));
    }

    #[test]
    fn the_austins_wheels_and_hitch_are_where_its_hierarchy_puts_them() {
        let (mut w, we) = retail();
        let hier = mercs2_formats::orchestrator::parse_hier(&w.model(AUSTIN_MODEL).unwrap().resident);
        let at = node_positions(&hier);
        let near = |a: f32, b: f32| (a - b).abs() < 0.006;
        let hitch = at[&pandemic_hash_m2("hp_trailerhitch")];
        assert!(near(hitch[0], 0.0) && near(hitch[1], 1.20) && near(hitch[2], -3.10), "{hitch:?}");
        let mut front_z = Vec::new();
        for wheel in wheels(&we, AUSTIN).unwrap() {
            let p = at.get(&wheel.susp_bone).unwrap_or_else(|| panic!("wheel node 0x{:08X} is not in the model", wheel.susp_bone));
            if !wheel.rear {
                front_z.push(p[2]);
            }
        }
        assert!(front_z.len() == 2 && front_z.iter().all(|z| near(*z, 4.07)), "{front_z:?}");
    }
}
