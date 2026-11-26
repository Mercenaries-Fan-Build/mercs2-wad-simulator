//! The Driving page's tank systems: the benches, cards, viewport marks and status lines of a
//! template with a `_TankPhysics` record. Chassis draws the running gear to scale, Powertrain the
//! thrust the engine curve gives at each speed, Steering what the steering keeps as speed rises.
//!
//! A tank rides on six ground probes, not on its wheels: each starts at one of six model nodes
//! (`FUN_00454de0`, names at `0xBAA428`–`0xBAA464`). Its wheels are `BoneCtrlFakeWheel` records,
//! drawn and never loaded.

use crate::drivingdata::{FakeWheel, Fields, Fleet};
use crate::drivingmath::{self, TorqueCurve};
use crate::drivingpage::{self as page, Marks, Overlay, Panel, System, Vehicle};
use crate::gui::theme;
use crate::plot::{self, number, Plot};
use egui::{Align2, Stroke};
use mercs2_formats::hash::pandemic_hash_m2;

/// The nodes a tank's six ground probes start from, in the game's slot order.
pub const PROBE_NODES: [&str; 6] = ["hp_wheel_fl", "hp_wheel_fr", "hp_wheel_rl", "hp_wheel_rr", "hp_wheel_ml", "hp_wheel_mr"];

/// The engine torque curve of a tank or a boat record.
pub(crate) fn curve(f: &Fields) -> Result<TorqueCurve, String> {
    Ok(TorqueCurve {
        ratios: [f.num("EngineTorqueRatio0")?, f.num("EngineTorqueRatio1")?, f.num("EngineTorqueRatio2")?, f.num("EngineTorqueRatio3")?],
        pos: [f.num("EngineTorquePos1")?, f.num("EngineTorquePos2")?],
    })
}

/// Where the probe nodes the model has are, in slot order.
fn probes(p: &Panel) -> Vec<[f32; 3]> {
    PROBE_NODES.iter().filter_map(|name| p.node(pandemic_hash_m2(name))).collect()
}

pub(crate) fn status(p: &Panel, f: &Fields, wheels: &[FakeWheel]) -> Option<String> {
    Some(match p.system {
        System::Chassis => {
            let sprung = wheels.iter().filter(|w| w.suspension != 0).count();
            format!("{} wheels · {sprung} on suspension · {} ground probes · mass {}", wheels.len(), probes(p).len(), number(f.num("Mass").ok()?))
        }
        System::Powertrain => {
            let (c, torque, cap) = (curve(f).ok()?, f.num("EngineTorque").ok()?, f.num("MaxSpeed").ok()?);
            format!("thrust {} at rest · none from {} km/h", number(drivingmath::thrust(&c, torque, cap, 0.0)), number(cap))
        }
        System::Steering => {
            let (loss, cap) = (f.num("HiSpeedSteerLoss").ok()?, f.num("MaxSpeed").ok()?);
            format!("steer speed {} · keeps {:.2} of it at {} km/h", number(f.num("SteerSpeed").ok()?), drivingmath::tank_steer_factor(loss, cap, cap), number(cap))
        }
        _ => page::coupling_status(p),
    })
}

pub(crate) fn bench(ui: &mut egui::Ui, p: &Panel, _v: &Vehicle, f: &Fields, wheels: &[FakeWheel]) {
    match p.system {
        System::Chassis => chassis_bench(ui, p, f, wheels),
        System::Powertrain => thrust_bench(ui, f, "MaxSpeed", "EngineTorque", None),
        System::Steering => steering_bench(ui, f),
        _ => {}
    }
}

fn chassis_bench(ui: &mut egui::Ui, p: &Panel, f: &Fields, wheels: &[FakeWheel]) {
    let placed: Vec<(&FakeWheel, [f32; 3])> = wheels.iter().filter_map(|w| p.node(w.wheel).map(|at| (w, at))).collect();
    let probes = probes(p);
    page::bench_header(ui, &[("Blueprint", &format!("{} wheels · {} ground probes · metres", wheels.len(), probes.len()))]);
    let (area, painter) = page::bench_area(ui);
    if placed.is_empty() {
        return;
    }
    let (exp, cmp) = (f.num("SpringExpLength").unwrap_or(0.0), f.num("SpringCmpLength").unwrap_or(0.0));
    let ground = placed.iter().map(|(w, at)| at[1] - w.radius).fold(f32::MAX, f32::min);
    let (mut z_lo, mut z_hi, mut top, mut wide) = (f32::MAX, f32::MIN, ground, 0.0_f32);
    for (w, at) in &placed {
        z_lo = z_lo.min(at[2] - w.radius);
        z_hi = z_hi.max(at[2] + w.radius);
        top = top.max(at[1] + w.radius);
        wide = wide.max(at[0].abs());
    }
    for at in &probes {
        z_lo = z_lo.min(at[2]);
        z_hi = z_hi.max(at[2]);
        top = top.max(at[1] + cmp);
        wide = wide.max(at[0].abs());
    }
    let (z_lo, z_hi, top, wide) = (z_lo - 0.4, z_hi + 0.4, top + 0.3, wide + 0.5);
    let (left, right) = page::split(area, 0.5);
    let inset = |r: egui::Rect| egui::Rect::from_min_max(r.min + egui::vec2(0.0, 16.0), r.max - egui::vec2(0.0, 34.0));
    let scale = Plot::to_scale(inset(left), (z_hi, z_lo), (ground, top)).per_unit().abs().min(Plot::to_scale(inset(right), (z_hi, z_lo), (-wide, wide)).per_unit().abs());
    let side = Plot::at_scale(inset(left), (z_hi, z_lo), (ground, top), scale);
    let plan = Plot::at_scale(inset(right), (z_hi, z_lo), (-wide, wide), scale);
    let (tire, dim, probe) = (Stroke::new(1.4, theme::TX), Stroke::new(1.0, theme::FAINT), Stroke::new(2.0, theme::GOOD));
    painter.text(left.left_top(), Align2::LEFT_TOP, "SIDE", theme::font(&theme::style_eyebrow()), theme::DIM);
    painter.text(right.left_top(), Align2::LEFT_TOP, "TOP", theme::font(&theme::style_eyebrow()), theme::DIM);

    // Side view: the ground, the wheels of the near side, each probe's ray from its top to its end.
    painter.line_segment([egui::pos2(left.left(), side.y(ground)), egui::pos2(left.right() - 10.0, side.y(ground))], Stroke::new(1.0, theme::LINE2));
    for (w, at) in placed.iter().filter(|(_, at)| at[0] <= 0.0) {
        let c = side.at(at[2], at[1]);
        painter.circle(c, w.radius * scale, theme::G2, tire);
        painter.circle_filled(c, 2.2, theme::TX);
    }
    let mut probe_z: Vec<f32> = Vec::new();
    for at in &probes {
        if !probe_z.iter().any(|z| (z - at[2]).abs() < 0.05) {
            probe_z.push(at[2]);
        }
    }
    probe_z.sort_by(|a, b| b.total_cmp(a));
    for z in &probe_z {
        let Some(at) = probes.iter().find(|at| (at[2] - z).abs() < 0.05) else { continue };
        let (hi, lo) = (side.at(*z, at[1] + cmp), side.at(*z, at[1] - exp));
        painter.line_segment([hi, lo], probe);
        painter.line_segment([lo + egui::vec2(-5.0, 0.0), lo + egui::vec2(5.0, 0.0)], probe);
        painter.line_segment([hi + egui::vec2(-5.0, 0.0), hi + egui::vec2(5.0, 0.0)], probe);
    }
    // The distance between neighbouring probes along the hull, under the ground line.
    let rule = side.y(ground) + 14.0;
    for z in &probe_z {
        painter.line_segment([egui::pos2(side.x(*z), rule - 6.0), egui::pos2(side.x(*z), rule + 6.0)], dim);
    }
    for pair in probe_z.windows(2) {
        painter.line_segment([egui::pos2(side.x(pair[0]), rule), egui::pos2(side.x(pair[1]), rule)], dim);
        plot::label(&painter, egui::pos2((side.x(pair[0]) + side.x(pair[1])) / 2.0, rule + 8.0), Align2::CENTER_TOP, number(((pair[0] - pair[1]) * 100.0).round() / 100.0), theme::TX);
    }

    // Top view: the centre line, every wheel, the probes and the width between them.
    let mid = plan.y(0.0);
    painter.extend(egui::Shape::dashed_line(&[egui::pos2(right.left(), mid), egui::pos2(right.right(), mid)], Stroke::new(1.0, theme::LINE), 3.0, 4.0));
    for (w, at) in &placed {
        let size = egui::vec2(2.0 * w.radius * scale, 8.0);
        painter.rect(egui::Rect::from_center_size(plan.at(at[2], at[0]), size), egui::CornerRadius::same(3), theme::G2, tire, egui::StrokeKind::Middle);
    }
    for at in &probes {
        let c = plan.at(at[2], at[0]);
        painter.line_segment([c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)], probe);
        painter.line_segment([c + egui::vec2(0.0, -5.0), c + egui::vec2(0.0, 5.0)], probe);
    }
    if let Some(z) = probe_z.first() {
        let (lo, hi) = probes.iter().filter(|at| (at[2] - z).abs() < 0.05).fold((f32::MAX, f32::MIN), |(l, h), at| (l.min(at[0]), h.max(at[0])));
        if hi > lo {
            plot::label(&painter, egui::pos2(plan.x(*z) + 10.0, mid + 9.0), Align2::LEFT_CENTER, number(((hi - lo) * 100.0).round() / 100.0), theme::TX);
        }
    }
    for z in &probe_z {
        plot::label(&painter, egui::pos2(plan.x(*z), plan.area.bottom() + 6.0), Align2::CENTER_TOP, number((z * 100.0).round() / 100.0), theme::DIM);
    }
    // The key, at the right of the header row.
    let key_y = area.top() - 19.0;
    let mut x = area.right();
    for (name, is_probe) in [("wheel", false), ("ground probe", true)] {
        let galley = painter.layout_no_wrap(name.to_string(), theme::font(&egui::TextStyle::Small), theme::DIM);
        x -= galley.size().x;
        painter.galley(egui::pos2(x, key_y - galley.size().y / 2.0), galley, theme::DIM);
        x -= 10.0;
        let c = egui::pos2(x, key_y);
        if is_probe {
            painter.line_segment([c + egui::vec2(0.0, -6.0), c + egui::vec2(0.0, 6.0)], probe);
        } else {
            painter.circle_stroke(c, 4.0, tire);
        }
        x -= 16.0;
    }
}

/// The thrust chart a tank and a boat share: thrust against speed with the throttle held, the
/// curve's knots, the cap, and beside it how fast the engine state follows the throttle.
/// `reverse` names the reverse cap and reverse torque fields of a record that has them.
pub(crate) fn thrust_bench(ui: &mut egui::Ui, f: &Fields, cap_field: &str, torque_field: &str, reverse: Option<(&str, &str)>) {
    page::bench_header(ui, &[("Thrust", "against km/h · throttle held")]);
    let (area, painter) = page::bench_area(ui);
    let (Ok(c), Ok(torque), Ok(cap), Ok(inertia)) = (curve(f), f.num(torque_field), f.num(cap_field), f.num("EngineInertia")) else { return };
    let (thrust_r, resp_r) = page::split(area, 0.63);
    plot::label(&painter, egui::pos2(resp_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, "ENGINE share against seconds", theme::DIM);
    if cap <= 0.0 || torque <= 0.0 {
        return;
    }
    let x_max = page::speed_axis(cap);
    let peak = c.ratios.iter().copied().fold(0.0_f32, f32::max) * torque * drivingmath::THRUST_SCALE;
    let y_max = peak * 1.125;
    let chart = Plot::chart(thrust_r, (0.0, x_max), (0.0, y_max));
    let thousands = |v: f32| (v, format!("{}k", number((v / 100.0).round() / 10.0)));
    plot::axes(&painter, &chart, &page::labelled(plot::ticks(20.0, x_max, 20.0)), &[thousands(0.0), thousands(peak / 2.0), thousands(peak)]);
    plot::dashed_up(&painter, &chart, cap, theme::BRASS_DK);
    plot::label(&painter, chart.at(cap, y_max) + egui::vec2(-4.0, 4.0), Align2::RIGHT_TOP, format!("cap {}", number(cap)), theme::BRASS);
    if let Some((reverse_cap, reverse_torque)) = reverse {
        if let (Ok(rc), Ok(rt)) = (f.num(reverse_cap), f.num(reverse_torque)) {
            let pts: Vec<(f32, f32)> = c.knots().iter().map(|(x, y)| (x * rc, y * rt * drivingmath::THRUST_SCALE)).collect();
            plot::line(&painter, &chart, &pts, Stroke::new(1.6, theme::TX));
            if let Some(&(x, y)) = pts.first() {
                plot::label(&painter, chart.at(x, y) + egui::vec2(6.0, 4.0), Align2::LEFT_TOP, format!("reverse · cap {}", number(rc)), theme::TX);
            }
        }
    }
    let pts: Vec<(f32, f32)> = c.knots().iter().map(|(x, y)| (x * cap, y * torque * drivingmath::THRUST_SCALE)).collect();
    plot::line(&painter, &chart, &pts, Stroke::new(2.0, theme::BRASS));
    for (x, y) in &pts {
        painter.circle_filled(chart.at(*x, *y), 3.0, theme::BRASS);
    }
    if let Some(&(x, y)) = pts.last() {
        plot::line(&painter, &chart, &[(x, y), (x, 0.0), (x_max, 0.0)], Stroke::new(2.0, theme::BRASS));
    }

    // How much of the throttle the engine state has taken up, over three seconds.
    let seconds = 3.0;
    let resp = Plot::chart(resp_r, (0.0, seconds), (0.0, 1.0));
    plot::axes(&painter, &resp, &page::labelled(plot::ticks(1.0, seconds, 1.0)), &page::labelled(vec![0.0, 0.5, 1.0]));
    let pts: Vec<(f32, f32)> = (0..=60).map(|i| seconds * i as f32 / 60.0).map(|t| (t, 1.0 - drivingmath::remaining(inertia, t))).collect();
    plot::line(&painter, &resp, &pts, Stroke::new(2.0, theme::BRASS));
    plot::label(&painter, resp.at(seconds, 0.3) + egui::vec2(-4.0, 0.0), Align2::RIGHT_TOP, format!("engine inertia · {}", number(inertia)), theme::BRASS);
}

fn steering_bench(ui: &mut egui::Ui, f: &Fields) {
    page::bench_header(ui, &[("Steering kept", "share against km/h")]);
    let (area, painter) = page::bench_area(ui);
    let (Ok(loss), Ok(cap), Ok(held), Ok(released)) = (f.num("HiSpeedSteerLoss"), f.num("MaxSpeed"), f.num("SteerInertia"), f.num("SteerInertiaNoInput")) else { return };
    let (kept_r, resp_r) = page::split(area, 0.6);
    plot::label(&painter, egui::pos2(resp_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, "RESPONSE share against seconds", theme::DIM);
    if cap <= 0.0 {
        return;
    }
    let x_max = page::speed_axis(cap);
    let kept = Plot::chart(kept_r, (0.0, x_max), (0.0, 1.125));
    plot::axes(&painter, &kept, &page::labelled(plot::ticks(20.0, x_max, 20.0)), &page::labelled(vec![0.0, 0.5, 1.0]));
    plot::dashed_up(&painter, &kept, cap, theme::BRASS_DK);
    plot::label(&painter, kept.at(cap, 1.125) + egui::vec2(-4.0, 4.0), Align2::RIGHT_TOP, format!("max speed {}", number(cap)), theme::BRASS);
    let end = drivingmath::tank_steer_factor(loss, cap, cap);
    plot::line(&painter, &kept, &[(0.0, 1.0), (cap, end), (x_max, end)], Stroke::new(2.0, theme::BRASS));
    plot::label(&painter, kept.at(cap, end) + egui::vec2(6.0, -3.0), Align2::LEFT_BOTTOM, format!("{end:.2}"), theme::BRASS);

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

/// The torque curve's rows: each ratio beside the share of the cap it sits at.
pub(crate) fn curve_card(ui: &mut egui::Ui, f: &Fields, fleet: &Fleet) {
    theme::card(ui, "Torque curve", None, |ui| {
        page::field_row(ui, fleet, f, "Ratio at rest", "EngineTorqueRatio0", "");
        page::field_row(ui, fleet, f, "Position 1", "EngineTorquePos1", "");
        page::field_row(ui, fleet, f, "Ratio at position 1", "EngineTorqueRatio1", "");
        page::field_row(ui, fleet, f, "Position 2", "EngineTorquePos2", "");
        page::field_row(ui, fleet, f, "Ratio at position 2", "EngineTorqueRatio2", "");
        page::field_row(ui, fleet, f, "Ratio at the cap", "EngineTorqueRatio3", "");
    });
}

pub(crate) fn cards(ui: &mut egui::Ui, p: &Panel, f: &Fields, wheels: &[FakeWheel], fleet: &Fleet) {
    match p.system {
        System::Chassis => {
            theme::card(ui, "Suspension", Some(&format!("{} tank templates", fleet.templates)), |ui| {
                page::ranged(ui, fleet, 128.0, "Ground probes", probes(p).len().to_string(), None, true);
                page::field_row(ui, fleet, f, "Spring strength", "SpringStrength", "");
                page::field_row(ui, fleet, f, "Spring damp", "SpringDamp", "");
                page::field_row(ui, fleet, f, "Spring exp length", "SpringExpLength", "");
                page::field_row(ui, fleet, f, "Spring cmp length", "SpringCmpLength", "");
            });
            theme::card(ui, "Grip", None, |ui| {
                page::field_row(ui, fleet, f, "Fwd friction", "FwdFriction", "");
                page::field_row(ui, fleet, f, "Side friction", "SideFriction", "");
                page::field_row(ui, fleet, f, "Brake", "Brake", "");
                page::field_row(ui, fleet, f, "Hi speed grip", "HiSpeedGrip", "");
            });
            theme::card(ui, "Body", None, |ui| {
                page::field_row(ui, fleet, f, "Mass", "Mass", "");
                page::field_row(ui, fleet, f, "Gravity factor", "GravityFactor", "");
                if let (Ok(x), Ok(y), Ok(z)) = (f.num("AngDampX"), f.num("AngDampY"), f.num("AngDampZ")) {
                    page::triple_row(ui, fleet, "Ang damp", [x, y, z]);
                }
                if let Ok(at) = f.vec3("CenterOfMassOffset") {
                    page::ranged(ui, fleet, 96.0, "Centre of mass", page::position(at), None, true);
                }
            });
            theme::card(ui, "Wheels", Some("BoneCtrlFakeWheel"), |ui| {
                let mut sets: Vec<(f32, f32, usize)> = Vec::new();
                for w in wheels {
                    match sets.iter_mut().find(|s| s.0 == w.radius && s.1 == w.travel) {
                        Some(s) => s.2 += 1,
                        None => sets.push((w.radius, w.travel, 1)),
                    }
                }
                for (radius, travel, count) in sets {
                    page::ranged(ui, fleet, 96.0, &format!("{count} wheels"), format!("radius {} · travel {}", number(radius), number(travel)), None, true);
                }
            });
        }
        System::Powertrain => {
            theme::card(ui, "Engine", Some(&format!("{} tank templates", fleet.templates)), |ui| {
                page::field_row(ui, fleet, f, "Engine torque", "EngineTorque", "");
                page::field_row(ui, fleet, f, "Max speed", "MaxSpeed", "km/h");
                page::field_row(ui, fleet, f, "Engine inertia", "EngineInertia", "");
                page::field_row(ui, fleet, f, "Engine offset Y", "EngineOffsetY", "");
            });
            curve_card(ui, f, fleet);
        }
        System::Steering => {
            theme::card(ui, "Steering", Some(&format!("{} tank templates", fleet.templates)), |ui| {
                page::field_row(ui, fleet, f, "Steer speed", "SteerSpeed", "");
                page::field_row(ui, fleet, f, "Inertia", "SteerInertia", "");
                page::field_row(ui, fleet, f, "No input inertia", "SteerInertiaNoInput", "");
                page::field_row(ui, fleet, f, "Hi speed steer loss", "HiSpeedSteerLoss", "");
                page::field_row(ui, fleet, f, "Yaw damp", "AngDampY", "");
            });
        }
        _ => {}
    }
}

/// The marks over the model: each fake wheel, the travel of the sprung ones, and the ground
/// probes' rays.
pub(crate) fn marks(m: &Marks, p: &Panel, f: &Fields, wheels: &[FakeWheel]) {
    if (m.on)(Overlay::Wheels) || (m.on)(Overlay::Travel) {
        for w in wheels {
            let Some(pos) = p.node(w.wheel) else { continue };
            let Some(c) = (m.at)(pos) else { continue };
            m.painter.circle_stroke(c, 6.0, Stroke::new(1.6, theme::BRASS));
            m.painter.circle_filled(c, 2.0, theme::BRASS);
            if !(m.on)(Overlay::Travel) || w.travel <= 0.0 {
                continue;
            }
            let Some(low) = (m.at)([pos[0], pos[1] - w.travel, pos[2]]) else { continue };
            m.painter.line_segment([c, low], Stroke::new(2.0, theme::GOOD));
            m.painter.line_segment([low + egui::vec2(-5.0, 0.0), low + egui::vec2(5.0, 0.0)], Stroke::new(2.0, theme::GOOD));
        }
    }
    if (m.on)(Overlay::Probes) {
        let (Ok(exp), Ok(cmp)) = (f.num("SpringExpLength"), f.num("SpringCmpLength")) else { return };
        for pos in probes(p) {
            let (Some(hi), Some(lo)) = ((m.at)([pos[0], pos[1] + cmp, pos[2]]), (m.at)([pos[0], pos[1] - exp, pos[2]])) else { continue };
            m.painter.line_segment([hi, lo], Stroke::new(2.0, theme::GOOD));
            m.painter.line_segment([lo + egui::vec2(-5.0, 0.0), lo + egui::vec2(5.0, 0.0)], Stroke::new(2.0, theme::GOOD));
            page::tag(m.painter, lo + egui::vec2(8.0, -6.0), number(((exp + cmp) * 1000.0).round() / 1000.0), theme::GOOD);
        }
    }
}
