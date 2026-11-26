//! The Driving page's plane system: the bench, cards and status line of a template whose
//! `ObjectScript` names `supportairplane`. A plane has no physics record: Lua flies it with
//! `Airstrike.Flyby(template, spawn, target, altitude, speed)`, and the executable moves it along
//! a path of five control points (worker `0x62A440`, flight update `0x62C130`). Flyby draws that
//! path and the bank the plane holds in a turn; neither depends on the template.

use crate::drivingdata::{Fleet, Spinner};
use crate::drivingmath;
use crate::drivingpage::{self as page, Panel, System};
use crate::gui::theme;
use crate::plot::{self, number, Plot};
use egui::{Align2, Stroke};

pub(crate) fn status(p: &Panel, spinners: &[Spinner]) -> String {
    match p.system {
        System::Flyby => format!("flown by Airstrike.Flyby · {} m/s when no speed is given · {} turning nodes", number(drivingmath::FLYBY_DEFAULT_SPEED), spinners.len()),
        _ => page::coupling_status(p),
    }
}

pub(crate) fn bench(ui: &mut egui::Ui) {
    page::bench_header(ui, &[("Path", "height against distance, in runs from spawn to target")]);
    let (area, painter) = page::bench_area(ui);
    let (path_r, bank_r) = page::split(area, 0.62);
    plot::label(&painter, egui::pos2(bank_r.left(), area.top() - 19.0), Align2::LEFT_CENTER, "BANK rad against rad/s of yaw", theme::DIM);

    // The control points: level from the spawn through the middle of the run to the target, then
    // two exit points, each a run further on and a quarter of a run higher.
    let points = drivingmath::flyby_points();
    let path = Plot::chart(path_r, (0.0, 3.0), (0.0, 0.625));
    plot::axes(&painter, &path, &page::labelled(vec![1.0, 2.0, 3.0]), &page::labelled(vec![0.0, 0.25, 0.5]));
    plot::dashed_up(&painter, &path, 1.0, theme::BRASS_DK);
    plot::line(&painter, &path, &points, Stroke::new(2.0, theme::BRASS));
    for (x, y) in points {
        painter.circle_filled(path.at(x, y), 3.0, theme::BRASS);
    }
    plot::label(&painter, path.at(0.0, 0.0) + egui::vec2(6.0, -6.0), Align2::LEFT_BOTTOM, "spawn", theme::TX);
    plot::label(&painter, path.at(1.0, 0.0) + egui::vec2(6.0, -6.0), Align2::LEFT_BOTTOM, "target · callback", theme::BRASS);
    plot::label(&painter, path.at(3.0, 0.5) + egui::vec2(-12.0, -8.0), Align2::RIGHT_BOTTOM, "then straight on", theme::DIM);

    let limit = drivingmath::FLYBY_BANK_LIMIT;
    let x_max = 6.0;
    let bank = Plot::chart(bank_r, (0.0, x_max), (0.0, limit * 1.125));
    plot::axes(&painter, &bank, &page::labelled(plot::ticks(2.0, x_max, 2.0)), &page::labelled(vec![0.0, (limit * 50.0).round() / 100.0, (limit * 100.0).round() / 100.0]));
    let pts: Vec<(f32, f32)> = (0..=60).map(|i| x_max * i as f32 / 60.0).map(|w| (w, drivingmath::flyby_bank(w))).collect();
    plot::line(&painter, &bank, &pts, Stroke::new(2.0, theme::BRASS));
    plot::label(&painter, bank.at(x_max, limit) + egui::vec2(-4.0, 12.0), Align2::RIGHT_TOP, format!("{:.2} rad · {:.0}°", limit, limit.to_degrees()), theme::BRASS);
}

pub(crate) fn cards(ui: &mut egui::Ui, p: &Panel, spinners: &[Spinner], fleet: &Fleet) {
    if p.system != System::Flyby {
        return;
    }
    theme::card(ui, "Flyby", Some("Airstrike.Flyby"), |ui| {
        page::ranged(ui, fleet, 128.0, "Script", crate::drivingdata::FLYBY_SCRIPT.to_string(), None, true);
        page::ranged(ui, fleet, 128.0, "Speed when not given", format!("{} m/s", number(drivingmath::FLYBY_DEFAULT_SPEED)), None, true);
        page::ranged(ui, fleet, 128.0, "Climb per exit span", format!("{} of the run", number(drivingmath::FLYBY_EXIT_CLIMB)), None, true);
        page::ranged(ui, fleet, 128.0, "Bank per rad/s of yaw", format!("{} rad", number(drivingmath::FLYBY_BANK_GAIN)), None, true);
        page::ranged(ui, fleet, 128.0, "Bank limit", format!("{:.2} rad", drivingmath::FLYBY_BANK_LIMIT), None, true);
    });
    if !spinners.is_empty() {
        theme::card(ui, "Turning nodes", Some("BoneCtrlLocalRotation"), |ui| {
            for s in spinners {
                page::ranged(ui, fleet, 128.0, &p.node_label(s.bone), format!("speed {} · axis {}", number(s.speed), page::position(s.axis)), None, true);
            }
        });
    }
}
