#!/usr/bin/env bash
# Regenerate workshop_data/ess_api.json from Wally's mercs2-lua-essentials.
#
# The Ess wrapper manifest (api/ess.json) is generated into a gitignored dist/ in his repo, so we
# derive our own snapshot from his COMMITTED src/*.lua — the authoritative `function Ess.NS.func(...)`
# definitions. His raw-engine catalogue (api/natives.json) IS committed and is copied verbatim.
#
# This bundles both into the Workshop's pack (per the standing rule: copy source data in, never link
# outside the repo), and `tests/ess_seam.rs` verifies our parser still reads them — so an API drift
# in his library fails a test here rather than silently rotting the console's reference.
#
# Usage:  scripts/regen-ess-api.sh [git-ref]     (default ref: master)
set -euo pipefail
REF="${1:-master}"
REPO="loganw234/mercs2-lua-essentials"
OUT="$(cd "$(dirname "$0")/.." && pwd)/workshop_data"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "fetching $REPO@$REF ..."
gh api "repos/$REPO/tarball/$REF" > "$TMP/ess.tar.gz"
tar -xzf "$TMP/ess.tar.gz" -C "$TMP"
D="$(echo "$TMP"/*/)"
SHA="$(basename "$D" | sed 's/.*-//')"

awk -v src="$REPO@$SHA" '
  BEGIN { print "{"; printf "  \"source\": \"%s (regenerate: scripts/regen-ess-api.sh)\",\n", src; print "  \"functions\": ["; first=1 }
  FNR==1 { fn=FILENAME; sub(/^.*\//,"",fn) }
  {
    if (match($0, /^function[ \t]+Ess\.[A-Za-z_]+\.[A-Za-z_]+[ \t]*\(/)) { s=substr($0, RSTART); sub(/^function[ \t]+/, "", s) }
    else if (match($0, /^Ess\.[A-Za-z_]+\.[A-Za-z_]+[ \t]*=[ \t]*function[ \t]*\(/)) { s=$0; sub(/[ \t]*=[ \t]*function[ \t]*/, "(", s) }
    else { next }
    p=index(s,"("); name=substr(s,1,p-1); gsub(/[ \t]+$/,"",name);
    rest=substr(s,p+1); q=index(rest,")"); args=(q>0)?substr(rest,1,q-1):rest;
    gsub(/[ \t]+/," ",args); gsub(/^ | $/,"",args);
    ns=name; sub(/^Ess\./,"",ns); sub(/\..*/,"",ns);
    if(!first){ printf ",\n" } first=0;
    printf "    {\"name\": \"%s\", \"ns\": \"%s\", \"sig\": \"(%s)\", \"file\": \"%s\"}", name, ns, args, fn;
  }
  END { print "\n  ]"; print "}" }
' "$D"src/*.lua > "$OUT/ess_api.json"

cp "$D"api/natives.json "$OUT/ess_natives.json"
echo "wrote $OUT/ess_api.json ($(grep -c '"name"' "$OUT/ess_api.json") functions) and ess_natives.json"
