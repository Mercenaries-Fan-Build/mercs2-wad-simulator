# GFx authoring feature spec — golden-set-backed guide for `.gfx` emitters

**What this is.** A spec for adding the features a community authoring tool
([`gfxforge`](https://github.com/loganw234/mercs2-tools-gfxforge), our review:
[`gfxforge_tooling_review.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/gfxforge_tooling_review.md)) is missing — **gradients, bitmaps, and
the rest** — grounded in **our 83 extracted retail movies as the golden reference set**. Every claim
here is measured, not assumed: the inventory tool `mercs2_probe gfx-golden`
([`gfx_golden.rs`](../../crates/mercs2_probe/src/gfx_golden.rs))
parses all 83 movies (`output/gfx_movies/`) and emits [`gfx_golden_set.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/gfx_golden_set.md) (the
report) + [`gfx_golden_set.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/gfx_golden_set.json) (the machine-readable index). Re-run,
from the repo root:

```
cargo run -p mercs2_probe -- gfx-extract output/gfx_movies         --wad <game>/data/vz.wad
cargo run -p mercs2_probe -- gfx-extract output/gfx_movies/shell   --wad <game>/data/shell.wad
cargo run -p mercs2_probe -- gfx-extract output/gfx_movies/loading --wad <game>/data/Loading.wad
cargo run -p mercs2_probe -- gfx-golden output/gfx_movies \
    --md docs/reverse_engineer/gfx_golden_set.md --json docs/data/gfx_golden_set.json
```

**The runtime ceiling (fixed).** Retail is **Scaleform GFx 2.0.48 / Flash 8 / AVM1 (AS2)**
([[scaleform-gfx-2048-lib-map]]). The loader rejects anything it flags
`"incompatible GFX file, version 2.x expected"`, and it runs **no AS3** — the golden set confirms
this empirically: **0 of 83** movies carry `DoABC` (AS3). So "supportable" = "whatever GFx 2.0.48
implements", and the tag-loader map ([`scaleform_gfx_function_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/data/scaleform_gfx_function_map.json))
plus the golden set below tell you exactly what that is.

---

## 1. The confirmed tag-support surface (what the runtime actually loads)

Every SWF/GFx tag the 83 shipping movies use (full table in `gfx_golden_set.md` §2). This is the
authoritative "safe to emit" set — if retail uses it, GFx 2.0.48 loads it:

| Category | Tags present in retail | Notes |
|---|---|---|
| Container | `GFX`/`CFX` header, `FileAttributes`(69), `SetBackgroundColor`(9), `ShowFrame`(1), `End`(0), `Protect`(24) | **retail ships `CFX` = zlib-compressed**; gfxforge emits raw `GFX` (loads, but larger — §7) |
| Shapes | `DefineShape`(2), `DefineShape2`(22), `DefineShape3`(32), `DefineShape4`(83) | all four in use; fills incl. gradient + bitmap (§3, §4) |
| Placement | `PlaceObject2`(26, 21.6k×), `PlaceObject3`(70), `RemoveObject2`(28) | matrix-always (gfxforge already does this) |
| Text | `DefineEditText`(37), `CSMTextSettings`(74) | dynamic fields + AA settings (§5) |
| Fonts | `DefineFont3`(75), `ImportAssets2`(71), `ExportAssets`(56) | embed-in-lib + import-by-name (§5) |
| Structure | `DefineSprite`(39), `FrameLabel`(43) | MovieClips (§6) |
| Morph | `DefineMorphShape`(46), `DefineMorphShape2`(84) | shape tweens (§6) |
| Script | `DoAction`(12) | **AVM1 only** — gfxforge already does this |
| **GFx-extension** | `GFx_ExporterInfo`(1000), `GFx_DefineExternalImage`(1001), `GFx_FontTextureInfo`(1002), `GFx_DefineExternalGradientImage`(1003), `GFx_DefineSubImage`(1004) | **the GFx-specific image/font machinery — §4** |

**Not used by any retail movie** (so unnecessary, and in one case impossible): `DefineButton`/
`DefineButton2` (0 — retail uses host-driven MovieClips, not SWF buttons, §6), `DefineVideoStream`
(0), embedded bitmap tags `DefineBits*` (0 — **images are always external**, §4), and `DoABC` (0 —
no AS3).

---

## 2. Priority for a `gfxforge`-style emitter

Ranked by value ÷ effort, from the golden data:

| # | Feature | Effort | Why | Golden example |
|---|---|---|---|---|
| 1 | **Gradient fill** | low (pure emitter) | format known, 3 golden movies | `SHELL`, `Statistics`, `pause_menu` (§3) |
| 2 | **External bitmap reference** | med (movie **+** WAD) | the real image path; 49 movies, 1296 image tags | `SHELL` (177 bitmap fills) (§4) |
| 3 | **CFX (zlib) output** | trivial | match retail convention / size | any retail movie is `CFX` (§7) |
| 4 | **Curved shapes** | med | arbitrary vector art beyond rects | any `DefineShape*` movie (§8) |
| 5 | Font embedding | med | *unnecessary* — import path covers shipped fonts | `GFxFontLib` (§5) |

---

## 3. Gradients (fill types `0x10`/`0x12`/`0x13`)

**Golden**: `SHELL`, `Statistics`, `pause_menu` (the only 3 movies with an inline shape gradient —
and, tellingly, the only 3 carrying `GFx_DefineExternalGradientImage`(1003), so GFx often bakes
author-time gradients into an **external gradient image** and references it as a bitmap fill; a plain
inline gradient still works).

gfxforge's `define_shape3` emits a **solid** fill (`FILLSTYLE` type `0x00` + RGBA). A gradient is the
same `FILLSTYLEARRAY` slot with a different type:

```
FILLSTYLE (gradient):
  U8   type            0x10 linear | 0x12 radial | 0x13 focal-radial (Flash 8)
  MATRIX gradientMatrix   (bit-packed; maps the [-16384..16384] gradient square to the shape)
  GRADIENT:
    UB[2] spreadMode        (0 pad / 1 reflect / 2 repeat; 0 pre-Flash8)
    UB[2] interpolationMode (0 normal / 1 linear-RGB)
    UB[4] numGradients      (1..15)
    GRADRECORD[numGradients]: { U8 ratio (0..255); RGBA color }   // RGB for DefineShape/2
  [ if type==0x13: FIXED8 focalPoint ]
```

The GFx runtime parses this via `GFxFillStyle::Read` (`FUN_007df960`) / `SetGradientData`
(`FUN_007dfb50`); the AS2 dynamic form is `MovieClip.beginGradientFill` (`FUN_00796f80`). To validate
a new `Movie.gradient_rect(...)`, byte-diff the emitted `DefineShape3` fill record against a gradient
fill in `SHELL` (decompress its CFX first).

---

## 4. Bitmaps — always external (this is the big one)

**Measured fact**: retail embeds **zero** bitmaps (`DefineBits*` = 0 across all 83 movies). Every UI
image is an **external texture**, referenced two ways that the golden set makes concrete:

1. **The image object** — `GFx_DefineExternalImage`(1001): **1,296 occurrences across 50 movies**.
   This tag binds a character id to an external texture *by name* (a `0xF011157A` DXT texture asset in
   the WAD — the `scaleform_shell` block is 416 such textures, `shell_i<hex>`,
   [loading_shell_wad_analysis.md](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/loading_shell_wad_analysis.md) §6). GFx runtime:
   `GFxImageResource` + the image-movie-def ctor (`FUN_007da120`).
2. **The bitmap fill** — a shape `FILLSTYLE` type `0x40`–`0x43` (`{tiled|clipped}×{smoothed|not}`)
   whose `bitmapId` U16 points at the external-image character, plus a `MATRIX`:

```
FILLSTYLE (bitmap):  U8 type (0x40..0x43);  U16 bitmapId;  MATRIX bitmapMatrix
```

**Golden**: `SHELL` (177 bitmap fills), `0x7269553D` (145). AS2 dynamic form:
`MovieClip.beginBitmapFill` (`FUN_00797870`).

So a `Movie.image(name, x, y, w, h)` in an authoring tool is a **two-part** contribution: (a) emit a
`GFx_DefineExternalImage` character bound to a texture name + a `DefineShape` with a bitmap fill; (b)
inject that DXT texture into the WAD with the matching name (our converter already writes
`0xF011157A` textures — this is the WAD-side half gfxforge explicitly leaves to "your WAD tool").
`GFx_DefineSubImage`(1004) lets several UI images share one atlas texture (3 movies use it).

---

## 5. Fonts — import is the retail idiom (embedding is optional)

The golden set shows both halves of the shared-font model:
- **Font libraries** embed glyphs: `DefineFont3`(75) in the font-lib movies, published via
  `ExportAssets`(56). (`GFxFontLib.gfx` / `_normal_font.gfx` / the language `fonts_*` movies.)
- **Everyone else imports**: `ImportAssets2`(71) in **29 movies** — exactly gfxforge's
  `import_font("_normal_Font")`. So gfxforge already does the right thing; **font embedding is
  unnecessary** for Mercs2. If ever needed, the embed path is `DefineFont3` + `GFx_FontTextureInfo`
  (1002, the glyph-atlas texture info; runtime `GFxFontPacker` `FUN_007fc760`/`FUN_007fcd20`).
- **Text**: `DefineEditText`(37) + `CSMTextSettings`(74, anti-alias). gfxforge does the edit-text
  part; add a `CSMTextSettings` tag to match retail's text AA. **Golden**: `SHELL` (227 edit texts).

---

## 6. Structure — sprites yes, buttons no

- **Sprites / MovieClips** (`DefineSprite`, 60 movies): the host-addressable timeline. gfxforge
  already emits these; retail leans on them heavily (`SHELL` = 304). **Golden**: `SHELL`.
- **Buttons**: **retail uses zero `DefineButton`.** Selection/clicks are driven from the **host**
  (Lua `_HandleInputForFlashWidget` → `SendFlashInput`, [[scaleform-gfx-2048-lib-map]]) over
  host-moved MovieClips + AVM1 `fscommand`. So gfxforge's `button()`/`menu()` are valid GFx but not
  the retail idiom — the more faithful pattern is a named MovieClip the host drives (which gfxforge's
  `clip()`/`menu()` highlight already does). No golden button exists to diff against; use the host
  input path instead.
- **Morph shapes** (`DefineMorphShape/2`, 17 movies): shape tweens for smooth UI transitions.
  gfxforge doesn't emit these. **Golden**: `pause_menu` (111 morphs).

---

## 7. Container — CFX vs raw GFX

Retail ships **`CFX`** (zlib-compressed: magic `CFX`, version `08`, a u32 length, then a `78 DA`
zlib stream), except three raw **`GFX`** movies: `fanfare_new_contact`, `fanfare_support_unlocked`
and `snatch_moment`. gfxforge emits raw **`GFX`** — the loader accepts both (it dispatches on the
magic), so gfxforge output loads, but it's larger on disk (relevant to WAD page budget). Adding CFX
output is a one-function change (deflate the body, swap the magic byte). Our tool decompresses CFX
transparently.

The header's u32 is not the inflated file's length:
- It counts the body **through the `End` tag**. Whether it also counts the header's own 8 bytes is
  the writing tool's choice: retail's `CFX` movies count them, its three `GFX` movies do not, and the
  `GFX` movies a `vz-patch.wad` adds do.
- A retail `CFX` body runs on past `End` to a **multiple of 4096 bytes**, and those bytes are not
  zero. A loader stops at `End`; a writer that wants the retail bytes keeps them.

`mercs2_formats::gfx_stream` reads and writes a movie with these choices kept; the workshop's
`every_movie_writes_back_as_it_was_read` checks it over every movie of `vz.wad` and of the default
stack.

## 7a. Writing tags back to their bytes

Measured over the 70 movies of `vz.wad` and the 12 a default `vz-patch.wad` adds, every instance
(`mercs2_formats::gfx_tags` reads them; the same workshop test checks each one writes back):

- **Bit-field widths.** In `PlaceObject2`/`3` and `DefineEditText`, every `MATRIX`, `CXFORMWITHALPHA`
  and `RECT` field is as wide as the bit length of its largest magnitude plus a sign bit. When every
  value is 0, a translation is 0 bits wide, and a scale or colour transform is 1 bit wide (no
  rotate-skew pair is all zero). About 1 in 100 of these widths is one bit wider than the smallest
  that holds the values: the rule, not the minimum, is what the exporter wrote.
- **Padding** bits after a bit field are always zero. No `PlaceObject3` sets a class name, an image or a
  reserved flag, and none carries a filter. Every string (instance names, labels, variables, text,
  export and import names) is valid UTF-8.
- **Tag header form.** A tag body under 63 bytes may still be written with the 6-byte header. In
  retail, `DoAction`, `DefineEditText`, `DefineSprite`, `FrameLabel`, `ExportAssets` and
  `ImportAssets2` always use it; `PlaceObject2`/`3` use it exactly when they carry an instance name;
  `ShowFrame` and `RemoveObject2` never do. The `vz-patch.wad` movies do not follow this (short
  headers on named placements, edit text, sprites and imports), so a byte-exact writer keeps the form
  each tag was read in.
- **Order within a frame** is usually `RemoveObject2`, `DoAction`, `FrameLabel`, then placements, but
  not always (placements before an action, a label before an action, two actions around a label), so a
  writer keeps each frame's order.
- **Motion** is stored per frame, baked from eased tweens: of the frames inside a run of three
  consecutive moves, about 20% are exactly linear in the stored integers and about half are more than
  2 units off linear (`SHELL`: 13% and 70%).

---

## 8. Curves (not yet inventoried per-feature)

gfxforge only draws axis-aligned rectangles (4 straight edges). Arbitrary vector art needs the SWF
**SHAPE record** edge encoding (straight + quadratic-Bézier `CurvedEdgeRecord`s after the
fill/line-style arrays). The `gfx_golden` tool currently flags gradient/bitmap **fills** but not
curved **edges** — that's the one feature axis it doesn't yet break out (noted in the tool). Any
`DefineShape*` movie is a golden example; the encoding is standard SWF and reversible from them. This
is the natural next extension of the golden tool if curve authoring is wanted.

---

## 9. How to use the golden set

- **Pick the example**: `gfx_golden_set.md` §1 (feature → golden movie) tells you which retail movie
  demonstrates the feature you're adding.
- **Diff against it**: decompress that movie's CFX (`78 DA` zlib) and compare your emitter's tag/field
  bytes against the real ones. The tool's per-movie tag histogram (`gfx_golden_set.json`) tells you
  which tags each movie contains.
- **Regression-gate**: `gfx_golden_set.json` is a stable machine-readable index — a golden test can
  assert "my authored movie's tag set ⊆ the retail support surface" so you never ship a tag GFx
  2.0.48 rejects.

**Bottom line**: we've decoded enough to fully support the modding-relevant feature set. The two
highest-value additions (gradients, external-image references) have known encodings and named golden
examples; the rest is either already handled (fonts-by-import, sprites, AVM1) or deliberately absent
in retail (buttons, video, AS3, embedded bitmaps). The golden set is the validation harness that
makes each addition provable against a movie the game actually loads.
