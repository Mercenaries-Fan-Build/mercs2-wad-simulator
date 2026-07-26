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
PWD_ROOT="$(pwd)"
WRITE=0
[[ "${1:-}" == "--write" ]] && WRITE=1

# Platform is read from the 4-byte magic, not the filename — a game-files/ directory legitimately
# holds every platform's bake side by side, and the names vary:
#
#   PC        FFCS  little-endian, `sges` blocks
#   Xbox 360  SCFF  big-endian,    `segs` blocks
#   PS3       SCFF  big-endian     (this dump; docs/ps3_wad_wrapper.md describes an older
#                                   1 GiB dump with an unknown/encrypted header instead)
#
# The console bakes are deliberately kept: Shipments are expected to export to every platform. The
# BUILDER cannot emit for them yet (ucfx_byteswap converts console -> PC only), so this script
# prefers PC while still reporting the rest.
wad_platform() {
    local f="$1"
    [[ -f "$f" ]] || { echo "missing"; return; }
    case "$(head -c 4 "$f" | LC_ALL=C od -An -tx1 | tr -d ' \n')" in
        46464353) echo "pc" ;;
        53434646) echo "console" ;;
        *)        echo "unknown" ;;
    esac
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
    "$PWD_ROOT/data/vz.wad"
)

FOUND=""
OTHERS=()
for c in "${CANDIDATES[@]}"; do
    [[ -f "$c" ]] || continue
    case "$(wad_platform "$c")" in
        pc)      [[ -z "$FOUND" ]] && FOUND="$c" ;;
        console) OTHERS+=("console  $c") ;;
        *)       OTHERS+=("unknown  $c") ;;
    esac
done

for o in "${OTHERS[@]:-}"; do
    [[ -n "$o" ]] && echo "also present: $o" >&2
done

if [[ -z "$FOUND" ]]; then
    cat >&2 <<EOF
No PC vz.wad found.

Searched \$MERCS2_VZ_WAD, sibling game-files/ directories, and the usual install paths.
Console bakes, if any, are listed above — they are readable but the builder cannot emit for
them yet, so they are not selected here.
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
