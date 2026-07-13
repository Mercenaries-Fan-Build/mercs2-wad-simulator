# mercs2_ui

The GUI/HUD state model for the Mercenaries 2 reimplementation — the retained-mode widget tree behind
the game's `Hud.*` Lua surface, and the world-space marker set behind `Gui._Marker*`.

## What it is

Two owned state models, no rendering:

- **A retained-mode widget tree** (`WidgetTree`). Retail's `Hud.*` cfuncs are a retained-mode widget
  API: script code mints a node (`CreateWidget`, `CreateImageWidget`, `CreateTextWidget`,
  `CreateSpriteWidget`, `CreateMovieWidget`, `CreateFlashWidget`, `MinimapCreate`), sets its
  location/colour/visibility/anchoring/viewport, parents it into a tree, and mutates per-kind data
  (image texture + UVs, text string/font/justification, sprite frame, movie file, `.swf`, minimap
  focus/range/objectives). This crate is the handle registry of those nodes, with the full property
  set the Lua reads back, so `Set*` → `Get*` round-trips for real — a HUD script that hides a widget
  actually hides it. Parenting, orphaning-on-delete, and `PushWidgetToFront`/`Back` z-restamping are
  modelled; `draw_order()` yields the back-to-front handle list a renderer would walk.
- **A HUD marker set** (`MarkerSet`). Blip / tripwire / disc / 3D / objective markers, each pinned to a
  world location or following an object GUID, with colour, scale and pulse state, plus the
  `_MarkerSetBlipLimit` cap.

The GFx *rasterization* — actually drawing the tree — is deliberately not here; it is a separate render
pass. This crate owns the scene-graph state that pass consumes.

The crate is live, not dormant: `mercs2_script` binds the `Hud.*` and `Gui._Marker*` Lua surfaces
directly to these types via the `EngineHost::hud` / `hud_ref` / `markers` / `markers_ref` seam, and
`mercs2_engine` owns the instances and re-exports the crate as `mercs2_engine::widgets`. Hosts that
return `None` from that seam (smoke/test hosts) make the `Hud.*` mutators no-ops.

## Where it comes from

Silo 15 of `docs/modernization/reimplementation_parallelization_plan.md` §3, covering scoreboard rows
27 and 18. The code map the crate cites is
`docs/reverse_engineer/scaleform_gfx_class_map.md` (plus `docs/reverse_engineer/input_code_map.md`).

Retail drives the HUD through a Scaleform GFx overlay; the shape of the model here is taken from the
`Hud.*` / `Gui.*` cfunc surface those maps describe. Owned Lua namespaces for the silo: `Hud`, `Pda`,
`Gui`, `Marker`, `_GuiInternal`.

## Usage

Library only — no binaries.

```rust
use mercs2_ui::{MarkerKind, MarkerSet, WidgetKind, WidgetTree};

// Retained-mode HUD tree: mint, mutate, parent, order.
let mut hud = WidgetTree::new();
let root = hud.create(WidgetKind::Container);
let label = hud.create(WidgetKind::Text);

hud.add_child(root, label);
assert_eq!(hud.children(root), vec![label]);

let w = hud.get_mut(label).unwrap();
w.location = [64.0, 32.0];
w.visible = true;
w.text.as_mut().unwrap().text = "OBJECTIVE".into();
w.text.as_mut().unwrap().justification = 1; // 0 = left, 1 = center, 2 = right

hud.push_to_front(label);
for handle in hud.draw_order() {
    // back-to-front: the renderer's walk order
    let _node = hud.get(handle).unwrap();
}

// World-space markers.
let mut markers = MarkerSet::new();
let objective = markers.add(MarkerKind::Objective);
markers.set_location(objective, [10.0, 0.0, 20.0]);
markers.set_follow(objective, 0x1000); // track a GUID; 0 = stay pinned to `location`
markers.set_pulsing(objective, true);
markers.remove(objective);
```

## Modules

- `widget` — the retained-mode widget tree behind `Hud.*`: `WidgetTree`, `Widget`, `WidgetKind`, and
  the per-kind payloads `ImageData` / `TextData` / `SpriteData` / `MovieData` / `FlashData` /
  `MinimapData`.
- `marker` — world-space HUD markers behind `Gui._Marker*`: `MarkerSet`, `Marker`, `MarkerKind`.

Both are re-exported at the crate root.

## Notes / gotchas

- **Widget colour is in the caller's domain, not normalized.** `Widget::color` defaults to
  `[255.0, 255.0, 255.0, 255.0]` because the game passes D3DCOLOR-style `0..255` components. Do not
  assume `0.0..=1.0`. (`Marker::color`, by contrast, defaults to `[1.0; 4]`.)
- **Handles are stable, unique and non-zero**, minted from a monotonic counter. They are never
  recycled, so a stale handle reads back as `None` rather than aliasing a new node.
- **Deleting a widget orphans its children rather than deleting them** — `delete()` detaches the node
  from its parent and clears each child's `parent` link, leaving the children live in the registry.
- **`push_to_back` restamps to `min_z - 1`**, so z values drift negative over repeated calls. Only the
  relative order is meaningful; do not persist z as a stable identifier.
- The silo's **input-extension** half (row 18) is scoped to this crate but not implemented here yet;
  only the HUD/marker state models exist. Likewise there is no GFx rasterizer in this crate.
