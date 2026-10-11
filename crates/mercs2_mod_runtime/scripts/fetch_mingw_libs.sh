#!/usr/bin/env bash
# Fetch the MinGW libraries Rust's i686-pc-windows-gnu target is built against (msvcrt CRT, DWARF-2
# libgcc) from the official rust-mingw component matching the active rustc, verify the tarball against
# static.rust-lang.org's published sha256, and place them in toolchain/i686-mingw/ with a VERSION file
# that build.rs checks. Linking against these instead of the host MinGW's keeps m2-sdk.dll on msvcrt.dll.
# Run from anywhere; the libraries land in crates/mercs2_mod_runtime/toolchain/.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
full="$(rustc -V)"
ver="$(rustc -V | cut -d' ' -f2)"
name="rust-mingw-${ver}-i686-pc-windows-gnu"
url="https://static.rust-lang.org/dist/${name}.tar.xz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -sSfL "$url" -o "$tmp/$name.tar.xz"
curl -sSfL "$url.sha256" -o "$tmp/$name.tar.xz.sha256"
published="$(cut -d' ' -f1 "$tmp/$name.tar.xz.sha256")"
actual="$(shasum -a 256 "$tmp/$name.tar.xz" | cut -d' ' -f1)"
if [[ "$published" != "$actual" ]]; then
  echo "sha256 mismatch for $url: published $published, downloaded $actual" >&2
  exit 1
fi
tar -xJf "$tmp/$name.tar.xz" -C "$tmp"
src="$tmp/$name/rust-mingw/lib/rustlib/i686-pc-windows-gnu/lib/self-contained"
rm -rf toolchain/i686-mingw
mkdir -p toolchain/i686-mingw
cp "$src"/*.a toolchain/i686-mingw/
printf '%s\n' "$full" > toolchain/i686-mingw/VERSION
echo "fetched $name (sha256 $actual) for: $full"
ls toolchain/i686-mingw
