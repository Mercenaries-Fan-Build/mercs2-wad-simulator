//! Readable views of a save's Lua tables, grouped by topic. One body per topic, shared by the
//! Saves inspector panel (the topics marked `panel`) and the Inspect window (every topic), so the
//! two never show the same data two ways.
//!
//! Rendered from the parsed Lua tree (`save_lua`), so a view shows what the save holds rather than
//! what a hand-written decoder chose to keep. Ids and hash keys resolve through the game's English
//! string table and `worldentity` template names when those are loaded; anything that does not
//! resolve shows its raw value. Tables are laid out by shape — records as grids, sets and arrays
//! as word lists, name→value maps as two-column lists — and any top-level key no topic claims is
//! rendered generically under OTHER.

use std::collections::HashMap;

use egui::RichText;
use mercs2_formats::save::Profile;
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

    fn template(&self, key: u32) -> Option<&'a str> {
        self.templates?.get(&key).map(String::as_str)
    }
}

/// What a topic body reads: the save's Lua root, its header when there is one, and lookups.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub root: &'a Table,
    pub profile: Option<&'a Profile>,
    pub lk: Lookups<'a>,
}

/// One topic: the top-level keys it shows, whether the inspector panel shows it, its badge and
/// its body.
pub struct Topic {
    pub title: &'static str,
    pub keys: &'static [&'static str],
    pub panel: bool,
    pub badge: fn(&Ctx) -> Option<String>,
    pub body: fn(&mut egui::Ui, &Ctx),
}

/// Every topic, in reading order. Each top-level key belongs to exactly one topic.
pub const TOPICS: &[Topic] = &[
    Topic { title: "SAVE", keys: &["nTimeElapsed"], panel: false, badge: |_| None, body: save_summary },
    Topic { title: "STORY PROGRESS", keys: &["tFlowData"], panel: false, badge: story_badge, body: story },
    Topic { title: "WORLD STATE", keys: &["tLayerData"], panel: false, badge: world_badge, body: world },
    Topic { title: "LOADOUT", keys: &["tPlayerData", "vEquippedSupport"], panel: true, badge: loadout_badge, body: loadout },
    Topic { title: "FAST TRAVEL", keys: &["tTransitData"], panel: true, badge: travel_badge, body: fast_travel },
    Topic {
        title: "PMC",
        keys: &["tPmcData", "tPmcInteriorData", "nAvailableCostumes"],
        panel: true,
        badge: pmc_badge,
        body: pmc,
    },
    Topic { title: "FACTIONS", keys: &["tFactionData"], panel: true, badge: factions_badge, body: factions },
    Topic { title: "SHOPS", keys: &["tShopData"], panel: true, badge: shops_badge, body: shops },
    Topic { title: "ARSENAL", keys: &["tSupportData", "tWifEquipmentData"], panel: true, badge: arsenal_badge, body: arsenal },
    Topic {
        title: "CONTACTS & CONTRACTS",
        keys: &["tStarterData", "tRewardData", "sLastMissionName", "tRetryLocations", "tBoundaryData", "tLockedGates"],
        panel: true,
        badge: contacts_badge,
        body: contacts,
    },
    Topic { title: "STATS", keys: &["tStatsData"], panel: true, badge: |_| None, body: stats },
    Topic { title: "HIGH-VALUE TARGETS", keys: &[], panel: true, badge: targets_badge, body: targets },
    Topic {
        title: "PROGRESS FLAGS",
        keys: &[
            "bGrappleEnabled",
            "bVehicleDisguiseEnabled",
            "bResourceCountersEnabled",
            "tActiveBio",
            "tTutorialData",
            "tActiveHint",
            "tMunitionsData",
            "tMrxAchievementsData",
        ],
        panel: true,
        badge: |_| None,
        body: progress,
    },
];

/// Top-level keys of `root` no topic claims.
pub fn unclaimed_keys(root: &Table) -> Vec<&str> {
    root.iter_str()
        .map(|(k, _)| k)
        .filter(|k| !TOPICS.iter().any(|t| t.keys.contains(k)))
        .collect()
}

/// The OTHER view: every unclaimed top-level key, rendered by shape.
pub fn other(ui: &mut egui::Ui, ctx: &Ctx) {
    for k in unclaimed_keys(ctx.root) {
        if let Some(v) = ctx.root.get(k) {
            ui.push_id(k, |ui| auto(ui, Some(k), v, ctx.lk));
        }
    }
}

/// The inspector panel's topics, each as a collapsed section, then OTHER when anything is unclaimed.
pub fn panel_sections(ui: &mut egui::Ui, ctx: &Ctx) {
    for t in TOPICS.iter().filter(|t| t.panel) {
        let badge = (t.badge)(ctx);
        theme::section(ui, t.title, badge.as_deref(), false, |ui| (t.body)(ui, ctx));
    }
    let n = unclaimed_keys(ctx.root).len();
    if n > 0 {
        theme::section(ui, "OTHER", Some(&format!("{n} tables")), false, |ui| other(ui, ctx));
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
pub(crate) fn key_text(e: &Entry) -> String {
    match &e.key {
        Some(Value::String(s)) => s.value.clone(),
        Some(Value::Number(n)) => fmt_num(n.value),
        Some(other) => format!("<{}>", other.kind()),
        None => String::new(),
    }
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Table(_))
}

/// Keys of the entries whose value is `true`.
fn true_keys(t: &Table) -> Vec<String> {
    t.iter_entries().filter(|e| matches!(e.value, Value::Bool(true))).map(key_text).collect()
}

/// Values of an array, as text, in order.
fn array_texts(t: &Table, lk: Lookups) -> Vec<String> {
    t.iter_entries().map(|e| value_text(&e.value, lk)).collect()
}

// ── formatting ────────────────────────────────────────────────────────────────────────────────

/// `bPlayedCashHint1` → `Played cash hint 1`, `nCivilianCasualties` → `Civilian casualties`.
/// Only keys carrying the one-letter type prefix the game's Lua uses for fields are rewritten;
/// identifiers (`PmcCon033`, `VehicleDisguise`, `Oil`) are already names and stay verbatim.
pub fn humanize(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let body: &[char] = match chars.as_slice() {
        [p, second, ..] if "bnstvouf".contains(*p) && second.is_ascii_uppercase() => &chars[1..],
        ['_', p, second, ..] if "bnstvouf".contains(*p) && second.is_ascii_uppercase() => &chars[2..],
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

/// Field names whose numbers are money or a duration, wherever they appear.
const MONEY_FIELDS: &[&str] = &["nWager", "nCash", "money", "nTotalCredit", "nTotalDebit", "nCashCost"];
const DURATION_FIELDS: &[&str] = &["nTimeElapsed", "nBestRaceTime"];

pub(crate) fn value_text(v: &Value, lk: Lookups) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Bool(b) => if *b { "yes" } else { "no" }.into(),
        Value::Number(n) => fmt_num(n.value),
        Value::String(s) if s.value.is_empty() => "—".into(),
        Value::String(s) => s.value.clone(),
        Value::Call(_) => match guid(v) {
            Some(g) => match lk.template(g) {
                Some(name) => format!("{name} (0x{g:08X})"),
                None => format!("0x{g:08X}"),
            },
            None => "<call>".into(),
        },
        Value::Table(t) => compact(t, lk),
    }
}

/// A value formatted for the field it sits under (money, duration, else plain).
fn field_text(field: &str, v: &Value, lk: Lookups) -> String {
    match (v, field) {
        (Value::Number(n), f) if MONEY_FIELDS.contains(&f) => money(n.value),
        (Value::Number(n), f) if DURATION_FIELDS.contains(&f) => duration(n.value),
        _ => value_text(v, lk),
    }
}

/// A table squeezed into one line: `Oil 2 · Chi 1`, `a, b, c`, or `none`.
fn compact(t: &Table, lk: Lookups) -> String {
    if t.entries.is_empty() {
        return "none".into();
    }
    let array = t.iter_entries().all(|e| matches!(e.key, Some(Value::Number(_)) | None));
    if array {
        return array_texts(t, lk).join(", ");
    }
    if t.iter_entries().all(|e| matches!(e.value, Value::Bool(true))) {
        return true_keys(t).join(", ");
    }
    t.iter_entries()
        .map(|e| {
            let k = key_text(e);
            format!("{} {}", humanize(&k), field_text(&k, &e.value, lk))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

// ── layout primitives ─────────────────────────────────────────────────────────────────────────

fn sub(ui: &mut egui::Ui, label: &str) {
    ui.add_space(6.0);
    ui.label(theme::disp_text(label, theme::style_eyebrow(), theme::DIM));
}

fn word_list(ui: &mut egui::Ui, words: &[String]) {
    if words.is_empty() {
        ui.label(RichText::new("none").small().color(theme::FAINT));
    } else {
        ui.label(RichText::new(words.join(", ")).color(theme::TX));
    }
}

/// A striped grid: a header row, then rows. Cell text is plain; the first column is a name.
fn grid(ui: &mut egui::Ui, id: &str, headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        ui.label(RichText::new("none").small().color(theme::FAINT));
        return;
    }
    egui::ScrollArea::horizontal().id_source(("grid.scroll", id)).show(ui, |ui| {
        egui::Grid::new(("grid", id)).striped(true).spacing([12.0, 3.0]).num_columns(headers.len()).show(ui, |ui| {
            for h in headers {
                ui.label(RichText::new(*h).small().color(theme::DIM));
            }
            ui.end_row();
            for r in rows {
                for (i, c) in r.iter().enumerate() {
                    let rt = RichText::new(c.as_str()).color(if i == 0 { theme::TX } else { theme::DIM });
                    ui.label(if i == 0 { rt } else { rt.monospace() });
                }
                ui.end_row();
            }
        });
    });
}

/// Render any value by its shape — the fallback for content without a dedicated layout.
fn auto(ui: &mut egui::Ui, label: Option<&str>, v: &Value, lk: Lookups) {
    let name = label.map(humanize);
    let Value::Table(t) = v else {
        theme::kv(ui, name.as_deref().unwrap_or("value"), RichText::new(field_text(label.unwrap_or(""), v, lk)));
        return;
    };
    if t.entries.is_empty() || t.iter_entries().all(|e| is_scalar(&e.value)) && t.entries.len() <= 1 {
        theme::kv(ui, name.as_deref().unwrap_or("value"), RichText::new(compact(t, lk)));
        return;
    }
    if let Some(n) = &name {
        sub(ui, &n.to_uppercase());
    }
    let array = t.iter_entries().all(|e| matches!(e.key, Some(Value::Number(_)) | None));
    let all_scalar = t.iter_entries().all(|e| is_scalar(&e.value));
    if all_scalar && (array || t.iter_entries().all(|e| matches!(e.value, Value::Bool(true)))) {
        word_list(ui, &if array { array_texts(t, lk) } else { true_keys(t) });
        return;
    }
    if all_scalar {
        let rows: Vec<Vec<String>> = t
            .iter_entries()
            .map(|e| {
                let k = key_text(e);
                vec![humanize(&k), field_text(&k, &e.value, lk)]
            })
            .collect();
        grid(ui, label.unwrap_or("map"), &["", ""], &rows);
        return;
    }
    // Records: every value a table → one row per entry, one column per field.
    if t.iter_entries().all(|e| matches!(e.value, Value::Table(_))) {
        let mut cols: Vec<String> = Vec::new();
        for e in t.iter_entries() {
            if let Value::Table(r) = &e.value {
                for f in r.iter_entries().map(key_text) {
                    if !cols.contains(&f) {
                        cols.push(f);
                    }
                }
            }
        }
        if !cols.is_empty() && cols.len() <= 8 {
            let rows: Vec<Vec<String>> = t
                .iter_entries()
                .map(|e| {
                    let Value::Table(r) = &e.value else { unreachable!() };
                    let mut row = vec![key_text(e)];
                    for c in &cols {
                        row.push(
                            r.iter_entries()
                                .find(|x| &key_text(x) == c)
                                .map(|x| field_text(c, &x.value, lk))
                                .unwrap_or_default(),
                        );
                    }
                    row
                })
                .collect();
            let headers: Vec<String> =
                std::iter::once(String::new()).chain(cols.iter().map(|c| humanize(c))).collect();
            let headers: Vec<&str> = headers.iter().map(String::as_str).collect();
            grid(ui, label.unwrap_or("records"), &headers, &rows);
            return;
        }
    }
    ui.indent(("auto", label), |ui| {
        for e in t.iter_entries() {
            let k = key_text(e);
            ui.push_id(&k, |ui| auto(ui, Some(&k), &e.value, lk));
        }
    });
}

/// `auto` for every key of `t` not in `handled` — content a topic has no dedicated layout for.
fn rest(ui: &mut egui::Ui, t: &Table, handled: &[&str], lk: Lookups) {
    for (k, v) in t.iter_str().filter(|(k, _)| !handled.contains(k)) {
        ui.push_id(k, |ui| auto(ui, Some(k), v, lk));
    }
}

// ── topics ────────────────────────────────────────────────────────────────────────────────────

fn save_summary(ui: &mut egui::Ui, ctx: &Ctx) {
    if let Some(p) = ctx.profile {
        let rows = vec![
            vec!["Hero".to_string(), crate::saves::hero_name(p.character_index).to_string()],
            vec!["Upgrade tier".to_string(), p.upgrade_index.to_string()],
            vec!["Cash".to_string(), money(p.cash as f64)],
            vec!["Fuel".to_string(), format!("{} / {}", fmt_num(p.fuel as f64), fmt_num(p.fuel_capacity as f64))],
            vec!["Contract".to_string(), p.active_contract.clone()],
            vec!["Play time".to_string(), crate::saves::hms_long(p.play_time_seconds)],
            vec!["Saved".to_string(), crate::saves::timestamp_utc(p.timestamp)],
            vec!["Slot".to_string(), p.save_name().to_string()],
            vec!["Outfits unlocked".to_string(), p.unlocked_costumes.to_string()],
        ];
        grid(ui, "save.summary", &["", ""], &rows);
    }
    if let Some(t) = num(ctx.root.get("nTimeElapsed")) {
        theme::kv(ui, "Time elapsed", RichText::new(duration(t)));
    }
}

fn story_badge(ctx: &Ctx) -> Option<String> {
    let fd = table(ctx.root.get("tFlowData"))?;
    let active = table(fd.get("tActiveMissions")).map_or(0, |t| t.entries.len());
    let beats = table(fd.get("tCulledBindings")).map_or(0, |t| t.entries.len());
    Some(format!("{beats} story beats · {active} active"))
}

/// `tFlowData`: missions in progress, the story bindings that have fired, the flow keys awarded.
fn story(ui: &mut egui::Ui, ctx: &Ctx) {
    let Some(fd) = table(ctx.root.get("tFlowData")) else { return };
    if let Some(am) = table(fd.get("tActiveMissions")) {
        sub(ui, "IN PROGRESS");
        let rows: Vec<Vec<String>> = am
            .iter_entries()
            .map(|e| {
                let m = table(Some(&e.value));
                let state = m
                    .and_then(|m| num(m.get("nState")))
                    .map(|s| crate::saves::TaskState::from_f64(s).map_or_else(|| fmt_num(s), |t| t.label().to_string()))
                    .unwrap_or_default();
                let targets = m.and_then(|m| num(m.get("_nTargetsComplete"))).map(fmt_num).unwrap_or_default();
                let collected = m.and_then(|m| table(m.get("tCollected"))).map_or(String::new(), |c| c.entries.len().to_string());
                let other = m
                    .map(|m| {
                        m.iter_entries()
                            .filter(|x| !["nState", "_nTargetsComplete", "tCollected"].contains(&key_text(x).as_str()))
                            .map(|x| format!("{} {}", humanize(&key_text(x)), value_text(&x.value, ctx.lk)))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    })
                    .unwrap_or_default();
                vec![key_text(e), state, targets, collected, other]
            })
            .collect();
        grid(ui, "story.active", &["Mission", "State", "Targets", "Collected", ""], &rows);
    }
    if let Some(cb) = table(fd.get("tCulledBindings")) {
        sub(ui, "STORY BEATS");
        ui.label(RichText::new(array_texts(cb, ctx.lk).join("  →  ")).color(theme::TX));
    }
    if let Some(keys) = table(fd.get("tMyFlowData")) {
        sub(ui, "FLOW KEYS");
        let mut words: Vec<String> = keys
            .iter_entries()
            .map(|e| match num(Some(&e.value)) {
                Some(n) if n != 1.0 => format!("{} ×{}", key_text(e), fmt_num(n)),
                _ => key_text(e),
            })
            .collect();
        words.sort();
        word_list(ui, &words);
    }
    rest(ui, fd, &["tActiveMissions", "tCulledBindings", "tMyFlowData"], ctx.lk);
}

fn world_badge(ctx: &Ctx) -> Option<String> {
    table(ctx.root.get("tLayerData")).map(|t| format!("{} layers", t.entries.len()))
}

/// `vz_state_oiljob012_x` → `Some(("OilJob012", "x"))`: the mission a layer is named after.
fn mission_of_layer(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("vz_state_")?.to_ascii_lowercase();
    let factions = ["all", "chi", "gur", "oil", "pir", "pmc", "jet", "mec", "vza"];
    let cap = |s: &str| s[..1].to_ascii_uppercase() + &s[1..];
    for f in factions {
        let Some(after) = rest.strip_prefix(f) else { continue };
        for kind in ["con", "job"] {
            let Some(digits) = after.strip_prefix(kind) else { continue };
            let n = digits.chars().take_while(|c| c.is_ascii_digit()).count();
            if n == 0 {
                continue;
            }
            let id = format!("{}{}{}", cap(f), cap(kind), &digits[..n]);
            let tail = digits[n..].trim_start_matches('_').to_string();
            return Some((id, tail));
        }
        if let Some(tail) = after.strip_prefix("intro") {
            return Some((format!("{}Intro", cap(f)), tail.trim_start_matches('_').to_string()));
        }
    }
    None
}

/// `tLayerData`: the world overlays loaded, grouped by the mission or area their names carry.
fn world(ui: &mut egui::Ui, ctx: &Ctx) {
    let Some(ld) = table(ctx.root.get("tLayerData")) else { return };
    let names: Vec<String> = array_texts(ld, ctx.lk);
    let mut missions: Vec<(String, Vec<String>)> = Vec::new();
    let mut areas: Vec<(String, Vec<String>)> = Vec::new();
    let push = |groups: &mut Vec<(String, Vec<String>)>, g: String, item: String| match groups.iter_mut().find(|(k, _)| *k == g) {
        Some((_, v)) => v.push(item),
        None => groups.push((g, vec![item])),
    };
    for n in &names {
        match mission_of_layer(n) {
            Some((id, tail)) => push(&mut missions, id, if tail.is_empty() { "—".into() } else { tail }),
            None => {
                let rest = n.strip_prefix("vz_state_").unwrap_or(n);
                let (area, tail) = rest.split_once('_').unwrap_or((rest, ""));
                push(&mut areas, area.to_string(), if tail.is_empty() { "—".into() } else { tail.to_string() });
            }
        }
    }
    missions.sort_by(|a, b| a.0.cmp(&b.0));
    areas.sort_by(|a, b| a.0.cmp(&b.0));
    let rows = |g: &[(String, Vec<String>)]| -> Vec<Vec<String>> {
        g.iter().map(|(k, v)| vec![k.clone(), v.len().to_string(), v.join(", ")]).collect()
    };
    sub(ui, &format!("FROM MISSIONS · {}", missions.len()));
    grid(ui, "world.missions", &["Mission", "Layers", ""], &rows(&missions));
    sub(ui, &format!("AREAS · {}", areas.len()));
    grid(ui, "world.areas", &["Area", "Layers", ""], &rows(&areas));
}

fn loadout_badge(ctx: &Ctx) -> Option<String> {
    let carried = table(ctx.root.get("tPlayerData"))
        .and_then(|p| p.iter_entries().next())
        .and_then(|e| table(Some(&e.value)))
        .and_then(|p| table(p.get("tEquipment")))
        .map_or(0, |t| t.entries.len());
    let support = table(ctx.root.get("vEquippedSupport")).map_or(0, |t| t.entries.len());
    Some(format!("{carried} carried · {support} support"))
}

/// `tPlayerData` (health, carried weapons) and `vEquippedSupport` (PDA support slots).
fn loadout(ui: &mut egui::Ui, ctx: &Ctx) {
    if let Some(players) = table(ctx.root.get("tPlayerData")) {
        for (i, e) in players.iter_entries().enumerate() {
            let Some(p) = table(Some(&e.value)) else { continue };
            if players.entries.len() > 1 {
                sub(ui, &format!("PLAYER {}", i + 1));
            }
            if let Some(h) = num(p.get("nHealth")) {
                theme::kv(ui, "Health", RichText::new(fmt_num(h)));
            }
            sub(ui, "CARRIED");
            let eq = table(p.get("tEquipment"));
            let any_extra = eq.is_some_and(|t| {
                t.iter_entries().any(|w| table(Some(&w.value)).is_some_and(|w| w.entries.len() > 2))
            });
            let rows: Vec<Vec<String>> = eq
                .map(|t| {
                    t.iter_entries()
                        .map(|w| {
                            let w = table(Some(&w.value));
                            let field = |n: usize| {
                                w.and_then(|w| w.entries.get(n)).map(|x| &x.value)
                            };
                            let tmpl = field(0).and_then(guid);
                            let name = tmpl
                                .and_then(|g| ctx.lk.template(g))
                                .map(str::to_string)
                                .or_else(|| tmpl.map(|g| format!("0x{g:08X}")))
                                .unwrap_or_default();
                            let mut row = vec![
                                name,
                                field(1).and_then(|v| num(Some(v))).map(fmt_num).unwrap_or_default(),
                                tmpl.map(|g| format!("0x{g:08X}")).unwrap_or_default(),
                            ];
                            if any_extra {
                                row.push(field(2).map(|v| value_text(v, ctx.lk)).unwrap_or_default());
                            }
                            row
                        })
                        .collect()
                })
                .unwrap_or_default();
            let headers: &[&str] = if any_extra {
                &["Weapon", "Reserve", "Template", "Third value"]
            } else {
                &["Weapon", "Reserve", "Template"]
            };
            grid(ui, &format!("loadout.carried.{i}"), headers, &rows);
            rest(ui, p, &["nHealth", "tEquipment"], ctx.lk);
        }
    }
    if let Some(sup) = table(ctx.root.get("vEquippedSupport")) {
        sub(ui, "SUPPORT");
        let rows: Vec<Vec<String>> = array_texts(sup, ctx.lk)
            .into_iter()
            .map(|k| vec![ctx.lk.text(&k).unwrap_or(&k).to_string(), k])
            .collect();
        grid(ui, "loadout.support", &["Support", "Key"], &rows);
    }
}

fn travel_badge(ctx: &Ctx) -> Option<String> {
    let t = table(ctx.root.get("tTransitData"))?;
    let zones: Vec<&Table> = t.iter_entries().filter_map(|e| table(Some(&e.value))).collect();
    let on = zones.iter().filter(|z| flag(z.get("bEnabled")) == Some(true)).count();
    let nuked = zones.iter().filter(|z| flag(z.get("bIsNuked")) == Some(true)).count();
    Some(format!("{on} enabled · {nuked} nuked"))
}

/// `tTransitData`: the fast-travel switch and each landing zone.
fn fast_travel(ui: &mut egui::Ui, ctx: &Ctx) {
    let Some(t) = table(ctx.root.get("tTransitData")) else { return };
    if let Some(b) = flag(t.get("bEnabled")) {
        theme::kv(ui, "Fast travel", RichText::new(if b { "on" } else { "off" }));
    }
    let yes = |z: &Table, k: &str| if flag(z.get(k)) == Some(true) { "yes" } else { "" }.to_string();
    let mut zones: Vec<(f64, Vec<String>)> = t
        .iter_entries()
        .filter_map(|e| {
            let z = table(Some(&e.value))?;
            let n = match &e.key {
                Some(Value::Number(n)) => n.value,
                _ => return None,
            };
            Some((
                n,
                vec![
                    format!("LZ {}", fmt_num(n)),
                    text(z.get("sFactionAbbrev")).unwrap_or("").to_string(),
                    yes(z, "bEnabled"),
                    yes(z, "bIsNuked"),
                    yes(z, "bHasPlayedFanfare"),
                ],
            ))
        })
        .collect();
    zones.sort_by(|a, b| a.0.total_cmp(&b.0));
    grid(
        ui,
        "travel.zones",
        &["Zone", "Owner", "Enabled", "Nuked", "Fanfare played"],
        &zones.into_iter().map(|(_, r)| r).collect::<Vec<_>>(),
    );
}

fn pmc_badge(ctx: &Ctx) -> Option<String> {
    let stock = table(ctx.root.get("tPmcData")).and_then(|t| table(t.get("tStockpile")))?;
    let held = stock
        .iter_entries()
        .filter(|e| table(Some(&e.value)).and_then(|t| num(t.get("nAmt"))).unwrap_or(0.0) > 0.0)
        .count();
    Some(format!("{held} stockpiled"))
}

/// `tPmcData` + `tPmcInteriorData` + `nAvailableCostumes`.
fn pmc(ui: &mut egui::Ui, ctx: &Ctx) {
    let lk = ctx.lk;
    let pd = table(ctx.root.get("tPmcData"));
    let pi = table(ctx.root.get("tPmcInteriorData"));
    let mut rows: Vec<Vec<String>> = Vec::new();
    if let Some(v) = pd.and_then(|t| num(t.get("nCash"))) {
        rows.push(vec!["Cash".into(), money(v)]);
    }
    if let Some(v) = pi.and_then(|t| table(t.get("tStockpileQty"))).and_then(|q| num(q.get("money"))) {
        rows.push(vec!["HQ stockpile money".into(), money(v)]);
    }
    if let Some(v) = pd.and_then(|t| num(t.get("nFuel"))) {
        rows.push(vec!["Fuel".into(), fmt_num(v)]);
    }
    if let Some(v) = num(ctx.root.get("nAvailableCostumes")) {
        rows.push(vec!["Available costumes".into(), fmt_num(v)]);
    }
    if let Some(b) = pi.and_then(|t| flag(t.get("bUnlocked"))) {
        rows.push(vec!["HQ unlocked".into(), if b { "yes" } else { "no" }.into()]);
    }
    grid(ui, "pmc.summary", &["", ""], &rows);
    if let Some(pi) = pi {
        if let Some(t) = table(pi.get("tIntroduced")) {
            sub(ui, "RECRUITS INTRODUCED");
            word_list(ui, &true_keys(t));
        }
        if let Some(q) = table(pi.get("tStockpileQty")) {
            sub(ui, "HQ STOCKPILE");
            let rows: Vec<Vec<String>> = q
                .iter_str()
                .filter(|(k, _)| *k != "money")
                .map(|(k, v)| vec![lk.support(k).unwrap_or(k).to_string(), value_text(v, lk)])
                .collect();
            grid(ui, "pmc.hq", &["Item", "Quantity"], &rows);
        }
        rest(ui, pi, &["bUnlocked", "tIntroduced", "tStockpileQty"], lk);
    }
    if let Some(pd) = pd {
        if let Some(t) = table(pd.get("tEquipment")) {
            sub(ui, "FUEL TANKS OWNED");
            word_list(ui, &true_keys(t));
        }
        if let Some(s) = table(pd.get("tStockpile")) {
            let mut held: Vec<Vec<String>> = Vec::new();
            let mut empty: Vec<String> = Vec::new();
            for (id, v) in s.iter_str() {
                let t = table(Some(v));
                let amt = t.and_then(|t| num(t.get("nAmt"))).unwrap_or(0.0);
                let new = t.and_then(|t| flag(t.get("bNew"))).unwrap_or(false);
                let name = lk.support(id).unwrap_or(id).to_string();
                if amt > 0.0 {
                    held.push(vec![name, fmt_num(amt), if new { "new".into() } else { String::new() }]);
                } else {
                    empty.push(name);
                }
            }
            sub(ui, "STOCKPILE");
            grid(ui, "pmc.stockpile", &["Item", "Amount", ""], &held);
            sub(ui, &format!("NONE IN STOCK · {}", empty.len()));
            word_list(ui, &empty);
        }
        if let Some(t) = table(pd.get("tFreebies")) {
            sub(ui, "FREEBIES");
            let rows: Vec<Vec<String>> = t
                .iter_entries()
                .map(|e| {
                    let k = key_text(e);
                    vec![lk.text(&k).or_else(|| lk.support(&k)).unwrap_or(&k).to_string(), value_text(&e.value, lk)]
                })
                .collect();
            grid(ui, "pmc.freebies", &["Item", ""], &rows);
        }
        rest(ui, pd, &["nCash", "nFuel", "tEquipment", "tStockpile", "tFreebies"], lk);
    }
}

fn factions_badge(ctx: &Ctx) -> Option<String> {
    let rel = table(ctx.root.get("tFactionData")).and_then(|t| table(t.get("tRelations")))?;
    Some(format!("{} factions", rel.entries.len()))
}

/// `tFactionData`: the relations grid, civilian casualties and penalty, mutable factions.
fn factions(ui: &mut egui::Ui, ctx: &Ctx) {
    let Some(fd) = table(ctx.root.get("tFactionData")) else { return };
    if let Some(rel) = table(fd.get("tRelations")) {
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
        egui::ScrollArea::horizontal().id_source("factions.grid").show(ui, |ui| {
            egui::Grid::new("factions.grid.cells").striped(true).spacing([8.0, 2.0]).show(ui, |ui| {
                ui.label(RichText::new("from \\ to").small().color(theme::FAINT));
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
    let mut rows: Vec<Vec<String>> = Vec::new();
    if let Some(v) = num(fd.get("nCivilianCasualties")) {
        rows.push(vec!["Civilian casualties".into(), fmt_num(v)]);
    }
    if let Some(v) = num(fd.get("nCivilianPenalty")) {
        rows.push(vec!["Civilian penalty".into(), fmt_num(v)]);
    }
    grid(ui, "factions.civ", &["", ""], &rows);
    if let Some(t) = table(fd.get("tMutableFactions")) {
        sub(ui, "STANDING CAN CHANGE");
        word_list(ui, &array_texts(t, ctx.lk));
    }
    rest(ui, fd, &["tRelations", "nCivilianCasualties", "nCivilianPenalty", "tMutableFactions"], ctx.lk);
}

fn shops_badge(ctx: &Ctx) -> Option<String> {
    table(ctx.root.get("tShopData")).map(|t| format!("{} shops", t.entries.len()))
}

/// `tShopData`: per faction shop, items bought of its total.
fn shops(ui: &mut egui::Ui, ctx: &Ctx) {
    let Some(sd) = table(ctx.root.get("tShopData")) else { return };
    let lk = ctx.lk;
    let rows: Vec<Vec<String>> = sd
        .iter_str()
        .map(|(faction, v)| {
            let shop = table(Some(v));
            let bought = shop.and_then(|s| table(s.get("tPurchased")));
            let n = |k: &str| bought.and_then(|b| num(b.get(k))).unwrap_or(0.0);
            let names = |k: &str| -> String {
                bought
                    .and_then(|b| table(b.get(k)))
                    .map(|t| {
                        true_keys(t)
                            .iter()
                            .map(|id| lk.support(id).unwrap_or(id).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default()
            };
            vec![
                faction.to_string(),
                fmt_num(n("nEquipment") + n("nSupport")),
                shop.and_then(|s| num(s.get("nTotalItems"))).map(fmt_num).unwrap_or_default(),
                names("tSupport"),
                names("tEquipment"),
            ]
        })
        .collect();
    grid(ui, "shops", &["Shop", "Bought", "Items", "Support bought", "Equipment bought"], &rows);
}

fn arsenal_badge(ctx: &Ctx) -> Option<String> {
    let n = table(ctx.root.get("tSupportData")).map_or(0, |t| t.iter_str().filter(|(k, _)| !k.starts_with('_')).count())
        + table(ctx.root.get("tWifEquipmentData")).map_or(0, |t| t.entries.len());
    Some(format!("{n} items"))
}

/// `tSupportData` (support/vehicle unlocks, `_tRequirementsObtained`) and `tWifEquipmentData`.
/// Unlock values show as saved; their meaning is not established.
fn arsenal(ui: &mut egui::Ui, ctx: &Ctx) {
    const REQUIREMENTS: &str = "_tRequirementsObtained";
    let lk = ctx.lk;
    let per_faction = |t: &Table| t.iter_str().map(|(f, s)| format!("{f} {}", value_text(s, lk))).collect::<Vec<_>>().join(" · ");
    let sup = table(ctx.root.get("tSupportData"));
    if let Some(r) = sup.and_then(|s| s.get(REQUIREMENTS)) {
        sub(ui, "REQUIREMENTS OBTAINED");
        match table(Some(r)) {
            Some(t) => word_list(ui, &true_keys(t)),
            None => word_list(ui, &[value_text(r, lk)]),
        }
    }
    if let Some(sup) = sup {
        let mut rows: Vec<Vec<String>> = sup
            .iter_str()
            .filter(|(id, _)| *id != REQUIREMENTS)
            .map(|(id, v)| {
                let t = table(Some(v));
                let unlock = t.and_then(|t| table(t.get("tUnlockStatus"))).map(per_faction).unwrap_or_default();
                let other = t
                    .map(|t| {
                        t.iter_str()
                            .filter(|(k, _)| *k != "tUnlockStatus")
                            .map(|(k, x)| format!("{} {}", humanize(k).to_lowercase(), value_text(x, lk)))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    })
                    .unwrap_or_default();
                vec![lk.support(id).unwrap_or(id).to_string(), unlock, other, id.to_string()]
            })
            .collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));
        sub(ui, "SUPPORT & VEHICLES");
        grid(ui, "arsenal.support", &["Item", "Unlock", "", "Id"], &rows);
    }
    if let Some(eq) = table(ctx.root.get("tWifEquipmentData")) {
        let rows: Vec<Vec<String>> = eq
            .iter_str()
            .map(|(id, v)| vec![id.to_string(), table(Some(v)).map(per_faction).unwrap_or_else(|| value_text(v, lk))])
            .collect();
        sub(ui, "EQUIPMENT");
        grid(ui, "arsenal.equipment", &["Item", "Unlock"], &rows);
    }
}

fn contacts_badge(ctx: &Ctx) -> Option<String> {
    table(ctx.root.get("tStarterData")).map(|t| format!("{} contacts", t.entries.len()))
}

/// `tStarterData`, `tRewardData`, `sLastMissionName`, `tRetryLocations`, `tBoundaryData`,
/// `tLockedGates`.
fn contacts(ui: &mut egui::Ui, ctx: &Ctx) {
    let root = ctx.root;
    let lk = ctx.lk;
    let mut rows: Vec<Vec<String>> = Vec::new();
    if let Some(s) = text(root.get("sLastMissionName")) {
        rows.push(vec!["Last contract completed".into(), s.to_string()]);
    }
    rows.push(vec![
        "Retry checkpoint".into(),
        match root.get("tRetryLocations") {
            Some(v) => value_text(v, lk),
            None => "none".into(),
        },
    ]);
    if let Some(b) = table(root.get("tBoundaryData")).and_then(|b| text(b.get("sBoundaryName"))) {
        rows.push(vec!["Boundary".into(), b.to_string()]);
    }
    if let Some(g) = table(root.get("tLockedGates")) {
        rows.push(vec!["Locked gates".into(), compact(g, lk)]);
    }
    grid(ui, "contacts.summary", &["", ""], &rows);
    if let Some(b) = table(root.get("tBoundaryData")) {
        rest(ui, b, &["sBoundaryName"], lk);
    }
    if let Some(r) = table(root.get("tRewardData")) {
        sub(ui, "WAGERS");
        auto(ui, None, &Value::Table(r.clone()), lk);
    }
    if let Some(st) = table(root.get("tStarterData")) {
        sub(ui, "CONTACTS");
        let rows: Vec<Vec<String>> = st
            .iter_str()
            .map(|(who, v)| {
                let t = table(Some(v));
                let yes = |k: &str| if t.and_then(|t| flag(t.get(k))) == Some(true) { "yes" } else { "" }.to_string();
                let briefings = t.and_then(|t| table(t.get("tOldBriefings"))).map(|b| true_keys(b).join(", ")).unwrap_or_default();
                let other = t
                    .map(|t| {
                        t.iter_str()
                            .filter(|(k, _)| !["bCardDisplayed", "bFanfareDisplayed", "tOldBriefings"].contains(k))
                            .map(|(k, x)| format!("{} {}", humanize(k), value_text(x, lk)))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    })
                    .unwrap_or_default();
                vec![who.to_string(), yes("bCardDisplayed"), yes("bFanfareDisplayed"), briefings, other]
            })
            .collect();
        grid(ui, "contacts.starters", &["Contact", "Card shown", "Fanfare played", "Briefings", ""], &rows);
    }
}

/// `tStatsData`, except `oVerifyData` (HIGH-VALUE TARGETS).
fn stats(ui: &mut egui::Ui, ctx: &Ctx) {
    let Some(st) = table(ctx.root.get("tStatsData")) else { return };
    let lk = ctx.lk;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (k, v) in st.iter_str() {
        let Some(n) = num(Some(v)) else { continue };
        let shown = if k == "nCompletedToolboxes" {
            format!("{} / {}", fmt_num(n), mercs2_formats::save::TOTAL_TOOLBOXES)
        } else {
            field_text(k, v, lk)
        };
        rows.push(vec![humanize(k), shown]);
    }
    grid(ui, "stats.counters", &["", ""], &rows);
    let timed = |ui: &mut egui::Ui, title: &str, key: &str, header: &str| {
        if let Some(t) = table(st.get(key)) {
            sub(ui, title);
            let mut rows: Vec<(f64, Vec<String>)> = t
                .iter_str()
                .map(|(k, v)| {
                    let s = num(Some(v)).unwrap_or(0.0);
                    (s, vec![lk.text(k).unwrap_or(k).to_string(), duration(s)])
                })
                .collect();
            rows.sort_by(|a, b| b.0.total_cmp(&a.0));
            grid(ui, key, &[header, "Time"], &rows.into_iter().map(|(_, r)| r).collect::<Vec<_>>());
        }
    };
    timed(ui, "FAVOURITE WEAPONS", "tFavWeapon", "Weapon");
    timed(ui, "FAVOURITE VEHICLES", "tFavVehicle", "Vehicle");
    timed(ui, "RACES", "tListOfRaces", "Race");
    let money_list = |ui: &mut egui::Ui, title: &str, key: &str| {
        if let Some(t) = table(st.get(key)) {
            sub(ui, title);
            let rows: Vec<Vec<String>> = t
                .iter_str()
                .map(|(k, v)| vec![lk.text(k).unwrap_or(k).to_string(), num(Some(v)).map(money).unwrap_or_else(|| value_text(v, lk))])
                .collect();
            grid(ui, key, &["", "Amount"], &rows);
        }
    };
    money_list(ui, "CREDITS", "tListOfCredits");
    money_list(ui, "DEBITS", "tListOfDebits");
    if let Some(t) = table(st.get("tDestroyBty")) {
        sub(ui, "DESTROY BOUNTIES");
        let rows: Vec<Vec<String>> = t.iter_str().map(|(k, v)| vec![k.to_string(), value_text(v, lk)]).collect();
        grid(ui, "stats.bounties", &["Faction", "Destroyed"], &rows);
    }
    let handled: Vec<&str> = st
        .iter_str()
        .filter(|(_, v)| num(Some(v)).is_some())
        .map(|(k, _)| k)
        .chain(["tFavWeapon", "tFavVehicle", "tListOfRaces", "tListOfCredits", "tListOfDebits", "tDestroyBty", "oVerifyData"])
        .collect();
    rest(ui, st, &handled, lk);
}

fn verify<'a>(ctx: &Ctx<'a>) -> Option<&'a Table> {
    table(ctx.root.get("tStatsData")).and_then(|t| table(t.get("oVerifyData")))
}

fn targets_badge(ctx: &Ctx) -> Option<String> {
    let lists = verify(ctx).and_then(|v| table(v.get("tTargetListStatus")))?;
    let n: usize = lists.iter_str().map(|(_, v)| table(Some(v)).map_or(0, |t| t.entries.len())).sum();
    Some(format!("{n} targets"))
}

/// `tStatsData.oVerifyData`: Solano and each faction's high-value targets.
fn targets(ui: &mut egui::Ui, ctx: &Ctx) {
    let Some(vd) = verify(ctx) else { return };
    if let Some(s) = text(vd.get("sSolanoStatus")) {
        theme::kv(ui, "Solano", RichText::new(if s.is_empty() { "—" } else { s }));
    }
    if let Some(l) = table(vd.get("tTargetListStatus")) {
        for (faction, v) in l.iter_str() {
            let Some(t) = table(Some(v)) else { continue };
            let done = t.iter_str().filter(|(_, s)| !text(Some(s)).unwrap_or("").is_empty()).count();
            sub(ui, &format!("{} · {done} OF {}", faction.to_uppercase(), t.entries.len()));
            let mut rows: Vec<Vec<String>> = t
                .iter_str()
                .map(|(k, s)| {
                    let fate = text(Some(s)).unwrap_or("");
                    vec![k.to_string(), if fate.is_empty() { "—".into() } else { fate.to_string() }]
                })
                .collect();
            rows.sort();
            grid(ui, &format!("targets.{faction}"), &["Target", "Fate"], &rows);
        }
    }
    rest(ui, vd, &["sSolanoStatus", "tTargetListStatus"], ctx.lk);
}

/// Gameplay switches, bios, tutorials, hints, munitions hints, achievement tracking.
fn progress(ui: &mut egui::Ui, ctx: &Ctx) {
    let root = ctx.root;
    let lk = ctx.lk;
    let rows: Vec<Vec<String>> = ["bGrappleEnabled", "bVehicleDisguiseEnabled", "bResourceCountersEnabled"]
        .iter()
        .filter_map(|k| flag(root.get(k)).map(|b| vec![humanize(k), if b { "on" } else { "off" }.to_string()]))
        .collect();
    grid(ui, "progress.switches", &["", ""], &rows);
    if let Some(t) = table(root.get("tActiveBio")) {
        let on: Vec<String> = true_keys(t).into_iter().map(|k| k.strip_prefix("Bio").unwrap_or(&k).to_string()).collect();
        sub(ui, &format!("BIOS UNLOCKED · {}", on.len()));
        word_list(ui, &on);
    }
    if let Some(t) = table(root.get("tTutorialData")) {
        let seen = array_texts(t, lk);
        sub(ui, &format!("TUTORIALS SEEN · {}", seen.len()));
        word_list(ui, &seen);
    }
    if let Some(t) = table(root.get("tActiveHint")) {
        sub(ui, "HINTS QUEUED");
        let rows: Vec<Vec<String>> = t
            .iter_str()
            .map(|(who, v)| vec![who.to_string(), table(Some(v)).map(|h| array_texts(h, lk).join(", ")).unwrap_or_default()])
            .collect();
        grid(ui, "progress.hints", &["Character", "Hints"], &rows);
    }
    if let Some(t) = table(root.get("tMunitionsData")) {
        sub(ui, "MUNITIONS");
        auto(ui, None, &Value::Table(t.clone()), lk);
    }
    if let Some(t) = table(root.get("tMrxAchievementsData")) {
        sub(ui, "ACHIEVEMENT TRACKING");
        auto(ui, None, &Value::Table(t.clone()), lk);
    }
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

    /// Every top-level table of every fixture save belongs to a topic, so Inspect shows all of it.
    #[test]
    fn every_fixture_top_level_key_has_a_topic() {
        for name in game_paths::SAVE_FIXTURES {
            let root = tree_of(name);
            let t = root.as_table().unwrap();
            assert!(t.entries.len() > 5, "{name}: suspiciously few top-level keys");
            assert_eq!(unclaimed_keys(t), Vec::<&str>::new(), "{name}");
        }
    }

    /// Top-level keys the panel shows outside its topics: the centre's PROGRESSION tabs
    /// (`tFlowData`, `tLayerData`) and METADATA (`nTimeElapsed`).
    const PANEL_ELSEWHERE: &[&str] = &["tFlowData", "tLayerData", "nTimeElapsed"];

    /// The panel shows every fixture key too: through its topics or its other sections.
    #[test]
    fn the_panel_covers_every_fixture_key() {
        for name in game_paths::SAVE_FIXTURES {
            let root = tree_of(name);
            for (k, _) in root.as_table().unwrap().iter_str() {
                let shown = PANEL_ELSEWHERE.contains(&k) || TOPICS.iter().any(|t| t.panel && t.keys.contains(&k));
                assert!(shown, "{name}: the panel does not show {k}");
            }
        }
    }

    #[test]
    fn each_key_belongs_to_one_topic() {
        let mut seen: Vec<&str> = Vec::new();
        for t in TOPICS {
            for k in t.keys {
                assert!(!seen.contains(k), "{k} is in two topics");
                seen.push(k);
            }
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
        assert_eq!(humanize("_nTargetsComplete"), "Targets complete");
        assert_eq!(humanize("VehicleDisguise"), "VehicleDisguise", "identifiers stay verbatim");
        assert_eq!(humanize("PmcCon033"), "PmcCon033");
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
    fn fields_format_by_meaning() {
        let v = save_lua::parse_value("10000.000000").unwrap();
        assert_eq!(field_text("nWager", &v, Lookups::default()), "$10,000");
        assert_eq!(field_text("nRetries", &v, Lookups::default()), "10,000");
        let t = save_lua::parse_value(r#"{ ["Oil"] = 2.000000, ["Chi"] = 1.000000, }"#).unwrap();
        assert_eq!(value_text(&t, Lookups::default()), "Oil 2 · Chi 1");
    }

    #[test]
    fn layer_names_group_by_mission() {
        assert_eq!(mission_of_layer("vz_state_oiljob012_02"), Some(("OilJob012".into(), "02".into())));
        assert_eq!(mission_of_layer("vz_state_PmcCon004_AlliesNuked"), Some(("PmcCon004".into(), "alliesnuked".into())));
        assert_eq!(mission_of_layer("vz_state_gurintro"), Some(("GurIntro".into(), String::new())));
        assert_eq!(mission_of_layer("vz_state_car_city_act2chi"), None);
    }

    #[test]
    fn guid_calls_decode() {
        let v = save_lua::parse_value("Sys.StringToGuid( '0x80005FD3' )").unwrap();
        assert_eq!(guid(&v), Some(0x8000_5FD3));
    }

    /// Every topic body and the generic renderer run over every fixture save without panicking.
    #[test]
    fn every_topic_renders_every_fixture() {
        let ctx_egui = egui::Context::default();
        crate::gui::theme::install(&ctx_egui);
        for name in game_paths::SAVE_FIXTURES {
            let root = tree_of(name);
            let root = root.as_table().unwrap();
            let profile = save::parse(&std::fs::read(game_paths::save_fixtures().join(name)).unwrap()).unwrap();
            let _ = ctx_egui.run(egui::RawInput::default(), |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    let ctx = Ctx { root, profile: Some(&profile), lk: Lookups::default() };
                    for t in TOPICS {
                        (t.body)(ui, &ctx);
                        let _ = (t.badge)(&ctx);
                    }
                    other(ui, &ctx);
                });
            });
        }
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
