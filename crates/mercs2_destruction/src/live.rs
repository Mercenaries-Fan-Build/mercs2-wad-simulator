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
//!
//! ★ **Bindings verified live over the bridge (2026-08-02).** The write path is
//! `ObjectState.SetState(guid, nodeHash, stateHash)` — **3-arg and node-keyed** (this file previously
//! emitted `Object.SetState(guid, state)`, which is *not a real native* and silently did nothing). There
//! is **no Lua getter** for an object's destruction state — the read side is `ObjectState.PrintStateMachine`
//! (a log dump) plus the `OnStateChange` watcher. And `SetState` drives the machine's LOGICAL state (it
//! fires `OnStateChange`) but is NOT a destroy: the object stays alive. Visible destruction is the health
//! path (`Ess.Object.kill`, asynchronous break-pieces), and `StartDestroyedState` is the state that plays
//! the wreck. The state/node hashes arrive at `OnStateChange` as GUIDs, so names resolve through
//! `Sys.GuidToString` against a STRING-keyed table (a numeric-keyed one collides under Lua 5.1 floats).

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

/// Turn a user-typed node into the Lua hash expression `ObjectState.SetState` wants. A bare word is a
/// node / hardpoint NAME, hashed in-engine with `String.GetHash`; a `0x…` is turned back into the hash
/// value with `Sys.StringToGuid` — the same split [`guid_expr`] makes, and the shape verified live (a name
/// node and a `0x…` node both drove a real building's machine over the bridge).
pub fn node_expr(node: &str) -> String {
    let n = node.trim();
    if n.starts_with("0x") || n.starts_with("0X") {
        format!("Sys.StringToGuid(\"{n}\")")
    } else {
        format!("String.GetHash(\"{n}\")")
    }
}

/// Force a destructible NODE into a named state via `ObjectState.SetState(guid, node, state)` — the real
/// 3-arg, node-keyed native (verified live; the `Object.SetState(guid, state)` this once emitted does not
/// exist). `state` must be in the cracked vocabulary. A destructible has MANY nodes (a building's structural
/// pieces), and `SetState` is per-node — enumerate them with [`print_machine_lua`] or [`watch_lua`]; node
/// `0x0` is not valid. This drives the machine's LOGICAL state and fires `OnStateChange`; it is NOT a destroy
/// (the object stays alive) — for visible destruction use [`demolish_lua`], and `StartDestroyedState` is the
/// state that plays the wreck. The chunk carries the resolved name + hash as a comment for a legible log.
pub fn set_state_lua(target: &str, node: &str, state: &str) -> Result<String, String> {
    if node.trim().is_empty() {
        return Err("a node is required — ObjectState.SetState is node-keyed; use \"Dump machine\" \
                    to list a destructible's nodes"
            .into());
    }
    let h = state_hash(state).ok_or_else(|| {
        let known: Vec<&str> = states().iter().map(|(n, _)| *n).collect();
        format!("{state:?} is not a known destruction state. Pick one of: {}", known.join(", "))
    })?;
    let named = state_name(h).unwrap_or(state);
    Ok(format!(
        "-- force {named} (0x{h:08X}) on node {node} of the target (logical state, not a destroy)\n\
         ObjectState.SetState({}, {}, String.GetHash(\"{named}\"))\n",
        guid_expr(target),
        node_expr(node),
    ))
}

/// Dump a target's live state machine — every node and its current state — to the game log via the
/// engine's own `ObjectState.PrintStateMachine`. This is how you INSPECT: there is no Lua getter for an
/// object's destruction state (the `Object.GetState`/`GetStateName` this once used do not exist — verified
/// against the live capture), so the read side is this dump plus the reactive [`watch_lua`]. It is also how
/// you discover the node hashes [`set_state_lua`] needs.
pub fn print_machine_lua(target: &str) -> String {
    format!(
        "-- dump the machine (nodes + current states) to the log; there is no state getter to return one\n\
         ObjectState.PrintStateMachine({})\n",
        guid_expr(target)
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

/// Install a reporter for `OnStateChange(guid, node, state)` — the Lua callback the engine fires on every
/// destruction transition — printing each with the state resolved to its vocabulary NAME. This is the read
/// side Ess has no equivalent for: watch the world's destructibles change state, legibly. Verified live —
/// the engine calls this global for every destructible as the world streams, and chaining `_prev` extends
/// any existing handler rather than clobbering it.
///
/// The args arrive as GUIDs, so this stringifies them with `Sys.GuidToString` and looks names up in a
/// STRING-keyed table: a numeric-keyed table would never match (the key is a guid, not a number) and
/// `string.format("%X", guid)` would error. Unknown states fall back to the bare `0x…`, never a guess.
pub fn watch_lua() -> String {
    let mut table = String::from("local NAMES = {\n");
    for (name, hash) in states() {
        table.push_str(&format!("  [\"0x{hash:08X}\"] = \"{name}\",\n"));
    }
    table.push_str("}\n");
    format!(
        "{table}\
         local _prev = OnStateChange\n\
         function OnStateChange(guid, node, state)\n\
         \x20 if _prev then _prev(guid, node, state) end\n\
         \x20 local ss = Sys.GuidToString(state)\n\
         \x20 Loader.Printf(\"destruct: %s node %s -> %s\", Sys.GuidToString(guid), Sys.GuidToString(node), NAMES[ss] or ss)\n\
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
    fn set_state_is_node_keyed_on_the_real_native_and_refuses_bad_input() {
        // Pin the WHOLE emitted chunk, not just fragments: a node NAME is hashed with String.GetHash, the
        // state carried as String.GetHash("Name"), on the real 3-arg node-keyed ObjectState.SetState.
        assert_eq!(
            set_state_lua("mytank", "hp_snap_tower", "PristineState").unwrap(),
            "-- force PristineState (0xACB51200) on node hp_snap_tower of the target (logical state, not a destroy)\n\
             ObjectState.SetState(Ess.Guid(\"mytank\"), String.GetHash(\"hp_snap_tower\"), String.GetHash(\"PristineState\"))\n",
        );
        // A 0x… node goes through Sys.StringToGuid; a guid-expression target is passed through, not re-wrapped.
        assert_eq!(
            set_state_lua("Ess.Player.guid()", "0x8DCB305A", "DestroyedState").unwrap(),
            "-- force DestroyedState (0x7687DF41) on node 0x8DCB305A of the target (logical state, not a destroy)\n\
             ObjectState.SetState(Ess.Player.guid(), Sys.StringToGuid(\"0x8DCB305A\"), String.GetHash(\"DestroyedState\"))\n",
        );
        // Belt-and-suspenders: the nonexistent 2-arg Object.SetState this used to emit is gone (this alone
        // proves nothing — the exact-match assertions above are what pin the correct call).
        assert!(!set_state_lua("t", "n", "GoneState").unwrap().contains("Object.SetState("));
        // A missing node is refused — SetState is node-keyed.
        assert!(set_state_lua("t", "", "PristineState").unwrap_err().contains("node is required"));
        // A name outside the vocabulary is refused with the list.
        let err = set_state_lua("t", "n", "KaboomState").unwrap_err();
        assert!(err.contains("PristineState") && err.contains("not a known"), "{err}");
    }

    #[test]
    fn demolish_repair_and_watch_generate_reachable_calls() {
        assert!(demolish_lua("t").contains("Ess.Object.kill(Ess.Guid(\"t\"))"));
        let r = repair_lua("t");
        assert!(r.contains("Ess.Object.setHealth(g, Ess.Object.maxHealth(g))") && r.contains("Ess.Object.revive(g)"), "{r}");
        let w = watch_lua();
        // Pin the exact fixed logic: STRING-keyed NAMES (the callback args are GUIDs), and every arg
        // stringified through Sys.GuidToString — not a numeric key or a raw-guid %X format.
        assert!(w.contains("  [\"0xACB51200\"] = \"PristineState\",\n"), "{w}");
        assert!(w.contains("function OnStateChange(guid, node, state)\n"), "{w}");
        assert!(w.contains("local ss = Sys.GuidToString(state)\n"), "{w}");
        assert!(
            w.contains(
                "Loader.Printf(\"destruct: %s node %s -> %s\", Sys.GuidToString(guid), Sys.GuidToString(node), NAMES[ss] or ss)\n"
            ),
            "{w}"
        );
        // The broken numeric key / raw-guid format must be gone (redundant given the exact positives above).
        assert!(!w.contains("[0xACB51200]") && !w.contains("string.format(\"0x%08X\", state)"), "{w}");
    }

    #[test]
    fn inspect_dumps_the_machine_since_there_is_no_state_getter() {
        // There is no Object.GetState — inspecting is a PrintStateMachine dump. Pin the whole chunk.
        assert_eq!(
            print_machine_lua("mytank"),
            "-- dump the machine (nodes + current states) to the log; there is no state getter to return one\n\
             ObjectState.PrintStateMachine(Ess.Guid(\"mytank\"))\n",
        );
        // A guid expression is passed through, not re-wrapped.
        assert_eq!(
            print_machine_lua("Ess.Player.guid()"),
            "-- dump the machine (nodes + current states) to the log; there is no state getter to return one\n\
             ObjectState.PrintStateMachine(Ess.Player.guid())\n",
        );
        // Health reads through the Ess wrappers (unchanged, and real — GetHealth exists).
        let h = read_health_lua("t");
        assert!(h.contains("local g = Ess.Guid(\"t\")"), "{h}");
        assert!(h.contains("Ess.Object.health(g)") && h.contains("Ess.Object.maxHealth(g)"), "{h}");
    }

    #[test]
    fn node_expr_hashes_names_but_stringtoguids_hashes() {
        assert_eq!(node_expr("hp_snap_tower"), "String.GetHash(\"hp_snap_tower\")");
        assert_eq!(node_expr("0x8DCB305A"), "Sys.StringToGuid(\"0x8DCB305A\")");
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
