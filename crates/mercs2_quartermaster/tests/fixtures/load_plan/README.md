# `load_plan` fixtures

The shared fixtures of `qm preflight`'s `load-request.json` and `load-plan.json`, loaded by
`tests/compat.rs` and `tests/cli.rs`.

## The chain

`my-mod` requires `{ shipment: ess, version: ">=0.7, <1" }`; `ess` requires
`{ shipment: lua-bridge, version: "^1.0.0" }`; `lua-bridge` requires nothing. There is no `m2-sdk`
in this chain, because Ess does not require it.

| Request | Game | Golden plan | Result |
|---|---|---|---|
| `request.chain.json` | `game-clean` | `plan.chain.pass.json` | ok; order lua-bridge, ess, my-mod |
| `request.chain-old-ess.json` | `game-clean` | `plan.chain.fail-old-ess.json` | M0204: ess 0.6.1 is outside my-mod's range |
| `request.chain-old-bridge.json` | `game-clean` | `plan.chain.fail-old-bridge.json` | M0204: lua-bridge 0.5.4 is outside Ess's range |
| `request.chain.json` | `game-legacy` | `plan.chain.fail-legacy-file.json` | M0208: `scripts/OnLoad/1_Ess.lua` is present |

Golden plans are compared as parsed JSON, with `quartermaster` replaced by the running version.

## Game layouts

- `game-clean/data/vz.wad` and `game-legacy/data/vz.wad` are empty: preflight only needs the file to
  exist, and never opens it.
- `game-legacy/scripts/OnLoad/1_Ess.lua` is the legacy OnLoad install Ess supersedes.
- `game-bad/vz.wad` is not in a `data` directory, so no game folder can be derived (exit 2).

## Binaries

Both are generated, not captured, and `tests/compat.rs::the_fixture_binaries_match_their_generators`
checks the committed bytes against the generators:

- `shipments/lua-bridge*/src/lua_bridge.asi`: `minimal_i386_dll()` in `tests/compat.rs`, a
  header-only i386 PE32 DLL (`MZ`, `e_lfanew`, `PE\0\0`, machine `0x014C`, characteristics
  `0x230E`).
- `shipments/ess-*/src/ess_ui.gfx`: `minimal_gfx()` in `tests/compat.rs`, a small uncompressed GFX
  movie.

## `invalid/`

Each request there makes `qm preflight` exit 2 and write no plan: a bad request `format`, a
duplicate id, an id containing `\`, an unknown key (a leftover `kind`), and manifests with a
self-`requires` (M0173), a bad range (M0172), the `{ name, version }` form, `format: 1`, the
`{ url, sha256 }` form (a parse failure), and a Shipment named after a deny-listed DLL stem (M0211).

Not here yet: `dup-runtime` (needs `add_runtime_dll`) and `stringdb-a` / `stringdb-b` (need per-key
stringdb claims).
