---
name: cross_platform_lua_porting_guide
description: "Modder-facing guide for shipping Lua mods across the three 'next-gen' Mercenaries 2 platforms (PC / PS3 / Xbox 360). What's portable, what's not, what to compile per platform, what reserved slots are safe to use, and the build pipeline for each target."
status: current
evidence: proven
verified_on: 2026-10-02
---

# Mercenaries 2 — cross-platform Lua modding guide

**For:** mod authors building a mod that should run on more than one of PC Retail,
Xbox 360, and PS3. The guide is grounded in direct byte-level comparison of the
shipped Lua on both PC and Xbox (378 base-game pairs + 36 DLC pairs), plus
engine-reference signature evidence for PS3. Every claim here is tool-produced,
not inferred from authority. See **Reference** at the end for the raw reports.

> **★ Complete-corpus coverage**
>
> This guide is grounded in every shipped base-game Lua chunk on three platforms:
> **679 PC + 676 PS3 + 672 Xbox NTSC-US**. PC retail is measured from the complete install
> at `~/Documents/Mercenaries 2 World in Flames/data/` (English/French/German/Italian/
> Spanish WADs); PS3 is measured from the BLUS30056 BLU-RAY ISO (adds Russian); Xbox is
> measured from the NTSC-US JTAGRip.
>
> Full breakdowns: [`_lua_corpus_coverage_audit.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/_lua_corpus_coverage_audit.md),
> [`mercs2-luacd-xbox/_structural_diff_report.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-luacd-xbox/_structural_diff_report.md),
> [`mercs2-luacd-xbox/_corpus_completion_manifest.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-luacd-xbox/_corpus_completion_manifest.md),
> [`_ps3_base_game_lua_diff.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/_ps3_base_game_lua_diff.md),
> [`_ps3_full_wad_set_lua_diff.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/_ps3_full_wad_set_lua_diff.md),
> [`_pc_xbox_resident_divergence_characterization.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/_pc_xbox_resident_divergence_characterization.md).

## The three-sentence summary

1. **For the Lua content itself, PS3 ≡ Xbox** (DLC proven, base-game strongly supported). The platform split that actually matters is **PC vs console**, not Xbox vs PS3.
2. **~68% of all base-game scripts are "same source, write once"** — recompile per endian and you're done. **~19%** compile cleanly after `Debug.Printf`-stripping. **~13% (89 of 671 scripts)** have real logical differences between PC and console and need **per-platform builds**.
3. **The 89 real-divergence scripts are overwhelmingly in the GUI/menu layer** (`mrxgui*`), especially pause / shell / dialog / numeric-input / loading-tip code — plus one single-constant PC placeholder bug Xbox got fixed (satellite overlay). Mission scripts, support/airstrike framework, population, AI, interactive-object scripts, mission dialogue, subtitles, and all data-only tables are overwhelmingly in the portable majority.

---

## 1. The compile matrix — one bytecode output or two?

| Target | Lua 5.1 header | int / size_t / instr | lua_Number | Endian | Debug info |
|---|---|---|---|---|---|
| PC Retail | 0x51 | 4 / 4 / 4 | 4 (**float**) | **little** (byte 6 = `01`) | **kept** |
| Xbox 360 Retail | 0x51 | 4 / 4 / 4 | 4 (**float**) | **big** (byte 6 = `00`) | **stripped** |
| PS3 Retail (DLC) | 0x51 | 4 / 4 / 4 | 4 (**float**) | **big** (byte 6 = `00`) | **stripped** |

**Only cross-platform header difference: byte 6 (endian flag).** Both PC and Xbox/PS3 use **4-byte float** for `lua_Number` — there is **no cross-platform numeric-precision divergence**. Earlier docs claiming PC was double-precision were wrong (corrected in [`docs/mercs2-luacd-xbox/_manifest.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-luacd-xbox/_manifest.md) Phase B).

**What this means for the compiler**: a mod's `.luac` output needs **exactly two forms** to cover all three platforms: one LE (PC) and one BE (both consoles). PS3 and Xbox share a bytecode — the retail PS3 DLC and Xbox DLC Lua chunks are byte-identical in 13/36 cases and structurally identical in all 36. The *build output* on both consoles came from the same compile.

Reference tool for byte-exact Lua 5.1.5 output matching retail: [`tools/wad_simulator/crates/mercs2_luac`](../../crates/mercs2_luac) (ships `float` `lua_Number`, 32-bit `size_t`, emits LuaQ chunks the retail game accepts).

---

## 2. Classification — which scripts are portable?

The full base-game diff ran `lua_structural_dump` across all 671 common PC↔Xbox pairs across both corpus phases:

**Phase 1 — three big host blocks** (vz 114, resident 238, shell 26 = 378 pairs):

| Category | Count | % |
|---|---:|---:|
| IDENTICAL-MODULO-DUMP-FLAGS | 192 | 50.8% |
| DEBUG-SOURCE-STRIPPED | 98 | 25.9% |
| REAL-DIVERGENCE | 88 | 23.3% |

**Phase 2 — gap-closure corpus** (missions 222, hijacks 29, subtitles 36, guilayouts 4, loading 1, english 1 = 293 pairs):

| Category | Count | % |
|---|---:|---:|
| IDENTICAL-MODULO-DUMP-FLAGS | 265 | 90.4% |
| DEBUG-SOURCE-STRIPPED | 27 | 9.2% |
| REAL-DIVERGENCE | 1 | 0.3% |

**Combined (671 common pairs):**

| Category | Count | % | Mod-author implication |
|---|---:|---:|---|
| **IDENTICAL-MODULO-DUMP-FLAGS** | **457** | **68.1%** | Author Lua once; compile LE for PC, BE for consoles. Done. |
| **DEBUG-SOURCE-STRIPPED** | **125** | **18.6%** | Behaviour identical. Xbox was compiled with `Debug.Printf`/`ASSERT` preprocessor-removed. If your mod doesn't rely on Debug being present at runtime (and on PC retail the `Debug.Printf` cfunc is a return-0 stub anyway), the single-source approach is still fine. |
| **REAL-DIVERGENCE** | **89** | **13.3%** | Per-platform builds required. See §4 for which scripts and what differs. |
| PC-only pairs | 4 | — | `mrxguiltiprecache` + `mrxguiltiprecachelayout` in both resident+shell. See §4.2. |
| PS3-only pairs | 1 | — | `russian.luac` — Russian language stub, not shipped on PC or Xbox NTSC-US. See §4.6. |
| Xbox-NTSC-US-only pairs | 0 | — | — |
| PARSE-FAIL | 0 | 0% | Tool proven on all 1342 chunks. |

**Phase 2 insight**: data-only tables (mission dialogue "spiel" chunks, subtitles, loader stubs) are the portability extreme — **mission spiel and subtitles classified 100% IDENTICAL**, no debug-strip even, since they contain only literal data structures with no `Debug.Printf` calls to strip. Vehicle hijack files are the opposite — **93% debug-strip** because they're `Debug.Printf`-dense. Everything that isn't GUI or hijack-logic is highly portable.

**Full per-pair classification** at [`docs/mercs2-luacd-xbox/_structural_diff_report.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-luacd-xbox/_structural_diff_report.md).

---

## 3. By subsystem — portability at a glance

Grouped from the full report. "Portable" = IDENTICAL or DEBUG-STRIPPED. "Diverges" = REAL-DIVERGENCE.

| Subsystem (approximate script set) | Portable | Diverges | Notes |
|---|---:|---:|---|
| **Mission framework** (`mrxtask*`, `mrxmission*`, `mrxbriefing`, `mrxcontract*`, `mrxcoop`) | ~24 | 1 (`mrxbriefing` — missing `LTILibName`) | Mostly debug-strip. |
| **Support / airstrike framework** (`mrxsupport*`, `mrxairstrike*`, `mrxbombingrun`, …) | ~30 | 0 | Essentially fully portable. |
| **AI + enemy types** (`mrxai`, `mrxfactionmanager`, `tank.lua`, `helicopter.lua`, `soldier.lua`, `antiair.lua`, …) | ~15 | 0 | Portable. |
| **Interactive world objects** (`crate.lua`, `fueltank.lua`, `telephone.lua`, `monument.lua`, …) | ~25 | 0 | Portable. |
| **Sound** (`mrxsound*`, `mrxsoundbanks`) | 4 | 0 | Portable. |
| **Player + bootstrap** (`mrxplayer`, `mrxpmc`, `mrxbootstrap`, `mrxhq*`, `mrxstarter*`) | ~13 | 2 | Mostly portable. |
| **Vehicle minigames + hijack** (`mrxactionhijack`, …) | 0 | 1 | `mrxactionhijack` has extensive Debug scaffolding + `charGuid` global PC-only. |
| **GUI / HUD** (`mrxgui*`, `mrxguihud*`, `mrxguishell`, `mrxguipda`, `mrxguidialogbox`, …) | ~15 | **~30** | **Biggest concentration of divergence.** See §4. |
| **vz tutorials** (`wiftutorial*`) | ~26 | 0 | Portable at bytecode level; stringdb texts localized separately. |
| **vz contracts** (`allcon`, `chicon`, `gurcon`, `oilcon`, `pircon`, `pmccon`, `jet/mec/vza`) | ~65 | ~4 | Most portable; a handful (incl. `vzacon001` — Xbox adds a tutorial key string PC lacks) diverge. |
| **vz data/flow** (`wifmissionflow`, `wifpmcinterior`, `wifbriefingdata`, `wifequipmentdata`, …) | ~15 | ~3 | `wifpmcinterior` + `wifbriefingdata` reference `LTILibName` on PC only. |

**Headline cluster**: GUI. Of the 89 real-divergence scripts, roughly a third are the `mrxgui*` family. If your mod doesn't touch menus/HUD, you can likely ship one PC bytecode + one console bytecode and be done. If your mod *does* touch the GUI layer, per-platform compile is the default assumption.

---

## 4. The real divergence findings — know these before you ship

### 4.1 Entire GUI functions are missing on Xbox

Xbox compiled with substantially less GUI code than PC. The pause/shell/dialog/numeric-input subsystems have whole method bodies absent:

| Script | PC protos | Xbox protos | Functions dropped on Xbox |
|---|---:|---:|---:|
| `mrxguishell.luac` (resident + shell copies) | 59 | 37 | **22** |
| `mrxguipausescreen.luac` | 49 | 20 | **29** |
| `mrxguibase.luac` | 184 | 183 | 1 |
| `mrxguipda.luac` | 83 | 81 | 2 |
| `mrxguidialogbox.luac` | 32 | 30 | 2 |
| `mrxguinumericbox.luac` | 18 | 15 | 3 |

**Root cause**: PC ships keyboard+mouse menu-interaction methods that Xbox doesn't need (there are no mouse clicks in a controller menu). The `MrxGuiBase.Joystick.*` input abstraction is shared, but PC adds a KBM layer on top. A mod that extends menu behaviour on PC may be touching PC-only methods; those modifications will not apply or will malfunction on Xbox.

**Modder actionable**: if your mod adds a `function MrxGuiPauseScreen:MouseOver(…)` or similar method, that function literally doesn't exist on Xbox and your extension to it won't apply. For console builds, either:
- Omit the KBM-only methods and ship a console-trimmed bytecode, or
- Add the methods back on console (they'll exist but be unused since the engine won't call them without a mouse).

### 4.2 The LTI subsystem — PC's entire frontend/options UI backbone — is PC-only

"LTI" is Pandemic's internal name for the **PC frontend / options / input-binding UI subsystem**. Its native source lives at `D:\Projects\Mercs2_PC\mercs2\LTI\Src\PgLtiRendererPc.cpp` — the module literally carries a `_Pc` suffix in the shipped exe's source paths. Scope is much larger than the "loading tip" framing earlier docs used.

Direct byte-level evidence (verified 2026-10-02):

| LTI area | ~70 PC-only symbols | PS3 EBOOT hits | Xbox retail string hits |
|---|---|---:|---:|
| Video options | `LTIVideoSetGamma`, `LTIVideoSetRes`, `LTIVideoSetVSync`, `LTIVideoDisableHighShaders`, … | 0 | 0 (Ghidra function-body dump = false-negative baseline) |
| KBM input binding | `LTIInputKMKeyMap`, `LTIInputKMApplyChanges`, `LTIInputKMEnter`, … | 0 | 0 |
| Joystick binding | `LTIInputJoystickMap`, `LTIInputJoystickApplyChanges`, … | 0 | 0 |
| Audio options | `LTIAudioSFXVolume`, `LTIAudioMusicVolume`, `LTIAudioDialogVolume`, … | 0 | 0 |
| Mouse control | `LTIControlsMouseInvert`, `LTIInputGeneralMouseSense`, … | 0 | 0 |
| Start screen / profile | `LTIPressStart`, `LTIgotoGame`, `LTIProfileEnter/Exit`, `LTIonlineMsgBox` | 0 | 0 |
| Precache + loading tips | `LTIPrecacheDone`, `LTIPrecacheSmokeDone`, `LTIGetPrecacheBypass` | 0 | 0 |
| Text input | 10× `LTITextInput*` | 0 | 0 |
| General settings | `LTIGameSetAutosave`, `LTIGameSetTutorial`, `LTISetGraphicDetail` | 0 | 0 |
| Localization | `LTIGetDateFormat`, `LTISetUSDateFormat` | 0 | 0 |

PS3 has 2 `LTI_*` string hits but both are Havok physics internals (`LtIntegrate`, `LtInterIsland`) — zero loading-tip or options-UI hits.

**Why**: consoles delegate video output / audio sliders / controller rebinding to the platform's native system menu (Xbox 360 blade, PS3 XMB). PC retail ships an entire module to implement what consoles get for free from the OS. The two PC-only script files `mrxguiltiprecache` + `mrxguiltiprecachelayout` are just the tip of this subsystem's Lua-visible surface.

**Modder actionable**:
- Any mod that **adds a video option, audio slider, keyboard-mouse rebinding, or custom frontend screen** is PC-only by construction. Not just because the Lua binding is missing on console — the whole native subsystem (`PgLtiRendererPc.cpp`) isn't shipped.
- A mod that modifies LTI behaviour (changing a video option default, say) would use PC-only cfuncs. No porting path to console.
- A console-side equivalent would require native engine work on the console platform, which is out of scope for Lua mods.
- The 4 PC-only script files (`mrxguiltiprecache`, `mrxguiltiprecachelayout`, each in resident + shell) are safe-to-omit on console builds — the Xbox/PS3 engine never would have loaded them.

### 4.3 `vzacon001` — the one case of Xbox adding a literal PC lacks

Xbox has `"[Tutorial.SupportMenu]\n[Tutorial.UseSupport]"` (a combined stringdb key) in `vzacon001` that PC does not. This is the only clear case so far of **Xbox-side code addition** rather than removal.

**Modder actionable**: trivial — if you're localizing or editing early-game tutorial text, that key exists only on Xbox. For PC, the two sub-keys (`Tutorial.SupportMenu`, `Tutorial.UseSupport`) are rendered separately.

### 4.4 `mrxactionhijack` — hijack minigame

The vehicle-hijack minigame has 4,134-line raw diff. On PC: extensive `Debug.Printf` scaffolding + a `charGuid` global reference. On Xbox: both removed. Behavior equivalent (debug prints don't execute visibly on either platform). But if you extend the minigame on PC and inherit from a method that references `charGuid`, the Xbox version won't have it.

### 4.5 Smaller long-tail REAL deltas

8× `Object`-related, 5× `LTILibName`, 4× `table`-related, 2× `TimeLeft`/`Gui`/`_nGlobalFadeCountNew`/`_AllRequiredModulesLoaded`/`_fActionInterval`/`_GetLocalizedName`, plus ~45 singletons. Most are PC-side references that Xbox doesn't have; a few are the opposite. Full list in the structural diff report.

### 4.6 Language-loader WADs — per-SKU

Language-WAD shipping is a per-SKU decision, not a cross-platform divergence:

| SKU | Language WADs shipped |
|---|---|
| PC retail (complete install) | 5: English, French, German, Italian, Spanish |
| PS3 BLUS30056 | 6: English, French, German, Italian, Russian, Spanish |
| Xbox 360 NTSC-US (JTAGRip) | 2: English, French |

Each `<lang>.wad` ships **exactly one Lua chunk** — the same 4-proto / 562-insn template with a 179-entry `vo_asset_table` and a single `AddLocalizedAsset(".<lang>")` call. All 13 shipped chunks (5 PC + 6 PS3 + 2 Xbox) have the same normalised string-pool SHA-12 (`b6eef55f5c79`), differing only in endian, debug retention, and the single lang-suffix string constant.

**Platform-exclusive language chunks:**
- **PS3-only:** `russian.luac` — not shipped by PC retail or Xbox NTSC-US.
- **PC+PS3 but not Xbox NTSC-US:** German, Italian, Spanish. Likely present on PAL Xbox variants; only NTSC-US JTAGRip measured here.
- **Xbox NTSC-US-only:** none.

**Modder actionable**: adding Russian to PC is trivial at the Lua layer — clone any PC lang chunk, swap the suffix constant to `.russian`, recompile LE. The real work is the VO wave assets (the WAD body carries hundreds of MB of localised audio); the Lua stub is a 15-line template.

Full measurement: [`docs/_ps3_full_wad_set_lua_diff.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/_ps3_full_wad_set_lua_diff.md).

### 4.7 ★ `mrxguisatellitelayout` — a legit PC bug Xbox got fixed (fix-pack candidate)

In the satellite-overlay button widget, PC retail ships with a **placeholder** that Xbox got replaced before shipping:

| Field | PC Retail | Xbox 360 |
|---|---|---|
| Button name constant | `"Pistol"` | `"icon_hijack_button_A"` |
| Texture path | `.../buttons/Pistol.tga` | `.../buttons/icon_hijack_button_A.tga` |

Confirmed at [`docs/mercs2-luacd/src/vz_guilayouts/mrxguisatellitelayout.lua:234-235`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz_guilayouts/mrxguisatellitelayout.lua#L234-L235). This is a build-time leftover — someone forgot to swap the placeholder before PC build, Xbox got the proper asset.

**Modder actionable**: ideal fix-pack candidate. Single-constant Lua patch backports the Xbox-side correct asset to PC. One `replace_lua`-type edit in a `vz-patch.wad`.

---

## 5. Reserved-name slots — safe hooks for mod content

Two independent findings produce a combined **19 reserved-name slots** mod authors can target without collision risk:

### 5.1 Empty-shipping stubs (12, from Phase C'/A')

PC retail ships these resident scripts as **empty Lua chunks** (compiled body is literally `RETURN r0 1`). The engine loads them as no-ops. The matching Xbox entries decompile to a bare `local L0_1, L1_1`, confirming they're the same empty-stub shape:

```
all_debug         all_gui           all_hijack        all_humans
all_objectscript  all_preload       all_sound         all_testscript
all_vehicles      all_weapons       common_asset      healthpickup
```

**Modder actionable**: each slot is a safe injection point. A mod replacing `all_vehicles` with real content isn't overriding any existing behavior; the engine was going to load an empty chunk anyway. Great for category-wide registration patterns (register all your custom weapons from a replacement `all_weapons`).

### 5.2 Cut-contract slots (7, from Phase D PS3 EBOOT scan)

Script names referenced by both PC and PS3 engine binaries as plain-text constants, but with **no matching Lua body shipped on any platform**:

```
AllCon009   ChiCon005   GurCon051
PmcCon011   PmcCon012   PmcCon014   PmcCon017
```

The engine will attempt to look these up if a mission flow references them; `wifmissionflow.lua` still has one orphan mention of `chicon005`. A mod registering a Lua body against any of these names won't collide with anything shipped and is eligible to be called by the mission-flow graph that already has a hook.

---

## 6. Controller / input portability — well-abstracted by design

The input layer in retail PC Lua carries **both** controller texture-mapping tables in the same file:

```lua
-- Generic / DirectInput (PlayStation-style button textures)
_ControllerSpriteTextureMapping[Joystick.BUTTON_PAD2_U] = "icon_hijack_button_Y"
-- Xbox / XInput
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_PAD2_U] = "icon_hijack_xbox_button_Y"
```

Both tables are keyed on the same `Joystick.BUTTON_PAD2_U` abstraction. The engine picks one at runtime. **The input-key constants (`Joystick.*`, `MrxGuiBase.Joystick.*`) are the common abstraction across all three platforms — 269 references across the PC corpus — and a mod that uses them (rather than hard-coded button indices) is portable at the input layer by construction.**

What differs per platform is only:
- Which **sprite file** is used for the button-prompt icon (handled by the engine's dual-table lookup)
- Which **stringdb key** gets the label (e.g. `[SHELL.Controls.Jump]`)

Neither is a Lua-source-level concern — both are data-side.

One quirky edge: `mrxguibase.lua:1911-1914` has a hard A/B swap (`BUTTON_PAD2_D ↔ BUTTON_PAD2_R`), likely for Japanese button-convention. Specific-case code, not a general portability pattern.

---

## 7. DLC — one Lua bytecode, two wrapper formats

The "Blow It Up Again" DLC Lua is byte-identical in its chunks across Xbox and PS3:

| Classification | Count |
|---|---:|
| BYTE-IDENTICAL (raw block matches) | 13 |
| STRUCTURAL-IDENTICAL (Lua chunk matches; only non-Lua filler in the block differs) | 23 |
| REAL-DIVERGENCE | **0** |

The 23 "structural-not-byte" pairs differ only because **Havok 5.5 packfiles cohabit the same resident block** as the Lua chunks, and a 1-bit flip in the Havok flags preceding `"Havok-5.5."` creates a raw-block diff that doesn't touch the Lua. All 72 chunks share identical Lua headers (`1b4c75615100000404040400`).

**Modder actionable for DLC-shaped content**: produce one compiled Lua chunk (BE, float, debug-stripped). Deploy to Xbox via STFS/DOH, to PS3 via NPDRM EDAT in PKG. The outer wrapper differs; the Lua inside is identical by construction.

Full report: [`docs/mercs2-dlc-luacd/_ps3_xbox_structural_diff.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-dlc-luacd/_ps3_xbox_structural_diff.md).

---

## 8. Build pipeline per platform

### 8.1 PC Retail

- **Lua compiler**: `mercs2_luac` CLI from [`tools/wad_simulator/crates/mercs2_luac/`](../../crates/mercs2_luac/) (byte-exact match to retail compiler output; float `lua_Number`, LE)
- **Container**: `wad_builder` → FFCS LE, chunk tag `KEYS/STRS` for stringdb
- **Delivery**: drop into `Data\<name>-patch.wad`; the engine auto-scans for `*-patch.wad` next to every base WAD
- **Signing**: not required
- **Verification**: `qm lint ./shipment` then `qm build`

### 8.2 Xbox 360

- **Lua compiler**: same `mercs2_luac` but set endian flag + strip debug (or use a `--strip-debug --endian=big` variant per the crate's docs). Note the `Debug.Printf` source-strip is a build-system preprocessor, not just a compiler flag — if faithfulness-to-retail matters, remove the `Debug.*` calls at source level before compile.
- **Container**: SCFF BE, chunk tags reversed (`XDNI/ATAD/MUSC/TESA`), register in `package.cfg` with `datafile` + `scriptname`
- **Delivery**: STFS container with signed cert blob (requires Xbox LIVE signing; not available for homebrew without devkit)
- **Verification**: platform ABI varies; running under devkit `Profile` build is the only tractable path

### 8.3 PS3

- **Lua compiler**: same as Xbox (BE, float, debug-stripped)
- **Container**: SCFF BE inside an inner WAD; the inner WAD is wrapped in NPDRM EDAT (title klicensee `1896170d86be49b983b7135c96d6fb79`); the EDAT lives inside a PSN PKG
- **Delivery**: PKG install via homebrew-enabled console
- **Tooling**: [`tools/wad_simulator/crates/ps3_dlc_crypt`](../../crates/ps3_dlc_crypt) CLI handles PKG-unpack / EDAT-decrypt / SELF-unseal / klic-scan; reverse the chain for packaging. See `memory/ps3-dlc-crack-and-klicensee.md` (`../../.claude/projects/.../memory/ps3-dlc-crack-and-klicensee.md`).

### 8.4 Shared across all three

- **Mount order is LAST-WINS** on every platform (verified in-game on PC via `shell.wad` + `vz.wad` sharing one slot; the console mechanism via PS3 `v1.03` `VZ-PATCH.WAD`/`SHELL-PATCH.WAD` is the same shape).
- **Chunk-registry insert is FIRST-WINS** (opposite). A `-patch.wad` that redeclares a resident chunk name may silently drop its own asset if the base WAD already defined it. This is a hazard, not a feature.
- **The engine `Lua` namespace, module/import system, `MrxState`, `WifMissionFlow`, `Hud.Radar`, `MrxGuiBase`, `Sys.*` surface is identical across platforms** — none of the 371 PC scripts reference a platform-conditional API gate (no `IsPc`, `IsConsole`, `IsXbox` constants exist in Lua).

---

## 9. What's proven vs inferred

**PROVEN at the byte level**:
- All 378 PC↔Xbox base-game pair classifications (`lua_structural_dump` + classifier, 2026-10-02)
- All 36 Xbox↔PS3 DLC pair classifications (same tool, 2026-10-02)
- Lua headers across all 756 base-game chunks + 72 DLC chunks: `float` on every platform; endian flag the only cross-platform difference
- The 19 reserved-name slots (12 empty-shipping + 7 cut-contract)
- The `mrxguiltiprecache` PC-only pair

**STRONGLY INFERRED but not direct byte diff**:
- PS3 base-game Lua content ≡ Xbox base-game Lua content (supported by Phase D: the PS3 EBOOT references the same 42/114 script names as literals and resolves the other 72 by WAD BINN iteration + runtime hash — identical mechanism to PC and the engine never ships platform-divergent script manifests). The ONLY place the inference would fall apart is if the PS3 base-game `VZ.WAD` (behind an uncracked envelope cipher) ships a *different* Lua chunk per name than Xbox. No evidence for that, and the DLC byte-identity is a strong prior against it.

**UNKNOWN**:
- PS3 base-game WAD contents (envelope encryption not yet solved; the 18-byte keystream prefix is as far as we got)
- Any Lua chunks in blocks we haven't extracted (we've covered `scripts_vz`, `resident`, `shell`; there may be other named blocks)

---

## 10. Reference

- [`docs/mercs2-luacd-xbox/_structural_diff_report.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-luacd-xbox/_structural_diff_report.md) — 378-pair PC↔Xbox classification table
- [`docs/mercs2-dlc-luacd/_ps3_xbox_structural_diff.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-dlc-luacd/_ps3_xbox_structural_diff.md) — 36-pair DLC classification
- [`docs/mercs2-luacd-xbox/_manifest.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-luacd-xbox/_manifest.md) — Xbox decomp invocation + header-correction note
- [`docs/mercs2-luacd-xbox/_corpus_lua_tool_eval.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-luacd-xbox/_corpus_lua_tool_eval.md) — why the `corpus_lua_*` MCP tools don't do this directly
- [`tools/wad_simulator/crates/mercs2_probe/src/bin/lua_structural_dump.rs`](../../crates/mercs2_probe/src/bin/lua_structural_dump.rs) — the structural dump tool
- [`docs/cross_platform_parity_reference.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/cross_platform_parity_reference.md) — the broader parity reference this fits into
- Memory: `ps3-dlc-crack-and-klicensee`, `multiplatform-compile-backend-assessment`
