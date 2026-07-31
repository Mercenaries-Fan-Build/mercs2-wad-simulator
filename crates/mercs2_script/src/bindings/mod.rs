//! Per-namespace engine binding harness + coverage gate (Wave-0 silo E3).
//!
//! # The nil-handle contract — hold this in every binding you write
//!
//! **A GUID/handle argument is [`crate::Guid`], never a bare `i64`, and a miss is a silent neutral
//! answer.**
//!
//! `Guid` is the Lua-boundary handle type: it goes out as **lightuserdata** (retail's type tag 2) and
//! comes back through a `FromLua` that absorbs `nil`, an absent argument and anything unrecognised
//! into [`crate::Guid::NONE`] — exactly what retail's reader does, and exactly as tolerant as the
//! `Option<i64>` this section used to prescribe. A handle **return** is a `Guid` too, so `0` pushes
//! `nil` rather than a truthy `0`. See [`crate::guid`] for the evidence and the pointer-width caveat.
//!
//! Retail's argument readers return `0` for anything they do not recognise rather than raising:
//! `FUN_0059FF50` accepts only the userdata type tags and yields 0 otherwise, and the miss path
//! `FUN_004B2A50` is literally `push nil; return 1`. Shipped scripts are written against exactly that.
//! They chain `Vehicle.GetRiders(Pg.GetGuidByName(sName))` and let a failed lookup fall through, or
//! branch on `if Player.X(u) then`. A binding typed `i64` turns that into a **Lua error**, which aborts
//! the caller's whole callback chain — and since those chains are how the game advances its state
//! machines, a single raised argument can stall the boot.
//!
//! This was not theoretical. ~160 bindings took a bare `i64` handle; converting them took the corpus
//! import sweep (`mercs2_engine/tests/corpus_replay.rs`) from 369/370 modules to **370/370**.
//! `lifestyle_oillif001_table.lua` is the canonical case: a world lookup misses, and everything
//! downstream of it has to tolerate the nil.
//!
//! One distinction the sweep had to respect: an argument that is a **constant**, not a handle, stays
//! `i64`. `Event.Create(kind, …)`'s event-kind is not a handle, so a nil there is a genuine script bug
//! and should raise. The same applies to ids, counts, indices and colour channels — and to two ids
//! that look like handles but are not: the **widget id** (`bindings::hud`) and the **event handle**
//! (`bindings::event`), each argued in place from the corpus.
//!
//! The engine's Lua binding surface is **~1086 cfuncs across 35 engine namespaces** (the live
//! Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`; the human index is
//! `docs/reverse_engineer/scripting_host_binding_code_map.md` §3). Program rule: **no stubbed Lua
//! streams** — every one of those bindings the game's Lua calls eventually needs a real body. This
//! module makes that surface *modular* (one file per namespace, so each later silo fills exactly one)
//! and *measurable* (a machine-readable coverage report so "N stubs remaining" is CI-checkable and
//! trends to zero).
//!
//! ## The convention every later silo follows
//! Each `bindings/<ns>.rs` is self-contained and declares:
//! - `NAMESPACE: &str` — stable coverage key (unique per luaL_Reg table).
//! - `GLOBAL: &str` — the Lua global table it installs as (two tables may share a global, e.g. `Pg`).
//! - `TABLE_VA: u32` — the luaL_Reg table VA in the unpacked image (traceability to the oracle).
//! - `REQUIRED: &[Required]` — the full cfunc surface (name + corpus call count). **Seeded from the
//!   corpus; never trimmed.** A name leaves the "stubs remaining" tally only by gaining a real body.
//! - `fn install(&Lua, &SharedHost) -> LuaResult<Installed>` — wires this build's bindings.
//!
//! To fill a namespace: open its file, add `b.real("Name", lua.create_function(..)?)?` for each
//! binding you back with a real [`crate::EngineHost`] call (or `b.stub("Name", ..)` for a deliberate
//! faithful no-op — e.g. the retail `0x006D5640` return-0 dev bindings), then `b.install_global(GLOBAL)`.
//! Nothing else in the crate changes; the coverage harness picks up the delta automatically.
//!
//! ## Reading the coverage report
//! [`coverage_json`] serializes [`NsCoverage`] to `binding_coverage.json` (written + asserted by the
//! `coverage_report` test). Per namespace it reports `required / real / stub / missing` and, most
//! importantly, `called_missing` — required cfuncs the game Lua actually calls (`corpus_calls > 0`)
//! that still lack a real body. Those are the faithful blockers; the headline metric is
//! `totals.remaining` (required − real) trending to zero.

use mercs2_luac::rt::{Function, IntoLua, Lua, Result as LuaResult, Table};

use crate::SharedHost;

// One module per engine namespace (per luaL_Reg table). Alphabetical-ish by VA; see `install_all`.
mod ai;
mod airstrike;
mod atmosphere;
mod bloom;
mod camera;
mod camera_fx;
mod debug;
pub(crate) mod event;
mod face;
mod fade;
mod fire;
mod graphics;
mod gui;
pub(crate) mod hud;
mod human;
mod inventory;
mod lti;
mod math_ns;
mod movie;
mod net;
mod object;
mod object_filter;
mod object_state;
mod pg;
mod pg_world;
pub(crate) mod player;
mod report;
mod socket;
mod sound;
mod string_ext;
mod table_ext;
mod sys;
mod sys_module;
mod timer;
mod vehicle;
mod vo;
mod weapon;

/// Back a list of action cfuncs as **recorded commands** on the host's generic command log
/// (`EngineHost::script_cmd`): each records `"Ns.Verb"` + its stringified args — a real intent the
/// corresponding runtime system drains, not a dropped no-op. Use for animation/callback/menu/action
/// verbs that have no queryable state of their own.
pub(crate) fn record_all(
    b: &mut NsBuilder,
    lua: &Lua,
    host: &SharedHost,
    ns: &'static str,
    names: &[&'static str],
) -> LuaResult<()> {
    for &name in names {
        let h = host.clone();
        let verb: std::rc::Rc<str> = std::rc::Rc::from(format!("{ns}.{name}").as_str());
        b.real(
            name,
            lua.create_function(move |_, args: mercs2_luac::rt::MultiValue| {
                let sa: Vec<String> = args.iter().map(stringify_arg).collect();
                h.borrow_mut().script_cmd(&verb, sa);
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

/// Stringify a Lua argument for a recorded command log (string/number/bool/nil → text; other → "").
/// Unpack a callback's **context table** into positional arguments.
///
/// The corpus's engine-callback convention is `fCallback` plus a `tData` *table*, which the engine
/// invokes as `fCallback(unpack(tData))` — e.g.
/// `InterpolateWidget(…, _HandleAnimationComplete, {self}, …)` must arrive as
/// `_HandleAnimationComplete(self)`, and `oMovie:SetEndCallback(HideSlow, {oWidget})` as
/// `HideSlow(oWidget)`. Passing the table itself instead lands one level of indirection off and the
/// handler indexes a field that is not there (`MrxGuiBase:678: attempt to index a nil value (field
/// 'AnimationData')`).
///
/// A non-table context is passed through as a single argument, and `None`/`Nil` yields no arguments.
pub(crate) fn unpack_ctx(v: Option<mercs2_luac::rt::Value>) -> Vec<mercs2_luac::rt::Value> {
    match v {
        Some(mercs2_luac::rt::Value::Table(t)) => {
            t.sequence_values::<mercs2_luac::rt::Value>().collect::<mercs2_luac::rt::Result<Vec<_>>>().unwrap_or_default()
        }
        Some(mercs2_luac::rt::Value::Nil) | None => Vec::new(),
        Some(other) => vec![other],
    }
}

pub(crate) fn stringify_arg(v: &mercs2_luac::rt::Value) -> String {
    match v {
        mercs2_luac::rt::Value::String(s) => s.to_string_lossy().to_string(),
        mercs2_luac::rt::Value::Number(n) => n.to_string(),
        mercs2_luac::rt::Value::Boolean(b) => b.to_string(),
        // A GUID now arrives as lightuserdata (see [`crate::guid`]), not `Value::Integer`. Render
        // the handle it carries, so the recorded-command log keeps printing `Ns.Verb(268435456,…)`
        // instead of silently dropping every handle it used to show.
        mercs2_luac::rt::Value::LightUserData(ud) => crate::guid::stringify_light_userdata(*ud),
        _ => String::new(),
    }
}

/// One required cfunc in a namespace's surface. `corpus_calls` = references seen in the decompiled
/// game Lua; `> 0` means the game actively uses it (a faithful blocker until it has a real body).
///
/// **Counting convention** — stated explicitly because two independent audits have flagged these
/// numbers as "stale" when they were merely counted differently:
/// - Corpora: `docs/mercs2-luacd/**` **plus** `docs/mercs2-dlc-luacd/src/**`. **Skip
///   `mercs2-dlc-luacd/raw/`** — it is the same DLC scripts in a rawer decompile, so including it
///   double-counts every DLC row.
/// - `*.lua` only. Prose hits in the corpora's own `.md` write-ups do not count.
/// - Count **references, not just invocations**: a `if Ns.Fn then` capability guard counts, because
///   it still requires the name to be present on the table.
///
/// The number is a priority signal, not an assertion about execution — being off by one changes
/// nothing downstream, since every consumer only tests `> 0`.
#[derive(Clone, Copy)]
pub struct Required {
    pub name: &'static str,
    pub corpus_calls: u32,
}

/// What a namespace's [`install`](ai::install) actually wired this build: the names given a real
/// engine-backed body vs. a deliberate no-op/placeholder ("stub"). Names outside `REQUIRED` (boot
/// conveniences like `Debug.Print`) may be installed too but are not tracked here.
pub struct Installed {
    pub real: Vec<&'static str>,
    pub stub: Vec<&'static str>,
}

impl Installed {
    /// Nothing wired yet — the default for an unimplemented namespace.
    pub fn none() -> Self {
        Installed {
            real: Vec::new(),
            stub: Vec::new(),
        }
    }
}

/// Accumulates real/stub bindings while building a namespace's Lua table, then installs it as a
/// global. This is the only surface a namespace file touches — it keeps the real/stub bookkeeping
/// honest (you cannot record a binding without also installing it).
pub struct NsBuilder<'a> {
    lua: &'a Lua,
    table: Table,
    real: Vec<&'static str>,
    stub: Vec<&'static str>,
}

impl<'a> NsBuilder<'a> {
    /// Start a fresh namespace table.
    pub fn new(lua: &'a Lua) -> LuaResult<Self> {
        Ok(NsBuilder {
            lua,
            table: lua.create_table()?,
            real: Vec::new(),
            stub: Vec::new(),
        })
    }

    /// Install `name` with a **real** (engine-backed) body and count it toward real coverage.
    pub fn real(&mut self, name: &'static str, f: Function) -> LuaResult<()> {
        self.table.set(name, f)?;
        self.real.push(name);
        Ok(())
    }

    /// Install `name` with a **stub** body (a deliberate no-op/placeholder — e.g. the retail
    /// `0x006D5640` return-0 dev bindings, or a boot placeholder). Counted as a remaining stub.
    pub fn stub(&mut self, name: &'static str, f: Function) -> LuaResult<()> {
        self.table.set(name, f)?;
        self.stub.push(name);
        Ok(())
    }

    /// Set a non-function member (e.g. `Event.ObjectHibernation` constants). Not coverage-tracked.
    pub fn value<V: IntoLua>(&mut self, name: &str, v: V) -> LuaResult<()> {
        self.table.set(name, v)
    }

    /// Install an extra convenience binding that is **not** part of `REQUIRED` (e.g. `Debug.Print`
    /// aliasing `Printf`). Installed but not coverage-tracked.
    pub fn extra(&mut self, name: &str, f: Function) -> LuaResult<()> {
        self.table.set(name, f)
    }

    /// Finish: install the table as the Lua global `global` and return the coverage delta.
    pub fn install_global(self, global: &str) -> LuaResult<Installed> {
        self.lua.globals().set(global, self.table)?;
        Ok(Installed {
            real: self.real,
            stub: self.stub,
        })
    }

    /// Install as a **child table** of an existing global — e.g. `Graphics.FuelTrail`.
    ///
    /// Retail builds these with marker rows inside the parent's `luaL_Reg` array
    /// (`{name, 0xFFFFFFFF}` opens, `{name, 0xFFFFFFFE}` closes), so Lua sees a nested table rather
    /// than a second global. `Human.Inventory` and the ten `Graphics.*` children are the real cases.
    /// The parent must already be installed; `install_all`'s order guarantees that for the pairs we
    /// wire.
    pub fn install_child(self, parent: &str, name: &str) -> LuaResult<Installed> {
        let globals = self.lua.globals();
        let parent_tbl: mercs2_luac::rt::Table = match globals.get(parent) {
            Ok(t) => t,
            Err(_) => {
                let t = self.lua.create_table()?;
                globals.set(parent, t.clone())?;
                t
            }
        };
        parent_tbl.set(name, self.table)?;
        Ok(Installed {
            real: self.real,
            stub: self.stub,
        })
    }
}

/// Per-namespace coverage record: the static `REQUIRED` surface joined with what [`install_all`]
/// wired this build.
pub struct NsCoverage {
    pub namespace: &'static str,
    pub global: &'static str,
    pub table_va: u32,
    pub required: &'static [Required],
    pub real: Vec<&'static str>,
    pub stub: Vec<&'static str>,
}

impl NsCoverage {
    pub fn required_count(&self) -> usize {
        self.required.len()
    }
    /// Required cfuncs given a real body.
    pub fn real_count(&self) -> usize {
        self.required
            .iter()
            .filter(|r| self.real.contains(&r.name))
            .count()
    }
    /// Required cfuncs given a stub (and not a real) body.
    pub fn stub_count(&self) -> usize {
        self.required
            .iter()
            .filter(|r| !self.real.contains(&r.name) && self.stub.contains(&r.name))
            .count()
    }
    /// Required cfuncs with no body at all yet.
    pub fn missing(&self) -> Vec<&'static str> {
        self.required
            .iter()
            .filter(|r| !self.real.contains(&r.name) && !self.stub.contains(&r.name))
            .map(|r| r.name)
            .collect()
    }
    /// Required cfuncs that still lack a **real** body (stub + missing) — the "stubs remaining" for
    /// this namespace.
    pub fn remaining(&self) -> usize {
        self.required_count() - self.real_count()
    }
    /// Required cfuncs the game Lua actually calls (`corpus_calls > 0`) that still lack a real body.
    /// These are the faithful blockers a silo should knock out first.
    pub fn called_missing(&self) -> Vec<&'static str> {
        self.required
            .iter()
            .filter(|r| r.corpus_calls > 0 && !self.real.contains(&r.name))
            .map(|r| r.name)
            .collect()
    }
}

/// Install every engine namespace for `host` and return the per-namespace coverage. This is the one
/// entry point `ScriptHost::register_engine` calls; the boot slice's real bodies live in the
/// individual namespace files (`debug`, `sys`, `pg`, `object`, `ai`, `vehicle`, `event`).
pub fn install_all(lua: &Lua, host: &SharedHost) -> LuaResult<Vec<NsCoverage>> {
    let mut cov = Vec::new();
    macro_rules! ns {
        ($m:ident) => {{
            let inst = $m::install(lua, host)?;
            cov.push(NsCoverage {
                namespace: $m::NAMESPACE,
                global: $m::GLOBAL,
                table_va: $m::TABLE_VA,
                required: $m::REQUIRED,
                real: inst.real,
                stub: inst.stub,
            });
        }};
    }
    ns!(object_filter);
    ns!(event);
    ns!(debug);
    ns!(weapon);
    ns!(vo);
    ns!(vehicle);
    ns!(sys);
    ns!(sound);
    ns!(report);
    ns!(player);
    ns!(pg);
    ns!(object_state);
    ns!(object);
    ns!(movie);
    ns!(net);
    ns!(timer);
    ns!(math_ns);
    ns!(lti);
    ns!(pg_world);
    ns!(human);
    ns!(inventory);
    ns!(hud);
    ns!(gui);
    ns!(graphics);
    ns!(camera);
    ns!(atmosphere);
    ns!(bloom);
    ns!(fade);
    ns!(fire);
    ns!(camera_fx);
    ns!(sys_module);
    ns!(face);
    ns!(airstrike);
    ns!(ai);
    ns!(socket);
    ns!(string_ext);
    ns!(table_ext);
    Ok(cov)
}

/// Aggregate totals over a coverage set.
pub struct Totals {
    pub namespaces: usize,
    pub required: usize,
    pub real: usize,
    pub stub: usize,
    pub missing: usize,
    pub remaining: usize,
    pub called_missing: usize,
}

/// Compute headline totals.
pub fn totals(cov: &[NsCoverage]) -> Totals {
    let mut t = Totals {
        namespaces: cov.len(),
        required: 0,
        real: 0,
        stub: 0,
        missing: 0,
        remaining: 0,
        called_missing: 0,
    };
    for c in cov {
        t.required += c.required_count();
        t.real += c.real_count();
        t.stub += c.stub_count();
        t.missing += c.missing().len();
        t.remaining += c.remaining();
        t.called_missing += c.called_missing().len();
    }
    t
}

/// Serialize the coverage set to the machine-readable `binding_coverage.json` (hand-rolled — every
/// value is a JSON identifier/number so no escaping is needed; keeps the crate dependency-light).
pub fn coverage_json(cov: &[NsCoverage]) -> String {
    fn arr(names: &[&str]) -> String {
        let inner: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
        format!("[{}]", inner.join(", "))
    }
    let t = totals(cov);
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"schema\": \"mercs2_script.binding_coverage/1\",\n");
    out.push_str(
        "  \"note\": \"Wave-0 E3 Lua binding harness. required=Surface-B cfunc surface; real=engine-backed body; stub=deliberate no-op; missing=no body. remaining=required-real (the stubs-remaining gate, trend to zero). Regenerate: cargo test -p mercs2_script coverage_report.\",\n",
    );
    out.push_str(&format!(
        "  \"totals\": {{ \"namespaces\": {}, \"required\": {}, \"real\": {}, \"stub\": {}, \"missing\": {}, \"remaining\": {}, \"called_missing\": {} }},\n",
        t.namespaces, t.required, t.real, t.stub, t.missing, t.remaining, t.called_missing
    ));
    out.push_str("  \"namespaces\": [\n");
    // Sort by table VA for stable, oracle-ordered output.
    let mut idx: Vec<usize> = (0..cov.len()).collect();
    idx.sort_by_key(|&i| cov[i].table_va);
    for (row, &i) in idx.iter().enumerate() {
        let c = &cov[i];
        let comma = if row + 1 < idx.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{ \"namespace\": \"{}\", \"global\": \"{}\", \"table_va\": \"{:#010x}\", \"required\": {}, \"real\": {}, \"stub\": {}, \"missing\": {}, \"remaining\": {}, \"real_fns\": {}, \"stub_fns\": {}, \"called_missing\": {} }}{}\n",
            c.namespace,
            c.global,
            c.table_va,
            c.required_count(),
            c.real_count(),
            c.stub_count(),
            c.missing().len(),
            c.remaining(),
            arr(&c
                .required
                .iter()
                .filter(|r| c.real.contains(&r.name))
                .map(|r| r.name)
                .collect::<Vec<_>>()),
            arr(&c
                .required
                .iter()
                .filter(|r| !c.real.contains(&r.name) && c.stub.contains(&r.name))
                .map(|r| r.name)
                .collect::<Vec<_>>()),
            arr(&c.called_missing()),
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    out
}
