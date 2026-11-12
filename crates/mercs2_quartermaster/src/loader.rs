//! The Lua Quartermaster splices onto retail modules: the hook dispatcher, the error policy, the
//! library loader, and one piece of code per hook of [`crate::hooks`].
//!
//! Every host's tail starts with the same prologue. It finds `_G._QM` (or creates it) and defines,
//! once per Lua state:
//! * `_QM.on(id, fn, owner)`: subscribe `fn` to hook `id`.
//! * `_QM.fire(id, ...)`: prints `[qm] hook <id>: enter` (mods or no mods), then calls each handler
//!   under `pcall`.
//! * `_QM.fail(owner, id, err)`: the error policy. It prints `[qm] FATAL ...`. When retail's dialog
//!   can be shown (`MrxGuiDialogBox` has run and there is a local player), it opens the dialog retail's
//!   own in-game prompts use, `MrxGuiDialogBox.DisplayDialogBox` (the death dialog,
//!   `mrxplayer.lua:400`), which takes keyboard, pad and mouse input, pausing the game; choosing
//!   **Exit** requests `Exiting`. Otherwise it prints `[qm] FATAL-NODIALOG ...`, which pmc_bb's log
//!   variants watch for: pmc_bb shows the text in a native message box and ends the process inside
//!   that print. Where nothing watches (pmc_bb's quiet variants), the print returns and the prologue
//!   requests `Exiting` itself. The game never continues past a failure. (`OpenSystemDialogBox` is not used: no
//!   retail code calls it, and live on 2026-11-07 it showed but took no input on PC.)
//! * `_QM.saved` (gameplay): the save table the world loaded from ([`prologue`]).
//! * `_QM.load(name, owner)`: `dynamic_import`s a library. The engine runs its body and calls back
//!   with its env, or with `nil` when it cannot load it, and logs nothing either way (memory
//!   `lua-import-never-executes-module`). `nil` goes to `_QM.fail`; an env marks the library ready.
//!
//! The prologue is defined in each host because a hook can fire before the session's loader entry
//! runs: `levelbootstrap.LoadLevel` runs before the `vz` master's chunk. A hook reaches the dispatcher
//! through `_G._QM`, never through `import`. An import in a host's tail would be a DEPS edge to qm code,
//! and a child's load error fails its parent.
//!
//! A library the session's entry (`world.scripts_loaded`, `shell.boot`) requested must be ready when
//! the session's settle hook (`world.begin` in gameplay, `shell.open` in the front end) fires, or the
//! hook fails naming it. That check covers the case where the engine never calls back. A library
//! requested at any later hook has no such check, because that hook can fire just before the settle
//! hook (live 2026-11-07: loading into an HQ, `hq.enter` fired 9 ms before `world.begin`, and
//! `dynamic_import` answered 325 ms after the request); its `nil` callback still reaches `_QM.fail`.
//!
//! Rows of one `(host, target)` are one wrapper ([`host_tails`]), which evaluates each row's filter
//! (`crate::hooks::Filter`) under `pcall`: a filter that errors goes to `_QM.fail` and does not dispatch.

use std::collections::BTreeMap;

use crate::hooks::{Call, Filter, Hook, Kind};
use crate::manifest::LoadSession;

/// The owner a failure names for qm's own code.
pub const QM_OWNER: &str = "Quartermaster";

/// The hooks qm's own loaders depend on in `session`: the loader's entry, the settle hook, and the hook
/// qm's sound loader unloads its banks at. Their hosts are the scripts a loader lives in
/// (`blast::loader_scripts`).
pub fn core_hooks(session: LoadSession) -> &'static [&'static str] {
    match session {
        LoadSession::Gameplay => &["world.scripts_loaded", "world.begin", "world.sound_exit"],
        LoadSession::FrontEnd => &["shell.boot", "shell.open", "shell.sound_exit"],
    }
}

/// The hooks a link generates in `session`: every hook of the catalogue that fires in it, in catalogue
/// order, mods or no mods (W-a), so each prints its entry line in every installed game and any library
/// can subscribe to any of them. A library's `load: { when }` that names no catalogue hook is an error
/// naming it.
pub fn generated_hooks(session: LoadSession, libraries: &[Library]) -> Result<Vec<&'static Hook>, String> {
    for lib in libraries.iter().filter(|l| l.sessions.contains(&session)) {
        if crate::hooks::hook(&lib.when).is_none() {
            return Err(format!("hook {} is not in qm's catalogue (data/hooks.tsv)", lib.when));
        }
    }
    Ok(crate::hooks::hooks().iter().filter(|h| h.sessions.contains(&session)).collect())
}

/// A library a Shipment's `add_script` asks qm to load ([`crate::manifest::LibraryLoad`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    /// The module name passed to `dynamic_import`.
    pub module: String,
    /// The Shipment that declares it, named in a failure.
    pub owner: String,
    /// The hook id it loads at.
    pub when: String,
    pub sessions: Vec<LoadSession>,
}

/// A host to splice code onto: the session whose blocks hold it, and its INFO name.
pub type Host = (LoadSession, String);

/// What [`host_tails`] splices onto one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tail {
    /// The Lua, one function its chunk calls.
    pub source: String,
    /// Each function the tail wraps, with the hooks its wrapper dispatches: the host's main chunk must
    /// define each one, which the link checks against the bytecode it splices onto.
    pub wraps: Vec<(String, Vec<String>)>,
}

/// The hook at which a session checks that every library requested before it is ready.
pub fn settle_hook(session: LoadSession) -> &'static str {
    match session {
        LoadSession::Gameplay => "world.begin",
        LoadSession::FrontEnd => "shell.open",
    }
}

/// Whether a library requested at `hook` is checked at its session's settle hook: true exactly for a
/// splice hook, the session's entry, which requests its libraries with the whole load still to come.
/// One requested at a later hook may be requested just before the settle hook (hq.enter fires 9 ms
/// before world.begin when a game loads into an HQ).
pub fn settles(hook: &Hook) -> bool {
    hook.kind == Kind::Splice
}

/// A Lua string literal, escaped so a name or message cannot end it.
pub fn lua_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The prologue every host's tail starts with in `session` (see the module documentation).
///
/// In gameplay it also keeps the save table the master's `LoadSingleton` receives, as `_QM.saved`
/// (`world.load`'s argument, `vz/xQ!L.lua:626`). Retail restores each system's state from that table
/// during the world load (`:790-815`) and clears it at the end of `_StartPlayerVisibleGameplay`
/// (`:865`), so state qm adds at `world.begin` reads it from here. The subscription is made when the
/// first gameplay host's chunk runs, before the engine calls `LoadSingleton`.
pub fn prologue(session: LoadSession) -> String {
    let keep_save = match session {
        LoadSession::Gameplay => format!(
            "  QM.on(\"world.load\", function(tSaveData)\n    QM.saved = tSaveData\n  end, {})\n",
            lua_string(QM_OWNER)
        ),
        LoadSession::FrontEnd => String::new(),
    };
    format!(
        "-- [Quartermaster] hook dispatcher, error policy and library loader (generated by qm link; do not edit).\n\
local QM = _G._QM\n\
if QM == nil then\n\
  QM = {{}}\n\
  _G._QM = QM\n\
end\n\
if QM.fire == nil then\n\
  QM.handlers = {{}}\n\
  QM.requested = {{}}\n\
  QM.settling = {{}}\n\
  QM.ready = {{}}\n\
  QM.failed = false\n\
  QM.settle = {settle}\n\
  function QM.pack(...)\n\
    return select(\"#\", ...), {{...}}\n\
  end\n\
  function QM.module(name)\n\
    local mods = _G._MODULES\n\
    if type(mods) ~= \"table\" then\n\
      return nil\n\
    end\n\
    local want = string.lower(name)\n\
    for k, v in pairs(mods) do\n\
      if type(k) == \"string\" and string.lower(k) == want then\n\
        return v\n\
      end\n\
    end\n\
    return nil\n\
  end\n\
  function QM.fail(owner, id, err)\n\
    if QM.failed then\n\
      return\n\
    end\n\
    QM.failed = true\n\
    local text = tostring(owner) .. \" at \" .. tostring(id) .. \": \" .. tostring(err)\n\
    print(\"[qm] FATAL \" .. text)\n\
    local box = QM.module(\"MrxGuiDialogBox\")\n\
    local player = nil\n\
    if Player ~= nil and Player.GetLocalPlayer ~= nil then\n\
      player = Player.GetLocalPlayer()\n\
    end\n\
    local shown = nil\n\
    if box ~= nil and box.DisplayDialogBox ~= nil and player ~= nil then\n\
      shown = box.DisplayDialogBox(player, \"Quartermaster: \" .. text, {{\"Exit\"}}, 1, function()\n\
        print(\"[qm] FATAL dismissed: requesting Exiting\")\n\
        Sys.RequestGameState(\"Exiting\")\n\
      end, {{}}, nil, nil, nil, nil, true, 1)\n\
    end\n\
    if shown == nil then\n\
      print(\"[qm] FATAL-NODIALOG \" .. text)\n\
      Sys.RequestGameState(\"Exiting\")\n\
    end\n\
  end\n\
  function QM.test(id, fn, ...)\n\
    local ok, v = pcall(fn, ...)\n\
    if not ok then\n\
      QM.fail({qm}, id, \"its filter: \" .. tostring(v))\n\
      return false\n\
    end\n\
    return v and true or false\n\
  end\n\
  function QM.on(id, fn, owner)\n\
    local list = QM.handlers[id]\n\
    if list == nil then\n\
      list = {{}}\n\
      QM.handlers[id] = list\n\
    end\n\
    list[#list + 1] = {{fn = fn, owner = owner or \"?\"}}\n\
  end\n\
  function QM.fire(id, ...)\n\
    print(\"[qm] hook \" .. id .. \": enter\")\n\
    if id == QM.settle then\n\
      for name, owner in pairs(QM.settling) do\n\
        if not QM.ready[name] then\n\
          QM.fail(owner, id, \"library \" .. name .. \" was requested but never loaded\")\n\
          return\n\
        end\n\
      end\n\
    end\n\
    local list = QM.handlers[id]\n\
    if list == nil then\n\
      return\n\
    end\n\
    for i = 1, #list do\n\
      local h = list[i]\n\
      local ok, err = pcall(h.fn, ...)\n\
      if not ok then\n\
        QM.fail(h.owner, id, err)\n\
        return\n\
      end\n\
    end\n\
  end\n\
  function QM.load(name, owner, settles)\n\
    if QM.requested[name] ~= nil then\n\
      return\n\
    end\n\
    QM.requested[name] = owner\n\
    if settles then\n\
      QM.settling[name] = owner\n\
    end\n\
    local function loaded(env)\n\
      if env == nil then\n\
        QM.fail(owner, \"load \" .. name, \"the engine could not load module \" .. name)\n\
      else\n\
        QM.ready[name] = true\n\
        print(\"[qm] library \" .. name .. \": ready\")\n\
      end\n\
    end\n\
    local ok = dynamic_import(name, loaded)\n\
    if not ok then\n\
      QM.fail(owner, \"load \" .. name, \"dynamic_import found no free slot for \" .. name)\n\
    end\n\
  end\n\
{keep_save}\
end\n",
        settle = lua_string(settle_hook(session)),
        qm = lua_string(QM_OWNER),
    )
}

/// The code for one splice, event or state hook. A splice hook fires where it stands; an event or state
/// hook subscribes persistently. A wrap is [`wrap_code`].
pub fn hook_code(hook: &Hook) -> Result<String, String> {
    let id = lua_string(&hook.id);
    match hook.kind {
        Kind::Splice => Ok(format!("QM.fire({id})\n")),
        Kind::Wrap => Err(format!("hook {}: a wrap is generated with the other rows of its function", hook.id)),
        Kind::Event | Kind::State => {
            let target = lua_string(hook.target.as_deref().ok_or_else(|| format!("hook {}: no target", hook.id))?);
            let spec = if hook.kind == Kind::Event {
                format!("Event.ScriptEvent, {{{target}, function()\n  return true\nend}}")
            } else {
                let action = hook.id.rsplit('.').next().unwrap_or_default();
                format!("Event.GameStateChange, {{{target}, {}}}", lua_string(action))
            };
            Ok(format!("Event.CreatePersistent({spec}, function(...)\n  QM.fire({id}, ...)\nend)\n"))
        }
    }
}

/// The ONE wrapper of `rows`, every wrap row of one `(host, target)`: it replaces the host's global
/// `target` with a function that calls the original and passes every return value through, and
/// dispatches each row on the row's side of the call, when its filter allows. An `args` filter is read
/// before the original runs, a `results` filter over what it returned; each runs under `QM.test`.
pub fn wrap_code(rows: &[&Hook]) -> Result<String, String> {
    let first = rows.first().ok_or("a wrapper of no rows")?;
    let target = first.target.as_deref().ok_or_else(|| format!("hook {}: a wrap without a target", first.id))?;
    let call = first.call.ok_or_else(|| format!("hook {}: a wrap without a call", first.id))?;
    let mut tests = String::new();
    let mut reads = String::new();
    let mut fires = String::new();
    for (i, h) in rows.iter().enumerate() {
        if h.kind != Kind::Wrap || h.host != first.host || h.target.as_deref() != Some(target) || h.call != Some(call) {
            return Err(format!("hooks {} and {} are not one wrapper", first.id, h.id));
        }
        let id = lua_string(&h.id);
        match &h.filter {
            None => fires.push_str(&format!("      QM.fire({id}, ...)\n")),
            Some(Filter::Args(e)) => {
                tests.push_str(&format!("    local test_{i} = function(...)\n      return ({e})\n    end\n"));
                reads.push_str(&format!("      local fire_{i} = QM.test({id}, test_{i}, ...)\n"));
                fires.push_str(&format!("      if fire_{i} then\n        QM.fire({id}, ...)\n      end\n"));
            }
            Some(Filter::Results(e)) => {
                tests.push_str(&format!("    local test_{i} = function(...)\n      return ({e})\n    end\n"));
                fires.push_str(&format!("      if QM.test({id}, test_{i}, unpack(r, 1, n)) then\n        QM.fire({id}, ...)\n      end\n"));
            }
        }
    }
    let body = match call {
        Call::Before => format!("{reads}{fires}      return original(...)\n"),
        Call::After => format!("{reads}      local n, r = QM.pack(original(...))\n{fires}      return unpack(r, 1, n)\n"),
    };
    let ids: Vec<&str> = rows.iter().map(|h| h.id.as_str()).collect();
    Ok(format!(
        "do\n  local original = {target}\n  if type(original) ~= \"function\" then\n    \
         QM.fail({owner}, {ids}, {where_} .. \" is not a function when qm's hook is spliced\")\n  \
         else\n{tests}    {target} = function(...)\n{body}    end\n  end\nend\n",
        owner = lua_string(QM_OWNER),
        ids = lua_string(&ids.join(", ")),
        where_ = lua_string(&format!("{}.{}", first.host, target)),
    ))
}

/// The tail spliced onto each host: the prologue, then each hook the host carries (the wrap rows of one
/// function as one wrapper, where the first of them stands), then each library that loads at one of
/// those hooks. A library at a splice hook loads where the hook fires. A library at another hook
/// subscribes to it, from the tail of the host that hook lives in, which has run before the hook can
/// fire.
///
/// `hooks` are the hooks to generate; `libraries` are every library of the set, in load order. A library
/// whose hook is not in `hooks`, or whose session the hook does not fire in, is an error naming it: the
/// library would never load.
pub fn host_tails(hooks: &[&Hook], libraries: &[Library]) -> Result<BTreeMap<Host, Tail>, String> {
    let mut tails: BTreeMap<Host, Tail> = BTreeMap::new();
    let ensure = |tails: &mut BTreeMap<Host, Tail>, host: &Host| {
        tails.entry(host.clone()).or_insert_with(|| Tail { source: prologue(host.0), wraps: Vec::new() });
    };
    for hook in hooks {
        for session in &hook.sessions {
            let host: Host = (*session, hook.host.clone());
            ensure(&mut tails, &host);
            let tail = tails.get_mut(&host).expect("prologue inserted");
            if hook.kind == Kind::Wrap {
                let target = hook.target.clone().ok_or_else(|| format!("hook {}: a wrap without a target", hook.id))?;
                if tail.wraps.iter().any(|(t, _)| *t == target) {
                    continue;
                }
                let rows: Vec<&Hook> = hooks
                    .iter()
                    .copied()
                    .filter(|h| h.kind == Kind::Wrap && h.host == hook.host && h.target == hook.target && h.sessions.contains(session))
                    .collect();
                tail.source.push_str(&wrap_code(&rows)?);
                tail.wraps.push((target, rows.iter().map(|h| h.id.clone()).collect()));
            } else {
                tail.source.push_str(&hook_code(hook)?);
            }
        }
    }
    for lib in libraries {
        let hook = hooks
            .iter()
            .find(|h| h.id == lib.when)
            .ok_or_else(|| format!("library {} of {} loads at {}, which qm does not generate", lib.module, lib.owner, lib.when))?;
        for session in &lib.sessions {
            if !hook.sessions.contains(session) {
                return Err(format!("library {} of {} loads at {} in {}, where that hook does not fire", lib.module, lib.owner, lib.when, session.token()));
            }
            let host: Host = (*session, hook.host.clone());
            let load = |checked: bool| format!("QM.load({}, {}, {checked})", lua_string(&lib.module), lua_string(&lib.owner));
            let code = if settles(hook) {
                format!("{}\n", load(true))
            } else {
                format!("QM.on({}, function()\n  {}\nend, {})\n", lua_string(&hook.id), load(false), lua_string(&lib.owner))
            };
            let tail = tails.get_mut(&host).expect("the hook's host has its tail");
            // A splice hook fires where it stands, so its libraries must load after it is fired; a
            // subscription must be registered before its hook can fire, which the host's top level is.
            tail.source.push_str(&code);
        }
    }
    // Each tail runs as one function its chunk calls. The host's env is the closure's, so a wrap still
    // replaces the host's global. The chunk's main function holds only that closure and call: the INFO
    // globals count of the splice is its own top-level definitions, and the tail defines none (it only
    // reassigns globals the host has), which the model computes without simulating the tail's branches.
    Ok(tails
        .into_iter()
        .map(|(host, t)| (host, Tail { source: format!("(function()\n{}end)()\n", t.source), wraps: t.wraps }))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::hook;

    fn compiles(src: &str) {
        if let Err(e) = mercs2_luac::compile(src, "qm_test") {
            panic!("generated Lua does not compile: {e}\n{src}");
        }
    }

    fn h(id: &str) -> &'static Hook {
        hook(id).unwrap_or_else(|| panic!("data/hooks.tsv has no {id}"))
    }

    #[test]
    fn every_host_tail_of_the_whole_catalogue_compiles() {
        let all: Vec<&Hook> = crate::hooks::hooks().iter().collect();
        let libs = vec![
            Library { module: "ess_names".into(), owner: "ess".into(), when: "world.scripts_loaded".into(), sessions: vec![LoadSession::Gameplay] },
            Library { module: "ess".into(), owner: "ess".into(), when: "world.scripts_loaded".into(), sessions: vec![LoadSession::Gameplay] },
            Library { module: "late".into(), owner: "x".into(), when: "hq.enter".into(), sessions: vec![LoadSession::Gameplay] },
        ];
        let tails = host_tails(&all, &libs).expect("tails");
        for ((session, host), t) in &tails {
            assert!(t.source.starts_with("(function()\n-- [Quartermaster] hook dispatcher"), "{session:?} {host}");
            assert!(t.source.ends_with("end)()\n"), "{session:?} {host}");
            compiles(&t.source);
            assert_eq!(t.source.matches("import(").count(), t.source.matches("dynamic_import(").count(), "{host} imports something: {}", t.source);
        }
        let vz = &tails[&(LoadSession::Gameplay, "vz".to_string())].source;
        let fire = vz.find("QM.fire(\"world.scripts_loaded\")").expect("the world entry fires");
        let ess_names = vz.find("QM.load(\"ess_names\", \"ess\", true)").expect("ess_names loads, checked at the settle hook");
        let ess = vz.find("QM.load(\"ess\", \"ess\", true)").expect("ess loads, checked at the settle hook");
        assert!(fire < ess_names && ess_names < ess, "fired first, then the libraries in load order");
        let hq = &tails[&(LoadSession::Gameplay, "mrxhq".to_string())].source;
        assert!(hq.contains("QM.on(\"hq.enter\", function()\n  QM.load(\"late\", \"x\", false)\nend, \"x\")"), "a later hook's library is not checked at the settle hook: {hq}");
        // Every wrap row is in exactly one wrapper, and each wrapper is listed for the link's check.
        let wrapped: usize = tails.values().map(|t| t.wraps.iter().map(|(_, ids)| ids.len()).sum::<usize>()).sum();
        assert_eq!(wrapped, all.iter().filter(|h| h.kind == Kind::Wrap).map(|h| h.sessions.len()).sum::<usize>());
    }

    #[test]
    fn a_wrap_passes_every_return_value_through_and_fires_on_its_side() {
        let after = wrap_code(&[h("hq.enter")]).expect("code");
        assert!(after.contains("local n, r = QM.pack(original(...))") && after.contains("return unpack(r, 1, n)"), "{after}");
        assert!(after.find("original(...)").expect("calls") < after.find("QM.fire(\"hq.enter\"").expect("fires"));
        let before = wrap_code(&[h("world.end")]).expect("code");
        assert!(before.find("QM.fire(\"world.end\"").expect("fires") < before.find("return original(...)").expect("calls"), "{before}");
        for src in [&after, &before] {
            compiles(&format!("{}{src}", prologue(LoadSession::Gameplay)));
        }
    }

    /// `world.begin` and `world.retry` are one wrapper of `_StartPlayerVisibleGameplay`: each filter is read
    /// before the original runs, under `QM.test`, and each row dispatches on its own filter.
    #[test]
    fn rows_of_one_function_are_one_wrapper_with_a_filter_each() {
        let tails = host_tails(&[h("world.scripts_loaded"), h("world.begin"), h("world.retry")], &[]).expect("tails");
        let t = &tails[&(LoadSession::Gameplay, "vz".to_string())];
        assert_eq!(t.wraps, vec![("_StartPlayerVisibleGameplay".to_string(), vec!["world.begin".to_string(), "world.retry".to_string()])]);
        assert_eq!(t.source.matches("_StartPlayerVisibleGameplay = function(...)").count(), 1, "{}", t.source);
        let read = t.source.find("local fire_0 = QM.test(\"world.begin\", test_0, ...)").expect("reads world.begin's filter");
        let read_retry = t.source.find("local fire_1 = QM.test(\"world.retry\", test_1, ...)").expect("reads world.retry's filter");
        let call = t.source.find("QM.pack(original(...))").expect("calls the original");
        assert!(read < call && read_retry < call, "{}", t.source);
        assert!(t.source.contains("return (not Pg.LoadIsRetry())") && t.source.contains("return (Pg.LoadIsRetry())"), "{}", t.source);
        compiles(&t.source);
    }

    #[test]
    fn a_results_filter_reads_what_the_original_returned() {
        let src = wrap_code(&[h("shop.purchase")]).expect("code");
        let call = src.find("QM.pack(original(...))").expect("calls");
        let test = src.find("QM.test(\"shop.purchase\", test_0, unpack(r, 1, n))").expect("tests the results");
        assert!(call < test, "{src}");
        compiles(&format!("{}{src}", prologue(LoadSession::Gameplay)));
    }

    #[test]
    fn an_event_and_a_state_hook_subscribe_persistently() {
        let ev = hook_code(h("cash.changed")).expect("code");
        assert!(ev.starts_with("Event.CreatePersistent(Event.ScriptEvent, {\"CashAdded\", function()"), "{ev}");
        let st = hook_code(h("state.pause.exit")).expect("code");
        assert!(st.starts_with("Event.CreatePersistent(Event.GameStateChange, {\"Pause\", \"exit\"}"), "{st}");
        assert!(hook_code(h("hq.enter")).unwrap_err().contains("generated with the other rows"));
    }

    #[test]
    fn a_library_at_a_hook_qm_does_not_generate_or_in_the_wrong_session_is_refused() {
        let gen = [h("world.scripts_loaded")];
        let lib = |when: &str, s: LoadSession| Library { module: "m".into(), owner: "o".into(), when: when.into(), sessions: vec![s] };
        assert!(host_tails(&gen, &[lib("hq.enter", LoadSession::Gameplay)]).unwrap_err().contains("does not generate"));
        assert!(host_tails(&gen, &[lib("world.scripts_loaded", LoadSession::FrontEnd)]).unwrap_err().contains("does not fire"));
    }

    #[test]
    fn a_link_generates_every_catalogue_hook_of_its_session() {
        for session in LoadSession::ALL {
            let got: Vec<&str> = generated_hooks(session, &[]).expect("hooks").iter().map(|h| h.id.as_str()).collect();
            let want: Vec<&str> = crate::hooks::hooks().iter().filter(|h| h.sessions.contains(&session)).map(|h| h.id.as_str()).collect();
            assert_eq!(got, want);
            for core in core_hooks(session) {
                assert!(got.contains(core), "{session:?} generates {core}");
            }
        }
        let none = Library { module: "m".into(), owner: "o".into(), when: "no.such".into(), sessions: vec![LoadSession::Gameplay] };
        assert!(generated_hooks(LoadSession::Gameplay, &[none]).unwrap_err().contains("no.such"));
    }
}
