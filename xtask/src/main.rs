//! `cargo xtask publish [--dry-run]` — publish the workspace crates to crates.io in dependency
//! order, resumable and rate-limit aware.
//!
//! This replaces `publish_release.sh` and `publish_release.ps1`. Those carried a HAND-MAINTAINED
//! `ORDER` list (and the bash copy fell behind the PowerShell one — it silently omitted
//! `mercs2_bridge`, `mercs2_destruction` and `mercs2_quartermaster`, which only surfaced as an
//! unresolved-dependency failure mid-publish). Here the publishable set and the order are DERIVED
//! from `cargo metadata`, so they cannot drift from the workspace:
//!
//! * publishable = every workspace member whose Cargo.toml does not set `publish = false`;
//! * order = a topological sort of the intra-workspace (normal + build) dependency graph, so each
//!   crate is published after the crates it depends on.
//!
//! crates.io rate limits (mirrored from the old scripts): a brand-new crate allows a burst of 5,
//! then one per 10 minutes; a new version of an existing crate is far more generous. New crates are
//! the bottleneck, so once the burst is spent the tool sleeps 10 minutes before each new-crate
//! publish. A crash or 429 is recoverable by re-running — anything already live is skipped.
//!
//! Prereqs: `cargo login` (or `CARGO_REGISTRY_TOKEN`). `--dry-run` prints the plan and publishes
//! nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, ExitCode};
use std::thread::sleep;
use std::time::Duration;

const UA: &str = "publish_release (mercs2 workspace xtask)";
const NEW_BURST: usize = 5;
const BACKOFF: Duration = Duration::from_secs(600);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dry = args.iter().any(|a| a == "--dry-run");
    match args.iter().find(|a| !a.starts_with("--")).map(String::as_str) {
        Some("publish") => publish(dry),
        _ => {
            eprintln!("usage: cargo xtask publish [--dry-run]");
            ExitCode::from(2)
        }
    }
}

fn publish(dry: bool) -> ExitCode {
    let meta = match cargo_metadata::MetadataCommand::new().exec() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cargo metadata failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let root = meta.workspace_root.clone();

    // Publishable workspace members: `publish = false` serializes as `Some([])`; anything else
    // (unset, or a registry allowlist) is publishable. The xtask itself is `publish = false`, so it
    // excludes itself here.
    let members: Vec<&cargo_metadata::Package> = meta
        .workspace_packages()
        .into_iter()
        .filter(|p| p.publish.as_ref().map_or(true, |allow| !allow.is_empty()))
        .collect();
    let names: BTreeSet<&str> = members.iter().map(|p| p.name.as_str()).collect();

    // Edges: dep -> pkg for every intra-workspace dependency, so a crate lands after the crates it
    // needs. Dev-dependencies are INCLUDED: `cargo publish` runs a verification build that resolves
    // the full dependency graph — dev-deps too — so a dev-dependency on a workspace crate that is
    // not yet on crates.io fails the publish (this is exactly how `mercs2_engine`, which
    // dev-depends on `mercs2_quartermaster`, failed against an unpublished quartermaster 3.0.0). A
    // genuine dev-dependency cycle would surface as a topo-sort error rather than silently
    // mis-ordering.
    let mut deps_of: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for p in &members {
        let entry = deps_of.entry(p.name.as_str()).or_default();
        for d in &p.dependencies {
            if names.contains(d.name.as_str()) && d.name != p.name {
                entry.insert(names.get(d.name.as_str()).copied().unwrap());
            }
        }
    }

    let order = match topo_sort(&names, &deps_of) {
        Ok(o) => o,
        Err(cycle) => {
            eprintln!("!! dependency cycle among publishable crates: {cycle:?}");
            return ExitCode::FAILURE;
        }
    };

    // name -> (version, crate dir relative to the workspace root)
    let mut info: BTreeMap<&str, (String, String)> = BTreeMap::new();
    for p in &members {
        let dir = p
            .manifest_path
            .parent()
            .and_then(|d| d.strip_prefix(&root).ok())
            .map(|d| d.to_string())
            .unwrap_or_else(|| p.manifest_path.parent().unwrap().to_string());
        info.insert(p.name.as_str(), (p.version.to_string(), dir));
    }

    let mut new_burst = 0usize;
    let mut needs_bump: Vec<String> = Vec::new();

    for c in &order {
        let (ver, dir) = &info[c.as_str()];
        // Already live at the target version: either a genuine no-op (nothing changed) or a
        // forgotten bump. cargo cannot re-publish an existing version, so the second case is made
        // loud rather than passing as a silent success.
        if version_is_live(c, ver) {
            let n = last_publish(c)
                .map(|since| commits_since(dir, &since))
                .unwrap_or(0);
            if n > 0 {
                println!("!! NEEDS BUMP  {c} {ver} is live but has {n} commit(s) since that release — NOT published.");
                needs_bump.push(format!("{c} ({n} commits)"));
            } else {
                println!("== skip  {c} {ver} (up to date — live, no commits since)");
            }
            continue;
        }

        let is_new = !crate_exists(c);
        if is_new && new_burst >= NEW_BURST {
            println!("-- rate limit: new-crate burst spent; sleeping 10 min before {c} ...");
            if !dry {
                sleep(BACKOFF);
            }
        }
        println!("== publish {c} {ver}  (new={is_new})");
        if dry {
            if is_new {
                new_burst += 1;
            }
            continue;
        }

        // Retry on a rate-limit rejection; treat "already uploaded" as done; abort on anything
        // else (fix it, then re-run — live crates are skipped on resume).
        loop {
            let out = Command::new("cargo").args(["publish", "-p", c]).output();
            let (ok, text) = match out {
                Ok(o) => (
                    o.status.success(),
                    format!(
                        "{}{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    ),
                ),
                Err(e) => (false, e.to_string()),
            };
            print!("{text}");
            let low = text.to_lowercase();
            if ok || low.contains("already uploaded") || low.contains("already exists") {
                break;
            }
            if low.contains("rate limit") || low.contains("429") || low.contains("too many requests")
            {
                println!("-- rate limited on {c}; waiting 10 min and retrying ...");
                sleep(BACKOFF);
                continue;
            }
            println!("!! {c} failed for a non-rate-limit reason (see above). Fix and re-run.");
            return ExitCode::FAILURE;
        }
        if is_new {
            new_burst += 1;
        }
    }

    if !needs_bump.is_empty() {
        println!();
        println!(
            "!! {} crate(s) have commits since their last release but were NOT bumped,",
            needs_bump.len()
        );
        println!("!! so nothing was published for them:");
        for x in &needs_bump {
            println!("!!   {x}");
        }
        println!("!! Bump the version in crates/<name>/Cargo.toml (and [workspace.dependencies]) and re-run.");
    }

    println!("All done. Live versions:");
    for c in &order {
        let mv = max_version(c).unwrap_or_else(|| "<none>".into());
        println!("  {c:<18} {mv}");
    }

    // Non-zero if any crate was skipped despite having changes, so a forgotten bump cannot pass as
    // success in CI or a scrollback.
    if needs_bump.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Kahn's algorithm over the publishable crates. Ties break alphabetically (BTree ordering) so the
/// plan is deterministic run to run. Returns the remaining nodes if a cycle blocks completion.
fn topo_sort(
    names: &BTreeSet<&str>,
    deps_of: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<Vec<String>, Vec<String>> {
    let empty = BTreeSet::new();
    let mut indeg: BTreeMap<&str, usize> = names.iter().map(|n| (*n, 0)).collect();
    for n in names {
        for _d in deps_of.get(n).unwrap_or(&empty) {
            *indeg.get_mut(n).unwrap() += 1;
        }
    }
    let mut ready: BTreeSet<&str> = indeg
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(n, _)| *n)
        .collect();
    let mut out: Vec<String> = Vec::new();
    while let Some(&n) = ready.iter().next() {
        ready.remove(n);
        out.push(n.to_string());
        // Anything that depended on `n` loses an in-edge.
        for m in names {
            if deps_of.get(m).unwrap_or(&empty).contains(n) {
                let d = indeg.get_mut(m).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(m);
                }
            }
        }
    }
    if out.len() == names.len() {
        Ok(out)
    } else {
        Err(names
            .iter()
            .filter(|n| !out.iter().any(|o| o == *n))
            .map(|s| s.to_string())
            .collect())
    }
}

fn cio_get(url: &str) -> Result<String, ()> {
    match ureq::get(url).set("User-Agent", UA).call() {
        Ok(resp) => resp.into_string().map_err(|_| ()),
        Err(_) => Err(()),
    }
}

fn version_is_live(c: &str, v: &str) -> bool {
    cio_get(&format!("https://crates.io/api/v1/crates/{c}/{v}")).is_ok()
}

fn crate_exists(c: &str) -> bool {
    cio_get(&format!("https://crates.io/api/v1/crates/{c}")).is_ok()
}

fn last_publish(c: &str) -> Option<String> {
    let body = cio_get(&format!("https://crates.io/api/v1/crates/{c}")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    v["crate"]["updated_at"].as_str().map(str::to_string)
}

fn max_version(c: &str) -> Option<String> {
    let body = cio_get(&format!("https://crates.io/api/v1/crates/{c}")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    v["crate"]["max_version"].as_str().map(str::to_string)
}

fn commits_since(dir: &str, since: &str) -> usize {
    let out = Command::new("git")
        .args(["log", "--oneline", &format!("--since={since}"), "--", dir])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count(),
        _ => 0,
    }
}
