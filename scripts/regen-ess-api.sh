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
unzip -o "$ZIP" 'api/ess.json' 'api/natives.json' 'api/nodes.json' -d "$TMP" >/dev/null
cp "$TMP/api/ess.json"     "$OUT/ess_api.json"
cp "$TMP/api/natives.json" "$OUT/ess_natives.json"
cp "$TMP/api/nodes.json"    "$OUT/ess_nodes.json"
echo "bundled $(basename "$ZIP") -> ess_api.json ($(grep -c '"tier"' "$OUT/ess_api.json") tiered fns) + ess_natives.json"

# The native ECS component-class registry (the *typed* half of the console reference — what a live
# entity is made of). This is OUR cracked doc set, not Wally's, so it is generated from the vendored
# `docs/mercs2-ecs/_manifests/` partitions rather than downloaded: one row per class,
# family<TAB>name<TAB>registrar-global<TAB>descriptor-vtable. Consumed by `crate::ecsreg`.
ECS_SRC="$(cd "$(dirname "$0")/../../.." && pwd)/docs/mercs2-ecs/_manifests"
if [ -d "$ECS_SRC" ]; then
  : > "$OUT/ecs_registry.tsv"
  for f in "$ECS_SRC"/*.tsv; do
    fam="$(basename "$f" .tsv | sed -E 's/^[0-9]+_//')"
    awk -v fam="$fam" 'NF>=1 && $1!="" {print fam"\t"$0}' "$f" >> "$OUT/ecs_registry.tsv"
  done
  echo "bundled ecs_registry.tsv ($(wc -l < "$OUT/ecs_registry.tsv") classes across $(cut -f1 "$OUT/ecs_registry.tsv" | sort -u | wc -l) families)"
else
  echo "note: docs/mercs2-ecs/_manifests not found — kept existing ecs_registry.tsv" >&2
fi
