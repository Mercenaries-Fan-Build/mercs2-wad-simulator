#!/usr/bin/env bash
# Refresh workshop_data/ess_api.json + ess_natives.json from Wally's mercs2-lua-essentials RELEASE.
#
# He PUBLISHES the generated manifests in the release zip (Ess-<ver>.zip → api/ess.json,
# api/natives.json) — api/ess.json is his authoritative 720-function manifest with tier / params /
# returns / description, not something we should re-derive from source. We bundle those verbatim
# (per the standing rule: copy source data in, never link outside the repo), and tests/essapi
# verifies our parser still reads them, so an upstream schema drift fails a test here.
#
# Usage:  scripts/regen-ess-api.sh [tag]      (default: the latest release)
set -euo pipefail
REPO="loganw234/mercs2-lua-essentials"
OUT="$(cd "$(dirname "$0")/.." && pwd)/workshop_data"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ "${1:-}" = "" ]; then
  echo "downloading the latest $REPO release ..."
  gh release download --repo "$REPO" --pattern 'Ess-*.zip' --dir "$TMP"
else
  echo "downloading $REPO release $1 ..."
  gh release download "$1" --repo "$REPO" --pattern 'Ess-*.zip' --dir "$TMP"
fi

ZIP="$(echo "$TMP"/Ess-*.zip)"
unzip -o "$ZIP" 'api/ess.json' 'api/natives.json' -d "$TMP" >/dev/null
cp "$TMP/api/ess.json"     "$OUT/ess_api.json"
cp "$TMP/api/natives.json" "$OUT/ess_natives.json"
echo "bundled $(basename "$ZIP") -> ess_api.json ($(grep -c '"tier"' "$OUT/ess_api.json") tiered fns) + ess_natives.json"
