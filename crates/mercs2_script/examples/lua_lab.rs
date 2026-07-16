//! `lua_lab` — a learn-by-breaking harness for the Mercenaries 2 object-script API.
//!
//! Reference docs tell you *what exists* (`Event.Create`, `OnActivate`, "the object lifecycle").
//! They do not tell you *why the thing you wrote did nothing*, which is the only question you
//! actually have. This runs your script against a miniature engine and prints the timeline —
//! including every call the engine quietly threw on the floor, and why.
//!
//! ```text
//! cargo run -p mercs2_script --example lua_lab -- examples/lessons/01_the_engine_calls_you.lua
//! ```
//!
//! The world it simulates is the real one an object script lives in:
//!
//! * **Two** objects share the script — because one object script serves every placement of its
//!   kind, which is what makes module-level globals a trap (`tEvents[uGuid]`-style per-guid tables
//!   are all over the resident corpus for exactly this reason).
//! * Both `OnActivate`s run while both objects are still **asleep**, before either wakes. That
//!   ordering is what turns a shared global into a silent identity swap.
//! * The block streams **out and back in**, so anything you registered runs again. Most real script
//!   bugs only surface on that second pass.
//!
//! Hook signature is the corpus-canonical `OnActivate(uGuid, uRuntimeOwner, iArg)`
//! (`docs/mercs2-luacd/06_ai_world_entities.md` §1.1) — `iArg` is the per-placement variant integer.
//! The `Ai.*`-drops-while-asleep behaviour modelled here is why the game's `mrxai` module wraps every
//! AI command in an awake-gate (ibid. §5).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mercs2_script::{EngineHost, ScriptHost, SharedHost};

// ---------------------------------------------------------------------------
//   The miniature engine
// ---------------------------------------------------------------------------

struct Obj {
    name: String,
    /// The placement's variant integer — handed to `OnActivate` as `iArg`. One script, many
    /// placements, each configured by this number (antiair tier, mine trigger mode, …).
    iarg: i64,
    awake: bool,
    alive: bool,
    /// Did this object ever wake up? Used to score "did every object actually get served".
    ever_awake: bool,
    hp: f32,
}

/// Why the engine ignored a call. Every one of these is silent in the real game — no error, no
/// warning, no return value you'd think to check. That silence is the thing being taught.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum DropReason {
    /// The object exists but hasn't woken up yet — lesson 2.
    Asleep,
    /// The guid is a number that no longer (or never did) refer to anything — lesson 4.
    Dead,
    NoSuchObject,
    GuidZero,
}

impl DropReason {
    fn why(&self) -> &'static str {
        match self {
            DropReason::Asleep => "object is ASLEEP (hibernated) — the engine drops the call",
            DropReason::Dead => "object is DEAD — its guid is stale",
            DropReason::NoSuchObject => "no such object — guid was never valid",
            DropReason::GuidZero => {
                "no object by that name — you get guid 0, and every call on guid 0 is a silent no-op"
            }
        }
    }

    /// Which lesson's axis this drop belongs to.
    fn is_guid_fault(&self) -> bool {
        !matches!(self, DropReason::Asleep)
    }
}

#[derive(Default)]
struct LabHost {
    now: f32,
    objs: HashMap<u64, Obj>,
    order: Vec<u64>,
    named: HashMap<String, u64>,
    next_guid: u64,
    /// Calls the engine silently ignored: (what, why).
    drops: Vec<(String, DropReason)>,
    /// Calls that actually did something. Zero of these plus zero drops means your code never ran
    /// at all — which is a very different situation from "everything worked".
    landed: u32,
    /// Landed calls per target guid. An object that woke up and received *zero* calls, while its
    /// sibling received several, is the fingerprint of a shared-global identity collision.
    served: HashMap<u64, u32>,
    /// Everything the script printed — so a handler firing twice is visible as two lines.
    printed: Vec<String>,
}

impl LabHost {
    fn new() -> Self {
        LabHost { next_guid: 6, ..Default::default() }
    }

    /// Bring an object into the world, asleep — exactly how streaming delivers it.
    fn stream_in(&mut self, name: &str, iarg: i64) -> u64 {
        self.next_guid += 1;
        let g = self.next_guid;
        self.objs.insert(
            g,
            Obj {
                name: name.to_string(),
                iarg,
                awake: false,
                alive: true,
                ever_awake: false,
                hp: 100.0,
            },
        );
        self.order.push(g);
        self.named.insert(name.to_string(), g);
        g
    }

    fn wake(&mut self, guid: u64) {
        if let Some(o) = self.objs.get_mut(&guid) {
            o.awake = true;
            o.ever_awake = true;
        }
    }

    fn set_now(&mut self, t: f32) {
        self.now = t;
    }

    /// A line the *engine* is doing to the world.
    fn engine(&self, msg: &str) {
        println!("  t={:<5.2} │ \x1b[36mengine\x1b[0m   {msg}", self.now);
    }

    /// A line the *script* caused. `ok=false` means the engine dropped it.
    fn call(&mut self, what: &str, ok: bool, why: &str) {
        if ok {
            self.landed += 1;
            println!("  t={:<5.2} │ script → {:<40} \x1b[32m✓\x1b[0m", self.now, what);
        } else {
            println!(
                "  t={:<5.2} │ script → {:<40} \x1b[31m✗ DROPPED\x1b[0m — {why}",
                self.now, what
            );
        }
    }

    /// The gate every engine call passes through. Returns false (and records the drop) if the object
    /// can't receive the call. The real engine does this silently; we narrate it.
    fn gated(&mut self, what: &str, guid: u64) -> bool {
        let reason = match self.objs.get(&guid) {
            None => Some(DropReason::NoSuchObject),
            Some(o) if !o.alive => Some(DropReason::Dead),
            Some(o) if !o.awake => Some(DropReason::Asleep),
            Some(_) => None,
        };
        match reason {
            None => {
                self.call(what, true, "");
                *self.served.entry(guid).or_default() += 1;
                true
            }
            Some(r) => {
                self.call(what, false, r.why());
                self.drops.push((what.to_string(), r));
                false
            }
        }
    }
}

impl EngineHost for LabHost {
    // --- the 9 the trait requires -------------------------------------------------
    fn log(&mut self, _source: &str, msg: &str) {
        // Printf reaches the engine like anything else — it counts as a call that landed, so a script
        // that only prints doesn't get scored as "never called the engine".
        self.landed += 1;
        println!("  t={:<5.2} │ script → \x1b[33mDebug.Printf\x1b[0m: {msg}", self.now);
        self.printed.push(msg.to_string());
    }
    fn get_level_name(&self) -> String {
        "vz".into()
    }
    fn guid_by_name(&mut self, name: &str) -> u64 {
        let g = self.named.get(name).copied().unwrap_or(0);
        let what = format!("Pg.GetGuidByName(\"{name}\")");
        if g == 0 {
            self.call(&what, false, DropReason::GuidZero.why());
            self.drops.push((what, DropReason::GuidZero));
        } else {
            self.call(&format!("{what} → guid {g}"), true, "");
        }
        g
    }
    fn pg_spawn(&mut self, template: &str, _pos: [f32; 3], _yaw: f32, _high: bool) -> u64 {
        let g = self.stream_in(template, 0);
        // A spawn arrives awake — unlike a streamed-in placement.
        self.wake(g);
        self.call(&format!("Pg.Spawn(\"{template}\") → guid {g}"), true, "");
        g
    }
    fn object_set_name(&mut self, guid: u64, name: &str) {
        if self.gated(&format!("Object.SetName({guid}, \"{name}\")"), guid) {
            self.named.insert(name.to_string(), guid);
        }
    }
    fn object_set_position(&mut self, guid: u64, pos: [f32; 3]) {
        self.gated(&format!("Object.SetPosition({guid}, {:?})", pos), guid);
    }
    fn object_set_yaw(&mut self, guid: u64, yaw: f32) {
        self.gated(&format!("Object.SetYaw({guid}, {yaw})"), guid);
    }
    fn teleport_hero(&mut self, pos: [f32; 3]) {
        self.call(&format!("Player.Teleport({:?})", pos), true, "");
    }
    fn add_layers(&mut self, layers: &[String]) {
        self.call(&format!("World.AddLayers({:?})", layers), true, "");
    }

    // --- the ones this lab is actually about --------------------------------------
    fn object_is_awake(&self, guid: u64) -> bool {
        self.objs.get(&guid).is_some_and(|o| o.awake)
    }
    fn object_is_hibernated(&self, guid: u64) -> bool {
        !self.objs.get(&guid).is_some_and(|o| o.awake)
    }
    fn object_is_alive(&self, guid: u64) -> bool {
        self.objs.get(&guid).is_some_and(|o| o.alive)
    }
    fn object_health(&self, guid: u64) -> f32 {
        self.objs.get(&guid).map_or(0.0, |o| o.hp)
    }
    fn object_name(&self, guid: u64) -> String {
        self.objs.get(&guid).map_or_else(String::new, |o| o.name.clone())
    }
    fn object_kill(&mut self, guid: u64) {
        if self.gated(&format!("Object.Kill({guid})"), guid) {
            if let Some(o) = self.objs.get_mut(&guid) {
                o.alive = false;
            }
        }
    }
    fn ai_goal(&mut self, guid: u64, goal: &str) -> bool {
        self.gated(&format!("Ai.Goal({guid}, \"{goal}\")"), guid)
    }
    fn ai_order(&mut self, guid: u64, verb: &str) -> bool {
        self.gated(&format!("Ai.Order({guid}, \"{verb}\")"), guid)
    }
    fn ai_set_state(&mut self, guid: u64, state: &str, on: bool) -> bool {
        self.gated(&format!("Ai.SetState({guid}, \"{state}\", {on})"), guid)
    }
    fn sound_cue(&mut self, cue: &str) -> u64 {
        self.call(&format!("Sound.Cue(\"{cue}\")"), true, "");
        self.printed.push(format!("Sound.Cue({cue})"));
        1
    }
}

// ---------------------------------------------------------------------------
//   Hook dispatch — the engine calls YOU, and only by exact name
// ---------------------------------------------------------------------------

/// The module-level callbacks the engine dispatches by name. It does not read a class, it does not
/// look for a base type: it looks up this exact string in your script's globals and calls it if it's
/// there. If it isn't, nothing happens and nothing is reported. (`06_ai_world_entities.md` §1.1)
const HOOKS: &[(&str, &str)] = &[
    ("Init", "script module loaded"),
    ("OnActivate", "object streamed in — NOTE: it is still ASLEEP here"),
    ("OnDeath", "object killed"),
    ("OnDeactivate", "object hibernates / streams out — your only chance to clean up"),
];

struct Dispatcher<'a> {
    sh: &'a ScriptHost,
    called: HashMap<String, u32>,
    missing: Vec<(String, Option<String>)>,
}

impl Dispatcher<'_> {
    /// Call hook `name` with `args` if the script defines it. If it doesn't, say so out loud — and if
    /// the script defined something that differs only in case, name it. That miscased-global mistake
    /// is invisible in the real engine and costs people entire evenings.
    fn call(&mut self, host: &Rc<RefCell<LabHost>>, name: &str, args: &[i64]) {
        let shown = format!(
            "{name}({})",
            args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")
        );
        let f: Option<mlua::Function> = self.sh.lua().globals().get(name).ok();

        let Some(f) = f else {
            let near = self.miscased(name);
            match &near {
                Some(other) => println!(
                    "  t={:<5.2} │ \x1b[36mengine\x1b[0m   → {:<40} \x1b[31m✗ NOT DEFINED\x1b[0m — but you defined \x1b[33m{other}\x1b[0m. The engine dispatches by EXACT name.",
                    host.borrow().now, shown
                ),
                None => println!(
                    "  t={:<5.2} │ \x1b[36mengine\x1b[0m   → {:<40} \x1b[90m· not defined (fine, it's optional)\x1b[0m",
                    host.borrow().now, shown
                ),
            }
            if !self.missing.iter().any(|(n, _)| n == name) {
                self.missing.push((name.to_string(), near));
            }
            return;
        };

        println!("  t={:<5.2} │ \x1b[36mengine\x1b[0m   → \x1b[1m{shown}\x1b[0m", host.borrow().now);
        if let Err(e) = f.call::<()>(mlua::Variadic::from_iter(args.iter().copied())) {
            println!("  t={:<5.2} │ \x1b[31mLUA ERROR\x1b[0m in {name}: {e}", host.borrow().now);
        }
        *self.called.entry(name.to_string()).or_default() += 1;
    }

    /// Find a global function whose name matches `want` case-insensitively but not exactly.
    fn miscased(&self, want: &str) -> Option<String> {
        for pair in self.sh.lua().globals().pairs::<mlua::Value, mlua::Value>().flatten() {
            let (mlua::Value::String(k), mlua::Value::Function(_)) = (&pair.0, &pair.1) else {
                continue;
            };
            let name = k.to_string_lossy().to_string();
            if name.eq_ignore_ascii_case(want) && name != want {
                return Some(name);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
//   The run
// ---------------------------------------------------------------------------

/// The two placements that share this script. Different `iArg`s, because that is the whole point of
/// `iArg`: one script, many placements, each configured by a number the level editor baked in.
const PLACEMENTS: &[(&str, i64)] = &[("Outpost_Guard", 1), ("Outpost_Sniper", 3)];

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: cargo run -p mercs2_script --example lua_lab -- <lesson.lua>");
        eprintln!("   e.g. …/examples/lessons/01_the_engine_calls_you.lua");
        std::process::exit(2);
    });
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(2);
    });

    let host = Rc::new(RefCell::new(LabHost::new()));
    let sh = ScriptHost::bare().expect("lua");
    let shared: SharedHost = host.clone();
    sh.register_engine(shared).expect("bindings");

    println!("\n\x1b[1m═══ lua_lab ═══\x1b[0m  {path}\n");
    println!("  The engine is about to stream in TWO objects that share your script, and run their");
    println!("  whole lives against it.");
    println!("  \x1b[32m✓\x1b[0m = the engine did it.  \x1b[31m✗\x1b[0m = the engine ignored you, silently, as it would in the real game.\n");

    // The script module body runs once, at load. Anything you do at the top level happens HERE —
    // before any object exists. That is why real scripts do nothing at the top level.
    println!("  ── module body (top level) ──────────────────────────────────");
    if let Err(e) = sh.exec(&src, &path) {
        println!("\n\x1b[31mLUA ERROR loading the script:\x1b[0m {e}\n");
        std::process::exit(1);
    }

    let mut d = Dispatcher { sh: &sh, called: HashMap::new(), missing: Vec::new() };
    d.call(&host, "Init", &[]);

    println!("\n  ── the objects' lives ──────────────────────────────────────");

    // Both objects stream in and get OnActivate'd while BOTH are still asleep. That ordering is not
    // an artificial cruelty — it's how a block arrives, and it's exactly what makes a module-level
    // global silently take on the *last* object's identity for *both* of them.
    let mut guids = Vec::new();
    for (i, (name, iarg)) in PLACEMENTS.iter().enumerate() {
        host.borrow_mut().set_now(i as f32 * 0.1);
        let g = host.borrow_mut().stream_in(name, *iarg);
        guids.push(g);
        host.borrow().engine(&format!(
            "block streams in — '{name}' is guid \x1b[1m{g}\x1b[0m, iArg=\x1b[1m{iarg}\x1b[0m, state=\x1b[31mASLEEP\x1b[0m"
        ));
        d.call(&host, "OnActivate", &[g as i64, 0, *iarg]);
    }

    // The streaming system wakes the block's objects, and *then* dispatches their hibernation
    // events. Waking everything first matters: it means a callback that fires for guid A can still
    // legally act on guid B. So when a shared global makes a callback act on the wrong object, the
    // call LANDS — on the wrong target — instead of being dropped for being asleep. The bug shows up
    // as "the wrong object got served", which is exactly what it is.
    host.borrow_mut().set_now(0.6);
    for &g in &guids {
        host.borrow_mut().wake(g);
        host.borrow().engine(&format!("guid {g} → \x1b[32mAWAKE\x1b[0m"));
    }
    for &g in &guids {
        host.borrow()
            .engine(&format!("dispatch Event.ObjectHibernation {{{g}, \"awake\"}}"));
        sh.fire_object_hibernation(g, "awake").ok();
    }

    // Time passes; timers advance.
    for i in 1..=8 {
        host.borrow_mut().set_now(0.7 + i as f32 * 0.3);
        sh.pump_events(0.3).ok();
    }

    // The player kills the first one.
    let dead = guids[0];
    host.borrow_mut().set_now(3.1);
    host.borrow().engine(&format!("player destroys guid {dead} (fires Event.ObjectDeath)"));
    if let Some(o) = host.borrow_mut().objs.get_mut(&dead) {
        o.alive = false;
        o.hp = 0.0;
    }
    sh.fire_object_death(dead).ok();
    d.call(&host, "OnDeath", &[dead as i64]);

    // The player drives away; the block streams out.
    host.borrow_mut().set_now(3.5);
    host.borrow().engine("player drives away — block streams \x1b[31mOUT\x1b[0m");
    let before = sh.live_event_handles();
    for &g in &guids {
        d.call(&host, "OnDeactivate", &[g as i64]);
        if let Some(o) = host.borrow_mut().objs.get_mut(&g) {
            o.awake = false;
        }
    }
    let after_deact = sh.live_event_handles();

    // …and drives back. Everything runs AGAIN. This is where leaks surface.
    host.borrow_mut().set_now(4.0);
    host.borrow().engine("player drives back — block streams \x1b[32mIN\x1b[0m again (same guids, fresh OnActivate)");
    for (i, &g) in guids.iter().enumerate() {
        if let Some(o) = host.borrow_mut().objs.get_mut(&g) {
            o.alive = true;
            o.hp = 100.0;
            o.awake = false;
        }
        d.call(&host, "OnActivate", &[g as i64, 0, PLACEMENTS[i].1]);
    }
    host.borrow_mut().set_now(4.6);
    for &g in &guids {
        host.borrow_mut().wake(g);
        host.borrow().engine(&format!("guid {g} → \x1b[32mAWAKE\x1b[0m again"));
    }
    for &g in &guids {
        sh.fire_object_hibernation(g, "awake").ok();
    }
    for _ in 0..4 {
        sh.pump_events(0.3).ok();
    }

    report(&sh, &host, &d, before, after_deact);
}

fn report(
    sh: &ScriptHost,
    host: &Rc<RefCell<LabHost>>,
    d: &Dispatcher,
    handles_before_deact: usize,
    handles_after_deact: usize,
) {
    let h = host.borrow();
    println!("\n  ── report ──────────────────────────────────────────────────\n");

    println!("  lifecycle hooks the engine looked for:");
    for (name, when) in HOOKS {
        match d.called.get(*name) {
            Some(n) => println!("    \x1b[32m✓\x1b[0m {name:<14} called {n}×   \x1b[90m({when})\x1b[0m"),
            None => {
                let near = d.missing.iter().find(|(m, _)| m == name).and_then(|(_, n)| n.clone());
                match near {
                    Some(other) => println!(
                        "    \x1b[31m✗\x1b[0m {name:<14} \x1b[31mNEVER CALLED\x1b[0m — you spelled it \x1b[33m{other}\x1b[0m"
                    ),
                    None => println!("    \x1b[90m·\x1b[0m {name:<14} not defined   \x1b[90m({when})\x1b[0m"),
                }
            }
        }
    }

    // Per-object outcome. Two objects share this script; each should have been served on its own
    // terms. One woken object with zero calls, next to a sibling with several, is the fingerprint of
    // a module-level global that took on the last object's identity.
    println!("\n  per-object outcome:");
    let mut served = 0usize;
    let mut starved: Vec<u64> = Vec::new();
    for &g in &h.order {
        let Some(o) = h.objs.get(&g) else { continue };
        if !o.ever_awake {
            continue;
        }
        let n = h.served.get(&g).copied().unwrap_or(0);
        if n > 0 {
            served += 1;
            println!(
                "    \x1b[32m✓\x1b[0m guid {g}  {:<16} iArg={}   {n} call(s) landed on it",
                o.name, o.iarg
            );
        } else {
            starved.push(g);
            println!(
                "    \x1b[31m✗\x1b[0m guid {g}  {:<16} iArg={}   \x1b[31mwoke up and received NOTHING\x1b[0m",
                o.name, o.iarg
            );
        }
    }
    // Only a mixed outcome indicts identity handling. If nothing was served at all, some earlier
    // axis (a missing hook, an asleep drop) is the real cause and will say so — don't double-blame.
    let identity_ok = starved.is_empty() || served == 0;
    if !identity_ok {
        println!("    \x1b[90m→ one object was served and another was ignored. They share your script;\x1b[0m");
        println!("    \x1b[90m  a module-level global holds ONE value, so the second OnActivate overwrote\x1b[0m");
        println!("    \x1b[90m  the first object's identity before either one woke up.\x1b[0m");
    }

    let leaked = sh.live_event_handles();
    println!("\n  calls the engine dropped on the floor: \x1b[1m{}\x1b[0m", h.drops.len());
    if h.drops.is_empty() && h.landed == 0 {
        // Zero drops looks like success. It isn't — nothing ran to be dropped.
        println!("    \x1b[31mnone — because your script never made a single engine call.\x1b[0m");
        println!("    \x1b[90mNothing was dropped because nothing was attempted. See the hooks above.\x1b[0m");
    } else if h.drops.is_empty() {
        println!("    \x1b[32mnone — all {} calls landed on a live, awake object.\x1b[0m", h.landed);
    } else {
        let mut tally: HashMap<(String, DropReason), u32> = HashMap::new();
        for (what, why) in &h.drops {
            *tally.entry((what.clone(), *why)).or_default() += 1;
        }
        let mut rows: Vec<_> = tally.into_iter().collect();
        rows.sort();
        for ((what, why), n) in rows {
            println!("    \x1b[31m✗\x1b[0m {what}  ×{n}\n        \x1b[90m{}\x1b[0m", why.why());
        }
    }

    let leaks = handles_after_deact >= handles_before_deact && handles_before_deact > 0;
    println!(
        "\n  event handlers still registered at the end: \x1b[1m{leaked}\x1b[0m  \x1b[90m(before OnDeactivate: {handles_before_deact}, after: {handles_after_deact})\x1b[0m"
    );
    if leaks {
        println!(
            "    \x1b[31m✗ LEAK\x1b[0m — OnDeactivate did not Event.Delete what OnActivate created."
        );
        println!("        Every stream-out/in cycle stacks another copy of every handler.");
        println!("        Symptom in-game: the VO plays twice, the drop spawns twice, the trigger fires twice.");
    }

    let dupes: HashMap<&String, usize> = h.printed.iter().fold(HashMap::new(), |mut m, l| {
        *m.entry(l).or_default() += 1;
        m
    });
    let mut doubled: Vec<_> = dupes.iter().filter(|(_, n)| **n > 2).collect();
    doubled.sort();
    if !doubled.is_empty() {
        println!("\n  things your script did more than twice:");
        for (line, n) in doubled {
            println!("    \x1b[33m{n}×\x1b[0m {line}");
        }
        println!("    \x1b[90m(two objects × two stream-ins = 4 is normal here. Climbing past that is a leak.)\x1b[0m");
    }

    // Independent axes, one per lesson. They are scored separately on purpose: fixing lesson 2 should
    // turn lesson 2's line green even while lesson 3's is still red. A single pass/fail would punish a
    // correct answer just because the next lesson's bug is still in the file.
    let hooks_ok = d.called.contains_key("OnActivate");
    // "Nothing ran" and "everything was dropped" are different failures and must not be conflated.
    let attempted = h.landed as usize + h.drops.len();
    let asleep_ok = !h.drops.iter().any(|(_, r)| *r == DropReason::Asleep) && attempted > 0;
    let guids_ok = !h.drops.iter().any(|(_, r)| r.is_guid_fault());
    let leak_ok = !leaks;
    let has_deact = d.called.contains_key("OnDeactivate");

    let mark = |ok: bool| if ok { "\x1b[32m✓\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };
    println!("\n  scorecard:");
    println!(
        "    {} lesson 1 — the engine reached your code  {}",
        mark(hooks_ok),
        if hooks_ok {
            "\x1b[90mOnActivate was found and called\x1b[0m".to_string()
        } else {
            "\x1b[31ma hook the engine calls is missing or miscased\x1b[0m".to_string()
        }
    );
    println!(
        "    {} lesson 2 — your calls landed             {}",
        mark(asleep_ok),
        if attempted == 0 {
            "\x1b[31myour script never called the engine at all\x1b[0m".to_string()
        } else if asleep_ok && h.landed == 0 {
            "\x1b[90mnothing was dropped for being asleep (but see lesson 4)\x1b[0m".to_string()
        } else if asleep_ok {
            format!("\x1b[90mall {} reached an awake object\x1b[0m", h.landed)
        } else {
            "\x1b[31msome calls hit a sleeping object and were dropped\x1b[0m".to_string()
        }
    );
    println!(
        "    {} lesson 3 — handlers cleaned up           {}",
        mark(leak_ok),
        match (leak_ok, has_deact) {
            (false, _) => "\x1b[31mhandlers leak on every stream cycle\x1b[0m".to_string(),
            (true, true) => "\x1b[90mOnDeactivate deleted what OnActivate created\x1b[0m".to_string(),
            (true, false) =>
                "\x1b[90mnothing left registered (no OnDeactivate, but nothing needed one)\x1b[0m"
                    .to_string(),
        }
    );
    println!(
        "    {} lesson 4 — guids were valid              {}",
        mark(guids_ok),
        if guids_ok {
            "\x1b[90mno calls on guid 0 or a dead object\x1b[0m".to_string()
        } else {
            "\x1b[31ma guid was 0 or stale — the calls vanished\x1b[0m".to_string()
        }
    );
    println!(
        "    {} lesson 5 — every object got served       {}",
        mark(identity_ok),
        if !identity_ok {
            format!(
                "\x1b[31mguid {} woke up and got nothing — a global collided\x1b[0m",
                starved.iter().map(|g| g.to_string()).collect::<Vec<_>>().join(", ")
            )
        } else if served > 1 {
            format!("\x1b[90mall {served} objects were set up on their own terms\x1b[0m")
        } else {
            "\x1b[90mnothing served yet — fix the axes above first\x1b[0m".to_string()
        }
    );

    println!();
    if hooks_ok && asleep_ok && guids_ok && leak_ok && identity_ok && served > 1 {
        println!("  \x1b[1;32mVERDICT: clean.\x1b[0m Nothing dropped, nothing leaked, both objects were set up");
        println!("  on their own terms once awake, and torn down when they left. This is the shape every");
        println!("  real object script has — and now you know what each part is defending against.\n");
    } else {
        println!("  \x1b[1;31mVERDICT: not clean yet.\x1b[0m Read the ✗ lines above, open the lesson file, find the");
        println!("  STATION comment, change ONE thing, and run it again. The timeline will tell you.");
        println!("  \x1b[90m(A red line for a lesson you haven't reached yet is expected — fix them in order.)\x1b[0m\n");
    }
}
