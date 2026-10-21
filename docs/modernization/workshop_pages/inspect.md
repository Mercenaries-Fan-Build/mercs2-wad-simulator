# Inspect — behaviour contract

Artboard: `Inspect.dc.html` ("Inspect · Models › Characters") of the Workshop Polish Pass design.
Capture state: `crates/mercs2_workshop/ui_states/inspect.json`.

Every row is an element of the page: what the user does, what happens, where the data comes from, what
it writes, and what proves it. Nothing on the page is drawn without its row being true.

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Header crumb | none; shows `INSPECTING` then the tab, the Lens (Models tab with a Lens on), and the loaded model with its hash — e.g. `Models › Characters › pmc_hum_mattias 0x…` | `kind`, `lens`, `preview` in `app.rs` | — | test: crumb text for no lens / lens / loaded |
| Tab strip | click a tab → the list shows that kind; each count is the live row count | `index::AssetIndex::count`, `Kind::ALL` | — | capture |
| Lens row (Models tab) | click a pill → the list narrows to that domain; counts live; the Lens is kept when you switch tabs and come back | `domain::Domain::model_lens`, `app::in_lens` | — | test (exists: `a_lens_count_is_what_the_lens_lists`) + capture |
| Filter | typing narrows the list by name, live; Enter loads the cursor row | `app::filter_rows` | — | existing behaviour |
| Group headers | under a Lens, rows group by faction (the model name's prefix before the first `_`, shown upper-case: PMC, VZ, AL, CIV, OC…), each with its live count; with All, by category as today; click ▾/▸ folds a group | `AssetRow::name` (a kit-template row carries its kit model's name, `index.rs:365`) | — | test: grouping + counts over real names |
| Row | click → loads the model into the viewport (as today) and moves the cursor there; ↑/↓ move the cursor; the cursor row is highlighted | `Act::LoadModelHash` | — | existing behaviour |
| Right-click menu: Load in viewport · Enter | loads the row's model | `Act::LoadModelHash` | — | existing |
| Export… · F10 | opens the Export dialog for that model | see Export dialog | — | — |
| Retarget a rig onto this / Fit an import to this | opens the Retarget / Fit bench with this model as donor; writes nothing (the bench writes on save) | `Act::Bench` | — | existing |
| Add to Shipment ▸ | each route adds a contribution seeded with this model (scaffolding a Shipment when none is open) and opens it on the Shipment page | `quartermaster::{routes_for, seeded, ensure_shipment, upsert_contribution}` | `manifest.yaml` | existing |
| Start a Shipment from this… | folder picker → scaffold a NEW Shipment there (the open one is closed) → add this model's first route (`routes_for(..)[0]`) → Shipment page with it selected; cancelling the picker changes nothing | `quartermaster::{scaffold, seeded, upsert_contribution}` | new folder: `manifest.yaml`, `src/`, `README.md` | test: new Shipment holds exactly the routed contribution |
| Copy name / hash / manifest reference | below a separator; copy to the clipboard | `asset_menu` | clipboard | existing |
| Export dialog (new) | **Name** (default: the asset's name), **Format**: *Lossless bundle* (glTF + every LOD rung's original bytes + textures + manifest — what F10 writes today) or *OBJ + MTL + PNG*; **Folder** (default `workshop_export/`, Choose… to change). Export runs in the background with the existing progress window; the status line names what was written. Cancel closes it | `export_bundle_in_background` / `bundle::export_bundle_by_hash`, `export_model_data` | `<folder>/<name>/…` | test: each format lands under the chosen folder and name |
| Verb bar: Export · F10 | enabled when a model is loaded; opens the Export dialog for it | as above | — | — |
| Verb bar: View textures · F3 | opens the texture plate viewer on the loaded model's textures | `Act::TexOfPreview` | — | existing |
| Verb bar: Next clip · F4 | steps to the next animation clip; **F4 is not bound today — bind it** | `Act::ClipNav(1)` | — | test: F4 maps to `ClipNav(1)` |
| Viewport | orbit camera; ORBIT chip | existing | — | capture |
| Inspector, nothing loaded | "No model loaded — pick one in the browser at left." With a Lens on: the Lens card — live model count, the domain's blurb, START A CONTRIBUTION FROM HERE with one button per domain kind (short title); a button adds that kind's stub (scaffolding a Shipment when none is open) and opens it on the Shipment page | `Domain::{blurb, kinds}`, `quartermaster::kind_title`, `Act::AddToShipment` | `manifest.yaml` | capture; existing add path |
| Inspector, loaded | the existing model cards | existing | — | — |
| Footer | READY + the load summary | existing | — | — |

## Open question (needs an answer before the grouping row is built)

Some kit-template rows (the "AL Defender (AA)", "Allied Airborne"… rows the Characters Lens lists) are
kits whose model has no known name, so they have no prefix to group by. The world templates do carry
a `FactionMarker` component (one u32 per template, `mercs2_formats/src/schema.rs:938-953`), but which
names its values resolve to — and whether those line up with the PMC/VZ/AL/CIV/OC prefixes — is not
yet checked. Options: group those rows by their `FactionMarker` once its values are resolved and shown
to line up; or put rows with no prefix under their own header.
