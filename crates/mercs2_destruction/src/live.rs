//! Live destruction control — the Lua the Workshop console sends to a running game to inspect and
//! drive a destructible's state machine.
//!
//! Ess already exposes the *health* lever (`Ess.Object.damage/kill/setHealth`), which the engine
//! converts to a destruction transition (`FUN_004d05c0` → `SetState(InitDestroyedState)` + break
//! pieces). What it does not expose — and what this project uniquely holds — is the **global
//! `SetState` vocabulary** (`orchestrator::STATE_VOCABULARY`, cracked from `edit_state_machine`): the
//! shared state hashes the engine's `SetState` (`FUN_004d3e10`) keys on. With those names we can
//! generate the chunk to force ANY state, read transitions by name, and demolish/repair — the
//! console then sends it over `mercs2_bridge`.
//!
//! These functions only GENERATE Lua; the console runs it. That keeps them pure and unit-testable —
//! the correctness of a chunk is its bytes, checked here — while the live game is the one thing a
//! test cannot stand in for.

use mercs2_formats::orchestrator::{state_name, STATE_VOCABULARY};

/// The named destruction states a modder can force, each with its global `SetState` hash. Only the
/// name-cracked members (the ones a person can pick from a list); the observed-but-unnamed hashes
/// are omitted here because a menu of bare hex helps no one.
pub fn states() -> Vec<(&'static str, u32)> {
    STATE_VOCABULARY
        .iter()
        .filter(|(_, n)| !n.is_empty())
        .map(|(h, n)| (*n, *h))
        .collect()
}

/// The global-vocabulary hash for a state name (case-insensitive), or `None` if it is not a known
/// destruction state — the guard that stops a typo becoming an unreachable state.
pub fn state_hash(name: &str) -> Option<u32> {
    STATE_VOCABULARY
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
        .map(|(h, _)| *h)
}

/// Turn a user-typed target into a Lua guid expression. A bare word is treated as an object NAME and
/// resolved with `Ess.Guid("name")`; anything containing a `(`, `.`, or a leading digit is passed
/// through as an expression the author already wrote (a variable, `Ess.Player.guid()`, a literal).
pub fn guid_expr(target: &str) -> String {
    let t = target.trim();
    let looks_like_expr = t.is_empty()
        || t.starts_with(|c: char| c.is_ascii_digit())
        || t.contains(['(', ')', '.', '[', '"', '\'']);
    if looks_like_expr {
        t.to_string()
    } else {
        format!("Ess.Guid(\"{t}\")")
    }
}

/// Force a destructible into a named state via the engine's `SetState`. `state` must be in the
/// cracked vocabulary; the chunk carries the resolved name as a comment so a decompile of the
/// console log stays legible.
pub fn set_state_lua(target: &str, state: &str) -> Result<String, String> {
    let h = state_hash(state).ok_or_else(|| {
        let known: Vec<&str> = states().iter().map(|(n, _)| *n).collect();
        format!("{state:?} is not a known destruction state. Pick one of: {}", known.join(", "))
    })?;
    let named = state_name(h).unwrap_or(state);
    Ok(format!(
        "-- force {named} (0x{h:08X}) on the target\nObject.SetState({}, 0x{h:08X})\n",
        guid_expr(target)
    ))
}

/// Read a target's CURRENT destruction state — the typed read the game does serve (unlike a generic
/// per-component read, which waits on Ess). `Object.GetState` returns the live state hash; this
/// resolves it to a vocabulary name the same way [`watch_lua`] does, so the console prints
/// `PristineState` rather than bare hex. `GetStateName` (the engine's own string) is printed beside
/// it when present, so a state outside our cracked vocabulary is still legible.
pub fn read_state_lua(target: &str) -> String {
    let g = guid_expr(target);
    let mut table = String::from("local NAMES = {\n");
    for (name, hash) in states() {
        table.push_str(&format!("  [0x{hash:08X}] = \"{name}\",\n"));
    }
    table.push_str("}\n");
    format!(
        "{table}\
         local g = {g}\n\
         local st = Object.GetState(g)\n\
         local nm = NAMES[st] or (Object.GetStateName and Object.GetStateName(g)) or \
         string.format(\"0x%08X\", st)\n\
         Loader.Printf(\"state: 0x%08X -> %s\", g, nm)\n"
    )
}

/// Read a target's HEALTH — current and max, through the Ess wrappers that front the engine's
/// `GetHealth`/`GetMaxHealth`. Health is the lever the engine converts into a destruction transition,
/// so reading it is the companion to reading state. A per-node read (`Object.GetNodeHealth`) is
/// emitted as an optional commented line, because a node index is entity-specific.
pub fn read_health_lua(target: &str) -> String {
    let g = guid_expr(target);
    format!(
        "-- read health (cur / max); the engine turns 0 health into a Destroyed transition\n\
         local g = {g}\n\
         Loader.Printf(\"health: %s / %s\", tostring(Ess.Object.health(g)), \
         tostring(Ess.Object.maxHealth(g)))\n\
         -- per-node: Object.GetNodeHealth(g, <nodeIndex>)\n"
    )
}

/// Demolish a target now — the engine's force-destroy path, reached through the health lever Ess
/// already wraps (`Object.Kill` → `FUN_004d05c0` → `SetState(InitDestroyedState)` + break pieces).
pub fn demolish_lua(target: &str) -> String {
    format!(
        "-- demolish: drive to DestroyedState (force-destroy + break pieces)\nEss.Object.kill({})\n",
        guid_expr(target)
    )
}

/// Repair a target — restore full health and revive it, which re-activates the machine at its
/// default (pristine) state.
pub fn repair_lua(target: &str) -> String {
    let g = guid_expr(target);
    format!(
        "-- repair: full health + revive (machine re-activates at its default state)\n\
         local g = {g}\nEss.Object.setHealth(g, Ess.Object.maxHealth(g))\nEss.Object.revive(g)\n"
    )
}

/// Install a reporter for `OnStateChange(guid, node, state)` — the Lua callback the engine fires on
/// every destruction transition — printing each with the state resolved to its vocabulary NAME. This
/// is the read side Ess has no equivalent for: watch the world's destructibles change state, legibly.
pub fn watch_lua() -> String {
    let mut table = String::from("local NAMES = {\n");
    for (name, hash) in states() {
        table.push_str(&format!("  [0x{hash:08X}] = \"{name}\",\n"));
    }
    table.push_str("}\n");
    format!(
        "{table}\
         local _prev = OnStateChange\n\
         function OnStateChange(guid, node, state)\n\
         \x20 if _prev then _prev(guid, node, state) end\n\
         \x20 local s = NAMES[state] or string.format(\"0x%08X\", state)\n\
         \x20 Loader.Printf(\"destruct: 0x%08X node 0x%08X -> %s\", guid, node, s)\n\
         end\n\
         Loader.Printf(\"destruct watcher installed\")\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_are_the_cracked_vocabulary() {
        let s = states();
        assert!(s.iter().any(|(n, h)| *n == "PristineState" && *h == 0xACB5_1200));
        assert!(s.iter().any(|(n, h)| *n == "DestroyedState" && *h == 0x7687_DF41));
        // Unnamed observed states are not offered as menu items.
        assert!(s.iter().all(|(n, _)| !n.is_empty()));
    }

    #[test]
    fn set_state_emits_the_right_hash_and_refuses_a_bad_name() {
        let lua = set_state_lua("mytank", "PristineState").unwrap();
        assert!(lua.contains("Object.SetState(Ess.Guid(\"mytank\"), 0xACB51200)"), "{lua}");
        assert!(lua.contains("PristineState"), "{lua}");
        // A guid expression is passed through, not re-wrapped.
        assert!(set_state_lua("g", "DestroyedState").unwrap().contains("Object.SetState(Ess.Guid(\"g\")"));
        assert!(set_state_lua("Ess.Player.guid()", "GoneState").unwrap().contains("Object.SetState(Ess.Player.guid(), "));
        // A name outside the vocabulary is refused with the list.
        let err = set_state_lua("t", "KaboomState").unwrap_err();
        assert!(err.contains("PristineState") && err.contains("not a known"), "{err}");
    }

    #[test]
    fn demolish_repair_and_watch_generate_reachable_calls() {
        assert!(demolish_lua("t").contains("Ess.Object.kill(Ess.Guid(\"t\"))"));
        let r = repair_lua("t");
        assert!(r.contains("Ess.Object.setHealth(g, Ess.Object.maxHealth(g))") && r.contains("Ess.Object.revive(g)"), "{r}");
        let w = watch_lua();
        assert!(w.contains("[0xACB51200] = \"PristineState\""), "{w}");
        assert!(w.contains("function OnStateChange(guid, node, state)"), "{w}");
    }

    #[test]
    fn reads_use_the_real_getters_and_resolve_state_names() {
        let s = read_state_lua("mytank");
        // The guid is bound once and read through `g`.
        assert!(s.contains("local g = Ess.Guid(\"mytank\")"), "{s}");
        assert!(s.contains("Object.GetState(g)"), "{s}");
        // Resolves to the vocabulary name, and falls back to the engine's own GetStateName.
        assert!(s.contains("[0xACB51200] = \"PristineState\""), "{s}");
        assert!(s.contains("Object.GetStateName"), "{s}");
        let h = read_health_lua("t");
        assert!(h.contains("local g = Ess.Guid(\"t\")"), "{h}");
        assert!(h.contains("Ess.Object.health(g)") && h.contains("Ess.Object.maxHealth(g)"), "{h}");
        // A guid expression is passed through, not re-wrapped.
        assert!(read_state_lua("Ess.Player.guid()").contains("local g = Ess.Player.guid()"));
    }

    #[test]
    fn guid_expr_wraps_names_but_passes_expressions() {
        assert_eq!(guid_expr("tank"), "Ess.Guid(\"tank\")");
        assert_eq!(guid_expr("g"), "Ess.Guid(\"g\")");
        assert_eq!(guid_expr("Ess.Guid(\"x\")"), "Ess.Guid(\"x\")");
        assert_eq!(guid_expr("myVar.guid"), "myVar.guid");
        assert_eq!(guid_expr("12345"), "12345");
    }
}
