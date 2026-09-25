//! `compat::plan`: requirements, versions, capabilities, conflicts, superseded files and the load
//! order, as `load-plan.json` states them.
//!
//! All hermetic. The shared fixtures under `tests/fixtures/load_plan/` (see its README) are also
//! used by `tests/cli.rs`; the golden plans are compared as parsed JSON with `quartermaster`
//! replaced by the running version. The rest build small Shipments in a scratch directory.

use mercs2_quartermaster::compat::{self, CompatError, PlanInput};
use mercs2_quartermaster::discover::{self, LoadedShipment};
use mercs2_quartermaster::link::{self, UiRegistration};
use mercs2_quartermaster::plan::{
    self, ConflictRow, DeclaredStatus, FindingSeverity, LoadPlan, Producer, RequirementStatus,
    Section,
};
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/load_plan")
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qm-compat-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A Shipment directory with this manifest body (everything after `format: 2`).
fn shipment_at(dir: &Path, body: &str) -> LoadedShipment {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("manifest.yaml"), format!("format: 2\n{body}")).unwrap();
    discover::open(dir).unwrap_or_else(|e| panic!("fixture must open: {e}\n{body}"))
}

/// A minimal Shipment: name, version and an optional `load:` block.
fn ship(root: &Path, name: &str, version: &str, load: &str) -> LoadedShipment {
    shipment_at(
        &root.join(format!("{name}-{version}")),
        &format!(
            "shipment: {{ name: {name}, version: {version}, target: retail }}\n{load}contributions: []\n"
        ),
    )
}

/// Plan over `shipments`, ids `shipment:<name>` (or `shipment:<name>#<n>` for a repeated name).
fn plan_of(shipments: &[&LoadedShipment], game_root: Option<&Path>) -> LoadPlan {
    let ids = ids_of(shipments);
    let inputs: Vec<PlanInput<'_>> = shipments
        .iter()
        .zip(&ids)
        .map(|(s, id)| PlanInput { id, shipment: s })
        .collect();
    compat::plan(&inputs, Producer::Preflight, game_root).expect("plan")
}

fn ids_of(shipments: &[&LoadedShipment]) -> Vec<String> {
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    shipments
        .iter()
        .map(|s| {
            let n = &s.manifest.shipment.name;
            let k = seen.entry(n.clone()).or_default();
            *k += 1;
            if *k == 1 {
                format!("shipment:{n}")
            } else {
                format!("shipment:{n}#{k}")
            }
        })
        .collect()
}

fn codes(p: &LoadPlan) -> Vec<&'static str> {
    p.findings.iter().map(|f| f.code).collect()
}

/// The header-only i386 DLL committed as the fixtures' plugin: `MZ`, `e_lfanew`, `PE\0\0` and the
/// COFF machine / characteristics words, which is all the load check reads. Pinned against the real
/// `pmc_bb.dll` v3.0.0 header (`e_lfanew=0x80, machine=0x014C, characteristics=0x230E`).
fn minimal_i386_dll() -> Vec<u8> {
    pe_image(0x014C, 0x230E)
}

fn pe_image(machine: u16, characteristics: u16) -> Vec<u8> {
    let pe_at = 0x80usize;
    let mut out = vec![0u8; pe_at + 24];
    out[0..2].copy_from_slice(b"MZ");
    out[0x3C..0x40].copy_from_slice(&(pe_at as u32).to_le_bytes());
    out[pe_at..pe_at + 4].copy_from_slice(b"PE\0\0");
    let coff = pe_at + 4;
    out[coff..coff + 2].copy_from_slice(&machine.to_le_bytes());
    out[coff + 18..coff + 20].copy_from_slice(&characteristics.to_le_bytes());
    out.extend_from_slice(b"load-plan-fixture-dll");
    out
}

/// The small uncompressed `GFX` movie committed as the Ess fixtures' `add_movie` source: an AVM1
/// `DoAction` and a `GFx_ExporterInfo`, the shape a GFx 2.x tool emits.
fn minimal_gfx() -> Vec<u8> {
    let mut body = vec![0u8];
    body.extend_from_slice(&(30u16 << 8).to_le_bytes());
    body.extend_from_slice(&1u16.to_le_bytes());
    for (code, b) in [
        (1000u16, &[0x07u8, 0x02, 0x00, 0x00][..]),
        (12, &[0x00]),
        (1, &[]),
        (0, &[]),
    ] {
        body.extend_from_slice(&((code << 6) | b.len() as u16).to_le_bytes());
        body.extend_from_slice(b);
    }
    let mut file = b"GFX".to_vec();
    file.push(8);
    file.extend_from_slice(&((8 + body.len()) as u32).to_le_bytes());
    file.extend_from_slice(&body);
    file
}

// ---------------------------------------------------------------------------
// The shared fixtures
// ---------------------------------------------------------------------------

/// The committed binaries are exactly what the generators above produce, so the fixture set can be
/// regenerated and nobody has to trust an opaque blob.
#[test]
fn the_fixture_binaries_match_their_generators() {
    let f = fixtures();
    for dir in ["lua-bridge", "lua-bridge-0.5.4"] {
        let bytes = std::fs::read(f.join(format!("shipments/{dir}/src/lua_bridge.asi"))).unwrap();
        assert_eq!(bytes, minimal_i386_dll(), "{dir}");
    }
    for dir in ["ess-0.7.0", "ess-0.6.1"] {
        let bytes = std::fs::read(f.join(format!("shipments/{dir}/src/ess_ui.gfx"))).unwrap();
        assert_eq!(bytes, minimal_gfx(), "{dir}");
    }
}

/// Plan a fixture request against a fixture game layout.
fn fixture_plan(request: &str, game: &str) -> LoadPlan {
    let f = fixtures();
    let items = plan::read_request(&f.join(request)).expect("request");
    let opened: Vec<LoadedShipment> = items
        .iter()
        .map(|i| discover::open(&i.path).expect("fixture Shipment opens"))
        .collect();
    let inputs: Vec<PlanInput<'_>> = items
        .iter()
        .zip(&opened)
        .map(|(i, s)| PlanInput {
            id: &i.id,
            shipment: s,
        })
        .collect();
    let vz = compat::resolve_vz_wad(Some(&f.join(game))).expect("fixture vz.wad");
    let root = compat::game_root_of(&vz).expect("fixture game root");
    compat::plan(&inputs, Producer::Preflight, Some(&root)).expect("plan")
}

/// Compare against a golden plan as parsed JSON, with `quartermaster` taken as the running version.
fn assert_golden(plan_: &LoadPlan, golden: &str) {
    let mut want: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join(golden)).unwrap()).unwrap();
    want["quartermaster"] = serde_json::Value::String(env!("CARGO_PKG_VERSION").into());
    let got = serde_json::to_value(plan_).unwrap();
    assert_eq!(
        got,
        want,
        "{golden} differs:\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

/// my-mod → ess → lua-bridge, all satisfied: ok, and lua-bridge before ess before my-mod even
/// though the request lists ess first.
#[test]
fn chain_passes() {
    let p = fixture_plan("request.chain.json", "game-clean");
    assert!(p.ok, "{:?}", p.findings);
    assert_eq!(
        p.order.as_deref(),
        Some(
            &[
                "shipment:lua-bridge".to_string(),
                "shipment:ess".into(),
                "shipment:my-mod".into()
            ][..]
        )
    );
    assert_golden(&p, "plan.chain.pass.json");
}

/// ess 0.6.1 does not satisfy my-mod's `>=0.7, <1` (M0204); the order is still there.
#[test]
fn chain_fails_with_old_ess() {
    let p = fixture_plan("request.chain-old-ess.json", "game-clean");
    assert!(!p.ok);
    assert_eq!(codes(&p), vec!["M0204"]);
    assert_golden(&p, "plan.chain.fail-old-ess.json");
}

/// lua-bridge 0.5.4 does not satisfy Ess's `^1.0.0` (M0204 on Ess).
#[test]
fn chain_fails_with_old_bridge() {
    let p = fixture_plan("request.chain-old-bridge.json", "game-clean");
    assert_eq!(codes(&p), vec!["M0204"]);
    assert_eq!(p.findings[0].items[0], "shipment:ess");
    assert_golden(&p, "plan.chain.fail-old-bridge.json");
}

/// The legacy OnLoad install is still in the game folder (M0208).
#[test]
fn chain_fails_with_the_legacy_file_present() {
    let p = fixture_plan("request.chain.json", "game-legacy");
    assert_eq!(codes(&p), vec!["M0208"]);
    assert_golden(&p, "plan.chain.fail-legacy-file.json");
}

/// No request path and no absolute path anywhere in the serialised plan, message text included.
#[test]
fn plan_json_has_no_local_paths() {
    let f = fixtures();
    for (request, game) in [
        ("request.chain.json", "game-clean"),
        ("request.chain-old-ess.json", "game-clean"),
        ("request.chain.json", "game-legacy"),
    ] {
        let text = serde_json::to_string(&fixture_plan(request, game)).unwrap();
        for item in plan::read_request(&f.join(request)).unwrap() {
            let path = item.path.to_string_lossy().into_owned();
            assert!(!text.contains(&path), "{request}: the plan carries {path}");
            let tail = item
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            assert!(
                !text.contains(&format!("shipments/{tail}")),
                "{request}: carries a request path"
            );
        }
        let root = f.to_string_lossy().into_owned();
        assert!(
            !text.contains(&root),
            "{request}: the plan carries an absolute path"
        );
    }
}

// ---------------------------------------------------------------------------
// M0203 / M0204 — requirements and versions
// ---------------------------------------------------------------------------

#[test]
fn duplicate_name_fails() {
    let d = scratch("dup");
    let a = ship(&d, "ess", "0.7.0", "");
    let b = ship(&d, "ess", "0.6.1", "");
    let consumer = ship(&d, "my-mod", "1.0.0", "load: { requires: [ess] }\n");
    let p = plan_of(&[&a, &b, &consumer], None);
    assert_eq!(codes(&p), vec!["M0203"]);
    assert_eq!(p.findings[0].items, vec!["shipment:ess", "shipment:ess#2"]);
    assert_eq!(p.requirements[0].status, RequirementStatus::Ambiguous);
    assert_eq!(p.requirements[0].resolved_version, None);
    // One edge from each item with the name.
    assert_eq!(p.edges.len(), 2);
}

#[test]
fn shipment_missing_fails() {
    let d = scratch("missing");
    let consumer = ship(&d, "my-mod", "1.0.0", "load: { requires: [ess] }\n");
    let p = plan_of(&[&consumer], None);
    assert_eq!(codes(&p), vec!["M0204"]);
    assert_eq!(p.requirements[0].status, RequirementStatus::Missing);
    assert!(p.edges.is_empty(), "a missing requirement orders nothing");
    assert!(p.order.is_some());
}

#[test]
fn older_than_range_fails() {
    let d = scratch("older");
    let ess = ship(&d, "ess", "0.6.1", "");
    let consumer = ship(
        &d,
        "my-mod",
        "1.0.0",
        "load: { requires: [{ shipment: ess, version: \">=0.7, <1\" }] }\n",
    );
    let p = plan_of(&[&ess, &consumer], None);
    assert_eq!(codes(&p), vec!["M0204"]);
    let msg = &p.findings[0].message;
    assert!(
        msg.contains(">=0.7, <1") && msg.contains("0.6.1") && msg.contains("my-mod"),
        "{msg}"
    );
    assert_eq!(
        p.requirements[0].status,
        RequirementStatus::VersionUnsatisfied
    );
}

#[test]
fn satisfying_version_passes() {
    let d = scratch("satisfies");
    let ess = ship(&d, "ess", "0.7.3", "");
    let consumer = ship(
        &d,
        "my-mod",
        "1.0.0",
        "load: { requires: [{ shipment: ess, version: \">=0.7, <1\" }] }\n",
    );
    let p = plan_of(&[&ess, &consumer], None);
    assert!(p.ok, "{:?}", p.findings);
    assert_eq!(p.requirements[0].resolved_version.as_deref(), Some("0.7.3"));
}

#[test]
fn bare_name_any_version() {
    let d = scratch("bare");
    let ess = ship(&d, "ess", "9.9.9", "");
    let consumer = ship(&d, "my-mod", "1.0.0", "load: { requires: [ess] }\n");
    let p = plan_of(&[&ess, &consumer], None);
    assert!(p.ok, "{:?}", p.findings);
    assert_eq!(p.requirements[0].range, None);
}

/// Caret on a 0.x version pins the minor: `^0.6` excludes `0.7.0`.
#[test]
fn caret_zero_major_excludes_next_minor() {
    let d = scratch("caret");
    let ess = ship(&d, "ess", "0.7.0", "");
    let consumer = ship(
        &d,
        "my-mod",
        "1.0.0",
        "load: { requires: [{ shipment: ess, version: \"^0.6\" }] }\n",
    );
    let p = plan_of(&[&ess, &consumer], None);
    assert_eq!(codes(&p), vec!["M0204"]);
}

#[test]
fn capability_unprovided_fails() {
    let d = scratch("cap-missing");
    let consumer = ship(
        &d,
        "ui-mod",
        "1.0.0",
        "load: { requires: [{ capability: widescreen }] }\n",
    );
    let p = plan_of(&[&consumer], None);
    assert_eq!(codes(&p), vec!["M0204"]);
    assert_eq!(p.capabilities.len(), 1);
    assert!(p.capabilities[0].providers.is_empty());
}

/// Several providers is fine, and each one gets an edge to the consumer.
#[test]
fn two_capability_providers_pass() {
    let d = scratch("cap-two");
    let consumer = ship(
        &d,
        "ui-mod",
        "1.0.0",
        "load: { requires: [{ capability: widescreen }] }\n",
    );
    let a = ship(&d, "wide-a", "1.0.0", "load: { provides: [widescreen] }\n");
    let b = ship(&d, "wide-b", "1.0.0", "load: { provides: [widescreen] }\n");
    let p = plan_of(&[&consumer, &a, &b], None);
    assert!(p.ok, "{:?}", p.findings);
    assert_eq!(p.edges.len(), 2);
    assert_eq!(
        p.capabilities[0].providers,
        vec!["shipment:wide-a", "shipment:wide-b"]
    );
    assert_eq!(p.capabilities[0].consumers, vec!["shipment:ui-mod"]);
    assert_eq!(
        p.order.as_deref(),
        Some(
            &[
                "shipment:wide-a".to_string(),
                "shipment:wide-b".into(),
                "shipment:ui-mod".into()
            ][..]
        )
    );
}

// ---------------------------------------------------------------------------
// M0206 — declared conflicts
// ---------------------------------------------------------------------------

#[test]
fn declared_conflict_fails() {
    let d = scratch("declared");
    let hostile = ship(&d, "hostile", "1.0.0", "load: { conflicts: [victim] }\n");
    let victim = ship(&d, "victim", "2.0.0", "");
    let p = plan_of(&[&hostile, &victim], None);
    assert_eq!(codes(&p), vec!["M0206"]);
    match &p.conflicts[0] {
        ConflictRow::Declared(r) => assert_eq!(r.status, DeclaredStatus::Conflict),
        other => panic!("{other:?}"),
    }
}

#[test]
fn declared_conflict_outside_range_passes() {
    let d = scratch("declared-range");
    let hostile = ship(
        &d,
        "hostile",
        "1.0.0",
        "load: { conflicts: [{ shipment: victim, version: \"<2\" }] }\n",
    );
    let victim = ship(&d, "victim", "2.0.0", "");
    let p = plan_of(&[&hostile, &victim], None);
    assert!(p.ok, "{:?}", p.findings);
    match &p.conflicts[0] {
        ConflictRow::Declared(r) => assert_eq!(r.status, DeclaredStatus::OutsideRange),
        other => panic!("{other:?}"),
    }
}

/// A declared conflict naming a Shipment that is not installed is inert.
#[test]
fn declared_conflict_not_installed_passes() {
    let d = scratch("declared-absent");
    let hostile = ship(&d, "hostile", "1.0.0", "load: { conflicts: [victim] }\n");
    let p = plan_of(&[&hostile], None);
    assert!(p.ok);
    match &p.conflicts[0] {
        ConflictRow::Declared(r) => assert_eq!(r.status, DeclaredStatus::NotInstalled),
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------
// M0207 — claim-graph conflicts
// ---------------------------------------------------------------------------

/// Two Shipments minting one `add_script` name is a duplicate key; the row names each claimant by
/// id with its contribution and kind.
#[test]
fn two_shipments_minting_one_script_name_conflict() {
    let d = scratch("claims");
    let body = |name: &str| {
        format!(
            "shipment: {{ name: {name}, version: 1.0.0, target: retail }}\n\
             contributions:\n  - {{ kind: add_script, name: ess, source: src/ess.lua }}\n"
        )
    };
    let a = shipment_at(&d.join("a"), &body("fork-a"));
    let b = shipment_at(&d.join("b"), &body("fork-b"));
    let p = plan_of(&[&a, &b], None);
    assert_eq!(codes(&p), vec!["M0207"]);
    match &p.conflicts[0] {
        ConflictRow::Claims(r) => {
            let who: Vec<(&str, usize, &str)> = r
                .claimants
                .iter()
                .map(|c| (c.item.as_str(), c.contribution, c.kind.as_str()))
                .collect();
            assert_eq!(
                who,
                vec![
                    ("shipment:fork-a", 0, "add_script"),
                    ("shipment:fork-b", 0, "add_script")
                ]
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(p.findings[0].refs[0].section, Section::Conflicts);
}

// ---------------------------------------------------------------------------
// M0208 — superseded files
// ---------------------------------------------------------------------------

fn superseder(root: &Path, file: &str) -> LoadedShipment {
    shipment_at(
        &root.join("ess"),
        &format!(
            "shipment: {{ name: ess, version: 0.7.0, target: retail }}\n\
             supersedes:\n  - {{ dest: on_load, file: {file} }}\ncontributions: []\n"
        ),
    )
}

#[test]
fn superseded_present_fails() {
    let d = scratch("sup-present");
    let game = d.join("game");
    std::fs::create_dir_all(game.join("scripts/OnLoad")).unwrap();
    std::fs::write(game.join("scripts/OnLoad/1_Ess.lua"), "--").unwrap();
    let p = plan_of(&[&superseder(&d, "1_Ess.lua")], Some(&game));
    assert_eq!(codes(&p), vec!["M0208"]);
    assert!(p.supersedes[0].present);
}

#[test]
fn superseded_absent_passes() {
    let d = scratch("sup-absent");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let p = plan_of(&[&superseder(&d, "1_Ess.lua")], Some(&game));
    assert!(p.ok);
    assert!(!p.supersedes[0].present);
    assert_eq!(p.supersedes[0].relative, "scripts/OnLoad/1_Ess.lua");
}

/// Windows file names are case-insensitive, so the probe is too, whatever the host.
#[test]
fn superseded_file_matches_case_insensitively() {
    let d = scratch("sup-case");
    let game = d.join("game");
    std::fs::create_dir_all(game.join("scripts/OnLoad")).unwrap();
    std::fs::write(game.join("scripts/OnLoad/1_ESS.LUA"), "--").unwrap();
    let p = plan_of(&[&superseder(&d, "1_Ess.lua")], Some(&game));
    assert_eq!(codes(&p), vec!["M0208"]);
}

/// A symlink counts as present, and is not followed.
#[cfg(unix)]
#[test]
fn a_dangling_symlink_counts_as_present() {
    let d = scratch("sup-link");
    let game = d.join("game");
    std::fs::create_dir_all(game.join("scripts/OnLoad")).unwrap();
    std::os::unix::fs::symlink(d.join("nowhere"), game.join("scripts/OnLoad/1_Ess.lua")).unwrap();
    let p = plan_of(&[&superseder(&d, "1_Ess.lua")], Some(&game));
    assert_eq!(codes(&p), vec!["M0208"]);
}

/// With no game folder, a Shipment that declares `supersedes` cannot be checked — never skipped.
#[test]
fn supersedes_without_a_game_root_is_an_error() {
    let d = scratch("sup-noroot");
    let s = superseder(&d, "1_Ess.lua");
    let inputs = [PlanInput {
        id: "shipment:ess",
        shipment: &s,
    }];
    match compat::plan(&inputs, Producer::Preflight, None) {
        Err(CompatError::GameRootRequired { id }) => assert_eq!(id, "shipment:ess"),
        other => panic!("expected GameRootRequired, got {other:?}"),
    }
}

/// The game folder is the parent of the `data` directory holding `vz.wad`; anything else is refused.
#[test]
fn game_root_not_under_data_fails() {
    let f = fixtures();
    let bad = compat::resolve_vz_wad(Some(&f.join("game-bad/vz.wad"))).expect("the file exists");
    let err = compat::game_root_of(&bad).expect_err("not under data/");
    assert!(err.contains("pass --game"), "{err}");

    let good = compat::resolve_vz_wad(Some(&f.join("game-clean"))).expect("install root form");
    assert_eq!(compat::game_root_of(&good).unwrap(), f.join("game-clean"));
    let via_data = compat::resolve_vz_wad(Some(&f.join("game-clean/data"))).expect("data form");
    assert_eq!(
        compat::game_root_of(&via_data).unwrap(),
        f.join("game-clean")
    );
}

// ---------------------------------------------------------------------------
// M0210 — the manifest's Quartermaster range
// ---------------------------------------------------------------------------

#[test]
fn quartermaster_range_unsatisfied_fails() {
    let d = scratch("qm-range");
    let s = shipment_at(
        &d.join("s"),
        "shipment: { name: s, version: 1.0.0, target: retail, quartermaster: \">=99\" }\ncontributions: []\n",
    );
    let p = plan_of(&[&s], None);
    assert_eq!(codes(&p), vec!["M0210"]);
    assert_eq!(p.items[0].quartermaster_range.as_deref(), Some(">=99"));
}

#[test]
fn quartermaster_range_satisfied_passes() {
    let d = scratch("qm-range-ok");
    let s = shipment_at(
        &d.join("s"),
        &format!(
            "shipment: {{ name: s, version: 1.0.0, target: retail, quartermaster: \"={}\" }}\ncontributions: []\n",
            env!("CARGO_PKG_VERSION")
        ),
    );
    assert!(plan_of(&[&s], None).ok);
}

// ---------------------------------------------------------------------------
// Plugins — M0162, M0178
// ---------------------------------------------------------------------------

fn with_plugin(root: &Path, file: &str, bytes: &[u8]) -> LoadedShipment {
    let dir = root.join("hook");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join(file), bytes).unwrap();
    shipment_at(
        &dir,
        &format!(
            "shipment: {{ name: hook, version: 1.0.0, target: retail }}\n\
             contributions:\n  - {{ kind: native_hook, target: retail, plugin: src/{file} }}\n"
        ),
    )
}

#[test]
fn a_plugin_that_is_not_i386_is_m0178() {
    let d = scratch("amd64");
    let s = with_plugin(&d, "hook.asi", &pe_image(0x8664, 0x2022));
    let p = plan_of(&[&s], None);
    assert_eq!(codes(&p), vec!["M0178"]);
    assert_eq!(
        p.items[0].plugins.len(),
        1,
        "a refused plugin still appears"
    );
}

#[test]
fn a_plugin_that_is_not_an_asi_is_m0162() {
    let d = scratch("not-asi");
    let s = with_plugin(&d, "hook.dll", &minimal_i386_dll());
    let p = plan_of(&[&s], None);
    assert_eq!(codes(&p), vec!["M0162"]);
    assert_eq!(p.items[0].plugins[0].relative, "scripts/hook.dll");
}

/// An unreadable plugin cannot be described, so there is no plan (exit 2), never an entry without
/// a digest.
#[test]
fn a_missing_plugin_is_an_error() {
    let d = scratch("no-plugin");
    let s = shipment_at(
        &d.join("hook"),
        "shipment: { name: hook, version: 1.0.0, target: retail }\n\
         contributions:\n  - { kind: native_hook, target: retail, plugin: src/absent.asi }\n",
    );
    let inputs = [PlanInput {
        id: "shipment:hook",
        shipment: &s,
    }];
    match compat::plan(&inputs, Producer::Preflight, None) {
        Err(CompatError::Item { id, message }) => {
            assert_eq!(id, "shipment:hook");
            assert!(
                !message.contains(&*d.to_string_lossy()),
                "no absolute path: {message}"
            );
        }
        other => panic!("expected an item error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Order through the plan
// ---------------------------------------------------------------------------

/// Errors that are not a cycle still leave the order in the plan.
#[test]
fn order_present_despite_non_cycle_errors() {
    let d = scratch("order-errors");
    let ess = ship(&d, "ess", "0.6.1", "");
    let consumer = ship(
        &d,
        "my-mod",
        "1.0.0",
        "load: { requires: [{ shipment: ess, version: \">=0.7\" }, lua-bridge] }\n",
    );
    let p = plan_of(&[&consumer, &ess], None);
    assert!(!p.ok);
    assert_eq!(codes(&p), vec!["M0204", "M0204"]);
    assert!(p.order.is_some(), "only a cycle removes the order");
    assert!(p.items.iter().all(|i| i.resolved.is_some()));
}

/// A two- and a three-node cycle: no order, and one M0174 per cycle listing every edge on it.
#[test]
fn a_requires_cycle_has_no_order() {
    let d = scratch("cycle");
    let a = ship(&d, "a", "1.0.0", "load: { requires: [b] }\n");
    let b = ship(&d, "b", "1.0.0", "load: { requires: [a] }\n");
    let x = ship(&d, "x", "1.0.0", "load: { requires: [z] }\n");
    let y = ship(&d, "y", "1.0.0", "load: { requires: [x] }\n");
    let z = ship(&d, "z", "1.0.0", "load: { requires: [y] }\n");
    let p = plan_of(&[&a, &b, &x, &y, &z], None);
    assert_eq!(p.order, None);
    assert!(p
        .items
        .iter()
        .all(|i| i.resolved.is_none() && i.held_back_by.is_none()));
    assert_eq!(codes(&p), vec!["M0174", "M0174"]);
    let refs: Vec<Vec<usize>> = p
        .findings
        .iter()
        .map(|f| f.refs.iter().map(|r| r.index).collect())
        .collect();
    assert_eq!(refs, vec![vec![0, 1], vec![2, 3, 4]]);
    assert!(p
        .findings
        .iter()
        .all(|f| f.refs.iter().all(|r| r.section == Section::Edges)));
}

/// With no edges the order is the request order, and reversing the request reverses it.
#[test]
fn with_no_edges_the_order_is_the_request_order() {
    let d = scratch("no-edges");
    let a = ship(&d, "a", "1.0.0", "");
    let b = ship(&d, "b", "1.0.0", "");
    let c = ship(&d, "c", "1.0.0", "");
    let fwd = plan_of(&[&b, &c, &a], None);
    let rev = plan_of(&[&a, &c, &b], None);
    assert_eq!(
        fwd.order.unwrap(),
        vec!["shipment:b", "shipment:c", "shipment:a"]
    );
    assert_eq!(
        rev.order.unwrap(),
        vec!["shipment:a", "shipment:c", "shipment:b"]
    );
}

/// A consumer listed before `ess` still loads after it, and the `qm_modloader` bake — which
/// follows the plan's order — registers `ess` before the consumer.
#[test]
fn a_consumer_listed_before_ess_is_baked_after_it() {
    let d = scratch("consumer-before-ess");
    let consumer = ship(&d, "a-consumer", "1.0.0", "load: { requires: [ess] }\n");
    let ess = ship(&d, "ess", "0.7.0", "");
    let p = plan_of(&[&consumer, &ess], None);
    assert!(p.ok);
    let names: Vec<String> = p
        .order
        .unwrap()
        .iter()
        .map(|id| id.trim_start_matches("shipment:").to_string())
        .collect();
    assert_eq!(names, vec!["ess", "a-consumer"]);
    assert_eq!(
        p.items[0].held_back_by,
        Some(0),
        "the consumer was held back by its ess edge"
    );

    let regs = [
        UiRegistration {
            shipment: "a-consumer".into(),
            movie: "consumer_hud".into(),
        },
        UiRegistration {
            shipment: "ess".into(),
            movie: "ess_ui".into(),
        },
    ];
    let bake = link::qm_modloader_source(&regs, &[], &[], &names).unwrap();
    assert!(
        bake.find("ess_ui").unwrap() < bake.find("consumer_hud").unwrap(),
        "{bake}"
    );
}

/// Finding shape: `fix` is always present and null in a plan; every finding is an error here.
#[test]
fn plan_findings_carry_a_null_fix() {
    let d = scratch("fix");
    let consumer = ship(&d, "my-mod", "1.0.0", "load: { requires: [ess] }\n");
    let p = plan_of(&[&consumer], None);
    let v = serde_json::to_value(&p).unwrap();
    assert!(v["findings"][0].get("fix").is_some_and(|f| f.is_null()));
    assert!(p
        .findings
        .iter()
        .all(|f| f.severity == FindingSeverity::Error));
}
