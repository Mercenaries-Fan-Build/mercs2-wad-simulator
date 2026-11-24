//! The Driving page's boat systems: the benches, cards and status lines of a template with a
//! `_BoatPhysics` record. Hull draws the four flotation springs' reach, strength and dampening as
//! speed rises, Powertrain the thrust the engine curve gives ahead and astern, Steering the
//! steering gain at each speed.
//!
//! A boat floats on four springs at the corners of its hull, each blended from its low-speed
//! values to its high-speed ones by speed over `MaxSpeed` (`FUN_004477a0`).

use crate::drivingdata::{Fields, Fleet};
use crate::drivingmath::{self, BoatSteering};
use crate::drivingpage::{self as page, Panel, System, Vehicle};
use crate::drivingtank;
use crate::gui::theme;
use crate::plot::{self, number, Plot};
use egui::{Align2, Stroke};

/// The one `_BoatPhysics` field the PC executable never reads: its loader asks for
/// `fShallowAngDamp` (`0xBC80BC`), a name the record's schema does not have, and takes 0.
pub const UNREAD: [&str; 1] = ["ShallowAngDamp · read as fShallowAngDamp"];

fn steering(f: &Fields) -> Result<BoatSteering, String> {
    Ok(BoatSteering {
        steering: f.num("Steering")?,
        no_speed_steering: f.num("NoSpeedSteering")?,
        low_speed_steering: f.num("LowSpeedSteering")?,
        low_speed_limit: f.num("LowSpeedLimit")?,
        high_speed_steer_loss: f.num("HighSpeedSteerLoss")?,
        max_speed: f.num("MaxSpeed")?,
        max_speed_reverse: f.num("MaxSpeedReverse")?,
    })
}

pub(crate) fn status(p: &Panel, f: &Fields) -> Option<String> {
    Some(match p.system {
        System::Hull => format!(
            "4 springs · strength {} at rest, {} at {} km/h · mass {}",
            number(f.num("SpringLoSpeedStrength").ok()?),
            number(f.num("SpringHiSpeedStrength").ok()?),
            number(f.num("MaxSpeed").ok()?),
            number(f.num("Mass").ok()?)
        ),
        System::Powertrain => {
            let (c, torque, cap) = (drivingtank::curve(f).ok()?, f.num("EngineTorque").ok()?, f.num("MaxSpeed").ok()?);
            format!("thrust {} at rest · none from {} km/h · {} km/h astern", number(drivingmath::thrust(&c, torque, cap, 0.0)), number(cap), number(f.num("MaxSpeedReverse").ok()?))
        }
        System::Steering => {
            let s = steering(f).ok()?;
            format!("gain {:.2} stopped on throttle · {:.2} at {} km/h", drivingmath::boat_steer_gain(&s, 1.0, 0.0), drivingmath::boat_steer_gain(&s, 1.0, s.max_speed), number(s.max_speed))
        }
        _ => page::coupling_status(p),
    })
}

pub(crate) fn bench(ui: &mut egui::Ui, p: &Panel, _v: &Vehicle, f: &Fields) {
    match p.system {
        System::Hull => hull_bench(ui, f),
        System::Powertrain => drivingtank::thrust_bench(ui, f, "MaxSpeed", "EngineTorque", Some(("MaxSpeedReverse", "EngineReverseTorque"))),
        System::Steering => steering_bench(ui, f),
        _ => {}
    }
}

fn hull_bench(ui: &mut egui::Ui, f: &Fields) {
    page::bench_header(ui, &[("Spring reach", "offsets against km/h")]);
    let (area, painter) = page::bench_area(ui);
    let num = |name: &str| f.num(name);
    let (Ok(cap), Ok(start_lo), Ok(start_hi), Ok(end_lo), Ok(end_hi)) =
        (num("MaxSpeed"), num("SpringLoSpeedStartOffset"), num("SpringHiSpeedStartOffset"), num("SpringLoSpeedEndOffset"), num("SpringHiSpeedEndOffset"))
    else {
        return;
    };
    let (Ok(k_lo), Ok(k_hi), Ok(d_lo), Ok(d_hi)) = (num("SpringLoSpeedStrength"), num("SpringHiSpeedStrength"), num("SpringLoSpeedDampening"), num("SpringHiSpeedDampening")) else { return };
    let (reach_r, force_r) = page::split(area, 0.5);
    plot::label(&painter, egui::pos2(force_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, "STRENGTH and DAMPENING against km/h", theme::DIM);
    if cap <= 0.0 {
        return;
    }
    let x_max = page::speed_axis(cap);
    let x_ticks = page::labelled(plot::ticks(20.0, x_max, 20.0));

    // The springs' start and end offsets: each a straight line from its low-speed value at rest to
    // its high-speed value at the cap, level past it.
    let (lo, hi) = ([start_lo, start_hi, end_lo, end_hi].into_iter().fold(0.0_f32, f32::min), [start_lo, start_hi, end_lo, end_hi].into_iter().fold(0.0_f32, f32::max));
    let pad = ((hi - lo) * 0.125).max(0.05);
    let reach = Plot::chart(reach_r, (0.0, x_max), (lo - pad, hi + pad));
    plot::axes(&painter, &reach, &x_ticks, &page::labelled(vec![lo, 0.0, hi]));
    plot::dashed_up(&painter, &reach, cap, theme::BRASS_DK);
    plot::dashed_across(&painter, &reach, 0.0, theme::FAINT);
    for (a, b, col, name) in [(start_lo, start_hi, theme::BRASS, "start"), (end_lo, end_hi, theme::TX, "end")] {
        speed_line(&painter, &reach, cap, x_max, (a, b), col, name);
    }

    let top = [k_lo, k_hi, d_lo, d_hi].into_iter().fold(0.0_f32, f32::max);
    if top <= 0.0 {
        return;
    }
    let force = Plot::chart(force_r, (0.0, x_max), (0.0, top * 1.125));
    plot::axes(&painter, &force, &x_ticks, &page::labelled(vec![0.0, top / 2.0, top]));
    plot::dashed_up(&painter, &force, cap, theme::BRASS_DK);
    for (a, b, col, name) in [(k_lo, k_hi, theme::BRASS, "strength"), (d_lo, d_hi, theme::TX, "dampening")] {
        speed_line(&painter, &force, cap, x_max, (a, b), col, name);
    }
}

/// A spring value's line: from its low-speed value at rest to its high-speed value at the cap,
/// level past it, with both values written on the side of the line it leaves clear.
fn speed_line(painter: &egui::Painter, chart: &Plot, cap: f32, x_max: f32, (a, b): (f32, f32), col: egui::Color32, name: &str) {
    plot::line(painter, chart, &[(0.0, a), (cap, b), (x_max, b)], Stroke::new(2.0, col));
    let (start, end) = if b > a { (Align2::LEFT_TOP, Align2::RIGHT_BOTTOM) } else { (Align2::LEFT_BOTTOM, Align2::RIGHT_TOP) };
    let off = |align: Align2| if align == Align2::LEFT_TOP || align == Align2::RIGHT_TOP { 4.0 } else { -3.0 };
    plot::label(painter, chart.at(0.0, a) + egui::vec2(6.0, off(start)), start, format!("{name} {}", number(a)), col);
    plot::label(painter, chart.at(cap, b) + egui::vec2(-6.0, off(end)), end, number(b), col);
}

fn steering_bench(ui: &mut egui::Ui, f: &Fields) {
    page::bench_header(ui, &[("Steering gain", "against km/h")]);
    let (area, painter) = page::bench_area(ui);
    let (Ok(s), Ok(held), Ok(released)) = (steering(f), f.num("SteerInertia"), f.num("SteerNoInputInertia")) else { return };
    let (gain_r, resp_r) = page::split(area, 0.6);
    plot::label(&painter, egui::pos2(resp_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, "RESPONSE share against seconds", theme::DIM);
    // The key, beside the title.
    let mut x = area.left() + 210.0;
    for (name, col) in [("on throttle", theme::BRASS), ("coasting", theme::TX)] {
        painter.line_segment([egui::pos2(x, area.top() - 19.0), egui::pos2(x + 14.0, area.top() - 19.0)], Stroke::new(2.0, col));
        let galley = painter.layout_no_wrap(name.to_string(), theme::font(&egui::TextStyle::Small), theme::DIM);
        let w = galley.size().x;
        painter.galley(egui::pos2(x + 19.0, area.top() - 19.0 - galley.size().y / 2.0), galley, theme::DIM);
        x += w + 33.0;
    }
    if s.max_speed <= 0.0 {
        return;
    }
    let x_max = page::speed_axis(s.max_speed);
    let speeds: Vec<f32> = (0..=200).map(|i| x_max * i as f32 / 200.0).collect();
    let line = |throttle: f32| -> Vec<(f32, f32)> { speeds.iter().map(|&kmh| (kmh, drivingmath::boat_steer_gain(&s, throttle, kmh))).collect() };
    let (on, off) = (line(1.0), line(0.0));
    let top = on.iter().chain(&off).map(|p| p.1).fold(0.0_f32, f32::max);
    if top <= 0.0 {
        return;
    }
    let gain = Plot::chart(gain_r, (0.0, x_max), (0.0, top * 1.125));
    plot::axes(&painter, &gain, &page::labelled(plot::ticks(20.0, x_max, 20.0)), &page::labelled(vec![0.0, top / 2.0, top]));
    plot::dashed_up(&painter, &gain, s.max_speed, theme::BRASS_DK);
    plot::label(&painter, gain.at(s.max_speed, top * 1.125) + egui::vec2(-4.0, 4.0), Align2::RIGHT_TOP, format!("max speed {}", number(s.max_speed)), theme::BRASS);
    if s.low_speed_limit > 0.0 && s.low_speed_limit < x_max {
        plot::dashed_up(&painter, &gain, s.low_speed_limit, theme::FAINT);
        plot::label(&painter, gain.at(s.low_speed_limit, 0.0) + egui::vec2(4.0, -4.0), Align2::LEFT_BOTTOM, format!("low speed limit {}", number(s.low_speed_limit)), theme::DIM);
    }
    plot::line(&painter, &gain, &off, Stroke::new(1.6, theme::TX));
    plot::line(&painter, &gain, &on, Stroke::new(2.0, theme::BRASS));

    let seconds = 3.0;
    let times: Vec<f32> = (0..=60).map(|i| seconds * i as f32 / 60.0).collect();
    let resp = Plot::chart(resp_r, (0.0, seconds), (0.0, 1.0));
    plot::axes(&painter, &resp, &page::labelled(plot::ticks(1.0, seconds, 1.0)), &page::labelled(vec![0.0, 0.5, 1.0]));
    let release: Vec<(f32, f32)> = times.iter().map(|&t| (t, drivingmath::remaining(released, t))).collect();
    let turn_in: Vec<(f32, f32)> = times.iter().map(|&t| (t, 1.0 - drivingmath::remaining(held, t))).collect();
    plot::line(&painter, &resp, &release, Stroke::new(1.6, theme::TX));
    plot::line(&painter, &resp, &turn_in, Stroke::new(2.0, theme::BRASS));
    plot::label(&painter, resp.at(1.4, 0.62), Align2::LEFT_TOP, format!("turning in · {}", number(held)), theme::BRASS);
    plot::label(&painter, resp.at(1.4, 0.22), Align2::LEFT_TOP, format!("released · {}", number(released)), theme::TX);
}

pub(crate) fn cards(ui: &mut egui::Ui, p: &Panel, f: &Fields, fleet: &Fleet) {
    let templates = format!("{} boat templates", fleet.templates);
    match p.system {
        System::Hull => {
            theme::card(ui, "Springs", Some(&templates), |ui| {
                let side = |name: &str| f.num(name).ok().map(|v| (v, fleet.place(name, v)));
                page::paired(
                    ui,
                    ["LO SPEED", "HI SPEED"],
                    &[
                        ("Start offset", [side("SpringLoSpeedStartOffset"), side("SpringHiSpeedStartOffset")]),
                        ("End offset", [side("SpringLoSpeedEndOffset"), side("SpringHiSpeedEndOffset")]),
                        ("Strength", [side("SpringLoSpeedStrength"), side("SpringHiSpeedStrength")]),
                        ("Dampening", [side("SpringLoSpeedDampening"), side("SpringHiSpeedDampening")]),
                    ],
                );
                ui.separator();
                if let (Ok(x), Ok(z)) = (f.word("NumSamplesPerSpringX"), f.word("NumSamplesPerSpringZ")) {
                    page::ranged(ui, fleet, 128.0, "Samples per spring", format!("{x} × {z}"), None, true);
                }
                page::field_row(ui, fleet, f, "Water smoothing", "SpringWaterSmoothing", "");
            });
            theme::card(ui, "Water", None, |ui| {
                page::field_row(ui, fleet, f, "Drag fwd", "WaterDragFwd", "");
                page::field_row(ui, fleet, f, "Drag side", "WaterDragSide", "");
                page::field_row(ui, fleet, f, "Drag up", "WaterDragUp", "");
                page::field_row(ui, fleet, f, "Ang damp roll", "AngDampRoll", "");
                page::field_row(ui, fleet, f, "Ang damp pitch", "AngDampPitch", "");
                page::field_row(ui, fleet, f, "Ang damp yaw", "AngDampYaw", "");
                page::field_row(ui, fleet, f, "Shallow depth", "ShallowDepth", "");
                page::field_row(ui, fleet, f, "Shallow lin damp", "ShallowLinDamp", "");
                page::field_row(ui, fleet, f, "Gravity out, rising", "OutOfWaterGravityFactorUp", "");
                page::field_row(ui, fleet, f, "Gravity out, falling", "OutOfWaterGravityFactorDown", "");
            });
            theme::card(ui, "Body", None, |ui| {
                page::field_row(ui, fleet, f, "Mass", "Mass", "");
                if let Ok(at) = f.vec3("CenterOfMassOffset") {
                    page::ranged(ui, fleet, 96.0, "Centre of mass", page::position(at), None, true);
                }
                page::field_row(ui, fleet, f, "Hull friction", "HullFriction", "");
                page::field_row(ui, fleet, f, "Collision stabilization", "CollisionStabilization", "");
                if let Ok(flip) = f.word("FlipOnLandEnable") {
                    page::ranged(ui, fleet, 128.0, "Flip on land", if flip != 0 { "True" } else { "False" }.to_string(), None, true);
                }
            });
            theme::card(ui, "Lean", None, |ui| {
                page::field_row(ui, fleet, f, "Roll from turning", "RollFromTurning", "rad");
                page::field_row(ui, fleet, f, "Roll inertia", "RollInertia", "");
                page::field_row(ui, fleet, f, "Pitch from acceleration", "PitchFromAcceleration", "rad");
                page::field_row(ui, fleet, f, "Pitch up speed", "PitchUpSpeed", "");
                page::field_row(ui, fleet, f, "Pitch down inertia", "PitchDownInertia", "");
            });
            theme::card(ui, "Wake", None, |ui| {
                if let Ok(at) = f.vec3("WakeOffset") {
                    page::ranged(ui, fleet, 96.0, "Offset", page::position(at), None, true);
                }
                page::field_row(ui, fleet, f, "Rate", "WakeRate", "");
                page::field_row(ui, fleet, f, "Speed", "WakeSpeed", "");
                page::field_row(ui, fleet, f, "Life time", "WakeLifeTime", "");
                page::field_row(ui, fleet, f, "Size", "WakeSize", "");
            });
            page::unread(ui, &UNREAD);
        }
        System::Powertrain => {
            theme::card(ui, "Engine", Some(&templates), |ui| {
                page::field_row(ui, fleet, f, "Engine torque", "EngineTorque", "");
                page::field_row(ui, fleet, f, "Reverse torque", "EngineReverseTorque", "");
                page::field_row(ui, fleet, f, "Max speed", "MaxSpeed", "km/h");
                page::field_row(ui, fleet, f, "Max speed reverse", "MaxSpeedReverse", "km/h");
                page::field_row(ui, fleet, f, "Engine inertia", "EngineInertia", "");
                page::field_row(ui, fleet, f, "Torque × at collision", "EngineTorqueMultiAtCollision", "");
                page::field_row(ui, fleet, f, "Brake drag", "BrakeDrag", "");
            });
            drivingtank::curve_card(ui, f, fleet);
        }
        System::Steering => {
            theme::card(ui, "Steering", Some(&templates), |ui| {
                page::field_row(ui, fleet, f, "Steering", "Steering", "");
                page::field_row(ui, fleet, f, "No speed steering", "NoSpeedSteering", "");
                page::field_row(ui, fleet, f, "Low speed steering", "LowSpeedSteering", "");
                page::field_row(ui, fleet, f, "Low speed limit", "LowSpeedLimit", "km/h");
                page::field_row(ui, fleet, f, "High speed steer loss", "HighSpeedSteerLoss", "");
                page::field_row(ui, fleet, f, "Inertia", "SteerInertia", "");
                page::field_row(ui, fleet, f, "No input inertia", "SteerNoInputInertia", "");
                page::field_row(ui, fleet, f, "Air ctrl steering", "AirCtrlSteering", "");
            });
            theme::card(ui, "Handbrake", None, |ui| {
                page::field_row(ui, fleet, f, "Inertia", "HandBrakeInertia", "");
                page::field_row(ui, fleet, f, "Steer boost", "HandBrakeSteerBoost", "");
                page::field_row(ui, fleet, f, "Anti side drag", "HandBrakeAntiSideDrag", "");
                page::field_row(ui, fleet, f, "Momentum keep", "HandBrakeMomentumKeep", "");
            });
            theme::card(ui, "Rudder", None, |ui| {
                if let Ok(bone) = f.word("VisRudderBone") {
                    page::ranged(ui, fleet, 96.0, "Bone", if bone == 0 { "None".to_string() } else { p.node_label(bone) }, None, true);
                }
                page::field_row(ui, fleet, f, "Max angle", "VisRudderMaxAngle", "");
                page::field_row(ui, fleet, f, "Inertia", "VisRudderInertia", "");
            });
        }
        _ => {}
    }
}
