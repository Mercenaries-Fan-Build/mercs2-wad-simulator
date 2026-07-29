# mercs2_quartermaster

The **Shipment** format for Mercenaries 2 mods, plus the linter and builder that read it.

A *Shipment* is a mod package: a `manifest.yaml` describing what it contributes, a `src/` directory
holding the files, and nothing else. The *Quartermaster* is what turns one into an overlay WAD the
game will load.

Neither the Workshop nor Modkit owns the format — this crate does, and both are clients.

## Why a linter at all

Mercenaries 2 does not report asset errors. It hangs on a loading screen.

A texture whose declared page count is one short of what it inflates to will overrun the engine
heap. An ASET row naming a LOD block that is not in the WAD makes the streamer size a buffer from a
garbage index and request something on the order of 549 GB. Neither produces a message, a log line,
or a crash dump — just a frozen screen and a modder with nothing to go on.

Every rule here is one of those traps, reversed out of the engine and given a stable code, a
one-line title, and a link to where the trap is written up. That rule set is the real product; the
builder is what makes it enforceable.

Rules that are known but **not yet implemented** stay registered and are printed by `qm rules` under
their own heading. A linter that silently omits its most dangerous checks reads as a clean bill of
health, which is worse than no linter.

## The `qm` CLI

```
qm lint  [DIR]                     check a Shipment — no game install needed
qm build [DIR] --out DIR           lower it into an overlay WAD
qm link  DIR... --out DIR          link several installed Shipments' Lua into one WAD
qm rules                           what is checked, what is not, and where each is documented
```

Prebuilt binaries are attached to each release, so nothing here requires a Rust toolchain. Modkit
manages the local copy.

### Exit codes

```
0  clean
1  findings at Error or above, including every HANG-class rule
2  the command could not run at all (no manifest, no game stack, bad usage)
```

A build is gated on the **exit code**, never on a printed count — a caller that discards stdout must
still be unable to ship a broken Shipment.

`1` and `2` are distinct on purpose. CI needs to tell "this Shipment is wrong" from "this runner has
no game install"; collapsed into a single nonzero code, a misconfigured runner reads as a failing
mod.

## Three stages, because they need different things

| stage | inputs | where it runs |
|---|---|---|
| `lint` | manifest text + `src/` | anywhere, including CI |
| `game_checks` | + the retail WADs | the author's machine |
| `artifact_checks` | + the WAD just assembled | after lowering, before the write |

`lint` is **hermetic**: no game install, no network. That is what lets a public runner — which will
never have the retail WADs — gate every push.

`artifact_checks` is the only stage that can catch a defect the *lowering* introduced rather than
one the author wrote, and it runs before the file is written, so a WAD that would hang the game
never reaches disk where its presence would read as success.

## Composition

Two mods that each ship their own copy of the same block silently annihilate each other, and the
engine's arbitration rules do not agree with one another:

- the WAD stack is **last-mounted-wins**
- the runtime chunk registry is **first-writer-wins**
- string databases are **last-registered-wins**, with a cap of eight
- ASI plugins arbitrate not at all

So a Shipment declares its blast radius — a write-set *and* a read-set — and the Quartermaster
resolves conflicts before anything is built. The default is fail-closed: a contribution whose effect
cannot be reasoned about is treated as exclusive rather than assumed safe.

Lua is the case that forced the design. Scripts load from a *block*, not per-hash, so editing one
script means re-emitting every script in that block — and two script-touching Shipments cannot both
win. `qm link` therefore composes the installed set's declared source-appends onto the base script,
compiles once, and emits a single WAD mounted last.

`patch_lua` reaches **two** blocks: `scripts_vz` (114 content scripts — contracts, jobs, tutorials)
and `resident` (~240 always-loaded framework modules, `Mrx*` and the world-entity scripts). A target
is resolved against both, and only the block it lands in is republished.

The resident block is **mixed** — those ~240 Lua chunks sit among ~7,000 entries of other types — so
two things there are not optional. The lookup is type-aware, or a 32-bit name-hash collision would
splice compiled Lua into a texture. And the republished block carries an ASET row for **every**
entry, copied from the base WAD: an asset in a block with no row naming it is the M0004 hang, and
`type_id` selects the loader, so it has to come from the WAD rather than from a type-hash table
(ours is wrong for 12 of 36 ids).

## Status

Every kind lowers end-to-end against retail WADs except `edit_state_machine`, which returns
`Unsupported` **with the reason** rather than being quietly skipped — a dropped contribution
produces a WAD that looks fine and does nothing. (`raw` and `native_hook` used to be in that
sentence too; both lower now.)

The Code layer is two kinds, and the split is about who chooses the destination. `native_hook`
places one `.asi` and chooses the directory itself; `place_file` places the companions that `.asi`
reads — the `.ini` beside it, a Lua framework's `.lua` files — and lets the author pick a
destination **name** from a closed set (`game_root`, `scripts`, `plugins`, `update`, `on_boot`,
`on_load`, `on_key`). Neither takes a path, so `Mercenaries2.exe` and `data/vz.wad` stay unreachable
by construction rather than by a rule that could be suppressed: a path-shaped `dest:` does not
parse, and the filename comes from the source file, with `.wad`, `.exe`, `.dll` and the loader's own
reserved name refused outright. Both need no game stack, so both lower in template CI.

`add_movie` is the odd one out in a useful way: it is the only Data kind that needs no game stack.
A Scaleform GFx movie is self-contained — no donor to borrow a rig from, no target whose dimensions
it must match — so it builds where the retail WADs do not exist, which is where template CI runs.
The movie is validated by parsing it and then injected **verbatim**; retail ships 61 compressed
`CFX` movies and 3 uncompressed `GFX`, so there is no encoding to normalise to and nothing is
re-encoded in either direction.

## License

MIT.
