//! `mission_lab` — the OTHER half of the Mercenaries 2 Lua API.
//!
//! Object scripts (see `lua_lab`) are free functions the engine calls by name. Missions,
//! contracts and objectives are a completely different animal: they are **task objects** built on
//! a real inheritance chain, they carry `self`, and the framework does a lot of work for you —
//! *if* you cooperate with it, and silently nothing if you don't.
//!
//! ```text
//! cargo run -p mercs2_script --example mission_lab -- 01_you_forgot_the_parent
//! ```
//!
//! Unlike a mock, this drives the **real** `MrxTask` base class decompiled out of the shipped game
//! (`docs/mercs2-luacd/src/resident/mrxtask.lua`), through its real entry points:
//!
//! ```text
//! MrxTask.Create(m) → Configure(tConfig) → Activate()
//!    → dynamic_import(sModuleName, self._ModuleLoaded, {self})
//!    → _ModuleLoaded → setmetatable(self, {__index = your module}) → _tEvents = {}
//!    → PreLoadAssets → LoadAssets → AssetsLoaded → Activated(self)   ← YOUR CODE
//! ...
//! Complete()/Cancel() → Cleanup() → Event.Delete(every handle in self._tEvents)
//!                                 → MarkForRemoval(tConfig.tLayers)
//!                                 → cascade Cleanup() to children
//! ```
//!
//! That last block is the whole point: the framework tears down what it knows about. Your job is to
//! make sure it knows.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use mercs2_script::{EngineHost, ScriptHost, SharedHost};

#[derive(Default)]
struct MissionHost {
    now: f32,
    /// Importing `MrxTask` drags in the resident framework — sound banks, the shop, the GUI layer —
    /// and all of it chatters through Debug.Printf. That's the real engine booting, and it's
    /// fascinating exactly once; it is not what you're here to read. Muted until the task starts.
    quiet: bool,
    boot_lines: usize,
    printed: Vec<String>,
    /// Localized-token check: the game's UI strings are `[Some.Token]` keys resolved from a string
    /// table. A bare English sentence renders as that literal sentence, untranslated.
    raw_strings: Vec<String>,
    layers: Vec<String>,
    layers_removed: Vec<String>,
    next_guid: u64,
}

impl MissionHost {
    fn note(&self, who: &str, msg: &str) {
        println!("  t={:<5.2} │ {who} {msg}", self.now);
    }
}

impl EngineHost for MissionHost {
    fn log(&mut self, _source: &str, msg: &str) {
        if self.quiet {
            self.boot_lines += 1;
            return;
        }
        // The task framework narrates itself through Debug.Printf ("Task X complete", "Cleaning up
        // X"). Those lines ARE the framework telling you what it did — worth reading, not filtering.
        let framework = msg.starts_with("Task ")
            || msg.starts_with("Cleaning up")
            || msg.starts_with("Adding ")
            || msg.starts_with("Dynamically ")
            || msg.starts_with("Not destroying")
            || msg.starts_with("Activation of")
            || msg.starts_with("Completion of")
            || msg.starts_with("Cancellation of")
            || msg.starts_with("_SetState")
            || msg.starts_with("Attempting to clean up")
            || msg.starts_with("ASSERT");
        let tag = if framework { "\x1b[35mMrxTask\x1b[0m" } else { "\x1b[33mscript \x1b[0m" };
        println!("  t={:<5.2} │ {tag} {msg}", self.now);
        self.printed.push(msg.to_string());
    }
    fn get_level_name(&self) -> String {
        "vz".into()
    }
    fn guid_by_name(&mut self, _name: &str) -> u64 {
        self.next_guid += 1;
        self.next_guid
    }
    fn pg_spawn(&mut self, template: &str, _pos: [f32; 3], _yaw: f32, _high: bool) -> u64 {
        self.next_guid += 1;
        self.note("\x1b[36mengine \x1b[0m", &format!("Pg.Spawn(\"{template}\") → guid {}", self.next_guid));
        self.next_guid
    }
    fn object_set_name(&mut self, _guid: u64, _name: &str) {}
    fn object_set_position(&mut self, _guid: u64, _pos: [f32; 3]) {}
    fn object_set_yaw(&mut self, _guid: u64, _yaw: f32) {}
    fn teleport_hero(&mut self, _pos: [f32; 3]) {}

    fn add_layers(&mut self, layers: &[String]) {
        for l in layers {
            self.note("\x1b[36mengine \x1b[0m", &format!("world layer \x1b[32mADDED\x1b[0m: {l}"));
            self.layers.push(l.clone());
        }
    }
}

fn main() {
    let lesson = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: cargo run -p mercs2_script --example mission_lab -- <lesson-name>");
        eprintln!("   e.g. 01_you_forgot_the_parent   (no .lua, no path — it's a MODULE name)");
        std::process::exit(2);
    });

    // Two module roots: the real decompiled game scripts (so `inherit(\"MrxTask\")` resolves to the
    // actual shipped base class), and our lessons. Modules are resolved by file stem, which is why
    // the lesson is named, not pathed — exactly how the game imports.
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = crate_dir.join("..").join("..").join("..").join("..");
    let corpus = repo.join("docs").join("mercs2-luacd").join("src");
    let lessons = crate_dir.join("examples").join("mission_lessons");
    if !corpus.exists() {
        eprintln!("cannot find the decompiled script corpus at {}", corpus.display());
        std::process::exit(2);
    }
    let lesson_path = lessons.join(format!("{lesson}.lua"));
    if !lesson_path.exists() {
        eprintln!("no such lesson: {}", lesson_path.display());
        std::process::exit(2);
    }

    let host = Rc::new(RefCell::new(MissionHost::default()));
    // Lessons root FIRST: earlier roots win a name collision, which lets `_engine_stubs/` shadow the
    // one corpus module whose auto-`Init()` would boot the whole shell UI (see that file for why it's
    // safe here). Everything else — MrxTask, MrxTaskState, MrxTimer, MrxUtil — is the real thing.
    let sh = ScriptHost::new(vec![lessons, corpus]).expect("lua");
    let shared: SharedHost = host.clone();
    sh.register_engine(shared).expect("bindings");
    // Bring-up stubs for engine surfaces the task framework touches but this lab doesn't model.
    let stubs: Rc<RefCell<BTreeSet<String>>> = Rc::new(RefCell::new(BTreeSet::new()));
    sh.enable_autostub(stubs.clone()).expect("autostub");

    println!("\n\x1b[1m═══ mission_lab ═══\x1b[0m  {lesson}\n");
    println!("  Driving the REAL MrxTask base class from the shipped game against your contract.");
    println!("  \x1b[35mMrxTask\x1b[0m lines are the framework narrating itself. \x1b[33mscript\x1b[0m lines are yours.\n");

    // Importing MrxTask boots the resident framework for real (sound banks, shop, GUI). Muted.
    host.borrow_mut().quiet = true;
    if let Err(e) = sh.exec("import(\"MrxTask\")", "@mission_lab_import") {
        println!("\n\x1b[31mLUA ERROR importing the task framework:\x1b[0m {e}\n");
        std::process::exit(1);
    }
    host.borrow_mut().quiet = false;
    println!(
        "  \x1b[90m(imported the real resident framework — {} boot log lines muted)\x1b[0m\n",
        host.borrow().boot_lines
    );

    // The framework's own modules register events during boot (GUI, sound, …). Those are not yours,
    // so everything below is measured as a DELTA against this baseline — otherwise the framework's
    // handles would read as your leak.
    let baseline = sh.live_event_handles();

    println!("  ── the task's life ─────────────────────────────────────────");

    // Build and run the task exactly the way the engine does. `sModuleName` is what makes Activate()
    // go through dynamic_import → _ModuleLoaded → your module, so this is the real chain, not a
    // shortcut into Activated().
    let boot = format!(
        r#"
        oTask = MrxTask.Create(MrxTask)
        oTask:Configure({{ sName = "MyContract", sModuleName = "{lesson}" }})
        oTask:Activate()
        "#
    );
    if let Err(e) = sh.exec(&boot, "@mission_lab_boot") {
        println!("\n\x1b[31mLUA ERROR bringing the task up:\x1b[0m {e}\n");
        std::process::exit(1);
    }

    // Did the task actually reach the Active state? It only does so if MrxTask.Activated(self) ran —
    // i.e. if the lesson's Activated() called its parent. This is the single most common way a
    // contract half-works.
    let active: bool = sh.eval("return oTask:IsActive()").unwrap_or(false);
    let children: usize = sh
        .eval("local n = 0 for _ in pairs(oTask:GetChildren()) do n = n + 1 end return n")
        .unwrap_or(0);

    // Events the task armed for itself, over and above the framework's own.
    let armed = sh.live_event_handles().saturating_sub(baseline);

    // Time passes. Timers registered by the contract advance here — including the one that completes
    // the mission, if the lesson set one.
    for i in 1..=10 {
        host.borrow_mut().now = i as f32 * 0.5;
        sh.pump_events(0.5).ok();
    }

    // If the contract didn't finish itself, the engine ends it — a mission always ends somehow.
    let self_completed: bool = sh.eval("return oTask:IsCompleted()").unwrap_or(false);
    if !self_completed {
        host.borrow_mut().now = 5.5;
        host.borrow().note("\x1b[36mengine \x1b[0m", "mission ends — the engine Completes the task");
        if let Err(e) = sh.exec("oTask:Complete()", "@mission_lab_complete") {
            println!("  \x1b[31mLUA ERROR in Complete:\x1b[0m {e}");
        }
    }
    let completed: bool = sh.eval("return oTask:IsCompleted()").unwrap_or(false);

    // Anything still armed now outlived the mission. In game that means a timer from a finished
    // contract still firing into a world that has moved on — watch for chatter below this line.
    host.borrow_mut().now = 6.0;
    host.borrow().note("\x1b[36mengine \x1b[0m", "\x1b[1m— the mission is over —\x1b[0m");
    for _ in 0..4 {
        host.borrow_mut().now += 0.5;
        sh.pump_events(0.5).ok();
    }
    let leaked = sh.live_event_handles().saturating_sub(baseline);

    // Player-visible strings are a source-level property, so this is a lint over the lesson file
    // rather than something the harness watched happen. Labelled as such in the report.
    if let Ok(src) = std::fs::read_to_string(lesson_path) {
        for key in ["sDspShortDesc", "sDspLongDesc", "sTitle", "sName"] {
            for line in src.lines() {
                let line = line.trim();
                if line.starts_with("--") || !line.contains(key) {
                    continue;
                }
                let Some(open) = line.find('"') else { continue };
                let Some(close) = line[open + 1..].find('"') else { continue };
                let val = &line[open + 1..open + 1 + close];
                // sName is an internal task id, never shown — only the Dsp*/Title strings are UI.
                if key == "sName" || val.is_empty() {
                    continue;
                }
                if !(val.starts_with('[') && val.ends_with(']')) {
                    host.borrow_mut().raw_strings.push(format!("{key} = \"{val}\""));
                }
            }
        }
    }

    report(&sh, &host, active, completed, children, armed, leaked, &stubs.borrow());
}

#[allow(clippy::too_many_arguments)]
fn report(
    _sh: &ScriptHost,
    host: &Rc<RefCell<MissionHost>>,
    active: bool,
    completed: bool,
    children: usize,
    armed: usize,
    leaked: usize,
    stubs: &BTreeSet<String>,
) {
    let h = host.borrow();
    println!("\n  ── report ──────────────────────────────────────────────────\n");

    let mark = |ok: bool| if ok { "\x1b[32m✓\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };

    println!("  task state:");
    println!(
        "    {} reached ACTIVE after Activate()   {}",
        mark(active),
        if active {
            "\x1b[90myour Activated() called MrxTask.Activated(self)\x1b[0m"
        } else {
            "\x1b[31mNEVER WENT ACTIVE — your Activated() did not call its parent\x1b[0m"
        }
    );
    println!(
        "    {} reached COMPLETED after Complete() {}",
        mark(completed),
        if completed {
            "\x1b[90mthe task closed out cleanly\x1b[0m"
        } else {
            "\x1b[31mComplete() refused — the task was never Active to begin with\x1b[0m"
        }
    );

    println!(
        "\n  event handlers YOUR TASK armed: \x1b[1m{armed}\x1b[0m → still armed after the mission: \x1b[1m{leaked}\x1b[0m"
    );
    println!("    \x1b[90m(measured as a delta against the framework's own handles, which aren't yours.)\x1b[0m");
    if leaked > 0 {
        println!("    \x1b[31m✗ {leaked} handler(s) OUTLIVED the mission.\x1b[0m");
        println!("        MrxTask.Cleanup() deletes every handle in `self._tEvents` — and the ONLY");
        println!("        things in there are what you created with \x1b[1mself:_CreateEvent\x1b[0m /");
        println!("        \x1b[1mself:_CreatePersistentEvent\x1b[0m. A raw \x1b[1mEvent.Create\x1b[0m is invisible to");
        println!("        the framework, so it survives the mission and keeps firing.");
    } else if armed > 0 {
        println!("    \x1b[32m✓ all reclaimed\x1b[0m — every event you made went through self:_CreateEvent,");
        println!("        so Cleanup() knew about it and deleted it. You wrote no teardown code at all.");
    }

    println!("\n  child objectives created via self:CreateChild(): \x1b[1m{children}\x1b[0m");
    if children > 0 {
        println!("    \x1b[90mCleanup() cascades into every child, so their events die with the mission too.\x1b[0m");
    }

    // A static lint on the lesson source, not a runtime observation — the string table isn't modelled
    // here, so this is honest about being a read of your code rather than of the engine's behaviour.
    let strings_ok = h.raw_strings.is_empty();
    if !strings_ok {
        println!("\n  \x1b[31m✗ player-visible text that is not a [Token]:\x1b[0m");
        for s in &h.raw_strings {
            println!("      \x1b[33m{s}\x1b[0m");
        }
        println!("      \x1b[90m(source lint, not a runtime check.)\x1b[0m Player-visible text is a \x1b[1m[Token]\x1b[0m key");
        println!("      resolved from the string table (\"[ChiCon003.Objective.destroy]\"). A bare English");
        println!("      sentence renders exactly as typed — in every language the game ships in.");
    }

    if !stubs.is_empty() {
        let shown: Vec<_> = stubs.iter().take(6).cloned().collect();
        println!(
            "\n  \x1b[90mengine surfaces this lab stubbed out ({}): {}{}\x1b[0m",
            stubs.len(),
            shown.join(", "),
            if stubs.len() > shown.len() { ", …" } else { "" }
        );
    }

    let clean = active && completed && leaked == 0 && strings_ok;
    println!();
    if clean {
        println!("  \x1b[1;32mVERDICT: clean.\x1b[0m The task went Active, the framework tore down every event you");
        println!("  handed it, and your UI text is localizable. Notice how little teardown code you wrote:");
        println!("  none. That is what cooperating with the framework buys you.\n");
    } else {
        println!("  \x1b[1;31mVERDICT: not clean yet.\x1b[0m Read the ✗ lines, open the lesson, find the STATION");
        println!("  comment, change one thing, run it again.\n");
    }
}
