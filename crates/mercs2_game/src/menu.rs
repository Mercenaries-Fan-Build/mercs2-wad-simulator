//! GAME shell menu — main menu + save selection (policy layer over the engine's `ui` mechanism).
//!
//! The retail front-end is a Lua shell state machine driving a Scaleform movie (`shell.gfx`):
//! Lua calls `ChangeShellState("newGame"|...)` and the engine tears screens up/down
//! (docs/ui/main_menu_structure.md, docs/ui/shell_menu_lua_anatomy.md). This module is that state
//! machine reimplemented natively, with the SAME option identity set as the retail menu strings
//! (`autoContinue` / `newGame` / load / `quitGame`); the Scaleform movie is replaced by the
//! engine's 2D UI pass until a GFx/SWF player exists. Save enumeration mirrors the retail
//! multi-slot profile manager (`getListProfiles` / `addSaveGame` over `SaveGames\*.profile`).
//!
//! Screens: `Main` → (`Load` → boot slot) | boot newest | boot new-game | quit.

use std::path::PathBuf;

use mercs2_engine::scene::Scene;
use mercs2_formats::save;

/// One entry of the save browser: the `.profile` header summary (cheap parse — the zlib Lua
/// payload is only decompressed for the slot the player actually boots).
pub struct SaveSlot {
    pub path: PathBuf,
    /// Slot label from the header (autosave label, e.g. `auto_634304EA`).
    pub name: String,
    /// Active/last contract id (`PmcCon001`, ...).
    pub contract: String,
    pub play_time_seconds: u32,
    pub cash: u32,
    /// Unix save timestamp from the header (FACT field @0x24).
    pub timestamp: u32,
    /// Hero index @0x4D (1 mattias / 2 chris / 3 jen) — see `crate::hero`.
    pub character_index: u8,
    /// Hero upgrade tier @0x4F (0..3) — drives the look via the upgrade template.
    pub upgrade_index: u8,
}

/// Enumerate every readable `.profile` in `dir`, newest (header timestamp) first — the retail
/// profile-manager list. Unreadable/corrupt files are skipped (retail shows `hasCorruptedSave`;
/// we log and drop for now).
pub fn scan_slots(dir: Option<PathBuf>) -> Vec<SaveSlot> {
    let Some(dir) = dir else { return Vec::new() };
    let mut slots: Vec<SaveSlot> = Vec::new();
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = e.path();
        let is_profile = path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("profile"));
        if !is_profile {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        match save::parse(&bytes) {
            Ok(p) => slots.push(SaveSlot {
                name: p.save_name().to_string(),
                contract: p.active_contract().to_string(),
                play_time_seconds: p.play_time_seconds,
                cash: p.cash,
                timestamp: p.timestamp,
                character_index: p.character_index,
                upgrade_index: p.upgrade_index,
                path,
            }),
            Err(err) => eprintln!("[shell] skipping corrupt save {}: {err}", path.display()),
        }
    }
    slots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    slots
}

/// What the shell asks the game loop to do (the native `ChangeShellState` verbs).
#[derive(Clone, PartialEq, Debug)]
pub enum MenuAction {
    None,
    /// Boot the world from a save (`Some(path)`) or as a fresh new game (`None`).
    Boot(Option<PathBuf>),
    /// Boot the controlled **integration test world** — the real game loop (render + UI + input + AI)
    /// over a scripted scenario (player + armed hostile NPC + turret) so the AI/combat integrations can
    /// be exercised and watched live. Not a save, not a CLI flag — a first-class menu entry.
    BootTestWorld,
    Quit,
}

/// Navigation verbs, source-agnostic: keyboard events and edge-detected gamepad actions both
/// funnel into `nav`.
#[derive(Clone, Copy, PartialEq)]
pub enum Nav {
    Up,
    Down,
    Select,
    Back,
}

/// Main-menu rows. Identity follows the retail .rdata option identifiers.
#[derive(Clone, Copy, PartialEq)]
enum MainOpt {
    Continue,  // `autoContinue` — boot the newest save
    NewGame,   // `newGame`
    LoadGame,  // load / `disableLoad` gate in retail
    TestWorld, // reimpl-only: the AI/combat integration test world
    Quit,      // `quitGame`
}

#[derive(Clone, Copy)]
enum Screen {
    Main { sel: usize },
    Load { sel: usize, scroll: usize },
}

pub struct Menu {
    slots: Vec<SaveSlot>,
    screen: Screen,
}

/// Rows visible at once in the save browser before it scrolls.
const LOAD_ROWS: usize = 8;

// Palette: the plate's frame gold (sampled in loading.wgsl) + neutral greys.
const GOLD: [f32; 4] = [0.87, 0.667, 0.192, 1.0];
const WHITE: [f32; 4] = [0.92, 0.93, 0.95, 1.0];
const GREY: [f32; 4] = [0.55, 0.57, 0.60, 1.0];
const DARK: [f32; 4] = [0.0, 0.0, 0.0, 0.55];

impl Menu {
    pub fn new(slots: Vec<SaveSlot>) -> Menu {
        Menu { slots, screen: Screen::Main { sel: 0 } }
    }

    /// The main rows in display order, honouring the retail gates: Continue/Load only exist when
    /// there is at least one save (retail `disableLoad`/`autoContinue` visibility).
    fn main_opts(&self) -> Vec<MainOpt> {
        if self.slots.is_empty() {
            vec![MainOpt::NewGame, MainOpt::TestWorld, MainOpt::Quit]
        } else {
            vec![MainOpt::Continue, MainOpt::NewGame, MainOpt::LoadGame, MainOpt::TestWorld, MainOpt::Quit]
        }
    }

    pub fn nav(&mut self, nav: Nav) -> MenuAction {
        match self.screen {
            Screen::Main { sel } => {
                let opts = self.main_opts();
                match nav {
                    Nav::Up => self.screen = Screen::Main { sel: (sel + opts.len() - 1) % opts.len() },
                    Nav::Down => self.screen = Screen::Main { sel: (sel + 1) % opts.len() },
                    Nav::Back => return MenuAction::Quit,
                    Nav::Select => match opts[sel] {
                        MainOpt::Continue => return MenuAction::Boot(Some(self.slots[0].path.clone())),
                        MainOpt::NewGame => return MenuAction::Boot(None),
                        MainOpt::LoadGame => self.screen = Screen::Load { sel: 0, scroll: 0 },
                        MainOpt::TestWorld => return MenuAction::BootTestWorld,
                        MainOpt::Quit => return MenuAction::Quit,
                    },
                }
            }
            Screen::Load { sel, scroll } => match nav {
                Nav::Up => {
                    let sel = (sel + self.slots.len() - 1) % self.slots.len();
                    let scroll = if sel < scroll {
                        sel
                    } else if sel >= scroll + LOAD_ROWS {
                        // wrapped from row 0 to the end
                        self.slots.len().saturating_sub(LOAD_ROWS)
                    } else {
                        scroll
                    };
                    self.screen = Screen::Load { sel, scroll };
                }
                Nav::Down => {
                    let sel = (sel + 1) % self.slots.len();
                    let scroll = if sel >= scroll + LOAD_ROWS {
                        sel + 1 - LOAD_ROWS
                    } else if sel < scroll {
                        0 // wrapped back to the top
                    } else {
                        scroll
                    };
                    self.screen = Screen::Load { sel, scroll };
                }
                Nav::Select => return MenuAction::Boot(Some(self.slots[sel].path.clone())),
                Nav::Back => self.screen = Screen::Main { sel: 0 },
            },
        }
        MenuAction::None
    }

    /// Stage this frame's menu overlay into the scene's UI pass (drawn by `render_menu`).
    /// Layout is computed from the CURRENT surface size; `scale` tracks window height so the
    /// shell reads the same at 720p and 4K.
    pub fn draw(&self, scene: &mut Scene, t: f32) {
        let (w, h) = (scene.size.width as f32, scene.size.height as f32);
        let s = (h / 720.0).max(1.0).floor(); // integer glyph scale — crisp pixel font
        let cell = 8.0 * s;
        let center = |scene: &mut Scene, y: f32, scale: f32, color: [f32; 4], text: &str| {
            let tw = mercs2_engine::ui::UiPass::text_width(text, scale);
            scene.ui_text((w - tw) * 0.5, y, scale, color, text);
        };

        // Title block (text stand-in for the retail Scaleform title art).
        center(scene, h * 0.16, 4.0 * s, GOLD, "MERCENARIES 2");
        center(scene, h * 0.16 + 4.0 * cell + 6.0, 1.5 * s, WHITE, "W O R L D   I N   F L A M E S");

        match &self.screen {
            Screen::Main { sel } => {
                let opts = self.main_opts();
                let y0 = h * 0.46;
                let row_h = 3.2 * cell;
                for (i, opt) in opts.iter().enumerate() {
                    let label = match opt {
                        MainOpt::Continue => "CONTINUE".to_string(),
                        MainOpt::NewGame => "NEW GAME".to_string(),
                        MainOpt::LoadGame => "LOAD GAME".to_string(),
                        MainOpt::TestWorld => "TEST WORLD".to_string(),
                        MainOpt::Quit => "QUIT".to_string(),
                    };
                    let y = y0 + i as f32 * row_h;
                    let active = i == *sel;
                    // Selection backing bar + pulse, behind the row text.
                    if active {
                        let pulse = 0.35 + 0.15 * (t * 3.0).sin();
                        scene.ui_rect(w * 0.30, y - 0.6 * cell, w * 0.40, 2.4 * cell, [0.0, 0.0, 0.0, pulse + 0.25]);
                        scene.ui_rect(w * 0.30, y + 1.8 * cell, w * 0.40, s, GOLD);
                    }
                    let color = if active { GOLD } else { WHITE };
                    center(scene, y, 2.0 * s, color, &label);
                    // Continue shows WHICH save it resumes (the newest), like the retail
                    // continue tooltip.
                    if *opt == MainOpt::Continue && active {
                        if let Some(top) = self.slots.first() {
                            let sub = format!(
                                "{}  -  {}  -  {} [{}]",
                                top.name,
                                top.contract,
                                crate::hero::hero(top.character_index).display,
                                crate::hero::look_label(top.character_index, top.upgrade_index, 0)
                            );
                            center(scene, y + 2.6 * cell, 1.0 * s, GREY, &sub);
                        }
                    }
                }
                center(scene, h - 3.0 * cell, 1.0 * s, GREY, "UP/DOWN select   ENTER confirm   ESC quit");
            }
            Screen::Load { sel, scroll } => {
                center(scene, h * 0.34, 2.0 * s, WHITE, "LOAD GAME");
                let y0 = h * 0.40;
                let row_h = 2.6 * cell;
                // Panel behind the list.
                scene.ui_rect(w * 0.14, y0 - cell, w * 0.72, row_h * LOAD_ROWS.min(self.slots.len()) as f32 + 2.0 * cell, DARK);
                let end = (*scroll + LOAD_ROWS).min(self.slots.len());
                for (row, i) in (*scroll..end).enumerate() {
                    let slot = &self.slots[i];
                    let y = y0 + row as f32 * row_h;
                    let active = i == *sel;
                    if active {
                        scene.ui_rect(w * 0.15, y - 0.4 * cell, w * 0.70, 1.8 * cell, [0.87, 0.667, 0.192, 0.18]);
                    }
                    let color = if active { GOLD } else { WHITE };
                    let hours = slot.play_time_seconds / 3600;
                    let mins = (slot.play_time_seconds % 3600) / 60;
                    // Hero + look from the header (the "chosen character" of this save).
                    let who = format!(
                        "{}/{}",
                        crate::hero::hero(slot.character_index).base,
                        crate::hero::look_label(slot.character_index, slot.upgrade_index, 0)
                    );
                    let line = format!(
                        "{:<16} {:<16} {:<12} {:>4}:{:02}  ${:<11} {}",
                        trunc(&slot.name, 16),
                        trunc(&who, 16),
                        trunc(&slot.contract, 12),
                        hours,
                        mins,
                        group_thousands(slot.cash),
                        date_ymd(slot.timestamp),
                    );
                    scene.ui_text(w * 0.16, y, 1.25 * s, color, &line);
                    if active {
                        scene.ui_text(w * 0.16, y, 1.25 * s, GOLD, ">");
                    }
                }
                if self.slots.len() > LOAD_ROWS {
                    let more = format!("{} / {} saves", *sel + 1, self.slots.len());
                    center(scene, y0 + LOAD_ROWS as f32 * row_h + cell, 1.0 * s, GREY, &more);
                }
                center(scene, h - 3.0 * cell, 1.0 * s, GREY, "ENTER load   ESC back");
            }
        }
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}..", s.chars().take(n.saturating_sub(2)).collect::<String>())
    }
}

fn group_thousands(v: u32) -> String {
    let raw = v.to_string();
    let mut out = String::new();
    for (i, c) in raw.chars().enumerate() {
        if i > 0 && (raw.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Unix timestamp -> `YYYY-MM-DD` (civil-from-days, Howard Hinnant's algorithm) — dependency-free.
fn date_ymd(ts: u32) -> String {
    let days = (ts / 86400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_ymd_known_values() {
        assert_eq!(date_ymd(0), "1970-01-01");
        assert_eq!(date_ymd(1_214_870_400), "2008-07-01"); // retail-era save
        assert_eq!(date_ymd(1_780_531_200), "2026-06-04");
    }

    #[test]
    fn main_menu_gates_on_saves() {
        // No saves: Continue/Load hidden → rows are [NEW GAME, TEST WORLD, QUIT].
        let mut m = Menu::new(Vec::new());
        assert_eq!(m.nav(Nav::Select), MenuAction::Boot(None)); // NEW GAME
        assert_eq!(m.nav(Nav::Down), MenuAction::None);
        assert_eq!(m.nav(Nav::Select), MenuAction::BootTestWorld); // TEST WORLD
        assert_eq!(m.nav(Nav::Down), MenuAction::None);
        assert_eq!(m.nav(Nav::Select), MenuAction::Quit); // QUIT
    }

    #[test]
    fn load_screen_selects_a_slot() {
        let slot = |n: &str, ts: u32| SaveSlot {
            path: PathBuf::from(format!("{n}.profile")),
            name: n.to_string(),
            contract: "PmcCon001".into(),
            play_time_seconds: 3600,
            cash: 1_000_000,
            timestamp: ts,
            character_index: 1,
            upgrade_index: 0,
        };
        let mut m = Menu::new(vec![slot("a", 10), slot("b", 20)]);
        // slots arrive pre-sorted by scan_slots; Menu preserves order. Continue = slots[0].
        assert_eq!(m.nav(Nav::Select), MenuAction::Boot(Some(PathBuf::from("a.profile"))));
        // Down x2 -> LOAD GAME, enter it, pick the second row.
        m.nav(Nav::Down);
        m.nav(Nav::Down);
        assert_eq!(m.nav(Nav::Select), MenuAction::None); // entered Load screen
        m.nav(Nav::Down);
        assert_eq!(m.nav(Nav::Select), MenuAction::Boot(Some(PathBuf::from("b.profile"))));
        // Back returns to Main, then Back again quits.
        let mut m2 = Menu::new(Vec::new());
        assert_eq!(m2.nav(Nav::Back), MenuAction::Quit);
    }
}
