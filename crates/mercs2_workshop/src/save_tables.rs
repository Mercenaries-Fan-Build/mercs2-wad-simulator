//! Read-only inspector sections for every save table the Saves page does not otherwise show.
//!
//! Rendered from the parsed Lua tree (`save_lua`), so a section shows what the save holds rather
//! than what a hand-written decoder chose to keep. Ids and hash keys are resolved through the
//! game's English string table and `worldentity` template names when those are loaded; anything
//! that does not resolve shows its raw value. Any top-level key no section claims lands in OTHER.

use std::collections::HashMap;

use egui::RichText;
use mercs2_formats::save_lua::{Entry, Table, Value};
use mercs2_formats::stringdb::key_hash;

use crate::gui::theme;

/// Name lookups loaded from the open WAD stack.
#[derive(Clone, Copy, Default)]
pub struct Lookups<'a> {
    pub english: Option<&'a HashMap<u32, String>>,
    pub templates: Option<&'a HashMap<u32, String>>,
}

impl<'a> Lookups<'a> {
    /// English text for a string-table key (`[Generic.Contracts]`, `[0x5cb97d23]`, `vehicle.wz10`).
    fn text(&self, key: &str) -> Option<&'a str> {
        self.english?.get(&key_hash(key)).map(String::as_str)
    }

    /// Display name of a support/vehicle id. `MrxSupportData` (`resident/mrxsupportdata.lua`)
    /// names 132 of its 134 entries `[vehicle.<id>]` or `[support.<category>.<id>.name]` with
    /// category `airstrike` / `supply` / `soldierdelivery`; the other two show raw.
    fn support(&self, id: &str) -> Option<&'a str> {
        [
            format!("vehicle.{id}"),
            format!("support.airstrike.{id}.name"),
            format!("support.supply.{id}.name"),
            format!("support.soldierdelivery.{id}.name"),
        ]
        .iter()
        .find_map(|k| self.text(k))
    }
}

/// Top-level keys the Saves page shows outside this module: the centre's PROGRESSION tabs
/// (`tFlowData`, `tLayerData`), and the inspector's LOADOUT (`vEquippedSupport`, `tPlayerData`),
/// TRANSIT ZONES (`tTransitData`) and METADATA (`nTimeElapsed`).
pub const SHOWN_ELSEWHERE: &[&str] =
    &["tFlowData", "tLayerData", "vEquippedSupport", "tPlayerData", "tTransitData", "nTimeElapsed"];

/// Top-level keys the sections in this module show.
pub const SHOWN_HERE: &[&str] = &[
    "tFactionData",
    "tPmcData",
    "tPmcInteriorData",
    "nAvailableCostumes",
    "tShopData",
    "tSupportData",
    "tWifEquipmentData",
    "tRewardData",
    "tStarterData",
    "tRetryLocations",
    "sLastMissionName",
    "tBoundaryData",
    "tLockedGates",
    "tStatsData",
    "tActiveBio",
    "tTutorialData",
    "tActiveHint",
    "tMunitionsData",
    "tMrxAchievementsData",
    "bGrappleEnabled",
    "bVehicleDisguiseEnabled",
    "bResourceCountersEnabled",
];

/// Top-level keys of `root` that no section shows.
pub fn unclaimed_keys(root: &Table) -> Vec<&str> {
    root.iter_str()
        .map(|(k, _)| k)
        .filter(|k| !SHOWN_ELSEWHERE.contains(k) && !SHOWN_HERE.contains(k))
        .collect()
}

/// Every section, in panel order.
pub fn sections(ui: &mut egui::Ui, root: &Table, lk: Lookups) {
    factions(ui, root);
    pmc(ui, root, lk);
    shops(ui, root, lk);
    arsenal(ui, root, lk);
    contracts(ui, root);
    stats(ui, root, lk);
    targets(ui, root);
    progress(ui, root);
    let other = unclaimed_keys(root);
    if !other.is_empty() {
        theme::section(ui, "OTHER", Some(&format!("{} tables", other.len())), false, |ui| {
            for k in other {
                if let Some(v) = root.get(k) {
                    tree(ui, &humanize(k), v, lk);
                }
            }
        });
    }
}

// ── value access ──────────────────────────────────────────────────────────────────────────────

fn table(v: Option<&Value>) -> Option<&Table> {
    match v {
        Some(Value::Table(t)) => Some(t),
        _ => None,
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => Some(n.value),
        _ => None,
    }
}

fn text(v: Option<&Value>) -> Option<&str> {
    match v {
        Some(Value::String(s)) => Some(s.value.as_str()),
        _ => None,
    }
}

fn flag(v: Option<&Value>) -> Option<bool> {
    match v {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

/// `Sys.StringToGuid('0x8000569E')` → `0x8000569E`.
fn guid(v: &Value) -> Option<u32> {
    let Value::Call(c) = v else { return None };
    if c.func != "Sys.StringToGuid" {
        return None;
    }
    let Some(Value::String(s)) = c.args.first() else { return None };
    let h = s.value.strip_prefix("0x").or_else(|| s.value.strip_prefix("0X"))?;
    u32::from_str_radix(h, 16).ok()
}

/// An entry's key as text: string keys verbatim, numeric keys as integers when whole.
fn key_text(e: &Entry) -> String {
    match &e.key {
        Some(Value::String(s)) => s.value.clone(),
        Some(Value::Number(n)) => fmt_num(n.value),
        Some(other) => format!("<{}>", other.kind()),
        None => String::new(),
    }
}

/// Values of a numeric-keyed array, as text, in order.
fn array_texts(t: &Table) -> Vec<String> {
    t.iter_entries().map(|e| value_text(&e.value, Lookups::default())).collect()
}

/// `bPlayedCashHint1` → `Played cash hint 1`, `nCivilianCasualties` → `Civilian casualties`.
/// Only keys carrying the one-letter type prefix the game's Lua uses for fields are rewritten;
/// identifiers (`PmcCon033`, `VehicleDisguise`, `Oil`) are already names and stay verbatim.
pub fn humanize(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let body: &[char] = match chars.as_slice() {
        [p, second, ..] if "bnstvouf".contains(*p) && second.is_ascii_uppercase() => &chars[1..],
        _ => return key.to_string(),
    };
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in body.iter().enumerate() {
        let prev = if i > 0 { Some(body[i - 1]) } else { None };
        let boundary = match prev {
            Some(p) => {
                (c.is_ascii_uppercase() && p.is_ascii_lowercase())
                    || (c.is_ascii_digit() && !p.is_ascii_digit())
                    || (!c.is_ascii_digit() && p.is_ascii_digit())
                    || c == '_'
            }
            None => false,
        };
        if boundary && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        if c != '_' {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    if words.is_empty() {
        return key.to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
        let all_caps = w.len() > 1 && w.chars().all(|c| !c.is_ascii_lowercase());
        out.push(if all_caps {
            w.clone()
        } else if i == 0 {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + &c.as_str().to_lowercase()).unwrap_or_default()
        } else {
            w.to_lowercase()
        });
    }
    out.join(" ")
}

/// Whole numbers with thousands separators, others to two decimals.
pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        let n = v as i64;
        let s = n.unsigned_abs().to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        if n < 0 {
            format!("-{out}")
        } else {
            out
        }
    } else {
        format!("{v:.2}")
    }
}

fn money(v: f64) -> String {
    let s = fmt_num(v.round());
    match s.strip_prefix('-') {
        Some(rest) => format!("-${rest}"),
        None => format!("${s}"),
    }
}

/// Seconds as `2m 54.95s` / `1h 02m`.
fn duration(secs: f64) -> String {
    if secs >= 3600.0 {
        let s = secs as u64;
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    } else if secs >= 60.0 {
        format!("{}m {:05.2}s", (secs / 60.0).floor() as u64, secs % 60.0)
    } else {
        format!("{secs:.2}s")
    }
}

fn value_text(v: &Value, lk: Lookups) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Bool(b) => if *b { "yes" } else { "no" }.into(),
        Value::Number(n) => fmt_num(n.value),
        Value::String(s) if s.value.is_empty() => "—".into(),
        Value::String(s) => s.value.clone(),
        Value::Call(_) => match guid(v) {
            Some(g) => match lk.templates.and_then(|t| t.get(&g)) {
                Some(name) => format!("{name} (0x{g:08X})"),
                None => format!("0x{g:08X}"),
            },
            None => "<call>".into(),
        },
        Value::Table(t) => format!("{} entries", t.entries.len()),
    }
}

// ── layout helpers ────────────────────────────────────────────────────────────────────────────

fn sub(ui: &mut egui::Ui, label: &str) {
    ui.add_space(4.0);
    ui.label(theme::disp_text(label, theme::style_eyebrow(), theme::DIM));
}

fn faint(ui: &mut egui::Ui, s: &str) {
    ui.label(RichText::new(s).small().color(theme::FAINT));
}

/// Name on the left, value on the right, raw id dimmed after the name when it was resolved.
fn named_row(ui: &mut egui::Ui, name: Option<&str>, raw: &str, value: &str) {
    ui.horizontal(|ui| {
        match name {
            Some(n) => {
                ui.label(RichText::new(n).color(theme::TX));
                ui.label(RichText::new(raw).small().color(theme::FAINT));
            }
            None => {
                ui.label(RichText::new(raw).monospace().color(theme::TX));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).monospace().color(theme::TX));
        });
    });
}

/// Comma-joined words, wrapped.
fn word_list(ui: &mut egui::Ui, words: &[String]) {
    if words.is_empty() {
        faint(ui, "none");
    } else {
        ui.label(RichText::new(words.join(", ")).color(theme::TX));
    }
}

/// Generic readable tree for a value: tables collapse, leaves are key/value rows.
fn tree(ui: &mut egui::Ui, label: &str, v: &Value, lk: Lookups) {
    match v {
        Value::Table(t) if !t.entries.is_empty() => {
            egui::CollapsingHeader::new(RichText::new(format!("{label}  ·  {}", t.entries.len())).color(theme::TX))
                .id_source(ui.id().with(label))
                .default_open(false)
                .show(ui, |ui| {
                    for e in t.iter_entries() {
                        let k = key_text(e);
                        ui.push_id(&k, |ui| tree(ui, &humanize(&k), &e.value, lk));
                    }
                });
        }
        Value::Table(_) => theme::kv(ui, label, RichText::new("empty")),
        leaf => theme::kv(ui, label, RichText::new(value_text(leaf, lk))),
    }
}

// ── sections ──────────────────────────────────────────────────────────────────────────────────

/// `tFactionData` — `tRelations` (each faction's standing toward each other, −100…100), civilian
/// casualties and penalty, and the factions whose standing can change.
fn factions(ui: &mut egui::Ui, root: &Table) {
    let Some(fd) = table(root.get("tFactionData")) else { return };
    let rel = table(fd.get("tRelations"));
    let n = rel.map_or(0, |r| r.entries.len());
    theme::section(ui, "FACTIONS", Some(&format!("{n} factions")), false, |ui| {
        if let Some(rel) = rel {
            let rows: Vec<&str> = rel.iter_str().map(|(k, _)| k).collect();
            let mut cols: Vec<&str> = Vec::new();
            for (_, row) in rel.iter_str() {
                if let Value::Table(t) = row {
                    for (k, _) in t.iter_str() {
                        if !cols.contains(&k) {
                            cols.push(k);
                        }
                    }
                }
            }
            faint(ui, "Standing of the row faction toward the column faction (−100 … 100).");
            egui::ScrollArea::horizontal().id_source("factions.grid").show(ui, |ui| {
                egui::Grid::new("factions.grid.cells").striped(true).spacing([8.0, 2.0]).show(ui, |ui| {
                    ui.label("");
                    for c in &cols {
                        ui.label(RichText::new(*c).color(theme::DIM));
                    }
                    ui.end_row();
                    for r in &rows {
                        ui.label(RichText::new(*r).color(theme::DIM));
                        let row = table(rel.get(r));
                        for c in &cols {
                            match row.and_then(|t| num(t.get(c))) {
                                Some(v) => {
                                    let col = if v > 0.0 {
                                        theme::GOOD
                                    } else if v < 0.0 {
                                        theme::BAD
                                    } else {
                                        theme::FAINT
                                    };
                                    ui.label(RichText::new(fmt_num(v)).monospace().color(col));
                                }
                                None => {
                                    ui.label(RichText::new("·").color(theme::FAINT));
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
            });
        }
        ui.add_space(4.0);
        if let Some(v) = num(fd.get("nCivilianCasualties")) {
            theme::kv(ui, "Civilian casualties", RichText::new(fmt_num(v)));
        }
        if let Some(v) = num(fd.get("nCivilianPenalty")) {
            theme::kv(ui, "Civilian penalty", RichText::new(fmt_num(v)));
        }
        if let Some(t) = table(fd.get("tMutableFactions")) {
            sub(ui, "STANDING CAN CHANGE");
            word_list(ui, &array_texts(t));
        }
        for (k, v) in fd.iter_str() {
            if !["tRelations", "nCivilianCasualties", "nCivilianPenalty", "tMutableFactions"].contains(&k) {
                tree(ui, &humanize(k), v, Lookups::default());
            }
        }
    });
}

/// `tPmcData` + `tPmcInteriorData` + `nAvailableCostumes` — PMC money/fuel, the HQ, recruits,
/// fuel tanks, the vehicle/support stockpile and freebies.
fn pmc(ui: &mut egui::Ui, root: &Table, lk: Lookups) {
    let pd = table(root.get("tPmcData"));
    let pi = table(root.get("tPmcInteriorData"));
    if pd.is_none() && pi.is_none() && root.get("nAvailableCostumes").is_none() {
        return;
    }
    let stock = pd.and_then(|t| table(t.get("tStockpile")));
    let held = stock.map_or(0, |s| {
        s.iter_str().filter(|(_, v)| table(Some(v)).and_then(|t| num(t.get("nAmt"))).unwrap_or(0.0) > 0.0).count()
    });
    theme::section(ui, "PMC", Some(&format!("{held} stockpiled")), false, |ui| {
        if let Some(pd) = pd {
            if let Some(v) = num(pd.get("nCash")) {
                theme::kv(ui, "Cash (tPmcData)", RichText::new(money(v)));
            }
            if let Some(v) = num(pd.get("nFuel")) {
                theme::kv(ui, "Fuel (tPmcData)", RichText::new(fmt_num(v)));
            }
        }
        if let Some(q) = pi.and_then(|t| table(t.get("tStockpileQty"))) {
            if let Some(v) = num(q.get("money")) {
                theme::kv(ui, "Money (HQ stockpile)", RichText::new(money(v)));
            }
        }
        if let Some(v) = num(root.get("nAvailableCostumes")) {
            theme::kv(ui, "Available costumes", RichText::new(fmt_num(v)));
        }
        if let Some(pi) = pi {
            if let Some(b) = flag(pi.get("bUnlocked")) {
                theme::kv(ui, "HQ unlocked", RichText::new(if b { "yes" } else { "no" }));
            }
            if let Some(t) = table(pi.get("tIntroduced")) {
                sub(ui, "RECRUITS INTRODUCED");
                word_list(ui, &t.iter_str().filter(|(_, v)| flag(Some(v)) == Some(true)).map(|(k, _)| k.to_string()).collect::<Vec<_>>());
            }
            if let Some(q) = table(pi.get("tStockpileQty")) {
                sub(ui, "HQ STOCKPILE");
                for (k, v) in q.iter_str().filter(|(k, _)| *k != "money") {
                    named_row(ui, lk.support(k), k, &num(Some(v)).map(fmt_num).unwrap_or_else(|| value_text(v, lk)));
                }
            }
            for (k, v) in pi.iter_str() {
                if !["bUnlocked", "tIntroduced", "tStockpileQty"].contains(&k) {
                    tree(ui, &humanize(k), v, lk);
                }
            }
        }
        if let Some(pd) = pd {
            if let Some(t) = table(pd.get("tEquipment")) {
                sub(ui, "FUEL TANKS OWNED");
                word_list(ui, &t.iter_str().filter(|(_, v)| flag(Some(v)) == Some(true)).map(|(k, _)| k.to_string()).collect::<Vec<_>>());
            }
            if let Some(s) = stock {
                sub(ui, "STOCKPILE");
                let mut zero: Vec<String> = Vec::new();
                for (id, v) in s.iter_str() {
                    let t = table(Some(v));
                    let amt = t.and_then(|t| num(t.get("nAmt"))).unwrap_or(0.0);
                    let new = t.and_then(|t| flag(t.get("bNew"))).unwrap_or(false);
                    let name = lk.support(id);
                    if amt > 0.0 {
                        let val = if new { format!("{}  new", fmt_num(amt)) } else { fmt_num(amt) };
                        named_row(ui, name, id, &val);
                    } else {
                        zero.push(name.map(str::to_string).unwrap_or_else(|| id.to_string()));
                    }
                }
                if !zero.is_empty() {
                    egui::CollapsingHeader::new(RichText::new(format!("{} at 0", zero.len())).color(theme::DIM))
                        .id_source("pmc.stockpile.zero")
                        .show(ui, |ui| word_list(ui, &zero));
                }
            }
            if let Some(t) = table(pd.get("tFreebies")) {
                sub(ui, "FREEBIES");
                if t.entries.is_empty() {
                    faint(ui, "none");
                }
                for e in t.iter_entries() {
                    let k = key_text(e);
                    named_row(ui, lk.text(&k).or_else(|| lk.support(&k)), &k, &value_text(&e.value, lk));
                }
            }
            for (k, v) in pd.iter_str() {
                if !["nCash", "nFuel", "tEquipment", "tStockpile", "tFreebies"].contains(&k) {
                    tree(ui, &humanize(k), v, lk);
                }
            }
        }
    });
}

/// `tShopData` — each faction shop's item total and what was bought from it.
fn shops(ui: &mut egui::Ui, root: &Table, lk: Lookups) {
    let Some(sd) = table(root.get("tShopData")) else { return };
    theme::section(ui, "SHOPS", Some(&format!("{} shops", sd.entries.len())), false, |ui| {
        for (faction, v) in sd.iter_str() {
            let Some(shop) = table(Some(v)) else {
                tree(ui, faction, v, lk);
                continue;
            };
            let bought = table(shop.get("tPurchased"));
            let n_eq = bought.and_then(|b| num(b.get("nEquipment"))).unwrap_or(0.0);
            let n_su = bought.and_then(|b| num(b.get("nSupport"))).unwrap_or(0.0);
            let total = num(shop.get("nTotalItems"));
            let head = match total {
                Some(t) => format!("{faction}  ·  {} bought of {} items", fmt_num(n_eq + n_su), fmt_num(t)),
                None => format!("{faction}  ·  {} bought", fmt_num(n_eq + n_su)),
            };
            egui::CollapsingHeader::new(RichText::new(head).color(theme::TX))
                .id_source(("shop", faction))
                .show(ui, |ui| {
                    if let Some(b) = bought {
                        let items = |key: &str| -> Vec<String> {
                            table(b.get(key))
                                .map(|t| {
                                    t.iter_str()
                                        .filter(|(_, v)| flag(Some(v)) == Some(true))
                                        .map(|(k, _)| lk.support(k).map(str::to_string).unwrap_or_else(|| k.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default()
                        };
                        sub(ui, "SUPPORT");
                        word_list(ui, &items("tSupport"));
                        sub(ui, "EQUIPMENT");
                        word_list(ui, &items("tEquipment"));
                    }
                });
        }
    });
}

/// `tSupportData` + `tWifEquipmentData` — every support/vehicle and equipment entry with its
/// per-faction `tUnlockStatus` value (meaning not established, so shown raw) and any other saved
/// field (`nGlobalStock` when set), plus `tSupportData._tRequirementsObtained`
/// (`MrxSupportData.SaveSingleton`, `resident/mrxsupportdata.lua:2433-2443`).
fn arsenal(ui: &mut egui::Ui, root: &Table, lk: Lookups) {
    let sup = table(root.get("tSupportData"));
    let eq = table(root.get("tWifEquipmentData"));
    if sup.is_none() && eq.is_none() {
        return;
    }
    let n = sup.map_or(0, |t| t.entries.len()) + eq.map_or(0, |t| t.entries.len());
    theme::section(ui, "ARSENAL", Some(&format!("{n} unlock entries")), false, |ui| {
        faint(ui, "Unlock status per faction, as saved (tUnlockStatus).");
        let per_faction = |t: &Table| -> String {
            t.iter_str()
                .map(|(f, s)| format!("{f} {}", num(Some(s)).map(fmt_num).unwrap_or_else(|| value_text(s, lk))))
                .collect::<Vec<_>>()
                .join(", ")
        };
        // A support entry: its unlock status, then every other field it saved.
        let support_fields = |v: &Value| -> String {
            let Some(t) = table(Some(v)) else { return value_text(v, lk) };
            let mut parts: Vec<String> = Vec::new();
            if let Some(u) = table(t.get("tUnlockStatus")) {
                parts.push(per_faction(u));
            }
            for (k, x) in t.iter_str().filter(|(k, _)| *k != "tUnlockStatus") {
                parts.push(format!("{} {}", humanize(k).to_lowercase(), value_text(x, lk)));
            }
            parts.join(" · ")
        };
        const REQUIREMENTS: &str = "_tRequirementsObtained";
        if let Some(r) = sup.and_then(|s| s.get(REQUIREMENTS)) {
            sub(ui, "REQUIREMENTS OBTAINED");
            match table(Some(r)) {
                Some(t) => word_list(
                    ui,
                    &t.iter_str().filter(|(_, v)| flag(Some(v)) == Some(true)).map(|(k, _)| k.to_string()).collect::<Vec<_>>(),
                ),
                None => faint(ui, &value_text(r, lk)),
            }
        }
        if let Some(sup) = sup {
            let ids = sup.iter_str().filter(|(k, _)| *k != REQUIREMENTS).count();
            egui::CollapsingHeader::new(RichText::new(format!("Support & vehicles  ·  {ids}")).color(theme::TX))
                .id_source("arsenal.support")
                .show(ui, |ui| {
                    let mut rows: Vec<(String, &str, String)> = sup
                        .iter_str()
                        .filter(|(id, _)| *id != REQUIREMENTS)
                        .map(|(id, v)| (lk.support(id).unwrap_or(id).to_string(), id, support_fields(v)))
                        .collect();
                    rows.sort_by(|a, b| a.0.cmp(&b.0));
                    for (name, id, st) in rows {
                        named_row(ui, (name != id).then_some(name.as_str()), id, &st);
                    }
                });
        }
        if let Some(eq) = eq {
            egui::CollapsingHeader::new(RichText::new(format!("Equipment  ·  {}", eq.entries.len())).color(theme::TX))
                .id_source("arsenal.equipment")
                .show(ui, |ui| {
                    for (id, v) in eq.iter_str() {
                        let st = table(Some(v)).map(per_faction).unwrap_or_else(|| value_text(v, lk));
                        named_row(ui, None, id, &st);
                    }
                });
        }
    });
}

/// `tRewardData` (wagers), `tStarterData` (contacts), `sLastMissionName`, `tRetryLocations` (the
/// in-progress contract's retry checkpoint; only in a save taken mid-contract), `tBoundaryData`,
/// `tLockedGates`.
fn contracts(ui: &mut egui::Ui, root: &Table) {
    let rewards = table(root.get("tRewardData"));
    let starters = table(root.get("tStarterData"));
    theme::section(
        ui,
        "CONTRACTS",
        Some(&format!("{} contacts", starters.map_or(0, |t| t.entries.len()))),
        false,
        |ui| {
            if let Some(s) = text(root.get("sLastMissionName")) {
                theme::kv(ui, "Last mission", RichText::new(s));
            }
            match root.get("tRetryLocations") {
                Some(Value::Table(t)) => {
                    theme::kv(ui, "Retry checkpoint", RichText::new(array_texts(t).join(", ")));
                }
                Some(other) => tree(ui, "Retry checkpoint", other, Lookups::default()),
                None => theme::kv(ui, "Retry checkpoint", RichText::new("none (not mid-contract)")),
            }
            if let Some(b) = table(root.get("tBoundaryData")) {
                for (k, v) in b.iter_str() {
                    theme::kv(ui, &humanize(k), RichText::new(value_text(v, Lookups::default())));
                }
            }
            if let Some(g) = table(root.get("tLockedGates")) {
                theme::kv(ui, "Locked gates", RichText::new(if g.entries.is_empty() { "none".to_string() } else { array_texts(g).join(", ") }));
            }
            if let Some(r) = rewards {
                sub(ui, "WAGERS");
                for (contract, v) in r.iter_str() {
                    match table(Some(v)).and_then(|t| num(t.get("nWager"))) {
                        Some(w) => named_row(ui, None, contract, &money(w)),
                        None => tree(ui, contract, v, Lookups::default()),
                    }
                }
            }
            if let Some(st) = starters {
                sub(ui, "CONTACTS");
                for (who, v) in st.iter_str() {
                    let Some(t) = table(Some(v)) else {
                        tree(ui, who, v, Lookups::default());
                        continue;
                    };
                    let card = flag(t.get("bCardDisplayed")).unwrap_or(false);
                    let fanfare = flag(t.get("bFanfareDisplayed")).unwrap_or(false);
                    let briefings: Vec<String> = table(t.get("tOldBriefings"))
                        .map(|b| b.iter_str().map(|(k, _)| k.to_string()).collect())
                        .unwrap_or_default();
                    let mut bits = Vec::new();
                    if card {
                        bits.push("card shown".to_string());
                    }
                    if fanfare {
                        bits.push("fanfare played".to_string());
                    }
                    if !briefings.is_empty() {
                        bits.push(format!("briefed: {}", briefings.join(", ")));
                    }
                    named_row(ui, None, who, &bits.join(" · "));
                    for (k, v) in t.iter_str() {
                        if !["bCardDisplayed", "bFanfareDisplayed", "tOldBriefings"].contains(&k) {
                            tree(ui, &humanize(k), v, Lookups::default());
                        }
                    }
                }
            }
        },
    );
}

/// `tStatsData` except the toolbox counter (COLLECTIBLES) and `oVerifyData` (TARGETS).
fn stats(ui: &mut egui::Ui, root: &Table, lk: Lookups) {
    let Some(st) = table(root.get("tStatsData")) else { return };
    theme::section(ui, "STATS", None, false, |ui| {
        for (k, v) in st.iter_str() {
            let Some(n) = num(Some(v)) else { continue };
            let shown = match k {
                "nTotalCredit" | "nTotalDebit" => money(n),
                "nBestRaceTime" => duration(n),
                _ => fmt_num(n),
            };
            if k != "nCompletedToolboxes" {
                theme::kv(ui, &humanize(k), RichText::new(shown));
            }
        }
        let timed = |ui: &mut egui::Ui, title: &str, key: &str| {
            if let Some(t) = table(st.get(key)) {
                sub(ui, title);
                let mut rows: Vec<(String, String, f64)> = t
                    .iter_str()
                    .map(|(k, v)| (lk.text(k).unwrap_or(k).to_string(), k.to_string(), num(Some(v)).unwrap_or(0.0)))
                    .collect();
                rows.sort_by(|a, b| b.2.total_cmp(&a.2));
                for (name, raw, t) in rows {
                    named_row(ui, (name != raw).then_some(name.as_str()), &raw, &duration(t));
                }
            }
        };
        timed(ui, "FAVOURITE WEAPONS (time used)", "tFavWeapon");
        timed(ui, "FAVOURITE VEHICLES (time used)", "tFavVehicle");
        timed(ui, "RACES (best time)", "tListOfRaces");
        let money_list = |ui: &mut egui::Ui, title: &str, key: &str| {
            if let Some(t) = table(st.get(key)) {
                sub(ui, title);
                for (k, v) in t.iter_str() {
                    named_row(ui, lk.text(k), k, &num(Some(v)).map(money).unwrap_or_else(|| value_text(v, lk)));
                }
            }
        };
        money_list(ui, "CREDITS", "tListOfCredits");
        money_list(ui, "DEBITS", "tListOfDebits");
        if let Some(t) = table(st.get("tDestroyBty")) {
            sub(ui, "DESTROY BOUNTIES");
            for (k, v) in t.iter_str() {
                named_row(ui, None, k, &num(Some(v)).map(fmt_num).unwrap_or_else(|| value_text(v, lk)));
            }
        }
        for (k, v) in st.iter_str() {
            let handled = num(Some(v)).is_some()
                || ["tFavWeapon", "tFavVehicle", "tListOfRaces", "tListOfCredits", "tListOfDebits", "tDestroyBty", "oVerifyData"]
                    .contains(&k);
            if !handled {
                tree(ui, &humanize(k), v, lk);
            }
        }
    });
}

/// `tStatsData.oVerifyData` — `sSolanoStatus` and each faction's high-value targets' fate.
fn targets(ui: &mut egui::Ui, root: &Table) {
    let Some(vd) = table(root.get("tStatsData")).and_then(|t| table(t.get("oVerifyData"))) else { return };
    let lists = table(vd.get("tTargetListStatus"));
    let total = lists.map_or(0, |l| l.iter_str().map(|(_, v)| table(Some(v)).map_or(0, |t| t.entries.len())).sum());
    theme::section(ui, "HIGH-VALUE TARGETS", Some(&format!("{total} targets")), false, |ui| {
        if let Some(s) = text(vd.get("sSolanoStatus")) {
            theme::kv(ui, "Solano", RichText::new(if s.is_empty() { "—" } else { s }));
        }
        if let Some(l) = lists {
            for (faction, v) in l.iter_str() {
                let Some(t) = table(Some(v)) else { continue };
                let done = t.iter_str().filter(|(_, s)| !text(Some(s)).unwrap_or("").is_empty()).count();
                egui::CollapsingHeader::new(RichText::new(format!("{faction}  ·  {done} of {} resolved", t.entries.len())).color(theme::TX))
                    .id_source(("hvt", faction))
                    .show(ui, |ui| {
                        let mut rows: Vec<(&str, &str)> =
                            t.iter_str().map(|(k, s)| (k, text(Some(s)).unwrap_or(""))).collect();
                        rows.sort();
                        for (target, fate) in rows {
                            let col = match fate {
                                "killed" => theme::BAD,
                                "captured" => theme::GOOD,
                                _ => theme::FAINT,
                            };
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(target).monospace().color(theme::TX));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(if fate.is_empty() { "—" } else { fate }).color(col));
                                });
                            });
                        }
                    });
            }
        }
        for (k, v) in vd.iter_str() {
            if !["sSolanoStatus", "tTargetListStatus"].contains(&k) {
                tree(ui, &humanize(k), v, Lookups::default());
            }
        }
    });
}

/// Bios, tutorials, hints, munitions hint flags, gameplay switches and achievement tracking.
fn progress(ui: &mut egui::Ui, root: &Table) {
    theme::section(ui, "PROGRESS FLAGS", None, false, |ui| {
        for k in ["bGrappleEnabled", "bVehicleDisguiseEnabled", "bResourceCountersEnabled"] {
            if let Some(b) = flag(root.get(k)) {
                theme::kv(ui, &humanize(k), RichText::new(if b { "on" } else { "off" }));
            }
        }
        if let Some(t) = table(root.get("tActiveBio")) {
            let on: Vec<String> = t
                .iter_str()
                .filter(|(_, v)| flag(Some(v)) == Some(true))
                .map(|(k, _)| k.strip_prefix("Bio").unwrap_or(k).to_string())
                .collect();
            sub(ui, &format!("BIOS UNLOCKED  ·  {}", on.len()));
            word_list(ui, &on);
        }
        if let Some(t) = table(root.get("tTutorialData")) {
            let seen = array_texts(t);
            sub(ui, &format!("TUTORIALS SEEN  ·  {}", seen.len()));
            word_list(ui, &seen);
        }
        if let Some(t) = table(root.get("tActiveHint")) {
            sub(ui, "HINTS QUEUED");
            for (who, v) in t.iter_str() {
                let hints = table(Some(v)).map(array_texts).unwrap_or_default();
                named_row(ui, None, who, &format!("{}", hints.len()))
            }
        }
        if let Some(t) = table(root.get("tMunitionsData")) {
            sub(ui, "MUNITIONS");
            for (k, v) in t.iter_str() {
                theme::kv(ui, &humanize(k), RichText::new(value_text(v, Lookups::default())));
            }
        }
        if let Some(t) = table(root.get("tMrxAchievementsData")) {
            sub(ui, "ACHIEVEMENT TRACKING");
            for (k, v) in t.iter_str() {
                tree(ui, &humanize(k), v, Lookups::default());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::{game_paths, save, save_lua};

    fn tree_of(name: &str) -> Value {
        let path = game_paths::save_fixtures().join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let lua = save::parse(&bytes).unwrap().decompress_lua().unwrap();
        save_lua::parse_chunk(std::str::from_utf8(&lua).unwrap()).unwrap()
    }

    /// Every top-level table in every fixture save is shown by some section: nothing falls to
    /// OTHER, and nothing is silently dropped.
    #[test]
    fn every_fixture_top_level_key_is_claimed() {
        for name in game_paths::SAVE_FIXTURES {
            let root = tree_of(name);
            let t = root.as_table().unwrap();
            assert!(t.entries.len() > 5, "{name}: suspiciously few top-level keys");
            assert_eq!(unclaimed_keys(t), Vec::<&str>::new(), "{name}");
        }
    }

    #[test]
    fn an_unknown_top_level_key_is_unclaimed() {
        let root = save_lua::parse_chunk(r#"return { ["tFactionData"] = { }, ["tBrandNew"] = { } }"#).unwrap();
        assert_eq!(unclaimed_keys(root.as_table().unwrap()), vec!["tBrandNew"]);
    }

    #[test]
    fn humanize_strips_the_type_prefix_and_splits_words() {
        assert_eq!(humanize("nCivilianCasualties"), "Civilian casualties");
        assert_eq!(humanize("bPlayedCashHint1"), "Played cash hint 1");
        assert_eq!(humanize("sSolanoStatus"), "Solano status");
        assert_eq!(humanize("VehicleDisguise"), "VehicleDisguise", "identifiers stay verbatim");
        assert_eq!(humanize("PmcCon033"), "PmcCon033");
        assert_eq!(humanize("APC"), "APC");
    }

    #[test]
    fn numbers_format_for_reading() {
        assert_eq!(fmt_num(731_715_968.0), "731,715,968");
        assert_eq!(fmt_num(-82_660.0), "-82,660");
        assert_eq!(fmt_num(66.5), "66.50");
        assert_eq!(money(-1500.0), "-$1,500");
        assert_eq!(duration(174.949661), "2m 54.95s");
        assert_eq!(duration(31.5), "31.50s");
    }

    #[test]
    fn guid_calls_decode() {
        let v = save_lua::parse_value("Sys.StringToGuid( '0x80005FD3' )").unwrap();
        assert_eq!(guid(&v), Some(0x8000_5FD3));
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;
    use crate::app::WadStack;
    use mercs2_formats::{game_paths, save, save_lua};

    /// Names resolve against the retail english table: every support id of the 6A0E523C fixture
    /// except `MrxSupportData`'s two one-off entries, every favourite-weapon key, and the
    /// credit/debit labels.
    #[test]
    fn retail_names_resolve() {
        let wad = mercs2_engine::wad::resolve_vz_wad(None)
            .expect("retail vz.wad — set vz_wad in .mercs2-local.toml or VZ_WAD");
        let mut w = WadStack::open(&wad, &[]).expect("open retail vz.wad");
        let english = w.string_table("english").expect("english");
        let lk = Lookups { english: Some(&english), templates: None };

        let path = game_paths::save_fixtures().join("Mattias Nilsson_6A0E523C.profile");
        let lua = save::parse(&std::fs::read(path).unwrap()).unwrap().decompress_lua().unwrap();
        let root = save_lua::parse_chunk(std::str::from_utf8(&lua).unwrap()).unwrap();
        let root = root.as_table().unwrap();

        let sup = table(root.get("tSupportData")).unwrap();
        let missing: Vec<&str> = sup
            .iter_str()
            .map(|(k, _)| k)
            .filter(|k| *k != "_tRequirementsObtained" && lk.support(k).is_none())
            .collect();
        for m in &missing {
            assert!(["upcombatairpatrol", "nuke"].contains(m), "{m} does not resolve");
        }
        assert_eq!(lk.support("wz10"), Some("Warsong Attack Helicopter"));
        assert_eq!(lk.support("bunkerbuster"), Some("Bunker Buster"));

        let stats = table(root.get("tStatsData")).unwrap();
        for list in ["tFavWeapon", "tListOfCredits", "tListOfDebits"] {
            for (k, _) in table(stats.get(list)).unwrap().iter_str() {
                assert!(lk.text(k).is_some(), "{list}: {k} does not resolve");
            }
        }
    }
}
