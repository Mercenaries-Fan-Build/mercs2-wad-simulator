# `mercs2_player` — deferred backlog

Per the plan's §6 convention: everything that changes shipped behaviour is logged here rather than baked
into the faithful pass. `[faithful-blocker: yes]` means the reimpl cannot be correct until it is closed;
`[faithful-blocker: no]` means the *fix* would be the divergence.

Oracle: `docs/reverse_engineer/player_code_map.md` (all 107 cfunc bodies read).

---

## Faithful bugs deliberately reproduced

These look like defects because they **are** defects — shipped ones. Each has a test asserting the
wrong-looking behaviour, so a well-meaning "fix" fails loudly.

- **Five profile setters never arm the autosave.** `+0x11` is the dirty flag and it gates `autoSave`
  (`FUN_00614540` `0x00614891 cmp byte [eax+0x11],0 / je`). `SetCash` (`0x005DF4FE`), `SetFuelCapacity`
  (`0x005DF778`), `SetProfileCharacter` (`0x005DF828`), `SetProfileCostume` (`0x005DF978`) and
  `SetAvailableCostumes` (`0x005DFB98`) are all bare `mov`s. So changing cash, fuel capacity, character,
  costume or the costume roster **alone** leaves the profile un-saved. Named exhaustively in
  `profile::NON_DIRTYING_SETTERS` — earlier revisions of the map listed three of the five, and an
  incomplete enumeration produces an incomplete fix. `[faithful-blocker: no]`
- **`SetCash`/`SetFuel` take an undocumented optional second boolean that suppresses the write
  entirely** (`0x005DF4EE`, `0x005DF63E`) — store *and* dirty OR. No shipped script passes it, so it can
  only surprise a new caller (a fix-pack calling `SetCash(n, true)` silently no-ops).
  `[faithful-blocker: no]`
- **`Add*` dirty on the delta, not on old-vs-new.** `AddCash(0)` does not dirty; `AddCash(n)` dirties
  even when the zero-clamp makes it a no-op. `[faithful-blocker: no]`
- **`GetCurrentLocalPlayers` always returns `1.0`** — an `.rdata` constant (`[0x00B9B664]`), not a
  query. Implementing it honestly diverges from retail on the split-screen path. `[faithful-blocker: no]`
- **`GetMaximumLocalPlayers` always returns `2.0`** (`[0x00B92874]`), and **`GetMaximumPlayers` reports
  `DAT_017C0DD0`, which nothing enforces** — the real cap is three separate compile-time immediates.
  `[faithful-blocker: no]`
- **`GetAnyCharacter` performs no lookup**, pushing the constant sentinel `0xF0000000` (223 call sites).
  `[faithful-blocker: no]`

## Inventions removed

The previous implementation was *less* faithful than the map in three places. Recorded so they are not
reintroduced by someone who remembers the old behaviour:

- The **1-billion cash clamp** is a Lua soft-clamp in `MrxPmc.AddCashQty`, not native — and
  `mrxpmc.lua:474,538` bypass it by calling `Player.AddCash`/`SetCash` directly. The native ceiling is
  `i32::MAX`.
- The **fuel-to-capacity clamp** is `mrxpmc.lua:114-115`'s job. Retail's `SetFuel` stores a raw dword.
- **`AddCash`/`AddFuel` returning a running total.** Retail pushes nothing.

---

## Faithful blockers

- **S1 — the writer of `player+0x24` (the control source).** `FUN_006A4060` only ever *clears* it
  (`0x006A4279`); the write that sets it to a ridden vehicle is not statically reachable. Every function
  touching a `Players`/`SeatLink` global was disassembled and every `mov [reg+0x24]` hit turned out to be
  a local argument struct, a generic list-insert, or an unrelated ctor. Consequence: `GetSeat`,
  `GetControlledObject`, `GetControlBindingType` and `Object.IsPlayerControlled` have no authentic
  producer, so the seat/ride system pushes it in via `PlayerWorld::set_control_source`.
  *Runtime recipe:* one-shot bp at `0x005DA9F7` to capture `playerObj`, then a HW **write** watchpoint on
  `+0x24`, then walk into a vehicle. `[faithful-blocker: yes]`
- **S3 — two unnamed hashes block `FUN_0041FE20`** (the per-player world probe): `0x892CF579` (feature
  gate) and `0x223F6FDA` (Havok filter). Every token in both the PC and Xbox images was hashed under both
  engine hash functions with no match — the same sweep *did* resolve `0xFA62754E → "PDA"`, so the method
  works and these names simply are not strings in either image. Carried in `system.rs` as named-unknowns.
  *Next step:* harvest candidates from `vz.wad` string tables via `mercs2_probe`, not from the exe.
  `[faithful-blocker: yes]`
- **`player+0x66`'s mode enum.** Only the comparison `== 4` is known. The live lead is the Xbox build's
  `PgPlayerPDAMapMode` / `PgPlayerBinocularsMode` / `PgPlayerHumanMode` / `PgPlayerEnterSeatMode` /
  `PgPlayerSeatedMode` classes (PPC strings 7507–7533); `PgPlayerPDAMapMode @825666a8` has a decompiled
  body in `docs/mercs2-pdb-analysis/gui-hud.md:244`. Anything gating on a mode other than 4 is blocked.
  `[faithful-blocker: yes]`
- **`GetVehicleDisguiseState`'s return type.** The map calls it an integer summed from two sub-queries
  off `player+0x3A8` (`FUN_006ABC30`/`FUN_006ABC50`), but those were not read, and the only shipped
  consumer `tostring()`s the result against `"true"`/`"false"`. A **boolean** is what satisfies the
  contract, so that is what is pushed. *Recipe:* bp at `0x005E0527`, which also closes `FUN_006CDB70`'s
  M→H. `[faithful-blocker: yes]`
- **`SetOutfit`'s three streaming calls.** `FUN_005DF980` adds the outfit component then drives
  `FUN_00874300`/`FUN_00874320`/`FUN_00874290`. The request lands; the streaming does not exist. This is
  the engine-side confirmation of the `STATE_WAITFORGAME` wardrobe wedge. `[faithful-blocker: yes]`
- **`FUN_006CDB70` is confidence M**, not H — it is a SecuROM VM stub named behaviourally (one register
  argument, returns a `Players` record, the Lua splits 6/6 on handle type). Raising it needs the SecuROM
  recovery pipeline or a live `EAX` compare at `0x005E0527`. Four cfuncs resolve through it.
  `[faithful-blocker: no]` — the observable behaviour is pinned.
- **S2 — the writer of `player+0x58`** (the remote flag). The *semantic* is H (`IsJoined`/`IsLocal`/
  `IsRemote` pin it), but `BindToLocal`/`BindToRemote`/`Unbind` are all SecuROM VM stubs, so which one
  assigns it is inferred. Marked `CONFIRM-LIVE` at each site. `[faithful-blocker: no]`

---

## Recorded tensions

- **Roster-tick vs vehicle-pump order.** `player_code_map.md` §1's *diagram* puts the vehicle-control
  pump before the two roster passes; the recovered call-site addresses put it after
  (`0x004C9861` roster A → `0x004C9869` probe → `0x004C9900` roster B → `0x004C990C` pump). Byte order in
  `FUN_004C9740` is the harder evidence, so `GameplaySystems::tick` runs the roster first. A map-vs-map
  disagreement, not a decision — recorded rather than silently resolved.
- **The frame gate.** §1 proves the real gate is `[0x01175A94] != 1`, checked twice, and that a stateless
  frame **still ticks the world** — the older "does not tick in shell/loading/pause" claim is withdrawn.
  A `GameplaySystems::level_transition` flag that skips the whole per-system list would model it, but
  that is a `GameplaySystems`-wide change, not a player one. Noted, not done.

## Enhancements (would change shipped behaviour)

- Fixing any of the six faithful bugs above — most usefully the five non-dirtying setters, since the
  observable symptom is "the game forgets you bought something".
- The `LocomotionQuery`-backed controller in `locomotion.rs` is a **modern stand-in**, not a recovered
  body: retail is `hkpCharacterProxy` + the 5-state `hkpCharacterContext` machine
  (`physics_code_map.md`). Replacing it is the physics system's work, and the seam is sized for exactly that swap.
