//! Per-binding smoke coverage: EVERY `Required` cfunc across every namespace must resolve to a callable
//! Lua function after `register_engine`. This is the cheap ~1-assertion-per-binding guarantee — it does
//! NOT check a binding's return *value* (that's the behavioral hook tests in `game_hooks.rs` + the
//! coverage_report counts), but it catches the two failure modes that silently break a whole surface:
//!   1. a binding that is MISSING (never installed), and
//!   2. a binding CLOBBERED by a sibling namespace that shares the same Lua global and installs later
//!      (exactly the `pg_world` install overwriting `Pg.GetGuidByName`/`Spawn` that shipped and was
//!      caught only by an unrelated test — this test would have named it immediately).
//! With ~1086 Required cfuncs, that is ~1086 individual presence checks.

use std::cell::RefCell;
use std::rc::Rc;

use mercs2_script::{EngineHost, ScriptHost, SharedHost};

/// A do-nothing host — enough to `register_engine` (only the non-default `EngineHost` methods need
/// bodies; everything else inherits the trait defaults). This test is about binding PRESENCE, not host
/// behavior, so every method is a trivial default.
#[derive(Default)]
struct SmokeHost;

impl EngineHost for SmokeHost {
    fn log(&mut self, _source: &str, _msg: &str) {}
    fn get_level_name(&self) -> String {
        "vz".into()
    }
    fn guid_by_name(&mut self, _name: &str) -> u64 {
        0
    }
    fn pg_spawn(&mut self, _template: &str, _pos: [f32; 3], _yaw: f32, _high_detail: bool) -> u64 {
        1
    }
    fn object_set_name(&mut self, _guid: u64, _name: &str) {}
    fn object_set_position(&mut self, _guid: u64, _pos: [f32; 3]) {}
    fn object_set_yaw(&mut self, _guid: u64, _yaw: f32) {}
    fn teleport_hero(&mut self, _pos: [f32; 3]) {}
    fn add_layers(&mut self, _layers: &[String]) {}
}

/// Every `Required` binding resolves to a callable Lua function (not `nil`, and its namespace global is
/// a table). Fails with the exact list of missing/clobbered bindings.
#[test]
fn every_required_binding_is_callable() {
    let host: SharedHost = Rc::new(RefCell::new(SmokeHost::default()));
    let sh = ScriptHost::bare().expect("bare host");
    let cov = sh.register_engine_reported(host).expect("register engine");

    let mut broken: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for ns in &cov {
        for req in ns.required {
            checked += 1;
            // `type(_G[global][name])` — "function" = installed & callable; "nil" = missing/clobbered;
            // "NO-GLOBAL" = the namespace table itself never installed.
            let expr = format!(
                "local g = _G[\"{}\"]\n\
                 if type(g) ~= \"table\" then return \"NO-GLOBAL\" end\n\
                 return type(g[\"{}\"])",
                ns.global, req.name
            );
            let ty: String = sh.eval(&expr).unwrap_or_else(|e| format!("ERR({e})"));
            if ty != "function" {
                broken.push(format!("{}.{} -> {ty}", ns.global, req.name));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "{}/{checked} required bindings are NOT callable (missing or clobbered by a shared-global sibling):\n{}",
        broken.len(),
        broken.join("\n")
    );
}
