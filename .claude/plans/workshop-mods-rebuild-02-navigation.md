# Workshop "Mods" rebuild — Plan 02: navigation, domains, tiers

**Status:** DESIGN (rev 2, 2026-07-25). Multi-session.
**Siblings:** `-01-mod-model.md`, `-03-live-bridge.md`, `-04-manifest-format.md`
**Scope:** how the Workshop is NAVIGATED once `mercs2_quartermaster` (Plan 01) exists. This is the UX spine.
A mod = a **Shipment** (see Plan 01 for the naming).

## The decision (settled with the user)

Navigate by **themed, invented feature-domains** (Unreal-style), NOT by a tool list, and NOT by my
earlier "object-first" framing. The user explicitly wants us to name our own areas and group related
tools under a theme — and to bring a Lua script editor into the Workshop. My "object-first" and their
"behavior-theme" converge once **the domain IS the object context**; the disagreement was labels.

### Three persistent surfaces + the domain spine

There are TWO orthogonal ways to slice the game, and the Workshop needs both:
- **By TYPE** (Models, Textures, Animations, VFX, Audio, …) → this is **Inspect**, the content
  browser / preview tool. Read-mostly, spans everything, the entry point. See its own section below.
- **By SUBJECT/BEHAVIOR** (Driving, Characters, …) → the **domains**, the authoring lenses.

So the top level is:
- **Inspect** (persistent) — the **Library**: browse-by-type + preview any asset. NOT an authoring
  workbench — it's where you find and examine things, then *act on* one (→ routes into a domain +
  starts/adds a Shipment contribution in the Quartermaster).
- **Domains** (the spine you navigate — themed by what part of the GAME you're changing):
  `World · Characters · Weapons · Driving · Audio · Missions · Systems/Engine`
- **Craft** (persistent, domain-agnostic tools that follow the current subject):
  mesh · texture · rig/retarget · **Lua editor** · state-machine graph.
- **Quartermaster** (persistent) — the Shipment accumulator: queue · Problems · Build · Publish (see below).

This mirrors Unreal exactly: a Content Browser (Inspect) + themed modes/areas (domains) + a persistent
details surface (craft/Quartermaster). It resolves the ONE hard constraint of pure domain-nav:

**Craft cuts across domains.** Texture-swap, Lua editing, retarget, the linter, build/publish each
apply to World AND Weapons AND Driving. If nav were purely by domain, those tools would appear five
times or fracture. So craft tools live in the always-present surface, not inside any one domain.

### Why domains are strong here (not just theming)

Each domain is backed by REAL reversed knowledge we already own — a domain is a curated lens, not a
marketing bucket:
- **Driving** = vehicle/road-AI code map + vehicle Lua + destruction state machine
  (`FUN_004cf340`, fully decoded) + vehicle roster + handling params + engine audio.
- **Characters** = the shared Pandemic human rig + wardrobe (`_tOutfits`) + animation selection chain
  (ActionTable→AnimationLookup→ASTO) + retarget.
- **Systems/Engine** = the home for Plan 01's Code-layer mods (native hooks, new systems, rewrites).
  Clean symmetry: the first six domains are content+behavior; the last is code.

So opening "Driving" co-locates everything needed to make ONE coherent change to how a vehicle works:
the vehicle, its skins (texture craft), handling, hardpoints/seats, its destruction states as a graph
editor (same egui-snarl tech as the planned Lua blueprint editor), its engine audio, and the governing
Lua. A tool-rail can never do that. This is where RE knowledge becomes UX — the SAME bet as the linter.

### The honest cost

Domain lenses are **bespoke curation** — each is hand-built and ongoing. That's real, but it's the
RIGHT cost (knowledge → interface), consistent with the linter being the differentiator. Build them
one at a time, deepest-knowledge-first (Driving or Characters); the rest start as thin browsers that
thicken over time.

## Tier calibration (settled)

- **Modkit = Tier 1.** Players. Never sees "ASET."
- **Workshop = Tier 2/3.** Modders. Tier 3 = FORMAT-level (ASET rows, host groups, chunk inventory,
  state hashes) behind per-panel **"Advanced" reveals** — NOT assembly. Raw disassembly/Ghidra stays a
  separate surface (x32dbg MCP, etc.). The Workshop's floor is "a modder willing to learn what a
  container is," not "a reverse engineer."
- Every Tier-1 recipe must have a visible **"show me what this does"** that drops the user a tier —
  that's how a modder graduates without a wiki.

## Inspect = the Library (content browser + preview) — user-set 2026-07-24

**Inspect CONTINUES to exist**, but reframed: it is the **content browser / preview tool**, not an
authoring workbench. Browse-by-TYPE across the whole install, preview faithfully in the engine, inspect
the details — then *act on* an asset (context menu → "edit in <domain>" / "start a Shipment from this"),
which routes into the relevant domain and adds a contribution to the Quartermaster. Read-mostly; the truthful
engine preview is its whole reason to exist (Plan 01 fork: Workshop = the accurate renderer).

**Expand the browsable types beyond Models/Textures.** The retail ASET type-discriminator table
(`docs/aset_format.md`, proven) is the real taxonomy to organize Inspect around:

| Inspect category | ASET type | retail count | preview treatment | status |
|---|---|---|---|---|
| **Models** | mesh `0x5B724250` | 3,007 | full pipeline render (exists) | ✅ done |
| **Textures** | streamed | 13,340 | plate view, streamed hi-mip (exists) | ✅ done |
| **Animations** | clips via `animationtable` `0x207359C7` (15 lookup tables) + wavelet clip data | 4,261+ clips | play clip on a rig (character-specific catalog exists; timeline exists) | ✅ mostly — promote from per-model to a first-class catalog |
| **Visual Effects** | `effect` `0x5608BD5A` (all in `effects` block) + `fxdict` `0xFA46D8A8` (resident singleton) | 314 | spawn in the particle sim + play | ⚠ PARTIAL — format decoded (`fxdict_parser`/`effect_block_probe`); param names still hash-only. Browsable by hash/name now; faithful preview needs more decode |
| **Audio** | `wavebank 0xF753F6D0` / `soundbank 0x9F8BCA10` / `sounddb 0xE5273C14` | 95 / 76 / 77 | playback (audio system wired) | ◻ candidate — VO extraction proven |
| **UI / GFX** | `scaleformgfx 0xFE0E8320` | 60 | movie preview | ◻ candidate — Wally's GfxForge territory |
| **FaceFX** | `facefxanimationset 0x665EF13E` / `facefxactor 0x1CF649BB` | 86 / 31 | facial anim on a head | ◻ later |
| minor | `font 0x99E77ACE` (9), `stringdb 0x39E5E978` (3) | — | plate / text list | ◻ later |

Launch order for Inspect categories: **Animations** and **Visual Effects** are the user's named next
two (Animations is nearly there; VFX is browsable now, faithful preview gated on finishing the effect
param decode). Audio / UI-GFX / FaceFX follow.

## The Quartermaster — the Shipment accumulator (naming re-settled 2026-07-25)

Every domain's edits (a skin in Characters, a handling tweak in Driving, a script in Missions) all
accumulate into the SAME current Shipment. So the surface that holds them is NOT a peer tab you navigate
away to — it's an **always-present panel** docked alongside every domain, named **Quartermaster** (the role
that compiles/manages/ships shipments). It is the **UI front-end to the `mercs2_quartermaster` crate** (Plan 01)
— same concept, two layers. It shows: the current Shipment's contribution list, the **Problems** panel
(linter, Plan 01), Build, and Publish.

Rejected panel names: **"Contracts"** — double collision (Wally's `Ess.Contract` = an in-game mission,
AND our own **Missions** domain authors real in-game contracts). Also a semantic mismatch: a Contract
is the job you take; a Shipment is the package you produce; the Quartermaster is where you assemble it.
**"Handler"** (used through 2026-07-24) went with its package noun — see the Plan 01 naming section for
why *Dossier* collided with the PDA's own bio database.

## Settings — where the game install is configured (new 2026-07-25)

A new persistent surface, small but load-bearing: **the Workshop owns game-path resolution and hands
paths to `mercs2_quartermaster`** (the crate never discovers anything itself — Plan 01, Plan 04).
Nothing like this exists today: the Workshop has **no settings persistence at all**, and
`Workbench` is exactly four tabs.

Resolution order, first hit wins:
1. **Explicit setting** — the configurable game folder. Community members have asked for this.
2. **Co-location** — `Mercenaries2.exe` next to the Workshop binary, then `./data/vz.wad`. Modders
   really do drop the Workshop into the game folder, and this is the only path that works off Windows.
3. **Registry** — `registry_vz_wad()` (`mercs2_engine/src/wad.rs:33`), the current behavior.
4. Nothing ⇒ a clear error that points here. Linting still works with no game configured.

⚠ `registry_vz_wad` is `#[cfg(windows)]`; the non-Windows arm returns `None` (wad.rs:47). On
macOS/Linux there is no discovery at all, so 1 and 2 are the only paths — including on the
maintainer's own machine.

**Show the resolved stack, don't just resolve it.** Which install was actually read is behind a large
share of our own trap reports; the panel should name the base WAD and every overlay, in order.

## What dissolves from the current UI

The `Workbench::{Inspect, Sandbox, Mods, Skeleton}` rail (a TOOL list) is replaced by the domain spine
+ craft surface. Specifically:
- **"Mods" as a page disappears.** Conform + hardpoints move into the relevant domain's craft surface
  (mostly Driving/Characters). The mod QUEUE + Publish move into the always-present **Quartermaster** panel
  (backed by `mercs2_quartermaster`). Delete the vehicle-only donor navigator — any host, not just vehicles.
- **Inspect STAYS** as a top-level peer — but reframed as the Library (browse + preview, see above),
  no longer a per-model authoring workbench.
- **Sandbox / Skeleton** dissolve into craft surfaces / domain views, not top-level peers.
- The engine viewport + camera stay persistent across everything (already true — the activity rail
  only ever swapped navigator+inspector, per memory `mercs2-workshop-devtool`).

## Open questions for the user
1. **The domain list + the Systems/Engine slot** — is "code-layer mods get their own domain" the right
   cut? What are the LAUNCH domains vs. STUB domains?
2. **Where the craft surface lives visually** relative to the domain spine — an always-docked panel, or
   a mode you drop into from a subject?
3. Naming: "World/Weapons/Driving/Audio" as literal labels, or a different themed set? (User was still
   :thinking: on this.)

## Grounding pointers
- memory: `mercs2-workshop-devtool` (the full workbench history + the UI-overhaul passes — reuse the
  `gui.rs::theme` system: brass=live, hazard=irreversible, the `card`/`section`/`kv` helpers).
- Code maps per domain already exist: `vehicle-road-ai-pc-code-maps`, `ai-code-map`,
  `faction-reputation-code-map`, `world-streaming-pc-code-map`, `rows-26-29-weapons-save-code-maps`,
  `render-core-lighting-pc-code-maps`, `keystone-bcd-pc-code-maps` — these POPULATE the domains.
- `human-animation-selection-system`, `pandemic-shared-human-rig-mercs2-saboteur` — Characters domain.
- The state-machine graph editor shares tech with the planned Lua blueprint editor
  (`docs/modernization/workshop_charter.md` §Mission designer: full_moon → idiom lifter → egui-snarl).
