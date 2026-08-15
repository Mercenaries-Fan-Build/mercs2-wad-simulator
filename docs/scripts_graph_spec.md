# Missions page — visual-scripting node-vocabulary spec

Design spec for the **Missions** domain's graph view: a Blueprints-style visual representation
of the Mercenaries 2 mission Lua (contracts, jobs, objectives), rendered with **egui_snarl**.

> **Scope:** Missions and Systems remain **two separate pages** — the earlier plan to merge them
> into one "Scripts" domain was reverted. This spec covers the Missions surface (the content graph,
> Tiers A + B below). The Systems surface (the `mrx*` framework/library layer and its lifecycle
> hooks) is a distinct representation, out of scope here.

Derived from a full pass over `workshop_data/lua/{resident,shell,vz}` (370 files) + `lua_dlc`
(39). This is the contract the extractor and the snarl node palette are built against; the
raw per-axis analyses (progression / objective palette / event triggers) back every claim here.

> **Status:** design only — no extractor code written yet. No rail changes landed (the
> Missions+Systems merge was reverted; both pages stay as-is).
>
> **Fact-checked 2026-08-12** against the reingested corpus by two independent adversarial
> reviewers (Tier A + Tier B); verdict **Solid** — every load-bearing claim confirmed at exact
> ids / spellings / counts. Their corrections (edge polarity & guards, non-mission consequence
> verbs, coded arg shapes, rare triggers, extra out-ports) are folded in below.

---

## 1. The two tiers, one IR

The corpus has a **finite, regular node vocabulary**, but it lives at two levels that need
different treatment. Both compile down to one shared graph IR so a single snarl renderer draws
both.

| | **Tier A — Progression** | **Tier B — Mission flow** |
|---|---|---|
| Subject | the whole campaign | one mission/job |
| Nodes | missions, starters, HQs, keys/flags | objectives, event triggers, VO, root |
| Edges | unlock / award / destroy | *contains*, *then*, *on-fail* |
| Source | `wifmissiondata` (attrs) + `wifmissionflow` closures (edges) | per-mission `CreateChild`/`_CreateEvent`/`MrxVoSequence` calls |
| Extraction | VM: read `tMissionData`; run flow closures under a recording oracle | VM: run the framework under a recording `EngineHost`; branch-explore via oracles |
| Risk | low (execution captures edges directly) | medium (branch coverage; world-state-gated paths) |

**Fidelity rule (agreed):** objective + event + VO nodes are lifted as graph nodes; imperative glue
(AI spawns, gate loops, faction tweaks) is **captured as a node annotation** — under the VM harness
(§5) that annotation is the *recorded call-list* the glue actually made, not a static guess — but it
is never exploded into per-statement sub-nodes. That is what keeps the graph readable and
Blueprint-like.

---

## 2. Shared IR (Rust sketch)

```rust
// ---- Tier A ---------------------------------------------------------------
struct ProgressionGraph {
    missions: Vec<MissionNode>,   // from tMissionData
    starters: Vec<StarterNode>,   // from WifStarterData
    hqs:      Vec<HqNode>,        // from WifHqData
    keys:     Vec<KeyNode>,       // mission-ids + synthetic story flags
    edges:    Vec<FlowEdge>,      // from wifmissionflow closures
}
struct MissionNode {
    id: String,                   // == sModuleName; case-insensitive match key
    faction: Faction,             // sFactionId — NOT the id prefix (see §6)
    is_contract: bool,            // derived: id matches /Con/ vs /Job/
    critical_path: bool,          // bCriticalPathMission
    repeatable: Option<u32>,      // nLevels when bRepeatable
    milestones: Vec<Milestone>,   // semantics differ job vs contract (§6)
    layers: Vec<String>,          // tLayers (own scene, NOT world-unlock layers)
    title_key: Option<String>,    // sTitle override
    starter: Option<String>,      // sStarter
    // ... sPdaTexture, nPdaSortOrder, flags
}
enum FlowEdgeKind { Unlock, Award, Destroy }   // Destroy = negative edge
struct KeyRef { key: String, negated: bool, min_count: Option<u32> } // `not HasKey` / `GetKeyValue>=n`
struct FlowEdge {
    prereqs: Vec<KeyRef>,         // conjunction of HasKey / `not HasKey` / GetKeyValue>=n atoms
    guard:   Vec<KeyRef>,         // enclosing `if` around a guarded consequence (§5 step 2, §6.11)
    target:  String,              // UnlockMission / AwardKey / DestroyMission arg
    kind:    FlowEdgeKind,
    world_effects: Vec<String>,   // layer/transit/standing/starter/intro effects from fConseq body
}

// ---- Tier B ---------------------------------------------------------------
struct MissionGraph {
    root: RootNode,               // Contract | ContractOutpost | Job* | Mission
    nodes: Vec<TaskNode>,
    wires: Vec<Wire>,
}
struct TaskNode {
    name: String,                 // sName (unique among siblings)
    kind: NodeKind,               // objective type / event trigger / vo / race / job-child
    pins: Vec<Pin>,               // typed inputs (see §4)
    annotations: Vec<String>,     // summarized side-effects of opaque glue
}
enum PortKind { Contains, OnActivate, OnComplete, OnCancel, OnPart, Timeout }
enum WireTarget { Node(String), ParentPort(PortKind), ScriptLogic(String) } // ScriptLogic = opaque
struct Wire { from: String, port: PortKind, to: WireTarget }
```

Two IRs, one node/edge rendering model: `snarl` sees `{node, in-pins, out-ports, wires}`
regardless of tier.

---

## 3. Node palette

**Blueprint mapping.** Two pin kinds, Unreal-style. *Exec pins* (control flow): an event node has an
exec-out; an objective/action node has an exec-in and one exec-out **per outcome** — `OnComplete`,
`OnCancel`, `OnActivate`, `OnPart*` (the `t/fOn*` wires). *Data pins* (typed): the config keys
(targets, destination, VO, quota, description) are data-in pins, fed by *pure* value nodes
(`GetGuidByName`, `GetAnyCharacter`). Meta-nodes (`Race`, `Job*`) are collapsed sub-graphs;
imperative glue rides as a node annotation (a recorded call-list, §5).

### 3a. Tier B — root nodes (the graph entry, one per mission file)

Every mission `.lua` `inherit(...)`s exactly one root and *is* that root node.

| Root | Distinguishing config | Graph behavior |
|---|---|---|
| `MrxTaskContract` (40) | `sFactionId`, `tRewards{nCash,nWager,…}`, `sStarter` | single-shot; win/lose fanfare; multi-stage Complete defers past interior loads |
| `MrxTaskContractOutpost` (14) | `tOutpostConfig{sOutpostBldg,sCapturePt,sRivalFaction,…layers}` | auto-spawns a `CaptureOutpost` child + Outpost manager |
| `MrxTaskContractPlaceholder` | — | stub: "not implemented" cinematic → Complete |
| `MrxTaskJob*` (CollectType/DestroySet/DestroyType/VerifySet) | setter API — type-based use `_SetLabelFilter`, set-based use `_AddTarget`, then `_Go`; `tMilestones` | repeatable bounty; **forces `bOptional=true` on all children** |
| `MrxTaskMission` | `sFactionId`, `oStarter` | abstract PDA/VO container |

Lifecycle hooks a mission overrides (call order): `PreLoadAssets` → `LoadAssets` (loads
`tLayers`) → `Activated` (spawns objectives) → `Complete`/`Cancel` → `Cleanup`.

### 3b. Tier B — objective nodes (`sModuleName`, ~11 types)

**Common pins on every objective** (inherited from `MrxTask`+`MrxTaskObjective`):
- identity: `sName`, `sModuleName`, `sDspShortDesc` (localized label)
- targeting: `vTgtInclude`/`vTgtExclude` (name|guid|array|player-sentinel), `sTgtLabelFilter`, `nQuota`
- display: `bDspBlp*` (radar/pda/world), `bDspMsg*`, `bOptional`, icons, fade distances
- VO: `vVoSeqOnAdd`
- **out-ports (wires):** `t/fOnAssetsLoaded` (between load & Activated), `t/fOnActivate`, `t/fOnComplete`, `t/fOnCancel`, `t/fOnPart{Complete,Cancel}`, `t/fOnInitialNotesComplete` (objective-only, after add-message VO)
- implicit: `nTimeLimit`/`tTimerParams` → a **timeout→Cancel** edge

| Node | Type-specific pins | Advances via |
|---|---|---|
| `Deliver` (50) | `vDestLoc`/`vDestRegion`, `fDist`, `bStop`, `bXZOnly`, `bHumansFollow`, follow-VO tables | part per delivered target |
| `Destroy` (59) | `bHeroOnly` (+ base targeting/quota) | `ObjectDeath`/`"ClientKill"` → part |
| `Protect` (3) | `bHeroOnly` | death → **CancelPart** (no self-complete; completed externally) |
| `Extract` (2) | `fDist`, follower reuse, follow-VO; hard-codes `Extraction_AL` + heli filter | target seated in heli |
| `EnterVehicle` (12) | `uPlayer` (any/all), `bUseAnySeat`, status-change cb | `ObjectInSeat` enter |
| `Verify` (6) | `sFactionId` (picks `Extraction_*`), `fOnTarget{Destroyed,Subdued,Actioned}` | `CompletePart(guid,bKilled)` |
| `Action` (9) | `sActionLabel` | `ContextAction` |
| `Release` (3) | `sActionLabel`; reads **parent's** `tMaterielScale` | nearby context action |
| `Accept` (—) | `sDialogText` (Yes/No) | action + "Yes" |
| `CaptureOutpost` (1) | `uOutpostBldg` (usually spawned by ContractOutpost) | manager captured/destroyed |
| `Race` (9) | `tCourseLocs`, `fWidth`, `sGateType`, timer; **meta-node** | spawns internal Deliver chain |

### 3c. Tier B — event trigger nodes (`Event.<Type>`, ≈two dozen real types)

Registered via `Event.Create` / `Event.CreatePersistent` / `self:_CreateEvent` (task-scoped
auto-cleanup). The `{argPins}` table is the node's input pins; the callback is the out-wire.
The `Event` enum is **engine-side C++ — not declared in Lua**, so shapes are empirical.

| Category | Type | Arg-tuple (pins) |
|---|---|---|
| time | `TimerRelative` (580) | `{nSeconds [, bRepeat]}` |
| spatial | `ObjectProximity` (137) | `{uChar, uTarget, "<"\|">", nDist [, b, b]}` |
| spatial | `Boundary` (104) | `{uChar, uRegion, "enter"\|"exit" [, bPersist]}` |
| lifecycle | `ObjectHibernation` (160) | `{uGuid, "awake"\|"hibernated"}` |
| lifecycle | `ObjectDeath` (102) | `{uGuid}` |
| lifecycle | `ObjectInSeat` (84) | `{uChar\|0, uVehicle, sSeat("d"\|"a"), sAction}` — sAction is a **code** `"e"\|"ei"\|"x"\|"xo"` (+caps), not "enter"/"exit" |
| lifecycle | `ObjectIsReady` (11) / `ObjectDelete` (4) / `ObjectIsVisible` (1) | `{uGuid}` |
| lifecycle | `ObjectWinched` (6) | overloaded `{uObj, nIdx\|uWincher, sMode}` |
| combat | `ObjectHealth` (17) | `{uGuid, "<"\|">", nHealth}` |
| combat | `ObjectHealthLessThan` (4) | `{uGuid, nHealth}` |
| combat | `WeaponEvent` (10) | `{sClass, sAction, uGuid}` |
| state | `HumanStateTransition` (~10) | variadic `{uChar, sFrom, sTo [, sQualifier]}`; sFrom/sTo are dotted `"Group.Phase"` strings, `"*"` wildcard, qualifier e.g. `"complete"` |
| state | `HumanActionComplete` (9) | `{uChar}` |
| state | `ObjectPhysicsEvent` (9) | `{uGuid, sPhysTag}` (free-text) |
| state | `AnimationEvent` (2) | `{uGuid\|0, sAnimTag}` |
| input | `ContextAction` (15) | `{uChar, uGuid}` |
| input | `Button` (12) | `{uPlayerChar, sButton, "press"\|"release"\|"hold", bConsume}` |
| input | `Minigame` (6) | `{uPlayerChar, nTimeOut, "mash"\|"hold", uButton}` |
| scripting | `ScriptEvent` (71) | `{sChannel, fFilter(tData)->bool}` — pairs with `Event.Post` |
| scripting | `GameStateChange` (16) | `{sState, "Enter"\|"Exit"}` |

**Rare triggers** (corpus-wide, 1–2 uses each, mostly framework/minigame residents — a
mission-scoped extractor may never see them, a corpus-wide one must not choke): `Timer{n}`,
`Player{uPlayer,sLabel,sAction}`, `ObjectIsGrounded{uGuid,b}`,
`HumanAnimationNearlyCompleted{uGuid,fThresh}`, `GuiGameTimer{}`, `GuiUpdate{}`.

### 3d. Value/source nodes (pin producers)

GUID/char pins are almost never literals. Palette needs small source nodes:
`Pg.GetGuidByName("name")`, `Player.GetAnyCharacter()`/`GetPrimaryCharacter()`/`GetLocalCharacter()`,
and the wildcard `0` sentinel ("any object/char").

### 3e. Tier A — progression nodes

`Mission` (contract/job subtype), `Starter` (contact NPC), `HQ`/`Outpost` (location + landing
zone), `Key`/`Flag` (state token — the shared namespace all edges thread through). Edges:
`hasStarter`, `locatedAt`, `awards`, `unlocks`, `removes`.

---

## 4. Wire semantics

- **Contains** (structural): `CreateChild` sets `oParent` + `_tChildren[sName]`. **This does not
  sequence** — children activate immediately; containment only scopes lifetime (parent
  cancel/complete cascades down).
- **Then / On-fail** (sequencing): the real ordering. `t/fOnComplete` and `t/fOnCancel` fire on
  state flip. Two authoring forms:
  - list `tOnComplete = {{NamedFn, {args}}, …}` → resolves to a named target
  - closure `fOnComplete = function() … end` → resolves **only** `self:Complete()`/`self:Cancel()`
    /`self:CreateChild(`; anything else is an opaque `ScriptLogic` out-port + summary
- **Bubble-up:** a closure that calls `self:Complete()`/`Cancel()` wires to the **parent's**
  corresponding out-port.
- **Per-part** (`fOnPart*`): fires once per target — render as a counter/self-loop, not a
  sequencing edge.
- **Timeout:** `nTimeLimit`/`tTimerParams` present ⇒ implicit timeout→Cancel edge.

---

## 5. Extraction strategy — the recording VM harness

Extraction runs the shipped Lua on the game's own VM (`mercs2_script`, Lua 5.1.5) under a
**recording `EngineHost`**, not a static parser. Running the code is what reaches true Blueprint
fidelity — an execution graph (exec pins = control flow) is exactly what a Blueprint *is* — and it
dissolves the opaque-closure ceiling. Every engine binding becomes a pure recorder / controllable
oracle (nothing mutates real state): `Pg.GetGuidByName(n)` → a synthetic handle carrying the *name*
(so data pins read as authored names); leaf verbs (`MrxLayerManager.*`, `Ai.Goal`, `Object.*`,
`MrxVoSequence.Start`, …) → appended to the current node's annotation call-list; `Event.Create` → a
trigger node. The **task framework runs for real** — `mrxtask`/`mrxtaskobjective*`/`mrxmissionflow`
are ordinary Lua; we stub only the engine leaves under them and introspect the tree they build
(`self._tChildren`, `self:GetConfig()`). To draw **every** exec wire (not one run's path), fire each
outcome callback and enumerate flag/key-gated branches by making `_GetFlag`/`HasKey` return each
value, re-running and **unioning** the result. The extractor is a new crate (`mercs2_missiongraph`)
depending on `mercs2_script`; the Missions page consumes its IR and renders with snarl.

1. **Tier A nodes:** read `tMissionData` (attrs) directly off the VM (`import("WifMissionData")`). Enumerate starters (module-global tables
   indexed by name — `AllStarter0 = {...}`, walked via `_sStarters` — **not** a single
   `WifStarterData` table) and HQs (`_tHqConfigs`, via `WifHqData.GetHqConfigFromId`). Derive
   `is_contract` from chars 4-6 of the id (`Con`→true / `Job`→false). Emit `hasStarter` from
   `sStarter` and `locatedAt` from the starter's `sHqName` — these are **attr edges, not flow
   edges**. Add synthetic `Key` nodes for any flow token with no mission entry (`Invasion`,
   `MonsterV4`, `MecIntro`/`JetIntro`/`GurIntro`/`PirIntro`/`AllChiIntro`, …).
2. **Tier A edges:** run each `GetOriginalFlowData` binding's `fPrereq`/`fConseq` under the recording
   oracle, which captures:
   - **prereqs** = every `HasKey`/`GetKeyValue>=n` atom in `fPrereq`, each with **polarity** —
     `not HasKey(x)` is a *negative* precondition (13 sites, e.g. the `PmcCon002_*` hint chain);
     flattening it to a positive prereq inverts the gate.
   - **consequences** = `UnlockMission`/`AwardKey`/`DestroyMission` args in `fConseq`. Emit
     `unlocks`/`awards`/`removes` edges through the key namespace. A consequence may sit inside an
     `if` (9 guarded unlocks, e.g. `OilCon052`→`OilCon005` only `if not HasKey("ChiCon002")`) —
     capture the enclosing condition as the edge `guard`, or the graph shows unlocks the game
     suppresses.
   - `fConseq` is **not always an inline closure**: handle bare named-fn refs
     (`fConseq = _AddHeroCostume`), a *missing* `fConseq` (`PmcJob001`), and verbs nested in
     `local function`s (`Start`/`PmcCon001`) — follow the reference.
   - **Non-mission consequence verbs are campaign state, not glue:** scrape
     `DestroyStarter`/`RequestStarter`, `SetAttitudeMutable`, `AddUnlockedItem` (bounty/outfit),
     `_AddIntro`/`_RemoveIntro` (rotates the contact-NPC set), and `_AddHeroCostume` (the *only*
     payoff of the `PmcCon03x_x3` `GetKeyValue>=3` bindings — invisible to an Unlock/Award scan),
     alongside `MrxLayerManager`/`MrxTransit` into `world_effects`.
   - **Completion auto-awards the mission's own key** (`AwardKey(sMissionName)` in `mrxmissionflow`),
     so `HasKey("PmcCon001")` is satisfied by *completing* it — thread mission-completion into the
     key namespace instead of hunting for an explicit `AwardKey` call.
3. **Tier B nodes:** run the mission's `Activated()` on the VM; the framework builds the objective
   tree for real. Each `CreateChild` (objective), `_CreateEvent` (trigger) and `MrxVoSequence.Start`
   (VO) the recorder sees becomes a node; pins come from `self:GetConfig()` per §3b/§3c.
4. **Tier B wires:** fire each node's `t/fOn*` outcomes under the oracle (§4); the `CreateChild`/
   `Complete`/`Cancel` calls they make are the wires, and the leaf verbs they make are the node's
   annotation call-list. A branch no oracle setting reaches falls back to the static AST (below).
5. **Meta-nodes:** `Race` and `Job*` emit children at runtime, not as source siblings — render as
   collapsible sub-graphs; for Jobs, trace the setter API (not `CreateChild` args) and handle
   **both entry paths**: type-based jobs configure via `_SetLabelFilter`, set-based via `_AddTarget`,
   then `_Go`.

---

6. **Static fallback (`full_moon`):** a lightweight AST pass covers only what execution can't reach —
   dead code, branches no oracle setting triggers. A backstop, not the primary path; log what only
   static saw so nothing is silently dropped.

## 6. Load-bearing gotchas

1. **Progression edges are closures, not data.** `tMissionData` has zero edges; all live in
   `wifmissionflow.lua` `fPrereq`/`fConseq` bodies. No declarative edge table exists.
2. **`ButtonPress`/`ButtonReleased`/`PosX`/`PosZ`/`PrimaryClipSize`/`PrimaryCurrentAmmo` are NOT
   triggers.** They're payload fields on the callback's `tEvent`/`Event` table. The real input
   trigger is `Event.Button`. Do not add them to the palette.
3. **Faction ≠ id prefix.** `MecCon001`/`JetCon001`/`OilCon020` are all `sFactionId="Pmc"`. Always
   read the field. Mec/Jet are PMC-side recruiters, not their own factions here.
4. **`bContract` is runtime-injected** (`Con`/`Job` regex on the id) — derive it the same way.
5. **`tMilestones` is overloaded:** jobs → cumulative *threshold* (all keys granted at once);
   contracts → *run number* (only the matching key granted).
6. **Spelling-bug keys:** getter reads `bCompleteable` (extra "e"); `AllJob003`/`ChiJob003`/
   `OilJob004` authored `bCompletable` → dead. Keep both keys distinct; don't normalize.
7. **Two layer namespaces:** mission-own `tMissionData.tLayers` ≠ world-unlock `vz_state_*` swapped
   inside flow closures. Don't conflate.
8. **`DestroyMission` is a real negative edge** (`OilCon005` un-offered by `ChiCon002`).
9. **Opaque pins:** `vTgtInclude` is polymorphic (name/guid/array/sentinel); state/tag strings
   (`ObjectHibernation`, `ObjectPhysicsEvent`, `WeaponEvent`) are engine vocabularies — make them
   free-text-with-suggestions, not fixed dropdowns. Trailing bools on `Boundary`/`ObjectProximity`
   are low-confidence; mark optional.
10. **Cross-node pin reads:** `ObjectiveRelease` reads its *parent's* `tMaterielScale`.
11. **Prereqs have polarity; consequences can be guarded.** `not HasKey(x)` (13 sites) is a real
    negative precondition, and 9 `UnlockMission`s sit inside `if`-guards. Both are *progression*
    edges — not summarizable glue — so model them with `negated` + `guard`, never flatten.
12. **`ObjectInSeat`/`HumanStateTransition` args are coded, not worded** — seat `"d"/"a"`, action
    `"e"/"ei"/"x"/"xo"`; transition states are dotted `"Group.Phase"` strings. Free-text-with-
    suggestions pins (per #9), not `enter`/`exit` dropdowns.

---

## 7. Open questions / risks

- **Execution coverage.** With the VM harness the question flips from "can we read it?" to "did we
  drive every branch?" Some objective branches gate on world state an oracle can't cheaply fake
  (streaming, physics, AI). Measure the unreached fraction on a first pass; the static AST backstop
  covers the remainder.
- **Layout.** Tier A is a DAG (layered/Sugiyama auto-layout). Tier B is closer to a hand-authored
  flow — does snarl's free placement + our auto-layout read well, or do we need per-mission layout
  heuristics?
- **`ObjectWinched` overload** and the trailing-bool pins need a live check against the game to pin
  semantics (out of scope for static analysis).
- **Editing vs read-only.** First cut is read-only visualization. Whether the graph becomes an
  *authoring* surface (write back to `patch_lua`) is a later question, but the IR should not
  foreclose it.
