#!/usr/bin/env bash
# Locate a PC `vz.wad` on this machine and (optionally) record it for the test suite.
#
# The game-dependent tests in `mercs2_quartermaster` skip when they cannot find a WAD. Skipping is
# the safe default, but a test that only runs when someone remembers an env var is a test that
# stops running — so this writes a machine-local, git-ignored pointer instead.
#
#   scripts/find-vz-wad.sh            # print what it finds
#   scripts/find-vz-wad.sh --write    # also write .mercs2-local.toml at the repo root
#
# Resolution the tests use (crate `game::discover`), first hit wins:
#   1. $MERCS2_VZ_WAD
#   2. .mercs2-local.toml   <- what --write produces
#   3. Mercenaries2.exe next to the binary, then data/vz.wad
#   4. the EA registry key (Windows only)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WRITE=0
[[ "${1:-}" == "--write" ]] && WRITE=1

# The PC bake is little-endian and its magic reads `FFCS`. The Xbox 360 / PS3 bakes are big-endian
# and read `SCFF` — byte-identical content, unreadable by this toolchain. Checking the magic is the
# only reliable discriminator; filenames are not (`pc-game-vz.wad` and `xbox-vz.wad` sit together).
is_pc_wad() {
    local f="$1"
    [[ -f "$f" ]] || return 1
    [[ "$(head -c 4 "$f" | LC_ALL=C od -An -tx1 | tr -d ' \n')" == "46464353" ]]
}

CANDIDATES=()
[[ -n "${MERCS2_VZ_WAD:-}" ]] && CANDIDATES+=("$MERCS2_VZ_WAD")

# Sibling checkouts of the notes repo, where the corpus and game files live.
for base in "$HOME/src/mercenaries-game" "$REPO_ROOT/.." "$REPO_ROOT"; do
    CANDIDATES+=("$base/game-files/vz.wad" "$base/game-files/pc-game-vz.wad" "$base/data/vz.wad")
done

# Common install locations.
CANDIDATES+=(
    "$HOME/Library/Application Support/Steam/steamapps/common/Mercenaries 2/data/vz.wad"
    "/Applications/Mercenaries 2/data/vz.wad"
    "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad"
    "C:/Program Files/EA Games/Mercenaries 2 World in Flames/data/vz.wad"
)

FOUND=""
REJECTED=()
for c in "${CANDIDATES[@]}"; do
    [[ -f "$c" ]] || continue
    if is_pc_wad "$c"; then
        FOUND="$c"
        break
    else
        REJECTED+=("$c")
    fi
done

for r in "${REJECTED[@]:-}"; do
    [[ -n "$r" ]] && echo "skipped (not a little-endian PC bake): $r" >&2
done

if [[ -z "$FOUND" ]]; then
    cat >&2 <<EOF
No PC vz.wad found.

Searched \$MERCS2_VZ_WAD, sibling game-files/ directories, and the usual install paths.
Game-dependent tests will SKIP (they will not fail). To point at one explicitly:

    scripts/find-vz-wad.sh --write   # after setting MERCS2_VZ_WAD=/path/to/vz.wad

or write $REPO_ROOT/.mercs2-local.toml by hand:

    vz_wad = "/path/to/vz.wad"
EOF
    exit 1
fi

SIZE=$(wc -c < "$FOUND" | tr -d ' ')
echo "found: $FOUND"
echo "size:  $SIZE bytes"

if [[ "$WRITE" == "1" ]]; then
    CONFIG="$REPO_ROOT/.mercs2-local.toml"
    cat > "$CONFIG" <<EOF
# Machine-local game paths. GIT-IGNORED — never commit this; the path is specific to one machine
# and the WAD itself is a retail asset we do not redistribute.
# Written by scripts/find-vz-wad.sh. Consumed by mercs2_quartermaster::game::discover.
vz_wad = "$FOUND"
EOF
    echo "wrote: $CONFIG"
fi
