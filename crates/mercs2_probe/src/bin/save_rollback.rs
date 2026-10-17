//! `save_rollback` — trim a Mercenaries 2 `.profile` back to **just before Act 2**.
//!
//! The gate: Act 2 opens when the `AllChiIntro` key fires in `wifmissionflow.lua:423-437`,
//! which calls `SetAttitudeMutable("All"|"Chi")` and `UnlockMission("AllCon050"|"ChiCon050")`.
//! Carmona swaps `vz_state_car_city_act1` → `vz_state_car_city_act2all` in `AllCon001`'s
//! fConseq (`wifmissionflow.lua:1002-1012`). The reward table itself names its tiers
//! `chapter_one_*` / `chapter_two_*` (`mrxrewarddata.lua:17-46`).
//!
//! This bin parses the Lua payload into a proper AST (`mercs2_formats::save_lua`),
//! does surgical tree edits, serializes back, and verifies the output is still a
//! loadable save (`Profile::hash_ok` + a Lua reference parse). An earlier
//! text-splice version silently corrupted the structure in a way the retail
//! engine's `loadstring` interpreted as "fresh start" — proper AST round-trip
//! kills that whole class of bug.
//!
//! Edits applied:
//!   - `tFlowData.tMyFlowData`: drop every `AllCon*` / `AllJob*` / `ChiCon*` /
//!     `ChiJob*` key and the `AllChiIntro` gate key
//!   - `tFlowData.tActiveMissions`: drop entries with Act-2 keys
//!   - `tFlowData.tCulledBindings`: drop Act-2 bindings + `AllOutposts` +
//!     `AllCon050_ChiCon050`, renumber the array dense
//!   - `tLayerData`: drop any `_act2chi*` / `_act2all*` overlays, renumber
//!   - `tStarterData`: drop `AllStarter*` / `ChiStarter*` recruit entries
//!   - `oVerifyData.tTargetListStatus["All"]` / `["Chi"]`: clear per-NPC state
//!
//! Preserved: hero + upgrade + costumes + cash + fuel + vEquippedSupport +
//! playtime + Oil/Gur/Pir/Pmc Act-1 progression + non-Act-2 flow keys.

use mercs2_formats::save::{self, parse};
use mercs2_formats::save_lua::{self, SerializeOptions, Table, Value};
use mercs2_formats::save_write;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!(
            "usage: save_rollback <input.profile> <output.profile>\n\
             \n\
             Trims a Mercenaries 2 save back to just before Act 2 opens (AllChiIntro).\n\
             Input is read-only; output is written to the second path. Prints sha256\n\
             of both files and a trim report."
        );
        std::process::exit(2);
    }
    let input = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);

    if let Err(e) = run(&input, &output) {
        eprintln!("save_rollback failed: {e}");
        std::process::exit(1);
    }
}

fn run(input: &PathBuf, output: &PathBuf) -> Result<(), String> {
    let bytes = fs::read(input).map_err(|e| format!("read {}: {e}", input.display()))?;
    let in_sha = sha256_hex(&bytes);
    eprintln!(
        "[save_rollback] input  : {} ({} bytes)",
        input.display(),
        bytes.len()
    );
    eprintln!("[save_rollback] input  sha256 : {in_sha}");

    let mut profile = parse(&bytes).map_err(|e| format!("parse: {e}"))?;
    if !profile.hash_ok() {
        return Err("input ProfileHash is invalid — refusing to touch a corrupt save".into());
    }

    let lua_bytes = profile.decompress_lua()?;
    let text = std::str::from_utf8(&lua_bytes)
        .map_err(|e| format!("Lua payload is not UTF-8: {e}"))?;

    let mut root = save_lua::parse_chunk(text).map_err(|e| format!("parse Lua AST: {e}"))?;
    let (before, after, trim) = {
        let table = root
            .as_table_mut()
            .map_err(|e| format!("top-level Lua is not a table: {e}"))?;
        let before = snapshot(table);
        let trim = trim_act2(table)?;
        let after = snapshot(table);
        (before, after, trim)
    };

    // Serialize. The on-disk zlib stream contains JUST the `{ … }` table — the
    // engine's load path is effectively `loadstring("return " .. inflated_text)`,
    // so a `return` prefix in the on-disk text would produce
    // `"return return { … }"` on the engine side, a syntax error that silently
    // yields `tSaveData = nil` and sends the player back to `VzaCon001` via the
    // fresh-start fallback in `xQ!L.lua:669`. Verified against every vendored
    // retail save: their inflated bytes start with `{`, not `return `.
    //
    // The engine's SAVE path (`FUN_005a4520` in save_serialize_code_map.md §2.1)
    // allocates a chunk and writes `"return "` into it, but that chunk is used
    // for the live `loadstring` dispatch, not the on-disk zlib body — the
    // save-side writer wraps it, the load-side reader wraps it, and the stored
    // payload is the bare table expression.
    let new_text = save_lua::serialize_with(&root, SerializeOptions { emit_return: false });

    // Round-trip guard: our own parser must accept what we just wrote. The
    // on-disk payload is a bare table expression — we feed it to `parse_value`,
    // not `parse_chunk` (which expects a `return` prefix).
    let round = save_lua::parse_value(&new_text)
        .map_err(|e| format!("self-check: our serializer produced unparseable Lua: {e}"))?;
    if round != root {
        return Err("self-check: serialize→parse differs from original AST".into());
    }

    // Simulate the engine's actual load wrapper: `loadstring("return " .. body)`.
    // If this doesn't compile, the game sees `tSaveData = nil` and plays the
    // VzaCon001 fresh-start — the exact regression this bin was built to fix.
    let engine_input = format!("return {new_text}");
    mercs2_luac::compile(&engine_input, "save_rollback_engine_load")
        .map_err(|e| format!("engine-load simulation failed to compile: {e}"))?;

    save_write::set_lua_payload(&mut profile, new_text.as_bytes())?;
    let out_bytes = save_write::write_profile(&profile);

    // On-disk guard: re-parse the whole profile and validate ProfileHash.
    let round_prof = parse(&out_bytes).map_err(|e| format!("output re-parse: {e}"))?;
    if !round_prof.hash_ok() {
        return Err("written ProfileHash fails Profile::hash_ok (writer bug)".into());
    }

    fs::write(output, &out_bytes).map_err(|e| format!("write {}: {e}", output.display()))?;
    let out_sha = sha256_hex(&out_bytes);

    eprintln!(
        "[save_rollback] output : {} ({} bytes)",
        output.display(),
        out_bytes.len()
    );
    eprintln!("[save_rollback] output sha256 : {out_sha}");
    eprintln!("[save_rollback] trim report:");
    eprintln!("  flow keys dropped       : {}", trim.flow_dropped);
    eprintln!("  active missions dropped : {}", trim.active_dropped);
    eprintln!("  culled bindings dropped : {}", trim.culled_dropped);
    eprintln!("  layers dropped          : {}", trim.layers_dropped);
    eprintln!("  starters dropped        : {}", trim.starters_dropped);
    eprintln!("  verify-tables cleared   : {}", trim.verify_cleared);
    eprintln!("  --");
    eprintln!(
        "  flow keys  : {} -> {}",
        before.flow_keys, after.flow_keys
    );
    eprintln!(
        "  active     : {} -> {}",
        before.active_missions, after.active_missions
    );
    eprintln!(
        "  culled     : {} -> {}",
        before.culled_bindings, after.culled_bindings
    );
    eprintln!(
        "  layers     : {} -> {}",
        before.layers, after.layers
    );
    eprintln!(
        "  starters   : {} -> {}",
        before.starters, after.starters
    );

    // Loud witnesses on the written profile — not on the mid-pipeline AST.
    verify_output(&round_prof)?;
    report_header(&profile);
    Ok(())
}

#[derive(Default, Debug)]
struct Snapshot {
    flow_keys: usize,
    active_missions: usize,
    culled_bindings: usize,
    layers: usize,
    starters: usize,
}

fn snapshot(root: &Table) -> Snapshot {
    let flow = root.get("tFlowData").and_then(|v| v.as_table().ok());
    Snapshot {
        flow_keys: flow
            .and_then(|t| t.get("tMyFlowData"))
            .and_then(|v| v.as_table().ok())
            .map(|t| t.entries.len())
            .unwrap_or(0),
        active_missions: flow
            .and_then(|t| t.get("tActiveMissions"))
            .and_then(|v| v.as_table().ok())
            .map(|t| t.entries.len())
            .unwrap_or(0),
        culled_bindings: flow
            .and_then(|t| t.get("tCulledBindings"))
            .and_then(|v| v.as_table().ok())
            .map(|t| t.entries.len())
            .unwrap_or(0),
        layers: root
            .get("tLayerData")
            .and_then(|v| v.as_table().ok())
            .map(|t| t.entries.len())
            .unwrap_or(0),
        starters: root
            .get("tStarterData")
            .and_then(|v| v.as_table().ok())
            .map(|t| t.entries.len())
            .unwrap_or(0),
    }
}

#[derive(Default, Debug)]
struct TrimReport {
    flow_dropped: usize,
    active_dropped: usize,
    culled_dropped: usize,
    layers_dropped: usize,
    starters_dropped: usize,
    verify_cleared: usize,
}

/// Perform the Act-2 trim on the parsed AST in place.
fn trim_act2(root: &mut Table) -> Result<TrimReport, String> {
    let mut r = TrimReport::default();

    // tFlowData: drop by key / by binding name on three child tables.
    if let Some(flow) = root.get_mut("tFlowData") {
        let flow = flow.as_table_mut().map_err(|e| format!("tFlowData: {e}"))?;

        if let Some(my) = flow.get_mut("tMyFlowData") {
            let my = my.as_table_mut().map_err(|e| format!("tMyFlowData: {e}"))?;
            r.flow_dropped = my.retain(|e| match &e.key {
                Some(Value::String(s)) => !is_act2_key(&s.value),
                _ => true,
            });
        }

        if let Some(active) = flow.get_mut("tActiveMissions") {
            let active = active
                .as_table_mut()
                .map_err(|e| format!("tActiveMissions: {e}"))?;
            r.active_dropped = active.retain(|e| match &e.key {
                Some(Value::String(s)) => !is_act2_key(&s.value),
                _ => true,
            });
        }

        if let Some(culled) = flow.get_mut("tCulledBindings") {
            let culled = culled
                .as_table_mut()
                .map_err(|e| format!("tCulledBindings: {e}"))?;
            r.culled_dropped = culled.retain(|e| match &e.value {
                Value::String(s) => !is_act2_binding(&s.value),
                _ => true,
            });
            culled.renumber_as_array();
        }
    }

    // tLayerData — drop east-side Act-2 overlays, renumber the dense array.
    if let Some(layers) = root.get_mut("tLayerData") {
        let layers = layers.as_table_mut().map_err(|e| format!("tLayerData: {e}"))?;
        r.layers_dropped = layers.retain(|e| match &e.value {
            Value::String(s) => !is_act2_layer(&s.value),
            _ => true,
        });
        layers.renumber_as_array();
    }

    // tStarterData — drop Allied / Chinese recruit subtables.
    if let Some(starters) = root.get_mut("tStarterData") {
        let starters = starters
            .as_table_mut()
            .map_err(|e| format!("tStarterData: {e}"))?;
        r.starters_dropped = starters.retain(|e| match &e.key {
            Some(Value::String(s)) => {
                !(s.value.starts_with("AllStarter") || s.value.starts_with("ChiStarter"))
            }
            _ => true,
        });
    }

    // tStatsData.oVerifyData.tTargetListStatus["All"] / ["Chi"] — clear per-NPC
    // killed/captured state.
    //
    // If AllChiIntro re-fires on replay and (say) AllCon050's outpost unlocks, the
    // mission would see pre-marked targets and either auto-complete or wedge. Clearing
    // the subtable body to {} resets that per-NPC state cleanly without disturbing
    // the parent structure the game expects.
    //
    // oVerifyData lives on the MrxVerifyManager singleton, which the engine's
    // SaveSingleton pipeline writes under `tStatsData` (the stats-and-persistence
    // bucket) — not at the top level. The dumped-text view is unindented, which is
    // what tripped the earlier text-splice version.
    if let Some(stats) = root.get_mut("tStatsData") {
        let stats = stats.as_table_mut().map_err(|e| format!("tStatsData: {e}"))?;
        if let Some(ov) = stats.get_mut("oVerifyData") {
            let ov = ov.as_table_mut().map_err(|e| format!("oVerifyData: {e}"))?;
            if let Some(tls) = ov.get_mut("tTargetListStatus") {
                let tls = tls
                    .as_table_mut()
                    .map_err(|e| format!("tTargetListStatus: {e}"))?;
                for faction in ["All", "Chi"] {
                    if let Some(Value::Table(t)) = tls.get_mut(faction) {
                        let before = t.entries.len();
                        t.entries.clear();
                        if before > 0 {
                            r.verify_cleared += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(r)
}

fn verify_output(p: &save::Profile) -> Result<(), String> {
    let lua = p.decompress_lua()?;
    let text = std::str::from_utf8(&lua).map_err(|e| format!("output utf8: {e}"))?;
    let root = save_lua::parse_chunk(text).map_err(|e| format!("output parse: {e}"))?;
    let root = root
        .as_table()
        .map_err(|e| format!("output top-level: {e}"))?;

    let flow = root
        .get("tFlowData")
        .and_then(|v| v.as_table().ok())
        .ok_or("output has no tFlowData")?;

    // 1. No Act-2 flow keys.
    if let Some(my) = flow.get("tMyFlowData").and_then(|v| v.as_table().ok()) {
        let act2_survived: Vec<_> = my
            .iter_str()
            .map(|(k, _)| k)
            .filter(|k| is_act2_key(k))
            .collect();
        if !act2_survived.is_empty() {
            return Err(format!("Act-2 keys survived: {act2_survived:?}"));
        }
        // Must still carry the critical Act-1 anchors.
        for req in ["VzaCon001", "PmcCon001"] {
            if my.get(req).is_none() {
                return Err(format!("Act-1 anchor {req} missing from tMyFlowData"));
            }
        }
    } else {
        return Err("output has no tFlowData.tMyFlowData".into());
    }

    // 2. tCulledBindings is a dense 1-based array (ipairs semantics).
    if let Some(cb) = flow.get("tCulledBindings").and_then(|v| v.as_table().ok()) {
        for (i, e) in cb.entries.iter().enumerate() {
            match &e.key {
                Some(Value::Number(n)) if (n.value - (i as f64 + 1.0)).abs() < 1e-9 => {}
                other => {
                    return Err(format!(
                        "tCulledBindings[{i}] key is not {}.0 — got {other:?}",
                        i + 1
                    ));
                }
            }
            match &e.value {
                Value::String(s) if !is_act2_binding(&s.value) => {}
                Value::String(s) => {
                    return Err(format!(
                        "tCulledBindings has surviving Act-2 binding {:?}",
                        s.value
                    ));
                }
                other => {
                    return Err(format!(
                        "tCulledBindings[{i}] value is not a string: {other:?}"
                    ));
                }
            }
        }
    }

    // 3. No east-Carmona Act-2 overlays in tLayerData.
    if let Some(layers) = root.get("tLayerData").and_then(|v| v.as_table().ok()) {
        for e in &layers.entries {
            if let Value::String(s) = &e.value {
                if is_act2_layer(&s.value) {
                    return Err(format!("Act-2 layer survived: {}", s.value));
                }
            }
        }
    }

    // 4. No Act-2 starters.
    if let Some(st) = root.get("tStarterData").and_then(|v| v.as_table().ok()) {
        let act2: Vec<_> = st
            .iter_str()
            .map(|(k, _)| k)
            .filter(|k| k.starts_with("AllStarter") || k.starts_with("ChiStarter"))
            .collect();
        if !act2.is_empty() {
            return Err(format!("Act-2 starters survived: {act2:?}"));
        }
    }

    Ok(())
}

fn report_header(p: &save::Profile) {
    eprintln!(
        "[save_rollback] header: hero={} upgrade={} costumes={} cash={} fuel={}/{} contract={} saveName={}",
        p.character_index,
        p.upgrade_index,
        p.unlocked_costumes,
        p.cash,
        p.fuel,
        p.fuel_capacity,
        p.active_contract(),
        p.save_name()
    );
}

/// Act-2 flow-key test. Matches the Allied + Chinese branch exactly.
fn is_act2_key(key: &str) -> bool {
    key == "AllChiIntro"
        || key.starts_with("AllCon")
        || key.starts_with("AllJob")
        || key.starts_with("ChiCon")
        || key.starts_with("ChiJob")
}

/// A `tCulledBindings` entry is Act-2 if its binding name is an Act-2 key OR one
/// of the composite Act-2 bindings observed in `wifmissionflow.lua`.
fn is_act2_binding(name: &str) -> bool {
    is_act2_key(name) || name == "AllOutposts" || name == "AllCon050_ChiCon050"
}

/// Act-2 Allied / Chinese layer (any area prefix — Carmona, Cumana, …).
fn is_act2_layer(layer: &str) -> bool {
    layer.contains("_act2chi") || layer.contains("_act2all")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest.iter() {
        use std::fmt::Write;
        write!(&mut s, "{b:02x}").unwrap();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::save::parse;
    use mercs2_formats::save_lua::{parse_chunk, Entry, LuaString, Number};

    #[test]
    fn is_act2_classifier() {
        for k in [
            "AllChiIntro",
            "AllCon001",
            "ChiJob002_Milestone5",
            "AllJob020",
        ] {
            assert!(is_act2_key(k));
        }
        for k in [
            "VzaCon001",
            "PmcCon001",
            "OilCon050",
            "GurIntro",
            "PirIntro",
            "PirCon001",
            "Invasion",
        ] {
            assert!(!is_act2_key(k));
        }
    }

    #[test]
    fn end_to_end_against_the_vendored_fixture() {
        // The user's save IS the vendored end-game fixture.
        let fixtures = mercs2_formats::game_paths::save_fixtures();
        let path = fixtures.join("Mattias Nilsson_6A0E523C.profile");
        let bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        let mut profile = parse(&bytes).unwrap();
        let lua = profile.decompress_lua().unwrap();
        let text = std::str::from_utf8(&lua).unwrap();

        let mut root = parse_chunk(text).unwrap();
        let before = {
            let t = root.as_table().unwrap();
            snapshot(t)
        };
        let trim = {
            let t = root.as_table_mut().unwrap();
            trim_act2(t).unwrap()
        };
        let after = {
            let t = root.as_table().unwrap();
            snapshot(t)
        };

        assert!(trim.flow_dropped >= 20, "flow_dropped {} < 20", trim.flow_dropped);
        assert!(trim.layers_dropped >= 2, "layers_dropped {}", trim.layers_dropped);
        assert!(trim.starters_dropped >= 5, "starters_dropped {}", trim.starters_dropped);
        assert_eq!(trim.verify_cleared, 2, "both All+Chi verify tables cleared");

        assert!(after.flow_keys < before.flow_keys);
        assert!(after.active_missions < before.active_missions);
        assert!(after.culled_bindings < before.culled_bindings);
        assert!(after.layers < before.layers);
        assert!(after.starters < before.starters);

        // Serialize AS A BARE BODY — the engine prepends `return ` at load time.
        let out_text = save_lua::serialize_with(
            &root,
            SerializeOptions { emit_return: false },
        );
        assert!(
            out_text.trim_start().starts_with('{'),
            "output must start with a bare `{{`, got {:?}",
            &out_text[..out_text.len().min(32)]
        );

        // Acid test: simulate the engine's `loadstring("return " .. body)` and
        // confirm it compiles under the Lua 5.1.5 reference. A chunk the engine
        // can't `loadstring` yields `tSaveData = nil` → fresh-start intro —
        // exactly the regression this test guards against.
        let engine_input = format!("return {out_text}");
        mercs2_luac::compile(&engine_input, "save_rollback_engine_load")
            .unwrap_or_else(|e| panic!("engine-load simulation failed to compile: {e}"));

        save_write::set_lua_payload(&mut profile, out_text.as_bytes()).unwrap();
        let out_bytes = save_write::write_profile(&profile);

        let round = parse(&out_bytes).unwrap();
        assert!(round.hash_ok(), "ProfileHash must validate on the written file");

        // Our own verifier passes on the written file.
        verify_output(&round).unwrap();

        // Act-1 keys the player must keep.
        let round_lua = round.decompress_lua().unwrap();
        let round_text = std::str::from_utf8(&round_lua).unwrap();
        let r_root = parse_chunk(round_text).unwrap();
        let r_root = r_root.as_table().unwrap();
        let flow = r_root
            .get("tFlowData")
            .and_then(|v| v.as_table().ok())
            .unwrap();
        let my = flow
            .get("tMyFlowData")
            .and_then(|v| v.as_table().ok())
            .unwrap();
        for req in [
            "VzaCon001",
            "PmcCon001",
            "PmcCon002",
            "PmcCon003",
            "OilCon050",
            "GurIntro",
            "MecIntro",
            "JetIntro",
            "PirIntro",
        ] {
            assert!(
                my.get(req).is_some(),
                "Act-1 key {req} must survive the trim"
            );
        }

        // Header fields preserved.
        assert_eq!(round.character_index, 1);
        assert_eq!(round.upgrade_index, 3);
        assert_eq!(round.unlocked_costumes, 5);
    }

    #[test]
    fn trim_a_synthetic_tree_cleanly() {
        let src = r#"return {
["tFlowData"] = {
["tCulledBindings"] = {
[1.000000] = "Start",
[2.000000] = "AllChiIntro",
[3.000000] = "VzaCon001",
[4.000000] = "AllOutposts",
},
["tActiveMissions"] = {
["PmcJob001"] = { ["nState"] = 1.000000 },
["AllCon001"] = { ["nState"] = 0.000000 },
},
["tMyFlowData"] = {
["VzaCon001"] = 1.000000,
["PmcCon001"] = 1.000000,
["AllChiIntro"] = 1.000000,
["AllCon001"] = 1.000000,
["ChiJob002"] = 1.000000,
},
},
["tLayerData"] = {
[1.000000] = "vz_state_car_city_act1",
[2.000000] = "vz_state_car_city_act2chi",
[3.000000] = "vz_state_cumana_act2chi",
},
["tStarterData"] = {
["PmcBoss"] = { },
["AllStarter0"] = { },
["ChiStarter4"] = { },
},
}"#;
        let mut v = parse_chunk(src).unwrap();
        let t = v.as_table_mut().unwrap();
        let r = trim_act2(t).unwrap();
        assert_eq!(r.flow_dropped, 3, "AllChiIntro+AllCon001+ChiJob002");
        assert_eq!(r.active_dropped, 1, "AllCon001 active");
        assert_eq!(r.culled_dropped, 2, "AllChiIntro+AllOutposts");
        assert_eq!(r.layers_dropped, 2, "two _act2chi layers");
        assert_eq!(r.starters_dropped, 2, "AllStarter0+ChiStarter4");

        // tCulledBindings must be dense [1]="Start", [2]="VzaCon001".
        let cb = t
            .get("tFlowData")
            .and_then(|v| v.as_table().ok())
            .unwrap()
            .get("tCulledBindings")
            .and_then(|v| v.as_table().ok())
            .unwrap();
        assert_eq!(cb.entries.len(), 2);
        if let Some(Value::Number(n)) = &cb.entries[0].key {
            assert_eq!(n.value, 1.0);
        } else {
            panic!();
        }
        if let Value::String(s) = &cb.entries[1].value {
            assert_eq!(s.value, "VzaCon001");
        } else {
            panic!();
        }

        // Round-trip through text in the on-disk form (bare body, no `return`).
        let out = save_lua::serialize_with(&v, SerializeOptions { emit_return: false });
        let back = save_lua::parse_value(&out).unwrap();
        assert_eq!(v, back);
    }

    /// Prevent unused-import warning on helpers only touched inside tests.
    #[test]
    fn _touch_helpers() {
        let _ = Entry::keyed("x", Value::Nil);
        let _ = Number::index(7);
        let _ = LuaString::new("hi");
    }
}
