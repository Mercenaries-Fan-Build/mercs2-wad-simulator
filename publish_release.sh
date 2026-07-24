#!/usr/bin/env bash
#
# publish_release.sh — publish the wad_simulator workspace crates to crates.io in
# dependency order, respecting crates.io rate limits, and RESUMABLE.
#
#   crates.io limits:
#     * brand-new crate:  burst of 5, then 1 every 10 minutes
#     * new version of an existing crate: burst of 30, then 1 per minute
#
# The 23 NEW crates are the bottleneck (~3 h at the default limit). This script
# skips anything already live, publishes in a dependency-valid order, and sleeps
# 10 min before each new-crate publish once the burst-of-5 is spent — so a single
# run just works, and a crash/429 can be resumed by re-running it.
#
# Prereqs: `cargo login` (or CARGO_REGISTRY_TOKEN) done first.
# Usage:   ./publish_release.sh            # real publish
#          DRY=1 ./publish_release.sh      # print the plan, publish nothing
set -uo pipefail
cd "$(dirname "$0")"

# Dependency-valid publish order. Each crate's internal deps appear earlier.
ORDER=(
  mercs2_formats      # existing 2.0.0 — foundational (root of nearly everything)
  mercs2_mesh         # carved out of formats — needs formats
  mercs2_luac         # carved out of formats — needs formats
  mercs2_core         # NEW — foundational
  loadprobe           # existing 2.0.0 — standalone
  ucfx_byteswap       # existing 2.0.0 — needs formats
  mercs2_audio mercs2_ai mercs2_anim mercs2_combat mercs2_decal mercs2_faction
  mercs2_jobs mercs2_net mercs2_physics mercs2_player mercs2_population
  mercs2_vehicle mercs2_water mercs2_ui                # NEW — need core+formats
  mercs2_smuggler wad_builder                          # NEW — need formats
  mercs2_reassemble                                    # NEW — standalone
  mercs2_script                                        # NEW — needs ui,core
  wad_simulator                                        # existing 2.0.0 — needs formats,audio,ucfx
  mercs2_engine                                        # NEW — needs all subsystems + script
  mercs2_probe mercs2_game mercs2_workshop             # NEW — need engine
)

crate_target_version() { # read version = "x" from crates/<c>/Cargo.toml
  sed -n 's/^version = "\(.*\)"/\1/p' "crates/$1/Cargo.toml" | head -1
}
version_is_live() { # $1=crate $2=version  -> 0 if that exact version exists on crates.io
  curl -s -A "publish_release" "https://crates.io/api/v1/crates/$1/$2" | grep -q '"num"'
}
crate_exists() { # $1=crate -> 0 if the crate has ANY published version (i.e. NOT brand-new)
  curl -s -A "publish_release" "https://crates.io/api/v1/crates/$1" | grep -q '"max_version"'
}
crate_last_publish() { # $1=crate -> ISO8601 timestamp of the crate's most recent publish ("" if none)
  curl -s -A "publish_release" "https://crates.io/api/v1/crates/$1" \
    | tr ',' '\n' | grep -m1 '"updated_at"' | cut -d'"' -f4
}
commits_since() { # $1=crate $2=ISO8601 -> number of commits touching crates/<c>/ since that instant
  git log --oneline --since="$2" -- "crates/$1/" 2>/dev/null | wc -l | tr -d '[:space:]'
}

new_burst_used=0   # count of NEW-crate publishes done in THIS run (burst budget = 5)
needs_bump=()      # live-but-changed crates: a release is due but the version was never bumped

for c in "${ORDER[@]}"; do
  ver="$(crate_target_version "$c")"
  # Already live at the target version. Two very different reasons, and conflating them is how a
  # crate with real changes gets left un-released: either nothing changed since the last publish
  # (a genuine no-op skip), or someone forgot to bump. Distinguish by commit count, and make the
  # second case loud — cargo cannot re-publish an existing version, so silence would look like success.
  if version_is_live "$c" "$ver"; then
    since="$(crate_last_publish "$c")"
    n=0; [ -n "$since" ] && n="$(commits_since "$c" "$since")"
    if [ "${n:-0}" -gt 0 ]; then
      echo "!! NEEDS BUMP  $c $ver is live but has $n commit(s) since that release — NOT published."
      needs_bump+=("$c ($n commits)")
    else
      echo "== skip  $c $ver (up to date — live, no commits since)"
    fi
    continue
  fi
  is_new=0; crate_exists "$c" || is_new=1

  if [ "$is_new" = 1 ] && [ "$new_burst_used" -ge 5 ]; then
    echo "-- rate limit: new-crate burst spent; sleeping 10 min before $c ..."
    [ "${DRY:-0}" = 1 ] || sleep 600
  fi

  echo "== publish $c $ver  (new=$is_new)"
  if [ "${DRY:-0}" = 1 ]; then
    [ "$is_new" = 1 ] && new_burst_used=$((new_burst_used+1))
    continue
  fi

  # retry loop: on a rate-limit rejection, honour a 10-min backoff and retry the same crate.
  # Any other failure aborts (fix it, then re-run — live crates are skipped on resume).
  while true; do
    out="$(cargo publish -p "$c" 2>&1)"; rc=$?
    echo "$out"
    if [ $rc -eq 0 ] || grep -qiE 'already (uploaded|exists)' <<<"$out"; then
      break
    fi
    if grep -qiE 'rate limit|429|too many requests' <<<"$out"; then
      echo "-- rate limited on $c; waiting 10 min and retrying ..."
      sleep 600; continue
    fi
    echo "!! $c failed for a non-rate-limit reason (see above). Fix and re-run."; exit 1
  done
  [ "$is_new" = 1 ] && new_burst_used=$((new_burst_used+1))
done

if [ "${#needs_bump[@]}" -gt 0 ]; then
  echo
  echo "!! ${#needs_bump[@]} crate(s) have commits since their last release but were NOT bumped,"
  echo "!! so nothing was published for them:"
  printf '!!   %s\n' "${needs_bump[@]}"
  echo "!! Bump the version in crates/<name>/Cargo.toml (and [workspace.dependencies]) and re-run."
fi

echo "All done. Live versions:"
for c in "${ORDER[@]}"; do
  printf '  %-18s %s\n' "$c" "$(curl -s -A pr https://crates.io/api/v1/crates/$c | tr ',' '\n' | grep -m1 max_version | cut -d'"' -f4)"
done

# Non-zero if any crate was skipped despite having changes, so a forgotten bump cannot pass as
# success in CI or in a scrollback. Everything else already published fine; re-running is safe.
[ "${#needs_bump[@]}" -eq 0 ]
