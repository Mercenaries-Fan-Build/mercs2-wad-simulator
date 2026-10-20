# Contract state in a save, and what the mission flow writes

How a `.profile`'s Lua payload records each contract, and where every effect of the mission flow
lands in it. The Workshop's Saves page edits contracts through this
(`crates/mercs2_workshop/src/save_flow.rs`, rules from `game_rules.rs`). Citations are to the
PC retail decompile in the notes repo, `docs/mercs2-luacd/src/`.

## Contract state

Two tables in `tFlowData` (`resident/mrxmissionflow.lua:597-636`):

| State | `tMyFlowData[id]` | `tActiveMissions[id]` |
|---|---|---|
| Completed | completion count | absent (repeatable: present again, latent) |
| In progress | — | `nState = 1` |
| Offered | — | `nState` ≠ 1 |
| Not unlocked | — | absent |

On load every `tActiveMissions` entry goes back through `UnlockMission`; only `nState == 1` is
read (it skips the briefing, `mrxmissionflow.lua:237`). `nState` 2 or 3 load as offered. A freshly
unlocked mission saves as `{nState = 0}` with a briefing; a job without one as
`{nState = 1, _nTargetsComplete = 0}`; a contract without one as `{nState = 1, tContractState = {}}`.

## Spawn: `tRetryLocations`

Retry locations belong to the one contract in progress. It sets them to its start locations when
it activates (`resident/mrxtaskcontract.lua:55-57`), checkpoints move them (`:419-424`), and
completing (`:144`, `mrxmissionflow.lua:161`) or cancelling (`:386`) clears them. A save that has
them loads into the world at those markers; one without loads into the PMC (`Pmc_Entry1/2`,
`vz/xQ!L.lua:656-662`). A save that keeps a completed contract's retry locations spawns the player
back at that contract's start.

## Header fields that mirror the Lua

Equal in all 8 vendored fixtures (`crates/mercs2_formats/fixtures/saves/`):

| Header | Lua |
|---|---|
| `0x18` cash | `tPmcData.nCash` |
| `0x1C` fuel | `tPmcData.nFuel` |
| `0x2C` (16 bytes) | `sLastMissionName`, set on every completion (`mrxmissionflow.lua:211`) |
| `0x24A` | `nAvailableCostumes` |

`tPmcInteriorData.tStockpileQty.money` is the PMC cash pile, raised to the highest balance seen
(`vz/wifpmcinterior.lua:1555-1563`) and not read back on load (saved as `tStockpileQty`, loaded as
`tSupportQty`, `:1626,1636`). "Total earned / spent" are `tStatsData.nTotalCredit` /
`nTotalDebit`, by reason in `tListOfCredits` / `tListOfDebits`.

## Where each flow-binding call lands

| Call | Save path |
|---|---|
| `UnlockMission(m)` / `DestroyMission(m)` | `tFlowData.tActiveMissions[m]` added / removed |
| `AwardKey(k)` | `tFlowData.tMyFlowData[k] += 1`, plus `MrxRewardData` row `k` (below) |
| binding fired | name appended to `tFlowData.tCulledBindings` |
| `WifHints.AddActiveHint` / `RemoveActiveHint` / `UnlockAllHints` | `tActiveHint[speaker][]` |
| `WifBios.AddDossierEntry(b)` | `tActiveBio[b] = true` |
| `MrxLayerManager.MarkForAddition` / `MarkForRemoval` | `tLayerData[]`, lowercased |
| `MrxStarterManager.RequestStarter` / `DestroyStarter` | `tStarterData[s]` added / removed |
| starter `AddIntro(i)` / `RemoveIntro(i)` | `tStarterData[s].tIntros[i] = false` / removed |
| `MrxFactionManager.SetAttitudeMutable(f)` | `tFactionData.tMutableFactions[]`, and `tRelations[f].Pmc` = the faction's starting relation |
| `MrxVerifyManager.SetKilledIfNotSet(t)` | `tStatsData.oVerifyData.tTargetListStatus[faction][t] = "captured"` if unverified |
| `MrxVerifyManager.SetSolanoVerified()` | `oVerifyData.sSolanoStatus` and `tTargetListStatus.Pmc.Solano` |
| `WifVzBoundary.SetupBoundary*` | `tBoundaryData.sBoundaryName` (new game: `BoundaryIntro`) |
| `WifVzBoundary.EnableExclusionBoundary(n, b)` | `tBoundaryData.tExclusionBoundaries[n] = b` |
| `WifPmcInterior.Unlock()` | `tPmcInteriorData.bUnlocked = true` |
| `WifPmcInterior.SetAvailableCostumes(n)` | `nAvailableCostumes` (+ header `0x24A`) |
| `friendlygate.LockGate(g, b)` | `tLockedGates["0x%08X"]`, the hex GUID in the gate's entity name |
| `Munitions.SetMunitionsTaggable(b)` | `tMunitionsData.bMunitionsTaggable` |
| `MrxTransit.SetSystemEnabled` / `SetLocationEnabled` / `SetLocationIsNuked` | `tTransitData.bEnabled`, `tTransitData[n]` |
| `SetGrappleEnabled` / `SetVehicleDisguiseEnabled` / `EnableResourceCounters` | `bGrappleEnabled` / `bVehicleDisguiseEnabled` / `bResourceCountersEnabled` |
| `MrxSupportData.Set*Recruited` (Start only) | `tSupportData._tRequirementsObtained` |

Movies, VO, radio, achievements, HUD, teleports and HQ display are not saved.

A reward row (`resident/mrxrewarddata.lua:1531-1602`) pays: `nCash` into `0x18`/`tPmcData.nCash`
(clamped 0..1e9) and `tStatsData.nTotalCredit` / `tListOfCredits["[Generic.Contracts]"]`; `nFuel`
into `0x1C`/`tPmcData.nFuel` (clamped to `0x2F8`) and `tStatsData.nFuelIn`; `tAttitude` into
`tFactionData.tRelations[f].Pmc` for mutable factions; `tSupport` into
`tSupportData[id].tUnlockStatus[faction] = 1`; `tEquipment` into
`tWifEquipmentData[id][faction] = 1`; `tStockpile` into `tPmcData.tStockpile[id].nAmt`. A contract's
in-mission bonus (`Complete2`'s `nCashOverride`) is not in the table.

## The flow bindings

`WifMissionFlow.GetOriginalFlowData()` returns 67 bindings keyed by name
(`vz/wifmissionflow.lua:34-1405`), each `{fPrereq, fConseq}`. `Refresh()`
(`mrxmissionflow.lua:512-561`) is event-driven: world start right after the save loads
(`vz/xQ!L.lua:877`), every completion (`mrxmissionflow.lua:189`), a job target
(`mrxtaskjob.lua:132`), an intro viewed at a starter (`mrxstarter.lua:234`). A binding whose prereq
holds fires and is culled; no retail binding is `bRecurring`. `AwardKey` calls during a pass are
deferred to its end, and the pass repeats while anything fired. Every prereq reads only `HasKey` /
`GetKeyValue` (`Start` is `return true`), so which bindings fire is a function of `tMyFlowData`;
the consequences branch on other keys and on skip mode.

Each line: a binding's prerequisite keys → the missions its consequence unlocks. `*Intro` keys are
awarded when the player views the intro a binding added at a starter (`mrxstarter.lua:229-231`).

```
Start                         → VzaCon001
VzaCon001                     → PmcCon001
PmcCon001                     → OilCon020, PmcCon031, PmcJob001
OilCon020                     → OilCon021
OilCon021                     → OilCon002
OilCon002                     → OilCon050, OilJob004, OilJob008, OilJob011, PmcCon013, PmcCon034
                                (adds the Gur intro at PmcBoss, the Mec intro at HelPmcBoss)
GurIntro                      → GurCon053
MecIntro                      → MecCon001 (also awards key MonsterV4)
MecCon001                     → PmcCon016 (adds the Jet intro at MecPmcBoss)
PmcCon016                     → PmcCon015
JetIntro                      → JetCon001
JetCon001                     → PmcCon018
OilCon050                     → OilCon001, OilCon051
OilCon051                     → OilCon003, OilCon052
OilCon052                     → OilCon005 (only without key ChiCon002)
OilCon001 + GurCon001         → PmcCon002
GurCon053                     → GurCon002, GurCon050, GurJob001, GurJob002, GurJob006, GurJob020
GurCon002                     → GurCon003 (adds the Pir intro at PmcBoss)
GurCon003                     → GurCon001
GurCon050                     → GurCon005, GurCon052
PirIntro                      → PirCon001
PirCon001                     → PirCon002, PirCon051, PirJob012, PirJob020
PirCon051                     → PirCon003, PirCon052
PirCon052                     → PirCon004
PmcCon002 + JetCon001         → PmcCon003
PmcCon003                     → (awards key Invasion; adds the AllChi intro at PmcBoss)
AllChiIntro                   → AllCon050, ChiCon050
AllCon050                     → AllCon002, AllCon052, AllJob002, AllJob003, AllJob020
AllCon002                     → AllCon001
AllCon001                     → AllCon003
AllCon052                     → AllCon008 (only without key PmcCon004), AllCon053
ChiCon050                     → ChiCon001, ChiCon051, ChiJob002, ChiJob003, ChiJob020
ChiCon001                     → ChiCon002
ChiCon002                     → ChiCon003 (destroys OilCon005)
ChiCon051                     → ChiCon008 (only without key PmcCon004), ChiCon053
ChiCon053                     → ChiCon009
AllCon003  /  ChiCon003       → PmcCon004 (either one)
PmcCon031                     → PmcCon032
PmcCon032                     → PmcCon033
PmcCon031..034 completed ×3   → one more outfit each
PmcCon004                     → destroys 11 open contracts, rolls the credits
```

The notes repo's `docs/vanilla_mission_lifecycle_analysis.md` §3.4 draws this graph from demo
bytecode and disagrees with it in eight places (PmcCon002's unlocker, PmcCon004's, where
OilCon050 and the intros come from, and the missing PmcCon03x, outpost and job chains).
