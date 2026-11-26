//! The Driving page's helicopter systems: the benches, cards, viewport marks and status lines of a
//! template with a `_HelicopterPhysics` record. Flight draws how fast the forward speed closes on
//! its target and the three speed limits, Yaw the yaw rate and the bank at each speed, Attitude the
//! pitch and roll the helicopter leans to.
//!
//! A helicopter template carries two records with the same fields: `_HelicopterPhysics`, flown by
//! a player, and `_HelicopterPhysicsAi`, flown by an AI pilot (`FUN_00439850`). Every chart and
//! card shows both.

use crate::drivingdata::{Fields, Fleet, Rotor, Winch};
use crate::drivingmath::{self, HeliTurn};
use crate::drivingpage::{self as page, Marks, Overlay, Panel, System};
use crate::gui::theme;
use crate::plot::{self, number, Plot};
use egui::{Align2, Color32, Stroke};

fn turn(f: &Fields) -> Result<HeliTurn, String> {
    Ok(HeliTurn {
        max_yaw_speed: f.num("MaxYawSpeed")?,
        low_speed_yaw_speed: f.num("LowSpeedYawSpeed")?,
        low_speed_limit: f.num("LowSpeedLimit")?,
        hi_speed_yaw_loss: f.num("HiSpeedYawLoss")?,
        max_fwd_speed: f.num("MaxFwdSpeed")?,
        bank_speed_limit: f.num("BankSpeedLimit")?,
        max_roll_bank: f.num("MaxRollBank")?,
    })
}

/// The chart's top speed: the next multiple of 10 above the faster record's `MaxFwdSpeed`.
fn speed_axis(player: &Fields, ai: Option<&Fields>) -> Option<f32> {
    let top = [Some(player), ai].into_iter().flatten().filter_map(|f| f.num("MaxFwdSpeed").ok()).fold(0.0_f32, f32::max);
    (top > 0.0).then(|| ((top / 10.0).floor() + 1.0) * 10.0)
}

/// The two records with the colour and line width each is drawn in: the player's, then the AI's.
fn pilots<'a>(player: &'a Fields, ai: Option<&'a Fields>) -> Vec<(&'a Fields, Color32, f32, &'static str)> {
    let mut out = vec![(player, theme::BRASS, 2.0, "player")];
    if let Some(ai) = ai {
        out.insert(0, (ai, theme::TX, 1.6, "AI"));
    }
    out
}

/// The key of the two pilots' lines, beside a bench's title.
fn pilot_key(painter: &egui::Painter, area: egui::Rect, from: f32, ai: bool) {
    let mut x = area.left() + from;
    for (name, col) in [("player", theme::BRASS), ("AI", theme::TX)].into_iter().take(if ai { 2 } else { 1 }) {
        painter.line_segment([egui::pos2(x, area.top() - 19.0), egui::pos2(x + 14.0, area.top() - 19.0)], Stroke::new(2.0, col));
        let galley = painter.layout_no_wrap(name.to_string(), theme::font(&egui::TextStyle::Small), theme::DIM);
        let w = galley.size().x;
        painter.galley(egui::pos2(x + 19.0, area.top() - 19.0 - galley.size().y / 2.0), galley, theme::DIM);
        x += w + 33.0;
    }
}

pub(crate) fn status(p: &Panel, player: &Fields, _ai: Option<&Fields>, rotors: &[Rotor], winch: Option<&Winch>) -> Option<String> {
    let f = player;
    Some(match p.system {
        System::Flight => format!(
            "forward {} m/s · sideways {} · climb {} · {} rotors",
            number(f.num("MaxFwdSpeed").ok()?),
            number(f.num("MaxRightSpeed").ok()?),
            number(f.num("MaxElevationSpeed").ok()?),
            rotors.len()
        ),
        System::Yaw => {
            let t = turn(f).ok()?;
            format!("yaw {:.2} rad/s at rest · {:.2} at {} m/s", drivingmath::heli_yaw_rate(&t, 0.0), drivingmath::heli_yaw_rate(&t, t.max_fwd_speed), number(t.max_fwd_speed))
        }
        System::Attitude => {
            let (pitch, fwd) = (f.num("MaxPitch").ok()?, f.num("MaxFwdSpeed").ok()?);
            format!("pitch {:.2} rad at {} m/s · roll {} rad on the side stick", drivingmath::heli_pitch(pitch, fwd, 1.0, fwd), number(fwd), number(f.num("MaxRollSide").ok()?))
        }
        _ => match winch {
            Some(w) => format!("winch at {} · reach {}", p.node_label(w.bone), number(w.length)),
            None => page::coupling_status(p),
        },
    })
}

pub(crate) fn bench(ui: &mut egui::Ui, p: &Panel, player: &Fields, ai: Option<&Fields>, _rotors: &[Rotor]) {
    match p.system {
        System::Flight => flight_bench(ui, player, ai),
        System::Yaw => yaw_bench(ui, player, ai),
        System::Attitude => attitude_bench(ui, player, ai),
        _ => {}
    }
}

fn flight_bench(ui: &mut egui::Ui, player: &Fields, ai: Option<&Fields>) {
    page::bench_header(ui, &[("Forward speed", "share of target against seconds")]);
    let (area, painter) = page::bench_area(ui);
    let (reach_r, limits_r) = page::split(area, 0.6);
    plot::label(&painter, egui::pos2(limits_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, "SPEED LIMITS m/s", theme::DIM);
    pilot_key(&painter, area, 300.0, ai.is_some());

    // Stick held: the speed closes on MaxFwdSpeed with FwdSpeedAcc. Stick let go: it falls away
    // with the gain FwdSpeedNoInputInertia leaves.
    let seconds = 8.0;
    let times: Vec<f32> = (0..=80).map(|i| seconds * i as f32 / 80.0).collect();
    let reach = Plot::chart(reach_r, (0.0, seconds), (0.0, 1.0));
    plot::axes(&painter, &reach, &page::labelled(plot::ticks(2.0, seconds, 2.0)), &page::labelled(vec![0.0, 0.5, 1.0]));
    for (f, col, width, _) in pilots(player, ai) {
        let (Ok(acc), Ok(keep)) = (f.num("FwdSpeedAcc"), f.num("FwdSpeedNoInputInertia")) else { continue };
        let held: Vec<(f32, f32)> = times.iter().map(|&t| (t, drivingmath::servo_reached(acc, t))).collect();
        let released: Vec<(f32, f32)> = times.iter().map(|&t| (t, 1.0 - drivingmath::servo_reached(acc * (1.0 - keep), t))).collect();
        plot::line(&painter, &reach, &released, Stroke::new(1.0, col));
        plot::line(&painter, &reach, &held, Stroke::new(width, col));
    }
    plot::label(&painter, reach.at(seconds, 1.0) + egui::vec2(-4.0, 16.0), Align2::RIGHT_TOP, "stick held", theme::DIM);
    plot::label(&painter, reach.at(seconds, 0.0) + egui::vec2(-4.0, -30.0), Align2::RIGHT_BOTTOM, "stick let go", theme::DIM);

    // The three speed limits, a bar each for the player and the AI.
    let rows = [("forward", "MaxFwdSpeed"), ("sideways", "MaxRightSpeed"), ("climb", "MaxElevationSpeed")];
    let top = pilots(player, ai).iter().flat_map(|(f, ..)| rows.iter().filter_map(|(_, name)| f.num(name).ok())).fold(0.0_f32, f32::max);
    if top <= 0.0 {
        return;
    }
    let (label_w, bar_h) = (64.0, 9.0);
    let bars = egui::Rect::from_min_max(limits_r.min + egui::vec2(label_w, 10.0), limits_r.max - egui::vec2(46.0, 0.0));
    let row_h = (bars.height() / rows.len() as f32).min(56.0);
    for (i, (label, name)) in rows.iter().enumerate() {
        let y = bars.top() + row_h * i as f32;
        plot::label(&painter, egui::pos2(limits_r.left(), y + bar_h), Align2::LEFT_CENTER, label, theme::DIM);
        for (j, (f, col, ..)) in pilots(player, ai).into_iter().rev().enumerate() {
            let Ok(v) = f.num(name) else { continue };
            let bar = egui::Rect::from_min_size(egui::pos2(bars.left(), y + (bar_h + 4.0) * j as f32), egui::vec2(bars.width() * v / top, bar_h));
            painter.rect_filled(bar, egui::CornerRadius::same(2), col);
            plot::label(&painter, bar.right_center() + egui::vec2(6.0, 0.0), Align2::LEFT_CENTER, number(v), col);
        }
    }
}

fn yaw_bench(ui: &mut egui::Ui, player: &Fields, ai: Option<&Fields>) {
    page::bench_header(ui, &[("Yaw rate", "rad/s against m/s")]);
    let (area, painter) = page::bench_area(ui);
    let (yaw_r, bank_r) = page::split(area, 0.6);
    plot::label(&painter, egui::pos2(bank_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, "BANK rad against m/s", theme::DIM);
    pilot_key(&painter, area, 210.0, ai.is_some());
    let Some(x_max) = speed_axis(player, ai) else { return };
    let speeds: Vec<f32> = (0..=200).map(|i| x_max * i as f32 / 200.0).collect();
    let x_ticks = page::labelled(plot::ticks(10.0, x_max, 10.0));
    let turns: Vec<(HeliTurn, f32, Color32, f32)> =
        pilots(player, ai).into_iter().filter_map(|(f, col, width, _)| Some((turn(f).ok()?, f.num("BankYawAmplification").ok()?, col, width))).collect();

    // The rate a full yaw input asks for, and under it what a full side input adds through the bank.
    let with_bank = |t: &HeliTurn, amp: f32, s: f32| drivingmath::heli_yaw_rate(t, s) + amp * drivingmath::heli_bank(t, s);
    let top = turns.iter().flat_map(|(t, amp, ..)| speeds.iter().map(move |&s| with_bank(t, *amp, s))).fold(0.0_f32, f32::max);
    if top <= 0.0 {
        return;
    }
    let step = if top > 6.0 { 2.0 } else { 1.0 };
    let yaw = Plot::chart(yaw_r, (0.0, x_max), (0.0, top * 1.125));
    plot::axes(&painter, &yaw, &x_ticks, &page::labelled(plot::ticks(0.0, top * 1.125, step)));
    for (t, amp, col, width) in &turns {
        let banked: Vec<(f32, f32)> = speeds.iter().map(|&s| (s, with_bank(t, *amp, s))).collect();
        let plain: Vec<(f32, f32)> = speeds.iter().map(|&s| (s, drivingmath::heli_yaw_rate(t, s))).collect();
        plot::line(&painter, &yaw, &banked, Stroke::new(1.0, *col));
        plot::line(&painter, &yaw, &plain, Stroke::new(*width, *col));
    }
    if let Some((t, amp, col, _)) = turns.last() {
        plot::dashed_up(&painter, &yaw, t.max_fwd_speed, theme::BRASS_DK);
        plot::label(&painter, yaw.at(t.max_fwd_speed, top * 1.125) + egui::vec2(-4.0, 4.0), Align2::RIGHT_TOP, format!("max fwd speed {}", number(t.max_fwd_speed)), theme::BRASS);
        if t.max_roll_bank > 0.0 && *amp > 0.0 {
            let s = t.max_fwd_speed;
            plot::label(&painter, yaw.at(s, with_bank(t, *amp, s)) + egui::vec2(-6.0, -3.0), Align2::RIGHT_BOTTOM, "with the side stick", *col);
        }
    }

    let top = turns.iter().map(|(t, ..)| t.max_roll_bank).fold(0.0_f32, f32::max);
    if top <= 0.0 {
        return;
    }
    let bank = Plot::chart(bank_r, (0.0, x_max), (0.0, top * 1.125));
    plot::axes(&painter, &bank, &x_ticks, &page::labelled(vec![0.0, top / 2.0, top]));
    for (t, _, col, width) in &turns {
        let pts: Vec<(f32, f32)> = speeds.iter().map(|&s| (s, drivingmath::heli_bank(t, s))).collect();
        plot::line(&painter, &bank, &pts, Stroke::new(*width, *col));
    }
}

fn attitude_bench(ui: &mut egui::Ui, player: &Fields, ai: Option<&Fields>) {
    page::bench_header(ui, &[("Pitch", "rad against m/s")]);
    let (area, painter) = page::bench_area(ui);
    let (pitch_r, roll_r) = page::split(area, 0.5);
    plot::label(&painter, egui::pos2(roll_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, "ROLL rad against m/s", theme::DIM);
    pilot_key(&painter, area, 170.0, ai.is_some());
    let Some(x_max) = speed_axis(player, ai) else { return };
    let speeds: Vec<f32> = (0..=100).map(|i| x_max * i as f32 / 100.0).collect();
    let x_ticks = page::labelled(plot::ticks(10.0, x_max, 10.0));
    let rows = pilots(player, ai);

    // Pitch: the forward stick held, and the stick let go, at each speed.
    let pitch_of = |f: &Fields, stick: f32, s: f32| Some(drivingmath::heli_pitch(f.num("MaxPitch").ok()?, f.num("MaxFwdSpeed").ok()?, stick, s));
    let top = rows.iter().filter_map(|(f, ..)| pitch_of(f, 1.0, x_max)).fold(0.0_f32, f32::max);
    if top > 0.0 {
        let pitch = Plot::chart(pitch_r, (0.0, x_max), (0.0, top * 1.125));
        plot::axes(&painter, &pitch, &x_ticks, &page::labelled(vec![0.0, top / 2.0, top]));
        for (f, col, width, _) in &rows {
            for (stick, w) in [(0.0, 1.0), (1.0, *width)] {
                let pts: Vec<(f32, f32)> = speeds.iter().filter_map(|&s| Some((s, pitch_of(f, stick, s)?))).collect();
                plot::line(&painter, &pitch, &pts, Stroke::new(w, *col));
            }
        }
        for (stick, name) in [(1.0, "stick held"), (0.0, "stick let go")] {
            if let Some(y) = pitch_of(player, stick, x_max) {
                plot::label(&painter, pitch.at(x_max, y) + egui::vec2(-4.0, 5.0), Align2::RIGHT_TOP, name, theme::DIM);
            }
        }
    }

    // Roll: the side stick, which adds the bank past BankSpeedLimit, and the yaw stick, whose roll
    // grows from a quarter of MaxRollFwd at rest to the whole at MaxFwdSpeed.
    let side_of = |f: &Fields, s: f32| Some(f.num("MaxRollSide").ok()? + drivingmath::heli_bank(&turn(f).ok()?, s));
    let yaw_of = |f: &Fields, s: f32| {
        let fwd = f.num("MaxFwdSpeed").ok()?;
        let share = if fwd > 0.0 { (s / fwd).clamp(0.0, 1.0) } else { 0.0 };
        Some((0.25 + 0.75 * share) * f.num("MaxRollFwd").ok()?)
    };
    let top = rows.iter().flat_map(|(f, ..)| [side_of(f, x_max), yaw_of(f, x_max)]).flatten().fold(0.0_f32, f32::max);
    if top <= 0.0 {
        return;
    }
    let roll = Plot::chart(roll_r, (0.0, x_max), (0.0, top * 1.125));
    plot::axes(&painter, &roll, &x_ticks, &page::labelled(vec![0.0, top / 2.0, top]));
    for (f, col, width, _) in &rows {
        let side: Vec<(f32, f32)> = speeds.iter().filter_map(|&s| Some((s, side_of(f, s)?))).collect();
        let yaw: Vec<(f32, f32)> = speeds.iter().filter_map(|&s| Some((s, yaw_of(f, s)?))).collect();
        plot::line(&painter, &roll, &yaw, Stroke::new(1.0, *col));
        plot::line(&painter, &roll, &side, Stroke::new(*width, *col));
    }
    if let (Some(a), Some(b)) = (side_of(player, x_max), yaw_of(player, x_max)) {
        plot::label(&painter, roll.at(x_max, a) + egui::vec2(-4.0, 5.0), Align2::RIGHT_TOP, "side stick", theme::DIM);
        plot::label(&painter, roll.at(x_max, b) + egui::vec2(-4.0, 5.0), Align2::RIGHT_TOP, "yaw stick", theme::DIM);
    }
}

/// A card of fields the two records share, the player's value beside the AI's.
fn pilot_card(ui: &mut egui::Ui, title: &str, badge: Option<&str>, player: &Fields, ai: Option<&Fields>, fleet: &Fleet, ai_fleet: Option<&Fleet>, rows: &[(&str, &str)]) {
    theme::card(ui, title, badge, |ui| {
        let side = |f: Option<&Fields>, fleet: Option<&Fleet>, name: &str| {
            let v = f?.num(name).ok()?;
            Some((v, fleet.and_then(|fleet| fleet.place(name, v))))
        };
        let rows: Vec<(&str, [Option<(f32, Option<f32>)>; 2])> =
            rows.iter().map(|(label, name)| (*label, [side(Some(player), Some(fleet), name), side(ai, ai_fleet, name)])).collect();
        page::paired(ui, ["PLAYER", "AI"], &rows);
    });
}

pub(crate) fn cards(ui: &mut egui::Ui, p: &Panel, player: &Fields, ai: Option<&Fields>, rotors: &[Rotor], fleet: &Fleet, ai_fleet: Option<&Fleet>) {
    let templates = format!("{} helicopter templates", fleet.templates);
    match p.system {
        System::Flight => {
            pilot_card(
                ui,
                "Speed",
                Some(&templates),
                player,
                ai,
                fleet,
                ai_fleet,
                &[
                    ("Max fwd speed", "MaxFwdSpeed"),
                    ("Fwd speed acc", "FwdSpeedAcc"),
                    ("No input inertia", "FwdSpeedNoInputInertia"),
                    ("Max right speed", "MaxRightSpeed"),
                    ("Right speed acc", "RightSpeedAcc"),
                    ("Max elev. speed", "MaxElevationSpeed"),
                ],
            );
            theme::card(ui, "Body", None, |ui| {
                page::field_row(ui, fleet, player, "Mass", "Mass", "");
                let kick = |f: Option<&Fields>| f.and_then(|f| f.word("KickBack").ok()).map(|v| if v != 0 { "True" } else { "False" });
                let text = match (kick(Some(player)), kick(ai)) {
                    (Some(a), Some(b)) => format!("{a} · AI {b}"),
                    (Some(a), None) => a.to_string(),
                    _ => return,
                };
                page::ranged(ui, fleet, 128.0, "Kick back", text, None, true);
            });
            theme::card(ui, "Rotors", Some(&rotors.len().to_string()), |ui| {
                for r in rotors {
                    page::ranged(ui, fleet, 96.0, "Hub", p.node_label(r.hub), None, true);
                    page::ranged(ui, fleet, 96.0, "Spin dir", number(r.spin_dir), None, true);
                    page::ranged(ui, fleet, 96.0, "Blur on", p.node_label(r.blur_on), None, true);
                    page::ranged(ui, fleet, 96.0, "Blur off", p.node_label(r.blur_off), None, true);
                }
            });
        }
        System::Yaw => pilot_card(
            ui,
            "Yaw",
            Some(&templates),
            player,
            ai,
            fleet,
            ai_fleet,
            &[
                ("Max yaw speed", "MaxYawSpeed"),
                ("Low speed yaw", "LowSpeedYawSpeed"),
                ("Low speed limit", "LowSpeedLimit"),
                ("Hi speed yaw loss", "HiSpeedYawLoss"),
                ("Yaw acc", "YawAcc"),
                ("Yaw damp", "YawDamp"),
                ("Bank yaw amp.", "BankYawAmplification"),
                ("Bank speed limit", "BankSpeedLimit"),
                ("Max roll bank", "MaxRollBank"),
            ],
        ),
        System::Attitude => pilot_card(
            ui,
            "Attitude",
            Some(&templates),
            player,
            ai,
            fleet,
            ai_fleet,
            &[
                ("Max pitch", "MaxPitch"),
                ("Pitch accel", "PitchAccel"),
                ("Max roll side", "MaxRollSide"),
                ("Max roll fwd", "MaxRollFwd"),
                ("Roll accel", "RollAccel"),
                ("Bank speed limit", "BankSpeedLimit"),
                ("Max roll bank", "MaxRollBank"),
            ],
        ),
        _ => {}
    }
}

/// The marks over the model: each rotor's hub with the way it spins, and how far the winch
/// reaches below its bone.
pub(crate) fn marks(m: &Marks, p: &Panel, rotors: &[Rotor], winch: Option<&Winch>) {
    if (m.on)(Overlay::Rotors) {
        for r in rotors {
            let Some(c) = p.node(r.hub).and_then(|pos| (m.at)(pos)) else { continue };
            m.painter.circle_stroke(c, 8.0, Stroke::new(1.6, theme::BRASS));
            m.painter.circle_filled(c, 2.0, theme::BRASS);
            page::tag(m.painter, c + egui::vec2(12.0, -7.0), format!("{} · spin {}", p.node_label(r.hub), number(r.spin_dir)), theme::BRASS);
        }
    }
    if (m.on)(Overlay::Winch) {
        let Some(winch) = winch else { return };
        let Some(pos) = p.node(winch.bone) else { return };
        let (Some(top), Some(low)) = ((m.at)(pos), (m.at)([pos[0], pos[1] - winch.length, pos[2]])) else { return };
        m.painter.line_segment([top, low], Stroke::new(2.0, theme::GOOD));
        m.painter.line_segment([low + egui::vec2(-5.0, 0.0), low + egui::vec2(5.0, 0.0)], Stroke::new(2.0, theme::GOOD));
        page::tag(m.painter, low + egui::vec2(8.0, -6.0), number(winch.length), theme::GOOD);
    }
}
