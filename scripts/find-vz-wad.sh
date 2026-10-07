#!/usr/bin/env bash
# Locate a PC `vz.wad` on this machine and (optionally) record it for the test suite.
#
# Game-gated tests (built by each crate's `retail` feature, run with `cargo xtask retail-test`) find
# the game ONLY through the repo-root .mercs2-local.toml this script writes. They consult no
# environment variable, and they FAIL — naming that file and the key — when it is missing, has no
# key the test needs, or a key names a path that is not a file.
#
#   scripts/find-vz-wad.sh            # print what it finds
#   scripts/find-vz-wad.sh --write    # also write .mercs2-local.toml at the repo root
#
# Keys in .mercs2-local.toml, one `key = "path"` per line:
#
#   vz_wad        the PC base archive; every game-gated test reads it. This script writes it.
#   xbox_vz_wad   an Xbox 360 bake (SCFF magic); read by the qm console-bake tests.
#   ps3_vz_wad    a PS3 bake (SCFF magic); read by the qm console-bake tests.
#
# --write rewrites the whole file with `vz_wad` only. The console keys are added by hand after it
# runs; this script reports console bakes it finds ("also present: console …") but never writes them.
#
# The same file is also a low-priority source for the tools (`mercs2_quartermaster::game::discover`,
# `mercs2_formats::game_paths::vz_wad`), which check the environment first.
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
Game-gated tests (`cargo xtask retail-test`) will FAIL until .mercs2-local.toml names one.
To point at one explicitly:

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
# Written by scripts/find-vz-wad.sh. The only source game-gated tests read
# (mercs2_formats::game_paths::local_config_vz_wad); also read by mercs2_quartermaster::game::discover.
vz_wad = "$FOUND"
EOF
    echo "wrote: $CONFIG"
fi
