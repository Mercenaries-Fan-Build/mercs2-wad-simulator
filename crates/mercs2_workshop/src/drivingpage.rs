//! The Driving page: one vehicle's driving systems. A systems bar picks one of the systems the
//! vehicle's physics class has (a car's Chassis, Powertrain, Steering and Coupling; a tank's, a
//! boat's and a helicopter's own); each has a bench under the viewport and cards in the inspector,
//! all read from the chosen template's records. The car's benches and cards are here; the other
//! classes' are in `drivingtank`, `drivingboat` and `drivingheli`. Behaviour contract:
//! `docs/modernization/workshop_pages/driving.md`.

use crate::app::WadStack;
use crate::drivingdata::{self, FakeWheel, Fields, Fleet, Kind, NodeUse, Rotor, Spinner, Wheel, Winch};
use crate::drivingmath::{self, Pedals};
use crate::gui::theme;
use crate::index::{AssetIndex, AssetRow};
use crate::plot::{self, number, Plot};
use egui::{Align2, Color32, Stroke};
use mercs2_engine::fx_sim::Starter;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::orchestrator::HierNode;
use mercs2_formats::worldentity::WorldEntity;
use std::collections::HashMap;

/// The bench under the viewport, in pixels. It does not resize.
pub const BENCH_H: f32 = 270.0;

/// How much further back the camera frames a model on this page than on Inspect: the window's
/// height over the height the header, verb bar, systems bar, bench and status bar leave.
pub fn framing(window_height: f32) -> f32 {
    window_height / (window_height - BENCH_H - 140.0).max(120.0)
}

/// The smallest sphere holding two spheres, each a centre and a radius: what the camera frames
/// when a trailer stands behind the tractor.
pub fn enclosing(a: (glam::Vec3, f32), b: (glam::Vec3, f32)) -> (glam::Vec3, f32) {
    let apart = (b.0 - a.0).length();
    if apart + b.1 <= a.1 {
        return a;
    }
    if apart + a.1 <= b.1 {
        return b;
    }
    let radius = (apart + a.1 + b.1) / 2.0;
    (a.0 + (b.0 - a.0) * ((radius - a.1) / apart), radius)
}

/// The node a trailer hangs from. No shipped record names it; the name is the string that hashes
/// to the node six tractor models carry.
const HITCH: &str = "hp_trailerhitch";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Chassis,
    Powertrain,
    Steering,
    Coupling,
    Hull,
    Flight,
    Yaw,
    Attitude,
    Flyby,
}

impl System {
    pub const ALL: [System; 9] = [
        System::Flyby,
        System::Chassis,
        System::Powertrain,
        System::Steering,
        System::Coupling,
        System::Hull,
        System::Flight,
        System::Yaw,
        System::Attitude,
    ];

    pub fn label(self) -> &'static str {
        match self {
            System::Chassis => "Chassis",
            System::Powertrain => "Powertrain",
            System::Steering => "Steering",
            System::Coupling => "Coupling",
            System::Hull => "Hull",
            System::Flight => "Flight",
            System::Yaw => "Yaw",
            System::Attitude => "Attitude",
            System::Flyby => "Flyby",
        }
    }

    /// The system a label names, folding case.
    pub fn named(label: &str) -> Option<System> {
        System::ALL.into_iter().find(|s| s.label().eq_ignore_ascii_case(label))
    }

    /// The systems a vehicle of a physics class has, in the order the bar shows them.
    pub fn of(kind: Kind) -> &'static [System] {
        match kind {
            Kind::Car | Kind::Tank => &[System::Chassis, System::Powertrain, System::Steering, System::Coupling],
            Kind::Boat => &[System::Hull, System::Powertrain, System::Steering, System::Coupling],
            Kind::Helicopter => &[System::Flight, System::Yaw, System::Attitude, System::Coupling],
            Kind::Plane => &[System::Flyby, System::Coupling],
        }
    }
}

/// What the viewport can mark on the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    Wheels,
    Travel,
    TorqueSplit,
    Hardpoints,
    FullLock,
    Probes,
    Rotors,
    Winch,
}

impl Overlay {
    const ALL: [Overlay; 8] = [
        Overlay::Wheels,
        Overlay::Travel,
        Overlay::TorqueSplit,
        Overlay::Hardpoints,
        Overlay::FullLock,
        Overlay::Probes,
        Overlay::Rotors,
        Overlay::Winch,
    ];

    fn label(self) -> &'static str {
        match self {
            Overlay::Wheels => "Show wheels",
            Overlay::Travel => "Show suspension travel",
            Overlay::TorqueSplit => "Show torque split",
            Overlay::Hardpoints => "Show hardpoints",
            Overlay::FullLock => "Show full lock",
            Overlay::Probes => "Show ground probes",
            Overlay::Rotors => "Show rotors",
            Overlay::Winch => "Show winch reach",
        }
    }

    /// The overlays a system of a vehicle class offers.
    fn of(kind: Kind, system: System) -> &'static [Overlay] {
        match (kind, system) {
            (Kind::Car, System::Chassis) => &[Overlay::Wheels, Overlay::Travel],
            (Kind::Car, System::Powertrain) => &[Overlay::TorqueSplit],
            (Kind::Car, System::Steering) => &[Overlay::FullLock],
            (Kind::Tank, System::Chassis) => &[Overlay::Wheels, Overlay::Travel, Overlay::Probes],
            (Kind::Helicopter, System::Flight) => &[Overlay::Rotors],
            (Kind::Helicopter, System::Coupling) => &[Overlay::Hardpoints, Overlay::Winch],
            (_, System::Coupling) => &[Overlay::Hardpoints],
            _ => &[],
        }
    }
}

/// The level's templates, read once, and which vehicle models carry each node.
pub(crate) struct Data {
    pub(crate) we: WorldEntity,
    /// The car templates' value ranges and the trailers.
    fleet: Fleet,
    /// The value ranges of the other physics classes, by class.
    fleets: HashMap<&'static str, Fleet>,
    template_names: HashMap<u32, String>,
    /// The names of the vehicle models whose hierarchy has each node.
    by_node: HashMap<u32, Vec<String>>,
}

impl Data {
    /// The value ranges of a physics class.
    pub(crate) fn fleet_of(&self, class: &str) -> Option<&Fleet> {
        if class == drivingdata::CAR_CLASS { Some(&self.fleet) } else { self.fleets.get(class) }
    }
}

/// One exhaust the Exhaust picker plays: the effect its part's template starts, and the node it
/// leaves from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exhaust {
    pub effect: u32,
    pub starter: Starter,
    pub at: [f32; 3],
    pub turn: glam::Quat,
}

/// A car template's physics and wheels.
pub(crate) struct Car {
    fields: Fields,
    wheels: Vec<Wheel>,
}

/// What a template drives with, by its physics class.
pub(crate) enum Drive {
    Car(Car),
    Tank { fields: Fields, wheels: Vec<FakeWheel> },
    Boat { fields: Fields },
    /// The record a player flies with and the one an AI pilot flies with.
    Helicopter { player: Fields, ai: Option<Fields>, rotors: Vec<Rotor>, winch: Option<Winch> },
    /// A plane `Airstrike.Flyby` flies: no physics record, and the nodes that turn by themselves.
    Plane { spinners: Vec<Spinner> },
}

impl Drive {
    pub(crate) fn kind(&self) -> Kind {
        match self {
            Drive::Car(_) => Kind::Car,
            Drive::Tank { .. } => Kind::Tank,
            Drive::Boat { .. } => Kind::Boat,
            Drive::Helicopter { .. } => Kind::Helicopter,
            Drive::Plane { .. } => Kind::Plane,
        }
    }
}

/// The chosen template of the model on show.
pub(crate) struct Vehicle {
    pub(crate) key: u32,
    pub(crate) drive: Drive,
    pub(crate) uses: Vec<NodeUse>,
    physical_links: usize,
    constraint_links: usize,
}

impl Vehicle {
    fn car(&self) -> Option<&Car> {
        match &self.drive {
            Drive::Car(car) => Some(car),
            _ => None,
        }
    }
}

/// The page's state.
pub struct Panel {
    data: Option<Result<Data, String>>,
    pub system: System,
    /// Narrows the vehicle list.
    pub filter: String,
    /// The class group the vehicle list has open, when the user chose one.
    open_group: Option<&'static str>,
    model: Option<u32>,
    /// The templates of the four physics classes that name the model on show, by name.
    templates: Vec<(u32, String)>,
    vehicle: Option<Result<Vehicle, String>>,
    /// The model's node positions, turns and names, by hash.
    nodes: HashMap<u32, [f32; 3]>,
    node_turns: HashMap<u32, glam::Quat>,
    node_names: HashMap<u32, String>,
    pub selected_node: Option<u32>,
    /// The trailer template on the hitch.
    pub trailer: Option<u32>,
    /// Whether the exhausts play.
    pub exhaust: bool,
    /// Whether a helicopter's rotors turn.
    pub rotors: bool,
    /// The overlays that are on, by [`Overlay`] as an index.
    pub show: [bool; 8],
    /// Why the chosen trailer is not drawn, why the exhausts do not play, and why the wheels do
    /// not turn.
    pub trailer_problem: Option<String>,
    pub exhaust_problem: Option<String>,
    pub lock_problem: Option<String>,
    /// A trailer and a template to choose, by name, once the next model's templates are read; the
    /// system a `--ui-state` file named, which the next model must have; and the rotor state it
    /// named.
    pending_trailer: Option<String>,
    pending_template: Option<String>,
    pending_system: Option<System>,
    pending_rotors: Option<bool>,
}

impl Default for Panel {
    fn default() -> Panel {
        Panel {
            data: None,
            system: System::Chassis,
            filter: String::new(),
            open_group: None,
            model: None,
            templates: Vec::new(),
            vehicle: None,
            nodes: HashMap::new(),
            node_turns: HashMap::new(),
            node_names: HashMap::new(),
            selected_node: None,
            trailer: None,
            exhaust: false,
            rotors: false,
            show: [true, false, false, true, false, false, true, true],
            trailer_problem: None,
            exhaust_problem: None,
            lock_problem: None,
            pending_trailer: None,
            pending_template: None,
            pending_system: None,
            pending_rotors: None,
        }
    }
}

pub enum Act {
    System(System),
    Filter(String),
    Group(&'static str),
    Load(u32, String),
    Template(u32),
    Node(u32),
    Trailer(Option<u32>),
    Exhaust(bool),
    Rotors(bool),
    Overlay(Overlay),
    Contribute(&'static str),
}

/// What the app does for the page after an act.
pub enum Host {
    Load(u32, String),
    Contribute(&'static str),
    /// Spin the helicopter's rotors up, or let them run down.
    Rotors(bool),
}

impl Panel {
    /// Reads the level's templates and the vehicle models' nodes the first time the page is shown.
    pub fn ensure(&mut self, w: &mut WadStack, index: &AssetIndex) {
        if self.data.is_none() {
            self.data = Some(read_data(w, index));
        }
    }

    /// A level change: the templates are read again on the next show.
    pub fn reset(&mut self) {
        *self = Panel { system: self.system, ..Panel::default() };
    }

    /// The steered wheels' nodes with the angle each turns to, while Show full lock is on: the
    /// front wheels, each by `SteerMaxAngle`. The game shows a front wheel's steering as the share
    /// of the lock in use times the whole `SteerMaxAngle`, whatever the speed (`FUN_0044f680`,
    /// `0x44fb8d`), and gives every front wheel the same angle (`FUN_00449dc0`).
    pub fn steer_pose(&self) -> Vec<(u32, f32)> {
        let mut out: Vec<(u32, f32)> = Vec::new();
        let Some(car) = self.vehicle().and_then(Vehicle::car) else { return out };
        if self.system != System::Steering || !self.show[Overlay::FullLock as usize] {
            return out;
        }
        let Ok(s) = car.fields.steering() else { return out };
        for w in car.wheels.iter().filter(|w| !w.rear) {
            if !out.iter().any(|(node, _)| *node == w.yaw_bone()) {
                out.push((w.yaw_bone(), s.max_angle));
            }
        }
        out
    }

    /// The template's exhaust parts.
    fn exhaust_parts(&self) -> Vec<&NodeUse> {
        let (Some(v), Some(data)) = (self.vehicle(), self.data()) else { return Vec::new() };
        v.uses.iter().filter(|u| u.class == "VehiclePart" && drivingdata::part_kind(&data.we, u.kind) == Some("Exhaust")).collect()
    }

    /// What plays while the Exhaust picker is On: each exhaust part's effect at its node. Nothing
    /// while it is Off. An error for a part whose node the model lacks or whose `PartData` names no
    /// template that starts an effect.
    pub fn exhausts(&self, index: &AssetIndex) -> Result<Vec<Exhaust>, String> {
        if !self.exhaust {
            return Ok(Vec::new());
        }
        self.exhaust_parts()
            .into_iter()
            .map(|u| {
                let (Some(at), Some(turn)) = (self.nodes.get(&u.node), self.node_turns.get(&u.node)) else {
                    return Err(format!("the model has no node 0x{:08X}", u.node));
                };
                let (effect, starter) = drivingdata::started_by_name(&index.effect_starters, u.data)?;
                Ok(Exhaust { effect, starter, at: *at, turn: *turn })
            })
            .collect()
    }

    /// The model the viewport shows, with its hierarchy. A new model picks the first of its templates
    /// that has one of the four physics classes, and the first system of that class when the open
    /// system is not one of its own. Returns what the app is to do for a newly shown model: the
    /// rotor state a `--ui-state` file named.
    pub fn show_model(&mut self, model: Option<(u32, &[HierNode])>, index: &AssetIndex) -> Option<Host> {
        let hash = model.map(|(h, _)| h);
        if hash == self.model {
            return None;
        }
        self.read_model(model, index);
        self.keep_system();
        // A named system the vehicle's class lacks, and a rotor state for a vehicle with no
        // rotors, are the capture's mistake: the bench says so.
        if let (Some(named), Some(kind)) = (self.pending_system.take(), self.kind()) {
            if System::of(kind).contains(&named) {
                self.system = named;
            } else {
                let offered: Vec<&str> = System::of(kind).iter().map(|s| s.label()).collect();
                self.vehicle = Some(Err(format!("a {} template has no {} system; one of: {}", kind.class(), named.label(), offered.join(", "))));
                return None;
            }
        }
        let on = self.pending_rotors.take()?;
        if !matches!(self.vehicle().map(|v| &v.drive), Some(Drive::Helicopter { rotors, .. }) if !rotors.is_empty()) {
            self.vehicle = Some(Err("the model on show has no rotors to spin".to_string()));
            return None;
        }
        self.rotors = on;
        Some(Host::Rotors(on))
    }

    fn read_model(&mut self, model: Option<(u32, &[HierNode])>, index: &AssetIndex) {
        let hash = model.map(|(h, _)| h);
        self.model = hash;
        self.templates.clear();
        self.vehicle = None;
        self.rotors = false;
        self.nodes.clear();
        self.node_turns.clear();
        self.node_names.clear();
        self.selected_node = None;
        self.trailer = None;
        self.exhaust_problem = None;
        self.lock_problem = None;
        let (Some((hash, hier)), Some(Ok(data))) = (model, &self.data) else { return };
        self.nodes = drivingdata::node_positions(hier);
        self.node_turns = drivingdata::node_turns(hier);
        self.node_names =
            self.nodes.keys().filter_map(|h| index.names.get(h).map(|n| (*h, n.clone()))).collect();
        for t in index.world_templates.iter().filter(|t| t.model == hash) {
            match Kind::of(&data.we, t.key) {
                Ok(Some(_)) => {
                    let name = data.template_names.get(&t.key).cloned().unwrap_or_else(|| format!("0x{:08X}", t.key));
                    self.templates.push((t.key, name));
                }
                Ok(None) => {}
                Err(e) => {
                    self.vehicle = Some(Err(e));
                    return;
                }
            }
        }
        self.templates.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        // The names of the links the templates' records carry, beside the node names.
        for (key, _) in &self.templates {
            for u in drivingdata::node_uses(&data.we, *key).unwrap_or_default() {
                if let Some(name) = index.names.get(&u.name) {
                    self.node_names.insert(u.name, name.clone());
                }
            }
        }
        match self.pending_template.take() {
            Some(name) => {
                self.vehicle = Some(match self.templates.iter().find(|t| t.1.eq_ignore_ascii_case(&name)) {
                    Some(&(key, _)) => read_vehicle(&data.we, key),
                    None => {
                        let names: Vec<&str> = self.templates.iter().map(|t| t.1.as_str()).collect();
                        Err(format!("no template of this model is named {name:?}; one of: {}", names.join(", ")))
                    }
                });
            }
            None => {
                if let Some(&(key, _)) = self.templates.first() {
                    self.vehicle = Some(read_vehicle(&data.we, key));
                }
            }
        }
        // A tractor opens with its hitch selected.
        let hitch = pandemic_hash_m2(HITCH);
        self.selected_node = self.nodes.contains_key(&hitch).then_some(hitch);
        if let Some(name) = self.pending_trailer.take() {
            self.trailer = data.fleet.trailers.iter().copied().find(|k| data.template_names.get(k) == Some(&name));
            if self.trailer.is_none() {
                self.trailer_problem = Some(format!("no trailer template named {name:?}"));
            }
        }
    }

    fn set_template(&mut self, key: u32) {
        if let (Some(Ok(data)), true) = (&self.data, self.templates.iter().any(|t| t.0 == key)) {
            self.vehicle = Some(read_vehicle(&data.we, key));
            self.trailer = None;
            self.exhaust_problem = None;
            self.lock_problem = None;
            self.keep_system();
        }
    }

    /// Opens the vehicle's first system when the open one is not one its physics class has.
    fn keep_system(&mut self) {
        if let Some(kind) = self.kind() {
            if !System::of(kind).contains(&self.system) {
                self.system = System::of(kind)[0];
            }
        }
    }

    pub(crate) fn vehicle(&self) -> Option<&Vehicle> {
        self.vehicle.as_ref().and_then(|v| v.as_ref().ok())
    }

    /// The physics class of the template on show.
    pub fn kind(&self) -> Option<Kind> {
        self.vehicle().map(|v| v.drive.kind())
    }

    /// Whether the model on show has a template of one of the four physics classes, so the page
    /// draws its systems.
    pub fn has_systems(&self) -> bool {
        self.vehicle.is_some()
    }

    pub(crate) fn data(&self) -> Option<&Data> {
        self.data.as_ref().and_then(|d| d.as_ref().ok())
    }

    fn template_name(&self) -> Option<&str> {
        let key = self.vehicle()?.key;
        self.templates.iter().find(|t| t.0 == key).map(|t| t.1.as_str())
    }

    pub(crate) fn node_label(&self, hash: u32) -> String {
        self.node_names.get(&hash).cloned().unwrap_or_else(|| format!("0x{hash:08X}"))
    }

    /// Where a node of the model on show is, at rest in model space.
    pub(crate) fn node(&self, hash: u32) -> Option<[f32; 3]> {
        self.nodes.get(&hash).copied()
    }


    pub(crate) fn hitch(&self) -> Option<[f32; 3]> {
        self.nodes.get(&pandemic_hash_m2(HITCH)).copied()
    }

    /// The vehicle models whose hierarchy has a node, by name.
    fn models_with(&self, node: u32) -> Vec<&str> {
        let mut names: Vec<&str> = self.data().and_then(|d| d.by_node.get(&node)).into_iter().flatten().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// The trailer to draw: its template, its model, and where its origin goes — on the hitch node,
    /// as `Object.Attach` puts it.
    pub fn trailer_at(&self, index: &AssetIndex) -> Option<(u32, u32, [f32; 3])> {
        let (key, at) = (self.trailer?, self.hitch()?);
        let model = index.world_templates.iter().find(|t| t.key == key)?.model;
        Some((key, model, at))
    }
}

fn read_data(w: &mut WadStack, index: &AssetIndex) -> Result<Data, String> {
    let we = WorldEntity::parse(&w.worldentity()?)?;
    let fleet = Fleet::read(&we, drivingdata::CAR_CLASS)?;
    let mut fleets = HashMap::new();
    for class in [drivingdata::TANK_CLASS, drivingdata::BOAT_CLASS, drivingdata::HELI_CLASS, drivingdata::HELI_AI_CLASS] {
        fleets.insert(class, Fleet::read(&we, class)?);
    }
    let template_names = w.template_names()?;
    let lens = crate::domain::Domain::Driving;
    let labels: HashMap<u32, String> = index.models().iter().filter(|r| lens.model_lens(r)).map(|r| (r.hash, r.label())).collect();
    let mut models: Vec<u32> = labels.keys().copied().collect();
    models.sort_unstable();
    let by_node = drivingdata::models_by_node(w, &models)?
        .into_iter()
        .map(|(node, models)| (node, models.iter().map(|m| labels[m].clone()).collect()))
        .collect();
    Ok(Data { we, fleet, fleets, template_names, by_node })
}

/// Turns each posed node, with every node under it, about its own up axis in `palette`, at its
/// own place: the turn the game's wheel pose gives a front wheel's yaw node (`FUN_006814e0`,
/// `D3DXMatrixRotationY` at `0x6819bf` ahead of the node's rest orientation). `binds` are the
/// nodes' rest transforms in model space, in hierarchy order.
pub fn turn_nodes(palette: &mut [[[f32; 4]; 4]], hier: &[HierNode], binds: &[[[f32; 4]; 4]], pose: &[(u32, f32)]) -> Result<(), String> {
    if !pose.is_empty() && binds.len() != hier.len() {
        return Err(format!("a {}-bone rig for {} nodes", binds.len(), hier.len()));
    }
    for (node, angle) in pose {
        let hub = hier.iter().find(|n| n.hash == *node).ok_or_else(|| format!("the model has no node 0x{node:08X}"))?.index;
        let bind = &binds[hub];
        let mut subtree = vec![hub];
        for n in hier {
            if n.parent.is_some_and(|q| subtree.contains(&q)) {
                subtree.push(n.index);
            }
        }
        mercs2_engine::vehicle::rotor::spin_subtree(palette, bind, &subtree, *angle)?;
    }
    Ok(())
}

fn read_vehicle(we: &WorldEntity, key: u32) -> Result<Vehicle, String> {
    let kind = Kind::of(we, key)?.ok_or_else(|| format!("template 0x{key:08X} has no vehicle physics"))?;
    let record = |class: &'static str| drivingdata::fields(we, class, key)?.ok_or_else(|| format!("template 0x{key:08X} has no {class}"));
    let drive = match kind {
        Kind::Car => Drive::Car(Car { fields: record(drivingdata::CAR_CLASS)?, wheels: drivingdata::wheels(we, key)? }),
        Kind::Tank => Drive::Tank { fields: record(drivingdata::TANK_CLASS)?, wheels: drivingdata::fake_wheels(we, key)? },
        Kind::Boat => Drive::Boat { fields: record(drivingdata::BOAT_CLASS)? },
        Kind::Helicopter => Drive::Helicopter {
            player: record(drivingdata::HELI_CLASS)?,
            ai: drivingdata::fields(we, drivingdata::HELI_AI_CLASS, key)?,
            rotors: drivingdata::rotors(we, key)?,
            winch: drivingdata::winch(we, key)?,
        },
        Kind::Plane => Drive::Plane { spinners: drivingdata::spinners(we, key)? },
    };
    Ok(Vehicle {
        key,
        drive,
        uses: drivingdata::node_uses(we, key)?,
        physical_links: drivingdata::record_count(we, "PhysicalLink", key),
        constraint_links: drivingdata::record_count(we, "ConstraintLink", key),
    })
}

pub fn apply(act: Act, p: &mut Panel) -> Option<Host> {
    match act {
        Act::System(s) => p.system = s,
        Act::Filter(f) => p.filter = f,
        Act::Group(g) => p.open_group = if p.open_group == Some(g) { None } else { Some(g) },
        Act::Load(hash, label) => return Some(Host::Load(hash, label)),
        Act::Template(key) => p.set_template(key),
        Act::Node(hash) => p.selected_node = (p.selected_node != Some(hash)).then_some(hash),
        Act::Trailer(key) => {
            p.trailer = if p.trailer == key { None } else { key };
            p.trailer_problem = None;
        }
        Act::Exhaust(on) => {
            p.exhaust = on;
            p.exhaust_problem = None;
        }
        Act::Rotors(on) => {
            p.rotors = on;
            return Some(Host::Rotors(on));
        }
        Act::Overlay(o) => {
            p.show[o as usize] = !p.show[o as usize];
            p.lock_problem = None;
        }
        Act::Contribute(kind) => return Some(Host::Contribute(kind)),
    }
    None
}

/// Puts the page in the state a `--ui-state` file names: its `system`, `overlays`, `exhaust`,
/// `template` and `trailer`.
pub fn apply_state(p: &mut Panel, st: &crate::ui_state::UiState) -> Result<(), String> {
    if let Some(s) = &st.system {
        let system = System::named(s).ok_or_else(|| format!("no Driving system named {s:?}"))?;
        p.system = system;
        p.pending_system = Some(system);
    }
    if let Some(value) = &st.rotors {
        p.pending_rotors = Some(crate::ui_state::pick("rotors value", value, &[("Stopped", false), ("Spinning", true)])?);
    }
    if let Some(names) = &st.overlays {
        p.show = [false; 8];
        for name in names {
            let o = Overlay::ALL
                .into_iter()
                .find(|o| o.label().eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("no Driving overlay named {name:?}"))?;
            p.show[o as usize] = true;
        }
    }
    if let Some(value) = &st.exhaust {
        p.exhaust = crate::ui_state::pick("exhaust value", value, &[("Off", false), ("On", true)])?;
    }
    p.pending_template = st.template.clone();
    p.pending_trailer = st.trailer.clone();
    Ok(())
}

/// The header crumb: class, template, system.
pub fn crumb(p: &Panel, class: Option<&str>) -> Vec<String> {
    let mut parts: Vec<String> = class.map(str::to_string).into_iter().collect();
    if let Some(name) = p.template_name() {
        parts.push(name.to_string());
        parts.push(p.system.label().to_string());
    }
    parts
}

/// The status bar's line for the open system.
pub fn status(p: &Panel) -> Option<String> {
    let (v, name) = (p.vehicle()?, p.template_name()?);
    let line = match &v.drive {
        Drive::Car(car) => car_status(p, car)?,
        Drive::Tank { fields, wheels } => crate::drivingtank::status(p, fields, wheels)?,
        Drive::Boat { fields } => crate::drivingboat::status(p, fields)?,
        Drive::Helicopter { player, ai, rotors, winch } => crate::drivingheli::status(p, player, ai.as_ref(), rotors, winch.as_ref())?,
        Drive::Plane { spinners } => crate::drivingplane::status(p, spinners),
    };
    Some(format!("{name} · {line}"))
}

/// The Coupling system's status line, the same for every class.
pub(crate) fn coupling_status(p: &Panel) -> String {
    match p.hitch() {
        Some(at) => format!("{HITCH} · {}", position(at)),
        None => "no hitch node".to_string(),
    }
}

fn car_status(p: &Panel, car: &Car) -> Option<String> {
    let f = &car.fields;
    Some(match p.system {
        System::Chassis => {
            let (front, rear) = (car.wheels.iter().filter(|w| !w.rear).count(), car.wheels.iter().filter(|w| w.rear).count());
            format!("{} wheels · {front} front, {rear} rear · mass {}", car.wheels.len(), number(f.num("Mass").ok()?))
        }
        System::Powertrain => {
            let (g, a) = (f.gearbox().ok()?, f.axles(&car.wheels).ok()?);
            let (kmh, rpm) = *drivingmath::full_throttle_path(&g, &a).last()?;
            format!("{} km/h cap at {} rpm · no drive for {} s after a shift", number(kmh), number(rpm.round()), number(f.num("ClutchDelayTime").ok()?))
        }
        System::Steering => {
            let s = f.steering().ok()?;
            format!(
                "lock {} rad · {} rad at {} km/h on throttle",
                number(s.max_angle),
                number((drivingmath::lock(&s, Pedals::Throttle, s.max_speed) * 100.0).round() / 100.0),
                number(s.max_speed)
            )
        }
        System::Coupling => coupling_status(p),
        System::Hull | System::Flight | System::Yaw | System::Attitude | System::Flyby => return None,
    })
}

pub(crate) fn position(at: [f32; 3]) -> String {
    format!("{:.2}, {:.2}, {:.2}", at[0], at[1], at[2]).replace('-', "−")
}

// ── navigator ────────────────────────────────────────────────────────────────────────────────────

/// The models with a vehicle class, by class then name.
fn vehicles(index: &AssetIndex) -> Vec<&AssetRow> {
    let lens = crate::domain::Domain::Driving;
    let mut rows: Vec<&AssetRow> = index.models().iter().filter(|r| lens.model_lens(r)).collect();
    rows.sort_by(|a, b| {
        crate::index::category_order(a.category()).cmp(&crate::index::category_order(b.category())).then_with(|| a.label().cmp(&b.label()))
    });
    rows
}

/// The vehicle list's groups: each class with the rows of it whose name contains `needle` (lower
/// case), in order. A class with no such row is left out.
fn grouped<'a>(rows: &[&'a AssetRow], needle: &str) -> Vec<(&'static str, Vec<&'a AssetRow>)> {
    let mut out: Vec<(&'static str, Vec<&'a AssetRow>)> = Vec::new();
    for r in rows.iter().copied().filter(|r| needle.is_empty() || r.label().to_lowercase().contains(needle)) {
        match out.last_mut() {
            Some((group, list)) if *group == r.category() => list.push(r),
            _ => out.push((r.category(), vec![r])),
        }
    }
    out
}

/// The vehicle list: models with a vehicle class, grouped by class, one group open.
pub fn navigator(ui: &mut egui::Ui, p: &Panel, index: &AssetIndex, loaded: Option<u32>) -> Vec<Act> {
    let mut acts = Vec::new();
    let lens = crate::domain::Domain::Driving;
    let rows = vehicles(index);
    ui.horizontal(|ui| {
        ui.label(theme::disp_text(lens.label().to_uppercase(), theme::style_title(), theme::TX));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(format!("{} models", rows.len())).monospace().color(theme::DIM));
        });
    });
    ui.add_space(6.0);
    let mut filter = p.filter.clone();
    if ui
        .add(egui::TextEdit::singleline(&mut filter).hint_text("filter…").margin(theme::TEXT_EDIT_MARGIN).desired_width(f32::INFINITY))
        .changed()
    {
        acts.push(Act::Filter(filter));
    }
    ui.add_space(6.0);
    let needle = p.filter.trim().to_lowercase();
    let loaded_group = rows.iter().find(|r| Some(r.hash) == loaded).map(|r| r.category());
    let open = p.open_group.or(loaded_group).or_else(|| rows.first().map(|r| r.category()));
    theme::scroll_vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (group, shown) in grouped(&rows, &needle) {
            let is_open = !needle.is_empty() || open == Some(group);
            if group_header(ui, group, shown.len(), is_open).clicked() {
                acts.push(Act::Group(group));
            }
            if !is_open {
                continue;
            }
            for r in shown {
                let on = Some(r.hash) == loaded;
                let resp = theme::row_chip(ui, if on { theme::BRASS_SOFT } else { Color32::TRANSPARENT }, Color32::TRANSPARENT, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(r.label()).color(if on { theme::BRASS } else { theme::TX })).truncate());
                });
                if resp.clicked() {
                    acts.push(Act::Load(r.hash, r.label()));
                }
            }
        }
    });
    acts
}

/// A class group's header row: a triangle, the class, its count.
fn group_header(ui: &mut egui::Ui, group: &str, count: usize, open: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
    let p = ui.painter();
    let c = egui::pos2(rect.left() + 6.0, rect.center().y);
    let (tri, col) = if open {
        (vec![c + egui::vec2(-4.0, -3.0), c + egui::vec2(4.0, -3.0), c + egui::vec2(0.0, 3.0)], theme::BRASS)
    } else {
        (vec![c + egui::vec2(-3.0, -4.0), c + egui::vec2(3.0, 0.0), c + egui::vec2(-3.0, 4.0)], theme::DIM)
    };
    p.add(egui::Shape::convex_polygon(tri, col, Stroke::NONE));
    p.text(egui::pos2(rect.left() + 16.0, rect.center().y), Align2::LEFT_CENTER, group.to_uppercase(), theme::font(&theme::style_eyebrow()), theme::DIM);
    p.text(egui::pos2(rect.right() - 6.0, rect.center().y), Align2::RIGHT_CENTER, count, theme::font(&egui::TextStyle::Monospace), theme::DIM);
    resp
}

// ── verbs ────────────────────────────────────────────────────────────────────────────────────────

/// The template picker, at the right of the verb bar.
pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(key) = p.vehicle().map(|v| v.key) else { return acts };
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let at = p.templates.iter().position(|t| t.0 == key).map_or(0, |i| i + 1);
        ui.label(egui::RichText::new(format!("{at} of {}", p.templates.len())).monospace().color(theme::DIM));
        let mut chosen = key;
        egui::ComboBox::from_id_salt("driving_template")
            .selected_text(p.template_name().unwrap_or_default())
            .width(180.0)
            .show_ui(ui, |ui| {
                for (k, name) in &p.templates {
                    ui.selectable_value(&mut chosen, *k, name);
                }
            });
        if chosen != key {
            acts.push(Act::Template(chosen));
        }
        ui.label(egui::RichText::new("Template").small().color(theme::DIM));
    });
    acts
}

// ── centre: the systems bar and the bench ────────────────────────────────────────────────────────

/// The systems bar over the viewport and the bench under it. Nothing for a model with no template
/// of the four physics classes.
pub fn center(root: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(vehicle) = &p.vehicle else { return acts };
    let frame = |margin: egui::Margin| egui::Frame::NONE.fill(theme::G1).inner_margin(margin);
    egui::Panel::top("driving_systems")
        .exact_size(34.0)
        .resizable(false)
        .frame(crate::gui::panel_frame(frame(egui::Margin::symmetric(8, 5)), crate::gui::SeparatorEdge::Bottom))
        .show(root, |ui| {
            let offered = p.kind().map_or(&[][..], System::of);
            let tabs: Vec<(System, &str)> = offered.iter().map(|s| (*s, s.label())).collect();
            if let Some(s) = theme::tab_row(ui, &tabs, p.system) {
                acts.push(Act::System(s));
            }
        });
    egui::Panel::bottom("driving_bench")
        .exact_size(BENCH_H)
        .resizable(false)
        .frame(crate::gui::panel_frame(frame(egui::Margin::symmetric(10, 0)), crate::gui::SeparatorEdge::Top))
        .show(root, |ui| match vehicle {
            Err(e) => {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(e).color(theme::BAD));
            }
            Ok(v) if p.system == System::Coupling => acts.extend(coupling_bench(ui, p, v)),
            Ok(v) => match &v.drive {
                Drive::Car(car) => match p.system {
                    System::Chassis => chassis_bench(ui, p, v, car),
                    System::Powertrain => powertrain_bench(ui, car),
                    System::Steering => steering_bench(ui, car),
                    _ => {}
                },
                Drive::Tank { fields, wheels } => crate::drivingtank::bench(ui, p, v, fields, wheels),
                Drive::Boat { fields } => crate::drivingboat::bench(ui, p, v, fields),
                Drive::Helicopter { player, ai, rotors, .. } => crate::drivingheli::bench(ui, p, player, ai.as_ref(), rotors),
                Drive::Plane { .. } => crate::drivingplane::bench(ui),
            },
        });
    acts
}

/// A small label on a dark plate, for a mark over the model.
pub(crate) fn tag(painter: &egui::Painter, at: egui::Pos2, text: String, color: Color32) {
    let galley = painter.layout_no_wrap(text, theme::font(&egui::TextStyle::Monospace), color);
    let rect = egui::Rect::from_min_size(at, galley.size()).expand2(egui::vec2(4.0, 2.0));
    painter.rect_filled(rect, egui::CornerRadius::same(3), Color32::from_rgba_unmultiplied(14, 16, 20, 215));
    painter.galley(at, galley, color);
}

/// What the page draws over the viewport: the open system's overlay toggles, the marks they turn
/// on, and the Trailer picker. `proj` is the renderer's view-projection for this frame and `screen`
/// the window it fills; `vp` is the part of it the page leaves the model.
pub fn viewport(ctx: &egui::Context, p: &Panel, proj: glam::Mat4, screen: egui::Vec2, vp: egui::Rect) -> Vec<Act> {
    let mut acts = Vec::new();
    let (Some(v), Some(data)) = (p.vehicle(), p.data()) else { return acts };
    let offered = Overlay::of(v.drive.kind(), p.system);
    if !offered.is_empty() {
        egui::Area::new(egui::Id::new("driving_overlays")).fixed_pos(vp.left_top() + egui::vec2(14.0, 44.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for o in offered {
                    if theme::toggle_chip(ui, o.label(), p.show[*o as usize]).clicked() {
                        acts.push(Act::Overlay(*o));
                    }
                }
                if let Some(problem) = &p.lock_problem {
                    ui.label(egui::RichText::new(problem).small().color(theme::BAD));
                }
            });
        });
    }
    let (has_hitch, has_exhaust) = (p.hitch().is_some(), !p.exhaust_parts().is_empty());
    let has_rotors = matches!(&v.drive, Drive::Helicopter { rotors, .. } if !rotors.is_empty());
    if has_hitch || has_exhaust || has_rotors {
        egui::Area::new(egui::Id::new("driving_state")).fixed_pos(vp.left_bottom() + egui::vec2(14.0, -36.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if has_rotors {
                    ui.label(egui::RichText::new("Rotors").small().color(theme::DIM));
                    let mut on = p.rotors;
                    egui::ComboBox::from_id_salt("driving_rotors").selected_text(if on { "Spinning" } else { "Stopped" }).width(84.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut on, false, "Stopped");
                        ui.selectable_value(&mut on, true, "Spinning");
                    });
                    if on != p.rotors {
                        acts.push(Act::Rotors(on));
                    }
                    ui.add_space(10.0);
                }
                if has_hitch {
                    ui.label(egui::RichText::new("Trailer").small().color(theme::DIM));
                    let name = |key: u32| data.template_names.get(&key).cloned().unwrap_or_else(|| format!("0x{key:08X}"));
                    let mut chosen = p.trailer;
                    egui::ComboBox::from_id_salt("driving_trailer")
                        .selected_text(p.trailer.map_or("None".to_string(), name))
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut chosen, None, "None");
                            let mut keys = data.fleet.trailers.clone();
                            keys.sort_by_key(|k| name(*k));
                            for key in keys {
                                ui.selectable_value(&mut chosen, Some(key), name(key));
                            }
                        });
                    if chosen != p.trailer {
                        acts.push(Act::Trailer(chosen.or(p.trailer)));
                    }
                    if let Some(problem) = &p.trailer_problem {
                        ui.label(egui::RichText::new(problem).small().color(theme::BAD));
                    }
                    ui.add_space(10.0);
                }
                if has_exhaust {
                    ui.label(egui::RichText::new("Exhaust").small().color(theme::DIM));
                    let mut on = p.exhaust;
                    egui::ComboBox::from_id_salt("driving_exhaust").selected_text(if on { "On" } else { "Off" }).width(60.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut on, false, "Off");
                        ui.selectable_value(&mut on, true, "On");
                    });
                    if on != p.exhaust {
                        acts.push(Act::Exhaust(on));
                    }
                    if let Some(problem) = &p.exhaust_problem {
                        ui.label(egui::RichText::new(problem).small().color(theme::BAD));
                    }
                }
            });
        });
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("driving_marks"))).with_clip_rect(vp);
    let on = |o: Overlay| offered.contains(&o) && p.show[o as usize];
    let at = |pos: [f32; 3]| crate::layerview::project(proj, glam::Vec3::from(pos), screen).map(|(px, _)| px);
    if on(Overlay::Hardpoints) {
        for node in coupling_rows(p, v) {
            let Some(c) = p.nodes.get(&node).and_then(|pos| at(*pos)) else { continue };
            if p.selected_node == Some(node) {
                diamond(&painter, c);
                painter.circle_stroke(c, 10.0, Stroke::new(1.6, theme::BRASS));
                let pos = p.nodes.get(&node).map(|pos| position(*pos)).unwrap_or_default();
                tag(&painter, c + egui::vec2(14.0, -7.0), format!("{} · {pos}", p.node_label(node)), theme::BRASS);
            } else {
                square(&painter, c);
            }
        }
    }
    let marks = Marks { painter: &painter, at: &at, on: &on };
    let car = match &v.drive {
        Drive::Car(car) => car,
        Drive::Tank { fields, wheels } => {
            crate::drivingtank::marks(&marks, p, fields, wheels);
            return acts;
        }
        Drive::Boat { .. } | Drive::Plane { .. } => return acts,
        Drive::Helicopter { rotors, winch, .. } => {
            crate::drivingheli::marks(&marks, p, rotors, winch.as_ref());
            return acts;
        }
    };
    let wheels: Vec<(&Wheel, [f32; 3])> = car.wheels.iter().filter_map(|w| p.nodes.get(&w.susp_bone).map(|pos| (w, *pos))).collect();
    if on(Overlay::Wheels) || on(Overlay::Travel) {
        for (w, pos) in &wheels {
            let Some(c) = at(*pos) else { continue };
            painter.circle_stroke(c, 7.0, Stroke::new(1.6, theme::BRASS));
            painter.circle_filled(c, 2.0, theme::BRASS);
            if !on(Overlay::Travel) {
                continue;
            }
            let end = if w.rear { "Rear" } else { "Front" };
            let length: f32 = [format!("{end}WheelSuspExpLength"), format!("{end}WheelSuspCmpLength"), "ExtraInvisibleSpringLength".to_string()]
                .iter()
                .filter_map(|name| car.fields.num(name).ok())
                .sum();
            let Some(low) = at([pos[0], pos[1] - length, pos[2]]) else { continue };
            painter.line_segment([c, low], Stroke::new(2.0, theme::GOOD));
            painter.line_segment([low + egui::vec2(-5.0, 0.0), low + egui::vec2(5.0, 0.0)], Stroke::new(2.0, theme::GOOD));
            tag(&painter, low + egui::vec2(8.0, -6.0), number((length * 1000.0).round() / 1000.0), theme::GOOD);
        }
    }
    if on(Overlay::TorqueSplit) {
        if let (Ok(g), Ok(axles)) = (car.fields.gearbox(), car.fields.axles(&car.wheels)) {
            for (w, pos) in &wheels {
                let Some(c) = at(*pos) else { continue };
                painter.circle_stroke(c, 7.0, Stroke::new(1.6, theme::BRASS));
                tag(&painter, c + egui::vec2(10.0, -7.0), number(drivingmath::wheel_share(&axles, g.rear_share, w.rear)), theme::BRASS);
            }
        }
    }
    acts
}

/// What a vehicle class draws its viewport marks with: the painter, a model-space point's place on
/// screen, and whether an overlay is offered and on.
pub(crate) struct Marks<'a> {
    pub(crate) painter: &'a egui::Painter,
    pub(crate) at: &'a dyn Fn([f32; 3]) -> Option<egui::Pos2>,
    pub(crate) on: &'a dyn Fn(Overlay) -> bool,
}

/// A bench's header row: a title in the stencil face and a caption beside it.
pub(crate) fn bench_header(ui: &mut egui::Ui, parts: &[(&str, &str)]) {
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(ui.max_rect().size());
        for (title, caption) in parts {
            ui.label(theme::disp_text(title.to_uppercase(), theme::style_eyebrow(), theme::DIM));
            ui.label(egui::RichText::new(*caption).monospace().color(theme::DIM));
            ui.add_space(14.0);
        }
    });
    let y = ui.cursor().top();
    ui.painter().line_segment([egui::pos2(ui.max_rect().left() - 10.0, y), egui::pos2(ui.max_rect().right() + 10.0, y)], Stroke::new(1.0, theme::LINE));
}

/// The rest of the bench as one painted area.
pub(crate) fn bench_area(ui: &mut egui::Ui) -> (egui::Rect, egui::Painter) {
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
    // The painter is the bench's, not the area's: a bench draws its key into the header row above.
    (rect.shrink2(egui::vec2(0.0, 6.0)), ui.painter().clone())
}

/// Splits a rect left and right at a fraction of its width, with a gap.
pub(crate) fn split(rect: egui::Rect, at: f32) -> (egui::Rect, egui::Rect) {
    let x = rect.left() + rect.width() * at;
    (egui::Rect::from_min_max(rect.min, egui::pos2(x - 10.0, rect.bottom())), egui::Rect::from_min_max(egui::pos2(x + 10.0, rect.top()), rect.max))
}

fn chassis_bench(ui: &mut egui::Ui, p: &Panel, v: &Vehicle, car: &Car) {
    let placed: Vec<(&Wheel, [f32; 3])> = car.wheels.iter().filter_map(|w| p.nodes.get(&w.susp_bone).map(|at| (w, *at))).collect();
    let mut axles: Vec<f32> = Vec::new();
    for (_, at) in &placed {
        if !axles.iter().any(|z| (z - at[2]).abs() < 0.05) {
            axles.push(at[2]);
        }
    }
    axles.sort_by(|a, b| b.total_cmp(a));
    bench_header(ui, &[("Blueprint", &format!("{} wheels · {} axles · metres", car.wheels.len(), axles.len()))]);
    let (area, painter) = bench_area(ui);
    let (Ok(front_radius), Ok(rear_radius)) = (car.fields.num("FrontWheelRadius"), car.fields.num("RearWheelRadius")) else { return };
    if placed.is_empty() {
        return;
    }
    let radius = |w: &Wheel| if w.rear { rear_radius } else { front_radius };
    // The marked nodes: the hitch, winch points, exhausts, seats.
    let exhaust = |u: &NodeUse| u.class == "VehiclePart" && p.data().and_then(|d| drivingdata::part_kind(&d.we, u.kind)) == Some("Exhaust");
    let of = |pick: &dyn Fn(&NodeUse) -> bool| -> Vec<[f32; 3]> {
        let mut seen: Vec<u32> = Vec::new();
        v.uses.iter().filter(|u| pick(u)).filter_map(|u| (!seen.contains(&u.node)).then(|| seen.push(u.node)).and(p.nodes.get(&u.node).copied())).collect()
    };
    let winch = of(&|u| u.class == "ConnectPoint");
    let exhausts = of(&exhaust);
    let seats = of(&|u| u.class == "SeatLink");
    let hitch = p.hitch();
    // Extents, in metres.
    let ground = placed.iter().map(|(w, at)| at[1] - radius(w)).fold(f32::MAX, f32::min);
    let marks = winch.iter().chain(&exhausts).chain(&seats).chain(hitch.iter());
    let (mut z_lo, mut z_hi, mut top, mut wide) = (f32::MAX, f32::MIN, ground, 0.0_f32);
    for (w, at) in &placed {
        z_lo = z_lo.min(at[2] - radius(w));
        z_hi = z_hi.max(at[2] + radius(w));
        top = top.max(at[1] + radius(w));
        wide = wide.max(at[0].abs());
    }
    for at in marks {
        z_lo = z_lo.min(at[2]);
        z_hi = z_hi.max(at[2]);
        top = top.max(at[1]);
        wide = wide.max(at[0].abs());
    }
    let (z_lo, z_hi, top, wide) = (z_lo - 0.4, z_hi + 0.4, top + 0.3, wide + 0.5);
    let (left, right) = split(area, 0.5);
    let inset = |r: egui::Rect| egui::Rect::from_min_max(r.min + egui::vec2(0.0, 16.0), r.max - egui::vec2(0.0, 34.0));
    let scale = Plot::to_scale(inset(left), (z_hi, z_lo), (ground, top)).per_unit().abs().min(Plot::to_scale(inset(right), (z_hi, z_lo), (-wide, wide)).per_unit().abs());
    let side = Plot::at_scale(inset(left), (z_hi, z_lo), (ground, top), scale);
    let plan = Plot::at_scale(inset(right), (z_hi, z_lo), (-wide, wide), scale);
    let (tire, hub, dim) = (Stroke::new(1.4, theme::TX), theme::TX, Stroke::new(1.0, theme::FAINT));
    let title = |r: egui::Rect, t: &str| {
        painter.text(r.left_top(), Align2::LEFT_TOP, t, theme::font(&theme::style_eyebrow()), theme::DIM);
    };
    title(left, "SIDE");
    title(right, "TOP");

    // Side view: the ground, each wheel, the marked nodes with their heights.
    painter.line_segment([egui::pos2(left.left(), side.y(ground)), egui::pos2(left.right() - 10.0, side.y(ground))], Stroke::new(1.0, theme::LINE2));
    for (w, at) in &placed {
        let c = side.at(at[2], at[1]);
        painter.circle(c, radius(w) * scale, theme::G2, tire);
        painter.circle_filled(c, 2.2, hub);
    }
    let height = |at: [f32; 3], col: Color32| {
        plot::label(&painter, side.at(at[2], at[1]) + egui::vec2(8.0, 0.0), Align2::LEFT_CENTER, number((at[1] * 100.0).round() / 100.0), col);
    };
    for at in &winch {
        square(&painter, side.at(at[2], at[1]));
    }
    if let Some(at) = winch.iter().copied().max_by(|a, b| a[1].total_cmp(&b[1])) {
        height(at, theme::DIM);
    }
    for at in &exhausts {
        triangle(&painter, side.at(at[2], at[1]));
    }
    if let Some(at) = exhausts.first() {
        height(*at, theme::DIM);
    }
    if let Some(at) = hitch {
        diamond(&painter, side.at(at[2], at[1]));
        plot::label(&painter, side.at(at[2], at[1]) + egui::vec2(0.0, -8.0), Align2::CENTER_BOTTOM, number((at[1] * 100.0).round() / 100.0), theme::BRASS);
    }
    // Wheelbase: the distance between neighbouring axles, under the ground line.
    let rule = side.y(ground) + 14.0;
    for z in &axles {
        painter.line_segment([egui::pos2(side.x(*z), rule - 6.0), egui::pos2(side.x(*z), rule + 6.0)], dim);
    }
    for pair in axles.windows(2) {
        painter.line_segment([egui::pos2(side.x(pair[0]), rule), egui::pos2(side.x(pair[1]), rule)], dim);
        plot::label(&painter, egui::pos2((side.x(pair[0]) + side.x(pair[1])) / 2.0, rule + 8.0), Align2::CENTER_TOP, number(((pair[0] - pair[1]) * 100.0).round() / 100.0), theme::TX);
    }

    // Top view: the centre line, each axle with its wheels and track, the marked nodes.
    let mid = plan.y(0.0);
    painter.extend(egui::Shape::dashed_line(&[egui::pos2(right.left(), mid), egui::pos2(right.right(), mid)], Stroke::new(1.0, theme::LINE), 3.0, 4.0));
    for (i, z) in axles.iter().enumerate() {
        let on: Vec<&(&Wheel, [f32; 3])> = placed.iter().filter(|(_, at)| (at[2] - z).abs() < 0.05).collect();
        let (lo, hi) = on.iter().fold((f32::MAX, f32::MIN), |(l, h), (_, at)| (l.min(at[0]), h.max(at[0])));
        painter.line_segment([plan.at(*z, lo), plan.at(*z, hi)], Stroke::new(1.4, theme::FAINT));
        for (w, at) in &on {
            let size = egui::vec2(2.0 * radius(w) * scale, 10.0);
            painter.rect(egui::Rect::from_center_size(plan.at(at[2], at[0]), size), egui::CornerRadius::same(3), theme::G2, tire, egui::StrokeKind::Middle);
        }
        let track = number(((hi - lo) * 100.0).round() / 100.0);
        let (offset, align) = if i == 0 { (8.0, Align2::LEFT_CENTER) } else { (-8.0, Align2::RIGHT_CENTER) };
        if on.len() > 1 && (i == 0 || i == 1) {
            plot::label(&painter, egui::pos2(plan.x(*z) + offset, mid + 9.0), align, track, theme::TX);
        }
        plot::label(&painter, egui::pos2(plan.x(*z), plan.area.bottom() + 6.0), Align2::CENTER_TOP, number((z * 100.0).round() / 100.0), theme::DIM);
    }
    for at in &winch {
        square(&painter, plan.at(at[2], at[0]));
    }
    for at in &exhausts {
        triangle(&painter, plan.at(at[2], at[0]));
    }
    for at in &seats {
        painter.circle_stroke(plan.at(at[2], at[0]), 4.0, Stroke::new(1.4, theme::DIM));
    }
    if let Some(at) = hitch {
        diamond(&painter, plan.at(at[2], at[0]));
        plot::label(&painter, egui::pos2(plan.x(at[2]), plan.area.bottom() + 20.0), Align2::CENTER_TOP, number((at[2] * 100.0).round() / 100.0), theme::BRASS);
    }
    // The key, at the right of the header row.
    let key_y = area.top() - 19.0;
    let mut x = area.right();
    for (name, mark) in [("seat", 3), ("exhaust", 2), ("winch point", 1), ("hitch", 0)] {
        let galley = painter.layout_no_wrap(name.to_string(), theme::font(&egui::TextStyle::Small), theme::DIM);
        x -= galley.size().x;
        painter.galley(egui::pos2(x, key_y - galley.size().y / 2.0), galley, theme::DIM);
        x -= 9.0;
        let c = egui::pos2(x, key_y);
        match mark {
            0 => diamond(&painter, c),
            1 => square(&painter, c),
            2 => triangle(&painter, c),
            _ => {
                painter.circle_stroke(c, 3.5, Stroke::new(1.4, theme::DIM));
            }
        }
        x -= 16.0;
    }
}

pub(crate) fn diamond(p: &egui::Painter, c: egui::Pos2) {
    let r = 5.0;
    p.add(egui::Shape::convex_polygon(vec![c + egui::vec2(0.0, -r), c + egui::vec2(r, 0.0), c + egui::vec2(0.0, r), c + egui::vec2(-r, 0.0)], theme::BRASS, Stroke::NONE));
}

pub(crate) fn square(p: &egui::Painter, c: egui::Pos2) {
    p.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0)), egui::CornerRadius::ZERO, Stroke::new(1.4, theme::DIM), egui::StrokeKind::Middle);
}

pub(crate) fn triangle(p: &egui::Painter, c: egui::Pos2) {
    let pts = vec![c + egui::vec2(0.0, -4.5), c + egui::vec2(4.5, 3.5), c + egui::vec2(-4.5, 3.5), c + egui::vec2(0.0, -4.5)];
    p.add(egui::Shape::line(pts, Stroke::new(1.4, theme::DIM)));
}

/// The chart's top speed: the next multiple of 20 km/h above the cap.
pub(crate) fn speed_axis(max_speed: f32) -> f32 {
    ((max_speed / 20.0).floor() + 1.0) * 20.0
}

pub(crate) fn labelled(values: Vec<f32>) -> Vec<(f32, String)> {
    values.into_iter().map(|v| (v, number(v))).collect()
}

fn powertrain_bench(ui: &mut egui::Ui, car: &Car) {
    bench_header(ui, &[("Gearing", "rpm against km/h · full throttle")]);
    let (area, painter) = bench_area(ui);
    let (Ok(g), Ok(a)) = (car.fields.gearbox(), car.fields.axles(&car.wheels)) else { return };
    let (gearing, torque) = split(area, 0.63);
    plot::label(&painter, egui::pos2(torque.left(), area.top() - 19.0), Align2::LEFT_CENTER, "TORQUE against rpm", theme::DIM);

    // Gearing: each gear's line, the full-throttle run over them, the shift rpms, the cap.
    let x_max = speed_axis(g.max_speed);
    let plot_g = Plot::chart(gearing, (0.0, x_max), (0.0, g.max_rpm));
    let rpm_step = if g.max_rpm > 12000.0 { 4000.0 } else { 2000.0 };
    plot::axes(&painter, &plot_g, &labelled(plot::ticks(20.0, x_max, 20.0)), &labelled(plot::ticks(0.0, g.max_rpm, rpm_step)));
    let clip = painter.with_clip_rect(plot_g.area.expand(1.0));
    let fade = g.max_speed * (1.0 - drivingmath::CAP_FADE);
    clip.rect_filled(egui::Rect::from_min_max(plot_g.at(fade, g.max_rpm), plot_g.at(g.max_speed, 0.0)), egui::CornerRadius::ZERO, theme::BRASS_SOFT);
    plot::dashed_up(&clip, &plot_g, g.max_speed, theme::BRASS_DK);
    plot::dashed_across(&clip, &plot_g, g.upshift_rpm, theme::GOOD_DK);
    plot::dashed_across(&clip, &plot_g, g.downshift_rpm, theme::FAINT);
    plot::label(&clip, plot_g.at(0.0, g.upshift_rpm) + egui::vec2(4.0, -2.0), Align2::LEFT_BOTTOM, format!("up {}", number(g.upshift_rpm)), theme::GOOD);
    plot::label(&clip, plot_g.at(0.0, g.downshift_rpm) + egui::vec2(4.0, -2.0), Align2::LEFT_BOTTOM, format!("down {}", number(g.downshift_rpm)), theme::DIM);
    for (i, ratio) in g.forward.iter().enumerate() {
        let slope = drivingmath::rpm_per_kmh(&g, &a, *ratio);
        if slope <= 0.0 {
            continue;
        }
        let end = (g.max_rpm / slope).min(x_max);
        plot::line(&clip, &plot_g, &[(0.0, 0.0), (end, end * slope)], Stroke::new(1.0, theme::LINE2));
        let at = (0.81 * g.max_rpm / slope).min(x_max - 2.5);
        plot::label(&clip, plot_g.at(at, at * slope) + egui::vec2(4.0, 0.0), Align2::LEFT_TOP, i + 1, theme::DIM);
    }
    let path = drivingmath::full_throttle_path(&g, &a);
    plot::line(&clip, &plot_g, &path, Stroke::new(2.0, theme::BRASS));
    if let Some(&(kmh, rpm)) = path.last() {
        clip.circle_filled(plot_g.at(kmh, rpm), 3.4, theme::BRASS);
        plot::label(&clip, plot_g.at(kmh, rpm) + egui::vec2(7.0, 6.0), Align2::LEFT_TOP, number(rpm.round()), theme::BRASS);
    }
    plot::label(&clip, plot_g.at(fade, g.max_rpm) + egui::vec2(-4.0, 4.0), Align2::RIGHT_TOP, format!("cap {}", number(g.max_speed)), theme::BRASS);

    // Torque: the curve, friction under it, and what is left at full throttle.
    if g.engine_torque <= 0.0 || g.max_rpm <= g.min_rpm {
        return;
    }
    let plot_t = Plot::chart(torque, (0.0, g.max_rpm), (0.0, g.engine_torque * 1.125));
    plot::axes(
        &painter,
        &plot_t,
        &labelled(vec![g.min_rpm, g.opt_rpm, g.max_rpm]),
        &labelled(vec![0.0, g.engine_torque / 2.0, g.engine_torque]),
    );
    let samples: Vec<f32> = (0..=40).map(|i| g.min_rpm + (g.max_rpm - g.min_rpm) * i as f32 / 40.0 * 0.9999).collect();
    let curve = |f: &dyn Fn(f32, f32) -> f32| -> Vec<(f32, f32)> {
        samples.iter().map(|&rpm| { let (t, fr) = drivingmath::torque_and_friction(&g, rpm); (rpm, f(t, fr)) }).collect()
    };
    let (t, fr, net) = (curve(&|t, _| t), curve(&|_, f| f), curve(&|t, f| t - f));
    plot::line(&painter, &plot_t, &t, Stroke::new(1.4, theme::DIM));
    plot::line(&painter, &plot_t, &fr, Stroke::new(1.4, theme::FAINT));
    plot::line(&painter, &plot_t, &net, Stroke::new(2.0, theme::BRASS));
    plot::label(&painter, plot_t.at(t[0].0, t[0].1) + egui::vec2(4.0, -3.0), Align2::LEFT_BOTTOM, "torque", theme::DIM);
    plot::label(&painter, plot_t.at(net[26].0, net[26].1) + egui::vec2(-6.0, 8.0), Align2::RIGHT_TOP, "less friction", theme::BRASS);
    plot::label(&painter, plot_t.at(fr[0].0, fr[0].1) + egui::vec2(4.0, -4.0), Align2::LEFT_BOTTOM, "friction", theme::DIM);
}

fn steering_bench(ui: &mut egui::Ui, car: &Car) {
    let f = &car.fields;
    let (Ok(s), Ok(inertia)) = (f.steering(), f.num("SteeringInertia")) else { return };
    let rate_mode = inertia < 0.0;
    bench_header(ui, &[("Lock", "rad against km/h")]);
    let (area, painter) = bench_area(ui);
    let (lock_r, resp_r) = split(area, 0.6);
    let caption = if rate_mode { "RESPONSE rad against seconds" } else { "RESPONSE share of lock against seconds" };
    plot::label(&painter, egui::pos2(resp_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, caption, theme::DIM);
    // The key, beside the title.
    let mut x = area.left() + 190.0;
    for (name, col) in [("on throttle", theme::BRASS), ("coasting", theme::TX), ("braking", theme::FAINT)] {
        painter.line_segment([egui::pos2(x, area.top() - 19.0), egui::pos2(x + 14.0, area.top() - 19.0)], Stroke::new(2.0, col));
        let galley = painter.layout_no_wrap(name.to_string(), theme::font(&egui::TextStyle::Small), theme::DIM);
        let w = galley.size().x;
        painter.galley(egui::pos2(x + 19.0, area.top() - 19.0 - galley.size().y / 2.0), galley, theme::DIM);
        x += w + 33.0;
    }

    // Lock against road speed, for each pedal state.
    let x_max = speed_axis(s.max_speed);
    let y_max = s.max_angle * 1.125;
    let lock = Plot::chart(lock_r, (0.0, x_max), (0.0, y_max));
    plot::axes(&painter, &lock, &labelled(plot::ticks(20.0, x_max, 20.0)), &labelled(plot::ticks(0.0, y_max, 0.2)));
    plot::dashed_up(&painter, &lock, s.max_speed, theme::BRASS_DK);
    plot::label(&painter, lock.at(s.max_speed, y_max) + egui::vec2(-4.0, 4.0), Align2::RIGHT_TOP, format!("max speed {}", number(s.max_speed)), theme::BRASS);
    for (pedals, col, width) in [(Pedals::Braking, theme::FAINT, 1.6), (Pedals::Coasting, theme::TX, 1.6), (Pedals::Throttle, theme::BRASS, 2.0)] {
        let end = drivingmath::lock(&s, pedals, s.max_speed);
        plot::line(&painter, &lock, &[(0.0, s.max_angle), (s.max_speed, end), (x_max, end)], Stroke::new(width, col));
        plot::label(&painter, lock.at(s.max_speed, end) + egui::vec2(6.0, -3.0), Align2::LEFT_BOTTOM, format!("{end:.2}"), if col == theme::FAINT { theme::DIM } else { col });
    }
    plot::label(&painter, lock.at(0.0, s.max_angle) + egui::vec2(6.0, -4.0), Align2::LEFT_BOTTOM, format!("{} rad · {:.1}°", number(s.max_angle), s.max_angle.to_degrees()), theme::TX);

    // Response over three seconds.
    let seconds = 3.0;
    let times: Vec<f32> = (0..=60).map(|i| seconds * i as f32 / 60.0).collect();
    if rate_mode {
        let (Ok(speed), Ok(low), Ok(limit)) = (f.num("SteerSpeed"), f.num("LowSpeedSteerSpeed"), f.num("LowSpeedLimit")) else { return };
        let resp = Plot::chart(resp_r, (0.0, seconds), (0.0, y_max));
        plot::axes(&painter, &resp, &labelled(plot::ticks(1.0, seconds, 1.0)), &labelled(plot::ticks(0.0, y_max, 0.2)));
        for (kmh, col, name) in [(limit, theme::TX, format!("at {} km/h", number(limit))), (0.0, theme::BRASS, "at rest".to_string())] {
            let rate = drivingmath::steer_rate(speed, low, limit, &s, kmh);
            let top = drivingmath::lock(&s, Pedals::Throttle, kmh);
            let pts: Vec<(f32, f32)> = times.iter().map(|&t| (t, (rate * t).min(top))).collect();
            plot::line(&painter, &resp, &pts, Stroke::new(if kmh == 0.0 { 2.0 } else { 1.6 }, col));
            plot::label(&painter, resp.at(seconds, top) + egui::vec2(-4.0, 4.0), Align2::RIGHT_TOP, name, col);
        }
        return;
    }
    let (Ok(no_input), Ok(_)) = (f.num("SteerNoInputInertia"), f.num("SteerDiffSignInertia")) else { return };
    let resp = Plot::chart(resp_r, (0.0, seconds), (0.0, 1.0));
    plot::axes(&painter, &resp, &labelled(plot::ticks(1.0, seconds, 1.0)), &labelled(vec![0.0, 0.5, 1.0]));
    let release: Vec<(f32, f32)> = times.iter().map(|&t| (t, drivingmath::remaining(no_input, t))).collect();
    let turn_in: Vec<(f32, f32)> = times.iter().map(|&t| (t, 1.0 - drivingmath::remaining(inertia, t))).collect();
    plot::line(&painter, &resp, &release, Stroke::new(1.6, theme::TX));
    plot::line(&painter, &resp, &turn_in, Stroke::new(2.0, theme::BRASS));
    plot::label(&painter, resp.at(1.4, 0.62), Align2::LEFT_TOP, format!("turning in · {}", number(inertia)), theme::BRASS);
    plot::label(&painter, resp.at(1.4, 0.22), Align2::LEFT_TOP, format!("released · {}", number(no_input)), theme::TX);
}

/// The nodes the Coupling table lists: every named hardpoint and every node a record names.
fn coupling_rows(p: &Panel, v: &Vehicle) -> Vec<u32> {
    let wheel = |class: &str| class == drivingdata::WHEEL_CLASS || class == drivingdata::FAKE_WHEEL_CLASS;
    let mut rows: Vec<u32> = p
        .nodes
        .keys()
        .copied()
        .filter(|h| p.node_names.get(h).is_some_and(|n| n.starts_with("hp_")) || v.uses.iter().any(|u| u.node == *h && !wheel(u.class)))
        .collect();
    let hitch = pandemic_hash_m2(HITCH);
    let unnamed = |h: &u32| !p.node_names.contains_key(h);
    rows.sort_by(|a, b| {
        (*a != hitch).cmp(&(*b != hitch)).then(unnamed(a).cmp(&unnamed(b))).then_with(|| p.node_label(*a).cmp(&p.node_label(*b)))
    });
    rows
}

/// What names a node: each record's class and link name.
fn used_by(p: &Panel, v: &Vehicle, node: u32) -> Vec<String> {
    v.uses
        .iter()
        .filter(|u| u.node == node)
        .map(|u| {
            let kind = p.data().and_then(|d| drivingdata::part_kind(&d.we, u.kind)).filter(|_| u.class == "VehiclePart");
            let name = (u.name != 0).then(|| p.node_label(u.name));
            match (kind, name) {
                (Some(kind), _) => format!("{} · {kind}", u.class),
                (None, Some(name)) => format!("{} · {name}", u.class),
                (None, None) => u.class.to_string(),
            }
        })
        .collect()
}

fn coupling_bench(ui: &mut egui::Ui, p: &Panel, v: &Vehicle) -> Vec<Act> {
    let mut acts = Vec::new();
    let rows = coupling_rows(p, v);
    bench_header(ui, &[("Hardpoints", &format!("{} nodes", rows.len()))]);
    let detail_w = 230.0;
    ui.horizontal_top(|ui| {
        let table_w = ui.available_width() - detail_w - 10.0;
        ui.allocate_ui_with_layout(egui::vec2(table_w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
            let cols = [150.0, 126.0];
            ui.horizontal(|ui| {
                for (text, w) in [("NODE", cols[0]), ("POSITION", cols[1]), ("USED BY", 80.0)] {
                    ui.allocate_ui_with_layout(egui::vec2(w, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(ui.max_rect().size());
                        ui.label(theme::disp_text(text, theme::style_eyebrow(), theme::DIM));
                    });
                }
            });
            theme::scroll_vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for node in rows {
                    let on = p.selected_node == Some(node);
                    let col = if on { theme::BRASS } else { theme::TX };
                    let used = used_by(p, v, node);
                    let resp = theme::row_chip(ui, if on { theme::BRASS_SOFT } else { Color32::TRANSPARENT }, Color32::TRANSPARENT, |ui| {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2(cols[0] - 9.0, 16.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(ui.max_rect().size());
                                ui.add(egui::Label::new(egui::RichText::new(p.node_label(node)).monospace().color(col)).truncate());
                            });
                            ui.allocate_ui_with_layout(egui::vec2(cols[1], 16.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(ui.max_rect().size());
                                ui.label(egui::RichText::new(p.nodes.get(&node).map(|at| position(*at)).unwrap_or_default()).monospace().color(theme::DIM));
                            });
                            let text = if used.is_empty() { "—".to_string() } else { used.join(" · ") };
                            ui.add(egui::Label::new(egui::RichText::new(text).small().color(if used.is_empty() { theme::DIM } else { col })).truncate());
                        });
                    });
                    if resp.clicked() {
                        acts.push(Act::Node(node));
                    }
                }
            });
        });
        ui.separator();
        ui.allocate_ui_with_layout(egui::vec2(detail_w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.add_space(8.0);
            let Some(node) = p.selected_node else { return };
            let row = |ui: &mut egui::Ui, label: &str, value: String, detail: Option<String>, lit: bool| {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(62.0, 16.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(ui.max_rect().size());
                        ui.label(theme::disp_text(label, theme::style_eyebrow(), if lit { theme::BRASS } else { theme::DIM }));
                    });
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(value).truncate());
                        if let Some(detail) = detail {
                            ui.add(egui::Label::new(egui::RichText::new(detail).monospace().color(theme::DIM)).truncate());
                        }
                    });
                });
                ui.add_space(4.0);
            };
            row(ui, "NODE", p.node_label(node), Some(format!("0x{node:08X}")), true);
            if let Some(at) = p.nodes.get(&node) {
                row(ui, "AT", position(*at), None, false);
            }
            let used = used_by(p, v, node);
            row(ui, "USED BY", format!("{} records", used.len()), (!used.is_empty()).then(|| used.join(" · ")), false);
            if let (Some(trailer), true, Some(at)) = (p.trailer, node == pandemic_hash_m2(HITCH), p.hitch()) {
                let name = p.data().and_then(|d| d.template_names.get(&trailer).cloned()).unwrap_or_default();
                row(ui, "TRAILER", name, Some(format!("origin height {}", number((at[1] * 100.0).round() / 100.0))), false);
            }
            // The vehicle models whose hierarchy has the node.
            let models = p.models_with(node);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(62.0, 16.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_size(ui.max_rect().size());
                    ui.label(theme::disp_text("MODELS", theme::style_eyebrow(), theme::DIM));
                });
                ui.label(models.len().to_string());
            });
            theme::scroll_vertical().id_salt("driving_node_models").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for name in models {
                    ui.add(egui::Label::new(egui::RichText::new(name).monospace().color(theme::DIM)).truncate());
                }
            });
        });
    });
    acts
}

// ── inspector ────────────────────────────────────────────────────────────────────────────────────

/// A value with its place in the fleet's range.
pub(crate) fn ranged(ui: &mut egui::Ui, fleet: &Fleet, label_w: f32, label: &str, text: String, field: Option<(&str, f32)>, lit: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.allocate_ui_with_layout(egui::vec2(label_w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(ui.max_rect().size());
            ui.add(egui::Label::new(egui::RichText::new(label).color(theme::DIM)).truncate());
        });
        let text = egui::RichText::new(text).color(if lit { theme::TX } else { theme::DIM });
        let place = field.and_then(|(name, v)| fleet.place(name, v));
        match place {
            // A single number, then its tick over the room that is left.
            Some(_) => {
                ui.label(text);
                let w = ui.available_width().max(8.0);
                theme::range_tick(ui, w, place);
            }
            // A value with no tick has the rest of the row, and is cut short there.
            None => {
                ui.add(egui::Label::new(text).truncate());
            }
        }
    });
}

/// A named float field as a ranged row; nothing for a field the record lacks.
pub(crate) fn field_row(ui: &mut egui::Ui, fleet: &Fleet, f: &Fields, label: &str, name: &str, unit: &str) {
    if let Ok(v) = f.num(name) {
        let text = if unit.is_empty() { number(v) } else { format!("{} {unit}", number(v)) };
        ranged(ui, fleet, 128.0, label, text, Some((name, v)), true);
    }
}

/// A row of three floats with no tick: a three-float field, or three fields of one kind.
pub(crate) fn triple_row(ui: &mut egui::Ui, fleet: &Fleet, label: &str, values: [f32; 3]) {
    ranged(ui, fleet, 96.0, label, format!("{} · {} · {}", number(values[0]), number(values[1]), number(values[2])), None, true);
}

/// Rows of two values side by side under two headings, each with its tick: a value and where it
/// sits in its own range. A side with no value is left empty.
pub(crate) fn paired(ui: &mut egui::Ui, headings: [&str; 2], rows: &[(&str, [Option<(f32, Option<f32>)>; 2])]) {
    let half = ((ui.available_width() - 104.0) / 2.0 - 6.0).max(60.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(110.0);
        for t in headings {
            ui.allocate_ui_with_layout(egui::vec2(half, 16.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_size(ui.max_rect().size());
                ui.label(theme::disp_text(t, theme::style_eyebrow(), theme::DIM));
            });
        }
    });
    for (label, sides) in rows {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.allocate_ui_with_layout(egui::vec2(104.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_size(ui.max_rect().size());
                ui.add(egui::Label::new(egui::RichText::new(*label).color(theme::DIM)).truncate());
            });
            for side in sides {
                ui.allocate_ui_with_layout(egui::vec2(half, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_size(ui.max_rect().size());
                    let Some((v, place)) = side else { return };
                    ui.allocate_ui_with_layout(egui::vec2(40.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_size(ui.max_rect().size());
                        ui.label(number(*v));
                    });
                    if place.is_some() {
                        let w = ui.available_width().max(8.0);
                        theme::range_tick(ui, w, *place);
                    }
                });
            }
        });
    }
}

pub(crate) fn unread(ui: &mut egui::Ui, names: &[&str]) {
    theme::card(ui, "Not read by the PC exe", None, |ui| {
        for name in names {
            ui.add(egui::Label::new(egui::RichText::new(*name).monospace().color(theme::DIM)).truncate());
        }
    });
}

/// The right column: the vehicle, then the open system's cards, then the contribution chips. The
/// model's own sections follow, drawn by the app.
pub fn inspector(ui: &mut egui::Ui, p: &Panel, row: Option<&AssetRow>) -> Vec<Act> {
    let mut acts = Vec::new();
    if let Some(Err(e)) = &p.data {
        ui.label(egui::RichText::new(e).color(theme::BAD));
        return acts;
    }
    let (Some(v), Some(data), Some(row)) = (p.vehicle(), p.data(), row) else { return acts };
    let class = v.drive.kind().class();
    ui.horizontal(|ui| {
        ui.label(theme::disp_text(row.category().to_uppercase(), theme::style_eyebrow(), theme::BRASS));
        ui.label(egui::RichText::new(class).monospace().color(theme::DIM));
    });
    ui.add(egui::Label::new(egui::RichText::new(row.label()).heading().color(theme::TX)).truncate());
    ui.label(egui::RichText::new(format!("0x{:08X} · template 0x{:08X}", row.hash, v.key)).monospace().color(theme::DIM));
    ui.add_space(8.0);
    match (&v.drive, data.fleet_of(class)) {
        _ if p.system == System::Coupling => acts.extend(coupling_cards(ui, p, v, data)),
        (Drive::Car(car), _) => car_cards(ui, p, car, &data.fleet),
        (Drive::Tank { fields, wheels }, Some(fleet)) => crate::drivingtank::cards(ui, p, fields, wheels, fleet),
        (Drive::Boat { fields }, Some(fleet)) => crate::drivingboat::cards(ui, p, fields, fleet),
        (Drive::Helicopter { player, ai, rotors, .. }, Some(fleet)) => {
            crate::drivingheli::cards(ui, p, player, ai.as_ref(), rotors, fleet, data.fleet_of(drivingdata::HELI_AI_CLASS))
        }
        (Drive::Plane { spinners }, _) => crate::drivingplane::cards(ui, p, spinners, &data.fleet),
        (_, None) => {
            ui.label(egui::RichText::new(format!("no value ranges were read for {class}")).color(theme::BAD));
        }
    }
    theme::eyebrow(ui, "Start a contribution from here");
    ui.horizontal_wrapped(|ui| {
        for kind in crate::domain::Domain::Driving.kinds() {
            if ui.button(crate::quartermaster::kind_title(kind)).on_hover_text(*kind).clicked() {
                acts.push(Act::Contribute(kind));
            }
        }
    });
    ui.add_space(10.0);
    acts
}

/// The car's cards for the open system.
fn car_cards(ui: &mut egui::Ui, p: &Panel, car: &Car, fleet: &Fleet) {
    let f = &car.fields;
    match p.system {
        System::Chassis => {
            theme::card(ui, "Axles", Some(&format!("{} car templates", fleet.templates)), |ui| {
                let side = |name: String| f.num(&name).ok().map(|v| (v, fleet.place(&name, v)));
                let rows: Vec<(&str, [Option<(f32, Option<f32>)>; 2])> = [
                    ("Wheel radius", "WheelRadius"),
                    ("Susp strength", "WheelSuspStrength"),
                    ("Susp damp", "WheelSuspDamp"),
                    ("Susp exp length", "WheelSuspExpLength"),
                    ("Susp cmp length", "WheelSuspCmpLength"),
                    ("Friction fwd", "WheelFrictionFwd"),
                    ("Friction side", "WheelFrictionSide"),
                    ("Brake", "Brake"),
                    ("Emergency brake", "EmergencyBrake"),
                ]
                .into_iter()
                .map(|(label, tail)| (label, [side(format!("Front{tail}")), side(format!("Rear{tail}"))]))
                .collect();
                paired(ui, ["FRONT", "REAR"], &rows);
            });
            theme::card(ui, "Body", None, |ui| {
                field_row(ui, fleet, f, "Mass", "Mass", "");
                field_row(ui, fleet, f, "Gravity factor", "GravityFactor", "");
                field_row(ui, fleet, f, "Linear dampening", "LinearDampening", "");
                let triple = |a: &str, b: &str, c: &str| Some(format!("{} · {} · {}", number(f.num(a).ok()?), number(f.num(b).ok()?), number(f.num(c).ok()?)));
                if let Some(t) = triple("AngDampeningX", "AngDampeningY", "AngDampeningZ") {
                    ranged(ui, fleet, 96.0, "Ang dampening", t, None, true);
                }
                if let Some(t) = triple("InertiaTensorX", "InertiaTensorY", "InertiaTensorZ") {
                    ranged(ui, fleet, 96.0, "Inertia tensor", t, None, true);
                }
                if let Ok(at) = f.vec3("CenterOfMassOffset") {
                    ranged(ui, fleet, 96.0, "Centre of mass", position(at), None, true);
                }
            });
        }
        System::Powertrain => {
            theme::card(ui, "Engine", None, |ui| {
                field_row(ui, fleet, f, "Engine torque", "EngineTorque", "");
                field_row(ui, fleet, f, "Max speed", "MaxSpeed", "km/h");
                field_row(ui, fleet, f, "Reverse max speed", "ReverseMaxSpeed", "km/h");
                field_row(ui, fleet, f, "Min rpm", "GearMinRPM", "rpm");
                field_row(ui, fleet, f, "Opt rpm", "GearOptRPM", "rpm");
                field_row(ui, fleet, f, "Max rpm", "GearMaxRPM", "rpm");
                field_row(ui, fleet, f, "Torque factor at min", "TorqueFactorAtMinRPM", "");
                field_row(ui, fleet, f, "Rear torque share", "RearToFrontTorqueDistrib", "");
            });
            if let (Ok(g), Ok(a)) = (f.gearbox(), f.axles(&car.wheels)) {
                theme::card(ui, "Gearbox", None, |ui| {
                    let line = |ui: &mut egui::Ui, gear: &str, ratio: &str, at: &str, head: bool| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for (text, w) in [(gear, 46.0), (ratio, 62.0), (at, 110.0)] {
                                ui.allocate_ui_with_layout(egui::vec2(w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(ui.max_rect().size());
                                    if head {
                                        ui.label(theme::disp_text(text, theme::style_eyebrow(), theme::DIM));
                                    } else {
                                        ui.label(text);
                                    }
                                });
                            }
                        });
                    };
                    line(ui, "GEAR", "RATIO", "UPSHIFT AT", true);
                    line(ui, "R", &number(g.reverse), "—", false);
                    for gear in 1..=5 {
                        let kmh = drivingmath::upshift_kmh(&g, &a, gear);
                        let at = if gear < 5 && kmh < g.max_speed { format!("{kmh:.1} km/h") } else { "—".to_string() };
                        line(ui, &gear.to_string(), &number(g.forward[gear - 1]), &at, false);
                    }
                    ui.separator();
                    field_row(ui, fleet, f, "Final drive", "GearRatioPrimary", "");
                    field_row(ui, fleet, f, "Upshift", "GearUpshiftRPM", "rpm");
                    field_row(ui, fleet, f, "Downshift", "GearDownshiftRPM", "rpm");
                    field_row(ui, fleet, f, "Clutch delay", "ClutchDelayTime", "s");
                });
            }
            unread(ui, &drivingdata::UNREAD[..7]);
        }
        System::Steering => {
            let rate_mode = f.num("SteeringInertia").is_ok_and(|k| k < 0.0);
            theme::card(ui, "Steering", None, |ui| {
                let front = car.wheels.iter().filter(|w| !w.rear).count();
                ranged(ui, fleet, 128.0, "Steered axle", format!("Front · {front} wheels"), None, true);
                field_row(ui, fleet, f, "Max angle", "SteerMaxAngle", "rad");
                field_row(ui, fleet, f, "High speed loss", "HighSpeedSteerLoss", "");
                field_row(ui, fleet, f, "Loss when braking ×", "HighSpeedSteerLossBrakeMulti", "");
                field_row(ui, fleet, f, "Loss when coasting ×", "HighSpeedSteerLossNoThrottleMulti", "");
                field_row(ui, fleet, f, "Inertia", "SteeringInertia", "");
                field_row(ui, fleet, f, "No input inertia", "SteerNoInputInertia", "");
                field_row(ui, fleet, f, "Diff sign inertia", "SteerDiffSignInertia", "");
                field_row(ui, fleet, f, "Low speed limit", "LowSpeedLimit", "km/h");
                field_row(ui, fleet, f, "Low speed boost", "LowSpeedSteerBoost", "");
            });
            theme::card(ui, "Used when inertia is below 0", None, |ui| {
                for (label, name) in [("Steer speed", "SteerSpeed"), ("At low speed", "LowSpeedSteerSpeed")] {
                    if let Ok(v) = f.num(name) {
                        ranged(ui, fleet, 128.0, label, format!("{} rad/s", number(v)), Some((name, v)), rate_mode);
                    }
                }
            });
            unread(ui, &drivingdata::UNREAD[7..8]);
        }
        System::Coupling | System::Hull | System::Flight | System::Yaw | System::Attitude | System::Flyby => {}
    }
}

/// The Coupling system's cards, the same for every class: what the vehicle has to couple with, a
/// helicopter's winch, and the trailers a hitch takes.
fn coupling_cards(ui: &mut egui::Ui, p: &Panel, v: &Vehicle, data: &Data) -> Vec<Act> {
    let mut acts = Vec::new();
    let fleet = &data.fleet;
    theme::card(ui, "On this vehicle", None, |ui| {
        match p.hitch() {
            Some(at) => {
                ranged(ui, fleet, 96.0, "Hitch", HITCH.to_string(), None, true);
                ranged(ui, fleet, 96.0, "Hitch at", position(at), None, true);
            }
            None => ranged(ui, fleet, 96.0, "Hitch", "None".to_string(), None, true),
        }
        let winch = v.uses.iter().filter(|u| u.class == "ConnectPoint").count();
        ranged(ui, fleet, 96.0, "Winch points", winch.to_string(), None, true);
        let count = |n: usize| if n == 0 { "None".to_string() } else { n.to_string() };
        ranged(ui, fleet, 96.0, "PhysicalLink", count(v.physical_links), None, true);
        ranged(ui, fleet, 96.0, "ConstraintLink", count(v.constraint_links), None, true);
    });
    if let Drive::Helicopter { winch: Some(winch), .. } = &v.drive {
        theme::card(ui, "Winch", None, |ui| {
            ranged(ui, fleet, 96.0, "Bone", p.node_label(winch.bone), None, true);
            ranged(ui, fleet, 96.0, "Length", number(winch.length), None, true);
            ranged(ui, fleet, 96.0, "Magnet radius", number(winch.magnet_radius), None, true);
            ranged(ui, fleet, 96.0, "Autodrop under", number(winch.autodrop_length), None, true);
        });
    }
    if p.hitch().is_some() {
        theme::card(ui, "Trailers", Some(&format!("{} templates", fleet.trailers.len())), |ui| {
            let mut trailers: Vec<(u32, String)> =
                fleet.trailers.iter().map(|k| (*k, data.template_names.get(k).cloned().unwrap_or_else(|| format!("0x{k:08X}")))).collect();
            trailers.sort_by(|a, b| a.1.cmp(&b.1));
            ui.spacing_mut().item_spacing.y = 0.0;
            for (key, name) in trailers {
                let on = p.trailer == Some(key);
                let resp = theme::row_chip(ui, if on { theme::BRASS_SOFT } else { Color32::TRANSPARENT }, Color32::TRANSPARENT, |ui| {
                    // The key at the right; the name has what is left and is cut short there.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("0x{key:08X}")).monospace().color(theme::DIM));
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(name).color(if on { theme::BRASS } else { theme::TX })).truncate());
                        });
                    });
                });
                if resp.clicked() {
                    acts.push(Act::Trailer(Some(key)));
                }
            }
        });
    }
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_act_opens_that_system() {
        let mut p = Panel::default();
        assert!(apply(Act::System(System::Steering), &mut p).is_none());
        assert_eq!(p.system, System::Steering);
    }

    #[test]
    fn a_group_act_opens_a_group_and_closes_it_again() {
        let mut p = Panel::default();
        apply(Act::Group("Semi"), &mut p);
        assert_eq!(p.open_group, Some("Semi"));
        apply(Act::Group("Semi"), &mut p);
        assert_eq!(p.open_group, None);
    }

    #[test]
    fn choosing_the_chosen_trailer_takes_it_off() {
        let mut p = Panel::default();
        apply(Act::Trailer(Some(7)), &mut p);
        assert_eq!(p.trailer, Some(7));
        apply(Act::Trailer(Some(7)), &mut p);
        assert_eq!(p.trailer, None);
    }

    #[test]
    fn load_and_contribute_are_the_apps_to_do() {
        let mut p = Panel::default();
        assert!(matches!(apply(Act::Load(1, "x".into()), &mut p), Some(Host::Load(1, _))));
        assert!(matches!(apply(Act::Contribute("add_model"), &mut p), Some(Host::Contribute("add_model"))));
    }

    fn state(json: &str) -> crate::ui_state::UiState {
        serde_json::from_str(json).expect("a ui-state")
    }

    #[test]
    fn a_ui_state_opens_a_system_and_sets_the_overlays() {
        let mut p = Panel::default();
        apply_state(&mut p, &state(r#"{"capture":"x.png","system":"steering","overlays":["Show suspension travel"],"trailer":"Flatbed Trailer"}"#)).unwrap();
        assert_eq!(p.system, System::Steering);
        assert_eq!(p.show, [false, true, false, false, false, false, false, false]);
        assert_eq!(p.pending_trailer.as_deref(), Some("Flatbed Trailer"));
        assert!(!p.exhaust);
    }

    #[test]
    fn a_ui_state_turns_the_exhausts_on_and_names_a_template() {
        let mut p = Panel::default();
        apply_state(&mut p, &state(r#"{"capture":"x.png","exhaust":"on","template":"Austin (CIV)","overlays":["Show full lock"]}"#)).unwrap();
        assert!(p.exhaust);
        assert_eq!(p.pending_template.as_deref(), Some("Austin (CIV)"));
        assert_eq!(p.show, [false, false, false, false, true, false, false, false]);
    }

    #[test]
    fn a_ui_state_with_an_unknown_name_is_refused() {
        let mut p = Panel::default();
        assert!(apply_state(&mut p, &state(r#"{"capture":"x.png","system":"Wheels"}"#)).unwrap_err().contains("Wheels"));
        assert!(apply_state(&mut p, &state(r#"{"capture":"x.png","overlays":["Show doors"]}"#)).unwrap_err().contains("Show doors"));
        assert!(apply_state(&mut p, &state(r#"{"capture":"x.png","exhaust":"Idle"}"#)).unwrap_err().contains("Off, On"));
    }

    #[test]
    fn an_exhaust_act_sets_the_picker() {
        let mut p = Panel::default();
        p.exhaust_problem = Some("x".into());
        apply(Act::Exhaust(true), &mut p);
        assert!(p.exhaust && p.exhaust_problem.is_none());
        apply(Act::Exhaust(false), &mut p);
        assert!(!p.exhaust);
    }

    /// A page with no car asks for no pose and plays no exhaust, whatever is switched on.
    #[test]
    fn a_page_with_no_car_turns_no_wheels_and_plays_no_exhaust() {
        let mut p = Panel::default();
        p.system = System::Steering;
        p.show[Overlay::FullLock as usize] = true;
        p.exhaust = true;
        assert!(p.steer_pose().is_empty());
        let index = AssetIndex::build(&[], &[], HashMap::new());
        assert_eq!(p.exhausts(&index), Ok(Vec::new()));
    }

    const IDENTITY: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

    fn at(x: f32, y: f32, z: f32) -> [[f32; 4]; 4] {
        let mut m = IDENTITY;
        m[3] = [x, y, z, 1.0];
        m
    }

    fn node(index: usize, hash: u32, parent: Option<usize>) -> HierNode {
        HierNode { index, hash, parent, local: glam::Mat4::IDENTITY.to_cols_array(), bbox_min: [0.0; 3], bbox_max: [0.0; 3] }
    }

    /// A row vector through a palette entry.
    fn through(m: &[[f32; 4]; 4], v: [f32; 3]) -> [f32; 3] {
        let v = [v[0], v[1], v[2], 1.0];
        std::array::from_fn(|c| (0..4).map(|r| v[r] * m[r][c]).sum())
    }

    /// Turning a wheel's node turns it and the node under it about the node's own place, and leaves
    /// every other node as it was.
    #[test]
    fn full_lock_turns_the_steered_node_and_what_hangs_from_it_only() {
        let hier = [node(0, 10, None), node(1, 11, Some(0)), node(2, 12, Some(1)), node(3, 13, Some(0))];
        let binds = [at(0.0, 0.0, 0.0), at(1.0, 0.5, 4.0), at(1.3, 0.5, 4.0), at(-1.0, 0.5, 4.0)];
        let mut palette = vec![IDENTITY; 4];
        turn_nodes(&mut palette, &hier, &binds, &[(11, std::f32::consts::FRAC_PI_2)]).unwrap();
        assert_eq!(palette[0], IDENTITY);
        assert_eq!(palette[3], IDENTITY);
        assert_eq!(palette[1], palette[2]);
        // the node's own place stays put; a point ahead of it swings to its side, at the same height
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
        assert!(near(through(&palette[1], [1.0, 0.5, 4.0]), [1.0, 0.5, 4.0]));
        assert!(near(through(&palette[1], [1.0, 0.5, 5.0]), [2.0, 0.5, 4.0]), "{:?}", through(&palette[1], [1.0, 0.5, 5.0]));
    }

    #[test]
    fn full_lock_on_a_node_the_model_lacks_is_an_error() {
        let hier = [node(0, 10, None)];
        let mut palette = vec![IDENTITY; 1];
        assert!(turn_nodes(&mut palette, &hier, &[IDENTITY], &[(99, 0.5)]).unwrap_err().contains("00000063"));
        assert!(turn_nodes(&mut palette, &hier, &[], &[(10, 0.5)]).unwrap_err().contains("0-bone rig"));
        assert_eq!(turn_nodes(&mut palette, &hier, &[], &[]), Ok(()));
    }

    #[test]
    fn the_camera_frames_the_tractor_and_the_trailer_together() {
        let (a, b) = ((glam::Vec3::ZERO, 4.0), (glam::Vec3::new(0.0, 0.0, -10.0), 6.0));
        let (centre, radius) = enclosing(a, b);
        assert!((radius - 10.0).abs() < 1e-5 && (centre - glam::Vec3::new(0.0, 0.0, -6.0)).length() < 1e-5, "{centre:?} {radius}");
        // one inside the other is the larger alone
        assert_eq!(enclosing(a, (glam::Vec3::new(0.0, 0.0, 1.0), 1.0)), a);
        assert_eq!(enclosing((glam::Vec3::new(0.0, 0.0, 1.0), 1.0), a), a);
    }

    #[test]
    fn an_overlay_act_turns_it_on_and_off() {
        let mut p = Panel::default();
        assert!(!p.show[Overlay::Travel as usize]);
        apply(Act::Overlay(Overlay::Travel), &mut p);
        assert!(p.show[Overlay::Travel as usize]);
        apply(Act::Overlay(Overlay::Travel), &mut p);
        assert!(!p.show[Overlay::Travel as usize]);
    }

    #[test]
    fn each_system_offers_its_own_overlays() {
        assert_eq!(Overlay::of(Kind::Car, System::Chassis), [Overlay::Wheels, Overlay::Travel]);
        assert_eq!(Overlay::of(Kind::Car, System::Powertrain), [Overlay::TorqueSplit]);
        assert_eq!(Overlay::of(Kind::Car, System::Steering), [Overlay::FullLock]);
        assert_eq!(Overlay::of(Kind::Tank, System::Chassis), [Overlay::Wheels, Overlay::Travel, Overlay::Probes]);
        assert!(Overlay::of(Kind::Tank, System::Steering).is_empty(), "full lock is a car's");
        assert_eq!(Overlay::of(Kind::Helicopter, System::Flight), [Overlay::Rotors]);
        assert_eq!(Overlay::of(Kind::Helicopter, System::Coupling), [Overlay::Hardpoints, Overlay::Winch]);
        for kind in Kind::ALL {
            assert_eq!(Overlay::of(kind, System::Coupling)[0], Overlay::Hardpoints);
            assert_eq!(System::of(kind).len(), 4);
            assert_eq!(System::of(kind)[3], System::Coupling);
        }
        // an overlay is its own index into the page's switches
        assert!(Overlay::ALL.iter().enumerate().all(|(i, o)| *o as usize == i));
    }

    #[test]
    fn the_camera_frames_for_the_part_of_the_window_the_page_leaves() {
        assert!((framing(810.0) - 810.0 / 400.0).abs() < 1e-6);
        // a very short window still frames
        assert!(framing(300.0).is_finite() && framing(300.0) > 1.0);
    }

    #[test]
    fn systems_are_named_folding_case() {
        assert_eq!(System::named("powertrain"), Some(System::Powertrain));
        assert_eq!(System::named("Wheels"), None);
    }

    #[test]
    fn the_speed_axis_ends_past_the_cap() {
        assert_eq!(speed_axis(90.0), 100.0);
        assert_eq!(speed_axis(80.0), 100.0);
        assert_eq!(speed_axis(165.0), 180.0);
        assert_eq!(speed_axis(75.0), 80.0);
    }

    #[test]
    fn a_page_with_no_model_has_no_crumb_past_the_class() {
        let p = Panel::default();
        assert_eq!(crumb(&p, Some("Semi")), ["Semi"]);
        assert_eq!(status(&p), None);
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;

    /// `civ_veh_semi_austin`.
    const AUSTIN_MODEL: u32 = 0x926D_9ECB;

    /// The page as it stands with the retail game open and `model` loaded.
    fn page(model: u32, state: impl FnOnce(&mut Panel)) -> (Panel, AssetIndex) {
        let mut w = WadStack::open(&crate::app::retail_vz_wad(), &[]).expect("open retail vz.wad");
        let names = crate::index::load_all_names().unwrap_or_else(|e| panic!("{e}"));
        let mut index = AssetIndex::build(&w.wads, &w.side, names);
        index.add_kit_templates(&crate::app::load_kit_templates(&mut w).unwrap_or_else(|e| panic!("{e}")));
        let mut p = Panel::default();
        state(&mut p);
        p.ensure(&mut w, &index);
        let hier = mercs2_formats::orchestrator::parse_hier(&w.extract_container(model).unwrap_or_else(|e| panic!("{e}")));
        p.show_model(Some((model, &hier)), &index);
        (p, index)
    }

    #[test]
    fn the_austin_model_opens_on_its_car_template() {
        let (p, _) = page(AUSTIN_MODEL, |_| {});
        assert!(p.templates.iter().any(|t| t.1 == "Austin (CIV)"), "{:?}", p.templates);
        assert_eq!(p.template_name(), p.templates.first().map(|t| t.1.as_str()));
        assert_eq!(p.selected_node, Some(pandemic_hash_m2(HITCH)));
        assert_eq!(p.models_with(pandemic_hash_m2(HITCH)).len(), 6);
        assert!(p.models_with(pandemic_hash_m2(HITCH)).contains(&"civ_veh_semi_austin"));
    }

    /// The vehicle list's groups hold every vehicle model once, in class order; a filter keeps the
    /// rows whose name contains it, across groups.
    #[test]
    fn the_vehicle_list_groups_every_vehicle_model_and_filters_by_name() {
        let (_, index) = page(AUSTIN_MODEL, |_| {});
        let rows = vehicles(&index);
        let groups = grouped(&rows, "");
        assert_eq!(groups.iter().map(|g| g.1.len()).sum::<usize>(), rows.len());
        let names: Vec<&str> = groups.iter().map(|g| g.0).collect();
        let mut once = names.clone();
        once.dedup();
        assert_eq!(names, once, "a class is one group");
        assert_eq!(groups.iter().find(|g| g.0 == "Semi").map(|g| g.1.len()), Some(5));
        let austins = grouped(&rows, "austin");
        assert!(!austins.is_empty());
        assert!(austins.iter().all(|g| g.1.iter().all(|r| r.label().contains("austin"))));
        assert!(austins.iter().any(|g| g.0 == "Semi" && g.1.len() == 2));
        assert!(grouped(&rows, "no such vehicle").is_empty());
    }

    /// Choosing another template of the model re-reads the car; the page reads the level once.
    #[test]
    fn choosing_a_template_reads_that_templates_car() {
        let (mut p, index) = page(AUSTIN_MODEL, |_| {});
        assert!(p.templates.len() > 1, "{:?}", p.templates);
        let first = p.vehicle().expect("a car").key;
        let other = p.templates.iter().map(|t| t.0).find(|k| *k != first).expect("a second template");
        apply(Act::Trailer(Some(0x8000_76A6)), &mut p);
        apply(Act::Template(other), &mut p);
        assert_eq!(p.vehicle().expect("a car").key, other);
        assert_eq!(p.trailer, None, "the trailer comes off with the template");
        // a template of another model is not this model's to choose
        apply(Act::Template(0x8000_76A6), &mut p);
        assert_eq!(p.vehicle().expect("a car").key, other);
        // a second show reads nothing again
        let mut w = WadStack::empty();
        p.ensure(&mut w, &index);
        assert!(p.data().is_some());
    }

    /// The flatbed's origin goes on the Austin's hitch node, 1.20 above the tractor's origin.
    #[test]
    fn a_trailer_rides_at_the_hitch_nodes_height() {
        let (mut p, index) = page(AUSTIN_MODEL, |p| p.pending_trailer = Some("Flatbed Trailer".into()));
        let (key, model, at) = p.trailer_at(&index).expect("a trailer on the hitch");
        assert_eq!(key, 0x8000_76A6);
        assert!(index.models().iter().any(|r| r.hash == model));
        assert!((at[1] - 1.20).abs() < 0.006 && (at[2] + 3.10).abs() < 0.006, "{at:?}");
        apply(Act::Trailer(None), &mut p);
        assert_eq!(p.trailer_at(&index), None);
        let (p, _) = page(AUSTIN_MODEL, |p| p.pending_trailer = Some("Caravan".into()));
        assert!(p.trailer_problem.as_deref().is_some_and(|e| e.contains("Caravan")));
    }

    #[test]
    fn a_named_template_is_the_one_read_and_an_unknown_name_is_refused() {
        let (p, _) = page(AUSTIN_MODEL, |p| p.pending_template = Some("austin (civ)".into()));
        assert_eq!(p.template_name(), Some("Austin (CIV)"));
        let (p, _) = page(AUSTIN_MODEL, |p| p.pending_template = Some("Flatbed Trailer".into()));
        let Some(Err(e)) = &p.vehicle else { panic!("a template the model does not have was read") };
        assert!(e.contains("Flatbed Trailer") && e.contains("Austin (CIV)"), "{e}");
    }

    /// Full lock turns the Austin's two front wheels by its `SteerMaxAngle`, on Steering only.
    #[test]
    fn full_lock_turns_the_austins_two_front_wheels() {
        let (mut p, _) = page(AUSTIN_MODEL, |p| p.pending_template = Some("Austin (CIV)".into()));
        p.show[Overlay::FullLock as usize] = true;
        assert!(p.steer_pose().is_empty(), "the pose is Steering's");
        p.system = System::Steering;
        let pose = p.steer_pose();
        assert_eq!(pose.len(), 2);
        assert!(pose.iter().all(|(node, angle)| p.nodes.contains_key(node) && *angle == 0.8), "{pose:?}");
        assert_ne!(pose[0].0, pose[1].0);
    }

    /// Exhaust On plays the Austin's two exhaust parts' effect at the tops of its stacks.
    #[test]
    fn the_austin_plays_two_exhausts() {
        let (mut p, index) = page(AUSTIN_MODEL, |p| p.pending_template = Some("Austin (CIV)".into()));
        assert_eq!(p.exhausts(&index), Ok(Vec::new()));
        apply(Act::Exhaust(true), &mut p);
        let list = p.exhausts(&index).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].effect, list[1].effect);
        assert!(index.rows(crate::index::Kind::Effect).iter().any(|r| r.hash == list[0].effect));
        assert!(list.iter().all(|x| (x.at[1] - 4.30).abs() < 0.006 && x.starter.duration == 0.0), "{list:?}");
        assert!((list[0].at[0] + list[1].at[0]).abs() < 1e-3, "one each side");
    }

    /// How wide `f` draws in a column `w` wide.
    fn drawn_width(w: f32, mut f: impl FnMut(&mut egui::Ui)) -> f32 {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 4000.0))), ..Default::default() };
        let mut out = 0.0;
        crate::gui::run_headless(&ctx, input, |ui| {
            out = ui.allocate_ui_with_layout(egui::vec2(w, 4000.0), egui::Layout::top_down(egui::Align::Min), |ui| f(ui)).response.rect.width();
        });
        out
    }

    /// Every system's cards fit the inspector at its narrowest (a 300 px panel leaves 275) and at
    /// its default width (372 leaves 347).
    #[test]
    fn every_systems_cards_fit_the_inspector() {
        let mut wide = Vec::new();
        for (model, template, kind) in [
            (AUSTIN_MODEL, "Austin (CIV)", Kind::Car),
            (AMX30_MODEL, "AMX30", Kind::Tank),
            (RIB36_MODEL, "Gar", Kind::Boat),
            (MI35_MODEL, "Mi35", Kind::Helicopter),
            (A10_MODEL, "Support Vehicle (A10)", Kind::Plane),
            (C130_MODEL, "Support Vehicle (C130)", Kind::Plane),
        ] {
            let (mut p, index) = page(model, |p| p.pending_template = Some(template.into()));
            assert_eq!(p.kind(), Some(kind), "{template}: {:?}", p.vehicle.as_ref().map(|v| v.as_ref().err()));
            let row = index.models().iter().find(|r| r.hash == model).expect("the model's row");
            for system in System::of(kind) {
                p.system = *system;
                assert!(status(&p).is_some(), "{template} has no {} status line", system.label());
                for w in [275.0, 347.0] {
                    let drawn = drawn_width(w, |ui| {
                        inspector(ui, &p, Some(row));
                    });
                    if drawn > w + 0.5 {
                        wide.push(format!("{template} {} at {w}: {drawn}", system.label()));
                    }
                }
            }
        }
        assert!(wide.is_empty(), "{wide:?}");
    }

    /// `vz_veh_tank_amx30_base`, `global_veh_rib36`, `vz_veh_helicopter_mi35hind`,
    /// `ch_veh_apc_zbd2000`, `al_veh_plane_a10`.
    const AMX30_MODEL: u32 = 0x1164_0B62;
    const RIB36_MODEL: u32 = 0x0C95_FF35;
    const MI35_MODEL: u32 = 0x1CE8_F54F;
    const ZBD2000_MODEL: u32 = 0x340B_1CB8;
    const A10_MODEL: u32 = 0x722B_EA76;
    /// `al_veh_plane_c130`.
    const C130_MODEL: u32 = 0x54E9_C345;

    /// A model opens on the systems of its template's physics class: a tank on Chassis, a boat on
    /// Hull, a helicopter on Flight; a plane, which has no physics record, on none.
    #[test]
    fn a_model_opens_on_its_own_classes_systems() {
        for (model, kind, first) in [
            (AMX30_MODEL, Kind::Tank, System::Chassis),
            (RIB36_MODEL, Kind::Boat, System::Hull),
            (MI35_MODEL, Kind::Helicopter, System::Flight),
        ] {
            let (p, _) = page(model, |_| {});
            assert_eq!(p.kind(), Some(kind));
            assert_eq!(p.system, first);
            assert!(p.has_systems());
        }
        // the Steering tab a car was left on is a tank's too, and is kept
        let (p, _) = page(AMX30_MODEL, |p| p.system = System::Steering);
        assert_eq!(p.system, System::Steering);
        // a plane has no physics record: its two fly-by templates open on Flyby, and the parked
        // `A10`, a prop, is not one of them
        let (mut p, index) = page(A10_MODEL, |_| {});
        assert_eq!((p.kind(), p.system), (Some(Kind::Plane), System::Flyby));
        let names: Vec<&str> = p.templates.iter().map(|t| t.1.as_str()).collect();
        assert_eq!(names, ["Support Vehicle (A10)", "Support Vehicle (A10) low altitude"]);
        // a model no template of the five kinds names has no systems
        p.show_model(Some((0x1234_5678, &[])), &index);
        assert!(!p.has_systems() && p.kind().is_none() && p.templates.is_empty());
    }

    /// The ZBD2000 has four tank templates and one boat template; choosing the boat one turns the
    /// page to a boat's systems.
    #[test]
    fn a_models_templates_can_be_of_two_classes() {
        let (mut p, _) = page(ZBD2000_MODEL, |p| p.pending_template = Some("ZBD2000".into()));
        assert_eq!(p.kind(), Some(Kind::Tank));
        assert_eq!(p.templates.len(), 5, "{:?}", p.templates);
        let boat = p.templates.iter().find(|t| t.1.contains("Amphibious")).expect("the amphibious template").0;
        apply(Act::Template(boat), &mut p);
        assert_eq!(p.kind(), Some(Kind::Boat));
        assert_eq!(p.system, System::Hull);
    }

    /// A `--ui-state` that names a system the vehicle's class lacks, or rotors on a vehicle with
    /// none, is refused where the bench reads.
    #[test]
    fn a_named_system_or_rotor_state_the_vehicle_lacks_is_refused() {
        let named = |system: System| move |p: &mut Panel| p.pending_system = Some(system);
        let (p, _) = page(MI35_MODEL, named(System::Yaw));
        assert_eq!(p.system, System::Yaw);
        let (p, _) = page(AMX30_MODEL, named(System::Flight));
        let Some(Err(e)) = &p.vehicle else { panic!("a tank opened on Flight") };
        assert!(e.contains("Flight") && e.contains("Chassis"), "{e}");
        let (p, _) = page(AMX30_MODEL, |p| p.pending_rotors = Some(true));
        let Some(Err(e)) = &p.vehicle else { panic!("a tank span its rotors") };
        assert!(e.contains("rotors"), "{e}");
        let (p, _) = page(MI35_MODEL, |p| p.pending_rotors = Some(true));
        assert!(p.rotors && p.vehicle().is_some());
    }

    /// The AMX30's six ground probes are nodes of its model, and its template names the winch a
    /// Mi-35 lifts with.
    #[test]
    fn a_tank_has_its_probe_nodes_and_a_helicopter_its_rotors_and_winch() {
        let (p, _) = page(AMX30_MODEL, |_| {});
        for name in crate::drivingtank::PROBE_NODES {
            assert!(p.node(pandemic_hash_m2(name)).is_some(), "{name}");
        }
        let Some(Drive::Tank { wheels, .. }) = p.vehicle().map(|v| &v.drive) else { panic!("not a tank") };
        assert_eq!(wheels.len(), 14);
        assert!(wheels.iter().all(|w| p.node(w.wheel).is_some()));
        let (p, _) = page(MI35_MODEL, |_| {});
        let Some(Drive::Helicopter { rotors, winch, ai, .. }) = p.vehicle().map(|v| &v.drive) else { panic!("not a helicopter") };
        assert_eq!(rotors.len(), 2);
        assert!(rotors.iter().all(|r| p.node(r.hub).is_some()));
        assert!(ai.is_some());
        assert!(winch.is_some_and(|w| p.node(w.bone).is_some()));
    }
}
