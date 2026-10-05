//! Effects, templates and sprites: the `replace_fx` edits form, target resolution, and the one
//! merge `qm build` and `qm link` both run.
//!
//! The game ships every effect in ONE block, `blocks\VZ\effects_P000_Q3.block` (314 effects and
//! the 46 models they draw), and every template in ONE container, the `worldentity` `0x50075B3B` in
//! `blocks\VZ\resident_P000_Q3.block`; the resident block also carries the `vfx` atlas every
//! particle samples and the `fxdict` that names its sprite rectangles ([`crate::sprite`]). [`merge`]
//! applies a set of Shipments' `add_fx_sprite`, `add_fx` and `replace_fx` to the game's copies, and
//! the builder re-emits the effects block at its own path and the worldentity, the `fxdict` and the
//! atlas inside the resident block.
//!
//! First, every Shipment's `add_fx_sprite` is packed into the base atlas ([`base_atlas`]: the set's
//! `replace_texture` of `vfx` when it has one, else the game's) with [`crate::sprite::pack`], and
//! each sprite's record joins the `fxdict`. A sprite whose name hashes to a key the `fxdict` already
//! has, or another sprite's, is refused.
//!
//! Then, Shipment by Shipment in the order given (`qm link`: the load plan's order, in which a
//! Shipment comes after the Shipments it requires):
//!
//! 1. each `replace_fx`, in contribution order: its target is resolved ([`FxTarget`]) — an effect by
//!    name or `0xHHHHHHHH`, or a template by name through its one `RedEffectComponent`'s `name`
//!    field — against the game plus the effects and templates added by the Shipments it requires,
//!    and its edits ([`EditsForm`]) are applied to that effect in place;
//! 2. each `add_fx`, in contribution order: its effect ([`crate::effect::EffectForm`]) is appended
//!    to the effects block under `pandemic_hash_m2(name)`, and its template
//!    ([`crate::template::TemplateForm`]) is appended to the worldentity under
//!    [`derived_template_key`].
//!
//! Two `replace_fx` that resolve to one effect are a [`Conflict`], whether either names the effect
//! directly or through a template.
//!
//! Every frame a Shipment gives an effect is a record of the game's `fxdict`, a sprite of the
//! Shipment, or a sprite of a Shipment it requires. `qm build` of a Shipment that requires others
//! leaves a frame it finds in none of these to `qm link`, which has the Shipments it requires.
//!
//! The edits form is a document with one key, `edits`, an ordered list of operations, each tagged
//! by `op`:
//!
//! | `op` | fields | what it does |
//! |---|---|---|
//! | `attribute` | `emitter`, `attribute`, and any of `value`, `curve`, `options` | edit one `PTYP` attribute |
//! | `channel` | `emitter`, `channel`, and any of `value`, `curve`, `options` | edit one `TRFM` channel |
//! | `colour_rgb` | `emitter`, `rgb` | set the first three bytes of all 100 `COLR` keys, keeping each key's fourth byte and `half` |
//! | `colour_keys` | `emitter`, `keys` (100 `{rgba, half}`) | replace the `COLR` keys |
//! | `frames` | `emitter`, `frames` | replace the `TEXT` frames |
//! | `transform` | `emitter`, `transform` | replace the `TRFM` 4×4 |
//! | `flags` | `emitter`, `flags` | replace the `PTYP` flags |
//! | `geom` | `emitter`, `geom` (`{shape, word}` or `none`) | replace the emitter's `GEOM` |
//! | `force_params` | `force`, `params` | replace a force's kind and parameters |
//! | `force_attribute` | `force`, `attribute`, and any of `value`, `curve`, `options` | edit one `FRCE` attribute |
//! | `shape` | `shape`, `records` | replace one `EMTR` shape table |
//! | `add_emitter` | `at`, `emitter` (a whole emitter, as the effect form writes one) | insert an emitter before index `at` (`at` = the count appends) |
//! | `remove_emitter` | `emitter` | remove an emitter |
//! | `add_force` | `at`, `force` | insert a force |
//! | `remove_force` | `force` | remove a force |
//! | `add_shape` | `at`, `records` | insert a shape table; every `GEOM` naming a shape at or after `at` moves up one |
//! | `remove_shape` | `shape` | remove a shape table no `GEOM` names; every `GEOM` naming a later shape moves down one |
//!
//! In an attribute edit, an absent `value`, `curve` or `options` keeps the attribute's own; `curve:
//! none` removes its curve. After the last operation the effect is checked against every rule the
//! effect writer enforces.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use mercs2_formats::fxdict::{parse_effect_container, write_effect_container, write_fxdict_container, EffectContainer, FxRect};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::schema::FieldValue;
use mercs2_formats::scripts_block::{Entry, ScriptsBlock};
use mercs2_formats::types::TYPE_HASH_EFFECT;
use mercs2_formats::worldentity::{
    derived_template_key, Value, WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, TEMPLATE_KEY_BIT, WORLDENTITY_TYPE_HASH,
};
use serde::{Deserialize, Serialize};

use crate::effect::{
    self, ColourKeyForm, CurveForm, EmitterForm, ForceForm, ForceParamsForm, GeomForm, ShapeForm, ValueInput,
};
use crate::manifest::{Contribution, FxTarget, Manifest, Requirement};
use crate::sprite::{self, Atlas, Placed, Square};
use crate::Format;

/// The effects block, as `(PTHS needle, PTHS path)`. The needle is anchored on its folder, as the
/// resident needle is ([`crate::link::SCRIPT_BLOCKS`]).
pub const EFFECTS_BLOCK: (&str, &str) = (r"\VZ\effects_P000_Q3.block", r"blocks\VZ\effects_P000_Q3.block");

/// The component class whose `name` field names the effect a template starts.
pub const RED_EFFECT_CLASS: &str = "RedEffectComponent";

/// `RedEffectComponent`'s `name` field (`pandemic_hash_m2("name")`, schm code 6, offset 0).
pub const RED_EFFECT_NAME_FIELD: u32 = 0x1DE5_C824;

/// The entry-table `field_c` of an effect: 0 in all 314 retail effects.
pub const EFFECT_FIELD_C: u32 = 0;

/// A `replace_fx` edits document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditsForm {
    pub edits: Vec<Edit>,
}

/// One edit operation. See the module docs for each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Attribute {
        emitter: usize,
        attribute: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<ValueInput>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        curve: Option<CurveForm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        options: Option<Vec<effect::AtrbOption>>,
    },
    Channel {
        emitter: usize,
        channel: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<ValueInput>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        curve: Option<CurveForm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        options: Option<Vec<effect::AtrbOption>>,
    },
    ColourRgb {
        emitter: usize,
        rgb: [u8; 3],
    },
    ColourKeys {
        emitter: usize,
        keys: Vec<ColourKeyForm>,
    },
    Frames {
        emitter: usize,
        frames: Vec<String>,
    },
    Transform {
        emitter: usize,
        transform: [[f32; 4]; 4],
    },
    Flags {
        emitter: usize,
        flags: u32,
    },
    Geom {
        emitter: usize,
        geom: GeomForm,
    },
    ForceParams {
        force: usize,
        params: ForceParamsForm,
    },
    ForceAttribute {
        force: usize,
        attribute: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<ValueInput>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        curve: Option<CurveForm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        options: Option<Vec<effect::AtrbOption>>,
    },
    Shape {
        shape: usize,
        records: ShapeForm,
    },
    AddEmitter {
        at: usize,
        emitter: Box<EmitterForm>,
    },
    RemoveEmitter {
        emitter: usize,
    },
    AddForce {
        at: usize,
        force: ForceForm,
    },
    RemoveForce {
        force: usize,
    },
    AddShape {
        at: usize,
        records: ShapeForm,
    },
    RemoveShape {
        shape: usize,
    },
}

impl Edit {
    /// The `op` tag as written.
    pub fn op(&self) -> &'static str {
        match self {
            Edit::Attribute { .. } => "attribute",
            Edit::Channel { .. } => "channel",
            Edit::ColourRgb { .. } => "colour_rgb",
            Edit::ColourKeys { .. } => "colour_keys",
            Edit::Frames { .. } => "frames",
            Edit::Transform { .. } => "transform",
            Edit::Flags { .. } => "flags",
            Edit::Geom { .. } => "geom",
            Edit::ForceParams { .. } => "force_params",
            Edit::ForceAttribute { .. } => "force_attribute",
            Edit::Shape { .. } => "shape",
            Edit::AddEmitter { .. } => "add_emitter",
            Edit::RemoveEmitter { .. } => "remove_emitter",
            Edit::AddForce { .. } => "add_force",
            Edit::RemoveForce { .. } => "remove_force",
            Edit::AddShape { .. } => "add_shape",
            Edit::RemoveShape { .. } => "remove_shape",
        }
    }
}

/// Parse an edits document from text.
pub fn edits_from_str(text: &str, format: Format) -> Result<EditsForm, String> {
    match format {
        Format::Yaml => serde_norway::from_str(text).map_err(|e| e.to_string()),
        Format::Json => serde_json::from_str(text).map_err(|e| e.to_string()),
        Format::Toml => toml::from_str(text).map_err(|e| e.to_string()),
    }
    .map_err(|e| format!("edits form ({format:?}): {e}"))
}

/// Write an edits document as text.
pub fn edits_to_string(form: &EditsForm, format: Format) -> Result<String, String> {
    match format {
        Format::Yaml => serde_norway::to_string(form).map_err(|e| e.to_string()),
        Format::Json => serde_json::to_string_pretty(form).map_err(|e| e.to_string()),
        Format::Toml => toml::to_string(form).map_err(|e| e.to_string()),
    }
}

/// Read an edits file, its format taken from its extension. An empty list is refused: it would
/// claim an effect and change nothing.
pub fn read_edits(path: &Path) -> Result<EditsForm, String> {
    let format = effect::file_format(path)?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let form = edits_from_str(&text, format).map_err(|e| format!("{}: {e}", path.display()))?;
    if form.edits.is_empty() {
        return Err(format!("{}: `edits` is empty, so the replacement changes nothing", path.display()));
    }
    Ok(form)
}

fn index_of(what: &str, i: usize, len: usize, noun: &str) -> Result<usize, String> {
    if i < len {
        Ok(i)
    } else {
        Err(format!("{what}: {noun} {i} does not exist; the effect has {len}"))
    }
}

fn insert_at(what: &str, at: usize, len: usize, noun: &str) -> Result<usize, String> {
    if at <= len {
        Ok(at)
    } else {
        Err(format!("{what}: `at` {at} is past the {len} {noun}(s) (at most {len}, which appends)"))
    }
}

/// Apply edits in order to an effect, then check it against the writer's rules. Errors name the
/// edit (`edits[i] (op)`) and the node it addresses.
pub fn apply_edits(fx: &mut EffectContainer, edits: &[Edit]) -> Result<(), String> {
    for (i, e) in edits.iter().enumerate() {
        let what = format!("edits[{i}] ({})", e.op());
        apply_edit(fx, e, &what)?;
    }
    fx.validate().map_err(|e| format!("after the edits, the effect breaks a writer rule: {e}"))
}

fn apply_edit(fx: &mut EffectContainer, e: &Edit, what: &str) -> Result<(), String> {
    let emitters = fx.emitters.len();
    let forces = fx.forces.len();
    let shapes = fx.shapes.len();
    match e {
        Edit::Attribute { emitter, attribute, value, curve, options } => {
            let em = &mut fx.emitters[index_of(what, *emitter, emitters, "emitter")?];
            let defs = effect::particle_defs();
            let p = effect::position_of(what, &defs, attribute)?;
            let a = &mut em.particle.attributes[p];
            *a = effect::edit_attr(what, defs[p], a, value.as_ref(), curve.as_ref(), options.as_deref())?;
        }
        Edit::Channel { emitter, channel, value, curve, options } => {
            let em = &mut fx.emitters[index_of(what, *emitter, emitters, "emitter")?];
            let defs = effect::channel_defs();
            let p = effect::position_of(what, &defs, channel)?;
            let a = &mut em.channels[p];
            *a = effect::edit_attr(what, defs[p], a, value.as_ref(), curve.as_ref(), options.as_deref())?;
        }
        Edit::ColourRgb { emitter, rgb } => {
            let em = &mut fx.emitters[index_of(what, *emitter, emitters, "emitter")?];
            for k in em.particle.colr.keys.iter_mut() {
                k.colour = [rgb[0], rgb[1], rgb[2], k.colour[3]];
            }
        }
        Edit::ColourKeys { emitter, keys } => {
            let em = &mut fx.emitters[index_of(what, *emitter, emitters, "emitter")?];
            em.particle.colr = effect::lower_colour(what, keys)?;
        }
        Edit::Frames { emitter, frames } => {
            let em = &mut fx.emitters[index_of(what, *emitter, emitters, "emitter")?];
            em.particle.text = effect::lower_frames(what, frames)?;
        }
        Edit::Transform { emitter, transform } => {
            let em = &mut fx.emitters[index_of(what, *emitter, emitters, "emitter")?];
            em.transform = effect::lower_transform(what, transform)?;
        }
        Edit::Flags { emitter, flags } => {
            fx.emitters[index_of(what, *emitter, emitters, "emitter")?].particle.flags = *flags;
        }
        Edit::Geom { emitter, geom } => {
            fx.emitters[index_of(what, *emitter, emitters, "emitter")?].geom = effect::lower_geom(what, geom)?;
        }
        Edit::ForceParams { force, params } => {
            let f = &mut fx.forces[index_of(what, *force, forces, "force")?];
            let kind = params.lower(what)?;
            if effect::force_defs(&kind).iter().map(|d| d.hash).ne(f.attributes.iter().map(|a| a.hash)) {
                return Err(format!(
                    "{what}: a {} force takes other attributes than this force carries; change the kind \
                     with remove_force and add_force",
                    mercs2_formats::fxdict::attribute_name(kind.hash()).unwrap_or("new")
                ));
            }
            f.kind = kind;
        }
        Edit::ForceAttribute { force, attribute, value, curve, options } => {
            let f = &mut fx.forces[index_of(what, *force, forces, "force")?];
            let defs = effect::force_defs(&f.kind);
            let p = effect::position_of(what, &defs, attribute)?;
            let a = &mut f.attributes[p];
            *a = effect::edit_attr(what, defs[p], a, value.as_ref(), curve.as_ref(), options.as_deref())?;
        }
        Edit::Shape { shape, records } => {
            let s = index_of(what, *shape, shapes, "shape")?;
            fx.shapes[s] = effect::lower_shape(what, records)?;
        }
        Edit::AddEmitter { at, emitter } => {
            let at = insert_at(what, *at, emitters, "emitter")?;
            fx.emitters.insert(at, emitter.lower(what)?);
        }
        Edit::RemoveEmitter { emitter } => {
            fx.emitters.remove(index_of(what, *emitter, emitters, "emitter")?);
        }
        Edit::AddForce { at, force } => {
            let at = insert_at(what, *at, forces, "force")?;
            fx.forces.insert(at, force.lower(what)?);
        }
        Edit::RemoveForce { force } => {
            fx.forces.remove(index_of(what, *force, forces, "force")?);
        }
        Edit::AddShape { at, records } => {
            let at = insert_at(what, *at, shapes, "shape")?;
            if shapes >= u16::MAX as usize {
                return Err(format!("{what}: a GEOM indexes shapes with a u16; {shapes} is the most"));
            }
            fx.shapes.insert(at, effect::lower_shape(what, records)?);
            for em in fx.emitters.iter_mut() {
                if let Some(g) = em.geom.as_mut() {
                    if g.shape_index as usize >= at {
                        g.shape_index += 1;
                    }
                }
            }
        }
        Edit::RemoveShape { shape } => {
            let s = index_of(what, *shape, shapes, "shape")?;
            let users: Vec<usize> = fx
                .emitters
                .iter()
                .enumerate()
                .filter(|(_, em)| em.geom.is_some_and(|g| g.shape_index as usize == s))
                .map(|(i, _)| i)
                .collect();
            if !users.is_empty() {
                return Err(format!(
                    "{what}: shape {s} is named by the GEOM of emitter(s) {users:?}; change or remove \
                     those first"
                ));
            }
            fx.shapes.remove(s);
            for em in fx.emitters.iter_mut() {
                if let Some(g) = em.geom.as_mut() {
                    if g.shape_index as usize > s {
                        g.shape_index -= 1;
                    }
                }
            }
        }
    }
    Ok(())
}

/// The Shipment names a manifest requires (`load.requires`, the Shipment forms).
pub fn required_shipments(manifest: &Manifest) -> Vec<&str> {
    manifest
        .load
        .requires
        .iter()
        .filter_map(|r| match r {
            Requirement::Shipment(name) => Some(name.as_str()),
            Requirement::ShipmentRange(r) => Some(r.shipment.as_str()),
            Requirement::Capability(_) | Requirement::Compatible(_) => None,
        })
        .collect()
}

/// Whether a manifest has an `add_fx` or a `replace_fx`.
pub fn has_fx(manifest: &Manifest) -> bool {
    manifest
        .contributions
        .iter()
        .any(|c| matches!(c, Contribution::AddFx { .. } | Contribution::ReplaceFx { .. }))
}

/// Whether a contribution is a `replace_texture` of the `vfx` atlas: its target resolves to
/// [`sprite::VFX_ATLAS`] ([`crate::manifest::asset_hash`], so the name `vfx` and `0x89E211AF` both do).
pub fn repaints_atlas(c: &Contribution) -> bool {
    matches!(c, Contribution::ReplaceTexture { target, .. } if crate::manifest::asset_hash(target) == sprite::VFX_ATLAS)
}

/// Whether a manifest has an `add_fx_sprite`.
pub fn has_sprites(manifest: &Manifest) -> bool {
    manifest.contributions.iter().any(|c| matches!(c, Contribution::AddFxSprite { .. }))
}

/// Whether a manifest has a contribution [`merge`] applies: an `add_fx`, a `replace_fx`, an
/// `add_fx_sprite` or a `replace_texture` of the `vfx` atlas.
pub fn merges_fx(manifest: &Manifest) -> bool {
    has_fx(manifest) || has_sprites(manifest) || manifest.contributions.iter().any(repaints_atlas)
}

/// Whether a manifest has an `add_fx`.
pub fn adds_templates(manifest: &Manifest) -> bool {
    manifest.contributions.iter().any(|c| matches!(c, Contribution::AddFx { .. }))
}

/// The template keys `name` is registered under: the keys of every `Name` record whose name hashes
/// like it and whose key carries the template bit.
pub fn template_keys(we: &WorldEntity, name: &str) -> Result<Vec<u32>, String> {
    let h = pandemic_hash_m2(name);
    let mut keys: Vec<u32> = we
        .names()?
        .into_iter()
        .filter(|(n, _)| pandemic_hash_m2(n) == h)
        .flat_map(|(_, k)| k.iter().copied().filter(|k| k & TEMPLATE_KEY_BIT != 0).collect::<Vec<_>>())
        .collect();
    keys.sort_unstable();
    keys.dedup();
    Ok(keys)
}

/// The effect a template starts: the `name` field of its one `RedEffectComponent` record.
pub fn template_effect(we: &WorldEntity, key: u32) -> Result<u32, String> {
    let mut found = Vec::new();
    for c in we.groups_of(RED_EFFECT_CLASS) {
        let at = c
            .schema
            .fields
            .iter()
            .position(|f| f.name_hash == RED_EFFECT_NAME_FIELD)
            .ok_or_else(|| format!("{RED_EFFECT_CLASS} has no `name` field (0x{RED_EFFECT_NAME_FIELD:08X})"))?;
        for r in c.records.iter().filter(|r| r.keys.contains(&key)) {
            match c.decode(&r.payload)?.get(at) {
                Some(Value::Field(FieldValue::U32(h))) => found.push(*h),
                other => return Err(format!("{RED_EFFECT_CLASS}.name of 0x{key:08X} reads {other:?}")),
            }
        }
    }
    match found.as_slice() {
        [h] => Ok(*h),
        _ => Err(format!(
            "template 0x{key:08X} has {} {RED_EFFECT_CLASS} records; a target names a template with \
             exactly one, the one that names its effect",
            found.len()
        )),
    }
}

/// The effect a lowered template declaration names, when it declares exactly one
/// `RedEffectComponent`.
fn declared_effect(we: &WorldEntity, decl: &mercs2_formats::worldentity::TemplateDecl) -> Result<u32, String> {
    let reds: Vec<_> = decl.components.iter().filter(|c| c.class == RED_EFFECT_CLASS).collect();
    let [red] = reds.as_slice() else {
        return Err(format!("the template declares {} {RED_EFFECT_CLASS} records; it needs exactly one", reds.len()));
    };
    let group = &we.components[we.append_group(RED_EFFECT_CLASS)?];
    let at = group
        .schema
        .fields
        .iter()
        .position(|f| f.name_hash == RED_EFFECT_NAME_FIELD)
        .ok_or_else(|| format!("{RED_EFFECT_CLASS} has no `name` field"))?;
    match red.values.get(at) {
        Some(Value::Field(FieldValue::U32(h))) => Ok(*h),
        other => Err(format!("{RED_EFFECT_CLASS}.name reads {other:?}")),
    }
}

/// The index of the worldentity `0x50075B3B` among a block's entries, found by its name hash and
/// type hash.
pub fn worldentity_entry(block: &ScriptsBlock) -> Result<usize, String> {
    let at: Vec<usize> = block
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name_hash == RETAIL_WORLDENTITY_NAME_HASH && e.type_hash == WORLDENTITY_TYPE_HASH)
        .map(|(i, _)| i)
        .collect();
    match at.as_slice() {
        [i] => Ok(*i),
        _ => Err(format!(
            "the resident block carries {} worldentity 0x{RETAIL_WORLDENTITY_NAME_HASH:08X} entries; it has one",
            at.len()
        )),
    }
}

/// The `fxdict` asset: the one record table a `TEXT` frame is looked up in.
pub const FXDICT_NAME_HASH: u32 = 0x86BF_6C5B;

/// The index of the one entry of `name_hash` and `type_hash` among a block's entries.
fn entry_of(block: &ScriptsBlock, name_hash: u32, type_hash: u32, what: &str) -> Result<usize, String> {
    let at: Vec<usize> = block
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name_hash == name_hash && e.type_hash == type_hash)
        .map(|(i, _)| i)
        .collect();
    match at.as_slice() {
        [i] => Ok(*i),
        _ => Err(format!("the resident block carries {} {what} 0x{name_hash:08X} entries; it has one", at.len())),
    }
}

/// The index of the `fxdict` among a block's entries (the resident block carries it).
pub fn fxdict_entry(block: &ScriptsBlock) -> Result<usize, String> {
    entry_of(block, FXDICT_NAME_HASH, mercs2_formats::types::TYPE_HASH_FX_DICTIONARY, "fxdict")
}

/// The index of the `vfx` atlas among a block's entries (the resident block carries it).
pub fn atlas_entry(block: &ScriptsBlock) -> Result<usize, String> {
    entry_of(block, sprite::VFX_ATLAS, mercs2_formats::types::TYPE_HASH_TEXTURE, "vfx atlas")
}

/// The `fxdict` records and the `vfx` atlas of a block (the resident block carries both).
pub fn sprites_of(block: &ScriptsBlock) -> Result<(Vec<FxRect>, Atlas), String> {
    let records = mercs2_formats::fxdict::parse_fxdict_container(&block.entries[fxdict_entry(block)?].bytes)
        .map_err(|e| format!("the fxdict: {e}"))?;
    let atlas = Atlas::parse(&block.entries[atlas_entry(block)?].bytes)?;
    Ok((records, atlas))
}

/// The atlas a set's sprites are drawn into, and whether a Shipment of the set repaints it: the
/// set's one `replace_texture` of `vfx` ([`repaints_atlas`]), encoded whole over `game`, or `game`.
/// Two repaints in a set are an error naming both: one Shipment of a set repaints the atlas.
pub fn base_atlas(game: &Atlas, set: &[FxShipment<'_>]) -> Result<(Atlas, bool), String> {
    let mut repaints = Vec::new();
    for s in set {
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            if let (true, Contribution::ReplaceTexture { image, .. }) = (repaints_atlas(c), c) {
                repaints.push((s, index, image));
            }
        }
    }
    match repaints.as_slice() {
        [] => Ok((game.clone(), false)),
        [(s, index, image)] => {
            let what = format!("{} contributions[{index}] replace_texture of vfx", s.manifest.shipment.name);
            let image = sprite::read_png(&s.root.join(image)).map_err(|e| format!("{what}: {e}"))?;
            Ok((game.repaint(&image).map_err(|e| format!("{what}: {e}"))?, true))
        }
        many => Err(format!(
            "[M0207] the vfx atlas 0x{:08X} is repainted by {}; one Shipment of a set repaints it",
            sprite::VFX_ATLAS,
            many.iter()
                .map(|(s, i, _)| format!("{} contributions[{i}]", s.manifest.shipment.name))
                .collect::<Vec<_>>()
                .join(" and ")
        )),
    }
}

/// The game's effects block, with the ASET rows it publishes, its worldentity, its fxdict records
/// and its `vfx` atlas.
pub struct GameFx {
    pub effects: ScriptsBlock,
    /// `asset_hash -> (packed_block_ref, secondary_ref, type_id)`, as the game has them.
    pub effects_rows: HashMap<u32, (u32, u32, u32)>,
    pub worldentity: WorldEntity,
    pub fxdict: Vec<FxRect>,
    pub atlas: Atlas,
}

impl GameFx {
    /// Read them out of the game stack: the effects block by its path ([`EFFECTS_BLOCK`]), the
    /// worldentity, the fxdict and the atlas out of the resident block ([`crate::link::SCRIPT_BLOCKS`]).
    pub fn read(game: &mut crate::game::GameStack) -> Result<GameFx, String> {
        let (raw, effects_rows) = game
            .block_and_rows_by_path(EFFECTS_BLOCK.0)
            .ok_or_else(|| format!("the game stack has no {}", EFFECTS_BLOCK.1))?;
        let effects = ScriptsBlock::parse(&raw).map_err(|e| format!("{}: {e}", EFFECTS_BLOCK.1))?;
        let (needle, path) = crate::link::SCRIPT_BLOCKS[1];
        let (raw, _) = game.block_and_rows_by_path(needle).ok_or_else(|| format!("the game stack has no {path}"))?;
        let resident = ScriptsBlock::parse(&raw).map_err(|e| format!("{path}: {e}"))?;
        let worldentity = WorldEntity::parse(&resident.entries[worldentity_entry(&resident)?].bytes)
            .map_err(|e| format!("the worldentity in {path}: {e}"))?;
        let (fxdict, atlas) = sprites_of(&resident).map_err(|e| format!("{path}: {e}"))?;
        Ok(GameFx { effects, effects_rows, worldentity, fxdict, atlas })
    }
}

/// What the merge needs from the game.
pub struct FxBase<'a> {
    /// The game's effects block, in block order.
    pub effects: &'a [Entry],
    /// The game's worldentity.
    pub worldentity: &'a WorldEntity,
    /// The game's `fxdict` records: what a `TEXT` frame names.
    pub fxdict: &'a [FxRect],
    /// The atlas the set's sprites are drawn into ([`base_atlas`]).
    pub atlas: &'a Atlas,
    /// Whether a Shipment of the set repaints the atlas. A repainted atlas is written whether or not
    /// the set adds a sprite.
    pub repainted: bool,
}

/// One Shipment of the set.
#[derive(Clone, Copy)]
pub struct FxShipment<'a> {
    pub manifest: &'a Manifest,
    /// Where its `src/` files resolve.
    pub root: &'a Path,
}

/// Which step runs the merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `qm build` of one Shipment. The Shipments it requires are not in the set, so a target found
    /// in neither the game nor the Shipment is left for `qm link`, which has them, when the
    /// Shipment requires any; otherwise it is an error.
    Build,
    /// `qm link` of the installed set: every target must resolve.
    Link,
}

/// A finding the merge reports against one contribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub shipment: String,
    pub index: usize,
    /// The lint code: M0252–M0261 or M0304–M0307.
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {} contributions[{}]: {}", self.code, self.shipment, self.index, self.message)
    }
}

/// One `replace_fx` claimant of an effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimant {
    pub shipment: String,
    pub index: usize,
    /// How the target was written: `effect <name>` or `template <name>`.
    pub via: String,
}

/// Two or more `replace_fx` that resolve to one effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub effect: u32,
    pub claimants: Vec<Claimant>,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let who: Vec<String> = self
            .claimants
            .iter()
            .map(|c| format!("{}[{}] (through {})", c.shipment, c.index, c.via))
            .collect();
        write!(
            f,
            "effect 0x{:08X} is replaced by {} — only one Shipment may edit an effect, and no load \
             order resolves this",
            self.effect,
            who.join(", ")
        )
    }
}

/// Why a merge failed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Failure {
    pub problems: Vec<Problem>,
    pub conflicts: Vec<Conflict>,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lines: Vec<String> = self
            .problems
            .iter()
            .map(|p| p.to_string())
            .chain(self.conflicts.iter().map(|c| format!("[M0207] {c}")))
            .collect();
        write!(f, "{}", lines.join("\n"))
    }
}

/// What the merge produced.
#[derive(Clone)]
pub struct Merged {
    /// The effects block: the game's entries in block order, edited in place, then the added ones.
    pub effects: Vec<Entry>,
    /// The hashes of the added effects, in order.
    pub added: Vec<u32>,
    /// The worldentity with every added template, when the set adds one.
    pub worldentity: Option<WorldEntity>,
    /// The `fxdict` container with every sprite's record, when the set adds a sprite.
    pub fxdict: Option<Vec<u8>>,
    /// The `vfx` atlas container with every sprite drawn, when the set adds a sprite or repaints it.
    pub atlas: Option<Vec<u8>>,
    /// The free square the sprites went into, when the set adds one.
    pub square: Option<Square>,
    /// Where each sprite went.
    pub placed: Vec<Placed>,
    /// Each `replace_fx` and the effect it resolved to.
    pub resolved: Vec<(Claimant, u32)>,
    /// `replace_fx` targets left for `qm link` ([`Scope::Build`]).
    pub deferred: Vec<Claimant>,
    /// Frames left for `qm link` ([`Scope::Build`]): who gave them, and the frame.
    pub deferred_frames: Vec<(Claimant, u32)>,
    pub log: Vec<String>,
}

/// Who added an effect or a template: the Shipment's position in the set and the contribution.
#[derive(Clone, Copy)]
struct Origin {
    shipment: usize,
    index: usize,
}

struct State<'a> {
    set: &'a [FxShipment<'a>],
    scope: Scope,
    effects: Vec<Entry>,
    we: WorldEntity,
    added_effects: HashMap<u32, Origin>,
    added_templates: HashMap<u32, (Origin, String)>,
    /// Each sprite's key and who added it.
    sprites: HashMap<u32, Origin>,
    /// The record keys of the game's `fxdict`.
    game_frames: BTreeSet<u32>,
    deferred_frames: Vec<(Claimant, u32)>,
    log: Vec<String>,
    edited: BTreeMap<u32, Vec<Claimant>>,
    problems: Vec<Problem>,
}

impl<'a> State<'a> {
    fn new(base: &FxBase<'_>, set: &'a [FxShipment<'a>], scope: Scope) -> State<'a> {
        State {
            set,
            scope,
            effects: base.effects.to_vec(),
            we: base.worldentity.clone(),
            added_effects: HashMap::new(),
            added_templates: HashMap::new(),
            sprites: HashMap::new(),
            game_frames: base.fxdict.iter().map(|r| r.key).collect(),
            deferred_frames: Vec::new(),
            log: Vec::new(),
            edited: BTreeMap::new(),
            problems: Vec::new(),
        }
    }

    fn name(&self, si: usize) -> &str {
        &self.set[si].manifest.shipment.name
    }

    fn problem(&mut self, si: usize, index: usize, code: &'static str, message: String) {
        let shipment = self.name(si).to_string();
        self.problems.push(Problem { shipment, index, code, message });
    }

    /// Whether Shipment `si` may target something Shipment `origin` added: it requires it.
    fn may_use(&self, si: usize, origin: Origin) -> Result<(), String> {
        let by = self.name(origin.shipment);
        if origin.shipment != si && required_shipments(self.set[si].manifest).contains(&by) {
            return Ok(());
        }
        Err(format!(
            "it was added by {by} (contributions[{}]), which {} does not require; a Shipment targets \
             another's addition only when it lists that Shipment in `load.requires`",
            origin.index,
            self.name(si)
        ))
    }

    fn effect_at(&self, h: u32) -> Option<usize> {
        self.effects.iter().position(|e| e.name_hash == h && e.type_hash == TYPE_HASH_EFFECT)
    }

    /// Resolve a target to an effect hash. `Ok(None)`: not found, and left for `qm link`.
    fn resolve(&self, si: usize, target: &FxTarget) -> Result<Option<u32>, String> {
        let deferrable = self.scope == Scope::Build && !required_shipments(self.set[si].manifest).is_empty();
        let effect = |h: u32, what: String| -> Result<Option<u32>, String> {
            match self.effect_at(h) {
                Some(_) => {
                    if let Some(&o) = self.added_effects.get(&h) {
                        self.may_use(si, o).map_err(|e| format!("{what}: {e}"))?;
                    }
                    Ok(Some(h))
                }
                None if deferrable => Ok(None),
                None => Err(format!(
                    "{what} is not an effect in the game{}",
                    if self.scope == Scope::Link { " or in a Shipment this one requires" } else { "" }
                )),
            }
        };
        match target {
            FxTarget::Effect { effect: name } => {
                let h = effect::name_hash(name)?;
                effect(h, format!("effect {name:?} (0x{h:08X})"))
            }
            FxTarget::Template { template: name } => {
                let keys = template_keys(&self.we, name)?;
                let key = match keys.as_slice() {
                    [] if deferrable => return Ok(None),
                    [] => {
                        return Err(format!(
                            "no template is named {name:?} in the game{}",
                            if self.scope == Scope::Link { " or in a Shipment this one requires" } else { "" }
                        ))
                    }
                    [k] => *k,
                    many => return Err(format!("template {name:?} is registered under {} keys: {many:08X?}", many.len())),
                };
                if let Some((o, _)) = self.added_templates.get(&key) {
                    self.may_use(si, *o).map_err(|e| format!("template {name:?}: {e}"))?;
                }
                let h = template_effect(&self.we, key).map_err(|e| format!("template {name:?}: {e}"))?;
                effect(h, format!("the effect 0x{h:08X} template {name:?} (0x{key:08X}) names"))
            }
        }
    }

    /// M0259: every frame the Shipment gives an effect (one `before` did not have) is a record of
    /// the game's `fxdict`, a sprite of the Shipment, or a sprite of a Shipment it requires. In
    /// [`Scope::Build`], a Shipment that requires others leaves the frames found in none of these to
    /// `qm link`.
    fn check_frames(&mut self, si: usize, index: usize, fx: &EffectContainer, before: &BTreeSet<u32>) {
        let mut missing: Vec<u32> = fx
            .emitters
            .iter()
            .flat_map(|e| e.particle.text.frames.iter().copied())
            .filter(|h| !before.contains(h) && !self.game_frames.contains(h))
            .filter(|h| self.sprites.get(h).is_none_or(|&o| o.shipment != si && self.may_use(si, o).is_err()))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        if missing.is_empty() {
            return;
        }
        let requires = !required_shipments(self.set[si].manifest).is_empty();
        if self.scope == Scope::Build && requires {
            let claimant = Claimant { shipment: self.name(si).to_string(), index, via: "frame".into() };
            for h in missing {
                self.log.push(format!(
                    "{} contributions[{index}] frame 0x{h:08X}: not in the game's fxdict or this Shipment's \
                     sprites; resolved by qm link against the Shipments it requires",
                    self.name(si)
                ));
                self.deferred_frames.push((claimant.clone(), h));
            }
            return;
        }
        let listed: Vec<String> = missing
            .iter()
            .map(|h| match self.sprites.get(h) {
                Some(&o) => format!("0x{h:08X} ({})", self.may_use(si, o).unwrap_err()),
                None => format!("0x{h:08X}"),
            })
            .collect();
        self.problem(
            si,
            index,
            "M0259",
            format!(
                "TEXT frame(s) {} are neither records of the game's fxdict (0x{FXDICT_NAME_HASH:08X}) nor \
                 sprites of this Shipment or of one it requires; the loader resolves each frame in the \
                 fxdict (FUN_004911a0 -> FUN_00491510), and a frame it does not find draws the whole \
                 vfx atlas",
                listed.join(", ")
            ),
        );
    }

    /// Every `add_fx_sprite` of the set, packed into `base.atlas` ([`crate::sprite::pack`]). `None`
    /// when the set adds no sprite that reads.
    fn sprites(&mut self, base: &FxBase<'_>) -> Option<sprite::Packed> {
        let mut images: Vec<(usize, usize, String, u32, sprite::Image)> = Vec::new();
        let set = self.set;
        for (si, s) in set.iter().enumerate() {
            for (index, c) in s.manifest.contributions.iter().enumerate() {
                let Contribution::AddFxSprite { name, image } = c else { continue };
                if let Some(m) = sprite::name_refusal(name) {
                    self.problem(si, index, "M0305", m);
                    continue;
                }
                let key = pandemic_hash_m2(name);
                if self.game_frames.contains(&key) {
                    self.problem(
                        si,
                        index,
                        "M0306",
                        format!(
                            "sprite {name:?} hashes to 0x{key:08X}, a record of the game's fxdict \
                             (0x{FXDICT_NAME_HASH:08X}); a frame names one record, so rename the sprite"
                        ),
                    );
                    continue;
                }
                if let Some(&o) = self.sprites.get(&key) {
                    let by = self.name(o.shipment).to_string();
                    self.problem(
                        si,
                        index,
                        "M0306",
                        format!("sprite {name:?} (0x{key:08X}) is already added by {by} contributions[{}]", o.index),
                    );
                    continue;
                }
                self.sprites.insert(key, Origin { shipment: si, index });
                match sprite::read_sprite(&s.root.join(image)) {
                    Ok(img) => images.push((si, index, format!("{} {name}", s.manifest.shipment.name), key, img)),
                    Err(e) => self.problem(si, index, "M0304", e),
                }
            }
        }
        if images.is_empty() {
            return None;
        }
        let wanted: Vec<sprite::Sprite<'_>> =
            images.iter().map(|(_, _, label, key, img)| sprite::Sprite { key: *key, label: label.clone(), image: img }).collect();
        match sprite::pack(base.atlas, base.fxdict, &wanted) {
            Ok(packed) => {
                for p in &packed.placed {
                    let (_, _, label, _, _) = images.iter().find(|i| i.3 == p.key).expect("a placed sprite was wanted");
                    self.log.push(format!(
                        "sprite {label} 0x{:08X}: {}x{} at ({}, {}) in the free square {}",
                        p.key, p.width, p.height, p.x, p.y, packed.square
                    ));
                }
                Some(packed)
            }
            Err(e) => {
                let message = e.to_string();
                for (si, index, _, _, _) in &images {
                    self.problem(*si, *index, "M0307", message.clone());
                }
                None
            }
        }
    }
}

/// Every frame of an effect.
fn frames_of(fx: &EffectContainer) -> BTreeSet<u32> {
    fx.emitters.iter().flat_map(|e| e.particle.text.frames.iter().copied()).collect()
}

/// Apply a set of Shipments' `replace_fx` and `add_fx` to the game's effects block and worldentity,
/// Shipment by Shipment in `set` order (module docs). Every problem is collected; any problem or
/// conflict fails the merge.
pub fn merge(base: &FxBase<'_>, set: &[FxShipment<'_>], scope: Scope) -> Result<Merged, Failure> {
    let mut st = State::new(base, set, scope);
    let mut added = Vec::new();
    let mut resolved = Vec::new();
    let mut deferred = Vec::new();
    let mut log = Vec::new();
    let mut templates = false;

    // 0. Sprites, every Shipment's, into one atlas and one fxdict.
    let packed = st.sprites(base);

    for (si, s) in set.iter().enumerate() {
        let name = s.manifest.shipment.name.clone();
        // 1. Replacements, in contribution order.
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let Contribution::ReplaceFx { target, edits } = c else { continue };
            let via = target.to_string();
            let claimant = Claimant { shipment: name.clone(), index, via: via.clone() };
            let h = match st.resolve(si, target) {
                Ok(Some(h)) => h,
                Ok(None) => {
                    log.push(format!(
                        "{name} contributions[{index}] replace_fx {via}: not in the game; resolved by \
                         qm link against the Shipments it requires"
                    ));
                    deferred.push(claimant);
                    continue;
                }
                Err(e) => {
                    st.problem(si, index, "M0260", format!("replace_fx {via}: {e}"));
                    continue;
                }
            };
            let entry = st.edited.entry(h).or_default();
            entry.push(claimant.clone());
            if entry.len() > 1 {
                continue;
            }
            resolved.push((claimant, h));
            let form = match read_edits(&s.root.join(edits)) {
                Ok(f) => f,
                Err(e) => {
                    st.problem(si, index, "M0254", e);
                    continue;
                }
            };
            let at = st.effect_at(h).expect("a resolved effect is in the block");
            let mut fx = match parse_effect_container(&st.effects[at].bytes) {
                Ok(fx) => fx,
                Err(e) => {
                    st.problem(si, index, "M0261", format!("effect 0x{h:08X} does not parse: {e}"));
                    continue;
                }
            };
            let before = frames_of(&fx);
            if let Err(e) = apply_edits(&mut fx, &form.edits) {
                st.problem(si, index, "M0261", format!("replace_fx {via} (0x{h:08X}): {e}"));
                continue;
            }
            st.check_frames(si, index, &fx, &before);
            match write_effect_container(&fx) {
                Ok(bytes) => {
                    log.push(format!(
                        "{name} contributions[{index}] replace_fx {via} -> effect 0x{h:08X}: {} edit(s), \
                         {} -> {} bytes",
                        form.edits.len(),
                        st.effects[at].bytes.len(),
                        bytes.len()
                    ));
                    st.effects[at].bytes = bytes;
                }
                Err(e) => st.problem(si, index, "M0261", format!("replace_fx {via}: {e}")),
            }
        }

        // 2. Additions, in contribution order.
        let own: Vec<u32> = s
            .manifest
            .contributions
            .iter()
            .filter_map(|c| match c {
                Contribution::AddFx { name, .. } => Some(pandemic_hash_m2(name)),
                _ => None,
            })
            .collect();
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let Contribution::AddFx { name: fx_name, effect: effect_path, template } = c else { continue };
            templates = true;
            let h = pandemic_hash_m2(fx_name);
            if let Some(o) = st.added_effects.get(&h).copied() {
                let by = st.name(o.shipment).to_string();
                st.problem(
                    si,
                    index,
                    "M0258",
                    format!("add_fx {fx_name:?} (0x{h:08X}) is already added by {by} contributions[{}]", o.index),
                );
            } else if st.effect_at(h).is_some() {
                st.problem(
                    si,
                    index,
                    "M0258",
                    format!(
                        "add_fx {fx_name:?} (0x{h:08X}) is an effect the game already has; to change it, \
                         use replace_fx"
                    ),
                );
            } else {
                match effect::read(&s.root.join(effect_path)).and_then(|f| f.lower()) {
                    Ok(fx) => {
                        st.check_frames(si, index, &fx, &BTreeSet::new());
                        match write_effect_container(&fx) {
                            Ok(bytes) => {
                                log.push(format!(
                                    "{name} contributions[{index}] add_fx {fx_name} 0x{h:08X}: {} emitter(s), {} bytes",
                                    fx.emitters.len(),
                                    bytes.len()
                                ));
                                st.effects.push(Entry {
                                    name_hash: h,
                                    type_hash: TYPE_HASH_EFFECT,
                                    field_c: EFFECT_FIELD_C,
                                    bytes,
                                });
                                st.added_effects.insert(h, Origin { shipment: si, index });
                                added.push(h);
                            }
                            Err(e) => st.problem(si, index, "M0252", format!("add_fx {fx_name:?}: {e}")),
                        }
                    }
                    Err(e) => st.problem(si, index, "M0252", format!("add_fx {fx_name:?}: {e}")),
                }
            }

            // The template, under the key its name derives.
            let key = derived_template_key(&template.name);
            let nh = pandemic_hash_m2(&template.name);
            let mut clash = None;
            for (&k, (o, n)) in &st.added_templates {
                if k == key || pandemic_hash_m2(n) == nh {
                    clash = Some(format!(
                        "template {:?} (key 0x{key:08X}, name hash 0x{nh:08X}) collides with template {n:?} \
                         (key 0x{k:08X}) added by {} contributions[{}]",
                        template.name,
                        st.name(o.shipment),
                        o.index
                    ));
                }
            }
            if let Some(m) = clash {
                st.problem(si, index, "M0257", m);
                continue;
            }
            if st.we.all_keys().contains(&key) {
                st.problem(
                    si,
                    index,
                    "M0257",
                    format!(
                        "template {:?} derives key 0x{key:08X}, which the game's worldentity already uses; \
                         rename the template",
                        template.name
                    ),
                );
                continue;
            }
            let decl = match template.lower(&st.we, key) {
                Ok(d) => d,
                Err(e) => {
                    st.problem(si, index, "M0256", format!("template {:?}: {e}", template.name));
                    continue;
                }
            };
            match declared_effect(&st.we, &decl) {
                Ok(e) if st.effect_at(e).is_some() && (st.added_effects.get(&e).is_none_or(|o| o.shipment == si)) => {}
                Ok(e) if own.contains(&e) => {}
                Ok(e) => st.problem(
                    si,
                    index,
                    "M0259",
                    format!(
                        "template {:?} names effect 0x{e:08X} in its {RED_EFFECT_CLASS}, which is neither \
                         an effect the game ships nor one this Shipment adds",
                        template.name
                    ),
                ),
                Err(e) => st.problem(si, index, "M0255", format!("template {:?}: {e}", template.name)),
            }
            match st.we.append_template(&decl) {
                Ok(()) => {
                    log.push(format!(
                        "{name} contributions[{index}] add_fx template {} -> key 0x{key:08X}, {} record(s)",
                        template.name,
                        decl.components.len()
                    ));
                    st.added_templates.insert(key, (Origin { shipment: si, index }, template.name.clone()));
                }
                Err(e) => st.problem(si, index, "M0257", format!("template {:?}: {e}", template.name)),
            }
        }
    }

    let conflicts: Vec<Conflict> = st
        .edited
        .iter()
        .filter(|(_, c)| c.len() > 1)
        .map(|(&effect, claimants)| Conflict { effect, claimants: claimants.clone() })
        .collect();
    if !st.problems.is_empty() || !conflicts.is_empty() {
        return Err(Failure { problems: st.problems, conflicts });
    }
    // `Atlas::parse` checked that the container takes a body of this length back.
    let (fxdict, atlas, square, placed) = match packed {
        Some(p) => {
            let atlas = p.atlas.container().expect("the atlas container takes back a body of its own length");
            (Some(write_fxdict_container(&p.records)), Some(atlas), Some(p.square), p.placed)
        }
        None if base.repainted => {
            (None, Some(base.atlas.container().expect("the atlas container takes back a body of its own length")), None, Vec::new())
        }
        None => (None, None, None, Vec::new()),
    };
    let mut all_log = std::mem::take(&mut st.log);
    all_log.extend(log);
    Ok(Merged {
        effects: st.effects,
        added,
        worldentity: templates.then_some(st.we),
        fxdict,
        atlas,
        square,
        placed,
        resolved,
        deferred,
        deferred_frames: st.deferred_frames,
        log: all_log,
    })
}

/// The `replace_fx` conflicts of a set: two that resolve to one effect, directly or through a
/// template. Reads no file: only the targets are resolved, against the game plus each Shipment's
/// additions in `set` order. Resolution problems are not conflicts and are left to [`merge`].
pub fn conflicts(base: &FxBase<'_>, set: &[FxShipment<'_>]) -> Vec<Conflict> {
    let mut st = State::new(base, set, Scope::Link);
    for (si, s) in set.iter().enumerate() {
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let Contribution::ReplaceFx { target, .. } = c else { continue };
            if let Ok(Some(h)) = st.resolve(si, target) {
                st.edited.entry(h).or_default().push(Claimant {
                    shipment: s.manifest.shipment.name.clone(),
                    index,
                    via: target.to_string(),
                });
            }
        }
        // Additions are recorded by name only: a target resolves against what they add, and the
        // effect bytes are not needed to resolve one.
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let Contribution::AddFx { name, template, .. } = c else { continue };
            let h = pandemic_hash_m2(name);
            if st.effect_at(h).is_none() {
                st.effects.push(Entry { name_hash: h, type_hash: TYPE_HASH_EFFECT, field_c: EFFECT_FIELD_C, bytes: Vec::new() });
                st.added_effects.insert(h, Origin { shipment: si, index });
            }
            let key = derived_template_key(&template.name);
            if let Ok(decl) = template.lower(&st.we, key) {
                if st.we.append_template(&decl).is_ok() {
                    st.added_templates.insert(key, (Origin { shipment: si, index }, template.name.clone()));
                }
            }
        }
    }
    st.edited
        .into_iter()
        .filter(|(_, c)| c.len() > 1)
        .map(|(effect, claimants)| Conflict { effect, claimants })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{EffectForm, ForceForm};
    use mercs2_formats::fxdict::{
        AnimKey, Atrb, AtrbValue, Colr, Emitter, EmitterGeom, EmitterShape, Force, ForceKind, ParticleType, Text,
        ValueKind, SHAPE_RECORD_FLOATS,
    };
    use mercs2_formats::ucfx::{write_ucfx_tree, UcfxNode};
    use std::path::PathBuf;

    const TEXTURE: u32 = 7;

    fn attrs(defs: &[&mercs2_formats::fxdict::AttrDef]) -> Vec<Atrb> {
        defs.iter()
            .map(|d| match d.kind {
                ValueKind::F32 => Atrb::f32(d.hash, 0.0),
                ValueKind::U32 => Atrb::u32(d.hash, 1),
            })
            .collect()
    }

    /// One shape, one emitter whose GEOM names it, one gravity force.
    fn effect() -> EffectContainer {
        let gravity = ForceKind::Gravity { magnitude: 9.8, direction: [0.0, -1.0, 0.0] };
        EffectContainer {
            shapes: vec![EmitterShape { records: vec![[0.0; SHAPE_RECORD_FLOATS]] }],
            emitters: vec![Emitter {
                transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
                channels: attrs(&effect::channel_defs()),
                geom: Some(EmitterGeom { shape_index: 0, word_00: 16 }),
                particle: ParticleType {
                    flags: 1,
                    attributes: attrs(&effect::particle_defs()),
                    colr: Colr::from_fn(|t| ([10, 20, 30, (255.0 * (1.0 - t)) as u8], 0xBC00)),
                    text: Text { frames: vec![TEXTURE] },
                },
            }],
            forces: vec![Force { attributes: attrs(&effect::force_defs(&gravity)), kind: gravity }],
        }
    }

    fn edits(yaml: &str) -> Vec<Edit> {
        edits_from_str(&format!("edits:\n{yaml}"), Format::Yaml).unwrap_or_else(|e| panic!("{e}\n{yaml}")).edits
    }

    fn applied(yaml: &str) -> EffectContainer {
        let mut fx = effect();
        apply_edits(&mut fx, &edits(yaml)).unwrap_or_else(|e| panic!("{e}"));
        fx
    }

    fn refused(yaml: &str) -> String {
        let mut fx = effect();
        apply_edits(&mut fx, &edits(yaml)).unwrap_err()
    }

    fn at<'a>(attrs: &'a [Atrb], name: &str) -> &'a Atrb {
        attrs.iter().find(|a| a.hash == pandemic_hash_m2(name)).unwrap()
    }

    #[test]
    fn attribute_and_channel_edits_keep_what_they_do_not_name() {
        let fx = applied(
            "  - { op: attribute, emitter: 0, attribute: size, curve: [[0, 1], [100, 2]], options: [bit7] }\n\
             \x20 - { op: attribute, emitter: 0, attribute: size, value: 2.5 }\n\
             \x20 - { op: channel, emitter: 0, channel: posx, value: 3, curve: [[0, 0]], options: [resample] }\n\
             \x20 - { op: attribute, emitter: 0, attribute: \"0x497A1895\", value: 1.5 }\n",
        );
        let p = &fx.emitters[0].particle.attributes;
        let size = at(p, "size");
        assert_eq!(size.value, AtrbValue::F32(2.5));
        assert_eq!(size.curve.as_deref(), Some(&[AnimKey { time: 0.0, value: 1.0 }, AnimKey { time: 100.0, value: 2.0 }][..]));
        assert_eq!(size.flags, atrb_bits(true, true, mercs2_formats::fxdict::atrb_flag::BIT7));
        assert_eq!(p.iter().find(|a| a.hash == 0x497A1895).unwrap().value, AtrbValue::F32(1.5));
        let posx = &fx.emitters[0].channels[0];
        assert_eq!(posx.value, AtrbValue::F32(3.0));
        assert_eq!(posx.flags & mercs2_formats::fxdict::atrb_flag::RESAMPLE, mercs2_formats::fxdict::atrb_flag::RESAMPLE);

        // `curve: none` removes the curve and keeps the value and options.
        let fx2 = applied(
            "  - { op: attribute, emitter: 0, attribute: size, curve: [[0, 1]], options: [bit9] }\n\
             \x20 - { op: attribute, emitter: 0, attribute: size, curve: none }\n",
        );
        let size = at(&fx2.emitters[0].particle.attributes, "size");
        assert_eq!(size.curve, None);
        assert_eq!(size.flags, atrb_bits(true, false, mercs2_formats::fxdict::atrb_flag::BIT9));
    }

    fn atrb_bits(float: bool, curve: bool, options: u32) -> u32 {
        use mercs2_formats::fxdict::atrb_flag;
        options | if float { atrb_flag::FLOAT } else { 0 } | if curve { atrb_flag::CURVE } else { 0 }
    }

    #[test]
    fn colour_rgb_keeps_each_keys_alpha_and_half() {
        let before = effect();
        let fx = applied("  - { op: colour_rgb, emitter: 0, rgb: [255, 0, 255] }\n");
        for (k, b) in fx.emitters[0].particle.colr.keys.iter().zip(before.emitters[0].particle.colr.keys.iter()) {
            assert_eq!(k.colour, [255, 0, 255, b.colour[3]]);
            assert_eq!(k.half_bits, b.half_bits);
        }
        let keys: Vec<String> = (0..100).map(|_| "{ rgba: [1, 2, 3, 4], half: 15360 }".to_string()).collect();
        let fx = applied(&format!("  - {{ op: colour_keys, emitter: 0, keys: [{}] }}\n", keys.join(", ")));
        assert!(fx.emitters[0].particle.colr.keys.iter().all(|k| k.colour == [1, 2, 3, 4] && k.half_bits == 0x3C00));
    }

    #[test]
    fn frames_transform_flags_geom_and_shapes_replace() {
        let fx = applied(
            "  - { op: frames, emitter: 0, frames: [\"0x00000009\", qm_disc] }\n\
             \x20 - { op: transform, emitter: 0, transform: [[2, 0, 0, 0], [0, 2, 0, 0], [0, 0, 2, 0], [0, 0, 0, 1]] }\n\
             \x20 - { op: flags, emitter: 0, flags: 3 }\n\
             \x20 - { op: geom, emitter: 0, geom: { shape: 0, word: 1 } }\n\
             \x20 - { op: shape, shape: 0, records: [[1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2]] }\n",
        );
        let e = &fx.emitters[0];
        assert_eq!(e.particle.text.frames, vec![9, pandemic_hash_m2("qm_disc")]);
        assert_eq!(e.transform[0][0], 2.0);
        assert_eq!(e.particle.flags, 3);
        assert_eq!(e.geom, Some(EmitterGeom { shape_index: 0, word_00: 1 }));
        assert_eq!(fx.shapes[0].records.len(), 2);
        let fx = applied("  - { op: geom, emitter: 0, geom: none }\n");
        assert_eq!(fx.emitters[0].geom, None);
    }

    #[test]
    fn force_edits_keep_the_kind_and_edit_attributes() {
        let fx = applied(
            "  - { op: force_params, force: 0, params: { kind: gravity, magnitude: 5, direction: [1, 0, 0] } }\n\
             \x20 - { op: force_attribute, force: 0, attribute: ampl, value: 0.5, curve: [[0, 1], [100, 0]] }\n",
        );
        assert_eq!(fx.forces[0].kind, ForceKind::Gravity { magnitude: 5.0, direction: [1.0, 0.0, 0.0] });
        assert_eq!(fx.forces[0].attributes[0].value, AtrbValue::F32(0.5));
        assert!(fx.forces[0].attributes[0].curve.is_some());
        let e = refused("  - { op: force_params, force: 0, params: { kind: drag, magnitude: 1 } }\n");
        assert!(e.contains("remove_force and add_force"), "{e}");
    }

    fn emitter_yaml() -> String {
        let form = EffectForm::express(&effect());
        let text = serde_json::to_string(&form.emitters[0]).unwrap();
        text
    }

    fn force_yaml(kind: ForceKind) -> String {
        let f = Force { attributes: attrs(&effect::force_defs(&kind)), kind };
        serde_json::to_string(&ForceForm::express(&f)).unwrap()
    }

    #[test]
    fn structural_edits_add_and_remove_emitters_forces_and_shapes() {
        let fx = applied(&format!(
            "  - {{ op: add_emitter, at: 1, emitter: {} }}\n\
             \x20 - {{ op: add_force, at: 0, force: {} }}\n",
            emitter_yaml(),
            force_yaml(ForceKind::Drag { magnitude: 0.3 })
        ));
        assert_eq!(fx.emitters.len(), 2);
        assert_eq!(fx.forces.len(), 2);
        assert!(matches!(fx.forces[0].kind, ForceKind::Drag { .. }));
        let fx2 = {
            let mut fx2 = fx.clone();
            apply_edits(&mut fx2, &edits("  - { op: remove_emitter, emitter: 0 }\n  - { op: remove_force, force: 1 }\n")).unwrap();
            fx2
        };
        assert_eq!(fx2.emitters.len(), 1);
        assert_eq!(fx2.forces.len(), 1);
        assert!(matches!(fx2.forces[0].kind, ForceKind::Drag { .. }));

        // A shape inserted before the one the GEOM names moves the GEOM up; removing the inserted
        // one moves it back. The named shape cannot be removed.
        let fx = applied("  - { op: add_shape, at: 0, records: [[3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3]] }\n");
        assert_eq!(fx.shapes.len(), 2);
        assert_eq!(fx.emitters[0].geom.unwrap().shape_index, 1);
        let e = refused(
            "  - { op: add_shape, at: 0, records: [[3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3]] }\n\
             \x20 - { op: remove_shape, shape: 1 }\n",
        );
        assert!(e.contains("named by the GEOM of emitter(s) [0]"), "{e}");
        let fx = applied(
            "  - { op: add_shape, at: 0, records: [[3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3]] }\n\
             \x20 - { op: remove_shape, shape: 0 }\n",
        );
        assert_eq!(fx.shapes.len(), 1);
        assert_eq!(fx.emitters[0].geom.unwrap().shape_index, 0);
        assert_eq!(fx.shapes[0].records[0][0], 0.0);
    }

    #[test]
    fn edits_that_address_nothing_or_break_a_writer_rule_are_refused() {
        for (yaml, want) in [
            ("  - { op: attribute, emitter: 1, attribute: size, value: 1.0 }\n", "emitter 1 does not exist"),
            ("  - { op: attribute, emitter: 0, attribute: nope, value: 1.0 }\n", "not one of its attributes"),
            ("  - { op: attribute, emitter: 0, attribute: size }\n", "none of value, curve, options"),
            ("  - { op: attribute, emitter: 0, attribute: size, value: x }\n", "cannot take"),
            ("  - { op: attribute, emitter: 0, attribute: mass, curve: [[0, 1]] }\n", "not decoded"),
            ("  - { op: force_attribute, force: 2, attribute: ampl, value: 1.0 }\n", "force 2 does not exist"),
            ("  - { op: shape, shape: 4, records: [] }\n", "shape 4 does not exist"),
            ("  - { op: remove_force, force: 1 }\n", "force 1 does not exist"),
            ("  - { op: remove_emitter, emitter: 0 }\n", "at least one emitter"),
            ("  - { op: add_shape, at: 3, records: [] }\n", "past the 1 shape"),
            ("  - { op: geom, emitter: 0, geom: { shape: 2, word: 0 } }\n", "shape index"),
            ("  - { op: frames, emitter: 0, frames: [] }\n", "at least one frame"),
            ("  - { op: flags, emitter: 0, flags: 8 }\n", "never reads"),
        ] {
            let e = refused(yaml);
            assert!(e.contains(want), "{yaml}: {e}");
        }
        assert!(edits_from_str("edits:\n  - { op: paint, emitter: 0 }\n", Format::Yaml).is_err());
        assert!(edits_from_str("edits:\n  - { op: flags, emitter: 0, flags: 1, extra: 2 }\n", Format::Yaml).is_err());
    }

    #[test]
    fn the_edits_form_reads_from_yaml_json_and_toml() {
        let form = EditsForm { edits: edits("  - { op: attribute, emitter: 0, attribute: size, value: 2.0, curve: none }\n  - { op: colour_rgb, emitter: 0, rgb: [1, 2, 3] }\n") };
        for f in [Format::Yaml, Format::Json, Format::Toml] {
            let text = edits_to_string(&form, f).unwrap();
            assert_eq!(edits_from_str(&text, f).unwrap(), form, "{f:?}\n{text}");
        }
    }

    // ---- the merge ------------------------------------------------------------------------------

    const RED_F32: u32 = 0x1111_0001;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn field(code: u32, hash: u32, off: u16) -> Vec<u8> {
        let mut e = le(&[code, hash, 0]);
        e.extend_from_slice(&off.to_le_bytes());
        e.extend_from_slice(&[0, 0]);
        e
    }

    fn comp(class: &str, version: u32, count: u32, fields: &[Vec<u8>], stride: u32, data: Vec<u8>) -> UcfxNode {
        let mut info = class.as_bytes().to_vec();
        info.push(0);
        info.extend_from_slice(&le(&[pandemic_hash_m2(class), version, count, 0]));
        let mut schm = le(&[fields.len() as u32, stride]);
        for f in fields {
            schm.extend_from_slice(f);
        }
        UcfxNode::marker(
            *b"COMP",
            vec![UcfxNode::leaf(*b"info", info), UcfxNode::leaf(*b"schm", schm), UcfxNode::leaf(*b"data", data)],
        )
    }

    /// The game: effects `fx_one` and `fx_two`; templates `tpl_one` (one RedEffectComponent naming
    /// `fx_one`), `tpl_none` (none) and `tpl_two` (two).
    fn game() -> (Vec<Entry>, WorldEntity) {
        let red = |key: u32, fx: &str| {
            let mut d = le(&[1, key, pandemic_hash_m2(fx)]);
            d.extend_from_slice(&1f32.to_le_bytes());
            d
        };
        let mut reds = red(0x8000_0002, "fx_one");
        reds.extend(red(0x8000_0004, "fx_one"));
        reds.extend(red(0x8000_0004, "fx_two"));
        let mut names = Vec::new();
        for (k, n) in [(0x8000_0002u32, "tpl_one"), (0x8000_0003, "tpl_none"), (0x8000_0004, "tpl_two")] {
            names.extend(le(&[1, k]));
            names.extend_from_slice(n.as_bytes());
            names.extend_from_slice(&[0, 1]);
        }
        let mut flgt = le(&[1, pandemic_hash_m2(RED_EFFECT_CLASS)]);
        flgt.extend_from_slice(b"RedEffectComponent\0");
        flgt.extend(le(&[0]));
        let mut flgs = le(&[2, 0x8000_0002, 1]);
        flgs.extend_from_slice(&[0; 28]);
        flgs.extend(le(&[0x8000_0004, 1]));
        flgs.extend_from_slice(&[0; 28]);
        let mut chdr = vec![0, 0, 0x33, 0];
        chdr.extend(le(&[1]));
        let we = WorldEntity::parse(&write_ucfx_tree(&[
            UcfxNode::leaf(*b"CHDR", chdr),
            UcfxNode::leaf(*b"enum", le(&[0])),
            UcfxNode::leaf(*b"UNIQ", le(&[3, 0x8000_0002, 0x8000_0003, 0x8000_0004])),
            comp(RED_EFFECT_CLASS, 0x57, 3, &[field(6, RED_EFFECT_NAME_FIELD, 0), field(7, RED_F32, 4)], 8, reds),
            comp("Name", 1, 3, &[field(8, 0x1DE5_C824, 0), field(1, 0x12AF_A0B8, 4)], 5, names),
            UcfxNode::leaf(*b"flgt", flgt),
            UcfxNode::leaf(*b"flgs", flgs),
        ]))
        .expect("the fixture parses");
        let bytes = write_effect_container(&effect()).unwrap();
        let effects = ["fx_one", "fx_two"]
            .iter()
            .map(|n| Entry { name_hash: pandemic_hash_m2(n), type_hash: TYPE_HASH_EFFECT, field_c: 0, bytes: bytes.clone() })
            .collect();
        (effects, we)
    }

    #[test]
    fn a_template_target_resolves_through_exactly_one_red_effect_component() {
        let (_, we) = game();
        assert_eq!(template_keys(&we, "tpl_one").unwrap(), vec![0x8000_0002]);
        assert_eq!(template_effect(&we, 0x8000_0002).unwrap(), pandemic_hash_m2("fx_one"));
        assert!(template_effect(&we, 0x8000_0003).unwrap_err().contains("has 0 RedEffectComponent"));
        assert!(template_effect(&we, 0x8000_0004).unwrap_err().contains("has 2 RedEffectComponent"));
        assert!(template_keys(&we, "tpl_missing").unwrap().is_empty());
    }

    struct Ship {
        root: PathBuf,
        manifest: Manifest,
    }

    /// A Shipment on disk: `contributions` (YAML at the list indent) and `files` under `src/`.
    fn ship(label: &str, name: &str, requires: &[&str], contributions: &str, files: &[(&str, String)]) -> Ship {
        let root = std::env::temp_dir().join(format!("qm_fx_{}_{label}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        for (f, text) in files {
            std::fs::write(root.join("src").join(f), text).unwrap();
        }
        let load = if requires.is_empty() { String::new() } else { format!("load: {{ requires: [{}] }}\n", requires.join(", ")) };
        let text = format!(
            "format: 2\nshipment: {{ name: {name}, version: 1.0.0, target: retail }}\n{load}contributions:\n{contributions}"
        );
        Ship { root, manifest: crate::from_str(&text, Format::Yaml).unwrap_or_else(|e| panic!("{e}\n{text}")) }
    }

    fn effect_file() -> String {
        effect::to_string(&EffectForm::express(&effect()), Format::Yaml).unwrap()
    }

    fn add_fx(fx: &str, template: &str, red: &str) -> String {
        format!(
            "  - kind: add_fx\n    name: {fx}\n    effect: src/{fx}.yaml\n    template:\n      name: {template}\n      \
             name_flag: 1\n      components:\n        RedEffectComponent: {{ name: {red}, \"0x11110001\": 2.0 }}\n"
        )
    }

    fn replace(target: &str, file: &str) -> String {
        format!("  - kind: replace_fx\n    target: {target}\n    edits: src/{file}\n")
    }

    const MAGENTA: &str = "edits:\n  - { op: colour_rgb, emitter: 0, rgb: [255, 0, 255] }\n";

    /// A transparent 64² atlas.
    fn atlas() -> Atlas {
        let px = vec![0f32; 64 * 64 * 4];
        let body = mercs2_formats::texture_encode::mip_chain(64, 64, 4, &px, mercs2_formats::texture_encode::encode_bc3);
        Atlas::parse(&mercs2_formats::texture_encode::ucfx_texture("vfx", 64, 64, b"DXT5", &body)).unwrap()
    }

    /// The game's fxdict: the frame [`TEXTURE`], a 4² rectangle at the atlas's bottom-left corner.
    fn records() -> Vec<FxRect> {
        vec![FxRect { key: TEXTURE, u: 0.0, v: 0.0, w: 0.0625, h: 0.0625 }]
    }

    fn run(ships: &[&Ship], scope: Scope) -> Result<Merged, Failure> {
        let (effects, we) = game();
        let (records, atlas) = (records(), atlas());
        let base = FxBase { effects: &effects, worldentity: &we, fxdict: &records, atlas: &atlas, repainted: false };
        let set: Vec<FxShipment<'_>> = ships.iter().map(|s| FxShipment { manifest: &s.manifest, root: &s.root }).collect();
        merge(&base, &set, scope)
    }

    fn codes(f: &Failure) -> Vec<&'static str> {
        f.problems.iter().map(|p| p.code).collect()
    }

    #[test]
    fn additions_append_in_set_order_under_derived_keys() {
        let a = ship("order", "mod-a", &[], &add_fx("fx_a", "tpl_a", "fx_a"), &[("fx_a.yaml", effect_file())]);
        let b = ship("order", "mod-b", &[], &add_fx("fx_b", "tpl_b", "fx_b"), &[("fx_b.yaml", effect_file())]);
        let ab = run(&[&a, &b], Scope::Link).unwrap();
        let ba = run(&[&b, &a], Scope::Link).unwrap();
        let (fa, fb) = (pandemic_hash_m2("fx_a"), pandemic_hash_m2("fx_b"));
        assert_eq!(ab.added, vec![fa, fb]);
        assert_eq!(ba.added, vec![fb, fa]);
        assert_eq!(ab.effects.len(), 4);
        assert_eq!(&ab.effects[2..].iter().map(|e| e.name_hash).collect::<Vec<_>>(), &[fa, fb]);
        for m in [&ab, &ba] {
            let we = m.worldentity.as_ref().unwrap();
            for t in ["tpl_a", "tpl_b"] {
                let key = derived_template_key(t);
                assert_eq!(template_keys(we, t).unwrap(), vec![key]);
                assert_eq!(template_effect(we, key).unwrap(), pandemic_hash_m2(&t.replace("tpl", "fx")));
            }
        }
    }

    #[test]
    fn a_template_name_or_key_added_twice_names_both_shipments() {
        let a = ship("dup", "mod-a", &[], &add_fx("fx_a", "tpl_same", "fx_a"), &[("fx_a.yaml", effect_file())]);
        let b = ship("dup", "mod-b", &[], &add_fx("fx_b", "tpl_same", "fx_b"), &[("fx_b.yaml", effect_file())]);
        let f = run(&[&a, &b], Scope::Link).err().unwrap();
        assert_eq!(codes(&f), vec!["M0257"]);
        let p = &f.problems[0];
        assert_eq!(p.shipment, "mod-b");
        assert!(p.message.contains("added by mod-a"), "{}", p.message);
    }

    #[test]
    fn a_game_effect_name_or_a_missing_template_effect_is_refused() {
        let s = ship("taken", "mod-a", &[], &add_fx("fx_one", "tpl_x", "fx_none"), &[("fx_one.yaml", effect_file())]);
        let f = run(&[&s], Scope::Build).err().unwrap();
        assert_eq!(codes(&f), vec!["M0258", "M0259"]);
        // A template may name a game effect.
        let s = ship("game-fx", "mod-a", &[], &add_fx("fx_new", "tpl_y", "fx_two"), &[("fx_new.yaml", effect_file())]);
        assert!(run(&[&s], Scope::Build).is_ok());
    }

    #[test]
    fn replacements_resolve_directly_or_through_a_template_and_apply() {
        let s = ship(
            "apply",
            "mod-a",
            &[],
            &(replace("{ template: tpl_one }", "m.yaml") + &replace("{ effect: fx_two }", "m.yaml")),
            &[("m.yaml", MAGENTA.into())],
        );
        let m = run(&[&s], Scope::Build).unwrap();
        assert!(m.worldentity.is_none(), "replacements add no template");
        for e in &m.effects {
            let fx = parse_effect_container(&e.bytes).unwrap();
            assert!(fx.emitters[0].particle.colr.keys.iter().all(|k| k.colour[..3] == [255, 0, 255]));
        }
        assert_eq!(m.resolved.iter().map(|(_, h)| *h).collect::<Vec<_>>(), vec![pandemic_hash_m2("fx_one"), pandemic_hash_m2("fx_two")]);
        for (t, want) in [
            ("{ template: tpl_none }", "has 0 RedEffectComponent"),
            ("{ template: tpl_two }", "has 2 RedEffectComponent"),
            ("{ template: tpl_missing }", "no template is named"),
            ("{ effect: fx_missing }", "is not an effect in the game"),
        ] {
            let s = ship("unresolved", "mod-a", &[], &replace(t, "m.yaml"), &[("m.yaml", MAGENTA.into())]);
            let f = run(&[&s], Scope::Build).err().unwrap();
            assert_eq!(codes(&f), vec!["M0260"], "{t}");
            assert!(f.problems[0].message.contains(want), "{t}: {}", f.problems[0].message);
        }
    }

    #[test]
    fn one_effect_replaced_twice_is_a_conflict_through_a_template_too() {
        let a = ship("twice", "mod-a", &[], &replace("{ effect: fx_one }", "m.yaml"), &[("m.yaml", MAGENTA.into())]);
        let b = ship("twice", "mod-b", &[], &replace("{ template: tpl_one }", "m.yaml"), &[("m.yaml", MAGENTA.into())]);
        let f = run(&[&a, &b], Scope::Link).err().unwrap();
        assert!(f.problems.is_empty());
        assert_eq!(f.conflicts.len(), 1);
        assert_eq!(f.conflicts[0].effect, pandemic_hash_m2("fx_one"));
        let (effects, we) = game();
        let atlas = atlas();
        let base = FxBase { effects: &effects, worldentity: &we, fxdict: &[], atlas: &atlas, repainted: false };
        let set = [FxShipment { manifest: &a.manifest, root: &a.root }, FxShipment { manifest: &b.manifest, root: &b.root }];
        let c = conflicts(&base, &set);
        assert_eq!(c, f.conflicts);
        assert!(c[0].to_string().contains("mod-a[0] (through effect fx_one)") && c[0].to_string().contains("mod-b[0] (through template tpl_one)"), "{}", c[0]);
    }

    #[test]
    fn another_shipments_addition_is_a_target_only_through_requires() {
        let a = ship("req", "mod-a", &[], &add_fx("fx_a", "tpl_a", "fx_a"), &[("fx_a.yaml", effect_file())]);
        let b = ship("req", "mod-b", &[], &replace("{ template: tpl_a }", "m.yaml"), &[("m.yaml", MAGENTA.into())]);
        let f = run(&[&a, &b], Scope::Link).err().unwrap();
        assert_eq!(codes(&f), vec!["M0260"]);
        assert!(f.problems[0].message.contains("does not require"), "{}", f.problems[0].message);

        let b = ship("req-ok", "mod-b", &["mod-a"], &replace("{ template: tpl_a }", "m.yaml"), &[("m.yaml", MAGENTA.into())]);
        let m = run(&[&a, &b], Scope::Link).unwrap();
        let fa = m.effects.iter().find(|e| e.name_hash == pandemic_hash_m2("fx_a")).unwrap();
        let fx = parse_effect_container(&fa.bytes).unwrap();
        assert!(fx.emitters[0].particle.colr.keys.iter().all(|k| k.colour[..3] == [255, 0, 255]));
        // Built alone, the requiring Shipment leaves the target to the link.
        let m = run(&[&b], Scope::Build).unwrap();
        assert_eq!(m.deferred.len(), 1);
        // A Shipment that requires nothing resolves every target itself.
        let c = ship("req-none", "mod-c", &[], &replace("{ template: tpl_a }", "m.yaml"), &[("m.yaml", MAGENTA.into())]);
        assert_eq!(codes(&run(&[&c], Scope::Build).err().unwrap()), vec!["M0260"]);
    }

    #[test]
    fn frames_must_be_fxdict_records() {
        let mut fx = effect();
        fx.emitters[0].particle.text.frames = vec![0x0BAD_0BAD];
        let text = effect::to_string(&EffectForm::express(&fx), Format::Yaml).unwrap();
        let s = ship("frames", "mod-a", &[], &add_fx("fx_f", "tpl_f", "fx_f"), &[("fx_f.yaml", text)]);
        let f = run(&[&s], Scope::Build).err().unwrap();
        assert_eq!(codes(&f), vec!["M0259"]);
        assert!(f.problems[0].message.contains("0x0BAD0BAD"), "{}", f.problems[0].message);
        // An edit that keeps the effect's own frames is quiet; one that adds an unknown frame is not.
        let s = ship("frames-edit", "mod-a", &[], &replace("{ effect: fx_one }", "e.yaml"), &[(
            "e.yaml",
            "edits:\n  - { op: frames, emitter: 0, frames: [\"0x00000007\", \"0x0BAD0BAD\"] }\n".into(),
        )]);
        assert_eq!(codes(&run(&[&s], Scope::Build).err().unwrap()), vec!["M0259"]);
    }
    // ---- sprites --------------------------------------------------------------------------------

    /// An 8² PNG, every texel white with alpha `a`.
    fn png(a: u8) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 8, 8);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&[255, 255, 255, a].repeat(64)).unwrap();
        out
    }

    /// `add_fx_sprite` of `name` from `src/<name>.png`.
    fn sprite(name: &str) -> String {
        format!("  - kind: add_fx_sprite\n    name: {name}\n    image: src/{name}.png\n")
    }

    /// An effect whose one frame is `frame`.
    fn framed(frame: &str) -> String {
        let mut fx = effect();
        fx.emitters[0].particle.text.frames = vec![pandemic_hash_m2(frame)];
        effect::to_string(&EffectForm::express(&fx), Format::Yaml).unwrap()
    }

    fn ship_files(label: &str, name: &str, requires: &[&str], contributions: &str, text: &[(&str, String)], pngs: &[&str]) -> Ship {
        let s = ship(label, name, requires, contributions, text);
        for p in pngs {
            std::fs::write(s.root.join("src").join(format!("{p}.png")), png(255)).unwrap();
        }
        s
    }

    #[test]
    fn a_shipments_own_sprite_is_a_frame_and_joins_the_fxdict_and_the_atlas() {
        let s = ship_files(
            "own",
            "mod-a",
            &[],
            &(sprite("qm_ring") + &add_fx("fx_a", "tpl_a", "fx_a")),
            &[("fx_a.yaml", framed("qm_ring"))],
            &["qm_ring"],
        );
        let m = run(&[&s], Scope::Build).unwrap_or_else(|f| panic!("{f}"));
        let records = mercs2_formats::fxdict::parse_fxdict_container(m.fxdict.as_ref().unwrap()).unwrap();
        let ring = pandemic_hash_m2("qm_ring");
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|r| r.key == ring));
        assert_eq!(m.placed.len(), 1);
        assert_eq!(m.square, Some(Square { x: 0, y: 0, side: 32 }));
        assert!(m.atlas.is_some());
        // Effects alone leave both untouched.
        let e = ship("own-none", "mod-b", &[], &add_fx("fx_b", "tpl_b", "fx_b"), &[("fx_b.yaml", effect_file())]);
        let m = run(&[&e], Scope::Build).unwrap();
        assert!(m.fxdict.is_none() && m.atlas.is_none());
    }

    #[test]
    fn another_shipments_sprite_is_a_frame_only_through_requires() {
        let a = ship_files("sreq", "mod-a", &[], &sprite("qm_star"), &[], &["qm_star"]);
        let b = ship("sreq", "mod-b", &[], &add_fx("fx_b", "tpl_b", "fx_b"), &[("fx_b.yaml", framed("qm_star"))]);
        let f = run(&[&a, &b], Scope::Link).err().unwrap();
        assert_eq!(codes(&f), vec!["M0259"]);
        assert!(f.problems[0].message.contains("does not require"), "{}", f.problems[0].message);
        let b = ship("sreq-ok", "mod-b", &["mod-a"], &add_fx("fx_b", "tpl_b", "fx_b"), &[("fx_b.yaml", framed("qm_star"))]);
        assert!(run(&[&a, &b], Scope::Link).is_ok());
        // Built alone, the requiring Shipment leaves the frame to the link.
        let m = run(&[&b], Scope::Build).unwrap();
        assert_eq!(m.deferred_frames.len(), 1);
        assert_eq!(m.deferred_frames[0].1, pandemic_hash_m2("qm_star"));
        // A Shipment that requires nothing resolves every frame itself.
        let c = ship("sreq-none", "mod-c", &[], &add_fx("fx_c", "tpl_c", "fx_c"), &[("fx_c.yaml", framed("qm_star"))]);
        assert_eq!(codes(&run(&[&c], Scope::Build).err().unwrap()), vec!["M0259"]);
    }

    #[test]
    fn a_sprite_key_the_fxdict_or_another_sprite_has_is_refused() {
        // A name another Shipment already added is M0306; a name written as a hash is M0305.
        let a = ship_files("sprite-taken", "mod-a", &[], &sprite("qm_same"), &[], &["qm_same"]);
        let b = ship_files("sprite-taken", "mod-b", &[], &sprite("qm_same"), &[], &["qm_same"]);
        let f = run(&[&a, &b], Scope::Link).err().unwrap();
        assert_eq!(codes(&f), vec!["M0306"]);
        assert!(f.problems[0].message.contains("already added by mod-a"), "{}", f.problems[0].message);
        let h = ship_files("hash", "mod-h", &[], "  - kind: add_fx_sprite\n    name: \"0x00000007\"\n    image: src/x.png\n", &[], &["x"]);
        assert_eq!(codes(&run(&[&h], Scope::Build).err().unwrap()), vec!["M0305"]);
    }

    #[test]
    fn the_set_packs_the_same_whatever_order_its_shipments_come_in() {
        let a = ship_files("order-s", "mod-a", &[], &(sprite("qm_one") + &sprite("qm_two")), &[], &["qm_one", "qm_two"]);
        let b = ship_files("order-s", "mod-b", &[], &sprite("qm_three"), &[], &["qm_three"]);
        let ab = run(&[&a, &b], Scope::Link).unwrap();
        let ba = run(&[&b, &a], Scope::Link).unwrap();
        assert_eq!(ab.fxdict, ba.fxdict);
        assert_eq!(ab.atlas, ba.atlas);
        assert_eq!(ab.placed.len(), 3);
    }

    #[test]
    fn sprites_that_do_not_fit_are_m0307_on_every_sprite() {
        // The free square of the test atlas is 32²: seventeen 8² sprites need 1,088 texels of its 1,024.
        let names: Vec<String> = (0..17).map(|i| format!("qm_s{i}")).collect();
        let contributions: String = names.iter().map(|n| sprite(n)).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let s = ship_files("full", "mod-a", &[], &contributions, &[], &refs);
        let f = run(&[&s], Scope::Build).err().unwrap();
        assert_eq!(codes(&f), vec!["M0307"; 17]);
        assert!(f.problems[0].message.contains("need 1088 texels"), "{}", f.problems[0].message);
    }
}
