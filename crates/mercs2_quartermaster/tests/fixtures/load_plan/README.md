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

## Runtime DLLs and string tables

| Request | Game | Golden plan | Result |
|---|---|---|---|
| `request.dup-runtime.json` | `game-clean` | `plan.dup-runtime.json` | M0162 + M0207: `dup-runtime` ships `m2-sdk.dll`, which is not `dup-runtime.dll` and collides with `m2-sdk`'s |
| `request.stringdb.json` | `game-clean` | `plan.stringdb.json` | ok: `stringdb-a` and `stringdb-b` both edit `english`, one key alike; `qm link` merges them, and `link_block_paths` names the merged table's block |

- `shipments/m2-sdk/` is a runtime Shipment: one `add_runtime_dll`, `src/m2-sdk.dll`.
- `shipments/dup-runtime/` ships `src/m2-sdk.dll` too, so it collides with `m2-sdk`.
- `shipments/stringdb-a/` edits `[Menu.Play]` and `[Menu.Quit]` and also replaces a text
  (`replace_stringdb_text`); `shipments/stringdb-b/` edits `[Menu.Play]` and adds
  `[StringdbB.Added]`. Writers to one table never conflict, across Shipments or inside one: the
  link applies their writes in load order, the later winning, a
  text replacement resolving against the table as merged so far. `stringdb-a`'s pairs file
  (`old<TAB>new`, free text) replaces the text its own edit gives `[Menu.Quit]`.
- These two sets are for `qm preflight`, which never reads their text files: `[Menu.Play]` and
  `[Menu.Quit]` are not keys of the retail `english` table (read from the game, 2026-09-25), so they
  do not build against it. The merge is exercised against the real table by `tests/build.rs`.

## Game layouts

- `game-clean/data/vz.wad` and `game-legacy/data/vz.wad` are empty: preflight only needs the file to
  exist, and never opens it.
- `game-legacy/scripts/OnLoad/1_Ess.lua` is the legacy OnLoad install Ess supersedes.
- `game-bad/vz.wad` is not in a `data` directory, so no game folder can be derived (exit 2).

## Binaries

Both are generated, not captured, and `tests/compat.rs::the_fixture_binaries_match_their_generators`
checks the committed bytes against the generators:

- `shipments/lua-bridge*/src/lua_bridge.asi`, `shipments/m2-sdk/src/m2-sdk.dll` and
  `shipments/dup-runtime/src/m2-sdk.dll`: `minimal_i386_dll()` in `tests/compat.rs`, a header-only
  i386 PE32 DLL (`MZ`, `e_lfanew`, `PE\0\0`, machine `0x014C`, characteristics `0x230E`).
- `shipments/ess-*/src/ess_ui.gfx`: `minimal_gfx()` in `tests/compat.rs`, a small uncompressed GFX
  movie.

## `invalid/`

Each request there makes `qm preflight` exit 2 and write no plan: a bad request `format`, a
duplicate id, an id containing `\`, an unknown key (a leftover `kind`), and manifests with a
self-`requires` (M0173), a bad range (M0172), the `{ name, version }` form, `format: 1`, the
`{ url, sha256 }` form (a parse failure), and a Shipment named after a deny-listed DLL stem (M0211).
