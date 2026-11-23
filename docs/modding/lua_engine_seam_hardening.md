---
title: Hardening the Lua↔engine seam for mods
status: design
evidence: inferred
date: 2026-09-15
---

# Hardening the Lua↔engine seam for mods

## The premise

Mercenaries 2 shipped with a **hostile** boundary between its Lua game-scripts and the native
engine. It is not that the seam is thin — retail installs **1086 required cfuncs across 35+
namespaces** ([`binding_burndown.md`](../modernization/binding_burndown.md); Player 107 / Hud 114 / Object 87 / Pg 80 / Ai 66 / Sound 88
/ Graphics 75 / _GuiInternal 114 / …) and every mission in `vz/*.lua` reaches through them
constantly — it is that the seam is **silent when it fails**. A Lua error inside an engine
callback gets `pcall`'d and swallowed, leaving no visible trace. A save that references a stale
mission ID is written back untouched. A layer added by a mission but never removed lingers on
load. The retail game itself gets away with this because its own missions were authored and QA'd
against the same monolithic build; the shipped scripts stay narrowly inside the undocumented
rules the engine assumes.

The moment we open this boundary to **modders**, we inherit responsibility for every failure mode
those undocumented rules were quietly masking. Uninstall poisoning, mid-callback crashes, layer/
faction leaks, save-schema drift, event-handle leaks, cross-mod hash collisions, index-shift
save re-dresses — none of these are bugs in the shipped game. They are bugs in **the seam we
exposed**. It is on us to close them at the engine surface, not on individual mod authors to
hand-roll them per mod in Lua.

We are uniquely positioned to do so. Between `pmc_bb.dll` (native hooks + Lua logging in the game
process), `qm` (build-time Shipment linter/builder), the SDK ASI (runtime engine surface), Modkit
(installer/registry), and the 27k-function Ghidra decompilation (behavioural ground truth), we own
the whole vertical. Wally's `mercs2-lua-essentials` framework built the best pure-Lua answer to
this from the outside — its `Ess.Sandbox` (per-provider mutation tracking + rollback) and
`Ess.Contract` (contract framework that deliberately does not touch the native contract system,
per `80_contract.lua`'s header) are, together, the reference specification of what a hardened
native seam needs to cover. From the *inside*, everything Ess opts out of in Lua can be made
safe by construction.

This document is a design for what "safe by default" looks like at the seam, split by which tool
owns which guarantee.

## Principles

Four rules, in decreasing order of priority. Later rules give way when they conflict with earlier.

1. **The mod author's Lua does not know about the seam's failure modes.** A Shipment that declares
   what it touches gets safety for free. There is no "call this to opt in", no `Ess.Sandbox.begin`,
   no `pcall` wrapper the author must remember. If the author has to know about a footgun to avoid
   it, we have already lost.
2. **Prefer structural fixes over documentation.** The Ess codebase itself makes this
   explicit — Design Principle 2 (`FEATURE_SHEET.md`, 2026-07-17): "make a footgun impossible, don't
   warn about it." A `qm lint` failure at the author's machine is worth a thousand `docs/` warnings
   at the player's.
3. **Every enforcement happens twice.** Once at build time in `qm` (statically preventable rules
   become compile errors), once at runtime in pmc_bb/SDK (dynamic conditions the build tool cannot
   see: mod uninstalled between save and load, save transferred cross-machine, mid-callback crash).
   Belt and suspenders — build-time checks are cheap and prevent 90% of shipped bugs; runtime
   checks catch the remaining 10% that only exist at the player's machine.
4. **Ess.Contract's approach stays valid — as one point on a spectrum.** Once the hardened seam
   exists, an author picks the ceremony they want: full native contract with save-resume (uses the
   hardened seam), or ephemeral contract with no save contract at all (uses `Ess.Contract` or an
   equivalent). Neither is second-class. The seam is safe *whether or not* the author engages the
   native mission pipeline.

## Blockers to name up front

Two design constraints shape everything below. Neither is a research task; both are physical facts
of the retail engine that any hardening plan must fit inside.

- **The `.profile` is a fixed 13,404-byte file** ([`SAVE_FORMAT.md`](../../crates/mercs2_formats/SAVE_FORMAT.md)).
  The little-endian header runs 0…0x467, the zlib payload lives at 0x468, and the retail loader
  rejects anything else. Any per-mission save envelope the steward wraps must budget bytes inside
  the deflated Lua payload — there is no room to grow the file without breaking the retail exe as
  a reader. The steward has to enforce a per-mission `data` size cap (proposed 512 B), quarantine
  the least-recently-active row on overflow, and surface `[mod-quarantine]` to the visible log.
- **The `ProfileHash` integrity word is DERIVED — CRC-32/BZIP2, non-reflected, over `[4:]`**
  ([[profile-hash-is-crc32-bzip2]], shipped in
  [`save_write::profile_hash`](../../crates/mercs2_formats/src/save_write.rs)).
  Byte-exact against every retail save fixture, round-trips clean. The steward can write mutated
  saves today; no RE prerequisite remains for step 5 below.

## The seam today (what modders trip on)

The failure modes divide into two groups: F1–F7 + F11–F12 for mission-shaped mods (add_script,
patch_lua into `mrxmissionflow`), F8–F10 for the other Shipment kinds (wardrobe outfits, store
items, localisation). Each is either directly seen in this project or directly implied by the Ess
codebase's own defensive design. (F11 and F12 were documented later than F1–F10, hence the ordering;
both are still mission-shaped.)

### F1–F7 — mission-shaped mods

**F1 — Uninstall poisoning.** `WifMissionFlow.SaveSingleton()`
([mrxmissionflow.lua:597](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxmissionflow.lua#L597))
walks `_tActiveMissions` and writes `oMission:SaveInstance()` for every entry. Load side
([mrxmissionflow.lua:632](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxmissionflow.lua#L632)):
`UnlockMission(sMissionName, tMissionSaveData, false)`. If the Shipment providing `sMissionName`
was uninstalled between save and load, `WifMissionData.tMissionData[sMissionName]` is nil,
`MrxUtil.CopyTable(nil)` returns `{}`, the empty config wedges the WAITFORSTREAMING gate, and the
loading screen sticks forever. Save is now a paperweight.

**F2 — Missing inherit → silent method-lookup death.** Confirmed live in this session
([[custom-mission-inherit-mrxtask-required]]). A custom-mission `add_script` module whose top-
level does not `inherit("MrxTask")` (or subclass) gets its `oMission.IsActive`/`SaveInstance`/
`Configure` resolved to nil, because `_ModuleLoaded`
([mrxtask.lua:294](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxtask.lua#L294)) replaces
the fresh MrxTask metatable with `{__index = this module}`. `RefreshAllPdaMissionDetails` calls
`oMission:IsActive()` on every support drop → `Event.Post("SupportUsed")` fanout unwinds the
`BeginSupportSequence` pcall → fuel deducted at
[mrxpmc:214](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxpmc.lua#L214), delivery never
runs, no visible error.

**F3 — Save-schema drift.** `SaveInstance`
([mrxtask.lua:388](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxtask.lua#L388))
`MrxUtil.CopyTable(_tSaveData)` — a raw deep-copy of whatever the mission stashed. If v1 of a mod
writes `{nWave=5, tSpawnedGuids={...}}` and v2 renames the fields, v2's `LoadAssets` reads garbage
from a v1 save. Author has no seam to declare a schema; engine has no way to detect drift.

**F4 — CopyTable and userdata.** `SaveInstance`'s deep-copy is table-shaped; live `uGuid`
(userdata) references stashed into `_tSaveData` are stored in-memory but do not survive
serialization. Shipped missions know this and store *names* (`"PMC003_EwanTaxi"`) to re-resolve via
`Pg.GetGuidByName` in `LoadAssets`. A modder without decomp access does not necessarily know this;
the seam does not warn.

**F5 — `pcall`-swallowed callback errors.** Every engine-invoked Lua callback (Event.*,
MrxLayerManager, MrxState transitions, Net.SendEvent_*) runs inside a native pcall frame. A Lua
error there prints nothing beyond an easy-to-miss `[lua]` line in `pmc_blackbox.log`, if that. In
practice: the mod stops working, the game stays up, and the modder has no idea which line broke.
Half the pain of debugging Mercs2 mods is this.

Two implementation constraints on the fix (both from
[`diagnostics_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/diagnostics_code_map.md) + [`dlc_mission_loading.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/dlc_mission_loading.md)):

- **Retail `Debug.Printf` / `print` are dead stubs on PC.** Both resolve to the shared no-op
  `0x006D5640` (`33 C0 C3`). Every visible `[lua]` line today exists only because `pmc_bb`
  re-hooks the slot at boot. A licensed dxwrapper build without the logging pmc_bb gets **no** Lua
  output at all — worse than an "easy-to-miss line". The one surviving live sink is
  `Sys.WriteToConsole` @ table `0x00B98A78`. The attribution+quarantine layer must sink to
  `Sys.WriteToConsole` and a native ring, not lean on `Debug.Printf`.
- **Native cfuncs cannot be wrapped from Lua.** `pcall`-wrapping `dynamic_import` / `Sys.*` or
  installing `__newindex` on `_G` crashes at `0x0059C82A` (`dynamic_import` writes to `_G`).
  Attribution therefore has to be installed at the native C→Lua dispatch — retail's real entry
  points are `luaD_call 0x008688D0` / `luaD_pcall 0x00868AD0` / `lua_pcall 0x0085DF50` /
  `luaB_pcall 0x008615F0` per [`scripting_host_binding_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/scripting_host_binding_code_map.md).
  (An earlier draft named `FUN_004b2a50`; verification against
  [`mission_contract_flow_validation.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/validation/mission_contract_flow_validation.md#p9-the-areas-pass-1-declared-unexamined-not-endorsed)
  showed that VA is a 27-byte push-nil-return helper, not `luaL_error` and not a dispatch point.)

**F6 — Layer/faction/hostility leaks.** A mission that adds a layer, mutates a faction attitude,
or calls `Pg.ContractActivated` owns the responsibility to reverse it in Cancel/Complete/Cleanup.
If the mod is uninstalled while the mission is active — or if the mission Cleanup itself throws
mid-way — the world stays in the mission's mutated state. `MrxLayerManager.SaveSingleton()`,
`MrxFactionManager.SaveSingleton()` etc. then serialize those mutations into the save even after
the responsible mission is gone. Wally's `Ess.Sandbox` (`63_sandbox.lua`) is exactly this problem
solved for one narrow domain, opt-in, in Lua.

**F7 — Event / callback handle leaks.** Distinct from F6 because Event handles are not
"mutations" — they are callback registrations. `Event.Create` (4 cfuncs total in the `Event`
table `0x007987F8`: `Create`/`CreatePersistent`/`Delete`/`Post`) returns a handle whose closure
captures `self`. Every shipped resident script (`alarm.lua`, `jammer.lua`, `moonpatrol.lua`,
`goal.lua`, `heavymg.lua`) and every mission (`pmccon001.lua`, dozens more) manually pairs
`Event.Create` → `Event.Delete` via `self._tEvents[k]` or `tEvents[uGuid]`. `MrxTask` /
`MrxTutorial` formalise it as `self:_CreateEvent(…)` + `DestroyEvents(self)` — but it stays
opt-in per script. A modder who calls bare `Event.Create` (or worse, `Event.CreatePersistent`,
which survives level transitions) leaks the closure and its captured `self` forever; Cleanup
runs, but the callback keeps firing against a torn-down mission. This is the biggest hidden
leak class in the seam today.

### F8–F10 — non-mission mod classes

**F8 — Wardrobe outfit index-shift.** A saved costume is stored as a **position in
`_tOutfits[hero]`** ([`manifest_format.md`](manifest_format.md)). Uninstalling a Shipment that
added an outfit shifts every later outfit's index → the save silently re-dresses the player.
`qm link` generates the wardrobe lists and the character map in load order, saves the worn outfit's
name beside the byte, and on load puts the byte back where that outfit now is (the hero's own model
when it is gone) — "Outfits are generated by the link" in `manifest_format.md`. A save without that
name and a co-op client still restore from the byte alone.

**F9 — Registry first-wins hash collisions.** [[no-arbitrary-hashes]] +
[[name-registry-spawn-by-hash]]. Registry insert @ `0xDF6B88` is first-wins; two Shipments each
registering `tMissionData["Rescue001"]`, both `Pg.Spawn("PMC003_EwanTaxi", …)`, or both appending
an outfit named `Rescue`, silently drop the *later* one. Nothing surfaces the collision.

**F10 — stringdb residency, not placement.** [[novel-language-stringdb-residency]]. A localisation
Shipment can ship a perfect stringdb into a patch WAD and still see every string render as
`[0x…]` hashes because the container is never made resident. The seam gives the modder no signal
that the file is present but not consulted — the shell probe is `0x4643b7`, not the intuitively
related `0x4b8901` (a red-herring terrainmesh-pool fast-path).

Adjacent, worth calling out explicitly: **`AddSupportData(tData, sKey)` silently no-ops unless
`g_bIsDlc` is true** (`mrxsupportdata.lua:2459`). Store-item Shipments must set that flag before
adding a row, or the row disappears without a message.

### F11 — Story-milestone gate leak (starter side-effect)

Mission-shaped, discovered after F1–F10 on new-game boot with `mercs2-fiona-wave-defense` v0.9.4
installed. A mission's `sStarter` (e.g. `"PmcBoss"` for Fiona) is a *briefing-room actor handle*
that is **also a world-state trigger**. Calling `UnlockMission` on a mission with
`sStarter="PmcBoss"` before the player has earned PMC ownership fires this seven-hop cascade:

1. `UnlockMission` runs `_AddBriefingToStarter()` at
   [`mrxmissionflow.lua:302`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxmissionflow.lua#L302).
2. → `MrxStarterManager.RequestStarter("PmcBoss"):AddBriefing("<mission_id>", ...)`.
3. → `Starter:AddBriefing` at
   [`mrxstarter.lua:153-154`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxstarter.lua#L153-L154):
   **`if not self:IsActivated() then self:Activate()`**. On a new game the PmcBoss starter has
   `_bActive = nil`, so Activate runs.
4. → `Starter:Activate` at
   [`mrxstarter.lua:303-317`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxstarter.lua#L303-L317)
   sets `_bActive=true` and calls `RefreshBriefingRoomDisplay()`.
5. → `RefreshBriefingRoomDisplay` at
   [`mrxstarter.lua:535-546`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxstarter.lua#L535-L546):
   PmcBoss carries `bPmcStarter=true`
   ([`wifstarterdata.lua:687`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifstarterdata.lua#L687)),
   so this calls `WifPmcInterior.RefreshUiDisplay()`.
6. → `RefreshUiDisplay` at
   [`wifpmcinterior.lua:816-874`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifpmcinterior.lua#L816-L874):
   `bDisplay = true` (not gated by `_bUnlocked` — only by the unset `_bEntranceLock`), and calls
   **`_EnablePortals(bDisplay, false)`** at line 874.
7. → `_EnablePortals(true, false)` at
   [`wifpmcinterior.lua:993-1011`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifpmcinterior.lua#L993-L1011)
   arms every `_sExterior_Entrance` portal with `_OnEnter` as callback. **Player walks into the
   HQ door and is teleported inside — before `WifPmcInterior.Unlock()` at
   [`wifmissionflow.lua:144`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifmissionflow.lua#L144)
   has ever fired**, and therefore before `_bUnlocked=true`, before `_OnPmcHibernation(building,
   true)` initialised any building, before the parking-lot / transit / garage / wager wiring —
   the player is inside a half-instantiated HQ.

Vanilla never hits this because every shipped PMC-faction contract with `sStarter="PmcBoss"`
(PmcCon002/003/031/032/033/034 …) is `UnlockMission`'d only from the `PmcCon001` flow-rule's
fConseq at [`wifmissionflow.lua:142-146`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifmissionflow.lua#L142-L146),
in the same block as `WifPmcInterior.Unlock()`. `PmcCon001` itself has **no `sStarter`**
([`wifmissiondata.lua:783-791`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifmissiondata.lua#L783-L791)),
so unlocking it doesn't touch PmcBoss. The ordering is implicit in the shipped scripts, not
documented; a modder authoring a Shipment with `sStarter="PmcBoss"` has no way to know without
the decomp. Every faction's starter is subject to the same class: `AllChiBoss`/`GurBoss`/etc.
each guard their own faction-HQ portal state via the same `RefreshBriefingRoomDisplay` →
`Hq:RefreshUiDisplay` fan-out ([`mrxstarter.lua:539-541`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxstarter.lua#L539-L541)).

Confirmed live: v0.9.4 armed the portal on new-game boot; the register append gated `UnlockMission`
on `HasKey("PmcCon001")` in v0.9.5.

### F12 — Briefing-dialog format crash from unparseable mission name

Mission-shaped, discovered 2026-10-01 against retail PC with
`mercs2-mission-ab-test` v0.2.0 installed. A custom row in
`WifMissionData.tMissionData` whose key does **not** match the shipped
`<Faction><Con|Job><NN>` naming shape (`PmcCon031`, `OilJob004`, …) wedges the
briefing-dialog teardown the instant the player selects it from a starter's
"Select an option" root menu.

The cascade from select-click to black-screen:

1. Starter dialog's root menu stores the clicked briefing as
   `_sSelectedMission` in `mrxbriefing` and begins the selection → confirmation
   → `_End` flow.
2. `_End` at
   [`mrxbriefing.lua:1263`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxbriefing.lua#L1263)
   calls `_UnloadSpiel(true)` early in its cleanup.
3. `_UnloadSpiel` at
   [`mrxbriefing.lua:1568-1572`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxbriefing.lua#L1568-L1572)
   calls `GetSpielFileName(_sSelectedMission)` to resolve the briefing
   cinematic's filename for unload.
4. `GetSpielFileName` at
   [`mrxbriefing.lua:2835-2850`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxbriefing.lua#L2835-L2850)
   calls `MrxUtil.ExplodeMissionName(sMissionName)` to split the id into
   `(sFaction, bContract, nNumber)`. For a non-matching name (e.g.
   `AbTest_E5_JetPmcBossJob`) it returns `nNumber=nil`.
5. [`mrxbriefing.lua:2849`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxbriefing.lua#L2849):
   **`string.format("%02d", nNumber=nil)` throws
   `bad argument #2 to 'format' (number expected, got nil)`**. The error
   propagates out of the dialog-selection handler before `_End` reaches
   `_Fade(false, _EndBegin)` at line 1286.
6. The briefing dialog's visual tears down (menu closes) but `_ClientMenuBox`
   stays `true`, `_sCurrentStarterId` stays set, `_bInside` stays `true`, and
   the briefing camera stays locked. Player sees "black screen then camera
   restored but no menu, no interaction" and the engine is stranded in the
   briefing's intermediate camera state with no path to re-enter the game loop.
   `WifPmcInterior.Exit` is never called; `MrxState.STATE_WAITFORGAME` may or
   may not be held — a desynced UI is the real problem, not a locked state.

Vanilla never hits this because every shipped mission id matches the parseable
shape (verified across all 69 `tMissionData` rows).

**Live recovery via Lua REPL.** The engine already contains the escape hatch:
`_UnloadSpiel` has an early return on `_sSelectedMission == nil` at
[`mrxbriefing.lua:1569`](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxbriefing.lua#L1569).
Clearing the selection first lets `_End`'s normal cascade run end-to-end:

```lua
local mb = _MODULES['mrxbriefing']
mb._sSelectedMission = nil
mb._End()
-- fade → _EndBegin → _oStarter:End({}, nil) → BriefingComplete(false)
-- → MrxState.Exit(STATE_WAITFORGAME).  Player regains control, stays in PMC.
```

Confirmed live 2026-10-01: the recovery restored interaction cleanly;
`_ClientMenuBox=nil`, `_sCurrentStarterId=nil`, `WFG nRefCount=0` all cleared
by the engine's own teardown.

**Fix for mod authors**: either (a) name custom test/dev missions with a
parseable shape (`AbCon05`, `AbJob01`, …) so `ExplodeMissionName` returns a
real `nNumber` and `GetSpielFileName` builds a well-formed name whose asset
simply doesn't resolve — the shipped missing-asset path is well-tested; or
(b) `patch_lua` into `mrxbriefing` and wrap `GetSpielFileName` to recognise
the mod's naming prefix with a safe fallback. Option (a) is lower-risk.

**Fix for `qm`**: a lint check on `WifMissionData.tMissionData` row keys
contributed via `patch_lua` could reject names that `ExplodeMissionName` can't
parse — this failure mode is detectable statically.

## Per-tool responsibility split

The four tools we have are a natural stack: `qm` at compile time, Modkit at
install/uninstall time, pmc_bb/SDK at boot and every subsequent frame. The design lets each own
what it is best placed to see.

### `qm` — compile-time correct-by-construction

`qm lint`/`build` is the only tool that runs on the *modder's* machine, before anything ships.
Every static, machine-checkable rule belongs here.

**`inherit` gate** ([[custom-mission-inherit-mrxtask-required]] as the canonical failure). A
Shipment whose `contributions` add a `tMissionData` row via `patch_lua mrxmissionflow` must also
contribute an `add_script` whose module's top-level calls `inherit("MrxTaskContract")` or a
subclass (`MrxTaskMission`, `MrxTaskJob`, `MrxTaskContractOutpost`). Missing inherit is
`error M0xxx`. This one check would have caught the FioDef001 support-drop bug at `qm lint`, days
before it shipped.

**Event-lifetime gate** (F7). Any `Event.Create` / `Event.CreatePersistent` call inside a
Shipment's Lua that is not routed through `self:_CreateEvent(…)` / `self:_CreatePersistentEvent(…)`
is `error M0xxx`. Modders never lose to leaked timer closures. Symmetric with the shipped
`MrxTask` / `MrxTutorial` idiom, generalised.

**Global-shadowing gate** (F7 companion). A top-level assignment in a Shipment module that
shadows a base-game module symbol (`tEvents`, `_tEvents`, `_MODULES`, `tMissionData`, etc.)
is `error M0xxx`. Prevents the resident-script pattern (`tEvents = tEvents or {}` shared across
`import()`ers) from being clobbered by a mod-local table. A module that writes a global on purpose —
a library's public table, an engine callback it chains — declares it in its `add_script`'s `exports`
list, and M0302 accepts a write to a declared name; anything undeclared stays an error.

**Cross-mod name-collision gate** (F9). `qm link` refuses to compose two Shipments whose
declared mission-ids, outfit-names, spawn-template-names, or `patch_lua` append targets collide.
Either the modder renames, or `qm build` transparently rewrites into a `<shipment_id>_` prefix
namespace at build time — the modder never sees the prefix in source.

**Save-schema declaration.** The manifest gains a `save_schema` block on mission contributions:

```yaml
contributions:
  - kind: add_script
    name: FioDef001
    source: src/fiodef001_module.lua
    mission:
      id: FioDef001
      save_schema:
        version: 1
        max_bytes: 512     # steward budget; overflow = quarantine
        fields:
          nWave: int
          nKills: int
          tCheckpoint: { x: float, y: float, z: float }
```

`qm build` emits a stub `SaveInstance`/`LoadAssets` pair over these fields, plus a schema-version
byte embedded in the emitted `_tSaveData`. The modder writes/reads plain fields; the envelope is
generated. A subsequent `qm build` that changes a field without bumping `version` is
`error M0xxx` — version drift becomes a compile-time failure on the modder's machine, not a
silent corruption on the player's. `max_bytes` is enforced against a canned-serialize measurement
so the 13,404-byte cap is not violated at runtime.

**Mutation manifest.** Missions declare what world state they touch:

```yaml
    mission:
      owns_layers: [Vz_State_FioDef001]
      mutates_factions: [Vzla]
      spawns_persistently: false     # true only if the mission adds objects meant to outlive it
```

`qm build` uses this to emit the companion cleanup hooks (F6) and to feed Modkit's provider
registry.

**Story-gate declaration** (F11). Missions declare the story milestone they unlock behind. Data-
driven, not hardcoded — the 1% of missions authored to be available from game start (a debug
range, a tutorial, an intro contract) opt out explicitly rather than being blocked by a rule:

```yaml
    mission:
      requires:
        mission_complete: PmcCon001   # or a list: [PmcCon001, OilCon001]
        # mission_complete: null      # explicit opt-out; mission available from game start
```

Defaultable from `sStarter`: `qm build` infers a sensible default whenever `requires:` is absent,
using the vanilla starter→gate mapping (PmcBoss/HelPmcBoss/MecPmcBoss/JetPmcBoss →
`mission_complete: PmcCon001`, AllChiBoss → `mission_complete: AllCon001`, and so on — the same
milestones every shipped faction contract's flow-rule implicitly gates on). Explicit `requires:`
overrides. Explicit `mission_complete: null` opts out. Missing declaration + unknown `sStarter`
is `error M0xxx` — the modder gets asked which gate applies, they don't get a silent default that
doesn't fit.

At build time, `qm build` emits a `HasKey(<mission>)` guard around every Shipment-authored
`UnlockMission` call site for that mission. Modders never need to write the guard themselves; the
generated code is symmetrical with the retry loop the framework already provides via
`_RefreshComplete`. Runtime side (below) enforces the same declaration as a backstop, so a mod
that predates the manifest field or hand-writes its own `UnlockMission` in a place `qm` cannot
statically see still gets gated.

**Capability declaration.** The manifest gains a `capabilities:` block listing the binding
namespaces the Shipment intends to touch (`spawns`, `layers`, `factions`, `ui`, `audio`,
`missions`, `save`, `wifflow`, `sys`, …). `qm lint` grep-checks the Shipment's Lua against the
declaration and errors on any use of a namespace not declared. The runtime side enforces the same
list (below). Sandboxing without a `require`-style capability model is theatre; making the
namespaces the manifest surface is the smallest workable version.

**Evidence-grade discipline.** Any field cited in `save_schema.fields` (or referenced by name in
a `mutation_manifest`) that resolves to a decomp doc marked `evidence: speculative` is
`error M0xxx`. Grades already exist in every `docs/reverse_engineer/*.md`; `qm` makes them
operational. A Shipment can only cite things we have proven.

**Companion `save_purge_hook`.** For every mission contribution, `qm build` auto-emits a small
`patch_lua mrxmissionflow` fragment that wraps `LoadSingleton` and elides `tActiveMissions[myId]`
if `WifMissionData.tMissionData[myId]` is nil at load time. Symmetric drop on the write side. The
modder never sees this; it just ships alongside every mission. Solves F1 for the case where the
mod is installed on the *loading* machine — the runtime backstop below covers when it is not.

**`qm test` — hermetic run harness.** `mercs2_script` (the Rust Lua host in the reimpl) + the
1058 backed bindings already are a mock engine. `qm test` runs a Shipment's Lua under it, feeds
a canned event trace (captured by `lua_trace.asi` on a real playthrough), and asserts: (1)
`Cleanup` reverses every mutation the mission made, (2) `SaveInstance → LoadAssets` roundtrips
the declared schema, (3) no bare `Event.Create` fired, (4) no touched-capability outside the
manifest declaration. Fast, hermetic, CI-friendly. Nothing about it depends on the retail game
being installed.

### Modkit — the installed-provider registry

Modkit already tracks what Shipments are installed. This becomes ground truth for the runtime
enforcement.

**`installed_shipments.json`.** Written at install/uninstall, read at every game boot. Maps
`mission_id → shipment_id`, `layer_owner → shipment_id`, `faction_mutator → shipment_id`,
`outfit_name → shipment_id`, `spawn_template → shipment_id`, `stringdb_lang → shipment_id`.
Formats already do the equivalent for their own bookkeeping; extending the schema to cover
every provider kind above is a schema change, not a new subsystem.

**Global name reservation** (F9 companion). Modkit refuses to install a Shipment that would
collide with an already-installed one on any of the reserved keys. Uninstall releases the key.
This is the runtime pair of `qm link`'s compile-time gate.

**Removal warning.** When Modkit uninstalls a Shipment that owns an active save entry, warn the
user before proceeding; offer a one-click "purge from all saves" that runs the companion cleanup
hooks pre-uninstall against every save in `SaveGames\*.profile`. Same UX Steam Workshop
normalised 15 years ago; Mercs2 got shipped without it.

**Cross-machine save transfer.** Save file arrives on a machine where the referenced mods are not
installed. Modkit sees the mismatch at boot; SDK filters accordingly; user gets a "these mods are
not present, do you want to load anyway?" dialog. Same "opt-in load with missing mods" idiom every
mainstream mod loader has.

**Wardrobe re-anchor on uninstall** (F8). The re-anchor runs in the game, from the outfit name
`qm link`'s generated code saves with each save (`tPmcInteriorData.tQmOutfit`), mapping by name,
not by index; Modkit rewrites no profile for it.

### pmc_bb / SDK ASI — runtime engine-surface enforcement

The native side owns everything Modkit and `qm` cannot see because it depends on live game state.

**Save-blob steward.** Hook the native save writer (`saveProfile`, registration-anchored @
`0x7BC628`; write happens inside a fixup that already stamps `data_size`/`ProfileHash`) and the
load path. Before write: walk `tFlowData.tActiveMissions`, drop any entry whose provider is not
in `installed_shipments.json`; drop any entry whose declared `max_bytes` overflowed and
`[mod-quarantine]` it. Before Lua sees the loaded blob: same filter. This is F1's runtime
backstop — even if `qm`'s companion `save_purge_hook` was never installed (because the mod is
not present on the loading machine), the save is repaired before Lua touches it. The write side
is now unblocked ([[profile-hash-is-crc32-bzip2]] + `save_write::write_profile`); the steward
can rewrite the `.profile` in place with a valid CRC-32/BZIP2 hash today.

**Schema-versioned envelope.** The engine wraps every mission's `_tSaveData` in
`{__mod=shipment_id, __ver=n, __schema_hash=…, data=…}` at write time; unwraps at read time.
Version mismatch runs a migration function (`<mission>_migrate_v1_to_v2.lua` in the Shipment) if
present, quarantines the entry if not. Modder never sees the envelope; they read/write `data`
fields as plain Lua. Solves F3.

**Reference-counted world mutations.** Every layer add, faction attitude change, contract-active
flag goes through a tracked handle owned by the mission that made it. Mission cancel/complete/
quarantine walks handles and reverses them, always. Solves F6 by making the mutations tracked
system-wide instead of per-mission-Lua bookkeeping. Same pattern as `Ess.Sandbox`'s providers, but
unconditional and native, not opt-in per mission. The full provider list to cover is what Ess's
`63_sandbox.lua` already enumerates — layers, faction attitudes, contract-active, HUD elements,
music state, ambient overrides. Use it as the spec.

**Event-handle tracking backstop.** The native `Event.Create` trampoline (`0x005F69F0`) consults
a thread-local mission-owner set by MrxTask's callback dispatch. An event created without an
owner falls into a session-scoped "unowned" bucket that emits `[unowned-event] <shipment_id>` at
level transition and forces `Event.Delete`. This is F7's runtime backstop for anything the qm
gate missed (bare `Event.Create` called from a `patch_lua`-appended chunk, dynamic code paths
`qm` cannot statically see).

**Lua-error containment.** Every native call-into-Lua site — retail's `luaD_call 0x008688D0` /
`luaD_pcall 0x00868AD0` / `lua_pcall 0x0085DF50` / `luaB_pcall 0x008615F0`, plus the callback
dispatch trampolines (installed there, *not* by monkey-patching cfuncs from Lua, which crashes
at `0x0059C82A`) — gets wrapped with a mod-attribution pcall. In the reimpl, the same wrap is
[`mercs2_script::attribution::call_attributed`](../../crates/mercs2_script/src/attribution.rs)
routing every internal `Function::call` site (Event dispatch, Hud movie-end, Player callbacks,
Pg layer-load, dynamic_import, Module.Init) through a thread-local host. When the mod's
callback throws:

- Log `[mod-crash] <shipment_id>/<mission_id> in <callback>: <traceback>` to `pmc_blackbox.log`
  **and** to `Sys.WriteToConsole` — the retail Debug family is a dead stub, so the visible sink
  must be the one live cfunc.
- Quarantine that mod's callbacks for the rest of the session (or offer a soft-reload for the
  developer path).
- Cascade to that mod's mission cleanup — reverse its tracked mutations, drop its `_tActiveMissions`
  row.

Half the pain of Mercs2 modding is F5. Attribution + quarantine + visible logs fix all of it. This
is the single highest-leverage item in this whole document.

**Capability enforcement.** The native `_SYS._IMPORT` hook and the per-namespace call
trampolines consult the loaded Shipment's declared `capabilities:` list; a call into an
undeclared namespace raises `[mod-cap-violation] <shipment_id> tried <ns>.<fn>` and skips the
call. Belt-and-suspenders with `qm lint`'s compile-time check.

**`Mercs2.Mods.evict(shipment_id)` verb.** Live uninstall of a running mod:
- Marks all Event handles owned by that Shipment for immediate teardown.
- Walks `_tActiveMissions` for that Shipment and calls `oMission:Cancel() → :Cleanup()`.
- Drops the row and its `tMissionData` entry.
- Cascades to reference-counted-mutation reversal.

Without this, Modkit's "uninstall while running" UX either kicks the player to menu (unfriendly)
or leaks a live mission (broken).

**Story-gate enforcement** (F11 runtime backstop). The SDK hooks `mrxmissionflow.UnlockMission`.
When called for a mission whose Shipment manifest declares `mission.requires.mission_complete`,
the SDK checks the requirement (`HasKey(...)` for each entry) and refuses the unlock if any
required key is missing. Refusal logs `[mod-gate-defer] <shipment_id>/<mission_id> waiting for
<mission_complete>` to `Sys.WriteToConsole` (not `Debug.Printf` — dead stub, see F5) and posts
`SupersededByGate` into the flow-refresh callback chain so the Shipment's install fires
automatically the instant the gate flips (no per-mod retry loop required). A mod whose Lua does
its own `UnlockMission` before the SDK sees it (a pre-manifest-field mod, or a modder who forgot)
still gets gated — the wrap catches the call at the module surface, not at the call site.
`Mercs2.Mods.status(id)` (below) surfaces the deferred reason so Modkit's dashboard can show
"waiting on: PmcCon001 complete" instead of the mod appearing broken.

**Live introspection surface.** Expose `Mercs2.Mods.list()`, `Mercs2.Mods.status(id)`,
`Mercs2.Mods.crashLog(id)`, `Mercs2.Mods.evict(id)` to Lua, and mirror the same as a JSON
endpoint for Modkit's dashboard. Both the game's own script layer and out-of-process tools (this
project's live-bridge REPL, a future Modkit dashboard) get the same view of what's loaded,
active, quarantined, or deferred behind a story gate.

## Community accelerators

Two adjacent surfaces already exist as internal RE tools; promoting them to first-class
modder-facing surfaces is the biggest force multiplier we have.

**Observability toolkit for mod authors.** [`lua_trace.asi`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/mods/lua_trace_asi) captures
the ordered `(binding, args)` stream for the reimpl's Surface B oracle; `pmc_bb` already hooks
`Debug.Printf`/`print` at native VAs and emits `[lua]`/`[world]` lines; `mercs2_bridge` runs a
live TCP REPL into the running game via Wally's lua-bridge ASI. None of these are surfaced to
modders today. Shipping them as first-class debugger:
- `lua_trace --filter-mod=<shipment_id>` writes only lines whose call originated in that mod's
  chunks (attribution comes from F5 above).
- `pmc_bb` emits `[mod-crash] …` (F5), `[unowned-event] …` (F7), `[mod-cap-violation] …`
  (capability enforcement), `[mod-quarantine] …` (steward) to `pmc_blackbox.log` **and**
  `Sys.WriteToConsole`.
- `mercs2_bridge` exposes `Mercs2.Mods.list()` / `crashLog(id)` / `evalInMod(id, chunk)` for a
  modder's editor to poke.

**Docs from the binding table.** [`lua_engine_bindings_audit_deep_dive.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/lua_engine_bindings_audit_deep_dive.md)
shows 714 of 1081 bindings have decompiled call sites across 370 scripts.
[`mercs2_reassemble`](../../crates/mercs2_reassemble/) left-joins every
attribution source onto the 27k-fn decomp. Nothing publishes any of this to modders — there is
no `Pg.Spawn` page a modder can Ctrl-F. A generator (Rust binary, no new RE):
- Emits EmmyLua / lua-language-server type stubs for all 1086 bindings, argument marshalling
  derived from each cfunc's `FUN_005*` arg-parser call pattern.
- Renders a searchable docs site (`mdBook` or similar) from `mercs2_reassemble`'s annotations,
  the audit's call-site frequency, and each doc's grade-tagged citations.

The template Shipment repo ships a `.luarc.json` that pulls the stubs in. This is the fastest
thing in the whole plan to build and the biggest single community accelerator on it.

## MrxTutorial, Ess.Contract, and Ess.Sandbox in this world

The seam hardening above targets the heavyweight offering-and-persistence path (MrxTaskContract
+ starter briefing + `_tActiveMissions` + `SaveInstance`). But it is not the only shipped mission
pattern, and modders who reflex-reach for it are frequently choosing a heavier tool than their
mission actually needs. Two lighter, F1–F11-immune-by-construction options already exist:

**`MrxTutorial` (shipped-native, F1-immune)** — the lightweight mission class Pandemic themselves
built for wiftutorial*.lua and shooting-gallery-style mode logic
([mrxtutorial.lua](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxtutorial.lua)).
No `MrxTaskState`, no `SaveInstance`, no `_tSaveData`, no starter interaction, no
`_tActiveMissions` registration. `MrxTutorialManager.SaveSingleton` at
[line 219](../../crates/mercs2_script/corpus/mercs2-luacd/src/resident/mrxtutorialmanager.lua#L219) saves
*only* a bounded list of completed-tutorial names — a shape that cannot participate in any of
F1/F3/F4/F11 by construction. The engine's own second mission-lifecycle class, always available,
zero dependency cost to modders.

**`Ess.Contract`** (`80_contract.lua` (`../../../mercs2-lua-essentials/src/80_contract.lua`))
does the same for authors who want a richer offering UI (multi-page menu, custom accept/decline
dialog) than MrxTutorial provides. Built without the decomp, it explicitly refuses to touch the
native contract system (file header: "the native contract system corrupts saves because it
registers into `WifMissionData`, serializes MrxTask nodes INTO the save, and drives missions
through dynamic_import + mrxbriefing + the MrxState load gate. This framework touches NONE of
that"). Same "no save-write participation" property as MrxTutorial, layered on a custom UI stack.

**When to choose which** — a modder authoring a mission has three viable lifecycle options, plus
five offering options, plus three persistence options, and the shipped-vs-Ess choice is
independent of that. The combinability matrix and per-recipe guidance is at
[`docs/reverse_engineer/mission_construction_patterns.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/mission_construction_patterns.md).
Concrete rule of thumb: **prefer the lightest lifecycle that meets the mission's real needs.**
A wave-defense arena is Recipe C (MrxTutorial-shape + region trigger + tiny persistent-key), not
Recipe A (MrxTaskContract + starter briefing + full save row) — and it doesn't matter that the
seam is hardened; the lighter recipe is still less surface. FioDef001 v0.10.0's rewrite from
Recipe A to Recipe C dropped the mod's poisoning surface to zero and cut its overlay WAD from
15.2 MB to 2.6 MB in the process — the heavier path was pulling in the whole scripts_vz block
for no gain.

Once the hardened seam exists, the native MrxTaskContract path is *also* safe. Authors then pick
based on **the mission's semantics** (does it need save-resume mid-mission? does it need a starter-
table briefing UI? does it own persistent world mutations?), not based on which path happens to
avoid corruption today.

**`Ess.Sandbox`** (`63_sandbox.lua` (`../../../mercs2-lua-essentials/src/63_sandbox.lua`)) is the
reference spec for the reference-counted-mutation system in the SDK design. Wally already
enumerated the providers that need reversal: layer add/remove, faction attitude, contract-active
flag, HUD-element registration, music-state transitions, ambient overrides. The engine-side
implementation should cover **every** provider Ess covers, not just layers/factions/contract-
active. Ess-in-Lua opts *in* per mission; the hardened seam does it *unconditionally* at the
C-binding boundary.

## Migration path

The individual pieces of this design each stand alone. Order suggested by expected leverage;
ProfileHash is not a step (already resolved, [[profile-hash-is-crc32-bzip2]]).

1. **`qm` inherit gate + Event-lifetime gate + global-shadowing gate.** Three `M0xxx` rules,
   trivially implementable, would have caught the current FioDef001 support-drop bug at compile
   time. Zero runtime dependencies. First shipment.
2. **Binding-derived docs site + EmmyLua stubs.** Zero engine work. The single biggest community
   accelerator on the list; unblocks new contributors before any of the deeper work lands.
3. **Attribution pcall + `[mod-crash]` + `lua_trace --filter-mod`.** F5 close; the
   single most player/modder-visible improvement. No manifest changes required to benefit
   existing mods.
4. **Modkit provider registry + name-collision gate + wardrobe re-anchor.** F1 (uninstall
   poisoning) + F8 (index-shift) + F9 (collisions) close here. Requires the manifest to gain
   `mission.id`/`owns_layers`/`outfit_name`, but old manifests without those fields still work
   (they just are not covered — same as today).
5. **Save-blob steward + schema-versioned envelope.** F3 (schema drift) closes here. Now
   unblocked by [[profile-hash-is-crc32-bzip2]] + `save_write::write_profile` — the steward can
   rewrite `.profile` in place with a valid hash today. Requires modders who want save-resume to
   declare a `save_schema` with `max_bytes`; not required for missions that opt out
   (Ess.Contract-style or `save_schema: none`).
6. **Reference-counted world mutations + Event-handle tracking + `Mercs2.Mods.evict`.** F6 + F7
   close; largest structural change; do after the smaller pieces have stabilised.
7. **Capability sandbox + `qm test` + evidence-grade discipline.** Hardening on top of a working
   system. Requires the earlier layers to be in place before it becomes actionable.

Each step is separately shippable; each strictly reduces the failure surface for existing and
future mods. None require modders to rewrite anything to benefit — a Shipment whose manifest
predates a new feature just does not get that feature, without breaking.

## Related

- [`docs/reverse_engineer/mission_construction_patterns.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/mission_construction_patterns.md)
  — the peer doc on the Lua-side authoring patterns (offering × lifecycle × persistence,
  combinability matrix, per-recipe guidance including Recipe C for wave-defense/challenge-arena
  modes). If this doc says "here's why the seam bites", that doc says "here's which patterns
  don't bite in the first place."
- [`docs/reverse_engineer/mission_inventory.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/mission_inventory.md) —
  every shipped mission classified by lifecycle class, starter, and unlock trigger. Useful for
  cross-referencing "does the seam-hardening design cover the way vanilla mission `X` works?"
  and for picking a shipped mission to model a new mod on (Recipe A → PmcCon031-shape,
  Recipe B → PmcJob001-shape, etc.).
- [`docs/reverse_engineer/mission_contract_flow_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/mission_contract_flow_code_map.md)
  — the native-side map (Pg cfuncs, `Sys.RequestGameState`, layer streaming, context actions)
  that both docs above sit on top of.
- [[custom-mission-inherit-mrxtask-required]] — the failure that motivated this document.
- [[custom-mission-add-script-loading-screen-wedge]] — the lifecycle-hole class this belongs to.
- [[profile-hash-is-crc32-bzip2]] — closes the save-blob-steward RE prerequisite.
- [[novel-language-stringdb-residency]] — F10's proven probe.
- [[no-arbitrary-hashes]] / [[name-registry-spawn-by-hash]] — F9's underlying trap.
- `mercs2-lua-essentials/src/80_contract.lua` — Wally's from-Lua answer to the same seam problems.
- `mercs2-lua-essentials/src/63_sandbox.lua` — the reference provider list for §Reference-counted
  world mutations.
- `mercs2-lua-essentials/src/24_save.lua` — the shared save-gate pattern (`Ess.Save.gate`/
  `.ungate`); Recipe E's `save_schema: none` opt-out is a native-side generalisation of this.
- `mercs2-lua-essentials/FEATURE_SHEET.md` — Design Principle 2 ("make a footgun impossible").
- [`docs/modding/manifest_format.md`](manifest_format.md) — the current manifest surface these additions land on.
- [`docs/modding/field_guide.md`](field_guide.md) — the collected traps that will be lint-preventable once shipped.
- [`docs/reverse_engineer/diagnostics_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/diagnostics_code_map.md) — the retail-Debug-stub finding under F5.
- [`docs/lua_engine_bindings_audit_deep_dive.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/lua_engine_bindings_audit_deep_dive.md) — the binding surface that powers §Community
  accelerators.
