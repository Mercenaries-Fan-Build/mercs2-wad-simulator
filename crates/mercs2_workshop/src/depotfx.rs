//! The Depot's effect editors and previews (`add_fx`, `replace_fx`;
//! `docs/modernization/workshop_pages/depot_kind_editors.md`, Effects).
//!
//! What the viewport runs is what `qm build` ships: the item's fx merged over the game's
//! ([`mercs2_quartermaster::fx::merge_shipment`], the merge `qm build` and the lint's fx checks run),
//! the effect read back from the merged effects block, run by the retail sim in the Depot's own world
//! under the templates that start it, drawn from the atlas the merge draws (or the game's).
//!
//! The editor edits the effect node by node, and every change is one of qm's `replace_fx` edit
//! operations ([`Edit`]): an `add_fx` applies it to its form and writes the form back
//! ([`EffectForm::express`]); a `replace_fx` folds it into its edits file ([`upsert`], [`settle`]).
//! A write rewrites only what the edit changed ([`mercs2_quartermaster::lossless`]), so what an
//! author wrote by hand stays.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use mercs2_formats::fxdict::{
    attribute_name, parse_effect_container, parse_fxdict_container, AnimKey, Atrb, AtrbValue, AttrDef, ColrKey, EffectContainer, Emitter, ForceKind,
    FxRect, ValueKind, COLR_KEYS,
};
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::texture::{parse_texture_container, TextureData};
use mercs2_quartermaster::effect::{self, AtrbOption, ColourKeyForm, CurveForm, EffectForm, EmitterForm, ForceForm, ValueInput, WordInput};
use mercs2_quartermaster::fx::{self, Edit, EditsForm};
use mercs2_quartermaster::game::GameStack;
use mercs2_quartermaster::manifest::{Contribution, FxTarget};
use mercs2_quartermaster::Format;

use crate::depot::Item;
use crate::depottext::{poll, spawn, Loading};
use crate::gui::theme::{self, FieldState};
use crate::vfxview::{StartedBy, VfxAct};

/// Whether `c` is an effect kind this module edits.
pub fn of(c: &Contribution) -> bool {
    matches!(c, Contribution::AddFx { .. } | Contribution::ReplaceFx { .. })
}

/// Whether `item` holds an effect kind.
pub fn of_item(item: &Item) -> bool {
    item.file.as_ref().is_ok_and(|f| f.contributions.iter().any(of))
}

// ─────────────────────────────────────────────────────────────── what the build ships

/// What the item's effects draw from: the atlas and `fxdict` the merge writes, or the game's.
pub struct FxAtlas {
    /// Changes with every read.
    pub id: u64,
    pub texture: TextureData,
    pub rects: Vec<FxRect>,
    /// A small level of it, for the frame thumbnails.
    pub thumb: (u32, u32, Vec<u8>),
    /// Where the atlas came from.
    pub from: &'static str,
}

/// One effect contribution, as shipped and as built.
pub struct View {
    pub hash: u32,
    pub name: String,
    /// The game's effect, for a `replace_fx`.
    pub shipped: Option<Arc<EffectContainer>>,
    /// The effect the merged effects block carries.
    pub built: Option<Arc<EffectContainer>>,
    /// The templates that start the built effect, the one a `replace_fx` names first.
    pub started_by: Vec<StartedBy>,
    /// The game's templates that start the shipped effect.
    pub shipped_started_by: Vec<StartedBy>,
    /// The merge's findings against this contribution, and why something is missing.
    pub problems: Vec<String>,
}

impl View {
    fn new(hash: u32, name: String) -> View {
        View { hash, name, shipped: None, built: None, started_by: Vec::new(), shipped_started_by: Vec::new(), problems: Vec::new() }
    }
}

/// How one field of a template class takes its value (`template.rs`'s table).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldKind {
    /// A one-bit field: `true` / `false`.
    Flag,
    /// An integer from `0` up to `max` (a bit field, `Byte`, `U8`, `Short`, `U16`).
    Count { max: i64 },
    /// An `i32`.
    Int,
    /// A name, hashed, or `0xHHHHHHHH`.
    Hash,
    F32,
    Vec3,
    /// Eight numbers.
    Blob32,
    /// An inline string.
    Text,
}

/// One field of a template class.
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub hash: u32,
    pub kind: FieldKind,
}

/// One template class: its fields, and a record of a shipped template that carries it, to start a new
/// component from.
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub fields: Vec<FieldInfo>,
    pub seed: Option<(String, BTreeMap<String, mercs2_quartermaster::template::FieldInput>)>,
}

/// What an `add_fx` template is declared against: the game's worldentity and its classes.
pub struct Classes {
    pub we: mercs2_formats::worldentity::WorldEntity,
    pub classes: BTreeMap<String, ClassInfo>,
}

fn field_kind(f: &mercs2_formats::schema::SchemaField, text: bool) -> FieldKind {
    use mercs2_formats::schema::SchemaFieldType as T;
    if text {
        return FieldKind::Text;
    }
    match (f.bit_width, f.field_type) {
        (1, _) => FieldKind::Flag,
        (w, _) if w > 1 => FieldKind::Count { max: (1i64 << w) - 1 },
        (_, T::Byte | T::U8) => FieldKind::Count { max: 0xFF },
        (_, T::Short | T::U16) => FieldKind::Count { max: 0xFFFF },
        (_, T::Int) => FieldKind::Int,
        (_, T::Hash | T::Enum | T::StringRef) => FieldKind::Hash,
        (_, T::F32) => FieldKind::F32,
        (_, T::Vec3) => FieldKind::Vec3,
        (_, T::Blob32) => FieldKind::Blob32,
    }
}

/// Every class a template may carry (not `Name`, which is the template's name), its fields, and the
/// first shipped template's record of it.
pub fn read_classes(we: mercs2_formats::worldentity::WorldEntity) -> Result<Classes, String> {
    use mercs2_formats::worldentity::Layout;
    let mut classes = BTreeMap::new();
    for c in we.components.iter().filter(|c| c.layout != Layout::Name) {
        if classes.contains_key(&c.class) {
            continue;
        }
        let texts: Vec<bool> = match c.layout {
            Layout::PointLocation => vec![false, true],
            _ => vec![false; c.schema.fields.len()],
        };
        let fields = c.schema.fields.iter().zip(texts).map(|(f, t)| FieldInfo { hash: f.name_hash, kind: field_kind(f, t) }).collect();
        let seed = c.records.iter().flat_map(|r| r.keys.iter()).find_map(|k| {
            let form = mercs2_quartermaster::template::TemplateForm::express(&we, *k).ok()?;
            let record = match form.components.get(&c.class)? {
                mercs2_quartermaster::template::Records::One(m) => m.clone(),
                mercs2_quartermaster::template::Records::Many(v) => v.first()?.clone(),
            };
            Some((form.name, record))
        });
        classes.insert(c.class.clone(), ClassInfo { fields, seed });
    }
    Ok(Classes { we, classes })
}

/// What the item's effect contributions build.
pub struct ItemFx {
    /// Changes with every read.
    pub id: u64,
    pub views: BTreeMap<usize, View>,
    pub atlas: Arc<FxAtlas>,
    /// Each block the build emits, and what it holds of the item's.
    pub ships: Vec<(String, String)>,
    /// Every frame an effect may name: the game's `fxdict` records and the item's sprites.
    pub frames: Vec<u32>,
    /// The item's sprites, by frame key.
    pub sprites: HashMap<u32, String>,
    /// What an `add_fx` template is declared against; `None` for an item without one.
    pub classes: Option<Arc<Classes>>,
    pub log: Vec<String>,
}

static READS: AtomicU64 = AtomicU64::new(1);

/// A name or `0xHHHHHHHH` as a hash, as qm reads an effect target or an attribute key.
fn name_hash(s: &str) -> u32 {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .filter(|h| !h.is_empty() && h.len() <= 8)
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| m2(s))
}

fn template_names(we: &mercs2_formats::worldentity::WorldEntity) -> Result<HashMap<u32, String>, String> {
    let mut names = HashMap::new();
    for (name, keys) in we.names()? {
        for k in keys {
            names.insert(*k, name.clone());
        }
    }
    Ok(names)
}

fn effect_of(entries: &[mercs2_formats::scripts_block::Entry], hash: u32) -> Option<Result<EffectContainer, String>> {
    entries.iter().find(|e| e.name_hash == hash).map(|e| parse_effect_container(&e.bytes))
}

/// `list` with the template named `first` moved to the front.
fn first_of(mut list: Vec<StartedBy>, first: Option<&String>) -> Vec<StartedBy> {
    if let Some(i) = first.and_then(|t| list.iter().position(|s| &s.template == t)) {
        let s = list.remove(i);
        list.insert(0, s);
    }
    list
}

/// What `contributions` (an item at `dir`) build, against the game whose `vz.wad` is the first of
/// `wads`.
pub fn after(wads: &[PathBuf], dir: &Path, contributions: &[Contribution]) -> Result<ItemFx, String> {
    let item = crate::depot::ItemFile { state: crate::depot::State::Draft, contributions: contributions.to_vec() };
    let mut manifest = crate::depot::item_manifest(&item);
    manifest.shipment.name = crate::depotsound::SHIPMENT.to_string();
    let vz = wads.first().ok_or("effects are merged over the game's, and no game archive is open")?;
    let paths = mercs2_quartermaster::compat::game_stack_paths(vz, [&manifest])?;
    let mut game = GameStack::open(&paths).map_err(|e| e.to_string())?;
    let base = fx::GameFx::read(&mut game)?;
    let (base_atlas, merged) = fx::merge_shipment(&manifest, dir, &mut game)?;
    let game_names = template_names(&base.worldentity)?;
    let game_starters = crate::vfxview::starters(&base.worldentity, &game_names)?;
    let sprites: HashMap<u32, String> = contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::AddFxSprite { name, .. } => Some((m2(name), name.clone())),
            _ => None,
        })
        .collect();

    // The atlas the effects draw from.
    let drawn = merged.as_ref().ok().and_then(|m| m.atlas.as_deref().map(|a| (a, m.fxdict.as_deref())));
    let (texture, rects, from) = match drawn {
        Some((atlas, fxdict)) => {
            let texture = parse_texture_container(atlas).map_err(|e| format!("the merged vfx atlas: {e}"))?;
            let rects = match fxdict {
                Some(c) => parse_fxdict_container(c).map_err(|e| format!("the merged fxdict: {e}"))?,
                None => base.fxdict.clone(),
            };
            (texture, rects, "the atlas the build draws")
        }
        None => (crate::depottex::atlas_texture(&base_atlas), base.fxdict.clone(), "the game's atlas"),
    };
    let levels = crate::texpng::decode_levels(&texture);
    let thumb = levels
        .iter()
        .find(|l| l.width <= 512)
        .or(levels.last())
        .map(|l| (l.width, l.height, l.rgba.clone()))
        .ok_or("the vfx atlas decodes to no level")?;
    let atlas = Arc::new(FxAtlas { id: READS.fetch_add(1, Ordering::Relaxed), texture, rects, thumb, from });
    let game_frames: std::collections::BTreeSet<u32> = base.fxdict.iter().map(|r| r.key).collect();
    let frames: Vec<u32> = game_frames.iter().copied().chain(sprites.keys().copied().filter(|k| !game_frames.contains(k))).collect();

    let (built_entries, built_starters, failure, log) = match &merged {
        Ok(m) => {
            let we = m.worldentity.as_ref().unwrap_or(&base.worldentity);
            let names = template_names(we)?;
            (m.effects.as_slice(), crate::vfxview::starters(we, &names)?, None, m.log.clone())
        }
        Err(f) => (&[][..], HashMap::new(), Some(f), Vec::new()),
    };
    let mut views = BTreeMap::new();
    let mut ships = Vec::new();
    for (index, c) in contributions.iter().enumerate() {
        let (hash, name, template) = match c {
            Contribution::AddFx { name, template, .. } => (m2(name), name.clone(), Some(template.name.clone())),
            Contribution::ReplaceFx { target, .. } => {
                let resolved = merged.as_ref().ok().and_then(|m| m.resolved.iter().find(|(cl, _)| cl.index == index).map(|(_, h)| *h));
                let (hash, template) = match (resolved, target) {
                    (Some(h), FxTarget::Template { template }) => (Some(h), Some(template.clone())),
                    (Some(h), FxTarget::Effect { .. }) => (Some(h), None),
                    (None, FxTarget::Effect { effect }) => (Some(name_hash(effect)), None),
                    (None, FxTarget::Template { template }) => {
                        let keys = fx::template_keys(&base.worldentity, template)?;
                        let h = keys.first().map(|k| fx::template_effect(&base.worldentity, *k)).transpose()?;
                        (h, Some(template.clone()))
                    }
                };
                let Some(hash) = hash else {
                    let mut v = View::new(0, target.to_string());
                    v.problems.push(format!("M0260 \u{2014} {target}: no template of that name"));
                    views.insert(index, v);
                    continue;
                };
                (hash, target.to_string(), template)
            }
            _ => continue,
        };
        let mut v = View::new(hash, name);
        if matches!(c, Contribution::ReplaceFx { .. }) {
            match effect_of(&base.effects.entries, hash) {
                Some(Ok(e)) => v.shipped = Some(Arc::new(e)),
                Some(Err(e)) => v.problems.push(format!("the game's effect 0x{hash:08X} does not read: {e}")),
                None => v.problems.push(format!("M0260 \u{2014} the game ships no effect 0x{hash:08X}")),
            }
        }
        if let Some(f) = failure {
            for p in f.problems.iter().filter(|p| p.index == index) {
                v.problems.push(format!("{} \u{2014} {}", p.code, p.message));
            }
            for k in &f.conflicts {
                v.problems.push(format!("M0207 \u{2014} {k}"));
            }
            if v.problems.is_empty() {
                v.problems.push(format!("the merge refuses the item: {f}"));
            }
        }
        match effect_of(built_entries, hash) {
            Some(Ok(e)) => v.built = Some(Arc::new(e)),
            Some(Err(e)) => v.problems.push(format!("the built effect 0x{hash:08X} does not read back: {e}")),
            None if failure.is_none() => v.problems.push(format!("the merged effects block holds no effect 0x{hash:08X}")),
            None => {}
        }
        v.started_by = first_of(built_starters.get(&hash).cloned().unwrap_or_default(), template.as_ref());
        v.shipped_started_by = first_of(game_starters.get(&hash).cloned().unwrap_or_default(), template.as_ref());
        if v.built.is_some() {
            let what = if matches!(c, Contribution::AddFx { .. }) { "appended" } else { "replaced in place" };
            ships.push((fx::EFFECTS_BLOCK.1.to_string(), format!("effect 0x{hash:08X} {}, {what}", v.name)));
            if let (Contribution::AddFx { .. }, Some(t)) = (c, &template) {
                ships.push((mercs2_quartermaster::link::SCRIPT_BLOCKS[1].1.to_string(), format!("template {t}, appended to the worldentity")));
            }
        }
        views.insert(index, v);
    }
    let classes = match contributions.iter().any(|c| matches!(c, Contribution::AddFx { .. })) {
        true => Some(Arc::new(read_classes(base.worldentity)?)),
        false => None,
    };
    Ok(ItemFx { id: READS.fetch_add(1, Ordering::Relaxed), views, atlas, ships, frames, sprites, classes, log })
}

// ─────────────────────────────────────────────────────────────── the edit model

/// The field an edit operation sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Address {
    Attribute(usize, u32),
    Channel(usize, u32),
    Colour(usize),
    Frames(usize),
    Transform(usize),
    Flags(usize),
    Geom(usize),
    ForceParams(usize),
    ForceAttribute(usize, u32),
    Shape(usize),
}

/// The field `e` sets; `None` for an operation that adds or removes a node.
pub fn address(e: &Edit) -> Option<Address> {
    Some(match e {
        Edit::Attribute { emitter, attribute, .. } => Address::Attribute(*emitter, name_hash(attribute)),
        Edit::Channel { emitter, channel, .. } => Address::Channel(*emitter, name_hash(channel)),
        Edit::ColourRgb { emitter, .. } | Edit::ColourKeys { emitter, .. } => Address::Colour(*emitter),
        Edit::Frames { emitter, .. } => Address::Frames(*emitter),
        Edit::Transform { emitter, .. } => Address::Transform(*emitter),
        Edit::Flags { emitter, .. } => Address::Flags(*emitter),
        Edit::Geom { emitter, .. } => Address::Geom(*emitter),
        Edit::ForceParams { force, .. } => Address::ForceParams(*force),
        Edit::ForceAttribute { force, attribute, .. } => Address::ForceAttribute(*force, name_hash(attribute)),
        Edit::Shape { shape, .. } => Address::Shape(*shape),
        Edit::AddEmitter { .. } | Edit::RemoveEmitter { .. } | Edit::AddForce { .. } | Edit::RemoveForce { .. } | Edit::AddShape { .. } | Edit::RemoveShape { .. } => {
            return None
        }
    })
}

/// The last edit that addresses `a`, when no later operation adds or removes a node (which would
/// renumber what it names).
fn last_settable(edits: &[Edit], a: Address) -> Option<usize> {
    let j = edits.iter().rposition(|e| address(e) == Some(a))?;
    edits[j + 1..].iter().all(|e| address(e).is_some()).then_some(j)
}

/// Fold `new` into `edits`: an edit of the same field that nothing renumbers since takes its parts
/// (a `colour_rgb` over `colour_keys` stays its own operation); anything else is appended. Returns
/// where it went.
pub fn upsert(edits: &mut Vec<Edit>, new: Edit) -> usize {
    let Some(j) = address(&new).and_then(|a| last_settable(edits, a)) else {
        edits.push(new);
        return edits.len() - 1;
    };
    let merged = match (&edits[j], new) {
        (Edit::Attribute { emitter, attribute, value, curve, options }, Edit::Attribute { value: v, curve: c, options: o, .. }) => {
            Edit::Attribute { emitter: *emitter, attribute: attribute.clone(), value: v.or(value.clone()), curve: c.or(curve.clone()), options: o.or(options.clone()) }
        }
        (Edit::Channel { emitter, channel, value, curve, options }, Edit::Channel { value: v, curve: c, options: o, .. }) => {
            Edit::Channel { emitter: *emitter, channel: channel.clone(), value: v.or(value.clone()), curve: c.or(curve.clone()), options: o.or(options.clone()) }
        }
        (Edit::ForceAttribute { force, attribute, value, curve, options }, Edit::ForceAttribute { value: v, curve: c, options: o, .. }) => {
            Edit::ForceAttribute { force: *force, attribute: attribute.clone(), value: v.or(value.clone()), curve: c.or(curve.clone()), options: o.or(options.clone()) }
        }
        (Edit::ColourKeys { .. }, n @ Edit::ColourRgb { .. }) => {
            edits.push(n);
            return edits.len() - 1;
        }
        (_, n) => n,
    };
    edits[j] = merged;
    j
}

/// Whether "use shipped" can take field `a` back: an edit sets it, and nothing renumbers since.
pub fn revertible(edits: &[Edit], a: Address) -> bool {
    last_settable(edits, a).is_some()
}

/// Take back every edit of field `a` that nothing renumbers since.
pub fn revert(edits: &mut Vec<Edit>, a: Address) {
    while let Some(j) = last_settable(edits, a) {
        edits.remove(j);
    }
}

/// Drop the field edit at `j` when the effect is the same without it: an edit back to the shipped
/// value takes itself out.
pub fn settle(shipped: &EffectContainer, edits: &mut Vec<Edit>, j: usize) {
    if address(&edits[j]).is_none() {
        return;
    }
    let apply = |list: &[Edit]| {
        let mut fx = shipped.clone();
        fx::apply_edits(&mut fx, list).map(|()| fx)
    };
    let mut without = edits.clone();
    without.remove(j);
    if let (Ok(a), Ok(b)) = (apply(edits), apply(&without)) {
        if a == b {
            *edits = without;
        }
    }
}

/// Every field the edits set that "use shipped" can take back.
pub fn edited(edits: &[Edit]) -> HashSet<Address> {
    edits.iter().filter_map(address).filter(|a| revertible(edits, *a)).collect()
}

// ─────────────────────────────────────────────────────────────── the files

/// An effect contribution's file, as the editor reads it.
pub enum Doc {
    /// No file yet.
    Missing,
    /// It does not read, or does not build.
    Bad(String),
    /// An `add_fx` effect form.
    Form { prior: String, format: Format, fx: EffectContainer },
    /// A `replace_fx` edits file.
    Edits { prior: String, format: Format, edits: Vec<Edit> },
}

/// The effect a form's text builds.
fn form_effect(text: &str, format: Format) -> Result<EffectContainer, String> {
    effect::from_str(text, format)?.build().map_err(|e| format!("M0252 \u{2014} the form does not build: {e}"))
}

fn form_text(fx: &EffectContainer, format: Format) -> Result<String, String> {
    effect::to_string(&EffectForm::express(fx), format)
}

/// Read an `add_fx` effect form.
pub fn read_form(path: &Path, text: &str) -> Doc {
    let format = match effect::file_format(path) {
        Ok(f) => f,
        Err(e) => return Doc::Bad(e),
    };
    let fx = match form_effect(text, format) {
        Ok(fx) => fx,
        Err(e) => return Doc::Bad(e),
    };
    match form_text(&fx, format) {
        Ok(_) => Doc::Form { prior: text.to_string(), format, fx },
        Err(e) => Doc::Bad(e),
    }
}

/// Read a `replace_fx` edits file.
pub fn read_edits(path: &Path, text: &str) -> Doc {
    let format = match effect::file_format(path) {
        Ok(f) => f,
        Err(e) => return Doc::Bad(e),
    };
    let form = match fx::edits_from_str(text, format) {
        Ok(f) => f,
        Err(e) => return Doc::Bad(e),
    };
    match fx::edits_to_string(&form, format) {
        Ok(_) => Doc::Edits { prior: text.to_string(), format, edits: form.edits },
        Err(e) => Doc::Bad(e),
    }
}

/// `form` written over `prior`, an edits file: only what changed is rewritten, so the rest of the
/// author's text stays ([`mercs2_quartermaster::lossless`]).
fn edits_over(prior: &str, format: Format, form: &EditsForm) -> Result<String, String> {
    mercs2_quartermaster::lossless::rewrite(prior, format, form, |t| fx::edits_from_str(t, format), |f| fx::edits_to_string(f, format))
}

/// The text an edit makes of a file, or why it is refused.
pub fn edited_text(doc: &Doc, path: &Path, shipped: Option<&EffectContainer>, e: Edit) -> Result<String, String> {
    match doc {
        Doc::Form { prior, format, fx } => {
            let mut next = fx.clone();
            fx::apply_edits(&mut next, std::slice::from_ref(&e))?;
            let f = *format;
            mercs2_quartermaster::lossless::rewrite(prior, f, &next, |t| form_effect(t, f), |x| form_text(x, f))
        }
        Doc::Edits { prior, format, edits } => {
            let shipped = shipped.ok_or("the shipped effect does not read")?;
            let mut next = edits.clone();
            let j = upsert(&mut next, e);
            settle(shipped, &mut next, j);
            let mut check = shipped.clone();
            fx::apply_edits(&mut check, &next)?;
            edits_over(prior, *format, &EditsForm { edits: next })
        }
        Doc::Missing => {
            let shipped = shipped.ok_or("the shipped effect does not read")?;
            let mut check = shipped.clone();
            fx::apply_edits(&mut check, std::slice::from_ref(&e))?;
            fx::edits_to_string(&EditsForm { edits: vec![e] }, effect::file_format(path)?)
        }
        Doc::Bad(_) => Err("the file does not read, so the editor does not write it".into()),
    }
}

/// The text "use shipped" makes of a `replace_fx` edits file.
pub fn reverted_text(doc: &Doc, a: Address) -> Result<String, String> {
    let Doc::Edits { prior, format, edits } = doc else { return Err("only a replace_fx has shipped values to go back to".into()) };
    let mut next = edits.clone();
    revert(&mut next, a);
    edits_over(prior, *format, &EditsForm { edits: next })
}

/// The form an `add_fx` starts from: the shipped `effect`, written as qm writes a form.
pub fn start_text(wads: &[PathBuf], effect: &str, path: &Path) -> Result<String, String> {
    let vz = wads.first().ok_or("no game archive is open")?;
    let mut game = GameStack::open(std::slice::from_ref(vz)).map_err(|e| e.to_string())?;
    let base = fx::GameFx::read(&mut game)?;
    let hash = name_hash(effect);
    let fx = effect_of(&base.effects.entries, hash).ok_or_else(|| format!("the game ships no effect {effect} (0x{hash:08X})"))??;
    effect::to_string(&EffectForm::express(&fx), effect::file_format(path)?)
}

// ─────────────────────────────────────────────────────────────── the page's state

/// The node of an effect the editor shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Emitter(usize),
    Force(usize),
    Shape(usize),
}

impl Default for Node {
    fn default() -> Node {
        Node::Emitter(0)
    }
}

/// What the viewport runs of an effect contribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Side {
    Shipped,
    #[default]
    Built,
}

/// The transport of the effect the viewport runs, as of this frame (the app's `VfxView`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Live {
    pub time: f32,
    pub span: f32,
    pub particles: usize,
    pub paused: bool,
    pub speed: f32,
    pub travel: f32,
    pub chosen: Option<usize>,
}

/// What the viewport should run.
pub struct Want {
    /// Changes when anything it runs changes.
    pub id: String,
    /// Changes with the item and contribution.
    pub frame: String,
    pub label: String,
    pub hash: u32,
    pub effect: Arc<EffectContainer>,
    pub started_by: Vec<StartedBy>,
    pub atlas: Arc<FxAtlas>,
}

/// Each contribution file read, by path: its modified time when read and what it read as.
type DocCache = HashMap<PathBuf, (Option<SystemTime>, Rc<Doc>)>;

/// The effect editors' reads and choices.
#[derive(Default)]
pub struct Fx {
    after: RefCell<HashMap<PathBuf, (String, Loading<ItemFx>)>>,
    docs: RefCell<DocCache>,
    sides: RefCell<HashMap<(PathBuf, usize), Side>>,
    nodes: RefCell<HashMap<(PathBuf, usize), Node>>,
    /// The contribution each item's viewport runs.
    running: RefCell<HashMap<PathBuf, usize>>,
    /// The transport of what the viewport runs, set by the app each frame.
    pub live: std::cell::Cell<Option<Live>>,
    thumbs: RefCell<Option<(u64, egui::TextureHandle)>>,
}

/// What the effect editors ask of the page.
pub enum Ask {
    /// Write `text` over `path`, whose text was `prior`.
    Write { path: PathBuf, prior: String, text: String },
    /// Write the form of the shipped `effect` into the item's file `path`.
    Start { path: PathBuf, effect: String },
    Side { item: PathBuf, index: usize, side: Side },
    Node { item: PathBuf, index: usize, node: Node },
    Play(VfxAct),
    Refused(String),
}

impl Fx {
    pub fn forget_game(&self) {
        self.after.borrow_mut().clear();
        *self.thumbs.borrow_mut() = None;
    }

    /// What `item` builds: `None` while it is read, or when it holds no effect kind.
    pub fn after(&self, game: &[PathBuf], item: &Item) -> Option<Result<Rc<ItemFx>, String>> {
        let contributions = item.file.as_ref().ok()?.contributions.clone();
        if !contributions.iter().any(of) {
            return None;
        }
        let mut stamp = format!("{game:?}|{contributions:?}");
        for c in &contributions {
            let file = match c {
                Contribution::AddFx { effect, .. } => effect,
                Contribution::ReplaceFx { edits, .. } => edits,
                Contribution::AddFxSprite { image, .. } | Contribution::ReplaceTexture { image, .. } => image,
                _ => continue,
            };
            let t = std::fs::metadata(item.dir.join(file)).and_then(|m| m.modified()).ok();
            stamp.push_str(&format!("|{t:?}"));
        }
        let mut map = self.after.borrow_mut();
        if map.get(&item.dir).is_none_or(|(s, _)| *s != stamp) {
            let (wads, dir) = (game.to_vec(), item.dir.clone());
            map.insert(item.dir.clone(), (stamp, spawn(move || after(&wads, &dir, &contributions))));
        }
        let (_, entry) = map.get_mut(&item.dir)?;
        poll(entry)
    }

    /// The file `abs` of an effect contribution, read again when it changes.
    pub fn doc(&self, abs: &Path, replace: bool) -> Rc<Doc> {
        let t = std::fs::metadata(abs).and_then(|m| m.modified()).ok();
        if let Some((at, d)) = self.docs.borrow().get(abs) {
            if *at == t {
                return d.clone();
            }
        }
        let d = Rc::new(match std::fs::read_to_string(abs) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Doc::Missing,
            Err(e) => Doc::Bad(format!("{}: {e}", abs.display())),
            Ok(text) if replace => read_edits(abs, &text),
            Ok(text) => read_form(abs, &text),
        });
        self.docs.borrow_mut().insert(abs.to_path_buf(), (t, d.clone()));
        d
    }

    pub fn side(&self, item: &Path, index: usize) -> Side {
        self.sides.borrow().get(&(item.to_path_buf(), index)).copied().unwrap_or_default()
    }

    pub fn set_side(&self, item: PathBuf, index: usize, side: Side) {
        self.running.borrow_mut().insert(item.clone(), index);
        self.sides.borrow_mut().insert((item, index), side);
    }

    pub fn node(&self, item: &Path, index: usize) -> Node {
        self.nodes.borrow().get(&(item.to_path_buf(), index)).copied().unwrap_or_default()
    }

    pub fn set_node(&self, item: PathBuf, index: usize, node: Node) {
        self.running.borrow_mut().insert(item.clone(), index);
        self.nodes.borrow_mut().insert((item, index), node);
    }

    /// What the viewport runs for `item`: the built effect of the contribution last touched (the
    /// first at the start), or the shipped one while Shipped is chosen.
    pub fn want(&self, game: &[PathBuf], item: &Item) -> Option<Want> {
        let f = item.file.as_ref().ok()?;
        let after = self.after(game, item)?.ok()?;
        let first = f.contributions.iter().position(of)?;
        let index = self.running.borrow().get(&item.dir).copied().filter(|i| f.contributions.get(*i).is_some_and(of)).unwrap_or(first);
        let v = after.views.get(&index)?;
        let side = self.side(&item.dir, index);
        let (effect, started_by) = match side {
            Side::Shipped => (v.shipped.clone()?, v.shipped_started_by.clone()),
            Side::Built => (v.built.clone()?, v.started_by.clone()),
        };
        let frame = format!("{}|{index}", item.dir.display());
        Some(Want { id: format!("{frame}|{side:?}|{}", after.id), frame, label: v.name.clone(), hash: v.hash, effect, started_by, atlas: after.atlas.clone() })
    }

    fn thumbs(&self, ctx: &egui::Context, atlas: &FxAtlas) -> egui::TextureHandle {
        let mut slot = self.thumbs.borrow_mut();
        if let Some((id, t)) = slot.as_ref() {
            if *id == atlas.id {
                return t.clone();
            }
        }
        let (w, h, rgba) = &atlas.thumb;
        let image = egui::ColorImage::from_rgba_unmultiplied([*w as usize, *h as usize], rgba);
        let t = ctx.load_texture(format!("depot-fx-atlas-{}", atlas.id), image, egui::TextureOptions::LINEAR);
        *slot = Some((atlas.id, t.clone()));
        t
    }
}

// ─────────────────────────────────────────────────────────────── widgets

/// A value an edit holds while the pointer or the keyboard is on it; `Some` once it is let go
/// changed.
fn held<T: Clone + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    value: &T,
    draw: impl FnOnce(&mut egui::Ui, &mut T) -> egui::Response,
) -> Option<T> {
    let id = ui.make_persistent_id(salt);
    let mut v = ui.data_mut(|d| d.get_temp::<(T, T)>(id)).filter(|(o, _)| o == value).map_or_else(|| value.clone(), |(_, v)| v);
    let resp = draw(ui, &mut v);
    let busy = resp.dragged() || resp.has_focus() || ui.ctx().input(|i| i.pointer.any_down());
    if v == *value {
        ui.data_mut(|d| d.remove::<(T, T)>(id));
        return None;
    }
    if busy {
        ui.data_mut(|d| d.insert_temp(id, (value.clone(), v)));
        return None;
    }
    ui.data_mut(|d| d.remove::<(T, T)>(id));
    Some(v)
}

fn drag_f32(ui: &mut egui::Ui, v: &mut f32, width: f32) -> egui::Response {
    let speed = (v.abs() * 0.01).max(0.001);
    ui.add_sized([width, 20.0], egui::DragValue::new(v).speed(speed).max_decimals(4))
}

/// A row of `N` floats under `label`, each committed when let go.
fn floats_row<const N: usize>(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug + Copy, label: &str, v: &[f32; N]) -> Option<[f32; N]> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        theme::field_label(ui, label);
        let w = ((ui.available_width() - 4.0 * N as f32) / N as f32).max(36.0);
        for (i, x0) in v.iter().enumerate() {
            if let Some(x) = held(ui, (salt, i), x0, |ui, x| drag_f32(ui, x, w)) {
                let mut next = *v;
                next[i] = x;
                out = Some(next);
            }
        }
    });
    out
}

fn attr_label(hash: u32) -> String {
    attribute_name(hash).map_or_else(|| format!("0x{hash:08X}"), str::to_string)
}

fn value_text(v: AtrbValue) -> String {
    match v {
        AtrbValue::F32(x) => format!("{x:.4}"),
        AtrbValue::U32(x) => x.to_string(),
    }
}

/// What a card's edits produce: the edits to make, and the fields to take back to shipped.
#[derive(Default)]
struct Out {
    changes: Vec<Edit>,
    reverts: Vec<Address>,
}

/// Where an attribute row sits, for its edit.
#[derive(Clone, Copy)]
enum AttrAt {
    Particle(usize),
    Channel(usize),
    Force(usize),
}

impl AttrAt {
    fn edit(self, hash: u32, value: Option<ValueInput>, curve: Option<CurveForm>, options: Option<Vec<AtrbOption>>) -> Edit {
        let name = attr_label(hash);
        match self {
            AttrAt::Particle(emitter) => Edit::Attribute { emitter, attribute: name, value, curve, options },
            AttrAt::Channel(emitter) => Edit::Channel { emitter, channel: name, value, curve, options },
            AttrAt::Force(force) => Edit::ForceAttribute { force, attribute: name, value, curve, options },
        }
    }

    fn address(self, hash: u32) -> Address {
        match self {
            AttrAt::Particle(e) => Address::Attribute(e, hash),
            AttrAt::Channel(e) => Address::Channel(e, hash),
            AttrAt::Force(f) => Address::ForceAttribute(f, hash),
        }
    }
}

const OPTIONS: [(AtrbOption, u32, &str); 3] = [
    (AtrbOption::Bit7, mercs2_formats::fxdict::atrb_flag::BIT7, "bit 7"),
    (AtrbOption::Resample, mercs2_formats::fxdict::atrb_flag::RESAMPLE, "resample"),
    (AtrbOption::Bit9, mercs2_formats::fxdict::atrb_flag::BIT9, "bit 9"),
];

const CURVE_HEIGHT: f32 = 36.0;

fn curve_at(keys: &[AnimKey], x: f32) -> f32 {
    match keys.iter().position(|k| k.time > x) {
        None => keys[keys.len() - 1].value,
        Some(0) => keys[0].value,
        Some(i) => {
            let (a, b) = (keys[i - 1], keys[i]);
            a.value + (b.value - a.value) * ((x - a.time) / (b.time - a.time).max(f32::EPSILON))
        }
    }
}

/// What a row knows of the shipped effect: the shipped attribute, and whether an edit sets it.
struct Shipped<'a> {
    attr: Option<&'a Atrb>,
    edited: bool,
}

fn keys_form(keys: &[AnimKey]) -> CurveForm {
    CurveForm::Keys(keys.iter().map(|k| [k.time, k.value]).collect())
}

/// One attribute: its value, its curve (plotted, its keys editable) and its option bits.
fn attr_row(ui: &mut egui::Ui, d: &AttrDef, a: &Atrb, at: AttrAt, shipped: &Shipped<'_>, out: &mut Out) {
    let label = attr_label(d.hash);
    ui.push_id(("attr", d.hash), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let lw = theme::field_label_w(ui.available_width());
            ui.allocate_ui_with_layout(egui::vec2(lw, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(lw);
                let ink = if shipped.edited { theme::BRASS } else { theme::DIM };
                ui.add(egui::Label::new(theme::disp_text(label.to_uppercase(), theme::style_eyebrow(), ink)).truncate().selectable(false))
                    .on_hover_text(format!("{label} \u{b7} 0x{:08X}", d.hash));
            });
            let w = (ui.available_width() - if shipped.edited { 28.0 } else { 4.0 }).max(40.0);
            match (d.kind, a.value) {
                (ValueKind::F32, AtrbValue::F32(x)) => {
                    if let Some(v) = held(ui, "value", &x, |ui, v| drag_f32(ui, v, w)) {
                        out.changes.push(at.edit(d.hash, Some(ValueInput::Float(v)), None, None));
                    }
                }
                (_, AtrbValue::U32(x)) => {
                    let x = x as i64;
                    if let Some(v) = held(ui, "value", &x, |ui, v| ui.add_sized([w, 20.0], egui::DragValue::new(v).range(0..=u32::MAX as i64))) {
                        out.changes.push(at.edit(d.hash, Some(ValueInput::Int(v)), None, None));
                    }
                }
                (_, v) => {
                    ui.label(egui::RichText::new(value_text(v)).monospace().color(theme::BAD));
                }
            }
            if shipped.edited && ui.small_button("Shipped").on_hover_text("Use the shipped value").clicked() {
                out.reverts.push(at.address(d.hash));
            }
        });
        if let Some(s) = shipped.attr.filter(|s| shipped.edited && *s != a) {
            let was = match &s.curve {
                Some(k) => format!("shipped {} \u{b7} a curve of {} keys", value_text(s.value), k.len()),
                None => format!("shipped {}", value_text(s.value)),
            };
            ui.label(egui::RichText::new(was).small().color(theme::FAINT));
        }
        // The curve and the option bits, folded until asked for.
        let on: Vec<&str> = OPTIONS.iter().filter(|(_, bit, _)| a.flags & bit != 0).map(|(_, _, n)| *n).collect();
        let title = match &a.curve {
            Some(k) => format!("curve of {} keys{}", k.len(), if on.is_empty() { String::new() } else { format!(" \u{b7} {}", on.join(", ")) }),
            None if !on.is_empty() => on.join(", "),
            None => "curve, options".to_string(),
        };
        egui::CollapsingHeader::new(egui::RichText::new(title).small().color(theme::FAINT)).id_salt("parts").show(ui, |ui| {
            match &a.curve {
                Some(keys) if !keys.is_empty() => {
                    let (x0, x1) = (keys[0].time, keys[keys.len() - 1].time);
                    theme::curve(ui, CURVE_HEIGHT, x0, x1.max(x0 + 1.0), x0, |x| curve_at(keys, x));
                    let mut next: Option<Vec<AnimKey>> = None;
                    for (i, k) in keys.iter().enumerate() {
                        ui.push_id(("key", i), |ui| {
                            if let Some([t, v]) = floats_row(ui, ("k", i), &format!("key {i}: time, value"), &[k.time, k.value]) {
                                let mut n = keys.clone();
                                n[i] = AnimKey { time: t, value: v };
                                next = Some(n);
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.small_button("Duplicate the last key").clicked() {
                            let mut n = keys.clone();
                            n.push(keys[keys.len() - 1]);
                            next = Some(n);
                        }
                        if keys.len() > 1 && ui.small_button("Remove the last key").clicked() {
                            let mut n = keys.clone();
                            n.pop();
                            next = Some(n);
                        }
                        if ui.small_button("Remove the curve").clicked() {
                            out.changes.push(at.edit(d.hash, None, Some(CurveForm::Keyword("none".into())), None));
                        }
                    });
                    if let Some(n) = next {
                        out.changes.push(at.edit(d.hash, None, Some(keys_form(&n)), None));
                    }
                }
                _ => {
                    if let AtrbValue::F32(x) = a.value {
                        if ui.small_button("Add a curve").on_hover_text("One key, at time 0, holding the value").clicked() {
                            out.changes.push(at.edit(d.hash, None, Some(CurveForm::Keys(vec![[0.0, x]])), None));
                        }
                    }
                }
            }
            ui.horizontal_wrapped(|ui| {
                theme::eyebrow(ui, "Options");
                for (o, bit, name) in OPTIONS {
                    let on = a.flags & bit != 0;
                    if theme::pill(ui, name, on).clicked() {
                        let set: Vec<AtrbOption> = OPTIONS.iter().filter(|(x, b, _)| if *x == o { !on } else { a.flags & b != 0 }).map(|(x, _, _)| *x).collect();
                        out.changes.push(at.edit(d.hash, None, None, Some(set)));
                    }
                }
            });
        });
    });
}

fn colour_strip(ui: &mut egui::Ui, keys: &[ColrKey], height: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click());
    let p = ui.painter_at(rect);
    let w = rect.width() / keys.len().max(1) as f32;
    for (i, k) in keys.iter().enumerate() {
        let [r, g, b, a] = k.rgba;
        let x = rect.left() + i as f32 * w;
        p.rect_filled(egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w + 0.5, rect.height())), 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, a.max(24)));
    }
    p.rect_stroke(rect, egui::CornerRadius::same(2), egui::Stroke::new(1.0, theme::LINE), egui::StrokeKind::Middle);
    resp
}

fn frame_uv(rects: &HashMap<u32, FxRect>, key: u32) -> egui::Rect {
    match rects.get(&key) {
        Some(r) => egui::Rect::from_min_size(egui::pos2(r.u, r.top()), egui::vec2(r.w, r.h)),
        None => egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
    }
}

fn frame_name(key: u32, sprites: &HashMap<u32, String>, names: Option<&mercs2_quartermaster::NameTable>) -> String {
    sprites.get(&key).cloned().or_else(|| names.and_then(|n| n.reverse(key)).map(str::to_string)).unwrap_or_else(|| format!("0x{key:08X}"))
}

const THUMB: f32 = 40.0;

// ─────────────────────────────────────────────────────────────── the cards

/// The context the cards draw with.
struct Ctx<'a> {
    item: &'a Item,
    index: usize,
    view: &'a View,
    after: &'a ItemFx,
    rects: HashMap<u32, FxRect>,
    thumbs: egui::TextureHandle,
    names: Option<&'a mercs2_quartermaster::NameTable>,
}

/// The effect the editor edits, the shipped one it is edited against, and the node shown.
struct Subject<'a> {
    fx: &'a EffectContainer,
    shipped: Option<&'a EffectContainer>,
    edited: HashSet<Address>,
    edits: &'a [Edit],
    node: Node,
}

/// The inspector's cards for contribution `index` of `item`, an effect kind: the preview's transport,
/// the effect's nodes, the selected node, and what the build ships.
pub fn cards(ui: &mut egui::Ui, f: &Fx, item: &Item, index: usize, game: &[PathBuf], names: Option<&mercs2_quartermaster::NameTable>) -> Vec<Ask> {
    let mut asks = Vec::new();
    let Some(c) = item.file.as_ref().ok().and_then(|x| x.contributions.get(index)) else { return asks };
    let (file, replace) = match c {
        Contribution::AddFx { effect, .. } => (effect, false),
        Contribution::ReplaceFx { edits, .. } => (edits, true),
        _ => return asks,
    };
    let after = match f.after(game, item) {
        None => {
            theme::card(ui, "Preview", None, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("Merging over the game's effects\u{2026}").color(theme::DIM));
                });
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            return asks;
        }
        Some(Err(e)) => {
            theme::card(ui, "Preview", None, |ui| theme::field_note(ui, FieldState::Bad, &e));
            return asks;
        }
        Some(Ok(a)) => a,
    };
    let Some(view) = after.views.get(&index) else { return asks };
    let abs = item.dir.join(file);
    let doc = f.doc(&abs, replace);
    let ctx = Ctx {
        item,
        index,
        view,
        after: &after,
        rects: after.atlas.rects.iter().map(|r| (r.key, *r)).collect(),
        thumbs: f.thumbs(ui.ctx(), &after.atlas),
        names,
    };
    preview_card(ui, f, &ctx, f.side(&item.dir, index), replace, &mut asks);

    // The effect the editor edits: the form's, or the shipped one after the edits.
    let shipped = view.shipped.as_deref();
    let current: Result<EffectContainer, String> = match (doc.as_ref(), shipped) {
        (Doc::Form { fx, .. }, _) => Ok(fx.clone()),
        (Doc::Edits { edits, .. }, Some(s)) => {
            let mut e = s.clone();
            fx::apply_edits(&mut e, edits).map(|()| e).map_err(|e| format!("M0261 \u{2014} {e}"))
        }
        (Doc::Missing, Some(s)) if replace => Ok(s.clone()),
        (Doc::Edits { .. } | Doc::Missing, None) if replace => Err("the shipped effect does not read".into()),
        (Doc::Missing, _) => Err(String::new()),
        (Doc::Bad(e), _) => Err(e.clone()),
        (Doc::Edits { .. }, None) => Err("an edits file on an add_fx".into()),
    };
    let edits: &[Edit] = match doc.as_ref() {
        Doc::Edits { edits, .. } => edits,
        _ => &[],
    };
    match &current {
        Ok(fx) => {
            let s = Subject { fx, shipped: shipped.filter(|_| replace), edited: edited(edits), edits, node: f.node(&item.dir, index) };
            let mut out = Out::default();
            nodes_card(ui, &ctx, &s, &mut out, &mut asks);
            node_card(ui, &ctx, &s, &mut out);
            let prior = match doc.as_ref() {
                Doc::Form { prior, .. } | Doc::Edits { prior, .. } => prior.clone(),
                _ => String::new(),
            };
            for e in out.changes {
                match edited_text(&doc, &abs, shipped, e) {
                    Ok(text) => asks.push(Ask::Write { path: abs.clone(), prior: prior.clone(), text }),
                    Err(e) => asks.push(Ask::Refused(e)),
                }
            }
            for a in out.reverts {
                match reverted_text(&doc, a) {
                    Ok(text) => asks.push(Ask::Write { path: abs.clone(), prior: prior.clone(), text }),
                    Err(e) => asks.push(Ask::Refused(e)),
                }
            }
        }
        Err(e) => {
            theme::card(ui, "Effect", Some(&file.display().to_string()), |ui| {
                if !e.is_empty() {
                    theme::field_note(ui, FieldState::Bad, e);
                }
                if matches!(doc.as_ref(), Doc::Missing) && !replace {
                    ui.label(egui::RichText::new("No effect form yet. Start from an effect the game ships.").small().color(theme::DIM));
                    start_picker(ui, &abs, game, &mut asks);
                }
            });
        }
    }
    asks
}

/// The shipped effect an `add_fx` starts from, by name or `0xHHHHHHHH`.
fn start_picker(ui: &mut egui::Ui, abs: &Path, game: &[PathBuf], asks: &mut Vec<Ask>) {
    let id = ui.make_persistent_id("fx_start");
    let mut typed: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    let r = theme::text_field(ui, "Start from", &mut typed, "global_explosion_c4", FieldState::Neutral);
    ui.data_mut(|d| d.insert_temp(id, typed.clone()));
    let ready = !typed.trim().is_empty() && !game.is_empty();
    let go = ui.add_enabled(ready, egui::Button::new("Start from this effect")).clicked() || (ready && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    if go {
        asks.push(Ask::Start { path: abs.to_path_buf(), effect: typed.trim().to_string() });
    }
}

/// The transport of what the viewport runs, the side it runs, and the templates it runs under.
fn preview_card(ui: &mut egui::Ui, f: &Fx, c: &Ctx<'_>, side: Side, replace: bool, asks: &mut Vec<Ask>) {
    theme::card(ui, "Preview", Some(&format!("0x{:08X}", c.view.hash)), |ui| {
        ui.add(egui::Label::new(egui::RichText::new(&c.view.name).color(theme::TX)).truncate());
        for p in &c.view.problems {
            theme::field_note(ui, FieldState::Bad, p);
        }
        if replace {
            ui.horizontal_wrapped(|ui| {
                for (s, label) in [(Side::Shipped, "Shipped"), (Side::Built, "As built")] {
                    if theme::pill(ui, label, side == s).clicked() && side != s {
                        asks.push(Ask::Side { item: c.item.dir.clone(), index: c.index, side: s });
                    }
                }
            });
        }
        let runs = match side {
            Side::Shipped => c.view.shipped.as_deref(),
            Side::Built => c.view.built.as_deref(),
        };
        if let Some(e) = runs {
            theme::kv(ui, "emitters", egui::RichText::new(e.emitters.len().to_string()));
            theme::kv(ui, "forces", egui::RichText::new(e.forces.len().to_string()));
        }
        theme::kv(ui, "atlas", egui::RichText::new(c.after.atlas.from));
        let Some(live) = f.live.get() else { return };
        theme::kv(ui, "particles", egui::RichText::new(live.particles.to_string()));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Restart").clicked() {
                asks.push(Ask::Play(VfxAct::Restart));
            }
            if ui.button(if live.paused { "Play" } else { "Pause" }).clicked() {
                asks.push(Ask::Play(VfxAct::Pause));
            }
        });
        let width = (ui.available_width() - 96.0).max(0.0);
        let mut t = live.time.min(live.span);
        if theme::scrub(ui, width, crate::vfxview::scrub_slider(&mut t, live.span)).changed() {
            asks.push(Ask::Play(VfxAct::Seek(t)));
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("speed").color(theme::DIM));
            for (s, text) in [(0.25, "\u{bc}\u{d7}"), (0.5, "\u{bd}\u{d7}"), (1.0, "1\u{d7}"), (2.0, "2\u{d7}")] {
                if ui.selectable_label(live.speed == s, text).clicked() && live.speed != s {
                    asks.push(Ask::Play(VfxAct::Speed(s)));
                }
            }
        });
        let mut travel = live.travel;
        ui.label(egui::RichText::new("travel").color(theme::DIM));
        if theme::scrub(ui, width, egui::Slider::new(&mut travel, 0.0..=30.0).suffix(" m/s")).changed() {
            asks.push(Ask::Play(VfxAct::Travel(travel)));
        }
        let started_by = match side {
            Side::Shipped => &c.view.shipped_started_by,
            Side::Built => &c.view.started_by,
        };
        ui.add_space(4.0);
        theme::eyebrow(ui, &format!("Started by \u{b7} {}", started_by.len()));
        theme::scroll_vertical().id_salt("fx_started_by").max_height(120.0).auto_shrink([false, true]).show(ui, |ui| {
            for (i, s) in started_by.iter().enumerate() {
                let duration = if s.starter.duration > 0.0 { format!("{:.2} \u{b1} {:.2} s", s.starter.duration, s.duration_var) } else { "no duration".into() };
                let text = format!("{} \u{b7} {duration} \u{b7} {:.2} per metre", s.template, s.starter.per_metre);
                if ui.add(egui::Button::selectable(live.chosen == Some(i), egui::RichText::new(text).small())).clicked() {
                    asks.push(Ask::Play(VfxAct::Starter(Some(i))));
                }
            }
        });
    });
}

/// Whether an edit addresses node `n`.
fn touches(edits: &[Edit], n: Node) -> bool {
    edits.iter().any(|e| match (address(e), n) {
        (
            Some(
                Address::Attribute(i, _)
                | Address::Channel(i, _)
                | Address::Colour(i)
                | Address::Frames(i)
                | Address::Transform(i)
                | Address::Flags(i)
                | Address::Geom(i),
            ),
            Node::Emitter(j),
        ) => i == j,
        (Some(Address::ForceParams(i) | Address::ForceAttribute(i, _)), Node::Force(j)) => i == j,
        (Some(Address::Shape(i)), Node::Shape(j)) => i == j,
        _ => false,
    })
}

/// The effect's nodes, as Unreal's emitter stack lists them: each emitter with its first frame and
/// its colour over life, each force, each shape table.
fn nodes_card(ui: &mut egui::Ui, c: &Ctx<'_>, s: &Subject<'_>, out: &mut Out, asks: &mut Vec<Ask>) {
    let fx = s.fx;
    let pick = |n: Node, asks: &mut Vec<Ask>| asks.push(Ask::Node { item: c.item.dir.clone(), index: c.index, node: n });
    let mark = |n: Node| if touches(s.edits, n) { " \u{25cf}" } else { "" };
    let badge = format!("{} emitters \u{b7} {} forces \u{b7} {} shapes", fx.emitters.len(), fx.forces.len(), fx.shapes.len());
    theme::card(ui, "Effect", Some(&badge), |ui| {
        theme::eyebrow(ui, "Emitters");
        for (i, e) in fx.emitters.iter().enumerate() {
            ui.push_id(("emitter", i), |ui| {
                let on = s.node == Node::Emitter(i);
                let fill = if on { theme::BRASS_SOFT } else { egui::Color32::TRANSPARENT };
                let r = egui::Frame::NONE.fill(fill).corner_radius(egui::CornerRadius::same(3)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(k) = e.particle.text.frames.first() {
                            ui.add(egui::Image::new(&c.thumbs).uv(frame_uv(&c.rects, *k)).fit_to_exact_size(egui::vec2(28.0, 28.0)));
                        }
                        ui.vertical(|ui| {
                            let rate = e.particle.attributes.iter().find(|a| a.hash == m2("rate")).map_or_else(String::new, |a| value_text(a.value));
                            ui.label(egui::RichText::new(format!("Emitter {i}{}", mark(Node::Emitter(i)))).color(if on { theme::BRASS } else { theme::TX }));
                            ui.label(egui::RichText::new(format!("{} frames \u{b7} rate {rate}", e.particle.text.frames.len())).small().color(theme::DIM));
                            colour_strip(ui, &e.particle.colr.keys, 6.0);
                        });
                    });
                });
                if ui.interact(r.response.rect, ui.scope_id().with("row"), egui::Sense::click()).clicked() {
                    pick(Node::Emitter(i), asks);
                }
            });
        }
        if let Node::Emitter(i) = s.node {
            ui.horizontal(|ui| {
                if ui.small_button("Duplicate emitter").clicked() {
                    out.changes.push(Edit::AddEmitter { at: i + 1, emitter: Box::new(EmitterForm::express(&fx.emitters[i])) });
                }
                if fx.emitters.len() > 1 && ui.small_button("Remove emitter").clicked() {
                    out.changes.push(Edit::RemoveEmitter { emitter: i });
                    pick(Node::Emitter(i.saturating_sub(1)), asks);
                }
            });
        }
        theme::eyebrow(ui, "Forces");
        if fx.forces.is_empty() {
            ui.label(egui::RichText::new("none").small().color(theme::FAINT));
        }
        for (i, f) in fx.forces.iter().enumerate() {
            if ui.add(egui::Button::selectable(s.node == Node::Force(i), format!("{} {i}{}", force_kind(&f.kind), mark(Node::Force(i))))).clicked() {
                pick(Node::Force(i), asks);
            }
        }
        if let Node::Force(i) = s.node {
            ui.horizontal(|ui| {
                if ui.small_button("Duplicate force").clicked() {
                    out.changes.push(Edit::AddForce { at: i + 1, force: ForceForm::express(&fx.forces[i]) });
                }
                if ui.small_button("Remove force").clicked() {
                    out.changes.push(Edit::RemoveForce { force: i });
                    pick(if fx.forces.len() > 1 { Node::Force(i.saturating_sub(1)) } else { Node::default() }, asks);
                }
            });
        }
        theme::eyebrow(ui, "Shapes");
        if fx.shapes.is_empty() {
            ui.label(egui::RichText::new("none").small().color(theme::FAINT));
        }
        for (i, sh) in fx.shapes.iter().enumerate() {
            let users: Vec<String> = fx.emitters.iter().enumerate().filter(|(_, e)| e.geom.is_some_and(|g| g.shape_index as usize == i)).map(|(j, _)| j.to_string()).collect();
            let users = if users.is_empty() { "no emitter".to_string() } else { format!("emitters {}", users.join(", ")) };
            let text = format!("Shape {i}{} \u{b7} {} triangles \u{b7} {users}", mark(Node::Shape(i)), sh.records.len());
            if ui.add(egui::Button::selectable(s.node == Node::Shape(i), text)).clicked() {
                pick(Node::Shape(i), asks);
            }
        }
        if let Node::Shape(i) = s.node {
            ui.horizontal(|ui| {
                if ui.small_button("Duplicate shape").clicked() {
                    out.changes.push(Edit::AddShape { at: i + 1, records: fx.shapes[i].records.clone() });
                }
                let used = fx.emitters.iter().any(|e| e.geom.is_some_and(|g| g.shape_index as usize == i));
                if ui.add_enabled(!used, egui::Button::new("Remove shape").small()).on_disabled_hover_text("An emitter spawns on it").clicked() {
                    out.changes.push(Edit::RemoveShape { shape: i });
                    pick(if fx.shapes.len() > 1 { Node::Shape(i.saturating_sub(1)) } else { Node::default() }, asks);
                }
            });
        }
    });
}

fn force_kind(k: &ForceKind) -> &'static str {
    match k {
        ForceKind::Gravity { .. } => "Gravity",
        ForceKind::Drag { .. } => "Drag",
        ForceKind::Wind { .. } => "Wind",
        ForceKind::Attractor { .. } => "Attractor",
        ForceKind::Vortex { .. } => "Vortex",
    }
}

/// The selected node in full, every part of it editable.
fn node_card(ui: &mut egui::Ui, c: &Ctx<'_>, s: &Subject<'_>, out: &mut Out) {
    let fx = s.fx;
    let is_edited = |a: Address| s.edited.contains(&a);
    match s.node {
        Node::Emitter(i) => {
            let Some(e) = fx.emitters.get(i) else { return };
            let se = s.shipped.and_then(|x| x.emitters.get(i));
            theme::card(ui, &format!("Emitter {i}"), None, |ui| {
                spawn_editor(ui, fx, i, e, is_edited(Address::Geom(i)), out);
                theme::eyebrow(ui, "Particles");
                ui.horizontal_wrapped(|ui| {
                    let flags = e.particle.flags;
                    for (bit, on_text, off_text) in [(1u32, "face their velocity", "face the camera"), (2u32, "frames play as a flipbook", "frames over life")] {
                        let on = flags & bit != 0;
                        if theme::pill(ui, if on { on_text } else { off_text }, on).on_hover_text("Click to switch").clicked() {
                            out.changes.push(Edit::Flags { emitter: i, flags: flags ^ bit });
                        }
                    }
                    if is_edited(Address::Flags(i)) && ui.small_button("Use the shipped flags").clicked() {
                        out.reverts.push(Address::Flags(i));
                    }
                });
                theme::eyebrow(ui, "Colour over life");
                colour_editor(ui, i, e, se.filter(|_| is_edited(Address::Colour(i))), out);
                theme::eyebrow(ui, "Frames");
                frames_editor(ui, c, i, e, is_edited(Address::Frames(i)), out);
                egui::CollapsingHeader::new(theme::disp_text("ATTRIBUTES", theme::style_eyebrow(), theme::DIM)).id_salt(("fx_attrs", i)).default_open(true).show(ui, |ui| {
                    for (d, a) in effect::particle_defs().into_iter().zip(&e.particle.attributes) {
                        let sh = Shipped { attr: se.and_then(|x| x.particle.attributes.iter().find(|y| y.hash == d.hash)), edited: is_edited(Address::Attribute(i, d.hash)) };
                        attr_row(ui, d, a, AttrAt::Particle(i), &sh, out);
                    }
                });
                egui::CollapsingHeader::new(theme::disp_text("TRANSFORM", theme::style_eyebrow(), theme::DIM)).id_salt(("fx_trfm", i)).show(ui, |ui| {
                    for (d, a) in effect::channel_defs().into_iter().zip(&e.channels) {
                        let sh = Shipped { attr: se.and_then(|x| x.channels.iter().find(|y| y.hash == d.hash)), edited: is_edited(Address::Channel(i, d.hash)) };
                        attr_row(ui, d, a, AttrAt::Channel(i), &sh, out);
                    }
                    ui.add_space(4.0);
                    for (r, row) in e.transform.iter().enumerate() {
                        if let Some(v) = floats_row(ui, ("trfm", i, r), &format!("matrix row {r}"), row) {
                            let mut t = e.transform;
                            t[r] = v;
                            out.changes.push(Edit::Transform { emitter: i, transform: t });
                        }
                    }
                    if is_edited(Address::Transform(i)) && ui.small_button("Use the shippedmatrix").clicked() {
                        out.reverts.push(Address::Transform(i));
                    }
                });
            });
        }
        Node::Force(i) => {
            let Some(f) = fx.forces.get(i) else { return };
            let sf = s.shipped.and_then(|x| x.forces.get(i));
            theme::card(ui, &format!("{} {i}", force_kind(&f.kind)), None, |ui| {
                if let Some(p) = force_params(ui, i, &effect::ForceParamsForm::express(&f.kind)) {
                    out.changes.push(Edit::ForceParams { force: i, params: p });
                }
                if is_edited(Address::ForceParams(i)) && ui.small_button("Use the shippedparameters").clicked() {
                    out.reverts.push(Address::ForceParams(i));
                }
                ui.add_space(4.0);
                for (d, a) in effect::force_defs(&f.kind).into_iter().zip(&f.attributes) {
                    let sh = Shipped { attr: sf.and_then(|x| x.attributes.iter().find(|y| y.hash == d.hash)), edited: is_edited(Address::ForceAttribute(i, d.hash)) };
                    attr_row(ui, d, a, AttrAt::Force(i), &sh, out);
                }
            });
        }
        Node::Shape(i) => {
            let Some(sh) = fx.shapes.get(i) else { return };
            theme::card(ui, &format!("Shape {i}"), Some(&format!("{} triangles", sh.records.len())), |ui| {
                ui.label(egui::RichText::new("Each triangle is its vertex P and its edges A and B; its area and normal follow from A \u{d7} B.").small().color(theme::DIM));
                let mut next: Option<Vec<[f32; 13]>> = None;
                for (r, rec) in sh.records.iter().enumerate() {
                    ui.push_id(("rec", r), |ui| {
                        ui.label(egui::RichText::new(format!("triangle {r} \u{b7} {:.3} m\u{b2}", rec[0])).small().color(theme::FAINT));
                        for (label, at) in [("P", 4usize), ("A", 7), ("B", 10)] {
                            if let Some(n) = floats_row(ui, (label, r), label, &[rec[at], rec[at + 1], rec[at + 2]]) {
                                let mut new = *rec;
                                new[at..at + 3].copy_from_slice(&n);
                                derive_triangle(&mut new, rec);
                                let mut all = sh.records.clone();
                                all[r] = new;
                                next = Some(all);
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("Duplicate").clicked() {
                                let mut all = sh.records.clone();
                                all.insert(r + 1, *rec);
                                next = Some(all);
                            }
                            if sh.records.len() > 1 && ui.small_button("Remove").clicked() {
                                let mut all = sh.records.clone();
                                all.remove(r);
                                next = Some(all);
                            }
                        });
                    });
                }
                if let Some(records) = next {
                    out.changes.push(Edit::Shape { shape: i, records });
                }
                if is_edited(Address::Shape(i)) && ui.small_button("Use the shippedtriangles").clicked() {
                    out.reverts.push(Address::Shape(i));
                }
            });
        }
    }
}

/// The shape an emitter spawns on, and how many of its triangles the engine samples.
fn spawn_editor(ui: &mut egui::Ui, fx: &EffectContainer, i: usize, e: &Emitter, edited: bool, out: &mut Out) {
    theme::eyebrow(ui, "Spawns on");
    let mut geom = e.geom.map(|g| g.shape_index as usize);
    let mut shapes: Vec<(Option<usize>, String)> = vec![(None, "no shape: spawns nothing".into())];
    shapes.extend(fx.shapes.iter().enumerate().map(|(n, s)| (Some(n), format!("shape {n} \u{b7} {} triangles", s.records.len()))));
    let opts: Vec<(Option<usize>, &str)> = shapes.iter().map(|(v, t)| (*v, t.as_str())).collect();
    let state = if edited { FieldState::Good } else { FieldState::Neutral };
    if theme::combo_field(ui, "Shape", &mut geom, &opts, state) {
        let g = match geom {
            Some(n) => {
                let word = e.geom.map_or(1, |g| g.word_00 as usize).clamp(1, fx.shapes[n].records.len().max(1));
                effect::GeomForm::Some(effect::GeomRef { shape: n as u16, word: word as u16 })
            }
            None => effect::GeomForm::None("none".into()),
        };
        out.changes.push(Edit::Geom { emitter: i, geom: g });
    }
    if let Some(g) = e.geom {
        let count = fx.shapes.get(g.shape_index as usize).map_or(1, |s| s.records.len()).max(1) as i64;
        let w = g.word_00 as i64;
        ui.horizontal(|ui| {
            theme::field_label(ui, "Triangles sampled");
            let width = (ui.available_width() - 4.0).max(40.0);
            if let Some(v) = held(ui, ("word", i), &w, |ui, v| ui.add_sized([width, 20.0], egui::DragValue::new(v).range(1..=count))) {
                out.changes.push(Edit::Geom { emitter: i, geom: effect::GeomForm::Some(effect::GeomRef { shape: g.shape_index, word: v as u16 }) });
            }
        });
    }
    if edited && ui.small_button("Use the shippedshape").clicked() {
        out.reverts.push(Address::Geom(i));
    }
}

/// A shape record's area and normal from its edges: `|A × B|`, and the unit `A × B` on the side
/// the record's normal was (`effect_container_format.md` §2.1).
pub fn derive_triangle(out: &mut [f32; 13], was: &[f32; 13]) {
    let a = glam::Vec3::new(out[7], out[8], out[9]);
    let b = glam::Vec3::new(out[10], out[11], out[12]);
    let n = a.cross(b);
    out[0] = n.length();
    let old = glam::Vec3::new(was[1], was[2], was[3]);
    let unit = n.normalize_or_zero();
    let unit = if unit.dot(old) < 0.0 { -unit } else { unit };
    out[1..4].copy_from_slice(&unit.to_array());
}

/// A force's parameters, its kind kept.
fn force_params(ui: &mut egui::Ui, i: usize, p: &effect::ForceParamsForm) -> Option<effect::ForceParamsForm> {
    use effect::ForceParamsForm as P;
    let scalar = |ui: &mut egui::Ui, key: &str, v: f32| floats_row(ui, (i, key), key, &[v]).map(|[x]| x);
    let vec3 = |ui: &mut egui::Ui, key: &str, v: &[f32; 3]| floats_row(ui, (i, key), key, v);
    let flag = |ui: &mut egui::Ui, key: &str, v: u32| {
        let x = v as i64;
        let mut out = None;
        ui.horizontal(|ui| {
            theme::field_label(ui, key);
            let w = (ui.available_width() - 4.0).max(40.0);
            out = held(ui, (i, key), &x, |ui, v| ui.add_sized([w, 20.0], egui::DragValue::new(v).range(0..=u32::MAX as i64))).map(|v| v as u32);
        });
        out
    };
    match *p {
        P::Gravity { magnitude, direction } => {
            let (m, d) = (scalar(ui, "magnitude", magnitude), vec3(ui, "direction", &direction));
            (m.is_some() || d.is_some()).then(|| P::Gravity { magnitude: m.unwrap_or(magnitude), direction: d.unwrap_or(direction) })
        }
        P::Drag { magnitude } => scalar(ui, "magnitude", magnitude).map(|m| P::Drag { magnitude: m }),
        P::Wind { magnitude, direction } => {
            let (m, d) = (scalar(ui, "magnitude", magnitude), vec3(ui, "direction", &direction));
            (m.is_some() || d.is_some()).then(|| P::Wind { magnitude: m.unwrap_or(magnitude), direction: d.unwrap_or(direction) })
        }
        P::Attractor { magnitude, flag_13c, param_130, param_134, vector_104 } => {
            let m = scalar(ui, "magnitude", magnitude);
            let f = flag(ui, "flag_13c", flag_13c);
            let (a, b) = (scalar(ui, "param_130", param_130), scalar(ui, "param_134", param_134));
            let v = vec3(ui, "vector_104", &vector_104);
            (m.is_some() || f.is_some() || a.is_some() || b.is_some() || v.is_some()).then(|| P::Attractor {
                magnitude: m.unwrap_or(magnitude),
                flag_13c: f.unwrap_or(flag_13c),
                param_130: a.unwrap_or(param_130),
                param_134: b.unwrap_or(param_134),
                vector_104: v.unwrap_or(vector_104),
            })
        }
        P::Vortex { magnitude, flag_13c, param_130, param_134, param_138, vector_104, vector_110, vector_11c } => {
            let m = scalar(ui, "magnitude", magnitude);
            let f = flag(ui, "flag_13c", flag_13c);
            let (a, b, x) = (scalar(ui, "param_130", param_130), scalar(ui, "param_134", param_134), scalar(ui, "param_138", param_138));
            let (v1, v2, v3) = (vec3(ui, "vector_104", &vector_104), vec3(ui, "vector_110", &vector_110), vec3(ui, "vector_11c", &vector_11c));
            [m.is_some(), f.is_some(), a.is_some(), b.is_some(), x.is_some(), v1.is_some(), v2.is_some(), v3.is_some()].contains(&true).then(|| P::Vortex {
                magnitude: m.unwrap_or(magnitude),
                flag_13c: f.unwrap_or(flag_13c),
                param_130: a.unwrap_or(param_130),
                param_134: b.unwrap_or(param_134),
                param_138: x.unwrap_or(param_138),
                vector_104: v1.unwrap_or(vector_104),
                vector_110: v2.unwrap_or(vector_110),
                vector_11c: v3.unwrap_or(vector_11c),
            })
        }
    }
}

/// The colour over life: the gradient of all 100 keys (the shipped one under it while edited), the
/// selected key's red, green, blue and alpha (`colour_keys`), and a tint that sets every key's red,
/// green and blue (`colour_rgb`).
fn colour_editor(ui: &mut egui::Ui, i: usize, e: &Emitter, shipped: Option<&Emitter>, out: &mut Out) {
    let keys = &e.particle.colr.keys;
    let id = ui.make_persistent_id(("fx_colour_key", i));
    let mut sel: usize = ui.data_mut(|d| d.get_temp(id)).unwrap_or(0usize).min(COLR_KEYS - 1);
    let r = colour_strip(ui, keys, 16.0);
    if let Some(p) = r.interact_pointer_pos().filter(|_| r.clicked()) {
        sel = (((p.x - r.rect.left()) / r.rect.width() * COLR_KEYS as f32) as usize).min(COLR_KEYS - 1);
        ui.data_mut(|d| d.insert_temp(id, sel));
    }
    let x = r.rect.left() + (sel as f32 + 0.5) / COLR_KEYS as f32 * r.rect.width();
    ui.painter().line_segment([egui::pos2(x, r.rect.top()), egui::pos2(x, r.rect.bottom())], egui::Stroke::new(1.5, theme::BRASS));
    if let Some(s) = shipped {
        ui.label(egui::RichText::new("shipped").small().color(theme::FAINT));
        colour_strip(ui, &s.particle.colr.keys, 8.0);
    }
    let blend = if keys[0].half_bits & 0x8000 != 0 { "adds" } else { "alpha blends" };
    ui.label(egui::RichText::new(format!("key {sel} of {COLR_KEYS} \u{b7} the particle {blend}")).small().color(theme::DIM));
    ui.horizontal(|ui| {
        theme::field_label(ui, "This key");
        let k = keys[sel].rgba;
        let now = egui::Color32::from_rgba_unmultiplied(k[0], k[1], k[2], k[3]);
        if let Some(c) = held(ui, ("key_rgba", i, sel), &now, |ui, c| egui::color_picker::color_edit_button_srgba(ui, c, egui::color_picker::Alpha::OnlyBlend)) {
            let mut all: Vec<ColourKeyForm> = keys.iter().map(|k| ColourKeyForm { rgba: k.rgba, half: WordInput::Text(format!("0x{:04X}", k.half_bits)) }).collect();
            all[sel].rgba = c.to_srgba_unmultiplied();
            out.changes.push(Edit::ColourKeys { emitter: i, keys: all });
        }
    });
    ui.horizontal(|ui| {
        theme::field_label(ui, "Tint every key");
        let k = keys[sel].rgba;
        let now = egui::Color32::from_rgb(k[0], k[1], k[2]);
        if let Some(c) = held(ui, ("tint", i), &now, |ui, c| egui::color_picker::color_edit_button_srgba(ui, c, egui::color_picker::Alpha::Opaque)) {
            out.changes.push(Edit::ColourRgb { emitter: i, rgb: [c.r(), c.g(), c.b()] });
        }
    });
    if shipped.is_some() && ui.small_button("Use the shippedcolours").clicked() {
        out.reverts.push(Address::Colour(i));
    }
}

/// The emitter's frames, each removable, and a frame of the `fxdict` or of the item's sprites to add.
fn frames_editor(ui: &mut egui::Ui, c: &Ctx<'_>, i: usize, e: &Emitter, edited: bool, out: &mut Out) {
    let frames = &e.particle.text.frames;
    let as_text = |keys: &[u32]| keys.iter().map(|k| format!("0x{k:08X}")).collect::<Vec<String>>();
    ui.horizontal_wrapped(|ui| {
        for (j, k) in frames.iter().enumerate() {
            ui.push_id(("frame", j), |ui| {
                ui.vertical(|ui| {
                    ui.add(egui::Image::new(&c.thumbs).uv(frame_uv(&c.rects, *k)).fit_to_exact_size(egui::vec2(THUMB, THUMB)))
                        .on_hover_text(frame_name(*k, &c.after.sprites, c.names));
                    if frames.len() > 1 && ui.small_button("Remove").clicked() {
                        let mut n = frames.clone();
                        n.remove(j);
                        out.changes.push(Edit::Frames { emitter: i, frames: as_text(&n) });
                    }
                });
            });
        }
    });
    let mut add: Option<u32> = None;
    egui::ComboBox::from_id_salt(("fx_add_frame", i)).selected_text(egui::RichText::new("+ Add a frame").color(theme::DIM)).width(ui.available_width().min(260.0)).show_ui(ui, |ui| {
        let mut list: Vec<(String, u32)> = c.after.frames.iter().map(|k| (frame_name(*k, &c.after.sprites, c.names), *k)).collect();
        list.sort();
        for (name, k) in list {
            if ui.selectable_label(false, name).clicked() {
                add = Some(k);
            }
        }
    });
    if let Some(k) = add {
        let mut n = frames.clone();
        n.push(k);
        out.changes.push(Edit::Frames { emitter: i, frames: as_text(&n) });
    }
    if edited && ui.small_button("Use the shippedframes").clicked() {
        out.reverts.push(Address::Frames(i));
    }
}

// ─────────────────────────────────────────────────────────────── the add_fx template

/// The key a field is written under in a record: the one the record has (a name or `0xHHHHHHHH`), or
/// the hash written `0xHHHHHHHH`.
fn field_key(record: &BTreeMap<String, mercs2_quartermaster::template::FieldInput>, hash: u32) -> String {
    record.keys().find(|k| name_hash(k) == hash).cloned().unwrap_or_else(|| format!("0x{hash:08X}"))
}

fn field_name(hash: u32, names: Option<&mercs2_quartermaster::NameTable>) -> String {
    names.and_then(|n| n.reverse(hash)).map_or_else(|| format!("0x{hash:08X}"), str::to_string)
}

/// One field of a template record, typed by its class's schema. `Some` with the value it was set to.
fn field_row(
    ui: &mut egui::Ui,
    f: &FieldInfo,
    v: Option<&mercs2_quartermaster::template::FieldInput>,
    names: Option<&mercs2_quartermaster::NameTable>,
) -> Option<mercs2_quartermaster::template::FieldInput> {
    use mercs2_quartermaster::template::FieldInput as I;
    let label = field_name(f.hash, names);
    let mut out = None;
    let Some(v) = v else {
        theme::field_note(ui, FieldState::Bad, &format!("{label} is not declared; every field of the class is"));
        return None;
    };
    let number = |v: &I| match v {
        I::Int(i) => Some(*i as f64),
        I::Float(x) => Some(*x),
        _ => None,
    };
    match f.kind {
        FieldKind::Flag => {
            let on = matches!(v, I::Bool(true));
            ui.horizontal(|ui| {
                theme::field_label(ui, &label);
                if theme::pill(ui, if on { "true" } else { "false" }, on).clicked() {
                    out = Some(I::Bool(!on));
                }
            });
        }
        FieldKind::Count { .. } | FieldKind::Int if number(v).is_some() => {
            let (lo, hi) = match f.kind {
                FieldKind::Count { max } => (0, max),
                _ => (i32::MIN as i64, i32::MAX as i64),
            };
            let x = number(v).unwrap_or_default() as i64;
            ui.horizontal(|ui| {
                theme::field_label(ui, &label);
                let w = (ui.available_width() - 4.0).max(40.0);
                out = held(ui, ("field", f.hash), &x, |ui, x| ui.add_sized([w, 20.0], egui::DragValue::new(x).range(lo..=hi))).map(I::Int);
            });
        }
        FieldKind::F32 if number(v).is_some() => {
            let x = number(v).unwrap_or_default() as f32;
            out = floats_row(ui, ("field", f.hash), &label, &[x]).map(|[x]| I::Float(x as f64));
        }
        FieldKind::Vec3 | FieldKind::Blob32 if matches!(v, I::List(_)) => {
            let I::List(xs) = v else { return None };
            let xs: Vec<f32> = xs.iter().map(|x| *x as f32).collect();
            out = match (f.kind, xs.as_slice()) {
                (FieldKind::Vec3, &[a, b, c]) => floats_row(ui, ("field", f.hash), &label, &[a, b, c]).map(|n| I::List(n.iter().map(|x| *x as f64).collect())),
                (FieldKind::Blob32, s) if s.len() == 8 => {
                    let (a, b) = (floats_row(ui, ("field_lo", f.hash), &label, &[s[0], s[1], s[2], s[3]]), floats_row(ui, ("field_hi", f.hash), "", &[s[4], s[5], s[6], s[7]]));
                    (a.is_some() || b.is_some()).then(|| {
                        let (a, b) = (a.unwrap_or([s[0], s[1], s[2], s[3]]), b.unwrap_or([s[4], s[5], s[6], s[7]]));
                        I::List(a.iter().chain(&b).map(|x| *x as f64).collect())
                    })
                }
                _ => {
                    theme::field_note(ui, FieldState::Bad, &format!("{label} takes {} numbers", if f.kind == FieldKind::Vec3 { 3 } else { 8 }));
                    None
                }
            };
        }
        FieldKind::Hash | FieldKind::Text if matches!(v, I::Text(_)) => {
            let I::Text(s) = v else { return None };
            let id = ui.make_persistent_id(("field_text", f.hash));
            let mut typed: String = ui.data_mut(|d| d.get_temp::<(String, String)>(id)).filter(|(o, _)| o == s).map_or_else(|| s.clone(), |(_, t)| t);
            let r = theme::text_field(ui, &label, &mut typed, "name or 0xHHHHHHHH", FieldState::Neutral);
            // What a hash field names, where the text does not already say it: a name's hash, or the
            // name the table knows for a hash.
            if f.kind == FieldKind::Hash {
                let h = name_hash(&typed);
                let hex = format!("0x{h:08X}");
                let shown = match names.and_then(|n| n.reverse(h)) {
                    Some(n) if !n.eq_ignore_ascii_case(&typed) => Some(format!("{n} \u{b7} {hex}")),
                    _ if !typed.eq_ignore_ascii_case(&hex) => Some(hex),
                    _ => None,
                };
                if let Some(shown) = shown {
                    ui.label(egui::RichText::new(shown).small().color(theme::FAINT));
                }
            }
            if r.lost_focus() && typed != *s && !typed.is_empty() {
                ui.data_mut(|d| d.remove::<(String, String)>(id));
                out = Some(I::Text(typed));
            } else {
                ui.data_mut(|d| d.insert_temp(id, (s.clone(), typed)));
            }
        }
        _ => theme::field_note(ui, FieldState::Bad, &format!("{label} cannot take {v:?}")),
    }
    out
}

/// An `add_fx`'s template, each component a card of typed fields from its class's schema in the
/// game's worldentity: started from a shipped template, its components added from a shipped
/// template's record of the class or removed. Returns whether it changed.
pub fn template_editor(
    ui: &mut egui::Ui,
    t: &mut mercs2_quartermaster::template::TemplateForm,
    effect: &str,
    classes: &Classes,
    names: Option<&mercs2_quartermaster::NameTable>,
) -> bool {
    use mercs2_quartermaster::template::{FieldInput, Records, TemplateForm};
    let mut commit = false;
    // Start from a shipped template: its components, its RedEffectComponent naming this effect.
    let id = ui.make_persistent_id("fx_template_from");
    let mut typed: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    theme::text_field(ui, "Start from template", &mut typed, "global_particle_explosion_c4", FieldState::Neutral);
    ui.data_mut(|d| d.insert_temp(id, typed.clone()));
    if ui.add_enabled(!typed.trim().is_empty(), egui::Button::new("Use this template's components")).clicked() {
        let expressed = fx::template_keys(&classes.we, typed.trim())
            .and_then(|keys| keys.first().copied().ok_or_else(|| format!("the game has no template {}", typed.trim())))
            .and_then(|k| TemplateForm::express(&classes.we, k));
        match expressed {
            Ok(e) => {
                t.components = e.components;
                commit = true;
                // A template with one RedEffectComponent names this effect in it; one with none or
                // several keeps what it has, and M0255 below says so.
                let mut set = mercs2_quartermaster::template::FieldEdits::new();
                set.entry(fx::RED_EFFECT_CLASS.to_string()).or_default().insert("name".into(), FieldInput::Text(effect.to_string()));
                let _ = t.edit(&set);
            }
            Err(e) => theme::field_note(ui, FieldState::Bad, &e),
        }
    }
    let reds = match t.components.get(fx::RED_EFFECT_CLASS) {
        Some(Records::One(_)) => 1,
        Some(Records::Many(v)) => v.len(),
        None => 0,
    };
    if reds != 1 {
        theme::field_note(ui, FieldState::Bad, &format!("M0255 \u{2014} the template has {reds} RedEffectComponent records; it has one"));
    }
    if let Some(Records::One(r)) = t.components.get(fx::RED_EFFECT_CLASS) {
        let key = field_key(r, fx::RED_EFFECT_NAME_FIELD);
        let starts = r.get(&key).and_then(|v| match v {
            FieldInput::Text(s) => Some(name_hash(s)),
            _ => None,
        });
        if starts != Some(m2(effect)) {
            let shown = starts.map_or_else(|| "nothing".to_string(), |h| field_name(h, names));
            theme::field_note(ui, FieldState::Bad, &format!("it starts {shown}, not this item's effect {effect}"));
            if ui.small_button("Start this item's effect").clicked() {
                if let Some(Records::One(r)) = t.components.get_mut(fx::RED_EFFECT_CLASS) {
                    r.insert(key, FieldInput::Text(effect.to_string()));
                    commit = true;
                }
            }
        }
    }
    let mut remove: Option<String> = None;
    for (class, records) in t.components.iter_mut() {
        ui.push_id(("class", class.clone()), |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                theme::eyebrow(ui, class);
                if ui.small_button("Remove").clicked() {
                    remove = Some(class.clone());
                }
            });
            let Some(info) = classes.classes.get(class) else {
                theme::field_note(ui, FieldState::Bad, &format!("{class} is not a class of the game's worldentity"));
                return;
            };
            let maps: Vec<&mut BTreeMap<String, FieldInput>> = match records {
                Records::One(m) => vec![m],
                Records::Many(v) => v.iter_mut().collect(),
            };
            let many = maps.len() > 1;
            for (n, m) in maps.into_iter().enumerate() {
                ui.push_id(n, |ui| {
                    if many {
                        ui.label(egui::RichText::new(format!("record {n}")).small().color(theme::FAINT));
                    }
                    for f in &info.fields {
                        let key = field_key(m, f.hash);
                        if let Some(v) = field_row(ui, f, m.get(&key), names) {
                            m.insert(key, v);
                            commit = true;
                        }
                    }
                    let unknown: Vec<String> = m.keys().filter(|k| !info.fields.iter().any(|f| f.hash == name_hash(k))).cloned().collect();
                    for k in unknown {
                        ui.horizontal(|ui| {
                            theme::field_note(ui, FieldState::Bad, &format!("{k} is not a field of {class}"));
                            if ui.small_button("Remove").clicked() {
                                m.remove(&k);
                                commit = true;
                            }
                        });
                    }
                });
            }
        });
    }
    if let Some(class) = remove {
        t.components.remove(&class);
        commit = true;
    }
    // A component the template does not carry yet, from a shipped template's record of its class.
    let mut add: Option<String> = None;
    egui::ComboBox::from_id_salt("fx_add_component").selected_text(egui::RichText::new("+ Add a component").color(theme::DIM)).width(ui.available_width().min(280.0)).show_ui(ui, |ui| {
        for (class, info) in &classes.classes {
            if t.components.contains_key(class) || info.seed.is_none() {
                continue;
            }
            if ui.selectable_label(false, class).on_hover_text(format!("values from {}", info.seed.as_ref().map_or("", |(n, _)| n.as_str()))).clicked() {
                add = Some(class.clone());
            }
        }
    });
    if let Some(class) = add {
        if let Some((_, seed)) = classes.classes.get(&class).and_then(|i| i.seed.clone()) {
            t.components.insert(class, Records::One(seed));
            commit = true;
        }
    }
    commit
}

/// The inspector's card: each block the build ships of the item's effects, and the merge's log.
pub fn after_card(ui: &mut egui::Ui, f: &Fx, game: &[PathBuf], item: &Item) {
    let Some(Ok(a)) = f.after(game, item) else { return };
    theme::card(ui, "Effect build", Some("qm build"), |ui| {
        for (path, what) in &a.ships {
            theme::kv(ui, "block", egui::RichText::new(path).monospace());
            ui.label(egui::RichText::new(what).monospace().small().color(theme::DIM));
        }
        for line in &a.log {
            ui.label(egui::RichText::new(line).small().color(theme::FAINT));
        }
    });
}

/// Over the viewport while an effect item is open and nothing runs: why.
pub fn viewport_note(ctx: &egui::Context, f: &Fx, game: &[PathBuf], item: &Item, rect: egui::Rect) {
    if !of_item(item) || f.want(game, item).is_some() {
        return;
    }
    let text = match f.after(game, item) {
        None => "Merging over the game's effects\u{2026}".to_string(),
        Some(Err(e)) => e,
        Some(Ok(a)) => a.views.values().flat_map(|v| v.problems.iter().cloned()).next().unwrap_or_else(|| "The build ships no effect to run.".into()),
    };
    egui::Area::new(egui::Id::unique("depot_fx_note")).fixed_pos(rect.left_top() + egui::vec2(12.0, 12.0)).show(ctx, |ui| {
        egui::Frame::NONE.fill(theme::G1).stroke(egui::Stroke::new(1.0, theme::LINE)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_max_width(rect.width() - 24.0);
            ui.label(egui::RichText::new(text).color(theme::DIM));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr(emitter: usize, name: &str, v: f32) -> Edit {
        Edit::Attribute { emitter, attribute: name.into(), value: Some(ValueInput::Float(v)), curve: None, options: None }
    }

    /// An edit of a field an earlier edit sets folds into it; nothing else is touched.
    #[test]
    fn an_edit_of_the_same_field_folds_into_the_earlier_one() {
        let mut edits = vec![attr(0, "size", 1.0), attr(1, "size", 2.0)];
        let j = upsert(&mut edits, Edit::Attribute { emitter: 0, attribute: "size".into(), value: None, curve: Some(CurveForm::Keyword("none".into())), options: None });
        assert_eq!(j, 0);
        assert_eq!(edits.len(), 2);
        let Edit::Attribute { value, curve, .. } = &edits[0] else { panic!("{:?}", edits[0]) };
        assert_eq!(value, &Some(ValueInput::Float(1.0)), "the value it had stays");
        assert_eq!(curve, &Some(CurveForm::Keyword("none".into())));
    }

    /// After an operation that renumbers the emitters, an edit addresses the new numbering: it is
    /// appended, never folded into an edit made under the old one.
    #[test]
    fn an_edit_after_a_removed_emitter_is_appended() {
        let mut edits = vec![attr(1, "size", 1.0), Edit::RemoveEmitter { emitter: 0 }];
        assert!(!revertible(&edits, Address::Attribute(1, m2("size"))), "the removal renumbers it");
        let j = upsert(&mut edits, attr(1, "size", 3.0));
        assert_eq!((j, edits.len()), (2, 3));
        assert!(revertible(&edits, Address::Attribute(1, m2("size"))));
    }

    /// "Use shipped" takes back only what nothing renumbers since.
    #[test]
    fn use_shipped_leaves_an_edit_a_later_removal_depends_on() {
        let mut edits = vec![attr(0, "size", 1.0), Edit::RemoveEmitter { emitter: 1 }, attr(0, "life", 2.0)];
        revert(&mut edits, Address::Attribute(0, m2("size")));
        assert_eq!(edits.len(), 3, "the size edit precedes the removal");
        revert(&mut edits, Address::Attribute(0, m2("life")));
        assert_eq!(edits.len(), 2);
        assert_eq!(edited(&edits), HashSet::new());
    }

    /// A tint after key edits stays its own operation, so the keys it tints are kept; a second tint
    /// folds into the first.
    #[test]
    fn a_tint_after_key_edits_is_its_own_operation() {
        let keys = vec![ColourKeyForm { rgba: [1, 2, 3, 4], half: WordInput::Int(0) }; COLR_KEYS];
        let mut edits = vec![Edit::ColourKeys { emitter: 0, keys }];
        upsert(&mut edits, Edit::ColourRgb { emitter: 0, rgb: [9, 9, 9] });
        assert_eq!(edits.len(), 2);
        upsert(&mut edits, Edit::ColourRgb { emitter: 0, rgb: [8, 8, 8] });
        assert_eq!(edits.len(), 2);
        assert!(matches!(edits[1], Edit::ColourRgb { rgb: [8, 8, 8], .. }));
    }

    /// A triangle's area and normal follow its edges, on the side its normal was.
    #[test]
    fn a_moved_edge_rederives_the_area_and_keeps_the_normal_side() {
        let was = [1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let mut out = was;
        out[7..10].copy_from_slice(&[2.0, 0.0, 0.0]);
        derive_triangle(&mut out, &was);
        assert_eq!(out[0], 2.0);
        assert_eq!(&out[1..4], &[0.0, -1.0, 0.0], "A \u{d7} B is +Y here; the record's normal was \u{2212}Y");
    }

    /// A file that reads is edited however it is written: a hand-written edits file keeps its
    /// comment through an added edit and a "use shipped".
    #[test]
    fn a_hand_written_edits_file_is_edited_in_place() {
        let p = Path::new("src/edits.yaml");
        let text = fx::edits_to_string(&EditsForm { edits: vec![attr(0, "size", 1.5)] }, Format::Yaml).unwrap();
        assert!(matches!(read_edits(p, &text), Doc::Edits { .. }));
        assert!(matches!(read_edits(p, "edits: [oops"), Doc::Bad(_)));
        let hand = format!("# my comment\n{text}");
        let doc = read_edits(p, &hand);
        let Doc::Edits { prior, format, edits } = &doc else { panic!("a hand-written edits file reads") };
        let both = EditsForm { edits: vec![edits[0].clone(), attr(0, "life", 2.0)] };
        let added = edits_over(prior, *format, &both).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(added, format!("# my comment\n{}", fx::edits_to_string(&both, Format::Yaml).unwrap()));
        let back = reverted_text(&doc, Address::Attribute(0, m2("size"))).unwrap_or_else(|e| panic!("{e}"));
        assert!(back.starts_with("# my comment\n"), "{back}");
        assert!(fx::edits_from_str(&back, Format::Yaml).unwrap().edits.is_empty(), "{back}");
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;

    fn vz() -> PathBuf {
        let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"))
    }

    fn item(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("m2ws_depotfx_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        d
    }

    const C4: u32 = 0x41B4_326E;

    /// A tint of the C4 explosion's first emitter, made through the editor's own path: the built
    /// effect carries the new red, green and blue in every key with the alphas kept, the other
    /// emitters are the game's, and a tint back to the shipped colours takes the edit out.
    #[test]
    fn a_tint_reaches_the_built_effect() {
        let dir = item("tint");
        let path = dir.join("src/edits.yaml");
        let c = Contribution::ReplaceFx { target: FxTarget::Effect { effect: "global_explosion_c4".into() }, edits: "src/edits.yaml".into() };
        let first = after(&[vz()], &dir, std::slice::from_ref(&c)).unwrap_or_else(|e| panic!("{e}"));
        let shipped = first.views[&0].shipped.clone().expect("the game's C4");
        let text = edited_text(&Doc::Missing, &path, Some(&shipped), Edit::ColourRgb { emitter: 0, rgb: [0, 255, 64] }).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&path, &text).unwrap();
        let a = after(&[vz()], &dir, &[c]).unwrap_or_else(|e| panic!("{e}"));
        let v = &a.views[&0];
        assert!(v.problems.is_empty(), "{:?}", v.problems);
        let built = v.built.as_ref().unwrap();
        for (s, b) in shipped.emitters[0].particle.colr.keys.iter().zip(&built.emitters[0].particle.colr.keys) {
            assert_eq!(&b.rgba[..3], &[0, 255, 64]);
            assert_eq!(b.rgba[3], s.rgba[3]);
        }
        assert_eq!(shipped.emitters[1..], built.emitters[1..], "the other emitters are the game's");
        assert!(!v.shipped_started_by.is_empty(), "templates start the C4");
        let doc = read_edits(&path, &text);
        let k = shipped.emitters[0].particle.colr.keys[0].rgba;
        let back = edited_text(&doc, &path, Some(&shipped), Edit::ColourRgb { emitter: 0, rgb: [k[0], k[1], k[2]] }).unwrap();
        let uniform = shipped.emitters[0].particle.colr.keys.iter().all(|x| x.rgba[..3] == k[..3]);
        println!("[depotfx] C4: {} emitters, {} templates; the first emitter's keys are one colour: {uniform}", built.emitters.len(), v.started_by.len());
        if uniform {
            assert!(fx::edits_from_str(&back, Format::Yaml).unwrap().edits.is_empty(), "a tint back to the shipped colour takes itself out");
        }
    }

    /// An `add_fx` started from a shipped effect builds to that effect under its own hash, and runs
    /// under its own template.
    #[test]
    fn an_effect_started_from_a_shipped_one_builds_to_it() {
        let dir = item("start");
        let path = dir.join("src/effect.yaml");
        std::fs::write(&path, start_text(&[vz()], "global_explosion_c4", &path).unwrap_or_else(|e| panic!("{e}"))).unwrap();
        assert!(matches!(read_form(&path, &std::fs::read_to_string(&path).unwrap()), Doc::Form { .. }), "the form the editor wrote is editable");
        let mut game = GameStack::open(&[vz()]).unwrap();
        let base = fx::GameFx::read(&mut game).unwrap();
        let names = template_names(&base.worldentity).unwrap();
        let starters = crate::vfxview::starters(&base.worldentity, &names).unwrap();
        let t = &starters[&C4][0].template;
        let key = fx::template_keys(&base.worldentity, t).unwrap()[0];
        let mut template = mercs2_quartermaster::template::TemplateForm::express(&base.worldentity, key).unwrap();
        template.name = "depot_test_c4_copy".into();
        let mut set = mercs2_quartermaster::template::FieldEdits::new();
        set.entry(fx::RED_EFFECT_CLASS.to_string()).or_default().insert("name".into(), mercs2_quartermaster::template::FieldInput::Text("depot_test_fx".into()));
        template.edit(&set).unwrap();
        let c = Contribution::AddFx { name: "depot_test_fx".into(), effect: "src/effect.yaml".into(), template };
        // The item as the Depot writes it, read back whole.
        let file = crate::depot::ItemFile { state: crate::depot::State::Draft, contributions: vec![c.clone()] };
        crate::depot::write_item(&dir, &file).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(crate::depot::read_item(&dir).unwrap_or_else(|e| panic!("{e}")).contributions, vec![c.clone()]);
        let a = after(&[vz()], &dir, &[c]).unwrap_or_else(|e| panic!("{e}"));
        let v = &a.views[&0];
        assert!(v.problems.is_empty(), "{:?}", v.problems);
        assert_eq!(v.hash, m2("depot_test_fx"));
        let shipped = effect_of(&base.effects.entries, C4).unwrap().unwrap();
        assert_eq!(v.built.as_deref(), Some(&shipped));
        assert_eq!(v.started_by.iter().map(|s| s.template.as_str()).collect::<Vec<_>>(), ["depot_test_c4_copy"]);
        assert!(a.classes.is_some(), "an add_fx item reads the template classes");
    }

    /// Every class a template may carry reads its fields, and each component "Add a component"
    /// starts from (a shipped template's record of its class) types against the class as qm lowers
    /// it.
    #[test]
    fn every_seeded_component_lowers() {
        use mercs2_quartermaster::template::{Records, TemplateForm};
        let mut game = GameStack::open(&[vz()]).unwrap();
        let base = fx::GameFx::read(&mut game).unwrap();
        let c = read_classes(base.worldentity).unwrap_or_else(|e| panic!("{e}"));
        let red = &c.classes[fx::RED_EFFECT_CLASS];
        assert!(red.fields.iter().any(|f| f.hash == fx::RED_EFFECT_NAME_FIELD && f.kind == FieldKind::Hash));
        let mut seeded = 0;
        for (class, info) in &c.classes {
            let Some((from, record)) = &info.seed else { continue };
            let form = TemplateForm { name: "depot_test_seed".into(), name_flag: 1, components: BTreeMap::from([(class.clone(), Records::One(record.clone()))]) };
            form.lower(&c.we, 0x8000_0001).unwrap_or_else(|e| panic!("{class} from {from}: {e}"));
            assert_eq!(record.len(), info.fields.len(), "{class}: the seed declares every field");
            seeded += 1;
        }
        println!("[depotfx] {} template classes, {seeded} with a shipped record to start from", c.classes.len());
        assert!(seeded > 0);
    }
}
