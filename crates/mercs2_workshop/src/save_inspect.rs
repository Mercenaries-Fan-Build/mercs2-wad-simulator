//! The Inspect window: a readable account of everything one `.profile` contains — the file, the
//! fixed-layout header field by field, then every Lua table the game wrote, each with what it is,
//! which game script writes it, its contents in readable form, and its raw Lua on request.
//!
//! Confidence follows the sources: header labels are `save.rs`'s FACT / INFERRED / unknown;
//! table descriptions come from the game's `GenerateSaveData` (`vz/xQ!L.lua:529-566`) and the
//! writers it calls. A table the game does not list there is shown as unknown.

use std::path::{Path, PathBuf};

use egui::RichText;
use mercs2_formats::save::{self, Profile};
use mercs2_formats::save_lua::{self, Value};

use crate::gui::theme;
use crate::save_tables::{key_text, tree, value_text, Lookups};

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
pub fn show(ctx: &egui::Context, ins: &Inspected, lk: Lookups) -> bool {
    let mut open = true;
    let stem = ins.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    egui::Window::new(format!("Inspect — {stem}"))
        .id(egui::Id::new("saves.inspect"))
        .open(&mut open)
        .default_size([680.0, 760.0])
        .vscroll(true)
        .show(ctx, |ui| body(ui, ins, lk));
    open
}

fn heading(ui: &mut egui::Ui, title: &str, note: &str) {
    ui.add_space(10.0);
    ui.label(theme::disp_text(title, theme::style_title(), theme::TX));
    if !note.is_empty() {
        ui.label(RichText::new(note).small().color(theme::FAINT));
    }
    ui.add_space(4.0);
}

fn err(ui: &mut egui::Ui, e: &str) {
    ui.label(RichText::new(e).small().color(theme::BAD));
}

fn body(ui: &mut egui::Ui, ins: &Inspected, lk: Lookups) {
    // ── the file ──
    heading(ui, "FILE", "");
    theme::kv(ui, "path", RichText::new(ins.path.display().to_string()));
    match &ins.bytes {
        Ok(b) => {
            theme::kv(ui, "size", RichText::new(format!("{} bytes", b.len())));
            theme::kv(ui, "sha256", RichText::new(crate::publish::sha256_hex(b)));
        }
        Err(e) => err(ui, e),
    }

    // ── the header ──
    heading(
        ui,
        "HEADER",
        "Fixed-layout fields at the start of the file. Offset, value, and how sure we are of the meaning.",
    );
    match &ins.profile {
        Ok(p) => header(ui, p, ins.lua_len.as_ref().ok().copied()),
        Err(e) => err(ui, e),
    }

    // ── the Lua payload ──
    heading(
        ui,
        "SAVE DATA",
        "A zlib-compressed Lua table at 0x468 that the game builds in GenerateSaveData \
         (vz/xQ!L.lua). One entry per table, in the order the save stores them.",
    );
    match &ins.tree {
        Ok(Value::Table(root)) => {
            for e in root.iter_entries() {
                let key = key_text(e);
                ui.push_id(&key, |ui| table_card(ui, &key, &e.value, lk));
            }
        }
        Ok(other) => err(ui, &format!("the payload is a {}, not a table", other.kind())),
        Err(e) => err(ui, e),
    }
}

/// The fixed header, field by field, with `save.rs`'s confidence for each.
fn header(ui: &mut egui::Ui, p: &Profile, lua_len: Option<usize>) {
    let check = if p.hash_ok() { "matches the file" } else { "DOES NOT match the file" };
    let rows: Vec<(&str, &str, String, String)> = vec![
        ("0x00", "checksum", format!("0x{:08X}", p.checksum), format!("CRC-32/BZIP2 over bytes 4..end — {check}")),
        ("0x04", "version", p.version.to_string(), "save-format version; 4 in retail".into()),
        ("0x08", "data size", p.data_size.to_string(), "bytes the checksum covers (file length − 4)".into()),
        ("0x0C", "unknown", p.unknown_0x0c.to_string(), "3 in every save seen; meaning unknown".into()),
        ("0x10", "unknown", p.unknown_0x10.to_string(), "0 in every save seen; meaning unknown".into()),
        ("0x14", "play time", hms(p.play_time_seconds), "seconds played (inferred)".into()),
        ("0x18", "cash", crate::save_tables::fmt_num(p.cash as f64), "PMC cash (inferred)".into()),
        ("0x1C", "fuel", p.fuel.to_string(), "PMC fuel (inferred)".into()),
        ("0x20", "unknown", p.unknown_0x20.to_string(), "0 in every save seen; meaning unknown".into()),
        ("0x24", "timestamp", p.timestamp.to_string(), "Unix time the save was written".into()),
        ("0x2C", "contract", p.active_contract.clone(), "active or last contract id".into()),
        ("0x4D", "hero", p.character_index.to_string(), "1 Mattias · 2 Chris · 3 Jennifer".into()),
        ("0x4F", "upgrade tier", p.upgrade_index.to_string(), "0…3; picks the hero's spawn template".into()),
        ("0x20A", "slot name", p.save_name().to_string(), "save-slot label (UTF-16), not the player's name".into()),
        ("0x24A", "unlocked costumes", p.unlocked_costumes.to_string(), "how many of the hero's outfits are unlocked".into()),
        ("0x24B", "unknown", p.unknown_0x24b.to_string(), "1 in every save seen; meaning unknown".into()),
        ("0x2F8", "fuel capacity", p.fuel_capacity.to_string(), "maximum fuel (inferred)".into()),
        (
            "0x468",
            "save data",
            lua_len.map(|n| format!("{n} bytes of Lua")).unwrap_or_else(|| "unreadable".into()),
            "zlib stream; decoded below".into(),
        ),
    ];
    egui::Grid::new("inspect.header").striped(true).spacing([10.0, 3.0]).num_columns(4).show(ui, |ui| {
        for (off, field, value, note) in rows {
            ui.label(RichText::new(off).monospace().color(theme::FAINT));
            ui.label(RichText::new(field).color(theme::DIM));
            ui.label(RichText::new(value).monospace().color(theme::TX));
            ui.label(RichText::new(note).small().color(theme::FAINT));
            ui.end_row();
        }
    });
}

/// What a top-level save table is and which game script writes it.
struct About {
    key: &'static str,
    title: &'static str,
    writer: &'static str,
    what: &'static str,
}

/// Every key `GenerateSaveData` (`vz/xQ!L.lua:529-566`) writes, with its writer. The `what` lines
/// say only what the writer's code or the saved data shows.
const ABOUT: &[About] = &[
    About { key: "sLastMissionName", title: "Last completed contract", writer: "WifMissionFlow.GetLastCompletedContractName()", what: "The id of the last contract completed." },
    About { key: "tPmcData", title: "PMC resources", writer: "MrxPmc.SaveSingleton", what: "PMC cash and fuel, owned fuel tanks, the vehicle/support stockpile (amount and a 'new' flag per item), and freebies." },
    About { key: "tPmcInteriorData", title: "PMC headquarters", writer: "WifPmcInterior.SaveSingleton", what: "Whether the HQ is unlocked, which recruits have been introduced, and the HQ stockpile quantities (including money)." },
    About { key: "tTransitData", title: "Fast travel", writer: "MrxTransit.SaveSingleton", what: "Whether fast travel is on, and each landing zone's owning faction and enabled / nuked / fanfare-played flags." },
    About { key: "tSupportData", title: "Support & vehicle unlocks", writer: "MrxSupportData.SaveSingleton", what: "Per support or vehicle id: its unlock status by faction (and global stock when set), plus the support requirements obtained." },
    About { key: "tRewardData", title: "Contract rewards", writer: "MrxRewardData.SaveSingleton", what: "Per contract: reward data, such as the wager placed." },
    About { key: "tFactionData", title: "Faction relations", writer: "MrxFactionManager.SaveSingleton", what: "Each faction's standing toward each other faction (−100 … 100), civilian casualties and penalty, and the factions whose standing can change." },
    About { key: "tLayerData", title: "World state", writer: "MrxLayerManager.SaveSingleton", what: "The vz_state_* world overlays currently loaded, plus those queued to load. Story bindings add and remove them as the story advances." },
    About { key: "tStarterData", title: "Contacts", writer: "MrxStarterManager.SaveSingleton", what: "The contacts unlocked so far, each with card-shown / fanfare-played flags and the briefings already given." },
    About { key: "tFlowData", title: "Story progress", writer: "WifMissionFlow.SaveSingleton", what: "tMyFlowData: the flow keys awarded. tActiveMissions: missions in progress. tCulledBindings: the one-shot story bindings that have already fired." },
    About { key: "tPlayerData", title: "Player", writer: "MrxPlayer.SaveSingleton", what: "Per player: health, and the carried weapons as template GUID + reserve ammo." },
    About { key: "tBoundaryData", title: "Play area boundary", writer: "WifVzBoundary.SaveSingleton", what: "The active world boundary." },
    About { key: "tActiveHint", title: "Queued hints", writer: "WifHints.SaveSingleton", what: "The hints queued for each character." },
    About { key: "tActiveBio", title: "Bios unlocked", writer: "WifBios.SaveSingleton", what: "The character bios unlocked in the PDA." },
    About { key: "tMunitionsData", title: "Munitions hints", writer: "Munitions.SaveSingleton", what: "Which cash / fuel / support pickup hints have played, and whether munitions can be tagged." },
    About { key: "nTimeElapsed", title: "Time elapsed", writer: "MrxPlayState.GetTotalTimeElapsed()", what: "Total time played, in seconds." },
    About { key: "bGrappleEnabled", title: "Grapple enabled", writer: "WifMissionFlow.IsGrappleEnabled()", what: "Gameplay switch." },
    About { key: "bVehicleDisguiseEnabled", title: "Vehicle disguise enabled", writer: "WifMissionFlow.IsVehicleDisguiseEnabled()", what: "Gameplay switch." },
    About { key: "bResourceCountersEnabled", title: "Resource counters enabled", writer: "WifMissionFlow.AreResourceCountersEnabled()", what: "Gameplay switch." },
    About { key: "tTutorialData", title: "Tutorials seen", writer: "MrxTutorialManager.SaveSingleton", what: "The tutorials already shown." },
    About { key: "nAvailableCostumes", title: "Available costumes", writer: "WifPmcInterior.GetAvailableCostumes()", what: "How many outfits are available at the HQ mirror." },
    About { key: "tStatsData", title: "Statistics", writer: "MrxStatsManager.SaveSingleton", what: "PDA statistics: toolboxes, credits and debits, favourite weapons and vehicles, race times, deaths, retries, destroy bounties, and oVerifyData (each high-value target's fate)." },
    About { key: "tShopData", title: "Shops", writer: "MrxShop.SaveSingleton", what: "Per faction shop: what was bought and the shop's total items." },
    About { key: "tWifEquipmentData", title: "Equipment unlocks", writer: "WifEquipmentData.SaveSingleton", what: "Per equipment id: its unlock status by faction." },
    About { key: "vEquippedSupport", title: "Equipped support", writer: "Pda.Support:ReadEquippedSupport", what: "The PDA support slots, as string-table keys." },
    About { key: "tMrxAchievementsData", title: "Achievement tracking", writer: "MrxAchievements.SaveSingleton", what: "Support used, solo and co-op, for achievements." },
    About { key: "tLockedGates", title: "Locked gates", writer: "friendlygate.SaveSingleton", what: "Gates currently locked." },
    About { key: "tRetryLocations", title: "Retry checkpoint", writer: "WifMissionFlow.GetRetryLocations()", what: "Written only when the save is taken mid-contract: where a retry respawns." },
];

fn about(key: &str) -> Option<&'static About> {
    ABOUT.iter().find(|a| a.key == key)
}

/// One top-level table: title, what it is and who writes it, contents, raw Lua on request.
fn table_card(ui: &mut egui::Ui, key: &str, v: &Value, lk: Lookups) {
    let a = about(key);
    let count = match v {
        Value::Table(t) => format!("  ·  {} entries", t.entries.len()),
        _ => String::new(),
    };
    let title = match a {
        Some(a) => format!("{}  —  {key}{count}", a.title),
        None => format!("{key}{count}"),
    };
    egui::CollapsingHeader::new(RichText::new(title).color(theme::TX))
        .id_source(("inspect.table", key))
        .default_open(false)
        .show(ui, |ui| {
            match a {
                Some(a) => {
                    ui.label(RichText::new(a.what).color(theme::DIM));
                    ui.label(RichText::new(format!("Written by {}", a.writer)).small().color(theme::FAINT));
                }
                None => {
                    ui.label(
                        RichText::new("Not written by GenerateSaveData; meaning unknown.").color(theme::BAD),
                    );
                }
            }
            ui.add_space(4.0);
            match v {
                Value::Table(t) if !t.entries.is_empty() => {
                    for e in t.iter_entries() {
                        let k = key_text(e);
                        ui.push_id(&k, |ui| tree(ui, &crate::save_tables::humanize(&k), &e.value, lk));
                    }
                }
                Value::Table(_) => {
                    ui.label(RichText::new("empty").color(theme::FAINT));
                }
                leaf => theme::kv(ui, "value", RichText::new(value_text(leaf, lk))),
            }
            let id = ui.make_persistent_id(("inspect.raw", key));
            let mut raw = ui.data_mut(|d| d.get_temp::<bool>(id).unwrap_or(false));
            if ui.checkbox(&mut raw, "Show raw Lua").changed() {
                ui.data_mut(|d| d.insert_temp(id, raw));
            }
            if raw {
                let mut text = format!("[\"{key}\"] = {}", save_lua::serialize(v));
                egui::ScrollArea::vertical().max_height(320.0).id_source(("inspect.raw.scroll", key)).show(ui, |ui| {
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

fn hms(s: u32) -> String {
    format!("{}h {:02}m {:02}s", s / 3600, (s % 3600) / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::game_paths;

    /// Every top-level table in every fixture save has a description, so the Inspect window
    /// never shows a retail table as unknown.
    #[test]
    fn every_fixture_table_is_described() {
        for name in game_paths::SAVE_FIXTURES {
            let ins = Inspected::load(&game_paths::save_fixtures().join(name));
            let root = match ins.tree {
                Ok(Value::Table(t)) => t,
                other => panic!("{name}: {:?}", other.map(|v| v.kind())),
            };
            for (k, _) in root.iter_str() {
                assert!(about(k).is_some(), "{name}: no description for {k}");
            }
        }
    }

    #[test]
    fn descriptions_are_unique() {
        for (i, a) in ABOUT.iter().enumerate() {
            assert!(ABOUT[i + 1..].iter().all(|b| b.key != a.key), "{} described twice", a.key);
        }
    }

    #[test]
    fn an_unreadable_file_reports_errors_instead_of_panicking() {
        let ins = Inspected::load(Path::new("/definitely/not/a/save.profile"));
        assert!(ins.bytes.is_err() && ins.profile.is_err() && ins.tree.is_err());
    }
}
