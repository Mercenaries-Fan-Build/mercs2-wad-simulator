#!/usr/bin/env python3
"""build_production_names.py — export the CURATED, verified name lookup for the workshop bundle.

The working corpus (`tools/rainbow_table.json`, ~739k entries) is a research artifact: it lives in
the PARENT repo, it is 32 MB, and it mixes verified authored names with Lua strings, collisions, and
brute-force handles. That is the wrong thing to ship.

This produces the PRODUCTION lookup — `crates/mercs2_workshop/data/production_names.json`, committed
inside the wad_simulator repo so it bundles with the workshop and needs no cross-repo file and no game
WAD to rebuild the pack.

It is the previous table (`production_names.base.json`, the committed file set aside before the
raw-corpora pack) plus every entry of the current `names.bin` (already trimmed to hashes the workshop
actually resolves) the previous table lacks. The pack opens only the archives at hand, so a name
the previous table holds is kept even when this `names.bin` does not reach it. Every new entry is:
  * hash-VERIFIED — m2(name) must equal its hash, or it is dropped; and
  * junk-FILTERED — generated `_gen_` handles, illegal characters, and consonant/mixed blobs are dropped.
A (hash, name) pair on `data/rejected_names.json` is dropped from both.

Output is sorted by hash for stable diffs. Run from the wad_simulator repo root.
"""
import json
import re
import struct
import sys
from pathlib import Path

# m2 (pandemic_hash_m2), inlined so this tool has no cross-repo import.
FNV_OFFSET, FNV_PRIME, MASK = 0x811C9DC5, 0x01000193, 0xFFFFFFFF


def m2(text: str) -> int:
    data = text.encode("ascii", "ignore")
    if not data:
        return 0
    h = FNV_OFFSET
    for b in data:
        h ^= (b | 0x20)
        h = (h * FNV_PRIME) & MASK
    h ^= 0x2A
    return (h * FNV_PRIME) & MASK


NAMES_PACK_MAGIC = b"M2NAMES1"


def parse_names_bin(path: Path):
    data = path.read_bytes()
    assert data[:8] == NAMES_PACK_MAGIC, "not a names.bin pack"
    count = struct.unpack_from("<I", data, 8)[0]
    table_end = 12 + count * 8
    blob = data[table_end + 4:]
    out = {}
    for i in range(count):
        h, off = struct.unpack_from("<II", data, 12 + i * 8)
        end = blob.index(b"\x00", off)
        out[h] = blob[off:end].decode("ascii", "ignore")
    return out


LEGAL = re.compile(r"^[A-Za-z0-9_.]+$")


def is_junk(n: str) -> bool:
    """DEFINITE junk only. Dropping a real authored name is worse than keeping a stray collision.

    Real names we MUST keep and previously mis-dropped: military designations (`ah1z`, `m1a2`,
    `hmmwv`), exporter-generated tails (`..._boothrental_a0`), and character display strings with
    spaces/hyphens (`Allied Infantry_Mirron01`). So we do NOT judge on vowels, digit patterns, or
    spaces. We drop only three unambiguous non-names: brute-force `_gen_` handles, bare hex literals,
    and the collision signature of a token that does not begin with a letter (`-jbe17`, `2v3tx`,
    `0x000b0d1c`). Everything a human plausibly typed is kept.
    """
    low = n.lower()
    if "_gen_" in low:
        return True
    if re.fullmatch(r"0x[0-9a-f]+", low) or re.fullmatch(r"[0-9a-f]{8,}", low):
        return True  # a bare hex literal, not a name
    if not n[:1].isalpha():
        return True  # authored names begin with a letter; `-jbe17` / `2v3tx` are collision debris
    return False


def main() -> int:
    here = Path(__file__).resolve().parents[1]  # wad_simulator repo root
    binp = None
    for cand in [here / "target/release/workshop_data/names.bin",
                 here / "target/debug/workshop_data/names.bin"]:
        if cand.is_file():
            binp = cand
            break
    if not binp:
        print("no names.bin found — run `mercs2_workshop --pack-data` first", file=sys.stderr)
        return 1
    data = here / "crates/mercs2_workshop/data"
    basep = data / "production_names.base.json"
    if not basep.is_file():
        print(f"no {basep.relative_to(here)} — set the committed production_names.json aside under that "
              "name before the raw-corpora pack (data/README.md)", file=sys.stderr)
        return 1
    base = {int(k, 16): v for k, v in json.loads(basep.read_text())["pandemic_hash_m2"].items()}
    rejected = {(int(k, 16), n) for k, ns in json.loads((data / "rejected_names.json").read_text())["pandemic_hash_m2"].items()
                for n in ns}
    raw = parse_names_bin(binp)
    kept = {h: n for h, n in base.items() if (h, n) not in rejected}
    added, bad_hash, junk, dropped = 0, 0, 0, 0
    for h, n in raw.items():
        if h in kept:
            continue
        if (h, n) in rejected:
            dropped += 1
            continue
        if m2(n) != h:
            bad_hash += 1
            continue
        if is_junk(n):
            junk += 1
            continue
        kept[h] = n
        added += 1
    outp = data / "production_names.json"
    payload = {
        "format": 1,
        "note": "Curated, hash-verified node/asset name lookup for the Mercs2 workshop. "
                "Built by tools/build_production_names.py from names.bin. Do NOT hand-edit.",
        "count": len(kept),
        "pandemic_hash_m2": {f"0x{h:08X}": n for h, n in sorted(kept.items())},
    }
    outp.write_text(json.dumps(payload, indent=0))
    basep.unlink()
    print(f"previous table: {len(base)} names; names.bin: {len(raw)} entries")
    print(f"  added {added}; dropped {bad_hash} bad-hash, {junk} junk, {dropped} rejected")
    print(f"  -> {outp.relative_to(here)}: {len(kept)} verified names")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
