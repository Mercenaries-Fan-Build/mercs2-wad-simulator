//! The Inspect window: everything one `.profile` contains, readable. The save's tables are shown
//! by topic through the same views the Saves inspector panel uses (`save_tables::TOPICS`), each
//! with its raw Lua behind a toggle; the file's fixed-layout header closes the window.

use std::path::{Path, PathBuf};

use egui::RichText;
use mercs2_formats::save::{self, Profile};
use mercs2_formats::save_lua::{self, Value};

use crate::gui::theme;
use crate::save_tables::{Ctx, TOPICS};
use crate::saves::Panel;

/// One save, decoded once when the window opens.
pub struct Inspected {
    pub path: PathBuf,
    bytes: Result<Vec<u8>, String>,
    profile: Result<Profile, String>,
    lua_len: Result<usize, String>,
    tree: Result<Value, String>,
}

impl Inspected {
    pub fn load(path: &Path) -> Inspected {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()));
        let profile = bytes.clone().and_then(|b| save::parse(&b));
        let lua = profile.as_ref().map_err(Clone::clone).and_then(|p| p.decompress_lua());
        let lua_len = lua.as_ref().map(Vec::len).map_err(Clone::clone);
        let tree = lua.and_then(|b| {
            let text = std::str::from_utf8(&b).map_err(|e| format!("Lua is not UTF-8: {e}"))?;
            save_lua::parse_chunk(text).map_err(|e| format!("parse Lua: {e}"))
        });
        Inspected { path: path.to_path_buf(), bytes, profile, lua_len, tree }
    }
}

/// Draw the window. Returns `false` once the user closes it.
pub fn show(ctx: &egui::Context, ins: &Inspected, p: &Panel) -> bool {
    let mut open = true;
    let stem = ins.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    egui::Window::new(format!("Inspect — {stem}"))
        .id(egui::Id::new("saves.inspect"))
        .open(&mut open)
        .default_size([720.0, 780.0])
        .vscroll(true)
        .show(ctx, |ui| body(ui, ins, p));
    open
}

fn err(ui: &mut egui::Ui, e: &str) {
    ui.label(RichText::new(e).small().color(theme::BAD));
}

fn body(ui: &mut egui::Ui, ins: &Inspected, p: &Panel) {
    crate::saves::lookup_errors(ui, p);
    match &ins.tree {
        Ok(Value::Table(root)) => {
            let ctx = Ctx { root, profile: ins.profile.as_ref().ok(), lk: p.lookups() };
            for (i, t) in TOPICS.iter().enumerate() {
                topic(ui, &ctx, t.title, (t.badge)(&ctx), t.keys, i == 0, |ui| (t.body)(ui, &ctx));
            }
            let other = crate::save_tables::unclaimed_keys(root);
            if !other.is_empty() {
                let keys: Vec<&str> = other.clone();
                topic(ui, &ctx, "OTHER", Some(format!("{} tables", keys.len())), &keys, false, |ui| {
                    crate::save_tables::other(ui, &ctx)
                });
            }
        }
        Ok(other) => err(ui, &format!("the save data is a {}, not a table", other.kind())),
        Err(e) => err(ui, e),
    }
    file(ui, ins);
}

/// One collapsible topic: title and badge, its view, then its raw Lua behind a toggle.
fn topic(
    ui: &mut egui::Ui,
    ctx: &Ctx,
    title: &str,
    badge: Option<String>,
    keys: &[&str],
    open: bool,
    view: impl FnOnce(&mut egui::Ui),
) {
    let mut job = egui::text::LayoutJob::default();
    job.append(title, 0.0, egui::TextFormat { font_id: theme::font(&theme::style_title()), color: theme::TX, ..Default::default() });
    if let Some(b) = &badge {
        job.append(
            &format!("   {b}"),
            0.0,
            egui::TextFormat { font_id: theme::font(&egui::TextStyle::Small), color: theme::FAINT, ..Default::default() },
        );
    }
    egui::CollapsingHeader::new(job)
        .id_source(("inspect.topic", title))
        .default_open(open)
        .show(ui, |ui| {
            view(ui);
            let present: Vec<(&str, &Value)> = keys.iter().filter_map(|k| ctx.root.get(k).map(|v| (*k, v))).collect();
            if present.is_empty() {
                return;
            }
            ui.add_space(6.0);
            let id = ui.make_persistent_id(("inspect.raw", title));
            let mut raw = ui.data_mut(|d| d.get_temp::<bool>(id).unwrap_or(false));
            if ui.checkbox(&mut raw, "Raw Lua").changed() {
                ui.data_mut(|d| d.insert_temp(id, raw));
            }
            if raw {
                let mut text = present
                    .iter()
                    .map(|(k, v)| format!("[\"{k}\"] = {}", save_lua::serialize(v)))
                    .collect::<Vec<_>>()
                    .join(",\n");
                egui::ScrollArea::vertical().max_height(320.0).id_source(("inspect.raw.scroll", title)).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
            }
        });
}

/// The file and its fixed-layout header: offset, field, value.
fn file(ui: &mut egui::Ui, ins: &Inspected) {
    egui::CollapsingHeader::new(RichText::new("FILE").text_style(theme::style_title()).color(theme::TX))
        .id_source("inspect.file")
        .default_open(false)
        .show(ui, |ui| {
            let mut rows: Vec<[String; 3]> = vec![["".into(), "Path".into(), ins.path.display().to_string()]];
            match &ins.bytes {
                Ok(b) => {
                    rows.push(["".into(), "Size".into(), format!("{} bytes", b.len())]);
                    rows.push(["".into(), "SHA-256".into(), crate::publish::sha256_hex(b)]);
                }
                Err(e) => err(ui, e),
            }
            match &ins.profile {
                Ok(p) => {
                    let check = if p.hash_ok() { "matches" } else { "does not match" };
                    for (off, field, value) in [
                        ("0x00", "Checksum", format!("0x{:08X} ({check})", p.checksum)),
                        ("0x04", "Version", p.version.to_string()),
                        ("0x08", "Data size", p.data_size.to_string()),
                        ("0x0C", "Unknown", p.unknown_0x0c.to_string()),
                        ("0x10", "Unknown", p.unknown_0x10.to_string()),
                        ("0x14", "Play time (s)", p.play_time_seconds.to_string()),
                        ("0x18", "Cash", p.cash.to_string()),
                        ("0x1C", "Fuel", p.fuel.to_string()),
                        ("0x20", "Unknown", p.unknown_0x20.to_string()),
                        ("0x24", "Timestamp", p.timestamp.to_string()),
                        ("0x2C", "Contract", p.active_contract.clone()),
                        ("0x4D", "Hero", p.character_index.to_string()),
                        ("0x4F", "Upgrade tier", p.upgrade_index.to_string()),
                        ("0x20A", "Slot name", p.save_name().to_string()),
                        ("0x24A", "Outfits unlocked", p.unlocked_costumes.to_string()),
                        ("0x24B", "Unknown", p.unknown_0x24b.to_string()),
                        ("0x2F8", "Fuel capacity", p.fuel_capacity.to_string()),
                        (
                            "0x468",
                            "Save data",
                            ins.lua_len.as_ref().map(|n| format!("{n} bytes of Lua (zlib)")).unwrap_or_else(|e| e.clone()),
                        ),
                    ] {
                        rows.push([off.into(), field.into(), value]);
                    }
                }
                Err(e) => err(ui, e),
            }
            egui::Grid::new("inspect.file.grid").striped(true).spacing([12.0, 3.0]).num_columns(3).show(ui, |ui| {
                for [off, field, value] in rows {
                    ui.label(RichText::new(off).monospace().color(theme::FAINT));
                    ui.label(RichText::new(field).color(theme::DIM));
                    ui.label(RichText::new(value).monospace().color(theme::TX));
                    ui.end_row();
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::game_paths;

    #[test]
    fn an_unreadable_file_reports_errors_instead_of_panicking() {
        let ins = Inspected::load(Path::new("/definitely/not/a/save.profile"));
        assert!(ins.bytes.is_err() && ins.profile.is_err() && ins.tree.is_err());
    }

    /// The whole window renders every fixture save without panicking.
    #[test]
    fn the_window_renders_every_fixture() {
        let ctx = egui::Context::default();
        crate::gui::theme::install(&ctx);
        let panel = Panel::default();
        for name in game_paths::SAVE_FIXTURES {
            let ins = Inspected::load(&game_paths::save_fixtures().join(name));
            assert!(ins.tree.is_ok(), "{name}");
            let _ = ctx.run(egui::RawInput::default(), |c| {
                assert!(show(c, &ins, &panel));
            });
        }
    }
}
