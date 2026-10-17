//! The Missions workbench — a Blueprint-style node editor for a mission script.
//!
//! The rail's **Missions** slot opened a filtered Lua list before this: pick a file, read it in the
//! Lua viewer, maybe wedge a `patch_lua`. Reading retail mission flow as a wall of plain text has
//! not been enough: the governing names (lifecycle hooks, Event kinds, child objective modules,
//! Mrx*.Method calls) all follow one pattern, and the pattern wants to be seen, not scrolled
//! through. So the slot now surfaces a Blueprint-style graph: lifecycle hooks are entry nodes,
//! `self:CreateChild{sModuleName=...}` becomes a typed objective node, each `Event.*` handler is a
//! node fed by the matching Event kind, and `Mrx*.Method(...)` calls are the exec-chain between
//! them. Wires are typed by Pandemic's own value kinds (string / int / object / array), which the
//! mockup's palette and this editor's pin colours mirror.
//!
//! # The featured script
//!
//! `PmcCon016` ("Highway to Hell") — a `MrxTaskContract` with a race objective, a destruction
//! sub-objective that extends the clock, a soundtrack cue, and three Fiona VO beats. Decoded by
//! hand against `docs/mercs2-luacd/src/vz/pmccon016.lua` so every parameter, node and wire is a
//! real line of retail Lua, not an illustration. The next drop will parse the script with
//! `mercs2_formats::save_lua`'s Lua parser (or its sibling for `.lua` source) rather than keeping
//! the model inline — but the parser surface is big enough that this first drop names it instead
//! of inventing it, and leaves it to a follow-up to generalise.
//!
//! # Shape
//!
//! `Panel` + `Act` reducer, the same shape as [`crate::quartermaster`]. The four free
//! `navigator` / `center` / `inspector` / `verbs` fns match the dispatch sites in `app.rs`, and
//! [`apply`] folds every queued action back into the panel. The graph itself is static for now —
//! laid out in canvas coordinates to match the mockup — and the panel carries only interaction
//! state (which node is selected, pan, zoom, mission picked).
//!
//! # Palette truth
//!
//! The left panel's "Palette" section surfaces the real engine vocabulary — the 5 `MrxTask`
//! lifecycle hooks, the 44 `Event.*` enum values grouped by prefix, 10 objective base classes, and
//! 9 call-palette namespaces with their retail call-site counts. The counts come from greps over
//! `crates/mercs2_script/corpus/mercs2-luacd/src/vz/` and the resident `mrxtask.lua`, not from
//! imagination — the file groups the same way the mockup does.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::gui::theme;

// ──────────────────────────────────────────────────────────────────────── taxonomy

/// Which category a node belongs to — governs the gradient header colour, mirroring the mockup.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A `MrxTask` lifecycle hook (`Activated`, `LoadAssets`, …) — purple.
    Lifecycle,
    /// A base-class call-up (`MrxTaskContract.Activated(self)`) — purple, "base" eyebrow.
    Base,
    /// A VO cue — pink.
    Vo,
    /// A child objective (`sModuleName = "MrxTask*"`) — amber.
    Objective,
    /// An `Event.*` handler (`_CreateEvent(Event.X, …)`) — red.
    Event,
    /// A music call — teal.
    Music,
    /// A plain Lua helper function defined in this file — slate blue.
    Fn,
}

impl NodeKind {
    /// (header gradient top, header gradient bottom, header text, eyebrow label).
    fn header_colours(self) -> (Color32, Color32, Color32, &'static str) {
        match self {
            NodeKind::Lifecycle => (
                Color32::from_rgb(0x4a, 0x3a, 0x7a),
                Color32::from_rgb(0x2f, 0x26, 0x50),
                Color32::from_rgb(0xde, 0xd2, 0xff),
                "LIFECYCLE",
            ),
            NodeKind::Base => (
                Color32::from_rgb(0x4a, 0x3a, 0x7a),
                Color32::from_rgb(0x2f, 0x26, 0x50),
                Color32::from_rgb(0xde, 0xd2, 0xff),
                "BASE",
            ),
            NodeKind::Vo => (
                Color32::from_rgb(0x7a, 0x2f, 0x57),
                Color32::from_rgb(0x4f, 0x1f, 0x39),
                Color32::from_rgb(0xff, 0xd9, 0xee),
                "VO",
            ),
            NodeKind::Objective => (
                Color32::from_rgb(0x6a, 0x4f, 0x14),
                Color32::from_rgb(0x49, 0x36, 0x11),
                Color32::from_rgb(0xf8, 0xe8, 0xbd),
                "OBJECTIVE",
            ),
            NodeKind::Event => (
                Color32::from_rgb(0x7a, 0x2f, 0x36),
                Color32::from_rgb(0x4f, 0x1f, 0x26),
                Color32::from_rgb(0xff, 0xda, 0xda),
                "EVENT",
            ),
            NodeKind::Music => (
                Color32::from_rgb(0x1e, 0x55, 0x55),
                Color32::from_rgb(0x11, 0x36, 0x36),
                Color32::from_rgb(0xa9, 0xed, 0xe1),
                "MUSIC",
            ),
            NodeKind::Fn => (
                Color32::from_rgb(0x1b, 0x4b, 0x6a),
                Color32::from_rgb(0x11, 0x31, 0x48),
                Color32::from_rgb(0xc9, 0xe4, 0xf7),
                "FN",
            ),
        }
    }
}

/// A typed pin on a node — colour-coded by the value kind, following the mockup's legend.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PinTy {
    /// Diamond. White. Control flow — `self:Method()` / `_CreateEvent` wake-up / callback entry.
    Exec,
    /// Round. Pink. Lua string literal, including stringdb keys and object-name literals.
    Str,
    /// Round. Green. Numeric parameter (`nTimeLimit`, `nStartTime`, …).
    Int,
    /// Round. Purple. Spawned/looked-up world object (`tVehicle`, `Pg.GetGuidByName(...)`).
    Obj,
    /// Round. Orange. Array literal (course locations, destruction target tables).
    Arr,
}

impl PinTy {
    pub fn colour(self) -> Color32 {
        match self {
            PinTy::Exec => Color32::from_rgb(0xf4, 0xf6, 0xfa),
            PinTy::Str => Color32::from_rgb(0xec, 0x48, 0x99),
            PinTy::Int => Color32::from_rgb(0x10, 0xb9, 0x81),
            PinTy::Obj => Color32::from_rgb(0x8b, 0x5c, 0xf6),
            PinTy::Arr => Color32::from_rgb(0xf9, 0x73, 0x16),
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            PinTy::Exec => "exec",
            PinTy::Str => "str",
            PinTy::Int => "int",
            PinTy::Obj => "obj",
            PinTy::Arr => "arr",
        }
    }
}

/// One pin on a node — rendered as a diamond for `Exec`, a circle for every data pin.
#[derive(Clone)]
pub struct Pin {
    pub id: &'static str,
    pub label: &'static str,
    pub ty: PinTy,
    /// Secondary annotation printed beside the pin, mono (literal values, defaults).
    pub annot: Option<&'static str>,
}

/// A graph node — the mockup's rounded card with gradient header and left/right pin rails.
pub struct Node {
    pub id: &'static str,
    pub title: &'static str,
    pub kind: NodeKind,
    pub pos: Pos2,
    pub width: f32,
    pub inputs: Vec<Pin>,
    pub outputs: Vec<Pin>,
    /// A one-line description for the inspector's heading card.
    pub blurb: &'static str,
    /// The Lua line (approximately) this node renders from, for the "Jump to source" verb.
    pub source_line: u32,
}

/// A typed wire between two pins. `(node_id, pin_id)` pairs on each end.
pub struct Wire {
    pub from: (&'static str, &'static str),
    pub to: (&'static str, &'static str),
    pub ty: PinTy,
}

/// The in-script outline the left panel shows — not the palette, but what THIS script defines.
pub struct Outline {
    pub lifecycle_defined: Vec<(&'static str, &'static str)>, // (name, override|new)
    pub children: Vec<(&'static str, &'static str, &'static str)>, // (name, base, note)
    pub vars: Vec<(&'static str, &'static str, PinTy)>,       // (name, ty-short, pin colour)
}

/// The whole decoded mission — nodes, wires, outline. For now one hand-decoded script
/// (`PmcCon016`); the follow-up is to parse `.lua` into this struct generically.
pub struct Mission {
    pub name: &'static str,
    pub inherits: &'static str,
    pub nodes: Vec<Node>,
    pub wires: Vec<Wire>,
    pub outline: Outline,
}

// ──────────────────────────────────────────────────────────────────────── pmccon016

/// The in-game "Highway to Hell" contract, decoded by hand from
/// `docs/mercs2-luacd/src/vz/pmccon016.lua`. Every pin, label and literal below is a line of real
/// retail Lua — no fabrications.
pub fn pmccon016() -> Mission {
    use NodeKind::*;
    use PinTy::*;

    // Positions mirror the mockup's canvas (centre pane, 760×700 viewBox). Rounded to the grid.
    let nodes = vec![
        Node {
            id: "activated",
            title: "On Activated",
            kind: Lifecycle,
            pos: Pos2::new(16.0, 60.0),
            width: 170.0,
            inputs: vec![Pin { id: "self", label: "self", ty: Obj, annot: None }],
            outputs: vec![Pin { id: "body", label: "Body", ty: Exec, annot: None }],
            blurb: "Mission entry. Runs once when the contract activates.",
            source_line: 14,
        },
        Node {
            id: "super_activated",
            title: "Super.Activated",
            kind: Base,
            pos: Pos2::new(236.0, 82.0),
            width: 164.0,
            inputs: vec![Pin { id: "exec", label: "Exec", ty: Exec, annot: None }],
            outputs: vec![Pin { id: "out", label: "Exec", ty: Exec, annot: None }],
            blurb: "MrxTaskContract.Activated(self) — the base class's own activation.",
            source_line: 15,
        },
        Node {
            id: "get_vehicles",
            title: "GetVehicles",
            kind: Fn,
            pos: Pos2::new(460.0, 86.0),
            width: 194.0,
            inputs: vec![Pin { id: "exec", label: "Exec", ty: Exec, annot: None }],
            outputs: vec![
                Pin { id: "out", label: "Exec", ty: Exec, annot: None },
                Pin { id: "tVehicle", label: "tVehicle", ty: Obj, annot: Some("spawn") },
            ],
            blurb: "Picks Panhard (1P) or Buggy (2P) and spawns it at loc_PmcCon016_Racer.",
            source_line: 50,
        },
        Node {
            id: "vo_start",
            title: "VO · Sequence.Start",
            kind: Vo,
            pos: Pos2::new(80.0, 200.0),
            width: 230.0,
            inputs: vec![
                Pin { id: "exec", label: "Exec", ty: Exec, annot: None },
                Pin { id: "cue", label: "cue", ty: Str, annot: Some("\"…Pmc16-01\"") },
            ],
            outputs: vec![Pin { id: "out", label: "Exec", ty: Exec, annot: None }],
            blurb: "First VO beat — Fiona briefs the race on activation.",
            source_line: 17,
        },
        Node {
            id: "race",
            title: "MrxTaskRace",
            kind: Objective,
            pos: Pos2::new(360.0, 192.0),
            width: 300.0,
            inputs: vec![
                Pin { id: "exec", label: "Exec", ty: Exec, annot: None },
                Pin { id: "sName", label: "sName", ty: Str, annot: Some("\"PmcCon016Race\"") },
                Pin { id: "nStartTime", label: "nStartTime", ty: Int, annot: Some("75") },
                Pin { id: "vTgtInclude", label: "vTgtInclude", ty: Obj, annot: None },
                Pin { id: "tCourseLocs", label: "tCourseLocs", ty: Arr, annot: Some("[27]") },
            ],
            outputs: vec![
                Pin { id: "fOnComplete", label: "fOnComplete", ty: Exec, annot: None },
                Pin { id: "fOnCancel", label: "fOnCancel", ty: Exec, annot: None },
            ],
            blurb: "Driving-timed objective. Spawns on race start, winds down from nStartTime, extendable by nTimeToAdd on each waypoint hit.",
            source_line: 60,
        },
        Node {
            id: "music",
            title: "Music · PlaySpecial",
            kind: Music,
            pos: Pos2::new(380.0, 380.0),
            width: 240.0,
            inputs: vec![
                Pin { id: "exec", label: "Exec", ty: Exec, annot: None },
                Pin { id: "sTrack", label: "sTrack", ty: Str, annot: Some("\"mu_PmcCon016_01\"") },
            ],
            outputs: vec![],
            blurb: "MrxMusic.PlaySpecialMusic — mission-specific soundtrack cue.",
            source_line: 34,
        },
        Node {
            id: "event_death",
            title: "Event · ObjectDeath",
            kind: Event,
            pos: Pos2::new(20.0, 452.0),
            width: 200.0,
            inputs: vec![Pin { id: "tVehicle", label: "tVehicle", ty: Obj, annot: None }],
            outputs: vec![Pin { id: "fire", label: "On Fire", ty: Exec, annot: None }],
            blurb: "_CreateEvent(Event.ObjectDeath, {tVehicle}, self.VehicleDeath, {self}).",
            source_line: 33,
        },
        Node {
            id: "vehicle_death",
            title: "VehicleDeath",
            kind: Fn,
            pos: Pos2::new(268.0, 492.0),
            width: 174.0,
            inputs: vec![Pin { id: "exec", label: "Exec", ty: Exec, annot: None }],
            outputs: vec![Pin { id: "self", label: "self", ty: Obj, annot: None }],
            blurb: "Fiona VO beat 04 → self:Cancel with the fail message.",
            source_line: 162,
        },
    ];

    let wires = vec![
        Wire { from: ("activated", "body"), to: ("super_activated", "exec"), ty: Exec },
        Wire { from: ("super_activated", "out"), to: ("get_vehicles", "exec"), ty: Exec },
        Wire { from: ("get_vehicles", "out"), to: ("vo_start", "exec"), ty: Exec },
        Wire { from: ("vo_start", "out"), to: ("race", "exec"), ty: Exec },
        Wire { from: ("race", "fOnComplete"), to: ("music", "exec"), ty: Exec },
        Wire { from: ("get_vehicles", "tVehicle"), to: ("race", "vTgtInclude"), ty: Obj },
        Wire { from: ("event_death", "fire"), to: ("vehicle_death", "exec"), ty: Exec },
    ];

    let outline = Outline {
        lifecycle_defined: vec![("Activated", "override"), ("LoadAssets", "override")],
        children: vec![
            ("Race", "MrxTaskRace", "27"),
            ("DestructionTargets", "ObjectiveDestroy", ""),
        ],
        vars: vec![
            ("tVehicle", "object", PinTy::Obj),
            ("nTimeLimit", "int", PinTy::Int),
            ("nTimeToAdd", "int", PinTy::Int),
            ("nCompletions", "int", PinTy::Int),
        ],
    };

    Mission {
        name: "PmcCon016",
        inherits: "MrxTaskContract",
        nodes,
        wires,
        outline,
    }
}

// ──────────────────────────────────────────────────────────────────────── palette (static)

/// The 5 `MrxTask` lifecycle hooks (`docs/mercs2-luacd/src/resident/mrxtask.lua`).
pub const LIFECYCLE_HOOKS: &[(&str, &str)] = &[
    ("Activated", "(self)"),
    ("Complete", "(self)"),
    ("Cancel", "(self)"),
    ("LoadAssets", "(self, tSaveData)"),
    ("AssetsLoaded", "(self)"),
];

/// 44 `Event.*` enum values, grouped by prefix.
pub const EVENT_GROUPS: &[(&str, usize)] = &[
    ("Object", 12),
    ("Human", 3),
    ("Button", 3),
    ("Weapon", 6),
    ("Timer", 2),
    ("Gui", 2),
    ("Script", 6),
    ("Misc", 10),
];

/// 10 objective base classes with retail usage counts / tags.
pub const OBJECTIVE_BASES: &[(&str, &str)] = &[
    ("MrxTaskContract", "38"),
    ("MrxTaskContractOutpost", "8"),
    ("MrxTaskObjectiveDestroy", "destr."),
    ("MrxTaskObjectiveDeliver", "deliv."),
    ("MrxTaskRace", "race"),
    ("MrxTaskJobDestroySet", "5"),
    ("MrxTaskJobDestroyType", "6"),
    ("MrxTaskJobVerifySet", "5"),
    ("MrxShootingGallery", "14"),
    ("MrxArtilleryAttack", "6"),
];

/// 9 call-palette namespaces with retail call-site counts (from grepping `Mrx*.Method`).
pub const CALL_PALETTE: &[(&str, Color32, usize)] = &[
    ("VO · MrxVoSequence", Color32::from_rgb(0xec, 0x48, 0x99), 380),
    ("Music · MrxMusic / SoundCat", Color32::from_rgb(0x74, 0xdd, 0xc8), 87),
    ("World · MrxLayerManager", Color32::from_rgb(0x8b, 0x5c, 0xf6), 160),
    ("State · MrxState", Color32::from_rgb(0xf4, 0xf6, 0xfa), 90),
    ("Tutorial · MrxTutorialMgr", Color32::from_rgb(0xf0, 0xa9, 0x2b), 65),
    ("Util · MrxUtil", Color32::from_rgb(0x9e, 0xc8, 0xea), 125),
    ("Faction · MrxFactionManager", Color32::from_rgb(0xef, 0x50, 0x64), 50),
    ("Economy · MrxPmc / Support", Color32::from_rgb(0x10, 0xb9, 0x81), 50),
    ("Achievements · MrxAchiev.", Color32::from_rgb(0xf6, 0xd9, 0x8a), 35),
];

// ──────────────────────────────────────────────────────────────────────── panel

pub struct Panel {
    pub mission: Mission,
    pub selected: Option<&'static str>,
    pub pan: Vec2,
    pub zoom: f32,
    pub filter: String,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            mission: pmccon016(),
            // The mockup opens with Race selected — it is the active objective, so the inspector
            // has something to show when the page opens blank.
            selected: Some("race"),
            pan: Vec2::ZERO,
            zoom: 1.0,
            filter: String::new(),
        }
    }
}

impl Panel {
    fn selected_node(&self) -> Option<&Node> {
        let id = self.selected?;
        self.mission.nodes.iter().find(|n| n.id == id)
    }
}

// ──────────────────────────────────────────────────────────────────────── Act / apply

pub enum Act {
    Select(&'static str),
    Compile,
    AddNode,
    ZoomDelta(f32),
    ResetView,
    JumpToSource,
}

pub fn apply(act: Act, p: &mut Panel) {
    match act {
        Act::Select(id) => p.selected = Some(id),
        Act::Compile => { /* reserved — real compile wires through qm later */ }
        Act::AddNode => { /* reserved — add-node palette comes in the follow-up */ }
        Act::ZoomDelta(d) => p.zoom = (p.zoom + d).clamp(0.4, 2.5),
        Act::ResetView => {
            p.zoom = 1.0;
            p.pan = Vec2::ZERO;
        }
        Act::JumpToSource => { /* reserved — hands off to LuaOpen in the follow-up */ }
    }
}

// ──────────────────────────────────────────────────────────────────────── navigator

pub fn navigator(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let acts: Vec<Act> = Vec::new();
    ui.label(theme::disp_text("MISSIONS", 15.0, theme::TX));
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Contracts, jobs, objectives — the Lua that drives play.")
            .size(11.5)
            .color(theme::FAINT),
    );
    ui.add_space(9.0);

    // ── Script picker. One row for now — the follow-up turns this into the mission list,
    //    ranked by frequency / lens like the Audio bank list.
    theme::eyebrow(ui, "Script");
    ui.label(
        egui::RichText::new("69 contracts in retail")
            .size(10.0)
            .color(theme::FAINT),
    );
    ui.add_space(2.0);
    theme::row_chip(ui, theme::BRASS_SOFT, theme::BRASS_DK, |ui| {
        ui.label(egui::RichText::new(p.mission.name).size(12.5).color(theme::BRASS));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(p.mission.inherits)
                    .monospace()
                    .size(9.5)
                    .color(theme::FAINT),
            );
        });
    });

    // ── In this script. The outline the decoded Mission carries.
    ui.add_space(10.0);
    theme::eyebrow(ui, "In this script");
    for (name, kind) in &p.mission.outline.lifecycle_defined {
        theme::row_chip(ui, egui::Color32::TRANSPARENT, theme::LINE, |ui| {
            swatch(ui, Color32::from_rgb(0x8b, 0x5c, 0xf6), true);
            ui.label(egui::RichText::new(*name).size(12.0).color(theme::TX));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(*kind).monospace().size(9.5).color(theme::FAINT));
            });
        });
    }
    for (name, base, note) in &p.mission.outline.children {
        theme::row_chip(ui, egui::Color32::TRANSPARENT, theme::LINE, |ui| {
            swatch(ui, Color32::from_rgb(0xf0, 0xa9, 0x2b), true);
            ui.label(egui::RichText::new(*name).size(12.0).color(theme::DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tail = if note.is_empty() {
                    base.to_string()
                } else {
                    format!("{base} · {note}")
                };
                ui.label(egui::RichText::new(tail).monospace().size(9.5).color(theme::FAINT));
            });
        });
    }
    for (name, ty, pin) in &p.mission.outline.vars {
        theme::row_chip(ui, egui::Color32::TRANSPARENT, theme::LINE, |ui| {
            swatch(ui, pin.colour(), false);
            ui.label(egui::RichText::new(*name).size(12.0).color(theme::DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(*ty).monospace().size(9.5).color(theme::FAINT));
            });
        });
    }

    // ── Palette from the engine.
    ui.add_space(12.0);
    theme::eyebrow(ui, "Palette");
    ui.label(
        egui::RichText::new("from engine")
            .size(10.0)
            .color(theme::FAINT),
    );

    palette_header(ui, "LIFECYCLE HOOKS", "MrxTask · 5");
    for (name, sig) in LIFECYCLE_HOOKS {
        palette_row(ui, Color32::from_rgb(0x8b, 0x5c, 0xf6), true, name, sig);
    }

    palette_header(ui, "EVENT KINDS", "Event.* · 44");
    for (prefix, n) in EVENT_GROUPS {
        palette_row(
            ui,
            Color32::from_rgb(0xef, 0x50, 0x64),
            false,
            &format!("Event.{prefix}*"),
            &n.to_string(),
        );
    }

    palette_header(ui, "OBJECTIVE BASES", "MrxTask* · 10");
    for (name, uses) in OBJECTIVE_BASES {
        palette_row(ui, Color32::from_rgb(0xf0, 0xa9, 0x2b), true, name, uses);
    }

    palette_header(ui, "CALL PALETTE", "Mrx*.Method · 1.2k sites");
    for (label, color, n) in CALL_PALETTE {
        palette_row(ui, *color, false, label, &n.to_string());
    }

    acts
}

fn swatch(ui: &mut egui::Ui, color: Color32, square: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), Sense::hover());
    if square {
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(2.0), color);
    } else {
        ui.painter().circle_filled(rect.center(), 4.5, color);
    }
}

fn palette_header(ui: &mut egui::Ui, label: &str, tail: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .monospace()
                .size(9.5)
                .color(theme::FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(tail).monospace().size(9.5).color(theme::FAINT));
        });
    });
}

fn palette_row(ui: &mut egui::Ui, color: Color32, square: bool, label: &str, tail: &str) {
    theme::row_chip(ui, egui::Color32::TRANSPARENT, theme::LINE, |ui| {
        swatch(ui, color, square);
        ui.label(egui::RichText::new(label).size(12.0).color(theme::DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(tail).monospace().size(9.5).color(theme::FAINT));
        });
    });
}

// ──────────────────────────────────────────────────────────────────────── canvas

/// The dark navy canvas colour (`#0c1015` from the mockup).
const CANVAS_BG: Color32 = Color32::from_rgb(0x0c, 0x10, 0x15);
/// Dot colour on the canvas grid.
const DOT: Color32 = Color32::from_rgba_premultiplied(78, 90, 108, 40);
/// Node body gradient-ish fill (approximation of the two-stop gradient).
const NODE_BODY: Color32 = Color32::from_rgb(0x1c, 0x23, 0x2e);
const NODE_BORDER: Color32 = Color32::from_rgb(0x28, 0x30, 0x3d);
const SEL_BORDER: Color32 = Color32::from_rgb(0x1e, 0x9b, 0xff);

/// Draw the canvas centre pane. Called from app.rs after the side panels have taken their width.
pub fn center(ctx: &egui::Context, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(CANVAS_BG))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            paint_grid(ui, rect);

            // Node positions are in the mission's own coordinate space; the pan + zoom map them
            // onto the viewport. For this first drop zoom and pan are inert — the pins and wires
            // land at the mockup's exact positions.
            let scale = p.zoom;
            let origin = rect.min + p.pan;
            let node_rects = compute_node_rects(&p.mission, origin, scale);

            // Wires FIRST so they lay under the node cards.
            paint_wires(ui, &p.mission, &node_rects);

            // Nodes.
            for n in &p.mission.nodes {
                let selected = p.selected == Some(n.id);
                let r = node_rects[&n.id];
                if paint_node(ui, n, r, selected) {
                    acts.push(Act::Select(n.id));
                }
            }

            // Legend overlay (bottom-left).
            paint_legend(ui, rect);
            // Zoom pill (bottom-right).
            acts.extend(paint_zoom_pill(ui, rect, p));
        });
    acts
}

fn paint_grid(ui: &mut egui::Ui, rect: Rect) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, CANVAS_BG);
    let step = 26.0_f32;
    let mut y = rect.min.y;
    while y < rect.max.y {
        let mut x = rect.min.x;
        while x < rect.max.x {
            p.circle_filled(Pos2::new(x, y), 1.2, DOT);
            x += step;
        }
        y += step;
    }
}

/// Pin header height — identical for every node, so the row maths cross-reference without a map.
const HEADER_H: f32 = 34.0;
const PIN_ROW_H: f32 = 26.0;
const NODE_PAD_Y: f32 = 8.0;

fn compute_node_rects(
    mission: &Mission,
    origin: Pos2,
    scale: f32,
) -> std::collections::HashMap<&'static str, Rect> {
    let mut out = std::collections::HashMap::new();
    for n in &mission.nodes {
        let rows = n.inputs.len().max(n.outputs.len()).max(1);
        let h = HEADER_H + NODE_PAD_Y * 2.0 + rows as f32 * PIN_ROW_H;
        let min = origin + (n.pos.to_vec2() * scale);
        let size = egui::vec2(n.width * scale, h * scale);
        out.insert(n.id, Rect::from_min_size(min, size));
    }
    out
}

/// Pin world-space position for a given (node_id, pin_id). `is_out` is false for the input rail.
fn pin_pos(
    mission: &Mission,
    rects: &std::collections::HashMap<&'static str, Rect>,
    node_id: &str,
    pin_id: &str,
) -> Option<(Pos2, PinTy, bool)> {
    let node = mission.nodes.iter().find(|n| n.id == node_id)?;
    let r = *rects.get(node_id)?;
    if let Some((i, pin)) = node
        .outputs
        .iter()
        .enumerate()
        .find(|(_, p)| p.id == pin_id)
    {
        let y = r.min.y + HEADER_H + NODE_PAD_Y + PIN_ROW_H * (i as f32 + 0.5);
        return Some((Pos2::new(r.max.x, y), pin.ty, true));
    }
    if let Some((i, pin)) = node.inputs.iter().enumerate().find(|(_, p)| p.id == pin_id) {
        let y = r.min.y + HEADER_H + NODE_PAD_Y + PIN_ROW_H * (i as f32 + 0.5);
        return Some((Pos2::new(r.min.x, y), pin.ty, false));
    }
    None
}

fn paint_wires(
    ui: &mut egui::Ui,
    mission: &Mission,
    rects: &std::collections::HashMap<&'static str, Rect>,
) {
    let p = ui.painter();
    for w in &mission.wires {
        let Some((a, _, _)) = pin_pos(mission, rects, w.from.0, w.from.1) else { continue };
        let Some((b, _, _)) = pin_pos(mission, rects, w.to.0, w.to.1) else { continue };
        let col = w.ty.colour();
        let stroke = Stroke::new(if matches!(w.ty, PinTy::Exec) { 3.0 } else { 2.4 }, col);
        bezier(p, a, b, stroke);
        // Port dots at each end.
        p.circle_filled(a, 3.6, col);
        p.circle_filled(b, 3.6, col);
    }
}

/// Cubic bezier approximation drawn as a chain of line segments — egui 0.28 has no first-class
/// path primitive, and a line-chain of 24 segments reads as smooth at the mockup's scales.
fn bezier(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let dx = (b.x - a.x).abs().max(40.0);
    let c1 = Pos2::new(a.x + dx * 0.55, a.y);
    let c2 = Pos2::new(b.x - dx * 0.55, b.y);
    let steps = 24;
    let mut prev = a;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        let p = Pos2::new(
            u * u * u * a.x
                + 3.0 * u * u * t * c1.x
                + 3.0 * u * t * t * c2.x
                + t * t * t * b.x,
            u * u * u * a.y
                + 3.0 * u * u * t * c1.y
                + 3.0 * u * t * t * c2.y
                + t * t * t * b.y,
        );
        painter.line_segment([prev, p], stroke);
        prev = p;
    }
}

fn paint_node(ui: &mut egui::Ui, n: &Node, rect: Rect, selected: bool) -> bool {
    let p = ui.painter();
    let rounding = egui::Rounding::same(10.0);
    // Drop shadow.
    p.rect_filled(
        rect.translate(egui::vec2(0.0, 6.0)),
        rounding,
        Color32::from_rgba_premultiplied(0, 0, 0, 90),
    );
    // Body.
    p.rect_filled(rect, rounding, NODE_BODY);
    let border = if selected { SEL_BORDER } else { NODE_BORDER };
    p.rect_stroke(rect, rounding, Stroke::new(if selected { 1.6 } else { 1.0 }, border));
    if selected {
        // Blue halo — a soft outer stroke, drawn as a slightly larger rect with the same colour at
        // reduced alpha. Keeps the hit area tight to `rect`.
        let halo = Rect::from_min_max(
            rect.min - egui::vec2(3.0, 3.0),
            rect.max + egui::vec2(3.0, 3.0),
        );
        p.rect_stroke(
            halo,
            egui::Rounding::same(13.0),
            Stroke::new(3.0, Color32::from_rgba_premultiplied(0x1e, 0x9b, 0xff, 90)),
        );
    }

    // Header.
    let (h_top, h_bot, h_text, eyebrow) = n.kind.header_colours();
    let header_rect = Rect::from_min_size(rect.min, egui::vec2(rect.width(), HEADER_H));
    paint_vgradient(p, header_rect, h_top, h_bot, egui::Rounding {
        nw: 10.0,
        ne: 10.0,
        sw: 0.0,
        se: 0.0,
    });
    p.text(
        header_rect.left_center() + egui::vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        n.title,
        egui::FontId::proportional(13.0),
        h_text,
    );
    p.text(
        header_rect.right_center() + egui::vec2(-10.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        eyebrow,
        egui::FontId::monospace(9.0),
        Color32::from_rgba_premultiplied(255, 255, 255, 110),
    );

    // Pin rows.
    let rows = n.inputs.len().max(n.outputs.len());
    for i in 0..rows {
        let y = rect.min.y + HEADER_H + NODE_PAD_Y + PIN_ROW_H * (i as f32 + 0.5);
        if let Some(pin) = n.inputs.get(i) {
            draw_pin(p, Pos2::new(rect.min.x, y), pin, false);
        }
        if let Some(pin) = n.outputs.get(i) {
            draw_pin(p, Pos2::new(rect.max.x, y), pin, true);
        }
    }

    // Click-to-select on the whole node rect.
    let id = egui::Id::new(("missions-node", n.id));
    let resp = ui.interact(rect, id, Sense::click());
    resp.clicked()
}

fn draw_pin(painter: &egui::Painter, port: Pos2, pin: &Pin, is_out: bool) {
    let text_pos = if is_out {
        port + egui::vec2(-12.0, 0.0)
    } else {
        port + egui::vec2(12.0, 0.0)
    };
    let align = if is_out {
        egui::Align2::RIGHT_CENTER
    } else {
        egui::Align2::LEFT_CENTER
    };
    // Pin shape.
    match pin.ty {
        PinTy::Exec => {
            // Diamond.
            let s = 6.0;
            let pts = vec![
                Pos2::new(port.x - s, port.y),
                Pos2::new(port.x, port.y - s),
                Pos2::new(port.x + s, port.y),
                Pos2::new(port.x, port.y + s),
            ];
            painter.add(egui::Shape::convex_polygon(
                pts,
                pin.ty.colour(),
                Stroke::new(1.0, Color32::from_black_alpha(220)),
            ));
        }
        _ => {
            painter.circle_filled(port, 5.0, pin.ty.colour());
            painter.circle_stroke(port, 5.0, Stroke::new(1.0, Color32::from_black_alpha(220)));
        }
    }
    let label_col = Color32::from_rgb(0xaa, 0xb4, 0xc2);
    painter.text(
        text_pos,
        align,
        pin.label,
        egui::FontId::proportional(12.0),
        label_col,
    );
    if let Some(annot) = pin.annot {
        let (annot_pos, annot_align) = if is_out {
            (port + egui::vec2(-12.0, 12.0), egui::Align2::RIGHT_CENTER)
        } else {
            (port + egui::vec2(12.0 + text_w(pin.label), 0.0), egui::Align2::LEFT_CENTER)
        };
        painter.text(
            annot_pos,
            annot_align,
            annot,
            egui::FontId::monospace(10.0),
            Color32::from_rgb(0x4a, 0x56, 0x66),
        );
    }
}

/// Rough mono/proportional width for layout. Pins carry at most ~12 chars, so a 7 px-per-char
/// bound is enough to space the annotation chip from the label without a layout pass.
fn text_w(s: &str) -> f32 {
    (s.chars().count() as f32) * 7.0 + 6.0
}

fn paint_vgradient(painter: &egui::Painter, rect: Rect, top: Color32, bot: Color32, rounding: egui::Rounding) {
    // Two-stop vertical gradient drawn as a stack of 1-px horizontal rects, so the header colour
    // reads the same as the mockup without needing a shader. 10 bands is enough at this height
    // for the eye not to see banding.
    let bands = 10;
    for i in 0..bands {
        let t = i as f32 / (bands - 1) as f32;
        let col = Color32::from_rgba_premultiplied(
            lerp8(top.r(), bot.r(), t),
            lerp8(top.g(), bot.g(), t),
            lerp8(top.b(), bot.b(), t),
            255,
        );
        let y0 = rect.min.y + rect.height() * i as f32 / bands as f32;
        let y1 = rect.min.y + rect.height() * (i + 1) as f32 / bands as f32;
        let band = Rect::from_min_max(
            Pos2::new(rect.min.x, y0),
            Pos2::new(rect.max.x, y1),
        );
        painter.rect_filled(
            band,
            if i == 0 { rounding } else { egui::Rounding::ZERO },
            col,
        );
    }
}

fn lerp8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 * (1.0 - t) + b as f32 * t).round().clamp(0.0, 255.0) as u8
}

fn paint_legend(ui: &mut egui::Ui, canvas: Rect) {
    let p = ui.painter();
    let margin = 14.0;
    let w = 360.0;
    let h = 28.0;
    let r = Rect::from_min_size(
        Pos2::new(canvas.min.x + margin, canvas.max.y - margin - h),
        egui::vec2(w, h),
    );
    p.rect_filled(
        r,
        egui::Rounding::same(10.0),
        Color32::from_rgba_premultiplied(0x11, 0x15, 0x1c, 220),
    );
    p.rect_stroke(r, egui::Rounding::same(10.0), Stroke::new(1.0, NODE_BORDER));
    let items = [
        ("exec", PinTy::Exec),
        ("str", PinTy::Str),
        ("int", PinTy::Int),
        ("obj", PinTy::Obj),
        ("arr", PinTy::Arr),
    ];
    let mut x = r.min.x + 11.0;
    p.text(
        Pos2::new(x, r.center().y),
        egui::Align2::LEFT_CENTER,
        "LEGEND",
        egui::FontId::monospace(10.0),
        theme::FAINT,
    );
    x += 60.0;
    for (label, ty) in items {
        if matches!(ty, PinTy::Exec) {
            let s = 5.0;
            let pts = vec![
                Pos2::new(x - s, r.center().y),
                Pos2::new(x, r.center().y - s),
                Pos2::new(x + s, r.center().y),
                Pos2::new(x, r.center().y + s),
            ];
            p.add(egui::Shape::convex_polygon(pts, ty.colour(), Stroke::NONE));
        } else {
            p.circle_filled(Pos2::new(x, r.center().y), 5.0, ty.colour());
        }
        p.text(
            Pos2::new(x + 10.0, r.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(11.0),
            theme::DIM,
        );
        x += 60.0;
    }
}

fn paint_zoom_pill(ui: &mut egui::Ui, canvas: Rect, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let margin = 14.0;
    let w = 140.0;
    let h = 30.0;
    let r = Rect::from_min_size(
        Pos2::new(canvas.max.x - margin - w, canvas.max.y - margin - h),
        egui::vec2(w, h),
    );
    let painter = ui.painter();
    painter.rect_filled(
        r,
        egui::Rounding::same(9.0),
        Color32::from_rgba_premultiplied(0x11, 0x15, 0x1c, 220),
    );
    painter.rect_stroke(r, egui::Rounding::same(9.0), Stroke::new(1.0, NODE_BORDER));
    let minus = Rect::from_min_size(r.min + egui::vec2(4.0, 2.0), egui::vec2(28.0, 26.0));
    let plus = Rect::from_min_max(
        Pos2::new(r.max.x - 32.0, r.min.y + 2.0),
        r.max - egui::vec2(4.0, 2.0),
    );
    painter.text(
        minus.center(),
        egui::Align2::CENTER_CENTER,
        "−",
        egui::FontId::proportional(15.0),
        theme::DIM,
    );
    painter.text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        format!("{}%", (p.zoom * 100.0).round() as i32),
        egui::FontId::monospace(11.0),
        theme::DIM,
    );
    painter.text(
        plus.center(),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(15.0),
        theme::DIM,
    );
    if ui
        .interact(minus, egui::Id::new("missions-zoom-minus"), Sense::click())
        .clicked()
    {
        acts.push(Act::ZoomDelta(-0.1));
    }
    if ui
        .interact(plus, egui::Id::new("missions-zoom-plus"), Sense::click())
        .clicked()
    {
        acts.push(Act::ZoomDelta(0.1));
    }
    if ui
        .interact(r, egui::Id::new("missions-zoom-reset"), Sense::click())
        .double_clicked()
    {
        acts.push(Act::ResetView);
    }
    acts
}

// ──────────────────────────────────────────────────────────────────────── inspector

pub fn inspector(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(node) = p.selected_node() else {
        ui.weak("No node selected — click one on the canvas.");
        return acts;
    };
    let (htop, hbot, _htext, eyebrow) = node.kind.header_colours();

    // ── Heading card: coloured chip, title, kind pills, blurb.
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::hover());
        paint_vgradient(ui.painter(), rect, htop, hbot, egui::Rounding::same(8.0));
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(node.title).size(15.0).color(theme::TX).strong());
            ui.horizontal(|ui| {
                pill_tag(ui, eyebrow, PinTy::Int /*only for colour fallback*/, theme::BRASS);
                if matches!(node.kind, NodeKind::Objective) {
                    pill_tag_neutral(ui, "child");
                }
            });
        });
    });
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(node.blurb)
            .size(12.0)
            .color(theme::FAINT),
    );
    ui.add_space(10.0);

    // ── Execution rows. For a selected lifecycle/objective node, this is the exec in / out wiring.
    theme::section(ui, "Execution", None, true, |ui| {
        for pin in &node.outputs {
            let swatch_col = pin.ty.colour();
            theme::row_chip(ui, egui::Color32::TRANSPARENT, theme::LINE, |ui| {
                swatch(
                    ui,
                    swatch_col,
                    matches!(pin.ty, PinTy::Exec),
                );
                ui.label(egui::RichText::new(pin.label).size(12.0).color(theme::TX));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(pin.annot.unwrap_or("(unbound)"))
                            .monospace()
                            .size(10.0)
                            .color(theme::FAINT),
                    );
                });
            });
        }
    });

    // ── Parameters. Each input pin that is not Exec is a parameter row.
    theme::section(ui, "Parameters", None, true, |ui| {
        for pin in &node.inputs {
            if matches!(pin.ty, PinTy::Exec) {
                continue;
            }
            theme::row_chip(ui, egui::Color32::TRANSPARENT, theme::LINE, |ui| {
                swatch(ui, pin.ty.colour(), false);
                ui.label(egui::RichText::new(pin.label).size(12.0).color(theme::TX));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(pin.ty.short())
                            .monospace()
                            .size(9.5)
                            .color(theme::FAINT),
                    );
                    if let Some(annot) = pin.annot {
                        ui.label(
                            egui::RichText::new(annot)
                                .monospace()
                                .size(10.0)
                                .color(theme::DIM),
                        );
                    }
                });
            });
        }
    });

    // ── HUD preview — only when the selected node is a known HUD-emitter.
    if node.id == "race" {
        theme::section(ui, "HUD preview", Some("LIVE"), true, |ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().min(260.0), 56.0),
                Sense::hover(),
            );
            let p = ui.painter();
            p.rect_filled(
                rect,
                egui::Rounding::same(8.0),
                Color32::from_rgb(0x12, 0x0d, 0x06),
            );
            p.rect_stroke(
                rect,
                egui::Rounding::same(8.0),
                Stroke::new(
                    1.0,
                    Color32::from_rgba_premultiplied(0xf0, 0xa9, 0x2b, 60),
                ),
            );
            p.text(
                rect.left_top() + egui::vec2(14.0, 14.0),
                egui::Align2::LEFT_TOP,
                "RACE  ·  CHECKPOINT 7 / 27",
                egui::FontId::proportional(12.5),
                Color32::from_rgb(0xf6, 0xd9, 0x8a),
            );
            // Progress bar (36% of 1:15).
            let bar = Rect::from_min_size(
                rect.left_bottom() + egui::vec2(14.0, -14.0),
                egui::vec2(rect.width() - 28.0, 5.0),
            );
            p.rect_filled(
                bar,
                egui::Rounding::same(3.0),
                Color32::from_rgba_premultiplied(0xf0, 0xa9, 0x2b, 40),
            );
            let fill = Rect::from_min_size(bar.min, egui::vec2(bar.width() * 0.36, bar.height()));
            p.rect_filled(fill, egui::Rounding::same(3.0), Color32::from_rgb(0xf0, 0xa9, 0x2b));
        });
        ui.label(
            egui::RichText::new("[PmcCon016.Objectives.002]")
                .monospace()
                .size(10.0)
                .color(theme::FAINT),
        );
    }
    acts
}

fn pill_tag(ui: &mut egui::Ui, text: &str, _ty: PinTy, border: Color32) {
    egui::Frame::none()
        .stroke(Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(7.0, 2.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .monospace()
                    .size(9.0)
                    .color(border),
            );
        });
}

fn pill_tag_neutral(ui: &mut egui::Ui, text: &str) {
    egui::Frame::none()
        .stroke(Stroke::new(1.0, theme::LINE))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(7.0, 2.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .monospace()
                    .size(9.0)
                    .color(theme::FAINT),
            );
        });
}

// ──────────────────────────────────────────────────────────────────────── verbs

pub fn verbs(ui: &mut egui::Ui, _p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    if ui.button("Jump to source").clicked() {
        acts.push(Act::JumpToSource);
    }
    if ui.button("Add node").on_hover_text("Tab").clicked() {
        acts.push(Act::AddNode);
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if theme::primary_button(ui, "\u{25B6} Compile", true).clicked() {
            acts.push(Act::Compile);
        }
    });
    acts
}
